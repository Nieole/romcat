//! **优先级表写回工作目录**，与**换一份表之前说得出哪几处显示值会变**
//! （票 `gui-looks-like-the-design/30`）。
//!
//! 往返那一条（导出成文本再读回来一模一样）在 `scrape::priority` 自己的单元测试里：
//! 它要碰私有的 `parse`。这里只走公开的那几个函数，与命令行、界面走的同一条路。

use std::fs;

use romcat_core::catalog::identify::{Identification, Provenance};
use romcat_core::catalog::scrape::{Harvested, HarvestedValue};
use romcat_core::catalog::{Catalog, Confidence, State, TitleRow};
use romcat_core::platform::Manifest;
use romcat_core::scrape::priority::{self, FieldShifts, Said, Shift};
use romcat_core::scrape::{AnchorKind, Priorities};
use romcat_core::shape::{Role, SINGLE_FILE_RULE, Variant};
use romcat_core::testing::temp_dir;
use romcat_core::title::{Language, TitleKind};
use romcat_core::{sync, workspace};

/// 把一段手写的表落进 `dir` 底下的 `名字` 再读回来：公开的入口只有 `load`。
fn 读一份(dir: &std::path::Path, 名字: &str, text: &str) -> Priorities {
    let path = dir.join(名字);
    fs::write(&path, text).expect("写得进测试用的表");
    Priorities::load(&path).expect("手写的表读得动")
}

#[test]
fn 写回工作目录那份之后_读的那一侧立即读到新的() {
    let 工作目录 = temp_dir("优先级-写回");
    let 草稿 = temp_dir("优先级-写回-草稿");
    let 改过的 = 读一份(
        草稿.path(),
        "改过的.toml",
        "\"版本\" = 1\n[[\"字段\"]]\n\"名\" = \"简介\"\n\
         \"顺序\" = [\"裁决\", \"Pegasus\", \"ES-Gamelist\", \"中文离线源\", \"ScreenScraper\"]\n",
    );

    // 还没写回时，读的那一侧拿到的是内置那份。
    assert_eq!(
        sync::prepare::priorities(None, 工作目录.path()).expect("读得动"),
        Priorities::builtin()
    );

    改过的
        .save(&workspace::priorities_path(工作目录.path()))
        .expect("写得进工作目录");
    assert_eq!(
        sync::prepare::priorities(None, 工作目录.path()).expect("读得动"),
        改过的,
        "写回之后读的那一侧没读到新的那份"
    );

    // 再写一次是**换上去**：读到的是最新那份，目录里也只有那一份，没留下写到一半的。
    Priorities::builtin()
        .save(&workspace::priorities_path(工作目录.path()))
        .expect("换得上去");
    assert_eq!(
        sync::prepare::priorities(None, 工作目录.path()).expect("读得动"),
        Priorities::builtin()
    );
    let 目录里有的: Vec<String> = fs::read_dir(工作目录.path())
        .expect("列得开")
        .map(|entry| {
            entry
                .expect("读得出")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(目录里有的, ["priorities.toml"]);
}

// ── 换一份表，哪几处显示值会变 ─────────────────────────────────────────────

const 根: &str = "主库";

fn 变体(相对路径: &str) -> Variant {
    let key = format!("{根}/{相对路径}");
    Variant {
        main_key: key.clone(),
        platform: 相对路径.split('/').next().map(ToString::to_string),
        rule: SINGLE_FILE_RULE.to_string(),
        manual: false,
        files: 1,
        bytes: 4096,
        unreadable_files: 0,
        members: vec![(key.clone(), Role::Main)],
        key,
    }
}

fn 采到(
    anchor: AnchorKind, subject: &str, source: &str, 字段和值: &[(&str, &str)]
) -> Harvested {
    Harvested {
        anchor: anchor.label().to_string(),
        subject: subject.to_string(),
        source: source.to_string(),
        input: "测试".to_string(),
        values: 字段和值
            .iter()
            .map(|(field, value)| HarvestedValue {
                field: (*field).to_string(),
                value: (*value).to_string(),
                evidence: "手写的".to_string(),
            })
            .collect(),
        media: Vec::new(),
    }
}

fn 英文官方名(work: &str, value: &str, source: &str) -> TitleRow {
    TitleRow {
        work: work.to_string(),
        value: value.to_string(),
        language: Language::English,
        kind: TitleKind::Official,
        source: source.to_string(),
        region: None,
        variant_key: None,
        confidence: Confidence::High,
        seam: None,
        evidence: "手写的".to_string(),
        seen: 1,
    }
}

fn 说(source: &str, value: &str) -> Said {
    Said {
        source: Some(source.to_string()),
        values: vec![value.to_string()],
    }
}

/// 一份小库，每一处都是冲着「导出真会写出去的是什么」摆的：
///
/// - 「幻想传说」（FC）名下两个变体，`幻想传说-1` 是首选（同一档按键排）：
///   - 作品上的简介两家各说一句——简介换顺序就该变；
///   - 作品上的年份两家说的一样——换谁说了算屏上看不出来，不算；
///   - 两个变体上的汉化组各有两家——**只有首选那个的算**，导出只从它取；
///   - 非首选那个变体上还挂着变体级的简介——**不算**，认出了作品的条目只读作品上的简介。
/// - 「只有一家」的简介只有一个源——换顺序它不变。
/// - 「双年份」的年份：同一个源说了两个年份，另一个源说的与它挑出来的那一个一样——
///   导出只写挑出来的那一条，**不算**。
/// - 「超时空之轮」（SFC）的标题集合里两条同一档的英文官方名，一条 No-Intro、一条 TOSEC——
///   通用那条里两家换了先后，显示标题就换了。
/// - FC 与街机各一个**没认出作品**的变体，No-Intro 与 MAME 各给一个标题——通用那条换了，
///   FC 那个变、街机那个不变（街机那条覆盖整条替换通用顺序）。
/// - 街机 `BIOS/` 底下一个没认出作品的**非游戏资产**，两家各给一个标题——它不导出，**不算**。
fn 小库() -> Catalog {
    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    romcat_core::catalog::roots::add_root(&catalog, None, 根, std::path::Path::new("/主库"))
        .expect("建得出根");
    let 名下 = [
        ("FC/幻想传说-1.nes", "幻想传说"),
        ("FC/幻想传说-2.nes", "幻想传说"),
        ("FC/只有一家.nes", "只有一家"),
        ("FC/双年份.nes", "双年份"),
        ("SFC/超时空之轮.sfc", "超时空之轮"),
    ];
    let 散的 = ["FC/魂斗罗.nes", "街机/contra.zip", "街机/BIOS/neogeo.zip"];
    let variants: Vec<Variant> = 名下
        .iter()
        .map(|(path, _)| *path)
        .chain(散的)
        .map(变体)
        .collect();
    catalog
        .replace_variants(&variants, 1, &Manifest::default())
        .expect("写得进变体");

    let mut 作品号 = std::collections::BTreeMap::new();
    let mut 结论 = Vec::new();
    for (path, work) in 名下 {
        let work_id = match 作品号.get(work) {
            Some(id) => *id,
            None => {
                let id = catalog
                    .add_work(work, Provenance::Identified)
                    .expect("建得出作品");
                作品号.insert(work, id);
                id
            }
        };
        结论.push(Identification {
            variant_key: format!("{根}/{path}"),
            platform: None,
            standalone: None,
            state: State::Matched,
            reason: None,
            units: 1,
            nkit: 0,
            read_bytes: 0,
            work_id: Some(work_id),
            release_id: None,
            candidates: Vec::new(),
        });
    }
    catalog
        .write_identifications(&结论)
        .expect("写得进识别结论");

    catalog
        .put_scraped(&[
            采到(
                AnchorKind::Work,
                "幻想传说",
                "ScreenScraper",
                &[("简介", "An RPG"), ("年份", "1995")],
            ),
            采到(
                AnchorKind::Work,
                "幻想传说",
                "中文离线源",
                &[("简介", "中文简介")],
            ),
            采到(AnchorKind::Work, "幻想传说", "TOSEC", &[("年份", "1995")]),
            采到(
                AnchorKind::Variant,
                "主库/FC/幻想传说-1.nes",
                "TOSEC",
                &[("汉化组", "甲组")],
            ),
            采到(
                AnchorKind::Variant,
                "主库/FC/幻想传说-1.nes",
                "文件名",
                &[("汉化组", "乙组")],
            ),
            采到(
                AnchorKind::Variant,
                "主库/FC/幻想传说-2.nes",
                "TOSEC",
                &[("汉化组", "丙组"), ("简介", "变体上的英文简介")],
            ),
            采到(
                AnchorKind::Variant,
                "主库/FC/幻想传说-2.nes",
                "文件名",
                &[("汉化组", "丁组")],
            ),
            采到(
                AnchorKind::Variant,
                "主库/FC/幻想传说-2.nes",
                "中文离线源",
                &[("简介", "变体上的中文简介")],
            ),
            采到(
                AnchorKind::Work,
                "只有一家",
                "ScreenScraper",
                &[("简介", "Only one")],
            ),
            采到(
                AnchorKind::Work,
                "双年份",
                "ScreenScraper",
                &[("年份", "1990"), ("年份", "1991")],
            ),
            采到(AnchorKind::Work, "双年份", "TOSEC", &[("年份", "1990")]),
            采到(
                AnchorKind::Variant,
                "主库/FC/魂斗罗.nes",
                "No-Intro",
                &[("标题", "Contra (USA)")],
            ),
            采到(
                AnchorKind::Variant,
                "主库/FC/魂斗罗.nes",
                "MAME",
                &[("标题", "Contra")],
            ),
            采到(
                AnchorKind::Variant,
                "主库/街机/contra.zip",
                "No-Intro",
                &[("标题", "Contra (USA)")],
            ),
            采到(
                AnchorKind::Variant,
                "主库/街机/contra.zip",
                "MAME",
                &[("标题", "Contra")],
            ),
            采到(
                AnchorKind::Variant,
                "主库/街机/BIOS/neogeo.zip",
                "No-Intro",
                &[("标题", "Neo Geo BIOS (USA)")],
            ),
            采到(
                AnchorKind::Variant,
                "主库/街机/BIOS/neogeo.zip",
                "MAME",
                &[("标题", "Neo Geo BIOS")],
            ),
        ])
        .expect("写得进刮削结果");
    catalog
        .put_titles(&[
            英文官方名("超时空之轮", "Chrono Trigger", "No-Intro"),
            英文官方名("超时空之轮", "Chrono Trigger (1995)(Square)", "TOSEC"),
        ])
        .expect("写得进标题集合");
    catalog
}

#[test]
fn 换一份表之前说得出哪几处显示值会变_照导出真会写出去的算() {
    let catalog = 小库();
    let 草稿 = temp_dir("优先级-变化");
    let 内置 = Priorities::builtin();
    // 标题：MAME 与 TOSEC 提到 No-Intro 前面；年份：ScreenScraper 提到 TOSEC 前面；
    // 汉化组：文件名排进来、压在 TOSEC 前面；简介：中文离线源提到 ScreenScraper 前面。
    // 发行商、开发商、类型与街机那条覆盖一个字不动。
    let 改过的 = 读一份(
        草稿.path(),
        "改过的.toml",
        r#""版本" = 1
[["字段"]]
"名" = "标题"
"顺序" = ["裁决", "Pegasus", "ES-Gamelist", "MAME", "TOSEC", "No-Intro", "Redump", "ScreenScraper", "GoodNES", "中文离线源", "文件名", "中文离线源·别名"]
[["字段"]]
"名" = "年份"
"顺序" = ["裁决", "Pegasus", "ES-Gamelist", "ScreenScraper", "TOSEC"]
[["字段"]]
"名" = "发行商"
"顺序" = ["裁决", "Pegasus", "ES-Gamelist", "TOSEC", "ScreenScraper", "中文离线源"]
[["字段"]]
"名" = "汉化组"
"顺序" = ["裁决", "Pegasus", "ES-Gamelist", "文件名", "TOSEC"]
[["字段"]]
"名" = "开发商"
"顺序" = ["裁决", "Pegasus", "ES-Gamelist", "ScreenScraper", "中文离线源"]
[["字段"]]
"名" = "类型"
"顺序" = ["裁决", "Pegasus", "ES-Gamelist", "ScreenScraper", "中文离线源"]
[["字段"]]
"名" = "简介"
"顺序" = ["裁决", "Pegasus", "ES-Gamelist", "中文离线源", "ScreenScraper"]
[["平台覆盖"]]
"平台" = "街机"
"字段" = "标题"
"顺序" = ["裁决", "Pegasus", "ES-Gamelist", "MAME", "No-Intro", "TOSEC", "文件名"]
"#,
    );

    let got = priority::shifts(&catalog, &内置, &改过的).expect("读得动库");
    assert_eq!(
        got,
        vec![
            // 平台照码位序走（FC 在 SFC 前），同一平台里作品在没认出作品的变体前面。
            // 「超时空之轮」在 SFC 上：MAME 在这张表里压过 TOSEC、TOSEC 压过 No-Intro，
            // 而它的标题集合里只有 No-Intro 与 TOSEC 两家——换成 TOSEC 那一条，算一个作品。
            FieldShifts {
                field: "标题".to_string(),
                works: 1,
                variants: 1,
                example: Some(Shift {
                    anchor: AnchorKind::Variant,
                    subject: "主库/FC/魂斗罗.nes".to_string(),
                    before: 说("No-Intro", "Contra (USA)"),
                    after: 说("MAME", "Contra"),
                }),
            },
            // 排得不一样，这份库上却一处都不变：照样列出来。
            FieldShifts {
                field: "年份".to_string(),
                ..FieldShifts::default()
            },
            FieldShifts {
                field: "简介".to_string(),
                works: 1,
                variants: 0,
                example: Some(Shift {
                    anchor: AnchorKind::Work,
                    subject: "幻想传说".to_string(),
                    before: 说("ScreenScraper", "An RPG"),
                    after: 说("中文离线源", "中文简介"),
                }),
            },
            FieldShifts {
                field: "汉化组".to_string(),
                works: 1,
                variants: 0,
                example: Some(Shift {
                    anchor: AnchorKind::Work,
                    subject: "幻想传说".to_string(),
                    before: 说("TOSEC", "甲组"),
                    after: 说("文件名", "乙组"),
                }),
            },
        ]
    );

    // 作品的显示标题那一处，单改标题再看一眼例子：换的正是标题集合挑出来的那一个。
    let 只改标题 = 读一份(
        草稿.path(),
        "只改标题.toml",
        &内置.to_text().replace(
            "\"No-Intro\", \"Redump\", \"MAME\", \"TOSEC\"",
            "\"TOSEC\", \"No-Intro\", \"Redump\", \"MAME\"",
        ),
    );
    assert_eq!(
        priority::shifts(&catalog, &内置, &只改标题).expect("读得动库"),
        vec![FieldShifts {
            field: "标题".to_string(),
            works: 1,
            variants: 0,
            example: Some(Shift {
                anchor: AnchorKind::Work,
                subject: "超时空之轮".to_string(),
                before: 说("No-Intro", "Chrono Trigger"),
                after: 说("TOSEC", "Chrono Trigger (1995)(Square)"),
            }),
        }]
    );

    // 两份一样：什么都不列。
    assert!(
        priority::shifts(&catalog, &内置, &内置)
            .expect("读得动库")
            .is_empty()
    );
}

// ── 一个条目上每个字段眼下写出去的是什么（作品详情页「元数据」那一面照它画，票 `gui-looks-like-the-design/15`） ──

use romcat_core::scrape::Field;

#[test]
fn 条目上每个字段写出去的是什么与导出同一处算_各个源说的都列着_裁决压过全部() {
    let 草稿 = temp_dir("优先级-条目上的字段");
    let 表 = 读一份(
        草稿.path(),
        "表.toml",
        "\"版本\" = 1\n\
         [[\"字段\"]]\n\"名\" = \"简介\"\n\
         \"顺序\" = [\"裁决\", \"Pegasus\", \"ES-Gamelist\", \"中文离线源\", \"ScreenScraper\"]\n\
         [[\"字段\"]]\n\"名\" = \"汉化组\"\n\
         \"顺序\" = [\"裁决\", \"Pegasus\", \"ES-Gamelist\", \"TOSEC\", \"文件名\"]\n",
    );
    let mut catalog = 小库();
    // 「幻想传说」在 FC 上的首选变体是 `幻想传说-1`（同一档按键排）：汉化组从它身上取。
    let 字段们 = |catalog: &Catalog| {
        priority::entry_fields(
            catalog,
            AnchorKind::Work,
            "幻想传说",
            "FC",
            Some("主库/FC/幻想传说-1.nes"),
            &表,
        )
        .expect("读得出")
    };
    let 那一格 = |字段们: &[priority::FieldShown], field: Field| {
        字段们
            .iter()
            .find(|one| one.field == field)
            .cloned()
            .unwrap_or_else(|| panic!("「{}」那一格不在", field.label()))
    };
    let 说的 = |one: &priority::FieldShown| -> Vec<(String, String)> {
        one.offered
            .iter()
            .map(|value| (value.source.clone(), value.value.clone()))
            .collect()
    };

    let 眼下 = 字段们(&catalog);
    assert_eq!(
        眼下.iter().map(|one| one.field).collect::<Vec<_>>(),
        Field::all(),
        "字段照 Field::all 的次序一格一格列"
    );
    // ── 简介：作品上两家各说一句，写出去的是表里排前面的那一家；变体上挂着的简介不算（导出只读作品上的）。
    let 简介 = 那一格(&眼下, Field::Description);
    assert_eq!(简介.shown, Some(说("中文离线源", "中文简介")));
    assert_eq!(
        说的(&简介),
        [
            ("中文离线源".to_string(), "中文简介".to_string()),
            ("ScreenScraper".to_string(), "An RPG".to_string()),
        ],
        "各个源说的都列着，照表的名次排；变体上的简介不混进来"
    );
    // ── 汉化组：只从首选变体身上取，非首选那个变体上的一条都不带。
    let 汉化组 = 那一格(&眼下, Field::TranslationGroup);
    assert_eq!(汉化组.shown, Some(说("TOSEC", "甲组")));
    assert_eq!(
        说的(&汉化组),
        [
            ("TOSEC".to_string(), "甲组".to_string()),
            ("文件名".to_string(), "乙组".to_string()),
        ]
    );
    // ── 显示标题：认出了作品的由标题集合挑；这份库里集合是空的，退回作品名，不是哪个源说的。
    assert_eq!(
        那一格(&眼下, Field::Title).shown,
        Some(Said {
            source: None,
            values: vec!["幻想传说".to_string()],
        })
    );

    // ── 人手写一条：写出去的换成它，别家说的照旧列着。
    catalog
        .put_verdict_value(
            AnchorKind::Work,
            "幻想传说",
            Field::Description,
            "手写的简介",
            "测试",
        )
        .expect("写得进裁决");
    let 写过 = 那一格(&字段们(&catalog), Field::Description);
    assert_eq!(写过.shown, Some(说("裁决", "手写的简介")));
    assert_eq!(
        说的(&写过).first(),
        Some(&("裁决".to_string(), "手写的简介".to_string())),
        "裁决排在最前"
    );
    assert_eq!(
        写过.offered.len(),
        3,
        "中文离线源与 ScreenScraper 那两句照旧列着"
    );
}
