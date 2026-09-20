//! 端到端验收 `romcat adapters` / `import` / `export` 这一层。
//!
//! 收敛规则、往返词法与首选变体那几条由 `romcat-core` 那一侧验
//! （`crates/core/tests/pegasus.rs` 与各模块的单元测试）。这个文件只验命令行：
//! **能力档位对用户可见**、导入不改那些文件、导出检测到外部改动时**以失败收场**
//! 而不是静默覆盖。

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
    let dir = temp_dir("pegasus-cli");
    let workspace = temp_dir("pegasus-cli-ws");
    写(&dir.path().join("FC/魂斗罗.zip"), &zip(2048));
    let out = romcat(&[
        "scan",
        "--root-name",
        "库",
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
    assert!(text.contains("Pegasus"), "{text}");
    assert!(text.contains("无损往返"), "{text}");
    assert!(text.contains("metadata.pegasus.txt"), "{text}");
    assert!(
        text.contains("实测"),
        "要说清「上限是声称的、档位是实测的」：{text}"
    );
    // 四个档位说不出「把值交给这个格式存一趟会变成什么样」。ADR-0003 要的
    // 「导出前就知道会丢掉什么」得在这里说全，而不是等用户导出完自己发现排版变了。
    assert!(
        text.contains("结构性损失"),
        "档位那四个词说不出的那一半也得列：{text}"
    );
    assert!(text.contains("换行"), "单个换行折成空格：{text}");
    assert!(text.contains("U+3000"), "开头的全角空格被掐掉：{text}");
}

#[test]
fn 没有这个格式时列出有哪些() {
    let out = romcat(&["import", "--format", "没这个", "/dev/null"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Pegasus"), "{err}");
}

#[test]
fn 导入干跑报出实测档位而且一个字节都不改那份文件() {
    let (dir, workspace) = 现场();
    let path = dir.path().join("metadata.pegasus.txt");
    let 原文 = "# 我的库\ncollection: FC\n\ngame: 魂斗罗\nfile: FC/魂斗罗.zip\nx-通关: 是\n";
    写(&path, 原文.as_bytes());

    let out = romcat(&[
        "import",
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
    assert!(text.contains("无损往返"), "{text}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("逐字节相同"), "{err}");
    assert!(err.contains("中立库一行没动"), "{err}");
    assert_eq!(
        fs::read_to_string(&path).expect("读得出"),
        原文,
        "导入不改那份文件"
    );
}

#[test]
fn 导出写得出文件_再导一次检测到外部改动就以失败收场() {
    let (dir, workspace) = 现场();
    let out_dir = dir.path();
    let 跑一次 = || {
        romcat(&[
            "export",
            "--out",
            &out_dir.to_string_lossy(),
            "--library",
            "测试库",
            "--workspace",
            &workspace.path().to_string_lossy(),
        ])
    };

    let out = 跑一次();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let target = out_dir.join("FC.metadata.pegasus.txt");
    let 写出来的 = fs::read_to_string(&target).expect("导出来的文件在");
    assert!(写出来的.starts_with("collection: FC"), "{写出来的}");
    assert!(写出来的.contains("game: "), "{写出来的}");

    // 原样再导一次：落点与上次对齐，照写。
    assert!(跑一次().status.success());

    // 有人在工具外面动了它。
    fs::write(&target, format!("{写出来的}\n# 我手加的一行\n")).expect("写得进");
    let out = 跑一次();
    assert!(
        !out.status.success(),
        "检测到外部改动要以失败收场，而不是静默覆盖"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("没有静默覆盖"), "{err}");
    assert!(
        fs::read_to_string(&target)
            .expect("读得出")
            .contains("# 我手加的一行"),
        "手改的那一行还在"
    );
}

#[test]
fn 干跑不写盘() {
    let (dir, workspace) = 现场();
    let out_dir = temp_dir("pegasus-cli-out");
    let out = romcat(&[
        "export",
        "--out",
        &out_dir.path().to_string_lossy(),
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
    assert!(
        !out_dir.path().join("FC.metadata.pegasus.txt").exists(),
        "`--dry-run` 一个字节都不写盘"
    );
    let _ = dir;
}

/// 往工作目录里那份**媒体池**塞一份封面，并在中立库里把它挂到那个变体上。返回内容哈希。
fn 塞一份封面(workspace: &Path, 变体: &str, 字节: &[u8]) -> String {
    use romcat_core::catalog::Catalog;
    use romcat_core::workspace::{self, Slug};

    let hash = romcat_core::catalog::frontend::hash_of(字节);
    let pool = romcat_core::scrape::pool::MediaPool::at(&workspace::media_pool_dir(workspace));
    写(&pool.path_of(&hash, "png"), 字节);
    let mut catalog = Catalog::open(&workspace::catalog_path(workspace, Slug::Named("测试库")))
        .expect("开得了中立库");
    挂上(&mut catalog, 变体, &hash, 字节.len() as u64);
    hash
}

fn 挂上(catalog: &mut romcat_core::catalog::Catalog, 变体: &str, hash: &str, bytes: u64) {
    use romcat_core::catalog::scrape::{Harvested, HarvestedMedia};
    use romcat_core::scrape::{AnchorKind, MediaKind};

    catalog
        .put_media(
            hash,
            "png",
            bytes,
            romcat_core::scrape::measure::Measured::default(),
        )
        .expect("池里记得下");
    catalog
        .put_scraped(&[Harvested {
            anchor: AnchorKind::Variant.label().to_string(),
            subject: 变体.to_string(),
            source: "本地媒体".to_string(),
            input: format!("{变体}/{hash}"),
            values: Vec::new(),
            media: vec![HarvestedMedia {
                kind: MediaKind::Cover.label().to_string(),
                hash: hash.to_string(),
                evidence: "测试".to_string(),
            }],
        }])
        .expect("引用写得进");
}

#[test]
fn 导出加上铺媒体才铺_干跑先说要铺几份多大() {
    // 票 `one-criterion-per-thing/08`：`--media` 默认关着。媒体池里塞一份挂在魂斗罗上的
    // 封面，三趟各验一件：干跑说得出要付多少、不加开关一份都不铺、加上开关照 Pegasus 的
    // 布局铺进 `media/`。
    let (dir, workspace) = 现场();
    let 封面 = b"\x89PNG-- cli cover --";
    let hash = 塞一份封面(workspace.path(), "库/FC/魂斗罗.zip", 封面);
    let out_dir = dir.path().to_string_lossy().into_owned();
    let ws = workspace.path().to_string_lossy().into_owned();
    let 导出 = |extra: &[&str]| {
        let mut args = vec![
            "export",
            "--out",
            out_dir.as_str(),
            "--library",
            "测试库",
            "--workspace",
            ws.as_str(),
        ];
        args.extend_from_slice(extra);
        romcat(&args)
    };

    // 干跑：说得出要铺几份、多大，一份都不铺。
    let out = 导出(&["--media", "--dry-run"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("1 份，共"),
        "按下之前得说得出要铺几份、多大：{text}"
    );
    assert!(!dir.path().join("media").exists(), "`--dry-run` 一份都不铺");

    // 不加开关：一份都不铺，报告里连这一节都没有。
    let out = 导出(&[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!dir.path().join("media").exists(), "默认关着");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(!text.contains("媒体池 → 媒体目录"), "{text}");

    // 加上开关：照 Pegasus 的布局铺进 `media/`，条目里写着那条路径。
    let out = 导出(&["--media"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let 落点 = format!("media/{}/{hash}.png", &hash[..2]);
    assert_eq!(fs::read(dir.path().join(&落点)).expect("铺出去了"), 封面);
    let 元数据 =
        fs::read_to_string(dir.path().join("FC.metadata.pegasus.txt")).expect("元数据写出来了");
    assert!(
        元数据.contains(&format!("assets.boxFront: {落点}")),
        "{元数据}"
    );
}
