//! 窗口本体：**待确认队列**是打开工具后看见的那一屏，变体表是它旁边的另一屏。
//!
//! ## 为什么默认是队列而不是封面墙
//!
//! ADR-0002 定死了：GUI 的主界面是待确认队列，封面墙是次要视图。库里以**汉化版**为主，
//! 而汉化补丁改了字节，最可靠的那一环（精确哈希）对主力内容结构性失效——于是「人来裁」
//! 不是收尾工作，是这条管线的正文。真机上那是一万六千多条。
//!
//! ## 中文输入放在详情面板里
//!
//! 不放在表格单元格里，而且不是审美选择：表格是虚拟化的，正在组字的那一行一旦滚出视口，
//! 那个控件就不存在了，输入法上屏时会没人接（ADR-0005 的修订段）。详情面板在表格之外，
//! 怎么滚都在。队列那一屏把这条推到底——**连搜索框都在面板里**，见 [`crate::queue`]。
//!
//! ## 退出前先关输入法
//!
//! macOS 上组字过程中直接关窗会 abort（winit#4626），规避办法是**窗口还活着的时候**先
//! `set_ime_allowed(false)`。所以关窗请求来的第一帧不真的关：先撤销关闭、放掉文本焦点、
//! 发一条 `IMEAllowed(false)`，下一帧再关。[`App::closing`] 就是这两拍的状态。

use egui::{Align, Layout};
use romcat_core::catalog::{VariantQuery, VariantRow};
use romcat_core::report::human_bytes;

use crate::table::{SPAN, Table, Window};
use crate::{font, queue};
use romcat_core::site::Site;

/// 关窗走到哪一拍了。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closing {
    /// 还没人要关。
    No,
    /// 关窗请求来了，这一帧撤销掉并把输入法关了，下一帧再真的关。
    Deferred,
    /// 已经把关闭命令发出去了。
    Sent,
}

/// 看的是哪一屏。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum View {
    /// **待确认队列**：打开工具就是它（ADR-0002）。
    #[default]
    Queue,
    /// 变体表：库浏览的骨架，票 25 在它上面长。
    Variants,
}

impl View {
    /// 顶栏照这个次序摆。
    pub const ALL: [Self; 2] = [Self::Queue, Self::Variants];

    /// 这一屏叫什么。用**词表**里的词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Queue => "待确认队列",
            Self::Variants => "变体",
        }
    }
}

/// 界面本体。
pub struct App {
    site: Site,
    /// 看的是哪一屏。**默认是队列**。
    view: View,
    /// 待确认队列那一屏。
    queue: queue::Screen,
    window: Window,
    /// 筛选与排序。**界面上这一份是源头**，[`Window`] 里那一份是它的副本，每帧同步一次。
    query: VariantQuery,
    selected: Option<u64>,
    detail: Option<VariantRow>,
    sample: bool,
    closing: Closing,
    /// 把表格的滚动位置强按到这个像素偏移。**只有量帧率时才设**（[`crate::bench`]），
    /// 真界面上永远是 `None`。
    pub scroll_to: Option<f32>,
}

impl App {
    /// 开一个界面，数据来自这份现成的库。**打开就是待确认队列。**
    #[must_use]
    pub fn new(site: Site) -> Self {
        let mut queue = queue::Screen::new();
        queue.reload(&site);
        Self {
            site,
            view: View::default(),
            queue,
            window: Window::new(SPAN),
            query: VariantQuery::default(),
            selected: None,
            detail: None,
            sample: false,
            closing: Closing::No,
            scroll_to: None,
        }
    }

    /// 关窗走到哪一拍了。测试拿它核对两拍的次序。
    #[must_use]
    pub fn closing(&self) -> Closing {
        self.closing
    }

    /// 看的是哪一屏。
    #[must_use]
    pub fn view(&self) -> View {
        self.view
    }

    /// 换一屏。量帧率那条路拿它点名要量哪一屏。
    pub fn show_view(&mut self, view: View) {
        self.view = view;
    }

    /// 待确认队列那一屏，供测试查「列出多少条、选中多少条」。
    #[must_use]
    pub fn queue(&self) -> &queue::Screen {
        &self.queue
    }

    /// 队列那一屏**连它的库**。
    ///
    /// 两样一起交出来，是因为队列上每一个真会花时间的动作——排计划、落下——都同时要
    /// 它们俩。实测（[`crate::bench::queue`]）与测试拿它走界面上那条一模一样的路，
    /// 而不是另写一份简化版。
    pub fn queue_and_site(&mut self) -> (&mut queue::Screen, &mut Site) {
        (&mut self.queue, &mut self.site)
    }

    /// 这份现场。
    #[must_use]
    pub fn site(&self) -> &Site {
        &self.site
    }

    /// 窗口，供测试查「内存里装了几行」。
    #[must_use]
    pub fn window(&self) -> &Window {
        &self.window
    }

    /// 画一帧。`eframe` 与量帧率的那条路走的是同一个函数——量出来的才是这个界面的代价。
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.handle_close(ui.ctx());
        egui::Panel::top("顶栏").show(ui, |ui| self.top_bar(ui));
        match self.view {
            View::Queue => {
                let (queue, site) = (&mut self.queue, &mut self.site);
                queue.ui(ui, site);
            }
            View::Variants => self.variants(ui),
        }
    }

    /// 变体表那一屏（票 22 的骨架，票 25 在它上面长）。
    fn variants(&mut self, ui: &mut egui::Ui) {
        self.window.set_query(self.query.clone());
        self.window.sync(&self.site.catalog);
        egui::Panel::bottom("详情").show(ui, |ui| self.detail_panel(ui));
        egui::CentralPanel::default().show(ui, |ui| {
            if self.sample {
                self.font_sample(ui);
                ui.separator();
            }
            let picked = Table {
                catalog: &self.site.catalog,
                window: &mut self.window,
                query: &mut self.query,
                selected: &mut self.selected,
                scroll_to: self.scroll_to,
            }
            .show(ui);
            if let Some(row) = picked {
                self.detail = Some(row);
            }
        });
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for view in View::ALL {
                ui.selectable_value(&mut self.view, view, view.label());
            }
            ui.separator();
            match self.view {
                View::Queue => {
                    let (queue, site) = (&mut self.queue, &self.site);
                    queue.status(ui, site);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(format!("沉淀库 {}", self.site.store.location()));
                    });
                }
                View::Variants => {
                    ui.toggle_value(&mut self.sample, "字体样张");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(error) = self.window.error() {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        } else {
                            ui.label(format!(
                                "{} 行；内存里 {} 行、读库 {} 次",
                                self.window.total(),
                                self.window.retained(),
                                self.window.reads()
                            ));
                        }
                    });
                }
            }
        });
    }

    fn detail_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        match &self.detail {
            None => {
                ui.label("点一行看详情。中文输入在这块面板里，不在表格单元格里。");
            }
            Some(variant) => {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(&variant.key);
                    ui.separator();
                    ui.label(format!(
                        "平台 {}｜成型规则 {}｜{} 个文件｜{}",
                        variant.platform.as_deref().unwrap_or("未知"),
                        variant.rule,
                        variant.files,
                        human_bytes(variant.bytes),
                    ));
                });
            }
        }
        // 筛选框在面板里而不在顶栏，与队列那一屏同一条规矩：会碰到输入法的控件全收在
        // **不虚拟化**的面板里。筛选本身下推到中立库（`catalog::browse`）。
        ui.horizontal(|ui| {
            ui.label("筛选");
            ui.add(
                egui::TextEdit::singleline(&mut self.query.contains)
                    .desired_width(320.0)
                    .hint_text("键里含这段文字"),
            );
        });
        ui.add_space(4.0);
    }

    /// 字体样张：把 egui 内置字体缺的那几类字**摆出来给人看**。
    ///
    /// 它是这张票「界面上不出现豆腐块」那条验收的肉眼证据，自动化那一份在
    /// `tests/font.rs`，拿真库核对覆盖面的那一条是 `romcat-gui --font-check --catalog`。
    fn font_sample(&self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.strong("字体样张");
            ui.label(format!("子集 {}", human_bytes(font::subset_bytes() as u64)));
        });
        for (what, text) in font::SAMPLE.iter().copied() {
            ui.horizontal(|ui| {
                ui.add_sized([90.0, 18.0], egui::Label::new(what));
                ui.label(text);
            });
        }
        // OFL 要求分发字体时随附许可，而这份字体是嵌在可执行文件里的——许可得跟着走。
        ui.collapsing("字体许可（SIL Open Font License 1.1）", |ui| {
            egui::ScrollArea::vertical()
                .max_height(160.0)
                .show(ui, |ui| ui.monospace(font::LICENSE));
        });
    }

    /// 关窗的两拍。
    ///
    /// 第一拍撤销关闭、放掉文本焦点、把输入法关掉；第二拍才真的发关闭命令。
    /// 顺序反过来（先关窗再关输入法）就是 winit#4626 那个 abort。
    fn handle_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| input.viewport().close_requested()) {
            if self.closing == Closing::No {
                self.closing = Closing::Deferred;
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                // 放掉焦点，让 egui-winit 自己那条通路也停止请求输入法。
                ctx.memory_mut(egui::Memory::stop_text_input);
                ctx.send_viewport_cmd(egui::ViewportCommand::IMEAllowed(false));
                ctx.request_repaint();
            }
        } else if self.closing == Closing::Deferred {
            self.closing = Closing::Sent;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        App::ui(self, ui);
    }
}
