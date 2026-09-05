//! 在真实磁盘上跑一遍扫描。
//!
//! 那块 10T 外置盘现在没挂载，也不该为了跑测试去挂——验收靠这个 fixture 主库：
//! 它把票里点名的每种情况都摆上一份真实文件，包括真实的头部字节。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use romcat_core::catalog::Catalog;
use romcat_core::task::Handle;
use romcat_core::classify::{Category, SuspectReason};
use romcat_core::fs::RealFs;
use romcat_core::header::ProbeClass;
use romcat_core::path::long_path;
use romcat_core::platform::Manifest;
use romcat_core::report::DuplicateDetails;
use romcat_core::scan::aggregate::Limits;
use romcat_core::scan::{self, CheckpointOptions, Jobs, ScanOptions, ScanOutcome};
use romcat_core::testing::sample::{chd, gba, iso, nes, zip};
use romcat_core::testing::{TempDir, temp_dir};

fn 写文件(path: &Path, bytes: &[u8]) {
    let path = long_path(path).into_owned();
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(&path, bytes).expect("能写文件");
}

/// 一份小而全的 fixture 主库：按平台分目录，含中文文件名、重复拷贝、
/// 半截下载、模拟器本体、说明文档、系统垃圾，以及一个平台目录之外的散文件。
fn 建_fixture_主库() -> TempDir {
    let dir = temp_dir("fixture-library");
    let root = dir.path();

    写文件(&root.join("FC/超级马里奥.zip"), &zip(4096));
    写文件(&root.join("FC/魂斗罗汉化版.nes"), &nes());
    写文件(&root.join("FC/Contra (USA).zip"), &zip(1024));
    写文件(&root.join("FC/说明.txt"), "这个目录是 FC".as_bytes());
    写文件(&root.join("FC/备份/超级马里奥.zip"), &zip(4096));
    写文件(&root.join("FC/.DS_Store"), &[0u8; 6]);

    写文件(&root.join("GBA/黄金太阳汉化版.gba"), &gba(b"AGSJ"));
    写文件(&root.join("GBA/坏掉的.gba"), &[0u8; 0x200]);

    写文件(&root.join("PS1/最终幻想7/最终幻想7.chd"), &chd());
    写文件(&root.join("PS1/生化危机.iso"), &iso());
    写文件(&root.join("PS1/模拟器/epsxe.exe"), &[0u8; 64]);
    写文件(&root.join("PS1/没下完.iso.part"), &[0u8; 8]);

    写文件(&root.join("PSP/游戏.7z.001"), &[0u8; 32]);
    写文件(&root.join("PSP/游戏.7z.002"), &[0u8; 32]);

    // 直接躺在库根下：没有平台目录，但照常入报告
    写文件(&root.join("散落的游戏.gba"), &gba(b"AGBJ"));
    写文件(&root.join("空文件.zip"), &[]);

    dir
}

fn 中立库() -> Catalog {
    Catalog::open_in_memory().expect("能开中立库")
}

fn 扫入(catalog: &mut Catalog, options: &ScanOptions) -> ScanOutcome {
    scan::scan(&RealFs::new(), catalog, options, &Handle::new()).expect("扫描不该失败")
}

fn 扫(root: &Path) -> ScanOutcome {
    let mut options = ScanOptions::named(root, "库");
    options.jobs = Jobs::Fixed(4);
    扫入(&mut 中立库(), &options)
}

/// 盘上的**基线**：路径、大小、修改时间、内容指纹。
///
/// 词表里**快照**是**清单**词条明列的 `_Avoid_` 词；这里记的正是「拿它作比」的那份
/// 三元组，所以叫**基线**。与 [`romcat_core::catalog::Baseline`] 是同一个概念的两处
/// 落点：那份来自中立库，这份直接量磁盘。
type 盘上基线 = BTreeMap<PathBuf, (u64, Option<SystemTime>, u64)>;

/// 量一遍整棵树，记下**基线**。
fn 记下基线(root: &Path) -> 盘上基线 {
    fn walk(dir: &Path, out: &mut 盘上基线) {
        for entry in fs::read_dir(dir).expect("能列目录") {
            let entry = entry.expect("能读目录项");
            let path = entry.path();
            let meta = entry.metadata().expect("能取元数据");
            if meta.is_dir() {
                out.insert(path.clone(), (0, meta.modified().ok(), 0));
                walk(&path, out);
            } else {
                let bytes = fs::read(&path).unwrap_or_default();
                let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
                for byte in &bytes {
                    hash ^= u64::from(*byte);
                    hash = hash.wrapping_mul(0x1000_0000_01b3);
                }
                out.insert(path.clone(), (meta.len(), meta.modified().ok(), hash));
            }
        }
    }
    let mut out = 盘上基线::new();
    walk(root, &mut out);
    out
}

#[test]
fn 按平台目录给出文件数与容量分布() {
    let library = 建_fixture_主库();
    let report = 扫(library.path()).report;

    assert_eq!(report.totals.files, 16);
    assert!(!report.interrupted);

    let 取 = |name: &str| {
        report
            .platforms
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("报告里有 {name}"))
    };
    assert_eq!(取("FC").files, 6);
    assert_eq!(取("GBA").files, 2);
    assert_eq!(取("PS1").files, 4);
    assert_eq!(取("PSP").files, 2);

    let unknown = report
        .platforms
        .iter()
        .find(|p| p.unknown)
        .expect("有平台未知这一组");
    assert_eq!(unknown.files, 2, "散落的游戏.gba 与 空文件.zip");

    let 合计: u64 = report.platforms.iter().map(|p| p.files).sum();
    assert_eq!(合计, report.totals.files, "每个文件都落在某一组里");
    assert_eq!(
        report.platforms.iter().map(|p| p.bytes).sum::<u64>(),
        report.totals.bytes
    );
}

#[test]
fn 扩展名构成区分三类() {
    let library = 建_fixture_主库();
    let report = 扫(library.path()).report;

    let 类 = |category: Category| {
        report
            .categories
            .iter()
            .find(|c| c.category == category)
            .expect("类别都在")
            .files
    };
    // zip×4（含空文件.zip）加两个分卷
    assert_eq!(类(Category::TransparentContainer), 6);
    assert_eq!(类(Category::CompressedImage), 1, "chd");
    assert_eq!(类(Category::BareFile), 5, "nes、gba×3、iso");
    // **入口段不算多出来的一段**：`.7z.001` 是那一组的入口，它自己就是那个容器；
    // 数它会把一组数两次（票 04）。
    assert_eq!(report.anomalies.split_volume_parts, 1, "只数 7z.002");

    let zip = report
        .extensions
        .iter()
        .find(|e| e.extension == "zip")
        .expect("扩展名表里有 zip");
    assert_eq!(zip.files, 4);
    assert_eq!(zip.category, Category::TransparentContainer);
}

#[test]
fn 疑似不该入库的内容被标出来() {
    let library = 建_fixture_主库();
    let report = 扫(library.path()).report;
    let 取 = |reason: SuspectReason| {
        report
            .suspects
            .by_reason
            .iter()
            .find(|s| s.reason == reason)
            .expect("理由都在")
    };

    assert_eq!(取(SuspectReason::DuplicateCopy).files, 2);
    assert_eq!(report.suspects.duplicate_groups, 1);
    assert_eq!(report.suspects.duplicate_reclaimable_bytes, 4096);
    assert_eq!(取(SuspectReason::Document).files, 1);
    assert_eq!(取(SuspectReason::EmulatorBinary).files, 1);
    assert_eq!(取(SuspectReason::PartialDownload).files, 1);
    assert_eq!(取(SuspectReason::SystemJunk).files, 1);
}

#[test]
fn 头部抽样报出各类的解析成功率() {
    let library = 建_fixture_主库();
    let report = 扫(library.path()).report;
    let 取 = |class: ProbeClass| {
        report
            .samples
            .iter()
            .find(|s| s.class == class)
            .unwrap_or_else(|| panic!("抽到了 {}", class.label()))
    };

    let zip = 取(ProbeClass::Zip);
    assert_eq!(zip.sampled, 4);
    assert_eq!(zip.parsed, 3, "空文件.zip 对不上");
    assert_eq!(zip.mismatched, 1);

    let gba = 取(ProbeClass::Gba);
    assert_eq!(gba.sampled, 3);
    assert_eq!(gba.parsed, 2, "坏掉的.gba 对不上");
    assert_eq!(gba.failures.len(), 1);
    assert!(gba.failures[0].0.contains("坏掉的.gba"));

    assert_eq!(取(ProbeClass::Nes).parsed, 1);
    assert_eq!(取(ProbeClass::Chd).parsed, 1);
    assert_eq!(取(ProbeClass::DiscImage).parsed, 1);
}

/// 报告只列前 10 组，可腾出的空间却算的是全部——要人工处理，得拿到完整明细，
/// 且组内每一份都要有完整路径。
#[test]
fn 完整重复明细列出每一组的每个文件且不动主库() {
    let library = temp_dir("duplicates");
    let root = library.path();
    // 12 组重复：第 n 组 n+1 份、每份 n KiB。报告只装得下 10 组。
    for n in 1..=12u64 {
        for copy in 0..=n {
            写文件(
                &root.join(format!("FC/备份{copy}/游戏{n}.zip")),
                &zip(usize::try_from(n).expect("组号不大") * 1024),
            );
        }
    }
    // 一组 25 份：超过默认每组 10 条路径的上限
    for copy in 0..25 {
        写文件(&root.join(format!("MD/备份{copy}/魂斗罗.zip")), &zip(4096));
    }

    let 之前 = 记下基线(root);

    let mut options = ScanOptions::named(root, "库");
    options.jobs = Jobs::Fixed(4);
    options.limits.max_duplicate_paths_per_group = Limits::FULL_DUPLICATE_PATHS_PER_GROUP;
    let outcome = 扫入(&mut 中立库(), &options);

    assert_eq!(
        outcome.report.suspects.top_duplicates.len(),
        10,
        "报告仍然只列前 10 组"
    );

    let details = DuplicateDetails::build(&outcome.aggregate, &outcome.report);
    assert_eq!(details.group_count(), 13);
    assert_eq!(details.groups_with_missing_paths, 0, "每一份都有路径");

    let text = details.render_text();
    for n in 1..=12u64 {
        for copy in 0..=n {
            let path = root.join(format!("FC/备份{copy}/游戏{n}.zip"));
            assert!(
                text.contains(&romcat_core::path::display(&path)),
                "清单里必须有 {}",
                path.display()
            );
        }
    }
    for copy in 0..25 {
        let path = root.join(format!("MD/备份{copy}/魂斗罗.zip"));
        assert!(
            text.contains(&romcat_core::path::display(&path)),
            "25 份都要有完整路径，缺了 {}",
            path.display()
        );
    }

    // 清单能落到主库之外的文件里，且导出这件事一个字节都不改主库
    let out = temp_dir("dump");
    let dump = out.path().join("重复拷贝.txt");
    fs::write(&dump, text.as_bytes()).expect("能写清单");
    assert!(
        fs::read_to_string(&dump)
            .expect("能读回")
            .contains("#1  可腾")
    );
    assert_eq!(之前, 记下基线(root), "导出清单不该改动主库");
}

#[test]
fn 遍历不改主库一个字节() {
    let library = 建_fixture_主库();
    let root = library.path();
    let workspace = temp_dir("workspace");

    let 之前 = 记下基线(root);

    let mut options = ScanOptions::named(root, "库");
    options.jobs = Jobs::Fixed(4);
    options.checkpoint = Some(CheckpointOptions {
        // 断点写在主库之外的工作目录里
        path: workspace.path().join("scans").join("checkpoint.json"),
        interval: std::time::Duration::ZERO,
        resume: false,
    });
    let mut catalog =
        Catalog::open(&workspace.path().join("catalog").join("库.sqlite3")).expect("能开中立库");
    let outcome = 扫入(&mut catalog, &options);
    assert!(outcome.report.totals.files > 0);

    let 之后 = 记下基线(root);
    assert_eq!(之前.len(), 之后.len(), "扫描不该增删任何条目");
    assert_eq!(之前, 之后, "扫描不该改动任何文件的大小、修改时间或内容");
}

#[test]
fn 断点绝不落在主库里() {
    let library = 建_fixture_主库();
    let mut options = ScanOptions::named(library.path(), "库");
    options.checkpoint = Some(CheckpointOptions {
        path: library.path().join(".romcat").join("checkpoint.json"),
        interval: std::time::Duration::ZERO,
        resume: false,
    });
    let err = scan::scan(&RealFs::new(), &mut 中立库(), &options, &Handle::new())
        .expect_err("必须拒绝");
    assert!(err.to_string().contains("主库只读"));
}

#[test]
fn 中立库绝不落在主库里() {
    // 主库只读（ADR-0004），而且外置盘不常挂载——中立库跟着盘走的话，
    // 盘不在时连浏览元数据都做不到（ADR-0009）。
    let library = 建_fixture_主库();
    let mut catalog =
        Catalog::open(&library.path().join(".romcat").join("库.sqlite3")).expect("能开中立库");
    let options = ScanOptions::named(library.path(), "库");
    let err = scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new())
        .expect_err("必须拒绝");
    assert!(err.to_string().contains("中立库"), "{err}");
    assert!(err.to_string().contains("主库只读"), "{err}");
}

#[test]
fn 超过_260_字符的路径照样扫得到() {
    let library = temp_dir("long-path");
    let root = library.path();

    // 四层 60 字符加一层汉字目录：远超 Windows 的 260 字符上限，
    // 又不至于撞上 macOS 的 1024 字节 PATH_MAX。
    let mut deep = root.join("PS2");
    for i in 0..4 {
        deep = deep.join(format!("{i}{}", "a".repeat(59)));
    }
    deep = deep.join("很深的目录".repeat(6));
    let file = deep.join("很深的游戏.iso");
    写文件(&file, &iso());
    assert!(
        romcat_core::path::exceeds_max_path(&romcat_core::path::display(&file)),
        "这条路径应当超过 260 字符，实际 {} 字符",
        romcat_core::path::char_len(&romcat_core::path::display(&file))
    );

    let report = 扫(root).report;
    assert_eq!(report.totals.files, 1, "最深的那个文件必须被扫到");
    assert_eq!(report.anomalies.over_max_path, 1);
    assert_eq!(report.anomalies.errors, 0);
    assert_eq!(
        report
            .platforms
            .iter()
            .find(|p| p.name == "PS2")
            .expect("有 PS2")
            .files,
        1
    );
}

#[test]
fn 中断后能从断点接着扫() {
    let library = 建_fixture_主库();
    let workspace = temp_dir("workspace");
    let checkpoint = workspace.path().join("scans").join("checkpoint.json");

    let mut options = ScanOptions::named(library.path(), "库");
    options.jobs = Jobs::Fixed(1);
    options.checkpoint = Some(CheckpointOptions {
        path: checkpoint.clone(),
        interval: std::time::Duration::ZERO,
        resume: true,
    });

    // 一开始就按下中断：什么都没扫，但断点必须留下来
    let task = Handle::new();
    task.stop();
    // 续跑要接着往同一个中立库里写，因此两趟共用一份
    let mut catalog = 中立库();
    let first =
        scan::scan(&RealFs::new(), &mut catalog, &options, &task).expect("中断也算正常返回");
    assert!(first.interrupted);
    assert!(checkpoint.exists(), "断点应当落在工作目录里");

    // 续跑，扫完，断点被清掉
    let second = scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new())
        .expect("续跑不该失败");
    assert!(second.report.resumed);
    assert!(!second.interrupted);
    assert_eq!(second.report.totals.files, 16);
    assert!(!checkpoint.exists());
}

#[test]
fn 并发数不影响结论() {
    let library = 建_fixture_主库();
    let mut single = ScanOptions::named(library.path(), "库");
    single.jobs = Jobs::Fixed(1);
    let mut many = ScanOptions::named(library.path(), "库");
    many.jobs = Jobs::Fixed(8);

    let a = 扫入(&mut 中立库(), &single);
    let b = 扫入(&mut 中立库(), &many);
    assert_eq!(a.report.totals, b.report.totals);
    assert_eq!(a.report.categories, b.report.categories);
    assert_eq!(a.report.extensions, b.report.extensions);
}

/// 增量的判据在真实文件系统上也得站得住：这里的修改时间是 APFS 真给的，
/// 不是 fixture 编出来的。
#[test]
fn 真实磁盘上第二次扫描跳过未变的文件() {
    let library = 建_fixture_主库();
    let root = library.path();
    let mut catalog = 中立库();
    let mut options = ScanOptions::named(root, "库");
    options.jobs = Jobs::Fixed(4);

    let 首扫 = 扫入(&mut catalog, &options);
    assert_eq!(首扫.delta.added, 16);
    assert_eq!(首扫.delta.unchanged, 0);

    let 再扫 = 扫入(&mut catalog, &options);
    assert_eq!(再扫.delta.unchanged, 16, "一个都没变");
    assert_eq!(再扫.delta.added, 0);
    assert_eq!(再扫.delta.changed, 0);
    assert_eq!(再扫.delta.removed, 0);
    assert_eq!(再扫.report.totals, 首扫.report.totals);

    // 大小一模一样，只有内容与修改时间变了
    let 改动 = root.join("FC/Contra (USA).zip");
    let 原大小 = fs::metadata(&改动).expect("能取元数据").len();
    写文件(&改动, &zip(1024));
    assert_eq!(
        fs::metadata(&改动).expect("能取元数据").len(),
        原大小,
        "这一步要保持大小不变，否则测不到 mtime 那一维"
    );
    写文件(&root.join("FC/新来的.zip"), &zip(64));
    fs::remove_file(root.join("PS1/没下完.iso.part")).expect("能删掉");

    let 三扫 = 扫入(&mut catalog, &options);
    assert_eq!(三扫.delta.changed, 1, "大小没变、修改时间变了，也是已变");
    assert_eq!(三扫.delta.added, 1);
    assert_eq!(三扫.delta.removed, 1);
    assert_eq!(三扫.delta.unchanged, 14);
    assert_eq!(三扫.report.totals.files, 16);
}

/// 中立库落在本机的工作目录里，因此重启工具、甚至外置盘不在位时，
/// 已有的结论照样读得出来（ADR-0009）。
#[test]
fn 中立库落在本机重启后仍读得出来() {
    let library = 建_fixture_主库();
    let workspace = temp_dir("workspace");
    let catalog_path = workspace.path().join("catalog").join("库.sqlite3");

    let 扫出来的 = {
        let mut catalog = Catalog::open(&catalog_path).expect("能开中立库");
        let mut options = ScanOptions::named(library.path(), "库");
        options.jobs = Jobs::Fixed(4);
        扫入(&mut catalog, &options).report
    };
    assert!(catalog_path.exists(), "中立库是本机上的一个文件");

    // 盘拔了，工具也重启了
    drop(library);
    let catalog = Catalog::open(&catalog_path).expect("能再打开");
    let aggregate = catalog
        .aggregate(&Limits::default(), &Manifest::builtin())
        .expect("读得出来");
    let report = romcat_core::report::HealthReport::build(
        &aggregate,
        &catalog.report_meta().expect("元信息读得出来"),
    );
    assert_eq!(report.totals, 扫出来的.totals);
    assert_eq!(report.platforms, 扫出来的.platforms);
    assert_eq!(report.suspects, 扫出来的.suspects);
    assert_eq!(report.delta, None, "没扫盘就没有增量可说");
}
