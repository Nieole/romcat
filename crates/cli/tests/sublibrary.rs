//! 端到端验收 `romcat sublibrary` 这一组：**子库是持久实体**，配一次反复使用；
//! 规则可重放；例外优先于规则；一个主库上多个子库互不干扰；选中多少条、多少容量看得见。
//!
//! 求值本身由 `romcat-core` 那一侧验（`crates/core/tests/sublibrary.rs` 与
//! `sublibrary` 的单元测试）。这个文件验命令行这一层：写错的规则当场被拦下、
//! 子库不在时说得清怎么建、**目标设备与主库都不在位照样干得了**（ADR-0009）。
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

/// 一个小 fixture 主库 + 一份扫过（也成过型）的中立库。
fn 现场() -> (TempDir, TempDir) {
    let dir = temp_dir("sublib-cli");
    let workspace = temp_dir("sublib-cli-ws");
    写(&dir.path().join("FC/魂斗罗.zip"), &zip(2048));
    写(&dir.path().join("FC/超级玛丽.zip"), &zip(4096));
    写(&dir.path().join("GB/口袋妖怪.zip"), &zip(8192));
    let out = romcat(
        workspace.path(),
        &[
            "scan",
            "--root-name",
            "库",
            &dir.path().display().to_string(),
            "--library",
            "测试库",
            "--no-checkpoint",
            "--samples-per-class",
            "0",
            "--quiet",
        ],
    );
    assert!(out.status.success(), "{}", 出来的话(&out));
    (dir, workspace)
}

/// 建一个子库，顺手把常用参数配齐。
fn 建(workspace: &Path, name: &str, extra: &[&str]) -> std::process::Output {
    let target = workspace.join(format!("卡-{name}"));
    let mut args = vec!["set", name, "--target", ""];
    let target = target.display().to_string();
    args[3] = &target;
    args.extend_from_slice(extra);
    子库(workspace, &args)
}

#[test]
fn 子库配一次反复使用_列得出来也删得掉() {
    let (_library, workspace) = 现场();
    let ws = workspace.path();

    // 一个都没有时说得清怎么建。
    let out = 子库(ws, &["list"]);
    assert!(out.status.success(), "{}", 出来的话(&out));
    assert!(出来的话(&out).contains("还没有子库"), "{}", 出来的话(&out));

    let out = 建(ws, "掌机", &["--capacity", "512GB"]);
    assert!(out.status.success(), "{}", 出来的话(&out));
    assert!(出来的话(&out).contains("已建子库"), "{}", 出来的话(&out));

    // **配一次反复使用**：另起一次进程照样读得回来。
    let out = 子库(ws, &["list"]);
    let text = 出来的话(&out);
    assert!(text.contains("掌机"), "{text}");
    assert!(
        text.contains("476.84 GiB"),
        "512GB 折成二进制印出来：{text}"
    );

    // 改一个字段不动别的。
    let out = 子库(ws, &["set", "掌机", "--no-capacity"]);
    assert!(out.status.success(), "{}", 出来的话(&out));
    assert!(出来的话(&out).contains("已改子库"), "{}", 出来的话(&out));
    let text = 出来的话(&子库(ws, &["list"]));
    assert!(text.contains("卡-掌机"), "目标路径没被改掉：{text}");

    let out = 子库(ws, &["remove", "掌机"]);
    assert!(out.status.success(), "{}", 出来的话(&out));
    assert!(
        !子库(ws, &["show", "掌机"]).status.success(),
        "删掉之后就该找不到了"
    );
}

#[test]
fn 写错的规则当场被拦下_不写进库() {
    let (_library, workspace) = 现场();
    let ws = workspace.path();
    assert!(建(ws, "掌机", &[]).status.success());

    let out = 子库(ws, &["rule", "掌机", "--add", "标签=汉化"]);
    assert!(!out.status.success(), "{}", 出来的话(&out));
    let text = 出来的话(&out);
    assert!(text.contains("这条规则读不懂"), "{text}");
    assert!(text.contains("认不出维度"), "{text}");

    // 没写进去：列出来还是空的。
    let text = 出来的话(&子库(ws, &["rule", "掌机"]));
    assert!(text.contains("一条都没有"), "{text}");
    // 帮助里列得出能筛的维度——用户不必翻文档才知道能写什么。
    for 维度 in ["平台", "语言", "中文", "年份", "体积", "合集"] {
        assert!(text.contains(维度), "{维度} 该列在帮助里：{text}");
    }
}

#[test]
fn 规则选得出东西_报告说得出多少条多少容量() {
    let (_library, workspace) = 现场();
    let ws = workspace.path();
    assert!(建(ws, "掌机", &[]).status.success());
    let out = 子库(ws, &["rule", "掌机", "--add", "平台=FC"]);
    assert!(out.status.success(), "{}", 出来的话(&out));
    assert!(
        出来的话(&out).contains("规则 1 已加进子库"),
        "{}",
        出来的话(&out)
    );

    let out = 子库(ws, &["show", "掌机"]);
    assert!(out.status.success(), "{}", 出来的话(&out));
    let text = 出来的话(&out);
    assert!(text.contains("选中 2 个变体"), "{text}");
    assert!(text.contains("变体            2 / 3"), "{text}");
    assert!(text.contains("平台=FC"), "报告原样印用户写的那句话：{text}");
}

#[test]
fn 例外优先于规则_并记得住为什么() {
    let (_library, workspace) = 现场();
    let ws = workspace.path();
    assert!(建(ws, "掌机", &[]).status.success());
    assert!(
        子库(ws, &["rule", "掌机", "--add", "平台=FC"])
            .status
            .success()
    );

    let out = 子库(
        ws,
        &[
            "except",
            "掌机",
            "--exclude",
            "库/FC/魂斗罗.zip",
            "--note",
            "这个我通关过了",
            "--include",
            "库/GB/口袋妖怪.zip",
        ],
    );
    assert!(out.status.success(), "{}", 出来的话(&out));
    let text = 出来的话(&out);
    assert!(text.contains("这个我通关过了"), "半年后要看得懂：{text}");

    let text = 出来的话(&子库(ws, &["show", "掌机"]));
    assert!(text.contains("选中 2 个变体"), "{text}");
    assert!(text.contains("**这几条真的在起作用**"), "{text}");

    // 改规则不动例外。
    assert!(
        子库(ws, &["rule", "掌机", "--remove", "1"])
            .status
            .success()
    );
    assert!(
        子库(ws, &["rule", "掌机", "--add", "平台=FC,GB"])
            .status
            .success()
    );
    let text = 出来的话(&子库(ws, &["except", "掌机"]));
    assert!(text.contains("这个我通关过了"), "例外是沉淀：{text}");
    let text = 出来的话(&子库(ws, &["show", "掌机"]));
    assert!(text.contains("选中 2 个变体"), "魂斗罗照旧被排除：{text}");
}

#[test]
fn 同一个变体不许在一条命令里领两个决定() {
    // 挨个执行的话后一个会静默盖掉前一个，而两行「已记下」都打了出来——
    // 对着「例外优先于规则、永久记住」这条纪律，那是最坏的一种错。
    let (_library, workspace) = 现场();
    let ws = workspace.path();
    assert!(建(ws, "掌机", &[]).status.success());
    let out = 子库(
        ws,
        &[
            "except",
            "掌机",
            "--include",
            "库/FC/魂斗罗.zip",
            "--exclude",
            "库/FC/魂斗罗.zip",
        ],
    );
    assert!(!out.status.success(), "{}", 出来的话(&out));
    assert!(
        出来的话(&out).contains("不止一个决定"),
        "{}",
        出来的话(&out)
    );
    // 一条都没写进去。
    let text = 出来的话(&子库(ws, &["except", "掌机"]));
    assert!(text.contains("一条都没有"), "{text}");
}

#[test]
fn 库里没有的变体照样记得下例外() {
    let (_library, workspace) = 现场();
    let ws = workspace.path();
    assert!(建(ws, "掌机", &[]).status.success());
    let out = 子库(
        ws,
        &["except", "掌机", "--include", "库/SFC/盘没插时看不到的.zip"],
    );
    assert!(out.status.success(), "{}", 出来的话(&out));
    let text = 出来的话(&out);
    assert!(text.contains("库里眼下没有这个变体"), "{text}");
    let text = 出来的话(&子库(ws, &["show", "掌机"]));
    assert!(text.contains("照旧记着"), "{text}");
}

#[test]
fn 多个子库互不干扰() {
    let (_library, workspace) = 现场();
    let ws = workspace.path();
    assert!(建(ws, "掌机", &[]).status.success());
    assert!(建(ws, "备用卡", &[]).status.success());
    assert!(
        子库(ws, &["rule", "掌机", "--add", "平台=FC"])
            .status
            .success()
    );
    assert!(
        子库(ws, &["rule", "备用卡", "--add", "平台=GB"])
            .status
            .success()
    );
    assert!(
        子库(ws, &["except", "掌机", "--include", "库/GB/口袋妖怪.zip"])
            .status
            .success()
    );

    assert!(出来的话(&子库(ws, &["show", "掌机"])).contains("选中 3 个变体"));
    assert!(出来的话(&子库(ws, &["show", "备用卡"])).contains("选中 1 个变体"));
    let text = 出来的话(&子库(ws, &["except", "备用卡"]));
    assert!(text.contains("一条都没有"), "掌机的例外没串过来：{text}");
}

#[test]
fn 主库不在位照样看得了选择集() {
    // 子库是持久实体，不是「插上卡才存在的东西」；选择集从中立库折出来（ADR-0009）。
    let (library, workspace) = 现场();
    let ws = workspace.path();
    assert!(建(ws, "掌机", &["--capacity", "8KiB"]).status.success());
    assert!(
        子库(ws, &["rule", "掌机", "--add", "平台=FC,GB"])
            .status
            .success()
    );
    drop(library);

    let out = 子库(ws, &["show", "掌机"]);
    assert!(out.status.success(), "{}", 出来的话(&out));
    let text = 出来的话(&out);
    assert!(text.contains("选中 3 个变体"), "{text}");
    // 8 KiB 的卡装不下这三个归档：报出超出量与裁剪建议，**不自动截断**（ADR-0016）。
    assert!(text.contains("装不下"), "{text}");
    assert!(text.contains("不会自动截断"), "{text}");
}

#[test]
fn 子库不在时说得清怎么建() {
    let (_library, workspace) = 现场();
    let out = 子库(workspace.path(), &["show", "还没建的"]);
    assert!(!out.status.success());
    let text = 出来的话(&out);
    assert!(text.contains("没有叫「还没建的」的子库"), "{text}");
    assert!(text.contains("romcat sublibrary set"), "{text}");
}

#[test]
fn 新建子库不给目标路径会被拦下() {
    let (_library, workspace) = 现场();
    let out = 子库(workspace.path(), &["set", "掌机"]);
    assert!(!out.status.success());
    assert!(出来的话(&out).contains("--target"), "{}", 出来的话(&out));
}
