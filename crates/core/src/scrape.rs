//! **刮削**：在**识别**结论的基础上取元数据与媒体。
//!
//! **识别与刮削是两个阶段，识别错了刮削再准也是错的**（`CONTEXT.md`）。这个模块从不
//! 重新判断「这是哪个游戏」——那是[识别](crate::identify)的活，它的结论已经躺在中立库里
//! （候选、作品、发行版）。这里只做一件事：**按锚点、按字段、按源，把元数据与媒体收进来**。
//!
//! ## 两套策略档案，在任务级别手动切换（ADR-0007）
//!
//! [离线档](Profile::Offline)只用**本地数据源**：中立库里已经有的候选（票 06 镜像下来的
//! DAT 撞出来的）、变体的文件名、主库里现成的图片与视频。全库刮一遍不消耗任何在线配额。
//!
//! [在线档](Profile::Online)在这些之上再加联网源（[`online`]），补离线档补不上的那一样
//! ——**封面**：本地数据源里根本没有图片。简介、类型、开发商与发行商票 02 至票 04 之后
//! 离线档自己都有了（[`zh`] 那个源）。它对这个库有结构性风险，全部小心都收在
//! [`online`] 那个模块里。
//!
//! 「离线档不联网」不是靠注释保证的，是一道**闸门**：每个源自报[本地还是联网](Locality)，
//! 而离线档只收本地源，混进一个联网源会当场被拒。这与
//! [`dat::guard`](crate::dat::guard) 是同一个套路——数据源清单是**数据**，用户可以换掉，
//! 靠自觉守不住。**给了网络句柄也一样**：[`run`] 拿到 `Some(net)` 而档案是离线时，
//! 那个句柄一次都不会被碰——闸门查的是源自报的 `Locality`，不是参数表长什么样。
//!
//! ## 两套档案共用同一份优先级表与同一份缓存
//!
//! 字段级优先级（[`Priorities`]）与采集缓存（`scrape_probe` / `scrape_value` /
//! `media_ref`）是两个档位**共用**的机制，不是在线档专有（ADR-0007）。于是「离线跑一遍、
//! 再用在线档补缺口」是同一份库上的两趟，不是两套结论；而改一次优先级不必重采。
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
pub mod estimate;
pub mod local;
pub mod online;
pub mod pool;
pub mod preview;
pub mod priority;
pub mod report;
pub mod zh;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::catalog::scrape::{Harvested, HarvestedMedia, HarvestedValue};
use crate::catalog::{Catalog, CatalogError, Roots};
use crate::fs::LibraryFs;
use crate::scan::CancelToken;

use crate::identify::fuzzy;
use online::{Halt, Net};
use pool::{MediaPool, PoolError};

pub use estimate::Estimate;
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
    /// 在线档没有网络句柄。
    ///
    /// **宁可不启动也不要退回离线偷偷跑完**：用户点名要在线档，是因为他要的正是离线档
    /// 补不上的那几样；悄悄降级只会让他对着一份缺封面的报告以为「在线源也没有」。
    #[error(
        "在线档要一个网络句柄与一套凭据才起得来。\
         凭据从环境变量读：{}。\
         ScreenScraper 的 devid 要在它的论坛人工申请（无 devid 直接 403），\
         **不要拿别人的 devid 用**——那会连累对方被拉黑。",
        online::ENV_KEYS.join(" / ")
    )]
    NoNetwork,
}

/// 一个源这一次没采成。
///
/// **与「无话可说」是两件事**：无话可说（`probe` 返回 `None`）说的是「这个源对这个锚点
/// 本来就没有意见」，引擎据此清掉上一轮的结论；而这里说的是「本该有意见，这次没问到」。
/// 混为一谈会让一次网络抖动把库里好好的结论删掉。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// 这一条没采到，整趟接着跑。**不写库**——那一对下一趟还会再来。
    Skip {
        /// 为什么。
        why: String,
    },
    /// **整趟到此为止。** 配额超限、凭据不对、网断了。已经采完的那部分留在库里。
    Halt(Halt),
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
/// 只列**这套模型真的表达得了**的那些。哪个源填得上哪一格是另一回事：报告要答得出
/// 「这一趟一个值都没采到的字段」，所以一个字段填不上也照样列在这里——那正是票 14
/// 存在的理由，藏起来等于假装缺口不存在。
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

    /// 从库里存的那个词认回一个字段；认不出就是 `None`。
    ///
    /// [`label`](Self::label) 反着走的那一条。中立库里字段名是**字符串**（锚点是自然键，
    /// 见 `catalog::scrape` 的模块文档），读回来要认回类型的地方就得有它。
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        Self::all().into_iter().find(|field| field.label() == label)
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

    /// 从词认回来。**认不出时是 `None`，不猜**——把一个不认得的类别静默归进
    /// [`Self::Other`]，等于把「这是张说明书」与「这一版不认得这个类别」说成同一件事，
    /// 而后者该被人看见（同 `State::from_label` / `FileKind::from_label` 的规矩）。
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        Some(match label {
            "封面" => Self::Cover,
            "截图" => Self::Screenshot,
            "视频" => Self::Video,
            "其他" => Self::Other,
            _ => return None,
        })
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
///
/// 切换是**任务级别**的：`romcat scrape --profile 在线` 是一趟，不是一个全局开关。
/// 两档共用同一份优先级表与同一份采集缓存，所以「先离线跑全库、再在线补缺口」
/// 是同一份库上的两趟。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Profile {
    /// **离线优先**：只用本地数据源，全库刮一遍不消耗任何在线配额。
    #[default]
    Offline,
    /// **在线优先**：本地源照用，再加联网源补它们补不上的那几样。
    ///
    /// **它不是「只用在线源」**：离线源是免费的，把它们关掉只会让在线源多背几个字段、
    /// 多花几份配额。在线源排在哪一位由优先级表说了算。
    Online,
}

impl Profile {
    /// 打给用户的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Offline => "离线档",
            Self::Online => "在线档",
        }
    }

    /// 从词认回来。命令行的 `--profile` 走它。
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "离线" | "离线档" | "offline" => Some(Self::Offline),
            "在线" | "在线档" | "online" => Some(Self::Online),
            _ => None,
        }
    }

    /// 这一档收哪几种源。
    #[must_use]
    pub fn accepts(self, locality: Locality) -> bool {
        match self {
            Self::Offline => locality == Locality::Local,
            Self::Online => true,
        }
    }
}

/// **判据**：一个已确认的条目撞上 DAT 时用的那几个数。
///
/// 名字不叫 `RomHash`：`ROM` 是词表里 **发行版** 与 **变体** 的 `_Avoid_` 词，而这个
/// 类型说的既不是发行版也不是变体，是**判据**——`identify::fingerprint` 那一侧
/// 用的就是这个词。只有 `rom_name` 保留 `rom`，因为它指的是 Logiqx schema 里那个
/// `<rom>` 记录的名字，也是 ScreenScraper `romnom` 参数要的东西。
///
/// 在线源拿它发查询：ScreenScraper 强制要求「crc / md5 / sha1 之一**加**文件字节大小」
/// 同发（调研 §1.4）。这两样票 07 都已经算好躺在中立库里——**在线档不重新算一遍哈希，
/// 更不重新判断这是哪个游戏**（那是识别的活）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Basis {
    /// 撞上时用的那套 CRC-32（含头或去头，以候选记的为准）。
    pub crc32: u32,
    /// 对应的字节数。**少了它这次查询注定未命中**，白扣一份「未识别 ROM」配额。
    pub bytes: u64,
    /// 那条 DAT 记录里的文件名。只作辅助，判据是哈希。
    pub rom_name: String,
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

/// **作品锚点手里的一个变体**：够拿去撞[中文离线源](zh)的那几样（票 02）。
///
/// ## 为什么作品锚点要带着它们
///
/// **撞只能在变体这一层做。** 高信号只有一处——文件名剥出来的**正题**是中文的，
/// 拿它去撞中文条目的中文名与别名；而作品这一层手里只有 DAT 给的名字（多为英文或
/// 罗马字），实测中文条目里带纯拉丁别名的不到六分之一，作品层自己撞根本撞不上。
///
/// 可**类型、简介、开发商、发行商属于作品**——那正是 [`AnchorKind::Work`] 的定义
/// （跨平台跨地区都成立的东西挂这一层）。挂到变体上就是同一部作品的每个变体各存
/// 一份重复内容。
///
/// 于是中文离线源**在两层都参加**：变体层照旧撞、出中文名与别名；作品层不自己撞名字，
/// 而是把名下这些变体各撞一遍，取撞得最多的那条条目，产出作品级字段。
///
/// ## 带的是「够撞一次的那几样」，不是撞完的结果
///
/// 撞完的结果会与「这一趟哪些锚点被输入指纹跳过了」绑在一起：第一趟采完之后变体那一层
/// 整片命中缓存，`collect` 一次都不跑，作品锚点手里就是空的——于是第二趟会把上一趟
/// 好好的类型当成「这个源改主意了」清掉。带输入而不带结果，作品那一层就与**采集顺序
/// 和缓存状态都无关**：同一份库跑几趟结果一样。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkVariant {
    /// 变体的键。**依据里要写它**——说得出「这条结论是名下哪个变体撞出来的」。
    pub key: String,
    /// 主文件的键；**正题**从它的文件名剥出来。
    pub main_key: String,
    /// 平台；认不出就是 `None`。
    pub platform: Option<String>,
    /// 撞出这个变体的 DAT 条目。年份从它们的名字里读。
    pub entries: Vec<DatEntry>,
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
    /// **作品**锚点才有：它名下的变体，够拿去撞[中文离线源](zh)的那几样（票 02）。
    ///
    /// **变体那一层永远是空的**：那一层撞的是它自己的文件名，不必再带一份名单。
    pub variants: &'a [WorkVariant],
    /// 这一趟单份媒体的上限；`None` 是不设上限。
    ///
    /// 它在这里，是因为**它改变采集的结果**：上限从 32 MiB 提到 128 MiB，同一批文件
    /// 该多收进来几份。凡是改变结果的东西都必须进**输入指纹**，否则调完上限重跑，
    /// 缓存会一口咬定「输入没变」而整条跳过——那些文件永远收不进来。
    pub media_limit: Option<u64>,
    /// **识别确认了这个锚点没有**：有没有一条自动通过的候选。
    ///
    /// 在线源只对已确认的发请求——真库里 46,444 个变体只有 30,024 个被认出来，
    /// 剩下那 16,420 个每问一次都要扣一份 ScreenScraper 的「未识别 ROM」配额，
    /// 而那份配额撞穿的处置是**连账号带 IP 永久封禁**（ADR-0007）。
    pub confirmed: bool,
    /// 已确认时，撞上的那份**判据**。
    ///
    /// **只有作品锚点有**：在线源一部作品只查一次，给每个变体都取一份判据是几万次
    /// 白查的库查询（见 `Plan::build`）。
    pub basis: Option<&'a Basis>,
}

/// 一个源说「这一份东西是这个锚点的某种媒体」。
///
/// 带着**事先知道的字节数**，于是超过上限的那些在**打开文件、或者下载之前**就拦得下来
/// ——少了这一样，一份 662 MiB 的预览视频要先读满上限那一段才发现超了，而在线那一侧
/// 还要连带白花一份配额。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaClaim {
    /// 是封面还是截图还是视频。
    pub kind: MediaKind,
    /// 这份媒体的字节从哪儿来。
    pub from: MediaFrom,
    /// 事先知道的字节数；不知道就是 `None`（主库那侧是 ADR-0021 的第三态，
    /// 在线那侧是源没说）。
    pub bytes: Option<u64>,
    /// **依据**。
    pub why: String,
}

/// 一份媒体的字节从哪儿来。
///
/// 两条路的代价完全不同，所以在类型上分开：主库那一份**读一遍盘**就有，在线那一份
/// 要**花一份配额加一段带宽**。凡是要在「值不值得重来一次」上做判断的地方，
/// 这个差别都是判据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaFrom {
    /// 主库里现成的一份文件，值是它在中立库里的键。
    Library(String),
    /// 在线源给的一个 URL。
    Online {
        /// 从哪儿下。**它是服务器说了算的，因此下之前要过一遍闸门。**
        url: String,
        /// 源说的格式。URL 里往往没有扩展名，而池要一个落盘的名字。
        ext: String,
    },
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
    /// 记**一个单值字段**的值。
    ///
    /// 两种情况**不记**：
    ///
    /// - **空值**。缺的字段就该是缺的，写个空串进去只会让优先级链在它身上停下来。
    /// - **这个字段这个源已经说过话了**。年份、类型、简介这些字段一个源只该有一个
    ///   答案；交出两个只说明它自己没想清楚，而那两个里必有一个是错的。库那一侧照单
    ///   全收（去重键里带着值），所以这道闸只能设在这里——设在库里的话，前端会拿到
    ///   两个互相打架的年份，而谁胜出取决于字典序。
    ///   于是**第一个胜出**：源按确定的顺序遍历条目名，同一份库跑两次结果一样。
    ///
    /// **集合字段**（标题、开发商、发行商）走 [`Harvest::each`]，不是这一条。
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

    /// 记**同一个字段上的又一个值**。
    ///
    /// 与 [`Harvest::value`] 是两种字段的两条路，不是同一条路的宽松版：
    ///
    /// - **单值字段**（年份、类型、简介……）走 `value`，第一个胜出。一个源在这些字段上
    ///   交出两个答案，只说明它自己没想清楚，而那两个里必有一个是错的。
    /// - **集合字段**走这一条。**标题**：中立库里标题永远是集合不是单值
    ///   （`CONTEXT.md` 的**标题集合**词条），同一部作品的中文名与别名本来就该都在里面
    ///   ——用户搜哪个都该找得到。**开发商与发行商**（票 04）：一部作品由几家公司合作
    ///   是常态，数据源自己就写成 `甲、乙`，几家都是真的。合成一串带顿号的长字符串，
    ///   用户在前端里按开发商筛就一家都筛不出来。
    ///
    /// 空值与**这个字段上一模一样的值**照样不记：前者会让优先级链在它身上停下来，
    /// 后者在库里本来就是同一行（去重键带着值），只是白搭一条**依据**。
    pub fn each(&mut self, field: Field, value: impl Into<String>, why: impl Into<String>) {
        let value = value.into();
        if value.trim().is_empty()
            || self
                .values
                .iter()
                .any(|found| found.field == field && found.value == value)
        {
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
/// **采集不碰字节**——它只说「主库里这个键的文件是这个变体的封面」「这个 URL 是这部
/// 作品的封面」，读盘、下载、算哈希、往池里放全都归引擎。这样源可以完全在内存里测，
/// 而全部 IO（连同那道限流与配额闸）收在一处。
///
/// 本地源因此**几乎**永远返回 `Ok(())`。唯一的例外是[中文离线源](zh)的作品那一层：
/// 简介不跟着索引进内存，撞上之后要按条目号去本机那份库里点一次名（`zh::Summaries`），
/// 而那一下读得出读不出是会失败的。它照 [`Failure::Skip`] 处置——**那一对不写库，
/// 下一趟再来**，不吞成「这条没有简介」。
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
    ///
    /// # Errors
    /// 这一条没采到时返回 [`Failure::Skip`]（那一对**不写库**，下一趟再来）；
    /// 整趟该停时返回 [`Failure::Halt`]（配额超限、凭据不对、网断了）。
    fn collect(&self, subject: &Subject<'_>, out: &mut Harvest) -> Result<(), Failure>;
}

/// **采法**：这一趟怎么采、跑多久。
///
/// 名字不叫 `Sweep`：界面那一侧已经有一个 `bench::Sweep`（量帧率时怎么扫过那张表），
/// 两个不相干的东西同名，看代码的人迟早会把它们当成一件事。
///
/// 它是界面上那第四个旋钮（票 `gui-redesign/10`），也是命令行 `--refresh` 的那一档。
/// 两个词摆在一处，是为了让「界面上按的那个」与「命令行打的那个」说的是同一件事。
///
/// **它管的是跑多久、花多少配额，不管显示哪个值。** 「有值了但我想换一个」不该靠重采
/// ——刮削结果按「锚点 × 字段 × 源」三元组**并存**，没有覆盖这回事，换的是
/// [优先级](Priorities)，改一次排序、零成本、不重跑。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Gather {
    /// **补缺**：输入指纹没变的整条跳过，只采还没采过的那些。
    #[default]
    Fill,
    /// **重采**：绕过输入指纹全部重来。
    ///
    /// 真正需要它的只有两种：数据源更新了，或者解析逻辑改了
    /// （[`Source::probe`] 盖不住的正是后者）。
    Refresh,
}

impl Gather {
    /// 打给用户的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Fill => "补缺",
            Self::Refresh => "重采",
        }
    }

    /// 一句说清它是什么。
    #[must_use]
    pub fn why(self) -> &'static str {
        match self {
            Self::Fill => "只采一个源都没给过值的",
            Self::Refresh => "绕过输入指纹，全部重来",
        }
    }

    /// 两档摆在一起的次序。
    #[must_use]
    pub fn all() -> [Self; 2] {
        [Self::Fill, Self::Refresh]
    }

    /// 落到选项上。**这是这两个词与 [`Options::refresh`] 之间唯一的一处映射**——
    /// 各处自己判一遍的话，迟早有一处把「补缺」判成了重采，而那一处会白烧一天的配额。
    pub fn apply(self, options: &mut Options) {
        options.refresh = self == Self::Refresh;
    }
}

/// 刮削的选项。
#[derive(Debug, Clone)]
pub struct Options {
    /// 主库那**一组根**：拿变体的键第一段查出那块盘在哪。
    /// 只有要把本地媒体读进池里时才用得上。
    pub roots: Roots,
    /// 媒体池在哪。
    pub pool: PathBuf,
    /// 策略档案。
    pub profile: Profile,
    /// 收不收媒体。关掉之后一个字节都不读主库。
    pub media: bool,
    /// **这一趟要哪几个字段**。不在这份名单里的，采到了也不落库。
    ///
    /// 默认是全部（[`Field::all`]），命令行走的就是这一份。界面上那个「字段」旋钮
    /// （票 `gui-redesign/10`）换的正是它。
    ///
    /// **不在名单里的字段，库里已有的值一个字都不动**：三元组并存、没有覆盖
    /// （ADR-0007 与 `catalog::scrape` 的模块文档），而「这一趟只要简介」不该把上一趟
    /// 采到的类型抹掉。落库那一侧因此只在这份名单之内替换（`put_scraped_within`）。
    pub fields: BTreeSet<Field>,
    /// 单份媒体大到多少字节就不收了；`None` 是不设上限。
    pub max_media_bytes: Option<u64>,
    /// 无视缓存，全部重采。
    pub refresh: bool,
    /// **只采这些变体**，连同它们所属的作品；`None` 是全库。
    ///
    /// 界面上那个「范围」旋钮就是它：浏览屏筛出来的那一批变体的键
    /// （`Catalog::scoped_variants`）。命令行不给这个开关，一律全库。
    ///
    /// **作品锚点手里的变体名单不跟着收窄**：那份名单进输入指纹，跟着范围变的话，
    /// 同一部作品先刮一半再刮另一半会得出两个不同的结论。范围管的是「过哪些锚点」，
    /// 不是「一个锚点看得见什么」。
    pub only: Option<BTreeSet<String>>,
    /// 每采完多少个锚点就写一批进中立库。
    pub write_batch: usize,
}

impl Options {
    /// 对着一组主库根与某个媒体池的默认选项。
    #[must_use]
    pub fn new(roots: Roots, pool: impl Into<PathBuf>) -> Self {
        Self {
            roots,
            pool: pool.into(),
            profile: Profile::Offline,
            media: true,
            fields: Field::all().into_iter().collect(),
            max_media_bytes: None,
            refresh: false,
            only: None,
            write_batch: 2_000,
        }
    }

    /// 这一趟**字段一个都不少**吗。
    ///
    /// 它是那道「指纹要不要跟着变」的判据：一个都不少时指纹一个字都不折，
    /// 于是既有的库不会因为多出这个旋钮而整片重采。
    #[must_use]
    pub fn wants_all_fields(&self) -> bool {
        Field::all().iter().all(|field| self.fields.contains(field))
    }

    /// 一个源报上来的**输入指纹**，折上这一趟「要什么」之后的那一份。
    ///
    /// 存进库、也拿来比对的都是它。**要什么改变结果，所以它必须进指纹**
    /// （同单份媒体上限那一条）：少了它，「先只要简介、再要类型」的第二趟会被缓存
    /// 一口咬定「输入没变」而整条跳过，那个类型就永远补不上。
    ///
    /// 两样折进来的东西各有各的作用面，**分开折而不是一把折进去**：
    ///
    /// - **字段名单**对每个源都成立：名单窄了，这个源落库的值就少了。
    /// - **收不收媒体**只对**联网源**成立。本地媒体那个源在不收媒体时**整个不参加**
    ///   （见 [`sources`]），别的本地源本来就不出媒体——把这一样折进它们的指纹，
    ///   等于「命令行跑一趟、界面不收媒体跑一趟」两边互相把对方的缓存作废掉，
    ///   而那两趟对这些源来说产出一模一样。
    pub(crate) fn cache_key(&self, locality: Locality, probed: String) -> String {
        let mut mark: Vec<&str> = Vec::new();
        let fields: Vec<&str>;
        if !self.wants_all_fields() {
            fields = self.fields.iter().map(|field| field.label()).collect();
            mark.extend(fields.iter().copied());
        }
        if locality == Locality::Online && !self.media {
            mark.push("不收媒体");
        }
        if mark.is_empty() {
            return probed;
        }
        fingerprint(&[&probed, &mark.join("+")])
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
    /// 有几个「锚点 × 源」这次没采成、**留着下一趟再来**。
    ///
    /// 与 `forgotten` 是相反的两件事：那个是「问过了，答案是没有」，这个是「没问到」。
    /// 没问到的**一个字都不写库**——写了就等于宣布这一对采全了，下一趟会整条跳过。
    pub skipped: u64,
    /// 源说「这一份媒体没有」的有几份。**不是错误**：那是一条结论，
    /// 下一趟不会为同一张不存在的图再花一份配额。
    pub missing_media: u64,
    /// 头一个「没采成」是为什么。
    ///
    /// **只留第一句**：一万条一样的抱怨没用，而一句都不留，用户只看得到一个数字——
    /// 「有 900 个没采成」既分不出是网断了还是凭据过期了，也就没法决定下一步做什么。
    pub first_skip: Option<String>,
    /// 这一趟停了没有，为什么。**不是错误**：已经采完的那部分留在中立库里，重跑接着采。
    pub halted: Option<Halt>,
    /// 在线那一侧用掉了多少。离线档是 `None`。
    pub online: Option<online::Usage>,
}

/// 一趟刮削。
///
/// **离线档不联网**，**两档都不写主库一个字节**（ADR-0004）：读主库只发生在一处——
/// 把一份本地媒体读进**媒体池**，而那也可以用 `options.media = false` 关掉。
///
/// `net` 给不给由档案定：离线档**给了也不会碰**（闸门查的是源自报的 `Locality`），
/// 在线档不给就起不来。
///
/// ## 停下来不等于失败
///
/// 撞上配额、凭据不对、网断了，这一趟会**停**：把已经采完的那批写进中立库、照常出报告，
/// 在 [`Outcome::halted`] 里说清为什么。做成错误的话，跑到 80% 撞上配额就会把那 80%
/// 一起丢掉——而配额是每天才回一次的东西。
///
/// # Errors
/// 中立库读写不了、媒体池建不出来、档案里混进了它不该有的源、或者在线档没有网络句柄
/// 时返回错误。
pub fn run(
    library: &dyn LibraryFs,
    catalog: &mut Catalog,
    priorities: &Priorities,
    options: &Options,
    net: Option<&Net<'_>>,
    context: &mut RunContext<'_>,
) -> Result<Outcome, ScrapeError> {
    let sources = sources(
        options.profile,
        options.media,
        net,
        context.naming,
        context.summaries,
        context.rulings,
    )?;
    // **不收媒体就一个目录都不建。** `--no-media` 说的是「这趟不收媒体」，而顺手在
    // 工作目录里建出两个空的池目录也算食言（同 `MediaPool::at` 的文档）。收媒体那一档
    // 照旧先把池建出来——`ingest` 要往里写文件。
    let pool = if options.media {
        MediaPool::open(&options.pool)?
    } else {
        MediaPool::at(&options.pool)
    };
    let into = Ingesting {
        library,
        pool: &pool,
        options,
        net,
    };

    let plan = Plan::build(catalog, options)?;
    let mut run = Run::default();
    let mut batch: Vec<Harvested> = Vec::new();
    let mut done = 0_u64;
    let total = u64::try_from(plan.subjects.len()).unwrap_or(u64::MAX);
    let mut interrupted = false;
    let mut halted: Option<Halt> = None;

    'subjects: for subject in &plan.subjects {
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
            // 这一趟「要什么」折进指纹（[`Options::cache_key`]）：字段选窄了再选宽，
            // 第二趟该真的重采。什么都不少时它一个字都不折。
            let input = options.cache_key(source.locality(), input);
            // **重采就是不看这道指纹**，不是「先把库清空再重来」。
            //
            // 清空那条路在两处会出事，而两处都是这一票要防的：一是它跑在采集**之前**，
            // 中途按停下或者撞上配额（`break 'subjects`）时，没走到的锚点就只剩空的
            // ——而 `Panel::settle` 对停下来那一档说的是「已经采到的那些留在中立库里」；
            // 二是它无条件删这一批的 `media_ref`，而「这趟不收媒体」时那些引用根本写不回来
            // （`put_scraped_within` 正是为此特意跳过那一句 DELETE）。
            //
            // 不看指纹就没有这两件事：每一对照旧走 `collect` 加 `put_scraped_within`，
            // **该替换的替换、该留的留**，半路停下也只是少采几个锚点。
            if !options.refresh && known.get(source.name()) == Some(&input) {
                run.reused_probes += 1;
                continue;
            }
            let mut harvest = Harvest::default();
            match source.collect(&view, &mut harvest) {
                Ok(()) => {}
                // **没问到就什么都不写。** 写了等于宣布这一对采全了，下一趟整条跳过，
                // 那条结论就永远缺着——而这里的典型成因只是网抖了一下。
                Err(Failure::Skip { why }) => {
                    run.note_skip(why);
                    continue;
                }
                Err(Failure::Halt(reason)) => {
                    halted = Some(reason);
                    break 'subjects;
                }
            }
            // **这一趟不要的东西，采到了也不带走。** 两条各有各的道理：
            //
            // - 不要的**字段**丢在这里而不是丢在源里：源只管「我看得出什么」，
            //   要不要是这一趟的事，让每个源各写一遍过滤只会让七处漏一处。
            // - 不收媒体时**媒体一律清掉**，在线那一侧尤其要紧：一次条目查询本来就带回
            //   一串图的 URL，不清的话「不收媒体」照样会为每张图各花一份配额加一段带宽
            //   ——而那正是这个开关要省下来的东西。
            harvest
                .values
                .retain(|found| options.fields.contains(&found.field));
            if !options.media {
                harvest.media.clear();
            }
            let media = match ingest_all(&into, catalog, &harvest, &mut run) {
                Ok(media) => media,
                Err(Stop::Incomplete(why)) => {
                    run.note_skip(why);
                    continue;
                }
                Err(Stop::Halt(reason)) => {
                    halted = Some(reason);
                    break 'subjects;
                }
                Err(Stop::Fatal(error)) => return Err(error),
            };
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
            catalog.put_scraped_within(&batch, &options.fields, options.media)?;
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
    // **停下来之前先落库。** 这一句就是「网络失败不影响已完成的部分」：撞上配额时
    // 手里那一批照样写进去，重跑从这儿接着采。
    catalog.put_scraped_within(&batch, &options.fields, options.media)?;
    (context.progress)(Progress {
        done,
        total,
        media: run.media_count,
        read_bytes: run.read_bytes,
    });

    let names: Vec<&str> = sources.iter().map(|source| source.name()).collect();
    let online = net.map(Net::usage);
    let report = ScrapeReport::build(
        catalog,
        priorities,
        options,
        &report::Run {
            sources: &names,
            plan: &plan.counts,
            online,
            halted: halted.as_ref().map(Halt::describe),
        },
    )?;
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
        skipped: run.skipped,
        missing_media: run.missing_media,
        first_skip: run.first_skip,
        halted,
        online,
    })
}

/// 一次采集半路上停下来的三种理由。
///
/// 三种的处置完全不同，所以分得开：**没采全**下一趟再来（不写库）、**整趟停**要把手里
/// 那批先落库、**真出错**才往上抛。
enum Stop {
    /// 这一对没采全（下媒体时网抖了一下）。**不写进采集记录**——写了下一趟就跳过了。
    Incomplete(String),
    /// 整趟到此为止。
    Halt(Halt),
    /// 中立库或媒体池真的坏了。
    Fatal(ScrapeError),
}

impl From<ScrapeError> for Stop {
    fn from(error: ScrapeError) -> Self {
        Self::Fatal(error)
    }
}

impl From<CatalogError> for Stop {
    fn from(error: CatalogError) -> Self {
        Self::Fatal(error.into())
    }
}

impl From<PoolError> for Stop {
    fn from(error: PoolError) -> Self {
        Self::Fatal(error.into())
    }
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
    /// **中文离线源**认得的东西与调得动的参数（票 11）。
    ///
    /// 它在这里而不在 [`Options`] 里，是因为它借着一份整装在内存里的索引活着，
    /// 而 `Options` 是一份可以随手 clone 的配置。[`Naming::off`](fuzzy::Naming::off)
    /// 是「这个源不参加」的那一份。
    pub naming: &'a fuzzy::Naming<'a>,
    /// **中文简介**从哪儿读；`None` 表示这条路没接上，这一趟一条简介都不产出（票 03）。
    ///
    /// 它与 `naming` 分开一格，是因为两者的代价与寿命都不同：索引整份装在内存里，
    /// 而简介留在本机那份库里、按条目号点着读（`zh::Summaries` 的文档）。
    /// 本机那份 [`zh::store::Store`](crate::zh::store::Store) 直接就是它的一个实现。
    pub summaries: Option<&'a dyn zh::Summaries>,
    /// **匹配裁决**按变体键摊平之后的那一份（票 05）。
    ///
    /// 人在队列里说过「这一次撞错了」的那些，在这里；[`zh::Rulings::none`] 是「一条都
    /// 没裁过」的那一份。它与 `naming`、`summaries` 分开一格的理由同上——三样的来处、
    /// 代价与寿命都不同：规则与索引在内存里，简介在中文索引那份库里，而裁决在**沉淀库**
    /// 里，那是三份库里唯一不可再生的一份。
    pub rulings: &'a zh::Rulings,
}

/// 一串输入折成**输入指纹**。
///
/// 取 SHA-256 的前 16 位十六进制。全长 64 位存进库里，46,444 个变体 × 7 个源就是
/// 20 MB 的纯噪音；16 位（64 比特）在这个量级上撞一次的概率可以忽略，而撞了的后果
/// 只是**少重采一次**——不是错，是慢一步被发现。
pub(crate) fn fingerprint(parts: &[&str]) -> String {
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    for part in parts {
        context.update(part.as_bytes());
        // 分隔符不能省：`["ab", "c"]` 与 `["a", "bc"]` 拼起来是同一串。
        context.update(&[0]);
    }
    pool::hex(&context.finish().as_ref()[..8])
}

/// 这个程序认得的全部源，两档合起来。
///
/// 报告拿它分辨「优先级表里点名了一个不存在的源」（多半是打错字）与「点名了一个这一档
/// 没参加的源」（正常，换个档案就有了）。混成一件事，用户每跑一趟离线档都会看见一句
/// 「ScreenScraper 不存在」。
///
/// **适配器也在里面**：`romcat import` 把维护者手工维护的前端元数据落成
/// `scrape_value`，源名就是那个适配器的名字（票 16）。它不参加 [`run`] 这一趟——
/// 它是导入那一趟产出的——但它**确实往同一张表里写值**，因此优先级表点名它是正当的。
/// 漏在这里，`priorities.toml` 里那一行就会被报成「打错字」。
#[must_use]
pub fn all_source_names() -> Vec<&'static str> {
    let mut out = vec![
        "No-Intro",
        "Redump",
        "TOSEC",
        "MAME",
        "GoodNES",
        local::FILENAME,
        local::LOCAL_MEDIA,
        fuzzy::SOURCE,
        fuzzy::ALIAS_SOURCE,
        online::SCREEN_SCRAPER,
    ];
    out.extend(crate::adapter::names());
    out
}

/// 这一档的全部源。**顺序无关**——谁排前面由优先级表说了算，不由这里说了算。
///
/// `media` 为假时**本地媒体源整个不参加**，而不是喂给它一份空的媒体清单。差别是要命的：
/// 空清单会让它的 `probe` 返回「无话可说」，而「无话可说 + 上次说过话」正是引擎清掉
/// 上一轮结论的那一态——于是 `--no-media` 会把此前收好的媒体映射**悄悄删掉**。
/// `--no-media` 说的是「这趟不收媒体」，不是「把收过的扔了」。
fn sources<'a>(
    profile: Profile,
    media: bool,
    net: Option<&'a Net<'a>>,
    naming: &'a fuzzy::Naming<'a>,
    summaries: Option<&'a dyn zh::Summaries>,
    rulings: &'a zh::Rulings,
) -> Result<Vec<Box<dyn Source + 'a>>, ScrapeError> {
    let mut sources: Vec<Box<dyn Source + 'a>> = vec![
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
    // **中文离线源只在取过数之后参加**（票 11）。没取过就整个不造这个源——
    // 与「`--no-media` 时本地媒体源整个不参加」同一条道理：造一个永远无话可说的源，
    // 会让引擎把它上一轮说过的话当成「这次改主意了」而清掉。
    if naming.ready() {
        // **简介那条路接得上就接上**（票 03）：接不上时这个源照样参加，只是这一趟
        // 一条简介都不产出——而那件事进它的输入指纹，接上之后重跑会真的重采。
        // **匹配裁决两路都接上**（票 05）：中文名与别名撞的是同一次，人否定了那一次，
        // 两路一起不产出。接的是同一份，所以不可能一路认裁决另一路不认。
        let mut chinese = zh::ChineseSource::new(*naming).with_rulings(rulings);
        if let Some(summaries) = summaries {
            chinese = chinese.with_summaries(summaries);
        }
        sources.push(Box::new(chinese));
        // **别名那一路单开一个源名**，好让优先级表把它排在标题那条链的最后：
        // 别名只进标题集合、只管搜得到，永不当显示标题（`zh::ChineseAliasSource`）。
        sources.push(Box::new(
            zh::ChineseAliasSource::new(*naming).with_rulings(rulings),
        ));
    }
    // **联网源只在在线档里造出来。** 离线档拿到 `Some(net)` 也不会碰它——这一条
    // 比「参数表里没有网络句柄」硬：句柄可以从别处传进来，而这里根本不造那个源。
    if profile == Profile::Online {
        let net = net.ok_or(ScrapeError::NoNetwork)?;
        sources.push(Box::new(online::ScreenScraper::new(net)));
    }
    // **闸门**：档案说不收的源，一个都不许混进来。写在这里而不是靠上面那张表自觉，
    // 是因为源的清单会一直长，而加的时候最容易忘的就是档案这一层。
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
    /// **有判据可以拿去发在线查询**的作品锚点几个。
    ///
    /// **只有在线档算这个数**（`None` 表示这一趟没算）：取判据要逐个查中立库，
    /// 真库上那是 9,226 次查询，而离线档一次都用不上它。
    pub queryable_works: Option<u64>,
    /// 未被识别确认、因此**一个在线请求都不会为它发出去**的变体有几个。
    ///
    /// 报告必须把这个数说出来：它既是「在线档为什么补不上这些」的解释，
    /// 也是「配额没有被这些烧掉」的凭据。两档都算得起——判据是变体那一行上的
    /// 作品链接，不必再查一次库。
    pub unconfirmed_variants: u64,
}

struct PlannedSubject {
    kind: AnchorKind,
    id: String,
    platform: Option<String>,
    entries: Vec<DatEntry>,
    main_key: Option<String>,
    media: Vec<LocalMedia>,
    variants: Vec<WorkVariant>,
    basis: Option<Basis>,
    confirmed: bool,
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
            variants: &self.variants,
            media_limit,
            confirmed: self.confirmed,
            basis: self.basis.as_ref(),
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

        // 作品锚点：把它下面全部变体的候选并起来。平台取第一个说得出的——同一部作品
        // 跨平台时哪个都不算错，而平台在这一层只用于按平台覆写优先级。
        let mut work_entries: BTreeMap<String, WorkSlot> = BTreeMap::new();
        let mut subjects = Vec::with_capacity(variants.len() + works.len());
        let mut unconfirmed = 0_u64;
        let mut local_media = 0_u64;
        // **范围之外的作品名**，用来把作品锚点也收窄。**攒的是名字不是判断**：
        // 一部作品名下的变体只要有一个在范围里，这部作品就要采——它的简介与封面挂在
        // 作品这一层，漏掉它等于这一批一个字段都补不上。
        let mut wanted_works: BTreeSet<String> = BTreeSet::new();
        for variant in &variants {
            // **范围只决定「过哪些锚点」。** 作品那一格照旧由**全部**变体攒起来
            // （名单、条目、代表变体），于是同一部作品先刮一半再刮另一半得出的是
            // 同一个结论——那份名单进输入指纹，跟着范围抖动的话缓存就永远对不上。
            let taken = options
                .only
                .as_ref()
                .is_none_or(|only| only.contains(&variant.key));
            let entries = by_variant.remove(&variant.key).unwrap_or_default();
            // **「已确认」的判据是有一条自动通过的候选**，不是「有发行版链接」：
            // 汉化版认得出是哪部作品、认不出基于哪一条发行版，发行版那一列本来就空着
            // （ADR-0012），拿它当判据会把整批汉化版划成未识别。判据在变体这一行上
            // 就有，不必再查一次库——于是两个档位都算得起这个数。
            let confirmed = variant.work_id.is_some();
            if !confirmed && taken {
                unconfirmed += 1;
            }
            if let Some(name) = variant.work_id.and_then(|id| works.get(&id)) {
                if taken {
                    wanted_works.insert(name.clone());
                }
                let slot = work_entries
                    .entry(name.clone())
                    .or_insert_with(|| WorkSlot {
                        platform: variant.platform.clone(),
                        entries: Vec::new(),
                        variants: Vec::new(),
                        representative: None,
                    });
                if slot.platform.is_none() {
                    slot.platform.clone_from(&variant.platform);
                }
                slot.entries.extend(entries.iter().cloned());
                // **作品层不自己撞名字，读的是名下变体撞出来的条目**（票 02）。
                // 这里攒的是「够撞一次的那几样」而不是撞完的结果，理由见 [`WorkVariant`]。
                slot.variants.push(WorkVariant {
                    key: variant.key.clone(),
                    main_key: variant.main_key.clone(),
                    platform: variant.platform.clone(),
                    entries: entries.clone(),
                });
                // **一部作品发一次查询就够**，所以只留一个代表变体。变体按键排序遍历，
                // 于是同一份库跑两次挑中的是同一个。按变体查等于把配额乘上三倍
                // （真库 30,024 个已确认变体 vs 9,226 部作品），而简介与封面本来就是
                // 挂在**作品**这一层的（`CONTEXT.md`）。
                if slot.representative.is_none() {
                    slot.representative = Some(variant.key.clone());
                }
            }
            if !taken {
                continue;
            }
            let media = media_index.get(&variant.key).cloned().unwrap_or_default();
            local_media += u64::try_from(media.len()).unwrap_or(0);
            subjects.push(PlannedSubject {
                kind: AnchorKind::Variant,
                id: variant.key.clone(),
                platform: variant.platform.clone(),
                entries,
                main_key: Some(variant.main_key.clone()),
                media,
                // 变体这一层撞的是它自己的文件名，不必再带一份名单。
                variants: Vec::new(),
                // **变体这一层不带判据**：在线源只查作品锚点，给每个变体都取一次判据
                // 就是 30,024 次白查的库查询（实测那一层不便宜）。
                basis: None,
                confirmed,
            });
        }
        let works_count = u64::try_from(wanted_works.len()).unwrap_or(u64::MAX);
        // **只有在线档取判据。** 取一次是一次库查询，离线档一次都用不上。
        let online = options.profile == Profile::Online;
        let mut queryable = 0_u64;
        for (name, slot) in work_entries {
            if !wanted_works.contains(&name) {
                continue;
            }
            let mut entries = slot.entries;
            entries.sort();
            entries.dedup();
            let basis = match (online, &slot.representative) {
                (true, Some(key)) => {
                    catalog
                        .accepted_hash(key)?
                        .map(|(crc32, bytes, rom_name)| Basis {
                            crc32,
                            bytes,
                            rom_name,
                        })
                }
                _ => None,
            };
            if basis.is_some() {
                queryable += 1;
            }
            subjects.push(PlannedSubject {
                kind: AnchorKind::Work,
                id: name,
                platform: slot.platform,
                entries,
                main_key: None,
                media: Vec::new(),
                variants: slot.variants,
                confirmed: slot.representative.is_some(),
                basis,
            });
        }
        Ok(Self {
            counts: PlanCounts {
                works: works_count,
                variants: u64::try_from(variants.len()).unwrap_or(u64::MAX),
                local_media,
                queryable_works: online.then_some(queryable),
                unconfirmed_variants: unconfirmed,
            },
            subjects,
        })
    }
}

/// 攒一个作品锚点时手里的那几样。
struct WorkSlot {
    platform: Option<String>,
    entries: Vec<DatEntry>,
    /// 名下的变体，够拿去撞**中文离线源**的那几样（[`WorkVariant`]）。
    variants: Vec<WorkVariant>,
    /// 拿哪个变体的判据去发那**一次**在线查询。
    representative: Option<String>,
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
    skipped: u64,
    missing_media: u64,
    media_count: u64,
    first_skip: Option<String>,
}

impl Run {
    /// 记一次「这一对没采成」。**留下第一句理由**：一万条一样的抱怨没用，
    /// 而一句都不留，用户只看得到一个数字。
    fn note_skip(&mut self, why: String) {
        self.skipped += 1;
        if self.first_skip.is_none() {
            self.first_skip = Some(why);
        }
    }
}

/// 媒体进池要的那几样外部东西。
///
/// 捏成一个结构而不是四个参数，与 [`RunContext`]、[`pool::Claim`] 是同一条纪律：
/// 它们本来就成群结队地一起走。
struct Ingesting<'a> {
    /// 主库的只读视图。
    library: &'a dyn LibraryFs,
    /// 媒体池。
    pool: &'a MediaPool,
    /// 这一趟的选项（主库根、单份上限）。
    options: &'a Options,
    /// 网络句柄；离线档是 `None`。
    net: Option<&'a Net<'a>>,
}

/// 把一次采集里的媒体全部收进池里，返回写库要的 `(类型, 哈希, 依据)`。
fn ingest_all(
    into: &Ingesting<'_>,
    catalog: &mut Catalog,
    harvest: &Harvest,
    run: &mut Run,
) -> Result<Vec<HarvestedMedia>, Stop> {
    let mut out = Vec::with_capacity(harvest.media.len());
    for claim in &harvest.media {
        let want = pool::Claim {
            key: match &claim.from {
                MediaFrom::Library(key) => key,
                MediaFrom::Online { url, .. } => url,
            },
            bytes: claim.bytes,
            max_bytes: into.options.max_media_bytes,
        };
        let ingested = match &claim.from {
            MediaFrom::Library(_) => {
                pool::ingest(into.library, catalog, into.pool, &into.options.roots, &want)?
            }
            MediaFrom::Online { ext, .. } => {
                let Some(net) = into.net else {
                    // 在线源造得出来就一定有 net；这一支只在有人把源接错时到得了。
                    return Err(Stop::Fatal(ScrapeError::NoNetwork));
                };
                ingest_online(net, catalog, into.pool, &want, ext)?
            }
        };
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
            // 几种跳过**分开数**。它们不是同一件事，塞进同一个计数器，报告就只能说
            // 「有 108 份没收进来」而说不出为什么——而它们的处置各不相同：超上限是
            // 自己设的，读不动是 ADR-0021 的第三态，闸门拦下是要去看一眼的，
            // 源说没有则什么都不必做。
            pool::Ingested::TooBig { .. } => run.oversized_media += 1,
            pool::Ingested::Unreadable { .. } => run.unreadable_media += 1,
            // 闸门拦下的数记在 `Net` 那一处，报告从它取——查询 URL 与媒体 URL 被拦下
            // 是同一件事，各记一个计数器只会让两个数对不上。
            pool::Ingested::Refused { .. } => {}
            pool::Ingested::Missing { .. } => run.missing_media += 1,
            pool::Ingested::NotMedia { .. } => run.not_media += 1,
        }
    }
    run.media_count += u64::try_from(out.len()).unwrap_or(0);
    Ok(out)
}

/// 把一份在线媒体下进池里。
///
/// 四道先手，全都是为了**别白花配额与带宽**：
///
/// 1. **闸门在发出去之前查**——媒体 URL 是服务器说了算的；
/// 2. **源说了多大就先按它判上限**——超了当场不下。这一条是在线这一侧最要紧的：
///    真库里最大的一份预览媒体有 662 MiB，先下回来再嫌它大，配额与带宽都已经花掉了；
/// 3. **下过的不重下**——`media_remote` 记着这个 URL 算出来是哪一份内容；
/// 4. 下回来之后再按实际大小兜一道——源报的大小可能是假的。
fn ingest_online(
    net: &Net<'_>,
    catalog: &mut Catalog,
    pool: &MediaPool,
    claim: &pool::Claim<'_>,
    ext: &str,
) -> Result<pool::Ingested, Stop> {
    let url = claim.key;
    // **闸门先查，而且这一条的结果是永久的**：URL 指向白名单之外，下一趟还是同一个
    // 答案。于是它不让这一对「没采全」，只是记一笔——否则这一对会永远重采下去。
    // 这里查一遍、`Net::request` 里再查一遍，**不是重复**：这一道要的是「拦下之后
    // 该怎么办」——它是永久的判断，所以不让这一对重采；那一道守的是「一个字节都
    // 不许发到禁区去」，它得挡住每一条走到发送口的 URL，不管谁先查过。
    if let Err(refusal) = crate::dat::guard::check(url) {
        net.refused();
        return Ok(pool::Ingested::Refused {
            why: format!("{url} 被取数闸门拦下：{refusal}"),
        });
    }
    // 上限先于复用判，也**先于下载**：源自报的大小与上一趟记下的大小，谁说得出算谁的。
    // 调低上限之后，此前下进来的那一份也该被挡在外面（同本地那一侧）。
    let known = catalog.remote_media(url)?;
    let size = claim.bytes.or(known.as_ref().map(|(bytes, _)| *bytes));
    if let (Some(cap), Some(bytes)) = (claim.max_bytes, size)
        && bytes > cap
    {
        return Ok(pool::Ingested::TooBig { bytes: Some(bytes) });
    }
    if let Some((_, hash)) = known {
        let stored = catalog.media_ext(&hash)?;
        if let Some(stored) = stored
            && pool.contains(&hash, &stored)
        {
            return Ok(pool::Ingested::Reused { hash });
        }
    }

    let bytes = match net.request(online::SCREEN_SCRAPER, url, online::Ask::Media) {
        Ok(Some(bytes)) => bytes,
        // **服务器说这张图没有。** 那是一条结论而不是失败：这一对照样算采全了，
        // 只是少一张图。当成失败的话，下一趟会为同一张不存在的图再花一份配额。
        Ok(None) => {
            return Ok(pool::Ingested::Missing {
                why: format!("{url} 那份媒体服务器说没有"),
            });
        }
        Err(Failure::Halt(reason)) => return Err(Stop::Halt(reason)),
        // 网抖了：**这一对整个不写库**，下一趟重来。少写一份图而把这一对记成采全了，
        // 那份图就永远缺着。
        Err(Failure::Skip { why }) => return Err(Stop::Incomplete(why)),
    };
    // 兜底：源报的大小可能是假的，也可能根本没报。
    let got = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if let Some(cap) = claim.max_bytes
        && got > cap
    {
        return Ok(pool::Ingested::TooBig { bytes: Some(got) });
    }
    let (hash, fresh) = pool::store(pool, catalog, &bytes, ext)?;
    catalog.put_remote_media(url, &hash, got)?;
    Ok(pool::Ingested::Stored {
        hash,
        bytes: got,
        fresh,
    })
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
            fn collect(&self, _: &Subject<'_>, _: &mut Harvest) -> Result<(), Failure> {
                Ok(())
            }
        }
        let source = 假源;
        assert!(!Profile::Offline.accepts(source.locality()));
        assert!(Profile::Offline.accepts(Locality::Local));
        assert!(Profile::Online.accepts(Locality::Online));
        // **在线档不是「只用在线源」**：离线源免费，关掉它们只会让在线源多花配额。
        assert!(Profile::Online.accepts(Locality::Local));
    }

    #[test]
    fn 离线档那七个源全是本地的() {
        let 关掉 = fuzzy::Naming::off();
        let 无裁决 = zh::Rulings::none();
        let sources =
            sources(Profile::Offline, true, None, &关掉, None, &无裁决).expect("离线档该收得下这七个源");
        assert_eq!(sources.len(), 7);
        assert!(sources.iter().all(|s| s.locality() == Locality::Local));
    }

    #[test]
    fn 离线档拿到网络句柄也不造联网源() {
        // 「离线档不联网」的保证不在参数表上——句柄可以从别处传进来。它在这里：
        // 档案是离线时，联网源**根本不会被造出来**。
        let fetcher = crate::dat::CannedFetcher::new();
        let cancel = CancelToken::new();
        let net = Net::new(
            &fetcher,
            online::Limits::default(),
            online::Credentials {
                dev_id: "测试".to_string(),
                dev_password: "口令".to_string(),
                soft_name: "romcat-test".to_string(),
                user: None,
                user_password: None,
            },
            &cancel,
        );
        let 关掉 = fuzzy::Naming::off();
        let 无裁决 = zh::Rulings::none();
        let sources = sources(Profile::Offline, true, Some(&net), &关掉, None, &无裁决).expect("收得下");
        assert!(sources.iter().all(|s| s.locality() == Locality::Local));
        assert!(fetcher.asked().is_empty());
    }

    #[test]
    fn 在线档没有网络句柄就不启动() {
        // **宁可不启动也不悄悄降级**：用户点名要在线档，要的正是离线档补不上的那几样。
        let 关掉 = fuzzy::Naming::off();
        let 无裁决 = zh::Rulings::none();
        assert!(matches!(
            sources(Profile::Online, true, None, &关掉, None, &无裁决),
            Err(ScrapeError::NoNetwork)
        ));
    }

    #[test]
    fn 不收媒体时本地媒体源整个不参加() {
        // 它若参加而拿到一份空清单，`probe` 会返回「无话可说」，
        // 上一轮收好的媒体映射就被当成过期结论清掉了。
        let 关掉 = fuzzy::Naming::off();
        let 无裁决 = zh::Rulings::none();
        let sources = sources(Profile::Offline, false, None, &关掉, None, &无裁决).expect("收得下");
        assert_eq!(sources.len(), 6);
        assert!(sources.iter().all(|s| s.name() != local::LOCAL_MEDIA));
    }

    #[test]
    fn 每个源都在优先级表里有位置() {
        // 优先级表点名了一个不存在的源，后果是**静默**的：它被排到链尾，用户以为
        // 自己调了优先级，实际什么也没发生。反过来，程序有而表里没有的源同样静默。
        let priorities = Priorities::builtin();
        assert!(
            priorities.sources_not_in(&all_source_names()).is_empty(),
            "内置优先级表里点名的源，这个程序全都有"
        );
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
