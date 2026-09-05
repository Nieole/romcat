//! 窗口本体：**待确认队列**是打开工具后看见的那一屏，变体表是它旁边的另一屏。
//!
//! ## 为什么默认是队列而不是封面墙
//!
//! ADR-0002 定死了：GUI 的主界面是待确认队列，封面墙是次要视图。库里以**汉化版**为主，
//! 而汉化补丁改了字节，最可靠的那一环（精确哈希）对主力内容结构性失效——于是「人来裁」
//! 不是收尾工作，是这条管线的正文。真机上那是一万八千多条，所以那一屏摆出来的是
//! **分好的几十批**而不是一万八千行的表（票 `gui-redesign/09`）。
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

use crate::{library, queue, roots, sublibrary, task};
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
    /// **库**：这个库由什么构成——**一组根**加上**数据源**（票 `gui-redesign/02`）。
    Library,
    /// **浏览**：找到这一批，然后对它施加操作——主列表一个作品一行，
    /// 变体在详情面板里挑（票 `gui-redesign/03`）。
    ///
    /// 名字里还留着 `Variants`，模块也还叫 `library`：改名要连带动
    /// `crates/gui/tests/library.rs` 与 `Cargo.toml` 里那条 `[[test]]`，
    /// 留给票 `12` 一起收（挂单 Q61）。
    Variants,
    /// **子库**：管住这几台设备——一台一张卡，配目标、排差量、同步。
    ///
    /// **这一屏不选内容**（票 `gui-redesign/11`）：选择集在这儿只读，改它点「改选择」
    /// 跳去浏览屏，调完按「更新到子库」回来（[`App::route`]）。
    Sublibraries,
    /// **任务**：排队、进度、可停、历史。**不发起操作，只承接**（票 01）。
    Tasks,
}

impl View {
    /// 顶栏照这个次序摆。
    pub const ALL: [Self; 5] = [
        Self::Queue,
        Self::Library,
        Self::Variants,
        Self::Sublibraries,
        Self::Tasks,
    ];

    /// 这一屏叫什么。用**词表**里的词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Queue => "待确认队列",
            Self::Library => "库",
            Self::Variants => "浏览",
            Self::Sublibraries => "子库",
            Self::Tasks => "任务",
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
    /// 库那一屏：一组根 + 数据源。
    roots: roots::Screen,
    /// 浏览那一屏。
    library: library::Screen,
    /// 子库那一屏。
    sublibrary: sublibrary::Screen,
    /// 任务那一屏。
    tasks: task::Screen,
    /// **任务台**：长活排在这儿跑，跑在画帧那条线程之外。
    board: task::Tasks,
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
        let mut library = library::Screen::new(workspace.clone());
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
        let mut roots = roots::Screen::new(workspace.clone());
        roots.reload(&site);
        let mut sublibrary = sublibrary::Screen::new(workspace);
        sublibrary.reload(&site);
        Self {
            site,
            view: View::default(),
            queue,
            roots,
            library,
            sublibrary,
            tasks: task::Screen::new(),
            board: task::Tasks::new(),
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

    /// 库那一屏，供测试查「有几个根、数据源什么状况」。
    #[must_use]
    pub fn roots(&self) -> &roots::Screen {
        &self.roots
    }

    /// 库那一屏、它的库、**再加任务台**。加根、扫描、取数据源这三下都要它们。
    pub fn roots_site_and_tasks(
        &mut self,
    ) -> (&mut roots::Screen, &mut Site, &mut task::Tasks) {
        (&mut self.roots, &mut self.site, &mut self.board)
    }

    /// 浏览那一屏，供测试查「筛出多少行、点开的那一行是什么」。
    #[must_use]
    pub fn library(&self) -> &library::Screen {
        &self.library
    }

    /// 浏览那一屏**连它的库**。改元数据这件事同时要它们俩。
    pub fn library_and_site(&mut self) -> (&mut library::Screen, &mut Site) {
        (&mut self.library, &mut self.site)
    }

    /// 浏览那一屏、它的库、**再加任务台**。按「刮削选中…」之后那一下三样都要：
    /// 展开这一批的键、算那本账、把活排到台上去（票 `gui-redesign/10`）。
    pub fn library_site_and_tasks(
        &mut self,
    ) -> (&mut library::Screen, &mut Site, &mut task::Tasks) {
        (&mut self.library, &mut self.site, &mut self.board)
    }

    /// 子库那一屏，供测试查「有几个子库、差量预览长什么样」。
    #[must_use]
    pub fn sublibrary(&self) -> &sublibrary::Screen {
        &self.sublibrary
    }

    /// 子库那一屏**连它的库**。建子库、写规则、同步都同时要它们俩。
    pub fn sublibrary_and_site(&mut self) -> (&mut sublibrary::Screen, &mut Site) {
        (&mut self.sublibrary, &mut self.site)
    }

    /// 子库那一屏、它的库、**再加任务台**。排差量预览这一下三样都要：
    /// 从库里分一份只读连接出来，把活排到台上去。
    pub fn sublibrary_site_and_tasks(
        &mut self,
    ) -> (&mut sublibrary::Screen, &mut Site, &mut task::Tasks) {
        (&mut self.sublibrary, &mut self.site, &mut self.board)
    }

    /// **任务台**，供测试与实测查「跑着什么、历史几条」。
    #[must_use]
    pub fn tasks(&self) -> &task::Tasks {
        &self.board
    }

    /// 任务台，供测试与实测往上排活、按停下。
    pub fn tasks_mut(&mut self) -> &mut task::Tasks {
        &mut self.board
    }

    /// 问一遍任务台：跑完的那几趟把产物交给该拿它的那一屏。
    ///
    /// 每帧一次。测试与实测在等一趟活跑完时也调它——**走的是界面上那条一模一样的路**。
    pub fn poll_tasks(&mut self) {
        while let Some(done) = self.board.poll() {
            // **各屏按任务号认领自己那一趟，不是它的就放过去。** 将来识别与刮削接上来
            // 时，各自在这儿多认一次。
            if self.roots.settle(&self.site, &done) {
                continue;
            }
            // **刮削跑完了要重读一遍**：这一屏画的元数据那几栏正是它刚写进去的。
            if self.library.scrape_mut().settle(&done) {
                self.library.refresh(&self.site);
                continue;
            }
            self.sublibrary.settle(done);
        }
    }

    /// **屏间跳转**：子库屏点「改选择」跳去浏览屏，浏览屏点「更新到子库」跳回来。
    ///
    /// 这一段住在窗口里而不在任何一屏里，因为**只有这儿同时够得着两屏**
    /// （ADR-0005：屏与屏之间不该互相拿着对方）。两屏各自只放下一个「按过了」的记号，
    /// 由这一趟取走：
    ///
    /// - **去程**：把那个子库的规则并成一条预填进浏览屏的筛选器，换到浏览屏。
    ///   规则**原样摊在筛选器里**——人改的时候看得见它真的筛出了什么。
    /// - **回程**：规则已经由浏览屏换进中立库了，这儿只负责让子库屏重读那个子库，
    ///   再换回子库屏。**重读之后那份差量预览当场作废**（`Screen::open` 走的
    ///   `invalidate`）：选择集变了，上一趟排的差量说的已经不是它了。
    ///
    /// **还有一条比回程更宽的**：例外是**一按就落库**的，而人可以按完例外就点
    /// 「不改了」、或者干脆从顶栏切回子库屏——那两条路上都没有「更新到子库」。
    /// 所以浏览屏每动一次某个子库的选择集就留一个记号（`take_touched`），
    /// 由这一趟转告子库屏把为那一台缓着的差量与容量账丢掉（`Screen::forget`）。
    /// 只认回程的话，子库屏会摆着一份按旧选择集排出来的差量，而「同步」认的正是它
    /// （ADR-0016）。
    ///
    /// 每帧一次。测试与实测拿它当那一下——**走的是界面上那条一模一样的路**。
    pub fn route(&mut self) {
        if let Some(jump) = self.sublibrary.take_jump() {
            self.library
                .begin_editing(&self.site, &jump.sublibrary, jump.rule, jump.broken);
            self.view = View::Variants;
        }
        // **先丢账再换屏**：回程那一下也会留下记号，丢在前面，`open` 重读到的就是新的。
        if let Some(name) = self.library.take_touched() {
            self.sublibrary.forget(&self.site, &name);
        }
        if let Some(name) = self.library.take_return() {
            self.sublibrary.reload(&self.site);
            self.sublibrary.open(&self.site, &name);
            self.view = View::Sublibraries;
        }
    }

    /// 变体表背后那扇窗，供测试查「内存里装了几行」。
    #[must_use]
    pub fn window(&self) -> &crate::table::Window {
        self.library.window()
    }

    /// 画一帧。`eframe` 与量帧率的那条路走的是同一个函数——量出来的才是这个界面的代价。
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.handle_close(ui.ctx());
        // 任务台先问一遍：这一帧要画的进度、要交出去的产物都从这儿来。
        self.poll_tasks();
        // **屏间跳转在画之前结算**：按下「改选择」那一下发生在上一帧的画里，
        // 记号也是那时放下的——搁在画完之后问，这一帧就会照旧那一屏画一遍，
        // 人会看见子库屏又闪一下才换过去。点一下 egui 本来就会再要一帧，所以
        // 「下一帧开头结算」在眼里就是「按下去就换」。
        self.route();
        egui::Panel::top("顶栏").show(ui, |ui| self.top_bar(ui));
        match self.view {
            View::Queue => {
                let (queue, site) = (&mut self.queue, &mut self.site);
                queue.ui(ui, site);
            }
            View::Library => {
                let (roots, site, board) = (&mut self.roots, &mut self.site, &mut self.board);
                roots.ui(ui, site, board);
            }
            View::Variants => {
                let (library, site, board) = (&mut self.library, &mut self.site, &mut self.board);
                library.ui(ui, site, board);
            }
            View::Sublibraries => {
                let (sublibrary, site, board) =
                    (&mut self.sublibrary, &mut self.site, &mut self.board);
                sublibrary.ui(ui, site, board);
            }
            View::Tasks => {
                let (tasks, board) = (&mut self.tasks, &mut self.board);
                tasks.ui(ui, board);
            }
        }
        // **这一句要在画完之后问**：排活的那一下就发生在上面那几屏里
        // （子库屏点「排差量预览」）。搁在这一帧开头问的话，刚排上去的那一趟要等到
        // 下一次有输入事件才会被画到——进度不走，「停下」也按不动。
        // **跑完还没被认领的也算**：`run_here` 就地跑完那一趟，结果落进待认领那一格的
        // 时刻已经在本帧问过任务台之后了（`Board::settled` 的文档）。
        if self.board.busy() || self.board.settled() {
            ui.ctx().request_repaint();
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for view in View::ALL {
                let label = match (view, self.board.running()) {
                    // **哪一屏上都看得见台上有活在跑**：跑着的时候人多半正在别的屏上。
                    (View::Tasks, Some(_)) => format!("{} ●", view.label()),
                    _ => view.label().to_string(),
                };
                ui.selectable_value(&mut self.view, view, label);
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
                View::Library => {
                    let (roots, site) = (&mut self.roots, &self.site);
                    roots.status(ui, site);
                }
                View::Variants => {
                    // **抬头上那颗「★ 收藏」真的写库**（票 `gui-redesign/06`），
                    // 所以这一屏的抬头拿的是可变的那一份。
                    let (library, site) = (&mut self.library, &mut self.site);
                    library.status(ui, site);
                }
                View::Sublibraries => {
                    let (sublibrary, site) = (&mut self.sublibrary, &self.site);
                    sublibrary.status(ui, site);
                }
                View::Tasks => {
                    let (tasks, board) = (&mut self.tasks, &self.board);
                    tasks.status(ui, board);
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
