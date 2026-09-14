//! **任务**那一屏：排队、进度、可停、历史。**它不发起操作，只承接。**
//!
//! ## 这一屏为什么是基础设施而不是一屏
//!
//! 核心库那几个长入口一趟要跑很久——扫一遍真库 37.1 分钟、识别 65.5 秒、
//! 排一次**差量预览**在真机量级上 343 毫秒。它们跑在画帧那条线程上的话，窗口就是一块
//! 白板：期间切不了屏、滚不动列表、连「停下」都点不着。所以它们统统搬到画帧线程之外，
//! 这一屏是那件事在界面上的落点。
//!
//! **眼下接上来的是排差量预览、算一遍容量、同步、扫描、取数据源、刮削、识别与折标题**。
//! **导出**在它自己那张票里接——接的办法与这里一模一样：核心那一侧收一个
//! [`Handle`](romcat_core::task::Handle)，界面这一侧往 [`Board`] 上排一趟。
//!
//! ## 领域判断一条都不在这里
//!
//! 排队怎么排、进度怎么算、被按停了算「停了」还是算「失败」——全在
//! [`romcat_core::task`]。这一层只做两件事：把任务台上的账画出来，把「停下」那一下
//! 转发回去。

use std::collections::BTreeMap;
use std::time::Duration;

use romcat_core::collection;
use romcat_core::report::{human_duration, human_time};
use romcat_core::scan::ScanOutcome;
use romcat_core::scrape::Outcome as ScrapeOutcome;
use romcat_core::sources::SourceStatus;
use romcat_core::sublibrary::report::SelectionReport;
use romcat_core::sync::{Outcome as SyncOutcome, Prepared};
use romcat_core::task::{Board, Ending, Live, Record};

use crate::tokens::Tokens;
use crate::{font, look};

/// 一趟任务跑完之后交出来的东西。
///
/// **导出接上来时往这里再加一支**（票 `gui-self-sufficient/08`）——这个枚举就是
/// 「任务台上会跑哪几种活」的清单。
///
/// ## 要写库的那几支，写在**认领**那一步
///
/// 台上那条线拿的是中立库的**只读**连接（`Catalog::read_only`），写不动。所以
/// [`Synced`](Self::Synced) 那份**清单**是当作产物交回来、由认领它的那一屏落库的
/// （`sublibrary::Screen::settle`）。台上那条线自己写的话，两份连接会在同一个 SQLite
/// 文件上撞车。
#[derive(Debug)]
pub enum Product {
    /// 一份排好的**差量预览**。
    ///
    /// 装箱是因为它很大（选择集、期望状态、清单、目标状态、计划全在里头），
    /// 而这个枚举将来还要长出别的支。
    Preview(Box<Prepared>),
    /// 扫完了一个**根**。装箱同上：一趟扫描的产物里带着整份体检统计。
    Scanned(Box<ScanOutcome>),
    /// 取回了一个**数据源**。
    Fetched(SourceStatus),
    /// 跑完了一趟**刮削**。装箱同上：一趟刮削的产物里带着整份报告。
    Scraped(Box<ScrapeOutcome>),
    /// 跑完了一趟**同步**。装箱同上：这份账里带着同步完之后的整份**清单**。
    ///
    /// **被按停的那一趟也会走到这儿**（`Outcome::interrupted` 记着），而且**必须**走到
    /// ——那份清单记的是「到中断为止目标上真实有什么」，认领时落回中立库，下一趟才接得上。
    Synced(Box<SyncOutcome>),
    /// 算了一遍**每台设备的容量**：一台一份[选择集报告](SelectionReport)，按子库名。
    Evaluated(Box<BTreeMap<String, SelectionReport>>),
    /// 跑完了一趟**识别**。装箱同上：这份账里带着整份命中率报告。
    ///
    /// **被按停的那一趟也会走到这儿**（`Outcome::interrupted` 记着），而且**必须**
    /// 走到——识别起手就把上一轮的结论清干净，已经算出来的那些真的落进了中立库，
    /// 库屏工序段那一行的数字要照它刷新。**它没有断点**：下一趟从头再算一遍
    /// （`romcat_core::identify::run_task` 报的那句「停在半路」说的就是这件事）。
    Identified(Box<romcat_core::identify::Outcome>),
    /// 跑完了一趟**折标题**：把识别与刮削的结论折成每个作品的**标题集合**。
    /// 装箱同上——这份报告里带着按语言、按类型、按来源的整份账。
    ///
    /// **它不走「写在认领那一步」那条**：重折是「把折出来的那批清掉再写回去」
    /// （`romcat_core::title::refold`），一路写批。台上那条线自己开一份写得动的库，
    /// 与扫描、刮削、识别同一条路（见本模块开头那段与 `crate::stages`）。
    ///
    /// **没有「停在半路」这一档**：重折在清空与写回之间一步都不停
    /// （停在那儿等于把整份标题集合丢掉），所以被按停的那一趟一个字节都没写
    /// ——那是[停了](romcat_core::task::Ending::Stopped)，不是半路。
    Titled(Box<romcat_core::title::TitleReport>),
    /// 跑完了一趟**导出**：把中立库写成前端能读的元数据，铺在导出目录里。
    /// 装箱同上——这份报告里带着逐份文件的账、被挡下的变体与实测**能力档位**。
    ///
    /// **它不走「写在认领那一步」那条**：一趟导出一份文件一份文件地写，每写完一份就把
    /// **底本**存进中立库（那是下一趟「外面有人动过没有」的唯一判据）。台上那条线
    /// 自己开一份写得动的库，与扫描、刮削、识别、折标题同一条路（见本模块开头那段与
    /// `crate::stages`）。
    ///
    /// **它有「停在半路」这一档**（与折标题正相反）：按停时已经写出去的那几份连底本
    /// 一起真的落了盘与库，下一趟拿它们当基线接着比
    /// ——`romcat_core::adapter::transfer::export_task` 报的那句「停在半路」说的就是
    /// 这件事。
    Exported(Box<romcat_core::adapter::report::ExportReport>),
    /// 排好了一趟**整批收藏**（或者自建合集的加减）：这一批各该钉在哪种锚上。
    ///
    /// **它要写两份库**（沉淀库那些成员关系、中立库那份投影），所以与
    /// [`Synced`](Self::Synced) 同一条：产物交回来，由认领它的那一屏落库
    /// （`browse::Screen::settle_collection`）。装箱是因为它带着整批的锚——
    /// 真库上「全选 46,483 行 → ★ 收藏」那一下就是四万多条。
    Planned(Box<collection::Plan>),
    /// 算了一遍**开着铺媒体时这一趟导出最多要铺多少**：几份、共多少字节
    /// （库屏工序段导出那一支，票 `one-criterion-per-thing/09`）。
    ///
    /// **是上界**（`romcat_core::adapter::transfer::media_to_lay`）：落点上已经有的真铺时
    /// 不重铺。只留两个数、不留整份落点表：屏上要画的只有这两个，而整库的落点表真库上
    /// 是几万条。整条只读，所以它与 [`Evaluated`](Self::Evaluated) 一样不必写在认领那一步。
    MediaCounted {
        /// 几份。
        files: u64,
        /// 共多少字节。
        bytes: u64,
    },
}

/// 这个界面上那张**任务台**。
pub type Tasks = Board<Product>;

/// 任务那一屏，连同每一屏底下那条状态栏里任务那一截。
///
/// **它自己只记一样东西：这一帧从任务台取的那一份快照**——要画的全在任务台上。
#[derive(Debug, Default)]
pub struct Screen {
    /// 这一帧的任务台快照（[`Board::running`]），连同是哪一帧取的。
    ///
    /// **状态栏与任务屏那张卡读的是同一份**：快照里的已用时间是取的那一刻现量的，同一帧里
    /// 问两次任务台，状态栏与卡上就会印出两个不一样的「约剩」。状态栏先画，任务屏后画，
    /// 后画的那一处照帧号认出这一份已经取过了。
    snapshot: Option<(u64, Option<Live>)>,
}

impl Screen {
    /// 开一个。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 这一帧正在跑的那一趟：这一帧头一回问时从任务台取，之后同一帧里都给这同一份。
    fn running(&mut self, ctx: &egui::Context, tasks: &Tasks) -> Option<Live> {
        let pass = ctx.cumulative_pass_nr();
        if let Some((taken, live)) = &self.snapshot
            && *taken == pass
        {
            return live.clone();
        }
        let live = tasks.running();
        self.snapshot = Some((pass, live.clone()));
        live
    }

    /// 顶栏上属于这一屏的那一段。
    pub fn status(&mut self, ui: &mut egui::Ui, tasks: &Tasks) {
        ui.label(summary(tasks));
    }

    /// **底部状态栏**（设计稿 `.statusbar`）：每一屏底下都有这一条。左边是任务台那一小截，
    /// 右边是「ROM 只读」与工作目录。返回「点了任务那一句没有」——点了就去任务屏。
    ///
    /// **任务那一小截与任务屏那张卡是同一件事**：读的是同一帧里同一份任务台快照
    /// （`Screen::snapshot`），百分比、约剩多少走的是同一个排法（`percent`、`elapsed`）。
    /// 说不出走了几成就不印百分比、算不出约剩就一个字都不画，与卡上同一个口径。
    pub fn status_bar(&mut self, ui: &mut egui::Ui, tasks: &Tasks, workspace: &str) -> bool {
        let mut go = false;
        let running = self.running(ui.ctx(), tasks);
        ui.horizontal_centered(|ui| {
            match &running {
                None => {
                    ui.label(egui::RichText::new("任务台空闲").small().weak());
                }
                Some(live) => {
                    let line = egui::Label::new(egui::RichText::new(status_line(live)).small())
                        .sense(egui::Sense::click());
                    go = ui.add(line).on_hover_text("去任务屏").clicked();
                    // 设计稿那条小进度条：宽取令牌 `statusbar-bar`，高取间距最窄那一档。
                    ui.add(
                        progress_bar(live.progress.fraction())
                            .desired_width(Tokens::builtin().layout.statusbar_bar)
                            .desired_height(step(0)),
                    );
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(font::mono(workspace).small().weak());
                ui.label(egui::RichText::new("ROM 只读").small().weak());
            });
        });
        go
    }

    /// 画一帧。
    pub fn ui(&mut self, ui: &mut egui::Ui, tasks: &mut Tasks) {
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("任务屏")
                .show(ui, |ui| self.body(ui, tasks));
        });
    }

    fn body(&mut self, ui: &mut egui::Ui, tasks: &mut Tasks) {
        ui.horizontal(|ui| {
            ui.heading("任务");
            ui.weak("查看正在运行、等待中的任务和历史");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!tasks.history().is_empty(), egui::Button::new("清空历史"))
                    .clicked()
                {
                    tasks.clear_history();
                }
            });
        });
        section(ui, "正在运行");
        let mut stop = None;
        let running = self.running(ui.ctx(), tasks);
        card(ui, running.is_some(), |ui| match &running {
            // **空台也摆一张卡**：与跑着时同一个位置、同一个框，人分得清「空着」与「没画出来」。
            None => {
                ui.label("眼下没有任务在跑。");
                ui.weak("在「库」屏点一道工序的按钮，那一趟就排到这里。");
            }
            Some(live) => {
                if running_ui(ui, live) {
                    stop = Some(live.id);
                }
            }
        });

        section(ui, "等待中");
        let queued = tasks.queued();
        if queued.is_empty() {
            ui.weak("没有等待中的任务。");
        }
        for (at, (id, name)) in queued.into_iter().enumerate() {
            card(ui, false, |ui| {
                ui.horizontal(|ui| {
                    // 排第几：从 1 数的位次，不是任务号——人问的是「前面还有几趟」。
                    ui.label(font::mono((at + 1).to_string()).weak());
                    ui.label(font::strong(&name));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // **撤掉排着的这一趟**走任务台现成的入口（`Board::stop`）：还没开跑的
                        // 直接撤下，照任务台原有的账在历史里记一条已取消、交回排它的那一屏
                        // ——不然那一屏会一直记着「我还有一趟在排」。正在跑的那一趟不受影响。
                        if ui.button("移除").clicked() {
                            stop = Some(id);
                        }
                    });
                });
            });
        }
        if let Some(id) = stop {
            tasks.stop(id);
        }

        section(ui, "历史");
        if tasks.history().is_empty() {
            // 不说「已完成」：历史里装着四档收场，「完成」只是其中一档。
            ui.weak("历史里还没有任务。");
        } else {
            card(ui, false, |ui| history_table(ui, tasks.history()));
        }

        // **一次只跑一趟**（词表**任务台**）：说出口，人才不会以为排着的那几趟卡住了。
        ui.add_space(step(3));
        ui.weak(
            "任务依次运行，一次只跑一趟：它们读写同一份中立库、同一块盘，同时跑只会互相抢。\
             任务跑着的时候，别的屏照常用。",
        );
    }
}

/// 历史那张表，四列：任务、收场、耗时、时间。
///
/// 第二列设计稿写的是「结果」，那是词表**收场**那一条的避用词（挂单 `Q832`）。
///
/// **后三列量出多宽就多宽，剩下的全给「任务」那一列**：名字底下那半句说明（留下了什么、
/// 下次从哪儿接着来）可能很长，得在这一列里折行。不先定宽的话，格子按上一帧量出的列宽折，
/// 头一帧列窄，中文一个字一行，一行历史能长到几百点高，后面几行直接掉出屏去。
fn history_table(ui: &mut egui::Ui, history: &[Record]) {
    let [pad_y, pad_x] = Tokens::builtin().space.cell_padding;
    // 两列之间的缝：左边那格的右内边距加右边那格的左内边距。
    let gap = 2.0 * pad_x;
    let [task_title, ending_title, took_title, at_title] = ["任务", "收场", "耗时", "时间"];
    let small = egui::TextStyle::Small;
    let body = egui::TextStyle::Body;
    // 收场那一格是一枚记号：圆点、一档间距、词，两侧各一档内边距（见 [`ending_chip`]）。
    let chip_extra = ui.text_style_height(&small) / 2.0 + step(0) + 2.0 * step(1);
    let chip = history
        .iter()
        .map(|record| text_width(ui, record.ending.word(), &small) + chip_extra)
        .fold(text_width(ui, ending_title, &small), f32::max);
    let took = history
        .iter()
        .map(|record| text_width(ui, &elapsed(record.elapsed), &body))
        .fold(text_width(ui, took_title, &small), f32::max);
    let at = history
        .iter()
        .map(|record| text_width(ui, &human_time(record.ended_at), &body))
        .fold(text_width(ui, at_title, &small), f32::max);
    let name = (ui.available_width() - chip - took - at - 3.0 * gap)
        .max(text_width(ui, task_title, &small));

    egui::Grid::new("任务历史")
        .num_columns(4)
        .spacing([gap, pad_y])
        .striped(true)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_width(name);
                ui.label(egui::RichText::new(task_title).small().weak());
            });
            for title in [ending_title, took_title, at_title] {
                ui.label(egui::RichText::new(title).small().weak());
            }
            ui.end_row();
            for record in history {
                history_row(ui, record, name);
            }
        });
}

/// 这段字不折行时多宽。
fn text_width(ui: &egui::Ui, text: &str, style: &egui::TextStyle) -> f32 {
    egui::WidgetText::from(text)
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            style.clone(),
        )
        .size()
        .x
}

/// 间距那几档里的第几档（`tokens.toml` 的 `space.steps`，从窄到宽）。
fn step(at: usize) -> f32 {
    Tokens::builtin()
        .space
        .steps
        .get(at)
        .copied()
        .unwrap_or_default()
}

/// 一块的小标题：设计稿 `.sec`，说明文字那一档字号、弱字色。上面空出一档间距隔开上一块。
fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(step(3));
    ui.label(egui::RichText::new(title).small().weak());
}

/// 一张卡：设计稿 `.card`——面板底、分隔线那一档描边、大圆角。颜色、描边与圆角取 `Visuals`
/// （[`crate::look::install`] 从令牌装上去的那几格），内边距取间距那几档。
///
/// `accent` 是设计稿 `.runcard` 那一张：描边换成强调色，正在跑的那一趟一眼认得出。
fn card<R>(ui: &mut egui::Ui, accent: bool, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let visuals = ui.visuals();
    let mut stroke = visuals.widgets.noninteractive.bg_stroke;
    if accent {
        stroke.color = visuals.selection.stroke.color;
    }
    egui::Frame::new()
        .fill(visuals.window_fill)
        .stroke(stroke)
        .corner_radius(visuals.window_corner_radius)
        .inner_margin(egui::Margin::from(egui::vec2(step(3), step(3))))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// 一条进度条，卡上与状态栏上走的是这同一个。
///
/// **说不出走了几成时画一条动着的条**，不是一条停在 0% 的条——后者看着像卡住了。
fn progress_bar(fraction: Option<f32>) -> egui::ProgressBar {
    match fraction {
        None => egui::ProgressBar::new(0.0).animate(true),
        Some(fraction) => egui::ProgressBar::new(fraction),
    }
}

/// 走了几成排成百分比，四舍五入到整数，如 `38%`。卡上与状态栏上走的是这同一个。
fn percent(fraction: f32) -> String {
    format!("{:.0}%", (fraction * 100.0).round())
}

/// 状态栏上任务那一句：「名字 · 38% · 约剩 3 分 12 秒」。说不出的那几段不画。
fn status_line(live: &Live) -> String {
    let mut line = live.name.clone();
    if let Some(fraction) = live.progress.fraction() {
        line.push_str(&format!(" · {}", percent(fraction)));
    }
    if let Some(left) = live.remaining() {
        line.push_str(&format!(" · 约剩 {}", elapsed(left)));
    }
    line
}

/// 「1 个进行中 · 3 条历史」那一句。
fn summary(tasks: &Tasks) -> String {
    let running = usize::from(tasks.running().is_some());
    let queued = tasks.queued().len();
    let mut line = format!("{running} 个进行中");
    if queued > 0 {
        line.push_str(&format!(" · {queued} 个排着队"));
    }
    line.push_str(&format!(" · {} 条历史", tasks.history().len()));
    line
}

/// 正在跑的那一趟：设计稿 `.runcard`——名字与「停下」一排，一条进度条，底下一排
/// 进度、已用、约剩、在做什么。返回「按了停下没有」。
fn running_ui(ui: &mut egui::Ui, live: &Live) -> bool {
    let mut stopped = false;
    ui.horizontal(|ui| {
        // 字号直接取令牌 `size-title`，**不走具名字号那一档**（`look::TITLE`）：开窗头一帧
        // 观感基线才装上去，而那一帧手上这个 `Ui` 还带着装之前的样式——台上头一帧就有活在跑时
        // （开场走完向导直接开扫就是这样），按名字找那一档会当场 panic。
        ui.label(font::strong(&live.name).size(Tokens::builtin().font.size_title));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if live.stopping {
                // 按下停下到真的停之间隔着一步——**如实说出来**，不然人会以为按钮没反应。
                ui.colored_label(ui.visuals().warn_fg_color, "正在停……走到下一步就停");
            } else if ui
                .button("停下")
                .on_hover_text("停在两步之间：不留半截状态。")
                .clicked()
            {
                stopped = true;
            }
        });
    });
    let fraction = live.progress.fraction();
    ui.add(progress_bar(fraction).desired_height(step(1)));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = step(3);
        // 说不出走了几成就不印百分比——与那条动着的条同一个口径。
        if let Some(fraction) = fraction {
            ui.weak(format!("进度 {}", percent(fraction)));
        }
        ui.weak(format!("已用 {}", elapsed(live.elapsed)));
        // **算不出来就一格都不画**：核心那一侧交出「没有」的时候（总步数还没报、
        // 或者走了零成），这儿连个占位都不摆。一个会跳的「约剩」比没有「约剩」更坏
        // ——维护者会照它安排接下来一小时干什么。折算本身在
        // [`Live::remaining`](romcat_core::task::Live::remaining)：这一层只画。
        if let Some(left) = live.remaining() {
            ui.weak(format!("约剩 {}", elapsed(left)));
        }
        // 在做什么：第几步、叫什么、这一步里走了几件——那一句由核心库折（`Progress::render`）。
        ui.weak(live.progress.render());
    });
    stopped
}

/// 历史里的一行，四列：任务（名字，底下跟那半句说明，在 `name` 那么宽里折行）、收场、
/// 耗时、时间。
fn history_row(ui: &mut egui::Ui, record: &Record, name: f32) {
    ui.vertical(|ui| {
        ui.set_width(name);
        ui.label(font::strong(&record.name));
        // **说明整句由核心库说**（[`Ending::detail`]）：部分完成留下了什么、下次从哪儿接着来，
        // 失败停在哪一步、为什么。这一层一个字都不添。
        if let Some(detail) = record.ending.detail() {
            ui.label(egui::RichText::new(detail).small().weak());
        }
    });
    ending_chip(ui, &record.ending);
    ui.label(elapsed(record.elapsed));
    // 时刻走 [`human_time`]（UTC）：库屏「上次扫描」、开场那一行用的是同一个。
    ui.weak(human_time(record.ended_at));
    ui.end_row();
}

/// 「收场」那一格：设计稿 `.chip`——那一档的浅底，同色的圆点与词。
///
/// **失败与已取消不许长得跟完成一样**：那句「跑了 X 秒」就成了骗人的话。**颜色又不是唯一
/// 线索**：词本身就在格子里（[`Ending::word`]，逐字是词表那四档）。四档配哪一色由
/// [`look::ending_colors`] 回答，这里只画。
fn ending_chip(ui: &mut egui::Ui, ending: &Ending<()>) {
    let (ink, soft) = look::ending_colors(ending, ui.visuals());
    egui::Frame::new()
        .fill(soft)
        .corner_radius(egui::CornerRadius::same(Tokens::builtin().radius.small))
        .inner_margin(egui::Margin::from(egui::vec2(step(1), 0.0)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = step(0);
                // 圆点直径取说明文字那一档字高的一半（设计稿 12 号字配 6 点的点）。
                let size = ui.text_style_height(&egui::TextStyle::Small) / 2.0;
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), size / 2.0, ink);
                ui.label(egui::RichText::new(ending.word()).small().color(ink));
            });
        });
}

/// 一段时长排成人看得懂的样子。**与报告那一侧同一个算法**
/// （[`human_duration`]），免得同一趟活在两处印出不一样的数。
fn elapsed(elapsed: Duration) -> String {
    human_duration(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
}
