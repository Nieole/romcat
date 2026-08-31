//! 端到端验收**成型**那几个子命令：`romcat shape` 与 `romcat platforms`。
//!
//! 要证的是两件在核心库里证不了的事：
//!
//! 1. **改一条成型规则不必重扫主库。** `shape` 只碰中立库——测试把 fixture 主库整个删掉
//!    再跑一次，照样出得来。
//! 2. **加一个平台只要给一份清单。** 换一份 `--manifest`，原本没映射的目录就成了平台，
//!    还按新规则成了型，一行 Rust 都没改。
//!
//! 工作目录一律显式指到临时目录：中立库会落在那里，绝不能让测试往开发者真实的
//! `~/.local/share/romcat` 里写东西。

use std::fs;
use std::path::Path;
use std::process::Command;

use romcat_core::testing::sample::{chd, zip};
use romcat_core::testing::{TempDir, temp_dir};

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

/// 一个小 fixture 主库：PSV 一份目录树转储、PS1 一对 cue+bin、FC 两个散归档。
fn 建库() -> TempDir {
    let dir = temp_dir("cli-shape");
    let root = dir.path();
    let psv = root.join("PSV/某游戏[PCSG00042]");
    写(&psv.join("app/PCSG00042/eboot.bin"), &[1u8; 32]);
    写(&psv.join("app/PCSG00042/sce_sys/param.sfo"), &[2u8; 16]);
    for i in 0..12 {
        写(&psv.join(format!("app/PCSG00042/bgm/{i}.at9")), &[3u8; 8]);
    }
    写(&root.join("ps/生化危机/生化危机.cue"), b"FILE \"x.bin\"");
    写(&root.join("ps/生化危机/生化危机.bin"), &[4u8; 256]);
    写(&root.join("ps/别的/别的.chd"), &chd());
    写(&root.join("FC/甲.zip"), &zip(64));
    写(&root.join("FC/乙.zip"), &zip(128));
    dir
}

fn romcat(workspace: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_romcat"))
        .args(args)
        .args(["--workspace", &workspace.display().to_string()])
        .output()
        .expect("能启动 romcat")
}

fn 扫(workspace: &Path, root: &Path, name: &str) -> std::process::Output {
    romcat(
        workspace,
        &[
            "scan",
            &root.display().to_string(),
            "--library",
            name,
            "--no-checkpoint",
            "--samples-per-class",
            "0",
            "--quiet",
        ],
    )
}

/// 从一份报告 JSON 里取成型那一节。
fn 成型(workspace: &Path, file: &str) -> serde_json::Value {
    let text = fs::read_to_string(workspace.join(file)).expect("报告写出来了");
    let value: serde_json::Value = serde_json::from_str(&text).expect("是 JSON");
    value["shaping"].clone()
}

#[test]
fn 扫完就成型且报告说得出变体数() {
    let workspace = temp_dir("cli-shape-ws");
    let library = 建库();
    let out = 扫(workspace.path(), library.path(), "主库");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("成型完毕"), "{stderr}");

    let json = workspace.path().join("体检.json");
    let out = romcat(
        workspace.path(),
        &[
            "report",
            "--library",
            "主库",
            "--json",
            &json.display().to_string(),
            "--quiet",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let shaping = 成型(workspace.path(), "体检.json");
    assert_eq!(shaping["shaped"], true);
    assert_eq!(shaping["stale"], false);
    // PSV 一个、生化危机一个、别的.chd 一个、FC 两个
    assert_eq!(shaping["variants"], 5);
    assert!(
        shaping["files_per_variant"].as_f64().expect("是个数") > 3.0,
        "PSV 那 14 个文件必须收敛掉：{shaping}"
    );
}

#[test]
fn 改一条成型规则不必重扫主库() {
    let workspace = temp_dir("cli-reshape-ws");
    let library = 建库();
    assert!(
        扫(workspace.path(), library.path(), "主库")
            .status
            .success()
    );

    // 把主库整个删掉——`shape` 只碰中立库，一个字节都不该读主库。
    let root = library.path().to_path_buf();
    fs::remove_dir_all(&root).expect("删得掉");
    assert!(!root.exists());

    let json = workspace.path().join("重新成型.json");
    let out = romcat(
        workspace.path(),
        &[
            "shape",
            "--library",
            "主库",
            "--json",
            &json.display().to_string(),
            "--quiet",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("没有读过主库"), "{stderr}");
    assert_eq!(成型(workspace.path(), "重新成型.json")["variants"], 5);
}

#[test]
fn 人工纠正把两个条目并成一个变体且撤得掉() {
    let workspace = temp_dir("cli-merge-ws");
    let library = 建库();
    assert!(
        扫(workspace.path(), library.path(), "主库")
            .status
            .success()
    );

    let json = workspace.path().join("并过.json");
    let out = romcat(
        workspace.path(),
        &[
            "shape",
            "--library",
            "主库",
            "--merge",
            "FC/甲.zip",
            "--merge",
            "FC/乙.zip",
            "--json",
            &json.display().to_string(),
            "--quiet",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let shaping = 成型(workspace.path(), "并过.json");
    assert_eq!(shaping["variants"], 4, "两个 zip 并成了一个：{shaping}");
    assert_eq!(shaping["manual"], 1);

    // 撤掉之后回到 5 个
    let json = workspace.path().join("撤过.json");
    let out = romcat(
        workspace.path(),
        &[
            "shape",
            "--library",
            "主库",
            "--forget-merge",
            "FC/乙.zip",
            "--forget-merge",
            "FC/甲.zip",
            "--json",
            &json.display().to_string(),
            "--quiet",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(成型(workspace.path(), "撤过.json")["variants"], 5);
}

#[test]
fn 键打错了当场报错而不是记一条永远不生效的纠正() {
    let workspace = temp_dir("cli-merge-typo-ws");
    let library = 建库();
    assert!(
        扫(workspace.path(), library.path(), "主库")
            .status
            .success()
    );
    let out = romcat(
        workspace.path(),
        &[
            "shape",
            "--library",
            "主库",
            "--merge",
            "FC/甲.zip",
            "--merge",
            "FC/打错了.zip",
            "--quiet",
        ],
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("没有 FC/打错了.zip"), "{stderr}");
}

#[test]
fn 只给一个键的合并被拦下来() {
    let workspace = temp_dir("cli-merge-one-ws");
    let library = 建库();
    assert!(
        扫(workspace.path(), library.path(), "主库")
            .status
            .success()
    );
    let out = romcat(
        workspace.path(),
        &[
            "shape",
            "--library",
            "主库",
            "--merge",
            "FC/甲.zip",
            "--quiet",
        ],
    );
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("至少要给两个键"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn 加一个平台只要给一份清单不必改代码() {
    let workspace = temp_dir("cli-manifest-ws");
    let library = temp_dir("cli-manifest-lib");
    写(
        &library.path().join("假想机/某游戏/data/main.fic"),
        &[1u8; 32],
    );
    写(&library.path().join("假想机/某游戏/CART.ID"), &[2u8; 8]);

    // 先用内置清单扫一遍：`假想机` 不认得，一个变体都不成。
    assert!(
        扫(workspace.path(), library.path(), "主库")
            .status
            .success()
    );
    let json = workspace.path().join("内置.json");
    assert!(
        romcat(
            workspace.path(),
            &[
                "report",
                "--library",
                "主库",
                "--json",
                &json.display().to_string(),
                "--quiet"
            ]
        )
        .status
        .success()
    );
    assert_eq!(成型(workspace.path(), "内置.json")["variants"], 0);

    // 写一份自己的清单，只加平台与规则，不动一行 Rust。
    let manifest = workspace.path().join("我的平台.toml");
    fs::write(
        &manifest,
        r#"
"版本" = 1
[["成型规则"]]
"名" = "假想机目录树"
"方式" = "目录树"
"锚文件" = ["CART.ID"]
"变体根" = "锚目录"
[["平台"]]
"名" = "假想机"
"目录" = ["假想机"]
"成型" = ["假想机目录树"]
"#,
    )
    .expect("写得下");

    let json = workspace.path().join("自定义.json");
    let out = romcat(
        workspace.path(),
        &[
            "shape",
            "--library",
            "主库",
            "--manifest",
            &manifest.display().to_string(),
            "--json",
            &json.display().to_string(),
            "--quiet",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let shaping = 成型(workspace.path(), "自定义.json");
    assert_eq!(shaping["variants"], 1, "整棵树成了一个变体：{shaping}");
    assert_eq!(shaping["files"], 2);
}

#[test]
fn 工作目录里的清单不给参数也生效() {
    let workspace = temp_dir("cli-ws-manifest");
    let library = temp_dir("cli-ws-manifest-lib");
    写(&library.path().join("假想机/某游戏/CART.ID"), &[2u8; 8]);
    写(&library.path().join("假想机/某游戏/data.fic"), &[1u8; 16]);
    fs::create_dir_all(workspace.path()).expect("能建目录");
    fs::write(
        workspace.path().join("platforms.toml"),
        r#"
"版本" = 1
[["成型规则"]]
"名" = "假想机目录树"
"方式" = "目录树"
"锚文件" = ["CART.ID"]
"变体根" = "锚目录"
[["平台"]]
"名" = "假想机"
"目录" = ["假想机"]
"成型" = ["假想机目录树"]
"#,
    )
    .expect("写得下");

    assert!(
        扫(workspace.path(), library.path(), "主库")
            .status
            .success()
    );
    let json = workspace.path().join("体检.json");
    assert!(
        romcat(
            workspace.path(),
            &[
                "report",
                "--library",
                "主库",
                "--json",
                &json.display().to_string(),
                "--quiet"
            ]
        )
        .status
        .success()
    );
    assert_eq!(成型(workspace.path(), "体检.json")["variants"], 1);
}

#[test]
fn 清单写坏了当场报错而不是悄悄退回内置的() {
    let workspace = temp_dir("cli-bad-manifest");
    let manifest = workspace.path().join("坏的.toml");
    fs::create_dir_all(workspace.path()).expect("能建目录");
    fs::write(&manifest, "\"版本\" = 1\n[[\"平台\"]]\n\"名\" = \"甲\"\n\"目录\" = [\"a\"]\n\"成型\" = [\"没这条\"]\n")
        .expect("写得下");
    let out = romcat(
        workspace.path(),
        &["platforms", "--manifest", &manifest.display().to_string()],
    );
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("不存在"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn 平台清单看得见也导得出() {
    let workspace = temp_dir("cli-platforms");
    let out = romcat(workspace.path(), &["platforms"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("PSV"), "{stdout}");
    assert!(stdout.contains("PSV 目录树"), "{stdout}");
    assert!(stdout.contains("明确排除的目录"), "{stdout}");

    let 底稿 = workspace.path().join("底稿.toml");
    let out = romcat(
        workspace.path(),
        &["platforms", "--dump-builtin", &底稿.display().to_string()],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = fs::read_to_string(&底稿).expect("导出来了");
    assert!(text.contains("[[\"平台\"]]"), "导出来的得是能照着改的 TOML");
}
