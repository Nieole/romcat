//! **平台纠正**（票 `gui-looks-like-the-design/28`）：磁盘上摆一份目录与内容对不上的主库，
//! 按组下一个决定，看三件事有没有跟着变——
//!
//! 1. **库体检那一格**：处理过的组不再计数，撤销之后重新计数。
//! 2. **下一趟识别**：按纠正后的平台重新匹配，而「目录声明的平台」那一列一个字不动
//!    （票 `one-criterion-per-thing/03` 那条对照物）。
//! 3. **ADR-0004**：从头到尾**盘上一个字节都没动**——整份快照逐字节相同。

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use romcat_core::catalog::{Catalog, Roots};
use romcat_core::dat::repo::DatRepo;
use romcat_core::fs::RealFs;
use romcat_core::identify::{self, Options, PlatformFixes, fuzzy};
use romcat_core::platform::Manifest;
use romcat_core::report::{HealthReport, PlatformCorrections};
use romcat_core::scan::aggregate::Limits;
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::task::Handle;
use romcat_core::testing::cart as real;
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::verdict::{self, PlatformDecision, Store};

/// 这份主库在盘上叫什么。
const LIBRARY: &str = "这一份主库";

struct 现场 {
    dir: TempDir,
    catalog: Catalog,
    store: Store,
}

/// 盘上摆一份目录与内容对不上的主库，**两组各挑一种凭据**：
///
/// - `fc/` 底下两份 `.fds`——扩展名只可能属于 FDS，而**卡带那一层对它一个字都说不出**
///   （磁碟不是卡带）。这一组用来看「按内容改」：改之前识别只好听目录的。
/// - `gba/` 底下一份 `.nds`，**头部字节是真的**（`testing::cart`），所以内容那一层自己就说得出
///   NDS。这一组用来看「保持目录的说法」——它要**压得过内容那一层**，否则人定完等于没定。
/// - `gba/` 底下还有一份本分的 `.gba`：同一个目录里不冲突的那一份一点不受影响。
/// - `杂物/` 底下也有一份 `.nds`——**未纳入管理的目录不进识别管线**，不算一条冲突（ADR-0011 修订段）。
fn 建现场() -> 现场 {
    let dir = temp_dir("platform-fix");
    let root = dir.path();
    for (相对, 内容) in [
        ("fc/日版/塞尔达传说.fds", b"fds-a".to_vec()),
        ("fc/合集/银河战士.fds", b"fds-b".to_vec()),
        (
            "gba/汉化/逆转裁判4.nds",
            real::padded(&real::NDS_GYAKUTEN_KENJI, 1 << 16),
        ),
        (
            "gba/汉化/火焰之纹章.gba",
            real::padded(&real::GBA_ROCKMAN_EXE6, 1 << 16),
        ),
        (
            "杂物/放错的.nds",
            real::padded(&real::NDS_GYAKUTEN_KENJI, 1 << 15),
        ),
    ] {
        let 落点 = root.join(相对);
        fs::create_dir_all(落点.parent().expect("有上级目录")).expect("建得出目录");
        fs::write(&落点, &内容).expect("写得进");
    }

    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    let mut options = ScanOptions::named(root, "库");
    options.jobs = Jobs::Fixed(1);
    scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");

    现场 {
        dir,
        catalog,
        store: Store::in_memory().expect("开得出沉淀库"),
    }
}

fn 体检(现场: &现场) -> HealthReport {
    let aggregate = 现场
        .catalog
        .aggregate(&Limits::default(), &Manifest::builtin())
        .expect("折得出统计");
    HealthReport::build(
        &aggregate,
        &现场.catalog.report_meta().expect("元信息读得出来"),
    )
}

fn 那一层(现场: &现场) -> PlatformCorrections {
    PlatformCorrections::build(
        &体检(现场),
        &Manifest::builtin(),
        &现场
            .store
            .platform_corrections(LIBRARY)
            .expect("读得出人定过的那些"),
    )
}

/// 跑一趟识别，带上人定过的那些平台纠正。
fn 跑一趟识别(现场: &mut 现场) {
    let corrections = 现场
        .store
        .platform_corrections(LIBRARY)
        .expect("读得出人定过的那些");
    let mut options = Options::new(Roots::single("库", 现场.dir.path()));
    options.platform_fixes = Some(PlatformFixes::new(&Manifest::builtin(), &corrections));
    let repo = DatRepo::in_memory().expect("开得出 DAT 库");
    identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &identify::Ammo {
            repo: &repo,
            verdicts: &verdict::Index::default(),
            naming: &fuzzy::Naming::off(),
            guessing: &identify::model::Guessing::off(),
            titledb: None,
        },
        &options,
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别跑得动");
}

/// 这个变体**按哪个平台算**：读的是识别落下来的那一列（`identification.platform`）。
fn 按哪个平台算(现场: &现场, 键里带着: &str) -> Option<String> {
    现场
        .catalog
        .identified_platforms()
        .expect("读得出")
        .into_iter()
        .find(|(key, _)| key.contains(键里带着))
        .map(|(_, platform)| platform)
}

/// 中立库里**目录声明的那个平台**：平台冲突那张报表拿它当对照物，纠正过也不许回写。
fn 目录声明的(现场: &现场, 键里带着: &str) -> Option<String> {
    现场
        .catalog
        .variants()
        .expect("读得出变体")
        .into_iter()
        .find(|variant| variant.key.contains(键里带着))
        .and_then(|variant| variant.platform)
}

#[test]
fn 平台冲突按组列出来_每组说得出从哪到哪几条凭什么两条样例() {
    let 现场 = 建现场();
    let 这一层 = 那一层(&现场);
    let 各组: Vec<(String, String, u64)> = 这一层
        .groups()
        .iter()
        .map(|one| {
            (
                one.group.declared.clone(),
                one.group.implied.clone(),
                one.group.count,
            )
        })
        .collect();
    assert_eq!(
        各组,
        vec![
            ("FC".to_string(), "FDS".to_string(), 2),
            ("GBA".to_string(), "NDS".to_string(), 1),
        ],
        "未纳入管理的目录那一份不算；条数多的那一组在前"
    );
    let 头一组 = &这一层.groups()[0];
    assert_eq!(头一组.headline(), "FC 目录里的 FDS 游戏");
    assert!(
        头一组.reason().contains(".fds"),
        "理由说得出凭什么这么判：{}",
        头一组.reason()
    );
    assert_eq!(头一组.group.examples.len(), 2, "两条样例列得出来");
    assert!(头一组.pending(), "还没处理过");
    assert_eq!(这一层.remaining(), 3);
}

#[test]
fn 按内容改之后下一趟识别按新平台算_目录声明的那一列一个字不动_那一格不再数它() {
    let mut 现场 = 建现场();
    跑一趟识别(&mut 现场);
    assert_eq!(
        按哪个平台算(&现场, "塞尔达传说.fds").as_deref(),
        Some("FC"),
        "没人定过的时候，内容那一层说不出话就退回目录说的那个"
    );

    现场
        .store
        .set_platform_correction(LIBRARY, "FC", "FDS", PlatformDecision::ByContent)
        .expect("记得下");
    现场.catalog.clear_identifications().expect("清得掉上一趟");
    跑一趟识别(&mut 现场);

    assert_eq!(
        按哪个平台算(&现场, "塞尔达传说.fds").as_deref(),
        Some("FDS"),
        "人说按内容改，下一趟识别就按 FDS 去撞"
    );
    assert_eq!(
        按哪个平台算(&现场, "火焰之纹章.gba").as_deref(),
        Some("GBA"),
        "不冲突的那一份一点不受影响"
    );
    assert_eq!(
        目录声明的(&现场, "塞尔达传说.fds").as_deref(),
        Some("FC"),
        "「目录声明的平台」那一列是平台冲突那张报表的对照物，纠正也不许回写它"
    );

    let 这一层 = 那一层(&现场);
    assert_eq!(这一层.remaining(), 1, "处理过的那一组不再算进概要");
    assert_eq!(这一层.handled_note().as_deref(), Some("已处理 1 组"));
    assert_eq!(这一层.groups()[0].settled().as_deref(), Some("已改为 FDS"));
    assert_eq!(
        体检(&现场).conflicts.total,
        3,
        "报告说的是盘上的事实：处理过也一条不少"
    );
}

#[test]
fn 保持目录的说法压得过内容那一层_而且不再问第二遍() {
    // 「保持」不是「什么都不做」：这一份 `.nds` 的**头部字节是真的**，内容那一层自己就说得出
    // NDS（ADR-0011）。人说保持目录的说法，它就得压过去——否则定完下一趟识别照旧按 NDS 算，
    // 而那正是他说不要的。
    let mut 现场 = 建现场();
    跑一趟识别(&mut 现场);
    assert_eq!(
        按哪个平台算(&现场, "逆转裁判4.nds").as_deref(),
        Some("NDS"),
        "没人定过的时候内容说了算"
    );

    现场
        .store
        .set_platform_correction(LIBRARY, "GBA", "NDS", PlatformDecision::KeepDeclared)
        .expect("记得下");
    现场.catalog.clear_identifications().expect("清得掉上一趟");
    跑一趟识别(&mut 现场);

    assert_eq!(
        按哪个平台算(&现场, "逆转裁判4.nds").as_deref(),
        Some("GBA"),
        "人说保持目录的说法，内容那一层也压不过它"
    );
    let 这一层 = 那一层(&现场);
    assert_eq!(
        这一层.remaining(),
        2,
        "「保持」也算处理过——否则每体检一趟就要再问一遍"
    );
    assert_eq!(这一层.groups()[1].settled().as_deref(), Some("已保持 GBA"));
}

#[test]
fn 撤销之后这一组回到还没处理_那一格重新数它() {
    let mut 现场 = 建现场();
    现场
        .store
        .set_platform_correction(LIBRARY, "FC", "FDS", PlatformDecision::ByContent)
        .expect("记得下");
    assert_eq!(那一层(&现场).remaining(), 1);

    assert!(
        现场
            .store
            .undo_platform_correction(LIBRARY, "FC", "FDS")
            .expect("撤得掉")
    );
    let 这一层 = 那一层(&现场);
    assert_eq!(这一层.remaining(), 3, "撤销之后那一格重新数它");
    assert!(这一层.groups()[0].pending());
    assert_eq!(这一层.handled_note(), None);
}

#[test]
fn 纠正一整轮下来盘上的文件一个字节都没动() {
    // **主库只读**（ADR-0004）：纠正改的是「这一组按哪个平台算」，不是盘上的文件。
    // 目标目录整份快照逐字节相同——照票 26「移除一个根不动盘上的任何一个字节」那条的形状写。
    let mut 现场 = 建现场();
    let 之前 = 快照(现场.dir.path());

    跑一趟识别(&mut 现场);
    现场
        .store
        .set_platform_correction(LIBRARY, "FC", "FDS", PlatformDecision::ByContent)
        .expect("记得下");
    现场
        .store
        .set_platform_correction(LIBRARY, "GBA", "NDS", PlatformDecision::KeepDeclared)
        .expect("记得下");
    现场.catalog.clear_identifications().expect("清得掉上一趟");
    跑一趟识别(&mut 现场);
    let _ = 那一层(&现场);
    现场
        .store
        .undo_platform_correction(LIBRARY, "FC", "FDS")
        .expect("撤得掉");

    assert_eq!(
        之前,
        快照(现场.dir.path()),
        "纠正不移动、不改名、不改写任何一个文件"
    );
}

/// 一个目录整份的样子：每个文件的相对路径 → 它的字节。
fn 快照(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut 待走 = vec![dir.to_path_buf()];
    while let Some(这一层) = 待走.pop() {
        for 项 in fs::read_dir(&这一层).expect("读得动目录") {
            let 项 = 项.expect("读得动一项");
            let 路 = 项.path();
            if 路.is_dir() {
                待走.push(路);
            } else {
                let 相对 = 路
                    .strip_prefix(dir)
                    .expect("在这个目录下面")
                    .to_string_lossy()
                    .into_owned();
                out.insert(相对, fs::read(&路).expect("读得动"));
            }
        }
    }
    out
}
