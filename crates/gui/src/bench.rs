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

use romcat_core::catalog::browse::{WorkOrder, WorkQuery};
use romcat_core::catalog::{Catalog, PlatformFilter, VariantQuery};
use romcat_core::report::thousands;
use romcat_core::triage::{Axis, Draft, ItemOrder, Overrides};

use crate::app::App;
use crate::headless::{self, VIEWPORT};
use crate::table::{ROW_HEIGHT, SPAN};

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
    app.browse_and_site().0.scroll_to = Some(0.0);
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
        app.browse_and_site().0.scroll_to = Some(at);
        let started = Instant::now();
        let output = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
        let _ = ctx.tessellate(output.shapes, output.pixels_per_point);
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        // 前两帧是热身：字体图集与列宽都在这两帧里定下来。
        if frame >= 2 {
            costs.push(elapsed);
        }
    }
    app.browse_and_site().0.scroll_to = None;

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
    /// **点一下表头**要多久：整份在内存里重排一遍再重新分一次区，**一次都不读库**。
    ///
    /// 量的是**整个队列**那一次，不是筛完之后那一小批：排序排的是手上这份 `Vec` 的
    /// 全部条目（`Queue::set_order`），换个选择器不会让它变便宜。
    pub sort_ms: f64,
    /// 那一下排的是几条——**整份**，见 [`sort_ms`](Self::sort_ms)。
    ///
    /// 它比 [`selected`](Self::selected) 大：选中的那些不含**跳过**（默认选择器筛掉了
    /// 它们），而排序排的是内存里全部条目，跳过的那些也在里头。
    pub sorted: u64,
    /// 排的是哪一列，倒着还是正着。
    pub sort_label: String,
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
             点一下表头      {:.1} ms（{}，整份 {} 条重排再分区，一次都不读库）\n\
             点一行分组表    {:.2} ms（{}，选中 {} 条，一次都不读库）\n\
             每帧            中位 {:.2} ms（{:.0} fps），最慢 {:.2} ms，共 {} 帧\n\
             排一次计划      {:.1} ms（{} 条）\n\
             落下            {:.1} ms；队列还剩 {} 条\n",
            thousands(self.queue),
            thousands(self.selected),
            self.load_ms,
            self.sort_ms,
            self.sort_label,
            thousands(self.sorted),
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
        // **量的是那张虚拟化的表**，而它在**逐条**那一屏上——批优先是打开时的默认
        // （票 `gui-redesign/09`）。不切过去的话下面那一趟滚动一行都没画。
        screen.show_one_by_one();
    }
    let load_ms = started.elapsed().as_secs_f64() * 1000.0;
    let (queue_total, selected) = {
        let queue = app.queue().queue();
        (queue.pending(), queue.selected().len() as u64)
    };

    // 二、点一下表头：**按容量倒着排**。那正是这一票要给人的动作——「16,656 条里
    //     哪几条最大」，从前只能靠选择器缩小范围。排的是整份条目，一次都不读库。
    let sort = (ItemOrder::Bytes, true);
    // 排的是**内存里全部条目**，跳过的那些也在里头——报出去的得是这个数，
    // 不是「选中多少条」（那个不含跳过）。
    let sorted = {
        let queue = app.queue().queue();
        queue.pending() + queue.skipped()
    };
    let started = Instant::now();
    {
        let (screen, _) = app.queue_and_site();
        screen.sort_by(sort.0, sort.1);
    }
    let sort_ms = started.elapsed().as_secs_f64() * 1000.0;

    // 三、点一行分组表：挑三个轴上**最大的那一组**——ADR-0002 说队列只能逐条点就等于
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

    // 四、滚一趟：表格是虚拟化的，代价该与队列有多少条无关。
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

    // 五、排一次计划：**手工指定作品**外加一个汉化组——队列里绝大多数条目一条候选
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

    // 六、落下。
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
        sort_ms,
        sorted,
        sort_label: format!("{}{}", sort.0.label(), if sort.1 { " ▼" } else { " ▲" }),
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

/// **浏览屏**量出来的响应，毫秒。
///
/// 量的不是「一帧多少毫秒」这一样：这一屏上人真正会等的是**列一次筛选面板**
/// （四条 `GROUP BY`）、**换一次筛选**（换一套 `WHERE` 再数一次总行数）、
/// **点开一行**（把作品底下那几个变体连候选一起折出来）、**全选之后展开作用范围**。
/// 几样的量级差着一两个数量级，只报其中一样会把结论带偏。
#[derive(Debug, Clone, PartialEq)]
pub struct BrowseCost {
    /// 主列表一共多少行（**一个作品一行**）。
    pub rows: u64,
    /// **筛完之后**那一批全选展开出多少个变体。它与 [`filtered`](Self::filtered) 是一对
    /// ——与 [`rows`](Self::rows) 不是（那是没筛之前的行数），两者并排读会得出一个
    /// 没有意义的比值。
    pub variants: u64,
    /// **全选之后展开作用范围**要多久：把选中的那几行折成一串变体的键。
    pub scope_ms: f64,
    /// **列一次筛选面板**要多久（连表里那一列作品名靠的那张小表一起）。
    pub facets_ms: f64,
    /// 平台、合集、语言、中文各有几个可选值。
    pub facet_counts: (usize, usize, usize, usize),
    /// **换一次筛选**要多久：换一套 `WHERE`、重数总行数、重取一页。
    pub filter_ms: f64,
    /// 换成了哪一条。
    pub filter_label: String,
    /// 换完之后剩多少行。
    pub filtered: u64,
    /// **点开一行**要多久。
    pub detail_ms: f64,
    /// 每帧的中位数。
    pub median_ms: f64,
    /// 最慢的一帧。
    pub worst_ms: f64,
    /// 量了几帧。
    pub frames: u32,
    /// 这一趟读了几次库。
    pub reads: u64,
    /// 内存里留了几行。
    pub retained: usize,
}

impl BrowseCost {
    /// 排成给人看的几行。
    #[must_use]
    pub fn render(&self) -> String {
        format!(
            "浏览屏 {} 行作品\n\
             列一次筛选面板  {:.1} ms（平台 {} 个、合集 {} 个、语言 {} 个、中文 {} 个）\n\
             换一次筛选      {:.1} ms（{}，剩 {} 行）\n\
             点开一行        {:.2} ms（作品、它的变体、每个变体的候选与依据一次折齐）\n\
             全选展开范围    {:.1} ms（筛完那 {} 行 → {} 个变体）\n\
             每帧            中位 {:.2} ms，最慢 {:.2} ms，共 {} 帧\n\
             滚一趟读库      {} 次；内存里始终 {} 行\n",
            thousands(self.rows),
            self.facets_ms,
            self.facet_counts.0,
            self.facet_counts.1,
            self.facet_counts.2,
            self.facet_counts.3,
            self.filter_ms,
            self.filter_label,
            thousands(self.filtered),
            self.detail_ms,
            self.scope_ms,
            thousands(self.filtered),
            thousands(self.variants),
            self.median_ms,
            self.worst_ms,
            self.frames,
            self.reads,
            self.retained,
        )
    }
}

/// 量一遍**浏览屏**：列筛选面板、换一次筛选、点开一行、全选展开、滚一趟。
///
/// 走的是界面上那条一模一样的路——[`App::ui`] 本人、
/// [`crate::browse::Screen::open_work`] 本人。**一个字节都不写库。**
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn browse(app: &mut App, frames: u32) -> BrowseCost {
    let ctx = headless::context();
    app.show_view(crate::app::View::Browse);

    // 一、列一次筛选面板。
    let started = Instant::now();
    {
        let (browse, site) = app.browse_and_site();
        browse.reload(site);
    }
    let facets_ms = started.elapsed().as_secs_f64() * 1000.0;
    let counts = {
        let facets = app.browse().facets();
        (
            facets.platforms.len(),
            facets.collections.len(),
            facets.languages.len(),
            facets.chinese.len(),
        )
    };

    // 先跑一帧把总行数问出来。
    headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let rows = app.window().total();

    // 二、换一次筛选：挑**最大的那个平台**——量的该是「筛完还剩上万行」那种，
    //     不是最小的那种。
    let biggest = app
        .browse()
        .facets()
        .platforms
        .first()
        .map(|facet| facet.value.clone());
    let started = Instant::now();
    if let Some(platform) = biggest.clone() {
        app.browse_and_site().0.query_mut().platform =
            Some(romcat_core::catalog::PlatformFilter::from_label(&platform));
    }
    // 换筛选是**下一帧**才兑现的（界面每帧把查询写进窗口），所以这一帧要跑完才算数。
    headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let filter_ms = started.elapsed().as_secs_f64() * 1000.0;
    let filtered = app.window().total();

    // 三、点开一行：作品、它底下那几个变体、每个变体的候选与依据一次折齐。
    let first = {
        let (browse, site) = app.browse_and_site();
        site.catalog
            .work_page(browse.query(), 0, 1)
            .ok()
            .and_then(|rows| rows.into_iter().next())
            .map(|row| row.anchor)
    };
    let started = Instant::now();
    if let Some(anchor) = &first {
        let (browse, site) = app.browse_and_site();
        browse.open_work(&site.catalog, anchor);
    }
    let detail_ms = started.elapsed().as_secs_f64() * 1000.0;

    // 四、**全选**，再把作用范围展开成一串变体的键——批量操作按下去要动的就是这一批。
    {
        let (browse, _) = app.browse_and_site();
        browse.picked_mut().select_all();
    }
    let started = Instant::now();
    let variants = {
        let (browse, site) = app.browse_and_site();
        browse
            .batch_variants(&site.catalog)
            .map_or(0, |keys| keys.len() as u64)
    };
    let scope_ms = started.elapsed().as_secs_f64() * 1000.0;
    {
        let (browse, _) = app.browse_and_site();
        browse.picked_mut().clear();
    }

    // 五、滚一趟：表格是虚拟化的，代价该与总行数无关。
    let travel = (filtered as f32 * row_pitch() - VIEWPORT[1]).max(0.0);
    let before = app.window().reads();
    let mut costs: Vec<f64> = Vec::with_capacity(frames as usize);
    for frame in 0..frames {
        let at = if frames > 1 {
            travel * frame as f32 / (frames - 1) as f32
        } else {
            0.0
        };
        app.browse_and_site().0.scroll_to = Some(at);
        let started = Instant::now();
        let output = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
        let _ = ctx.tessellate(output.shapes, output.pixels_per_point);
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        // 前两帧是热身：字体图集与列宽都在这两帧里定下来。
        if frame >= 2 {
            costs.push(elapsed);
        }
    }
    app.browse_and_site().0.scroll_to = None;
    costs.sort_by(f64::total_cmp);

    BrowseCost {
        rows,
        variants,
        scope_ms,
        facets_ms,
        facet_counts: counts,
        filter_ms,
        filter_label: biggest
            .map(|value| format!("平台={value}"))
            .unwrap_or_default(),
        filtered,
        detail_ms,
        median_ms: costs.get(costs.len() / 2).copied().unwrap_or(0.0),
        worst_ms: costs.last().copied().unwrap_or(0.0),
        frames: u32::try_from(costs.len()).unwrap_or(u32::MAX),
        reads: app.window().reads() - before,
        retained: app.window().retained(),
    }
}

/// **子库**那一屏量出来的响应。
#[derive(Debug, Clone, PartialEq)]
pub struct SubCost {
    /// 选择集选出多少个变体。
    pub picked: u64,
    /// 选出来一共多少字节。
    pub bytes: u64,
    /// **排一次差量预览**要多久：折事实、求值、折期望状态、看一遍目标、三方对比。
    pub prepare_ms: f64,
    /// 这份计划要动几个文件。
    pub steps: u64,
    /// 新增几个、多少字节。
    pub adds: (u64, u64),
    /// 超出容量上限多少字节。
    pub over_capacity: Option<u64>,
    /// 给了几条裁剪建议。
    pub trims: usize,
    /// 摊开那张**步骤表**滚一趟，每帧最多真的画了几行。
    ///
    /// **这是「翻行的代价与总步数无关」那句话的量具**，不是秒表：`steps` 从几百涨到
    /// 上万，这个数一动不动（视口就那么高）。挂钟在门禁上是一张彩票，这个数机器忙不忙
    /// 一个字都不影响（票 `parking-3/17` 给主列表立的也是这条判据）。
    pub steps_rows: usize,
    /// 滚那一趟每帧的中位数，毫秒。
    pub steps_median_ms: f64,
    /// 滚那一趟最慢的一帧，毫秒。
    pub steps_worst_ms: f64,
    /// 滚那一趟量了几帧。
    pub steps_frames: u32,
}

/// 摊开步骤表之后滚几帧。头两帧是热身（字体图集与列宽在那两帧里定下来），所以取的
/// 帧数得比热身多得多。
const STEP_FRAMES: u32 = 60;

impl SubCost {
    /// 排成给人看的几行。
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = format!(
            "子库：选择集选出 {} 个变体、{}\n\
             排一次差量预览  {:.0} ms\n\
             这份差量        动 {} 个文件；新增 {} 个、{}\n",
            thousands(self.picked),
            romcat_core::report::human_bytes(self.bytes),
            self.prepare_ms,
            thousands(self.steps),
            thousands(self.adds.0),
            romcat_core::report::human_bytes(self.adds.1),
        );
        out.push_str(&format!(
            "摊开步骤表滚一趟  每帧画 {} 行（与总步数无关）；\
             中位 {:.2} ms，最慢 {:.2} ms，共 {} 帧\n",
            self.steps_rows, self.steps_median_ms, self.steps_worst_ms, self.steps_frames,
        ));
        match self.over_capacity {
            Some(over) => out.push_str(&format!(
                "容量            超出 {}，给了 {} 条裁剪建议（**绝不自动截断**）\n",
                romcat_core::report::human_bytes(over),
                self.trims,
            )),
            None => out.push_str("容量            没超\n"),
        }
        out
    }
}

/// 量一遍**子库**那一屏：建一个、写一条规则、排一次差量预览。
///
/// `target` 是目标设备的**本地 fixture 目录**——绝不去动任何真实设备或 SD 卡。
#[must_use]
pub fn sublibrary(
    app: &mut App,
    name: &str,
    target: &std::path::Path,
    capacity: Option<u64>,
    rule: &str,
) -> SubCost {
    app.show_view(crate::app::View::Sublibraries);
    {
        let (screen, site) = app.sublibrary_and_site();
        {
            let form = screen.form_mut();
            form.name = name.to_string();
            form.target = romcat_core::path::display(target);
            form.capacity = capacity
                .map(romcat_core::report::human_bytes)
                .unwrap_or_default();
        }
        screen.save(site);
    }
    // **规则不在子库屏上写了**（票 `gui-redesign/11`：那一屏只管「送到哪」）。
    // 量的是排差量预览那一趟，规则只是它的前提，所以直接摆进库里——走一遍浏览屏的
    // 筛选器再「存成子库」，量出来的是筛选器的代价，不是这一屏的。
    {
        let (_, site) = app.sublibrary_and_site();
        // **摆不进去就说出口**：吞掉的话这一趟量的是「一条规则都没有」的子库，
        // 印出来是「选出 0 个变体、0 步」——一个看着像结果的假数。
        match romcat_core::sublibrary::Rule::parse(rule) {
            Ok(parsed) => {
                if let Err(error) = site.catalog.add_rule(name, &parsed) {
                    eprintln!("规则写不进中立库：{error}");
                }
            }
            Err(error) => eprintln!("这条规则读不懂：{error}"),
        }
    }
    {
        let (screen, site) = app.sublibrary_and_site();
        screen.reload(site);
        screen.open(site, name);
    }
    {
        let (screen, site, tasks) = app.sublibrary_site_and_tasks();
        screen.preview(site, tasks);
    }
    // 排差量预览跑在**任务台**上（票 01）：主线程这边得一直问「跑完没有」。
    // 量的是那一趟活本身花了多久（[`crate::task`] 记的耗时），不是这个循环的开销。
    for _ in 0..6_000 {
        app.poll_tasks();
        if app.sublibrary().prepared().is_some() || app.sublibrary().error().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let prepare_ms = app.sublibrary().prepare_ms();
    let Some(prepared) = app.sublibrary().prepared().cloned() else {
        return SubCost {
            picked: 0,
            bytes: 0,
            prepare_ms,
            steps: 0,
            adds: (0, 0),
            over_capacity: None,
            trims: 0,
            steps_rows: 0,
            steps_median_ms: 0.0,
            steps_worst_ms: 0.0,
            steps_frames: 0,
        };
    };

    // **摊开那张步骤表滚一趟**：计划整份在界面状态里，而一帧画几行只跟视口有多高有关
    // （票 `parking-3/08` 去掉了那个 2,000 条的上限）。量的正是这一条——每帧真的画了
    // 几行，连带每帧的 CPU 代价。
    //
    // **这一趟单独画那张表，不走 [`App::ui`]**，与这个模块别处的规矩不一样，理由是
    // 真机量级上量出来的会是别的东西：一份两万步的计划，卡头、容量条、四行汇总与
    // 「要说出口的怪事」那几段加起来就比一屏高，于是那张表**整个落在视口之外、一行都
    // 不画**（实测：300 个变体那份画 5 行，默认那份画 0 行）。那时「每帧多少毫秒」量的
    // 是这一屏别的东西多贵，而这一条要量的是**翻行本身**。屏上那张表画的是同一个
    // [`steps_table`](crate::sublibrary::steps_table)，一个字都不另写。
    let ctx = headless::context();
    let plan = &prepared.plan;
    let travel = (plan.steps.len() as f32 * row_pitch() - VIEWPORT[1]).max(0.0);
    let mut steps_rows = 0;
    let mut costs: Vec<f64> = Vec::with_capacity(STEP_FRAMES as usize);
    for frame in 0..STEP_FRAMES {
        let at = if STEP_FRAMES > 1 {
            travel * frame as f32 / (STEP_FRAMES - 1) as f32
        } else {
            0.0
        };
        let mut drawn = 0;
        let started = Instant::now();
        let output = headless::frame(&ctx, headless::input(), |ui| {
            drawn = crate::sublibrary::steps_table(ui, plan, Some(at));
        });
        let _ = ctx.tessellate(output.shapes, output.pixels_per_point);
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        // 前两帧是热身：字体图集与列宽都在这两帧里定下来。
        if frame >= 2 {
            costs.push(elapsed);
            steps_rows = steps_rows.max(drawn);
        }
    }
    costs.sort_by(f64::total_cmp);

    SubCost {
        picked: prepared.selected.picked.len() as u64,
        bytes: prepared.selected.bytes,
        prepare_ms,
        steps: prepared.plan.touched(),
        adds: (prepared.plan.adds.files, prepared.plan.adds.bytes),
        over_capacity: prepared.plan.over_capacity,
        trims: prepared.plan.trim_suggestions.len(),
        steps_rows,
        steps_median_ms: costs.get(costs.len() / 2).copied().unwrap_or(0.0),
        steps_worst_ms: costs.last().copied().unwrap_or(0.0),
        steps_frames: u32::try_from(costs.len()).unwrap_or(u32::MAX),
    }
}

/// **主列表翻页**量出来的代价，毫秒。
///
/// 量的不是「一帧多少毫秒」——那一半 [`scroll`] 与 [`browse`] 已经量过了。
/// 这一条量的是**核心库那两条查询本身**：`work_total`（滚动条的长度）与
/// `work_page`（屏上那一窗 512 行）。两件事分开量，才分得清慢的是
/// `GROUP BY`、`ORDER BY`，还是搜索那三条 `OR`（票 `gui-redesign/13`）。
///
/// **一个字节都不写库。**
#[derive(Debug, Clone, PartialEq)]
pub struct PagingCost {
    /// 库里一共多少个变体。
    pub variants: u64,
    /// 收敛成多少行。
    pub rows: u64,
    /// 一窗几行。
    pub page: u64,
    /// 各量了一趟。
    pub probes: Vec<PagingProbe>,
}

/// 一趟翻页的量法与量出来的数。
#[derive(Debug, Clone, PartialEq)]
pub struct PagingProbe {
    /// 这一趟量的是什么（搜没搜、按哪一列排）。
    pub label: String,
    /// 这个筛选下剩多少行。
    pub matched: u64,
    /// **数一次总行数**要多久（`work_total`）。
    pub total_ms: f64,
    /// **取第一页**要多久（`work_page`，`OFFSET 0`）。
    pub first_ms: f64,
    /// **翻到最后一页**要多久（`OFFSET` 顶到底）。**滚动条一拖到底就是它。**
    pub last_ms: f64,
}

impl PagingCost {
    /// 排成给人看的几行。
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = format!(
            "主列表翻页：{} 个变体收敛成 {} 行，一窗 {} 行\n\
             {:<30}{:>10}{:>12}{:>12}{:>12}\n",
            thousands(self.variants),
            thousands(self.rows),
            self.page,
            "量的是什么",
            "命中行数",
            "数总行数",
            "第一页",
            "最后一页",
        );
        for probe in &self.probes {
            out.push_str(&format!(
                "{:<30}{:>10}{:>9.1} ms{:>9.1} ms{:>9.1} ms\n",
                probe.label,
                thousands(probe.matched),
                probe.total_ms,
                probe.first_ms,
                probe.last_ms,
            ));
        }
        out
    }
}

/// 一窗几行：与界面上那扇窗**同一个数**（[`SPAN`]）。这张票只治时间，
/// 内存那一半一点不许退。
const PAGING_WINDOW: u64 = SPAN;

/// 同一条查询跑几趟取中位数。头一趟连页缓存与临时表一起热身，不计。
const PAGING_RUNS: usize = 5;

/// 跑几趟取中位数。
fn median_ms(mut runs: impl FnMut()) -> f64 {
    let mut costs: Vec<f64> = Vec::with_capacity(PAGING_RUNS);
    for at in 0..=PAGING_RUNS {
        let started = Instant::now();
        runs();
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        if at > 0 {
            costs.push(elapsed);
        }
    }
    costs.sort_by(f64::total_cmp);
    costs[costs.len() / 2]
}

/// 量一趟：数总行数、取第一页、翻到最后一页。
fn probe(catalog: &Catalog, label: &str, query: &WorkQuery) -> PagingProbe {
    let matched = catalog.work_total(query).unwrap_or(0);
    let last = matched.saturating_sub(PAGING_WINDOW);
    PagingProbe {
        label: label.to_string(),
        matched,
        total_ms: median_ms(|| {
            let _ = catalog.work_total(query);
        }),
        first_ms: median_ms(|| {
            let _ = catalog.work_page(query, 0, PAGING_WINDOW);
        }),
        last_ms: median_ms(|| {
            let _ = catalog.work_page(query, last, PAGING_WINDOW);
        }),
    }
}

/// 量一遍**主列表翻页**：三种排法、三种搜法、外加筛着的那两趟，各量一趟。
///
/// 搜索那三个词各挑一条命中路（名字 / 别名 / 一条都不命中），因为它们在
/// `WHERE` 里是三条 `OR` 且**短路求值**：名字就命中的行走不到后两条，
/// 一条都不命中的那一行三条全跑满——那是这条代价的上界（挂单 Q108）。
///
/// **筛着的那两趟不是凑数**：搜索那三条改成非相关子查询之后，扫表那笔开销是
/// **固定的**，与筛剩几行无关——筛得很窄时它就显出来（挂单 Q155）。
/// 那一格在这张表里常驻，以后谁再动搜索那一层，它会自己说话。
#[must_use]
pub fn paging(catalog: &Catalog) -> PagingCost {
    let variants = catalog.variant_total(&VariantQuery::default()).unwrap_or(0);
    let rows = catalog.work_total(&WorkQuery::default()).unwrap_or(0);
    let mut probes = Vec::new();
    for (label, order) in [
        ("不搜、按作品名（默认）", WorkOrder::Name),
        ("不搜、按容量", WorkOrder::Bytes),
        ("不搜、按年份", WorkOrder::Year),
    ] {
        probes.push(probe(
            catalog,
            label,
            &WorkQuery {
                order,
                ..WorkQuery::default()
            },
        ));
    }
    for (label, needle) in [
        ("搜「幻想」（名字命中）", "幻想"),
        ("搜「Sakuhin 1」（别名命中）", "Sakuhin 1"),
        ("搜「外星人」（一条不中）", "外星人"),
    ] {
        probes.push(probe(
            catalog,
            label,
            &WorkQuery {
                search: needle.to_string(),
                ..WorkQuery::default()
            },
        ));
    }
    // **筛着的时候也量一趟**：筛选把行收窄之后，`WHERE` 上那一维自己有索引
    // （`variant_platform_key`），而分组那条又有 `variant_group`——两条索引摆在一起，
    // SQLite 挑哪一条不是想当然的事，得量出来（票 `gui-redesign/13`）。
    // 挑**最大的那个平台**：量的该是「筛完还剩不少」那种，不是最小的那种。
    if let Some(platform) = catalog
        .facets()
        .ok()
        .and_then(|facets| facets.platforms.first().map(|facet| facet.value.clone()))
    {
        let picked = Some(PlatformFilter::from_label(&platform));
        probes.push(probe(
            catalog,
            &format!("按平台筛（{platform}）"),
            &WorkQuery {
                platform: picked.clone(),
                ..WorkQuery::default()
            },
        ));
        probes.push(probe(
            catalog,
            &format!("按平台筛（{platform}）+ 搜"),
            &WorkQuery {
                platform: picked,
                search: "幻想".to_string(),
                ..WorkQuery::default()
            },
        ));
    }
    PagingCost {
        variants,
        rows,
        page: PAGING_WINDOW,
        probes,
    }
}
