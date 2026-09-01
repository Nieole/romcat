//! **规则**：选择集里可重放的那一半。
//!
//! 一条规则是一句「我要什么」，写成若干**子句**用 `且` 连起来：
//!
//! ```text
//! 平台=GB,GBA 且 中文=汉化 且 体积<=64MiB
//! ```
//!
//! 一个选择集里可以有多条规则，**多条之间是并集**——各选各的，合起来就是规则那一半
//! 选中的全部。子句之间是交集，这样「所有某平台的汉化版」写成一行就够，而「再加上
//! 另一个平台的官中版」另起一条，不必把两句话拧成一个表达式。
//!
//! ## 为什么是文本而不是一组开关
//!
//! 规则要**存得住、看得见、改得动**：它落在中立库里跨进程活着，也要能原样印进报告让
//! 用户核对自己写的是什么（ADR-0016 的「可重放」说的正是这件事）。一组结构化开关存进
//! 数据库要么摊成十几列，要么塞成一团 JSON——前者每加一个维度就改一次表，后者用户
//! 看不见自己写了什么。文本两头都占得住：一列存得下，一行印得出。
//!
//! ## 值取不到时怎么算
//!
//! 一条**明确的**约定，九个维度一视同仁：**取不到值时，`!=` 成立，别的一律不成立。**
//!
//! 年份没刮到的变体，`年份>=1990` 不匹配（不知道就是不知道，不能当成匹配放进去），
//! 而 `年份!=1990` 匹配（它确实不是 1990 年）。这条约定必须写死并说出口——留给直觉
//! 的话，同一条规则在两个人脑子里会算出两个结果。

use std::fmt;

/// 子句之间的连接词。**要求两侧有空白**，于是值里出现这个字也不会被当成分隔符。
const AND: char = '且';

/// 规则里能筛的**维度**。
///
/// 九个维度各自对应中立库里的一列事实（见 [`VariantFacts`](super::VariantFacts)）。
///
/// **`评分` 眼下没有源**：`scrape::Field` 里根本没有这个字段，一条值也产不出来
/// （挂账 D68）。维度照立，因为等 ScreenScraper 的凭据到位、值落进库那天，规则不必
/// 改一个字就生效；而在那之前 [`选择集报告`](super::report::SelectionReport) 会点名
/// 说「这条规则引到的 `评分` 在这份库里一条数据都没有」——**选不出东西时说清是缺数据
/// 还是写错了**，比让用户对着 0 猜强。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Dimension {
    /// **平台**：变体所属的硬件系统。
    Platform,
    /// **语言**：发行版的语言标记（`Ja`、`En`、`Zh`）。
    Language,
    /// 变体的中文身份：`汉化` 或 `官中`（ADR-0012）。
    Chinese,
    /// **合集**：用户自定义的一组游戏，与平台正交。
    Collection,
    /// 刮削来的类型。
    Genre,
    /// **作品**名。
    Work,
    /// 刮削来的发行年份。
    Year,
    /// 评分，0.0–1.0。
    Rating,
    /// 变体的容量。**这是个下界**——元数据读不到的成员按 0 计入（ADR-0021）。
    Size,
}

impl Dimension {
    /// 规则里写的那个词，**也就是词表里的那个词**。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Platform => "平台",
            Self::Language => "语言",
            Self::Chinese => "中文",
            Self::Collection => "合集",
            Self::Genre => "类型",
            Self::Work => "作品",
            Self::Year => "年份",
            Self::Rating => "评分",
            Self::Size => "体积",
        }
    }

    /// 一句话说清这个维度筛的是什么，`romcat sublibrary rule` 的帮助里印它。
    #[must_use]
    pub fn hint(self) -> &'static str {
        match self {
            Self::Platform => "变体所属的硬件系统，如 `平台=GB,GBA`",
            Self::Language => "发行版的语言标记，如 `语言=Zh`",
            Self::Chinese => "变体的中文身份：`汉化` 或 `官中`",
            Self::Collection => "用户自定义的合集（**眼下还没有建合集的办法**，挂账 D74）",
            Self::Genre => "刮削来的类型",
            Self::Work => "作品名，配 `~` 取子串，如 `作品~火焰纹章`",
            Self::Year => "刮削来的发行年份，如 `年份>=1990`",
            Self::Rating => "评分，写 0–1 的小数或 `80%`",
            Self::Size => "变体的容量，如 `体积<=64MiB`（是下界，ADR-0021）",
        }
    }

    /// 帮助与报告里固定的排列顺序。
    #[must_use]
    pub fn all() -> [Self; 9] {
        [
            Self::Platform,
            Self::Language,
            Self::Chinese,
            Self::Collection,
            Self::Genre,
            Self::Work,
            Self::Year,
            Self::Rating,
            Self::Size,
        ]
    }

    /// 从规则里写的那个词认回来。
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        Self::all().into_iter().find(|d| d.label() == label)
    }

    /// 这个维度装的是文字还是数。
    #[must_use]
    pub fn is_number(self) -> bool {
        matches!(self, Self::Year | Self::Rating | Self::Size)
    }
}

/// 子句里的运算符。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Op {
    /// `=`：是其中之一。
    Is,
    /// `!=`：一个都不是。
    IsNot,
    /// `~`：含有这段文字。
    Contains,
    /// `<=`。
    Le,
    /// `<`。
    Lt,
    /// `>=`。
    Ge,
    /// `>`。
    Gt,
}

impl Op {
    /// 规则里写的那个符号。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Is => "=",
            Self::IsNot => "!=",
            Self::Contains => "~",
            Self::Le => "<=",
            Self::Lt => "<",
            Self::Ge => ">=",
            Self::Gt => ">",
        }
    }

    /// 认符号的顺序：**长的排在前面**，否则 `<=` 会被 `<` 先咬掉一半。
    fn candidates() -> [Self; 7] {
        [
            Self::IsNot,
            Self::Le,
            Self::Ge,
            Self::Is,
            Self::Contains,
            Self::Lt,
            Self::Gt,
        ]
    }

    /// 这个符号配得上这个维度吗。
    fn fits(self, dimension: Dimension) -> bool {
        if dimension.is_number() {
            !matches!(self, Self::Contains)
        } else {
            matches!(self, Self::Is | Self::IsNot | Self::Contains)
        }
    }
}

/// 子句右边那一半：一串文字，或者一个数。
#[derive(Debug, Clone, PartialEq)]
pub enum Bound {
    /// 一串文字，**彼此之间是或**。
    Text(Vec<String>),
    /// 一个数。数值维度只收一个——`年份<=1990,2000` 说不清是什么意思。
    Number(f64),
}

/// 一个子句：`维度 运算符 值`。
#[derive(Debug, Clone, PartialEq)]
pub struct Clause {
    /// 筛哪一维。
    pub dimension: Dimension,
    /// 怎么比。
    pub op: Op,
    /// 比什么。
    pub bound: Bound,
}

/// 一条规则：若干子句，**彼此之间是且**。
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    /// 用户写的原文。报告原样印它——用户核对的是自己写的那句话，不是我们重排的版本。
    pub text: String,
    /// 拆出来的子句。
    pub clauses: Vec<Clause>,
}

/// 规则写得不对。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuleError {
    /// 空规则。
    #[error("规则是空的。写法是 `维度 运算符 值`，多个子句用 ` 且 ` 连起来")]
    Empty,
    /// 认不出这个维度。
    #[error("认不出维度「{text}」。能筛的是：{known}")]
    UnknownDimension {
        /// 用户写的那个词。
        text: String,
        /// 认得的那几个。
        known: String,
    },
    /// 这一段里没有运算符。
    #[error("子句「{text}」里没有运算符。能用的是 = != ~ <= < >= >")]
    MissingOperator {
        /// 那一段原文。
        text: String,
    },
    /// 运算符配不上这个维度。
    #[error("{dimension} 用不了 `{op}`：{advice}")]
    BadOperator {
        /// 哪个维度。
        dimension: &'static str,
        /// 写的哪个符号。
        op: &'static str,
        /// 该怎么写。
        advice: &'static str,
    },
    /// 值是空的。
    #[error("{dimension} 后面没写值")]
    EmptyValue {
        /// 哪个维度。
        dimension: &'static str,
    },
    /// 值不是个数。
    #[error("{dimension} 要的是{expected}，「{value}」不是")]
    BadNumber {
        /// 哪个维度。
        dimension: &'static str,
        /// 该写成什么样。
        expected: &'static str,
        /// 用户写的那个值。
        value: String,
    },
    /// 数值维度给了不止一个值。
    #[error("{dimension} 只收一个值，「{value}」给了 {count} 个")]
    TooManyValues {
        /// 哪个维度。
        dimension: &'static str,
        /// 用户写的那一段。
        value: String,
        /// 给了几个。
        count: usize,
    },
}

impl Rule {
    /// 读一条规则。
    ///
    /// # Errors
    /// 维度认不出、运算符配不上、值不是个数时返回错误。**在写进中立库之前就报出来**：
    /// 一条存进去才发现读不懂的规则，下次同步时才炸，那时用户早忘了自己写了什么。
    pub fn parse(text: &str) -> Result<Self, RuleError> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(RuleError::Empty);
        }
        let mut clauses = Vec::new();
        for piece in split_clauses(trimmed) {
            clauses.push(Clause::parse(piece)?);
        }
        if clauses.is_empty() {
            return Err(RuleError::Empty);
        }
        Ok(Self {
            text: trimmed.to_string(),
            clauses,
        })
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// 按**两侧带空白的** `且` 把规则切成一个个子句。
///
/// 要求两侧有空白不是挑剔：作品名里完全可能有这个字（「一将功成万骨枯」那类），
/// 见字就切会把它劈成两半。
fn split_clauses(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    for (index, ch) in text.char_indices() {
        if ch != AND {
            continue;
        }
        let before = text[..index].chars().next_back();
        let after = text[index + ch.len_utf8()..].chars().next();
        let padded =
            before.is_some_and(char::is_whitespace) && after.is_some_and(char::is_whitespace);
        if padded {
            out.push(text[start..index].trim());
            start = index + ch.len_utf8();
        }
    }
    debug_assert!(start <= bytes.len());
    out.push(text[start..].trim());
    out.retain(|piece| !piece.is_empty());
    out
}

impl Clause {
    /// 读一个子句。
    ///
    /// # Errors
    /// 见 [`RuleError`]。
    pub fn parse(text: &str) -> Result<Self, RuleError> {
        let (head, op, tail) = split_operator(text).ok_or_else(|| RuleError::MissingOperator {
            text: text.to_string(),
        })?;
        let name = head.trim();
        let dimension = Dimension::from_label(name).ok_or_else(|| RuleError::UnknownDimension {
            text: name.to_string(),
            known: Dimension::all()
                .into_iter()
                .map(Dimension::label)
                .collect::<Vec<_>>()
                .join("、"),
        })?;
        if !op.fits(dimension) {
            return Err(RuleError::BadOperator {
                dimension: dimension.label(),
                op: op.label(),
                advice: if dimension.is_number() {
                    "数值维度用 = != <= < >= >"
                } else {
                    "文字维度用 = != ~"
                },
            });
        }
        let values: Vec<String> = tail
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect();
        if values.is_empty() {
            return Err(RuleError::EmptyValue {
                dimension: dimension.label(),
            });
        }
        let bound = if dimension.is_number() {
            if values.len() > 1 {
                return Err(RuleError::TooManyValues {
                    dimension: dimension.label(),
                    value: tail.trim().to_string(),
                    count: values.len(),
                });
            }
            Bound::Number(parse_number(dimension, &values[0])?)
        } else {
            Bound::Text(values)
        };
        Ok(Self {
            dimension,
            op,
            bound,
        })
    }
}

/// 找出这一段里的运算符，切成 `(左, 符号, 右)`。
fn split_operator(text: &str) -> Option<(&str, Op, &str)> {
    let mut best: Option<(usize, Op)> = None;
    for op in Op::candidates() {
        if let Some(at) = text.find(op.label()) {
            // 位置最靠前的那个符号才是真正的分界；同一位置上取先认出来的（长的排在前）。
            if best.is_none_or(|(current, _)| at < current) {
                best = Some((at, op));
            }
        }
    }
    let (at, op) = best?;
    Some((&text[..at], op, &text[at + op.label().len()..]))
}

/// 把值读成一个数。
fn parse_number(dimension: Dimension, value: &str) -> Result<f64, RuleError> {
    let bad = |expected: &'static str| RuleError::BadNumber {
        dimension: dimension.label(),
        expected,
        value: value.to_string(),
    };
    match dimension {
        Dimension::Year => value
            .parse::<i32>()
            .map(f64::from)
            .map_err(|_| bad("一个年份，如 1990")),
        Dimension::Rating => parse_rating(value).ok_or_else(|| bad("0 到 1 的小数，或 `80%`")),
        Dimension::Size => parse_size(value)
            .map(|bytes| {
                #[allow(clippy::cast_precision_loss)]
                {
                    bytes as f64
                }
            })
            .ok_or_else(|| bad("一个容量，如 `64MiB`、`1.5GiB`、`512GB`")),
        _ => Err(bad("一个数")),
    }
}

/// 读一个评分：`0.85` 或 `85%`，都折成 0–1。
#[must_use]
pub fn parse_rating(text: &str) -> Option<f64> {
    let text = text.trim();
    let value = if let Some(percent) = text.strip_suffix('%') {
        percent.trim().parse::<f64>().ok()? / 100.0
    } else {
        text.parse::<f64>().ok()?
    };
    (value.is_finite() && (0.0..=1.0).contains(&value)).then_some(value)
}

/// 读一个容量：`4096`、`64MiB`、`1.5GiB`、`512GB`。
///
/// **二进制与十进制单位都收，但必须写全**：`GiB` 是 1024³，`GB` 是 1000³，光写 `G`
/// 一律拒绝。一张标着 512GB 的卡实际是 476.84 GiB，两个数差 7%——在一个专门为
/// 「装不装得下」服务的字段上，让用户猜我们按哪种算是不可接受的。
///
/// 二进制那几个与 [`human_bytes`](crate::report::human_bytes) 印出来的单位逐字相同，
/// 于是报告里读到的数字可以原样抄回规则里。
#[must_use]
pub fn parse_size(text: &str) -> Option<u64> {
    const UNITS: [(&str, f64); 9] = [
        ("KiB", 1024.0),
        ("MiB", 1024.0 * 1024.0),
        ("GiB", 1024.0 * 1024.0 * 1024.0),
        ("TiB", 1024.0 * 1024.0 * 1024.0 * 1024.0),
        ("KB", 1000.0),
        ("MB", 1_000_000.0),
        ("GB", 1_000_000_000.0),
        ("TB", 1_000_000_000_000.0),
        ("B", 1.0),
    ];
    let text = text.trim();
    for (suffix, scale) in UNITS {
        // `get` 而不是下标：值里可以是任意 UTF-8（`大` 这种），切在字符中间会 panic。
        // 单位大小写随便写：`64mib` 与 `64MiB` 是同一个数。
        let Some(at) = text.len().checked_sub(suffix.len()) else {
            continue;
        };
        if text
            .get(at..)
            .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix))
        {
            let number: f64 = text.get(..at)?.trim().parse().ok()?;
            return finite_bytes(number * scale);
        }
    }
    finite_bytes(text.parse::<f64>().ok()?)
}

fn finite_bytes(value: f64) -> Option<u64> {
    #[allow(clippy::cast_precision_loss)]
    let ceiling = u64::MAX as f64;
    if !value.is_finite() || value < 0.0 || value >= ceiling {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(value.round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 一条像样的规则拆得开() {
        let rule = Rule::parse("平台=GB,GBA 且 中文=汉化 且 体积<=64MiB").expect("读得懂");
        assert_eq!(rule.clauses.len(), 3);
        assert_eq!(rule.clauses[0].dimension, Dimension::Platform);
        assert_eq!(
            rule.clauses[0].bound,
            Bound::Text(vec!["GB".to_string(), "GBA".to_string()])
        );
        assert_eq!(rule.clauses[2].op, Op::Le);
        assert_eq!(rule.clauses[2].bound, Bound::Number(64.0 * 1024.0 * 1024.0));
        // 原文一字不改地留着：报告里印的是用户写的那句话。
        assert_eq!(rule.text, "平台=GB,GBA 且 中文=汉化 且 体积<=64MiB");
    }

    #[test]
    fn 值里的且不当分隔符() {
        // 两侧没空白就不是连接词，否则作品名一带这个字就被劈成两半。
        let rule = Rule::parse("作品~一将功成万骨枯").expect("读得懂");
        assert_eq!(rule.clauses.len(), 1);
        assert_eq!(
            rule.clauses[0].bound,
            Bound::Text(vec!["一将功成万骨枯".to_string()])
        );
    }

    #[test]
    fn 长符号先认() {
        let rule = Rule::parse("年份>=1990").expect("读得懂");
        assert_eq!(rule.clauses[0].op, Op::Ge);
        assert_eq!(rule.clauses[0].bound, Bound::Number(1990.0));
        let rule = Rule::parse("平台!=PSV").expect("读得懂");
        assert_eq!(rule.clauses[0].op, Op::IsNot);
    }

    #[test]
    fn 写错了当场说清楚() {
        assert!(matches!(Rule::parse("  "), Err(RuleError::Empty)));
        assert!(matches!(
            Rule::parse("标签=汉化"),
            Err(RuleError::UnknownDimension { .. })
        ));
        assert!(matches!(
            Rule::parse("平台"),
            Err(RuleError::MissingOperator { .. })
        ));
        assert!(matches!(
            Rule::parse("体积~大"),
            Err(RuleError::BadOperator { .. })
        ));
        assert!(matches!(
            Rule::parse("年份<=1990,2000"),
            Err(RuleError::TooManyValues { .. })
        ));
        assert!(matches!(
            Rule::parse("年份>=很久以前"),
            Err(RuleError::BadNumber { .. })
        ));
        assert!(matches!(
            Rule::parse("平台="),
            Err(RuleError::EmptyValue { .. })
        ));
    }

    #[test]
    fn 容量单位必须写全() {
        assert_eq!(parse_size("4096"), Some(4096));
        assert_eq!(parse_size("64MiB"), Some(64 * 1024 * 1024));
        assert_eq!(parse_size("64mib"), Some(64 * 1024 * 1024));
        assert_eq!(parse_size("1.5GiB"), Some(1024 * 1024 * 1024 * 3 / 2));
        // 十进制与二进制差 7%，各写各的，绝不互相猜。
        assert_eq!(parse_size("512GB"), Some(512_000_000_000));
        assert_ne!(parse_size("512GB"), parse_size("512GiB"));
        // 光写一个字母说不清是哪一种，拒绝。
        assert_eq!(parse_size("512G"), None);
        assert_eq!(parse_size("大"), None);
    }

    #[test]
    fn 评分两种写法折成同一个数() {
        assert_eq!(parse_rating("0.8"), Some(0.8));
        assert_eq!(parse_rating("80%"), Some(0.8));
        assert_eq!(parse_rating("1.5"), None);
        assert_eq!(parse_rating("-1"), None);
    }
}
