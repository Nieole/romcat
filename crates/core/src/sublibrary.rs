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
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::catalog::{Catalog, CatalogError};
use crate::scrape::{AnchorKind, Field};
use crate::task::{Cutoff, Handle};

pub use rule::{Bound, Clause, Dimension, Group, Join, Node, Op, Rule, RuleError};

/// **评分**在 `scrape_value` 里的字段名。
///
/// 它**不在 [`Field`] 里**：眼下没有任何源产得出评分（挂账 D68）。这个常量在这里，
/// 是为了让规则那一维与将来真的落库的那个字段名对得上，而不必现在就在 `Field` 上
/// 立一列谁也填不满的枚举值。
pub(crate) const RATING_FIELD: &str = "评分";

/// 一个**子库**的定义。
///
/// 目标路径、前端格式与**能力档案**的名字都在这里。档案本身不在：它是一份可以整份换掉
/// 的数据（[`capability::Roster`](crate::capability::Roster)），子库只记着自己挑的是哪一份
/// 的名字——换一份名册、改一条矩阵，子库一个字都不用动。
///
/// ## 目标路径存两份，键与读盘各用各的
///
/// ADR-0020 的红线：**读盘用系统给的原始形式，入库与比较用 NFC**，两者不能混用。
/// [`Self::target`] 是 NFC 的那一份（当键、进报告），[`Self::target_raw`] 是系统交出来
/// 的原始那一份（[`Self::read_path`] 拿它去开目录）。只存 NFC 一份的话，目标目录名
/// 是分解形式、又挂在**分解敏感**的文件系统上时，`canonicalize` 会失败然后被报成
/// 「目标不在位」——而那正是最不该说的那个谎（挂账 D82）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sublibrary {
    /// 子库叫什么。命令行拿它指名道姓，也是中立库里的主键。
    pub name: String,
    /// 目标设备上的子库根，**NFC 形式**（ADR-0020）。当键用、进报告用。
    ///
    /// 一律走读卡器（ADR-0015）：SD 卡挂成普通盘，于是这就是本机上的一个绝对路径。
    /// 卡不在位时它照样存着——子库是持久实体，不是「插上卡才存在的东西」。
    pub target: String,
    /// 同一个目标根，**系统给的原始形式**。读盘走它（[`Self::read_path`]）。
    ///
    /// 路径不是有效 UTF-8 时是 `None`——那种名字存不成文本，只能退回 NFC 那一份。
    /// 票 18 之前建的子库这一列也是 `None`（那时还没有这一列），行为与从前一致。
    pub target_raw: Option<String>,
    /// 前端格式（适配器名）。
    pub format: String,
    /// 容量上限，字节；`None` 表示不设限。
    ///
    /// **超限绝不自动截断**（ADR-0016）：报出超出量与按体积排序的裁剪建议，砍谁由用户定。
    /// 自动截断的结果不可预测——同一套规则在两张不同容量的卡上会选出完全不同的东西，
    /// 而用户无从得知它砍掉了什么。
    pub capacity: Option<u64>,
    /// **能力档案**的名字：这台设备吃得下什么、这张卡放得下什么（票 21、ADR-0017）。
    ///
    /// `None` 是「没挑过」，走[不作声称](crate::capability::DEFAULT_PROFILE)那一份
    /// ——不转换、不检查，与票 20 的行为一个字不差。票 21 之前建的子库也是 `None`。
    /// **默认必须是「不作声称」而不是某份真的矩阵**：一份没人挑过的矩阵替用户做了
    /// 决定，而它可能是错的（ADR-0017：矩阵错误比不转换更糟）。
    pub capability: Option<String>,
}

impl Sublibrary {
    /// 从一条**系统给的**目标路径造一个子库：两种形式一次填齐。
    #[must_use]
    pub fn at(name: &str, target: &Path, format: &str, capacity: Option<u64>) -> Self {
        Self {
            name: name.to_string(),
            target: crate::path::nfc(&crate::path::display(target)).into_owned(),
            target_raw: target.to_str().map(ToString::to_string),
            format: format.to_string(),
            capacity,
            capability: None,
        }
    }

    /// **读盘该用的那条路径**（ADR-0020）。
    ///
    /// 原始形式在就用原始形式；不在（老库、或者路径不是 UTF-8）才退回 NFC 那一份。
    #[must_use]
    pub fn read_path(&self) -> PathBuf {
        PathBuf::from(self.target_raw.as_ref().unwrap_or(&self.target))
    }
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
    ///
    /// 只有[读得成年份的值](rule::parse_year)进得来。读不成的（`199X`、`一九九六`、
    /// `+1996`）不在这儿，于是那个变体在这一维上就是取不到值。
    pub years: Vec<f64>,
    /// 刮削来的评分，0–1。眼下没有源（见 `RATING_FIELD`）。
    pub ratings: Vec<f64>,
    /// 刮削来的开发商。
    pub developers: Vec<String>,
    /// 刮削来的发行商。
    pub publishers: Vec<String>,
    /// 刮削来的简介。
    pub descriptions: Vec<String>,
    /// 在不在**收藏**里。
    ///
    /// **收藏就是名字定死的那个合集**（[`collection::FAVORITE`](crate::collection::FAVORITE)）：
    /// 这个布尔与 [`Self::collections`] 里有没有那个名字**永远是同一件事**，
    /// [`facts`] 从同一份成员关系里折出这两样。它单独立一个字段而不是让人自己去
    /// `collections` 里找，是因为规则语言把它立成了独立的一维（`收藏=是`），
    /// 而那一维在 SQL 那一侧也是单独一条谓词。
    pub favorite: bool,
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
            Dimension::Developer => self.developers.iter().any(|value| hit(value)),
            Dimension::Publisher => self.publishers.iter().any(|value| hit(value)),
            Dimension::Description => self.descriptions.iter().any(|value| hit(value)),
            // **收藏是个是非题**：两档都是值，没有「取不到」这一说。于是
            // `收藏=否` 对一个没收藏的变体成立，而不是像缺数据那样一律不成立。
            Dimension::Favorite => hit(if self.favorite { "是" } else { "否" }),
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
        match dimension {
            // **收藏这一维「有数据」指的是真收藏了**。两档都算有值的话，一份一条收藏
            // 都没有的库上，`收藏=是` 选不出东西这件事就没人说得出是为什么——
            // 而报告的正题恰恰是「说清是缺数据还是写错了」。
            Dimension::Favorite => self.favorite,
            _ if dimension.is_number() => self.any_number(dimension, |_| true),
            _ => self.any_text(dimension, |_| true),
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

/// 「**扔掉一条读不懂的规则**」那一下的结果。
///
/// 三种结局要分得开：界面上那句回执写的正是它，而**「读得懂」这一种是一道闸**
/// ——见 [`Catalog::discard_broken_rule`](crate::catalog::Catalog::discard_broken_rule)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Discarded {
    /// 扔掉了。
    Gone,
    /// 库里已经没有这个序号了（别处先删过一遍）。
    Absent,
    /// 这一条**读得懂**——这条路删不掉它。
    Readable,
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

/// 超容量时列几个最大的当裁剪建议。
///
/// 十个是**看得完**与**够用**之间的折中：真库上一条规则能选出上千个变体，全列出来
/// 谁也读不完；而 [`Trim::cumulative`] 让「这十个全砍掉够不够」一眼看得出来，
/// 不够时报告直接说还差多少。
pub const TRIM_SUGGESTIONS: usize = 10;

/// 一条**裁剪建议**：砍掉这个变体能腾出多少。
///
/// **落在变体这一层而不是文件**：例外记的是变体的键
/// （`romcat sublibrary except --exclude <变体的键>`），落在文件上的建议照着做不了。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Trim {
    /// 变体的键。
    pub variant: String,
    /// 砍掉它腾出多少字节。
    pub bytes: u64,
    /// 从最大的那个砍起、砍到这一条为止一共腾出多少。
    ///
    /// **「砍到第几个才够」直接读得出来**——没有它，用户对着十行数字还得自己加一遍，
    /// 而超出量动辄是几十上百 GiB。
    pub cumulative: u64,
}

/// 超出容量上限多少字节；没超或没设上限时是 `None`。
#[must_use]
pub fn over_capacity(capacity: Option<u64>, bytes: u64) -> Option<u64> {
    capacity
        .and_then(|limit| bytes.checked_sub(limit))
        .filter(|over| *over > 0)
}

/// **容量条**那三段：**选中的**、**清单之外的**、**上限**。
///
/// 子库屏一台设备一张卡，卡上那根条子画的就是它（票 `gui-redesign/11`）。
/// 三个数各有各的出处，而且**出处不同这件事本身要说得出口**：
///
/// - **选中**——这个子库在卡上占的地方。**卡不在手边时**是选择集选出来的那批变体一共
///   多大（只问中立库）；**排过差量预览之后**换成计划里那个数，因为那时算得准了
///   （元数据与媒体也要占地方、转换又省下来一些）。
/// - **清单之外**——目标上工具没放过的那些文件一共多大（[`Plan::stranger_bytes`]）。
///   它要**目标设备在位**才知道，所以没排过差量预览时是 `None`——那是「还不知道」，
///   不是「一个字节都没有」。两者在卡上画成同一个 0 的话，人会以为卡上是空的。
/// - **上限**——子库自己记着的容量上限。`None` 是不设限。
///
/// **这里不算「超没超」。** 超出量与裁剪建议由 [`over_capacity`] 与
/// [`trim_suggestions`] 一处算（ADR-0016），报告与计划各自摆的都是那一份；
/// 条子再算一遍就会出现「条子说满了、旁边那行字说没超」这种对不上的账。
///
/// **不算，也就必须对得上**，而对得上是一条能写下来的规矩：填这三个数的人要保证
/// `over_capacity(capacity, taken())` 等于旁边那行字用的那个超出量。
/// 排过差量的那一台照 [`Plan::over_capacity`] 那条口径填
/// （`选中 = after_bytes − stranger_bytes`，于是 [`Self::taken`] 正好是 `after_bytes`）；
/// 没排过的照 [`SelectionReport::over_capacity`] 那条填（`选中 = report.bytes`、
/// 清单之外是 `None`，于是 `taken()` 正好是 `report.bytes`）。两条都是恒等式，
/// 不是「差不多」。
///
/// [`Plan::stranger_bytes`]: crate::sync::Plan::stranger_bytes
/// [`Plan::over_capacity`]: crate::sync::Plan::over_capacity
/// [`SelectionReport::over_capacity`]: report::SelectionReport::over_capacity
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Gauge {
    /// 选择集选中的那批变体一共多大。
    pub picked: u64,
    /// 目标上**清单之外**的文件一共多大；`None` 是还没排过差量预览，不知道。
    pub strangers: Option<u64>,
    /// 容量上限；`None` 是不设限。
    pub capacity: Option<u64>,
}

impl Gauge {
    /// 条子画满对应多少字节。
    ///
    /// 有上限就是上限；**超了上限就照实际占用画满**，不然超出去的那一截无处可画，
    /// 一根画满的条子会同时表示「正好装满」与「超了三倍」。没上限时就照占用本身画，
    /// 于是两段的比例仍然看得出谁大谁小。
    #[must_use]
    pub fn scale(&self) -> u64 {
        self.taken().max(self.capacity.unwrap_or(0))
    }

    /// 卡上一共占掉多少：选中的加上清单之外的。
    #[must_use]
    pub fn taken(&self) -> u64 {
        self.picked.saturating_add(self.strangers.unwrap_or(0))
    }

    /// **选中**那一段占条子的几成，0.0–1.0。
    #[must_use]
    pub fn picked_share(&self) -> f32 {
        share(self.picked, self.scale())
    }

    /// **清单之外**那一段占条子的几成，0.0–1.0。
    #[must_use]
    pub fn stranger_share(&self) -> f32 {
        share(self.strangers.unwrap_or(0), self.scale())
    }
}

/// 一段占整条的几成。分母是 0 时是 0——画一根空条子，不是除以零。
fn share(part: u64, whole: u64) -> f32 {
    if whole == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let value = part as f32 / whole as f32;
    value.clamp(0.0, 1.0)
}

/// 按体积排序的**裁剪建议**：最大的那几个变体。
///
/// **只建议，绝不自动截断**（ADR-0016）——同一套规则在两张不同容量的卡上会选出
/// 完全不同的东西，而用户无从得知它砍掉了什么。
#[must_use]
pub fn trim_suggestions(by_variant: impl IntoIterator<Item = (String, u64)>) -> Vec<Trim> {
    let mut biggest: Vec<(String, u64)> = by_variant.into_iter().collect();
    // 体积降序；同体积按键排，同一份输入跑两次结果必须一样。
    biggest.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    biggest.truncate(TRIM_SUGGESTIONS);
    let mut cumulative = 0;
    biggest
        .into_iter()
        .map(|(variant, bytes)| {
            cumulative += bytes;
            Trim {
                variant,
                bytes,
                cumulative,
            }
        })
        .collect()
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
        for condition in rule.clauses() {
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

/// 这条规则选中这个变体吗——从顶层那个组算起。
fn matches(rule: &Rule, facts: &VariantFacts) -> bool {
    holds(&rule.root, facts)
}

/// 这个组成立吗。三种连接各算各的，**空组按各自的中性元算**：
/// 「全部满足」与「都不满足」空着成立（没有一项不成立），「任一满足」空着不成立。
fn holds(group: &Group, facts: &VariantFacts) -> bool {
    let mut each = group.nodes.iter().map(|node| match node {
        Node::Clause(clause) => satisfies(clause, facts),
        Node::Group(inner) => holds(inner, facts),
    });
    match group.join {
        Join::All => each.all(|ok| ok),
        Join::Any => each.any(|ok| ok),
        Join::None => !each.any(|ok| ok),
    }
}

/// 这个子句成立吗。
///
/// **取不到值时 `!=` 成立、别的一律不成立**（见 [`rule`] 的模块文档）：那一条约定
/// 在这里就是「先算 `any`，再由 `!=` 把它翻过来」——空集合上 `any` 是假，翻过来是真。
fn satisfies(condition: &Clause, facts: &VariantFacts) -> bool {
    let any = match &condition.bound {
        Bound::Text(wanted) => facts.any_text(condition.dimension, |value| match condition.op {
            Op::Contains => wanted.iter().any(|want| contains_ignore_case(value, want)),
            Op::StartsWith => wanted
                .iter()
                .any(|want| starts_with_ignore_case(value, want)),
            Op::EndsWith => wanted.iter().any(|want| ends_with_ignore_case(value, want)),
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
///
/// **不看两头是不是纯 ASCII**：`口袋RED` 里含不含 `red`，答案不该取决于这个作品名
/// 里还有没有汉字。从前那个写法（非纯 ASCII 就退成逐字节比）与 SQL 那一侧
/// （`lower()` 只折 ASCII，混着汉字照折）在中英混排的名字上会给出两个答案，
/// 而那种名字这个库里到处都是。
fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

/// 忽略 ASCII 大小写的**开头**判断（`^ 以…开始`）。
fn starts_with_ignore_case(haystack: &str, needle: &str) -> bool {
    haystack
        .get(..needle.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(needle))
}

/// 忽略 ASCII 大小写的**结尾**判断（`$ 以…结束`）。
fn ends_with_ignore_case(haystack: &str, needle: &str) -> bool {
    haystack
        .len()
        .checked_sub(needle.len())
        .and_then(|at| haystack.get(at..))
        .is_some_and(|tail| tail.eq_ignore_ascii_case(needle))
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
            .map(crate::catalog::ReleaseRow::language_codes)
            .unwrap_or_default();
        let joined = collections.get(&variant.key).cloned().unwrap_or_default();
        let mut row = VariantFacts {
            platform: variant.platform,
            bytes: variant.bytes,
            languages,
            chinese: chinese
                .get(&variant.key)
                .map(|marks| marks.iter().map(|mark| (*mark).to_string()).collect())
                .unwrap_or_default(),
            // **收藏就是那个名字定死的合集**：两样从同一份成员关系里折出来，
            // 不给它们留下各说各的余地（`catalog::filter` 那一侧也是同一条判据）。
            favorite: joined
                .iter()
                .any(|name| name == crate::collection::FAVORITE),
            collections: joined,
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

/// **算一遍容量**：每台设备各求一次选择集，折出各自的[选择集报告](report::SelectionReport)。
///
/// ## 折一趟事实，全部设备共用
///
/// 大头是 [`facts`] 走一遍全库（真机量级上 343 毫秒，挂账 D156），而按选择集求值是
/// 内存里的事。一台一折的话，五张卡就是五趟全库。
///
/// ## 不碰目标设备
///
/// 「这套规则选出多少、装不装得下」只要中立库——卡不在手边也算得出来（ADR-0009）。
/// 要目标设备在位的是**差量预览**（[`sync::prepare`](fn@crate::sync::prepare)），那是另一趟活。
///
/// `task` 是这一趟的**把手**：折事实一步，此后一台设备一步。**整条只读**，
/// 所以被叫停时停在哪儿都是干净的——一个字节都没写，再算一次就是。
///
/// # Errors
/// 中立库读不动时返回 [`Cutoff::Failed`]；被叫停时返回
/// [`Cutoff::Halted`]——**那是两个不同的支，不是两句
/// 不同的话**，任务台按它分「停了」与「失败」。
pub fn survey(
    catalog: &Catalog,
    sublibraries: &[Sublibrary],
    task: &Handle,
) -> Result<BTreeMap<String, report::SelectionReport>, Cutoff> {
    // 折事实那一步 + 一台设备一步。
    task.steps(u32::try_from(sublibraries.len() + 1).unwrap_or(u32::MAX));
    task.step("折事实")?;
    let facts = facts(catalog).map_err(|error| format!("中立库读不动：{error}"))?;
    let mut out = BTreeMap::new();
    for sublibrary in sublibraries {
        task.step(&format!("算「{}」", sublibrary.name))?;
        let loaded = catalog
            .selection(&sublibrary.name)
            .map_err(|error| format!("中立库读不动：{error}"))?;
        let selected = select(&loaded.selection, &facts);
        out.insert(
            sublibrary.name.clone(),
            report::SelectionReport::build(
                catalog.location(),
                sublibrary,
                &loaded,
                &facts,
                &selected,
            ),
        );
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
    Developer,
    Publisher,
    Description,
}

impl ScrapedInto {
    fn of(field: &str) -> Option<Self> {
        if field == Field::Genre.label() {
            Some(Self::Genre)
        } else if field == Field::Year.label() {
            Some(Self::Year)
        } else if field == RATING_FIELD {
            Some(Self::Rating)
        } else if field == Field::Developer.label() {
            Some(Self::Developer)
        } else if field == Field::Publisher.label() {
            Some(Self::Publisher)
        } else if field == Field::Description.label() {
            Some(Self::Description)
        } else {
            None
        }
    }

    /// 把一条刮削值收进事实里。**读不成数的值直接扔掉**——刮来的年份是字符串，
    /// 里面出现 `199X` 这种写法时，与其猜一个不如当它没有（取不到值的算法是定死的）。
    ///
    /// 年份与评分各自读法只有**一份定义**（[`rule::parse_year`] / [`rule::parse_rating`]），
    /// SQL 那一侧（`catalog::filter`）逐字照它写。这里从前是 `parse::<i32>()`，
    /// 它收下前导 `0` 与前导 `+` 而 SQL 那句不收，于是 `01996` 在屏上不算年份、
    /// 按下同步却算——**筛出来的那批与搬过去的那批不是同一批**。
    fn absorb(self, facts: &mut VariantFacts, value: &str) {
        match self {
            Self::Genre => facts.genres.push(value.to_string()),
            Self::Year => {
                if let Some(year) = rule::parse_year(value) {
                    facts.years.push(f64::from(year));
                }
            }
            Self::Rating => {
                if let Some(rating) = rule::parse_rating(value) {
                    facts.ratings.push(rating);
                }
            }
            Self::Developer => facts.developers.push(value.to_string()),
            Self::Publisher => facts.publishers.push(value.to_string()),
            Self::Description => facts.descriptions.push(value.to_string()),
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
