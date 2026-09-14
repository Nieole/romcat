//! **开场**：五屏之外的那一屏。
//!
//! 挑一份**现场**开进去，或者添加一个主库。它交出现场之后自己退场——五屏都建立在
//! 「库一定在」这个前提上，开场就是守住那个前提的地方（`CONTEXT.md` 词表、ADR-0023）。
//!
//! ## 这一屏一条领域判断都没有
//!
//! 列一个**工作目录**里有哪些**中立库**、比对结构版本、读出**主库原名**，三样全在核心库
//! （[`workspace::catalogs`]）。这儿只把要来的画出来（ADR-0005 的红线）。
//! **结构版本对不上的那一份照列不误**，而那句「版本 X，本程序认得的是 Y」也是核心库
//! 交出来的原话，界面一个字都不改写——两处各写一套措辞，人在终端里看见的与在界面上
//! 看见的迟早分家。
//!
//! ## 那三个数为什么盘没挂上也在
//!
//! **主库原名**、变体数、上次扫描时刻全住在**中立库**里，不在主库上（ADR-0009）。外置盘
//! 不常挂载（ADR-0018 的双机工作流），而开场正是盘不在位时最常看见的那一屏。
//!
//! ## 版式
//!
//! 照设计稿（`.scratch/gui-looks-like-the-design/prototype.html` 的 `#opening`）：**左栏**一句话
//! 讲清这个工具做什么，外加三条承诺；**右栏**是工作目录，底下按三态画——有库一份一行、空的一张
//! 卡片只摆一个主要操作（添加主库）、读不动只画核心库那句原话。添加主库那条向导是一层弹层，
//! **盖在这一屏上**，底下两栏照常画（挂单 `Q667`：遮罩挡着点击）。尺寸、间距、颜色全取令牌。

use std::path::{Path, PathBuf};

use romcat_core::path;
use romcat_core::report::{human_time, thousands};
use romcat_core::site::Site;
use romcat_core::workspace::{self, CatalogEntry, CatalogState, Listing};

use crate::claim;
use crate::font;
use crate::look::{self, Tone};
use crate::tokens::Tokens;

/// 这个工作目录里一份中立库都没有时说的那句话。
///
/// **它指向下一步**，不是一句「空」：添加一个主库才是这时候唯一做得下去的事，
/// 而它指的就是它底下那颗「添加主库」——[那条向导](crate::claim)在那儿起步。
pub const NO_CATALOG: &str = "还没有库，添加一个主库开始";

/// 换工作目录那个框里的提示字。
const WORKSPACE_HINT: &str = "换一个工作目录：把路径贴在这儿";

/// 左栏那句大标题：这个工具做什么。
const HEADLINE: &str = "管理你的模拟器游戏库";

/// 大标题底下那段导语。
const LEAD: &str = "自动识别 ROM、刮削元数据，并导出到 Pegasus、ES-DE 等前端。\
                    所有结果保存在工作目录中，ROM 文件始终保持原样。";

/// 三条承诺：字块上那个字、标题、说明。字句照设计稿。**每一条都得是真话**，依据写在旁边。
const PROMISES: [(&str, &str, &str); 3] = [
    // ADR-0004：主库只读，一切磁盘接触走只读接缝。
    (
        "读",
        "只读访问 ROM",
        "扫描、识别和导出都不会修改、移动或重命名任何 ROM 文件。",
    ),
    // ADR-0007：联网源默认不勾；识别走本地的 DAT 库与中文离线源。
    (
        "离",
        "默认离线运行",
        "识别和刮削优先使用本地数据源，不产生网络请求。",
    ),
    // 每条候选带着置信度与依据；拿不准的进待确认队列，按批裁决。
    (
        "核",
        "不确定的结果由你确认",
        "每条识别结果都附带置信度和依据；无法自动确定的进入待确认队列，可批量处理。",
    ),
];

/// 开场交出来的东西：一份开好的**现场**，连它住在哪个**工作目录**里。
///
/// 两样一起交，是因为 [`App::new`](crate::app::App::new) 两样都要，而工作目录**不能
/// 由拿到它的人再算一遍**——开场上可以换工作目录，再算一遍算出来的会是换之前那个。
pub struct Chosen {
    /// 开好的现场：中立库、沉淀库、这份主库的**主库标识**。
    pub site: Site,
    /// 它住在哪个工作目录里。
    pub workspace: PathBuf,
    /// **刚添加出来的那一份还欠着的那一下**：把第一个根加上，再把第一趟扫描排上
    /// **任务台**。挑中一份现成的库时它是 `None`——那一份的根早就在库里了。
    ///
    /// **为什么欠到进主窗口之后才还**：加根与排扫描这两下的现成入口都长在库屏上
    /// （[`crate::roots::Screen`]），而库屏是主窗口的一部分，向导手上没有。所以向导
    /// 只把这两串字捎过去，由 [`Program`](crate::program::Program) 在换成主窗口之后
    /// 交给库屏——**不另造一条加根的路，也不另造一条排扫描的路**（ADR-0005）。
    pub first_root: Option<claim::FirstRoot>,
}

/// 开场那一屏。
pub struct Screen {
    /// 眼下在看哪个**工作目录**。
    workspace: PathBuf,
    /// 那个目录里的中立库，**列一次记下来**。
    ///
    /// 不每帧现列：列一遍要把目录里每一份库都开一趟（[`workspace::catalogs`]），
    /// 那是几次到几十次磁盘往返，一秒钟做六十遍是拿开场那一屏当磁盘压力测试。
    /// 换了工作目录才重列——而那正是列表内容会变的唯一时机。
    catalogs: Listing,
    /// 换工作目录那个框里正打着的字。
    draft: String,
    /// 上一次开库没开成时说的那句话。
    ///
    /// 与「这一份读不开」不是一回事：那一条跟着行走（[`workspace::CatalogEntry`]），
    /// 这一条是**按下「打开」之后**才知道的（文件在列出来之后被挪走了、被别人占着）。
    error: Option<String>,
    /// **退回这一屏的原因**：上次开的那份打不开了，这是那句原话。
    ///
    /// 与[按下「打开」之后那一句](Self::error)分开摆，因为它们**知道的时机**不同：
    /// 这一条在这一屏画出来之前就已经知道了，画在右栏**最上头**人第一眼就看得见；那一条
    /// 要等人按下去才知道，画在上头的话人得多等一帧才看见它。
    fallback_reason: Option<String>,
    /// 正在走[添加主库那条向导](crate::claim)时，那条向导。
    ///
    /// **它是盖在这一屏上的一层弹层**：底下那两栏照常画，遮罩挡着点击（挂单 `Q667`）。从前走向导时
    /// 那张表与换目录那一行收起来，理由是「在添加一个新的与开那一份之间犹豫」「走到一半换了目录」
    /// ——这两样遮罩都已经替着挡了。
    ///
    /// **装箱**：开场这一屏本身几乎不占地方，而
    /// [`Program`](crate::program::Program) 那两态正是照着这一条摆的（那个枚举里
    /// 「开场中」不装箱、「已开库」装箱）。把攒着三串字的向导直接嵌进来，这一屏就翻了
    /// 一倍——而它只在人真按下「添加主库」之后才存在。
    claiming: Option<Box<claim::Wizard>>,
}

impl Screen {
    /// 进开场：把这个工作目录里的库列出来。
    #[must_use]
    pub fn new(workspace: PathBuf) -> Self {
        let catalogs = workspace::catalogs(&workspace);
        Self::listed(workspace, catalogs)
    }

    /// 进开场，**列出来的那几份由调用方交进来**，这一下不碰盘。
    ///
    /// **截图门要它**（`tests/snapshot.rs`）：基线里画着工作目录的路径、每一份库的主库原名与
    /// 上次扫描时刻，从真盘上列的话，临时目录在每台机器、每一趟上都是另一串，像素跟着变。
    /// 交进来的仍是核心库那个类型（[`Listing`]），画法与真盘上列出来的走同一条路；
    /// 换工作目录（[`Self::look_at`]）照旧去盘上列。
    #[must_use]
    pub fn listed(workspace: PathBuf, catalogs: Listing) -> Self {
        Self {
            workspace,
            catalogs,
            draft: String::new(),
            error: None,
            fallback_reason: None,
            claiming: None,
        }
    }

    /// 摆一句「**为什么你会看见这一屏**」在右栏最上头。
    ///
    /// [`Program`](crate::program::Program) 拿它交进来那句核心库的原话：上次开的那份
    /// 打不开时，人期待的是直接进主窗口，突然看见开场就得当场说得出是哪一条原因——
    /// 挪走了、删了、还是结构版本对不上（票 `gui-self-sufficient/04` 验收第 4 条）。
    pub fn explain(&mut self, 说的: String) {
        self.fallback_reason = Some(说的);
    }

    /// 换一个工作目录，**立刻**列出那个目录里的库。
    ///
    /// 不换的话，那些不在默认位置的库一份都开不出来——默认工作目录那条链
    /// （[`workspace::default_dir`]）认的是环境变量，而盘在两台机器之间来回接时
    /// 它常常不是人想开的那一个。
    pub fn look_at(&mut self, workspace: PathBuf) {
        self.workspace = workspace;
        self.error = None;
        // **换了目录那句话就作废**：它说的是上次开的那份在**原来那个**目录里出了什么事。
        self.fallback_reason = None;
        // **攒到一半那条向导也作废**：它添加进的是**原来那个**目录，连起名那一问查的重名都是
        // 在那儿查的。（向导开着时遮罩挡着换目录那一行，屏上走不到这条；留着是因为这个方法是
        // 公开的，谁都能在向导开着的时候换一个目录进来。）
        self.claiming = None;
        self.catalogs = workspace::catalogs(&self.workspace);
    }

    /// 画一帧。挑中了一份并且开得出来时，交出那份**现场**——开场到此退场。
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<Chosen> {
        let tokens = Tokens::builtin();
        let (窗口底, 次级底) = (ui.visuals().panel_fill, ui.visuals().faint_bg_color);
        // 右栏取令牌那一段宽度，左栏至少留令牌那一截；窗口再窄时右栏收到最窄那一档。
        let [最窄, 最宽] = tokens.layout.opening_side_width;
        let 右栏宽 = (ui.available_width() - tokens.layout.opening_hero_min).clamp(最窄, 最宽);
        let [side_y, side_x] = tokens.space.opening_side_padding;
        let [hero_y, hero_x] = tokens.space.opening_hero_padding;

        // 右栏先画：两栏之间那条线是它的边（设计稿 `.op-hero` 的右边框）。
        let 开出来的 = egui::Panel::right("开场右栏")
            .resizable(false)
            .exact_size(右栏宽)
            .frame(
                egui::Frame::new()
                    .fill(窗口底)
                    .inner_margin(egui::Margin::from(egui::vec2(side_x, side_y))),
            )
            .show(ui, |ui| self.side_ui(ui))
            .inner;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(次级底)
                    .inner_margin(egui::Margin::from(egui::vec2(hero_x, hero_y))),
            )
            .show(ui, hero_ui);

        // **向导盖在这一屏上**（挂单 `Q667`）：底下两栏照常画完了，弹层画在它们上头。
        let 添加出来的 = self.claiming_ui(ui);
        开出来的.or(添加出来的)
    }

    /// 右栏：退回这一屏的原因（有的话）、工作目录、三态之一。
    fn side_ui(&mut self, ui: &mut egui::Ui) -> Option<Chosen> {
        egui::ScrollArea::vertical()
            .id_salt("开场右栏")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let 间隔 = look::step(3);
                // **它画在最上头**：这一句回答的是「我明明开过库了，怎么又看见这一屏」，
                // 那个问题在人找那份库之前就问出口了。
                if let Some(说的) = &self.fallback_reason {
                    let 提醒 = ui.visuals().warn_fg_color;
                    look::note(ui, egui::RichText::new(说的).small().color(提醒));
                    ui.add_space(间隔);
                }
                self.workspace_ui(ui);
                ui.add_space(间隔);
                let 要开的 = self.catalogs_ui(ui);
                let 开出来的 = 要开的.and_then(|catalog| self.open(&catalog));
                // **那句话画在表底下**，不是画在上头：它是**按下「打开」之后**才知道的，
                // 而这一帧的按下发生在表画完的那一刻——画在上头的话，人要等下一帧才看见它。
                if let Some(error) = &self.error {
                    ui.add_space(look::step(1));
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                开出来的
            })
            .inner
    }

    /// 工作目录那一栏：现在看的是哪个目录（换一个：弹选择器，或者把路径贴进底下那个框）。
    ///
    /// **认错了目录，建出来的库在别处**：所以它排在右栏最前头，在「添加主库」之前。
    fn workspace_ui(&mut self, ui: &mut egui::Ui) {
        let tokens = Tokens::builtin();
        let (面板底, 描边) = (
            ui.visuals().window_fill,
            ui.visuals().widgets.noninteractive.bg_stroke,
        );
        look::section(ui, "工作目录");
        ui.add_space(look::step(0));
        egui::Frame::new()
            .fill(面板底)
            .stroke(描边)
            .corner_radius(tokens.radius.medium)
            .inner_margin(egui::Margin::from(egui::vec2(look::step(2), look::step(1))))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    // 「更改…」是小号按钮（设计稿 `.btn.sm`）：量宽也按小号量。
                    let 留给按钮 =
                        look::small_button_width(ui, "更改…") + ui.spacing().item_spacing.x;
                    let 路径宽 = (ui.available_width() - 留给按钮).max(0.0);
                    ui.allocate_ui(egui::vec2(路径宽, ui.spacing().interact_size.y), |ui| {
                        // 占满留给路径的那一截：不占满的话「更改…」紧贴着路径摆，而不是靠右。
                        ui.set_min_width(路径宽);
                        ui.add(
                            egui::Label::new(font::mono(path::display(&self.workspace))).truncate(),
                        );
                    });
                    // 系统目录选择器（[`crate::pick`]）：选中之后走的是底下「换过去」那一条路。
                    let 更改 = look::small_buttons(ui, |ui| {
                        ui.button("更改…").on_hover_text(crate::pick::FALLBACK_HINT)
                    });
                    if 更改.clicked() {
                        self.picked(crate::pick::directory("换一个工作目录", &self.workspace));
                    }
                });
            });
        ui.add_space(look::step(0));
        // 贴路径那一行：弹不出选择窗口时（远程会话）的退路，常驻着。「换过去」与「更改…」同一档小号。
        ui.horizontal(|ui| {
            let 留给按钮 = look::small_button_width(ui, "换过去") + ui.spacing().item_spacing.x;
            let 框宽 = (ui.available_width() - 留给按钮).max(0.0);
            // 旁边是小号按钮「换过去」，这一框也是小号那一档（与它等高）；宽占满留给它的那一截。
            look::small_text_input(
                ui,
                框宽,
                egui::TextEdit::singleline(&mut self.draft).hint_text(WORKSPACE_HINT),
            );
            // 空着按下去什么都不该发生：那不是「换到根目录」，是还没打完字。
            let 打了字 = !self.draft.trim().is_empty();
            if look::small_buttons(ui, |ui| ui.add_enabled(打了字, egui::Button::new("换过去")))
                .clicked()
            {
                // **与目录选择器交回来的那一下走同一处**（挂单 `Q696`）：往后谁给换目录多加一步，
                // 两颗按钮一起跟上。
                let 换到 = PathBuf::from(self.draft.trim());
                self.picked(Some(换到));
            }
        });
        look::help(
            ui,
            "中立库、沉淀库、DAT 库与媒体池都住在这里，一个字节都不写进主库。",
        );
    }

    /// [添加主库那条向导](crate::claim)那一层。走完了就跟挑中一份一样：交出现场，
    /// 开场到此退场。
    fn claiming_ui(&mut self, ui: &mut egui::Ui) -> Option<Chosen> {
        let wizard = self.claiming.as_mut()?;
        match wizard.show(ui.ctx()) {
            claim::Outcome::Going => None,
            // **回开场**：攒着的那三串字连同那条向导一起丢掉，磁盘上本来就什么都没有。
            claim::Outcome::Dropped => {
                self.claiming = None;
                None
            }
            claim::Outcome::Done(添加的) => {
                self.claiming = None;
                let claim::Claimed { site, root } = *添加的;
                Some(Chosen {
                    site,
                    workspace: self.workspace.clone(),
                    first_root: Some(root),
                })
            }
        }
    }

    /// 工作目录底下那一块，**三态各画各的**（[`Listing`]）。挑中了哪一份就交出它的文件路径。
    ///
    /// 读不动与空的在屏上说同一句话的话，人会照着「还没有库」去建第二份库——而该做的是去修那个
    /// 目录的权限（ADR-0021 那条修订）。
    fn catalogs_ui(&mut self, ui: &mut egui::Ui) -> Option<PathBuf> {
        let mut 要添加 = false;
        let mut 要开的 = None;
        match &self.catalogs {
            Listing::Empty => 要添加 = empty_card(ui),
            Listing::Unreadable(为什么) => unreadable_card(ui, &为什么.to_string()),
            Listing::Catalogs(那几份) => (要添加, 要开的) = list_ui(ui, 那几份),
        }
        // 向导开着时遮罩挡着这一颗；万一有一下漏过来，也不把人攒到一半的那三串字抹掉。
        if 要添加 && self.claiming.is_none() {
            self.claiming = Some(Box::new(claim::Wizard::new(self.workspace.clone())));
        }
        要开的
    }

    /// 开这一份。开不出来就把那句话摆到屏上，**留在开场**。
    fn open(&mut self, catalog: &Path) -> Option<Chosen> {
        match Site::open_file(&self.workspace, catalog, None) {
            Ok(site) => {
                self.error = None;
                Some(Chosen {
                    site,
                    workspace: self.workspace.clone(),
                    first_root: None,
                })
            }
            Err(error) => {
                self.error = Some(format!("{error}"));
                None
            }
        }
    }
}

impl Screen {
    /// **目录选择器交回来的那一下**（[`crate::pick`]），也是「换过去」那一下（挂单 `Q696`）：
    /// 选中了一个目录就换过去，取消了（`None`）什么都不动——框里打着的字、眼下看的工作目录，
    /// 一个字都不变。
    ///
    /// 换过去是两下——框里的字清掉、[`Self::look_at`]。清框是因为人改用选择器之后，框里那半截字
    /// 说的已经是换过去之前的打算，留着它，下一下「换过去」会把人带回一个他已经放弃的目录。
    ///
    /// 这一半与弹对话框那一层分开摆，是为了**它测得到**：对话框是系统的模态窗口，
    /// 测试驱动不了；拿到路径之后发生什么，测试递一个进来就验得着。
    pub fn picked(&mut self, 选中的: Option<PathBuf>) {
        let Some(换到) = 选中的 else {
            return;
        };
        self.draft.clear();
        self.look_at(换到);
    }
}

/// 左栏：标志、一句话讲清这个工具做什么、导语、三条承诺。
fn hero_ui(ui: &mut egui::Ui) {
    let tokens = Tokens::builtin();
    let 间隔 = look::step(4);
    let 标志 = tokens.layout.mark;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(标志, 标志), egui::Sense::hover());
    look::mark(ui.painter(), rect, tokens.radius.large, ui.visuals());
    ui.add_space(间隔);
    let 强调 = ui.visuals().strong_text_color();
    ui.label(
        egui::RichText::new(HEADLINE)
            .text_style(egui::TextStyle::Name(look::HERO.into()))
            .color(强调),
    );
    ui.add_space(间隔);
    ui.label(LEAD);
    ui.add_space(间隔);
    for (字, 题, 说) in PROMISES {
        promise_ui(ui, 字, 题, 说);
        ui.add_space(look::step(2));
    }
}

/// 一条承诺：左边一枚字块，右边标题与说明（设计稿 `.promise`）。
fn promise_ui(ui: &mut egui::Ui, 字: &str, 题: &str, 说: &str) {
    let tokens = Tokens::builtin();
    let size = tokens.layout.promise_icon;
    ui.horizontal_top(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
        let (面板底, 描边, 字色) = (
            ui.visuals().window_fill,
            ui.visuals().widgets.noninteractive.bg_stroke,
            ui.visuals().hyperlink_color,
        );
        ui.painter().rect(
            rect,
            tokens.radius.medium,
            面板底,
            描边,
            egui::StrokeKind::Inside,
        );
        let galley = egui::WidgetText::from(egui::RichText::new(字).color(字色)).into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Body,
        );
        let 摆在 = rect.center() - galley.size() / 2.0;
        ui.painter().galley(摆在, galley, 字色);
        ui.add_space(look::step(2));
        ui.vertical(|ui| {
            ui.label(font::strong(题));
            ui.label(egui::RichText::new(说).small().weak());
        });
    });
}

/// **空的**：一张卡片，只摆一个主要操作；底下一条提示，说命令行扫过的库怎么在这儿找回来。
/// 交回按没按下「添加主库」。
fn empty_card(ui: &mut egui::Ui) -> bool {
    let 强调 = ui.visuals().strong_text_color();
    let mut 按了 = false;
    look::card(ui, egui::Vec2::splat(look::step(4)), |ui| {
        ui.label(
            egui::RichText::new(NO_CATALOG)
                .text_style(egui::TextStyle::Name(look::TITLE.into()))
                .color(强调),
        );
        ui.label(
            egui::RichText::new("走完三步就开始扫描。扫描在任务台上跑，期间可以照常用别的功能。")
                .weak(),
        );
        ui.add_space(look::step(2));
        // 大号主按钮（设计稿 `.btn.pri.lg`）：高与左右留白取令牌那一档。
        按了 = look::large_buttons(ui, |ui| {
            look::primary_button(ui.visuals_mut());
            ui.button("添加主库")
        })
        .clicked();
    });
    ui.add_space(look::step(2));
    look::note(
        ui,
        egui::RichText::new(
            "已经在其他电脑上用命令行扫描过？把工作目录指向那份数据所在的位置，这里就会列出来。",
        )
        .small(),
    );
    按了
}

/// **读不动**：一张卡片装着核心库那句原话（哪个目录、系统怎么说的、下一步去看它的权限）。
///
/// **不摆「添加主库」**：那句原话说的正是「别急着再建一份」，而这时候按下去，起名那一问就会撞上
/// 同一个列不开的目录——列不开就查不了重名，核心库不收（挂单 `Q612`）。
fn unreadable_card(ui: &mut egui::Ui, 说的: &str) {
    let 出错 = ui.visuals().error_fg_color;
    look::card(ui, egui::Vec2::splat(look::step(4)), |ui| {
        ui.label(egui::RichText::new(说的).color(出错));
    });
}

/// **有库**：抬头写着几份、一颗小号的「添加主库」；一张卡片里一份一行；卡片底下一行帮助字。
/// 交回（按没按下「添加主库」，要开哪一份）。
fn list_ui(ui: &mut egui::Ui, 那几份: &[CatalogEntry]) -> (bool, Option<PathBuf>) {
    let mut 要添加 = false;
    let mut 要开的 = None;
    ui.horizontal(|ui| {
        look::section(ui, &format!("主库 · {} 个", 那几份.len()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // 小号按钮（设计稿 `.btn.sm`）。
            要添加 = look::small_buttons(ui, |ui| ui.button("添加主库")).clicked();
        });
    });
    ui.add_space(look::step(1));
    // 头一份开得进去的那一份的「打开」画成主按钮：列表是核心库排好的（从没扫过的在前，其余按
    // 上次扫描倒排），排在最前的那一份多半就是人要开的。
    let 主按钮在 = 那几份.iter().position(|一份| 一份.state.openable());
    look::card(ui, egui::Vec2::ZERO, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (at, 一份) in 那几份.iter().enumerate() {
            if at > 0 {
                look::divider(ui);
            }
            if row_ui(ui, 一份, 主按钮在 == Some(at)) {
                要开的 = Some(一份.path.clone());
            }
        }
    });
    ui.add_space(look::step(1));
    look::help(
        ui,
        "下次启动时直接打开上次开的那一份。只有它打不开、或者你主动换一份库时，才会看见这一屏。",
    );
    (要添加, 要开的)
}

/// 一份库一行：左边名字（开不进去的带一枚标签）与那几个数或那句原话，右边「打开」。
/// 交回按没按下「打开」。
fn row_ui(ui: &mut egui::Ui, 一份: &CatalogEntry, 主按钮: bool) -> bool {
    let tokens = Tokens::builtin();
    let (正文间距, 提醒) = (ui.spacing().item_spacing, ui.visuals().warn_fg_color);
    // 行内边距取令牌（设计稿 `.catrow` 的 padding；拿主意的人 2026-09-14 第三次裁：照稿收紧）。
    let [行上下, 行左右] = tokens.space.catalog_row_padding;
    let 这一行 = egui::Frame::new()
        .inner_margin(egui::Margin::from(egui::vec2(行左右, 行上下)))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = 正文间距;
            ui.set_width(ui.available_width());
            // 顶端对齐：居中的一行里，左边那一块会从这一行的半腰画起，上头平白多出半行空。
            ui.horizontal_top(|ui| {
                // 「打开」是小号按钮（设计稿 `.btn.sm`）：量宽也按小号量。
                let 留给按钮 = look::small_button_width(ui, "打开") + ui.spacing().item_spacing.x;
                let 左边宽 = (ui.available_width() - 留给按钮).max(0.0);
                let 左边 = ui.allocate_ui_with_layout(
                    egui::vec2(左边宽, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        // 占满左边那一截：不占满的话「打开」紧贴着字摆，而不是靠右。
                        ui.set_min_width(左边宽);
                        // **这一块里只有字，没有控件**：名字与底下那句之间只隔令牌那一格（设计稿
                        // `.catrow` 的 gap），名字那一横排也不按按钮高撑——egui 的横排一行最矮就是
                        // 按钮那么高，照那个撑的话一行多出小半行空。「打开」在右边自己居中。
                        ui.spacing_mut().item_spacing.y = tokens.space.catalog_row_gap;
                        ui.spacing_mut().interact_size.y = 0.0;
                        ui.horizontal(|ui| {
                            ui.label(font::strong(&一份.name));
                            match &一份.state {
                                CatalogState::SchemaMismatch { .. } => {
                                    look::chip(ui, Tone::Caution, "版本不兼容");
                                }
                                CatalogState::Broken { .. } => {
                                    look::chip(ui, Tone::Caution, "打不开");
                                }
                                CatalogState::Openable(_) => {}
                            }
                        });
                        match &一份.state {
                            CatalogState::Openable(Ok(facts)) => {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} 个变体 · {}",
                                        thousands(facts.variants),
                                        facts.scanned_at.map_or_else(
                                            || "还没扫过".to_string(),
                                            |at| format!("上次扫描 {}", human_time(at)),
                                        ),
                                    ))
                                    .small()
                                    .weak(),
                                );
                            }
                            // 核心库那句话原样画出来：它自己就说清了是哪个版本对哪个版本、
                            // 该怎么办。
                            CatalogState::Openable(Err(说的))
                            | CatalogState::SchemaMismatch { said: 说的, .. }
                            | CatalogState::Broken { said: 说的 } => {
                                ui.label(egui::RichText::new(说的).small().color(提醒));
                            }
                        }
                    },
                );
                // 「打开」摆在右头，在左边那一块的高度里上下居中（设计稿 `.catrow .go`）。
                let 行高 = 左边.response.rect.height();
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 行高),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        // **开不进去的那一份按钮按不下去，但那一行照样在。** 从列表里静静消失
                        // 才是最难查的那种错（ADR-0023）。
                        //
                        // 按的是「开得进去」而**不是**「那几个数读出来了没有」：开进去了、
                        // 只是数没读回来的那一份照样开得进去，把两件事并成一件就会把它画成
                        // 一份坏库。
                        let 打开 = |ui: &mut egui::Ui| {
                            look::small_buttons(ui, |ui| {
                                ui.add_enabled(一份.state.openable(), egui::Button::new("打开"))
                            })
                        };
                        let 按钮 = if 主按钮 {
                            ui.scope(|ui| {
                                look::primary_button(ui.visuals_mut());
                                打开(ui)
                            })
                            .inner
                        } else {
                            打开(ui)
                        };
                        按钮.clicked()
                    },
                )
                .inner
            })
            .inner
        });
    // 开不进去的那一行，左边一条提醒色的竖条（设计稿 `.catrow.old`），宽取置信度色条那一格。
    if !一份.state.openable() {
        let 行 = 这一行.response.rect;
        let 竖条 =
            egui::Rect::from_min_size(行.min, egui::vec2(tokens.layout.tier_bar, 行.height()));
        let 颜色 = look::tone_colors(Tone::Caution, ui.visuals()).0;
        ui.painter().rect_filled(竖条, 0.0, 颜色);
    }
    这一行.inner
}
