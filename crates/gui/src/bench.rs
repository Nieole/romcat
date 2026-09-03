//! 不开窗量帧率。
//!
//! ## 为什么不开窗也算数
//!
//! 一帧的代价分两半：CPU 上的**布局 + 三角化**，与 GPU 上的**画三角形**。表格这种界面
//! 后一半是常数级的几千个三角形，真正会随行数涨的是前一半——而前一半
//! [`crate::headless`] 里那条通路就跑得完，一个像素都不用画。
//!
//! 跑的是 [`crate::app::App::ui`] **本人**，不是另写一份简化版。量出来的数字是
//! 「这个界面每帧要花多少 CPU」，与「开窗之后看起来顺不顺」之间只差一个 GPU 常数。
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

use romcat_core::report::thousands;
use romcat_core::triage::{Axis, Draft, Overrides};

use crate::app::App;
use crate::headless::{self, VIEWPORT};
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
    let ctx = headless::context();

    // 先跑一帧把总行数问出来，滚动的行程要按它算。
    app.scroll_to = Some(0.0);
    headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
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
        let output = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
        let _ = ctx.tessellate(output.shapes, output.pixels_per_point);
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
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

/// **待确认队列**量出来的响应，毫秒。
///
/// 量的不是「一帧多少毫秒」这一样：队列上人真正会等的是**列一次队列**（真机 16,656 条）
/// 与**点一行分组表**（重新筛一遍再重新分一次组）。三样一起报，因为它们的量级差着
/// 两三个数量级，只报其中一样会把结论带偏。
#[derive(Debug, Clone, PartialEq)]
pub struct QueueCost {
    /// 队列里有多少条待裁决。
    pub queue: u64,
    /// 眼下的选择器选中多少条。
    pub selected: u64,
    /// **列一次队列**要多久：中立库连表带候选走一遍，再照沉淀库剔掉裁决过的。
    pub load_ms: f64,
    /// **点一行分组表**要多久：就地重筛加按三个轴重新分组，**一次都不读库**。
    pub filter_ms: f64,
    /// 点中的那一批有多少条。
    pub batch: u64,
    /// 点的是哪个轴上的哪一组。
    pub batch_label: String,
    /// 每帧的中位数。
    pub median_ms: f64,
    /// 最慢的一帧。
    pub worst_ms: f64,
    /// 量了几帧。
    pub frames: u32,
    /// **排一次计划**要多久（选中的那些补内容判据 + 逐条查沉淀库）。
    pub plan_ms: f64,
    /// 这份计划要落下几条。
    pub planned: u64,
    /// **落下**要多久：写沉淀库、当场在中立库里兑现、把裁完的从队列里去掉。
    pub apply_ms: f64,
    /// 落下之后队列还剩多少条。
    pub left: u64,
}

impl QueueCost {
    /// 按中位数折成每秒多少帧。
    #[must_use]
    pub fn fps(&self) -> f64 {
        if self.median_ms <= 0.0 {
            f64::INFINITY
        } else {
            1000.0 / self.median_ms
        }
    }

    /// 排成给人看的几行。
    #[must_use]
    pub fn render(&self) -> String {
        format!(
            "待确认队列 {} 条；选中 {} 条\n\
             列一次队列      {:.1} ms\n\
             点一行分组表    {:.2} ms（{}，选中 {} 条，一次都不读库）\n\
             每帧            中位 {:.2} ms（{:.0} fps），最慢 {:.2} ms，共 {} 帧\n\
             排一次计划      {:.1} ms（{} 条）\n\
             落下            {:.1} ms；队列还剩 {} 条\n",
            thousands(self.queue),
            thousands(self.selected),
            self.load_ms,
            self.filter_ms,
            self.batch_label,
            thousands(self.batch),
            self.median_ms,
            self.fps(),
            self.worst_ms,
            self.frames,
            self.plan_ms,
            thousands(self.planned),
            self.apply_ms,
            thousands(self.left),
        )
    }
}

/// 量一遍**待确认队列**：列队列、换选择器、滚一趟、排计划、落下。
///
/// 走的是界面上那条一模一样的路——[`App::ui`] 本人、[`crate::queue::Screen::pick`]
/// 本人、[`crate::queue::Screen::preview`] 本人。落下那一步真的写库，所以它只该在
/// **合成数据**上跑（沉淀库在内存里，主库一个字节都不碰）。
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn queue(app: &mut App, frames: u32) -> QueueCost {
    let ctx = headless::context();

    // 一、列一次队列。
    let started = Instant::now();
    {
        let (screen, site) = app.queue_and_site();
        screen.reload(site);
    }
    let load_ms = started.elapsed().as_secs_f64() * 1000.0;
    let (queue_total, selected) = {
        let queue = app.queue().queue();
        (queue.pending(), queue.selected().len() as u64)
    };

    // 二、点一行分组表：挑三个轴上**最大的那一组**——ADR-0002 说队列只能逐条点就等于
    //     没有，所以量的该是「一次盖住几百上千条」那种批，不是最小的那种。
    let biggest = Axis::ALL
        .iter()
        .filter_map(|axis| {
            app.queue()
                .queue()
                .groups(*axis)
                .first()
                .map(|row| (row.count, *axis, row.label.clone()))
        })
        .max_by_key(|(count, _, _)| *count);
    let started = Instant::now();
    if let Some((_, axis, label)) = &biggest {
        let (screen, _) = app.queue_and_site();
        screen.pick(*axis, label);
    }
    // 换选择器是**下一帧**才兑现的（界面每帧把草稿写进队列），所以这一帧要跑完才算数。
    headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let filter_ms = started.elapsed().as_secs_f64() * 1000.0;
    let batch = app.queue().queue().selected().len() as u64;

    // 三、滚一趟：表格是虚拟化的，代价该与队列有多少条无关。
    let travel = (batch as f32 * row_pitch() - VIEWPORT[1]).max(0.0);
    let mut costs: Vec<f64> = Vec::with_capacity(frames as usize);
    for frame in 0..frames {
        let at = if frames > 1 {
            travel * frame as f32 / (frames - 1) as f32
        } else {
            0.0
        };
        {
            let (screen, _) = app.queue_and_site();
            screen.scroll_to = Some(at);
        }
        let started = Instant::now();
        let output = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
        let _ = ctx.tessellate(output.shapes, output.pixels_per_point);
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        // 前两帧是热身：字体图集与列宽都在这两帧里定下来。
        if frame >= 2 {
            costs.push(elapsed);
        }
    }
    {
        let (screen, _) = app.queue_and_site();
        screen.scroll_to = None;
    }
    costs.sort_by(f64::total_cmp);

    // 四、排一次计划：**手工指定作品**外加一个汉化组——队列里绝大多数条目一条候选
    //     都没有，那是主路径。
    let draft = Draft {
        work: Some("实测作品".to_string()),
        overrides: Overrides {
            team: Some("实测汉化组".to_string()),
            ..Overrides::default()
        },
        ..Draft::default()
    };
    let started = Instant::now();
    {
        let (screen, site) = app.queue_and_site();
        screen.preview(site, &draft);
    }
    let plan_ms = started.elapsed().as_secs_f64() * 1000.0;
    let planned = app
        .queue()
        .pending()
        .map_or(0, |plan| plan.decided.len() as u64);

    // 五、落下。
    let started = Instant::now();
    {
        let (screen, site) = app.queue_and_site();
        screen.commit(site);
    }
    let apply_ms = started.elapsed().as_secs_f64() * 1000.0;

    QueueCost {
        queue: queue_total,
        selected,
        load_ms,
        filter_ms,
        batch,
        batch_label: biggest
            .map(|(_, axis, label)| format!("{} {label}", axis.selector()))
            .unwrap_or_default(),
        median_ms: costs.get(costs.len() / 2).copied().unwrap_or(0.0),
        worst_ms: costs.last().copied().unwrap_or(0.0),
        frames: u32::try_from(costs.len()).unwrap_or(u32::MAX),
        plan_ms,
        planned,
        apply_ms,
        left: app.queue().queue().pending(),
    }
}
