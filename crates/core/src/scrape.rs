//! **刮削**：在**识别**结论的基础上取元数据与媒体。
//!
//! **识别与刮削是两个阶段，识别错了刮削再准也是错的**（`CONTEXT.md`）。这个模块从不
//! 重新判断「这是哪个游戏」——那是[识别](crate::identify)的活，它的结论已经躺在中立库里
//! （候选、作品、发行版）。这里只做一件事：**按锚点、按字段、按源，把元数据与媒体收进来**。
//!
//! ## 离线档：运行时零网络请求
//!
//! 这一趟只用**本地数据源**：中立库里已经有的候选（票 06 镜像下来的 DAT 撞出来的）、
//! 变体的文件名、主库里现成的图片与视频。全库刮一遍不消耗任何在线配额（ADR-0007）。
//!
//! 「不联网」不是靠注释保证的，是一道**闸门**：每个源自报[本地还是联网](Locality)，
//! 而[离线档](Profile::Offline)只收本地源，混进一个联网源会当场被拒。这与
//! [`dat::guard`](crate::dat::guard) 是同一个套路——数据源清单是**数据**，用户可以换掉，
//! 靠自觉守不住。
//!
//! ## 三个 ID 必须分开（调研 13.3(2)）
//!
//! 这是最容易做错的地方，混用会让缓存命中率崩塌或者误合并：
//!
//! | 概念 | 这里叫什么 | 是什么 |
//! |---|---|---|
//! | 采集缓存的主键 | **锚点**（[`AnchorKind`] 加一个自然键） | 刮削结论挂在哪儿：作品名，或变体的键 |
//! | 查询哈希 | 识别那一侧的 CRC-32 | 拿去撞 DAT 的判据，刮削这一层根本不碰 |
//! | 媒体主键 | 内容哈希（[`pool`]） | **文件名不是媒体的主键**（ADR-0009） |
//!
//! ## 缓存不是优化项
//!
//! 调研发现 ES-DE 完全没有结果缓存，每次刮削都重打一遍 API。这里反过来：一个源在一个
//! 锚点上采过了就记下当时的**输入指纹**，指纹没变就整条跳过——离线档省的是算力与回盘
//! 读取，在线档（票 14）省的是配额。**「查过、没有」也记着**，否则每跑一趟都要重打一遍
//! 同样的空查询。

pub mod dat;
pub mod local;
pub mod pool;
pub mod priority;
pub mod report;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::catalog::scrape::{Harvested, HarvestedMedia, HarvestedValue};
use crate::catalog::{Catalog, CatalogError};
use crate::fs::LibraryFs;
use crate::scan::CancelToken;

use pool::{MediaPool, PoolError};

pub use priority::Priorities;
pub use report::ScrapeReport;

/// 刮削跑不下去的原因。
///
/// **读不动一份媒体不在这里**——那一份跳过、如实记一句就够了，不该让整趟停下来。
/// 能到这一层的只有中立库读写不了、或者媒体池建不出来。
#[derive(Debug, thiserror::Error)]
pub enum ScrapeError {
    /// 中立库读写失败。
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    /// 媒体池读写失败。
    #[error(transparent)]
    Pool(#[from] PoolError),
    /// 策略档案里混进了它不该有的源。
    ///
    /// 字段不叫 `source`：`thiserror` 把叫这个名字的字段当成**错误的来源**，
    /// 而这里它只是一个源的名字。
    #[error("{profile}不许用「{name}」这个源：它要联网，而这一档的全部意义就是不联网")]
    WrongLocality {
        /// 哪个档案。
        profile: &'static str,
        /// 哪个源。
        name: String,
    },
}

/// **锚点种类**：刮削结论挂在三层内容层级的哪一层。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnchorKind {
    /// **作品**。简介、年份、发行商这些跨平台跨地区都成立的东西挂这一层
    /// （`CONTEXT.md` 的「作品」词条）。
    Work,
    /// **变体**。汉化组、以及躺在这个变体目录里的那几张图挂这一层——依据是变体级的。
    Variant,
}

impl AnchorKind {
    /// 存进库、也打给用户的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Work => "作品",
            Self::Variant => "变体",
        }
    }
}

/// 一个刮削**字段**。
///
/// 只列**这套模型真的表达得了**的那些。离线源填不上的（简介、类型、开发商）照样列在
/// 这里，因为报告要答得出「离线档补不上哪些字段」——那正是票 14 存在的理由，
/// 藏起来等于假装缺口不存在。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Field {
    /// 标题。**这里只收原始值**，选出**显示标题**是票 15 的活。
    Title,
    /// 发行年份。
    Year,
    /// 发行商。
    Publisher,
    /// 开发商。
    Developer,
    /// 类型。
    Genre,
    /// 简介。
    Description,
    /// **汉化组**：民间汉化是谁做的。它挂在**变体**上——汉化版是变体不是发行版
    /// （ADR-0012）。
    TranslationGroup,
}

impl Field {
    /// 存进库、也打给用户的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Title => "标题",
            Self::Year => "年份",
            Self::Publisher => "发行商",
            Self::Developer => "开发商",
            Self::Genre => "类型",
            Self::Description => "简介",
            Self::TranslationGroup => "汉化组",
        }
    }

    /// 报告里固定的排列顺序。
    #[must_use]
    pub fn all() -> [Self; 7] {
        [
            Self::Title,
            Self::Year,
            Self::Publisher,
            Self::Developer,
            Self::Genre,
            Self::Description,
            Self::TranslationGroup,
        ]
    }
}

/// 一份媒体是什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MediaKind {
    /// 封面。
    Cover,
    /// 截图。
    Screenshot,
    /// 视频。
    Video,
    /// 认不出是什么的图。**不猜**——猜错了导出时会把说明书当封面铺出去。
    Other,
}

impl MediaKind {
    /// 存进库、也打给用户的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Cover => "封面",
            Self::Screenshot => "截图",
            Self::Video => "视频",
            Self::Other => "其他",
        }
    }

    /// 报告里固定的排列顺序。
    #[must_use]
    pub fn all() -> [Self; 4] {
        [Self::Cover, Self::Screenshot, Self::Video, Self::Other]
    }
}

/// 一个源是本地的还是要联网的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locality {
    /// 只用本机已有的东西：中立库、DAT 库、主库里现成的文件。
    Local,
    /// 要发网络请求。**离线档一个都不收。**
    Online,
}

/// **刮削策略档案**。预置两套，在任务级别切换（ADR-0007）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// **离线优先**：只用本地数据源，全库刮一遍不消耗任何在线配额。
    Offline,
}

impl Profile {
    /// 打给用户的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Offline => "离线档",
        }
    }

    /// 这一档收哪几种源。
    #[must_use]
    pub fn accepts(self, locality: Locality) -> bool {
        match self {
            Self::Offline => locality == Locality::Local,
        }
    }
}

/// 一条撞出来的 DAT 条目，刮削用得上的那两列。
///
/// **只有这两列**：哪个源、条目名。是哪一份 DAT、有没有中文记号，`candidate` 那张表里
/// 都记着，事后复核照查不误——在这里再抄一遍，只是让 46,444 个锚点各多背一串没人读的字符串。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DatEntry {
    /// 哪个数据源（`No-Intro` / `TOSEC` / …）。
    pub source: String,
    /// 条目名。**语义信息全编码在这里面**——TOSEC 的名字里就带着年份与发行商。
    pub game: String,
}

/// 主库里一份现成的媒体文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalMedia {
    /// 它在中立库里的键。
    pub key: String,
    /// 字节数；元数据读不到时是 `None`（ADR-0021）。
    pub bytes: Option<u64>,
    /// 修改时间；读不到时是 `None`。
    pub mtime: Option<i64>,
    /// 这是封面还是截图还是视频。
    pub kind: MediaKind,
    /// 凭什么把它归给这个变体。
    pub why: &'static str,
}

/// 交给一个源去看的东西。
///
/// 一个源不必用得上全部——`probe` 返回 `None` 就是「这个源对这个锚点无话可说」。
#[derive(Debug, Clone, Copy)]
pub struct Subject<'a> {
    /// 哪一层。
    pub kind: AnchorKind,
    /// 锚点。
    pub id: &'a str,
    /// 平台；认不出就是 `None`。
    pub platform: Option<&'a str>,
    /// 撞出这个锚点的 DAT 条目。
    pub entries: &'a [DatEntry],
    /// **变体**锚点才有：主文件的键。
    pub main_key: Option<&'a str>,
    /// **变体**锚点才有：能归给它的本地媒体。
    pub media: &'a [LocalMedia],
    /// 这一趟单份媒体的上限；`None` 是不设上限。
    ///
    /// 它在这里，是因为**它改变采集的结果**：上限从 32 MiB 提到 128 MiB，同一批文件
    /// 该多收进来几份。凡是改变结果的东西都必须进**输入指纹**，否则调完上限重跑，
    /// 缓存会一口咬定「输入没变」而整条跳过——那些文件永远收不进来。
    pub media_limit: Option<u64>,
}

/// 一个源说「主库里这份文件是这个锚点的某种媒体」。
///
/// 带着**库里记的字节数**，于是超过上限的那些在**打开文件之前**就拦得下来——
/// 少了这一样，一份 662 MiB 的预览视频要先读满上限那一段才发现超了。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaClaim {
    /// 是封面还是截图还是视频。
    pub kind: MediaKind,
    /// 主库里那份文件的键。
    pub key: String,
    /// 库里记的字节数；元数据读不到时是 `None`（ADR-0021）。
    pub bytes: Option<u64>,
    /// **依据**。
    pub why: String,
}

/// 一个源给出的一个字段值，连着它的**依据**。
///
/// 不用三元组：第三位是词表里的一等术语**依据**，而「没有依据的结论事后无法复核」
/// 是这个项目反复说的一条（ADR-0002 在识别那一侧，这里是同一条）。叫得出名字的东西
/// 不该在类型里退化成 `String`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// 哪个字段。
    pub field: Field,
    /// 值。
    pub value: String,
    /// **依据**：这个值是怎么来的。
    pub evidence: String,
}

/// 一个源在一个锚点上采到的东西。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Harvest {
    /// 字段值。
    pub values: Vec<Finding>,
    /// 媒体。
    pub media: Vec<MediaClaim>,
}

impl Harvest {
    /// 记一个字段值。
    ///
    /// 两种情况**不记**：
    ///
    /// - **空值**。缺的字段就该是缺的，写个空串进去只会让优先级链在它身上停下来。
    /// - **这个字段这个源已经说过话了**。库里一个「锚点 × 字段 × 源」只存一条
    ///   （那正是三元组并存的粒度），一个源交出两个值只会有一个落库，另一个连同它的
    ///   **依据**一起蒸发，而采集记录还记着「采到两条」——数对不上，还查不出为什么。
    ///   于是**第一个胜出**：源按确定的顺序遍历条目名，同一份库跑两次结果一样。
    ///   一个字段要装多个值，那是**标题集合**的形状，票 15 的模型，不是这一层。
    pub fn value(&mut self, field: Field, value: impl Into<String>, why: impl Into<String>) {
        let value = value.into();
        if value.trim().is_empty() || self.values.iter().any(|found| found.field == field) {
            return;
        }
        self.values.push(Finding {
            field,
            value,
            evidence: why.into(),
        });
    }

    /// 记一份媒体。
    pub fn picture(&mut self, claim: MediaClaim) {
        self.media.push(claim);
    }
}

/// 一个刮削**数据源**。
///
/// 三件事：自报家门（名字与本地/联网）、给出**输入指纹**、采集。
///
/// **采集是纯的**——它只说「主库里这个键的文件是这个变体的封面」，读字节、算哈希、
/// 往池里放全都归引擎。这样源可以完全在内存里测，而全部 IO 收在一处。
pub trait Source {
    /// 这个源叫什么。它会进优先级表，也会进每一条**依据**。
    fn name(&self) -> &str;

    /// 本地还是联网。
    fn locality(&self) -> Locality;

    /// 这一轮对这个锚点的**输入指纹**；`None` 表示这个源对这个锚点无话可说。
    ///
    /// 指纹与上次一样就整条跳过，连采集都不跑。它必须**盖住全部会改变结果的输入**：
    /// 漏了一样，那一样变了也不会重采。媒体那个源的上限就是这么一样——上限从 32 MiB
    /// 提到 128 MiB，同一批文件该多收进来几份，不盖它的话它们永远收不进来。
    ///
    /// **盖不住的只有一样：这个源自己的解析逻辑。** 改了 `collect` 里读名字的办法，
    /// 指纹不会变，已经采过的锚点不会重采。这一条不打算靠「往指纹里折一个代码版本号」
    /// 解决——那要靠人手动改一个常量，忘了改就是同样的静默失效。出口是 `--refresh`，
    /// 而它在真库上只要 4.1 秒、回盘 0 字节（算过的媒体哈希留着）。
    fn probe(&self, subject: &Subject<'_>) -> Option<String>;

    /// 采集。
    fn collect(&self, subject: &Subject<'_>, out: &mut Harvest);
}

/// 刮削的选项。
#[derive(Debug, Clone)]
pub struct Options {
    /// 主库根。只有要把本地媒体读进池里时才用得上。
    pub root: PathBuf,
    /// 媒体池在哪。
    pub pool: PathBuf,
    /// 策略档案。
    pub profile: Profile,
    /// 收不收媒体。关掉之后一个字节都不读主库。
    pub media: bool,
    /// 单份媒体大到多少字节就不收了；`None` 是不设上限。
    pub max_media_bytes: Option<u64>,
    /// 无视缓存，全部重采。
    pub refresh: bool,
    /// 每采完多少个锚点就写一批进中立库。
    pub write_batch: usize,
}

impl Options {
    /// 对着某个主库根与某个媒体池的默认选项。
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, pool: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            pool: pool.into(),
            profile: Profile::Offline,
            media: true,
            max_media_bytes: None,
            refresh: false,
            write_batch: 2_000,
        }
    }
}

/// 跑到哪儿了。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Progress {
    /// 采完了几个锚点。
    pub done: u64,
    /// 一共几个。
    pub total: u64,
    /// 收进池里几份媒体。
    pub media: u64,
    /// 为收媒体回盘读了多少字节。
    pub read_bytes: u64,
}

/// 一趟刮削的产物。
#[derive(Debug, Clone)]
pub struct Outcome {
    /// 报告。
    pub report: ScrapeReport,
    /// 这一趟是不是被中断了。
    pub interrupted: bool,
    /// 回盘读了多少字节。**第二趟应当是 0**——算过的媒体哈希留在中立库里。
    pub read_bytes: u64,
    /// 回盘读了几份媒体。
    pub read_files: u64,
    /// 有几份媒体的哈希是从中立库里直接取回来的，没有再读一遍盘。
    pub reused_hashes: u64,
    /// 有几个「锚点 × 源」因为输入指纹没变而整条跳过。
    pub reused_probes: u64,
    /// 这一趟往池里新增了几份媒体。
    pub new_blobs: u64,
    /// 这一趟有几次「算出来的哈希池里已经有了」——那就是「只存一份」省下来的次数。
    pub deduped: u64,
    /// **读不动**、因此跳过的媒体有几份（ADR-0021 的第三态）。
    pub unreadable_media: u64,
    /// **超过单份上限**、因此跳过的媒体有几份。**与读不动是平行的两件事**：
    /// 那些文件好好的，是用户设了上限。混成一个数，报告就说不出跳过的到底怎么了。
    pub oversized_media: u64,
    /// 扩展名不认得、因此没当成媒体收的有几份。**正常情况下是 0**：本地媒体源本来就
    /// 按扩展名筛过一遍。它不是 0 就说明两处的表对不上了，那是要查的。
    pub not_media: u64,
    /// 有几个「锚点 × 源」这次无话可说、于是上一轮的结论被清掉了。
    ///
    /// 重跑识别把某个源的候选清空时就是这一态。**留着比缺着更糟**：那些值带着一条
    /// 指向已经不存在的条目的**依据**。
    pub forgotten: u64,
}

/// 一趟刮削。
///
/// **不联网**（离线档的全部意义），**不写主库一个字节**（ADR-0004）：读主库只发生在
/// 一处——把一份本地媒体读进**媒体池**，而那也可以用 `options.media = false` 关掉。
///
/// # Errors
/// 中立库读写不了、媒体池建不出来、或者档案里混进了联网源时返回错误。
pub fn run(
    library: &dyn LibraryFs,
    catalog: &mut Catalog,
    priorities: &Priorities,
    options: &Options,
    context: &mut RunContext<'_>,
) -> Result<Outcome, ScrapeError> {
    let sources = offline_sources(options.profile, options.media)?;
    if options.refresh {
        catalog.clear_scraped()?;
    }
    let pool = MediaPool::open(&options.pool)?;

    let plan = Plan::build(catalog, options)?;
    let mut run = Run::default();
    let mut batch: Vec<Harvested> = Vec::new();
    let mut done = 0_u64;
    let total = u64::try_from(plan.subjects.len()).unwrap_or(u64::MAX);
    let mut interrupted = false;

    for subject in &plan.subjects {
        if context.cancel.is_cancelled() {
            interrupted = true;
            break;
        }
        let known = catalog.scrape_inputs(subject.kind.label(), &subject.id)?;
        for source in &sources {
            let view = subject.view(options.max_media_bytes);
            let Some(input) = source.probe(&view) else {
                // **这个源这次无话可说，而上次说过话**——那上次说的话已经作废了。
                // 重跑一次识别就会出现这一态：TOSEC 的候选没了，它上一轮挂上去的年份
                // 与发行商还留在库里，而库里现在没有任何东西支持它们。留着比缺着更糟：
                // 它带着一条指向已经不存在的条目的**依据**，事后复核会对不上。
                if known.contains_key(source.name()) {
                    catalog.forget_scraped(subject.kind.label(), &subject.id, source.name())?;
                    run.forgotten += 1;
                }
                continue;
            };
            if known.get(source.name()) == Some(&input) {
                run.reused_probes += 1;
                continue;
            }
            let mut harvest = Harvest::default();
            source.collect(&view, &mut harvest);
            let media = ingest_all(library, catalog, &pool, options, &harvest, &mut run)?;
            batch.push(Harvested {
                anchor: subject.kind.label().to_string(),
                subject: subject.id.clone(),
                source: source.name().to_string(),
                input,
                values: harvest
                    .values
                    .into_iter()
                    .map(|found| HarvestedValue {
                        field: found.field.label().to_string(),
                        value: found.value,
                        evidence: found.evidence,
                    })
                    .collect(),
                media,
            });
        }
        done += 1;
        if batch.len() >= options.write_batch {
            catalog.put_scraped(&batch)?;
            batch.clear();
        }
        if done.is_multiple_of(500) {
            (context.progress)(Progress {
                done,
                total,
                media: run.media_count,
                read_bytes: run.read_bytes,
            });
        }
    }
    catalog.put_scraped(&batch)?;
    (context.progress)(Progress {
        done,
        total,
        media: run.media_count,
        read_bytes: run.read_bytes,
    });

    let names: Vec<&str> = sources.iter().map(|source| source.name()).collect();
    let report = ScrapeReport::build(catalog, priorities, options, &names, &plan.counts)?;
    Ok(Outcome {
        report,
        interrupted,
        read_bytes: run.read_bytes,
        read_files: run.read_files,
        reused_hashes: run.reused_hashes,
        reused_probes: run.reused_probes,
        new_blobs: run.new_blobs,
        deduped: run.deduped,
        unreadable_media: run.unreadable_media,
        oversized_media: run.oversized_media,
        not_media: run.not_media,
        forgotten: run.forgotten,
    })
}

/// 跑一趟要的那两样外部东西：中断信号与进度回调。
///
/// 捏成一个结构而不是两个参数，是为了让 [`run`] 的参数表停在六个以内——再加就该有人
/// 问「这个函数是不是做了两件事」了。
pub struct RunContext<'a> {
    /// 中断信号。
    pub cancel: &'a CancelToken,
    /// 进度回调。
    pub progress: &'a mut dyn FnMut(Progress),
}

/// 一串输入折成**输入指纹**。
///
/// 取 SHA-256 的前 16 位十六进制。全长 64 位存进库里，46,444 个变体 × 7 个源就是
/// 20 MB 的纯噪音；16 位（64 比特）在这个量级上撞一次的概率可以忽略，而撞了的后果
/// 只是**少重采一次**——不是错，是慢一步被发现。
fn fingerprint(parts: &[&str]) -> String {
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    for part in parts {
        context.update(part.as_bytes());
        // 分隔符不能省：`["ab", "c"]` 与 `["a", "bc"]` 拼起来是同一串。
        context.update(&[0]);
    }
    pool::hex(&context.finish().as_ref()[..8])
}

/// 离线档的全部源。**顺序无关**——谁排前面由优先级表说了算，不由这里说了算。
///
/// `media` 为假时**本地媒体源整个不参加**，而不是喂给它一份空的媒体清单。差别是要命的：
/// 空清单会让它的 `probe` 返回「无话可说」，而「无话可说 + 上次说过话」正是引擎清掉
/// 上一轮结论的那一态——于是 `--no-media` 会把此前收好的媒体映射**悄悄删掉**。
/// `--no-media` 说的是「这趟不收媒体」，不是「把收过的扔了」。
fn offline_sources(profile: Profile, media: bool) -> Result<Vec<Box<dyn Source>>, ScrapeError> {
    let mut sources: Vec<Box<dyn Source>> = vec![
        Box::new(dat::DatSource::new("No-Intro")),
        Box::new(dat::DatSource::new("Redump")),
        Box::new(dat::DatSource::new("TOSEC")),
        Box::new(dat::DatSource::new("MAME")),
        Box::new(dat::DatSource::new("GoodNES")),
        Box::new(local::FilenameSource::new()),
    ];
    if media {
        sources.push(Box::new(local::LocalMediaSource::new()));
    }
    // **闸门**：档案说不收的源，一个都不许混进来。写在这里而不是靠上面那张表自觉，
    // 是因为将来票 14 会往这张表里加联网源，而加的时候最容易忘的就是档案这一层。
    for source in &sources {
        if !profile.accepts(source.locality()) {
            return Err(ScrapeError::WrongLocality {
                profile: profile.label(),
                name: source.name().to_string(),
            });
        }
    }
    Ok(sources)
}

/// 这一趟的锚点清单，以及报告要的几个计数。
struct Plan {
    subjects: Vec<PlannedSubject>,
    counts: PlanCounts,
}

/// 报告里「这一趟看了些什么」那几个数。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlanCounts {
    /// 作品锚点几个。
    pub works: u64,
    /// 变体锚点几个。
    pub variants: u64,
    /// 找到几份可以归给某个变体的本地媒体。
    pub local_media: u64,
}

struct PlannedSubject {
    kind: AnchorKind,
    id: String,
    platform: Option<String>,
    entries: Vec<DatEntry>,
    main_key: Option<String>,
    media: Vec<LocalMedia>,
}

impl PlannedSubject {
    fn view(&self, media_limit: Option<u64>) -> Subject<'_> {
        Subject {
            kind: self.kind,
            id: &self.id,
            platform: self.platform.as_deref(),
            entries: &self.entries,
            main_key: self.main_key.as_deref(),
            media: &self.media,
            media_limit,
        }
    }
}

impl Plan {
    fn build(catalog: &Catalog, options: &Options) -> Result<Self, CatalogError> {
        let variants = catalog.variants()?;
        let works = catalog.work_names()?;

        // 候选按变体归堆。同一个源在同一个变体上可能撞出好几条（多碟、多芯片），
        // 归堆时去重——刮削要的是「这个源说这是什么」，不是「撞了几次」。
        let mut by_variant: BTreeMap<String, Vec<DatEntry>> = BTreeMap::new();
        let mut seen: BTreeSet<(String, String, String)> = BTreeSet::new();
        catalog.for_each_candidate_fact(&mut |key, source, game| {
            let mark = (key.to_string(), source.to_string(), game.to_string());
            if !seen.insert(mark) {
                return;
            }
            by_variant
                .entry(key.to_string())
                .or_default()
                .push(DatEntry {
                    source: source.to_string(),
                    game: game.to_string(),
                });
        })?;

        let media_index = if options.media {
            local::index(catalog, &variants)?
        } else {
            BTreeMap::new()
        };
        let local_media = media_index.values().map(|list| list.len() as u64).sum();

        // 作品锚点：把它下面全部变体的候选并起来。平台取第一个说得出的——同一部作品
        // 跨平台时哪个都不算错，而平台在这一层只用于按平台覆写优先级。
        let mut work_entries: BTreeMap<String, (Option<String>, Vec<DatEntry>)> = BTreeMap::new();
        let mut subjects = Vec::with_capacity(variants.len() + works.len());
        for variant in &variants {
            let entries = by_variant.remove(&variant.key).unwrap_or_default();
            if let Some(name) = variant.work_id.and_then(|id| works.get(&id)) {
                let slot = work_entries
                    .entry(name.clone())
                    .or_insert_with(|| (variant.platform.clone(), Vec::new()));
                if slot.0.is_none() {
                    slot.0.clone_from(&variant.platform);
                }
                slot.1.extend(entries.iter().cloned());
            }
            subjects.push(PlannedSubject {
                kind: AnchorKind::Variant,
                id: variant.key.clone(),
                platform: variant.platform.clone(),
                entries,
                main_key: Some(variant.main_key.clone()),
                media: media_index.get(&variant.key).cloned().unwrap_or_default(),
            });
        }
        let works_count = u64::try_from(work_entries.len()).unwrap_or(u64::MAX);
        for (name, (platform, mut entries)) in work_entries {
            entries.sort();
            entries.dedup();
            subjects.push(PlannedSubject {
                kind: AnchorKind::Work,
                id: name,
                platform,
                entries,
                main_key: None,
                media: Vec::new(),
            });
        }
        Ok(Self {
            counts: PlanCounts {
                works: works_count,
                variants: u64::try_from(variants.len()).unwrap_or(u64::MAX),
                local_media,
            },
            subjects,
        })
    }
}

/// 一趟跑下来攒的那些数。
#[derive(Debug, Default)]
struct Run {
    read_bytes: u64,
    read_files: u64,
    reused_hashes: u64,
    reused_probes: u64,
    new_blobs: u64,
    deduped: u64,
    unreadable_media: u64,
    oversized_media: u64,
    not_media: u64,
    forgotten: u64,
    media_count: u64,
}

/// 把一次采集里的媒体全部收进池里，返回写库要的 `(类型, 哈希, 依据)`。
fn ingest_all(
    library: &dyn LibraryFs,
    catalog: &mut Catalog,
    pool: &MediaPool,
    options: &Options,
    harvest: &Harvest,
    run: &mut Run,
) -> Result<Vec<HarvestedMedia>, ScrapeError> {
    let mut out = Vec::with_capacity(harvest.media.len());
    for claim in &harvest.media {
        let ingested = pool::ingest(
            library,
            catalog,
            pool,
            &options.root,
            &pool::Claim {
                key: &claim.key,
                bytes: claim.bytes,
                max_bytes: options.max_media_bytes,
            },
        )?;
        match ingested {
            pool::Ingested::Reused { hash } => {
                run.reused_hashes += 1;
                out.push(HarvestedMedia {
                    kind: claim.kind.label().to_string(),
                    hash,
                    evidence: claim.why.clone(),
                });
            }
            pool::Ingested::Stored { hash, bytes, fresh } => {
                run.read_bytes += bytes;
                run.read_files += 1;
                if fresh {
                    run.new_blobs += 1;
                } else {
                    run.deduped += 1;
                }
                out.push(HarvestedMedia {
                    kind: claim.kind.label().to_string(),
                    hash,
                    evidence: claim.why.clone(),
                });
            }
            // 三种跳过分开数。**它们不是同一件事**，塞进同一个计数器，
            // 报告就只能说「有 108 份没收进来」而说不出为什么。
            pool::Ingested::TooBig { .. } => run.oversized_media += 1,
            pool::Ingested::Unreadable { .. } => run.unreadable_media += 1,
            pool::Ingested::NotMedia { .. } => run.not_media += 1,
        }
    }
    run.media_count += u64::try_from(out.len()).unwrap_or(0);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 离线档不收联网源() {
        struct 假源;
        impl Source for 假源 {
            fn name(&self) -> &str {
                "假的在线源"
            }
            fn locality(&self) -> Locality {
                Locality::Online
            }
            fn probe(&self, _: &Subject<'_>) -> Option<String> {
                None
            }
            fn collect(&self, _: &Subject<'_>, _: &mut Harvest) {}
        }
        let source = 假源;
        assert!(!Profile::Offline.accepts(source.locality()));
        assert!(Profile::Offline.accepts(Locality::Local));
    }

    #[test]
    fn 离线档那七个源全是本地的() {
        let sources = offline_sources(Profile::Offline, true).expect("离线档该收得下这七个源");
        assert_eq!(sources.len(), 7);
        assert!(sources.iter().all(|s| s.locality() == Locality::Local));
    }

    #[test]
    fn 不收媒体时本地媒体源整个不参加() {
        // 它若参加而拿到一份空清单，`probe` 会返回「无话可说」，
        // 上一轮收好的媒体映射就被当成过期结论清掉了。
        let sources = offline_sources(Profile::Offline, false).expect("收得下");
        assert_eq!(sources.len(), 6);
        assert!(sources.iter().all(|s| s.name() != local::LOCAL_MEDIA));
    }

    #[test]
    fn 空值不进库() {
        let mut harvest = Harvest::default();
        harvest.value(Field::Title, "  ", "空的");
        harvest.value(Field::Title, "魔界村", "有的");
        assert_eq!(harvest.values.len(), 1);
        assert_eq!(harvest.values[0].value, "魔界村");
    }

    #[test]
    fn 一个源在一个字段上只留第一个值() {
        // 库里一个「锚点 × 字段 × 源」只存一条。交出两个，第二个连同它的**依据**
        // 一起蒸发，而采集记录还记着「采到两条」——数对不上，还查不出为什么。
        let mut harvest = Harvest::default();
        harvest.value(Field::Title, "魔界村", "第一个条目名");
        harvest.value(Field::Title, "Ghosts 'n Goblins", "第二个条目名");
        harvest.value(Field::Year, "1985", "同一个源的另一个字段照记");
        assert_eq!(harvest.values.len(), 2);
        assert_eq!(harvest.values[0].value, "魔界村");
        assert_eq!(harvest.values[1].field, Field::Year);
    }
}
