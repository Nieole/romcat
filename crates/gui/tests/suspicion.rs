//! **疑似同一作品**在屏上（票 `gui-looks-like-the-design/17`）。
//!
//! 这一层要钉住的是四件事，每一件不这么做界面就会骗人：
//!
//! 1. **左栏「整理建议」那颗标签的数出自核心库**，而且**数与列出来的组对得上**——
//!    按下去之后表里剩几行，由那几条建议牵着的作品说了算。
//! 2. **理由逐条列在屏上**，而且逐字就是核心库交回来的那几句
//!    （`same_work::Clue::sentence`）：界面不自己比一遍名字、比一遍年份（ADR-0024）。
//! 3. **「不是同一个」按下去就不再提**，而且屏上那句回话**不说「记为裁决」**——
//!    它落的是沉淀库里自己那张表，撤销不走裁决记录（同挂单 `Q1032` 在平台纠正上的裁定）。
//! 4. **界面里没有新长出的领域判断**：屏上那几句与核心库当场算出来的逐字相等。
//!
//! 断言挂在**画出来的字**上（`shared::画出来的字`），不看数据结构。

use romcat_core::catalog::identify::{Identification, Provenance};
use romcat_core::catalog::scrape::{Harvested, HarvestedValue};
use romcat_core::catalog::title::TitleRow;
use romcat_core::catalog::{Catalog, Confidence, State};
use romcat_core::platform::Manifest;
use romcat_core::scrape::{AnchorKind, Field};
use romcat_core::shape::{Role, Variant};
use romcat_core::site::Site;
use romcat_core::title::{Language, TitleKind};
use romcat_core::triage::same_work;
use romcat_core::verdict::Store;
use romcat_gui::app::{App, View};
use romcat_gui::browse::suspicion;
use romcat_gui::headless;

mod shared;
use shared::{点一下, 跑一帧};

const 根: &str = "主库";
/// 这一对疑似是同一个：两边的名字在中文离线源里指向同一条条目，平台与年份也一致。
const 甲: &str = "Pocket Monsters - Aka (Japan)";
const 乙: &str = "Pocket Monster - Red Version (Japan)";
/// 屏上印的是**显示标题**，不是作品名。
const 甲的译名: &str = "精灵宝可梦 红";
const 乙的译名: &str = "口袋妖怪 红";
/// 完全不参与这件事的第三个作品。
const 丙: &str = "Seiken Densetsu 2 (Japan)";
/// 两边撞上的那条中文条目。
const 条目: u32 = 4312;

fn 键(名字: &str) -> String {
    format!("{根}/GB/{名字}.zip")
}

fn 变体(名字: &str) -> Variant {
    let key = 键(名字);
    Variant {
        key: key.clone(),
        platform: Some("GB".to_string()),
        rule: "一文件一变体".into(),
        main_key: key.clone(),
        manual: false,
        files: 1,
        bytes: 1024 * 1024,
        unreadable_files: 0,
        members: vec![(key, Role::Main)],
    }
}

/// 一条**中文离线源**落下的叫法，依据里写着条目号——认它的是核心库那一处
/// （`zh::entry_in`），这里照那个记号拼。
fn 撞上(名字: &str, 叫作: &str) -> Harvested {
    Harvested {
        anchor: AnchorKind::Variant.label().to_string(),
        subject: 键(名字),
        source: "中文离线源".to_string(),
        input: "夹具".to_string(),
        values: vec![HarvestedValue {
            field: Field::Title.label().to_string(),
            value: 叫作.to_string(),
            evidence: format!(
                "「{叫作}」撞上了中文离线源{}{条目}：名字一字不差",
                romcat_core::zh::ENTRY_MARK
            ),
        }],
        media: Vec::new(),
    }
}

fn 叫法(作品: &str, 值: &str) -> TitleRow {
    TitleRow {
        work: 作品.to_string(),
        value: 值.to_string(),
        language: Language::Chinese,
        kind: TitleKind::Translated,
        source: "中文离线源".to_string(),
        region: None,
        variant_key: None,
        confidence: Confidence::High,
        seam: None,
        evidence: "夹具".to_string(),
        seen: 1,
    }
}

/// 三个作品：甲乙疑似是同一个，丙不参与。
fn 界面() -> App {
    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    romcat_core::catalog::roots::add_root(&catalog, None, 根, std::path::Path::new("/主库"))
        .expect("建得出根");
    let variants: Vec<Variant> = [甲, 乙, 丙].iter().map(|名字| 变体(名字)).collect();
    catalog
        .replace_variants(&variants, 1, &Manifest::default())
        .expect("写得进变体");

    let mut records = Vec::new();
    for 名字 in [甲, 乙, 丙] {
        let work = catalog
            .add_work(名字, Provenance::Identified)
            .expect("建得出作品");
        records.push(Identification {
            variant_key: 键(名字),
            state: State::Matched,
            reason: None,
            platform: Some("GB".to_string()),
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
    for (名字, 年份) in [(甲, "1996"), (乙, "1996"), (丙, "1993")] {
        catalog
            .put_verdict_value(AnchorKind::Work, 名字, Field::Year, 年份, "夹具")
            .expect("写得进年份");
    }
    catalog
        .put_scraped(&[撞上(甲, 甲的译名), 撞上(乙, 乙的译名)])
        .expect("写得进刮削值");
    catalog
        .put_titles(&[叫法(甲, 甲的译名), 叫法(乙, 乙的译名)])
        .expect("写得进标题集合");

    let site = Site::in_memory(catalog, Store::in_memory().expect("开得出沉淀库"), 根);
    let mut app = App::new(site, shared::干净工作目录("romcat-测试-整理建议"));
    app.show_view(View::Browse);
    app
}

/// 核心库当场算出来的那一条建议：屏上那几句得与它逐字相等。
fn 核心库怎么说(app: &mut App) -> Vec<String> {
    let (_, site) = app.browse_and_site();
    let found =
        same_work::survey(&site.catalog, &site.store, &site.library_identity).expect("扫得动");
    assert_eq!(found.len(), 1, "夹具该摆出正好一对：{found:?}");
    found[0].reasons()
}

#[test]
fn 左栏整理建议那颗标签的数出自核心库_按下去之后表里只剩那一对() {
    let ctx = headless::context();
    let mut app = 界面();
    let 屏上 = 跑一帧(&ctx, |ui| app.ui(ui));
    assert!(
        屏上.contains(suspicion::SECTION),
        "左栏没有整理建议那一簇：\n{屏上}"
    );
    assert!(
        屏上.contains(suspicion::FACET) && 屏上.contains(&suspicion::groups(1)),
        "那颗标签上没写「{} {}」：\n{屏上}",
        suspicion::FACET,
        suspicion::groups(1),
    );
    // 它与「识别结论」同一个处境：只用于浏览、不写进规则——屏上说出来（票 12 验收第 3 条）。
    assert!(
        屏上.contains(suspicion::BROWSE_ONLY),
        "没说清它不写进规则：\n{屏上}"
    );

    let 屏上 = 点一下(&ctx, suspicion::FACET, |ui| app.ui(ui));
    // **数与列出来的组对得上**：一组两个作品，表里就该只剩这两行。
    assert!(
        屏上.contains(甲的译名) && 屏上.contains(乙的译名),
        "按下那颗标签之后，那一对没都列出来：\n{屏上}"
    );
    assert!(
        !屏上.contains(丙),
        "按下那颗标签之后，不在建议里的作品还列着：\n{屏上}"
    );
}

#[test]
fn 建议卡上的理由逐条列出来_逐字就是核心库交回来的那几句() {
    let ctx = headless::context();
    let mut app = 界面();
    let 理由 = 核心库怎么说(&mut app);
    跑一帧(&ctx, |ui| app.ui(ui));
    点一下(&ctx, suspicion::FACET, |ui| app.ui(ui));
    let 屏上 = 点一下(&ctx, 乙的译名, |ui| app.ui(ui));

    assert!(
        屏上.contains(&format!("可能与《{甲的译名}》是同一个作品")),
        "点开那一行，侧边详情里没有建议卡：\n{屏上}"
    );
    // **界面不自己比一遍名字、比一遍年份**（ADR-0024）：屏上那几句与核心库当场算出来的
    // 逐字相等。界面若自己长出一套说法，这一条当场红。
    for 一句 in &理由 {
        assert!(
            屏上.contains(一句.as_str()),
            "核心库交回来的这一句没画在屏上：{一句}\n{屏上}"
        );
    }
    assert!(
        理由.iter().any(|句| 句.contains(&条目.to_string())),
        "理由里该说出撞的是哪一条中文条目：{理由:?}"
    );
    assert!(
        屏上.contains(suspicion::NOT_SAME),
        "卡上没有那颗按钮：\n{屏上}"
    );
}

#[test]
fn 按不是同一个之后那一对不再提_那句回话不说记为裁决() {
    let ctx = headless::context();
    let mut app = 界面();
    跑一帧(&ctx, |ui| app.ui(ui));
    点一下(&ctx, suspicion::FACET, |ui| app.ui(ui));
    点一下(&ctx, 乙的译名, |ui| app.ui(ui));
    点一下(&ctx, suspicion::NOT_SAME, |ui| app.ui(ui));
    // 回执浮在窗口底边那条提示条上，下一帧才画得出来。
    let 屏上 = 跑一帧(&ctx, |ui| app.ui(ui));

    assert!(
        屏上.contains(suspicion::NOT_SAME_SAID),
        "按了「{}」，屏上没回一句：\n{屏上}",
        suspicion::NOT_SAME,
    );
    // **不说「记为裁决」**：它落的是沉淀库里自己那张表，撤销不走裁决记录
    // （挂单 `Q1032` 在平台纠正上裁过同一件事）。
    assert!(
        !屏上.contains("记为裁决"),
        "那句回话说成了裁决——它不是一条裁决：\n{屏上}"
    );
    assert!(
        屏上.contains(&suspicion::groups(0)),
        "记过「不是同一个」之后那颗标签的数没降：\n{屏上}"
    );
    assert!(
        !屏上.contains("可能与"),
        "记过「不是同一个」之后那张卡还画着：\n{屏上}"
    );
    // **空态那一句得说实话**：库里三个作品都还在，一行都不剩是**这颗标签筛没的**
    // （`WorkQuery::same_filter` 得把它算进筛选那一边）。
    assert!(
        屏上.contains("没有符合当前筛选条件的作品。"),
        "一行都不剩时屏上该说是筛没的、旁边给一颗「清除筛选」：\n{屏上}"
    );
    assert!(
        !屏上.contains("库里还没有能列出来的东西"),
        "库里明明还有三个作品，屏上却说库里没东西：\n{屏上}"
    );

    // **沉淀库里真记下了**：撤掉那一条，那一对回到「还没看过」。
    let (_, site) = app.browse_and_site();
    assert!(
        same_work::undo_not_same(&mut site.store, &site.library_identity.clone(), 甲, 乙)
            .expect("撤得掉"),
        "屏上按下去的那一下没落进沉淀库"
    );
}

#[test]
fn 作品详情页概览那一面上也摆着同一张建议卡() {
    // 票面：「在浏览**与作品详情**里看到」。两处画的是同一份（`browse::suspicion::cards`）。
    let ctx = headless::context();
    let mut app = 界面();
    let 理由 = 核心库怎么说(&mut app);
    跑一帧(&ctx, |ui| app.ui(ui));
    点一下(&ctx, suspicion::FACET, |ui| app.ui(ui));
    点一下(&ctx, 乙的译名, |ui| app.ui(ui));
    let 屏上 = 点一下(&ctx, "查看详情", |ui| app.ui(ui));

    assert!(
        屏上.contains(&format!("可能与《{甲的译名}》是同一个作品")),
        "作品详情页概览那一面上没有建议卡：\n{屏上}"
    );
    for 一句 in &理由 {
        assert!(
            屏上.contains(一句.as_str()),
            "详情页上这一句没画出来：{一句}\n{屏上}"
        );
    }
}

#[test]
fn 认不出作品的那一行不摆建议卡() {
    // 合并的两侧都得说得出作品名，所以没有作品链接的那一行不参与这件事。
    let ctx = headless::context();
    let mut app = 界面();
    assert!(!核心库怎么说(&mut app).is_empty(), "夹具里该有建议");
    跑一帧(&ctx, |ui| app.ui(ui));
    let 屏上 = 点一下(&ctx, 丙, |ui| app.ui(ui));
    assert!(
        !屏上.contains("可能与"),
        "不在任何一条建议里的作品身上摆了建议卡：\n{屏上}"
    );
}
