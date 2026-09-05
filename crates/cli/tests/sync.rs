//! 端到端验收 `romcat sublibrary plan`：**差量预览**在命令行上出得来，
//! 而且这条命令**一个文件都不写**。
//!
//! 计划器本身由 `romcat-core` 那一侧验（`crates/core/tests/sync.rs` 与 `sync` 的
//! 单元测试）。这个文件验的是命令行这一层：目标不在位时停得住、手动拷进去的东西
//! 一条都不出现在计划里、`--json` 出的**就是计划本身**。
//!
//! 工作目录一律显式指到临时目录：绝不能让测试往开发者真实的
//! `~/.local/share/romcat` 里写东西。

use std::fs;
use std::path::Path;
use std::process::Command;

use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

fn romcat(workspace: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_romcat"))
        .args(args)
        .args(["--workspace", &workspace.display().to_string()])
        .output()
        .expect("能启动 romcat")
}

fn 子库(workspace: &Path, args: &[&str]) -> std::process::Output {
    let mut all = vec!["sublibrary"];
    all.extend_from_slice(args);
    all.extend_from_slice(&["--library", "测试库"]);
    romcat(workspace, &all)
}

fn 出来的话(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// 数一数一棵目录树底下有几个文件。用来证明这条命令没往目标上写东西。
fn 文件数(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() { 文件数(&path) } else { 1 }
        })
        .sum()
}

/// 一份小 fixture 主库 + 一个配好规则的子库 + 一个当目标设备用的空目录。
fn 现场() -> (TempDir, TempDir, TempDir) {
    let library = temp_dir("sync-cli-lib");
    let workspace = temp_dir("sync-cli-ws");
    let target = temp_dir("sync-cli-card");
    写(&library.path().join("FC/魂斗罗.zip"), &zip(2048));
    写(&library.path().join("FC/超级玛丽.zip"), &zip(4096));
    写(&library.path().join("GB/口袋妖怪.zip"), &zip(8192));
    let out = romcat(
        workspace.path(),
        &[
            "scan",
            "--root-name",
            "库",
            &library.path().display().to_string(),
            "--library",
            "测试库",
            "--no-checkpoint",
            "--samples-per-class",
            "0",
            "--quiet",
        ],
    );
    assert!(out.status.success(), "{}", 出来的话(&out));
    let out = 子库(
        workspace.path(),
        &[
            "set",
            "掌机",
            "--target",
            &target.path().display().to_string(),
        ],
    );
    assert!(out.status.success(), "{}", 出来的话(&out));
    let out = 子库(workspace.path(), &["rule", "掌机", "--add", "平台=FC"]);
    assert!(out.status.success(), "{}", 出来的话(&out));
    (library, workspace, target)
}

#[test]
fn 差量预览出得来_而且一个文件都没写() {
    let (_library, workspace, target) = 现场();
    let out = 子库(workspace.path(), &["plan", "掌机"]);
    let text = 出来的话(&out);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("差量预览"), "{text}");
    assert!(text.contains("新增"), "{text}");
    assert!(text.contains("净变化"), "{text}");
    assert!(text.contains("一个文件都没写"), "{text}");
    // 头一次同步：清单是空的，删除项无从长出。
    assert!(text.contains("清单里 0 个文件"), "{text}");
    assert_eq!(文件数(target.path()), 0, "**排计划不搬任何文件**");
}

#[test]
fn 手动拷进目标的东西一条都不出现在计划里() {
    let (_library, workspace, target) = 现场();
    写(&target.path().join("saves/魂斗罗.sav"), &[9u8; 512]);
    写(&target.path().join("cheats/金手指.txt"), b"unlimited lives");
    写(&target.path().join("screenshots/一.png"), &[7u8; 64]);

    let json = workspace.path().join("计划.json");
    let out = 子库(
        workspace.path(),
        &["plan", "掌机", "--json", &json.display().to_string()],
    );
    let text = 出来的话(&out);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("清单之外"), "{text}");
    assert!(text.contains("3 个文件"), "{text}");

    // `--json` 出的**就是计划本身**，不是另算的一份。
    let plan: serde_json::Value =
        serde_json::from_slice(&fs::read(&json).expect("读得出")).expect("是 JSON");
    let steps = plan["steps"].as_array().expect("有步骤");
    assert!(!steps.is_empty(), "{plan}");
    for step in steps {
        let path = step["path"].as_str().expect("有路径");
        assert!(
            !path.starts_with("saves/")
                && !path.starts_with("cheats/")
                && !path.starts_with("screenshots/"),
            "{path} 混进了计划"
        );
        assert_eq!(step["act"], "Add", "清单是空的，只可能有新增");
    }
    assert_eq!(plan["deletes"]["files"], 0);
    assert_eq!(plan["strangers"], 3);
    assert_eq!(
        plan["net_bytes"].as_i64().expect("是数"),
        plan["adds"]["bytes"].as_i64().expect("是数"),
        "净变化就是新增那些",
    );
    assert_eq!(
        文件数(target.path()),
        3,
        "目标上还是那三个，一个没多一个没少"
    );
}

#[test]
fn 目标不在位时停住并说清怎么办() {
    let (_library, workspace, target) = 现场();
    let 不在了 = target.path().join("没插上");
    let out = 子库(
        workspace.path(),
        &["plan", "掌机", "--target", &不在了.display().to_string()],
    );
    let text = 出来的话(&out);
    assert!(!out.status.success(), "{text}");
    assert!(text.contains("目标不在位"), "{text}");
    assert!(text.contains("插上读卡器"), "{text}");
}

#[test]
fn 子库不在时说得清怎么建() {
    let (_library, workspace, _target) = 现场();
    let out = 子库(workspace.path(), &["plan", "备用卡"]);
    let text = 出来的话(&out);
    assert!(!out.status.success(), "{text}");
    assert!(text.contains("没有叫「备用卡」的子库"), "{text}");
}

// ───────────────────────── `romcat sublibrary sync`：真的往目标上写

/// 卡上文件的 `(相对路径, 字节数)`。
fn 卡上有什么(dir: &Path) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        let Ok(entries) = fs::read_dir(&at) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            out.push((
                path.strip_prefix(dir)
                    .expect("在树里")
                    .display()
                    .to_string(),
                entry.metadata().expect("读得到").len(),
            ));
        }
    }
    out.sort();
    out
}

#[test]
fn 同步之前一定先印一遍差量预览() {
    // ADR-0016：「永远不能点了同步就开始传」。预览与计划是同一个值，于是这一句
    // 印的就是等下真要做的事。
    let (_library, workspace, target) = 现场();
    let out = 子库(workspace.path(), &["sync", "掌机"]);
    let text = 出来的话(&out);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("差量预览"), "{text}");
    assert!(text.contains("同步结果"), "{text}");
    // 预览排在结果前面——反过来就不叫「先呈现」了。
    assert!(
        text.find("差量预览") < text.find("同步结果"),
        "预览必须印在动手之前：{text}"
    );
    assert_eq!(卡上有什么(target.path()).len(), 3, "两个 ROM 加一份元数据");
}

#[test]
fn 干跑一个字节都不写() {
    let (_library, workspace, target) = 现场();
    let out = 子库(workspace.path(), &["sync", "掌机", "--dry-run"]);
    let text = 出来的话(&out);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("差量预览"), "{text}");
    assert!(text.contains("一个字节都没写"), "{text}");
    assert_eq!(文件数(target.path()), 0, "**干跑不搬任何文件**");
}

#[test]
fn 有删除时不点头就一个字节都不动() {
    let (_library, workspace, target) = 现场();
    assert!(子库(workspace.path(), &["sync", "掌机"]).status.success());
    let 放好了 = 卡上有什么(target.path());
    assert!(!放好了.is_empty());

    // 规则改成只要 GB：FC 那两个加那份元数据都该被删——但**删之前要点头**。
    assert!(
        子库(workspace.path(), &["rule", "掌机", "--remove", "1"])
            .status
            .success()
    );
    assert!(
        子库(workspace.path(), &["rule", "掌机", "--add", "平台=GB"])
            .status
            .success()
    );
    let out = 子库(workspace.path(), &["sync", "掌机"]);
    let text = 出来的话(&out);
    assert!(!out.status.success(), "没点头不该算成功：{text}");
    assert!(text.contains("加 `--yes` 再跑一次"), "{text}");
    assert_eq!(卡上有什么(target.path()), 放好了, "卡上一个字节都没变");

    // 点头之后才真的删。
    let out = 子库(workspace.path(), &["sync", "掌机", "--yes"]);
    let text = 出来的话(&out);
    assert!(out.status.success(), "{text}");
    let 现在 = 卡上有什么(target.path());
    assert!(
        现在.iter().all(|(path, _)| !path.starts_with("FC")),
        "{现在:?}"
    );
    assert!(
        现在.iter().any(|(path, _)| path.starts_with("GB")),
        "{现在:?}"
    );
}

#[test]
fn 同步完清单记的是目标的真实状态_再跑一趟什么都不用动() {
    let (_library, workspace, target) = 现场();
    assert!(子库(workspace.path(), &["sync", "掌机"]).status.success());
    let out = 子库(workspace.path(), &["sync", "掌机"]);
    let text = 出来的话(&out);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("一个文件都不用动"), "{text}");
    assert!(text.contains("一个字节都没写"), "{text}");
    assert_eq!(卡上有什么(target.path()).len(), 3);
}

#[test]
fn 在掌机上删掉的东西不会自己长回来() {
    // 用户故事 65。清单更新成目标的真实状态之后，那一格记着「你删过、我不补」。
    let (_library, workspace, target) = 现场();
    assert!(子库(workspace.path(), &["sync", "掌机"]).status.success());
    fs::remove_file(target.path().join("FC/魂斗罗.zip")).expect("删得掉");

    let text = 出来的话(&子库(workspace.path(), &["sync", "掌机"]));
    assert!(text.contains("没了"), "第一趟要如实报一次：{text}");
    assert!(!target.path().join("FC/魂斗罗.zip").exists(), "不静默补回");

    let text = 出来的话(&子库(workspace.path(), &["sync", "掌机"]));
    assert!(text.contains("你删过"), "{text}");
    assert!(!target.path().join("FC/魂斗罗.zip").exists(), "还是不补");

    // 明说要补才补——**明知故犯不是静默**。
    let text = 出来的话(&子库(workspace.path(), &["sync", "掌机", "--restore"]));
    assert!(text.contains("补回"), "{text}");
    assert!(target.path().join("FC/魂斗罗.zip").exists(), "这次补回来了");
}

#[test]
fn 手动拷进目标的东西同步之后一个字节都没变() {
    let (_library, workspace, target) = 现场();
    写(&target.path().join("saves/魂斗罗.sav"), &[9u8; 512]);
    写(&target.path().join("cheats/金手指.txt"), b"unlimited lives");
    assert!(子库(workspace.path(), &["sync", "掌机"]).status.success());
    assert_eq!(
        fs::read(target.path().join("saves/魂斗罗.sav")).expect("还在"),
        vec![9u8; 512],
    );
    assert_eq!(
        fs::read(target.path().join("cheats/金手指.txt")).expect("还在"),
        b"unlimited lives",
    );
}

#[test]
fn 子库里的元数据路径全部相对子库根() {
    let (library, workspace, target) = 现场();
    assert!(子库(workspace.path(), &["sync", "掌机"]).status.success());
    let text = fs::read_to_string(target.path().join("FC.metadata.pegasus.txt")).expect("落到位了");
    assert!(
        !text.contains(&library.path().display().to_string()),
        "绝不写主库的绝对路径：\n{text}"
    );
    assert!(text.contains("files: FC/魂斗罗.zip"), "{text}");
}
