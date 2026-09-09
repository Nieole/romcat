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
use romcat_core::sources::SourceState;
use romcat_core::stage::{Behind, Stage, StageRow};
use romcat_core::task::{Cutoff, Ending};
use romcat_core::testing::container::{ZipEntrySpec, crc32, zip_container};
use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};
use romcat_gui::app::{App, View};
use romcat_gui::headless;

mod shared;
use shared::画出来的字;

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
        drop(Catalog::open(&库文件).expect("能开中立库"));
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

    /// 工序段上导出那一行。
    fn 导出那一行(&self) -> StageRow {
        self.app
            .roots()
            .stages()
            .of(Stage::Export)
            .expect("工序段有导出那一行")
            .clone()
    }

    /// 选一次**前端格式**与**导出目录**。界面上工序段底下那一行填完按「记下」走的就是它。
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

    /// 队列屏眼下说库里还有多少个变体连识别都没跑过。
    fn 队列屏说的(&self) -> u64 {
        self.app.queue().queue().not_run()
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
        屏上.lines().any(|line| line.trim().starts_with("工序 · ")),
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
fn 识别跑到一半按停在任务台历史上记成停在半路() {
    // **写过东西的活被叫停要记成第三档**：识别起手就把上一轮的结论清干净，所以它一定
    // 动过库——记成「停了，什么都没留下、可以当没跑过」的话，那句话是骗人的。
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
    // **识别没有断点**：下一趟从头再算一遍。说反了的话人会以为按停是省时间的。
    assert!(
        left_behind.contains("从头再算一遍"),
        "那一句说得像接得上：{left_behind}",
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

    let 占位 = 现场.app.tasks_mut().queue("装作在扫一趟库", |task| {
        for _ in 0..3_000 {
            task.check()?;
            std::thread::sleep(Duration::from_millis(2));
        }
        Err(Cutoff::failed("这一趟本来就只是占着位子"))
    });

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
        "这一趟还在台上，那一行的按钮却还写着「开跑」：\n{屏上}",
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
    现场.app.tasks_mut().stop(占位);
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
fn 还没取回那份弹药时识别如实拒绝并说清为什么() {
    // **偷偷开一份空的 DAT 库跑下去是一句假话**：整库都会落成「未命中」，
    // 而人会去找哪儿坏了。没有弹药就没有命中率——直说，并指向上面那一段。
    let 库 = 建库("gui-stages-没弹药");
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");
    现场.跑识别();

    let record = &现场.app.tasks().history()[0];
    let Ending::Failed { why, .. } = &record.ending else {
        panic!("没有 DAT 库却把这一趟记成了「{}」", record.ending.render());
    };
    assert!(why.contains("DAT 库"), "说不清为什么跑不了：{why}");
    assert!(why.contains("数据源"), "没指向取回它的地方：{why}");
    // 库里一条结论都没多出来。
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
    assert_eq!(record.name, "折标题");
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

    let 占位 = 现场.app.tasks_mut().queue("装作在扫一趟库", |task| {
        for _ in 0..3_000 {
            task.check()?;
            std::thread::sleep(Duration::from_millis(2));
        }
        Err(Cutoff::failed("这一趟本来就只是占着位子"))
    });
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

    现场.app.tasks_mut().stop(占位);
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
        屏上.lines().any(|line| line.trim() == "折标题"),
        "工序段上没有折标题那一行：\n{屏上}",
    );
    assert!(
        屏上.lines().any(|line| line.trim() == "工序 · 3 道"),
        "工序段说的道数不对：\n{屏上}",
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
fn 还没选过格式与目录时点导出_当场说清而不是默默不动() {
    // **排一趟活的入口只有一个**（`App::start_stage`），票 `09` 的捷径走的也是它。
    // 那时人可能一次都没选过——默默不动的话，他会以为按钮坏了。
    let 库 = 建库("gui-stages-导出没选过");
    let mut 现场 = 现场::摆好();
    现场.加根(库.path(), "主库");
    现场.扫("主库");

    现场.导出();
    let 说的 = 现场.app.roots().stages().error().expect("该说清");
    assert!(说的.contains("格式"), "没说清缺的是什么：{说的}");
    assert!(说的.contains("目录"), "没说清缺的是什么：{说的}");
    // 一个字节都没写出去。
    assert_eq!(现场.app.site().catalog.exported_at().expect("读得出"), None);
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

    let 占位 = 现场.app.tasks_mut().queue("装作在扫一趟库", |task| {
        for _ in 0..3_000 {
            task.check()?;
            std::thread::sleep(Duration::from_millis(2));
        }
        Err(Cutoff::failed("这一趟本来就只是占着位子"))
    });
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

    现场.app.tasks_mut().stop(占位);
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
    let 占位 = 现场.app.tasks_mut().queue("装作在扫一趟库", |task| {
        for _ in 0..3_000 {
            task.check()?;
            std::thread::sleep(Duration::from_millis(2));
        }
        Err(Cutoff::failed("这一趟本来就只是占着位子"))
    });
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
    现场.app.tasks_mut().stop(占位);
    现场.等任务跑完();
}
