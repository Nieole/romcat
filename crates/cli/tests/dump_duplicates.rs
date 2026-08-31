//! 端到端验收 `--dump-duplicates`：真的跑一遍 `romcat` 这个二进制。
//!
//! 明细本身在核心库那边验过了。这里验的是只有命令行才有的三件事：见到开关就把每组
//! 路径的上限放开、明细落到指定文件、以及**报告不跟着一起膨胀**。这段接线一旦回归，
//! 导出的明细会静默地只剩每组 10 条路径，而核心库的测试照样全绿。

use std::fs;
use std::path::Path;
use std::process::Command;

use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};

/// 一个主库：25 份同名同大小的**透明容器**，正好是一组重复拷贝。
fn 建_主库() -> TempDir {
    let dir = temp_dir("cli-duplicates");
    for copy in 0..25 {
        let path = dir.path().join(format!("FC/备份{copy}/魂斗罗.zip"));
        fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
        fs::write(&path, zip(4096)).expect("能写文件");
    }
    dir
}

fn 跑(args: &[&Path]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_romcat"))
        .arg("scan")
        .args(args)
        .args(["--no-checkpoint", "--quiet", "--samples-per-class", "0"])
        .output()
        .expect("能启动 romcat")
}

#[test]
fn 导出的明细记全每一份而报告不跟着膨胀() {
    let library = 建_主库();
    let out = temp_dir("cli-out");
    let dump = out.path().join("重复拷贝.txt");
    let json = out.path().join("体检.json");

    let output = 跑(&[
        library.path(),
        Path::new("--dump-duplicates"),
        &dump,
        Path::new("--json"),
        &json,
    ]);
    assert!(
        output.status.success(),
        "扫描该成功：{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let text = fs::read_to_string(&dump).expect("明细该写出来");
    assert_eq!(
        text.matches("魂斗罗.zip").count(),
        25,
        "25 份的完整路径一条都不能少"
    );
    assert!(text.contains("共 25 份"));
    assert!(!text.contains("另有"), "没有任何一份缺路径");
    assert!(text.contains("绝不自动删除"));

    // 完整明细导出去了，主 JSON 却不许被撑大——这正是不把它塞进报告的理由。
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&json).expect("报告该写出来")).expect("是 JSON");
    let 首组 = &report["suspects"]["top_duplicates"][0];
    assert_eq!(首组["count"], 25);
    assert_eq!(
        首组["paths"].as_array().expect("有路径数组").len(),
        10,
        "报告里每组仍然只留 10 条"
    );
}

#[test]
fn 不加开关就不写明细() {
    let library = 建_主库();
    let out = temp_dir("cli-out");
    let dump = out.path().join("重复拷贝.txt");

    assert!(跑(&[library.path()]).status.success());
    assert!(!dump.exists());
}

/// 主库只读（ADR-0004）：明细落在主库里就该在开扫之前被拦下。
#[test]
fn 明细写不进主库且拦在开扫之前() {
    let library = 建_主库();
    let dump = library.path().join("重复拷贝.txt");

    let output = 跑(&[library.path(), Path::new("--dump-duplicates"), &dump]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("主库只读"),
        "要说清为什么被拒"
    );
    assert!(!dump.exists(), "被拒之后主库里不该多出任何东西");
}
