//! 翻页浏览：**排序、筛选、分页一律下推到中立库**。
//!
//! 界面上那张变体表在真机上是四万多行，做完识别后还要连上候选与标题——十万行是设计
//! 时就该按住的量级（ADR-0005 的修订段）。这一层存在的理由只有一条：
//! **内存里永远只有当前视口那几十行**。
//!
//! ## 为什么排序不能在内存里做
//!
//! `egui_extras` 完全没有排序能力（源码里 `sort` 零出现），于是排序必须由别人做。
//! 「别人」有两个候选：把全部变体读成一个 `Vec` 再 `sort_by`，或者写成 `ORDER BY`。
//! 选后者，因为前者与「绝不把全库载入内存」这条既定原则直接冲突——四万行的 `VariantRow`
//! 每行两个 `String`，读一次就是几十兆常驻，而这还只是今天的库。
//!
//! 分页同理：`LIMIT` 加 `OFFSET` 让「翻到第九万行」的代价与视口大小成正比，而不是与
//! 库的大小成正比。
//!
//! ## 排序键永远带上变体键
//!
//! 按容量排时有大量并列（同一个平台下一堆 0 字节的目录树），并列行的相对次序若不定死，
//! 翻页就会漏行与重行——第 100 页的最后一行可能在第 101 页再出现一次。所以每条
//! `ORDER BY` 后面都缀一个 `key`，而 `key` 是主键，全序由它兜住。
//!
//! **升降序对两段一起翻**（`bytes DESC, key DESC`），不是只翻第一段。这样
//! `(bytes, key)` 那条索引倒着扫就能直接出结果，不必落到临时表排序。

use rusqlite::{ToSql, params_from_iter};

use super::content::VariantRow;
use super::{Catalog, CatalogError};

/// 变体表按哪一列排。
///
/// 是个闭集合而不是一个字符串：拼进 `ORDER BY` 的列名只能来自这里，界面上点哪个表头
/// 都拼不出别的 SQL。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VariantOrder {
    /// 变体的键，也就是主库里的相对路径。默认按它排——那是人在文件管理器里看惯的次序。
    #[default]
    Key,
    /// 平台。
    Platform,
    /// 哪条**成型规则**成的型。
    Rule,
    /// 文件成员数。
    Files,
    /// 容量（字节合计，是个下界）。
    Bytes,
}

impl VariantOrder {
    /// 全部可排的列，界面照这个次序摆表头。
    pub const ALL: [Self; 5] = [
        Self::Key,
        Self::Platform,
        Self::Rule,
        Self::Files,
        Self::Bytes,
    ];

    /// 这一列在界面上叫什么。用**词表**里的词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Key => "变体",
            Self::Platform => "平台",
            Self::Rule => "成型规则",
            Self::Files => "文件数",
            Self::Bytes => "容量",
        }
    }

    /// 拼进 `ORDER BY` 的列名。**只有这个函数认得列名**。
    fn column(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Platform => "platform",
            Self::Rule => "rule",
            Self::Files => "files",
            Self::Bytes => "bytes",
        }
    }
}

/// 一次翻页要的是哪一段：筛什么、按什么排。
///
/// 它是**值**而不是游标：界面把它整个换掉就等于换了一张表，窗口据此作废重取。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VariantQuery {
    /// 变体的键里含这个子串才算数；空串等于不筛。
    ///
    /// 大小写按 SQLite 的 `LIKE` 语义——**只对 ASCII 不敏感**，汉字与假名是逐字节比的。
    /// 中文本来就没有大小写，这个限制在这里不咬人。
    pub contains: String,
    /// 按哪一列排。
    pub order: VariantOrder,
    /// 倒着排。
    pub descending: bool,
}

/// 一页最多取多少行。
///
/// 视口撑死几十行，取这么多已经给预取留了十几倍余量。**存在的意义是拦住
/// `limit = u64::MAX` 这种把全库拉进内存的写法**——那正是这一层要防的事。
pub const MAX_PAGE: u64 = 4_096;

/// 把 `LIKE` 的三个元字符转义掉。
///
/// 不转义的话，用户在筛选框里打一个 `%` 就等于「什么都匹配」，打 `_` 会悄悄多匹配一个
/// 字符——而主库里真有带 `%` 的文件名。
fn escape_like(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    for ch in text.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

impl VariantQuery {
    /// 折出 `WHERE` 那一段与它的参数。两处（数总数、取一页）共用，免得筛选条件漂开——
    /// 总数与页内容用了不同的筛选，滚动条就会指向不存在的行。
    fn where_clause(&self) -> (&'static str, Vec<Box<dyn ToSql>>) {
        if self.contains.is_empty() {
            return ("", Vec::new());
        }
        (
            " WHERE key LIKE ? ESCAPE '\\'",
            vec![Box::new(format!("%{}%", escape_like(&self.contains)))],
        )
    }

    /// 折出 `ORDER BY` 那一段。列名来自 [`VariantOrder::column`]，不含任何用户输入。
    fn order_clause(&self) -> String {
        let direction = if self.descending { "DESC" } else { "ASC" };
        let column = self.order.column();
        if column == "key" {
            format!(" ORDER BY key {direction}")
        } else {
            // 并列行的次序由主键兜住，否则翻页会漏行与重行。
            format!(" ORDER BY {column} {direction}, key {direction}")
        }
    }
}

const COLUMNS: &str = "key, platform, rule, main_key, files, bytes, unreadable, manual,
                       work_id, release_id";

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<VariantRow> {
    Ok(VariantRow {
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
}

impl Catalog {
    /// 满足这个筛选条件的变体一共几行。**滚动条的长度**由它来。
    ///
    /// 单独一次查询而不是「取一页顺便数一下」：滚动条要的是总数，而总数与视口无关，
    /// 换了筛选才变。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn variant_total(&self, query: &VariantQuery) -> Result<u64, CatalogError> {
        let (where_sql, args) = query.where_clause();
        let sql = format!("SELECT COUNT(*) FROM variant{where_sql}");
        let count: i64 = self
            .conn
            .query_row(&sql, params_from_iter(args.iter()), |row| row.get(0))
            .map_err(|source| self.err(source))?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    /// 取一页变体：从第 `offset` 行起、最多 `limit` 行，已排好序。
    ///
    /// `limit` 会被夹到 [`MAX_PAGE`]——这个入口不接受「把全库读出来」这种要求。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn variant_page(
        &self,
        query: &VariantQuery,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<VariantRow>, CatalogError> {
        let limit = limit.min(MAX_PAGE);
        if limit == 0 {
            return Ok(Vec::new());
        }
        let (where_sql, mut args) = query.where_clause();
        let order_sql = query.order_clause();
        let sql = format!("SELECT {COLUMNS} FROM variant{where_sql}{order_sql} LIMIT ? OFFSET ?");
        args.push(Box::new(i64::try_from(limit).unwrap_or(i64::MAX)));
        args.push(Box::new(i64::try_from(offset).unwrap_or(i64::MAX)));
        let mut statement = self.conn.prepare(&sql).map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params_from_iter(args.iter()), read_row)
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }
}
