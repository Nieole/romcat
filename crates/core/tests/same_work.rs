//! **疑似同一作品**：哪两个作品其实是同一个（票 `gui-looks-like-the-design/17`）。
//!
//! 这一层要钉住的是五件事，每一件不这么做界面就会骗人：
//!
//! 1. **判断在核心库这一处**：建议连**理由**一起交出来，界面只印——不自己比一遍名字、
//!    比一遍年份（ADR-0024）。
//! 2. **两条线索各自立得了案**：名字归一之后撞上、两边指向同一条中文条目。
//! 3. **阈值挡得住两类假阳性**：只有「平台与年份一致」的那一对不提；连一个平台都不共有的
//!    那一对不提。
//! 4. **「不是同一个」记下来就不再提**，而且撤得掉。
//! 5. **同一份库跑两遍交回来的是同一份**——屏上那个数不会自己跳。
//!
//! 断言挂在**规矩**上：问的是「这一对在不在建议里」「那几句理由说得出什么」，
//! 不是「第几条是谁」。

use romcat_core::catalog::identify::{Identification, Provenance};
use romcat_core::catalog::scrape::{Harvested, HarvestedValue};
use romcat_core::catalog::title::TitleRow;
use romcat_core::catalog::{Catalog, Confidence, State};
use romcat_core::identify::fuzzy;
use romcat_core::platform::Manifest;
use romcat_core::scrape::{AnchorKind, Field};
use romcat_core::shape::{Role, Variant};
use romcat_core::title::{Language, TitleKind};
use romcat_core::triage::same_work::{self, Clue, Suspicion};
use romcat_core::verdict::Store;
use romcat_core::zh;

const 主库标识: &str = "小库";

/// 这一对该被提出来：两边的名字指向**同一条中文条目**，平台与年份也一致。
const 甲: &str = "口袋妖怪 红";
const 乙: &str = "精灵宝可梦 红";
/// 这一对该被提出来：两个名字**归一之后是同一串字**（差的只是那个破折号）。
const 丙: &str = "塞尔达传说 - 时空之章";
const 丁: &str = "塞尔达传说：时空之章";
/// 这一对**不该**被提出来：平台与年份都一致，可没有一条立得了案的线索。
const 戊: &str = "超级机器人大战J";
const 己: &str = "超级机器人大战R";
/// 这一对**不该**被提出来：指向同一条中文条目，可连一个平台都不共有。
const 庚: &str = "幻想传说";
const 辛: &str = "幻想传说 重制版";

/// 甲、乙两边撞上的那条中文条目。
const 条目: u32 = 4312;
/// 庚、辛两边撞上的那条中文条目（它们在两个平台上）。
const 另一条目: u32 = 9001;

fn 键(平台: &str, 名字: &str) -> String {
    format!("主库/{平台}/{名字}.zip")
}

fn 变体(平台: &str, 名字: &str) -> Variant {
    let key = 键(平台, 名字);
    Variant {
        key: key.clone(),
        platform: Some(平台.to_string()),
        rule: "一文件一变体".into(),
        main_key: key.clone(),
        manual: false,
        files: 1,
        bytes: 4_096,
        unreadable_files: 0,
        members: vec![(key, Role::Main)],
    }
}

/// 一条**中文离线源**落下的叫法，依据里写着条目号——认条目号的是
/// `zh::entry_in`，这里照它那个记号拼，不自己编一套格式。
fn 中文叫法(平台: &str, 名字: &str, 叫作: &str, 撞上的条目: u32) -> Harvested {
    Harvested {
        anchor: AnchorKind::Variant.label().to_string(),
        subject: 键(平台, 名字),
        source: fuzzy::SOURCE.to_string(),
        input: format!("夹具：{名字}"),
        values: vec![HarvestedValue {
            field: Field::Title.label().to_string(),
            value: 叫作.to_string(),
            evidence: format!(
                "「{名字}」撞上了中文离线源{}{撞上的条目}：名字一字不差",
                zh::ENTRY_MARK
            ),
        }],
        media: Vec::new(),
    }
}

struct 现场 {
    catalog: Catalog,
    store: Store,
}

/// 同一串字底下再挤几个作品（[`建现场_`] 的那个参数）：防炸那道闸的门槛是 8。
const 挤: usize = 9;

/// 那一堆里第 `n` 个叫什么。
fn 挤的名字(n: usize) -> String {
    format!("某个太泛的名字 第{n}作")
}

/// 一份合成的小库：四对作品，各自摆成上面那四种情形。
fn 建现场() -> 现场 {
    建现场_(false)
}

/// 同上；`太挤` 为真时再摆 [`挤`] 个作品，它们各有一条**一模一样**的叫法。
fn 建现场_(太挤: bool) -> 现场 {
    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    let mut 摆法: Vec<(&str, String)> = [
        ("GB", 甲),
        ("GB", 乙),
        ("GB", 丙),
        ("GB", 丁),
        ("GBA", 戊),
        ("GBA", 己),
        ("SFC", 庚),
        ("PS1", 辛),
    ]
    .iter()
    .map(|(平台, 名字)| (*平台, (*名字).to_string()))
    .collect();
    if 太挤 {
        for n in 0..挤 {
            摆法.push(("GB", 挤的名字(n)));
        }
    }
    let variants: Vec<Variant> = 摆法.iter().map(|(平台, 名字)| 变体(平台, 名字)).collect();
    catalog
        .replace_variants(&variants, 1, &Manifest::default())
        .expect("写得进变体");

    let mut records = Vec::new();
    for (平台, 名字) in &摆法 {
        let work = catalog
            .add_work(名字, Provenance::Identified)
            .expect("建得出作品");
        records.push(Identification {
            variant_key: 键(平台, 名字),
            state: State::Matched,
            reason: None,
            platform: Some((*平台).to_string()),
            standalone: None,
            edition: None,
            units: 1,
            nkit: 0,
            read_bytes: 0,
            work_id: Some(work),
            release_id: None,
            candidates: Vec::new(),
        });
    }
    catalog
        .write_identifications(&records)
        .expect("写得进识别结论");

    // **年份**：甲乙一致（佐证得出得来）、戊己也一致（可它们没有立得了案的线索）。
    for (名字, 年份) in [
        (甲, "1996"),
        (乙, "1996"),
        (戊, "2005"),
        (己, "2005"),
        (庚, "1995"),
        (辛, "1998"),
    ] {
        catalog
            .put_verdict_value(AnchorKind::Work, 名字, Field::Year, 年份, "夹具")
            .expect("写得进年份");
    }

    // **中文离线源那几次匹配**：甲乙撞的是同一条，庚辛撞的是另一条同一条。
    catalog
        .put_scraped(&[
            中文叫法("GB", 甲, 甲, 条目),
            中文叫法("GB", 乙, 乙, 条目),
            中文叫法("SFC", 庚, 庚, 另一条目),
            中文叫法("PS1", 辛, 庚, 另一条目),
        ])
        .expect("写得进刮削值");

    // **DAT 那两个数据库对丙各有一个叫法**：它们与丁的作品名归一之后是同一串字。
    let mut 叫法们 = vec![叫法(丙, "塞尔达传说·时空之章", "No-Intro")];
    if 太挤 {
        // 挤在一堆的那几个各有一条**一模一样**的叫法：归一之后撞在同一串字上。
        for n in 0..挤 {
            叫法们.push(叫法(&挤的名字(n), "同一串字", "No-Intro"));
        }
    }
    catalog.put_titles(&叫法们).expect("写得进标题集合");

    现场 {
        catalog,
        store: Store::in_memory().expect("开得出沉淀库"),
    }
}

fn 叫法(作品: &str, 值: &str, 源: &str) -> TitleRow {
    TitleRow {
        work: 作品.to_string(),
        value: 值.to_string(),
        language: Language::Chinese,
        kind: TitleKind::Official,
        source: 源.to_string(),
        region: None,
        variant_key: None,
        confidence: Confidence::High,
        seam: None,
        evidence: "夹具".to_string(),
        seen: 1,
    }
}

fn 扫一趟(现场: &现场) -> Vec<Suspicion> {
    same_work::survey(&现场.catalog, &现场.store, 主库标识).expect("扫得动")
}

fn 这一对(建议: &[Suspicion], a: &str, b: &str) -> Option<Suspicion> {
    建议
        .iter()
        .find(|one| one.works == romcat_core::verdict::work_pair(a, b))
        .cloned()
}

#[test]
fn 两边指向同一条中文条目那一对提得出来_理由逐条说得出() {
    let 现场 = 建现场();
    let 建议 = 扫一趟(&现场);
    let 那一对 = 这一对(&建议, 甲, 乙).expect("甲乙该被提出来");

    let 理由 = 那一对.reasons();
    assert!(
        理由.iter().any(|句| 句.contains(&条目.to_string())),
        "理由里得说出撞的是哪一条中文条目：{理由:?}",
    );
    assert!(
        理由
            .iter()
            .any(|句| 句.contains("1996") && 句.contains("GB")),
        "平台与年份一致该当佐证列出来：{理由:?}",
    );
    assert!(
        那一对.clues.iter().any(Clue::makes_a_case),
        "至少得有一条立得了案的线索：{:?}",
        那一对.clues,
    );
    assert_eq!(
        那一对.other_than(甲),
        Some(乙),
        "屏上那句「可能与……」写的是它"
    );
}

#[test]
fn 名字归一之后撞上的那一对也提得出来() {
    let 现场 = 建现场();
    let 那一对 = 这一对(&扫一趟(&现场), 丙, 丁).expect("丙丁该被提出来");
    let 理由 = 那一对.reasons();
    assert!(
        理由.iter().any(|句| 句.contains("同一串字")),
        "名字撞上那一条该说清凭什么：{理由:?}",
    );
    assert!(
        理由.iter().any(|句| 句.contains("No-Intro")),
        "出处得写出来——「不同数据库对这部作品的命名不同」正是这一条：{理由:?}",
    );
}

#[test]
fn 只有平台与年份一致的那一对不提() {
    let 现场 = 建现场();
    assert!(
        这一对(&扫一趟(&现场), 戊, 己).is_none(),
        "同一年同一平台的游戏成百上千，只凭这一条不许提",
    );
}

#[test]
fn 连一个平台都不共有的那一对不提() {
    let 现场 = 建现场();
    assert!(
        这一对(&扫一趟(&现场), 庚, 辛).is_none(),
        "两个作品连一个平台都不共有时，「被识别成了两个作品」这句话说不通",
    );
}

#[test]
fn 说过不是同一个之后不再提_撤掉又回来() {
    let mut 现场 = 建现场();
    assert!(这一对(&扫一趟(&现场), 甲, 乙).is_some(), "先得提得出来");

    same_work::not_same(&mut 现场.store, 主库标识, 乙, 甲).expect("记得下");
    assert!(
        这一对(&扫一趟(&现场), 甲, 乙).is_none(),
        "记过「不是同一个」之后不许再提（次序反着给也认得出同一对）",
    );

    assert!(
        same_work::undo_not_same(&mut 现场.store, 主库标识, 甲, 乙).expect("撤得掉"),
        "撤的时候得说得出原来有这一条",
    );
    assert!(
        这一对(&扫一趟(&现场), 甲, 乙).is_some(),
        "撤掉之后这一对回到「还没看过」",
    );
}

#[test]
fn 说过不是同一个只管这一份主库() {
    let mut 现场 = 建现场();
    same_work::not_same(&mut 现场.store, "另一份库", 甲, 乙).expect("记得下");
    assert!(
        这一对(&扫一趟(&现场), 甲, 乙).is_some(),
        "作品名只在一份主库的中立库里成立，别人库里的决定管不到这儿",
    );
}

#[test]
fn 同一串字底下挤着太多作品时整堆不提() {
    // 防炸的那道闸：一堆里两两成对是 k(k-1)/2。真到那个数也不是「两个数据库对同一部
    // 作品的命名差异」，是某个名字太泛——不该由这一处来问人。
    let 现场 = 建现场_(true);
    let 建议 = 扫一趟(&现场);
    let 挤的那些: Vec<String> = (0..挤).map(挤的名字).collect();
    assert!(
        建议
            .iter()
            .all(|one| !one.works.iter().any(|名字| 挤的那些.contains(名字))),
        "挤在一堆的那些不该被两两配对：{建议:?}",
    );
    // 原先那两对照旧提得出来——这道闸只挡那一堆。
    assert!(这一对(&建议, 甲, 乙).is_some(), "别的对不该被连累");
    assert!(这一对(&建议, 丙, 丁).is_some(), "别的对不该被连累");
}

#[test]
fn 同一份库跑两遍交回来的是同一份() {
    let 现场 = 建现场();
    assert_eq!(扫一趟(&现场), 扫一趟(&现场), "屏上那个数不许自己跳");
}
