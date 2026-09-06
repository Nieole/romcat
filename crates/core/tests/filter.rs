//! **筛选器就是规则语言**：可嵌套的组、三种连接、九个运算符，命令行与界面共用同一套。
//!
//! 这个文件钉的是**这一票的正题**——同一条规则有两个求值器，它们必须给出同一批：
//!
//! - **内存那一侧**（`sublibrary::select`）：子库同步走它，因为同步本来就要把整批
//!   选中的东西列出来。
//! - **SQL 那一侧**（`catalog::filter`）：浏览屏走它，因为四万多个变体上「改一个条件
//!   当场看见筛出多少」不允许每次把全库折成事实。
//!
//! 两处答案不一样时，用户是在按下同步之后才发现的——那时错的是数据，不是屏幕。
//! 所以下面每一条规则都各跑一遍，比的是**两批键完全相同**，而不是「条数差不多」。
//!
//! 另外两件事也在这儿钉：**旧的平铺规则原样解析**（已存的子库不许因为这一票失效），
//! 以及**当前筛选原样变成规则**（「存成子库」按下去之后选出来的与屏上一致）。

use std::collections::BTreeSet;

use romcat_core::catalog::browse::{MAX_PAGE, PlatformFilter, StateFilter, Unruly, VariantQuery, WorkQuery};
use romcat_core::catalog::scrape::{Harvested, HarvestedValue};
use romcat_core::catalog::{Candidate, Catalog, Confidence, Identification, Provenance, State};
use romcat_core::dat::Convention;
use romcat_core::dat::chinese::ChineseMark;
use romcat_core::platform::Manifest;
use romcat_core::scrape::{AnchorKind, Field};
use romcat_core::shape::{Role, SINGLE_FILE_RULE, Variant};
use romcat_core::sublibrary::{self, Rule, Selection};

fn 变体(key: &str, platform: Option<&str>, bytes: u64) -> Variant {
    Variant {
        key: key.to_string(),
        platform: platform.map(str::to_string),
        rule: SINGLE_FILE_RULE.to_string(),
        main_key: key.to_string(),
        manual: false,
        files: 1,
        bytes,
        unreadable_files: 0,
        members: vec![(key.to_string(), Role::Main)],
    }
}

/// 把一个变体挂到一个作品与一条发行版上，顺带给它一个**中文身份**记号。
///
/// 记号从**自动通过**的候选上读回来，与 `adapter::converge` 同一条路——两处答案不一样的话，
/// 界面上筛出来的那批与真正导出去的那批就对不上。
fn 认出来(
    catalog: &mut Catalog,
    key: &str,
    work_id: Option<i64>,
    release_id: Option<i64>,
    mark: Option<ChineseMark>,
) {
    catalog
        .write_identifications(&[Identification {
            variant_key: key.to_string(),
            state: State::Matched,
            reason: None,
            units: 1,
            nkit: 0,
            read_bytes: 0,
            work_id,
            release_id,
            candidates: vec![Candidate {
                member_key: key.to_string(),
                inner: String::new(),
                confidence: Confidence::High,
                accepted: true,
                source: "No-Intro".to_string(),
                dat: "测试.dat".to_string(),
                platform: "GB".to_string(),
                game: "测试条目".to_string(),
                rom: "测试.gb".to_string(),
                hashed_as: Convention::AsIs,
                dat_convention: Convention::AsIs,
                evidence: "合成 fixture 里钉死的依据".to_string(),
                chinese: mark,
                serial: None,
                release_id,
            }],
        }])
        .expect("识别结论写得进");
}

fn 刮一条(catalog: &mut Catalog, anchor: AnchorKind, subject: &str, source: &str, values: &[(Field, &str)]) {
    catalog
        .put_scraped(&[Harvested {
            anchor: anchor.label().to_string(),
            subject: subject.to_string(),
            source: source.to_string(),
            input: format!("{subject}/{source}"),
            values: values
                .iter()
                .map(|(field, value)| HarvestedValue {
                    field: field.label().to_string(),
                    value: (*value).to_string(),
                    evidence: "合成 fixture".to_string(),
                })
                .collect(),
            media: Vec::new(),
        }])
        .expect("刮削值写得进");
}

/// 一份形状照真库来的小库。**每一维上都留了会咬人的那一档**：
///
/// - 平台有**未知**的一档（`variant.platform` 是 `NULL`）；
/// - 有**认不出作品**的变体（真库上那是一多半）；
/// - 语言那一列是逗号串，里面有 `En` 也有 `Danish`——`en` 不许撞上后者；
/// - 同一个字段上有**两个源各给一个值**（年份 1996 与 1998），规则按「有一个落在范围里就算」；
/// - 年份里塞了一条 `199X`：读不成整数的值两边都得扔掉；
/// - 刮削值**作品锚点与变体锚点都有**，规则不该要求用户先弄清某个字段挂在哪一层。
fn 建库() -> Catalog {
    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    catalog
        .replace_variants(
            &[
                变体("卡一/GB/口袋妖怪 红.zip", Some("GB"), 1024 * 1024),
                变体("卡一/GB/口袋妖怪 红 汉化.zip", Some("GB"), 2 * 1024 * 1024),
                变体("卡一/GBA/火焰纹章.gba", Some("GBA"), 8 * 1024 * 1024),
                变体("卡二/SFC/圣剑传说2.sfc", Some("SFC"), 4 * 1024 * 1024),
                变体("卡二/未知/来路不明.bin", None, 64 * 1024),
                变体("卡二/未知/散落一个.bin", None, 128 * 1024),
            ],
            1,
            &Manifest::builtin(),
        )
        .expect("变体写得进");

    let 口袋 = catalog
        .add_work("口袋妖怪 红", Provenance::Identified)
        .expect("建得出作品");
    let 火纹 = catalog
        .add_work("火焰纹章", Provenance::Identified)
        .expect("建得出作品");
    let 圣剑 = catalog
        .add_work("圣剑传说2", Provenance::Identified)
        .expect("建得出作品");

    let 日版 = catalog
        .add_release(口袋, Some("GB"), Some("JP"), None, Some("Ja"), Provenance::Identified)
        .expect("建得出发行版");
    let 多语 = catalog
        .add_release(火纹, Some("GBA"), Some("EU"), None, Some("En, Danish"), Provenance::Identified)
        .expect("建得出发行版");
    let 无语言 = catalog
        .add_release(圣剑, Some("SFC"), Some("JP"), None, None, Provenance::Identified)
        .expect("建得出发行版");

    认出来(&mut catalog, "卡一/GB/口袋妖怪 红.zip", Some(口袋), Some(日版), None);
    认出来(
        &mut catalog,
        "卡一/GB/口袋妖怪 红 汉化.zip",
        Some(口袋),
        Some(日版),
        Some(ChineseMark::FanTranslated),
    );
    认出来(&mut catalog, "卡一/GBA/火焰纹章.gba", Some(火纹), Some(多语), None);
    认出来(
        &mut catalog,
        "卡二/SFC/圣剑传说2.sfc",
        Some(圣剑),
        Some(无语言),
        Some(ChineseMark::Official),
    );
    // 「来路不明」认出了作品但没有发行版；「散落一个」压根没识别过。
    认出来(&mut catalog, "卡二/未知/来路不明.bin", Some(圣剑), None, None);

    let 通关过的 = catalog.add_collection("通关过的").expect("建得出合集");
    for key in ["卡一/GB/口袋妖怪 红 汉化.zip", "卡二/SFC/圣剑传说2.sfc"] {
        catalog
            .add_to_collection(通关过的, key)
            .expect("合集成员写得进");
    }

    // 作品锚点上的刮削值。同一个字段上**两个源各给一个年份**。
    刮一条(
        &mut catalog,
        AnchorKind::Work,
        "口袋妖怪 红",
        "中文离线源",
        &[
            (Field::Year, "1996"),
            (Field::Genre, "RPG"),
            (Field::Developer, "Game Freak"),
            (Field::Publisher, "Nintendo"),
            // **中英混排**：大小写只折 ASCII，而两个求值器折的必须是同一套
            // ——这个库里中英混排的名字到处都是。
            (Field::Description, "一部关于收集怪物的勇者故事 RED VERSION"),
        ],
    );
    刮一条(&mut catalog, AnchorKind::Work, "口袋妖怪 红", "ScreenScraper", &[(Field::Year, "1998")]);
    刮一条(
        &mut catalog,
        AnchorKind::Work,
        "火焰纹章",
        "中文离线源",
        &[
            (Field::Year, "2003"),
            (Field::Genre, "SRPG"),
            (Field::Developer, "Intelligent Systems"),
            (Field::Publisher, "Nintendo"),
        ],
    );
    // 读不成整数的年份：两边都得当它没有。
    刮一条(
        &mut catalog,
        AnchorKind::Work,
        "圣剑传说2",
        "中文离线源",
        &[(Field::Year, "199X"), (Field::Genre, "Action-RPG")],
    );
    // **变体锚点**上的值：没认出作品的那些只有这一条路。
    刮一条(
        &mut catalog,
        AnchorKind::Variant,
        "卡二/未知/散落一个.bin",
        "中文离线源",
        &[(Field::Genre, "RPG"), (Field::Year, "1994")],
    );
    catalog
}

/// **SQL 那一侧**：这条规则在中立库里筛出哪几个变体的键。
fn 库里筛(catalog: &Catalog, rule: &Rule) -> BTreeSet<String> {
    let query = VariantQuery {
        rule: Some(rule.clone()),
        ..VariantQuery::default()
    };
    let total = catalog.variant_total(&query).expect("数得出总数");
    let rows = catalog
        .variant_page(&query, 0, MAX_PAGE)
        .expect("取得出一页");
    assert_eq!(
        u64::try_from(rows.len()).expect("行数装得下"),
        total,
        "总数与页内容用了不同的筛选，滚动条会指向不存在的行",
    );
    rows.into_iter().map(|row| row.key).collect()
}

/// **内存那一侧**：同一条规则走选择集求值，选出哪几个变体的键。
fn 选择集选(catalog: &Catalog, rule: &Rule) -> BTreeSet<String> {
    let facts = sublibrary::facts(catalog).expect("事实折得出来");
    let selection = Selection {
        rules: vec![rule.clone()],
        exceptions: Vec::new(),
    };
    sublibrary::select(&selection, &facts)
        .picked
        .into_iter()
        .map(|picked| picked.key)
        .collect()
}

/// 每一条规则各跑两遍，比两批键。
fn 两边一致(catalog: &Catalog, text: &str) -> BTreeSet<String> {
    let rule = Rule::parse(text).unwrap_or_else(|error| panic!("「{text}」读不懂：{error}"));
    let 库 = 库里筛(catalog, &rule);
    let 内存 = 选择集选(catalog, &rule);
    assert_eq!(
        库, 内存,
        "「{text}」在中立库里筛出来的与选择集选出来的不是同一批",
    );
    库
}

#[test]
fn 屏上筛出来的与子库选出来的是同一批() {
    let catalog = 建库();
    // 每一条都覆盖一样会让两个求值器分家的东西。
    for text in [
        // 平台：一列可空的文字，`!=` 要选得中平台未知的那些。
        "平台=GB",
        "平台=GB,GBA",
        "平台!=GB",
        // 多个值上的 `!=` 是「一个都不是」，而**取不到值时它成立**——
        // 平台未知的那些行必须选得中（SQL 那一侧少一层 `COALESCE` 就会整批漏掉）。
        "平台!=GB,GBA",
        "中文!=汉化,官中",
        "语言!=En,Ja",
        // 否上加否：`都不(A)` 里那个 A 自己就是个 `!=`。
        "都不(平台!=GB)",
        "都不(年份!=1996)",
        "平台~B",
        "平台^G",
        "平台$A",
        // 作品：认不出作品的那些在这一维上取不到值。
        "作品~口袋",
        "作品^口袋",
        "作品$红",
        "作品!=火焰纹章",
        // 语言：逐个码比，`en` 不许撞上 `Danish`。
        "语言=En",
        "语言=Ja",
        "语言~an",
        "语言^Da",
        "语言$sh",
        "语言!=En",
        // 中文身份：从自动通过的候选上读。
        "中文=汉化",
        "中文=汉化,官中",
        "中文!=汉化",
        // 合集。
        "合集=通关过的",
        "合集!=通关过的",
        "合集^通关",
        // 刮削来的那几维，作品锚点与变体锚点合起来看。
        "类型=RPG",
        "类型~RP",
        // 大小写只折 ASCII，两边同一套；中英混排的值也照折。
        "类型=action-rpg",
        "类型~ACTION",
        "类型^action",
        "类型$rpg",
        "简介~red",
        "简介$VERSION",
        "简介^一部",
        "开发商^Game",
        "开发商$Freak",
        "发行商=Nintendo",
        "简介~勇者",
        "简介!=没有这句话",
        // 年份：同一个作品两个源各一个值，读不成整数的那条两边都得扔掉。
        "年份>=1996",
        "年份<=1996",
        "年份=1998",
        "年份!=1996",
        "年份>1994",
        "年份<2000",
        // 体积。
        "体积<=2MiB",
        "体积>4MiB",
        "体积!=64KiB",
        // 收藏：眼下一条都没有，`=是` 一个都选不中、`=否` 全选中。
        "收藏=是",
        "收藏=否",
        "收藏!=是",
        // 三种连接与嵌套。
        "平台=GB 且 中文=汉化",
        "平台=GB 或 平台=GBA",
        "都不(平台=GB 或 平台=GBA)",
        "平台=GB,GBA 且 (中文=汉化 或 类型~RPG)",
        "平台=GB 且 (中文=汉化 或 (类型=RPG 且 年份>=1996))",
        "都不((平台=GB 且 中文=汉化))",
        "都不(年份>=1990)",
        // 顶层就是一个「任一满足」组。
        "作品^口袋 或 都不(平台=GB,GBA,SFC)",
    ] {
        两边一致(&catalog, text);
    }
}

/// 一份**只为年份的写法**立的小库：一个变体挂一种写法，值一律落在**变体锚点**上
/// （作品锚点那一路由 [`建库`] 盖着）。
///
/// 值给空的就是「这一维上一条值都没有」——真库里那是最常见的一档。同一个变体上给
/// 几条就是**几个源各给一个**，规则按「有一个算得数就算」。
fn 建年份库(条目: &[(&str, &[&str])]) -> Catalog {
    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    let variants: Vec<_> = 条目
        .iter()
        .map(|(key, _)| 变体(key, Some("GB"), 1024))
        .collect();
    catalog
        .replace_variants(&variants, 1, &Manifest::builtin())
        .expect("变体写得进");
    for (key, years) in 条目 {
        for (n, year) in years.iter().enumerate() {
            刮一条(
                &mut catalog,
                AnchorKind::Variant,
                key,
                &format!("源{}", n + 1),
                &[(Field::Year, year)],
            );
        }
    }
    catalog
}

#[test]
fn 年份的写法两条路认的是同一份() {
    // 甲＝`1996`、乙＝`01996`、丙＝`+1996`、丁＝`1996` 外加一条读不成年份的 `一九九六`、
    // 戊＝一条年份都没有。
    //
    // **`01996` 与 `+1996` 不是这个库里年份的写法**：会产年份的源只产四位数字
    // （`identify::naming::tosec_year` 与 `scrape::online` 都写着「认不出四位数字就不
    // 产出」，中文离线源根本不产年份），能写出别的形态的只有人手打的裁决值。
    //
    // 从前 SQL 那一侧 `CAST` 往返回不到原文就扔、内存那一侧 `parse::<i32>()` 却收下
    // 前导 `0` 与前导 `+`——同一条规则屏上筛出两个、按下同步搬走四个。
    let catalog = 建年份库(&[
        ("年/甲.bin", &["1996"]),
        ("年/乙.bin", &["01996"]),
        ("年/丙.bin", &["+1996"]),
        ("年/丁.bin", &["1996", "一九九六"]),
        ("年/戊.bin", &[]),
    ]);
    let 甲丁 = BTreeSet::from(["年/甲.bin".to_string(), "年/丁.bin".to_string()]);
    assert_eq!(两边一致(&catalog, "年份=1996"), 甲丁);
    assert_eq!(两边一致(&catalog, "年份>=1990"), 甲丁);
    assert_eq!(两边一致(&catalog, "年份<2000"), 甲丁);
    // **读不成年份的值就是「这一维上没有值」。** 乙丙有值但读不成、戊一条值都没有，
    // 两种形状同一个答案：`!=` 成立，别的运算符一律不成立。
    assert_eq!(
        两边一致(&catalog, "年份!=1996"),
        BTreeSet::from([
            "年/乙.bin".to_string(),
            "年/丙.bin".to_string(),
            "年/戊.bin".to_string(),
        ]),
        "读不成年份的值该算成「取不到值」，`!=` 对它成立",
    );
    // 丁身上那条读不成的 `一九九六` 不许把同一个变体上读得成的 `1996` 挤掉。
    assert!(两边一致(&catalog, "年份=1996").contains("年/丁.bin"));
    // 否上加否落在同一条约定之上，不是绕过它。
    assert_eq!(两边一致(&catalog, "都不(年份!=1996)"), 甲丁);
}

#[test]
fn 年份只认四位数字() {
    // 「什么算一个年份」只有一份定义（`sublibrary::rule::parse_year`），SQL 那一侧
    // 逐字照它写。三位、五位、半截日期、汉字数字**一概不是年份**——把「不知道」
    // 伪装成「知道」比缺着更难查（`tosec_year` 对 `199x` 的处置是同一条）。
    let catalog = 建年份库(&[
        ("年/空格.bin", &[" 1996 "]),
        ("年/带X.bin", &["199X"]),
        ("年/三位.bin", &["196"]),
        ("年/五位.bin", &["19960"]),
        ("年/带月.bin", &["1996-12"]),
        ("年/汉字.bin", &["一九九六"]),
    ]);
    // **前后的空格去掉再看**，两边都只去空格（SQLite 的 `trim()` 就只去空格）。
    let 空格 = BTreeSet::from(["年/空格.bin".to_string()]);
    assert_eq!(两边一致(&catalog, "年份=1996"), 空格);
    // 从前 `19960` 两边都当年份收下（`CAST` 往返回得到原文，`parse` 也读得出）。
    assert_eq!(两边一致(&catalog, "年份>=1000"), 空格);
    // 从前 `196` 同理，于是「年份<1000」真能筛出一个「196 年」的游戏来。
    assert!(
        两边一致(&catalog, "年份<1000").is_empty(),
        "三位数不是年份，不该有东西落在 1000 年以前",
    );
    // 剩下那五个在这一维上都取不到值。
    assert_eq!(
        两边一致(&catalog, "年份!=1996"),
        BTreeSet::from([
            "年/三位.bin".to_string(),
            "年/五位.bin".to_string(),
            "年/带X.bin".to_string(),
            "年/带月.bin".to_string(),
            "年/汉字.bin".to_string(),
        ]),
    );
}

#[test]
fn 三种连接各自成立而且组嵌得动() {
    let catalog = 建库();
    let 全部 = 两边一致(&catalog, "平台=GB 且 中文=汉化");
    assert_eq!(
        全部,
        BTreeSet::from(["卡一/GB/口袋妖怪 红 汉化.zip".to_string()]),
        "「全部满足」把两条都得成立算成了别的",
    );

    let 任一 = 两边一致(&catalog, "平台=SFC 或 中文=汉化");
    assert_eq!(
        任一,
        BTreeSet::from([
            "卡一/GB/口袋妖怪 红 汉化.zip".to_string(),
            "卡二/SFC/圣剑传说2.sfc".to_string(),
        ]),
    );

    // 「都不满足」不是「不全满足」：GB 与 GBA 一个都不许沾。
    let 都不 = 两边一致(&catalog, "都不(平台=GB 或 平台=GBA)");
    assert!(!都不.iter().any(|key| key.contains("/GB")));
    assert!(都不.contains("卡二/SFC/圣剑传说2.sfc"));
    // 平台未知的那些在这一维上取不到值——「都不满足」对它们成立。
    assert!(都不.contains("卡二/未知/来路不明.bin"));

    // 嵌两层：里面那个组自己就是「任一满足」。
    let 嵌套 = 两边一致(&catalog, "平台=GB,GBA 且 (中文=汉化 或 类型~SRPG)");
    assert_eq!(
        嵌套,
        BTreeSet::from([
            "卡一/GB/口袋妖怪 红 汉化.zip".to_string(),
            "卡一/GBA/火焰纹章.gba".to_string(),
        ]),
    );

    // 嵌三层。
    let 三层 = 两边一致(&catalog, "平台=GB 且 (中文=汉化 或 (类型=RPG 且 年份>=1998))");
    assert_eq!(
        三层,
        BTreeSet::from([
            "卡一/GB/口袋妖怪 红.zip".to_string(),
            "卡一/GB/口袋妖怪 红 汉化.zip".to_string(),
        ]),
    );
}

#[test]
fn 以开始与以结束各自成立() {
    let catalog = 建库();
    let 开头 = 两边一致(&catalog, "开发商^Game");
    assert_eq!(
        开头,
        BTreeSet::from([
            "卡一/GB/口袋妖怪 红.zip".to_string(),
            "卡一/GB/口袋妖怪 红 汉化.zip".to_string(),
        ]),
        "`^` 该只认开头，`Intelligent Systems` 里也有 `Game` 之外的字",
    );
    // 「含有」比「以…开始」宽：`Systems` 含 `Game` 不成立，但含 `em` 成立。
    assert!(两边一致(&catalog, "开发商~Systems").contains("卡一/GBA/火焰纹章.gba"));
    assert!(两边一致(&catalog, "开发商^Systems").is_empty());

    let 结尾 = 两边一致(&catalog, "开发商$Freak");
    assert_eq!(结尾.len(), 2);
    assert!(两边一致(&catalog, "开发商$Game").is_empty());

    // 大小写不敏感，两边同一条口径（ASCII 折大小写，汉字本来就没有大小写）。
    assert_eq!(两边一致(&catalog, "开发商^game"), 开头);

    // **中英混排的值也照折。** 两个求值器有一处只在「整串是不是纯 ASCII」上分家过：
    // 一边非纯 ASCII 就退成逐字节比、另一边 `lower()` 照折 ASCII——而这个库里
    // 中英混排的名字到处都是。下面这几条空集合==空集合就验不出东西，所以各断言非空。
    for text in ["简介~red", "简介$VERSION", "类型~ACTION", "类型^action", "类型$rpg"] {
        assert!(
            !两边一致(&catalog, text).is_empty(),
            "「{text}」两边都是空的，这条断言等于没测",
        );
    }
    // 圣剑那个作品底下挂着两个变体，作品锚点上的值对它们都算数。
    assert_eq!(两边一致(&catalog, "类型=action-rpg").len(), 2);
}

#[test]
fn 旧的平铺规则原样解析() {
    let catalog = 建库();
    // 这一票之前存进中立库的规则长这个样子：没有括号、没有 `或`、子句之间一律 `且`。
    for text in [
        "平台=GB",
        "平台=GB,GBA 且 中文=汉化",
        "作品~口袋 且 年份>=1996 且 体积<=4MiB",
        "平台!=PSV 且 合集=通关过的",
    ] {
        let rule = Rule::parse(text).expect("老规则照样读得懂");
        // 平铺的老规则**就是**一个顶层的「全部满足」组，一层都不多。
        assert_eq!(rule.root.join, romcat_core::sublibrary::Join::All);
        assert_eq!(rule.root.depth(), 1, "「{text}」凭空多出了一层组");
        // 原文一个字都没变——报告里印的是用户写的那句话。
        assert_eq!(rule.text, text);
        两边一致(&catalog, text);
    }

    // 已存的子库照常可用：写进去、读回来、求值，一步都没坏。
    let mut catalog = catalog;
    catalog
        .put_sublibrary(&romcat_core::sublibrary::Sublibrary {
            name: "掌机".to_string(),
            target: "/Volumes/SDCARD/掌机".to_string(),
            target_raw: Some("/Volumes/SDCARD/掌机".to_string()),
            format: "Pegasus".to_string(),
            capacity: None,
            capability: None,
        })
        .expect("子库写得进");
    let 老规则 = Rule::parse("平台=GB,GBA 且 中文=汉化").expect("读得懂");
    catalog.add_rule("掌机", &老规则).expect("规则写得进");
    let loaded = catalog.selection("掌机").expect("选择集读得回来");
    assert!(loaded.broken.is_empty(), "老规则被这一票读坏了：{:?}", loaded.broken);
    assert_eq!(loaded.selection.rules[0].text, "平台=GB,GBA 且 中文=汉化");
}

#[test]
fn 当前筛选原样变成规则() {
    let catalog = 建库();
    // 左栏点了平台与中文，筛选器里又搭了一个「任一满足」组。
    let query = WorkQuery {
        platform: Some(PlatformFilter::Named("GB".to_string())),
        chinese: Some("汉化".to_string()),
        rule: Some(Rule::parse("类型~RPG 或 年份>=1996").expect("读得懂")),
        ..WorkQuery::default()
    };
    let rule = query
        .to_rule()
        .expect("这几条都写得成规则")
        .expect("有条件在筛，不该是空的");
    // 印出来的那行字就是屏上那几条，一条不多一条不少。
    assert_eq!(rule.text, "平台=GB 且 中文=汉化 且 (类型~RPG 或 年份>=1996)");

    // **存成子库之后选出来的，与屏上筛出来的是同一批。**
    let 屏上: BTreeSet<String> = catalog
        .scoped_variants(
            &query,
            romcat_core::catalog::browse::Scope::AllExcept(&[]),
        )
        .expect("展得开当前筛选")
        .into_iter()
        .collect();
    assert_eq!(屏上, 选择集选(&catalog, &rule));
    assert!(!屏上.is_empty(), "这份 fixture 上这条筛选该选得中东西");

    // 手搭的那棵树若顶层本来就是「全部满足」，摊进来而不是再套一层括号。
    let query = WorkQuery {
        platform: Some(PlatformFilter::Named("GB".to_string())),
        rule: Some(Rule::parse("中文=汉化 且 年份>=1996").expect("读得懂")),
        ..WorkQuery::default()
    };
    assert_eq!(
        query.to_rule().expect("写得成").expect("不是空的").text,
        "平台=GB 且 中文=汉化 且 年份>=1996",
    );

    // 一个条件都没有时是「整个库」，不是「一条都选不中」。
    assert_eq!(WorkQuery::default().to_rule(), Ok(None));
}

#[test]
fn 写不成规则的条件当场挡住而不是悄悄少写一条() {
    // 少写一条，子库选出来的就比屏上多——那正是这条约定要防的事。
    let 搜索 = WorkQuery {
        contains: "口袋".to_string(),
        ..WorkQuery::default()
    };
    assert_eq!(搜索.to_rule(), Err(Unruly::Search));

    let 状态 = WorkQuery {
        state: Some(StateFilter::Unidentified),
        ..WorkQuery::default()
    };
    assert_eq!(状态.to_rule(), Err(Unruly::State));

    let 未知平台 = WorkQuery {
        platform: Some(PlatformFilter::Unknown),
        ..WorkQuery::default()
    };
    assert_eq!(未知平台.to_rule(), Err(Unruly::UnknownPlatform));

    // 值里带逗号：逗号是规则里的值分隔符，写进去会被读成两个值。
    let 带逗号 = WorkQuery {
        collection: Some("送朋友的, 备份".to_string()),
        ..WorkQuery::default()
    };
    assert!(matches!(带逗号.to_rule(), Err(Unruly::Comma(_))));

    // **合集名是用户自己起的**，里面完全可能有规则语言的记号。折成规则那一步要挡住，
    // 而不是折出一条读回来变了样的规则——那种走样是静默的。
    // 结在**末尾**的连接词一样要挡：印出去后面一接兄弟子句，` 且 ` 那个分隔符把它缺的
    // 空白补上，读回来合集名少一截——**子库选出来的比屏上多，一声不响**。
    for name in [
        "送朋友的 或 备份",
        "口袋(日版",
        "通关过的 且 双人",
        "通关过的 且",
        "备份 或",
        "双人　且",
    ] {
        let query = WorkQuery {
            collection: Some(name.to_string()),
            ..WorkQuery::default()
        };
        let 折出来的 = query.to_rule();
        assert!(
            matches!(折出来的, Err(Unruly::Unwritable(_))),
            "合集「{name}」居然折成了一条规则：{折出来的:?}",
        );
    }
    // 括号配对的照收——那种名字读回来还是它自己。
    let 带括号 = WorkQuery {
        collection: Some("口袋(日版)".to_string()),
        ..WorkQuery::default()
    };
    let rule = 带括号.to_rule().expect("这个折得出来").expect("不是空的");
    assert_eq!(
        Rule::parse(&rule.text).expect("印回去还读得懂").root,
        rule.root,
    );

    // 每一条都说得出是哪一条、以及怎么办。
    for unruly in [
        Unruly::Search,
        Unruly::State,
        Unruly::UnknownPlatform,
        Unruly::Comma(romcat_core::sublibrary::Dimension::Collection),
        Unruly::Unwritable(romcat_core::sublibrary::Dimension::Collection),
    ] {
        assert!(!unruly.advice().is_empty());
    }
}

#[test]
fn 评分那一维读不成数与超出范围的值两边都得扔掉() {
    // **眼下一条评分也没有**（挂账 D68），所以这一条是拿手塞的值验的：
    // 少了那两道闸，`CAST('优秀' AS REAL)` 会读成 0.0（在 0–1 里！），
    // 于是 `评分<=0.5` 在 SQL 那侧选中一批，内存那侧一个都不选。
    let mut catalog = 建库();
    for (subject, value) in [
        ("口袋妖怪 红", "优秀"),
        ("火焰纹章", "1.5"),
        ("圣剑传说2", "0.8"),
    ] {
        catalog
            .put_scraped(&[Harvested {
                anchor: AnchorKind::Work.label().to_string(),
                subject: subject.to_string(),
                source: "手塞的".to_string(),
                input: format!("评分/{subject}"),
                values: vec![HarvestedValue {
                    field: "评分".to_string(),
                    value: value.to_string(),
                    evidence: "合成 fixture".to_string(),
                }],
                media: Vec::new(),
            }])
            .expect("刮削值写得进");
    }
    for text in ["评分<=0.5", "评分>=0.5", "评分=0.8", "评分!=0.8", "评分>0"] {
        两边一致(&catalog, text);
    }
    // 只有 `0.8` 那条读得成——两个作品各一个变体挂在圣剑底下。
    assert_eq!(两边一致(&catalog, "评分>=0.5").len(), 2);
    assert!(两边一致(&catalog, "评分<=0.5").is_empty());
}

#[test]
fn 多条规则并成一条就是任一满足() {
    let catalog = 建库();
    // 子库屏点「改选择」跳回浏览屏时预填的正是这一条（票 11）。
    let rules = vec![
        Rule::parse("平台=GB 且 中文=汉化").expect("读得懂"),
        Rule::parse("平台=SFC").expect("读得懂"),
    ];
    let 并 = Rule::any_of(rules.clone()).expect("两条并得起来");
    assert_eq!(并.root.join, romcat_core::sublibrary::Join::Any);

    // 并出来的那条选中的，正好是原来两条各自选中的**并集**——那是选择集里
    // 「多条规则之间是并集」这条口径。
    let selection = Selection {
        rules,
        exceptions: Vec::new(),
    };
    let facts = sublibrary::facts(&catalog).expect("事实折得出来");
    let 分开: BTreeSet<String> = sublibrary::select(&selection, &facts)
        .picked
        .into_iter()
        .map(|picked| picked.key)
        .collect();
    assert_eq!(分开, 两边一致(&catalog, &并.text));
    assert_eq!(Rule::any_of(Vec::new()), None);
}

#[test]
fn 筛选结果的条数与真正命中的条数一致() {
    let catalog = 建库();
    // 屏上写着几行、几个变体，按下批量操作就该动几个——三处同一个数。
    let query = WorkQuery {
        rule: Some(Rule::parse("平台=GB,GBA 或 中文=官中").expect("读得懂")),
        ..WorkQuery::default()
    };
    let 行数 = catalog.work_total(&query).expect("数得出行数");
    let 变体数 = catalog
        .scoped_variant_total(&query, romcat_core::catalog::browse::Scope::AllExcept(&[]))
        .expect("数得出变体数");
    let 键 = catalog
        .scoped_variants(&query, romcat_core::catalog::browse::Scope::AllExcept(&[]))
        .expect("展得开");
    assert_eq!(u64::try_from(键.len()).expect("装得下"), 变体数);
    assert_eq!(
        键.iter().cloned().collect::<BTreeSet<_>>(),
        选择集选(&catalog, &query.to_rule().expect("写得成").expect("不是空的")),
    );

    // 口径照票 03 那条：**作品数 ＋ 还没认出作品的变体数**。
    // GB 两个变体同属「口袋妖怪 红」一个作品，GBA 一个，官中那个又一个作品。
    assert_eq!(行数, 3);
    assert_eq!(变体数, 4);
}
