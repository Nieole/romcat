//! 库屏底下的**库体检**那一块（票 `gui-looks-like-the-design/27`）：把从**中立库**折出来的那份体检报告摆成一块概要，
//! 每一格点进去看明细。体检那一趟不读主库（盘在扫描那一趟走过了）。**只报告、不处理**——重复拷贝只发现不删除，
//! 主库一个字节都不写（ADR-0004）。
//!
//! 这几条读的是这一帧**画出来的字**（`shared::画出来的字`），与 `tests/roots.rs` 同一条接缝：那一块在库屏正文最底下，
//! 1280×800 的视口里要先滚到底（`shared::滚到底`）才画得出来——egui 不画视口外的字。
//!
//! 主库**一律拿本地临时目录模拟**：绝不去动任何真实设备。

use std::path::Path;

use romcat_core::catalog::Catalog;
use romcat_core::platform::Manifest;
use romcat_core::report::{DuplicateDetails, HealthReport, human_bytes};
use romcat_core::scan::aggregate::Limits;
use romcat_core::site::Site;
use romcat_core::testing::cart;
use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};
use romcat_gui::app::{App, View};
use romcat_gui::clock::Clock;
use romcat_gui::headless;

mod shared;
use shared::{占位活, 滚到底, 点一下, 等任务台空了};

/// 「上次体检」画时刻用的钟：此刻钉在 2026-09-14 08:00（UTC），偏移 0。
const 此刻: i64 = 1_789_372_800;

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

    /// 加一个根。屏头「添加根…」选中一个目录之后走的就是这条（`roots::Screen::add_root`）。
    fn 加根(&mut self, 目录: &Path, 名字: &str) {
        let (screen, site, _) = self.app.roots_site_and_tasks();
        screen.add_root(site, &目录.to_string_lossy(), 名字);
    }

    /// 扫一个根：排上任务台，等台上空了。根那一行「扫描」按的就是它。
    fn 扫(&mut self, 名字: &str) {
        let (screen, site, tasks) = self.app.roots_site_and_tasks();
        screen.scan(site, tasks, 名字);
        self.等台上空了();
    }

    /// 等任务台上的活都收场、都认领完（`shared::等任务台空了`）。
    fn 等台上空了(&mut self) {
        等任务台空了(&mut self.app);
    }
}

/// 一块只往临时目录里写的「盘」：库体检每一格要报的东西各摆一份（不可读与平台不符在临时目录里造不出来，那两格是 0）。
/// **一个字节都不碰真盘。**
fn 有体检发现的盘() -> TempDir {
    let 盘 = temp_dir("gui-health-disk");
    for (相对, 字节) in [
        // 重复拷贝：同名且同大小的两份。
        ("FC/魂斗罗.zip", zip(2048)),
        ("FC/备份/魂斗罗.zip", zip(2048)),
        // 成型存疑：FDS 没声明多碟同族，两面磁碟各成一个变体。
        ("FDS/某游戏/某游戏 (Disk 1).fds", vec![1_u8; 64]),
        ("FDS/某游戏/某游戏 (Disk 2).fds", vec![2_u8; 64]),
        // 附属文件落单：同目录里没有同名的主文件。
        ("GBA/汉化/火焰之纹章.sav", vec![3_u8; 64]),
        // 非游戏资产：平台目录里的 `bios/`。
        ("PS1/bios/scph1001.bin", vec![4_u8; 512]),
        // 未纳入管理的目录：不在任何平台目录下。
        ("杂物/说明.txt", vec![5_u8; 16]),
    ] {
        let 落点 = 盘.path().join(相对);
        std::fs::create_dir_all(落点.parent().expect("有上级目录")).expect("建得出目录");
        std::fs::write(&落点, 字节).expect("写得进");
    }
    盘
}

/// 一块只往临时目录里写的「盘」，专为**平台纠正**摆的（票 `gui-looks-like-the-design/28`）：
/// 两组「目录说 A、内容是 B」，一组只能改、一组改不改都行。**一个字节都不碰真盘。**
///
/// - `fc/` 底下两份 `.fds`：扩展名只可能属于 FDS，而 FC 跑不了磁碟机的游戏——**只能改**。
/// - `3ds/` 底下一份 `.nds`，头部字节是真的（`testing::cart`）：3DS **向下兼容** NDS，
///   有意放在 `3ds/` 也说得通——**改不改都行**。
fn 有平台不符的盘() -> TempDir {
    let 盘 = temp_dir("gui-platfix-disk");
    let 摆 = |相对: &str, 字节: Vec<u8>| {
        let 落点 = 盘.path().join(相对);
        std::fs::create_dir_all(落点.parent().expect("有上级目录")).expect("建得出目录");
        std::fs::write(&落点, 字节).expect("写得进");
    };
    摆("fc/日版/塞尔达传说.fds", vec![1_u8; 64]);
    摆("fc/合集/银河战士.fds", vec![2_u8; 64]);
    摆(
        "3ds/合集/雷顿教授.nds",
        cart::padded(&cart::NDS_GYAKUTEN_KENJI, 1 << 16),
    );
    盘
}

/// 摆好一份有平台不符的库，滚到库屏底下，交出现场。
fn 摆好平台不符的现场(ctx: &egui::Context, 盘: &TempDir) -> 现场 {
    let mut 现场 = 现场::摆好();
    现场.加根(盘.path(), "主库");
    现场.扫("主库");
    滚到底(ctx, |ui| 现场.app.ui(ui));
    现场
}

/// 库体检里 `标题` 那一格画的数与小字：一格里标题、数、小字是连着画的三段。取**最后**一处写着这个标题的——
/// 库体检那一块在库屏最底下。
fn 那一格(屏上: &str, 标题: &str) -> (String, String) {
    let 行: Vec<&str> = 屏上.lines().map(str::trim).collect();
    let Some(在) = 行.iter().rposition(|line| *line == 标题) else {
        panic!("屏上没有「{标题}」那一格：\n{屏上}");
    };
    let 取 = |偏: usize| {
        行.get(在 + 偏)
            .map_or_else(String::new, ToString::to_string)
    };
    (取(1), 取(2))
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
    assert!(屏上.contains(空态), "库体检那一块默认是摊开的：\n{屏上}");

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

#[test]
fn 扫过之后八格的数照核心库的报告画_疑似同一作品在识别完成前写识别完成后才有() {
    // 票 27 验收第 1 条「概要八格，数字来自核心库的报告」：扫完一个根，就用扫描交回的那份报告（拿主意的人 2026-09-15 答
    // 岔路口 8）。期望的数照盘上摆的东西写，不照报告抄。
    let ctx = headless::context();
    let 盘 = 有体检发现的盘();
    let mut 现场 = 现场::摆好();
    现场.加根(盘.path(), "主库");
    现场.扫("主库");

    let 屏上 = 滚到底(&ctx, |ui| 现场.app.ui(ui));
    assert!(
        !屏上.contains("扫描完成后生成体检报告。"),
        "扫过了还画着空态：\n{屏上}"
    );
    assert_eq!(
        那一格(&屏上, "重复拷贝"),
        ("1 组".to_string(), format!("可腾出 {}", human_bytes(2048)))
    );
    assert_eq!(那一格(&屏上, "目录与内容平台不符").0, "0 条");
    assert_eq!(那一格(&屏上, "成型存疑").0, "1 处");
    assert_eq!(那一格(&屏上, "不可读").0, "0 个");
    let (未纳入的数, 未纳入的小字) = 那一格(&屏上, "未纳入管理的目录");
    assert_eq!(未纳入的数, "1 个");
    // 岔路口 10：设计稿那句「扫描时跳过」不真——照扫照报，只是不进识别与刮削（ADR-0011 修订段）。
    assert!(
        !未纳入的小字.contains("跳过") && 未纳入的小字.contains("识别"),
        "未纳入管理的目录那句小字：{未纳入的小字}"
    );
    assert_eq!(那一格(&屏上, "附属文件落单").0, "1 个");
    assert_eq!(那一格(&屏上, "非游戏资产").0, "1 个");
    // 疑似同一作品的判断在票 17，这之前界面不许自己算一个（验收第 6 条）；识别还没跑，照票写。
    assert_eq!(
        那一格(&屏上, "疑似同一作品"),
        ("—".to_string(), "识别完成后才有".to_string())
    );
}

/// 任务台上叫「库体检 · 全部根」的那几趟：正在跑的与排着队的。
fn 台上的体检(app: &App) -> usize {
    let 跑着 = app
        .tasks()
        .running()
        .is_some_and(|live| live.name == "库体检 · 全部根");
    let 排着 = app
        .tasks()
        .queued()
        .iter()
        .filter(|(_, name)| name == "库体检 · 全部根")
        .count();
    usize::from(跑着) + 排着
}

#[test]
fn 开窗后头一次进库屏有扫过的根_自动排一趟体检_跑着时说正在体检_收场后写上次体检的时刻_一次只跑一趟()
 {
    // 拿主意的人 2026-09-15 答岔路口 8：开窗后头一次进库屏、且有扫过的根时，自动排一趟只读体检；「上次体检」写那一趟
    // 收场的时刻；一次只跑一趟。验收第 5 条「「重新体检」排进任务台，跑的时候概要上说明白正在体检」。
    let 盘 = 有体检发现的盘();
    let mut 现场 = 现场::摆好();
    现场.加根(盘.path(), "主库");
    现场.扫("主库");

    // 换一扇窗：扫描交回的那份报告跟着上一扇窗走了，这一扇还没有报告。台上先摆一趟占位活，体检那一趟就稳稳排在队里。
    现场.关掉再打开();
    现场
        .app
        .roots_site_and_tasks()
        .0
        .set_clock(Clock::fixed(此刻, 0));
    let 占位 = 占位活::排上(现场.app.tasks_mut(), "占着任务台");
    let ctx = headless::context();
    let 屏上 = 滚到底(&ctx, |ui| 现场.app.ui(ui));
    assert_eq!(台上的体检(&现场.app), 1, "头一次进库屏没自动排体检");
    assert!(
        屏上.contains("正在体检…"),
        "体检排在台上，概要上没说正在体检：\n{屏上}"
    );

    // 正排着时再按「重新体检」：一次只跑一趟，不排第二趟。
    点一下(&ctx, "重新体检", |ui| 现场.app.ui(ui));
    assert_eq!(台上的体检(&现场.app), 1, "体检还在台上时又排了一趟");

    占位.放行();
    现场.等台上空了();
    let 屏上 = 滚到底(&ctx, |ui| 现场.app.ui(ui));
    assert!(
        屏上.contains("上次体检 09-14 08:00 · 只读检查，只报告、不处理"),
        "体检收场之后标题栏没写上次体检的时刻：\n{屏上}"
    );
    assert_eq!(那一格(&屏上, "重复拷贝").0, "1 组");
    let 体检过几趟 = 现场
        .app
        .tasks()
        .history()
        .iter()
        .filter(|record| record.name == "库体检 · 全部根")
        .count();
    assert_eq!(体检过几趟, 1);

    // 已经有报告了：再画几帧不会再自动排一趟。
    滚到底(&ctx, |ui| 现场.app.ui(ui));
    assert_eq!(台上的体检(&现场.app), 0, "有报告了还自动排体检");
}

/// 一块摆着**两组重复拷贝**的盘：「逆转裁判.gba」三份、每份 8 KiB（留一份可腾出 16 KiB）；「魂斗罗.zip」两份、每份 2 KiB。
fn 有两组重复拷贝的盘() -> TempDir {
    let 盘 = temp_dir("gui-health-dups");
    for (相对, 字节) in [
        ("FC/魂斗罗.zip", zip(2048)),
        ("FC/备份/魂斗罗.zip", zip(2048)),
        ("GBA/汉化/逆转裁判.gba", vec![7_u8; 8192]),
        ("GBA/备份一/逆转裁判.gba", vec![7_u8; 8192]),
        ("GBA/备份二/逆转裁判.gba", vec![7_u8; 8192]),
    ] {
        let 落点 = 盘.path().join(相对);
        std::fs::create_dir_all(落点.parent().expect("有上级目录")).expect("建得出目录");
        std::fs::write(&落点, 字节).expect("写得进");
    }
    盘
}

/// 屏上**先后**画着这几段字：每一段都在，而且照这个次序出现。
fn 先后画着(屏上: &str, 那几段: &[&str]) -> bool {
    let mut 从 = 0;
    for 一段 in 那几段 {
        match 屏上[从..].find(一段) {
            Some(在) => 从 += 在 + 一段.len(),
            None => return false,
        }
    }
    true
}

#[test]
fn 点重复拷贝那一格_明细按可腾出的空间排_展开一组列出全部路径_写明不会自动删除() {
    // 票 27 验收第 3 条：重复拷贝按可腾出的空间排序，每组列得出全部路径，屏上写明不会自动删除；判据照实写同名且同大小
    // （拿主意的人 2026-09-15 答岔路口 3）。明细表四列照稿（岔路口 4）。
    let ctx = headless::context();
    let 盘 = 有两组重复拷贝的盘();
    let mut 现场 = 现场::摆好();
    现场.加根(盘.path(), "主库");
    现场.扫("主库");
    滚到底(&ctx, |ui| 现场.app.ui(ui));

    let 屏上 = 点一下(&ctx, "重复拷贝", |ui| 现场.app.ui(ui));
    assert!(
        屏上.contains("库体检 · 重复拷贝"),
        "点了重复拷贝那一格，没弹出明细：\n{屏上}"
    );
    assert!(
        屏上.contains("同名且同大小") && 屏上.contains("动手前请自己核一眼"),
        "明细没照实写判据：\n{屏上}"
    );
    assert!(
        屏上.contains("绝不自动删除"),
        "明细没写明不会自动删除：\n{屏上}"
    );
    assert!(
        先后画着(&屏上, &["内容", "平台", "份数", "单份大小"]),
        "明细表的四列照稿：\n{屏上}"
    );
    assert!(
        先后画着(
            &屏上,
            &[
                "逆转裁判.gba",
                "GBA",
                "3",
                "8.00 KiB",
                "魂斗罗.zip",
                "FC",
                "2",
                "2.00 KiB"
            ]
        ),
        "明细没按可腾出的空间从大到小排：\n{屏上}"
    );
    assert!(
        !屏上.contains("备份二"),
        "还没展开，一组的路径不该先摊出来：\n{屏上}"
    );

    let 屏上 = 点一下(&ctx, "逆转裁判.gba", |ui| 现场.app.ui(ui));
    // 岔路口 4：展开后的路径照票 09 写「根名 · 相对路径」（拆键在核心库，界面照 `table::root_and_path` 接起来）。
    for 那一份 in [
        "GBA/汉化/逆转裁判.gba",
        "GBA/备份一/逆转裁判.gba",
        "GBA/备份二/逆转裁判.gba",
    ] {
        let 画的 = format!("主库 · {那一份}");
        assert!(
            屏上.lines().any(|line| line == 画的),
            "展开之后「{画的}」那一份不在屏上：\n{屏上}"
        );
    }
    assert!(
        屏上.contains("在文件系统中打开"),
        "展开的路径旁边没有「在文件系统中打开」：\n{屏上}"
    );
}

#[test]
fn 点其余几格_明细里是路径与原因_路径旁边能在文件系统中打开_未纳入管理的目录不画映射下拉() {
    // 票 27 验收第 2 条：每一格点进去有明细——路径、原因、数量，能在文件系统里打开。明细的行与原因出自核心库
    // （`HealthReport::finding_rows`）。岔路口 10：未纳入管理的目录不画「映射到平台」下拉。
    let ctx = headless::context();
    let 盘 = 有体检发现的盘();
    let mut 现场 = 现场::摆好();
    现场.加根(盘.path(), "主库");
    现场.扫("主库");
    滚到底(&ctx, |ui| 现场.app.ui(ui));

    let 屏上 = 点一下(&ctx, "附属文件落单", |ui| 现场.app.ui(ui));
    assert!(
        屏上.contains("库体检 · 附属文件落单"),
        "点了附属文件落单那一格，没弹出明细：\n{屏上}"
    );
    assert!(
        屏上
            .lines()
            .any(|line| line.ends_with("GBA/汉化/火焰之纹章.sav")),
        "明细里没有那份落单的存档：\n{屏上}"
    );
    assert!(
        屏上.contains("存档 · 同一目录里找不到同名的主文件"),
        "明细里没说为什么落单：\n{屏上}"
    );
    assert!(
        屏上.contains("在文件系统中打开"),
        "路径旁边没有「在文件系统中打开」：\n{屏上}"
    );

    点一下(&ctx, "关闭", |ui| 现场.app.ui(ui));
    滚到底(&ctx, |ui| 现场.app.ui(ui));
    let 屏上 = 点一下(&ctx, "未纳入管理的目录", |ui| 现场.app.ui(ui));
    assert!(
        屏上.contains("库体检 · 未纳入管理的目录"),
        "点了未纳入管理的目录那一格，没弹出明细：\n{屏上}"
    );
    assert!(
        屏上.contains("杂物（1 个文件"),
        "明细里没有那个未纳入管理的目录：\n{屏上}"
    );
    assert!(
        屏上.contains("不进识别与刮削"),
        "明细那句说明没照实说照扫照报、只是不进识别与刮削：\n{屏上}"
    );
    assert!(
        !屏上.contains("映射到平台") && !屏上.contains("不纳入"),
        "未纳入管理的目录不画映射下拉：\n{屏上}"
    );
    assert!(
        !屏上.contains("在文件系统中打开"),
        "只有目录名、没有路径的那一行不该有「在文件系统中打开」：\n{屏上}"
    );
}

/// 命令行 `romcat report --dump-duplicates` 会写出去的那一份：从中立库折出统计（每组记全路径）、出报告、折明细。
fn 命令行写出去的重复拷贝明细(site: &Site) -> String {
    let limits = Limits {
        max_duplicate_paths_per_group: Limits::FULL_DUPLICATE_PATHS_PER_GROUP,
        ..Limits::default()
    };
    let aggregate = site
        .catalog
        .aggregate(&limits, &Manifest::builtin())
        .expect("折得出统计");
    let report = HealthReport::build(
        &aggregate,
        &site.catalog.report_meta().expect("元信息读得出来"),
    );
    DuplicateDetails::build(&aggregate, &report).render_text()
}

#[test]
fn 明细导出成清单_重复拷贝与命令行同一份字节_其余几格是核心库的文本明细_落点在主库里就拒并说原因() {
    // 票 27 验收第 4 条「明细能导出成一份清单」。拿主意的人 2026-09-15 答岔路口 5、6：按钮照稿写「导出清单…」，系统保存对话框，
    // 纯文本；重复拷贝那一份与 `romcat report --dump-duplicates` 同样的字节，其余几格由核心库给文本明细；落点在主库里就拒，
    // 屏上说原因。保存对话框本身测不了（同 `tests/pick.rs`）：选中之后那一半递固定路径进去验。
    let ctx = headless::context();
    let 盘 = 有体检发现的盘();
    let mut 现场 = 现场::摆好();
    现场.加根(盘.path(), "主库");
    现场.扫("主库");
    let 放清单的地方 = temp_dir("gui-health-export");

    // ── 重复拷贝：与命令行同一份字节
    滚到底(&ctx, |ui| 现场.app.ui(ui));
    let 屏上 = 点一下(&ctx, "重复拷贝", |ui| 现场.app.ui(ui));
    assert!(
        屏上.contains("导出清单…"),
        "明细弹层上没有「导出清单…」：\n{屏上}"
    );
    let 落点 = 放清单的地方.path().join("重复拷贝.txt");
    {
        let (screen, site, _) = 现场.app.roots_site_and_tasks();
        screen.health_export_picked(site, Some(落点.clone()));
    }
    let 写出去的 = std::fs::read_to_string(&落点).expect("清单写出去了");
    assert_eq!(写出去的, 命令行写出去的重复拷贝明细(现场.app.site()));
    assert!(写出去的.contains("只发现并报告，绝不自动删除"));
    let 屏上 = 滚到底(&ctx, |ui| 现场.app.ui(ui));
    assert!(屏上.contains("清单写到了"), "写完了屏上没说：\n{屏上}");

    // ── 附属文件落单：核心库那一份文本明细
    点一下(&ctx, "关闭", |ui| 现场.app.ui(ui));
    滚到底(&ctx, |ui| 现场.app.ui(ui));
    点一下(&ctx, "附属文件落单", |ui| 现场.app.ui(ui));
    let 落点 = 放清单的地方.path().join("落单.txt");
    {
        let (screen, site, _) = 现场.app.roots_site_and_tasks();
        screen.health_export_picked(site, Some(落点.clone()));
    }
    let 写出去的 = std::fs::read_to_string(&落点).expect("清单写出去了");
    let 核心库那一份 = 现场
        .app
        .roots()
        .health()
        .checked()
        .expect("有报告")
        .report
        .render_finding(romcat_core::report::Finding::StrandedCompanions);
    assert_eq!(写出去的, 核心库那一份);
    assert!(写出去的.contains("火焰之纹章.sav"), "{写出去的}");

    // ── 落点在主库里：拒，一个字节都不写，屏上说原因
    let 主库里 = 盘.path().join("GBA/落单清单.txt");
    {
        let (screen, site, _) = 现场.app.roots_site_and_tasks();
        screen.health_export_picked(site, Some(主库里.clone()));
    }
    assert!(!主库里.exists(), "清单写进了主库");
    let 屏上 = 滚到底(&ctx, |ui| 现场.app.ui(ui));
    assert!(
        屏上.contains("落在主库内"),
        "落点在主库里被拒了，屏上却没说原因：\n{屏上}"
    );
}

#[test]
fn 点疑似同一作品那一格_不跳屏也不开明细_只在屏上说一句_识别跑完之后那一格与那一句都改说实话() {
    // 拿主意的人 2026-09-15 答岔路口 9：识别没跑完照票写「识别完成后才有」，跑完改一句实话；点进去不跳屏，只在屏上说一句
    // （跳到浏览屏「整理建议」由票 17 接上，挂单 `Q957`）。界面里不许自己算一个数（验收第 6 条）。
    let ctx = headless::context();
    let 盘 = 有体检发现的盘();
    let mut 现场 = 现场::摆好();
    现场.加根(盘.path(), "主库");
    现场.扫("主库");
    滚到底(&ctx, |ui| 现场.app.ui(ui));

    let 屏上 = 点一下(&ctx, "疑似同一作品", |ui| 现场.app.ui(ui));
    assert_eq!(
        现场.app.view(),
        View::Library,
        "点了疑似同一作品那一格跳走了"
    );
    assert!(
        !屏上.contains("库体检 · 疑似同一作品"),
        "疑似同一作品那一格不开明细弹层：\n{屏上}"
    );
    assert!(
        屏上.contains("识别完成后才有疑似同一作品的建议"),
        "点了疑似同一作品那一格，屏上没说一句：\n{屏上}"
    );

    // 识别跑完：弹药空着也跑得完（识别认不出什么不要紧，要的是那一道做完了）。
    drop(
        romcat_core::dat::DatRepo::open(&romcat_core::workspace::dat_repo_path(现场.工作区.path()))
            .expect("开得出 DAT 库"),
    );
    现场.app.start_stage(romcat_core::stage::Stage::Identify);
    现场.等台上空了();
    let 屏上 = 滚到底(&ctx, |ui| 现场.app.ui(ui));
    assert_eq!(
        那一格(&屏上, "疑似同一作品"),
        ("—".to_string(), "暂时还给不出这一项".to_string()),
        "识别跑完之后那一格还说识别完成后才有"
    );
    let 屏上 = 点一下(&ctx, "疑似同一作品", |ui| 现场.app.ui(ui));
    assert!(
        屏上.contains("暂时还给不出疑似同一作品的建议"),
        "识别跑完之后点那一格，屏上没改说实话：\n{屏上}"
    );
}

/// 一块摆着**十二份落单存档**的盘：`GBA/汉化/落单NN.sav`，同目录里一份主文件都没有。扫描那一趟每类只留十个样例。
fn 有十二份落单存档的盘() -> TempDir {
    let 盘 = temp_dir("gui-health-stranded");
    std::fs::write(盘.path().join("占位.txt"), b"x").expect("写得进");
    for n in 0..12 {
        let 落点 = 盘.path().join(format!("GBA/汉化/落单{n:02}.sav"));
        std::fs::create_dir_all(落点.parent().expect("有上级目录")).expect("建得出目录");
        std::fs::write(&落点, vec![9_u8; 64]).expect("写得进");
    }
    盘
}

#[test]
fn 扫描交回的报告每类只留十个样例_重新体检那一趟列全_导出的清单也全() {
    // 拿主意的人 2026-09-15 答岔路口 2（挂单 `Q959`）：只放开「重新体检」那一趟，其余几格列全、导出也全；底下「另有 N 个」照实写。
    let ctx = headless::context();
    let 盘 = 有十二份落单存档的盘();
    let mut 现场 = 现场::摆好();
    现场.加根(盘.path(), "主库");
    现场.扫("主库");
    let 放清单的地方 = temp_dir("gui-health-full");

    // ── 扫描交回的那一份：十个样例，另有两个没列出
    滚到底(&ctx, |ui| 现场.app.ui(ui));
    点一下(&ctx, "附属文件落单", |ui| 现场.app.ui(ui));
    let 落点 = 放清单的地方.path().join("扫描那一份.txt");
    {
        let (screen, site, _) = 现场.app.roots_site_and_tasks();
        screen.health_export_picked(site, Some(落点.clone()));
    }
    let 写出去的 = std::fs::read_to_string(&落点).expect("清单写出去了");
    assert!(写出去的.contains("另有 2 个没列出"), "{写出去的}");
    let 屏上 = 滚到底(&ctx, |ui| 现场.app.ui(ui));
    assert!(
        屏上.contains("另有 2 个"),
        "样例截断了，弹层底下没照实说另有几个：\n{屏上}"
    );
    点一下(&ctx, "关闭", |ui| 现场.app.ui(ui));

    // ── 重新体检那一趟：列全
    {
        let (screen, site, tasks) = 现场.app.roots_site_and_tasks();
        screen.check_health(site, tasks);
    }
    现场.等台上空了();
    滚到底(&ctx, |ui| 现场.app.ui(ui));
    点一下(&ctx, "附属文件落单", |ui| 现场.app.ui(ui));
    let 落点 = 放清单的地方.path().join("重新体检那一份.txt");
    {
        let (screen, site, _) = 现场.app.roots_site_and_tasks();
        screen.health_export_picked(site, Some(落点.clone()));
    }
    let 写出去的 = std::fs::read_to_string(&落点).expect("清单写出去了");
    for n in 0..12 {
        assert!(
            写出去的.contains(&format!("落单{n:02}.sav")),
            "重新体检之后清单里少了第 {n} 份：\n{写出去的}"
        );
    }
    assert!(!写出去的.contains("另有"), "列全了就不说另有：\n{写出去的}");
    let 屏上 = 跑一帧不动(&ctx, &mut 现场);
    assert!(
        !屏上.contains("另有 2 个"),
        "列全了弹层底下还说另有：\n{屏上}"
    );
}

/// 不带事件跑两帧，交出第二帧画出来的字（弹层里的虚拟化列表头一帧在量行高）。
fn 跑一帧不动(ctx: &egui::Context, 现场: &mut 现场) -> String {
    headless::frame(ctx, headless::input(), |ui| 现场.app.ui(ui));
    shared::画出来的字(&headless::frame(ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }))
}

#[test]
fn 点目录与内容平台不符那一格_开的是平台纠正那一层_按组列出从哪到哪几条凭什么两条样例与两条出路() {
    // 票 28 验收第 1、2 条：**按组列出**（从什么平台 → 到什么平台、多少条、理由、样例），
    // **每组两条出路**。设计稿 `HEALTH` 里这一格点进去开的正是 `DLG.platfix`，不是明细弹层。
    let ctx = headless::context();
    let 盘 = 有平台不符的盘();
    let mut 现场 = 摆好平台不符的现场(&ctx, &盘);

    let 屏上 = 点一下(&ctx, "目录与内容平台不符", |ui| 现场.app.ui(ui));
    assert!(
        有一行正好是(&屏上, "平台纠正"),
        "点了那一格，开的不是平台纠正那一层：\n{屏上}"
    );
    assert!(
        !屏上.contains("库体检 · 目录与内容平台不符"),
        "这一格不该退回明细弹层：\n{屏上}"
    );
    // 一组：从哪个平台到哪个平台、叫什么、多少条。条数多的那一组在前（核心库排的）。
    assert!(
        屏上.contains("FC 目录里的 FDS 游戏"),
        "没按组说清从哪个平台到哪个平台：\n{屏上}"
    );
    assert!(有一行正好是(&屏上, "2 条"), "没说这一组多少条：\n{屏上}");
    assert!(
        屏上.contains(".fds") && 屏上.contains("只可能属于 FDS"),
        "没说凭什么这么判：\n{屏上}"
    );
    // 两条样例，第三条没有就不写「另有」。
    assert!(
        屏上.lines().any(|line| line.ends_with("塞尔达传说.fds"))
            && 屏上.lines().any(|line| line.ends_with("银河战士.fds")),
        "这一组的两条样例没列出来：\n{屏上}"
    );
    // 两条出路。
    assert!(
        有一行正好是(&屏上, "按内容改为 FDS（2 条）") && 有一行正好是(&屏上, "保持 FC"),
        "一组没给出「按内容改」与「保持目录的说法」两条出路：\n{屏上}"
    );
    // 改不改都行的那一类要说明白（票面那条 ⚠️）。
    assert!(
        屏上.contains("3DS 能运行 NDS 的游戏，保持也不影响游玩"),
        "3DS 向下兼容 NDS，那一组没说「改不改都行」：\n{屏上}"
    );
    assert!(
        屏上.contains("不移动任何文件"),
        "那一层没说清纠正不移动文件（ADR-0004）：\n{屏上}"
    );
    // **屏上说「记为人工纠正」而不是「记为裁决」**（拿主意的人 2026-09-21 裁，挂单 `Q1032`）：
    // 落下来的是 `platform_correction` 那张表，与**成型的人工纠正**同族，撤销**不走裁决记录**。
    assert!(
        屏上.contains("记为人工纠正") && !屏上.contains("记为裁决"),
        "屏上该说「记为人工纠正」，它走的不是裁决记录：\n{屏上}"
    );
    assert!(
        屏上.contains("改了平台的变体会在下次识别时按新平台重新匹配"),
        "那一层末尾没说下一趟识别会重新匹配：\n{屏上}"
    );
    // **屏上不许许一句做不到的话**：导出与同步眼下不读纠正（挂单 `Q1033`），
    // 稿上那句「导出和同步时按纠正后的平台放置」因此改成实话（收尾审查 Spec 轴第 1 条）。
    assert!(
        屏上.contains("导出与同步眼下仍按目录放置"),
        "那一层没说清导出与同步还没跟上：\n{屏上}"
    );
    assert!(
        !屏上.contains("导出和同步时按纠正后的平台放置"),
        "屏上不许许一句做不到的话：\n{屏上}"
    );
}

#[test]
fn 按内容改之后那一组摊着说已改为_那一格的数跟着降_撤销回得去() {
    // 票 28 验收第 3、6 条：纠正**可撤销**，处理过的组**在库体检概要里不再计数**。
    let ctx = headless::context();
    let 盘 = 有平台不符的盘();
    let mut 现场 = 摆好平台不符的现场(&ctx, &盘);
    assert_eq!(
        那一格(&滚到底(&ctx, |ui| 现场.app.ui(ui)), "目录与内容平台不符").0,
        "3 条"
    );

    点一下(&ctx, "目录与内容平台不符", |ui| 现场.app.ui(ui));
    let 屏上 = 点一下(&ctx, "按内容改为 FDS（2 条）", |ui| 现场.app.ui(ui));
    assert!(
        屏上.contains("已改为 FDS"),
        "定完那一组没摊着说「已改为 FDS」：\n{屏上}"
    );
    assert!(
        屏上.contains("记为人工纠正") && 屏上.contains("一个字节都没动"),
        "定完那一句回话没说清记成了什么、盘上动没动：\n{屏上}"
    );
    assert!(
        !有一行正好是(&屏上, "按内容改为 FDS（2 条）"),
        "定过的那一组不该还摆着两颗按钮：\n{屏上}"
    );
    assert!(
        有一行正好是(&屏上, "撤销"),
        "定过的那一组没有「撤销」：\n{屏上}"
    );

    点一下(&ctx, "完成", |ui| 现场.app.ui(ui));
    let 屏上 = 滚到底(&ctx, |ui| 现场.app.ui(ui));
    assert_eq!(
        那一格(&屏上, "目录与内容平台不符"),
        ("1 条".to_string(), "已处理 1 组".to_string()),
        "处理过的那一组还算在概要里：\n{屏上}"
    );

    // 撤销：那一组回到还没处理，概要重新数它。
    点一下(&ctx, "目录与内容平台不符", |ui| 现场.app.ui(ui));
    let 屏上 = 点一下(&ctx, "撤销", |ui| 现场.app.ui(ui));
    assert!(
        有一行正好是(&屏上, "按内容改为 FDS（2 条）"),
        "撤销之后那一组没回到还没处理：\n{屏上}"
    );
    点一下(&ctx, "完成", |ui| 现场.app.ui(ui));
    let 屏上 = 滚到底(&ctx, |ui| 现场.app.ui(ui));
    assert_eq!(那一格(&屏上, "目录与内容平台不符").0, "3 条");
    assert!(
        !屏上.contains("已处理"),
        "撤销之后小字还写着已处理：\n{屏上}"
    );
}

#[test]
fn 保持目录的说法也记得住_那一组不再问第二遍() {
    // 票 28 验收第 2 条后半句「两种都记得住」：「保持」不记下来的话，每体检一趟就要再问一遍。
    let ctx = headless::context();
    let 盘 = 有平台不符的盘();
    let mut 现场 = 摆好平台不符的现场(&ctx, &盘);

    点一下(&ctx, "目录与内容平台不符", |ui| 现场.app.ui(ui));
    let 屏上 = 点一下(&ctx, "保持 FC", |ui| 现场.app.ui(ui));
    assert!(
        屏上.contains("已保持 FC"),
        "定完那一组没摊着说「已保持 FC」：\n{屏上}"
    );
    assert!(
        屏上.contains("不再提示"),
        "那一句回话没说清这一组不再问第二遍：\n{屏上}"
    );

    点一下(&ctx, "完成", |ui| 现场.app.ui(ui));
    let 屏上 = 滚到底(&ctx, |ui| 现场.app.ui(ui));
    assert_eq!(
        那一格(&屏上, "目录与内容平台不符"),
        ("1 条".to_string(), "已处理 1 组".to_string())
    );
}

#[test]
fn 纠正一整轮_盘上的文件一个字节都没动() {
    // **主库只读**（ADR-0004）：这一层只往沉淀库写一条决定。目标目录整份快照前后逐字节相同
    // ——照票 26「移除一个根不动盘上的任何一个字节」那条的形状写。
    let ctx = headless::context();
    let 盘 = 有平台不符的盘();
    let mut 现场 = 摆好平台不符的现场(&ctx, &盘);
    let 之前 = 盘上快照(盘.path());

    点一下(&ctx, "目录与内容平台不符", |ui| 现场.app.ui(ui));
    点一下(&ctx, "按内容改为 FDS（2 条）", |ui| 现场.app.ui(ui));
    点一下(&ctx, "保持 3DS", |ui| 现场.app.ui(ui));
    点一下(&ctx, "撤销", |ui| 现场.app.ui(ui));
    点一下(&ctx, "完成", |ui| 现场.app.ui(ui));

    assert_eq!(
        之前,
        盘上快照(盘.path()),
        "纠正不移动、不改名、不改写主库里的任何一个文件"
    );
}

/// 一个目录整份的样子：每个文件的相对路径 → 它的字节。
fn 盘上快照(dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut out = std::collections::BTreeMap::new();
    let mut 待走 = vec![dir.to_path_buf()];
    while let Some(这一层) = 待走.pop() {
        for 项 in std::fs::read_dir(&这一层).expect("读得动目录") {
            let 路 = 项.expect("读得动一项").path();
            if 路.is_dir() {
                待走.push(路);
            } else {
                let 相对 = 路
                    .strip_prefix(dir)
                    .expect("在这个目录下面")
                    .to_string_lossy()
                    .into_owned();
                out.insert(相对, std::fs::read(&路).expect("读得动"));
            }
        }
    }
    out
}

// ═══════════════════════════════════════════════════════════════════════════
// 成型纠正（票 `gui-looks-like-the-design/29`）
// ═══════════════════════════════════════════════════════════════════════════

/// 一块只往临时目录里写的「盘」，专为**成型纠正**摆的：两种成型存疑各一处。**一个字节都不碰真盘。**
///
/// - `FDS/某游戏/` 与 `FDS/某游戏乙/` 底下各两面磁碟：FDS 没声明多碟同族，两面各成一个变体——
///   **多碟没合在一起**，而且**同结构的有两处**（逐处确认，不一次性全改）。
/// - `ps3/动作合集/` 是一棵目录树（`PS3_GAME` 是锚），可它**直接**躺着两份各自独立的内容——
///   **一个目录被当成一个变体**。
fn 有成型存疑的盘() -> TempDir {
    let 盘 = temp_dir("gui-shaping-disk");
    for (相对, 字节) in [
        ("FDS/某游戏/某游戏 (Disk 1).fds", vec![1_u8; 64]),
        ("FDS/某游戏/某游戏 (Disk 2).fds", vec![2_u8; 64]),
        // **同结构的另一处**：验收第 6 条要的是「同样的另几处列在库体检里逐处确认」。
        ("FDS/某游戏乙/某游戏乙 (Disk 1).fds", vec![6_u8; 64]),
        ("FDS/某游戏乙/某游戏乙 (Disk 2).fds", vec![7_u8; 64]),
        ("ps3/动作合集/PS3_GAME/USRDIR/EBOOT.BIN", vec![3_u8; 64]),
        ("ps3/动作合集/甲.iso", vec![4_u8; 2048]),
        ("ps3/动作合集/乙.7z", vec![5_u8; 2048]),
    ] {
        let 落点 = 盘.path().join(相对);
        std::fs::create_dir_all(落点.parent().expect("有上级目录")).expect("建得出目录");
        std::fs::write(&落点, 字节).expect("写得进");
    }
    盘
}

/// 摆好一份有成型存疑的库，滚到库屏底下，交出现场。
fn 摆好成型存疑的现场(ctx: &egui::Context, 盘: &TempDir) -> 现场 {
    let mut 现场 = 现场::摆好();
    现场.加根(盘.path(), "主库");
    现场.扫("主库");
    滚到底(ctx, |ui| 现场.app.ui(ui));
    现场
}

/// 库里眼下有哪几个变体：（键, 是不是人工纠正出来的），按键排。
fn 库里的变体(现场: &mut 现场) -> Vec<(String, bool)> {
    let (_, site, _) = 现场.app.roots_site_and_tasks();
    let mut out: Vec<(String, bool)> = site
        .catalog
        .variants()
        .expect("读得出变体")
        .into_iter()
        .map(|row| (row.key, row.manual))
        .collect();
    out.sort();
    out
}

#[test]
fn 点成型存疑那一格_明细一行一颗处理_开的是调整成型那一层_说清现在怎样与纠正后怎样() {
    // 票 29 验收第 1 条前半句（**从库体检的「成型存疑」进得去**）与第 2 条中段
    // （**预览合成后的主文件与附属文件**）。
    let ctx = headless::context();
    let 盘 = 有成型存疑的盘();
    let mut 现场 = 摆好成型存疑的现场(&ctx, &盘);
    assert_eq!(
        那一格(&滚到底(&ctx, |ui| 现场.app.ui(ui)), "成型存疑").0,
        "3 处"
    );

    let 屏上 = 点一下(&ctx, "成型存疑", |ui| 现场.app.ui(ui));
    assert!(
        有一行正好是(&屏上, "库体检 · 成型存疑"),
        "点了那一格没开明细：\n{屏上}"
    );
    assert!(
        屏上.contains("处理…"),
        "成型存疑那一格的明细里没有进得去的门：\n{屏上}"
    );
    // **判据那一句仍旧出自核心库**，后面接的政策话改成了实话：这一格点得下去。
    assert!(
        屏上.contains("这一格点得进去：记下的是人工纠正") && 屏上.contains("一个字节都不动"),
        "那一层没说清按下去会发生什么、盘上动不动：\n{屏上}"
    );

    let 屏上 = 点一下(&ctx, "处理…", |ui| 现场.app.ui(ui));
    assert!(
        有一行正好是(&屏上, "调整成型 · 某游戏"),
        "「处理…」开的不是调整成型那一层：\n{屏上}"
    );
    assert!(
        屏上.contains("现在：这 2 个变体各自独立"),
        "没说清这一处现在是什么样：\n{屏上}"
    );
    assert!(
        屏上.contains("2 个变体只差碟片标记"),
        "没说凭什么判这一处存疑（核心库那一句）：\n{屏上}"
    );
    // **预览合成后的主文件与附属文件**（验收第 2 条）。
    assert!(
        屏上.contains("主文件：某游戏 (Disk 1).fds") && 屏上.contains("附属文件 1 个"),
        "没预览合成之后的主文件与附属文件：\n{屏上}"
    );
    assert!(
        有一行正好是(&屏上, "合成一个变体"),
        "那一层页脚上没有「合成一个变体」：\n{屏上}"
    );
    // **屏上说「记为人工纠正」而不是「记为裁决」**（票 28 那条先例，挂单 `Q1032`）。
    assert!(
        屏上.contains("记为人工纠正") && !屏上.contains("记为裁决"),
        "屏上该说「记为人工纠正」，它落的是自己那张表、撤销不走裁决记录：\n{屏上}"
    );
    // **屏上不许许一句做不到的话**：稿上那句「按文件内容永久保留」不真——那张表的键是路径。
    assert!(
        屏上.contains("按路径永久记住") && !屏上.contains("按文件内容永久保留"),
        "纠正锚在路径上，屏上不许说成锚在内容上：\n{屏上}"
    );
    // **逐处确认，不一次性全改**（验收第 6 条）。
    assert!(
        屏上.contains("同样是「多碟没合在一起」的另有 1 处")
            && 屏上.contains("列在库体检的「成型存疑」里"),
        "没说清同结构的其余几处在哪儿逐处确认：\n{屏上}"
    );
}

#[test]
fn 合成一处多碟_那两个变体并成一个_成型存疑那一格跟着降_盘上一个字节都没动() {
    // 票 29 验收第 2 条（**合成后变体数跟着变**）与「主库只读」（ADR-0004）。
    let ctx = headless::context();
    let 盘 = 有成型存疑的盘();
    let mut 现场 = 摆好成型存疑的现场(&ctx, &盘);
    let 之前 = 盘上快照(盘.path());
    assert_eq!(
        库里的变体(&mut 现场)
            .iter()
            .filter(|(key, _)| key.contains("FDS/某游戏/"))
            .count(),
        2,
        "合之前那两面磁碟该是两个变体"
    );

    点一下(&ctx, "成型存疑", |ui| 现场.app.ui(ui));
    点一下(&ctx, "处理…", |ui| 现场.app.ui(ui));
    let 屏上 = 点一下(&ctx, "合成一个变体", |ui| 现场.app.ui(ui));
    assert!(
        屏上.contains("已把 2 个变体合成一个（记为人工纠正）"),
        "按完没说清落成了什么：\n{屏上}"
    );
    现场.等台上空了();
    let 屏上 = 跑一帧不动(&ctx, &mut 现场);

    let 那几个: Vec<(String, bool)> = 库里的变体(&mut 现场)
        .into_iter()
        .filter(|(key, _)| key.contains("FDS/某游戏/"))
        .collect();
    assert_eq!(
        那几个,
        vec![("主库/FDS/某游戏/某游戏 (Disk 1).fds".to_string(), true)],
        "两面磁碟没合成一个人工纠正出来的变体：\n{屏上}"
    );
    点一下(&ctx, "关闭", |ui| 现场.app.ui(ui));
    let 屏上 = 滚到底(&ctx, |ui| 现场.app.ui(ui));
    assert_eq!(
        那一格(&屏上, "成型存疑").0,
        "2 处",
        "纠正过的那一处还算在概要里：\n{屏上}"
    );
    assert_eq!(
        之前,
        盘上快照(盘.path()),
        "成型纠正不移动、不改名、不改写主库里的任何一个文件"
    );
}

#[test]
fn 一个目录被当成一个变体那一处_拆成几个变体_拆出来的各自在库里() {
    // 票 29 验收第 3 条：**目录拆成多个变体，拆开后各自参与下一趟识别**——「各自参与识别」
    // 的前提就是它们各自是一个变体（识别按变体跑，`identify::run`）。
    let ctx = headless::context();
    let 盘 = 有成型存疑的盘();
    let mut 现场 = 摆好成型存疑的现场(&ctx, &盘);
    assert!(
        库里的变体(&mut 现场)
            .iter()
            .any(|(key, _)| key == "主库/ps3/动作合集"),
        "拆之前整个目录该是一个变体"
    );

    点一下(&ctx, "成型存疑", |ui| 现场.app.ui(ui));
    // 两处各一颗「处理…」，这一处排在后面：点的是**最后**那一颗。
    let 屏上 = 点最后一个(&ctx, &mut 现场, "处理…");
    assert!(
        有一行正好是(&屏上, "调整成型 · 动作合集"),
        "开的不是那个目录那一处：\n{屏上}"
    );
    assert!(
        屏上.contains("现在：整个目录被当成 1 个变体") && 屏上.contains("甲.iso"),
        "没说清这个目录里头有哪几份：\n{屏上}"
    );
    assert!(
        屏上.contains("拆成 2 个变体，每份内容一个") && 屏上.contains("拆开后各自参与下一趟识别"),
        "没说清拆完会怎样：\n{屏上}"
    );

    // 「拆成 2 个变体」屏上有两处：那一档单选的名字与页脚那颗按钮；页脚那颗最后画。
    let 屏上 = 点最后一个(&ctx, &mut 现场, "拆成 2 个变体");
    assert!(
        屏上.contains("已拆成 2 个变体（记为人工纠正）"),
        "按完没说清落成了什么：\n{屏上}"
    );
    现场.等台上空了();
    跑一帧不动(&ctx, &mut 现场);

    let 那几个: Vec<(String, bool)> = 库里的变体(&mut 现场)
        .into_iter()
        .filter(|(key, _)| key.starts_with("主库/ps3/"))
        .collect();
    assert_eq!(
        那几个,
        vec![
            ("主库/ps3/动作合集".to_string(), false),
            ("主库/ps3/动作合集/乙.7z".to_string(), true),
            ("主库/ps3/动作合集/甲.iso".to_string(), true),
        ],
        "那两份独立内容没各自成变体（剩下的那份转储该照旧是一个变体）"
    );
}

/// 点屏上**最后**一处写着这几个字的地方（同一句话屏上有好几处时用它）。
fn 点最后一个(ctx: &egui::Context, 现场: &mut 现场, 那一段: &str) -> String {
    let 头一帧 = headless::frame(ctx, headless::input(), |ui| 现场.app.ui(ui));
    let 位置 =
        最后一处画在哪儿(&头一帧, 那一段).unwrap_or_else(|| panic!("屏上没有「{那一段}」，没处点"));
    let 按 = |pressed: bool| egui::Event::PointerButton {
        pos: 位置,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut input = headless::input();
    input.events.push(egui::Event::PointerMoved(位置));
    input.events.push(按(true));
    headless::frame(ctx, input, |ui| 现场.app.ui(ui));
    let mut input = headless::input();
    input.events.push(按(false));
    headless::frame(ctx, input, |ui| 现场.app.ui(ui));
    跑一帧不动(ctx, 现场)
}

/// 屏上**最后**一处含着这几个字的地方画在哪儿（中心点）。
fn 最后一处画在哪儿(output: &egui::FullOutput, 那一段: &str) -> Option<egui::Pos2> {
    fn 找(shape: &egui::epaint::Shape, 那一段: &str, out: &mut Option<egui::Pos2>) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                if text.galley.text().contains(那一段) {
                    *out = Some(egui::Rect::from_min_size(text.pos, text.galley.size()).center());
                }
            }
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    找(one, 那一段, out);
                }
            }
            _ => {}
        }
    }
    let mut out = None;
    for clipped in &output.shapes {
        找(&clipped.shape, 那一段, &mut out);
    }
    out
}
