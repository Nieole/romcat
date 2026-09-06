//! **作品级的主列表**：一个游戏一行，变体在详情面板里挑。
//!
//! 这一层要钉住的是四件事，每一件都是「不这么做界面就会出错」：
//!
//! 1. **行数与库里的作品数对得上**，而且**认不出作品的那些一条都不许被吞掉**——
//!    真库上后者是一多半，折进「未知」那一行等于让人看不见自己一半的库。
//! 2. **一页一页翻完等于一次全取**。并列行的次序不定死，翻页就会漏行与重行，
//!    而人正照着这张表按批量操作。
//! 3. **每行那几个数是真的**：变体数、容量合计、平台集合与逐个变体加起来一致。
//! 4. **选中语义**：选中主列表的行 ＝ 选中这些作品，批量操作作用于它们的变体；
//!    在详情面板里选中某一个变体，变体级的东西只关它自己。

use std::collections::BTreeSet;

use romcat_core::catalog::browse::{Scope, WORK_FIELDS, WorkAnchor, WorkOrder, WorkQuery, WorkRow};
use romcat_core::catalog::identify::{Candidate, Identification, Provenance};
use romcat_core::catalog::{Catalog, Confidence, State};
use romcat_core::dat::Convention;
use romcat_core::dat::chinese::ChineseMark;
use romcat_core::platform::Manifest;
use romcat_core::scrape::priority::VERDICT;
use romcat_core::scrape::{AnchorKind, Field};
use romcat_core::shape::{Role, Variant};

/// 这份 fixture 里造几个作品。
const WORKS: usize = 6;

/// 每个作品底下挂几个变体。**大于一**：六成作品下面挂着不止一个变体，
/// 收敛这件事有没有起作用全看这个数。
const PER_WORK: u64 = 3;

/// 造几个**还没认出作品**的变体。它们一个变体一行，一条都不许被吞掉。
const LOOSE: u64 = 4;

/// 平台，故意让同一个作品横跨两个——**一行的平台是个集合**。
const PLATFORMS: [&str; 2] = ["GB", "GBC"];

fn 变体(key: &str, platform: Option<&str>, bytes: u64) -> Variant {
    Variant {
        key: key.to_string(),
        platform: platform.map(str::to_string),
        rule: "一文件一变体".into(),
        main_key: key.to_string(),
        manual: false,
        files: 2,
        bytes,
        unreadable_files: 1,
        members: vec![
            (key.to_string(), Role::Main),
            (format!("{key}.sav"), Role::Companion),
            (format!("{key}/内部/贴图.pak"), Role::Internal),
        ],
    }
}

fn 候选(key: &str, confidence: Confidence, evidence: &str) -> Candidate {
    Candidate {
        member_key: key.to_string(),
        inner: String::new(),
        confidence,
        accepted: confidence == Confidence::High,
        source: "No-Intro".to_string(),
        dat: "gameboy.dat".to_string(),
        platform: "GB".to_string(),
        game: "某条 DAT 条目".to_string(),
        rom: "rom.bin".to_string(),
        hashed_as: Convention::AsIs,
        dat_convention: Convention::AsIs,
        evidence: evidence.to_string(),
        chinese: Some(ChineseMark::FanTranslated),
        serial: None,
        release_id: None,
    }
}

/// 作品 `at` 的第 `n` 个变体叫什么。
fn 键(at: usize, n: u64) -> String {
    format!(
        "主库/{}/作品{at:02} 变体{n}.zip",
        PLATFORMS[(n as usize) % PLATFORMS.len()]
    )
}

/// 第 `n` 个**还没认出作品**的变体叫什么。
fn 散键(n: u64) -> String {
    format!("主库/未知/散落{n:02}.zip")
}

/// 一份形状照真库来的 fixture：作品底下挂着好几个变体、还有一批没认出作品的、
/// 平台有未知的一档、刮削字段有齐有缺、候选的置信度三档都有。
fn 建库() -> Catalog {
    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    let mut variants = Vec::new();
    for at in 0..WORKS {
        for n in 0..PER_WORK {
            variants.push(变体(
                &键(at, n),
                // 每个作品的最后一个变体**平台未知**：那是真库里存在的一档。
                (n + 1 != PER_WORK).then(|| PLATFORMS[(n as usize) % PLATFORMS.len()]),
                1_000 * (n + 1),
            ));
        }
    }
    for n in 0..LOOSE {
        variants.push(变体(&散键(n), Some("SFC"), 7_000));
    }
    catalog
        .replace_variants(&variants, 1, &Manifest::default())
        .expect("写得进去");

    let mut works = Vec::new();
    for at in 0..WORKS {
        works.push(
            catalog
                .add_work(&format!("作品{at:02}"), Provenance::Identified)
                .expect("建得出作品"),
        );
    }

    // 识别结论：作品那批挂上 work_id，散落那批**一行都不写**——那是「还没识别」。
    let mut records = Vec::new();
    for (at, work) in works.iter().enumerate() {
        for n in 0..PER_WORK {
            records.push(Identification {
                variant_key: 键(at, n),
                state: State::Matched,
                reason: None,
                units: 1,
                nkit: 0,
                read_bytes: 0,
                work_id: Some(*work),
                release_id: None,
                // 每个作品底下三档置信度各一个：最高的那一档就是这一行显示的那个。
                candidates: vec![候选(
                    &键(at, n),
                    [Confidence::Low, Confidence::Medium, Confidence::High][(at + n as usize) % 3],
                    "合成 fixture 里钉死的依据",
                )],
            });
        }
    }
    catalog
        .write_identifications(&records)
        .expect("写得进识别结论");

    // 刮削：前一半作品五样齐，后一半只有年份——「元数据齐不齐」得看得出区别。
    for at in 0..WORKS {
        let subject = format!("作品{at:02}");
        let fields: Vec<Field> = if at * 2 < WORKS {
            WORK_FIELDS.to_vec()
        } else {
            vec![Field::Year]
        };
        for field in fields {
            let value = if field == Field::Year {
                format!("{}", 1990 + at)
            } else {
                format!("{}-{at}", field.label())
            };
            catalog
                .put_verdict_value(AnchorKind::Work, &subject, field, &value, "fixture")
                .expect("写得进刮削值");
        }
    }
    catalog
}

/// 一页一页翻完，把每一行的身份收成一串。
fn 翻完(catalog: &Catalog, query: &WorkQuery, page: u64) -> Vec<WorkAnchor> {
    let total = catalog.work_total(query).expect("数得出总数");
    let mut out = Vec::new();
    let mut offset = 0;
    while offset < total {
        let rows = catalog.work_page(query, offset, page).expect("取得出一页");
        assert!(!rows.is_empty(), "还没到总数就取不出行了");
        out.extend(rows.into_iter().map(|row: WorkRow| row.anchor));
        offset += page;
    }
    out
}

#[test]
fn 主列表按作品出行而且认不出作品的一条都没被吞掉() {
    let catalog = 建库();
    let query = WorkQuery::default();
    let total = catalog.work_total(&query).expect("数得出总数");
    // 一个作品一行，加上**还没认出作品**的那些各自一行。后者压成一行的写法
    // （只写 `GROUP BY work_id`）会让这个数变成 WORKS + 1。
    assert_eq!(total, WORKS as u64 + LOOSE);

    let rows = catalog.work_page(&query, 0, 64).expect("取得出一页");
    let works = rows
        .iter()
        .filter(|row| matches!(row.anchor, WorkAnchor::Work(_)))
        .count();
    assert_eq!(works, WORKS, "作品行数与库里的作品数对不上");
    assert_eq!(
        rows.len() - works,
        LOOSE as usize,
        "还没认出作品的那些没有各自成行",
    );
    // 变体表那一层照旧是全部变体——收敛的是**行**，不是库。
    assert_eq!(
        catalog
            .variant_total(&romcat_core::catalog::VariantQuery::default())
            .expect("数得出来"),
        WORKS as u64 * PER_WORK + LOOSE,
    );
}

#[test]
fn 每行看得见平台变体数容量年份元数据齐不齐与最高置信度() {
    let catalog = 建库();
    let rows = catalog
        .work_page(&WorkQuery::default(), 0, 64)
        .expect("取得出一页");
    let 作品行: Vec<&WorkRow> = rows
        .iter()
        .filter(|row| matches!(row.anchor, WorkAnchor::Work(_)))
        .collect();

    for row in &作品行 {
        assert_eq!(row.variants, PER_WORK, "{} 的变体数不对", row.name);
        // 容量是逐个变体加起来的那个数；**是个下界**，读不到的成员按 0 计入。
        assert_eq!(row.bytes, 1_000 + 2_000 + 3_000);
        assert_eq!(row.unreadable_files, PER_WORK, "少算了几个成员没记下来");
        // 平台是个**集合**：两个平台加上**平台未知**那一档。
        assert_eq!(
            row.platforms,
            vec![
                "GB".to_string(),
                "GBC".to_string(),
                romcat_core::report::UNKNOWN_PLATFORM_LABEL.to_string(),
            ],
            "{} 的平台集合不对",
            row.name,
        );
        assert!(row.year.is_some(), "{} 的年份没折出来", row.name);
        // 三档候选里最高的那一档，正是 ADR-0002 说的「工具最有把握的那条结论」。
        assert_eq!(
            row.confidence,
            Some(Confidence::High),
            "{} 的最高置信度不对",
            row.name,
        );
    }

    // 元数据齐不齐：前一半齐、后一半只有年份。
    let 齐 = 作品行.iter().filter(|row| row.complete()).count();
    assert_eq!(齐, WORKS / 2, "「齐」的那几行数不对");
    let 缺 = 作品行
        .iter()
        .find(|row| !row.complete())
        .expect("有缺的那一行");
    assert!(!缺.missing.contains(&Field::Year), "年份明明刮到了");
    assert_eq!(缺.missing.len(), WORK_FIELDS.len() - 1);
    assert!(
        缺.missing_label().starts_with('缺'),
        "{}",
        缺.missing_label()
    );

    // **还没认出作品**的那些：一条候选都没有，那不是「撞过没撞上」（ADR-0002）。
    for row in rows
        .iter()
        .filter(|row| matches!(row.anchor, WorkAnchor::Loose(_)))
    {
        assert_eq!(row.variants, 1, "没认出作品的那一行就该只有它自己");
        assert_eq!(row.confidence, None);
        assert_eq!(row.confidence_label(), "还没识别");
        assert_eq!(row.missing_label(), "缺全部");
    }
}

#[test]
fn 五列都排得了序而且一页页翻完等于一次全取() {
    let catalog = 建库();
    for order in WorkOrder::ALL {
        for descending in [false, true] {
            let query = WorkQuery {
                order,
                descending,
                ..WorkQuery::default()
            };
            let 一次全取 = catalog
                .work_page(&query, 0, 1_000)
                .expect("取得出一页")
                .into_iter()
                .map(|row| row.anchor)
                .collect::<Vec<_>>();
            for page in [1, 3, 7] {
                assert_eq!(
                    翻完(&catalog, &query, page),
                    一次全取,
                    "按{}{}排、每页 {page} 行翻完，与一次全取对不上",
                    order.label(),
                    if descending { "倒着" } else { "" },
                );
            }
            // 全序：没有重行，也没有漏行。
            let 去重: BTreeSet<&WorkAnchor> = 一次全取.iter().collect();
            assert_eq!(去重.len(), 一次全取.len(), "同一行出现了两次");
        }
    }

    // 排序真的换了次序，不是摆着好看的表头。
    let 按名 = catalog
        .work_page(&WorkQuery::default(), 0, 64)
        .expect("取得出一页");
    let 按容量 = catalog
        .work_page(
            &WorkQuery {
                order: WorkOrder::Bytes,
                descending: true,
                ..WorkQuery::default()
            },
            0,
            64,
        )
        .expect("取得出一页");
    assert!(
        按容量.first().map(|row| row.bytes) >= 按容量.last().map(|row| row.bytes),
        "按容量倒着排，第一行反而比最后一行小",
    );
    assert_ne!(
        按名.iter().map(|row| &row.anchor).collect::<Vec<_>>(),
        按容量.iter().map(|row| &row.anchor).collect::<Vec<_>>(),
        "换了排序次序一模一样",
    );
}

#[test]
fn 筛选下推之后行与聚合一起收窄() {
    let catalog = 建库();
    let 全部 = catalog
        .work_total(&WorkQuery::default())
        .expect("数得出总数");

    // 按平台筛：作品行还在（它在 GB 上有变体），但那一行的变体数收窄了。
    let query = WorkQuery {
        platform: Some(romcat_core::catalog::PlatformFilter::Named("GB".into())),
        ..WorkQuery::default()
    };
    let rows = catalog.work_page(&query, 0, 64).expect("取得出一页");
    assert_eq!(
        catalog.work_total(&query).expect("数得出总数"),
        WORKS as u64,
        "按 GB 筛之后散落那批（SFC）该整批出局",
    );
    for row in &rows {
        assert_eq!(row.variants, 1, "屏上那一行的变体数没跟着筛选收窄");
        assert_eq!(row.platforms, vec!["GB".to_string()]);
        assert_eq!(row.bytes, 1_000);
    }
    assert!(catalog.work_total(&query).expect("数得出来") <= 全部);

    // **搜索框**：找的是**这一行画出来的那个名字**，不是变体的键——认出作品的按
    // 作品名找，没认出来的按它自己的键找（票 `gui-redesign/05` 的排序另见 `search.rs`）。
    let query = WorkQuery {
        search: "作品0".to_string(),
        ..WorkQuery::default()
    };
    assert_eq!(
        catalog.work_total(&query).expect("数得出总数"),
        WORKS as u64,
        "按作品名搜不出来",
    );
    let query = WorkQuery {
        search: "散落".to_string(),
        ..WorkQuery::default()
    };
    assert_eq!(
        catalog.work_total(&query).expect("数得出总数"),
        LOOSE,
        "没认出作品的那一行按它自己的键搜不出来",
    );

    // **筛选与搜索叠加时，收窄聚合的只有筛选那一半。** 这条查询分两趟走
    // （票 `gui-redesign/13`）：第一趟按筛选加搜索挑出这一页是哪几行，第二趟
    // **只按筛选**给这几行算聚合——搜索那三条组内恒定，第二趟再判一遍答案一样。
    // 两趟若在这儿分了家，屏上「变体数」就会跟着搜索词变，而批量操作照着它动手。
    let 又筛又搜 = WorkQuery {
        platform: Some(romcat_core::catalog::PlatformFilter::Named("GB".into())),
        search: "作品0".to_string(),
        ..WorkQuery::default()
    };
    let rows = catalog.work_page(&又筛又搜, 0, 64).expect("取得出一页");
    assert_eq!(rows.len(), WORKS, "又筛又搜之后行数不对");
    for row in &rows {
        assert_eq!(row.variants, 1, "搜索把这一行的变体数改了");
        assert_eq!(row.platforms, vec!["GB".to_string()], "搜索把平台集合改了");
        assert_eq!(row.bytes, 1_000, "搜索把容量改了");
    }
}

#[test]
fn 选中作品时批量操作作用于它的全部变体() {
    let catalog = 建库();
    let rows = catalog
        .work_page(&WorkQuery::default(), 0, 64)
        .expect("取得出一页");
    let 一个作品 = rows
        .iter()
        .find(|row| matches!(row.anchor, WorkAnchor::Work(_)))
        .expect("有作品行");

    // **选中一行 ＝ 选中这个作品**：展开出来的正是它底下那几个变体，一个不多一个不少。
    let 作用范围 = catalog
        .scoped_variants(
            &WorkQuery::default(),
            Scope::Rows(std::slice::from_ref(&一个作品.anchor)),
        )
        .expect("展开得了");
    assert_eq!(作用范围.len() as u64, 一个作品.variants);
    assert_eq!(
        作用范围.len() as u64,
        PER_WORK,
        "不筛的时候就是它的全部变体"
    );
    let 详情 = catalog
        .work_detail(&WorkQuery::default(), &一个作品.anchor)
        .expect("读得出详情")
        .expect("这一行有变体");
    assert_eq!(
        详情
            .variants
            .iter()
            .map(|v| v.row.key.clone())
            .collect::<Vec<_>>(),
        作用范围,
        "详情面板列的那几个变体，与批量操作作用的那几个不是同一批",
    );

    // 一行都没选就是一个变体都不动——**空选择不等于全选**。
    assert!(
        catalog
            .scoped_variants(&WorkQuery::default(), Scope::Rows(&[]))
            .expect("展开得了")
            .is_empty(),
    );

    // 全选：当前筛选下的每一行；减去点掉的那一行，正好少它底下那几个变体。
    let 全部变体 = catalog
        .scoped_variants(&WorkQuery::default(), Scope::AllExcept(&[]))
        .expect("展开得了");
    assert_eq!(全部变体.len() as u64, WORKS as u64 * PER_WORK + LOOSE);
    let 减一行 = catalog
        .scoped_variants(
            &WorkQuery::default(),
            Scope::AllExcept(std::slice::from_ref(&一个作品.anchor)),
        )
        .expect("展开得了");
    assert_eq!(减一行.len(), 全部变体.len() - 作用范围.len());
    assert!(减一行.iter().all(|key| !作用范围.contains(key)));

    // 作用范围跟着**当前筛选**走：屏上那一行写着几个变体，就作用于那几个。
    let query = WorkQuery {
        platform: Some(romcat_core::catalog::PlatformFilter::Named("GB".into())),
        ..WorkQuery::default()
    };
    let 筛过 = catalog
        .scoped_variants(&query, Scope::Rows(std::slice::from_ref(&一个作品.anchor)))
        .expect("展开得了");
    assert_eq!(筛过.len(), 1, "筛过之后作用范围没跟着收窄");
}

#[test]
fn 选中一个变体时变体级的东西只关它自己() {
    let catalog = 建库();
    let 详情 = catalog
        .work_detail(&WorkQuery::default(), &WorkAnchor::Work(1))
        .expect("读得出详情")
        .expect("这一行有变体");
    assert_eq!(详情.variants.len() as u64, PER_WORK);

    // 详情面板列出**全部变体，每个带置信度与依据**（ADR-0002：没有依据的候选
    // 事后无法复核）。
    for variant in &详情.variants {
        assert!(variant.confidence().is_some(), "这个变体一条候选都没有");
        assert!(!variant.candidates.is_empty());
        for candidate in &variant.candidates {
            assert!(!candidate.evidence.is_empty(), "候选没有依据，事后没法复核",);
        }
    }

    // **变体级只作用于它**：文件成员是这一个变体的，含**附属文件与内部资源**。
    let 挑中 = &详情.variants[0];
    let 文件 = catalog
        .variant_members(&挑中.row.key)
        .expect("列得出文件成员");
    assert_eq!(文件.len(), 3, "文件成员没列全");
    let 身份: BTreeSet<Role> = 文件.iter().map(|(_, role)| *role).collect();
    assert!(身份.contains(&Role::Main));
    assert!(身份.contains(&Role::Companion), "附属文件没列出来");
    assert!(身份.contains(&Role::Internal), "内部资源没列出来");
    for (key, _) in &文件 {
        assert!(
            key.starts_with(&挑中.row.key),
            "列进来的 {key} 不是这个变体的成员",
        );
    }
    // 同一个作品底下另一个变体的文件，一个都不在这一份里。
    let 另一个 = catalog
        .variant_members(&详情.variants[1].row.key)
        .expect("列得出文件成员");
    assert!(
        另一个
            .iter()
            .all(|(key, _)| !文件.iter().any(|(k, _)| k == key)),
        "两个变体的文件混在一起了",
    );
}

#[test]
fn 年份取的是裁决那一条而且列表与详情写的是同一个数() {
    let mut catalog = 建库();
    // 同一个作品上再刮一条更早的年份。**裁决排在每个字段的最前**（ADR-0001），
    // 于是列表上那一栏不该被它改掉。
    catalog
        .put_scraped(&[romcat_core::catalog::scrape::Harvested {
            anchor: AnchorKind::Work.label().to_string(),
            subject: "作品00".to_string(),
            source: "某个数据源".to_string(),
            input: "指纹".to_string(),
            values: vec![romcat_core::catalog::scrape::HarvestedValue {
                field: Field::Year.label().to_string(),
                value: "1970".to_string(),
                evidence: "测试摆进去的".to_string(),
            }],
            media: Vec::new(),
        }])
        .expect("写得进去");

    let rows = catalog
        .work_page(&WorkQuery::default(), 0, 64)
        .expect("取得出一页");
    let 那一行 = rows
        .iter()
        .find(|row| row.name == "作品00")
        .expect("有这一行");
    assert_eq!(那一行.year.as_deref(), Some("1990"), "裁决那一条没排在最前");

    let 详情 = catalog
        .work_detail(&WorkQuery::default(), &那一行.anchor)
        .expect("读得出详情")
        .expect("这一行有变体");
    assert_eq!(
        详情.year, 那一行.year,
        "面板上写的年份与列表上写的不是一个数",
    );

    // 撤掉裁决之后退回数据源给的那一条。
    catalog
        .clear_verdict_value(AnchorKind::Work, "作品00", Field::Year)
        .expect("撤得掉");
    let rows = catalog
        .work_page(&WorkQuery::default(), 0, 64)
        .expect("取得出一页");
    let 那一行 = rows
        .iter()
        .find(|row| row.name == "作品00")
        .expect("有这一行");
    assert_eq!(那一行.year.as_deref(), Some("1970"));
    assert_ne!(VERDICT, "某个数据源");
}

#[test]
fn 换一种排法画出来的那一行一个字都不变() {
    // 票 `gui-redesign/13` 把主列表那条查询拆成两趟——第一趟只挑「这一页是哪几行」，
    // 第二趟才给这几百行算聚合；年份那张 `LEFT JOIN` 也只在**按年份排**时才连，
    // 平时由补刮削值那一趟带回来。于是这里钉住一件事：**换排法只换次序，不换内容**。
    // 两趟若在哪儿分了家，最先看得出来的就是「按年份排之后年份那一栏变了」。
    let catalog = 建库();
    let 基准: Vec<WorkRow> = catalog
        .work_page(&WorkQuery::default(), 0, 1_000)
        .expect("取得出一页");
    assert!(!基准.is_empty(), "fixture 里得有行");

    for order in WorkOrder::ALL {
        for descending in [false, true] {
            let query = WorkQuery {
                order,
                descending,
                ..WorkQuery::default()
            };
            let 这一趟 = catalog.work_page(&query, 0, 1_000).expect("取得出一页");
            assert_eq!(这一趟.len(), 基准.len(), "{order:?} 排出来的行数不对");
            for row in &这一趟 {
                let 原样 = 基准
                    .iter()
                    .find(|had| had.anchor == row.anchor)
                    .expect("这一行在默认排法里也在");
                assert_eq!(
                    row, 原样,
                    "{order:?} / 倒序 {descending} 把这一行画的东西改了"
                );
            }
        }
    }
}

#[test]
fn 一页取不出全库() {
    let catalog = 建库();
    let rows = catalog
        .work_page(&WorkQuery::default(), 0, u64::MAX)
        .expect("取得出一页");
    assert!(rows.len() as u64 <= romcat_core::catalog::MAX_PAGE);
    assert!(
        catalog
            .work_page(&WorkQuery::default(), 0, 0)
            .expect("取得出一页")
            .is_empty(),
    );
    // 越过末尾取一页得到空表，不是报错——滚动条拖过头是常事。
    assert!(
        catalog
            .work_page(&WorkQuery::default(), 10_000, 64)
            .expect("取得出一页")
            .is_empty(),
    );
}
