//! **刮削接缝**：识别跑完之后，元数据与媒体是怎么落到锚点上的。
//!
//! 密集逻辑挂在纯函数上（`scrape::dat` 读 TOSEC 的名字、`scrape::local` 的两条归属规则、
//! `scrape::priority` 的合并各有自己的单元测试），这里要证的是另外六件事——正是票 13
//! 的六条验收：
//!
//! 1. 离线档只用本地数据源，跑完不需要任何网络句柄；
//! 2. 多个源按**字段级**优先级合并；
//! 3. 结果持久化缓存，重跑不重新计算；
//! 4. 媒体按**内容哈希**进池，中立库只记映射；
//! 5. 同一份媒体被多个条目引用时**只存一份**；
//! 6. **识别与刮削是两个可分别重跑的阶段**——重跑识别不该把刮削结论冲掉。
//!
//! fixture 的形状照真机来（`docs/library-facts.md`）：内容在**透明容器**里，媒体是
//! 汉化目录里那几张 `封面.png` / `1.jpg`，而 `FC/一堆/` 底下几个 zip 挤在一起——
//! 那正是「独占目录」规则必须缩手的场景。

use std::fs;
use std::path::Path;

use romcat_core::catalog::Catalog;
use romcat_core::dat::Convention;
use romcat_core::dat::logiqx::{DatHeader, GameRecord, RomRecord};
use romcat_core::dat::repo::{DatMeta, DatRepo, Unit};
use romcat_core::fs::RealFs;
use romcat_core::identify;
use romcat_core::identify::fuzzy;
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::scrape::pool::MediaPool;
use romcat_core::scrape::{self, Priorities};
use romcat_core::testing::container::{ZipEntrySpec, crc32, zip_container};
use romcat_core::testing::{TempDir, temp_dir};

/// 原版魂斗罗的字节。
fn 原版() -> Vec<u8> {
    vec![0xA1; 4_096]
}

/// 汉化版的字节：改过，No-Intro 与 Redump 政策上永不收录，只有 TOSEC 有。
fn 汉化版() -> Vec<u8> {
    vec![0xB2; 4_096]
}

/// 一张封面。**两个变体旁边各放一份一模一样的**，用来验「只存一份」。
fn 封面() -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend(std::iter::repeat_n(0x5Au8, 512));
    bytes
}

/// 一张截图。与封面**不同**的内容。
fn 截图() -> Vec<u8> {
    let mut bytes = b"\xff\xd8\xff\xe0".to_vec();
    bytes.extend(std::iter::repeat_n(0x77u8, 256));
    bytes
}

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

struct 现场 {
    dir: TempDir,
    pool_dir: TempDir,
    catalog: Catalog,
    repo: DatRepo,
}

const 原版变体: &str = "FC/魂斗罗原版/Contra (Japan).zip";
const 汉化变体: &str = "FC/魂斗罗汉化/魂斗罗[dwt_so 汉化].zip";
const 作品: &str = "Contra";

fn 建现场() -> 现场 {
    let dir = temp_dir("scrape");
    let root = dir.path();

    // ── 两个变体各自独占一个目录，旁边躺着图。这是真库里汉化合集的标准形态。
    写(
        &root.join(原版变体),
        &zip_container(&[ZipEntrySpec::stored("Contra.nes", 原版())]),
    );
    写(&root.join("FC/魂斗罗原版/封面.png"), &封面());
    写(
        &root.join(汉化变体),
        &zip_container(&[ZipEntrySpec::stored("魂斗罗.nes", 汉化版())]),
    );
    // **同一张封面**：内容一模一样，名字与位置都不同。
    写(&root.join("FC/魂斗罗汉化/封面.png"), &封面());
    写(&root.join("FC/魂斗罗汉化/1.jpg"), &截图());

    // ── 一个目录里两个变体：**同目录的图一张都不许认**。
    写(
        &root.join("FC/一堆/甲.zip"),
        &zip_container(&[ZipEntrySpec::stored("甲.nes", vec![0xC3; 1_024])]),
    );
    写(
        &root.join("FC/一堆/乙.zip"),
        &zip_container(&[ZipEntrySpec::stored("乙.nes", vec![0xD4; 1_024])]),
    );
    写(&root.join("FC/一堆/无关.jpg"), &截图());

    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::new(root);
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &CancelToken::new()).expect("扫得动");

    现场 {
        dir,
        pool_dir: temp_dir("scrape-pool"),
        catalog,
        repo: 建_dat(),
    }
}

fn 条目(name: &str, rom: &str, bytes: &[u8]) -> GameRecord {
    GameRecord {
        name: name.to_string(),
        roms: vec![RomRecord {
            name: rom.to_string(),
            size: Some(bytes.len() as u64),
            crc32: Some(crc32(bytes)),
            ..RomRecord::default()
        }],
        ..GameRecord::default()
    }
}

fn 装(repo: &mut DatRepo, source: &str, dat: &str, games: &[GameRecord]) {
    let mut writer = repo
        .begin(&Unit {
            source: source.to_string(),
            name: dat.to_string(),
            url: "https://example.invalid/x".to_string(),
            fingerprint: "sha".to_string(),
        })
        .expect("开得了事务");
    writer
        .write_dat(
            &DatMeta {
                name: dat.to_string(),
                platform: "FC".to_string(),
                convention: Convention::AsIs,
                header: DatHeader::default(),
            },
            games,
        )
        .expect("写得进");
    writer.commit().expect("提交");
}

fn 建_dat() -> DatRepo {
    let mut repo = DatRepo::in_memory().expect("开得出来");
    // No-Intro：条目名最整齐，但**没有年份也没有发行商**。
    装(
        &mut repo,
        "No-Intro",
        "Nintendo - Nintendo Entertainment System",
        &[条目("Contra (Japan)", "Contra (Japan).nes", &原版())],
    );
    // TOSEC：名字里带着发行日期与发行商，还收录了汉化版。
    装(
        &mut repo,
        "TOSEC",
        "TOSEC/Nintendo Famicom & Entertainment System - Games - [NES].dat",
        &[
            条目(
                "Contra (1988-02-09)(Konami)(JP)",
                "Contra (1988-02-09)(Konami)(JP).nes",
                &原版(),
            ),
            条目(
                "Contra (1988-02-09)(Konami)(JP)[tr zh dwt_so][v.20030208]",
                "Contra [tr zh].nes",
                &汉化版(),
            ),
        ],
    );
    repo
}

fn 识别(现场: &mut 现场) {
    identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &identify::Ammo {
            repo: &现场.repo,
            verdicts: &verdict::Index::empty(),
            naming: &fuzzy::Naming::off(),
            guessing: &identify::model::Guessing::off(),
            titledb: None,
        },
        &identify::Options::new(现场.dir.path()),
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别不该失败");
}

fn 刮削(现场: &mut 现场) -> scrape::Outcome {
    刮削一趟(现场, false)
}

fn 刮削一趟(现场: &mut 现场, refresh: bool) -> scrape::Outcome {
    刮削带上限(现场, refresh, None)
}

fn 刮削带上限(现场: &mut 现场, refresh: bool, cap: Option<u64>) -> scrape::Outcome {
    let mut options = scrape::Options::new(现场.dir.path(), 现场.pool_dir.path());
    options.refresh = refresh;
    options.max_media_bytes = cap;
    scrape::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &Priorities::builtin(),
        &options,
        None,
        &mut scrape::RunContext {
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
            naming: &fuzzy::Naming::off(),
        },
    )
    .expect("刮削不该失败")
}

/// 一个锚点上某个字段、某个源给的值。
fn 值(现场: &现场, anchor: &str, subject: &str, field: &str, source: &str) -> Option<String> {
    现场
        .catalog
        .scraped_values(anchor, subject)
        .expect("读得出")
        .into_iter()
        .find(|value| value.field == field && value.source == source)
        .map(|value| value.value)
}

#[test]
fn 离线档只用本地数据源() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let outcome = 刮削(&mut 现场);

    // `scrape::run` 的参数表里根本没有网络句柄——它拿到的是主库的**只读**视图、
    // 中立库、优先级表。这一条不是靠自觉，档案那道闸门会拒掉任何自报「要联网」的源。
    assert_eq!(
        outcome.report.sources,
        vec![
            "No-Intro",
            "Redump",
            "TOSEC",
            "MAME",
            "GoodNES",
            "文件名",
            "本地媒体"
        ],
        "离线档该正好是这七个本地源"
    );
}

#[test]
fn 元数据按层挂到锚点上() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);

    // **作品**这一层：标题两个源都给得出，年份与发行商只有 TOSEC 给得出。
    assert_eq!(
        值(&现场, "作品", 作品, "标题", "No-Intro").as_deref(),
        Some("Contra")
    );
    assert_eq!(
        值(&现场, "作品", 作品, "标题", "TOSEC").as_deref(),
        Some("Contra")
    );
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988")
    );
    assert_eq!(
        值(&现场, "作品", 作品, "发行商", "TOSEC").as_deref(),
        Some("Konami")
    );
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "No-Intro"),
        None,
        "No-Intro 的名字里根本没有年份，不该凭空冒出来"
    );

    // **变体**这一层：汉化组挂在这里（汉化版是变体不是发行版，ADR-0012）。
    assert_eq!(
        值(&现场, "变体", 汉化变体, "汉化组", "TOSEC").as_deref(),
        Some("dwt_so")
    );
    // 文件名兜底：DAT 认不出来的那些变体，标题只能从这儿来。
    assert_eq!(
        值(&现场, "变体", "FC/一堆/甲.zip", "标题", "文件名").as_deref(),
        Some("甲")
    );
}

#[test]
fn 字段级优先级用一个源的标题配另一个源的年份() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);

    let priorities = Priorities::builtin();
    let values = 现场.catalog.scraped_values("作品", 作品).expect("读得出");
    let merged = priorities.merge(Some("FC"), &values);

    assert_eq!(
        merged["标题"].source, "No-Intro",
        "标题该取条目名最整齐的那家"
    );
    assert_eq!(merged["年份"].source, "TOSEC", "而年份只有 TOSEC 给得出");
    assert_eq!(merged["发行商"].value, "Konami");

    // **改一次优先级不必重新采集**：三元组并存，换的只是排序。
    let 换一份 = Priorities::load(&写一份优先级表(&现场)).expect("读得动");
    assert_eq!(
        换一份.merge(Some("FC"), &values)["标题"].source,
        "TOSEC",
        "把 TOSEC 提到前面之后，标题该跟着换源——而库里一条值都没重采"
    );
}

/// 写一份把 TOSEC 提到 No-Intro 前面的优先级表。
fn 写一份优先级表(现场: &现场) -> std::path::PathBuf {
    let path = 现场.pool_dir.path().join("我的优先级.toml");
    fs::write(
        &path,
        "\"版本\" = 1\n\
         [[\"字段\"]]\n\
         \"名\" = \"标题\"\n\
         \"顺序\" = [\"裁决\", \"TOSEC\", \"No-Intro\", \"文件名\"]\n\
         [[\"字段\"]]\n\
         \"名\" = \"年份\"\n\
         \"顺序\" = [\"裁决\", \"TOSEC\"]\n",
    )
    .expect("写得出");
    path
}

#[test]
fn 媒体按内容哈希入池且同一份只存一份() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let outcome = 刮削(&mut 现场);

    // 两个变体旁边各有一份**一模一样**的封面，加上一张截图：三条引用、两份内容。
    let 原版媒体 = 现场
        .catalog
        .scraped_media("变体", 原版变体)
        .expect("读得出");
    let 汉化媒体 = 现场
        .catalog
        .scraped_media("变体", 汉化变体)
        .expect("读得出");
    assert_eq!(原版媒体.len(), 1);
    assert_eq!(汉化媒体.len(), 2);

    let 原版封面 = &原版媒体[0];
    let 汉化封面 = 汉化媒体
        .iter()
        .find(|m| m.kind == "封面")
        .expect("汉化那边也有封面");
    assert_eq!(
        原版封面.hash, 汉化封面.hash,
        "**文件名不是媒体的主键**：两份一模一样的字节必须算出同一个哈希"
    );

    // 池里只有一个文件。
    let pool = MediaPool::open(现场.pool_dir.path()).expect("开得出池");
    assert!(pool.contains(&原版封面.hash, "png"), "池里该有这一份");
    assert_eq!(outcome.new_blobs, 2, "两份不同的内容");
    assert_eq!(outcome.deduped, 1, "第三份算出来发现已经有了");

    let counts = 现场.catalog.pool_counts().expect("数得出");
    assert_eq!(counts.blobs, 2, "池里两份内容");
    assert_eq!(counts.refs, 3, "三条引用");
    assert_eq!(counts.shared, 1, "其中一份被两个锚点引用");

    // **中立库只记映射**：文件躺在池里，库里存的是哈希。
    assert!(
        !原版封面.hash.is_empty() && 原版封面.hash.len() == 64,
        "哈希该是 SHA-256 的 64 位十六进制"
    );
}

#[test]
fn 挤在一个目录里的变体一张图都不认() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);

    for key in ["FC/一堆/甲.zip", "FC/一堆/乙.zip"] {
        assert!(
            现场
                .catalog
                .scraped_media("变体", key)
                .expect("读得出")
                .is_empty(),
            "{key} 所在的目录里有两个变体，同目录的图属于谁说不清，宁可不给"
        );
    }
}

#[test]
fn 重跑不重新计算() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let 首趟 = 刮削(&mut 现场);
    assert!(首趟.read_bytes > 0, "首趟要把媒体读进池里");

    let 第二趟 = 刮削(&mut 现场);
    assert_eq!(第二趟.read_bytes, 0, "第二趟一个字节都不该再读盘");
    assert_eq!(第二趟.read_files, 0);
    assert!(
        第二趟.reused_probes > 0,
        "输入指纹没变的「锚点 × 源」该整条跳过"
    );
    assert_eq!(第二趟.new_blobs, 0, "池里不该多出任何东西");

    // 结论仍然在，且没有被重复插成两条。
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988")
    );
    assert_eq!(现场.catalog.pool_counts().expect("数得出").refs, 3);
}

#[test]
fn refresh_把结论重采一遍但池里的文件一个都不删() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);
    let 重来 = 刮削一趟(&mut 现场, true);

    assert_eq!(重来.reused_probes, 0, "--refresh 之后没有一条能跳过");
    assert_eq!(
        重来.read_bytes, 0,
        "但**算过的媒体哈希还在**——重采的是结论，不是重读一遍盘"
    );
    assert_eq!(现场.catalog.pool_counts().expect("数得出").blobs, 2);
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988")
    );
}

#[test]
fn 识别重跑之后刮削结论还在() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);
    let 之前 = 现场.catalog.scraped_values("作品", 作品).expect("读得出");
    assert!(!之前.is_empty());

    // 重跑识别：它会把自己上一轮造的作品与发行版**整批删掉再造一遍**，
    // 新造出来的行拿的是新的行号。刮削结论挂在**作品名**上，因此活得下来。
    识别(&mut 现场);

    let 之后 = 现场.catalog.scraped_values("作品", 作品).expect("读得出");
    assert_eq!(之前, 之后, "重跑识别不该把刮削结论冲掉");
    assert_eq!(
        现场
            .catalog
            .scraped_media("变体", 原版变体)
            .expect("读得出")
            .len(),
        1,
        "媒体映射同样活得下来——它挂在变体的键上"
    );

    // 反过来也成立：刮削可以在不重跑识别的前提下再跑一遍。
    let 再刮 = 刮削(&mut 现场);
    assert_eq!(再刮.read_bytes, 0);
}

#[test]
fn 报告说得出离线档补不上哪些字段() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let outcome = 刮削(&mut 现场);

    // 这三样本地数据源里没有，报告必须点名——空着不点名，用户会以为刮完了。
    for field in ["简介", "类型", "开发商"] {
        assert!(
            outcome.report.gaps.iter().any(|gap| gap == field),
            "报告该点名「{field}」补不上"
        );
    }
    let text = outcome.report.render_text();
    assert!(text.contains("一个值都没采到的字段"));
    assert!(text.contains("换 `--profile 在线` 跑一趟才补得上"));
    assert!(text.contains("媒体池"));
}

#[test]
fn 一个字节都不读主库也采得出元数据() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let outcome = 不收媒体(&mut 现场);

    assert_eq!(outcome.read_bytes, 0);
    assert_eq!(outcome.new_blobs, 0);
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988")
    );
    assert!(
        现场
            .catalog
            .scraped_media("变体", 原版变体)
            .expect("读得出")
            .is_empty(),
        "关掉媒体就一份都不该进池"
    );
}

#[test]
fn 不收媒体不会把收过的媒体扔掉() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);
    let 收过的 = 现场.catalog.pool_counts().expect("数得出");
    assert_eq!(收过的.refs, 3);

    // `--no-media` 说的是「这趟不收媒体」，**不是「把收过的扔了」**。
    let outcome = 不收媒体(&mut 现场);
    assert_eq!(outcome.forgotten, 0, "本地媒体源整个没参加，谈不上作废");
    assert_eq!(现场.catalog.pool_counts().expect("数得出").refs, 3);
    assert_eq!(
        现场
            .catalog
            .scraped_media("变体", 原版变体)
            .expect("读得出")
            .len(),
        1
    );
}

fn 不收媒体(现场: &mut 现场) -> scrape::Outcome {
    let mut options = scrape::Options::new(现场.dir.path(), 现场.pool_dir.path());
    options.media = false;
    scrape::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &Priorities::builtin(),
        &options,
        None,
        &mut scrape::RunContext {
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
            naming: &fuzzy::Naming::off(),
        },
    )
    .expect("刮削不该失败")
}

#[test]
fn 超上限与读不动是两件事() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    // 上限设成 100 字节：三份媒体全都超了。**它们文件好好的**，
    // 与「读不动」（ADR-0021 的第三态）不是同一件事，两个计数器必须分得开。
    let outcome = 刮削带上限(&mut 现场, false, Some(100));

    assert_eq!(outcome.oversized_media, 3, "三份都超上限");
    assert_eq!(outcome.unreadable_media, 0, "一份都不是读不动");
    assert_eq!(outcome.read_bytes, 0, "超上限的在打开文件之前就该拦下来");
    assert_eq!(outcome.new_blobs, 0);
    assert!(
        现场
            .catalog
            .scraped_media("变体", 原版变体)
            .expect("读得出")
            .is_empty()
    );
}

#[test]
fn 一个源不再说话时它上一轮的结论被清掉() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988")
    );

    // 把 TOSEC 从 DAT 库里撤掉再重跑识别：那个作品上不再有任何 TOSEC 条目。
    // 上一轮 TOSEC 挂上去的年份与发行商**必须跟着消失**——留着的话，它们带着一条
    // 指向已经不存在的条目的**依据**，事后复核会对不上。
    现场.repo = {
        let mut repo = DatRepo::in_memory().expect("开得出来");
        装(
            &mut repo,
            "No-Intro",
            "Nintendo - Nintendo Entertainment System",
            &[条目("Contra (Japan)", "Contra (Japan).nes", &原版())],
        );
        repo
    };
    识别(&mut 现场);
    let outcome = 刮削(&mut 现场);

    assert!(outcome.forgotten > 0, "该有「锚点 × 源」被清掉");
    assert_eq!(值(&现场, "作品", 作品, "年份", "TOSEC"), None);
    assert_eq!(值(&现场, "作品", 作品, "发行商", "TOSEC"), None);
    assert_eq!(值(&现场, "变体", 汉化变体, "汉化组", "TOSEC"), None);
    // 别的源一条都没被碰。
    assert_eq!(
        值(&现场, "作品", 作品, "标题", "No-Intro").as_deref(),
        Some("Contra")
    );
    assert_eq!(
        值(&现场, "变体", 汉化变体, "标题", "文件名").as_deref(),
        Some("魂斗罗")
    );
}

#[test]
fn 调高媒体上限之后那些媒体真的会被收进来() {
    let mut 现场 = 建现场();
    识别(&mut 现场);

    // 先用一个小得离谱的上限跑一趟：三份媒体全被挡在外面。
    let 收紧 = 刮削带上限(&mut 现场, false, Some(100));
    assert_eq!(收紧.oversized_media, 3);
    assert_eq!(现场.catalog.pool_counts().expect("数得出").refs, 0);

    // **上限放开再跑，它们必须进来。**
    // 上限改变采集的结果，所以它必须进**输入指纹**——不进的话，这一趟会一口咬定
    // 「输入没变」而整条跳过，那三份永远收不进来，只有整份 `--refresh` 才逃得掉。
    let 放开 = 刮削带上限(&mut 现场, false, None);
    assert_eq!(放开.oversized_media, 0);
    assert_eq!(放开.new_blobs, 2, "两份不同的内容");
    assert_eq!(现场.catalog.pool_counts().expect("数得出").refs, 3);

    // 反过来收紧也一样：上限降下来，此前收进来的那一份也该被挡在外面。
    let 再收紧 = 刮削带上限(&mut 现场, false, Some(100));
    assert_eq!(再收紧.oversized_media, 3);
    assert_eq!(现场.catalog.pool_counts().expect("数得出").refs, 0);
}

#[test]
fn 报告把合并真的跑了一遍() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let outcome = 刮削(&mut 现场);

    let 标题 = outcome
        .report
        .fields
        .iter()
        .find(|row| row.field == "标题")
        .expect("该有标题这一行");
    // 三个源都给了标题（No-Intro、TOSEC 各一个作品锚点，文件名给了每个变体），
    // 但**作品那一层上 No-Intro 胜出**——TOSEC 贡献了值却一条都没胜出。
    assert!(标题.sources.iter().any(|(source, _)| source == "TOSEC"));
    let 胜出: Vec<&str> = 标题
        .winners
        .iter()
        .map(|(source, _)| source.as_str())
        .collect();
    assert!(胜出.contains(&"No-Intro"), "作品那一层该是 No-Intro 胜出");
    assert!(
        !胜出.contains(&"TOSEC"),
        "TOSEC 排在 No-Intro 之后，一条都不该胜出"
    );

    // 年份只有 TOSEC 给得出，那它当然胜出——「用一个源的标题配另一个源的年份」
    // 这件事，报告自己就说得出来。
    let 年份 = outcome
        .report
        .fields
        .iter()
        .find(|row| row.field == "年份")
        .expect("该有年份这一行");
    assert_eq!(年份.winners, vec![("TOSEC".to_string(), 1)]);
    assert_eq!(年份.subjects, 1);

    assert!(
        outcome.report.unknown_sources.is_empty(),
        "内置优先级表里点名的源，这一档全都有"
    );
    assert!(outcome.report.render_text().contains("合并之后谁说了算"));
}

// ════════════════════════════════════════════════════════════════════════
// 票 14：在线档与配额守护
//
// **真实凭据这一趟拿不到，也不该去申请**——ScreenScraper 的 devid 要在论坛人工审批，
// 而用户的账号与 IP 不是试验场。所以下面这些验的是**一个可复现的假服务器**：
// `CannedFetcher` 与真的走同一条路（同一个 `Fetcher` 接缝、同一道取数闸门、同一套
// 状态码处置），差别只在「字节从哪儿来」。它验得了限流、431 硬停止、闸门与续跑；
// 验不了的只有「真实凭据下的端到端」那一件，那件挂着账。
// ════════════════════════════════════════════════════════════════════════

use romcat_core::dat::CannedFetcher;
use romcat_core::scrape::online::{self, Credentials, Halt, Limits, Net};
use romcat_core::verdict;

/// `jeuInfos.php` 的落点。假服务器按**前缀**答，因为查询串里带着凭据与逐条参数。
const 查询端点: &str = "https://api.screenscraper.fr/api2/jeuInfos.php";
const 封面地址: &str = "https://www.screenscraper.fr/image.php?gameid=1&media=box-2D";

fn 凭据() -> Credentials {
    Credentials {
        dev_id: "测试".to_string(),
        dev_password: "口令".to_string(),
        soft_name: "romcat-test".to_string(),
        user: None,
        user_password: None,
    }
}

/// 不限流的一组参数：测试不该真的等在那儿。频率那一条另有专门的单元测试。
fn 宽松() -> Limits {
    Limits {
        interval: std::time::Duration::from_millis(0),
        budget: 100,
        backoff: std::time::Duration::from_millis(0),
    }
}

/// 一份 `jeuInfos` 的答复。形状照调研 §1.4 / §1.5。
fn 答复(带封面: bool) -> Vec<u8> {
    let medias = if 带封面 {
        serde_json::json!([{
            "type": "box-2D", "region": "wor", "url": 封面地址,
            "format": "png", "size": "520"
        }])
    } else {
        serde_json::json!([])
    };
    serde_json::to_vec(&serde_json::json!({
        "header": {"success": "true"},
        "response": {
            "ssuser": {
                "maxrequestsperday": "20000", "requeststoday": "7",
                "maxrequestskoperday": "500", "requestskotoday": "1",
                "maxthreads": "1"
            },
            "jeu": {
                "noms": [{"region": "wor", "text": "Contra"}],
                "synopsis": [{"langue": "zh", "text": "两个大兵闯外星人基地"}],
                "developpeur": {"id": 3, "text": "Konami 开发部"},
                "genres": [{"noms": [{"langue": "zh", "text": "动作"}]}],
                "medias": medias
            }
        }
    }))
    .expect("造得出 JSON")
}

fn 刮削在线(现场: &mut 现场, fetcher: &CannedFetcher, limits: Limits) -> scrape::Outcome {
    let mut options = scrape::Options::new(现场.dir.path(), 现场.pool_dir.path());
    options.profile = scrape::Profile::Online;
    let cancel = CancelToken::new();
    let net = Net::new(fetcher, limits, 凭据(), &cancel);
    scrape::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &Priorities::builtin(),
        &options,
        Some(&net),
        &mut scrape::RunContext {
            cancel: &cancel,
            progress: &mut |_| {},
            naming: &fuzzy::Naming::off(),
        },
    )
    .expect("刮削不该失败——配额超限是「停」不是「错」")
}

#[test]
fn 两档在任务级别切换而优先级表一份都不用换() {
    let mut 现场 = 建现场();
    识别(&mut 现场);

    // 第一趟离线：一个网络请求都不发，而简介、类型、开发商一个都补不上。
    let 离线 = 刮削(&mut 现场);
    assert_eq!(离线.report.profile, "离线档");
    assert!(离线.online.is_none(), "离线档不该有在线的账");
    for field in ["简介", "类型", "开发商"] {
        assert!(离线.report.gaps.iter().any(|gap| gap == field));
    }
    assert!(
        离线
            .report
            .idle_sources
            .contains(&"ScreenScraper".to_string()),
        "报告该说清「这一档没参加的源」，而不是把它报成不存在的源"
    );
    assert!(离线.report.unknown_sources.is_empty());

    // 第二趟在线：**同一份库、同一份优先级表**，只换了 `--profile`。
    let fetcher = CannedFetcher::new()
        .with_prefix(查询端点, 200, 答复(false))
        .with(封面地址, 封面());
    let 在线 = 刮削在线(&mut 现场, &fetcher, 宽松());
    assert_eq!(在线.report.profile, "在线档");
    assert!(在线.report.sources.contains(&"ScreenScraper".to_string()));

    // 离线档补不上的那三样，现在有了。
    assert_eq!(
        值(&现场, "作品", 作品, "简介", "ScreenScraper").as_deref(),
        Some("两个大兵闯外星人基地"),
        "**中文简介是这个库要的东西**，在线源取简介时中文排在最前"
    );
    assert_eq!(
        值(&现场, "作品", 作品, "开发商", "ScreenScraper").as_deref(),
        Some("Konami 开发部")
    );
    assert_eq!(
        值(&现场, "作品", 作品, "类型", "ScreenScraper").as_deref(),
        Some("动作")
    );

    // **离线那一趟的结论一条都没被冲掉**：三元组并存，两档共用同一份缓存。
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988")
    );
    assert_eq!(
        值(&现场, "作品", 作品, "标题", "No-Intro").as_deref(),
        Some("Contra")
    );

    // 而**字段级优先级照样可以单独覆盖**，且不必重采：标题这一栏在线源排在 DAT 之后。
    let values = 现场.catalog.scraped_values("作品", 作品).expect("读得出");
    let merged = Priorities::builtin().merge(Some("FC"), &values);
    assert_eq!(merged["标题"].source, "No-Intro");
    assert_eq!(merged["简介"].source, "ScreenScraper");
    assert_eq!(
        Priorities::load(&写一份优先级表(&现场))
            .expect("读得动")
            .merge(Some("FC"), &values)["标题"]
            .source,
        "TOSEC",
        "换一份表，标题跟着换源——库里一条值都没重采"
    );
}

#[test]
fn 在线档默认限流_并发与频率有明确上限() {
    // **默认值本身就是验收对象**：这一档的默认必须是收着的，而不是「用户不调就放开跑」。
    let 默认 = Limits::default();
    assert_eq!(online::MAX_CONCURRENCY, 1, "并发上限恒为 1，而且不给调");
    assert!(默认.interval >= std::time::Duration::from_millis(1_000));
    assert!(
        默认.budget > 0 && 默认.budget <= 1_000,
        "自设的请求上限要往小里设"
    );

    let mut 现场 = 建现场();
    识别(&mut 现场);
    let fetcher = CannedFetcher::new().with_prefix(查询端点, 200, 答复(false));
    let outcome = 刮削在线(&mut 现场, &fetcher, 宽松());

    // 报告必须把这几个数说出来——用户要在对着真账号跑之前看得见它们。
    let text = outcome.report.render_text();
    assert!(text.contains("在线那一侧的账"), "{text}");
    assert!(text.contains("并发            1"), "{text}");
    let 在线 = outcome.report.online.expect("在线档该有这一节");
    assert_eq!(在线.concurrency, 1);
    // 服务端允许几个线程也报出来——「我们比对面允许的还保守」该是看得见的事实。
    assert_eq!(在线.server_threads, Some(1));
    assert!(text.contains("服务端允许"), "{text}");
    // **服务端说的数字才作数**：三份官方文档给了三个不同的日配额。
    assert_eq!(在线.requests_left, Some(20_000 - 7));
    assert_eq!(在线.ko_left, Some(500 - 1));
}

#[test]
fn 配额超限是硬停止_不重试也不换账号() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    // 先离线跑一趟，攒下一批结论——它们必须活过下面那次硬停止。
    刮削(&mut 现场);

    // 431 是**专门为「未识别 ROM」设的**那一份配额。这个库里上万个变体在
    // ScreenScraper 眼里正是未识别 ROM，撞穿了连账号带 IP 一起永久封禁。
    let fetcher = CannedFetcher::new().with_prefix(
        查询端点,
        431,
        b"Faite du tri dans vos fichiers roms et repassez demain !".to_vec(),
    );
    let outcome = 刮削在线(&mut 现场, &fetcher, 宽松());

    let halt = outcome.halted.expect("431 该让整趟停下来");
    assert!(
        matches!(halt, Halt::Quota { .. }),
        "431 是配额超限，不是网络抖动"
    );
    assert!(halt.describe().contains("不重试也不换账号"));
    assert_eq!(
        fetcher.asked().len(),
        1,
        "**撞上 431 之后一个请求都不许再发**——重试与换账号都是永久封禁那条路"
    );

    // **已经采完的那部分留在库里。** 报告照出，退出的是这一趟不是这些结论。
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988")
    );
    assert_eq!(现场.catalog.pool_counts().expect("数得出").refs, 3);
    assert!(outcome.report.render_text().contains("这一趟停了"));
    assert!(
        值(&现场, "作品", 作品, "简介", "ScreenScraper").is_none(),
        "没问到就什么都不写——写了下一趟会整条跳过，那条结论就永远缺着"
    );
}

#[test]
fn 只对已确认的条目发请求_未识别的变体不消耗配额() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let fetcher = CannedFetcher::new().with_prefix(查询端点, 200, 答复(false));
    let outcome = 刮削在线(&mut 现场, &fetcher, 宽松());

    // 库里四个变体：两个撞上了 DAT，`FC/一堆/甲.zip` 与 `乙.zip` 一条候选都没有。
    assert_eq!(outcome.report.unconfirmed_variants, 2);
    assert_eq!(outcome.report.queryable_works, Some(1));
    // **一部作品一次查询**：两个已确认的变体属于同一部作品，合起来只发一个请求。
    // 未识别那两个一个字节都没发出去。
    assert_eq!(
        fetcher.asked().len(),
        1,
        "发过的请求：{:?}",
        fetcher.asked()
    );
    let asked = &fetcher.asked()[0];
    assert!(!asked.contains("甲"), "{asked}");
    assert!(!asked.contains("乙"), "{asked}");
    // 官方要求哈希与文件大小同发，少一样这次查询注定未命中、白扣一份配额。
    assert!(asked.contains("crc="), "{asked}");
    assert!(asked.contains("romtaille="), "{asked}");

    let text = outcome.report.render_text();
    assert!(text.contains("一个请求都没有为它们发出去"), "{text}");
}

#[test]
fn 在线结果进同一份持久化缓存_重跑不再问一遍() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let fetcher = CannedFetcher::new()
        .with_prefix(查询端点, 200, 答复(true))
        .with(封面地址, 封面());
    刮削在线(&mut 现场, &fetcher, 宽松());
    let 首趟 = fetcher.asked().len();
    assert!(首趟 > 0);

    // 第二趟：输入指纹没变，**整条跳过**。离线档省的是算力，在线档省的是配额。
    let 再来 = 刮削在线(&mut 现场, &fetcher, 宽松());
    assert_eq!(fetcher.asked().len(), 首趟, "第二趟一个请求都不该再发");
    assert!(再来.reused_probes > 0);
    assert_eq!(再来.online.expect("有在线的账").requests, 0);
    assert_eq!(
        值(&现场, "作品", 作品, "简介", "ScreenScraper").as_deref(),
        Some("两个大兵闯外星人基地")
    );
}

#[test]
fn 网络失败不影响已完成的部分_可续跑() {
    let mut 现场 = 建现场();
    识别(&mut 现场);

    // 一个什么都答不上来的服务器：在线那一对采不成，**而离线那七个源照常落库**。
    let 断网 = CannedFetcher::new();
    let 第一趟 = 刮削在线(&mut 现场, &断网, 宽松());
    assert!(第一趟.skipped > 0, "在线那一对该被记成「没采成」");
    assert!(第一趟.halted.is_none(), "一次网络错不该让整趟停下来");
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988")
    );
    assert_eq!(
        现场.catalog.pool_counts().expect("数得出").refs,
        3,
        "本地媒体照收"
    );
    assert!(值(&现场, "作品", 作品, "简介", "ScreenScraper").is_none());

    // 网回来了，**接着采**：没采成的那一对没写过采集记录，所以这一趟还会再来一次。
    let 通了 = CannedFetcher::new().with_prefix(查询端点, 200, 答复(false));
    let 第二趟 = 刮削在线(&mut 现场, &通了, 宽松());
    assert_eq!(通了.asked().len(), 1);
    assert_eq!(第二趟.skipped, 0);
    assert_eq!(
        值(&现场, "作品", 作品, "简介", "ScreenScraper").as_deref(),
        Some("两个大兵闯外星人基地")
    );
}

#[test]
fn 在线媒体进同一个媒体池且与本地媒体只存一份() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);
    let 收本地之后 = 现场.catalog.pool_counts().expect("数得出");

    // **在线源回来的封面与本地那张一模一样**——池是内容寻址的，它落在同一个文件上。
    let fetcher = CannedFetcher::new()
        .with_prefix(查询端点, 200, 答复(true))
        .with(封面地址, 封面());
    let outcome = 刮削在线(&mut 现场, &fetcher, 宽松());

    let 之后 = 现场.catalog.pool_counts().expect("数得出");
    assert_eq!(之后.blobs, 收本地之后.blobs, "同一串字节，池里仍然只有一份");
    assert_eq!(
        之后.refs,
        收本地之后.refs + 1,
        "多的是一条引用，不是一份内容"
    );
    assert_eq!(outcome.new_blobs, 0);
    assert_eq!(outcome.deduped, 1, "算出来发现池里已经有了");

    let 作品媒体 = 现场.catalog.scraped_media("作品", 作品).expect("读得出");
    assert_eq!(作品媒体.len(), 1);
    assert_eq!(作品媒体[0].source, "ScreenScraper");
    assert_eq!(作品媒体[0].kind, "封面");
    // **封面挂在作品这一层**（`CONTEXT.md` 的「作品」词条）。
    let pool = MediaPool::open(现场.pool_dir.path()).expect("开得出池");
    assert!(pool.contains(&作品媒体[0].hash, "png"));

    // 下过的不重下：URL 记在库里，第二趟连那张图都不再取。
    let 之前问过 = fetcher.asked().len();
    刮削在线(&mut 现场, &fetcher, 宽松());
    assert_eq!(fetcher.asked().len(), 之前问过);
}

#[test]
fn 响应里指向禁区的媒体地址下不来() {
    let mut 现场 = 建现场();
    识别(&mut 现场);

    // **媒体 URL 是服务器说了算的。** 一条被改过的响应把它指到 datomatic，
    // 一次请求就够触发永久 IP 封禁（ADR-0007）——闸门是唯一挡得住这件事的东西。
    let 禁区 = "https://datomatic.no-intro.org/box.png";
    let 答复 = serde_json::to_vec(&serde_json::json!({
        "response": {"jeu": {
            "noms": [{"region": "wor", "text": "Contra"}],
            "medias": [{"type": "box-2D", "region": "wor", "url": 禁区, "format": "png"}]
        }}
    }))
    .expect("造得出 JSON");
    let fetcher = CannedFetcher::new()
        .with_prefix(查询端点, 200, 答复)
        .with(禁区, 封面());
    let outcome = 刮削在线(&mut 现场, &fetcher, 宽松());

    assert_eq!(
        outcome.online.expect("在线档该有这一笔账").refused,
        1,
        "该被闸门拦下"
    );
    // **闸门拦下不是「读不动」**（ADR-0021 的第三态说的是盘），也不是「源说没有」。
    // 三件事的处置各不相同，混进同一个计数器，报告就会指着一块好好的盘说它读不动。
    assert_eq!(outcome.unreadable_media, 0, "盘好好的");
    assert_eq!(outcome.missing_media, 0, "源没说这张图不存在，是我们不许下");
    assert!(
        !fetcher.asked().iter().any(|url| url.contains("no-intro")),
        "一个字节都不该发到禁区去：{:?}",
        fetcher.asked()
    );
    assert!(outcome.halted.is_none(), "拦下一张图不该让整趟停下来");
    // 拦下是**永久**的事，所以这一对照样算采全了——标题落了库，下一趟不必重问。
    assert_eq!(
        值(&现场, "作品", 作品, "标题", "ScreenScraper").as_deref(),
        Some("Contra")
    );
    assert!(
        现场
            .catalog
            .scraped_media("作品", 作品)
            .expect("读得出")
            .is_empty()
    );
}
