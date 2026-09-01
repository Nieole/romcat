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

/// 默认每个主机之间隔多久再发下一个请求。
pub const DEFAULT_THROTTLE: Duration = Duration::from_secs(1);

/// 单个响应最多收多少字节。No-Intro 那个整包实测 106 MB，留到 1 GiB。
const MAX_BODY: u64 = 1 << 30;

/// 最多跟几跳重定向。
const MAX_HOPS: usize = 5;

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
        let mut file = std::fs::File::create(&temp).map_err(|source| FetchError::Io {
            path: temp.display().to_string(),
            source,
        })?;
        let mut reader = body.with_config().limit(MAX_BODY).reader();
        io::copy(&mut reader, &mut file).map_err(|source| FetchError::Io {
            path: temp.display().to_string(),
            source,
        })?;
        drop(file);
        std::fs::rename(&temp, to).map_err(|source| FetchError::Io {
            path: to.display().to_string(),
            source,
        })?;
        Ok(head)
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
