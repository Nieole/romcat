//! `romcat-gui`：界面的入口。
//!
//! 四种跑法：开窗（默认）、量变体表的帧率（`--bench`）、量**待确认队列**的响应
//! （`--bench-queue`）、查豆腐块（`--font-check`）。后三种不开窗，于是在没有显示器的
//! 地方也跑得起来——那几条验收因此可以进门禁。
//!
//! 开哪份库：给主库根或 `--library <名字>` 就按名字去工作目录里找，`--catalog <文件>`
//! 直接开一份，三样都不给就用**合成数据**（形状照真机来，见 [`demo`]）。
//! **一个字节都不读主库**（ADR-0001、ADR-0004）。

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use romcat_core::catalog::{Catalog, VariantQuery};
use romcat_core::site::Site;
use romcat_gui::app::{App, View};
use romcat_gui::bench::Sweep;
use romcat_gui::site::Locate;
use romcat_gui::{bench, demo, font, headless};

/// romcat 的界面。
#[derive(Debug, Parser)]
#[command(
    name = "romcat-gui",
    version,
    about = "romcat 界面：待确认队列与变体表"
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

    /// 直接打开这一份中立库文件；不给就按名字找，都不给就用合成数据
    #[arg(long, value_name = "文件")]
    catalog: Option<PathBuf>,

    /// 合成数据里的**待裁决**条数。**主库只读**，这条路一个字节都不碰真库
    #[arg(long, value_name = "条数", default_value_t = demo::QUEUE_ROWS)]
    queue_rows: u64,

    /// `--bench` 用的合成变体数（那一条量的是变体表，不是队列）
    #[arg(long, value_name = "行数", default_value_t = 100_000)]
    rows: u64,

    /// 不开窗，滚一遍变体表量每帧的代价，打印中位数与最慢的一帧
    #[arg(long, value_name = "帧数", num_args = 0..=1, default_missing_value = "240")]
    bench: Option<u32>,

    /// 不开窗，量**待确认队列**：列队列、换选择器、每帧、排计划各要多久
    ///
    /// **只在合成数据上跑**：它最后一步真的落一批裁决下去，那不该落进你的沉淀库
    #[arg(long, value_name = "帧数", num_args = 0..=1, default_missing_value = "240")]
    bench_queue: Option<u32>,

    /// 不开窗，量**库浏览**：列筛选面板、换一次筛选、点开一条、每帧各要多久
    #[arg(long, value_name = "帧数", num_args = 0..=1, default_missing_value = "240")]
    bench_browse: Option<u32>,

    /// 不开窗，量**子库**：建一个、写一条规则、排一次差量预览
    ///
    /// **目标设备一律用本地 fixture 目录模拟**：这条命令自己在临时目录里造一个空目录当
    /// 目标，绝不去动任何真实设备或 SD 卡
    #[arg(long)]
    bench_sublibrary: bool,

    /// `--bench-sublibrary` 用的规则；容量上限写在 `--bench-capacity`
    #[arg(long, value_name = "规则", default_value = "平台=SFC,GBA,MD")]
    bench_rule: String,

    /// `--bench-sublibrary` 里那个子库的容量上限，如 `512MB`
    #[arg(long, value_name = "容量", default_value = "512MB")]
    bench_capacity: String,

    /// 每帧往下滚几行；不给就一趟滚完整张表（**最坏情况**，每帧都要读库）
    #[arg(long, value_name = "行数")]
    rows_per_frame: Option<f32>,

    /// 不开窗，检查必备字符有没有豆腐块。开了现成的库就连库里全部变体的键一起查
    #[arg(long)]
    font_check: bool,
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

    /// 开一份现成的库；没说要开就造一份合成的。
    fn open(&self, synthetic: impl FnOnce() -> Result<Catalog, String>) -> Result<Site, String> {
        if self.locate().given() {
            return self.locate().open();
        }
        demo::site(synthetic()?)
    }

    /// 这一趟的**工作目录**：子库那一屏排差量预览时要读它里头的媒体池与能力档案名册。
    ///
    /// 说了 `--workspace` 就用它；开的是现成的库就按 [`Locate`] 那条算；跑合成数据时
    /// 用一个**临时目录**——演示不该去翻维护者真正的那一份（[`demo::workspace`]）。
    fn workspace_dir(&self) -> PathBuf {
        if self.workspace.is_some() || self.locate().given() {
            self.locate().workspace_dir()
        } else {
            demo::workspace()
        }
    }
}

fn main() -> ExitCode {
    let args = Args::parse();

    if args.font_check {
        return font_check(&args);
    }

    if let Some(frames) = args.bench {
        // 这一条量的是**变体表**：十万行虚拟滚动的代价（票 22）。
        let rows = args.rows;
        let site = match args
            .open(|| demo::synthetic(rows).map_err(|error| format!("造不出合成数据：{error}")))
        {
            Ok(site) => site,
            Err(message) => return fail(&message),
        };
        let mut app = App::new(site, args.workspace_dir());
        app.show_view(View::Variants);
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
        return ExitCode::SUCCESS;
    }

    // 量队列的那一趟**最后真的落一批裁决下去**（那是「落下要多久」这个数的来源），
    // 所以它只许在合成数据上跑——落进真的沉淀库就是往用户的裁决里掺假数据。
    if args.bench_queue.is_some() && args.locate().given() {
        return fail(
            "`--bench-queue` 最后会真的落一批裁决下去，只能在合成数据上跑。\n\
             去掉 `--catalog` / `--library` / 主库根，改用 `--queue-rows <条数>` 定规模。",
        );
    }

    let queue_rows = args.queue_rows;
    let site = match args
        .open(|| demo::queue(queue_rows).map_err(|error| format!("造不出合成数据：{error}")))
    {
        Ok(site) => site,
        Err(message) => return fail(&message),
    };

    if let Some(frames) = args.bench_queue {
        let mut app = App::new(site, args.workspace_dir());
        let cost = bench::queue(&mut app, frames);
        print!("{}", cost.render());
        return ExitCode::SUCCESS;
    }

    if let Some(frames) = args.bench_browse {
        return bench_browse(&args, frames);
    }

    if args.bench_sublibrary {
        return bench_sublibrary(&args);
    }

    let app = App::new(site, args.workspace_dir());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([720.0, 480.0])
            .with_title("romcat"),
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

fn fail(message: &str) -> ExitCode {
    eprintln!("{message}");
    ExitCode::FAILURE
}

/// 量一遍**库浏览**。开了现成的库就量真库，不然量合成数据。
fn bench_browse(args: &Args, frames: u32) -> ExitCode {
    let rows = args.rows;
    let site = match args
        .open(|| demo::library(rows).map_err(|error| format!("造不出合成数据：{error}")))
    {
        Ok(site) => site,
        Err(message) => return fail(&message),
    };
    let mut app = App::new(site, args.workspace_dir());
    let cost = bench::browse(&mut app, frames);
    print!("{}", cost.render());
    ExitCode::SUCCESS
}

/// 量一遍**子库**那一屏。
///
/// **目标设备用本地 fixture 目录模拟**：在临时目录里造一个空目录当目标，绝不去动任何
/// 真实设备或 SD 卡。它会往中立库里建一个子库，所以**只在合成数据上跑**。
fn bench_sublibrary(args: &Args) -> ExitCode {
    if args.locate().given() {
        return fail(
            "`--bench-sublibrary` 会往中立库里建一个子库，只能在合成数据上跑。\n             去掉 `--catalog` / `--library` / 主库根。",
        );
    }
    let rows = args.rows;
    let site = match demo::site(match demo::library(rows) {
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
