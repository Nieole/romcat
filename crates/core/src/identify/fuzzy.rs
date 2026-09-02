//! **倒数第二层：文件名规则加中文离线模糊匹配。**
//!
//! 前面三层——CRC-32、光盘序列号、卡带内部头——判据都来自**内容自己的字节**。到这一层
//! 字节已经说不出话了：这份内容要么撞不上任何 DAT（汉化版改过字节、而 GBA 与 NDS 的
//! 汉化版在 DAT 里根本没有对应条目），要么压根没有可以撞的东西。手上只剩一个文件名。
//!
//! 于是这一层做两件事，各由一个模块承担，这里只把它们接到识别管线上：
//!
//! 1. [`filename::Rules`] 把文件名剥成**正题**——汉化组署名、版本号、`(简)(JP)(64Mb)`
//!    这些记号全剥掉。
//! 2. [`zh::Index`] 拿正题去撞**中文离线数据源**，用**平台与年份**做交叉校验。
//!
//! ## 它产出的候选**永远不自动通过**
//!
//! 这不是打折，是如实说：**这一层没看这个文件里的一个字节**。同名的游戏、同系列的
//! 续作、同一个名字的不同平台移植，在名字这一层是分不开的——`超级机器人大战R` 与
//! `超级机器人大战` 的相似度是 0.92。所以结论一律进**待确认队列**（ADR-0002），
//! 由人裁决。两档：
//!
//! | 档 | 判据 |
//! |---|---|
//! | **中置信** | 相似度过 [`Tuning::strong`](zh::Tuning::strong)，**而且平台与年份两道交叉校验都真的对上** |
//! | **低置信** | 过了入门相似度，但至少有一道校验「说不出」 |
//!
//! 「对不上」那一档在 [`zh::Index::lookup`] 里就整条不产出了——**宁可留空也不要写错的
//! 中文名**（调研 §13.1）。
//!
//! ## 拿哪几个名字去撞
//!
//! 真库里那个游戏的中文名不一定写在变体自己的名字上：
//!
//! ```text
//! psp/PSP汉化典藏合集/ROM/我的暑假[简体汉化版][ACG汉化组]/ACG_Summer_Holiday.7z
//! ```
//!
//! 中文名在**上一级目录**上，变体自己叫 `ACG_Summer_Holiday`。所以还要收目录名——
//! 但**只收独占目录**（这个目录下只有这一个变体）。判据与刮削那一侧认本地媒体的
//! 「独占目录」规则同源（`scrape::local`）：`FC/` 底下三千个 zip 挤在一起时，
//! 目录名属于谁根本说不清，而这一层错一条就是往队列里塞一条错的候选。
//!
//! ## 名字还是乱码的那些，不撞
//!
//! 票 03 对非 UTF-8 的容器内部文件名按**有损转换**处理，理由是「识别靠 CRC-32、
//! 名字不参与命中」。**到这一层名字开始参与了**，那批乱码就成了实打实的问题。
//! 判据是字符串里有没有 `U+FFFD`（有损转换留下的替换字符）：有就不撞，并单独计数
//! ——[`crate::container`] 那一侧现在会先探编码再解码，重扫一遍容器就好了，
//! 而报告要说得出还剩多少条。

use std::collections::BTreeMap;

use crate::catalog::VariantRow;
use crate::catalog::identify::{Candidate, Confidence};
use crate::dat::Convention;
use crate::filename::{self, Rules};
use crate::zh;

/// 这一层在候选表的「数据源」那一栏里叫什么。
///
/// 它与 `No-Intro`、`TOSEC`、`沉淀库` 平级地出现在报告里——**这一层贡献了多少覆盖率，
/// 要一眼看得出来**，与 DAT 给的那部分分开数（同 `identify::VERDICT_SOURCE`）。
/// 刮削那一侧的中文名源用的也是这个名字（`scrape::zh`），两处必须是同一个字符串：
/// 优先级表按它排，写岔了那条优先级就永远命不中。
pub const SOURCE: &str = "中文离线源";

/// 这一层认得的东西与调得动的参数。
///
/// 三样捆在一起传，是因为它们**同进同出**：没有索引这一层什么都做不了，
/// 而剥离规则与匹配参数换一份就该重跑一遍。散成三个参数，调用处迟早会漏传一个。
#[derive(Debug, Clone, Copy)]
pub struct Naming<'a> {
    /// 剥离规则。
    pub rules: &'a Rules,
    /// 中文离线索引；`None` 表示还没取过数，这一层整个不跑。
    pub index: Option<&'a zh::Index>,
    /// 匹配参数。
    pub tuning: zh::Tuning,
}

impl<'a> Naming<'a> {
    /// 只有剥离规则、没有中文索引的一份。这一层不会产出任何候选。
    #[must_use]
    pub fn new(rules: &'a Rules) -> Self {
        Self {
            rules,
            index: None,
            tuning: zh::Tuning::default(),
        }
    }

    /// **这一层关掉的那一份**：没取过中文数据源时识别照跑，只是不产出这一层的候选。
    ///
    /// 它自己带一份内置剥离规则（只解析一次，之后共用），于是调用方不必为了「关掉它」
    /// 先造一份规则出来——**关掉一层不该比开着它还麻烦**。
    #[must_use]
    pub fn off() -> Naming<'static> {
        static RULES: std::sync::OnceLock<Rules> = std::sync::OnceLock::new();
        Naming {
            rules: RULES.get_or_init(Rules::builtin),
            index: None,
            tuning: zh::Tuning::default(),
        }
    }

    /// 这一层跑得起来吗。
    #[must_use]
    pub fn ready(&self) -> bool {
        self.index.is_some_and(|index| !index.is_empty())
    }
}

/// 一个可以拿去撞的名字，连它的**出处**。
///
/// 出处要写进依据：人在队列里看到一条候选，第一件要判断的事就是「它是从哪个名字
/// 剥出来的」——是这个文件自己的名字，还是它上一级目录的名字。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    /// 名字本身。
    pub text: String,
    /// 从哪儿来的。
    pub from: &'static str,
}

impl Named {
    /// 变体自己的文件名。
    #[must_use]
    pub fn own(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            from: "变体自己的名字",
        }
    }

    /// 独占目录的名字。
    #[must_use]
    pub fn directory(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            from: "独占目录的名字",
        }
    }

    /// 容器里唯一那个内容条目的名字。
    #[must_use]
    pub fn inside(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            from: "容器里那个文件的名字",
        }
    }
}

/// 撞上的那一条，连**它是怎么撞上的**。
///
/// 捏成一个类型而不是一个五元组：五样东西一路穿过一张表、一次排序、再进候选，
/// 而其中三样都是 `String`——元组里写错顺序编译器一个字都不会说。
struct Picked {
    /// 撞上的那条条目与两道校验的结论。
    one: zh::Match,
    /// 拿哪一串字撞的。
    text: String,
    /// 那一串字是名字的哪一部分（正题 / 正题里的中文）。
    label: &'static str,
    /// 那个名字本身从哪儿来（变体自己的 / 独占目录的 / 容器里那个文件的）。
    from: &'static str,
    /// 剥离那一步做了什么，写成一句。
    stripped: String,
}

/// 这一层对一个变体干了什么。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Found {
    /// 产出的候选。
    pub candidates: Vec<Candidate>,
    /// 真的拿去撞了几个名字。
    pub tried: u64,
    /// 有几个名字因为**还是乱码**没敢撞。
    pub garbled: u64,
    /// 剥离规则**归不了类的记号**。维护者照着它往配置里补。
    pub unknown: Vec<String>,
    /// 产出的候选里有几条够得着中置信。
    pub strong: u64,
}

/// 拿一个变体的几个名字去撞中文离线数据源。
///
/// `platform` 是拿去做**平台交叉校验**的那一个。**它该是内容说的那个，不是目录说的**
/// ——目录只是强先验，字节说了算（ADR-0011）。调用方按这个顺序取：卡带内部头读出来的
/// 平台优先，读不出来才退回变体所在的目录。真库里 `psp/` 目录下混着整包的 FC / GB /
/// SFC ROM，按目录判会把它们的中文名整批判成「平台对不上」。
///
/// `year` 是**这个变体这一侧**说得出的发行年份：文件名里明写的，或者已有候选的
/// DAT 条目名里读出来的（TOSEC 的第一个括号是发行日期）。说不出就是 `None`——
/// **那时年份那道校验只能是「说不出」，这条候选够不着中置信**。
#[must_use]
pub fn candidates(
    naming: &Naming<'_>,
    variant: &VariantRow,
    names: &[Named],
    platform: Option<&str>,
    year: Option<u16>,
) -> Found {
    let mut found = Found::default();
    let Some(index) = naming.index else {
        return found;
    };
    if index.is_empty() {
        return found;
    }
    // 同一条条目会被好几个名字撞上（变体自己的名字与它上一级目录名说的是同一件事），
    // 只留分最高的那一条——留两条只是让人在队列里读同一句话两遍。
    let mut best: BTreeMap<u32, Picked> = BTreeMap::new();
    for named in names {
        // **乱码不撞。** 有损转换留下的 `U+FFFD` 一进来就把相似度算成一团糟，
        // 而撞出来的东西没人分辨得了对错。
        if named.text.contains('\u{FFFD}') {
            found.garbled += 1;
            continue;
        }
        let parsed = naming.rules.parse(&named.text);
        for mark in &parsed.unknown {
            if !found.unknown.contains(mark) {
                found.unknown.push(mark.clone());
            }
        }
        let year = year.or(parsed.year);
        let stripped = describe(&named.text, &parsed);
        for (label, text) in parsed.queries() {
            found.tried += 1;
            for one in index.lookup(
                &zh::Query {
                    text,
                    platform,
                    year,
                },
                &naming.tuning,
            ) {
                let picked = Picked {
                    one,
                    text: text.to_string(),
                    label,
                    from: named.from,
                    stripped: stripped.clone(),
                };
                let slot = best.entry(picked.one.entry.id);
                match slot {
                    std::collections::btree_map::Entry::Vacant(slot) => {
                        slot.insert(picked);
                    }
                    std::collections::btree_map::Entry::Occupied(mut slot) => {
                        if picked.one.score > slot.get().one.score {
                            slot.insert(picked);
                        }
                    }
                }
            }
        }
    }
    let mut rows: Vec<Picked> = best.into_values().collect();
    // 分高的在前；同分按条目号定死顺序——同一份库跑两次产出的次序必须一样。
    rows.sort_by(|a, b| {
        b.one
            .score
            .partial_cmp(&a.one.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.one.entry.id.cmp(&b.one.entry.id))
    });
    rows.truncate(naming.tuning.limit);
    for picked in rows {
        if picked.one.strong(&naming.tuning) {
            found.strong += 1;
        }
        found
            .candidates
            .push(candidate_of(variant, &picked, index.dump(), &naming.tuning));
    }
    found
}

/// 剥离那一步做了什么，写成一句。**依据里要有它**：人在队列里判断一条候选对不对，
/// 第一件事是看它是从哪个名字、剥掉了什么之后剥出来的。
fn describe(name: &str, parsed: &filename::Parsed) -> String {
    let mut text = format!("文件名「{name}」按剥离规则剥成「{}」", parsed.title);
    if !parsed.stripped.is_empty() {
        let mut parts: Vec<String> = Vec::new();
        for strip in &parsed.stripped {
            let part = format!("{}「{}」", strip.why.label(), strip.what);
            if !parts.contains(&part) {
                parts.push(part);
            }
        }
        text.push_str(&format!("（剥掉了 {}）", parts.join("、")));
    }
    if let Some(team) = &parsed.team {
        text.push_str(&format!("；名字上写着汉化组「{team}」"));
    }
    if let Some(version) = &parsed.version {
        text.push_str(&format!("，版本 {version}"));
    }
    text
}

fn candidate_of(
    variant: &VariantRow,
    picked: &Picked,
    dump: &str,
    tuning: &zh::Tuning,
) -> Candidate {
    let one = &picked.one;
    let strong = one.strong(tuning);
    let mut evidence = format!("{}（{}）。", picked.stripped, picked.from);
    evidence.push_str(&one.evidence(dump, picked.label, &picked.text));
    if !strong {
        evidence.push_str(
            "；**两道交叉校验没有都对上**，所以只到低置信——平台与年份任何一边说不出，\
             同名的续作与同系列的移植就分不开",
        );
    }
    Candidate {
        member_key: variant.main_key.clone(),
        inner: String::new(),
        // 中置信是这一层的天花板（ADR-0002 的三档里，高置信留给精确命中与裁决）。
        confidence: if strong {
            Confidence::Medium
        } else {
            Confidence::Low
        },
        // **永不自动通过。** 这一层一个字节都没看。
        accepted: false,
        source: SOURCE.to_string(),
        dat: dump.to_string(),
        // 平台取条目自己说的那个（交叉校验已经保证它与变体的平台不冲突）；
        // 条目没说平台时退回变体的平台——候选表这一列不许空着，报告按它分组。
        platform: one
            .entry
            .platforms
            .first()
            .cloned()
            .or_else(|| variant.platform.clone())
            .unwrap_or_default(),
        game: one.entry.shown().to_string(),
        // 这一列在别的层里是「DAT 里那条 `<rom>` 记录的名字」。这一层没有文件记录，
        // 写**撞上的那个叫法**——编一个文件名顶上去，事后复核的人会以为真有那么一条记录
        // （与序列号那一层在这一列写编号是同一个道理）。
        rom: one.matched.clone(),
        hashed_as: Convention::AsIs,
        dat_convention: Convention::AsIs,
        evidence,
        // **中文记号留空**：`chinese` 这一列说的是「DAT 那条条目说自己是汉化版还是官中版」
        // （ADR-0012），而中文数据源对这件事一个字都没说。名字里带着汉化组署名不等于
        // 识别认出了它是汉化版——票 10 那份「中文占比是识别结论不是从文件名猜的」统计
        // 正是靠这一列，从这儿灌进去会让那张表当场失真。
        chinese: None,
        serial: None,
        release_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::VariantRow;

    fn 变体(key: &str, platform: Option<&str>) -> VariantRow {
        VariantRow {
            key: key.to_string(),
            platform: platform.map(ToString::to_string),
            rule: "同名成组".to_string(),
            main_key: key.to_string(),
            files: 1,
            bytes: 1,
            unreadable_files: 0,
            manual: false,
            work_id: None,
            release_id: None,
        }
    }

    fn 索引() -> zh::Index {
        zh::Index::build(
            vec![
                zh::Entry {
                    id: 4,
                    name: "メタルスラッグ7".to_string(),
                    name_cn: "合金弹头7".to_string(),
                    aliases: vec!["Metal Slug 7".to_string()],
                    year: Some(2008),
                    platforms: vec!["NDS".to_string()],
                    platform_text: "NDS".to_string(),
                },
                zh::Entry {
                    id: 9677,
                    name: "スーパーロボット大戦R".to_string(),
                    name_cn: "超级机器人大战R".to_string(),
                    aliases: Vec::new(),
                    year: Some(2002),
                    platforms: vec!["GBA".to_string()],
                    platform_text: "GBA".to_string(),
                },
            ],
            "dump-2026-09-01".to_string(),
        )
    }

    fn 撞(name: &str, platform: Option<&str>, year: Option<u16>) -> Found {
        let rules = Rules::builtin();
        let index = 索引();
        let naming = Naming {
            rules: &rules,
            index: Some(&index),
            tuning: zh::Tuning::default(),
        };
        candidates(
            &naming,
            &变体("nds/游戏.7z", platform),
            &[Named::own(name)],
            platform,
            year,
        )
    }

    #[test]
    fn 剥完之后撞得上而且带着依据() {
        let found = 撞("合金弹头7[某汉化组](简)(64Mb).7z", Some("NDS"), None);
        assert_eq!(found.candidates.len(), 1);
        let candidate = &found.candidates[0];
        assert_eq!(candidate.game, "合金弹头7");
        assert_eq!(candidate.source, SOURCE);
        // 依据要说得出：从哪个名字剥的、剥掉了什么、撞上了哪条条目、两道校验各是什么。
        assert!(candidate.evidence.contains("按剥离规则剥成「合金弹头7」"));
        assert!(candidate.evidence.contains("汉化组「某汉化组」"));
        assert!(candidate.evidence.contains("条目 4"));
        assert!(candidate.evidence.contains("平台交叉校验对得上"));
        assert!(candidate.evidence.contains("年份交叉校验说不出"));
    }

    #[test]
    fn 这一层永不自动通过() {
        // ADR-0002：模糊匹配的结论一律进待确认队列。
        let found = 撞("合金弹头7.7z", Some("NDS"), Some(2008));
        assert!(!found.candidates[0].accepted);
        // 两道校验都对上也只到**中置信**——高置信留给精确命中与裁决。
        assert_eq!(found.candidates[0].confidence, Confidence::Medium);
        assert_eq!(found.strong, 1);
    }

    #[test]
    fn 只是像而两道校验没都对上的只到低置信() {
        let found = 撞("合金弹头7代.7z", Some("NDS"), None);
        assert_eq!(found.candidates[0].confidence, Confidence::Low);
        assert!(found.candidates[0].evidence.contains("没有都对上"));
        assert_eq!(found.strong, 0);
    }

    #[test]
    fn 平台对不上的一条都不产出() {
        // 不是降档是不产出（`zh::Index::lookup` 那一层就挡掉了）。
        assert!(撞("合金弹头7.7z", Some("GBA"), None).candidates.is_empty());
    }

    #[test]
    fn 名字还是乱码的不撞() {
        // 票 03 有损转换留下的替换字符。名字开始参与匹配之后，那批乱码就是实打实的问题。
        let found = 撞("\u{fffd}\u{fffd}\u{fffd}\u{fffd}.nes", Some("NDS"), None);
        assert_eq!(found.garbled, 1);
        assert_eq!(found.tried, 0);
        assert!(found.candidates.is_empty());
    }

    #[test]
    fn 认不出的记号收上来了() {
        let found = 撞("合金弹头7[某个没见过的记号].7z", Some("NDS"), None);
        assert_eq!(found.unknown, vec!["某个没见过的记号"]);
    }

    #[test]
    fn 同一条条目被两个名字撞上只留一条() {
        let rules = Rules::builtin();
        let index = 索引();
        let naming = Naming {
            rules: &rules,
            index: Some(&index),
            tuning: zh::Tuning::default(),
        };
        let found = candidates(
            &naming,
            &变体("nds/合金弹头7/游戏.7z", Some("NDS")),
            &[
                Named::own("合金弹头7.7z"),
                Named::directory("合金弹头7[汉化]"),
            ],
            Some("NDS"),
            None,
        );
        assert_eq!(found.candidates.len(), 1);
    }

    #[test]
    fn 没有索引时这一层整个不跑() {
        let rules = Rules::builtin();
        let naming = Naming::new(&rules);
        assert!(!naming.ready());
        let found = candidates(
            &naming,
            &变体("nds/合金弹头7.7z", Some("NDS")),
            &[Named::own("合金弹头7.7z")],
            Some("NDS"),
            None,
        );
        assert!(found.candidates.is_empty());
        assert_eq!(found.tried, 0);
    }

    #[test]
    fn 中文记号那一列留空() {
        // 这一列说的是「DAT 那条条目说自己是汉化版还是官中版」，而中文数据源
        // 对这件事一个字都没说。从这儿灌进去会让票 10 那份中文占比统计当场失真。
        let found = 撞("合金弹头7[某汉化组].7z", Some("NDS"), None);
        assert_eq!(found.candidates[0].chinese, None);
    }
}
