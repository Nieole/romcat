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

use rusqlite::{ToSql, params_from_iter};

use super::content::{VARIANT_COLUMNS, VariantRow, read_variant_row};
use super::identify::State;
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    ///
    /// 每一条都是**参数化**的：拼进 SQL 的只有这个函数里写死的那些字，用户在筛选框里
    /// 打什么都只会落进 `?`。
    fn where_clause(&self) -> (String, Vec<Box<dyn ToSql>>) {
        let mut parts: Vec<&'static str> = Vec::new();
        let mut args: Vec<Box<dyn ToSql>> = Vec::new();
        if !self.contains.is_empty() {
            parts.push("key LIKE ? ESCAPE '\\'");
            args.push(Box::new(format!("%{}%", escape_like(&self.contains))));
        }
        match &self.platform {
            None => {}
            // **平台未知那一档要选得中**：它在表里是 `NULL`，而 `= NULL` 永远不成立。
            Some(PlatformFilter::Unknown) => parts.push("platform IS NULL"),
            Some(PlatformFilter::Named(platform)) => {
                parts.push("platform = ?");
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
