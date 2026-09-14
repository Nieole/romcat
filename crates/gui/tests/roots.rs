//! **库**那一屏：一组根加得进、扫得完、移得掉，数据源的账看得见，**工序**那一段说得出还差什么。
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
//! - **工序那一段说的是「还差多少」而不是「上次几点跑的」**：加了一块盘重扫之后那个
//!   数自己就涨上去，人不必再自己推理「是不是该重跑识别了」——时间戳答不了这个问题。
//! - **那个数与待确认队列屏上的是同一个**：造第二份的话，同一份库在两屏上会报出
//!   两个数。
//! - **识别跑完之后队列自己重新列过**：不然人得再点一次「重新列队列」，而那正是
//!   这一票要消掉的那种「还得记住下一步」。
//! - **折标题跑完之后浏览屏上的显示标题跟着更新**：那是这道工序起没起作用**唯一看得见
//!   的地方**。
//! - **折标题被按停时中立库一个字节都没动**：重折是「清掉再写回」，停在那中间等于把
//!   整份**标题集合**丢掉——所以它压根不停在那儿。
//! - **刮削那一行的口径画在屏上**：那个数数的是一条刮削结论都没有的变体，一眼看去却像
//!   「按我眼下那套旋钮还差多少」——不写出来，人会拿它当刮削面板那本估算账读。
//!
//! 主库**一律拿本地 fixture 目录模拟**：绝不去动任何真实设备或 SD 卡。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use romcat_core::catalog::{Catalog, Roots};
use romcat_core::dat::Convention;
use romcat_core::dat::logiqx::{DatHeader, GameRecord, RomRecord};
use romcat_core::dat::repo::{DatMeta, Unit};
use romcat_core::fs::RealFs;
use romcat_core::identify::fuzzy;
use romcat_core::scan::CancelToken;
use romcat_core::scrape::{self, Priorities};
use romcat_core::site::Site;
use romcat_core::sources::{Source, SourceState};
use romcat_core::stage::{Behind, Stage, StageRow};
use romcat_core::task::Ending;
use romcat_core::testing::container::{ZipEntrySpec, crc32, zip_container};
use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::triage::Scope;
use romcat_gui::app::{App, View};
use romcat_gui::headless;

mod shared;
use shared::{占位活, 画出来的字};

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

/// 一份**大到来不及扫完**的 fixture：400 个目录，各一个小文件。
///
/// 「按停」只有在扫描还没跑完的时候才验得到——两个文件的 fixture 在按下停之前就扫完了，
/// 那时验的是「跑完了」而不是「停下了」。**只往临时目录里写**，一个字节都不碰真库。
fn 建大库(tag: &str) -> TempDir {
    let dir = temp_dir(tag);
    for i in 0..400 {
        写(
            &dir.path().join(format!("SFC/第{i:03}组/游戏{i:03}.zip")),
            &zip(512),
        );
    }
    dir
}

/// 盘上那个文件叫**中文名**、DAT 里那条是**英文名**的一份 fixture 主库。
///
/// **折标题起没起作用要看得见，就得有这道落差**：折之前浏览屏上的显示标题退回
/// **作品名**（识别从 DAT 条目名折出来的那个英文名），折之后取的是盘上那个中文名
/// ——那正是这个库里中文名的**唯一来源**，因为 DAT 里根本没有中文（ADR-0019）。
///
/// 返回那份内容的字节：DAT 那一条要按它的 CRC-32 与大小来撞。
fn 建中文库(tag: &str) -> (TempDir, Vec<u8>) {
    let dir = temp_dir(tag);
    let bytes = vec![0xA1_u8; 4_096];
    写(
        &dir.path().join("FC/魂斗罗.zip"),
        &zip_container(&[ZipEntrySpec::stored("Contra.nes", bytes.clone())]),
    );
    (dir, bytes)
}

struct 现场 {
    工作区: TempDir,
    app: App,
}

impl 现场 {
    /// **中立库落在磁盘上**，不是只活在内存里：扫描跑在任务台上，后台那条线程按文件
    /// 路径自己开一份写得动的库。真库本来就是这个样子，fixture 照着摆才验得到那条路。
    fn 摆好() -> Self {
        let 工作区 = temp_dir("gui-roots-ws");
        let 库文件 = 工作区.path().join("catalog").join("fixture.sqlite3");
        drop(Catalog::create(&库文件, "fixture").expect("能开中立库"));
        let site = Site::open_file(工作区.path(), &库文件, None).expect("开得出现场");
        let mut app = App::new(site, 工作区.path().to_path_buf());
        app.show_view(View::Library);
        Self { 工作区, app }
    }

    /// 装一份**空的 DAT 库**：没有弹药识别根本不跑（那时它如实拒绝，见底下那条）。
    ///
    /// **空着是故意的**：工序段这几条钉的是「识别接上了任务台、那个数会刷新」，
    /// 不是识别认得出什么——那由 `romcat-core` 的 `tests/identify.rs` 钉。
    fn 装上弹药(&self) {
        let path = romcat_core::workspace::dat_repo_path(self.工作区.path());
        drop(romcat_core::dat::DatRepo::open(&path).expect("开得出 DAT 库"));
    }

    /// 装一份**认得出东西的** DAT 库：一条条目，按内容的 CRC-32 与大小撞。
    ///
    /// 折标题这几条要的是「库里真有一个作品」——标题挂在作品上，一个作品都没有的话
    /// 折出来是空的，那时验不到「显示标题跟着变」。
    fn 装上认得出的弹药(&self, 条目名: &str, bytes: &[u8]) {
        let path = romcat_core::workspace::dat_repo_path(self.工作区.path());
        let mut repo = romcat_core::dat::DatRepo::open(&path).expect("开得出 DAT 库");
        let mut writer = repo
            .begin(&Unit {
                source: "No-Intro".to_string(),
                name: "Nintendo - Nintendo Entertainment System".to_string(),
                url: "https://example.invalid/x".to_string(),
                fingerprint: "sha".to_string(),
            })
            .expect("开得了事务");
        writer
            .write_dat(
                &DatMeta {
                    name: "Nintendo - Nintendo Entertainment System".to_string(),
                    platform: "FC".to_string(),
                    convention: Convention::AsIs,
                    header: DatHeader::default(),
                },
                &[GameRecord {
                    name: 条目名.to_string(),
                    roms: vec![RomRecord {
                        name: format!("{条目名}.nes"),
                        size: Some(bytes.len() as u64),
                        crc32: Some(crc32(bytes)),
                        ..RomRecord::default()
                    }],
                    ..GameRecord::default()
                }],
            )
            .expect("写得进");
        writer.commit().expect("提交");
    }

    /// 采一趟**刮削**：把盘上那个文件的名字采成标题那个字段的值。
    ///
    /// **摆料而已，不是这几条要验的东西**——刮削自己那条路由 `romcat-core` 的
    /// `tests/titles.rs` 与界面上的刮削面板各自钉着。这里走核心库那个入口，
    /// 一个请求都不发（`media = false`、`Net` 是 `None`）。
    fn 刮一遍(&mut self, 根名: &str, 目录: &Path) {
        let pool = self.工作区.path().join("pool");
        let (_, site, _) = self.app.roots_site_and_tasks();
        let mut options = scrape::Options::new(Roots::single(根名, 目录), &pool);
        options.media = false;
        scrape::run(
            &RealFs::new(),
            &mut site.catalog,
            &Priorities::builtin(),
            &options,
            None,
            &mut scrape::RunContext {
                cancel: &CancelToken::new(),
                progress: &mut |_| {},
                naming: &fuzzy::Naming::off(),
                summaries: None,
                rulings: &scrape::zh::Rulings::none(),
            },
        )
        .expect("刮得动");
    }

    /// 把**折标题**那一道工序排上任务台，等它收场。界面上点那一行的按钮走的就是这条。
    fn 折标题(&mut self) {
        self.app.start_stage(Stage::FoldTitles);
        self.等任务跑完();
    }

    /// 工序段上折标题那一行。
    fn 折标题那一行(&self) -> StageRow {
        self.app
            .roots()
            .stages()
            .of(Stage::FoldTitles)
            .expect("工序段有折标题那一行")
            .clone()
    }

    /// 中立库里眼下有多少条叫法。
    fn 叫法条数(&self) -> u64 {
        self.app.site().catalog.title_count().expect("数得出")
    }

    /// 把**识别**那一道工序排上任务台，等它收场。界面上点那一行的按钮走的就是这条。
    fn 跑识别(&mut self) {
        self.app.start_stage(Stage::Identify);
        self.等任务跑完();
    }

    /// 工序段上识别那一行。
    fn 识别那一行(&self) -> StageRow {
        self.app
            .roots()
            .stages()
            .of(Stage::Identify)
            .expect("工序段有识别那一行")
            .clone()
    }

    /// 台上摆一趟**等到被按停才收场**的活，返回它的任务号。
    ///
    /// 任务台一次只跑一趟，于是之后排上去的那一趟稳稳停在队里。**钉在「按停」这个信号上，
    /// 不钉在挂钟上**：规格不许再添「几千步、每步睡几毫秒」那种写法（挂单 `Q427`）
    /// ——机器一忙它就自己先结束。忘了按停的话，`等任务跑完` 会当场炸出来。
    fn 占住任务台(&mut self) -> u64 {
        self.app.tasks_mut().queue("装作在扫一趟库", |task| {
            loop {
                task.check()?;
                std::thread::sleep(Duration::from_millis(1));
            }
        })
    }

    /// 把**刮削**那一道工序排上任务台，等它收场。界面上点那一行的按钮走的就是这条。
    fn 跑刮削(&mut self) {
        self.app.start_stage(Stage::Scrape);
        self.等任务跑完();
    }

    /// 工序段上刮削那一行。
    fn 刮削那一行(&self) -> StageRow {
        self.app
            .roots()
            .stages()
            .of(Stage::Scrape)
            .expect("工序段有刮削那一行")
            .clone()
    }

    /// 工序段上导出那一行。
    fn 导出那一行(&self) -> StageRow {
        self.app
            .roots()
            .stages()
            .of(Stage::Export)
            .expect("工序段有导出那一行")
            .clone()
    }

    /// 选一次**前端格式**与**导出目录**。界面上「导出设置」那一块填完按「记下」走的就是它。
    fn 选一次导出去哪儿(&mut self, 格式: &str, 目录: &Path) {
        let (screen, site, _) = self.app.roots_site_and_tasks();
        screen
            .stages_mut()
            .set_export_setup(site, 格式, &目录.to_string_lossy());
    }

    /// 把**导出**那一道工序排上任务台，等它收场。界面上点那一行的按钮走的就是这条。
    fn 导出(&mut self) {
        self.app.start_stage(Stage::Export);
        self.等任务跑完();
    }

    /// 队列屏眼下说库里还有多少个变体连识别都没跑过——**屏头那句话画的那个数**。
    fn 队列屏说的(&self) -> u64 {
        self.app.queue().not_run()
    }

    /// 队列屏重列一次——界面上那颗「重新列队列」按的就是它。
    fn 重列队列(&mut self) {
        let (queue, site) = self.app.queue_and_site();
        queue.reload(site);
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

    /// 排一趟扫描，**当场按「停下」**，等它收场。
    ///
    /// 界面上那一下就是这样：`scan` 把活排上台（`Board::queue` 当场开跑），
    /// 人紧接着在任务屏上按「停下」。
    fn 扫了就停(&mut self, 名字: &str) {
        let (screen, site, tasks) = self.app.roots_site_and_tasks();
        screen.scan(site, tasks, 名字);
        let id = screen.job_of(名字).expect("这一趟排上任务台了");
        tasks.stop(id);
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

/// 主库那棵目录树眼下长什么样：每份文件的**相对路径与内容**。
///
/// **导出只写元数据文件，一个 ROM 都不搬**（ADR-0004、验收第 6 条）。这一份在导出前后
/// 各取一次，逐字节比——比「文件数没变」严，那样连「原地改写了一个 ROM」都验得到。
fn 主库快照(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut 待走 = vec![root.to_path_buf()];
    while let Some(dir) = 待走.pop() {
        for entry in fs::read_dir(&dir).expect("读得出目录") {
            let path = entry.expect("读得出一条").path();
            if path.is_dir() {
                待走.push(path);
            } else {
                let 相对 = path.strip_prefix(root).expect("在这棵树里").to_path_buf();
                out.push((相对, fs::read(&path).expect("读得出文件")));
            }
        }
    }
    out.sort();
    out
}

fn 跑一帧(ctx: &egui::Context, app: &mut App) {
    headless::frame(ctx, headless::input(), |ui| app.ui(ui));
}

/// **没开跑的那一下不许在任务历史里留一条**（票 `gui-looks-like-the-design/07`）：任务历史眼下
/// 几条，与按下去之前数的一样。多出来的话，把多出来的那一条怎么收的场一并印出来。
fn 历史没多一条(app: &App, 之前: usize) {
    assert_eq!(
        app.tasks().history().len(),
        之前,
        "没开跑的那一下在任务历史里多了一条：{:?}",
        app.tasks()
            .history()
            .first()
            .map(|record| record.ending.render()),
    );
}

/// 导出目录里眼下躺着的那几份元数据文件，按路径排好。
fn 导出去的文件(导出去: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(导出去)
        .expect("导出目录在")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file())
        .collect();
    out.sort();
    out
}

/// 屏上**整段就是** `按钮上的字` 的那一段画在哪儿（中心点）；找不着时是 `None`。
///
/// 与 `shared::那一段画在哪儿` 的差别是**整段相等**而不是「含着这几个字」：按钮上的字
/// 常常也出现在旁边那句说明里（「……再按「我看过了，照写」」），按「含着」找会点到
/// 那句说明上，按钮一下都没挨着。
fn 按钮画在哪儿(output: &egui::FullOutput, 按钮上的字: &str) -> Option<egui::Pos2> {
    fn 找(shape: &egui::epaint::Shape, 按钮上的字: &str) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(text) => (text.galley.text() == 按钮上的字)
                .then(|| egui::Rect::from_min_size(text.pos, text.galley.size()).center()),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|one| 找(one, 按钮上的字)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|clipped| 找(&clipped.shape, 按钮上的字))
}

/// 在窗口上**真点一下**写着 `按钮上的字` 的那颗按钮：指针挪过去、按下、松开，各一帧。
///
/// 位置从画出来的那一段字上量（[`按钮画在哪儿`]），不把控件的 `Rect` 从界面层漏出来。
/// 先空跑一帧把界面跑稳——首帧还在估滚动区尺寸，量出来的位置会偏。
///
/// # Panics
/// 屏上找不着那颗按钮时当场炸，并把这一帧画出来的字一并印出来。
fn 点一下(ctx: &egui::Context, app: &mut App, 按钮上的字: &str) {
    跑一帧(ctx, app);
    let 稳了 = headless::frame(ctx, headless::input(), |ui| app.ui(ui));
    let Some(pos) = 按钮画在哪儿(&稳了, 按钮上的字) else {
        panic!(
            "屏上没有「{按钮上的字}」这颗按钮，没处点：\n{}",
            画出来的字(&稳了)
        );
    };
    let 按 = |pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    };
    for events in [
        vec![egui::Event::PointerMoved(pos)],
        vec![按(true)],
        vec![按(false)],
    ] {
        let mut input = headless::input();
        input.events = events;
        headless::frame(ctx, input, |ui| app.ui(ui));
    }
}

/// **滚到库屏底下**：真发滚轮事件往下滚，滚到这一帧画出来的字不再变为止（等的是滚动停下，不是等一段时间），
/// 再把指针挪走。只滚、不读——读字仍由调用方接着那一帧做。
///
/// 工序段底下「一起铺媒体」那几句在库屏正文的最底下：主窗口长出左栏、屏头比从前的顶栏高
/// （票 `gui-looks-like-the-design/32`）之后，它们落到了视口外，而 egui 不画视口外的字。
fn 滚到库屏底下(ctx: &egui::Context, app: &mut App) {
    let 指在 = egui::pos2(headless::VIEWPORT[0] * 0.6, headless::VIEWPORT[1] * 0.75);
    let mut 上一帧 = None;
    let mut 停了 = false;
    for _ in 0..200 {
        let mut input = headless::input();
        input.events.push(egui::Event::PointerMoved(指在));
        input.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -headless::VIEWPORT[1]),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        });
        let 这一帧 = 画出来的字(&headless::frame(ctx, input, |ui| app.ui(ui)));
        if 上一帧.as_ref() == Some(&这一帧) {
            停了 = true;
            break;
        }
        上一帧 = Some(这一帧);
    }
    assert!(停了, "滚了两百帧，库屏正文还在动");
    let mut input = headless::input();
    input.events.push(egui::Event::PointerGone);
    headless::frame(ctx, input, |ui| app.ui(ui));
}

/// 摆一份**已经导过一趟**的现场：横跨两个平台的 fixture 主库扫进来、选好 Pegasus 与
/// 导出目录、导一趟。返回主库（得活到测试结束）、现场与导出目录。
fn 导过一趟(tag: &str) -> (TempDir, 现场, PathBuf) {
    let 库 = 建库(tag);
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    let 导出去 = 现场.工作区.path().join("导出去");
    现场.选一次导出去哪儿("Pegasus", &导出去);
    现场.导出();
    (库, 现场, 导出去)
}

/// 有人在工具外面给这份文件手加了一行。返回手改之后的全文。
fn 手改一行(落点: &Path) -> String {
    let mut 手改的 = fs::read_to_string(落点).expect("读得出");
    手改的.push_str("\n# 我后来手加的一行\n");
    fs::write(落点, &手改的).expect("写得进");
    手改的
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
    let 工作区 = 现场.工作区.path().to_path_buf();
    let 手滑 = 工作区.join("roms");
    fs::create_dir_all(&手滑).expect("建得出");

    现场.加根(&手滑, "手滑");
    let 错 = 现场.app.roots().error().expect("该被拒");
    assert!(错.contains("主库只读"), "得点名那条纪律：{错}");
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
fn 根不在位时按扫描_屏上说清插上那块盘_任务历史不多一条() {
    // 票 `gui-looks-like-the-design/07`：盘不在位是按下去之前就判得出的（查一眼那个目录在不在），
    // 那一下不往任务台上排，只在屏上说为什么不行、去哪儿办——任务历史只记真跑过的。
    let 库 = 建库("gui-roots-拔盘再扫");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    drop(库);
    {
        let (screen, site, _) = 现场.app.roots_site_and_tasks();
        screen.reload(site);
    }
    let 历史几条 = 现场.app.tasks().history().len();

    {
        let (screen, site, tasks) = 现场.app.roots_site_and_tasks();
        screen.scan(site, tasks, "主库");
    }
    现场.等任务跑完();
    跑一帧(&ctx, &mut 现场.app);
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    let 说的 = 现场.app.roots().error().expect("该直说");
    assert!(说的.contains("不在位"), "没说清为什么不行：{说的}");
    assert!(说的.contains("插上那块盘"), "没说去哪儿办：{说的}");
    assert!(
        屏上.contains(说的),
        "那句话没画在屏上：{说的}\n屏上：\n{屏上}"
    );
    历史没多一条(&现场.app, 历史几条);
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

#[test]
fn 界面上按停一趟扫描_任务台与库屏都说它被按停了_不说跑完了() {
    // 同一趟活在三处说的话得是同一句：根那一行写着「那一趟被中断」，任务屏历史却记
    // 「完成」、库屏 notice 还说「跑完了」——那三样里有两样是骗人的。
    // 命令行上同一趟看的是 `outcome.interrupted`，印「扫描被中断」并退 130。
    let 库 = 建大库("gui-roots-按停");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫了就停("主库");
    跑一帧(&ctx, &mut 现场.app);

    // 先确认这一趟**真的**是半路停下的——fixture 小到扫得完的话，下面两条就没在测东西。
    let scan = 现场.app.roots().roots()[0]
        .root
        .scan
        .expect("这一趟扫描留下了记录");
    assert!(
        scan.interrupted,
        "这一趟没被中断，fixture 太小，测不到按停那条路"
    );

    let history = 现场.app.tasks().history();
    assert_eq!(history.len(), 1, "任务台上该正好留下这一趟");
    // **它走的是「停在半路」那一档**：断点与那半份体检报告都留下了，不是「什么都没留下」。
    let Ending::Halfway { left_behind, .. } = &history[0].ending else {
        panic!(
            "任务屏历史把按停记成了「{}」——那句「跑了 X 秒」于是成了骗人的话",
            history[0].ending.render(),
        );
    };
    // **断到那半句上**：只断「断点」两个字的话，「这一趟没设断点」那一支照样绿，
    // 而界面这条路是**设了**断点的（`Screen::scan` 里那份 `CheckpointOptions`）。
    assert!(
        left_behind.contains("断点写下了"),
        "说不出留下了断点：{left_behind}",
    );

    let 话 = 现场.app.roots().notice().expect("库屏该说一句");
    assert!(!话.contains("跑完了"), "按停的那一趟不许说「跑完了」：{话}");
    assert!(话.contains("按停"), "库屏得说清它是被按停的：{话}");
}

#[test]
fn 界面发起的扫描停下之后断点真的写在盘上() {
    // 库屏停下来那句话是「下次接着跑」，而**依据只能是断点文件**。命令行默认就设断点、
    // 15 秒存一次、`--resume` 接着扫；界面这边不设的话，那句话是空头支票——37 分钟那种
    // 遍历得从头走一遍。
    let 库 = 建大库("gui-roots-断点");
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫了就停("主库");

    let scan = 现场.app.roots().roots()[0]
        .root
        .scan
        .expect("这一趟扫描留下了记录");
    assert!(scan.interrupted, "这一趟没被中断，断点本来就不该留下");

    // **路径与命令行 `--resume` 找的是同一个**（`Site::checkpoint_path`）——
    // 两条路折出两个文件名的话，界面停下的那一趟命令行就接不上。
    let 断点 = 现场.app.site().checkpoint_path(现场.工作区.path(), "主库");
    assert!(
        断点.is_file(),
        "停下来了却没有断点，「下次接着跑」没有依据：{}",
        断点.display(),
    );
}

#[test]
fn 扫完一个根之后浏览屏当场看得见新扫进来的那几行() {
    // 同一个窗口里库屏说 2 个变体、浏览屏说 0 行——浏览屏缓着的东西只在**换筛选**时才
    // 作废，而扫描一个字都没改筛选。得由窗口在认领完那一趟之后转告它一声。
    let 库 = 建库("gui-roots-刷新");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();

    // 先看一眼浏览屏：空库，0 行、筛选面板上一个平台都没有。
    现场.app.show_view(View::Browse);
    跑一帧(&ctx, &mut 现场.app);
    assert_eq!(现场.app.window().total(), 0, "空库上浏览屏本该是 0 行");
    assert!(
        现场.app.browse().facets().platforms.is_empty(),
        "空库上筛选面板本该是空的",
    );

    现场.app.show_view(View::Library);
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    let 库屏上的变体数 = 现场.app.roots().roots()[0].stats.variants;
    assert!(库屏上的变体数 > 0, "这一趟本该扫出东西来");

    现场.app.show_view(View::Browse);
    跑一帧(&ctx, &mut 现场.app);
    assert!(
        现场.app.window().total() > 0,
        "库屏说 {库屏上的变体数} 个变体，浏览屏还画着 0 行",
    );
    assert!(
        !现场.app.browse().facets().platforms.is_empty(),
        "扫进来两个平台，筛选面板上一个都没有",
    );
}

#[test]
fn 库屏在根与数据源之后长出工序那一段_识别那一行说的是还差多少() {
    // **这一段答的是「下一步该干什么」**。时间戳答不了这个问题：屏上写「上次 10:31 跑过
    // 识别」，人还是不知道加完那块盘之后要不要重跑。
    let 库 = 建库("gui-stages-一段");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");

    assert_eq!(
        现场.识别那一行().behind,
        Behind::Left(2),
        "扫进来两个变体，一个都还没跑过识别",
    );

    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    assert!(
        屏上.lines().any(|line| line.trim() == "工序"),
        "库屏上没有工序那一段：\n{屏上}",
    );
    assert!(
        屏上.lines().any(|line| line.trim() == "识别"),
        "工序段上没有识别那一行：\n{屏上}",
    );
    assert!(
        屏上
            .lines()
            .any(|line| line.trim() == "2 个变体连识别都还没跑过"),
        "识别那一行说的不是「还差多少」：\n{屏上}",
    );
}

#[test]
fn 工序段那个数与待确认队列屏上的是同一个() {
    // **不另造一份**：两处各算一份的话，同一份库在库屏与队列屏上会报出两个数，
    // 而人没有办法知道该信哪一个。
    let 甲 = 建库("gui-stages-甲");
    let 乙 = 建库("gui-stages-乙");
    let mut 现场 = 现场::摆好();
    现场.装上弹药();
    现场.加根(甲.path(), "主库");
    现场.扫("主库");
    现场.跑识别();
    assert_eq!(
        现场.识别那一行().behind,
        Behind::Left(0),
        "跑完了就不差什么了"
    );

    // **加了一块盘重扫**：那个数自己就涨上去（规格 28）。
    现场.加根(乙.path(), "元数据库");
    现场.扫("元数据库");
    let Behind::Left(还差) = 现场.识别那一行().behind else {
        panic!("识别这一支说得出还差多少");
    };
    assert_eq!(还差, 2, "乙那两个变体连识别都还没跑过");

    现场.重列队列();
    assert_eq!(
        现场.队列屏说的(),
        还差,
        "库屏工序段与待确认队列屏说的不是同一个数",
    );
}

#[test]
fn 点一下把识别排上任务台_跑完那一行的数字当场刷新_队列自己重新列过() {
    // 三件事一趟兑现：**排上任务台**（跑在画帧那条线程之外）、**那一行当场刷新**
    // （看得出这一趟起了作用）、**队列自己重新列过**（不必再点一次「重新列队列」）。
    let 库 = 建库("gui-stages-跑一趟");
    let mut 现场 = 现场::摆好();
    现场.装上弹药();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    现场.重列队列();
    assert!(!现场.app.queue().queue().identified(), "前提：还没跑过识别");
    assert_eq!(现场.队列屏说的(), 2);

    现场.跑识别();

    // **那一行当场刷新**：不必再点一次什么。
    assert_eq!(现场.识别那一行().behind, Behind::Left(0));
    // **队列自己重新列过**：这几句一次「重新列队列」都没点。
    assert!(
        现场.app.queue().queue().identified(),
        "识别跑完了，队列屏却还说「还没跑过识别」——它没有自己重列",
    );
    assert_eq!(现场.队列屏说的(), 0, "队列屏那个数没跟着刷新");

    // 任务台上留下一条**跑完了**的历史。
    let record = &现场.app.tasks().history()[0];
    assert_eq!(record.name, "识别");
    assert!(
        matches!(record.ending, Ending::Done(_)),
        "跑完的那一趟记成了「{}」",
        record.ending.render(),
    );
}

#[test]
fn 识别跑到一半按停在任务台历史上记成部分完成() {
    // **写过东西的活被叫停要记成第三档**：识别起手就动过库（从头算的那一趟把上一轮的
    // 结论清干净），记成「已取消、可以当没跑过」的话，那句话是骗人的。
    //
    // 大 fixture：两个文件的库在按下停之前就跑完了，那时验的是「跑完了」而不是「停下了」。
    let 库 = 建大库("gui-stages-按停");
    let mut 现场 = 现场::摆好();
    现场.装上弹药();
    现场.加根(库.path(), "主库");
    现场.扫("主库");

    现场.app.start_stage(Stage::Identify);
    let id = 现场
        .app
        .roots()
        .stages()
        .task_of(Stage::Identify)
        .expect("这一趟排上任务台了");
    现场.app.tasks_mut().stop(id);
    现场.等任务跑完();

    let record = &现场.app.tasks().history()[0];
    let Ending::Halfway { left_behind, .. } = &record.ending else {
        panic!("按停了却把这一趟记成了「{}」", record.ending.render());
    };
    // **下一趟接着算剩下的**（票 `gui-answers-all-six/03`）。还说从头再算的话，人会以为
    // 按停白按了——那份结论其实留着，下一趟接着用。
    assert!(
        left_behind.contains("下一趟接着算剩下的") && !left_behind.contains("从头再算"),
        "那一句说不出下一趟接着算：{left_behind}",
    );
}

#[test]
fn 这一趟正在跑的时候那一行的按钮按不下去() {
    // 不禁掉的话同一趟活会被排两遍——而两趟识别在同一份中立库上互相清对方的结论。
    //
    // **台上先摆一趟别的活**：任务台一次只跑一趟，于是排上去的识别稳稳地停在队列里。
    // 这一条要验的是「那一行的按钮按不下去」，不该靠「识别恰好还没跑完」这种挂钟彩票
    // （挂单 `Q196` / `Q349` 说的正是那种测试）——那样机器一忙它就绿得莫名其妙。
    let 库 = 建库("gui-stages-按不下去");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.装上弹药();
    现场.加根(库.path(), "主库");
    现场.扫("主库");

    let 占位 = 占位活::排上(现场.app.tasks_mut(), "装作在扫一趟库");

    现场.app.start_stage(Stage::Identify);
    let id = 现场
        .app
        .roots()
        .stages()
        .task_of(Stage::Identify)
        .expect("这一趟排上任务台了");

    // 屏上那一行写着「跑着呢」，那颗按钮是禁着的。
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    assert!(
        屏上.lines().any(|line| line.trim() == "跑着呢"),
        "这一趟还在台上，那一行的按钮却还写着「运行」：\n{屏上}",
    );

    // 再按一次（别处的捷径走的也是这个入口）：**什么都不该发生**。
    现场.app.start_stage(Stage::Identify);
    assert_eq!(
        现场.app.tasks().queued().len(),
        1,
        "同一趟识别被排了两遍：{:?}",
        现场.app.tasks().queued(),
    );
    assert_eq!(
        现场.app.roots().stages().task_of(Stage::Identify),
        Some(id),
        "第二次按下换掉了台上那一趟",
    );

    现场.app.tasks_mut().stop(id);
    占位.按停(现场.app.tasks_mut());
    现场.等任务跑完();
}

#[test]
fn 外置盘不在位的时候工序那几行照样看得见() {
    // 工序段的数从中立库里折出来，**一个字节都不读主库**（ADR-0001）。盘不在位时它照样
    // 答得出话——那正是「外置盘没挂上也能知道库的状况」。
    let 库 = 建库("gui-stages-不在位");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    assert_eq!(现场.识别那一行().behind, Behind::Left(2));

    // 把那块「盘」拔掉。
    drop(库);
    let (screen, site, _) = 现场.app.roots_site_and_tasks();
    screen.reload(site);
    assert!(
        现场.app.roots().roots().iter().all(|row| !row.mounted),
        "前提：那个根现在不在位",
    );

    assert_eq!(
        现场.识别那一行().behind,
        Behind::Left(2),
        "盘拔了，工序段就答不出话了",
    );
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    assert!(
        屏上
            .lines()
            .any(|line| line.trim() == "2 个变体连识别都还没跑过"),
        "盘不在位，工序那一行就不见了：\n{屏上}",
    );
}

#[test]
fn 还没取回那份弹药时按识别_屏上说清为什么与去哪儿取_任务历史不多一条() {
    // 票 `gui-looks-like-the-design/07`：**压根没开跑与跑了没成是两件事。** 没有 DAT 库是
    // 按下去之前就判得出的（ADR-0005 修订段「原料还没备齐」）——那一下不往任务台上排，
    // 只在屏上说一句为什么不行、去哪儿办。排上去再在那一趟里报失败的话，任务历史里就多一条
    // 从没跑过的「失败」，人会去找哪儿坏了。
    //
    // **偷偷开一份空的 DAT 库跑下去更不行**：整库都会落成「未命中」。
    let 库 = 建库("gui-stages-没弹药");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    let 历史几条 = 现场.app.tasks().history().len();

    现场.app.start_stage(Stage::Identify);
    现场.等任务跑完();
    跑一帧(&ctx, &mut 现场.app);
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));

    assert!(
        屏上.contains("还没有 DAT 库"),
        "屏上没说清为什么不行：\n{屏上}"
    );
    assert!(屏上.contains("数据源"), "屏上没指向取回它的地方：\n{屏上}");
    历史没多一条(&现场.app, 历史几条);
    // 库里一条结论都没多出来。
    assert_eq!(现场.识别那一行().behind, Behind::Left(2));
}

#[test]
fn 取回_dat_那一趟已经排在台上时按识别_排在它后面而不是当场拒() {
    // 票 `gui-looks-like-the-design/07` 只拒**按下去之前就判得出**的前提。取回 DAT 那一趟已经排在
    // 台上，「还没有 DAT 库」就判不出来了：轮到识别时它多半已经取回来了（改之前识别就这样排在它
    // 后面跑成）。当场拒的话，人得干等取回跑完再按一次。
    //
    // 取回那一趟**从头到尾排着、一次都不开跑**：台上先摆一趟占位活占着位子——一个网络请求都不发。
    let 库 = 建库("gui-stages-弹药在路上");
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    let 占位 = 占位活::排上(现场.app.tasks_mut(), "装作在扫一趟库");
    {
        let (screen, _, tasks) = 现场.app.roots_site_and_tasks();
        screen.fetch(tasks, Source::Dat);
    }
    let 取回 = 现场
        .app
        .tasks()
        .queued()
        .into_iter()
        .find(|(_, name)| name.starts_with("取回"))
        .map(|(id, _)| id)
        .expect("取回 DAT 那一趟排上了");

    现场.app.start_stage(Stage::Identify);
    let 识别 = 现场
        .app
        .roots()
        .stages()
        .task_of(Stage::Identify)
        .expect("取回 DAT 已经排在台上，识别该排在它后面，而不是当场拒");
    assert!(
        现场.app.roots().stages().error().is_none(),
        "排上了却还挂着一句拒绝：{:?}",
        现场.app.roots().stages().error(),
    );

    // 收拾：排着的两趟撤掉，再按停占位活——取回那一趟一次都没开跑。
    现场.app.tasks_mut().stop(取回);
    现场.app.tasks_mut().stop(识别);
    占位.按停(现场.app.tasks_mut());
    现场.等任务跑完();

    // 取回那一趟撤掉了，DAT 库仍不在：这时再按识别就当场拒，任务历史不多一条。
    let 历史几条 = 现场.app.tasks().history().len();
    现场.app.start_stage(Stage::Identify);
    assert!(
        现场.app.roots().stages().task_of(Stage::Identify).is_none(),
        "取回撤掉之后还没有 DAT 库，识别却排上了",
    );
    let 说的 = 现场.app.roots().stages().error().expect("该当场说清");
    assert!(说的.contains("还没有 DAT 库"), "{说的}");
    历史没多一条(&现场.app, 历史几条);
}

#[test]
fn 那份弹药在却打不开时识别照旧排上去_任务历史记失败() {
    // 票 `gui-looks-like-the-design/07` 的另一半：**跑了没成照旧进历史，收场是「失败」。**
    // DAT 库那个文件在，按下去之前查一眼看不出毛病；打不开是真去开它的那一下才撞上的
    // ——那一趟开跑过，照实记失败、说清为什么。与上面那条分开的正是这一下。
    let 库 = 建库("gui-stages-弹药坏了");
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    写(
        &romcat_core::workspace::dat_repo_path(现场.工作区.path()),
        "这不是一份 SQLite 库".as_bytes(),
    );
    let 历史几条 = 现场.app.tasks().history().len();

    现场.跑识别();

    assert_eq!(
        现场.app.tasks().history().len(),
        历史几条 + 1,
        "真跑过的那一趟没进任务历史",
    );
    let record = &现场.app.tasks().history()[0];
    let Ending::Failed { why, .. } = &record.ending else {
        panic!("DAT 库打不开却把这一趟记成了「{}」", record.ending.render());
    };
    assert!(why.contains("DAT 库打不开"), "说不清为什么没成：{why}");
    let 说的 = 现场.app.roots().stages().error().expect("失败要说出来");
    assert!(说的.contains(&record.ending.render()), "{说的}");
    assert_eq!(现场.识别那一行().behind, Behind::Left(2));
}

#[test]
fn 折标题跑完之后浏览屏上的显示标题跟着更新() {
    // **这道工序起没起作用，唯一看得见的地方就是浏览屏上那个显示标题。**
    // 不重读那一格的话，人点完折标题看见的还是折之前那个名字，只好去关掉窗口重开
    // ——而那正是这一票要消掉的那种「还得记住下一步」。
    let (库, 字节) = 建中文库("gui-stages-折标题");
    let mut 现场 = 现场::摆好();
    现场.装上认得出的弹药("Contra (Japan)", &字节);
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    现场.跑识别();
    现场.刮一遍("主库", 库.path());

    // 点开那一行、再选中它底下那个变体——**界面上那两下**。
    {
        let (browse, site) = 现场.app.browse_and_site();
        let 一行 = site
            .catalog
            .work_page(browse.query(), 0, 8)
            .expect("取得出一页")
            .into_iter()
            .next()
            .expect("识别认出了一个作品");
        browse.open_work(&site.catalog, &一行.anchor);
        browse.pick(&site.catalog, "主库/FC/魂斗罗.zip");
    }
    // **折之前**标题集合是空的，显示标题退回作品名（那个英文名）。
    let 折前 = 现场.app.browse().detail().expect("点得开").clone();
    assert!(
        折前.titles.is_empty(),
        "还没折就有标题集合了：{:?}",
        折前.titles,
    );
    let 折前显示 = 折前.display.expect("显示标题总挑得出一个").display;
    assert_eq!(折前显示, "Contra", "折之前该退回作品名");

    // 点一下**折标题**。
    现场.折标题();

    let 折后 = 现场.app.browse().detail().expect("那一条还在");
    assert!(!折后.titles.is_empty(), "折完标题集合还是空的");
    let 折后显示 = &折后.display.as_ref().expect("显示标题").display;
    assert_ne!(
        *折后显示, 折前显示,
        "折完了，浏览屏上的显示标题一个字都没变"
    );
    assert_eq!(
        折后显示, "魂斗罗",
        "中文名没顶上来——DAT 里没有中文，这个名字只可能从盘上那个文件名来",
    );

    // 任务台历史上留下一条**跑完了**，名字就是那道工序的名字。
    let record = &现场.app.tasks().history()[0];
    assert_eq!(record.name, "整理标题");
    assert!(
        matches!(record.ending, Ending::Done(_)),
        "跑完的那一趟记成了「{}」",
        record.ending.render(),
    );

    // 工序段那一行改口说「上次跑是 ⋯」，而**识别那一行照旧报数**（验收第 2 条）。
    let 那一句 = 现场.折标题那一行().render();
    assert!(那一句.contains("上次跑是"), "{那一句}");
    assert_eq!(
        现场.识别那一行().behind,
        Behind::Left(0),
        "折标题退回时刻，把识别那一支也一起降级了",
    );
}

#[test]
fn 折标题排着队被撤掉时标题集合一条都没少() {
    // **重折是「清掉再写回」**：停在那中间等于把整份标题集合丢掉，所以这一支压根不停
    // 在那儿——按停只停在开折之前，那一趟一个字节都没写，记的是「停了」而不是
    // **停在半路**。这一条钉的正是「停下来什么都没留下」这句话是真的。
    //
    // **台上先摆一趟别的活**：任务台一次只跑一趟，于是排上去的折标题稳稳地停在队列里
    // ——不靠「恰好还没跑完」那种挂钟彩票（挂单 `Q196` / `Q349`）。
    let (库, 字节) = 建中文库("gui-stages-折标题按停");
    let mut 现场 = 现场::摆好();
    现场.装上认得出的弹药("Contra (Japan)", &字节);
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    现场.跑识别();
    现场.刮一遍("主库", 库.path());
    现场.折标题();
    let 折过之后 = 现场.叫法条数();
    assert!(折过之后 > 0, "前提：折过一趟，库里有叫法");

    let 占位 = 占位活::排上(现场.app.tasks_mut(), "装作在扫一趟库");
    现场.app.start_stage(Stage::FoldTitles);
    let id = 现场
        .app
        .roots()
        .stages()
        .task_of(Stage::FoldTitles)
        .expect("这一趟排上任务台了");
    现场.app.tasks_mut().stop(id);
    现场.app.poll_tasks();

    let record = 现场
        .app
        .tasks()
        .history()
        .iter()
        .find(|one| one.id == id)
        .expect("撤掉的那一趟也进历史");
    assert!(
        matches!(record.ending, Ending::Stopped),
        "撤掉的那一趟记成了「{}」——它一个字节都没写，不是停在半路",
        record.ending.render(),
    );
    assert_eq!(
        现场.叫法条数(),
        折过之后,
        "这一趟停在开折之前，标题集合却少了几条",
    );
    // 那一行的按钮又按得下去了。
    assert!(
        现场
            .app
            .roots()
            .stages()
            .task_of(Stage::FoldTitles)
            .is_none()
    );

    占位.按停(现场.app.tasks_mut());
    现场.等任务跑完();
}

#[test]
fn 工序段上折标题一行_画的是上次跑的时刻() {
    // 验收第 1 条与第 2 条：那一行在屏上，说的是**上次跑的时刻**（这一支的度量走了
    // 退路，见 `romcat_core::stage::Stage::FoldTitles` 与挂单 `Q426`），
    // **而识别那一行不受影响**。
    let 库 = 建库("gui-stages-两行");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");

    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    assert!(
        屏上.lines().any(|line| line.trim() == "整理标题"),
        "工序段上没有整理标题那一行：\n{屏上}",
    );
    assert!(
        屏上.lines().any(|line| line.trim() == "工序"),
        "工序段没有「工序」那条标题栏：\n{屏上}",
    );

    // 一趟都没折过：说的是「还没跑过」，**不是零**。
    let 那一句 = 现场.折标题那一行().render();
    assert!(那一句.contains("还没跑过"), "{那一句}");
    assert!(那一句.contains("算不出还差多少"), "{那一句}");
    // 识别那一支照旧报数。
    assert_eq!(现场.识别那一行().behind, Behind::Left(2));
}

#[test]
fn 折标题读不动优先级表时如实拒绝_并说清停在哪一步() {
    // **两件事一条测试**：
    //
    // 1. **不静默退回内置那份优先级表。** 挑**显示标题**用的就是这份表，而工作目录里
    //    那份 `priorities.toml` 正是人改过的说法——悄悄退回内置的，人会看见一份自己
    //    没定过的显示标题，还查不出为什么。
    // 2. **这一趟报得出走到第几步。** 任务台记下来的那句「停在哪一步」取自把手上的
    //    进度（`Board::settle`），它对得上就说明 `task.step` 那几句真的报出去了
    //    ——**验收第 3 条「报得出进度」的落点**。
    let 库 = 建库("gui-stages-折标题坏表");
    let mut 现场 = 现场::摆好();
    写(
        &现场.工作区.path().join("priorities.toml"),
        "这不是一份 TOML".as_bytes(),
    );
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    现场.折标题();

    let record = &现场.app.tasks().history()[0];
    let Ending::Failed { step, why } = &record.ending else {
        panic!("表都读不动却把这一趟记成了「{}」", record.ending.render());
    };
    assert_eq!(step, "读优先级表", "说不清停在哪一步");
    assert!(!why.is_empty(), "说不清为什么跑不了");
    // 库里一条叫法都没多出来。
    assert_eq!(现场.叫法条数(), 0);
    // 那一行照旧说「还没跑过」——**失败不算跑过**。
    assert!(
        现场.折标题那一行().render().contains("还没跑过"),
        "{}",
        现场.折标题那一行().render(),
    );
}

#[test]
fn 工序段上导出一行_画的是上次跑的时刻() {
    // 验收第 1 条：那一行在屏上，说的是**上次导出的时刻**（这一支的度量走了退路，
    // 见 `romcat_core::stage::Stage::Export` 与挂单 `Q436`），**而另外两支不受牵连**。
    let 库 = 建库("gui-stages-导出一行");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");

    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    assert!(
        屏上.lines().any(|line| line.trim() == "导出"),
        "工序段上没有导出那一行：\n{屏上}",
    );

    // 一趟都没导过：说的是「还没跑过」，**不是零**。
    let 那一句 = 现场.导出那一行().render();
    assert!(那一句.contains("还没跑过"), "{那一句}");
    assert!(那一句.contains("算不出还差多少"), "{那一句}");
    // **另外两支照旧**：识别报数，折标题说它自己那一句（验收第 1 条「不牵连另外两支」）。
    assert_eq!(现场.识别那一行().behind, Behind::Left(2));
    assert!(现场.折标题那一行().render().contains("还没跑过"));
}

#[test]
fn 工序段上刮削一行_数与口径都画在屏上() {
    // 验收第 1–3 条：工序段有这几行（票 `gui-answers-all-six/04` 时四行，票
    // `gui-looks-like-the-design/06` 补齐扫描与裁决成六行）、刮削那一行报得出「一条刮削结论都没有的
    // 变体」有几个、那个数的口径**画在屏上**——不只在悬停里，人不会去悬停一个数。
    let 库 = 建库("gui-stages-刮削一行");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.装上弹药();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    // 识别先跑过：刮削那一行才是下一步、说它自己的数——识别没跑过时它说「等待识别完成」（挂单 `Q827` 照稿）。
    现场.跑识别();

    assert_eq!(
        现场.刮削那一行().behind,
        Behind::Left(2),
        "扫进来两个变体，一条刮削结论都还没有",
    );

    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    assert!(
        屏上.lines().any(|line| line.trim() == "工序"),
        "工序段没有「工序」那条标题栏：\n{屏上}",
    );
    assert!(
        屏上.lines().any(|line| line.trim() == "刮削"),
        "工序段上没有刮削那一行：\n{屏上}",
    );
    assert!(
        屏上
            .lines()
            .any(|line| line.trim() == "2 个变体一条刮削结论都还没有"),
        "刮削那一行说的不是「还差多少」：\n{屏上}",
    );
    assert!(
        屏上.contains(romcat_core::stage::SCRAPE_BASIS),
        "刮削那一行的口径没画在屏上：\n{屏上}",
    );
    // **另外几行照旧**：识别那一行说它自己的话。
    assert!(
        屏上
            .lines()
            .any(|line| line.trim() == "每个变体都跑过识别了"),
        "加了刮削那一行，识别那一行不见了：\n{屏上}",
    );
}

#[test]
fn 工序六行齐_次序照设计稿_每行报得出数或退回时刻() {
    // 票 `gui-looks-like-the-design/06` 验收第 1 条：库屏上看得见**整条路有多长、走到了哪儿**
    // ——扫描、识别、刮削、整理标题、裁决、导出六行，次序照设计稿。每一行都得说话：说得出
    // 还差多少的报数，算不出的那两支（整理标题、导出）退回上次跑的时刻并说清为什么。
    let 库 = 建库("gui-stages-六行");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");

    跑一帧(&ctx, &mut 现场.app);
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));

    // **六个名字按这个次序画出来**：屏上别处也可能有同一个词（按钮、说明），所以按子序列找。
    let 六行 = ["扫描", "识别", "刮削", "整理标题", "裁决", "导出"];
    let mut 往下找 = 屏上.lines().map(str::trim);
    for 名字 in 六行 {
        assert!(
            往下找.any(|line| line == 名字),
            "工序段上「{名字}」那一行没画出来，或者次序与设计稿对不上：\n{屏上}",
        );
    }

    let 行 = 现场.app.roots().stages().rows();
    assert_eq!(行.len(), 六行.len(), "工序段不是六行：{行:?}");
    for row in 行 {
        let 那一句 = 现场.app.roots().stages().line(row);
        assert!(
            屏上.contains(&那一句),
            "「{}」那一行说的话没画在屏上：{那一句}\n屏上：\n{屏上}",
            row.stage.label(),
        );
    }
    // **新补的两行各说各的**：唯一那个根完整扫过一趟；还没跑过识别，识别后面那几行（裁决在内）不说「没有等着
    // 裁决的」那句空话，说在等识别（`Stages::line`，挂单 `Q827` 照稿）。
    assert!(
        屏上
            .lines()
            .any(|line| line.trim() == "每个根都完整扫过一趟了"),
        "扫描那一行说的不是还差几个根：\n{屏上}",
    );
    assert!(
        屏上.lines().any(|line| line.trim() == "等待识别完成"),
        "识别还没跑过，裁决那一行没说它在等识别：\n{屏上}",
    );
    // **算不出的那两支退回时刻**，不画零。
    for 算不出的 in [Stage::FoldTitles, Stage::Export] {
        let 那一句 = 现场
            .app
            .roots()
            .stages()
            .of(算不出的)
            .expect("有这一行")
            .render();
        assert!(
            那一句.contains("还没跑过") && 那一句.contains("算不出还差多少"),
            "「{}」那一行没退回时刻：{那一句}",
            算不出的.label(),
        );
    }
}

/// 库屏眼下**真画出来**的字：先空跑一帧把界面跑稳，再画一帧收字。
fn 库屏上的字(ctx: &egui::Context, app: &mut App) -> String {
    跑一帧(ctx, app);
    画出来的字(&headless::frame(ctx, headless::input(), |ui| app.ui(ui)))
}

#[test]
fn 屏头添加根_选中一个目录就加成一个根_根名取目录名() {
    // 挂单 `Q890`（拿主意的人答：照稿）：屏头「添加根…」直接弹系统的选目录窗口，选中就加，根名取目录名。系统窗口测试点
    // 不了，选中之后那一半照 `tests/pick.rs` 的做法递一个固定路径进去（`roots::Screen::picked_root`）。
    let 乙 = 建库("gui-roots-选根-加上");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    let 历史几条 = 现场.app.tasks().history().len();

    {
        let (screen, site, _) = 现场.app.roots_site_and_tasks();
        screen.picked_root(site, Some(乙.path().to_path_buf()));
    }

    let 目录名 = romcat_core::path::normalize_existing(乙.path())
        .file_name()
        .expect("临时目录有名字")
        .to_string_lossy()
        .into_owned();
    assert!(
        现场
            .app
            .roots()
            .roots()
            .iter()
            .any(|row| row.root.name == 目录名),
        "选中的目录没加成根，或者根名不是目录名「{目录名}」"
    );
    let 屏上 = 库屏上的字(&ctx, &mut 现场.app);
    assert!(屏上.contains(&目录名), "新根没画在根那张表里：\n{屏上}");
    assert_eq!(
        现场.app.tasks().history().len(),
        历史几条,
        "加根不是任务，任务历史却多了一条"
    );
}

#[test]
fn 屏头添加根_加不上时核心库那句话画在屏上_任务历史不多一条() {
    // 同上：加不上（这里是同一个目录加第二次）时，**那句话在屏上说清为什么**——照票 `gui-looks-like-the-design/07` 被拒的
    // 写法，任务历史不多一条。
    let 甲 = 建库("gui-roots-选根-拦下");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.加根(甲.path(), "主库");
    let 历史几条 = 现场.app.tasks().history().len();

    {
        let (screen, site, _) = 现场.app.roots_site_and_tasks();
        screen.picked_root(site, Some(甲.path().to_path_buf()));
    }

    // 拦下时说的那句话是**核心库的原话**：同一个目录再问一遍核心库，拿它的原话来比，不抄一段字。
    let 核心库说的 = romcat_gui::roots::add_root_from_fields(
        &现场.app.site().catalog,
        现场.工作区.path(),
        &甲.path().to_string_lossy(),
        "",
    )
    .expect_err("同一个目录不该收两次");
    let 屏上 = 库屏上的字(&ctx, &mut 现场.app);
    assert!(
        屏上.contains(&核心库说的),
        "拦下的那句话没画在屏上：{核心库说的}\n{屏上}"
    );
    assert_eq!(现场.app.roots().roots().len(), 1, "被拦下却多了一个根");
    assert_eq!(
        现场.app.tasks().history().len(),
        历史几条,
        "被拒的那一下在任务历史里多了一条"
    );
}

#[test]
fn 屏头添加根_取消或弹不出选择窗口时屏上说去哪儿办() {
    // 同上：窗口交回 `None`——取消了，或者压根弹不出来（远程会话），两样分不开（`crate::pick` 的模块文档）——屏上说一句
    // 去哪儿办，一个根都不多、任务历史不多一条。
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    let 历史几条 = 现场.app.tasks().history().len();

    {
        let (screen, site, _) = 现场.app.roots_site_and_tasks();
        screen.picked_root(site, None);
    }

    let 屏上 = 库屏上的字(&ctx, &mut 现场.app);
    assert!(
        屏上.contains(romcat_gui::roots::PICK_ROOT_NONE),
        "没选到目录，屏上却没说去哪儿办：\n{屏上}"
    );
    assert!(
        现场.app.roots().roots().is_empty(),
        "没选到目录却多了一个根"
    );
    assert_eq!(现场.app.tasks().history().len(), 历史几条);
}

#[test]
fn 导出设置照稿_没有记下那颗_目录选中且格式挑过就当场记下() {
    // 挂单 `Q887`（拿主意的人答：逐字逐项照稿）：导出设置那一块没有「记下」，格式与目录两样齐了就当场记进中立库。
    // 「选择…」弹的系统窗口测试点不了，选中之后那一半递一个固定路径进去（`stages::Section::picked_export_dir`）。
    let 甲 = temp_dir("gui-导出设置-甲");
    let 乙 = temp_dir("gui-导出设置-乙");
    let mut 现场 = 现场::摆好();

    // **格式还没挑**：选中了目录只填进框里，不记——人还没挑完。
    {
        let (screen, site, _) = 现场.app.roots_site_and_tasks();
        screen
            .stages_mut()
            .picked_export_dir(site, Some(甲.path().to_path_buf()));
    }
    assert_eq!(
        现场.app.site().catalog.export_setup().expect("读得出"),
        None,
        "格式还没挑就记下了"
    );

    // **格式挑过了**（这里走记下那一条现成的路）：再选一个目录，当场记下新目录。
    现场.选一次导出去哪儿("Pegasus", 甲.path());
    {
        let (screen, site, _) = 现场.app.roots_site_and_tasks();
        screen
            .stages_mut()
            .picked_export_dir(site, Some(乙.path().to_path_buf()));
    }
    let 记着的 = 现场
        .app
        .site()
        .catalog
        .export_setup()
        .expect("读得出")
        .expect("记下了");
    assert_eq!(记着的.format, "Pegasus");
    assert_eq!(
        记着的.out,
        PathBuf::from(乙.path().to_string_lossy().as_ref()),
        "选中的新目录没当场记下"
    );
}

#[test]
fn 顶上那一行下一步指向该做的那一道_按钮点得动_跑完自动指向下一道() {
    // 票 `gui-looks-like-the-design/06` 验收第 2 条：人不必自己判断先做哪个。顶上那一行说
    // 「下一步：……」、说清那一道还差什么，旁边那颗按钮一按就办；**跑完它自己指向下一道**。
    // 指哪一道由核心库判（`Stages::next_up`，挂单 `Q823`），这一条只看屏上与按下去之后。
    let 库 = 建库("gui-stages-下一步");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.装上弹药();
    现场.加根(库.path(), "主库");

    // 加了根还没扫：下一步是扫描。**真点那颗按钮**（指针事件），不是直接调函数。
    let 屏上 = 库屏上的字(&ctx, &mut 现场.app);
    assert!(
        屏上.lines().any(|line| line.trim() == "下一步：扫描"),
        "加了根还没扫，顶上没指向扫描：\n{屏上}",
    );
    点一下(&ctx, &mut 现场.app, "开始扫描");
    现场.等任务跑完();

    // 扫完了：**自己指向识别**，并说清那一道还差什么。
    let 屏上 = 库屏上的字(&ctx, &mut 现场.app);
    assert!(
        屏上.lines().any(|line| line.trim() == "下一步：识别"),
        "扫完了，顶上没自己指向识别：\n{屏上}",
    );
    assert!(
        屏上
            .lines()
            .any(|line| line.trim() == "2 个变体连识别都还没跑过"),
        "顶上没说识别还差什么：\n{屏上}",
    );
    // 按钮上的字照稿（挂单 `Q881`）：顶上那颗与识别那一行都写「运行」，点到哪一颗都是同一个入口。
    点一下(&ctx, &mut 现场.app, "运行");
    现场.等任务跑完();

    let 屏上 = 库屏上的字(&ctx, &mut 现场.app);
    assert!(
        屏上.lines().any(|line| line.trim() == "下一步：刮削"),
        "识别跑完了，顶上没自己指向刮削：\n{屏上}",
    );
    点一下(&ctx, &mut 现场.app, "运行");
    现场.等任务跑完();

    let 屏上 = 库屏上的字(&ctx, &mut 现场.app);
    assert!(
        屏上.lines().any(|line| line.trim() == "下一步：整理标题"),
        "刮削跑完了，顶上没自己指向整理标题：\n{屏上}",
    );
    点一下(&ctx, &mut 现场.app, "运行");
    现场.等任务跑完();

    // DAT 库是空的：两个变体都没认出来，等着裁决。**裁决不排任务**，那颗按钮把人带去待确认队列屏。
    let 屏上 = 库屏上的字(&ctx, &mut 现场.app);
    assert!(
        屏上.lines().any(|line| line.trim() == "下一步：裁决"),
        "整理标题跑完了，顶上没自己指向裁决：\n{屏上}",
    );
    assert!(
        屏上
            .lines()
            .any(|line| line.trim() == "2 个变体在待确认队列里等着裁决"),
        "顶上没说裁决还差什么：\n{屏上}",
    );
    点一下(&ctx, &mut 现场.app, "去处理");
    跑一帧(&ctx, &mut 现场.app);
    assert_eq!(
        现场.app.view(),
        View::Queue,
        "按了去待确认队列，却没换到那一屏"
    );

    // 按过的那几下**真排上了任务台、真跑完了**：扫描、识别、刮削、整理标题各一趟。
    let 跑过的: Vec<&str> = 现场
        .app
        .tasks()
        .history()
        .iter()
        .rev()
        .map(|record| record.name.as_str())
        .collect();
    assert_eq!(
        跑过的,
        ["扫描 · 主库", "识别", "刮削", "整理标题"],
        "按下去的那几下排上去的不是这几趟",
    );
    for record in 现场.app.tasks().history() {
        assert!(
            matches!(record.ending, Ending::Done(_)),
            "「{}」记成了「{}」",
            record.name,
            record.ending.render(),
        );
    }
}

/// **关掉再打开**：同一个工作目录、同一份中立库，新开一个窗口本体。版式偏好在构造时从工作目录读出来。
fn 关掉再打开(现场: &mut 现场) {
    let 库文件 = 现场.工作区.path().join("catalog").join("fixture.sqlite3");
    let site = Site::open_file(现场.工作区.path(), &库文件, None).expect("开得出现场");
    现场.app = App::new(site, 现场.工作区.path().to_path_buf());
    现场.app.show_view(View::Library);
}

#[test]
fn 根_数据源_导出设置三块收得起来_关掉再打开还是收着的() {
    // 票 `gui-looks-like-the-design/06` 验收第 3 条：三块照设计稿排在工序段旁边，各自收得起来。
    // **收起来的样子记在工作目录的版式偏好里**（`layout.rs`，与面板边界同一份文件，不另起一份存储），
    // 关掉窗口再打开还是收着的。
    let 库 = 建库("gui-库屏-三块收起");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    let 根的位置 = 现场.app.roots().roots()[0].root.path.clone();
    let 源的名字 = 现场.app.roots().sources()[0].name.to_string();
    // 每一块里一段只在那一块里画的字：收起来之后它就不该再画出来。
    let 三块 = [
        ("根", 根的位置.as_str()),
        ("数据源", 源的名字.as_str()),
        ("导出设置", "选一个前端格式"),
    ];

    let 屏上 = 库屏上的字(&ctx, &mut 现场.app);
    for (标题, 里面的字) in 三块 {
        assert!(
            屏上.lines().any(|line| line.trim() == 标题),
            "库屏上没有「{标题}」那一块：\n{屏上}",
        );
        assert!(
            屏上.contains(里面的字),
            "「{标题}」那一块默认就收着：\n{屏上}"
        );
    }

    for (标题, _) in 三块 {
        点一下(&ctx, &mut 现场.app, 标题);
    }
    let 屏上 = 库屏上的字(&ctx, &mut 现场.app);
    for (标题, 里面的字) in 三块 {
        assert!(
            屏上.lines().any(|line| line.trim() == 标题),
            "收起之后连「{标题}」那一块的标题都没了——收起来的块得还点得开：\n{屏上}",
        );
        assert!(
            !屏上.contains(里面的字),
            "点了「{标题}」，那一块却没收起来：\n{屏上}",
        );
    }
    assert!(
        屏上.lines().any(|line| line.trim() == "识别"),
        "收起那三块，工序段也跟着没了：\n{屏上}",
    );

    // **关掉再打开**：换一个窗口本体、换一份 egui 上下文——记住它的只能是工作目录里那份文件。
    关掉再打开(&mut 现场);
    let ctx = headless::context();
    let 屏上 = 库屏上的字(&ctx, &mut 现场.app);
    for (标题, 里面的字) in 三块 {
        assert!(
            !屏上.contains(里面的字),
            "关掉再打开，「{标题}」那一块又摊开了：\n{屏上}",
        );
    }
    assert!(
        现场.app.layout().path().starts_with(现场.工作区.path()),
        "收起来的样子没记在工作目录里：{}",
        现场.app.layout().path().display(),
    );

    // **再点一下就摊开**，下次打开也记得是摊开的。
    点一下(&ctx, &mut 现场.app, "根");
    关掉再打开(&mut 现场);
    let 屏上 = 库屏上的字(&headless::context(), &mut 现场.app);
    assert!(
        屏上.contains(&根的位置),
        "摊开之后关掉再打开，根那一块还收着：\n{屏上}",
    );
}

#[test]
fn 还没扫描时每一块都有空态并写明下一步_一个根都没有时按扫描只在屏上说() {
    // 票 `gui-looks-like-the-design/06` 验收第 4 条：一份刚建出来、一个根都没有的库，库屏上**每一块**
    // 都得说话——空着的那一块说清下一步去哪儿办，而不是一片空白或者一张只有表头的表。
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    let 历史几条 = 现场.app.tasks().history().len();

    let 屏上 = 库屏上的字(&ctx, &mut 现场.app);
    // 工序段：下一步是扫描，而扫描之前得先添加根。
    assert!(
        屏上.lines().any(|line| line.trim() == "下一步：扫描"),
        "一个根都没有，顶上没指向扫描：\n{屏上}",
    );
    // 那一句照拿主意的人 2026-09-14 的答复写，出自核心库（`Stages::line`）。
    assert!(
        屏上.contains("还没有根，先点右上角「添加根…」选一个目录"),
        "工序段没说扫描之前得先添加根：\n{屏上}"
    );
    // 工序段后面那几行**不说空话**：一个变体都还没有，识别那一行不许说「每个变体都跑过识别了」，说它在等扫描。
    assert!(
        !屏上.contains("每个变体都跑过识别了") && 屏上.contains("等待扫描完成"),
        "还没扫描，工序段后面那几行在说空话：\n{屏上}",
    );
    // 根那一块：一个根都没有，说一句空态。去哪儿加由工序段扫描那一行说（上面那条），这一块只留后半句、不再重复
    // （拿主意的人 2026-09-14 答）。
    assert!(
        屏上.contains(romcat_gui::roots::ROOTS_EMPTY) && !屏上.contains("还没有根。按右上角"),
        "根那一块空着却没画那句空态，或者还在重复去哪儿加：\n{屏上}",
    );
    // 数据源那一块：还没扫描也能先取回，说清取回来做什么用。
    assert!(
        屏上.contains("还没扫描") && 屏上.contains("先把这几个源下载下来"),
        "数据源那一块没说还没扫描时下一步做什么：\n{屏上}",
    );
    // 导出设置那一块：还没选过，说清先选一次。第二段逐字照稿（挂单 `Q887`）之后这一块不再有「第一次导出之前先选一次」
    // 那句，下一步由下拉上那句「选一个前端格式」与底下稿上那句说明（`stages::EXPORT_HELP`）说。
    assert!(
        屏上.contains("选一个前端格式") && 屏上.contains(romcat_gui::stages::EXPORT_HELP),
        "导出设置那一块空着却没说下一步：\n{屏上}",
    );

    // **顶上那颗按钮按得动**：一个根都没有是按下去之前就判得出的，只在屏上说为什么不行、去哪儿办，
    // 任务历史不多一条压根没开跑的「失败」（票 `gui-looks-like-the-design/07`）。
    点一下(&ctx, &mut 现场.app, "开始扫描");
    let 屏上 = 库屏上的字(&ctx, &mut 现场.app);
    let 说的 = 现场
        .app
        .roots()
        .stages()
        .error()
        .expect("按了扫描却没说为什么扫不了");
    assert!(
        说的.contains("一个根都还没有") && 说的.contains("「添加根…」"),
        "没说清为什么扫不了、去哪儿加根：{说的}",
    );
    assert!(
        屏上.contains(说的),
        "那句话没画在屏上：{说的}\n屏上：\n{屏上}"
    );
    历史没多一条(&现场.app, 历史几条);
}

/// 这一帧里**正好**画着 `那几个字` 的那一段排成了几行；没画就是 `None`。
fn 那一段排成几行(output: &egui::FullOutput, 那几个字: &str) -> Option<usize> {
    fn 找(shape: &egui::epaint::Shape, 那几个字: &str) -> Option<usize> {
        match shape {
            egui::epaint::Shape::Text(text) => {
                (text.galley.text() == 那几个字).then(|| text.galley.rows.len())
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|one| 找(one, 那几个字)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|clipped| 找(&clipped.shape, 那几个字))
}

#[test]
fn 根名很长时两行根都在屏上_根名截断成一行_路径那一列不被挤窄() {
    // 挂单 `Q911`（拿主意的人 2026-09-14：截断加悬停）：根名照稿取目录名，目录名可能很长。根名称那一列宽不超过令牌
    // `root-name-max`，超过就截断成一行、末尾「…」、悬停看全名；路径那一列始终留得出地方，那一行不被撑高。
    //
    // **比的是同一对目录、两个都超过上限的根名**：根名那一列在上限之内本来就跟着名字变宽（短名时只有表头那么宽），
    // 那不是被撑坏；要钉的是**过了上限再长，路径那一列不再变窄、那一行不再变高**。从前根名不截断时，名字长一截，
    // 路径就挤窄一截，一直挤到一个字一行。
    let 甲 = 建库("gui-roots-长根名-甲");
    let 乙 = 建库("gui-roots-长根名-乙");
    let 过了上限的名 = "这个根目录的名字已经超过上限";
    let 长名 = "这是一个名字特别特别特别特别特别特别特别特别特别特别长的根目录";

    // 摆两个根（乙那个叫 `乙的名字`），跑稳之后画一帧：交回乙的路径排成几行、乙的根名排成几行。
    let 画一遍 = |乙的名字: &str| {
        let ctx = headless::context();
        let mut 现场 = 现场::摆好();
        现场.加根(甲.path(), "甲");
        现场.加根(乙.path(), 乙的名字);
        跑一帧(&ctx, &mut 现场.app);
        let 这一帧 = headless::frame(&ctx, headless::input(), |ui| 现场.app.ui(ui));
        let 屏上 = 画出来的字(&这一帧);
        let 路径 = |名字: &str| {
            现场
                .app
                .roots()
                .roots()
                .iter()
                .find(|row| row.root.name == 名字)
                .expect("两个根都加上了")
                .root
                .path
                .clone()
        };
        let (甲的路径, 乙的路径) = (路径("甲"), 路径(乙的名字));
        assert!(
            屏上.contains(&甲的路径) && 屏上.contains(&乙的路径),
            "两行根没都画在屏上：\n{屏上}"
        );
        (
            那一段排成几行(&这一帧, &乙的路径).expect("画了乙的路径"),
            那一段排成几行(&这一帧, 乙的名字),
        )
    };

    let (刚过上限时路径几行, 刚过上限几行) = 画一遍(过了上限的名);
    let (长名时路径几行, 长名几行) = 画一遍(长名);
    assert_eq!(刚过上限几行, Some(1), "过了上限的根名没截断成一行");
    assert_eq!(长名几行, Some(1), "很长的根名没截断成一行");
    assert_eq!(
        长名时路径几行, 刚过上限时路径几行,
        "根名过了上限再长，路径那一列还在变窄、那一行还在变高：刚过上限时路径排 {刚过上限时路径几行} 行，\
         很长时排 {长名时路径几行} 行",
    );
}

#[test]
fn 在待确认队列屏上裁完一批再回库屏_裁决那一行跟着变() {
    // 审查 Spec 轴报的：裁决那一行说的是待确认队列里还有几个变体等着裁决，与待确认队列屏同一个数
    // （挂单 `Q822`）。人在那一屏上落下一批、回到库屏，那一行得跟着变——不然库屏上说的是裁之前的数，
    // 顶上「下一步」也还指着裁决。
    let 库 = 建库("gui-stages-裁完回来");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.装上弹药();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    现场.跑识别();
    let 裁决那一行 = |app: &App| {
        app.roots()
            .stages()
            .of(Stage::Triage)
            .expect("工序段有裁决那一行")
            .behind
            .clone()
    };
    assert_eq!(
        裁决那一行(&现场.app),
        Behind::Left(2),
        "前提：DAT 库是空的，两个变体都等着裁决",
    );

    // 到待确认队列屏上，把头一批整批判成「认不出」并落下——那一屏上「整批拒绝」与「落下」走的就是这两下。
    现场.app.show_view(View::Queue);
    跑一帧(&ctx, &mut 现场.app);
    let 那一批 = Scope::whole(现场.app.queue().queue().batches()[0].shape.clone());
    let 这一批几个 = 现场.app.queue().queue().count(&那一批);
    assert!(这一批几个 > 0, "前提：那一批里有东西");
    {
        let (screen, site) = 现场.app.queue_and_site();
        screen.reject(site, &那一批);
        screen.commit(site);
    }
    assert_eq!(
        现场.app.queue().queue().pending(),
        2 - 这一批几个,
        "前提：待确认队列屏落下了那一批",
    );

    // 回库屏：那一行与待确认队列屏说的是同一个数。
    现场.app.show_view(View::Library);
    let 屏上 = 库屏上的字(&ctx, &mut 现场.app);
    assert_eq!(
        裁决那一行(&现场.app),
        Behind::Left(2 - 这一批几个),
        "裁完一批回到库屏，裁决那一行还是裁之前的数：\n{屏上}",
    );
}

#[test]
fn 重排之后刮削口径挨着刮削那一行画_铺媒体开关挨着导出那一行画() {
    // 票 `gui-looks-like-the-design/06` 验收第 5 条、收挂单 `Q554`：前几张票写上屏的话一句不丢，
    // 而且**挨着它说的那一行画**——口径说的是刮削那一个数，画到整张表底下，人读到它时已经不知道它
    // 说的是哪一行；铺媒体那颗开关是导出这一趟的旋钮，也跟着导出那一行。
    let 库 = 建库("gui-stages-挨着那一行");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");

    let 屏上 = 库屏上的字(&ctx, &mut 现场.app);
    let 行: Vec<&str> = 屏上.lines().map(str::trim).collect();
    let 第几行 = |说的: &str, 要找: &dyn Fn(&str) -> bool| {
        行.iter()
            .position(|line| 要找(line))
            .unwrap_or_else(|| panic!("屏上没有{说的}：\n{屏上}"))
    };
    let 刮削 = 第几行("刮削那一行", &|line| line == "刮削");
    let 口径 = 第几行("刮削那一行的口径", &|line| {
        line.contains(romcat_core::stage::SCRAPE_BASIS)
    });
    let 整理标题 = 第几行("整理标题那一行", &|line| line == "整理标题");
    assert!(
        刮削 < 口径 && 口径 < 整理标题,
        "口径没挨着刮削那一行画：刮削在第 {刮削} 段、口径第 {口径} 段、整理标题第 {整理标题} 段\n{屏上}",
    );

    let 导出 = 第几行("导出那一行", &|line| line == "导出");
    let 开关 = 第几行("铺媒体那颗开关", &|line| {
        line == romcat_gui::stages::LAY_MEDIA
    });
    assert!(
        导出 < 开关,
        "铺媒体那颗开关画在导出那一行前面：导出第 {导出} 段、开关第 {开关} 段\n{屏上}",
    );
    assert!(
        !行[导出..开关]
            .iter()
            .any(|line| line.contains(romcat_core::stage::SCRAPE_BASIS)),
        "导出那一行与铺媒体开关之间隔着别的话：\n{屏上}",
    );
}

#[test]
fn 点一下把刮削排上任务台_跑完那一行的数跟着变_主库一个字节都没动() {
    // 验收第 4 条：按那一行的按钮排一趟刮削上任务台，跑完那一行的数跟着变。
    // **一个请求都不发**：这一趟没给凭据，工序段排出去的那一趟也只用本地源。
    let 库 = 建库("gui-stages-刮削跑一趟");
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    let 刮之前 = 主库快照(库.path());
    assert_eq!(现场.刮削那一行().behind, Behind::Left(2), "前提：还没刮过");

    现场.跑刮削();

    // 任务台上留下一条**跑完了**的历史，名字就是这道工序的名字。
    let record = &现场.app.tasks().history()[0];
    assert_eq!(record.name, "刮削");
    assert!(
        matches!(record.ending, Ending::Done(_)),
        "跑完的那一趟记成了「{}」",
        record.ending.render(),
    );
    // **那一行当场刷新**：不必再点一次什么。
    assert_eq!(现场.刮削那一行().behind, Behind::Left(0));
    let 说的 = 现场.app.roots().stages().notice().expect("跑完了也要说话");
    assert!(说的.starts_with("刮削跑完了"), "{说的}");
    assert!(!说的.contains("联网"), "工序段排的那一趟发了请求：{说的}");
    // **主库一个字节都没动**（ADR-0004）。
    assert_eq!(主库快照(库.path()), 刮之前, "刮一趟动了主库");
}

#[test]
fn 刮削这一趟正在跑的时候那一行的按钮按不下去() {
    // 验收第 6 条，照识别那一行的先例：不禁掉的话同一趟刮削会被排两遍。
    let 库 = 建库("gui-stages-刮削按不下去");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    let 占位 = 现场.占住任务台();

    现场.app.start_stage(Stage::Scrape);
    let id = 现场
        .app
        .roots()
        .stages()
        .task_of(Stage::Scrape)
        .expect("这一趟排上任务台了");

    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    assert!(
        屏上.lines().any(|line| line.trim() == "跑着呢"),
        "刮削还在台上，那一行的按钮却还写着「运行」：\n{屏上}",
    );
    // **只禁它自己那一行**：下一步那一行照旧按得下去。票 `gui-looks-like-the-design/06` 第二段照稿之后（挂单
    // `Q827`、`Q881`），在等前面那一道的几行不给按钮、做完的扫描那一行写「重新扫描」，于是写着「运行」的只剩
    // 两颗：顶上「下一步」那一颗，与下一步识别那一行自己那一颗。
    assert_eq!(
        屏上.lines().filter(|line| line.trim() == "运行").count(),
        2,
        "禁掉的不只是刮削那一行：\n{屏上}",
    );

    // 再按一次：**什么都不该发生**。
    现场.app.start_stage(Stage::Scrape);
    assert_eq!(
        现场.app.tasks().queued().len(),
        1,
        "同一趟刮削被排了两遍：{:?}",
        现场.app.tasks().queued(),
    );
    assert_eq!(
        现场.app.roots().stages().task_of(Stage::Scrape),
        Some(id),
        "第二次按下换掉了台上那一趟",
    );

    现场.app.tasks_mut().stop(id);
    现场.app.tasks_mut().stop(占位);
    现场.等任务跑完();
}

#[test]
fn 刮削排着队被撤掉时记成已取消_一条刮削结论都没多() {
    // 验收第 5 条「四档收场照旧分得开」里的第二档：还没开采就停下，什么都没留下。
    // 与导出那一支同一个验法——排在一趟占位的活后面，撤掉它。
    let 库 = 建库("gui-stages-刮削撤掉");
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    let 占位 = 现场.占住任务台();

    现场.app.start_stage(Stage::Scrape);
    let id = 现场
        .app
        .roots()
        .stages()
        .task_of(Stage::Scrape)
        .expect("这一趟排上任务台了");
    现场.app.tasks_mut().stop(id);
    现场.app.poll_tasks();

    let record = 现场
        .app
        .tasks()
        .history()
        .iter()
        .find(|one| one.id == id)
        .expect("撤掉的那一趟也进历史");
    assert!(
        matches!(record.ending, Ending::Stopped),
        "撤掉的那一趟记成了「{}」——它一个锚点都没采",
        record.ending.render(),
    );
    assert_eq!(
        现场.刮削那一行().behind,
        Behind::Left(2),
        "撤掉的那一趟写了库"
    );
    let 说的 = 现场.app.roots().stages().notice().expect("停下了也要说话");
    assert!(说的.starts_with("刮削"), "{说的}");
    assert!(!说的.contains("跑完了"), "停下的那一趟说成了跑完了：{说的}");
    // 那一行的按钮又按得下去了。
    assert!(现场.app.roots().stages().task_of(Stage::Scrape).is_none());

    现场.app.tasks_mut().stop(占位);
    现场.等任务跑完();
}

#[test]
fn 刮削读不动优先级表时记成失败_并说清停在哪一步() {
    // 验收第 5 条的第四档，**也是「报得出跑到哪儿」的落点**：任务台记下来的那句「停在哪一步」
    // 取自把手上的进度，对得上就说明这一趟的 `task.step` 真的报出去了——与折标题那一条
    // 同一个验法。**不静默退回内置那份表**：挑哪个源的值说了算的正是它。
    let 库 = 建库("gui-stages-刮削坏表");
    let mut 现场 = 现场::摆好();
    写(
        &现场.工作区.path().join("priorities.toml"),
        "这不是一份 TOML".as_bytes(),
    );
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    现场.跑刮削();

    let record = &现场.app.tasks().history()[0];
    assert_eq!(record.name, "刮削");
    let Ending::Failed { step, why } = &record.ending else {
        panic!("表都读不动却把这一趟记成了「{}」", record.ending.render());
    };
    assert_eq!(step, "读优先级表", "说不清停在哪一步");
    assert!(!why.is_empty(), "说不清为什么跑不了");
    let 说的 = 现场.app.roots().stages().error().expect("失败要说出来");
    assert!(说的.contains("读优先级表"), "{说的}");
    // **失败不算刮过**：那一行照旧是两个。
    assert_eq!(现场.刮削那一行().behind, Behind::Left(2));
}

#[test]
fn 选一次前端格式与目录之后_点一下就重导_而主库里的东西一个字节都没动() {
    // 验收第 2、3、4、6 条一条线走完：选一次记进中立库 → 点一下排上任务台 →
    // 元数据落在导出目录里 → **主库那几个 ROM 一个字节都没动**（ADR-0004）。
    let 库 = 建库("gui-stages-导出跑一趟");
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    let 导出去 = 现场.工作区.path().join("导出去");
    let 主库原样 = 主库快照(库.path());

    现场.选一次导出去哪儿("Pegasus", &导出去);
    assert!(现场.app.roots().stages().error().is_none());
    // **记进了中立库**：这一份就是下一趟不必再选的依据。
    let 记下的 = 现场
        .app
        .site()
        .catalog
        .export_setup()
        .expect("读得出")
        .expect("记下了");
    assert_eq!(记下的.format, "Pegasus");
    assert_eq!(记下的.out, 导出去);

    现场.导出();

    // 元数据真的写出去了。
    let 写出来的: Vec<PathBuf> = fs::read_dir(&导出去)
        .expect("导出目录建出来了")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file())
        .collect();
    assert!(!写出来的.is_empty(), "一份元数据都没写出来");

    // **主库一个 ROM 都没搬、一个字节都没改**（验收第 6 条逐字要求）。
    assert_eq!(主库快照(库.path()), 主库原样, "导出动了主库里的东西");

    // 任务台历史上留下一条**跑完了**，名字就是那道工序的名字。
    let record = &现场.app.tasks().history()[0];
    assert_eq!(record.name, "导出");
    assert!(
        matches!(record.ending, Ending::Done(_)),
        "跑完的那一趟记成了「{}」",
        record.ending.render(),
    );

    // 那一行改口说「上次跑是 ⋯」，而**识别那一行照旧报数**（验收第 1 条）。
    let 那一句 = 现场.导出那一行().render();
    assert!(那一句.contains("上次跑是"), "{那一句}");
    assert_eq!(
        现场.识别那一行().behind,
        Behind::Left(2),
        "导出退回时刻，把识别那一支也一起降级了",
    );

    // **下一趟不必再选**：直接再点一下就重导（验收第 3 条）。
    现场.导出();
    assert!(现场.app.roots().stages().error().is_none());
    assert_eq!(
        现场
            .app
            .tasks()
            .history()
            .iter()
            .filter(|record| record.name == "导出")
            .count(),
        2,
        "第二趟没排上台——「下一趟不必再选」没兑现",
    );
}

#[test]
fn 外面有人动过那些元数据文件时停下来_不静默覆盖() {
    // 验收第 5 条。判据是**底本**：上次我们写出去的那份是这个哈希，盘上那份不是，
    // 那就是有人在外面动过（`catalog::frontend` 的模块文档）。
    let 库 = 建库("gui-stages-导出撞手改");
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    let 导出去 = 现场.工作区.path().join("导出去");
    现场.选一次导出去哪儿("Pegasus", &导出去);
    现场.导出();

    // 有人在工具外面改了它。
    let 落点 = fs::read_dir(&导出去)
        .expect("导出目录在")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .find(|path| path.is_file())
        .expect("写出来了一份");
    let mut 手改的 = fs::read_to_string(&落点).expect("读得出");
    手改的.push_str("\n# 我后来手加的一行\n");
    fs::write(&落点, &手改的).expect("写得进");

    现场.导出();

    assert_eq!(
        fs::read_to_string(&落点).expect("读得出"),
        手改的,
        "**没有静默覆盖**：手改的那一行还在",
    );
    let 说的 = 现场
        .app
        .roots()
        .stages()
        .error()
        .expect("撞上手改要说出来，不能默默跳过");
    assert!(说的.contains("动过"), "没说清为什么没写：{说的}");
    // **「跑完了」那一句数的是真写出去的那几份**：这个库横跨两个平台、收敛成两份，
    // 手改的是其中一份，于是这一趟只写成了另一份。报「2 份」就是虚报——被挡下的那一份
    // 一个字节都没写。
    let 那一句 = 现场.app.roots().stages().notice().expect("跑完了也要说话");
    assert!(
        那一句.contains("写进 1 份"),
        "写出去的份数报错了（被挡下的那份也算进去了？）：{那一句}",
    );
}

#[test]
fn 外面有人动过时这一趟停下来_屏上逐份点名是哪几份() {
    // 票 `gui-answers-all-six/05` 验收第 1 条：导一趟 → 改掉盘上那一份 → 再导。
    // **名单是导出本来就交得出的那一份**（`ExportReport::conflicts`），界面不另比一遍。
    // 这个库收敛成两份，只改其中一份：屏上点名的得是**那一份**，没动过的那份不许混进来。
    let (_库, mut 现场, 导出去) = 导过一趟("gui-stages-导出点名");
    let ctx = headless::context();
    let 写出来的 = 导出去的文件(&导出去);
    assert_eq!(写出来的.len(), 2, "这个库横跨两个平台：{写出来的:?}");
    let (动过的, 没动的) = (&写出来的[0], &写出来的[1]);
    let 手改的 = 手改一行(动过的);

    现场.导出();

    assert_eq!(
        fs::read_to_string(动过的).expect("读得出"),
        手改的,
        "没有静默覆盖：手改的那一行还在",
    );
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    assert!(
        屏上.contains(&动过的.display().to_string()),
        "屏上没点名被动过的那一份：\n{屏上}",
    );
    assert!(
        !屏上.contains(&没动的.display().to_string()),
        "没人动过的那一份也被点了名：\n{屏上}",
    );
    // **为什么没写也画在那一份底下**：「有人在外面改过」与「工具从没见过、可能就是原件」
    // 要人去看的东西不一样。
    assert!(
        屏上.contains("有人在工具外面改过它"),
        "点了名却没说为什么没写：\n{屏上}",
    );
}

#[test]
fn 看过之后按一下我看过了照写_那几份真的被写过去了() {
    // 验收第 2 条：屏上有一颗「我看过了，照写」，**真点一下**（指针事件，不是直接调函数），
    // 那一份就被写过去了。**丢掉手改也得说出口**：照写掉了哪几份，回执里逐份点名
    // （`ExportReport::forced`——「不静默」说的是不许悄悄发生，不是不许发生）。
    let (_库, mut 现场, 导出去) = 导过一趟("gui-stages-导出照写");
    let ctx = headless::context();
    let 动过的 = 导出去的文件(&导出去)[0].clone();
    手改一行(&动过的);
    现场.导出();

    点一下(&ctx, &mut 现场.app, "我看过了，照写");
    现场.等任务跑完();

    let 现在的 = fs::read_to_string(&动过的).expect("读得出");
    assert!(
        !现在的.contains("我后来手加的一行"),
        "按了照写，手改的那一行却还在：\n{现在的}",
    );
    let 回执 = 现场
        .app
        .roots()
        .stages()
        .notice()
        .expect("照写那一趟也要说话");
    assert!(
        回执.contains(&动过的.display().to_string()),
        "照写掉了哪一份没说出口：{回执}",
    );
    assert!(
        现场.app.roots().stages().error().is_none(),
        "照写过去之后还挂着「有几份没写」",
    );
    // 任务台历史上看得出**这一趟是照写的**：丢掉手改的那一趟不许与平常那几趟长得一样。
    // 历史**最近的在前面**（`Board::history`）。
    let 最后一趟 = 现场.app.tasks().history().first().expect("进了历史");
    assert!(
        最后一趟.name.contains("照写"),
        "历史上看不出这一趟是照写的：{}",
        最后一趟.name,
    );
}

#[test]
fn 照写那颗按钮不记状态_下一趟撞上同样的事还得再点一次() {
    // 验收第 3 条。照写丢掉的是人的一次手改，**每次都得当场点**：按过一次之后，
    // 下一趟平常的导出撞上外面有人动过，照样停下来、照样点名、照样一个字节不写。
    let (_库, mut 现场, 导出去) = 导过一趟("gui-stages-导出照写不记");
    let ctx = headless::context();
    let 动过的 = 导出去的文件(&导出去)[0].clone();
    手改一行(&动过的);
    现场.导出();
    点一下(&ctx, &mut 现场.app, "我看过了，照写");
    现场.等任务跑完();

    // 又有人在外面动了它，然后人照平常那样点一下导出。
    let 又改的 = 手改一行(&动过的);
    现场.导出();

    assert_eq!(
        fs::read_to_string(&动过的).expect("读得出"),
        又改的,
        "上一趟按过照写，这一趟就静默覆盖了——照写被记住了",
    );
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    assert!(
        屏上.contains(&动过的.display().to_string()),
        "这一趟没再点名被动过的那一份：\n{屏上}",
    );
    assert!(
        屏上.lines().any(|line| line.trim() == "我看过了，照写"),
        "照写那颗按钮没再摆出来，人没处当场点：\n{屏上}",
    );
    // 「每次都得当场点」这句话**画在屏上**，不只藏在悬停里。
    assert!(
        屏上.contains("只管这一趟"),
        "屏上没说清照写只管这一趟：\n{屏上}",
    );
    let 最后一趟 = 现场.app.tasks().history().first().expect("进了历史");
    assert_eq!(最后一趟.name, "导出", "平常那一趟被记成了照写");
}

#[test]
fn 没人动过任何文件时_导出一路走完不多问一句() {
    // 验收第 4 条：**默认一个字不变**。没人动过的时候不摆照写那颗按钮、不挂红字，
    // 点一下导出就是一趟——不多一次点击。
    let (_库, mut 现场, _导出去) = 导过一趟("gui-stages-导出不多问");
    let ctx = headless::context();
    现场.导出();

    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    assert!(
        !屏上.lines().any(|line| line.trim() == "我看过了，照写"),
        "没人动过却摆出了照写那颗按钮：\n{屏上}",
    );
    assert!(
        现场.app.roots().stages().error().is_none(),
        "没人动过却挂着一句红字",
    );
    let 那一句 = 现场.app.roots().stages().notice().expect("跑完了要说话");
    assert!(那一句.contains("写进 2 份"), "两份都该写出去：{那一句}");
    // **点两下就是两趟**，每一趟都一路走完：没有哪一趟停下来等人再点一次。
    let 历史 = 现场.app.tasks().history();
    assert_eq!(历史.len(), 3, "扫描一趟、导出两趟：{历史:?}");
    for record in 历史.iter().filter(|record| record.name.starts_with("导出")) {
        assert_eq!(record.name, "导出", "没人动过却排了一趟照写");
        assert!(
            matches!(record.ending, Ending::Done(_)),
            "没人动过的那一趟记成了「{}」",
            record.ending.render(),
        );
    }
}

#[test]
fn 停下那一趟不打上次导出的时刻_照写那一趟走完之后那一行的时刻跟着更新() {
    // 验收第 5、6 条。**不靠挂钟**：时刻戳是秒级的，「导一趟 → 改 → 再导」那条路上
    // 第二趟有没有重打，同一秒里分不出来。于是让撞上的那一趟就是头一趟：导出目录里
    // 先躺着一份工具从没见过的文件（那可能就是维护者的原件），这一趟写成了另一份、
    // 挡下了这一份——**只写了一半，说不上导过了**。
    let 库 = 建库("gui-stages-导出时刻戳");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    let 导出去 = 现场.工作区.path().join("导出去");
    现场.选一次导出去哪儿("Pegasus", &导出去);
    let 原件 = 导出去.join("FC.metadata.pegasus.txt");
    写(&原件, "# 维护者自己手写的\n".as_bytes());

    现场.导出();

    assert_eq!(
        导出去的文件(&导出去).len(),
        2,
        "没人动过的那一份该照常写出去",
    );
    assert_eq!(
        fs::read_to_string(&原件).expect("读得出"),
        "# 维护者自己手写的\n",
        "没有静默覆盖：原件还在",
    );
    assert_eq!(
        现场.app.site().catalog.exported_at().expect("读得出"),
        None,
        "停下那一趟打了「上次导出」的时刻戳——只写了一半说不上导过了",
    );
    assert!(
        现场.导出那一行().render().contains("还没跑过"),
        "{}",
        现场.导出那一行().render(),
    );

    点一下(&ctx, &mut 现场.app, "我看过了，照写");
    现场.等任务跑完();

    assert!(
        现场
            .app
            .site()
            .catalog
            .exported_at()
            .expect("读得出")
            .is_some(),
        "照写那一趟走完了却没打时刻戳",
    );
    let 那一句 = 现场.导出那一行().render();
    assert!(
        那一句.contains("上次跑是"),
        "工序段导出那一行没跟着更新：{那一句}"
    );
    // **画在屏上的那一行也跟着变**，不只是数据结构里那一格。票 `gui-looks-like-the-design/06` 第二段照稿之后（挂单
    // `Q827`），前面还有一道没做完时导出那一行不画它自己那一句、说在等谁（这一份库只扫过、识别没跑过，说「等待识别
    // 完成」），于是屏上断的是工序段**从刷新过的那几行现折**给导出那一行的那句话（`Stages::line`）。
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    let 屏上那一句 = 现场.app.roots().stages().line(&现场.导出那一行());
    assert!(
        屏上.contains(&屏上那一句),
        "屏上导出那一行不是工序段眼下说的那一句「{屏上那一句}」：\n{屏上}"
    );
}

#[test]
fn 还没选过格式与目录时点导出_当场说清而不是默默不动_任务历史不多一条() {
    // **排一趟活的入口只有一个**（`App::start_stage`），票 `09` 的捷径走的也是它。
    // 那时人可能一次都没选过——默默不动的话，他会以为按钮坏了。
    //
    // **没选过是按下去之前就判得出的**（票 `gui-looks-like-the-design/07`）：不往任务台上排，
    // 屏上说一句缺什么、去哪儿选；任务历史里不多一条压根没开跑的「失败」。
    let 库 = 建库("gui-stages-导出没选过");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    let 历史几条 = 现场.app.tasks().history().len();

    现场.导出();
    跑一帧(&ctx, &mut 现场.app);
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    let 说的 = 现场.app.roots().stages().error().expect("该说清");
    assert!(说的.contains("格式"), "没说清缺的是什么：{说的}");
    assert!(说的.contains("目录"), "没说清缺的是什么：{说的}");
    assert!(说的.contains("「导出设置」那一块"), "没说去哪儿选：{说的}");
    assert!(
        屏上.contains(说的),
        "那句话没画在屏上：{说的}\n屏上：\n{屏上}"
    );
    assert!(
        !说的.starts_with("导出 失败"),
        "没开跑的那一下说成了失败：{说的}"
    );
    历史没多一条(&现场.app, 历史几条);
    // 一个字节都没写出去。
    assert_eq!(现场.app.site().catalog.exported_at().expect("读得出"), None);
}

#[test]
fn 记着的前端格式这一版没有时点导出_当场说清去哪儿重选_任务历史不多一条() {
    // 换了一版程序、或者库里记着的是这一版没带的格式：配置读得出来，只是**那个格式这一版没有
    // 适配器**。这一样按下去之前就判得出（判据在核心里，`ExportSetup::adapter`），不排上去
    // 再记一条失败（票 `gui-looks-like-the-design/07`）。
    let 库 = 建库("gui-stages-导出没这个格式");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    let 导出去 = 现场.工作区.path().join("导出去");
    {
        let (screen, site, _) = 现场.app.roots_site_and_tasks();
        site.catalog
            .set_export_setup(&romcat_core::catalog::ExportSetup {
                format: "这一版没带的格式".to_string(),
                out: 导出去.clone(),
            })
            .expect("写得进");
        screen.reload(site);
    }
    let 历史几条 = 现场.app.tasks().history().len();

    现场.导出();
    跑一帧(&ctx, &mut 现场.app);
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    let 说的 = 现场.app.roots().stages().error().expect("该说清");
    assert!(
        说的.contains("这一版没带的格式"),
        "没说清是哪个格式：{说的}"
    );
    assert!(
        说的.contains("「导出设置」那一块"),
        "没说去哪儿重选：{说的}"
    );
    assert!(
        屏上.contains(说的),
        "那句话没画在屏上：{说的}\n屏上：\n{屏上}"
    );
    历史没多一条(&现场.app, 历史几条);
    assert!(!导出去.exists(), "没开跑却建出了导出目录");
}

#[test]
fn 还没选过导出配置就打开铺媒体_不排那一趟去算_屏上说清去哪儿选_选好之后自己算() {
    // 媒体的布局随前端格式不同：没选过，「这一趟最多要铺多少」一定算不出来。那是打开开关之前
    // 就判得出的——排上去再记一条失败，任务历史里就多一条压根没开跑的「失败」
    // （票 `gui-looks-like-the-design/07`）。屏上那句要说清缺什么、去哪儿选；**选好之后自己算**，
    // 不逼人关掉再打开。
    let 库 = 建库("gui-stages-铺媒体没选过");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    let 历史几条 = 现场.app.tasks().history().len();

    现场.打开铺媒体();
    跑一帧(&ctx, &mut 现场.app);
    滚到库屏底下(&ctx, &mut 现场.app);
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    assert!(
        屏上.contains("还没选过导出的前端格式与目录") && 屏上.contains("「导出设置」那一块"),
        "屏上没说清缺什么、去哪儿选：\n{屏上}",
    );
    历史没多一条(&现场.app, 历史几条);

    现场.选一次导出去哪儿("Pegasus", &现场.工作区.path().join("导出去"));
    跑一帧(&ctx, &mut 现场.app);
    现场.等任务跑完();
    滚到库屏底下(&ctx, &mut 现场.app);
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    assert!(
        屏上.contains(&romcat_gui::stages::media_cost(0, 0)),
        "选好之后没自己算：\n{屏上}",
    );
    let record = &现场.app.tasks().history()[0];
    assert_eq!(record.name, romcat_gui::stages::COUNT_MEDIA);
    assert!(
        matches!(record.ending, Ending::Done(())),
        "选好之后那一趟记成了「{}」",
        record.ending.render(),
    );
}

#[test]
fn 导出不出现在子库屏上_也没占用子库那套选择集与清单() {
    // 验收第 7 条。词表里**子库**是「从主库挑选**一部分**导出到某个**目标设备**形成的
    // **派生库**」，而导出三个限定词一个都不满足：不挑选（整库级、作品级收敛）、
    // 没有目标设备、不是派生库。做成一个特殊子库的话，「子库」就从「给掌机的一份派生」
    // 稀释成「任何一次往外写」，而**清单**——工具在目标设备上的行为边界——在主库根上
    // 根本没有边界可划。
    let 库 = 建库("gui-stages-导出不是子库");
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    现场.选一次导出去哪儿("Pegasus", &现场.工作区.path().join("导出去"));
    现场.导出();

    let (sublibrary, site) = 现场.app.sublibrary_and_site();
    sublibrary.reload(site);
    assert!(
        现场.app.sublibrary().list().is_empty(),
        "导出在子库屏上多出了一行：{:?}",
        现场.app.sublibrary().list(),
    );
}

#[test]
fn 导出排着队被撤掉时一份元数据都没写出去() {
    // **规格 Testing Decisions 要求三支各走一遍这两条**：排一趟等它收场（上面那条），
    // 与排一趟当场按停。这一条是后者——按停在**一份都还没写**那个位置上，记的是
    // 「停了，什么都没留下」而不是**停在半路**（写过之后再停才是那一档，那条钉在
    // `romcat-core` 的 `tests/stage.rs` 上，靠一个「写出第一份就按停」的适配器把那一下
    // 钉死，不靠挂钟）。
    //
    // **台上先摆一趟别的活**：任务台一次只跑一趟，于是排上去的导出稳稳地停在队列里
    // ——不靠「恰好还没跑完」那种挂钟彩票（挂单 `Q196` / `Q349`）。
    let 库 = 建库("gui-stages-导出按停");
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    let 导出去 = 现场.工作区.path().join("导出去");
    现场.选一次导出去哪儿("Pegasus", &导出去);

    let 占位 = 占位活::排上(现场.app.tasks_mut(), "装作在扫一趟库");
    现场.app.start_stage(Stage::Export);
    let id = 现场
        .app
        .roots()
        .stages()
        .task_of(Stage::Export)
        .expect("这一趟排上任务台了");
    现场.app.tasks_mut().stop(id);
    现场.app.poll_tasks();

    let record = 现场
        .app
        .tasks()
        .history()
        .iter()
        .find(|one| one.id == id)
        .expect("撤掉的那一趟也进历史");
    assert!(
        matches!(record.ending, Ending::Stopped),
        "撤掉的那一趟记成了「{}」——它一份都没写，不是停在半路",
        record.ending.render(),
    );
    assert!(!导出去.exists(), "一份都没写的那一趟却建出了导出目录");
    assert_eq!(现场.app.site().catalog.exported_at().expect("读得出"), None);
    // 那一行的按钮又按得下去了。
    assert!(现场.app.roots().stages().task_of(Stage::Export).is_none());

    占位.按停(现场.app.tasks_mut());
    现场.等任务跑完();
}

/// 跑一帧队列屏，返回**屏上那些字**。
///
/// 队列屏那四处从前写的都是「先跑一次 `romcat identify`」，而它们分住在三种状态里
/// （顶栏、一级分批那张空态、逐条那张空态、以及「还没识别」那句警告），
/// 底下几条各自把屏切到那一种再看一遍。
fn 队列屏上(ctx: &egui::Context, 现场: &mut 现场) -> String {
    现场.app.show_view(View::Queue);
    画出来的字(&headless::frame(ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }))
}

#[test]
fn 队列屏那几处空态不再指向命令行_就地摆着一颗跑识别() {
    // **留一处旧文案，人就照着去开终端了**（规格 Further Notes）。所以这一条把三种
    // 状态各画一遍：顶栏、一级分批那张空态、逐条那张空态——三处从前是同一句
    // 「先跑一次 `romcat identify`」。
    let 库 = 建库("gui-捷径-空态");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    assert!(
        !现场.app.queue().queue().identified(),
        "前提：这份库还没跑过识别",
    );

    // ── 一、一级分批那一档（打开就是它）连顶栏。
    let 屏上 = 队列屏上(&ctx, &mut 现场);
    assert!(
        !屏上.contains("romcat identify"),
        "队列屏还在叫人去开终端：\n{屏上}",
    );
    assert!(
        屏上.lines().any(|line| line.trim() == "跑识别"),
        "队列屏空态上没有就地跑识别那颗捷径：\n{屏上}",
    );

    // ── 二、逐条那一档：那张表自己也有一张空态。
    现场.app.queue_and_site().0.show_one_by_one();
    let 屏上 = 队列屏上(&ctx, &mut 现场);
    assert!(
        !屏上.contains("romcat identify"),
        "逐条那一档还在叫人去开终端：\n{屏上}",
    );
    assert!(
        屏上.lines().any(|line| line.trim() == "跑识别"),
        "逐条那一档的空态上没有那颗捷径：\n{屏上}",
    );
}

#[test]
fn 队列屏那颗捷径排的是与库屏工序段完全同一趟识别() {
    // **同一个函数、同一趟任务、同一份产物**（验收第 3、4 条）。断言落在
    // 「库屏工序段认领了这一趟」上：只有 `Section::start` 排出去的任务号才进得了
    // `Section::running`，也只有它认领得下来（`Section::settle`）。捷径要是第二份
    // 实现，这两句一句都过不去。
    let 甲 = 建库("gui-捷径-甲");
    let 乙 = 建库("gui-捷径-乙");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.装上弹药();
    现场.加根(甲.path(), "主库");
    现场.扫("主库");
    现场.重列队列();

    // 按下队列屏空态上那颗捷径。**这一屏排不了活**，它只留一个记号。
    现场.app.queue_and_site().0.ask_identify();
    assert!(
        现场.app.roots().stages().task_of(Stage::Identify).is_none(),
        "按下去那一下不该由队列屏自己排活",
    );

    // 窗口跑一帧：`App::route` 取走那个记号，交给 `App::start_stage`。
    let _ = headless::frame(&ctx, headless::input(), |ui| 现场.app.ui(ui));
    现场.等任务跑完();

    // **同一趟任务**：库屏工序段认领了它。只有 `Section::start` 排出去的任务号才进得了
    // `Section::running`，也只有它认领得下来（`Section::settle`）——这一句要是捷径自己
    // 另排了一趟，工序段一个字都不会说。
    assert!(
        现场
            .app
            .roots()
            .stages()
            .notice()
            .is_some_and(|说的| 说的.starts_with("识别 跑完了")),
        "库屏工序段没认领这一趟：{:?} / {:?}",
        现场.app.roots().stages().notice(),
        现场.app.roots().stages().error(),
    );
    // **在任务台上与从库屏排的看不出区别**：那一行的名字就是这道工序的名字。
    let record = &现场.app.tasks().history()[0];
    assert_eq!(record.name, "识别");
    assert!(matches!(record.ending, Ending::Done(_)));
    // **同一份产物**：那一行的数当场刷新，队列自己重新列过——一次「重新列队列」都没点。
    assert_eq!(现场.识别那一行().behind, Behind::Left(0));
    assert!(现场.app.queue().queue().identified());

    // ── 「还没识别」那句警告旁边也是就地的按钮，不是一句 `romcat identify`。
    现场.加根(乙.path(), "元数据库");
    现场.扫("元数据库");
    现场.重列队列();
    assert_eq!(现场.队列屏说的(), 2, "乙那两个变体连识别都还没跑过");
    let 屏上 = 队列屏上(&ctx, &mut 现场);
    assert!(
        屏上.contains("个变体连识别都还没跑过"),
        "那句警告没了：\n{屏上}",
    );
    assert!(
        !屏上.contains("romcat identify"),
        "那句警告旁边还在叫人去开终端：\n{屏上}",
    );
    assert!(
        屏上.lines().any(|line| line.trim() == "跑识别"),
        "那句警告旁边没有就地的按钮：\n{屏上}",
    );
}

#[test]
fn 台上已经有一趟识别时再按队列屏那颗捷径_不会排第二趟() {
    // **捷径按不禁**：工序段那一行跑着时会写「跑着呢」并禁掉按钮（规格 34），可队列屏
    // 够不着工序段，那颗捷径不知道台上有没有活。兜底在 `Section::start` 那句「同一道
    // 工序已经在跑就什么都不做」——这一条走的是**真的那条路**：留记号、`App::route`
    // 取走、交给 `App::start_stage`。两趟识别在同一份中立库上互相清对方的结论。
    let 库 = 建库("gui-捷径-按两下");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.装上弹药();
    现场.加根(库.path(), "主库");
    现场.扫("主库");

    // 占住台上那个位子，好让下面那一趟停在队里、不会自己跑完。
    let 占位 = 占位活::排上(现场.app.tasks_mut(), "装作在扫一趟库");
    现场.app.start_stage(Stage::Identify);
    let id = 现场
        .app
        .roots()
        .stages()
        .task_of(Stage::Identify)
        .expect("这一趟排上任务台了");

    现场.app.queue_and_site().0.ask_identify();
    let _ = headless::frame(&ctx, headless::input(), |ui| 现场.app.ui(ui));
    assert_eq!(
        现场.app.roots().stages().task_of(Stage::Identify),
        Some(id),
        "捷径按下去换掉了台上那一趟",
    );
    assert_eq!(
        现场.app.tasks().queued().len(),
        1,
        "同一趟识别被排了两遍：{:?}",
        现场.app.tasks().queued(),
    );

    现场.app.tasks_mut().stop(id);
    占位.按停(现场.app.tasks_mut());
    现场.等任务跑完();
}

#[test]
fn 扫完一个根再看待确认队列屏_屏头那个数是扫描之后的_而且没有把整份队列重列一遍() {
    // 从前「跑完自己重新列队列」只做识别那一支（挂单 `Q419`）：扫完一个根，队列屏屏头
    // 那句「另有 N 个变体连识别都还没跑过」还是扫描之前的数——屏上那个数在说错话。
    // 队列本身确实一条没变（新扫进来的变体连结论都还没有，进不了队列），所以要的只是
    // **重算那个数**，不是把真库上一万八千条整份再列一遍。
    let 甲 = 建库("gui-队列屏头-甲");
    let 乙 = 建库("gui-队列屏头-乙");
    let ctx = headless::context();
    let mut 现场 = 现场::摆好();
    现场.装上弹药();
    现场.加根(甲.path(), "主库");
    现场.扫("主库");
    现场.跑识别();
    assert!(
        现场.app.queue().queue().pending() > 0,
        "前提：队列里得有东西，不然底下那条「没重列」的断言什么都没验",
    );
    let 屏上 = 队列屏上(&ctx, &mut 现场);
    assert!(
        屏上.contains("随机样本"),
        "前提：列完队列头一批是展开的：\n{屏上}",
    );
    // 人把展开的那一批收起来了。**整份重列会把头一批重新展开**（`queue::Screen::reload`），
    // 所以扫完之后它还收着，就是没重列过的样子——人看到哪儿，扫完还在哪儿。
    {
        let (queue, _) = 现场.app.queue_and_site();
        let scope = queue.scope().expect("头一批是展开的");
        queue.open_batch(&scope.shape);
    }
    let 屏上 = 队列屏上(&ctx, &mut 现场);
    assert!(!屏上.contains("随机样本"), "前提：那一批收起来了：\n{屏上}");

    // 加一块盘，扫它。**一次「重新列队列」都不点。**
    现场.app.show_view(View::Library);
    现场.加根(乙.path(), "元数据库");
    现场.扫("元数据库");
    let Behind::Left(还差) = 现场.识别那一行().behind else {
        panic!("识别这一支说得出还差多少");
    };
    assert_eq!(还差, 2, "乙那两个变体连识别都还没跑过");

    let 屏上 = 队列屏上(&ctx, &mut 现场);
    assert!(
        屏上.contains(&format!("另有 {还差} 个变体连识别都还没跑过")),
        "扫完了，队列屏屏头那个数还是扫描之前的：\n{屏上}",
    );
    assert!(
        !屏上.contains("随机样本"),
        "扫完之后收起来的那一批又展开了——整份队列被重列了一遍：\n{屏上}",
    );
}

// ——— 导出那一支上的铺媒体开关（票 `one-criterion-per-thing/09`） ———

/// 一张封面的字节：**19 个字节**。屏上那句「共多大」对的就是这个数。
const 一张封面: &[u8] = b"\x89PNG-- contra cover";

impl 现场 {
    /// 库里键里带着 `名字` 的那个变体的键。
    fn 变体键(&self, 名字: &str) -> String {
        self.app
            .site()
            .catalog
            .variants()
            .expect("读得出变体")
            .into_iter()
            .map(|row| row.key)
            .find(|key| key.contains(名字))
            .expect("那个变体扫进来了")
    }

    /// 往**媒体池**里放一份媒体、挂到这个变体上。刮削收媒体那一趟落下来的就是这两样：
    /// 池里那个文件、中立库里那条引用。返回它的内容哈希。
    ///
    /// **摆料而已**：收媒体那条路由 `romcat-core` 的 `tests/scrape.rs` 钉着。
    fn 收一份媒体(
        &mut self,
        变体: &str,
        kind: romcat_core::scrape::MediaKind,
        bytes: &[u8],
    ) -> String {
        use romcat_core::catalog::scrape::{Harvested, HarvestedMedia};
        let hash = romcat_core::catalog::frontend::hash_of(bytes);
        let 池 = romcat_core::scrape::pool::MediaPool::at(&romcat_core::workspace::media_pool_dir(
            self.工作区.path(),
        ));
        写(&池.path_of(&hash, "png"), bytes);
        let (_, site, _) = self.app.roots_site_and_tasks();
        site.catalog
            .put_media(&hash, "png", bytes.len() as u64)
            .expect("池里记得下");
        site.catalog
            .put_scraped(&[Harvested {
                anchor: romcat_core::scrape::AnchorKind::Variant.label().to_string(),
                subject: 变体.to_string(),
                // 一个源在一个锚点上写两次是同一个结果，于是每份媒体各记一个源。
                source: format!("本地媒体-{hash}"),
                input: format!("{变体}/{hash}"),
                values: Vec::new(),
                media: vec![HarvestedMedia {
                    kind: kind.label().to_string(),
                    hash: hash.clone(),
                    evidence: "测试".to_string(),
                }],
            }])
            .expect("引用写得进");
        hash
    }

    /// 打开导出那一支上那颗**铺媒体**开关，等「这一趟最多要铺多少」那一趟算完。
    /// 界面上点那颗开关走的就是它（真点那一下由 `打开铺媒体时_…` 那一条钉着）。
    fn 打开铺媒体(&mut self) {
        let (screen, site, tasks) = self.app.roots_site_and_tasks();
        screen.stages_mut().set_lay_media(true, site, tasks);
        self.等任务跑完();
    }
}

/// 一棵目录树底下的全部文件：相对树根、`/` 分隔、排好。树不在就是空的。
fn 树里的文件(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut 待走 = vec![root.to_path_buf()];
    while let Some(dir) = 待走.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                待走.push(path);
            } else {
                let 相对 = path.strip_prefix(root).expect("在这棵树里");
                out.push(
                    相对
                        .components()
                        .map(|part| part.as_os_str().to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join("/"),
                );
            }
        }
    }
    out.sort();
    out
}

/// 摆一份**要不要铺媒体还没定**的现场：横跨两个平台的 fixture 主库扫进来、选好 Pegasus
/// 与导出目录，池里有一张挂在魂斗罗上的封面。返回主库（得活到测试结束）、现场、
/// 导出目录与那张封面的哈希。
fn 摆好一张封面(tag: &str) -> (TempDir, 现场, PathBuf, String) {
    let 库 = 建库(tag);
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    let 魂斗罗 = 现场.变体键("魂斗罗");
    let hash = 现场.收一份媒体(&魂斗罗, romcat_core::scrape::MediaKind::Cover, 一张封面);
    let 导出去 = 现场.工作区.path().join("导出去");
    现场.选一次导出去哪儿("Pegasus", &导出去);
    (库, 现场, 导出去, hash)
}

#[test]
fn 铺媒体开关默认关着_关着时导出与今天一模一样_一份媒体都不铺() {
    // 票 `one-criterion-per-thing/09` 验收第 1、3 条。池里**真有**一张挂在魂斗罗上的封面，
    // 「关着就一份都不铺」才验得出来——池是空的话，关着与开着铺出去的都是零份。
    let (_库, mut 现场, 导出去, _) = 摆好一张封面("gui-stages-铺媒体默认关");
    let ctx = headless::context();
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    assert!(
        屏上
            .lines()
            .any(|line| line.trim() == romcat_gui::stages::LAY_MEDIA),
        "导出那一支上没有铺媒体那颗开关：\n{屏上}",
    );
    assert!(!现场.app.roots().stages().lay_media(), "开关默认开着");
    assert!(
        !屏上.contains("最多要铺"),
        "开关关着，屏上却说起了要铺多少：\n{屏上}",
    );

    现场.导出();

    // **导出目录里只多出元数据文件**：一个 `media/` 都没有，条目里一个资源槽都不写。
    let 盘上 = 树里的文件(&导出去);
    assert_eq!(盘上.len(), 2, "这个库横跨两个平台：{盘上:?}");
    assert!(
        盘上
            .iter()
            .all(|path| path.ends_with(".metadata.pegasus.txt")),
        "关着时导出目录里多出了元数据之外的东西：{盘上:?}",
    );
    for 文件 in 导出去的文件(&导出去) {
        let 元数据 = fs::read_to_string(&文件).expect("读得出");
        assert!(
            !元数据.contains("assets."),
            "关着时条目里写了资源槽：{元数据}"
        );
    }
    let record = &现场.app.tasks().history()[0];
    assert_eq!(record.name, "导出", "关着时那一趟的名字变了");
    assert!(
        matches!(record.ending, Ending::Done(_)),
        "关着时那一趟记成了「{}」",
        record.ending.render(),
    );
    let 回执 = 现场.app.roots().stages().notice().expect("跑完了要说话");
    assert!(!回执.contains("媒体"), "关着时回执里说起了媒体：{回执}");
}

#[test]
fn 打开铺媒体时_按下导出之前屏上先说清这一趟最多要铺几份多大() {
    // 验收第 2 条。**真点一下那颗开关**（指针事件，不是直接调函数），不按导出：
    // 那句代价要在按下之前就画在屏上，不是按下去之后才发现在拷贝。
    //
    // 数是核心库算的**上界**（`transfer::media_to_lay`：落点上已经有的真铺时不重铺），
    // 所以屏上说「最多」（挂单 `Q584`）。池里一张封面、19 个字节：那句话得说「1 份」「19 B」。
    let (_库, mut 现场, 导出去, _) = 摆好一张封面("gui-stages-铺媒体说代价");
    let ctx = headless::context();

    点一下(&ctx, &mut 现场.app, romcat_gui::stages::LAY_MEDIA);
    assert!(
        现场.app.roots().stages().lay_media(),
        "点了那颗开关，它却没开"
    );
    现场.等任务跑完();

    滚到库屏底下(&ctx, &mut 现场.app);
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    // 那句话是**说得出名字的那个函数**折的：重排库屏时（票 `gui-looks-like-the-design/06`）
    // 它一句都不许丢，就按它找。
    let 那一句 = romcat_gui::stages::media_cost(1, 19);
    assert!(屏上.contains(&那一句), "打开之后屏上没说要铺多少：\n{屏上}");
    for 该有的 in ["最多", "1 份", "19 B"] {
        assert!(
            那一句.contains(该有的),
            "那句代价里没有「{该有的}」：{那一句}"
        );
    }
    // **还没按导出**：一个媒体文件都不许先铺出去。
    assert!(
        树里的文件(&导出去).is_empty(),
        "只拨了开关，导出目录里就多了东西：{:?}",
        树里的文件(&导出去),
    );
}

#[test]
fn 要铺多少还没算出来时按导出_当场说清_一份都不先铺() {
    // 验收第 2 条的另一半：代价那句话要在**按下之前**说得出来。开关开着、数还没出来时
    // 按导出，不许先排上去拷起来——人按下去的那一刻屏上还没说过要付多少。
    //
    // **台上先摆一趟占位的活**：任务台一次只跑一趟，算要铺多少那一趟稳稳排在它后面。
    // 钉在「按停」这个信号上，不钉挂钟（同 `占住任务台`）。
    let (_库, mut 现场, _导出去, _) = 摆好一张封面("gui-stages-铺媒体没算完");
    let 占位 = 现场.占住任务台();
    {
        let (screen, site, tasks) = 现场.app.roots_site_and_tasks();
        screen.stages_mut().set_lay_media(true, site, tasks);
    }
    let ctx = headless::context();
    滚到库屏底下(&ctx, &mut 现场.app);
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    assert!(
        !屏上.contains(&romcat_gui::stages::media_cost(1, 19)),
        "还没算出来就画了那句代价：\n{屏上}"
    );
    assert!(
        屏上.contains("正在算"),
        "开着开关、数还没出来，屏上一句都没说：\n{屏上}",
    );

    现场.app.start_stage(Stage::Export);
    assert!(
        现场.app.roots().stages().task_of(Stage::Export).is_none(),
        "要铺多少还没说清，导出就排上了任务台",
    );
    let 说的 = 现场
        .app
        .roots()
        .stages()
        .error()
        .expect("按不下去要说清为什么");
    assert!(
        说的.contains(romcat_gui::stages::LAY_MEDIA),
        "没说清是那颗开关在等：{说的}",
    );

    现场.app.tasks_mut().stop(占位);
    现场.等任务跑完();
    // **数出来之后再按，就排得上了。**
    现场.导出();
    assert!(
        现场
            .app
            .tasks()
            .history()
            .iter()
            .any(|record| record.name == "导出"),
        "算出来之后导出还是排不上：{:?}",
        现场.app.tasks().history(),
    );
}

#[test]
fn 打开铺媒体之后导出那一趟真的铺出去_任务台记完成_回执说得出铺了几份() {
    // 验收第 4 条的第一档（完成）。Pegasus 按内容寻址铺：`media/<哈希前两位>/<哈希>.png`，
    // 条目里写着 `assets.boxFront` 指着它。那条路本身由 `romcat-core` 的 `tests/pegasus.rs`
    // 钉着；这一条钉的是**界面上那颗开关真的接到了那条路上**。
    let (_库, mut 现场, 导出去, hash) = 摆好一张封面("gui-stages-铺媒体完成");
    现场.打开铺媒体();

    现场.导出();

    let 落点 = format!("media/{}/{hash}.png", &hash[..2]);
    assert_eq!(
        fs::read(导出去.join(&落点)).expect("那张封面铺出去了"),
        一张封面,
        "铺出去的不是池里那一份",
    );
    assert!(
        导出去的文件(&导出去).iter().any(|文件| {
            fs::read_to_string(文件)
                .expect("读得出")
                .contains(&format!("assets.boxFront: {落点}"))
        }),
        "条目里没写封面铺在哪儿",
    );
    let record = &现场.app.tasks().history()[0];
    assert_eq!(record.name, "导出");
    assert!(
        matches!(record.ending, Ending::Done(_)),
        "铺完了的那一趟记成了「{}」",
        record.ending.render(),
    );
    let 回执 = 现场.app.roots().stages().notice().expect("跑完了要说话");
    assert!(
        回执.contains("铺出去 1 份"),
        "回执没说铺出去几份媒体：{回执}"
    );
}

#[test]
fn 开着铺媒体的导出排着队被撤掉时记成已取消_一份媒体都没铺() {
    // 验收第 4 条的第二档。台上先摆一趟占位的活，开着铺媒体的导出稳稳排在它后面，撤掉它
    // ——与导出、刮削那两条「排着队被撤掉」同一个验法，不靠挂钟。
    let (_库, mut 现场, 导出去, _) = 摆好一张封面("gui-stages-铺媒体撤掉");
    现场.打开铺媒体();
    let 占位 = 现场.占住任务台();

    现场.app.start_stage(Stage::Export);
    let id = 现场
        .app
        .roots()
        .stages()
        .task_of(Stage::Export)
        .expect("这一趟排上任务台了");
    现场.app.tasks_mut().stop(id);
    现场.app.poll_tasks();

    let record = 现场
        .app
        .tasks()
        .history()
        .iter()
        .find(|one| one.id == id)
        .expect("撤掉的那一趟也进历史");
    assert!(
        matches!(record.ending, Ending::Stopped),
        "撤掉的那一趟记成了「{}」——它一份都没铺",
        record.ending.render(),
    );
    assert!(
        树里的文件(&导出去).is_empty(),
        "撤掉的那一趟往导出目录里放了东西：{:?}",
        树里的文件(&导出去),
    );
    let 说的 = 现场.app.roots().stages().notice().expect("停下了也要说话");
    assert!(
        说的.contains(&Ending::<()>::Stopped.render()),
        "停下的那一趟没说「已取消」：{说的}",
    );

    现场.app.tasks_mut().stop(占位);
    现场.等任务跑完();
}

#[test]
fn 铺媒体连着没铺成主动停了_任务台记部分完成_说得出铺到第几份() {
    // 验收第 4 条的第三档：没走完、却留下了东西（词表**部分完成**）。
    //
    // 让它**确定地**连着失败，不靠挂钟（与 `romcat-core` 的 `tests/pegasus.rs` 那一条同一个
    // 办法）：Pegasus 按内容寻址铺（`media/<哈希前两位>/…`），把排在后面那几份的
    // `media/<前两位>` 先占成一个**文件**——那一枝的目录建不出来，每一份都以同一句话失败。
    let 库 = 建库("gui-stages-铺媒体部分完成");
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    let 导出去 = 现场.工作区.path().join("导出去");
    现场.选一次导出去哪儿("Pegasus", &导出去);
    let 魂斗罗 = 现场.变体键("魂斗罗");
    let mut 按前缀: std::collections::BTreeMap<String, Vec<u8>> = std::collections::BTreeMap::new();
    let mut n = 0;
    while 按前缀.len() < 12 {
        let 字节 = format!("screenshot-{n}").into_bytes();
        n += 1;
        let hash = romcat_core::catalog::frontend::hash_of(&字节);
        按前缀.entry(hash[..2].to_string()).or_insert(字节);
    }
    for 字节 in 按前缀.values() {
        现场.收一份媒体(&魂斗罗, romcat_core::scrape::MediaKind::Screenshot, 字节);
    }
    let mut 前缀们 = 按前缀.keys();
    let 第一份 = 前缀们.next().expect("有第一份").clone();
    for 前缀 in 前缀们 {
        写(&导出去.join("media").join(前缀), b"not a directory");
    }
    现场.打开铺媒体();

    现场.导出();

    let record = &现场.app.tasks().history()[0];
    let Ending::Halfway { left_behind, .. } = &record.ending else {
        panic!(
            "铺媒体连着失败主动停了，台上却记成了「{}」",
            record.ending.render()
        );
    };
    assert!(
        left_behind.contains("（共 12 份）"),
        "那一句没说清一共几份：{left_behind}"
    );
    assert!(
        导出去.join("media").join(&第一份).is_dir(),
        "铺出去的那一份没留在盘上"
    );
    let 说的 = 现场.app.roots().stages().notice().expect("收了场要说话");
    assert!(
        说的.starts_with("导出") && 说的.contains(&record.ending.render()),
        "屏上没说这一趟部分完成：{说的}",
    );
    assert!(
        !说的.contains("跑完了"),
        "没走完的那一趟说成了跑完了：{说的}"
    );
}

#[test]
fn 开着铺媒体读不动优先级表时记成失败_一份媒体都没铺() {
    // 验收第 4 条的第四档：说得出停在哪一步、为什么（与折标题那一条同一个办法）。
    let (_库, mut 现场, 导出去, _) = 摆好一张封面("gui-stages-铺媒体失败");
    现场.打开铺媒体();
    写(
        &现场.工作区.path().join("priorities.toml"),
        "这不是一份 TOML".as_bytes(),
    );

    现场.导出();

    let record = &现场.app.tasks().history()[0];
    let Ending::Failed { step, why } = &record.ending else {
        panic!("表都读不动却把这一趟记成了「{}」", record.ending.render());
    };
    assert_eq!(step, "读优先级表", "说不清停在哪一步");
    assert!(!why.is_empty(), "说不清为什么跑不了");
    assert!(
        树里的文件(&导出去).is_empty(),
        "失败的那一趟往导出目录里放了东西：{:?}",
        树里的文件(&导出去),
    );
    let 说的 = 现场
        .app
        .roots()
        .stages()
        .error()
        .expect("失败要说出来，不能默默结束");
    assert!(
        说的.starts_with("导出") && 说的.contains(&record.ending.render()),
        "屏上没把失败那一档说出来：{说的}",
    );
}

#[test]
fn 照写那一趟带着同一颗铺媒体开关() {
    // 照写走的是与平常那一趟同一份实现、同一套旋钮（`Section::export_knobs`，票
    // `gui-answers-all-six/05`）。先关着导一趟、外面有人动了一份、再导撞上点名；**这时才**
    // 打开铺媒体，**真点一下**「我看过了，照写」——那一趟既照写过去，也把媒体铺出去。
    let (_库, mut 现场, 导出去, hash) = 摆好一张封面("gui-stages-铺媒体照写");
    let ctx = headless::context();
    现场.导出();
    let 动过的 = 导出去的文件(&导出去)[0].clone();
    手改一行(&动过的);
    现场.导出();
    assert!(!导出去.join("media").exists(), "开关还关着，媒体就铺出去了");

    现场.打开铺媒体();
    点一下(&ctx, &mut 现场.app, "我看过了，照写");
    现场.等任务跑完();

    let 落点 = format!("media/{}/{hash}.png", &hash[..2]);
    assert!(
        导出去.join(&落点).is_file(),
        "照写那一趟没带上铺媒体：{:?}",
        树里的文件(&导出去),
    );
    assert!(
        !fs::read_to_string(&动过的)
            .expect("读得出")
            .contains("我后来手加的一行"),
        "按了照写，手改的那一行却还在",
    );
    let 最后一趟 = 现场.app.tasks().history().first().expect("进了历史");
    assert!(
        最后一趟.name.contains("照写"),
        "最后一趟不是照写那一趟：{}",
        最后一趟.name
    );
    assert!(
        matches!(最后一趟.ending, Ending::Done(_)),
        "照写那一趟记成了「{}」",
        最后一趟.ending.render(),
    );
}

#[test]
fn 要铺多少那一趟被撤掉时屏上说清_关掉再打开就重算() {
    // 算要铺多少那一趟也是台上的一趟，按得停。停了之后屏上得说清没算出来、导出照旧按不下去；
    // 屏上那句话说「关掉再打开就重算」——**它得是真的**。
    //
    // 台上先摆一趟占位的活，算的那一趟稳稳排在它后面，撤掉它（不靠挂钟）。
    let (_库, mut 现场, _导出去, _) = 摆好一张封面("gui-stages-铺媒体算被撤掉");
    let 占位 = 现场.占住任务台();
    {
        let (screen, site, tasks) = 现场.app.roots_site_and_tasks();
        screen.stages_mut().set_lay_media(true, site, tasks);
    }
    let 算的那一趟 = 现场
        .app
        .tasks()
        .queued()
        .into_iter()
        .find(|(_, name)| name == romcat_gui::stages::COUNT_MEDIA)
        .map(|(id, _)| id)
        .expect("打开开关就排上了算要铺多少那一趟");
    现场.app.tasks_mut().stop(算的那一趟);
    现场.app.poll_tasks();

    let ctx = headless::context();
    滚到库屏底下(&ctx, &mut 现场.app);
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    assert!(
        屏上.contains(&Ending::<()>::Stopped.render()) && 屏上.contains("关掉再打开"),
        "算的那一趟撤掉了，屏上没说清：\n{屏上}",
    );
    现场.app.start_stage(Stage::Export);
    assert!(
        现场.app.roots().stages().task_of(Stage::Export).is_none(),
        "要铺多少没算出来，导出却排上了",
    );

    现场.app.tasks_mut().stop(占位);
    现场.等任务跑完();
    {
        let (screen, site, tasks) = 现场.app.roots_site_and_tasks();
        screen.stages_mut().set_lay_media(false, site, tasks);
        screen.stages_mut().set_lay_media(true, site, tasks);
    }
    现场.等任务跑完();
    滚到库屏底下(&ctx, &mut 现场.app);
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    assert!(
        屏上.contains(&romcat_gui::stages::media_cost(1, 19)),
        "关掉再打开没重算：\n{屏上}",
    );
}

#[test]
fn 跑完一道工序之后那句代价跟着重算_不拿旧数骗人() {
    // 那个数缓存着、不每帧重算——**缓存到库变了为止**。库变了还画着旧数，屏上说 1 份、
    // 按下去铺 2 份：少报正是这句代价要防的那个方向。
    //
    // 算过一次（1 份、19 个字节）之后，池里又多了一张截图挂在同一个变体上（19 个字节），
    // 再在这一段上跑完一道工序（刮削：只用本地源、不收媒体，库里那两条引用都还在）。
    // 下一帧那句话得说 2 份、38 B。
    let (_库, mut 现场, _导出去, _) = 摆好一张封面("gui-stages-铺媒体库变了");
    现场.打开铺媒体();
    let 魂斗罗 = 现场.变体键("魂斗罗");
    现场.收一份媒体(
        &魂斗罗,
        romcat_core::scrape::MediaKind::Screenshot,
        b"\x89PNG-- contra title",
    );

    现场.跑刮削();

    let ctx = headless::context();
    跑一帧(&ctx, &mut 现场.app);
    现场.等任务跑完();
    滚到库屏底下(&ctx, &mut 现场.app);
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    assert!(
        屏上.contains(&romcat_gui::stages::media_cost(2, 38)),
        "库变了之后那句代价没跟着重算：\n{屏上}",
    );
}

#[test]
fn 离开库屏再回来_那句代价重算一遍() {
    // 别的屏也改得动库（裁决、合并作品、刮削面板收媒体），而那几条出口不经过这一段。
    // 人要改它们就得先离开库屏，所以**离开过这一屏再回来，那个数就重算一遍**（挂单 `Q654`）。
    //
    // 算过一次（1 份）之后池里又挂上一张截图——这一下不经过这一段，与在别的屏上改库一样——
    // 切去浏览屏画一帧、再切回来：那句话得说 2 份、38 B。
    let (_库, mut 现场, _导出去, _) = 摆好一张封面("gui-stages-铺媒体切屏回来");
    let ctx = headless::context();
    现场.打开铺媒体();
    跑一帧(&ctx, &mut 现场.app);
    let 魂斗罗 = 现场.变体键("魂斗罗");
    现场.收一份媒体(
        &魂斗罗,
        romcat_core::scrape::MediaKind::Screenshot,
        b"\x89PNG-- contra title",
    );

    // **还在库屏上**：那个数不动，也不又排一趟去算——缓存着就是不每帧重算。
    滚到库屏底下(&ctx, &mut 现场.app);
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    assert!(
        屏上.contains(&romcat_gui::stages::media_cost(1, 19)),
        "没离开库屏，那句代价就变了：\n{屏上}",
    );
    assert!(!现场.app.tasks().busy(), "没离开库屏，又排了一趟去算");

    现场.app.show_view(View::Browse);
    跑一帧(&ctx, &mut 现场.app);
    现场.app.show_view(View::Library);
    跑一帧(&ctx, &mut 现场.app);
    现场.等任务跑完();

    滚到库屏底下(&ctx, &mut 现场.app);
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        现场.app.ui(ui)
    }));
    assert!(
        屏上.contains(&romcat_gui::stages::media_cost(2, 38)),
        "离开库屏再回来，那句代价没重算：\n{屏上}",
    );
}

#[test]
fn 台上排着一道别的工序时按导出_那句代价作废_不排() {
    // 那个数算出来之后，人在这一段上又排了一道会改库的工序（刮削、识别都会改媒体引用或作品
    // 归属），它还没收场就按导出：导出排在它后面跑，屏上那个数说的却是它之前那一份库。
    // **排别的工序那一下，那句代价就作废**，数重新出来之前导出按不下去。
    //
    // 台上先摆一趟占位的活，刮削稳稳排在它后面（不靠挂钟）。
    let (_库, mut 现场, _导出去, _) = 摆好一张封面("gui-stages-铺媒体排着别的");
    现场.打开铺媒体();
    let 占位 = 现场.占住任务台();
    现场.app.start_stage(Stage::Scrape);
    let 刮削 = 现场
        .app
        .roots()
        .stages()
        .task_of(Stage::Scrape)
        .expect("刮削排上任务台了");

    现场.app.start_stage(Stage::Export);

    assert!(
        现场.app.roots().stages().task_of(Stage::Export).is_none(),
        "台上排着一道会改库的工序，导出却拿着之前那个数排上了",
    );
    let 说的 = 现场
        .app
        .roots()
        .stages()
        .error()
        .expect("按不下去要说清为什么");
    assert!(
        说的.contains(romcat_gui::stages::LAY_MEDIA),
        "没说清是那颗开关在等：{说的}",
    );

    现场.app.tasks_mut().stop(刮削);
    现场.app.tasks_mut().stop(占位);
    现场.等任务跑完();
}

#[test]
fn 算要铺多少那一趟收场时浏览屏不重读() {
    // 那一趟整条只读：认领了它不等于库变了。照「认领了就转告浏览屏」那条路走的话，每打开
    // 一次开关、每离开库屏再回来一次，浏览屏就把窗里那几百行连同筛选面板整份重读一遍。
    let (_库, mut 现场, _导出去, _) = 摆好一张封面("gui-stages-铺媒体不惊动浏览屏");
    let ctx = headless::context();
    现场.app.show_view(View::Browse);
    跑一帧(&ctx, &mut 现场.app);
    跑一帧(&ctx, &mut 现场.app);
    let 读过几次 = 现场.app.window().reads();
    assert!(读过几次 > 0, "浏览屏一次都没读过库，这一条什么都没验");

    现场.app.show_view(View::Library);
    现场.打开铺媒体();
    现场.app.show_view(View::Browse);
    跑一帧(&ctx, &mut 现场.app);
    跑一帧(&ctx, &mut 现场.app);

    assert_eq!(
        现场.app.window().reads(),
        读过几次,
        "算要铺多少那一趟收了场，浏览屏跟着重读了一遍",
    );
}

#[test]
fn 要铺多少算出来之后_那句按不下去的话收掉() {
    // 数没出来时按导出，屏上挂一句「先看清那句再按」；数出来之后那句话还挂着，就与底下已经
    // 画出来的代价互相顶：一句说还没说清，一句已经说清了。
    let (_库, mut 现场, _导出去, _) = 摆好一张封面("gui-stages-铺媒体收掉拒绝");
    let 占位 = 现场.占住任务台();
    {
        let (screen, site, tasks) = 现场.app.roots_site_and_tasks();
        screen.stages_mut().set_lay_media(true, site, tasks);
    }
    现场.app.start_stage(Stage::Export);
    assert!(
        现场.app.roots().stages().error().is_some(),
        "数没出来时按导出，一句都没说",
    );

    现场.app.tasks_mut().stop(占位);
    现场.等任务跑完();

    assert!(
        现场.app.roots().stages().error().is_none(),
        "数算出来了，那句按不下去的话还挂着：{:?}",
        现场.app.roots().stages().error(),
    );
}

#[test]
fn 改了导出的前端格式之后_那句代价重算一遍() {
    // 媒体的布局随前端格式不同（Pegasus 按内容寻址、ES-DE 按 ROM 名），换了格式那个数就不是
    // 这一份了——哪怕这份小库上两种格式算出来碰巧一样，也得重算，不拿旧格式的数说新格式。
    let (_库, mut 现场, 导出去, _) = 摆好一张封面("gui-stages-铺媒体换格式");
    let ctx = headless::context();
    现场.打开铺媒体();
    let 另一个格式 = romcat_core::adapter::names()
        .into_iter()
        .find(|name| *name != "Pegasus")
        .expect("除了 Pegasus 还有别的格式");

    现场.选一次导出去哪儿(另一个格式, &导出去);
    跑一帧(&ctx, &mut 现场.app);
    现场.等任务跑完();

    let 算了几趟 = 现场
        .app
        .tasks()
        .history()
        .iter()
        .filter(|record| record.name == romcat_gui::stages::COUNT_MEDIA)
        .count();
    assert_eq!(算了几趟, 2, "换了前端格式，那句代价没重算");
}

/// **识别用得上库里已经问过的答案**（票 `gui-answers-all-six/02`，挂单 `Q418`）。
///
/// `model_answer` 那张表是中立库里唯一花过钱的一张：人在命令行上带 `--model` 问过一趟，
/// 答案连同那笔账落在库里，重扫、重跑识别都不清它。界面上跑一趟识别得照旧把它们折成
/// **候选**、与命令行那一趟一样多，而且**一个请求都不发**。
mod 问过的答案 {
    use super::*;

    use romcat_core::dat::{CannedFetcher, DatRepo};
    use romcat_core::filename::Rules;
    use romcat_core::identify;
    use romcat_core::identify::model::{self, Credentials, Guessing, Inference, Limits, Pricing};
    use romcat_core::identify::report::IdentifyReport;
    use romcat_core::verdict;
    use romcat_core::zh;

    /// `建库` 摆的那两份：穿不透的 zip，DAT 库又是空的，前面各层一条候选都给不出。
    const 问过的两个: [&str; 2] = ["主库/FC/魂斗罗.zip", "主库/SFC/幻想传说 汉化版.zip"];

    /// 问过那一趟之后才扫进来的那一份：**没被问过**。它照样落到模型推断那一层——
    /// 手里有网络句柄的话，它就会被问出去。
    const 没问过的: &str = "主库/MD/新来的.zip";

    /// 假服务器答的那一份：这一批 `n` 条，每条两个候选。
    ///
    /// **答案那一段照核心库自己的写法折**（`model::Answer::to_json`），这里只补编号与外面
    /// 那层信封——手写那几个键的话，核心库改一个键名，这份摆料就悄悄成了一份认不出的答复。
    fn 一份答复(n: usize) -> Vec<u8> {
        let rows: Vec<serde_json::Value> = (1..=n)
            .map(|id| {
                let answer = model::Answer {
                    guesses: vec![
                        model::Guess {
                            title: format!("模型说的第{id}个"),
                            platform: None,
                            basis: "名字像".to_string(),
                        },
                        model::Guess {
                            title: format!("模型说的第{id}个备选"),
                            platform: None,
                            basis: "同系列".to_string(),
                        },
                    ],
                };
                let mut row: serde_json::Value =
                    serde_json::from_str(&answer.to_json()).expect("核心库折出来的是 JSON");
                row["编号"] = serde_json::json!(id);
                row
            })
            .collect();
        let text = serde_json::json!({ "答案": rows }).to_string();
        serde_json::to_vec(&serde_json::json!({
            "model": model::DEFAULT_MODEL,
            "content": [{ "type": "text", "text": text }],
            "usage": { "input_tokens": 3_000, "output_tokens": 600 }
        }))
        .expect("造得出")
    }

    /// 命令行 `romcat identify` **一个旋钮都不拨**时的上限：`ModelArgs::limits` 照那几个
    /// 开关的默认值折出来的那一副（`crates/cli/src/main.rs`），一格一格照抄——那边是字面量
    /// 的（花费上限、请求间隔）这边也写字面量。
    ///
    /// **提问指纹认的就是它**：界面那一路取的是 `Limits::default()`。两边对不上的话，命令行
    /// 问过的答案界面上一条都命中不了，候选数悄悄少掉——这几条测试拿它问答案、拿它当命令行
    /// 那一趟，于是那件事一发生就当场红。
    fn 命令行不拨旋钮时的上限() -> Limits {
        Limits {
            batch: model::DEFAULT_BATCH,
            guesses: model::DEFAULT_GUESSES,
            budget: model::DEFAULT_BUDGET,
            // `--model-max-spend` 的默认值是 "5.00" 美元。
            spend_cap_micros: 5_000_000,
            max_output_tokens: model::DEFAULT_MAX_OUTPUT,
            effort: model::DEFAULT_EFFORT.to_string(),
            // `--model-interval-ms` 的默认值。
            interval: Duration::from_millis(1_000),
            backoff: model::BACKOFF,
        }
    }

    fn 开_dat(现场: &现场) -> DatRepo {
        DatRepo::open(&romcat_core::workspace::dat_repo_path(现场.工作区.path()))
            .expect("开得出 DAT 库")
    }

    /// **在界面之外**跑一趟识别，模型推断那一层由调用方给。
    ///
    /// 别的原料是界面与命令行在一份什么都没摆的工作目录里摆出来的那一副：空 DAT 库、
    /// 内置剥离规则、没取过中文离线源、沉淀库里什么都没有、没取过 TitleID 索引。
    fn 在界面之外跑一趟识别(
        现场: &mut 现场,
        guessing: &Guessing<'_>,
    ) -> identify::Outcome {
        let repo = 开_dat(现场);
        let rules = Rules::builtin();
        let naming = fuzzy::Naming {
            rules: &rules,
            index: None,
            tuning: zh::Tuning::default(),
        };
        let (_, site, _) = 现场.app.roots_site_and_tasks();
        let verdicts =
            verdict::Index::load(&site.store, &site.library_identity).expect("沉淀库读得动");
        let roots = Roots::load(&site.catalog).expect("读得出根");
        identify::run(
            &RealFs::new(),
            &mut site.catalog,
            &identify::Ammo {
                repo: &repo,
                verdicts: &verdicts,
                naming: &naming,
                guessing,
                titledb: None,
            },
            &identify::Options::new(roots),
            &CancelToken::new(),
            &mut |_| {},
        )
        .expect("识别不该失败")
    }

    /// **在界面之外问过一趟**：命令行带 `--model`、别的旋钮不拨跑过的那一趟——答案连同那笔账
    /// 落进中立库。
    ///
    /// 答话的是假服务器（`CannedFetcher`），一个网络请求都不发，同 `romcat-core` 的
    /// `tests/model_inference.rs`。**凭据只在这一趟摆料里出现**：界面上那一趟要证的正是它
    /// 手里一套都没有。
    fn 在界面之外问过一趟(现场: &mut 现场) {
        let fetcher = CannedFetcher::new().with_prefix(model::ENDPOINT, 200, 一份答复(20));
        let cancel = CancelToken::new();
        let price = model::Price {
            input_per_mtok: 500,
            output_per_mtok: 2_500,
        };
        // 间隔与退避不进提问指纹，摆料这一趟不必真等。
        let limits = Limits {
            interval: Duration::ZERO,
            backoff: Duration::ZERO,
            ..命令行不拨旋钮时的上限()
        };
        let net = Inference::new(
            &fetcher,
            limits.clone(),
            price,
            Credentials::api_key("摆料用的假凭据"),
            &cancel,
        );
        let 还没问过 = model::Answers::default();
        let outcome = 在界面之外跑一趟识别(
            现场,
            &Guessing {
                answers: &还没问过,
                net: Some(&net),
                announce: None,
                planning: false,
                model: model::DEFAULT_MODEL.to_string(),
                price,
                checked: String::new(),
                limits,
            },
        );
        assert_eq!(
            outcome.model.asked, 2,
            "前提：那两个变体真的被问过：{:?}",
            outcome.model,
        );
    }

    /// 照命令行 `romcat identify`（**不带** `--model`）那一副装配，在同一份中立库上跑一趟识别。
    ///
    /// **命令行没有库函数可调**（`romcat-cli` 只有一个 `main.rs`），界面测试也够不着那个
    /// 二进制，于是照 `crates/cli/src/main.rs` 里 `model::Guessing` 那段字面量抄一份：答案整份
    /// 读回来、价钱**从内置价目表里查**、念计划的回调装着、没有网络句柄、不排计划、上限是
    /// 旋钮全不拨的那一副。与界面那一副差的正是价目表与念计划那两样。
    fn 照命令行那一副跑一趟(现场: &mut 现场) {
        let answers = model::Answers::build(
            现场
                .app
                .site()
                .catalog
                .model_answers()
                .expect("读得出问过的答案"),
        );
        let pricing = Pricing::builtin();
        let price = pricing
            .price(model::DEFAULT_MODEL)
            .expect("库里存着答案时命令行查不到价就不启动：内置价目表里得有默认模型");
        let announce = |_: &model::Plan| {};
        let _ = 在界面之外跑一趟识别(
            现场,
            &Guessing {
                answers: &answers,
                net: None,
                announce: Some(&announce),
                planning: false,
                model: model::DEFAULT_MODEL.to_string(),
                price,
                checked: pricing.checked().to_string(),
                limits: 命令行不拨旋钮时的上限(),
            },
        );
    }

    /// 中立库里这个变体身上有几条**模型推断**那一层的候选。
    fn 模型的候选(现场: &现场, key: &str) -> usize {
        现场
            .app
            .site()
            .catalog
            .candidates_of(key)
            .expect("读得出候选")
            .iter()
            .filter(|candidate| candidate.source == model::SOURCE)
            .count()
    }

    /// 一份库眼下的**候选**账。
    #[derive(Debug, PartialEq, Eq)]
    struct 候选账 {
        /// 问过的两个与没问过的那个，各几条。
        每个变体: Vec<(&'static str, usize)>,
        /// 报告里一共几条。
        一共: u64,
        /// 其中模型推断那一层几条。
        模型推断: u64,
    }

    fn 记一笔候选账(现场: &现场) -> 候选账 {
        let catalog = &现场.app.site().catalog;
        let 每个变体 = [问过的两个[0], 问过的两个[1], 没问过的]
            .into_iter()
            .map(|key| (key, catalog.candidates_of(key).expect("读得出候选").len()))
            .collect();
        let report = IdentifyReport::build(catalog, &开_dat(现场)).expect("报告折得出");
        let 模型推断 = report
            .sources
            .iter()
            .find(|row| row.source == model::SOURCE)
            .map_or(0, |row| row.candidates);
        候选账 {
            每个变体,
            一共: report.candidates,
            模型推断,
        }
    }

    /// 摆好一份问过一趟的库：两个变体、两份答案、一笔账。
    fn 问过一趟的库(tag: &str) -> (TempDir, 现场) {
        let 库 = 建库(tag);
        let mut 现场 = 现场::摆好();
        现场.装上弹药();
        现场.加根(库.path(), "主库");
        现场.扫("主库");
        在界面之外问过一趟(&mut 现场);
        (库, 现场)
    }

    /// 问过那一趟之后，往这份 fixture 主库里再放一份、重扫一遍。
    fn 再扫进一个没问过的(库: &TempDir, 现场: &mut 现场) {
        写(&库.path().join("MD/新来的.zip"), &zip(1_024));
        现场.扫("主库");
    }

    fn 屏上的字(现场: &mut 现场) -> String {
        let ctx = headless::context();
        画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
            现场.app.ui(ui)
        }))
    }

    #[test]
    fn 库里已经问过的答案_界面上跑一趟识别照旧折成候选() {
        let (_库, mut 现场) = 问过一趟的库("gui-stages-问过的答案");

        现场.跑识别();

        let record = &现场.app.tasks().history()[0];
        assert!(
            matches!(record.ending, Ending::Done(_)),
            "识别那一趟记成了「{}」",
            record.ending.render(),
        );
        for key in 问过的两个 {
            assert_eq!(
                模型的候选(&现场, key),
                2,
                "{key} 问过的那两条没折成候选——界面上那一趟没用库里已经问过的答案",
            );
        }
    }

    #[test]
    fn 界面上那一趟一个请求都不发_没问过的那个照旧没有答案_零价不上屏() {
        // **一个请求都不发**看的是库里那本账：发出去的每一个请求都记一笔
        // （`Catalog::put_model_call`），答回来的每一条都落一行答案。
        let (库, mut 现场) = 问过一趟的库("gui-stages-一个请求都不发");
        再扫进一个没问过的(&库, &mut 现场);
        let 账 = 现场.app.site().catalog.model_spend().expect("读得出总账");
        let 答案 = 现场
            .app
            .site()
            .catalog
            .model_answers()
            .expect("读得出答案")
            .len();
        let (请求数, _) = 账;
        assert_eq!(请求数, 1, "前提：问过的那一趟发过一个请求");

        现场.跑识别();

        assert_eq!(
            现场.app.site().catalog.model_spend().expect("读得出总账"),
            账,
            "界面上那一趟发了请求",
        );
        assert_eq!(
            现场
                .app
                .site()
                .catalog
                .model_answers()
                .expect("读得出答案")
                .len(),
            答案,
            "界面上那一趟替没问过的那个问了",
        );
        assert_eq!(
            模型的候选(&现场, 没问过的),
            0,
            "没问过的那个凭空多出了模型推断的候选",
        );
        for key in 问过的两个 {
            assert_eq!(模型的候选(&现场, key), 2, "{key} 问过的那两条没折成候选");
        }
        // **零价不上屏是一道护栏**：没问过的那个让核心库照零价算了一份计划（界面这一路价钱
        // 传零），那份计划留在这一趟的产物里。界面眼下一处都不画计划与花费，所以这一句
        // 改动之前也是绿的——它防的是日后有人把那份计划、或者报告里模型推断那一段画上屏。
        let 屏上 = 屏上的字(&mut 现场);
        assert!(!屏上.contains("美元"), "零价的花费上了屏：\n{屏上}");
    }

    #[test]
    fn 跑完那句回执说得出几条候选来自已经问过的答案() {
        let (_库, mut 现场) = 问过一趟的库("gui-stages-回执数得出");

        现场.跑识别();

        let 屏上 = 屏上的字(&mut 现场);
        assert!(
            屏上.lines().any(|line| line.trim()
                == "4 条候选来自已经问过的答案（2 个变体），这一趟一个请求都没发。"),
            "屏上说不出几条候选来自已经问过的答案：\n{屏上}",
        );
    }

    #[test]
    fn 同一份库上界面那一趟与命令行那一趟折出的候选一样多() {
        let (库, mut 现场) = 问过一趟的库("gui-stages-与命令行一样多");
        再扫进一个没问过的(&库, &mut 现场);

        现场.跑识别();
        let 界面那一趟 = 记一笔候选账(&现场);
        照命令行那一副跑一趟(&mut 现场);
        let 命令行那一趟 = 记一笔候选账(&现场);

        assert_eq!(
            界面那一趟, 命令行那一趟,
            "同一份库上界面与命令行折出的候选不一样多",
        );
        assert_eq!(
            界面那一趟.模型推断, 4,
            "前提：两条路都把问过的那四条折成了候选",
        );
    }
}
