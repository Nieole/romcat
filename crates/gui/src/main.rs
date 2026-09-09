//! `romcat-gui`：界面的入口。
//!
//! 交付出去的客户端只有两种跑法：**开窗**（默认）与**查豆腐块**（`--font-check`）。
//!
//! 实测那几条（`--bench` / `--bench-queue` / `--bench-browse` / `--bench-sublibrary`）
//! 与合成数据（`--demo`）在 `demo` feature 之后，**默认不编进来**——它们是量数用的
//! 脚手架，不是交付物。跑它们要 `cargo run -p romcat-gui --features demo -- …`。
//! 它们都不开窗，于是在没有显示器的地方也跑得起来。
//!
//! 开哪份库：给主库根或 `--library <名字>` 就按名字去工作目录里找，`--catalog <文件>`
//! 直接开一份。**三样都不给就先看[上次开的那份](romcat_gui::recent)**——记着的那一份
//! 直接开进主窗口，没记过或者记的那份打不开才进**开场**（列出这个工作目录里有哪些
//! 中立库，挑一份开进去。ADR-0023：界面自足，从第一步起不必开终端）。
//! **不擅自造一份合成的糊弄人**：假数据与真库在界面上长得一模一样，看见一屏假名字的
//! 第一反应会是「我的库怎么了」。
//! 要看合成数据（形状照真机来，见 [`demo`]）得显式给 `--demo`。
//! **一个字节都不读主库**（ADR-0001、ADR-0004）。
//!
//! **定位、开出现场、进主窗口那一段不在这个文件里**，在 [`Program`] 上——这儿的东西
//! 一条测试都够不着（它是个二进制的 `main`），而那一段正是维护者第一次打开工具时走的
//! 那条路。入口只剩**解析参数**与**开窗**。

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
#[cfg(feature = "demo")]
use romcat_core::catalog::Catalog;
use romcat_core::catalog::VariantQuery;
#[cfg(feature = "demo")]
use romcat_core::site::Site;
#[cfg(feature = "demo")]
use romcat_gui::app::{App, View};
#[cfg(feature = "demo")]
use romcat_gui::bench::Sweep;
use romcat_gui::program::Program;
#[cfg(feature = "demo")]
use romcat_gui::recent::Recent;
use romcat_gui::site::Locate;
#[cfg(feature = "demo")]
use romcat_gui::{bench, demo};
use romcat_gui::{font, headless};

/// `--bench` 与 `--bench-sublibrary` 不给 `--rows` 时造多少个变体。
///
/// **十万行是变体表那条线的量级**（票 22）——那一屏一行一个变体，与收敛成多少行无关。
/// 浏览屏那两条（`--bench-browse` / `--bench-paging`）量的是**作品级主列表**，
/// 它们的默认是真库的形状：[`demo::BROWSE_VARIANTS`] 个变体收敛成
/// [`demo::BROWSE_LINES`] 行。
#[cfg(feature = "demo")]
const BENCH_ROWS: u64 = 100_000;

/// romcat 的界面。
#[derive(Debug, Parser)]
#[command(
    name = "romcat-gui",
    version,
    about = "romcat 界面：库 / 浏览 / 待确认 / 子库 / 任务，五屏"
)]
struct Args {
    /// 主库根目录。**只用来找到对应的中立库，一个字节都不读它**
    #[arg(value_name = "主库根")]
    root: Option<PathBuf>,

    /// 按名字找中立库（扫描时用 `--library` 起的那个名字）
    #[arg(long, value_name = "名字")]
    library: Option<String>,

    /// 工作目录：中立库与**沉淀库**存这里
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,

    /// 直接打开这一份中立库文件；不给就按名字找
    #[arg(long, value_name = "文件")]
    catalog: Option<PathBuf>,

    /// 不开窗，检查必备字符有没有豆腐块。开了现成的库就连库里全部变体的键一起查
    #[arg(long)]
    font_check: bool,

    // ── 以下全在 `demo` feature 之后：量数用的脚手架，不是交付物 ──────────────
    /// 拿**合成数据**开一个演示窗口。**主库一个字节都不读**
    #[cfg(feature = "demo")]
    #[arg(long)]
    demo: bool,

    /// 合成数据里的**待裁决**条数。**主库只读**，这条路一个字节都不碰真库
    #[cfg(feature = "demo")]
    #[arg(long, value_name = "条数", default_value_t = demo::QUEUE_ROWS)]
    queue_rows: u64,

    /// 合成变体数（`--bench` / `--bench-browse` / `--bench-paging` / `--bench-sublibrary` 共用）
    ///
    /// 不给就按各条自己的默认：`--bench-browse` 与 `--bench-paging` 是 46,428
    /// （真库的规模），其余是 100,000
    #[cfg(feature = "demo")]
    #[arg(long, value_name = "行数")]
    rows: Option<u64>,

    /// 不开窗，滚一遍变体表量每帧的代价，打印中位数与最慢的一帧
    #[cfg(feature = "demo")]
    #[arg(long, value_name = "帧数", num_args = 0..=1, default_missing_value = "240")]
    bench: Option<u32>,

    /// 不开窗，量**待确认队列**：列队列、换选择器、每帧、排计划各要多久
    ///
    /// **只在合成数据上跑**：它最后一步真的落一批裁决下去，那不该落进你的沉淀库
    #[cfg(feature = "demo")]
    #[arg(long, value_name = "帧数", num_args = 0..=1, default_missing_value = "240")]
    bench_queue: Option<u32>,

    /// 不开窗，量**浏览屏**：列筛选面板、换一次筛选、点开一行、全选展开、每帧各要多久
    #[cfg(feature = "demo")]
    #[arg(long, value_name = "帧数", num_args = 0..=1, default_missing_value = "240")]
    bench_browse: Option<u32>,

    /// 不开窗，量**主列表翻页**：`work_total` 与 `work_page` 在三种排法、三种搜法与筛着时各多久
    ///
    /// 量的是**核心库那两条查询本身**，不是一帧画多久——翻页贵在库里
    #[cfg(feature = "demo")]
    #[arg(long)]
    bench_paging: bool,

    /// `--bench-browse` / `--bench-paging` 那份合成数据里有几个作品，也就是那些变体
    /// 收敛成多少行
    ///
    /// 不给就**按真库的比例折**（`demo::works_for`）：`--rows` 用默认的 46,428 时正好
    /// 7,407 个作品，加上 1/13 那批「还没识别」的（一个变体一行），**收出真库那
    /// 10,978 行**。拧 `--rows` 时这个数跟着走，不必手动配对
    #[cfg(feature = "demo")]
    #[arg(long, value_name = "个数")]
    bench_works: Option<usize>,

    /// 不开窗，量**子库**：建一个、写一条规则、排一次差量预览
    ///
    /// **目标设备一律用本地 fixture 目录模拟**：这条命令自己在临时目录里造一个空目录当
    /// 目标，绝不去动任何真实设备或 SD 卡
    #[cfg(feature = "demo")]
    #[arg(long)]
    bench_sublibrary: bool,

    /// `--bench-sublibrary` 用的规则；容量上限写在 `--bench-capacity`
    #[cfg(feature = "demo")]
    #[arg(long, value_name = "规则", default_value = "平台=SFC,GBA,MD")]
    bench_rule: String,

    /// `--bench-sublibrary` 里那个子库的容量上限，如 `512MB`
    #[cfg(feature = "demo")]
    #[arg(long, value_name = "容量", default_value = "512MB")]
    bench_capacity: String,

    /// 每帧往下滚几行；不给就一趟滚完整张表（**最坏情况**，每帧都要读库）
    #[cfg(feature = "demo")]
    #[arg(long, value_name = "行数")]
    rows_per_frame: Option<f32>,
}

impl Args {
    /// 三种给法折成核心库认得的形状。
    fn locate(&self) -> Locate<'_> {
        Locate {
            root: self.root.as_deref(),
            library: self.library.as_deref(),
            workspace: self.workspace.as_deref(),
            catalog: self.catalog.as_deref(),
        }
    }

    /// 这一趟造多少个变体：`--rows` 说了就听它的，没说就是 [`BENCH_ROWS`]。
    #[cfg(feature = "demo")]
    fn rows(&self) -> u64 {
        self.rows.unwrap_or(BENCH_ROWS)
    }

    /// 同上，但**浏览屏那两条**的默认是 [`demo::BROWSE_VARIANTS`]——它们量的是
    /// **作品级主列表**，而那条查询的代价跟着收敛成多少行走；拿十万行量出来的数
    /// 与 `docs/library-facts.md` 那张表不可比。
    #[cfg(feature = "demo")]
    fn browse_rows(&self) -> u64 {
        self.rows.unwrap_or(demo::BROWSE_VARIANTS)
    }

    /// 这一趟造多少个**作品**：`--bench-works` 说了就听它的，没说就按真库的比例折。
    ///
    /// **两个数是一对**（变体多少个、收敛成多少行），从前要手动配对，拧了 `--rows`
    /// 忘了拧 `--bench-works` 量出来的就是另一个形状。现在没说的那一半自己跟上。
    #[cfg(feature = "demo")]
    fn works(&self, rows: u64) -> usize {
        self.bench_works.unwrap_or_else(|| demo::works_for(rows))
    }

    /// 说了要开合成数据吗。没编进 `demo` feature 时**永远是否**。
    #[cfg(feature = "demo")]
    fn wants_demo(&self) -> bool {
        self.demo || self.benching()
    }

    /// 在跑哪一条**实测**。这几条量的就是合成数据，不给库也照造。
    #[cfg(feature = "demo")]
    fn benching(&self) -> bool {
        self.bench.is_some()
            || self.bench_queue.is_some()
            || self.bench_browse.is_some()
            || self.bench_paging
            || self.bench_sublibrary
    }

    /// 走完启动那条路，交出**程序本体**。
    ///
    /// 开现成的库那条整条在 [`Program::start`] 上——**没说开哪份、也没要合成数据，
    /// 就先看上次开的那份，没有才进开场**（这条路从前是「都不给就悄悄造一份合成的」，
    /// 再往后是「都不给就报错退出」；合成数据与真库在界面上长得一模一样，于是不带参数
    /// 打开看见的会是一屏假名字，第一反应是「我的库怎么了」而不是「我打开的不是我的
    /// 库」——**不报错的错比报错的错难查得多**）。
    ///
    /// 这儿只多一支：**合成数据那一路**。它的现场是造出来的，压根不走定位那一段，
    /// 而且进窗口之前要把标题里那个库名换掉。
    #[cfg(feature = "demo")]
    fn start(&self) -> Result<Program, String> {
        // 没说开哪份现成的库、又点名要看合成数据——只有这一种情况走演示那条路。
        if !self.locate().given() && self.wants_demo() {
            let synthetic =
                demo::queue(self.queue_rows).map_err(|error| format!("造不出合成数据：{error}"))?;
            // **工作目录只折一次**：`Program` 那一侧拿它开「换一份库」回到的那个开场，
            // `App` 拿它找媒体池——两处各折一遍，演示窗口里按一下就换到别处去了。
            let workspace = self.workspace_dir();
            let mut app = App::new(demo::site(synthetic)?, workspace.clone());
            // 标题里说清开的是哪一份。合成数据与真库在界面上长得一模一样，标题是唯一
            // 一直看得见的区分处。
            app.set_library_label("合成数据（演示）");
            // **合成数据自己一个字都不记**（它没有中立库文件）；这一份记忆是给「在演示
            // 窗口里按下换一份库、然后真开了一份库」那一下准备的，那时记的是真的那一份。
            return Ok(Program::opened(app, workspace, Recent::here()));
        }
        Program::start(&self.locate())
    }

    /// 走完启动那条路。没编进 `demo` feature 时**只有开现成的库这一条**。
    #[cfg(not(feature = "demo"))]
    fn start(&self) -> Result<Program, String> {
        Program::start(&self.locate())
    }

    /// 实测那几条专用：它们量的就是合成数据，不给库也照造。
    #[cfg(feature = "demo")]
    fn open_for_bench(
        &self,
        synthetic: impl FnOnce() -> Result<Catalog, String>,
    ) -> Result<Site, String> {
        if self.locate().given() {
            return self.locate().open();
        }
        demo::site(synthetic()?)
    }

    /// 这一趟的**工作目录**：子库那一屏排差量预览时要读它里头的媒体池与能力档案名册。
    ///
    /// 说了 `--workspace` 就用它；开的是现成的库就按 [`Locate`] 那条算；跑**合成数据**
    /// 时用一个**临时目录**——演示不该去翻维护者真正的那一份。
    ///
    /// **只有合成数据与实测那几条要它**：开现成的库那条路上，工作目录由
    /// [`Program::start`] 自己从三种给法里折（`Locate::workspace_dir`），两处各算一遍
    /// 迟早对不上。
    #[cfg(feature = "demo")]
    fn workspace_dir(&self) -> PathBuf {
        if self.workspace.is_some() || self.locate().given() {
            return self.locate().workspace_dir();
        }
        demo::workspace()
    }
}

fn main() -> ExitCode {
    let args = Args::parse();

    if args.font_check {
        return font_check(&args);
    }

    #[cfg(feature = "demo")]
    if let Some(code) = benches(&args) {
        return code;
    }

    let program = match args.start() {
        Ok(program) => program,
        Err(message) => return fail(&message),
    };

    // 标题里说清开的是哪一份。**开的是哪一屏也写进去**（票 `gui-redesign/12` 验收
    // 第 6 条）——那一半跟着屏变，所以由 [`App::window_title`] 每次换屏时重发一条。
    // 这一句只管**开窗到第一帧之间**那一小会儿：第一帧一画，`App` 就把带屏名的那个
    // 标题发下来了。
    let title = program.window_title();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([720.0, 480.0])
            .with_title(title),
        ..Default::default()
    };
    match eframe::run_native(
        "romcat",
        options,
        Box::new(|cc| {
            font::install(&cc.egui_ctx);
            Ok(Box::new(program))
        }),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("界面起不来：{error}");
            ExitCode::FAILURE
        }
    }
}

/// 实测那几条的派发。跑了哪一条就返回它的退出码，一条都没跑就是 `None`。
#[cfg(feature = "demo")]
fn benches(args: &Args) -> Option<ExitCode> {
    if let Some(frames) = args.bench {
        return Some(bench_scroll(args, frames));
    }
    // 量队列那一趟**最后真的落一批裁决下去**（那是「落下要多久」这个数的来源），
    // 所以它只许在合成数据上跑——落进真的沉淀库就是往用户的裁决里掺假数据。
    if let Some(frames) = args.bench_queue {
        if args.locate().given() {
            return Some(fail(
                "`--bench-queue` 最后会真的落一批裁决下去，只能在合成数据上跑。\n\
                 去掉 `--catalog` / `--library` / 主库根，改用 `--queue-rows <条数>` 定规模。",
            ));
        }
        let rows = args.queue_rows;
        let site = match args.open_for_bench(|| {
            demo::queue(rows).map_err(|error| format!("造不出合成数据：{error}"))
        }) {
            Ok(site) => site,
            Err(message) => return Some(fail(&message)),
        };
        let mut app = App::new(site, args.workspace_dir());
        let cost = bench::queue(&mut app, frames);
        print!("{}", cost.render());
        return Some(ExitCode::SUCCESS);
    }
    if let Some(frames) = args.bench_browse {
        return Some(bench_browse(args, frames));
    }
    if args.bench_paging {
        return Some(bench_paging(args));
    }
    if args.bench_sublibrary {
        return Some(bench_sublibrary(args));
    }
    None
}

/// 量一遍**变体表**：十万行虚拟滚动的代价（票 22）。
#[cfg(feature = "demo")]
fn bench_scroll(args: &Args, frames: u32) -> ExitCode {
    let rows = args.rows();
    let site = match args.open_for_bench(|| {
        demo::synthetic(rows).map_err(|error| format!("造不出合成数据：{error}"))
    }) {
        Ok(site) => site,
        Err(message) => return fail(&message),
    };
    let mut app = App::new(site, args.workspace_dir());
    app.show_view(View::Browse);
    let sweep = args.rows_per_frame.map_or(Sweep::Whole, Sweep::Rows);
    let cost = bench::scroll(&mut app, frames, sweep);
    println!(
        "{} 行、{}：{} 帧，中位 {:.2} ms（{:.0} fps），最慢 {:.2} ms；\
         内存里 {} 行，这一趟读库 {} 次",
        app.window().total(),
        match sweep {
            Sweep::Whole => "一趟滚到底".to_string(),
            Sweep::Rows(step) => format!("每帧 {step} 行"),
        },
        cost.frames,
        cost.median_ms,
        cost.fps(),
        cost.worst_ms,
        app.window().retained(),
        cost.reads,
    );
    ExitCode::SUCCESS
}

fn fail(message: &str) -> ExitCode {
    eprintln!("{message}");
    ExitCode::FAILURE
}

/// 量一遍**浏览屏**。开了现成的库就量真库，不然量合成数据。
///
/// 合成数据**照真库的形状造**（票 `parking-3/17`）：默认 [`demo::BROWSE_VARIANTS`] 个
/// 变体收敛成 [`demo::BROWSE_LINES`] 行。从前是十万个变体配写死的二十个作品——
/// 收出来 **7,712 行**（二十个作品加上那 1/13 压根没识别过的散行），而且**筛完
/// 只剩 473 行、比一扇窗还少**，于是那一趟「滚动」一次库都没读过。量出来的帧率
/// 不是维护者真会遇到的那个（挂单 `Q156`，实测三趟并排在 `docs/library-facts.md`）。
/// **「落一次整批收藏」那一步只在合成数据上量**：它真的往**沉淀库**里钉收藏，
/// 而那份东西不可再生（`bench::FavoriteCost`）。开了现成的库时那一步整个跳过，
/// 印出来的那一行如实说没量——与 `--bench-queue` 干脆拒绝在真库上跑同一条理由，
/// 只是这一条还有五样别的数值得在真库上量，不必整条拒绝。
#[cfg(feature = "demo")]
fn bench_browse(args: &Args, frames: u32) -> ExitCode {
    let rows = args.browse_rows();
    let works = args.works(rows);
    let synthetic = !args.locate().given();
    let site = match args.open_for_bench(|| {
        demo::browse_shaped(rows, works).map_err(|error| format!("造不出合成数据：{error}"))
    }) {
        Ok(site) => site,
        Err(message) => return fail(&message),
    };
    let mut app = App::new(site, args.workspace_dir());
    let cost = bench::browse(&mut app, frames, synthetic);
    print!("{}", cost.render());
    ExitCode::SUCCESS
}

/// 量一遍**主列表翻页**。开了现成的库就量真库，不然量合成数据。
///
/// 合成数据的**作品数**由 `--bench-works` 定，不给就按真库的比例折：主列表那条查询的
/// 代价跟着分出来多少组走，而这一条量的正是那笔钱（票 `gui-redesign/13`）。
/// 要看「同样多的变体收敛成多少行、查询就慢多少」，拧的就是这个旋钮。
#[cfg(feature = "demo")]
fn bench_paging(args: &Args) -> ExitCode {
    let rows = args.browse_rows();
    let works = args.works(rows);
    let site = match args.open_for_bench(|| {
        demo::browse_shaped(rows, works).map_err(|error| format!("造不出合成数据：{error}"))
    }) {
        Ok(site) => site,
        Err(message) => return fail(&message),
    };
    print!("{}", bench::paging(&site.catalog).render());
    ExitCode::SUCCESS
}

/// 量一遍**子库**那一屏。
///
/// **目标设备用本地 fixture 目录模拟**：在临时目录里造一个空目录当目标，绝不去动任何
/// 真实设备或 SD 卡。它会往中立库里建一个子库，所以**只在合成数据上跑**。
#[cfg(feature = "demo")]
fn bench_sublibrary(args: &Args) -> ExitCode {
    if args.locate().given() {
        return fail(
            "`--bench-sublibrary` 会往中立库里建一个子库，只能在合成数据上跑。\n             去掉 `--catalog` / `--library` / 主库根。",
        );
    }
    let rows = args.rows();
    let site = match demo::site(match demo::browse(rows) {
        Ok(catalog) => catalog,
        Err(error) => return fail(&format!("造不出合成数据：{error}")),
    }) {
        Ok(site) => site,
        Err(message) => return fail(&message),
    };
    let target = demo::workspace().join("目标设备-fixture");
    if let Err(error) = std::fs::create_dir_all(&target) {
        return fail(&format!("造不出 fixture 目标目录：{error}"));
    }
    let capacity = romcat_core::sublibrary::rule::parse_size(&args.bench_capacity);
    let mut app = App::new(site, args.workspace_dir());
    let cost = bench::sublibrary(&mut app, "实测掌机", &target, capacity, &args.bench_rule);
    println!(
        "规则：{}｜目标：{}",
        args.bench_rule,
        romcat_core::path::display(&target)
    );
    print!("{}", cost.render());
    ExitCode::SUCCESS
}

/// 装完字体跑一帧，问 egui 哪些字画不出来。
///
/// 开了现成的库就把**全部变体的键连同它们的作品名**过一遍。子集只覆盖
/// GBK + Big5 + 假名 + 那一段符号区，之外的字（CJK 扩展 B 的生僻字、谚文……）照样是
/// 豆腐块——这一趟就是用来知道自己的库撞不撞得上的，而不是等它在屏幕上出现
/// （`docs/research/egui-viability.md` 的第 2 步）。
///
/// **作品名也要过**（票 `parking-3/11`）：主列表第一列画的正是它，而它来自 DAT 与
/// **中文离线源**，跟变体的键根本不是同一批字。从前这一趟只过键，于是屏上最显眼的
/// 那一列反而没人替它查过豆腐块。作品名由页查询顺路带回来（`variant_browse_page`），
/// 不为它整份读一遍那张作品表。
fn font_check(args: &Args) -> ExitCode {
    let ctx = headless::context();
    headless::frame(&ctx, egui::RawInput::default(), |_| {});

    println!(
        "字体子集 {} 字节；必备字符 {} 个",
        font::subset_bytes(),
        font::REQUIRED.chars().count()
    );
    let mut missing: BTreeSet<char> = font::missing(&ctx, font::REQUIRED).into_iter().collect();
    for (_, text) in font::SAMPLE.iter().copied() {
        missing.extend(font::missing(&ctx, text));
    }

    if args.locate().given() {
        let site = match args.locate().open() {
            Ok(site) => site,
            Err(message) => return fail(&message),
        };
        let query = VariantQuery::default();
        let total = match site.catalog.variant_total(&query) {
            Ok(total) => total,
            Err(error) => return fail(&format!("读不动中立库：{error}")),
        };
        // 一页一页地过，理由与界面上那扇窗一样：不把全库读进内存。
        let page = romcat_core::catalog::MAX_PAGE;
        let mut offset = 0;
        while offset < total {
            match site.catalog.variant_browse_page(&query, offset, page) {
                Ok(rows) => {
                    for row in &rows {
                        missing.extend(font::missing(&ctx, &row.variant.key));
                        if let Some(work) = &row.work {
                            missing.extend(font::missing(&ctx, work));
                        }
                    }
                }
                Err(error) => return fail(&format!("读不动中立库：{error}")),
            }
            offset += page;
        }
        println!("过了中立库里 {total} 个变体的键，连它们的作品名一起");
    }

    if missing.is_empty() {
        println!("没有豆腐块。");
        ExitCode::SUCCESS
    } else {
        println!("画不出来：{}", missing.into_iter().collect::<String>());
        ExitCode::FAILURE
    }
}
