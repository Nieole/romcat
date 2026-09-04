//! **在线档**：唯一一处让刮削发网络请求的地方，以及守着配额的那道闸。
//!
//! 离线源补不上简介、类型、开发商，**也补不上封面**——本地数据源里根本没有图片
//! （ADR-0007）。在线档就是为这些字段存在的。
//!
//! ## 这一档对这个库有结构性风险
//!
//! ScreenScraper 有一个**专门的** 431「当日未识别 ROM 配额超限」错误码，而**配额同时按
//! 账号与 IP 计**，多账号轮换的处置是**永久封禁**（注册页原文：“Do not create additional
//! account(s) when you have exceeded your daily scrape quota… you will be banned for life…
//! per day for the same account **AND/OR the same IP address**”，调研 §1.1）。
//!
//! 而这个库在 ScreenScraper 眼里正是一片「未识别 ROM」：票 07 的真机命中率按平台拆开是
//! 两个世界——GB 91.5%、FC 82.8%，而 GBA 5.8%、NDS 4.4%、GBC 0%。库里上万个变体撞过去
//! 就是上万次「未找到」，足以在一天之内撞穿那个**专门用来惩罚「乱扔文件」**的配额。
//!
//! 于是这一档的三条纪律，每一条都是代码里的一道闸，不是注释：
//!
//! | 纪律 | 落在哪 |
//! |---|---|
//! | **只对已确认的条目发请求** | [`ScreenScraper::probe`]：没有自动通过的候选就整条无话可说，一个字节都不发 |
//! | **默认限流，并发与频率有明确上限** | [`MAX_CONCURRENCY`]（恒为 1）、[`Limits::interval`]、[`Limits::budget`] |
//! | **配额超限是硬停止，不重试、不换账号** | [`classify`] 把 430 / 431 判成 [`Verdict::Quota`]，[`Net`] 记下 [`Halt`] 之后**再也不发第二个请求** |
//!
//! ## 「查过、没有」也要记着
//!
//! 431 惩罚的是**未识别**请求，所以**重问一次没有的东西要额外扣一份独立配额**
//! （调研 13.3(9)）。404 因此不是失败，是一条**结论**：它照样写进采集记录
//! （`scrape_probe` 的 `vals = 0`），下一趟整条跳过。
//!
//! ## 凭据只有一套，而且不由这个模块去申请
//!
//! ScreenScraper 的 `devid` 要在论坛人工审批（调研 §1.1，实测无 devid 直接 403）。
//! [`Credentials`] 只从环境变量读**一套**，读不到就整档不启动——**绝不匿名试探**，
//! 也**没有任何一处能装第二套凭据**：轮换是被永久封禁的那条路，代码里不留这个口子。
//!
//! ## 服务端说的数字比我们猜的准
//!
//! 三份官方文档给出三个不同的日配额（Recalbox 15,000 / Batocera 50,000 /
//! Skyscraper 20,000，调研 §11.4），所以**一个都不写死**：[`Server`] 从每次响应里读
//! `maxrequestsperday` / `requeststoday` / `maxrequestskoperday` / `requestskotoday`，
//! 读到 0 就停。[`Limits::budget`] 是另一回事——它是**我们自己设的保守闸**，
//! 在服务端说话之前就先把这一趟框住。

use std::cell::RefCell;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::dat::fetch::{FetchError, Fetcher};
use crate::dat::guard;
use crate::scan::CancelToken;

use super::{
    AnchorKind, Basis, Failure, Field, Harvest, Locality, MediaClaim, MediaFrom, MediaKind, Source,
    Subject,
};

/// 这一发问的是什么。
///
/// 分开是因为 **431 只惩罚「未识别 ROM」的查询**：一张图 404 说的是「这份媒体没有」，
/// 与「这个 ROM 我不认识」完全不是一回事。混着数，报告里那一栏就说不出配额的实情。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// 一次条目查询（`jeuInfos.php`）。它的「没找到」才算进「未识别 ROM」。
    Query,
    /// 下一份媒体。
    Media,
}

/// 这个源在优先级表与**依据**里叫什么。
pub const SCREEN_SCRAPER: &str = "ScreenScraper";

/// 在线档的**并发上限**。
///
/// **恒为 1，而且不给调。** 刮削引擎本来就是一条顺序的循环，这个常量是把那件事
/// 说出来：ES-DE 选择完全串行，而 ScreenScraper 的 429 正是「线程数超了」
/// （调研 §11.5、§1.3）。多开线程省下的那点时间，换不来把账号连 IP 一起赌进去。
pub const MAX_CONCURRENCY: usize = 1;

/// 两次在线请求之间至少隔多久。
pub const DEFAULT_INTERVAL: Duration = Duration::from_millis(1_000);

/// 一趟最多发多少个在线请求。
///
/// 这是**我们自己设的保守闸**，不是服务端的配额（那个从响应里读，见 [`Server`]）。
/// 默认往小里设：真库 9,226 个作品锚点，第一次就放开跑等于拿账号做实验。
pub const DEFAULT_BUDGET: u64 = 200;

/// 429（线程数超了）最多退避几次。**退避是给 429 的，配额超限一次都不退。**
pub const MAX_BACKOFF_TRIES: u32 = 3;

/// 429 之后等多久再试。Recalbox 的实现是 5 秒（调研 §11.4）。
pub const BACKOFF: Duration = Duration::from_secs(5);

/// 连着多少次网络错就认定网断了、别再耗下去。
pub const MAX_CONSECUTIVE_FAILURES: u32 = 5;

/// `jeuInfos.php` 的落点。
const GAME_INFO: &str = "https://api.screenscraper.fr/api2/jeuInfos.php";

/// 一个状态码该怎么处置。
///
/// **这张表是这一档最要紧的东西**，所以它是一个纯函数：不联网也断言得出「431 是停不是
/// 重试」。映射照 Recalbox 的实现抄（调研 §11.4），那是同一个 API 上跑了多年的一份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// 正常。
    Ok,
    /// **这条没有。** 记成「查过、没有」，下一趟整条跳过——重问一次要额外扣一份
    /// 「未识别 ROM」配额，而那份配额撞穿就是 431。
    NotFound,
    /// 线程数超了。可恢复：退避后重试，**次数有限**。
    Backoff,
    /// **硬停止**：当日配额用完了。不重试、不换账号。
    Quota(&'static str),
    /// **硬停止**：凭据不对、软件被拉黑、服务关着。
    Fatal(&'static str),
}

/// 一个 HTTP 状态码该怎么处置（调研 §1.3 的官方错误码表 + §11.4 的处置）。
#[must_use]
pub fn classify(status: u16) -> Verdict {
    match status {
        200..=299 => Verdict::Ok,
        404 => Verdict::NotFound,
        429 => Verdict::Backoff,
        430 => Verdict::Quota("当日 scrape 配额超限（430）"),
        431 => Verdict::Quota(
            "当日「未识别 ROM」配额超限（431）——\
             这个库里大量变体在 ScreenScraper 眼里就是未识别 ROM",
        ),
        400 => Verdict::Fatal("请求参数不对（400）：缺参、rom 名里带了路径、或哈希格式错"),
        401 => Verdict::Fatal("服务器饱和，对非会员关闭（401）"),
        403 => Verdict::Fatal("开发者凭据不对（403）：devid / devpassword 要在论坛申请"),
        423 => Verdict::Fatal("API 整个关着（423）"),
        426 => Verdict::Fatal("这个刮削软件被拉黑了（426）：必须换版本，不许继续打"),
        // 5xx 是服务端自己的毛病，退避几次是合理的。
        500..=599 => Verdict::Backoff,
        // **没见过的码一律停。** 这一档的代价不对称：猜错方向继续打，赌上的是账号与 IP。
        _ => Verdict::Fatal("没见过的状态码——停下来问人比接着试安全"),
    }
}

/// 正文开头那一段，拿去找配额话术用。
///
/// **只看开头**：这条路上走的不只有 JSON，还有一张张封面——把一份 2 MiB 的 PNG 整个
/// 转成字符串再去找一句法语，每下一张图就白烧一遍那么多内存。配额那句话在正文最前面。
fn head_of(body: &[u8]) -> std::borrow::Cow<'_, str> {
    const HEAD: usize = 4 * 1024;
    String::from_utf8_lossy(&body[..body.len().min(HEAD)])
}

/// 正文里有没有配额超限的话。
///
/// 有些实现见过「200 里带着一句配额已满」的情形（Skyscraper 就是扫响应文本认出来的，
/// 调研 §1.3）。状态码那一路已经盖住绝大多数，这一条是兜底：**宁可多停一次**。
#[must_use]
pub fn quota_phrase(body: &str) -> Option<&'static str> {
    const PHRASES: &[(&str, &str)] = &[
        ("Votre quota de scrape est", "正文说当日 scrape 配额已满"),
        (
            "Faite du tri dans vos fichiers roms",
            "正文说当日「未识别 ROM」配额已满",
        ),
    ];
    PHRASES
        .iter()
        .find(|(needle, _)| body.contains(needle))
        .map(|(_, why)| *why)
}

/// 整趟到此为止的理由。
///
/// **它不是错误**：已经采完的那部分留在中立库里，重跑接着采。把它做成错误，
/// 一趟跑到 80% 撞上配额就会把那 80% 一起丢掉——而配额是每天才回一次的东西。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Halt {
    /// 当日配额超限。**不重试、不换账号**：轮换的处置是永久封禁。
    Quota {
        /// 哪个源。
        source: String,
        /// 哪一条配额。
        why: String,
    },
    /// 凭据不对、被拉黑、服务关着。
    Fatal {
        /// 哪个源。
        source: String,
        /// 服务器说了什么。
        why: String,
    },
    /// 退避到头仍然超线程数。
    Throttled {
        /// 哪个源。
        source: String,
        /// 退避了几次。
        tries: u32,
    },
    /// **自己设的**这一趟请求上限用完了。与配额超限不是一回事——它是我们主动收的手。
    Budget {
        /// 哪个源。
        source: String,
        /// 用掉几个。
        spent: u64,
        /// 上限是几个。
        cap: u64,
    },
    /// 连着好几次网络错，网大概断了。
    Offline {
        /// 哪个源。
        source: String,
        /// 连着错了几次。
        tries: u32,
    },
}

impl Halt {
    /// 打给用户的那一句。
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Quota { source, why } => format!(
                "{source} 的{why}。**这一趟到此为止，不重试也不换账号**——\
                 配额同时按账号与 IP 计，轮换账号的处置是永久封禁。明天再来。"
            ),
            Self::Fatal { source, why } => {
                format!("{source} 停了：{why}。已经采到的那部分留在中立库里。")
            }
            Self::Throttled { source, tries } => format!(
                "{source} 退避 {tries} 次仍然超线程数（429），停下。\
                 把 --online-interval-ms 调大一点再来。"
            ),
            Self::Budget { source, spent, cap } => format!(
                "{source} 这一趟自设的请求上限用完了（{spent}/{cap}）。\
                 这不是服务端的配额，是我们主动收的手——想多采就调大 --online-budget。"
            ),
            Self::Offline { source, tries } => {
                format!("{source} 连着 {tries} 次都没连上，当网断了处理。重跑会接着采。")
            }
        }
    }
}

/// 服务端自己说的配额剩余。**一个数字都不写死**（调研 13.3(9)）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Server {
    /// 今天还能发几个请求；服务端没说就是 `None`。
    pub requests_left: Option<u64>,
    /// 今天还能发几个**未识别**请求。431 盯的就是这个。
    pub ko_left: Option<u64>,
    /// 服务端允许这个账号开几个线程。**只用来报告**——这一档恒为串行。
    pub max_threads: Option<u64>,
}

/// 在线档的限流参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// 两次请求之间至少隔多久。
    pub interval: Duration,
    /// 这一趟自己设的请求上限。
    pub budget: u64,
    /// 撞上 429（线程数超了）之后等多久再试。
    ///
    /// 它可调**只为了让退避这条路测得动**：默认 5 秒，而一条要睡 15 秒才走得完的
    /// 分支等于没有测试——偏偏它就在配额逻辑正中间。
    pub backoff: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            interval: DEFAULT_INTERVAL,
            budget: DEFAULT_BUDGET,
            backoff: BACKOFF,
        }
    }
}

/// ScreenScraper 的一套凭据。**一套，不多不少。**
///
/// `devid` / `devpassword` 是**开发者**凭据，要在论坛人工申请（调研 §1.1）；
/// `ssid` / `sspassword` 是**终端用户**的站点账号，可以不给。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    /// 开发者标识。
    pub dev_id: String,
    /// 开发者密码。
    pub dev_password: String,
    /// 调用方软件名。会被服务端记录，也会被拉黑（426）。
    pub soft_name: String,
    /// 终端用户账号。
    pub user: Option<String>,
    /// 终端用户密码。
    pub user_password: Option<String>,
}

/// 凭据从哪几个环境变量读。
pub const ENV_KEYS: [&str; 4] = [
    "SCREENSCRAPER_DEVID",
    "SCREENSCRAPER_DEVPASSWORD",
    "SCREENSCRAPER_SSID",
    "SCREENSCRAPER_SSPASSWORD",
];

impl Credentials {
    /// 从环境变量读那**一套**凭据；开发者那两样缺一样就返回 `None`。
    ///
    /// **不提供「读第二套」的办法**，这是有意的：多账号轮换绕配额的处置是永久封禁
    /// （调研 §1.1），代码里不留这个口子比写一句注释管用。
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let get = |key: &str| std::env::var(key).ok().filter(|value| !value.is_empty());
        Some(Self {
            dev_id: get(ENV_KEYS[0])?,
            dev_password: get(ENV_KEYS[1])?,
            soft_name: concat!("romcat", env!("CARGO_PKG_VERSION")).to_string(),
            user: get(ENV_KEYS[2]),
            user_password: get(ENV_KEYS[3]),
        })
    }

    /// 拿一份**判据**造一条条目查询的 URL。
    ///
    /// 造 URL 归凭据管，是因为这条 URL 一半是身份、一半是判据，而身份只在这里。
    /// **哈希与文件大小必须同发**：官方原文「除非获得豁免，你必须发送 crc/md5/sha1
    /// 之一**以及**文件字节大小」（调研 §1.4）。少一样这次查询注定未命中，
    /// 而未命中要扣的正是 431 那份配额。
    #[must_use]
    pub fn game_info_url(&self, basis: &Basis) -> String {
        let mut out = format!(
            "{GAME_INFO}?devid={}&devpassword={}&softname={}&output=json",
            encode(&self.dev_id),
            encode(&self.dev_password),
            encode(&self.soft_name),
        );
        if let Some(user) = &self.user {
            out.push_str(&format!("&ssid={}", encode(user)));
        }
        if let Some(password) = &self.user_password {
            out.push_str(&format!("&sspassword={}", encode(password)));
        }
        out.push_str(&format!(
            "&romtype=rom&crc={:08X}&romtaille={}&romnom={}",
            basis.crc32,
            basis.bytes,
            encode(&basis.rom_name),
        ));
        out
    }
}

/// 一趟在线刮削攒下来的数。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    /// 一共发了几个请求（**查询与下媒体都算**）。
    pub requests: u64,
    /// 其中几个是「查过、没有」。431 盯的就是这一类。
    pub not_found: u64,
    /// 几次因为闸门拦下而没发出去。
    pub refused: u64,
    /// 服务端自己说的剩余。
    pub server: Server,
}

#[derive(Debug, Default)]
struct NetState {
    spent: u64,
    not_found: u64,
    refused: u64,
    last: Option<Instant>,
    server: Server,
    fails: u32,
    halted: Option<Halt>,
}

/// **网络句柄**：在线源要发请求，只能经过它。
///
/// 限流、配额、硬停止都收在这一处，而不是散在每个源里。一个源自己去 `fetcher.get`
/// 就绕开了全部三道闸——所以源拿不到 `fetcher`，只拿得到这个。
pub struct Net<'a> {
    fetcher: &'a dyn Fetcher,
    limits: Limits,
    credentials: Credentials,
    cancel: &'a CancelToken,
    state: RefCell<NetState>,
}

impl<'a> Net<'a> {
    /// 拿一个取数接缝、一组限流参数与**那一套**凭据起一个。
    ///
    /// 凭据装在这里而不是装在源里，是因为「这一趟对外的一切」都归它管：限流、配额、
    /// 硬停止、身份。**一趟只有一个 `Net`，于是一趟只有一套凭据**——轮换在类型上就
    /// 做不出来，而轮换的处置是永久封禁。
    #[must_use]
    pub fn new(
        fetcher: &'a dyn Fetcher,
        limits: Limits,
        credentials: Credentials,
        cancel: &'a CancelToken,
    ) -> Self {
        Self {
            fetcher,
            limits,
            credentials,
            cancel,
            state: RefCell::new(NetState::default()),
        }
    }

    /// 这一趟用的那套凭据。**一趟一套**——源要造查询 URL 只能问它。
    #[must_use]
    pub fn credentials(&self) -> &Credentials {
        &self.credentials
    }

    /// 这一趟用掉了多少。
    #[must_use]
    pub fn usage(&self) -> Usage {
        let state = self.state.borrow();
        Usage {
            requests: state.spent,
            not_found: state.not_found,
            refused: state.refused,
            server: state.server,
        }
    }

    /// 这一趟停了没有，为什么。
    #[must_use]
    pub fn halted(&self) -> Option<Halt> {
        self.state.borrow().halted.clone()
    }

    /// 记一次被闸门拦下的 URL。
    pub fn refused(&self) {
        self.state.borrow_mut().refused += 1;
    }

    /// 发一个请求，**过三道闸**：硬停止过没有、闸门放不放、配额还够不够。
    ///
    /// 返回 `Ok(None)` 是**「查过、没有」**（404）——那是一条结论，调用方要把它当成
    /// 一次成功的空采集记下来，否则下一趟会重问一遍，而重问一次要额外扣一份
    /// 「未识别 ROM」配额。
    ///
    /// # Errors
    /// 这一条没取到时返回 [`Failure::Skip`]；整趟该停时返回 [`Failure::Halt`]。
    pub fn request(&self, source: &str, url: &str, ask: Ask) -> Result<Option<Vec<u8>>, Failure> {
        // 一旦停了就再也不发。**这一条就是「不重试、不换账号」本身**：撞上 431 之后
        // 后面几千个锚点一个请求都不会发出去。
        if let Some(halt) = self.halted() {
            return Err(Failure::Halt(halt));
        }
        // **走已有的闸门，不另开一条路**：媒体 URL 是服务器给的，一条被改过的响应
        // 就能把请求送到 datomatic 去，而那是一次就永久封 IP 的地方。
        if let Err(refusal) = guard::check(url) {
            self.refused();
            return Err(Failure::Skip {
                why: format!("{refusal}"),
            });
        }
        let mut tries = 0_u32;
        loop {
            // **每一发都过一次闸，退避重发的那几发也算。** 放在循环外面的话，一次 429
            // 退避三次就是四发只记一发，自设的上限当场漏掉三个——而上限存在的全部理由
            // 就是「别打太多」。
            self.charge(source)?;
            self.wait_turn();
            self.state.borrow_mut().spent += 1;
            match self.fetcher.get(url) {
                Ok(fetched) => {
                    self.state.borrow_mut().fails = 0;
                    if let Some(why) = quota_phrase(&head_of(&fetched.body)) {
                        return Err(self.halt(Halt::Quota {
                            source: source.to_string(),
                            why: why.to_string(),
                        }));
                    }
                    return Ok(Some(fetched.body));
                }
                Err(FetchError::Status { status, .. }) => match classify(status) {
                    Verdict::Ok => unreachable!("2xx 走不到这条路：get 只在非 2xx 上报 Status"),
                    Verdict::NotFound => {
                        let mut state = self.state.borrow_mut();
                        state.fails = 0;
                        // **只有条目查询的「没找到」才算进「未识别 ROM」**：431 惩罚的是
                        // 「你拿一堆我不认识的 ROM 来问」，一张图没有不是那回事。
                        if ask == Ask::Query {
                            state.not_found += 1;
                        }
                        return Ok(None);
                    }
                    Verdict::Backoff => {
                        tries += 1;
                        if tries > MAX_BACKOFF_TRIES {
                            return Err(self.halt(Halt::Throttled {
                                source: source.to_string(),
                                tries,
                            }));
                        }
                        self.nap(self.limits.backoff);
                    }
                    Verdict::Quota(why) => {
                        return Err(self.halt(Halt::Quota {
                            source: source.to_string(),
                            why: why.to_string(),
                        }));
                    }
                    Verdict::Fatal(why) => {
                        return Err(self.halt(Halt::Fatal {
                            source: source.to_string(),
                            why: why.to_string(),
                        }));
                    }
                },
                Err(FetchError::Refused(refusal)) => {
                    self.refused();
                    return Err(Failure::Skip {
                        why: format!("{refusal}"),
                    });
                }
                Err(error) => {
                    // 传输层的毛病：这一条跳过，接着采下一个。连着错太多次才认定网断了
                    // ——**已经采完的那部分留在库里，重跑接着采**。
                    let fails = {
                        let mut state = self.state.borrow_mut();
                        state.fails += 1;
                        state.fails
                    };
                    if fails >= MAX_CONSECUTIVE_FAILURES {
                        return Err(self.halt(Halt::Offline {
                            source: source.to_string(),
                            tries: fails,
                        }));
                    }
                    return Err(Failure::Skip {
                        why: format!("{error}"),
                    });
                }
            }
        }
    }

    /// 记下服务端自己说的配额剩余。
    pub fn observe(&self, response: &Value) {
        let user = &response["response"]["ssuser"];
        let left = |max: &str, used: &str| {
            let max = loose_u64(&user[max])?;
            let used = loose_u64(&user[used]).unwrap_or(0);
            Some(max.saturating_sub(used))
        };
        let mut state = self.state.borrow_mut();
        if let Some(value) = left("maxrequestsperday", "requeststoday") {
            state.server.requests_left = Some(value);
        }
        if let Some(value) = left("maxrequestskoperday", "requestskotoday") {
            state.server.ko_left = Some(value);
        }
        if let Some(value) = loose_u64(&user["maxthreads"]) {
            state.server.max_threads = Some(value);
        }
    }

    /// 这一趟还发不发得起下一个请求。
    fn charge(&self, source: &str) -> Result<(), Failure> {
        let (spent, cap, server) = {
            let state = self.state.borrow();
            (state.spent, self.limits.budget, state.server)
        };
        if spent >= cap {
            return Err(self.halt(Halt::Budget {
                source: source.to_string(),
                spent,
                cap,
            }));
        }
        // 服务端说今天没了就是没了——它比我们猜得准。
        if server.requests_left == Some(0) {
            return Err(self.halt(Halt::Quota {
                source: source.to_string(),
                why: "服务端说今天的请求配额已经用完".to_string(),
            }));
        }
        if server.ko_left == Some(0) {
            return Err(self.halt(Halt::Quota {
                source: source.to_string(),
                why: "服务端说今天的「未识别 ROM」配额已经用完".to_string(),
            }));
        }
        Ok(())
    }

    fn halt(&self, halt: Halt) -> Failure {
        self.state.borrow_mut().halted = Some(halt.clone());
        Failure::Halt(halt)
    }

    /// 到点了再发下一个。**串行**，所以一张「上次几点」就够，不必按主机分。
    fn wait_turn(&self) {
        let sleep = {
            let mut state = self.state.borrow_mut();
            let now = Instant::now();
            let sleep = state
                .last
                .and_then(|at| self.limits.interval.checked_sub(now.duration_since(at)));
            state.last = Some(now + sleep.unwrap_or_default());
            sleep
        };
        if let Some(sleep) = sleep {
            self.nap(sleep);
        }
    }

    /// 睡一会儿，**但看得见 Ctrl-C**。
    ///
    /// 直接 `sleep(5s)` 退避三次，用户按下 Ctrl-C 之后要干等 15 秒才有反应——而这一档
    /// 正是最可能让人想中途叫停的那一档（它在花配额）。切成小段，按下就走。
    fn nap(&self, total: Duration) {
        const SLICE: Duration = Duration::from_millis(100);
        let mut left = total;
        while !left.is_zero() {
            if self.cancel.is_cancelled() {
                return;
            }
            let step = left.min(SLICE);
            std::thread::sleep(step);
            left -= step;
        }
    }
}

/// **ScreenScraper 源**：按哈希查，补简介、类型、开发商与封面。
///
/// 为什么是它而不是别家：它是**唯一以「哈希 + 文件大小」为强制主键**的源
/// （官方原文「除非获得豁免，你必须发送 crc/md5/sha1 之一**以及**文件字节大小」，
/// 调研 §1.4）——那正是票 07 已经算好躺在中立库里的东西，匹配基于内容而不是文件名。
/// 代价就是这一档全部的小心：它同时是唯一一个有 431 的源。
pub struct ScreenScraper<'a> {
    net: &'a Net<'a>,
}

impl<'a> ScreenScraper<'a> {
    /// 拿一个网络句柄起一个。凭据、落点、限流与配额全在那个句柄上。
    #[must_use]
    pub fn new(net: &'a Net<'a>) -> Self {
        Self { net }
    }

    /// 一个已确认条目的查询 URL。
    fn url(&self, subject: &Subject<'_>) -> Option<String> {
        Some(self.net.credentials().game_info_url(subject.basis?))
    }
}

impl Source for ScreenScraper<'_> {
    fn name(&self) -> &str {
        SCREEN_SCRAPER
    }

    fn locality(&self) -> Locality {
        Locality::Online
    }

    fn probe(&self, subject: &Subject<'_>) -> Option<String> {
        // **只对已确认的条目发请求。** 没有自动通过的候选就没有可发的哈希，而拿文件名
        // 去碰运气正是 431 惩罚的那件事——真库里那是 16,420 个变体。
        if subject.kind != AnchorKind::Work || !subject.confirmed {
            return None;
        }
        let basis = subject.basis?;
        // 指纹盖住全部会改变结果的输入：换了判据要重查，改了媒体上限要重收。
        Some(super::fingerprint(&[
            &format!("{:08X}", basis.crc32),
            &basis.bytes.to_string(),
            &basis.rom_name,
            &subject
                .media_limit
                .map_or("无上限".to_string(), |cap| cap.to_string()),
        ]))
    }

    fn collect(&self, subject: &Subject<'_>, out: &mut Harvest) -> Result<(), Failure> {
        let Some(url) = self.url(subject) else {
            return Ok(());
        };
        let Some(body) = self.net.request(self.name(), &url, Ask::Query)? else {
            // **「查过、没有」是一条结论**，不是失败：空的采集记录照写，下一趟整条跳过。
            return Ok(());
        };
        let Ok(json) = serde_json::from_slice::<Value>(&body) else {
            return Err(Failure::Skip {
                why: format!("{SCREEN_SCRAPER} 回了一段读不动的 JSON"),
            });
        };
        self.net.observe(&json);
        harvest_game(&json["response"]["jeu"], out);
        Ok(())
    }
}

/// 从一份 `jeuInfos` 响应的 `jeu` 节点里读出字段与媒体。
///
/// **纯函数**，因此不必联网就测得动——而真实凭据这一趟本来就拿不到。
///
/// ## 两种形状都读
///
/// 调研 §1.5 记的 `medias` 是一组 `media_screenshot` / `media_box_2d` 这样的**扁平键**，
/// 而 API v2 的 JSON 输出实际给的是一个 `{type, url, format, …}` 的**数组**。
/// 手上没有真实凭据，无法当场判定哪一种是现行形状，于是**两种都认**：
/// 判据是「拿得到 URL 就收」，认不出的形状就是采不到，而不是采错。
pub fn harvest_game(game: &Value, out: &mut Harvest) {
    if let Some(title) = pick_text(&game["noms"], "region", &["ss", "wor", "us", "jp", "eu"]) {
        out.value(Field::Title, title, format!("{SCREEN_SCRAPER} 的条目名"));
    }
    // **中文排在最前**：这个库要的就是中文简介，而它正是离线档最大的那个缺口。
    if let Some(text) = pick_text(&game["synopsis"], "langue", &["zh", "cn", "en"]) {
        out.value(Field::Description, text, format!("{SCREEN_SCRAPER} 的简介"));
    }
    if let Some(text) = flat_text(&game["editeur"]) {
        out.value(Field::Publisher, text, format!("{SCREEN_SCRAPER} 的发行商"));
    }
    if let Some(text) = flat_text(&game["developpeur"]) {
        out.value(Field::Developer, text, format!("{SCREEN_SCRAPER} 的开发商"));
    }
    if let Some(date) = pick_text(&game["dates"], "region", &["wor", "jp", "us", "eu"])
        && let Some(year) = year_of(&date)
    {
        out.value(
            Field::Year,
            year,
            format!("{SCREEN_SCRAPER} 的发行日期「{date}」"),
        );
    }
    if let Some(genre) = first_genre(&game["genres"]) {
        out.value(Field::Genre, genre, format!("{SCREEN_SCRAPER} 的类型"));
    }
    for claim in media_claims(&game["medias"]) {
        out.picture(claim);
    }
}

/// 一个媒体类型名归成哪一类。
///
/// **只收认得出的那三类**，别的一概不收。两个理由：**不猜**（把说明书当封面铺出去
/// 是错，缺一张封面只是缺），以及**每下一份就花一份配额与带宽**——bezel、wheel、
/// flyer 那些眼下没有任何一个前端会用到。
#[must_use]
pub fn media_kind(name: &str) -> Option<MediaKind> {
    let folded = name.to_ascii_lowercase().replace(['_', '-', ' '], "");
    if let Some(rest) = folded.strip_prefix("media") {
        return media_kind(rest);
    }
    match folded.as_str() {
        "box2d" | "box3d" | "boxtexture" => Some(MediaKind::Cover),
        "ss" | "sstitle" | "screenshot" | "screenshottitle" => Some(MediaKind::Screenshot),
        "video" | "videonormalized" => Some(MediaKind::Video),
        _ => None,
    }
}

/// 从 `medias` 节点里挑出要收的那几份。**每一类最多一份**——多下一张就多花一份配额。
fn media_claims(medias: &Value) -> Vec<MediaClaim> {
    let mut picked: Vec<(MediaKind, u32, MediaClaim)> = Vec::new();
    let mut offer = |kind: MediaKind, rank: u32, claim: MediaClaim| match picked
        .iter()
        .position(|(had, _, _)| *had == kind)
    {
        Some(at) if picked[at].1 <= rank => {}
        Some(at) => picked[at] = (kind, rank, claim),
        None => picked.push((kind, rank, claim)),
    };
    match medias {
        Value::Array(items) => {
            for item in items {
                let Some(kind) = item["type"].as_str().and_then(media_kind) else {
                    continue;
                };
                let Some(url) = item["url"].as_str() else {
                    continue;
                };
                let region = item["region"].as_str().unwrap_or("");
                offer(
                    kind,
                    region_rank(region),
                    MediaClaim {
                        kind,
                        from: MediaFrom::Online {
                            url: url.to_string(),
                            ext: normalize_ext(item["format"].as_str().unwrap_or("")),
                        },
                        bytes: loose_u64(&item["size"]),
                        why: format!(
                            "{SCREEN_SCRAPER} 的 {} 媒体（{}）",
                            item["type"].as_str().unwrap_or("?"),
                            if region.is_empty() {
                                "无区域"
                            } else {
                                region
                            }
                        ),
                    },
                );
            }
        }
        Value::Object(fields) => {
            for (name, value) in fields {
                let Some(kind) = media_kind(name) else {
                    continue;
                };
                let Some(url) = value.as_str() else {
                    continue;
                };
                offer(
                    kind,
                    region_rank(""),
                    MediaClaim {
                        kind,
                        from: MediaFrom::Online {
                            url: url.to_string(),
                            ext: normalize_ext(""),
                        },
                        bytes: None,
                        why: format!("{SCREEN_SCRAPER} 的 {name}"),
                    },
                );
            }
        }
        _ => {}
    }
    picked.sort_by_key(|(kind, _, _)| *kind);
    picked.into_iter().map(|(_, _, claim)| claim).collect()
}

/// 区域排第几。小的优先：世界版最通用，然后日欧美，最后无区域与其余。
fn region_rank(region: &str) -> u32 {
    match region {
        "wor" => 0,
        "jp" => 1,
        "us" => 2,
        "eu" => 3,
        "" => 5,
        _ => 4,
    }
}

/// 媒体的扩展名。源没说就当 `png`——ScreenScraper 的图默认是 PNG，而**猜错扩展名
/// 只是名字不好看，内容哈希不受影响**（池按内容寻址，ADR-0009）。
fn normalize_ext(format: &str) -> String {
    let folded = format.trim().to_ascii_lowercase();
    match crate::scrape::pool::normalized_ext(&folded) {
        Some(ext) => ext.to_string(),
        None => "png".to_string(),
    }
}

/// 一组带语言/区域标签的文本里挑一条：按 `prefer` 的顺序，都不中就取第一条。
///
/// 数组形状（`[{region, text}]`）与对象形状（`{nom_us: "…"}`）都认。
fn pick_text(node: &Value, tag: &str, prefer: &[&str]) -> Option<String> {
    let mut found: Vec<(String, String)> = Vec::new();
    match node {
        Value::Array(items) => {
            for item in items {
                if let Some(text) = item["text"].as_str().filter(|text| !text.trim().is_empty()) {
                    found.push((
                        item[tag].as_str().unwrap_or("").to_string(),
                        text.to_string(),
                    ));
                }
            }
        }
        Value::Object(fields) => {
            for (name, value) in fields {
                if let Some(text) = value.as_str().filter(|text| !text.trim().is_empty()) {
                    // `nom_us` / `synopsis_zh`：标签是最后一段。
                    let label = name.rsplit('_').next().unwrap_or(name);
                    found.push((label.to_string(), text.to_string()));
                }
            }
        }
        Value::String(text) if !text.trim().is_empty() => return Some(text.clone()),
        _ => {}
    }
    for want in prefer {
        if let Some((_, text)) = found.iter().find(|(label, _)| label == want) {
            return Some(text.clone());
        }
    }
    found.first().map(|(_, text)| text.clone())
}

/// `{"id": 3, "text": "Konami"}` 或者直接一个字符串。
fn flat_text(node: &Value) -> Option<String> {
    match node {
        Value::String(text) => Some(text.clone()),
        Value::Object(_) => node["text"].as_str().map(ToString::to_string),
        _ => None,
    }
    .filter(|text| !text.trim().is_empty())
}

/// 第一个类型的名字。中文优先。
fn first_genre(node: &Value) -> Option<String> {
    match node {
        Value::Array(items) => items
            .iter()
            .find_map(|item| pick_text(&item["noms"], "langue", &["zh", "cn", "en"])),
        _ => pick_text(node, "langue", &["zh", "cn", "en"]),
    }
}

/// 一个发行日期里的年份。**认不出四位数字就不产出**——同 TOSEC 那一侧的处置：
/// 把「不知道」伪装成「知道」比缺着更难查。
fn year_of(date: &str) -> Option<String> {
    let head = date.get(..4)?;
    if head.len() == 4 && head.chars().all(|c| c.is_ascii_digit()) {
        Some(head.to_string())
    } else {
        None
    }
}

/// JSON 里的数字，字符串装着也认。ScreenScraper 的这几个计数常常是字符串。
fn loose_u64(node: &Value) -> Option<u64> {
    match node {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

/// 查询串里的一段值。
///
/// 自己写而不引一个 crate：要转义的只有查询串这一处，而 RFC 3986 的未保留字符集
/// 就这么四行。**中文文件名必须转义**——真库里到处都是。
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 凭据() -> Credentials {
        Credentials {
            dev_id: "测试".to_string(),
            dev_password: "口令".to_string(),
            soft_name: "romcat-test".to_string(),
            user: None,
            user_password: None,
        }
    }

    #[test]
    fn 配额超限判成硬停止而不是重试() {
        // **这张表是这一档的心脏。** 431 是 ScreenScraper 专门为「未识别 ROM」设的
        // 配额，而这个库里上万个变体在它眼里正是未识别 ROM。判错方向就是接着打，
        // 而配额同时按账号与 IP 计——撞穿的处置是永久封禁。
        assert!(matches!(classify(430), Verdict::Quota(_)));
        assert!(matches!(classify(431), Verdict::Quota(_)));
        assert!(matches!(classify(403), Verdict::Fatal(_)));
        assert!(matches!(classify(426), Verdict::Fatal(_)));
        assert!(matches!(classify(423), Verdict::Fatal(_)));
        assert_eq!(classify(404), Verdict::NotFound);
        assert_eq!(classify(429), Verdict::Backoff, "线程数超了才是可恢复的");
        assert_eq!(classify(503), Verdict::Backoff);
        assert_eq!(classify(200), Verdict::Ok);
        // 没见过的码一律停：这一档猜错方向赌上的是账号与 IP。
        assert!(matches!(classify(418), Verdict::Fatal(_)));
    }

    #[test]
    fn 正文里的配额话术也认() {
        assert!(quota_phrase("Votre quota de scrape est atteint").is_some());
        assert!(quota_phrase("Faite du tri dans vos fichiers roms et repassez demain !").is_some());
        assert!(quota_phrase("{\"response\":{}}").is_none());
    }

    #[test]
    fn 只对已确认的条目发请求() {
        let fetcher = crate::dat::CannedFetcher::new();
        let cancel = CancelToken::new();
        let net = Net::new(&fetcher, Limits::default(), 凭据(), &cancel);
        let source = ScreenScraper::new(&net);

        let basis = super::super::Basis {
            crc32: 0x50AB_C90A,
            bytes: 749_652,
            rom_name: "Contra (Japan).nes".to_string(),
        };
        let mut subject = Subject {
            kind: AnchorKind::Work,
            id: "Contra",
            platform: Some("FC"),
            entries: &[],
            main_key: None,
            media: &[],
            variants: &[],
            media_limit: None,
            confirmed: true,
            basis: Some(&basis),
        };
        assert!(source.probe(&subject).is_some(), "已确认的该发");

        // **未识别的一个字节都不发**：真库里那是 16,420 个变体，
        // 每一个在 ScreenScraper 眼里都是一次「未识别 ROM」。
        subject.confirmed = false;
        assert!(source.probe(&subject).is_none());

        // 变体那一层也不发：一部作品查一次就够，按变体查等于把配额乘三倍。
        subject.confirmed = true;
        subject.kind = AnchorKind::Variant;
        assert!(source.probe(&subject).is_none());
    }

    #[test]
    fn 停了之后一个请求都不再发() {
        // 「不重试、不换账号」在代码里的样子：撞上 431 之后，后面每一次 `request`
        // 都在**发出去之前**就被挡回来。
        let url = "https://api.screenscraper.fr/api2/jeuInfos.php?x=1";
        let fetcher = crate::dat::CannedFetcher::new().with_status(url, 431, Vec::new());
        let cancel = CancelToken::new();
        let net = Net::new(&fetcher, Limits::default(), 凭据(), &cancel);

        let first = net
            .request(SCREEN_SCRAPER, url, Ask::Query)
            .expect_err("该停");
        assert!(matches!(first, Failure::Halt(Halt::Quota { .. })));
        let second = net
            .request(SCREEN_SCRAPER, url, Ask::Query)
            .expect_err("还是停");
        assert!(matches!(second, Failure::Halt(Halt::Quota { .. })));
        assert_eq!(
            fetcher.asked().len(),
            1,
            "撞上 431 之后不许再发第二个请求——重试与换账号都是永久封禁那条路"
        );
    }

    #[test]
    fn 自设的请求上限先于服务端配额收手() {
        let url = "https://api.screenscraper.fr/api2/jeuInfos.php?x=1";
        let fetcher = crate::dat::CannedFetcher::new().with(url, b"{}".to_vec());
        let cancel = CancelToken::new();
        let net = Net::new(
            &fetcher,
            Limits {
                interval: Duration::from_millis(0),
                budget: 2,
                backoff: Duration::from_millis(0),
            },
            凭据(),
            &cancel,
        );
        assert!(net.request(SCREEN_SCRAPER, url, Ask::Query).is_ok());
        assert!(net.request(SCREEN_SCRAPER, url, Ask::Query).is_ok());
        let stopped = net
            .request(SCREEN_SCRAPER, url, Ask::Query)
            .expect_err("上限到了");
        assert!(matches!(
            stopped,
            Failure::Halt(Halt::Budget {
                spent: 2,
                cap: 2,
                ..
            })
        ));
        assert_eq!(fetcher.asked().len(), 2);
    }

    #[test]
    fn 查过没有是一条结论而不是失败() {
        // 404 若当成失败，采集记录不写，下一趟会重问一遍——而重问一次要额外扣一份
        // 「未识别 ROM」配额，那份配额撞穿就是 431。
        let url = "https://api.screenscraper.fr/api2/jeuInfos.php?x=1";
        let fetcher = crate::dat::CannedFetcher::new().with_status(url, 404, Vec::new());
        let cancel = CancelToken::new();
        let net = Net::new(&fetcher, Limits::default(), 凭据(), &cancel);
        assert_eq!(
            net.request(SCREEN_SCRAPER, url, Ask::Query)
                .expect("不是失败"),
            None
        );
        assert_eq!(net.usage().not_found, 1);
        assert!(net.halted().is_none(), "查不到不该让整趟停下来");
    }

    #[test]
    fn 请求之间真的隔着那么久() {
        let url = "https://api.screenscraper.fr/api2/jeuInfos.php?x=1";
        let fetcher = crate::dat::CannedFetcher::new().with(url, b"{}".to_vec());
        let cancel = CancelToken::new();
        let net = Net::new(
            &fetcher,
            Limits {
                interval: Duration::from_millis(40),
                budget: 10,
                backoff: Duration::from_millis(0),
            },
            凭据(),
            &cancel,
        );
        let started = Instant::now();
        for _ in 0..3 {
            net.request(SCREEN_SCRAPER, url, Ask::Query)
                .expect("发得出去");
        }
        // 三个请求之间有两个间隔。**限流是这一档的默认行为，不是可选项。**
        assert!(
            started.elapsed() >= Duration::from_millis(80),
            "三个请求只用了 {:?}，限流没生效",
            started.elapsed()
        );
        assert_eq!(MAX_CONCURRENCY, 1, "并发上限恒为 1");
    }

    #[test]
    fn 服务端说没配额了就不发() {
        let url = "https://api.screenscraper.fr/api2/jeuInfos.php?x=1";
        let fetcher = crate::dat::CannedFetcher::new().with(url, b"{}".to_vec());
        let cancel = CancelToken::new();
        let net = Net::new(
            &fetcher,
            Limits {
                interval: Duration::from_millis(0),
                budget: 100,
                backoff: Duration::from_millis(0),
            },
            凭据(),
            &cancel,
        );
        // **一个数字都不写死**：日配额三份官方文档给了三个数，只有响应里的才作数。
        net.observe(&serde_json::json!({
            "response": { "ssuser": {
                "maxrequestsperday": "20000", "requeststoday": "20000",
                "maxrequestskoperday": "500", "requestskotoday": "10",
                "maxthreads": "1"
            }}
        }));
        assert_eq!(net.usage().server.requests_left, Some(0));
        assert_eq!(net.usage().server.ko_left, Some(490));
        let stopped = net
            .request(SCREEN_SCRAPER, url, Ask::Query)
            .expect_err("该停");
        assert!(matches!(stopped, Failure::Halt(Halt::Quota { .. })));
        assert!(fetcher.asked().is_empty(), "一个请求都不该发出去");
    }

    #[test]
    fn 退避重发也吃自设的请求上限() {
        // `charge` 若留在重试循环外面，一次 429 退避三次就是**四发只记一发**——
        // 而自设上限存在的全部理由就是「别打太多」。
        let url = "https://api.screenscraper.fr/api2/jeuInfos.php?x=1";
        let fetcher = crate::dat::CannedFetcher::new().with_status(url, 429, Vec::new());
        let cancel = CancelToken::new();
        let net = Net::new(
            &fetcher,
            Limits {
                interval: Duration::from_millis(0),
                budget: 2,
                backoff: Duration::from_millis(0),
            },
            凭据(),
            &cancel,
        );
        let stopped = net
            .request(SCREEN_SCRAPER, url, Ask::Query)
            .expect_err("该停");
        // 退避到第三发时上限先到，于是停在「自己收的手」而不是「退避到头」。
        assert!(
            matches!(stopped, Failure::Halt(Halt::Budget { .. })),
            "{stopped:?}"
        );
        assert_eq!(fetcher.asked().len(), 2, "一共只许发出去两发");
    }

    #[test]
    fn 退避有次数上限_到头就停() {
        let url = "https://api.screenscraper.fr/api2/jeuInfos.php?x=1";
        let fetcher = crate::dat::CannedFetcher::new().with_status(url, 429, Vec::new());
        let cancel = CancelToken::new();
        let net = Net::new(
            &fetcher,
            Limits {
                interval: Duration::from_millis(0),
                budget: 100,
                backoff: Duration::from_millis(0),
            },
            凭据(),
            &cancel,
        );
        let stopped = net
            .request(SCREEN_SCRAPER, url, Ask::Query)
            .expect_err("该停");
        assert!(
            matches!(stopped, Failure::Halt(Halt::Throttled { .. })),
            "{stopped:?}"
        );
        assert_eq!(
            fetcher.asked().len(),
            MAX_BACKOFF_TRIES as usize + 1,
            "第一发加三次退避，到头就停——不是无限重试"
        );
    }

    #[test]
    fn 一张图没有不算进未识别_rom() {
        // **431 惩罚的是「拿一堆我不认识的 ROM 来问」**。一张封面 404 说的是
        // 「这份媒体没有」，混着数，报告里那一栏就说不出配额的实情。
        let url = "https://www.screenscraper.fr/image.php?gameid=1";
        let fetcher = crate::dat::CannedFetcher::new().with_status(url, 404, Vec::new());
        let cancel = CancelToken::new();
        let net = Net::new(&fetcher, Limits::default(), 凭据(), &cancel);
        assert_eq!(
            net.request(SCREEN_SCRAPER, url, Ask::Media)
                .expect("不是失败"),
            None
        );
        assert_eq!(net.usage().not_found, 0, "媒体没有不算「未识别 ROM」");
        assert_eq!(net.usage().requests, 1, "但请求照样发出去了，照样计数");
    }

    #[test]
    fn 按下_ctrl_c_之后退避不再干等() {
        // 直接 `sleep(5s)` 退避三次，按下 Ctrl-C 要干等 15 秒——而这一档正是最可能
        // 让人想中途叫停的那一档（它在花配额）。
        let url = "https://api.screenscraper.fr/api2/jeuInfos.php?x=1";
        let fetcher = crate::dat::CannedFetcher::new().with_status(url, 429, Vec::new());
        let cancel = CancelToken::new();
        cancel.cancel();
        let net = Net::new(
            &fetcher,
            Limits {
                interval: Duration::from_secs(30),
                budget: 100,
                backoff: Duration::from_secs(30),
            },
            凭据(),
            &cancel,
        );
        let started = Instant::now();
        let _ = net.request(SCREEN_SCRAPER, url, Ask::Query);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "取消之后不该还在睡：用了 {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn 媒体_url_也过闸门() {
        // 媒体 URL 是**服务器说了算**的。一条被改过的响应把它指到 datomatic，
        // 一次请求就够触发永久 IP 封禁——闸门是唯一挡得住这件事的东西。
        let fetcher = crate::dat::CannedFetcher::new();
        let cancel = CancelToken::new();
        let net = Net::new(&fetcher, Limits::default(), 凭据(), &cancel);
        let refused = net
            .request(
                SCREEN_SCRAPER,
                "https://datomatic.no-intro.org/box.png",
                Ask::Media,
            )
            .expect_err("该拦下");
        assert!(matches!(refused, Failure::Skip { .. }));
        assert!(fetcher.asked().is_empty());
        assert_eq!(net.usage().refused, 1);
        assert!(net.halted().is_none(), "拦下一张图不该让整趟停下来");
    }

    #[test]
    fn 查询里带着哈希与文件大小() {
        // 官方要求「除非获得豁免，你**必须**发送 crc/md5/sha1 之一**以及**文件字节
        // 大小」（调研 §1.4）。少一样就是一次注定的未命中，白扣一份 431 的配额。
        let fetcher = crate::dat::CannedFetcher::new();
        let cancel = CancelToken::new();
        let net = Net::new(&fetcher, Limits::default(), 凭据(), &cancel);
        let source = ScreenScraper::new(&net);
        let basis = super::super::Basis {
            crc32: 0x50AB_C90A,
            bytes: 749_652,
            rom_name: "魂斗罗 (Japan).nes".to_string(),
        };
        let url = source
            .url(&Subject {
                kind: AnchorKind::Work,
                id: "Contra",
                platform: Some("FC"),
                entries: &[],
                main_key: None,
                media: &[],
                variants: &[],
                media_limit: None,
                confirmed: true,
                basis: Some(&basis),
            })
            .expect("造得出 URL");
        assert!(url.contains("crc=50ABC90A"), "{url}");
        assert!(url.contains("romtaille=749652"), "{url}");
        assert!(url.contains("output=json"), "{url}");
        assert!(!url.contains('魂'), "中文文件名必须转义：{url}");
        guard::check(&url).expect("自己造的 URL 也得过闸门");
    }

    #[test]
    fn 数组形状与扁平形状都读得出来() {
        // 调研 §1.5 记的是 `media_screenshot` 这样的扁平键，而 v2 的 JSON 实际给的
        // 是数组。手上没有真实凭据判不了哪一种是现行形状，于是两种都认。
        let mut 数组 = Harvest::default();
        harvest_game(
            &serde_json::json!({
                "noms": [{"region": "wor", "text": "Contra"}],
                "synopsis": [{"langue": "en", "text": "run and gun"},
                             {"langue": "zh", "text": "魂斗罗的简介"}],
                "editeur": {"id": 3, "text": "Konami"},
                "developpeur": {"id": 3, "text": "Konami"},
                "dates": [{"region": "jp", "text": "1988-02-09"}],
                "genres": [{"noms": [{"langue": "zh", "text": "动作"}]}],
                "medias": [
                    {"type": "box-2D", "region": "jp", "url": "https://www.screenscraper.fr/a",
                     "format": "png", "size": "1024"},
                    {"type": "box-2D", "region": "wor", "url": "https://www.screenscraper.fr/b",
                     "format": "png", "size": "2048"},
                    {"type": "bezel-16-9", "url": "https://www.screenscraper.fr/c"}
                ]
            }),
            &mut 数组,
        );
        let 取 = |harvest: &Harvest, field: Field| {
            harvest
                .values
                .iter()
                .find(|found| found.field == field)
                .map(|found| found.value.clone())
        };
        assert_eq!(取(&数组, Field::Title).as_deref(), Some("Contra"));
        assert_eq!(
            取(&数组, Field::Description).as_deref(),
            Some("魂斗罗的简介"),
            "简介必须中文优先——那是这个库要的东西"
        );
        assert_eq!(取(&数组, Field::Year).as_deref(), Some("1988"));
        assert_eq!(取(&数组, Field::Publisher).as_deref(), Some("Konami"));
        assert_eq!(取(&数组, Field::Genre).as_deref(), Some("动作"));
        assert_eq!(数组.media.len(), 1, "一类只收一份，bezel 那种一概不收");
        assert!(matches!(
            &数组.media[0].from,
            MediaFrom::Online { url, .. } if url.ends_with("/b")
        ));
        assert_eq!(数组.media[0].bytes, Some(2048));

        let mut 扁平 = Harvest::default();
        harvest_game(
            &serde_json::json!({
                "noms": {"nom_wor": "Contra"},
                "synopsis": {"synopsis_zh": "魂斗罗"},
                "medias": {"media_box2d": "https://www.screenscraper.fr/x",
                           "media_manuel": "https://www.screenscraper.fr/m"}
            }),
            &mut 扁平,
        );
        assert_eq!(取(&扁平, Field::Title).as_deref(), Some("Contra"));
        assert_eq!(取(&扁平, Field::Description).as_deref(), Some("魂斗罗"));
        assert_eq!(扁平.media.len(), 1, "说明书不是封面，不收");
        assert_eq!(扁平.media[0].kind, MediaKind::Cover);
    }

    #[test]
    fn 认不出的媒体类型不猜() {
        assert_eq!(media_kind("box-2D"), Some(MediaKind::Cover));
        assert_eq!(media_kind("media_box2d"), Some(MediaKind::Cover));
        assert_eq!(media_kind("ss"), Some(MediaKind::Screenshot));
        assert_eq!(media_kind("video"), Some(MediaKind::Video));
        assert_eq!(media_kind("manuel"), None, "说明书当封面铺出去是错");
        assert_eq!(media_kind("wheel"), None);
        assert_eq!(media_kind("bezel-16-9"), None);
    }

    #[test]
    fn 年份认不出四位数字就不产出() {
        assert_eq!(year_of("1988-02-09").as_deref(), Some("1988"));
        assert_eq!(year_of("198x"), None);
        assert_eq!(year_of(""), None);
    }
}
