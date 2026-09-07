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

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{OptionalExtension, Transaction, params};

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

-- **按内容判据反查**（票 05）：与 `container_entry_print` 同一条道理，见那一条上的说明。
-- 裸文件的判据落在这张表上，容器里那些落在 `container_entry` 上，反查要两张都问一遍。
CREATE INDEX IF NOT EXISTS content_hash_print ON content_hash(crc32, size);

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

-- 从一份 Switch 容器的**明文文件名表**里读出来的东西（票 27）。与 `content_disc`、
-- `content_cart` 同一条路：同样自带有效期、同样「读过的不白读」、同样加表而不加列。
--
-- `title_id` 与 `kind` 单独两列而不是只躺在 JSON 里，理由与 `content_cart.platform`
-- 一样：**报告要按它们数东西**。「这个库里有多少个本体、多少个补丁、多少份附属内容」
-- 是一条 SQL 数得出来的，不该逐行反序列化几万段 JSON 去数——而不数它，库体检就会把
-- 一堆更新包报成游戏（调研的实现陷阱第 7 条）。
CREATE TABLE IF NOT EXISTS content_switch(
    key      TEXT    NOT NULL,
    inner    TEXT    NOT NULL,
    len      INTEGER,
    mtime_ns INTEGER,
    -- `.tik` 的文件名说出来的 TitleID（16 位 hex 大写）；没有票据就是 NULL。
    title_id TEXT,
    -- 本体 / 补丁 / 附属内容，存的是 `identify::switch::Kind::code`。
    kind     TEXT,
    -- `identify::switch::Facts` 的 JSON。
    facts    TEXT    NOT NULL,
    PRIMARY KEY (key, inner)
) STRICT;

-- **模型推断问过的答案**（票 12）。识别管线里唯一花过钱的东西，所以它只花一次。
--
-- 它**不被 `clear_identifications` 清掉**，与 `candidate` / `identification` 那两张
-- 恰恰相反：那两张是识别每一趟重算的产物，清了再算一遍是免费的；这一张是**付过钱的**。
-- 清掉它等于把几十美元冲进下水道，而下一趟识别会一声不响地再付一遍。
--
-- 主键里有 `ask`（**提问指纹**：提示词版本 + 模型名 + 力度 + 问题正文）。换一个模型
-- 或改一版提示词，指纹就变，于是自动重问；只是重跑一遍识别，指纹一模一样，于是白拿。
CREATE TABLE IF NOT EXISTS model_answer(
    variant_key TEXT NOT NULL,
    ask         TEXT NOT NULL,
    -- 真的答话的那个模型（照抄响应里的 `model`，未必等于我们请求的那个）。
    model       TEXT NOT NULL,
    asked_at    INTEGER NOT NULL,
    -- `identify::model::Answer` 的 JSON，**原样留着**——事后复核靠它。
    answer      TEXT NOT NULL,
    PRIMARY KEY (variant_key, ask)
) STRICT;

-- **一次提问的账**（票 12）。一个请求一行。
--
-- 它与 `model_answer` 分开：那一张按变体记「答了什么」，这一张按请求记「花了多少」。
-- 合成一张的话，一个请求里打包的二十条会各背上二十分之一笔账——而那是编出来的数。
-- 有了它，「这一层到目前为止一共花了多少」是一条 SQL，不必翻日志。
CREATE TABLE IF NOT EXISTS model_call(
    id            INTEGER PRIMARY KEY,
    asked_at      INTEGER NOT NULL,
    model         TEXT    NOT NULL,
    -- 这一发里打包了几条。**批量打包这件事在库里也留得下证据。**
    packed        INTEGER NOT NULL,
    input_tokens  INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    -- 微美元。整数——一趟几百笔加起来正好是「有没有超上限」那个判断的输入。
    cost_micros   INTEGER NOT NULL
) STRICT;

-- 一**批**裁决落下之前，被它盖掉的那些**结论**（票 gui-redesign/08）。
-- 撤销靠它把中立库那一半原样放回去，**不必重跑识别、也不必 DAT 库在手边**。
--
-- ## 它凭什么不算「当场手改投影」
--
-- 中立库里 `origin = 裁决` 那几行是**沉淀库的投影**，而重算是它唯一的权威来路
-- （原挂账 D102）。这张表里存的**正是重算那条路自己的产物**——上一趟 `identify` 为这个
-- 变体算出来的结论与候选，一个字节都不是撤销现编的。撤销把它原样放回去，于是
-- 「重算一遍是什么样」与「眼下是什么样」照旧只有一个答案。
--
-- 反过来说才是撞车的那一侧：删掉裁决却把它投影出来的「命中」留在原地，那时两个答案
-- 才真的分了家——而那正是 D102 维持原样时中立库的样子。
--
-- ## 它凭什么住在中立库里
--
-- 里面装的每一样都**可再生**（识别重跑一遍就有），所以它按中立库的规矩活：
-- [`Catalog::clear_identifications`] 把它一起清掉。清掉之后那一批的中立库那一半就撤不
-- 回来了——那是实话，也不是损失：那时该做的本来就是再跑一趟识别。
--
-- 它**也跟着变体活**：每一行的主语是一个变体的键，那个变体没了（改名、挪目录）或者
-- 它的字节换了，这一行说的「之前」就不再是任何人的之前，随 `drop_variant_orphans` /
-- `drop_stale_conclusions` 一起收掉。留着它只会让撤销的账说「中立库那一半也回去了
-- （0 个变体）」，而那几份内容眼下是**还没识别**、不在队列里。
CREATE TABLE IF NOT EXISTS verdict_batch_shadow(
    -- 沉淀库里那一批的编号（`verdict::Batch::id`）。**编号由沉淀库发**——批本身住在
    -- 那边，因为被盖掉的旧裁决除了那儿没有第二份。
    batch       INTEGER NOT NULL,
    variant_key TEXT    NOT NULL,
    state       TEXT    NOT NULL,
    reason      TEXT,
    units       INTEGER NOT NULL,
    candidates  INTEGER NOT NULL,
    accepted    INTEGER NOT NULL,
    nkit        INTEGER NOT NULL,
    read_bytes  INTEGER NOT NULL,
    -- 那时候变体挂在哪个作品、哪次发行上。队列里的条目这两样都是空的（队列的判据
    -- 就是「一条自动通过的候选都没有」），存着是为了不去赌它。
    work_id     INTEGER,
    release_id  INTEGER,
    PRIMARY KEY (batch, variant_key)
) STRICT;

-- 那时候的**候选**，一条一行。列与 `candidate` 一一对应——放回去就是原样插回那张表。
--
-- `ordinal` 是当初那张表里的 `id`，只为**把次序定死**：候选的次序是有意义的
-- （`--pick 1` 挑的就是第一条），排序键漂一下，撤销之后同一条命令就挑中别人了。
CREATE TABLE IF NOT EXISTS verdict_batch_shadow_candidate(
    batch       INTEGER NOT NULL,
    variant_key TEXT    NOT NULL,
    ordinal     INTEGER NOT NULL,
    member_key  TEXT    NOT NULL,
    inner       TEXT    NOT NULL,
    confidence  TEXT    NOT NULL,
    accepted    INTEGER NOT NULL,
    source      TEXT    NOT NULL,
    dat         TEXT    NOT NULL,
    platform    TEXT    NOT NULL,
    game        TEXT    NOT NULL,
    rom         TEXT    NOT NULL,
    hashing     TEXT    NOT NULL,
    convention  TEXT    NOT NULL,
    evidence    TEXT    NOT NULL,
    chinese     TEXT,
    serial      TEXT,
    release_id  INTEGER,
    PRIMARY KEY (batch, variant_key, ordinal)
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

/// **置信度那四档**：三档置信度，加上「一条候选都没有」那一档（ADR-0002）。
///
/// [`Confidence`] 只有三档，因为它是**一条候选**的属性——而屏上要标出来的是**一个变体**
/// 或**一批变体**，那里第四种情形是实打实存在的：一条候选都没有。真机上队列里
/// 绝大多数条目正落在这一档，把它并进「低置信」等于说「工具猜了一下但不太确信」，
/// 而实情是工具**一个字都没说**。
///
/// 收在核心库里而不是各屏各写一份：五屏都要标这四档，含义漂开一点点，用户就再也
/// 认不出颜色的意思。**票 `gui-redesign/12` 把界面那一侧全换到了这一份上**——
/// 那个词只有 [`Tier::label`] 一处写，那个颜色只有 `romcat_gui::look::tier_color` 一处写。
///
/// **浏览那一侧多问一句「跑过没跑过」**（票 `gui-redesign/17`）：
/// [`WorkRow::confidence_label`](super::browse::WorkRow::confidence_label) 与
/// [`WorkVariant::confidence_label`](super::browse::WorkVariant::confidence_label)
/// 在一条候选都没有时先分辨那是这一档（**没有候选**）还是 [`NOT_RUN_LABEL`]
/// （**还没识别**），分不出来就得对着一整批压根没识别过的变体说「识别跑过了、没找着」。
/// 队列那一侧不必问：进得了队列的条目**全都跑过识别**。
///
/// 五屏里真会标置信度的是**浏览**与**待确认**两屏：库屏摆的是根与数据源、子库屏摆的是
/// 设备与差量、任务屏摆的是队列与历史，那三屏上没有一个变体级的结论可标。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    /// **高置信**：精确哈希命中。
    High,
    /// **中置信**：撞上了但有保留。
    Medium,
    /// **低置信**：文件名规则、模糊匹配与模型推断那几层的产物。
    Low,
    /// **没有候选**：一条候选都没有，谈不上置信度。它不是「猜得不准」，是**一个字都没说**。
    ///
    /// ⚠️ 这一档**曾经也叫「还没识别」**，与词表里那条「**还没识别**：一个变体连识别都
    /// 还没跑过」撞词。撞词在待确认屏上撞出了一个真的错账：屏头同时要说这一档有几条、
    /// 又要说库里**连识别都没跑过**的有几个（[`NOT_RUN_LABEL`]），两句话写的是同四个字、
    /// 数的是两回事，于是后一句一直没人敢加。**给这一档另起一个词**——改成「没有候选」，
    /// 两句话这才并得进同一屏。词表**两个词都收了**（`CONTEXT.md` 的**还没识别**与
    /// **没有候选**两条），这儿印的是后一条。
    ///
    /// 前一条在核心里落成 [`NOT_RUN_LABEL`] 与各处的 `not_run` 计数
    /// （[`Catalog::not_run_count`](super::Catalog::not_run_count)）。**这一档里的条目
    /// 全都跑过识别**——它们进得了[待确认队列](crate::triage)，只是一条候选都没有；
    /// 那一条里的变体**连队列都进不去**，因为库里根本没有它们的结论。
    ///
    /// **两个词在五屏上分开印**（票 `gui-redesign/17`）：待确认屏的屏头把两个数并排说，
    /// 浏览屏那一栏与详情面板的变体行先问一句「跑过没跑过」再挑词
    /// （[`WorkRow::identified`](super::browse::WorkRow::identified)、
    /// [`WorkVariant::state`](super::browse::WorkVariant::state)）。
    Unidentified,
}

impl Tier {
    /// 四档全在这儿，屏上照这个次序摆。
    pub const ALL: [Self; 4] = [Self::High, Self::Medium, Self::Low, Self::Unidentified];

    /// 从一条候选的置信度折过来；`None` 就是**没有候选**。
    #[must_use]
    pub fn of(confidence: Option<Confidence>) -> Self {
        match confidence {
            Some(Confidence::High) => Self::High,
            Some(Confidence::Medium) => Self::Medium,
            Some(Confidence::Low) => Self::Low,
            None => Self::Unidentified,
        }
    }

    /// 打给用户的那个词。前三档与 [`Confidence::label`] **逐字一样**——
    /// 同一件事在两处写成两个词，用户会以为那是两件事。
    ///
    /// 第四档**故意不叫 [`NOT_RUN_LABEL`]**：那四个字归词表那条「连识别都还没跑过」，
    /// 而这一档说的是「跑过了，只是一条候选都没有」。同屏要把两个数并排说出来
    /// （待确认屏的屏头），它们就不能是同一串字。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::High => Confidence::High.label(),
            Self::Medium => Confidence::Medium.label(),
            Self::Low => Confidence::Low.label(),
            Self::Unidentified => "没有候选",
        }
    }
}

/// **还没识别**打给用户的那个词（`CONTEXT.md` 的「还没识别」条）。
///
/// 说的是**一个变体连识别都还没跑过**——中立库里它一行 `identification` 都没有。
/// 它不是 [`State`] 的第五档：那个枚举装的是**识别跑完留下的结论**，而这里说的是
/// 「这一趟还没轮到它」，库里根本没有那一行可存。报告与队列因此把它当作一个**计数**
/// 来处理（`not_run`），不进 `identification` 那张表。
///
/// 三者分开数才有意义（词表原话）：
///
/// - **未命中**：撞过没撞上，是**结论**。
/// - **无判据**：拿不到可撞的东西，也是**结论**。
/// - **还没识别**：一个字都还没说。
///
/// 混在一起，命中率就失真——真机上那正是「4 个变体识别完、又扫进 1 个新文件」之后
/// 报告说「变体 4」的那个坑：第 5 个连分母都进不去，覆盖率虚高。
///
/// **与 [`Tier::Unidentified`] 分得开**：那一档从前也叫这四个字，说的却是
/// 「这一格没有置信度可标」（一条候选都没有，但**识别跑过了**），同屏并排说两个数时
/// 撞在一起。现在那一档叫「**没有候选**」，五屏同用一个词（`Tier::label`），
/// 词表两条各立一条（`CONTEXT.md` 的**还没识别**与**没有候选**）。
///
/// **筛选器与浏览那一侧也认这个常量**：
/// [`StateFilter::Unidentified`](super::browse::StateFilter::Unidentified) 的词、
/// 主列表那一栏（[`WorkRow::confidence_label`](super::browse::WorkRow::confidence_label)）
/// 与详情面板的变体行都从这儿取——各处各抄一遍那四个字的话，改一处、断一处，
/// 而断了没有一条编译错误会说话（票 `gui-redesign/17`）。
pub const NOT_RUN_LABEL: &str = "还没识别";

/// 一个变体这一轮识别的结论。
///
/// **它只有跑过识别的变体才有。** 一个变体连识别都还没跑过时，库里一行都没有——
/// 那是[还没识别](NOT_RUN_LABEL)，不是这里的第五档。
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
    /// 四档全在这儿。
    ///
    /// **点名一条变体时四档全看**：用户已经说出它的键了，再拿默认那三档把它筛掉
    /// 只会让人以为库里没有这个变体。界面上的「结论」筛选也照这个次序摆。
    pub const ALL: [Self; 4] = [
        Self::Matched,
        Self::Unmatched,
        Self::NoEvidence,
        Self::Skipped,
    ];

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
                        WHERE c.variant_key = i.variant_key
                          AND instr(?1, ',' || c.source || ',') = 0)
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

/// 一份 Switch 容器探出来的东西，写库前的样子。
///
/// 与 [`DiscFactRow`] 同一个道理捏成一个类型：几样都是 `String`，元组里写错顺序
/// 编译器不会说话。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchFactRow {
    /// 是变体的哪个成员：容器的键，或者裸文件自己的键。
    pub key: String,
    /// **透明容器**内部路径；裸文件是空串。
    pub inner: String,
    /// `.tik` 说出来的 TitleID；没有票据就是 `None`。
    pub title_id: Option<String>,
    /// 本体 / 补丁 / 附属内容的短码。
    pub kind: Option<String>,
    /// `identify::switch::Facts` 的 JSON。
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

/// 逐条走[还没识别](NOT_RUN_LABEL)的变体时收到的那两样：平台、变体的键。
///
/// **只有这两样**：它们连一行结论都没有，结论、理由、读了多少字节这几样都无从谈起
/// ——那正是「还没识别」与「未命中 / 无判据」的分界。报告拿键做的事与走结论那一趟
/// 一样（按平台归堆、数文件名里有没有汉字），所以键要给。
pub type NotRunVisitor<'a> = dyn FnMut(Option<&str>, &str) + 'a;

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
    ChineseMark::from_label(label)
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

/// **模型推断问过的一条答案**（票 12）。
///
/// 四样东西同进同出：哪个变体、哪一次提问、**谁答的**、答了什么。裸元组穿过读、写、
/// 缓存三处的话，写岔一个位置编译器一个字都不会说——而写岔 `model` 与 `answer` 的
/// 后果是把「谁答的」印成一段 JSON（同 `DiscFactRow` / `CartFactRow` 的道理）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelAnswerRow {
    /// 哪个变体。
    pub variant_key: String,
    /// **提问指纹**：提示词版本 + 模型名 + 力度 + 这个变体的身份。
    pub ask: String,
    /// **真答话的那个模型**，照抄响应里的 `model`——未必等于我们请求的那个。
    pub model: String,
    /// `identify::model::Answer` 的 JSON，原样留着。
    pub answer: String,
}

impl Catalog {
    /// 全部变体连它们这一轮的识别结论，按键排序。**待确认队列**的原料。
    ///
    /// [还没识别](NOT_RUN_LABEL)的变体**不在里面**：队列说的是「识别拿不定主意的
    /// 那些」，而一条结论都没有的变体谈不上拿不拿得定——它们一条候选都没有，裁不了。
    ///
    /// **但它们得有人报数**，否则「队列 2 条」会被读成「库里只剩 2 条没定下来」。
    /// 那个数走 [`not_run_count`](Self::not_run_count)，队列拿它在旁边说一句
    /// 「另有 N 个还没识别，先跑一趟 `romcat identify`」。
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

    /// **哪些变体装着这份内容**：拿内容判据（含头的 CRC-32 加大小）反查变体的键。
    ///
    /// ## 为什么要有反着走的这一条
    ///
    /// 一条**匹配裁决**钉在**内容锚**上（票 05），那正是「换台机器、改过名字仍然认得出」
    /// 的来处；而刮削那一侧手里只有变体的键。两头要接得上，只有两条路：
    ///
    /// - 为每个变体算一次内容判据（`identify::content_print`）——真库 46,444 个变体，
    ///   每个要查两次库。**这条路刮削那一侧本来就特意不走**（见 `scrape::Plan::build`
    ///   里那句「变体这一层不带判据」）。
    /// - 反过来，拿手里那几条裁决去问「谁装着这份内容」。裁决是**人一条条裁出来的**，
    ///   量级是几百到几千，与变体数不同阶。
    ///
    /// 走后者。两张表都要问：**裸文件**的判据落在 `content_hash` 上，**透明容器**里那些
    /// 零解压就有的落在 `container_entry` 上。
    ///
    /// **返回的是「某个成员正好是这份内容」的那些变体**，不是「代表这个变体的那份内容
    /// 正好是它」。两者的差别只在一种情况下现形：一个变体里某个**附属**成员与另一个
    /// 变体的锚撞了同一个 CRC-32 加大小（同一份说明文件是最可能的那一种）。调用方要自己
    /// 拿 `identify::content_print` 复核一遍——那一层才是「谁代表这个变体」的唯一说法。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn variants_with_content(
        &self,
        crc32: u32,
        size: u64,
    ) -> Result<Vec<String>, CatalogError> {
        let size = i64::try_from(size).unwrap_or(i64::MAX);
        let crc32 = i64::from(crc32);
        let mut statement = self
            .conn
            .prepare_cached(
                "SELECT DISTINCT variant_key FROM variant_member
                 WHERE key IN (
                     SELECT key FROM content_hash    WHERE crc32 = ?1 AND size = ?2
                     UNION
                     SELECT key FROM container_entry WHERE crc32 = ?1 AND size = ?2
                 )
                 ORDER BY variant_key",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![crc32, size], |row| row.get::<_, String>(0))
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
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

    /// 某个成员上探过的 **Switch 容器**：内部路径 → 那一份。裸文件的内部路径是空串。
    ///
    /// 与 [`cart_facts`](Self::cart_facts) 同一条路，**自带有效期**。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn switch_facts(&self, key: &str) -> Result<BTreeMap<String, String>, CatalogError> {
        let mut statement = self
            .conn
            .prepare_cached(
                "SELECT s.inner, s.facts FROM content_switch s
                 JOIN entry e ON e.key = s.key
                 WHERE s.key = ?1
                   AND s.len IS e.len AND s.mtime_ns IS e.mtime_ns",
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

    /// 把探出来的 Switch 容器事实整批存下来。有效期那两列从 `entry` 上现取。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put_switch_facts(&mut self, rows: &[SwitchFactRow]) -> Result<(), CatalogError> {
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
                    "INSERT INTO content_switch(key, inner, len, mtime_ns, title_id, kind, facts)
                     VALUES(?1, ?2,
                            (SELECT len FROM entry WHERE key = ?1),
                            (SELECT mtime_ns FROM entry WHERE key = ?1),
                            ?3, ?4, ?5)
                     ON CONFLICT(key, inner) DO UPDATE SET
                        len = excluded.len, mtime_ns = excluded.mtime_ns,
                        title_id = excluded.title_id, kind = excluded.kind,
                        facts = excluded.facts",
                )
                .map_err(to_err)?;
            for row in rows {
                insert
                    .execute(params![
                        row.key,
                        row.inner,
                        row.title_id,
                        row.kind,
                        row.facts
                    ])
                    .map_err(to_err)?;
            }
        }
        tx.commit().map_err(to_err)
    }

    /// 一共读出了几份 Switch 容器的**明文文件名表**。
    ///
    /// 它是 [`switch_kinds`](Self::switch_kinds) 的分母：那几个数只有带 `.tik` 的容器
    /// 说得出，不摆分母，读者会以为「本体 16」是全部。
    ///
    /// **口径与 [`switch_facts`](Self::switch_facts) 一模一样**：连 `entry` 再对一遍
    /// 这一行自带的有效期。这张表建来就写着「不依赖任何人记得去作废它」，那句话只有
    /// 在**每一个**读它的地方都对一遍有效期时才成立——一处对、一处不对，同一份库上
    /// 报告数出来的份数就与识别真正拿得到的事实各说各话。
    ///
    /// 读不到元数据的条目不会被这一条漏掉：它的名字 `readdir` 列得出来，`entry` 里
    /// 那一行还在，而扫描绝不用「不可读」覆盖上次读到的大小与时间（ADR-0021）。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn switch_read(&self) -> Result<u64, CatalogError> {
        self.conn
            .query_row(
                "SELECT count(*) FROM content_switch s
                 JOIN entry e ON e.key = s.key
                 WHERE s.len IS e.len AND s.mtime_ns IS e.mtime_ns",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|it| u64::try_from(it).unwrap_or(0))
            .map_err(|source| self.err(source))
    }

    /// **Switch 的内容分布**：本体 / 补丁 / 附属内容各有多少份。
    ///
    /// 报告从中立库折出来、不重跑识别（ADR-0001），所以这件事是一条 SQL 而不是攒在
    /// 一趟识别的内存里。不摆出这个数，库体检会把一堆更新包报成游戏。
    ///
    /// 口径同 [`switch_read`](Self::switch_read)——它是这几个数的分母，两者对不上
    /// 就是报告自己跟自己打架。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn switch_kinds(&self) -> Result<Vec<(String, u64)>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT s.kind, count(*) FROM content_switch s
                 JOIN entry e ON e.key = s.key
                 WHERE s.kind IS NOT NULL
                   AND s.len IS e.len AND s.mtime_ns IS e.mtime_ns
                 GROUP BY s.kind ORDER BY 2 DESC, 1",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|source| self.err(source))?;
        let mut out = Vec::new();
        for row in rows {
            let (kind, count) = row.map_err(|source| self.err(source))?;
            out.push((kind, u64::try_from(count).unwrap_or(0)));
        }
        Ok(out)
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

    /// 把这些变体**眼下的结论**收进一批的快照里。批量裁决落下之前走这一步。
    ///
    /// 收的是结论、候选与变体身上那两条链接——也就是这一批马上要盖掉的全部东西。
    /// 收进来的是**上一趟识别自己算出来的**那份，不是撤销现编的（见
    /// `verdict_batch_shadow` 上的说明）。
    ///
    /// 同一批同一个变体收两次是**覆盖**：一批里一个变体只该有一份快照。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn stash_conclusions(&mut self, batch: i64, keys: &[&str]) -> Result<u64, CatalogError> {
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        let mut stashed = 0;
        {
            let mut one = tx
                .prepare(
                    "INSERT OR REPLACE INTO verdict_batch_shadow(batch, variant_key, state,
                         reason, units, candidates, accepted, nkit, read_bytes,
                         work_id, release_id)
                     SELECT ?1, i.variant_key, i.state, i.reason, i.units, i.candidates,
                            i.accepted, i.nkit, i.read_bytes, v.work_id, v.release_id
                     FROM identification i JOIN variant v ON v.key = i.variant_key
                     WHERE i.variant_key = ?2",
                )
                .map_err(to_err)?;
            let mut drop_old = tx
                .prepare(
                    "DELETE FROM verdict_batch_shadow_candidate
                     WHERE batch = ?1 AND variant_key = ?2",
                )
                .map_err(to_err)?;
            let mut candidates = tx
                .prepare(
                    "INSERT INTO verdict_batch_shadow_candidate(batch, variant_key, ordinal,
                         member_key, inner, confidence, accepted, source, dat, platform,
                         game, rom, hashing, convention, evidence, chinese, serial, release_id)
                     SELECT ?1, variant_key, id, member_key, inner, confidence, accepted,
                            source, dat, platform, game, rom, hashing, convention, evidence,
                            chinese, serial, release_id
                     FROM candidate WHERE variant_key = ?2",
                )
                .map_err(to_err)?;
            for key in keys {
                stashed +=
                    u64::try_from(one.execute(params![batch, key]).map_err(to_err)?).unwrap_or(0);
                drop_old.execute(params![batch, key]).map_err(to_err)?;
                candidates.execute(params![batch, key]).map_err(to_err)?;
            }
        }
        tx.commit().map_err(to_err)?;
        Ok(stashed)
    }

    /// 一批的快照里还剩几个变体。**撤销之前问它**：为 0 就是这一批的中立库那一半
    /// 已经回不去了，那时只回滚得了沉淀库那一半，该如实说出来。
    ///
    /// 两条来路：重跑识别把它整批清掉了（[`Catalog::clear_identifications`]），
    /// 或者这几个变体改过名、挪过位置、字节换过，快照跟着旧键一起作废了
    /// （`drop_variant_orphans` / `drop_stale_conclusions`）。
    ///
    /// **它数的是「快照还在几个变体上」，不是「撤销放得回去几个」**：混着读，一批里
    /// 一半变体改过名时账就说得比做到的多。放回去了几个由
    /// [`Catalog::restore_conclusions`] 的返回值说了算。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn stashed(&self, batch: i64) -> Result<u64, CatalogError> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM verdict_batch_shadow WHERE batch = ?1",
                params![batch],
                |row| row.get::<_, i64>(0),
            )
            .map(|value| u64::try_from(value).unwrap_or(0))
            .map_err(|source| self.err(source))
    }

    /// 把一批的快照原样放回去：结论、候选、变体身上那两条链接。返回放回了几个变体。
    ///
    /// **只放回点名的那几个变体**。撤销时有些条撤不动（同一条锚上后来有人重新裁过，
    /// 那是别人的账），那几个变体的结论一个字都不该动——它们眼下的样子是那条新裁决的
    /// 投影，拿一份更老的快照盖上去才是真的改坏了。
    ///
    /// **不删快照**：撤销本身要撤得回来，撤回去之后还能再撤一次，靠的就是它还在。
    ///
    /// 顺手把这一批建出来、如今没人再指着的**作品**与**发行版**收掉。那不是额外的清理，
    /// 是对齐权威那条路：重跑一趟识别会把这两张表整个清掉再重建，那时这几行本来就不在。
    /// 收的范围**只限这几个变体刚才指着的那几行**——别人的孤行不归这一次撤销管。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn restore_conclusions(&mut self, batch: i64, keys: &[&str]) -> Result<u64, CatalogError> {
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        // 先记下这几个变体眼下指着谁——放回去之后它们就不指了，那时才好问「还有人指吗」。
        let mut works: BTreeSet<i64> = BTreeSet::new();
        let mut releases: BTreeSet<i64> = BTreeSet::new();
        {
            let mut statement = self
                .conn
                .prepare("SELECT work_id, release_id FROM variant WHERE key = ?1")
                .map_err(to_err)?;
            for key in keys {
                let row = statement
                    .query_row(params![key], |row| {
                        Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?))
                    })
                    .optional()
                    .map_err(to_err)?;
                if let Some((work, release)) = row {
                    works.extend(work);
                    releases.extend(release);
                }
            }
        }
        let tx = self.conn.transaction().map_err(to_err)?;
        let mut restored = 0;
        {
            // 顺序是**从引用方往被引用方**走，与 `clear_identifications` 同一条道理：
            // 外键是开着的，先删被指着的那一行会当场报错。
            let mut drop_candidates = tx
                .prepare("DELETE FROM candidate WHERE variant_key = ?1")
                .map_err(to_err)?;
            let mut put_identification = tx
                .prepare(
                    "INSERT INTO identification(variant_key, state, reason, units, candidates,
                         accepted, nkit, read_bytes)
                     SELECT s.variant_key, s.state, s.reason, s.units, s.candidates,
                            s.accepted, s.nkit, s.read_bytes
                     FROM verdict_batch_shadow s
                     WHERE s.batch = ?1 AND s.variant_key = ?2
                       AND EXISTS(SELECT 1 FROM variant v WHERE v.key = s.variant_key)
                     ON CONFLICT(variant_key) DO UPDATE SET
                        state = excluded.state, reason = excluded.reason,
                        units = excluded.units, candidates = excluded.candidates,
                        accepted = excluded.accepted, nkit = excluded.nkit,
                        read_bytes = excluded.read_bytes",
                )
                .map_err(to_err)?;
            // **链接先摘、行后删**，与 `clear_identifications` 同序。指向已经不在的
            // 作品或发行版时落空，而不是把一个悬空的 id 写回去。
            let mut relink = tx
                .prepare(
                    "UPDATE variant SET
                         work_id = (SELECT w.id FROM work w WHERE w.id =
                             (SELECT s.work_id FROM verdict_batch_shadow s
                              WHERE s.batch = ?1 AND s.variant_key = variant.key)),
                         release_id = (SELECT r.id FROM release r WHERE r.id =
                             (SELECT s.release_id FROM verdict_batch_shadow s
                              WHERE s.batch = ?1 AND s.variant_key = variant.key))
                     WHERE key = ?2",
                )
                .map_err(to_err)?;
            let mut put_candidates = tx
                .prepare(
                    "INSERT INTO candidate(variant_key, member_key, inner, confidence, accepted,
                         source, dat, platform, game, rom, hashing, convention, evidence,
                         chinese, serial, release_id)
                     SELECT c.variant_key, c.member_key, c.inner, c.confidence, c.accepted,
                            c.source, c.dat, c.platform, c.game, c.rom, c.hashing, c.convention,
                            c.evidence, c.chinese, c.serial,
                            (SELECT r.id FROM release r WHERE r.id = c.release_id)
                     FROM verdict_batch_shadow_candidate c
                     WHERE c.batch = ?1 AND c.variant_key = ?2
                       AND EXISTS(SELECT 1 FROM variant v WHERE v.key = c.variant_key)
                     ORDER BY c.ordinal",
                )
                .map_err(to_err)?;
            for key in keys {
                drop_candidates.execute(params![key]).map_err(to_err)?;
                restored += put_identification
                    .execute(params![batch, key])
                    .map_err(to_err)?;
                relink.execute(params![batch, key]).map_err(to_err)?;
                put_candidates
                    .execute(params![batch, key])
                    .map_err(to_err)?;
            }
            for id in &releases {
                tx.execute(
                    "DELETE FROM release WHERE id = ?1 AND origin = ?2
                     AND NOT EXISTS(SELECT 1 FROM variant WHERE release_id = ?1)
                     AND NOT EXISTS(SELECT 1 FROM candidate WHERE release_id = ?1)",
                    params![id, Provenance::Verdict.label()],
                )
                .map_err(to_err)?;
            }
            for id in &works {
                tx.execute(
                    "DELETE FROM work WHERE id = ?1 AND origin = ?2
                     AND NOT EXISTS(SELECT 1 FROM variant WHERE work_id = ?1)
                     AND NOT EXISTS(SELECT 1 FROM release WHERE work_id = ?1)",
                    params![id, Provenance::Verdict.label()],
                )
                .map_err(to_err)?;
            }
        }
        tx.commit().map_err(to_err)?;
        Ok(u64::try_from(restored).unwrap_or(0))
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
    /// ## 那几批的**快照**也一起清
    ///
    /// `verdict_batch_shadow` 装的是「一批裁决落下之前这几个变体是什么样」，而这一趟
    /// 重算把每个变体重新算了一遍——快照说的那个「之前」从此不再是任何人的现状。
    /// 留着它，撤销会把一份过期的结论盖回一份刚算出来的上面。
    ///
    /// 清掉的后果说清楚：**那几批只撤得回沉淀库那一半了**，中立库那一半要再跑一趟识别
    /// 才回到队列。批本身与它盖掉的旧裁决**一条都不受影响**——那些住在沉淀库里。
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
        for sql in [
            "DELETE FROM candidate",
            "DELETE FROM identification",
            "DELETE FROM verdict_batch_shadow_candidate",
            "DELETE FROM verdict_batch_shadow",
        ] {
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
    /// ⚠️ **它走的是结论，不是变体。** 连识别都还没跑过的变体这里一条都不出现
    /// ——库里本来就没有它们的行。**要「变体总数」的地方必须再走一趟
    /// [`for_each_not_run`](Self::for_each_not_run)**，否则那些变体会被整个抹掉：
    /// 命中率的分母少了它们，覆盖率就虚高（词表「还没识别」条、[`NOT_RUN_LABEL`]）。
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

    /// 一条条走过[还没识别](NOT_RUN_LABEL)的变体：平台、变体的键。
    ///
    /// 它是 [`for_each_identification`](Self::for_each_identification) 的**另一半**。
    /// 两半合起来正好是全部变体——报告的「变体总数」与「全部变体里命中多少」那个分母
    /// 必须走完两半，只走前一半就等于把还没轮到的变体从分母里抹掉，覆盖率当场虚高。
    ///
    /// 真机上这一半什么时候不空：识别被中断（没轮到的那些）、识别跑完之后又扫进了
    /// 新文件或加了新的**根**。识别每一趟起手都 `clear_identifications` 整批重算
    /// （`identify::run`），所以跑完一整趟之后这一半是空的。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn for_each_not_run(&self, each: &mut NotRunVisitor) -> Result<(), CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT v.platform, v.key FROM variant v
                 WHERE NOT EXISTS (SELECT 1 FROM identification i WHERE i.variant_key = v.key)
                 ORDER BY v.key",
            )
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let platform: Option<String> = row.get(0).map_err(|source| self.err(source))?;
            let key: String = row.get(1).map_err(|source| self.err(source))?;
            each(platform.as_deref(), &key);
        }
        Ok(())
    }

    /// 整个库里[还没识别](NOT_RUN_LABEL)的变体有多少个。
    ///
    /// **待确认队列**拿它说那句「另有 N 个还没识别」：那些变体一条候选都没有、裁不了，
    /// 所以它们不进队列、也不算待裁决（[`queue_rows`](Self::queue_rows)）；但把它们
    /// 一声不响地咽下去，用户就会以为库里只有队列里那些东西没定下来。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn not_run_count(&self) -> Result<u64, CatalogError> {
        let value: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM variant v
                 WHERE NOT EXISTS (SELECT 1 FROM identification i WHERE i.variant_key = v.key)",
                [],
                |row| row.get(0),
            )
            .map_err(|source| self.err(source))?;
        Ok(u64::try_from(value).unwrap_or(0))
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

    /// **命中里只有「一个字节都不读」那几个源的候选**的变体数，按平台。
    ///
    /// `sources` 是那几个源的名字，写成 `,中文离线源,Switch 文件名,` 这样两头带逗号的
    /// 一串——与 `content_cart.family` 同一个写法，为的是 SQL 里能用 `instr` 做整词
    /// 匹配。**它不止一个源**：票 11 的中文离线源与票 27 的 Switch 文件名层是同一件事
    /// ——都只看名字、都永不自动通过，而这一列存在的理由正是**不让「命中」两个字被
    /// 只看名字的层撑起来**。少数一个，那一列就会撒谎。
    ///
    /// 判据见 [`NAME_ONLY_SQL`]。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn name_only_by_platform(
        &self,
        sources: &str,
        unknown: &str,
    ) -> Result<BTreeMap<String, (u64, u64)>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(NAME_ONLY_SQL)
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![sources, unknown, State::Matched.label()], |row| {
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
    /// **模型推断问过的答案**，整份读回来（票 12）。
    ///
    /// 整份读而不是逐个变体查：残渣最多也就一万多条，一次读进内存是几 MB，而逐个查
    /// 是每个变体一次查询——识别那一趟本来就在为 46,444 个变体做别的事了。
    ///
    /// **`model` 那一列要一起读回来**：依据里印的必须是**真答话的那个模型**，
    /// 而请求的那个未必是它。少读这一列，同一条候选两趟会印出两个模型名。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn model_answers(&self) -> Result<Vec<ModelAnswerRow>, CatalogError> {
        let mut statement = self
            .conn
            .prepare("SELECT variant_key, ask, model, answer FROM model_answer")
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok(ModelAnswerRow {
                    variant_key: row.get(0)?,
                    ask: row.get(1)?,
                    model: row.get(2)?,
                    answer: row.get(3)?,
                })
            })
            .map_err(|source| self.err(source))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|source| self.err(source))?);
        }
        Ok(out)
    }

    /// 把这一批答案落库。**收到一个响应就落一次**——这一层每一条都付过钱，
    /// 攒到最后一次性写的话，跑到一半被 Ctrl-C 就等于把已经花掉的钱扔了。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put_model_answers(&mut self, rows: &[ModelAnswerRow]) -> Result<(), CatalogError> {
        if rows.is_empty() {
            return Ok(());
        }
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let now = crate::catalog::now_secs();
        let tx = self.conn.transaction().map_err(to_err)?;
        {
            let mut insert = tx
                .prepare(
                    "INSERT INTO model_answer(variant_key, ask, model, asked_at, answer)
                     VALUES(?1,?2,?3,?4,?5)
                     ON CONFLICT(variant_key, ask) DO UPDATE SET
                        model = excluded.model, asked_at = excluded.asked_at,
                        answer = excluded.answer",
                )
                .map_err(to_err)?;
            for row in rows {
                insert
                    .execute(params![
                        row.variant_key,
                        row.ask,
                        row.model,
                        now,
                        row.answer
                    ])
                    .map_err(to_err)?;
            }
        }
        tx.commit().map_err(to_err)
    }

    /// 记一次提问的账（票 12）。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put_model_call(
        &mut self,
        model: &str,
        packed: u64,
        input_tokens: u64,
        output_tokens: u64,
        cost_micros: u64,
    ) -> Result<(), CatalogError> {
        self.conn
            .execute(
                "INSERT INTO model_call(asked_at, model, packed, input_tokens, output_tokens,
                     cost_micros) VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    crate::catalog::now_secs(),
                    model,
                    i64::try_from(packed).unwrap_or(i64::MAX),
                    i64::try_from(input_tokens).unwrap_or(i64::MAX),
                    i64::try_from(output_tokens).unwrap_or(i64::MAX),
                    i64::try_from(cost_micros).unwrap_or(i64::MAX),
                ],
            )
            .map_err(|source| self.err(source))?;
        Ok(())
    }

    /// 这一层**到目前为止一共**发过几个请求、花了多少微美元（票 12）。
    ///
    /// 报告要它：一趟的花费只说得出这一趟，而「这个库上这一层一共烧了多少」是另一个
    /// 问题，也是决定下一趟设多少上限的那个数。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn model_spend(&self) -> Result<(u64, u64), CatalogError> {
        let (calls, micros): (i64, i64) = self
            .conn
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(cost_micros), 0) FROM model_call",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|source| self.err(source))?;
        Ok((
            u64::try_from(calls).unwrap_or(0),
            u64::try_from(micros).unwrap_or(0),
        ))
    }

    /// 往一条已有的结论上**追加候选**，别的一个字不动（票 12）。
    ///
    /// 与 [`write_identifications`](Self::write_identifications) 恰恰相反：那一条整条
    /// 重写（先 `DELETE FROM candidate`），这一条只加。模型推断那一层在识别的主循环
    /// **跑完之后**才问得出答案，那时结论早已落库；整条重写就要把 `state`、`reason`、
    /// `units`、`nkit`、`read_bytes` 全部再算一遍并原样写回，而其中任何一个写岔了都是
    /// 静默的坏账。
    ///
    /// **`state` 与 `reason` 一个字都不改**，这是有意的（见 `identify::model` 的模块
    /// 文档）：那两列上写着「rar 容器这一层还穿不透」这类真事实，让一句模型的猜测把它
    /// 覆盖掉是净损失。候选照样进**待确认队列**——队列的判据是「没有自动通过的候选、
    /// 而且不是跳过」，与状态无关。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn append_candidates(
        &mut self,
        variant_key: &str,
        candidates: &[Candidate],
    ) -> Result<(), CatalogError> {
        if candidates.is_empty() {
            return Ok(());
        }
        // **不许从这条路写进一条自动通过的候选。** 这一条通道是为模型推断开的，
        // 而 ADR-0002 说那一层永不自动通过；真要有自动通过的东西，它得走
        // `write_identifications`——那里会顺带把作品与发行版立起来，这里不会。
        debug_assert!(
            candidates.iter().all(|candidate| !candidate.accepted),
            "append_candidates 不接自动通过的候选"
        );
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        {
            let mut insert = tx
                .prepare(
                    "INSERT INTO candidate(variant_key, member_key, inner, confidence, accepted,
                         source, dat, platform, game, rom, hashing, convention, evidence,
                         chinese, serial, release_id)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
                )
                .map_err(to_err)?;
            for candidate in candidates {
                if candidate.accepted {
                    continue;
                }
                insert
                    .execute(params![
                        variant_key,
                        candidate.member_key,
                        candidate.inner,
                        candidate.confidence.label(),
                        0_i64,
                        candidate.source,
                        candidate.dat,
                        candidate.platform,
                        candidate.game,
                        candidate.rom,
                        candidate.hashed_as.label(),
                        candidate.dat_convention.label(),
                        candidate.evidence,
                        candidate.chinese.map(|mark| mark.label()),
                        candidate.serial,
                        candidate.release_id,
                    ])
                    .map_err(to_err)?;
            }
            // 队列按这一列决定要不要去把候选读出来，不加就等于白写。
            tx.execute(
                "UPDATE identification SET candidates = candidates + ?2 WHERE variant_key = ?1",
                params![variant_key, i64::try_from(candidates.len()).unwrap_or(0)],
            )
            .map_err(to_err)?;
        }
        tx.commit().map_err(to_err)
    }
}

/// 收掉**指不着任何变体**的识别结论与候选，连同它们独家撑着的作品与发行版。
///
/// ## 为什么落在这一步，而不是 `sweep` 或 `write`
///
/// 「条目没了」与「变体没了」是两层，作废也就分两处落。挂在**条目**（文件）上的那几张
/// 内容表由扫描收（[`Catalog::write`] 管文件变了、`Catalog::drop_orphans` 管文件没了）；
/// 识别这几张挂在**变体**上，而变体是成型算出来的——少一个文件不等于少一个变体
/// （三块 `.bin` 少一块，那个变体还在），所以只有重新成型之后才说得出「哪个变体真没了」。
/// 于是这一步跟着 [`Catalog::replace_variants`] 走：那是变体表唯一的写入口，
/// 在它那个事务里跑，收不干净就跟着一起回滚。
///
/// **「变体没了」与「变体的字节变了」还是两件事**：后者键一个字没变，落不进这张网，
/// 由 [`drop_stale_conclusions`] 在扫描写库那一步收——两者收的是同几张表，
/// 判据一个问「键还在不在」，一个问「那份字节还是不是原来那份」。
///
/// ## 为什么不怕把结论清光
///
/// 判据是**键还在不在**，不是「这一趟有没有重新成型」。成型只是把散落的文件重聚一遍，
/// 键没变的变体一条都落不进这张网——改一条成型规则不该把攒了半天的识别结论冲掉
/// （ADR-0022 那条「`work_id` / `release_id` 保住」是同一条道理的另一半）。
///
/// ## 作品与发行版凭什么也删得
///
/// 这两张表**每一行都可再生**：`origin = 识别` 的重跑一趟识别就有，`origin = 裁决` 的
/// 是**沉淀库**的投影、照那份库重放一遍就有（见本模块开头与
/// [`verdict`](crate::verdict) 的模块文档）。删掉一行不带走任何不可再生的东西。
/// 闸是「眼下还有没有人指着它」——留着没人指的那些，报告里「识别建出来的作品数」
/// 会一直虚高，浏览屏上还会长出指不着任何文件的行。这与
/// [`Catalog::restore_conclusions`] 收尾那两句是同一条路，只是那里按一批划范围，
/// 这里按整库——重新成型本来就是整库一遍的纯计算。
///
/// ## 一**批**裁决的**快照**也在这份清单里
///
/// `verdict_batch_shadow` 那两张表存的是「这一批落下之前，这几个变体是什么样」——
/// 说的是**某个变体的键**。那个键没了，那句话就没有主语了：同一份内容改过名、挪过目录
/// 之后，旧键的快照一行都放不回去（[`Catalog::restore_conclusions`] 插不进、改不动），
/// 而 [`Catalog::stashed`] 照旧数得出它，于是撤销的账上会说「中立库那一半也回去了
/// （0 个变体）」——那是许诺队列里有东西，而新键上那份内容是**还没识别**、压根不在队列里。
///
/// 它跟着这一步走的判据与上面几张表同一条：**可再生**（重跑一趟识别就有）、
/// **指不着任何变体**。批本身与它盖掉的旧裁决一条都不受影响，那些住在**沉淀库**里。
///
/// **`model_answer` 不在这份清单里**：那是唯一花过钱的一张表，键回来了还白拿一次
/// （见它自己那段表注释）。**人工纠正与合集成员也不在**：那两样明写着不随重新成型消失。
pub(super) fn drop_variant_orphans(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    // 顺序是**从引用方往被引用方**走，与 `clear_identifications` 同一条道理：
    // 外键是开着的，先删被指着的那一行会当场报错。
    tx.execute(
        "DELETE FROM candidate WHERE variant_key NOT IN (SELECT key FROM variant)",
        [],
    )?;
    tx.execute(
        "DELETE FROM identification WHERE variant_key NOT IN (SELECT key FROM variant)",
        [],
    )?;
    tx.execute(
        "DELETE FROM verdict_batch_shadow_candidate
         WHERE variant_key NOT IN (SELECT key FROM variant)",
        [],
    )?;
    tx.execute(
        "DELETE FROM verdict_batch_shadow WHERE variant_key NOT IN (SELECT key FROM variant)",
        [],
    )?;
    drop_unheld_works(tx)
}

/// 这几个**条目**的字节变了，挂在它们所属**变体**上的识别结论就此作废。
///
/// ## 判据是「成员的三元组变了」
///
/// 扫描这一层拿得到的只有 `(路径, 大小, 修改时间)`——内容哈希要到识别那一趟才算，
/// 而它本身正是这一步要作废的东西之一。于是「内容变了」在这里就是
/// [`Verdict::Changed`](crate::catalog::Verdict)：变体的任一成员的三元组变了，
/// 这个变体那份结论就不再是「这份字节」的结论。
///
/// **成员集合变了不走这条路**：加进来的是一个新键，它这一趟还不属于任何变体；
/// 少掉的那个由重新成型之后的 [`drop_variant_orphans`] 按「键还在不在」收。
///
/// ## 为什么落在 [`Catalog::write`] 而不是 `replace_variants`
///
/// **只有这一处看得见「变体级的变化」。** 成型是键的纯函数（ADR-0022），它看得见
/// 哪个变体没了，看不见哪个变体的字节换了——`replace_variants` 拿到的是一份变体清单，
/// 里面没有「这一趟哪几个条目变了」。而扫描写库这一步手里正好有那份判断，
/// 与它作废 `content_hash` 那几张内容表是同一个分支、同一个事务。
///
/// **中断的扫描也走到这里**，那正是要的：成型与删除都只在完整扫完一遍之后跑，
/// 而这条结论已经确定对不上盘上的字节了，早一步作废好过在库里多躺半趟。
///
/// ## 作废哪几样，不作废哪几样
///
/// 走的是 `candidate`、`identification`，以及变体身上那两条 `work_id` / `release_id`
/// 链接——三样都是**按字节**得出来的，字节换了就都不算数（`CONTEXT.md` 的**识别**）。
/// 收尾照 [`drop_variant_orphans`] 那两句收一遍没人指的作品与发行版。
///
/// **一批裁决的快照跟着走**（`verdict_batch_shadow` 那两张表，与
/// [`drop_variant_orphans`] 收的是同几张表）：快照说的是「这一批落下之前这个变体是什么
/// 样」，而那句话说的是**旧字节**。留着它，撤销会拿一份对着旧字节算出来的结论盖在新
/// 字节上——正是这一步要治的那种「新字节配旧结论」。撤销那时改说「中立库那一半没回去，
/// 跑一趟 `romcat identify`」，那是实话。
///
/// **刮削的结论不走**：它锚在作品名或变体的键上（`CONTEXT.md` 的**锚点**），
/// 这一次变的是字节不是名字。作品真成了孤儿的话，它连同挂在上面的东西一起被收掉。
///
/// **沉淀库里那条裁决更不动**：那是人定的、不可再生的（ADR-0008），中立库里这几行
/// 只是它的投影。下一趟识别按锚重新投影一遍——新字节撞不上旧锚，本来就该撞不上。
pub(super) fn drop_stale_conclusions(
    tx: &Transaction<'_>,
    changed: &BTreeSet<String>,
) -> rusqlite::Result<()> {
    let mut variants: BTreeSet<String> = BTreeSet::new();
    {
        // 一个条目只属于一个变体（`variant_member.key` 就是主键），因此这是一次索引命中。
        let mut owner = tx.prepare("SELECT variant_key FROM variant_member WHERE key = ?1")?;
        for key in changed {
            if let Some(variant_key) = owner
                .query_row(params![key], |row| row.get::<_, String>(0))
                .optional()?
            {
                variants.insert(variant_key);
            }
        }
    }
    if variants.is_empty() {
        return Ok(());
    }
    {
        // 顺序与 `drop_variant_orphans` 同源：从引用方往被引用方走。
        let mut drop_candidates = tx.prepare("DELETE FROM candidate WHERE variant_key = ?1")?;
        let mut drop_identification =
            tx.prepare("DELETE FROM identification WHERE variant_key = ?1")?;
        let mut unlink =
            tx.prepare("UPDATE variant SET work_id = NULL, release_id = NULL WHERE key = ?1")?;
        let mut drop_shadow_candidates =
            tx.prepare("DELETE FROM verdict_batch_shadow_candidate WHERE variant_key = ?1")?;
        let mut drop_shadow =
            tx.prepare("DELETE FROM verdict_batch_shadow WHERE variant_key = ?1")?;
        for variant_key in &variants {
            drop_candidates.execute(params![variant_key])?;
            drop_identification.execute(params![variant_key])?;
            unlink.execute(params![variant_key])?;
            drop_shadow_candidates.execute(params![variant_key])?;
            drop_shadow.execute(params![variant_key])?;
        }
    }
    drop_unheld_works(tx)
}

/// 收掉眼下没人指着的作品与发行版。
///
/// 闸是「还有没有人指着它」，而不是「它是怎么来的」——这两张表每一行都可再生，
/// 判据写在 [`drop_variant_orphans`] 的「作品与发行版凭什么也删得」那一段。
fn drop_unheld_works(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute(
        "DELETE FROM release
         WHERE NOT EXISTS(SELECT 1 FROM variant v WHERE v.release_id = release.id)
           AND NOT EXISTS(SELECT 1 FROM candidate c WHERE c.release_id = release.id)",
        [],
    )?;
    tx.execute(
        "DELETE FROM work
         WHERE NOT EXISTS(SELECT 1 FROM variant v WHERE v.work_id = work.id)
           AND NOT EXISTS(SELECT 1 FROM release r WHERE r.work_id = work.id)",
        [],
    )?;
    Ok(())
}
