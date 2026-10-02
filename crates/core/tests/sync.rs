//! 验收**同步计划器与差量预览**这一层。
//!
//! 计划器本身是纯函数，它的细节在 `sync` 的单元测试里逐条验。这个文件验三件别处
//! 验不了的事：
//!
//! 1. **那条硬约束是一条性质，不是一句注释**——穷举「期望 / 清单 / 实际」的每一种
//!    组合，断言删除项只可能来自清单（ADR-0015）。
//! 2. **期望状态真的是从中立库折出来的**：磁盘上摆一份 fixture 主库，扫、成型、
//!    写规则，折出来的文件与真实的成员一一对得上。
//! 3. **整条链路串得起来**：选择集 → 期望状态 → 看一遍目标 → 计划，一个文件都不写。

use std::fs;
use std::path::{Path, PathBuf};

use romcat_core::capability::Profile;
use romcat_core::capability::{Filesystem, RejectReason};
use romcat_core::catalog::{Catalog, Roots, roots};
use romcat_core::fs::RealFs;
use romcat_core::scan::{self, Jobs, ScanOptions};
use romcat_core::site::Site;
use romcat_core::sublibrary::{self, Rule, Selection, Sublibrary};
use romcat_core::sync::{
    self, Act, Desired, DesiredFile, FileKind, Manifest, ManifestFile, Options, Rejected, Stamp,
    SurpriseKind, TargetFile, TargetState,
};
use romcat_core::task::Handle;
use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::verdict::Store;

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

fn 子库(target: &Path, capacity: Option<u64>) -> Sublibrary {
    Sublibrary::at("掌机", target, "Pegasus", capacity)
}

// ───────────────────────── 一、那条硬约束是一条可断言的性质

/// 目标上那个文件是什么状况。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum 目标态 {
    没有,
    与清单一致,
    被改过,
    读不到,
}

/// 一种「期望 / 清单 / 实际」的组合。
#[derive(Debug, Clone, Copy)]
struct 组合 {
    要不要: bool,
    清单里有: bool,
    目标: 目标态,
}

/// 穷举全部组合，每种一条路径。
fn 全部组合() -> Vec<(String, 组合)> {
    let mut out = Vec::new();
    for 要不要 in [false, true] {
        for 清单里有 in [false, true] {
            for 目标 in [
                目标态::没有,
                目标态::与清单一致,
                目标态::被改过,
                目标态::读不到,
            ] {
                let path = format!(
                    "库/组合/{}-{}-{:?}.zip",
                    u8::from(要不要),
                    u8::from(清单里有),
                    目标
                );
                out.push((
                    path,
                    组合 {
                        要不要,
                        清单里有,
                        目标,
                    },
                ));
            }
        }
    }
    // 清单之外、期望之外、目标上却实实在在躺着的东西：存档、金手指、截图。
    for 名字 in [
        "saves/口袋妖怪.sav",
        "cheats/金手指.txt",
        "screenshots/一.png",
    ] {
        out.push((
            名字.to_string(),
            组合 {
                要不要: false,
                清单里有: false,
                目标: 目标态::被改过,
            },
        ));
    }
    out
}

fn 摆好现场(组合们: &[(String, 组合)]) -> (Desired, Manifest, TargetState) {
    let 记着 = Stamp {
        bytes: 1024,
        mtime_ns: Some(1_700_000_000_000_000_000),
    };
    let mut desired = Desired::default();
    let mut manifest = Manifest::default();
    let mut actual = TargetState::default();
    for (path, 这一种) in 组合们 {
        if 这一种.要不要 {
            desired.files.push(DesiredFile {
                path: path.clone(),
                kind: FileKind::Rom,
                bytes: 1024,
                unreadable: false,
                source: path.clone(),
                source_stamp: 记着,
                variant: path.clone(),
                convert: None,
            });
        }
        if 这一种.清单里有 {
            manifest.files.push(ManifestFile {
                path: path.clone(),
                kind: FileKind::Rom,
                stamp: 记着,
                source: path.clone(),
                source_stamp: 记着,
                variant: path.clone(),
                absent: false,
            });
        }
        let stamp = match 这一种.目标 {
            目标态::没有 => continue,
            目标态::与清单一致 => Some(记着),
            目标态::被改过 => Some(Stamp {
                bytes: 4096,
                mtime_ns: Some(1_800_000_000_000_000_000),
            }),
            目标态::读不到 => None,
        };
        actual.files.push(TargetFile {
            path: path.clone(),
            stamp,
        });
    }
    (desired, manifest, actual)
}

#[test]
fn 计划里的删除项只可能来自清单_清单之外的文件永不出现在删除列表中() {
    let dir = temp_dir("sync-plan-prop");
    let 组合们 = 全部组合();
    let (desired, manifest, actual) = 摆好现场(&组合们);

    for restore in [false, true] {
        let plan = sync::plan(
            &子库(dir.path(), None),
            &desired,
            &manifest,
            &actual,
            Options {
                restore_missing: restore,
            },
        );
        let 清单里的: Vec<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
        let 期望里的: Vec<&str> = desired.files.iter().map(|f| f.path.as_str()).collect();
        let 目标上的: Vec<&str> = actual.files.iter().map(|f| f.path.as_str()).collect();

        for step in &plan.steps {
            let path = step.path.as_str();
            match step.act {
                // **删与改一律来自清单。** 这是 ADR-0015 那条硬约束的全部内容。
                Act::Delete | Act::Update => assert!(
                    清单里的.contains(&path),
                    "{path} 不在清单里却被 {} 了（restore={restore}）",
                    step.act.label()
                ),
                // 新增只落在**目标上根本没有东西**的位置：落点上有别人的文件就不覆盖。
                Act::Add => {
                    assert!(期望里的.contains(&path), "{path} 不在期望里却要新增");
                    assert!(
                        !目标上的.contains(&path),
                        "{path} 落点上有东西却仍要写（restore={restore}）"
                    );
                }
            }
        }
        // 清单之外、期望之外的三个文件：连成为一条步骤的路径都没有。
        for 名字 in [
            "saves/口袋妖怪.sav",
            "cheats/金手指.txt",
            "screenshots/一.png",
        ] {
            assert!(
                plan.steps.iter().all(|step| step.path != 名字),
                "{名字} 出现在计划里（restore={restore}）"
            );
        }
        // 九个：那三个手动拷进去的，加上组合表里目标上有东西、清单里却没有的六种
        // （期望要不要各三种：一致 / 被改过 / 读不到）。**落点被占的那三个也算**——
        // ADR-0015 的原话是「清单之外的一切文件对工具不存在」，漏数哪一个，
        // 报告都可能说出「目标上没有清单之外的文件」而卡上明明有。
        assert_eq!(plan.strangers, 9, "清单之外的如实数出来");
        assert!(
            plan.render_text().contains("清单之外"),
            "报告里说得出这一栏"
        );
    }
}

#[test]
fn 每一种组合的结论都是说好的那一个() {
    let dir = temp_dir("sync-plan-table");
    let 组合们 = 全部组合();
    let (desired, manifest, actual) = 摆好现场(&组合们);
    let plan = sync::plan(
        &子库(dir.path(), None),
        &desired,
        &manifest,
        &actual,
        Options::default(),
    );
    let 步 = |path: &str| {
        plan.steps
            .iter()
            .find(|step| step.path == path)
            .map(|s| s.act)
    };
    let 意外 = |path: &str| {
        plan.surprises
            .iter()
            .find(|s| s.path == path)
            .map(|s| s.kind)
    };

    for (path, 这一种) in &组合们 {
        let 结论 = (步(path), 意外(path));
        let 该是 = match (这一种.要不要, 这一种.清单里有, 这一种.目标) {
            // 要、清单里有、目标上还是那份 → 原样留着，一步都不用走。
            (true, true, 目标态::与清单一致) => (None, None),
            (true, true, 目标态::没有) => (None, Some(SurpriseKind::Gone)),
            (true, true, 目标态::被改过) => (None, Some(SurpriseKind::Changed)),
            (true, true, 目标态::读不到) => (None, Some(SurpriseKind::Unreadable)),
            (true, false, 目标态::没有) => (Some(Act::Add), None),
            (true, false, 目标态::读不到) => (None, Some(SurpriseKind::Unreadable)),
            // 落点上有个清单之外的文件挡着——**那多半就是自己拷进去的**，不覆盖。
            (true, false, _) => (None, Some(SurpriseKind::Occupied)),
            (false, true, 目标态::与清单一致) => (Some(Act::Delete), None),
            (false, true, 目标态::没有) => (None, Some(SurpriseKind::Gone)),
            (false, true, 目标态::被改过) => (None, Some(SurpriseKind::Changed)),
            (false, true, 目标态::读不到) => (None, Some(SurpriseKind::Unreadable)),
            // 清单之外、期望之外：只数一数。
            (false, false, _) => (None, None),
        };
        assert_eq!(结论, 该是, "{path} {这一种:?}");
    }
}

#[test]
fn 目标上的意外变化被报告而不是静默补回() {
    let dir = temp_dir("sync-restore");
    let (desired, manifest, actual) = 摆好现场(&全部组合());
    let 默认 = sync::plan(
        &子库(dir.path(), None),
        &desired,
        &manifest,
        &actual,
        Options::default(),
    );
    let 没了的: Vec<&str> = 默认
        .surprises
        .iter()
        .filter(|s| s.kind == SurpriseKind::Gone && s.still_wanted)
        .map(|s| s.path.as_str())
        .collect();
    assert_eq!(没了的.len(), 1, "清单说有、实际没了、而且还要它");
    assert!(
        默认.steps.iter().all(|step| step.path != 没了的[0]),
        "默认不补回——那可能是在掌机上有意删的"
    );

    // 明说 `--restore` 才补，而且照样进意外那一栏：**明知故犯不是静默**。
    let 补回 = sync::plan(
        &子库(dir.path(), None),
        &desired,
        &manifest,
        &actual,
        Options {
            restore_missing: true,
        },
    );
    let 补的: Vec<&str> = 补回
        .steps
        .iter()
        .filter(|step| step.restore)
        .map(|step| step.path.as_str())
        .collect();
    assert_eq!(补的, 没了的);
    assert_eq!(补回.surprises.len(), 默认.surprises.len());
}

#[test]
fn 补回之后新增几个_开没开补回答的是同一个数_勾上之后等于新增那一格() {
    // 票 `gui-draws-the-rest-of-the-design/15`（差距 C-1）：界面上补回那一格说明的前半句「补回后新增变为 N 个」
    // 要这个数。它得开没开补回都答同一个——人是在勾之前读它的——而勾上之后它就是新增那一格。
    let dir = temp_dir("sync-adds-if-restored");
    let (mut desired, mut manifest, actual) = 摆好现场(&全部组合());
    // 再摆一份**你删过、工具记着不补**的：清单记着它不在，目标上也没有，选择集还要它。
    // 它不进意外那一栏（上一趟已经报过），可补回补的也有它。
    let 记着不补 = "库/记着不补.zip".to_string();
    let 戳 = Stamp {
        bytes: 1024,
        mtime_ns: Some(1_700_000_000_000_000_000),
    };
    desired.files.push(DesiredFile {
        path: 记着不补.clone(),
        kind: FileKind::Rom,
        bytes: 1024,
        unreadable: false,
        source: 记着不补.clone(),
        source_stamp: 戳,
        variant: 记着不补.clone(),
        convert: None,
    });
    manifest.files.push(ManifestFile {
        path: 记着不补.clone(),
        kind: FileKind::Rom,
        stamp: 戳,
        source: 记着不补.clone(),
        source_stamp: 戳,
        variant: 记着不补,
        absent: true,
    });
    let 排 = |restore_missing: bool| {
        sync::plan(
            &子库(dir.path(), None),
            &desired,
            &manifest,
            &actual,
            Options { restore_missing },
        )
    };
    let (不补, 补) = (排(false), 排(true));
    assert_eq!(不补.withheld, 1, "前提：有一份记着不补的");
    assert!(不补.restorable >= 2, "前提：这一趟没了的与记着不补的都能补");
    assert_eq!(
        不补.adds_if_restored(),
        补.adds_if_restored(),
        "开没开补回，「补回之后新增几个」得是同一个数",
    );
    assert_eq!(
        补.adds_if_restored(),
        补.adds.files,
        "勾上之后它就是新增那一格"
    );
    assert_eq!(不补.adds_if_restored(), 不补.adds.files + 不补.restorable);
}

/// 内置名册里叫这个名字的那一份文件系统声明。
fn 内置的文件系统(name: &str) -> Filesystem {
    romcat_core::capability::Roster::builtin()
        .filesystems()
        .iter()
        .find(|filesystem| filesystem.name == name)
        .cloned()
        .unwrap_or_else(|| panic!("内置名册里有 {name}"))
}

#[test]
fn 命令行的差量预览印得出被修改过的哪一样变了与文件名里不收的是哪几个字() {
    // 票 `gui-draws-the-rest-of-the-design/15`（差距 D-21、C-2）：那两个半句由核心一处拼（`Surprise::change`、
    // `BadName::shown`），界面那一行与命令行印同一份。
    let dir = temp_dir("sync-report-change");
    let (desired, manifest, actual) = 摆好现场(&全部组合());
    let plan = sync::plan(
        &子库(dir.path(), None),
        &desired,
        &manifest,
        &actual,
        Options::default(),
    );
    let 那一份 = plan
        .surprises
        .iter()
        .find(|one| one.kind == SurpriseKind::Changed)
        .expect("前提：有一份被修改过的");
    let 变了 = 那一份.change().expect("被修改过的说得出哪一样变了");
    assert_eq!(变了, "大小 1.00 KiB → 4.00 KiB");
    let 印的 = plan.render_text();
    assert!(
        印的.contains(&format!("{} · {变了}", 那一份.path)),
        "命令行没印那半句：\n{印的}"
    );

    let exfat = 内置的文件系统("exFAT");
    let mut desired = Desired {
        files: vec![DesiredFile {
            path: "PSP/最终幻想 纷争012: 前传.iso".to_string(),
            kind: FileKind::Rom,
            bytes: 1024,
            unreadable: false,
            source: "库/PSP/最终幻想 纷争012: 前传.iso".to_string(),
            source_stamp: Stamp {
                bytes: 1024,
                mtime_ns: None,
            },
            variant: "库/PSP/最终幻想 纷争012: 前传.iso".to_string(),
            convert: None,
        }],
        ..Desired::default()
    };
    desired.screen(&exfat, 0);
    let plan = sync::plan(
        &子库(dir.path(), None),
        &desired,
        &Manifest::default(),
        &TargetState::default(),
        Options::default(),
    );
    assert_eq!(plan.filesystem, exfat, "计划记着拦下它们的是哪一份文件系统");
    let 印的 = plan.render_text();
    assert!(
        印的.contains("PSP/最终幻想 纷争012: 前传.iso · 含有「:」"),
        "命令行没印不收的是哪几个字：\n{印的}"
    );
    let 劝告 = RejectReason::BadName.advice(&exfat).expect("说得出怎么办");
    assert!(印的.contains(&劝告), "命令行没印那句劝告：\n{印的}");
    assert!(!印的.contains("ADR-0004"), "劝告里还带着编号：\n{印的}");
}

// ───────────────────────── 二、期望状态真的从中立库折出来

/// 一份 fixture 主库：一个单文件变体，一个目录树变体（成员十几个、还带目录本身）。
fn 建库() -> TempDir {
    let dir = temp_dir("sync-lib");
    let root = dir.path();
    写(&root.join("FC/魂斗罗.zip"), &zip(2048));
    写(&root.join("FC/超级玛丽.zip"), &zip(4096));
    let dump = root.join("PSV/PSVENJP/零之轨迹[PCSG00042][日版]");
    写(&dump.join("app/PCSG00042/eboot.bin"), &[1u8; 64]);
    写(&dump.join("app/PCSG00042/sce_sys/param.sfo"), &[2u8; 32]);
    写(&dump.join("app/PCSG00042/data.psarc"), &[3u8; 128]);
    dir
}

fn 扫成库(root: &Path) -> Catalog {
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::named(root, "库");
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");
    catalog
}

fn 选中(catalog: &Catalog, 规则: &str) -> sublibrary::Selected {
    let selection = Selection {
        rules: vec![Rule::parse(规则).expect("规则读得懂")],
        exceptions: Vec::new(),
    };
    let facts = sublibrary::facts(catalog).expect("折得出事实");
    sublibrary::select(&selection, &facts)
}

#[test]
fn 期望状态是选中变体的文件成员_容量与变体那一层对得上() {
    let dir = 建库();
    let catalog = 扫成库(dir.path());
    let selected = 选中(&catalog, "平台=FC,PSV");
    let desired =
        sync::desired(&catalog, &selected, &Profile::unclaimed()).expect("折得出期望状态");

    // 一个变体可以是好几个文件，而容量的账两层必须一致。
    assert!(
        desired.files.len() > selected.picked.len(),
        "目录树变体摊成了好几个文件：{} 个文件 / {} 个变体",
        desired.files.len(),
        selected.picked.len()
    );
    assert_eq!(
        desired.bytes(),
        selected.bytes,
        "期望总量与选择集报出的容量是同一个数"
    );
    // 目录本身不是要搬的东西，但要数得出来——一个成员都不该凭空消失。
    assert!(desired.non_files > 0, "目录树变体的目录成员被数出来了");
    assert!(desired.empty_variants.is_empty());
    // 路径一律相对子库根：与主库里的键同一个写法，只是**不带根名**——
    // 前端认平台靠的是顶层那一级目录（ADR-0013、ADR-0015、ADR-0020）。
    assert!(
        desired
            .files
            .iter()
            .all(|file| !file.path.starts_with('/') && !file.path.contains('\\')),
        "{:?}",
        desired.files.first()
    );
    assert!(
        desired.files.iter().any(
            |file| file.path == "PSV/PSVENJP/零之轨迹[PCSG00042][日版]/app/PCSG00042/eboot.bin"
        ),
        "目录树里的成员一个个都在"
    );
}

// ───────────────────────── 三、整条链路

#[test]
fn 头一次同步是全新增_一条删除也长不出来() {
    let dir = 建库();
    let 目标 = temp_dir("sync-target-empty");
    let catalog = 扫成库(dir.path());
    let selected = 选中(&catalog, "平台=FC");
    let desired =
        sync::desired(&catalog, &selected, &Profile::unclaimed()).expect("折得出期望状态");
    let actual = sync::observe(&RealFs::new(), 目标.path()).expect("目标在位");
    let plan = sync::plan(
        &子库(目标.path(), None),
        &desired,
        // 没同步过的子库读出来就是一份空清单。
        &Manifest::empty(),
        &actual,
        Options::default(),
    );
    assert_eq!(plan.adds.files, 2);
    assert_eq!(plan.adds.variants, 2);
    assert_eq!(plan.deletes.files, 0, "清单是空的，删除项无从长出");
    assert_eq!(plan.net_bytes as u64, plan.desired_bytes);
    assert!(plan.render_text().contains("差量预览"));
}

#[test]
fn 手动拷进目标的存档在整条链路上绝对安全() {
    let dir = 建库();
    let 目标 = temp_dir("sync-target-saves");
    写(&目标.path().join("saves/魂斗罗.sav"), &[9u8; 512]);
    写(&目标.path().join("cheats/金手指.txt"), b"unlimited lives");

    let mut catalog = 扫成库(dir.path());
    catalog
        .put_sublibrary(&子库(目标.path(), None))
        .expect("子库写得进");
    // 上次同步放过一个 FC 的游戏；这次规则只要 PSV，于是它该被删。
    let 上次 = Manifest {
        files: vec![ManifestFile {
            path: "库/FC/魂斗罗.zip".to_string(),
            kind: FileKind::Rom,
            stamp: Stamp {
                bytes: 2048,
                mtime_ns: Some(7),
            },
            source: "库/FC/魂斗罗.zip".to_string(),
            source_stamp: Stamp {
                bytes: 2048,
                mtime_ns: Some(7),
            },
            variant: "库/FC/魂斗罗.zip".to_string(),
            absent: false,
        }],
    };
    catalog.put_manifest("掌机", &上次).expect("清单写得进");
    assert_eq!(
        catalog.manifest("掌机").expect("读得回"),
        上次,
        "清单存得住"
    );

    let selected = 选中(&catalog, "平台=PSV");
    let desired =
        sync::desired(&catalog, &selected, &Profile::unclaimed()).expect("折得出期望状态");
    let actual = sync::observe(&RealFs::new(), 目标.path()).expect("目标在位");
    let plan = sync::plan(
        &子库(目标.path(), None),
        &desired,
        &catalog.manifest("掌机").expect("读得回"),
        &actual,
        Options::default(),
    );

    assert_eq!(plan.strangers, 2, "存档与金手指都看见了");
    assert!(
        plan.steps
            .iter()
            .all(|step| !step.path.starts_with("saves/") && !step.path.starts_with("cheats/")),
        "{:?}",
        plan.steps
    );
    // 清单里那条实际上已经不在目标上了（上次导出的那份没真的放过去）：
    // **报告，而不是当作删掉了**。
    assert_eq!(plan.deletes.files, 0);
    assert!(
        plan.surprises.iter().any(|s| s.path == "库/FC/魂斗罗.zip"
            && s.kind == SurpriseKind::Gone
            && !s.still_wanted)
    );
    let text = plan.render_text();
    assert!(text.contains("清单之外"), "{text}");
    assert!(text.contains("工具连碰都不碰"), "{text}");
}

#[test]
fn 卡不在位时停住_而不是排出一份全删全传的计划() {
    let 目标 = temp_dir("sync-target-gone");
    let 不在了 = 目标.path().join("没插上");
    let error = sync::observe(&RealFs::new(), &不在了).expect_err("停住");
    assert!(format!("{error}").contains("目标未连接"), "{error}");
}

#[test]
fn 超出目标容量时给出超出量与裁剪建议_一个都不砍() {
    let dir = 建库();
    let 目标 = temp_dir("sync-target-full");
    let catalog = 扫成库(dir.path());
    let selected = 选中(&catalog, "平台=FC,PSV");
    let desired =
        sync::desired(&catalog, &selected, &Profile::unclaimed()).expect("折得出期望状态");
    let 只装得下一半 = desired.bytes() / 2;
    let plan = sync::plan(
        &子库(目标.path(), Some(只装得下一半)),
        &desired,
        &Manifest::empty(),
        &sync::observe(&RealFs::new(), 目标.path()).expect("目标在位"),
        Options::default(),
    );
    assert_eq!(
        plan.over_capacity,
        Some(desired.bytes() - 只装得下一半),
        "超出量报得出来"
    );
    assert_eq!(
        plan.adds.files,
        desired.files.len() as u64,
        "**一个都不砍**：砍谁由用户定（ADR-0016）"
    );
    assert!(!plan.trim_suggestions.is_empty());
    let 按体积降序 = plan
        .trim_suggestions
        .windows(2)
        .all(|pair| pair[0].bytes >= pair[1].bytes);
    assert!(按体积降序, "{:?}", plan.trim_suggestions);
    let text = plan.render_text();
    assert!(text.contains("不会自动截断"), "{text}");
}

#[test]
fn 清单跟着子库一起没() {
    let 目标 = temp_dir("sync-manifest-drop");
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    catalog
        .put_sublibrary(&子库(目标.path(), None))
        .expect("子库写得进");
    catalog
        .put_manifest(
            "掌机",
            &Manifest {
                files: vec![ManifestFile {
                    path: "库/FC/一.zip".to_string(),
                    kind: FileKind::Rom,
                    stamp: Stamp {
                        bytes: 1,
                        mtime_ns: None,
                    },
                    source: "库/FC/一.zip".to_string(),
                    source_stamp: Stamp {
                        bytes: 1,
                        mtime_ns: None,
                    },
                    variant: "库/FC/一.zip".to_string(),
                    absent: false,
                }],
            },
        )
        .expect("清单写得进");
    assert!(catalog.remove_sublibrary("掌机").expect("删得掉"));
    assert!(catalog.manifest("掌机").expect("读得回").files.is_empty());
}

// ───────────────────────── 四、`--library-root` 只换得动库里**已有**的根

fn 两个根的库() -> Catalog {
    let catalog = Catalog::open_in_memory().expect("能开中立库");
    roots::add_root(&catalog, None, "主库", Path::new("/盘甲/Game")).expect("加得上");
    roots::add_root(&catalog, None, "元数据库", Path::new("/盘乙/Pegasus")).expect("加得上");
    catalog
}

#[test]
fn 覆盖一个不存在的根名要报错并列出库里有哪些根() {
    // 用户那一幕：盘换了位置，拿 `--library-root` 指过去，根名打错一个字（`主庫`）。
    // 从前它被静默收下，一组根里凭空多出第三个，而后面报出来的是
    // 「主库这几个根不在位：主库」——说的是另一件事，照着它去插盘一辈子查不出打错了字。
    let catalog = 两个根的库();
    let 打错一个字 = vec![(Some("主庫".to_string()), PathBuf::from("/盘甲搬走了/Game"))];
    let 话 = sync::prepare::library_roots(&catalog, &打错一个字).expect_err("该被拒");
    assert!(话.contains("主庫"), "得说出点错的是哪个名字：{话}");
    assert!(
        话.contains("主库") && 话.contains("元数据库"),
        "库里有哪些根要列出来：{话}"
    );
    assert!(
        话.contains("`scan`"),
        "加一个根是 scan 的活，得说出来：{话}"
    );
}

#[test]
fn 带等号的相对路径不会被切成一个不存在的根() {
    // `roms=2024` 是一条相对路径，左半 `roms` 语法上像个根名，从前就被切成
    // 根名 `roms` + 路径 `2024`，凭空多出一个根。这一刀切得对不对要看库里有没有
    // 这个根，判据因此在核心里而不在命令行的解析里。
    let catalog = 两个根的库();
    let 相对路径 = vec![(Some("roms".to_string()), PathBuf::from("2024"))];
    let 话 = sync::prepare::library_roots(&catalog, &相对路径).expect_err("该被拒");
    assert!(话.contains("roms"), "{话}");
    assert!(话.contains("主库") && 话.contains("元数据库"), "{话}");
}

#[test]
fn 多于一个根时不点名照旧要求点名() {
    let catalog = 两个根的库();
    let 不点名 = vec![(None, PathBuf::from("/盘甲搬走了/Game"))];
    let 话 = sync::prepare::library_roots(&catalog, &不点名).expect_err("该被拒");
    assert!(话.contains("2 个根"), "{话}");
    assert!(
        话.contains("--library-root 根名=路径"),
        "怎么写才对要说出来：{话}"
    );
}

#[test]
fn 只有一个根时不点名照旧换得动位置() {
    // **这条便利不能丢。** 真库上大多数人只有一个根，那时「哪个根」没有歧义。
    let catalog = Catalog::open_in_memory().expect("能开中立库");
    roots::add_root(&catalog, None, "主库", Path::new("/盘甲/Game")).expect("加得上");
    let 不点名 = vec![(None, PathBuf::from("/盘甲搬走了/Game"))];
    let roots = sync::prepare::library_roots(&catalog, &不点名).expect("换得动");
    assert_eq!(roots.len(), 1);
    assert_eq!(
        roots.path_of("主库"),
        Some(Path::new("/盘甲搬走了/Game")),
        "换的是那个独苗，名字不变"
    );
    // 点名换的是同一个根，不是再多出一个。
    let 点名 = vec![(Some("主库".to_string()), PathBuf::from("/又搬了"))];
    let roots = sync::prepare::library_roots(&catalog, &点名).expect("换得动");
    assert_eq!(roots.len(), 1);
    assert_eq!(roots.path_of("主库"), Some(Path::new("/又搬了")));
}

/// 目标落在主库里那道 ADR-0004 的红线：Windows 上 `normalize_existing` 把目标化成
/// `\\?\D:\…`，而库里的根存的是 display 形态 `D:\…`，两种写法按 `Path` 的分量比
/// 恒不相等——**红线从此形同虚设**，同步会往那块 10 TiB 不可再生的盘上写文件、删文件。
/// 判据折成可比形态（`path::is_inside_place`）之后才拦得住。
///
/// **本机是 macOS，这条没跑过**：Unix 上 `D:\Game\子库` 不是绝对路径，
/// `normalize_existing` 会把它接到工作目录后面去，这一幕在 macOS 上摆不出来。
#[cfg(windows)]
#[test]
fn 扩展长度形式的目标落在主库里照样拦得住() {
    let catalog = Catalog::open_in_memory().expect("能开中立库");
    roots::add_root(&catalog, None, "主库", Path::new(r"D:\Game")).expect("加得上");
    let 话 = sync::prepare::refuse_target_in_library(&catalog, &[], Path::new(r"D:\Game\子库"))
        .expect_err("该被拒");
    assert!(话.contains("主库"), "哪个根拦下的要说出来：{话}");
    assert!(话.contains("主库只读"), "红线要说出来：{话}");
}

#[test]
fn 两个根里同一条相对路径落在卡上同一个文件上_排计划时就报出来() {
    // 挂单 Q57：子库里的落点**剥掉根名**（不剥的话卡上多出一层，而前端认平台靠的正是
    // 顶层那一级目录，ADR-0013），于是 `甲/FC/魂斗罗.zip` 与 `乙/FC/魂斗罗.zip`
    // 都想落在卡上同一个 `FC/魂斗罗.zip` 上。
    //
    // **走的是既有那道闸**（`Desired::screen` 的 `RejectReason::Collision`，票 21 为
    // 「转换之后两份撞到一起」立的）：判据是**落点**，不是撞车的原因，所以一组根这条
    // 新路不必另加一份检查。这条测试钉的是「它真的咬得到这条新路」。
    let 甲 = temp_dir("sync-root-a");
    let 乙 = temp_dir("sync-root-b");
    写(&甲.path().join("FC/魂斗罗.zip"), &zip(2048));
    写(&乙.path().join("FC/魂斗罗.zip"), &zip(4096));
    写(&乙.path().join("FC/只有乙有.zip"), &zip(1024));

    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    for (name, dir) in [("甲", &甲), ("乙", &乙)] {
        let mut options = ScanOptions::named(dir.path(), name);
        options.jobs = Jobs::Fixed(2);
        scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");
    }
    let selected = 选中(&catalog, "平台=FC");
    assert_eq!(selected.picked.len(), 3, "两个根上一共三个变体");
    let mut desired =
        sync::desired(&catalog, &selected, &Profile::unclaimed()).expect("折得出期望状态");
    assert_eq!(desired.files.len(), 3, "折出来时三条都还在");
    desired.screen(&Filesystem::unlimited(), 0);

    // **撞上的一个都不放行**：留一个放行等于随排序决定谁赢，而下一趟排序变了赢家就
    // 换人，卡上那份会莫名其妙地改内容。
    let 撞上的: Vec<&Rejected> = desired
        .rejected
        .iter()
        .filter(|row| row.reason == RejectReason::Collision)
        .collect();
    assert_eq!(撞上的.len(), 2, "两条都该被挡下：{:?}", desired.rejected);
    assert!(撞上的.iter().all(|row| row.path == "FC/魂斗罗.zip"));
    // **那句话里印的是完整的键**（带根名）：不带的话两行长得一模一样，
    // 人看不出撞的是哪两块盘（挂单 Q57）。
    assert!(
        撞上的
            .iter()
            .any(|row| row.detail.contains("甲/FC/魂斗罗.zip"))
            && 撞上的
                .iter()
                .any(|row| row.detail.contains("乙/FC/魂斗罗.zip")),
        "{:?}",
        撞上的,
    );
    // 只有一块盘有的那个照旧放行——撞车判的是落点，不是「这个名字出现过两次」。
    assert_eq!(desired.files.len(), 1);
    assert_eq!(desired.files[0].path, "FC/只有乙有.zip");
}

/// 期望状态里的一份：从 `根` 那块盘上的 `相对路径` 来，落在卡上同一条相对路径上。
fn 一份(根: &str, 相对路径: &str, bytes: u64) -> DesiredFile {
    let key = format!("{根}/{相对路径}");
    DesiredFile {
        path: 相对路径.to_string(),
        kind: FileKind::Rom,
        bytes,
        unreadable: false,
        source: key.clone(),
        source_stamp: Stamp {
            bytes,
            mtime_ns: Some(1_700_000_000_000_000_000),
        },
        variant: key,
        convert: None,
    }
}

#[test]
fn 三份撞到同一条落点_放不进目标的容量只算一份() {
    // 票 `verdict-store-and-sync/13`（挂单 `Q1029`）：撞在一起的几份**最终一份都不落**，
    // 可它们要的是卡上**同一条路径**——解开撞车之后那条路径上也只躺得下一份。各算一遍的话，
    // 「放不进目标」那一格的容量就把同一个落点数了三遍。
    //
    // 一处里取**最大的那一份**：排除哪几份由人定，这个数要答的是「这条落点最多要多大地方」。
    let dir = temp_dir("sync-collide-once");
    let mut desired = Desired {
        files: vec![
            一份("甲", "FC/魂斗罗.zip", 1_000),
            一份("乙", "FC/魂斗罗.zip", 3_000),
            一份("丙", "FC/魂斗罗.zip", 2_000),
            一份("甲", "GB/俄罗斯方块.zip", 500),
            一份("乙", "GB/俄罗斯方块.zip", 500),
            一份("甲", "FC/沙罗曼蛇.zip", 700),
        ],
        ..Desired::default()
    };
    desired.screen(&Filesystem::unlimited(), 0);
    let plan = sync::plan(
        &子库(dir.path(), None),
        &desired,
        &Manifest::default(),
        &TargetState::default(),
        Options::default(),
    );

    let 放不进 = plan.rejected_tally();
    // **份数照旧一份一份数**：那一格的大数字数的是文件（词表**差量预览**），撞上的五份
    // 这一趟一份都传不上去。
    assert_eq!(放不进.files, 5, "{:?}", plan.rejected);
    assert_eq!(放不进.variants, 5);
    // **容量一条落点只算一次**：魂斗罗那一处取最大的 3000，俄罗斯方块那一处 500。
    // 从前是五份各算一遍（7000）。
    assert_eq!(
        放不进.bytes,
        3_000 + 500,
        "撞车的几份各算了一遍：{:?}",
        plan.rejected
    );
    // 没撞的那一份照旧要传，不进这一笔账。
    assert!(plan.steps.iter().any(|step| step.path == "FC/沙罗曼蛇.zip"));
}

// ───────────────────────── 四、装得下吗：计划器那个数就是子库报告那个数（挂账 D76）

/// 一棵目录树底下全部文件一共多少字节——**从盘上量**，不经过计划器。
fn 盘上一共多大(dir: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        for entry in fs::read_dir(&at).expect("列得开").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                total += entry.metadata().expect("读得到元数据").len();
            }
        }
    }
    total
}

fn 排计划(catalog: &Catalog, 工作区: &Path) -> sync::Prepared {
    sync::prepare(
        catalog,
        工作区,
        "掌机",
        &sync::Request::default(),
        &Handle::new(),
    )
    .expect("排得出计划")
}

/// 子库报告那一份：界面上「算一遍容量」与 `romcat sublibrary show` 走的那一趟。
fn 算容量(catalog: &Catalog, 工作区: &Path) -> sublibrary::report::SelectionReport {
    let list = catalog.sublibraries().expect("读得出子库");
    sublibrary::survey(catalog, 工作区, &list, &Handle::new())
        .expect("算得出来")
        .remove("掌机")
        .expect("一台设备一份报告")
}

#[test]
fn 装得下吗_计划器与子库报告是同一个数_说装得下就真装得下_装不下报得出差多少() {
    let dir = 建库();
    let 工作区 = temp_dir("sync-fit-ws");
    let 目标 = temp_dir("sync-fit-card");
    // 维护者自己拷进卡里的存档：清单之外，工具不碰——但它占着卡上的地方。
    写(&目标.path().join("saves/魂斗罗.sav"), &[9u8; 512]);
    let mut catalog = 扫成库(dir.path());
    catalog
        .put_sublibrary(&子库(目标.path(), None))
        .expect("子库写得进");
    catalog
        .add_rule("掌机", &Rule::parse("平台=FC").expect("规则读得懂"), None)
        .expect("规则写得进");
    let 同步之后 = 排计划(&catalog, 工作区.path()).plan.after_bytes;
    assert!(
        同步之后 > 512,
        "存档之外还该有两个归档与前端元数据：{同步之后}"
    );

    // ── 上限比同步之后少 1000 字节：两边都说装不下，差的正是那 1000。
    catalog
        .put_sublibrary(&子库(目标.path(), Some(同步之后 - 1000)))
        .expect("改得了上限");
    let plan = 排计划(&catalog, 工作区.path()).plan;
    assert_eq!(plan.over_capacity, Some(1000), "计划器报的超出量");
    let report = 算容量(&catalog, 工作区.path());
    let room = report.fit.known().expect("卡在手边，报告该算得出");
    assert_eq!(
        (room.after_bytes, room.over_capacity),
        (plan.after_bytes, Some(1000)),
        "子库报告与计划器不是同一个数",
    );
    let text = report.render_text();
    assert!(
        text.contains(&format!("超出 {}", romcat_core::report::human_bytes(1000))),
        "{text}"
    );

    // ── 上限正好等于同步之后：两边都说装得下。
    catalog
        .put_sublibrary(&子库(目标.path(), Some(同步之后)))
        .expect("改得了上限");
    let prepared = 排计划(&catalog, 工作区.path());
    assert_eq!(prepared.plan.over_capacity, None, "计划器说装得下");
    let report = 算容量(&catalog, 工作区.path());
    let room = report.fit.known().expect("卡在手边，报告该算得出");
    assert_eq!((room.after_bytes, room.over_capacity), (同步之后, None));
    assert!(report.render_text().contains("装得下："));

    // ── 真同步一趟：卡上量出来的总量就是说好的那个数，一个字节都没超出上限。
    let roots = Roots::single("库", dir.path());
    let sources = sync::Sources {
        library: &RealFs,
        target: &RealFs,
        library_roots: Some(&roots),
        target_root: &prepared.root,
        from_pool: &prepared.from_pool,
        generated: &prepared.generated,
        link_probe_dir: Some(&prepared.scratch),
        convert_cache: None,
    };
    let outcome = sync::execute::run(
        &prepared.plan,
        &prepared.desired,
        &prepared.actual,
        &prepared.manifest,
        &sources,
        &Handle::new(),
    )
    .expect("传得动");
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
    assert_eq!(
        盘上一共多大(目标.path()),
        同步之后,
        "说好的「同步之后」与卡上真实占用对不上——它说装得下，卡却可能装不下",
    );
}

// ───────────────────────── 脚印：读库那一半与按档案折那一半分开（票 `gui-looks-like-the-design/21`）
//
// 目标设置弹层里换一份能力档案、改一行按平台覆盖，要当场说出「FAT32 放不下哪几份」。读库（折事实、读成员、
// 读内部构成）一趟是全库量级，得跑在任务台上；换档案之后那一步是纯的，当场重算。两半合起来必须与
// `sync::desired` 一口气折出来的一模一样——判断只有一处（ADR-0024）。

#[test]
fn 脚印读一次库_换一份档案折期望状态不再碰库_与一口气折出来的一样() {
    let dir = 建库();
    let catalog = 扫成库(dir.path());
    let selected = 选中(&catalog, "平台=FC,PSV");
    let 脚印 = sync::Footprint::gather(&catalog, &selected).expect("读得动");
    for profile in romcat_core::capability::Roster::builtin().profiles() {
        assert_eq!(
            脚印.desired(profile),
            sync::desired(&catalog, &selected, profile).expect("折得出期望状态"),
            "档案「{}」下两条路折出来的不一样",
            profile.name
        );
    }
    assert_eq!(脚印.platforms(), ["FC", "PSV"]);
}

#[test]
fn 脚印说得出这份档案的文件系统放不下哪几份_只看单文件上限() {
    let dir = 建库();
    let catalog = 扫成库(dir.path());
    let selected = 选中(&catalog, "平台=FC");
    let 脚印 = sync::Footprint::gather(&catalog, &selected).expect("读得动");
    let mut 小卡 = Profile::unclaimed();
    小卡.filesystem.name = "小卡".to_string();
    小卡.filesystem.max_file_bytes = Some(3000);
    let 放不下 = 脚印.too_big(&小卡);
    assert_eq!(
        放不下
            .iter()
            .map(|row| row.path.as_str())
            .collect::<Vec<_>>(),
        ["FC/超级玛丽.zip"],
        "4096 字节那一份超过 3000 的上限，2048 那一份放得下"
    );
    assert!(放不下.iter().all(|row| row.reason == RejectReason::TooBig));
    assert!(
        脚印.too_big(&Profile::unclaimed()).is_empty(),
        "不设单文件上限时一份都不拦"
    );
}

#[test]
fn 按名字读一台设备的脚印_读选择集折事实求值读成员() {
    let dir = 建库();
    let mut catalog = 扫成库(dir.path());
    let 卡 = temp_dir("sync-footprint-card");
    catalog
        .put_sublibrary(&子库(卡.path(), None))
        .expect("子库写得进");
    catalog
        .add_rule("掌机", &Rule::parse("平台=FC").expect("读得懂"), None)
        .expect("规则写得进");
    let 脚印 =
        sync::prepare::footprint(&catalog, "掌机", &Handle::new()).expect("读得出这一台的脚印");
    assert_eq!(脚印.platforms(), ["FC"]);
    assert_eq!(
        脚印.desired(&Profile::unclaimed()).files.len(),
        2,
        "FC 两个变体各一份文件"
    );
}

// ───────────────────────── 清单之外的文件只有一处数法（票 `gui-looks-like-the-design/21`）
//
// 目标设置弹层里那句「目录里已有 N 个文件，它们不在清单里」与差量预览里那个数必须是同一个数：
// 界面上各数一遍，迟早数出两个答案（ADR-0024）。

fn 卡上一份清单(path: &str, bytes: u64) -> Manifest {
    Manifest {
        files: vec![ManifestFile {
            path: path.to_string(),
            kind: FileKind::Rom,
            stamp: Stamp {
                bytes,
                mtime_ns: None,
            },
            source: format!("库/{path}"),
            source_stamp: Stamp {
                bytes,
                mtime_ns: None,
            },
            variant: format!("库/{path}"),
            absent: false,
        }],
    }
}

#[test]
fn 清单之外的文件只有一处数法_单独数的与计划里的一样() {
    let dir = 建库();
    let catalog = 扫成库(dir.path());
    let 卡 = temp_dir("sync-strangers-card");
    写(&卡.path().join("saves/我的.sav"), &[7u8; 300]);
    写(&卡.path().join("FC/魂斗罗.zip"), &zip(2048));
    let actual = sync::observe(&RealFs::new(), 卡.path()).expect("看得了目标");
    let 清单 = 卡上一份清单("FC/魂斗罗.zip", 2048);

    let 数的 = sync::strangers(&清单, &actual);
    assert_eq!((数的.count, 数的.bytes, 数的.unreadable), (1, 300, 0));

    let selected = 选中(&catalog, "平台=FC");
    let desired =
        sync::desired(&catalog, &selected, &Profile::unclaimed()).expect("折得出期望状态");
    let plan = sync::plan(
        &子库(卡.path(), None),
        &desired,
        &清单,
        &actual,
        Options {
            restore_missing: false,
        },
    );
    assert_eq!(
        (
            plan.strangers,
            plan.stranger_bytes,
            plan.stranger_unreadable
        ),
        (数的.count, 数的.bytes, 数的.unreadable),
        "计划里数的与单独数的不是一个数"
    );
}

#[test]
fn 数一台设备卡上清单之外的文件_路径可以是框里还没存的那一条() {
    let dir = 建库();
    let mut catalog = 扫成库(dir.path());
    let 卡 = temp_dir("sync-strangers-card");
    写(&卡.path().join("saves/我的.sav"), &[7u8; 300]);
    写(&卡.path().join("FC/魂斗罗.zip"), &zip(2048));
    catalog
        .put_sublibrary(&子库(卡.path(), None))
        .expect("子库写得进");
    catalog
        .put_manifest("掌机", &卡上一份清单("FC/魂斗罗.zip", 2048))
        .expect("清单写得进");

    let 数的 =
        sync::prepare::strangers_at(&catalog, "掌机", 卡.path(), &Handle::new()).expect("数得出来");
    assert_eq!((数的.count, 数的.bytes), (1, 300));
    let 新的 = sync::prepare::strangers_at(&catalog, "还没建的", 卡.path(), &Handle::new())
        .expect("数得出来");
    assert_eq!(新的.count, 2, "还没建的子库没有清单，卡上的都算清单之外");
}

// ───────────────────────── 落点预览照实际规则（票 `gui-looks-like-the-design/21`，拿主意的人 2026-09-15 定）

#[test]
fn 落点预览取头一个变体的真实落点_元数据位置照适配器_新建时示例名照同一条规则() {
    let dir = 建库();
    let catalog = 扫成库(dir.path());
    let selected = 选中(&catalog, "平台=FC");
    let 脚印 = sync::Footprint::gather(&catalog, &selected).expect("读得动");
    let pegasus = romcat_core::adapter::find("Pegasus").expect("带着 Pegasus");
    let es = romcat_core::adapter::find("ES-Gamelist").expect("带着 ES");

    let 落点 = 脚印
        .landing(&Profile::unclaimed(), pegasus.as_ref())
        .expect("选中了东西就有落点");
    assert!(
        落点.rom.starts_with("FC/") && 落点.rom.ends_with(".zip"),
        "落点剥掉根名、照主库里的平台目录：{落点:?}"
    );
    assert_eq!(落点.metadata, "FC.metadata.pegasus.txt");
    assert_eq!(
        脚印
            .landing(&Profile::unclaimed(), es.as_ref())
            .expect("有落点")
            .metadata,
        "gamelists/FC/gamelist.xml"
    );
    assert!(
        sync::Footprint::default()
            .landing(&Profile::unclaimed(), pegasus.as_ref())
            .is_none(),
        "什么都没选中时没有落点可说"
    );

    let 示例 = sync::Landing::example(es.as_ref(), "GBA", "火焰之纹章 烈火之剑.gba");
    assert_eq!(示例.rom, "GBA/火焰之纹章 烈火之剑.gba");
    assert_eq!(示例.metadata, "gamelists/GBA/gamelist.xml");
}

// ───────────────────────── 前端里的游玩记录与收藏不会被覆盖（票 `gui-looks-like-the-design/21`）
//
// 目标设置弹层里那句「前端里的游玩记录和收藏不会被覆盖」要有代码钉着才许照稿写（拿主意的人 2026-09-15 定）。前端那一侧的事实：
// - ES-DE 把 `favorite` / `playcount` / `playtime` / `lastplayed` 记在 gamelist.xml 里（`es-app/src/MetaData.cpp` 的
//   `gameDecls`），启动游戏时改 `playcount` 与 `lastplayed` 并存回去（`es-app/src/FileData.cpp` 的 `onMetaDataSavePoint`）。
// - Pegasus 的收藏在 `writableConfigDir()/favorites.txt`、游玩时长在 `writableConfigDir()/stats.db`
//   （`pegasus_favorites/Favorites.cpp`、`pegasus_playtime/PlaytimeStats.cpp`），一样都不在 metadata.pegasus.txt 里。
// 导出那一路（写回主库、带底本）由 `adapter::gamelist` 的「导出时用户状态逐条原样搬过去_省略等于清零」钉着。

#[test]
fn 前端在卡上改过的元数据文件同步不写回去_游玩记录与收藏还在() {
    // 那份 gamelist 是工具放上去的（清单记着），ES-DE 玩过之后往里写了 favorite / playcount / lastplayed：
    // 卡上那份与清单对不上 → 报告、本次不动（ADR-0015），这一趟一个字节都不写回去。
    let 卡 = temp_dir("sync-frontend-state-card");
    let 路径 = "gamelists/FC/gamelist.xml";
    let 工具放的 = b"<gameList><game><path>./a.zip</path><name>A</name></game></gameList>";
    let 前端改过的 = "<gameList><game><path>./a.zip</path><name>A</name>\
        <favorite>true</favorite><playcount>42</playcount><lastplayed>20240115T203000</lastplayed>\
        </game></gameList>";
    写(&卡.path().join(路径), 前端改过的.as_bytes());
    let 清单 = Manifest {
        files: vec![ManifestFile {
            path: 路径.to_string(),
            kind: FileKind::Metadata,
            stamp: Stamp {
                bytes: 工具放的.len() as u64,
                mtime_ns: None,
            },
            source: "gamelist.xml#0000000000000000".to_string(),
            source_stamp: Stamp {
                bytes: 工具放的.len() as u64,
                mtime_ns: None,
            },
            variant: sync::frontend::NOT_A_VARIANT.to_string(),
            absent: false,
        }],
    };
    let desired = Desired {
        files: vec![DesiredFile {
            path: 路径.to_string(),
            kind: FileKind::Metadata,
            bytes: 128,
            unreadable: false,
            source: "gamelist.xml#1111111111111111".to_string(),
            source_stamp: Stamp {
                bytes: 128,
                mtime_ns: None,
            },
            variant: sync::frontend::NOT_A_VARIANT.to_string(),
            convert: None,
        }],
        ..Desired::default()
    };
    let actual = sync::observe(&RealFs::new(), 卡.path()).expect("看得了目标");
    let plan = sync::plan(
        &子库(卡.path(), None),
        &desired,
        &清单,
        &actual,
        Options {
            restore_missing: false,
        },
    );
    assert!(
        plan.steps.iter().all(|step| step.path != 路径),
        "前端改过的那份 gamelist 被排进了要写的那几步：{:?}",
        plan.steps
    );
    assert!(
        plan.surprises.iter().any(|surprise| surprise.path == 路径),
        "改过的那份要报出来"
    );
}

#[test]
fn 同步不碰卡上pegasus的收藏与游玩时长文件() {
    // Pegasus 的收藏与游玩时长不在它的元数据文件里；卡上若躺着那两份，它们是清单之外的文件，一步都不进计划。
    let 卡 = temp_dir("sync-pegasus-state-card");
    写(
        &卡.path().join("pegasus-frontend/favorites.txt"),
        b"GBA/a.zip\n",
    );
    写(
        &卡.path().join("pegasus-frontend/stats.db"),
        b"SQLite format 3\0",
    );
    let actual = sync::observe(&RealFs::new(), 卡.path()).expect("看得了目标");
    let desired = Desired {
        files: vec![DesiredFile {
            path: "GBA.metadata.pegasus.txt".to_string(),
            kind: FileKind::Metadata,
            bytes: 64,
            unreadable: false,
            source: "metadata.pegasus.txt#2222222222222222".to_string(),
            source_stamp: Stamp {
                bytes: 64,
                mtime_ns: None,
            },
            variant: sync::frontend::NOT_A_VARIANT.to_string(),
            convert: None,
        }],
        ..Desired::default()
    };
    let plan = sync::plan(
        &子库(卡.path(), None),
        &desired,
        &Manifest::default(),
        &actual,
        Options {
            restore_missing: false,
        },
    );
    assert!(
        plan.steps
            .iter()
            .all(|step| !step.path.ends_with("favorites.txt") && !step.path.ends_with("stats.db")),
        "Pegasus 的收藏或游玩时长文件进了计划：{:?}",
        plan.steps
    );
    assert_eq!(plan.strangers, 2, "那两份是清单之外的文件，只数一数");
}

// ───────────────────────── 五、收回清单（票 `verdict-store-and-sync/08`，挂单 `Q1022`）
//
// 词表**收回清单**：把目标上一份**被修改过**的文件记回清单，此后工具重新有权更新或删除它。它扩的是工具在
// 目标设备上的行为边界（ADR-0015），所以每一次都留一笔审计（谁、何时、哪几份），住沉淀库；不收回的那几份
// 每趟照旧报出来、工具照旧不碰。收回只改清单与审计——设备上与主库里的文件一个字节都不动。

/// 照排好的那份计划真同步一趟，把交回来的清单落回中立库——界面上「同步」那一趟收回来时做的就是这两步。
fn 同步一趟(catalog: &mut Catalog, prepared: &sync::Prepared, 库根: &Path) {
    let roots = Roots::single("库", 库根);
    let sources = sync::Sources {
        library: &RealFs,
        target: &RealFs,
        library_roots: Some(&roots),
        target_root: &prepared.root,
        from_pool: &prepared.from_pool,
        generated: &prepared.generated,
        link_probe_dir: Some(&prepared.scratch),
        convert_cache: None,
    };
    let outcome = sync::execute::run(
        &prepared.plan,
        &prepared.desired,
        &prepared.actual,
        &prepared.manifest,
        &sources,
        &Handle::new(),
    )
    .expect("传得动");
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
    catalog
        .put_manifest("掌机", &outcome.manifest)
        .expect("清单写得回");
}

/// 这份计划里**被修改过**的那几条，按路径。
fn 被修改过的(plan: &sync::Plan) -> Vec<&str> {
    plan.surprises
        .iter()
        .filter(|one| one.kind == SurpriseKind::Changed)
        .map(|one| one.path.as_str())
        .collect()
}

/// 此刻，UNIX 纪元起的秒。审计里「何时」那一格拿它夹着比。
fn 此刻() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("钟没倒着走")
        .as_secs();
    i64::try_from(secs).expect("装得下")
}

/// 卡上被人改过的那一份：同步上去之后，别的工具把它整份改写了。
const 改过的: &str = "FC/魂斗罗.zip";

/// 改写成了什么。
const 改成: &str = "别的工具改过它";

/// 往 `catalog` 里建「掌机」这一台（只要 FC），同步一趟到 `目标`，然后在卡上把 [`改过的`] 那一份改写掉。
fn 同步之后改掉一份(catalog: &mut Catalog, 库根: &Path, 工作区: &Path, 目标: &Path) {
    catalog
        .put_sublibrary(&子库(目标, None))
        .expect("子库写得进");
    catalog
        .add_rule("掌机", &Rule::parse("平台=FC").expect("规则读得懂"), None)
        .expect("规则写得进");
    let prepared = 排计划(catalog, 工作区);
    同步一趟(catalog, &prepared, 库根);
    写(&目标.join(改过的), 改成.as_bytes());
}

/// 摆一张同步过一趟、其中一份随后被人改过的卡：交回主库、工作目录、卡与中立库。
fn 同步过又被改过的卡() -> (TempDir, TempDir, TempDir, Catalog) {
    let dir = 建库();
    let 工作区 = temp_dir("sync-takeback-ws");
    let 目标 = temp_dir("sync-takeback-card");
    let mut catalog = 扫成库(dir.path());
    同步之后改掉一份(&mut catalog, dir.path(), 工作区.path(), 目标.path());
    (dir, 工作区, 目标, catalog)
}

#[test]
fn 被人改过的那一份收回清单之后_下一趟差量预览不再把它列进被修改过() {
    let (dir, 工作区, 目标, mut catalog) = 同步过又被改过的卡();
    let 卡上那份 = 目标.path().join(改过的);

    // ── 不收回：每趟照旧报出来，同步一个字节都不碰它（ADR-0015）。
    let prepared = 排计划(&catalog, 工作区.path());
    assert_eq!(被修改过的(&prepared.plan), vec![改过的]);
    同步一趟(&mut catalog, &prepared, dir.path());
    assert_eq!(
        fs::read(&卡上那份).expect("读得到"),
        改成.as_bytes(),
        "同步碰了被修改过的那一份",
    );
    let prepared = 排计划(&catalog, 工作区.path());
    assert_eq!(
        被修改过的(&prepared.plan),
        vec![改过的],
        "不收回的那一份下一趟照旧报出来",
    );

    // ── 收回清单：只改清单与审计。
    let mut site = Site::in_memory(catalog, Store::in_memory().expect("开得出沉淀库"), "主库");
    let 之前 = 此刻();
    let 收回 = site
        .take_back_into_manifest("掌机", &prepared.plan.surprises, "测试员")
        .expect("收得回")
        .expect("有一份可收");
    let 之后 = 此刻();
    assert_eq!(
        收回
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        vec![改过的],
    );
    assert_eq!(
        fs::read(&卡上那份).expect("读得到"),
        改成.as_bytes(),
        "收回清单动了设备上的文件",
    );

    // ── 下一趟差量预览：它不再是「被修改过」，而是设备上那一份——主库没变，就不动它。
    let prepared = 排计划(&site.catalog, 工作区.path());
    assert!(
        被修改过的(&prepared.plan).is_empty(),
        "收回之后还列在被修改过里：{:?}",
        prepared.plan.surprises,
    );
    assert!(
        prepared.plan.steps.iter().all(|step| step.path != 改过的),
        "收回之后主库那份没变，却排了一步动它：{:?}",
        prepared.plan.steps,
    );

    // ── 审计读得回：谁、何时、哪几份。按主库分开，别的主库读不到这一笔。
    let 账 = site.store.take_backs("主库").expect("读得回");
    assert_eq!(账.len(), 1, "{账:?}");
    assert_eq!(账[0].sublibrary, "掌机");
    assert_eq!(账[0].who, "测试员");
    assert!(
        (之前..=之后).contains(&账[0].at),
        "何时那一格不在收回那一刻：{} 不在 {之前}..={之后}",
        账[0].at,
    );
    assert_eq!(
        账[0]
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        vec![改过的],
    );
    assert_eq!(
        账[0].files[0].now.bytes,
        改成.len() as u64,
        "哪几份要说得出是设备上哪一份：记的是收回那一刻设备上那一份的戳",
    );
    assert!(账[0].files[0].still_wanted, "那一刻选择集还要它");
    assert_eq!(
        账[0].files, 收回.files,
        "读回来的与收回时交出来的不是同一份"
    );
    assert!(site.store.take_backs("别的库").expect("读得回").is_empty());
}

#[test]
fn 选择集已经不要的那一份收回之后_下一趟就删掉它_审计里记着那一刻不要了() {
    // 收回意味着工具此后**有权更新或删除**它——确认那一层说的正是这句。选择集已经不要了的那一份，不收回时
    // 只报不删（它已经不是工具放的那一份，ADR-0015）；收回之后它就是一条普通的删除。
    let (_dir, 工作区, _目标, mut catalog) = 同步过又被改过的卡();
    for stored in catalog.sublibrary_rules("掌机").expect("读得动") {
        catalog.remove_rule("掌机", stored.ordinal).expect("删得掉");
    }
    catalog
        .add_rule("掌机", &Rule::parse("平台=PSV").expect("规则读得懂"), None)
        .expect("规则写得进");
    let prepared = 排计划(&catalog, 工作区.path());
    assert_eq!(被修改过的(&prepared.plan), vec![改过的]);
    assert!(
        prepared.plan.steps.iter().all(|step| step.path != 改过的),
        "没收回之前就排了一步动它：{:?}",
        prepared.plan.steps,
    );

    let mut site = Site::in_memory(catalog, Store::in_memory().expect("开得出沉淀库"), "主库");
    let 收回 = site
        .take_back_into_manifest("掌机", &prepared.plan.surprises, "测试员")
        .expect("收得回")
        .expect("有一份可收");
    assert!(
        !收回.files[0].still_wanted,
        "那一刻选择集已经不要它了，交回来的却说还要",
    );
    assert!(
        !site.store.take_backs("主库").expect("读得回")[0].files[0].still_wanted,
        "审计里没记下那一刻选择集已经不要它了",
    );

    let prepared = 排计划(&site.catalog, 工作区.path());
    assert!(
        prepared
            .plan
            .steps
            .iter()
            .any(|step| step.act == Act::Delete && step.path == 改过的),
        "收回之后选择集不要它，下一趟却没删：{:?}",
        prepared.plan.steps,
    );
}

#[test]
fn 清单写不进去时审计里那一笔删回去_两份库都像没按过() {
    // 两份库不在一个事务里：先记审计、再改清单。清单写不进去，那一笔收回就没生效——审计里留着它就是一句假话。
    let dir = 建库();
    let 工作区 = temp_dir("sync-takeback-ro-ws");
    let 目标 = temp_dir("sync-takeback-ro-card");
    let 库文件 = 工作区.path().join("catalog").join("fixture.sqlite3");
    fs::create_dir_all(库文件.parent().expect("有上级目录")).expect("能建目录");
    let mut catalog = Catalog::create(&库文件, "fixture").expect("能建中立库");
    let mut options = ScanOptions::named(dir.path(), "库");
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");
    同步之后改掉一份(&mut catalog, dir.path(), 工作区.path(), 目标.path());
    let prepared = 排计划(&catalog, 工作区.path());
    let 清单原样 = catalog.manifest("掌机").expect("读得回");

    // 同一份库文件的**只读**连接：读得出清单，写不进去。
    let 写不动 = catalog.read_only().expect("分得出只读连接");
    let mut site = Site::in_memory(写不动, Store::in_memory().expect("开得出沉淀库"), "主库");
    let 结果 = site.take_back_into_manifest("掌机", &prepared.plan.surprises, "测试员");
    assert!(结果.is_err(), "清单写不进去却说收回了：{结果:?}");
    assert!(
        site.store.take_backs("主库").expect("读得回").is_empty(),
        "清单没写进去，审计里却留着那一笔",
    );
    assert_eq!(
        catalog.manifest("掌机").expect("读得回"),
        清单原样,
        "清单变了",
    );
}

#[test]
fn 收回之后清单变过的那一份不收_收的只是人看见的那一份() {
    // 人按下去时看着的是排预览那一刻的差量。之后清单又变过（另一趟同步把它重新放上去了），那条异常说的已经
    // 不是眼下清单里的那一份——照着它改清单，等于替人收回一份他没看见的东西。
    let (_dir, 工作区, _目标, mut catalog) = 同步过又被改过的卡();
    let prepared = 排计划(&catalog, 工作区.path());
    let mut 清单 = catalog.manifest("掌机").expect("读得回");
    for file in &mut 清单.files {
        if file.path == 改过的 {
            file.stamp.bytes += 1;
        }
    }
    catalog.put_manifest("掌机", &清单).expect("清单写得进");

    let mut site = Site::in_memory(catalog, Store::in_memory().expect("开得出沉淀库"), "主库");
    let 收回 = site
        .take_back_into_manifest("掌机", &prepared.plan.surprises, "测试员")
        .expect("读写得动");
    assert!(收回.is_none(), "清单变过的那一份被收了：{收回:?}");
    assert_eq!(
        site.catalog.manifest("掌机").expect("读得回"),
        清单,
        "一份都没收，清单却变了",
    );
    assert!(
        site.store.take_backs("主库").expect("读得回").is_empty(),
        "一份都没收，审计里却记了一笔",
    );
}
