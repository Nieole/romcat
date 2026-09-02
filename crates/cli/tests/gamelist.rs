//! 端到端验收 ES gamelist 那一侧的 `romcat adapters` / `import` / `export`。
//!
//! 往返词法、用户状态搬运与布局那几条由 `romcat-core` 那一侧验
//! （`crates/core/tests/gamelist.rs` 与 `adapter::gamelist` 的单元测试）。这个文件
//! 只验命令行：**能力档位对用户可见**、两个根元素的文件在真的命令行上也读得动、
//! 导入不改那份文件、导出落在 `gamelists/<平台目录>/` 下而且一个用户状态元素都不生成。

use std::fs;
use std::path::Path;
use std::process::Command;

use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

fn romcat(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_romcat"))
        .args(args)
        .output()
        .expect("能启动 romcat")
}

/// 一个小 fixture 主库 + 一份扫过的中立库。
fn 现场() -> (TempDir, TempDir) {
    let dir = temp_dir("gamelist-cli");
    let workspace = temp_dir("gamelist-cli-ws");
    写(&dir.path().join("FC/魂斗罗.zip"), &zip(2048));
    let out = romcat(&[
        "scan",
        &dir.path().to_string_lossy(),
        "--workspace",
        &workspace.path().to_string_lossy(),
        "--library",
        "测试库",
        "--no-checkpoint",
        "--quiet",
    ]);
    assert!(out.status.success(), "扫描该成功");
    (dir, workspace)
}

#[test]
fn 能力档位对用户可见() {
    let out = romcat(&["adapters"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("ES-Gamelist"), "{text}");
    assert!(text.contains("gamelist.xml"), "{text}");
}

#[test]
fn 导入别人分享的_gamelist_把用户状态搬运的账说出口() {
    let (dir, workspace) = 现场();
    // 照 ES-DE 的布局摆：`gamelists/<平台目录>/gamelist.xml`，`<path>` 相对那个目录。
    let path = dir.path().join("gamelists/FC/gamelist.xml");
    let 原文 = "<?xml version=\"1.0\"?>\n\
        <alternativeEmulator>\n\
        \x20   <label>Nestopia UE</label>\n\
        </alternativeEmulator>\n\
        <gameList>\n\
        \x20   <game>\n\
        \x20       <path>./魂斗罗.zip</path>\n\
        \x20       <name>魂斗罗（台版）</name>\n\
        \x20       <favorite>true</favorite>\n\
        \x20       <playcount>137</playcount>\n\
        \x20       <lastplayed>20240115T203000</lastplayed>\n\
        \x20   </game>\n\
        </gameList>\n";
    写(&path, 原文.as_bytes());

    let out = romcat(&[
        "import",
        "--format",
        "ES-Gamelist",
        &path.to_string_lossy(),
        "--root",
        &dir.path().to_string_lossy(),
        "--library",
        "测试库",
        "--workspace",
        &workspace.path().to_string_lossy(),
        "--dry-run",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    // ⚠️ 两个根元素的文件读得动，而且往返逐字节相同。
    assert!(text.contains("无损往返"), "{text}");
    // **搬运的账要说出口**：省略这些元素等于把维护者的收藏与游玩记录清零。
    assert!(text.contains("用户状态"), "{text}");
    assert!(text.contains("3 处"), "{text}");
    assert_eq!(
        fs::read_to_string(&path).expect("读得出"),
        原文,
        "导入不改那份文件"
    );
}

#[test]
fn 导出_gamelist_落在_gamelists_下每个平台目录一份() {
    let (dir, workspace) = 现场();
    let out_dir = temp_dir("gamelist-cli-out");
    let out = romcat(&[
        "export",
        "--format",
        "ES-Gamelist",
        "--out",
        &out_dir.path().to_string_lossy(),
        "--library",
        "测试库",
        "--workspace",
        &workspace.path().to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let target = out_dir.path().join("gamelists/FC/gamelist.xml");
    let 写出来的 = fs::read_to_string(&target).expect("导出来的文件在");
    assert!(写出来的.starts_with("<?xml"), "{写出来的}");
    assert!(写出来的.contains("<gameList>"), "{写出来的}");
    assert!(写出来的.contains("<path>./魂斗罗.zip</path>"), "{写出来的}");
    // **一个用户状态元素都不生成**（ADR-0006）。
    for 不该有 in ["<favorite>", "<playcount>", "<lastplayed>", "<playtime>"] {
        assert!(!写出来的.contains(不该有), "{不该有}：{写出来的}");
    }
    let _ = dir;
}
