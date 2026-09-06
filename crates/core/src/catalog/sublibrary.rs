//! 中立库里的**子库**与**选择集**：一台目标设备一行，带它的规则与例外。
//!
//! ## 四张表各自回答一个问题
//!
//! - `sublibrary`：**这台设备是什么样的**——目标路径、前端格式、容量上限。
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
//! 票 20 给这两张表各加了一列（`sublibrary.target_raw`、`sublibrary_manifest.absent`），
//! 判据仍是同一条：两列在老行上取得到的值与加它们之前的唯一可能完全一致，
//! 于是旧数据读不错。补列的活在 [`add_columns`] 里，那个函数的注释写着为什么。

use rusqlite::{OptionalExtension, params};

use super::{Catalog, CatalogError};
use crate::path;
use crate::sublibrary::{
    Discarded, Exception, ExceptionRow, LoadedSelection, Rule, StoredRule, Sublibrary,
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
                     name, target, target_raw, format, capacity, capability, next_rule, at)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, 1, ?7)
                 ON CONFLICT(name) DO UPDATE SET
                    target = excluded.target, target_raw = excluded.target_raw,
                    format = excluded.format,
                    capacity = excluded.capacity, capability = excluded.capability,
                    at = excluded.at",
                params![
                    sublibrary.name,
                    target,
                    sublibrary.target_raw,
                    sublibrary.format,
                    capacity,
                    sublibrary.capability,
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
                "SELECT name, target, target_raw, format, capacity, capability
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
                "SELECT name, target, target_raw, format, capacity, capability
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
                })
            })
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 删掉一个子库，连它的规则与例外一起。返回它本来在不在。
    ///
    /// 例外跟着一起没，与「例外永久记住」不矛盾：**记的是这个子库的口味**，
    /// 子库都不要了，那些决定也就没有归属。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn remove_sublibrary(&mut self, name: &str) -> Result<bool, CatalogError> {
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        // **先删子表**：外键检查默认是开着的（见 `catalog::content` 里那段注释）。
        tx.execute(
            "DELETE FROM sublibrary_manifest WHERE sublibrary = ?1",
            params![name],
        )
        .map_err(to_err)?;
        tx.execute(
            "DELETE FROM sublibrary_exception WHERE sublibrary = ?1",
            params![name],
        )
        .map_err(to_err)?;
        tx.execute(
            "DELETE FROM sublibrary_rule WHERE sublibrary = ?1",
            params![name],
        )
        .map_err(to_err)?;
        let gone = tx
            .execute("DELETE FROM sublibrary WHERE name = ?1", params![name])
            .map_err(to_err)?;
        tx.commit().map_err(to_err)?;
        Ok(gone > 0)
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
        self.conn
            .execute(
                "INSERT INTO sublibrary_exception(sublibrary, variant_key, kind, note, at)
                 VALUES(?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(sublibrary, variant_key) DO UPDATE SET
                    kind = excluded.kind, note = excluded.note, at = excluded.at",
                params![
                    name,
                    path::nfc(variant_key).into_owned(),
                    kind.label(),
                    note,
                    super::now_secs()
                ],
            )
            .map_err(|source| self.err(source))?;
        Ok(())
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
