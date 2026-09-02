//! **光盘序列号那一层**：磁盘上摆一份光盘世代的主库，跑一遍识别。
//!
//! 这里要证的是票 09 的那句话：**识别一个 8 GB 的 ISO 不需要读 8 GB。** 密集逻辑挂在
//! 纯函数上（`identify::disc` / `iso9660` / `sfo` / `serial` 各有自己的单元测试），
//! 这一份证的是它们真的接在了扫描、成型、DAT 库与中立库之间——尤其是那几种**压缩镜像**
//! 与**目录树转储**：它们在票 07 那一层一条判据都拿不到。
//!
//! fixture 的形状照真机来（`docs/library-facts.md`）：PS1 / PS2 / PSP 的镜像装在
//! **透明容器**里，PSV 是**目录树转储**（1,416 个变体吞掉 170,794 个文件），
//! GC / Wii 有 RVZ 与 WBFS。

use std::fs;
use std::path::Path;

use romcat_core::catalog::identify::State;
use romcat_core::catalog::{Catalog, Confidence};
use romcat_core::dat::Convention;
use romcat_core::dat::logiqx::{DatHeader, GameRecord, RomRecord};
use romcat_core::dat::repo::{DatMeta, DatRepo, Unit};
use romcat_core::fs::RealFs;
use romcat_core::identify::{self, Options};
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::testing::container::{ZipEntrySpec, zip_container};
use romcat_core::testing::disc;
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::verdict;

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

/// PS1 的盘：根目录下一份 `SYSTEM.CNF`，`BOOT` 行写着序列号。
///
/// **故意撑到 16 MiB**：这张票的那句话（「识别一个 8 GB 的 ISO 不需要读 8 GB」）只有在
/// 盘比读取上限大得多时才证得出来。真库里那 1,755 份 `.iso` 合计 2,566 GiB，
/// 而 `SYSTEM.CNF` 躺在最前面。
fn ps1_iso() -> Vec<u8> {
    let mut image = disc::iso9660(&[(
        "SYSTEM.CNF",
        b"BOOT = cdrom:\\SLPS_021.70;1\r\nTCB = 4\r\nEVENT = 10\r\n".to_vec(),
    )]);
    image.resize(16 << 20, 0);
    image
}

/// PSP 的 UMD：`PSP_GAME/PARAM.SFO` 里写着 `DISC_ID`，主卷描述符里也有一份明文。
fn psp_iso() -> Vec<u8> {
    let mut image = disc::iso9660(&[(
        "PSP_GAME/PARAM.SFO",
        disc::param_sfo(&[
            ("CATEGORY", "UG"),
            ("DISC_ID", "ULJM05800"),
            ("DISC_VERSION", "1.00"),
            ("TITLE", "某个 PSP 游戏"),
        ]),
    )]);
    disc::with_psp_application_use(&mut image, "ULJM-05800|0123456789ABCDEF");
    image
}

/// PSV 的 `param.sfo`：TitleID 与 CONTENT_ID 都在里面。
fn psv_sfo() -> Vec<u8> {
    disc::param_sfo(&[
        ("APP_VER", "01.03"),
        ("CATEGORY", "gd"),
        ("CONTENT_ID", "JP0103-PCSG00245_00-APP0000000000000"),
        ("TITLE", "新・ロロナのアトリエ"),
        ("TITLE_ID", "PCSG00245"),
    ])
}

struct 现场 {
    dir: TempDir,
    catalog: Catalog,
    repo: DatRepo,
}

fn 建现场() -> 现场 {
    let dir = temp_dir("disc-serial");
    let root = dir.path();

    // ── PS1：汉化版，字节改过，CRC 撞不上任何 DAT——**盘里那串序列号没变**。
    写(
        &root.join("ps/某游戏 汉化版.zip"),
        &zip_container(&[ZipEntrySpec::stored("GAME.ISO", ps1_iso())]),
    );
    // ── PSP：一份 UMD ISO 装在容器里，外加一份 PBP 与一份 CSO。
    写(
        &root.join("psp/某游戏.zip"),
        &zip_container(&[ZipEntrySpec::stored("GAME.ISO", psp_iso())]),
    );
    写(
        &root.join("psp/EBOOT.PBP"),
        &disc::pbp(&disc::param_sfo(&[
            ("CATEGORY", "MG"),
            ("DISC_ID", "ULUS10041"),
            ("TITLE", "某个 PBP 游戏"),
        ])),
    );
    写(&root.join("psp/压过的.cso"), &disc::cso(&psp_iso()));
    // ── PSV：**目录树转储**。整个变体里没有一个「整文件」可以算哈希，
    //    锚是那一份 1.6 KB 的 `param.sfo`。
    写(
        &root.join("PSV/A11 新罗罗的炼金工房[PCSG00245][日版]/app/PCSG00245/sce_sys/param.sfo"),
        &psv_sfo(),
    );
    写(
        &root.join("PSV/A11 新罗罗的炼金工房[PCSG00245][日版]/app/PCSG00245/eboot.bin"),
        &vec![0x7Fu8; 4096],
    );
    // ── NGC / WII：压缩镜像。光盘头分别躺在文件偏移 0x58 与 0x200。
    写(
        &root.join("ngc/某游戏.rvz"),
        &disc::rvz(&disc::disc_header("GALE01", "Some GC Game", false)),
    );
    写(
        &root.join("wii/某游戏.wbfs"),
        &disc::wbfs(&disc::disc_header("RMCE01", "Some Wii Game", true)),
    );

    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::new(root);
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &CancelToken::new()).expect("扫得动");

    现场 {
        dir,
        catalog,
        repo: 建_dat(),
    }
}

/// 一条只有序列号、没有可撞哈希的 DAT 条目——真库里 Redump 与 MAME 的形态。
fn 序列号条目(name: &str, serial: &str) -> GameRecord {
    GameRecord {
        name: name.to_string(),
        serial: Some(serial.to_string()),
        roms: vec![RomRecord {
            name: format!("{name}.iso"),
            size: Some(1),
            crc32: Some(0xDEAD_BEEF),
            ..RomRecord::default()
        }],
        ..GameRecord::default()
    }
}

/// 序列号写在 `<game_id>` 子元素里的条目——No-Intro 的 Vita 与 PSP (PSN) 集就是这样。
fn game_id条目(name: &str, game_id: &str) -> GameRecord {
    let mut game = 序列号条目(name, "");
    game.name = name.to_string();
    game.serial = None;
    game.game_id = Some(game_id.to_string());
    game
}

fn 装(repo: &mut DatRepo, source: &str, dat: &str, platform: &str, games: &[GameRecord]) {
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
    // MAME 的 software list 把再版并在一条里，逗号分隔——不拆开就一条都撞不上。
    装(
        &mut repo,
        "MAME",
        "psx.xml",
        "PS1",
        &[序列号条目(
            "Seiken Densetsu - Legend of Mana (Japan)",
            "SLPS-02170, SLPS-02170GH",
        )],
    );
    装(
        &mut repo,
        "No-Intro",
        "Non-Redump - Sony - PlayStation Portable",
        "PSP",
        &[
            序列号条目("Some PSP Game (Japan)", "ULJM-05800"),
            序列号条目("Some PBP Game (USA)", "ULUS-10041"),
        ],
    );
    // Vita 的两份集：VPK 的 `<game_id>` 是 TitleID（精确），PSN Content 的是 CONTENT_ID。
    装(
        &mut repo,
        "No-Intro",
        "Sony - PlayStation Vita (PSN) (Content)",
        "PSV",
        &[
            game_id条目(
                "Shin Rorona no Atelier (Japan)",
                "JP0103-PCSG00245_00-APP0000000000000",
            ),
            game_id条目(
                "Shin Rorona no Atelier - Peach Vacation (Japan) (DLC)",
                "JP0103-PCSG00245_00-SPECIALFREE00001",
            ),
        ],
    );
    // GC / Wii：Redump 的官方 DAT **一条序列号都不写**，所以这里只装 Wii 一条，
    // 好让「读得出标识但撞不上」与「撞得上」两种情形都出现在同一趟里。
    装(
        &mut repo,
        "No-Intro",
        "Non-Redump - Nintendo - Wii",
        "WII",
        &[序列号条目("Some Wii Game (USA)", "RMCE01")],
    );
    repo
}

fn 跑(现场: &mut 现场) -> identify::Outcome {
    let options = Options::new(现场.dir.path());
    identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &现场.repo,
        &verdict::Index::empty(),
        &options,
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别不该失败")
}

fn 候选(现场: &现场, key: &str) -> Vec<romcat_core::catalog::identify::Candidate> {
    现场.catalog.candidates_of(key).expect("读得出")
}

#[test]
fn ps1_的启动配置读得出序列号并撞上_dat() {
    // 这一份是**汉化版**：CRC 与任何 DAT 都对不上，而 `SYSTEM.CNF` 里那串编号没动过。
    let mut 现场 = 建现场();
    跑(&mut 现场);
    let 候选 = 候选(&现场, "ps/某游戏 汉化版.zip");
    let 找到 = 候选
        .iter()
        .find(|c| c.source == "MAME")
        .expect("序列号撞上了 MAME 的 psx.xml");
    assert_eq!(
        找到.confidence,
        Confidence::High,
        "票据：等同于精确哈希命中"
    );
    assert!(找到.accepted);
    assert!(找到.evidence.contains("SLPS-02170"), "{}", 找到.evidence);
    assert!(找到.evidence.contains("SYSTEM.CNF"), "{}", 找到.evidence);
    assert_eq!(
        现场
            .catalog
            .identification_of("ps/某游戏 汉化版.zip")
            .expect("读得出")
            .expect("有结论")
            .0,
        State::Matched
    );
}

#[test]
fn psp_的参数文件读得出光盘标识与标题() {
    let mut 现场 = 建现场();
    跑(&mut 现场);
    let 候选 = 候选(&现场, "psp/某游戏.zip");
    let 找到 = 候选.first().expect("撞上了");
    assert!(找到.accepted);
    // 盘里写的是 `ULJM05800`（`DISC_ID` 不带连字符），DAT 写的是 `ULJM-05800`——
    // 折平之后是同一串，而**候选上记的是盘里那一份**。
    assert_eq!(
        找到.serial.as_deref(),
        Some("ULJM05800"),
        "取的是 `PSP_GAME/PARAM.SFO` 那一条"
    );
    assert!(
        找到.evidence.contains("ULJM-05800"),
        "依据里要写得出 DAT 上那一串：{}",
        找到.evidence
    );
    // 票据要的是「光盘标识与标题标识」——标题读出来了就得看得见。
    assert!(
        找到.evidence.contains("某个 PSP 游戏"),
        "标题该写进依据：{}",
        找到.evidence
    );
}

#[test]
fn pbp_与_cso_都不解全就取得到标识() {
    // **压缩镜像不是容器**（ADR-0014）：这里一个都没被当归档解包，
    // PBP 是读 `param_sfo_offset` 处那段未压缩的明文，CSO 是按索引解头几个块。
    let mut 现场 = 建现场();
    跑(&mut 现场);
    let pbp = 候选(&现场, "psp/EBOOT.PBP");
    assert!(
        pbp.iter().any(|c| c.evidence.contains("PBP 内")),
        "PBP 的 PARAM.SFO 该在固定偏移处读到：{pbp:#?}"
    );
    let cso = 候选(&现场, "psp/压过的.cso");
    assert!(
        cso.iter().any(|c| c.accepted),
        "CSO 解头几个块就该拿到 PSP 的序列号：{cso:#?}"
    );
}

#[test]
fn psv_目录树转储靠_param_sfo_认出来() {
    // 整个变体里没有一个「整文件」可以算哈希——票 07 在它身上只能报「无判据」。
    let key = "PSV/A11 新罗罗的炼金工房[PCSG00245][日版]";
    let mut 现场 = 建现场();
    跑(&mut 现场);
    let 候选 = 候选(&现场, key);
    let 精确 = 候选
        .iter()
        .find(|c| c.accepted)
        .expect("CONTENT_ID 一模一样，那是同一次发行");
    assert_eq!(精确.game, "Shin Rorona no Atelier (Japan)");
    assert!(精确.evidence.contains("CONTENT_ID"), "{}", 精确.evidence);
    // DLC 那一条只对上里面那段 TitleID，**不自动通过**。
    let dlc = 候选
        .iter()
        .find(|c| c.game.contains("DLC"))
        .expect("TitleID 也撞得上它");
    assert!(!dlc.accepted);
    assert_eq!(dlc.confidence, Confidence::Medium);
    assert!(dlc.evidence.contains("里面那一段"), "{}", dlc.evidence);
    assert_eq!(
        现场
            .catalog
            .identification_of(key)
            .expect("读得出")
            .expect("有结论")
            .0,
        State::Matched
    );
}

#[test]
fn 目录名里的_titleid_是一条独立的依据且不自动通过() {
    let key = "PSV/A11 新罗罗的炼金工房[PCSG00245][日版]";
    let mut 现场 = 建现场();
    // 盘里读出来的那条已经把这个 TitleID 占了，所以名字那一条不重复产出——
    // 它要在**盘读不到**时才顶上。`--no-read-library` 正是那种情形，
    // 而且必须是**第一趟**就不读：读过一次之后事实落在中立库里，还是拿得到。
    let mut options = Options::new(现场.dir.path());
    options.read_library = false;
    identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &现场.repo,
        &verdict::Index::empty(),
        &options,
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别不该失败");
    let 候选 = 候选(&现场, key);
    let 名字 = 候选
        .iter()
        .find(|c| c.evidence.contains("名字里直接写着"))
        .expect("目录名里那个 TitleID 是一条独立的依据");
    assert!(!名字.accepted, "目录只是强先验（ADR-0011）");
    assert_eq!(名字.confidence, Confidence::Medium);
}

#[test]
fn 压缩镜像的光盘头在固定偏移上读得到() {
    // RVZ 的光盘头在文件偏移 0x58，WBFS 的在 `hd_sector_size`。两者都**不解压**。
    let mut 现场 = 建现场();
    跑(&mut 现场);
    let wii = 候选(&现场, "wii/某游戏.wbfs");
    let 找到 = wii.first().expect("WBFS 里那份 Wii 光盘头认得出来");
    assert!(找到.evidence.contains("RMCE01"), "{}", 找到.evidence);
    // **NGC 那一份读得出 GALE01，但 DAT 里一条 GC 序列号都没有**——
    // 读得出标识与撞得上是两件事，报告要分得开。
    let (state, _) = 现场
        .catalog
        .identification_of("ngc/某游戏.rvz")
        .expect("读得出")
        .expect("有结论");
    assert_eq!(state, State::Unmatched, "读出了判据，只是 DAT 里没有");
}

#[test]
fn 没验过_nkit_的_gc_与_wii_镜像不许自动通过() {
    // WBFS 只交得出 256 字节的光盘头，够不到光盘逻辑偏移 0x200——**验不了**。
    // 验不了就不自动通过，理由与第一命中层那条一模一样（Dolphin）。
    let mut 现场 = 建现场();
    跑(&mut 现场);
    let wii = 候选(&现场, "wii/某游戏.wbfs");
    let 找到 = wii.first().expect("撞上了");
    assert!(!找到.accepted, "没验过 NKit 就不敢自动通过");
    assert!(找到.evidence.contains("NKit"), "{}", 找到.evidence);
    assert_eq!(找到.confidence, Confidence::Medium);
}

#[test]
fn 光盘那一层第二趟不再读一遍盘() {
    // **读过的盘不白读**（挂账 D14）：探出来的事实落在中立库里，按文件的三元组作废。
    let mut 现场 = 建现场();
    let 首趟 = 跑(&mut 现场);
    assert!(首趟.probed > 0, "第一趟该探到东西");
    assert!(首趟.read_bytes > 0);
    let 第二趟 = 跑(&mut 现场);
    assert_eq!(第二趟.read_bytes, 0, "第二趟一个字节都不该读");
    assert_eq!(第二趟.probed, 首趟.probed, "探出来的还是那些");
    assert_eq!(第二趟.serial_candidates, 首趟.serial_candidates);
}

#[test]
fn 只读几百字节就认出来() {
    // 这张票的那句话要有可断言的证据：整个 fixture 主库里那几份光盘加起来多大，
    // 这一趟读了多少。
    let mut 现场 = 建现场();
    let outcome = 跑(&mut 现场);
    let 库大小: u64 = walk(现场.dir.path());
    assert!(
        outcome.read_bytes * 3 < 库大小,
        "读了 {} / 库共 {}——这一层的全部意义就是不读完",
        outcome.read_bytes,
        库大小
    );
    assert!(outcome.with_id > 0, "读出来的标识不该是 0");
}

fn walk(dir: &Path) -> u64 {
    let mut total = 0;
    for entry in fs::read_dir(dir).expect("列得动").flatten() {
        let meta = entry.metadata().expect("看得见");
        total += if meta.is_dir() {
            walk(&entry.path())
        } else {
            meta.len()
        };
    }
    total
}
