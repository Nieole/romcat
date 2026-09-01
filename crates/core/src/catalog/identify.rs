//! 中立库里的**识别**结论：**候选**、**依据**、**置信度**，以及算过的哈希。
//!
//! ## 四张表各自回答一个问题
//!
//! - `candidate`：**候选**。一个变体可以有多条——同一份内容会同时撞上 No-Intro 的
//!   原版条目与 TOSEC 的汉化条目。每条带**置信度**与**依据**（命中了哪个数据库的
//!   哪条记录、匹配了哪个字段、按哪套哈希口径），没有依据的候选事后无法复核（ADR-0002）。
//! - `identification`：这个变体这一轮的结论——**命中 / 未命中 / 无判据 / 跳过**。
//!   报告因此和体检报告一样，是从中立库折出来的，不必重跑一遍识别。
//! - `content_hash`：算过的哈希留着。**这是增量在这张票上的兑现**（挂账 D14）：
//!   容器里的 CRC-32 零解压就有，而裸文件要整份读一遍——一块 8.60 TiB 的盘上，
//!   第二趟识别不该再读一遍。它由扫描按文件的三元组作废（`Catalog::write`），
//!   与容器内部构成的作废方式是同一条。
//! - 作品与发行版多出一列 `origin`：这一行是**识别**自己造的，还是**裁决**定下来的。
//!   没有这一列，重跑识别就分不清哪些该清掉——而清错了会把人工裁决的结论冲掉。
//!
//! ## 「没有发行版链接」为什么必须分得出来路
//!
//! `CONTEXT.md` 说：同人移植与 homebrew 直接挂在**作品**下，**没有发行版链接这件事
//! 本身就告诉识别管线不要拿它去撞 DAT**。这条判据只有在链接的来路分得清时才成立——
//! 识别自己刚挂上去的「作品有、发行版没有」（汉化版就是这样：认得出是哪个作品，
//! 认不出它基于哪一条发行版）不能反过来被下一轮当成 homebrew。因此重跑识别的第一件事
//! 是把**识别自己造的**那些行连同链接一起清掉，留下的才是裁决说的话。

use std::collections::BTreeMap;

use rusqlite::{OptionalExtension, params};

use super::{Catalog, CatalogError};
use crate::dat::Convention;
use crate::dat::chinese::ChineseMark;

/// 识别相关的表。
pub(super) const IDENTIFY_SCHEMA: &str = "\
-- 一条**候选**：识别为一个变体给出的一个可能的发行版，带**置信度**与**依据**。
-- 一个变体可以有多条——同一份内容会同时撞上 No-Intro 的原版与 TOSEC 的汉化条目。
CREATE TABLE IF NOT EXISTS candidate(
    id          INTEGER PRIMARY KEY,
    variant_key TEXT    NOT NULL,
    -- 是变体的哪个成员撞上的：容器的键，或者裸文件自己的键。
    member_key  TEXT    NOT NULL,
    -- 容器内部路径；裸文件是空串。**判据是 CRC-32 加大小，名字不参与命中**，
    -- 这一列只为事后复核说得出「是包里的哪一个」。
    inner       TEXT    NOT NULL,
    confidence  TEXT    NOT NULL,
    -- 自动通过没有。精确命中自动通过，不占用人工裁决的时间（ADR-0002）。
    accepted    INTEGER NOT NULL,
    source      TEXT    NOT NULL,
    dat         TEXT    NOT NULL,
    platform    TEXT    NOT NULL,
    game        TEXT    NOT NULL,
    rom         TEXT    NOT NULL,
    -- 撞上时用的是哪套哈希（含头 / 去头），以及那份 DAT 自己声明的口径。
    hashing     TEXT    NOT NULL,
    convention  TEXT    NOT NULL,
    -- 依据：匹配了哪个字段，写成给人看的一句。
    evidence    TEXT    NOT NULL,
    chinese     TEXT,
    serial      TEXT,
    release_id  INTEGER REFERENCES release(id)
) STRICT;

CREATE INDEX IF NOT EXISTS candidate_variant ON candidate(variant_key);

-- 一个变体这一轮识别的结论。**跳过与无判据不混进未命中**——混进去命中率就失真了。
CREATE TABLE IF NOT EXISTS identification(
    variant_key TEXT PRIMARY KEY,
    state       TEXT    NOT NULL,
    reason      TEXT,
    units       INTEGER NOT NULL,
    candidates  INTEGER NOT NULL,
    accepted    INTEGER NOT NULL,
    nkit        INTEGER NOT NULL,
    read_bytes  INTEGER NOT NULL
) STRICT;

-- 算过的哈希。**同一份内容的两套口径都在这里**：`crc32`/`size` 是含头（原样），
-- `bare_crc32`/`bare_size` 是去头之后；`header` 记剥掉的是哪种外挂头，
-- 为 NULL 且 `looked` 为真表示**看过了、没有外挂头**——那与「还没看过」是两回事。
CREATE TABLE IF NOT EXISTS content_hash(
    key        TEXT    NOT NULL,
    inner      TEXT    NOT NULL,
    size       INTEGER NOT NULL,
    crc32      INTEGER NOT NULL,
    looked     INTEGER NOT NULL,
    header     TEXT,
    bare_size  INTEGER,
    bare_crc32 INTEGER,
    nkit       INTEGER,
    PRIMARY KEY (key, inner)
) STRICT;
";

/// 一条候选的**置信度**（ADR-0002）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    /// **高置信**：精确哈希命中，自动通过，不进待确认队列。
    High,
    /// **中置信**：撞上了但有保留（DAT 没记大小、或者这份镜像是 NKit 处理过的），
    /// 通过但标记，等人裁决。
    Medium,
    /// **低置信**：文件名规则、模糊匹配与模型推断那几层的产物，一律进待确认队列。
    /// 这张票还产不出这一档，它先立在这里等票 11 与票 12。
    Low,
}

impl Confidence {
    /// 存进库、也打给用户的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::High => "高置信",
            Self::Medium => "中置信",
            Self::Low => "低置信",
        }
    }

    /// 从词认回来。
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "高置信" => Some(Self::High),
            "中置信" => Some(Self::Medium),
            "低置信" => Some(Self::Low),
            _ => None,
        }
    }
}

/// 一个变体这一轮识别的结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum State {
    /// 撞上了 DAT。
    Matched,
    /// 拿判据撞过了，DAT 里没有。
    Unmatched,
    /// **拿不到判据**：容器穿不透、压缩镜像这一层认不了、元数据读不到。
    /// 它不是「DAT 里没有」，混进未命中会把命中率说低。
    NoEvidence,
    /// **不该撞 DAT**：补丁与没有发行版链接的变体。混进未命中会把命中率说低。
    Skipped,
}

impl State {
    /// 存进库、也打给用户的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Matched => "命中",
            Self::Unmatched => "未命中",
            Self::NoEvidence => "无判据",
            Self::Skipped => "跳过",
        }
    }

    /// 从词认回来。
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "命中" => Some(Self::Matched),
            "未命中" => Some(Self::Unmatched),
            "无判据" => Some(Self::NoEvidence),
            "跳过" => Some(Self::Skipped),
            _ => None,
        }
    }
}

/// 这一行**作品**或**发行版**是谁造的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// 识别自己造的。重跑识别时整批清掉再造一遍。
    Identified,
    /// 人工**裁决**定下来的。识别绝不碰它。
    Verdict,
}

impl Origin {
    /// 存进库的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Identified => "识别",
            Self::Verdict => "裁决",
        }
    }
}

/// 一条候选，写库前的样子。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// 是哪个成员撞上的。
    pub member_key: String,
    /// 容器内部路径；裸文件是空串。
    pub inner: String,
    /// 置信度。
    pub confidence: Confidence,
    /// 自动通过没有。
    pub accepted: bool,
    /// 哪个数据源。
    pub source: String,
    /// 哪一份 DAT。
    pub dat: String,
    /// 那份 DAT 归哪个平台。
    pub platform: String,
    /// 条目名。
    pub game: String,
    /// 文件记录名。
    pub rom: String,
    /// 撞上时用的是哪套哈希（含头 / 去头）。
    pub hashing: Convention,
    /// 那份 DAT 自己声明的口径。
    pub convention: Convention,
    /// **依据**：给人看的那一句。
    pub evidence: String,
    /// 中文记号。
    pub chinese: Option<ChineseMark>,
    /// 序列号。
    pub serial: Option<String>,
    /// 这条候选建出来的发行版。
    pub release_id: Option<i64>,
}

/// 一个变体这一轮的结论，写库前的样子。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identification {
    /// 哪个变体。
    pub variant_key: String,
    /// 结论。
    pub state: State,
    /// 跳过或无判据的具体理由。
    pub reason: Option<String>,
    /// 拿了几份内容去撞。
    pub units: u64,
    /// 这个变体里有几份是 NKit 处理过的镜像。
    pub nkit: u64,
    /// 为它回盘读了多少字节。第二趟应当是 0——那就是增量兑现的样子。
    pub read_bytes: u64,
    /// 挂到哪个作品。
    pub work_id: Option<i64>,
    /// 基于哪个发行版。
    pub release_id: Option<i64>,
    /// 全部候选。
    pub candidates: Vec<Candidate>,
}

/// 逐条走识别结论时收到的那五样：平台、结论、理由、变体的键、这一趟读了多少字节。
pub type IdentificationVisitor<'a> = dyn FnMut(Option<&str>, State, Option<&str>, &str, u64) + 'a;

/// 一个数据源贡献了多少条候选。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceCount {
    /// 数据源。
    pub source: String,
    /// 候选条数。
    pub candidates: u64,
    /// 其中自动通过的。
    pub accepted: u64,
    /// 涉及多少个变体。
    pub variants: u64,
}

/// 报告要的那几个计数。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CandidateCounts {
    /// 候选总数。
    pub candidates: u64,
    /// 自动通过的候选数。
    pub accepted: u64,
    /// 有不止一条候选的变体数。
    pub multi: u64,
    /// 命中里带**汉化**记号的变体数。
    pub fan: u64,
    /// 命中里带**官中**记号的变体数。
    pub official: u64,
    /// 验出是 NKit 处理过的镜像有几份。
    pub nkit: u64,
    /// 识别建出来的作品数。
    pub works: u64,
    /// 识别建出来的发行版数。
    pub releases: u64,
    /// 按数据源分。
    pub sources: Vec<SourceCount>,
}

/// 算过的一份内容的哈希，写库前的样子。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentHash {
    /// 成员的键。
    pub key: String,
    /// 容器内部路径；裸文件是空串。
    pub inner: String,
    /// 含头（原样）的大小。
    pub size: u64,
    /// 含头（原样）的 CRC-32。
    pub crc32: u32,
    /// 看过字节没有。看过才说得出有没有外挂头。
    pub looked: bool,
    /// 剥掉的是哪种外挂头的名字；看过且没有头时是 `None`。
    pub header: Option<String>,
    /// 去头之后的大小。
    pub bare_size: Option<u64>,
    /// 去头之后的 CRC-32。
    pub bare_crc32: Option<u32>,
    /// 验过 NKit 没有、结论是什么。
    pub nkit: Option<bool>,
}

/// 一个条目在中立库里是什么样。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryFact {
    /// 库里没有这一条。
    Missing,
    /// 是个目录。
    Dir,
    /// 是文件，元数据也读到了，这么大。
    File(u64),
    /// 是文件，但**元数据读不到**（ADR-0021 的第三态）。
    Unreadable,
    /// 符号链接或别的什么，不参与识别。
    Other,
}

impl Catalog {
    /// 全部变体，按键排序。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn variants(&self) -> Result<Vec<super::VariantRow>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT key, platform, rule, main_key, files, bytes, unreadable, manual,
                        work_id, release_id
                 FROM variant ORDER BY key",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok(super::VariantRow {
                    key: row.get(0)?,
                    platform: row.get(1)?,
                    rule: row.get(2)?,
                    main_key: row.get(3)?,
                    files: u64::try_from(row.get::<_, i64>(4)?).unwrap_or(0),
                    bytes: u64::try_from(row.get::<_, i64>(5)?).unwrap_or(0),
                    unreadable_files: u64::try_from(row.get::<_, i64>(6)?).unwrap_or(0),
                    manual: row.get::<_, i64>(7)? != 0,
                    work_id: row.get(8)?,
                    release_id: row.get(9)?,
                })
            })
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 一个**透明容器**穿透出来的内部文件：内部路径、未压缩大小、CRC-32。
    ///
    /// 目录条目与没有内容的条目不返回——它们不参与识别。穿不透的容器返回空表，
    /// 那正是「无判据」的一种。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn container_files(
        &self,
        key: &str,
    ) -> Result<Vec<(String, u64, Option<u32>)>, CatalogError> {
        let mut statement = self
            .conn
            .prepare_cached(
                "SELECT inner, size, crc32 FROM container_entry
                 WHERE key = ?1 AND is_dir = 0 ORDER BY ordinal",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![key], |row| {
                let size = u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0);
                let crc: Option<i64> = row.get(2)?;
                Ok((
                    row.get::<_, String>(0)?,
                    size,
                    crc.and_then(|crc| u32::try_from(crc).ok()),
                ))
            })
            .map_err(|source| self.err(source))?;
        let mut out = Vec::new();
        for row in rows {
            let row = row.map_err(|source| self.err(source))?;
            if row.1 > 0 {
                out.push(row);
            }
        }
        Ok(out)
    }

    /// 一个条目在库里是什么。
    ///
    /// 识别要把这几种情况分开说：目录（目录树转储，这一层认不了）、元数据读不到
    /// （ADR-0021 的第三态）、不在库里。**都塌成「没有大小」的话，报告就说不出
    /// 「为什么没判据」**，而那正是命中率旁边最该说清的一栏。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn entry_fact(&self, key: &str) -> Result<EntryFact, CatalogError> {
        let row: Option<(i64, i64, Option<i64>)> = self
            .conn
            .prepare_cached("SELECT kind, readable, len FROM entry WHERE key = ?1")
            .and_then(|mut statement| {
                statement
                    .query_row(params![key], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                    })
                    .optional()
            })
            .map_err(|source| self.err(source))?;
        Ok(match row {
            None => EntryFact::Missing,
            Some((kind, _, _)) if kind == super::KIND_DIR => EntryFact::Dir,
            Some((kind, _, _)) if kind != super::KIND_FILE => EntryFact::Other,
            Some((_, 0, _)) => EntryFact::Unreadable,
            Some((_, _, len)) => match len.and_then(|len| u64::try_from(len).ok()) {
                Some(len) => EntryFact::File(len),
                None => EntryFact::Unreadable,
            },
        })
    }

    /// 一个**透明容器**上次穿透的结论：`None` 表示库里没有这条（不是容器，或者
    /// 那次扫描关掉了穿透），`Some(None)` 是穿透了，`Some(Some(_))` 是**穿不透**
    /// 并附上那一句。
    ///
    /// 穿不透与不可读是平行的两件事（`CONTEXT.md`），报告要分开说。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn container_status(&self, key: &str) -> Result<Option<Option<String>>, CatalogError> {
        self.conn
            .prepare_cached("SELECT reason, detail FROM container WHERE key = ?1")
            .and_then(|mut statement| {
                statement
                    .query_row(params![key], |row| {
                        let reason: Option<String> = row.get(0)?;
                        let detail: Option<String> = row.get(1)?;
                        Ok(reason.map(|reason| detail.unwrap_or(reason)))
                    })
                    .optional()
            })
            .map_err(|source| self.err(source))
    }

    /// 某个成员上算过的哈希：内部路径 → 那一份。裸文件的内部路径是空串。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn content_hashes(&self, key: &str) -> Result<BTreeMap<String, ContentHash>, CatalogError> {
        let mut statement = self
            .conn
            .prepare_cached(
                "SELECT inner, size, crc32, looked, header, bare_size, bare_crc32, nkit
                 FROM content_hash WHERE key = ?1",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![key], |row| {
                let nkit: Option<i64> = row.get(7)?;
                Ok(ContentHash {
                    key: key.to_string(),
                    inner: row.get(0)?,
                    size: u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                    crc32: u32::try_from(row.get::<_, i64>(2)?).unwrap_or(0),
                    looked: row.get::<_, i64>(3)? != 0,
                    header: row.get(4)?,
                    bare_size: row
                        .get::<_, Option<i64>>(5)?
                        .and_then(|size| u64::try_from(size).ok()),
                    bare_crc32: row
                        .get::<_, Option<i64>>(6)?
                        .and_then(|crc| u32::try_from(crc).ok()),
                    nkit: nkit.map(|nkit| nkit != 0),
                })
            })
            .map_err(|source| self.err(source))?;
        let mut out = BTreeMap::new();
        for row in rows {
            let row = row.map_err(|source| self.err(source))?;
            out.insert(row.inner.clone(), row);
        }
        Ok(out)
    }

    /// 把算过的哈希整批存下来。**读过的盘不白读**（挂账 D14）。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put_content_hashes(&mut self, rows: &[ContentHash]) -> Result<(), CatalogError> {
        if rows.is_empty() {
            return Ok(());
        }
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        {
            let mut insert = tx
                .prepare(
                    "INSERT INTO content_hash(key, inner, size, crc32, looked, header,
                         bare_size, bare_crc32, nkit)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
                     ON CONFLICT(key, inner) DO UPDATE SET
                        size = excluded.size, crc32 = excluded.crc32,
                        looked = excluded.looked, header = excluded.header,
                        bare_size = excluded.bare_size, bare_crc32 = excluded.bare_crc32,
                        nkit = excluded.nkit",
                )
                .map_err(to_err)?;
            for row in rows {
                insert
                    .execute(params![
                        row.key,
                        row.inner,
                        i64::try_from(row.size).unwrap_or(i64::MAX),
                        i64::from(row.crc32),
                        i64::from(row.looked),
                        row.header,
                        row.bare_size
                            .map(|size| i64::try_from(size).unwrap_or(i64::MAX)),
                        row.bare_crc32.map(i64::from),
                        row.nkit.map(i64::from),
                    ])
                    .map_err(to_err)?;
            }
        }
        tx.commit().map_err(to_err)
    }

    /// 把一批变体的结论写进去。**一个变体写两次是同一个结果**：先删掉它上一批候选
    /// 再插新的，于是重跑与分批写都不会攒出重复的候选。
    ///
    /// 调用方在开跑前先调一次
    /// [`clear_identifications`](Self::clear_identifications)——那一步把识别自己上一轮
    /// 造的东西整批清掉。分批写而不是攒到最后一次性写，是为了让被中断的识别留下
    /// 已经算完的那部分（与扫描的批次写同源）。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn write_identifications(
        &mut self,
        records: &[Identification],
    ) -> Result<(), CatalogError> {
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        {
            let mut insert_candidate = tx
                .prepare(
                    "INSERT INTO candidate(variant_key, member_key, inner, confidence, accepted,
                         source, dat, platform, game, rom, hashing, convention, evidence,
                         chinese, serial, release_id)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
                )
                .map_err(to_err)?;
            let mut insert_identification = tx
                .prepare(
                    "INSERT INTO identification(variant_key, state, reason, units, candidates,
                         accepted, nkit, read_bytes)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
                     ON CONFLICT(variant_key) DO UPDATE SET
                        state = excluded.state, reason = excluded.reason,
                        units = excluded.units, candidates = excluded.candidates,
                        accepted = excluded.accepted, nkit = excluded.nkit,
                        read_bytes = excluded.read_bytes",
                )
                .map_err(to_err)?;
            let mut link = tx
                .prepare("UPDATE variant SET work_id = ?2, release_id = ?3 WHERE key = ?1")
                .map_err(to_err)?;
            let mut drop_candidates = tx
                .prepare("DELETE FROM candidate WHERE variant_key = ?1")
                .map_err(to_err)?;
            for record in records {
                drop_candidates
                    .execute(params![record.variant_key])
                    .map_err(to_err)?;
                let accepted = record.candidates.iter().filter(|c| c.accepted).count();
                insert_identification
                    .execute(params![
                        record.variant_key,
                        record.state.label(),
                        record.reason,
                        i64::try_from(record.units).unwrap_or(i64::MAX),
                        i64::try_from(record.candidates.len()).unwrap_or(i64::MAX),
                        i64::try_from(accepted).unwrap_or(i64::MAX),
                        i64::try_from(record.nkit).unwrap_or(i64::MAX),
                        i64::try_from(record.read_bytes).unwrap_or(i64::MAX),
                    ])
                    .map_err(to_err)?;
                for candidate in &record.candidates {
                    insert_candidate
                        .execute(params![
                            record.variant_key,
                            candidate.member_key,
                            candidate.inner,
                            candidate.confidence.label(),
                            i64::from(candidate.accepted),
                            candidate.source,
                            candidate.dat,
                            candidate.platform,
                            candidate.game,
                            candidate.rom,
                            candidate.hashing.label(),
                            candidate.convention.label(),
                            candidate.evidence,
                            candidate.chinese.map(ChineseMark::label),
                            candidate.serial,
                            candidate.release_id,
                        ])
                        .map_err(to_err)?;
                }
                if record.work_id.is_some() || record.release_id.is_some() {
                    link.execute(params![
                        record.variant_key,
                        record.work_id,
                        record.release_id
                    ])
                    .map_err(to_err)?;
                }
            }
        }
        tx.commit().map_err(to_err)
    }

    /// 把**识别自己**上一轮造的东西清掉：候选、结论、以及它建出来的作品与发行版。
    ///
    /// 顺序是有讲究的：先摘链接再删行，否则变体上会留下指向已删除记录的悬空 id。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn clear_identifications(&mut self) -> Result<(), CatalogError> {
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        for sql in [
            "UPDATE variant SET release_id = NULL
             WHERE release_id IN (SELECT id FROM release WHERE origin = '识别')",
            "UPDATE variant SET work_id = NULL
             WHERE work_id IN (SELECT id FROM work WHERE origin = '识别')",
            "DELETE FROM candidate",
            "DELETE FROM identification",
            "DELETE FROM release WHERE origin = '识别'",
            "DELETE FROM work WHERE origin = '识别'",
        ] {
            tx.execute(sql, []).map_err(to_err)?;
        }
        tx.commit().map_err(to_err)
    }

    /// 一个变体的全部**候选**，按 id 排序。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn candidates_of(&self, variant_key: &str) -> Result<Vec<Candidate>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT member_key, inner, confidence, accepted, source, dat, platform,
                        game, rom, hashing, convention, evidence, chinese, serial, release_id
                 FROM candidate WHERE variant_key = ?1 ORDER BY id",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![variant_key], |row| {
                let confidence: String = row.get(2)?;
                let hashing: String = row.get(9)?;
                let convention: String = row.get(10)?;
                let chinese: Option<String> = row.get(12)?;
                Ok(Candidate {
                    member_key: row.get(0)?,
                    inner: row.get(1)?,
                    confidence: Confidence::from_label(&confidence).unwrap_or(Confidence::Low),
                    accepted: row.get::<_, i64>(3)? != 0,
                    source: row.get(4)?,
                    dat: row.get(5)?,
                    platform: row.get(6)?,
                    game: row.get(7)?,
                    rom: row.get(8)?,
                    hashing: Convention::from_label(&hashing).unwrap_or(Convention::AsIs),
                    convention: Convention::from_label(&convention).unwrap_or(Convention::AsIs),
                    evidence: row.get(11)?,
                    chinese: chinese.as_deref().and_then(|label| match label {
                        "汉化" => Some(ChineseMark::FanTranslated),
                        "官中" => Some(ChineseMark::Official),
                        _ => None,
                    }),
                    serial: row.get(13)?,
                    release_id: row.get(14)?,
                })
            })
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 一条条走过全部识别结论：平台、结论、理由、变体的键、这一趟读了多少字节。
    ///
    /// 走回调而不是返回一整份 `Vec`：真库里这是 46,444 行，报告要的只是几个计数与
    /// 几个例子，攒一份完整的表纯属浪费。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn for_each_identification(
        &self,
        each: &mut IdentificationVisitor,
    ) -> Result<(), CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT v.platform, i.state, i.reason, i.variant_key, i.read_bytes
                 FROM identification i JOIN variant v ON v.key = i.variant_key
                 ORDER BY i.variant_key",
            )
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let platform: Option<String> = row.get(0).map_err(|source| self.err(source))?;
            let state: String = row.get(1).map_err(|source| self.err(source))?;
            let reason: Option<String> = row.get(2).map_err(|source| self.err(source))?;
            let key: String = row.get(3).map_err(|source| self.err(source))?;
            let read_bytes: i64 = row.get(4).map_err(|source| self.err(source))?;
            each(
                platform.as_deref(),
                State::from_label(&state).unwrap_or(State::NoEvidence),
                reason.as_deref(),
                &key,
                u64::try_from(read_bytes).unwrap_or(0),
            );
        }
        Ok(())
    }

    /// 报告要的那几个计数：候选、自动通过、中文、NKit、建出来的作品与发行版。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn candidate_counts(&self) -> Result<CandidateCounts, CatalogError> {
        let mut counts = CandidateCounts::default();
        let one = |sql: &str| -> Result<u64, CatalogError> {
            let value: i64 = self
                .conn
                .query_row(sql, [], |row| row.get(0))
                .map_err(|source| self.err(source))?;
            Ok(u64::try_from(value).unwrap_or(0))
        };
        counts.candidates = one("SELECT COUNT(*) FROM candidate")?;
        counts.accepted = one("SELECT COUNT(*) FROM candidate WHERE accepted <> 0")?;
        counts.multi = one("SELECT COUNT(*) FROM (SELECT variant_key FROM candidate
             GROUP BY variant_key HAVING COUNT(*) > 1)")?;
        counts.fan =
            one("SELECT COUNT(DISTINCT variant_key) FROM candidate WHERE chinese = '汉化'")?;
        counts.official =
            one("SELECT COUNT(DISTINCT variant_key) FROM candidate WHERE chinese = '官中'")?;
        counts.nkit = one("SELECT COALESCE(SUM(nkit), 0) FROM identification")?;
        counts.works = one("SELECT COUNT(*) FROM work WHERE origin = '识别'")?;
        counts.releases = one("SELECT COUNT(*) FROM release WHERE origin = '识别'")?;

        let mut statement = self
            .conn
            .prepare(
                "SELECT source, COUNT(*), SUM(accepted), COUNT(DISTINCT variant_key)
                 FROM candidate GROUP BY source ORDER BY COUNT(*) DESC",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok(SourceCount {
                    source: row.get(0)?,
                    candidates: u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                    accepted: u64::try_from(row.get::<_, i64>(2)?).unwrap_or(0),
                    variants: u64::try_from(row.get::<_, i64>(3)?).unwrap_or(0),
                })
            })
            .map_err(|source| self.err(source))?;
        counts.sources = rows
            .collect::<Result<_, _>>()
            .map_err(|source| self.err(source))?;
        Ok(counts)
    }

    /// 一个变体这一轮的结论；没识别过时是 `None`。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn identification_of(
        &self,
        variant_key: &str,
    ) -> Result<Option<(State, Option<String>)>, CatalogError> {
        self.conn
            .query_row(
                "SELECT state, reason FROM identification WHERE variant_key = ?1",
                params![variant_key],
                |row| {
                    let state: String = row.get(0)?;
                    Ok((
                        State::from_label(&state).unwrap_or(State::NoEvidence),
                        row.get(1)?,
                    ))
                },
            )
            .optional()
            .map_err(|source| self.err(source))
    }
}
