//! `romcat-gui`：界面的入口。
//!
//! 三种跑法：开窗（默认）、量帧率（`--bench`）、查豆腐块（`--font-check`）。后两种
//! 不开窗，于是在没有显示器的地方也跑得起来——这张票的两条验收因此可以进门禁。

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use romcat_core::catalog::{Catalog, VariantQuery};
use romcat_gui::app::App;
use romcat_gui::bench::Sweep;
use romcat_gui::{bench, demo, font, headless};

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

    /// 不开窗，检查必备字符有没有豆腐块。给了 `--catalog` 就连库里全部变体的键一起查。
    #[arg(long)]
    font_check: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();

    let catalog = match &args.catalog {
        Some(path) => match Catalog::open(path) {
            Ok(catalog) => Some(catalog),
            Err(error) => {
                eprintln!("打不开中立库：{error}");
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };

    if args.font_check {
        return font_check(catalog.as_ref());
    }

    let catalog = match catalog {
        Some(catalog) => catalog,
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

/// 装完字体跑一帧，问 egui 哪些字画不出来。
///
/// 给了中立库就把**全部变体的键**过一遍。子集只覆盖 GBK + Big5 + 假名 + 那一段符号区，
/// 之外的字（CJK 扩展 B 的生僻字、谚文……）照样是豆腐块——这一趟就是用来知道自己的库
/// 撞不撞得上的，而不是等它在屏幕上出现（`docs/research/egui-viability.md` 的第 2 步）。
fn font_check(catalog: Option<&Catalog>) -> ExitCode {
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

    if let Some(catalog) = catalog {
        let query = VariantQuery::default();
        let total = match catalog.variant_total(&query) {
            Ok(total) => total,
            Err(error) => {
                eprintln!("读不动中立库：{error}");
                return ExitCode::FAILURE;
            }
        };
        // 一页一页地过，理由与界面上那扇窗一样：不把全库读进内存。
        let page = romcat_core::catalog::MAX_PAGE;
        let mut offset = 0;
        while offset < total {
            match catalog.variant_page(&query, offset, page) {
                Ok(rows) => {
                    for row in &rows {
                        missing.extend(font::missing(&ctx, &row.key));
                    }
                }
                Err(error) => {
                    eprintln!("读不动中立库：{error}");
                    return ExitCode::FAILURE;
                }
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
