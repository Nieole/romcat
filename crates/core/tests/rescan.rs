//! **重扫之后中立库该作废什么、该记什么**：三条缝各一条回归。
//!
//! 三条缝都长在「上一趟扫描留下的东西，这一趟还算不算数」这句话上，而它们各自在
//! 不同的一层上答错：
//!
//! - **同一条路径换了内容**：作废的是挂在**条目**上的那几张内容表，挂在**变体**上的
//!   识别结论一条没动——于是报告、标题集合与导出都在交出一条对不上任何字节的「命中」。
//! - **按根续扫**：成型对着的是**整份库**，续跑沿用的却是断点里那个旧代号，
//!   `shaped_scan` 因此倒退，`romcat report` 误报「成型比库旧」。
//! - **非 UTF-8 名字的容器**：键里那段指纹缀在扩展名之后，`Path::extension` 从此认不出
//!   它是容器，第二趟扫描撞 `container.key` 主键整趟失败。
//!
//! 三条各自能单独回退验证。

use std::fs;
use std::path::Path;
use std::time::Duration;

use romcat_core::catalog::identify::State;
use romcat_core::catalog::{Catalog, Roots};
use romcat_core::dat::Convention;
use romcat_core::dat::chinese::ChineseMark;
use romcat_core::dat::logiqx::{DatHeader, GameRecord, RomRecord};
use romcat_core::dat::repo::{DatMeta, DatRepo, Unit};
use romcat_core::fs::{MemFs, RealFs};
use romcat_core::identify::fuzzy;
use romcat_core::identify::{self, Options};
use romcat_core::platform::Manifest;
use romcat_core::report::HealthReport;
use romcat_core::scan::aggregate::Limits;
use romcat_core::scan::{self, CancelToken, CheckpointOptions, Jobs, ScanOptions};
use romcat_core::task::Handle;
use romcat_core::testing::container::{ZipEntrySpec, zip_container};
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::triage::{self, Decide, DecisionSpec, Filter, Overrides};
use romcat_core::verdict::{self, Anchor, Store};

const 库名: &str = "库";

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

/// 一份带 iNES 头的 FC 卡带。
fn 卡带(fill: u8, payload: usize) -> Vec<u8> {
    let mut data = vec![0u8; 16];
    data[..4].copy_from_slice(b"NES\x1A");
    data.extend(std::iter::repeat_n(fill, payload));
    data
}

fn 扫一遍(catalog: &mut Catalog, root: &Path) {
    let mut options = ScanOptions::named(root, 库名);
    options.jobs = Jobs::Fixed(1);
    scan::scan(&RealFs::new(), catalog, &options, &Handle::new()).expect("扫得动");
}

// ── 一、同路径换了内容重扫 ────────────────────────────────────────────────

struct 现场 {
    dir: TempDir,
    catalog: Catalog,
    repo: DatRepo,
    store: Store,
}

fn 空_dat() -> DatRepo {
    let mut repo = DatRepo::in_memory().expect("开得出来");
    let mut writer = repo
        .begin(&Unit {
            source: "TOSEC".to_string(),
            name: "fc.dat".to_string(),
            url: "https://example.invalid/x".to_string(),
            fingerprint: "sha".to_string(),
        })
        .expect("开得了事务");
    writer
        .write_dat(
            &DatMeta {
                name: "Nintendo Famicom - Games".to_string(),
                platform: "FC".to_string(),
                convention: Convention::AsIs,
                header: DatHeader::default(),
            },
            &[GameRecord {
                name: "Some Other Game (1990)(Someone)".to_string(),
                roms: vec![RomRecord {
                    name: "other.nes".to_string(),
                    size: Some(4),
                    crc32: Some(0x1234_5678),
                    ..RomRecord::default()
                }],
                ..GameRecord::default()
            }],
        )
        .expect("写得进");
    writer.commit().expect("提交");
    repo
}

fn 跑识别(现场: &mut 现场) -> identify::Outcome {
    let index = verdict::Index::load(&现场.store, 库名).expect("读得出沉淀库");
    identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &identify::Ammo {
            repo: &现场.repo,
            verdicts: &index,
            naming: &fuzzy::Naming::off(),
            guessing: &identify::model::Guessing::off(),
            titledb: None,
        },
        &Options::new(Roots::single(库名, 现场.dir.path())),
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别不该失败")
}

/// 把队列里选中的那批裁成 `work`，落沉淀库也落中立库。
fn 裁(现场: &mut 现场, work: &str) -> triage::Applied {
    let index = verdict::Index::load(&现场.store, 库名).expect("读得出沉淀库");
    let mut items = triage::survey(&现场.catalog, &index, &Filter::default())
        .expect("折得出队列")
        .items;
    triage::fill_prints(&现场.catalog, &mut items).expect("算得出判据");
    assert!(!items.is_empty(), "队列里得有东西可裁");
    let decide = Decide {
        spec: DecisionSpec::Manual(work.to_string()),
        overrides: Overrides {
            chinese: Some(ChineseMark::FanTranslated),
            ..Overrides::default()
        },
        note: None,
        library: 库名.to_string(),
    };
    let plan = triage::plan(&现场.store, &items, &decide).expect("排得出计划");
    triage::apply(&mut 现场.catalog, &mut 现场.store, &items, &plan).expect("落得下")
}

/// 这个变体眼下那份字节的**内容锚**。
fn 内容锚(catalog: &Catalog, key: &str) -> Anchor {
    let variant = catalog
        .variant(key)
        .expect("读得出")
        .unwrap_or_else(|| panic!("{key} 这个变体不在了"));
    let print = identify::content_print(catalog, &variant)
        .expect("算得出")
        .expect("zip 的 CRC 零解压就有");
    Anchor::Content {
        crc32: print.crc32,
        size: print.size,
        sha1: None,
    }
}

#[test]
fn 同路径换了内容重扫之后旧裁决投影出来的命中不再挂在变体上() {
    // ⭐ **字节换了，按字节得出的结论就不再是这份字节的结论。** 识别是按字节的
    // （`CONTEXT.md` 的**识别**），所以同一条路径上换一份内容，挂在那个变体上的
    // 结论、候选与作品 / 发行版链接全部作废——窗口是「重扫到下一趟识别之间」，
    // 那期间报告、标题集合与导出读的都是这几张表。
    //
    // **沉淀库里那条裁决一个字都不动**：它是人定的（ADR-0008），钉的是那串字节，
    // 与谁躺在哪条路径上无关。下一趟识别按锚重新投影，新字节撞不上它是应该的。
    let dir = temp_dir("rescan-swap");
    let 路径 = dir.path().join("FC/某汉化.zip");
    let 旧内容 = 卡带(0xB3, 40_960);
    写(
        &路径,
        &zip_container(&[ZipEntrySpec::stored("rom.nes", 旧内容)]),
    );
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    扫一遍(&mut catalog, dir.path());
    let mut 现场 = 现场 {
        catalog,
        repo: 空_dat(),
        store: Store::in_memory().expect("开得出沉淀库"),
        dir,
    };
    跑识别(&mut 现场);

    let 键 = "库/FC/某汉化.zip";
    assert_eq!(
        现场
            .catalog
            .identification_of(键)
            .expect("读得出")
            .map(|it| it.0),
        Some(State::Unmatched),
        "前提：DAT 里没有它"
    );
    let applied = 裁(&mut 现场, "作品X");
    assert_eq!(applied.content_anchored, 1, "裁决该钉在内容上");
    let 旧锚 = 内容锚(&现场.catalog, 键);
    assert_eq!(
        现场
            .catalog
            .identification_of(键)
            .expect("读得出")
            .map(|it| it.0),
        Some(State::Matched),
        "前提：裁决投影下来先得是一条命中"
    );
    assert!(
        现场
            .catalog
            .variant(键)
            .expect("读得出")
            .expect("变体在")
            .work_id
            .is_some(),
        "前提：变体挂上了那个作品"
    );

    // 同一条路径换成另一份字节（大小也变，三元组一定变）。
    std::thread::sleep(Duration::from_millis(20));
    写(
        &现场.dir.path().join("FC/某汉化.zip"),
        &zip_container(&[ZipEntrySpec::stored("rom.nes", 卡带(0xB4, 40_961))]),
    );
    let root = 现场.dir.path().to_path_buf();
    扫一遍(&mut 现场.catalog, &root);

    let 新锚 = 内容锚(&现场.catalog, 键);
    assert!(
        现场.store.find(&新锚).expect("读得出").is_none(),
        "前提：新那份字节没被裁过"
    );
    assert_eq!(
        现场.catalog.identification_of(键).expect("读得出"),
        None,
        "字节换了，那条结论不再是这份字节的结论——重扫之后它该回到「还没识别」"
    );
    assert!(
        现场.catalog.candidates_of(键).expect("读得出").is_empty(),
        "候选跟着结论走：依据里写的是旧内容的 CRC"
    );
    let variant = 现场.catalog.variant(键).expect("读得出").expect("变体还在");
    assert_eq!(
        (variant.work_id, variant.release_id),
        (None, None),
        "作品与发行版的链接也是按字节得出来的，一起走"
    );
    assert!(
        现场.store.find(&旧锚).expect("读得出").is_some(),
        "沉淀库里那条裁决不动：它钉的是那串字节，人定的东西不因为一次重扫消失"
    );
}

// ── 二、按根续扫之后的成型代号 ────────────────────────────────────────────

/// 一个能扫的小根：两个平台目录各一个文件。
fn 建根(root: &str) -> MemFs {
    let mut library = MemFs::new();
    library.file(format!("{root}/FC/甲.zip"), b"pretend".to_vec());
    library.file(format!("{root}/FC/乙.zip"), b"pretend too".to_vec());
    library.dir(root);
    library
}

fn 断点选项(dir: &Path, root_name: &str) -> CheckpointOptions {
    CheckpointOptions {
        path: dir.join(format!("{root_name}.checkpoint.json")),
        interval: Duration::ZERO,
        resume: true,
    }
}

#[test]
fn 按根续扫之后成型代号不倒退报告不误报成型比库旧() {
    // ⭐ **成型对着的是整份库，不是这一趟走的那个根**（ADR-0022）。续跑沿用断点里的
    // 旧代号是对的——收尾要靠它认「这次没见到的」——但拿那个代号去记「成型跑到哪一趟
    // 为止」就错了：中间扫过的别的根已经把全局代号推走，`shaped_scan` 于是倒退，
    // 而 `romcat report` 比的是 `last_traversal()`（全库最后一趟）。
    let workspace = temp_dir("rescan-resume");
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");

    // 甲：一开扫就被叫停，断点落下，遍历代号 1。
    let mut 甲 = ScanOptions::named("/Volumes/甲", "甲盘");
    甲.jobs = Jobs::Fixed(1);
    甲.checkpoint = Some(断点选项(workspace.path(), "甲盘"));
    let 停手 = Handle::new();
    停手.cancel().cancel();
    let 中断 =
        scan::scan(&建根("/Volumes/甲"), &mut catalog, &甲, &停手).expect("中断也算正常返回");
    assert!(中断.interrupted, "这一趟本该被中断");
    assert!(!中断.shaped, "中断的扫描不成型");

    // 乙：完整扫完一遍，遍历代号 2，成型跟着跑。
    let mut 乙 = ScanOptions::named("/Volumes/乙", "乙盘");
    乙.jobs = Jobs::Fixed(1);
    let 扫乙 = scan::scan(&建根("/Volumes/乙"), &mut catalog, &乙, &Handle::new()).expect("扫得动");
    assert!(!扫乙.interrupted && 扫乙.shaped);

    // 回来续甲：完整跑完，成型确实又跑了一遍——对着的是整份库。
    let 续甲 = scan::scan(&建根("/Volumes/甲"), &mut catalog, &甲, &Handle::new()).expect("扫得动");
    assert!(续甲.report.resumed, "前提：这一趟认得出是续跑");
    assert!(!续甲.interrupted && 续甲.shaped, "前提：续跑扫完了也成型了");

    let last = catalog
        .last_traversal()
        .expect("读得出")
        .expect("扫过")
        .scan;
    assert_eq!(
        catalog.shaped_scan().expect("读得出"),
        Some(last),
        "成型对着的是整份库，记下的代号该是全局最新那一个，不是断点里那个旧的"
    );
    let aggregate = catalog
        .aggregate(&Limits::default(), &Manifest::builtin())
        .expect("折得出统计");
    let report = HealthReport::build(&aggregate, &catalog.report_meta().expect("元信息读得出来"));
    assert!(!report.shaping.stale, "成型明明刚跑完，报告不许说它比库旧");
}

// ── 三、非 UTF-8 名字的容器 ──────────────────────────────────────────────

/// 非 UTF-8 的文件名只在 Unix 上造得出来：Windows 的路径是 UTF-16，
/// 而 macOS 的 APFS 压根不收不合法的 UTF-8 名字——这条路只有内存文件系统走得通。
#[cfg(unix)]
#[test]
fn 非_utf8_名字的容器第二趟扫描不撞主键() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::path::PathBuf;

    // ⭐ **「是不是容器」这个判断不许从键上推。** 键里那段指纹缀在扩展名之后
    // （`path::catalog_key`），`Path::extension` 于是读出 `zip#0123…`，写入侧因此
    // 「先清后插」那一步整个跳过，第二趟穿透的结果撞上 `container.key` 主键，
    // 一条读不出内部构成的容器让整趟扫描失败。
    let 名字 = PathBuf::from(OsString::from_vec(
        b"/Volumes/\xe4\xb8\x99/FC/\xff\xfe\xff.zip".to_vec(),
    ));
    let mut library = MemFs::new();
    // 一份**穿不透**的 zip：名字是 zip，里面读不出中央目录（ADR-0021 的那一档）。
    library.file(&名字, b"this is not a zip at all".to_vec());
    library.dir("/Volumes/丙");

    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::named("/Volumes/丙", "丙盘");
    options.jobs = Jobs::Fixed(1);
    let 首扫 = scan::scan(&library, &mut catalog, &options, &Handle::new()).expect("首扫扫得动");
    assert_eq!(
        首扫.aggregate.containers.totals().failed,
        1,
        "前提：这一份穿不透，穿不透的结论要落库"
    );

    let 二扫 = scan::scan(&library, &mut catalog, &options, &Handle::new())
        .expect("第二趟同样得扫得动：穿不透的容器不许把整趟扫描带下水");
    assert_eq!(
        二扫.aggregate.containers.totals().failed,
        1,
        "重穿一遍还是那一个，不该多出一条"
    );
    assert_eq!(二扫.report.totals.files, 1, "库里就这一个文件");
}
