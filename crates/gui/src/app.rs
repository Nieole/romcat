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

use std::path::PathBuf;

use egui::{Align, Layout};

use crate::{library, queue, sublibrary};
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
    /// **库浏览**：翻整个主库、按四个维度筛、改条目的元数据（票 25）。
    ///
    /// 名字里留着 `Variants`，因为它中间那张表就是票 22 那张**变体表**。
    Variants,
    /// **子库**：选择集、差量预览、同步（票 25）。
    Sublibraries,
}

impl View {
    /// 顶栏照这个次序摆。
    pub const ALL: [Self; 3] = [Self::Queue, Self::Variants, Self::Sublibraries];

    /// 这一屏叫什么。用**词表**里的词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Queue => "待确认队列",
            Self::Variants => "库浏览",
            Self::Sublibraries => "子库",
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
    /// 库浏览那一屏。
    library: library::Screen,
    /// 子库那一屏。
    sublibrary: sublibrary::Screen,
    closing: Closing,
}

impl App {
    /// 开一个界面，数据来自这份现成的库。**打开就是待确认队列。**
    ///
    /// `workspace` 是**工作目录**：子库那一屏排差量预览时要读它里头的**媒体池**与
    /// 能力档案名册（`romcat_core::sync::prepare`）。
    #[must_use]
    pub fn new(site: Site, workspace: PathBuf) -> Self {
        let mut queue = queue::Screen::new();
        queue.reload(&site);
        let mut library = library::Screen::new();
        library.reload(&site);
        // 优先级表与导出共用一份：面板上写着的显示标题就是同步到掌机上会看见的那个。
        if let Ok(priorities) = romcat_core::sync::prepare::priorities(None, &workspace) {
            library.set_priorities(priorities);
        }
        // **媒体池不在就不指**：那时详情面板如实说「没查池子」，而不是报一句
        // 「一张都没有」——后者会把人赶去重跑刮削，而问题其实出在工作目录上。
        let pool_dir = romcat_core::workspace::media_pool_dir(&workspace);
        library.set_pool(
            pool_dir
                .is_dir()
                .then(|| romcat_core::scrape::pool::MediaPool::at(&pool_dir)),
        );
        let mut sublibrary = sublibrary::Screen::new(workspace);
        sublibrary.reload(&site);
        Self {
            site,
            view: View::default(),
            queue,
            library,
            sublibrary,
            closing: Closing::No,
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

    /// 库浏览那一屏，供测试查「筛出多少行、点开的那一条是什么」。
    #[must_use]
    pub fn library(&self) -> &library::Screen {
        &self.library
    }

    /// 库浏览那一屏**连它的库**。改元数据这件事同时要它们俩。
    pub fn library_and_site(&mut self) -> (&mut library::Screen, &mut Site) {
        (&mut self.library, &mut self.site)
    }

    /// 子库那一屏，供测试查「有几个子库、差量预览长什么样」。
    #[must_use]
    pub fn sublibrary(&self) -> &sublibrary::Screen {
        &self.sublibrary
    }

    /// 子库那一屏**连它的库**。建子库、写规则、排预览、同步都同时要它们俩。
    pub fn sublibrary_and_site(&mut self) -> (&mut sublibrary::Screen, &mut Site) {
        (&mut self.sublibrary, &mut self.site)
    }

    /// 变体表背后那扇窗，供测试查「内存里装了几行」。
    #[must_use]
    pub fn window(&self) -> &crate::table::Window {
        self.library.window()
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
            View::Variants => {
                let (library, site) = (&mut self.library, &mut self.site);
                library.ui(ui, site);
            }
            View::Sublibraries => {
                let (sublibrary, site) = (&mut self.sublibrary, &mut self.site);
                sublibrary.ui(ui, site);
            }
        }
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
                    let (library, site) = (&mut self.library, &self.site);
                    library.status(ui, site);
                }
                View::Sublibraries => {
                    let (sublibrary, site) = (&mut self.sublibrary, &self.site);
                    sublibrary.status(ui, site);
                }
            }
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
