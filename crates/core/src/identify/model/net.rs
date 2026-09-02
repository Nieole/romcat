//! **网络句柄**：模型推断这一层唯一发得出请求的地方。
//!
//! 形状照票 14 的 `scrape::online::Net` 抄——限流、上限、硬停止收在一处，而不是散在
//! 调用方那边。调用方拿不到 [`Fetcher`]，只拿得到 [`Inference`]，于是绕不开这三道闸。
//!
//! ## 停下来**不是错误**
//!
//! 与票 14 同一条：跑到 80% 撞上上限就把那 80% 一起丢掉，是最不该发生的事——
//! 而这一层丢掉的是**已经付过钱的答案**。所以 [`Halt`] 不是 `Error`：撞上就先把手里
//! 那批答案落库、照常出报告，再在报告里说清为什么停。
//!
//! ## 凭据只有一套，而且读不到就整层不启动
//!
//! 一趟一个 [`Inference`]，一个 [`Inference`] 一套凭据——轮换在类型上就做不出来。
//! 读不到凭据时**不匿名试探**：没有凭据的请求必然 401，而那一发对谁都没有好处。

use std::cell::RefCell;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::dat::{FetchError, Fetcher, guard};
use crate::scan::CancelToken;

use super::{
    API_VERSION, ENDPOINT, Limits, MAX_BACKOFF_TRIES, MAX_CONSECUTIVE_FAILURES, Price, dollars,
};

/// 一个状态码该怎么处置。**纯函数**，一个字节都不用联网就测得动。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// 收下。
    Ok,
    /// 可恢复：退避之后重试，**次数有限**。
    Backoff,
    /// **硬停止**。不重试、不换凭据。
    Fatal(&'static str),
}

/// 这个状态码怎么处置。
///
/// 口径与票 14 的 `scrape::online::classify` 同源，只是这一头的码表不同：
///
/// | 码 | 处置 | 为什么 |
/// |---|---|---|
/// | 2xx | 收下 | |
/// | 429 | **退避** | 撞上速率限制，等一会儿是对的 |
/// | 500–599 | **退避** | 对面的事，等一会儿 |
/// | 400 | **停** | 请求形状不对。重发一模一样的东西只会再错一次 |
/// | 401 / 403 | **停** | 凭据不对或没权限。**绝不换一套再试** |
/// | 404 | **停** | 落点或模型名不对，不是「这一条没有」 |
/// | 413 | **停** | 一批装太多了。改 `--model-batch` 再来，不是重试 |
/// | 别的 | **停** | 没见过的码一律停——这一层每一发都花钱，代价不对称 |
///
/// 注意 404 在这里是**硬停止**而不是「查过、没有」：刮削那一头 404 说的是「这个游戏
/// 我没有」，是一条结论；这一头 404 说的是「你问的这个落点或模型不存在」，接着问
/// 只是把同一个错误再犯几百遍。
#[must_use]
pub fn classify(status: u16) -> Verdict {
    match status {
        200..=299 => Verdict::Ok,
        429 => Verdict::Backoff,
        500..=599 => Verdict::Backoff,
        400 => Verdict::Fatal(
            "请求形状不对（400）：多半是 `output_config` 里的 `format` 或 `effort` \
             这个模型不认，也可能是 `max_tokens` 超了它的上限。\
             换 `--model-id` 或调 `--model-max-tokens` 再来，别重发同一份",
        ),
        401 => Verdict::Fatal(
            "凭据不对（401）：ANTHROPIC_API_KEY 或 ANTHROPIC_AUTH_TOKEN 没设对。\
             **不换一套再试**",
        ),
        403 => Verdict::Fatal("这套凭据没有权限（403）"),
        404 => Verdict::Fatal("落点或模型名不存在（404）：`--model-id` 写对了吗"),
        413 => Verdict::Fatal("一个请求装太多了（413）：把 `--model-batch` 调小再来"),
        _ => Verdict::Fatal("没见过的状态码——停下来问人比接着试安全"),
    }
}

/// 这一趟为什么停了。**它不是错误**，见模块文档。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Halt {
    /// 自设的请求个数用完了。
    Budget {
        /// 发了几个。
        spent: u64,
        /// 上限是几个。
        cap: u64,
    },
    /// 自设的花费上限到了。
    Spend {
        /// 已经花了多少微美元。
        spent: u64,
        /// 上限是多少微美元。
        cap: u64,
    },
    /// 对面不让了。
    Fatal {
        /// 为什么。
        why: &'static str,
    },
    /// 退避到头了。
    Throttled {
        /// 退了几次。
        tries: u32,
    },
    /// 网断了。
    Offline {
        /// 连着失败几次。
        tries: u32,
    },
}

impl Halt {
    /// 写成给人看的一句。
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Budget { spent, cap } => format!(
                "这一趟停了：自设的请求上限用完了（发了 {spent} 个，上限 {cap} 个）。\
                 **已经问到的答案都落库了**，立刻重跑就接着问下一批（问过的不再问一遍）。"
            ),
            Self::Spend { spent, cap } => format!(
                "这一趟停了：自设的花费上限到了（已经花了 {}，上限 {}）。\
                 **已经问到的答案都落库了**，抬高 `--model-max-spend` 重跑就接着问。",
                dollars(*spent),
                dollars(*cap)
            ),
            Self::Fatal { why } => format!("这一趟停了：{why}。重跑没用，先把它解决掉。"),
            Self::Throttled { tries } => format!(
                "这一趟停了：连着退避 {tries} 次还是被限速。等一会儿再来，\
                 或者把 `--model-interval-ms` 调大。"
            ),
            Self::Offline { tries } => {
                format!("这一趟停了：连着 {tries} 次连不上。网断了，已经问到的答案都在库里。")
            }
        }
    }
}

/// 停下来的两种，脚本要分得开。
///
/// 与票 14 的退出码 3 / 4 同一条道理，只是这一层赌的是**钱**而不是配额：
/// 「自己收的手」立刻重跑就接着问（问过的不再问一遍），「对面不让了」重跑没用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum HaltKind {
    /// **自己收的手**：请求个数或花费的自设上限到了。抬高上限重跑就接着问。
    #[serde(rename = "自己收的手")]
    Self_,
    /// **对面不让了**：凭据被拒、请求形状不对、限速到头、网断了。重跑没用。
    #[serde(rename = "对面不让了")]
    Refused,
}

impl Halt {
    /// 这是哪一种停。
    #[must_use]
    pub fn kind(&self) -> HaltKind {
        match self {
            Self::Budget { .. } | Self::Spend { .. } => HaltKind::Self_,
            Self::Fatal { .. } | Self::Throttled { .. } | Self::Offline { .. } => HaltKind::Refused,
        }
    }
}

/// 一次提问没成，是哪一种。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// 整趟停了。
    Halt(Halt),
    /// 只有这一批没问成，别的照问。
    Skip {
        /// 为什么。
        why: String,
    },
}

/// 凭据。**一趟一套，没有第二处能装。**
#[derive(Clone)]
pub struct Credentials {
    key: String,
    bearer: bool,
}

/// 凭据从哪几个环境变量读。
pub const ENV_KEYS: [&str; 2] = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"];

impl std::fmt::Debug for Credentials {
    /// **不印出凭据本身。** 这个类型会跟着 [`Inference`] 出现在各种调试输出里，
    /// 而那些输出会被贴进工单、日志与测试的失败信息。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("kind", &if self.bearer { "bearer" } else { "api-key" })
            .field("key", &"（不印）")
            .finish()
    }
}

impl Credentials {
    /// 从环境变量读那**一套**。两个都没有就是 `None`，而 `None` 让整层不启动。
    ///
    /// `ANTHROPIC_API_KEY` 优先；没有它时用 `ANTHROPIC_AUTH_TOKEN`，
    /// 那是一个 OAuth 令牌，走 `Authorization: Bearer` 而不是 `x-api-key`，
    /// 并且要多带一个 beta 头——**两种放错了头都是 401**。
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let get = |key: &str| std::env::var(key).ok().filter(|value| !value.is_empty());
        if let Some(key) = get(ENV_KEYS[0]) {
            return Some(Self { key, bearer: false });
        }
        get(ENV_KEYS[1]).map(|key| Self { key, bearer: true })
    }

    /// 直接给一套（测试用）。
    #[must_use]
    pub fn api_key(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            bearer: false,
        }
    }

    /// 这一发要带哪些请求头。**凭据只出现在这里，正文里一个字都没有。**
    #[must_use]
    fn headers(&self) -> Vec<(&'static str, String)> {
        let mut out = vec![
            ("content-type", "application/json".to_string()),
            ("anthropic-version", API_VERSION.to_string()),
        ];
        if self.bearer {
            out.push(("authorization", format!("Bearer {}", self.key)));
            // OAuth 令牌要它，API key 不要。
            out.push(("anthropic-beta", "oauth-2025-04-20".to_string()));
        } else {
            out.push(("x-api-key", self.key.clone()));
        }
        out
    }
}

/// 这一趟在网络上的账。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct Usage {
    /// 发了几个请求。
    pub requests: u64,
    /// 输入 token（响应里 `usage` 说的真数字）。
    pub input_tokens: u64,
    /// 输出 token。
    pub output_tokens: u64,
    /// 花了多少微美元。
    pub cost_micros: u64,
    /// 有几个 URL 被**取数闸门**拦下、没有发出去。正常是 0。
    pub refused: u64,
}

#[derive(Debug, Default)]
struct State {
    usage: Usage,
    last: Option<Instant>,
    fails: u32,
    halted: Option<Halt>,
}

/// **网络句柄**：这一层要发请求，只能经过它。
pub struct Inference<'a> {
    fetcher: &'a dyn Fetcher,
    limits: Limits,
    price: Price,
    credentials: Credentials,
    cancel: &'a CancelToken,
    state: RefCell<State>,
}

impl<'a> Inference<'a> {
    /// 起一个。**一趟一个**——于是一趟一套凭据。
    #[must_use]
    pub fn new(
        fetcher: &'a dyn Fetcher,
        limits: Limits,
        price: Price,
        credentials: Credentials,
        cancel: &'a CancelToken,
    ) -> Self {
        Self {
            fetcher,
            limits,
            price,
            credentials,
            cancel,
            state: RefCell::new(State::default()),
        }
    }

    /// 这一趟的账。
    #[must_use]
    pub fn usage(&self) -> Usage {
        self.state.borrow().usage
    }

    /// 停了没有，为什么。
    #[must_use]
    pub fn halted(&self) -> Option<Halt> {
        self.state.borrow().halted.clone()
    }

    /// 发一个请求，把响应的 JSON 交回来。
    ///
    /// # Errors
    /// 撞上任何一道闸时返回 [`Failure::Halt`]；只有这一批没成时返回 [`Failure::Skip`]。
    pub fn ask(&self, body: &Value) -> Result<Value, Failure> {
        // 一旦停了就再也不发。
        if let Some(halt) = self.halted() {
            return Err(Failure::Halt(halt));
        }
        // **走已有的闸门，不另开一条路**（`dat::guard`）。
        if let Err(refusal) = guard::check(ENDPOINT) {
            self.state.borrow_mut().usage.refused += 1;
            return Err(Failure::Skip {
                why: format!("{refusal}"),
            });
        }
        let payload = serde_json::to_vec(body).map_err(|error| Failure::Skip {
            why: format!("请求正文拼不出来：{error}"),
        })?;
        let headers = self.credentials.headers();
        let headers: Vec<(&str, &str)> = headers
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        let mut tries = 0_u32;
        loop {
            // **每一发都过一次闸，退避重发的那几发也算。**
            self.charge()?;
            self.wait_turn();
            self.state.borrow_mut().usage.requests += 1;
            match self.fetcher.post(ENDPOINT, &headers, &payload) {
                Ok(fetched) => {
                    self.state.borrow_mut().fails = 0;
                    let json: Value =
                        serde_json::from_slice(&fetched.body).map_err(|error| Failure::Skip {
                            why: format!("响应不是 JSON：{error}"),
                        })?;
                    self.charge_tokens(&json);
                    return Ok(json);
                }
                Err(FetchError::Status { status, .. }) => match classify(status) {
                    Verdict::Ok => {
                        return Err(Failure::Skip {
                            why: format!("状态码 {status} 判成可以收下，但正文没拿到"),
                        });
                    }
                    Verdict::Backoff => {
                        tries += 1;
                        if tries > MAX_BACKOFF_TRIES {
                            return Err(self.halt(Halt::Throttled { tries }));
                        }
                        self.nap(self.limits.backoff);
                    }
                    Verdict::Fatal(why) => return Err(self.halt(Halt::Fatal { why })),
                },
                Err(FetchError::Refused(refusal)) => {
                    self.state.borrow_mut().usage.refused += 1;
                    return Err(Failure::Skip {
                        why: format!("{refusal}"),
                    });
                }
                Err(error) => {
                    let fails = {
                        let mut state = self.state.borrow_mut();
                        state.fails += 1;
                        state.fails
                    };
                    if fails >= MAX_CONSECUTIVE_FAILURES {
                        return Err(self.halt(Halt::Offline { tries: fails }));
                    }
                    return Err(Failure::Skip {
                        why: format!("{error}"),
                    });
                }
            }
        }
    }

    /// 响应里的 `usage` 是**真数字**，账按它记，不按估的记。
    fn charge_tokens(&self, response: &Value) {
        let usage = response.get("usage");
        let read = |name: &str| {
            usage
                .and_then(|it| it.get(name))
                .and_then(Value::as_u64)
                .unwrap_or(0)
        };
        // 缓存读写那两笔也是输入 token，服务端分开报，不算进去就少记钱。
        let input = read("input_tokens")
            + read("cache_creation_input_tokens")
            + read("cache_read_input_tokens");
        let output = read("output_tokens");
        let mut state = self.state.borrow_mut();
        state.usage.input_tokens += input;
        state.usage.output_tokens += output;
        state.usage.cost_micros += self.price.cost_micros(input, output);
    }

    /// 发出去之前过的那两道闸：请求个数与花费。
    ///
    /// **花费卡的是已经花掉的那个数**：加到头就停，因此最多超出一个请求的顶
    /// （[`Plan::ceiling_per_request_micros`](super::Plan::ceiling_per_request_micros)，
    /// 计划里印得出来）。要在发之前就按最坏情况卡，一趟能发的请求数会缩到十几个，
    /// 而实测每个请求的实际花费往往只有那个顶的几十分之一——那不是「设了上限」，
    /// 是「几乎不让跑」。
    fn charge(&self) -> Result<(), Failure> {
        let (requests, spent) = {
            let state = self.state.borrow();
            (state.usage.requests, state.usage.cost_micros)
        };
        if requests >= self.limits.budget {
            return Err(self.halt(Halt::Budget {
                spent: requests,
                cap: self.limits.budget,
            }));
        }
        if spent >= self.limits.spend_cap_micros {
            return Err(self.halt(Halt::Spend {
                spent,
                cap: self.limits.spend_cap_micros,
            }));
        }
        Ok(())
    }

    fn halt(&self, halt: Halt) -> Failure {
        self.state.borrow_mut().halted = Some(halt.clone());
        Failure::Halt(halt)
    }

    /// 到点了再发下一个。
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
    /// 直接 `sleep(5s)` 退避三次，用户按下 Ctrl-C 之后要干等 15 秒才有反应——
    /// 而这一档正是最可能让人想中途叫停的那一档（它在花钱）。
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dat::CannedFetcher;

    fn 价() -> Price {
        Price {
            input_per_mtok: 500,
            output_per_mtok: 2500,
        }
    }

    fn 宽松() -> Limits {
        Limits {
            interval: Duration::from_millis(0),
            backoff: Duration::from_millis(0),
            ..Limits::default()
        }
    }

    fn 一份好答复() -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "content": [{ "type": "text", "text": "{\"答案\":[]}" }],
            "usage": { "input_tokens": 1000, "output_tokens": 200 }
        }))
        .expect("造得出")
    }

    #[test]
    fn 状态码的处置是一张明码表() {
        assert_eq!(classify(200), Verdict::Ok);
        assert_eq!(classify(429), Verdict::Backoff);
        assert_eq!(classify(529), Verdict::Backoff);
        assert!(matches!(classify(400), Verdict::Fatal(_)));
        assert!(matches!(classify(401), Verdict::Fatal(_)));
        // 404 在这一头**不是**「查过、没有」——刮削那一头才是。
        assert!(matches!(classify(404), Verdict::Fatal(_)));
        assert!(matches!(classify(418), Verdict::Fatal(_)));
    }

    #[test]
    fn 凭据不进正文只进请求头() {
        let fetcher = CannedFetcher::new().with_prefix(ENDPOINT, 200, 一份好答复());
        let cancel = CancelToken::new();
        let net = Inference::new(
            &fetcher,
            宽松(),
            价(),
            Credentials::api_key("绝密口令"),
            &cancel,
        );
        net.ask(&serde_json::json!({ "model": "x" })).expect("该成");
        let posted = fetcher.posted();
        assert_eq!(posted.len(), 1);
        let body = String::from_utf8_lossy(&posted[0]);
        assert!(!body.contains("绝密口令"), "凭据绝不许出现在正文里");
    }

    #[test]
    fn 凭据不进调试输出() {
        // 这个类型会跟着句柄出现在各种调试输出里，而那些输出会被贴进工单与日志。
        let text = format!("{:?}", Credentials::api_key("绝密口令"));
        assert!(!text.contains("绝密口令"), "{text}");
    }

    #[test]
    fn 硬停止之后一个请求都不再发() {
        let fetcher = CannedFetcher::new().with_prefix(ENDPOINT, 401, b"nope".to_vec());
        let cancel = CancelToken::new();
        let net = Inference::new(&fetcher, 宽松(), 价(), Credentials::api_key("x"), &cancel);
        let first = net.ask(&serde_json::json!({})).expect_err("该停");
        assert!(matches!(first, Failure::Halt(Halt::Fatal { .. })));
        let second = net.ask(&serde_json::json!({})).expect_err("还是停");
        assert!(matches!(second, Failure::Halt(Halt::Fatal { .. })));
        assert_eq!(
            fetcher.asked().len(),
            1,
            "凭据不对之后不许再发第二个——换一套再试正是这一层不做的事"
        );
    }

    #[test]
    fn 请求个数的上限挡得住() {
        let fetcher = CannedFetcher::new().with_prefix(ENDPOINT, 200, 一份好答复());
        let cancel = CancelToken::new();
        let limits = Limits {
            budget: 3,
            ..宽松()
        };
        let net = Inference::new(&fetcher, limits, 价(), Credentials::api_key("x"), &cancel);
        for _ in 0..3 {
            net.ask(&serde_json::json!({})).expect("前三个该成");
        }
        let halt = net.ask(&serde_json::json!({})).expect_err("第四个该停");
        assert!(matches!(halt, Failure::Halt(Halt::Budget { cap: 3, .. })));
        assert_eq!(fetcher.asked().len(), 3);
    }

    #[test]
    fn 花费的上限挡得住() {
        // 一发 1000 输入 + 200 输出 = 1000×500 + 200×2500 微美分 = 100 万微美分 = 10000 微美元。
        let fetcher = CannedFetcher::new().with_prefix(ENDPOINT, 200, 一份好答复());
        let cancel = CancelToken::new();
        let limits = Limits {
            budget: 100,
            spend_cap_micros: 25_000,
            ..宽松()
        };
        let net = Inference::new(&fetcher, limits, 价(), Credentials::api_key("x"), &cancel);
        for _ in 0..3 {
            net.ask(&serde_json::json!({})).expect("头三个该成");
        }
        // 花到 30000 微美元，过了 25000 的线，第四个不发。
        let halt = net.ask(&serde_json::json!({})).expect_err("该停");
        assert!(matches!(halt, Failure::Halt(Halt::Spend { .. })));
        assert_eq!(fetcher.asked().len(), 3);
        assert_eq!(net.usage().cost_micros, 30_000);
    }

    #[test]
    fn 退避有次数上限_到头就停() {
        let fetcher = CannedFetcher::new().with_prefix(ENDPOINT, 429, b"slow down".to_vec());
        let cancel = CancelToken::new();
        let net = Inference::new(&fetcher, 宽松(), 价(), Credentials::api_key("x"), &cancel);
        let halt = net.ask(&serde_json::json!({})).expect_err("该停");
        assert!(matches!(halt, Failure::Halt(Halt::Throttled { .. })));
        // 退避重发的那几发**也吃请求个数的上限**：一共 1 + 3 次。
        assert_eq!(
            fetcher.asked().len(),
            usize::try_from(1 + MAX_BACKOFF_TRIES).unwrap_or(4)
        );
    }

    #[test]
    fn 退避重发也吃自设的请求上限() {
        let fetcher = CannedFetcher::new().with_prefix(ENDPOINT, 429, Vec::new());
        let cancel = CancelToken::new();
        let limits = Limits {
            budget: 2,
            ..宽松()
        };
        let net = Inference::new(&fetcher, limits, 价(), Credentials::api_key("x"), &cancel);
        let halt = net.ask(&serde_json::json!({})).expect_err("该停");
        // 上限先到：**一次 429 退避三次是四发**，不记账的话自设上限当场漏掉三个。
        assert!(matches!(halt, Failure::Halt(Halt::Budget { cap: 2, .. })));
        assert_eq!(fetcher.asked().len(), 2);
    }

    #[test]
    fn 账按响应里的真数字记() {
        let fetcher = CannedFetcher::new().with_prefix(ENDPOINT, 200, 一份好答复());
        let cancel = CancelToken::new();
        let net = Inference::new(&fetcher, 宽松(), 价(), Credentials::api_key("x"), &cancel);
        net.ask(&serde_json::json!({})).expect("该成");
        let usage = net.usage();
        assert_eq!(usage.requests, 1);
        assert_eq!(usage.input_tokens, 1000);
        assert_eq!(usage.output_tokens, 200);
        assert_eq!(usage.cost_micros, 10_000);
    }

    #[test]
    fn 请求之间真的隔着那么久() {
        let fetcher = CannedFetcher::new().with_prefix(ENDPOINT, 200, 一份好答复());
        let cancel = CancelToken::new();
        let limits = Limits {
            interval: Duration::from_millis(120),
            backoff: Duration::from_millis(0),
            ..Limits::default()
        };
        let net = Inference::new(&fetcher, limits, 价(), Credentials::api_key("x"), &cancel);
        let at = Instant::now();
        for _ in 0..3 {
            net.ask(&serde_json::json!({})).expect("该成");
        }
        assert!(
            at.elapsed() >= Duration::from_millis(240),
            "三发之间该隔着两个间隔，实测 {:?}",
            at.elapsed()
        );
    }
}
