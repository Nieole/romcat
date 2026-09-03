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
use romcat_gui::app::{App, View};
use romcat_gui::bench::Sweep;
use romcat_gui::site::{Locate, Site};
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
    #[arg(long, value_name = "帧数", num_args = 0..=1, default_missing_value = "240")]
    bench_queue: Option<u32>,

    /// 每帧往下滚几行；不给就一趟滚完整张表（**最坏情况**，每帧都要读库）
    #[arg(long, value_name = "行数")]
    rows_per_frame: Option<f32>,

    /// 不开窗，检查必备字符有没有豆腐块。开了现成的库就连库里全部变体的键一起查
    #[arg(long)]
    font_check: bool,
}

impl Args {
    /// 说了要开现成的库吗。三样给法任给一样都算。
    fn wants_real(&self) -> bool {
        self.catalog.is_some() || self.library.is_some() || self.root.is_some()
    }

    /// 开一份现成的库；没说要开就造一份合成的。
    fn open(&self, synthetic: impl FnOnce() -> Result<Catalog, String>) -> Result<Site, String> {
        if self.wants_real() {
            return Site::open(&Locate {
                root: self.root.as_deref(),
                library: self.library.as_deref(),
                workspace: self.workspace.as_deref(),
                catalog: self.catalog.as_deref(),
            });
        }
        demo::site(synthetic()?)
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
        let mut app = App::new(site);
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

    let queue_rows = args.queue_rows;
    let site = match args
        .open(|| demo::queue(queue_rows).map_err(|error| format!("造不出合成数据：{error}")))
    {
        Ok(site) => site,
        Err(message) => return fail(&message),
    };

    if let Some(frames) = args.bench_queue {
        let mut app = App::new(site);
        let cost = bench::queue(&mut app, frames);
        print!("{}", cost.render());
        return ExitCode::SUCCESS;
    }

    let app = App::new(site);
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

    if args.wants_real() {
        let site = match Site::open(&Locate {
            root: args.root.as_deref(),
            library: args.library.as_deref(),
            workspace: args.workspace.as_deref(),
            catalog: args.catalog.as_deref(),
        }) {
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
