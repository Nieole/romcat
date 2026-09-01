//! 端到端验收 `romcat identify`：扫一遍小库、装一份迷你 DAT、撞出命中率。
//!
//! 密集逻辑在核心库那一侧（`romcat-core` 的 `tests/identify.rs`）。这里只验命令行
//! 这一层的三件事：**不联网**、结论落进中立库、以及话说得对不对。

use std::fs;
use std::path::Path;
use std::process::Command;

use romcat_core::catalog::Catalog;
use romcat_core::dat::Convention;
use romcat_core::dat::logiqx::{DatHeader, GameRecord, RomRecord};
use romcat_core::dat::repo::{DatMeta, DatRepo, Unit};
use romcat_core::testing::container::{ZipEntrySpec, crc32, zip_container};
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::workspace;

fn 跑(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_romcat"))
        .args(args)
        .output()
        .expect("跑得起来")
}

fn 卡带() -> Vec<u8> {
    let mut data = vec![0u8; 16];
    data[..4].copy_from_slice(b"NES\x1A");
    data.extend(std::iter::repeat_n(0x5Au8, 40_960));
    data
}

/// 一个小主库 + 一份装好 DAT 的工作目录。
fn 现场() -> (TempDir, TempDir) {
    let library = temp_dir("identify-cli-库");
    let workspace = temp_dir("identify-cli-工作");
    let zip = zip_container(&[ZipEntrySpec::stored("Game (Japan).nes", 卡带())]);
    fs::create_dir_all(library.path().join("FC")).expect("能建目录");
    fs::write(library.path().join("FC/游戏.zip"), &zip).expect("能写");

    let mut repo = DatRepo::open(&workspace::dat_repo_path(workspace.path())).expect("开得出来");
    let mut writer = repo
        .begin(&Unit {
            source: "TOSEC".to_string(),
            name: "fc.dat".to_string(),
            url: "https://example.invalid/x".to_string(),
            fingerprint: "sha".to_string(),
        })
        .expect("事务");
    writer
        .write_dat(
            &DatMeta {
                name: "Nintendo Famicom - Games".to_string(),
                platform: "FC".to_string(),
                convention: Convention::AsIs,
                header: DatHeader::default(),
            },
            &[GameRecord {
                name: "Game (1990)(Someone)".to_string(),
                roms: vec![RomRecord {
                    name: "Game.nes".to_string(),
                    size: Some(40_976),
                    crc32: Some(crc32(&卡带())),
                    ..RomRecord::default()
                }],
                ..GameRecord::default()
            }],
        )
        .expect("写");
    writer.commit().expect("提交");
    (library, workspace)
}

fn 扫(library: &Path, workspace: &Path) {
    let out = 跑(&[
        "scan",
        &library.to_string_lossy(),
        "--library",
        "小库",
        "--workspace",
        &workspace.to_string_lossy(),
        "--quiet",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn 识别跑通并把命中率打出来() {
    let (library, workspace) = 现场();
    扫(library.path(), workspace.path());

    let out = 跑(&[
        "identify",
        &library.path().to_string_lossy(),
        "--library",
        "小库",
        "--workspace",
        &workspace.path().to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("命中率"), "{text}");
    assert!(text.contains("FC"), "{text}");
    assert!(text.contains("TOSEC"), "{text}");

    // 结论落进中立库：候选、依据、作品与发行版都在。
    let catalog = Catalog::open(&workspace::catalog_path(
        workspace.path(),
        workspace::Slug::Named("小库"),
    ))
    .expect("开得出中立库");
    let 候选 = catalog.candidates_of("FC/游戏.zip").expect("读得出");
    assert_eq!(候选.len(), 1);
    assert!(候选[0].accepted, "精确命中自动通过");
    assert!(候选[0].evidence.contains("CRC-32"), "{}", 候选[0].evidence);
    let 变体 = catalog.variant("FC/游戏.zip").expect("读得出").expect("在");
    assert!(变体.work_id.is_some() && 变体.release_id.is_some());
}

#[test]
fn 没有_dat_库时说得清下一步() {
    let (library, workspace) = 现场();
    fs::remove_file(workspace::dat_repo_path(workspace.path())).expect("删得掉");
    扫(library.path(), workspace.path());
    let out = 跑(&[
        "identify",
        &library.path().to_string_lossy(),
        "--library",
        "小库",
        "--workspace",
        &workspace.path().to_string_lossy(),
    ]);
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("romcat dat sync"), "{text}");
}

#[test]
fn 报告不许写进主库() {
    // 主库只读（ADR-0004）：识别要读字节，这道守卫更不能少。
    let (library, workspace) = 现场();
    扫(library.path(), workspace.path());
    let out = 跑(&[
        "identify",
        &library.path().to_string_lossy(),
        "--library",
        "小库",
        "--workspace",
        &workspace.path().to_string_lossy(),
        "--json",
        &library.path().join("识别.json").to_string_lossy(),
    ]);
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("主库只读"), "{text}");
}

#[test]
fn 不读主库那条路盘不在位也跑得动() {
    let (library, workspace) = 现场();
    扫(library.path(), workspace.path());
    // 只给名字，不给主库路径——盘不在位时就是这样。
    let out = 跑(&[
        "identify",
        "--library",
        "小库",
        "--workspace",
        &workspace.path().to_string_lossy(),
        "--no-read-library",
        "--quiet",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("回盘读了 0 B"), "{text}");
}
