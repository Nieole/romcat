//! **沉淀库**：由**裁决**积累起来的本地数据库，把文件哈希直接钉到发行版、汉化组与版本上。
//!
//! 它补的正是 TOSEC 与 No-Intro 覆盖不到的中文汉化部分，**可导出分享**（`CONTEXT.md`、
//! ADR-0008）。裁决一次永久受益：重装、换机、日后拷进来的同一文件直接精确命中。
//!
//! ## 为什么它不住在中立库里
//!
//! 中立库的每一条都是**可再生的**——扫描、成型、识别、刮削全都能重跑，所以它的结构
//! 版本一变就让用户删库重扫（`catalog::SCHEMA_VERSION`），那是省下一整套迁移代码的
//! 便宜买卖。**裁决不可再生**：那是用户一条条看出来的判断，删掉就没了。
//!
//! 两种东西住在同一个文件里，便宜那条路就再也走不通了（原挂账 D26）。于是把不可再生
//! 的那一份搬出来单独存——和 **DAT 库**、**媒体池**是同一个形状，理由也同源：
//!
//! - **一条裁决说的是「世上这份内容是什么」，与它躺在哪块盘上无关。** 键是内容哈希
//!   不是路径，所以它**不跟主库走**（[`workspace::verdict_store_path`] 不带
//!   [`Slug`](crate::workspace::Slug)）。两块盘接同一台机器，裁决一次两边都受益。
//! - **删库重扫不该赔上攒了几个月的裁决。**
//!
//! 中立库里那几行 `origin = 裁决` 的作品与发行版因此是**这份库的投影**，不是它本身：
//! 重跑识别把它们整批清掉再照沉淀库重建一遍，结果一模一样。
//!
//! ## 这份库自己怎么迁移
//!
//! 装着不可再生的东西，就**不许**再走「版本对不上让用户删掉」那条路。这里走顺序迁移：
//! [`MIGRATIONS`] 是一串只增不改的建表 / 改表语句，`PRAGMA user_version` 记着跑到第几条，
//! 打开时把没跑过的接着跑完。**往前迁得动，往后（库比程序新）如实拒绝并说清**——
//! 那时该换新程序，而不是删库。
//!
//! 现在只有一条迁移（建表），趁库还是空的把框架立起来最便宜。
//!
//! ## 两种锚，如实分开
//!
//! - **内容锚**：`CRC-32` 加大小——本项目第一命中层的口径（ADR-0002 的再修订），
//!   容器里零解压就有、裸文件上一趟识别算过就存着，于是裁决**零成本**、当场生效。
//!   它是**可分享**的那一种——换机、改名、重新整理目录都不影响命中。
//!   ADR-0008 的字面写法是以 `SHA-1` 为键，**这一版没有实现它**：那要把内容整份读一遍。
//!   表里 `sha1` 那一列已经建好、导出格式里也有它（ADR-0008 的修订段、挂账 D98）。
//! - **路径锚**：拿不到内容判据时（容器穿不透、压缩镜像、目录树转储——识别里的
//!   **无判据**那一档）退到变体的键上。它**只在本机成立**，导出时默认不带。
//!
//! 分开记而不是含糊成一种，是 ADR-0021 那条纪律在这里的样子：说得出「这条裁决换台
//! 机器还认不认得出」，比让用户以为每条都认得出强。

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::catalog::now_secs;
use crate::dat::chinese::ChineseMark;

/// 沉淀库跑到第几条迁移，也就是它的结构版本。
///
/// 等于 [`MIGRATIONS`] 的条数——加一条迁移这个数自然加一，不必两处各记一遍。
#[must_use]
pub fn schema_version() -> u32 {
    u32::try_from(MIGRATIONS.len()).unwrap_or(u32::MAX)
}

/// 顺序迁移。**只许往后追加，已经发出去的一条一个字都不许改**——改了的话，早先按旧
/// 语句建出来的库与新装的程序建出来的库形状不同，而 `user_version` 说它们是同一版。
const MIGRATIONS: &[&str] = &[
    // 1：建表。
    "\
-- 一条**裁决**。锚有两种（见模块文档），靠 `anchor` 分开；两种各有各的唯一性。
CREATE TABLE IF NOT EXISTS verdict(
    id          INTEGER PRIMARY KEY,
    anchor      TEXT    NOT NULL,
    -- 内容锚：CRC-32 加大小是第一命中层的口径，零成本地从中立库取得。
    crc32       INTEGER,
    size        INTEGER,
    -- SHA-1 是 ADR-0008 写明的键，但它要整份读一遍才算得出来（容器里的还要解压）。
    -- **这一版一条都不算，因此这一列眼下一律为空**——列先建好，将来补上只是「算不算」
    -- 的问题，不是「存不存得下」的问题（ADR-0008 的修订段、挂账 D98）。
    sha1        TEXT,
    -- 路径锚：哪份主库的哪个变体。只在本机成立。
    library     TEXT,
    variant_key TEXT,
    -- 裁决说了什么：定成一个发行版 / 确认它没有发行版 / 都不对而且认不出。
    kind        TEXT    NOT NULL,
    work        TEXT,
    platform    TEXT,
    region      TEXT,
    serial      TEXT,
    languages   TEXT,
    chinese     TEXT,
    -- **汉化组**与**版本**：自动识别只保证做到发行版级，这两样靠裁决补（ADR-0008）。
    team        TEXT,
    version     TEXT,
    note        TEXT,
    decided_at  INTEGER NOT NULL
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS verdict_content
    ON verdict(crc32, size) WHERE anchor = '内容';
CREATE UNIQUE INDEX IF NOT EXISTS verdict_path
    ON verdict(library, variant_key) WHERE anchor = '路径';
CREATE INDEX IF NOT EXISTS verdict_sha1 ON verdict(sha1);
",
];

/// 「内容锚」在库里与报告里叫什么。
pub const ANCHOR_CONTENT: &str = "内容";

/// 「路径锚」在库里与报告里叫什么。
pub const ANCHOR_PATH: &str = "路径";

/// 沉淀库读写不了的原因。
#[derive(Debug, thiserror::Error)]
pub enum VerdictError {
    /// 目录建不出来。
    #[error("沉淀库的目录建不出来：{path}（{source}）")]
    Io {
        /// 出问题的路径。
        path: String,
        /// 底层错误。
        source: std::io::Error,
    },
    /// 底层读写失败。
    #[error("沉淀库读写失败：{path}（{source}）")]
    Sqlite {
        /// 沉淀库文件。
        path: String,
        /// 底层错误。
        source: rusqlite::Error,
    },
    /// 库比程序新。**绝不提议删掉它**——里面装的是裁决。
    #[error(
        "沉淀库 {path} 是第 {found} 版结构，本程序只认到第 {expected} 版。\
         请换用新版程序打开它；这份库里装着裁决，删不得"
    )]
    Ahead {
        /// 沉淀库文件。
        path: String,
        /// 文件里的版本。
        found: u32,
        /// 本程序认得到第几版。
        expected: u32,
    },
    /// 导入的文件读不懂。
    #[error("这份裁决文件读不懂：{0}")]
    Format(String),
}

/// 一条裁决钉在什么上。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Anchor {
    /// **内容锚**：换机、改名都认得出，可分享。
    Content {
        /// 含头（原样）的 CRC-32，与第一命中层同一套口径。
        crc32: u32,
        /// 未压缩大小。
        size: u64,
        /// SHA-1（40 位小写十六进制）；没算过就是 `None`。
        /// **这一版一条都不算**（ADR-0008 的修订段、挂账 D98）。
        sha1: Option<String>,
    },
    /// **路径锚**：拿不到内容判据时的退路，**只在本机成立**。
    Path {
        /// 哪一份主库（`--library` 起的名字，或者中立库的名字）。
        library: String,
        /// 变体的键。
        variant_key: String,
    },
}

impl Anchor {
    /// 报告里说的那个词。
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Content { .. } => ANCHOR_CONTENT,
            Self::Path { .. } => ANCHOR_PATH,
        }
    }

    /// 这条锚**换台机器还认不认得出**。
    #[must_use]
    pub fn is_shareable(&self) -> bool {
        matches!(self, Self::Content { .. })
    }

    /// 给人看的一句。
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Content { crc32, size, sha1 } => match sha1 {
                Some(sha1) => format!("CRC-32 {crc32:08X} + {size} 字节，SHA-1 {sha1}"),
                None => format!("CRC-32 {crc32:08X} + {size} 字节"),
            },
            Self::Path {
                library,
                variant_key,
            } => format!("主库「{library}」的 {variant_key}（只在本机成立）"),
        }
    }
}

/// 一条裁决**说了什么**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// 定成这个作品的这一次发行。
    Release(Facts),
    /// **确认它没有发行版**：同人移植与 homebrew 直接挂在作品下（`CONTEXT.md`）。
    ///
    /// 这一档必须**明说**而不能靠「作品有、发行版空」推断出来——两者形状一样，
    /// 混着读会把「人定了作品、机器认出了发行版」的变体误判成 homebrew（原挂账 D48）。
    NoRelease {
        /// 挂在哪个作品下；认不出就留空。
        work: Option<String>,
    },
    /// **都不对，而且认不出是什么**。记下来是为了不再进队列——「我看过了，认不出」
    /// 与「还没人看过」是两件事。
    Unknown,
}

impl Decision {
    /// 存进库、也打给用户的那个词。
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Release(_) => "发行版",
            Self::NoRelease { .. } => "没有发行版",
            Self::Unknown => "认不出",
        }
    }

    /// 这条裁决挂在哪个作品下。
    #[must_use]
    pub fn work(&self) -> Option<&str> {
        match self {
            Self::Release(facts) => Some(facts.work.as_str()),
            Self::NoRelease { work } => work.as_deref(),
            Self::Unknown => None,
        }
    }
}

/// 一条**发行版**裁决记下来的事实。
///
/// **汉化组**与**版本**在这里：自动识别只保证做到发行版级，「这是谁汉化的第几版」
/// 靠裁决补（ADR-0008）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    /// **作品**名。裁决必须说得出这一样，否则它什么也没定下来。
    pub work: String,
    /// 平台；可空。
    pub platform: Option<String>,
    /// 地区；可空。
    pub region: Option<String>,
    /// 序列号；可空。
    pub serial: Option<String>,
    /// 语言标记组（`Ja,Zh-Hans`）；可空。
    pub languages: Option<String>,
    /// 中文身份：**汉化版**还是**官中版**（ADR-0012）。
    pub chinese: Option<ChineseMark>,
    /// **汉化组**：谁做的这个中文版本。
    pub team: Option<String>,
    /// 版本，如 `v1.2`。
    pub version: Option<String>,
}

/// 一条裁决。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// 钉在什么上。
    pub anchor: Anchor,
    /// 说了什么。
    pub decision: Decision,
    /// 记一句为什么。半年后你会想知道当初凭什么这么定。
    pub note: Option<String>,
    /// 什么时候定的（UNIX 纪元起的秒）。
    pub decided_at: i64,
}

impl Verdict {
    /// 立一条**现在**定下来的裁决。
    #[must_use]
    pub fn now(anchor: Anchor, decision: Decision) -> Self {
        Self {
            anchor,
            decision,
            note: None,
            decided_at: now_secs(),
        }
    }

    /// 给它记一句为什么。
    #[must_use]
    pub fn with_note(mut self, note: Option<String>) -> Self {
        self.note = note;
        self
    }

    /// **依据**：这条裁决打进候选里时说的那句话。
    #[must_use]
    pub fn evidence(&self) -> String {
        let mut text = format!(
            "沉淀库：人工**裁决**定下来的（锚是{}——{}）",
            self.anchor.label(),
            self.anchor.describe(),
        );
        if let Decision::Release(facts) = &self.decision {
            if let Some(team) = &facts.team {
                text.push_str(&format!("；汉化组「{team}」"));
            }
            if let Some(version) = &facts.version {
                text.push_str(&format!("；版本 {version}"));
            }
        }
        if let Some(note) = &self.note {
            text.push_str(&format!("；{note}"));
        }
        text
    }
}

/// 沉淀库里有多少条、都是什么样。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    /// 一共几条。
    pub total: u64,
    /// 内容锚几条——**可分享的就是这些**。
    pub content: u64,
    /// 路径锚几条，只在本机成立。
    pub path: u64,
    /// 算过 SHA-1 的几条。
    pub with_sha1: u64,
    /// 定成发行版的几条。
    pub releases: u64,
    /// 确认没有发行版的几条。
    pub no_release: u64,
    /// 认不出的几条。
    pub unknown: u64,
    /// 补了**汉化组**的几条。
    pub with_team: u64,
}

/// **沉淀库**。
#[derive(Debug)]
pub struct Store {
    conn: Connection,
    path: String,
}

impl Store {
    /// 打开（必要时新建并迁移）一份落在磁盘上的沉淀库。
    ///
    /// # Errors
    /// 目录建不出来、打不开、迁移跑不动、或者库比程序新时返回错误。
    pub fn open(path: &Path) -> Result<Self, VerdictError> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|source| VerdictError::Io {
                path: crate::path::display(parent),
                source,
            })?;
        }
        let display = crate::path::display(path);
        let conn = Connection::open(path).map_err(|source| VerdictError::Sqlite {
            path: display.clone(),
            source,
        })?;
        let store = Self {
            conn,
            path: display,
        };
        store.migrate()?;
        Ok(store)
    }

    /// 开一份只活在内存里的。测试用。
    ///
    /// # Errors
    /// 建不出来时返回错误。
    pub fn in_memory() -> Result<Self, VerdictError> {
        let conn = Connection::open_in_memory().map_err(|source| VerdictError::Sqlite {
            path: "（内存）".to_string(),
            source,
        })?;
        let store = Self {
            conn,
            path: "（内存）".to_string(),
        };
        store.migrate()?;
        Ok(store)
    }

    /// 把没跑过的迁移接着跑完。
    ///
    /// **绝不倒着迁，也绝不叫用户删库。** 库比程序新时如实说清并停下——那时该换程序。
    fn migrate(&self) -> Result<(), VerdictError> {
        self.conn
            .execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")
            .map_err(|source| self.err(source))?;
        let found: u32 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .map(|value| u32::try_from(value).unwrap_or(0))
            .map_err(|source| self.err(source))?;
        let target = schema_version();
        if found > target {
            return Err(VerdictError::Ahead {
                path: self.path.clone(),
                found,
                expected: target,
            });
        }
        for sql in &MIGRATIONS[found as usize..] {
            self.conn
                .execute_batch(sql)
                .map_err(|source| self.err(source))?;
        }
        // `PRAGMA` 不吃占位符，而 `target` 是本程序自己的常量，不是外面来的数。
        self.conn
            .execute_batch(&format!("PRAGMA user_version = {target}"))
            .map_err(|source| self.err(source))?;
        Ok(())
    }

    fn err(&self, source: rusqlite::Error) -> VerdictError {
        VerdictError::Sqlite {
            path: self.path.clone(),
            source,
        }
    }

    /// 沉淀库落在哪。
    #[must_use]
    pub fn location(&self) -> &str {
        &self.path
    }

    /// 记下（或覆盖）一条裁决。返回它是不是**新**的一条。
    ///
    /// 同一条锚上再裁一次是**覆盖**：人改主意了，改的就该是那一条，而不是攒出两条
    /// 互相打架的记录。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put(&mut self, verdict: &Verdict) -> Result<bool, VerdictError> {
        let existed = self.find(&verdict.anchor)?.is_some();
        let facts = match &verdict.decision {
            Decision::Release(facts) => Some(facts),
            _ => None,
        };
        let work = verdict.decision.work();
        let (crc32, size, sha1, library, variant_key) = match &verdict.anchor {
            Anchor::Content { crc32, size, sha1 } => (
                Some(i64::from(*crc32)),
                Some(i64::try_from(*size).unwrap_or(i64::MAX)),
                sha1.clone(),
                None,
                None,
            ),
            Anchor::Path {
                library,
                variant_key,
            } => (
                None,
                None,
                None,
                Some(library.clone()),
                Some(variant_key.clone()),
            ),
        };
        // 覆盖走「先删再插」而不是 `ON CONFLICT`：两条唯一索引各管一半（内容锚一条、
        // 路径锚一条），`ON CONFLICT` 的目标只能点一条索引，写两条 upsert 语句就有
        // 两处要一起改。
        self.remove(&verdict.anchor)?;
        self.conn
            .execute(
                "INSERT INTO verdict(anchor, crc32, size, sha1, library, variant_key,
                     kind, work, platform, region, serial, languages, chinese, team, version,
                     note, decided_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
                params![
                    verdict.anchor.label(),
                    crc32,
                    size,
                    sha1,
                    library,
                    variant_key,
                    verdict.decision.label(),
                    work,
                    facts.and_then(|f| f.platform.clone()),
                    facts.and_then(|f| f.region.clone()),
                    facts.and_then(|f| f.serial.clone()),
                    facts.and_then(|f| f.languages.clone()),
                    facts.and_then(|f| f.chinese).map(ChineseMark::label),
                    facts.and_then(|f| f.team.clone()),
                    facts.and_then(|f| f.version.clone()),
                    verdict.note,
                    verdict.decided_at,
                ],
            )
            .map_err(|source| self.err(source))?;
        Ok(!existed)
    }

    /// 忘掉一条锚上的裁决。返回真的忘掉了没有。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn remove(&mut self, anchor: &Anchor) -> Result<bool, VerdictError> {
        let changed = match anchor {
            Anchor::Content { crc32, size, .. } => self.conn.execute(
                "DELETE FROM verdict WHERE anchor = ?1 AND crc32 = ?2 AND size = ?3",
                params![
                    ANCHOR_CONTENT,
                    i64::from(*crc32),
                    i64::try_from(*size).unwrap_or(i64::MAX)
                ],
            ),
            Anchor::Path {
                library,
                variant_key,
            } => self.conn.execute(
                "DELETE FROM verdict WHERE anchor = ?1 AND library = ?2 AND variant_key = ?3",
                params![ANCHOR_PATH, library, variant_key],
            ),
        };
        changed
            .map(|rows| rows > 0)
            .map_err(|source| self.err(source))
    }

    /// 查一条锚上的裁决。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn find(&self, anchor: &Anchor) -> Result<Option<Verdict>, VerdictError> {
        let row = match anchor {
            Anchor::Content { crc32, size, .. } => self
                .conn
                .query_row(
                    &format!("{SELECT} WHERE anchor = ?1 AND crc32 = ?2 AND size = ?3"),
                    params![
                        ANCHOR_CONTENT,
                        i64::from(*crc32),
                        i64::try_from(*size).unwrap_or(i64::MAX)
                    ],
                    read_row,
                )
                .optional(),
            Anchor::Path {
                library,
                variant_key,
            } => self
                .conn
                .query_row(
                    &format!("{SELECT} WHERE anchor = ?1 AND library = ?2 AND variant_key = ?3"),
                    params![ANCHOR_PATH, library, variant_key],
                    read_row,
                )
                .optional(),
        };
        row.map_err(|source| self.err(source))
    }

    /// 全部裁决，按定下来的先后排。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn all(&self) -> Result<Vec<Verdict>, VerdictError> {
        let mut statement = self
            .conn
            .prepare(&format!("{SELECT} ORDER BY decided_at, id"))
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], read_row)
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 库里有多少条、都是什么样。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn counts(&self) -> Result<Counts, VerdictError> {
        let one = |sql: &str| -> Result<u64, VerdictError> {
            self.conn
                .query_row(sql, [], |row| row.get::<_, i64>(0))
                .map(|value| u64::try_from(value).unwrap_or(0))
                .map_err(|source| self.err(source))
        };
        Ok(Counts {
            total: one("SELECT COUNT(*) FROM verdict")?,
            content: one("SELECT COUNT(*) FROM verdict WHERE anchor = '内容'")?,
            path: one("SELECT COUNT(*) FROM verdict WHERE anchor = '路径'")?,
            with_sha1: one("SELECT COUNT(*) FROM verdict WHERE sha1 IS NOT NULL")?,
            releases: one("SELECT COUNT(*) FROM verdict WHERE kind = '发行版'")?,
            no_release: one("SELECT COUNT(*) FROM verdict WHERE kind = '没有发行版'")?,
            unknown: one("SELECT COUNT(*) FROM verdict WHERE kind = '认不出'")?,
            with_team: one("SELECT COUNT(*) FROM verdict WHERE team IS NOT NULL")?,
        })
    }
}

/// 读一行时要的那一串列，两处查询共用。
const SELECT: &str = "SELECT anchor, crc32, size, sha1, library, variant_key, kind, work,
     platform, region, serial, languages, chinese, team, version, note, decided_at
     FROM verdict";

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Verdict> {
    let anchor: String = row.get(0)?;
    let anchor = if anchor == ANCHOR_PATH {
        Anchor::Path {
            library: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
            variant_key: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
        }
    } else {
        Anchor::Content {
            crc32: u32::try_from(row.get::<_, i64>(1).unwrap_or(0)).unwrap_or(0),
            size: u64::try_from(row.get::<_, i64>(2).unwrap_or(0)).unwrap_or(0),
            sha1: row.get(3)?,
        }
    };
    let work: Option<String> = row.get(7)?;
    let kind: String = row.get(6)?;
    let decision = match kind.as_str() {
        "没有发行版" => Decision::NoRelease { work },
        "认不出" => Decision::Unknown,
        _ => Decision::Release(Facts {
            work: work.unwrap_or_default(),
            platform: row.get(8)?,
            region: row.get(9)?,
            serial: row.get(10)?,
            languages: row.get(11)?,
            chinese: row
                .get::<_, Option<String>>(12)?
                .as_deref()
                .and_then(mark_of_label),
            team: row.get(13)?,
            version: row.get(14)?,
        }),
    };
    Ok(Verdict {
        anchor,
        decision,
        note: row.get(15)?,
        decided_at: row.get(16)?,
    })
}

/// 库里存的那个词认回一个中文记号；认不出一律当作没有记号。
fn mark_of_label(label: &str) -> Option<ChineseMark> {
    match label {
        "汉化" => Some(ChineseMark::FanTranslated),
        "官中" => Some(ChineseMark::Official),
        _ => None,
    }
}

/// 识别那一趟拿在手里的沉淀库快照。
///
/// 是一份**内存里的快照**而不是一个连接：识别要为 46,444 个变体各查一次，逐次开库查
/// 是把一件常数时间的事做成 46,444 次 I/O。库里的条数与裁决的条数同阶（几万），
/// 整份读进来不值一提。
#[derive(Debug, Default)]
pub struct Index {
    content: BTreeMap<(u32, u64), Verdict>,
    path: BTreeMap<String, Verdict>,
}

impl Index {
    /// 一条都没有的空快照。没有沉淀库时用它，识别照跑不误。
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// 把一份沉淀库整个读进来。`library` 是这一趟对着的主库名，**只有它的路径锚算数**。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn load(store: &Store, library: &str) -> Result<Self, VerdictError> {
        let mut index = Self::default();
        for verdict in store.all()? {
            match &verdict.anchor {
                Anchor::Content { crc32, size, .. } => {
                    index.content.insert((*crc32, *size), verdict);
                }
                Anchor::Path {
                    library: owner,
                    variant_key,
                } if owner == library => {
                    let key = variant_key.clone();
                    index.path.insert(key, verdict);
                }
                Anchor::Path { .. } => {}
            }
        }
        Ok(index)
    }

    /// 一条都没有吗。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.content.is_empty() && self.path.is_empty()
    }

    /// 一共几条（只算这一趟用得上的）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.content.len() + self.path.len()
    }

    /// 拿判据查一条内容锚的裁决。
    #[must_use]
    pub fn by_content(&self, crc32: u32, size: u64) -> Option<&Verdict> {
        self.content.get(&(crc32, size))
    }

    /// 拿变体的键查一条路径锚的裁决。
    #[must_use]
    pub fn by_path(&self, variant_key: &str) -> Option<&Verdict> {
        self.path.get(variant_key)
    }
}

/// 导出文件的格式名。导入时认它，认不出就拒绝——**读错一份别人的裁决比读不了更糟**。
pub const EXPORT_FORMAT: &str = "romcat-沉淀库";

/// 导出文件的版本。
pub const EXPORT_VERSION: u32 = 1;

/// 一份可分享的裁决文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Export {
    /// 格式名，固定是 [`EXPORT_FORMAT`]。
    pub format: String,
    /// 格式版本。
    pub version: u32,
    /// 导出的时刻（UNIX 纪元起的秒）。
    pub exported_at: i64,
    /// 全部裁决。
    pub verdicts: Vec<Row>,
}

/// 导出文件里的一条。
///
/// CRC-32 写成**八位十六进制字符串**而不是十进制数：DAT 与哈希工具都这么印，
/// 人拿它去 grep 一份 DAT 时不必先换算。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Row {
    /// CRC-32，八位十六进制；路径锚的那些没有。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub crc32: Option<String>,
    /// 未压缩大小。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub size: Option<u64>,
    /// SHA-1；没算过就没有。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sha1: Option<String>,
    /// 路径锚：哪份主库。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub library: Option<String>,
    /// 路径锚：变体的键。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub variant_key: Option<String>,
    /// 裁决说了什么：`发行版` / `没有发行版` / `认不出`。
    pub kind: String,
    /// 作品名。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub work: Option<String>,
    /// 平台。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub platform: Option<String>,
    /// 地区。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub region: Option<String>,
    /// 序列号。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub serial: Option<String>,
    /// 语言标记组。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub languages: Option<String>,
    /// 中文身份：`汉化` 或 `官中`。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub chinese: Option<String>,
    /// **汉化组**。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub team: Option<String>,
    /// 版本。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub version: Option<String>,
    /// 记的那一句。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub note: Option<String>,
    /// 定下来的时刻。
    pub decided_at: i64,
}

impl Row {
    /// 把一条裁决折成导出行。
    #[must_use]
    pub fn of(verdict: &Verdict) -> Self {
        let facts = match &verdict.decision {
            Decision::Release(facts) => Some(facts),
            _ => None,
        };
        let mut row = Self {
            kind: verdict.decision.label().to_string(),
            work: verdict.decision.work().map(ToString::to_string),
            platform: facts.and_then(|f| f.platform.clone()),
            region: facts.and_then(|f| f.region.clone()),
            serial: facts.and_then(|f| f.serial.clone()),
            languages: facts.and_then(|f| f.languages.clone()),
            chinese: facts
                .and_then(|f| f.chinese)
                .map(|mark| mark.label().to_string()),
            team: facts.and_then(|f| f.team.clone()),
            version: facts.and_then(|f| f.version.clone()),
            note: verdict.note.clone(),
            decided_at: verdict.decided_at,
            ..Self::default()
        };
        match &verdict.anchor {
            Anchor::Content { crc32, size, sha1 } => {
                row.crc32 = Some(format!("{crc32:08X}"));
                row.size = Some(*size);
                row.sha1 = sha1.clone();
            }
            Anchor::Path {
                library,
                variant_key,
            } => {
                row.library = Some(library.clone());
                row.variant_key = Some(variant_key.clone());
            }
        }
        row
    }

    /// 从导出行认回一条裁决；锚认不出来时是 `None`。
    #[must_use]
    pub fn into_verdict(self) -> Option<Verdict> {
        let anchor = match (&self.crc32, self.size, &self.variant_key) {
            (Some(crc32), Some(size), _) => Anchor::Content {
                crc32: u32::from_str_radix(crc32.trim(), 16).ok()?,
                size,
                sha1: self.sha1.clone(),
            },
            (_, _, Some(variant_key)) => Anchor::Path {
                library: self.library.clone()?,
                variant_key: variant_key.clone(),
            },
            _ => return None,
        };
        let decision = match self.kind.as_str() {
            "没有发行版" => Decision::NoRelease { work: self.work },
            "认不出" => Decision::Unknown,
            _ => Decision::Release(Facts {
                work: self.work?,
                platform: self.platform,
                region: self.region,
                serial: self.serial,
                languages: self.languages,
                chinese: self.chinese.as_deref().and_then(mark_of_label),
                team: self.team,
                version: self.version,
            }),
        };
        Some(Verdict {
            anchor,
            decision,
            note: self.note,
            decided_at: self.decided_at,
        })
    }
}

/// 导入一份裁决文件之后的账。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Imported {
    /// 文件里一共几条。
    pub read: u64,
    /// 新收下几条。
    pub added: u64,
    /// 盖掉了本机已有的几条。
    pub replaced: u64,
    /// 认不出锚、丢掉了几条。
    pub unreadable: u64,
}

impl Store {
    /// 把库里的裁决折成一份可分享的文件。
    ///
    /// `include_path` 为假时**不带路径锚的那些**：它们只在本机成立，带给别人只是噪音，
    /// 而且顺带把自己的目录结构也交出去了。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn export(&self, include_path: bool) -> Result<Export, VerdictError> {
        let verdicts = self
            .all()?
            .iter()
            .filter(|verdict| include_path || verdict.anchor.is_shareable())
            .map(Row::of)
            .collect();
        Ok(Export {
            format: EXPORT_FORMAT.to_string(),
            version: EXPORT_VERSION,
            exported_at: now_secs(),
            verdicts,
        })
    }

    /// 收下一份别人的裁决文件。
    ///
    /// **同一条锚上本机已有裁决时照样盖掉**——导入是一次明确的动作，用户说的是
    /// 「用这一份」。账上分开记「新收」与「盖掉」，于是盖掉了多少看得见。
    ///
    /// # Errors
    /// 文件读不懂、或者写库失败时返回错误。
    pub fn import(&mut self, text: &str) -> Result<Imported, VerdictError> {
        let export: Export = serde_json::from_str(text)
            .map_err(|error| VerdictError::Format(format!("不是一份 JSON 裁决文件：{error}")))?;
        if export.format != EXPORT_FORMAT {
            return Err(VerdictError::Format(format!(
                "格式是「{}」，本程序只认「{EXPORT_FORMAT}」",
                export.format
            )));
        }
        if export.version > EXPORT_VERSION {
            return Err(VerdictError::Format(format!(
                "这份文件是第 {} 版格式，本程序只认到第 {EXPORT_VERSION} 版。请换用新版程序",
                export.version
            )));
        }
        let mut account = Imported {
            read: u64::try_from(export.verdicts.len()).unwrap_or(u64::MAX),
            ..Imported::default()
        };
        for row in export.verdicts {
            match row.into_verdict() {
                Some(verdict) => {
                    if self.put(&verdict)? {
                        account.added += 1;
                    } else {
                        account.replaced += 1;
                    }
                }
                None => account.unreadable += 1,
            }
        }
        Ok(account)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 内容锚(crc32: u32, size: u64) -> Anchor {
        Anchor::Content {
            crc32,
            size,
            sha1: None,
        }
    }

    fn 汉化裁决() -> Verdict {
        Verdict::now(
            内容锚(0x1234_5678, 40_976),
            Decision::Release(Facts {
                work: "重装机兵".to_string(),
                platform: Some("FC".to_string()),
                chinese: Some(ChineseMark::FanTranslated),
                team: Some("外星科技".to_string()),
                version: Some("v1.2".to_string()),
                ..Facts::default()
            }),
        )
    }

    #[test]
    fn 新建的库跑到最新一版迁移() {
        let store = Store::in_memory().expect("开得出来");
        let version: i64 = store
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("读得到");
        assert_eq!(u32::try_from(version).expect("装得下"), schema_version());
    }

    #[test]
    fn 库比程序新时如实拒绝而不是叫人删库() {
        // 裁决不可再生。「版本对不上就删掉重来」这条路在这份库上永远不许走。
        let store = Store::in_memory().expect("开得出来");
        store
            .conn
            .execute_batch("PRAGMA user_version = 99")
            .expect("改得动");
        let error = store.migrate().expect_err("该拒绝");
        let text = format!("{error}");
        assert!(text.contains("换用新版程序"), "{text}");
        assert!(!text.contains("删掉"), "绝不该提议删库：{text}");
    }

    #[test]
    fn 裁决存得进也取得回() {
        let mut store = Store::in_memory().expect("开得出来");
        let verdict = 汉化裁决();
        assert!(store.put(&verdict).expect("写得进"), "第一次是新增");
        let back = store
            .find(&verdict.anchor)
            .expect("读得到")
            .expect("有一条");
        assert_eq!(back.decision, verdict.decision);
        // 汉化组与版本是 ADR-0008 点名要沉淀的两样。
        let Decision::Release(facts) = &back.decision else {
            panic!("该是发行版裁决");
        };
        assert_eq!(facts.team.as_deref(), Some("外星科技"));
        assert_eq!(facts.version.as_deref(), Some("v1.2"));
    }

    #[test]
    fn 同一条锚上再裁一次是覆盖不是攒两条() {
        let mut store = Store::in_memory().expect("开得出来");
        store.put(&汉化裁决()).expect("写得进");
        let mut 改主意 = 汉化裁决();
        改主意.decision = Decision::Unknown;
        assert!(!store.put(&改主意).expect("写得进"), "第二次不是新增");
        assert_eq!(store.counts().expect("数得出").total, 1);
        let back = store.find(&改主意.anchor).expect("读得到").expect("有一条");
        assert_eq!(back.decision, Decision::Unknown);
    }

    #[test]
    fn 两种锚互不覆盖() {
        let mut store = Store::in_memory().expect("开得出来");
        store.put(&汉化裁决()).expect("写得进");
        store
            .put(&Verdict::now(
                Anchor::Path {
                    library: "主库".to_string(),
                    variant_key: "PSV/PCSG00718".to_string(),
                },
                Decision::NoRelease { work: None },
            ))
            .expect("写得进");
        let counts = store.counts().expect("数得出");
        assert_eq!((counts.content, counts.path), (1, 1));
    }

    #[test]
    fn 导出默认不带只在本机成立的路径锚() {
        let mut store = Store::in_memory().expect("开得出来");
        store.put(&汉化裁决()).expect("写得进");
        store
            .put(&Verdict::now(
                Anchor::Path {
                    library: "主库".to_string(),
                    variant_key: "PSV/x".to_string(),
                },
                Decision::Unknown,
            ))
            .expect("写得进");
        assert_eq!(store.export(false).expect("导得出").verdicts.len(), 1);
        assert_eq!(store.export(true).expect("导得出").verdicts.len(), 2);
    }

    #[test]
    fn 导出再导入是同一份() {
        let mut store = Store::in_memory().expect("开得出来");
        store.put(&汉化裁决()).expect("写得进");
        let text = serde_json::to_string(&store.export(false).expect("导得出")).expect("序列化");
        let mut 别人的 = Store::in_memory().expect("开得出来");
        let account = 别人的.import(&text).expect("收得下");
        assert_eq!((account.read, account.added, account.replaced), (1, 1, 0));
        let back = 别人的
            .find(&汉化裁决().anchor)
            .expect("读得到")
            .expect("有一条");
        assert_eq!(back.decision, 汉化裁决().decision);
    }

    #[test]
    fn 认不出格式的文件一律拒绝() {
        let mut store = Store::in_memory().expect("开得出来");
        assert!(store.import("{}").is_err());
        assert!(
            store
                .import(r#"{"format":"别的","version":1,"exported_at":0,"verdicts":[]}"#)
                .is_err()
        );
    }

    #[test]
    fn 快照只认这一份主库的路径锚() {
        let mut store = Store::in_memory().expect("开得出来");
        for library in ["甲", "乙"] {
            store
                .put(&Verdict::now(
                    Anchor::Path {
                        library: library.to_string(),
                        variant_key: "FC/x.zip".to_string(),
                    },
                    Decision::Unknown,
                ))
                .expect("写得进");
        }
        let index = Index::load(&store, "甲").expect("读得出");
        assert_eq!(index.len(), 1);
        assert!(index.by_path("FC/x.zip").is_some());
    }
}
