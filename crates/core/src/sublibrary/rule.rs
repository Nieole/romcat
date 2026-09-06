//! **规则**：选择集里可重放的那一半，**也就是浏览屏上那个筛选器**。
//!
//! 同一套语言两处用：临时筛一批是它，把那一批存成子库还是它。两处若各写一套，
//! 「筛到满意按存成子库」这个动作就要在中间翻译一道，而翻译得不准时用户看见的那批
//! 与真正导出去的那批对不上——那正是这份规格要消灭的东西。
//!
//! ## 一条规则是一棵**组**
//!
//! 每个组挑一种**连接**：`全部满足` / `任一满足` / `都不满足`；组里摆的既可以是
//! **子句**，也可以是另一个组。于是它嵌得下去：
//!
//! ```text
//! 平台=GB,GBA 且 (作品^口袋 或 类型~RPG) 且 收藏=是
//! ```
//!
//! 写法三条，就这么多：
//!
//! | 连接 | 怎么写 | 意思 |
//! |---|---|---|
//! | 全部满足 | `A 且 B` | 都得成立 |
//! | 任一满足 | `A 或 B` | 有一个成立就算 |
//! | 都不满足 | `都不(A 或 B)` | 一个都不成立 |
//!
//! - **连接词两侧要有空白**（`且` `或` 都一样），于是值里出现这个字也不会被当成分隔符
//!   ——「一将功成万骨枯」「生存或毁灭」都是能进库的作品名。
//! - **同一层里不许混用** `且` 与 `或`：混着写就得有优先级，而一条规则里看不见的优先级
//!   是「用户以为选中了什么」与「实际选中了什么」分家的起点。要混就套括号，
//!   括号在纸面上就是那个组。
//! - `都不(…)` 里通常写 ` 或 `：「这几样一个都别沾」。写成 `都不((A 且 B))` 时否掉的是
//!   那一整个「全部满足」组——「A 与 B 不同时成立」，两种意思都写得出来。
//!
//! **一条平铺的老规则原样读得懂**：`平台=GB 且 中文=汉化` 里没有括号也没有 `或`，
//! 它就是一个顶层的「全部满足」组。这一票之前存进中立库的子库规则一条都没有失效。
//!
//! 一个选择集里可以有多条规则，**多条之间是并集**——各选各的，合起来就是规则那一半
//! 选中的全部。
//!
//! ## 括号只在**项的开头**才是括号
//!
//! 作品名里带括号是常态（`Pocket Monsters (Japan)`、`口袋妖怪(日`）。所以一个 `(` 只有
//! 出现在**一项的开头**（前面那一段去掉空白之后是空的，或者正好是 `都不`）才开一个组;
//! 别处的括号是值里的字，配不配对都不影响这一层怎么切。
//!
//! ## 读得进来的比写得出去的宽，这是故意的
//!
//! [`Clause::parse`] 读的是**库里已经躺着的**规则：这一票之前存进去的那些，值里带一个
//! 落单的括号也照样读得懂，一条都不许因为这一票失效。
//!
//! [`Clause::build`] 造的是**一条新的**（界面上点出来的、或者从筛选器折出来的），
//! 于是它多两道闸：**值里不许有会被读成分隔符的连接词**，**括号得配对**。理由是它造出来的
//! 东西马上要被 [`Rule::from_group`] 印成一行字存进中立库——`作品~RPG 且 平台=GB`
//! 打在一个值格子里，印出来那行字下次读回来是**两个**子句，而屏上筛的是一个。
//! 那种走样是静默的，所以在造它的那一步就挡住。
//!
//! **「会被读成分隔符」比「两侧带空白」宽一点，宽的是末尾那一头**：值印出去时，
//! 后面紧跟的可能就是 `" 且 "` / `" 或 "` 这个分隔符，**它把值末尾那个连接词缺的空白
//! 补上了**。于是 `作品~RPG 且` 这个值后面一接兄弟子句就成了
//! `作品~RPG 且 且 平台=GB`：读回来值被截成 `RPG`，选出来的比屏上多，**一声不响**；
//! 后面接的是 ` 或 ` 时整条落成 `MixedJoin`，那个子库一条都选不中。值**开头**那个连接词
//! 反过来不用管——值左边紧挨着的永远是运算符（`作品~` 那个 `~`），绝不是空白，
//! 所以 `作品~且 RPG`、甚至值恰好就是一个 `且` 字，印出去都读得回来，照收。
//!
//! **闸只有这一道，印那一侧不设第二道。** [`Rule::from_group`] 拿到的树未必都出自
//! `build`：[`Rule::any_of`] 印的就是从中立库里 [`Rule::parse`] 读出来的老规则，
//! 而读那一侧是**故意宽的**（下一节）。在印的时候再收一次紧，等于让那些老规则印不出来
//! ——那正是「一条都不许失效」要挡的事。所以分工是：**造新的那一步挡住走样**
//! （错误还能指回用户刚打字的那个格子），**印那一步只管照实印**。
//!
//! ## 为什么是文本而不是一棵存进库的树
//!
//! 规则要**存得住、看得见、改得动**：它落在中立库里跨进程活着，也要能原样印进报告让
//! 用户核对自己写的是什么（ADR-0016 的「可重放」说的正是这件事）。一列文本存得下一棵树，
//! 而把树摊成表要么每加一层就多一张表，要么塞成一团 JSON——后者用户看不见自己写了什么。
//! 界面上搭出来的那棵树同样落回这一行文本（[`Rule::from_group`]），于是**界面存的与
//! 命令行写的是同一个东西**。
//!
//! ## 值取不到时怎么算
//!
//! 一条**明确的**约定，全部维度一视同仁：**取不到值时，`!=` 成立，别的一律不成立。**
//!
//! 年份没刮到的变体，`年份>=1990` 不匹配（不知道就是不知道，不能当成匹配放进去），
//! 而 `年份!=1990` 匹配（它确实不是 1990 年）。这条约定必须写死并说出口——留给直觉
//! 的话，同一条规则在两个人脑子里会算出两个结果。
//!
//! `都不满足` 落在这条约定之上而不是绕过它：`都不(年份>=1990)` 对一个没刮到年份的变体
//! **成立**，因为里面那句本来就不成立。
//!
//! **读不成的值与没有值是同一档**：刮来的年份是一列文本，里面会出现 `199X`、`一九九六`、
//! 人手打进裁决的 `+1996`。这些一律不是年份（[`parse_year`]），于是那个变体在年份这一维上
//! 就是取不到值，上面那条约定原样管用。

use std::fmt;

/// 「全部满足」写成的那个连接词。**要求两侧有空白**，于是值里出现这个字也不会被当成分隔符。
const AND: char = '且';

/// 「任一满足」写成的那个连接词。同样**要求两侧有空白**。
const OR: char = '或';

/// 「都不满足」写在组前面的那两个字，后面紧跟一对括号。
const NOT: &str = "都不";

/// 一个组里那几项**怎么连起来**。
///
/// 三种，与界面上那三个选项逐字对应——[`Self::label`] 印的就是屏上那个词。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Join {
    /// **全部满足**：里面每一项都得成立。空组算成立。
    All,
    /// **任一满足**：有一项成立就算。空组不成立。
    Any,
    /// **都不满足**：一项都不许成立。空组算成立。
    None,
}

impl Join {
    /// 界面上那个词，**也就是词表里的那个词**。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "全部满足",
            Self::Any => "任一满足",
            Self::None => "都不满足",
        }
    }

    /// 界面照这个次序摆那三个选项。
    pub const ALL: [Self; 3] = [Self::All, Self::Any, Self::None];
}

/// 一个**组**：一种连接，加上里面那几项。
///
/// 一条规则的顶层就是一个组（[`Rule::root`]），组里可以再套组——嵌多深都行。
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// 里面那几项怎么连。
    pub join: Join,
    /// 里面那几项，**按写的次序**。
    pub nodes: Vec<Node>,
}

impl Group {
    /// 造一个组。
    #[must_use]
    pub fn new(join: Join, nodes: Vec<Node>) -> Self {
        Self { join, nodes }
    }

    /// 这棵树上的全部子句，**按出现次序**（深度优先）。
    ///
    /// 报告拿它数「这条规则引到了哪几维」——引到一维却一条数据都没有时要点名说清，
    /// 而那件事与组怎么嵌套无关。
    #[must_use]
    pub fn clauses(&self) -> Vec<&Clause> {
        let mut out = Vec::new();
        self.walk(&mut out);
        out
    }

    fn walk<'a>(&'a self, out: &mut Vec<&'a Clause>) {
        for node in &self.nodes {
            match node {
                Node::Clause(clause) => out.push(clause),
                Node::Group(group) => group.walk(out),
            }
        }
    }

    /// 嵌了几层。顶层是 1。**界面画缩进靠它**。
    #[must_use]
    pub fn depth(&self) -> usize {
        1 + self
            .nodes
            .iter()
            .filter_map(|node| match node {
                Node::Group(group) => Some(group.depth()),
                Node::Clause(_) => None,
            })
            .max()
            .unwrap_or(0)
    }
}

/// 组里的一项：一个**子句**，或者又一个**组**。
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// 一个子句。
    Clause(Clause),
    /// 又一个组。
    Group(Group),
}

/// 规则里能筛的**维度**。
///
/// 每个维度对应中立库里的一样事实（见 [`VariantFacts`](super::VariantFacts)）。
///
/// **`评分` 眼下没有源**：`scrape::Field` 里根本没有这个字段，一条值也产不出来
/// （挂账 D68）。
///
/// 维度可以先于源立起来，因为等值落进库那天，规则不必改一个字就生效；而在那之前
/// [`选择集报告`](super::report::SelectionReport) 会点名说「这条规则引到的这一维
/// 在这份库里一条数据都没有」——**选不出东西时说清是缺数据还是写错了**，比让用户
/// 对着 0 猜强。`收藏` 与 `合集` 走过的正是这条路：票 `gui-redesign/04` 立维度、
/// 票 `gui-redesign/06` 把值接上来（[`crate::collection`]），规则一个字都没改。
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
    /// **收藏**：这个变体在不在收藏里。值写 `是` / `否`。
    ///
    /// 收藏就是名字定死的那个合集（[`crate::collection::FAVORITE`]），落**沉淀库**、
    /// 锚在**内容锚**上——所以 `收藏=是` 与 `合集=收藏` 选出来的是同一批。
    Favorite,
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
    /// 刮削来的开发商。
    Developer,
    /// 刮削来的发行商。
    Publisher,
    /// 刮削来的简介。配 `~` 取子串。
    Description,
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
            Self::Favorite => "收藏",
            Self::Genre => "类型",
            Self::Work => "作品",
            Self::Year => "年份",
            Self::Rating => "评分",
            Self::Size => "体积",
            Self::Developer => "开发商",
            Self::Publisher => "发行商",
            Self::Description => "简介",
        }
    }

    /// 一句话说清这个维度筛的是什么，`romcat sublibrary rule` 的帮助里印它。
    #[must_use]
    pub fn hint(self) -> &'static str {
        match self {
            Self::Platform => "变体所属的硬件系统，如 `平台=GB,GBA`",
            Self::Language => "发行版的语言标记，如 `语言=Zh`",
            Self::Chinese => "变体的中文身份：`汉化` 或 `官中`",
            Self::Collection => "用户自定义的合集，如 `合集=我通关过的`（在界面上多选一批建）",
            Self::Favorite => "在不在收藏里，写 `收藏=是`（收藏是名字定死的那个合集）",
            Self::Genre => "刮削来的类型",
            Self::Work => "作品名，配 `~` 取子串，如 `作品~火焰纹章`",
            Self::Year => "刮削来的发行年份，如 `年份>=1990`",
            Self::Rating => "评分，写 0–1 的小数或 `80%`",
            Self::Size => "变体的容量，如 `体积<=64MiB`（是下界，ADR-0021）",
            Self::Developer => "刮削来的开发商，如 `开发商^Game Freak`",
            Self::Publisher => "刮削来的发行商",
            Self::Description => "刮削来的简介，配 `~` 取子串",
        }
    }

    /// 帮助、报告与界面里固定的排列顺序。
    #[must_use]
    pub fn all() -> [Self; 13] {
        [
            Self::Platform,
            Self::Language,
            Self::Chinese,
            Self::Collection,
            Self::Favorite,
            Self::Genre,
            Self::Work,
            Self::Year,
            Self::Rating,
            Self::Size,
            Self::Developer,
            Self::Publisher,
            Self::Description,
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
    /// `^`：以这段文字开始。
    StartsWith,
    /// `$`：以这段文字结束。
    EndsWith,
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
            Self::StartsWith => "^",
            Self::EndsWith => "$",
            Self::Le => "<=",
            Self::Lt => "<",
            Self::Ge => ">=",
            Self::Gt => ">",
        }
    }

    /// 界面上那个下拉框里写的字。符号本身太短，光一个 `^` 看不出是什么。
    #[must_use]
    pub fn hint(self) -> &'static str {
        match self {
            Self::Is => "= 是其中之一",
            Self::IsNot => "!= 一个都不是",
            Self::Contains => "~ 含有",
            Self::StartsWith => "^ 以…开始",
            Self::EndsWith => "$ 以…结束",
            Self::Le => "<= 不大于",
            Self::Lt => "< 小于",
            Self::Ge => ">= 不小于",
            Self::Gt => "> 大于",
        }
    }

    /// 界面照这个次序摆运算符；文字维度取前五个，数值维度取 [`Self::for_number`]。
    pub const ALL: [Self; 9] = [
        Self::Is,
        Self::IsNot,
        Self::Contains,
        Self::StartsWith,
        Self::EndsWith,
        Self::Le,
        Self::Lt,
        Self::Ge,
        Self::Gt,
    ];

    /// 这个维度上摆得出来的那几个运算符。
    #[must_use]
    pub fn for_dimension(dimension: Dimension) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|op| op.fits(dimension))
            .collect()
    }

    /// 认符号的顺序：**长的排在前面**，否则 `<=` 会被 `<` 先咬掉一半。
    fn candidates() -> [Self; 9] {
        [
            Self::IsNot,
            Self::Le,
            Self::Ge,
            Self::Is,
            Self::Contains,
            Self::StartsWith,
            Self::EndsWith,
            Self::Lt,
            Self::Gt,
        ]
    }

    /// 这个符号配得上这个维度吗。
    fn fits(self, dimension: Dimension) -> bool {
        if dimension.is_number() {
            !matches!(self, Self::Contains | Self::StartsWith | Self::EndsWith)
        } else {
            matches!(
                self,
                Self::Is | Self::IsNot | Self::Contains | Self::StartsWith | Self::EndsWith
            )
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
    /// 值那一段的**原文**。印出来的是它，不是折算过的数。
    ///
    /// 少了这一列，`体积<=64MiB` 印回去就成了 `体积<=67108864`——两个数一样，
    /// 但用户在子库规则列表里认不出那是自己写的那一条。
    pub raw: String,
}

/// 规则写得不对。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuleError {
    /// 空规则。
    #[error("规则是空的。写法是 `维度 运算符 值`，多个子句用 ` 且 `（全部满足）或 ` 或 `（任一满足）连起来")]
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
    #[error("子句「{text}」里没有运算符。能用的是 = != ~ ^ $ <= < >= >")]
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
    /// 同一层里既写了 `且` 又写了 `或`。
    #[error("「{text}」这一层里既有 ` 且 ` 又有 ` 或 `。哪个先算说不清，用括号分组：`A 且 (B 或 C)`")]
    MixedJoin {
        /// 那一层的原文。
        text: String,
    },
    /// 括号对不上。
    #[error("「{text}」里的括号对不上。值里的括号要成对写")]
    UnbalancedParen {
        /// 那一段原文。
        text: String,
    },
    /// `都不` 后面没跟一对括号。
    #[error("「{text}」：`都不` 后面要跟一对括号，如 `都不(语言=En 或 中文=汉化)`")]
    LonelyNot {
        /// 那一段原文。
        text: String,
    },
    /// 括号里是空的。
    #[error("「{text}」里的括号是空的")]
    EmptyGroup {
        /// 那一段原文。
        text: String,
    },
    /// 值里有会被读成分隔符的连接词：前面是空白，后面是空白或者正好到值的末尾。
    #[error(
        "{dimension} 的值「{value}」里的 `{joiner}` 会被读成连接词——它前面是空白，\
         后面是空白或者正好到值的末尾（末尾那个空白由分隔符补上）。\
         写进规则读回来会少一截，甚至整条读不回来。\
         去掉 `{joiner}` 前面那个空白，或者本来就该拆成两条子句"
    )]
    JoinerInValue {
        /// 哪个维度。
        dimension: &'static str,
        /// 用户写的那个值。
        value: String,
        /// 撞上的是哪个连接词。
        joiner: char,
    },
}

impl Rule {
    /// 读一条规则。
    ///
    /// # Errors
    /// 维度认不出、运算符配不上、值不是个数、同一层混用连接词、括号对不上时返回错误。
    /// **在写进中立库之前就报出来**：一条存进去才发现读不懂的规则，下次同步时才炸，
    /// 那时用户早忘了自己写了什么。
    pub fn parse(text: &str) -> Result<Self, RuleError> {
        let trimmed = text.trim();
        let root = parse_group(trimmed)?;
        Ok(Self {
            text: trimmed.to_string(),
            root,
        })
    }

    /// 从一棵组树造一条规则：**原文由树印出来**。
    ///
    /// 界面搭的那棵树走这条路落成文本，命令行写的那行文本走 [`Self::parse`] 变成树。
    /// 两头是同一个东西，所以「筛到满意按存成子库」中间没有翻译这一道。
    #[must_use]
    pub fn from_group(root: Group) -> Self {
        Self {
            text: render_group(&root, false),
            root,
        }
    }

    /// 这条规则上的全部子句，**按出现次序**。
    #[must_use]
    pub fn clauses(&self) -> Vec<&Clause> {
        self.root.clauses()
    }

    /// 把几条规则并成一条。
    ///
    /// 一个选择集里**多条规则之间是并集**，而并集就是一个「任一满足」组。子库屏点
    /// 「改选择」跳回浏览屏时（票 `gui-redesign/11`），要预填进筛选器的正是这一条。
    ///
    /// 一条都没有时是 `None`——那是「没有任何条件」，不是「一条都选不中」。
    #[must_use]
    pub fn any_of(rules: impl IntoIterator<Item = Self>) -> Option<Self> {
        let mut rules: Vec<Self> = rules.into_iter().collect();
        match rules.len() {
            0 => None,
            // **一条就是它自己，连原文一并留着**：「任一满足」里只有一项时，套那层组
            // 只会让印出来的原文多一对括号。子库屏的「改选择」是一趟往返
            // （票 `gui-redesign/11`），每往返一次多套一层的话，规则原文会越印越深
            // ——而它正是用户要照着核对的那句话。
            1 => rules.pop(),
            _ => Some(Self::from_group(Group {
                join: Join::Any,
                nodes: rules.into_iter().map(|rule| Node::Group(rule.root)).collect(),
            })),
        }
    }
}

/// 一条规则：一棵**组**树。
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    /// 用户写的原文。报告原样印它——用户核对的是自己写的那句话，不是我们重排的版本。
    pub text: String,
    /// 顶层那个组。一条平铺的老规则在这里就是一个「全部满足」组。
    pub root: Group,
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl fmt::Display for Clause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}{}", self.dimension.label(), self.op.label(), self.raw)
    }
}

impl fmt::Display for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&render_group(self, false))
    }
}

/// 把一个组印回文本。`nested` 是「它套在别的组里」——那时要加括号。
fn render_group(group: &Group, nested: bool) -> String {
    let (separator, wrap) = match group.join {
        Join::All => (" 且 ", nested),
        Join::Any => (" 或 ", nested),
        // `都不` 那个前缀自带一对括号，不看 `nested`。
        Join::None => (" 或 ", false),
    };
    let body = group
        .nodes
        .iter()
        .map(render_node)
        .collect::<Vec<_>>()
        .join(separator);
    match group.join {
        Join::None => format!("{NOT}({body})"),
        _ if wrap => format!("({body})"),
        _ => body,
    }
}

fn render_node(node: &Node) -> String {
    match node {
        Node::Clause(clause) => clause.to_string(),
        Node::Group(group) => render_group(group, true),
    }
}

/// 读一层：切成项，认出这一层的连接词，再各自读下去。
///
/// **一项且只有一项、而那一项是个组**时直接把那个组交出去——`(A 且 B)` 与 `A 且 B`
/// 是同一棵树，否则每套一层括号就白多一层，印回去还会越印越深。
fn parse_group(text: &str) -> Result<Group, RuleError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(RuleError::Empty);
    }
    let (join, pieces) = split_items(text)?;
    if pieces.is_empty() {
        return Err(RuleError::Empty);
    }
    let mut nodes = Vec::with_capacity(pieces.len());
    for piece in pieces {
        nodes.push(parse_item(piece)?);
    }
    if nodes.len() == 1
        && let Node::Group(inner) = &nodes[0]
    {
        return Ok(inner.clone());
    }
    Ok(Group {
        join: join.unwrap_or(Join::All),
        nodes,
    })
}

/// 读一项：一个组、一个 `都不(…)`、或者一个子句。
fn parse_item(text: &str) -> Result<Node, RuleError> {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix(NOT) {
        let rest = rest.trim_start();
        let Some(inner) = strip_parens(rest)? else {
            return Err(RuleError::LonelyNot {
                text: text.to_string(),
            });
        };
        if inner.trim().is_empty() {
            return Err(RuleError::EmptyGroup {
                text: text.to_string(),
            });
        }
        return Ok(Node::Group(negate(parse_group(inner)?)));
    }
    if let Some(inner) = strip_parens(text)? {
        if inner.trim().is_empty() {
            return Err(RuleError::EmptyGroup {
                text: text.to_string(),
            });
        }
        return Ok(Node::Group(parse_group(inner)?));
    }
    Ok(Node::Clause(Clause::parse(text)?))
}

/// 把一个读出来的组翻成「都不满足」。
///
/// `都不(A 或 B)` 说的是「A 与 B 一个都别成立」——那正是一个装着 A 与 B 的
/// **都不满足**组，所以里面那层「任一满足」拍平掉。而 `都不((A 且 B))` 否掉的是
/// **一整个**「全部满足」组，那一层留着，否则「A 与 B 不同时成立」会被悄悄改成
/// 「A 与 B 都不成立」——两句话选出来的东西差得远。
fn negate(inner: Group) -> Group {
    // **里面那层本身就是「都不满足」时一律留着。** 拍平的话 `都不(都不 A)` 会变成
    // `都不 A`——双重否定被折成了单重否定，**意思正好相反**。留着那一层，它读作
    // 「那个『都不满足』组不成立」，也就是「至少有一条成立」，正是双重否定该有的意思。
    if inner.join != Join::None && (inner.nodes.len() == 1 || inner.join == Join::Any) {
        Group {
            join: Join::None,
            nodes: inner.nodes,
        }
    } else {
        Group {
            join: Join::None,
            nodes: vec![Node::Group(inner)],
        }
    }
}

/// 这个 `(` 是**结构上的括号**吗——它前面那一段（从这一项的开头算起）去掉空白之后
/// 是空的，或者正好是 `都不`。
///
/// 别处的 `(` 是值里的字。作品名里带括号是常态，而且**未必配对**
/// （`口袋妖怪(日`）——见字就当括号会让那种名字整条读不懂。
fn opens_group(text: &str, item_start: usize, at: usize) -> bool {
    let head = text[item_start..at].trim();
    head.is_empty() || head == NOT
}

/// 一个**两侧带空白的**连接词吗。
///
/// 要求两侧有空白不是挑剔：作品名里完全可能有这两个字（「一将功成万骨枯」
/// 「生存或毁灭」那类），见字就切会把它劈成两半。
fn padded_at(text: &str, index: usize, ch: char) -> bool {
    text[..index].chars().next_back().is_some_and(char::is_whitespace)
        && text[index + ch.len_utf8()..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
}

/// 一个**值里**的连接词，印进规则之后会被 [`padded_at`] 认成分隔符吗。
///
/// 与 [`padded_at`] 差在两头，差的理由都在 [`render_group`] 印出来的那行字里：
///
/// - **右边到头也算一侧空白。** 值印出去后面紧跟的可能就是 `" 且 "` / `" 或 "` 这个
///   分隔符——**它把值末尾那个连接词缺的空白补上了**。于是 `作品~RPG 且` 后面一接
///   兄弟就成了 `作品~RPG 且 且 平台=GB`，读回来值被截成 `RPG`（子库比屏上多选，
///   **一声不响**），后面接的是 ` 或 ` 时整条落成 [`RuleError::MixedJoin`]。
/// - **左边到头不算。** 值左边紧挨着的永远是运算符（`作品~` 那个 `~`，见
///   [`Clause`] 的 `Display`），**绝不是空白**，所以值开头那个连接词读不成分隔符
///   ——值恰好就是一个 `且` 字也一样，照收。
///
/// [`padded_at`] 看的是**整行**规则，那里右边到头就是真的到头、后面不会再有分隔符，
/// 所以那一侧不能这么算。两处规矩不同是因为看的东西不同，不是两套写法没对齐。
fn reads_as_joiner(value: &str, index: usize, ch: char) -> bool {
    value[..index]
        .chars()
        .next_back()
        .is_some_and(char::is_whitespace)
        && value[index + ch.len_utf8()..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
}

/// 从 `at` 处那个 `(` 找出与它配对的 `)`；配不上就是 `None`。
///
/// **数括号的规矩与 [`split_items`] 逐字一致**（[`opens_group`]）：两处不一致的话，
/// 同一行字切成几项与切在哪儿会给出两个答案。
fn matching(text: &str, at: usize) -> Option<usize> {
    // 每一层记着「当前这一项从哪个字节开始」，于是「项的开头」这条规则在里层也成立。
    let mut starts: Vec<usize> = vec![at + 1];
    // **与 `split_items` 同一套数法**：值里的括号成对跳过（那个函数里有为什么）。
    let mut in_value: Vec<u32> = vec![0];
    for (index, ch) in text.char_indices().filter(|(index, _)| *index > at) {
        match ch {
            '(' => {
                let start = *starts.last()?;
                if opens_group(text, start, index) {
                    starts.push(index + ch.len_utf8());
                    in_value.push(0);
                } else if let Some(depth) = in_value.last_mut() {
                    *depth += 1;
                }
            }
            ')' if in_value.last().is_some_and(|depth| *depth > 0) => {
                if let Some(depth) = in_value.last_mut() {
                    *depth -= 1;
                }
            }
            ')' => {
                starts.pop();
                in_value.pop();
                if starts.is_empty() {
                    return Some(index);
                }
            }
            AND | OR if padded_at(text, index, ch) => {
                *starts.last_mut()? = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    None
}

/// 这一段是不是**整个**裹在一对括号里；是的话把里面交出来。
fn strip_parens(text: &str) -> Result<Option<&str>, RuleError> {
    if !text.starts_with('(') {
        return Ok(None);
    }
    let Some(close) = matching(text, 0) else {
        return Err(RuleError::UnbalancedParen {
            text: text.to_string(),
        });
    };
    // 收尾那个括号后面还有字，说明这一项不是「整个裹在括号里」。
    Ok((close + 1 == text.len()).then(|| &text[1..close]))
}

/// 按**两侧带空白的**连接词把一层切成一个个项，顺带认出这一层用的是哪个连接词。
///
/// 括号里的连接词不算数——那是下一层的事。
fn split_items(text: &str) -> Result<(Option<Join>, Vec<&str>), RuleError> {
    let mut out = Vec::new();
    let mut join: Option<Join> = None;
    // 栈顶那一层的「当前这一项从哪个字节开始」；栈底就是这一层。
    let mut starts: Vec<usize> = vec![0];
    // **每一层还欠着几个「值里的左括号」。** 不数这个的话，`(作品~魂斗罗 (J) 且 平台=FC)`
    // 里 `(J)` 那个右括号会把**外层组**提前关掉：`(J)` 的左括号不在项的开头、不开组，
    // 而右括号只看「有没有开着的组」就 pop，两边对不上。后果是整条规则切错、存成子库
    // 之后静默变成 `BrokenRule`——而 ROM 名字带括号是常态（`魂斗罗 (J)`、`Contra (USA)`）。
    let mut in_value: Vec<u32> = vec![0];
    for (index, ch) in text.char_indices() {
        match ch {
            '(' => {
                let start = *starts.last().unwrap_or(&0);
                if opens_group(text, start, index) {
                    starts.push(index + ch.len_utf8());
                    in_value.push(0);
                } else if let Some(depth) = in_value.last_mut() {
                    *depth += 1;
                }
            }
            // 先还值里欠下的那些，再谈关组。没有开着的组时，`)` 是值里的字
            // （`作品~笑)` 那种）。
            ')' if in_value.last().is_some_and(|depth| *depth > 0) => {
                if let Some(depth) = in_value.last_mut() {
                    *depth -= 1;
                }
            }
            ')' if starts.len() > 1 => {
                starts.pop();
                in_value.pop();
            }
            AND | OR if padded_at(text, index, ch) => {
                if starts.len() > 1 {
                    // 里层的连接词是下一层的事，这里只把那一层的项起点往后挪。
                    if let Some(last) = starts.last_mut() {
                        *last = index + ch.len_utf8();
                    }
                    continue;
                }
                let here = if ch == AND { Join::All } else { Join::Any };
                match join {
                    Some(current) if current != here => {
                        return Err(RuleError::MixedJoin {
                            text: text.to_string(),
                        });
                    }
                    _ => join = Some(here),
                }
                let start = starts.first().copied().unwrap_or(0);
                out.push(text[start..index].trim());
                if let Some(first) = starts.first_mut() {
                    *first = index + ch.len_utf8();
                }
            }
            _ => {}
        }
    }
    if starts.len() > 1 {
        return Err(RuleError::UnbalancedParen {
            text: text.to_string(),
        });
    }
    out.push(text[starts.first().copied().unwrap_or(0)..].trim());
    out.retain(|piece| !piece.is_empty());
    Ok((join, out))
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
                    "文字维度用 = != ~ ^ $"
                },
            });
        }
        // **读库里躺着的那些走宽的这条**：值里带一个落单的括号也照样读得懂，
        // 这一票之前存进去的规则一条都不许失效（见模块文档）。
        Self::assemble(dimension, op, tail)
    }

    /// 从**三样东西**造一个子句：界面上那三个控件各交出一样。
    ///
    /// 与 [`Self::parse`] 同一条读法，但**多两道闸**：造出来的东西马上要被
    /// [`Rule::from_group`] 印成一行字存进中立库，所以它必须**摆在组里哪个位置都
    /// 印出去再读回来还是它自己**。两道闸各挡一种走样（见模块文档）：
    ///
    /// - 值里有会被读成分隔符的连接词（`reads_as_joiner`）——`作品~RPG 且 平台=GB`
    ///   打在一个值格子里，印出来那行字读回来是**两个**子句，而屏上筛的是一个；
    ///   `作品~RPG 且` 这种结在末尾的一样走样，**缺的那个空白由分隔符补上**。
    /// - 值里的括号不配对——这个子句套进一个组里之后，那半个括号会把组提前收尾。
    ///
    /// **两道闸都看 `value.trim()`**，因为存进 [`Self::raw`]、将来印出去的正是它：
    /// 值两端的空白 trim 掉就不存在了，「结在末尾」说的也是 trim 之后的末尾。
    ///
    /// # Errors
    /// 见 [`RuleError`]。
    pub fn build(dimension: Dimension, op: Op, value: &str) -> Result<Self, RuleError> {
        // 闸看的是 trim 过的那份——存进去、印出去的就是它。
        let value = value.trim();
        for (index, ch) in value.char_indices() {
            if matches!(ch, AND | OR) && reads_as_joiner(value, index, ch) {
                return Err(RuleError::JoinerInValue {
                    dimension: dimension.label(),
                    value: value.to_string(),
                    joiner: ch,
                });
            }
        }
        let mut depth = 0_i32;
        for ch in value.chars() {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            if depth < 0 {
                break;
            }
        }
        if depth != 0 {
            return Err(RuleError::UnbalancedParen {
                text: value.to_string(),
            });
        }
        Self::assemble(dimension, op, value)
    }

    /// 读法那一段，[`Self::parse`] 与 [`Self::build`] 共用。
    fn assemble(dimension: Dimension, op: Op, value: &str) -> Result<Self, RuleError> {
        if !op.fits(dimension) {
            return Err(RuleError::BadOperator {
                dimension: dimension.label(),
                op: op.label(),
                advice: if dimension.is_number() {
                    "数值维度用 = != <= < >= >"
                } else {
                    "文字维度用 = != ~ ^ $"
                },
            });
        }
        let values: Vec<String> = value
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
                    value: value.trim().to_string(),
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
            raw: value.trim().to_string(),
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
        // **与库里那些年份值同一份判据**（[`parse_year`]）：规则里写 `年份>=199` 时
        // 当场说不认得，好过悄悄比一个库里根本不会有的数。
        Dimension::Year => parse_year(value)
            .map(f64::from)
            .ok_or_else(|| bad("一个年份，如 1990")),
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

/// 读一个年份：**前后的空格去掉之后恰好四位 ASCII 数字**，别的一概不是年份。
///
/// ## 这是「什么算一个年份」的唯一一份定义
///
/// 年份在库里是一列**文本**，两个求值器都要从它读出一个数来比：内存那一侧
/// （[`sublibrary::facts`](crate::sublibrary::facts)）与 SQL 那一侧
/// （`catalog::filter` 的 `YEAR_VALUE`）。两处各写一遍判据的下场是这个函数的来历——
/// 从前 SQL 拿 `CAST` 往返比原文、内存拿 `parse::<i32>()`，于是 `01996` 与 `+1996`
/// 在屏上不算年份、按下同步却算，**筛出来的那批与搬过去的那批不是同一批**。
/// SQL 那一侧眼下逐字照这里写成一句 `GLOB '[0-9][0-9][0-9][0-9]'`。
///
/// ## 为什么是「恰好四位」
///
/// 会产年份的源本来就只产四位数字，而且都写着为什么：
/// [`tosec_year`](crate::identify::naming::tosec_year) 对 `199x` 的处置是「只认得出
/// 四位数字才产出——把『不知道』伪装成『知道』等于骗人」，在线源那一侧一字不差。
/// 能写出别的形态的只剩人手打进裁决的那条路，而 `01996`、`+1996`、`196`、`19960`
/// 都不是这个库里年份的写法：与其猜一个，不如当这个变体在年份这一维上没有值
/// （那时 `!=` 成立、别的运算符一律不成立，见模块文档）。
///
/// **只去空格，不是 [`str::trim`]**：SQLite 的 `trim()` 只去空格，而这一条判据在两侧
/// 必须逐字对得上——制表符去不去这种事上分家，与当初 `CAST` 那次分家是同一个错。
#[must_use]
pub fn parse_year(text: &str) -> Option<i32> {
    let text = text.trim_matches(' ');
    (text.len() == 4 && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
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

    fn 子句(rule: &Rule, at: usize) -> Clause {
        rule.clauses()[at].clone()
    }

    #[test]
    fn 值里带括号的作品名不会把组提前关掉() {
        // **ROM 名字带括号是常态**（`魂斗罗 (J)`、`Contra (USA)`）。早先 `(J)` 那个
        // 右括号只看「有没有开着的组」就关组，于是外层组被提前关掉、整条规则切错，
        // 在界面上「存成子库」之后**静默变成 `BrokenRule`**。
        let rule = Rule::parse("(作品~魂斗罗 (J) 且 平台=FC) 或 平台=SFC").expect("读得懂");
        assert_eq!(rule.root.join, Join::Any, "顶层是「任一满足」");
        assert_eq!(rule.root.nodes.len(), 2, "顶层两项：那个组，加一条平台");
        let Node::Group(inner) = &rule.root.nodes[0] else {
            panic!("头一项该是个组：{:?}", rule.root.nodes[0]);
        };
        assert_eq!(inner.join, Join::All);
        assert_eq!(inner.nodes.len(), 2, "组里两条：作品与平台");
        // 括号原样留在值里，一个字都没少。
        let Node::Clause(clause) = &inner.nodes[0] else {
            panic!("组里头一条该是子句");
        };
        assert!(
            format!("{clause:?}").contains("魂斗罗 (J)"),
            "值里那对括号要原样留着：{clause:?}"
        );
        // 整条读回来还是它自己。
        assert!(
            Rule::parse(&rule.text).is_ok(),
            "读得回来：{}",
            rule.text
        );
    }

    #[test]
    fn 双重否定翻回来而不是折成单重否定() {
        // `都不(都不 A)` 说的是 A 成立。早先「里面只有一条就拍平」把它折成了
        // `都不 A`——**意思正好相反**，而且不报错。
        let 单 = Rule::parse("都不(平台=FC)").expect("读得懂");
        let 双 = Rule::parse("都不(都不(平台=FC))").expect("读得懂");
        assert_ne!(
            format!("{:?}", 单.root),
            format!("{:?}", 双.root),
            "双重否定不该与单重否定长成同一棵树"
        );
        // 双重否定那一层留着：读作「那个『都不满足』组不成立」。
        assert_eq!(双.root.join, Join::None);
        assert_eq!(双.root.nodes.len(), 1);
        assert!(
            matches!(&双.root.nodes[0], Node::Group(inner) if inner.join == Join::None),
            "里面那层「都不满足」要留着：{:?}",
            双.root.nodes[0]
        );
    }

    #[test]
    fn 一条像样的规则拆得开() {
        let rule = Rule::parse("平台=GB,GBA 且 中文=汉化 且 体积<=64MiB").expect("读得懂");
        // 平铺的老规则就是一个顶层的「全部满足」组。
        assert_eq!(rule.root.join, Join::All);
        assert_eq!(rule.root.nodes.len(), 3);
        assert_eq!(rule.root.depth(), 1);
        assert_eq!(子句(&rule, 0).dimension, Dimension::Platform);
        assert_eq!(
            子句(&rule, 0).bound,
            Bound::Text(vec!["GB".to_string(), "GBA".to_string()])
        );
        assert_eq!(子句(&rule, 2).op, Op::Le);
        assert_eq!(子句(&rule, 2).bound, Bound::Number(64.0 * 1024.0 * 1024.0));
        // 原文一字不改地留着：报告里印的是用户写的那句话。
        assert_eq!(rule.text, "平台=GB,GBA 且 中文=汉化 且 体积<=64MiB");
        // 值那一段也留着原文，`64MiB` 印回去还是 `64MiB` 而不是 67108864。
        assert_eq!(子句(&rule, 2).to_string(), "体积<=64MiB");
    }

    #[test]
    fn 值里的连接词不当分隔符() {
        // 两侧没空白就不是连接词，否则作品名一带这两个字就被劈成两半。
        for name in ["一将功成万骨枯", "生存或毁灭"] {
            let rule = Rule::parse(&format!("作品~{name}")).expect("读得懂");
            assert_eq!(rule.clauses().len(), 1);
            assert_eq!(子句(&rule, 0).bound, Bound::Text(vec![name.to_string()]));
        }
    }

    #[test]
    fn 三种连接各自成立而且组嵌得下去() {
        let rule = Rule::parse("平台=GB 且 (作品^口袋 或 类型~RPG) 且 都不(语言=En 或 中文=汉化)")
            .expect("读得懂");
        assert_eq!(rule.root.join, Join::All);
        assert_eq!(rule.root.nodes.len(), 3);
        assert_eq!(rule.root.depth(), 2, "顶层加一层子组");
        let Node::Group(any) = &rule.root.nodes[1] else {
            panic!("第二项该是个组");
        };
        assert_eq!(any.join, Join::Any);
        assert_eq!(any.nodes.len(), 2);
        let Node::Group(none) = &rule.root.nodes[2] else {
            panic!("第三项该是个组");
        };
        // `都不(A 或 B)` 就是一个装着 A 与 B 的「都不满足」组，中间不多一层。
        assert_eq!(none.join, Join::None);
        assert_eq!(none.nodes.len(), 2);
    }

    #[test]
    fn 嵌到第三层还是读得懂() {
        let rule =
            Rule::parse("平台=GB 且 (中文=汉化 或 (类型~RPG 且 年份>=1995))").expect("读得懂");
        assert_eq!(rule.root.depth(), 3);
        assert_eq!(rule.clauses().len(), 4);
    }

    #[test]
    fn 都不套一个全部满足组否掉的是那一整条() {
        let inner = Rule::parse("都不((平台=GB 且 中文=汉化))").expect("读得懂");
        let Group { join, nodes } = &inner.root;
        assert_eq!(*join, Join::None);
        assert_eq!(nodes.len(), 1, "「不同时成立」那一层留着，不许拍平");
        let Node::Group(all) = &nodes[0] else {
            panic!("里面该还是个组");
        };
        assert_eq!(all.join, Join::All);
    }

    #[test]
    fn 印回去再读一遍还是同一棵树() {
        for text in [
            "平台=GB,GBA 且 中文=汉化 且 体积<=64MiB",
            "平台=GB 且 (作品^口袋 或 类型~RPG)",
            "都不(语言=En 或 中文=汉化)",
            "都不((平台=GB 且 中文=汉化))",
            "平台=GB 且 (中文=汉化 或 (类型~RPG 且 年份>=1995))",
            "收藏=是 或 开发商$Freak",
        ] {
            let once = Rule::parse(text).expect("读得懂");
            let printed = Rule::from_group(once.root.clone());
            let twice = Rule::parse(&printed.text).expect("印回去还读得懂");
            assert_eq!(once.root, twice.root, "「{text}」印回去变了棵树");
        }
    }

    #[test]
    fn 括号只在项的开头才是括号() {
        // 作品名里的括号是值里的字，不开组。
        let rule = Rule::parse("作品~Pocket Monsters (Japan) 或 平台=GB").expect("读得懂");
        assert_eq!(rule.root.join, Join::Any);
        assert_eq!(
            子句(&rule, 0).bound,
            Bound::Text(vec!["Pocket Monsters (Japan)".to_string()])
        );
    }

    #[test]
    fn 值里落单的括号不影响这一层怎么切() {
        // **这一票之前这几条就是合法的**，一条都不许因为加了分组而失效。
        for (text, want) in [
            ("作品~口袋妖怪(日", "口袋妖怪(日"),
            ("作品~笑)", "笑)"),
            ("简介~(注", "(注"),
        ] {
            let rule = Rule::parse(text).unwrap_or_else(|error| panic!("「{text}」：{error}"));
            assert_eq!(rule.clauses().len(), 1, "「{text}」被切成了好几项");
            assert_eq!(子句(&rule, 0).bound, Bound::Text(vec![want.to_string()]));
        }
        // 落单的括号在一层里也不该把后面那半吞掉。
        let rule = Rule::parse("作品~口袋妖怪(日 且 平台=GB").expect("读得懂");
        assert_eq!(rule.clauses().len(), 2);
        // 而**项开头**那个括号没收尾，那是真的对不上，当场报出来。
        assert!(matches!(
            Rule::parse("(平台=GB 且 中文=汉化"),
            Err(RuleError::UnbalancedParen { .. })
        ));
    }

    #[test]
    fn 造一条新子句时挡住印出去会走样的值() {
        // 界面上把整句话打进一个值格子里：印出来那行字读回来会是**两个**子句，
        // 而屏上筛的是一个——在造它的那一步就挡住，不让它进中立库。
        assert!(matches!(
            Clause::build(Dimension::Work, Op::Contains, "RPG 且 平台=GB"),
            Err(RuleError::JoinerInValue { joiner: '且', .. })
        ));
        assert!(matches!(
            Clause::build(Dimension::Work, Op::Contains, "生存 或 毁灭"),
            Err(RuleError::JoinerInValue { joiner: '或', .. })
        ));
        // 括号不配对：这个子句套进一个组之后，那半个括号会把组提前收尾。
        assert!(matches!(
            Clause::build(Dimension::Work, Op::Contains, "口袋妖怪(日"),
            Err(RuleError::UnbalancedParen { .. })
        ));
        // 配对的照收——作品名里带括号是常态。
        assert!(Clause::build(Dimension::Work, Op::Contains, "Pocket (Japan)").is_ok());
        // 两侧没空白的「且」「或」是值里的字，照收。
        assert!(Clause::build(Dimension::Work, Op::Is, "一将功成万骨枯").is_ok());
        assert!(Clause::build(Dimension::Work, Op::Is, "生存或毁灭").is_ok());
        // **读库里躺着的那条路照旧宽**：这一票之前存进去的规则一条都不许失效。
        assert!(Rule::parse("作品~口袋妖怪(日").is_ok());
    }

    #[test]
    fn 造出来的子句套进组里印回去还是它自己() {
        // 值里带一对括号、又套在组里——两条规矩（项开头才是括号、造的时候要配对）
        // 合起来才保得住这一条。
        let clause = Clause::build(Dimension::Work, Op::Contains, "Pocket (Japan)").expect("造得出");
        let rule = Rule::from_group(Group::new(
            Join::Any,
            vec![
                Node::Clause(clause.clone()),
                Node::Clause(Clause::build(Dimension::Platform, Op::Is, "GB").expect("造得出")),
            ],
        ));
        let back = Rule::parse(&rule.text).expect("印回去还读得懂");
        assert_eq!(back.root, rule.root, "「{}」印回去变了棵树", rule.text);
        assert_eq!(back.clauses()[0], &clause);
    }

    /// 对拍表里那两条填充子句：摆在新造的那条**前面**与**后面**当兄弟。
    fn 平台() -> Node {
        Node::Clause(Clause::build(Dimension::Platform, Op::Is, "GB").expect("造得出"))
    }

    fn 中文() -> Node {
        Node::Clause(Clause::build(Dimension::Chinese, Op::Is, "汉化").expect("造得出"))
    }

    /// 一种**组形**：报错时印的那个名字，加上把新造的那条子句摆进去的办法。
    type 一种组形 = (&'static str, fn(Node) -> Group);

    /// 九种**组形**：把新造的那条子句摆在各个位置——独自一条、后面有兄弟、前面有兄弟、
    /// 夹在中间、在子组里、在 `都不` 组里、在第三层。
    ///
    /// **位置要紧**：值末尾那个连接词只在**后面还接着分隔符**时才现形，一条子句
    /// 自己印出去看不出任何毛病。
    fn 组形() -> Vec<一种组形> {
        vec![
            ("独自一条", |新| Group::new(Join::All, vec![新])),
            ("且·后面有兄弟", |新| {
                Group::new(Join::All, vec![新, 平台()])
            }),
            ("且·前面有兄弟", |新| {
                Group::new(Join::All, vec![平台(), 新])
            }),
            ("且·夹在中间", |新| {
                Group::new(Join::All, vec![平台(), 新, 中文()])
            }),
            ("或·后面有兄弟", |新| {
                Group::new(Join::Any, vec![新, 平台()])
            }),
            ("子组里·后面有兄弟", |新| {
                Group::new(
                    Join::All,
                    vec![
                        Node::Group(Group::new(Join::Any, vec![新, 平台()])),
                        中文(),
                    ],
                )
            }),
            ("子组里·收尾那一条", |新| {
                Group::new(
                    Join::All,
                    vec![
                        Node::Group(Group::new(Join::Any, vec![平台(), 新])),
                        中文(),
                    ],
                )
            }),
            ("都不组里·后面有兄弟", |新| {
                Group::new(
                    Join::All,
                    vec![
                        Node::Group(Group::new(Join::None, vec![新, 平台()])),
                        中文(),
                    ],
                )
            }),
            ("第三层最里那一条", |新| {
                Group::new(
                    Join::All,
                    vec![
                        平台(),
                        Node::Group(Group::new(
                            Join::Any,
                            vec![
                                中文(),
                                Node::Group(Group::new(Join::All, vec![平台(), 新])),
                            ],
                        )),
                    ],
                )
            }),
        ]
    }

    /// 对拍表里的值：走样过的那一类，加上看着就刁钻的那些（括号、逗号、运算符的字面、
    /// `都不`、全角空白）。
    fn 刁钻的值() -> [&'static str; 36] {
        [
            "GB",
            "GB,GBA",
            "RPG",
            "Pocket Monsters (Japan)",
            "魂斗罗 (J)",
            "一将功成万骨枯",
            "生存或毁灭",
            "a且",
            "且a",
            "a且b",
            "且",
            "或",
            "且 RPG",
            "RPG 且",
            "RPG 或",
            "RPG　且",
            "(a) 且",
            "RPG 且 平台=GB",
            "生存 或 毁灭",
            "((a))",
            "(a)(b)",
            "a (b) c",
            "a,,b",
            "a, ,b",
            "=",
            "!=x",
            "~x",
            "平台=GB",
            "都不(x)",
            "x 都不(y)",
            "口袋妖怪(日",
            "笑)",
            "64MiB",
            "1990",
            "0.8",
            "80%",
        ]
    }

    #[test]
    fn 新造的子句摆在组里哪个位置印出去都读得回来() {
        // **屏上筛出来那批与存进子库那批必须是同一批**（票 04 立的那条硬约束）。两者之间
        // 只隔着「印成一行字再读回来」这一道，所以这里逐个值、逐种组形对拍：
        // `Clause::build` 放行的东西，`Rule::from_group` 印出去必须被 `Rule::parse`
        // 读回同一棵树。挡下来的不算数——那正是闸该干的活，另有测试钉它挡了什么。
        let 维度与运算符 = [
            (Dimension::Work, Op::Contains),
            (Dimension::Work, Op::Is),
            (Dimension::Work, Op::IsNot),
            (Dimension::Genre, Op::StartsWith),
            (Dimension::Developer, Op::EndsWith),
            (Dimension::Year, Op::Ge),
            (Dimension::Size, Op::Le),
            (Dimension::Rating, Op::Lt),
        ];
        let mut 对拍过 = 0_usize;
        for (dimension, op) in 维度与运算符 {
            for value in 刁钻的值() {
                let Ok(clause) = Clause::build(dimension, op, value) else {
                    continue;
                };
                for (形, 搭) in 组形() {
                    let root = 搭(Node::Clause(clause.clone()));
                    let rule = Rule::from_group(root.clone());
                    let back = Rule::parse(&rule.text).unwrap_or_else(|error| {
                        panic!("{形}｜值「{value}」印成「{}」读不回来：{error}", rule.text)
                    });
                    assert_eq!(
                        back.root, root,
                        "{形}｜值「{value}」印成「{}」读回来变了棵树",
                        rule.text
                    );
                    对拍过 += 1;
                }
            }
        }
        assert!(对拍过 > 200, "对拍表缩水了，只对了 {对拍过} 条");
    }

    #[test]
    fn 值以空白加连接词结尾时当场挡住() {
        // **末尾那个连接词后面缺的空白由分隔符补上**：`render_group` 拼的是 " 且 " / " 或 "，
        // 于是 `作品~RPG 且` 后面一接兄弟就成了 `作品~RPG 且 且 平台=GB`——读回来值被截成
        // `RPG`，子库比屏上多选，**一声不响**；后面接的是 ` 或 ` 的话整条落成 `MixedJoin`，
        // 那个子库一条都选不中。两样都是「屏上那批 ≠ 存进去那批」，在造它的那一步就挡住。
        for (value, joiner) in [
            ("RPG 且", '且'),
            ("RPG 或", '或'),
            ("RPG　且", '且'),
            ("(a) 且", '且'),
            ("RPG 且 ", '且'),
            (" x 或\u{3000}", '或'),
        ] {
            let built = Clause::build(Dimension::Work, Op::Contains, value);
            assert!(
                matches!(&built, Err(RuleError::JoinerInValue { joiner: 撞上, .. }) if *撞上 == joiner),
                "「{value}」该被挡住：{built:?}"
            );
        }
        // **值开头那个连接词照收**：印出去它左边紧挨着的永远是运算符（`作品~` 那个 `~`），
        // 不是空白，读不成分隔符。值恰好就是一个连接词也一样。
        for value in ["且", "或", "且 RPG", "或 毁灭", "a且", "RPG或"] {
            assert!(
                Clause::build(Dimension::Work, Op::Contains, value).is_ok(),
                "「{value}」印出去读得回来，不该挡"
            );
        }
    }

    #[test]
    fn 长符号先认() {
        let rule = Rule::parse("年份>=1990").expect("读得懂");
        assert_eq!(子句(&rule, 0).op, Op::Ge);
        assert_eq!(子句(&rule, 0).bound, Bound::Number(1990.0));
        let rule = Rule::parse("平台!=PSV").expect("读得懂");
        assert_eq!(子句(&rule, 0).op, Op::IsNot);
    }

    #[test]
    fn 两个新运算符认得出来() {
        let rule = Rule::parse("作品^口袋 且 开发商$Freak").expect("读得懂");
        assert_eq!(子句(&rule, 0).op, Op::StartsWith);
        assert_eq!(子句(&rule, 1).op, Op::EndsWith);
        assert_eq!(子句(&rule, 1).dimension, Dimension::Developer);
        // 数值维度上这两个都用不了：「以 1990 开始」说不清是什么意思。
        assert!(matches!(
            Rule::parse("年份^199"),
            Err(RuleError::BadOperator { .. })
        ));
    }

    #[test]
    fn 新维度认得出来() {
        for (text, dimension) in [
            ("收藏=是", Dimension::Favorite),
            ("开发商=Game Freak", Dimension::Developer),
            ("发行商=Nintendo", Dimension::Publisher),
            ("简介~勇者", Dimension::Description),
        ] {
            let rule = Rule::parse(text).expect("读得懂");
            assert_eq!(子句(&rule, 0).dimension, dimension);
        }
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
        // 同一层混用连接词：哪个先算说不清，当场拦下来而不是替用户猜。
        assert!(matches!(
            Rule::parse("平台=GB 且 中文=汉化 或 语言=En"),
            Err(RuleError::MixedJoin { .. })
        ));
        assert!(matches!(
            Rule::parse("(平台=GB 且 中文=汉化"),
            Err(RuleError::UnbalancedParen { .. })
        ));
        assert!(matches!(
            Rule::parse("都不 平台=GB"),
            Err(RuleError::LonelyNot { .. })
        ));
        assert!(matches!(
            Rule::parse("平台=GB 且 ()"),
            Err(RuleError::EmptyGroup { .. })
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
    fn 年份只认四位数字() {
        assert_eq!(parse_year("1996"), Some(1996));
        // 前后的空格去掉再看——**只去空格**，与 SQLite 的 `trim()` 逐字对齐。
        assert_eq!(parse_year(" 1996 "), Some(1996));
        assert_eq!(parse_year("\t1996"), None);
        // 前导 `0` 与前导 `+`：`parse::<i32>()` 收得下，而它们不是年份的写法。
        assert_eq!(parse_year("01996"), None);
        assert_eq!(parse_year("+1996"), None);
        // 位数不对、半截日期、说不出是哪一年、汉字数字，一概不是年份。
        assert_eq!(parse_year("196"), None);
        assert_eq!(parse_year("19960"), None);
        assert_eq!(parse_year("1996-12"), None);
        assert_eq!(parse_year("199X"), None);
        assert_eq!(parse_year("一九九六"), None);
        assert_eq!(parse_year(""), None);
        // 规则里写的那个数走的是同一份判据。
        assert_eq!(
            子句(&Rule::parse("年份>=1990").expect("读得懂"), 0).bound,
            Bound::Number(1990.0)
        );
        assert!(matches!(
            Rule::parse("年份>=199"),
            Err(RuleError::BadNumber { .. })
        ));
    }

    #[test]
    fn 评分两种写法折成同一个数() {
        assert_eq!(parse_rating("0.8"), Some(0.8));
        assert_eq!(parse_rating("80%"), Some(0.8));
        assert_eq!(parse_rating("1.5"), None);
        assert_eq!(parse_rating("-1"), None);
    }
}
