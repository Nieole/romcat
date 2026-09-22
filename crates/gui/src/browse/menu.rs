//! **右键菜单**：浏览屏上一行或一张卡右键按下去摊开的那一层（设计稿 `.ctx`）。
//!
//! 表格那一路与卡片墙那一路**共用这一层**：两边各自认出「这一下是右键、按在哪一行」，
//! 之后交给这儿的是同一份[开单](Facts)——菜单上摆哪几项、每一项写什么字、按下去交回什么，
//! 只有这一处说了算。两边各画一份的话，同一个动作在表上与卡上迟早写成两句话。
//!
//! ## 这一层只画和转发（ADR-0005、ADR-0024）
//!
//! 每一项**该不该出现、写哪一句话**都不是在这儿判的：
//!
//! - 「勾没勾中」问的是 [`crate::table::Picked::contains`]，而那一份选中集本身就是
//!   「筛选加手动例外」（ADR-0016），由核心库展开。
//! - 「收没收藏」问的是核心库的 [`romcat_core::collection::favorite_of`]——**不是**界面
//!   自己记一个 `fav` 标志。屏上这一句与筛选面板上「收藏」那一格说的因此是同一件事。
//! - 「合并这一下要带上哪几个作品」照的是勾中的那一批（[`Facts::merging`]），而合并本身
//!   开不开得了由 [`merge::Wizard`] 自己说。
//!
//! 菜单收到的是**已经答完的那几个事实**，它只负责把答案写成屏上那句话。
//!
//! ## 这一层一个灰项都没有
//!
//! ADR-0005「不禁按钮」，再修订那一节允许同时画灰要满足两条：守卫那一侧拒得带理由、
//! **屏上常驻着那条理由**。菜单是一层**按一下就收**的浮层——它上面写的任何一句话都只在
//! 摊开的那几秒里看得见，**算不上常驻**：人得先右键、再把指针移到那一项上，才读得到为什么
//! 按不动，而那正是那一条要防的事。所以这一层的规矩更硬一档：**摆上来的每一项都按得动**，
//! 按不动的**不摆**。
//!
//! 于是设计稿那十项里有两项眼下**不摆**（挂单 `Q1140`）：
//!
//! - **加入合集…**——那层弹层是票 `gui-looks-like-the-design/13`（收藏与合集）的。
//!   **它做出来了**（`browse::collections::Join`，走表格上方那一条的「加入合集…」），
//!   于是「弹层还没做」这条理由不再成立；今天不摆只是因为没人往底下这份清单里添这一项，
//!   而添不添挂在 `Q1140` 上、还没裁。（票 13 那一趟没自己添：`Q1140` 的「谁来裁」
//!   写的是拿主意的人，票 13 的票面也一个字没提右键菜单。）
//! - **加入子库…**——那层弹层是票 `gui-looks-like-the-design/23` 的（挂单 `Q806` 那一支）。
//!   今天只有「存成子库」，那是另一件事。
//!
//! 与快捷键表同一条纪律（[`crate::keys`] 的「接一个列一个」）：**屏上不许印一件做不到的事**。
//! 哪一项摆上来，`tests/menu.rs` 那条 `做不到的那两项一项都不摆` 就翻成正面断言。

use romcat_core::catalog::browse::WorkAnchor;
use romcat_core::report::thousands;

use crate::keys;
use crate::look;
use crate::tokens::Tokens;

use super::merge;

/// 菜单上按下去的是哪一项。
///
/// **它只说按了哪一项**，那一项该做什么在浏览屏那一处（`Screen::apply_menu`）——
/// 与屏头那几颗按钮、作品详情页头上那一排按下去走的是同一批入口。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressed {
    /// 打开作品详情。
    OpenDetail,
    /// 编辑元数据：进作品详情页的元数据那一面、当场进编辑态。
    EditMeta,
    /// 勾选 / 取消勾选这一行。
    TogglePick,
    /// 收藏 / 取消收藏这一行。
    ToggleFavorite,
    /// 合并：勾中那一批，或者只有这一个。
    Merge,
    /// 刮削这一个作品底下那几个变体。
    Scrape,
    /// 在文件系统中打开这一行所在的目录。
    Reveal,
    /// 复制这一行屏上写着的名字。
    CopyName,
}

/// 菜单摊开着时贴的是哪一行、那一行眼下是什么样。
///
/// 几个事实**在右键按下去那一下问一次**，不是每帧问一遍：菜单摊开的那几秒里库不会变，
/// 而「这一行收没收藏」是一次读库（[`romcat_core::collection::favorite_of`]）。
#[derive(Debug, Clone)]
pub struct Facts {
    /// 摊在屏上哪儿：右键按下去那一下指针在的地方。
    pub at: egui::Pos2,
    /// 哪一行。
    pub anchor: WorkAnchor,
    /// 菜单顶上那一行写的字：这一行屏上的名字（设计稿 `.ctx .hd`）。
    pub title: String,
    /// 这一行眼下勾中了没有。
    pub picked: bool,
    /// 这一行眼下收藏了没有。
    pub favorited: bool,
    /// 「合并」那一项要带上几个作品：勾中的那一批连这一行算在内。
    ///
    /// 小于 2 时写成「合并…」（从这一个起头，向导第一步再搜别的加进来）；
    /// 2 以上写成「合并勾选的 N 个作品…」（设计稿 `many>=2`）。
    pub merging: u64,
}

/// 菜单上的一项：写在上面那句话、右边那一列提示、按下去交回什么。
struct Item {
    label: String,
    hint: &'static str,
    pressed: Pressed,
}

/// 菜单上一条：一项，或者一道分隔线（设计稿 `.ctx hr`）。
enum Line {
    Item(Item),
    Rule,
}

impl Facts {
    /// 这一份开单摊开来是哪几条，照设计稿的次序。
    fn lines(&self) -> Vec<Line> {
        let item = |label: String, hint: &'static str, pressed: Pressed| {
            Line::Item(Item {
                label,
                hint,
                pressed,
            })
        };
        vec![
            item("打开详情".to_owned(), keys::打开键, Pressed::OpenDetail),
            item("编辑元数据".to_owned(), keys::编辑键, Pressed::EditMeta),
            item(
                if self.picked {
                    "取消勾选"
                } else {
                    "勾选"
                }
                .to_owned(),
                keys::勾选键,
                Pressed::TogglePick,
            ),
            item(
                if self.favorited {
                    "取消收藏"
                } else {
                    "收藏"
                }
                .to_owned(),
                keys::收藏键,
                Pressed::ToggleFavorite,
            ),
            Line::Rule,
            item(
                if self.merging >= 2 {
                    format!("合并勾选的 {} 个作品…", thousands(self.merging))
                } else {
                    merge::MERGE_ONE.to_owned()
                },
                "",
                Pressed::Merge,
            ),
            item("刮削此作品".to_owned(), "", Pressed::Scrape),
            Line::Rule,
            item("在文件系统中打开".to_owned(), "", Pressed::Reveal),
            item("复制名称".to_owned(), "", Pressed::CopyName),
        ]
    }
}

/// 菜单那一层在 egui 记忆里的名字。**全窗口只有这一层**：表格与卡片墙共用它。
fn id() -> egui::Id {
    egui::Id::new("浏览屏右键菜单")
}

/// 浏览屏手上那一层右键菜单：开着时是 `Some`。
#[derive(Debug, Default)]
pub struct Menu {
    open: Option<Facts>,
}

impl Menu {
    /// 摊开一层，贴着 `open.at`。已经开着一层就换成这一份。
    ///
    /// 开没开着记在 **egui 的记忆**里（[`egui::Popup::open_id`]），不是这个结构体自己记：
    /// 那一份记忆同时是全窗口「眼下有没有浮层摊着」那一问的答案
    /// （[`egui::Popup::is_any_open`]），而读快捷键之前正要问那一句（`App::shortcuts`）。
    /// 各记各的，就会出现菜单摊着、单键快捷键照接的那一帧。
    pub fn open(&mut self, ctx: &egui::Context, facts: Facts) {
        self.open = Some(facts);
        egui::Popup::open_id(ctx, id());
    }

    /// 收起来。
    pub fn close(&mut self, ctx: &egui::Context) {
        self.open = None;
        egui::Popup::close_id(ctx, id());
    }

    /// 画这一帧；按下去的那一项**连它贴的那一行整份**一起交出来。
    ///
    /// 交的是整份[开单](Facts)而不只是那一行是谁：「复制名称」复制的得是**屏上摆着的那个
    /// 名字**，而那一句话就写在开单上——回头再去库里问一次，问出来的可能已经不是人眼前
    /// 那一个了。
    ///
    /// **Esc 与点别处由 egui 那一层收**（`Popup` 自己认 `Key::Escape` 与
    /// `CloseOnClickOutside`），这里只跟着它把手上那份开单扔掉——Esc 一层一层退那一条
    /// 因此不用在这儿再写一遍：菜单摊着时最上面的一层就是它。
    pub fn ui(&mut self, ctx: &egui::Context) -> Option<(Pressed, Facts)> {
        let open = self.open.clone()?;
        let lines = open.lines();
        let 一层 = egui::Popup::new(
            id(),
            ctx.clone(),
            egui::PopupAnchor::Position(open.at),
            egui::LayerId::new(egui::Order::Foreground, id()),
        )
        .open_memory(None)
        .kind(egui::PopupKind::Menu)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
        .layout(egui::Layout::top_down(egui::Align::Min))
        .gap(0.0)
        .frame(菜单框(ctx));
        let shown = 一层.show(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            // **先量再画**：那一列提示要立得住，各项就得一样宽（[`量宽`]）。
            // 定宽靠 `set_min_width`，不靠布局器交回来的那个宽（挂单 `Q1078`）。
            let 内宽 = 量宽(ui, &lines);
            ui.set_min_width(内宽);
            抬头(ui, &open.title, 内宽);
            let mut 按的 = None;
            for line in &lines {
                match line {
                    Line::Rule => 分隔线(ui, 内宽),
                    Line::Item(item) => {
                        if 一项(ui, item, 内宽) {
                            按的 = Some(item.pressed);
                        }
                    }
                }
            }
            按的
        });
        let 按的 = shown.and_then(|shown| shown.inner);
        // egui 那一层收起来了（Esc、点别处、或者刚按下一项），手上这份开单跟着扔。
        if !egui::Popup::is_id_open(ctx, id()) {
            self.open = None;
        }
        if 按的.is_some() {
            self.close(ctx);
        }
        按的.map(|pressed| (pressed, open))
    }
}

/// 菜单外面那个框：面板底、大圆角、一圈弹层阴影（设计稿 `.ctx`）**外加一圈描边**。
///
/// ⚠️ **那圈描边稿上没有**（`.ctx` 只有 `background` 与 `box-shadow`），这儿加了一圈
/// `window_stroke`。**已裁：留着**（协调人 2026-09-22，挂单 `Q1141`）。
///
/// 两条理由：① 仓里每一层浮起来的东西都带这一圈（`dialog.rs` 走的也是 `window_stroke`，
/// egui 自己的 `Frame::popup` 同样）——这不是新发明一种画法，是归队；② 这一层**落在哪儿
/// 由右键按在哪儿定**，落到右边那块详情栏上时它与身底下那一块是**同一个颜色**（两处都是
/// 令牌 `panel`），暗色里除了一点阴影什么边都看不出——一层看不出边界的浮层，人分不清
/// 哪几行属于菜单。
fn 菜单框(ctx: &egui::Context) -> egui::Frame {
    let tokens = Tokens::builtin();
    let style = ctx.style_of(ctx.theme());
    let visuals = &style.visuals;
    egui::Frame::new()
        .fill(visuals.window_fill)
        .stroke(visuals.window_stroke)
        .corner_radius(tokens.radius.large)
        .shadow(visuals.popup_shadow)
        .inner_margin(egui::Margin::same(菜单留白(tokens.space.menu_padding)))
}

/// 四周那点留白（设计稿 `.ctx` 的 `padding:5px`）取整成 `Margin` 要的整数。
fn 菜单留白(padding: f32) -> i8 {
    padding.round().clamp(0.0, f32::from(i8::MAX)) as i8
}

/// 菜单**里头**多宽：最窄那一档与「最长的一项摆得下」两者取大（设计稿 `.ctx` 的 `min-width`）。
///
/// 先量再画，是为了让右边那一列提示**立成一列**：每一项各自靠右摆的话，这一列的右缘
/// 就是各项自己的右缘——而各项一样宽，那一列才真的是一列。
fn 量宽(ui: &egui::Ui, lines: &[Line]) -> f32 {
    let tokens = Tokens::builtin();
    let 左右 = tokens.space.menu_item_padding;
    let mut 宽 = tokens.layout.menu_min_width;
    for line in lines {
        let Line::Item(item) = line else {
            continue;
        };
        let 话 = 量一段(ui, &item.label, 正文字号(ui.ctx()), false).x;
        let 提示 = if item.hint.is_empty() {
            0.0
        } else {
            tokens.space.menu_item_gap + 量一段(ui, item.hint, 提示字号(ui.ctx()), true).x
        };
        宽 = 宽.max(话 + 提示 + 2.0 * 左右);
    }
    // **抬头那一行不撑宽菜单**：它自己最宽到 `menu-head-width` 就省略号（设计稿 `.ctx .hd`）。
    宽
}

/// 菜单上一项的字号（设计稿 `.ctx button` 的 `12.5px`）。
fn 正文字号(ctx: &egui::Context) -> f32 {
    look::font_size(ctx, Tokens::builtin().font.size_small_plus)
}

/// 右边那一列提示的字号（设计稿 `.ctx button span` 的 `11px`，等宽）。
fn 提示字号(ctx: &egui::Context) -> f32 {
    look::font_size(ctx, Tokens::builtin().font.size_caption)
}

/// 量一段字多大。`等宽` 时用等宽族（提示那一列照稿是 `var(--mono)`）。
fn 量一段(ui: &egui::Ui, text: &str, size: f32, 等宽: bool) -> egui::Vec2 {
    let font = if 等宽 {
        egui::FontId::monospace(size)
    } else {
        egui::FontId::proportional(size)
    };
    ui.painter()
        .layout_no_wrap(text.to_owned(), font, egui::Color32::PLACEHOLDER)
        .size()
}

/// 菜单顶上那一行：这一层贴的是哪一行（设计稿 `.ctx .hd`）。太长就末尾省略号。
fn 抬头(ui: &mut egui::Ui, title: &str, 宽: f32) {
    let tokens = Tokens::builtin();
    let [上, 左右, 下] = tokens.space.menu_head_padding;
    let palette = look::palette(ui);
    let 字号 = look::font_size(ui.ctx(), tokens.font.size_caption_plus);
    let mut job = egui::text::LayoutJob::simple_singleline(
        title.to_owned(),
        egui::FontId::proportional(字号),
        palette.ink_3,
    );
    job.wrap.max_width = tokens.layout.menu_head_width.min(宽 - 2.0 * 左右);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    let galley = ui.painter().layout_job(job);
    let 高 = 上 + galley.size().y + 下;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(宽, 高), egui::Sense::hover());
    ui.painter().galley(
        egui::pos2(rect.left() + 左右, rect.top() + 上),
        galley,
        palette.ink_3,
    );
}

/// 菜单里那道分隔线（设计稿 `.ctx hr` 的 `margin:4px 2px`）。
fn 分隔线(ui: &mut egui::Ui, 宽: f32) {
    let tokens = Tokens::builtin();
    let [上下, 缩进] = tokens.space.menu_rule_margin;
    let palette = look::palette(ui);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(宽, 2.0 * 上下 + 1.0), egui::Sense::hover());
    ui.painter().hline(
        (rect.left() + 缩进)..=(rect.right() - 缩进),
        rect.center().y,
        egui::Stroke::new(1.0, palette.line),
    );
}

/// 菜单上一项：那句话靠左、提示靠右，停上去整项换强调色底（设计稿 `.ctx button`）。
///
/// **右边那一列提示的右缘是同一条线**：每一项都占满整个菜单的内宽（`宽`），提示一律
/// 贴着 `右缘 − menu-item-padding` 摆——那一列因此立得住，不随各项那句话有多长参差
/// （设计稿 `.ctx button span` 的 `margin-left:auto`）。截图门那一条钉的就是它。
fn 一项(ui: &mut egui::Ui, item: &Item, 宽: f32) -> bool {
    let tokens = Tokens::builtin();
    let palette = look::palette(ui);
    let 左右 = tokens.space.menu_item_padding;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(宽, tokens.layout.menu_item_height),
        egui::Sense::click(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, item.label.as_str())
    });
    let 停着 = response.hovered();
    if 停着 {
        ui.painter()
            .rect_filled(rect, tokens.radius.small, palette.accent_soft);
    }
    let 字色 = if 停着 {
        palette.accent_ink
    } else {
        palette.ink
    };
    let 话 = ui.painter().layout_no_wrap(
        item.label.clone(),
        egui::FontId::proportional(正文字号(ui.ctx())),
        字色,
    );
    ui.painter().galley(
        egui::pos2(rect.left() + 左右, rect.center().y - 话.size().y / 2.0),
        话,
        字色,
    );
    if !item.hint.is_empty() {
        let 提示色 = if 停着 {
            palette.accent_ink
        } else {
            palette.ink_3
        };
        let 提示 = ui.painter().layout_no_wrap(
            item.hint.to_owned(),
            egui::FontId::monospace(提示字号(ui.ctx())),
            提示色,
        );
        ui.painter().galley(
            egui::pos2(
                rect.right() - 左右 - 提示.size().x,
                rect.center().y - 提示.size().y / 2.0,
            ),
            提示,
            提示色,
        );
    }
    response.clicked()
}
