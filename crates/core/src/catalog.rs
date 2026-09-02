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
//! 1. **键是 NFC 的相对路径**（ADR-0020）。读盘用系统给的原始路径，入库与比较用
//!    [`path::catalog_key`] 折出来的键，两者不能混用。
//! 2. **不可读是第三态**（ADR-0021）。`readable = 0` 的记录既不算已变也不算已删，
//!    `len` 是 `NULL` 而不是 `0`——库里另有 4,317 个真正的空文件。
//! 3. **删除只在完整扫完一遍之后判**。[`Catalog::sweep`] 删的是「这次扫描没见到的」，
//!    中断的扫描绝不能调它，否则没扫到的那半个库会被当成已删除抹掉。

pub mod baseline;
pub mod content;
pub mod frontend;
pub mod identify;
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

pub use baseline::{Baseline, Recorded, ScanDelta, Verdict};
pub use content::{MemberFile, ReleaseRow, VariantRow};
pub use frontend::{SnapshotOrigin, SnapshotRow};
pub use identify::{
    AcceptedCandidate, Candidate, CandidateCounts, Confidence, ContentHash, EntryFact,
    Identification, Provenance, SourceCount, State,
};
pub use title::TitleRow;

/// 中立库的结构版本。**读到对不上的版本直接让用户删库重扫。**
///
/// 3 是票 05 加的三层内容层级与合集（`catalog::content`）；**4** 是票 07 加的识别结论
/// （`catalog::identify`：候选、结论、算过的哈希，以及作品与发行版上那一列来路）。
///
/// ## 这条便宜路为什么到票 08 依然走得通（原挂账 D26）
///
/// 票 07 留下的判断是「删库重扫这条路到票 08 就走不通了——那时沉淀库里攒着裁决，
/// 重扫补不回来」。**这条判断成立，但它的结论不是「给中立库写迁移」，而是
/// 「别把裁决放进中立库」。** 票 08 因此把**沉淀库**做成一份单独的文件
/// （[`workspace::verdict_store_path`](crate::workspace::verdict_store_path)），
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
/// 票 18 的子库三张表都是同一档，因此这个数一直停在 4。
pub const SCHEMA_VERSION: u32 = 4;

const SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS meta(
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

-- 一个条目一行，文件、目录、符号链接都在里面。键是相对主库根、分隔符统一成 `/`、
-- 再规范化成 NFC 的路径（ADR-0020）；主库根自己的键是空串。
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
    root              TEXT    NOT NULL,
    elapsed_ms        INTEGER NOT NULL,
    jobs              INTEGER NOT NULL,
    samples_per_class INTEGER NOT NULL,
    containers        INTEGER NOT NULL,
    interrupted       INTEGER NOT NULL,
    resumed           INTEGER NOT NULL
) STRICT;

-- 遍历路上的事件：读不到的目录、整棵跳过的系统目录。
-- 主键是 (类别, 路径) 而不是自增 id，于是续跑时重扫同一个目录只会覆盖，不会数两遍。
CREATE TABLE IF NOT EXISTS traversal_note(
    kind   TEXT NOT NULL,
    path   TEXT NOT NULL,
    detail TEXT,
    PRIMARY KEY (kind, path)
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
";

/// `meta` 里记主库根的那把键。
const META_LIBRARY_ROOT: &str = "library_root";

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
    /// 主库根的展示形态。挂载点会变，因此它跟着每次扫描更新。
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
    /// # Errors
    /// 建目录、打开文件、建表或版本对不上时返回错误。
    pub fn open(path: &Path) -> Result<Self, CatalogError> {
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
        Self::prepare(conn, Some(path.to_path_buf()), display)
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
        Self::prepare(conn, None, "（内存）".to_string())
    }

    fn prepare(
        conn: Connection,
        file: Option<PathBuf>,
        path: String,
    ) -> Result<Self, CatalogError> {
        let catalog = Self { conn, file, path };
        // WAL：中断的扫描已经写进去的部分不会因为没提交而整份丢掉。
        catalog.batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
        catalog.batch(SCHEMA)?;
        catalog.batch(content::CONTENT_SCHEMA)?;
        catalog.batch(identify::IDENTIFY_SCHEMA)?;
        catalog.batch(scrape::SCRAPE_SCHEMA)?;
        catalog.batch(title::TITLE_SCHEMA)?;
        catalog.batch(frontend::FRONTEND_SCHEMA)?;
        catalog.batch(sublibrary::SUBLIBRARY_SCHEMA)?;
        // 建完表再补列：票 18、19 建的那两张表在老库里已经存在，
        // `CREATE TABLE IF NOT EXISTS` 对它们一个字都不改（见 `add_columns`）。
        sublibrary::add_columns(&catalog.conn).map_err(|source| catalog.err(source))?;
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

    /// 这份中立库上次记的主库根在哪；从没记过时是 `None`。
    ///
    /// 它**不是**定位这份库的依据——`--library` 起了名字之后，定位跟名字走
    /// （[`crate::workspace::Slug`]）。它只用来回答一个问题：换了挂载点之后，
    /// 眼前这个根还是不是同一个主库。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn library_root(&self) -> Result<Option<String>, CatalogError> {
        self.meta_get(META_LIBRARY_ROOT)
    }

    /// 记下这份中立库对着哪个主库根。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn set_library_root(&self, root: &str) -> Result<(), CatalogError> {
        self.meta_set(META_LIBRARY_ROOT, root)
    }

    /// 抹掉「这份库对着哪个主库」这条记录，装成票 29 之前建的老库。
    #[cfg(test)]
    pub(crate) fn forget_library_root(&self) {
        self.conn
            .execute(
                "DELETE FROM meta WHERE key = ?1",
                params![META_LIBRARY_ROOT],
            )
            .expect("删得掉");
    }

    /// 顶层条目的键，至多 `limit` 条，按键排序。
    ///
    /// 顶层键就是主库根下面那一层的名字。它是「这还是不是同一个主库」最便宜的判据：
    /// 一次 `read_dir` 就能拿实际的那一份来比，不必碰盘上的第二层。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn top_level_keys(&self, limit: usize) -> Result<Vec<String>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT key FROM entry
                 WHERE key <> '' AND instr(key, '/') = 0
                 ORDER BY key LIMIT ?1",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![i64::try_from(limit).unwrap_or(i64::MAX)], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|source| self.err(source))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|source| self.err(source))?);
        }
        Ok(out)
    }

    /// 上一次遍历留下的记录；从没扫过时是 `None`。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn last_traversal(&self) -> Result<Option<Traversal>, CatalogError> {
        self.conn
            .query_row(
                "SELECT scan, root, elapsed_ms, jobs, samples_per_class, containers,
                        interrupted, resumed
                 FROM traversal ORDER BY scan DESC LIMIT 1",
                [],
                |row| {
                    Ok(Traversal {
                        scan: row.get(0)?,
                        root: row.get(1)?,
                        elapsed_ms: u64::try_from(row.get::<_, i64>(2)?).unwrap_or(0),
                        jobs: usize::try_from(row.get::<_, i64>(3)?).unwrap_or(1),
                        samples_per_class: usize::try_from(row.get::<_, i64>(4)?).unwrap_or(0),
                        penetrated_containers: row.get::<_, i64>(5)? != 0,
                        interrupted: row.get::<_, i64>(6)? != 0,
                        resumed: row.get::<_, i64>(7)? != 0,
                    })
                },
            )
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

    /// 开一次新扫描：清掉上一次的遍历记录与样例。
    ///
    /// **不动 `entry`**——那正是增量要比对的基线，清了就等于每次都全扫。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn begin_scan(&mut self, scan: i64) -> Result<(), CatalogError> {
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        tx.execute("DELETE FROM traversal WHERE scan <> ?1", params![scan])
            .and_then(|_| tx.execute("DELETE FROM traversal_note", []))
            .map_err(to_err)?;
        tx.commit().map_err(to_err)
    }

    /// 写一批记录。
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
            // 容器的内部构成随容器本身一起更新。**先清后插**：容器变了而这次没穿透
            // （比如关掉了穿透），旧的内部条目就该消失，不能拿一份对不上的清单
            // 冒充新的。
            // 文件变了，上一趟算出来的哈希就作废了——留着它，识别会拿一份对不上的
            // CRC-32 去撞 DAT，撞出来的候选还带着「精确命中」的置信度。
            // 与容器内部构成的作废方式是同一条（挂账 D14）。
            let mut clear_hashes = tx
                .prepare("DELETE FROM content_hash WHERE key = ?1")
                .map_err(to_err)?;
            // 媒体文件变了，上一趟算出来的内容哈希同样作废——留着它，刮削会拿一个
            // 对不上的哈希去引用**媒体池**里另一份内容的图。与 content_hash 同一条路。
            let mut clear_media = tx
                .prepare("DELETE FROM media_blob WHERE key = ?1")
                .map_err(to_err)?;
            let mut clear_container = tx
                .prepare("DELETE FROM container WHERE key = ?1")
                .map_err(to_err)?;
            let mut clear_inner = tx
                .prepare("DELETE FROM container_entry WHERE key = ?1")
                .map_err(to_err)?;
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

                // 容器的内部构成。**先清后插**，而且清这一步在
                // 「容器变了但这次没穿透」时也要做——留着一份对不上的旧清单，
                // 比没有清单更糟。
                let is_container = ContainerKind::for_path(Path::new(&record.key)).is_some();
                let changed = matches!(record.verdict, Verdict::Added | Verdict::Changed);
                if changed {
                    clear_hashes.execute(params![record.key]).map_err(to_err)?;
                    clear_media.execute(params![record.key]).map_err(to_err)?;
                }
                if is_container && (changed || record.container.is_some()) {
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
        tx.commit().map_err(to_err)
    }

    /// 记一条「这个目录读不到」。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn note_error(&mut self, path: &str, detail: &str) -> Result<(), CatalogError> {
        self.note(NOTE_ERROR, path, Some(detail))
    }

    /// 记一条「这棵系统目录整棵跳过了」。跳过什么都要说出来，不能悄悄少扫。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn note_skipped_dir(&mut self, path: &str) -> Result<(), CatalogError> {
        self.note(NOTE_SKIPPED, path, None)
    }

    fn note(&mut self, kind: &str, path: &str, detail: Option<&str>) -> Result<(), CatalogError> {
        self.conn
            .execute(
                "INSERT INTO traversal_note(kind, path, detail) VALUES(?1, ?2, ?3)
                 ON CONFLICT(kind, path) DO UPDATE SET detail = excluded.detail",
                params![kind, path, detail],
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
                "INSERT INTO traversal(scan, root, elapsed_ms, jobs, samples_per_class,
                     containers, interrupted, resumed)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(scan) DO UPDATE SET
                     root = excluded.root, elapsed_ms = excluded.elapsed_ms,
                     jobs = excluded.jobs, samples_per_class = excluded.samples_per_class,
                     containers = excluded.containers,
                     interrupted = excluded.interrupted, resumed = excluded.resumed",
                params![
                    traversal.scan,
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

    /// 删掉这次扫描没见到的**文件**记录，返回删了几个文件。
    ///
    /// **只有完整扫完一遍才能调**。中断的扫描没走完整个库，没见到不等于不存在——
    /// 那时候调它会把还没扫到的那半个库当成已删除抹掉。
    ///
    /// 读不到元数据的文件不会被扫到这里：它们的名字 `readdir` 列得出来，因此
    /// 「这次见过」照样会更新（ADR-0021）。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn sweep(&mut self, scan: i64) -> Result<u64, CatalogError> {
        let removed: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM entry WHERE seen <> ?1 AND kind = ?2",
                params![scan, KIND_FILE],
                |row| row.get(0),
            )
            .map_err(|source| self.err(source))?;
        let removed = u64::try_from(removed).unwrap_or(0);
        self.conn
            .execute("DELETE FROM entry WHERE seen <> ?1", params![scan])
            .map_err(|source| self.err(source))?;
        // 容器没了，它的内部构成也就没了——留着会让报告数出一批不存在的内部文件。
        self.batch(
            "DELETE FROM container_entry WHERE key NOT IN (SELECT key FROM entry);
             DELETE FROM container       WHERE key NOT IN (SELECT key FROM entry);
             DELETE FROM content_hash    WHERE key NOT IN (SELECT key FROM entry);
             DELETE FROM media_blob      WHERE key NOT IN (SELECT key FROM entry);",
        )?;
        Ok(removed)
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
                &FileObservation::derive(
                    manifest,
                    &traversal.root,
                    &key,
                    len,
                    non_utf8 != 0,
                    sample,
                    role,
                ),
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
            let display = path::display_key(&traversal.root, &key);
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
                let display = path::display_key(&traversal.root, &key);
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
            .prepare("SELECT kind, path, detail FROM traversal_note ORDER BY kind, path")
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
