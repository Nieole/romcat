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

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::fs::{EntryKind, EntryMeta};
use crate::header::ProbeClass;
use crate::path;
use crate::report::ReportMeta;
use crate::scan::aggregate::{Aggregate, FileObservation, Limits, SampleResult};

pub use baseline::{Baseline, ScanDelta, Snapshot, Verdict};

/// 中立库的结构版本。结构变了就加 1；读到对不上的版本直接让用户删库重扫。
pub const SCHEMA_VERSION: u32 = 1;

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

    /// 上一次遍历留下的记录；从没扫过时是 `None`。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn last_traversal(&self) -> Result<Option<Traversal>, CatalogError> {
        self.conn
            .query_row(
                "SELECT scan, root, elapsed_ms, jobs, samples_per_class, interrupted, resumed
                 FROM traversal ORDER BY scan DESC LIMIT 1",
                [],
                |row| {
                    Ok(Traversal {
                        scan: row.get(0)?,
                        root: row.get(1)?,
                        elapsed_ms: u64::try_from(row.get::<_, i64>(2)?).unwrap_or(0),
                        jobs: usize::try_from(row.get::<_, i64>(3)?).unwrap_or(1),
                        samples_per_class: usize::try_from(row.get::<_, i64>(4)?).unwrap_or(0),
                        interrupted: row.get::<_, i64>(5)? != 0,
                        resumed: row.get::<_, i64>(6)? != 0,
                    })
                },
            )
            .optional()
            .map_err(|source| self.err(source))
    }

    /// 库里记了多少个文件。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn len(&self) -> Result<u64, CatalogError> {
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
        Ok(self.len()? == 0)
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
            .prepare("SELECT key, readable, len, mtime_ns, sample FROM entry")
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let key: String = row.get(0).map_err(|source| self.err(source))?;
            let readable: i64 = row.get(1).map_err(|source| self.err(source))?;
            let len: Option<i64> = row.get(2).map_err(|source| self.err(source))?;
            let mtime_ns: Option<i64> = row.get(3).map_err(|source| self.err(source))?;
            let sample: Option<String> = row.get(4).map_err(|source| self.err(source))?;
            let snapshot = match (readable, len) {
                (0, _) | (_, None) => Snapshot::Unreadable,
                (_, Some(len)) => Snapshot::Known {
                    len: u64::try_from(len).unwrap_or(0),
                    mtime_ns,
                },
            };
            if let Some((class, _)) = self.decode_sample(&key, sample.as_deref())? {
                baseline.count_sample(class);
            }
            baseline.insert(key, snapshot);
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
        let tx = self
            .conn
            .transaction()
            .map_err(|source| CatalogError::Sqlite {
                path: self.path.clone(),
                source,
            })?;
        tx.execute("DELETE FROM traversal WHERE scan <> ?1", params![scan])
            .and_then(|_| tx.execute("DELETE FROM traversal_note", []))
            .map_err(|source| CatalogError::Sqlite {
                path: self.path.clone(),
                source,
            })?;
        tx.commit().map_err(|source| CatalogError::Sqlite {
            path: self.path.clone(),
            source,
        })
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
                     interrupted, resumed)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(scan) DO UPDATE SET
                     root = excluded.root, elapsed_ms = excluded.elapsed_ms,
                     jobs = excluded.jobs, samples_per_class = excluded.samples_per_class,
                     interrupted = excluded.interrupted, resumed = excluded.resumed",
                params![
                    traversal.scan,
                    traversal.root,
                    i64::try_from(traversal.elapsed_ms).unwrap_or(i64::MAX),
                    i64::try_from(traversal.jobs).unwrap_or(i64::MAX),
                    i64::try_from(traversal.samples_per_class).unwrap_or(i64::MAX),
                    i64::from(traversal.interrupted),
                    i64::from(traversal.resumed),
                ],
            )
            .map(|_| ())
            .map_err(|source| self.err(source))
    }

    /// 删掉这次扫描没见到的记录，返回删了几条。
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
        Ok(removed)
    }

    /// 从库里的记录折出一份库体检的统计。**不碰磁盘**，外置盘不在位时照样出得来。
    ///
    /// # Errors
    /// 读库失败、或存进去的抽样结果读不回来时返回错误。
    pub fn aggregate(&self, limits: &Limits) -> Result<Aggregate, CatalogError> {
        let traversal = self.last_traversal()?.unwrap_or_default();
        let mut aggregate = Aggregate::default();

        let mut statement = self
            .conn
            // 按键排序，于是例子列表与「先看到谁」无关：同一份中立库出的报告永远一样，
            // 而扫描时哪个线程先跑完是不确定的。
            .prepare("SELECT key, len, non_utf8, sample FROM entry WHERE kind = ?1 ORDER BY key")
            .map_err(|source| self.err(source))?;
        let mut rows = statement
            .query(params![KIND_FILE])
            .map_err(|source| self.err(source))?;
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let key: String = row.get(0).map_err(|source| self.err(source))?;
            let len: Option<i64> = row.get(1).map_err(|source| self.err(source))?;
            let non_utf8: i64 = row.get(2).map_err(|source| self.err(source))?;
            let sample: Option<String> = row.get(3).map_err(|source| self.err(source))?;
            let sample = self.decode_sample(&key, sample.as_deref())?;
            aggregate.record_file(
                &FileObservation::derive(
                    &traversal.root,
                    &key,
                    len.map(|len| u64::try_from(len).unwrap_or(0)),
                    non_utf8 != 0,
                    sample,
                ),
                limits,
            );
        }

        // 这几个计数是数出来的而不是攒出来的：中断续跑时重扫一个目录不会让它们翻倍。
        let count = "SELECT COUNT(*) FROM entry WHERE kind = ?1";
        aggregate.dirs = self.count(count, KIND_DIR)?;
        aggregate.anomalies.symlinks = self.count(count, KIND_SYMLINK)?;
        aggregate.anomalies.other_entries = self.count(count, KIND_OTHER)?;
        aggregate.elapsed_ms = traversal.elapsed_ms;

        let mut statement = self
            .conn
            .prepare("SELECT kind, path, detail FROM traversal_note ORDER BY kind, path")
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let kind: String = row.get(0).map_err(|source| self.err(source))?;
            let path: String = row.get(1).map_err(|source| self.err(source))?;
            let detail: Option<String> = row.get(2).map_err(|source| self.err(source))?;
            let (count, list, text) = if kind == NOTE_SKIPPED {
                (
                    &mut aggregate.anomalies.skipped_system_dirs,
                    &mut aggregate.anomalies.skipped_system_dir_examples,
                    path,
                )
            } else {
                (
                    &mut aggregate.anomalies.errors,
                    &mut aggregate.anomalies.error_examples,
                    format!("{path} — {}", detail.unwrap_or_default()),
                )
            };
            *count += 1;
            if list.len() < limits.max_examples {
                list.push(text);
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
            interrupted: traversal.interrupted,
            resumed: traversal.resumed,
            jobs: traversal.jobs,
            samples_per_class: traversal.samples_per_class,
            delta: None,
        })
    }
}
