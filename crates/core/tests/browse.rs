//! 翻页浏览：排序、筛选、分页下推到中立库这条接缝。
//!
//! 要证的是三件事，每一件都是「不这么做界面就会出错」而不是「这样比较好看」：
//!
//! 1. **一页一页翻完等于一次全取**。并列行的次序若不定死，翻页会漏行与重行——
//!    第 100 页的最后一行在第 101 页再出现一次，而人正在照着这张表做裁决。
//! 2. **筛选用的是子串，不是通配符**。用户在筛选框里打一个 `%` 是在找文件名里的百分号，
//!    不是在写模式。
//! 3. **一页取不出全库**。`limit` 有硬上界，堵住「把四万行读进内存」这条路。

use romcat_core::catalog::browse::{MAX_PAGE, VariantOrder, VariantQuery};
use romcat_core::catalog::{Catalog, VariantRow};
use romcat_core::platform::Manifest;
use romcat_core::shape::Variant;

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

/// 一份形状照真库来的合成库：平台若干、容量大量并列、还有**平台未知**的一档。
fn 合成库(rows: u64) -> Catalog {
    let 平台 = ["SFC", "PS1", "PSP", "NDS"];
    let variants: Vec<Variant> = (0..rows)
        .map(|i| {
            变体(
                &format!("{}/幻想传说 {i:05} 汉化版.zip", 平台[(i % 4) as usize]),
                // 每七个留一个平台未知：那是真库里存在的一档（认不出平台的内容照常入库），
                // 排序遇到 `NULL` 时不该翻车。
                (i % 7 != 2).then(|| 平台[(i % 4) as usize]),
                1 + i % 3,
                // 只有十种取值，于是并列一大片。
                (i % 10) * 1_000_000,
            )
        })
        .collect();
    建库(&variants)
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
        contains: "PSP/".into(),
        ..Default::default()
    };
    let total = catalog.variant_total(&query).expect("数得出");
    assert_eq!(total, 75, "四个平台里 PSP 该占四分之一");
    let rows = catalog.variant_page(&query, 0, 10).expect("取得出");
    assert_eq!(rows.len(), 10, "筛完还有 75 行，一页该给满 10 行");
    assert!(rows.iter().all(|row| row.key.starts_with("PSP/")));
}

#[test]
fn 筛选框里的通配符是普通字符() {
    let catalog = 建库(&[
        变体("SFC/百分之 100% 通关.sfc", Some("SFC"), 1, 10),
        变体("SFC/百分之百通关.sfc", Some("SFC"), 1, 10),
        变体("SFC/下划_线.sfc", Some("SFC"), 1, 10),
        变体("SFC/下划X线.sfc", Some("SFC"), 1, 10),
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
