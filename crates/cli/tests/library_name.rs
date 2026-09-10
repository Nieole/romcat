//! 端到端验收 `--library`：给主库起个名字，中立库跟名字走而不跟绝对路径走。
//!
//! 要治的病是这个：中立库的文件名按主库**绝对路径**的哈希取，可 macOS 重挂一次盘就可能
//! 从 `/Volumes/新加卷` 变成 `/Volumes/新加卷 1`，Windows 上盘符也会变——换了挂载点就
//! 找不到原来那份中立库，全库白扫一遍。而 ADR-0018 定的工作方式正是盘在两台机器之间
//! 来回接，所以这事会反复发生（挂账 D16）。
//!
//! **验证一律用本地 fixture 做**：把 fixture 主库扫一遍，整个目录挪到另一个路径，
//! 用同一个名字再扫。用户那块 8.6 TiB 的外置盘一个字节都不碰，更不会为了测试去
//! 卸载重挂它。

use std::fs;
use std::path::Path;
use std::process::Command;

use romcat_core::testing::sample::{gba, zip};
use romcat_core::testing::{TempDir, temp_dir};

/// 一个小 fixture 主库：按平台分目录，够顶层条目比对认得出它。
fn 建_主库(dir: &Path) {
    写(&dir.join("FC/超级马里奥.zip"), &zip(4096));
    写(&dir.join("FC/魂斗罗.zip"), &zip(2048));
    写(&dir.join("GBA/黄金太阳.gba"), &gba(b"AGSJ"));
    写(
        &dir.join("PS1/最终幻想7/说明.txt"),
        "随便写点什么".as_bytes(),
    );
    写(&dir.join("散落的游戏.gba"), &gba(b"AGBJ"));
}

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

/// 跑一次扫描。工作目录一定要显式指到临时目录：中立库会落在那里，绝不能让测试
/// 往开发者真实的 `~/.local/share/romcat` 里写东西。
fn 扫(workspace: &Path, root: &Path, library: Option<&str>) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_romcat"));
    command
        .arg("scan")
        .args(["--root-name", "库"])
        .arg(root)
        .arg("--workspace")
        .arg(workspace)
        .args(["--no-checkpoint", "--samples-per-class", "0", "--json"]);
    let json = workspace.join("体检.json");
    command.arg(&json);
    if let Some(name) = library {
        command.args(["--library", name]);
    }
    command.args(["--quiet"]).output().expect("能启动 romcat")
}

/// 从刚写出来的报告 JSON 里取这次扫描的增量三个数。
fn 增量(workspace: &Path) -> (u64, u64, u64) {
    let text = fs::read_to_string(workspace.join("体检.json")).expect("报告写出来了");
    let value: serde_json::Value = serde_json::from_str(&text).expect("是 JSON");
    let delta = &value["delta"];
    (
        delta["added"].as_u64().expect("有新增"),
        delta["unchanged"].as_u64().expect("有未变"),
        delta["removed"].as_u64().expect("有已删"),
    )
}

fn 挪走(from: &TempDir, tag: &str) -> TempDir {
    let to = temp_dir(tag);
    let target = to.path().join("Game");
    fs::rename(from.path(), &target).expect("能改名");
    fs::create_dir_all(from.path()).expect("原地重建一个空的，好让 TempDir 收得掉");
    to
}

#[test]
fn 起了名字之后换个路径判为全部未变而不是全部新增() {
    let workspace = temp_dir("library-name-workspace");
    let 原地 = temp_dir("library-name-a");
    建_主库(原地.path());

    let 第一次 = 扫(workspace.path(), 原地.path(), Some("主库"));
    assert!(第一次.status.success(), "{第一次:?}");
    let (新增, _, _) = 增量(workspace.path());
    assert!(新增 > 0, "第一次扫当然全是新增");

    // 盘换了挂载点：同一份内容，换了个绝对路径。
    let 挪到 = 挪走(&原地, "library-name-b");
    let 第二次 = 扫(workspace.path(), &挪到.path().join("Game"), Some("主库"));
    assert!(第二次.status.success(), "{第二次:?}");
    let (新增2, 未变2, 已删2) = 增量(workspace.path());
    assert_eq!(新增2, 0, "换个路径不该把整个库判成新增");
    assert_eq!(已删2, 0, "更不该把旧的那份判成已删");
    assert_eq!(未变2, 新增, "每一条都该判成未变");
}

#[test]
fn 不给名字时换个路径还是从头扫() {
    // 不给 `--library` 就维持老行为（按绝对路径取），已有的中立库照样打得开。
    // 这条同时钉住「老行为没被顺手改掉」。
    let workspace = temp_dir("library-path-workspace");
    let 原地 = temp_dir("library-path-a");
    建_主库(原地.path());

    assert!(扫(workspace.path(), 原地.path(), None).status.success());
    let (新增, _, _) = 增量(workspace.path());
    assert!(新增 > 0);

    let 挪到 = 挪走(&原地, "library-path-b");
    assert!(
        扫(workspace.path(), &挪到.path().join("Game"), None)
            .status
            .success()
    );
    let (新增2, 未变2, _) = 增量(workspace.path());
    assert_eq!(新增2, 新增, "换了绝对路径就是另一份中立库，从头扫");
    assert_eq!(未变2, 0);
}

#[test]
fn 两个不同的主库用同一个名字时报错而不是混表() {
    let workspace = temp_dir("library-clash-workspace");
    let 甲 = temp_dir("library-clash-a");
    建_主库(甲.path());
    assert!(
        扫(workspace.path(), 甲.path(), Some("主库"))
            .status
            .success()
    );

    // 顶层条目全不一样：这个根名底下换了另一块盘。中立库的键是「根名 + 相对那个根的
    // 路径」（ADR-0020），两块盘挤进同一个根名会直接撞车。
    let 乙 = temp_dir("library-clash-b");
    写(&乙.path().join("漫画/第一话.zip"), &zip(64));
    写(&乙.path().join("照片/去年.jpg"), &[0u8; 64]);
    let 出错 = 扫(workspace.path(), 乙.path(), Some("主库"));
    assert!(!出错.status.success(), "该拦下来");
    let 说明 = String::from_utf8_lossy(&出错.stderr);
    assert!(说明.contains("多半不是同一块盘"), "{说明}");
    assert!(说明.contains("换个根名"), "{说明}");
}

#[test]
fn 按名字就出得来报告而不必报出主库在哪() {
    // 盘换了挂载点、甚至根本没插，报告照样出得来（ADR-0009）。
    let workspace = temp_dir("library-report-workspace");
    let 主库 = temp_dir("library-report-lib");
    建_主库(主库.path());
    assert!(
        扫(workspace.path(), 主库.path(), Some("我的库"))
            .status
            .success()
    );

    let 报告 = Command::new(env!("CARGO_BIN_EXE_romcat"))
        .args(["report", "--library", "我的库", "--workspace"])
        .arg(workspace.path())
        .output()
        .expect("能启动 romcat");
    assert!(报告.status.success(), "{报告:?}");
    let 正文 = String::from_utf8_lossy(&报告.stdout);
    assert!(
        正文.contains("超级马里奥.zip") || 正文.contains("文件"),
        "{正文}"
    );
    assert!(
        String::from_utf8_lossy(&报告.stderr).contains("没有读过主库"),
        "报告不该碰主库"
    );
}

#[test]
fn 只给名字出报告时照样不许把报告写进主库() {
    // 只给名字的话，主库在哪只有中立库知道——这道守卫不能因此漏掉（ADR-0004）。
    let workspace = temp_dir("library-guard-workspace");
    let 主库 = temp_dir("library-guard-lib");
    建_主库(主库.path());
    assert!(
        扫(workspace.path(), 主库.path(), Some("守卫"))
            .status
            .success()
    );

    let 落在库里 = 主库.path().join("体检.json");
    let 出错 = Command::new(env!("CARGO_BIN_EXE_romcat"))
        .args(["report", "--library", "守卫", "--workspace"])
        .arg(workspace.path())
        .arg("--json")
        .arg(&落在库里)
        .output()
        .expect("能启动 romcat");
    assert!(!出错.status.success(), "该拦下来");
    assert!(
        String::from_utf8_lossy(&出错.stderr).contains("主库只读"),
        "{出错:?}"
    );
    assert!(!落在库里.exists(), "拦下来之后一个字节都不该写出去");
}

#[test]
fn 既不给路径也不给名字时说清楚该怎么办() {
    let workspace = temp_dir("library-neither-workspace");
    let 报告 = Command::new(env!("CARGO_BIN_EXE_romcat"))
        .args(["report", "--workspace"])
        .arg(workspace.path())
        .output()
        .expect("能启动 romcat");
    assert!(!报告.status.success());
    assert!(
        String::from_utf8_lossy(&报告.stderr).contains("--library"),
        "{报告:?}"
    );
}

/// 跑一次报告。`定位` 是「怎么找到那份库」那几个参数：`--library <名字>`，或者主库根。
fn 出报告(workspace: &Path, 定位: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_romcat"))
        .arg("report")
        .args(定位)
        .arg("--workspace")
        .arg(workspace)
        .arg("--quiet")
        .output()
        .expect("能启动 romcat")
}

#[test]
fn 报告印得出主库名而不是那串带哈希的文件名() {
    // 名字此前只活在人敲过的那行命令里：中立库的文件名是「可读的一段 + 哈希」，
    // 而哈希那半段不可逆。这一条钉住「起的那个名字有个落点，报告里读得回来」。
    let workspace = temp_dir("library-print-workspace");
    let 主库 = temp_dir("library-print-lib");
    建_主库(主库.path());
    assert!(
        扫(workspace.path(), 主库.path(), Some("我的主库"))
            .status
            .success()
    );

    let 报告 = 出报告(workspace.path(), &["--library", "我的主库"]);
    assert!(报告.status.success(), "{报告:?}");
    let 说明 = String::from_utf8_lossy(&报告.stderr);
    assert!(
        说明.lines().any(|line| line == "主库：我的主库"),
        "报告里该印出起的那个名字：{说明}"
    );
}

#[test]
fn 没起名字时报告印的是主库根的末级目录名() {
    // 不给 `--library` 时文件名的可读一半只留 ASCII 字母数字，人认不出是哪块盘。
    // 印出来的该是那个目录真正叫什么。
    let workspace = temp_dir("library-print-path-workspace");
    let 主库 = temp_dir("库名-末级目录");
    建_主库(主库.path());
    assert!(扫(workspace.path(), 主库.path(), None).status.success());

    let 末级 = 主库
        .path()
        .file_name()
        .expect("有末级目录名")
        .to_string_lossy()
        .into_owned();
    let 根 = 主库.path().to_str().expect("临时目录的路径是 UTF-8");
    let 报告 = 出报告(workspace.path(), &[根]);
    assert!(报告.status.success(), "{报告:?}");
    let 说明 = String::from_utf8_lossy(&报告.stderr);
    assert!(
        说明.lines().any(|line| line == format!("主库：{末级}")),
        "报告里该印出那个目录真正叫什么：{说明}"
    );
}

#[test]
fn 扫描那一趟也印得出主库名() {
    // 报告印它、`shape` 印它，而**建库那一趟**原先只印中立库的路径——偏偏名字就是在这一刻
    // 被钉死的，此后这条命令再不改它。人在这时候看不见自己建的这份库叫什么，就得回头去
    // 跑一趟 `report` 才确认得了（挂单 `Q369`）。
    let workspace = temp_dir("library-scan-print-workspace");
    let 主库 = temp_dir("library-scan-print-lib");
    建_主库(主库.path());

    let 扫过 = 扫(workspace.path(), 主库.path(), Some("扫这一趟的主库"));
    assert!(扫过.status.success(), "{扫过:?}");
    let 说明 = String::from_utf8_lossy(&扫过.stderr);
    assert!(
        说明.lines().any(|line| line == "主库：扫这一趟的主库"),
        "扫描收尾该印出起的那个名字：{说明}"
    );
}
