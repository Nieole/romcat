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

use romcat_core::catalog::browse::{WorkAnchor, WorkQuery};
use romcat_core::catalog::identify::State;
use romcat_core::catalog::scrape::{Harvested, HarvestedMedia, HarvestedValue};
use romcat_core::catalog::{Catalog, Confidence, Provenance, Roots};
use romcat_core::collection::FAVORITE;
use romcat_core::dat::Convention;
use romcat_core::dat::logiqx::{DatHeader, GameRecord, RomRecord};
use romcat_core::dat::repo::{DatMeta, DatRepo, Unit};
use romcat_core::fs::{MemFs, RealFs};
use romcat_core::identify::fuzzy;
use romcat_core::identify::report::IdentifyReport;
use romcat_core::identify::{self, Options};
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::scrape::{AnchorKind, Field};
use romcat_core::task::Handle;
use romcat_core::testing::container::{ZipEntrySpec, crc32, zip_container};
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::verdict::{self, Anchor, Decision, Facts, Membership, Store, Verdict};

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
    /// **沉淀库**。多数测试用不上它（空的等于 `Index::empty()`），
    /// 作品那几条要靠它证「裁决造的那一行重跑识别之后原样还在」。
    store: Store,
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

/// 谁也不认得的那一份：DAT 里一条都没有，识别只说得出「未命中」。
fn 陌生() -> Vec<u8> {
    ines(0xEE, 4_096)
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
        &zip_container(&[ZipEntrySpec::stored("陌生.nes", 陌生())]),
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

    扫成现场(dir, "库", 建_dat())
}

/// 把 `dir` 当成一份根叫 `根名` 的主库扫一遍，配上 `repo` 与一个空的沉淀库。
fn 扫成现场(dir: TempDir, 根名: &str, repo: DatRepo) -> 现场 {
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::named(dir.path(), 根名);
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");

    现场 {
        dir,
        catalog,
        repo,
        store: Store::in_memory().expect("开得出沉淀库"),
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
    按根名跑(现场, "库")
}

/// 与 [`跑`] 同一趟，只是这份主库的**根**叫 `根名`。
fn 按根名跑(现场: &mut 现场, 根名: &str) -> identify::Outcome {
    let options = Options::new(Roots::single(根名, 现场.dir.path()));
    let verdicts = verdict::Index::load(&现场.store, 根名).expect("读得出沉淀库");
    identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &identify::Ammo {
            repo: &现场.repo,
            verdicts: &verdicts,
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

// ───── 重跑识别复用同名的现成行（票 parking-3/10） ─────────────────────────────
//
// 老路子是**删了重建**：`clear_identifications` 把 `work` 整张清掉，下一趟照名字再
// 建一遍。于是 `work.id` 每跑一趟就换一批——挂在旧 id 上的东西集体失联（挂单 `Q193`），
// 同名的作品还可能攒出两行（挂账 `D162`）。这几条钉的是新的口径：**按名字找现成的
// 那一行复用，找不到才新建**。

/// 把一条**裁决**落进沉淀库，钉在这份字节的**内容锚**上。
fn 裁(现场: &mut 现场, bytes: &[u8], work: &str) {
    现场
        .store
        .put(&Verdict::now(
            Anchor::Content {
                crc32: crc32(bytes),
                size: u64::try_from(bytes.len()).expect("装得下"),
                sha1: None,
            },
            Decision::Release(Facts {
                work: work.to_string(),
                platform: Some("FC".to_string()),
                ..Facts::default()
            }),
        ))
        .expect("落得进沉淀库");
}

/// 跑一趟**起手就被按停**的识别：`clear_identifications` 已经跑过，而一个变体都没轮到。
fn 按停着跑(现场: &mut 现场) -> identify::Outcome {
    let cancel = CancelToken::new();
    cancel.cancel();
    let verdicts = verdict::Index::load(&现场.store, "库").expect("读得出沉淀库");
    identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &identify::Ammo {
            repo: &现场.repo,
            verdicts: &verdicts,
            naming: &fuzzy::Naming::off(),
            guessing: &identify::model::Guessing::off(),
            titledb: None,
        },
        &Options::new(Roots::single("库", 现场.dir.path())),
        &cancel,
        &mut |_| {},
    )
    .expect("中断不是错误")
}

/// 这个变体眼下挂在哪一条发行版上。
fn 挂着的发行版(现场: &现场, key: &str) -> i64 {
    现场
        .catalog
        .variant(key)
        .expect("读得出")
        .unwrap_or_else(|| panic!("{key} 这个变体不在"))
        .release_id
        .unwrap_or_else(|| panic!("{key} 认出了它基于的那次发行"))
}

/// 这个变体眼下挂在哪一行作品上。
fn 挂着的作品(现场: &现场, key: &str) -> Option<i64> {
    现场
        .catalog
        .variant(key)
        .expect("读得出")
        .unwrap_or_else(|| panic!("{key} 这个变体不在"))
        .work_id
}

#[test]
fn 连跑两趟识别作品行不涨也不换一批_id() {
    let mut 现场 = 建现场();
    跑(&mut 现场);
    let 第一趟 = 现场.catalog.work_names().expect("读得出作品");
    assert!(!第一趟.is_empty(), "前提：第一趟真的建出了作品行");
    let 马里奥 = 挂着的作品(&现场, "库/FC/超级马里奥.zip").expect("挂上了作品");

    跑(&mut 现场);

    assert_eq!(
        现场.catalog.work_names().expect("读得出作品"),
        第一趟,
        "作品行数不涨，id 一个都不换"
    );
    assert_eq!(
        挂着的作品(&现场, "库/FC/超级马里奥.zip"),
        Some(马里奥),
        "变体照旧指着同一行"
    );

    // ⚠️ 上面两条是**形状断言，不是回归闸**：删了重建那条老路子在这儿也过得去——
    // 表清空之后 rowid 从 1 重发，变体次序不变时发出来的号逐字相同。真正钉住这一票的
    // 是下面第三趟。
    //
    // ⭐ 库里多出一部作品的那一趟才是要害：删了重建的老路子按变体的次序重新发号，
    // 中间插进来一行，它**后面**的作品全体换号。复用不发新号——新的那一行拿新号，
    // 现成的一个都不动。
    裁(&mut 现场, &陌生(), "一部新作品");
    跑(&mut 现场);

    let 第三趟 = 现场.catalog.work_names().expect("读得出作品");
    for (id, name) in &第一趟 {
        assert_eq!(
            第三趟.get(id),
            Some(name),
            "现成的作品 {id}「{name}」换号了：{第三趟:?}"
        );
    }
    assert_eq!(第三趟.len(), 第一趟.len() + 1, "只多出裁决新点名的那一行");
}

#[test]
fn 作品锚点在重跑识别之后仍然指得中() {
    // ⭐ 挂单 `Q193`：`WorkAnchor::Work(id)` 攒下来的那些东西（收藏、媒体、刮削结论）
    // 全靠这个 id 指得中。id 一换，挂在旧 id 上的东西集体失联。
    let mut 现场 = 建现场();
    跑(&mut 现场);
    let 锚 = WorkAnchor::Work(挂着的作品(&现场, "库/FC/超级马里奥.zip").expect("挂上了作品"));
    let 名字 = 现场
        .catalog
        .work_detail(&WorkQuery::default(), &锚)
        .expect("读得出")
        .expect("这一行在")
        .name;

    // 往作品锚点上挂两样东西：一条刮削值（年份）与一份媒体。
    现场
        .catalog
        .put_media("deadbeef", "png", 1)
        .expect("媒体池收得下");
    现场
        .catalog
        .put_scraped(&[Harvested {
            anchor: AnchorKind::Work.label().to_string(),
            subject: 名字.clone(),
            source: "某源".to_string(),
            input: "指纹".to_string(),
            values: vec![HarvestedValue {
                field: Field::Year.label().to_string(),
                value: "1985".to_string(),
                evidence: "测试".to_string(),
            }],
            media: vec![HarvestedMedia {
                kind: "封面".to_string(),
                hash: "deadbeef".to_string(),
                evidence: "测试".to_string(),
            }],
        }])
        .expect("写得进");

    // **收藏**走的是另一条路：它落**沉淀库**、锚在**内容锚**上，每趟识别照那份库重投
    // 一遍。这里一起钉着，是因为界面上「★ 收藏」这个动作瞄的正是作品锚点那一行
    // ——id 换了，星就按在别人身上了。
    现场
        .store
        .join(&[Membership::now(
            FAVORITE,
            Anchor::Content {
                crc32: crc32(&原版()),
                size: u64::try_from(原版().len()).expect("装得下"),
                sha1: None,
            },
        )])
        .expect("收藏落得进沉淀库");

    // 库里少掉一部作品，剩下的那些在老路子上会整体换号——那正是 `Q193` 说的
    // 「作品锚点里的那个 id 会被重跑识别换掉」。
    fs::remove_file(现场.dir.path().join("FC/某游戏 汉化版.zip")).expect("删得掉");
    重扫(&mut 现场);
    跑(&mut 现场);

    let 再看 = 现场
        .catalog
        .work_detail(&WorkQuery::default(), &锚)
        .expect("读得出")
        .expect("重跑识别之后这条作品锚点还指得中");
    assert_eq!(再看.name, 名字, "指着的还是同一部作品");
    assert_eq!(
        再看.year.as_deref(),
        Some("1985"),
        "挂在作品锚点上的刮削值一条不丢"
    );
    assert!(
        !现场
            .catalog
            .scraped_media(AnchorKind::Work.label(), &再看.name)
            .expect("读得出")
            .is_empty(),
        "挂在作品锚点上的媒体一条不丢"
    );

    let 合集 = 现场
        .catalog
        .collection_memberships()
        .expect("读得出合集成员");
    assert!(
        再看.variants.iter().any(|it| 合集
            .get(&it.row.key)
            .is_some_and(|names| names.iter().any(|name| name == FAVORITE))),
        "这一行底下那个收藏还在：库里收着 {合集:?}，这一行是 {:?}",
        再看
            .variants
            .iter()
            .map(|it| &it.row.key)
            .collect::<Vec<_>>(),
    );
}

#[test]
fn 被叫停之后重扫再跑一趟作品锚点照样指得中() {
    // ⭐ 这条守的是「按名字复用」最容易漏的那条缝：**被叫停的那一趟不收作品**，可紧接着
    // 的一次**重新成型**收作品走的是另一张网（`drop_unheld_works`），判据是「还有没有人
    // 指着」、**不看来路**。识别起手要是把 `variant.work_id` 整列摘空，没轮到的变体就从此
    // 挂着空，那张网会把整库的作品连同 id 一起扫掉——下一趟回来全是新号。
    //
    // 「被叫停之后接着重扫」是常规动作，不是边角。
    let mut 现场 = 建现场();
    跑(&mut 现场);
    let 第一趟 = 现场.catalog.work_names().expect("读得出作品");
    assert!(!第一趟.is_empty(), "前提：第一趟真的建出了作品行");
    let 马里奥 = 挂着的作品(&现场, "库/FC/超级马里奥.zip").expect("挂上了作品");

    assert!(按停着跑(&mut 现场).interrupted, "前提：这一趟是被按停的");
    重扫(&mut 现场);

    assert_eq!(
        现场.catalog.work_names().expect("读得出作品"),
        第一趟,
        "没轮到的变体照旧指着它们的作品，那几行一个都不该被重新成型收掉"
    );

    跑(&mut 现场);
    assert_eq!(
        挂着的作品(&现场, "库/FC/超级马里奥.zip"),
        Some(马里奥),
        "跑完一整趟之后，作品锚点还是原来那一个"
    );
}

#[test]
fn 同名的两条发行版线不会被并成一条() {
    // ⭐ 同名异作是真实存在的（1988 与 2004 两部 Ninja Gaiden）。工具手上只有名字，
    // 所以**作品**那一层它们仍旧共用一行——那正是 `work.name` 上**不补 `UNIQUE`** 的
    // 理由：门留着，日后把其中一部改个名（一条裁决就够）就是两行，而重跑识别不会把
    // 裁决那一行抹掉（见上一条）。
    //
    // **分得开的那一层是发行版**：两条发行版线各自成行，按名字复用作品那一行不把它们
    // 并成一条，重跑一趟也不多攒出第三条。
    let dir = temp_dir("identify-同名异作");
    let root = dir.path();
    let 卡带版 = ines(0x11, 4_096);
    let 另一部 = smc(0x44, 32_768);
    写(
        &root.join("FC/忍者龙剑传.zip"),
        &zip_container(&[ZipEntrySpec::stored("ng.nes", 卡带版.clone())]),
    );
    写(&root.join("SFC/忍者龙剑传 另一部.smc"), &另一部);

    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::named(root, "库");
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");

    let mut repo = DatRepo::in_memory().expect("开得出来");
    装(
        &mut repo,
        "TOSEC",
        "Nintendo Famicom - Games",
        "FC",
        Convention::AsIs,
        &[条目(
            "Ninja Gaiden (1988)(Tecmo)",
            "Ninja Gaiden (1988)(Tecmo).nes",
            u64::try_from(卡带版.len()).expect("装得下"),
            crc32(&卡带版),
        )],
    );
    装(
        &mut repo,
        "No-Intro",
        "Nintendo - Super Nintendo Entertainment System",
        "SFC",
        Convention::Headerless,
        &[条目(
            "Ninja Gaiden (USA)",
            "Ninja Gaiden (USA).sfc",
            32_768,
            crc32(&另一部[512..]),
        )],
    );
    let mut 现场 = 现场 {
        dir,
        catalog,
        repo,
        store: Store::in_memory().expect("开得出沉淀库"),
    };
    跑(&mut 现场);

    let 甲 = "库/FC/忍者龙剑传.zip";
    let 乙 = "库/SFC/忍者龙剑传 另一部.smc";
    assert_eq!(
        现场.catalog.work_of_variant(甲).expect("读得出"),
        现场.catalog.work_of_variant(乙).expect("读得出"),
        "前提：两条 DAT 记录折出来的是同一个作品名"
    );
    let 作品 = 挂着的作品(&现场, 甲).expect("认出了作品");
    assert_eq!(
        挂着的作品(&现场, 乙),
        Some(作品),
        "作品是跨平台跨地区的**游戏概念**，同名的这两条在这一层共用一行"
    );

    assert_ne!(
        挂着的发行版(&现场, 甲),
        挂着的发行版(&现场, 乙),
        "两条发行版线各是各的，没被并成一条"
    );
    let 发行版们 = 现场.catalog.releases().expect("读得出发行版");
    assert_eq!(发行版们.len(), 2, "一共就这两条：{发行版们:?}");
    assert_eq!(
        发行版们[&挂着的发行版(&现场, 甲)].platform.as_deref(),
        Some("FC")
    );
    assert_eq!(
        发行版们[&挂着的发行版(&现场, 乙)].platform.as_deref(),
        Some("SFC")
    );

    跑(&mut 现场);

    assert_eq!(
        挂着的作品(&现场, 甲),
        Some(作品),
        "重跑之后作品那一行还是它"
    );
    assert_eq!(挂着的作品(&现场, 乙), Some(作品));
    assert_ne!(
        挂着的发行版(&现场, 甲),
        挂着的发行版(&现场, 乙),
        "重跑之后两条线照旧分得开"
    );
    assert_eq!(
        现场.catalog.releases().expect("读得出发行版").len(),
        2,
        "重跑不多攒出第三条发行版"
    );
}

#[test]
fn 重跑识别认领裁决造的那一行而不是新建一行() {
    let mut 现场 = 建现场();
    跑(&mut 现场);
    let 作品名 = 现场
        .catalog
        .work_of_variant("库/FC/超级马里奥.zip")
        .expect("读得出")
        .expect("认出了作品");
    let 那一行 = 挂着的作品(&现场, "库/FC/超级马里奥.zip").expect("挂上了作品");
    let 汉化那一行 = 挂着的作品(&现场, "库/FC/某游戏 汉化版.zip").expect("汉化版也认出了作品");
    assert_ne!(那一行, 汉化那一行, "前提：这一趟它们是两部作品");

    // 一条裁决把汉化版那一份也钉在**同一部作品**上：识别与裁决共用同一张作品表。
    裁(&mut 现场, &汉化版(), &作品名);
    跑(&mut 现场);

    assert_eq!(
        挂着的作品(&现场, "库/FC/超级马里奥.zip"),
        Some(那一行),
        "识别这一趟认领的是现成的那一行，不是新建的"
    );
    assert_eq!(
        挂着的作品(&现场, "库/FC/某游戏 汉化版.zip"),
        Some(那一行),
        "裁决落在同一行上——两行同名的作品会让导出时的收敛拆成两个条目"
    );

    // 再跑一趟：裁决造出来的结果不会被下一趟识别抹掉。
    跑(&mut 现场);
    assert_eq!(
        挂着的作品(&现场, "库/FC/某游戏 汉化版.zip"),
        Some(那一行),
        "裁决过的结果原样还在"
    );
}

#[test]
fn 同名异作插得进两行而重跑识别不抹掉裁决造的那一行() {
    // **作品是跨平台、跨地区的游戏概念**（平台在发行版那一层），同名异作是真实存在的
    // （1988 与 2004 两部 Ninja Gaiden）。所以 `work.name` 上**没有 `UNIQUE`**：
    // 裸约束会把它们强行并成一部，连人工把其中一部拆出来的路都堵死。
    let mut 现场 = 建现场();
    let 识别造的 = 现场
        .catalog
        .add_work("Ninja Gaiden", Provenance::Identified)
        .expect("插得进");
    let 裁决造的 = 现场
        .catalog
        .add_work("Ninja Gaiden", Provenance::Verdict)
        .expect("同名的第二行照样插得进——库里没有拦它的约束");
    assert_ne!(识别造的, 裁决造的, "两行同名的作品各是各的");

    // 复用挑的是**裁决造的**那一行：人定下来的优先于机器撞出来的。**直接断在
    // `Catalog::work_named` 上**是有意的——它是核心库的公开接缝，而这条口径（票里那句
    // 「裁决来源的优先」）只在库里同名有好几行时才看得出来，而那种库识别自己造不出来。
    assert_eq!(
        现场.catalog.work_named("Ninja Gaiden").expect("读得出"),
        Some(裁决造的),
    );

    跑(&mut 现场);

    let 作品们 = 现场.catalog.work_names().expect("读得出作品");
    assert_eq!(
        作品们.get(&裁决造的).map(String::as_str),
        Some("Ninja Gaiden"),
        "裁决造的那一行不会被下一趟识别抹掉——重跑识别只清「识别自己造的」那些"
    );
    assert!(
        !作品们.contains_key(&识别造的),
        "识别自己造的、如今没人指着的那一行跟着这一趟收掉"
    );
}

// ───── 按停之后接着算（票 gui-answers-all-six/03） ─────────────────────────────────
//
// 从前识别起手把结论整份清掉，按停之后下一趟从头再算一遍（挂单 `Q420`）。这几条钉的是
// 新的口径：**下一趟只算还没算过的那些**，算完之后与一趟不停跑到底一模一样。
//
// **按停钉在一个信号上，不钉在挂钟上**：一个变体一批，每算完一个报一次进度，报到第几个
// 就在那一下按停——主循环在轮到下一个变体之前就看得见它。

/// 跑一趟识别，**算完第 `按停在` 个变体就按停**（`None` 是不按）；另把这一趟报的最后
/// 一次进度交回来——「这一趟过了几个变体」看的就是它。
fn 跑一趟记下进度(
    现场: &mut 现场,
    按停在: Option<u64>,
) -> (identify::Outcome, identify::Progress) {
    带着模型那一层跑一趟(现场, 按停在, &identify::model::Guessing::off())
}

/// 同 [`跑一趟记下进度`]，模型推断那一层的弹药由调用方给。
fn 带着模型那一层跑一趟(
    现场: &mut 现场,
    按停在: Option<u64>,
    guessing: &identify::model::Guessing<'_>,
) -> (identify::Outcome, identify::Progress) {
    let cancel = CancelToken::new();
    let mut options = Options::new(Roots::single("库", 现场.dir.path()));
    options.write_batch = 1;
    let verdicts = verdict::Index::load(&现场.store, "库").expect("读得出沉淀库");
    let mut 最后一次 = identify::Progress::default();
    let outcome = identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &identify::Ammo {
            repo: &现场.repo,
            verdicts: &verdicts,
            naming: &fuzzy::Naming::off(),
            guessing,
            titledb: None,
        },
        &options,
        &cancel,
        &mut |progress| {
            最后一次 = *progress;
            if 按停在.is_some_and(|at| progress.done >= at) {
                cancel.cancel();
            }
        },
    )
    .expect("按停不是错误");
    (outcome, 最后一次)
}

/// 这个变体在 `variants()` 的次序里排第几（从 1 数）。按停就停在它后面。
fn 排第几(现场: &现场, key: &str) -> u64 {
    let at = 现场
        .catalog
        .variants()
        .expect("读得出变体")
        .iter()
        .position(|variant| variant.key == key)
        .unwrap_or_else(|| panic!("{key} 这个变体不在"));
    u64::try_from(at + 1).expect("数得过来")
}

#[test]
fn 按停之后再跑一趟_已经算完的那些不再重算() {
    let mut 现场 = 建现场();
    let 变体数 =
        u64::try_from(现场.catalog.variants().expect("读得出变体").len()).expect("数得过来");
    // 停在那份带拷贝机头的**裸文件**后面：它的哈希是回盘读出来、落进中立库的。下一趟要是
    // 把它再过一遍，「从中立库直接取回来的哈希」那个数就不是 0（见本条最后一段对照）。
    let 按停在 = 排第几(&现场, "库/SFC/带拷贝机头的.smc");
    assert!(按停在 < 变体数, "前提：按停之后还剩没轮到的变体");

    let (第一趟, _) = 跑一趟记下进度(&mut 现场, Some(按停在));
    assert!(第一趟.interrupted, "前提：这一趟是被按停的");
    assert_eq!(
        变体数 - 现场.catalog.not_run_count().expect("数得出"),
        按停在,
        "前提：按停时正好算完这几个"
    );

    let (第二趟, 进度) = 跑一趟记下进度(&mut 现场, None);
    assert!(!第二趟.interrupted, "没人按停，这一趟却没走完");
    assert_eq!(
        (进度.done, 进度.total),
        (变体数 - 按停在, 变体数 - 按停在),
        "这一趟该过的只有上一趟没轮到的那几个"
    );
    assert_eq!(
        第二趟.reused_hashes, 0,
        "上一趟算过哈希的变体这一趟又被过了一遍"
    );
    assert_eq!(
        现场.catalog.not_run_count().expect("数得出"),
        0,
        "接着算完之后一个「还没识别」都不剩"
    );

    // 对照：上一趟跑完了，这一趟**照旧从头算**（DAT 库换过之后重跑一遍靠的就是它），
    // 而同一份裸文件的哈希这时是从中立库取回来的——上面那个 0 不是因为这份 fixture
    // 根本取不回哈希。
    let (第三趟, 进度) = 跑一趟记下进度(&mut 现场, None);
    assert_eq!(进度.total, 变体数, "上一趟跑完了，这一趟该从头算");
    assert!(
        第三趟.reused_hashes > 0,
        "从头算的那一趟一份哈希都没从中立库取回来，上面那个 0 验不出东西"
    );
}

/// 再摆两份字节相同的：
///
/// - 一份「超级马里奥」：两个变体撞上**同一条 DAT 条目**，该共用一条发行版。
/// - 一份「谁也不认得」，放进 `wii/`：调用方裁过那份字节的话，两个变体钉着**同一条裁决**，
///   该共用裁决落成的那条发行版。
fn 建现场加两份同款() -> 现场 {
    let mut 现场 = 建现场();
    写(
        &现场.dir.path().join("FC/超级马里奥 备份.zip"),
        &zip_container(&[ZipEntrySpec::stored("Super Mario (Japan).nes", 原版())]),
    );
    写(
        &现场.dir.path().join("wii/谁也不认得的又一份.zip"),
        &zip_container(&[ZipEntrySpec::stored("陌生.nes", 陌生())]),
    );
    重扫(&mut 现场);
    现场
}

/// 两份库里识别落下来的结论一行一行比，红了说得出是哪一行不一样。
fn 两份结论一样(一口气: &现场, 停过: &现场) {
    let (甲, 乙) = (结论全貌(一口气), 结论全貌(停过));
    for (一口气的, 停过的) in 甲.iter().zip(&乙) {
        assert_eq!(停过的, 一口气的, "按停过再接着算完的那份库，这一行不一样");
    }
    assert_eq!(乙.len(), 甲.len(), "两份库的结论条数不一样");
}

/// 每个变体的结论上记着的**这一趟为它读了多少字节**。
///
/// 它是账，不是结论：同一份库从头再跑一趟，算过的哈希取回来不读盘，这一列就成了 0。
/// 所以它不在 [`结论全貌`] 里，要比的那一条自己比。
fn 读盘的账(现场: &现场) -> Vec<(String, u64)> {
    let mut 账 = Vec::new();
    现场
        .catalog
        .for_each_identification(&mut |_, _, _, key, read_bytes| {
            账.push((key.to_string(), read_bytes));
        })
        .expect("走得动");
    账
}

/// 一份中立库里**识别落下来的全部结论**，一行一行拼成字。
///
/// **不带行号**：行号是插入次序的产物，比它等于在比「两份库按同一个次序建行」，而这里
/// 要比的是结论本身。作品与发行版因此按它们说的东西比，外加各有几行。
fn 结论全貌(现场: &现场) -> Vec<String> {
    let catalog = &现场.catalog;
    let 作品们 = catalog.work_names().expect("读得出作品");
    let 发行版们 = catalog.releases().expect("读得出发行版");
    let 作品 = |id: Option<i64>| {
        id.map(|id| {
            作品们
                .get(&id)
                .cloned()
                .unwrap_or_else(|| format!("悬空的作品 {id}"))
        })
    };
    let 发行版 = |id: Option<i64>| {
        id.map(|id| match 发行版们.get(&id) {
            Some(row) => format!(
                "{:?}|{:?}|{:?}|{:?}|{:?}",
                作品们.get(&row.work_id),
                row.platform,
                row.region,
                row.serial,
                row.languages
            ),
            None => format!("悬空的发行版 {id}"),
        })
    };
    let 平台们 = catalog.identified_platforms().expect("读得出平台");
    let mut 行 = vec![format!(
        "作品 {} 行，发行版 {} 行",
        作品们.len(),
        发行版们.len()
    )];
    catalog
        .for_each_identification(&mut |_, state, reason, key, _| {
            行.push(format!(
                "{key}：{state:?}，理由 {reason:?}，平台 {:?}",
                平台们.get(key)
            ));
        })
        .expect("走得动");
    for variant in catalog.variants().expect("读得出变体") {
        行.push(format!(
            "{}：作品 {:?}，发行版 {:?}",
            variant.key,
            作品(variant.work_id),
            发行版(variant.release_id)
        ));
        for mut candidate in catalog.candidates_of(&variant.key).expect("读得出候选") {
            let 指着 = 发行版(candidate.release_id.take());
            行.push(format!(
                "{}：候选 {candidate:?}，指着 {指着:?}",
                variant.key
            ));
        }
    }
    行.push(
        IdentifyReport::build(catalog, &现场.repo)
            .expect("折得出报告")
            .render_text(),
    );
    行
}

#[test]
fn 按停之后接着跑完_结论与一趟不停跑到底一模一样() {
    // 验收那句「完全一样」：状态（`State::of` 那一处定的）、理由、识别判定的平台、候选、
    // 作品与发行版，连报告一起比。
    //
    // ⭐ 按停落在**两对该共用一条发行版的变体中间**：上一趟那一个建出了发行版，下一趟
    // 那一个得认领它，而不是再建一条——多出来的那一行会让导出时的**收敛**把一个条目拆成
    // 两个。一对撞的是同一条 DAT 条目，一对钉的是同一条裁决，两条认领的路各走一遍。
    let 同款们 = [
        ("库/FC/超级马里奥 备份.zip", "库/FC/超级马里奥.zip"),
        ("库/FC/谁也不认得.zip", "库/wii/谁也不认得的又一份.zip"),
    ];
    let mut 一口气 = 建现场加两份同款();
    裁(&mut 一口气, &陌生(), "一部新作品");
    跑一趟记下进度(&mut 一口气, None);
    for (这个, 那个) in 同款们 {
        assert_eq!(
            挂着的发行版(&一口气, 这个),
            挂着的发行版(&一口气, 那个),
            "前提：{这个} 与 {那个} 共用一条发行版"
        );
    }

    let mut 停过 = 建现场加两份同款();
    裁(&mut 停过, &陌生(), "一部新作品");
    let 按停在 = 排第几(&停过, "库/FC/超级马里奥 备份.zip");
    for (这个, 那个) in 同款们 {
        assert!(
            排第几(&停过, 这个) <= 按停在 && 排第几(&停过, 那个) > 按停在,
            "前提：{这个} 与 {那个} 分在按停的两边"
        );
    }
    let (第一趟, _) = 跑一趟记下进度(&mut 停过, Some(按停在));
    assert!(第一趟.interrupted, "前提：这一趟是被按停的");
    let (第二趟, _) = 跑一趟记下进度(&mut 停过, None);
    assert!(!第二趟.interrupted, "没人按停，这一趟却没走完");

    两份结论一样(&一口气, &停过);
    // 读盘的账这一回也对得上：上一趟没轮到的那几个这一趟才头一回读盘，
    // 与一趟不停跑到底读的一样多。
    assert_eq!(读盘的账(&停过), 读盘的账(&一口气), "读盘的账不一样");
}

#[test]
fn 按停之后接着算_模型推断那一层要问的一个不少() {
    // 模型推断那一层跑在主循环**之后**，被按停的那一趟整个不跑；它要问谁，得等全部变体
    // 都有了结论才定。接着算的那一趟要是只把这一趟算的那几个交给它，上一趟算完、一条
    // 候选都没有的那些就从此漏问——计划少算钱，真问的那一趟少问，结论就与一趟不停跑到底
    // 的不一样了。
    //
    // **只算计划，一个请求都不发**：要问几个变体，计划上那个数就说得出来。
    let mut 只算计划 = identify::model::Guessing::off();
    只算计划.planning = true;

    let mut 一口气 = 建现场();
    let (全算, _) = 带着模型那一层跑一趟(&mut 一口气, None, &只算计划);
    let 要问 = 全算.model.plan.as_ref().map(|plan| plan.variants);
    assert!(
        要问.is_some_and(|count| count > 0),
        "前提：这份 fixture 里有要交给模型推断的变体：{要问:?}"
    );

    let mut 停过 = 建现场();
    // 停在那个谁也不认得的后面：它一条候选都没有，正是要交给模型推断的那一种。
    let 按停在 = 排第几(&停过, "库/FC/谁也不认得.zip");
    let (第一趟, _) = 带着模型那一层跑一趟(&mut 停过, Some(按停在), &只算计划);
    assert!(第一趟.interrupted, "前提：这一趟是被按停的");
    let (接着算, _) = 带着模型那一层跑一趟(&mut 停过, None, &只算计划);

    assert_eq!(
        接着算.model.plan.as_ref().map(|plan| plan.variants),
        要问,
        "接着算完的那一趟，计划上要问的变体数与一趟不停跑到底的不一样"
    );
    // 重问的那几个又过了一遍，结论照样与一趟不停跑到底的一样。**读盘的账不比**：它们的
    // 哈希这一趟是从中立库取回来的，记成 0——那一列本来就是这一趟的账。
    两份结论一样(&一口气, &停过);
}

/// 一份 FC 游戏。没有 iNES 头，含头与去头两套哈希落在同一串字节上。
fn 魂斗罗() -> Vec<u8> {
    vec![0xC3; 4_096]
}

/// 磁碟机的 BIOS（`disksys.rom`）：模拟器要它，它本身不是游戏。
fn 磁碟机_bios() -> Vec<u8> {
    vec![0x5A; 8_192]
}

/// 认得出上面两份的 DAT。**BIOS 那条在 No-Intro 里、游戏那条在 TOSEC 里**：源的先后
/// 让 BIOS 那条排在前面（`identify::rank`），真库里 BIOS 本来就收在 No-Intro 的主集里。
fn 认得出游戏与_bios_的_dat() -> DatRepo {
    let mut repo = DatRepo::in_memory().expect("开得出来");
    let bios = 磁碟机_bios();
    let 游戏 = 魂斗罗();
    装(
        &mut repo,
        "No-Intro",
        "Nintendo - Family Computer Disk System",
        "FC",
        Convention::AsIs,
        &[条目(
            "[BIOS] Family Computer Disk System (Japan)",
            "disksys.rom",
            bios.len() as u64,
            crc32(&bios),
        )],
    );
    装(
        &mut repo,
        "TOSEC",
        "Nintendo Famicom - Games",
        "FC",
        Convention::AsIs,
        &[条目(
            "Contra (1988-02-09)(Konami)(JP)",
            "Contra (1988-02-09)(Konami)(JP).nes",
            游戏.len() as u64,
            crc32(&游戏),
        )],
    );
    repo
}

/// 一份根叫 `根名` 的主库：`摆` 里每一条是「相对根的路径、盘上的字节」。
fn 按根名建现场(根名: &str, 摆: &[(&str, Vec<u8>)]) -> 现场 {
    let dir = temp_dir("identify-non-game-asset");
    for (relative, bytes) in 摆 {
        写(&dir.path().join(relative), bytes);
    }
    扫成现场(dir, 根名, 认得出游戏与_bios_的_dat())
}

/// 这个变体挂着的那部作品叫什么。
fn 作品名(现场: &现场, key: &str) -> Option<String> {
    挂着的作品(现场, key).map(|work| {
        现场
            .catalog
            .work_name(work)
            .expect("读得出")
            .unwrap_or_else(|| panic!("{key} 挂着的作品 {work} 不在作品表里"))
    })
}

#[test]
fn 容器里捎带一份_bios_时作品不被它定() {
    // 挂账 `D66`：整理包常把模拟器要的 BIOS 一起塞进游戏的透明容器。那份 BIOS 在 DAT 里
    // 同样是一条精确命中、自动通过，而且源排得更靠前——作品要是跟着「排第一的那条自动
    // 通过的候选」定，整包游戏就挂到了 BIOS 名下，屏上那一行写的不是维护者拥有的那个游戏。
    let key = "库/FC/魂斗罗 带 BIOS.zip";
    let 单放的_bios = "库/FC/bios/disksys.rom";
    let mut 现场 = 按根名建现场(
        "库",
        &[
            (
                "FC/魂斗罗 带 BIOS.zip",
                zip_container(&[
                    ZipEntrySpec::stored("Contra (Japan).nes", 魂斗罗()),
                    ZipEntrySpec::stored("bios/disksys.rom", 磁碟机_bios()),
                ]),
            ),
            ("FC/bios/disksys.rom", 磁碟机_bios()),
        ],
    );
    跑(&mut 现场);

    let 候选 = 现场.catalog.candidates_of(key).expect("读得出");
    assert_eq!(候选.len(), 2, "前提：游戏与 BIOS 各撞上一条：{候选:#?}");
    assert!(
        候选.iter().all(|c| c.accepted),
        "前提：两条都是精确命中、自动通过：{候选:#?}"
    );
    assert_eq!(
        候选[0].inner, "bios/disksys.rom",
        "前提：BIOS 那条排在第一：{候选:#?}"
    );

    assert_eq!(
        作品名(&现场, key).as_deref(),
        Some("Contra"),
        "作品是包里那个游戏，不是捎带的 BIOS"
    );
    // 另一头：一整个变体就是那份 BIOS 时，它**不归属任何作品**（ADR-0013）——导出那道闸
    // 把同一个变体挡成非游戏资产，两侧是同一个答案。
    assert_eq!(
        结论(&现场, 单放的_bios).0,
        State::Matched,
        "前提：它照样撞上了 DAT"
    );
    assert_eq!(作品名(&现场, 单放的_bios), None, "非游戏资产不挂到作品上");
}

#[test]
fn 真叫_bios_的根底下的游戏照旧认得出作品() {
    // **根名不参与判断。** 根的名字是维护者起的，不是这份内容是什么的依据：一个真叫
    // `BIOS` 的根底下的游戏照样是游戏，作品照样跟着它定。导出那一侧是同一个答案
    // （`tests/pegasus.rs` 的 `真叫_bios_的根底下的游戏照旧导出成条目`）。
    let key = "BIOS/FC/魂斗罗.zip";
    let mut 现场 = 按根名建现场(
        "BIOS",
        &[(
            "FC/魂斗罗.zip",
            zip_container(&[ZipEntrySpec::stored("Contra (Japan).nes", 魂斗罗())]),
        )],
    );
    按根名跑(&mut 现场, "BIOS");

    assert_eq!(
        作品名(&现场, key).as_deref(),
        Some("Contra"),
        "根叫 BIOS 不让它底下的游戏变成非游戏资产"
    );
}

/// **文件表那几行**（作品详情页「变体与文件」那张表，票 `gui-looks-like-the-design/15`，拿主意的人 2026-09-15 定核心库补查询）：
/// 一个透明容器是容器一行、里头的文件逐行带大小与 CRC-32（零解压就在容器头里）；裸文件一行，只扫没识别时 CRC 空着。
#[test]
fn 文件表那几行_容器一行里头的文件逐行带大小与校验和_裸文件一行() {
    use romcat_core::catalog::detail::FileLine;
    use romcat_core::shape::Role;

    let dir = temp_dir("识别-文件表");
    let root = dir.path().to_path_buf();
    let 包 = zip_container(&[ZipEntrySpec::stored("幻想传说.sfc", vec![7u8; 64])]);
    写(&root.join("SFC/幻想传说.zip"), &包);
    写(&root.join("SFC/魂斗罗.sfc"), &[1u8; 32]);
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::named(&root, "库");
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");

    assert_eq!(
        catalog
            .file_lines("库/SFC/幻想传说.zip", 40)
            .expect("读得出"),
        [
            FileLine {
                role: None,
                name: "库/SFC/幻想传说.zip".to_string(),
                size: Some(u64::try_from(包.len()).expect("装得下")),
                crc32: None,
                inner: false,
            },
            FileLine {
                role: Some(Role::Main),
                name: "幻想传说.sfc".to_string(),
                size: Some(64),
                // 64 个字节 7 的 CRC-32，另拿 zlib 算的。
                crc32: Some(0xD53C_59B8),
                inner: true,
            },
        ]
    );
    assert_eq!(
        catalog.file_lines("库/SFC/魂斗罗.sfc", 40).expect("读得出"),
        [FileLine {
            role: Some(Role::Main),
            name: "库/SFC/魂斗罗.sfc".to_string(),
            size: Some(32),
            crc32: None,
            inner: false,
        }]
    );
}
