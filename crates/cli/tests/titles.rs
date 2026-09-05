//! 端到端验收 `romcat titles` 这一层：它**一个字节都不读主库**，报告说得出多少个作品
//! 拿到了中文显示标题，而且**盘不在位时照样出得来**——标题集合是从中立库折出来的
//! （ADR-0001、ADR-0009）。
//!
//! 挑标题的规则本身由 `romcat-core` 那一侧验（`crates/core/tests/titles.rs` 与
//! `title::choose` 的单元测试）。这个文件只验命令行这一层：找得到库、报告印得出来、
//! 库里还没有作品时说得清下一步该干什么。

use std::fs;
use std::path::Path;
use std::process::Command;

use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

/// 一个小 fixture 主库 + 一份扫过的中立库。
fn 现场() -> (TempDir, TempDir) {
    let dir = temp_dir("titles-cli");
    let workspace = temp_dir("titles-cli-ws");
    写(&dir.path().join("FC/魂斗罗.zip"), &zip(2048));
    let out = Command::new(env!("CARGO_BIN_EXE_romcat"))
        .arg("scan")
        .args(["--root-name", "库"])
        .arg(dir.path())
        .arg("--workspace")
        .arg(workspace.path())
        .args(["--library", "测试库", "--no-checkpoint", "--quiet"])
        .output()
        .expect("能启动 romcat");
    assert!(out.status.success(), "扫描该成功");
    (dir, workspace)
}

fn 折(workspace: &Path, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_romcat"))
        .arg("titles")
        .args(["--library", "测试库", "--workspace"])
        .arg(workspace)
        .args(extra)
        .output()
        .expect("能启动 romcat")
}

#[test]
fn 折标题不读主库_盘不在位也出得来() {
    let (dir, workspace) = 现场();
    // 把主库整个挪走：`titles` 连主库根都不需要知道。
    drop(dir);
    let out = 折(workspace.path(), &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("标题集合与显示、排序标题"), "{text}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("一个字节都没读主库"), "{err}");
}

#[test]
fn 库里还没有作品时说得清下一步() {
    let (_dir, workspace) = 现场();
    let out = 折(workspace.path(), &[]);
    assert!(out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    // 只扫过、没识别过：作品那张表是空的，标题挂在作品上。
    assert!(err.contains("romcat identify"), "{err}");
    assert!(err.contains("romcat scrape"), "{err}");
}

#[test]
fn 报告里那个中文覆盖数导得出_json() {
    let (_dir, workspace) = 现场();
    let json = workspace.path().join("titles.json");
    let out = 折(
        workspace.path(),
        &["--quiet", "--json", &json.to_string_lossy()],
    );
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stdout).is_empty(),
        "--quiet 就该一个字都不往标准输出打"
    );
    let text = fs::read_to_string(&json).expect("报告写出来了");
    let value: serde_json::Value = serde_json::from_str(&text).expect("是合法 JSON");
    assert!(
        value.get("chinese_works").is_some(),
        "「多少个作品拿到了中文显示标题」是这张票对用户的意义，报告里必须有：{text}"
    );
}

#[test]
fn 没有这份中立库时说得清先跑什么() {
    let workspace = temp_dir("titles-cli-empty");
    let out = 折(workspace.path(), &[]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("romcat scan"), "{err}");
}
