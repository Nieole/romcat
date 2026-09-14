//! 五屏共用的**观感基线**：两套主题的颜色与字号、置信度四档标成什么样、键盘焦点长什么样。
//!
//! ## 颜色与字号只从令牌来
//!
//! 亮暗两套 `Visuals` 与字号表由 [`install`] 从**令牌**（[`crate::tokens`]，同目录的
//! `tokens.toml`）装上去，**每一个颜色都取自令牌**：底色、描边、字色、选中、焦点、警告与错误。
//! `Visuals` 里没有槽位的颜色：置信度四档由 [`tier_color`]、弹层遮罩由 [`scrim`] 按主题挑，
//! 主按钮那一档由 [`primary_button`] 换上；平台色、视频播放标这类
//! 两套主题共用、没有映射可言的，直接问令牌（[`Tokens::builtin`] 的 `color.platform`、
//! `color.video`）。**界面里别处不写一个颜色**，要颜色就问 `ui.visuals()`、[`tier_color`]
//! 或令牌。
//!
//! 槽位怎么对上令牌：
//!
//! | 令牌 | egui 槽位 |
//! |---|---|
//! | `win` | `panel_fill`（窗口底色） |
//! | `panel` | `window_fill`；不可交互、未激活两档的控件底；展开那一档的底 |
//! | `panel-2` | `faint_bg_color`（表头、条纹行） |
//! | `sunken` | `extreme_bg_color`、文本框底、代码底；悬停、按下两档的控件底；展开那一档的弱底 |
//! | `ink` | 按钮字；悬停、按下、展开三档的字——也就是**强调字**（`strong_text_color`） |
//! | `ink-2` | 正文（不可交互那一档的字） |
//! | `ink-3` | 弱字（`weak_text_color`）、悬停那一档的描边 |
//! | `ink-4` | 输入法组字里没在转换的那几段下划线 |
//! | `line` | 分隔线（不可交互那一档的描边） |
//! | `line-2` | 未激活、展开两档的描边，弹窗描边 |
//! | `accent` | 选中的字与描边、**键盘焦点**、文本光标、组字下划线；主按钮未激活那一档的底与描边 |
//! | `accent-hover` | 主按钮悬停、按下两档的底 |
//! | `on-accent` | 主按钮上的字，主按钮拿到焦点那一档的描边 |
//! | `accent-soft` | 选中的底色 |
//! | `accent-ink` | 链接 |
//! | `pop-color` | 窗口与弹出菜单的阴影（`window_shadow`、`popup_shadow`；形状照令牌 `[shadow.pop]`） |
//! | `mid` / `lo` | `warn_fg_color` / `error_fg_color` |
//!
//! **正文是 `ink-2`、强调字是 `ink`**：中文没有粗体（票 `gui-looks-like-the-design/02`），
//! 一句纯中文的小标题与正文之间只剩颜色这一层差别——两个都给 `ink`，那一层也没了。
//!
//! **线宽只有一处从令牌来：控件那一圈描边**（令牌 `control-stroke`，未激活、悬停、按下、展开四档一样宽）。
//! egui 原样里未激活那一档是 0、其余三档是 1，于是次要按钮没有边框；设计稿 `.btn` 是一圈 1px 的
//! `line-2`——拿主意的人 2026-09-14 看过开场的候选基线之后裁：照稿补上（票 `gui-looks-like-the-design/05`）。
//! 四档必须一样宽，理由见下面「键盘焦点」一节；别的线宽照 egui 原样。屏的版式由各屏自己的票重排。
//!
//! **按钮三档的高与左右留白也从令牌来**（默认 `button-height` / `button-padding`，小号
//! `button-small-*`，大号 `button-large-*`，照设计稿 `.btn` / `.btn.sm` / `.btn.lg`）。egui 原样的按钮
//! 只有 18 点高、左右各 4 点，比稿上矮一截——拿主意的人 2026-09-14 **第二次**看过开场的候选基线之后裁：
//! 照稿加高、加宽，小号单列一档（票 `gui-looks-like-the-design/05`）。默认那一档由 [`install`] 装上，
//! 小号与大号在一块里换（[`small_buttons`]、[`large_buttons`]）。egui 的横排一行最矮就是按钮那么高，
//! 于是摆着按钮的那几行跟着一起高。
//!
//! **单行输入框的高与左右留白、开场主库列表每一行的间距也从令牌来**（`input-height` /
//! `input-small-height` / `input-padding`；`catalog-row-padding` / `catalog-row-gap`）——拿主意的人
//! 2026-09-14 **第三次**看过候选基线之后裁：输入框加高到与同一行那一档按钮等高（设计稿 `.input` 写的是
//! 30，不照它），主库列表那几行照稿 `.catrow` 收紧，行内的「打开」用小号按钮、不把行撑高
//! （票 `gui-looks-like-the-design/05`）。输入框的尺寸由 [`text_input`]、[`small_text_input`] 换上。
//! 圆角照令牌：控件 `medium`，窗口、弹窗与菜单 `large`。阴影照令牌 `[shadow.pop]` 那一节，颜色
//! `pop-color`（设计稿 `--pop`）。
//!
//! 字号六档：egui 自带五档照它的名字对上（`Small` 说明文字、`Body` 与 `Button` 正文、
//! `Heading` 页面标题、`Monospace` 与正文同大），另三档挂成具名档 [`CAPTION`] / [`TITLE`] /
//! [`HERO`]，取法是 `egui::TextStyle::Name(look::TITLE.into())`。
//!
//! 这一层里一条领域判断都没有（ADR-0005）。「哪一档」是核心库说的
//! （[`Tier`]），「叫什么」也是核心库说的（[`Tier::label`]）；这儿只回答一个纯画法的
//! 问题——**那一档上什么颜色**——并且**只在这一处回答**。
//!
//! ## 为什么要收在一处
//!
//! 收之前，同一件事在三处各写了一份：待确认屏一个 `tier_color` 加 [`Tier::label`]，
//! 浏览屏的变体行手写「高 / 中 / 低 / 还没识别」，主列表那一栏又是一个
//! `match work.confidence`。于是**同一个变体在两屏上是两个词**——右边详情面板写「高」，
//! 中间那张表写「高置信」。颜色的意思是学出来的，学的前提是它到处一样
//! （挂单 `Q83`，票 `gui-redesign/12` 收）。
//!
//! 那第四档**今天叫「没有候选」**：手写那一版里它叫「还没识别」，而那四个字归词表另一条
//! （「一个变体连识别都还没跑过」）——两件事撞在一个词上，待确认屏的屏头就没法把两个数
//! 并排说出来（票 `gui-redesign/17`）。
//!
//! ## 颜色不是唯一线索
//!
//! 每一处上色的地方**都跟着那个词**：色条旁边有标签、被染色的数字后面有档名。
//! 色觉障碍下颜色全糊成一片，那时读得出来的只有字。这条不是建议，是这一票的验收。
//!
//! 这也是**四档颜色够用**的原因（票 `gui-redesign/17`）：浏览那一侧要分开印
//! **没有候选**与**还没识别**两个词，可那两件事的区别由词说——加第五个颜色只会让
//! 一屏上要认的颜色多一个，而认颜色本来就是这一层想少要求的事。
//!
//! ## 键盘焦点
//!
//! egui 把「拿到焦点」与「正被按下」并成同一档 `WidgetVisuals`（`widgets.active`），
//! 默认那一档的描边在暗色主题里是白的、亮色里是黑的——与**悬停**那一档的灰只差一点点。
//! [`install`] 把它换成主题自己的**强调色**（`selection.stroke`），也就是 `TextEdit`
//! 得到焦点时用的那一串：于是按钮、勾选框、可选标签与文本框**得到焦点的样子是同一个**。
//!
//! **四档一样宽，宽度取令牌 `control-stroke`。** egui 把描边宽度反过来从按钮的内边距里扣
//! （`Style::button_style`），哪一档比别的档宽，控件就会在进出那一档的那一帧缩一下——焦点在
//! 控件之间跳的时候，整排按钮跟着抖。

use egui::Color32;
use romcat_core::catalog::identify::Tier;

use crate::tokens::{Palette, Tokens};

/// 把这个窗口的**观感基线**装上去。开窗那一路与不开窗跑帧那一路走的是同一句
/// （[`crate::app::App::ui`] 每次开头问一遍，只装一次）。
///
/// **亮暗两套主题都装**，各取令牌里的那一套：只装一套的话，人换一次系统主题，
/// 界面就悄悄退回 egui 默认的那一套颜色。
pub fn install(ctx: &egui::Context) {
    install_tokens(ctx, Tokens::builtin());
}

/// 同 [`install`]，只是**这个窗口装过一次就不再装**。
///
/// [`crate::program::Program`] 每帧开头问它一遍：**开场**那一态手上还没有
/// [`crate::app::App`]（装基线的另一处在它的第一帧里），而开场上的弹层——添加主库那条向导
/// ——标题要的是令牌那几档字号（[`TITLE`]），没装的话 egui 当场 panic。
pub fn install_once(ctx: &egui::Context) {
    let 装过 = egui::Id::new("观感基线装过了");
    if ctx
        .data(|data| data.get_temp::<bool>(装过))
        .unwrap_or(false)
    {
        return;
    }
    ctx.data_mut(|data| data.insert_temp(装过, true));
    install(ctx);
}

/// 拿**这一份**令牌装。拆出来是为了让测试拿**改过的**令牌走一遍整条装配：哪一格写死了
/// 颜色——哪怕与令牌同值——改令牌时它不跟着变，测试就抓得到。
fn install_tokens(ctx: &egui::Context, tokens: &Tokens) {
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.style_mut_of(theme, |style| {
            style.visuals = visuals(tokens, theme);
            style.text_styles = text_styles(tokens);
            button_spacing(&mut style.spacing, &tokens.layout);
        });
    }
}

/// 按钮的高与左右留白：**默认那一档**（设计稿 `.btn`），取令牌 `button-height` / `button-padding`。
///
/// egui 的按钮最矮不矮过 `interact_size.y`，左右留白是 `button_padding.x`——两样都换成令牌里那一格，
/// 按钮就是稿上那么高。上下留白取 0：稿上是 `padding:0 12px`，高由那个最矮值撑。
/// **副作用**：egui 的横排一行最矮也是 `interact_size.y`，于是摆着按钮的那几行跟着一起高。
fn button_spacing(spacing: &mut egui::style::Spacing, layout: &crate::tokens::Layout) {
    spacing.interact_size.y = layout.button_height;
    spacing.button_padding = egui::vec2(layout.button_padding, 0.0);
}

/// 在这一块里摆的按钮是**小号**的（设计稿 `.btn.sm`）：高、左右留白取令牌 `button-small-*`，
/// 按钮上的字换成说明字号（令牌 `size-small`）。行内的「打开」、抬头上的「添加主库」「更改…」用它。
///
/// 要先量小号按钮多宽再摆的，用 [`small_button_width`] 在块外量，**别在这一块里量**：这一块是一个
/// `ui.scope`，里头什么都不摆也会在横排里占一格间距，后面摆的东西就被往右推了一格。
pub fn small_buttons<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let tokens = Tokens::builtin();
    let font = egui::FontId::proportional(tokens.font.size_small);
    sized_buttons(
        ui,
        [
            tokens.layout.button_small_height,
            tokens.layout.button_small_padding,
        ],
        Some(font),
        add,
    )
}

/// 在这一块里摆的按钮是**大号**的（设计稿 `.btn.lg`）：高、左右留白取令牌 `button-large-*`。
/// 开场空态那一颗「添加主库」用它。
pub fn large_buttons<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let layout = &Tokens::builtin().layout;
    sized_buttons(
        ui,
        [layout.button_large_height, layout.button_large_padding],
        None,
        add,
    )
}

/// **摆一个单行输入框，默认那一档**：高取令牌 `input-height`（与同一行默认那一档按钮等高）、左右留白取
/// `input-padding`，字上下居中；宽是 `width`（连留白在内的外宽，占满一栏就给 `ui.available_width()`）。
///
/// 收一个搭好提示字、字体的 `TextEdit`，这一处替它摆上去。**高不能交给 `TextEdit` 自己**：egui 0.36 的
/// 单行输入框多高由字的行高加上下留白算出来，`min_size` 只管宽——令牌里的高得靠摆它的那一块给足。
pub fn text_input(ui: &mut egui::Ui, width: f32, edit: egui::TextEdit<'_>) -> egui::Response {
    let layout = &Tokens::builtin().layout;
    sized_input(ui, [width, layout.input_height], layout.input_padding, edit)
}

/// **摆一个小号单行输入框**：高取令牌 `input-small-height`（与小号按钮等高），用在旁边摆着小号按钮的
/// 那一框。其余同 [`text_input`]。
pub fn small_text_input(ui: &mut egui::Ui, width: f32, edit: egui::TextEdit<'_>) -> egui::Response {
    let layout = &Tokens::builtin().layout;
    sized_input(
        ui,
        [width, layout.input_small_height],
        layout.input_padding,
        edit,
    )
}

/// 在一块正好 `[宽, 高]` 的地方里摆这个输入框：左右留这么多、上下不另留（高由那一块撑满）、字上下居中。
fn sized_input(
    ui: &mut egui::Ui,
    size: [f32; 2],
    padding: f32,
    edit: egui::TextEdit<'_>,
) -> egui::Response {
    ui.add_sized(
        size,
        edit.margin(egui::Margin::from(egui::vec2(padding, 0.0)))
            .vertical_align(egui::Align::Center),
    )
}

/// 在一个 `scope` 里把按钮换成这一档：`[高, 左右留白]`，要换字号时连字号一起换。别处不受影响。
///
/// **字号换的是 `override_font_id`，不是 `TextStyle::Button` 那一格**：egui 0.36 的按钮取字只认
/// `override_font_id`，没有就用正文那一档（`Style::widget_style`），`Button` 那一格它不读。所以这一块
/// 里只摆按钮（以及量按钮多宽）——摆一句说明进来，它也会跟着变小。
fn sized_buttons<R>(
    ui: &mut egui::Ui,
    [height, padding]: [f32; 2],
    font: Option<egui::FontId>,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.scope(|ui| {
        let spacing = ui.spacing_mut();
        spacing.interact_size.y = height;
        spacing.button_padding.x = padding;
        if let Some(font) = font {
            ui.style_mut().override_font_id = Some(font);
        }
        add(ui)
    })
    .inner
}

/// 具名字号「角标、分组标题」（令牌 `size-caption`）在 egui 里的名字。
///
/// 令牌六档里 egui 没有现成名字的三档挂成具名档，名字与令牌里的键同名（去掉 `size-`）。
/// 取法是 `egui::TextStyle::Name(look::CAPTION.into())`——**用这三个常量，别手写字符串**：
/// 拼错的名字 egui 要到解析那一档字号时才 panic。
pub const CAPTION: &str = "caption";
/// 具名字号「卡片标题、对话框标题」（令牌 `size-title`），取法同 [`CAPTION`]。
pub const TITLE: &str = "title";
/// 具名字号「作品详情页的大标题」（令牌 `size-hero`），取法同 [`CAPTION`]。
pub const HERO: &str = "hero";

/// 字号：**整张换掉**，不在 egui 那张表上增补——留着 egui 自带的哪一档，屏上就有一个字号
/// 不从令牌来。
///
/// egui 自带五档照它的名字对上令牌：`Small` 是说明文字、`Body` 与 `Button` 是正文、
/// `Heading` 是页面标题、`Monospace` 跟正文一样大（等宽只换字族，[`crate::font::mono`]
/// 也是这么做的）。
fn text_styles(tokens: &Tokens) -> std::collections::BTreeMap<egui::TextStyle, egui::FontId> {
    use egui::FontFamily::{Monospace, Proportional};
    use egui::{FontId, TextStyle};
    let font = &tokens.font;
    [
        (TextStyle::Small, FontId::new(font.size_small, Proportional)),
        (TextStyle::Body, FontId::new(font.size_body, Proportional)),
        (TextStyle::Button, FontId::new(font.size_body, Proportional)),
        (
            TextStyle::Heading,
            FontId::new(font.size_page, Proportional),
        ),
        (TextStyle::Monospace, FontId::new(font.size_body, Monospace)),
        (
            TextStyle::Name(CAPTION.into()),
            FontId::new(font.size_caption, Proportional),
        ),
        (
            TextStyle::Name(TITLE.into()),
            FontId::new(font.size_title, Proportional),
        ),
        (
            TextStyle::Name(HERO.into()),
            FontId::new(font.size_hero, Proportional),
        ),
    ]
    .into()
}

/// 那一套主题的 `Visuals`：**每一个颜色都取自令牌**；控件那一圈描边的宽也取令牌；别的不是颜色的
/// （其余线宽、手柄形状……）照 egui 原样。
///
/// **控件四档的描边一样宽**（令牌 `control-stroke`）。egui 把描边宽度从按钮的内边距里扣，而「不画框
/// 的那一档」只留内边距不画描边——哪一档比别的档宽，可选标签就会在悬停前后差一个点。egui 原样里
/// 未激活那一档是 0、其余三档是 1：次要按钮于是没有边框，而设计稿 `.btn` 有一圈 `line-2`
/// （拿主意的人 2026-09-14 裁：照稿补上，票 `gui-looks-like-the-design/05`）。
fn visuals(tokens: &Tokens, theme: egui::Theme) -> egui::Visuals {
    let p = tokens.color.theme(theme);
    let mut v = theme.default_visuals();
    let medium = egui::CornerRadius::same(tokens.radius.medium);
    let large = egui::CornerRadius::same(tokens.radius.large);

    // 每一档：底色、弱底色（按钮底）、描边、前景（字）。
    let paint = |widget: &mut egui::style::WidgetVisuals,
                 [bg, weak_bg, stroke, fg]: [Color32; 4]| {
        widget.bg_fill = bg;
        widget.weak_bg_fill = weak_bg;
        widget.bg_stroke.color = stroke;
        widget.fg_stroke.color = fg;
        widget.corner_radius = medium;
    };
    paint(
        &mut v.widgets.noninteractive,
        [p.panel, p.panel, p.line, p.ink_2],
    );
    paint(&mut v.widgets.inactive, [p.panel, p.panel, p.line_2, p.ink]);
    paint(&mut v.widgets.hovered, [p.sunken, p.sunken, p.ink_3, p.ink]);
    paint(&mut v.widgets.active, [p.sunken, p.sunken, p.accent, p.ink]);
    paint(&mut v.widgets.open, [p.panel, p.sunken, p.line_2, p.ink]);
    // 控件四档的描边一样宽，宽度取令牌（设计稿 `.btn` / `.input` 那一圈 1px；颜色是上面各档自己那一格）。
    let 线宽 = tokens.layout.control_stroke;
    for widget in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        widget.bg_stroke.width = 线宽;
    }

    v.override_text_color = None;
    v.weak_text_color = Some(p.ink_3);
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke.color = p.accent;
    v.ime_composition.active_underline_stroke.color = p.accent;
    v.ime_composition.inactive_underline_stroke.color = p.ink_4;
    v.hyperlink_color = p.accent_ink;
    v.faint_bg_color = p.panel_2;
    v.extreme_bg_color = p.sunken;
    v.text_edit_bg_color = Some(p.sunken);
    v.code_bg_color = p.sunken;
    v.warn_fg_color = p.mid;
    v.error_fg_color = p.lo;
    v.window_corner_radius = large;
    v.window_fill = p.panel;
    v.window_stroke.color = p.line_2;
    v.menu_corner_radius = large;
    // 弹层与弹出菜单的阴影：形状照令牌 `[shadow.pop]` 那一节，颜色是这一套主题的 `pop-color`。
    let shape = tokens.shadow.pop;
    let pop = egui::Shadow {
        offset: shape.offset,
        blur: shape.blur,
        spread: shape.spread,
        color: p.pop_color,
    };
    v.window_shadow = pop;
    v.popup_shadow = pop;
    v.panel_fill = p.win;
    v.text_cursor.stroke.color = p.accent;
    v
}

/// **主按钮**那一档：把这块 `Visuals` 里按钮画得到的三档（未激活、悬停、按下与拿到焦点）换成
/// 强调色底、`on-accent` 字。**全窗口只有这一处回答「主按钮什么颜色」**；画它的是
/// [`crate::dialog`] 页脚上标了 `primary` 的那一颗。
///
/// 在一个 `ui.scope` 里改 `ui.visuals_mut()`，别处的控件不受影响。**拿到焦点那一档的描边换成
/// `on-accent`**：强调色底上再描一圈强调色是看不见的，而焦点得看得见（模块文档「键盘焦点」一节）。
pub fn primary_button(visuals: &mut egui::Visuals) {
    let theme = egui::Theme::from_dark_mode(visuals.dark_mode);
    primary_button_in(Tokens::builtin().color.theme(theme), visuals);
}

/// 主按钮在这一套颜色里取哪几个。拆出来的理由同 [`tier_color_in`]。
fn primary_button_in(palette: &Palette, visuals: &mut egui::Visuals) {
    let widgets = &mut visuals.widgets;
    // 每一档：底色、描边。字一律 `on-accent`。
    for (widget, fill, stroke) in [
        (&mut widgets.inactive, palette.accent, palette.accent),
        (
            &mut widgets.hovered,
            palette.accent_hover,
            palette.accent_hover,
        ),
        (&mut widgets.active, palette.accent_hover, palette.on_accent),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke.color = stroke;
        widget.fg_stroke.color = palette.on_accent;
    }
}

/// **置信度四档**画成什么颜色。**全窗口只有这一处回答这个问题。**
///
/// 颜色取自令牌的 `hi` / `mid` / `lo` / `none`，按 `visuals` 是哪一套主题挑那一套：
/// 亮色与暗色主题下同一串 RGB 不是同一个可读性，令牌两套各配一份。
///
/// **不再借 egui 的 `selection` / `warn` / `error` / `weak`**：那几个是别的意思的颜色——
/// 从前高置信借的是选中色，于是改一处选中色，高置信就悄悄跟着变。
#[must_use]
pub fn tier_color(tier: Tier, visuals: &egui::Visuals) -> Color32 {
    let theme = egui::Theme::from_dark_mode(visuals.dark_mode);
    tier_color_in(Tokens::builtin().color.theme(theme), tier)
}

/// 那一档在这一套颜色里取哪一个。拆出来是为了让测试拿**改过的**令牌来问：
/// 这里写死一个颜色（哪怕与令牌同值），改令牌时它不跟着变，测试就抓得到。
fn tier_color_in(palette: &Palette, tier: Tier) -> Color32 {
    match tier {
        Tier::High => palette.hi,
        Tier::Medium => palette.mid,
        Tier::Low => palette.lo,
        Tier::Unidentified => palette.none,
    }
}

/// 弹层底下那层**遮罩**画成什么颜色（令牌 `scrim`，半透明）。**全窗口只有这一处回答。**
///
/// `Visuals` 里没有这个槽位，于是与置信度四档同一个办法：按 `visuals` 是哪一套主题挑那一套。
/// 画它的只有 [`crate::dialog`]。
#[must_use]
pub fn scrim(visuals: &egui::Visuals) -> Color32 {
    let theme = egui::Theme::from_dark_mode(visuals.dark_mode);
    scrim_in(Tokens::builtin().color.theme(theme))
}

/// 遮罩在这一套颜色里取哪一个。拆出来的理由同 [`tier_color_in`]。
fn scrim_in(palette: &Palette) -> Color32 {
    palette.scrim
}

/// 一枚**标签**说的是好事还是要留神（设计稿 `.ro`「只读」、`.chip.t-mid`「版本不兼容」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// 放心：令牌 `hi` 那一对（`hi` 的字、`hi-soft` 的底）。
    Good,
    /// 留神：令牌 `mid` 那一对。
    Caution,
    /// 要紧：令牌 `lo` 那一对（设计稿 `.chip.t-lo`「待确认」）。
    Risk,
    /// 不置可否：令牌 `none` 那一对（设计稿 `.chip.t-none`「仅文件名」）。
    Neutral,
}

/// 一档**置信度**的标签用哪种语气：高置信放心、中置信留神、低置信要紧、没有候选不置可否
/// （设计稿 `.t-hi` / `.t-mid` / `.t-lo` / `.t-none`）。字那一格与 [`tier_color`] 取的是同一个令牌。
#[must_use]
pub fn tier_tone(tier: Tier) -> Tone {
    match tier {
        Tier::High => Tone::Good,
        Tier::Medium => Tone::Caution,
        Tier::Low => Tone::Risk,
        Tier::Unidentified => Tone::Neutral,
    }
}

/// 那种语气的标签画成什么颜色：`(字, 底)`。**全窗口只有这一处回答。**
///
/// 借的是与置信度四档同一对令牌（令牌里 `hi` / `mid` 本来就写着「高置信 / 成功」「中置信 /
/// 提醒」），但**不走 [`tier_color`]**：「只读」不是一档置信度，改置信度颜色的人不该顺手改了它。
#[must_use]
pub fn tone_colors(tone: Tone, visuals: &egui::Visuals) -> (Color32, Color32) {
    let theme = egui::Theme::from_dark_mode(visuals.dark_mode);
    tone_colors_in(Tokens::builtin().color.theme(theme), tone)
}

/// 标签在这一套颜色里取哪一对。拆出来的理由同 [`tier_color_in`]。
fn tone_colors_in(palette: &Palette, tone: Tone) -> (Color32, Color32) {
    match tone {
        Tone::Good => (palette.hi, palette.hi_soft),
        Tone::Caution => (palette.mid, palette.mid_soft),
        Tone::Risk => (palette.lo, palette.lo_soft),
        Tone::Neutral => (palette.none, palette.none_soft),
    }
}

/// 间距档位里的第 `at` 档（令牌 `space.steps`，从窄到宽，从 0 数）。**版式里的间距从这儿取。**
///
/// 档位不够时交回 0：宁可挤一点，也不在画帧时 panic。
#[must_use]
pub fn step(at: usize) -> f32 {
    Tokens::builtin()
        .space
        .steps
        .get(at)
        .copied()
        .unwrap_or_default()
}

/// 一枚**标签**：浅底小圆角，同色的圆点与字（设计稿 `.chip`）。圆点直径取令牌 `chip-dot`。
pub fn chip(ui: &mut egui::Ui, tone: Tone, text: &str) -> egui::Response {
    let (字色, 底色) = tone_colors(tone, ui.visuals());
    let galley = egui::WidgetText::from(egui::RichText::new(text).small().color(字色)).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Small,
    );
    let (边, 点, 缝) = (step(1), Tokens::builtin().layout.chip_dot, step(0));
    let 字 = galley.size();
    let size = egui::vec2(边 + 点 + 缝 + 字.x + 边, 字.y + step(0));
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, Tokens::builtin().radius.small, 底色);
    painter.circle_filled(
        egui::pos2(rect.left() + 边 + 点 / 2.0, rect.center().y),
        点 / 2.0,
        字色,
    );
    let 摆在 = egui::pos2(rect.left() + 边 + 点 + 缝, rect.center().y - 字.y / 2.0);
    painter.galley(摆在, galley, 字色);
    response
}

/// 一张**卡片**：面板底、分隔线色描边、大圆角（设计稿 `.card`）。`padding` 是（左右, 上下）。
pub fn card<R>(
    ui: &mut egui::Ui,
    padding: egui::Vec2,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let (底色, 描边) = (
        ui.visuals().window_fill,
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    egui::Frame::new()
        .fill(底色)
        .stroke(描边)
        .corner_radius(Tokens::builtin().radius.large)
        .inner_margin(egui::Margin::from(padding))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
}

/// 一条**提示条**（设计稿 `.note`）：次级底色、分隔线色描边、中圆角，占满这一栏的宽。
pub fn note(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) {
    let (底色, 描边) = (
        ui.visuals().faint_bg_color,
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    egui::Frame::new()
        .fill(底色)
        .stroke(描边)
        .corner_radius(Tokens::builtin().radius.medium)
        .inner_margin(egui::Margin::from(egui::vec2(step(2), step(1))))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(text);
        });
}

/// 一块**提示框**（设计稿 `.note`）：次级底色、分隔线色描边、中圆角，内边距取令牌 `note-padding`，
/// 里头的字是 `size-small-plus` 那一档，占满这一栏的宽。
///
/// 与 [`note`] 是同一个样子，只是**里头摆什么由调用方给**（判定依据那几行、一句说明）。[`note`] 的留白
/// 没照令牌走，开场与添加主库向导的基线里画着它，所以不动它，另起这一个。
pub fn note_box<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let tokens = Tokens::builtin();
    let [上下, 左右] = tokens.space.note_padding;
    let (底色, 描边) = (
        ui.visuals().faint_bg_color,
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    egui::Frame::new()
        .fill(底色)
        .stroke(描边)
        .corner_radius(tokens.radius.medium)
        .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.style_mut().override_font_id =
                Some(egui::FontId::proportional(tokens.font.size_small_plus));
            add(ui)
        })
        .inner
}

/// 一颗**图标按钮**（设计稿 `.iconbtn`）：边长取令牌 `icon-button` 的方块，平时透明无框、字是弱字色；
/// 悬停或拿到焦点时铺凹陷底、描一圈分隔线、字换成强调字。收起、展开那两颗箭头用它。
///
/// 自己画而不走 `egui::Button`：按钮最矮也有令牌 `button-height` 那么高、两边还留着 `button-padding`，
/// 塞不进 36 点宽的窄条（设计稿 `.strip`）。
pub fn icon_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let tokens = Tokens::builtin();
    let 边长 = tokens.layout.icon_button;
    let enabled = ui.is_enabled();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(边长, 边长), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, text));
    let visuals = ui.visuals();
    let 亮 = response.hovered() || response.has_focus();
    if 亮 {
        ui.painter().rect(
            rect,
            tokens.radius.small,
            visuals.extreme_bg_color,
            visuals.widgets.noninteractive.bg_stroke,
            egui::StrokeKind::Inside,
        );
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            tokens.radius.small,
            egui::Stroke::new(tokens.layout.control_stroke, visuals.selection.stroke.color),
            egui::StrokeKind::Inside,
        );
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(tokens.font.size_body),
        if 亮 {
            visuals.strong_text_color()
        } else {
            visuals.weak_text_color()
        },
    );
    response
}

/// 一颗**幽灵按钮**（设计稿 `.btn.ghost`）：平时没有底也没有框，悬停、按下时才露出按钮那一层。
/// 高与留白跟着摆它的那一块走——浏览屏左栏标题行那颗「清除」摆在 [`small_buttons`] 里。
pub fn ghost_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(text).frame_when_inactive(false))
}

/// 一枚**分面标签**（设计稿 `.fchip`）：一个值加上它的条数，点一下收窄到这个值、再点一下放开。
///
/// 高、左右留白、值与条数之间取令牌 `facet-chip-height` / `facet-chip-padding` / `facet-chip-gap`；
/// 值是说明字号（`size-small`），条数是等宽的 `size-mini`、弱字色。平时铺面板底色、描一圈分隔线；
/// 选中时换成强调那一套——描边 `accent`、底 `accent-soft`、字 `accent-ink`，拉丁与数字加粗。
pub fn facet_chip(ui: &mut egui::Ui, selected: bool, value: &str, count: &str) -> egui::Response {
    let tokens = Tokens::builtin();
    let (字色, 数色, 底色, 描边色, 焦点色) = {
        let visuals = ui.visuals();
        let 焦点色 = visuals.selection.stroke.color;
        if selected {
            (
                visuals.hyperlink_color,
                visuals.hyperlink_color,
                visuals.selection.bg_fill,
                焦点色,
                焦点色,
            )
        } else {
            (
                visuals.text_color(),
                visuals.weak_text_color(),
                visuals.window_fill,
                visuals.widgets.noninteractive.bg_stroke.color,
                焦点色,
            )
        }
    };
    let 字族 = if selected {
        crate::font::strong_family()
    } else {
        egui::FontFamily::Proportional
    };
    let 值 = ui.painter().layout_no_wrap(
        value.to_owned(),
        egui::FontId::new(tokens.font.size_small, 字族),
        字色,
    );
    let 数 = ui.painter().layout_no_wrap(
        count.to_owned(),
        egui::FontId::monospace(tokens.font.size_mini),
        数色,
    );
    let 边 = tokens.layout.facet_chip_padding;
    let 缝 = if count.is_empty() {
        0.0
    } else {
        tokens.space.facet_chip_gap
    };
    let (值宽, 值高, 数高) = (值.size().x, 值.size().y, 数.size().y);
    let enabled = ui.is_enabled();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(
            边 + 值宽 + 缝 + 数.size().x + 边,
            tokens.layout.facet_chip_height,
        ),
        egui::Sense::click(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            enabled,
            selected,
            format!("{value} {count}").trim_end(),
        )
    });
    let painter = ui.painter();
    painter.rect(
        rect,
        tokens.radius.small,
        底色,
        egui::Stroke::new(tokens.layout.control_stroke, 描边色),
        egui::StrokeKind::Inside,
    );
    painter.galley(
        egui::pos2(rect.left() + 边, rect.center().y - 值高 / 2.0),
        值,
        字色,
    );
    painter.galley(
        egui::pos2(rect.left() + 边 + 值宽 + 缝, rect.center().y - 数高 / 2.0),
        数,
        数色,
    );
    if response.has_focus() {
        painter.rect_stroke(
            rect,
            tokens.radius.small,
            egui::Stroke::new(2.0 * tokens.layout.control_stroke, 焦点色),
            egui::StrokeKind::Inside,
        );
    }
    response
}

/// 分面标签那一簇末尾那颗「更多（N）」/「收起」（设计稿 `.fmore`）：与分面标签一样高、一样的留白，
/// 描一圈虚线（令牌 `line-2`），字是说明字号、弱字色，悬停时换成强调字。
pub fn more_chip(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let tokens = Tokens::builtin();
    let (弱, 强, 线色) = {
        let visuals = ui.visuals();
        (
            visuals.weak_text_color(),
            visuals.strong_text_color(),
            visuals.widgets.inactive.bg_stroke.color,
        )
    };
    let 字 = ui.painter().layout_no_wrap(
        text.to_owned(),
        egui::FontId::proportional(tokens.font.size_small),
        弱,
    );
    let 边 = tokens.layout.facet_chip_padding;
    let 字大小 = 字.size();
    let enabled = ui.is_enabled();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(边 + 字大小.x + 边, tokens.layout.facet_chip_height),
        egui::Sense::click(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, text));
    let 线宽 = tokens.layout.control_stroke;
    let 框 = rect.shrink(线宽 / 2.0);
    // 虚线一段多长、两段之间空多少：浏览器画 `dashed` 大约是线宽的三倍，这里照那个比例。
    let 段 = 3.0 * 线宽;
    let painter = ui.painter();
    painter.extend(egui::Shape::dashed_line(
        &[
            框.left_top(),
            框.right_top(),
            框.right_bottom(),
            框.left_bottom(),
            框.left_top(),
        ],
        egui::Stroke::new(线宽, 线色),
        段,
        段,
    ));
    painter.galley_with_override_text_color(
        rect.center() - 字大小 / 2.0,
        字,
        if response.hovered() { 强 } else { 弱 },
    );
    response
}

/// 一栏的**小标题**（设计稿 `.sec`）：说明字号、弱字色。稿上还加粗，眼下没照，画得与 [`help`] 一样（挂单 `Q770`）。
pub fn section(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.label(egui::RichText::new(text).small().weak())
}

/// 一行**帮助字**（设计稿 `.help`）：说明字号、弱字色，摆在它说的那样东西底下。
pub fn help(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.label(egui::RichText::new(text).small().weak())
}

/// 两段之间那条一点宽的**分隔线**，颜色取不可交互那一档的描边（令牌 `line`）。
pub fn divider(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
}

/// 一颗写着 `label` 的按钮画出来多宽：字宽加两边的内边距，与 `egui::Button` 自己量的一样。
///
/// 要把按钮摆在一行的右头、先替它留出地方时用。**字照按钮取字的规矩取**：`override_font_id`，
/// 没有就用正文那一档（egui 0.36 的 `Style::widget_style`）——小号按钮那一块里换的正是前者
/// （[`small_buttons`]），量的与摆的才是同一个字号。描边不另算：egui 把描边宽从内边距里扣了。
#[must_use]
pub fn button_width(ui: &egui::Ui, label: &str) -> f32 {
    let font = ui
        .style()
        .override_font_id
        .clone()
        .unwrap_or_else(|| egui::TextStyle::Body.resolve(ui.style()));
    measured_button_width(ui, label, font, ui.spacing().button_padding.x)
}

/// 一颗写着 `label` 的**小号**按钮（[`small_buttons`]）画出来多宽：字取令牌 `size-small`，左右留白取
/// `button-small-padding`。
///
/// **量小号按钮只用它，别在 [`small_buttons`] 那一块里用 [`button_width`] 量**：那一块是一个 `ui.scope`，
/// 哪怕里头什么都不摆，它也在横排里占一格间距——量的那一下就把后面摆的东西往右推了一格。
#[must_use]
pub fn small_button_width(ui: &egui::Ui, label: &str) -> f32 {
    let tokens = Tokens::builtin();
    measured_button_width(
        ui,
        label,
        egui::FontId::proportional(tokens.font.size_small),
        tokens.layout.button_small_padding,
    )
}

/// 按钮多宽：这一个字号下的字宽，加两边各一份留白。描边不另算：egui 把描边宽从内边距里扣了。
fn measured_button_width(ui: &egui::Ui, label: &str, font: egui::FontId, padding: f32) -> f32 {
    let galley = egui::WidgetText::from(label).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        font,
    );
    galley.size().x + 2.0 * padding
}

/// 行左边缘那条**置信度色条**：宽取令牌 `tier-bar`、与一行正文一样高。
///
/// 它**从不单独出现**——摆它的地方旁边一定跟着 [`tier_label`] 或那个词本身
/// （[`tier_tag`] 把两样一起给出来）。
pub fn tier_bar(ui: &mut egui::Ui, tier: Tier) {
    let height = ui.text_style_height(&egui::TextStyle::Body);
    let width = Tokens::builtin().layout.tier_bar;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 1.0, tier_color(tier, ui.visuals()));
}

/// 那一档的**词**，染成那一档的颜色。词来自 [`Tier::label`]，一个字都不在这里写。
pub fn tier_label(ui: &mut egui::Ui, tier: Tier) -> egui::Response {
    ui.colored_label(tier_color(tier, ui.visuals()), tier.label())
}

/// 一处完整的**置信度记号**：色条 ＋ 那个词。
///
/// 两样一起给，是因为**分开给就会有人只给一半**——那正是「颜色不是唯一线索」失守的
/// 唯一走法。返回的是词那一块的 `Response`，挂悬停用。
pub fn tier_tag(ui: &mut egui::Ui, tier: Tier) -> egui::Response {
    tier_bar(ui, tier);
    tier_label(ui, tier)
}

/// 给一个**自己画底色的可点件**补上焦点那一圈：表格的行、缩略图那几格。
///
/// [`install`] 换的那圈强调色只到得了走 egui 按钮那条路的控件（按钮、勾选框、可选标签、
/// 文本框）。表格的行自己画底色（`TableRow::set_selected`），缩略图那几格自己画边框
/// ——焦点落上去一点动静都没有，而它们**是点得中的**，于是 Tab 走得到
/// （`egui::Sense::click` 自带 `FOCUSABLE`）。走得到又看不见，是最坏的一种。
///
/// 收一个 `Response` 而不是 `&mut Ui`：`egui_extras::TableRow::response` 要等列都加完
/// 才拿得到，那时手上已经没有那一行的 `Ui` 了。画在哪一层由 `Response` 自己说
/// （`layer_id`），描的这一圈**压在那一行的内容上头**。
///
/// `visible` 是**看得见的那一块**——表格里传的是格子的裁剪矩形，那一半正是滚动视口的
/// 上下沿。只取它的**竖直**范围：表格的行是虚拟化的，最上和最下那一行常常只露半截，
/// 而 `Response::rect` 是整行；不夹的话，那 2 点宽的上沿会画到表头上去
/// （表头先画、这一圈后画，盖得住）。横向那一半不能拿来夹——它是**这一列**的宽，
/// 拿它去夹整行会把右边那几列裁掉。
pub fn focus_ring(ctx: &egui::Context, visible: egui::Rect, response: &egui::Response) {
    if !response.has_focus() {
        return;
    }
    let rect = response.rect.intersect(egui::Rect::from_x_y_ranges(
        response.rect.x_range(),
        visible.y_range(),
    ));
    if !rect.is_positive() {
        return;
    }
    let color = ctx.style_of(ctx.theme()).visuals.selection.stroke.color;
    egui::Painter::new(ctx.clone(), response.layer_id, rect).rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(2.0, color),
        egui::StrokeKind::Inside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless;
    use egui::{CornerRadius, Theme};

    /// 一处与令牌对不上的地方：`(令牌键, 哪儿对不上)`。
    type 偏离 = (String, String);

    /// 令牌里**界面还没有一处用上**的颜色：页面背景（设计稿自己用）、低与没有候选那两档的浅底
    /// ——egui 的 `Visuals` 里没有它们的槽位，而画它们的那几屏还没照稿重排。
    /// 哪一屏第一个用上它，就从这张单子里划掉（下面那条变异测试会提醒）。`hi-soft` 与 `mid-soft`
    /// 由标签（[`tone_colors`]）用上了（票 `gui-looks-like-the-design/05`，开场与添加主库向导）。
    const NOT_YET_USED: &[&str] = &["ground"];

    /// 这套主题下**有映射的每一个颜色**与令牌逐项比，对不上的那几项：`visuals` 的每个颜色槽位，
    /// 加上 `四档`（置信度四档各取哪个颜色）。平台色与播放标直接读令牌、没有映射可接错，
    /// 不在这里比。
    ///
    /// **`Visuals` 整个拆开**（不写 `..`）：egui 升版多长一个字段，这里当场编不过，
    /// 新长出来的颜色槽位没法悄悄留在 egui 的默认值上。不比的字段逐个写明为什么不比。
    ///
    /// **槽位 → 令牌键**这张表是这条测试自己写的，不从 [`install`] 那边抄——
    /// 抄过来就是拿实现去验实现。
    fn 颜色_偏离(
        tokens: &Tokens,
        theme: Theme,
        visuals: &egui::Visuals,
        四档: impl Fn(Tier) -> Color32,
        遮罩: Color32,
        主按钮: &egui::style::Widgets,
        标签: impl Fn(Tone) -> (Color32, Color32),
    ) -> Vec<偏离> {
        let p = tokens.color.theme(theme);
        // `clip_rect_margin` 在 egui 0.36 里弃用了，可整个拆开就得点到它的名字。
        #[allow(deprecated)]
        let egui::Visuals {
            dark_mode,
            // 字形的伽马曲线，不是颜色。
            text_options: _,
            override_text_color,
            // 设了 `weak_text_color` 之后用不上。
            weak_text_alpha: _,
            weak_text_color,
            widgets,
            selection,
            ime_composition,
            hyperlink_color,
            faint_bg_color,
            extreme_bg_color,
            text_edit_bg_color,
            code_bg_color,
            warn_fg_color,
            error_fg_color,
            window_corner_radius,
            window_shadow,
            window_fill,
            window_stroke,
            window_highlight_topmost: _,
            menu_corner_radius,
            panel_fill,
            popup_shadow,
            // 以下都不是颜色也不是圆角。
            resize_corner_size: _,
            text_cursor,
            clip_rect_margin: _,
            button_frame: _,
            collapsing_header_frame: _,
            indent_has_left_vline: _,
            striped: _,
            slider_trailing_fill: _,
            handle_shape: _,
            interact_cursor: _,
            image_loading_spinners: _,
            numeric_color_space: _,
            disabled_alpha: _,
        } = visuals;
        let egui::style::Selection {
            bg_fill: selection_fill,
            stroke: selection_stroke,
        } = selection;
        let egui::style::ImeComposition {
            active_underline_stroke,
            inactive_underline_stroke,
            legacy_visuals: _,
        } = ime_composition;
        let egui::style::TextCursorStyle {
            stroke: cursor_stroke,
            preview: _,
            blink: _,
            on_duration: _,
            off_duration: _,
        } = text_cursor;

        let mut 颜色 = vec![
            (
                "weak_text_color".to_owned(),
                weak_text_color.unwrap_or(Color32::TRANSPARENT),
                "ink-3",
            ),
            (
                "selection.bg_fill".to_owned(),
                *selection_fill,
                "accent-soft",
            ),
            (
                "selection.stroke".to_owned(),
                selection_stroke.color,
                "accent",
            ),
            (
                "ime_composition.active_underline_stroke".to_owned(),
                active_underline_stroke.color,
                "accent",
            ),
            (
                "ime_composition.inactive_underline_stroke".to_owned(),
                inactive_underline_stroke.color,
                "ink-4",
            ),
            ("hyperlink_color".to_owned(), *hyperlink_color, "accent-ink"),
            ("faint_bg_color".to_owned(), *faint_bg_color, "panel-2"),
            ("extreme_bg_color".to_owned(), *extreme_bg_color, "sunken"),
            (
                "text_edit_bg_color".to_owned(),
                text_edit_bg_color.unwrap_or(Color32::TRANSPARENT),
                "sunken",
            ),
            ("code_bg_color".to_owned(), *code_bg_color, "sunken"),
            ("warn_fg_color".to_owned(), *warn_fg_color, "mid"),
            ("error_fg_color".to_owned(), *error_fg_color, "lo"),
            ("window_fill".to_owned(), *window_fill, "panel"),
            ("window_stroke".to_owned(), window_stroke.color, "line-2"),
            ("panel_fill".to_owned(), *panel_fill, "win"),
            (
                "window_shadow.color".to_owned(),
                window_shadow.color,
                "pop-color",
            ),
            (
                "popup_shadow.color".to_owned(),
                popup_shadow.color,
                "pop-color",
            ),
            (
                "text_cursor.stroke".to_owned(),
                cursor_stroke.color,
                "accent",
            ),
        ];
        let mut 圆角 = vec![
            (
                "window_corner_radius".to_owned(),
                *window_corner_radius,
                tokens.radius.large,
            ),
            (
                "menu_corner_radius".to_owned(),
                *menu_corner_radius,
                tokens.radius.large,
            ),
        ];

        let egui::style::Widgets {
            noninteractive,
            inactive,
            hovered,
            active,
            open,
        } = widgets;
        // 每一档：底色、弱底色（按钮）、描边、前景（字）。
        for (name, widget, [bg, weak_bg, stroke, fg]) in [
            (
                "noninteractive",
                noninteractive,
                ["panel", "panel", "line", "ink-2"],
            ),
            ("inactive", inactive, ["panel", "panel", "line-2", "ink"]),
            ("hovered", hovered, ["sunken", "sunken", "ink-3", "ink"]),
            ("active", active, ["sunken", "sunken", "accent", "ink"]),
            ("open", open, ["panel", "sunken", "line-2", "ink"]),
        ] {
            let egui::style::WidgetVisuals {
                bg_fill,
                weak_bg_fill,
                bg_stroke,
                corner_radius,
                fg_stroke,
                // 不是颜色；而且动它会让控件在状态之间抖（模块文档「键盘焦点」一节）。
                expansion: _,
            } = widget;
            颜色.push((format!("widgets.{name}.bg_fill"), *bg_fill, bg));
            颜色.push((
                format!("widgets.{name}.weak_bg_fill"),
                *weak_bg_fill,
                weak_bg,
            ));
            颜色.push((format!("widgets.{name}.bg_stroke"), bg_stroke.color, stroke));
            颜色.push((format!("widgets.{name}.fg_stroke"), fg_stroke.color, fg));
            圆角.push((
                format!("widgets.{name}.corner_radius"),
                *corner_radius,
                tokens.radius.medium,
            ));
        }

        for (tier, key) in [
            (Tier::High, "hi"),
            (Tier::Medium, "mid"),
            (Tier::Low, "lo"),
            (Tier::Unidentified, "none"),
        ] {
            颜色.push((format!("四档（{}）", tier.label()), 四档(tier), key));
        }
        颜色.push(("弹层遮罩".to_owned(), 遮罩, "scrim"));
        // 标签两种语气（[`tone_colors`]）：字与底各一格。
        for (tone, [字键, 底键]) in [
            (Tone::Good, ["hi", "hi-soft"]),
            (Tone::Caution, ["mid", "mid-soft"]),
            (Tone::Risk, ["lo", "lo-soft"]),
            (Tone::Neutral, ["none", "none-soft"]),
        ] {
            let (字, 底) = 标签(tone);
            颜色.push((format!("标签（{tone:?}）的字"), 字, 字键));
            颜色.push((format!("标签（{tone:?}）的底"), 底, 底键));
        }
        // 主按钮三档（[`primary_button`]）：底色、描边；字一律 `on-accent`。拿到焦点那一档的描边是
        // `on-accent`——强调色底上描一圈强调色看不见。
        for (name, widget, [fill, stroke]) in [
            ("inactive", &主按钮.inactive, ["accent", "accent"]),
            ("hovered", &主按钮.hovered, ["accent-hover", "accent-hover"]),
            ("active", &主按钮.active, ["accent-hover", "on-accent"]),
        ] {
            颜色.push((format!("主按钮.{name}.bg_fill"), widget.bg_fill, fill));
            颜色.push((
                format!("主按钮.{name}.weak_bg_fill"),
                widget.weak_bg_fill,
                fill,
            ));
            颜色.push((
                format!("主按钮.{name}.bg_stroke"),
                widget.bg_stroke.color,
                stroke,
            ));
            颜色.push((
                format!("主按钮.{name}.fg_stroke"),
                widget.fg_stroke.color,
                "on-accent",
            ));
        }

        let mut out = Vec::new();
        if *dark_mode != (theme == Theme::Dark) {
            out.push(("(主题)".to_owned(), format!("dark_mode 是 {dark_mode}")));
        }
        if let Some(color) = override_text_color {
            out.push((
                "ink-2".to_owned(),
                format!("override_text_color 是 {color:?}：字色该由 widgets 各档说"),
            ));
        }
        for (槽位, got, key) in 颜色 {
            let want = p.get(key).unwrap_or_else(|| panic!("令牌里没有 {key}"));
            if got != want {
                out.push((
                    key.to_owned(),
                    format!("{槽位} 是 {got:?}，令牌 {key} 是 {want:?}"),
                ));
            }
        }
        for (槽位, got, want) in 圆角 {
            if got != CornerRadius::same(want) {
                out.push((
                    "(圆角)".to_owned(),
                    format!("{槽位} 是 {got:?}，令牌是 {want}"),
                ));
            }
        }
        let 令牌 = &tokens.shadow.pop;
        for (槽位, shadow) in [
            ("window_shadow", window_shadow),
            ("popup_shadow", popup_shadow),
        ] {
            if (shadow.offset, shadow.blur, shadow.spread) != (令牌.offset, 令牌.blur, 令牌.spread)
            {
                out.push((
                    "(阴影)".to_owned(),
                    format!("{槽位} 的形状是 {shadow:?}，令牌是 {令牌:?}"),
                ));
            }
        }
        out
    }

    /// 偏离清单排成人读的几行。
    fn 列(偏离: &[偏离]) -> String {
        偏离
            .iter()
            .map(|(_, what)| what.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 一个与 `color` 一定不同的颜色——变异用。
    fn 另一个颜色(color: Color32) -> Color32 {
        let 哨兵 = Color32::from_rgb(1, 2, 3);
        if color == 哨兵 {
            Color32::from_rgb(3, 2, 1)
        } else {
            哨兵
        }
    }

    /// 走 [`install`] 装好之后，这套主题的 `Visuals` 与内置令牌对不上的那几项。
    fn 装好之后的偏离(theme: Theme) -> Vec<偏离> {
        let ctx = headless::context();
        install(&ctx);
        let style = ctx.style_of(theme);
        let mut 主按钮 = style.visuals.clone();
        primary_button(&mut 主按钮);
        颜色_偏离(
            Tokens::builtin(),
            theme,
            &style.visuals,
            |tier| tier_color(tier, &style.visuals),
            scrim(&style.visuals),
            &主按钮.widgets,
            |tone| tone_colors(tone, &style.visuals),
        )
    }

    /// **变异实测写成测试**：这套主题的令牌里每一个颜色各改一次，**拿改过的令牌走一遍整条装配**
    /// （`install_tokens`，四档走 `tier_color_in`），返回 `(接线断了的键, 没人读的键)`。
    ///
    /// - **接线断了**：装出来的与**改过的**令牌对不上。某一格写死了颜色——哪怕与令牌同值——
    ///   改令牌时它不跟着变，就落在这里。
    /// - **没人读**：装出来的与**没改的**令牌比，一格都没因为这个键报出来。
    fn 变异(theme: Theme) -> (Vec<&'static str>, Vec<&'static str>) {
        let mut 断了 = Vec::new();
        let mut 没人读 = Vec::new();
        for key in Palette::KEYS {
            let mut 改过 = Tokens::builtin().clone();
            let palette = match theme {
                Theme::Dark => &mut 改过.color.dark,
                Theme::Light => &mut 改过.color.light,
            };
            let color = palette.get_mut(key).expect("KEYS 里的键都取得到");
            *color = 另一个颜色(*color);

            let ctx = headless::context();
            install_tokens(&ctx, &改过);
            let 装出来 = ctx.style_of(theme);
            let 四档 = |tier| tier_color_in(改过.color.theme(theme), tier);
            let 遮罩 = scrim_in(改过.color.theme(theme));
            let mut 主按钮 = 装出来.visuals.clone();
            primary_button_in(改过.color.theme(theme), &mut 主按钮);
            let 主按钮 = &主按钮.widgets;
            let 标签 = |tone| tone_colors_in(改过.color.theme(theme), tone);
            if !颜色_偏离(&改过, theme, &装出来.visuals, 四档, 遮罩, 主按钮, 标签).is_empty()
            {
                断了.push(*key);
            }
            if !颜色_偏离(
                Tokens::builtin(),
                theme,
                &装出来.visuals,
                四档,
                遮罩,
                主按钮,
                标签,
            )
            .iter()
            .any(|(报的, _)| 报的 == key)
            {
                没人读.push(*key);
            }
        }
        (断了, 没人读)
    }

    /// 这套主题装上去的字号，与令牌对不上的那几项。
    ///
    /// egui 自带五档，令牌有六档：对得上号的照 egui 的名字装，对不上号的三档（角标、
    /// 卡片标题、大标题）按令牌里的名字挂成具名档。**多出一档也算对不上**。
    fn 字号_偏离(tokens: &Tokens, style: &egui::Style) -> Vec<String> {
        use egui::FontFamily::{Monospace, Proportional};
        use egui::{FontId, TextStyle};
        let font = &tokens.font;
        let want = [
            (TextStyle::Small, FontId::new(font.size_small, Proportional)),
            (TextStyle::Body, FontId::new(font.size_body, Proportional)),
            (TextStyle::Button, FontId::new(font.size_body, Proportional)),
            (
                TextStyle::Heading,
                FontId::new(font.size_page, Proportional),
            ),
            (TextStyle::Monospace, FontId::new(font.size_body, Monospace)),
            (
                TextStyle::Name("caption".into()),
                FontId::new(font.size_caption, Proportional),
            ),
            (
                TextStyle::Name("title".into()),
                FontId::new(font.size_title, Proportional),
            ),
            (
                TextStyle::Name("hero".into()),
                FontId::new(font.size_hero, Proportional),
            ),
        ];
        let mut out = Vec::new();
        for (text_style, font_id) in &want {
            match style.text_styles.get(text_style) {
                Some(got) if got == font_id => {}
                got => out.push(format!("{text_style:?} 是 {got:?}，令牌是 {font_id:?}")),
            }
        }
        for text_style in style.text_styles.keys() {
            if !want.iter().any(|(wanted, _)| wanted == text_style) {
                out.push(format!("多出来一档 {text_style:?}"));
            }
        }
        out
    }

    #[test]
    fn 两套主题的字号取自令牌() {
        let ctx = headless::context();
        install(&ctx);
        for theme in [Theme::Dark, Theme::Light] {
            let 偏离 = 字号_偏离(Tokens::builtin(), &ctx.style_of(theme));
            assert!(偏离.is_empty(), "{theme:?}：\n{}", 偏离.join("\n"));
        }
    }

    #[test]
    fn 暗色主题的颜色逐项取自令牌() {
        let 偏离 = 装好之后的偏离(Theme::Dark);
        assert!(偏离.is_empty(), "暗色主题与令牌对不上：\n{}", 列(&偏离));
    }

    #[test]
    fn 亮色主题的颜色逐项取自令牌() {
        let 偏离 = 装好之后的偏离(Theme::Light);
        assert!(偏离.is_empty(), "亮色主题与令牌对不上：\n{}", 列(&偏离));
    }

    #[test]
    fn 改暗色令牌里任意一个颜色界面跟着变() {
        // 验收第 3 条「改令牌里任意一个颜色，这条当场红」落成的样子（挂单 `Q480`）：
        // 拿改过的令牌去装，装出来的得跟着变——哪一格写死了颜色，这里报「接线断了」；
        // 没人读的恰好是 `NOT_YET_USED` 那张单子——新加一个颜色忘了接、接上了忘了划掉，这条都红。
        let (断了, 没人读) = 变异(Theme::Dark);
        assert!(断了.is_empty(), "改了这几个键，装出来的没跟着变：{断了:?}");
        assert_eq!(没人读, NOT_YET_USED);
    }

    #[test]
    fn 改亮色令牌里任意一个颜色界面跟着变() {
        let (断了, 没人读) = 变异(Theme::Light);
        assert!(断了.is_empty(), "改了这几个键，装出来的没跟着变：{断了:?}");
        assert_eq!(没人读, NOT_YET_USED);
    }

    #[test]
    fn 按钮三档的高与左右留白取自令牌() {
        // 拿主意的人 2026-09-14 第二次裁：按钮照稿加高、加宽，小号单列一档（票 `gui-looks-like-the-design/05`）。
        //
        // 一、装配：拿**改过的**令牌走一遍，默认那一档的高与左右留白跟着变——哪一格写死了这里就红。
        let mut 改过 = Tokens::builtin().clone();
        改过.layout.button_height = 31.0;
        改过.layout.button_padding = 15.0;
        let ctx = headless::context();
        install_tokens(&ctx, &改过);
        for theme in [Theme::Dark, Theme::Light] {
            let spacing = &ctx.style_of(theme).spacing;
            assert_eq!(
                spacing.interact_size.y, 31.0,
                "{theme:?} 的按钮高没跟着令牌变"
            );
            assert_eq!(
                spacing.button_padding.x, 15.0,
                "{theme:?} 的按钮左右留白没跟着令牌变"
            );
        }

        // 二、真摆出来量：默认、小号、大号三颗按钮各多高、左右各留多少、摆出来多宽。
        let layout = &Tokens::builtin().layout;
        let ctx = headless::context();
        install(&ctx);
        let mut 量到 = Vec::new();
        // 头一帧 egui 还在量尺寸，第二帧才是摆稳的样子。
        for _ in 0..2 {
            量到.clear();
            headless::frame(&ctx, headless::input(), |ui| {
                let 摆一颗 = |ui: &mut egui::Ui| {
                    let 量的宽 = button_width(ui, "按钮");
                    (
                        ui.button("按钮").rect,
                        ui.spacing().button_padding.x,
                        量的宽,
                    )
                };
                量到.push(("默认", 摆一颗(ui)));
                量到.push(("小号", small_buttons(ui, 摆一颗)));
                量到.push(("大号", large_buttons(ui, 摆一颗)));
            });
        }
        let 期望 = [
            (layout.button_height, layout.button_padding),
            (layout.button_small_height, layout.button_small_padding),
            (layout.button_large_height, layout.button_large_padding),
        ];
        assert_eq!(量到.len(), 期望.len());
        for ((name, (rect, 留白, 量的宽)), (高, 令牌留白)) in 量到.iter().zip(期望) {
            assert_eq!(
                rect.height(),
                高,
                "{name}按钮高 {}，令牌是 {高}",
                rect.height()
            );
            assert_eq!(
                *留白, 令牌留白,
                "{name}按钮左右留白 {留白}，令牌是 {令牌留白}"
            );
            assert!(
                (rect.width() - 量的宽).abs() < 0.5,
                "{name}按钮摆出来宽 {}，照令牌留白量的是 {量的宽}",
                rect.width(),
            );
        }
    }

    #[test]
    fn 单行输入框的高取自令牌_与同档按钮等高() {
        // 拿主意的人 2026-09-14 第三次裁：输入框加高到与同一行那一档按钮等高（票 `gui-looks-like-the-design/05`）。
        let layout = &Tokens::builtin().layout;
        assert_eq!(
            layout.input_height, layout.button_height,
            "默认那一档输入框与按钮在令牌里就不等高"
        );
        assert_eq!(
            layout.input_small_height, layout.button_small_height,
            "小号输入框与小号按钮在令牌里就不等高"
        );

        // 真摆出来量：两档输入框各多高，与同一行里同档的那颗按钮是不是一样高；宽是不是连留白在内的外宽。
        let ctx = headless::context();
        install(&ctx);
        let (mut 默认框, mut 小号框) = (String::new(), String::new());
        let mut 量到 = None;
        // 头一帧 egui 还在量尺寸，第二帧才是摆稳的样子。
        for _ in 0..2 {
            headless::frame(&ctx, headless::input(), |ui| {
                let (框, 钮) = ui
                    .horizontal(|ui| {
                        let 框 =
                            text_input(ui, 200.0, egui::TextEdit::singleline(&mut 默认框)).rect;
                        (框, ui.button("按钮").rect)
                    })
                    .inner;
                let (小框, 小钮) = ui
                    .horizontal(|ui| {
                        let 框 =
                            small_text_input(ui, 200.0, egui::TextEdit::singleline(&mut 小号框))
                                .rect;
                        (框, small_buttons(ui, |ui| ui.button("按钮")).rect)
                    })
                    .inner;
                量到 = Some((框, 钮, 小框, 小钮));
            });
        }
        let (框, 钮, 小框, 小钮) = 量到.expect("跑过帧");
        assert_eq!(
            框.height(),
            layout.input_height,
            "默认那一档输入框高 {}，令牌是 {}",
            框.height(),
            layout.input_height,
        );
        assert_eq!(
            框.height(),
            钮.height(),
            "默认那一档输入框与同一行的按钮不等高"
        );
        assert_eq!(
            小框.height(),
            layout.input_small_height,
            "小号输入框高 {}，令牌是 {}",
            小框.height(),
            layout.input_small_height,
        );
        assert_eq!(
            小框.height(),
            小钮.height(),
            "小号输入框与同一行的小号按钮不等高"
        );
        assert!(
            (框.width() - 200.0).abs() < 0.5,
            "desired_width 该是连留白在内的外宽，量到 {}",
            框.width(),
        );
    }

    #[test]
    fn 控件四档的描边一样宽_宽度取令牌() {
        // 票 `gui-looks-like-the-design/05`（拿主意的人 2026-09-14 裁）：次要按钮照稿画一圈描边。
        // 四档一样宽、宽度跟着令牌走——拿**改过的**令牌走一遍整条装配，哪一档写死了宽度这里就红。
        for 宽 in [Tokens::builtin().layout.control_stroke, 2.5] {
            let mut 改过 = Tokens::builtin().clone();
            改过.layout.control_stroke = 宽;
            let ctx = headless::context();
            install_tokens(&ctx, &改过);
            for theme in [Theme::Dark, Theme::Light] {
                let widgets = &ctx.style_of(theme).visuals.widgets;
                for (name, widget) in [
                    ("inactive", &widgets.inactive),
                    ("hovered", &widgets.hovered),
                    ("active", &widgets.active),
                    ("open", &widgets.open),
                ] {
                    assert_eq!(
                        widget.bg_stroke.width, 宽,
                        "{theme:?} 的 {name} 那一档描边宽 {}，令牌是 {宽}",
                        widget.bg_stroke.width,
                    );
                }
            }
        }
        assert!(
            Tokens::builtin().layout.control_stroke > 0.0,
            "次要按钮得有一圈描边（设计稿 .btn）",
        );
    }

    #[test]
    fn 四档各画各的颜色不串() {
        // 两套主题各查一遍：暗色下分得开、亮色下也得分得开。
        for visuals in [egui::Visuals::dark(), egui::Visuals::light()] {
            let 颜色: Vec<egui::Color32> = Tier::ALL
                .iter()
                .map(|tier| tier_color(*tier, &visuals))
                .collect();
            for at in 0..颜色.len() {
                for other in (at + 1)..颜色.len() {
                    assert_ne!(
                        颜色[at],
                        颜色[other],
                        "{} 与 {} 撞了同一个颜色",
                        Tier::ALL[at].label(),
                        Tier::ALL[other].label(),
                    );
                }
            }
        }
    }

    #[test]
    fn 高置信那一档在两套主题下都读得出来() {
        // 「颜色不是唯一线索」的另一半：**字得先读得出来才算线索**。
        // 这一档曾经借过 `selection.bg_fill`——那是给背景用的一串，当前景色用时对比度远在
        // WCAG AA 的 4.5 之下。现在取令牌的 `hi`，比的是**装好之后**的窗口底色。
        let ctx = headless::context();
        install(&ctx);
        for theme in [Theme::Dark, Theme::Light] {
            let visuals = &ctx.style_of(theme).visuals;
            let 对比度 = contrast(tier_color(Tier::High, visuals), visuals.panel_fill);
            assert!(
                对比度 >= 4.5,
                "高置信在 {theme:?} 主题下对比度只有 {对比度:.2}"
            );
        }
    }

    /// WCAG 2.x 的对比度：`(亮 + 0.05) / (暗 + 0.05)`。
    fn contrast(a: egui::Color32, b: egui::Color32) -> f32 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// 相对亮度，照 WCAG 的定义。
    fn luminance(color: egui::Color32) -> f32 {
        let channel = |raw: u8| {
            let value = f32::from(raw) / 255.0;
            if value <= 0.03928 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    }
}
