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
//! 眼下四条：第 1 条建 `verdict` 表，第 2 条建 `match_verdict` 表（票 05 的**匹配裁决**），
//! 第 3 条建 `verdict_batch` 与 `verdict_batch_row` 两张表（**批**，见下一节），
//! 第 4 条建 `collection_member` 表（**合集**与**收藏**，见再下一节）。
//! 加这几条时库还是空的，但那不改变纪律——**永远不要求删库**，中立库那条「版本一变就
//! 重建」的便宜路子在这份库上不许走。
//!
//! ## **批**：撤销认得住的那个粒度
//!
//! 一次 [`triage::apply`](crate::triage::apply) 落下的那些是一**批**。批本身记在这里而不是
//! 中立库里，理由只有一条，而且是硬的：**一批可能盖掉先前的裁决，而被盖掉的那一条
//! 除了这里没有第二份**。裁决不可再生，一份只有一处的东西不许住在可以删掉重建的库里。
//!
//! `verdict_batch_row` 因此为每条记两样：这一批**落下的那条**（`after`）与**它盖掉的那条**
//! （`before`，没盖掉就为空）。撤销时先核对「锚上眼下这一条还是我们当初落下的那条吗」
//! ——不是就一个字都不动（后来有人在同一条锚上重新裁过，那是别人的账），是就删掉它、
//! 再把 `before` 原样放回去。
//!
//! 中立库那一半的撤销原料**不在这里**：那是候选与结论，可再生，住在中立库自己的
//! `verdict_batch_shadow`（`catalog::identify`）。两半分开住，各按各的身份。
//!
//! ## **合集**与**收藏**：为什么它们也住这儿
//!
//! 一条**合集成员关系**（[`Membership`]）说的是「这份内容属于我起名叫某某的那一组」。
//! 收藏是其中名字定死的那一组（[`crate::collection::FAVORITE`]）——`CONTEXT.md` 的词条
//! 写着「合集是一组自己起名的，收藏是那个默认的一组」，所以这里只有一张表。
//!
//! 它住这儿的理由与裁决同一条，而且更硬：**这是用户亲手点的，一份只有一处，
//! 删掉就没了**。中立库整份可再生（结构一变就让人删掉重扫），把收藏放进去等于说
//! 「下一次改结构时你那几百颗星归零」。
//!
//! 中立库里 `collection` / `collection_variant` 那两张表因此是**这份库的投影**——与
//! 那几行 `origin = 裁决` 的作品和发行版一模一样的身份：识别跑完照沉淀库重建一遍，
//! 结果一致（[`crate::collection::project`]）。
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
//! ## 换键那一次，路径锚身上发生了什么
//!
//! 中立库结构版本 6 把**变体的键**从「相对主库根的路径」换成「**根名** + 相对那个根」
//! （主库变成一组根，`catalog::SCHEMA_VERSION` 的第 6 段）。中立库整份可再生，删掉重扫
//! 就是了；**这份库不可再生，一条都不许删**。于是这次换键在这里的落点是：
//!
//! - 换键之前落下的**路径锚**，`variant_key` 里存的是旧形状，**从此撞不上**。
//!   那些行**原样留着**——不删、不改、不猜着往前挪一格。猜是这里最不该做的事：
//!   哪条旧键属于哪个根，这份库里没有任何判据说得出来（它只记着主库叫什么名字，
//!   记不着那时候只有一个根）。
//! - **内容锚一条都不受影响。** 它钉的是那份字节，与文件躺在哪块盘上、叫什么名字无关
//!   ——那正是它存在的理由，也正是这次换键在这里几乎不疼的原因。
//! - **不为它加一条迁移。** 迁移只许往后追加，而这件事没有一条正确的 SQL 写得出来。
//!
//! 换键这一刻真库里 0 条裁决、0 条人工纠正（`.scratch/gui-redesign/spec.md`），
//! 所以这笔账的实际金额是零；记在这儿是为了让日后看见这几行「撞不上的旧锚」的人
//! 知道它们是什么，而不是把它们当成脏数据清掉。
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

use std::collections::{BTreeMap, BTreeSet};
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

/// 撞上写锁时**等多久**（毫秒）。**与中立库同一个数**，理由也同源。
///
/// **不是调优，是两机流程（ADR-0018）的前提**：命令行与界面常常同时开着，两边都往
/// 这一份库里落裁决，撞上就报错的话，用户看见的是「沉淀库打不开：…
/// database is locked」。落一条裁决是几毫秒的事，十秒是很宽的余量。
///
/// **明写出来，是因为不写也有一个数，而那个数不是谁挑的**：`rusqlite` 的
/// `Connection::open` 自己塞了 5 秒（`inner_connection.rs` 里那句
/// `sqlite3_busy_timeout(db, 5000)`）。中立库那一侧早已把 10 秒明写出来并说清了理由
/// （`Catalog::open`）；同一台机器上一份库等 10 秒、另一份等一个第三方库默认的、
/// 没有人裁过的 5 秒，说不出道理，而且它改版就没了。
///
/// **这份取 10 秒而三份镜像取 3 秒**，差别只在等在哪条线程上：镜像库的 `open` 有一路
/// 是 `sources::survey`，眼下跑在画帧线程上，等 10 秒等于把界面挂死；这一份是
/// `Site::open` 一次开好一直用着，不在画帧线程上。
const BUSY_TIMEOUT_MS: u32 = 10_000;

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
    // 3：**批**（票 gui-redesign/08）。一次批量裁决落下的那些记成一批，**撤销以它为粒度**。
    "\
-- 一**批**裁决。`undone_at` 非空就是已经撤过了——撤掉的批不删行：撤销本身要撤得回来，
-- 而把它放回去要的正是这几行（`after`）。
CREATE TABLE IF NOT EXISTS verdict_batch(
    id         INTEGER PRIMARY KEY,
    -- 这一批落在哪份主库上。路径锚只在本机的这一份主库里成立，列批时按它筛。
    library    TEXT    NOT NULL,
    -- 裁成什么，给人看的一句（`triage::Decide::summary`）。
    summary    TEXT    NOT NULL,
    note       TEXT,
    decided_at INTEGER NOT NULL,
    undone_at  INTEGER
) STRICT;

CREATE INDEX IF NOT EXISTS verdict_batch_library ON verdict_batch(library);

-- 一批里的一条：钉在哪条锚上、**落下的那条**长什么样、**它盖掉的那条**长什么样。
--
-- `after` 与 `before` 存的是导出格式里那一行（`Row`）的 JSON——同一个形状读写两处，
-- 不为撤销另造一份序列化。`before` 为空表示那条锚上当时一条裁决都没有。
CREATE TABLE IF NOT EXISTS verdict_batch_row(
    batch       INTEGER NOT NULL REFERENCES verdict_batch(id),
    -- 哪个变体。中立库那一半按它找回快照。
    variant_key TEXT    NOT NULL,
    -- 内容锚钉在这个变体的哪一份内容上。**重做要它**：把裁决重新投影回中立库时，
    -- 那条候选要说得出「是包里的哪一个」（`identify::Projector::project`）。
    member      TEXT    NOT NULL,
    inner       TEXT    NOT NULL,
    -- **锚不另开几列**：它已经在 `after` 里了（`Row` 那个形状连锚一起存）。
    -- 另存一份的话，同一条锚在一行里有两个说法，而它们迟早会各说各的。
    after       TEXT    NOT NULL,
    before      TEXT,
    PRIMARY KEY (batch, variant_key)
) STRICT;
",
    // 4：**合集成员关系**（票 gui-redesign/06）。**收藏是名字定死的那一组**，
    // 与自建合集同一张表——`CONTEXT.md` 说的「合集是一组自己起名的，收藏是那个默认的
    // 一组」，两张表会让「按合集筛」与「按收藏筛」变成两套算法。
    "\
-- 一条**合集成员关系**：这份内容属于叫这个名字的那一组。锚与 `verdict` 那张表是
-- 同一套两种（见模块文档），理由也同一条：**内容锚换台机器、改过名字之后仍然认得出**。
--
-- **没有单独的「合集」表**：一个合集就是它那些成员关系，成员一条不剩它就没了。
-- 立一张空合集表的话，屏上「合集 3 个」里可能有两个一条东西都选不出来，而这一票的
-- 正题恰恰是把「合集 0 个」那句话收掉（挂账 D74）。
CREATE TABLE IF NOT EXISTS collection_member(
    id          INTEGER PRIMARY KEY,
    -- 合集的名字。原样存人打的那串字（前后空白由上一层修掉）。
    name        TEXT    NOT NULL,
    anchor      TEXT    NOT NULL,
    -- 内容锚。
    crc32       INTEGER,
    size        INTEGER,
    sha1        TEXT,
    -- 路径锚：哪份主库的哪个变体。只在本机成立，**挪了位置会飘**。
    library     TEXT,
    variant_key TEXT,
    added_at    INTEGER NOT NULL
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS collection_member_content
    ON collection_member(name, crc32, size) WHERE anchor = '内容';
CREATE UNIQUE INDEX IF NOT EXISTS collection_member_path
    ON collection_member(name, library, variant_key) WHERE anchor = '路径';
CREATE INDEX IF NOT EXISTS collection_member_name ON collection_member(name);
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
        /// 变体的键：**根名** + 相对那个根的路径（`path::library_key`）。
        ///
        /// 换过一次形状（见模块文档「换键那一次」）：结构版本 6 之前落下的锚里存的是
        /// 不带根名的旧形状，撞不上，但一条都没删。
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
        Self::at(anchor, decision, now_secs())
    }

    /// 立一条**在这个时刻**定下来的裁决。
    ///
    /// **一批裁决共用一个时刻走的是它**（`triage::plan_each`）：一批是一次落下，
    /// 也就是一个时刻。逐条各取一次的话，一批三千条会跨过秒界，而同一条锚上的几份
    /// **重复拷贝**本该落成同一条裁决——差一秒就成了两条，撤销那一侧再也认不出
    /// 「锚上眼下这条是不是这一批自己落下的」。
    #[must_use]
    pub fn at(anchor: Anchor, decision: Decision, decided_at: i64) -> Self {
        Self {
            anchor,
            decision,
            note: None,
            decided_at,
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

/// 一**批**裁决：一次批量裁决落下的那些。**撤销以它为粒度**。
///
/// 批不是「几条裁决凑在一起」那么简单——它还记着**每条盖掉了什么**，而被盖掉的那条
/// 除了这里没有第二份（见模块文档）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    /// 编号。命令行与界面拿它点名要撤哪一批。
    pub id: i64,
    /// 落在哪份主库上。
    pub library: String,
    /// 裁成什么，给人看的一句。
    pub summary: String,
    /// 记的那一句为什么。
    pub note: Option<String>,
    /// 落下的时刻（UNIX 纪元起的秒）。
    pub decided_at: i64,
    /// 撤掉的时刻；没撤过就是 `None`。
    pub undone_at: Option<i64>,
    /// 这一批有几条。
    pub rows: u64,
}

impl Batch {
    /// 这一批眼下是撤掉的状态吗。
    #[must_use]
    pub fn undone(&self) -> bool {
        self.undone_at.is_some()
    }
}

/// 一批里的一条。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchRow {
    /// 哪个变体。中立库那一半按它找回快照。
    pub variant_key: String,
    /// 内容锚钉在这个变体的哪一份内容上：成员的键。
    pub member: String,
    /// 容器内部路径；裸文件是空串。
    pub inner: String,
    /// **这一批落下的那条**。撤销前先核对锚上眼下是不是还是它。
    pub after: Verdict,
    /// **它盖掉的那条**；那条锚上当时没有裁决就是 `None`。
    pub before: Option<Verdict>,
}

impl BatchRow {
    /// 这一条钉在什么上。
    #[must_use]
    pub fn anchor(&self) -> &Anchor {
        &self.after.anchor
    }
}

/// 一条**合集成员关系**：这份内容属于叫这个名字的那一组（票 `gui-redesign/06`）。
///
/// **收藏是名字定死的那一组**（[`crate::collection::FAVORITE`]），不另立一种。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Membership {
    /// 哪个合集。
    pub name: String,
    /// 钉在什么上。**内容锚认得出改名与挪目录，路径锚不认得**。
    pub anchor: Anchor,
    /// 什么时候放进去的（Unix 秒）。
    pub added_at: i64,
}

impl Membership {
    /// 现在把这条锚放进这个合集。
    #[must_use]
    pub fn now(name: &str, anchor: Anchor) -> Self {
        Self {
            name: name.to_string(),
            anchor,
            added_at: now_secs(),
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
        // **这条余量要排在最前面**，后面每一句才等得起（为什么是 10 秒见
        // `BUSY_TIMEOUT_MS`）。
        self.conn
            .execute_batch(&format!("PRAGMA busy_timeout = {BUSY_TIMEOUT_MS};"))
            .map_err(|source| self.err(source))?;
        // **偏偏转日志模式这一句不认忙等待**：它要的是独占，走的不是忙等待那条路
        // ——实测把余量设成 300 毫秒、另一份连接占着写锁，这一句 337 微秒就当场
        // `SQLITE_BUSY`，而建表与写 `user_version` 都老老实实等满了 300 毫秒。
        // **撞上就放过**——能撞上只有一种情形：另一份连接正在建这同一份空库；而 WAL
        // 记在库文件头里，谁转成了所有连接都按 WAL 走，这一份不必去争。已经是 WAL 的
        // 库上它本来就是空操作（实测 2.4 微秒，写锁占着也不报忙）。
        if let Err(source) = self.conn.execute_batch("PRAGMA journal_mode = WAL;")
            && source.sqlite_error_code() != Some(rusqlite::ErrorCode::DatabaseBusy)
        {
            return Err(self.err(source));
        }
        self.conn
            .execute_batch("PRAGMA synchronous = NORMAL;")
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
        // **版本号只在真的变了才写。** `PRAGMA user_version = N` 是一次**写事务**，
        // **同值重写照样要拿写锁**（实测：另一份连接占着写锁时，同值那一句等满了余量
        // 才报忙）。而它原来每次开库都跑一遍——于是「开一下当前版本的沉淀库」这件本该
        // 只读的事成了一次写：命令行那一侧开个库，就跟界面正在落的那条裁决抢同一把锁，
        // 抢不过就是用户看见的那句「沉淀库打不开：… database is locked」。
        // **当前版本的库开起来该是一个字都不写**，那样两边根本不必相遇。
        if found == target {
            return Ok(());
        }
        Self::apply(&self.conn, found, MIGRATIONS, target).map_err(|source| self.err(source))
    }

    /// 把 `from` 之后那几条迁移与新版本号**一起**落下去。
    ///
    /// **同一个事务。** 中途断电要么整批迁移带着新版本号一起落下，要么一个字都没落下、
    /// 下次开库从 `from` 接着跑。分成两笔写会留下「表已经改了、版本号还写着旧的」那种
    /// 库——眼下几条都是 `CREATE TABLE IF NOT EXISTS`，重跑无害，但这份库**不可再生**，
    /// 纪律得在第一条会改已有表的迁移出现之前就立住。
    ///
    /// 走 `BEGIN IMMEDIATE` 而不是默认的延迟事务：延迟事务先拿读锁、写第一句时才想升级
    /// 成写锁，而这一升在 WAL 下撞上别的写者是**不走忙等待**的，上面那条余量就白设了。
    ///
    /// `migrations` 与 `target` 是参数而不是直接读常量，为的是测试能塞一条炸的进来，
    /// 把「半途炸了什么都不留下」真的走一遍。
    fn apply(
        conn: &Connection,
        from: u32,
        migrations: &[&str],
        target: u32,
    ) -> Result<(), rusqlite::Error> {
        let tx =
            rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
        for sql in &migrations[from as usize..] {
            tx.execute_batch(sql)?;
        }
        // `PRAGMA` 不吃占位符，而 `target` 是本程序自己的常量，不是外面来的数。
        tx.execute_batch(&format!("PRAGMA user_version = {target}"))?;
        tx.commit()
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

    /// 记下一**批**：这一批落下的那些，连各自盖掉的那条一起。返回这一批的编号。
    ///
    /// **落裁决与记批在同一个事务里**（调用方把两件事一起交过来）：批记下了而裁决没落，
    /// 或者反过来，都会让撤销这件事从一开始就说不准。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put_batch(
        &mut self,
        library: &str,
        summary: &str,
        note: Option<&str>,
        rows: &[BatchRow],
    ) -> Result<i64, VerdictError> {
        let path = self.path.clone();
        let to_err = |source| VerdictError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        let id = {
            tx.execute(
                "INSERT INTO verdict_batch(library, summary, note, decided_at)
                 VALUES(?1,?2,?3,?4)",
                params![library, summary, note, now_secs()],
            )
            .map_err(to_err)?;
            let id = tx.last_insert_rowid();
            let mut insert = tx
                .prepare(
                    "INSERT INTO verdict_batch_row(batch, variant_key, member, inner,
                         after, before)
                     VALUES(?1,?2,?3,?4,?5,?6)",
                )
                .map_err(to_err)?;
            for row in rows {
                insert
                    .execute(params![
                        id,
                        row.variant_key,
                        row.member,
                        row.inner,
                        encode(&row.after)?,
                        row.before.as_ref().map(encode).transpose()?,
                    ])
                    .map_err(to_err)?;
            }
            id
        };
        tx.commit().map_err(to_err)?;
        Ok(id)
    }

    /// 这份主库上的那些**批**，新的在前。`limit` 为 0 表示不限。
    ///
    /// 按主库筛，是因为**路径锚只在本机的这一份主库里成立**——把别的主库的批列出来，
    /// 撤起来一条也对不上。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn batches(&self, library: &str, limit: usize) -> Result<Vec<Batch>, VerdictError> {
        let mut sql = String::from(
            "SELECT b.id, b.library, b.summary, b.note, b.decided_at, b.undone_at,
                    (SELECT COUNT(*) FROM verdict_batch_row r WHERE r.batch = b.id)
             FROM verdict_batch b WHERE b.library = ?1 ORDER BY b.id DESC",
        );
        if limit > 0 {
            sql.push_str(&format!(" LIMIT {limit}"));
        }
        let mut statement = self.conn.prepare(&sql).map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![library], |row| {
                Ok(Batch {
                    id: row.get(0)?,
                    library: row.get(1)?,
                    summary: row.get(2)?,
                    note: row.get(3)?,
                    decided_at: row.get(4)?,
                    undone_at: row.get(5)?,
                    rows: u64::try_from(row.get::<_, i64>(6)?).unwrap_or(0),
                })
            })
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 点名要一**批**；没这一批就是 `None`。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn batch(&self, id: i64) -> Result<Option<Batch>, VerdictError> {
        self.conn
            .query_row(
                "SELECT b.id, b.library, b.summary, b.note, b.decided_at, b.undone_at,
                        (SELECT COUNT(*) FROM verdict_batch_row r WHERE r.batch = b.id)
                 FROM verdict_batch b WHERE b.id = ?1",
                params![id],
                |row| {
                    Ok(Batch {
                        id: row.get(0)?,
                        library: row.get(1)?,
                        summary: row.get(2)?,
                        note: row.get(3)?,
                        decided_at: row.get(4)?,
                        undone_at: row.get(5)?,
                        rows: u64::try_from(row.get::<_, i64>(6)?).unwrap_or(0),
                    })
                },
            )
            .optional()
            .map_err(|source| self.err(source))
    }

    /// 一批里的那些条，按变体的键排。
    ///
    /// **读不回来的行整条丢掉而不是报错**：`after` 是本程序自己写下的 JSON，读不回来
    /// 说明这一行已经不可信了，拿着半份数据去撤销比撤不了更糟。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn batch_rows(&self, id: i64) -> Result<Vec<BatchRow>, VerdictError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT variant_key, member, inner, after, before
                 FROM verdict_batch_row WHERE batch = ?1 ORDER BY variant_key",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })
            .map_err(|source| self.err(source))?;
        let mut out = Vec::new();
        for row in rows {
            let (variant_key, member, inner, after, before) =
                row.map_err(|source| self.err(source))?;
            let Some(after) = decode(&after) else {
                continue;
            };
            out.push(BatchRow {
                variant_key,
                member,
                inner,
                after,
                before: before.as_deref().and_then(decode),
            });
        }
        Ok(out)
    }

    /// 这一批之后落下、**眼下还在册**的那些批里，头一个碰过这些锚的是第几批，
    /// 连它在这些锚上占了几条。都没碰过就是 `None`。
    ///
    /// **撤销与放回都要先问它一句。** 批与批在同一条锚上是**叠着的**：后一批的
    /// [`BatchRow::before`] 里存着前一批落下的那条，撤后一批就会把它放回来。所以前一批
    /// 被后一批盖住时根本回不到「它落下之前」——硬撤的话它被标成已撤，而它的裁决
    /// 过一会儿又活了。
    ///
    /// 问的是**册子**而不是「锚上眼下那条长什么样」：两批落下的裁决**值可以一模一样**
    /// （同一秒、同一部作品），按值比对认不出这件事。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn batch_covering(
        &self,
        after: i64,
        anchors: &BTreeSet<Anchor>,
    ) -> Result<Option<(i64, u64)>, VerdictError> {
        if anchors.is_empty() {
            return Ok(None);
        }
        let mut statement = self
            .conn
            .prepare(
                "SELECT r.batch, r.after FROM verdict_batch_row r
                 JOIN verdict_batch b ON b.id = r.batch
                 WHERE r.batch > ?1 AND b.undone_at IS NULL
                 ORDER BY r.batch",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![after], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|source| self.err(source))?;
        let mut found: Option<(i64, u64)> = None;
        for row in rows {
            let (batch, blob) = row.map_err(|source| self.err(source))?;
            // 按批号排着，所以头一批的行是连在一起的：换了批号就已经数完了。
            if found.is_some_and(|(id, _)| id != batch) {
                break;
            }
            if !decode(&blob).is_some_and(|verdict| anchors.contains(&verdict.anchor)) {
                continue;
            }
            match &mut found {
                Some((_, count)) => *count += 1,
                None => found = Some((batch, 1)),
            }
        }
        Ok(found)
    }

    /// 把一批标成撤掉的（或者标回没撤）。
    ///
    /// **撤掉的批不删行**：撤销本身要撤得回来，而把它放回去要的正是那几行。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn mark_batch_undone(&mut self, id: i64, undone: bool) -> Result<(), VerdictError> {
        self.conn
            .execute(
                "UPDATE verdict_batch SET undone_at = ?2 WHERE id = ?1",
                params![id, undone.then(now_secs)],
            )
            .map(|_| ())
            .map_err(|source| self.err(source))
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

    // ── 合集与收藏：同一套成员关系（票 gui-redesign/06） ──────────────────────

    /// 把一批锚放进合集。返回其中**本来不在里面**的有几条。
    ///
    /// 同一条锚放两次是空操作而不是攒出两行：成员关系是个是非题，「在里面」没有第二种
    /// 程度。**一整批一个事务**：界面上「全选 → 收藏」一下就是四万多条，一条一个事务
    /// 等于四万多次提交（这份库是 WAL，那是四万多次写日志）。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn join(&mut self, memberships: &[Membership]) -> Result<usize, VerdictError> {
        let path = self.path.clone();
        let to_err = |source| VerdictError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        let mut changed = 0;
        {
            let mut insert = tx
                .prepare(
                    "INSERT INTO collection_member(name, anchor, crc32, size, sha1,
                         library, variant_key, added_at)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
                     ON CONFLICT DO NOTHING",
                )
                .map_err(to_err)?;
            for membership in memberships {
                let (crc32, size, library, variant_key) = match_columns(&membership.anchor);
                let sha1 = match &membership.anchor {
                    Anchor::Content { sha1, .. } => sha1.clone(),
                    Anchor::Path { .. } => None,
                };
                changed += insert
                    .execute(params![
                        membership.name,
                        membership.anchor.label(),
                        crc32,
                        size,
                        sha1,
                        library,
                        variant_key,
                        membership.added_at,
                    ])
                    .map_err(to_err)?;
            }
        }
        tx.commit().map_err(to_err)?;
        Ok(changed)
    }

    /// 把一批锚从一个合集里拿出来。返回真的拿出来了几条。
    ///
    /// **一整批一个事务**，理由同 [`Self::join`]。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn leave(&mut self, name: &str, anchors: &[Anchor]) -> Result<usize, VerdictError> {
        let path = self.path.clone();
        let to_err = |source| VerdictError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        let mut changed = 0;
        {
            let mut by_content = tx
                .prepare(
                    "DELETE FROM collection_member
                     WHERE name = ?1 AND anchor = ?2 AND crc32 = ?3 AND size = ?4",
                )
                .map_err(to_err)?;
            let mut by_path = tx
                .prepare(
                    "DELETE FROM collection_member
                     WHERE name = ?1 AND anchor = ?2 AND library = ?3 AND variant_key = ?4",
                )
                .map_err(to_err)?;
            for anchor in anchors {
                changed += match anchor {
                    Anchor::Content { crc32, size, .. } => by_content.execute(params![
                        name,
                        ANCHOR_CONTENT,
                        i64::from(*crc32),
                        size_column(*size)
                    ]),
                    Anchor::Path {
                        library,
                        variant_key,
                    } => by_path.execute(params![name, ANCHOR_PATH, library, variant_key]),
                }
                .map_err(to_err)?;
            }
        }
        tx.commit().map_err(to_err)?;
        Ok(changed)
    }

    /// 这条锚在哪几个合集里，按名字排。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn joined(&self, anchor: &Anchor) -> Result<Vec<String>, VerdictError> {
        let mut statement = self
            .conn
            .prepare(match anchor {
                Anchor::Content { .. } => {
                    "SELECT name FROM collection_member
                     WHERE anchor = ?1 AND crc32 = ?2 AND size = ?3 ORDER BY name"
                }
                Anchor::Path { .. } => {
                    "SELECT name FROM collection_member
                     WHERE anchor = ?1 AND library = ?2 AND variant_key = ?3 ORDER BY name"
                }
            })
            .map_err(|source| self.err(source))?;
        let read = |row: &rusqlite::Row<'_>| row.get::<_, String>(0);
        let rows = match anchor {
            Anchor::Content { crc32, size, .. } => statement.query_map(
                params![ANCHOR_CONTENT, i64::from(*crc32), size_column(*size)],
                read,
            ),
            Anchor::Path {
                library,
                variant_key,
            } => statement.query_map(params![ANCHOR_PATH, library, variant_key], read),
        }
        .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 整份成员关系。**投影**要它（[`crate::collection::project`]）。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn memberships(&self) -> Result<Vec<Membership>, VerdictError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT name, anchor, crc32, size, sha1, library, variant_key, added_at
                 FROM collection_member ORDER BY name, added_at, id",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                let anchor: String = row.get(1)?;
                let anchor = if anchor == ANCHOR_CONTENT {
                    Anchor::Content {
                        crc32: u32::try_from(row.get::<_, i64>(2)?).unwrap_or(0),
                        size: u64::try_from(row.get::<_, i64>(3)?).unwrap_or(0),
                        sha1: row.get(4)?,
                    }
                } else {
                    Anchor::Path {
                        library: row.get(5)?,
                        variant_key: row.get(6)?,
                    }
                };
                Ok(Membership {
                    name: row.get(0)?,
                    anchor,
                    added_at: row.get(7)?,
                })
            })
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 库里有哪几个合集，各有多少条成员关系。按条数从多到少、同数按名字排。
    ///
    /// **数的是成员关系不是变体**：一条内容锚可能在本机对应好几个变体（同一份内容
    /// 存了两处），而这个数说的是「用户点过多少下」。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn collections(&self) -> Result<Vec<(String, u64)>, VerdictError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT name, COUNT(*) FROM collection_member
                 GROUP BY name ORDER BY COUNT(*) DESC, name",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                ))
            })
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }
}

/// 大小落进 SQLite 那一列时的样子。装不下就钉在上限——**这条路走不到**：
/// `i64::MAX` 字节是 8 EiB。
fn size_column(size: u64) -> i64 {
    i64::try_from(size).unwrap_or(i64::MAX)
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
///
/// **[`Membership`] 也在这份快照里**，不是因为识别要拿它撞什么——它一次都不参与识别。
/// 它跟着走，是因为识别那一趟**末尾**要照沉淀库把中立库里的合集重建一遍
/// （[`crate::collection::project`]），而那与作品、发行版那几行 `origin = 裁决` 是同一件事：
/// 中立库里那些行是这份库的投影。同一份快照拿着走，就不必为它再开一次库，
/// 也不会出现「裁决照的是这一刻、合集照的是另一刻」。
#[derive(Debug, Default)]
pub struct Index {
    content: BTreeMap<(u32, u64), Verdict>,
    path: BTreeMap<String, Verdict>,
    memberships: Vec<Membership>,
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
        index.memberships = store
            .memberships()?
            .into_iter()
            .filter(|one| match &one.anchor {
                Anchor::Content { .. } => true,
                Anchor::Path { library: owner, .. } => owner == library,
            })
            .collect();
        Ok(index)
    }

    /// 一条**裁决**都没有吗。
    ///
    /// **只算裁决，不算合集成员关系**：这个数是拿来说「这份库里人裁过多少」的
    /// （报告里那一行），而合集是另一回事。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.content.is_empty() && self.path.is_empty()
    }

    /// 一共几条**裁决**（只算这一趟用得上的）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.content.len() + self.path.len()
    }

    /// 这一趟用得上的**合集成员关系**：内容锚全要，路径锚只要这份主库的。
    #[must_use]
    pub fn memberships(&self) -> &[Membership] {
        &self.memberships
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
                    index
                        .content
                        .entry((*crc32, *size))
                        .or_default()
                        .push(verdict);
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
        self.content
            .iter()
            .map(|(key, list)| (key, list.as_slice()))
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
        if matches.is_empty() {
            1
        } else {
            EXPORT_VERSION
        }
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

/// 把一条裁决折成**批**里存的那份 JSON。
///
/// 用的是导出格式里那一行（[`Row`]）：同一个形状读写两处，不为撤销另造一份序列化——
/// 造第二份的话，哪天导出格式加了一栏而这一处忘了跟，撤销会把那一栏悄悄丢掉。
fn encode(verdict: &Verdict) -> Result<String, VerdictError> {
    serde_json::to_string(&Row::of(verdict))
        .map_err(|error| VerdictError::Format(format!("这条裁决记不进批里：{error}")))
}

/// 从批里那份 JSON 认回一条裁决；读不回来就是 `None`。
fn decode(text: &str) -> Option<Verdict> {
    serde_json::from_str::<Row>(text)
        .ok()
        .and_then(Row::into_verdict)
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
    fn 第一版的老库带着裁决升上来_一条都不丢() {
        // **这份库不可再生**，所以「迁移只许往后追加」不能只写在模块文档里——要有一条
        // 测试真的走一遍「第 1 版的库 → 最新版」，并且看着老裁决原样还在。
        // 眼下第 2、3、4 条迁移都是 `CREATE TABLE IF NOT EXISTS`，本来就动不了老数据；
        // 这一条钉的是**将来**：等哪天有一条迁移改的是已有的表，它会先炸，
        // 而不是等用户丢了裁决才发现。
        let conn = Connection::open_in_memory().expect("开得出来");
        conn.execute_batch(MIGRATIONS[0]).expect("建得出第一版");
        conn.execute_batch("PRAGMA user_version = 1")
            .expect("盖得上第一版的版本号");
        let mut store = Store {
            conn,
            path: "（内存）".to_string(),
        };
        let verdict = 汉化裁决();
        store.put(&verdict).expect("第一版里就存得进");

        store.migrate().expect("升得上来");

        let version: i64 = store
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("读得到");
        assert_eq!(
            u32::try_from(version).expect("装得下"),
            schema_version(),
            "升到最新一版"
        );
        let back = store
            .find(&verdict.anchor)
            .expect("读得到")
            .expect("老裁决还在");
        assert_eq!(back.decision, verdict.decision, "一个字都没变");
    }

    #[test]
    fn 第三版的老库带着裁决和批升上来_一条都不丢() {
        // 与上一条同一个形状、同一条理由，钉的是**第 4 条迁移**（合集，票
        // `gui-redesign/06`）。**每加一条迁移就照这个形状补一条**：这份库不可再生，
        // 「升上来之后老东西还在」是它唯一不能出错的地方。
        //
        // 从第 3 版起头而不是第 1 版，是因为第 1 版那条已经把「1 → 最新」走过一遍了；
        // 这一条要看的是**跨过第 4 条那一步**——升上来之后老库里的裁决与批照旧读得回来，
        // 而新的那张表也真的建出来了。
        let conn = Connection::open_in_memory().expect("开得出来");
        for sql in &MIGRATIONS[..3] {
            conn.execute_batch(sql).expect("建得出第三版");
        }
        conn.execute_batch("PRAGMA user_version = 3")
            .expect("盖得上第三版的版本号");
        let mut store = Store {
            conn,
            path: "（内存）".to_string(),
        };
        let verdict = 汉化裁决();
        store.put(&verdict).expect("第三版里就存得进");
        let batch = store
            .put_batch(
                "小库",
                "作品《魂斗罗》",
                None,
                &[BatchRow {
                    variant_key: "库/FC/某.zip".to_string(),
                    member: "库/FC/某.zip".to_string(),
                    inner: "rom.nes".to_string(),
                    after: verdict.clone(),
                    before: None,
                }],
            )
            .expect("第三版里就记得下批");

        store.migrate().expect("升得上来");

        let version: i64 = store
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("读得到");
        assert_eq!(
            u32::try_from(version).expect("装得下"),
            schema_version(),
            "升到最新一版"
        );
        let back = store
            .find(&verdict.anchor)
            .expect("读得到")
            .expect("老裁决还在");
        assert_eq!(back.decision, verdict.decision, "裁决一个字都没变");
        assert_eq!(
            store.batch_rows(batch).expect("读得到").len(),
            1,
            "那一批里的行也还在",
        );
        // 新那张表真的建出来了，而且是空的——升级不会凭空长出成员关系。
        assert!(store.memberships().expect("读得到").is_empty());
    }

    #[test]
    fn 同一条锚放两次进同一个合集只算一条() {
        // 成员关系是个是非题：「在里面」没有第二种程度。攒出两行的话，
        // 取消收藏点一下会只去掉一半。
        let mut store = Store::in_memory().expect("开得出来");
        let anchor = Anchor::Content {
            crc32: 0x1234_5678,
            size: 1024,
            sha1: None,
        };
        let 一条 = [Membership::now("收藏", anchor.clone())];
        assert_eq!(store.join(&一条).expect("放得进"), 1);
        assert_eq!(store.join(&一条).expect("放得进"), 0, "第二次不是新的一条");
        assert_eq!(store.memberships().expect("读得到").len(), 1);
        assert_eq!(
            store.joined(&anchor).expect("读得到"),
            vec!["收藏".to_string()]
        );
        assert_eq!(
            store
                .leave("收藏", std::slice::from_ref(&anchor))
                .expect("拿得出"),
            1
        );
        assert!(store.joined(&anchor).expect("读得到").is_empty());
    }

    #[test]
    fn 路径锚的成员关系只在自己那份主库里算数() {
        // 与裁决同一条：路径锚记的是「主库『某某』里的某某变体」，换一份主库那条锚
        // 说的就不是同一个东西了。快照按主库名筛，投影才不会把甲库的收藏落到乙库上。
        let mut store = Store::in_memory().expect("开得出来");
        let 两条: Vec<Membership> = ["甲", "乙"]
            .into_iter()
            .map(|library| {
                Membership::now(
                    "收藏",
                    Anchor::Path {
                        library: library.to_string(),
                        variant_key: "FC/某.zip".to_string(),
                    },
                )
            })
            .collect();
        store.join(&两条).expect("放得进");
        let index = Index::load(&store, "甲").expect("读得出快照");
        assert_eq!(index.memberships().len(), 1);
        assert_eq!(
            index.memberships()[0].anchor,
            Anchor::Path {
                library: "甲".to_string(),
                variant_key: "FC/某.zip".to_string(),
            },
        );
    }

    #[test]
    fn 一批连它盖掉的那条一起记得住也读得回来() {
        // **被盖掉的那条除了这儿没有第二份**（它不可再生），所以批里存着它，撤销才放得回去。
        let mut store = Store::in_memory().expect("开得出来");
        let 旧的 = 汉化裁决();
        let 新的 = Verdict::now(
            旧的.anchor.clone(),
            Decision::Release(Facts {
                work: "改成这个".to_string(),
                ..Facts::default()
            }),
        );
        let rows = vec![BatchRow {
            variant_key: "库/FC/某.zip".to_string(),
            member: "库/FC/某.zip".to_string(),
            inner: "rom.nes".to_string(),
            after: 新的.clone(),
            before: Some(旧的.clone()),
        }];
        let id = store
            .put_batch("小库", "作品《改成这个》", Some("按记号裁一批"), &rows)
            .expect("记得下");

        let 列出来 = store.batches("小库", 0).expect("列得出");
        assert_eq!(列出来.len(), 1);
        assert_eq!((列出来[0].id, 列出来[0].rows), (id, 1));
        assert_eq!(列出来[0].summary, "作品《改成这个》");
        assert!(!列出来[0].undone(), "刚落下的一批不该是撤掉的状态");
        // **按主库筛**：路径锚只在本机的这一份主库里成立，别的主库的批列出来撤不动。
        assert!(store.batches("另一份库", 0).expect("列得出").is_empty());

        let 读回来 = store.batch_rows(id).expect("读得回来");
        assert_eq!(读回来.len(), 1);
        assert_eq!(读回来[0].after, 新的, "落下的那条要一个字不差");
        assert_eq!(读回来[0].before, Some(旧的), "盖掉的那条也要一个字不差");
        assert_eq!(读回来[0].inner, "rom.nes", "重做要靠它说得出是包里的哪一个");

        store.mark_batch_undone(id, true).expect("标得上");
        assert!(store.batch(id).expect("读得到").expect("在").undone());
        store.mark_batch_undone(id, false).expect("标得回去");
        assert!(!store.batch(id).expect("读得到").expect("在").undone());
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
                .put_match(&MatchVerdict::now(
                    锚.clone(),
                    "中文离线源",
                    entry,
                    accepted,
                ))
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
        assert_eq!(
            只有路径锚.export(true).expect("导得出").version,
            EXPORT_VERSION
        );
    }

    #[test]
    fn 第一版的裁决文件照读只是没有匹配裁决() {
        // 往前兼容：老文件里没有 `matches` 那一栏，读回来是空的，不是读不动。
        let mut store = Store::in_memory().expect("开得出来");
        let text = r#"{"format":"romcat-沉淀库","version":1,"exported_at":0,
             "verdicts":[{"crc32":"12345678","size":40976,"kind":"认不出","decided_at":0}]}"#;
        let account = store.import(text).expect("收得下");
        assert_eq!(
            (account.read, account.added, account.matches_read),
            (1, 1, 0)
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

    #[test]
    fn 沉淀库的连接设了等锁的余量() {
        let dir = crate::testing::temp_dir("verdict-timeout");
        let store = Store::open(&dir.path().join("verdict.sqlite3")).expect("开得起来");
        let 余量: i64 = store
            .conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .expect("读得回来");
        assert_eq!(余量, i64::from(BUSY_TIMEOUT_MS), "设进去的读得回来");
    }

    /// 拿一份**还没建起来的空库**当现场：第二份连接 `BEGIN IMMEDIATE` 占住写锁，
    /// 这时候开库要把它转成 WAL、还要跑迁移，两样都得拿写锁。而**转日志模式那一句不认
    /// 忙等待**，旧代码在这儿当场「沉淀库打不开：… database is locked」。
    ///
    /// **钉不成 flaky**：断言只说「等得到、开得出来」，锁放得早放得晚它都成立。
    /// 中间那一小段停顿不参与判定，只是让**没设余量**的旧代码必定撞上那一下。
    #[test]
    fn 写锁占着时沉淀库等得到而不是当场报忙() {
        let dir = crate::testing::temp_dir("verdict-busy");
        let path = dir.path().join("verdict.sqlite3");
        let blocker = Connection::open(&path).expect("开得起来");
        blocker
            .execute_batch("BEGIN IMMEDIATE")
            .expect("拿得到写锁");

        let (报开工, 等开工) = std::sync::mpsc::channel();
        let (交结果, 等结果) = std::sync::mpsc::channel();
        let 那份路径 = path.clone();
        let 那条线程 = std::thread::spawn(move || {
            报开工.send(()).expect("说得出去");
            let 结果 = Store::open(&那份路径)
                .and_then(|store| store.counts())
                .map(|counts| counts.total)
                .map_err(|error| error.to_string());
            交结果.send(结果).expect("交得回去");
        });
        等开工.recv().expect("那条线程起来了");
        std::thread::sleep(std::time::Duration::from_millis(100));
        blocker.execute_batch("ROLLBACK").expect("放得开");

        let 拿到 = 等结果
            .recv_timeout(std::time::Duration::from_secs(60))
            .expect("等得到那条线程交回来的结果");
        那条线程.join().expect("收得回来");
        assert_eq!(拿到, Ok(0), "撞上写锁该等着，不该当场报忙");
    }

    #[test]
    fn 当前版本的沉淀库打开时一个字都不写() {
        // 根因就在这一句上：`PRAGMA user_version = N` 是一次写事务，**同值重写照样要拿
        // 写锁**，而它原来每次开库都跑一遍——「开一下当前版本的沉淀库」这件本该只读的
        // 事成了一次写，于是命令行开个库就跟界面正在落的那条裁决抢同一把锁。
        //
        // **怎么钉住「没写」而不碰时钟**：`PRAGMA data_version` 这个数只在**别的连接**
        // 提交过写事务之后才会变。旁观的那份连接**先开好、全程开着**——它一直在，
        // 被观察的那份就不是最后一份连接，关掉时不会顺手做一次 WAL 收尾（那本身是写）。
        let dir = crate::testing::temp_dir("verdict-idle-open");
        let path = dir.path().join("verdict.sqlite3");
        drop(Store::open(&path).expect("建得出来"));

        let 旁观 = Connection::open(&path).expect("开得起来");
        let 之前: i64 = 旁观
            .query_row("PRAGMA data_version", [], |row| row.get(0))
            .expect("读得到");
        drop(Store::open(&path).expect("再开一次"));
        let 之后: i64 = 旁观
            .query_row("PRAGMA data_version", [], |row| row.get(0))
            .expect("读得到");
        assert_eq!(之前, 之后, "当前版本的沉淀库开一次不该写任何东西");
    }

    #[test]
    fn 一条迁移半途炸了_版本号与建到一半的表一起退回去() {
        // 迁移语句与版本号在**同一个事务**里，不然会留下「表已经改了、版本号还写着旧的」
        // 那种库。眼下三条都是 `CREATE TABLE IF NOT EXISTS`，重跑无害——这一条钉的是
        // **将来**：等哪天有一条迁移改的是已有的表，它得先炸，而不是等用户丢了裁决。
        let conn = Connection::open_in_memory().expect("开得出来");
        conn.execute_batch(MIGRATIONS[0]).expect("建得出第一版");
        conn.execute_batch("PRAGMA user_version = 1")
            .expect("盖得上第一版的版本号");
        let 掺了一条炸的 = [MIGRATIONS[0], MIGRATIONS[1], "这不是一句 SQL"];

        Store::apply(&conn, 1, &掺了一条炸的, 3).expect_err("最后那条该炸");

        let 版本: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("读得到");
        assert_eq!(版本, 1, "版本号跟着退回去，下次开库还从第一版接着跑");
        let 建出来了吗: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = 'match_verdict'",
                [],
                |row| row.get(0),
            )
            .expect("数得出");
        assert_eq!(建出来了吗, 0, "半路建出来的表也跟着退回去");
    }
}
