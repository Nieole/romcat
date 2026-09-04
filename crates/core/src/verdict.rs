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
//! 眼下两条：第 1 条建 `verdict` 表，第 2 条建 `match_verdict` 表（票 05 的**匹配裁决**）。
//! 加第二条时库还是空的，但那不改变纪律——**永远不要求删库**，中立库那条「版本一变就
//! 重建」的便宜路子在这份库上不许走。
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
//!
//! ## 两种裁决：**这是什么** 与 **这次匹配对不对**
//!
//! [`Verdict`] 说的是「世上这份内容是哪个作品的哪一次发行」——识别那一层的活。
//! [`MatchVerdict`] 说的是另一件事：**某个源在这份内容上撞出来的那一次匹配，人说对
//! 还是不对**（票 05）。
//!
//! 两者分开，是因为它们管的范围不同：
//!
//! - 一个变体只有一个「它是什么」，所以 [`Verdict`] 一条锚上只有一条。
//! - 一份内容上可以有好几次匹配（这个源撞出条目 4，那个源撞出别的），所以
//!   [`MatchVerdict`] 的键上还带着**哪个源**与**哪个条目号**。别的源在同名字段上说的话
//!   **一个字都不受影响**。
//!
//! **为什么粒度是「一次匹配」而不是「一个字段」**：中文离线源撞上一条条目之后一口气
//! 给出中文名、别名、类型、简介、开发商与发行商——它们**同生共死**，都来自同一个条目号。
//! 按字段裁，用户得为同一次误撞裁决五遍；按匹配裁，一条就够。
//!
//! 它与 [`Verdict`] 共用同一套两种锚，理由也是同一条：**内容锚换台机器、改过名字之后
//! 仍然认得出**，两块盘接同一台机器裁决一次两边都受益。

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
    // 2：**匹配裁决**（票 05）。一条说的是「某个源在这份内容上撞出来的那一次匹配，
    // 人说对还是不对」——不是某一个字段对不对。
    "\
-- 一条**匹配裁决**。锚与 `verdict` 那张表是同一套两种（见模块文档）。
CREATE TABLE IF NOT EXISTS match_verdict(
    id          INTEGER PRIMARY KEY,
    anchor      TEXT    NOT NULL,
    crc32       INTEGER,
    size        INTEGER,
    library     TEXT,
    variant_key TEXT,
    -- **哪个源撞的。** 有了它，这条裁决只管这一个源那一次匹配：别的源在同名字段上
    -- 说的话一个字都不受影响。
    source      TEXT    NOT NULL,
    -- **那个源那边的条目号。** 它是「同一次匹配」的唯一判据——同一次匹配带来的几个
    -- 字段散在两层锚点上，值里没有一样共通的东西，共通的只有这个号。
    entry       TEXT    NOT NULL,
    -- 1 = 就是这条；0 = 不是这条。
    accepted    INTEGER NOT NULL,
    note        TEXT,
    decided_at  INTEGER NOT NULL
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS match_verdict_content
    ON match_verdict(crc32, size, source, entry) WHERE anchor = '内容';
CREATE UNIQUE INDEX IF NOT EXISTS match_verdict_path
    ON match_verdict(library, variant_key, source, entry) WHERE anchor = '路径';
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

/// 一条**匹配裁决**：某个源在这份内容上撞出来的那一次匹配，人说对还是不对（票 05）。
///
/// ## 为什么它不是 [`Decision`] 的第四个变体
///
/// [`Decision`] 那三档回答的是同一个问题（「这份内容是什么」），一条锚上只允许有一个
/// 答案，所以它是个枚举、`put` 是覆盖。匹配裁决回答的是另一个问题，而且**同一份内容上
/// 可以有好几条**——这个源撞出条目 4、那个源撞出条目 9，各裁各的。混进同一张表就得在
/// 唯一索引上二选一：要么「一条锚一条」把好几个源挤成一条，要么放开唯一性让「这是什么」
/// 也能攒出两条打架的记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchVerdict {
    /// 钉在什么上。与 [`Verdict`] 同一套两种锚。
    pub anchor: Anchor,
    /// **哪个源撞的**（`scrape::priority` 里那个源名）。
    pub source: String,
    /// **那个源那边的条目号**。写成字符串而不是数字：不同的源编号方式不一样，
    /// 而这一层要做的只是「同一次匹配认得回来」，不必自己会算。
    pub entry: String,
    /// 人说的是「就是这条」还是「不是这条」。
    pub accepted: bool,
    /// 记一句为什么。
    pub note: Option<String>,
    /// 什么时候定的（UNIX 纪元起的秒）。
    pub decided_at: i64,
}

impl MatchVerdict {
    /// 立一条**现在**定下来的匹配裁决。
    #[must_use]
    pub fn now(anchor: Anchor, source: &str, entry: &str, accepted: bool) -> Self {
        Self {
            anchor,
            source: source.to_string(),
            entry: entry.to_string(),
            accepted,
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

    /// 存进库、也打给用户的那个词。
    #[must_use]
    pub fn label(&self) -> &'static str {
        if self.accepted {
            "就是这条"
        } else {
            "不是这条"
        }
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
    /// **匹配裁决**一共几条（票 05）。与上面那几个数不重叠——它们是两张表。
    pub matches: u64,
    /// 其中说「就是这条」的几条。
    pub matches_accepted: u64,
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
            matches: one("SELECT COUNT(*) FROM match_verdict")?,
            matches_accepted: one("SELECT COUNT(*) FROM match_verdict WHERE accepted = 1")?,
        })
    }

    /// 记下（或改掉）一条**匹配裁决**。返回它是不是**新**的一条。
    ///
    /// 同一条锚、同一个源、同一个条目号上再裁一次是**覆盖**：人从「不是这条」改成
    /// 「就是这条」，改的就该是那一条，而不是攒出两条互相打架的记录。
    ///
    /// **不同的条目号各算一条**：一份内容上撞过条目 4 也撞过条目 9 时，「4 不对」与
    /// 「9 对」是两句不同的话，都要留着——把它们挤成一条，改一次匹配参数就分不清人
    /// 到底否定过哪一个了。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put_match(&mut self, verdict: &MatchVerdict) -> Result<bool, VerdictError> {
        let existed = self
            .find_match(&verdict.anchor, &verdict.source, &verdict.entry)?
            .is_some();
        let (crc32, size, library, variant_key) = match_columns(&verdict.anchor);
        self.remove_match(&verdict.anchor, &verdict.source, &verdict.entry)?;
        self.conn
            .execute(
                "INSERT INTO match_verdict(anchor, crc32, size, library, variant_key,
                     source, entry, accepted, note, decided_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    verdict.anchor.label(),
                    crc32,
                    size,
                    library,
                    variant_key,
                    verdict.source,
                    verdict.entry,
                    i64::from(verdict.accepted),
                    verdict.note,
                    verdict.decided_at,
                ],
            )
            .map_err(|source| self.err(source))?;
        Ok(!existed)
    }

    /// 忘掉一条匹配裁决。返回真的忘掉了没有。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn remove_match(
        &mut self,
        anchor: &Anchor,
        source: &str,
        entry: &str,
    ) -> Result<bool, VerdictError> {
        let changed = match anchor {
            Anchor::Content { crc32, size, .. } => self.conn.execute(
                "DELETE FROM match_verdict
                 WHERE anchor = ?1 AND crc32 = ?2 AND size = ?3 AND source = ?4 AND entry = ?5",
                params![
                    ANCHOR_CONTENT,
                    i64::from(*crc32),
                    i64::try_from(*size).unwrap_or(i64::MAX),
                    source,
                    entry
                ],
            ),
            Anchor::Path {
                library,
                variant_key,
            } => self.conn.execute(
                "DELETE FROM match_verdict
                 WHERE anchor = ?1 AND library = ?2 AND variant_key = ?3
                   AND source = ?4 AND entry = ?5",
                params![ANCHOR_PATH, library, variant_key, source, entry],
            ),
        };
        changed
            .map(|rows| rows > 0)
            .map_err(|source| self.err(source))
    }

    /// 查一条匹配裁决。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn find_match(
        &self,
        anchor: &Anchor,
        source: &str,
        entry: &str,
    ) -> Result<Option<MatchVerdict>, VerdictError> {
        let row = match anchor {
            Anchor::Content { crc32, size, .. } => self
                .conn
                .query_row(
                    &format!(
                        "{MATCH_SELECT} WHERE anchor = ?1 AND crc32 = ?2 AND size = ?3
                         AND source = ?4 AND entry = ?5"
                    ),
                    params![
                        ANCHOR_CONTENT,
                        i64::from(*crc32),
                        i64::try_from(*size).unwrap_or(i64::MAX),
                        source,
                        entry
                    ],
                    read_match_row,
                )
                .optional(),
            Anchor::Path {
                library,
                variant_key,
            } => self
                .conn
                .query_row(
                    &format!(
                        "{MATCH_SELECT} WHERE anchor = ?1 AND library = ?2 AND variant_key = ?3
                         AND source = ?4 AND entry = ?5"
                    ),
                    params![ANCHOR_PATH, library, variant_key, source, entry],
                    read_match_row,
                )
                .optional(),
        };
        row.map_err(|source| self.err(source))
    }

    /// 全部匹配裁决，按定下来的先后排。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn all_matches(&self) -> Result<Vec<MatchVerdict>, VerdictError> {
        let mut statement = self
            .conn
            .prepare(&format!("{MATCH_SELECT} ORDER BY decided_at, id"))
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], read_match_row)
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }
}

/// 一条匹配裁决那四列锚，两处共用。
fn match_columns(anchor: &Anchor) -> (Option<i64>, Option<i64>, Option<String>, Option<String>) {
    match anchor {
        Anchor::Content { crc32, size, .. } => (
            Some(i64::from(*crc32)),
            Some(i64::try_from(*size).unwrap_or(i64::MAX)),
            None,
            None,
        ),
        Anchor::Path {
            library,
            variant_key,
        } => (None, None, Some(library.clone()), Some(variant_key.clone())),
    }
}

/// 读一行匹配裁决时要的那一串列。
const MATCH_SELECT: &str = "SELECT anchor, crc32, size, library, variant_key, source, entry,
     accepted, note, decided_at FROM match_verdict";

fn read_match_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MatchVerdict> {
    let anchor: String = row.get(0)?;
    let anchor = if anchor == ANCHOR_PATH {
        Anchor::Path {
            library: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
            variant_key: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
        }
    } else {
        Anchor::Content {
            crc32: u32::try_from(row.get::<_, i64>(1).unwrap_or(0)).unwrap_or(0),
            size: u64::try_from(row.get::<_, i64>(2).unwrap_or(0)).unwrap_or(0),
            // 匹配裁决这一侧一条 SHA-1 都不记：它的锚是从中立库里零成本取来的那一份。
            sha1: None,
        }
    };
    Ok(MatchVerdict {
        anchor,
        source: row.get(5)?,
        entry: row.get(6)?,
        accepted: row.get::<_, i64>(7)? != 0,
        note: row.get(8)?,
        decided_at: row.get(9)?,
    })
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

/// 一趟刮削拿在手里的**匹配裁决**快照（票 05）。
///
/// 与 [`Index`] 同一个形状、同一条道理：一趟刮削要为几万个锚点各查一次，逐次开库查是把
/// 一件常数时间的事做成几万次 I/O。
///
/// **它按锚存，不按变体键存**——把它摊平到变体键上是另一件事，要中立库才做得到
/// （`scrape::zh::Rulings::resolve`）：内容锚说的是「世上这份内容」，而「本机哪个变体
/// 装着这份内容」只有中立库答得出。分成两步，是为了让「换台机器仍然认得出」这条性质
/// 留在锚这一侧，不被本机的路径吃掉。
#[derive(Debug, Default)]
pub struct MatchIndex {
    content: BTreeMap<(u32, u64), Vec<MatchVerdict>>,
    path: BTreeMap<String, Vec<MatchVerdict>>,
}

impl MatchIndex {
    /// 一条都没有的空快照。没有沉淀库时用它，刮削照跑不误。
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// 把一份沉淀库里的匹配裁决整个读进来。
    ///
    /// `library` 是这一趟对着的主库名，**只有它的路径锚算数**（同 [`Index::load`]）。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn load(store: &Store, library: &str) -> Result<Self, VerdictError> {
        let mut index = Self::default();
        for verdict in store.all_matches()? {
            match &verdict.anchor {
                Anchor::Content { crc32, size, .. } => {
                    index.content.entry((*crc32, *size)).or_default().push(verdict);
                }
                Anchor::Path {
                    library: owner,
                    variant_key,
                } if owner == library => {
                    let key = variant_key.clone();
                    index.path.entry(key).or_default().push(verdict);
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
        self.content.values().map(Vec::len).sum::<usize>()
            + self.path.values().map(Vec::len).sum::<usize>()
    }

    /// 一条条走过全部**内容锚**上的匹配裁决：`((CRC-32, 大小), 那一批)`。
    pub fn by_content(&self) -> impl Iterator<Item = (&(u32, u64), &[MatchVerdict])> {
        self.content.iter().map(|(key, list)| (key, list.as_slice()))
    }

    /// 一条条走过全部**路径锚**上的匹配裁决：`(变体的键, 那一批)`。
    pub fn by_path(&self) -> impl Iterator<Item = (&str, &[MatchVerdict])> {
        self.path
            .iter()
            .map(|(key, list)| (key.as_str(), list.as_slice()))
    }
}

/// 导出文件的格式名。导入时认它，认不出就拒绝——**读错一份别人的裁决比读不了更糟**。
pub const EXPORT_FORMAT: &str = "romcat-沉淀库";

/// 导出文件的版本，**本程序认得到第几版**。
///
/// **票 05 从 1 涨到 2**：文件里多了**匹配裁决**那一批。往前兼容（第 1 版的文件照读，
/// 只是那一批是空的），往后如实拒绝——一份第 2 版的文件里可能装着老程序读不出来的
/// 匹配裁决，静静地丢掉它们比读不了更糟。
///
/// **导出时写的不一定是这个数**：见 [`Export::version_for`]。
pub const EXPORT_VERSION: u32 = 2;

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
    /// 全部**匹配裁决**（票 05）。第 1 版的文件里没有这一栏，读回来就是空的。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matches: Vec<MatchRow>,
}

impl Export {
    /// 一份导出文件**该盖第几版**。
    ///
    /// 一条匹配裁决都没有时盖 **1**，有才盖 [`EXPORT_VERSION`]。理由是版本号在这个格式里
    /// 的唯一作用是**那道往后拒绝的闸**（`version > EXPORT_VERSION` 就不收）：无条件盖 2
    /// 的话，升级之后导出的**每一份**文件——哪怕内容与第 1 版一模一样——都会被老版本的
    /// 程序拒收，而它其实一个字都读得懂。**装着新东西的才该拦下，空的不该。**
    #[must_use]
    pub fn version_for(matches: &[MatchRow]) -> u32 {
        if matches.is_empty() { 1 } else { EXPORT_VERSION }
    }
}

/// 导出文件里的一条**匹配裁决**。
///
/// CRC-32 与 [`Row`] 一样写成八位十六进制字符串，理由也一样：人拿它去 grep 一份 DAT 时
/// 不必先换算。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MatchRow {
    /// CRC-32，八位十六进制；路径锚的那些没有。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub crc32: Option<String>,
    /// 未压缩大小。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub size: Option<u64>,
    /// 路径锚：哪份主库。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub library: Option<String>,
    /// 路径锚：变体的键。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub variant_key: Option<String>,
    /// 哪个源撞的。
    pub source: String,
    /// 那个源那边的条目号。
    pub entry: String,
    /// 就是这条（真）还是不是这条（假）。
    pub accepted: bool,
    /// 记的那一句。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub note: Option<String>,
    /// 定下来的时刻。
    pub decided_at: i64,
}

impl MatchRow {
    /// 把一条匹配裁决折成导出行。
    #[must_use]
    pub fn of(verdict: &MatchVerdict) -> Self {
        let mut row = Self {
            source: verdict.source.clone(),
            entry: verdict.entry.clone(),
            accepted: verdict.accepted,
            note: verdict.note.clone(),
            decided_at: verdict.decided_at,
            ..Self::default()
        };
        match &verdict.anchor {
            Anchor::Content { crc32, size, .. } => {
                row.crc32 = Some(format!("{crc32:08X}"));
                row.size = Some(*size);
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

    /// 从导出行认回一条匹配裁决；锚认不出来时是 `None`。
    #[must_use]
    pub fn into_match(self) -> Option<MatchVerdict> {
        let anchor = match (&self.crc32, self.size, &self.variant_key) {
            (Some(crc32), Some(size), _) => Anchor::Content {
                crc32: u32::from_str_radix(crc32.trim(), 16).ok()?,
                size,
                sha1: None,
            },
            (_, _, Some(variant_key)) => Anchor::Path {
                library: self.library.clone()?,
                variant_key: variant_key.clone(),
            },
            _ => return None,
        };
        Some(MatchVerdict {
            anchor,
            source: self.source,
            entry: self.entry,
            accepted: self.accepted,
            note: self.note,
            decided_at: self.decided_at,
        })
    }
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
    /// 认不出锚、丢掉了几条。**两张表合起来数**——裁决与匹配裁决都算在这一格里。
    pub unreadable: u64,
    /// 文件里有几条**匹配裁决**。
    pub matches_read: u64,
    /// 收下了几条匹配裁决（新增加盖掉，合起来数）。
    pub matches_taken: u64,
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
        // **匹配裁决走同一道闸**：只在本机成立的那些默认不带出去，理由与上面那一批
        // 一模一样——带给别人只是噪音，还顺带把自己的目录结构交出去了。
        let matches: Vec<MatchRow> = self
            .all_matches()?
            .iter()
            .filter(|verdict| include_path || verdict.anchor.is_shareable())
            .map(MatchRow::of)
            .collect();
        Ok(Export {
            format: EXPORT_FORMAT.to_string(),
            version: Export::version_for(&matches),
            exported_at: now_secs(),
            verdicts,
            matches,
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
        account.matches_read = u64::try_from(export.matches.len()).unwrap_or(u64::MAX);
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
        for row in export.matches {
            match row.into_match() {
                Some(verdict) => {
                    self.put_match(&verdict)?;
                    account.matches_taken += 1;
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
    fn 匹配裁决存得进也取得回() {
        let mut store = Store::in_memory().expect("开得出来");
        let 裁决 = MatchVerdict::now(内容锚(0xAAAA_BBBB, 4_096), "中文离线源", "12345", false)
            .with_note(Some("撞成别的游戏了".to_string()));
        assert!(store.put_match(&裁决).expect("写得进"), "第一次是新增");
        let back = store
            .find_match(&裁决.anchor, "中文离线源", "12345")
            .expect("读得到")
            .expect("有一条");
        assert_eq!(back, 裁决);
        assert_eq!(back.label(), "不是这条");
    }

    #[test]
    fn 同一条条目上再裁一次是覆盖而不同条目各算一条() {
        // 「4 不对」与「9 就是它」是两句不同的话，两句都要留着——挤成一条，
        // 改一次匹配参数就分不清人到底否定过哪一个了。
        let mut store = Store::in_memory().expect("开得出来");
        let 锚 = 内容锚(0x1111_2222, 512);
        for (entry, accepted) in [("4", false), ("9", true)] {
            store
                .put_match(&MatchVerdict::now(锚.clone(), "中文离线源", entry, accepted))
                .expect("写得进");
        }
        // 同一条条目上改主意：覆盖，不是攒第二条。
        assert!(
            !store
                .put_match(&MatchVerdict::now(锚.clone(), "中文离线源", "4", true))
                .expect("写得进"),
            "第二次不是新增"
        );
        let counts = store.counts().expect("数得出");
        assert_eq!((counts.matches, counts.matches_accepted), (2, 2));
        // **两张表互不干扰**：匹配裁决一条都不算进「这是什么」那几个数里。
        assert_eq!(counts.total, 0);
    }

    #[test]
    fn 同一条条目号在两个源下各算一条() {
        // 别的源撞出来的那一次匹配**不受这条裁决影响**——键上带着源名就是为了这个。
        let mut store = Store::in_memory().expect("开得出来");
        let 锚 = 内容锚(0x3333_4444, 1_024);
        for source in ["中文离线源", "另一家中文源"] {
            store
                .put_match(&MatchVerdict::now(锚.clone(), source, "7", false))
                .expect("写得进");
        }
        assert_eq!(store.counts().expect("数得出").matches, 2);
        assert!(
            store
                .find_match(&锚, "另一家中文源", "7")
                .expect("读得到")
                .is_some()
        );
    }

    #[test]
    fn 匹配裁决跟着导出再导入是同一份() {
        let mut store = Store::in_memory().expect("开得出来");
        store.put(&汉化裁决()).expect("写得进");
        store
            .put_match(&MatchVerdict::now(
                内容锚(0x5555_6666, 2_048),
                "中文离线源",
                "12345",
                true,
            ))
            .expect("写得进");
        // 只在本机成立的那一条**默认不带出去**，与「这是什么」那一批同一道闸。
        store
            .put_match(&MatchVerdict::now(
                Anchor::Path {
                    library: "主库".to_string(),
                    variant_key: "FC/x.zip".to_string(),
                },
                "中文离线源",
                "9",
                false,
            ))
            .expect("写得进");
        let 导出 = store.export(false).expect("导得出");
        assert_eq!(导出.matches.len(), 1, "路径锚那条不该带出去");
        let text = serde_json::to_string(&导出).expect("序列化");

        let mut 别人的 = Store::in_memory().expect("开得出来");
        let account = 别人的.import(&text).expect("收得下");
        assert_eq!((account.matches_read, account.matches_taken), (1, 1));
        let back = 别人的
            .find_match(&内容锚(0x5555_6666, 2_048), "中文离线源", "12345")
            .expect("读得到")
            .expect("有一条");
        assert!(back.accepted);
    }

    #[test]
    fn 一条匹配裁决都没有的导出文件还盖第一版() {
        // 版本号在这个格式里的唯一作用是那道**往后拒绝**的闸。无条件盖 2 的话，升级之后
        // 导出的每一份文件——哪怕内容与第 1 版一模一样——都会被老版本的程序拒收，
        // 而它其实一个字都读得懂。装着新东西的才该拦下，空的不该。
        let mut store = Store::in_memory().expect("开得出来");
        store.put(&汉化裁决()).expect("写得进");
        assert_eq!(store.export(false).expect("导得出").version, 1);
        store
            .put_match(&MatchVerdict::now(
                内容锚(0x9999_0000, 8),
                "中文离线源",
                "1",
                false,
            ))
            .expect("写得进");
        assert_eq!(store.export(false).expect("导得出").version, EXPORT_VERSION);
        // 只在本机成立的那条不带出去，于是**不带路径锚的那一份仍旧盖第 1 版**。
        let mut 只有路径锚 = Store::in_memory().expect("开得出来");
        只有路径锚
            .put_match(&MatchVerdict::now(
                Anchor::Path {
                    library: "主库".to_string(),
                    variant_key: "FC/x.zip".to_string(),
                },
                "中文离线源",
                "1",
                false,
            ))
            .expect("写得进");
        assert_eq!(只有路径锚.export(false).expect("导得出").version, 1);
        assert_eq!(只有路径锚.export(true).expect("导得出").version, EXPORT_VERSION);
    }

    #[test]
    fn 第一版的裁决文件照读只是没有匹配裁决() {
        // 往前兼容：老文件里没有 `matches` 那一栏，读回来是空的，不是读不动。
        let mut store = Store::in_memory().expect("开得出来");
        let text = r#"{"format":"romcat-沉淀库","version":1,"exported_at":0,
             "verdicts":[{"crc32":"12345678","size":40976,"kind":"认不出","decided_at":0}]}"#;
        let account = store.import(text).expect("收得下");
        assert_eq!((account.read, account.added, account.matches_read), (1, 1, 0));
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

    #[test]
    fn 匹配裁决的快照也只认这一份主库的路径锚() {
        let mut store = Store::in_memory().expect("开得出来");
        for library in ["甲", "乙"] {
            store
                .put_match(&MatchVerdict::now(
                    Anchor::Path {
                        library: library.to_string(),
                        variant_key: "FC/x.zip".to_string(),
                    },
                    "中文离线源",
                    "4",
                    false,
                ))
                .expect("写得进");
        }
        // 内容锚那一批**两台机器都算数**，那正是它存在的理由。
        store
            .put_match(&MatchVerdict::now(
                内容锚(0x7777_8888, 64),
                "中文离线源",
                "5",
                true,
            ))
            .expect("写得进");
        let index = MatchIndex::load(&store, "甲").expect("读得出");
        assert_eq!(index.len(), 2, "乙那一条路径锚不算数，内容锚那条算");
        assert_eq!(index.by_path().count(), 1);
        assert_eq!(index.by_content().count(), 1);
    }
}
