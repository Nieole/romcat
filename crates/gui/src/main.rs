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
//! 直接开一份。**三样都不给就如实报错**——不擅自造一份合成的糊弄人：假数据与真库在
//! 界面上长得一模一样，看见一屏假名字的第一反应会是「我的库怎么了」。要看合成数据
//! （形状照真机来，见 [`demo`]）得显式给 `--demo`。
//! **一个字节都不读主库**（ADR-0001、ADR-0004）。

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
#[cfg(feature = "demo")]
use romcat_core::catalog::Catalog;
use romcat_core::catalog::VariantQuery;
#[cfg(feature = "demo")]
use romcat_core::site::Site;
use romcat_gui::app::App;
#[cfg(feature = "demo")]
use romcat_gui::app::View;
#[cfg(feature = "demo")]
use romcat_gui::bench::Sweep;
use romcat_gui::site::Locate;
#[cfg(feature = "demo")]
use romcat_gui::{bench, demo};
use romcat_gui::{font, headless};

/// 实测那几条不给 `--rows` 时造多少个变体。**十万行是变体表那条线的量级**（票 22）。
#[cfg(feature = "demo")]
const BENCH_ROWS: u64 = 100_000;

/// `--bench-paging` 不给 `--rows` 时造多少个变体。
///
/// **它与 [`PAGING_WORKS`] 是一对**：46,428 个变体、7,407 个作品，加上 1/13 那批
/// 「还没识别」的（一个变体一行），正好收出真库的形状——**10,978 行**。
/// 别的实测量的是帧率与面板，与收敛成多少行无关，所以它们照旧用 [`BENCH_ROWS`]。
/// 数字的出处见 `docs/library-facts.md`（票 `gui-redesign/13`）。
#[cfg(feature = "demo")]
const PAGING_ROWS: u64 = 46_428;

/// `--bench-paging` 不给 `--bench-works` 时造多少个作品。见 [`PAGING_ROWS`]。
#[cfg(feature = "demo")]
const PAGING_WORKS: usize = 7_407;

/// 没说开哪份库时说的那句话。**不擅自造一份假的**。
const NO_LIBRARY: &str = "说清要开哪份库：\n\
     \x20 romcat-gui <主库根>\n\
     \x20 romcat-gui --library <名字> [--workspace <目录>]\n\
     \x20 romcat-gui --catalog <中立库文件>\n\
     \n\
     只想看看界面长什么样：`cargo run -p romcat-gui --features demo -- --demo`。";

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
    /// 不给就按各条自己的默认：`--bench-paging` 是 46,428（真库的规模），其余是 100,000
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

    /// `--bench-paging` 那份合成数据里有几个作品，也就是那些变体收敛成多少行
    ///
    /// 默认 7,407，配上默认的 46,428 个变体（其中 1/13 是「还没识别」，一个变体一行），
    /// **正好收出真库的形状：46,428 变体 → 10,978 行**。两个数是一对，动一个就得动另一个
    #[cfg(feature = "demo")]
    #[arg(long, value_name = "个数", default_value_t = PAGING_WORKS)]
    bench_works: usize,

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

    /// 同上，但 `--bench-paging` 那条的默认是 [`PAGING_ROWS`]——**它量的是收敛成多少行，
    /// 而那正是真库形状的那一半**，拿十万行量出来的数与 `docs/library-facts.md` 那张表
    /// 不可比。
    #[cfg(feature = "demo")]
    fn paging_rows(&self) -> u64 {
        self.rows.unwrap_or(PAGING_ROWS)
    }

    /// 说了要开合成数据吗。没编进 `demo` feature 时**永远是否**。
    #[cfg(feature = "demo")]
    fn wants_demo(&self) -> bool {
        self.demo || self.benching()
    }

    /// 没编进 `demo` feature：合成数据这条路根本不存在。
    #[cfg(not(feature = "demo"))]
    const fn wants_demo(&self) -> bool {
        false
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

    /// 开一份现成的库。**没说开哪份、也没要合成数据，就如实报错。**
    ///
    /// 以前这里是「都不给就悄悄造一份合成的」。合成数据与真库在界面上长得一模一样，
    /// 于是不带参数打开看见的是一屏假名字，第一反应是「我的库怎么了」而不是
    /// 「我打开的不是我的库」——**不报错的错比报错的错难查得多**。
    #[cfg(feature = "demo")]
    fn open(&self, synthetic: impl FnOnce() -> Result<Catalog, String>) -> Result<Site, String> {
        if self.locate().given() {
            return self.locate().open();
        }
        if !self.wants_demo() {
            return Err(NO_LIBRARY.to_string());
        }
        demo::site(synthetic()?)
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
    fn workspace_dir(&self) -> PathBuf {
        if self.workspace.is_some() || self.locate().given() {
            return self.locate().workspace_dir();
        }
        #[cfg(feature = "demo")]
        {
            demo::workspace()
        }
        #[cfg(not(feature = "demo"))]
        {
            romcat_core::workspace::default_dir()
        }
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

    if !args.locate().given() && !args.wants_demo() {
        return fail(NO_LIBRARY);
    }

    #[cfg(feature = "demo")]
    let site = match args
        .open(|| demo::queue(args.queue_rows).map_err(|error| format!("造不出合成数据：{error}")))
    {
        Ok(site) => site,
        Err(message) => return fail(&message),
    };
    #[cfg(not(feature = "demo"))]
    let site = match args.locate().open() {
        Ok(site) => site,
        Err(message) => return fail(&message),
    };

    // 标题里说清开的是哪一份。合成数据与真库在界面上长得一模一样，标题是唯一
    // 一直看得见的区分处。**开的是哪一屏也写进去**（票 `gui-redesign/12` 验收第 6 条）
    // ——那一半跟着屏变，所以由 [`App::window_title`] 每次换屏时重发一条。
    let mut app = App::new(site, args.workspace_dir());
    if !args.locate().given() {
        app.set_library_label("合成数据（演示）");
    }
    // 这一句只管**开窗到第一帧之间**那一小会儿：第一帧一画，`App` 就把带屏名的那个
    // 标题发下来了。
    let title = app.window_title();
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
            Ok(Box::new(app))
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
/// **「落一次整批收藏」那一步只在合成数据上量**：它真的往**沉淀库**里钉收藏，
/// 而那份东西不可再生（`bench::FavoriteCost`）。开了现成的库时那一步整个跳过，
/// 印出来的那一行如实说没量——与 `--bench-queue` 干脆拒绝在真库上跑同一条理由，
/// 只是这一条还有五样别的数值得在真库上量，不必整条拒绝。
#[cfg(feature = "demo")]
fn bench_browse(args: &Args, frames: u32) -> ExitCode {
    let rows = args.rows();
    let synthetic = !args.locate().given();
    let site = match args
        .open_for_bench(|| demo::browse(rows).map_err(|error| format!("造不出合成数据：{error}")))
    {
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
/// 合成数据的**作品数**由 `--bench-works` 定：主列表那条查询的代价跟着分出来多少组走，
/// 二十个作品收出来的 3,596 行不是真库的形状（票 `gui-redesign/13`）。
#[cfg(feature = "demo")]
fn bench_paging(args: &Args) -> ExitCode {
    let (rows, works) = (args.paging_rows(), args.bench_works);
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
/// 开了现成的库就把**全部变体的键**过一遍。子集只覆盖 GBK + Big5 + 假名 + 那一段符号区，
/// 之外的字（CJK 扩展 B 的生僻字、谚文……）照样是豆腐块——这一趟就是用来知道自己的库
/// 撞不撞得上的，而不是等它在屏幕上出现（`docs/research/egui-viability.md` 的第 2 步）。
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
            match site.catalog.variant_page(&query, offset, page) {
                Ok(rows) => {
                    for row in &rows {
                        missing.extend(font::missing(&ctx, &row.key));
                    }
                }
                Err(error) => return fail(&format!("读不动中立库：{error}")),
            }
            offset += page;
        }
        println!("过了中立库里 {total} 个变体的键");
    }

    if missing.is_empty() {
        println!("没有豆腐块。");
        ExitCode::SUCCESS
    } else {
        println!("画不出来：{}", missing.into_iter().collect::<String>());
        ExitCode::FAILURE
    }
}
