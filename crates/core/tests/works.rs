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

use romcat_core::catalog::browse::{
    NonGameAssets, PlatformFilter, Scope, VariantQuery, WORK_FIELDS, WorkAnchor, WorkOrder,
    WorkQuery, WorkRow,
};
use romcat_core::catalog::identify::{Candidate, Identification, NOT_RUN_LABEL, Provenance};
use romcat_core::catalog::{Catalog, Confidence, State};
use romcat_core::dat::Convention;
use romcat_core::dat::chinese::ChineseMark;
use romcat_core::platform::Manifest;
use romcat_core::scrape::measure::Measured;
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
                platform: None,
                standalone: None,
                edition: None,
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
    // 这份 fixture 里它们**一行结论都不写**，所以那一栏印的是**还没识别**——
    // 印「没有候选」是撒谎：那句话说的是「识别跑过了、只是一个字都没说」，
    // 而这些变体连跑都还没跑过（词表两条词条、票 `gui-redesign/17`）。
    for row in rows
        .iter()
        .filter(|row| matches!(row.anchor, WorkAnchor::Loose(_)))
    {
        assert_eq!(row.variants, 1, "没认出作品的那一行就该只有它自己");
        assert_eq!(row.confidence, None);
        assert!(!row.identified, "这一行底下那个变体一行结论都没写过");
        assert_eq!(row.confidence_label(), NOT_RUN_LABEL);
        assert_eq!(row.missing_label(), "缺全部");
    }
}

/// **「还没识别」与「没有候选」在同一张表上分得开**（票 `gui-redesign/17`）。
///
/// 两者折进 [`WorkRow::confidence`] 都是 `None`：一个是连识别都还没跑过（库里连它的
/// 结论都没有），一个是识别跑过了、却一条候选都没有。这条界线正是**命中率的分母**
/// 那条界线（ADR-0002），并成一个词的话屏上就得挑一件事去撒谎——而它指的下一步也不同：
/// 前者去跑 `romcat identify`，后者得人自己来。
#[test]
fn 连识别都没跑过的与跑过了没候选的在同一张表上印两个词() {
    let mut catalog = 建库();
    // 散落那批**本来就一行结论都不写**。给其中一个补一条结论、但**一条候选都没有**：
    // 于是同一页上两种行并存，两者的 `confidence` 都是 `None`。
    let 跑过了 = 散键(0);
    catalog
        .write_identifications(&[Identification {
            variant_key: 跑过了.clone(),
            platform: None,
            standalone: None,
            edition: None,
            state: State::Unmatched,
            reason: None,
            units: 1,
            nkit: 0,
            read_bytes: 0,
            work_id: None,
            release_id: None,
            candidates: Vec::new(),
        }])
        .expect("写得进识别结论");

    let rows = catalog
        .work_page(&WorkQuery::default(), 0, 64)
        .expect("取得出一页");
    let 那一行 = |key: &str| {
        rows.iter()
            .find(|row| row.anchor == WorkAnchor::Loose(key.to_string()))
            .unwrap_or_else(|| panic!("{key} 那一行没了"))
    };

    let 有结论 = 那一行(&跑过了);
    assert_eq!(有结论.confidence, None, "它一条候选都没有");
    assert!(有结论.identified, "它有一行结论");
    assert_eq!(有结论.confidence_label(), "没有候选");

    let 没结论 = 那一行(&散键(1));
    assert_eq!(没结论.confidence, None, "它也一条候选都没有");
    assert!(!没结论.identified, "它一行结论都没有");
    assert_eq!(没结论.confidence_label(), NOT_RUN_LABEL);

    // **详情面板那一层同一条口径**：那里手上就是一个变体，不必折。
    // 连那句「接下来该干什么」也在核心库里挑（`no_candidate_hint`），
    // 因为它与印哪个词是同一条判据的两面——分家写两处，改一处就指错一处。
    let 一个变体 = |row: &WorkRow| {
        let detail = catalog
            .work_detail(&WorkQuery::default(), &row.anchor)
            .expect("读得动")
            .expect("点得开");
        let mut variants = detail.variants;
        assert_eq!(variants.len(), 1, "没认出作品的那一行就该只有它自己");
        variants.remove(0)
    };

    let 跑过的那个 = 一个变体(有结论);
    assert_eq!(跑过的那个.confidence_label(), "没有候选");
    assert_eq!(
        跑过的那个.no_candidate_hint(),
        Some("识别跑过了，一条候选都没有——那是没有候选，不是「撞过没撞上」。"),
    );

    let 没跑过的那个 = 一个变体(没结论);
    assert_eq!(没跑过的那个.confidence_label(), NOT_RUN_LABEL);
    let 那一句 = 没跑过的那个.no_candidate_hint().expect("该说一句");
    assert!(那一句.contains(NOT_RUN_LABEL), "{那一句}");
    assert!(
        那一句.contains("romcat identify"),
        "还没识别那一句该指向下一步该干什么：{那一句}",
    );

    // **有候选就不必说这一句**：那时屏上摆的是候选本身。作品那几行底下的变体
    // 各带一条候选（见 `建库`）。
    let 有候选的 = catalog
        .work_detail(
            &WorkQuery::default(),
            &rows
                .iter()
                .find(|row| row.confidence.is_some())
                .expect("有一行是有候选的")
                .anchor,
        )
        .expect("读得动")
        .expect("点得开")
        .variants
        .remove(0);
    assert!(!有候选的.candidates.is_empty(), "这个变体该有候选");
    assert_eq!(有候选的.no_candidate_hint(), None);
}

/// **一行是一批变体时，`identified` 问的是「是不是全都跑过」，不是「有没有一个」**
/// （票 `gui-redesign/17`、挂单 `Q190`）。
///
/// 上面那条断的全是**没认出作品**的行，而那种行只有一个变体——「一批」这件事它验不到。
/// 折的方向反了（SQL 里 `MIN` 写成 `MAX`）在那条测试上一点动静都没有，可屏上会出这样一行：
/// 十个变体里一个跑过识别、九个还没轮到，那一栏说「没有候选」——
/// 意思是「识别跑过了、只是一个字都没说，接下来得你自己来」，
/// 而这一行真正该做的事是**先跑一趟 `romcat identify`**。
#[test]
fn 一行底下只要还剩一个变体没跑过识别这一行就说还没识别() {
    // 两个作品各挂两个变体，两边都**一条候选都没有**，差别只在跑没跑过：
    // 「剩一个」那边的第二个变体走 `link_variant` 挂上作品、**不写结论行**
    // ——真机上「识别跑完之后又扫进一个新文件」就是这个形状。
    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    let 键 = |作品: &str, n: u64| format!("主库/GB/{作品}{n}.zip");
    let variants: Vec<Variant> = ["剩一个", "全跑过"]
        .into_iter()
        .flat_map(|作品| (0..2).map(move |n| 变体(&键(作品, n), Some("GB"), 1_000)))
        .collect();
    catalog
        .replace_variants(&variants, 1, &Manifest::default())
        .expect("写得进去");
    let 剩一个 = catalog
        .add_work("剩一个", Provenance::Identified)
        .expect("建得出作品");
    let 全跑过 = catalog
        .add_work("全跑过", Provenance::Identified)
        .expect("建得出作品");
    let 一条不带候选的 = |key: String, work: i64| Identification {
        variant_key: key,
        platform: None,
        standalone: None,
        edition: None,
        state: State::Unmatched,
        reason: None,
        units: 1,
        nkit: 0,
        read_bytes: 0,
        work_id: Some(work),
        release_id: None,
        candidates: Vec::new(),
    };
    catalog
        .write_identifications(&[
            一条不带候选的(键("剩一个", 0), 剩一个),
            一条不带候选的(键("全跑过", 0), 全跑过),
            一条不带候选的(键("全跑过", 1), 全跑过),
        ])
        .expect("写得进识别结论");
    // 这一个**只挂作品、不写结论**：它就是那个「还没轮到它」的变体。
    catalog
        .link_variant(&键("剩一个", 1), Some(剩一个), None)
        .expect("挂得上作品");

    let rows = catalog
        .work_page(&WorkQuery::default(), 0, 64)
        .expect("取得出一页");
    let 那一行 = |name: &str| {
        rows.iter()
            .find(|row| row.name == name)
            .unwrap_or_else(|| panic!("{name} 那一行没了"))
    };

    let 剩一个没跑过 = 那一行("剩一个");
    assert_eq!(剩一个没跑过.variants, 2, "这一行底下该有两个变体");
    assert_eq!(剩一个没跑过.confidence, None, "它们一条候选都没有");
    assert!(
        !剩一个没跑过.identified,
        "两个变体里还有一个连结论行都没有，这一行不算「全都跑过」",
    );
    assert_eq!(剩一个没跑过.confidence_label(), NOT_RUN_LABEL);

    let 全跑过了 = 那一行("全跑过");
    assert_eq!(全跑过了.variants, 2);
    assert_eq!(全跑过了.confidence, None, "它们也一条候选都没有");
    assert!(全跑过了.identified, "两个变体一个不落全都有结论行");
    assert_eq!(全跑过了.confidence_label(), "没有候选");
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

// ——— 非游戏资产默认收起（票 `gui-looks-like-the-design/08`） ———

/// 夹着的那几个**自成一行**的非游戏资产的键。
///
/// 键按次序排时它们**插在游戏中间**（`bios` 那一段比「游戏」排得靠前），翻页才测得出
/// 漏行重行。**哪几个算非游戏资产不在这里判**：判断只有 `classify::non_game_asset`
/// 那一处，这份库只是照「根名之后有一段目录叫 `bios`」摆的数据。
const 散落的非游戏资产: [&str; 4] = [
    "主库/FC/bios/disksys.rom",
    "主库/GB/BIOS/gb_bios.bin",
    "主库/PS/bios/SCPH-1001.BIN",
    "主库/街机/FBA-ROMS/BIOS/neogeo.zip",
];

/// 五个平台各六个游戏，外加：
///
/// - [`散落的非游戏资产`] 那四个，各自成一行（识别挑作品时跳过它们，于是它们认不出作品）；
/// - 一个**文件名**叫 `bios` 的游戏——判的是目录段，它照旧是游戏；
/// - 一部作品底下一个游戏、一个被人**手工挂进来**的 BIOS：这一行两档都列着。
///
/// 返回库与那部作品的 id。
fn 夹着非游戏资产的库() -> (Catalog, i64) {
    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    let mut variants = Vec::new();
    for platform in ["FC", "GB", "PS", "SFC", "街机"] {
        for n in 0..6 {
            variants.push(变体(
                &format!("主库/{platform}/游戏{n}.zip"),
                Some(platform),
                1_000,
            ));
        }
    }
    for key in 散落的非游戏资产 {
        let platform = key.split('/').nth(1);
        variants.push(变体(key, platform, 512));
    }
    variants.push(变体("主库/SFC/bios.zip", Some("SFC"), 2_000));
    variants.push(变体("主库/FC/魂斗罗.nes", Some("FC"), 4_000));
    variants.push(变体("主库/FC/魂斗罗/bios/disksys.rom", Some("FC"), 512));
    catalog
        .replace_variants(&variants, 1, &Manifest::default())
        .expect("写得进去");
    let work = catalog
        .add_work("魂斗罗", Provenance::Identified)
        .expect("建得出作品");
    for key in ["主库/FC/魂斗罗.nes", "主库/FC/魂斗罗/bios/disksys.rom"] {
        catalog
            .link_variant(key, Some(work), None)
            .expect("挂得上作品");
    }
    (catalog, work)
}

/// 这份查询在库里一共几行、翻出来是哪几行。
fn 数与表(catalog: &Catalog, query: &WorkQuery) -> (u64, BTreeSet<WorkAnchor>) {
    let total = catalog.work_total(query).expect("数得出总数");
    let rows = 翻完(catalog, query, 7);
    let set: BTreeSet<WorkAnchor> = rows.iter().cloned().collect();
    assert_eq!(set.len(), rows.len(), "翻页翻出了重行");
    assert_eq!(rows.len() as u64, total, "翻出来的行数与总数对不上");
    (total, set)
}

#[test]
fn 非游戏资产默认不列出_收起了几行与翻出来的表对得上() {
    let (catalog, work) = 夹着非游戏资产的库();
    let 默认 = WorkQuery::default();
    let 列出 = WorkQuery {
        non_game_assets: NonGameAssets::Listed,
        ..WorkQuery::default()
    };
    assert_eq!(默认.non_game_assets, NonGameAssets::Hidden, "浏览默认收起");
    assert!(
        !默认.same_filter(&列出),
        "拨一下开关换的是一批行，全选说的那一批跟着变",
    );

    let 收起了 = catalog
        .non_game_asset_rows(&默认)
        .expect("数得出收起了几行");
    assert_eq!(收起了, 散落的非游戏资产.len() as u64);
    // 开关拨在哪一档，这个数都是同一个：它说的是「这批筛选下整行都是非游戏资产的有几行」。
    assert_eq!(catalog.non_game_asset_rows(&列出).expect("数得出"), 收起了);

    let (收着的数, 收着的表) = 数与表(&catalog, &默认);
    let (列着的数, 列着的表) = 数与表(&catalog, &列出);
    assert_eq!(
        收着的数 + 收起了,
        列着的数,
        "屏上说收起了几行，表上就得正好少那几行"
    );
    assert!(收着的表.is_subset(&列着的表), "收起不该让别的行冒出来");
    let 多出来的: BTreeSet<WorkAnchor> = 列着的表.difference(&收着的表).cloned().collect();
    let 期望: BTreeSet<WorkAnchor> = 散落的非游戏资产
        .iter()
        .map(|key| WorkAnchor::Loose((*key).to_string()))
        .collect();
    assert_eq!(多出来的, 期望);

    // 一部作品底下夹着一个 BIOS：**这一行两档都列着**，收起的只是底下那一个变体，
    // 行上的变体数跟着说真话。
    let 魂斗罗 = WorkAnchor::Work(work);
    assert!(收着的表.contains(&魂斗罗) && 列着的表.contains(&魂斗罗));
    let 变体数 = |query: &WorkQuery| {
        catalog
            .work_page(query, 0, 64)
            .expect("取得出一页")
            .into_iter()
            .find(|row| row.anchor == 魂斗罗)
            .map(|row| row.variants)
    };
    assert_eq!(变体数(&默认), Some(1));
    assert_eq!(变体数(&列出), Some(2));

    // 筛着的时候也对得上：只数这一批里收起的。
    for (platform, 该收起) in [("FC", 1), ("PS", 1), ("SFC", 0)] {
        let 筛 = WorkQuery {
            platform: Some(PlatformFilter::Named(platform.to_string())),
            ..WorkQuery::default()
        };
        let 筛着列出 = WorkQuery {
            non_game_assets: NonGameAssets::Listed,
            ..筛.clone()
        };
        let 收起 = catalog.non_game_asset_rows(&筛).expect("数得出");
        assert_eq!(收起, 该收起, "{platform}");
        assert_eq!(
            数与表(&catalog, &筛).0 + 收起,
            数与表(&catalog, &筛着列出).0,
            "{platform} 那一批数与表对不上",
        );
    }

    // 变体那一层与主列表共用同一份筛选：亲手收起时五个非游戏资产都不在。
    // **它默认全列**——拿它默认值的调用方问的都是库里一共有什么（挂单 `Q776`）。
    assert_eq!(
        catalog
            .variant_total(&VariantQuery {
                non_game_assets: NonGameAssets::Hidden,
                ..VariantQuery::default()
            })
            .expect("数得出"),
        32
    );
    assert_eq!(
        catalog
            .variant_total(&VariantQuery::default())
            .expect("数得出"),
        37
    );

    // 左栏那几档的条数跟着开关走（挂单 `Q775`）：FC 底下六个游戏、魂斗罗一个、两个 BIOS。
    let 平台条数 = |switch: NonGameAssets, platform: &str| {
        catalog
            .facets(switch)
            .expect("问得出")
            .platforms
            .into_iter()
            .find(|facet| facet.value == platform)
            .map(|facet| facet.count)
    };
    assert_eq!(平台条数(NonGameAssets::Hidden, "FC"), Some(7));
    assert_eq!(平台条数(NonGameAssets::Listed, "FC"), Some(9));
    // 「还没识别」那一档拿总数去减：这份库一条结论都没写，两档各是它自己那个总数。
    let 还没识别 = |switch: NonGameAssets| {
        catalog
            .facets(switch)
            .expect("问得出")
            .states
            .into_iter()
            .find(|(filter, _)| *filter == romcat_core::catalog::browse::StateFilter::Unidentified)
            .map(|(_, count)| count)
    };
    assert_eq!(还没识别(NonGameAssets::Hidden), Some(32));
    assert_eq!(还没识别(NonGameAssets::Listed), Some(37));
}

#[test]
fn 几种开法开出来的库都问得动那一处判断() {
    // 判断挂成 SQL 函数，而函数不落在库文件里——**每条连接各挂一次**。日后添一个开连接的
    // 入口却忘了挂，浏览那几条查询就报「没有这个函数」，这条当场红。
    let dir = romcat_core::testing::temp_dir("浏览-非游戏资产");
    let 库文件 = dir.path().join("catalog").join("库.sqlite3");
    let 两个变体 = [
        变体("库/PS/游戏.zip", Some("PS"), 1_000),
        变体("库/PS/bios/SCPH-1001.BIN", Some("PS"), 512),
    ];
    let 问一遍 = |开法: &str, catalog: &Catalog| {
        let 默认 = WorkQuery::default();
        let 列出 = WorkQuery {
            non_game_assets: NonGameAssets::Listed,
            ..WorkQuery::default()
        };
        let 数 = |结果: Result<u64, romcat_core::catalog::CatalogError>| {
            结果.unwrap_or_else(|error| panic!("{开法} 开出来的库问不动：{error}"))
        };
        assert_eq!(数(catalog.work_total(&默认)), 1, "{开法}");
        assert_eq!(数(catalog.work_total(&列出)), 2, "{开法}");
        assert_eq!(数(catalog.non_game_asset_rows(&默认)), 1, "{开法}");
        assert_eq!(
            数(catalog.variant_total(&VariantQuery {
                non_game_assets: NonGameAssets::Hidden,
                ..VariantQuery::default()
            })),
            1,
            "{开法}"
        );
        // 左栏那几档也问它。
        assert_eq!(
            数(catalog.facets(NonGameAssets::Hidden).map(|facets| facets
                .platforms
                .iter()
                .map(|facet| facet.count)
                .sum())),
            1,
            "{开法}"
        );
    };

    {
        let mut 建的 = Catalog::create(&库文件, "库").expect("建得出中立库");
        建的
            .replace_variants(&两个变体, 1, &Manifest::default())
            .expect("写得进去");
        问一遍("Catalog::create", &建的);
    }
    let 读写 = Catalog::open(&库文件).expect("打得开");
    问一遍("Catalog::open", &读写);
    问一遍(
        "Catalog::read_only",
        &读写.read_only().expect("分得出只读的一份"),
    );
    问一遍(
        "Catalog::open_read_only",
        &Catalog::open_read_only(&库文件).expect("只读打得开"),
    );
    let mut 内存 = Catalog::open_in_memory().expect("开得出内存库");
    内存
        .replace_variants(&两个变体, 1, &Manifest::default())
        .expect("写得进去");
    问一遍("Catalog::open_in_memory", &内存);
}

#[test]
fn 列出来的每一行说得出它是非游戏资产_收起时一行都不标() {
    let (catalog, work) = 夹着非游戏资产的库();
    let 默认 = WorkQuery::default();
    let 列出 = WorkQuery {
        non_game_assets: NonGameAssets::Listed,
        ..WorkQuery::default()
    };
    let 期望: BTreeSet<WorkAnchor> = 散落的非游戏资产
        .iter()
        .map(|key| WorkAnchor::Loose((*key).to_string()))
        .collect();

    // 一页一页翻完：标着的是哪几行、一共几行。**标记是核心库交回来的**，这里不判。
    let 翻着数标记 = |query: &WorkQuery| {
        let total = catalog.work_total(query).expect("数得出总数");
        let mut 标着的 = BTreeSet::new();
        let mut offset = 0;
        while offset < total {
            for row in catalog.work_page(query, offset, 7).expect("取得出一页") {
                if row.non_game_asset {
                    assert!(标着的.insert(row.anchor), "翻页翻出了重行");
                }
            }
            offset += 7;
        }
        标着的
    };
    let 标着的 = 翻着数标记(&列出);
    assert_eq!(标着的, 期望, "列出来之后，标着的得正好是那几个");
    assert_eq!(
        标着的.len() as u64,
        catalog.non_game_asset_rows(&列出).expect("数得出"),
        "屏上说列出了几个，行上就标着几个",
    );
    assert!(翻着数标记(&默认).is_empty(), "收起时列着的行一行都不该标");

    // 夹着一个 BIOS 的作品：**行上不标**（它底下还有游戏），详情里那一个变体标着。
    let 魂斗罗 = WorkAnchor::Work(work);
    let 列着的详情 = catalog
        .work_detail(&列出, &魂斗罗)
        .expect("读得动")
        .expect("这一行列着");
    assert_eq!(列着的详情.variants.len(), 2);
    let 标着的变体: Vec<&str> = 列着的详情
        .variants
        .iter()
        .filter(|variant| variant.non_game_asset())
        .map(|variant| variant.row.key.as_str())
        .collect();
    assert_eq!(标着的变体, vec!["主库/FC/魂斗罗/bios/disksys.rom"]);
    let 收着的详情 = catalog
        .work_detail(&默认, &魂斗罗)
        .expect("读得动")
        .expect("这一行照样列着");
    assert_eq!(
        收着的详情
            .variants
            .iter()
            .map(|variant| variant.row.key.as_str())
            .collect::<Vec<_>>(),
        vec!["主库/FC/魂斗罗.nes"],
        "收起时详情里也不列那个 BIOS",
    );

    // **开关进不了子库的规则**：子库选的变体照旧由规则说了算（挂单 `Q773`）。
    let 筛 = WorkQuery {
        platform: Some(PlatformFilter::Named("FC".to_string())),
        ..WorkQuery::default()
    };
    let 筛着列出 = WorkQuery {
        non_game_assets: NonGameAssets::Listed,
        ..筛.clone()
    };
    assert_eq!(筛.to_rule(), 筛着列出.to_rule());
}

/// 一条叫法：作品名是锚点（`title` 那张表的 `work` 列）。
fn 叫法(
    work: &str,
    value: &str,
    language: romcat_core::title::Language,
    kind: romcat_core::title::TitleKind,
) -> romcat_core::catalog::TitleRow {
    romcat_core::catalog::TitleRow {
        work: work.to_string(),
        value: value.to_string(),
        language,
        kind,
        source: "测试".to_string(),
        region: None,
        variant_key: None,
        confidence: Confidence::High,
        seam: None,
        evidence: "测试里写进去的".to_string(),
        seen: 1,
    }
}

#[test]
fn 主列表每行带出显示标题_标题集合是空的或挑出来就是作品名时不带() {
    use romcat_core::title::{Language, TitleKind};

    let mut catalog = 建库();
    // 作品00 有一条官中译名；作品01 只有一条与作品名一字不差的官方名；其余作品一条叫法都没有。
    catalog
        .put_titles(&[
            叫法(
                "作品00",
                "作品零号",
                Language::Chinese,
                TitleKind::Translated,
            ),
            叫法("作品01", "作品01", Language::English, TitleKind::Official),
        ])
        .expect("写得进标题集合");
    let priorities = romcat_core::scrape::Priorities::builtin();
    let query = WorkQuery::default();
    let rows = catalog
        .work_page_with_titles(&query, 0, 64, &priorities)
        .expect("取得出一页");
    let 行 = |name: &str| {
        rows.iter()
            .find(|row| row.name == name)
            .unwrap_or_else(|| panic!("这一页里没有「{name}」"))
    };
    assert_eq!(行("作品00").display.as_deref(), Some("作品零号"));
    assert_eq!(行("作品01").display, None, "挑出来就是作品名：第二行不写");
    assert_eq!(行("作品02").display, None, "一条叫法都没有：退回作品名");
    assert!(
        rows.iter()
            .filter(|row| matches!(row.anchor, WorkAnchor::Loose(_)))
            .all(|row| row.display.is_none()),
        "认不出作品的那几行不带显示标题：它们屏上的名字是正题",
    );

    // **与详情面板挑的是同一个**（ADR-0024）：点开作品00 的一个变体，详情里的显示标题一字不差。
    let detail = catalog
        .variant_detail(&键(0, 0), &priorities, None)
        .expect("读得动")
        .expect("有这个变体");
    assert_eq!(
        detail.display.map(|chosen| chosen.display).as_deref(),
        Some("作品零号"),
    );

    // 带不带显示标题，别的格子一个字都不差：补的只有这一格。
    let plain = catalog.work_page(&query, 0, 64).expect("取得出一页");
    let without: Vec<WorkRow> = rows
        .into_iter()
        .map(|mut row| {
            row.display = None;
            row
        })
        .collect();
    assert_eq!(without, plain);
}

/// 点开一个作品，交回它底下每个变体的 `(键, 变体简称)`。
fn 简称们(catalog: &Catalog, 作品: &str) -> Vec<(String, String)> {
    let query = WorkQuery::default();
    let anchor = catalog
        .work_page(&query, 0, 64)
        .expect("取得出一页")
        .into_iter()
        .find(|row| row.name == 作品)
        .unwrap_or_else(|| panic!("这一页里没有「{作品}」"))
        .anchor;
    let detail = catalog
        .work_detail(&query, &anchor)
        .expect("读得动")
        .expect("有这一行");
    let names = catalog
        .variant_short_names(&detail, &romcat_core::scrape::Priorities::builtin())
        .expect("拼得出变体简称");
    assert_eq!(
        names.len(),
        detail.variants.len(),
        "一个变体一个简称，次序对得上"
    );
    detail
        .variants
        .iter()
        .map(|variant| variant.row.key.clone())
        .zip(names)
        .collect()
}

#[test]
fn 变体简称是哪一种照首选变体那条规则_汉化组取导出写的那个_说不出时退回文件名() {
    let mut catalog = 建库();
    // fixture 里每条候选都带着汉化记号，只有高置信那条定下来了：作品00 是变体2，作品01 是变体1。
    catalog
        .put_verdict_value(
            AnchorKind::Variant,
            &键(0, 2),
            Field::TranslationGroup,
            "口袋汉化组",
            "fixture",
        )
        .expect("写得进汉化组");
    let 叫 = |pairs: &[(String, String)], key: String| {
        pairs
            .iter()
            .find(|(it, _)| *it == key)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| panic!("详情里没有「{key}」"))
    };

    let 作品00 = 简称们(&catalog, "作品00");
    assert_eq!(叫(&作品00, 键(0, 2)), "汉化版 · 口袋汉化组");
    for n in [0, 1] {
        assert_eq!(
            叫(&作品00, 键(0, n)),
            romcat_core::path::file_name_of_key(&键(0, n)),
            "候选没定下来、也没有发行版：说不出是哪一种，退回文件名",
        );
    }
    // 定下来了、是汉化版，没人写过汉化组：只写「汉化版」。
    let 作品01 = 简称们(&catalog, "作品01");
    assert_eq!(叫(&作品01, 键(1, 1)), "汉化版");
}

/// 点开一个作品（或一行认不出作品的），交回核心库答的**中文版本**。
fn 中文版本(catalog: &Catalog, anchor: &WorkAnchor) -> Option<ChineseMark> {
    let detail = catalog
        .work_detail(&WorkQuery::default(), anchor)
        .expect("读得动")
        .expect("有这一行");
    catalog.work_chinese_mark(&detail).expect("答得出中文版本")
}

/// 这一页里叫 `作品` 的那一行。
fn 那一行(catalog: &Catalog, 作品: &str) -> WorkAnchor {
    catalog
        .work_page(&WorkQuery::default(), 0, 64)
        .expect("取得出一页")
        .into_iter()
        .find(|row| row.name == 作品)
        .unwrap_or_else(|| panic!("这一页里没有「{作品}」"))
        .anchor
}

#[test]
fn 作品的中文版本照首选变体那条规则判_汉化压过官中_首选被裁决指定时照样答得出() {
    let mut catalog = 建库();
    // fixture 里每个作品定下来的那一个变体带着汉化记号。
    assert_eq!(
        中文版本(&catalog, &那一行(&catalog, "作品00")),
        Some(ChineseMark::FanTranslated),
    );

    // 改几个作品的识别结论：作品03 定下来的那一个是官中；作品04 定下来的那一个一个中文记号都没有；
    // 作品05 两个都定下来了，一个官中、一个汉化。
    let 结论 = |catalog: &Catalog, at: usize, n: u64, chinese: Option<ChineseMark>| {
        let WorkAnchor::Work(work_id) = 那一行(catalog, &format!("作品{at:02}")) else {
            panic!("作品{at:02} 该是认出作品的一行");
        };
        let mut 候选 = 候选(&键(at, n), Confidence::High, "测试改过的依据");
        候选.chinese = chinese;
        Identification {
            variant_key: 键(at, n),
            platform: None,
            standalone: None,
            edition: None,
            state: State::Matched,
            reason: None,
            units: 1,
            nkit: 0,
            read_bytes: 0,
            work_id: Some(work_id),
            release_id: None,
            candidates: vec![候选],
        }
    };
    let records = vec![
        结论(&catalog, 3, 2, Some(ChineseMark::Official)),
        结论(&catalog, 4, 1, None),
        结论(&catalog, 5, 0, Some(ChineseMark::Official)),
        结论(&catalog, 5, 1, Some(ChineseMark::FanTranslated)),
    ];
    catalog
        .write_identifications(&records)
        .expect("写得进识别结论");
    assert_eq!(
        中文版本(&catalog, &那一行(&catalog, "作品03")),
        Some(ChineseMark::Official),
    );
    assert_eq!(
        中文版本(&catalog, &那一行(&catalog, "作品05")),
        Some(ChineseMark::FanTranslated),
        "汉化与官中都有：照首选变体那条规则，汉化压过官中",
    );

    // **首选变体被裁决指定时照样答得出**：作品01 定下来的那个汉化版被人指成首选，首选规则里它排「裁决」那一档，
    // 可它是汉化版这件事不因此改变。
    catalog
        .set_preferred_variant("作品01", PLATFORMS[1], &键(1, 1))
        .expect("裁得了首选变体");
    let 作品01 = 那一行(&catalog, "作品01");
    let WorkAnchor::Work(_) = &作品01 else {
        panic!("作品01 该是认出作品的一行");
    };
    let 详情 = catalog
        .variant_detail(&键(1, 1), &romcat_core::scrape::Priorities::builtin(), None)
        .expect("读得动")
        .expect("有这个变体");
    assert_eq!(
        详情.siblings.first().map(|sibling| sibling.preference),
        Some(romcat_core::adapter::converge::Preference::Verdict),
        "前提：那个汉化版眼下是裁决指定的首选"
    );
    assert_eq!(
        中文版本(&catalog, &作品01),
        Some(ChineseMark::FanTranslated)
    );
    let 作品01详情 = catalog
        .work_detail(&WorkQuery::default(), &作品01)
        .expect("读得动")
        .expect("有这一行");
    let 那个汉化版 = 作品01详情
        .variants
        .iter()
        .find(|variant| variant.row.key == 键(1, 1))
        .expect("在作品01 底下");
    assert_eq!(
        catalog.variant_kind(那个汉化版).expect("答得出"),
        Some(romcat_core::adapter::converge::Preference::FanTranslated),
        "变体是哪一种也不因裁决改变",
    );

    // 一个中文记号都没有：没有中文版本；认不出作品、一条候选都没有的散落变体也一样。
    assert_eq!(中文版本(&catalog, &那一行(&catalog, "作品04")), None);
    assert_eq!(中文版本(&catalog, &WorkAnchor::Loose(散键(0))), None);
}

#[test]
fn 变体上定下来的那条候选与判定依据摆的那一条由核心库挑() {
    let mut catalog = 建库();
    // 作品04 的变体0 改成两条都没定下来的候选：一条低置信、一条中置信。
    let WorkAnchor::Work(work_id) = 那一行(&catalog, "作品04") else {
        panic!("作品04 该是认出作品的一行");
    };
    let mut 中 = 候选(&键(4, 0), Confidence::Medium, "测试改过的依据");
    中.accepted = false;
    let mut 低 = 候选(&键(4, 0), Confidence::Low, "测试改过的依据");
    低.accepted = false;
    catalog
        .write_identifications(&[Identification {
            variant_key: 键(4, 0),
            platform: None,
            standalone: None,
            edition: None,
            state: State::Unmatched,
            reason: None,
            units: 1,
            nkit: 0,
            read_bytes: 0,
            work_id: Some(work_id),
            release_id: None,
            candidates: vec![低, 中],
        }])
        .expect("写得进识别结论");
    let 变体 = |作品: &str, at: usize, n: u64| {
        catalog
            .work_detail(&WorkQuery::default(), &那一行(&catalog, 作品))
            .expect("读得动")
            .expect("有这一行")
            .variants
            .into_iter()
            .find(|variant| variant.row.key == 键(at, n))
            .expect("在这个作品底下")
    };

    // fixture：作品00 的变体2 那条高置信候选定下来了。
    let 定了的 = 变体("作品00", 0, 2);
    assert_eq!(
        定了的
            .accepted_candidate()
            .map(|candidate| candidate.game.as_str()),
        Some("某条 DAT 条目"),
    );
    assert_eq!(
        定了的
            .best_candidate()
            .map(|candidate| candidate.confidence),
        Some(Confidence::High),
    );
    let 没定的 = 变体("作品04", 4, 0);
    assert!(
        没定的.accepted_candidate().is_none(),
        "两条都没定下来：交不出定下来的那条"
    );
    assert_eq!(
        没定的
            .best_candidate()
            .map(|candidate| candidate.confidence),
        Some(Confidence::Medium),
        "判定依据摆置信度最高的那一条",
    );
}

#[test]
fn 作品详情页上哪几个变体摆汉化组那一行由核心库答_汉化版或身上有汉化组值的() {
    let mut catalog = 建库();
    // 作品00：变体2 是定下来的汉化版；变体0 不是汉化版，身上摆着一条汉化组值；变体1 两样都没有。
    for (n, 组) in [(2, "口袋汉化组"), (0, "某汉化组")] {
        catalog
            .put_verdict_value(
                AnchorKind::Variant,
                &键(0, n),
                Field::TranslationGroup,
                组,
                "fixture",
            )
            .expect("写得进汉化组");
    }
    let 作品00 = catalog
        .work_detail(&WorkQuery::default(), &那一行(&catalog, "作品00"))
        .expect("读得动")
        .expect("有这一行");
    let 行 = romcat_core::scrape::priority::translation_groups(
        &catalog,
        &作品00,
        &romcat_core::scrape::Priorities::builtin(),
    )
    .expect("答得出");
    let 摆的: Vec<(String, Vec<String>)> = 行
        .into_iter()
        .map(|(key, group)| (key, group.shown.map(|said| said.values).unwrap_or_default()))
        .collect();
    assert_eq!(
        摆的,
        [
            (键(0, 0), vec!["某汉化组".to_string()]),
            (键(0, 2), vec!["口袋汉化组".to_string()]),
        ],
        "汉化版一行、身上有汉化组值的一行，两样都没有的不摆",
    );
}

#[test]
fn 几个变体的媒体清单并成一份_作品上那几份只留一遍() {
    use romcat_core::catalog::detail::{MediaItem, merge_media_items};
    use romcat_core::scrape::MediaKind;

    let 一份 = |anchor: AnchorKind, kind: MediaKind, hash: &str| MediaItem {
        anchor,
        kind,
        source: "测试".to_owned(),
        hash: hash.to_owned(),
        ext: "png".to_owned(),
        bytes: 1,
        measured: Measured::default(),
        at: None,
        in_pool: None,
        evidence: "测试".to_owned(),
    };
    let 甲 = vec![
        一份(AnchorKind::Work, MediaKind::Cover, "作品的封面"),
        一份(AnchorKind::Variant, MediaKind::Video, "甲的视频"),
    ];
    let 乙 = vec![
        一份(AnchorKind::Work, MediaKind::Cover, "作品的封面"),
        一份(AnchorKind::Variant, MediaKind::Video, "乙的视频"),
    ];
    let 并了 = merge_media_items([甲.as_slice(), 乙.as_slice()]);
    assert_eq!(
        并了
            .iter()
            .map(|item| item.hash.as_str())
            .collect::<Vec<_>>(),
        ["作品的封面", "甲的视频", "乙的视频"],
    );
}

#[test]
fn 判定依据那一句照依据形状各段排_来源_数据文件_哈希口径_依据_候选数() {
    let catalog = 建库();
    let 变体 = |作品: &str, at: usize, n: u64| {
        catalog
            .work_detail(&WorkQuery::default(), &那一行(&catalog, 作品))
            .expect("读得动")
            .expect("有这一行")
            .variants
            .into_iter()
            .find(|variant| variant.row.key == 键(at, n))
            .expect("在这个作品底下")
    };
    // fixture 里每个变体一条候选：No-Intro / gameboy.dat / 原样哈希。
    assert_eq!(
        变体("作品00", 0, 2).basis_line().as_deref(),
        Some("No-Intro / gameboy.dat / 含头 · 合成 fixture 里钉死的依据 · 1 个候选"),
    );
    // 认不出作品、一条候选都没有的散落变体：没有这一句（屏上照核心库那句「为什么没定下来」说）。
    let 散落 = catalog
        .work_detail(&WorkQuery::default(), &WorkAnchor::Loose(散键(0)))
        .expect("读得动")
        .expect("有这一行")
        .variants
        .into_iter()
        .next()
        .expect("有它自己");
    assert_eq!(散落.basis_line(), None);
}

#[test]
fn 第几版两层一条回退链_裁决压过发行版的修订_都没有就说不出() {
    // 票 `gui-looks-like-the-design/34`，词表**第几版**（2026-09-20 拿主意的人定）：
    // 裁决 > DAT 条目名里的修订 > 说不出。**挑哪一层只在核心库一处判**（ADR-0024），
    // 界面只把它印出来。
    use romcat_core::catalog::identify::Provenance;
    use romcat_core::scrape::Priorities;

    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    let 键 = |n: usize| format!("库/GB/第{n}份.zip");
    catalog
        .replace_variants(
            &(0..3)
                .map(|n| Variant {
                    key: 键(n),
                    platform: Some("GB".to_owned()),
                    rule: romcat_core::shape::SINGLE_FILE_RULE.to_owned(),
                    main_key: 键(n),
                    manual: false,
                    files: 1,
                    bytes: 1_000,
                    unreadable_files: 0,
                    members: vec![(键(n), Role::Main)],
                })
                .collect::<Vec<_>>(),
            1,
            &Manifest::default(),
        )
        .expect("变体写得进");
    let work = catalog
        .add_work("口袋妖怪", Provenance::Identified)
        .expect("建得出作品");
    // 头一条发行版名字里带修订，第二条不带——发行版那一层的两种情形。
    let 带修订 = catalog
        .add_release(
            work,
            Some("GB"),
            Some("Japan"),
            None,
            None,
            Provenance::Identified,
            Some("Rev 1"),
        )
        .expect("建得出发行版");
    let 不带 = catalog
        .add_release(
            work,
            Some("GB"),
            Some("USA"),
            None,
            None,
            Provenance::Identified,
            None,
        )
        .expect("建得出发行版");
    let 一条结论 = |key: String, release: Option<i64>, edition: Option<&str>| Identification {
        variant_key: key,
        platform: None,
        standalone: None,
        edition: edition.map(str::to_owned),
        state: State::Matched,
        reason: None,
        units: 1,
        nkit: 0,
        read_bytes: 0,
        work_id: Some(work),
        release_id: release,
        candidates: Vec::new(),
    };
    catalog
        .write_identifications(&[
            // 甲：发行版带修订，没人裁过 → 听发行版那一层的。
            一条结论(键(0), Some(带修订), None),
            // 乙：发行版也带修订，**但人裁过** → 裁决压过它。
            一条结论(键(1), Some(带修订), Some("v1.2")),
            // 丙：发行版不带修订，也没人裁过 → 说不出。
            一条结论(键(2), Some(不带), None),
        ])
        .expect("结论写得进");
    for (n, release) in [(0, Some(带修订)), (1, Some(带修订)), (2, Some(不带))] {
        catalog
            .link_variant(&键(n), Some(work), release)
            .expect("挂得上");
    }

    let 第几版 = |key: &str| {
        catalog
            .variant_detail(key, &Priorities::builtin(), None)
            .expect("读得出")
            .expect("有这个变体")
            .edition()
            .map(str::to_owned)
    };
    assert_eq!(
        第几版(&键(0)).as_deref(),
        Some("Rev 1"),
        "没人裁过时看已接受那条候选撞上的 DAT 条目名里的修订"
    );
    assert_eq!(
        第几版(&键(1)).as_deref(),
        Some("v1.2"),
        "裁决说了就听裁决——「这是谁汉化的第几版」只有人说得出（ADR-0008）"
    );
    assert_eq!(
        第几版(&键(2)),
        None,
        "名字里没有修订标记就是没有，不拿 1.0 去补（2026-09-20 拿主意的人定）"
    );
}
