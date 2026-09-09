//! **启动那条路**：定位要开哪份库、开出**现场**、进主窗口。
//!
//! 这几条验的是那条路本身，而不是它跑完之后的五屏。从前它整条摊在 `main.rs` 里，
//! 一条测试都够不着——「三种给法行为一致」只能靠人手敲三遍命令。现在它住在
//! [`romcat_gui::program::Program`] 上，于是**开进主窗口**这段过渡可测。
//!
//! **一个字节都不碰真库**：中立库一律现建在临时目录里（ADR-0004）。

use std::path::Path;

use romcat_core::catalog::Catalog;
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::workspace::{self, Slug};
use romcat_gui::headless;
use romcat_gui::program::Program;
use romcat_gui::site::Locate;

mod shared;

/// 在这个工作目录里现建一份中立库，返回它的文件路径。
///
/// **建的是磁盘上那一份**，不是内存里那一份：启动那条路认的正是「这个文件在不在」
/// （`Site::open` 在 `catalog.exists()` 那一步挡下），只活在内存里的库它一眼都看不见。
fn 建一份库(workspace: &Path, slug: Slug<'_>) -> std::path::PathBuf {
    let path = workspace::catalog_path(workspace, slug);
    drop(Catalog::open(&path).expect("开得出中立库"));
    path
}

/// 那份中立库的主文件名——**主库在路径锚里叫什么名字**（`Site::library`），
/// 也就是窗口标题里写着的那一段。
fn 主库名(catalog: &Path) -> String {
    catalog
        .file_stem()
        .expect("有主文件名")
        .to_string_lossy()
        .into_owned()
}

/// 一个工作目录，连它里头那份现成的库。
fn 摆好一份库(tag: &str) -> (TempDir, std::path::PathBuf) {
    let 工作目录 = temp_dir(tag);
    let 库文件 = 建一份库(工作目录.path(), Slug::Named("测试库"));
    (工作目录, 库文件)
}

#[test]
fn 给一份现成的库就走完启动进主窗口() {
    let (_工作目录, 库文件) = 摆好一份库("gui-program-开进主窗口");

    let mut program = Program::start(&Locate {
        catalog: Some(&库文件),
        ..Locate::default()
    })
    .expect("走得完启动");

    // **标题写着开的是哪一份**：任务栏上并排两个 romcat 时，那是唯一分得开的地方。
    assert!(
        program.window_title().contains(&主库名(&库文件)),
        "标题里没写开的是哪一份库：{}",
        program.window_title(),
    );

    // 主窗口真的画出来了：顶栏上五屏的名字都在。
    let ctx = headless::context();
    let out = headless::frame(&ctx, headless::input(), |ui| program.ui(ui));
    let 屏上 = shared::画出来的字(&out);
    for 屏 in ["待确认队列", "库", "浏览", "子库", "任务"] {
        assert!(屏上.contains(屏), "顶栏上没有「{屏}」这一屏：\n{屏上}");
    }
}

#[test]
fn 三种给法开出来的是同一份库() {
    // 三种给法（主库根 / `--library <名字>` / `--catalog <文件>`）折的是同一条算法
    // （`workspace::catalog_path`）。它们在启动那条路上从来没有一起被验过——从前那段
    // 摊在 `main.rs` 里，只能靠人手敲三遍命令。
    let 工作目录 = temp_dir("gui-program-三种给法");
    let 主库根 = temp_dir("gui-program-主库根");

    // 按名字那份与直接给文件那份**是同一个文件**：两条路开出来的主库名必须一模一样，
    // 不然同一份库裁出来的**路径锚**会记在两个名字下。
    let 按名字 = 建一份库(工作目录.path(), Slug::Named("测试库"));
    let 按根 = 建一份库(工作目录.path(), Slug::AtPath(主库根.path()));

    let 开出的标题 =
        |locate: &Locate<'_>| Program::start(locate).expect("走得完启动").window_title();

    let 名字那条 = 开出的标题(&Locate {
        library: Some("测试库"),
        workspace: Some(工作目录.path()),
        ..Locate::default()
    });
    let 文件那条 = 开出的标题(&Locate {
        catalog: Some(&按名字),
        ..Locate::default()
    });
    let 根那条 = 开出的标题(&Locate {
        root: Some(主库根.path()),
        workspace: Some(工作目录.path()),
        ..Locate::default()
    });

    assert_eq!(
        名字那条, 文件那条,
        "`--library` 与 `--catalog` 指着同一个文件，开出来的却是两个名字",
    );
    assert!(
        名字那条.contains(&主库名(&按名字)),
        "标题里写的不是那份库：{名字那条}",
    );
    assert!(
        根那条.contains(&主库名(&按根)),
        "给主库根开出来的不是它那一份：{根那条}",
    );
}

#[test]
fn 一个参数都不给就说清要开哪份库() {
    // **这一张不改这个行为**（票 `gui-self-sufficient/03` 才把这条路引到开场那一屏）。
    // 不擅自造一份合成的糊弄人：假数据与真库在界面上长得一模一样。
    let Err(说的) = Program::start(&Locate::default()) else {
        panic!("一个参数都不给应当开不起来，界面里没有它开得起来的东西");
    };
    // **字面量，不引 `program::NO_LIBRARY`**：引常量的话两边同一个出处，谁把那句话
    // 改成别的，两边一起变而这条测试照样绿。这一条钉的正是「人看见的还是那句话」。
    assert!(说的.contains("说清要开哪份库"), "说的不是那句话：{说的}");
}

#[test]
fn 库不在就如实说而不是空手开一个窗() {
    let 工作目录 = temp_dir("gui-program-库不在");
    let Err(说的) = Program::start(&Locate {
        library: Some("压根不存在的那一份"),
        workspace: Some(工作目录.path()),
        ..Locate::default()
    }) else {
        panic!("那份库根本不在，却开起来了");
    };
    // 断的是**它说得出你要开的是哪一份**，不是核心库那句话的措辞——措辞归
    // `SiteError::NoCatalog` 管，抄一段过来只会在核心改一个字时红。
    assert!(
        说的.contains("压根不存在的那一份"),
        "没说清开不起来的是哪一份：{说的}",
    );
}
