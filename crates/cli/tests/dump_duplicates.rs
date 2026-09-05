//! 端到端验收命令行：真的跑一遍 `romcat` 这个二进制。
//!
//! 明细与中立库本身在核心库那边验过了。这里验的是只有命令行才有的接线：见到
//! `--dump-duplicates` 就把每组路径的上限放开、明细落到指定文件、**报告不跟着一起
//! 膨胀**，以及 `romcat report` 在主库不在位时照样出得来。这几段接线一旦回归，
//! 核心库的测试照样全绿。

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

/// 跑一次扫描。工作目录一定要显式指到临时目录：中立库会落在那里，绝不能让测试
/// 往开发者真实的 `~/.local/share/romcat` 里写东西。
fn 扫(workspace: &Path, args: &[&Path]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_romcat"))
        .arg("scan")
        .args(["--root-name", "库"])
        .args(args)
        .arg("--workspace")
        .arg(workspace)
        .args(["--no-checkpoint", "--quiet", "--samples-per-class", "0"])
        .output()
        .expect("能启动 romcat")
}

fn 出报告(workspace: &Path, args: &[&Path]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_romcat"))
        .arg("report")
        .args(args)
        .arg("--workspace")
        .arg(workspace)
        .output()
        .expect("能启动 romcat")
}

#[test]
fn 导出的明细记全每一份而报告不跟着膨胀() {
    let library = 建_主库();
    let out = temp_dir("cli-out");
    let dump = out.path().join("重复拷贝.txt");
    let json = out.path().join("体检.json");

    let workspace = temp_dir("cli-workspace");
    let output = 扫(
        workspace.path(),
        &[
            library.path(),
            Path::new("--dump-duplicates"),
            &dump,
            Path::new("--json"),
            &json,
        ],
    );
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

    let workspace = temp_dir("cli-workspace");
    assert!(扫(workspace.path(), &[library.path()]).status.success());
    assert!(!dump.exists());
}

/// 主库只读（ADR-0004）：明细落在主库里就该在开扫之前被拦下。
#[test]
fn 明细写不进主库且拦在开扫之前() {
    let library = 建_主库();
    let dump = library.path().join("重复拷贝.txt");

    let workspace = temp_dir("cli-workspace");
    let output = 扫(
        workspace.path(),
        &[library.path(), Path::new("--dump-duplicates"), &dump],
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("主库只读"),
        "要说清为什么被拒"
    );
    assert!(!dump.exists(), "被拒之后主库里不该多出任何东西");
}

/// 外置盘不常挂载，中立库存在本机（ADR-0009）：盘拔了也该看得到库里有什么。
#[test]
fn 主库不在位时报告照样出得来() {
    let library = 建_主库();
    let workspace = temp_dir("cli-workspace");
    let root = library.path().to_path_buf();

    let 扫的输出 = 扫(workspace.path(), &[&root]);
    assert!(
        扫的输出.status.success(),
        "扫描该成功：{}",
        String::from_utf8_lossy(&扫的输出.stderr)
    );

    // 把盘拔了
    fs::remove_dir_all(&root).expect("能删掉 fixture 主库");
    assert!(!root.exists());

    let out = temp_dir("cli-out");
    let dump = out.path().join("重复拷贝.txt");
    let output = 出报告(
        workspace.path(),
        &[&root, Path::new("--dump-duplicates"), &dump],
    );
    assert!(
        output.status.success(),
        "盘不在位也该出得来：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("库体检报告"));
    assert!(text.contains("25"), "25 个文件的结论还在中立库里");
    assert!(!text.contains("这次扫描的增量"), "没扫盘就没有增量可说");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("没有读过主库"),
        "要说清这份报告是从哪儿来的"
    );

    // 重复分组是从中立库现折出来的，不是扫描时攒下来的：完整明细因此不必重扫（挂账 D12）
    let 明细 = fs::read_to_string(&dump).expect("明细该写出来");
    assert_eq!(
        明细.matches("魂斗罗.zip").count(),
        25,
        "25 份的完整路径一条都不能少，而这一遍连盘都没有"
    );
}

/// 第二次扫描按 `(路径, 大小, 修改时间)` 跳过未变的文件。
#[test]
fn 第二次扫描跳过未变的文件() {
    let library = 建_主库();
    let workspace = temp_dir("cli-workspace");

    assert!(扫(workspace.path(), &[library.path()]).status.success());

    let output = Command::new(env!("CARGO_BIN_EXE_romcat"))
        .arg("scan")
        .args(["--root-name", "库"])
        .arg(library.path())
        .arg("--workspace")
        .arg(workspace.path())
        .args(["--no-checkpoint", "--samples-per-class", "0"])
        .output()
        .expect("能启动 romcat");
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("这次扫描的增量"), "第二次要报出增量");
    let 增量行 = text
        .lines()
        .skip_while(|line| !line.starts_with("未变（跳过）"))
        .nth(1)
        .expect("增量表下面有一行数字");
    assert!(
        增量行.starts_with("25"),
        "25 个文件全都未变，实际：{增量行}"
    );
}
