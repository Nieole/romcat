//! 中立库里的**子库**与**选择集**：一台目标设备一行，带它的规则与例外。
//!
//! ## 五张表各自回答一个问题
//!
//! - `sublibrary`：**这台设备是什么样的**——目标路径、前端格式、容量上限。
//! - `sublibrary_override`：**这台设备在哪几个平台上不听能力档案的**——按平台覆盖档案的结论
//!   （票 `gui-looks-like-the-design/21`），只影响这一个子库。
//! - `sublibrary_rule`：**要什么**，可重放的那一半。存的是**规则的原文**而不是求值
//!   结果——存结果的话「主库新增的内容下次自动进入」就不成立了，那正是规则存在的理由。
//! - `sublibrary_exception`：**另外还要 / 偏不要什么**，优先于规则的那一半。
//! - `sublibrary_manifest`：**上次往这台设备上放了哪些文件**——增量同步的依据，
//!   也是工具在目标设备上的行为边界（ADR-0015）。它由票 20 的执行写，票 19 的
//!   [`plan`](crate::sync::plan) 只读。
//!
//! ## 例外为什么不给变体挂外键
//!
//! 与 `collection_variant` 同一条理由：变体会消失又回来（盘没插、目录改了名、
//! 重新成型换了键的写法），而例外是**永久记住**的（ADR-0016）。挂上外键，第一次
//! 重新成型就会因为「指着一个不存在的变体」而写不进去，或者被连带删掉——那等于
//! 用户手挑的那几百个决定随一次成型规则调整蒸发。
//!
//! 指向库里没有的变体的例外，由 [`select`](crate::sublibrary::select) 如实报出来，
//! 不删也不当错。
//!
//! ## 清单为什么落在中立库而不是卡上
//!
//! 中立库是事实来源（ADR-0001），而子库的定义本来就在这儿；清单是子库的一部分，
//! 分家存没有道理。放卡上还有一个绕不开的怪圈：**那份清单自己也是工具写在目标上的
//! 一个文件**，于是它要不要记进自己里？记则自引用，不记则它成了「清单之外」而工具
//! 连自己的记录都不敢碰。挂账 D77 记着另一条路（卡自描述）。
//!
//! ## 结构版本没有加 1
//!
//! 这四张表是**纯加表**：已有的表一列没动、一条语义没改，`CREATE TABLE IF NOT EXISTS`
//! 在打开时就补上，旧库照样打得开。判据是「旧数据会不会被读错」而不是「文件里多了
//! 点东西」（见 [`SCHEMA_VERSION`](super::SCHEMA_VERSION) 与挂账 D50）。
//!
//! 票 `gui-looks-like-the-design/21` 加的 `sublibrary_override` 也是纯加表：老库上它是空的，读出来就是
//! 「一个平台都没覆盖」——那正是加它之前的唯一可能。
//!
//! 票 20 给这两张表各加了一列（`sublibrary.target_raw`、`sublibrary_manifest.absent`），
//! 判据仍是同一条：两列在老行上取得到的值与加它们之前的唯一可能完全一致，
//! 于是旧数据读不错。补列的活在 `add_columns` 里，那个函数的注释写着为什么。

use std::collections::BTreeMap;

use rusqlite::{OptionalExtension, params};

use super::{Catalog, CatalogError};
use crate::capability::Override;
use crate::path;
use crate::sublibrary::target::{NameRefusal, vet_name};
use crate::sublibrary::{
    Discarded, Exception, ExceptionDetail, ExceptionRow, LoadedSelection, Rule, StoredRule,
    Sublibrary,
};
use crate::sync::{FileKind, Manifest, ManifestFile, Stamp};

/// 子库与选择集的表。
pub(super) const SUBLIBRARY_SCHEMA: &str = "\
-- **子库**：从主库挑一部分导到某台目标设备形成的派生库（ADR-0015）。
-- 主键是名字：命令行拿它指名道姓，而一台设备换了挂载点名字不变——与中立库自己
-- 靠 `--library <名字>` 而不是绝对路径找回来是同一条道理（票 29）。
CREATE TABLE IF NOT EXISTS sublibrary(
    name      TEXT PRIMARY KEY,
    -- 目标设备上的子库根。一律走读卡器，因此这是本机上的一个绝对路径（ADR-0015）。
    -- 存 NFC 形式（ADR-0020）：它是**键**，也是报告里印出来的那一份。
    target    TEXT NOT NULL,
    -- 同一个目标根，**系统给的原始形式**——读盘走这一列（ADR-0020、挂账 D82）。
    -- 路径不是有效 UTF-8 时为 NULL，读的时候退回 `target`。
    -- 这一列由 `add_columns` 给老库补上，见那个函数的注释。
    target_raw TEXT,
    -- 前端格式（适配器名）。
    format    TEXT NOT NULL,
    -- 容量上限，字节；NULL 表示不设限。**超限不自动截断**（ADR-0016）。
    capacity  INTEGER,
    -- **能力档案**的名字（票 21、ADR-0017）：这台设备吃得下什么、这张卡放得下什么。
    -- NULL 是「没挑过」，走**不作声称**那一份——不转换、不检查。存的是名字而不是
    -- 档案本身：矩阵会过时、要能整份换掉，而子库不该跟着一起改。
    -- 这一列由 `add_columns` 给老库补上，见那个函数的注释。
    capability TEXT,
    -- 容量上限是不是**按设备容量**那一档（票 gui-looks-like-the-design/21）：1 是跟着设备总容量走，
    -- 这时 `capacity` 记的是上次连上时读到的总容量；0 是自定义，`capacity` 就是那个上限。
    -- 这一列由 `add_columns` 给老库补上：老行是 0，正是加它之前的唯一可能。
    capacity_by_device INTEGER NOT NULL DEFAULT 0,
    -- 下一条规则发几号。**只增不减**，于是删掉的号永不复用——见 sublibrary_rule。
    next_rule INTEGER NOT NULL,
    at        INTEGER NOT NULL
) STRICT;

-- **规则**：选择集里可重放的那一半。存原文，求值时现算。
-- `ordinal` 是给人用的序号（命令行按它删），由 `sublibrary.next_rule` 发号，
-- **同一个子库之内删掉的号永不复用**——不然用户照着上一份报告删 2 号，删掉的会是
-- 后来补进来的另一条规则，而报告上那两条长得完全不一样。
-- （删掉整个子库再同名重建是从 1 号重新开始：那时旧规则也一并没了，两份报告
-- 说的本来就不是同一个子库。）
CREATE TABLE IF NOT EXISTS sublibrary_rule(
    sublibrary TEXT    NOT NULL REFERENCES sublibrary(name),
    ordinal    INTEGER NOT NULL,
    text       TEXT    NOT NULL,
    at         INTEGER NOT NULL,
    PRIMARY KEY (sublibrary, ordinal)
) STRICT;

-- **例外**：手动增删的那一半，**优先于规则、永久记住**（ADR-0016）。
-- `variant_key` **刻意不挂外键**，见模块文档。
CREATE TABLE IF NOT EXISTS sublibrary_exception(
    sublibrary  TEXT    NOT NULL REFERENCES sublibrary(name),
    variant_key TEXT    NOT NULL,
    -- `收入` 或 `排除`。
    kind        TEXT    NOT NULL,
    -- 用户自己写的一句「为什么」；可空。口味半年后就想不起来了。
    note        TEXT,
    at          INTEGER NOT NULL,
    PRIMARY KEY (sublibrary, variant_key)
) STRICT;

-- **按平台覆盖**能力档案的结论（票 gui-looks-like-the-design/21、ADR-0017「用户可手动覆盖」）。
-- 只影响这一个子库：名册里那份档案不动，排计划时叠上去（`Profile::with_overrides`）。
CREATE TABLE IF NOT EXISTS sublibrary_override(
    sublibrary TEXT    NOT NULL REFERENCES sublibrary(name),
    -- 平台名，照变体上记的那个写法存；比的时候由矩阵折大小写。
    platform   TEXT    NOT NULL,
    -- `不转换` / `zip` / `裸文件`（`Override::code`）。
    choice     TEXT    NOT NULL,
    at         INTEGER NOT NULL,
    PRIMARY KEY (sublibrary, platform)
) STRICT;

-- **清单**：某个子库上次导出的完整记录（ADR-0015）。
-- 它是增量同步的依据，**也是工具在目标设备上的行为边界**——清单之外的文件一律不碰。
-- 于是这张表是同步计划里唯一能长出「删除」的地方。
CREATE TABLE IF NOT EXISTS sublibrary_manifest(
    sublibrary TEXT    NOT NULL REFERENCES sublibrary(name),
    -- **相对子库根**的路径，`/` 分隔、NFC（ADR-0015、ADR-0020）。
    -- 绝不存绝对路径：卡插到别的设备、盘符变了都不该让全库对不上。
    path       TEXT    NOT NULL,
    -- `ROM` / `媒体` / `元数据`。
    kind       TEXT    NOT NULL,
    -- 写完之后**从目标上读回来的**大小与修改时间，不是主库侧那份的。
    -- 于是 FAT32 那 2 秒的时间戳刻度不构成问题：两次读的是同一个被截断过的值。
    bytes      INTEGER NOT NULL,
    mtime_ns   INTEGER,
    -- 主库侧的键（媒体是媒体池里的键）。
    source     TEXT    NOT NULL,
    -- 放上去的**那一刻**主库侧那份的大小与修改时间。判「主库那份变了没有」用这两列，
    -- 不用上面那两列——目标上那份的戳与主库侧那份的根本不是一回事，票 21 转起格式来
    -- 连大小都会变。只比大小也不够：**原地改过、大小没变**的文件会被静默判成不用重传。
    source_bytes    INTEGER NOT NULL,
    source_mtime_ns INTEGER,
    -- 属于哪个变体。报告拿它把文件数折回用户认得的那个数。
    variant    TEXT    NOT NULL,
    -- **工具放过它，而维护者在目标设备上把它删了，工具没有补回。**
    -- 0 是「就在目标上」，1 是「知道它没了、也知道你没让我补」（挂账 D75）。
    -- 这一格不留的话，同步完清单里那条会整个消失，于是**下一趟它变成一条普通的新增
    -- 又长回来**——而用户故事 65 的原话是「在掌机上有意删掉的东西不会自己长回来」。
    absent     INTEGER NOT NULL DEFAULT 0,
    at         INTEGER NOT NULL,
    PRIMARY KEY (sublibrary, path)
) STRICT;
";

/// 给老表补上后面几张票加的那几列。
///
/// `CREATE TABLE IF NOT EXISTS` 对**已经存在**的表一个字都不改，于是加一列得单独走
/// 一趟 `ALTER TABLE`。判有没有走 `PRAGMA table_info`，因此重复调用是安全的。
///
/// **这不算结构版本加 1**（见 [`SCHEMA_VERSION`](crate::catalog::SCHEMA_VERSION)）：
/// 判据是「旧数据会不会被读错」。三列在老行上都取得到一个与从前完全一致的含义
/// ——`target_raw` 为 NULL 就是「没有原始形式，用 `target`」，`absent` 为 0 就是
/// 「它就在目标上」，`capability` 为 NULL 就是「没挑过能力档案，不转换也不检查」，
/// 那正是加这几列之前的唯一可能。反过来，旧版程序按列名取值，多几列它也照样读得动。
pub(super) fn add_columns(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    add_column(conn, "sublibrary", "target_raw", "TEXT")?;
    add_column(conn, "sublibrary", "capability", "TEXT")?;
    add_column(
        conn,
        "sublibrary",
        "capacity_by_device",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column(
        conn,
        "sublibrary_manifest",
        "absent",
        "INTEGER NOT NULL DEFAULT 0",
    )
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

/// **删掉之前整份留下来的一个子库**：它那一行、规则、例外、清单，**逐列原样**。
///
/// [`Catalog::take_sublibrary`] 交出来，[`Catalog::restore_sublibrary`] 原样放回去——界面上删掉一个子库之后
/// 提示条上那颗「撤销」靠的就是它（票 `gui-looks-like-the-design/20`，拿主意的人 2026-09-14 定）。
///
/// **存的是库里那几列本来的值，不是读成领域类型之后的样子**：规则的序号与下一个发几号（`next_rule`）、
/// 每一行记下的时刻、清单里认不出类别的那几行（[`Catalog::manifest`] 读的时候跳过它们）都得原样回去——
/// 读成 [`Manifest`] 再写回的话，那几行就悄悄没了，下一条规则也会从 1 号重新发。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedSublibrary {
    /// 子库叫什么（主键）。
    name: String,
    /// 目标根，NFC 那一份。
    target: String,
    /// 目标根，系统给的原始形式。
    target_raw: Option<String>,
    /// 前端格式。
    format: String,
    /// 容量上限，库里那一格的原值。
    capacity: Option<i64>,
    /// 能力档案的名字。
    capability: Option<String>,
    /// 容量上限是不是按设备容量那一档，库里那一格的原值。
    capacity_by_device: i64,
    /// 下一条规则发几号。
    next_rule: i64,
    /// 这一行记下的时刻。
    at: i64,
    /// 规则，按序号。
    rules: Vec<RemovedRule>,
    /// 例外，按变体的键。
    exceptions: Vec<RemovedException>,
    /// 清单，按路径。
    manifest: Vec<RemovedManifestFile>,
    /// 按平台覆盖，按平台。
    overrides: Vec<RemovedOverride>,
}

impl RemovedSublibrary {
    /// 删掉的是哪个子库。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 它的目标路径（NFC 那一份，也就是报告里印的那一份）。
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }
}

/// [`RemovedSublibrary`] 里的一条规则：`sublibrary_rule` 那几列。
#[derive(Debug, Clone, PartialEq, Eq)]
struct RemovedRule {
    /// 序号。
    ordinal: i64,
    /// 原文。
    text: String,
    /// 记下的时刻。
    at: i64,
}

/// [`RemovedSublibrary`] 里的一条例外：`sublibrary_exception` 那几列。
#[derive(Debug, Clone, PartialEq, Eq)]
struct RemovedException {
    /// 变体的键。
    variant_key: String,
    /// 方向，库里那个词的原样。
    kind: String,
    /// 用户写的那句「为什么」。
    note: Option<String>,
    /// 记下的时刻。
    at: i64,
}

/// [`Catalog::rename_sublibrary`] 怎么了。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Renamed {
    /// 改好了。新名与旧名一样时什么都不做，也算改好了。
    Done,
    /// 没有叫旧名的子库。
    Missing,
    /// 新名字不能用：空着、或者已被别的子库用了（[`vet_name`]）。一行都没动。
    Refused(NameRefusal),
}

/// [`RemovedSublibrary`] 里一条按平台覆盖：`sublibrary_override` 那几列。
#[derive(Debug, Clone, PartialEq, Eq)]
struct RemovedOverride {
    /// 平台名。
    platform: String,
    /// 库里那个词的原样——认不出的也留着。
    choice: String,
    /// 记下的时刻。
    at: i64,
}

/// [`RemovedSublibrary`] 里清单的一行：`sublibrary_manifest` 那几列。
#[derive(Debug, Clone, PartialEq, Eq)]
struct RemovedManifestFile {
    /// 相对子库根的路径。
    path: String,
    /// 类别，库里那个词的原样——认不出的也留着。
    kind: String,
    /// 目标上那份的大小。
    bytes: i64,
    /// 目标上那份的修改时间。
    mtime_ns: Option<i64>,
    /// 主库侧的键。
    source: String,
    /// 属于哪个变体。
    variant: String,
    /// 放上去那一刻主库侧那份的大小。
    source_bytes: i64,
    /// 放上去那一刻主库侧那份的修改时间。
    source_mtime_ns: Option<i64>,
    /// 工具放过、维护者删了。
    absent: i64,
    /// 记下的时刻。
    at: i64,
}

impl Catalog {
    /// 新建或改一个子库。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put_sublibrary(&mut self, sublibrary: &Sublibrary) -> Result<(), CatalogError> {
        let target = path::nfc(&sublibrary.target).into_owned();
        let capacity = sublibrary
            .capacity
            .map(|bytes| i64::try_from(bytes).unwrap_or(i64::MAX));
        self.conn
            .execute(
                // 改一个已有的子库**不碰 `next_rule`**：发号器是单调的，
                // 「改一次目标路径」不该让规则的号从头再来。
                "INSERT INTO sublibrary(
                     name, target, target_raw, format, capacity, capability, capacity_by_device,
                     next_rule, at)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8)
                 ON CONFLICT(name) DO UPDATE SET
                    target = excluded.target, target_raw = excluded.target_raw,
                    format = excluded.format,
                    capacity = excluded.capacity, capability = excluded.capability,
                    capacity_by_device = excluded.capacity_by_device,
                    at = excluded.at",
                params![
                    sublibrary.name,
                    target,
                    sublibrary.target_raw,
                    sublibrary.format,
                    capacity,
                    sublibrary.capability,
                    i64::from(sublibrary.capacity_by_device),
                    super::now_secs()
                ],
            )
            .map_err(|source| self.err(source))?;
        Ok(())
    }

    /// 读一个子库。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn sublibrary(&self, name: &str) -> Result<Option<Sublibrary>, CatalogError> {
        self.conn
            .query_row(
                "SELECT name, target, target_raw, format, capacity, capability, capacity_by_device
                 FROM sublibrary WHERE name = ?1",
                params![name],
                |row| {
                    Ok(Sublibrary {
                        name: row.get(0)?,
                        target: row.get(1)?,
                        target_raw: row.get(2)?,
                        format: row.get(3)?,
                        capacity: row
                            .get::<_, Option<i64>>(4)?
                            .and_then(|v| u64::try_from(v).ok()),
                        capability: row.get(5)?,
                        capacity_by_device: row.get::<_, i64>(6)? != 0,
                    })
                },
            )
            .optional()
            .map_err(|source| self.err(source))
    }

    /// 列出全部子库，按名字排。
    ///
    /// **一个主库上可以同时有好几个子库，互不干扰**：规则与例外都按名字分开存。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn sublibraries(&self) -> Result<Vec<Sublibrary>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT name, target, target_raw, format, capacity, capability, capacity_by_device
                 FROM sublibrary ORDER BY name",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok(Sublibrary {
                    name: row.get(0)?,
                    target: row.get(1)?,
                    target_raw: row.get(2)?,
                    format: row.get(3)?,
                    capacity: row
                        .get::<_, Option<i64>>(4)?
                        .and_then(|v| u64::try_from(v).ok()),
                    capability: row.get(5)?,
                    capacity_by_device: row.get::<_, i64>(6)? != 0,
                })
            })
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// **给一个子库改名**（票 `gui-looks-like-the-design/21`，拿主意的人 2026-09-15 定：照稿名字可改）。
    ///
    /// 子库按名字存：规则、例外、按平台覆盖、清单都挂在名字上。所以改名是**在一个事务里按新名放一行、把挂着的
    /// 几张表挪过去、再删掉旧的那一行**——目标、前端格式、容量上限、能力档案、下一条规则发几号、清单里「你删过、
    /// 工具记着不补」的那几格原样跟过去，下一趟差量预览与改名之前一模一样。放行一条不挪的，同步就会把自己放过的
    /// 文件当成清单之外，碰都不敢碰（ADR-0015）。
    ///
    /// 新名两头的空白去掉；空着、或者撞上别的子库时一行都不动，交回 [`Renamed::Refused`]（判据是 [`vet_name`]，
    /// 目标设置弹层当场判的也是它）。
    ///
    /// # Errors
    /// 读写库失败时返回错误。
    pub fn rename_sublibrary(&mut self, from: &str, to: &str) -> Result<Renamed, CatalogError> {
        if self.sublibrary(from)?.is_none() {
            return Ok(Renamed::Missing);
        }
        let to = to.trim();
        if let Err(refusal) = vet_name(self, Some(from), to)? {
            return Ok(Renamed::Refused(refusal));
        }
        if to == from {
            return Ok(Renamed::Done);
        }
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        // **先放新的那一行**：挂着的几张表有外键指着 `sublibrary(name)`，挪过去之前新名得已经在。
        // 这张表往后再加列，这一句的列表要跟着加——漏一列，改名就悄悄把那一格丢了。
        tx.execute(
            "INSERT INTO sublibrary(
                 name, target, target_raw, format, capacity, capability, capacity_by_device,
                 next_rule, at)
             SELECT ?2, target, target_raw, format, capacity, capability, capacity_by_device,
                    next_rule, at
             FROM sublibrary WHERE name = ?1",
            params![from, to],
        )
        .map_err(to_err)?;
        for table in [
            "sublibrary_rule",
            "sublibrary_exception",
            "sublibrary_override",
            "sublibrary_manifest",
        ] {
            // 表名是上面这几个写死的字面量，不来自外面。
            tx.execute(
                &format!("UPDATE {table} SET sublibrary = ?2 WHERE sublibrary = ?1"),
                params![from, to],
            )
            .map_err(to_err)?;
        }
        tx.execute("DELETE FROM sublibrary WHERE name = ?1", params![from])
            .map_err(to_err)?;
        tx.commit().map_err(to_err)?;
        Ok(Renamed::Done)
    }

    /// 删掉一个子库，连它的规则、例外与清单一起。返回它本来在不在。
    ///
    /// 例外跟着一起没，与「例外永久记住」不矛盾：**记的是这个子库的口味**，
    /// 子库都不要了，那些决定也就没有归属。
    ///
    /// 走的是 [`Self::take_sublibrary`] 那一条，只是不要交回来的那一份。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn remove_sublibrary(&mut self, name: &str) -> Result<bool, CatalogError> {
        Ok(self.take_sublibrary(name)?.is_some())
    }

    /// 删掉一个子库，**删之前把它整份留下来交回**（[`RemovedSublibrary`]）：它那一行、规则、例外、清单。
    /// 本来就不在时交回 `None`，什么都不删。
    ///
    /// 读与删在同一个事务里：读完、删之前别处又加了一条规则的话，那一条跟着删掉了却不在交回的那一份里，
    /// 撤销之后就少了它。
    ///
    /// # Errors
    /// 读写库失败时返回错误。
    pub fn take_sublibrary(
        &mut self,
        name: &str,
    ) -> Result<Option<RemovedSublibrary>, CatalogError> {
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        let row = tx
            .query_row(
                "SELECT name, target, target_raw, format, capacity, capability, next_rule, at,
                        capacity_by_device
                 FROM sublibrary WHERE name = ?1",
                params![name],
                |row| {
                    Ok(RemovedSublibrary {
                        name: row.get(0)?,
                        target: row.get(1)?,
                        target_raw: row.get(2)?,
                        format: row.get(3)?,
                        capacity: row.get(4)?,
                        capability: row.get(5)?,
                        next_rule: row.get(6)?,
                        at: row.get(7)?,
                        capacity_by_device: row.get(8)?,
                        rules: Vec::new(),
                        exceptions: Vec::new(),
                        manifest: Vec::new(),
                        overrides: Vec::new(),
                    })
                },
            )
            .optional()
            .map_err(to_err)?;
        let Some(mut removed) = row else {
            return Ok(None);
        };
        {
            let mut statement = tx
                .prepare(
                    "SELECT ordinal, text, at FROM sublibrary_rule
                     WHERE sublibrary = ?1 ORDER BY ordinal",
                )
                .map_err(to_err)?;
            let rows = statement
                .query_map(params![name], |row| {
                    Ok(RemovedRule {
                        ordinal: row.get(0)?,
                        text: row.get(1)?,
                        at: row.get(2)?,
                    })
                })
                .map_err(to_err)?;
            removed.rules = rows.collect::<Result<_, _>>().map_err(to_err)?;
        }
        {
            let mut statement = tx
                .prepare(
                    "SELECT variant_key, kind, note, at FROM sublibrary_exception
                     WHERE sublibrary = ?1 ORDER BY variant_key",
                )
                .map_err(to_err)?;
            let rows = statement
                .query_map(params![name], |row| {
                    Ok(RemovedException {
                        variant_key: row.get(0)?,
                        kind: row.get(1)?,
                        note: row.get(2)?,
                        at: row.get(3)?,
                    })
                })
                .map_err(to_err)?;
            removed.exceptions = rows.collect::<Result<_, _>>().map_err(to_err)?;
        }
        {
            let mut statement = tx
                .prepare(
                    "SELECT path, kind, bytes, mtime_ns, source, variant,
                            source_bytes, source_mtime_ns, absent, at
                     FROM sublibrary_manifest WHERE sublibrary = ?1 ORDER BY path",
                )
                .map_err(to_err)?;
            let rows = statement
                .query_map(params![name], |row| {
                    Ok(RemovedManifestFile {
                        path: row.get(0)?,
                        kind: row.get(1)?,
                        bytes: row.get(2)?,
                        mtime_ns: row.get(3)?,
                        source: row.get(4)?,
                        variant: row.get(5)?,
                        source_bytes: row.get(6)?,
                        source_mtime_ns: row.get(7)?,
                        absent: row.get(8)?,
                        at: row.get(9)?,
                    })
                })
                .map_err(to_err)?;
            removed.manifest = rows.collect::<Result<_, _>>().map_err(to_err)?;
        }
        {
            let mut statement = tx
                .prepare(
                    "SELECT platform, choice, at FROM sublibrary_override
                     WHERE sublibrary = ?1 ORDER BY platform",
                )
                .map_err(to_err)?;
            let rows = statement
                .query_map(params![name], |row| {
                    Ok(RemovedOverride {
                        platform: row.get(0)?,
                        choice: row.get(1)?,
                        at: row.get(2)?,
                    })
                })
                .map_err(to_err)?;
            removed.overrides = rows.collect::<Result<_, _>>().map_err(to_err)?;
        }
        // **先删子表**：外键检查默认是开着的（见 `catalog::content` 里那段注释）。
        for table in [
            "sublibrary_manifest",
            "sublibrary_exception",
            "sublibrary_rule",
            "sublibrary_override",
        ] {
            // 表名是上面这几个写死的字面量，不来自外面。
            tx.execute(
                &format!("DELETE FROM {table} WHERE sublibrary = ?1"),
                params![name],
            )
            .map_err(to_err)?;
        }
        tx.execute("DELETE FROM sublibrary WHERE name = ?1", params![name])
            .map_err(to_err)?;
        tx.commit().map_err(to_err)?;
        Ok(Some(removed))
    }

    /// 把 [`Self::take_sublibrary`] 交出来的那一份**原样放回去**：那一行、规则（连序号与下一个发几号）、
    /// 例外、清单，逐列与删之前一样。放回去了交回 `true`。
    ///
    /// **同名的子库这会儿已经又有了**（删完之后人又建了一个同名的）时**一行都不写**，交回 `false`：
    /// 两份揉在一起的话谁的规则、谁的清单都说不清——清单说错了，同步就会去删不是自己放的文件（ADR-0015）。
    ///
    /// # Errors
    /// 读写库失败时返回错误。
    pub fn restore_sublibrary(
        &mut self,
        removed: &RemovedSublibrary,
    ) -> Result<bool, CatalogError> {
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        let taken: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sublibrary WHERE name = ?1)",
                params![removed.name],
                |row| row.get(0),
            )
            .map_err(to_err)?;
        if taken {
            return Ok(false);
        }
        tx.execute(
            "INSERT INTO sublibrary(
                 name, target, target_raw, format, capacity, capability, next_rule, at,
                 capacity_by_device)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                removed.name,
                removed.target,
                removed.target_raw,
                removed.format,
                removed.capacity,
                removed.capability,
                removed.next_rule,
                removed.at,
                removed.capacity_by_device,
            ],
        )
        .map_err(to_err)?;
        {
            let mut insert = tx
                .prepare(
                    "INSERT INTO sublibrary_rule(sublibrary, ordinal, text, at)
                     VALUES(?1, ?2, ?3, ?4)",
                )
                .map_err(to_err)?;
            for rule in &removed.rules {
                insert
                    .execute(params![removed.name, rule.ordinal, rule.text, rule.at])
                    .map_err(to_err)?;
            }
            let mut insert = tx
                .prepare(
                    "INSERT INTO sublibrary_exception(sublibrary, variant_key, kind, note, at)
                     VALUES(?1, ?2, ?3, ?4, ?5)",
                )
                .map_err(to_err)?;
            for exception in &removed.exceptions {
                insert
                    .execute(params![
                        removed.name,
                        exception.variant_key,
                        exception.kind,
                        exception.note,
                        exception.at,
                    ])
                    .map_err(to_err)?;
            }
            let mut insert = tx
                .prepare(
                    "INSERT INTO sublibrary_manifest(
                         sublibrary, path, kind, bytes, mtime_ns, source, variant,
                         source_bytes, source_mtime_ns, absent, at)
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                )
                .map_err(to_err)?;
            for file in &removed.manifest {
                insert
                    .execute(params![
                        removed.name,
                        file.path,
                        file.kind,
                        file.bytes,
                        file.mtime_ns,
                        file.source,
                        file.variant,
                        file.source_bytes,
                        file.source_mtime_ns,
                        file.absent,
                        file.at,
                    ])
                    .map_err(to_err)?;
            }
            let mut insert = tx
                .prepare(
                    "INSERT INTO sublibrary_override(sublibrary, platform, choice, at)
                     VALUES(?1, ?2, ?3, ?4)",
                )
                .map_err(to_err)?;
            for row in &removed.overrides {
                insert
                    .execute(params![removed.name, row.platform, row.choice, row.at])
                    .map_err(to_err)?;
            }
        }
        tx.commit().map_err(to_err)?;
        Ok(true)
    }

    /// 往选择集里加一条规则，返回它的序号。
    ///
    /// **收的是已经读通了的 [`Rule`]，不是一串字符串。** 一条存进去才发现读不懂的
    /// 规则，要等到下次同步才炸，那时用户早忘了自己写了什么——把「读得懂」做成参数类型，
    /// 这条路就走不通了，而不是靠每个调用方记得先校验一次。
    ///
    /// # Errors
    /// 写库失败，或者这个子库不存在时返回错误。
    pub fn add_rule(&mut self, name: &str, rule: &Rule) -> Result<i64, CatalogError> {
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        // 发号与占号在同一个事务里：分开做的话，两处同时加规则会拿到同一个号。
        let ordinal: i64 = tx
            .query_row(
                "SELECT next_rule FROM sublibrary WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .map_err(to_err)?;
        tx.execute(
            "INSERT INTO sublibrary_rule(sublibrary, ordinal, text, at)
             VALUES(?1, ?2, ?3, ?4)",
            params![name, ordinal, rule.text, super::now_secs()],
        )
        .map_err(to_err)?;
        tx.execute(
            "UPDATE sublibrary SET next_rule = ?2 WHERE name = ?1",
            params![name, ordinal + 1],
        )
        .map_err(to_err)?;
        tx.commit().map_err(to_err)?;
        Ok(ordinal)
    }

    /// 把这个子库**读得懂的那几条规则整个换成这一条**，返回新那条的序号。
    ///
    /// 「**改选择**」那条回程走的就是它（票 `gui-redesign/11`）：子库屏点「改选择」时
    /// 把这个子库的规则并成一条（[`Rule::any_of`](crate::sublibrary::rule::Rule::any_of)）
    /// 预填进浏览屏的筛选器，调完按「更新到子库」，屏上那份筛选折回一条规则原样带回来。
    /// 带回来的是**一条**：筛选器是一棵树，它折出来的本来就是一条
    /// （`WorkQuery::to_rule`）。往上加而不是换掉的话，旧的那几条还在，
    /// 子库选出来的就比屏上多——而那正是「筛选就是子库的规则」这条约定要消灭的东西。
    ///
    /// **读不懂的那几条原样留着，一条都不碰。** 它们本来就没参与求值
    /// （[`LoadedSelection::from_stored`] 把它们挑出来另放），所以留着不改变这个子库
    /// 选出什么；而顺手删掉它们等于拿一次「改选择」悄悄清掉用户还没来得及修的东西。
    ///
    /// 整趟在一个事务里：删一半就断电的话，那个子库会变成「一条规则都没有」，
    /// 同步过去是空的。
    ///
    /// # Errors
    /// 写库失败，或者这个子库不存在时返回错误。
    pub fn replace_rules(&mut self, name: &str, rule: &Rule) -> Result<i64, CatalogError> {
        let stale: Vec<i64> = self
            .sublibrary_rules(name)?
            .into_iter()
            .filter(|stored| Rule::parse(&stored.text).is_ok())
            .map(|stored| stored.ordinal)
            .collect();
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        for ordinal in stale {
            tx.execute(
                "DELETE FROM sublibrary_rule WHERE sublibrary = ?1 AND ordinal = ?2",
                params![name, ordinal],
            )
            .map_err(to_err)?;
        }
        let ordinal: i64 = tx
            .query_row(
                "SELECT next_rule FROM sublibrary WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .map_err(to_err)?;
        tx.execute(
            "INSERT INTO sublibrary_rule(sublibrary, ordinal, text, at)
             VALUES(?1, ?2, ?3, ?4)",
            params![name, ordinal, rule.text, super::now_secs()],
        )
        .map_err(to_err)?;
        tx.execute(
            "UPDATE sublibrary SET next_rule = ?2 WHERE name = ?1",
            params![name, ordinal + 1],
        )
        .map_err(to_err)?;
        tx.commit().map_err(to_err)?;
        Ok(ordinal)
    }

    /// 删掉一条规则。返回它本来在不在。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn remove_rule(&mut self, name: &str, ordinal: i64) -> Result<bool, CatalogError> {
        let gone = self
            .conn
            .execute(
                "DELETE FROM sublibrary_rule WHERE sublibrary = ?1 AND ordinal = ?2",
                params![name, ordinal],
            )
            .map_err(|source| self.err(source))?;
        Ok(gone > 0)
    }

    /// 把这个子库的**第 `ordinal` 条规则换成这一条**，序号不变。返回那一条本来在不在；不在就一行都不写。
    ///
    /// 子库屏规则行上「✎」那条回程走的就是它（票 `gui-looks-like-the-design/20`，拿主意的人 2026-09-14 定）：跳去浏览屏时
    /// 只把这一条预填进筛选器，调完按「更新到子库」只换回这一条——别的规则、读不懂的那几条、例外一样不碰。与
    /// [`Self::replace_rules`] 的差别就在这儿：那一条把读得懂的整批换成一条。
    ///
    /// **序号照旧**：人照着报告认的是「第 2 条」，改了条件它还是第 2 条；发号器（`next_rule`）不动。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn replace_rule(
        &mut self,
        name: &str,
        ordinal: i64,
        rule: &Rule,
    ) -> Result<bool, CatalogError> {
        let changed = self
            .conn
            .execute(
                "UPDATE sublibrary_rule SET text = ?3, at = ?4 WHERE sublibrary = ?1 AND ordinal = ?2",
                params![name, ordinal, rule.text, super::now_secs()],
            )
            .map_err(|source| self.err(source))?;
        Ok(changed > 0)
    }

    /// **扔掉一条读不懂的规则**——读得懂的那几条一条都碰不到。
    ///
    /// 与 [`Self::remove_rule`] 的差别只有一条：它按序号删任何一条，这一条**先读一遍
    /// 再决定删不删**，读得懂就拒绝（[`Discarded::Readable`]）。这一道闸是给界面上
    /// 那条路准备的（票 `gui-redesign/14`）：读不懂的那几条在界面上处置得掉，而读得懂
    /// 的那几条只经由「改选择」整批换掉（[`Self::replace_rules`]）——两条路混在一起
    /// 的话，一次「扔掉这条坏的」就能悄悄删掉一条好的，而屏上写的是「扔掉读不懂的」。
    ///
    /// 判「读不懂」用的是与 [`LoadedSelection::from_stored`] **同一条** `Rule::parse`：
    /// 界面上摆出来的那几条红的，正是这条路删得掉的那几条，两处分歧不了。
    ///
    /// 例外一条都不碰：它们与规则各存一张表，而**例外是永久记住的**（ADR-0016）。
    ///
    /// # Errors
    /// 读库或写库失败时返回错误。
    pub fn discard_broken_rule(
        &mut self,
        name: &str,
        ordinal: i64,
    ) -> Result<Discarded, CatalogError> {
        let Some(stored) = self
            .sublibrary_rules(name)?
            .into_iter()
            .find(|stored| stored.ordinal == ordinal)
        else {
            return Ok(Discarded::Absent);
        };
        if Rule::parse(&stored.text).is_ok() {
            return Ok(Discarded::Readable);
        }
        let gone = self
            .conn
            .execute(
                "DELETE FROM sublibrary_rule WHERE sublibrary = ?1 AND ordinal = ?2",
                params![name, ordinal],
            )
            .map_err(|source| self.err(source))?;
        // 读到与删掉之间那个缝：另一个进程刚好把它删了，那也算「已经不在了」。
        Ok(if gone > 0 {
            Discarded::Gone
        } else {
            Discarded::Absent
        })
    }

    /// 读一个子库的规则，按序号排。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn sublibrary_rules(&self, name: &str) -> Result<Vec<StoredRule>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT ordinal, text FROM sublibrary_rule
                 WHERE sublibrary = ?1 ORDER BY ordinal",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![name], |row| {
                Ok(StoredRule {
                    ordinal: row.get(0)?,
                    text: row.get(1)?,
                })
            })
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 记一条例外。同一个变体上再记一条会覆盖方向——**收入与排除是同一个决定的两面**，
    /// 一个变体在一个子库里不可能既收入又排除。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn set_exception(
        &mut self,
        name: &str,
        variant_key: &str,
        kind: Exception,
        note: Option<&str>,
    ) -> Result<(), CatalogError> {
        self.set_exceptions(name, std::slice::from_ref(&variant_key), kind, note)?;
        Ok(())
    }

    /// 一批变体**整批**记成同一个方向的例外，一个事务里写完。交回写了几条。
    ///
    /// 屏上「搜索作品直接添加」那一下走的就是它（票 `gui-looks-like-the-design/22`）：一个**作品**底下
    /// 常常挂着好几个变体，而例外落在**变体**这一层——一条一条写的话，中途写不动就会留下半个作品。
    ///
    /// **同一个变体上再记一条会覆盖方向**（与 [`Self::set_exception`] 同一条 upsert，它就是拿一个键调的这一支）：
    /// 收入与排除是同一个决定的两面，一个变体在一个子库里不可能既收入又排除，因此**从一栏换到另一栏不会留下两条**。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn set_exceptions(
        &mut self,
        name: &str,
        variant_keys: &[&str],
        kind: Exception,
        note: Option<&str>,
    ) -> Result<usize, CatalogError> {
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let at = super::now_secs();
        let tx = self.conn.transaction().map_err(to_err)?;
        {
            let mut insert = tx
                .prepare(
                    "INSERT INTO sublibrary_exception(sublibrary, variant_key, kind, note, at)
                     VALUES(?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(sublibrary, variant_key) DO UPDATE SET
                        kind = excluded.kind, note = excluded.note, at = excluded.at",
                )
                .map_err(to_err)?;
            for variant_key in variant_keys {
                insert
                    .execute(params![
                        name,
                        path::nfc(variant_key).into_owned(),
                        kind.label(),
                        note,
                        at
                    ])
                    .map_err(to_err)?;
            }
        }
        tx.commit().map_err(to_err)?;
        Ok(variant_keys.len())
    }

    /// 忘掉一条例外——从此这个变体听规则的。返回它本来在不在。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn clear_exception(&mut self, name: &str, variant_key: &str) -> Result<bool, CatalogError> {
        let gone = self
            .conn
            .execute(
                "DELETE FROM sublibrary_exception WHERE sublibrary = ?1 AND variant_key = ?2",
                params![name, path::nfc(variant_key).into_owned()],
            )
            .map_err(|source| self.err(source))?;
        Ok(gone > 0)
    }

    /// 读一个子库的例外，按变体的键排。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn sublibrary_exceptions(&self, name: &str) -> Result<Vec<ExceptionRow>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT variant_key, kind, note, at FROM sublibrary_exception
                 WHERE sublibrary = ?1 ORDER BY variant_key",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![name], |row| {
                let kind: String = row.get(1)?;
                Ok(ExceptionRow {
                    variant_key: row.get(0)?,
                    // 认不出方向的行按**排除**处置：库里被人手改坏了的时候，
                    // 少带一个游戏比多带一个安全——多带的那个可能正是用户特意踢掉的。
                    kind: Exception::from_label(&kind).unwrap_or(Exception::Exclude),
                    note: row.get(2)?,
                    at: row.get(3)?,
                })
            })
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 读一个子库的例外，**连库里那个变体眼下是谁**（[`ExceptionDetail`]）：作品（显示标题）、平台、容量。
    /// 手动例外那张表（票 `gui-looks-like-the-design/22`）逐条写的就是它。
    ///
    /// **按记下的时刻倒着排**，同一刻的按键排：刚记的那一条摆最上面——人刚按完「包含」，
    /// 第一眼要看见的就是它进没进去。（[`Self::sublibrary_exceptions`] 那一份照旧按键排：
    /// 求值不看次序，而报告里要的是一份稳定的名单。）
    ///
    /// **库里眼下没有那个变体的那几条照旧交出来**，平台与容量是 `None`——例外是永久记住的
    /// （ADR-0016），盘没插不该让它从屏上消失。
    ///
    /// 显示标题由 [`title::choose`](crate::title::choose) 挑，`priorities` 要与浏览屏主列表、详情面板、
    /// 导出交的是同一份（工作目录里那份优先级表），不然同一个作品在两处是两个名字。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn sublibrary_exception_details(
        &self,
        name: &str,
        priorities: &crate::scrape::Priorities,
    ) -> Result<Vec<ExceptionDetail>, CatalogError> {
        // **方向那一列只在一处认**（[`Self::sublibrary_exceptions`]，连同库被人手改坏时退成「排除」
        // 那条判断）：这一趟只在旁边补上库里的事实。
        let mut rows = self.sublibrary_exceptions(name)?;
        rows.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.variant_key.cmp(&b.variant_key)));
        // 头一趟：库里眼下有没有这一份、它是谁。**逐条按主键取**，不拼一条长 `IN`——例外一台
        // 几十条，每一条都是一次索引定位，而把几十个键塞进一条 SQL 要跟着分批（`KEYS_PER_QUERY`）。
        // 这一步 `work` 那一格装的还是**作品名**，显示标题在第二趟里换。
        let mut details: Vec<ExceptionDetail> = Vec::with_capacity(rows.len());
        {
            let mut statement = self
                .conn
                .prepare(
                    "SELECT variant.platform, variant.bytes, work.name
                       FROM variant LEFT JOIN work ON work.id = variant.work_id
                      WHERE variant.key = ?1",
                )
                .map_err(|source| self.err(source))?;
            for row in rows {
                let seen = statement
                    .query_row(params![row.variant_key], |found| {
                        Ok((
                            found.get::<_, Option<String>>(0)?,
                            found.get::<_, i64>(1)?,
                            found.get::<_, Option<String>>(2)?,
                        ))
                    })
                    .optional()
                    .map_err(|source| self.err(source))?;
                details.push(match seen {
                    Some((platform, bytes, work)) => ExceptionDetail {
                        row,
                        work,
                        platform,
                        // 容量是下界（ADR-0021）；库里存的是 `INTEGER`，负数只可能是被人手改坏了。
                        bytes: Some(u64::try_from(bytes).unwrap_or(0)),
                    },
                    // **库里眼下没有这一份**：平台与容量都不写，`missing()` 照它答。
                    None => ExceptionDetail {
                        row,
                        work: None,
                        platform: None,
                        bytes: None,
                    },
                });
            }
        }
        // 第二趟：这几条指到的作品那几份叫法一起读回来，逐个交给 `title::choose`
        // （与 `work_page_with_titles` 同一处挑）。
        let works: Vec<&str> = details
            .iter()
            .filter_map(|detail| detail.work.as_deref())
            .collect();
        let titles = self.titles_of_works(&works)?;
        for detail in &mut details {
            // **同一个作品可以挂着好几条例外**：这份叫法表按作品**查**、不取走，不然第二条就没名字了。
            let Some(work) = detail.work.as_deref() else {
                continue;
            };
            let Some(entries) = titles.get(work) else {
                continue;
            };
            detail.work = Some(
                crate::title::choose(
                    &crate::title::TitleSet {
                        work: work.to_string(),
                        entries: entries.clone(),
                    },
                    priorities,
                )
                .display,
            );
        }
        Ok(details)
    }

    /// 读一个子库的**按平台覆盖**（票 `gui-looks-like-the-design/21`）：平台名 → 覆盖成什么。一个都没有是空表。
    ///
    /// 库里认不出的那个词（被人手改坏了）**跳过**：跳过就是照名册判，而名册里的结论是有来源的——
    /// 猜一个覆盖出来替用户做决定，正是 ADR-0017 那句「矩阵错误比不转换更糟」说的那种错。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn capability_overrides(
        &self,
        name: &str,
    ) -> Result<BTreeMap<String, Override>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT platform, choice FROM sublibrary_override
                 WHERE sublibrary = ?1 ORDER BY platform",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![name], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|source| self.err(source))?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (platform, choice) = row.map_err(|source| self.err(source))?;
            if let Some(choice) = Override::from_code(&choice) {
                out.insert(platform, choice);
            }
        }
        Ok(out)
    }

    /// 库里**头一个有平台的变体**（按键排）住在哪个平台目录下：目标设置里前端格式那句说明拿它举例
    /// （「每个平台一份，例如 GBA.metadata.pegasus.txt」，票 `gui-looks-like-the-design/21`），界面上不写死平台名。
    /// 目录照键的第二段（`path::platform_of_key`，与收敛、落点同一个口径）；库里一个有平台的变体都没有时是 `None`。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn sample_platform_directory(&self) -> Result<Option<String>, CatalogError> {
        let key: Option<String> = self
            .conn
            .query_row(
                "SELECT key FROM variant WHERE platform IS NOT NULL ORDER BY key LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|source| self.err(source))?;
        Ok(key.and_then(|key| path::platform_of_key(&key).map(ToString::to_string)))
    }

    /// 换掉一个子库的**按平台覆盖**：**整份替换**，不是往上叠——目标设置里改回「按档案」的那几行就是没了。
    ///
    /// # Errors
    /// 写库失败，或者这个子库不存在时返回错误。
    pub fn set_capability_overrides(
        &mut self,
        name: &str,
        overrides: &BTreeMap<String, Override>,
    ) -> Result<(), CatalogError> {
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        tx.execute(
            "DELETE FROM sublibrary_override WHERE sublibrary = ?1",
            params![name],
        )
        .map_err(to_err)?;
        {
            let mut insert = tx
                .prepare(
                    "INSERT INTO sublibrary_override(sublibrary, platform, choice, at)
                     VALUES(?1, ?2, ?3, ?4)",
                )
                .map_err(to_err)?;
            let at = super::now_secs();
            for (platform, choice) in overrides {
                insert
                    .execute(params![name, platform, choice.code(), at])
                    .map_err(to_err)?;
            }
        }
        tx.commit().map_err(to_err)
    }

    /// 把一个子库的规则与例外读成一份**选择集**。
    ///
    /// 读不懂的规则**跳过并报出来**，不整趟失败（见
    /// [`LoadedSelection::from_stored`]，读规则那一半的逻辑在那里，是纯的）。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn selection(&self, name: &str) -> Result<LoadedSelection, CatalogError> {
        let mut out = LoadedSelection::from_stored(&self.sublibrary_rules(name)?);
        out.selection.exceptions = self.sublibrary_exceptions(name)?;
        Ok(out)
    }

    /// 读一个子库的**清单**：上次导出往目标上放了哪些文件。
    ///
    /// 没同步过的子库读出来是一份**空清单**，于是计划里一条删除也长不出来——
    /// 「工具只删自己放过的东西」在这条路径上是自动成立的。
    ///
    /// 认不出类别的行**整条跳过**：那一条说不清是 ROM 还是别人的截图，而说不清的
    /// 一律当作不归工具管（清单之外）。宁可少删，不可错删。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn manifest(&self, name: &str) -> Result<Manifest, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT path, kind, bytes, mtime_ns, source, variant,
                        source_bytes, source_mtime_ns, absent
                 FROM sublibrary_manifest WHERE sublibrary = ?1 ORDER BY path",
            )
            .map_err(|source| self.err(source))?;
        let mut rows = statement
            .query(params![name])
            .map_err(|source| self.err(source))?;
        let mut out = Manifest::default();
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let kind: String = row.get(1).map_err(|source| self.err(source))?;
            let Some(kind) = FileKind::from_label(&kind) else {
                continue;
            };
            let bytes: i64 = row.get(2).map_err(|source| self.err(source))?;
            let source_bytes: i64 = row.get(6).map_err(|source| self.err(source))?;
            out.files.push(ManifestFile {
                path: row.get(0).map_err(|source| self.err(source))?,
                kind,
                stamp: Stamp {
                    bytes: u64::try_from(bytes).unwrap_or(0),
                    mtime_ns: row.get(3).map_err(|source| self.err(source))?,
                },
                source: row.get(4).map_err(|source| self.err(source))?,
                source_stamp: Stamp {
                    bytes: u64::try_from(source_bytes).unwrap_or(0),
                    mtime_ns: row.get(7).map_err(|source| self.err(source))?,
                },
                variant: row.get(5).map_err(|source| self.err(source))?,
                absent: row.get::<_, i64>(8).map_err(|source| self.err(source))? != 0,
            });
        }
        Ok(out)
    }

    /// 把一个子库的清单整份换掉——同步完成后记下**目标的真实状态**（票 20）。
    ///
    /// **整份换而不是逐条改**，而且在一个事务里：清单是「上次导出的完整记录」，
    /// 半份新半份旧的清单比没有清单更危险——它会让工具以为自己放过某个文件。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put_manifest(&mut self, name: &str, manifest: &Manifest) -> Result<(), CatalogError> {
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let at = super::now_secs();
        let tx = self.conn.transaction().map_err(to_err)?;
        tx.execute(
            "DELETE FROM sublibrary_manifest WHERE sublibrary = ?1",
            params![name],
        )
        .map_err(to_err)?;
        {
            let mut insert = tx
                .prepare(
                    "INSERT INTO sublibrary_manifest(
                         sublibrary, path, kind, bytes, mtime_ns, source, variant,
                         source_bytes, source_mtime_ns, absent, at)
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                )
                .map_err(to_err)?;
            for file in &manifest.files {
                insert
                    .execute(params![
                        name,
                        crate::path::nfc(&file.path).into_owned(),
                        file.kind.label(),
                        i64::try_from(file.stamp.bytes).unwrap_or(i64::MAX),
                        file.stamp.mtime_ns,
                        file.source,
                        file.variant,
                        i64::try_from(file.source_stamp.bytes).unwrap_or(i64::MAX),
                        file.source_stamp.mtime_ns,
                        i64::from(file.absent),
                        at,
                    ])
                    .map_err(to_err)?;
            }
        }
        tx.commit().map_err(to_err)?;
        Ok(())
    }
}
