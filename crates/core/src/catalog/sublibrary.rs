//! 中立库里的**子库**与**选择集**：一台目标设备一行，带它的规则与例外。
//!
//! ## 三张表各自回答一个问题
//!
//! - `sublibrary`：**这台设备是什么样的**——目标路径、前端格式、容量上限。
//! - `sublibrary_rule`：**要什么**，可重放的那一半。存的是**规则的原文**而不是求值
//!   结果——存结果的话「主库新增的内容下次自动进入」就不成立了，那正是规则存在的理由。
//! - `sublibrary_exception`：**另外还要 / 偏不要什么**，优先于规则的那一半。
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
//! ## 结构版本没有加 1
//!
//! 这三张表是**纯加表**：已有的表一列没动、一条语义没改，`CREATE TABLE IF NOT EXISTS`
//! 在打开时就补上，旧库照样打得开。判据是「旧数据会不会被读错」而不是「文件里多了
//! 点东西」（见 [`SCHEMA_VERSION`](super::SCHEMA_VERSION) 与挂账 D50）。

use rusqlite::{OptionalExtension, params};

use super::{Catalog, CatalogError};
use crate::path;
use crate::sublibrary::{Exception, ExceptionRow, LoadedSelection, Rule, StoredRule, Sublibrary};

/// 子库与选择集的表。
pub(super) const SUBLIBRARY_SCHEMA: &str = "\
-- **子库**：从主库挑一部分导到某台目标设备形成的派生库（ADR-0015）。
-- 主键是名字：命令行拿它指名道姓，而一台设备换了挂载点名字不变——与中立库自己
-- 靠 `--library <名字>` 而不是绝对路径找回来是同一条道理（票 29）。
CREATE TABLE IF NOT EXISTS sublibrary(
    name      TEXT PRIMARY KEY,
    -- 目标设备上的子库根。一律走读卡器，因此这是本机上的一个绝对路径（ADR-0015）。
    -- 存 NFC 形式（ADR-0020）。
    target    TEXT NOT NULL,
    -- 前端格式（适配器名）。
    format    TEXT NOT NULL,
    -- 容量上限，字节；NULL 表示不设限。**超限不自动截断**（ADR-0016）。
    capacity  INTEGER,
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
";

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
                "INSERT INTO sublibrary(name, target, format, capacity, next_rule, at)
                 VALUES(?1, ?2, ?3, ?4, 1, ?5)
                 ON CONFLICT(name) DO UPDATE SET
                    target = excluded.target, format = excluded.format,
                    capacity = excluded.capacity, at = excluded.at",
                params![
                    sublibrary.name,
                    target,
                    sublibrary.format,
                    capacity,
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
                "SELECT name, target, format, capacity FROM sublibrary WHERE name = ?1",
                params![name],
                |row| {
                    Ok(Sublibrary {
                        name: row.get(0)?,
                        target: row.get(1)?,
                        format: row.get(2)?,
                        capacity: row
                            .get::<_, Option<i64>>(3)?
                            .and_then(|v| u64::try_from(v).ok()),
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
            .prepare("SELECT name, target, format, capacity FROM sublibrary ORDER BY name")
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok(Sublibrary {
                    name: row.get(0)?,
                    target: row.get(1)?,
                    format: row.get(2)?,
                    capacity: row
                        .get::<_, Option<i64>>(3)?
                        .and_then(|v| u64::try_from(v).ok()),
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
}
