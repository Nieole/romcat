//! **中文离线数据源**：本机一份中文条目索引，加一个带交叉校验的模糊匹配。
//!
//! 这是识别的**倒数第二层**（票 11）。前面几层——CRC-32、光盘序列号、卡带内部头——
//! 靠的都是**内容自己的字节**；到这一层字节已经说不出话了，手上只剩一个文件名。
//! 而这个库里的文件名多半是中文，中文在 No-Intro / Redump / TOSEC 里**一个字都没有**
//! （`title` 模块文档说过这件事）。所以要另一口井。
//!
//! ## 为什么是 Bangumi 的离线 dump
//!
//! 调研（`docs/research/scraper-sources.md` §9.1、§13.1）的结论逐条落在这里：
//!
//! - **它是中文游戏元数据里最好的一口井**，而且不只做日系：实测 Halo、GTA、
//!   Gunstar Heroes 都有条目、有中文译名。共 87,188 条游戏。
//! - **有官方每周三导出的离线 dump**，435 MB 一个 zip。于是这一层
//!   **零成本、可复现、可反复重跑调参数**——这三样是模型推断（票 12）都没有的，
//!   也是它值得做扎实的全部理由。
//! - **老平台深度浅**：N64 仅 98 条、DC 38 条、PCE 27 条。这不是缺陷是事实，
//!   报告要按平台把条目数摆出来，不然「这个平台命中低」会被误读成「匹配算法不行」
//!   （与 DAT 那一侧「没有弹药的平台命中率低与识别准不准无关」是同一条）。
//! - **dump 完全不含图片**。封面仍然只能上网取（票 14），这一层补不了。
//!
//! ## 模糊匹配一定会错配，所以有两道交叉校验
//!
//! 调研实测的那个反例：`Solar Jetman` 会被 Bangumi 的搜索错配成 `Solar 2`。结论是
//! **必须三重校验**——平台一致、年份差 ≤1、名称相似度过阈值——**宁可留空也不要写错的
//! 中文名**。这一层照做：
//!
//! | 校验 | 两边都说得出 | 有一边说不出 | 对不上 |
//! |---|---|---|---|
//! | 平台 | [`Check::Agrees`] | [`Check::Unknown`] | **整条不产出** |
//! | 年份 | [`Check::Agrees`] | [`Check::Unknown`] | **整条不产出** |
//!
//! **「说不出」不等于「对得上」。** 真库里绝大多数文件名写不出年份（`2021汉化修复版`
//! 里那个 2021 是汉化的年份，不是发行的年份），所以年份那一栏多半是「说不出」——
//! 那时这条候选只能是**低置信**。两道校验都真的对上了才够得着**中置信**，
//! 而**这一层永远不产出高置信、永远不自动通过**（ADR-0002）。
//!
//! ## 匹配算法：二元组的 Dice 系数
//!
//! 把两个名字都折成一串「只剩字母数字与汉字」的键，切成相邻两字的二元组，
//! 算 `2×交集 / (两边之和)`。选它而不是编辑距离，是因为中文标题的差异多半是
//! **插入与删除**（`口袋妖怪 火红` vs `口袋妖怪火红版`），而编辑距离对长度差异敏感、
//! 又不便于用倒排索引把候选圈小。
//!
//! 上面那个反例在这套算法下自己就落选了：`solarjetman` 与 `solar2` 的 Dice 是 0.53，
//! 够不到门槛（有一条测试钉着这件事）。
//!
//! ## 光有相似度不够：**数字与拉丁字母必须一模一样**
//!
//! 真机第一趟实测把这件事摆得很清楚：低置信那一档抽样 25 条，**17 条是错的**，
//! 而错法高度集中——`第3次超级机器人大战` 撞上 `第4次超级机器人大战`、
//! `大金刚鼓1` 撞上 `大金刚鼓3`、`超级马里奥银河` 撞上 `超级马力欧银河2`、
//! `最终幻想2` 撞上 `最终幻想`。
//!
//! 二元组的 Dice 对这种差异天生迟钝：十个二元组里差一个，分数还有 0.9。可
//! **续作编号恰恰是标题里最不能忽略的那一部分**——差一个数字就是另一个游戏。
//!
//! 所以这一层有一道硬闸：**两边的数字与拉丁字母序列必须完全相同**（[`alnum_of`]），
//! 不同就整条不产出，与相似度多高无关。
//!
//! 为什么连字母一起管：第二轮抽样里剩下的错配换了个马甲，判据一模一样——
//! `超级机器人大战J` 撞上 `超级机器人大战R`（0.88）、`無人島物語` 撞上
//! `無人島物語X～外伝～`、`Civilization VI` 撞上 `Civilization VII`。**续作与版本的
//! 区分符在中文标题里几乎总是落在数字与拉丁字母上**，而汉字那一侧的差异相似度看得见。
//!
//! 代价说清楚：一边写中文名、另一边只有拉丁名时撞不上了（`合金弹头7` 对
//! `Metal Slug 7`）——但那两串本来就没有共同的二元组，相似度早就是 0。真正的代价是
//! `FFTA2_CHS_BUS` 这种名字里挂着汉化组缩写的：`ffta2chsbus` 与 `ffta2` 对不上。
//! 出口是剥离规则——把 `CHS` 这类记号补进配置，重跑一遍就撞上了。

pub mod dump;
pub mod store;
pub mod sync;

use std::collections::BTreeMap;

use crate::classify::is_cjk;
use crate::path::nfc;

/// 一条**叫法**是哪一种。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NameKind {
    /// 原名：条目自己的名字，多半是日文或英文。
    Original,
    /// **中文名**：这份数据源的全部价值所在。
    Chinese,
    /// 别名。
    Alias,
}

impl NameKind {
    /// 依据里写的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Original => "原名",
            Self::Chinese => "中文名",
            Self::Alias => "别名",
        }
    }

    /// 存进索引用的短码。**不拿展示词当键**（同 `identify::ident::IdKind::code`）。
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Original => "name",
            Self::Chinese => "name-cn",
            Self::Alias => "alias",
        }
    }
}

/// 条目上一条**不是叫法**的事实是哪一种。
///
/// 与 [`NameKind`] 分开是有理由的：那一个说的是「这条条目还叫什么」，会进**标题集合**；
/// 这一个说的是「这条条目是什么样的东西」，会进**字段**。两者存在同一张表里，读库的人
/// 就分不出哪些行该拿去撞名字、哪些行不该。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FactKind {
    /// 类型。
    Genre,
    /// 开发商。
    Developer,
    /// 发行商。
    Publisher,
}

impl FactKind {
    /// 依据里写的那个词。**用词表里的词**（`CONTEXT.md`）。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Genre => "类型",
            Self::Developer => "开发商",
            Self::Publisher => "发行商",
        }
    }

    /// 存进索引用的短码。**不拿展示词当键**（同 [`NameKind::code`]）。
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Genre => "genre",
            Self::Developer => "developer",
            Self::Publisher => "publisher",
        }
    }

    /// 存库时固定的遍历顺序。
    #[must_use]
    pub fn all() -> [Self; 3] {
        [Self::Genre, Self::Developer, Self::Publisher]
    }
}

/// 索引里的一条中文条目。
///
/// `Default` 是给测试与「先造一条再往里填」用的：真库里的条目全部由
/// [`sync`] 从 dump 折出来，**这份库里没有一行是攒出来的**（`store` 的模块文档）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    /// 数据源里的条目号。**依据里要写它**——人要去核对时，那是唯一查得回去的东西。
    pub id: u32,
    /// 原名。
    pub name: String,
    /// 中文名；数据源没写就是空串。
    pub name_cn: String,
    /// 别名。
    pub aliases: Vec<String>,
    /// 发行年份。
    pub year: Option<u16>,
    /// 折成本工具平台名的平台；一个都折不出来时是空的。
    pub platforms: Vec<String>,
    /// 数据源原样写的平台，写进依据。
    pub platform_text: String,
    /// **中文简介**；数据源没写就是空串。
    ///
    /// **原样留着**：换行、全角空格与数据源自带的排版都是内容的一部分（规格 18）。
    ///
    /// ⚠️ **从 [`store::Store::load`] 读回来的条目上，这一格永远是空串**——简介不进内存
    /// （那是九十来 MB 常驻，而撞名字不看简介）。要某一条的简介走
    /// [`store::Store::summary`]。
    pub summary: String,
    /// 类型；`infobox` 里一个键装着好几个的已经拆开了。
    pub genres: Vec<String>,
    /// 开发商。
    pub developers: Vec<String>,
    /// 发行商。
    pub publishers: Vec<String>,
}

impl Entry {
    /// 这条条目的全部叫法，按可信程度排好。
    #[must_use]
    pub fn names(&self) -> Vec<(NameKind, &str)> {
        let mut out: Vec<(NameKind, &str)> = Vec::new();
        for (kind, value) in [
            (NameKind::Chinese, self.name_cn.as_str()),
            (NameKind::Original, self.name.as_str()),
        ] {
            if !value.trim().is_empty() {
                out.push((kind, value));
            }
        }
        for alias in &self.aliases {
            if !alias.trim().is_empty() {
                out.push((NameKind::Alias, alias.as_str()));
            }
        }
        out
    }

    /// 某一类事实的全部值，按数据源里的原次序。
    #[must_use]
    pub fn facts(&self, kind: FactKind) -> &[String] {
        match kind {
            FactKind::Genre => &self.genres,
            FactKind::Developer => &self.developers,
            FactKind::Publisher => &self.publishers,
        }
    }

    /// 某一类事实那一格，写得进去的那一面。
    ///
    /// 收在这里而不是让调用方 `match` 一遍：读库那一侧要按种类把三格填回去，
    /// 各写一遍 `match` 的话，加第四种事实时漏掉一处编译器一句话都不会说。
    pub fn facts_mut(&mut self, kind: FactKind) -> &mut Vec<String> {
        match kind {
            FactKind::Genre => &mut self.genres,
            FactKind::Developer => &mut self.developers,
            FactKind::Publisher => &mut self.publishers,
        }
    }

    /// 拿去展示的那个名字：有中文名就用中文名。
    #[must_use]
    pub fn shown(&self) -> &str {
        if self.name_cn.trim().is_empty() {
            &self.name
        } else {
            &self.name_cn
        }
    }
}

/// 一道交叉校验的结论。
///
/// **三档而不是布尔**：「说不出」与「对不上」的处置完全相反——前者只是降一档置信度，
/// 后者整条候选不产出。混成一个布尔，一份年份读不出来的文件名会被当成「年份对不上」
/// 而整批落选，或者反过来被当成「对上了」而整批升档。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// 两边都说得出，而且对得上。
    Agrees,
    /// 至少有一边说不出。
    Unknown,
    /// 两边都说得出，对不上。
    Conflicts,
}

impl Check {
    /// 依据里写的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Agrees => "对得上",
            Self::Unknown => "说不出",
            Self::Conflicts => "对不上",
        }
    }
}

/// 匹配的参数。**它们是可调的，而且改完重跑一遍就看得见效果**——不必重新扫描，
/// 也不必重新取数（票 11 的验收）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tuning {
    /// 入门相似度：低于它的一律不产出。
    pub threshold: f64,
    /// 够得着**中置信**的相似度；还得两道交叉校验都真的对上。
    pub strong: f64,
    /// 一条查询最多产出几条候选。
    pub limit: usize,
    /// 一个二元组的倒排表长过这个数就不拿它去圈候选。
    ///
    /// 它只影响**圈候选**这一步的代价，不影响算分：`の` 这种二元组在几万条名字里都有，
    /// 拿它圈等于把全库过一遍，而它对「是不是同一个游戏」几乎不提供信息。
    pub max_postings: usize,
    /// 年份差多少之内算对得上。调研给的三重校验里写的是 ≤1。
    pub year_slack: u16,
}

impl Tuning {
    /// 这几个参数折成一串，进**输入指纹**。
    ///
    /// **每一个都要在里面**：任何一个变了，同一个名字撞出来的东西就可能不一样，
    /// 而漏掉的那一个变了之后，缓存会一口咬定「输入没变」而整条跳过
    /// （`scrape::Source::probe` 的文档说的就是这件事）。
    #[must_use]
    pub fn fingerprint(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            self.threshold, self.strong, self.limit, self.max_postings, self.year_slack
        )
    }
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            // **0.85 是拿真库调出来的**，而且是调过一轮才定的：0.72 那一版低置信档
            // 抽样 25 条错了 17 条（`口袋妖怪金` 撞上 `精灵宝可梦绿`、
            // `超级机器人大战LB` 撞上 `超级机器人大战DD`），提到 0.85 之后那一类
            // 「差一两个字的同系列条目」整批落选，而 `口袋妖怪 火红` 与
            // `口袋妖怪火红版` 这类真该撞上的照样撞得上。
            threshold: 0.85,
            strong: 0.90,
            limit: 3,
            max_postings: 3_000,
            year_slack: 1,
        }
    }
}

/// 一次查询。
#[derive(Debug, Clone, Copy)]
pub struct Query<'a> {
    /// 要撞的那串字（**剥离规则**剥完的正题，见 [`crate::filename`]）。
    pub text: &'a str,
    /// 这个变体的平台；说不出就是 `None`。
    pub platform: Option<&'a str>,
    /// 这个变体的发行年份；说不出就是 `None`。**多数时候就是说不出**。
    pub year: Option<u16>,
}

/// 一条匹配结论。
#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    /// 撞上的那条条目。
    pub entry: Entry,
    /// 撞上的是它的哪个叫法。
    pub matched: String,
    /// 那个叫法是哪一种。
    pub kind: NameKind,
    /// 相似度，0 到 1。
    pub score: f64,
    /// 两串字**折平之后完全相同**吗。
    ///
    /// 它与 `score == 1.0` 是同一件事，但单列一栏是有理由的：**中置信那一档要看它**
    /// （见 [`Match::strong`]），而拿浮点数去比相等是那种「今天对、改一行算法就错」
    /// 的判据。
    pub exact: bool,
    /// 平台这一道校验。
    pub platform: Check,
    /// 年份这一道校验。
    pub year: Check,
}

impl Match {
    /// 够得着**中置信**吗。
    ///
    /// **这是「中置信」的判据，不是「自动通过」的判据**——这一层永远不自动通过
    /// （ADR-0002：模糊匹配的结论一律进待确认队列）。
    ///
    /// 两条路，都要**平台对得上**打底：
    ///
    /// - **名字完全相同**（折平之后一个字不差）。
    /// - **相似度够高，而且年份也对得上。**
    ///
    /// 为什么给「名字完全相同」单开一条：真库实测**年份这一侧几乎永远说不出**——
    /// 文件名里写着的年份多半是汉化的年份（`地球冒险3 2021汉化修复版`），认了就是编数据，
    /// 所以 [`crate::filename`] 只认「整组正好四位数字」那一种，而那种写法在这个库里
    /// 少之又少。一刀切要求年份对上，这一档在真机上就是空的，等于没有分档。
    /// 而「名字一字不差 + 平台对得上」本来就是这一层给得出的最硬的证据。
    ///
    /// **年份仍然在起作用**：对不上的那些在 [`Index::lookup`] 里整条就不产出了。
    #[must_use]
    pub fn strong(&self, tuning: &Tuning) -> bool {
        if self.platform != Check::Agrees {
            return false;
        }
        self.exact || (self.score >= tuning.strong && self.year == Check::Agrees)
    }

    /// 这条匹配的**依据**，写成给人看的一句。
    ///
    /// 收在这里而不是各调用方各写一遍：识别那一层与刮削那一侧读的是同一条结论，
    /// 两处各写一句的话，同一条匹配在队列里和在标题集合里会给出不同的说法。
    #[must_use]
    pub fn evidence(&self, dump: &str, query_label: &str, query_text: &str) -> String {
        self.evidence_with(dump, query_label, query_text, FUZZY_TAIL)
    }

    /// 同一条匹配的**依据**，但末尾那句改成「**人已经裁决过这一次匹配**」（票 05）。
    ///
    /// 两句尾巴是同一件事的两个态，所以摆在同一个类型上：一条模糊匹配来的结论**永不
    /// 自动通过**，除非人亲口说了它对；而人说过之后，那句「一律进待确认队列」在这一条
    /// 上就不再成立——留着它，半年后读依据的人会以为这一条还等着裁。
    ///
    /// `anchor` 是那条裁决钉在什么上（`verdict::Anchor::describe`），写进依据是为了
    /// 说得出「换台机器还认不认得出」。
    #[must_use]
    pub fn evidence_confirmed(
        &self,
        dump: &str,
        query_label: &str,
        query_text: &str,
        anchor: &str,
    ) -> String {
        self.evidence_with(
            dump,
            query_label,
            query_text,
            &format!(
                "。{CONFIRMED_MARK}（锚是{anchor}）：这一次匹配带来的**全部字段**\
                 ——中文名、别名、类型、简介、开发商、发行商——由这**一条**裁决一并定下，\
                 不再进待确认队列"
            ),
        )
    }

    fn evidence_with(
        &self,
        dump: &str,
        query_label: &str,
        query_text: &str,
        tail: &str,
    ) -> String {
        let mut text = format!(
            "中文离线数据源（Bangumi 离线 dump {dump}）{ENTRY_MARK}{} 「{}」的{}「{}」，\
             与文件名剥出来的{}「{}」相似度 {:.2}",
            self.entry.id,
            self.entry.shown(),
            self.kind.label(),
            self.matched,
            query_label,
            query_text,
            self.score,
        );
        text.push_str(&format!(
            "；平台交叉校验{}（条目写的是「{}」）",
            self.platform.label(),
            if self.entry.platform_text.is_empty() {
                "没写"
            } else {
                self.entry.platform_text.as_str()
            },
        ));
        text.push_str(&format!(
            "；年份交叉校验{}（条目写的是 {}）",
            self.year.label(),
            self.entry
                .year
                .map_or_else(|| "没写".to_string(), |year| year.to_string()),
        ));
        if !self.entry.name_cn.trim().is_empty() {
            text.push_str(&format!("；这条条目的中文名是「{}」", self.entry.name_cn));
        }
        text.push_str(tail);
        text
    }
}

/// **条目号**在一条依据里写成什么样。
///
/// 队列要**看得出哪几个字段来自同一次匹配**（票 05），而判据只能是条目号——中文名、
/// 别名挂在变体上，类型、简介、开发商、发行商挂在作品上，四处的值里没有任何一样是共通的，
/// 共通的只有它们各自的**依据**里那个号。
///
/// **按记号找而不是按位置找**，与 `scrape::zh::TRUNCATED_MARK` 同一条道理：依据这句话
/// 是会改的，改完之后按位置切出来的东西会悄悄变成别的字。
pub const ENTRY_MARK: &str = "的条目 ";

/// 一条依据里说「人已经裁决过这一次匹配」时写的那个记号。
///
/// 队列靠它把**已经定下的**与**还等着裁的**分开——两者的下一步完全不同。
pub const CONFIRMED_MARK: &str = "这一次匹配**由人工裁决确认过**";

/// 依据的最后一句：这一档在**置信度**上算什么。
///
/// 它与 [`Match::evidence_confirmed`] 那一句是同一个位置上的两个态，摆在一起是为了
/// 让「裁决过了还写着一律进队列」这种自相矛盾没地方长出来。
const FUZZY_TAIL: &str = "。**这是模糊匹配不是命中**：它只看名字，没看这个文件里的一个字节，\
     所以永不自动通过，一律进待确认队列（ADR-0002）";

/// 从一条**依据**里认回**条目号**；不是这个源写的那句话就是 `None`。
///
/// 与 [`Match::evidence`] 摆在同一个文件里，因为它们是同一条约定的两头：写的那一侧改了
/// 格式，读的这一侧当场就该跟着改。一条单元测试钉着「写出去的认得回来」。
#[must_use]
pub fn entry_in(evidence: &str) -> Option<u32> {
    let at = evidence.find(ENTRY_MARK)? + ENTRY_MARK.len();
    let digits: String = evidence[at..].chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// 这条依据说的是「人已经裁决过这一次匹配」吗。
#[must_use]
pub fn is_confirmed(evidence: &str) -> bool {
    evidence.contains(CONFIRMED_MARK)
}

/// 索引里的一条叫法。
#[derive(Debug, Clone)]
struct Name {
    entry: u32,
    kind: NameKind,
    value: String,
    key: String,
}

/// **建索引那一刻，平台折叠实际折出来的那张表。**
///
/// 建索引时每条条目的平台名都要折成本工具的平台名（`sync::entry_of` →
/// [`sync::platform_of`]：先问平台清单再问剥离规则的别名表），
/// 折出来的那一串决定交叉校验，进而决定这个源说不说得出话。**折叠发生在建索引那一刻**，
/// 用的是那一刻的平台清单与别名表——所以这张表记的是「这份索引是怎么建出来的」，
/// 不是「现在那两张表长什么样」。
///
/// 它进[`Index::platform_fold`]，再进刮削那一侧两层锚点的**输入指纹**：补一条平台别名
/// 之后重建索引，新折得动的那些条目该重采一遍，而不是被缓存一口咬定「输入没变」
/// 而整片跳过。
///
/// ## 只记**真折出来了**的那些
///
/// 折不动的原文一个都不记。两条理由：
///
/// - **够用**。同一份 dump 上，「折出来的那些对」一样就等于每条条目的平台一模一样：
///   折叠是原文的一个函数，某个原文在这一版折得出 `NDS`、在那一版折不出，两版的表必然
///   不一样。反过来也一样。
/// - **有界**。记折不动的那些等于把 dump 里那份用户随手写的平台词表整个抄进来；
///   只记折得动的，条数被平台清单与别名表本身框住（真机上百来条）。
///
/// 于是「平台清单里加了一个这份 dump 里根本没人写的平台」不改变这张表，那种无关改动
/// **不引发全片重采**——盖的是折叠真发生了什么，不是那两张表长什么样。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlatformFold {
    pairs: std::collections::BTreeSet<(String, String)>,
}

impl PlatformFold {
    /// 记一次：数据源写的 `raw` 这一版折成了本工具的 `platform`。
    ///
    /// 去重且按序——同一份 dump 建两遍，写出来的那一行必须一模一样。
    pub fn record(&mut self, raw: &str, platform: &str) {
        self.pairs.insert((raw.to_string(), platform.to_string()));
    }

    /// 折出来了几对。
    #[must_use]
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    /// 一对都没折出来吗。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// 写成落进 `meta` 的那一行（也就是进指纹的那一串）。
    ///
    /// **头一行是对数，一对都没有时也写。** 于是「这一趟一对都没折出来」写出来是
    /// `共 0 对`，与**老索引根本没记过这件事**（空串）分得开——两者混成一件事的话，
    /// 删掉最后一条别名重建之后，指纹会撞回老索引那一份而整片跳过。
    #[must_use]
    pub fn line(&self) -> String {
        let mut out = format!("共 {} 对", self.pairs.len());
        for (raw, platform) in &self.pairs {
            // 原文与平台名都出自 infobox 里的一行，**换不了行**，所以按行摆是明确的。
            out.push('\n');
            out.push_str(raw);
            out.push('=');
            out.push_str(platform);
        }
        out
    }
}

/// **中文条目索引**：整份装在内存里。
///
/// 装得下是算过的：真机 8.7 万条条目、二十几万条叫法，倒排表按**二元组**打，
/// 键是两个 `char` 拼成的一个 `u64`（不是 `String`——五十万个短字符串的开销比它们
/// 装着的信息还大）。
#[derive(Debug, Clone, Default)]
pub struct Index {
    entries: Vec<Entry>,
    names: Vec<Name>,
    postings: BTreeMap<u64, Vec<u32>>,
    dump: String,
    fields: String,
    platform_fold: String,
}

impl Index {
    /// 从一批条目建一份索引。
    #[must_use]
    pub fn build(entries: Vec<Entry>, dump: String) -> Self {
        let mut names: Vec<Name> = Vec::new();
        for (at, entry) in entries.iter().enumerate() {
            let mut seen: Vec<String> = Vec::new();
            for (kind, value) in entry.names() {
                let key = key_of(value);
                // 太短的键撞不出信息：一个字的名字与谁都像。
                if key.chars().count() < 2 || seen.contains(&key) {
                    continue;
                }
                seen.push(key.clone());
                names.push(Name {
                    entry: u32::try_from(at).unwrap_or(u32::MAX),
                    kind,
                    value: value.to_string(),
                    key,
                });
            }
        }
        let mut postings: BTreeMap<u64, Vec<u32>> = BTreeMap::new();
        for (at, name) in names.iter().enumerate() {
            let at = u32::try_from(at).unwrap_or(u32::MAX);
            let mut grams = grams(&name.key);
            grams.dedup();
            for gram in grams {
                postings.entry(gram).or_default().push(at);
            }
        }
        Self {
            entries,
            names,
            postings,
            dump,
            fields: store::FIELDS.to_string(),
            platform_fold: String::new(),
        }
    }

    /// 记上这一版索引**取了哪几样**（[`store::FIELDS`]）。
    ///
    /// 建索引的那一刻取的当然是本程序这一版取的那几样；从**本机那份库**读回来的就不一定
    /// 了——那份库可能是上一版程序建的。所以读库那一侧要把库里记着的那一行盖回来。
    #[must_use]
    pub fn with_fields(mut self, fields: String) -> Self {
        if !fields.is_empty() {
            self.fields = fields;
        }
        self
    }

    /// 记上这份索引**建的时候把平台折成了什么样**（[`PlatformFold::line`]）。
    ///
    /// **空串照样盖上去**，这一点与 [`Index::with_fields`] 相反，而且不能照抄它：
    /// 「取了哪几样」有一份「本程序这一版取的那几样」可以兜底，而**折叠没有**——
    /// 折叠是建索引那一刻的事，本程序现在这两张表长什么样说明不了那一刻。
    /// 库里没记（老索引）就是**不知道**，如实交出空串比编一个像样的值安全。
    #[must_use]
    pub fn with_platform_fold(mut self, fold: String) -> Self {
        self.platform_fold = fold;
        self
    }

    /// 这份索引是从哪一版 dump 建的。**它进输入指纹**：换一版 dump 就该重跑一遍。
    #[must_use]
    pub fn dump(&self) -> &str {
        &self.dump
    }

    /// 这份索引**从数据源里取了哪几样**。**它也进输入指纹**：改了取哪些字段，
    /// 撞上过的锚点该重采一遍，而不是被缓存一口咬定「输入没变」而整条跳过。
    #[must_use]
    pub fn fields(&self) -> &str {
        &self.fields
    }

    /// 这份索引**建的时候把平台折成了什么样**（[`PlatformFold`]）。
    ///
    /// **它也进输入指纹**，而且 [`Index::dump`] 与 [`Index::fields`] 都盖不住它：
    /// 补一条平台别名再重建，dump 是同一份、取的字段一个没变，变的只有折出来的那一串
    /// ——而那一串正是交叉校验看的东西。空串表示**这份索引没记过这件事**（上一版程序
    /// 建的），不是「一个都没折出来」。
    #[must_use]
    pub fn platform_fold(&self) -> &str {
        &self.platform_fold
    }

    /// 索引里有多少条条目。
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 索引是空的吗。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 全部条目。
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// 撞一次。产出按相似度从高到低，最多 [`Tuning::limit`] 条。
    ///
    /// **两道交叉校验对不上的一条都不产出**——不是降档，是不产出（调研：宁可留空
    /// 也不要写错的中文名）。
    #[must_use]
    pub fn lookup(&self, query: &Query<'_>, tuning: &Tuning) -> Vec<Match> {
        let key = key_of(query.text);
        if !worth_querying(&key) {
            return Vec::new();
        }
        let mut wanted = grams(&key);
        wanted.dedup();
        // 一、圈候选：共享过二元组的那些叫法。**长得离谱的倒排表不参与圈**，
        // 它对「是不是同一个游戏」几乎不提供信息，却要把全库过一遍。
        let mut hits: Vec<u32> = Vec::new();
        for gram in &wanted {
            let Some(posting) = self.postings.get(gram) else {
                continue;
            };
            if posting.len() > tuning.max_postings {
                continue;
            }
            hits.extend_from_slice(posting);
        }
        hits.sort_unstable();
        hits.dedup();
        // 二、逐条算分，同一条条目只留最好的那个叫法。
        let mut best: BTreeMap<u32, (f64, usize)> = BTreeMap::new();
        let digits = alnum_of(&key);
        for at in hits {
            let Some(name) = self.names.get(at as usize) else {
                continue;
            };
            // ⭐ **数字与拉丁字母必须一模一样**，与相似度多高无关（见模块文档）。
            // 续作与版本的区分符几乎总是落在它们身上，而 Dice 对它们天生迟钝。
            if alnum_of(&name.key) != digits {
                continue;
            }
            let score = similarity(&key, &name.key);
            if score < tuning.threshold {
                continue;
            }
            let slot = best.entry(name.entry).or_insert((0.0, at as usize));
            if score > slot.0 {
                *slot = (score, at as usize);
            }
        }
        // 三、交叉校验，然后排序。
        let mut out: Vec<Match> = Vec::new();
        for (entry_at, (score, name_at)) in best {
            let Some(entry) = self.entries.get(entry_at as usize) else {
                continue;
            };
            let name = &self.names[name_at];
            let platform = check_platform(query.platform, &entry.platforms);
            let year = check_year(query.year, entry.year, tuning.year_slack);
            if platform == Check::Conflicts || year == Check::Conflicts {
                continue;
            }
            out.push(Match {
                entry: entry.clone(),
                matched: name.value.clone(),
                kind: name.kind,
                score,
                exact: name.key == key,
                platform,
                year,
            });
        }
        // 分高的在前；同分时**平台对上的**在前，再同就按条目号——同一份索引
        // 跑两次产出的次序必须一样。
        out.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| rank_check(a.platform).cmp(&rank_check(b.platform)))
                .then_with(|| rank_check(a.year).cmp(&rank_check(b.year)))
                .then_with(|| a.entry.id.cmp(&b.entry.id))
        });
        out.truncate(tuning.limit);
        out
    }
}

fn rank_check(check: Check) -> u8 {
    match check {
        Check::Agrees => 0,
        Check::Unknown => 1,
        Check::Conflicts => 2,
    }
}

/// 平台这一道校验。
fn check_platform(query: Option<&str>, entry: &[String]) -> Check {
    let Some(query) = query else {
        return Check::Unknown;
    };
    if entry.is_empty() {
        return Check::Unknown;
    }
    if entry.iter().any(|it| it.eq_ignore_ascii_case(query)) {
        Check::Agrees
    } else {
        Check::Conflicts
    }
}

/// 年份这一道校验。
fn check_year(query: Option<u16>, entry: Option<u16>, slack: u16) -> Check {
    let (Some(query), Some(entry)) = (query, entry) else {
        return Check::Unknown;
    };
    if query.abs_diff(entry) <= slack {
        Check::Agrees
    } else {
        Check::Conflicts
    }
}

/// 这串键值不值得拿去撞。
///
/// **纯数字的一律不撞**：真库里 `1017040110.EDAT` 这类内容 ID 成批出现，
/// 撞出来的只会是噪音。拉丁字母也要够长——`DLC` 三个字母与几百条名字都像。
fn worth_querying(key: &str) -> bool {
    let cjk = key.chars().filter(|c| is_cjk(*c)).count();
    let letters = key.chars().filter(char::is_ascii_alphabetic).count();
    cjk >= 2 || letters >= 4
}

/// 把一个名字折成可比较的键：小写、NFC、全角折半角、只留字母数字与汉字假名。
///
/// 折掉空格与标点是有意的：`塞尔达传说 - 时空之章` 与 `塞尔达传说：时空之章` 是同一个
/// 游戏，而这个库里的名字在这件事上毫无规律。
#[must_use]
pub fn key_of(text: &str) -> String {
    let text = nfc(text);
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        // 全角 ASCII 折成半角：`ＦＩＮＡＬ` 与 `FINAL` 是同一个词。
        let ch = match ch {
            '\u{ff01}'..='\u{ff5e}' => char::from_u32(ch as u32 - 0xfee0).unwrap_or(ch),
            other => other,
        };
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
        }
    }
    out
}

/// 一串键的相邻二元组，按 `u64` 打包（高 32 位是前一个 `char`，低 32 位是后一个）。
///
/// 只有一个字符时，那个字符自己当一个一元组——不然一个字的名字算出来的 Dice 恒为 0。
fn grams(key: &str) -> Vec<u64> {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() < 2 {
        return chars.iter().map(|c| u64::from(*c as u32)).collect();
    }
    let mut out: Vec<u64> = chars
        .windows(2)
        .map(|pair| (u64::from(pair[0] as u32) << 32) | u64::from(pair[1] as u32))
        .collect();
    out.sort_unstable();
    out
}

/// 两串键的 Dice 系数：`2×交集 / (两边之和)`。完全相等直接是 1。
#[must_use]
pub fn similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    let (left, right) = (grams(a), grams(b));
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    // 两边都排好序了，一趟并走数交集（**按重数算**：`ababab` 与 `ab` 不是同一个名字）。
    let (mut i, mut j, mut shared) = (0usize, 0usize, 0usize);
    while i < left.len() && j < right.len() {
        match left[i].cmp(&right[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                shared += 1;
                i += 1;
                j += 1;
            }
        }
    }
    #[allow(clippy::cast_precision_loss)]
    {
        2.0 * shared as f64 / (left.len() + right.len()) as f64
    }
}

/// 一串键里的数字与拉丁字母，按出现顺序接起来。
///
/// **它是一道硬闸**：两边不相等就整条不产出。`第3次超级机器人大战` 与
/// `第4次超级机器人大战` 的相似度是 0.9，`超级机器人大战J` 与 `超级机器人大战R`
/// 是 0.88——而它们各是两个不同的游戏。
#[must_use]
pub fn alnum_of(key: &str) -> String {
    key.chars().filter(char::is_ascii_alphanumeric).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 条目(id: u32, name: &str, cn: &str, year: Option<u16>, platform: &str) -> Entry {
        Entry {
            id,
            name: name.to_string(),
            name_cn: cn.to_string(),
            aliases: Vec::new(),
            year,
            platforms: if platform.is_empty() {
                Vec::new()
            } else {
                vec![platform.to_string()]
            },
            platform_text: platform.to_string(),
            ..Entry::default()
        }
    }

    fn 索引() -> Index {
        Index::build(
            vec![
                条目(4, "メタルスラッグ7", "合金弹头7", Some(2008), "NDS"),
                条目(100, "Solar Jetman", "", Some(1990), "FC"),
                条目(101, "Solar 2", "", Some(2011), "PC"),
                条目(
                    200,
                    "ポケットモンスター ファイアレッド",
                    "口袋妖怪 火红",
                    Some(2004),
                    "GBA",
                ),
            ],
            "dump-2026-09-01".to_string(),
        )
    }

    fn 撞(text: &str, platform: Option<&str>, year: Option<u16>) -> Vec<Match> {
        索引().lookup(
            &Query {
                text,
                platform,
                year,
            },
            &Tuning::default(),
        )
    }

    #[test]
    fn 中文名撞得上() {
        let found = 撞("合金弹头7", Some("NDS"), None);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].entry.id, 4);
        assert_eq!(found[0].kind, NameKind::Chinese);
        assert!((found[0].score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn 调研那个错配的反例落选() {
        // 调研实测：Bangumi 的搜索会把 `Solar Jetman` 错配成 `Solar 2`。
        // 这套算法下它自己就落选了——两串键的 Dice 是 0.53，够不到 0.72。
        let score = similarity(&key_of("Solar Jetman"), &key_of("Solar 2"));
        assert!(score < 0.72, "{score}");
        let found = 撞("Solar Jetman", Some("FC"), None);
        assert_eq!(found.len(), 1, "只该撞上它自己");
        assert_eq!(found[0].entry.id, 100);
    }

    #[test]
    fn 平台对不上的整条不产出() {
        // 不是降档是不产出：宁可留空也不要写错的中文名（调研 §13.1）。
        assert!(撞("合金弹头7", Some("GBA"), None).is_empty());
        // 说不出平台时照样产出，只是校验那一栏是「说不出」。
        let found = 撞("合金弹头7", None, None);
        assert_eq!(found[0].platform, Check::Unknown);
    }

    #[test]
    fn 年份对不上的整条不产出而说不出的照产出() {
        assert!(撞("合金弹头7", Some("NDS"), Some(1998)).is_empty());
        // 差一年算对得上（调研给的三重校验里写的就是 ≤1）。
        let found = 撞("合金弹头7", Some("NDS"), Some(2009));
        assert_eq!(found[0].year, Check::Agrees);
        // 多数文件名写不出年份，那时是「说不出」——照样产出。
        let found = 撞("合金弹头7", Some("NDS"), None);
        assert_eq!(found[0].year, Check::Unknown);
    }

    #[test]
    fn 两道校验都对上够得着中置信() {
        let found = 撞("合金弹头7", Some("NDS"), Some(2008));
        assert!(found[0].strong(&Tuning::default()));
        assert_eq!(found[0].platform, Check::Agrees);
        assert_eq!(found[0].year, Check::Agrees);
    }

    #[test]
    fn 名字一字不差加平台对得上也够得着中置信() {
        // 真库里年份那一侧几乎永远说不出，一刀切要求它对上，这一档就是空的。
        let found = 撞("合金弹头7", Some("NDS"), None);
        assert!(found[0].exact);
        assert_eq!(found[0].year, Check::Unknown);
        assert!(found[0].strong(&Tuning::default()));
    }

    #[test]
    fn 只是像而年份说不出的够不着中置信() {
        // 相似度 0.89：过了 0.85 的门槛，够不着 0.90 那一档，而年份又说不出。
        let found = 撞("合金弹头7代", Some("NDS"), None);
        assert_eq!(found.len(), 1);
        assert!(!found[0].exact);
        assert!(
            found[0].score > 0.85 && found[0].score < 0.9,
            "{}",
            found[0].score
        );
        assert!(!found[0].strong(&Tuning::default()));
    }

    #[test]
    fn 平台对不上时名字一字不差也够不着中置信() {
        // `strong` 的两条路都要平台对得上打底；平台对不上的在 `lookup` 里就没了。
        let one = Match {
            entry: 条目(1, "Foo", "", None, "GBA"),
            matched: "Foo".to_string(),
            kind: NameKind::Original,
            score: 1.0,
            exact: true,
            platform: Check::Unknown,
            year: Check::Unknown,
        };
        assert!(!one.strong(&Tuning::default()));
    }

    #[test]
    fn 数字或字母不一样的整条不产出() {
        // 真机第一趟的主要错法：`第3次` 撞上 `第4次`、`大金刚鼓1` 撞上 `大金刚鼓3`。
        // 二元组的 Dice 对这种差异天生迟钝，所以另设一道硬闸。
        let index = Index::build(
            vec![
                条目(
                    1,
                    "スーパーロボット大戦4",
                    "第4次超级机器人大战",
                    Some(1995),
                    "SFC",
                ),
                条目(2, "ドンキーコンガ3", "大金刚鼓3", Some(2005), "NGC"),
            ],
            "dump".to_string(),
        );
        for (text, platform) in [
            ("第3次超级机器人大战", "SFC"),
            ("大金刚鼓1", "NGC"),
            // 字母那一侧：`超级机器人大战J` 与 `超级机器人大战R` 相似度 0.88。
            ("第4次超级机器人大战J", "SFC"),
        ] {
            let found = index.lookup(
                &Query {
                    text,
                    platform: Some(platform),
                    year: None,
                },
                &Tuning::default(),
            );
            assert!(found.is_empty(), "{text} 不该撞上");
        }
        // 数字一样的照撞不误。
        assert_eq!(alnum_of("游戏王5ds卡片力量4"), "5ds4");
        assert_eq!(alnum_of("蚊2"), "2");
        assert_eq!(alnum_of("超级马里奥银河"), "");
    }

    #[test]
    fn 差一两个字的同系列条目落选() {
        // 这五条都是真机第一趟（门槛 0.72）实际撞出来的错配。
        let index = Index::build(
            vec![
                条目(
                    1,
                    "ポケットモンスター 緑",
                    "精灵宝可梦 绿",
                    Some(1996),
                    "GB",
                ),
                条目(
                    2,
                    "スーパーロボット大戦DD",
                    "超级机器人大战DD",
                    Some(2019),
                    "GB",
                ),
                条目(3, "ワンピース 王冠", "海贼王冠", Some(2001), "GBC"),
                条目(4, "雪の少女", "我与雪之少女", Some(2000), "PSP"),
            ],
            "dump".to_string(),
        );
        for (text, platform) in [
            ("口袋妖怪金", "GB"),
            ("超级机器人大战LB", "GB"),
            ("海贼王", "GBC"),
            ("雪之少女", "PSP"),
        ] {
            let found = index.lookup(
                &Query {
                    text,
                    platform: Some(platform),
                    year: None,
                },
                &Tuning::default(),
            );
            assert!(found.is_empty(), "{text} 撞出了 {found:?}");
        }
    }

    #[test]
    fn 标点与空格不影响撞不撞得上() {
        // `口袋妖怪 火红` 与 `口袋妖怪火红` 是同一个游戏，而库里的名字在这件事上没规律。
        let found = 撞("口袋妖怪火红", Some("GBA"), None);
        assert_eq!(found[0].entry.id, 200);
    }

    #[test]
    fn 纯数字与太短的名字不拿去撞() {
        // `1017040110.EDAT` 这类内容 ID 真库里成批出现，撞出来的只会是噪音。
        assert!(撞("1017040110", Some("PSP"), None).is_empty());
        assert!(撞("DLC", Some("PSP"), None).is_empty());
        assert!(撞("冰球", Some("FC"), None).is_empty());
    }

    #[test]
    fn 全角折成半角() {
        assert_eq!(key_of("ＦＩＮＡＬ　ＦＡＮＴＡＳＹ"), "finalfantasy");
    }

    #[test]
    fn 同一条条目只留最好的那个叫法() {
        let index = Index::build(
            vec![Entry {
                id: 1,
                name: "Metal Slug 7".to_string(),
                name_cn: "合金弹头7".to_string(),
                aliases: vec!["合金彈頭7".to_string()],
                year: Some(2008),
                platforms: vec!["NDS".to_string()],
                platform_text: "NDS".to_string(),
                ..Entry::default()
            }],
            "dump".to_string(),
        );
        let found = index.lookup(
            &Query {
                text: "合金弹头7",
                platform: None,
                year: None,
            },
            &Tuning::default(),
        );
        assert_eq!(found.len(), 1, "一条条目只该出一次");
        assert_eq!(found[0].kind, NameKind::Chinese);
    }

    #[test]
    fn 依据说得出条目号平台与年份两道校验() {
        let found = 撞("合金弹头7", Some("NDS"), Some(2008));
        let text = found[0].evidence("dump-2026-09-01", "正题", "合金弹头7");
        assert!(text.contains("条目 4"), "{text}");
        assert!(text.contains("平台交叉校验对得上"), "{text}");
        assert!(text.contains("年份交叉校验对得上"), "{text}");
        assert!(text.contains("永不自动通过"), "{text}");
        // **写出去的号认得回来**：队列按条目号把「同一次匹配带来的字段」归堆（票 05），
        // 而依据这句话是会改的——两头摆在同一个文件里，这条钉着它们不许各走各的。
        assert_eq!(entry_in(&text), Some(4));
        assert!(!is_confirmed(&text), "没人裁过的那一档不该说已确认");
    }

    #[test]
    fn 裁决过的那一条依据里不再说一律进待确认队列() {
        // 票 05：人说过「就是这条」之后，那句「一律进待确认队列」在这一条上就不再成立。
        // 留着它，半年后读依据的人会以为这一条还等着裁。
        let found = 撞("合金弹头7", Some("NDS"), Some(2008));
        let text = found[0].evidence_confirmed(
            "dump-2026-09-01",
            "正题",
            "合金弹头7",
            "CRC-32 1234ABCD + 4096 字节",
        );
        assert!(is_confirmed(&text), "{text}");
        assert!(!text.contains("一律进待确认队列"), "{text}");
        assert!(text.contains("CRC-32 1234ABCD"), "锚要写进依据：{text}");
        // 条目号那一半一个字都没变——归堆的判据两档共用。
        assert_eq!(entry_in(&text), Some(4));
    }

    #[test]
    fn 不是这个源写的那句话认不出条目号() {
        assert_eq!(entry_in("TOSEC 的条目名里第一个括号是发行日期"), None);
        assert_eq!(entry_in(""), None);
    }

    #[test]
    fn 折出来的那张表去重且与记录次序无关() {
        // 同一份 dump 建两遍写出来的那一行必须一模一样——它进输入指纹，
        // 次序稍有不同就会被当成「输入变了」而整片重采。
        let mut 甲 = PlatformFold::default();
        甲.record("Nintendo DS", "NDS");
        甲.record("GBA", "GBA");
        甲.record("Nintendo DS", "NDS");
        let mut 乙 = PlatformFold::default();
        乙.record("GBA", "GBA");
        乙.record("Nintendo DS", "NDS");
        assert_eq!(甲.line(), 乙.line());
        assert_eq!(甲.len(), 2, "重复那一次不另算一对");
        assert_eq!(甲.line(), "共 2 对\nGBA=GBA\nNintendo DS=NDS");
    }

    #[test]
    fn 一对都没折出来与老索引没记过这件事分得开() {
        // 两者混成一件事的话，删掉最后一条别名重建之后，指纹会撞回老索引那一份
        // 而整片跳过——那正是这一层要拦的事。
        let 空的 = PlatformFold::default();
        assert!(空的.is_empty());
        assert_eq!(空的.line(), "共 0 对");
        assert_ne!(空的.line(), "", "老索引读回来才是空串");
    }

    #[test]
    fn 折得动的那一串变了折出来的那张表就跟着变() {
        // 用户的原话：补一条平台别名之后重建索引。折不动的那一版与折得动的那一版
        // 必须写出两行不同的字。
        let 折不动 = PlatformFold::default();
        let mut 折得动 = PlatformFold::default();
        折得动.record("Nintendo DS", "NDS");
        assert_ne!(折不动.line(), 折得动.line());
    }

    #[test]
    fn 索引把建的时候折成什么样带在身上() {
        let index = 索引().with_platform_fold("共 1 对\nNDS=NDS".to_string());
        assert_eq!(index.platform_fold(), "共 1 对\nNDS=NDS");
        // **空串照样盖得上去**：库里没记过就是不知道，这一点与 `with_fields` 相反
        // ——那一边有「本程序这一版取的那几样」可以兜底，折叠没有。
        assert_eq!(索引().with_platform_fold(String::new()).platform_fold(), "");
        assert_eq!(索引().fields(), store::FIELDS, "字段那一行照旧兜得住底");
    }
}
