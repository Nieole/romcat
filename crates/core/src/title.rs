//! **标题集合**、**显示标题**与**排序标题**。
//!
//! 中立库里标题永远是**集合**：一个作品的全部叫法，每条带语言、地区、来源与类型
//! （`CONTEXT.md`）。导出到某个前端时才从集合里挑出那**一个** [`显示标题`](choose)，
//! 另外生成一个 [`排序标题`](Chosen::sort)。这个模块做的就是这三件事：**折**出集合、
//! **挑**出显示标题、**另行生成**排序标题。
//!
//! ## 三处特别容易做错的地方
//!
//! ### 一、标题不是一个字段
//!
//! 塞成单值字段，第一件事就是要在**写的时候**挑一个，而挑哪个取决于导出到哪个前端。
//! 挑早了，跨格式转换与按中文搜索这两件事就都做不了了。所以集合落库
//! （[`catalog::title`](crate::catalog::title)），而显示标题与排序标题**不落库**——
//! 它们是集合的纯函数，换一份优先级表就该跟着变。
//!
//! ### 二、显示标题的来源与**首选变体**解耦（ADR-0012）
//!
//! **即使默认启动的是民间汉化版，中文标题仍取官中版的官方译名。** 这两条链路必须分开：
//! 首选变体那条规则是「汉化 > 官中 > 日版 > 其他」，标题这条是「官中的官方译名 >
//! 社区译名」——方向正好相反。顺手写成「首选变体的名字就是显示标题」是错的，
//! 那会把汉化组自取的名字铺满整个前端。
//!
//! 落到代码上：[`choose`] 拿到的只有[标题集合](TitleSet)，**它连变体是不是首选都看不到**。
//! 汉化版的名字照样在集合里（类型是**汉化组自取的名**），只是排在中文那一档的最后一位。
//!
//! ### 三、世代裂缝（ADR-0019）
//!
//! 中文译名**归到哪一条发行版**上，两个世代的答案不一样，而**这道裂缝是现实的裂缝，
//! 不是模型的缺陷**：
//!
//! | 世代 | 官中是什么 | 中文名归给谁 | 这里叫 |
//! |---|---|---|---|
//! | 卡带与光盘（有独立序列号） | 一条**独立的发行版**，与日版、美版平级 | 那一条官中发行版自己 | [`Seam::OwnRelease`] |
//! | 数字（Switch 港服与美服 69.4% 共用 TitleID） | 同一发行版的**语言属性** | 那条**多地区共用**的发行版 | [`Seam::LanguageField`] |
//!
//! **分叉的是归属，不是取字的那口井。** 两侧的中文名都只能从**盘上那个文件的名字**里
//! 来，因为 **DAT 里根本没有中文**：No-Intro 给台版卡带的条目名是拉丁字母的
//! `Pokemon 4-in-1 (China) (En,Zh) (Pirate)`，官方译名「口袋妖怪四合一」只写在用户那个
//! zip 的名字上。把两侧写成两段不同的取字代码，只会得到两段一模一样的代码。
//!
//! 两侧的判据是**发行版自己的身份**，不是一张世代表——这个库里没有世代这一列，也不该
//! 为它造一列：地区是中国 / 台湾 / 香港，说明这条发行版就是官中那一条；地区是别的
//! （亚洲、美国、欧洲）而语言里带 `Zh`，说明它是一条多地区共用的记录、中文只是它的一项
//! 属性。真库两侧都有（FC 与 GBC 的台版卡带 vs PSV 港版与美版共用的那些记录），
//! 任何一侧被抹平都会在那一侧撒谎。
//!
//! ## 排序标题必须独立生成
//!
//! 中文显示标题按 Unicode 码位排等于乱排——码位顺序对读者是随机的。所以排序标题另取：
//! **优先用同一部作品的拉丁字母标题**（官方英文名 / 日版罗马字名 / 作品名），
//! 那是 `CONTEXT.md` 给的两个选项之一。一个拉丁标题都没有的作品，报告会点名——
//! 那是要人工补的，不是悄悄按码位排掉。

pub mod report;

use std::collections::{BTreeMap, BTreeSet};

use crate::catalog::identify::Confidence;
use crate::catalog::title::TitleRow;
use crate::catalog::{Catalog, CatalogError, ReleaseRow};
use crate::dat::chinese::ChineseMark;
use crate::identify::naming;
use crate::scrape::priority::Priorities;
use crate::scrape::{AnchorKind, Field};

pub use report::TitleReport;

/// 一条叫法的语言。
///
/// 只分得出这四档，而这正是[显示标题的回退链](choose)要的四档：中文、官方英文名、
/// 日文原名、其余。**再细分不是这一层能做到的**——No-Intro 的欧版条目名是拉丁字母，
/// 分不出那是英文、法文还是德文，硬猜只会得到错的元数据。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Language {
    /// 中文。
    Chinese,
    /// 日文（含罗马字转写的日版条目名）。
    Japanese,
    /// 拉丁字母写的名字，且不是日版的。
    English,
    /// 认不出。**不猜**——认不出的名字照样入集合，只是排在回退链的后面。
    Unknown,
}

impl Language {
    /// 存进库的那个码。
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Chinese => "zh",
            Self::Japanese => "ja",
            Self::English => "en",
            Self::Unknown => "und",
        }
    }

    /// 打给用户的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Chinese => "中文",
            Self::Japanese => "日文",
            Self::English => "英文",
            Self::Unknown => "认不出",
        }
    }

    /// 从码认回来。
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "zh" => Some(Self::Chinese),
            "ja" => Some(Self::Japanese),
            "en" => Some(Self::English),
            "und" => Some(Self::Unknown),
            _ => None,
        }
    }

    /// 报告里固定的排列顺序。
    #[must_use]
    pub fn all() -> [Self; 4] {
        [Self::Chinese, Self::Japanese, Self::English, Self::Unknown]
    }
}

/// 一条叫法是**哪一种**叫法。
///
/// 这一列是 ADR-0012 那条「官中的官方译名 > 社区译名」的落点：两者都是中文，
/// 差别全在这里。塌成一个「中文名」，那条决定就无处可写。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TitleKind {
    /// **官方名**：原厂在那次发行上用的名字，DAT 的条目名就是它。
    Official,
    /// **译名**：原厂的官方翻译。中文这一侧就是**官中版**的官方译名。
    Translated,
    /// **别名**：既不是官方名也不是译名的一个叫法——库里那些中文文件名多半是这一档，
    /// 谁也没为它背书。
    Alias,
    /// **汉化组自取的名**：民间汉化版自己起的名字。ADR-0012 明说**绝不拿它当标题**，
    /// 所以它排在中文那一档的最后——但它照样入集合，因为没有别的中文名时它就是唯一的。
    FanName,
}

impl TitleKind {
    /// 存进库、也打给用户的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Official => "官方名",
            Self::Translated => "译名",
            Self::Alias => "别名",
            Self::FanName => "汉化组自取的名",
        }
    }

    /// 从词认回来。
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "官方名" => Some(Self::Official),
            "译名" => Some(Self::Translated),
            "别名" => Some(Self::Alias),
            "汉化组自取的名" => Some(Self::FanName),
            _ => None,
        }
    }

    /// 报告里固定的排列顺序。
    #[must_use]
    pub fn all() -> [Self; 4] {
        [Self::Official, Self::Translated, Self::Alias, Self::FanName]
    }
}

/// 中文译名走的是**世代裂缝**的哪一侧（ADR-0019）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Seam {
    /// **独立发行版**：卡带与光盘世代，官中有自己的地区与序列号，在 DAT 里是独立一条。
    /// 中文名归到**那一条发行版**上，它与日版、美版平级。
    OwnRelease,
    /// **语言属性**：数字世代，港服与美服共用同一个 TitleID，同一条记录服务多个地区。
    /// 中文名归到**那条共用的发行版**上，中文只是它语言标记组里的一项。
    LanguageField,
}

impl Seam {
    /// 存进库、也打给用户的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::OwnRelease => "独立发行版",
            Self::LanguageField => "语言属性",
        }
    }

    /// 从词认回来。
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "独立发行版" => Some(Self::OwnRelease),
            "语言属性" => Some(Self::LanguageField),
            _ => None,
        }
    }
}

/// **官中版**的地区叫什么。地区是中国 / 台湾 / 香港的那条发行版，就是官中那一条本身。
///
/// `Asia` **不在这里**，这是关键的一刀：PSV 的港服中文版在 Redump 里地区就是 `Asia`，
/// 它与美版共用同一条记录——中文在那里是语言属性，不是一条独立的发行版（ADR-0019）。
const CHINESE_REGIONS: &[&str] = &["China", "Taiwan", "Hong Kong"];

/// 一个作品的**标题集合**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleSet {
    /// 作品名。它同时是兜底——一个叫法都没有时，显示标题就是它。
    pub work: String,
    /// 全部叫法。
    pub entries: Vec<TitleRow>,
}

/// 排序标题是从哪儿来的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortFrom {
    /// 显示标题本身就排得动（拉丁字母）。
    Display,
    /// 显示标题排不动，改用集合里的拉丁字母标题。
    LatinTitle,
    /// 集合里也没有拉丁标题，用作品名。
    WorkName,
    /// **一个拉丁标题都没有**。这一档要报出来——排序键只能退回显示标题，
    /// 而那正是「按码位排等于乱排」的那一档，得人工补一个。
    None,
}

impl SortFrom {
    /// 打给用户的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Display => "显示标题",
            Self::LatinTitle => "集合里的拉丁标题",
            Self::WorkName => "作品名",
            Self::None => "没有拉丁标题",
        }
    }
}

/// 从一个标题集合里挑出来的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen {
    /// **显示标题**：导出时实际写进去的那一个。
    pub display: String,
    /// 它是哪种语言。
    pub language: Language,
    /// 它是哪一种叫法；集合是空的、退回作品名时是 `None`。
    pub kind: Option<TitleKind>,
    /// 它的置信度；退回作品名时是 `None`。
    pub confidence: Option<Confidence>,
    /// 中文译名走的是世代裂缝的哪一侧。
    pub seam: Option<Seam>,
    /// **依据**。
    pub evidence: String,
    /// **排序标题**：与显示标题分开生成。
    pub sort: String,
    /// 排序标题从哪儿来。
    pub sort_from: SortFrom,
    /// 这个作品一共有几个**中文**叫法。大于一时，选定规则真的起了作用。
    pub chinese_names: usize,
}

/// 挑出**显示标题**，并另行生成**排序标题**。
///
/// ## 回退链
///
/// 按「中文 > 官方英文名 > 日文原名 > 文件名」回退，而中文那一档内部再按类型分先后
/// （ADR-0012：官中的官方译名 > 社区译名）：
///
/// | 名次 | 语言 | 类型 | 说明 |
/// |---|---|---|---|
/// | 0 | 任意 | 任意 | **裁决**——人改过的东西不许被任何数据源覆盖 |
/// | 1 | 中文 | 译名 | **官中版的官方译名**。两侧世代路径都落在这里 |
/// | 2 | 中文 | 官方名 | 条目名本身就是中文的那些 |
/// | 3 | 中文 | 别名 | 库里的中文文件名。谁也没背书，所以**低置信**、进队列 |
/// | 4 | 中文 | 汉化组自取的名 | ADR-0012 明说别拿它当标题，所以排在中文的最后 |
/// | 5 | 英文 | 官方名 | **官方英文名** |
/// | 6 | 日文 | 官方名 | **日文原名**（No-Intro 的日版条目名是它的罗马字转写） |
/// | 7 | 其余 | 别名 / 汉化组自取的名 | **文件名**兜底 |
///
/// ## 同一部作品有好几个叫法时挑哪一个
///
/// **确定的选定规则**，[六层](rank)，缺一不可。这一段是那条规则的**唯一出处**——
/// 报告与文档都从这里抄，抄岔了两处就会各说一套：
///
/// 1. **人工裁决**——人改过的东西不许被任何数据源覆盖；
/// 2. **语言与类型的档位**——就是上面那张表；
/// 3. **置信度**——官中那一条压过没人背书的文件名；
/// 4. **优先级表里的源名次**（`标题` 那一栏）——已经有一份配置好的表，不另发明一套。
///    **它排在数量之前**：`priorities.toml` 说 No-Intro 的条目名最整齐，那就不该被
///    「同一个 TOSEC 写法在盘上重复了三十遍」压过去；
/// 5. **有几个变体这么叫**，多的优先——同一档、同一个源之内，几十个文件都这么写的
///    那个比孤零零一个的可信；
/// 6. **字典序**——前五层平手时定死顺序，同一份库跑两次挑出来的必须是同一条。
#[must_use]
pub fn choose(set: &TitleSet, priorities: &Priorities) -> Chosen {
    let best = set
        .entries
        .iter()
        .min_by_key(|entry| rank(entry, priorities));
    let chinese_names = set
        .entries
        .iter()
        .filter(|entry| entry.language == Language::Chinese)
        .map(|entry| entry.value.as_str())
        .collect::<BTreeSet<_>>()
        .len();

    let Some(best) = best else {
        // 一个叫法都没有：退回作品名。它永远有——识别从 DAT 条目名折出来的那个。
        return Chosen {
            display: set.work.clone(),
            language: language_of(&set.work, None),
            kind: None,
            confidence: None,
            seam: None,
            evidence: "标题集合是空的，退回作品名".to_string(),
            sort: sort_title(&set.work),
            sort_from: if sortable(&set.work) {
                SortFrom::Display
            } else {
                SortFrom::None
            },
            chinese_names: 0,
        };
    };

    // **排序标题独立生成**：显示标题排得动就用它，排不动就另找一个拉丁标题。
    let (sort, sort_from) = if sortable(&best.value) {
        (sort_title(&best.value), SortFrom::Display)
    } else if let Some(latin) = set
        .entries
        .iter()
        .filter(|entry| sortable(&entry.value))
        .min_by_key(|entry| rank(entry, priorities))
    {
        (sort_title(&latin.value), SortFrom::LatinTitle)
    } else if sortable(&set.work) {
        (sort_title(&set.work), SortFrom::WorkName)
    } else {
        // 一个拉丁标题都没有。退回显示标题，**并让报告点名**——按码位排等于乱排，
        // 悄悄排掉比排不动更糟。
        (sort_title(&best.value), SortFrom::None)
    };

    Chosen {
        display: best.value.clone(),
        language: best.language,
        kind: Some(best.kind),
        confidence: Some(best.confidence),
        seam: best.seam,
        evidence: best.evidence.clone(),
        sort,
        sort_from,
        chinese_names,
    }
}

/// 一条叫法在回退链上排第几。数字小的排前面。六层的含义见 [`choose`]。
fn rank(entry: &TitleRow, priorities: &Priorities) -> Rank {
    let bucket = match (entry.language, entry.kind) {
        (Language::Chinese, TitleKind::Translated) => 1,
        (Language::Chinese, TitleKind::Official) => 2,
        (Language::Chinese, TitleKind::Alias) => 3,
        (Language::Chinese, TitleKind::FanName) => 4,
        (Language::English, TitleKind::Official | TitleKind::Translated) => 5,
        (Language::Japanese, TitleKind::Official | TitleKind::Translated) => 6,
        _ => 7,
    };
    (
        // **人工来源永远排最前**（`priorities.toml` 的规则一）。它压过语言那一档：
        // 人指名要哪个标题，就该是哪个，哪怕他挑的是英文名。
        u8::from(!entry.is_verdict()),
        bucket,
        match entry.confidence {
            Confidence::High => 0,
            Confidence::Medium => 1,
            Confidence::Low => 2,
        },
        // 平台传 `None`：一个作品可以横跨几个平台，按哪个平台取覆盖都不对。
        // 按平台的覆盖是导出到某个平台的子库时才用得上的，那是票 18 的活。
        priorities.rank(Field::Title.label(), None, &entry.source),
        std::cmp::Reverse(entry.seen),
        entry.value.clone(),
    )
}

/// [`rank`] 的排序键。六层，含义见 [`choose`]。
///
/// 起个名字而不是把六元组摊在签名里：`choose` 有两处按它排序（挑显示标题、挑排序标题
/// 用的拉丁标题），两处的类型必须一模一样。
type Rank = (u8, u8, u8, usize, std::cmp::Reverse<u64>, String);

/// 折出一个**排序标题**：去掉两头的空白、把中间的空白压成一个、折成大写。
///
/// **只做这三样**。搬走冠词（`The Legend of Zelda` → `Legend of Zelda, The`）是另一种
/// 口味，不是所有前端都这么排，挂在 D61。
#[must_use]
pub fn sort_title(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

/// 这一串字排得动吗——也就是**没有汉字与假名**。
///
/// 排得动的判据是「按码位排出来的顺序对读者说得通」。拉丁字母说得通，汉字不说得通：
/// `三國志` 的码位在 `爆破彗星` 前面，而按拼音、按官方英文名都在后面。
#[must_use]
pub fn sortable(title: &str) -> bool {
    !title.chars().any(|c| is_han(c) || is_kana(c))
}

fn is_han(c: char) -> bool {
    matches!(c,
        '\u{3400}'..='\u{4DBF}'
        | '\u{4E00}'..='\u{9FFF}'
        | '\u{F900}'..='\u{FAFF}'
        | '\u{20000}'..='\u{2FA1F}')
}

fn is_kana(c: char) -> bool {
    matches!(c,
        '\u{3040}'..='\u{30FF}' | '\u{31F0}'..='\u{31FF}' | '\u{FF66}'..='\u{FF9D}')
}

/// 这一串字是什么语言：**先看字形，再让发行版的地区纠正**。
///
/// 地区那一道纠正不是装饰，它管着回退链里「官方英文名」与「日文原名」的分野：
/// No-Intro 的日版条目名是**罗马字**（`Gensou Suikoden`），字形上与英文名一模一样，
/// 只有那一条发行版的地区说得出它其实是日文原名。汉字同理——日版条目名里的汉字
/// 是日文不是中文。
#[must_use]
pub fn language_of(title: &str, region: Option<&str>) -> Language {
    let japanese = region.is_some_and(|region| region.eq_ignore_ascii_case("Japan"));
    if title.chars().any(is_kana) {
        return Language::Japanese;
    }
    if title.chars().any(is_han) {
        return if japanese {
            Language::Japanese
        } else {
            Language::Chinese
        };
    }
    if title.chars().any(|c| c.is_ascii_alphabetic()) {
        return if japanese {
            Language::Japanese
        } else {
            Language::English
        };
    }
    Language::Unknown
}

/// 把中立库里的识别与刮削结论**折**成标题集合。
///
/// 值来自[刮削](crate::scrape)（DAT 条目名、文件名），而语言、地区与类型来自
/// [识别](crate::identify)（发行版的地区与语言、候选上的中文记号）——**识别给身份，
/// 刮削给值**，这一层只是把两边对起来。
///
/// # Errors
/// 读库失败时返回错误。
pub fn fold(catalog: &Catalog) -> Result<Vec<TitleRow>, CatalogError> {
    let works = catalog.work_names()?;
    let releases = catalog.releases()?;
    let variants = catalog.variants()?;

    // 一次扫过自动通过的候选，攒两样：变体上的中文记号，以及每条发行版的官方条目名。
    let mut marks: BTreeMap<String, BTreeSet<ChineseMark>> = BTreeMap::new();
    let mut official: BTreeMap<i64, BTreeSet<(String, String)>> = BTreeMap::new();
    catalog.for_each_accepted_candidate(&mut |candidate| {
        if let Some(mark) = candidate.chinese {
            marks
                .entry(candidate.variant_key.to_string())
                .or_default()
                .insert(mark);
        }
        if let Some(release) = candidate.release_id {
            official
                .entry(release)
                .or_default()
                .insert((candidate.source.to_string(), candidate.game.to_string()));
        }
    })?;

    // 刮削那一侧的标题值，按变体归堆。
    let mut harvested: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    catalog.for_each_scraped_value(&mut |anchor, subject, value| {
        if anchor == AnchorKind::Variant.label() && value.field == Field::Title.label() {
            harvested
                .entry(subject.to_string())
                .or_default()
                .push((value.source, value.value));
        }
    })?;

    let mut tally = Tally::default();

    // 一、发行版一侧：**官方名**。一条 DAT 条目就是一次官方发行，它的名字就是官方名。
    for (id, release) in &releases {
        let Some(work) = works.get(&release.work_id) else {
            continue;
        };
        let Some(names) = official.get(id) else {
            continue;
        };
        for (source, game) in names {
            let value = naming::work_title(game);
            if value.trim().is_empty() {
                continue;
            }
            tally.add(TitleRow {
                work: work.clone(),
                language: language_of(&value, release.region.as_deref()),
                kind: TitleKind::Official,
                source: source.clone(),
                value,
                region: release.region.clone(),
                variant_key: None,
                // 发行版是从**自动通过**的候选建出来的（精确哈希命中），
                // 它的条目名就是那次发行的官方名——没有比这更硬的依据。
                confidence: Confidence::High,
                seam: None,
                evidence: format!(
                    "{source} 的条目「{game}」——{}{}那一条发行版",
                    release.platform.as_deref().unwrap_or("平台不详"),
                    release
                        .region
                        .as_deref()
                        .map_or_else(|| "、地区不详".to_string(), |region| format!("、{region}")),
                ),
                seen: 1,
            });
        }
    }

    // 二、变体一侧：**译名 / 汉化组自取的名 / 别名**。
    //
    // 官中版的官方译名只可能从这儿来：**DAT 里没有中文**——`Pokemon 4-in-1 (China)
    // (En,Zh) (Pirate)` 是那条官中发行版的**英文条目名**，中文名躺在用户盘上那个文件的
    // 名字里（`口袋妖怪四合一`）。所以「取官中版的官方译名」落到实处就是：
    // **取那条官中发行版下面的变体的中文名**。
    for variant in &variants {
        let Some(work) = variant.work_id.and_then(|id| works.get(&id)) else {
            continue;
        };
        let Some(names) = harvested.get(&variant.key) else {
            continue;
        };
        let release = variant.release_id.and_then(|id| releases.get(&id));
        let marks = marks.get(&variant.key);
        let chinese = chinese_release(release, marks);
        for (source, value) in names {
            if value.trim().is_empty() {
                continue;
            }
            // **地区那道纠正只用在官方名上，这里不传。** 变体这一侧的值是**盘上那个文件
            // 的名字**，是用户自己起的：一份日版转储被起名叫「勇者斗恶龙」，那就是个
            // 中文名，发行版的地区说不了它是什么语言。反过来，DAT 的条目名确实跟着那次
            // 发行走，所以上面那一轮传了地区。
            let language = language_of(value, None);
            let told = classify(source, language, marks, chinese, release);
            tally.add(TitleRow {
                work: work.clone(),
                language,
                kind: told.kind,
                source: source.clone(),
                value: value.clone(),
                region: release.and_then(|r| r.region.clone()),
                variant_key: Some(variant.key.clone()),
                confidence: told.confidence,
                seam: told.seam,
                evidence: format!("{source}：变体「{}」——{}", variant.key, told.evidence),
                seen: 1,
            });
        }
    }

    Ok(tally.rows())
}

/// 一条**中文发行版**：它是世代裂缝的哪一侧，以及**是什么说它有中文的**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ChineseRelease {
    seam: Seam,
    /// 依据里那一句：语言标记组，还是候选上的官中记号。
    declared_by: &'static str,
}

/// 这条发行版的中文是**世代裂缝的哪一侧**；它根本不是中文发行版时是 `None`。
///
/// 判据分两步，顺序不能反：
///
/// 1. **这条发行版说自己有中文吗**——语言标记组里有 `Zh`，或者候选上带**官中**记号。
///    两者本来就同源（`dat::chinese::mark_of` 认的正是那个语言标记组），
///    但候选那一侧是逐条的，发行版那一列取的是最佳候选那一条，所以两边都看。
/// 2. **它是官中那一条本身，还是一条顺带带着中文的发行版**——看地区。
///
/// 第二步的**举证责任在「语言属性」那一侧**：说一条发行版的中文只是它的一项属性，
/// 等于说**这同一条记录还服务着别的地区**（港服与美服共用同一个 TitleID），
/// 那要有正面证据——地区读得出来、而且不是中文地区。地区读不出来（TOSEC 的条目名
/// 第一个括号是发行日期，地区在后面，`identify::naming` 保守地留空）就没有这个证据，
/// 那时落回「它就是那条中文发行版」：手上确知的只有「这条记录声明了中文、这个变体基于
/// 它」，而那正是**独立发行版**那一侧的说法。
fn chinese_release(
    release: Option<&ReleaseRow>,
    marks: Option<&BTreeSet<ChineseMark>>,
) -> Option<ChineseRelease> {
    let release = release?;
    let declared_by = if release
        .languages
        .as_deref()
        .is_some_and(has_chinese_language)
    {
        "语言标记组里有中文"
    } else if marks.is_some_and(|marks| marks.contains(&ChineseMark::Official)) {
        "撞上的那条候选带**官中**记号"
    } else {
        return None;
    };
    let shared = release.region.as_deref().is_some_and(|region| {
        !CHINESE_REGIONS
            .iter()
            .any(|known| known.eq_ignore_ascii_case(region))
    });
    Some(ChineseRelease {
        seam: if shared {
            Seam::LanguageField
        } else {
            Seam::OwnRelease
        },
        declared_by,
    })
}

/// 语言标记组里有中文吗。`En,Zh-Hans` 与 `Ja,Zh` 都算。
fn has_chinese_language(languages: &str) -> bool {
    languages.split(',').any(|item| {
        let item = item.trim().to_ascii_lowercase();
        item == "zh" || item.starts_with("zh-")
    })
}

/// 一个变体给出的叫法**是哪一种**。
///
/// 四样一起返回而不是四个位置参数：它们本来就是同一次判断的四个面，
/// 分开返回的话调用处要记住第三个 `Option<Seam>` 是谁（同 [`ChineseRelease`]、
/// [`AcceptedCandidate`](crate::catalog::AcceptedCandidate) 的做法）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Classified {
    kind: TitleKind,
    confidence: Confidence,
    seam: Option<Seam>,
    evidence: String,
}

/// 一个变体给出的叫法是**哪一种**，以及它的置信度与**依据**。
fn classify(
    source: &str,
    language: Language,
    marks: Option<&BTreeSet<ChineseMark>>,
    chinese: Option<ChineseRelease>,
    release: Option<&ReleaseRow>,
) -> Classified {
    // **中文离线源那一条排第一**（票 11）。它与别的值有一处根本不同：**这串字不是盘上
    // 那个文件的名字**，是中文数据源里那条条目的名字，而且平台与年份两道交叉校验都对上。
    //
    // 排第一是必须的，不是图省事：下面第一条按「这个变体撞上了汉化条目」判**汉化组自取
    // 的名**——那句话对文件名成立，对一条来自 wiki 的条目名不成立。让它落到那一档，
    // 一个有出处的中文名会被记成汉化组起的名字，而 ADR-0012 明说别拿那种名字当标题。
    if source == crate::identify::fuzzy::SOURCE {
        return Classified {
            kind: TitleKind::Alias,
            // **中置信**：有出处，但出处是一份用户共同维护的 wiki，不是原厂——
            // 所以它不是**译名**那一档（那一档留给官中版的官方译名，ADR-0012），
            // 而是一条**有人背书的别名**。高置信留给裁决。
            confidence: Confidence::Medium,
            seam: None,
            evidence: "这个名字有出处：中文离线数据源里那条条目的中文名，\
                       平台与年份两道交叉校验都对上（候选那一侧记着完整依据）"
                .to_string(),
        };
    }
    // **汉化版是变体**（ADR-0012）：它的文件名是汉化组自取的名字，不是官方译名。
    // 这一档先判，因为一个汉化版完全可能基于一条带中文语言标记的发行版——
    // 底版说什么语言不改变「这个名字是汉化组起的」这件事。
    if marks.is_some_and(|marks| marks.contains(&ChineseMark::FanTranslated)) {
        return Classified {
            kind: TitleKind::FanName,
            // **中置信**：这个变体确实是这部作品的汉化版（哈希撞上了 TOSEC 的 `[tr zh]`
            // 条目），名字也确实是它的名字；不确定的只是「该不该拿它当标题」，
            // 而那由类型说了算，不由置信度说了算。
            confidence: Confidence::Medium,
            seam: None,
            evidence: "它撞上的是一条汉化条目，文件名是汉化组自取的名".to_string(),
        };
    }
    if language == Language::Chinese
        && let Some(chinese) = chinese
    {
        let region = release
            .and_then(|r| r.region.as_deref())
            .unwrap_or("地区没记出来");
        let evidence = match chinese.seam {
            Seam::OwnRelease => format!(
                "它基于的那条发行版就是**官中那一条**（地区 {region}、序列号 {}，\
                 {}）——官中在这一侧是独立一条发行版，中文名归到那一条上",
                release
                    .and_then(|r| r.serial.as_deref())
                    .unwrap_or("没记出来"),
                chinese.declared_by,
            ),
            Seam::LanguageField => format!(
                "它基于的那条发行版地区是 {region}、而{}（{}）——\
                 中文在这一侧是**同一条发行版的语言属性**，不另成发行版",
                chinese.declared_by,
                release
                    .and_then(|r| r.languages.as_deref())
                    .unwrap_or("语言没记出来"),
            ),
        };
        return Classified {
            kind: TitleKind::Translated,
            // **中置信，不是高置信。** 两件事要分开：**这个变体是不是那条官中发行版**
            // ——精确哈希撞上了，确凿；**这串字是不是官方译名**——不确凿，因为
            // **官方译名不在 DAT 里**（那条记录的名字是拉丁字母的
            // `Pokemon 4-in-1 (China) (En,Zh) (Pirate)`），中文名只存在于用户盘上那个
            // 文件的名字里，而文件名是用户起的。来源可信，字面不保证，所以是中置信。
            // 高置信这一档留给**裁决**——人看过一眼才叫确凿。
            confidence: Confidence::Medium,
            seam: Some(chinese.seam),
            evidence,
        };
    }
    Classified {
        kind: TitleKind::Alias,
        // **低置信**：没有任何东西为这个名字背书——它只是盘上那个文件叫这个。
        // 合集包、精简版、带广告后缀的文件名全落在这一档，所以它进待确认队列。
        confidence: Confidence::Low,
        seam: None,
        evidence: "没有官中发行版为这个名字背书，它只是这个变体的文件名".to_string(),
    }
}

/// 折的时候攒着的那本账：**同一串字只留一行**，并数着有几个变体这么叫。
#[derive(Default)]
struct Tally {
    rows: BTreeMap<(String, Language, TitleKind, String, String), TitleRow>,
}

impl Tally {
    fn add(&mut self, row: TitleRow) {
        let key = (
            row.work.clone(),
            row.language,
            row.kind,
            row.source.clone(),
            row.value.clone(),
        );
        match self.rows.get_mut(&key) {
            // 已经有了：只把「有几个变体这么叫」加一。**第一条的依据留着**——
            // 集合按确定的顺序折，第一条永远是同一条。
            Some(existing) => existing.seen += 1,
            None => {
                self.rows.insert(key, row);
            }
        }
    }

    fn rows(self) -> Vec<TitleRow> {
        self.rows.into_values().collect()
    }
}

/// 折一遍标题集合、写进中立库、再折出一份报告。
///
/// **不碰主库、不联网**：要的东西全在中立库里躺着（ADR-0001）。
///
/// # Errors
/// 读写中立库失败时返回错误。
pub fn run(catalog: &mut Catalog, priorities: &Priorities) -> Result<TitleReport, CatalogError> {
    let rows = fold(catalog)?;
    catalog.clear_titles()?;
    catalog.put_titles(&rows)?;
    TitleReport::build(catalog, priorities)
}

/// 一个作品在标题集合之外还有一条兜底的叫法：**作品名**。
///
/// 报告与导出都要遍历「全部作品」，而不是「有标题的那些作品」——一个叫法都没有的作品
/// 照样要有显示标题，否则前端里是一片空白。
///
/// # Errors
/// 读库失败时返回错误。
pub fn work_titles(catalog: &Catalog) -> Result<Vec<TitleSet>, CatalogError> {
    let mut sets: BTreeMap<String, TitleSet> = BTreeMap::new();
    for name in catalog.work_names()?.into_values() {
        sets.entry(name.clone()).or_insert_with(|| TitleSet {
            work: name,
            entries: Vec::new(),
        });
    }
    catalog.for_each_title(&mut |row| {
        // 作品在库里没了（重跑识别换掉了），而裁决留下的叫法还在——照样列出来，
        // 它是人说过的话。
        sets.entry(row.work.clone())
            .or_insert_with(|| TitleSet {
                work: row.work.clone(),
                entries: Vec::new(),
            })
            .entries
            .push(row.clone());
    })?;
    Ok(sets.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scrape::priority::VERDICT;

    fn 叫法(value: &str, language: Language, kind: TitleKind, source: &str) -> TitleRow {
        TitleRow {
            work: "某作品".to_string(),
            value: value.to_string(),
            language,
            kind,
            source: source.to_string(),
            region: None,
            variant_key: None,
            confidence: Confidence::Low,
            seam: None,
            evidence: "编的".to_string(),
            seen: 1,
        }
    }

    fn 集合(entries: Vec<TitleRow>) -> TitleSet {
        TitleSet {
            work: "Chrono Trigger".to_string(),
            entries,
        }
    }

    #[test]
    fn 显示标题按中文英文日文文件名回退() {
        let priorities = Priorities::builtin();
        let 中文 = 叫法("超时空之轮", Language::Chinese, TitleKind::Alias, "文件名");
        let 英文 = 叫法(
            "Chrono Trigger",
            Language::English,
            TitleKind::Official,
            "No-Intro",
        );
        let 日文 = 叫法(
            "Kuronoo Torigaa",
            Language::Japanese,
            TitleKind::Official,
            "No-Intro",
        );
        let 兜底 = 叫法("某个文件名", Language::Unknown, TitleKind::Alias, "文件名");

        let 全都有 = 集合(vec![兜底.clone(), 日文.clone(), 英文.clone(), 中文.clone()]);
        assert_eq!(choose(&全都有, &priorities).display, "超时空之轮");

        let 没中文 = 集合(vec![兜底.clone(), 日文.clone(), 英文.clone()]);
        assert_eq!(choose(&没中文, &priorities).display, "Chrono Trigger");

        let 只有日文 = 集合(vec![兜底.clone(), 日文]);
        assert_eq!(choose(&只有日文, &priorities).display, "Kuronoo Torigaa");

        let 只剩兜底 = 集合(vec![兜底]);
        assert_eq!(choose(&只剩兜底, &priorities).display, "某个文件名");

        let 空的 = 集合(Vec::new());
        let chosen = choose(&空的, &priorities);
        assert_eq!(chosen.display, "Chrono Trigger", "一条都没有就退回作品名");
        assert_eq!(chosen.kind, None);
    }

    #[test]
    fn 中文标题取官中的官方译名而不是汉化版文件名() {
        // **这是这张票最容易做错的地方**（ADR-0012）：首选启动的是汉化版，
        // 而标题这条链路根本不看首选——`choose` 拿到的只有标题集合。
        let priorities = Priorities::builtin();
        let mut 官中 = 叫法(
            "口袋妖怪四合一",
            Language::Chinese,
            TitleKind::Translated,
            "文件名",
        );
        官中.confidence = Confidence::High;
        官中.seam = Some(Seam::OwnRelease);
        let mut 汉化 = 叫法(
            "口袋妖怪 某某汉化组版",
            Language::Chinese,
            TitleKind::FanName,
            "文件名",
        );
        汉化.confidence = Confidence::Medium;
        // 汉化版在库里有一大把文件，官中版只有一个——**数量压不过类型**。
        汉化.seen = 40;

        let both = 集合(vec![汉化.clone(), 官中]);
        let chosen = choose(&both, &priorities);
        assert_eq!(chosen.display, "口袋妖怪四合一");
        assert_eq!(chosen.kind, Some(TitleKind::Translated));
        assert_eq!(chosen.seam, Some(Seam::OwnRelease));

        // 官中不在了，汉化组自取的名才轮得上——它照样在集合里，只是排在最后。
        let 只有汉化 = 集合(vec![汉化]);
        assert_eq!(
            choose(&只有汉化, &priorities).display,
            "口袋妖怪 某某汉化组版"
        );
    }

    #[test]
    fn 裁决压过一切() {
        let priorities = Priorities::builtin();
        let mut 人说的 = 叫法(
            "Chrono Trigger",
            Language::English,
            TitleKind::Official,
            VERDICT,
        );
        人说的.confidence = Confidence::High;
        let mut 官中 = 叫法(
            "超时空之轮",
            Language::Chinese,
            TitleKind::Translated,
            "文件名",
        );
        官中.confidence = Confidence::High;
        let set = 集合(vec![官中, 人说的]);
        assert_eq!(
            choose(&set, &priorities).display,
            "Chrono Trigger",
            "人改过的东西不许被任何数据源覆盖，哪怕他挑的是英文名"
        );
    }

    #[test]
    fn 同一档里有好几个时的选定规则是确定的() {
        let priorities = Priorities::builtin();
        let mut 多数派 = 叫法("超时空之轮", Language::Chinese, TitleKind::Alias, "文件名");
        多数派.seen = 12;
        let mut 少数派 = 叫法("时空之轮", Language::Chinese, TitleKind::Alias, "文件名");
        少数派.seen = 1;
        let set = 集合(vec![少数派.clone(), 多数派.clone()]);
        let chosen = choose(&set, &priorities);
        assert_eq!(chosen.display, "超时空之轮", "同一档里多数派胜出");
        assert_eq!(chosen.chinese_names, 2, "两个中文译名都在集合里");

        // 置信度压过数量：一条官中译名压得过十二个文件名。
        let mut 官中 = 少数派.clone();
        官中.kind = TitleKind::Translated;
        官中.confidence = Confidence::High;
        let set = 集合(vec![多数派.clone(), 官中]);
        assert_eq!(choose(&set, &priorities).display, "时空之轮");

        // **源名次排在数量之前。** `priorities.toml` 说 No-Intro 的条目名最整齐，
        // 那就不该被「同一个 TOSEC 写法在盘上重复了三十遍」压过去。
        let mut 整齐的 = 叫法(
            "Chrono Trigger",
            Language::English,
            TitleKind::Official,
            "No-Intro",
        );
        整齐的.seen = 1;
        let mut 重复多的 = 叫法(
            "Chrono Trigger (1995)(Squaresoft)",
            Language::English,
            TitleKind::Official,
            "TOSEC",
        );
        重复多的.seen = 30;
        let set = 集合(vec![重复多的, 整齐的]);
        assert_eq!(choose(&set, &priorities).display, "Chrono Trigger");

        // 全都一样时按字典序，同一份库跑两次结果一样。
        let mut 甲 = 多数派.clone();
        甲.value = "甲".to_string();
        let mut 乙 = 多数派;
        乙.value = "乙".to_string();
        let 正 = 集合(vec![甲.clone(), 乙.clone()]);
        let 反 = 集合(vec![乙, 甲]);
        assert_eq!(
            choose(&正, &priorities).display,
            choose(&反, &priorities).display
        );
    }

    #[test]
    fn 排序标题独立生成而不是拿中文标题去排() {
        let priorities = Priorities::builtin();
        let 一部 = |work: &str, 中文: &str| TitleSet {
            work: work.to_string(),
            entries: vec![
                叫法(中文, Language::Chinese, TitleKind::Alias, "文件名"),
                叫法(work, Language::English, TitleKind::Official, "No-Intro"),
            ]
            .into_iter()
            .map(|mut row| {
                row.work = work.to_string();
                row
            })
            .collect(),
        };
        let mut 两部: Vec<Chosen> = vec![
            choose(&一部("Sangokushi", "三國志"), &priorities),
            choose(&一部("Asteroids", "爆破彗星"), &priorities),
        ];

        // 显示标题是中文的。
        assert_eq!(两部[0].display, "三國志");
        assert_eq!(两部[1].display, "爆破彗星");
        // 按显示标题的码位排：`三`(U+4E09) 在 `爆`(U+7206) 前面——那是**乱排**。
        let mut 按码位: Vec<&str> = 两部.iter().map(|c| c.display.as_str()).collect();
        按码位.sort_unstable();
        assert_eq!(按码位, vec!["三國志", "爆破彗星"]);

        // 按排序标题排：Asteroids 在 Sangokushi 前面，这才是读者预期的顺序。
        两部.sort_by(|a, b| a.sort.cmp(&b.sort));
        assert_eq!(
            两部.iter().map(|c| c.display.as_str()).collect::<Vec<_>>(),
            vec!["爆破彗星", "三國志"]
        );
        assert_eq!(两部[0].sort, "ASTEROIDS");
        assert_eq!(两部[0].sort_from, SortFrom::LatinTitle);
    }

    #[test]
    fn 一个拉丁标题都没有时排序标题报得出来() {
        let priorities = Priorities::builtin();
        let set = TitleSet {
            work: "超时空之轮".to_string(),
            entries: vec![叫法(
                "超时空之轮",
                Language::Chinese,
                TitleKind::Alias,
                "文件名",
            )],
        };
        let chosen = choose(&set, &priorities);
        assert_eq!(
            chosen.sort_from,
            SortFrom::None,
            "这一档要人工补，不能悄悄排掉"
        );

        // 作品名是拉丁的话就用作品名——它永远有。
        let set = TitleSet {
            work: "Chrono Trigger".to_string(),
            entries: vec![叫法(
                "超时空之轮",
                Language::Chinese,
                TitleKind::Alias,
                "文件名",
            )],
        };
        assert_eq!(choose(&set, &priorities).sort_from, SortFrom::WorkName);
        assert_eq!(choose(&set, &priorities).sort, "CHRONO TRIGGER");
    }

    #[test]
    fn 日版条目名是日文原名而不是官方英文名() {
        // No-Intro 的日版条目名是**罗马字**，字形上与英文名一模一样——
        // 只有那一条发行版的地区说得出它其实是日文原名。
        assert_eq!(
            language_of("Gensou Suikoden", Some("Japan")),
            Language::Japanese
        );
        assert_eq!(language_of("Suikoden", Some("USA")), Language::English);
        // 汉字同理：日版条目名里的汉字是日文，不是中文译名。
        assert_eq!(language_of("幻想水滸伝", Some("Japan")), Language::Japanese);
        assert_eq!(language_of("幻想水滸传", Some("China")), Language::Chinese);
        assert_eq!(language_of("幻想水滸传", None), Language::Chinese);
        // 假名一律日文，地区说什么都不改。
        assert_eq!(language_of("ポケモン", Some("China")), Language::Japanese);
        assert_eq!(language_of("1942", None), Language::Unknown);
    }

    #[test]
    fn 世代裂缝两侧分得开() {
        let 发行版 = |region: Option<&str>, languages: Option<&str>| ReleaseRow {
            id: 1,
            work_id: 1,
            platform: Some("FC".to_string()),
            region: region.map(ToString::to_string),
            serial: None,
            languages: languages.map(ToString::to_string),
        };
        let 哪一侧 = |release: ReleaseRow, marks: Option<&BTreeSet<ChineseMark>>| {
            chinese_release(Some(&release), marks).map(|chinese| chinese.seam)
        };
        // 卡带世代：官中有自己的地区（台版卡带），是**独立一条发行版**。
        assert_eq!(
            哪一侧(发行版(Some("Taiwan"), Some("En,Zh-Hant")), None),
            Some(Seam::OwnRelease)
        );
        // 数字世代：港服在 Redump 里地区就是 `Asia`，与美服共用同一条记录，
        // 中文只是**语言属性**。
        assert_eq!(
            哪一侧(发行版(Some("Asia"), Some("En,Zh,Ko")), None),
            Some(Seam::LanguageField)
        );
        // 语言里没有中文就根本不是中文发行版。
        assert_eq!(哪一侧(发行版(Some("Japan"), Some("Ja")), None), None);
        // 发行版那一列没记语言，但候选上带官中记号——照样算。
        let marks = BTreeSet::from([ChineseMark::Official]);
        assert_eq!(
            哪一侧(发行版(Some("China"), None), Some(&marks)),
            Some(Seam::OwnRelease)
        );
        // **地区读不出来时落回「独立发行版」**：说中文只是一项属性，等于说这同一条
        // 记录还服务着别的地区，那要有正面证据。TOSEC 的条目名第一个括号是发行日期，
        // 地区在后面，`identify::naming` 保守地留空——那时手上确知的只有
        // 「这条记录声明了中文」。
        assert_eq!(
            哪一侧(发行版(None, Some("Zh")), None),
            Some(Seam::OwnRelease)
        );
        // 没有发行版链接就没有官中可言（汉化版正是这一档，ADR-0012）。
        assert_eq!(chinese_release(None, Some(&marks)), None);
    }

    #[test]
    fn 汉化记号压过底版的中文语言标记() {
        // 一个汉化版完全可能基于一条带 `(Ja,Zh)` 的发行版——底版说什么语言不改变
        // 「这个名字是汉化组起的」这件事。
        let release = ReleaseRow {
            id: 1,
            work_id: 1,
            platform: Some("FC".to_string()),
            region: Some("Taiwan".to_string()),
            serial: None,
            languages: Some("En,Zh".to_string()),
        };
        let marks = BTreeSet::from([ChineseMark::FanTranslated, ChineseMark::Official]);
        let chinese = chinese_release(Some(&release), Some(&marks));
        assert!(chinese.is_some(), "底版确实是一条中文发行版");
        let told = classify(
            crate::scrape::local::FILENAME,
            Language::Chinese,
            Some(&marks),
            chinese,
            Some(&release),
        );
        assert_eq!(told.kind, TitleKind::FanName);
        assert_eq!(told.seam, None);
    }

    #[test]
    fn 中文离线源给的名字是有出处的别名而不是汉化组自取的名() {
        // 同一个变体：它撞上了汉化条目，所以**文件名**那一条是汉化组自取的名。
        // 但中文离线源给的那一串不是盘上那个文件的名字，是数据源里那条条目的名字——
        // 落到汉化组那一档，一个有出处的中文名会被 ADR-0012 那条「别拿它当标题」误伤。
        let marks = BTreeSet::from([ChineseMark::FanTranslated]);
        let 文件名 = classify(
            crate::scrape::local::FILENAME,
            Language::Chinese,
            Some(&marks),
            None,
            None,
        );
        assert_eq!(文件名.kind, TitleKind::FanName);
        let 中文源 = classify(
            crate::identify::fuzzy::SOURCE,
            Language::Chinese,
            Some(&marks),
            None,
            None,
        );
        assert_eq!(中文源.kind, TitleKind::Alias);
        assert_eq!(中文源.confidence, Confidence::Medium);
        assert!(中文源.evidence.contains("有出处"));
        // 没人背书的文件名照旧是低置信别名。
        let 没背书 = classify(
            crate::scrape::local::FILENAME,
            Language::Chinese,
            None,
            None,
            None,
        );
        assert_eq!(没背书.kind, TitleKind::Alias);
        assert_eq!(没背书.confidence, Confidence::Low);
    }

    #[test]
    fn 排得动与排不动分得开() {
        assert!(sortable("Chrono Trigger"));
        assert!(sortable("1942"));
        assert!(!sortable("超时空之轮"));
        assert!(!sortable("ポケモン"));
        assert_eq!(sort_title("  Chrono   Trigger "), "CHRONO TRIGGER");
    }
}
