//! **任务**那一屏：排队、进度、可停、历史。**它不发起操作，只承接。**
//!
//! ## 这一屏为什么是基础设施而不是一屏
//!
//! 核心库那几个长入口一趟要跑很久——扫一遍真库 37.1 分钟、识别 65.5 秒、
//! 排一次**差量预览**在真机量级上 343 毫秒。它们跑在画帧那条线程上的话，窗口就是一块
//! 白板：期间切不了屏、滚不动列表、连「停下」都点不着。所以它们统统搬到画帧线程之外，
//! 这一屏是那件事在界面上的落点。
//!
//! **眼下接上来的是排差量预览、扫描、取数据源与刮削**。识别与同步各在各自的票里接
//! ——接的办法与这里一模一样：核心那一侧收一个
//! [`Handle`](romcat_core::task::Handle)，界面这一侧往 [`Board`] 上排一趟。
//!
//! ## 领域判断一条都不在这里
//!
//! 排队怎么排、进度怎么算、被按停了算「停了」还是算「失败」——全在
//! [`romcat_core::task`]。这一层只做两件事：把任务台上的账画出来，把「停下」那一下
//! 转发回去。

use std::time::Duration;

use romcat_core::report::human_duration;
use romcat_core::scan::ScanOutcome;
use romcat_core::scrape::Outcome as ScrapeOutcome;
use romcat_core::sources::SourceStatus;
use romcat_core::sync::Prepared;
use romcat_core::task::{Board, Ending, Live, Record};

/// 一趟任务跑完之后交出来的东西。
///
/// **识别与同步在各自的票里接上来时，各自往这里加一支**——这个枚举就是「任务台上会跑
/// 哪几种活」的清单。
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
}

/// 这个界面上那张**任务台**。
pub type Tasks = Board<Product>;

/// 任务那一屏。
///
/// **它自己一点状态都没有**——要画的全在任务台上。这个类型存在只是为了与另外三屏
/// 一个写法。
#[derive(Debug, Default)]
pub struct Screen;

impl Screen {
    /// 开一个。
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// 顶栏上属于这一屏的那一段。
    pub fn status(&mut self, ui: &mut egui::Ui, tasks: &Tasks) {
        ui.label(summary(tasks));
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
            ui.weak(summary(tasks));
            if ui
                .add_enabled(!tasks.history().is_empty(), egui::Button::new("清空历史"))
                .clicked()
            {
                tasks.clear_history();
            }
        });
        ui.weak("任务跑着的时候，别的屏照常用——浏览、筛选、看详情都不受影响。");
        ui.separator();

        ui.strong("进行中");
        let mut stop = None;
        match tasks.running() {
            None => {
                ui.weak("眼下没有任务在跑。");
            }
            Some(live) => {
                if running_ui(ui, &live) {
                    stop = Some(live.id);
                }
            }
        }
        let queued = tasks.queued();
        if !queued.is_empty() {
            ui.add_space(6.0);
            ui.strong("排着队");
            for (id, name) in queued {
                ui.horizontal(|ui| {
                    ui.label(format!("{name}（第 {id} 号）"));
                    if ui.button("撤掉").clicked() {
                        stop = Some(id);
                    }
                });
            }
        }
        if let Some(id) = stop {
            tasks.stop(id);
        }

        ui.add_space(12.0);
        ui.strong("历史");
        if tasks.history().is_empty() {
            ui.weak("还没有跑完过任何任务。");
            return;
        }
        egui::Grid::new("任务历史")
            .num_columns(3)
            .spacing([16.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                for record in tasks.history() {
                    history_row(ui, record);
                }
            });
    }
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

/// 正在跑的那一趟：名字、进度、已用时间、停下。返回「按了停下没有」。
fn running_ui(ui: &mut egui::Ui, live: &Live) -> bool {
    let mut stopped = false;
    ui.horizontal(|ui| {
        ui.strong(&live.name);
        ui.weak(format!("已用 {}", elapsed(live.elapsed)));
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
    let bar = match live.progress.fraction() {
        // **说不出走了几成时画一条动着的条**，不是一条停在 0% 的条——后者看着像卡住了。
        None => egui::ProgressBar::new(0.0).animate(true),
        Some(fraction) => egui::ProgressBar::new(fraction),
    };
    ui.add(bar.text(live.progress.render()));
    stopped
}

/// 历史里的一行：名字、耗时、怎么收场的。
fn history_row(ui: &mut egui::Ui, record: &Record) {
    ui.label(&record.name);
    ui.label(elapsed(record.elapsed));
    match &record.ending {
        Ending::Done => {
            ui.weak(record.ending.render());
        }
        // **失败与被按停不许长得跟完成一样**：那句「跑了 X 秒」就成了骗人的话。
        Ending::Stopped => {
            ui.colored_label(ui.visuals().warn_fg_color, record.ending.render());
        }
        Ending::Failed { .. } => {
            ui.colored_label(ui.visuals().error_fg_color, record.ending.render());
        }
    }
    ui.end_row();
}

/// 一段时长排成人看得懂的样子。**与报告那一侧同一个算法**
/// （[`human_duration`]），免得同一趟活在两处印出不一样的数。
fn elapsed(elapsed: Duration) -> String {
    human_duration(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
}
