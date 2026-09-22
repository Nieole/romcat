//! 窗口本体：六屏由左栏切换，每屏一个屏头，**待确认队列**是打开工具后看见的那一屏。
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
//! ## 六屏共用的那两样也在这儿接上
//!
//! 开窗第一帧装两样：[观感基线](crate::look)——置信度四档的颜色与
//! 键盘焦点长什么样；以及[上次拖到哪儿的版式](crate::layout)——六条面板边界的宽度，
//! 从**工作目录**里读出来塞回 egui。画完一帧再问一遍面板现在多宽，手松开了才落盘。
//! 窗口标题跟着屏走：`romcat — {哪一份库} — {哪一屏}`，**换屏才发一条命令**。
//!
//! ## 左栏与屏头
//!
//! 顶栏没了（票 `gui-looks-like-the-design/32`）：导航在[左栏](crate::rail)，「切换主库」是左栏顶上那张卡；
//! 每屏一个[屏头](crate::look::screen_header)——标题、副标题，右侧是那一屏原来画在顶栏上的那一段
//! （`<屏>::Screen::status`，原样调进来）。屏里自己画的东西一行没动，照稿重排由各屏自己的票接手。
//!
//! ## 左栏的几个数从哪来
//!
//! **一个都不在这儿另算**，取的都是现成的那一份：库是库屏手上的根数，待确认是队列里待裁决的条数
//! （还没跑过识别画「—」），子库是子库屏列出来的个数，任务是台上跑着的加排着的。另两个要问库：
//! 浏览是核心库按默认那一套筛选数的作品数（[`Catalog::work_total`](romcat_core::catalog::Catalog::work_total)，
//! 与浏览屏没筛过时的总行数是同一句查询；一个根都还没扫过、数又是 0 时画「—」），已保存的裁决是沉淀库自己数的（`Store::counts`）。这两个
//! **不每帧问**：开库时问一次，库变了再问（`App::recount`）——库屏认领完一趟、待确认屏落下或撤回一批。

use std::path::PathBuf;

use crate::{browse, dialog, layout, look, queue, rail, roots, settings, sublibrary, task};
use romcat_core::catalog::WorkQuery;
use romcat_core::report::thousands;
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

/// **这一下是不是「单按了这个键」**：按下、而且**没带 `⌘` / `Ctrl` / `Alt`**。
///
/// 照的是设计稿那一支（`if(typing||mod&&k.toLowerCase()!=='a')return`）：带修饰键的那几下
/// 各有各的归处（`⌘ A` 全选、`⌘ F` 搜索），剩下的一概不接——不然 macOS 上 `Ctrl+F`
/// （系统里那是「光标右移」）会被当成收藏。
///
/// **看的是那一下自己带的修饰键**，不是 `InputState::modifiers` 那份「眼下按着什么」：
/// 后者由开窗那一层每帧填，测试里合成的输入填不出来——而这道门正是测试要钉的。
/// `shift` 不算：`?` 在多数键盘上就得按 `shift` 才打得出来。
fn 单键按下(ctx: &egui::Context, key: egui::Key) -> bool {
    ctx.input(|input| {
        input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key {
                    key: 按的,
                    modifiers,
                    pressed: true,
                    ..
                } if *按的 == key && !(modifiers.command || modifiers.ctrl || modifiers.alt)
            )
        })
    })
}

/// 看的是哪一屏。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
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
    /// 已经归了**库**——`site.library`（后来正名为 `site.library_identity`）是**主库标识**，`View::Library` 是**库屏**。
    /// 同一个词指着三样东西，谁读代码都得先猜一遍。
    Browse,
    /// **子库**：管住这几台设备——一台一张卡，目标设置（一层弹层）、排差量、同步。
    ///
    /// **规则与例外的增减在浏览屏上做**（票 `gui-redesign/11`）：改它点「改选择」跳去浏览屏，
    /// 调完按「更新到子库」回来（[`App::route`]）。这一屏只留一处写例外的动作：超限时删减建议表
    /// 上的「排除」（票 `gui-looks-like-the-design/20`）。
    Sublibraries,
    /// **任务**：排队、进度、可停、历史。**不发起操作，只承接**（票 01）。
    Tasks,
    /// **设置**：八节摆在一屏上——常规、工作目录、数据源、刮削、导出、工具、快捷键、关于
    /// （票 `gui-looks-like-the-design/31`）。
    ///
    /// **它是一屏，不是一层弹层**（规格实现决定三）：与另外五屏同级，走同一套左栏入口、同一个屏头。
    Settings,
}

impl View {
    /// 全部六屏。左栏里的次序另有一份，照设计稿分组（[`rail::GROUPS`]）。
    pub const ALL: [Self; 6] = [
        Self::Queue,
        Self::Library,
        Self::Browse,
        Self::Sublibraries,
        Self::Tasks,
        Self::Settings,
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
            Self::Settings => "设置",
        }
    }

    /// 左栏入口与屏头上写的名字。**只有待确认队列那一屏与 [`Self::label`] 不同**：左栏与屏头照设计稿写
    /// 「待确认」，窗口标题留「待确认队列」（拿主意的人 2026-09-14 定，词表**待确认队列**那一条写着）。
    #[must_use]
    pub fn nav_label(self) -> &'static str {
        match self {
            Self::Queue => "待确认",
            other => other.label(),
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
    /// 设置那一屏。
    settings: settings::Screen,
    /// **任务台**：长活排在这儿跑，跑在画帧那条线程之外。
    board: task::Tasks,
    /// 六条**面板边界**各自拖到哪儿了。存**工作目录**，不存中立库。
    layout: layout::Layout,
    /// 标题里那一段名字。默认是这份现场的**主库原名**
    /// （[`Site::display_name`]，也就是开场那一屏上画着的同一个）；合成数据那一路另给
    /// 一个（[`Self::set_library_label`]），免得一屏假名字看着像真库。
    library_label: String,
    /// 底部状态栏右边那一段：**工作目录**，`HOME` 那一截缩成 `~`（设计稿写的是
    /// `~/.local/share/romcat`）。截图那一路另给一个定死的（[`Self::set_workspace_label`]）：
    /// 测试的工作目录是临时目录，每台机器、每一趟都不一样。
    workspace_label: String,
    /// **观感基线与上次的版式装过了没有。** 只在开窗第一帧装一次。
    prepared: bool,
    /// 上一次写进窗口标题的那一句。**标题变了才发一条命令**（换屏、打开或换掉作品详情页），不是每帧发一条。
    titled: Option<String>,
    /// **按 `?` 摊开的那层快捷键表**开着没有（票 `gui-looks-like-the-design/14`）。
    ///
    /// 开没开着由画它的那一屏自己记——这一层是窗口，因为 `?` 在哪一屏上按都摊得开
    /// （[`crate::dialog`] 那一条「弹层开没开着是画它的那一屏自己记着的」）。
    keys_sheet: bool,
    /// **人按了左栏顶上那张「切换主库」。**
    ///
    /// 这一层自己换不了库：六屏全建立在「库一定在」这个前提上，换库那一下要把整份
    /// **现场**换掉，而那件事在 [`Program`](crate::program::Program) 上（ADR-0023）。
    /// 这儿只放下一个记号，由它下一步读走——**放下就不撤**：读到它的那一下这份 `App`
    /// 整个被丢掉，没有「换回来」这回事。
    switching: bool,
    /// **人在设置屏上挑了一个新的工作目录。**
    ///
    /// 与 [`Self::switching`] 同一个形状、同一个理由：换一个工作目录等于换一整套工具状态
    /// （词表**工作目录**那一条），那件事同样在 [`Program`](crate::program::Program) 上。
    /// 这儿只放下一个记号，由它下一步读走，整份退回**开场**——那边列得出新目录里有哪几份库。
    switch_workspace: Option<PathBuf>,
    /// 左栏「浏览」那一项的**作品数**：核心库按默认那一套筛选数的。`None` 是数不出来。
    /// 不每帧问，见模块文档「左栏的几个数从哪来」。
    works: Option<u64>,
    /// 左栏底下那句「已保存 N 条裁决」：沉淀库自己数的。`None` 是读不出来。
    verdicts: Option<u64>,
    /// **窗口窄时人点「»」临时展开了左栏**，记的是点的那一刻窗口多宽。宽度一变就作废，回到自动收起；
    /// 不写进版式文件（拿主意的人 2026-09-14 定，挂单 `Q867`）。
    rail_peek: Option<f32>,
    /// **这扇窗是不是带 `--demo` 启动的**（合成数据那一路，`main.rs` 设）。开发用的东西只在它为真时摆出来——
    /// 浏览屏屏头上那颗「字体样张」（挂单 `Q874`）。看的是运行时，不是编译开关：截图测试不设它，
    /// 开不开 `demo` 特性截出来的图都一样。
    demo: bool,
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
        browse.restore_view_preferences(&layout);
        // **库屏那几块收着没有**也住在这份版式里（票 `gui-looks-like-the-design/06`）：开窗之前交给库屏。
        for fold in layout::Fold::ALL {
            roots.set_folded(fold, layout.folded(fold));
        }
        let workspace_label = shorten_home(&workspace);
        let workspace_for_settings = workspace.clone();
        let mut sublibrary = sublibrary::Screen::new(workspace);
        sublibrary.reload(&site);
        // **给人看的主库原名，不是主库标识。** `Site::library_identity` 是中立库的主文件名
        // （「可读的一半 + 十六位哈希」），人在开场那一屏上看见的却是「我的主库」——
        // 同一份库在相邻两屏上两个样子，而票 01 把原名落进元数据表存在的全部理由
        // 就是这个（ADR-0023、规格 User Story 5）。
        let library_label = site.display_name();
        let mut app = Self {
            site,
            view: View::default(),
            queue,
            roots,
            browse,
            sublibrary,
            tasks: task::Screen::new(),
            settings: settings::Screen::new(workspace_for_settings),
            board: task::Tasks::new(),
            layout,
            library_label,
            workspace_label,
            prepared: false,
            titled: None,
            keys_sheet: false,
            switching: false,
            switch_workspace: None,
            works: None,
            verdicts: None,
            rail_peek: None,
            demo: false,
            closing: Closing::No,
        };
        app.recount();
        app
    }

    /// 重问左栏上那两个要问库的数：浏览的作品数、已保存的裁决数（模块文档「左栏的几个数从哪来」）。
    ///
    /// 开库时一次，之后只在**库变了**的那几下调：库屏认领完一趟（扫描、识别、刮削……）、待确认屏落下或撤回一批。
    fn recount(&mut self) {
        self.works = self.site.catalog.work_total(&WorkQuery::default()).ok();
        self.verdicts = self.site.store.counts().ok().map(|counts| counts.total);
    }

    /// 换掉标题里那一段名字。
    ///
    /// **合成数据那一路要它**：假数据与真库在界面上长得一模一样，标题是唯一一直看得见的
    /// 区分处（`main.rs` 拿它写「合成数据（演示）」）。真库那一路不必调——默认就是
    /// 这份现场自己的名字。
    pub fn set_library_label(&mut self, label: impl Into<String>) {
        self.library_label = label.into();
        // 名字变了，标题得重发一次。
        self.titled = None;
    }

    /// 记下**这扇窗是带 `--demo` 启动的**：开发用的东西（浏览屏屏头上那颗「字体样张」）从此摆出来。
    ///
    /// 只有 `main.rs` 在合成数据那一路调；截图与别的测试一律不调（挂单 `Q874`）。
    pub fn mark_demo(&mut self) {
        self.demo = true;
    }

    /// 换掉底部状态栏右边那一段工作目录。
    ///
    /// **截图那一路要它**：测试的工作目录是临时目录，每台机器、每一趟都不一样，
    /// 照实画的话同一屏的截图一趟一个样。真窗口那一路不必调——默认就是工作目录本身。
    pub fn set_workspace_label(&mut self, label: impl Into<String>) {
        self.workspace_label = label.into();
    }

    /// 定死任务屏与状态栏上跟着挂钟走的那几个数（[`task::Clock`]）：已用、剩余约、耗时、收场时刻。
    ///
    /// **截图那一路要它**，理由同 [`Self::set_workspace_label`]：照实画的话同一屏的截图一趟一个样。
    /// 真窗口那一路不必调。
    pub fn pin_task_clock(&mut self, clock: task::Clock) {
        self.tasks.pin_clock(clock);
    }

    /// 窗口标题：**开的是哪一份库、看的是哪一屏**（验收第 6 条）。
    ///
    /// 两样都写进去，是因为它们各自回答一个只有标题答得了的问题：任务栏上并排两个
    /// romcat 时「哪个是哪份库」，以及截图发出来时「这是哪一屏」。
    ///
    /// 名字那一段默认是**主库原名**（[`Site::display_name`]）——与开场那一屏上画着的
    /// 是同一个字，不是那串带哈希的主文件名。
    #[must_use]
    pub fn window_title(&self) -> String {
        match self.browse.page_title() {
            // **作品详情页开着时连作品名一起写**（设计稿 `render` 里 `wdOn` 那一支）：那一层盖住了整块浏览屏，
            // 截图发出来时「这是哪个作品」也只有标题答得了。
            Some(work) if self.view == View::Browse => {
                format!(
                    "romcat — {} — {} — {work}",
                    self.library_label,
                    self.view.label()
                )
            }
            _ => format!("romcat — {} — {}", self.library_label, self.view.label()),
        }
    }

    /// 六条面板边界各自拖到哪儿了。测试拿它核对「存在工作目录里」。
    #[must_use]
    pub fn layout(&self) -> &layout::Layout {
        &self.layout
    }

    /// **人要换一份库了吗**——左栏顶上那张「切换主库」按下去之后就是真。
    ///
    /// [`Program`](crate::program::Program) 每帧画完问一次，问到就换回**开场**。
    /// 摆成一个记号而不是让这一层自己动手，是因为换库要换掉整份**现场**，而这一层
    /// 拿的是一份**已经开好的**现场——那件事只有它上头那一层做得了。
    #[must_use]
    pub fn switching(&self) -> bool {
        self.switching
    }

    /// **人在设置屏上挑好的那个新工作目录**：[`Program`](crate::program::Program) 取走它，
    /// 整份退回开场。取走就没了，同 [`Self::switching`] 那个记号一样不撤。
    pub fn take_workspace_switch(&mut self) -> Option<PathBuf> {
        self.switch_workspace.take()
    }

    /// 设置那一屏，供测试查看的是哪一节、改名改成了没有。
    #[must_use]
    pub fn settings(&self) -> &settings::Screen {
        &self.settings
    }

    /// 设置那一屏，改得动的那一份：截图那一路拿它翻到某一节、把探 ffmpeg 那个程序名钉死。
    pub fn settings_mut(&mut self) -> &mut settings::Screen {
        &mut self.settings
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

    /// **刚认领出来的那一份的第一个根**：加上它，再把第一趟扫描排到任务台上。
    ///
    /// [开场那条向导](crate::claim)走完之后欠着的就是这一下
    /// （[`Chosen::first_root`](crate::opening::Chosen::first_root)）。它走的是**库屏那
    /// 两条现成的路**——加根是 [`roots::Screen::add_root`]（判断全在
    /// [`romcat_core::catalog::roots::add_root`] 里），排扫描是 [`roots::Screen::scan`]
    /// （**断点**、并发档、被按停时怎么记账，全在那一条上）。向导重画 UI，这两下与库屏上
    /// 按出来的是同一件事（ADR-0005）。
    ///
    /// **它不是第二条加根的路**：第二个根照旧从库屏加。这一条只在向导走完的那一下走
    /// 一趟，比库屏多做的只有两件——把那一趟扫描接着排上去，以及换到任务屏，好让人
    /// 当场看见它在跑。
    ///
    /// 摆在窗口上而不在任何一屏里，与 [`Self::start_stage`] 同理：**只有这儿同时够得着
    /// 库屏与任务台**。
    pub fn claim_first_root(&mut self, first: &crate::claim::FirstRoot) {
        let Some(根名) = self.roots.add_root(&self.site, &first.path, &first.name) else {
            // **走到这儿说明向导拦过一遍之后又被拦了一次**：那个目录刚被挪走，或者中立库
            // 写不动。那句话已经写在库屏上了，换过去让人看见——**不吞掉，也不退回开场**：
            // 库这时候已经建出来了，而库屏正是接着把这个根加上的地方。
            self.view = View::Library;
            return;
        };
        let (roots, site, board) = (&mut self.roots, &mut self.site, &mut self.board);
        roots.scan(site, board, &根名);
        // **落在任务屏上**：这一下之后唯一在动的就是那一趟扫描，而人要的正是「立刻看得见
        // 进度、按得下停下」。默认那一屏（**待确认队列**）这会儿必然是空的——一份刚建出来
        // 的库里一个变体都还没有，而那句空话答不了「我刚才那一下成了没有」。
        self.view = View::Tasks;
    }

    /// 把一道**工序**排到任务台上。
    ///
    /// **这是排一趟工序的唯一入口**：库屏工序段那一行的按钮走的是它，各屏空态上那几颗
    /// 捷径走的也是它——那几屏够不着库屏，所以它们只留一个记号，由 [`Self::route`]
    /// 取走再交到这儿（票 `gui-self-sufficient/09`）。摆在窗口上而不是某一屏里，
    /// 正因为**别的屏够不着库屏**（ADR-0005：屏与屏之间不该互相拿着对方）。
    pub fn start_stage(&mut self, stage: romcat_core::stage::Stage) {
        let (roots, site, board) = (&mut self.roots, &mut self.site, &mut self.board);
        roots.stages_mut().start(stage, site, board);
        // **扫描那一道由库屏自己排**（`stages::Section::start` 只留记号）：当场取走，按下去就排上。
        roots.take_scan(site, board);
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

    /// 浏览那一屏、它的库、**再加任务台**。按「刮削…」之后那一下三样都要：
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
            // **算要铺多少媒体那一趟先认**（`stages::Section::settle_media_cost`）：它整条只读，
            // 认领了不等于库变了，于是不走底下「转告浏览屏整页重读」那条路。
            if self.roots.stages_mut().settle_media_cost(&done) {
                continue;
            }
            // **体检那一趟也先认**（`roots::Screen::settle_health`）：同上，整条只读。
            let Some(done) = self.roots.settle_health(done) else {
                // **重新成型那一趟例外**（票 `gui-looks-like-the-design/29`）：它交回的是同一样
                // 东西（一份新报告），可它**把变体整批换过了**——浏览屏那几页与屏头那些数因此
                // 作废。这一句住在窗口里而不在库屏里，因为只有这儿够得着两屏（ADR-0005）。
                if self.roots.take_reshaped() {
                    self.browse.invalidate(&self.site);
                    self.recount();
                }
                continue;
            };
            // **各屏按任务号认领自己那一趟，不是它的就放过去。** 将来识别与刮削接上来
            // 时，各自在这儿多认一次。
            if self.roots.settle(&self.site, &done) {
                self.browse.invalidate(&self.site);
                self.recount();
                // **识别跑完了，待确认队列自己重新列过**：那一屏的每一批都是识别结论
                // 折出来的，不重列的话人得再点一次「重新列队列」——而那正是这一票要
                // 消掉的那种「还得记住下一步」。这一句住在窗口里而不在库屏里，
                // 因为**只有这儿同时够得着两屏**（ADR-0005，与 [`Self::route`] 同理）。
                if let Some(stage) = self.roots.stages_mut().take_ran() {
                    match stage {
                        romcat_core::stage::Stage::Identify => self.queue.reload(&self.site),
                        // **刮削跑完了，浏览屏重读一遍**：那一屏画的元数据那几栏正是它刚
                        // 写进去的——与刮削面板排的那一趟认领之后同一句（见下面）。
                        romcat_core::stage::Stage::Scrape => self.browse.refresh(&self.site),
                        // **折标题跑完了，浏览屏上的显示标题跟着更新**：那是这道工序
                        // 起没起作用唯一看得见的地方。走 `refresh` 而不是上面那句
                        // `invalidate`——显示标题画在**详情面板**上，而只有 `refresh`
                        // 连那一格一起重读（`browse::Screen::refresh`）。
                        romcat_core::stage::Stage::FoldTitles => {
                            self.browse.refresh(&self.site);
                        }
                        // **导出跑完了，哪一屏都不必重读**：它写出去的是主库根上那些
                        // 元数据文件，六屏一个都不画它们；库里被它动过的只有**底本**
                        // 与那个时刻戳，而工序段那一行已经由 `Section::settle` 自己
                        // 重问过了。这一支空着是**故意的**，不是漏了——照抄上面两支
                        // 随手 `reload` 一屏，等于每导一趟就白读一遍几万行。
                        romcat_core::stage::Stage::Export => {}
                        // **扫描与裁决两支从不经工序段排上任务台**，`take_ran` 交不出它们。
                        romcat_core::stage::Stage::Scan | romcat_core::stage::Stage::Triage => {}
                    }
                } else {
                    // **扫完一个根（或者取回一个数据源），待确认队列只重算屏头那个数**：
                    // 扫描不改变队列本身——新扫进来的变体连结论都还没有，进不了队列——
                    // 过期的只有「另有 N 个变体连识别都还没跑过」那一个数（挂单 `Q419`）。
                    // **不重列整份队列**：真库上那是一万八千条，人展开到哪一批也会被打回头一批。
                    // 取回数据源那一趟不改变这个数，重算一次是一句 `COUNT`，不值得为它另分一支。
                    self.queue.recount_not_run(&self.site);
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
    /// 「不改了」、或者干脆从左栏切回子库屏——那两条路上都没有「更新到子库」。
    /// 所以浏览屏每动一次某个子库的选择集就留一个记号（`take_touched`），
    /// 由这一趟转告子库屏把为那一台缓着的差量与容量账丢掉（`Screen::forget`）。
    /// 只认回程的话，子库屏会摆着一份按旧选择集排出来的差量，而「同步」认的正是它
    /// （ADR-0016）。
    ///
    /// **这一条还有反向的一条**（挂单 `Q812`）：子库屏也改得动例外了——「手动例外」那层弹层
    /// （票 `gui-looks-like-the-design/22`）与删减建议表上的「排除」。那时浏览屏「改选择」可能正开着
    /// 同一个子库，手上缓着的是改之前那几条，而它的详情面板上就写着「眼下：包含（…）」。
    /// 所以子库屏也留一个记号（`sublibrary::Screen::take_touched`），由这一趟转告浏览屏重读
    /// （`browse::Screen::exceptions_changed`）。**两条同形**：谁改了谁留记号，窗口负责转告。
    ///
    /// **还有一条与跳转无关、但同样只有这儿够得着两屏的**：待确认屏落下一批、撤回一批
    /// 之后，浏览屏缓着的那几行（置信度那一列画的正是裁决改的东西）也过期了。队列那一屏
    /// 留一个记号（`queue::Screen::take_changed`），由这一趟转告浏览屏——与库屏扫完一个根
    /// 走的是同一个入口（[`browse::Screen::invalidate`]），只是那一趟由任务台交回来，
    /// 这一趟就发生在本进程的这一帧里。
    ///
    /// **还有一条不是跳转的**：待确认队列屏与浏览屏空态上那几颗**捷径**（「运行识别」、
    /// 「折标题」）按下去只留一个记号，由这一趟取走、交给 [`Self::start_stage`]——
    /// 排一趟工序要同时够得着库屏工序段与任务台，而那两屏够不着库屏。
    ///
    /// 每帧一次。测试与实测拿它当那一下——**走的是界面上那条一模一样的路**。
    pub fn route(&mut self) {
        // **移掉一个根之后，别的屏也得跟着变**（票 `gui-looks-like-the-design/26` 验收第 4 条）。
        // 根那张表与工序那几行由库屏自己重读了（`roots::Screen::reload`），够不着的三样在这儿：
        // 左栏那个作品数、浏览屏缓着的那几行、待确认队列里指着那几个变体的批——
        // **只有窗口这一层同时够得着它们**（ADR-0005，同上面裁决那一支）。
        if self.roots.take_removed().is_some() {
            self.browse.invalidate(&self.site);
            self.recount();
            self.queue.reload(&self.site);
            // **每一台子库缓着的那份账也作废**：弹层上一句刚说「子库「X」的选择集会少 N 个
            // 变体」，缓着的选择集与差量预览说的却还是移除之前那一批——而「同步」认的正是
            // 那份差量（ADR-0016）。稿上按下去做的也是这一下（`S.subs.forEach(s=>{s.planned=false;})`）。
            // **一台都不漏**：规则里有没有引到这个根，是选择集求值才答得出的事。
            for name in self
                .sublibrary
                .list()
                .iter()
                .map(|one| one.name.clone())
                .collect::<Vec<_>>()
            {
                self.sublibrary.forget(&self.site, &name);
            }
        }
        if self.queue.take_changed() {
            self.browse.invalidate(&self.site);
            // 落下、撤回一批改的正是作品归属与裁决条数，左栏那两个数跟着重问。
            self.recount();
            // **库屏工序段跟着重问一遍**（票 `gui-looks-like-the-design/06`）：裁决那一行与待确认队列屏说的是
            // 同一个数（挂单 `Q822`），落下、撤回一批之后不重问，回到库屏看见的是裁之前的数，顶上
            // 「下一步」也还指着裁决。
            self.roots.stages_mut().reload(&self.site);
        }
        if let Some(jump) = self.sublibrary.take_jump() {
            self.browse
                .begin_editing(&self.site, &jump.sublibrary, jump.rule, jump.broken);
            // 规则行上「✎」跳过来的只改那一条（票 `gui-looks-like-the-design/20`）。
            if let Some(ordinal) = jump.ordinal {
                self.browse.edit_only(ordinal);
            }
            self.view = View::Browse;
        }
        // **先丢账再换屏**：回程那一下也会留下记号，丢在前面，`open` 重读到的就是新的。
        if let Some(name) = self.browse.take_touched() {
            self.sublibrary.forget(&self.site, &name);
        }
        // **反向的那一条**（挂单 `Q812`）：子库屏也改得动例外了——「手动例外」那层弹层
        // （票 `gui-looks-like-the-design/22`）与删减建议表上的「排除」。浏览屏「改选择」若正开着同一个子库，
        // 它手上缓着的是改之前那几条，而那一面板上就摆着「眼下：包含（…）」。
        if let Some(name) = self.sublibrary.take_touched() {
            self.browse.exceptions_changed(&self.site, &name);
        }
        if let Some(name) = self.browse.take_return() {
            self.sublibrary.reload(&self.site);
            self.sublibrary.open(&self.site, &name);
            self.view = View::Sublibraries;
        }
        // **各屏空态上那几颗捷径**（票 `gui-self-sufficient/09`）：按下去的那一屏排不了
        // 活——排一趟**工序**要同时够得着库屏那一段与**任务台**，而只有这儿够得着两边
        // （ADR-0005，与上面那几个跳转记号同理）。所以那几屏只留一个记号，这一趟取走、
        // 交给 [`Self::start_stage`]：**同一个函数、同一趟任务、同一份产物**，
        // 在任务台上与从库屏排的看不出区别。
        if let Some(stage) = self.queue.take_asked() {
            self.start_stage(stage);
        }
        if let Some(stage) = self.browse.take_asked() {
            self.start_stage(stage);
        }
        // **库屏工序段上裁决那一道**（票 `gui-looks-like-the-design/06`）：裁决不排任务，按下去换到
        // 待确认队列屏——库屏够不着那一屏，只留一个记号（`stages::Section::start`）。
        if self
            .roots
            .stages_mut()
            .take_handoff(romcat_core::stage::Stage::Triage)
        {
            self.view = View::Queue;
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
        self.shortcuts(ui.ctx());
        self.keys_sheet(ui.ctx());
        // **换到别的屏，子库屏删掉一台之后留着的那一份撤销就丢掉**（拿主意的人 2026-09-14 定）：提示条上
        // 那颗「撤销」只在子库屏摆着的时候按得着。
        if self.view != View::Sublibraries {
            self.sublibrary.leave();
        }
        self.rail(ui);
        // **底部状态栏**（设计稿 `.statusbar`，票 `gui-looks-like-the-design/25`）：每一屏都有。
        // 左边任务台那一小截与任务屏那张卡读同一份快照，点一下去任务屏；右边「ROM 只读」与
        // 工作目录。**左栏先声明、状态栏后声明**：左栏纵贯到底，状态栏只占右侧主区的底下
        // （设计稿 `.rail{grid-row:1/3}`）；也得赶在屏头与正文那几屏之前占好地方。
        let status_height = crate::tokens::Tokens::builtin().layout.statusbar;
        let go_to_tasks = egui::Panel::bottom("状态栏")
            .resizable(false)
            .exact_size(status_height)
            .show(ui, |ui| {
                self.tasks
                    .status_bar(ui, &self.board, &self.workspace_label)
            })
            .inner;
        if go_to_tasks {
            self.show_view(View::Tasks);
        }
        // 换屏发生在左栏里（点一个入口），标题跟着这一帧要看的那一屏改。
        self.retitle(ui.ctx());
        // **版式存不下来就说一句**：吞掉的话人只看见「拖了半天，下次全忘」，
        // 而真正的病在工作目录上（写不动的工作目录还会连累中立库与沉淀库）。
        // 摆在主区最上方单独一行、只在出错时有（拿主意的人 2026-09-14 定）。
        if let Some(说的) = self.layout.error() {
            let 说的 = format!("版式存不下来：{说的}");
            let [_, 左右] = crate::tokens::Tokens::builtin().space.screen_header_padding;
            egui::Panel::top("版式存不下来")
                .resizable(false)
                .frame(
                    egui::Frame::new()
                        .fill(ui.visuals().panel_fill)
                        .inner_margin(egui::Margin::from(egui::vec2(左右, look::step(1)))),
                )
                .show(ui, |ui| {
                    ui.colored_label(ui.visuals().warn_fg_color, 说的);
                });
        }
        // **屏头连屏体那一整块**：待确认屏的裁决记录抽屉贴着它的右沿、从屏头顶上一直到状态栏上沿
        // （设计稿 `.drawer` 摆在 `section.scr` 里）。左栏、状态栏已经占好了地方，剩下的就是它。
        let 主区 = ui.available_rect_before_wrap();
        // **作品详情页开着时不画浏览屏的屏头**：稿上 `.wd` 盖住整块屏，它顶上那一条（「← 返回浏览」）就是
        // 这一层的屏头（票 `gui-looks-like-the-design/15`）。
        if !(self.view == View::Browse && self.browse.page().is_some()) {
            let 副标题 = self.subtitle();
            look::screen_header(
                ui,
                egui::Id::new(("屏头", self.view)),
                self.view.nav_label(),
                &副标题,
                |ui| self.header_actions(ui),
            );
        }
        match self.view {
            View::Queue => {
                let (queue, site) = (&mut self.queue, &mut self.site);
                queue.ui(ui, site, 主区);
            }
            View::Library => {
                let (roots, site, board) = (&mut self.roots, &mut self.site, &mut self.board);
                roots.ui(ui, site, board);
            }
            View::Browse => {
                let (browse, site, board) = (&mut self.browse, &mut self.site, &mut self.board);
                browse.ui(ui, site, board);
                // **刮削面板里的优先级那一层刚保存了一份**：浏览屏手上缓着的那一份跟着换，
                // 整屏重读——详情面板上写着的显示值得是导出会写进去的那个。读这份表的别的
                // 几条路（刮削、整理标题、导出、同步）每一趟开头自己读，不用转告。
                if let Some(priorities) = browse.scrape_mut().priority_mut().take_saved() {
                    browse.set_priorities(priorities);
                    browse.refresh(site);
                }
                // **作品详情那一面刚落过一笔成型纠正**（票 `gui-looks-like-the-design/29`）：
                // 纠正只写沉淀库，中立库里的变体要重算一遍才跟着变。**全窗口只有库屏那一处排
                // 重新成型**，报告才不会两份各说各的。
                if browse.take_reshaped() {
                    let (roots, site, board) = (&mut self.roots, &mut self.site, &mut self.board);
                    roots.reshape(site, board);
                }
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
            View::Settings => {
                let facts = settings::Facts {
                    site: &self.site,
                    workspace_label: &self.workspace_label,
                    verdicts: self.verdicts,
                    open_last: settings::open_last(&self.layout),
                };
                self.settings.ui(ui, &facts);
                self.settle_settings();
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
        // **库屏那几块收着没有**：画完这一帧抄回版式；落盘照旧跟着底下那句，手松开了才写。
        for fold in layout::Fold::ALL {
            self.layout.set_folded(fold, self.roots.folded(fold));
        }
        self.browse.save_view_preferences(&mut self.layout);
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
        // 与开场那一态（`Program::ui`）走同一句：两态各装一遍的话，换进主窗口那一帧会再装一次。
        look::install_once(ctx);
        self.layout.seed(ctx);
        // **记着的那一档外观装回去**（设置屏「常规」那一排，[`settings::THEME_KEY`]）。
        // 没记过就一个字都不碰：egui 默认跟随系统，而截图那一路正靠这一条——
        // 临时工作目录里没有这份偏好，两张基线各自按自己要的那套主题画。
        if let Some(记着的) = self.layout.preference(settings::THEME_KEY)
            && let Some(挑的) = settings::theme_of(记着的)
        {
            ctx.set_theme(挑的);
        }
    }

    /// 窗口标题变了（换屏、打开或换掉作品详情页）就改掉。**变了才发**，不是每帧发一条。
    fn retitle(&mut self, ctx: &egui::Context) {
        let title = self.window_title();
        if self.titled.as_deref() == Some(title.as_str()) {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
        self.titled = Some(title);
    }

    /// **全窗口的快捷键，全在这一处**（票 `gui-looks-like-the-design/14`）。
    ///
    /// 摆在窗口这一层而不在某一屏里，理由与 [`Self::start_stage`] 同一条：**只有这儿够得着
    /// 「看的是哪一屏」**（ADR-0005：屏与屏之间不该互相拿着对方）。浏览屏那几下
    /// （`↑` `↓` / `Enter` / `空格` / `⌘ A` / `F` / `E`）因此也接在这儿，不在那一屏里另开一个
    /// 键盘入口——**一件事一个判据**（ADR-0024）：「这一下算不算快捷键」的那三道门只有一份。
    ///
    /// ## 三道门
    ///
    /// 前两道抄的是逐条那一处（`queue::Screen::keyboard`）：
    ///
    /// 1. **有一层弹层开着不接**（[`crate::dialog::screen_has_keys`]）——egui 的 `Modal`
    ///    拦得住指针、拦不住键盘。
    /// 2. **光标在文本框里不接**——那一栏里正打着中文，`F` 是用户要的字母不是命令。
    /// 3. **有一层浮层摊着不接**（[`egui::Popup::is_any_open`]）——右键菜单、下拉都算。
    ///    菜单摊着时 `Esc` 该收的是菜单，那一下归 egui 那一层收（`browse::menu`）；这里再接
    ///    一遍，就成了一下退两层。
    ///
    /// ## `Esc` 一层一层退
    ///
    /// 这里**一个 `Esc` 都不接**。那一下收的是**浮起来的那两层**，各自收各自的：
    /// 右键菜单由 `egui::Popup` 自己收（`browse::menu`），弹层由 [`crate::dialog`] 那一处
    /// `consume_key` 收。**一下只退一层**，因为上面那两层开着时这一处压根不跑（门 1 与门 3），
    /// 而两层叠着时 `dialog` 自己只退最上面那一层。
    ///
    /// 屏与屏之间**不归 `Esc` 管**：作品详情页回三栏走的是它顶上那颗「← 返回浏览」，
    /// 屏上那张表里 `Esc` 那一条写的也是「关闭对话框或菜单」（[`crate::keys`]）。
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if !crate::dialog::screen_has_keys(ctx)
            || egui::Popup::is_any_open(ctx)
            || ctx.egui_wants_keyboard_input()
        {
            return;
        }
        self.switch_view_keys(ctx);
        self.browse_keys(ctx);
    }

    /// 走到哪一屏、开哪一层：`⌘/Ctrl+1–6`、`⌘/Ctrl+,`、`⌘/Ctrl+F`、`?`。
    fn switch_view_keys(&mut self, ctx: &egui::Context) {
        const 数字: [egui::Key; 6] = [
            egui::Key::Num1,
            egui::Key::Num2,
            egui::Key::Num3,
            egui::Key::Num4,
            egui::Key::Num5,
            egui::Key::Num6,
        ];
        // **第几下就是左栏上从上往下第几个**（[`rail::order`]）：屏上摆着的次序与键上数的
        // 次序是同一份，不在这儿另列一遍。
        for (at, view) in rail::order().into_iter().enumerate() {
            let Some(key) = 数字.get(at).copied() else {
                break;
            };
            let 这一下 = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, key);
            if ctx.input_mut(|input| input.consume_shortcut(&这一下)) {
                // **先关掉作品详情页**（设计稿 `S.wd=false` 在 `go(...)` 之前）：它盖住整块屏，
                // 留着的话换回浏览屏时看见的还是它。
                self.browse.close_page();
                self.show_view(view);
            }
        }
        let 设置 = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Comma);
        if ctx.input_mut(|input| input.consume_shortcut(&设置)) {
            self.view = View::Settings;
        }
        // **搜索**：换到浏览屏、把筛选栏展开（收着时那一框连画都不画）、光标放进去。
        // 放进去那一下由浏览屏在画那一框的那一帧落实（`browse::Screen::focus_search`）。
        let 搜索 = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::F);
        if ctx.input_mut(|input| input.consume_shortcut(&搜索)) {
            // 详情页盖住整块屏，那一框在它底下——不关掉的话记号会一直悬着（设计稿 `S.wd=false`）。
            self.browse.close_page();
            self.view = View::Browse;
            layout::FILTER.set_collapsed(ctx, false);
            self.browse.focus_search();
        }
        // **`?` 看快捷键**：一层弹层，摆的是全仓那唯一一份表（[`crate::keys`]）。
        let 快捷键 = egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Questionmark);
        if ctx.input_mut(|input| input.consume_shortcut(&快捷键)) {
            self.keys_sheet = true;
        }
    }

    /// 浏览屏那几下：`↑` `↓` 挪高亮、`Enter` 打开、`空格` 勾选、`⌘/Ctrl+A` 全选、
    /// `F` 收藏、`E` 编辑元数据。
    ///
    /// **只在浏览屏、而且作品详情页没开着时才接**（设计稿 `S.screen!=='browse'||S.wd`）：
    /// 详情页盖住整块屏，那时 `F` 与 `E` 说的是另一件事。
    ///
    /// **摆着卡片墙时，跟高亮走的那六下一下都不接**（挂单 `Q1142`）：高亮是表格背后那扇窗的
    /// **行序号**，而卡片墙背后是另一扇窗——在卡上按空格会勾中人看不见的另一行，比什么都不
    /// 发生坏得多。卡片墙自己那条路照旧走得通：Tab 走到一张卡，`Enter` / `空格` 由那张卡
    /// 自己接（`browse::Screen::card_grid`）。设计稿拿
    /// `if(e.target.closest('.gcard'))return` 挡的是同一件事。
    ///
    /// **`⌘/Ctrl+A` 不在这道门里头**：全选的是**当前这个筛选**（ADR-0016），与光标落在哪一行
    /// 无关，两种视图上说的是同一件事。
    fn browse_keys(&mut self, ctx: &egui::Context) {
        if self.view != View::Browse || self.browse.page().is_some() {
            return;
        }
        let 全选 = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::A);
        if ctx.input_mut(|input| input.consume_shortcut(&全选)) {
            self.browse.select_all();
        }
        if self.browse.showing_cards() {
            return;
        }
        let (往上, 往下, 打开, 勾选, 收藏, 编辑) = (
            单键按下(ctx, egui::Key::ArrowUp),
            单键按下(ctx, egui::Key::ArrowDown),
            单键按下(ctx, egui::Key::Enter),
            单键按下(ctx, egui::Key::Space),
            单键按下(ctx, egui::Key::F),
            单键按下(ctx, egui::Key::E),
        );
        if 往上 {
            self.browse.step_focus(&self.site.catalog, false);
        }
        if 往下 {
            self.browse.step_focus(&self.site.catalog, true);
        }
        if 打开 {
            self.browse.open_focused(&self.site.catalog);
        }
        if 勾选 {
            self.browse.toggle_pick_focused(&self.site.catalog);
        }
        if 收藏 {
            self.browse
                .toggle_favorite_focused(&self.site, &mut self.board);
        }
        if 编辑 {
            self.browse.edit_focused(&self.site.catalog);
        }
    }

    /// **按 `?` 摊开的那层快捷键表**（设计稿 `DLG.keys`）。
    ///
    /// 摆的是全仓那唯一一份表，连**怎么画**也是那一处（[`crate::keys::table`]）——设置屏
    /// 「快捷键」那一节画的是同一个函数。照稿 `w:620`，也就是[默认那一档](crate::dialog::Width::Standard)。
    fn keys_sheet(&mut self, ctx: &egui::Context) {
        if !self.keys_sheet {
            return;
        }
        /// 页脚上按下去的那一颗。
        enum 按的 {
            知道了,
        }
        let footer = dialog::Footer::new(dialog::Button::new("知道了", 按的::知道了).primary())
            .dismiss_on_right();
        let shown = dialog::Dialog::new("快捷键", "快捷键", footer)
            .note(crate::keys::NOTE)
            .width(dialog::Width::Standard)
            .show(ctx, crate::keys::table);
        if shown.pressed.is_some() {
            self.keys_sheet = false;
        }
    }

    /// 设置屏这一帧交出来的那几样，各自落到该落的地方。
    ///
    /// **四样都不是设置屏自己办得了的**：改完名要换窗口标题与左栏那张卡（那两处在窗口上）、
    /// 换工作目录要整份退回开场（那件事在 [`Program`](crate::program::Program) 上）、
    /// 外观那一档记进版式文件（版式归窗口管）、优先级表保存后要换进浏览屏（够得着两屏的只有窗口）。
    fn settle_settings(&mut self) {
        if let Some(新名) = self.settings.take_renamed() {
            self.library_label = 新名;
        }
        if let Some(开着) = self.settings.take_open_last() {
            self.layout
                .set_preference(settings::OPEN_LAST_KEY, if 开着 { "是" } else { "否" });
        }
        if let Some(挑的) = self.settings.take_theme() {
            self.layout
                .set_preference(settings::THEME_KEY, settings::theme_label(挑的));
        }
        if let Some(去处) = self.settings.take_workspace() {
            self.switch_workspace = Some(去处);
        }
        // **与浏览屏那一支是同一件事**（`View::Browse` 那一臂）：优先级那一层在哪儿打开的
        // 都一样，保存之后浏览屏手上缓着的那一份要跟着换（挂单 `Q782`）。
        if let Some(priorities) = self.settings.priority_mut().take_saved() {
            self.browse.set_priorities(priorities);
            self.browse.refresh(&self.site);
        }
    }

    /// 画左栏：几个计数交进去，按下去的那一下在这儿落实（[`rail`]）。
    fn rail(&mut self, ui: &mut egui::Ui) {
        // **窗口宽度一变，临时展开就作废**：人拖窗口是在换版式，回到自动收起的规则。
        let 窗口宽 = ui.ctx().content_rect().width();
        if self.rail_peek.is_some_and(|点的时候| 点的时候 != 窗口宽) {
            self.rail_peek = None;
        }
        let queue = self.queue.queue();
        let 待裁 = if queue.identified() {
            thousands(queue.pending())
        } else {
            "—".to_owned()
        };
        let 根 = format!("{} 个根", self.roots.roots().len());
        // **一个根都还没扫过、作品数又是 0，画「—」**（设计稿 `!S.scanDone?'—'`）：那个 0 说的是「还不知道」，
        // 不是「这份库有 0 个作品」。扫没扫过取库屏手上那几个根自己记着的（`RootRow::root.scan`）。
        let 扫过 = self.roots.roots().iter().any(|row| row.root.scan.is_some());
        let 作品 = match self.works {
            Some(0) if !扫过 => "—".to_owned(),
            Some(数) => thousands(数),
            None => "—".to_owned(),
        };
        let 子库 = self.sublibrary.list().len().to_string();
        // **哪一屏上都看得见台上有活在跑**：跑着的时候人多半正在别的屏上。数的是跑着的加排着的。
        let 任务 = self
            .board
            .running()
            .map(|_| (1 + self.board.queued().len()).to_string());
        let badge = |view: View| match view {
            View::Library => rail::Badge::Count(根.clone()),
            View::Queue => rail::Badge::Count(待裁.clone()),
            View::Browse => rail::Badge::Count(作品.clone()),
            View::Sublibraries => rail::Badge::Count(子库.clone()),
            View::Tasks => 任务.clone().map_or(rail::Badge::Nothing, rail::Badge::Live),
            // **设置不带计数**：这一屏里没有哪个数答得上「还差多少」（设计稿那一颗也没有徽标）。
            View::Settings => rail::Badge::Nothing,
        };
        let facts = rail::Facts {
            library: &self.library_label,
            current: self.view,
            badge: &badge,
            verdicts: self.verdicts,
            chosen_collapsed: self.layout.rail_collapsed(),
            peeking: self.rail_peek.is_some(),
        };
        match rail::show(ui, &facts) {
            Some(rail::Pressed::Go(view)) => self.view = view,
            // **回开场换一份库的那条路**（票 `gui-self-sufficient/04` 验收第 5 条）。
            //
            // **它不是又一屏**，所以不长成入口里的一项：开场是六屏之外的那一屏，它交出现场之后
            // 自己退场（词表**开场**那一条）。一按，这份 `App` 连同它手上那份现场整个退场，不必关窗重开。
            //
            // 这句话从前写的是「它不是第六屏」——第六屏自票 `gui-looks-like-the-design/31` 起
            // 真的有了，就是**设置**（[`View::Settings`]），所以换了说法，说的仍是同一件事。
            Some(rail::Pressed::SwitchLibrary) => self.switching = true,
            Some(rail::Pressed::Collapse(collapsed)) => self.layout.set_rail_collapsed(collapsed),
            Some(rail::Pressed::Peek(peeking)) => self.rail_peek = peeking.then_some(窗口宽),
            None => {}
        }
    }

    /// 屏头上的副标题（设计稿各屏 `.scrhead .sub` 那一句）。
    fn subtitle(&self) -> String {
        match self.view {
            View::Library => "根目录、数据源和处理进度".to_owned(),
            View::Queue => {
                let queue = self.queue.queue();
                if queue.identified() && queue.pending() > 0 {
                    format!("{} 个变体待确认", thousands(queue.pending()))
                } else {
                    "暂无待确认项".to_owned()
                }
            }
            View::Browse => "查找作品，并对选中的内容进行操作".to_owned(),
            View::Sublibraries => "选择集在「浏览」中编辑，这里负责同步到各台设备".to_owned(),
            View::Tasks => "查看正在运行、等待中和已完成的任务".to_owned(),
            View::Settings => settings::SUBTITLE.to_owned(),
        }
    }

    /// 屏头右侧：那一屏原来画在顶栏上的那一段，**原样调进来**（照稿重排由各屏自己的票接手）。
    fn header_actions(&mut self, ui: &mut egui::Ui) {
        match self.view {
            View::Queue => {
                // 照稿重排过（票 `gui-looks-like-the-design/18`）：「沉淀库 在哪」挪进了裁决记录那块抽屉的页脚。
                let (queue, site) = (&mut self.queue, &self.site);
                queue.status(ui, site);
            }
            View::Library => {
                // 库屏照稿重排过（票 `gui-looks-like-the-design/06` 接手挂单 `Q866` / `Q868`）：右侧是稿上那颗「添加根…」，
                // 原来顶栏上那一句不要了。
                let (roots, site) = (&mut self.roots, &self.site);
                roots.header_actions(ui, site);
            }
            View::Browse => {
                // **浏览屏的屏头右边，照稿只剩开发用的那一个开关**
                // （拿主意的人 2026-09-22 对着稿裁的，挂单 `Q1102`）：
                // 稿上 `#s-browse .scrhead` 里只有标题、那句副标题，以及两样**默认不画**的
                // 东西（「正在编辑子库…」与「更新子库」，归票 `23`）。
                // 那几颗批量按钮稿上从来就在**表格上方那一条**的右端（`.tbar .acts`），
                // 票 09 把它们整组挪到了屏头（挂单 `Q876`／`Q877`），这一票挪回去。
                self.browse.status(ui, self.demo);
            }
            View::Sublibraries => {
                let (sublibrary, site) = (&mut self.sublibrary, &self.site);
                sublibrary.status(ui, site);
            }
            View::Tasks => {
                // 照稿只有一颗「清空历史」，按得动，所以任务台拿的是可变的那一份。
                let (tasks, board) = (&mut self.tasks, &mut self.board);
                tasks.status(ui, board);
            }
            // **设置屏的屏头右边是空的**（设计稿 `#s-set .scrhead` 上只有标题与那句副标题）：
            // 这一屏上每个动作都贴着它管的那一格，抬到屏头上反而说不清它动的是哪一节。
            View::Settings => {}
        }
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

/// 一条路径排成给人看的样子：落在 `HOME` 底下的缩成 `~/…`，别处照原样。
///
/// **摆在模块上而不在 `App` 里**：底部状态栏那一段工作目录走它（[`App::new`]），设置屏
/// 「导出目录」那一格也走它（`crate::settings`）——两处各写一份，同一条路径在相邻两格里
/// 就会一个带 `~`、一个不带。
pub(crate) fn shorten_home(path: &std::path::Path) -> String {
    std::env::var_os("HOME")
        .and_then(|home| {
            path.strip_prefix(&home)
                .ok()
                .map(|rest| std::path::Path::new("~").join(rest))
        })
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
}
