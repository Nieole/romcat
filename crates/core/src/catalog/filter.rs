//! 把一条**规则**折成 `WHERE` 里的一段——**筛选下推到中立库**那一半。
//!
//! ## 为什么这一层非有不可
//!
//! 规则本来有一个求值器：[`sublibrary::select`](crate::sublibrary::select)，纯函数，
//! 吃一批 [`VariantFacts`](crate::sublibrary::VariantFacts)。子库那一侧用它正合适
//! ——同步前要把整批选中的东西列出来，本来就得走一遍全库。
//!
//! **浏览屏走不了那条路**：真库四万多个变体，每换一个条件就把全库折成事实再过一遍，
//! 既是几十兆常驻内存，也是每次几百毫秒的卡顿。而这一屏的正题恰恰是「改一个条件
//! 当场看见筛出多少」。所以规则在这里必须变成 SQL（`catalog::browse` 的模块文档
//! 说的是同一件事）。
//!
//! ## 两个求值器必须给出同一批
//!
//! 这是**这一层唯一的硬约束**：屏上筛出来的那批，与「存成子库」之后同步真正搬过去的
//! 那批，得是同一批。两处答案不一样时，用户按下同步才发现——那时错的是数据，不是屏幕。
//!
//! 所以每一维的写法都照着 [`sublibrary::facts`](crate::sublibrary::facts) 那一侧抄：
//!
//! - **中文身份**从**自动通过**的候选上读（`candidate.accepted <> 0`），与
//!   `adapter::converge` 同一条路。
//! - **刮削来的那几维**（类型、年份、开发商、发行商、简介）**作品锚点与变体锚点合起来
//!   看**：年份挂在作品上、汉化组挂在变体上，而规则不该要求用户先弄清某个字段挂在哪一层。
//! - **语言**逐个语言码比，不是在整串上取子串：`en` 不该撞上 `Danish`。
//! - **取不到值时 `!=` 成立、别的一律不成立**——那条约定在 SQL 里就是
//!   `COALESCE(…, 0)`，空集合上求值出假，`!=` 再把它翻过来。
//!
//! 这件事由测试钉着，不是靠两边程序员记性好：`crates/core/tests/filter.rs` 拿同一条
//! 规则各跑一遍，比的是**两批键完全相同**。
//!
//! ## 参数化，一个字都不拼
//!
//! 拼进 SQL 的只有这个文件里写死的那些字；用户在筛选器里打的每一个字符都落进 `?`。
//! `LIKE` 的三个元字符另外转义（`browse::escape_like`），
//! 否则筛选框里打一个 `%` 就等于「什么都匹配」。

use rusqlite::ToSql;

use crate::collection::FAVORITE;
use crate::scrape::{AnchorKind, Field};
use crate::sublibrary::{Bound, Clause, Dimension, Group, Join, Node, Op, RATING_FIELD, Rule};

use super::browse::escape_like;

/// 折出来的一段谓词，连它的参数。
pub(super) type Predicate = (String, Vec<Box<dyn ToSql>>);

/// 把一条规则折成 `WHERE` 里的一段。
pub(super) fn rule_sql(rule: &Rule) -> Predicate {
    group_sql(&rule.root)
}

/// 把一个组折成一段。三种连接各算各的，**空组按各自的中性元算**——与
/// [`sublibrary`](crate::sublibrary) 那一侧逐字一致。
fn group_sql(group: &Group) -> Predicate {
    let mut parts: Vec<String> = Vec::new();
    let mut args: Vec<Box<dyn ToSql>> = Vec::new();
    for node in &group.nodes {
        let (sql, mut more) = match node {
            Node::Clause(clause) => clause_sql(clause),
            Node::Group(inner) => group_sql(inner),
        };
        parts.push(sql);
        args.append(&mut more);
    }
    let sql = match group.join {
        // 空的「全部满足」成立：没有一项不成立。
        Join::All if parts.is_empty() => "1".to_string(),
        // 空的「任一满足」不成立：一项都没有，谈不上有一项成立。
        Join::Any if parts.is_empty() => "0".to_string(),
        // 空的「都不满足」成立：一项都没有，自然一项都没成立。
        Join::None if parts.is_empty() => "1".to_string(),
        Join::All => format!("({})", parts.join(" AND ")),
        Join::Any => format!("({})", parts.join(" OR ")),
        Join::None => format!("NOT ({})", parts.join(" OR ")),
    };
    (sql, args)
}

/// 把一个子句折成一段。
///
/// 两步：先折出「**这一维上有没有一个值让这个比较成立**」，再由 `!=` 决定翻不翻过来。
/// 这正是 `sublibrary::satisfies` 的形状——两处同一个骨架，才谈得上同一个答案。
fn clause_sql(clause: &Clause) -> Predicate {
    let (sql, args) = any_sql(clause);
    if clause.op == Op::IsNot {
        // `COALESCE` 那一层不能省：直接比一个可空的列时结果是 `NULL`，
        // 而 `NOT NULL` 还是 `NULL`——取不到值的那些行会连 `!=` 也选不中。
        (format!("(NOT COALESCE({sql}, 0))"), args)
    } else {
        (format!("COALESCE({sql}, 0)"), args)
    }
}

/// 「这一维上有没有一个值让这个比较成立」。
fn any_sql(clause: &Clause) -> Predicate {
    match clause.dimension {
        // 平台是 `variant` 上的一列，直接比。
        Dimension::Platform => column_any("variant.platform", clause),
        Dimension::Work => {
            let (pred, args) = column_any("w.name", clause);
            (
                format!("EXISTS (SELECT 1 FROM work w WHERE w.id = variant.work_id AND {pred})"),
                args,
            )
        }
        Dimension::Language => language_any(clause),
        Dimension::Chinese => {
            let (pred, args) = column_any("c.chinese", clause);
            (
                format!(
                    "EXISTS (SELECT 1 FROM candidate c
                             WHERE c.variant_key = variant.key AND c.accepted <> 0 AND {pred})"
                ),
                args,
            )
        }
        Dimension::Collection => {
            let (pred, args) = column_any("c.name", clause);
            (
                format!(
                    "EXISTS (SELECT 1 FROM collection_variant cv
                             JOIN collection c ON c.id = cv.collection_id
                             WHERE cv.variant_key = variant.key AND {pred})"
                ),
                args,
            )
        }
        Dimension::Favorite => favorite_any(clause),
        Dimension::Genre => scraped_any(Field::Genre.label(), clause),
        Dimension::Developer => scraped_any(Field::Developer.label(), clause),
        Dimension::Publisher => scraped_any(Field::Publisher.label(), clause),
        Dimension::Description => scraped_any(Field::Description.label(), clause),
        // 体积是 `variant` 上的一列，直接比。
        Dimension::Size => number_any("variant.bytes", clause),
        Dimension::Year => scraped_number(Field::Year.label(), YEAR_VALUE, clause),
        Dimension::Rating => scraped_number(RATING_FIELD, RATING_VALUE, clause),
    }
}

/// 一列文字上的比较：`=` `!=` 逐个值比，`~` `^` `$` 走 `LIKE`。
///
/// **大小写只折 ASCII，两边都是。** SQLite 的 `lower()` 本来就只动 ASCII，所以这边
/// 那个值也得走 `to_ascii_lowercase` 而不是 `to_lowercase`——后者是 Unicode 折叠，
/// 会把 `Ä` 折成 `ä` 而 SQLite 不折，于是同一条规则两个求值器答案就分家了。
/// 汉字两边都不受影响（本来就没有大小写）。
fn column_any(column: &str, clause: &Clause) -> Predicate {
    let Bound::Text(values) = &clause.bound else {
        // 数值维度不会走到这儿（`Op::fits` 拦在解析那一步）。
        return ("0".to_string(), Vec::new());
    };
    let mut parts: Vec<String> = Vec::new();
    let mut args: Vec<Box<dyn ToSql>> = Vec::new();
    for value in values {
        match clause.op {
            Op::Contains | Op::StartsWith | Op::EndsWith => {
                parts.push(format!("lower({column}) LIKE ? ESCAPE '\\'"));
                args.push(Box::new(like_pattern(clause.op, value)));
            }
            _ => {
                parts.push(format!("lower({column}) = ?"));
                args.push(Box::new(value.to_ascii_lowercase()));
            }
        }
    }
    (format!("({})", parts.join(" OR ")), args)
}

/// `~` `^` `$` 各自的 `LIKE` 模板。**转义先做**，否则筛选框里打一个 `%` 就等于
/// 「什么都匹配」，而主库里真有带 `%` 的文件名。
fn like_pattern(op: Op, value: &str) -> String {
    let escaped = escape_like(&value.to_ascii_lowercase());
    match op {
        Op::StartsWith => format!("{escaped}%"),
        Op::EndsWith => format!("%{escaped}"),
        _ => format!("%{escaped}%"),
    }
}

/// **语言逐个码比**。
///
/// `release.languages` 是一串逗号分隔的码。两头各补一个逗号之后，四个运算符正好是
/// 四种 `instr`：`,En,` 是整码相等、`,En` 是以它开头、`En,` 是以它结尾、`En` 是含有。
/// 少了这两个逗号，`en` 会撞上 `Danish`。
///
/// 值里不会有逗号（逗号是规则里的值分隔符），所以子串跨不过码与码的边界。
fn language_any(clause: &Clause) -> Predicate {
    let Bound::Text(values) = &clause.bound else {
        return ("0".to_string(), Vec::new());
    };
    let padded = "',' || lower(replace(COALESCE(r.languages, ''), ' ', '')) || ','";
    let mut parts: Vec<String> = Vec::new();
    let mut args: Vec<Box<dyn ToSql>> = Vec::new();
    for value in values {
        let needle = match clause.op {
            Op::Contains => value.to_ascii_lowercase(),
            Op::StartsWith => format!(",{}", value.to_ascii_lowercase()),
            Op::EndsWith => format!("{},", value.to_ascii_lowercase()),
            _ => format!(",{},", value.to_ascii_lowercase()),
        };
        parts.push(format!("instr({padded}, ?) > 0"));
        args.push(Box::new(needle));
    }
    (
        format!(
            "EXISTS (SELECT 1 FROM release r WHERE r.id = variant.release_id AND ({}))",
            parts.join(" OR ")
        ),
        args,
    )
}

/// **收藏是个是非题**，所以这一维只有两个可能的值：收藏了就是 `是`，没收藏就是 `否`。
///
/// 于是「这一维上有没有一个值让这个比较成立」拆成两问——`是` 成立吗、`否` 成立吗——
/// 两个答案定下四种局面里的哪一种：两个都成立就是全库、都不成立就是空集，
/// 只有其中一个成立时才真去问库。**这与 `sublibrary::VariantFacts::any_text` 那句
/// `hit(if self.favorite { "是" } else { "否" })` 是同一个骨架**，两处必须同一个答案。
///
/// 问库问的是**投影**：收藏落在沉淀库里（`collection::FAVORITE`），中立库里
/// `collection` / `collection_variant` 那两张表是它的投影，识别跑完照沉淀库重建
/// （`collection::project`）。这一层跑在中立库自己的连接上，够不着另一个文件。
fn favorite_any(clause: &Clause) -> Predicate {
    let Bound::Text(values) = &clause.bound else {
        return ("0".to_string(), Vec::new());
    };
    let hit = |candidate: &str| {
        values.iter().any(|value| match clause.op {
            Op::Contains => candidate.contains(value.as_str()),
            Op::StartsWith => candidate.starts_with(value.as_str()),
            Op::EndsWith => candidate.ends_with(value.as_str()),
            _ => value == candidate,
        })
    };
    // 收藏了的那些：投影里有一条名字是「收藏」的成员关系。
    let joined = "EXISTS (SELECT 1 FROM collection_variant cv
                          JOIN collection c ON c.id = cv.collection_id
                          WHERE cv.variant_key = variant.key AND c.name = ?)";
    match (hit("是"), hit("否")) {
        (true, true) => ("1".to_string(), Vec::new()),
        (true, false) => (joined.to_string(), vec![Box::new(FAVORITE.to_string()) as _]),
        (false, true) => (
            format!("(NOT {joined})"),
            vec![Box::new(FAVORITE.to_string()) as _],
        ),
        (false, false) => ("0".to_string(), Vec::new()),
    }
}

/// 一列数上的比较。
fn number_any(column: &str, clause: &Clause) -> Predicate {
    let Bound::Number(value) = clause.bound else {
        return ("0".to_string(), Vec::new());
    };
    let comparison = match clause.op {
        Op::Le => "<=",
        Op::Lt => "<",
        Op::Ge => ">=",
        Op::Gt => ">",
        // `=` 与 `!=` 都先算「等不等」，翻不翻过来是 `clause_sql` 的事。
        _ => "=",
    };
    (
        format!("({column} {comparison} ?)"),
        vec![Box::new(value) as Box<dyn ToSql>],
    )
}

/// 刮削来的那几维**文字**的取法。
///
/// **作品锚点与变体锚点合起来看**，与 [`sublibrary::facts`](crate::sublibrary::facts)
/// 那一侧逐字一致：认出作品的按作品名去查，没认出来的按变体的键去查，而作品锚点上的
/// 值对它底下每一个变体都算数。
fn scraped_any(field: &str, clause: &Clause) -> Predicate {
    let (pred, mut args) = column_any("sv.value", clause);
    let mut head = scraped_head(field);
    head.append(&mut args);
    (
        format!("EXISTS (SELECT 1 FROM scrape_value sv WHERE {SCRAPED_ANCHOR} AND {pred})"),
        head,
    )
}

/// 刮削来的那几维**数**的取法。`reading` 是把那一列文字读成数的那段表达式。
fn scraped_number(field: &str, reading: &str, clause: &Clause) -> Predicate {
    let Bound::Number(value) = clause.bound else {
        return ("0".to_string(), Vec::new());
    };
    let comparison = match clause.op {
        Op::Le => "<=",
        Op::Lt => "<",
        Op::Ge => ">=",
        Op::Gt => ">",
        _ => "=",
    };
    let mut args = scraped_head(field);
    args.push(Box::new(value));
    (
        format!(
            "EXISTS (SELECT 1 FROM scrape_value sv
                     WHERE {SCRAPED_ANCHOR} AND {reading} {comparison} ?)"
        ),
        args,
    )
}

/// `scrape_value` 那条子查询里，锚点与字段那一段。**参数按出现次序**：字段、作品锚点、变体锚点。
const SCRAPED_ANCHOR: &str = "sv.field = ?
      AND ((sv.anchor = ?
            AND sv.subject = (SELECT w.name FROM work w WHERE w.id = variant.work_id))
        OR (sv.anchor = ? AND sv.subject = variant.key))";

/// [`SCRAPED_ANCHOR`] 的三个参数，按出现次序。
fn scraped_head(field: &str) -> Vec<Box<dyn ToSql>> {
    vec![
        Box::new(field.to_string()),
        Box::new(AnchorKind::Work.label().to_string()),
        Box::new(AnchorKind::Variant.label().to_string()),
    ]
}

/// 把年份那一列文字读成数。
///
/// **读不成整数的值一概不算**，与内存那一侧的 `value.trim().parse::<i32>()` 对齐：
/// 刮来的年份里有 `199X` 这种写法，`CAST` 会把它读成 199 而内存那边直接扔掉。
/// 「转回文字还是原样」这一条正好把这类值挡在外面。
const YEAR_VALUE: &str = "CASE WHEN trim(sv.value) = CAST(CAST(trim(sv.value) AS INTEGER) AS TEXT)
                               THEN CAST(trim(sv.value) AS INTEGER) END";

/// 把评分那一列文字读成 0–1 的数：`0.85` 与 `85%` 折成同一个。
///
/// **两道闸照着 [`parse_rating`](crate::sublibrary::rule::parse_rating) 抄**：那边要求
/// 「读得成数」且「落在 0 到 1 之间」，两条都不满足就当它没有。少了这两道闸，
/// `CAST('优秀' AS REAL)` 会读成 `0.0`（在范围里！）、`1.5` 会照收，于是一条
/// `评分<=0.5` 在 SQL 那侧把所有评分写成非数字的变体都选中，内存那侧一个都不选。
///
/// 「读得成数」这条用的是**里面得有个数字**（`GLOB '*[0-9]*'`）——SQLite 没有
/// 「这段文字是不是个数」的问法，而 `CAST` 对读不出数的一律给 `0.0`，
/// 光看值分不出 `'优秀'` 与 `'0'`。
///
/// **眼下一条评分也没有**（挂账 D68：没有任何源产得出评分），所以这一段跑不到真数据上；
/// 剩下那点对不齐（`'0.8x'` 这种半截数）记在挂单 Q72，等值真落进库那天补一次两边比键。
const RATING_VALUE: &str = "CASE
        WHEN trim(sv.value) GLOB '*[0-9]*'
             AND (CASE WHEN trim(sv.value) LIKE '%!%' ESCAPE '!'
                       THEN CAST(trim(rtrim(trim(sv.value), '%')) AS REAL) / 100.0
                       ELSE CAST(trim(sv.value) AS REAL) END) BETWEEN 0.0 AND 1.0
        THEN (CASE WHEN trim(sv.value) LIKE '%!%' ESCAPE '!'
                   THEN CAST(trim(rtrim(trim(sv.value), '%')) AS REAL) / 100.0
                   ELSE CAST(trim(sv.value) AS REAL) END) END";
