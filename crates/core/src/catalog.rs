//! **中立库**：工具自己的事实来源（ADR-0001）。
//!
//! 扫描的结论落在这里，而不是每次重新遍历一遍 10T 主库。它带来两件事：
//!
//! - **报告不必扫盘**。体检报告由 [`Catalog::aggregate`] 从库里的记录现折出来，
//!   外置盘不在位时照样出得来——中立库存在**本机**而不是跟着盘走（ADR-0009）。
//! - **增量**。库里记着上次每个文件的 `(路径, 大小, 修改时间)`，下次扫描据此跳过未变的。
//!
//! ## 三件必须记住的事
//!
//! 1. **键是「根名 + NFC 的相对路径」**（ADR-0020）。主库是**一组根**，键的第一段是
//!    根名（[`path::library_key`]）。读盘用系统给的原始路径，入库与比较用键，
//!    两者不能混用；从键回到盘走 [`roots::Roots`]。
//! 2. **不可读是第三态**（ADR-0021）。`readable = 0` 的记录既不算已变也不算已删，
//!    `len` 是 `NULL` 而不是 `0`——库里另有 4,317 个真正的空文件。
//! 3. **删除只在完整扫完一遍之后判**。[`Catalog::sweep`] 删的是「这次扫描没见到的」，
//!    中断的扫描绝不能调它，否则没扫到的那半个库会被当成已删除抹掉。

pub mod baseline;
pub mod browse;
pub mod content;
pub mod detail;
pub mod export;
mod filter;
pub mod frontend;
pub mod identify;
pub mod roots;
pub mod scrape;
pub mod sublibrary;
pub mod title;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::container::{ContainerKind, Contents, FailureReason, Penetration};
use crate::fs::{EntryKind, EntryMeta};
use crate::header::ProbeClass;
use crate::path;
use crate::platform::Manifest;
use crate::report::ReportMeta;
use crate::scan::aggregate::{
    Aggregate, ContainerFacts, FileObservation, InnerEntryContext, Limits, SampleResult,
};
use crate::shape;
use crate::workspace;

pub use baseline::{Baseline, Recorded, ScanDelta, Verdict};
pub use browse::{
    BrowseVariant, Facet, Facets, MAX_PAGE, PlatformFilter, Scope, SearchHit, StateFilter,
    VariantOrder, VariantQuery, WORK_FIELDS, WorkAnchor, WorkDetail, WorkOrder, WorkQuery, WorkRow,
    WorkVariant,
};
pub use content::{MemberFile, ReleaseRow, VariantRow};
pub use detail::{MediaHave, MediaItem, Sibling, ValueItem, VariantDetail};
pub use export::{ExportSetup, ExportSetupError};
pub use frontend::{SnapshotOrigin, SnapshotRow};
pub use identify::{
    AcceptedCandidate, Candidate, CandidateCounts, Confidence, ContentHash, EntryFact,
    Identification, Provenance, SourceCount, State,
};
pub use roots::{AddRootError, LibraryRoot, RootScan, RootStats, Roots};
pub use title::TitleRow;

/// 中立库的结构版本。**读到对不上的版本直接让用户删库重扫。**
///
/// ⚠️ **眼下是几，只认这段文档底下那个常量。** 下面从 `3` 一路数到 `7` 的是一部历史，
/// 从中间读起、读到一半就停的人拿到的是一个**陈年数**——本轮就有人据此把「不许升版」
/// 那条约束写成了「眼下是 4」（挂单 `Q366`）。
///
/// 3 是票 05 加的三层内容层级与合集（`catalog::content`）；**4** 是票 07 加的识别结论
/// （`catalog::identify`：候选、结论、算过的哈希，以及作品与发行版上那一列来路）。
///
/// ## 这条便宜路为什么到票 08 依然走得通（原挂账 D26）
///
/// 票 07 留下的判断是「删库重扫这条路到票 08 就走不通了——那时沉淀库里攒着裁决，
/// 重扫补不回来」。**这条判断成立，但它的结论不是「给中立库写迁移」，而是
/// 「别把裁决放进中立库」。** 票 08 因此把**沉淀库**做成一份单独的文件
/// （[`workspace::verdict_store_path`]），
/// 与 DAT 库、媒体池同构，理由也同源：一条**裁决**说的是「世上这份内容是什么」，
/// 与它躺在哪块盘上无关。
///
/// 于是这份库里**每一条都还是可再生的**：扫描、成型、识别、刮削全能重跑，
/// `origin = 裁决` 的那几行是沉淀库的投影，重放一遍就回来
/// （[`Catalog::clear_identifications`]）。删库重扫依旧只是「花点时间」，
/// 而不是「丢掉人一条条看出来的判断」。
///
/// **不可再生的东西再往这里放，这条就重新失效**——那时该做的还是先问一句
/// 「它是不是本来就该住在别处」。眼下还剩几样半可再生的（成型的人工纠正、子库的
/// 定义与**清单**、导入的前端快照），记在挂账 D97 上。
///
/// ## 什么算「结构变了」
///
/// 加 1 的判据是**旧数据会不会被读错**，不是「文件里多了点东西」。票 13 的刮削那五张表
/// （`catalog::scrape`）是**纯加表**：已有的表一列没动、一条语义没改，`CREATE TABLE IF
/// NOT EXISTS` 在打开时就把它们补上，旧库照样打得开，拿旧版程序再打开也照样能用。
/// 为它逼用户删掉 780 MB 的库、重扫 27 分钟、重跑 14 分钟识别，换不到任何东西
/// （挂账 D50）。**改了已有表的列或含义才加 1。** 票 15 的标题集合、票 16 的旁路快照、
/// 票 18 的子库三张表都是同一档。
///
/// **中文离线源那批票的 01 是头一个真正撞上这条判据的**：它给 `scrape_value` 的去重键
/// 加了 `value` 那一列——改的是已有表的键，旧库拿新程序打开会把「一个源的第二个值」
/// 当成冲突丢掉。按上面那条判据加 1，于是这个数是 5。**不为它写迁移代码**：这张表整份
/// 可再生，而中立库本来就是「结构版本一变就删库重扫」那一档——省下一整套迁移代码
/// 是这个设计当初就付过账的便宜买卖。
///
/// ## 6：主库变成**一组根**
///
/// 键的形状变了：从「相对主库根的路径」变成「**根名** + 相对那个根的路径」
/// （[`path::library_key`]）。这不是加一张表，是**每一条记录的主键都换了形状**——
/// 旧库拿新程序打开，`FC/魂斗罗.zip` 会被当成根名叫 `FC`、相对路径是 `魂斗罗.zip`
/// 的一条记录，平台从此认不出来。按上面那条判据这是最硬的一次「改了已有表的含义」。
///
/// **照旧不写迁移代码**，理由还是那一条：中立库整份可再生。删库重扫一遍 37.1 分钟，
/// 比一套只用一次的迁移代码便宜。
///
/// **沉淀库不在这条路上。** 它不可再生，走顺序迁移永不要求删库
/// （[`verdict::MIGRATIONS`](crate::verdict)）。这次换键的代价落在它身上的那一份是：
/// **路径锚**（`(主库名, 变体的键)`）里存的键是旧形状，从此撞不上——那些行原样留着，
/// 一条都不删，也不改。**内容锚一条都不受影响**，而那正是它存在的理由。
///
/// ## 7：**遍历也按根记**
///
/// 6 只把**条目**按根分开了，遍历那两张表还是「整份库一份」：`traversal` 每开一次新
/// 扫描就把别的行删光，`traversal_note` 更是整张清空。主库变成一组根之后这是错的——
/// 扫一遍乙盘会把甲盘那条「这个目录列不开」从报告里抹掉，而它的子树还被
/// [`Catalog::keep_subtree`] 保在库里（ADR-0021），报告与中立库从此各说各话。
///
/// 于是 `traversal_note` 多一列 `root_name`，主键从 `(kind, path)` 变成
/// `(root_name, kind, path)`：**改了已有表的键**，按上面那条判据加 1。
/// 照旧不写迁移代码。
pub const SCHEMA_VERSION: u32 = 7;

/// `prepare_cached` 那张表留几条。
///
/// **默认那 16 条不够用了**（票 `parking-3/09`）：按一批键取行的那几条 SQL 是照段长
/// 现拼的（`… IN (?1,?2,…)`），段长不同就是不同的语句，而摊开**一个变体**时段长就是
/// 它有几个成员——真库上从 1 到几十都有。缓存一挤爆就次次重新解析，
/// 那正是 `Catalog::variant` 当年改走 `prepare_cached` 要省下的那笔钱（挂单 `Q119`）。
///
/// 一条准备好的语句是几 KiB 量级，128 条的代价可以忽略。
const STATEMENT_CACHE: usize = 128;

/// 一条 `IN (…)` 查询一次问多少个键。
///
/// 把要问的键切成这么长的段，一段一条 `IN`。逐个键往返一次的话，真库上
/// 「全选 46,483 行 → ★ 收藏」要为每个变体各问好几次库（变体行、成员、容器构成、
/// 容器状态、条目、算过的哈希），那是二十几万次往返（票 `parking-3/09`）。
///
/// **它不是全仓库唯一的一份**：`catalog::scrape` 里另有一个同名的模块私有常量
/// （900），连占位符也自己拼了一套。两处该合成一处，记在挂单 `Q283`——那一处属于
/// 刮削那一族的下推查询，本票没伸手。
///
/// **一个键那一档是它的特例**，不是另一条路：`chunks` 交出一段长度为 1 的段，
/// 折出来的 SQL 是 `… IN (?1)`，与从前那句 `… = ?1` 走同一个索引、同样进
/// `prepare_cached`。于是「一条」与「一批」共用同一句 SQL——**哪几列算一行**
/// 这种事写两遍，迟早有一处漏掉新加的那一列。
///
/// 取 500 是给 SQLite 的绑定参数上限留余量：这个仓库用的 `bundled` 是新版（上限
/// 32,766），但那条上限历史上是 999，而 500 段与 32,000 段的往返次数在这几张表上
/// 已经差不到一个量级。
pub(crate) const KEYS_PER_QUERY: usize = 500;

/// `?1, ?2, …, ?n`：一条 `IN (…)` 里那一串占位符。
pub(crate) fn placeholders(count: usize) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(count * 5);
    for at in 1..=count {
        if at > 1 {
            out.push(',');
        }
        // 写进 `String` 不会失败。
        let _ = write!(out, "?{at}");
    }
    out
}

/// 元数据表里记**这份主库叫什么**的那个键。
///
/// 加这一个键是**纯加**：已有的表一列没动、一条语义没改，于是
/// [`SCHEMA_VERSION`] 一动不动（判据见它的文档）。旧库拿新程序打开照样能用，
/// 只是读不到这一行——那时走 [`Catalog::library_name`] 的退路。
const META_LIBRARY_NAME: &str = "library_name";

const SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS meta(
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

-- 一个条目一行，文件、目录、符号链接都在里面。键是「**根名** + `/` + 相对那个根、
-- 分隔符统一成 `/`、再规范化成 NFC 的路径」（ADR-0020、`path::library_key`）；
-- **一个根自己的键就是它的名字**。
--
-- 根名进键里，是因为主库是**一组根**：几块盘扫进同一份中立库，只按相对路径当键的话
-- 两块盘上同名的 `FC/魂斗罗.zip` 会静默覆盖成一条，而中立库是事实来源（ADR-0001）。
--
-- 目录也存，是为了让「扫过几个目录」这类计数从表里数出来而不是攒在内存里：
-- 攒着的计数在中断续跑时会重复累加，数出来的不会。
CREATE TABLE IF NOT EXISTS entry(
    key      TEXT PRIMARY KEY,
    kind     INTEGER NOT NULL,
    readable INTEGER NOT NULL,
    len      INTEGER,
    mtime_ns INTEGER,
    non_utf8 INTEGER NOT NULL,
    sample   TEXT,
    seen     INTEGER NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS entry_kind ON entry(kind);

-- 这次走了一遍的元信息。计数不在这里——那些从 entry 与 traversal_note 数出来。
CREATE TABLE IF NOT EXISTS traversal(
    scan              INTEGER PRIMARY KEY,
    -- 这一趟扫的是哪个**根**：名字与它当时挂在哪。一趟只扫一个根。
    root_name         TEXT    NOT NULL,
    root              TEXT    NOT NULL,
    elapsed_ms        INTEGER NOT NULL,
    jobs              INTEGER NOT NULL,
    samples_per_class INTEGER NOT NULL,
    containers        INTEGER NOT NULL,
    interrupted       INTEGER NOT NULL,
    resumed           INTEGER NOT NULL
) STRICT;

-- 遍历路上的事件：读不到的目录、整棵跳过的系统目录。
-- 主键是 (根名, 类别, 路径) 而不是自增 id，于是续跑时重扫同一个目录只会覆盖，不会数两遍。
--
-- **根名也在主键里**，是因为开一次新扫描要清掉的只是**这一个根**上一趟留下的批注：
-- 一趟扫描只走一个根，把整张表清空等于让扫甲盘的报告说不出乙盘那个目录列不开，
-- 而那棵子树还好端端地保在 `entry` 里（ADR-0021）。
CREATE TABLE IF NOT EXISTS traversal_note(
    root_name TEXT NOT NULL,
    kind      TEXT NOT NULL,
    path      TEXT NOT NULL,
    detail    TEXT,
    PRIMARY KEY (root_name, kind, path)
) STRICT;

-- 穿透一个**透明容器**的结论，一个容器一行。`reason` 非空就是没穿透：那一列是分好类的
-- 短码（报告要数得出「多少个要密码」），`detail` 是给人看的那一句
-- （ADR-0021 那条「如实记录，不猜」的道理，粒度在容器上）。
CREATE TABLE IF NOT EXISTS container(
    key    TEXT PRIMARY KEY,
    kind   TEXT    NOT NULL,
    reason TEXT,
    detail TEXT,
    files  INTEGER NOT NULL,
    bytes  INTEGER NOT NULL,
    blocks INTEGER NOT NULL,
    solid  INTEGER NOT NULL,
    no_crc INTEGER NOT NULL
) STRICT;

-- 容器里的内部文件。落库是为了**第二次扫描不必再穿一遍**：容器的三元组没变，
-- 这些行原样留着（调研第 5 部分 L4）。
--
-- 主键带 `ordinal` 而不只是内部路径：zip 允许同名条目重复出现，拿路径当键会
-- 悄悄丢掉其中几条。
CREATE TABLE IF NOT EXISTS container_entry(
    key     TEXT    NOT NULL,
    ordinal INTEGER NOT NULL,
    inner   TEXT    NOT NULL,
    size    INTEGER NOT NULL,
    crc32   INTEGER,
    block   INTEGER,
    is_dir  INTEGER NOT NULL,
    lossy   INTEGER NOT NULL,
    PRIMARY KEY (key, ordinal)
) STRICT;

-- **按内容判据反查**（票 05）：一条**匹配裁决**钉在内容锚（CRC-32 加大小）上，而刮削
-- 那一侧手里只有变体的键——两头要接得上，就得答得出「本机哪个变体装着这份内容」。
-- 没有这条索引就是一次全表扫描，真库里 216,203 条内部条目。
CREATE INDEX IF NOT EXISTS container_entry_print ON container_entry(crc32, size);
";

const NOTE_ERROR: &str = "error";
const NOTE_SKIPPED: &str = "skipped";

const KIND_FILE: i64 = 0;
const KIND_DIR: i64 = 1;
const KIND_SYMLINK: i64 = 2;
const KIND_OTHER: i64 = 3;

fn kind_code(kind: EntryKind) -> i64 {
    match kind {
        EntryKind::File => KIND_FILE,
        EntryKind::Dir => KIND_DIR,
        EntryKind::Symlink => KIND_SYMLINK,
        EntryKind::Other => KIND_OTHER,
    }
}

/// 中立库读写过程中的错误。
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    /// 中立库所在的目录建不出来。
    #[error("中立库的目录建不出来：{path}（{source}）")]
    Io {
        /// 出问题的路径。
        path: String,
        /// 底层错误。
        source: std::io::Error,
    },
    /// 底层读写失败。
    #[error("中立库读写失败：{path}（{source}）")]
    Sqlite {
        /// 中立库文件。
        path: String,
        /// 底层错误。
        source: rusqlite::Error,
    },
    /// 结构版本对不上。
    #[error("中立库 {path} 的结构版本是 {found}，本程序认得的是 {expected}。删掉它重扫一遍即可")]
    Version {
        /// 中立库文件。
        path: String,
        /// 文件里的版本。
        found: u32,
        /// 本程序的版本。
        expected: u32,
    },
    /// 这份库只活在内存里，分不出第二份连接。
    #[error(
        "{path} 这份中立库只活在内存里，分不出第二份连接——\
         后台跑的活要的是一份落在磁盘上的库"
    )]
    NotOnDisk {
        /// 这份库自称在哪儿。
        path: String,
    },
    /// 存进去的抽样结果读不回来。
    #[error("中立库 {path} 里 {key} 的抽样结果读不回来：{source}")]
    Corrupt {
        /// 中立库文件。
        path: String,
        /// 出问题的记录。
        key: String,
        /// 底层错误。
        source: serde_json::Error,
    },
}

/// 现在是 UNIX 纪元起的第几秒。
///
/// 一处定死：快照、刮削、子库三处都往库里记时刻，各写一遍的话「取不到时钟怎么办」
/// 这个岔路口就有三个不一样的答案。
pub(crate) fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_secs()).ok())
        .unwrap_or(0)
}

/// 把 [`SystemTime`] 折成 UNIX 纪元起的纳秒。
///
/// 纳秒是够用的：NTFS 的刻度是 100 纳秒、APFS 是 1 纳秒，两者都能逐位存下
/// （`docs/research/ntfs-mtime-precision.md`）。`i64` 覆盖 1678–2262 年，
/// 超出范围返回 `None`，于是那条记录被保守地判为已变而不是被悄悄截断。
#[must_use]
pub fn mtime_ns(time: SystemTime) -> Option<i64> {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_nanos()).ok(),
        Err(before) => i64::try_from(before.duration().as_nanos())
            .ok()
            .and_then(i64::checked_neg),
    }
}

/// 一次遍历的元信息。
///
/// 计数（目录数、链接数、跳过的系统目录、读不到的目录）**不在这里**——它们从
/// `entry` 与 `traversal_note` 里数出来。攒在内存里的计数在中断续跑时会重复累加，
/// 数出来的不会。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Traversal {
    /// 扫描代号。
    pub scan: i64,
    /// 这一趟扫的是哪个**根**。它是那个根下面所有键的第一段。
    pub root_name: String,
    /// 那个根的展示形态。挂载点会变，因此它跟着每次扫描更新。
    pub root: String,
    /// 累计耗时，含此前几次续跑。
    pub elapsed_ms: u64,
    /// 并发线程数。
    pub jobs: usize,
    /// 每类文件的抽样配额。
    pub samples_per_class: usize,
    /// 这次扫描有没有穿透**透明容器**。
    ///
    /// 报告要说得出这一条：没穿透时「内部文件 0 个」是因为没去看，不是因为容器是空的。
    pub penetrated_containers: bool,
    /// 这次扫描是否被中断。
    pub interrupted: bool,
    /// 这次扫描是否从断点续跑。
    pub resumed: bool,
}

/// 一条待写进中立库的记录。
#[derive(Debug, Clone)]
pub struct EntryRecord {
    /// 中立库的键（相对根、NFC）。
    pub key: String,
    /// 是文件、目录还是链接。
    pub kind: EntryKind,
    /// 这次看到的元数据。
    pub meta: EntryMeta,
    /// 原始路径能否无损表示成 UTF-8。键是有损转换的结果，这个标记是它补不回来的那半条。
    pub non_utf8: bool,
    /// 这次的判断。
    pub verdict: Verdict,
    /// 这次抽到的头部样本；`None` 表示这次没抽。
    pub sample: Option<(ProbeClass, SampleResult)>,
    /// 这次穿透**透明容器**的结论；`None` 表示这次没穿（不是容器，或者关掉了穿透）。
    pub container: Option<Penetration>,
}

/// 中立库。
#[derive(Debug)]
pub struct Catalog {
    conn: Connection,
    file: Option<PathBuf>,
    path: String,
}

impl Catalog {
    /// 打开（必要时新建）一个落在磁盘上的中立库。
    ///
    /// **建出来的库不知道自己叫什么**，读它的名字只能从文件名截
    /// （[`Self::library_name`]）。手上有 [`workspace::Slug`] 的调用方走
    /// [`Self::open_named`]，那一条会在**建库那一趟**把原名记下。
    ///
    /// # Errors
    /// 建目录、打开文件、建表或版本对不上时返回错误。
    pub fn open(path: &Path) -> Result<Self, CatalogError> {
        Self::open_with(path, None)
    }

    /// 同上，外加**建库时**记下这份主库叫什么。
    ///
    /// `name` 是**原名**——人起的那个名字，或者没起名字时主库根的末级目录名
    /// （[`workspace::Slug::display_name`]）。它一个字符都不折：
    /// 名字里带 `/`、带控制字符、长过 24 个字符时，落进元数据表的是原名，
    /// **文件名照旧按 [`workspace::Slug::text`] 那套折**——两条路
    /// 互不干扰。
    ///
    /// **只在建库那一趟写。** 库已经在那儿了就一个字不改：那一行是「这份库是谁建的、
    /// 当时叫什么」，不是「这次是拿哪个名字打开的」。于是票 01 之前建的那些库
    /// 一辈子读不到这一行，走 [`Self::library_name`] 的退路——那正是它存在的理由。
    ///
    /// # Errors
    /// 同 [`Self::open`]。
    pub fn open_named(path: &Path, name: &str) -> Result<Self, CatalogError> {
        Self::open_with(path, Some(name))
    }

    fn open_with(path: &Path, name: Option<&str>) -> Result<Self, CatalogError> {
        let display = path::display(path);
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|source| CatalogError::Io {
                path: crate::path::display(parent),
                source,
            })?;
        }
        let conn = Connection::open(path).map_err(|source| CatalogError::Sqlite {
            path: display.clone(),
            source,
        })?;
        Self::prepare(conn, Some(path.to_path_buf()), display, name)
    }

    /// 为**另一条线程**再开一份同一份中立库，**只读**。
    ///
    /// ## 为什么这不会长出「两份库不一致」
    ///
    /// 挂账 D156 当时对「为界面再开一份连接」的顾虑正是这句话。这份连接把它拆掉，
    /// 靠的是三条构造上的事实，不是纪律：
    ///
    /// 1. **它写不动。** 连接带 `SQLITE_OPEN_READ_ONLY` 开出来，往它上面写一个字节都会
    ///    被 SQLite 当场拒绝。**全程只有一个写者**，那还是原来那份连接。
    /// 2. **它不建表、不改版本。** 建表与版本那一套只在 [`Catalog::open`] 里做一次；
    ///    这一份只核对版本对不对，对不上就不开。
    /// 3. **它活得比一趟活还短。** 一趟长活开一份、跑完就丢，不是一份放在那儿慢慢变旧的
    ///    缓存。WAL 让它在这段时间里读到一份一致的快照——另一条线程同时在写也不打架。
    ///
    /// 这份连接对着的是**同一个文件**（路径从这份库自己身上取，调用方无从指错）。
    ///
    /// # Errors
    /// 这份库只活在内存里（分不出第二份连接）、文件打不开、或者结构版本对不上时返回错误。
    pub fn read_only(&self) -> Result<Self, CatalogError> {
        let file = self.file.clone().ok_or_else(|| CatalogError::NotOnDisk {
            path: self.path.clone(),
        })?;
        Self::read_only_at(file, self.path.clone())
    }

    /// 只读地打开磁盘上**已经在那儿**的一份中立库。
    ///
    /// 与 [`Self::open`] 差在两处，而这两处正是**列举**要的
    /// （[`workspace::catalogs`]）：
    ///
    /// - **不建库。** 文件不在就报错，不会凭空建一份空的。
    /// - **不建表、不补列、不写版本。** [`Self::open`] 那条路会给每一份老库补上新表、
    ///   补上新列——**只是想看看这个工作目录里有哪些库**，不该改动其中任何一份。
    ///   版本对不上时它照旧开不出来（[`CatalogError::Version`] 带着两个版本号），
    ///   而那份库的内容一个字节都没被动过。
    ///
    /// 开出来的那一份写不动（`SQLITE_OPEN_READ_ONLY`），理由与 [`Self::read_only`]
    /// 同源。
    ///
    /// # Errors
    /// 文件不在、打不开、或者结构版本对不上时返回错误。
    pub fn open_read_only(file: &Path) -> Result<Self, CatalogError> {
        Self::read_only_at(file.to_path_buf(), path::display(file))
    }

    fn read_only_at(file: PathBuf, path: String) -> Result<Self, CatalogError> {
        let flags =
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn =
            Connection::open_with_flags(&file, flags).map_err(|source| CatalogError::Sqlite {
                path: path.clone(),
                source,
            })?;
        let twin = Self {
            conn,
            file: Some(file),
            path,
        };
        twin.conn
            .set_prepared_statement_cache_capacity(STATEMENT_CACHE);
        // **这一份也要等。** WAL 让读与写并行，但写者提交那一刻仍会短暂独占；
        // 默认超时是 0，于是长活那一侧会在扫描提交的那一瞬间拿到一句
        // 「database is locked」而不是等一会儿（同 `Catalog::open` 那条注释）。
        twin.batch("PRAGMA busy_timeout = 10000;")?;
        // **只核对，不建、不改。** 版本对不上时开出来的是一份读得出行、却对不上号的库，
        // 那比打不开更坏。
        let found: Option<String> = twin
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|source| twin.err(source))?;
        match found.as_deref().map(str::parse::<u32>) {
            Some(Ok(version)) if version == SCHEMA_VERSION => Ok(twin),
            found => Err(CatalogError::Version {
                path: twin.path.clone(),
                found: found.and_then(Result::ok).unwrap_or(0),
                expected: SCHEMA_VERSION,
            }),
        }
    }

    /// 开一个只活在内存里的中立库。测试用，也用于「只想看看不想留痕」。
    ///
    /// # Errors
    /// 建表失败时返回错误。
    pub fn open_in_memory() -> Result<Self, CatalogError> {
        let conn = Connection::open_in_memory().map_err(|source| CatalogError::Sqlite {
            path: "（内存）".to_string(),
            source,
        })?;
        Self::prepare(conn, None, "（内存）".to_string(), None)
    }

    fn prepare(
        conn: Connection,
        file: Option<PathBuf>,
        path: String,
        name: Option<&str>,
    ) -> Result<Self, CatalogError> {
        let catalog = Self { conn, file, path };
        catalog
            .conn
            .set_prepared_statement_cache_capacity(STATEMENT_CACHE);
        // WAL：中断的扫描已经写进去的部分不会因为没提交而整份丢掉。
        // **`busy_timeout` 不是调优，是界面那一屏的前提**：扫描跑在画帧线程之外，
        // 后台那条线程按文件路径自己开一份写得动的库（`gui::roots::Screen::scan`），
        // 于是同一个文件上会有两个写者。WAL 允许一写多读，但两个写者撞上时默认是
        // **当场返回 `SQLITE_BUSY`** ——那会让用户在界面上按一下裁决就报一句
        // 「数据库忙」。等一会儿是对的：扫描一批写完就放手。
        catalog.batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 10000;",
        )?;
        catalog.batch(SCHEMA)?;
        catalog.batch(roots::ROOTS_SCHEMA)?;
        catalog.batch(content::CONTENT_SCHEMA)?;
        catalog.batch(identify::IDENTIFY_SCHEMA)?;
        catalog.batch(scrape::SCRAPE_SCHEMA)?;
        catalog.batch(title::TITLE_SCHEMA)?;
        catalog.batch(frontend::FRONTEND_SCHEMA)?;
        catalog.batch(sublibrary::SUBLIBRARY_SCHEMA)?;
        // 建完表再补列：票 18、19 建的那两张表在老库里已经存在，
        // `CREATE TABLE IF NOT EXISTS` 对它们一个字都不改（见 `add_columns`）。
        sublibrary::add_columns(&catalog.conn).map_err(|source| catalog.err(source))?;
        identify::add_columns(&catalog.conn).map_err(|source| catalog.err(source))?;
        let found: Option<String> = catalog
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|source| catalog.err(source))?;
        match found.as_deref().map(str::parse::<u32>) {
            None => {
                catalog
                    .conn
                    .execute(
                        "INSERT INTO meta(key, value) VALUES('schema_version', ?1)",
                        params![SCHEMA_VERSION.to_string()],
                    )
                    .map_err(|source| catalog.err(source))?;
                // **「结构版本这一行还不在」就是「这份库是这一趟建出来的」。**
                // 名字只在这一趟落，理由见 `Catalog::open_named`。
                // **空白不是名字**：`--library ""` 命令行不拦（那是它自己的行为），
                // 落一行空串下去，开场那一屏就多一行没有名字的库。宁可不写，走退路。
                if let Some(name) = name.filter(|name| !name.trim().is_empty()) {
                    catalog.meta_set(META_LIBRARY_NAME, name)?;
                }
            }
            Some(Ok(version)) if version == SCHEMA_VERSION => {}
            Some(found) => {
                return Err(CatalogError::Version {
                    path: catalog.path.clone(),
                    found: found.unwrap_or(0),
                    expected: SCHEMA_VERSION,
                });
            }
        }
        Ok(catalog)
    }

    fn err(&self, source: rusqlite::Error) -> CatalogError {
        CatalogError::Sqlite {
            path: self.path.clone(),
            source,
        }
    }

    fn batch(&self, sql: &str) -> Result<(), CatalogError> {
        self.conn
            .execute_batch(sql)
            .map_err(|source| self.err(source))
    }

    /// 中立库文件的位置，报告里说得出来。
    #[must_use]
    pub fn location(&self) -> &str {
        &self.path
    }

    /// 中立库落在磁盘上的哪个文件；只活在内存里时是 `None`。
    ///
    /// 扫描器拿它来守只读边界：中立库绝不许落进主库（ADR-0004、ADR-0009）。
    #[must_use]
    pub fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }

    /// 这份主库叫什么——说得出口的那个名字。
    ///
    /// 先读元数据表里那一行（建库时落的原名，见 [`Self::open_named`]）；**读不到就退回
    /// 从中立库的文件名截**，把哈希后缀剥掉只留人认得出的那一半
    /// （[`workspace::readable_half`]）。
    ///
    /// **它不报错、不中断，一定交得出一句话。** 退路要顶的有三种库：票 01 之前建的
    /// （那一行压根没写过）、拿 [`Self::open`] 建的（不知道自己叫什么）、以及库本身
    /// 读不动的。而开场那一屏的判据是**照列不误**——从列表里静静消失才是最难查的那种错
    /// （ADR-0023），一份库说不出名字不该让整屏失败。
    ///
    /// 只活在内存里的那份没有文件名可截，交出的是它那个占位路径。空白也算读不到。
    ///
    /// ## 它**不是** [`Site::library`](crate::site::Site::library)
    ///
    /// 那一个是 [`workspace::Slug::text`] 折出来的那串「可读的一半 + 哈希」，是中立库的
    /// 主文件名，也是**路径锚**里记的那个键——换一个字，命令行裁的界面就看不见了。
    /// 这一个是**给人看的**，进不了任何键，也没人拿它去找文件。两样都叫「主库名」，
    /// 但只有那一个是标识符。
    #[must_use]
    pub fn library_name(&self) -> String {
        if let Ok(Some(name)) = self.meta_get(META_LIBRARY_NAME)
            && !name.trim().is_empty()
        {
            return name;
        }
        self.file.as_deref().and_then(Path::file_stem).map_or_else(
            || self.path.clone(),
            |stem| workspace::readable_half(&stem.to_string_lossy()).to_string(),
        )
    }

    fn meta_get(&self, key: &str) -> Result<Option<String>, CatalogError> {
        self.conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|source| self.err(source))
    }

    fn meta_set(&self, key: &str, value: &str) -> Result<(), CatalogError> {
        self.conn
            .execute(
                "INSERT INTO meta(key, value) VALUES(?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map_err(|source| self.err(source))?;
        Ok(())
    }

    /// 一批**透明容器**的内部构成，零解压层当初落库的那一份。
    ///
    /// **格式转换在差量预览阶段就得知道「转出来多大、转出来叫什么」**，而这两样正好
    /// 都在容器头里（ADR-0014：内部文件名与未压缩大小零解压可得，扫描那一趟已经读进
    /// 中立库了，调研第 5 部分 L4）。于是排计划这一步**一个字节都不必解压**，
    /// 外置盘不在位照样排得出。
    ///
    /// 穿不透的容器（`container.reason` 非空）**不在返回值里**：内部构成读不出来，
    /// 于是它转不了，由调用方判成「吃不下且转不了」如实报出来——而不是当成一个空容器。
    ///
    /// 与 [`variant_files`](Self::variant_files) 同一条取数纪律：一趟顺读、在内存里筛。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    /// 名字里**还有替换字符**的那些容器，按键排序。
    ///
    /// 票 03 对非 UTF-8 的内部名字按有损转换处理（那时名字不参与命中）。票 11 起名字
    /// 参与匹配了，而有损转换不可逆——`U+FFFD` 已经把原字节吃掉了。这张名单是
    /// **补救的入口**（`scan::names::recheck`）：只把这些容器的中央目录重读一遍，
    /// 几 KB 一个，不必为它重扫整个主库。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn containers_with_lossy_names(&self) -> Result<Vec<String>, CatalogError> {
        let mut statement = self
            .conn
            .prepare("SELECT DISTINCT key FROM container_entry WHERE lossy = 1 ORDER BY key")
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|source| self.err(source))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|source| self.err(source))?);
        }
        Ok(out)
    }

    /// **一个容器**的内部条目，按序号排好。
    ///
    /// 它与 [`container_contents`](Self::container_contents) 是两件事：那一个一趟把
    /// **全库**的容器构成折出来（成型要的就是全库），这一个只问一个键。名字重解那一趟
    /// 要按容器一个个问——拿全库那一个去问 7,177 次，等于把一百万行的表扫 7,177 遍。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn container_entries(
        &self,
        key: &str,
    ) -> Result<Vec<crate::container::InnerEntry>, CatalogError> {
        let mut statement = self
            .conn
            .prepare_cached(
                "SELECT inner, size, crc32, block, is_dir, lossy FROM container_entry
                 WHERE key = ?1 ORDER BY ordinal",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![key], |row| {
                Ok(crate::container::InnerEntry {
                    path: row.get(0)?,
                    size: u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                    crc32: row
                        .get::<_, Option<i64>>(2)?
                        .and_then(|raw| u32::try_from(raw).ok()),
                    block: row
                        .get::<_, Option<i64>>(3)?
                        .and_then(|raw| usize::try_from(raw).ok()),
                    is_dir: row.get::<_, i64>(4)? != 0,
                    name_lossy: row.get::<_, i64>(5)? != 0,
                })
            })
            .map_err(|source| self.err(source))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|source| self.err(source))?);
        }
        Ok(out)
    }

    /// 只改一个容器里那些内部条目的**名字**，别的一个字不动。
    ///
    /// **按 `ordinal` 对位**：重读的是同一个文件（大小与修改时间都没变，不然扫描早就
    /// 把它整条换掉了），条目的顺序因此与当初落库时一模一样。条数对不上就一条都不改并
    /// 返回 `None`——那说明这个文件确实变了，该走的是**扫描**那条路，不是这条补救路。
    ///
    /// 名字之外一个字节都不碰：大小、CRC-32、块号、是不是目录全留着。那些是识别的判据，
    /// 而这一趟的全部目的只是把名字解对。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn rename_container_entries(
        &mut self,
        key: &str,
        entries: &[crate::container::InnerEntry],
    ) -> Result<Option<u64>, CatalogError> {
        let stored = self.container_entries(key)?;
        if stored.is_empty() || stored.len() != entries.len() {
            return Ok(None);
        }
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        let mut changed = 0u64;
        {
            let mut update = tx
                .prepare(
                    "UPDATE container_entry SET inner = ?3, lossy = ?4
                      WHERE key = ?1 AND ordinal = ?2",
                )
                .map_err(to_err)?;
            for (ordinal, (was, now)) in stored.iter().zip(entries).enumerate() {
                if was.path == now.path && was.name_lossy == now.name_lossy {
                    continue;
                }
                update
                    .execute(params![
                        key,
                        i64::try_from(ordinal).unwrap_or(i64::MAX),
                        now.path,
                        i64::from(now.name_lossy),
                    ])
                    .map_err(to_err)?;
                changed += 1;
            }
        }
        tx.commit().map_err(to_err)?;
        Ok(Some(changed))
    }

    /// 一批**透明容器**的内部构成，零解压层当初落库的那一份。
    ///
    /// **格式转换在差量预览阶段就得知道「转出来多大、转出来叫什么」**，而这两样正好
    /// 都在容器头里（ADR-0014：内部文件名与未压缩大小零解压可得，扫描那一趟已经读进
    /// 中立库了，调研第 5 部分 L4）。于是排计划这一步**一个字节都不必解压**，
    /// 外置盘不在位照样排得出。
    ///
    /// 穿不透的容器（`container.reason` 非空）**不在返回值里**：内部构成读不出来，
    /// 于是它转不了，由调用方判成「吃不下且转不了」如实报出来——而不是当成一个空容器。
    ///
    /// 与 [`variant_files`](Self::variant_files) 同一条取数纪律：一趟顺读、在内存里筛。
    /// 只问**一个**容器时走 [`container_entries`](Self::container_entries)——
    /// 这一个每次都把整张表扫一遍。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn container_contents(
        &self,
        keys: &std::collections::BTreeSet<String>,
    ) -> Result<BTreeMap<String, Contents>, CatalogError> {
        let mut penetrated: BTreeSet<String> = BTreeSet::new();
        let mut statement = self
            .conn
            .prepare("SELECT key FROM container WHERE reason IS NULL")
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let key: String = row.get(0).map_err(|source| self.err(source))?;
            if keys.contains(&key) {
                penetrated.insert(key);
            }
        }

        let mut out: BTreeMap<String, Contents> = BTreeMap::new();
        let mut statement = self
            .conn
            .prepare(
                "SELECT key, ordinal, inner, size, crc32, block, is_dir, lossy
                 FROM container_entry ORDER BY key, ordinal",
            )
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let key: String = row.get(0).map_err(|source| self.err(source))?;
            if !penetrated.contains(&key) {
                continue;
            }
            let size: i64 = row.get(3).map_err(|source| self.err(source))?;
            let crc32: Option<i64> = row.get(4).map_err(|source| self.err(source))?;
            let block: Option<i64> = row.get(5).map_err(|source| self.err(source))?;
            let is_dir: i64 = row.get(6).map_err(|source| self.err(source))?;
            let lossy: i64 = row.get(7).map_err(|source| self.err(source))?;
            let contents = out.entry(key).or_default();
            contents.entries.push(crate::container::InnerEntry {
                path: row.get(2).map_err(|source| self.err(source))?,
                size: u64::try_from(size).unwrap_or(0),
                crc32: crc32.and_then(|raw| u32::try_from(raw).ok()),
                is_dir: is_dir != 0,
                block: block.and_then(|raw| usize::try_from(raw).ok()),
                name_lossy: lossy != 0,
            });
        }
        for contents in out.values_mut() {
            // 块数是条目里出现过的不同块号有几个。存的时候没单独记一列，数出来即可
            // ——`Contents::is_solid` 与调度只看这个数。
            let mut blocks: Vec<usize> = contents.entries.iter().filter_map(|e| e.block).collect();
            blocks.sort_unstable();
            blocks.dedup();
            contents.blocks = blocks.len();
        }
        Ok(out)
    }

    /// 一个根**顶层**条目的名字，至多 `limit` 条，按名字排序。返回的是**相对那个根**
    /// 的那一段，不带根名。
    ///
    /// 顶层名就是那个根下面那一层的名字。它是「这个根还是不是原来那块盘」最便宜的判据：
    /// 一次 `read_dir` 就能拿实际的那一份来比，不必碰盘上的第二层。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn top_level_keys(
        &self,
        root_name: &str,
        limit: usize,
    ) -> Result<Vec<String>, CatalogError> {
        let prefix = format!("{root_name}/");
        let mut statement = self
            .conn
            .prepare(
                "SELECT substr(key, length(?1) + 1) FROM entry
                 WHERE substr(key, 1, length(?1)) = ?1
                   AND instr(substr(key, length(?1) + 1), '/') = 0
                 ORDER BY key LIMIT ?2",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(
                params![prefix, i64::try_from(limit).unwrap_or(i64::MAX)],
                |row| row.get::<_, String>(0),
            )
            .map_err(|source| self.err(source))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|source| self.err(source))?);
        }
        Ok(out)
    }

    /// 上一次遍历留下的记录；从没扫过时是 `None`。
    ///
    /// **一份中立库里每个根各留一行**（[`Catalog::begin_scan`]），所以这里给出的是
    /// **最后走的那个根**那一趟。报告的抬头眼下取的就是它，而正文数的是整份库——
    /// 那条账挂在后续清单上，要的是报告结构上的改动，不是这里多一句 `ORDER BY`。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn last_traversal(&self) -> Result<Option<Traversal>, CatalogError> {
        self.traversal_row(None)
    }

    /// **这一个根**最后一趟遍历的记录；这个根从没扫过时是 `None`。
    ///
    /// 它是断点身份的判据：断点说的那一趟，得就是这个根在中立库里最后走的那一趟
    /// （[`scan::load_start_state`](crate::scan)）。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn last_traversal_of(&self, root_name: &str) -> Result<Option<Traversal>, CatalogError> {
        self.traversal_row(Some(root_name))
    }

    /// `root_name` 是 `None` 就取整份库最后那一行，给了名字就只在那个根的行里取。
    fn traversal_row(&self, root_name: Option<&str>) -> Result<Option<Traversal>, CatalogError> {
        let read = |row: &rusqlite::Row<'_>| {
            Ok(Traversal {
                scan: row.get(0)?,
                root_name: row.get(1)?,
                root: row.get(2)?,
                elapsed_ms: u64::try_from(row.get::<_, i64>(3)?).unwrap_or(0),
                jobs: usize::try_from(row.get::<_, i64>(4)?).unwrap_or(1),
                samples_per_class: usize::try_from(row.get::<_, i64>(5)?).unwrap_or(0),
                penetrated_containers: row.get::<_, i64>(6)? != 0,
                interrupted: row.get::<_, i64>(7)? != 0,
                resumed: row.get::<_, i64>(8)? != 0,
            })
        };
        match root_name {
            Some(name) => self.conn.query_row(
                "SELECT scan, root_name, root, elapsed_ms, jobs, samples_per_class, containers,
                        interrupted, resumed
                 FROM traversal WHERE root_name = ?1 ORDER BY scan DESC LIMIT 1",
                params![name],
                read,
            ),
            None => self.conn.query_row(
                "SELECT scan, root_name, root, elapsed_ms, jobs, samples_per_class, containers,
                        interrupted, resumed
                 FROM traversal ORDER BY scan DESC LIMIT 1",
                [],
                read,
            ),
        }
        .optional()
        .map_err(|source| self.err(source))
    }

    /// 库里记了多少个文件。目录与链接不算——`entry` 表里三种都有。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn file_count(&self) -> Result<u64, CatalogError> {
        self.count("SELECT COUNT(*) FROM entry WHERE kind = ?1", KIND_FILE)
    }

    fn count(&self, sql: &str, kind: i64) -> Result<u64, CatalogError> {
        let count: i64 = self
            .conn
            .query_row(sql, params![kind], |row| row.get(0))
            .map_err(|source| self.err(source))?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    /// 库里一个文件都没有。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn is_empty(&self) -> Result<bool, CatalogError> {
        Ok(self.file_count()? == 0)
    }

    /// 库里有没有这条键。键是 NFC 的相对路径（ADR-0020）。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn contains(&self, key: &str) -> Result<bool, CatalogError> {
        self.conn
            .query_row("SELECT 1 FROM entry WHERE key = ?1", params![key], |_| {
                Ok(())
            })
            .optional()
            .map(|found| found.is_some())
            .map_err(|source| self.err(source))
    }

    /// 把全库的三元组读进内存，供工作线程做增量判断。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn baseline(&self) -> Result<Baseline, CatalogError> {
        let mut baseline = Baseline::empty();
        let mut statement = self
            .conn
            .prepare("SELECT key, readable, len, mtime_ns FROM entry")
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let key: String = row.get(0).map_err(|source| self.err(source))?;
            let readable: i64 = row.get(1).map_err(|source| self.err(source))?;
            let len: Option<i64> = row.get(2).map_err(|source| self.err(source))?;
            let mtime_ns: Option<i64> = row.get(3).map_err(|source| self.err(source))?;
            let recorded = match (readable, len) {
                (0, _) | (_, None) => Recorded::Unreadable,
                (_, Some(len)) => Recorded::Known {
                    len: u64::try_from(len).unwrap_or(0),
                    mtime_ns,
                },
            };
            baseline.insert(key, recorded);
        }
        // 哪些容器**穿透成功过**。没成功过的即便三元组没变也要再试一遍：
        // 一来上次可能是 `--no-containers` 扫的，压根没试；二来上次的失败可能是
        // 一次性的（库里 1.59% 的文件在 fskit 下连元数据都读不到，那是会变的），
        // 把失败当成定论会让它永远不再被试。
        let mut statement = self
            .conn
            .prepare("SELECT key FROM container WHERE reason IS NULL")
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            baseline.insert_penetrated(row.get(0).map_err(|source| self.err(source))?);
        }
        Ok(baseline)
    }

    fn decode_sample(
        &self,
        key: &str,
        raw: Option<&str>,
    ) -> Result<Option<(ProbeClass, SampleResult)>, CatalogError> {
        raw.map(|text| {
            serde_json::from_str(text).map_err(|source| CatalogError::Corrupt {
                path: self.path.clone(),
                key: key.to_string(),
                source,
            })
        })
        .transpose()
    }

    /// 这次扫描的代号：比库里记过的最大代号大 1。
    ///
    /// **代号全局单调递增，不按根分。** `entry.seen` 是全局一列，存的是「最后一次见到
    /// 这条记录的那一趟的代号」；代号按根算的话甲盘的第二趟与乙盘的第二趟共用数字 2，
    /// 那一列从此再也说不出「上一次是谁见到的」，任何一处忘了划根名范围的查询都会
    /// 静默混淆两个根。全局递增下「`seen` 不等于这一趟的代号」在任何范围里都只有一个
    /// 意思：这一趟没见到它——[`Catalog::sweep`] 靠的正是这句话。
    ///
    /// 遍历行按根各留最后一条（[`Catalog::begin_scan`]），于是这里的 `MAX` 天然就是
    /// 全局最大值。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn next_scan(&self) -> Result<i64, CatalogError> {
        let last: i64 = self
            .conn
            .query_row("SELECT COALESCE(MAX(scan), 0) FROM traversal", [], |row| {
                row.get(0)
            })
            .map_err(|source| self.err(source))?;
        Ok(last + 1)
    }

    /// 开一次新扫描：清掉**这个根**上一趟的遍历记录与批注。
    ///
    /// **只清这一个根，不是优化是正确性。** 一趟扫描只走一个根（`CONTEXT.md` 的**根**），
    /// 别的根那几行说的是别的盘上次走了一遍留下的账：清掉它们，报告的抬头与耗时就只剩
    /// 最后一根的，而正文数的是整份中立库；连同批注一起清掉，扫一遍乙盘还会让甲盘那条
    /// 「这个目录列不开」从报告里消失——而那棵子树还被 [`Catalog::keep_subtree`]
    /// 保在 `entry` 里（ADR-0021）。
    ///
    /// **不动 `entry`**——那正是增量要比对的基线，清了就等于每次都全扫。
    ///
    /// **中断续跑不调它**：续跑沿用同一个代号，这一趟的行就是断点那一趟的行。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn begin_scan(&mut self, scan: i64, root_name: &str) -> Result<(), CatalogError> {
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        tx.execute(
            "DELETE FROM traversal WHERE root_name = ?1 AND scan <> ?2",
            params![root_name, scan],
        )
        .and_then(|_| {
            tx.execute(
                "DELETE FROM traversal_note WHERE root_name = ?1",
                params![root_name],
            )
        })
        .map_err(to_err)?;
        tx.commit().map_err(to_err)
    }

    /// 写一批记录。
    ///
    /// **「文件变了」是一整套作废**，不只是把 `entry` 那一行改掉：上一趟算出来的哈希、
    /// 光盘 / 卡带 / Switch 的内部事实、容器的内部构成，以及挂在这个条目所属**变体**上的
    /// 识别结论，全都是**按那份字节**得出来的，字节换了就一条都不算数
    /// （`identify::drop_stale_conclusions` 上写着为什么变体那一层也落在这里）。
    ///
    /// # Errors
    /// 写库或序列化抽样结果失败时返回错误。
    pub fn write(&mut self, scan: i64, records: &[EntryRecord]) -> Result<(), CatalogError> {
        if records.is_empty() {
            return Ok(());
        }
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        // 这一批里字节变了的那些键。攒起来一次问完，是因为一个变体常有好几个成员
        // （三块 `.bin` 一个变体），逐条去问等于把同一个变体的结论删上三遍。
        let mut changed_keys: BTreeSet<String> = BTreeSet::new();
        {
            // 未变的只更新「这次见过」，其余字段一律不碰——上次抽的头部样本要留着。
            let mut touch = tx
                .prepare("UPDATE entry SET seen = ?1 WHERE key = ?2")
                .map_err(to_err)?;
            // 读不到元数据时：库里没有就记一条「不可读」，已经有了就只更新「这次见过」。
            // 绝不用「不可读」覆盖上次在 Windows 上读到的真实大小与时间（ADR-0021）。
            let mut keep = tx
                .prepare(
                    "INSERT INTO entry(key, kind, readable, len, mtime_ns, non_utf8, sample, seen)
                     VALUES(?1, ?2, 0, NULL, NULL, ?3, NULL, ?4)
                     ON CONFLICT(key) DO UPDATE SET seen = excluded.seen",
                )
                .map_err(to_err)?;
            let mut upsert = tx
                .prepare(
                    "INSERT INTO entry(key, kind, readable, len, mtime_ns, non_utf8, sample, seen)
                     VALUES(?1, ?2, 1, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT(key) DO UPDATE SET
                        kind     = excluded.kind,
                        readable = excluded.readable,
                        len      = excluded.len,
                        mtime_ns = excluded.mtime_ns,
                        non_utf8 = excluded.non_utf8,
                        sample   = excluded.sample,
                        seen     = excluded.seen",
                )
                .map_err(to_err)?;
            // 文件变了，上一趟算出来的哈希就作废了——留着它，识别会拿一份对不上的
            // CRC-32 去撞 DAT，撞出来的候选还带着「精确命中」的置信度。
            // 与容器内部构成的作废方式是同一条（挂账 D14）。
            let mut clear_hashes = tx
                .prepare("DELETE FROM content_hash WHERE key = ?1")
                .map_err(to_err)?;
            // 光盘标识同理（票 09）。那张表每一行自带有效期，所以这一句不是它唯一的
            // 依靠，只是把过期的行**当场**清掉而不是留到下一趟识别去发现。
            let mut clear_disc = tx
                .prepare("DELETE FROM content_disc WHERE key = ?1")
                .map_err(to_err)?;
            // 卡带内部头同理（票 10）。
            let mut clear_cart = tx
                .prepare("DELETE FROM content_cart WHERE key = ?1")
                .map_err(to_err)?;
            // Switch 容器的明文文件名表同理（票 27）。**它一度漏在这份清单外**，于是
            // 换掉一份 `.nsp` 之后，库体检的「Switch 的内容分布」照着旧容器的
            // TitleID 与本体 / 补丁 / 附属内容分档数，数的是一份已经不在盘上的东西。
            let mut clear_switch = tx
                .prepare("DELETE FROM content_switch WHERE key = ?1")
                .map_err(to_err)?;
            // 媒体文件变了，上一趟算出来的内容哈希同样作废——留着它，刮削会拿一个
            // 对不上的哈希去引用**媒体池**里另一份内容的图。与 content_hash 同一条路。
            let mut clear_media = tx
                .prepare("DELETE FROM media_blob WHERE key = ?1")
                .map_err(to_err)?;
            // 容器的内部构成同理，只是它多一半：这次穿透了就得**先清后插**。
            // 容器变了而这次没穿透（比如关掉了穿透），旧的内部条目照样该消失——
            // 拿一份对不上的清单冒充新的，比没有清单更糟。
            let mut clear_container = tx
                .prepare("DELETE FROM container WHERE key = ?1")
                .map_err(to_err)?;
            let mut clear_inner = tx
                .prepare("DELETE FROM container_entry WHERE key = ?1")
                .map_err(to_err)?;
            // **裸 `INSERT`，不加 `ON CONFLICT`。** 每一条插入之前那两句 `DELETE` 一定
            // 跑过（见下面那个分支），所以撞上主键只可能是这条不变式破了，
            // 那时该当场炸而不是被一句 `DO UPDATE` 抹平——把撞车咽下去，
            // 换来的是一份说不清是哪一趟穿出来的内部构成。
            let mut insert_container = tx
                .prepare(
                    "INSERT INTO container(key, kind, reason, detail, files, bytes, blocks,
                         solid, no_crc)
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                )
                .map_err(to_err)?;
            let mut insert_inner = tx
                .prepare(
                    "INSERT INTO container_entry(key, ordinal, inner, size, crc32,
                         block, is_dir, lossy)
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )
                .map_err(to_err)?;
            for record in records {
                match record.verdict {
                    Verdict::Unchanged => {
                        touch.execute(params![scan, record.key]).map_err(to_err)?;
                    }
                    Verdict::Unreadable => {
                        keep.execute(params![
                            record.key,
                            kind_code(record.kind),
                            i64::from(record.non_utf8),
                            scan
                        ])
                        .map_err(to_err)?;
                    }
                    Verdict::Added | Verdict::Changed => {
                        let sample = record
                            .sample
                            .as_ref()
                            .map(serde_json::to_string)
                            .transpose()
                            .map_err(|source| CatalogError::Corrupt {
                                path: path.clone(),
                                key: record.key.clone(),
                                source,
                            })?;
                        let len = record
                            .meta
                            .byte_len()
                            .and_then(|len| i64::try_from(len).ok());
                        let mtime = record.meta.modified().and_then(mtime_ns);
                        upsert
                            .execute(params![
                                record.key,
                                kind_code(record.kind),
                                len,
                                mtime,
                                i64::from(record.non_utf8),
                                sample,
                                scan
                            ])
                            .map_err(to_err)?;
                    }
                }

                let changed = matches!(record.verdict, Verdict::Added | Verdict::Changed);
                if changed {
                    clear_hashes.execute(params![record.key]).map_err(to_err)?;
                    clear_disc.execute(params![record.key]).map_err(to_err)?;
                    clear_cart.execute(params![record.key]).map_err(to_err)?;
                    clear_switch.execute(params![record.key]).map_err(to_err)?;
                    clear_media.execute(params![record.key]).map_err(to_err)?;
                    changed_keys.insert(record.key.clone());
                }
                // **「是不是容器」这个判断不从键上推。** 键里非 UTF-8 的那一段缀着一段
                // 指纹（[`path::catalog_key`]），而它缀在扩展名**之后**——
                // `Path::extension` 读出来是 `zip#0123…`，于是同一个文件在扫描那一侧
                // 是容器、在这一侧不是，「先清后插」整个跳过，第二趟插入撞上主键，
                // 一份读不出内部构成的容器让整趟扫描失败。
                //
                // 这一句压根不问那个问题：不是容器的键，这两句 `DELETE` 本来就删不到
                // 任何一行（键是主键，一次索引落空），代价与另外五张内容表同一档。
                if changed || record.container.is_some() {
                    clear_inner.execute(params![record.key]).map_err(to_err)?;
                    clear_container
                        .execute(params![record.key])
                        .map_err(to_err)?;
                }
                if let Some(penetration) = &record.container {
                    let contents = &penetration.contents;
                    insert_container
                        .execute(params![
                            record.key,
                            penetration.kind.code(),
                            penetration.failure.as_ref().map(|f| f.reason.code()),
                            penetration.failure.as_ref().map(|f| f.detail.as_str()),
                            i64::try_from(contents.file_count()).unwrap_or(i64::MAX),
                            i64::try_from(contents.total_size()).unwrap_or(i64::MAX),
                            i64::try_from(contents.blocks).unwrap_or(i64::MAX),
                            i64::from(contents.is_solid()),
                            i64::try_from(contents.without_crc()).unwrap_or(i64::MAX),
                        ])
                        .map_err(to_err)?;
                    for (ordinal, inner) in contents.entries.iter().enumerate() {
                        insert_inner
                            .execute(params![
                                record.key,
                                i64::try_from(ordinal).unwrap_or(i64::MAX),
                                inner.path,
                                i64::try_from(inner.size).unwrap_or(i64::MAX),
                                inner.crc32.map(i64::from),
                                inner.block.map(|b| i64::try_from(b).unwrap_or(i64::MAX)),
                                i64::from(inner.is_dir),
                                i64::from(inner.name_lossy),
                            ])
                            .map_err(to_err)?;
                    }
                }
            }
        }
        // 字节变了的那些条目，挂在它们所属**变体**上的识别结论也跟着作废。
        // **在同一个事务里**：条目变了与它的结论跟着走必须一起落盘，分两次提交
        // 中间被打断，就正好留下一份「新字节配旧结论」的库。
        identify::drop_stale_conclusions(&tx, &changed_keys).map_err(to_err)?;
        tx.commit().map_err(to_err)
    }

    /// 记一条「这个目录读不到」。`root_name` 是这一趟扫的那个根——批注跟着根走，
    /// 下一趟扫别的根不许把它清掉。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn note_error(
        &mut self,
        root_name: &str,
        path: &str,
        detail: &str,
    ) -> Result<(), CatalogError> {
        self.note(root_name, NOTE_ERROR, path, Some(detail))
    }

    /// 记一条「这棵系统目录整棵跳过了」。跳过什么都要说出来，不能悄悄少扫。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn note_skipped_dir(&mut self, root_name: &str, path: &str) -> Result<(), CatalogError> {
        self.note(root_name, NOTE_SKIPPED, path, None)
    }

    fn note(
        &mut self,
        root_name: &str,
        kind: &str,
        path: &str,
        detail: Option<&str>,
    ) -> Result<(), CatalogError> {
        self.conn
            .execute(
                "INSERT INTO traversal_note(root_name, kind, path, detail) VALUES(?1, ?2, ?3, ?4)
                 ON CONFLICT(root_name, kind, path) DO UPDATE SET detail = excluded.detail",
                params![root_name, kind, path, detail],
            )
            .map(|_| ())
            .map_err(|source| self.err(source))
    }

    /// 存下这次遍历的计数。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn save_traversal(&mut self, traversal: &Traversal) -> Result<(), CatalogError> {
        self.conn
            .execute(
                "INSERT INTO traversal(scan, root_name, root, elapsed_ms, jobs,
                     samples_per_class, containers, interrupted, resumed)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(scan) DO UPDATE SET
                     root_name = excluded.root_name, root = excluded.root,
                     elapsed_ms = excluded.elapsed_ms,
                     jobs = excluded.jobs, samples_per_class = excluded.samples_per_class,
                     containers = excluded.containers,
                     interrupted = excluded.interrupted, resumed = excluded.resumed",
                params![
                    traversal.scan,
                    traversal.root_name,
                    traversal.root,
                    i64::try_from(traversal.elapsed_ms).unwrap_or(i64::MAX),
                    i64::try_from(traversal.jobs).unwrap_or(i64::MAX),
                    i64::try_from(traversal.samples_per_class).unwrap_or(i64::MAX),
                    i64::from(traversal.penetrated_containers),
                    i64::from(traversal.interrupted),
                    i64::from(traversal.resumed),
                ],
            )
            .map(|_| ())
            .map_err(|source| self.err(source))
    }

    /// 把一棵子树整个标记成「这次见过」，返回标了几条。
    ///
    /// 列不开的目录要用它。目录列不出来时，它下面的记录这一趟一条也不会被写到，
    /// 而收尾时 [`Catalog::sweep`] 删的正是「这次没见到的」——**看不见不等于不存在**，
    /// 不标一下的话，一次拒绝访问就会让整棵子树从中立库里消失。这与 ADR-0021 对
    /// 单个文件的要求是同一条道理，只是粒度在目录上。
    ///
    /// `dir_key` 是空串时标记全库：主库根都列不开，这次扫描什么都没看见。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn keep_subtree(&mut self, scan: i64, dir_key: &str) -> Result<u64, CatalogError> {
        let prefix = if dir_key.is_empty() {
            String::new()
        } else {
            format!("{dir_key}/")
        };
        let kept = self
            .conn
            .execute(
                "UPDATE entry SET seen = ?1
                 WHERE key = ?2 OR substr(key, 1, length(?3)) = ?3",
                params![scan, dir_key, prefix],
            )
            .map_err(|source| self.err(source))?;
        Ok(kept as u64)
    }

    /// 删掉**这个根下面**这次扫描没见到的文件记录，返回删了几个文件。
    ///
    /// **只有完整扫完一遍才能调**。中断的扫描没走完整个库，没见到不等于不存在——
    /// 那时候调它会把还没扫到的那半个库当成已删除抹掉。
    ///
    /// **只收这一个根**，这不是优化是正确性：一趟扫描只走一个根，别的根这一趟一条都
    /// 没见到——不划范围的话，扫一遍甲盘会把乙盘那几万条整批抹掉。
    ///
    /// 读不到元数据的文件不会被扫到这里：它们的名字 `readdir` 列得出来，因此
    /// 「这次见过」照样会更新（ADR-0021）。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn sweep(&mut self, scan: i64, root_name: &str) -> Result<u64, CatalogError> {
        let prefix = format!("{root_name}/");
        let removed: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM entry
                 WHERE seen <> ?1 AND kind = ?2
                   AND (key = ?3 OR substr(key, 1, length(?4)) = ?4)",
                params![scan, KIND_FILE, root_name, prefix],
                |row| row.get(0),
            )
            .map_err(|source| self.err(source))?;
        let removed = u64::try_from(removed).unwrap_or(0);
        self.conn
            .execute(
                "DELETE FROM entry
                 WHERE seen <> ?1 AND (key = ?2 OR substr(key, 1, length(?3)) = ?3)",
                params![scan, root_name, prefix],
            )
            .map_err(|source| self.err(source))?;
        self.drop_orphans()?;
        Ok(removed)
    }

    /// 条目没了，挂在它身上的那几张表也就没了——留着会让报告数出一批不存在的内部文件。
    ///
    /// **这里收的是挂在条目（文件）上的那一层。** 挂在**变体**上的那一层（候选、结论、
    /// 标题）不在这儿收：变体不是文件，它由成型算出来，删一个文件不等于删一个变体
    /// ——三块 `.bin` 少了一块，那个变体还在。那一层由
    /// [`Catalog::replace_variants`](crate::catalog::Catalog::replace_variants) 在
    /// 重新成型之后收，那是变体表唯一的写入口。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub(crate) fn drop_orphans(&self) -> Result<(), CatalogError> {
        self.batch(
            "DELETE FROM container_entry WHERE key NOT IN (SELECT key FROM entry);
             DELETE FROM container       WHERE key NOT IN (SELECT key FROM entry);
             DELETE FROM content_hash    WHERE key NOT IN (SELECT key FROM entry);
             DELETE FROM content_disc    WHERE key NOT IN (SELECT key FROM entry);
             DELETE FROM content_cart    WHERE key NOT IN (SELECT key FROM entry);
             DELETE FROM content_switch  WHERE key NOT IN (SELECT key FROM entry);
             DELETE FROM media_blob      WHERE key NOT IN (SELECT key FROM entry);",
        )
    }

    /// 从库里的记录折出一份库体检的统计。**不碰磁盘**，外置盘不在位时照样出得来。
    ///
    /// # Errors
    /// 读库失败、或存进去的抽样结果读不回来时返回错误。
    pub fn aggregate(
        &self,
        limits: &Limits,
        manifest: &Manifest,
    ) -> Result<Aggregate, CatalogError> {
        let traversal = self.last_traversal()?.unwrap_or_default();
        // 展示路径要按**每条键自己的根**去拼：一份中立库里装着几个根，拿上一趟那个根
        // 的路径去接别的根的键，印出来的是一条盘上根本不存在的路径。
        let roots = Roots::load(self)?;
        let mut aggregate = Aggregate::default();

        let mut statement = self
            .conn
            // 按键排序，于是例子列表与「先看到谁」无关：同一份中立库出的报告永远一样，
            // 而扫描时哪个线程先跑完是不确定的。
            // 左连变体成员表：报告要答得出「范围之内哪些文件根本没进变体」——
            // 那个数按归类拆开之后，「未归类」那一栏就是成型的缺口。
            .prepare(
                "SELECT e.key, e.readable, e.len, e.non_utf8, e.sample, m.role
                 FROM entry e LEFT JOIN variant_member m ON m.key = e.key
                 WHERE e.kind = ?1 ORDER BY e.key",
            )
            .map_err(|source| self.err(source))?;
        let mut rows = statement
            .query(params![KIND_FILE])
            .map_err(|source| self.err(source))?;
        // 库里各有几个容器，按格式分。它是「还没读过几个」的被减数（见下）。
        let mut in_library: BTreeMap<ContainerKind, u64> = BTreeMap::new();
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let key: String = row.get(0).map_err(|source| self.err(source))?;
            if let Some(kind) = ContainerKind::for_path(Path::new(&key)) {
                *in_library.entry(kind).or_default() += 1;
            }
            let readable: i64 = row.get(1).map_err(|source| self.err(source))?;
            let len: Option<i64> = row.get(2).map_err(|source| self.err(source))?;
            let non_utf8: i64 = row.get(3).map_err(|source| self.err(source))?;
            let sample: Option<String> = row.get(4).map_err(|source| self.err(source))?;
            let sample = self.decode_sample(&key, sample.as_deref())?;
            let role: Option<String> = row.get(5).map_err(|source| self.err(source))?;
            let role = role.as_deref().and_then(shape::Role::from_code);
            // 大小以 `readable` 为准而不是「`len` 是不是 NULL」：两者现在等价，
            // 但把判据挂在那一列上，将来改结构也不会悄悄换掉「大小未知」的定义。
            let len = if readable == 0 {
                None
            } else {
                len.map(|len| u64::try_from(len).unwrap_or(0))
            };
            aggregate.record_file(
                &FileObservation::derive(manifest, &roots, &key, len, non_utf8 != 0, sample, role),
                limits,
            );
        }

        // 容器的穿透结论与内部构成一样是从库里折出来的：盘不在位时报告照样说得出
        // 「容器里装着什么」，这正是零解压层落库的意义（调研第 5 部分 L4）。
        let mut statement = self
            .conn
            .prepare(
                "SELECT key, kind, reason, detail, files, bytes, blocks, solid, no_crc
                 FROM container ORDER BY key",
            )
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let key: String = row.get(0).map_err(|source| self.err(source))?;
            let kind: String = row.get(1).map_err(|source| self.err(source))?;
            let reason: Option<String> = row.get(2).map_err(|source| self.err(source))?;
            let detail: Option<String> = row.get(3).map_err(|source| self.err(source))?;
            let Some(kind) = ContainerKind::from_code(&kind) else {
                continue;
            };
            let display = roots.display_key(&key);
            if let Some(reason) = reason.as_deref() {
                // 短码认不出来只可能是库被人改过；当成「结构读不下去」而不是悄悄丢掉这一条。
                let reason = FailureReason::from_code(reason).unwrap_or(FailureReason::Malformed);
                aggregate.record_penetration_failure(
                    &display,
                    kind,
                    reason,
                    detail.as_deref().unwrap_or(""),
                    limits,
                );
                continue;
            }
            let count = |index: usize| -> Result<u64, CatalogError> {
                let raw: i64 = row.get(index).map_err(|source| self.err(source))?;
                Ok(u64::try_from(raw).unwrap_or(0))
            };
            aggregate.record_penetrated(
                kind,
                &ContainerFacts {
                    inner_files: count(4)?,
                    inner_bytes: count(5)?,
                    blocks: count(6)?,
                    solid: count(7)? != 0,
                    inner_without_crc: count(8)?,
                },
            );
        }

        // 库里有、`container` 表里没有的容器：**还没读过**。zst 默认不读（穿不透，
        // 要全量解压），带 `--no-containers` 扫过的那一趟同理。不数出来的话这批容器会从
        // 报告里整个消失——「有多少东西还没看过」正是维护者最该知道的那个数。
        //
        // **减出来而不是再查一遍**：上面那趟已经把每个容器数过（`in_library`），
        // 这趟又把每一行 `container` 数过，两者一减就是答案。为它单开一趟
        // `entry LEFT JOIN container` 等于在二十几万行上白走一遍。
        for (kind, seen) in in_library {
            let read = aggregate
                .containers
                .by_kind
                .get(&kind)
                .map_or(0, |acc| acc.containers);
            aggregate.record_containers_unread(kind, seen.saturating_sub(read));
        }

        // 按容器排序，于是同一个容器的内部条目连着来，容器那一侧的上下文只算一次。
        let mut statement = self
            .conn
            .prepare("SELECT key, inner, size, is_dir, lossy FROM container_entry ORDER BY key")
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        let mut current: Option<(String, String)> = None;
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let is_dir: i64 = row.get(3).map_err(|source| self.err(source))?;
            if is_dir != 0 {
                continue;
            }
            let key: String = row.get(0).map_err(|source| self.err(source))?;
            if current.as_ref().is_none_or(|(seen, _)| *seen != key) {
                let display = roots.display_key(&key);
                current = Some((key.clone(), display));
            }
            let (container_key, display) = current.as_ref().expect("刚填上");
            let inner: String = row.get(1).map_err(|source| self.err(source))?;
            let size: i64 = row.get(2).map_err(|source| self.err(source))?;
            let lossy: i64 = row.get(4).map_err(|source| self.err(source))?;
            aggregate.record_inner_entry(
                &InnerEntryContext {
                    manifest,
                    display_path: display,
                    scope: shape::scope_of(manifest, container_key),
                },
                &inner,
                u64::try_from(size).unwrap_or(0),
                lossy != 0,
                limits,
            );
        }

        // 这几个计数是数出来的而不是攒出来的：中断续跑时重扫一个目录不会让它们翻倍。
        let count = "SELECT COUNT(*) FROM entry WHERE kind = ?1";
        aggregate.dirs = self.count(count, KIND_DIR)?;
        aggregate.anomalies.symlinks = self.count(count, KIND_SYMLINK)?;
        aggregate.anomalies.other_entries = self.count(count, KIND_OTHER)?;
        aggregate.elapsed_ms = traversal.elapsed_ms;

        // 成型的结论也是从库里折出来的：盘不在位时报告照样说得出「库里有多少个变体」。
        aggregate.shaping = self.shaping(manifest)?;
        for (platform, counts) in &aggregate.shaping.by_platform {
            if let Some(acc) = aggregate.platforms.get_mut(platform) {
                acc.variants = counts.variants;
            }
        }

        let mut statement = self
            .conn
            .prepare("SELECT kind, path, detail FROM traversal_note ORDER BY kind, path, root_name")
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let kind: String = row.get(0).map_err(|source| self.err(source))?;
            let path: String = row.get(1).map_err(|source| self.err(source))?;
            let detail: Option<String> = row.get(2).map_err(|source| self.err(source))?;
            if kind == NOTE_SKIPPED {
                aggregate.record_skipped_system_dir(&path, limits);
            } else {
                aggregate.record_unlistable_dir(&path, &detail.unwrap_or_default(), limits);
            }
        }

        Ok(aggregate)
    }

    /// 出一份报告需要的、统计之外的信息。
    ///
    /// `delta` 是 `None`：这份报告直接从中立库折出来，没有扫过盘，也就没有「这次相对
    /// 上次变了什么」可说。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn report_meta(&self) -> Result<ReportMeta, CatalogError> {
        let traversal = self.last_traversal()?.unwrap_or_default();
        Ok(ReportMeta {
            root_name: traversal.root_name,
            root: traversal.root,
            scan: traversal.scan,
            interrupted: traversal.interrupted,
            resumed: traversal.resumed,
            jobs: traversal.jobs,
            samples_per_class: traversal.samples_per_class,
            penetrated_containers: traversal.penetrated_containers,
            delta: None,
        })
    }
}
