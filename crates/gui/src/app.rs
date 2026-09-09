//! 窗口本体：五屏由顶栏切换，**待确认队列**是打开工具后看见的那一屏。
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
//!
//! ## 五屏共用的那两样也在这儿接上
//!
//! 开窗第一帧装两样：[观感基线](crate::look)——置信度四档的颜色与
//! 键盘焦点长什么样；以及[上次拖到哪儿的版式](crate::layout)——七条面板边界的宽度，
//! 从**工作目录**里读出来塞回 egui。画完一帧再问一遍面板现在多宽，手松开了才落盘。
//! 窗口标题跟着屏走：`romcat — {哪一份库} — {哪一屏}`，**换屏才发一条命令**。

use std::path::PathBuf;

use egui::{Align, Layout};

use crate::{browse, layout, look, queue, roots, sublibrary, task};
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
    /// 名字与模块名从 `Variants` / `library` 改成了 `Browse` / [`crate::browse`]
    /// （票 `12`，挂单 Q61）：规格里这一屏叫「浏览」，而 `library` 那个名字在这个仓库里
    /// 已经归了**库**——`site.library` 是「这份主库叫什么」，`View::Library` 是**库屏**。
    /// 同一个词指着三样东西，谁读代码都得先猜一遍。
    Browse,
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
        Self::Browse,
        Self::Sublibraries,
        Self::Tasks,
    ];

    /// 这一屏叫什么。用**词表**里的词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Queue => "待确认队列",
            Self::Library => "库",
            Self::Browse => "浏览",
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
    browse: browse::Screen,
    /// 子库那一屏。
    sublibrary: sublibrary::Screen,
    /// 任务那一屏。
    tasks: task::Screen,
    /// **任务台**：长活排在这儿跑，跑在画帧那条线程之外。
    board: task::Tasks,
    /// 七条**面板边界**各自拖到哪儿了。存**工作目录**，不存中立库。
    layout: layout::Layout,
    /// 标题里那个库名。默认是这份现场**给人看的**那个名字
    /// （[`Site::display_name`]，也就是开场那一屏上画着的同一个）；合成数据那一路另给
    /// 一个（[`Self::set_library_label`]），免得一屏假名字看着像真库。
    library_label: String,
    /// **观感基线与上次的版式装过了没有。** 只在开窗第一帧装一次。
    prepared: bool,
    /// 上一次写进窗口标题的是哪一屏。**换屏才发一条命令**，不是每帧发一条。
    titled: Option<View>,
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
        let mut browse = browse::Screen::new(workspace.clone());
        browse.reload(&site);
        // 优先级表与导出共用一份：面板上写着的显示标题就是同步到掌机上会看见的那个。
        if let Ok(priorities) = romcat_core::sync::prepare::priorities(None, &workspace) {
            browse.set_priorities(priorities);
        }
        // **媒体池不在就不指**：那时详情面板如实说「没查池子」，而不是报一句
        // 「一张都没有」——后者会把人赶去重跑刮削，而问题其实出在工作目录上。
        let pool_dir = romcat_core::workspace::media_pool_dir(&workspace);
        browse.set_pool(
            pool_dir
                .is_dir()
                .then(|| romcat_core::scrape::pool::MediaPool::at(&pool_dir)),
        );
        let mut roots = roots::Screen::new(workspace.clone());
        roots.reload(&site);
        // **版式先读出来**：面板尺寸要赶在开窗第一帧画面板之前塞进 egui 那张表里
        // （[`layout::Layout::seed`]），晚一帧人就会看见面板从默认宽度跳一下。
        let layout = layout::Layout::load(&workspace);
        let mut sublibrary = sublibrary::Screen::new(workspace);
        sublibrary.reload(&site);
        // **给人看的那个名字，不是标识符。** `Site::library` 是中立库的主文件名
        // （「可读的一半 + 十六位哈希」），人在开场那一屏上看见的却是「我的主库」——
        // 同一份库在相邻两屏上两个样子，而票 01 把原名落进元数据表存在的全部理由
        // 就是这个（ADR-0023、规格 User Story 5）。
        let library_label = site.display_name();
        Self {
            site,
            view: View::default(),
            queue,
            roots,
            browse,
            sublibrary,
            tasks: task::Screen::new(),
            board: task::Tasks::new(),
            layout,
            library_label,
            prepared: false,
            titled: None,
            closing: Closing::No,
        }
    }

    /// 换掉标题里那个库名。
    ///
    /// **合成数据那一路要它**：假数据与真库在界面上长得一模一样，标题是唯一一直看得见的
    /// 区分处（`main.rs` 拿它写「合成数据（演示）」）。真库那一路不必调——默认就是
    /// 这份现场自己的名字。
    pub fn set_library_label(&mut self, label: impl Into<String>) {
        self.library_label = label.into();
        // 名字变了，标题得重发一次。
        self.titled = None;
    }

    /// 窗口标题：**开的是哪一份库、看的是哪一屏**（验收第 6 条）。
    ///
    /// 两样都写进去，是因为它们各自回答一个只有标题答得了的问题：任务栏上并排两个
    /// romcat 时「哪个是哪份库」，以及截图发出来时「这是哪一屏」。
    ///
    /// 库名那一段是**给人看的**那个（[`Site::display_name`]）——与开场那一屏上画着的
    /// 是同一个字，不是那串带哈希的主文件名。
    #[must_use]
    pub fn window_title(&self) -> String {
        format!("romcat — {} — {}", self.library_label, self.view.label())
    }

    /// 七条面板边界各自拖到哪儿了。测试拿它核对「存在工作目录里」。
    #[must_use]
    pub fn layout(&self) -> &layout::Layout {
        &self.layout
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
    pub fn roots_site_and_tasks(&mut self) -> (&mut roots::Screen, &mut Site, &mut task::Tasks) {
        (&mut self.roots, &mut self.site, &mut self.board)
    }

    /// 把一道**工序**排到任务台上。
    ///
    /// **这是排一趟工序的唯一入口**：库屏工序段那一行的按钮走的是它，各屏空态上的捷径
    /// 走的也该是它（票 `gui-self-sufficient/09` 要把队列屏那三处「先跑一次
    /// `romcat identify`」换成就地按钮）。摆在窗口上而不是某一屏里，正因为**别的屏
    /// 够不着库屏**（ADR-0005：屏与屏之间不该互相拿着对方）。
    pub fn start_stage(&mut self, stage: romcat_core::stage::Stage) {
        let (roots, site, board) = (&mut self.roots, &mut self.site, &mut self.board);
        roots.stages_mut().start(stage, site, board);
    }

    /// 浏览那一屏，供测试查「筛出多少行、点开的那一行是什么」。
    #[must_use]
    pub fn browse(&self) -> &browse::Screen {
        &self.browse
    }

    /// 浏览那一屏**连它的库**。改元数据这件事同时要它们俩。
    pub fn browse_and_site(&mut self) -> (&mut browse::Screen, &mut Site) {
        (&mut self.browse, &mut self.site)
    }

    /// 浏览那一屏、它的库、**再加任务台**。按「刮削选中…」之后那一下三样都要：
    /// 展开这一批的键、算那本账、把活排到台上去（票 `gui-redesign/10`）。
    pub fn browse_site_and_tasks(&mut self) -> (&mut browse::Screen, &mut Site, &mut task::Tasks) {
        (&mut self.browse, &mut self.site, &mut self.board)
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
    ///
    /// ## 认领完还要转告浏览屏
    ///
    /// 库屏认领的那两种活（扫描、取数据源）**动的是中立库本身**，而浏览屏缓着的东西
    /// ——窗里那 512 行、总行数、筛选面板上那几档——只在**换查询**时才作废。扫完一个根
    /// 查询一个字都没改，于是库屏说 3 个变体、浏览屏还画着 0 行，同一个窗口里两个数
    /// 对不上。所以认领完就告诉它一声（[`browse::Screen::invalidate`]）。
    ///
    /// **被按停的那一趟也算**：它写进中立库的那半份记录是真的。这里因此不看是哪一种
    /// 收场，只看「库屏认领了没有」。
    pub fn poll_tasks(&mut self) {
        while let Some(done) = self.board.poll() {
            // **各屏按任务号认领自己那一趟，不是它的就放过去。** 将来识别与刮削接上来
            // 时，各自在这儿多认一次。
            if self.roots.settle(&self.site, &done) {
                self.browse.invalidate(&self.site);
                // **识别跑完了，待确认队列自己重新列过**：那一屏的每一批都是识别结论
                // 折出来的，不重列的话人得再点一次「重新列队列」——而那正是这一票要
                // 消掉的那种「还得记住下一步」。这一句住在窗口里而不在库屏里，
                // 因为**只有这儿同时够得着两屏**（ADR-0005，与 [`Self::route`] 同理）。
                if let Some(stage) = self.roots.stages_mut().take_ran() {
                    match stage {
                        romcat_core::stage::Stage::Identify => self.queue.reload(&self.site),
                        // **折标题跑完了，浏览屏上的显示标题跟着更新**：那是这道工序
                        // 起没起作用唯一看得见的地方。走 `refresh` 而不是上面那句
                        // `invalidate`——显示标题画在**详情面板**上，而只有 `refresh`
                        // 连那一格一起重读（`browse::Screen::refresh`）。
                        romcat_core::stage::Stage::FoldTitles => {
                            self.browse.refresh(&self.site);
                        }
                        // **导出跑完了，哪一屏都不必重读**：它写出去的是主库根上那些
                        // 元数据文件，五屏一个都不画它们；库里被它动过的只有**底本**
                        // 与那个时刻戳，而工序段那一行已经由 `Section::settle` 自己
                        // 重问过了。这一支空着是**故意的**，不是漏了——照抄上面两支
                        // 随手 `reload` 一屏，等于每导一趟就白读一遍几万行。
                        romcat_core::stage::Stage::Export => {}
                    }
                }
                continue;
            }
            // **刮削跑完了要重读一遍**：这一屏画的元数据那几栏正是它刚写进去的。
            if self.browse.scrape_mut().settle(&done) {
                self.browse.refresh(&self.site);
                continue;
            }
            // **整批收藏那一趟也要写两份库**（沉淀库那些成员关系、中立库那份投影），
            // 同上：写在认领这一步。
            let Some(done) = self.browse.settle_collection(&mut self.site, done) else {
                continue;
            };
            // **同步那一趟要把清单写回中立库**，所以子库屏认领时拿的是可写的那份现场
            // ——台上那条线拿的是只读连接，写不动（`task::Product` 的文档）。
            self.sublibrary.settle(&mut self.site, done);
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
    /// **还有一条与跳转无关、但同样只有这儿够得着两屏的**：待确认屏落下一批、撤回一批
    /// 之后，浏览屏缓着的那几行（置信度那一列画的正是裁决改的东西）也过期了。队列那一屏
    /// 留一个记号（`queue::Screen::take_changed`），由这一趟转告浏览屏——与库屏扫完一个根
    /// 走的是同一个入口（[`browse::Screen::invalidate`]），只是那一趟由任务台交回来，
    /// 这一趟就发生在本进程的这一帧里。
    ///
    /// 每帧一次。测试与实测拿它当那一下——**走的是界面上那条一模一样的路**。
    pub fn route(&mut self) {
        if self.queue.take_changed() {
            self.browse.invalidate(&self.site);
        }
        if let Some(jump) = self.sublibrary.take_jump() {
            self.browse
                .begin_editing(&self.site, &jump.sublibrary, jump.rule, jump.broken);
            self.view = View::Browse;
        }
        // **先丢账再换屏**：回程那一下也会留下记号，丢在前面，`open` 重读到的就是新的。
        if let Some(name) = self.browse.take_touched() {
            self.sublibrary.forget(&self.site, &name);
        }
        if let Some(name) = self.browse.take_return() {
            self.sublibrary.reload(&self.site);
            self.sublibrary.open(&self.site, &name);
            self.view = View::Sublibraries;
        }
    }

    /// 变体表背后那扇窗，供测试查「内存里装了几行」。
    #[must_use]
    pub fn window(&self) -> &crate::table::Window {
        self.browse.window()
    }

    /// 画一帧。`eframe` 与量帧率的那条路走的是同一个函数——量出来的才是这个界面的代价。
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.prepare(ui.ctx());
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
            View::Browse => {
                let (browse, site, board) = (&mut self.browse, &mut self.site, &mut self.board);
                browse.ui(ui, site, board);
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
        // **画完了才问面板有多宽**：这一帧的边界是刚才那几句 `show` 定下来的。
        self.layout.harvest(ui.ctx());
        // **手松开了才写盘**：拖的过程中每帧写一次是六十次写盘，而那六十次里有
        // 五十九次的值只是路过。egui 那一侧也照这条办（拖的时候不存尺寸）。
        if !ui.ctx().input(|input| input.pointer.any_down()) {
            self.layout.flush();
        }
    }

    /// 开窗第一帧装两样：**观感基线**与**上次拖到哪儿的版式**。
    ///
    /// 两样都只装一次。观感基线装两次没坏处但白花；版式装两次是真会坏事——人正拖着的
    /// 那一下会被上一次存下的值按回去。
    ///
    /// 装在这儿而不在 `main.rs` 里，是因为**不开窗跑帧那一路也要它**：headless 的一帧
    /// 走的就是这个函数，两处各装一遍迟早漏一处，而漏的那一处正是测试跑的那一路。
    fn prepare(&mut self, ctx: &egui::Context) {
        if self.prepared {
            return;
        }
        self.prepared = true;
        look::install(ctx);
        self.layout.seed(ctx);
    }

    /// 换屏了就把窗口标题改掉。**换屏才发**，不是每帧发一条。
    fn retitle(&mut self, ctx: &egui::Context) {
        if self.titled == Some(self.view) {
            return;
        }
        self.titled = Some(self.view);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));
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
            // 屏名那一排刚画完，`self.view` 已经是这一帧要看的那一屏——标题跟着它改。
            self.retitle(ui.ctx());
            ui.separator();
            // **版式存不下来就说一句**：吞掉的话人只看见「拖了半天，下次全忘」，
            // 而真正的病在工作目录上（写不动的工作目录还会连累中立库与沉淀库）。
            if let Some(说的) = self.layout.error() {
                ui.colored_label(ui.visuals().warn_fg_color, format!("版式存不下来：{说的}"));
                ui.separator();
            }
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
                View::Browse => {
                    // **抬头上那颗「★ 收藏」真的写库**（票 `gui-redesign/06`），
                    // 所以这一屏的抬头拿的是可变的那一份。它同时**往任务台上排活**
                    // ——那一下在真库量级上是几秒的读（票 `parking-3/09`）。
                    let (browse, site, board) = (&mut self.browse, &mut self.site, &mut self.board);
                    browse.status(ui, site, board);
                }
                View::Sublibraries => {
                    // **顶栏上那个「停下」按得动**，所以任务台拿的是可变的那一份。
                    let (sublibrary, site, board) =
                        (&mut self.sublibrary, &self.site, &mut self.board);
                    sublibrary.status(ui, site, board);
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
