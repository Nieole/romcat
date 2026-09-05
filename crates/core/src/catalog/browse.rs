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
//!
//! ## 四个筛选维度也一律下推
//!
//! 票 25 的库浏览要按**平台**、**合集**、**语言**、**识别状态**筛。四条都写成 `WHERE`
//! 而不是取回来再过一遍，理由与排序同一条：过一遍要求先把全库读进内存。合集与语言
//! 各走一条 `EXISTS` 子查询——它们是一对多，`JOIN` 会让同一个变体出现好几行，
//! 而那会让总行数与滚动条一起说谎。
//!
//! **筛选面板上那几个选项从哪来**：[`Catalog::facets`] 一次问出四个维度各有哪些值、
//! 各多少个变体。它是 `GROUP BY`，不是把全库读回来数——真库四万多个变体上，
//! 平台十几个、合集几十个，一次几毫秒。
//!
//! ## 那棵**条件组**也一律下推
//!
//! 票 `gui-redesign/04`：筛选器就是**规则**语言，可嵌套的组加三种连接。它同样折成
//! `WHERE`（`catalog::filter`）而不是取回来再过一遍，理由与上面那四条同一条。
//!
//! **它与那四个档之间是且**：那四个是一按就有的快捷档（带条数，供探索），
//! 这一条是手搭的表达式（供表达）。两边收窄的是同一批变体，
//! 而「存成子库」时两边一起折进同一条规则（[`WorkQuery::to_rule`]）。
//!
//! ## 作品级的主列表另是一个查询面
//!
//! [`WorkQuery`] 那一族按**作品**出行（票 `gui-redesign/03`）：一个游戏一行，
//! 变体在详情面板里挑。它与变体表**共用同一份筛选**（[`VariantQuery::where_clause`]），
//! 因为规格里那条贯穿全局的约定是「主列表的筛选就是子库的规则」，而子库选的是变体。
//!
//! **它比变体表贵，而且贵得有理由**：变体表的每一条 `ORDER BY` 都有一条索引正好接住
//! （`variant_bytes_key` 那几条），一次翻页是索引倒着扫；作品级那一条要先 `GROUP BY`
//! 把全表折成行，再按聚合出来的列排序——两步都没有索引接得住，代价与**库有多大**
//! 成正比，不与视口成正比。这不是可以绕开的实现细节：「这个作品有几个变体」这件事
//! 本身就要看过它的每一个变体。内存那一半照旧只有视口那几十行（[`MAX_PAGE`] 还在），
//! 涨的是每次翻页的时间。数字见 `docs/library-facts.md` 与挂单 Q64。

use rusqlite::{ToSql, params_from_iter};

use super::content::{VARIANT_COLUMNS, VariantRow, read_variant_row};
use super::identify::{Candidate, Confidence, State};
use super::{Catalog, CatalogError};
use crate::scrape::{AnchorKind, Field};
use crate::sublibrary::{Clause, Dimension, Group, Join, Node, Op, Rule};

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

/// **识别状态**那一维怎么筛。
///
/// 它比 [`State`] 多一档：**还没识别**。中立库里一个变体可以压根没有 `identification`
/// 那一行——那不是「未命中」，是识别还没跑到它头上。混成一档的话，界面上「未命中有
/// 多少」这个数会把没跑过的一起算进去，而那正是命中率最容易被说错的地方（ADR-0002）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateFilter {
    /// 有结论，而且是这一档。
    Concluded(State),
    /// **还没识别**：这个变体在 `identification` 里没有行。
    Unidentified,
}

impl StateFilter {
    /// 界面上照这个次序摆。
    pub const ALL: [Self; 5] = [
        Self::Concluded(State::Matched),
        Self::Concluded(State::Unmatched),
        Self::Concluded(State::NoEvidence),
        Self::Concluded(State::Skipped),
        Self::Unidentified,
    ];

    /// 打给用户的那个词。用**词表**里的词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Concluded(state) => state.label(),
            Self::Unidentified => "还没识别",
        }
    }

    /// 从词认回来；认不出是 `None`。
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        if label == "还没识别" {
            return Some(Self::Unidentified);
        }
        State::from_label(label).map(Self::Concluded)
    }
}

/// **平台**那一维怎么筛。
///
/// 它比一个 `Option<String>` 多一档：**平台未知**。`variant.platform` 可空，而
/// 「认不出平台」在真库里是一档真实存在的内容，不是缺陷（ADR-0011）——`= NULL`
/// 永远不成立，所以它只能是独立的一支。
///
/// **不拿一个约定字符串当哨兵。** 借 [`UNKNOWN_PLATFORM_LABEL`] 那串字去表示 `NULL`
/// 的话，一个真的叫这个名字的平台就会把两件事撞在一起；而这个库里「两件事必须分得开」
/// 是一条反复出现的纪律（ADR-0021 的不可读、[`StateFilter`] 的还没识别）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformFilter {
    /// 就这个平台。
    Named(String),
    /// **平台未知**：这一列在库里是 `NULL`。
    Unknown,
}

impl PlatformFilter {
    /// 打给用户的那个词。平台未知那一档借报告里已经在用的那一串，两处印的字一样。
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Named(platform) => platform,
            Self::Unknown => UNKNOWN_PLATFORM_LABEL,
        }
    }

    /// 从筛选面板上那一行的字认回来。
    #[must_use]
    pub fn from_label(label: &str) -> Self {
        if label == UNKNOWN_PLATFORM_LABEL {
            Self::Unknown
        } else {
            Self::Named(label.to_string())
        }
    }
}

/// **平台未知**那一档在界面上印成什么。
pub use crate::report::UNKNOWN_PLATFORM_LABEL;

/// 一次翻页要的是哪一段：筛什么、按什么排。
///
/// 它是**值**而不是游标：界面把它整个换掉就等于换了一张表，窗口据此作废重取。
///
/// 四个可选维度之间是**且**：选了平台又选了合集，两条都得满足。这是浏览而不是搜索
/// ——「一层层收窄」是人在文件管理器里的动作，而并集会让每多选一个条件行数反而变多。
///
/// 要并集就写进[那棵条件组](Self::rule)里，**亲手选出「任一满足」那一档**——
/// 行数变多之前人先看见了那四个字（挂账 D154 的裁决）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VariantQuery {
    /// 变体的键里含这个子串才算数；空串等于不筛。
    ///
    /// 大小写按 SQLite 的 `LIKE` 语义——**只对 ASCII 不敏感**，汉字与假名是逐字节比的。
    /// 中文本来就没有大小写，这个限制在这里不咬人。
    pub contains: String,
    /// 只要这个**平台**的；[`PlatformFilter::Unknown`] 选的是平台未知那一档。
    pub platform: Option<PlatformFilter>,
    /// 只要在这个**合集**里的。
    pub collection: Option<String>,
    /// 只要发行版标着这个语言的。**逐个语言码比**，不是子串——`zh` 不该匹配上 `zh-Hant`
    /// 之外的东西，而 `en` 更不该匹配上 `Danish`。
    pub language: Option<String>,
    /// 只要带这个**中文身份**记号的：`汉化` / `官中`（ADR-0012）。
    ///
    /// **它与[语言](Self::language)不是一回事，缺一不可。** 语言是**发行版**标着的
    /// 语言码（`Zh-Hans`），而**汉化版是变体**——它多半基于一条日版发行版，语言那一列
    /// 上一个中文字都没有。这个库里最要紧的那批内容恰恰全在这一档里，只按语言筛的话
    /// 它们一条都不出现。
    pub chinese: Option<String>,
    /// 只要**识别状态**是这一档的。
    pub state: Option<StateFilter>,
    /// **筛选器那棵条件树**：可嵌套的组，三种连接（票 `gui-redesign/04`）。
    ///
    /// 它与上面那几个维度之间是**且**——上面那几个是一按就有的快捷档，这一条是
    /// 手搭的表达式，两边收窄的是同一批变体。`None` 是「没搭任何条件」。
    ///
    /// **它就是子库的规则**：同一套语言、同一个求值口径（`catalog::filter`）。
    pub rule: Option<Rule>,
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
pub(super) fn escape_like(text: &str) -> String {
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
    ///
    /// 每一条都是**参数化**的：拼进 SQL 的只有这个函数里写死的那些字，用户在筛选框里
    /// 打什么都只会落进 `?`。
    fn where_clause(&self) -> (String, Vec<Box<dyn ToSql>>) {
        let mut parts: Vec<&'static str> = Vec::new();
        let mut args: Vec<Box<dyn ToSql>> = Vec::new();
        if !self.contains.is_empty() {
            parts.push("variant.key LIKE ? ESCAPE '\\'");
            args.push(Box::new(format!("%{}%", escape_like(&self.contains))));
        }
        match &self.platform {
            None => {}
            // **平台未知那一档要选得中**：它在表里是 `NULL`，而 `= NULL` 永远不成立。
            Some(PlatformFilter::Unknown) => parts.push("variant.platform IS NULL"),
            Some(PlatformFilter::Named(platform)) => {
                parts.push("variant.platform = ?");
                args.push(Box::new(platform.clone()));
            }
        }
        if let Some(collection) = &self.collection {
            // `EXISTS` 而不是 `JOIN`：一个变体可以在好几个合集里，`JOIN` 会让它在结果里
            // 出现好几行——总行数与页内容会一起说谎。
            parts.push(
                "EXISTS (SELECT 1 FROM collection_variant cv
                         JOIN collection c ON c.id = cv.collection_id
                         WHERE cv.variant_key = variant.key AND c.name = ?)",
            );
            args.push(Box::new(collection.clone()));
        }
        if let Some(language) = &self.language {
            // 逐个语言码比：`languages` 是逗号分隔的一串，两头各补一个逗号之后
            // `instr` 找的就是完整的一段，`en` 不会撞上 `Danish`。
            parts.push(
                "EXISTS (SELECT 1 FROM release r
                         WHERE r.id = variant.release_id
                           AND instr(',' || replace(COALESCE(r.languages,''), ' ', '') || ',',
                                     ',' || ? || ',') > 0)",
            );
            args.push(Box::new(language.clone()));
        }
        if let Some(mark) = &self.chinese {
            // 记号从**自动通过**的候选上读回来，与 `sublibrary::facts` 和
            // `adapter::converge` 同一条路——三处答案不一样的话，界面上筛出来的那批
            // 与真正导出去的那批就对不上。
            parts.push(
                "EXISTS (SELECT 1 FROM candidate c
                         WHERE c.variant_key = variant.key AND c.accepted <> 0 AND c.chinese = ?)",
            );
            args.push(Box::new(mark.clone()));
        }
        match self.state {
            None => {}
            Some(StateFilter::Unidentified) => parts.push(
                "NOT EXISTS (SELECT 1 FROM identification i WHERE i.variant_key = variant.key)",
            ),
            Some(StateFilter::Concluded(state)) => {
                parts.push(
                    "EXISTS (SELECT 1 FROM identification i
                             WHERE i.variant_key = variant.key AND i.state = ?)",
                );
                args.push(Box::new(state.label().to_string()));
            }
        }
        let mut parts: Vec<String> = parts.into_iter().map(str::to_string).collect();
        if let Some(rule) = &self.rule {
            let (sql, mut more) = crate::catalog::filter::rule_sql(rule);
            parts.push(sql);
            args.append(&mut more);
        }
        if parts.is_empty() {
            return (String::new(), args);
        }
        (format!(" WHERE {}", parts.join(" AND ")), args)
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
        let sql =
            format!("SELECT {VARIANT_COLUMNS} FROM variant{where_sql}{order_sql} LIMIT ? OFFSET ?");
        args.push(Box::new(i64::try_from(limit).unwrap_or(i64::MAX)));
        args.push(Box::new(i64::try_from(offset).unwrap_or(i64::MAX)));
        let mut statement = self.conn.prepare(&sql).map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params_from_iter(args.iter()), read_variant_row)
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }
}

/// 筛选面板上一个可选值，连它选中多少个变体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facet {
    /// 值本身。平台未知那一档写成 [`UNKNOWN_PLATFORM_LABEL`]。
    pub value: String,
    /// 这个值选中多少个变体。
    pub count: u64,
}

/// 四个筛选维度各有哪些值可选。
///
/// **每一档都带着条数**：一个选不出东西的筛选条件，与一个「这个平台只有 3 个变体」的
/// 事实，在界面上得看得出区别——没有条数的下拉框，用户只能一个个点开试。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facets {
    /// 平台，按条数从多到少。
    pub platforms: Vec<Facet>,
    /// **合集**（与平台正交，ADR-0011），按条数从多到少。
    pub collections: Vec<Facet>,
    /// 发行版标着的语言，按条数从多到少。
    pub languages: Vec<Facet>,
    /// **中文身份**：`汉化` / `官中`（ADR-0012）。与语言那一维分开，见
    /// [`VariantQuery::chinese`]。
    pub chinese: Vec<Facet>,
    /// 识别状态，按 [`StateFilter::ALL`] 的次序。
    ///
    /// **不是 [`Facet`]，也不按条数排**：另外三维的值是库里现有的字符串（库里没有 PSV
    /// 就不该有 PSV 那一档），而这一维是个**闭集合**——四档结论加「还没识别」，
    /// 一档为零也照样摆出来。零本身是句话：「这份库里一条无判据都没有」与
    /// 「这份库没跑过识别」看起来会是同一片空白，而它们是两回事（ADR-0002）。
    pub states: Vec<(StateFilter, u64)>,
}

/// 按条数从多到少、同数按值排。同一份库问两次必须是同一个次序。
fn rank_facets(mut facets: Vec<Facet>) -> Vec<Facet> {
    facets.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.value.cmp(&b.value)));
    facets
}

impl Catalog {
    /// 筛选面板上那四个维度各有哪些值可选。
    ///
    /// 四条 `GROUP BY`，**不把变体读进内存**。语言那一条例外地在 Rust 里拆了一次：
    /// `release.languages` 是逗号分隔的一串，而 SQLite 没有拆串的内置函数——
    /// 拆的是**去重之后的组合**（真库里几十种），不是四万多个变体。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn facets(&self) -> Result<Facets, CatalogError> {
        let mut out = Facets::default();

        let mut statement = self
            .conn
            .prepare("SELECT COALESCE(platform, ?1), COUNT(*) FROM variant GROUP BY platform")
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([UNKNOWN_PLATFORM_LABEL], |row| {
                Ok(Facet {
                    value: row.get(0)?,
                    count: u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                })
            })
            .map_err(|source| self.err(source))?;
        out.platforms = rank_facets(
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|source| self.err(source))?,
        );

        let mut statement = self
            .conn
            .prepare(
                "SELECT c.name, COUNT(*) FROM collection_variant cv
                 JOIN collection c ON c.id = cv.collection_id
                 GROUP BY c.name",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok(Facet {
                    value: row.get(0)?,
                    count: u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                })
            })
            .map_err(|source| self.err(source))?;
        out.collections = rank_facets(
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|source| self.err(source))?,
        );

        let mut statement = self
            .conn
            .prepare(
                "SELECT r.languages, COUNT(*) FROM variant v
                 JOIN release r ON r.id = v.release_id
                 WHERE COALESCE(r.languages, '') <> ''
                 GROUP BY r.languages",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|source| self.err(source))?;
        let mut by_language: std::collections::BTreeMap<String, u64> =
            std::collections::BTreeMap::new();
        for row in rows {
            let (combination, count) = row.map_err(|source| self.err(source))?;
            let count = u64::try_from(count).unwrap_or(0);
            for language in combination
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
            {
                *by_language.entry(language.to_string()).or_default() += count;
            }
        }
        out.languages = rank_facets(
            by_language
                .into_iter()
                .map(|(value, count)| Facet { value, count })
                .collect(),
        );

        let mut statement = self
            .conn
            .prepare(
                "SELECT c.chinese, COUNT(DISTINCT c.variant_key) FROM candidate c
                 WHERE c.accepted <> 0 AND c.chinese IS NOT NULL
                 GROUP BY c.chinese",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok(Facet {
                    value: row.get(0)?,
                    count: u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                })
            })
            .map_err(|source| self.err(source))?;
        out.chinese = rank_facets(
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|source| self.err(source))?,
        );

        let mut statement = self
            .conn
            .prepare("SELECT state, COUNT(*) FROM identification GROUP BY state")
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|source| self.err(source))?;
        let mut by_state: std::collections::BTreeMap<String, u64> =
            std::collections::BTreeMap::new();
        let mut concluded: u64 = 0;
        for row in rows {
            let (label, count) = row.map_err(|source| self.err(source))?;
            let count = u64::try_from(count).unwrap_or(0);
            concluded += count;
            *by_state.entry(label).or_default() += count;
        }
        let total = self.variant_total(&VariantQuery::default())?;
        out.states = StateFilter::ALL
            .into_iter()
            .map(|filter| match filter {
                StateFilter::Concluded(state) => {
                    (filter, by_state.get(state.label()).copied().unwrap_or(0))
                }
                // 「还没识别」数不出行来——它恰恰是**没有行**的那些，只能拿总数减。
                StateFilter::Unidentified => (filter, total.saturating_sub(concluded)),
            })
            .collect();

        Ok(out)
    }
}

// ══ 作品级的主列表 ═══════════════════════════════════════════════════════════
//
// 变体表是**磁盘上有什么**，主列表是**我有哪些游戏**。真库里 46,444 个变体收敛成
// 一万出头的行，六成作品下面挂着不止一个变体——翻库时人认的是后者，变体在详情面板里挑。
//
// ## 为什么不是把变体表在界面里聚合
//
// 聚合要先看见全部行才数得出「这个作品有几个变体」。那正是这一层从头到尾在躲的事
// （ADR-0005）。所以它是**新的一个查询面**：`GROUP BY` 在 SQLite 里做，界面照旧
// 只拿视口那几十行。
//
// ## 认不出作品的那些**自成一行**，不许被吞掉
//
// 与导出那一侧同一条口径（`adapter::converge` 的 `Anchor::Work` / `Anchor::Loose`）：
// 识别认出作品的按作品收敛，没认出来的一个变体一行。真库上后者是一多半——把它们
// 折进「未知」那一行，等于让人在界面上看不见自己一半的库。

/// 主列表这一行**是谁**。
///
/// 两支的分野正是 `converge` 那条：识别认出作品的按作品收敛，没认出来的一个变体一行。
///
/// **它只在一趟之内有效。** [`Work`](Self::Work) 里那个数是 `work` 表的行号，而重跑
/// 识别会把自己上一轮造的作品整批删掉再造一遍（[`Catalog::clear_identifications`]），
/// 新造出来的行拿的是新的号。所以它是**界面上这一屏的临时身份**，不是能存起来的东西
/// ——要存的东西（裁决、收藏）一律挂**内容锚**或作品名，见 `catalog::scrape` 的
/// 模块文档。界面在重跑识别之后要把选中与点开的那一行一起丢掉。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorkAnchor {
    /// 认出了**作品**：这一行是那个作品，底下挂着它的全部变体。
    ///
    /// 那个数是 `work` 表的行号，**重跑识别就换一批**（见枚举文档）。
    Work(i64),
    /// **还没认出作品**：这一行就是那一个变体，键是它自己。
    Loose(String),
}

/// 主列表按哪一列排。
///
/// 与 [`VariantOrder`] 一样是个闭集合：拼进 `ORDER BY` 的只能来自这里。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkOrder {
    /// 作品名。没认出作品的那些行用它自己的键——**排的与画的是同一串字**。
    #[default]
    Name,
    /// 平台。一行可以跨好几个平台（同一部作品在 GB 与 GBC 上各有变体），
    /// 排的是这一行平台集合里**排最前**的那个。
    Platform,
    /// 变体数。
    Variants,
    /// 容量合计（下界，ADR-0021）。
    Bytes,
    /// 年份。
    Year,
}

impl WorkOrder {
    /// 全部可排的列，界面照这个次序摆表头。
    pub const ALL: [Self; 5] = [
        Self::Name,
        Self::Platform,
        Self::Variants,
        Self::Bytes,
        Self::Year,
    ];

    /// 这一列在界面上叫什么。用**词表**里的词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "作品",
            Self::Platform => "平台",
            Self::Variants => "变体",
            Self::Bytes => "容量",
            Self::Year => "年份",
        }
    }

    /// 拼进 `ORDER BY` 的那个名字。**只有这个函数认得它们**，而且它们全是
    /// [`WORK_COLUMNS`] 里自己起的别名，一个字都不来自外面。
    fn column(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Platform => "platform",
            Self::Variants => "variants",
            Self::Bytes => "bytes",
            Self::Year => "year",
        }
    }
}

/// 主列表上「**元数据齐不齐**」看的是作品这一层该有的哪几样。
///
/// **标题不在里面**：叫法是个集合不是单值（`catalog::title`），缺不缺由标题集合自己答。
/// **汉化组也不在里面**：它挂在**变体**上（ADR-0012），一个作品下有汉化版才谈得上，
/// 拿它当作品级的缺口会让一整排原版游戏都显示成「缺」。
pub const WORK_FIELDS: [Field; 5] = [
    Field::Year,
    Field::Publisher,
    Field::Developer,
    Field::Genre,
    Field::Description,
];

/// 主列表的一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkRow {
    /// 这一行是谁。批量操作的作用范围靠它展开（[`Catalog::scoped_variants`]）。
    pub anchor: WorkAnchor,
    /// 画出来的那个名字：作品名，或者那个变体的键。
    pub name: String,
    /// **平台集合**。一部作品可以横跨好几个平台，真库上 9,226 个作品里有 2,314 个如此。
    pub platforms: Vec<String>,
    /// 底下挂着几个变体。**按当前筛选算**——屏上写着几个，批量操作就作用于那几个。
    pub variants: u64,
    /// 容量合计。**是个下界**：元数据读不到的成员按 0 计入（ADR-0021）。
    pub bytes: u64,
    /// 其中有几个成员的元数据读不到。
    pub unreadable_files: u64,
    /// 年份；一条都没刮到时是 `None`。
    pub year: Option<String>,
    /// [`WORK_FIELDS`] 里**一个值都没有**的那几样。空着就是齐了。
    pub missing: Vec<Field>,
    /// 底下那些变体里**最高的那档置信度**（ADR-0002）；
    /// 一条候选都没有时是 `None`，那是**还没识别**，不是「没撞上」。
    pub confidence: Option<Confidence>,
}

impl WorkRow {
    /// 元数据齐了吗。
    #[must_use]
    pub fn complete(&self) -> bool {
        self.missing.is_empty()
    }

    /// 「元数据」那一栏画成什么。齐了就一个字，缺了就点名缺哪几样。
    #[must_use]
    pub fn missing_label(&self) -> String {
        if self.missing.is_empty() {
            return "齐".to_string();
        }
        if self.missing.len() == WORK_FIELDS.len() {
            return "缺全部".to_string();
        }
        format!(
            "缺{}",
            self.missing
                .iter()
                .map(|field| field.label())
                .collect::<Vec<_>>()
                .join("、")
        )
    }

    /// 置信度那一栏画成什么。**「还没识别」是独立的一档**（ADR-0002）。
    #[must_use]
    pub fn confidence_label(&self) -> &'static str {
        match self.confidence {
            Some(Confidence::High) => "高",
            Some(Confidence::Medium) => "中",
            Some(Confidence::Low) => "低",
            None => "还没识别",
        }
    }
}

/// 主列表一次翻页要的是哪一段。
///
/// 六个筛选维度**加上那棵条件组**与变体表**共用一份**（[`VariantQuery::where_clause`]）：规格里那条
/// 贯穿全局的约定是「**主列表的筛选就是子库的规则**」，而子库选的是变体——两处筛的
/// 若不是同一批变体，界面上筛出来的那批与真正导出去的那批就对不上。
///
/// 只有 [`contains`](Self::contains) 是主列表自己的：变体表按**键**里含什么筛，
/// 主列表按**这一行的名字**里含什么筛。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkQuery {
    /// 这一行的名字里含这个子串才算数；空串等于不筛。
    pub contains: String,
    /// 只要这个**平台**的变体；[`PlatformFilter::Unknown`] 选的是平台未知那一档。
    pub platform: Option<PlatformFilter>,
    /// 只要在这个**合集**里的变体。
    pub collection: Option<String>,
    /// 只要发行版标着这个语言的变体。
    pub language: Option<String>,
    /// 只要带这个**中文身份**记号的变体：`汉化` / `官中`（ADR-0012）。
    pub chinese: Option<String>,
    /// 只要**识别状态**是这一档的变体。
    pub state: Option<StateFilter>,
    /// **筛选器那棵条件树**。见 [`VariantQuery::rule`]——两处共用同一份。
    pub rule: Option<Rule>,
    /// 按哪一列排。
    pub order: WorkOrder,
    /// 倒着排。
    pub descending: bool,
}

/// 当前筛选里**写不成规则**的那一条。
///
/// 存成子库时它必须挡住而不是被悄悄漏掉：漏一条，子库选出来的就比屏上多，
/// 而「筛出来的这一批」与「导过去的那一批」对不上正是这份规格要消灭的东西。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unruly {
    /// **搜索框**里打了字。搜索与筛选是两件事：搜索管的是排序，筛选管的是集合
    /// （票 `gui-redesign/05`）。
    Search,
    /// 按**识别状态**筛了。规则语言里没有这一维——子库的规则是「我要什么内容」，
    /// 而「这个变体识别到哪一步了」是这一趟的进度，下次重跑就变。
    State,
    /// 选的是**平台未知**那一档。写成规则要把全部已知平台列一遍，而清单一变那条规则
    /// 就悄悄失效——宁可不写。
    UnknownPlatform,
    /// 这一维选中的值**里面带逗号**，而逗号是规则里的值分隔符，写进去会被读成两个值。
    Comma(Dimension),
    /// 这一维选中的值里有规则语言的记号（两侧带空白的连接词、或者没配对的括号），
    /// 写进去读回来就不是它自己了。
    ///
    /// **合集名是用户自己起的**，「送朋友的 或 备份」「口袋(日版」都合法——
    /// 折成规则那一步得挡住它，而不是折出一条读回来变了样的规则。
    Unwritable(Dimension),
}

impl Unruly {
    /// 界面上照原样印的那句话。**说清是哪一条、以及怎么办**。
    #[must_use]
    pub fn advice(self) -> String {
        match self {
            Self::Search => "搜索框里还有字。搜索管的是排序、筛选管的是集合，\
                             它进不了规则——先把搜索框清空。"
                .to_string(),
            Self::State => "还筛着「识别状态」。那是这一趟的进度不是内容，\
                            重跑识别就变，写不进子库的规则——先把它设回「不筛」。"
                .to_string(),
            Self::UnknownPlatform => "还筛着「平台未知」。写成规则要把全部已知平台列一遍，\
                                      而清单一变那条规则就悄悄失效——先把平台设回「不筛」。"
                .to_string(),
            Self::Comma(dimension) => format!(
                "选中的那个{}里带逗号，而逗号是规则里的值分隔符，写进去会被读成两个值。",
                dimension.label()
            ),
            Self::Unwritable(dimension) => format!(
                "选中的那个{}里有规则语言的记号（两侧带空白的「且」「或」，                 或者没配对的括号），写进规则读回来就不是它自己了。",
                dimension.label()
            ),
        }
    }
}

/// 把左栏一个一按就有的维度折成子句。
fn facet_clause(dimension: Dimension, value: &str) -> Result<Node, Unruly> {
    if value.contains(',') {
        return Err(Unruly::Comma(dimension));
    }
    // **`Clause::build` 那两道闸也得翻过来**：合集名是用户自己起的，
    // 「送朋友的 或 备份」「口袋(日版」都合法，而它们折成规则读回来就不是自己了。
    // 静默少写一条的话，子库选出来的比屏上多——那正是 `Unruly` 要防的事。
    Clause::build(dimension, Op::Is, value)
        .map(Node::Clause)
        .map_err(|_| Unruly::Unwritable(dimension))
}

/// 一次批量操作作用在**哪些行**上。
///
/// 两支的形状照 ADR-0016 那条「**规则加手动例外**」来：全选不是把一万行的键抓进内存，
/// 它就是**当前这个筛选**本身，再减去人点掉的那几行。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope<'a> {
    /// 点选的这几行。空的就是一行都没选，展开出来一个变体都没有。
    Rows(&'a [WorkAnchor]),
    /// **全选**：当前筛选下的每一行，减去点掉的这几行。
    AllExcept(&'a [WorkAnchor]),
}

/// 主列表那条查询的 `SELECT` 列表。**只有这一处写这些别名**——`ORDER BY` 拼的就是它们。
///
/// 几处值得说明的写法：
///
/// - `group_concat` **不给 `DISTINCT`**：去重与定序反正要在 Rust 那边做一遍
///   （[`platform_set`] 排序定序——`group_concat` 不保证次序，而同一份库问两次必须
///   一样），SQLite 那一侧的 `DISTINCT` 聚合要为每一组多建一棵临时 b 树，白花的。
///   平台名里没有逗号（它来自平台清单），所以逗号拆得回来。
/// - 平台未知那一档**不塞一个约定字符串进 SQL**：`MIN` 与 `group_concat` 都跳过 `NULL`，
///   另数一列 `unknowns` 出来，标签在 Rust 那边补（同 [`PlatformFilter`] 的道理）。
/// - `MIN(year)` 只是个取值器：同一行里 `year` 是常数（作品名一样，join 出来的就是同一条）。
const WORK_COLUMNS: &str = "\
    variant.work_id AS work_id,
    CASE WHEN variant.work_id IS NULL THEN variant.key END AS loose,
    COALESCE(work.name, variant.key) AS name,
    COUNT(*) AS variants,
    SUM(variant.bytes) AS bytes,
    SUM(variant.unreadable) AS unreadable,
    MIN(variant.platform) AS platform,
    group_concat(variant.platform) AS platforms,
    SUM(variant.platform IS NULL) AS unknowns,
    MIN(year.value) AS year";

/// 主列表那条查询的 `FROM` 的头一半：变体连它的作品。
///
/// 单独拆出来是给[数总行数](Catalog::work_total)用的——**那一条不必连年份那一张**：
/// 年份既不进 `WHERE` 也不进它的 `ORDER BY`，多连一张表只是白扫一遍。
const WORK_FROM_BASE: &str = "
    FROM variant
    LEFT JOIN work ON work.id = variant.work_id";

/// 主列表那条查询的 `FROM`：变体、它的作品、以及**年份**那一列。
///
/// 年份要能排序，所以它必须在这条查询里，不能留到取回来之后再补。它挂的锚点两支
/// 不同——认出作品的挂**作品名**，没认出来的挂**变体的键**（与 `converge` 同一条口径）。
///
/// 同一个作品的年份可以有好几条（一个源一条，三元组并存不互相覆盖）。这里的取法是
/// **裁决优先，其次取最早的那一个**：裁决排在每个字段的最前是优先级表的第一条规则
/// （ADR-0001）；而一部作品跨地区先后发行好几次，**最早的那次才是它的年份**。
///
/// 参数按出现次序绑：`裁决`、`年份`、`变体`、`作品`。
const WORK_FROM: &str = "
    FROM variant
    LEFT JOIN work ON work.id = variant.work_id
    LEFT JOIN (SELECT anchor, subject,
                      COALESCE(MIN(CASE WHEN source = ? THEN value END), MIN(value)) AS value
                 FROM scrape_value WHERE field = ?
                GROUP BY anchor, subject) year
           ON year.subject = COALESCE(work.name, variant.key)
          AND year.anchor = CASE WHEN variant.work_id IS NULL THEN ? ELSE ? END";

/// 一行一个作品：认出作品的按 `work_id` 归堆，没认出来的按自己的键各成一堆。
///
/// **不能只写 `GROUP BY work_id`**：`NULL` 在 `GROUP BY` 里是同一堆，那会把真库里
/// 一多半的变体压成一行。
const WORK_GROUP_BY: &str =
    " GROUP BY variant.work_id, CASE WHEN variant.work_id IS NULL THEN variant.key END";

/// 年份那条 join 的四个参数，按 [`WORK_FROM`] 里的出现次序。
fn year_args() -> Vec<Box<dyn ToSql>> {
    vec![
        Box::new(crate::scrape::priority::VERDICT.to_string()),
        Box::new(Field::Year.label().to_string()),
        Box::new(AnchorKind::Variant.label().to_string()),
        Box::new(AnchorKind::Work.label().to_string()),
    ]
}

/// 把 `? , ? , ?` 拼出 `n` 个来。
fn placeholders(n: usize) -> String {
    std::iter::repeat_n("?", n).collect::<Vec<_>>().join(", ")
}

impl WorkQuery {
    /// 两份查询**筛的是不是同一批**（排序不算）。
    ///
    /// 界面拿它分清两种「换过了」：换**排序**只是把同一批行重排，选中的那几行一条都没变；
    /// 换**筛选**才是换了一批行，那时全选说的「当前筛出来的这一批」已经不是同一批，
    /// 留着上一批的选中会让批量操作作用到人根本没看见的行上。
    ///
    /// 判断落在这一层而不是界面里：哪几个字段是**筛选**、哪几个是**排法**，
    /// 是这个查询面自己的事（ADR-0005）。
    #[must_use]
    pub fn same_filter(&self, other: &Self) -> bool {
        self.contains == other.contains
            && self.platform == other.platform
            && self.collection == other.collection
            && self.language == other.language
            && self.chinese == other.chinese
            && self.state == other.state
            && self.rule == other.rule
    }

    /// **当前筛选原样变成的那条规则**——「存成子库」按下去时走的就是这里。
    ///
    /// 这是这份规格里那条贯穿全局的约定落成代码的地方：**主列表的筛选就是子库的规则**。
    /// 左栏那几个一按就有的维度折成子句，手搭的那棵条件树原样接上来，合起来是一个
    /// 「全部满足」组——与屏上「各维之间是且」写的是同一句话。
    ///
    /// 一个条件都没有时是 `Ok(None)`：那不是「选不中任何东西」，是「整个库」。
    /// 拿它去建子库是不是个好主意由调用方判断（多半不是）。
    ///
    /// # Errors
    /// 屏上有条件**写不成规则**时返回它，见 [`Unruly`]。**不是少写一条就算了**：
    /// 少一条的子库选出来的比屏上多，而那正是这条约定要防的事。
    pub fn to_rule(&self) -> Result<Option<Rule>, Unruly> {
        if !self.contains.is_empty() {
            return Err(Unruly::Search);
        }
        if self.state.is_some() {
            return Err(Unruly::State);
        }
        let mut nodes: Vec<Node> = Vec::new();
        match &self.platform {
            None => {}
            Some(PlatformFilter::Unknown) => return Err(Unruly::UnknownPlatform),
            Some(PlatformFilter::Named(platform)) => {
                nodes.push(facet_clause(Dimension::Platform, platform)?);
            }
        }
        for (dimension, picked) in [
            (Dimension::Collection, &self.collection),
            (Dimension::Language, &self.language),
            (Dimension::Chinese, &self.chinese),
        ] {
            if let Some(value) = picked {
                nodes.push(facet_clause(dimension, value)?);
            }
        }
        if let Some(rule) = &self.rule {
            // 手搭的那棵树若顶层本来就是「全部满足」，摊进来而不是再套一层括号——
            // 印出来的那行字是用户要照着核对的东西，白多的括号是噪音。
            if rule.root.join == Join::All {
                nodes.extend(rule.root.nodes.iter().cloned());
            } else {
                nodes.push(Node::Group(rule.root.clone()));
            }
        }
        if nodes.is_empty() {
            return Ok(None);
        }
        Ok(Some(Rule::from_group(Group::new(Join::All, nodes))))
    }

    /// 把一条规则预填进筛选器：子库屏点「改选择」跳回浏览屏时走的那条路
    /// （票 `gui-redesign/11`）。排序不属于筛选，留给调用方自己补。
    #[must_use]
    pub fn from_rule(rule: Rule) -> Self {
        Self {
            rule: Some(rule),
            ..Self::default()
        }
    }

    /// 变体那一层的筛选。**与变体表一字不差地共用**——见结构体文档。
    fn variant_filter(&self) -> VariantQuery {
        VariantQuery {
            // 变体表那一维筛的是**键**，主列表筛的是**名字**，不共用。
            contains: String::new(),
            platform: self.platform.clone(),
            collection: self.collection.clone(),
            language: self.language.clone(),
            chinese: self.chinese.clone(),
            state: self.state,
            rule: self.rule.clone(),
            order: VariantOrder::default(),
            descending: false,
        }
    }

    /// 折出 `WHERE` 那一段与它的参数。
    ///
    /// 名字那一条**也落在 `WHERE` 而不是 `HAVING`**：它是逐行判得了的条件，
    /// 搁在 `HAVING` 里就得先把全部行分完组才筛得掉。
    fn where_clause(&self) -> (String, Vec<Box<dyn ToSql>>) {
        let (mut sql, mut args) = self.variant_filter().where_clause();
        if !self.contains.is_empty() {
            sql.push_str(if sql.is_empty() { " WHERE " } else { " AND " });
            sql.push_str("COALESCE(work.name, variant.key) LIKE ? ESCAPE '\\'");
            args.push(Box::new(format!("%{}%", escape_like(&self.contains))));
        }
        (sql, args)
    }

    /// 折出 `ORDER BY` 那一段。
    ///
    /// **末尾一律缀上那一行的身份**（名字、`work_id`、那个键），理由与变体表同一条：
    /// 并列行的次序不定死，翻页就会漏行与重行。`(work_id, loose)` 是这张表的主键，
    /// 全序由它兜住。
    fn order_clause(&self) -> String {
        let direction = if self.descending { "DESC" } else { "ASC" };
        let column = self.order.column();
        let mut parts = vec![format!("{column} {direction}")];
        for tail in ["name", "work_id", "loose"] {
            if tail != column {
                parts.push(format!("{tail} {direction}"));
            }
        }
        format!(" ORDER BY {}", parts.join(", "))
    }
}

/// 把 `group_concat` 那一串拆回平台集合，排好序、去掉空的。
///
/// 排序在这里做而不是 SQL 里：`group_concat` 不保证次序，而同一份库问两次必须一样。
fn platform_set(joined: Option<String>, unknowns: u64) -> Vec<String> {
    let mut out: Vec<String> = joined
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect();
    out.sort();
    out.dedup();
    if unknowns > 0 {
        out.push(UNKNOWN_PLATFORM_LABEL.to_string());
    }
    out
}

/// 把候选那一列的置信度折成一个可比的名次；认不出的当**低置信**。
///
/// 那两个词**照旧走参数**（[`confidence_rank_args`]），与这一层别处一个规矩：
/// 拼进 SQL 的只有这个文件里写死的那些字。
const CONFIDENCE_RANK: &str = "MIN(CASE candidate.confidence WHEN ? THEN 0 WHEN ? THEN 1 ELSE 2 END)";

/// [`CONFIDENCE_RANK`] 的两个参数，按出现次序。
fn confidence_rank_args() -> Vec<Box<dyn ToSql>> {
    vec![
        Box::new(Confidence::High.label().to_string()),
        Box::new(Confidence::Medium.label().to_string()),
    ]
}

/// 名次折回置信度。
fn confidence_of_rank(rank: i64) -> Confidence {
    match rank {
        0 => Confidence::High,
        1 => Confidence::Medium,
        _ => Confidence::Low,
    }
}

impl Catalog {
    /// 满足这个筛选条件的主列表一共几行。**滚动条的长度**由它来。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn work_total(&self, query: &WorkQuery) -> Result<u64, CatalogError> {
        let (where_sql, args) = query.where_clause();
        let sql = format!(
            "SELECT COUNT(*) FROM \
             (SELECT variant.work_id{WORK_FROM_BASE}{where_sql}{WORK_GROUP_BY})"
        );
        let count: i64 = self
            .conn
            .query_row(&sql, params_from_iter(args.iter()), |row| row.get(0))
            .map_err(|source| self.err(source))?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    /// 取主列表的一页：从第 `offset` 行起、最多 `limit` 行，已排好序。
    ///
    /// `limit` 会被夹到 [`MAX_PAGE`]——这个入口同样不接受「把全库读出来」。
    ///
    /// **两趟**：一趟 `GROUP BY` 出这一页的骨架，再拿这一页那几十行去补
    /// 「元数据齐不齐」与「最高置信度」。后两样各要连一张大表，摊在整库上做的话
    /// 每翻一页都要多扫两遍；而它们**排不了序**（表头上没有这两列），所以补在后面
    /// 不会让这一页的次序变。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn work_page(
        &self,
        query: &WorkQuery,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<WorkRow>, CatalogError> {
        let limit = limit.min(MAX_PAGE);
        if limit == 0 {
            return Ok(Vec::new());
        }
        let (where_sql, where_args) = query.where_clause();
        let order_sql = query.order_clause();
        let mut args = year_args();
        args.extend(where_args);
        args.push(Box::new(i64::try_from(limit).unwrap_or(i64::MAX)));
        args.push(Box::new(i64::try_from(offset).unwrap_or(i64::MAX)));
        let sql = format!(
            "SELECT {WORK_COLUMNS}{WORK_FROM}{where_sql}{WORK_GROUP_BY}{order_sql} \
             LIMIT ? OFFSET ?"
        );
        let mut statement = self.conn.prepare(&sql).map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params_from_iter(args.iter()), |row| {
                let work_id: Option<i64> = row.get(0)?;
                let loose: Option<String> = row.get(1)?;
                let unknowns = u64::try_from(row.get::<_, i64>(8)?).unwrap_or(0);
                Ok(WorkRow {
                    anchor: match (work_id, loose) {
                        (Some(id), _) => WorkAnchor::Work(id),
                        (None, Some(key)) => WorkAnchor::Loose(key),
                        // 分组键的两支必有其一；真到不了这里，兜个不会撞上的值。
                        (None, None) => WorkAnchor::Loose(String::new()),
                    },
                    name: row.get(2)?,
                    variants: u64::try_from(row.get::<_, i64>(3)?).unwrap_or(0),
                    bytes: u64::try_from(row.get::<_, i64>(4)?).unwrap_or(0),
                    unreadable_files: u64::try_from(row.get::<_, i64>(5)?).unwrap_or(0),
                    platforms: platform_set(row.get(7)?, unknowns),
                    year: row.get(9)?,
                    // 这两样下面补。
                    missing: WORK_FIELDS.to_vec(),
                    confidence: None,
                })
            })
            .map_err(|source| self.err(source))?;
        let mut out: Vec<WorkRow> = rows
            .collect::<Result<_, _>>()
            .map_err(|source| self.err(source))?;
        self.fill_missing(&mut out)?;
        self.fill_confidence(query, &mut out)?;
        Ok(out)
    }

    /// 补上这一页每一行「**元数据齐不齐**」。
    ///
    /// 锚点两支分开问：认出作品的看**作品**锚点，没认出来的看**变体**锚点——
    /// 与 `converge` 挑值时走的是同一条岔路，于是屏上写着「齐」的那一行，导出时
    /// 真的填得满。
    fn fill_missing(&self, rows: &mut [WorkRow]) -> Result<(), CatalogError> {
        for anchor in [AnchorKind::Work, AnchorKind::Variant] {
            let subjects: Vec<&str> = rows
                .iter()
                .filter(|row| matches!(row.anchor, WorkAnchor::Work(_)) == (anchor == AnchorKind::Work))
                .map(|row| row.name.as_str())
                .collect();
            if subjects.is_empty() {
                continue;
            }
            let sql = format!(
                "SELECT subject, field FROM scrape_value
                  WHERE anchor = ? AND field IN ({}) AND subject IN ({})
                  GROUP BY subject, field",
                placeholders(WORK_FIELDS.len()),
                placeholders(subjects.len()),
            );
            let mut args: Vec<Box<dyn ToSql>> = vec![Box::new(anchor.label().to_string())];
            for field in WORK_FIELDS {
                args.push(Box::new(field.label().to_string()));
            }
            for subject in &subjects {
                args.push(Box::new((*subject).to_string()));
            }
            let mut statement = self.conn.prepare(&sql).map_err(|source| self.err(source))?;
            let found = statement
                .query_map(params_from_iter(args.iter()), |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|source| self.err(source))?;
            let mut have: std::collections::BTreeSet<(String, String)> =
                std::collections::BTreeSet::new();
            for row in found {
                have.insert(row.map_err(|source| self.err(source))?);
            }
            for row in rows
                .iter_mut()
                .filter(|row| matches!(row.anchor, WorkAnchor::Work(_)) == (anchor == AnchorKind::Work))
            {
                row.missing = WORK_FIELDS
                    .into_iter()
                    .filter(|field| {
                        !have.contains(&(row.name.clone(), field.label().to_string()))
                    })
                    .collect();
            }
        }
        Ok(())
    }

    /// 补上这一页每一行的**最高置信度**。
    ///
    /// 算的是**当前筛选下**那些变体上的候选：屏上那一行写着几个变体，这一档就是那几个
    /// 变体里最有把握的那条结论。一条候选都没有就留 `None`——那是**还没识别**，
    /// 与「撞过没撞上」不是一回事（ADR-0002）。
    fn fill_confidence(&self, query: &WorkQuery, rows: &mut [WorkRow]) -> Result<(), CatalogError> {
        let works: Vec<i64> = rows
            .iter()
            .filter_map(|row| match row.anchor {
                WorkAnchor::Work(id) => Some(id),
                WorkAnchor::Loose(_) => None,
            })
            .collect();
        let loose: Vec<&str> = rows
            .iter()
            .filter_map(|row| match &row.anchor {
                WorkAnchor::Loose(key) => Some(key.as_str()),
                WorkAnchor::Work(_) => None,
            })
            .collect();
        let mut by_work: std::collections::BTreeMap<i64, Confidence> =
            std::collections::BTreeMap::new();
        if !works.is_empty() {
            let (where_sql, where_args) = query.where_clause();
            let sql = format!(
                "SELECT variant.work_id, {CONFIDENCE_RANK}
                   FROM variant
                   JOIN candidate ON candidate.variant_key = variant.key
                   LEFT JOIN work ON work.id = variant.work_id
                  {where_sql}{glue} variant.work_id IN ({ids})
                  GROUP BY variant.work_id",
                glue = if where_sql.is_empty() { " WHERE" } else { " AND" },
                ids = placeholders(works.len()),
            );
            let mut args = confidence_rank_args();
            args.extend(where_args);
            for id in &works {
                args.push(Box::new(*id));
            }
            let mut statement = self.conn.prepare(&sql).map_err(|source| self.err(source))?;
            let found = statement
                .query_map(params_from_iter(args.iter()), |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
                })
                .map_err(|source| self.err(source))?;
            for row in found {
                let (id, rank) = row.map_err(|source| self.err(source))?;
                by_work.insert(id, confidence_of_rank(rank));
            }
        }
        let mut by_key: std::collections::BTreeMap<String, Confidence> =
            std::collections::BTreeMap::new();
        if !loose.is_empty() {
            // 没认出作品的那些行本来就是一个变体一行，页上那几个键已经是筛过的，
            // 不必再把筛选条件带一遍。
            let sql = format!(
                "SELECT candidate.variant_key, {CONFIDENCE_RANK}
                   FROM candidate WHERE candidate.variant_key IN ({keys})
                  GROUP BY candidate.variant_key",
                keys = placeholders(loose.len()),
            );
            let mut args = confidence_rank_args();
            args.extend(
                loose
                    .iter()
                    .map(|key| Box::new((*key).to_string()) as Box<dyn ToSql>),
            );
            let mut statement = self.conn.prepare(&sql).map_err(|source| self.err(source))?;
            let found = statement
                .query_map(params_from_iter(args.iter()), |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })
                .map_err(|source| self.err(source))?;
            for row in found {
                let (key, rank) = row.map_err(|source| self.err(source))?;
                by_key.insert(key, confidence_of_rank(rank));
            }
        }
        for row in rows {
            row.confidence = match &row.anchor {
                WorkAnchor::Work(id) => by_work.get(id).copied(),
                WorkAnchor::Loose(key) => by_key.get(key).copied(),
            };
        }
        Ok(())
    }

    /// 一次批量操作**作用于哪些变体**。
    ///
    /// 这是这一票要钉死的那半条选中语义：**选中主列表的行 ＝ 选中这些作品，批量操作
    /// 作用于它们的变体**。另半条（在详情面板里选中某一个变体，变体级操作只作用于它）
    /// 不必经过这里——那时手上就是一个键。
    ///
    /// 作用范围随**当前筛选**收窄，与屏上那一行写着的变体数是同一个数。不筛的时候
    /// 它就是那些作品的**全部**变体。
    ///
    /// 它会把这一批的键整个列出来——**批量操作必须列得出作用范围才谈得上作用范围**。
    /// 浏览那一半照旧只有视口那几十行，两件事不共用一条路。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn scoped_variants(
        &self,
        query: &WorkQuery,
        scope: Scope<'_>,
    ) -> Result<Vec<String>, CatalogError> {
        let Some((sql, args)) = scoped_sql(query, scope, "variant.key") else {
            return Ok(Vec::new());
        };
        let mut statement = self.conn.prepare(&sql).map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params_from_iter(args.iter()), |row| row.get::<_, String>(0))
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 这份作用范围里有多少个变体。
    ///
    /// 与 [`scoped_variants`](Self::scoped_variants) 分开：屏上那句「作用于多少个变体」
    /// 每帧都要，而它只要一个数——把四万个键读回来只为数一遍，正是这一层要防的事。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn scoped_variant_total(
        &self,
        query: &WorkQuery,
        scope: Scope<'_>,
    ) -> Result<u64, CatalogError> {
        let Some((sql, args)) = scoped_sql(query, scope, "COUNT(*)") else {
            return Ok(0);
        };
        let count: i64 = self
            .conn
            .query_row(&sql, params_from_iter(args.iter()), |row| row.get(0))
            .map_err(|source| self.err(source))?;
        Ok(u64::try_from(count).unwrap_or(0))
    }
}

/// 折出「这份作用范围里的变体」那条查询；一行都没选中时是 `None`。
///
/// `pick` 是要取的那一列（键，或者一个 `COUNT(*)`）——两处共用一条 `WHERE`，
/// 免得「屏上说作用于多少个」与「按下去真动了多少个」漂开。
fn scoped_sql(
    query: &WorkQuery,
    scope: Scope<'_>,
    pick: &str,
) -> Option<(String, Vec<Box<dyn ToSql>>)> {
    {
        let (where_sql, where_args) = query.where_clause();
        let (anchors, negated) = match scope {
            Scope::Rows(anchors) => (anchors, false),
            Scope::AllExcept(anchors) => (anchors, true),
        };
        if anchors.is_empty() && !negated {
            return None;
        }
        let works: Vec<i64> = anchors
            .iter()
            .filter_map(|anchor| match anchor {
                WorkAnchor::Work(id) => Some(*id),
                WorkAnchor::Loose(_) => None,
            })
            .collect();
        let loose: Vec<&str> = anchors
            .iter()
            .filter_map(|anchor| match anchor {
                WorkAnchor::Loose(key) => Some(key.as_str()),
                WorkAnchor::Work(_) => None,
            })
            .collect();
        let mut args: Vec<Box<dyn ToSql>> = where_args;
        let mut sql = format!(
            "SELECT {pick} FROM variant LEFT JOIN work ON work.id = variant.work_id{where_sql}"
        );
        if !anchors.is_empty() {
            let mut parts: Vec<String> = Vec::new();
            if !works.is_empty() {
                // `work_id IN (…)` 在 `work_id` 为 `NULL` 时求值出 `NULL`，而
                // `NOT NULL` 还是 `NULL`——不裹这一层，全选减例外会把**还没认出作品**
                // 的那些行整批漏掉（真库上那是一多半）。
                parts.push(format!(
                    "COALESCE(variant.work_id IN ({}), 0)",
                    placeholders(works.len()),
                ));
                for id in &works {
                    args.push(Box::new(*id));
                }
            }
            if !loose.is_empty() {
                parts.push(format!(
                    "(variant.work_id IS NULL AND variant.key IN ({}))",
                    placeholders(loose.len()),
                ));
                for key in &loose {
                    args.push(Box::new((*key).to_string()));
                }
            }
            sql.push_str(if where_sql.is_empty() { " WHERE " } else { " AND " });
            if negated {
                sql.push_str("NOT ");
            }
            sql.push_str(&format!("({})", parts.join(" OR ")));
        }
        if pick == "variant.key" {
            sql.push_str(" ORDER BY variant.key");
        }
        Some((sql, args))
    }
}

/// 主列表点开一行之后，详情面板里那一个变体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkVariant {
    /// 变体本身。
    pub row: VariantRow,
    /// 这一轮的识别结论；压根没识别过时是 `None`。
    pub state: Option<State>,
    /// 没定下来的话，**为什么**。
    pub reason: Option<String>,
    /// 全部候选，**每条带着置信度与依据**。没有依据的候选事后无法复核（ADR-0002）。
    pub candidates: Vec<Candidate>,
}

impl WorkVariant {
    /// 这个变体最高的那档置信度；一条候选都没有时是 `None`（**还没识别**）。
    #[must_use]
    pub fn confidence(&self) -> Option<Confidence> {
        self.candidates
            .iter()
            .map(|candidate| candidate.confidence)
            .min()
    }
}

/// 主列表点开一行之后，详情面板上摆的那一份。
///
/// 三层里的头两层（**作品** → **变体**）在这儿；第三层**文件**跟着选中的那个变体走
/// （[`Catalog::variant_members`](Catalog::variant_members)），因为一个变体可以是
/// 一整个目录，真库上最大的一份底下有 21,436 个文件——不选中就整份读出来，
/// 点一行的代价会跟着最大的那个变体走。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkDetail {
    /// 这一行是谁。
    pub anchor: WorkAnchor,
    /// 画出来的那个名字。
    pub name: String,
    /// 年份；一条都没刮到时是 `None`。
    pub year: Option<String>,
    /// 平台集合。
    pub platforms: Vec<String>,
    /// 底下挂着的全部变体（按当前筛选），按键排。
    pub variants: Vec<WorkVariant>,
}

impl Catalog {
    /// 主列表某一行的详情；这一行在当前筛选下一个变体都不剩时是 `None`。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn work_detail(
        &self,
        query: &WorkQuery,
        anchor: &WorkAnchor,
    ) -> Result<Option<WorkDetail>, CatalogError> {
        let keys = self.scoped_variants(query, Scope::Rows(std::slice::from_ref(anchor)))?;
        if keys.is_empty() {
            return Ok(None);
        }
        let mut variants = Vec::with_capacity(keys.len());
        let mut platforms: Vec<String> = Vec::new();
        for key in &keys {
            let Some(row) = self.variant(key)? else {
                continue;
            };
            platforms.push(
                row.platform
                    .clone()
                    .unwrap_or_else(|| UNKNOWN_PLATFORM_LABEL.to_string()),
            );
            let (state, reason) = match self.identification_of(key)? {
                Some((state, reason)) => (Some(state), reason),
                None => (None, None),
            };
            variants.push(WorkVariant {
                candidates: self.candidates_of(key)?,
                row,
                state,
                reason,
            });
        }
        platforms.sort();
        platforms.dedup();
        let name = match anchor {
            WorkAnchor::Work(id) => self.work_name(*id)?.unwrap_or_default(),
            WorkAnchor::Loose(key) => key.clone(),
        };
        let year = self.work_year(anchor, &name)?;
        Ok(Some(WorkDetail {
            anchor: anchor.clone(),
            name,
            year,
            platforms,
            variants,
        }))
    }

    /// 一行的年份。取法与主列表那一列**一模一样**（裁决优先，其次最早的那一个），
    /// 否则面板上写的与列表上写的会是两个数。
    fn work_year(&self, anchor: &WorkAnchor, name: &str) -> Result<Option<String>, CatalogError> {
        let kind = match anchor {
            WorkAnchor::Work(_) => AnchorKind::Work,
            WorkAnchor::Loose(_) => AnchorKind::Variant,
        };
        self.conn
            .prepare_cached(
                "SELECT COALESCE(MIN(CASE WHEN source = ?3 THEN value END), MIN(value))
                   FROM scrape_value WHERE anchor = ?1 AND subject = ?2 AND field = ?4",
            )
            .and_then(|mut statement| {
                statement.query_row(
                    rusqlite::params![
                        kind.label(),
                        name,
                        crate::scrape::priority::VERDICT,
                        Field::Year.label(),
                    ],
                    |row| row.get::<_, Option<String>>(0),
                )
            })
            .map_err(|source| self.err(source))
    }
}
