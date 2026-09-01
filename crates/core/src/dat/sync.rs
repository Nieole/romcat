//! **同步**：列举 → 排计划 → 取 → 入库。
//!
//! 中间那一步是纯计算，也是这个模块的重心。[`plan`] 拿三样东西——源报出来有什么、
//! 库里已有什么指纹、数据源清单怎么映射——算出「这一趟要干什么」，一个字节都不发。
//! 于是「会不会碰 datomatic」「哪几份要重下」「哪几份根本不入库」在开工前就是可断言的
//! 事实，而不是跑完才知道。
//!
//! ## 增量落在「一件东西」上
//!
//! 每个源的最小取数单位不同，指纹的来源也就不同：
//!
//! | 源 | 一件东西 | 指纹 |
//! |---|---|---|
//! | No-Intro | 那个 106 MB 的整包 | 发行资产的 `updated_at` + 大小 |
//! | Redump | 一个系统 | `Content-Disposition` 里的附件名（带条目数与生成时刻） |
//! | TOSEC / MAME / GoodNES | 仓库里的一个文件 | git blob 的 sha |
//!
//! 后两类的指纹是**服务器算好的**，一次 HEAD 或一次 trees 调用就问得到全部，
//! 于是「没变的整件跳过」几乎不花代价。No-Intro 那一档粒度粗，是因为镜像只发整包——
//! 这是数据源的形态决定的，不是这里偷懒。
//!
//! ## 列举也要过闸门
//!
//! 列举仓库文件一律走 `git/trees?recursive=1`，并且**必须检查 `truncated`**。
//! `/contents` 在 1000 条处硬截断且不报错，调研中因此误判 TOSEC 缺少多个平台集；
//! `truncated` 是 trees 接口给出的、`/contents` 根本没有的那个诚实信号，不看它就等于
//! 把同一个坑换个地方再踩一遍。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::guard::{self, Refusal};
use super::registry::{Lookup, Mapped, Origin, Registry, Shape, Source};
use super::repo::{DatCounts, DatMeta, DatRepo, RepoError, Unit};
use super::{Convention, fetch, gamedb, logiqx, softlist};
use crate::container::{self, ReadPlan};
use crate::fs::RealFs;
use fetch::{FetchError, Fetcher};

/// 同步途中出的错。
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// 取数失败。
    #[error(transparent)]
    Fetch(#[from] FetchError),
    /// 闸门拦下了。
    #[error(transparent)]
    Refused(#[from] Refusal),
    /// DAT 库写不进。
    #[error(transparent)]
    Repo(#[from] RepoError),
    /// DAT 读不动。
    #[error(transparent)]
    Parse(#[from] logiqx::ParseError),
    /// 列举回来的东西读不动。
    #[error("{what} 的列举结果读不动：{detail}")]
    Listing {
        /// 在列举哪个源。
        what: String,
        /// 哪里不对。
        detail: String,
    },
    /// 列举被截断了。
    #[error(
        "{what} 的文件清单被截断了（trees 接口报 truncated）。\
         这正是 /contents 那个坑的另一种形态——清单不全就会漏掉整个平台的 DAT。\
         用「子树」把范围缩小，或者分批列举。"
    )]
    Truncated {
        /// 在列举哪个源。
        what: String,
    },
    /// 整包打不开。
    #[error("{path} 打不开：{detail}")]
    Bundle {
        /// 出问题的文件。
        path: String,
        /// 哪里不对。
        detail: String,
    },
    /// 清单里没有这个源。
    #[error("数据源清单里没有 {0} 这个源")]
    NoSuchSource(String),
}

/// 一个源报出来的、可取的一件东西。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Available {
    /// 源内唯一的名字：DAT 名、系统短码，或仓库内路径。**映射拿它去匹配。**
    pub name: String,
    /// 从哪儿取。
    pub url: String,
    /// 变没变的依据。**问不出来时是 `None`，那就每次都取。**
    ///
    /// 这里绝不能拿一个固定的占位串顶上：那样第一趟存进去之后，之后每一趟都
    /// 「指纹没变」，那份 DAT 再也不更新，而报告照报旧数——正是 ADR-0007
    /// 警告的「以旧数据误导」的形态，只是换了个地方发生。
    pub fingerprint: Option<String>,
}

/// 这一趟拿这件东西怎么办。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// 要取。
    Fetch,
    /// 指纹没变，整件跳过。
    UpToDate,
    /// 没有任何映射命中，不入库。这是范围边界，不是遗漏。
    Unmapped,
    /// 映射里明确不要，附理由。
    Excluded(String),
    /// 闸门拦下了。
    Refused(Refusal),
}

impl Action {
    /// 报告里写的那个词。
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Fetch => "取",
            Self::UpToDate => "已是最新",
            Self::Unmapped => "没映射",
            Self::Excluded(_) => "不要",
            Self::Refused(_) => "拦下",
        }
    }
}

/// 计划里的一条。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanItem {
    /// 哪个源。
    pub source: String,
    /// 这件东西叫什么。
    pub name: String,
    /// 从哪儿取。
    pub url: String,
    /// 指纹。问不出来时是 `None`，那就每次都取。
    pub fingerprint: Option<String>,
    /// 归哪个平台。整包那一档是 `None`——包里每份 DAT 各有各的平台。
    pub platform: Option<String>,
    /// 哈希口径。整包那一档同上。
    pub convention: Option<Convention>,
    /// 怎么办。
    pub action: Action,
}

/// 一个源这一趟的计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// 哪个源。
    pub source: String,
    /// 逐条。
    pub items: Vec<PlanItem>,
}

impl Plan {
    /// 要取几件。
    #[must_use]
    pub fn to_fetch(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.action == Action::Fetch)
            .count()
    }

    /// 已是最新的有几件。
    #[must_use]
    pub fn up_to_date(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.action == Action::UpToDate)
            .count()
    }

    /// 不入库的有几件（没映射 + 明确不要）。
    #[must_use]
    pub fn out_of_scope(&self) -> usize {
        self.items
            .iter()
            .filter(|item| matches!(item.action, Action::Unmapped | Action::Excluded(_)))
            .count()
    }
}

/// 排一个源这一趟的计划。**纯计算，不碰网络。**
///
/// `known` 是库里已有的「名字 → 指纹」；`full` 为真时无视指纹，全部重取。
#[must_use]
pub fn plan(
    source: &Source,
    available: &[Available],
    known: &BTreeMap<String, String>,
    registry: &Registry,
    full: bool,
) -> Plan {
    // 整包那一档：映射作用在包**里面**的每份 DAT 上，包本身没有平台。
    let bundle = source.origin.is_bundle();
    let items = available
        .iter()
        .map(|item| {
            let mapped = if bundle {
                None
            } else {
                registry.map(&source.name, &item.name)
            };
            let action = if let Err(refusal) = guard::check(&item.url) {
                // 闸门放在最前面：连不该连的地方，连「要不要」都不必问。
                Action::Refused(refusal)
            } else if bundle || mapped.is_some() {
                // 指纹问不出来（`None`）时永远算「变了」——见 `Available::fingerprint`。
                let unchanged = item
                    .fingerprint
                    .as_ref()
                    .is_some_and(|fingerprint| known.get(&item.name) == Some(fingerprint));
                if !full && unchanged {
                    Action::UpToDate
                } else {
                    Action::Fetch
                }
            } else {
                match registry.lookup(&source.name, &item.name) {
                    Lookup::Excluded(why) => Action::Excluded(why.to_string()),
                    Lookup::Unmapped | Lookup::Mapped(_) => Action::Unmapped,
                }
            };
            PlanItem {
                source: source.name.clone(),
                name: item.name.clone(),
                url: item.url.clone(),
                fingerprint: item.fingerprint.clone(),
                platform: mapped.map(|mapped| mapped.platform.to_string()),
                convention: mapped.map(|mapped| mapped.convention),
                action,
            }
        })
        .collect();
    Plan {
        source: source.name.clone(),
        items,
    }
}

// ════════════════════════════════════════════════════════════════════════
// 列举
// ════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
struct ReleaseJson {
    assets: Vec<AssetJson>,
}

#[derive(Debug, Deserialize)]
struct AssetJson {
    name: String,
    size: u64,
    updated_at: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct TreeJson {
    #[serde(default)]
    truncated: bool,
    #[serde(default)]
    tree: Vec<TreeEntryJson>,
}

#[derive(Debug, Deserialize)]
struct TreeEntryJson {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    sha: String,
}

/// 列举一个源手上有什么。**这一步碰网络。**
///
/// Redump 那一档要为每个**映射到平台的**系统各发一次 HEAD 才拿得到指纹（它既不给
/// `ETag` 也不给 `Last-Modified`）。因此列举需要看一眼清单——只问要的那十几个，
/// 不去挨个骚扰全部 106 个系统。
///
/// # Errors
/// 取不到、读不动，或者文件清单被截断时返回错误。
pub fn list(
    fetcher: &dyn Fetcher,
    source: &Source,
    registry: &Registry,
) -> Result<Vec<Available>, SyncError> {
    match &source.origin {
        Origin::ReleaseBundle { repo, asset } => {
            let url = format!("https://api.github.com/repos/{repo}/releases/latest");
            let body = fetcher.get(&url)?.body;
            let release: ReleaseJson =
                serde_json::from_slice(&body).map_err(|error| SyncError::Listing {
                    what: source.name.clone(),
                    detail: error.to_string(),
                })?;
            let found = release
                .assets
                .into_iter()
                .find(|candidate| &candidate.name == asset)
                .ok_or_else(|| SyncError::Listing {
                    what: source.name.clone(),
                    detail: format!("最新的发行里没有 {asset} 这个资产"),
                })?;
            Ok(vec![Available {
                name: found.name,
                url: found.browser_download_url,
                // 资产的 updated_at 与大小合起来当指纹。镜像每 24 小时重建一次，
                // 内容没变时这两个值也不变。
                fingerprint: Some(format!("{}/{}", found.updated_at, found.size)),
            }])
        }
        Origin::RedumpSite { site } => {
            let page = fetcher.get(&format!("{site}/downloads/"))?.body;
            let html = String::from_utf8_lossy(&page);
            let mut out = Vec::new();
            for code in datfile_codes(&html) {
                // 只问清单里要的那些：现站 106 个系统，挨个 HEAD 一遍既慢又不礼貌。
                if registry.map(&source.name, &code).is_none() {
                    continue;
                }
                let url = format!("{site}/datfile/{code}");
                let head = fetcher.head(&url)?;
                out.push(Available {
                    name: code,
                    url,
                    // 附件名里带条目数与生成时刻，是这个源唯一的指纹来源。
                    // **问不出来就是 `None`**，于是这一份每趟都重取——总好过
                    // 拿一个固定串顶上，那会让它从此再也不更新。
                    fingerprint: head.attachment_name().map(str::to_string),
                });
            }
            Ok(out)
        }
        Origin::RepoFiles {
            repo,
            reference,
            subtree,
        } => {
            let spec = match subtree {
                Some(subtree) => format!("{reference}:{subtree}"),
                None => reference.clone(),
            };
            let url = format!("https://api.github.com/repos/{repo}/git/trees/{spec}?recursive=1");
            let body = fetcher.get(&url)?.body;
            let tree: TreeJson =
                serde_json::from_slice(&body).map_err(|error| SyncError::Listing {
                    what: source.name.clone(),
                    detail: error.to_string(),
                })?;
            // **这一句是那个坑的解药。** `/contents` 截断了不说，trees 会说。
            if tree.truncated {
                return Err(SyncError::Truncated {
                    what: source.name.clone(),
                });
            }
            Ok(tree
                .tree
                .into_iter()
                .filter(|entry| entry.kind == "blob")
                .map(|entry| {
                    let full = match subtree {
                        Some(subtree) => format!("{subtree}/{}", entry.path),
                        None => entry.path.clone(),
                    };
                    Available {
                        name: entry.path,
                        url: format!(
                            "https://raw.githubusercontent.com/{repo}/{reference}/{}",
                            percent_encode(&full)
                        ),
                        fingerprint: Some(entry.sha),
                    }
                })
                .collect())
        }
    }
}

/// 从 Redump 的下载页里抠出系统短码。
///
/// 页面是普通 HTML，链接长这样：`href="/datfile/PSX"`。不引 HTML 解析器——
/// 要认的只有一种形状，而多一个依赖就多一份要跟着升级的东西。
fn datfile_codes(html: &str) -> Vec<String> {
    const MARK: &str = "href=\"/datfile/";
    let mut out: Vec<String> = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find(MARK) {
        rest = &rest[at + MARK.len()..];
        let end = rest.find('"').unwrap_or(rest.len());
        let code = rest[..end].trim_end_matches('/').to_string();
        if !code.is_empty() && !out.contains(&code) {
            out.push(code);
        }
    }
    out
}

/// 路径里的空格与非 ASCII 要转义才放得进 URL。TOSEC 的 DAT 名里全是空格与 `&`。
fn percent_encode(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(*byte as char);
            }
            other => {
                out.push('%');
                out.push_str(&format!("{other:02X}"));
            }
        }
    }
    out
}

// ════════════════════════════════════════════════════════════════════════
// 执行
// ════════════════════════════════════════════════════════════════════════

/// 这一趟怎么跑。
#[derive(Debug, Clone)]
pub struct SyncOptions {
    /// 只同步这几个源；空表示全部。
    pub only: Vec<String>,
    /// 无视指纹，全部重取。
    pub full: bool,
    /// 只排计划，不取也不写。
    pub dry_run: bool,
    /// 大件落在哪。整包 106 MB，留在盘上下次改了映射不必重下。
    pub cache: PathBuf,
}

impl SyncOptions {
    /// 工作目录底下那份默认设置。
    #[must_use]
    pub fn new(workspace: &Path) -> Self {
        Self {
            only: Vec::new(),
            full: false,
            dry_run: false,
            cache: crate::workspace::dat_cache_dir(workspace),
        }
    }
}

/// 一趟同步的结果。
#[derive(Debug, Clone, Default)]
pub struct SyncOutcome {
    /// 每个源的计划。
    pub plans: Vec<Plan>,
    /// 取了几件。
    pub fetched: u64,
    /// 跳过几件（已是最新）。
    pub skipped: u64,
    /// 写进了几份 DAT。
    pub dats: u64,
    /// 取回来了、但没有映射命中因而没入库的有几份。
    ///
    /// 单独一个数是因为**整包那一档在计划里看不出来**：No-Intro 取的是一整个包，
    /// 计划上只有一条「取」，而包里 334 份 DAT 只有几十份该收。不报这个数，
    /// 报告会显得 No-Intro 一份都没落下。
    pub unmapped_dats: u64,
    /// 写进多少条。
    pub counts: DatCounts,
    /// 出了但没让整趟停下的问题：某一份 DAT 读不动，不该带走另外三百份。
    pub problems: Vec<String>,
}

/// 跑一趟同步。
///
/// # Errors
/// 列举失败、DAT 库写不进时返回错误。单份 DAT 取不到或读不动只记进
/// [`SyncOutcome::problems`]，不打断整趟。
pub fn run(
    fetcher: &dyn Fetcher,
    repo: &mut DatRepo,
    registry: &Registry,
    options: &SyncOptions,
) -> Result<SyncOutcome, SyncError> {
    let mut outcome = SyncOutcome::default();
    for source in registry.sources() {
        if !options.only.is_empty() && !options.only.iter().any(|name| name == &source.name) {
            continue;
        }
        let available = list(fetcher, source, registry)?;
        let known = repo.fingerprints(&source.name)?;
        let plan = plan(source, &available, &known, registry, options.full);
        outcome.skipped += plan.up_to_date() as u64;

        if !options.dry_run {
            for item in &plan.items {
                if item.action != Action::Fetch {
                    continue;
                }
                match fetch_one(fetcher, repo, registry, source, item, options) {
                    Ok(ingested) => {
                        outcome.fetched += 1;
                        outcome.dats += ingested.dats;
                        outcome.unmapped_dats += ingested.unmapped;
                        outcome.counts = outcome.counts.plus(ingested.counts);
                    }
                    // 一份读不动不该带走另外三百份——DAT 是几百份一起同步的东西。
                    Err(error) => outcome
                        .problems
                        .push(format!("{} / {}：{error}", source.name, item.name)),
                }
            }
        }
        outcome.plans.push(plan);
    }
    Ok(outcome)
}

/// 一件东西入库之后的账。
#[derive(Debug, Clone, Copy, Default)]
struct Ingested {
    /// 写进了几份 DAT。
    dats: u64,
    /// 包里有几份没有映射命中。
    unmapped: u64,
    /// 写进多少条。
    counts: DatCounts,
}

fn fetch_one(
    fetcher: &dyn Fetcher,
    repo: &mut DatRepo,
    registry: &Registry,
    source: &Source,
    item: &PlanItem,
    options: &SyncOptions,
) -> Result<Ingested, SyncError> {
    let unit = Unit {
        source: source.name.clone(),
        name: item.name.clone(),
        url: item.url.clone(),
        // 指纹问不出来时存空串。**它永远不会被判成「没变」**——`plan` 只在
        // `Available::fingerprint` 是 `Some` 时才比对。
        fingerprint: item.fingerprint.clone().unwrap_or_default(),
    };
    match &source.origin {
        Origin::ReleaseBundle { .. } => {
            let path = options.cache.join(&source.name).join(sanitize(&item.name));
            fetch_into_cache(fetcher, item, &path)?;
            ingest_bundle(repo, registry, source, &unit, &path)
        }
        Origin::RedumpSite { .. } => {
            // Redump 的 `/datfile/<码>` 直接返回一个 ZIP，里面正好一份 `.dat`。
            let path = options
                .cache
                .join(&source.name)
                .join(format!("{}.zip", sanitize(&item.name)));
            fetch_into_cache(fetcher, item, &path)?;
            ingest_bundle(repo, registry, source, &unit, &path)
        }
        Origin::RepoFiles { .. } => {
            let bytes = fetcher.get(&item.url)?.body;
            // 计划里已经查过一次，这里直接照着走——「第一条匹配的胜出」只实现在
            // `Registry::lookup` 一处，别的地方一律问它。
            let mapped = Mapped {
                platform: item.platform.as_deref().unwrap_or_default(),
                convention: item.convention.unwrap_or(source.convention),
            };
            let mut writer = repo.begin(&unit)?;
            let counts = write_one(&mut writer, source.shape, &bytes, &item.name, mapped)?;
            writer.commit()?;
            Ok(Ingested {
                dats: 1,
                unmapped: 0,
                counts,
            })
        }
    }
}

/// 取到缓存里，**指纹没变就不重下**。
///
/// 缓存留着原件是有用的，但只有配上这一步才真的有用：`--full` 与「改了一条映射要重新
/// 入库」这两件事都不需要重新下载——No-Intro 那个整包 106 MB，重解一遍只要几秒，
/// 重下一遍要几分钟。指纹存在原件旁边的一个小文件里，与 DAT 库里那份各记各的：
/// 库可以被删掉重建，缓存不必跟着重下。
fn fetch_into_cache(fetcher: &dyn Fetcher, item: &PlanItem, path: &Path) -> Result<(), SyncError> {
    // 指纹戳的名字是**加**一截后缀而不是换掉原来的：`with_extension` 会把
    // `no-intro.zip` 变成 `no-intro.fingerprint`，于是同名不同扩展名的两件东西
    // 会共用一个戳。
    let stamp = path.with_file_name(format!(
        "{}.fingerprint",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    if let Some(fingerprint) = &item.fingerprint
        && path.exists()
        && std::fs::read_to_string(&stamp).is_ok_and(|cached| &cached == fingerprint)
    {
        return Ok(());
    }
    // 先删掉旧戳再下载：下到一半失败时留着旧戳，下次就会拿半截文件当成下好的。
    let _ = std::fs::remove_file(&stamp);
    fetcher.download(&item.url, path)?;
    if let Some(fingerprint) = &item.fingerprint {
        write_cache(&stamp, fingerprint.as_bytes())?;
    }
    Ok(())
}

/// 从一个 zip 里挑出该收的 DAT 收进去。No-Intro 的整包与 Redump 的单系统包共用这条。
///
/// **一次读一个成员**，而不是排一张大计划一趟读完。整包里有 334 份 DAT 而映射命中的只有
/// 几十份，一趟读完要把「回调拿到的这一条是列表里的第几条」认出来——`InnerEntry` 没有
/// 身份字段，只能靠指针相等或按名字回查，两者都建立在「回调的顺序与实现细节」上。
/// zip 不是 solid 的，一个成员一趟的代价就是多几次 seek，换掉一个会静默错位的相关性
/// 是划算的。
fn ingest_bundle(
    repo: &mut DatRepo,
    registry: &Registry,
    source: &Source,
    unit: &Unit,
    path: &Path,
) -> Result<Ingested, SyncError> {
    let library = RealFs::new();
    let bundle = |detail: String| SyncError::Bundle {
        path: crate::path::display(path),
        detail,
    };
    let listing = container::list(&library, path).map_err(|error| bundle(error.to_string()))?;

    // 先决定要哪几个成员：包里绝大多数根本不该入库。
    let mut wanted: Vec<(usize, String, Mapped<'_>)> = Vec::new();
    let mut unmapped = 0_u64;
    for (index, entry) in listing.contents.entries.iter().enumerate() {
        if entry.is_dir || !entry.has_content() {
            continue;
        }
        let member = entry.path.rsplit('/').next().unwrap_or(&entry.path);
        let dat_name = bundle_dat_name(member);
        // Redump 那一档的映射键是系统短码（在 `unit.name` 上），包里那份 DAT 叫什么
        // 与映射无关；No-Intro 那一档反过来，映射键就在成员名上。
        let mapped = match &source.origin {
            Origin::RedumpSite { .. } => registry.map(&source.name, &unit.name),
            _ => registry.map(&source.name, &dat_name),
        };
        if let Some(mapped) = mapped {
            wanted.push((index, dat_name, mapped));
        } else {
            unmapped += 1;
        }
    }
    if wanted.is_empty() {
        return Ok(Ingested {
            unmapped,
            ..Ingested::default()
        });
    }

    let mut writer = repo.begin(unit)?;
    let mut total = DatCounts::default();
    let mut dats = 0_u64;
    for (index, name, mapped) in &wanted {
        let mut bytes = Vec::new();
        let plan = ReadPlan::only(&listing, *index);
        container::read_entries(&library, path, &listing, &plan, &mut |_, reader| {
            std::io::Read::read_to_end(reader, &mut bytes).map(|_| ())
        })
        .map_err(|error| bundle(error.to_string()))?;
        let counts = write_one(&mut writer, source.shape, &bytes, name, *mapped)?;
        total = total.plus(counts);
        dats += 1;
    }
    writer.commit()?;
    Ok(Ingested {
        dats,
        unmapped,
        counts: total,
    })
}

fn write_one(
    writer: &mut super::repo::UnitWriter<'_>,
    shape: Shape,
    bytes: &[u8],
    name: &str,
    mapped: Mapped<'_>,
) -> Result<DatCounts, SyncError> {
    let mut games = Vec::new();
    let header = match shape {
        Shape::Logiqx => logiqx::parse(bytes, name, &mut |game| games.push(game))?,
        Shape::SoftwareList => softlist::parse(bytes, name, &mut |game| games.push(game))?,
        Shape::GameDb => gamedb::parse(bytes, name, &mut |game| games.push(game))?,
    };
    let meta = DatMeta {
        name: name.to_string(),
        platform: mapped.platform.to_string(),
        convention: mapped.convention,
        header,
    };
    Ok(writer.write_dat(&meta, &games)?)
}

/// 整包里的成员名是 `<DAT 名> (<版本>).dat`，映射匹配的是前半截。
///
/// 版本每天都变，拿整个文件名去匹配等于每条映射都得写通配——那会让「先窄后宽」
/// 这条规则没法用。
#[must_use]
pub fn bundle_dat_name(member: &str) -> String {
    let stem = member
        .strip_suffix(".dat")
        .or_else(|| member.strip_suffix(".DAT"))
        .unwrap_or(member);
    match stem.rfind(" (") {
        Some(at) if stem.ends_with(')') => stem[..at].to_string(),
        _ => stem.to_string(),
    }
}

/// 名字里可能有斜杠与冒号，落到缓存文件名上要先洗一遍。
///
/// 洗完可能撞名（两个不同的名字洗成同一串），所以缓存**按源分目录**——
/// 撞名只可能发生在同一个源之内，而一个源内的名字本来就唯一。
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn write_cache(path: &Path, bytes: &[u8]) -> Result<(), SyncError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| SyncError::Bundle {
            path: crate::path::display(parent),
            detail: error.to_string(),
        })?;
    }
    std::fs::write(path, bytes).map_err(|error| SyncError::Bundle {
        path: crate::path::display(path),
        detail: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dat::registry::Registry;

    fn source(name: &str) -> Source {
        Registry::builtin()
            .source(name)
            .expect("内置清单里有这个源")
            .clone()
    }

    fn available(name: &str, url: &str, fingerprint: &str) -> Available {
        Available {
            name: name.to_string(),
            url: url.to_string(),
            fingerprint: Some(fingerprint.to_string()),
        }
    }

    #[test]
    fn 指纹没变的整件跳过() {
        let registry = Registry::builtin();
        let tosec = source("TOSEC");
        let items = vec![
            available(
                "TOSEC/SNK Neo-Geo Pocket Color - Games.dat",
                "https://raw.githubusercontent.com/smesgr9000/TOSEC-DAT/main/x",
                "sha-aaa",
            ),
            available(
                "TOSEC/Nintendo Game Boy - Games.dat",
                "https://raw.githubusercontent.com/smesgr9000/TOSEC-DAT/main/y",
                "sha-bbb",
            ),
        ];
        let known = [(
            "TOSEC/SNK Neo-Geo Pocket Color - Games.dat".to_string(),
            "sha-aaa".to_string(),
        )]
        .into_iter()
        .collect();
        let 这趟 = plan(&tosec, &items, &known, &registry, false);
        assert_eq!(这趟.to_fetch(), 1);
        assert_eq!(这趟.up_to_date(), 1);
        assert_eq!(这趟.items[0].action, Action::UpToDate);
        assert_eq!(这趟.items[1].action, Action::Fetch);
        assert_eq!(这趟.items[1].platform.as_deref(), Some("GB"));

        // `--full` 一给，指纹一律不认。
        let 全取 = plan(&tosec, &items, &known, &registry, true);
        assert_eq!(全取.to_fetch(), 2);
    }

    #[test]
    fn 问不出指纹的那一份每趟都重取() {
        // Redump 不给 ETag 也不给 Last-Modified，指纹全靠 Content-Disposition。
        // 那个头一旦缺了，**绝不能拿一个固定串顶上**——第一趟存进去之后每趟都会
        // 「没变」，那份 DAT 从此再也不更新，报告照报旧数。
        let registry = Registry::builtin();
        let redump = source("Redump");
        let items = vec![Available {
            name: "PSX".to_string(),
            url: "https://redump.info/datfile/PSX".to_string(),
            fingerprint: None,
        }];
        // 就算库里存着一条同名记录（空指纹），也照样重取。
        let known = [("PSX".to_string(), String::new())].into_iter().collect();
        let 这趟 = plan(&redump, &items, &known, &registry, false);
        assert_eq!(这趟.items[0].action, Action::Fetch);
        assert_eq!(这趟.up_to_date(), 0);
    }

    #[test]
    fn 没映射与明确不要分得开() {
        let registry = Registry::builtin();
        let tosec = source("TOSEC");
        let items = vec![
            available(
                "TOSEC-PIX/Nintendo Famicom & Entertainment System - Books.dat",
                "https://raw.githubusercontent.com/smesgr9000/TOSEC-DAT/main/a",
                "1",
            ),
            available(
                "TOSEC/Commodore Amiga - Games.dat",
                "https://raw.githubusercontent.com/smesgr9000/TOSEC-DAT/main/b",
                "2",
            ),
            available(
                "README.md",
                "https://raw.githubusercontent.com/smesgr9000/TOSEC-DAT/main/README.md",
                "3",
            ),
        ];
        let plan = plan(&tosec, &items, &BTreeMap::new(), &registry, false);
        assert_eq!(plan.to_fetch(), 0);
        assert_eq!(plan.out_of_scope(), 3);
        // 扫描件那一族是**明确不要**并带理由，Amiga 只是没映射——报告里要分得开，
        // 不然「TOSEC 怎么少了这么多」永远得重新查一遍。
        assert!(
            matches!(plan.items[0].action, Action::Excluded(ref why) if why.contains("扫描件"))
        );
        assert_eq!(plan.items[1].action, Action::Unmapped);
    }

    #[test]
    fn 计划这一层就把禁区拦下来() {
        // 用户可以整份换掉数据源清单。**闸门要在一个字节发出去之前就生效。**
        let registry = Registry::builtin();
        let nointro = source("No-Intro");
        let items = vec![available(
            "no-intro.zip",
            "https://datomatic.no-intro.org/index.php?page=download",
            "1",
        )];
        let plan = plan(&nointro, &items, &BTreeMap::new(), &registry, false);
        assert_eq!(plan.to_fetch(), 0);
        assert_eq!(plan.items[0].action, Action::Refused(Refusal::Datomatic));
    }

    #[test]
    fn redump_的冻结镜像也在计划这一层被拦下() {
        let registry = Registry::builtin();
        let redump = source("Redump");
        let items = vec![available("PSX", "https://redump.org/datfile/psx/", "1")];
        let plan = plan(&redump, &items, &BTreeMap::new(), &registry, false);
        assert_eq!(plan.items[0].action, Action::Refused(Refusal::FrozenRedump));
    }

    #[test]
    fn 整包那一档的映射作用在包里面() {
        // 包本身没有平台——334 份 DAT 在里面各归各的。
        let registry = Registry::builtin();
        let nointro = source("No-Intro");
        let items = vec![available(
            "no-intro.zip",
            "https://github.com/hugo19941994/auto-datfile-generator/releases/download/Daily_Rebuild/no-intro.zip",
            "2026-07-08T13:31:47Z/106776071",
        )];
        let plan = plan(&nointro, &items, &BTreeMap::new(), &registry, false);
        assert_eq!(plan.to_fetch(), 1);
        assert_eq!(plan.items[0].platform, None);
    }

    #[test]
    fn 整包成员名里的版本要剥掉() {
        // 版本每天都变，映射匹配的是前半截。
        assert_eq!(
            bundle_dat_name("Nintendo - Game Boy (20260625-122811).dat"),
            "Nintendo - Game Boy"
        );
        assert_eq!(
            bundle_dat_name("Sony - PlayStation - Datfile (10974) (2026-08-31 07-59-02).dat"),
            "Sony - PlayStation - Datfile (10974)"
        );
        assert_eq!(bundle_dat_name("没有版本.dat"), "没有版本");
    }

    #[test]
    fn redump_的下载页抠得出系统短码() {
        let html = r#"<a href="/datfile/PSX">PlayStation</a>
            <a href="/datfile/WIIU/">Wii U</a>
            <a href="/cues/PSX">cues</a>
            <a href="/datfile/PSX">重复的</a>"#;
        assert_eq!(datfile_codes(html), ["PSX", "WIIU"]);
    }

    #[test]
    fn 仓库路径要转义才放得进_url() {
        assert_eq!(
            percent_encode("TOSEC/SNK Neo-Geo Pocket Color - Games.dat"),
            "TOSEC/SNK%20Neo-Geo%20Pocket%20Color%20-%20Games.dat"
        );
        assert_eq!(
            percent_encode("TOSEC/Nintendo Famicom & Entertainment System - Games - [NES].dat"),
            "TOSEC/Nintendo%20Famicom%20%26%20Entertainment%20System%20-%20Games%20-%20%5BNES%5D.dat"
        );
    }
}
