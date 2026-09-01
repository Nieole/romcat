//! **子库**与**选择集**：从主库挑一部分出来，导到某台目标设备上去玩。
//!
//! 主库是 8.6 TiB 的仓储，用户**从不直接在它上面游玩**（ADR-0015）。真正每天做的那件事
//! 是「从主库里挑一批，导到掌机的 SD 卡上」。这个模块立起那件事需要的两个概念。
//!
//! ## 子库是持久实体，不是一次导出的参数
//!
//! 一台目标设备一个子库：带**目标路径**、**前端格式**、**容量上限**，以及一份**选择集**。
//! 它落在中立库里跨进程活着——配一次，此后每次同步都用它。同一个主库上可以同时有好几个
//! 子库（掌机一个、备用卡一个），**互不干扰**：规则各写各的，例外各记各的。
//!
//! ## 选择集 = 可重放的规则 + 优先于规则的手动例外
//!
//! ADR-0016。两半各治一种病，界限不能糊：
//!
//! - **规则**可重放。写下「所有 GB 与 GBA 的汉化版」之后，主库里新增的、规则说得中的内容
//!   **下次自动进入**，不必重挑。规则不存结果只存写法——每次求值都拿当下的中立库现算，
//!   于是「自动进入」不是一个要触发的动作，是这个形状天然给的。多设备时这个收益乘以设备数。
//! - **例外**处理规则表达不了的个人口味（「这个我小时候玩过」「这个太占地方先不带」），
//!   **优先于规则、永久记住**。它是**沉淀**：规则改了、重跑识别、重新成型，一条都不动
//!   ——与 `shaping_override`、`preferred_variant`、`source = 裁决` 的标题是同一条纪律。
//!
//! ## 选中的是**变体**
//!
//! 不是前端条目。理由有三条，每条单独都够：
//!
//! 1. **变体是磁盘上一份实际可玩的东西**，同步要搬的正是它的那几个文件；容量也只在
//!    这一层说得清（`variant.bytes`）。
//! 2. **收敛是导出那一步的事**（`adapter::converge`，按作品 × 平台）。一个前端条目
//!    底下挂着好几个变体，而用户想说的恰恰是「这部作品我只带汉化版，不带日版」——
//!    选在条目那一层就永远表达不了。
//! 3. **例外要挂在一把不随重跑变的键上**，而变体的键（相对主库根的路径，ADR-0020）
//!    正是库里到处在用的那把自然键。
//!
//! ## 求值是纯函数
//!
//! [`select`] 只吃 [`Selection`] 与一批 [`VariantFacts`]，不碰中立库、不碰磁盘。
//! 把库折成事实的那一步是 [`facts`]，单独一个函数。这道缝是故意留的：票 19 的
//! **同步计划器**也是纯函数，两者串起来才能在没有主库、没有目标设备的情况下测出
//! 「这套规则会选出什么、会造成什么差量」。

pub mod report;
pub mod rule;

use std::collections::{BTreeMap, BTreeSet};

use crate::catalog::{Catalog, CatalogError};
use crate::scrape::{AnchorKind, Field};

pub use rule::{Bound, Clause, Dimension, Op, Rule, RuleError};

/// **评分**在 `scrape_value` 里的字段名。
///
/// 它**不在 [`Field`] 里**：眼下没有任何源产得出评分（挂账 D68）。这个常量在这里，
/// 是为了让规则那一维与将来真的落库的那个字段名对得上，而不必现在就在 `Field` 上
/// 立一列谁也填不满的枚举值。
const RATING_FIELD: &str = "评分";

/// 一个**子库**的定义。
///
/// 目标路径与前端格式在这里，**能力档案**（目标设备吃哪些格式）不在——那是票 21 的活。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sublibrary {
    /// 子库叫什么。命令行拿它指名道姓，也是中立库里的主键。
    pub name: String,
    /// 目标设备上的子库根。
    ///
    /// 一律走读卡器（ADR-0015）：SD 卡挂成普通盘，于是这就是本机上的一个绝对路径。
    /// 卡不在位时它照样存着——子库是持久实体，不是「插上卡才存在的东西」。
    pub target: String,
    /// 前端格式（适配器名）。
    pub format: String,
    /// 容量上限，字节；`None` 表示不设限。
    ///
    /// **超限绝不自动截断**（ADR-0016）：报出超出量与按体积排序的裁剪建议，砍谁由用户定。
    /// 自动截断的结果不可预测——同一套规则在两张不同容量的卡上会选出完全不同的东西，
    /// 而用户无从得知它砍掉了什么。
    pub capacity: Option<u64>,
}

/// 一条**例外**是把变体强行拉进来还是踢出去。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Exception {
    /// 强行收入：规则没选中也要带上。
    Include,
    /// 强行排除：规则选中了也不带。
    Exclude,
}

impl Exception {
    /// 存进库、也打给用户的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Include => "收入",
            Self::Exclude => "排除",
        }
    }

    /// 从词认回来。
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        Some(match label {
            "收入" => Self::Include,
            "排除" => Self::Exclude,
            _ => return None,
        })
    }
}

/// 一条存下来的例外。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionRow {
    /// 哪个变体。**键是相对主库根的路径**（ADR-0020）。
    pub variant_key: String,
    /// 收入还是排除。
    pub kind: Exception,
    /// 用户自己写的一句「为什么」；可空。
    ///
    /// 例外治的是规则表达不了的个人口味，而口味半年后就想不起来了。**留一句话的位置**，
    /// 比日后对着一串路径猜自己当初为什么排除它强。
    pub note: Option<String>,
    /// 什么时候记下的（Unix 秒）。
    pub at: i64,
}

/// 一个子库的**选择集**：规则打底，例外覆盖。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Selection {
    /// 规则，**按存进去的顺序**。多条之间是并集。
    pub rules: Vec<Rule>,
    /// 例外。**优先于规则**。
    pub exceptions: Vec<ExceptionRow>,
}

/// 一个变体在选择集眼里的样子。
///
/// 这是 [`select`] 的全部输入——把中立库折成这个形状之后，求值再不碰库一下。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VariantFacts {
    /// 变体的键。
    pub key: String,
    /// 平台；可空。
    pub platform: Option<String>,
    /// 容量。**是下界**：元数据读不到的成员按 0 计入（ADR-0021）。
    pub bytes: u64,
    /// 属于哪个作品；识别还没认出来时是 `None`。
    pub work: Option<String>,
    /// 发行版的语言标记，拆开的。
    pub languages: Vec<String>,
    /// 中文身份：`汉化` / `官中`（ADR-0012）。一个变体可能两个记号都有。
    pub chinese: Vec<String>,
    /// 在哪几个合集里。
    pub collections: Vec<String>,
    /// 刮削来的类型。
    pub genres: Vec<String>,
    /// 刮削来的年份。**可能不止一个**——几个源各给一个，规则按「有一个落在范围里就算」。
    pub years: Vec<f64>,
    /// 刮削来的评分，0–1。眼下没有源（见 [`RATING_FIELD`]）。
    pub ratings: Vec<f64>,
}

impl VariantFacts {
    /// 这个变体在这一维上的文字值里，有没有一个让 `hit` 点头的。
    fn any_text(&self, dimension: Dimension, hit: impl Fn(&str) -> bool) -> bool {
        match dimension {
            Dimension::Platform => self.platform.as_deref().is_some_and(hit),
            Dimension::Work => self.work.as_deref().is_some_and(hit),
            Dimension::Language => self.languages.iter().any(|value| hit(value)),
            Dimension::Chinese => self.chinese.iter().any(|value| hit(value)),
            Dimension::Collection => self.collections.iter().any(|value| hit(value)),
            Dimension::Genre => self.genres.iter().any(|value| hit(value)),
            Dimension::Year | Dimension::Rating | Dimension::Size => false,
        }
    }

    /// 这个变体在这一维上的数值里，有没有一个让 `hit` 点头的。
    fn any_number(&self, dimension: Dimension, hit: impl Fn(f64) -> bool) -> bool {
        match dimension {
            #[allow(clippy::cast_precision_loss)]
            Dimension::Size => hit(self.bytes as f64),
            Dimension::Year => self.years.iter().copied().any(&hit),
            Dimension::Rating => self.ratings.iter().copied().any(&hit),
            _ => false,
        }
    }

    /// 这个变体在这一维上有没有值。报告拿它算「这一维在这份库里有多少数据」。
    #[must_use]
    pub fn has(&self, dimension: Dimension) -> bool {
        if dimension.is_number() {
            self.any_number(dimension, |_| true)
        } else {
            self.any_text(dimension, |_| true)
        }
    }
}

/// 一条从中立库里读回来、却读不懂的规则。
///
/// **一条手改坏了的规则不该让另外五条也用不了**：跳过它、把它连同错在哪一起报出来，
/// 剩下的照常求值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokenRule {
    /// 它在库里的序号。用户按这个号删掉或改写它。
    pub ordinal: i64,
    /// 存进去的原文。
    pub text: String,
    /// 错在哪。
    pub error: RuleError,
}

/// 库里存着的一条规则原文，连它的序号。
///
/// 规则**存原文**而不是存拆开的结构：它要跨进程活着，也要原样印进报告让用户核对
/// 自己写的是什么（见 [`rule`] 的模块文档）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRule {
    /// 序号。命令行按它删。
    pub ordinal: i64,
    /// 规则原文。
    pub text: String,
}

/// 从中立库读回来的一份选择集。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoadedSelection {
    /// 读得懂的那些规则，加上例外。
    pub selection: Selection,
    /// 每条规则在库里的序号，与 [`Selection::rules`] 一一对应。
    ///
    /// 报告要印的是**库里那个号**——用户照着报告删规则，印错一个号就删错一条。
    pub ordinals: Vec<i64>,
    /// 读不懂的那几条。
    pub broken: Vec<BrokenRule>,
}

impl LoadedSelection {
    /// 读一批存着的规则原文，读不懂的挑出来另放。
    ///
    /// 例外由调用方补上——这里只管规则那一半，于是这一步是**纯的**，
    /// 拿一串手写的字符串就测得了。
    ///
    /// 库里怎么会有读不懂的规则？[`Catalog::add_rule`](crate::catalog::Catalog::add_rule)
    /// 只收已经读通了的 [`Rule`]，所以正常路径上不会。但**中立库是个 SQLite 文件，
    /// 人打得开**；换一版程序、删掉一个维度之后旧规则也会读不懂。一条坏的不该让
    /// 另外五条一起用不了。
    #[must_use]
    pub fn from_stored(stored: &[StoredRule]) -> Self {
        let mut out = Self::default();
        for row in stored {
            match Rule::parse(&row.text) {
                Ok(rule) => {
                    out.selection.rules.push(rule);
                    out.ordinals.push(row.ordinal);
                }
                Err(error) => out.broken.push(BrokenRule {
                    ordinal: row.ordinal,
                    text: row.text.clone(),
                    error,
                }),
            }
        }
        out
    }
}

/// 一个变体凭什么进了选择集。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// 第几条规则选中的（从 0 数）。
    Rule(usize),
    /// **例外**收进来的。
    Exception,
}

/// 一个被选中的变体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picked {
    /// 变体的键。
    pub key: String,
    /// 平台；可空。
    pub platform: Option<String>,
    /// 容量（下界）。
    pub bytes: u64,
    /// 凭什么进来的。
    pub why: Why,
}

/// 求一次值的产物与它的账。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Selected {
    /// 选中的变体，**按键排**——同一份库求两次值必须是同一个结果。
    pub picked: Vec<Picked>,
    /// 选中的容量合计（下界）。
    pub bytes: u64,
    /// 每条规则各命中多少个变体。
    ///
    /// **逐条各自算，不扣例外、也不扣与别条的重叠**：这个数回答的是「我这条规则写对了吗」，
    /// 扣掉重叠之后它就变成了「这条规则的边际贡献」——那是另一个问题，而且答案取决于
    /// 规则的排列顺序，用户核对不了。
    pub rule_hits: Vec<u64>,
    /// 例外收入了几个（变体在库里的那些）。
    pub forced_in: u64,
    /// 其中规则本来也会选中的——**这几条例外是多余的**，删掉不影响结果。
    pub forced_in_redundant: u64,
    /// 例外排除了几个。
    pub forced_out: u64,
    /// 其中规则本来会选中的——**这几条例外真的起了作用**。
    pub forced_out_effective: u64,
    /// 指向库里没有这个变体的例外。
    ///
    /// **不是错误，也不删**：盘没插、目录改了名、内容暂时不在，例外照旧记着（ADR-0016
    /// 那句「永久记住」）。如实报出来即可。
    pub missing_exceptions: Vec<String>,
    /// 规则引到、但这份中立库里一条数据都没有的维度。
    ///
    /// 选不出东西时，**是缺数据还是规则写错了**，这一列直接分开。
    pub thin_dimensions: Vec<&'static str>,
}

/// 按选择集在一批事实上求值。**纯函数**：不碰中立库、不碰磁盘、不看时钟。
#[must_use]
pub fn select(selection: &Selection, facts: &[VariantFacts]) -> Selected {
    let exceptions: BTreeMap<&str, Exception> = selection
        .exceptions
        .iter()
        .map(|row| (row.variant_key.as_str(), row.kind))
        .collect();
    let mut referenced: BTreeSet<Dimension> = BTreeSet::new();
    for rule in &selection.rules {
        for condition in &rule.clauses {
            referenced.insert(condition.dimension);
        }
    }
    let mut covered: BTreeSet<Dimension> = BTreeSet::new();

    let mut out = Selected {
        rule_hits: vec![0; selection.rules.len()],
        ..Selected::default()
    };
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for variant in facts {
        seen.insert(variant.key.as_str());
        for dimension in &referenced {
            if !covered.contains(dimension) && variant.has(*dimension) {
                covered.insert(*dimension);
            }
        }
        let mut by_rule: Option<usize> = None;
        for (index, rule) in selection.rules.iter().enumerate() {
            if matches(rule, variant) {
                out.rule_hits[index] += 1;
                by_rule.get_or_insert(index);
            }
        }
        // **例外先看，而且看完就定。** 规则算出什么都不改变这一步的结论——
        // 「优先于规则」在代码里就该长成这样。
        let why = match exceptions.get(variant.key.as_str()) {
            Some(Exception::Exclude) => {
                out.forced_out += 1;
                if by_rule.is_some() {
                    out.forced_out_effective += 1;
                }
                continue;
            }
            Some(Exception::Include) => {
                out.forced_in += 1;
                if by_rule.is_some() {
                    out.forced_in_redundant += 1;
                }
                Why::Exception
            }
            None => match by_rule {
                Some(index) => Why::Rule(index),
                None => continue,
            },
        };
        out.bytes += variant.bytes;
        out.picked.push(Picked {
            key: variant.key.clone(),
            platform: variant.platform.clone(),
            bytes: variant.bytes,
            why,
        });
    }
    out.missing_exceptions = selection
        .exceptions
        .iter()
        .filter(|row| !seen.contains(row.variant_key.as_str()))
        .map(|row| row.variant_key.clone())
        .collect();
    out.thin_dimensions = referenced
        .difference(&covered)
        .map(|dimension| dimension.label())
        .collect();
    out.picked.sort_by(|a, b| a.key.cmp(&b.key));
    out
}

/// 这条规则选中这个变体吗——子句之间是**且**。
fn matches(rule: &Rule, facts: &VariantFacts) -> bool {
    rule.clauses
        .iter()
        .all(|condition| satisfies(condition, facts))
}

/// 这个子句成立吗。
///
/// **取不到值时 `!=` 成立、别的一律不成立**（见 [`rule`] 的模块文档）：那一条约定
/// 在这里就是「先算 `any`，再由 `!=` 把它翻过来」——空集合上 `any` 是假，翻过来是真。
fn satisfies(condition: &Clause, facts: &VariantFacts) -> bool {
    let any = match &condition.bound {
        Bound::Text(wanted) => facts.any_text(condition.dimension, |value| match condition.op {
            Op::Contains => wanted.iter().any(|want| contains_ignore_case(value, want)),
            _ => wanted.iter().any(|want| value.eq_ignore_ascii_case(want)),
        }),
        Bound::Number(wanted) => {
            facts.any_number(condition.dimension, |value| match condition.op {
                Op::Le => value <= *wanted,
                Op::Lt => value < *wanted,
                Op::Ge => value >= *wanted,
                Op::Gt => value > *wanted,
                _ => (value - *wanted).abs() <= 1e-9,
            })
        }
    };
    if condition.op == Op::IsNot { !any } else { any }
}

/// 忽略 ASCII 大小写的子串判断。汉字不受影响（本来就没有大小写）。
fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    if haystack.is_ascii() && needle.is_ascii() {
        return haystack
            .to_ascii_lowercase()
            .contains(&needle.to_ascii_lowercase());
    }
    haystack.contains(needle)
}

/// 把中立库折成 [`select`] 要的那批事实。
///
/// **不碰主库、不联网**：要的东西全在中立库里躺着（ADR-0001）。真库上这是一趟
/// 46,444 个变体、78,902 条刮削值的顺读。
///
/// # Errors
/// 读中立库失败时返回错误。
pub fn facts(catalog: &Catalog) -> Result<Vec<VariantFacts>, CatalogError> {
    let variants = catalog.variants()?;
    let works = catalog.work_names()?;
    let releases = catalog.releases()?;
    let collections = catalog.collection_memberships()?;

    // 变体上的中文记号，从**自动通过**的候选上读回来（与 `adapter::converge` 同一条路）。
    let mut chinese: BTreeMap<String, BTreeSet<&'static str>> = BTreeMap::new();
    catalog.for_each_accepted_candidate(&mut |candidate| {
        if let Some(mark) = candidate.chinese {
            chinese
                .entry(candidate.variant_key.to_string())
                .or_default()
                .insert(mark.label());
        }
    })?;

    // 刮削那一侧。**作品锚点与变体锚点合起来看**：年份挂在作品上、汉化组挂在变体上，
    // 而规则不该要求用户先弄清某个字段挂在哪一层。
    let mut by_work: BTreeMap<String, Vec<(ScrapedInto, String)>> = BTreeMap::new();
    let mut by_variant: BTreeMap<String, Vec<(ScrapedInto, String)>> = BTreeMap::new();
    catalog.for_each_scraped_value(&mut |anchor, subject, value| {
        let Some(into) = ScrapedInto::of(&value.field) else {
            return;
        };
        let bucket = if anchor == AnchorKind::Work.label() {
            &mut by_work
        } else {
            &mut by_variant
        };
        bucket
            .entry(subject.to_string())
            .or_default()
            .push((into, value.value));
    })?;

    let mut out = Vec::with_capacity(variants.len());
    for variant in variants {
        let work = variant.work_id.and_then(|id| works.get(&id)).cloned();
        let languages = variant
            .release_id
            .and_then(|id| releases.get(&id))
            .and_then(|release| release.languages.as_deref())
            .map(|text| {
                text.split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let mut row = VariantFacts {
            platform: variant.platform,
            bytes: variant.bytes,
            languages,
            chinese: chinese
                .get(&variant.key)
                .map(|marks| marks.iter().map(|mark| (*mark).to_string()).collect())
                .unwrap_or_default(),
            collections: collections.get(&variant.key).cloned().unwrap_or_default(),
            ..VariantFacts::default()
        };
        for values in [
            work.as_ref().and_then(|name| by_work.get(name)),
            by_variant.get(&variant.key),
        ]
        .into_iter()
        .flatten()
        {
            for (into, value) in values {
                into.absorb(&mut row, value);
            }
        }
        row.work = work;
        row.key = variant.key;
        out.push(row);
    }
    Ok(out)
}

/// 规则用得上的那几个刮削字段，以及它落进事实的哪一格。
///
/// **一处定死。** 攒的时候按它筛（真库上 78,902 条刮削值里 65,630 条是标题，
/// 全塞进内存纯属浪费），收进事实的时候按它分派。两处各写一遍清单的话，
/// 加第四个字段必然漏改一处。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScrapedInto {
    Genre,
    Year,
    Rating,
}

impl ScrapedInto {
    fn of(field: &str) -> Option<Self> {
        if field == Field::Genre.label() {
            Some(Self::Genre)
        } else if field == Field::Year.label() {
            Some(Self::Year)
        } else if field == RATING_FIELD {
            Some(Self::Rating)
        } else {
            None
        }
    }

    /// 把一条刮削值收进事实里。**读不成数的值直接扔掉**——刮来的年份是字符串，
    /// 里面出现 `199X` 这种写法时，与其猜一个不如当它没有（取不到值的算法是定死的）。
    fn absorb(self, facts: &mut VariantFacts, value: &str) {
        match self {
            Self::Genre => facts.genres.push(value.to_string()),
            Self::Year => {
                if let Ok(year) = value.trim().parse::<i32>() {
                    facts.years.push(f64::from(year));
                }
            }
            Self::Rating => {
                if let Some(rating) = rule::parse_rating(value) {
                    facts.ratings.push(rating);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 变体(key: &str, platform: &str, bytes: u64) -> VariantFacts {
        VariantFacts {
            key: key.to_string(),
            platform: Some(platform.to_string()),
            bytes,
            ..VariantFacts::default()
        }
    }

    fn 汉化(mut facts: VariantFacts) -> VariantFacts {
        facts.chinese.push("汉化".to_string());
        facts
    }

    fn 选择集(规则: &[&str]) -> Selection {
        Selection {
            rules: 规则
                .iter()
                .map(|text| Rule::parse(text).expect("规则读得懂"))
                .collect(),
            exceptions: Vec::new(),
        }
    }

    #[test]
    fn 规则按平台与中文身份挑得出来() {
        let facts = vec![
            汉化(变体("GB/口袋妖怪.zip", "GB", 1024)),
            变体("GB/日版.zip", "GB", 2048),
            汉化(变体("PSV/大作.vpk", "PSV", 4096)),
        ];
        let out = select(&选择集(&["平台=GB 且 中文=汉化"]), &facts);
        assert_eq!(out.picked.len(), 1);
        assert_eq!(out.picked[0].key, "GB/口袋妖怪.zip");
        assert_eq!(out.bytes, 1024);
        assert_eq!(out.rule_hits, vec![1]);
    }

    #[test]
    fn 多条规则之间是并集() {
        let facts = vec![
            汉化(变体("GB/一.zip", "GB", 1)),
            变体("SFC/二.zip", "SFC", 2),
            变体("PSV/三.vpk", "PSV", 4),
        ];
        let out = select(&选择集(&["平台=GB 且 中文=汉化", "平台=SFC"]), &facts);
        assert_eq!(out.picked.len(), 2);
        assert_eq!(out.bytes, 3);
        assert_eq!(out.rule_hits, vec![1, 1]);
    }

    #[test]
    fn 主库新增符合规则的内容下次自动进入() {
        // 规则存的是写法不是结果：同一条规则、同一个选择集，库长大了就多选出来。
        let selection = 选择集(&["平台=GB 且 中文=汉化"]);
        let mut facts = vec![汉化(变体("GB/一.zip", "GB", 1024))];
        assert_eq!(select(&selection, &facts).picked.len(), 1);
        facts.push(汉化(变体("GB/新来的.zip", "GB", 2048)));
        let out = select(&selection, &facts);
        assert_eq!(out.picked.len(), 2, "新增的自动落进选择集");
        assert_eq!(out.bytes, 3072);
    }

    #[test]
    fn 例外优先于规则() {
        let facts = vec![
            汉化(变体("GB/规则选中的.zip", "GB", 1)),
            变体("PSV/规则没选的.vpk", "PSV", 2),
        ];
        let mut selection = 选择集(&["平台=GB 且 中文=汉化"]);
        selection.exceptions = vec![
            ExceptionRow {
                variant_key: "GB/规则选中的.zip".to_string(),
                kind: Exception::Exclude,
                note: Some("太占地方".to_string()),
                at: 0,
            },
            ExceptionRow {
                variant_key: "PSV/规则没选的.vpk".to_string(),
                kind: Exception::Include,
                note: None,
                at: 0,
            },
        ];
        let out = select(&selection, &facts);
        assert_eq!(out.picked.len(), 1);
        assert_eq!(out.picked[0].key, "PSV/规则没选的.vpk");
        assert_eq!(out.picked[0].why, Why::Exception);
        assert_eq!(out.forced_out, 1);
        assert_eq!(out.forced_out_effective, 1, "规则本来会选中它");
        assert_eq!(out.forced_in, 1);
        assert_eq!(out.forced_in_redundant, 0);
        // 规则自己那一栏不受例外影响：用户核对的是「我这条规则写对了吗」。
        assert_eq!(out.rule_hits, vec![1]);
    }

    #[test]
    fn 例外不随规则重算被覆盖() {
        // 规则整个换掉，例外一条不动——它是沉淀（ADR-0016）。
        let facts = vec![汉化(变体("GB/一.zip", "GB", 1))];
        let exceptions = vec![ExceptionRow {
            variant_key: "GB/一.zip".to_string(),
            kind: Exception::Exclude,
            note: None,
            at: 0,
        }];
        for 规则 in [
            vec!["平台=GB"],
            vec!["中文=汉化"],
            vec!["平台=GB", "体积>=0"],
        ] {
            let mut selection = 选择集(&规则);
            selection.exceptions.clone_from(&exceptions);
            assert!(select(&selection, &facts).picked.is_empty(), "{规则:?}");
        }
    }

    #[test]
    fn 指向库里没有的变体的例外照旧记着() {
        let facts = vec![变体("GB/在的.zip", "GB", 1)];
        let mut selection = 选择集(&["平台=GB"]);
        selection.exceptions = vec![ExceptionRow {
            variant_key: "GB/盘没插时看不到的.zip".to_string(),
            kind: Exception::Include,
            note: None,
            at: 0,
        }];
        let out = select(&selection, &facts);
        assert_eq!(out.missing_exceptions, vec!["GB/盘没插时看不到的.zip"]);
        assert_eq!(out.picked.len(), 1);
    }

    #[test]
    fn 取不到值时只有不等号成立() {
        let facts = vec![VariantFacts {
            key: "散装/没识别出来的.bin".to_string(),
            bytes: 100,
            ..VariantFacts::default()
        }];
        assert!(select(&选择集(&["平台=GB"]), &facts).picked.is_empty());
        assert!(select(&选择集(&["年份>=1990"]), &facts).picked.is_empty());
        assert_eq!(select(&选择集(&["平台!=GB"]), &facts).picked.len(), 1);
        assert_eq!(select(&选择集(&["年份!=1990"]), &facts).picked.len(), 1);
    }

    #[test]
    fn 规则引到没有数据的维度会被点名() {
        let facts = vec![汉化(变体("GB/一.zip", "GB", 1))];
        let out = select(&选择集(&["平台=GB 且 评分>=0.8"]), &facts);
        assert!(out.picked.is_empty());
        assert_eq!(out.thin_dimensions, vec!["评分"], "选不出来是因为缺数据");
    }

    #[test]
    fn 体积与年份按数比() {
        let mut 大 = 变体("PSV/大.vpk", "PSV", 3 * 1024 * 1024 * 1024);
        大.years.push(2015.0);
        let mut 小 = 变体("GB/小.zip", "GB", 512 * 1024);
        小.years.push(1998.0);
        let facts = vec![大, 小];
        assert_eq!(select(&选择集(&["体积<=64MiB"]), &facts).picked.len(), 1);
        assert_eq!(select(&选择集(&["年份>=2000"]), &facts).picked.len(), 1);
        assert_eq!(
            select(&选择集(&["年份<2000 且 体积<1GiB"]), &facts)
                .picked
                .len(),
            1
        );
    }

    #[test]
    fn 作品名取子串() {
        let mut facts = 变体("GBA/火焰纹章 烈火之剑.zip", "GBA", 1);
        facts.work = Some("火焰纹章 烈火之剑".to_string());
        let facts = vec![facts];
        assert_eq!(select(&选择集(&["作品~火焰纹章"]), &facts).picked.len(), 1);
        assert!(
            select(&选择集(&["作品~勇者斗恶龙"]), &facts)
                .picked
                .is_empty()
        );
    }
}
