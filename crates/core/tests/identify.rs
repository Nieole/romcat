//! **识别接缝**：磁盘上摆一份主库、手边一份迷你 DAT，跑一遍第一命中层。
//!
//! 规格里点名的第一条接缝就是它（`spec.md`「接缝一 — 识别」）：输入是变体的字节与
//! 已加载的 DAT，输出是带**置信度**与**依据**的**候选**。密集逻辑挂在纯函数上
//! （`identify::header` / `fingerprint` / `scope` / `naming` 各有自己的单元测试），
//! 这里要证的是另一件事：那几条纯函数真的接在了扫描、DAT 库与中立库之间。
//!
//! fixture 的形状照真机来（`docs/library-facts.md`）：库里 91.1% 的容量在**透明容器**
//! 里，所以主力路径是「零解压读容器里的 CRC-32」；`.smc` 那一份带 512 字节拷贝机头，
//! 只有去头哈希撞得上 No-Intro；`补丁` 与 `AIME00001` 那两份真库里就躺着。

use std::fs;
use std::path::Path;

use romcat_core::catalog::identify::State;
use romcat_core::catalog::{Catalog, Confidence, Roots};
use romcat_core::dat::Convention;
use romcat_core::dat::logiqx::{DatHeader, GameRecord, RomRecord};
use romcat_core::dat::repo::{DatMeta, DatRepo, Unit};
use romcat_core::fs::{MemFs, RealFs};
use romcat_core::identify::fuzzy;
use romcat_core::identify::report::IdentifyReport;
use romcat_core::identify::{self, Options};
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::task::Handle;
use romcat_core::testing::container::{ZipEntrySpec, crc32, zip_container};
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::verdict;

/// 一份带 iNES 头的 FC 卡带：前 16 字节是外挂头，后面才是内容。
fn ines(fill: u8, payload: usize) -> Vec<u8> {
    let mut data = vec![0u8; 16];
    data[..4].copy_from_slice(b"NES\x1A");
    data[4] = 2;
    data[5] = 1;
    data.extend(std::iter::repeat_n(fill, payload));
    data
}

/// 一份带 512 字节拷贝机头的 SFC 卡带。拷贝机头**没有魔数**，判据是总长 % 1024 == 512。
fn smc(fill: u8, payload: usize) -> Vec<u8> {
    let mut data = vec![0u8; 512];
    data.extend(std::iter::repeat_n(fill, payload));
    data
}

/// 一份 NKit 处理过的 Wii 镜像：光盘逻辑偏移 0x200 处写着 `NKIT`。
fn nkit_iso() -> Vec<u8> {
    let mut data = vec![0x33u8; 0x400];
    data[0x200..0x204].copy_from_slice(b"NKIT");
    data
}

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

struct 现场 {
    dir: TempDir,
    catalog: Catalog,
    repo: DatRepo,
}

/// 原版 FC 卡带（含头 40,976 字节），No-Intro 按去头收、TOSEC 按含头收。
fn 原版() -> Vec<u8> {
    ines(0xA1, 40_960)
}

/// 一份汉化版 FC 卡带：字节改过，只有 TOSEC 的 `[tr zh]` 条目收得到它。
fn 汉化版() -> Vec<u8> {
    ines(0xB2, 40_960)
}

/// 一份带拷贝机头的 SFC 卡带。
fn 拷贝机头版() -> Vec<u8> {
    smc(0xC3, 32_768)
}

fn 建现场() -> 现场 {
    let dir = temp_dir("identify");
    let root = dir.path();

    // ── FC：三个变体，全在透明容器里（真库 91.1% 的容量是这个形态）
    写(
        &root.join("FC/超级马里奥.zip"),
        &zip_container(&[ZipEntrySpec::stored("Super Mario (Japan).nes", 原版())]),
    );
    写(
        &root.join("FC/某游戏 汉化版.zip"),
        &zip_container(&[ZipEntrySpec::deflated("某游戏(汉化).nes", 汉化版())]),
    );
    写(
        &root.join("FC/谁也不认得.zip"),
        &zip_container(&[ZipEntrySpec::stored("陌生.nes", ines(0xEE, 4_096))]),
    );

    // ── SFC：带拷贝机头的裸文件。只有**去头**那套撞得上 No-Intro。
    写(&root.join("SFC/带拷贝机头的.smc"), &拷贝机头版());

    // ── WII：NKit 处理过的镜像，CRC 与 Redump 那条一模一样（Dolphin 说的正是这件事）
    写(&root.join("wii/某游戏.iso"), &nkit_iso());
    // 同一份东西再装进一个容器：容器里的 CRC-32 零解压就有，**但 NKit 只能读字节才验得出**。
    写(
        &root.join("wii/装进包里的.zip"),
        &zip_container(&[ZipEntrySpec::stored("某游戏.iso", nkit_iso())]),
    );

    // ── 两类不该撞 DAT 的东西，真库里就躺着这两份形态
    写(
        &root.join("PSV/《某游戏》汉化补丁 Ver.1.0.zip"),
        &zip_container(&[
            ZipEntrySpec::stored("某游戏.ppf", vec![1u8; 128]),
            ZipEntrySpec::stored("使用说明.txt", "先备份".as_bytes().to_vec()),
        ]),
    );
    写(
        &root.join("PSV/AIME00001(某同人移植).zip"),
        &zip_container(&[ZipEntrySpec::stored("game.vpk", vec![2u8; 64])]),
    );

    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::named(root, "库");
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");

    现场 {
        dir,
        catalog,
        repo: 建_dat(),
    }
}

fn 条目(name: &str, rom: &str, size: u64, crc: u32) -> GameRecord {
    GameRecord {
        name: name.to_string(),
        roms: vec![RomRecord {
            name: rom.to_string(),
            size: Some(size),
            crc32: Some(crc),
            ..RomRecord::default()
        }],
        ..GameRecord::default()
    }
}

fn 装(
    repo: &mut DatRepo,
    source: &str,
    dat: &str,
    platform: &str,
    convention: Convention,
    games: &[GameRecord],
) {
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
                platform: platform.to_string(),
                convention,
                header: DatHeader::default(),
            },
            games,
        )
        .expect("写得进");
    writer.commit().expect("提交");
}

fn 建_dat() -> DatRepo {
    let mut repo = DatRepo::in_memory().expect("开得出来");
    let 原 = 原版();
    let 汉 = 汉化版();
    let 拷 = 拷贝机头版();

    // No-Intro 的 FC headerless 集：按**去头**记。含头那套永远撞不上它。
    装(
        &mut repo,
        "No-Intro",
        "Nintendo - Nintendo Entertainment System (Headerless)",
        "FC",
        Convention::Headerless,
        &[条目(
            "Super Mario Bros. (Japan)",
            "Super Mario Bros. (Japan).unh",
            40_960,
            crc32(&原[16..]),
        )],
    );
    // TOSEC 的 FC 集：按**含头**原样记，而且汉化条目只有这里有。
    装(
        &mut repo,
        "TOSEC",
        "Nintendo Famicom - Games",
        "FC",
        Convention::AsIs,
        &[
            条目(
                "Super Mario Bros. (1985)(Nintendo)",
                "Super Mario Bros. (1985)(Nintendo).nes",
                40_976,
                crc32(&原),
            ),
            条目(
                "Some Game (1990)(Someone)(zh)[tr zh 某汉化组]",
                "Some Game [tr zh].nes",
                40_976,
                crc32(&汉),
            ),
        ],
    );
    // No-Intro 的 SFC 集只有一套，而且是**去头**的——带拷贝机头的 `.smc` 只能靠去头哈希。
    装(
        &mut repo,
        "No-Intro",
        "Nintendo - Super Nintendo Entertainment System",
        "SFC",
        Convention::Headerless,
        &[条目(
            "Some SFC Game (Japan)",
            "Some SFC Game (Japan).sfc",
            32_768,
            crc32(&拷[512..]),
        )],
    );
    // Redump 的 Wii：CRC 与那份 NKit 镜像一模一样——Dolphin 说的正是这件事。
    装(
        &mut repo,
        "Redump",
        "Nintendo - Wii",
        "WII",
        Convention::AsIs,
        &[条目(
            "Some Wii Game (Japan)",
            "Some Wii Game.iso",
            0x400,
            crc32(&nkit_iso()),
        )],
    );
    repo
}

fn 跑(现场: &mut 现场) -> identify::Outcome {
    let options = Options::new(Roots::single("库", 现场.dir.path()));
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
        &options,
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别不该失败")
}

fn 结论(现场: &现场, key: &str) -> (State, Option<String>) {
    现场
        .catalog
        .identification_of(key)
        .expect("读得出")
        .unwrap_or_else(|| panic!("{key} 没有识别结论"))
}

/// 完整重扫一遍这个根。**删除与成型都只在完整扫完一遍之后才做**，所以要走整条流程，
/// 不能只写几条记录（ADR-0022）。
fn 重扫(现场: &mut 现场) {
    let mut options = ScanOptions::named(现场.dir.path(), "库");
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut 现场.catalog, &options, &Handle::new()).expect("扫得动");
}

#[test]
fn 含头与去头两套规则同时算并且各撞各的() {
    let mut 现场 = 建现场();
    跑(&mut 现场);

    // 含头那套：零解压从容器元数据里就拿得到，撞上 TOSEC。
    // 去头那套：剥掉 16 字节 iNES 头之后才撞得上 No-Intro 的 headerless 集。
    // **一个变体两条候选**，两套口径各一条。
    let 候选 = 现场
        .catalog
        .candidates_of("库/FC/超级马里奥.zip")
        .expect("读得出");
    assert_eq!(候选.len(), 2, "两套口径各撞上一条：{候选:#?}");
    let 去头 = 候选
        .iter()
        .find(|c| c.hashed_as == Convention::Headerless)
        .expect("去头那套撞上了 No-Intro");
    assert_eq!(去头.source, "No-Intro");
    assert_eq!(去头.dat_convention, Convention::Headerless);
    assert!(
        去头.evidence.contains("剥掉了 iNES 16 字节头"),
        "{}",
        去头.evidence
    );
    let 含头 = 候选
        .iter()
        .find(|c| c.hashed_as == Convention::AsIs)
        .expect("含头那套撞上了 TOSEC");
    assert_eq!(含头.source, "TOSEC");

    // 只算含头那一套的话，No-Intro 那条永远撞不上；只算去头那套，TOSEC 全部汉化条目
    // 一条都撞不上。两条都在，才说明两套规则真的都跑了。
    assert_eq!(结论(&现场, "库/FC/超级马里奥.zip").0, State::Matched);
}

#[test]
fn 每条候选都带置信度与依据() {
    let mut 现场 = 建现场();
    跑(&mut 现场);
    let 候选 = &现场
        .catalog
        .candidates_of("库/FC/超级马里奥.zip")
        .expect("读得出")[0];

    assert_eq!(候选.confidence, Confidence::High, "精确命中是高置信");
    // 依据要答得出「命中了哪个数据库的哪条记录、匹配了哪个字段」（ADR-0002）。
    assert!(候选.evidence.contains("No-Intro"), "{}", 候选.evidence);
    assert!(
        候选.evidence.contains("Super Mario Bros. (Japan)"),
        "{}",
        候选.evidence
    );
    assert!(候选.evidence.contains("CRC-32"), "{}", 候选.evidence);
    assert!(候选.evidence.contains("大小"), "{}", 候选.evidence);
    assert_eq!(候选.member_key, "库/FC/超级马里奥.zip");
    assert_eq!(
        候选.inner, "Super Mario (Japan).nes",
        "包里的哪一个也说得出"
    );
}

#[test]
fn 精确命中的候选自动通过() {
    let mut 现场 = 建现场();
    let outcome = 跑(&mut 现场);
    let 候选 = 现场
        .catalog
        .candidates_of("库/FC/超级马里奥.zip")
        .expect("读得出");
    assert!(候选.iter().all(|c| c.accepted), "精确命中不必人工介入");
    assert!(outcome.report.accepted > 0);
}

#[test]
fn 识别结果在中立库里建起作品与发行版并让变体指向它() {
    let mut 现场 = 建现场();
    跑(&mut 现场);

    let 变体 = 现场
        .catalog
        .variant("库/FC/超级马里奥.zip")
        .expect("读得出")
        .expect("在");
    let work = 变体.work_id.expect("挂上了作品");
    let release = 变体.release_id.expect("指向它所基于的发行版");
    assert!(
        现场
            .catalog
            .releases_of(work)
            .expect("读得出")
            .contains(&release),
        "发行版挂在那个作品下"
    );

    // **汉化版条目是变体不是发行版**（ADR-0012）：认得出是哪部作品，认不出它基于
    // 哪一条发行版，那就只挂作品、发行版留空，等裁决补。
    let 汉化 = 现场
        .catalog
        .variant("库/FC/某游戏 汉化版.zip")
        .expect("读得出")
        .expect("在");
    assert_eq!(结论(&现场, "库/FC/某游戏 汉化版.zip").0, State::Matched);
    assert!(汉化.work_id.is_some(), "汉化版认得出是哪部作品");
    assert_eq!(汉化.release_id, None, "但它不是一次官方发行");
}

#[test]
fn 带拷贝机头的裸文件靠去头哈希撞上() {
    // SFC 只有一套 DAT 而且是去头的：只算含头那套，整个平台全落空。
    let mut 现场 = 建现场();
    跑(&mut 现场);
    assert_eq!(结论(&现场, "库/SFC/带拷贝机头的.smc").0, State::Matched);
    let 候选 = &现场
        .catalog
        .candidates_of("库/SFC/带拷贝机头的.smc")
        .expect("读得出")[0];
    assert_eq!(候选.hashed_as, Convention::Headerless);
    assert!(
        候选.evidence.contains("SFC 拷贝机 512 字节头"),
        "{}",
        候选.evidence
    );
}

#[test]
fn nkit_验在撞_crc_之前撞上了也不许自动通过() {
    // Dolphin 原话：这个文件的 CRC32 可能和好转储的相同，即使两个文件并不完全一样。
    let mut 现场 = 建现场();
    let outcome = 跑(&mut 现场);
    let 候选 = &现场
        .catalog
        .candidates_of("库/wii/某游戏.iso")
        .expect("读得出")[0];
    assert_eq!(候选.confidence, Confidence::Medium, "降一档");
    assert!(!候选.accepted, "不许自动通过");
    assert!(候选.evidence.contains("NKit"), "{}", 候选.evidence);
    assert_eq!(
        outcome.report.nkit, 2,
        "报告数得出来：裸的那份与装进包里的那份"
    );

    // 降档了但**候选照样在**——它确实撞上了那条记录，只是不能自动认账。
    assert_eq!(结论(&现场, "库/wii/某游戏.iso").0, State::Matched);
}

#[test]
fn 验不了_nkit_的_gc_与_wii_镜像不许自动通过() {
    // 容器里那套 CRC-32 是零解压白拿的，**而 NKit 要读字节才验得出**。不读盘时
    // 两者都在：撞得上，但验不了——这时候绝不能自动认账，Dolphin 说过它的 CRC32
    // 可能与好转储相同而内容不同。判据取自**撞上的那条记录说它是 GC / Wii 的光盘**，
    // 不是取自目录名（ADR-0011：目录只是强先验）。
    let mut 现场 = 建现场();
    let mut options = Options::new(Roots::single("库", 现场.dir.path()));
    options.read_library = false;
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
        &options,
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别不该失败");

    let 候选 = &现场
        .catalog
        .candidates_of("库/wii/装进包里的.zip")
        .expect("读得出")[0];
    assert_eq!(候选.confidence, Confidence::Medium);
    assert!(!候选.accepted, "没验过 NKit 就不敢自动通过");
    assert!(候选.evidence.contains("没验过 NKit"), "{}", 候选.evidence);

    // 读得了盘的那一趟，同一个变体验得出来、也照样不自动通过（它真是 NKit）。
    跑(&mut 现场);
    let 候选 = &现场
        .catalog
        .candidates_of("库/wii/装进包里的.zip")
        .expect("读得出")[0];
    assert!(!候选.accepted);
    assert!(候选.evidence.contains("NKit 处理过的"), "{}", 候选.evidence);
}

#[test]
fn 逐芯片的命中通过但标记不自动过() {
    // MAME 的一条 `rom` 是**一颗芯片**的内容，不是一个文件。对上一颗芯片不等于
    // 对上整次发行——多芯片卡上另一颗可能根本不在这个文件里。
    let mut 现场 = 建现场();
    装(
        &mut 现场.repo,
        "MAME",
        "nes.xml",
        "FC",
        Convention::PerChip,
        &[条目("smb", "prg", 40_960, crc32(&原版()[16..]))],
    );
    跑(&mut 现场);
    let 候选 = 现场
        .catalog
        .candidates_of("库/FC/超级马里奥.zip")
        .expect("读得出");
    let 芯片 = 候选
        .iter()
        .find(|c| c.source == "MAME")
        .expect("MAME 那条也在");
    assert_eq!(芯片.confidence, Confidence::Medium);
    assert!(!芯片.accepted, "逐芯片不自动通过");
    assert!(芯片.evidence.contains("逐芯片"), "{}", 芯片.evidence);
}

/// 给那份「谁也不认得」的 zip 摆两条候选，**可信程度与源的先后故意反着来**：
/// 高置信那条出自源那一列排第五的 GoodNES，中置信那条出自排头一个的 No-Intro。
///
/// 反着摆才验得出「先按可信程度排」这句话：顺着摆的话，两种排法给出同一个次序，
/// 那条断言就是恒真的。
fn 装_两条可信程度与源反着来的候选(现场: &mut 现场) {
    let 陌生 = ines(0xEE, 4_096);
    // **中置信那条先写进去**：不这么摆的话，「按可信程度排」与「照写入顺序原样交回」
    // 给出同一个次序，那条断言就分不出排序在不在。
    装(
        &mut 现场.repo,
        "No-Intro",
        "Nintendo - Nintendo Entertainment System (Headered)",
        "FC",
        Convention::AsIs,
        // 这份 DAT **没记大小**，只凭 CRC-32 撞上——通过但标记，中置信（ADR-0002）。
        &[GameRecord {
            name: "Unknown Thing".to_string(),
            roms: vec![RomRecord {
                name: "unknown.nes".to_string(),
                size: None,
                crc32: Some(crc32(&陌生)),
                ..RomRecord::default()
            }],
            ..GameRecord::default()
        }],
    );
    装(
        &mut 现场.repo,
        "GoodNES",
        "GoodNES 3.23",
        "FC",
        Convention::AsIs,
        &[条目(
            "Unknown Thing [T+Chi]",
            "unknown.nes",
            陌生.len() as u64,
            crc32(&陌生),
        )],
    );
}

#[test]
fn 中立库交回候选的次序就是按可信程度排的那一个() {
    // **这条钉的是待确认屏一级分批的键。** 分批取的是**第一条候选**
    // （`triage::batch::Shape::of`——「整批通过」就是 `--pick 1`，采用的正是第一条），
    // 而候选从中立库出来是**按写入顺序**（`candidates_of` 的 `ORDER BY id`），
    // 也就是识别当初产出它们的顺序。于是「第一条就是最可信的那条」这句话，
    // 整条链上只由 `identify` 那一次排序担着——写入这一侧再没有第二道闸（挂单 Q82）。
    let mut 现场 = 建现场();
    装_两条可信程度与源反着来的候选(&mut 现场);
    跑(&mut 现场);

    let 候选 = 现场
        .catalog
        .candidates_of("库/FC/谁也不认得.zip")
        .expect("读得出");
    assert_eq!(
        候选
            .iter()
            .map(|one| (one.source.as_str(), one.confidence))
            .collect::<Vec<_>>(),
        vec![
            ("GoodNES", Confidence::High),
            ("No-Intro", Confidence::Medium),
        ],
        "**可信程度压过源的先后**，而中立库按写入顺序原样交回：{候选:#?}",
    );
    assert!(候选[0].accepted, "第一条正是自动通过的那条");
    assert!(
        候选[1].evidence.contains("这份 DAT 没记大小"),
        "{}",
        候选[1].evidence
    );
}

#[test]
fn 补丁与没有发行版链接的变体不去撞_dat() {
    let mut 现场 = 建现场();
    let outcome = 跑(&mut 现场);

    let (state, reason) = 结论(&现场, "库/PSV/《某游戏》汉化补丁 Ver.1.0.zip");
    assert_eq!(state, State::Skipped);
    assert!(reason.unwrap_or_default().contains("补丁"));
    assert!(
        现场
            .catalog
            .candidates_of("库/PSV/《某游戏》汉化补丁 Ver.1.0.zip")
            .expect("读得出")
            .is_empty(),
        "跳过的不产生候选"
    );

    let (state, reason) = 结论(&现场, "库/PSV/AIME00001(某同人移植).zip");
    assert_eq!(state, State::Skipped);
    assert!(reason.unwrap_or_default().contains("没有发行版链接"));

    // **被跳过的单独计数，不混进未命中**——混进去命中率就失真了。
    assert_eq!(outcome.report.total.skipped, 2);
    let 跳过 = &outcome.report.skipped;
    assert_eq!(跳过.iter().map(|row| row.count).sum::<u64>(), 2);
    assert!(跳过.iter().any(|row| row.reason == "补丁"));
    assert!(跳过.iter().any(|row| row.reason == "没有发行版链接"));
}

#[test]
fn 报告给出每个平台的命中率与未命中数() {
    let mut 现场 = 建现场();
    let outcome = 跑(&mut 现场);
    let report = &outcome.report;

    let fc = report
        .platforms
        .iter()
        .find(|row| row.platform == "FC")
        .expect("有 FC");
    assert_eq!(fc.variants, 3);
    assert_eq!(fc.matched, 2);
    assert_eq!(fc.unmatched, 1, "那份谁也不认得的 zip");
    assert!(
        (fc.hit_rate() - 66.6).abs() < 0.2,
        "命中率 {:.1}%",
        fc.hit_rate()
    );
    assert!(fc.dat_games > 0, "报告顺带说得出这个平台有多少弹药");

    // 跳过与无判据都不在命中率的分母里。
    assert_eq!(report.total.matched + report.total.unmatched, 6);
    assert_eq!(report.total.skipped, 2);

    let text = report.render_text();
    assert!(text.contains("命中率"), "{text}");
    assert!(text.contains("未命中"), "{text}");
    assert!(text.contains("FC"), "{text}");
}

#[test]
fn 第二趟不再读一遍盘() {
    // 挂账 D14：增量的价值在这票才兑现——算过的哈希留在中立库里，
    // 第二趟识别一个字节都不该再读。
    let mut 现场 = 建现场();
    let 第一趟 = 跑(&mut 现场);
    assert!(第一趟.read_bytes > 0, "第一趟要读裸文件与要去头的那几份");

    let 第二趟 = 跑(&mut 现场);
    assert_eq!(第二趟.read_bytes, 0, "第二趟一个字节都不读");
    assert!(第二趟.reused_hashes > 0, "哈希是从中立库里取回来的");
    assert_eq!(
        第二趟.report.total.matched, 第一趟.report.total.matched,
        "结论一模一样"
    );
}

#[test]
fn 识别一个字节都不写主库() {
    // 主库只读（ADR-0004）。识别要读字节才能算去头哈希，那就更得钉住这一条。
    let mut 现场 = 建现场();
    let before = 快照(现场.dir.path());
    跑(&mut 现场);
    assert_eq!(快照(现场.dir.path()), before, "主库一个字节都没变");
}

#[test]
fn 不读主库时容器里那套零解压的_crc32_照撞() {
    // 盘不在位、或者不想为去头那套付读盘的钱时：容器里的 CRC-32 零解压就有。
    let mut 现场 = 建现场();
    let mut options = Options::new(Roots::single("库", 现场.dir.path()));
    options.read_library = false;
    let outcome = identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &identify::Ammo {
            repo: &现场.repo,
            verdicts: &verdict::Index::empty(),
            naming: &fuzzy::Naming::off(),
            guessing: &identify::model::Guessing::off(),
            titledb: None,
        },
        &options,
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别不该失败");

    assert_eq!(outcome.read_bytes, 0, "一个字节都没读");
    // 含头那套照样撞上 TOSEC。
    assert_eq!(结论(&现场, "库/FC/超级马里奥.zip").0, State::Matched);
    // 裸文件没有判据可用——报「无判据」，不是「未命中」。
    assert_eq!(结论(&现场, "库/SFC/带拷贝机头的.smc").0, State::NoEvidence);
}

/// 主库里每个文件的 `(路径, 大小, 修改时间)`。
fn 快照(root: &Path) -> Vec<(String, u64, std::time::SystemTime)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("列得出") {
            let entry = entry.expect("读得出");
            let meta = entry.metadata().expect("读得出");
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                out.push((
                    entry.path().to_string_lossy().into_owned(),
                    meta.len(),
                    meta.modified().expect("读得出"),
                ));
            }
        }
    }
    out.sort();
    out
}

// ───────── 盘上的名字是分解形式：回盘读那一趟不能落成「无判据」 ─────────
//
// 中立库的键一律是 NFC（ADR-0020），而主库里 1.99% 的名字在盘上是**分解形式**。
// 在**分解敏感**的文件系统上（Windows 的 NTFS、Linux 的 ext4——ADR-0018 说主力机
// 是 Windows），拿 NFC 的键直接拼出来的那条路径根本开不了，而识别把「开不了」读成
// **无判据**：几百个变体从此认不出来，报告里说的却是「拿不到可撞的东西」。

/// 同一个名字的两种规范化形式。`ゲ` 预组合 vs `ケ` + 组合浊音符（U+3099）。
const 预组合名: &str = "ゲーム.nes";
const 分解形名: &str = "\u{30b1}\u{3099}ーム.nes";

/// 一份**分解敏感**的主库：[`MemFs`] 按字节精确认路径，正是 NTFS 与 ext4 的样子。
/// macOS 的 fskit 驱动查找**不**分解敏感（ADR-0020 修订段实测 383/383 两种形式都开得了），
/// 所以真机上今天看不见这条——这份 fixture 是它唯一能在 macOS 上变红的地方。
fn 建一份分解形名字的主库() -> (MemFs, Catalog) {
    let mut library = MemFs::new();
    library.file(format!("/lib/FC/{分解形名}"), 原版());
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    scan::scan(
        &library,
        &mut catalog,
        &ScanOptions::named("/lib", "库"),
        &Handle::new(),
    )
    .expect("扫得动");
    (library, catalog)
}

fn 跑一趟(library: &MemFs, catalog: &mut Catalog, repo: &DatRepo) {
    identify::run(
        library,
        catalog,
        &identify::Ammo {
            repo,
            verdicts: &verdict::Index::empty(),
            naming: &fuzzy::Naming::off(),
            guessing: &identify::model::Guessing::off(),
            titledb: None,
        },
        &Options::new(Roots::single("库", "/lib")),
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别不该失败");
}

#[test]
fn 盘上的名字是分解形式时照样读得到字节而不是落成无判据() {
    let (library, mut catalog) = 建一份分解形名字的主库();
    let 键 = format!("库/FC/{预组合名}");

    // 前提：键是 NFC 的那一份，与盘上那串字节**不同**。
    assert_ne!(预组合名, 分解形名, "两串字节本来就不一样");
    assert!(
        catalog.identification_of(&键).expect("读得出").is_none(),
        "跑之前这条还没识别",
    );

    跑一趟(&library, &mut catalog, &建_dat());

    let (结论, 为什么) = catalog
        .identification_of(&键)
        .expect("读得出")
        .unwrap_or_else(|| panic!("{键} 没有识别结论"));
    assert_eq!(
        结论,
        State::Matched,
        "分解形式的名字照样该读得到字节：{为什么:?}",
    );
    let 候选 = catalog.candidates_of(&键).expect("读得出");
    assert!(
        候选.iter().any(|c| c.source == "No-Intro"),
        "去头那套撞得上 No-Intro：{候选:#?}",
    );
}

#[test]
fn 目录名是分解形式时它底下整棵子树都还认得出() {
    // 一个**目录名**中招，整棵子树的键全跟着走折回去那条退路——主库实测有 5 个目录名
    // 是分解形式（ADR-0020）。这里还顺带证同一层的两条只列一次目录就都认得出。
    let mut library = MemFs::new();
    let 分解形目录 = "\u{30b1}\u{3099}ーム";
    let 预组合目录 = "ゲーム";
    library.file(format!("/lib/FC/{分解形目录}/原版.nes"), 原版());
    library.file(format!("/lib/FC/{分解形目录}/汉化.nes"), 汉化版());
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    scan::scan(
        &library,
        &mut catalog,
        &ScanOptions::named("/lib", "库"),
        &Handle::new(),
    )
    .expect("扫得动");

    跑一趟(&library, &mut catalog, &建_dat());

    for 文件名 in ["原版.nes", "汉化.nes"] {
        let 键 = format!("库/FC/{预组合目录}/{文件名}");
        let (结论, 为什么) = catalog
            .identification_of(&键)
            .expect("读得出")
            .unwrap_or_else(|| panic!("{键} 没有识别结论"));
        assert_eq!(结论, State::Matched, "{键}：{为什么:?}");
    }
}

#[test]
fn 还没识别的变体照样占着变体总数与全部变体里那个分母() {
    // 词表「还没识别」：一个变体**连识别都还没跑过**。它既不是「未命中」（撞过没撞上，
    // 是结论），也不是「无判据」（拿不到可撞的东西）。三者混在一起，命中率就失真。
    //
    // 报告从前是 `identification JOIN variant` 折出来的，于是一行结论都没有的变体
    // **连分母都进不去**：识别完再扫进一个新文件，报告照旧说「变体 8」、覆盖率按 8 算。
    let mut 现场 = 建现场();
    let 第一趟 = 跑(&mut 现场);
    let 跑过的 = 第一趟.report.total.variants;
    assert_eq!(跑过的, 8, "fixture 里八个变体");
    assert_eq!(第一趟.report.total.not_run, 0, "跑完一整趟，一个都不剩");

    // 再扫进一个新文件，**不重跑识别**。
    写(
        &现场.dir.path().join("FC/后来才放进来的.zip"),
        &zip_container(&[ZipEntrySpec::stored("新的.nes", ines(0x5A, 4_096))]),
    );
    重扫(&mut 现场);

    let report = IdentifyReport::build(&现场.catalog, &现场.repo).expect("折得出报告");
    assert_eq!(report.total.variants, 跑过的 + 1, "新来的那个也是一个变体");
    assert_eq!(report.total.not_run, 1, "它还没识别");
    // **四档一档都没多**：还没识别既不是未命中也不是无判据，更不是跳过。
    assert_eq!(report.total.matched, 第一趟.report.total.matched);
    assert_eq!(report.total.unmatched, 第一趟.report.total.unmatched);
    assert_eq!(report.total.no_evidence, 第一趟.report.total.no_evidence);
    assert_eq!(report.total.skipped, 第一趟.report.total.skipped);

    // 「撞了 DAT 的里面」那个分母不动——它一次都没撞过。
    assert!(
        (report.total.hit_rate() - 第一趟.report.total.hit_rate()).abs() < 1e-9,
        "{:.3}% vs {:.3}%",
        report.total.hit_rate(),
        第一趟.report.total.hit_rate(),
    );
    // 「全部变体里」那个分母跟着涨，于是覆盖率降下来——那才是实话。
    assert!(
        report.total.coverage() < 第一趟.report.total.coverage(),
        "分母多了一个还没识别的，覆盖率该降：{:.1}% vs {:.1}%",
        report.total.coverage(),
        第一趟.report.total.coverage(),
    );

    // 按平台那一行加得起来：变体 = 命中 + 未命中 + 无判据 + 跳过 + 还没识别。
    let fc = report
        .platforms
        .iter()
        .find(|row| row.platform == "FC")
        .expect("有 FC");
    assert_eq!(fc.not_run, 1);
    assert_eq!(
        fc.variants,
        fc.matched + fc.unmatched + fc.no_evidence + fc.skipped + fc.not_run,
        "{fc:?}"
    );

    // 报告嘴上说得出这个数——三处没有一处说得出，正是这一票的病根。
    let text = report.render_text();
    assert!(text.contains("还没识别 1"), "总数那一行要点名：{text}");
    assert!(text.contains("其中 1 个**还没识别**"), "{text}");
}

#[test]
fn 识别被中断后报告说的是全部变体而不是跑完的那几个() {
    // 现象：识别只跑完一部分就被中断，报告说「变体 1 个，全部变体里 100.0%」，
    // 而库里躺着 5 个。没轮到的那些是**还没识别**，它们照样是这个库的一部分。
    let mut 现场 = 建现场();
    let cancel = CancelToken::new();
    cancel.cancel();
    let outcome = identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &identify::Ammo {
            repo: &现场.repo,
            verdicts: &verdict::Index::empty(),
            naming: &fuzzy::Naming::off(),
            guessing: &identify::model::Guessing::off(),
            titledb: None,
        },
        &Options::new(Roots::single("库", 现场.dir.path())),
        &cancel,
        &mut |_| {},
    )
    .expect("中断不是错误");

    assert!(outcome.interrupted, "这一趟是被中断的");
    let report = &outcome.report;
    assert_eq!(report.total.variants, 8, "库里有八个变体，中断不改变这件事");
    assert_eq!(report.total.not_run, 8, "一个都还没轮到");
    assert_eq!(report.total.matched, 0);
    assert_eq!(report.total.unmatched, 0);
    // 一次都没撞过 DAT：命中率的分母是 0，按报告的规矩算 0，**不许算成 100%**。
    assert!((report.total.hit_rate() - 0.0).abs() < 1e-9);
    assert!((report.total.coverage() - 0.0).abs() < 1e-9);
    let text = report.render_text();
    assert!(text.contains("还没识别 8"), "{text}");
}

#[test]
fn 命中的那份删掉重扫之后结论与候选不再交出() {
    // ⭐ **变体没了，挂在它身上的结论也就没了。** 窗口是「重扫到下一趟识别之间」：
    // 那期间报告、标题集合与导出都还在读这几张表，读到的是一个盘上已经不存在的东西。
    let mut 现场 = 建现场();
    跑(&mut 现场);
    let 键 = "库/FC/超级马里奥.zip";
    assert_eq!(结论(&现场, 键).0, State::Matched, "先得真命中一次");
    assert_eq!(
        现场.catalog.candidates_of(键).expect("读得出").len(),
        2,
        "含头与去头两套口径各一条"
    );
    let 发行版数 = 现场.catalog.releases().expect("读得出").len();

    fs::remove_file(现场.dir.path().join("FC/超级马里奥.zip")).expect("删得掉");
    重扫(&mut 现场);

    assert!(
        现场.catalog.variant(键).expect("读得出").is_none(),
        "前提：重新成型之后这个变体已经不在了"
    );
    assert!(
        现场
            .catalog
            .identification_of(键)
            .expect("读得出")
            .is_none(),
        "结论跟着变体走"
    );
    assert!(
        现场.catalog.candidates_of(键).expect("读得出").is_empty(),
        "候选跟着变体走"
    );
    let mut 交出的 = Vec::new();
    现场
        .catalog
        .for_each_accepted_candidate(&mut |candidate| {
            交出的.push(candidate.variant_key.to_string());
        })
        .expect("走得动");
    assert!(
        !交出的.contains(&键.to_string()),
        "刮削那一侧不许再收到这个键：{交出的:?}"
    );
    assert_eq!(
        现场.catalog.releases().expect("读得出").len(),
        发行版数 - 1,
        "只剩那份候选独家撑着的发行版跟着收掉"
    );
}

#[test]
fn 作品与发行版表里不留指不着任何变体的行() {
    // 留着的话，「识别建出来的作品数」会一直虚高，浏览屏上还会长出指不着任何文件的行。
    let mut 现场 = 建现场();
    跑(&mut 现场);
    fs::remove_file(现场.dir.path().join("FC/超级马里奥.zip")).expect("删得掉");
    fs::remove_file(现场.dir.path().join("FC/某游戏 汉化版.zip")).expect("删得掉");
    重扫(&mut 现场);

    let 变体们 = 现场.catalog.variants().expect("读得出变体");
    let 发行版们 = 现场.catalog.releases().expect("读得出发行版");
    let 作品们 = 现场.catalog.work_names().expect("读得出作品");
    let mut 候选指着的 = Vec::new();
    现场
        .catalog
        .for_each_accepted_candidate(&mut |candidate| {
            候选指着的.extend(candidate.release_id);
        })
        .expect("走得动");
    for (id, release) in &发行版们 {
        let 有人指 = 变体们.iter().any(|it| it.release_id == Some(*id)) || 候选指着的.contains(id);
        assert!(有人指, "发行版 {id} 指不着任何变体：{release:?}");
        assert!(作品们.contains_key(&release.work_id), "它的作品还在");
    }
    for id in 作品们.keys() {
        assert!(
            变体们.iter().any(|it| it.work_id == Some(*id))
                || 发行版们.values().any(|it| it.work_id == *id),
            "作品 {id} 指不着任何变体、也没有发行版挂在它下面"
        );
    }
    // **标题集合是这几张表的直接消费者**：孤儿发行版留着，`title::fold` 就照它的
    // 官方条目名折出一条指不着任何文件的标题。
    let 折出来 = romcat_core::title::fold(&现场.catalog).expect("折得出标题集合");
    assert!(
        !折出来.iter().any(|row| row.value.contains("Super Mario")),
        "那份已删文件的官方名不许再进标题集合：{:?}",
        折出来.iter().map(|row| &row.value).collect::<Vec<_>>()
    );
}
