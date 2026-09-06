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
use romcat_core::task::Handle;
use romcat_core::catalog::{Catalog, roots};
use romcat_core::fs::RealFs;
use romcat_core::scan::{self, Jobs, ScanOptions};
use romcat_core::sublibrary::{self, Rule, Selection, Sublibrary};
use romcat_core::capability::{Filesystem, RejectReason};
use romcat_core::sync::{
    self, Act, Desired, DesiredFile, FileKind, Manifest, ManifestFile, Options, Rejected, Stamp,
    SurpriseKind, TargetFile, TargetState,
};
use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};

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
        plan.surprises
            .iter()
            .any(|s| s.path == "库/FC/魂斗罗.zip" && s.kind == SurpriseKind::Gone && !s.still_wanted)
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
    assert!(format!("{error}").contains("目标不在位"), "{error}");
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
    assert!(话.contains("`scan`"), "加一个根是 scan 的活，得说出来：{话}");
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
    assert!(话.contains("ADR-0004"), "红线的出处要说出来：{话}");
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
        撞上的.iter().any(|row| row.detail.contains("甲/FC/魂斗罗.zip"))
            && 撞上的.iter().any(|row| row.detail.contains("乙/FC/魂斗罗.zip")),
        "{:?}",
        撞上的,
    );
    // 只有一块盘有的那个照旧放行——撞车判的是落点，不是「这个名字出现过两次」。
    assert_eq!(desired.files.len(), 1);
    assert_eq!(desired.files[0].path, "FC/只有乙有.zip");
}
