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
//! - 作品与发行版多出一列 `origin`：这一行是**识别**撞出来的，还是**裁决**定下来的。
//!   报告靠它把「识别建出来的作品数」与裁决攒出来的分开数。
//!
//! ## 这里的每一行都是可再生的
//!
//! 三张表加那两列 `origin`，**没有一行是攒出来的**：识别撞出来的重跑一遍就有，
//! 裁决定下来的照**沉淀库**（[`verdict::Store`](crate::verdict::Store)）重放一遍就有。
//! 沉淀库是一份不跟中立库走、也不随它删的文件（票 08），中立库里这几行只是它的投影。
//!
//! 这条性质值钱：中立库因此可以一直走「结构版本对不上就删库重扫」那条便宜路
//! （见 [`SCHEMA_VERSION`](super::SCHEMA_VERSION)），不必为它写迁移。
//!
//! ## 「没有发行版链接」不再靠推断
//!
//! `CONTEXT.md` 说：同人移植与 homebrew 直接挂在**作品**下，没有发行版链接这件事
//! 本身就在告诉识别管线别拿它去撞 DAT。**但那条判据不能靠「作品有、发行版空」推出来**
//! ——裁决定了作品、识别认出了发行版，清完也是这个形状（原挂账 D48）。票 08 起
//! 沉淀库把「确认没有发行版」记成一条**明确的裁决**，识别读那一条，不再猜。

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

-- 从一份内容前几百字节里读出来的**光盘标识**与 **NKit** 结论（票 09）。
--
-- 它与 `content_hash` 同源同命：都是「读过的盘不白读」，都按文件的三元组作废。
-- 分开一张表而不是往 `content_hash` 上加列，是为了**不动已有的表结构**——加列要用户
-- 删掉 8.60 TiB 的中立库重扫一遍，而加表在 `CREATE TABLE IF NOT EXISTS` 这条路上是
-- 白拿的。代价写在下面那两列上。
--
-- `len` 与 `mtime_ns` 是**这一行自带的有效期**：算这一条时那个成员文件多大、什么时候
-- 改的。读回来先对一遍，对不上就当没算过。有了这两列，这张表就不依赖任何人记得
-- 去作废它——而一个旧版本的程序照样会更新 `entry`、却不知道这张表存在。
CREATE TABLE IF NOT EXISTS content_disc(
    key      TEXT    NOT NULL,
    inner    TEXT    NOT NULL,
    len      INTEGER,
    mtime_ns INTEGER,
    -- `identify::disc::Facts` 的 JSON。壳子、NKit 结论、读出来的标识、说不出口时那句话。
    facts    TEXT    NOT NULL,
    PRIMARY KEY (key, inner)
) STRICT;

-- 从一份内容前几百字节里读出来的**卡带内部头**（票 10）。与 `content_disc` 同一条路：
-- 同样自带有效期、同样「读过的不白读」、同样加表而不加列。
--
-- `platform` 单独一列而不是只躺在 JSON 里，是因为**报告要按它数一件事**：内部头说的
-- 平台与目录声明的平台对不上（ADR-0011 说那正是最该报告的产出之一）。一条 SQL 数得出来
-- 的东西，不该逐行反序列化几万段 JSON 去数。
CREATE TABLE IF NOT EXISTS content_cart(
    key      TEXT    NOT NULL,
    inner    TEXT    NOT NULL,
    len      INTEGER,
    mtime_ns INTEGER,
    -- 内部头说这份内容属于哪个平台；认不出是 NULL。
    platform TEXT,
    -- 这份头**认哪几个平台**，写成 `,GB,GBC,` 这样两头带逗号的一串。
    --
    -- 它与 `platform` 是两件事，而**判冲突要看它不看那一个**：GB 与 GBC 共用一份卡带头
    -- （差别只在 `0x143` 那一个字节），一份 CGB 卡躺在 `gb/` 目录里不是冲突，是常态。
    -- 只按 `platform` 比字符串，报告会被这类同族配对淹掉（真机实测 1,722 份里绝大多数）。
    -- 两头的逗号是为了让 SQL 能用 `instr` 做整词匹配，不至于 `GB` 匹配上 `GBA`。
    family   TEXT,
    -- `identify::cart::Facts` 的 JSON。
    facts    TEXT    NOT NULL,
    PRIMARY KEY (key, inner)
) STRICT;
";

/// 补上后来加的列。**纯加列，不改已有列的含义**，所以不动
/// [`SCHEMA_VERSION`](super::SCHEMA_VERSION)——旧库照样打得开，旧程序也照样能用
/// （与 `catalog::sublibrary::add_columns` 同一条路）。
///
/// 票 10 加的是 `content_hash` 上那两列 SHA-1。为它逼用户删掉 8.60 TiB 的中立库、
/// 重扫 27 分钟，换不到任何东西。
///
/// **`content_cart` 不在这里**：那张表是票 10 新建的，`CREATE TABLE IF NOT EXISTS`
/// 一次就把它连同 `family` 那一列建齐了。这里只放「已经存在于旧库里的表上后来加的列」。
pub(super) fn add_columns(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    add_column(conn, "content_hash", "sha1", "TEXT")?;
    add_column(conn, "content_hash", "bare_sha1", "TEXT")
}

/// 一张表上缺了这一列就补上；已经有了就什么都不做。
fn add_column(
    conn: &rusqlite::Connection,
    table: &str,
    column: &str,
    decl: &str,
) -> rusqlite::Result<()> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        if row.get::<_, String>(1)? == column {
            return Ok(());
        }
    }
    drop(rows);
    drop(statement);
    // 表名与列名都是这个文件里写死的字面量，不来自外面。
    conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"))
}

/// 一条候选的**置信度**（ADR-0002）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    /// **高置信**：精确哈希命中，自动通过，不进待确认队列。
    High,
    /// **中置信**：撞上了但有保留（DAT 没记大小、或者这份镜像是 NKit 处理过的），
    /// 通过但标记，等人裁决。
    Medium,
    /// **低置信**：文件名规则、模糊匹配与模型推断那几层的产物，一律进待确认队列。
    ///
    /// 票 10 起有一条真的落在这一档：**目录名里直接写着的那个 TitleID**
    /// （`identify::serial::title_id_in_name`）——它连内容都没看，正是「文件名规则」
    /// 本身。票 11 与票 12 会把这一档填满。
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
///
/// 名字不叫 `Origin`：`dat::registry::Origin` 说的是「一份 DAT 从哪儿取」，
/// 同一个 crate 里两个 `Origin` 只会让人读错。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    /// **识别**撞 DAT 撞出来的。
    Identified,
    /// 人工**裁决**定下来的。
    ///
    /// **票 08 起它也随重跑识别整批清掉再重建**——裁决的家搬去了
    /// [`verdict::Store`](crate::verdict::Store)，中立库里这几行只是它的投影
    /// （见 [`Catalog::clear_identifications`](super::Catalog::clear_identifications)）。
    /// 这一列留着是为了**报告分得清两者各建出来多少**，不再是「别删我」的记号。
    Verdict,
}

impl Provenance {
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
    /// 撞上时用的是**哪套哈希**（含头 / 去头）。
    pub hashed_as: Convention,
    /// 那份 DAT 自己声明的**口径**。两者会不一样：没有外挂头的文件，含头那套哈希
    /// 照样撞得上去头口径的 DAT——两套本来就落在同一串字节上。
    pub dat_convention: Convention,
    /// **依据**：给人看的那一句。
    pub evidence: String,
    /// 中文记号。
    pub chinese: Option<ChineseMark>,
    /// 序列号。
    pub serial: Option<String>,
    /// 这条候选建出来的发行版。
    pub release_id: Option<i64>,
}

/// 一份内容探出来的**卡带内部头**，写库前的样子（票 10）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CartFactRow {
    /// 成员的键。
    pub key: String,
    /// 容器内部路径；裸文件是空串。
    pub inner: String,
    /// 内部头说这份内容属于哪个平台；认不出是 `None`。
    pub platform: Option<String>,
    /// 这份头认哪几个平台，写成 `,GB,GBC,` 这样两头带逗号的一串。**判冲突看它。**
    pub family: Option<String>,
    /// `identify::cart::Facts` 的 JSON。
    pub facts: String,
}

/// **命中里只靠某一个源的候选**的变体数，按平台。
///
/// 它为一件事而存在：[文件名那一层](crate::identify::fuzzy)产出的候选一条都不自动通过，
/// 却照样把变体的结论从「未命中」变成「命中」——那是对的（**它确实拿到了带依据的候选**），
/// 但如果报告只给一个总的命中率，读者就分不出「认出来了」与「有人猜了一下」。
///
/// 于是这一列单列。判据是「这个变体的候选**全部**来自那个源」——只要还有一条别的源的
/// 候选，它就不算只靠名字。
///
/// **第三列是「这些变体本来是无判据的」**（`units = 0`：一份可以撞的内容都没有）。
/// 少了它，「不算那一层的命中率」会算错：那一层把一批本来在**无判据**里的变体拉进了
/// 命中，而无判据本来就不在命中率的分母里（跳过与无判据不混进未命中，见模块文档）。
/// 只从分子里减掉它们、分母却留着，那个数会比那一层跑之前还低——凭空冤枉前几层。
pub(super) const NAME_ONLY_SQL: &str = "\
SELECT COALESCE(v.platform, ?2), COUNT(*), SUM(CASE WHEN i.units = 0 THEN 1 ELSE 0 END)
                 FROM identification i
                 JOIN variant v ON v.key = i.variant_key
                 WHERE i.state = ?3
                   AND i.candidates > 0
                   AND NOT EXISTS (
                       SELECT 1 FROM candidate c
                        WHERE c.variant_key = i.variant_key AND c.source <> ?1)
                 GROUP BY 1";

/// 「内部头与目录声明的平台对不上」这件事的判据，两处查询共用一份。
///
/// **判据是家族不是那一个平台**：GB 与 GBC 共用一份卡带头，一份 CGB 卡躺在 `gb/`
/// 目录里不是冲突。`instr` 上两头带逗号是为了整词匹配——不然 `GB` 会匹配上 `GBA`。
const CONFLICT_FROM: &str = "\
                 FROM content_cart c
                 JOIN variant_member m ON m.key = c.key
                 JOIN variant v ON v.key = m.variant_key
                 WHERE c.platform IS NOT NULL
                   AND c.family IS NOT NULL
                   AND v.platform IS NOT NULL
                   AND instr(c.family, ',' || v.platform || ',') = 0";

/// 内部头说的平台与**目录声明的平台**对不上的一条（ADR-0011）。
///
/// 「目录说 GBA、文件头说 NDS」是真实会发生的（下错、放错、压缩包混装），而 ADR-0011
/// 明说这类冲突不是错误而是**最该报告的产出之一**。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PlatformConflict {
    /// 哪个变体。
    pub variant_key: String,
    /// 目录声明的平台。
    pub declared: String,
    /// 内部头说的平台。
    pub found: String,
    /// 是哪一份内容说的。
    pub member: String,
}

/// 一份内容探出来的**光盘标识**，写库前的样子。
///
/// 捏成一个类型而不是一个三元组：三样都是 `String`，元组里写错顺序编译器不会说话
/// （与 `identify::Wanted` 同理）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscFactRow {
    /// 是变体的哪个成员：容器的键，或者裸文件自己的键。
    pub key: String,
    /// 容器内部路径；裸文件是空串。
    pub inner: String,
    /// `identify::disc::Facts` 的 JSON。
    pub facts: String,
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

/// 逐条走候选时收到的那三样：变体的键、源、条目名。
///
/// **只有这三样**：刮削从条目名里读元数据，是哪一份 DAT、有没有中文记号都在
/// `candidate` 那张表里躺着，事后复核照查不误。真库里这是 150,959 行，多搬一列
/// 就是多搬 150,959 个字符串。
pub type CandidateFactVisitor<'a> = dyn FnMut(&str, &str, &str) + 'a;

/// 一条**自动通过**的候选，**标题集合**用得上的那几列。
///
/// 捏成一个结构而不是五个参数：这几样本来就成群结队地一起走，而分开传的话
/// `source` 与 `game` 两个 `&str` 挨在一起，调用处写反了编译器不会说话。
#[derive(Debug, Clone, Copy)]
pub struct AcceptedCandidate<'a> {
    /// 哪个变体撞上的。
    pub variant_key: &'a str,
    /// 哪个数据源。
    pub source: &'a str,
    /// 条目名。**那次发行的官方名就编码在这里面。**
    pub game: &'a str,
    /// 中文记号。
    pub chinese: Option<ChineseMark>,
    /// 这条候选建出来的发行版。
    pub release_id: Option<i64>,
}

/// 逐条走**自动通过**的候选时收到的那一条。
pub type AcceptedCandidateVisitor<'a> = dyn FnMut(&AcceptedCandidate<'_>) + 'a;

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
    /// 含头（原样）的 SHA-1，四十位小写十六进制；没算过是 `None`（票 10）。
    pub sha1: Option<String>,
    /// 去头之后的 SHA-1；没有外挂头时与 `sha1` 相同，没算过是 `None`。
    pub bare_sha1: Option<String>,
}

/// 库里存的那个词认回一个中文记号。
///
/// 认不出的一律 `None`：中文记号只有两种（[`ChineseMark`]），认不出说明库被人改过，
/// 那时**当作没有记号**比猜一个安全——猜错了会把汉化版当成官中版，
/// 而那正是 ADR-0012 要分开的两件事。
fn chinese_mark(label: &str) -> Option<ChineseMark> {
    match label {
        "汉化" => Some(ChineseMark::FanTranslated),
        "官中" => Some(ChineseMark::Official),
        _ => None,
    }
}

/// **待确认队列**要的一行：变体连它这一轮的结论。
///
/// 捏成一次查询而不是「先列变体、再逐个问结论」：真库里那是 46,444 个变体，
/// 逐个问就是 46,444 次查询，而队列只是要把其中一万多条挑出来。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueRow {
    /// 变体本身。
    pub variant: super::VariantRow,
    /// 这一轮的结论。
    pub state: State,
    /// 跳过或无判据的具体理由。
    pub reason: Option<String>,
    /// 有几条**候选**。
    pub candidates: u64,
    /// 其中**自动通过**的有几条。**一条都没有的才要人裁决**（ADR-0002）。
    pub accepted: u64,
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
    /// 全部变体连它们这一轮的识别结论，按键排序。**待确认队列**的原料。
    ///
    /// 还没识别过的变体**不在里面**：队列说的是「识别拿不定主意的那些」，
    /// 而没跑过识别时那是「全部」——那时该说的是「先跑一次 `romcat identify`」。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn queue_rows(&self) -> Result<Vec<QueueRow>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT v.key, v.platform, v.rule, v.main_key, v.files, v.bytes, v.unreadable,
                        v.manual, v.work_id, v.release_id, i.state, i.reason, i.candidates,
                        i.accepted
                 FROM variant v JOIN identification i ON i.variant_key = v.key
                 ORDER BY v.key",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                let state: String = row.get(10)?;
                Ok(QueueRow {
                    variant: super::VariantRow {
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
                    },
                    state: State::from_label(&state).unwrap_or(State::NoEvidence),
                    reason: row.get(11)?,
                    candidates: u64::try_from(row.get::<_, i64>(12)?).unwrap_or(0),
                    accepted: u64::try_from(row.get::<_, i64>(13)?).unwrap_or(0),
                })
            })
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 按名字找一个**作品**；没有就是 `None`。
    ///
    /// **裁决**拿它把说的是同一部作品的几条裁决归到同一行上——建出两行作品会让导出时的
    /// **收敛**把它们拆成两个前端条目。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn work_named(&self, name: &str) -> Result<Option<i64>, CatalogError> {
        self.conn
            .query_row(
                "SELECT id FROM work WHERE name = ?1 ORDER BY id LIMIT 1",
                params![name],
                |row| row.get(0),
            )
            .optional()
            .map_err(|source| self.err(source))
    }

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

    /// 一个**已确认**的变体撞上 DAT 时用的那套判据：`(CRC-32, 字节数, 记录名)`。
    ///
    /// 「已确认」的判据是**有一条自动通过的候选**（`accepted`）。这正是在线刮削档发查询
    /// 的门槛：ScreenScraper 强制要求「crc/md5/sha1 之一**加**文件字节大小」同发
    /// （调研 §1.4），而它对认不出来的 ROM 有一份**专门的当日配额**（431），撞穿了
    /// 连账号带 IP 一起封（ADR-0007）。**未识别的变体一个请求都不许发。**
    ///
    /// 取的是**撞上时用的那一套**，不是随便一套：候选的 `hashing` 记着它撞的是含头还是
    /// 去头的哈希，取错一套就是把一串对不上的 CRC 发出去——那是白扣一份「未识别 ROM」
    /// 配额，正是这道门槛要省下来的东西。
    ///
    /// 判据从**两张表**里找，因为它本来就躺在两处：**裸文件**读一遍算出来，落在
    /// `content_hash`；**透明容器**里的东西零解压就有，落在 `container_entry`
    /// （票 03 的整个意义就是不去解压它们）。只查前一张，库里 91.1% 的容量——
    /// 也就是绝大多数已确认的变体——会安静地取不到判据。
    ///
    /// ## 为什么是逐个查而不是一条大 JOIN
    ///
    /// 写成一条 `candidate LEFT JOIN content_hash LEFT JOIN container_entry` 在真库上
    /// **实测 73 秒**：136,238 条自动通过的候选各自去 1,016,857 行的容器构成表里按
    /// `key` 做一次范围扫描，而候选是按 `variant_key` 排的，没有任何局部性。
    /// 逐个查的次数少一个量级——**在线档只为每部作品的那一个代表变体查一次**
    /// （真库 9,226 次而不是 136,238 次），而且离线档一次都不查。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn accepted_hash(
        &self,
        variant_key: &str,
    ) -> Result<Option<(u32, u64, String)>, CatalogError> {
        // 一个变体可以有好几条自动通过的候选（同一份内容撞上 No-Intro 与 TOSEC 的同一条）。
        // 它们说的是**同一串字节**，取第一条就够；`ORDER BY id` 保证同一份库跑两次
        // 取到的是同一条。
        let pick: Option<(String, String, String, String)> = self
            .conn
            .prepare_cached(
                "SELECT hashing, rom, member_key, inner FROM candidate
                 WHERE variant_key = ?1 AND accepted = 1 ORDER BY id LIMIT 1",
            )
            .and_then(|mut statement| {
                statement
                    .query_row(params![variant_key], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                    })
                    .optional()
            })
            .map_err(|source| self.err(source))?;
        let Some((hashing, rom, member_key, inner)) = pick else {
            return Ok(None);
        };

        // **去头那一套只有 `content_hash` 有**——容器内部构成给的永远是原样的字节。
        // 取哪一套在这里当场判完，免得把四个 `Option<i64>` 原样端出去。
        let bare = Convention::from_label(&hashing) == Some(Convention::Headerless);
        let picked: Option<(i64, i64)> = self
            .conn
            .prepare_cached(
                "SELECT size, crc32, bare_size, bare_crc32 FROM content_hash
                 WHERE key = ?1 AND inner = ?2",
            )
            .and_then(|mut statement| {
                statement
                    .query_row(params![member_key, inner], |row| {
                        let size: Option<i64> = row.get(0)?;
                        let crc: Option<i64> = row.get(1)?;
                        let bare_size: Option<i64> = row.get(2)?;
                        let bare_crc: Option<i64> = row.get(3)?;
                        Ok(if bare {
                            bare_crc.zip(bare_size).or_else(|| crc.zip(size))
                        } else {
                            crc.zip(size)
                        })
                    })
                    .optional()
            })
            .map_err(|source| self.err(source))?
            .flatten();
        let picked = match picked {
            Some(pair) => Some(pair),
            None => self
                .conn
                .prepare_cached(
                    "SELECT crc32, size FROM container_entry
                     WHERE key = ?1 AND inner = ?2 LIMIT 1",
                )
                .and_then(|mut statement| {
                    statement
                        .query_row(params![member_key, inner], |row| {
                            Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, i64>(1)?))
                        })
                        .optional()
                })
                .map_err(|source| self.err(source))?
                .and_then(|(crc, size)| crc.map(|crc| (crc, size))),
        };
        let Some((crc, size)) = picked else {
            return Ok(None);
        };
        let Ok(size) = u64::try_from(size) else {
            return Ok(None);
        };
        Ok(Some((u32::try_from(crc).unwrap_or(0), size, rom)))
    }

    /// 某个成员上算过的哈希：内部路径 → 那一份。裸文件的内部路径是空串。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn content_hashes(&self, key: &str) -> Result<BTreeMap<String, ContentHash>, CatalogError> {
        let mut statement = self
            .conn
            .prepare_cached(
                "SELECT inner, size, crc32, looked, header, bare_size, bare_crc32, nkit,
                        sha1, bare_sha1
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
                    sha1: row.get(8)?,
                    bare_sha1: row.get(9)?,
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
                         bare_size, bare_crc32, nkit, sha1, bare_sha1)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
                     ON CONFLICT(key, inner) DO UPDATE SET
                        size = excluded.size, crc32 = excluded.crc32,
                        looked = excluded.looked, header = excluded.header,
                        bare_size = excluded.bare_size, bare_crc32 = excluded.bare_crc32,
                        nkit = excluded.nkit,
                        -- **算过的 SHA-1 不许被一次没算的覆盖回 NULL**：这一层是按需
                        -- 才付的钱（票 10），下一趟没走到它不等于上一趟白算了。
                        sha1 = COALESCE(excluded.sha1, content_hash.sha1),
                        bare_sha1 = COALESCE(excluded.bare_sha1, content_hash.bare_sha1)",
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
                        row.sha1.as_deref(),
                        row.bare_sha1.as_deref(),
                    ])
                    .map_err(to_err)?;
            }
        }
        tx.commit().map_err(to_err)
    }

    /// 某个成员上探过的**光盘标识**：内部路径 → 那一份。裸文件的内部路径是空串。
    ///
    /// **自带有效期**：存进去时记下了那个成员文件的大小与修改时间，这里逐行对一遍，
    /// 对不上的当没算过（那一行随后会被这一趟重新算出来的覆盖掉）。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn disc_facts(&self, key: &str) -> Result<BTreeMap<String, String>, CatalogError> {
        let mut statement = self
            .conn
            .prepare_cached(
                "SELECT d.inner, d.facts FROM content_disc d
                 JOIN entry e ON e.key = d.key
                 WHERE d.key = ?1
                   AND d.len IS e.len AND d.mtime_ns IS e.mtime_ns",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![key], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|source| self.err(source))?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (inner, facts) = row.map_err(|source| self.err(source))?;
            out.insert(inner, facts);
        }
        Ok(out)
    }

    /// 某个成员上探过的**卡带内部头**：内部路径 → 那一份。裸文件的内部路径是空串。
    ///
    /// 与 [`disc_facts`](Self::disc_facts) 同一条路，**自带有效期**。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn cart_facts(&self, key: &str) -> Result<BTreeMap<String, String>, CatalogError> {
        let mut statement = self
            .conn
            .prepare_cached(
                "SELECT c.inner, c.facts FROM content_cart c
                 JOIN entry e ON e.key = c.key
                 WHERE c.key = ?1
                   AND c.len IS e.len AND c.mtime_ns IS e.mtime_ns",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![key], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|source| self.err(source))?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (inner, facts) = row.map_err(|source| self.err(source))?;
            out.insert(inner, facts);
        }
        Ok(out)
    }

    /// 把探出来的卡带内部头整批存下来。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put_cart_facts(&mut self, rows: &[CartFactRow]) -> Result<(), CatalogError> {
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
                    "INSERT INTO content_cart(key, inner, len, mtime_ns, platform, family, facts)
                     VALUES(?1, ?2,
                            (SELECT len FROM entry WHERE key = ?1),
                            (SELECT mtime_ns FROM entry WHERE key = ?1),
                            ?3, ?4, ?5)
                     ON CONFLICT(key, inner) DO UPDATE SET
                        len = excluded.len, mtime_ns = excluded.mtime_ns,
                        platform = excluded.platform, family = excluded.family,
                        facts = excluded.facts",
                )
                .map_err(to_err)?;
            for row in rows {
                insert
                    .execute(params![
                        row.key,
                        row.inner,
                        row.platform,
                        row.family,
                        row.facts
                    ])
                    .map_err(to_err)?;
            }
        }
        tx.commit().map_err(to_err)
    }

    /// **内部头说的平台与目录声明的平台对不上的那些**（ADR-0011）。
    ///
    /// 报告从中立库折出来，不重跑识别（ADR-0001）：所以这件事记在 `content_cart` 上，
    /// 而不是攒在一趟识别的内存里。`limit` 是最多取几条例子。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn platform_conflicts(&self, limit: usize) -> Result<Vec<PlatformConflict>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(&format!(
                "SELECT v.key, v.platform, c.platform, c.key
                 {CONFLICT_FROM}
                 ORDER BY v.key
                 LIMIT ?1"
            ))
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![i64::try_from(limit).unwrap_or(i64::MAX)], |row| {
                Ok(PlatformConflict {
                    variant_key: row.get(0)?,
                    declared: row.get(1)?,
                    found: row.get(2)?,
                    member: row.get(3)?,
                })
            })
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 把探出来的光盘标识整批存下来。有效期那两列从 `entry` 上现取。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put_disc_facts(&mut self, rows: &[DiscFactRow]) -> Result<(), CatalogError> {
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
                    "INSERT INTO content_disc(key, inner, len, mtime_ns, facts)
                     VALUES(?1, ?2,
                            (SELECT len FROM entry WHERE key = ?1),
                            (SELECT mtime_ns FROM entry WHERE key = ?1),
                            ?3)
                     ON CONFLICT(key, inner) DO UPDATE SET
                        len = excluded.len, mtime_ns = excluded.mtime_ns,
                        facts = excluded.facts",
                )
                .map_err(to_err)?;
            for row in rows {
                insert
                    .execute(params![row.key, row.inner, row.facts])
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
                            candidate.hashed_as.label(),
                            candidate.dat_convention.label(),
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

    /// 把上一轮的识别结论整批清掉：候选、结论，以及**作品**与**发行版**里由它们造出来的行。
    ///
    /// 顺序是有讲究的：先摘链接再删行，否则变体上会留下指向已删除记录的悬空 id。
    ///
    /// ## 为什么 `origin = 裁决` 的行也一起清
    ///
    /// 票 08 之前那些行没有产者，留着它们是怕清掉之后没处补。**票 08 起裁决有了自己的
    /// 家**——[`verdict::Store`](crate::verdict::Store)，一份不跟中立库走、也不随它删的
    /// 文件。于是中立库里这几行成了那份库的**投影**：清掉再照沉淀库重建一遍，结果一模一样，
    /// 而不清的话每跑一次识别就多攒一份重复的作品与发行版。
    ///
    /// `origin` 这一列照旧有用——它说得出一行是**识别**撞出来的还是**裁决**定下来的，
    /// 报告里的「识别建出来的作品数」靠它把两者分开数。
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
        // 顺序是**从引用方往被引用方**走：候选指着发行版、变体指着作品与发行版，
        // 外键是开着的（`rusqlite` 的 bundled SQLite 编译时开了
        // `SQLITE_DEFAULT_FOREIGN_KEYS=1`），先删被指着的那一行会当场报错。
        for sql in ["DELETE FROM candidate", "DELETE FROM identification"] {
            tx.execute(sql, []).map_err(to_err)?;
        }
        for sql in [
            "UPDATE variant SET release_id = NULL",
            "UPDATE variant SET work_id = NULL",
            "DELETE FROM release",
            "DELETE FROM work",
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
                    hashed_as: Convention::from_label(&hashing).unwrap_or(Convention::AsIs),
                    dat_convention: Convention::from_label(&convention).unwrap_or(Convention::AsIs),
                    evidence: row.get(11)?,
                    chinese: chinese.as_deref().and_then(chinese_mark),
                    serial: row.get(13)?,
                    release_id: row.get(14)?,
                })
            })
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 一条条走过全部**候选**里刮削用得上的那几列：变体的键、源、条目名。
    ///
    /// 走回调而不是返回一整份 `Vec`：真库里这是 150,959 行，而刮削要的只是把它们按
    /// 锚点归堆。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn for_each_candidate_fact(
        &self,
        each: &mut CandidateFactVisitor,
    ) -> Result<(), CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT variant_key, source, game FROM candidate
                 ORDER BY variant_key, id",
            )
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let key: String = row.get(0).map_err(|source| self.err(source))?;
            let src: String = row.get(1).map_err(|source| self.err(source))?;
            let game: String = row.get(2).map_err(|source| self.err(source))?;
            each(&key, &src, &game);
        }
        Ok(())
    }

    /// 一条条走过**自动通过**的候选：变体的键、源、条目名、中文记号、发行版。
    ///
    /// **只走自动通过的那些**：标题集合要的是「那次发行的官方名叫什么」与「这个变体是不是
    /// 官中 / 汉化」，两者都是**结论**而不是猜测——没通过的候选连它到底是不是这个游戏
    /// 都还没定，拿它的条目名当官方名等于把猜测写成事实。真库上这一刀把 150,959 条
    /// 候选筛成 136,238 条。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn for_each_accepted_candidate(
        &self,
        each: &mut AcceptedCandidateVisitor,
    ) -> Result<(), CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT variant_key, source, game, chinese, release_id FROM candidate
                 WHERE accepted <> 0 ORDER BY variant_key, id",
            )
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let variant_key: String = row.get(0).map_err(|source| self.err(source))?;
            let src: String = row.get(1).map_err(|source| self.err(source))?;
            let game: String = row.get(2).map_err(|source| self.err(source))?;
            let chinese: Option<String> = row.get(3).map_err(|source| self.err(source))?;
            let release_id: Option<i64> = row.get(4).map_err(|source| self.err(source))?;
            each(&AcceptedCandidate {
                variant_key: &variant_key,
                source: &src,
                game: &game,
                chinese: chinese.as_deref().and_then(chinese_mark),
                release_id,
            });
        }
        Ok(())
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
        let one_of = |sql: &str, arg: &str| -> Result<u64, CatalogError> {
            let value: i64 = self
                .conn
                .query_row(sql, params![arg], |row| row.get(0))
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
        counts.works = one_of(
            "SELECT COUNT(*) FROM work WHERE origin = ?1",
            Provenance::Identified.label(),
        )?;
        counts.releases = one_of(
            "SELECT COUNT(*) FROM release WHERE origin = ?1",
            Provenance::Identified.label(),
        )?;

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

    /// 按平台数**中文**：官中版几个、汉化版几个。
    ///
    /// 这是**发行版级**的统计（票 10 从挂账转来的那条前瞻）：数的是识别撞出来的候选
    /// 上带的那个记号，而不是文件名里有没有汉字。两者差得远——票 01 用文件名做的粗略
    /// 代理实测 15.6%，那个数里既有官中也有汉化，还混着一堆压根不是中文版的目录名。
    ///
    /// **官中版与汉化版分开数，不许加成一个「中文条目数」**（ADR-0012）：前者在卡带与
    /// 光盘世代是一次独立的**官方发行**（独立序列号、DAT 里独立一条、精确哈希直接命中），
    /// 后者是改过字节的**变体**。加成一个数，「TOSEC 与 GoodNES 补的正是官方库覆盖不到
    /// 的那部分」这个判断就彻底失真了。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn chinese_by_platform(
        &self,
        unknown: &str,
    ) -> Result<BTreeMap<String, (u64, u64)>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                // **平台为空的一行也要在里面**。滤掉它，按平台加出来的总数就与
                // `candidate_counts` 那个全局数对不上——同一份报告里两个中文总数，
                // 读的人无从判断哪个是真的。空平台交给调用方按它自己的「（未知）」归。
                "SELECT COALESCE(v.platform, ?1),
                        COUNT(DISTINCT CASE WHEN c.chinese = '官中' THEN c.variant_key END),
                        COUNT(DISTINCT CASE WHEN c.chinese = '汉化' THEN c.variant_key END)
                 FROM candidate c
                 JOIN variant v ON v.key = c.variant_key
                 GROUP BY 1",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![unknown], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    (
                        u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                        u64::try_from(row.get::<_, i64>(2)?).unwrap_or(0),
                    ),
                ))
            })
            .map_err(|source| self.err(source))?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (platform, counts) = row.map_err(|source| self.err(source))?;
            out.insert(platform, counts);
        }
        Ok(out)
    }

    /// 内部头与目录声明的平台对不上的**总数**（ADR-0011）。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn platform_conflict_count(&self) -> Result<u64, CatalogError> {
        let value: i64 = self
            .conn
            .query_row(
                &format!("SELECT COUNT(DISTINCT v.key) {CONFLICT_FROM}"),
                [],
                |row| row.get(0),
            )
            .map_err(|source| self.err(source))?;
        Ok(u64::try_from(value).unwrap_or(0))
    }

    /// **命中里只有 `source` 这一个源的候选**的变体数，按平台。判据见 [`NAME_ONLY_SQL`]。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn name_only_by_platform(
        &self,
        source: &str,
        unknown: &str,
    ) -> Result<BTreeMap<String, (u64, u64)>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(NAME_ONLY_SQL)
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![source, unknown, State::Matched.label()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                ))
            })
            .map_err(|source| self.err(source))?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (platform, count, blind) = row.map_err(|source| self.err(source))?;
            out.insert(
                platform,
                (
                    u64::try_from(count).unwrap_or(0),
                    u64::try_from(blind).unwrap_or(0),
                ),
            );
        }
        Ok(out)
    }

    /// 只改一条结论的**理由**那一列，别的一个字不动。
    ///
    /// **裁决**里「都不对，而且认不出」那一档走这条：结论本身没有变（照旧是未命中或
    /// 无判据），变的只是「为什么还停在这儿」。整条重写的话，那个变体的候选会被
    /// 连带清掉——而它们是识别撞出来的事实，与人认不认得出无关。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn set_identification_reason(
        &mut self,
        variant_key: &str,
        reason: Option<&str>,
    ) -> Result<bool, CatalogError> {
        self.conn
            .execute(
                "UPDATE identification SET reason = ?2 WHERE variant_key = ?1",
                params![variant_key, reason],
            )
            .map(|rows| rows > 0)
            .map_err(|source| self.err(source))
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
