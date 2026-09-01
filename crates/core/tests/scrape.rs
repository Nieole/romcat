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
        &现场.repo,
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
        &mut scrape::RunContext {
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
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
    assert!(text.contains("离线档补不上的字段"));
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
        &mut scrape::RunContext {
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
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
