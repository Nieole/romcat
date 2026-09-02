//! 端到端验收 `romcat triage`：列队列 → 批量裁决 → 沉淀 → 导出 → 下一趟直接命中。
//!
//! 密集逻辑在核心库那一侧（`romcat-core` 的 `tests/triage.rs`）。这里只验命令行这一层：
//! **一次改不止一条时要点头**、沉淀库**不跟中立库走**（删掉中立库重扫也不丢裁决）、
//! 导出的那份 JSON 收得回来。

use std::fs;
use std::path::Path;
use std::process::Command;

use romcat_core::catalog::{Catalog, State};
use romcat_core::dat::Convention;
use romcat_core::dat::logiqx::{DatHeader, GameRecord, RomRecord};
use romcat_core::dat::repo::{DatMeta, DatRepo, Unit};
use romcat_core::testing::container::{ZipEntrySpec, crc32, zip_container};
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::workspace;

fn 跑(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_romcat"))
        .args(args)
        .output()
        .expect("跑得起来")
}

fn 卡带(fill: u8) -> Vec<u8> {
    let mut data = vec![0u8; 16];
    data[..4].copy_from_slice(b"NES\x1A");
    data.extend(std::iter::repeat_n(fill, 40_960));
    data
}

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写");
}

/// 一个小主库：一份 DAT 认得的原版，三份 DAT 里一条都没有的汉化版。
fn 现场() -> (TempDir, TempDir) {
    let library = temp_dir("triage-cli-库");
    let workspace = temp_dir("triage-cli-工作");
    写(
        &library.path().join("FC/原版.zip"),
        &zip_container(&[ZipEntrySpec::stored("smb.nes", 卡带(0xA1))]),
    );
    for (index, name) in [
        "FC/甲 外星科技汉化.zip",
        "FC/乙 外星科技汉化.zip",
        "FC/丙 别家汉化.zip",
    ]
    .into_iter()
    .enumerate()
    {
        写(
            &library.path().join(name),
            &zip_container(&[ZipEntrySpec::stored(
                "rom.nes",
                卡带(0xB0 + u8::try_from(index).expect("装得下")),
            )]),
        );
    }

    let mut repo = DatRepo::open(&workspace::dat_repo_path(workspace.path())).expect("开得出来");
    let mut writer = repo
        .begin(&Unit {
            source: "TOSEC".to_string(),
            name: "fc.dat".to_string(),
            url: "https://example.invalid/x".to_string(),
            fingerprint: "sha".to_string(),
        })
        .expect("事务");
    writer
        .write_dat(
            &DatMeta {
                name: "Nintendo Famicom - Games".to_string(),
                platform: "FC".to_string(),
                convention: Convention::AsIs,
                header: DatHeader::default(),
            },
            &[GameRecord {
                name: "Super Mario Bros. (1985)(Nintendo)".to_string(),
                roms: vec![RomRecord {
                    name: "smb.nes".to_string(),
                    size: Some(40_976),
                    crc32: Some(crc32(&卡带(0xA1))),
                    ..RomRecord::default()
                }],
                ..GameRecord::default()
            }],
        )
        .expect("写");
    writer.commit().expect("提交");
    (library, workspace)
}

fn 扫并识别(library: &Path, workspace: &Path) {
    for args in [
        vec![
            "scan",
            &*library.to_string_lossy(),
            "--library",
            "小库",
            "--workspace",
            &workspace.to_string_lossy(),
            "--quiet",
        ],
        vec![
            "identify",
            &library.to_string_lossy(),
            "--library",
            "小库",
            "--workspace",
            &workspace.to_string_lossy(),
            "--quiet",
        ],
    ] {
        let out = 跑(&args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

fn 开中立库(workspace: &Path) -> Catalog {
    Catalog::open(&workspace::catalog_path(
        workspace,
        workspace::Slug::Named("小库"),
    ))
    .expect("开得出中立库")
}

#[test]
fn 队列列得出来并说清一条命令覆盖多少() {
    let (library, workspace) = 现场();
    扫并识别(library.path(), workspace.path());
    let out = 跑(&[
        "triage",
        "list",
        "--library",
        "小库",
        "--workspace",
        &workspace.path().to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("待确认队列"), "{text}");
    assert!(text.contains("3 条待裁决"), "{text}");
    // 三个批量的轴各印一张表——报告直接说清一条命令值多少。
    assert!(text.contains("按目录——一条 `--under` 覆盖多少"), "{text}");
    assert!(
        text.contains("按候选作品——一条 `--candidate-work` 覆盖多少"),
        "{text}"
    );
    // 自动通过的那份原版不占人的时间。
    assert!(!text.contains("原版.zip"), "{text}");
}

#[test]
fn 还没识别过时说得清下一步() {
    let (library, workspace) = 现场();
    let out = 跑(&[
        "scan",
        &library.path().to_string_lossy(),
        "--library",
        "小库",
        "--workspace",
        &workspace.path().to_string_lossy(),
        "--quiet",
    ]);
    assert!(out.status.success());
    let out = 跑(&[
        "triage",
        "list",
        "--library",
        "小库",
        "--workspace",
        &workspace.path().to_string_lossy(),
    ]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("romcat identify"), "{text}");
}

#[test]
fn 一次改不止一条要点头() {
    let (library, workspace) = 现场();
    扫并识别(library.path(), workspace.path());
    let 工作目录 = workspace.path().to_string_lossy().into_owned();
    let 命令 = |extra: &[&str]| {
        let mut args = vec![
            "triage",
            "decide",
            "--library",
            "小库",
            "--workspace",
            &*工作目录,
            "--name",
            "外星科技",
            "--work",
            "某部作品",
        ];
        args.extend_from_slice(extra);
        跑(&args)
    };
    // 不点头：印出计划，然后停下。
    let out = 命令(&[]);
    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("批量裁决计划"), "{stdout}");
    assert!(stdout.contains("要裁            2 条"), "{stdout}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("--yes"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // 一个字都没写。
    assert_eq!(
        开中立库(workspace.path())
            .identification_of("FC/甲 外星科技汉化.zip")
            .expect("读得出")
            .expect("有结论")
            .0,
        State::Unmatched
    );

    // --dry-run 也一个字都不写。
    assert!(命令(&["--dry-run"]).status.success());

    // 点头之后落下去。
    let out = 命令(&["--yes", "--team", "外星科技", "--version", "v1.2"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("裁决已沉淀 2 条"), "{stdout}");
    assert_eq!(
        开中立库(workspace.path())
            .identification_of("FC/甲 外星科技汉化.zip")
            .expect("读得出")
            .expect("有结论")
            .0,
        State::Matched,
        "裁决当场兑现，不必等下一趟识别"
    );
}

#[test]
fn 沉淀库不跟中立库走删掉中立库重扫也不丢裁决() {
    // 这是原挂账 D26 的正面回答：中立库里每一条都可再生，所以它照旧走「删库重扫」；
    // 而**裁决不可再生**，于是它单独一份文件。
    let (library, workspace) = 现场();
    扫并识别(library.path(), workspace.path());
    let out = 跑(&[
        "triage",
        "decide",
        "--library",
        "小库",
        "--workspace",
        &workspace.path().to_string_lossy(),
        "--name",
        "别家汉化",
        "--work",
        "某部没人收录的作品",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // 把中立库整个删掉，重扫重识别。
    fs::remove_file(workspace::catalog_path(
        workspace.path(),
        workspace::Slug::Named("小库"),
    ))
    .expect("删得掉");
    扫并识别(library.path(), workspace.path());

    let catalog = 开中立库(workspace.path());
    let (state, _) = catalog
        .identification_of("FC/丙 别家汉化.zip")
        .expect("读得出")
        .expect("有结论");
    assert_eq!(state, State::Matched, "裁决活了下来");
    let 候选 = catalog.candidates_of("FC/丙 别家汉化.zip").expect("读得出");
    assert_eq!(候选.len(), 1);
    assert_eq!(候选[0].source, "沉淀库");
    assert!(候选[0].accepted);
    assert!(候选[0].evidence.contains("裁决"), "{}", 候选[0].evidence);
}

#[test]
fn 导出再收回来是同一份() {
    let (library, workspace) = 现场();
    扫并识别(library.path(), workspace.path());
    assert!(
        跑(&[
            "triage",
            "decide",
            "--library",
            "小库",
            "--workspace",
            &workspace.path().to_string_lossy(),
            "--name",
            "外星科技",
            "--work",
            "某部作品",
            "--team",
            "外星科技",
            "--yes",
        ])
        .status
        .success()
    );
    let 文件 = workspace.path().join("分享.json");
    let out = 跑(&[
        "triage",
        "export",
        "--workspace",
        &workspace.path().to_string_lossy(),
        "--out",
        &文件.to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = fs::read_to_string(&文件).expect("读得出");
    assert!(text.contains("romcat-沉淀库"), "{text}");
    assert!(text.contains("外星科技"), "{text}");
    // 分享出去的那份**不带路径**——不然顺带把自己的目录结构也交出去了。
    assert!(!text.contains("FC/甲"), "{text}");

    // 另一台机器收下它。
    let 别处 = temp_dir("triage-cli-别处");
    let out = 跑(&[
        "triage",
        "import",
        "--workspace",
        &别处.path().to_string_lossy(),
        &文件.to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("读到 2 条，新收 2"), "{stdout}");
}

#[test]
fn 说不成立的话当场说不成立而不是静默丢掉() {
    // 「没有发行版」与「认不出」说的是「它不成其为一次发行」，那就没有汉化组可记。
    // 收下再默默扔掉的话，计划书上印着「汉化组 外星科技」，库里却一个字都没记。
    let (library, workspace) = 现场();
    扫并识别(library.path(), workspace.path());
    for 裁法 in ["--no-release", "--unknown"] {
        let out = 跑(&[
            "triage",
            "decide",
            "--library",
            "小库",
            "--workspace",
            &workspace.path().to_string_lossy(),
            "--under",
            "FC",
            裁法,
            "--team",
            "外星科技",
        ]);
        assert!(!out.status.success(), "{裁法} 配 --team 该被挡下");
        let text = String::from_utf8_lossy(&out.stderr);
        assert!(text.contains("不成其为一次发行"), "{text}");
    }
}

#[test]
fn 裁成什么没说清就停下() {
    let (library, workspace) = 现场();
    扫并识别(library.path(), workspace.path());
    let out = 跑(&[
        "triage",
        "decide",
        "--library",
        "小库",
        "--workspace",
        &workspace.path().to_string_lossy(),
        "--under",
        "FC",
    ]);
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("没说要裁成什么"), "{text}");
}

#[test]
fn 忘掉裁决要给条件也要点头() {
    let (library, workspace) = 现场();
    扫并识别(library.path(), workspace.path());
    assert!(
        跑(&[
            "triage",
            "decide",
            "--library",
            "小库",
            "--workspace",
            &workspace.path().to_string_lossy(),
            "--name",
            "外星科技",
            "--work",
            "某部作品",
            "--yes",
        ])
        .status
        .success()
    );
    // 空选择器 = 把整份沉淀库忘掉，不接受。
    let out = 跑(&[
        "triage",
        "undo",
        "--library",
        "小库",
        "--workspace",
        &workspace.path().to_string_lossy(),
    ]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("空的选择器"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = 跑(&[
        "triage",
        "undo",
        "--library",
        "小库",
        "--workspace",
        &workspace.path().to_string_lossy(),
        "--name",
        "外星科技",
        "--yes",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("已忘掉 2 条"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}
