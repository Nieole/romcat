//! **卡带内部头那一层**：磁盘上摆一份卡带世代的主库，跑一遍识别。
//!
//! 这里要证的是票 10 的那句话：**汉化补丁通常不改内部头，所以一份对不上任何数据库的
//! 汉化版照样说得出它基于哪一次发行。** 密集逻辑挂在纯函数上（`identify::cart` 自己有
//! 一整套用真实 ROM 头部字节写的单元测试），这一份证的是它真的接在了扫描、成型、
//! DAT 库与中立库之间。
//!
//! **fixture 的头部字节全部来自真主库**（`testing::cart`）：六个平台六份汉化版，
//! 一份 FC 原版。手工编造的头永远是「文档说该长什么样」，而这一层要对付的恰恰是
//! 真实世界里长得不太一样的那些。

use std::fs;
use std::path::Path;

use romcat_core::catalog::identify::State;
use romcat_core::task::Handle;
use romcat_core::catalog::{Catalog, Confidence, Roots};
use romcat_core::dat::Convention;
use romcat_core::dat::logiqx::{DatHeader, GameRecord, RomRecord};
use romcat_core::dat::repo::{DatMeta, DatRepo, Unit};
use romcat_core::fs::RealFs;
use romcat_core::identify::fuzzy;
use romcat_core::identify::{self, Options};
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::testing::cart as real;
use romcat_core::testing::container::{ZipEntrySpec, zip_container};
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::verdict;

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

/// 一份带 iNES 外挂头的 FC 卡带。`tail` 是头后面那串字节——**SHA-1 那一层撞的就是它**。
fn nes(tail: u8) -> Vec<u8> {
    let mut rom = real::NES_KUAIJIE.to_vec();
    rom.resize(16 + 40_960, tail);
    rom
}

/// 一份**官中版** GBC 卡：字节没被改过，DAT 里有它自己那一条。
fn 官中版() -> Vec<u8> {
    let mut rom = real::GBC_TWINE.to_vec();
    rom.resize(1 << 15, 0x5A);
    rom
}

/// 这份内容的 CRC-32。
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = flate2::Crc::new();
    crc.update(bytes);
    crc.sum()
}

/// 这份内容的 SHA-1，写成 DAT 里那种四十位小写十六进制。
fn sha1(bytes: &[u8]) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY, bytes);
    digest.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

struct 现场 {
    dir: TempDir,
    catalog: Catalog,
    repo: DatRepo,
}

fn 建现场() -> 现场 {
    let dir = temp_dir("cart-header");
    let root = dir.path();

    // ── GBA：裸的汉化版。字节改过，CRC 撞不上任何 DAT——**Game Code `BR6J` 没动**。
    写(
        &root.join("gba/洛克人EXE6 电脑兽法尔扎 汉化版.gba"),
        &real::padded(&real::GBA_ROCKMAN_EXE6, 1 << 20),
    );
    // ── NDS：汉化版装在**透明容器**里。容器给得出 CRC-32，但撞不上任何一条。
    写(
        &root.join("nds/逆转检事 完美汉化版.zip"),
        &zip_container(&[ZipEntrySpec::stored(
            "逆转检事 完美汉化版.nds",
            real::padded(&real::NDS_GYAKUTEN_KENJI, 1 << 16),
        )]),
    );
    // ── GBC：汉化版。票 07 那一层这个平台的命中率是 0%。
    写(
        &root.join("gbc/007 黑日危机 繁体修正版.gbc"),
        &real::padded(&real::GBC_TWINE, 1 << 16),
    );
    // ── MD：汉化版。海外标题被改成了 GBK 中文，产品码原封不动。
    写(
        &root.join("MD/光明力量2 古代之封印 简体汉化版.md"),
        &real::padded(&real::MD_SHINING_FORCE_2, 1 << 16),
    );
    // ── SFC：同一份内部头，前面挂 512 字节**拷贝机头**。解析前要剥掉。
    let mut smc = vec![0u8; 0x200];
    smc.extend_from_slice(&real::snes_rom());
    写(&root.join("SFC/3x3只眼 兽魔奉还 汉化版.smc"), &smc);
    // ── N64：`.v64` 的**半字交换**排法。解析前要归一化成大端 z64。
    写(
        &root.join("n64/Paper Mario 汉化版.v64"),
        &real::swapped16(&real::padded(&real::N64_PAPER_MARIO, 1 << 16)),
    );
    // ── FC：iNES 头里没有编号，这一层给不出候选。它撞的是 **GoodNES 那条 SHA-1 窄路**。
    写(&root.join("FC/1942 汉化版.nes"), &nes(0x42));
    // ── 官中版：**原厂发行的官方中文版**。在卡带世代它有独立序列号、DAT 里独立一条，
    //    **精确哈希直接过**（ADR-0012、ADR-0019）。这里让它按原样 CRC 命中。
    写(&root.join("gbc/官方中文版.gbc"), &官中版());
    // ── 冲突：**目录说 GBA，文件里躺着的是一张 NDS 卡**（ADR-0011）。
    写(
        &root.join("gba/下错了的.gba"),
        &real::padded(&real::NDS_GYAKUTEN_KENJI, 1 << 16),
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

/// 一条 No-Intro 的卡带条目：序列号写在 `<rom serial>` 上，哈希是原版的——
/// 而磁盘上那份是汉化版，**撞不上**。
fn 卡带条目(name: &str, serial: &str) -> GameRecord {
    GameRecord {
        name: name.to_string(),
        roms: vec![RomRecord {
            name: format!("{name}.rom"),
            size: Some(1),
            crc32: Some(0xDEAD_BEEF),
            serial: Some(serial.to_string()),
            ..RomRecord::default()
        }],
        ..GameRecord::default()
    }
}

/// 一条 **GoodNES 形态**的条目：`crc32` 与 `size` 两列都是空的，只有 SHA-1。
fn 只有_sha1的条目(name: &str, sha: &str) -> GameRecord {
    GameRecord {
        name: name.to_string(),
        roms: vec![RomRecord {
            name: name.to_string(),
            size: None,
            crc32: None,
            sha1: Some(sha.to_string()),
            ..RomRecord::default()
        }],
        ..GameRecord::default()
    }
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
    装(
        &mut repo,
        "No-Intro",
        "Nintendo - Game Boy Advance",
        "GBA",
        &[卡带条目(
            "Rockman EXE 6 - Dennoujuu Falzar (Japan)",
            "BR6J",
        )],
    );
    装(
        &mut repo,
        "No-Intro",
        "Nintendo - Nintendo DS (Decrypted)",
        "NDS",
        &[卡带条目("Gyakuten Kenji (Japan)", "C32J")],
    );
    装(
        &mut repo,
        "No-Intro",
        "Nintendo - Game Boy Color",
        "GBC",
        &[
            卡带条目("007 - The World Is Not Enough (USA, Europe)", "BO7E"),
            // **官中版**：No-Intro / Redump 用 `(Zh)` 这样的语言标记组正常收录它，
            // 而它就是一次独立的官方发行——精确哈希直接命中（ADR-0012）。
            GameRecord {
                name: "Some Official Chinese Game (China) (Zh)".to_string(),
                roms: vec![RomRecord {
                    name: "Some Official Chinese Game (China) (Zh).gbc".to_string(),
                    size: Some(1 << 15),
                    crc32: Some(crc32(&官中版())),
                    serial: Some("BCNC".to_string()),
                    ..RomRecord::default()
                }],
                ..GameRecord::default()
            },
        ],
    );
    装(
        &mut repo,
        "No-Intro",
        "Sega - Mega Drive - Genesis",
        "MD",
        &[卡带条目(
            "Shining Force II - Inishie no Fuuin (Japan)",
            "G-5521-00",
        )],
    );
    // **同一串 `A83J` 在两个平台上各有一条**——4 个字符的游戏码跨平台撞车是现实存在的。
    // 内部头说这张卡是 SFC 的，GBA 那一条就该被圈在外面。
    装(
        &mut repo,
        "No-Intro",
        "Nintendo - Super Nintendo Entertainment System",
        "SFC",
        &[卡带条目("3x3 Eyes - Juuma Houkan (Japan)", "A83J")],
    );
    装(
        &mut repo,
        "No-Intro",
        "Nintendo - Nintendo 64 (BigEndian)",
        "N64",
        &[卡带条目("Paper Mario (USA)", "NMQE")],
    );
    // GoodNES：**只记 SHA-1**，第一命中层够不到它。汉化条目正是这一批里的 646 条。
    装(
        &mut repo,
        "GoodNES",
        "gamedb_goodnes.txt",
        "FC",
        &[只有_sha1的条目(
            "1942 (JU) [T+Chi_MS emumax]",
            &sha1(&nes(0x42)),
        )],
    );
    repo
}

fn 跑一趟(现场: &mut 现场) -> identify::Outcome {
    let options = Options::new(Roots::single("库", 现场.dir.path()));
    identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &identify::Ammo {
            repo: &现场.repo,
            verdicts: &verdict::Index::default(),
            naming: &fuzzy::Naming::off(),
            guessing: &identify::model::Guessing::off(),
            titledb: None,
        },
        &options,
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别跑得动")
}

/// 一个变体这一轮的结论与候选，一起取回来。
fn 结论(catalog: &Catalog, key: &str) -> (State, Vec<romcat_core::catalog::identify::Candidate>) {
    let (state, _reason) = catalog
        .identification_of(key)
        .expect("读得出结论")
        .unwrap_or_else(|| panic!("{key} 该有一条结论"));
    (state, catalog.candidates_of(key).expect("读得出候选"))
}

fn 变体键(catalog: &Catalog, 片段: &str) -> String {
    catalog
        .variants()
        .expect("读得出变体")
        .into_iter()
        .find(|variant| variant.key.contains(片段))
        .unwrap_or_else(|| panic!("库里该有一个键里带「{片段}」的变体"))
        .key
}

#[test]
fn 汉化版哈希撞不上但内部头说得出它基于哪一次发行() {
    let mut 现场 = 建现场();
    let outcome = 跑一趟(&mut 现场);

    // 六个平台，每一个都该靠内部头认出来。GBA 与 NDS 是中文玩家存量最大的两个平台，
    // 票 07 在它们身上的命中率是 5.8% 与 4.4%。
    for (片段, 条目) in [
        ("洛克人EXE6", "Rockman EXE 6 - Dennoujuu Falzar (Japan)"),
        ("逆转检事", "Gyakuten Kenji (Japan)"),
        (
            "007 黑日危机",
            "007 - The World Is Not Enough (USA, Europe)",
        ),
        ("光明力量2", "Shining Force II - Inishie no Fuuin (Japan)"),
        ("3x3只眼", "3x3 Eyes - Juuma Houkan (Japan)"),
        ("Paper Mario", "Paper Mario (USA)"),
    ] {
        let key = 变体键(&现场.catalog, 片段);
        let (state, candidates) = 结论(&现场.catalog, &key);
        assert_eq!(state, State::Matched, "{片段}");
        assert!(
            candidates.iter().any(|it| it.game == 条目),
            "{片段} 该撞上《{条目}》，实际是 {:?}",
            candidates
                .iter()
                .map(|it| it.game.as_str())
                .collect::<Vec<_>>()
        );
    }
    assert!(outcome.cart.with_id >= 6, "六份都该读出游戏码");
    assert!(outcome.cart.only >= 6, "六份都是只靠内部头才认出来的");
}

#[test]
fn 内部头这一层只说得到发行版这一层不自动通过() {
    // 票据原话：「内部头命中产出发行版级候选，置信度低于精确哈希但高于文件名」。
    // 汉化补丁不改卡带头，同一个游戏码底下躺着原版和一堆汉化版（ADR-0008）。
    let mut 现场 = 建现场();
    跑一趟(&mut 现场);
    let key = 变体键(&现场.catalog, "洛克人EXE6");
    let (_, candidates) = 结论(&现场.catalog, &key);
    let candidate = candidates
        .iter()
        .find(|it| it.game.contains("Rockman"))
        .expect("有那条候选");
    assert_eq!(candidate.confidence, Confidence::Medium);
    assert!(!candidate.accepted, "不自动通过，进待确认队列");
    assert!(
        candidate.evidence.contains("发行版"),
        "依据要说清楚为什么只到发行版：{}",
        candidate.evidence
    );
    // 没有自动通过的候选，就没有发行版那一行——它等裁决补。
    assert_eq!(candidate.release_id, None);
}

#[test]
fn 四字符游戏码不许撞到别的机器的_dat_上() {
    // `A83J` 在 SFC 与 GBA 各有一条。内部头说这张卡是 SFC 的，GBA 那一条是撞车不是候选。
    let mut 现场 = 建现场();
    跑一趟(&mut 现场);
    let key = 变体键(&现场.catalog, "3x3只眼");
    let (_, candidates) = 结论(&现场.catalog, &key);
    assert!(!candidates.is_empty());
    assert!(candidates.iter().all(|it| it.platform == "SFC"));
}

#[test]
fn 内部头与目录声明的平台冲突记下来() {
    // ADR-0011：目录只是强先验，文件内容可以推翻它。这类冲突不是错误，
    // 是库体检最该报告的产出之一。
    let mut 现场 = 建现场();
    let outcome = 跑一趟(&mut 现场);
    assert!(outcome.cart.conflicts >= 1, "该报出至少一条冲突");
    let conflicts = 现场.catalog.platform_conflicts(10).expect("查得出");
    let found = conflicts
        .iter()
        .find(|it| it.variant_key.contains("下错了的"))
        .expect("那一份该在里面");
    assert_eq!(found.declared, "GBA");
    assert_eq!(found.found, "NDS");
}

#[test]
fn goodnes_那批只记_sha1_的记录靠_sha1_才撞得上() {
    // 它们连 `crc32` 与 `size` 两列都是空的——第一命中层不是撞不上，是**根本没有可以
    // 对的那一列**。卡带 ROM 体积极小，整份读一遍算得起。
    let mut 现场 = 建现场();
    let outcome = 跑一趟(&mut 现场);
    let key = 变体键(&现场.catalog, "1942");
    let (state, candidates) = 结论(&现场.catalog, &key);
    assert_eq!(state, State::Matched);
    let candidate = candidates
        .iter()
        .find(|it| it.source == "GoodNES")
        .expect("该有一条 GoodNES 的候选");
    assert!(candidate.accepted, "SHA-1 对上就是精确命中");
    assert_eq!(candidate.confidence, Confidence::High);
    assert!(
        candidate.evidence.contains("SHA-1"),
        "{}",
        candidate.evidence
    );
    assert!(outcome.sha1_hits >= 1);
    assert!(outcome.sha1_only >= 1);
}

#[test]
fn 第二趟不再为这一层读一个字节() {
    // 探出来的事实落在中立库的 `content_cart` 里，按文件的三元组作废（挂账 D14）。
    let mut 现场 = 建现场();
    let first = 跑一趟(&mut 现场);
    assert!(first.read_bytes > 0, "第一趟当然要读");
    let second = 跑一趟(&mut 现场);
    assert_eq!(second.read_bytes, 0, "第二趟一个字节都不该读");
    assert_eq!(second.cart.with_id, first.cart.with_id);
    assert_eq!(second.cart.only, first.cart.only);
}

#[test]
fn 盘不在位时这一层如实报没读到而不是瞎猜() {
    // ADR-0021 的第三态：读不到不是结论，不落库。
    let mut 现场 = 建现场();
    let mut options = Options::new(Roots::single("库", 现场.dir.path()));
    options.read_library = false;
    let outcome = identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &identify::Ammo {
            repo: &现场.repo,
            verdicts: &verdict::Index::default(),
            naming: &fuzzy::Naming::off(),
            guessing: &identify::model::Guessing::off(),
            titledb: None,
        },
        &options,
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("跑得动");
    assert_eq!(outcome.cart.probed, 0);
    assert_eq!(outcome.read_bytes, 0);
    let key = 变体键(&现场.catalog, "洛克人EXE6");
    let (state, _) = 结论(&现场.catalog, &key);
    assert_ne!(state, State::Matched, "没读盘就认出来才是瞎猜");
}

#[test]
fn 官中版是发行版汉化版是变体() {
    // ADR-0012 与 ADR-0019：**这张票覆盖的全是卡带世代，官中在这里确实是独立发行版**
    // ——有独立序列号、DAT 里独立一条，走精确哈希直接过。汉化版是**变体**，
    // 认得出它基于哪部作品，认不出它基于哪一条发行版，那条留空等裁决。
    let mut 现场 = 建现场();
    跑一趟(&mut 现场);

    // 官中版：精确哈希命中，自动通过，**立起一条发行版**。
    let 官中 = 变体键(&现场.catalog, "官方中文版");
    let (state, candidates) = 结论(&现场.catalog, &官中);
    assert_eq!(state, State::Matched);
    let candidate = candidates
        .iter()
        .find(|it| it.game.contains("Official Chinese"))
        .expect("该撞上那一条");
    assert!(candidate.accepted, "官中版走精确哈希直接过");
    assert_eq!(
        candidate.chinese,
        Some(romcat_core::dat::ChineseMark::Official)
    );
    assert!(
        candidate.release_id.is_some(),
        "官中版在卡带世代是一条独立的发行版"
    );

    // 汉化版：撞上了 GoodNES 那条 `[T+Chi]`，认得出是哪部作品——**但发行版留空**。
    let 汉化 = 变体键(&现场.catalog, "1942");
    let (_, candidates) = 结论(&现场.catalog, &汉化);
    let candidate = candidates
        .iter()
        .find(|it| it.source == "GoodNES")
        .expect("该有那一条");
    assert_eq!(
        candidate.chinese,
        Some(romcat_core::dat::ChineseMark::FanTranslated)
    );
    assert_eq!(
        candidate.release_id, None,
        "汉化版是变体，认不出它基于哪一条发行版——那条留空等裁决"
    );
}
