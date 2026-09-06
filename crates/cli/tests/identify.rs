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
        "--root-name",
        "库",
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
    let 候选 = catalog.candidates_of("库/FC/游戏.zip").expect("读得出");
    assert_eq!(候选.len(), 1);
    assert!(候选[0].accepted, "精确命中自动通过");
    assert!(候选[0].evidence.contains("CRC-32"), "{}", 候选[0].evidence);
    let 变体 = catalog
        .variant("库/FC/游戏.zip")
        .expect("读得出")
        .expect("在");
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

/// 主库里再摆一份**认不出来的**：模型推断兜底那一层的对象。
fn 现场加残渣() -> (TempDir, TempDir) {
    let (library, workspace) = 现场();
    let zip = zip_container(&[ZipEntrySpec::stored("x.nes", {
        let mut data = 卡带();
        // 改的必须是**头之后**那一段：iNES 的 16 字节头会被去头那一套哈希剥掉，
        // 只改头里的字节，去头之后两份内容一模一样，照样撞得上 DAT。
        data[100] = 0x7F;
        data
    })]);
    fs::write(library.path().join("FC/033.動作：掃地雷.zip"), &zip).expect("能写");
    (library, workspace)
}

#[test]
fn 模型推断只算计划那一趟一个请求都不发() {
    let (library, workspace) = 现场加残渣();
    扫(library.path(), workspace.path());
    let out = 跑(&[
        "identify",
        &library.path().to_string_lossy(),
        "--library",
        "小库",
        "--workspace",
        &workspace.path().to_string_lossy(),
        "--model-plan",
        "--quiet",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let 话 = String::from_utf8_lossy(&out.stderr);
    assert!(
        话.contains("只算计划（--model-plan），一个请求都不发"),
        "{话}"
    );
    // 花费**在发第一个请求之前**就说得出来，而且下界与上界都给。
    assert!(话.contains("模型推断兜底"), "{话}");
    assert!(话.contains("计划（第一个请求发出去之前就算得出）"), "{话}");
    assert!(话.contains("美元 到 "), "{话}");
    // 报告必须当场说清这一层的性质：几千条模型编的东西将要出现在队列里。
    assert!(话.contains("一条都不自动通过"), "{话}");
    assert!(话.contains("待确认队列"), "{话}");
}

#[test]
fn 没有凭据时这一层不启动_也绝不匿名试探() {
    let (library, workspace) = 现场加残渣();
    扫(library.path(), workspace.path());
    let out = Command::new(env!("CARGO_BIN_EXE_romcat"))
        .args([
            "identify",
            &library.path().to_string_lossy(),
            "--library",
            "小库",
            "--workspace",
            &workspace.path().to_string_lossy(),
            "--model",
            "--quiet",
        ])
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("ANTHROPIC_AUTH_TOKEN")
        .output()
        .expect("跑得起来");
    assert!(!out.status.success(), "没有凭据不该当成跑成了");
    let 话 = String::from_utf8_lossy(&out.stderr);
    assert!(话.contains("ANTHROPIC_API_KEY"), "{话}");
    assert!(话.contains("不匿名试探"), "{话}");
    // 说得出下一步：只想看看花多少钱有 --model-plan。
    assert!(话.contains("--model-plan"), "{话}");
}

#[test]
fn 价目表里没有这个模型就整层不启动() {
    let (library, workspace) = 现场加残渣();
    扫(library.path(), workspace.path());
    let out = 跑(&[
        "identify",
        &library.path().to_string_lossy(),
        "--library",
        "小库",
        "--workspace",
        &workspace.path().to_string_lossy(),
        "--model-plan",
        "--model-id",
        "没见过的模型",
        "--quiet",
    ]);
    assert!(!out.status.success());
    let 话 = String::from_utf8_lossy(&out.stderr);
    // 不知道 token 值多少钱，「花费上限」就是一句空话——所以是**不启动**，
    // 不是「跑起来但花费那一栏写说不出」。
    assert!(话.contains("花费上限设不了"), "{话}");
    assert!(话.contains("claude-opus-5"), "报错要列出表里有哪些：{话}");
}

#[test]
fn 价目表导得出底稿() {
    let workspace = temp_dir("pricing");
    let path = workspace.path().join("pricing.toml");
    let out = 跑(&["identify", "--model-dump-pricing", &path.to_string_lossy()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = fs::read_to_string(&path).expect("写下来了");
    assert!(text.contains("claude-opus-5"));
    assert!(text.contains("核实日期"), "价目表必须自带核实日期");
}
