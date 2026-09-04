//! 端到端验收「两套策略档案在**任务级别**手动切换」（ADR-0007）。
//!
//! 这一条的落点在命令行上：`--profile` 是一趟一趟给的，不是一个记在什么地方的全局开关。
//! 而在线档赌上的是用户的账号与 IP（ScreenScraper 的配额同时按账号与 IP 计，撞穿了
//! 永久封禁），所以它**起不来的时候必须说得清清楚楚，而不是悄悄退回离线跑完**——
//! 用户对着一份缺封面的报告，会以为在线源也没有。
//!
//! **这里一个网络请求都不发。** 真实凭据这一趟拿不到，也不该去申请；在线档拿到凭据
//! 之后的行为由 `romcat-core` 那一侧对着假服务器验（`crates/core/tests/scrape.rs`）。
//! 这个文件验的是命令行这一层：档案切得动、切错了说得清、缺凭据不硬闯。

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
    let dir = temp_dir("scrape-profile");
    let workspace = temp_dir("scrape-profile-ws");
    写(&dir.path().join("FC/魂斗罗.zip"), &zip(2048));
    let out = Command::new(env!("CARGO_BIN_EXE_romcat"))
        .arg("scan")
        .arg(dir.path())
        .args(["--workspace"])
        .arg(workspace.path())
        .args(["--library", "测试库", "--no-checkpoint", "--quiet"])
        .output()
        .expect("能启动 romcat");
    assert!(out.status.success(), "扫描该成功");
    (dir, workspace)
}

fn 刮(workspace: &Path, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_romcat"))
        .arg("scrape")
        .args(["--library", "测试库", "--workspace"])
        .arg(workspace)
        .args(["--no-media"])
        .args(extra)
        // 就算测试机上碰巧配着凭据，这一趟也不许拿它去联网。
        .env_remove("SCREENSCRAPER_DEVID")
        .env_remove("SCREENSCRAPER_DEVPASSWORD")
        .env_remove("SCREENSCRAPER_SSID")
        .env_remove("SCREENSCRAPER_SSPASSWORD")
        .output()
        .expect("能启动 romcat")
}

#[test]
fn 不给档案就是离线档_一个网络请求都不发() {
    let (_dir, workspace) = 现场();
    let out = 刮(workspace.path(), &[]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("刮削：离线档"), "{text}");
    // 报告要说清**离线档补不上的是图**（票 06），而不是把空着的字段推给在线档；
    // 在线源也不许被报成不存在的源。
    assert!(text.contains("**离线档补不上的是图**"), "{text}");
    assert!(text.contains("这正是在线档存在的理由"), "{text}");
    assert!(text.contains("**网络请求数是 0**"), "{text}");
    assert!(text.contains("这一档没参加的源"), "{text}");
    assert!(text.contains("ScreenScraper"), "{text}");
}

#[test]
fn 档案是任务级别的开关_显式给离线也一样() {
    let (_dir, workspace) = 现场();
    let out = 刮(workspace.path(), &["--profile", "离线"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("刮削：离线档"));
    // 英文写法也认：这一档的名字进的是命令行，不是只进报告。
    let out = 刮(workspace.path(), &["--profile", "offline"]);
    assert!(out.status.success());
}

#[test]
fn 认不得的档案当场说清有哪两套() {
    let (_dir, workspace) = 现场();
    let out = 刮(workspace.path(), &["--profile", "胡说八道"]);
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("不认得策略档案"), "{text}");
    assert!(text.contains("离线") && text.contains("在线"), "{text}");
}

#[test]
fn 在线档缺凭据时不硬闯_也不悄悄退回离线() {
    let (_dir, workspace) = 现场();
    let out = 刮(workspace.path(), &["--profile", "在线"]);
    assert!(
        !out.status.success(),
        "起不来就该失败，而不是跑完一趟离线的"
    );
    let text = String::from_utf8_lossy(&out.stderr);
    // 说清凭据从哪儿来、为什么要人工申请、以及**不要拿别人的 devid 用**。
    assert!(text.contains("SCREENSCRAPER_DEVID"), "{text}");
    assert!(text.contains("论坛人工申请"), "{text}");
    assert!(text.contains("不要拿别人的 devid 用"), "{text}");
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains("刮削："),
        "既然起不来，就不该出一份看起来跑完了的报告"
    );
}
