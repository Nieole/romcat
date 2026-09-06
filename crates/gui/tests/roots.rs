//! **库**那一屏：一组根加得进、扫得完、移得掉，数据源的账看得见。
//!
//! 这几条都是「不这么做会出事」而不是「这样比较好看」：
//!
//! - **两个根的变体互不覆盖**：只按相对路径当键的话，两块盘上同名的东西会静默
//!   覆盖成一条，而中立库是事实来源（ADR-0001）。
//! - **新根不许与已有的根、与工作目录套在一起**：前者让同一批文件被数两遍，
//!   后者让中立库被圈进主库，而**主库只读**（ADR-0004）。
//! - **移除一个根之前得说清会去掉多少变体**：那是工具唯一会主动丢掉扫描结果的地方。
//! - **盘没挂上时上次结果仍然看得见**：它住在中立库里，不跟着盘走（ADR-0009）。
//! - **还没取回的数据源要被明确标出来**：新用户卡在「扫完了怎么没认出来」时，
//!   答案就在这一行上。
//!
//! 主库**一律拿本地 fixture 目录模拟**：绝不去动任何真实设备或 SD 卡。

use std::fs;
use std::path::Path;
use std::time::Duration;

use romcat_core::catalog::Catalog;
use romcat_core::site::Site;
use romcat_core::sources::SourceState;
use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};
use romcat_gui::app::{App, View};
use romcat_gui::headless;

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

/// 一份小 fixture 主库。**只读**——这几行只往临时目录里写，一个字节都不碰真库。
fn 建库(tag: &str) -> TempDir {
    let dir = temp_dir(tag);
    写(&dir.path().join("SFC/幻想传说 汉化版.zip"), &zip(4096));
    写(&dir.path().join("FC/魂斗罗.zip"), &zip(2048));
    dir
}

struct 现场 {
    _工作区: TempDir,
    app: App,
}

impl 现场 {
    /// **中立库落在磁盘上**，不是只活在内存里：扫描跑在任务台上，后台那条线程按文件
    /// 路径自己开一份写得动的库。真库本来就是这个样子，fixture 照着摆才验得到那条路。
    fn 摆好() -> Self {
        let 工作区 = temp_dir("gui-roots-ws");
        let 库文件 = 工作区.path().join("catalog").join("fixture.sqlite3");
        drop(Catalog::open(&库文件).expect("能开中立库"));
        let site = Site::open_file(工作区.path(), &库文件, None).expect("开得出现场");
        let mut app = App::new(site, 工作区.path().to_path_buf());
        app.show_view(View::Library);
        Self {
            _工作区: 工作区,
            app,
        }
    }

    fn 加根(&mut self, 目录: &Path, 名字: &str) {
        let (screen, site, _) = self.app.roots_site_and_tasks();
        screen.add_root(site, &目录.to_string_lossy(), 名字);
    }

    fn 扫(&mut self, 名字: &str) {
        let (screen, site, tasks) = self.app.roots_site_and_tasks();
        screen.scan(site, tasks, 名字);
        self.等任务跑完();
    }

    fn 等任务跑完(&mut self) {
        for _ in 0..600 {
            self.app.poll_tasks();
            if !self.app.tasks().busy() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("这一趟活迟迟不结束");
    }
}

fn 跑一帧(ctx: &egui::Context, app: &mut App) {
    headless::frame(ctx, headless::input(), |ui| app.ui(ui));
}

#[test]
fn 两个根扫进同一份中立库而且互不覆盖() {
    let 甲 = 建库("gui-roots-甲");
    let 乙 = 建库("gui-roots-乙");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();

    现场.加根(甲.path(), "主库");
    现场.加根(乙.path(), "元数据库");
    assert_eq!(现场.app.roots().roots().len(), 2, "两个根都加上了");
    assert!(现场.app.roots().error().is_none());
    跑一帧(&ctx, &mut 现场.app);

    现场.扫("主库");
    现场.扫("元数据库");
    跑一帧(&ctx, &mut 现场.app);

    // 两块盘上**同名同大小**的东西：只按相对路径当键的话它们会是同一条记录。
    for 根 in ["主库", "元数据库"] {
        assert!(
            现场
                .app
                .site()
                .catalog
                .contains(&format!("{根}/FC/魂斗罗.zip"))
                .expect("查得到"),
            "{根} 那一支得独立存在"
        );
    }
    let rows = 现场.app.roots().roots();
    assert!(
        rows.iter().all(|row| row.stats.variants > 0),
        "两个根都扫出了变体"
    );
    assert!(
        rows.iter().all(|row| row.root.scan.is_some()),
        "两个根各自记下了上次扫描的结果"
    );
}

#[test]
fn 新根落在已有根内部时被拒绝并说清为什么() {
    let 库 = 建库("gui-roots-嵌套");
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");

    现场.加根(&库.path().join("FC"), "子集");
    let 错 = 现场.app.roots().error().expect("该被拒");
    assert!(错.contains("主库"), "得说清撞上的是哪一个：{错}");
    assert!(错.contains("数两遍"), "得说清为什么：{错}");
    assert_eq!(现场.app.roots().roots().len(), 1, "只加上了一个根");
}

#[test]
fn 新根落在工作目录里时被拒绝() {
    // **中立库不许被圈进主库**（ADR-0004）：它、断点与媒体池都写在工作目录里。
    let mut 现场 = 现场::摆好();
    let 工作区 = 现场._工作区.path().to_path_buf();
    let 手滑 = 工作区.join("roms");
    fs::create_dir_all(&手滑).expect("建得出");

    现场.加根(&手滑, "手滑");
    let 错 = 现场.app.roots().error().expect("该被拒");
    assert!(错.contains("ADR-0004"), "得点名那条纪律：{错}");
    assert!(现场.app.roots().roots().is_empty());
}

#[test]
fn 移除一个根时说得出会去掉多少变体_而且只去掉它自己那一支() {
    let 甲 = 建库("gui-roots-移甲");
    let 乙 = 建库("gui-roots-移乙");
    let mut 现场 = 现场::摆好();
    现场.加根(甲.path(), "主库");
    现场.加根(乙.path(), "元数据库");
    现场.扫("主库");
    现场.扫("元数据库");

    let 会去掉 = 现场
        .app
        .roots()
        .roots()
        .iter()
        .find(|row| row.root.name == "元数据库")
        .expect("有这个根")
        .stats
        .variants;
    assert!(会去掉 > 0);

    {
        let (screen, site, _) = 现场.app.roots_site_and_tasks();
        screen.remove_root(site, "元数据库");
    }
    let 话 = 现场.app.roots().notice().expect("该说一句");
    assert!(
        话.contains(&会去掉.to_string()),
        "按下去之前看见的那个数，就是它该说的那个数：{话}"
    );
    assert!(话.contains("沉淀库"), "得说清沉淀库没被动：{话}");
    assert_eq!(现场.app.roots().roots().len(), 1);
    assert!(
        现场
            .app
            .site()
            .catalog
            .contains("主库/FC/魂斗罗.zip")
            .expect("查得到"),
        "另一个根一条都不许少"
    );
}

#[test]
fn 盘没挂上时这个根的上次结果仍然看得见() {
    let 库 = 建库("gui-roots-拔盘");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    let 扫出来的 = 现场.app.roots().roots()[0].stats;
    assert!(扫出来的.variants > 0);

    // 盘拔了：目录整个没了。
    drop(库);
    {
        let (screen, site, _) = 现场.app.roots_site_and_tasks();
        screen.reload(site);
    }
    跑一帧(&ctx, &mut 现场.app);

    let row = &现场.app.roots().roots()[0];
    assert!(!row.mounted, "那块盘确实不在位了");
    assert_eq!(row.stats, 扫出来的, "上次扫出来的账照样在——它住在中立库里");
    assert!(
        row.root.scan.is_some(),
        "上次什么时候扫的、扫了多久，都还看得见"
    );

    // 不在位就别去读盘，直说。
    {
        let (screen, site, tasks) = 现场.app.roots_site_and_tasks();
        screen.scan(site, tasks, "主库");
    }
    let 错 = 现场.app.roots().error().expect("该直说");
    assert!(错.contains("不在位"), "{错}");
}

#[test]
fn 加根与扫描一个字节都不写主库_也不_touch_时间戳() {
    // **主库只读**（ADR-0004）。这一条不是靠纪律，是靠接缝：一切磁盘接触走
    // `LibraryFs`，那个 trait 上没有写的办法。这里从外面直接量一遍。
    let 库 = 建库("gui-roots-只读");
    let 之前 = 目录快照(库.path());
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    assert!(
        现场.app.roots().roots()[0].stats.variants > 0,
        "确实扫出了东西"
    );

    let 之后 = 目录快照(库.path());
    assert_eq!(之前, 之后, "主库里一个字节、一个时间戳都不许变");
}

/// 一棵子树里每个文件的 `(相对路径, 大小, 修改时间)`。
fn 目录快照(root: &Path) -> Vec<(String, u64, std::time::SystemTime)> {
    let mut out = Vec::new();
    走一遍(root, root, &mut out);
    out.sort();
    out
}

fn 走一遍(root: &Path, at: &Path, out: &mut Vec<(String, u64, std::time::SystemTime)>) {
    for entry in fs::read_dir(at).expect("列得开") {
        let entry = entry.expect("读得到");
        let path = entry.path();
        let meta = fs::metadata(&path).expect("读得到元数据");
        if meta.is_dir() {
            走一遍(root, &path, out);
        } else {
            out.push((
                path.strip_prefix(root)
                    .expect("在根下面")
                    .to_string_lossy()
                    .into_owned(),
                meta.len(),
                meta.modified().expect("有修改时间"),
            ));
        }
    }
}

#[test]
fn 还没取回的数据源被明确标出来() {
    // 新用户最容易卡的「扫完了怎么没认出来」，答案就在这一行上。
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    跑一帧(&ctx, &mut 现场.app);

    let sources = 现场.app.roots().sources();
    assert_eq!(sources.len(), 3, "三个源一个都不少");
    for status in sources {
        assert_eq!(
            status.state,
            SourceState::Missing,
            "{} 该明说还没取回",
            status.name
        );
        assert!(
            !status.cost.is_empty(),
            "{} 得说清没取回的代价",
            status.name
        );
    }
}

#[test]
fn 库屏与别的屏切得动而且台上有活时别的屏照常画() {
    let 库 = 建库("gui-roots-切屏");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");

    for view in View::ALL {
        现场.app.show_view(view);
        跑一帧(&ctx, &mut 现场.app);
        assert_eq!(现场.app.view(), view);
    }
    现场.app.show_view(View::Library);
    跑一帧(&ctx, &mut 现场.app);

    // 扫一趟：跑完之后任务台上留下一条历史，那一趟的耗时记在里头。
    现场.扫("主库");
    跑一帧(&ctx, &mut 现场.app);
    let history = 现场.app.tasks().history();
    assert_eq!(history.len(), 1, "扫描进了任务台的历史");
    assert!(history[0].name.contains("扫描"), "{}", history[0].name);
}
