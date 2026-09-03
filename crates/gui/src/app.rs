//! 界面骨架：一张变体表、一个详情面板、一份字体样张。
//!
//! 这是**骨架**而不是成品——待确认队列（票 24）与库浏览（票 25）在这上面长。它现在
//! 就该立住的是三件事：表格的形状、中文输入的位置、退出的路线。
//!
//! ## 中文输入放在详情面板里
//!
//! 不放在表格单元格里，而且不是审美选择：表格是虚拟化的，正在组字的那一行一旦滚出视口，
//! 那个控件就不存在了，输入法上屏时会没人接（ADR-0005 的修订段）。详情面板在表格之外，
//! 怎么滚都在。
//!
//! ## 退出前先关输入法
//!
//! macOS 上组字过程中直接关窗会 abort（winit#4626），规避办法是**窗口还活着的时候**先
//! `set_ime_allowed(false)`。所以关窗请求来的第一帧不真的关：先撤销关闭、放掉文本焦点、
//! 发一条 `IMEAllowed(false)`，下一帧再关。[`App::closing`] 就是这两拍的状态。

use egui::{Align, Layout};
use romcat_core::catalog::{Catalog, VariantQuery, VariantRow};
use romcat_core::report::human_bytes;

use crate::font;
use crate::table::{SPAN, Table, Window};

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

/// 界面本体。
pub struct App {
    catalog: Catalog,
    window: Window,
    /// 筛选与排序。**界面上这一份是源头**，[`Window`] 里那一份是它的副本，每帧同步一次。
    query: VariantQuery,
    selected: Option<u64>,
    detail: Option<VariantRow>,
    /// 详情面板里的备注框。
    ///
    /// **它是验证脚手架，不是功能**：内容不落库，也没人读。存在的理由是票 23 要在
    /// Windows 真机上敲中文，而 ADR-0005 定的位置是详情面板——这个框把那个位置占住，
    /// 免得票 24 顺手把输入放进表格单元格里。
    note: String,
    sample: bool,
    closing: Closing,
    /// 把表格的滚动位置强按到这个像素偏移。**只有量帧率时才设**（[`crate::bench`]），
    /// 真界面上永远是 `None`。
    pub scroll_to: Option<f32>,
}

impl App {
    /// 开一个界面，数据来自这份中立库。
    #[must_use]
    pub fn new(catalog: Catalog) -> Self {
        Self {
            catalog,
            window: Window::new(SPAN),
            query: VariantQuery::default(),
            selected: None,
            detail: None,
            note: String::new(),
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

    /// 窗口，供测试查「内存里装了几行」。
    #[must_use]
    pub fn window(&self) -> &Window {
        &self.window
    }

    /// 画一帧。`eframe` 与量帧率的那条路走的是同一个函数——量出来的才是这个界面的代价。
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.handle_close(ui.ctx());
        self.window.set_query(self.query.clone());
        self.window.sync(&self.catalog);

        egui::Panel::top("顶栏").show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("详情").show(ui, |ui| self.detail_panel(ui));
        egui::CentralPanel::default().show(ui, |ui| {
            if self.sample {
                self.font_sample(ui);
                ui.separator();
            }
            let picked = Table {
                catalog: &self.catalog,
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
            ui.heading("变体");
            ui.separator();
            ui.label("筛选");
            ui.add(
                egui::TextEdit::singleline(&mut self.query.contains)
                    .desired_width(260.0)
                    .hint_text("键里含这段文字"),
            );
            ui.separator();
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
        ui.horizontal(|ui| {
            ui.label("备注");
            ui.add(
                egui::TextEdit::multiline(&mut self.note)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .hint_text("中文输入在这里试打（还不落库）"),
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
