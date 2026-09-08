//! 翻页浏览：排序、筛选、分页下推到中立库这条接缝。
//!
//! 要证的是三件事，每一件都是「不这么做界面就会出错」而不是「这样比较好看」：
//!
//! 1. **一页一页翻完等于一次全取**。并列行的次序若不定死，翻页会漏行与重行——
//!    第 100 页的最后一行在第 101 页再出现一次，而人正在照着这张表做裁决。
//! 2. **筛选用的是子串，不是通配符**。用户在筛选框里打一个 `%` 是在找文件名里的百分号，
//!    不是在写模式。
//! 3. **一页取不出全库**。`limit` 有硬上界，堵住「把四万行读进内存」这条路。
//! 4. **筛选面板印出去的那个词认得回来**。面板上一档只有一串字可点，印与认两处各抄
//!    一遍的话，改一处就静默断掉一档——断了的样子是「点了没反应」。

use romcat_core::catalog::browse::{MAX_PAGE, StateFilter, VariantOrder, VariantQuery};
use romcat_core::catalog::identify::{NOT_RUN_LABEL, Tier};
use romcat_core::catalog::{Catalog, Provenance, VariantRow};
use romcat_core::platform::Manifest;
use romcat_core::shape::Variant;
use romcat_core::sublibrary::Rule;

/// 造一份变体。`bytes` 故意大量并列，逼出「翻页会不会漏行」这个问题。
fn 变体(key: &str, platform: Option<&str>, files: u64, bytes: u64) -> Variant {
    Variant {
        key: key.to_string(),
        platform: platform.map(str::to_string),
        rule: "一文件一变体".into(),
        main_key: key.to_string(),
        manual: false,
        files,
        bytes,
        unreadable_files: 0,
        members: Vec::new(),
    }
}

fn 建库(variants: &[Variant]) -> Catalog {
    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    catalog
        .replace_variants(variants, 1, &Manifest::default())
        .expect("写得进去");
    catalog
}

/// 合成库里那几个**作品**的名字。
///
/// **故意与键的次序对不上**：按作品名排出来的次序若与按键排出来的一样，
/// 「真按作品名排了」与「压根没排」这两件事就分不开——那时断言只是在说
/// 「这批数据碰巧排成这样」。汉字、假名、拉丁各占几条，中文那一段的次序正是
/// 最容易写成「碰巧」的地方。
const 作品们: [&str; 5] = [
    "幻想传说",
    "ゼルダの伝説",
    "Ninja Gaiden",
    "口袋妖怪 绿宝石",
    "沙罗曼蛇",
];

/// 一份形状照真库来的合成库：平台若干、容量大量并列、有**平台未知**的一档，
/// 也有**还没认出作品**的那一批（每三个留一个）。
fn 合成库(rows: u64) -> Catalog {
    let 平台 = ["SFC", "PS1", "PSP", "NDS"];
    // 键带**根名**（`path::library_key`）：合成库照真库的形状摆。
    let keys: Vec<String> = (0..rows)
        .map(|i| format!("库/{}/幻想传说 {i:05} 汉化版.zip", 平台[(i % 4) as usize]))
        .collect();
    let variants: Vec<Variant> = (0..rows)
        .map(|i| {
            变体(
                &keys[i as usize],
                // 每七个留一个平台未知：那是真库里存在的一档（认不出平台的内容照常入库），
                // 排序遇到 `NULL` 时不该翻车。
                (i % 7 != 2).then(|| 平台[(i % 4) as usize]),
                1 + i % 3,
                // 只有十种取值，于是并列一大片。
                (i % 10) * 1_000_000,
            )
        })
        .collect();
    let mut catalog = 建库(&variants);
    let works: Vec<i64> = 作品们
        .iter()
        .map(|name| {
            catalog
                .add_work(name, Provenance::Identified)
                .expect("建得出作品")
        })
        .collect();
    for (i, key) in keys.iter().enumerate() {
        // **每三个留一个还没认出作品的**：真库上那是一千七百多个（`adapter::converge`
        // 的 `Anchor::Loose`）。这里的比例比真库高得多是有意的——这条测试要断的是
        // 「空的那一批连成一段」，稀疏的话一次抽样就说明不了什么。
        if i % 3 == 0 {
            continue;
        }
        catalog
            .link_variant(key, Some(works[i % works.len()]), None)
            .expect("挂得上作品");
    }
    catalog
}

/// 一页一页翻完，把键收成一串。
fn 翻完(catalog: &Catalog, query: &VariantQuery, page: u64) -> Vec<String> {
    let total = catalog.variant_total(query).expect("数得出总数");
    let mut keys = Vec::new();
    let mut offset = 0;
    while offset < total {
        let rows = catalog
            .variant_page(query, offset, page)
            .expect("取得出一页");
        assert!(!rows.is_empty(), "还没到总数就取不出行了");
        keys.extend(rows.into_iter().map(|row: VariantRow| row.key));
        offset += page;
    }
    keys
}

#[test]
fn 一页页翻完等于一次全取() {
    let catalog = 合成库(500);
    for order in VariantOrder::ALL {
        for descending in [false, true] {
            let query = VariantQuery {
                order,
                descending,
                ..Default::default()
            };
            let 一次取完 = 翻完(&catalog, &query, 500);
            let 小步翻 = 翻完(&catalog, &query, 7);
            assert_eq!(
                一次取完,
                小步翻,
                "按 {} {} 排时，分页翻出来的次序和一次取完不一样",
                order.label(),
                if descending { "倒序" } else { "正序" },
            );
            let mut 去重 = 小步翻.clone();
            去重.sort();
            去重.dedup();
            assert_eq!(去重.len(), 小步翻.len(), "分页翻出了重复的行");
        }
    }
}

#[test]
fn 排序是全序不是页内序() {
    let catalog = 合成库(200);
    let query = VariantQuery {
        order: VariantOrder::Bytes,
        descending: true,
        ..Default::default()
    };
    // 跨两页取，若排序只发生在页内，第二页的第一行会比第一页的最后一行大。
    let 第一页 = catalog.variant_page(&query, 0, 20).expect("取得出");
    let 第二页 = catalog.variant_page(&query, 20, 20).expect("取得出");
    assert!(
        第一页.last().expect("非空").bytes >= 第二页.first().expect("非空").bytes,
        "第二页的容量比第一页还大，排序没有下推到库里",
    );
}

#[test]
fn 筛选也下推到库里() {
    let catalog = 合成库(300);
    let query = VariantQuery {
        contains: "库/PSP/".into(),
        ..Default::default()
    };
    let total = catalog.variant_total(&query).expect("数得出");
    assert_eq!(total, 75, "四个平台里 PSP 该占四分之一");
    let rows = catalog.variant_page(&query, 0, 10).expect("取得出");
    assert_eq!(rows.len(), 10, "筛完还有 75 行，一页该给满 10 行");
    assert!(rows.iter().all(|row| row.key.starts_with("库/PSP/")));
}

#[test]
fn 筛选框里的通配符是普通字符() {
    let catalog = 建库(&[
        变体("库/SFC/百分之 100% 通关.sfc", Some("SFC"), 1, 10),
        变体("库/SFC/百分之百通关.sfc", Some("SFC"), 1, 10),
        变体("库/SFC/下划_线.sfc", Some("SFC"), 1, 10),
        变体("库/SFC/下划X线.sfc", Some("SFC"), 1, 10),
    ]);
    for (打的字, 该有几条) in [("100%", 1u64), ("下划_", 1), ("%", 1)] {
        let query = VariantQuery {
            contains: 打的字.into(),
            ..Default::default()
        };
        assert_eq!(
            catalog.variant_total(&query).expect("数得出"),
            该有几条,
            "筛「{打的字}」时把它当成了通配符",
        );
    }
}

#[test]
fn 平台未知的那些行照样排得进去() {
    // `platform` 可空，而排序键缀了主键。按平台排时 `NULL` 不该让某些行凭空消失——
    // 十行里有一行是平台未知的，翻完仍得是七十行。
    let catalog = 合成库(70);
    let query = VariantQuery {
        order: VariantOrder::Platform,
        ..Default::default()
    };
    assert_eq!(catalog.variant_total(&query).expect("数得出"), 70);
    let keys = 翻完(&catalog, &query, 9);
    assert_eq!(keys.len(), 70);
    let 未知的 = catalog
        .variant_page(&query, 0, 10)
        .expect("取得出")
        .into_iter()
        .filter(|row| row.platform.is_none())
        .count();
    assert!(未知的 > 0, "平台未知的行没排在最前，`NULL` 的次序漂了");
}

/// **按作品名排序是下推出来的**，而且**作品那一格空着的一律排在末尾**（票 `parking-3/11`）。
///
/// 三件事一条测试里断，它们互为前提：
///
/// 1. **真的按作品名排**——期望值是从这一趟取回来的那批名字自己排出来的，不是写死一串。
///    换一份同样合法的数据这条照样成立，而写死那串就只证得了「这批数据碰巧排成这样」。
/// 2. **它与按键排不是同一个次序**。少了这一句，一个「作品名那一档什么都没干、
///    照旧按键排」的实现会一路绿着过去。
/// 3. **作品那一格是空的那些排在末尾，正反两个方向都是**。`NULL` 在 `ORDER BY` 里
///    自己有一套默认次序，跟着方向翻——那意味着人点一下表头，「还没认出作品」的那一批
///    就从表尾跳到表头。位置得是定死的，不是随方向漂的。
#[test]
fn 按作品名排序是下推出来的而且作品那一格空着的一律排在末尾() {
    let catalog = 合成库(300);
    let 按键排 = 翻完(
        &catalog,
        &VariantQuery {
            order: VariantOrder::Key,
            ..Default::default()
        },
        300,
    );
    for descending in [false, true] {
        let query = VariantQuery {
            order: VariantOrder::Work,
            descending,
            ..Default::default()
        };
        let rows = catalog
            .variant_browse_page(&query, 0, MAX_PAGE)
            .expect("取得出一页");
        assert_eq!(rows.len(), 300, "按作品名排完少了行");

        // 空的那一批一律在末尾：找到第一个空的，从它往后必须全空。
        let 空的从哪起 = rows
            .iter()
            .position(|row| row.work.is_none())
            .expect("这份库里该有还没认出作品的变体");
        assert!(
            rows[..空的从哪起].iter().all(|row| row.work.is_some()),
            "作品那一格空着的行插进了有作品的那一段",
        );
        assert!(
            rows[空的从哪起..].iter().all(|row| row.work.is_none()),
            "作品那一格空着的那一批没有连成一段，位置漂了",
        );

        // 有作品的那一段真的按作品名排——期望从数据自己算出来。
        let 名字: Vec<&str> = rows[..空的从哪起]
            .iter()
            .map(|row| row.work.as_deref().expect("这一段全有作品"))
            .collect();
        let mut 期望 = 名字.clone();
        期望.sort_unstable();
        if descending {
            期望.reverse();
        }
        assert_eq!(
            名字,
            期望,
            "按作品名{}排出来的不是作品名的次序",
            if descending { "倒" } else { "正" },
        );

        // 与按键排不是同一个次序，否则这条测试证不出「作品名那一档真的生效了」。
        let 键: Vec<String> = rows
            .iter()
            .map(|row| row.variant.key.clone())
            .collect::<Vec<_>>();
        assert_ne!(
            键, 按键排,
            "按作品名排出来的与按键排出来的一模一样，这批数据分不出真假",
        );
    }
}

/// **作品名是页查询交回来的**，不是回来之后在内存里补的。
///
/// 判据是「翻页也对」：只取中间一页时，界面手上没有别的行、也没有那张作品表——
/// 那一页上每一格作品名都对得上，只可能是查询自己带回来的（D161 的 B 路）。
#[test]
fn 页查询自己带回作品名而不必整份读那张小表() {
    let catalog = 合成库(120);
    let names = catalog.work_names().expect("读得出作品");
    let query = VariantQuery {
        order: VariantOrder::Work,
        ..Default::default()
    };
    // 中间一页：前面那些行压根没取回来过。
    let rows = catalog
        .variant_browse_page(&query, 40, 20)
        .expect("取得出一页");
    assert_eq!(rows.len(), 20);
    for row in &rows {
        assert_eq!(
            row.work,
            row.variant.work_id.and_then(|id| names.get(&id).cloned()),
            "「{}」那一格作品名与作品表说的对不上",
            row.variant.key,
        );
    }
    assert!(
        rows.iter().any(|row| row.work.is_some()),
        "这一页一条有作品的都没有，比不出什么",
    );
}

/// **筛选器里作品名也用得上，而且是下推的**（`Dimension::Work` → `catalog::filter`）。
///
/// 断的是「总数与页内容筛的是同一批」：两处若各筛各的，滚动条会指向不存在的行。
#[test]
fn 按作品名筛也下推到库里() {
    let catalog = 合成库(300);
    let 全部 = catalog
        .variant_total(&VariantQuery::default())
        .expect("数得出");
    let query = VariantQuery {
        rule: Some(Rule::parse("作品^幻想").expect("规则读得懂")),
        ..Default::default()
    };
    let total = catalog.variant_total(&query).expect("数得出");
    assert!(
        total > 0 && total < 全部,
        "按作品名筛出 {total} 行（一共 {全部} 行），这条筛选没起作用",
    );
    let rows = catalog
        .variant_browse_page(&query, 0, MAX_PAGE)
        .expect("取得出一页");
    assert_eq!(rows.len() as u64, total, "总数与页内容筛的不是同一批");
    assert!(
        rows.iter()
            .all(|row| row.work.as_deref().is_some_and(|w| w.starts_with("幻想"))),
        "筛出来的行里有作品名对不上的",
    );
}

#[test]
fn 一页取不出全库() {
    let catalog = 合成库(50);
    let rows = catalog
        .variant_page(&VariantQuery::default(), 0, u64::MAX)
        .expect("取得出");
    assert_eq!(rows.len(), 50, "库里只有 50 行");

    let 大库 = 合成库(MAX_PAGE + 100);
    let rows = 大库
        .variant_page(&VariantQuery::default(), 0, u64::MAX)
        .expect("取得出");
    assert_eq!(
        rows.len() as u64,
        MAX_PAGE,
        "要 u64::MAX 行居然真给了——那这个入口拦不住「把全库读进内存」",
    );
}

#[test]
fn 越过末尾取一页得到空表() {
    let catalog = 合成库(30);
    let rows = catalog
        .variant_page(&VariantQuery::default(), 1_000, 10)
        .expect("取得出");
    assert!(rows.is_empty(), "越过末尾还取出了行");
}

/// **筛选面板那五档的往返**：印出去的那个词，认得回来同一档。
///
/// 面板上每一档只有一串字可点，点中之后靠 [`StateFilter::from_label`] 认回是哪一档。
/// 两处各抄一遍那几个字的话，改一处、断一处——而断了的样子是「点了没反应」，
/// 一条编译错误都没有（票 `gui-redesign/17`）。
#[test]
fn 识别状态那五档印出去的词认得回来() {
    for filter in StateFilter::ALL {
        assert_eq!(
            StateFilter::from_label(filter.label()),
            Some(filter),
            "「{}」这一档认不回来",
            filter.label(),
        );
    }

    // **还没识别那一档印的就是词表那个词**（`CONTEXT.md` 的**还没识别**条，
    // 落在 `NOT_RUN_LABEL` 上）。它与置信度第四档「没有候选」**不是同一件事**：
    // 那一档说的是「识别跑过了、一条候选都没有」。
    assert_eq!(StateFilter::Unidentified.label(), NOT_RUN_LABEL);
    assert_eq!(
        StateFilter::from_label(NOT_RUN_LABEL),
        Some(StateFilter::Unidentified)
    );
    assert_ne!(
        StateFilter::Unidentified.label(),
        Tier::Unidentified.label()
    );

    // 认不出的字不许折成某一档——那会让筛选器悄悄换一批行。
    assert_eq!(StateFilter::from_label("还没识别过"), None);
    assert_eq!(StateFilter::from_label(""), None);
}
