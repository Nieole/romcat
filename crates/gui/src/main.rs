//! `romcat-gui`：界面的入口。
//!
//! 三种跑法：开窗（默认）、量帧率（`--bench`）、查豆腐块（`--font-check`）。后两种
//! 不开窗，于是在没有显示器的地方也跑得起来——这张票的两条验收因此可以进门禁。

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use romcat_core::catalog::Catalog;
use romcat_gui::app::App;
use romcat_gui::bench::Sweep;
use romcat_gui::{bench, demo, font};

/// romcat 的界面。
#[derive(Debug, Parser)]
#[command(name = "romcat-gui", version, about = "romcat 界面：变体表与详情面板")]
struct Args {
    /// 打开一份现成的**中立库**；不给就用合成数据。
    #[arg(long, value_name = "文件")]
    catalog: Option<PathBuf>,

    /// 合成数据造多少个变体。**主库只读**，这条路一个字节都不碰真库。
    #[arg(long, value_name = "行数", default_value_t = 100_000)]
    rows: u64,

    /// 不开窗，滚一遍十万行量每帧的代价，打印中位数与最慢的一帧。
    #[arg(long, value_name = "帧数", num_args = 0..=1, default_missing_value = "240")]
    bench: Option<u32>,

    /// 每帧往下滚几行；不给就一趟滚完整张表（**最坏情况**，每帧都要读库）。
    #[arg(long, value_name = "行数")]
    rows_per_frame: Option<f32>,

    /// 不开窗，检查必备字符里有没有豆腐块。
    #[arg(long)]
    font_check: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();

    if args.font_check {
        return font_check();
    }

    let catalog = match &args.catalog {
        Some(path) => match Catalog::open(path) {
            Ok(catalog) => catalog,
            Err(error) => {
                eprintln!("打不开中立库：{error}");
                return ExitCode::FAILURE;
            }
        },
        None => match demo::synthetic(args.rows) {
            Ok(catalog) => catalog,
            Err(error) => {
                eprintln!("造不出合成数据：{error}");
                return ExitCode::FAILURE;
            }
        },
    };

    let mut app = App::new(catalog);

    if let Some(frames) = args.bench {
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
            App::setup(&cc.egui_ctx);
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

/// 装完字体跑一帧，问 egui 必备字符里哪些画不出来。
fn font_check() -> ExitCode {
    let ctx = egui::Context::default();
    App::setup(&ctx);
    // 没有显卡在收字体图集，得显式认领掉这批纹理增量，否则 `epaint` 会在丢弃时 panic。
    ctx.run_ui(egui::RawInput::default(), |_| {})
        .textures_delta
        .clear();
    let missing = font::missing(&ctx, font::REQUIRED);
    println!(
        "字体子集 {} 字节；必备字符 {} 个",
        font::subset_bytes(),
        font::REQUIRED.chars().count()
    );
    if missing.is_empty() {
        println!("没有豆腐块。");
        ExitCode::SUCCESS
    } else {
        println!("画不出来：{}", missing.into_iter().collect::<String>());
        ExitCode::FAILURE
    }
}
