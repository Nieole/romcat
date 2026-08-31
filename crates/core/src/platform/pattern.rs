//! 成型规则里那套极小的名字模式。
//!
//! **它存在的唯一理由是让规则留在配置里。** 「TitleID 目录长什么样」「碟片标记长什么样」
//! 都是随平台变的知识，写进 Rust 就等于每加一个平台改一次代码（`docs/platforms.md`：
//! 平台清单是数据不是代码）。但为它引一个正则引擎又太重——真正需要表达的只有三件事：
//! 一位数字、一位字母、以及「后面随便还有什么」。
//!
//! | 记号 | 含义 |
//! |---|---|
//! | `#` | 正好一位 ASCII 数字 |
//! | `@` | 正好一位 ASCII 字母 |
//! | `*` | 任意长度（含零）的任意字符 |
//! | 其余 | 字面量，**大小写不敏感** |
//!
//! 大小写不敏感是有意的：同一个 TitleID 目录在不同转储工具下会写成 `PCSG00042` 或
//! `pcsg00042`，而这两者显然是同一件事。
//!
//! `*` 带回溯，因此最坏情况是指数级——但模式来自配置而不是用户输入，且
//! [`Pattern::new`] 会拒收多于 [`MAX_WILDCARDS`] 个 `*` 的模式，把这条路堵死。

/// 一个模式里最多允许几个 `*`。
///
/// 回溯匹配的代价随 `*` 的个数指数增长。真实规则里 `*` 至多出现一次
/// （`@@@@#####*` 这种「TitleID 打头，后面还跟着中文说明」的目录名），
/// 留 4 个已经宽得离谱，超过就是配置写错了。
pub const MAX_WILDCARDS: usize = 4;

/// 一条编译好的名字模式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    units: Vec<Unit>,
    source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    /// 一位 ASCII 数字。
    Digit,
    /// 一位 ASCII 字母。
    Letter,
    /// 任意长度的任意字符。
    Any,
    /// 一个字面字符，已折成小写。
    Literal(char),
}

/// 模式本身写坏了。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PatternError {
    /// 空模式。空模式会匹配空名字，那多半是配置漏填而不是本意。
    #[error("模式是空的")]
    Empty,
    /// `*` 太多。
    #[error("模式 {pattern} 里有 {found} 个 `*`，最多允许 {MAX_WILDCARDS} 个")]
    TooManyWildcards {
        /// 出问题的模式。
        pattern: String,
        /// 实际有几个。
        found: usize,
    },
}

impl Pattern {
    /// 编译一条模式。
    ///
    /// # Errors
    /// 模式为空、或 `*` 多于 [`MAX_WILDCARDS`] 个时返回错误。
    pub fn new(source: &str) -> Result<Self, PatternError> {
        if source.is_empty() {
            return Err(PatternError::Empty);
        }
        let wildcards = source.chars().filter(|c| *c == '*').count();
        if wildcards > MAX_WILDCARDS {
            return Err(PatternError::TooManyWildcards {
                pattern: source.to_string(),
                found: wildcards,
            });
        }
        let units = source
            .chars()
            .map(|ch| match ch {
                '#' => Unit::Digit,
                '@' => Unit::Letter,
                '*' => Unit::Any,
                other => Unit::Literal(lower(other)),
            })
            .collect();
        Ok(Self {
            units,
            source: source.to_string(),
        })
    }

    /// 模式的原文，报错与展示用。
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// 整个 `text` 是否匹配这条模式。
    #[must_use]
    pub fn matches(&self, text: &str) -> bool {
        let chars: Vec<char> = text.chars().collect();
        matches_from(&self.units, &chars)
    }

    /// `text` 的某个**结尾**是否匹配这条模式，匹配上就返回剩下的那一截。
    ///
    /// 碟片标记要的就是它：`龙骑士传说[简]Disc A` 去掉尾巴的 `Disc A`。
    /// 只认结尾而不认任意位置，是因为标记出现在名字中间的话，去掉它会把两段不相干的
    /// 文字接到一起，凭空造出一个同族。
    ///
    /// 匹配的那一截前面必须是**分隔符**或者字符串开头，否则 `disc @` 会从
    /// `Blitzdisc A` 里啃掉半个词。收尾的括号也算分隔符——真库里的目录名长这样：
    /// `龙骑士传说[简][v1.0][…]Disc A`，标记紧跟在 `]` 后面。
    #[must_use]
    pub fn strip_suffix<'t>(&self, text: &'t str) -> Option<&'t str> {
        let chars: Vec<char> = text.chars().collect();
        // 从最长的尾巴试起：`(disc #)` 遇上 `x (Disc 1)` 要吃掉整个括号段，
        // 而不是只吃掉能凑合匹配的最短一截。
        let mut byte_offsets: Vec<usize> = text.char_indices().map(|(index, _)| index).collect();
        byte_offsets.push(text.len());
        for start in 0..chars.len() {
            if start > 0 && !is_boundary(chars[start - 1]) {
                continue;
            }
            if matches_from(&self.units, &chars[start..]) {
                return Some(&text[..byte_offsets[start]]);
            }
        }
        None
    }
}

fn is_boundary(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '(' | '['
                | '{'
                | ')'
                | ']'
                | '}'
                | '-'
                | '_'
                | '.'
                | '、'
                | '（'
                | '［'
                | '【'
                | '）'
                | '］'
                | '】'
        )
}

fn lower(ch: char) -> char {
    ch.to_lowercase().next().unwrap_or(ch)
}

fn matches_from(units: &[Unit], text: &[char]) -> bool {
    match units.split_first() {
        None => text.is_empty(),
        Some((Unit::Any, rest)) => {
            // 从最长开始试：`*` 后面通常还有字面量，长的那头更快落地。
            (0..=text.len())
                .rev()
                .any(|take| matches_from(rest, &text[take..]))
        }
        Some((unit, rest)) => {
            let Some((head, tail)) = text.split_first() else {
                return false;
            };
            let ok = match unit {
                Unit::Digit => head.is_ascii_digit(),
                Unit::Letter => head.is_ascii_alphabetic(),
                Unit::Literal(expected) => lower(*head) == *expected,
                Unit::Any => unreachable!("上一条分支已经处理"),
            };
            ok && matches_from(rest, tail)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 模式(source: &str) -> Pattern {
        Pattern::new(source).expect("模式编得出来")
    }

    #[test]
    fn 数字位与字母位各认一个字符() {
        let p = 模式("PCS@#####");
        assert!(p.matches("PCSG00042"));
        assert!(p.matches("pcsg00042"), "大小写不敏感");
        assert!(!p.matches("PCSG0042"), "少一位数字");
        assert!(!p.matches("PCSG000420"), "多一位数字");
        assert!(!p.matches("PCS900042"), "字母位上是数字");
    }

    #[test]
    fn 星号吃掉任意长的尾巴() {
        let p = 模式("@@@@#####*");
        assert!(p.matches("AIME00001"), "尾巴可以是空的");
        assert!(p.matches("AIME00001(wan华镜 v3.1)"));
        assert!(!p.matches("AIME0001"), "数字不够");
        assert!(!p.matches("PSVENJP"), "没有五位数字");
    }

    #[test]
    fn 空模式与太多星号都被拒收() {
        assert_eq!(Pattern::new(""), Err(PatternError::Empty));
        let 太多 = "*".repeat(MAX_WILDCARDS + 1);
        assert!(matches!(
            Pattern::new(&太多),
            Err(PatternError::TooManyWildcards { .. })
        ));
    }

    #[test]
    fn 碟片标记只从结尾剥且要挨着分隔符() {
        let p = 模式("disc @");
        assert_eq!(
            p.strip_suffix("龙骑士传说[简] Disc A"),
            Some("龙骑士传说[简] ")
        );
        assert_eq!(
            p.strip_suffix("Blitzdisc A"),
            None,
            "标记前面不是分隔符，不许啃掉半个词"
        );
        assert_eq!(
            p.strip_suffix("龙骑士传说[简][v1.0]Disc A"),
            Some("龙骑士传说[简][v1.0]"),
            "真库里的目录名就长这样，标记紧跟在 `]` 后面"
        );
    }

    #[test]
    fn 括号形的碟片标记整段剥掉() {
        let p = 模式("(disc #)");
        assert_eq!(
            p.strip_suffix("Legend of Dragoon, The (Disc 1)"),
            Some("Legend of Dragoon, The ")
        );
        assert_eq!(p.strip_suffix("Legend of Dragoon, The"), None);
    }

    #[test]
    fn 剥尾巴时取最长的那一截() {
        // `*` 能匹配空串，于是「最短的尾巴」永远是空尾巴。要的是最长的那个。
        let p = 模式("(v*)");
        assert_eq!(p.strip_suffix("游戏 (v1.02)"), Some("游戏 "));
    }

    #[test]
    fn 汉字字面量也认得() {
        let p = 模式("光盘#");
        assert!(p.matches("光盘2"));
        assert_eq!(p.strip_suffix("圣剑传说 光盘2"), Some("圣剑传说 "));
    }
}
