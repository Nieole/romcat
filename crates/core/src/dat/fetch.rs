//! **取数接缝**：这个库里唯一碰网络的地方。
//!
//! 与 [`LibraryFs`](crate::fs::LibraryFs) 同源的做法——把 IO 挤到边缘，同步的编排
//! 全是纯计算。[`CannedFetcher`] 让整条链路完全离线测得动：DAT 是几百 MB 的东西，
//! 每跑一次测试就去拉一遍既慢又不礼貌。
//!
//! ## 重定向自己跟
//!
//! [`HttpFetcher`] 关掉了 HTTP 层的自动重定向，一跳一跳自己走。理由只有一个：
//! [`super::guard`] 那道闸门必须看到**每一跳**。让 HTTP 层闷头跟完，一个
//! 302 就能把请求送到 `datomatic` 上去——而那正是要挡的那件事。
//!
//! ## 礼貌
//!
//! Redump 的 `robots.txt` 是全站放行、没有 `Crawl-delay`，但调研的结论是「仍应加入
//! 礼貌性节流」。[`HttpFetcher`] 按**主机**限速，默认每秒一次。

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::guard::{self, Refusal};
use crate::task::Halted;

/// 默认每个主机之间隔多久再发下一个请求。
pub const DEFAULT_THROTTLE: Duration = Duration::from_secs(1);

/// 单个响应最多收多少字节。No-Intro 那个整包实测 106 MB，留到 1 GiB。
const MAX_BODY: u64 = 1 << 30;

/// 最多跟几跳重定向。
const MAX_HOPS: usize = 5;

/// 大件下载**一次搬多少字节**。
///
/// 这个数就是「按下停下」到「真的收手」之间的粒度：[`copy_while`] 每搬一块之前问一次
/// 要不要收手，所以最坏是**读完当前这一块**的时间。中文离线源那份 dump 是 435 MB，
/// 一口气搬完要几分钟——那几分钟里按停下没有反应，等于按了个假按钮。
///
/// 一块 1 MiB 是在两头之间选的：再小就是每几毫秒问一次，白花系统调用；
/// 再大，一条慢线路上读完一块就要好几秒，那个按钮又开始迟钝。
const DOWNLOAD_CHUNK: usize = 1 << 20;

/// 一次响应的状态与响应头。**正文不在里面**——问「变了没有」只需要这些。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Head {
    /// HTTP 状态码。
    pub status: u16,
    /// 响应头。名字一律折成小写，省得调用方猜大小写。
    pub headers: BTreeMap<String, String>,
}

impl Head {
    /// 取一个响应头。名字大小写不敏感。
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// 2xx。
    #[must_use]
    pub fn is_ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// `Content-Disposition` 里那个 `filename="…"`。
    ///
    /// Redump 不给 `ETag` 也不给 `Last-Modified`，但它在这个头里写了
    /// `Sony - PlayStation - Datfile (10974) (2026-08-31 07-59-02).zip`
    /// ——**条目数与生成时刻都在里面**，正好当增量的指纹用。
    #[must_use]
    pub fn attachment_name(&self) -> Option<&str> {
        let value = self.get("content-disposition")?;
        let at = value.find("filename=")? + "filename=".len();
        let rest = value[at..].trim();
        Some(match rest.strip_prefix('"') {
            Some(quoted) => quoted.split('"').next().unwrap_or(quoted),
            None => rest.split(';').next().unwrap_or(rest).trim(),
        })
    }
}

/// 一次取回来的东西。
#[derive(Debug, Clone)]
pub struct Fetched {
    /// 状态与响应头。
    pub head: Head,
    /// 正文。
    pub body: Vec<u8>,
}

/// 取数失败。
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// 闸门拦下了。
    #[error("{0}")]
    Refused(#[from] Refusal),
    /// 传输层出错。
    #[error("取 {url} 失败：{detail}")]
    Transport {
        /// 出问题的 URL。
        url: String,
        /// 底层说了什么。
        detail: String,
    },
    /// 服务器回了个不是 2xx 的码。
    #[error("取 {url} 得到 HTTP {status}")]
    Status {
        /// 出问题的 URL。
        url: String,
        /// 状态码。
        status: u16,
    },
    /// 重定向跟不完。
    #[error("取 {url} 时重定向超过 {MAX_HOPS} 跳")]
    TooManyHops {
        /// 起点。
        url: String,
    },
    /// 落盘失败。
    #[error("写不进 {path}：{source}")]
    Io {
        /// 目标文件。
        path: String,
        /// 底层错误。
        source: io::Error,
    },
    /// 测试替身没有备下这一条。
    #[error("没有为 {url} 备下响应")]
    NotCanned {
        /// 问的是哪个 URL。
        url: String,
    },
    /// **被叫停了。** 停在两块之间，那半截临时文件已经删掉。
    ///
    /// 它与别的几样分开一格，是因为**这不是失败**：一个字节都没坏，接着按一次
    /// 就从头再下。折成一句「取数失败」的话，界面上按一下停下会说成出了错。
    #[error(transparent)]
    Halted(#[from] Halted),
}

/// 一块一块地搬，**每搬一块之前问一次要不要收手**。
///
/// 交出搬进去多少字节。`keep_going` 说「别搬了」时当场返回
/// [`FetchError::Halted`]——**已经搬进去的那半截归调用方处置**
/// （[`HttpFetcher::download_while`] 把那个临时文件删掉）。
///
/// 收手有多快只取决于一块有多大（[`DOWNLOAD_CHUNK`]），与整份有多大无关：
/// 那 435 MB 里按下停下，收手的代价是读完手上这一块，不是把剩下那 400 多 MB 读完。
///
/// `path` 只用来在出错时说清是哪个文件。
fn copy_while(
    from: &mut dyn io::Read,
    into: &mut dyn io::Write,
    path: &str,
    keep_going: &dyn Fn() -> bool,
) -> Result<u64, FetchError> {
    let mut buf = vec![0_u8; DOWNLOAD_CHUNK];
    let mut moved = 0_u64;
    loop {
        // **问在读之前**：这一趟要么整块搬完、要么这一块根本没开始，
        // 于是那个临时文件里不会留下半块。
        if !keep_going() {
            return Err(Halted.into());
        }
        let read = match from.read(&mut buf) {
            Ok(read) => read,
            // **EINTR 不是「读不动」，是「再读一次」。** 它换掉的 `io::copy` 本来就吞
            // 并重试这一档；不吞的话，一个信号就会让「按停」显示成
            // 「读写 … 失败：Interrupted」——那正是这条路要消灭的那种误报。
            Err(source) if source.kind() == io::ErrorKind::Interrupted => continue,
            Err(source) => {
                return Err(FetchError::Io {
                    path: path.to_string(),
                    source,
                });
            }
        };
        if read == 0 {
            return Ok(moved);
        }
        into.write_all(&buf[..read])
            .map_err(|source| FetchError::Io {
                path: path.to_string(),
                source,
            })?;
        moved += read as u64;
    }
}

/// 取数的接缝。
pub trait Fetcher: Sync {
    /// 只要响应头。用来问「这份东西变了没有」。
    ///
    /// # Errors
    /// 闸门拦下、传输失败或状态码不是 2xx 时返回错误。
    fn head(&self, url: &str) -> Result<Head, FetchError>;

    /// 取整个正文。
    ///
    /// # Errors
    /// 同 [`Self::head`]。
    fn get(&self, url: &str) -> Result<Fetched, FetchError>;

    /// 取整个正文并落到文件——大件走这条，不进内存。
    ///
    /// # Errors
    /// 同 [`Self::head`]，外加写不进目标文件。
    fn download(&self, url: &str, to: &Path) -> Result<Head, FetchError>;

    /// 同 [`Self::download`]，**只是边下边问要不要收手**。
    ///
    /// 中文离线源那份 dump 是 435 MB，一口气下完要几分钟；界面上那个「停下」与命令行
    /// 的 Ctrl-C 得在那几分钟里**当场**有反应，不能等它下完。走这条的调用方把自己那个
    /// 中断信号折成 `keep_going` 递进来。
    ///
    /// **默认实现只在开工之前问一次**，之后一口气下完。小件（几百 KB 的清单、
    /// 几十 MB 的 DAT）走这条没问题——它们本来就一瞬就完。**大件必须自己覆盖它**，
    /// 不然按下停下要等到整份下完才收手。
    ///
    /// # Errors
    /// 同 [`Self::download`]；被叫停时返回 [`FetchError::Halted`]。
    fn download_while(
        &self,
        url: &str,
        to: &Path,
        keep_going: &dyn Fn() -> bool,
    ) -> Result<Head, FetchError> {
        if !keep_going() {
            return Err(Halted.into());
        }
        self.download(url, to)
    }

    /// 发一个带正文的请求（票 12 的模型推断要它——问题装在正文里，不装在查询串上）。
    ///
    /// **这一条不跟重定向。** `head` / `get` 一跳一跳自己走，是因为 DAT 的落点真的会
    /// 302 到对象存储上去；而一个 POST 被重定向意味着落点不对，把正文原样再发一遍到
    /// 另一个主机上，是把请求（连同 `headers` 里的凭据）送到没打算送的地方。走到头
    /// 不是 2xx 就照 [`require_ok`] 折成 [`FetchError::Status`]，**状态码原样带着**。
    ///
    /// `headers` 里装的是凭据。**实现不许把它记进任何日志或测试录音**——
    /// [`CannedFetcher`] 只记 URL 与正文。
    ///
    /// # Errors
    /// 同 [`Self::head`]。
    fn post(&self, url: &str, headers: &[(&str, &str)], body: &[u8])
    -> Result<Fetched, FetchError>;
}

/// 真的联网的那一个。
pub struct HttpFetcher {
    agent: ureq::Agent,
    throttle: Duration,
    last: Mutex<BTreeMap<String, Instant>>,
    github_token: Option<String>,
}

impl HttpFetcher {
    /// 起一个，按默认节流。
    #[must_use]
    pub fn new() -> Self {
        Self::with_throttle(DEFAULT_THROTTLE)
    }

    /// 起一个，自己定每个主机之间隔多久。
    #[must_use]
    pub fn with_throttle(throttle: Duration) -> Self {
        let config = ureq::Agent::config_builder()
            .user_agent(concat!("romcat/", env!("CARGO_PKG_VERSION")))
            .timeout_global(Some(Duration::from_secs(300)))
            // 重定向自己跟：闸门必须看到每一跳（见模块文档）。
            .max_redirects(0)
            .max_redirects_will_error(false)
            // **状态码要拿到手，不要被折成一句错误文本。** ureq 默认把 4xx/5xx 变成
            // `Error::StatusCode`，于是这一层只剩下一个 `Transport{detail}`
            // ——而在线刮削档必须分得出 430（当日配额超限）与 431（当日「未识别 ROM」
            // 配额超限），那两个码的处置是**硬停止**，跟一次网络抖动完全不是一回事
            // （`scrape::online::classify`）。关掉它，状态码原样交上来，
            // 由 [`require_ok`] 统一折成 [`FetchError::Status`]。
            .http_status_as_error(false)
            .build();
        Self {
            agent: config.into(),
            throttle,
            last: Mutex::new(BTreeMap::new()),
            // GitHub 未登录时每小时只给 60 次 API 调用。这一趟同步要用掉八九次，
            // 平时够用；有 token 就带上，省得跟别的工具抢配额。
            // **只往 api.github.com 发**——闸门保证了别的主机拿不到它。
            github_token: std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty()),
        }
    }

    /// 到点了再发下一个。按主机分别计时：拉 GitHub 不该拖慢拉 Redump。
    fn wait_turn(&self, host: &str) {
        let sleep = {
            let mut last = self.last.lock().unwrap_or_else(|poisoned| {
                // 锁只护着一张「上次几点」的表，毒了也不影响正确性，最多多等一会儿。
                poisoned.into_inner()
            });
            let now = Instant::now();
            let sleep = last
                .get(host)
                .and_then(|at| self.throttle.checked_sub(now.duration_since(*at)));
            last.insert(host.to_string(), now + sleep.unwrap_or_default());
            sleep
        };
        if let Some(sleep) = sleep {
            std::thread::sleep(sleep);
        }
    }

    /// 发一次，**不跟重定向**，返回状态、响应头与响应体。
    fn once(&self, method: Method, url: &str) -> Result<(Head, ureq::Body), FetchError> {
        let host = guard::check(url)?;
        self.wait_turn(host);
        let mut request = match method {
            Method::Head => self.agent.head(url),
            Method::Get => self.agent.get(url),
        };
        if host == "api.github.com" {
            request = request.header("Accept", "application/vnd.github+json");
            if let Some(token) = &self.github_token {
                request = request.header("Authorization", &format!("Bearer {token}"));
            }
        }
        let response = request.call().map_err(|error| FetchError::Transport {
            url: url.to_string(),
            detail: error.to_string(),
        })?;
        let (parts, body) = response.into_parts();
        let mut headers = BTreeMap::new();
        for (name, value) in &parts.headers {
            if let Ok(text) = value.to_str() {
                headers.insert(name.as_str().to_ascii_lowercase(), text.to_string());
            }
        }
        let head = Head {
            status: parts.status.as_u16(),
            headers,
        };
        Ok((head, body))
    }

    /// 一跳一跳走到底，每一跳都过闸门。
    fn follow(&self, method: Method, url: &str) -> Result<(Head, ureq::Body), FetchError> {
        let mut at = url.to_string();
        for _ in 0..MAX_HOPS {
            let (head, body) = self.once(method, &at)?;
            match next_hop(&at, &head)? {
                Some(next) => at = next,
                None => return Ok((head, body)),
            }
        }
        Err(FetchError::TooManyHops {
            url: url.to_string(),
        })
    }
}

impl Default for HttpFetcher {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
enum Method {
    Head,
    Get,
}

/// 这一跳之后还要不要再走一跳，要的话走到哪。
///
/// **抽成纯函数是为了让「重定向也过闸门」这件事测得动。** 关掉 HTTP 层的自动重定向
/// 只是第一步——真正要证明的是「一个 302 送不到 datomatic 去」，而那需要在不联网的
/// 情况下断言得出来。
///
/// **它不判状态码好坏**：走到头就是走到头，是不是 2xx 由 [`require_ok`] 说。分开是
/// 因为在线刮削档要看到 430 / 431 那两个码本身，而不是一句「取数失败」。
fn next_hop(from: &str, head: &Head) -> Result<Option<String>, FetchError> {
    let redirect = (300..400).contains(&head.status);
    match head.get("location") {
        Some(location) if redirect => {
            let next = absolute(from, location);
            guard::check(&next)?;
            Ok(Some(next))
        }
        _ => Ok(None),
    }
}

/// 走到头的那一跳是不是 2xx。
///
/// # Errors
/// 不是 2xx 时返回 [`FetchError::Status`]，**状态码原样带着**。
fn require_ok(url: &str, head: &Head) -> Result<(), FetchError> {
    if head.is_ok() {
        Ok(())
    } else {
        Err(FetchError::Status {
            url: url.to_string(),
            status: head.status,
        })
    }
}

/// 把 `Location` 折成绝对 URL。
///
/// 相对跳转必须接回**原来那一跳的主机**，不能接回最初那个——不然一串跳转里
/// 中间换过主机之后，相对路径会被接到错的站上。
fn absolute(from: &str, location: &str) -> String {
    if location.starts_with("https://") || location.starts_with("http://") {
        return location.to_string();
    }
    let base = from.strip_prefix("https://").unwrap_or(from);
    let host = base.split('/').next().unwrap_or(base);
    if let Some(rest) = location.strip_prefix('/') {
        format!("https://{host}/{rest}")
    } else {
        format!("https://{host}/{location}")
    }
}

impl Fetcher for HttpFetcher {
    fn head(&self, url: &str) -> Result<Head, FetchError> {
        let (head, _) = self.follow(Method::Head, url)?;
        require_ok(url, &head)?;
        Ok(head)
    }

    fn get(&self, url: &str) -> Result<Fetched, FetchError> {
        let (head, mut body) = self.follow(Method::Get, url)?;
        require_ok(url, &head)?;
        let bytes = body
            .with_config()
            .limit(MAX_BODY)
            .read_to_vec()
            .map_err(|error| FetchError::Transport {
                url: url.to_string(),
                detail: error.to_string(),
            })?;
        Ok(Fetched { head, body: bytes })
    }

    fn download(&self, url: &str, to: &Path) -> Result<Head, FetchError> {
        // 没人会来叫停的那些走这条：一路放行。
        self.download_while(url, to, &|| true)
    }

    fn download_while(
        &self,
        url: &str,
        to: &Path,
        keep_going: &dyn Fn() -> bool,
    ) -> Result<Head, FetchError> {
        let (head, mut body) = self.follow(Method::Get, url)?;
        require_ok(url, &head)?;
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|source| FetchError::Io {
                path: parent.display().to_string(),
                source,
            })?;
        }
        // 先写临时文件再改名：下到一半断网留下的半截文件会被当成「下好了」，
        // 而它的指纹已经记进库里，于是那份 DAT 永远不会被重下（挂账里那类静默坏账）。
        let temp = to.with_extension("partial");
        let label = temp.display().to_string();
        let mut file = std::fs::File::create(&temp).map_err(|source| FetchError::Io {
            path: label.clone(),
            source,
        })?;
        let mut reader = body.with_config().limit(MAX_BODY).reader();
        // **一块一块地搬，每块之前问一次**：那 435 MB 里按下停下，收手的代价是读完
        // 手上这一块，不是把剩下那 400 多 MB 读完。
        let 搬完了 = copy_while(&mut reader, &mut file, &label, keep_going);
        drop(file);
        if let Err(error) = 搬完了 {
            // **半截的不留。** 收手也好、断网也好，那个 `.partial` 留在缓存里
            // 只会让下一趟以为原件在手边（`zh::sync` 就是按文件在不在决定要不要下的）。
            let _ = std::fs::remove_file(&temp);
            return Err(error);
        }
        std::fs::rename(&temp, to).map_err(|source| FetchError::Io {
            path: to.display().to_string(),
            source,
        })?;
        Ok(head)
    }

    fn post(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> Result<Fetched, FetchError> {
        let host = guard::check(url)?;
        self.wait_turn(host);
        let mut request = self.agent.post(url);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = request.send(body).map_err(|error| FetchError::Transport {
            url: url.to_string(),
            detail: error.to_string(),
        })?;
        let (parts, mut payload) = response.into_parts();
        let mut got = BTreeMap::new();
        for (name, value) in &parts.headers {
            if let Ok(text) = value.to_str() {
                got.insert(name.as_str().to_ascii_lowercase(), text.to_string());
            }
        }
        let head = Head {
            status: parts.status.as_u16(),
            headers: got,
        };
        // **状态码走同一道 `require_ok`**：假服务器与真服务器对 4xx 的处置必须逐字一致
        // （见 [`CannedFetcher`] 的文档），而这一层唯一能跑起来的验证场就是假服务器。
        require_ok(url, &head)?;
        let bytes = payload
            .with_config()
            .limit(MAX_BODY)
            .read_to_vec()
            .map_err(|error| FetchError::Transport {
                url: url.to_string(),
                detail: error.to_string(),
            })?;
        Ok(Fetched { head, body: bytes })
    }
}

/// **测试替身**：备好的响应从这里发，一个字节都不上网。
///
/// 它与 [`MemFs`](crate::fs::MemFs) 是同一个用法：同步编排的全部逻辑挂在纯计算上，
/// 测试用它把「服务器会怎么答」直接摆出来。
///
/// ## 它必须和真的一样处置状态码
///
/// [`with_status`](Self::with_status) 备得下 429 / 430 / 431 这些码，而且**返回的形状
/// 与 [`HttpFetcher`] 一模一样**（走同一个 [`require_ok`]）。差一点都不行：在线刮削档
/// 的硬停止全靠认出这几个码，而真实凭据这一趟拿不到——**假服务器是这件事唯一能跑起来的
/// 验证场**，它若比真的宽容，验的就是另一个东西。
#[derive(Debug, Default)]
pub struct CannedFetcher {
    canned: BTreeMap<String, (Head, Vec<u8>)>,
    /// 按**前缀**匹配的那些。真 API 的查询串里带着凭据与逐条参数，整条 URL
    /// 对不上，而「这个端点回什么」正是要摆出来的东西。
    prefixed: Vec<(String, Head, Vec<u8>)>,
    /// 问过哪些 URL，按顺序。测试据此断言「没有多发请求」。
    asked: Mutex<Vec<String>>,
    /// [`Fetcher::post`] 发出去的正文，按顺序。**只记正文，不记请求头**——
    /// 头里装着凭据，录下来就等于把它写进测试的失败输出里。
    posted: Mutex<Vec<Vec<u8>>>,
}

impl CannedFetcher {
    /// 起一个空的。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 备一条：这个 URL 回这些字节。
    #[must_use]
    pub fn with(mut self, url: &str, body: impl Into<Vec<u8>>) -> Self {
        self.canned.insert(
            url.to_string(),
            (
                Head {
                    status: 200,
                    headers: BTreeMap::new(),
                },
                body.into(),
            ),
        );
        self
    }

    /// 备一条，连响应头一起。
    #[must_use]
    pub fn with_headers(
        mut self,
        url: &str,
        headers: &[(&str, &str)],
        body: impl Into<Vec<u8>>,
    ) -> Self {
        let head = Head {
            status: 200,
            headers: headers
                .iter()
                .map(|(name, value)| (name.to_ascii_lowercase(), (*value).to_string()))
                .collect(),
        };
        self.canned.insert(url.to_string(), (head, body.into()));
        self
    }

    /// 备一条：这个 URL 回这个状态码与这段正文。
    ///
    /// 备 4xx / 5xx 时，取回来的形状与真服务器一致——[`Fetcher::get`] 返回
    /// [`FetchError::Status`]，状态码原样带着。
    #[must_use]
    pub fn with_status(mut self, url: &str, status: u16, body: impl Into<Vec<u8>>) -> Self {
        self.canned.insert(
            url.to_string(),
            (
                Head {
                    status,
                    headers: BTreeMap::new(),
                },
                body.into(),
            ),
        );
        self
    }

    /// 备一条：**以这一段开头**的 URL 都回这个状态码与这段正文。
    ///
    /// 真 API 的查询串里带着凭据与逐条参数，整条 URL 在测试里写不出来；而要摆出来的
    /// 本来就是「这个端点会怎么答」。**整条匹配的那些优先**，所以一个端点可以先定一个
    /// 默认答案、再给某几条 URL 单独备一个。
    #[must_use]
    pub fn with_prefix(mut self, prefix: &str, status: u16, body: impl Into<Vec<u8>>) -> Self {
        self.prefixed.push((
            prefix.to_string(),
            Head {
                status,
                headers: BTreeMap::new(),
            },
            body.into(),
        ));
        self
    }

    /// 一共问过哪些 URL。
    #[must_use]
    pub fn asked(&self) -> Vec<String> {
        self.asked
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// [`Fetcher::post`] 一共发出去哪些正文，按顺序。
    ///
    /// **批量打包这件事只能从这里验**（票 12）：「一次给若干条」不是看发了几个请求，
    /// 而是看**一个正文里装着几条**——两者只有对着正文才分得开。
    #[must_use]
    pub fn posted(&self) -> Vec<Vec<u8>> {
        self.posted
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn look_up(&self, url: &str) -> Result<(&Head, &Vec<u8>), FetchError> {
        self.asked
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(url.to_string());
        if let Some((head, body)) = self.canned.get(url) {
            return Ok((head, body));
        }
        self.prefixed
            .iter()
            .find(|(prefix, _, _)| url.starts_with(prefix.as_str()))
            .map(|(_, head, body)| (head, body))
            .ok_or_else(|| FetchError::NotCanned {
                url: url.to_string(),
            })
    }
}

impl Fetcher for CannedFetcher {
    fn head(&self, url: &str) -> Result<Head, FetchError> {
        let (head, _) = self.look_up(url)?;
        require_ok(url, head)?;
        Ok(head.clone())
    }

    fn get(&self, url: &str) -> Result<Fetched, FetchError> {
        let (head, body) = self.look_up(url)?;
        require_ok(url, head)?;
        Ok(Fetched {
            head: head.clone(),
            body: body.clone(),
        })
    }

    fn download(&self, url: &str, to: &Path) -> Result<Head, FetchError> {
        let (head, body) = self.look_up(url)?;
        require_ok(url, head)?;
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|source| FetchError::Io {
                path: parent.display().to_string(),
                source,
            })?;
        }
        std::fs::write(to, body).map_err(|source| FetchError::Io {
            path: PathBuf::from(to).display().to_string(),
            source,
        })?;
        Ok(head.clone())
    }

    fn post(
        &self,
        url: &str,
        _headers: &[(&str, &str)],
        body: &[u8],
    ) -> Result<Fetched, FetchError> {
        self.posted
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(body.to_vec());
        let (head, canned) = self.look_up(url)?;
        require_ok(url, head)?;
        Ok(Fetched {
            head: head.clone(),
            body: canned.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    /// 中文离线源那份 dump 有多大——`aux/latest.json` 上写的那个数，一个字节不差。
    const 那份原件多大: u64 = 435_891_841;

    /// 一份**读得出那么多字节**的正文：不进内存、不上网，只是一直给字节。
    ///
    /// **它每次都填满整个 buf。** 真的 `Read`（socket）短读是合法的，那时
    /// `copy_while` 照旧对——只是「搬进去正好几块」那个精确的等号变成了「不超过几块」。
    /// 底下那条测试两样都断，`<=` 那一条才是判据。
    struct 一份大正文 {
        剩下: u64,
    }

    impl io::Read for 一份大正文 {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = buf
                .len()
                .min(usize::try_from(self.剩下).unwrap_or(usize::MAX));
            buf[..n].fill(0);
            self.剩下 -= n as u64;
            Ok(n)
        }
    }

    /// 一个**只数不留**的落点：搬进去多少字节数得出来，一个字节都不占内存。
    #[derive(Default)]
    struct 数着丢掉 {
        搬进去了: u64,
    }

    impl io::Write for 数着丢掉 {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.搬进去了 += buf.len() as u64;
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn 那份_435_兆的原件按停之后在当前这一块读完就收手_不等整份下完() {
        // 挂单 `Q150`：这份 dump 一口气下完要几分钟，而那几分钟里「停下」按下去没有
        // 反应，等于摆了个假按钮。
        //
        // **判据是搬进去了多少字节，不是花了多久**：时间随机器与线路飘，而
        // 「收手时搬进去的只有放行过的那几块」是这条循环的定义，换一份多大的正文、
        // 换一条多快的线路都成立。

        // 先量一遍「不按停」那一趟：整份 435 MB 一个字节不少地搬完。
        let mut 落点 = 数着丢掉::default();
        let 开始 = Instant::now();
        let 搬完 = copy_while(
            &mut 一份大正文 {
                剩下: 那份原件多大
            },
            &mut 落点,
            "缓存里那份原件",
            &|| true,
        )
        .expect("没人叫停就该整份搬完");
        let 下完用了 = 开始.elapsed();
        assert_eq!(搬完, 那份原件多大);
        assert_eq!(落点.搬进去了, 那份原件多大);

        // 再量一遍「按了停下」那一趟：放行三块，第四次问就收手。
        let 放行 = AtomicU64::new(3);
        let mut 落点 = 数着丢掉::default();
        let 开始 = Instant::now();
        let 结果 = copy_while(
            &mut 一份大正文 {
                剩下: 那份原件多大
            },
            &mut 落点,
            "缓存里那份原件",
            &|| 放行.fetch_sub(1, Ordering::Relaxed) > 0,
        );
        let 收手用了 = 开始.elapsed();
        assert!(
            matches!(结果, Err(FetchError::Halted(_))),
            "按了停下却不是「被叫停」那一档：{结果:?}",
        );

        // **收手的代价是当前这一块，不是剩下那 400 多 MB。**
        let 一块 = DOWNLOAD_CHUNK as u64;
        // 这一条是**判据**：放行几次，搬进去的就不超过几块——换一份多大的正文、
        // 换一条会短读的线路都成立。
        assert!(
            (0 < 落点.搬进去了) && (落点.搬进去了 <= 3 * 一块),
            "放行了三块，搬进去的却是 {} 字节——收手没落在块与块之间",
            落点.搬进去了,
        );
        // 这一条精确到等号，靠的是 `一份大正文` 每次填满整个 buf（见它的文档）。
        assert_eq!(落点.搬进去了, 3 * 一块);
        assert!(
            落点.搬进去了 * 100 < 那份原件多大,
            "按停之后还是把大半份搬完了：{} / {那份原件多大}",
            落点.搬进去了,
        );
        eprintln!(
            "435 MB 整份搬完 {下完用了:?}；按停之后搬了 3 块（{} 字节）就收手，\
             用了 {收手用了:?}（合成正文，内存速度）。收手的上界是**读完当前这一块**\
             ——{} 字节，换算到一条 20 MB/s 的线路上约 50 ms，\
             而从前要等的是剩下那 430 MB，约 21 秒。",
            3 * 一块,
            一块,
        );
    }

    #[test]
    fn 一块搬不完的正文也搬得完() {
        // 一块是 1 MiB，而正文不会正好是它的整数倍——最后那不足一块的一段照旧要搬进去。
        let 零头 = DOWNLOAD_CHUNK as u64 * 2 + 7;
        let mut 落点 = Vec::new();
        let 搬完 = copy_while(&mut 一份大正文 { 剩下: 零头 }, &mut 落点, "零头", &|| true)
            .expect("搬得完");
        assert_eq!(搬完, 零头);
        assert_eq!(落点.len() as u64, 零头);
    }

    /// **真的那一个也过闸门。**
    ///
    /// 假服务器的 `post` 不查闸门（与既有的 `get` / `head` 一致——测试替身不该替
    /// 生产代码守边界），于是 [`HttpFetcher::post`] 里那一行 `guard::check` 漏掉了
    /// 假服务器**测不出来**。这条测试就是补那个缺口：它一个字节都不上网——
    /// 闸门在任何连接之前就拦下了。
    #[test]
    fn 真的那个_post_也过闸门() {
        let error = HttpFetcher::new()
            .post("https://example.com/v1/messages", &[], b"{}")
            .expect_err("白名单之外该拦下");
        assert!(matches!(error, FetchError::Refused(_)), "{error:?}");
        assert!(format!("{error}").contains("example.com"));
        // 点名拒绝的那些同样拦得住。
        let error = HttpFetcher::new()
            .post("https://datomatic.no-intro.org/x", &[], b"{}")
            .expect_err("点名拒绝的该拦下");
        assert!(matches!(error, FetchError::Refused(_)), "{error:?}");
    }

    fn head_with(value: &str) -> Head {
        Head {
            status: 200,
            headers: [("content-disposition".to_string(), value.to_string())]
                .into_iter()
                .collect(),
        }
    }

    #[test]
    fn 从响应头里认出_redump_的版本() {
        // Redump 不给 ETag 也不给 Last-Modified，这个头就是全部的增量依据。
        let head = head_with(
            "attachment; filename=\"Sony - PlayStation - Datfile (10974) (2026-08-31 07-59-02).zip\"",
        );
        assert_eq!(
            head.attachment_name(),
            Some("Sony - PlayStation - Datfile (10974) (2026-08-31 07-59-02).zip")
        );
    }

    #[test]
    fn 不带引号的附件名也认() {
        assert_eq!(
            head_with("attachment; filename=PSX.zip").attachment_name(),
            Some("PSX.zip")
        );
        assert_eq!(Head::default().attachment_name(), None);
    }

    #[test]
    fn 响应头名字大小写不敏感() {
        let head = head_with("attachment; filename=\"x.zip\"");
        assert_eq!(
            head.get("Content-Disposition"),
            head.get("content-disposition")
        );
    }

    #[test]
    fn 相对跳转接回本跳的主机而不是起点() {
        assert_eq!(
            absolute("https://github.com/a/b/releases", "/x/y.zip"),
            "https://github.com/x/y.zip"
        );
        assert_eq!(
            absolute("https://objects.githubusercontent.com/p", "q"),
            "https://objects.githubusercontent.com/q"
        );
        assert_eq!(
            absolute(
                "https://github.com/a",
                "https://objects.githubusercontent.com/z"
            ),
            "https://objects.githubusercontent.com/z"
        );
    }

    fn redirect_to(location: &str) -> Head {
        Head {
            status: 302,
            headers: [("location".to_string(), location.to_string())]
                .into_iter()
                .collect(),
        }
    }

    #[test]
    fn 一个_302_也送不到禁区去() {
        // 闸门挂在**每一跳**上，不只是起点。镜像哪天被改成 302 跳到 datomatic，
        // 一次请求就够触发永久封禁——这条断言就是那道防线。
        let refused = next_hop(
            "https://github.com/x/y",
            &redirect_to("https://datomatic.no-intro.org/index.php?page=download"),
        )
        .expect_err("该拦下");
        assert!(matches!(refused, FetchError::Refused(Refusal::Datomatic)));

        let frozen = next_hop(
            "https://github.com/x/y",
            &redirect_to("https://redump.org/datfile/psx"),
        )
        .expect_err("该拦下");
        assert!(matches!(frozen, FetchError::Refused(Refusal::FrozenRedump)));
    }

    #[test]
    fn 正常的一跳照走_到头就停() {
        let next = next_hop(
            "https://github.com/x/y",
            &redirect_to("https://objects.githubusercontent.com/z"),
        )
        .expect("该放行");
        assert_eq!(
            next.as_deref(),
            Some("https://objects.githubusercontent.com/z")
        );

        let done = next_hop(
            "https://redump.info/datfile/PSX",
            &Head {
                status: 200,
                headers: BTreeMap::new(),
            },
        )
        .expect("2xx 就是到头了");
        assert_eq!(done, None);

        // 404 也是「走到头」——**状态码好不好由 `require_ok` 说**，分开是因为在线档
        // 要认出 430 / 431 那两个码本身。
        let stopped = next_hop(
            "https://redump.info/datfile/NOPE",
            &Head {
                status: 404,
                headers: BTreeMap::new(),
            },
        )
        .expect("走到头了");
        assert_eq!(stopped, None);
    }

    #[test]
    fn 状态码原样交上来_而不是折成一句错误文本() {
        // 在线刮削档靠 430 / 431 分辨「今天别再打了」与「今天别再打没识别的了」，
        // 而这两个的处置都是**硬停止**。少了这一条，两个码都会退化成一句
        // 「取数失败」，于是被当成可以重试的网络抖动——那正是会被永久封禁的走法。
        for status in [404_u16, 429, 430, 431] {
            let url = format!("https://api.screenscraper.fr/api2/jeuInfos.php?n={status}");
            let fetcher = CannedFetcher::new().with_status(&url, status, Vec::new());
            let error = fetcher.get(&url).expect_err("非 2xx 该是错误");
            assert!(
                matches!(error, FetchError::Status { status: got, .. } if got == status),
                "{status} 该原样带回来，实际是 {error}"
            );
        }
    }

    #[test]
    fn 测试替身记下问过哪些() {
        let fetcher = CannedFetcher::new().with("https://redump.info/downloads/", b"hi".to_vec());
        assert_eq!(
            fetcher.get("https://redump.info/downloads/").unwrap().body,
            b"hi"
        );
        assert!(fetcher.get("https://redump.info/datfile/PSX").is_err());
        assert_eq!(
            fetcher.asked(),
            vec![
                "https://redump.info/downloads/".to_string(),
                "https://redump.info/datfile/PSX".to_string()
            ]
        );
    }
}
