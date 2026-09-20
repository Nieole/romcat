//! **弹层**：窗口里每一处对话框都照这一种写法画——添加主库向导、刮削、批量裁决计划，往后的
//! 子库目标设置、手动例外、合集、优先级、导出照写、移除根、成型纠正、平台纠正、库体检明细、导入、
//! 快捷键表、删除确认、移出此作品（票 `gui-looks-like-the-design/04`）。
//!
//! **不许各写各的。** 自己拿 `egui::Modal` / `egui::Window` 摆一块，遮罩、宽度、Esc、焦点、
//! 底下那一屏的快捷键就又各是各的——这一层收之前，待确认屏那份计划书正是这样：现编一个 560 的
//! 宽度、Esc 不管、「开着不接键盘」那道门记在那一屏自己身上。
//!
//! ## 怎么用
//!
//! 弹层开没开着是**画它的那一屏**自己记着的（一个 `Option`、一个 `bool`），这一层不替谁记。
//! 开着的每一帧画一次，看交回来的动作：
//!
//! ```no_run
//! use romcat_gui::dialog::{Button, Dialog, Footer, Width};
//!
//! /// 页脚上按下去的是哪一颗。
//! enum 按的 {
//!     取消,
//!     保存,
//! }
//!
//! /// 画弹层的那一屏：弹层开没开着由它自己记。
//! struct 子库屏 {
//!     /// 「目标设置」开着时，框里正打着的目标路径。
//!     目标设置: Option<String>,
//! }
//!
//! impl 子库屏 {
//!     fn ui(&mut self, ctx: &egui::Context) {
//!         let Some(路径) = &mut self.目标设置 else {
//!             return;
//!         };
//!         let 填了 = !路径.trim().is_empty();
//!         // 退出那一颗靠左，Esc 等于按它；其余几颗照读的次序靠右，往前走的那一颗是主按钮。
//!         let footer = Footer::new(Button::new("取消", 按的::取消))
//!             .button(Button::new("保存", 按的::保存).enabled(填了).primary());
//!         let shown = Dialog::new("目标设置", "目标设置 · 掌机", footer)
//!             .note("改了目标路径，已经排好的差量预览会作废")
//!             .width(Width::Wide)
//!             .show(ctx, |ui| ui.text_edit_singleline(路径));
//!         match shown.pressed {
//!             Some(按的::取消) => self.目标设置 = None,
//!             Some(按的::保存) => {
//!                 // 存下来……
//!                 self.目标设置 = None;
//!             }
//!             None => {}
//!         }
//!     }
//! }
//! ```
//!
//! **页脚先搭好再画内容区**：页脚是一份数据（字、动作、按不按得动、悬停那句），内容区是一个
//! 闭包——两样都要读写同一份状态的话，只有这样借用才排得开。按不按得动因此看的是这一帧画之前的
//! 状态；按下任何一颗弹层都会要一次重画，下一帧就对上了。
//!
//! ## 这一层替每一处做掉的几件事
//!
//! - **遮罩**：令牌 `scrim`（[`look::scrim`]），盖住整个窗口，底下那一屏点不动。
//!   **点遮罩不关**：弹层里常有打到一半的字，手一滑就没了。
//! - **标题 + 说明**：标题是令牌 `size-title` 那一档，说明是弱字。
//! - **内容区滚得动，页脚不被顶出去**：这一层最多多高由窗口定（离上下边各留间距最宽那一档），
//!   内容比这还长时是内容区自己滚。
//! - **页脚**：退出那一颗靠左，其余几颗照读的次序靠右——看着靠右，Tab 照读的次序走。往前走的
//!   那一颗（「下一步」「开始扫描」「开始刮削」「落下」）标上 [`Button::primary`]，画成强调色底。
//!   **没有往前走那一颗的只读弹层**（库体检明细）另有一种写法：退出那一颗靠右画成主按钮，其余几颗靠左
//!   （[`Footer::dismiss_on_right`]，常配 [`Button::ghost`]）。
//! - **宽度只取令牌里那四档**（[`Width`]），不许现编一个数；窗口比那一档还窄时收进窗口里。
//! - **Esc 关最上面那一层**，等于按退出那一颗（[`Shown::pressed`] 交回的就是它的动作）。
//!   两层叠着时一下只退一层。
//! - **焦点**：打开时落进这一层里头头一个接得住焦点的控件（向导是起名那一框；内容区里没有
//!   控件时是页脚头一颗）。这一层不再画了——不管是 Esc、页脚、还是画它的那一屏把它扔了——焦点
//!   还给打开它时拿着焦点的那个控件。**鼠标点开的弹层关上之后焦点不落到任何地方**：egui 点一下
//!   按钮不给它焦点，没有「原来那颗」可还（挂单 `Q668`）。
//! - **底下那一屏的快捷键不接**：各屏读快捷键之前先问 [`screen_has_keys`]。
//!
//! ## 两条要知道的
//!
//! - **中文输入放在不会被回收的区域里**（ADR-0005）：内容区每帧把整份内容都摆一遍，输入框滚到
//!   外头去也还在（`tests/dialog.rs` 钉着）。**别在内容区里再套一层只摆看得见那几行的东西**
//!   （`ScrollArea::show_rows` 之类）**再往里放输入框**——那正是 ADR-0005 拦的那种回收。
//! - **那道快捷键的门看的是上一帧**：[`screen_has_keys`] 问的是上一帧有没有弹层画出来
//!   （挂单 `Q662`）。真窗口里打开弹层的那一下与它第一次画在同一帧，没有缝；弹层在两帧之间被
//!   打开（测试里直接调 `open` 那种）时，它画出来之前那一帧的按键拦不住。关上那一下弹层自己
//!   要一次重画，补上那一帧。

use std::fmt::Debug;
use std::hash::Hash;

use crate::font;
use crate::look;
use crate::tokens::Tokens;

/// 弹层有多宽：**只有令牌里那四档**（`tokens.toml` 的 `dialog-width`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Width {
    /// 最窄那一档：一两句话的确认。
    Narrow,
    /// 默认那一档：向导、快捷键表。
    #[default]
    Standard,
    /// 宽一档：表单、带几列旋钮的面板。
    Wide,
    /// 最宽那一档：带表格的明细。
    Widest,
}

impl Width {
    /// 四档，从窄到宽。
    pub const ALL: [Self; 4] = [Self::Narrow, Self::Standard, Self::Wide, Self::Widest];

    /// 这一档多宽，点：令牌里那一格。
    #[must_use]
    pub fn points(self) -> f32 {
        let at = match self {
            Self::Narrow => 0,
            Self::Standard => 1,
            Self::Wide => 2,
            Self::Widest => 3,
        };
        Tokens::builtin().layout.dialog_width[at]
    }
}

/// 页脚上的一颗按钮：写在上面的字，与按下去交回的那个动作。
pub struct Button<A> {
    /// 按钮上的字。
    label: String,
    /// 按下去交回的动作。
    action: A,
    /// 按得动没有。
    enabled: bool,
    /// 是不是往前走的那一颗。
    primary: bool,
    /// 是不是删掉东西的那一颗。
    danger: bool,
    /// 是不是弱化的那一颗（幽灵按钮）。
    ghost: bool,
    /// 指针停在上面时说的那句话。
    hover: Option<String>,
}

impl<A> Button<A> {
    /// 一颗写着 `label`、按下去交回 `action` 的按钮。
    #[must_use]
    pub fn new(label: impl Into<String>, action: A) -> Self {
        Self {
            label: label.into(),
            action,
            enabled: true,
            primary: false,
            danger: false,
            ghost: false,
            hover: None,
        }
    }

    /// **弱化的那一颗**（幽灵按钮，设计稿 `.btn.ghost`）：平时不描边不填色，悬停才垫底色（[`look::ghost_button`]）。
    /// 库体检明细弹层上靠左的「导出清单…」标它。与 [`Self::primary`]、[`Self::danger`] 不同时标。
    #[must_use]
    pub fn ghost(mut self) -> Self {
        self.ghost = true;
        self
    }

    /// 按得动没有。
    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// **往前走的那一颗**：画成强调色底（[`look::primary_button`]）。
    ///
    /// 一排里最多标一颗——两颗都标，人就分不出哪一颗是这一层等着他按的。退出那一颗不标。
    #[must_use]
    pub fn primary(mut self) -> Self {
        self.primary = true;
        self
    }

    /// **删掉东西的那一颗**：画成危险色底（[`look::danger_button`]）。删除确认弹层上按下去就真删的那一颗标它；
    /// 与 [`Self::primary`] 不同时标。
    #[must_use]
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    /// 指针停在上面时说的那句话。
    #[must_use]
    pub fn hover(mut self, text: impl Into<String>) -> Self {
        self.hover = Some(text.into());
        self
    }
}

/// 页脚那一排按钮。
pub struct Footer<A> {
    /// **退出那一颗**：Esc 等于按它。
    dismiss: Button<A>,
    /// 其余几颗，照读的次序。
    rest: Vec<Button<A>>,
    /// 退出那一颗摆在右边（[`Self::dismiss_on_right`]）。
    dismiss_on_right: bool,
}

impl<A> Footer<A> {
    /// 一排页脚，退出那一颗是 `dismiss`。
    #[must_use]
    pub fn new(dismiss: Button<A>) -> Self {
        Self {
            dismiss,
            rest: Vec::new(),
            dismiss_on_right: false,
        }
    }

    /// **退出那一颗摆在右边、画成主按钮**，其余几颗照读的次序靠左（设计稿 `DLG.health` 的页脚：左边幽灵按钮「导出清单…」、
    /// 右边主按钮「关闭」）。给**没有「往前走」那一颗**的只读弹层用：关掉就是这一层唯一的出口（票
    /// `gui-looks-like-the-design/27`，挂单 `Q958`）。Esc 照旧等于按退出那一颗；已有的弹层不标它，照旧退出那一颗靠左。
    #[must_use]
    pub fn dismiss_on_right(mut self) -> Self {
        self.dismiss_on_right = true;
        self
    }

    /// 再摆一颗。
    #[must_use]
    pub fn button(mut self, button: Button<A>) -> Self {
        self.rest.push(button);
        self
    }

    /// 这一颗交回的动作。
    fn take(mut self, slot: Slot) -> A {
        match slot {
            Slot::Dismiss => self.dismiss.action,
            Slot::Rest(at) => self.rest.swap_remove(at).action,
        }
    }
}

/// 页脚上的**哪一颗**：退出那一颗，或者其余几颗里的第几颗。
///
/// 与各处自己的动作枚举不是一回事——那一个说按下去要干什么，这一个只说按的是第几颗，
/// 画完这一帧由 [`Footer::take`] 换成那个动作。
#[derive(Debug, Clone, Copy)]
enum Slot {
    /// 退出那一颗。
    Dismiss,
    /// 其余几颗里的第几颗。
    Rest(usize),
}

/// 一层弹层。
///
/// `'a` 是[标头那一排](Self::head)那个闭包借着的东西活多久——摆它的那一屏手上的状态。调用方摆弹层
/// 一律是一条写到底的链（`Dialog::new(..).head(..).show(..)`），那个寿命自己对得上，写不出来。
pub struct Dialog<'a, A> {
    /// 这一层的 id。
    id: egui::Id,
    /// 标题。
    title: String,
    /// 标题底下那句说明。
    note: Option<String>,
    /// 标头那一排「走到第几问」：各问的名字，与眼下是第几问（从 0 数）。
    pages: Option<(Vec<String>, usize)>,
    /// 标头底下、分隔线上面那一排（[`Self::head`]）。
    head: Option<Box<dyn FnOnce(&mut egui::Ui) + 'a>>,
    /// 页脚左边那句说明字（[`Self::footer_note`]）。
    footer_note: Option<String>,
    /// 多宽。
    width: Width,
    /// 页脚。
    footer: Footer<A>,
}

/// 画完一帧交回来的东西。
pub struct Shown<A, R> {
    /// 这一帧按下去的那颗交回的动作；按了 Esc 就是退出那一颗的。
    pub pressed: Option<A>,
    /// 内容区交回来的东西。
    pub inner: R,
    /// 这一层连边框画在哪儿。
    pub rect: egui::Rect,
}

impl<'a, A> Dialog<'a, A> {
    /// 一层弹层：`id_salt` 在全窗口里认得出它，`title` 是标题。
    #[must_use]
    pub fn new(id_salt: impl Hash + Debug, title: impl Into<String>, footer: Footer<A>) -> Self {
        Self {
            id: egui::Id::new(("弹层", id_salt)),
            title: title.into(),
            note: None,
            pages: None,
            head: None,
            footer_note: None,
            width: Width::default(),
            footer,
        }
    }

    /// 标题底下那句说明。
    #[must_use]
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// **向导那一排「走到第几问」**：画在标题与说明底下，一问一格——走过的打勾、正在问的是强调色、
    /// 还没到的是弱字（设计稿 `.steps`）。`at` 从 0 数。
    ///
    /// 格里只写那一问的名字（「命名」「选择目录」）；整句的说明归 [`Self::note`]。
    #[must_use]
    pub fn pages<S: Into<String>>(
        mut self,
        labels: impl IntoIterator<Item = S>,
        at: usize,
    ) -> Self {
        self.pages = Some((labels.into_iter().map(Into::into).collect(), at));
        self
    }

    /// **标头底下、分隔线上面那一排**（设计稿 `.mhead` 里的 `head`）：分栏那一排、过滤那一排——
    /// 一眼看得出这一层眼下摆的是哪一份的那种东西。手动例外那层的「包含｜排除」走的就是它
    /// （票 `gui-looks-like-the-design/22`）。
    ///
    /// **它不是内容区**：内容区滚得动，而这一排要一直钉在分隔线上头——滚下去之后看不出自己在哪一栏，
    /// 正是这一排要防的事。也因此这里**只摆一排挑东西的控件**，别往里塞输入框（会碰到输入法的东西归内容区，
    /// ADR-0005）。
    ///
    /// 给了它，标头那一块的下留白收窄成一档（设计稿拿 `margin-bottom:-14px` 把 `.mhead` 的下留白抵掉，
    /// 让那一排贴着分隔线；这里换成少留一点，效果是同一个）。**不给就一点地方都不占**，画出来与没有这个槽
    /// 之前一模一样。
    #[must_use]
    pub fn head(mut self, add: impl FnOnce(&mut egui::Ui) + 'a) -> Self {
        self.head = Some(Box::new(add));
        self
    }

    /// **页脚左边那句说明字**（设计稿 `.mfoot` 里那个 `.help`）：这一层的规矩——按完之后还要做什么、
    /// 这一层改的东西会连累到谁。手动例外那层的「修改例外后，同步前需要重新生成差量预览。」走的就是它。
    ///
    /// 摆在页脚而不是内容区末尾：内容区滚得动，这句话滚出去就看不见了，而它说的是**整层**的规矩，
    /// 不是最后那一段的注脚。
    ///
    /// 配 [`Footer::dismiss_on_right`] 才是设计稿那个样子：说明字靠左、退出那一颗贴右当主按钮。
    /// **不给就一点地方都不占。**
    #[must_use]
    pub fn footer_note(mut self, text: impl Into<String>) -> Self {
        self.footer_note = Some(text.into());
        self
    }

    /// 多宽：令牌里那四档之一。
    #[must_use]
    pub fn width(mut self, width: Width) -> Self {
        self.width = width;
        self
    }

    /// 画这一帧。
    pub fn show<R>(
        self,
        ctx: &egui::Context,
        body: impl FnOnce(&mut egui::Ui) -> R,
    ) -> Shown<A, R> {
        let Self {
            id,
            title,
            note,
            pages,
            head,
            footer_note,
            width,
            footer,
        } = self;

        // **焦点**：头一回画这一层时记下是谁拿着焦点（多半是打开它的那颗按钮），并把焦点交给
        // 这一层里头头一个接得住的控件；这一层不再画了，由 [`return_focus`] 还回去。
        hook_return_focus(ctx);
        let pass = ctx.cumulative_pass_nr();
        let opener = ctx.memory(egui::Memory::focused);
        let focus_landed = ctx.data_mut(|data| {
            let layer = data
                .get_temp_mut_or_default::<egui::IdMap<OpenLayer>>(open_layers_id())
                .entry(id)
                .or_insert(OpenLayer {
                    opener,
                    focus_landed: false,
                    shown_pass: pass,
                });
            layer.shown_pass = pass;
            layer.focus_landed
        });
        if !focus_landed {
            // 头一帧 egui 在量这一层多大，里头的控件接不住焦点——所以没接住就下一帧再来一遍。
            ctx.memory_mut(|memory| {
                if let Some(focused) = memory.focused() {
                    memory.surrender_focus(focused);
                }
                memory.move_focus(egui::FocusDirection::Next);
            });
        }

        let tokens = Tokens::builtin();
        let style = ctx.style_of(ctx.theme());
        let visuals = &style.visuals;
        let screen = ctx.content_rect();
        // 间距只从令牌那几档里取（`space.steps`，从窄到宽）：离窗口边取最宽那一档；抬头上下取
        // 第四档、页脚上下取第三档——设计稿 `.mhead` 是上 18 下 14、`.mfoot` 是 12，各取最近那一档。
        let step = |at: usize| tokens.space.steps.get(at).copied().unwrap_or_default();
        let margin = tokens.space.steps.last().copied().unwrap_or_default();
        let [header_pad_y, footer_pad_y] = [step(3), step(2)];
        let stroke = visuals.window_stroke.width;
        // 窗口比那一档还窄时收进窗口里：窗口最小 720 点宽，而最宽那一档是 840。
        let outer_width = width.points().min(screen.width() - 2.0 * margin);
        let outer_height = screen.height() - 2.0 * margin;

        // 页脚多高是上一帧量出来的：内容区先画、页脚后画，而内容区能占多高得扣掉页脚。
        let footer_height_id = id.with("页脚多高");
        let footer_height = ctx
            .data(|data| data.get_temp::<f32>(footer_height_id))
            .unwrap_or(style.spacing.interact_size.y + 2.0 * footer_pad_y);

        let frame = egui::Frame::new()
            .fill(visuals.window_fill)
            .stroke(visuals.window_stroke)
            .corner_radius(visuals.window_corner_radius)
            .shadow(visuals.window_shadow);
        let [pad_y, pad_x] = tokens.space.dialog_padding;
        let footer_radius = egui::CornerRadius {
            nw: 0,
            ne: 0,
            ..visuals.window_corner_radius
        };
        let footer_fill = visuals.faint_bg_color;
        let modal = egui::Modal::new(id)
            .backdrop_color(look::scrim(visuals))
            .frame(frame)
            .show(ctx, |ui| {
                ui.set_width(outer_width - 2.0 * stroke);
                let spacing = ui.spacing().item_spacing;
                // 三段之间不留缝：分隔线就是缝。段里头照旧用原来的间距。
                ui.spacing_mut().item_spacing.y = 0.0;

                // **有标头那一排时下留白收窄**（[`Dialog::head`]）：设计稿拿 `margin-bottom:-14px` 把
                // `.mhead` 的下留白抵掉，让那一排贴着分隔线；这里换成少留一点，效果是同一个。
                // 没给那个槽时这里一个字都不变——下留白照旧是 `header_pad_y`。
                //
                // **没给那个槽时这一句折出来的与从前一模一样**：底下那一行原来就是
                // `Margin::from(vec2(pad_x, header_pad_y))`，只在有标头那一排时才去改它的下边。
                let mut header_margin = egui::Margin::from(egui::vec2(pad_x, header_pad_y));
                if head.is_some() {
                    header_margin.bottom = egui::Margin::from(egui::vec2(pad_x, step(1))).bottom;
                }
                egui::Frame::new()
                    .inner_margin(header_margin)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = spacing;
                        ui.set_width(ui.available_width());
                        ui.label(
                            font::strong(title)
                                .text_style(egui::TextStyle::Name(look::TITLE.into())),
                        );
                        if let Some(note) = note {
                            ui.label(egui::RichText::new(note).small().weak());
                        }
                        // 设计稿 `.steps` 离说明是 14，取最近那一档。
                        if let Some((labels, at)) = &pages {
                            ui.add_space(step(2));
                            pages_ui(ui, labels, *at);
                        }
                        // 设计稿 `.dtabs` 离说明是 12，取最近那一档。
                        if let Some(head) = head {
                            ui.add_space(step(2));
                            head(ui);
                        }
                    });
                look::divider(ui);
                let above_body = ui.min_rect().height();

                let body_height =
                    (outer_height - 2.0 * stroke - above_body - 1.0 - footer_height).max(0.0);
                // **内容区摆在一块明说了多大的地方里。** 不明说的话，它能占多高由这一层眼下摆在
                // 哪儿决定（离窗口底边还剩多少）——而这一层是照上一帧的尺寸居中的，于是一帧长
                // 一截、要好多帧才长到头，人看见的是弹层一点点往下撑。
                let body_rect = egui::Rect::from_min_size(
                    ui.cursor().min,
                    egui::vec2(outer_width - 2.0 * stroke, body_height),
                );
                let inner = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(body_rect), |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("内容区")
                            .auto_shrink([false, true])
                            .max_height(body_height)
                            .show(ui, |ui| {
                                egui::Frame::new()
                                    .inner_margin(egui::Margin::from(egui::vec2(pad_x, pad_y)))
                                    .show(ui, |ui| {
                                        ui.spacing_mut().item_spacing = spacing;
                                        ui.set_width(ui.available_width());
                                        body(ui)
                                    })
                                    .inner
                            })
                            .inner
                    })
                    .inner;
                look::divider(ui);

                let above_footer = ui.min_rect().height();
                let clicked = egui::Frame::new()
                    .fill(footer_fill)
                    .corner_radius(footer_radius)
                    .inner_margin(egui::Margin::from(egui::vec2(pad_x, footer_pad_y)))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = spacing;
                        ui.set_width(ui.available_width());
                        footer_ui(ui, &footer, footer_note.as_deref())
                    })
                    .inner;
                let measured = ui.min_rect().height() - above_footer;
                ui.ctx()
                    .data_mut(|data| data.insert_temp(footer_height_id, measured));
                (inner, clicked)
            });
        if !focus_landed {
            let landed = ctx.memory(egui::Memory::focused).is_some();
            // 没接住的话别让这一回「交给下一个」漏到这一层之后才画的控件上去。
            ctx.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
            if landed {
                ctx.data_mut(|data| {
                    if let Some(layer) = data
                        .get_temp_mut_or_default::<egui::IdMap<OpenLayer>>(open_layers_id())
                        .get_mut(&id)
                    {
                        layer.focus_landed = true;
                    }
                });
            }
        }
        let (inner, clicked) = modal.inner;
        let escaped = modal.is_top_modal
            && !modal.any_popup_open
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let pressed = match clicked {
            Some(slot) => Some(footer.take(slot)),
            None if escaped => Some(footer.take(Slot::Dismiss)),
            None => None,
        };
        if pressed.is_some() {
            ctx.request_repaint();
        }
        Shown {
            pressed,
            inner,
            rect: modal.response.rect,
        }
    }
}

/// 一层开着的弹层替**焦点**记下的东西：谁打开的它、焦点落进去没有、最近一回哪一趟画过它。
#[derive(Debug, Clone, Copy)]
struct OpenLayer {
    /// 头一回画它时拿着焦点的那个控件：关上之后焦点还给它。
    opener: Option<egui::Id>,
    /// 焦点落进这一层里头了没有。
    focus_landed: bool,
    /// 最近一回画它是第几趟。
    shown_pass: u64,
}

/// 开着的那几层记在 egui 临时数据里的哪一格。
fn open_layers_id() -> egui::Id {
    egui::Id::new("开着的弹层")
}

/// 每一趟画完问一遍 [`return_focus`]。**一个窗口只挂一次。**
fn hook_return_focus(ctx: &egui::Context) {
    let hooked = egui::Id::new("弹层关上时还焦点");
    if ctx
        .data(|data| data.get_temp::<bool>(hooked))
        .unwrap_or(false)
    {
        return;
    }
    ctx.data_mut(|data| data.insert_temp(hooked, true));
    ctx.on_end_pass(
        "弹层关上时还焦点",
        std::sync::Arc::new(|ui: &mut egui::Ui| return_focus(ui.ctx())),
    );
}

/// 这一趟没再画的那几层**就是关上了**：焦点还给当初打开它的那个控件。
///
/// 放在每一趟的末尾而不交给各处自己还：弹层是怎么关的有好几条路（Esc、页脚那几颗、
/// 画它的那一屏把它扔了），各处自己还就得每一条路都记得还。
fn return_focus(ctx: &egui::Context) {
    let pass = ctx.cumulative_pass_nr();
    let openers: Vec<egui::Id> = ctx.data_mut(|data| {
        let mut openers = Vec::new();
        data.get_temp_mut_or_default::<egui::IdMap<OpenLayer>>(open_layers_id())
            .retain(|_, layer| {
                let still_open = layer.shown_pass == pass;
                if !still_open {
                    openers.extend(layer.opener);
                }
                still_open
            });
        openers
    });
    for opener in openers {
        ctx.memory_mut(|memory| memory.request_focus(opener));
    }
}

/// 标头那一排「走到第几问」（[`Dialog::pages`]）。
///
/// 每一格：一圈里写着序号（走过的画一个勾），跟着那一问的名字，格与格之间一道细线。颜色：正在问的
/// 那一圈是主按钮那一档（[`look::primary_button`]），走过的是「放心」那一对标签色
/// （[`look::tone_colors`]），还没到的是面板底、输入框描边、弱字。
///
/// **勾是两段线画的**，不靠字体里有没有 ✓：打包的子集字体里没有它，落到回退链上画成什么说不准。
fn pages_ui(ui: &mut egui::Ui, labels: &[String], at: usize) {
    let tokens = Tokens::builtin();
    let 直径 = tokens.layout.page_dot;
    let 半径 = 直径 / 2.0;
    let 缝 = look::step(1);
    let visuals = ui.visuals().clone();
    let mut 主按钮 = visuals.clone();
    look::primary_button(&mut 主按钮);
    let (放心字, 放心底) = look::tone_colors(look::Tone::Good, &visuals);
    let 线色 = visuals.widgets.inactive.bg_stroke.color;
    let 格宽 = ui.available_width() / labels.len().max(1) as f32;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (i, label) in labels.iter().enumerate() {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(格宽, 直径), egui::Sense::hover());
            let 圆心 = egui::pos2(rect.left() + 半径, rect.center().y);
            let (底, 边, 字色) = match i.cmp(&at) {
                std::cmp::Ordering::Less => (放心底, 放心字, 放心字),
                std::cmp::Ordering::Equal => {
                    let 这一档 = &主按钮.widgets.inactive;
                    (
                        这一档.bg_fill,
                        这一档.bg_stroke.color,
                        这一档.fg_stroke.color,
                    )
                }
                std::cmp::Ordering::Greater => {
                    (visuals.window_fill, 线色, visuals.weak_text_color())
                }
            };
            let painter = ui.painter();
            painter.circle(
                圆心,
                半径,
                底,
                egui::Stroke::new(tokens.layout.control_stroke, 边),
            );
            if i < at {
                let 勾 = egui::Stroke::new(0.15 * 半径, 字色);
                let 折点 = 圆心 + egui::vec2(-0.1 * 半径, 0.3 * 半径);
                painter.line_segment([圆心 + egui::vec2(-0.4 * 半径, 0.0), 折点], 勾);
                painter.line_segment([折点, 圆心 + egui::vec2(0.4 * 半径, -0.3 * 半径)], 勾);
            } else {
                let 序号 =
                    egui::WidgetText::from(egui::RichText::new((i + 1).to_string()).color(字色))
                        .into_galley(
                            ui,
                            Some(egui::TextWrapMode::Extend),
                            f32::INFINITY,
                            egui::TextStyle::Name(look::CAPTION.into()),
                        );
                painter.galley(圆心 - 序号.size() / 2.0, 序号, 字色);
            }
            let 名字色 = if i == at {
                visuals.strong_text_color()
            } else {
                visuals.weak_text_color()
            };
            let 名字 = egui::WidgetText::from(egui::RichText::new(label).color(名字色))
                .into_galley(
                    ui,
                    Some(egui::TextWrapMode::Extend),
                    f32::INFINITY,
                    egui::TextStyle::Small,
                );
            let 名字在 = egui::pos2(
                rect.left() + 直径 + 缝,
                rect.center().y - 名字.size().y / 2.0,
            );
            let 线从 = 名字在.x + 名字.size().x + 缝;
            painter.galley(名字在, 名字, 名字色);
            let 线到 = rect.right() - 缝;
            if i + 1 < labels.len() && 线到 > 线从 {
                painter.hline(
                    线从..=线到,
                    rect.center().y,
                    egui::Stroke::new(tokens.layout.control_stroke, 线色),
                );
            }
        }
    });
}

/// 页脚那一排：退出那一颗靠左，其余几颗照读的次序靠右。返回这一帧按下的是哪一颗。
///
/// **靠右不走 `right_to_left`**：那样摆出来的次序是反的，Tab 从右往左走。先量出右边那几颗
/// 一共多宽、空出左边那一截，再从左往右摆——看着靠右，Tab 照读的次序走。
fn footer_ui<A>(ui: &mut egui::Ui, footer: &Footer<A>, note: Option<&str>) -> Option<Slot> {
    let mut clicked = None;
    ui.horizontal(|ui| {
        // **页脚左边那句说明字**（[`Dialog::footer_note`]）：摆在最前头，按钮照旧从它右边接着排。
        // 没给就一个控件都不摆——那一行画出来与没有这个槽之前一模一样。
        //
        // 截断而不是折行：页脚是一行高的，折行会把按钮挤下去；说明字本来就该是一句短话。
        if let Some(note) = note {
            ui.add(
                egui::Label::new(egui::RichText::new(note).small().weak())
                    .truncate()
                    .selectable(false),
            );
        }
        // 退出那一颗摆右边的写法（[`Footer::dismiss_on_right`]）：其余几颗先从左往右摆，再空出中间、退出那一颗贴右当主按钮。
        if footer.dismiss_on_right {
            for (at, button) in footer.rest.iter().enumerate() {
                if add_button(ui, button, button.primary).clicked() {
                    clicked = Some(Slot::Rest(at));
                }
            }
            let wide = look::button_width(ui, &footer.dismiss.label);
            ui.add_space((ui.available_width() - wide).max(0.0));
            if add_button(ui, &footer.dismiss, true).clicked() {
                clicked = Some(Slot::Dismiss);
            }
            return;
        }
        if add_button(ui, &footer.dismiss, footer.dismiss.primary).clicked() {
            clicked = Some(Slot::Dismiss);
        }
        let gap = ui.spacing().item_spacing.x;
        let wide: f32 = footer
            .rest
            .iter()
            .map(|button| look::button_width(ui, &button.label))
            .sum::<f32>()
            + gap * footer.rest.len().saturating_sub(1) as f32;
        ui.add_space((ui.available_width() - wide).max(0.0));
        for (at, button) in footer.rest.iter().enumerate() {
            if add_button(ui, button, button.primary).clicked() {
                clicked = Some(Slot::Rest(at));
            }
        }
    });
    clicked
}

/// 摆一颗页脚按钮。主按钮、危险按钮、幽灵按钮在一个 `scope` 里换上那一档颜色，别的控件不受影响。`primary` 是这一颗
/// 这一回画不画成主按钮：退出那一颗摆右边时由页脚替它定（[`Footer::dismiss_on_right`]）。
fn add_button<A>(ui: &mut egui::Ui, button: &Button<A>, primary: bool) -> egui::Response {
    let add = |ui: &mut egui::Ui| {
        ui.add_enabled(button.enabled, egui::Button::new(button.label.as_str()))
    };
    let response = if primary {
        ui.scope(|ui| {
            look::primary_button(ui.visuals_mut());
            add(ui)
        })
        .inner
    } else if button.ghost {
        ui.scope(|ui| {
            look::ghost_button(ui.visuals_mut());
            add(ui)
        })
        .inner
    } else if button.danger {
        ui.add_enabled_ui(button.enabled, |ui| {
            look::danger_button(ui, button.label.as_str())
        })
        .inner
    } else {
        add(ui)
    };
    match &button.hover {
        Some(text) => response.on_hover_text(text.as_str()),
        None => response,
    }
}

/// 底下那一屏这一帧接不接键盘快捷键：**有一层弹层开着就不接。**
#[must_use]
pub fn screen_has_keys(ctx: &egui::Context) -> bool {
    ctx.memory(|memory| memory.top_modal_layer().is_none())
}
