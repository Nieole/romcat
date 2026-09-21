//! **合并作品**与**移出此作品**（票 `gui-looks-like-the-design/16`）。
//!
//! 这一层要钉住的是四件事，每一件不这么做界面就会骗人：
//!
//! 1. **落成的是一批裁决，每个归入的变体一条**——与待确认队列里「手工指定作品」是同一种，
//!    因此撤销走的是既有那条按批撤销的路（`CONTEXT.md` 的**合并作品**条）。
//! 2. **发行版信息一个字不动**：只把作品换掉，地区、序列号、语言、中文身份原样抄回去。
//!    照现成的 `DecisionSpec::Manual` 落一遍会把它们整片抹掉，那正是这张票的要害。
//! 3. **按下去之前算得清会发生什么**：写几条裁决、前端条目从多少变多少、哪几个作品会空掉、
//!    哪几台子库要重排差量预览。
//! 4. **撤销把变体还回原来的作品**，而且没参与这一趟的作品一个字都不动。
//!
//! 断言挂在**规矩**上：问的是「这个变体眼下挂在哪个作品名下」「那几格事实还在不在」，
//! 不是「计划里第几行是谁」。

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use romcat_core::catalog::identify::{Identification, Provenance};
use romcat_core::catalog::{Catalog, NewRelease, State};
use romcat_core::dat::chinese::ChineseMark;
use romcat_core::fs::RealFs;
use romcat_core::platform::Manifest;
use romcat_core::scan::{self, Jobs, ScanOptions};
use romcat_core::scrape::priority::VERDICT;
use romcat_core::scrape::{AnchorKind, Field, Priorities};
use romcat_core::sublibrary::{Rule, Sublibrary};
use romcat_core::task::Handle;
use romcat_core::testing::container::{ZipEntrySpec, zip_container};
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::triage::merge::{self, Kind};
use romcat_core::verdict::{self, Anchor, Decision, Facts, Store, Verdict};

const 主库标识: &str = "小库";

/// 保留的那个作品：底下两个变体。
const 甲: &str = "口袋妖怪 红";
/// 被合并的那个作品：底下一个变体，合并之后它一个变体都不剩。
const 乙: &str = "精灵宝可梦 红";
/// 完全不参与这一趟的作品——**它一个字都不许被动到**。
const 丙: &str = "幻想传说";

fn 键(相对: &str) -> String {
    format!("库/{相对}")
}

const 甲一: &str = "GB/口袋妖怪 红(汉化).zip";
const 甲二: &str = "GB/口袋妖怪 红(汉化 修正).zip";
const 乙一: &str = "GB/精灵宝可梦 红.zip";
const 丙一: &str = "SFC/幻想传说 汉化版.zip";
/// **还没认出作品**的那一份：合并的被合并一侧允许它（拿主意的人 2026-09-21 定）。
const 散一: &str = "GB/认不出这是什么.zip";

struct 现场 {
    _dir: TempDir,
    catalog: Catalog,
    store: Store,
}

/// 一份**穿得透**的 zip：内部那一条的 CRC-32 明写在容器头里，于是裁决钉得上**内容锚**
/// （`zip` 那份样例只有四个字节的头，穿不透——锚会悄悄退成路径锚）。
fn 一份内容(fill: u8) -> Vec<u8> {
    zip_container(&[ZipEntrySpec::stored(
        "rom.bin",
        std::iter::repeat_n(fill, 4_096).collect::<Vec<u8>>(),
    )])
}

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

/// 扫一份真的小主库（**内容判据因此是真的**：zip 头里记着每一条的 CRC-32，
/// 裁决钉的是内容锚而不是路径锚），再手写识别结论摆出三个作品。
fn 建现场() -> 现场 {
    let dir = temp_dir("merge");
    for (相对, 填) in [
        (甲一, 0xA1),
        (甲二, 0xA2),
        (乙一, 0xB1),
        (丙一, 0xC1),
        (散一, 0xD1),
    ] {
        写(&dir.path().join(相对), &一份内容(填));
    }
    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    let mut options = ScanOptions::named(dir.path(), "库");
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");

    let mut records = Vec::new();
    for (作品, 平台, 地区, 序列号, 语言, 变体) in [
        (
            甲,
            "GB",
            "Japan",
            "DMG-APAJ",
            "Ja,Zh-Hans",
            vec![甲一, 甲二],
        ),
        (乙, "GB", "USA", "DMG-APAE", "En", vec![乙一]),
        (丙, "SFC", "Japan", "SHVC-TZ", "Ja", vec![丙一]),
    ] {
        let work = catalog
            .add_work(作品, Provenance::Identified)
            .expect("建得出作品");
        let release = catalog
            .add_release(
                work,
                &NewRelease {
                    platform: Some(平台),
                    region: Some(地区),
                    serial: Some(序列号),
                    languages: Some(语言),
                    revision: None,
                },
                Provenance::Identified,
            )
            .expect("建得出发行版");
        for 相对 in 变体 {
            records.push(Identification {
                variant_key: 键(相对),
                state: State::Matched,
                reason: None,
                platform: Some(平台.to_string()),
                standalone: None,
                edition: None,
                units: 1,
                nkit: 0,
                read_bytes: 0,
                work_id: Some(work),
                release_id: Some(release),
                candidates: Vec::new(),
            });
        }
    }
    catalog
        .write_identifications(&records)
        .expect("写得进识别结论");

    // 两个作品各有一句简介：甲空着年份、乙有年份——**冲突那一步只该列出有冲突的**。
    for (作品, 字段, 值) in [
        (甲, Field::Description, "口袋妖怪 红的简介"),
        (乙, Field::Description, "精灵宝可梦 红的简介"),
        (乙, Field::Year, "1996"),
        (甲, Field::Developer, "Game Freak"),
        (乙, Field::Developer, "Game Freak"),
    ] {
        catalog
            .put_verdict_value(AnchorKind::Work, 作品, 字段, 值, "fixture")
            .expect("写得进刮削值");
    }

    现场 {
        _dir: dir,
        catalog,
        store: Store::in_memory().expect("开得出沉淀库"),
    }
}

/// 这个变体眼下挂在哪个作品名下；没挂上是 `None`。
fn 挂在(catalog: &Catalog, 相对: &str) -> Option<String> {
    let row = catalog
        .variant(&键(相对))
        .expect("读得动")
        .expect("有这一行");
    row.work_id
        .map(|id| catalog.work_name(id).expect("读得动").expect("作品行还在"))
}

/// 这个变体眼下基于的发行版那几格事实。
fn 发行版(
    catalog: &Catalog,
    相对: &str,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let row = catalog
        .variant(&键(相对))
        .expect("读得动")
        .expect("有这一行");
    let release = row
        .release_id
        .and_then(|id| catalog.release(id).expect("读得动"));
    match release {
        Some(release) => (
            release.platform,
            release.region,
            release.serial,
            release.languages,
        ),
        None => (None, None, None, None),
    }
}

fn 排一趟(现场: &现场, kind: Kind, into: &str, 变体: &[&str]) -> merge::Regrouping {
    let keys: Vec<String> = 变体.iter().map(|相对| 键(相对)).collect();
    merge::plan(&现场.catalog, &现场.store, 主库标识, kind, into, &keys).expect("排得出计划")
}

fn 落一趟(现场: &mut 现场, 计划: &merge::Regrouping) -> i64 {
    merge::apply(&mut 现场.catalog, &mut 现场.store, 计划)
        .expect("落得下")
        .batch
}

#[test]
fn 合并落成一批裁决_每个归入的变体一条_而且发行版信息一个字不动() {
    let mut 现场 = 建现场();
    let (平台, 地区, 序列号, 语言) = 发行版(&现场.catalog, 乙一);
    assert_eq!(
        (
            平台.as_deref(),
            地区.as_deref(),
            序列号.as_deref(),
            语言.as_deref()
        ),
        (Some("GB"), Some("USA"), Some("DMG-APAE"), Some("En")),
        "fixture 自己就没摆对",
    );

    let 计划 = 排一趟(&现场, Kind::Merge, 甲, &[乙一]);
    // **每个归入的变体一条**，不多不少。
    assert_eq!(计划.verdicts(), 1, "写几条裁决说的是归入的变体数");
    assert!(
        计划.blocked().is_empty(),
        "不该有落不下去的：{:?}",
        计划.blocked()
    );
    assert_eq!(计划.from(), [乙.to_string()], "从哪个作品来的没说对");

    落一趟(&mut 现场, &计划);

    assert_eq!(
        挂在(&现场.catalog, 乙一).as_deref(),
        Some(甲),
        "变体没改挂到保留的作品名下"
    );
    // **一个字不动**：地区、序列号、语言原样，只有作品换了。照 `DecisionSpec::Manual`
    // 落一遍的话这四格会全空——那正是票面第一段与 ⚠️ 第一条互相顶上的地方。
    assert_eq!(
        发行版(&现场.catalog, 乙一),
        (平台, 地区, 序列号, 语言),
        "合并把发行版信息动了",
    );
    // 沉淀库里那一条也说得出同一份事实。
    let 落下的 = 现场
        .store
        .all()
        .expect("读得出沉淀库")
        .into_iter()
        .find(|one| matches!(&one.decision, Decision::Release(facts) if facts.work == 甲))
        .expect("沉淀库里该有这一条");
    let Decision::Release(facts) = &落下的.decision else {
        panic!("落下的不是一条发行版裁决");
    };
    assert_eq!(
        facts.serial.as_deref(),
        Some("DMG-APAE"),
        "裁决里的序列号丢了"
    );
    assert_eq!(
        facts.languages.as_deref(),
        Some("En"),
        "裁决里的语言标记组丢了"
    );
    assert!(
        落下的.anchor.is_shareable(),
        "zip 里的内容判据取得到，锚该是**内容锚**：{:?}",
        落下的.anchor,
    );
}

#[test]
fn 撤掉那一批_变体回到原来的作品_而且没参与的作品一个字没动() {
    let mut 现场 = 建现场();
    let 计划 = 排一趟(&现场, Kind::Merge, 甲, &[乙一]);
    let batch = 落一趟(&mut 现场, &计划);
    assert_eq!(挂在(&现场.catalog, 乙一).as_deref(), Some(甲));

    let 账 =
        romcat_core::triage::undo_batch(&mut 现场.catalog, &mut 现场.store, batch).expect("撤得掉");
    assert_eq!(账.removed, 1, "该从沉淀库里删掉一条");
    assert!(账.catalog_rolled_back, "中立库那一半也该回去");
    assert_eq!(
        挂在(&现场.catalog, 乙一).as_deref(),
        Some(乙),
        "撤销没把变体还回原来的作品",
    );
    assert_eq!(
        发行版(&现场.catalog, 乙一).2.as_deref(),
        Some("DMG-APAE"),
        "撤销之后发行版那一行也该是原来那条",
    );
    // 没参与的那个作品：撤销前后都在原地。
    assert_eq!(挂在(&现场.catalog, 丙一).as_deref(), Some(丙));
}

#[test]
fn 取消勾选的变体不归入_它还留在原来的作品里() {
    let mut 现场 = 建现场();
    // 把甲底下的两个变体合到丙名下，但只勾了第一个——第二个是「取消勾选」的那种。
    let 计划 = 排一趟(&现场, Kind::Merge, 丙, &[甲一]);
    assert_eq!(计划.verdicts(), 1);
    落一趟(&mut 现场, &计划);

    assert_eq!(
        挂在(&现场.catalog, 甲一).as_deref(),
        Some(丙),
        "勾了的那个该归入"
    );
    assert_eq!(
        挂在(&现场.catalog, 甲二).as_deref(),
        Some(甲),
        "取消勾选的那个仍该留在原来的作品里",
    );
}

#[test]
fn 已经挂在保留作品名下的变体不再裁一遍() {
    let 现场 = 建现场();
    // 甲一本来就在甲名下：它不成其为「归入」，再裁一遍只是多一条一模一样的裁决。
    let 计划 = 排一趟(&现场, Kind::Merge, 甲, &[甲一, 乙一]);
    assert_eq!(计划.verdicts(), 1, "只该为真的换了作品的那一个写裁决");
    assert_eq!(计划.moving(), [键(乙一).as_str()]);
}

#[test]
fn 还没认出作品的变体也归得进去_走的是同一条裁决() {
    let mut 现场 = 建现场();
    assert_eq!(
        挂在(&现场.catalog, 散一),
        None,
        "fixture 里它本来就没挂上作品"
    );

    let 计划 = 排一趟(&现场, Kind::Merge, 甲, &[散一]);
    assert!(
        计划.from().is_empty(),
        "它没有原来的作品名，`from` 该是空的：{:?}",
        计划.from(),
    );
    assert!(
        计划.plan().summary.contains("还没认出作品的变体"),
        "这一批的摘要得替它说清从哪儿来：{}",
        计划.plan().summary,
    );
    落一趟(&mut 现场, &计划);
    assert_eq!(挂在(&现场.catalog, 散一).as_deref(), Some(甲));
}

#[test]
fn 移出此作品_移到新建的作品也成_而且撤得回来() {
    let mut 现场 = 建现场();
    let 新作品 = "口袋妖怪 红（漫游汉化）";
    let 计划 = 排一趟(&现场, Kind::Split, 新作品, &[甲二]);
    assert_eq!(计划.verdicts(), 1, "移出此作品是一条裁决");
    assert!(
        计划.plan().summary.starts_with("移出此作品 · "),
        "裁决记录里得认得出按的是哪一颗：{}",
        计划.plan().summary,
    );
    let batch = 落一趟(&mut 现场, &计划);

    assert_eq!(
        挂在(&现场.catalog, 甲二).as_deref(),
        Some(新作品),
        "没移到新建的作品名下"
    );
    assert_eq!(
        挂在(&现场.catalog, 甲一).as_deref(),
        Some(甲),
        "原来的作品还剩一个变体"
    );
    // 发行版信息照旧一个字不动。
    assert_eq!(发行版(&现场.catalog, 甲二).2.as_deref(), Some("DMG-APAJ"));

    romcat_core::triage::undo_batch(&mut 现场.catalog, &mut 现场.store, batch).expect("撤得掉");
    assert_eq!(
        挂在(&现场.catalog, 甲二).as_deref(),
        Some(甲),
        "撤销没把变体还回原来的作品",
    );
}

#[test]
fn 沉淀库已经说过话的那一条起头用它那一份事实_只把作品换掉() {
    let mut 现场 = 建现场();
    // 人先在待确认里手工裁过一遍：汉化组与第几版只有人说得出（ADR-0008）。
    let row = 现场
        .catalog
        .variant(&键(乙一))
        .expect("读得动")
        .expect("有这一行");
    let prints = romcat_core::identify::content_prints(&现场.catalog, std::iter::once(&row))
        .expect("算得出判据");
    let print = prints.get(&键(乙一)).expect("zip 里取得到内容判据").clone();
    let anchor = Anchor::Content {
        crc32: print.crc32,
        size: print.size,
        sha1: None,
    };
    现场
        .store
        .put(&Verdict::now(
            anchor.clone(),
            Decision::Release(Facts {
                work: 乙.to_string(),
                platform: Some("GB".to_string()),
                region: Some("Japan".to_string()),
                serial: Some("DMG-APAJ-1".to_string()),
                languages: Some("Ja,Zh-Hans".to_string()),
                chinese: Some(ChineseMark::FanTranslated),
                team: Some("漫游汉化".to_string()),
                version: Some("v1.1".to_string()),
            }),
        ))
        .expect("写得进沉淀库");

    let 计划 = 排一趟(&现场, Kind::Merge, 甲, &[乙一]);
    落一趟(&mut 现场, &计划);

    let index = verdict::Index::load(&现场.store, 主库标识).expect("读得出沉淀库");
    let 眼下 = index
        .by_content(print.crc32, print.size)
        .expect("锚上该有一条");
    let Decision::Release(facts) = &眼下.decision else {
        panic!("落下的不是一条发行版裁决");
    };
    assert_eq!(facts.work, 甲, "作品该换成保留的那个");
    // 人亲手补上的那几样**一个字不动**：汉化组与第几版重扫补不回来。
    assert_eq!(facts.team.as_deref(), Some("漫游汉化"));
    assert_eq!(facts.version.as_deref(), Some("v1.1"));
    assert_eq!(facts.serial.as_deref(), Some("DMG-APAJ-1"));
    assert_eq!(facts.chinese, Some(ChineseMark::FanTranslated));
}

#[test]
fn 只列出有冲突的字段_两边一样的与对方空着的都不列() {
    let 现场 = 建现场();
    let 冲突 = merge::conflicts(&现场.catalog, &Priorities::builtin(), 甲, &[乙.to_string()])
        .expect("算得出冲突");
    let 字段: Vec<Field> = 冲突.iter().map(|one| one.field).collect();

    assert!(
        字段.contains(&Field::Description),
        "两句不一样的简介该算冲突：{字段:?}"
    );
    // **对方空着的不算冲突**：保留作品有、别人没有，没什么可选的。
    // **两边一样的也不算**：开发商两边都是 Game Freak。
    assert!(
        !字段.contains(&Field::Developer),
        "两边说的一模一样，不该列成冲突：{字段:?}",
    );
    // **保留作品空着而对方有值的照列**：那一格默认用别人的补上（设计稿第三步那句话）。
    assert!(
        字段.contains(&Field::Year),
        "保留作品空着、对方有值，该列出来让人选：{字段:?}"
    );
    let 年份 = 冲突
        .iter()
        .find(|one| one.field == Field::Year)
        .expect("有年份这一行");
    assert!(年份.keep.is_none(), "保留作品那一格本来就空着");
    assert_eq!(
        年份.others.first().map(|offer| offer.said.values.clone()),
        Some(vec!["1996".to_string()]),
        "对方那一格没读对",
    );
    // **汉化组不在里面**：它挂在变体上（ADR-0012），合并一个字不动它。
    assert!(!字段.contains(&Field::TranslationGroup));
}

#[test]
fn 选了别的作品那一格_记成裁决挂到保留作品名下() {
    let mut 现场 = 建现场();
    let 冲突 = merge::conflicts(&现场.catalog, &Priorities::builtin(), 甲, &[乙.to_string()])
        .expect("算得出冲突");
    let 年份 = 冲突
        .into_iter()
        .find(|one| one.field == Field::Year)
        .expect("有年份这一行");
    let offer = 年份.others.first().expect("对方说了话").clone();
    merge::adopt(
        &mut 现场.catalog,
        &Priorities::builtin(),
        甲,
        Field::Year,
        &offer,
    )
    .expect("写得进去");

    let 值 = 现场
        .catalog
        .scraped_values(AnchorKind::Work.label(), 甲)
        .expect("读得动");
    let 那一格 = 值
        .iter()
        .find(|value| value.field == Field::Year.label() && value.source == VERDICT)
        .expect("该有一条源是裁决的年份");
    assert_eq!(那一格.value, "1996");
}

#[test]
fn 被合并作品的名字留作别名_源是裁决所以重折标题不碰它() {
    let mut 现场 = 建现场();
    merge::keep_aliases(&mut 现场.catalog, 甲, &[乙.to_string()]).expect("写得进去");
    现场.catalog.clear_titles().expect("重折一遍标题集合");

    let 叫法 = 现场.catalog.titles_of(甲).expect("读得动");
    assert!(
        叫法
            .iter()
            .any(|row| row.value == 乙 && row.source == VERDICT),
        "被合并作品的名字没留作别名、或者被重折冲掉了：{叫法:?}",
    );
}

#[test]
fn 合并之前算得出会发生什么_裁决数_前端条目从多少变多少_空掉的作品_子库() {
    let mut 现场 = 建现场();
    // 摆一台子库，规则选中 GB 那几个变体——它装着要动的那个变体，差量预览要重排。
    现场
        .catalog
        .put_sublibrary(&Sublibrary {
            name: "掌机".to_string(),
            target: "/Volumes/SDCARD/掌机".to_string(),
            target_raw: Some("/Volumes/SDCARD/掌机".to_string()),
            format: "Pegasus".to_string(),
            capacity: None,
            capability: None,
            capacity_by_device: false,
        })
        .expect("建得出子库");
    现场
        .catalog
        .add_rule("掌机", &Rule::parse("平台=GB").expect("规则读得懂"))
        .expect("写得进规则");

    let 计划 = 排一趟(&现场, Kind::Merge, 甲, &[乙一]);
    let 账 = merge::impact(&现场.catalog, &计划).expect("算得出会发生什么");

    assert_eq!(账.verdicts, 1, "写几条裁决");
    // 眼下：甲（GB）、乙（GB）、丙（SFC）、散落那一个（GB）各一条 = 4。
    assert_eq!(账.entries_before, 4, "眼下的前端条目数");
    // 合并之后乙那一条并进甲 = 3。
    assert_eq!(账.entries_after, 3, "合并之后的前端条目数");
    assert_eq!(
        账.emptied,
        [乙.to_string()],
        "乙一个变体都不剩了，名字该留作别名"
    );
    assert_eq!(
        账.sublibraries,
        ["掌机".to_string()],
        "装着这些变体的子库没点出来"
    );

    // 真落下去之后，前端条目数与刚才预告的那个对得上——**说的与做的是同一个数**。
    落一趟(&mut 现场, &计划);
    let 空计划 = merge::plan(
        &现场.catalog,
        &现场.store,
        主库标识,
        Kind::Merge,
        甲,
        &[键(甲一)],
    );
    assert!(空计划.is_err(), "一个真要归入的变体都没有时该当场说不成立");
    let 之后 =
        merge::impact(&现场.catalog, &排一趟(&现场, Kind::Merge, 丙, &[散一])).expect("算得出来");
    assert_eq!(
        之后.entries_before, 3,
        "落下之后眼下就是 3 条，与刚才预告的对得上"
    );
}

#[test]
fn 说不成立的话当场说不成立() {
    let 现场 = 建现场();
    let keys = vec![键(乙一)];
    assert!(
        matches!(
            merge::plan(
                &现场.catalog,
                &现场.store,
                主库标识,
                Kind::Merge,
                "   ",
                &keys
            ),
            Err(merge::MergeError::NoWork)
        ),
        "没说归到哪个作品名下时该当场说不成立",
    );
    assert!(
        matches!(
            merge::plan(&现场.catalog, &现场.store, 主库标识, Kind::Merge, 甲, &[]),
            Err(merge::MergeError::Nothing)
        ),
        "一个要归入的变体都没有时该当场说不成立",
    );
    assert!(
        matches!(
            merge::plan(
                &现场.catalog,
                &现场.store,
                主库标识,
                Kind::Merge,
                甲,
                &["库/没有这一份.zip".to_string()],
            ),
            Err(merge::MergeError::NoVariant(_))
        ),
        "点名的变体不在库里时该说清是哪一个",
    );
}

#[test]
fn 自动归入那一句说得出为什么还不能选() {
    let 话 = merge::auto_absorb_reason(甲);
    assert!(
        话.contains("沉淀库"),
        "得说清缺的是沉淀库里那条作品级记录：{话}"
    );
    assert!(话.contains("再合并一次"), "得说清眼下的走法：{话}");
    assert!(话.contains(甲), "得说清再合一次会归进哪个作品：{话}");
}

#[test]
fn 一条锚上的几份重复拷贝在一批里只落一条裁决() {
    // 同样大小的两份 zip 内容一模一样，扫出来是两个变体、同一条内容锚。
    let dir = temp_dir("merge-dup");
    for 相对 in ["GB/一份.zip", "GB/另一份拷贝.zip"] {
        写(&dir.path().join(相对), &一份内容(0xE1));
    }
    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    let mut options = ScanOptions::named(dir.path(), "库");
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");
    let work = catalog
        .add_work("原来那个", Provenance::Identified)
        .expect("建得出");
    catalog
        .write_identifications(
            &["GB/一份.zip", "GB/另一份拷贝.zip"]
                .into_iter()
                .map(|相对| Identification {
                    variant_key: 键(相对),
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
                })
                .collect::<Vec<_>>(),
        )
        .expect("写得进");
    let mut store = Store::in_memory().expect("开得出沉淀库");
    let keys: Vec<String> = ["GB/一份.zip", "GB/另一份拷贝.zip"]
        .into_iter()
        .map(键)
        .collect();
    let 计划 = merge::plan(&catalog, &store, 主库标识, Kind::Merge, "并到这儿", &keys)
        .expect("排得出计划");
    assert_eq!(计划.verdicts(), 2, "计划里两个变体各占一行");
    let 账 = merge::apply(&mut catalog, &mut store, &计划).expect("落得下");
    let 锚: BTreeSet<Anchor> = store
        .all()
        .expect("读得出")
        .into_iter()
        .map(|one| one.anchor)
        .collect();
    assert_eq!(锚.len(), 1, "同一条内容锚上只该有一条裁决：{锚:?}");
    // 两个变体都改挂过去了。
    for 相对 in ["GB/一份.zip", "GB/另一份拷贝.zip"] {
        assert_eq!(挂在(&catalog, 相对).as_deref(), Some("并到这儿"));
    }
    romcat_core::triage::undo_batch(&mut catalog, &mut store, 账.batch).expect("撤得掉");
    for 相对 in ["GB/一份.zip", "GB/另一份拷贝.zip"] {
        assert_eq!(
            挂在(&catalog, 相对).as_deref(),
            Some("原来那个"),
            "撤销该把两份拷贝一起还回去",
        );
    }
}

/// 夹具自己摆得对不对：`Manifest` 与平台表这一趟用不上，留一句免得读的人去找。
#[test]
fn 夹具摆出来的是三个作品加一份还没认出作品的() {
    let 现场 = 建现场();
    let _ = Manifest::default();
    assert_eq!(挂在(&现场.catalog, 甲一).as_deref(), Some(甲));
    assert_eq!(挂在(&现场.catalog, 甲二).as_deref(), Some(甲));
    assert_eq!(挂在(&现场.catalog, 乙一).as_deref(), Some(乙));
    assert_eq!(挂在(&现场.catalog, 丙一).as_deref(), Some(丙));
    assert_eq!(挂在(&现场.catalog, 散一), None);
}
