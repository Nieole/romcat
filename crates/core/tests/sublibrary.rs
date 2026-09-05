//! 验收**子库与选择集**这一层：子库是持久实体、规则可重放、例外优先于规则且永久记住、
//! 一个主库上多个子库互不干扰、选中多少条多少容量数得出来。
//!
//! 规则的写法与求值本身在 `sublibrary` 与 `sublibrary::rule` 的单元测试里验；
//! 这个文件验的是**它们与中立库接上之后**还成不成立——存进去读回来是不是同一份，
//! 事实是不是真的从三层内容层级、识别结论、刮削结论、合集那几张表折出来的。

use romcat_core::catalog::scrape::{Harvested, HarvestedValue};
use romcat_core::catalog::{Candidate, Catalog, Confidence, Identification, Provenance, State};
use romcat_core::dat::Convention;
use romcat_core::dat::chinese::ChineseMark;
use romcat_core::platform::Manifest;
use romcat_core::scrape::Field;
use romcat_core::shape::{Role, SINGLE_FILE_RULE, Variant};
use romcat_core::sublibrary::{
    self, Exception, Gauge, LoadedSelection, Rule, StoredRule, Sublibrary,
};

fn 变体(key: &str, platform: &str, bytes: u64) -> Variant {
    Variant {
        key: key.to_string(),
        platform: Some(platform.to_string()),
        rule: SINGLE_FILE_RULE.to_string(),
        main_key: key.to_string(),
        manual: false,
        files: 1,
        bytes,
        unreadable_files: 0,
        members: vec![(key.to_string(), Role::Main)],
    }
}

fn 标上中文(catalog: &mut Catalog, key: &str, mark: ChineseMark) {
    catalog
        .write_identifications(&[Identification {
            variant_key: key.to_string(),
            state: State::Matched,
            reason: None,
            units: 1,
            nkit: 0,
            read_bytes: 0,
            work_id: None,
            release_id: None,
            candidates: vec![Candidate {
                member_key: key.to_string(),
                inner: String::new(),
                confidence: Confidence::High,
                accepted: true,
                source: "TOSEC".to_string(),
                dat: "测试.dat".to_string(),
                platform: "GB".to_string(),
                game: "测试条目".to_string(),
                rom: "测试.gb".to_string(),
                hashed_as: Convention::AsIs,
                dat_convention: Convention::AsIs,
                evidence: "测试".to_string(),
                chinese: Some(mark),
                serial: None,
                release_id: None,
            }],
        }])
        .expect("识别结论写得进");
}

/// 一份小库：三个变体，一个汉化的、一个官中的、一个什么都不是的。
fn 现场() -> Catalog {
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    catalog
        .replace_variants(
            &[
                变体("库/GB/口袋妖怪 汉化.zip", "GB", 4 * 1024 * 1024),
                变体("库/GB/官中版.zip", "GB", 2 * 1024 * 1024),
                变体("库/PSV/大作.vpk", "PSV", 3 * 1024 * 1024 * 1024),
            ],
            1,
            &Manifest::builtin(),
        )
        .expect("变体写得进");
    标上中文(
        &mut catalog,
        "库/GB/口袋妖怪 汉化.zip",
        ChineseMark::FanTranslated,
    );
    标上中文(&mut catalog, "库/GB/官中版.zip", ChineseMark::Official);
    catalog
}

/// 读通一条规则再写进去——`add_rule` 收的就是读通了的 [`Rule`]。
fn 加规则(catalog: &mut Catalog, name: &str, text: &str) -> i64 {
    let rule = Rule::parse(text).expect("规则读得懂");
    catalog.add_rule(name, &rule).expect("规则写得进")
}

fn 建子库(catalog: &mut Catalog, name: &str, capacity: Option<u64>) {
    catalog
        .put_sublibrary(&Sublibrary {
            name: name.to_string(),
            target: format!("/Volumes/SDCARD/{name}"),
            target_raw: Some(format!("/Volumes/SDCARD/{name}")),
            format: "Pegasus".to_string(),
            capacity,
            capability: None,
        })
        .expect("子库写得进");
}

/// 求一次值，返回 `(选中几个, 共多少字节)`。
fn 求值(catalog: &Catalog, name: &str) -> (usize, u64) {
    let loaded = catalog.selection(name).expect("选择集读得回来");
    assert!(loaded.broken.is_empty(), "{:?}", loaded.broken);
    let facts = sublibrary::facts(catalog).expect("事实折得出来");
    let selected = sublibrary::select(&loaded.selection, &facts);
    (selected.picked.len(), selected.bytes)
}

#[test]
fn 子库是持久实体_配一次反复使用() {
    let mut catalog = 现场();
    建子库(&mut catalog, "掌机", Some(512_000_000_000));
    加规则(&mut catalog, "掌机", "平台=GB 且 中文=汉化");

    // 换一个 `Catalog` 句柄读回来——**跨进程活着**才叫持久实体。
    let 读回来 = catalog.sublibrary("掌机").expect("读得动").expect("在");
    assert_eq!(读回来.target, "/Volumes/SDCARD/掌机");
    assert_eq!(读回来.format, "Pegasus");
    assert_eq!(读回来.capacity, Some(512_000_000_000));
    let rules = catalog.sublibrary_rules("掌机").expect("读得动");
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].text, "平台=GB 且 中文=汉化");

    assert_eq!(求值(&catalog, "掌机"), (1, 4 * 1024 * 1024));
}

#[test]
fn 主库新增符合规则的内容自动落进选择集() {
    let mut catalog = 现场();
    建子库(&mut catalog, "掌机", None);
    加规则(&mut catalog, "掌机", "平台=GB");
    assert_eq!(求值(&catalog, "掌机").0, 2);

    // 再扫一遍主库，多出一个 GB 的东西。**规则一个字没改。**
    catalog
        .replace_variants(
            &[
                变体("库/GB/口袋妖怪 汉化.zip", "GB", 4 * 1024 * 1024),
                变体("库/GB/官中版.zip", "GB", 2 * 1024 * 1024),
                变体("库/GB/新来的.zip", "GB", 1024 * 1024),
                变体("库/PSV/大作.vpk", "PSV", 3 * 1024 * 1024 * 1024),
            ],
            2,
            &Manifest::builtin(),
        )
        .expect("变体写得进");
    assert_eq!(求值(&catalog, "掌机").0, 3, "新增的自动进来，不必重挑");
}

#[test]
fn 例外优先于规则且不被规则重算覆盖() {
    let mut catalog = 现场();
    建子库(&mut catalog, "掌机", None);
    加规则(&mut catalog, "掌机", "平台=GB");
    catalog
        .set_exception(
            "掌机",
            "库/GB/官中版.zip",
            Exception::Exclude,
            Some("这个我玩过了"),
        )
        .expect("例外写得进");
    catalog
        .set_exception("掌机", "库/PSV/大作.vpk", Exception::Include, None)
        .expect("例外写得进");
    assert_eq!(求值(&catalog, "掌机").0, 2, "GB 汉化 + 手动收入的 PSV");

    // 把规则整个换掉——例外一条不动（ADR-0016 那句「永久记住」）。
    assert!(catalog.remove_rule("掌机", 1).expect("删得动"));
    加规则(&mut catalog, "掌机", "平台=GB,PSV");
    let exceptions = catalog.sublibrary_exceptions("掌机").expect("读得动");
    assert_eq!(exceptions.len(), 2);
    assert_eq!(exceptions[0].note.as_deref(), Some("这个我玩过了"));
    assert_eq!(求值(&catalog, "掌机").0, 2, "官中那个照旧被排除在外");

    // 忘掉例外之后它才回来。
    assert!(
        catalog
            .clear_exception("掌机", "库/GB/官中版.zip")
            .expect("删得动")
    );
    assert_eq!(求值(&catalog, "掌机").0, 3);
}

#[test]
fn 一个主库上多个子库互不干扰() {
    let mut catalog = 现场();
    建子库(&mut catalog, "掌机", None);
    建子库(&mut catalog, "备用卡", None);
    加规则(&mut catalog, "掌机", "平台=GB");
    加规则(&mut catalog, "备用卡", "平台=PSV");
    catalog
        .set_exception("掌机", "库/PSV/大作.vpk", Exception::Include, None)
        .expect("例外写得进");

    assert_eq!(求值(&catalog, "掌机").0, 3);
    assert_eq!(求值(&catalog, "备用卡").0, 1, "掌机的例外不影响备用卡");
    assert_eq!(catalog.sublibrary_rules("备用卡").expect("读得动").len(), 1);
    assert_eq!(
        catalog
            .sublibrary_exceptions("备用卡")
            .expect("读得动")
            .len(),
        0
    );

    // 删掉一个不碰另一个。
    assert!(catalog.remove_sublibrary("掌机").expect("删得动"));
    assert!(catalog.sublibrary("掌机").expect("读得动").is_none());
    assert_eq!(catalog.sublibraries().expect("读得动").len(), 1);
    assert_eq!(求值(&catalog, "备用卡").0, 1);
}

#[test]
fn 同一个子库之内规则序号不复用() {
    // 用户照着上一份报告删规则，序号一复用就会删错一条。
    let mut catalog = 现场();
    建子库(&mut catalog, "掌机", None);
    assert_eq!(加规则(&mut catalog, "掌机", "平台=GB"), 1);
    assert_eq!(加规则(&mut catalog, "掌机", "平台=PSV"), 2);
    assert!(catalog.remove_rule("掌机", 2).expect("删得动"));
    assert_eq!(加规则(&mut catalog, "掌机", "平台=SFC"), 3);
}

#[test]
fn 读不懂的规则跳过并报出来_不连累别的() {
    // `add_rule` 收的是读通了的 `Rule`，走它写不进坏规则。会读不懂的只有
    // **人打开这个 SQLite 文件手改过**、或者换了一版程序之后的旧规则——
    // 那一路直接拿存着的原文来测。
    let stored = [
        StoredRule {
            ordinal: 1,
            text: "平台=GB".to_string(),
        },
        StoredRule {
            ordinal: 7,
            text: "标签=汉化".to_string(),
        },
        StoredRule {
            ordinal: 9,
            text: "平台=GB 且 中文=汉化".to_string(),
        },
    ];
    let loaded = LoadedSelection::from_stored(&stored);
    assert_eq!(loaded.selection.rules.len(), 2, "好的两条照常读回来");
    assert_eq!(loaded.ordinals, vec![1, 9], "序号跟着好的那两条走");
    assert_eq!(loaded.broken.len(), 1);
    assert_eq!(loaded.broken[0].ordinal, 7);
    assert!(
        format!("{}", loaded.broken[0].error).contains("认不出维度"),
        "{}",
        loaded.broken[0].error
    );
}

#[test]
fn 读不懂的规则进得了报告_不只写在标准错误上() {
    // 照 JSON 核对的人看不到 stderr，而「有一条规则被跳过了」正是他最需要知道的。
    let mut catalog = 现场();
    建子库(&mut catalog, "掌机", None);
    加规则(&mut catalog, "掌机", "平台=GB");
    let sublibrary = catalog.sublibrary("掌机").expect("读得动").expect("在");
    let mut loaded = catalog.selection("掌机").expect("读得回来");
    // 模拟库里躺着一条读不懂的规则（换一版程序、或者人手改过这个 SQLite 文件）。
    let 坏的 = LoadedSelection::from_stored(&[StoredRule {
        ordinal: 7,
        text: "标签=汉化".to_string(),
    }]);
    loaded.broken = 坏的.broken;

    let facts = sublibrary::facts(&catalog).expect("事实折得出来");
    let selected = sublibrary::select(&loaded.selection, &facts);
    let report = sublibrary::report::SelectionReport::build(
        catalog.location(),
        &sublibrary,
        &loaded,
        &facts,
        &selected,
    );
    assert_eq!(report.broken_rules.len(), 1);
    assert_eq!(report.broken_rules[0].ordinal, 7);
    assert!(report.broken_rules[0].error.contains("认不出维度"));
    assert!(
        report.render_text().contains("条规则读不懂"),
        "报告里要说出口"
    );
    assert!(
        !report.catalog.is_empty(),
        "中立库那一行由 build 填，不靠调用方补"
    );
}

#[test]
fn 事实从三层内容层级与刮削结论折出来() {
    let mut catalog = 现场();
    let work = catalog
        .add_work("口袋妖怪 金", Provenance::Identified)
        .expect("建得了作品");
    let release = catalog
        .add_release(
            work,
            Some("GB"),
            Some("Japan"),
            None,
            Some("Ja,Zh"),
            Provenance::Identified,
        )
        .expect("建得了发行版");
    catalog
        .link_variant("库/GB/口袋妖怪 汉化.zip", Some(work), Some(release))
        .expect("挂得上");
    catalog
        .put_scraped(&[Harvested {
            anchor: "作品".to_string(),
            subject: "口袋妖怪 金".to_string(),
            source: "测试源".to_string(),
            input: "指纹".to_string(),
            values: vec![HarvestedValue {
                field: Field::Year.label().to_string(),
                value: "1999".to_string(),
                evidence: "测试".to_string(),
            }],
            media: Vec::new(),
        }])
        .expect("刮削结论写得进");
    let collection = catalog.add_collection("我通关过的").expect("建得了合集");
    catalog
        .add_to_collection(collection, "库/GB/口袋妖怪 汉化.zip")
        .expect("加得进合集");

    建子库(&mut catalog, "掌机", None);
    for 规则 in [
        "作品~口袋妖怪",
        "语言=Zh",
        "年份<=2000",
        "合集=我通关过的",
        "中文=汉化",
        "体积>2MiB 且 平台=GB",
    ] {
        加规则(&mut catalog, "掌机", 规则);
    }
    let loaded = catalog.selection("掌机").expect("读得回来");
    let facts = sublibrary::facts(&catalog).expect("事实折得出来");
    let selected = sublibrary::select(&loaded.selection, &facts);
    // 六条规则各自都该只选中那一个变体——六个维度全部从库里折出来了。
    assert_eq!(selected.rule_hits, vec![1, 1, 1, 1, 1, 1]);
    assert_eq!(selected.picked.len(), 1);
    assert_eq!(selected.picked[0].key, "库/GB/口袋妖怪 汉化.zip");
}

#[test]
fn 报告数得出选中多少条与多少容量_并报出超限() {
    let mut catalog = 现场();
    // 上限故意压到一个 GB 都不到：PSV 那个 3 GiB 的一定超。
    建子库(&mut catalog, "掌机", Some(1024 * 1024 * 1024));
    加规则(&mut catalog, "掌机", "平台=GB,PSV");

    let sublibrary = catalog.sublibrary("掌机").expect("读得动").expect("在");
    let loaded = catalog.selection("掌机").expect("读得回来");
    let facts = sublibrary::facts(&catalog).expect("事实折得出来");
    let selected = sublibrary::select(&loaded.selection, &facts);
    let report = sublibrary::report::SelectionReport::build(
        catalog.location(),
        &sublibrary,
        &loaded,
        &facts,
        &selected,
    );

    assert_eq!(report.picked, 3);
    assert_eq!(report.variants, 3);
    assert_eq!(report.bytes, 3 * 1024 * 1024 * 1024 + 6 * 1024 * 1024);
    assert_eq!(report.platforms.len(), 2);
    // **不自动截断**：报出超出量与按体积排序的裁剪建议（ADR-0016）。
    assert_eq!(
        report.over_capacity,
        Some(2 * 1024 * 1024 * 1024 + 6 * 1024 * 1024)
    );
    assert_eq!(report.trim_suggestions[0].variant, "库/PSV/大作.vpk");
    assert_eq!(
        report.trim_suggestions[0].cumulative, report.trim_suggestions[0].bytes,
        "累计从最大的那个起算——「砍到第几个才够」直接读得出来"
    );
    let text = report.render_text();
    assert!(text.contains("装不下"), "{text}");
    assert!(text.contains("不会自动截断"), "{text}");
    assert!(text.contains("库/PSV/大作.vpk"), "{text}");
}

#[test]
fn 换掉规则时读不懂的那几条原样留着() {
    // 「**改选择**」那条回程走的是 `replace_rules`（票 `gui-redesign/11`）：筛选器折出来
    // 的是**一条**，所以是换而不是加——加的话旧那几条还在，子库选出来的就比屏上多。
    let mut catalog = 现场();
    建子库(&mut catalog, "掌机", None);
    加规则(&mut catalog, "掌机", "平台=GB");
    加规则(&mut catalog, "掌机", "平台=SFC");
    // 中立库是个 SQLite 文件，人打得开；换一版程序、删掉一个维度之后旧规则也会读不懂。
    catalog
        .add_rule(
            "掌机",
            &Rule {
                text: "这不是一条规则".to_string(),
                root: romcat_core::sublibrary::Group::new(
                    romcat_core::sublibrary::Join::All,
                    Vec::new(),
                ),
            },
        )
        .expect("写得进");

    let 新的 = Rule::parse("平台=GB 或 平台=SFC").expect("读得懂");
    catalog.replace_rules("掌机", &新的).expect("换得了");

    let 剩下的: Vec<String> = catalog
        .sublibrary_rules("掌机")
        .expect("读得动")
        .into_iter()
        .map(|stored| stored.text)
        .collect();
    assert_eq!(
        剩下的,
        vec![
            "这不是一条规则".to_string(),
            "平台=GB 或 平台=SFC".to_string()
        ],
        "读得懂的那两条该被换掉，读不懂的那条该原样留着",
    );
    // **序号不复用**：换一趟之后新那条拿的是下一个号，不是被删掉那两个之一。
    let ordinals: Vec<i64> = catalog
        .sublibrary_rules("掌机")
        .expect("读得动")
        .into_iter()
        .map(|stored| stored.ordinal)
        .collect();
    assert_eq!(ordinals, vec![3, 4]);
}

#[test]
fn 容量条三段各自说得清而且清单之外分得出没有与不知道() {
    // 卡不在手边时目标上有什么本来就没看过——摆一个 0 出去等于说「卡上是空的」。
    let 没排过 = Gauge {
        picked: 300,
        strangers: None,
        capacity: Some(1_000),
    };
    assert_eq!(没排过.taken(), 300);
    assert_eq!(没排过.scale(), 1_000, "没超限时条子照上限画满");
    assert!((没排过.picked_share() - 0.3).abs() < 1e-6);
    assert_eq!(没排过.stranger_share(), 0.0);

    // 排过差量、而且清单之外真的是零：与「不知道」画出来一样，但**说出来的话不同**
    // ——那句话由界面照 `strangers` 是不是 `None` 分。
    let 空的 = Gauge {
        strangers: Some(0),
        ..没排过
    };
    assert_eq!(空的.stranger_share(), 0.0);
    assert_eq!(空的.taken(), 300);

    let 有外人 = Gauge {
        picked: 300,
        strangers: Some(200),
        capacity: Some(1_000),
    };
    assert_eq!(有外人.taken(), 500);
    assert!((有外人.stranger_share() - 0.2).abs() < 1e-6);

    // **超了就照实际占用画满**：不然「正好装满」与「超了三倍」长得一模一样。
    let 超了 = Gauge {
        picked: 900,
        strangers: Some(300),
        capacity: Some(1_000),
    };
    assert_eq!(超了.scale(), 1_200);
    assert!((超了.picked_share() + 超了.stranger_share() - 1.0).abs() < 1e-6);
    assert_eq!(
        sublibrary::over_capacity(超了.capacity, 超了.taken()),
        Some(200),
        "超没超由核心一处算，条子自己不算第二遍",
    );

    // 不设上限时照占用本身画，两段的比例仍然看得出谁大谁小；一个字节都没有就是空条子。
    let 不设限 = Gauge {
        picked: 300,
        strangers: Some(100),
        capacity: None,
    };
    assert_eq!(不设限.scale(), 400);
    assert_eq!(Gauge::default().scale(), 0);
    assert_eq!(Gauge::default().picked_share(), 0.0);
}
