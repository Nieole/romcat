//! **疑似同一作品**：哪两个**作品**其实是同一个，以及凭什么这么说
//! （票 `gui-looks-like-the-design/17`）。
//!
//! ## 它住在这儿，与[合并作品](super::merge)同一族
//!
//! 这一处答的是「**该不该合**」，[`merge`](super::merge) 答的是「**怎么合**」——一条建议
//! 按下去就是那一层的 [`plan`](super::merge::plan)，落成一批**裁决**。两件事同一族、
//! 同一套词：这一处说的「两个作品」，到那一层就是「保留哪一个、别的归进去」。
//!
//! ## 判断只有这一处（ADR-0024）
//!
//! **界面不许自己比一遍名字、比一遍年份。** 屏上要列的那几条理由由 [`Clue::sentence`]
//! 给，屏上那一行只是印出来；库体检那一格的数、浏览屏「整理建议」那一簇的数，数的都是
//! [`survey`] 交回来的这一份。
//!
//! ## 三类线索，两条能立案
//!
//! 设计稿写出来的线索有三类，可它们的分量不一样：
//!
//! | 线索 | 立得了案吗 | 为什么 |
//! |---|---|---|
//! | [命名撞上](Clue::Naming) | **立得了** | 两个作品各有一个叫法，归一之后是同一串字 |
//! | [同一条中文条目](Clue::ChineseEntry) | **立得了** | 两边的名字在**中文离线源**里指向同一条条目 |
//! | [平台与年份一致](Clue::PlatformAndYear) | **立不了** | 同一年同一平台的游戏成百上千 |
//!
//! 于是**阈值**是这么一句：**平台得有交集**（硬条件），而且**至少有一条立得了案的线索**。
//! 平台与年份那一条只当佐证——它一个人在场时这一对不提。
//!
//! ### 为什么名字这一条只认「归一之后一模一样」，不按相似度
//!
//! 库里现成的相似度（[`zh::similarity`]）在这一层会把**同系列的两部作品**配成一对：
//! `第3次超级机器人大战` 与 `第4次超级机器人大战` 是 0.9，`超级机器人大战J` 与
//! `超级机器人大战R` 是 0.88（[`zh::alnum_of`] 那段文档量的就是它们）。
//! 中文离线源那一侧撞错了，代价是人在队列里多看一眼；**这一层给出的建议人是要按下去
//! 合并的**，撞错了就是把两部游戏并成一个。所以这一层宁可少提：
//! 名字一条只收 [`zh::key_of`] 归一之后**一字不差**的，两边名字确实不一样的那一类
//! 交给中文条目那一条去连（设计稿举的 `Pocket Monster - Red Version` 与
//! `Pocket Monsters - Aka` 正是这一类）。
//!
//! ## 它贵，**不许进画帧那条线程**
//!
//! [`survey`] 要走一遍全库变体表、一遍全库标题集合、一遍中文离线源那几次匹配，
//! 再逐对问年份。界面在**识别做完之后**算一次、把结果攥在手里（同
//! [`merge::impact`](super::merge::impact) 那一条，挂单 `Q1011`）。

use std::collections::{BTreeMap, BTreeSet};

use crate::catalog::{Catalog, CatalogError};
use crate::scrape::AnchorKind;
use crate::scrape::zh::entry_marks;
use crate::title;
use crate::verdict::{NotSameWork, Store, VerdictError, work_pair};
use crate::zh;

/// 一条**线索**：这一对作品凭什么看着像同一个。
///
/// **那句话长在这儿**（[`Self::sentence`]），不在界面上：命令行与界面逐字说同一句
/// （ADR-0024）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Clue {
    /// **命名撞上**：两个作品各有一个叫法，归一之后是同一串字。
    ///
    /// 设计稿把它叫作「不同数据库对这部作品的命名差异」——差异是**结果**（识别因此建成了
    /// 两个作品），而这一条记下的是那串**撞上的字**连各自的出处。
    Naming {
        /// 撞上的那串字，左边那个作品写的样子。
        left: String,
        /// 哪个源说的（**No-Intro** / **TOSEC** / 中文离线源 / 裁决……）。
        left_source: String,
        /// 撞上的那串字，右边那个作品写的样子。
        right: String,
        /// 哪个源说的。
        right_source: String,
    },
    /// **平台与年份一致**。只当佐证，一个人在场时立不了案。
    PlatformAndYear {
        /// 两边共有的那几个平台。
        platforms: Vec<String>,
        /// 两边都说得出、而且相同的那个年份。
        year: String,
    },
    /// **中文离线源**里两个名字指向同一条条目。
    ChineseEntry {
        /// 条目号。
        entry: u32,
    },
}

impl Clue {
    /// 屏上与命令行里**逐条列出来**的那一句。
    #[must_use]
    pub fn sentence(&self) -> String {
        match self {
            Self::Naming {
                left,
                left_source,
                right,
                right_source,
            } if left == right => {
                format!("两边都有「{left}」这个叫法（{left_source} 与 {right_source} 各说了一次）")
            }
            Self::Naming {
                left,
                left_source,
                right,
                right_source,
            } => {
                format!(
                    "「{left}」（{left_source}）与「{right}」（{right_source}）\
                     去掉空格与标点之后是同一串字"
                )
            }
            Self::PlatformAndYear { platforms, year } => {
                format!("平台（{}）和年份（{year}）一致", platforms.join("、"))
            }
            Self::ChineseEntry { entry } => {
                format!("中文离线源里两个名字指向同一条条目（条目 {entry}）")
            }
        }
    }

    /// 这一条**立得了案**吗：只有它在场时，这一对提不提。
    ///
    /// 「平台与年份一致」立不了——同一年同一平台的游戏成百上千（见模块文档那张表）。
    #[must_use]
    pub fn makes_a_case(&self) -> bool {
        !matches!(self, Self::PlatformAndYear { .. })
    }
}

/// 一条**疑似同一作品**的建议：两个作品，连理由。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Suspicion {
    /// 那一对作品名，**排过序**（[`work_pair`]）：同一对问两遍答的是同一个次序，
    /// 落库那一侧认的也是它。
    pub works: [String; 2],
    /// 凭什么。**至少一条立得了案**（[`Clue::makes_a_case`]），次序固定。
    pub clues: Vec<Clue>,
}

impl Suspicion {
    /// 这一对里**另一个**是谁；`work` 不在这一对里时是 `None`。
    ///
    /// 屏上那张卡片写的是「可能与《某某》是同一个作品」——「某某」就是它。
    #[must_use]
    pub fn other_than(&self, work: &str) -> Option<&str> {
        match (self.works[0].as_str(), self.works[1].as_str()) {
            (left, right) if left == work => Some(right),
            (left, right) if right == work => Some(left),
            _ => None,
        }
    }

    /// 这一对里有 `work` 吗。
    #[must_use]
    pub fn touches(&self, work: &str) -> bool {
        self.other_than(work).is_some()
    }

    /// 屏上逐条列出来的那几句（[`Clue::sentence`]）。
    #[must_use]
    pub fn reasons(&self) -> Vec<String> {
        self.clues.iter().map(Clue::sentence).collect()
    }
}

/// 扫一趟疑似同一作品跑不下去的原因。
#[derive(Debug, thiserror::Error)]
pub enum SuspicionError {
    /// 中立库读写失败。
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    /// 沉淀库读写失败。
    #[error(transparent)]
    Verdict(#[from] VerdictError),
}

/// **全库扫一趟**：哪几对作品疑似是同一个。人说过「不是同一个」的那些**不在里面**。
///
/// 次序固定：按那一对作品名排。同一份库跑两遍交回来的是同一份。
///
/// **两份库一个字都不写，主库一个字节都不读**（ADR-0001、ADR-0004）。
/// **它贵**，别放进画帧那条线程（见模块文档）。
///
/// # Errors
/// 读两份库失败时返回错误。
pub fn survey(
    catalog: &Catalog,
    store: &Store,
    library: &str,
) -> Result<Vec<Suspicion>, SuspicionError> {
    let held = holdings(catalog)?;
    let mut clues: BTreeMap<[String; 2], BTreeSet<Clue>> = BTreeMap::new();
    for (pair, clue) in naming_clues(catalog, &held)? {
        clues.entry(pair).or_default().insert(clue);
    }
    for (pair, clue) in chinese_clues(catalog, &held)? {
        clues.entry(pair).or_default().insert(clue);
    }
    let dismissed: BTreeSet<[String; 2]> = store
        .not_same_works(library)?
        .into_iter()
        .map(|one: NotSameWork| one.works)
        .collect();
    let mut out = Vec::new();
    for (pair, found) in clues {
        // **人看过了、说不是同一个**：那一对从此不提（[`Store::set_not_same_work`]）。
        if dismissed.contains(&pair) {
            continue;
        }
        // **平台得有交集**（硬条件）：两个作品连一个平台都不共有时，「同一个游戏被识别
        // 成了两个作品」这句话说不通——同一部游戏跨平台发行时，库里本来就该是一个作品
        // 底下挂着几个平台的变体。
        let shared: Vec<String> = held
            .get(&pair[0])
            .zip(held.get(&pair[1]))
            .map(|(left, right)| {
                left.platforms
                    .intersection(&right.platforms)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if shared.is_empty() {
            continue;
        }
        let mut found: Vec<Clue> = found.into_iter().collect();
        if !found.iter().any(Clue::makes_a_case) {
            continue;
        }
        // **佐证最后补**：它立不了案，所以排在立了案的那几条后面——屏上那一列读下来
        // 是「凭这个，还有这个佐证」。
        if let Some(clue) = platform_and_year(catalog, &pair, &shared)? {
            found.push(clue);
        }
        out.push(Suspicion {
            works: pair,
            clues: found,
        });
    }
    Ok(out)
}

/// 人说**「不是同一个」**：这一对从此不再提。记进[沉淀库](Store)，撤得掉。
///
/// **它不是一条[裁决](crate::verdict::Decision)**，尽管屏上两颗按钮并排摆着：
/// 肯定那一档按下去是**合并作品**，落成的是一批裁决（[`merge::apply`](super::merge::apply)）；
/// 否定这一档什么都不合，只是记下「人看过了」。两者落在两张表里，撤销也各走各的
/// （见 [`verdict`](crate::verdict) 的模块文档）。
///
/// # Errors
/// 写沉淀库失败时返回错误。
pub fn not_same(store: &mut Store, library: &str, a: &str, b: &str) -> Result<(), VerdictError> {
    store.set_not_same_work(library, a, b)
}

/// 撤掉一条「不是同一个」，交回原来有没有这一条。撤完这一对重新出现在 [`survey`] 里。
///
/// # Errors
/// 写沉淀库失败时返回错误。
pub fn undo_not_same(
    store: &mut Store,
    library: &str,
    a: &str,
    b: &str,
) -> Result<bool, VerdictError> {
    store.undo_not_same_work(library, a, b)
}

/// 一个作品名下**眼下有什么**：这一处判断要的那几样。
#[derive(Debug, Default, Clone)]
struct Holding {
    /// 名下那几个变体的键。
    variants: BTreeSet<String>,
    /// 名下那几个变体落在哪几个平台上；说不出平台的那些不在里面。
    platforms: BTreeSet<String>,
}

/// 每个作品名下**眼下有什么**。**数的是整份中立库，不过任何筛选**——
/// 两个作品是不是同一个，与人眼下筛着什么无关（同
/// [`merge::remaining`](super::merge::remaining) 逐字同一条）。
///
/// **按作品名归堆，不按行号**：`work` 那张表的 `name` 上没有唯一约束（同名异作是真的，
/// 两部 `Ninja Gaiden`），而全仓归堆一律按名字（`Catalog::work_representatives`）。
/// 一个变体都不剩的作品不在里面：它不该被建议合并，合过去也没有东西可搬。
fn holdings(catalog: &Catalog) -> Result<BTreeMap<String, Holding>, CatalogError> {
    let works = catalog.work_names()?;
    let mut out: BTreeMap<String, Holding> = BTreeMap::new();
    for variant in catalog.variants()? {
        let Some(name) = variant.work_id.and_then(|id| works.get(&id)) else {
            continue;
        };
        let held = out.entry(name.clone()).or_default();
        held.variants.insert(variant.key.clone());
        if let Some(platform) = variant.platform {
            held.platforms.insert(platform);
        }
    }
    Ok(out)
}

/// **命名撞上**那一条：把全库的叫法按归一之后那串字倒排，同一串字底下的作品两两成对。
///
/// 叫法从[标题集合](title::work_titles)取——**DAT 条目名本来就在里面**
/// （`title::fold` 把自动通过的候选那条条目名折成**官方名称**，源就是那个数据库的名字），
/// 所以「TOSEC 叫 X、No-Intro 叫 Y」这件事读得出来，不必再去翻一遍候选。
/// 一条叫法都没有的作品也在里面：那一份兜底的叫法是**作品名**自己。
fn naming_clues(
    catalog: &Catalog,
    held: &BTreeMap<String, Holding>,
) -> Result<Vec<([String; 2], Clue)>, CatalogError> {
    // 归一之后那串字 → 说得出它的那几个作品，各配一条「谁说的、写成什么样」。
    let mut by_key: BTreeMap<String, BTreeMap<String, (String, String)>> = BTreeMap::new();
    for set in title::work_titles(catalog)? {
        if !held.contains_key(&set.work) {
            continue;
        }
        let mut said: Vec<(&str, &str)> = Vec::with_capacity(set.entries.len() + 1);
        for row in &set.entries {
            // **压过的叫法不算**：`title::fold` 折出来的这一份里本来就没有它们，
            // 这里只挡住裁决那一路顺手写进来的空串。
            if row.value.trim().is_empty() {
                continue;
            }
            said.push((row.value.as_str(), row.source.as_str()));
        }
        // **作品名自己垫在最后**：它也是一条叫法（一条叫法都没有的作品身上只有它，
        // 而它正是识别建这一行时用的那个名字），但**说得出数据库的那几条排在它前面**
        // ——理由那一句要答的是「哪个数据库这么叫」，而作品名答不了
        // （源写成[作品名那一档](WORK_NAME)，屏上读得出来）。
        said.push((set.work.as_str(), WORK_NAME));
        for (value, source) in said {
            let key = zh::key_of(value);
            // **归一之后空了的不撞**：剥到只剩标点的名字撞上的是一大片。
            if key.is_empty() {
                continue;
            }
            by_key
                .entry(key)
                .or_default()
                // 同一个作品在同一串字上只留一条：留**头一个**，于是同一份库跑两遍
                // 挑出来的出处是同一个（叫法按作品、语言、类型、源、值排过序）。
                .entry(set.work.clone())
                .or_insert_with(|| (value.to_string(), source.to_string()));
        }
    }
    let mut out = Vec::new();
    for (_, works) in by_key {
        if works.len() < 2 {
            continue;
        }
        let said: Vec<(&String, &(String, String))> = works.iter().collect();
        for (at, (left, left_said)) in said.iter().enumerate() {
            for (right, right_said) in said.iter().skip(at + 1) {
                out.push((
                    work_pair(left, right),
                    Clue::Naming {
                        left: left_said.0.clone(),
                        left_source: left_said.1.clone(),
                        right: right_said.0.clone(),
                        right_source: right_said.1.clone(),
                    },
                ));
            }
        }
    }
    Ok(out)
}

/// 一条叫法**出自作品名自己**时，理由那一句里写的出处。
///
/// 它不是一个源——库里没有哪个数据源叫这个名字；它说的是「这串字就是这一行在库里的
/// 名字」。写成空的话屏上那一句会变成「（）说的」。
pub const WORK_NAME: &str = "作品名";

/// **同一条中文条目**那一条：把全库撞上的条目号倒排，同一条条目底下的作品两两成对。
///
/// 一个作品撞上了哪条条目，问的是[中文离线源那几次匹配](entry_marks)——**认法与队列里
/// 归堆那一处是同一个**（ADR-0024）。两层锚点都算：类型、简介那几格挂在**作品**上，
/// 中文名与别名挂在**变体**上（票 02），两边都是「这个作品的名字撞上了那条条目」。
fn chinese_clues(
    catalog: &Catalog,
    held: &BTreeMap<String, Holding>,
) -> Result<Vec<([String; 2], Clue)>, CatalogError> {
    let marks = entry_marks(catalog)?;
    // 变体键 → 它属于哪个作品。
    let mut owner: BTreeMap<&str, &str> = BTreeMap::new();
    for (work, holding) in held {
        for key in &holding.variants {
            owner.insert(key.as_str(), work.as_str());
        }
    }
    let mut by_entry: BTreeMap<u32, BTreeSet<&str>> = BTreeMap::new();
    for ((kind, subject), entries) in &marks {
        let work = match kind {
            AnchorKind::Work if held.contains_key(subject.as_str()) => subject.as_str(),
            AnchorKind::Variant => match owner.get(subject.as_str()) {
                Some(work) => work,
                None => continue,
            },
            AnchorKind::Work => continue,
        };
        for entry in entries {
            by_entry.entry(*entry).or_default().insert(work);
        }
    }
    let mut out = Vec::new();
    for (entry, works) in by_entry {
        if works.len() < 2 {
            continue;
        }
        let works: Vec<&str> = works.into_iter().collect();
        for (at, left) in works.iter().enumerate() {
            for right in works.iter().skip(at + 1) {
                out.push((work_pair(left, right), Clue::ChineseEntry { entry }));
            }
        }
    }
    Ok(out)
}

/// **平台与年份一致**那条佐证：两边年份都说得出而且相同时才有。
///
/// 年份从[屏上写着的那一个](Catalog::work_year_of)取，不另算一遍：这一句理由摆在屏上，
/// 而同一个作品的年份在库里可以有好几条，各取各的就会与那一行写着的对不上。
///
/// **年份对不上时不否掉这一对**，只是不补这一条：不同数据库给同一部作品记的年份本来就
/// 常常差一年（首发地区不同），拿它当否决闸会把真该提的那些整片挡掉。
fn platform_and_year(
    catalog: &Catalog,
    pair: &[String; 2],
    shared: &[String],
) -> Result<Option<Clue>, CatalogError> {
    let (Some(left), Some(right)) = (
        catalog.work_year_of(&pair[0])?,
        catalog.work_year_of(&pair[1])?,
    ) else {
        return Ok(None);
    };
    if left != right {
        return Ok(None);
    }
    Ok(Some(Clue::PlatformAndYear {
        platforms: shared.to_vec(),
        year: left,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 平台与年份一致那一条一个人在场时立不了案() {
        let 佐证 = Clue::PlatformAndYear {
            platforms: vec!["GB".to_string()],
            year: "1996".to_string(),
        };
        assert!(!佐证.makes_a_case(), "同一年同一平台的游戏成百上千");
        assert!(Clue::ChineseEntry { entry: 7 }.makes_a_case());
        assert!(
            Clue::Naming {
                left: "幻想传说".to_string(),
                left_source: "No-Intro".to_string(),
                right: "幻想传说".to_string(),
                right_source: "TOSEC".to_string(),
            }
            .makes_a_case()
        );
    }

    #[test]
    fn 两边写法一样与两边写法不同的那两句话分开说() {
        let 一样 = Clue::Naming {
            left: "幻想传说".to_string(),
            left_source: "No-Intro".to_string(),
            right: "幻想传说".to_string(),
            right_source: "TOSEC".to_string(),
        };
        let 话 = 一样.sentence();
        assert!(话.contains("两边都有「幻想传说」"), "{话}");
        assert!(话.contains("No-Intro") && 话.contains("TOSEC"), "{话}");

        let 不同 = Clue::Naming {
            left: "塞尔达传说 - 时空之章".to_string(),
            left_source: "No-Intro".to_string(),
            right: "塞尔达传说：时空之章".to_string(),
            right_source: "TOSEC".to_string(),
        };
        let 话 = 不同.sentence();
        assert!(话.contains("塞尔达传说 - 时空之章"), "{话}");
        assert!(话.contains("塞尔达传说：时空之章"), "{话}");
        assert!(话.contains("同一串字"), "得说清凭什么算撞上：{话}");
    }

    #[test]
    fn 平台与年份那一句照设计稿写出平台与年份() {
        let 话 = Clue::PlatformAndYear {
            platforms: vec!["GB".to_string()],
            year: "1996".to_string(),
        }
        .sentence();
        assert_eq!(话, "平台（GB）和年份（1996）一致");
    }

    #[test]
    fn 中文条目那一句带得出条目号() {
        let 话 = Clue::ChineseEntry { entry: 1234 }.sentence();
        assert!(话.contains("1234"), "半年后要按号复核：{话}");
        assert!(话.contains("中文离线源"), "{话}");
    }

    #[test]
    fn 一对里另一个是谁两个方向都答得出() {
        let 建议 = Suspicion {
            works: work_pair("精灵宝可梦 红", "口袋妖怪 红"),
            clues: vec![Clue::ChineseEntry { entry: 1 }],
        };
        assert_eq!(建议.other_than("口袋妖怪 红"), Some("精灵宝可梦 红"));
        assert_eq!(建议.other_than("精灵宝可梦 红"), Some("口袋妖怪 红"));
        assert_eq!(建议.other_than("幻想传说"), None);
        assert!(建议.touches("口袋妖怪 红"));
        assert!(!建议.touches("幻想传说"));
    }

    #[test]
    fn 那一对排过序_同一对问两遍答的是同一个次序() {
        assert_eq!(
            work_pair("口袋妖怪 红", "精灵宝可梦 红"),
            work_pair("精灵宝可梦 红", "口袋妖怪 红"),
        );
    }
}
