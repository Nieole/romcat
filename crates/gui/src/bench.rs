//! 不开窗量帧率。
//!
//! ## 为什么不开窗也算数
//!
//! 一帧的代价分两半：CPU 上的**布局 + 三角化**，与 GPU 上的**画三角形**。表格这种界面
//! 后一半是常数级的几千个三角形，真正会随行数涨的是前一半——而前一半 `egui::Context`
//! 自己就跑得完，一个像素都不用画。
//!
//! 于是这里跑的是 [`egui::Context::run_ui`] 加 [`egui::Context::tessellate`]，走的是
//! [`crate::app::App::ui`] **本人**，不是另写一份简化版。量出来的数字是「这个界面每帧
//! 要花多少 CPU」，与「开窗之后看起来顺不顺」之间只差一个 GPU 常数。
//!
//! 顺带，这也是这张票的帧率验收能进门禁的原因：不开窗就不需要显示器。
//!
//! ## 两种滚法量的是两件事
//!
//! [`Sweep::Whole`] 是**最坏情况**：一趟从第一行滚到最后一行，每帧跨出的行数都超过一整个
//! 窗口，于是每帧都要读一次库。人手上做不出这么快的滚动，它量的是「拖着滚动条一路甩到底」
//! 的下限。
//!
//! [`Sweep::Rows`] 是**常态**：每帧往下几行，读库只在跨出窗口时发生。它量的是
//! `body.rows()` 那一半——那一半的开销**与总行数无关**，这正是选 `egui_extras` 的前提。

use std::time::Instant;

use crate::app::App;
use crate::table::ROW_HEIGHT;

/// 一次实测的结果，毫秒。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameCost {
    /// 量了几帧。
    pub frames: u32,
    /// 中位数。
    pub median_ms: f64,
    /// 最慢的一帧。**跨窗口读库的那几帧落在这里**。
    pub worst_ms: f64,
    /// 这一趟读了几次中立库。
    pub reads: u64,
}

impl FrameCost {
    /// 按中位数折成每秒多少帧。
    #[must_use]
    pub fn fps(&self) -> f64 {
        if self.median_ms <= 0.0 {
            f64::INFINITY
        } else {
            1000.0 / self.median_ms
        }
    }
}

/// 怎么滚。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sweep {
    /// 整张表在给定帧数里滚完。每帧跨的行数与总行数成正比——**最坏情况**。
    Whole,
    /// 每帧往下滚这么多行。人手滚轮的量级。
    Rows(f32),
}

/// 视口大小，点。定死一个常见的窗口尺寸，免得量出来的数字随屏幕变。
const VIEWPORT: [f32; 2] = [1280.0, 800.0];

/// 一行占多高，含行距。
fn row_pitch() -> f32 {
    ROW_HEIGHT + egui::Style::default().spacing.item_spacing.y
}

/// 滚一趟，量每帧的 CPU 代价。
///
/// 头两帧不计——第一帧要把用到的汉字栅格化进字体图集，那是**一次性**开销，
/// 算进稳态帧率里会把结论带偏。
#[must_use]
pub fn scroll(app: &mut App, frames: u32, sweep: Sweep) -> FrameCost {
    let ctx = egui::Context::default();
    App::setup(&ctx);
    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, VIEWPORT.into())),
        ..Default::default()
    };

    // 先跑一帧把总行数问出来，滚动的行程要按它算。
    app.scroll_to = Some(0.0);
    let mut output = ctx.run_ui(input(), |ui| app.ui(ui));
    // 没有显卡在收字体图集，得显式认领掉这批纹理增量，否则 `epaint` 会在丢弃时 panic。
    output.textures_delta.clear();
    let before = app.window().reads();
    let travel = (app.window().total() as f32 * row_pitch() - VIEWPORT[1]).max(0.0);

    let mut costs: Vec<f64> = Vec::with_capacity(frames as usize);
    for frame in 0..frames {
        let at = match sweep {
            Sweep::Whole if frames > 1 => travel * frame as f32 / (frames - 1) as f32,
            Sweep::Whole => 0.0,
            Sweep::Rows(step) => (frame as f32 * step * row_pitch()).min(travel),
        };
        app.scroll_to = Some(at);
        let started = Instant::now();
        let mut output = ctx.run_ui(input(), |ui| app.ui(ui));
        let _ = ctx.tessellate(output.shapes, output.pixels_per_point);
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        output.textures_delta.clear();
        // 前两帧是热身：字体图集与列宽都在这两帧里定下来。
        if frame >= 2 {
            costs.push(elapsed);
        }
    }
    app.scroll_to = None;

    let reads = app.window().reads() - before;
    costs.sort_by(f64::total_cmp);
    FrameCost {
        frames: u32::try_from(costs.len()).unwrap_or(u32::MAX),
        median_ms: costs.get(costs.len() / 2).copied().unwrap_or(0.0),
        worst_ms: costs.last().copied().unwrap_or(0.0),
        reads,
    }
}
