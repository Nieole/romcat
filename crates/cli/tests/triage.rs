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
            "--root-name",
            "库",
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
        "--root-name",
        "库",
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
            .identification_of("库/FC/甲 外星科技汉化.zip")
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
            .identification_of("库/FC/甲 外星科技汉化.zip")
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
        .identification_of("库/FC/丙 别家汉化.zip")
        .expect("读得出")
        .expect("有结论");
    assert_eq!(state, State::Matched, "裁决活了下来");
    let 候选 = catalog
        .candidates_of("库/FC/丙 别家汉化.zip")
        .expect("读得出");
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
    assert!(!text.contains("库/FC/甲"), "{text}");

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
            "库/FC",
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
        "库/FC",
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

#[test]
fn 按批撤销之后当场列队列就看得见它们回来了() {
    // 票 gui-redesign/08 的要害：**不必重跑识别**。命令行这一层验的是那句话真的印出来了，
    // 而且 `triage list` 当场数得出那两条回到了队列里（原挂账 D102 被推翻的那一条）。
    let (library, workspace) = 现场();
    扫并识别(library.path(), workspace.path());
    let 工作目录 = workspace.path().to_string_lossy().into_owned();

    let out = 跑(&[
        "triage",
        "decide",
        "--library",
        "小库",
        "--workspace",
        &工作目录,
        "--name",
        "外星科技",
        "--work",
        "某部作品",
        "--yes",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let 落下 = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(落下.contains("这是第 1 批"), "{落下}");
    assert!(
        落下.contains("undo --batch 1"),
        "按错了怎么走回来要印在这儿：{落下}"
    );

    // `batches` 列得出这一批：编号、条数、裁成什么。
    let out = 跑(&[
        "triage",
        "batches",
        "--library",
        "小库",
        "--workspace",
        &工作目录,
    ]);
    let 列表 = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(列表.contains("#1"), "{列表}");
    assert!(列表.contains("2 条"), "{列表}");
    assert!(
        列表.contains("作品《某部作品》"),
        "摘要要与计划书上那句话对得上：{列表}"
    );

    // 一次撤不止一条要点头。
    let out = 跑(&[
        "triage",
        "undo",
        "--library",
        "小库",
        "--workspace",
        &工作目录,
        "--batch",
        "1",
    ]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("--yes"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = 跑(&[
        "triage",
        "undo",
        "--library",
        "小库",
        "--workspace",
        &工作目录,
        "--batch",
        "1",
        "--yes",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let 撤回 = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(撤回.contains("已撤 2 条"), "{撤回}");
    assert!(撤回.contains("不必重跑识别"), "{撤回}");

    // **不重跑识别**，当场列队列：那两条回来了。
    let out = 跑(&[
        "triage",
        "list",
        "--library",
        "小库",
        "--workspace",
        &工作目录,
        "--limit",
        "0",
    ]);
    let 队列 = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        队列.contains("队列            3 条待裁决"),
        "撤完该回到裁决之前的 3 条：{队列}"
    );

    // 撤销本身撤得回来。
    let out = 跑(&[
        "triage",
        "redo",
        "--library",
        "小库",
        "--workspace",
        &工作目录,
        "--last",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("第 1 批放回去了"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let out = 跑(&[
        "triage",
        "list",
        "--library",
        "小库",
        "--workspace",
        &工作目录,
        "--limit",
        "0",
    ]);
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("队列            1 条待裁决"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// 再装一份 GoodNES：给「甲」那份汉化版一条**只凭 CRC-32 撞上**的候选（中置信）。
///
/// 队列于是分成两批——一批带候选、一批一条候选都没有。一批的时候 `--shape` 选中的
/// 恒等于整个队列，那条断言就分不出它到底筛没筛。
fn 装_goodnes(workspace: &Path) {
    let mut repo = DatRepo::open(&workspace::dat_repo_path(workspace)).expect("开得出来");
    let mut writer = repo
        .begin(&Unit {
            source: "GoodNES".to_string(),
            name: "good.dat".to_string(),
            url: "https://example.invalid/z".to_string(),
            fingerprint: "sha3".to_string(),
        })
        .expect("事务");
    writer
        .write_dat(
            &DatMeta {
                name: "GoodNES 3.23".to_string(),
                platform: "FC".to_string(),
                convention: Convention::AsIs,
                header: DatHeader::default(),
            },
            // **没记大小**，只凭 CRC-32 撞上——通过但标记，中置信（ADR-0002）。
            &[GameRecord {
                name: "Jia [T+Chi]".to_string(),
                roms: vec![RomRecord {
                    name: "jia.nes".to_string(),
                    size: None,
                    crc32: Some(crc32(&卡带(0xB0))),
                    ..RomRecord::default()
                }],
                ..GameRecord::default()
            }],
        )
        .expect("写");
    writer.commit().expect("提交");
}

/// 报告里那几条 `--shape '…'`，照抄的次序，去重。
fn 抄下那几串字(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in text.split("--shape '").skip(1) {
        let 一串 = part.split('\'').next().unwrap_or_default().to_string();
        if !一串.is_empty() && !out.contains(&一串) {
            out.push(一串);
        }
    }
    out
}

#[test]
fn 报告印出来的那串字照抄一条就选中同一批() {
    // 票 `queue-followups/08` 的验收第 2 条。**按依据形状**从前只有界面上点得到，
    // 命令行选不出同一批。这条走的是完整一趟：报告把每一批连它那串字一起印出来，
    // 照抄一条 `--shape` 回去，选中的条数与报告上写的那一批**一个数都不差**。
    let (library, workspace) = 现场();
    装_goodnes(workspace.path());
    扫并识别(library.path(), workspace.path());
    let 工作目录 = workspace.path().to_string_lossy().into_owned();
    let 账本 = workspace.path().join("账.json");
    let 账本路径 = 账本.to_string_lossy().into_owned();

    let out = 跑(&[
        "triage",
        "list",
        "--library",
        "小库",
        "--workspace",
        &工作目录,
        "--json",
        &账本路径,
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let 文本 = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        文本.contains("按依据形状——一条 `--shape` 覆盖多少"),
        "{文本}"
    );
    // **最值钱的那一批连整条命令一起给**，而且带着这一趟用的库选择器——
    // 不带的话粘到别处开的是另一份库。
    assert!(
        文本.contains(
            "romcat triage decide --shape 'GoodNES / GoodNES 3.23 / 中置信 / 含头 / 只有一个候选' \
             --library '小库' --pick 1 --dry-run"
        ),
        "{文本}"
    );

    let 报告: serde_json::Value =
        serde_json::from_slice(&fs::read(&账本).expect("报告该写出来")).expect("是 JSON");
    let 几批 = 报告["by_shape"]
        .as_array()
        .expect("报告里该有这张表")
        .clone();
    assert_eq!(几批.len(), 2, "一批带候选、一批一条候选都没有：{几批:#?}");
    assert_eq!(
        几批
            .iter()
            .map(|one| one["count"].as_u64().expect("是数"))
            .sum::<u64>(),
        3,
        "各批加起来就是整个队列",
    );
    assert_eq!(
        抄下那几串字(&文本)
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        几批
            .iter()
            .map(|one| one["selector"].as_str().expect("是串字").to_string())
            .collect::<std::collections::BTreeSet<_>>(),
        "屏上印出来的与报告里记着的不是同一批串字：{文本}",
    );

    // 照抄每一批那串字回去：选中的必须正好是报告上写的那个数。
    for 一批 in &几批 {
        let 那串字 = 一批["selector"].as_str().expect("是串字").to_string();
        let 该有几条 = 一批["count"].as_u64().expect("是数");
        let 再一份 = workspace.path().join("再.json");
        let 再一份路径 = 再一份.to_string_lossy().into_owned();
        let out = 跑(&[
            "triage",
            "list",
            "--library",
            "小库",
            "--workspace",
            &工作目录,
            "--shape",
            &那串字,
            "--json",
            &再一份路径,
            "--quiet",
        ]);
        assert!(
            out.status.success(),
            "{那串字}：{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let 这一份: serde_json::Value =
            serde_json::from_slice(&fs::read(&再一份).expect("写得出")).expect("是 JSON");
        assert_eq!(这一份["selected"].as_u64(), Some(该有几条), "{那串字}");
        assert!(该有几条 < 3, "选中的与整个队列一样多，这条就分不出它筛没筛");
        assert_eq!(
            这一份["by_shape"].as_array().map(Vec::len),
            Some(1),
            "选一批出来该只剩这一批：{那串字}"
        );
    }

    // **那条可粘贴的命令得把这一趟的选择器一样不少地带上。** 那张表上的条数是选择器
    // 筛过之后数出来的；只带 `--shape` 粘过去跑的是全库那一批，旁边写的数当场变成假的。
    let out = 跑(&[
        "triage",
        "list",
        "--library",
        "小库",
        "--workspace",
        &工作目录,
        "--name",
        "甲",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let 筛过的 = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(筛过的.contains("从最值钱的那一批下手（1 条）"), "{筛过的}");
    assert!(
        筛过的.contains("只有一个候选' --name '甲' --library '小库' --pick 1 --dry-run"),
        "这一趟的 `--name` 没带上，粘过去跑的就是另一批：{筛过的}"
    );
}

#[test]
fn 依据形状写岔了当场说清该怎么写() {
    // 静悄悄选中零条是最坏的一种：人会以为这一批真的空了。
    let (library, workspace) = 现场();
    扫并识别(library.path(), workspace.path());
    let 工作目录 = workspace.path().to_string_lossy().into_owned();
    let 敲 = |shape: &str| {
        跑(&[
            "triage",
            "list",
            "--library",
            "小库",
            "--workspace",
            &工作目录,
            "--shape",
            shape,
        ])
    };

    let out = 敲("MAME / nes.xml");
    assert!(!out.status.success());
    let 抱怨 = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(抱怨.contains("不是一个依据形状"), "{抱怨}");
    assert!(
        抱怨.contains("romcat triage list"),
        "得说清上哪儿抄：{抱怨}"
    );

    // 形状认得下来、库里却没有这一批：那**不是写错**，是真的一条都没有——两句话不一样，
    // 所以这一趟照样算跑成了，只是报告上写着一条都没选中。
    let out = 敲("一条候选都没有 / 无判据");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("一条都没选中"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn 导出不许把那份_json_写进主库() {
    // ADR-0004 那条红线：工具写出去的任何文件都不许落进主库（10 TB 不可再生）。
    // 那道守卫十三处里十二处都接了，`triage export --out` 是唯一漏掉的一处（挂账 `D104`）
    // ——它是工作目录级的命令、票面上没有 `--root`，于是没人给它一个根去比。
    let (library, workspace) = 现场();
    扫并识别(library.path(), workspace.path());

    let 落在库里 = library.path().join("分享.json");
    let out = 跑(&[
        "triage",
        "export",
        "--workspace",
        &workspace.path().to_string_lossy(),
        "--out",
        &落在库里.to_string_lossy(),
    ]);
    assert!(
        !out.status.success(),
        "该拒绝：{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let 说明 = String::from_utf8_lossy(&out.stderr);
    assert!(说明.contains("主库只读"), "拒绝的话要说清为什么：{说明}");
    assert!(!落在库里.exists(), "拒绝之后主库里不该多出这个文件");

    // 写到别处照旧走得通——这道守卫不能把合法输出也拦下来。
    let 写别处 = workspace.path().join("分享.json");
    let 好的 = 跑(&[
        "triage",
        "export",
        "--workspace",
        &workspace.path().to_string_lossy(),
        "--out",
        &写别处.to_string_lossy(),
    ]);
    assert!(
        好的.status.success(),
        "{}",
        String::from_utf8_lossy(&好的.stderr)
    );
    assert!(写别处.exists(), "写到工作目录里该成功");
}
