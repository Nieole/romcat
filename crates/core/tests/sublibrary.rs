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
    self, Discarded, Exception, Fit, Gauge, LoadedSelection, Rule, StoredRule, Sublibrary,
};
use romcat_core::sync::{FileKind, Manifest as 同步清单, ManifestFile, Stamp};

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
            platform: None,
            standalone: None,
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
            capacity_by_device: false,
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

/// 把几个变体挂到同一个作品底下（识别结论），交回那个作品的号。
fn 挂到作品(catalog: &mut Catalog, name: &str, keys: &[&str]) -> i64 {
    let work = catalog
        .add_work(name, Provenance::Identified)
        .expect("建得出作品");
    let records: Vec<Identification> = keys
        .iter()
        .map(|key| Identification {
            variant_key: (*key).to_string(),
            platform: None,
            standalone: None,
            state: State::Matched,
            reason: None,
            units: 1,
            nkit: 0,
            read_bytes: 0,
            work_id: Some(work),
            release_id: None,
            candidates: Vec::new(),
        })
        .collect();
    catalog
        .write_identifications(&records)
        .expect("识别结论写得进");
    work
}

#[test]
fn 例外那张表逐条写得出作品平台体积与备注_库里没有那个变体的照旧在() {
    // 票 `gui-looks-like-the-design/22`：手动例外那张表逐条显示作品、平台、体积、备注与时间。
    // 作品名照**显示标题**那一处挑（与浏览屏主列表、详情面板同一份），别处不另挑一遍（ADR-0024）。
    let mut catalog = 现场();
    建子库(&mut catalog, "掌机", None);
    挂到作品(&mut catalog, "口袋妖怪 红", &["库/GB/口袋妖怪 汉化.zip"]);
    catalog
        .set_exception(
            "掌机",
            "库/GB/口袋妖怪 汉化.zip",
            Exception::Include,
            Some("小时候玩的就是这一版"),
        )
        .expect("例外写得进");
    // **库里眼下没有这个变体**：盘没插、目录改了名。例外照旧记着，不删（ADR-0016）。
    catalog
        .set_exception("掌机", "库/GB/盘没插时看不到的.zip", Exception::Exclude, None)
        .expect("例外写得进");

    let 表 = catalog
        .sublibrary_exception_details("掌机", &romcat_core::scrape::Priorities::builtin())
        .expect("读得动");
    assert_eq!(表.len(), 2, "两条都要在表上");
    // **次序是「时刻倒着排，同一刻按键排」**：这两条是同一秒写进去的（库里记的是秒），
    // 所以这里看得见的是后半句。写成「照这条规矩排好了」而不是「第几条是谁」——
    // 断言挂在规矩上，不挂在这一趟碰巧几点几秒。
    let 照规矩 = {
        let mut 照规矩 = 表.clone();
        照规矩.sort_by(|a, b| {
            b.row
                .at
                .cmp(&a.row.at)
                .then_with(|| a.row.variant_key.cmp(&b.row.variant_key))
        });
        照规矩
    };
    assert_eq!(表, 照规矩, "没按「时刻倒着排、同一刻按键排」摆");

    let 认一条 = |key: &str| {
        表.iter()
            .find(|detail| detail.row.variant_key == key)
            .unwrap_or_else(|| panic!("表上没有「{key}」"))
    };
    let 没在库里 = 认一条("库/GB/盘没插时看不到的.zip");
    assert!(没在库里.missing(), "库里没有这个变体，这一条该说得出口");
    assert_eq!(没在库里.bytes, None, "库里没有的那一条体积不写 0");
    assert_eq!(没在库里.platform, None);
    assert_eq!(
        没在库里.title(),
        "库/GB/盘没插时看不到的.zip",
        "认不出作品的那一行写变体的键"
    );

    let 在库里 = 认一条("库/GB/口袋妖怪 汉化.zip");
    assert_eq!(在库里.row.kind, Exception::Include);
    assert_eq!(
        在库里.row.note.as_deref(),
        Some("小时候玩的就是这一版"),
        "备注那一列"
    );
    assert!(在库里.row.at > 0, "时间那一列");
    assert!(!在库里.missing());
    assert_eq!(在库里.title(), "口袋妖怪 红", "作品那一列");
    assert_eq!(在库里.platform.as_deref(), Some("GB"));
    assert_eq!(在库里.bytes, Some(4 * 1024 * 1024), "体积那一列");
}

#[test]
fn 一个作品整批记成例外_从一栏换到另一栏不留两条() {
    // 票 `gui-looks-like-the-design/22`：「搜索作品直接添加为包含或排除；同一个作品从一栏换到
    // 另一栏时不会留下两条」。例外落在**变体**这一层，一个作品底下那几个变体整批写。
    let mut catalog = 现场();
    建子库(&mut catalog, "掌机", None);
    let 两份 = ["库/GB/口袋妖怪 汉化.zip", "库/GB/官中版.zip"];
    挂到作品(&mut catalog, "口袋妖怪 红", &两份);

    assert_eq!(
        catalog
            .set_exceptions("掌机", &两份, Exception::Include, Some("小时候玩过"))
            .expect("例外写得进"),
        2,
    );
    let 表 = catalog.sublibrary_exceptions("掌机").expect("读得动");
    assert_eq!(表.len(), 2);
    assert!(表.iter().all(|row| row.kind == Exception::Include));

    // 换到另一栏：还是两条，方向全变了——不是四条。
    catalog
        .set_exceptions("掌机", &两份, Exception::Exclude, None)
        .expect("例外写得进");
    let 表 = catalog.sublibrary_exceptions("掌机").expect("读得动");
    assert_eq!(表.len(), 2, "换一栏留下了两份记录");
    assert!(
        表.iter().all(|row| row.kind == Exception::Exclude),
        "换过去的方向没落到每一份上：{表:?}"
    );
    assert!(
        表.iter().all(|row| row.note.is_none()),
        "换栏时那一句备注该跟着换成新的（这一趟没写备注）"
    );
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
fn 只换第几条规则_序号不变_别的规则一条不碰() {
    // 票 `gui-looks-like-the-design/20`（拿主意的人 2026-09-14 定）：子库屏规则行上「✎」只改这一条。
    let mut catalog = 现场();
    建子库(&mut catalog, "掌机", None);
    建子库(&mut catalog, "备用卡", None);
    加规则(&mut catalog, "掌机", "平台=GB");
    加规则(&mut catalog, "掌机", "平台=PSV");
    加规则(&mut catalog, "备用卡", "平台=GB");
    let 新的 = Rule::parse("平台=GB 且 中文=汉化").expect("规则读得懂");
    assert!(catalog.replace_rule("掌机", 1, &新的).expect("写得进"));
    let 规则: Vec<(i64, String)> = catalog
        .sublibrary_rules("掌机")
        .expect("读得动")
        .into_iter()
        .map(|stored| (stored.ordinal, stored.text))
        .collect();
    assert_eq!(
        规则,
        vec![(1, 新的.text.clone()), (2, "平台=PSV".to_string())],
        "换掉的不只是第 1 条，或者序号变了"
    );
    assert_eq!(
        catalog.sublibrary_rules("备用卡").expect("读得动")[0].text,
        "平台=GB",
        "别的子库的规则被碰了"
    );
    assert_eq!(加规则(&mut catalog, "掌机", "平台=SFC"), 3, "发号器被动了");
    assert!(
        !catalog.replace_rule("掌机", 9, &新的).expect("读得动"),
        "不在的那一条该交回 false"
    );
    assert_eq!(
        catalog.sublibrary_rules("掌机").expect("读得动").len(),
        3,
        "不在的那一条也写进去了"
    );
}

#[test]
fn 删掉子库交回整份_原样放回去之后与删之前逐列一样() {
    // 票 `gui-looks-like-the-design/20`（拿主意的人 2026-09-14 定）：界面上删掉一个子库之后，提示条上有一颗
    // 「撤销」。放回去的得是**同一份**：规则的序号与下一个发几号、例外、清单一样不差——清单少一行，同步就把
    // 自己放过的文件当成清单之外，碰都不敢碰；规则从 1 号重新发，照着旧报告删 2 号就删错一条。
    let mut catalog = 现场();
    建子库(&mut catalog, "掌机", Some(64_000_000_000));
    建子库(&mut catalog, "备用卡", None);
    加规则(&mut catalog, "掌机", "平台=GB");
    加规则(&mut catalog, "掌机", "平台=PSV");
    加规则(&mut catalog, "备用卡", "平台=PSV");
    // 删掉 1 号：下一条发 3 号，放回去之后也得接着发 3 号。
    assert!(catalog.remove_rule("掌机", 1).expect("删得动"));
    catalog
        .set_exception("掌机", "库/PSV/大作.vpk", Exception::Include, Some("想玩"))
        .expect("例外写得进");
    let 清单 = 同步清单 {
        files: vec![ManifestFile {
            path: "roms/gb/口袋妖怪 汉化.zip".to_string(),
            kind: FileKind::Rom,
            stamp: Stamp {
                bytes: 4096,
                mtime_ns: Some(1_700_000_000_000_000_000),
            },
            source: "库/GB/口袋妖怪 汉化.zip".to_string(),
            source_stamp: Stamp {
                bytes: 4096,
                mtime_ns: None,
            },
            variant: "库/GB/口袋妖怪 汉化.zip".to_string(),
            absent: true,
        }],
    };
    catalog.put_manifest("掌机", &清单).expect("清单写得进");
    let 读一遍 = |catalog: &Catalog| {
        (
            catalog.sublibrary("掌机").expect("读得动"),
            catalog.sublibrary_rules("掌机").expect("读得动"),
            catalog.sublibrary_exceptions("掌机").expect("读得动"),
            catalog.manifest("掌机").expect("读得动"),
        )
    };
    let 之前 = 读一遍(&catalog);
    assert!(
        之前.0.is_some() && !之前.3.files.is_empty(),
        "前提：子库与清单都在"
    );

    let 留下的 = catalog
        .take_sublibrary("掌机")
        .expect("删得动")
        .expect("本来在");
    assert_eq!(留下的.name(), "掌机");
    let 删了之后 = 读一遍(&catalog);
    assert!(删了之后.0.is_none(), "交回来了却没删");
    assert!(删了之后.1.is_empty() && 删了之后.2.is_empty() && 删了之后.3.files.is_empty());
    assert_eq!(
        catalog.sublibrary_rules("备用卡").expect("读得动").len(),
        1,
        "删掉一个不碰另一个"
    );
    assert!(
        catalog.take_sublibrary("掌机").expect("读得动").is_none(),
        "不在的交回 None"
    );

    assert!(catalog.restore_sublibrary(&留下的).expect("写得进"));
    assert_eq!(读一遍(&catalog), 之前, "放回来的与删之前读出来的不一样");
    // **逐列**：再整份拿一次与头一份比——这一份连 `next_rule`、每一行记下的时刻都带着。
    let 再拿一次 = catalog
        .take_sublibrary("掌机")
        .expect("删得动")
        .expect("放回来了");
    assert_eq!(再拿一次, 留下的, "放回来的那几列与删之前不一样");
    assert!(catalog.restore_sublibrary(&再拿一次).expect("写得进"));
    assert_eq!(
        加规则(&mut catalog, "掌机", "平台=GB"),
        3,
        "下一条规则没接着删之前的号发"
    );

    // 删完之后人又建了一个同名的：一行都不写，两份不揉在一起。
    let 又删一次 = catalog
        .take_sublibrary("掌机")
        .expect("删得动")
        .expect("在");
    建子库(&mut catalog, "掌机", None);
    assert!(
        !catalog.restore_sublibrary(&又删一次).expect("读得动"),
        "同名的已经有了还往里写"
    );
    let 新建的 = 读一遍(&catalog);
    assert!(新建的.1.is_empty(), "旧的规则揉进了新建的那一个");
    assert!(新建的.2.is_empty(), "旧的例外揉进了新建的那一个");
    assert!(新建的.3.files.is_empty(), "旧的清单揉进了新建的那一个");
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
        Fit::Unknown {
            why: "这份现场没有卡".to_string(),
        },
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
fn 报告数得出选中多少条与多少容量() {
    let mut catalog = 现场();
    建子库(&mut catalog, "掌机", None);
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
        // 这份现场的变体键是摆出来的，没有真文件、也没有卡：装不装得下由
        // `装得下吗与同步计划器同底_…` 那几条对着真卡验，这里只验选中那几个数。
        Fit::Unknown {
            why: "这份现场没有卡".to_string(),
        },
    );

    assert_eq!(report.picked, 3);
    assert_eq!(report.variants, 3);
    assert_eq!(report.bytes, 3 * 1024 * 1024 * 1024 + 6 * 1024 * 1024);
    assert_eq!(report.platforms.len(), 2);
    assert!(report.render_text().contains("规则之间没有重复"));
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
fn 扔掉读不懂的那一条不碰读得懂的那几条与例外() {
    // 票 `gui-redesign/14`：一条读不回来的规则在界面上处置得掉，而处置它**只删它自己**。
    // 这道闸落在核心里而不是界面上（ADR-0005）：界面只递一个序号过来，
    // 「这个号该不该删」由这儿判——不然一次「扔掉读不懂的」就能删掉一条好的。
    let mut catalog = 现场();
    建子库(&mut catalog, "掌机", None);
    let 好的 = 加规则(&mut catalog, "掌机", "平台=GB");
    // 中立库是个 SQLite 文件，人打得开；换一版程序、删掉一个维度之后旧规则也会读不懂。
    let 坏的 = catalog
        .add_rule(
            "掌机",
            &Rule {
                text: "这不是一条规则".to_string(),
                root: sublibrary::Group::new(sublibrary::Join::All, Vec::new()),
            },
        )
        .expect("写得进");
    catalog
        .set_exception(
            "掌机",
            "库/PSV/大作.vpk",
            Exception::Include,
            Some("小时候玩过"),
        )
        .expect("例外写得进");

    // **读得懂的那条这条路删不掉**：拒绝，而且库里一条都没少。
    assert_eq!(
        catalog.discard_broken_rule("掌机", 好的).expect("问得动"),
        Discarded::Readable,
    );
    assert_eq!(catalog.sublibrary_rules("掌机").expect("读得动").len(), 2);

    // 读不懂的那条扔得掉。
    assert_eq!(
        catalog.discard_broken_rule("掌机", 坏的).expect("问得动"),
        Discarded::Gone,
    );
    let 剩下的: Vec<(i64, String)> = catalog
        .sublibrary_rules("掌机")
        .expect("读得动")
        .into_iter()
        .map(|stored| (stored.ordinal, stored.text))
        .collect();
    assert_eq!(
        剩下的,
        vec![(好的, "平台=GB".to_string())],
        "该只剩读得懂的那一条",
    );
    // **例外是永久记住的**（ADR-0016）：扔一条规则不许顺手把它清掉。
    let 例外 = catalog.sublibrary_exceptions("掌机").expect("读得动");
    assert_eq!(例外.len(), 1);
    assert_eq!(例外[0].variant_key, "库/PSV/大作.vpk");
    assert_eq!(例外[0].note.as_deref(), Some("小时候玩过"));
    // 一条读不懂的规则本来就没参与求值——扔掉它**不改变这个子库选出什么**。
    assert_eq!(求值(&catalog, "掌机").0, 3, "两个 GB 变体加那条收入的例外");

    // 再扔一遍：已经不在了，不是错。两个窗口开着同一份库时就是这个样子。
    assert_eq!(
        catalog.discard_broken_rule("掌机", 坏的).expect("问得动"),
        Discarded::Absent,
    );
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

// ───────────────────────── 装得下吗：与同步计划器同底（挂账 D76）

/// 一份**落在磁盘上**的小主库、扫进来的中立库、一个工作目录、一张当目标用的卡。
///
/// 「装得下吗」要看目标上实际有什么，于是这几条测试不能再拿假的变体键摆现场：
/// 期望状态要从真实的成员折出来，卡上要真的躺着东西。
struct 一张卡 {
    _库目录: romcat_core::testing::TempDir,
    工作区: romcat_core::testing::TempDir,
    卡: romcat_core::testing::TempDir,
    catalog: Catalog,
}

fn 写(path: &std::path::Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    std::fs::write(path, bytes).expect("能写文件");
}

impl 一张卡 {
    /// 主库里两个 FC、一个 GB；卡上预先躺着维护者自己拷进去的 `存档` 那么多字节。
    fn 摆好(存档: usize) -> Self {
        use romcat_core::fs::RealFs;
        use romcat_core::scan::{self, Jobs, ScanOptions};
        use romcat_core::task::Handle;
        use romcat_core::testing::sample::zip;
        use romcat_core::testing::temp_dir;

        let 库目录 = temp_dir("sublib-fit-lib");
        写(&库目录.path().join("FC/魂斗罗.zip"), &zip(2048));
        写(&库目录.path().join("FC/超级玛丽.zip"), &zip(4096));
        写(&库目录.path().join("GB/口袋妖怪.zip"), &zip(8192));
        let 工作区 = temp_dir("sublib-fit-ws");
        let 卡 = temp_dir("sublib-fit-card");
        if 存档 > 0 {
            写(&卡.path().join("saves/魂斗罗.sav"), &vec![9u8; 存档]);
        }
        let mut catalog = Catalog::open_in_memory().expect("能开中立库");
        let mut options = ScanOptions::named(库目录.path(), "库");
        options.jobs = Jobs::Fixed(2);
        scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");
        Self {
            _库目录: 库目录,
            工作区,
            卡,
            catalog,
        }
    }

    /// 这条规则只比**选中容量**的话选出多少字节——旧口径那个数。
    fn 选中容量(&self, 规则: &str) -> u64 {
        let selection = sublibrary::Selection {
            rules: vec![Rule::parse(规则).expect("规则读得懂")],
            exceptions: Vec::new(),
        };
        let facts = sublibrary::facts(&self.catalog).expect("事实折得出来");
        sublibrary::select(&selection, &facts).bytes
    }

    /// 建「掌机」这个子库，目标指着这张卡。
    fn 建子库(&mut self, 规则: &str, capacity: Option<u64>) {
        self.catalog
            .put_sublibrary(&Sublibrary::at("掌机", self.卡.path(), "Pegasus", capacity))
            .expect("子库写得进");
        加规则(&mut self.catalog, "掌机", 规则);
    }

    /// 同步计划器那一份。
    fn 排计划(&self) -> romcat_core::sync::Prepared {
        romcat_core::sync::prepare(
            &self.catalog,
            self.工作区.path(),
            "掌机",
            &romcat_core::sync::Request::default(),
            &romcat_core::task::Handle::new(),
        )
        .expect("排得出计划")
    }

    /// 子库报告那一份：界面上「算一遍容量」与 `romcat sublibrary show` 走的那一趟。
    fn 算容量(&self) -> sublibrary::report::SelectionReport {
        let list = self.catalog.sublibraries().expect("读得出子库");
        let mut reports = sublibrary::survey(
            &self.catalog,
            self.工作区.path(),
            &list,
            &romcat_core::task::Handle::new(),
        )
        .expect("算得出来");
        reports.remove("掌机").expect("一台设备一份报告")
    }
}

#[test]
fn 装得下吗与同步计划器同底_卡上清单之外的东西也算进去() {
    let mut 场 = 一张卡::摆好(512);
    // 上限**正好等于选中容量**：只比选中容量的话，这里该说「装得下」。
    let 选中 = 场.选中容量("平台=FC");
    场.建子库("平台=FC", Some(选中));

    let prepared = 场.排计划();
    let plan = &prepared.plan;
    // 独立的账：卡上眼下只躺着维护者那份 512 字节的存档，它在清单之外。
    assert_eq!(plan.actual_bytes, 512, "目标现占");
    assert_eq!(plan.stranger_bytes, 512, "清单之外");
    let over = plan
        .over_capacity
        .expect("卡上那份存档还占着地方，计划器该说装不下");
    // 清单之外那一份**正好**被算进去：同一批文件、同一个上限，卡上没有那份存档时，
    // 超出量整整少 512 字节（剩下那一截是前端元数据占的）。
    let mut 空卡 = 一张卡::摆好(0);
    let 空卡选中 = 空卡.选中容量("平台=FC");
    空卡.建子库("平台=FC", Some(空卡选中));
    let 空卡超出 = 空卡
        .算容量()
        .fit
        .known()
        .and_then(|room| room.over_capacity)
        .expect("前端元数据也占地方，空卡照样超出一截");
    assert_eq!(over - 空卡超出, 512, "卡上那份存档没被算进装得下吗");

    let report = 场.算容量();
    let room = report.fit.known().expect("卡在手边，报告该算得出");
    assert_eq!(
        (room.after_bytes, room.over_capacity),
        (plan.after_bytes, plan.over_capacity),
        "同一批文件、同一个目标，子库报告与同步计划器说的不是同一个数",
    );
    assert_eq!(
        room.actual_bytes, 512,
        "报告那一侧的目标现占也得把存档算进去"
    );
    assert_eq!(room.stranger_bytes, 512);
    let text = report.render_text();
    assert!(text.contains("装不下"), "{text}");
    assert!(!text.contains("装得下："), "{text}");
}

#[test]
fn 目标不在位时装不装得下如实说算不出_不给一个数() {
    let mut 场 = 一张卡::摆好(0);
    let 选中 = 场.选中容量("平台=FC");
    // 上限远大于选中容量：只比选中容量的话，这里会信心十足地说「装得下」。
    场.建子库("平台=FC", Some(选中 * 1000));
    // 卡没插：把当目标用的那个目录整个挪走。
    std::fs::remove_dir_all(场.卡.path()).expect("删得掉");

    let report = 场.算容量();
    assert_eq!(
        report.bytes, 选中,
        "选中多少只问中立库，卡不在手边照样算得出"
    );
    match &report.fit {
        Fit::Unknown { why } => assert!(why.contains("目标未连接"), "{why}"),
        Fit::Known(room) => panic!("卡不在手边却给了一个数：{room:?}"),
    }
    let text = report.render_text();
    assert!(text.contains("算不出"), "{text}");
    assert!(!text.contains("装得下："), "{text}");
    assert!(!text.contains("装不下"), "{text}");
    // 照 JSON 核对的人也读不出一个「装得下」：没有一个空着的超出量摆在顶层。
    let json = serde_json::to_value(&report).expect("序列化得了");
    assert!(json.get("over_capacity").is_none(), "{json}");
    assert!(json["fit"]["Unknown"]["why"].is_string(), "{json}");
}

// ——— 目标路径当场校验（票 `gui-looks-like-the-design/21`）———
//
// 新建子库、改目标设置时，路径一填进来就判一遍：落在主库的根里、属于工作目录、已被别的子库占用，
// 三种当场拦下并说清是哪一种。**判断在核心里**（`sublibrary::target`），界面只画那句话。

/// 一个根（叫「主库」）挂在 `盘/Game` 上的中立库。
fn 带一个根(盘: &std::path::Path) -> Catalog {
    let 根 = 盘.join("Game");
    std::fs::create_dir_all(&根).expect("能建目录");
    let catalog = Catalog::open_in_memory().expect("能开中立库");
    romcat_core::catalog::roots::add_root(
        &catalog,
        None,
        "主库",
        &romcat_core::path::normalize_existing(&根),
    )
    .expect("加得上");
    catalog
}

#[test]
fn 目标落在主库的根里或者把根包在里面_当场拦下并点名是哪个根() {
    use romcat_core::sublibrary::target::{self, TargetRefusal};
    use romcat_core::testing::temp_dir;

    let 盘 = temp_dir("sub-target-disk");
    let 工作区 = temp_dir("sub-target-ws");
    let catalog = 带一个根(盘.path());
    let 根 = 盘.path().join("Game");
    // 根里、根本身、把根包在里面的上一级（同步往 `<平台>/…` 写，平台目录与根同名时就写进了主库）。
    for 目标 in [根.join("GBA"), 根.clone(), 盘.path().to_path_buf()] {
        match target::vet(&catalog, 工作区.path(), None, &目标).expect("中立库读得动") {
            Err(TargetRefusal::InLibrary { root, .. }) => assert_eq!(root, "主库"),
            other => panic!("{} 该被拦成落在主库里：{other:?}", 目标.display()),
        }
    }
    // 同步那一道闸是同一条判断：把根包在里面的目标，点同步时一样拦下。
    let 话 = romcat_core::sync::prepare::refuse_target_in_library(&catalog, &[], 盘.path())
        .expect_err("把根包在里面也该被拒");
    assert!(话.contains("主库只读"), "红线要说出来：{话}");
}

#[test]
fn 目标属于工作目录或者把工作目录包在里面_当场拦下() {
    use romcat_core::sublibrary::target::{self, TargetRefusal};
    use romcat_core::testing::temp_dir;

    let 盘 = temp_dir("sub-target-disk");
    let 外 = temp_dir("sub-target-outer");
    let 工作区 = 外.path().join("romcat");
    std::fs::create_dir_all(&工作区).expect("能建目录");
    let catalog = 带一个根(盘.path());
    for 目标 in [工作区.join("子库"), 工作区.clone(), 外.path().to_path_buf()] {
        match target::vet(&catalog, &工作区, None, &目标).expect("中立库读得动") {
            Err(TargetRefusal::InWorkspace { .. }) => {}
            other => panic!("{} 该被拦成属于工作目录：{other:?}", 目标.display()),
        }
    }
}

#[test]
fn 目标已被别的子库占用_相同或者套在一起都拦下_改自己那一台不算() {
    use romcat_core::sublibrary::target::{self, TargetRefusal};
    use romcat_core::testing::temp_dir;

    let 盘 = temp_dir("sub-target-disk");
    let 工作区 = temp_dir("sub-target-ws");
    let 卡 = temp_dir("sub-target-card");
    let mut catalog = 带一个根(盘.path());
    let 掌机的 = 卡.path().join("掌机");
    catalog
        .put_sublibrary(&Sublibrary::at("掌机", &掌机的, "Pegasus", None))
        .expect("子库写得进");
    for 目标 in [掌机的.clone(), 掌机的.join("里头"), 卡.path().to_path_buf()] {
        match target::vet(&catalog, 工作区.path(), None, &目标).expect("中立库读得动") {
            Err(TargetRefusal::Taken { by }) => assert_eq!(by, "掌机"),
            other => panic!("{} 该被拦成已被「掌机」占用：{other:?}", 目标.display()),
        }
    }
    // 改「掌机」自己的目标设置时，它原来那条路径不算被占。
    assert!(
        target::vet(&catalog, 工作区.path(), Some("掌机"), &掌机的)
            .expect("中立库读得动")
            .is_ok(),
        "改自己那一台，原路径不该被拦"
    );
}

#[test]
fn 目标在不在_在就报出卷上的可用空间_不在也照样建得出() {
    use romcat_core::sublibrary::target::{self, Presence, TargetRefusal};
    use romcat_core::testing::temp_dir;

    let 盘 = temp_dir("sub-target-disk");
    let 工作区 = temp_dir("sub-target-ws");
    let 卡 = temp_dir("sub-target-card");
    let catalog = 带一个根(盘.path());

    match target::vet(&catalog, 工作区.path(), None, 卡.path()).expect("中立库读得动") {
        Ok(Presence::Present(volume)) => {
            // 三个平台都读得出：可移动与否、文件系统、总量与可用空间（拿主意的人 2026-09-15 定，引一个跨平台依赖）。
            let 可用 = volume.available.expect("读得出可用空间");
            let 总量 = volume.total.expect("读得出总量");
            assert!(可用 <= 总量, "可用 {可用} 比总量 {总量} 还大");
            assert!(volume.filesystem.is_some(), "读得出文件系统：{volume:?}");
            let _可移动: bool = volume.removable;
        }
        other => panic!("插着的卡该报在：{other:?}"),
    }
    let 没插 = 卡.path().join("没插上的卡");
    assert!(
        matches!(
            target::vet(&catalog, 工作区.path(), None, &没插).expect("中立库读得动"),
            Ok(Presence::Absent)
        ),
        "不在的目录照样建得出，只是说不在"
    );
    let 一份文件 = 卡.path().join("一份文件.txt");
    std::fs::write(&一份文件, b"x").expect("能写文件");
    assert!(
        matches!(
            target::vet(&catalog, 工作区.path(), None, &一份文件).expect("中立库读得动"),
            Err(TargetRefusal::NotADirectory)
        ),
        "路径上是一份文件时要拦下"
    );
    assert!(matches!(
        target::vet(&catalog, 工作区.path(), None, std::path::Path::new("")).expect("中立库读得动"),
        Err(TargetRefusal::Empty)
    ));
}

#[test]
fn 名字空着或者已被别的子库用了_当场拦下_改自己那一台不算() {
    use romcat_core::sublibrary::target::{self, NameRefusal};

    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    建子库(&mut catalog, "掌机", None);
    建子库(&mut catalog, "备份卡", None);
    assert_eq!(
        target::vet_name(&catalog, None, "  ").expect("读得动"),
        Err(NameRefusal::Empty)
    );
    // 新建一台同名的：核心按名字存，放行就是悄悄把「掌机」的目标设置盖掉。
    assert_eq!(
        target::vet_name(&catalog, None, "掌机").expect("读得动"),
        Err(NameRefusal::Taken)
    );
    assert_eq!(
        target::vet_name(&catalog, Some("掌机"), " 掌机 ").expect("读得动"),
        Ok(())
    );
    assert_eq!(
        target::vet_name(&catalog, Some("掌机"), "备份卡").expect("读得动"),
        Err(NameRefusal::Taken)
    );
    assert_eq!(
        target::vet_name(&catalog, None, "新掌机").expect("读得动"),
        Ok(())
    );
}

// ——— 按平台覆盖能力档案的结论（票 `gui-looks-like-the-design/21`）———

#[test]
fn 按平台覆盖存进去读得回来_只影响这个子库_整份替换_删了撤销原样回来() {
    use romcat_core::capability::Override;
    use std::collections::BTreeMap;

    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    建子库(&mut catalog, "掌机", None);
    建子库(&mut catalog, "备份卡", None);
    let 覆盖 = BTreeMap::from([
        ("PSV".to_string(), Override::Rezip),
        ("SFC".to_string(), Override::Keep),
    ]);
    catalog
        .set_capability_overrides("掌机", &覆盖)
        .expect("写得进");
    assert_eq!(catalog.capability_overrides("掌机").expect("读得回"), 覆盖);
    assert!(
        catalog
            .capability_overrides("备份卡")
            .expect("读得回")
            .is_empty(),
        "覆盖只影响这个子库"
    );

    // 再存一份是整份替换，不是往上叠：弹层里改掉的那几行不该还留着。
    let 换 = BTreeMap::from([("GBA".to_string(), Override::Unpack)]);
    catalog
        .set_capability_overrides("掌机", &换)
        .expect("写得进");
    assert_eq!(catalog.capability_overrides("掌机").expect("读得回"), 换);

    let removed = catalog
        .take_sublibrary("掌机")
        .expect("删得动")
        .expect("在");
    assert!(
        catalog
            .capability_overrides("掌机")
            .expect("读得回")
            .is_empty()
    );
    assert!(catalog.restore_sublibrary(&removed).expect("放得回"));
    assert_eq!(
        catalog.capability_overrides("掌机").expect("读得回"),
        换,
        "撤销删除之后覆盖原样回来"
    );
}

#[test]
fn 排差量预览照这个子库的按平台覆盖判_别的子库照名册() {
    use romcat_core::capability::Override;
    use std::collections::BTreeMap;

    let mut 场 = 一张卡::摆好(0);
    let 另一张 = romcat_core::testing::temp_dir("sublib-override-card");
    for (name, target) in [("掌机", 场.卡.path()), ("备份卡", 另一张.path())] {
        let mut sublibrary = Sublibrary::at(name, target, "Pegasus", None);
        sublibrary.capability = Some("retroarch-exfat".to_string());
        场.catalog.put_sublibrary(&sublibrary).expect("子库写得进");
        加规则(&mut 场.catalog, name, "平台=FC");
    }
    let 排 = |catalog: &Catalog, name: &str| {
        romcat_core::sync::prepare(
            catalog,
            场.工作区.path(),
            name,
            &romcat_core::sync::Request::default(),
            &romcat_core::task::Handle::new(),
        )
        .expect("排得出计划")
    };
    assert!(
        排(&场.catalog, "掌机").desired.unsupported.is_empty(),
        "RetroArch 的 FC 吃 zip，名册里的结论是原样搬"
    );

    场.catalog
        .set_capability_overrides(
            "掌机",
            &BTreeMap::from([("FC".to_string(), Override::Unpack)]),
        )
        .expect("写得进");
    // 覆盖成「取出为裸文件」：这几份 zip 是假的、穿不透，解不开——照实报出来，说明覆盖真的生效了。
    let 覆盖后 = 排(&场.catalog, "掌机").desired;
    assert_eq!(覆盖后.unsupported.len(), 2, "{:?}", 覆盖后.unsupported);
    assert!(
        覆盖后
            .unsupported
            .iter()
            .all(|row| row.platform.as_deref() == Some("FC"))
    );
    assert!(
        排(&场.catalog, "备份卡").desired.unsupported.is_empty(),
        "另一台没覆盖，照名册"
    );
}

// ——— 子库改名（票 `gui-looks-like-the-design/21`，拿主意的人 2026-09-15 定：照稿名字可改）———

#[test]
fn 子库改名_目标规则例外覆盖清单都跟过去_旧名不在_新名空白重名当场拒() {
    use romcat_core::capability::Override;
    use romcat_core::catalog::sublibrary::Renamed;
    use romcat_core::sublibrary::target::NameRefusal;
    use std::collections::BTreeMap;

    let mut catalog = 现场();
    建子库(&mut catalog, "掌机", Some(64_000_000_000));
    建子库(&mut catalog, "备用卡", None);
    加规则(&mut catalog, "掌机", "平台=GB");
    加规则(&mut catalog, "掌机", "平台=PSV");
    assert!(catalog.remove_rule("掌机", 1).expect("删得动"));
    catalog
        .set_exception("掌机", "库/PSV/大作.vpk", Exception::Include, Some("想玩"))
        .expect("例外写得进");
    let 覆盖 = BTreeMap::from([("GB".to_string(), Override::Keep)]);
    catalog
        .set_capability_overrides("掌机", &覆盖)
        .expect("覆盖写得进");
    let 清单 = 同步清单 {
        files: vec![ManifestFile {
            path: "GB/口袋妖怪 汉化.zip".to_string(),
            kind: FileKind::Rom,
            stamp: Stamp {
                bytes: 4096,
                mtime_ns: Some(1_700_000_000_000_000_000),
            },
            source: "库/GB/口袋妖怪 汉化.zip".to_string(),
            source_stamp: Stamp {
                bytes: 4096,
                mtime_ns: None,
            },
            variant: "库/GB/口袋妖怪 汉化.zip".to_string(),
            absent: true,
        }],
    };
    catalog.put_manifest("掌机", &清单).expect("清单写得进");
    let 读一遍 = |catalog: &Catalog, name: &str| {
        (
            catalog.sublibrary(name).expect("读得动").map(|mut row| {
                row.name = String::new();
                row
            }),
            catalog.sublibrary_rules(name).expect("读得动"),
            catalog.sublibrary_exceptions(name).expect("读得动"),
            catalog.capability_overrides(name).expect("读得动"),
            catalog.manifest(name).expect("读得动"),
        )
    };
    let 之前 = 读一遍(&catalog, "掌机");

    assert_eq!(
        catalog.rename_sublibrary("掌机", "备用卡").expect("读得动"),
        Renamed::Refused(NameRefusal::Taken),
        "重名的当场拒，不揉进备用卡"
    );
    assert_eq!(
        catalog.rename_sublibrary("掌机", "  ").expect("读得动"),
        Renamed::Refused(NameRefusal::Empty)
    );
    assert_eq!(
        catalog
            .rename_sublibrary("没这一台", "新名")
            .expect("读得动"),
        Renamed::Missing
    );
    assert_eq!(读一遍(&catalog, "掌机"), 之前, "拒掉的那几下一行都没动");

    assert_eq!(
        catalog
            .rename_sublibrary("掌机", " RG35XX ")
            .expect("写得动"),
        Renamed::Done
    );
    assert!(
        catalog.sublibrary("掌机").expect("读得动").is_none(),
        "旧名还在"
    );
    assert_eq!(
        读一遍(&catalog, "RG35XX"),
        之前,
        "目标、规则、例外、覆盖、清单没有原样跟过去"
    );
    assert_eq!(
        catalog.sublibrary_rules("备用卡").expect("读得动").len(),
        0,
        "改名不碰别的子库"
    );
    assert_eq!(
        加规则(&mut catalog, "RG35XX", "平台=GB"),
        3,
        "发号器没跟过去：下一条该接着发 3 号"
    );
}

#[test]
fn 子库改名之后差量预览与改名之前一样() {
    let mut 场 = 一张卡::摆好(0);
    场.建子库("平台=FC", None);
    let 之前 = 场.排计划().plan;
    assert_eq!(
        场.catalog
            .rename_sublibrary("掌机", "RG35XX")
            .expect("写得动"),
        romcat_core::catalog::sublibrary::Renamed::Done
    );
    let 之后 = romcat_core::sync::prepare(
        &场.catalog,
        场.工作区.path(),
        "RG35XX",
        &romcat_core::sync::Request::default(),
        &romcat_core::task::Handle::new(),
    )
    .expect("改名之后排得出计划")
    .plan;
    assert_eq!(之后.sublibrary, "RG35XX");
    assert_eq!(之后.steps, 之前.steps, "改名改动了要做的事");
    assert_eq!(之后.adds, 之前.adds);
    assert_eq!(之后.strangers, 之前.strangers);
}

// ——— 容量上限「按设备容量」那一档（票 `gui-looks-like-the-design/21`，拿主意的人 2026-09-15 照稿定）———

#[test]
fn 按设备容量那一档_在位时跟着设备总量_不在位时用上次读到的_没读过不设上限_自定义照记着的() {
    let mut 按设备 = Sublibrary::at(
        "掌机",
        std::path::Path::new("/Volumes/SDCARD"),
        "Pegasus",
        None,
    );
    按设备.capacity_by_device = true;
    assert_eq!(按设备.limit(None), None, "没读过设备总量：不设上限");
    assert_eq!(
        按设备.limit(Some(128_000_000_000)),
        Some(128_000_000_000),
        "在位时就是这张卡的总量"
    );
    按设备.capacity = Some(64_000_000_000);
    assert_eq!(
        按设备.limit(None),
        Some(64_000_000_000),
        "不在位时用上次连上时读到的"
    );
    assert_eq!(
        按设备.limit(Some(256_000_000_000)),
        Some(256_000_000_000),
        "换了一张卡，上限跟着变"
    );
    let 自定义 = Sublibrary::at(
        "备用卡",
        std::path::Path::new("/Volumes/SDCARD"),
        "Pegasus",
        Some(58_000_000_000),
    );
    assert!(!自定义.capacity_by_device, "新建默认是自定义那一档");
    assert_eq!(自定义.limit(Some(256_000_000_000)), Some(58_000_000_000));
}

#[test]
fn 按设备容量那一档存进去读得回来_删了撤销与改名都跟着() {
    let mut catalog = 现场();
    let mut 掌机 = Sublibrary::at(
        "掌机",
        std::path::Path::new("/Volumes/SDCARD"),
        "Pegasus",
        Some(64_000_000_000),
    );
    掌机.capacity_by_device = true;
    catalog.put_sublibrary(&掌机).expect("子库写得进");
    let 读 = |catalog: &Catalog, name: &str| catalog.sublibrary(name).expect("读得动").expect("在");
    assert!(读(&catalog, "掌机").capacity_by_device);
    assert_eq!(读(&catalog, "掌机").capacity, Some(64_000_000_000));

    let removed = catalog
        .take_sublibrary("掌机")
        .expect("删得动")
        .expect("在");
    assert!(catalog.restore_sublibrary(&removed).expect("放得回"));
    assert!(
        读(&catalog, "掌机").capacity_by_device,
        "撤销删除之后这一档没回来"
    );

    assert_eq!(
        catalog.rename_sublibrary("掌机", "RG35XX").expect("写得动"),
        romcat_core::catalog::sublibrary::Renamed::Done
    );
    assert!(
        读(&catalog, "RG35XX").capacity_by_device,
        "改名之后这一档没跟过去"
    );
    assert!(
        catalog
            .sublibraries()
            .expect("读得动")
            .iter()
            .all(|row| row.capacity_by_device),
        "列出来的那一份也得带着这一档"
    );
}

#[test]
fn 排差量预览时按设备容量那一档的上限就是这张卡此刻的总量() {
    use romcat_core::sublibrary::target::{self, Presence};

    let mut 场 = 一张卡::摆好(0);
    场.建子库("平台=FC", None);
    let mut 掌机 = 场.catalog.sublibrary("掌机").expect("读得动").expect("在");
    掌机.capacity_by_device = true;
    场.catalog.put_sublibrary(&掌机).expect("写得进");
    let 总量 = match target::vet(&场.catalog, 场.工作区.path(), Some("掌机"), 场.卡.path())
        .expect("读得动")
    {
        Ok(Presence::Present(volume)) => volume.total.expect("读得出卷的总量"),
        other => panic!("卡插着：{other:?}"),
    };
    assert_eq!(场.排计划().plan.capacity, Some(总量));
}

// ——— 本机磁盘不设容量上限时按剩余空间算（票 `gui-looks-like-the-design/21`，拿主意的人 2026-09-15 照稿定）———

#[test]
fn 本机磁盘不设容量上限时按剩余空间算_可移动存储设了上限与未连接的都不适用() {
    use romcat_core::sublibrary::target::Volume;

    let 本机 = Volume {
        filesystem: Some("APFS".to_string()),
        total: Some(500_000_000_000),
        available: Some(120_000_000_000),
        removable: false,
    };
    let 卡 = Volume {
        removable: true,
        ..本机.clone()
    };
    let 目标 = std::path::Path::new("/Users/我/roms");
    let 不设限 = Sublibrary::at("本机", 目标, "Pegasus", None);
    assert_eq!(
        不设限.limit_on(Some(&本机), 30_000_000_000),
        Some(150_000_000_000),
        "本机磁盘不设上限：目标上已经占着的加上还写得下的"
    );
    assert_eq!(
        不设限.limit_on(Some(&卡), 30_000_000_000),
        None,
        "可移动存储不适用"
    );
    assert_eq!(
        不设限.limit_on(None, 30_000_000_000),
        None,
        "未连接：没读过就不设上限"
    );
    let 设了 = Sublibrary::at("本机", 目标, "Pegasus", Some(58_000_000_000));
    assert_eq!(
        设了.limit_on(Some(&本机), 30_000_000_000),
        Some(58_000_000_000)
    );
    let mut 按设备 = 不设限.clone();
    按设备.capacity_by_device = true;
    assert_eq!(
        按设备.limit_on(Some(&本机), 30_000_000_000),
        Some(500_000_000_000),
        "按设备容量那一档照旧跟着总量"
    );
}

#[test]
fn 排差量预览时的容量上限照核心那一处判_容量账里带着它() {
    // 卷上还写得下多少是个活的数（别的进程一写就变），所以不拿两个时刻读的数去逐字节比：只钉住规矩。
    use romcat_core::sublibrary::target;

    let mut 场 = 一张卡::摆好(4096);
    场.建子库("平台=FC", None);
    let plan = 场.排计划().plan;
    let volume = target::volume(场.卡.path());
    match (volume.removable, volume.available, plan.capacity) {
        (true, _, capacity) => assert_eq!(capacity, None, "可移动存储不按剩余空间算"),
        (false, None, capacity) => assert_eq!(capacity, None, "可用空间读不出就不设上限"),
        (false, Some(_), Some(capacity)) => assert!(
            capacity >= plan.actual_bytes,
            "按剩余空间算的上限至少是目标现占：上限 {capacity}，现占 {}",
            plan.actual_bytes
        ),
        (false, Some(_), None) => panic!("本机磁盘读得出可用空间，计划里却没有上限"),
    }
    assert_eq!(
        sublibrary::Room::of(&plan).capacity,
        plan.capacity,
        "容量账里得带着那个上限——容量条照它画"
    );

    // 自定义设了数的那一档：计划里就是那个数，不去看卷。
    let mut 掌机 = 场.catalog.sublibrary("掌机").expect("读得动").expect("在");
    掌机.capacity = Some(58_000_000_000);
    场.catalog.put_sublibrary(&掌机).expect("写得进");
    assert_eq!(场.排计划().plan.capacity, Some(58_000_000_000));
}

#[test]
fn 库里头一个有平台的变体住在哪个平台目录_目标设置里那句说明拿它举例() {
    // 票 `gui-looks-like-the-design/21`：前端格式底下那句「每个平台一份，例如 GBA.metadata.pegasus.txt」拿库里真实的头一个平台
    // 目录举例，界面上不写死平台名。空库说不出例子。
    let catalog = 现场();
    assert_eq!(
        catalog
            .sample_platform_directory()
            .expect("读得动")
            .as_deref(),
        Some("GB")
    );
    let 空库 = Catalog::open_in_memory().expect("能开中立库");
    assert_eq!(空库.sample_platform_directory().expect("读得动"), None);
}
