//! 库屏底下的**库体检**那一块（票 `gui-looks-like-the-design/27`）：对主库跑一遍只读扫描得到的报告，摆成一块概要，
//! 每一格点进去看明细。**只报告、不处理**——重复拷贝只发现不删除，主库一个字节都不写（ADR-0004）。
//!
//! 这几条读的是这一帧**画出来的字**（`shared::画出来的字`），与 `tests/roots.rs` 同一条接缝：那一块在库屏正文最底下，
//! 1280×800 的视口里要先滚到底（`shared::滚到底`）才画得出来——egui 不画视口外的字。
//!
//! 主库**一律拿本地临时目录模拟**：绝不去动任何真实设备。

use romcat_core::catalog::Catalog;
use romcat_core::site::Site;
use romcat_core::testing::{TempDir, temp_dir};
use romcat_gui::app::{App, View};
use romcat_gui::headless;

mod shared;
use shared::{滚到底, 点一下};

/// 一扇开在库屏上的主窗口，中立库落在临时工作目录里（与 `tests/roots.rs` 的现场同一个摆法：版式偏好写进工作目录，
/// 几条测试各用各的）。
struct 现场 {
    工作区: TempDir,
    app: App,
}

impl 现场 {
    /// 一份刚建出来、一个根都没有的库。
    fn 摆好() -> Self {
        let 工作区 = temp_dir("gui-health-ws");
        drop(Catalog::create(&Self::库文件(&工作区), "fixture").expect("建得出中立库"));
        let app = Self::开窗(&工作区);
        Self { 工作区, app }
    }

    /// 这份中立库落在工作目录里的哪儿。
    fn 库文件(工作区: &TempDir) -> std::path::PathBuf {
        工作区.path().join("catalog").join("fixture.sqlite3")
    }

    /// 对着这个工作目录与这份中立库开一扇主窗口，停在库屏上。
    fn 开窗(工作区: &TempDir) -> App {
        let site = Site::open_file(工作区.path(), &Self::库文件(工作区), None).expect("开得出现场");
        let mut app = App::new(site, 工作区.path().to_path_buf());
        app.show_view(View::Library);
        app
    }

    /// **关掉再打开**：同一个工作目录、同一份中立库，新开一个窗口本体。版式偏好在构造时从工作目录读出来。
    fn 关掉再打开(&mut self) {
        self.app = Self::开窗(&self.工作区);
    }
}

/// 屏上有一行**整行就是**这几个字（标题、按钮上的字）。
fn 有一行正好是(屏上: &str, 那几个字: &str) -> bool {
    屏上.lines().any(|line| line.trim() == 那几个字)
}

#[test]
fn 还没扫描时库屏底下有库体检那一块_说清扫描完成后才生成报告() {
    // 票 27 验收第 1 条后半句「还没扫描时是空态」；标题栏与空态那一句照设计稿库屏 `data-panel="health"` 逐字。
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();

    let 屏上 = 滚到底(&ctx, |ui| 现场.app.ui(ui));
    assert!(
        有一行正好是(&屏上, "库体检"),
        "库屏底下没有「库体检」那一块：\n{屏上}"
    );
    assert!(
        屏上.contains("只读检查，只报告、不处理"),
        "库体检标题栏没说它只读、只报告：\n{屏上}"
    );
    assert!(
        有一行正好是(&屏上, "重新体检"),
        "库体检标题栏上没有「重新体检」：\n{屏上}"
    );
    assert!(
        屏上.contains("扫描完成后生成体检报告。"),
        "还没扫描，库体检那一块没画空态那一句：\n{屏上}"
    );
}

#[test]
fn 库体检那一块收得起来_关掉再打开还是收着的() {
    // 设计稿那一块标题栏右端有折叠标（`data-pcol="health"`），与根、数据源、导出设置三块同一个样子：点标题收起、再点摊开，
    // **收起来的样子记在工作目录的版式偏好里**，关掉窗口再打开还是收着的（票 06 那三块同一条路）。
    const 空态: &str = "扫描完成后生成体检报告。";
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    let 屏上 = 滚到底(&ctx, |ui| 现场.app.ui(ui));
    assert!(屏上.contains(空态), "库体检那一块默认就收着：\n{屏上}");

    点一下(&ctx, "库体检", |ui| 现场.app.ui(ui));
    let 屏上 = 滚到底(&ctx, |ui| 现场.app.ui(ui));
    assert!(
        有一行正好是(&屏上, "库体检"),
        "收起之后连「库体检」的标题都没了——收起来的块得还点得开：\n{屏上}"
    );
    assert!(
        !屏上.contains(空态),
        "点了「库体检」，那一块却没收起来：\n{屏上}"
    );

    // 换一个窗口本体、换一份 egui 上下文——记住它的只能是工作目录里那份文件。
    现场.关掉再打开();
    let ctx = headless::context();
    let 屏上 = 滚到底(&ctx, |ui| 现场.app.ui(ui));
    assert!(
        !屏上.contains(空态),
        "关掉再打开，库体检那一块又摊开了：\n{屏上}"
    );

    // 再点一下就摊开，下次打开也记得是摊开的。
    点一下(&ctx, "库体检", |ui| 现场.app.ui(ui));
    现场.关掉再打开();
    let 屏上 = 滚到底(&headless::context(), |ui| 现场.app.ui(ui));
    assert!(
        屏上.contains(空态),
        "摊开之后关掉再打开，库体检那一块还收着：\n{屏上}"
    );
}
