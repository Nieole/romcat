//! **标题这一折**：扫描 → 识别 → 刮削之后，标题集合是怎么攒起来的，显示标题与排序
//! 标题又是怎么挑出来的。
//!
//! 挑的规则本身挂在纯函数上（`title::choose` 有自己的单元测试），这里要证的是另外
//! 五件事——正是票 15 的验收里最容易只在测试里成立、在真库上不成立的那几条：
//!
//! 1. **标题真的以集合形式落库**，每条带语言、地区、来源与类型；
//! 2. **中文标题取官中版的官方译名，不是汉化版文件名**——而且这条链路**根本不看首选
//!    变体**（ADR-0012）；
//! 3. **世代裂缝两侧都走得通**（ADR-0019）：台版卡带是独立一条发行版，港服数字版是
//!    同一条发行版的语言属性；
//! 4. **排序标题独立生成**，中文条目排出来的顺序与按码位排不一样；
//! 5. **低置信的中文名照样给结论，但标记出来**，进得了待确认队列。
//!
//! fixture 的形状照真机来（`docs/library-facts.md`）：内容在**透明容器**里，官中版与
//! 汉化版各占一个目录，文件名是中文的——那正是这个库里中文名的唯一来源。

use std::fs;
use std::path::Path;

use romcat_core::catalog::Catalog;
use romcat_core::catalog::Roots;
use romcat_core::dat::Convention;
use romcat_core::dat::logiqx::{DatHeader, GameRecord, RomRecord};
use romcat_core::dat::repo::{DatMeta, DatRepo, Unit};
use romcat_core::fs::RealFs;
use romcat_core::identify;
use romcat_core::identify::fuzzy;
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::scrape::{self, Priorities};
use romcat_core::task::Handle;
use romcat_core::testing::container::{ZipEntrySpec, crc32, zip_container};
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::title::{self, Language, Seam, SortFrom, TitleKind};
use romcat_core::verdict;

/// 日版魂斗罗的字节。
fn 日版() -> Vec<u8> {
    vec![0xA1; 4_096]
}

/// **台版官中**的字节。它是一次**独立的官方发行**，有自己的 DAT 记录、自己的哈希。
fn 台版() -> Vec<u8> {
    vec![0xA2; 4_096]
}

/// 民间**汉化版**的字节。改过，No-Intro 政策上永不收录，只有 TOSEC 有。
fn 汉化版() -> Vec<u8> {
    vec![0xB2; 4_096]
}

/// 数字世代那一份：港服与美服**共用同一条记录**，所以只有一串字节。
fn 数字版() -> Vec<u8> {
    vec![0xC3; 2_048]
}

/// 一份没有任何中文名的东西，用来验回退链走到官方英文名那一档。
fn 只有英文() -> Vec<u8> {
    vec![0xD4; 1_024]
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

/// 把一条**中立库的键**折回盘上那条相对主库根的路径：剥掉第一段根名。
/// 摆 fixture 用它，断言用键本身——两者差的正是这一段（`path::library_key`）。
fn 相对(key: &str) -> &str {
    key.strip_prefix("库/").unwrap_or(key)
}

const 日版变体: &str = "库/FC/魂斗罗日版/Contra (Japan).zip";
const 台版变体: &str = "库/FC/魂斗罗台版/魂斗罗.zip";
const 汉化变体: &str = "库/FC/魂斗罗汉化/魂斗罗 中文版[dwt_so 汉化].zip";
const 数字美服变体: &str = "库/FC/Gaia/Gaia.zip";
const 数字港服变体: &str = "库/FC/盖亚/盖亚.zip";

fn 建现场() -> 现场 {
    let dir = temp_dir("titles");
    let root = dir.path();

    // ── 卡带世代：日版、**台版官中**、民间汉化版各占一个目录。
    写(
        &root.join(相对(日版变体)),
        &zip_container(&[ZipEntrySpec::stored("Contra.nes", 日版())]),
    );
    写(
        &root.join(相对(台版变体)),
        &zip_container(&[ZipEntrySpec::stored("Contra.nes", 台版())]),
    );
    写(
        &root.join(相对(汉化变体)),
        &zip_container(&[ZipEntrySpec::stored("魂斗罗.nes", 汉化版())]),
    );

    // ── 数字世代：**同一串字节**在盘上有两份，一份英文名一份中文名。
    // 它们撞上的是**同一条** DAT 记录——港服与美服共用同一个 TitleID 就是这个样子。
    for key in [数字美服变体, 数字港服变体] {
        写(
            &root.join(相对(key)),
            &zip_container(&[ZipEntrySpec::stored("Gaia.bin", 数字版())]),
        );
    }

    // ── 一部没有任何中文名的作品。
    写(
        &root.join("FC/Zelda/Zelda (USA).zip"),
        &zip_container(&[ZipEntrySpec::stored("Zelda.nes", 只有英文())]),
    );

    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::named(root, "库");
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");

    现场 {
        dir,
        pool_dir: temp_dir("titles-pool"),
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
    装(
        &mut repo,
        "No-Intro",
        "Nintendo - Nintendo Entertainment System",
        &[
            条目("Contra (Japan)", "Contra (Japan).nes", &日版()),
            // **官中在卡带世代是独立一条发行版**：自己的地区、自己的哈希、DAT 里独立
            // 一条，和日版平级（ADR-0012）。
            条目(
                "Contra (Taiwan) (En,Zh-Hant)",
                "Contra (Taiwan).nes",
                &台版(),
            ),
            // **数字世代**：地区是 `Asia`，中文只是这一条记录的**语言属性**——
            // 港服与美服共用它（ADR-0019）。
            条目("Gaia (Asia) (En,Zh,Ko)", "Gaia.bin", &数字版()),
            条目("Zelda (USA)", "Zelda (USA).nes", &只有英文()),
        ],
    );
    // TOSEC 收录汉化版，那正是官方数据库覆盖不到的那一块。
    装(
        &mut repo,
        "TOSEC",
        "TOSEC/Nintendo Famicom & Entertainment System - Games - [NES].dat",
        &[条目(
            "Contra (1988-02-09)(Konami)(JP)[tr zh dwt_so][v.20030208]",
            "Contra [tr zh].nes",
            &汉化版(),
        )],
    );
    repo
}

/// 跑一遍完整的管线：识别 → 刮削 → 折标题。
fn 跑一遍(现场: &mut 现场) -> title::TitleReport {
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
        &identify::Options::new(Roots::single("库", 现场.dir.path())),
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别不该失败");
    let mut options =
        scrape::Options::new(Roots::single("库", 现场.dir.path()), 现场.pool_dir.path());
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
            summaries: None,
            rulings: &scrape::zh::Rulings::none(),
        },
    )
    .expect("刮削不该失败");
    title::run(&mut 现场.catalog, &Priorities::builtin()).expect("折得出标题")
}

/// 一部作品挑出来的显示标题与排序标题。
fn 挑(现场: &现场, work: &str) -> title::Chosen {
    let set = title::TitleSet {
        work: work.to_string(),
        entries: 现场.catalog.titles_of(work).expect("读得出"),
    };
    title::choose(&set, &Priorities::builtin())
}

#[test]
fn 标题以集合形式落库每条带语言地区来源与类型() {
    let mut 现场 = 建现场();
    跑一遍(&mut 现场);

    let 集合 = 现场.catalog.titles_of("Contra").expect("读得出");
    assert!(
        集合.len() >= 4,
        "一部作品同时有好几个叫法，标题不是一个单值字段：{集合:#?}"
    );
    // 四个维度一个都不能少——少了哪个，跨格式转换与按中文搜索就做不了。
    for row in &集合 {
        assert!(!row.source.is_empty(), "每条叫法都要说得出是谁给的");
        assert!(!row.evidence.is_empty(), "没有依据的结论事后无法复核");
    }
    let 官中 = 集合
        .iter()
        .find(|row| row.value == "魂斗罗")
        .expect("台版官中那个中文名在集合里");
    assert_eq!(官中.language, Language::Chinese);
    assert_eq!(官中.kind, TitleKind::Translated);
    assert_eq!(官中.region.as_deref(), Some("Taiwan"));
    assert_eq!(官中.source, "文件名");

    let 汉化 = 集合
        .iter()
        .find(|row| row.value == "魂斗罗 中文版")
        .expect("汉化组自取的名照样入集合");
    assert_eq!(汉化.kind, TitleKind::FanName);
    assert_eq!(
        汉化.region, None,
        "汉化版是变体、没有发行版链接（ADR-0012），因此也没有地区"
    );

    // 官方名从**发行版**上来，日版那条的名字是日文原名而不是官方英文名。
    let 日版名 = 集合
        .iter()
        .find(|row| row.kind == TitleKind::Official && row.region.as_deref() == Some("Japan"))
        .expect("日版那条发行版的官方名在集合里");
    assert_eq!(日版名.language, Language::Japanese);
    assert_eq!(日版名.value, "Contra");
}

#[test]
fn 中文标题取官中的官方译名而不是汉化版文件名() {
    // **这一条是这张票最容易做错的地方。** 按「汉化 > 官中 > 日版 > 其他」，
    // 这部作品的**首选变体**是那个汉化版；而标题这条链路走的是反方向——
    // `title::choose` 拿到的只有标题集合，它连哪个变体是首选都看不见。
    let mut 现场 = 建现场();
    跑一遍(&mut 现场);

    let chosen = 挑(&现场, "Contra");
    assert_eq!(chosen.display, "魂斗罗", "中文标题该取官中版的官方译名");
    assert_eq!(chosen.kind, Some(TitleKind::Translated));
    assert_ne!(
        chosen.display, "魂斗罗 中文版",
        "绝不用汉化版文件名里的名字当标题（ADR-0012）"
    );

    // 汉化版的名字**在集合里**，只是排在中文那一档的最后——官中不在了它才轮得上。
    assert!(
        现场
            .catalog
            .titles_of("Contra")
            .expect("读得出")
            .iter()
            .any(|row| row.value == "魂斗罗 中文版"),
        "汉化组自取的名不是被扔掉了，是排在后面"
    );
    assert!(chosen.chinese_names >= 2, "这部作品有不止一个中文叫法");
}

#[test]
fn 世代裂缝两侧都走得通() {
    let mut 现场 = 建现场();
    let report = 跑一遍(&mut 现场);

    // 卡带世代：官中是**独立一条发行版**，中文名从那一条上取。
    let 卡带 = 挑(&现场, "Contra");
    assert_eq!(卡带.seam, Some(Seam::OwnRelease));
    assert!(
        卡带.evidence.contains("官中那一条"),
        "依据要说得出是从哪一条发行版上取的：{}",
        卡带.evidence
    );

    // 数字世代：中文是**同一条发行版的语言属性**，两个变体撞的是同一条记录。
    let 数字 = 挑(&现场, "Gaia");
    assert_eq!(数字.display, "盖亚");
    assert_eq!(数字.seam, Some(Seam::LanguageField));
    assert!(
        数字.evidence.contains("语言属性"),
        "依据要说得出中文是这条发行版的一项属性：{}",
        数字.evidence
    );

    let 美服 = 现场
        .catalog
        .variant(数字美服变体)
        .expect("读得出")
        .expect("在");
    let 港服 = 现场
        .catalog
        .variant(数字港服变体)
        .expect("读得出")
        .expect("在");
    assert_eq!(
        美服.release_id, 港服.release_id,
        "数字世代港服与美服共用同一条发行版——这正是不能为中文另造一条的理由"
    );

    // 报告把两侧分开数，抹平任何一侧都会在那一侧撒谎。
    let 两侧: Vec<&str> = report
        .chinese_by_seam
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(两侧, vec!["独立发行版", "语言属性"]);
}

#[test]
fn 显示标题按中文英文日文的顺序回退() {
    let mut 现场 = 建现场();
    跑一遍(&mut 现场);

    // 有中文就用中文。
    assert_eq!(挑(&现场, "Contra").display, "魂斗罗");
    // 没有中文名的作品退到官方英文名。
    let zelda = 挑(&现场, "Zelda");
    assert_eq!(zelda.display, "Zelda");
    assert_eq!(zelda.language, Language::English);
    assert_eq!(zelda.kind, Some(TitleKind::Official));
}

#[test]
fn 排序标题独立生成中文条目排得对() {
    let mut 现场 = 建现场();
    跑一遍(&mut 现场);

    let mut 三部: Vec<title::Chosen> = ["Contra", "Gaia", "Zelda"]
        .into_iter()
        .map(|work| 挑(&现场, work))
        .collect();

    // 按**显示标题**的码位排：`盖`(U+76D6) 在 `魂`(U+9B42) 前面——那是乱排。
    let mut 按码位: Vec<String> = 三部.iter().map(|c| c.display.clone()).collect();
    按码位.sort();
    assert_eq!(按码位, vec!["Zelda", "盖亚", "魂斗罗"]);

    // 按**排序标题**排：Contra < Gaia < Zelda，这才是读者预期的顺序。
    三部.sort_by(|a, b| a.sort.cmp(&b.sort));
    assert_eq!(
        三部.iter().map(|c| c.display.as_str()).collect::<Vec<_>>(),
        vec!["魂斗罗", "盖亚", "Zelda"]
    );
    assert_eq!(三部[0].sort, "CONTRA");
    assert_eq!(三部[0].sort_from, SortFrom::LatinTitle);
    assert_eq!(三部[2].sort_from, SortFrom::Display);
}

#[test]
fn 中文名带置信度低的那些进得了队列() {
    let mut 现场 = 建现场();
    let report = 跑一遍(&mut 现场);

    // 官中那条是**中置信**的：精确哈希确认了「这个变体就是那条官中发行版」，
    // 但**这串字是不是官方译名**没人确认过——官方译名不在 DAT 里，中文名只写在盘上
    // 那个文件的名字上。高置信留给裁决。
    let 官中 = 挑(&现场, "Contra");
    assert_eq!(
        官中.confidence,
        Some(romcat_core::catalog::Confidence::Medium)
    );

    // 再放两个「盘上叫这个名字、但没有任何官中发行版背书」的变体进来。
    写(
        &现场.dir.path().join("FC/合集/魂斗罗全集.zip"),
        &zip_container(&[ZipEntrySpec::stored("Contra.nes", 日版())]),
    );
    写(
        &现场.dir.path().join("FC/合集/塞尔达全集.zip"),
        &zip_container(&[ZipEntrySpec::stored("Zelda.nes", 只有英文())]),
    );
    let mut options = ScanOptions::named(现场.dir.path(), "库");
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut 现场.catalog, &options, &Handle::new()).expect("扫得动");
    let report2 = 跑一遍(&mut 现场);

    assert!(
        report2.queue_works > report.queue_works,
        "没人背书的中文名要进队列：{} → {}",
        report.queue_works,
        report2.queue_works
    );
    let 别名 = 现场
        .catalog
        .titles_of("Contra")
        .expect("读得出")
        .into_iter()
        .find(|row| row.value == "魂斗罗全集")
        .expect("那个合集包的名字照样入集合");
    assert_eq!(别名.kind, TitleKind::Alias);
    assert_eq!(别名.confidence, romcat_core::catalog::Confidence::Low);
    // **照用但标记**：它没有把官中的译名挤掉。
    assert_eq!(挑(&现场, "Contra").display, "魂斗罗");

    // 而本来只有英文名的那部作品，现在拿到了一个中文名——**照用**（前端里多一条
    // 中文），**但标记**：它是低置信的，队列里躺着等人裁决。合集包的名字本来就不该
    // 当成一部作品的中文名，而这一点只有人看得出来。
    let zelda = 挑(&现场, "Zelda");
    assert_eq!(zelda.display, "塞尔达全集");
    assert_eq!(zelda.language, Language::Chinese);
    assert_eq!(
        zelda.confidence,
        Some(romcat_core::catalog::Confidence::Low)
    );
    assert!(
        report2
            .queue_examples
            .iter()
            .any(|example| example.display == "塞尔达全集"),
        "队列里要说得出是哪一条、凭什么"
    );
}

#[test]
fn 重折不冲掉裁决而且重跑识别之后标题还认得回来() {
    let mut 现场 = 建现场();
    跑一遍(&mut 现场);

    // 人工定一个标题（票 08 才有界面，这里直接写一行——形状是一样的）。
    现场
        .catalog
        .put_titles(&[romcat_core::catalog::TitleRow {
            work: "Contra".to_string(),
            value: "魂斗羅".to_string(),
            language: Language::Chinese,
            kind: TitleKind::Translated,
            source: romcat_core::scrape::priority::VERDICT.to_string(),
            region: None,
            variant_key: None,
            confidence: romcat_core::catalog::Confidence::High,
            seam: None,
            evidence: "人说的".to_string(),
            seen: 1,
        }])
        .expect("写得进");
    assert_eq!(挑(&现场, "Contra").display, "魂斗羅", "人工来源排在最前");

    // 重跑一遍识别：发行版整批换掉、行号全变；作品那一行按名字复用（票 parking-3/10）。
    // 标题挂在**作品名**上，两条路都活得下来。
    跑一遍(&mut 现场);
    assert_eq!(
        挑(&现场, "Contra").display,
        "魂斗羅",
        "重折不许把人工定下来的叫法冲掉"
    );
    assert!(
        现场
            .catalog
            .titles_of("Contra")
            .expect("读得出")
            .iter()
            .any(|row| row.value == "魂斗罗"),
        "折出来的那些照样重新折了一遍"
    );
}
