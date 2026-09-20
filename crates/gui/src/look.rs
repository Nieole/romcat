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
//! | `on-accent` | 主按钮与危险按钮上的字，这两种按钮拿到焦点那一档的描边 |
//! | `accent-soft` | 选中的底色 |
//! | `accent-ink` | 链接 |
//! | `pop-color` | 窗口与弹出菜单的阴影（`window_shadow`、`popup_shadow`；形状照令牌 `[shadow.pop]`） |
//! | `mid` / `lo` | `warn_fg_color` / `error_fg_color`；`lo` 还是危险按钮的底、警示按钮的字与描边（[`danger_button`]、[`warn_button`]） |
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
//! 于是摆着按钮的那几行跟着一起高。**按钮上的字照稿**（拿主意的人 2026-09-14 定）：小号 `size-small`（`.btn.sm` 12）、
//! 大号 `size-button-large`（`.btn.lg` 14）在各自那一块里换；默认那一档是半号 `size-small-plus`（`.btn` 12.5），
//! 由 [`buttons`] 那一块换上。**默认那一档装不成全窗口的默认**：egui 0.36 的按钮取字只认 `override_font_id`，
//! 没有就用正文那一档（`Style::widget_style`），`TextStyle::Button` 那一格按钮不读——把正文改成 12.5 会连正文一起变小，
//! 所以各屏照稿重排时把按钮摆进 [`buttons`] 那一块（挂单 `Q862`）。
//!
//! **半号字按像素倍率取整**（[`font_size`]，挂单 `Q863`）：1 倍屏上 egui 把中文字形的横坐标取整，12.5 像素宽的字
//! 只能 12、13 交替落，字距忽宽忽窄。于是 12.5 在 1 倍屏上画成 13、在 2 倍屏上画成 12.5；取字号凡是碰上
//! 半号那两档的地方都走这个入口。
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

use egui::{Align, Color32};
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
///
/// 装上去的字号表里没有半号：半号那两档在画的那一刻按那块屏的倍率取（[`font_size`]），所以窗口拖到另一块
/// 倍率不同的屏上不必重装。
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

/// **取字号的统一入口**：令牌里的字号乘上这块屏的像素倍率取整到整像素，再除回点。
///
/// 半号那两档（`size-caption-plus` 11.5、`size-small-plus` 12.5）要它：1 倍屏上 egui 把中文字形的横坐标取整
/// （`epaint` 排字那一步），12.5 像素宽的字只能 12、13 交替落，字距看着忽宽忽窄（拿主意的人 2026-09-14 定，
/// 挂单 `Q863`）。整数号的字走它不变。**用到半号的地方一律走这里**，别直接拿令牌里那个数去排字。
#[must_use]
pub fn font_size(ctx: &egui::Context, size: f32) -> f32 {
    font_size_at(size, ctx.pixels_per_point())
}

/// 同 [`font_size`]，倍率由调用方给。倍率不是正数时原样交回。
fn font_size_at(size: f32, pixels_per_point: f32) -> f32 {
    if !(pixels_per_point.is_finite() && pixels_per_point > 0.0) {
        return size;
    }
    (size * pixels_per_point).round() / pixels_per_point
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
    let font = egui::FontId::proportional(font_size(ui.ctx(), tokens.font.size_small));
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

/// 在这一块里摆的按钮是**默认那一档**的（设计稿 `.btn`）：高、左右留白与全窗口装上的一样（令牌 `button-height` /
/// `button-padding`），按钮上的字换成半号 `size-small-plus`（按倍率取整，[`font_size`]）。
///
/// **为什么要一块**：egui 0.36 的按钮取字只认 `override_font_id`，没有就用正文那一档，于是全窗口的按钮默认
/// 与正文一样大（13）；照稿的 12.5 只有在这一块里摆才换得上（拿主意的人 2026-09-14 定照稿，挂单 `Q862`）。
/// 1 倍屏上 12.5 取整成 13，与不进这一块时一个像素都不差；2 倍屏上才看得出来。
pub fn buttons<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let tokens = Tokens::builtin();
    let font = egui::FontId::proportional(font_size(ui.ctx(), tokens.font.size_small_plus));
    sized_buttons(
        ui,
        [tokens.layout.button_height, tokens.layout.button_padding],
        Some(font),
        add,
    )
}

/// 在这一块里摆的按钮是**大号**的（设计稿 `.btn.lg`）：高、左右留白取令牌 `button-large-*`，字取
/// `size-button-large`。开场空态那一颗「添加主库」用它。
pub fn large_buttons<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let tokens = Tokens::builtin();
    let font = egui::FontId::proportional(font_size(ui.ctx(), tokens.font.size_button_large));
    sized_buttons(
        ui,
        [
            tokens.layout.button_large_height,
            tokens.layout.button_large_padding,
        ],
        Some(font),
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
/// 里只摆按钮（以及量按钮多宽）——摆一句说明进来，它也会跟着换字号。
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
/// 也是这么做的）。`Button` 那一格按钮自己不读（[`buttons`]），读它的是下拉框、折叠标题这几样。
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

/// **幽灵按钮**那一档（设计稿 `.btn.ghost`）：未激活时不画底也不画描边，字取次要文字色（`ink-2`）；
/// 悬停出一块凹陷底（`sunken`）、字换强调字色（`ink`）。按下与拿到焦点那一档描强调色——焦点得看得见。
/// **全窗口只有这一处回答「幽灵按钮什么颜色」**。
///
/// 用法同 [`primary_button`]：在一个 `ui.scope` 里改 `ui.visuals_mut()`。
pub fn ghost_button(visuals: &mut egui::Visuals) {
    let theme = egui::Theme::from_dark_mode(visuals.dark_mode);
    ghost_button_in(Tokens::builtin().color.theme(theme), visuals);
}

/// 幽灵按钮在这一套颜色里取哪几个。拆出来的理由同 [`tier_color_in`]。
fn ghost_button_in(palette: &Palette, visuals: &mut egui::Visuals) {
    let widgets = &mut visuals.widgets;
    // 每一档：底色、描边、字。
    for (widget, fill, stroke, ink) in [
        (
            &mut widgets.inactive,
            Color32::TRANSPARENT,
            Color32::TRANSPARENT,
            palette.ink_2,
        ),
        (
            &mut widgets.hovered,
            palette.sunken,
            Color32::TRANSPARENT,
            palette.ink,
        ),
        (
            &mut widgets.active,
            palette.sunken,
            palette.accent,
            palette.ink,
        ),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke.color = stroke;
        widget.fg_stroke.color = ink;
    }
}

/// 一颗**危险按钮**（设计稿 `.btn.danger`）：危险色底（令牌 `lo`）、`on-accent` 字。**全窗口只有这一处回答
/// 「危险按钮什么颜色」**；画它的是 [`crate::dialog`] 页脚上标了 `danger` 的那一颗——删掉一样东西之前问的那一下，
/// 按下去就真删。
///
/// 在一个 `ui.scope` 里换颜色，别的控件不受影响。字取 `on-accent`（强色底上的字那一格），不照稿写死白字
/// （挂单 `Q894`）。
///
/// ## 按不动的那一档为什么不是「照旧画一遍再调淡」
///
/// 稿上禁用只有一条规则：`.btn[disabled]{opacity:.45}`——**照背景调淡**。egui 也正是这么干的
/// （`Ui::add_enabled` 把这一层的画笔往面板底色上兑）。这条规则在浅色里成立，在暗色里不成立，
/// 因为两套主题的 `lo` 站在面板的**两侧**：
///
/// - 浅色 `lo` 是 `#B0392F`（比白面板**暗**），兑一半成了淡粉，一眼就是按不动的。
/// - 暗色 `lo` 是 `#EC7C70`（比深面板**亮**），兑一半成了 `#834E4C`——**仍旧是一整块亮过底色的实心色**，
///   看上去与能按的那一颗没两样（实测基线上量出来的就是这个数）。
///
/// 所以按不动的那一档换的不是浓淡，是**份量**：实心的危险色底换成**危险色的浅底**（令牌 `lo-soft`）
/// 加危险色的字——就是 [`chip`] 那一档 [`Tone::Bad`] 的搭配。「实心」与「浅底」的差别在两套主题里
/// 都是一眼的事，而字还留在危险色那一族里，人看得清自己眼下按不动的是哪一颗。
/// egui 那层调淡照旧盖在上面，于是两套主题各自又往自己的面板底色退了一步。
///
/// **颜色由截图门守**（`library/remove-root-*`），这一层只钉「按不动」那件事本身。
pub fn danger_button(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    ui.scope(|ui| {
        let theme = egui::Theme::from_dark_mode(ui.visuals().dark_mode);
        let palette = Tokens::builtin().color.theme(theme);
        if enabled {
            danger_button_in(palette, ui.visuals_mut());
        } else {
            disabled_danger_button_in(palette, ui.visuals_mut());
        }
        ui.button(text)
    })
    .inner
}

/// 按不动的危险按钮在这一套颜色里取哪几个：浅底 `lo-soft`、危险色的字与描边。拆出来的理由同 [`tier_color_in`]。
///
/// **三档同一套**：按不动的时候 egui 压根不会走到悬停与按下那两档，写齐只是免得哪天有人把它用在
/// 别的地方时看见一颗半旧半新的按钮。
fn disabled_danger_button_in(palette: &Palette, visuals: &mut egui::Visuals) {
    let widgets = &mut visuals.widgets;
    for widget in [
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
    ] {
        widget.bg_fill = palette.lo_soft;
        widget.weak_bg_fill = palette.lo_soft;
        widget.bg_stroke.color = palette.lo_soft;
        widget.fg_stroke.color = palette.lo;
    }
}

/// 危险按钮在这一套颜色里取哪几个。拆出来的理由同 [`tier_color_in`]。
fn danger_button_in(palette: &Palette, visuals: &mut egui::Visuals) {
    let widgets = &mut visuals.widgets;
    // 三档同一个底：令牌里没有「危险色悬停」那一格。拿到焦点那一档的描边换成 `on-accent`，
    // 理由同 [`primary_button_in`]：危险色底上再描一圈危险色是看不见的。
    for (widget, stroke) in [
        (&mut widgets.inactive, palette.lo),
        (&mut widgets.hovered, palette.lo),
        (&mut widgets.active, palette.on_accent),
    ] {
        widget.bg_fill = palette.lo;
        widget.weak_bg_fill = palette.lo;
        widget.bg_stroke.color = stroke;
        widget.fg_stroke.color = palette.on_accent;
    }
}

/// **警示按钮**那一档（设计稿 `.btn.warn`）：白底（`panel`）、红字、红描边（`lo`），悬停时照旧红描边——
/// 拿主意的人定，与库屏「移除」同一档。按下与拿到焦点那一档描强调色，焦点得看得见。
/// **全窗口只有这一处回答「警示按钮什么颜色」**。
///
/// 用法同 [`primary_button`]：在一个 `ui.scope` 里改 `ui.visuals_mut()`。
pub fn warn_button(visuals: &mut egui::Visuals) {
    let theme = egui::Theme::from_dark_mode(visuals.dark_mode);
    warn_button_in(Tokens::builtin().color.theme(theme), visuals);
}

/// 警示按钮在这一套颜色里取哪几个。拆出来的理由同 [`tier_color_in`]。
fn warn_button_in(palette: &Palette, visuals: &mut egui::Visuals) {
    let widgets = &mut visuals.widgets;
    // 每一档：底色、描边。字一律 `lo`。
    for (widget, fill, stroke) in [
        (&mut widgets.inactive, palette.panel, palette.lo),
        (&mut widgets.hovered, palette.panel, palette.lo),
        (&mut widgets.active, palette.sunken, palette.accent),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke.color = stroke;
        widget.fg_stroke.color = palette.lo;
    }
}

/// 一颗**图标按钮**（设计稿 `.iconbtn`）：边长取令牌 `icon-button`，没底没框、弱字色的一个字形（正文字号），小圆角；
/// 悬停或拿到焦点时垫凹陷底、描一圈分隔线色、字换成强调字。子库屏规则行尾那颗「×」（移除这条规则）、浏览屏左右两栏收起与展开那两颗箭头用它。
///
/// **字形得在打包的字体里**（[`crate::font`]）：不在的画出来是豆腐块——「✎」就不在，走 [`pencil_button`]。
/// 无障碍树上报成一颗按钮，名字就是那个字形。
pub fn icon_button(ui: &mut egui::Ui, glyph: &str) -> egui::Response {
    icon_button_drawn(ui, glyph, |painter, rect, 字色| {
        let 字号 = font_size(painter.ctx(), Tokens::builtin().font.size_body);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            glyph,
            egui::FontId::proportional(字号),
            字色,
        );
    })
}

/// 一颗画着**铅笔**的图标按钮（设计稿 `.iconbtn` 里那个「✎」）：打包的字体里没有这个字形，拿线条画——笔身、笔尖、靠笔尾
/// 的一道箍；一笔宽取令牌 `control-stroke`，占的方块与字形同大（正文字号），底、描边、颜色与 [`icon_button`] 同一档
/// （拿主意的人 2026-09-14 定）。子库屏规则行尾那颗「✎」（在浏览中编辑这条规则）用它；无障碍树上的名字是「编辑」。
pub fn pencil_button(ui: &mut egui::Ui) -> egui::Response {
    icon_button_drawn(ui, "编辑", |painter, rect, 字色| {
        let tokens = Tokens::builtin();
        let 边 = font_size(painter.ctx(), tokens.font.size_body);
        let 方块 = egui::Rect::from_center_size(rect.center(), egui::vec2(边, 边));
        // 铅笔的形状写在一格单位方块里（左下是笔尖、右上是笔尾），再照方块放大——这几个数是图形，不是间距。
        let 点 = |x: f32, y: f32| 方块.min + egui::vec2(x, y) * 边;
        let 笔 = egui::Stroke::new(tokens.layout.control_stroke, 字色);
        painter.add(egui::Shape::closed_line(
            vec![
                点(0.88, 0.28),
                点(0.72, 0.12),
                点(0.18, 0.66),
                点(0.08, 0.92),
                点(0.34, 0.82),
            ],
            笔,
        ));
        painter.line_segment([点(0.64, 0.20), 点(0.80, 0.36)], 笔);
    })
}

/// 图标按钮的底、描边、悬停与无障碍信息画在一处；中间画什么由 `draw` 定（照这一帧该用的字色）。
fn icon_button_drawn(
    ui: &mut egui::Ui,
    name: &str,
    draw: impl FnOnce(&egui::Painter, egui::Rect, Color32),
) -> egui::Response {
    let tokens = Tokens::builtin();
    let side = tokens.layout.icon_button;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click());
    let enabled = ui.is_enabled();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name));
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let 线 = visuals.widgets.noninteractive.bg_stroke;
        let (底色, 描边, 字色) = if response.hovered() || response.has_focus() {
            (visuals.extreme_bg_color, 线, visuals.strong_text_color())
        } else {
            (
                Color32::TRANSPARENT,
                egui::Stroke::new(线.width, Color32::TRANSPARENT),
                visuals.weak_text_color(),
            )
        };
        let painter = ui.painter();
        painter.rect(
            rect,
            tokens.radius.small,
            底色,
            描边,
            egui::StrokeKind::Inside,
        );
        draw(painter, rect, 字色);
    }
    response
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

/// 一枚**标签**是哪种语气（设计稿 `.ro`「只读」、`.chip.t-mid`「版本不兼容」、`.chip.t-none`「未连接」；任务屏历史收场那一格的
/// `t-hi` / `t-none` / `t-mid` / `t-lo`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// 放心：令牌 `hi` 那一对（`hi` 的字、`hi-soft` 的底）。
    Good,
    /// 留神：令牌 `mid` 那一对。
    Caution,
    /// 不置可否：令牌 `none` 那一对（设计稿 `.t-none`）——浏览屏「仅文件名」、库屏上盘不在位的那个根、子库屏「未连接」、任务屏历史里的「已取消」。
    Neutral,
    /// 要紧、出错、出了界：令牌 `lo` 那一对——浏览屏「待确认」（设计稿 `.chip.t-lo`）、任务屏历史里的「失败」、
    /// 子库屏删减建议表头的「超出容量上限」（设计稿 `.trim .th`）。
    Bad,
    /// 强调：令牌 `accent-ink` 的字、`accent-soft` 的底（设计稿 `.t-acc`）——子库屏「尚未生成差量预览」。
    Accent,
}

/// 一档**置信度**的标签用哪种语气：高置信放心、中置信留神、低置信要紧、没有候选不置可否
/// （设计稿 `.t-hi` / `.t-mid` / `.t-lo` / `.t-none`）。字那一格与 [`tier_color`] 取的是同一个令牌。
#[must_use]
pub fn tier_tone(tier: Tier) -> Tone {
    match tier {
        Tier::High => Tone::Good,
        Tier::Medium => Tone::Caution,
        Tier::Low => Tone::Bad,
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
        Tone::Neutral => (palette.none, palette.none_soft),
        Tone::Bad => (palette.lo, palette.lo_soft),
        Tone::Accent => (palette.accent_ink, palette.accent_soft),
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

/// 一枚**标签**：浅底小圆角，同色的圆点与字（设计稿 `.chip`）。高、左右留白、圆点直径、圆点与字的间距取令牌
/// `chip-height` / `chip-padding` / `chip-dot` / `chip-gap`，字取半号 `size-caption-plus`（按倍率取整，[`font_size`]）。
pub fn chip(ui: &mut egui::Ui, tone: Tone, text: &str) -> egui::Response {
    let layout = &Tokens::builtin().layout;
    tag(
        ui,
        tone,
        text,
        [layout.chip_height, layout.chip_padding, layout.chip_gap],
        Tokens::builtin().font.size_caption_plus,
        true,
    )
}

/// 一枚**不带圆点**的标签（设计稿 `.chip.plain`，库屏根那张表上盘没接上的那一枚）：不画点、也不留点那一格，其余同 [`chip`]。
pub fn plain_chip(ui: &mut egui::Ui, tone: Tone, text: &str) -> egui::Response {
    let layout = &Tokens::builtin().layout;
    tag(
        ui,
        tone,
        text,
        [layout.chip_height, layout.chip_padding, layout.chip_gap],
        Tokens::builtin().font.size_caption_plus,
        false,
    )
}

/// 一枚**只读标签**（设计稿 `.ro`，添加主库向导里「只读访问，不会写入这个根」「只读」那两枚）：高置信那一对颜色，
/// 高、左右留白、圆点与字的间距取令牌 `ro-height` / `ro-padding` / `ro-gap`，圆点与 [`chip`] 同一个直径，字是说明字号
/// `size-small`。**与 [`chip`] 是两个样式**：稿上它比标签高一截、字大半号（拿主意的人 2026-09-14 定单列一档，挂单 `Q862`）。
pub fn read_only(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let tokens = Tokens::builtin();
    let layout = &tokens.layout;
    tag(
        ui,
        Tone::Good,
        text,
        [layout.ro_height, layout.ro_padding, layout.ro_gap],
        tokens.font.size_small,
        true,
    )
}

/// 标签与只读标签共用的画法：`[高, 左右留白, 圆点与字的间距]`，这个字号（按倍率取整），圆点直径取令牌 `chip-dot`。
/// `圆点` 为假时不画点、也不留点那一格（[`plain_chip`]）。
fn tag(
    ui: &mut egui::Ui,
    tone: Tone,
    text: &str,
    [高, 边, 缝]: [f32; 3],
    字号: f32,
    圆点: bool,
) -> egui::Response {
    let tokens = Tokens::builtin();
    let (字色, 底色) = tone_colors(tone, ui.visuals());
    let 字号 = font_size(ui.ctx(), 字号);
    let galley = egui::WidgetText::from(
        egui::RichText::new(text)
            .font(egui::FontId::proportional(字号))
            .color(字色),
    )
    .into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Small,
    );
    let 点 = if 圆点 { tokens.layout.chip_dot } else { 0.0 };
    let 点后 = if 圆点 { 缝 } else { 0.0 };
    let 字 = galley.size();
    let size = egui::vec2(边 + 点 + 点后 + 字.x + 边, 高);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, tokens.radius.small, 底色);
    if 圆点 {
        painter.circle_filled(
            egui::pos2(rect.left() + 边 + 点 / 2.0, rect.center().y),
            点 / 2.0,
            字色,
        );
    }
    let 摆在 = egui::pos2(rect.left() + 边 + 点 + 点后, rect.center().y - 字.y / 2.0);
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
            let 字号 = font_size(ui.ctx(), tokens.font.size_small_plus);
            ui.style_mut().override_font_id = Some(egui::FontId::proportional(字号));
            add(ui)
        })
        .inner
}

/// 一行**单选**（设计稿 `.opt`）：左边一枚圆点，右边名字、底下一行说明小字，整行按得动。圆点选中时是强调色外圈、一道底色缝、
/// 强调色圆心；没选中是一圈说明字色的细线。直径、圆心、缝、行内间距与上下留白取令牌 `radio-diameter` / `radio-dot` /
/// `radio-gap` / `option-gap` / `option-padding`；名字 `size-small-plus`、说明 `size-caption-plus`（稿 12.5 / 11.5）。
///
/// 交回整行的点击（圆点、名字、说明哪一处按下去都算）。
pub fn radio_option(ui: &mut egui::Ui, selected: bool, title: &str, note: &str) -> egui::Response {
    let tokens = Tokens::builtin();
    let layout = &tokens.layout;
    let palette = tokens
        .color
        .theme(egui::Theme::from_dark_mode(ui.visuals().dark_mode));
    let 名字号 = font_size(ui.ctx(), tokens.font.size_small_plus);
    let 说明号 = font_size(ui.ctx(), tokens.font.size_caption_plus);
    ui.add_space(layout.option_padding);
    let row = ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = layout.option_gap;
        let 名字 =
            egui::WidgetText::from(egui::RichText::new(title).size(名字号).color(palette.ink))
                .into_galley(
                    ui,
                    Some(egui::TextWrapMode::Extend),
                    f32::INFINITY,
                    egui::TextStyle::Body,
                );
        let (dot_rect, dot) = ui.allocate_exact_size(
            egui::vec2(layout.radio_diameter, 名字.size().y),
            egui::Sense::click(),
        );
        let center = dot_rect.center();
        let painter = ui.painter();
        if selected {
            painter.circle_filled(center, layout.radio_diameter / 2.0, palette.accent);
            painter.circle_filled(
                center,
                layout.radio_dot / 2.0 + layout.radio_gap,
                palette.panel,
            );
            painter.circle_filled(center, layout.radio_dot / 2.0, palette.accent);
        } else {
            painter.circle(
                center,
                layout.radio_diameter / 2.0 - 0.5,
                palette.panel,
                egui::Stroke::new(1.0, palette.ink_3),
            );
        }
        let texts = ui
            .vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let 名 = ui.add(egui::Label::new(名字).sense(egui::Sense::click()));
                let 注 = ui.add(
                    egui::Label::new(egui::RichText::new(note).size(说明号).color(palette.ink_3))
                        .sense(egui::Sense::click()),
                );
                名 | 注
            })
            .inner;
        dot | texts
    });
    ui.add_space(layout.option_padding);
    row.inner
}

/// 一块**警示框**（设计稿 `.warnbox`）：`lo-soft` 底、描边是分隔线色往 `lo` 挪四成、中圆角，内边距取令牌
/// `warn-box-padding`，字是 `size-small-plus`；头一句用 `lo` 色、拉丁与数字加粗，后面接着正文色。占满这一栏的宽。
pub fn warn_box(ui: &mut egui::Ui, head: &str, body: &str) {
    let tokens = Tokens::builtin();
    let palette = tokens
        .color
        .theme(egui::Theme::from_dark_mode(ui.visuals().dark_mode));
    let [上下, 左右] = tokens.layout.warn_box_padding;
    let 字号 = font_size(ui.ctx(), tokens.font.size_small_plus);
    egui::Frame::new()
        .fill(palette.lo_soft)
        .stroke(egui::Stroke::new(
            1.0,
            palette.line.lerp_to_gamma(palette.lo, 0.4),
        ))
        .corner_radius(tokens.radius.medium)
        .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut job = egui::text::LayoutJob::default();
            let font = egui::FontId::proportional(字号);
            crate::font::strong(head)
                .size(字号)
                .color(palette.lo)
                .append_to(
                    &mut job,
                    ui.style(),
                    egui::FontSelection::FontId(font.clone()),
                    Align::LEFT,
                );
            egui::RichText::new(body)
                .size(字号)
                .color(palette.ink)
                .append_to(
                    &mut job,
                    ui.style(),
                    egui::FontSelection::FontId(font),
                    Align::LEFT,
                );
            job.wrap.max_width = ui.available_width();
            ui.label(job);
        });
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
        egui::FontId::monospace(font_size(ui.ctx(), tokens.font.size_mini)),
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
    let painter = ui.painter();
    dashed_outline(
        painter,
        rect,
        egui::Stroke::new(tokens.layout.control_stroke, 线色),
    );
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

/// 弹层里「会怎样」的一条（设计稿 `.impact li`）：行首一枚强调色圆点，后面一句半号字，`(字, 要不要强调)`
/// 一段段接起来（稿上 `<b>` 那几个字是强调字）。
///
/// 圆点多大、那一列多宽取令牌 `impact-dot` / `impact-column`；圆点对齐头一行的中线，字折行时不跟着往下挪。
///
/// **住在这一层而不是某一屏里**：子库屏那三处弹层（删子库、删规则、移出此作品）与库屏
/// 「移除根」画的是同一种东西，一屏一份的话那枚圆点、那一列宽与那一档字号就会各漂各的。
pub fn impact(ui: &mut egui::Ui, parts: &[(&str, bool)]) {
    let color = ui.visuals().selection.stroke.color;
    impact_row(ui, color, parts);
}

/// 同上，**留神那一档**（设计稿 `.impact li.warn`）：圆点换成令牌 `mid`。
///
/// 稿上标 `warn` 的是「这一条会少东西」那几句——移除根那一层里「从库中去掉多少变体」
/// 与「哪台子库会少多少」用的就是它。
pub fn impact_warn(ui: &mut egui::Ui, parts: &[(&str, bool)]) {
    let color = tone_colors(Tone::Caution, ui.visuals()).0;
    impact_row(ui, color, parts);
}

/// 画一条，圆点用交进来的那个颜色。
fn impact_row(ui: &mut egui::Ui, dot_color: Color32, parts: &[(&str, bool)]) {
    let tokens = Tokens::builtin();
    let style = ui.style().clone();
    let mut job = egui::text::LayoutJob::default();
    for (text, strong) in parts {
        let rich = if *strong {
            crate::font::strong(*text)
        } else {
            egui::RichText::new(*text)
        };
        rich.size(font_size(ui.ctx(), tokens.font.size_small_plus))
            .append_to(
                &mut job,
                &style,
                egui::FontSelection::Default,
                Align::Center,
            );
    }
    ui.horizontal_top(|ui| {
        let column = tokens.layout.impact_column;
        let width = (ui.available_width() - column - ui.spacing().item_spacing.x).max(0.0);
        let galley = egui::WidgetText::from(job).into_galley(
            ui,
            Some(egui::TextWrapMode::Wrap),
            width,
            egui::FontSelection::Default,
        );
        #[allow(clippy::cast_precision_loss)]
        let rows = galley.rows.len().max(1) as f32;
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(column, galley.size().y), egui::Sense::hover());
        let dot = tokens.layout.impact_dot;
        ui.painter().circle_filled(
            egui::pos2(
                rect.left() + dot / 2.0,
                rect.top() + galley.size().y / rows / 2.0,
            ),
            dot / 2.0,
            dot_color,
        );
        ui.label(galley);
    });
}

/// 一段**弱色的说明**（一整句话，摆在面板正文里）：照可用宽折行，**段末不留孤字**（`no_orphan_width`）。
///
/// 第十四版库屏候选图上，空库时数据源那一句「……识别和刮削要用它们。」最后折出一个孤零零的「们。」。
pub fn weak_paragraph(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let 字 = egui::WidgetText::from(egui::RichText::new(text).weak());
    let 宽 = no_orphan_width(ui, &字, ui.available_width());
    ui.scope(|ui| {
        ui.set_max_width(宽);
        ui.label(字)
    })
    .inner
}

/// 一段字照 `宽` 折行时，**段末不留孤字**得折在多宽：末行只剩一个字（标点不算）时，把上一行末尾那个字挪下来，
/// 直到末行至少两个字——中文排版说的「孤字」（W3C《中文排版需求》）。不折行、或末行本来就够两个字，原样交回 `宽`。
///
/// 不写一个像素：挪一个字就是把折行宽收到上一行最后那个字的正中，那个字放不下、落到下一行，它前面那个字照样放得下
/// （收到正中而不是左沿：宽度摆进界面时差一点零头，也不会多挪一个字）；字宽现量。
fn no_orphan_width(ui: &egui::Ui, text: &egui::WidgetText, 宽: f32) -> f32 {
    /// 末行至少几个字（标点不算）。
    const 末行至少: usize = 2;
    let 是字 = |glyph: &&egui::epaint::text::Glyph| glyph.chr.is_alphanumeric();
    let mut 宽 = 宽;
    // 每挪一趟末行至少多一个字，整段有几个字就最多挪几趟。
    for _ in 0..text.text().chars().count() {
        let galley = text.clone().into_galley(
            ui,
            Some(egui::TextWrapMode::Wrap),
            宽,
            egui::FontSelection::Default,
        );
        let [.., 上一行, 末行] = galley.rows.as_slice() else {
            break;
        };
        if 末行.glyphs.iter().filter(是字).count() >= 末行至少 {
            break;
        }
        let Some(末字) = 上一行.glyphs.iter().rev().find(是字) else {
            break;
        };
        宽 = 末字.pos.x + 末字.advance_width / 2.0;
    }
    宽
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

/// 那枚**标志**（设计稿 `.mark`）：强调字色的底，两道窗口底色的横条，左下一小块强调色，画满 `rect`。
///
/// 设计稿是按 44 点见方画的，里头那几道条按这个比例缩放；外圈圆角照交进来的那一档。开场左栏那一枚（44 点、
/// 圆角 `large`）与左栏顶上切换主库那张卡上的那一枚（令牌 `rail-mark`、圆角 `medium`，设计稿 `.libsw .mark`）
/// 共用这一处（票 `gui-looks-like-the-design/32`）。
pub fn mark(painter: &egui::Painter, rect: egui::Rect, corner: u8, visuals: &egui::Visuals) {
    let 格 = rect.width() / 44.0;
    painter.rect_filled(rect, corner, visuals.strong_text_color());
    let 横条 = |左: f32, 上: f32, 右: f32| {
        egui::Rect::from_min_max(
            rect.min + egui::vec2(左 * 格, 上 * 格),
            egui::pos2(rect.right() - 右 * 格, rect.top() + (上 + 5.0) * 格),
        )
    };
    painter.rect_filled(横条(9.0, 12.0, 9.0), 2.0 * 格, visuals.panel_fill);
    painter.rect_filled(横条(9.0, 21.0, 17.0), 2.0 * 格, visuals.panel_fill);
    let 小块 = egui::Rect::from_min_size(
        egui::pos2(rect.left() + 9.0 * 格, rect.bottom() - 15.0 * 格),
        egui::vec2(10.0 * 格, 6.0 * 格),
    );
    painter.rect_filled(小块, 2.0 * 格, visuals.selection.stroke.color);
}

/// 一屏的**屏头**（设计稿 `.scrhead`）：左边是标题（页面标题字号、强调字）与副标题（令牌 `size-small-plus`、
/// 弱字），右边是这一屏的动作；窗口底色、内边距取令牌 `screen-header-padding`、彼此隔 `screen-header-gap`，
/// 底下一道分隔线。**占掉这一块地方最上头那一截**（一块 `egui::Panel::top`），屏体接着画在它底下。
///
/// `id` 是这块面板的 id，一屏一个。交回的 `response.rect` 是整个屏头。
///
/// ## 右侧那一段怎么靠右
///
/// egui 的横排从左往右摆，右对齐的那种（`right_to_left`）会把里头的控件倒过来摆。所以这一段照常从左往右摆，
/// 只是先让出「剩下的宽 − 它有多宽」那么一截。它有多宽，**同一帧里先在一块看不见、按不动的地方摆一遍量出来**
/// （egui 的 `sizing_pass`），再真摆——`actions` 因此每帧调两遍，只有真摆的那一遍按得动。于是换了宽度的那一帧
/// 画出来的已经是靠右的样子，不会先在左边画一帧、下一帧才挪过去（测试照上一帧的位置去点，点的正是那一帧）。
/// 里头要是有占满剩下那一截的东西（一段 `right_to_left`），量到的就是整截，那时照常从副标题后面接着摆。
///
/// **摆不下就整段折到下一行**（设计稿 `.scrhead` 的 `flex-wrap:wrap`）：从左边内边距起摆，与上一行隔
/// `screen-header-gap`，屏头跟着长高；这一段自己比一整行还宽时，摆不下的那几样再往下折。不折的话它溢出屏头右沿，
/// 后半截被裁掉。**屏头刚长高的那一帧**面板还按上一帧的高
/// 裁剪，折下来那一行的字 egui 不画——只在这一帧让它重画一遍（`request_discard`）；平时不动用那一遍额度。
///
/// **不用「量上一帧、宽变了就让 egui 重画这一帧」**（`request_discard`）：egui 一帧最多画两遍，这一遍让屏头用掉，
/// 同一帧里头一回出现的表格（`egui::Grid` 头一帧也要重画一遍才看得见）就只能隐身一帧——
/// 任务屏的历史就是这么在换进来的那一帧里一行都没画出来的。
///
/// 右侧那一段里的控件间距照这块 `ui` 原来的，不跟着换成屏头那一档。
pub fn screen_header<R>(
    ui: &mut egui::Ui,
    id: impl Into<egui::Id>,
    title: &str,
    subtitle: &str,
    mut actions: impl FnMut(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let tokens = Tokens::builtin();
    let [上下, 左右] = tokens.space.screen_header_padding;
    let id = id.into();
    let 框 = egui::Frame::new()
        .fill(ui.visuals().panel_fill)
        .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)));
    egui::Panel::top(id)
        .resizable(false)
        .frame(框)
        .show(ui, |ui| {
            let 原来的间距 = ui.spacing().item_spacing;
            let 间距 = tokens.space.screen_header_gap;
            let 摆进这一行 = ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 间距;
                let 强调字 = ui.visuals().strong_text_color();
                ui.label(egui::RichText::new(title).heading().color(强调字));
                let 弱字 = ui.visuals().weak_text_color();
                ui.label(
                    egui::RichText::new(subtitle)
                        .font(egui::FontId::proportional(font_size(
                            ui.ctx(),
                            tokens.font.size_small_plus,
                        )))
                        .color(弱字),
                );
                靠右摆(ui, 原来的间距, &mut actions)
            });
            match 摆进这一行.inner {
                Some(inner) => inner,
                // 摆不下：整段折到下一行，从左边起摆，行距也是那一档间距。
                None => {
                    ui.add_space((间距 - ui.spacing().item_spacing.y).max(0.0));
                    // 折下来的那一行是**会接着折的**：这一段自己比一整行还宽时，摆不下的那几样再往下折，不溢出右沿
                    // （`flex-wrap` 折的是每一样，不是整段）。**字不许在一样里头断开**：会折行的横排里 egui 的字默认
                    // 跟着折，一句「队列 3 条待裁决；选中 3 条」会从半中间断到下一行——换成整样挪下去。
                    let inner = ui
                        .horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = 原来的间距;
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                            actions(ui)
                        })
                        .inner;
                    // **屏头刚长高的那一帧**：面板这一帧还按上一帧的高裁剪，折下来的那一行落在裁剪框外，
                    // egui 不画那一行的字。只在这一下让它当场把这一帧重画一遍——第二遍面板已经记住了新的高。
                    let 要的底 = ui.min_rect().bottom() + 上下;
                    let 上一帧的底 =
                        egui::PanelState::load(ui.ctx(), id).map(|state| state.outer_rect.bottom());
                    if 上一帧的底.is_none_or(|底| 底 + 0.5 < 要的底) {
                        ui.ctx().request_discard("屏头折成两行，刚长高");
                    }
                    inner
                }
            }
        })
}

/// 在这一行剩下的地方里把 `add` 摆的那一段**靠右**：先在一块看不见、按不动的地方摆一遍量宽，
/// 让出「剩下的宽 − 它有多宽」，再真摆。**剩下的地方摆不下就不摆**，交回 `None`，由调用方折到下一行。
/// 见 [`screen_header`]「右侧那一段怎么靠右」。
fn 靠右摆<R>(
    ui: &mut egui::Ui,
    间距: egui::Vec2,
    mut add: impl FnMut(&mut egui::Ui) -> R,
) -> Option<R> {
    let 剩下 = ui.available_width();
    let mut 量 = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("屏头右侧那一段量宽")
            .max_rect(ui.available_rect_before_wrap())
            .layout(*ui.layout())
            .sizing_pass()
            .invisible(),
    );
    量.spacing_mut().item_spacing = 间距;
    add(&mut 量);
    let 宽 = 量.min_rect().width();
    // 差不到半点不算摆不下：位置取整到像素时宽会多出一点点。
    if 宽 > 剩下 + 0.5 {
        return None;
    }
    ui.add_space((剩下 - 宽).max(0.0));
    Some(
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = 间距;
            add(ui)
        })
        .inner,
    )
}

/// 一屏的**屏体**（设计稿 `.scrbody`）：屏头底下剩下的整块，窗口底色，竖着滚；内边距取令牌
/// `screen-body-padding`（上、左右、下）。
///
/// **内边距在滚动区里面**：滚动条贴着这一块的右沿，底下那一截留白要滚到底才看得见——与稿上
/// `padding` 写在 `overflow:auto` 那一层是同一个样子。`id_salt` 分开各屏的滚动位置。
///
/// 各屏自己的 `CentralPanel` 自带一圈 8 点边距（`egui::Frame::central_panel`），改用它时连那一层一起换掉，
/// 不要套在外面——套在外面就是两层内边距（票 `gui-looks-like-the-design/32`，由各屏的票接手）。
pub fn screen_body<R>(
    ui: &mut egui::Ui,
    id_salt: impl egui::AsIdSalt,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let [上, 左右, 下] = Tokens::builtin().space.screen_body_padding;
    let 留白 = egui::Margin {
        left: 左右 as i8,
        right: 左右 as i8,
        top: 上 as i8,
        bottom: 下 as i8,
    };
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(ui.visuals().panel_fill))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(id_salt)
                .auto_shrink(false)
                .show(ui, |ui| {
                    egui::Frame::new()
                        .inner_margin(留白)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            add(ui)
                        })
                        .inner
                })
                .inner
        })
        .inner
}

/// 一颗写着 `label` 的按钮画出来多宽：字宽加两边的内边距，与 `egui::Button` 自己量的一样。
///
/// 要把按钮摆在一行的右头、先替它留出地方时用。**字照按钮取字的规矩取**：`override_font_id`，
/// 没有就用正文那一档（egui 0.36 的 `Style::widget_style`）——各档按钮那一块里换的正是前者
/// （[`buttons`]、[`small_buttons`]、[`large_buttons`]），量的与摆的才是同一个字号。描边不另算：egui 把描边宽从内边距里扣了。
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
        egui::FontId::proportional(font_size(ui.ctx(), tokens.font.size_small)),
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

/// 一枚**行内标签**（设计稿 `.tag`）：凹陷底（`sunken`）、小圆角、次要字色（`ink-2`），高 `tag-height`、左右留白
/// `tag-padding`，字取半号 `size-caption-plus`（按倍率取整，[`font_size`]）。待确认屏逐条那一屏抬头上的平台、容量、
/// 裁决钉在哪几枚用它。
pub fn inline_tag(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let tokens = Tokens::builtin();
    let palette = palette(ui);
    let 字号 = font_size(ui.ctx(), tokens.font.size_caption_plus);
    let 字 = ui.painter().layout_no_wrap(
        text.to_owned(),
        egui::FontId::proportional(字号),
        palette.ink_2,
    );
    let 边 = tokens.layout.tag_padding;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(字.size().x + 2.0 * 边, tokens.layout.tag_height),
        egui::Sense::hover(),
    );
    let painter = ui.painter();
    painter.rect_filled(rect, tokens.radius.small, palette.sunken);
    painter.galley(
        egui::pos2(rect.left() + 边, rect.center().y - 字.size().y / 2.0),
        字,
        palette.ink_2,
    );
    response
}

/// 一枚**键帽**（设计稿 `.kbd`）：面板底、描一圈 `line-2`、底边描得粗一些（`kbd-bottom`），字是等宽的 `size-caption`、
/// 次要字色，四边留白 `kbd-padding`。待确认屏逐条那一屏的键位提示用它。
pub fn kbd(ui: &mut egui::Ui, key: &str) -> egui::Response {
    let (尺寸, 字) = kbd_galley(ui, key);
    let (rect, response) = ui.allocate_exact_size(尺寸, egui::Sense::hover());
    paint_kbd(ui, rect, 字, 1.0);
    response
}

/// 一枚键帽多大，连它上面那个字排好。
fn kbd_galley(ui: &egui::Ui, key: &str) -> (egui::Vec2, std::sync::Arc<egui::Galley>) {
    let tokens = Tokens::builtin();
    let 字号 = font_size(ui.ctx(), tokens.font.size_caption);
    let 字 = ui.painter().layout_no_wrap(
        key.to_owned(),
        egui::FontId::monospace(字号),
        egui::Color32::PLACEHOLDER,
    );
    let [上下, 左右] = tokens.space.kbd_padding;
    let 尺寸 = egui::vec2(
        字.size().x + 2.0 * 左右,
        字.size().y + 2.0 * 上下 + tokens.layout.kbd_bottom,
    );
    (尺寸, 字)
}

/// 在 `rect` 里画一枚键帽；`淡` 是这一枚画几成深（按不动的按钮上那一枚淡一半）。
fn paint_kbd(ui: &egui::Ui, rect: egui::Rect, 字: std::sync::Arc<egui::Galley>, 淡: f32) {
    let tokens = Tokens::builtin();
    let palette = palette(ui);
    let 线宽 = tokens.layout.control_stroke;
    let 线色 = palette.line_2.gamma_multiply(淡);
    let painter = ui.painter();
    painter.rect(
        rect,
        tokens.radius.small,
        palette.panel.gamma_multiply(淡),
        egui::Stroke::new(线宽, 线色),
        egui::StrokeKind::Inside,
    );
    // 底边描得粗一些：在那一道描边里头再描一道。
    let 加粗 = (tokens.layout.kbd_bottom - 线宽).max(0.0);
    if 加粗 > 0.0 {
        painter.hline(
            rect.x_range().shrink(f32::from(tokens.radius.small)),
            rect.bottom() - 线宽 - 加粗 / 2.0,
            egui::Stroke::new(加粗, 线色),
        );
    }
    let 摆在 = egui::pos2(
        rect.center().x - 字.size().x / 2.0,
        rect.center().y - (字.size().y + 加粗) / 2.0,
    );
    painter.galley_with_override_text_color(摆在, 字, palette.ink_2.gamma_multiply(淡));
}

/// 一颗**带键帽的按钮**（设计稿 `.btn` 里嵌一枚 `.kbd`）：字在左、键帽在右，中间隔 `key-button-gap`。高、左右留白、底色、描边与
/// 字色照这块 `ui` 眼下的按钮样式——在 `ui.scope` 里换成主按钮、幽灵按钮就跟着换；字号照 [`buttons`] 那一块换上的那一档。
/// `enabled` 为假时按不动、整颗淡下去（设计稿 `.btn[disabled]` 的 `opacity`，令牌 `disabled-opacity`）。无障碍树上报成一颗
/// 按钮，名字是按钮上的字。待确认屏逐条那一屏「通过所选候选 Y」那四颗用它。
pub fn key_button(ui: &mut egui::Ui, label: &str, key: &str, enabled: bool) -> egui::Response {
    let tokens = Tokens::builtin();
    let 字体 = ui
        .style()
        .override_font_id
        .clone()
        .unwrap_or_else(|| egui::TextStyle::Body.resolve(ui.style()));
    let 字 = ui
        .painter()
        .layout_no_wrap(label.to_owned(), 字体, egui::Color32::PLACEHOLDER);
    let (键尺寸, 键字) = kbd_galley(ui, key);
    let 留白 = ui.spacing().button_padding;
    let 缝 = tokens.space.key_button_gap;
    let 宽 = 留白.x + 字.size().x + 缝 + 键尺寸.x + 留白.x;
    let 高 = ui
        .spacing()
        .interact_size
        .y
        .max(字.size().y.max(键尺寸.y) + 2.0 * 留白.y);
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(宽, 高), sense);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    if ui.is_rect_visible(rect) {
        let 淡 = if enabled {
            1.0
        } else {
            tokens.mix.disabled_opacity
        };
        let visuals = if enabled {
            *ui.style().interact(&response)
        } else {
            ui.style().visuals.widgets.inactive
        };
        let painter = ui.painter();
        painter.rect(
            rect.expand(visuals.expansion),
            visuals.corner_radius,
            visuals.weak_bg_fill.gamma_multiply(淡),
            egui::Stroke::new(
                visuals.bg_stroke.width,
                visuals.bg_stroke.color.gamma_multiply(淡),
            ),
            egui::StrokeKind::Inside,
        );
        painter.galley_with_override_text_color(
            egui::pos2(rect.left() + 留白.x, rect.center().y - 字.size().y / 2.0),
            字,
            visuals.text_color().gamma_multiply(淡),
        );
        let 键框 = egui::Rect::from_min_size(
            egui::pos2(
                rect.right() - 留白.x - 键尺寸.x,
                rect.center().y - 键尺寸.y / 2.0,
            ),
            键尺寸,
        );
        paint_kbd(ui, 键框, 键字, 淡);
    }
    response
}

/// 这一块 `ui` 眼下那一套主题的颜色（令牌 `[color.light]` / `[color.dark]`）。自己画底色、描边、字色的控件从这儿取。
#[must_use]
pub fn palette(ui: &egui::Ui) -> &'static crate::tokens::Palette {
    Tokens::builtin()
        .color
        .theme(egui::Theme::from_dark_mode(ui.visuals().dark_mode))
}

/// 一颗**小号幽灵按钮**（设计稿 `.btn.ghost.sm`）：[`small_buttons`] 那一档的高与字，[`ghost_button`] 那一档的颜色。
pub fn small_ghost_button(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    small_buttons(ui, |ui| {
        ui.scope(|ui| {
            ghost_button(ui.visuals_mut());
            ui.button(text)
        })
        .inner
    })
}

/// 虚线一段多长、两段之间空多少，是线宽的几倍：浏览器画 `dashed` 大约是这个比例。
const DASH_RATIO: f32 = 3.0;

/// 一圈**虚线**描边，画在 `rect` 里头（设计稿 `border: 1px dashed`）。一段多长、两段之间空多少都是线宽的三倍（`DASH_RATIO`）。
pub fn dashed_outline(painter: &egui::Painter, rect: egui::Rect, stroke: egui::Stroke) {
    let 框 = rect.shrink(stroke.width / 2.0);
    dashed(
        painter,
        &[
            框.left_top(),
            框.right_top(),
            框.right_bottom(),
            框.left_bottom(),
            框.left_top(),
        ],
        stroke,
    );
}

/// 一道横的**虚线**（设计稿 `border-bottom: 1px dashed`），段长同 [`dashed_outline`]。
pub fn dashed_hline(painter: &egui::Painter, x: egui::Rangef, y: f32, stroke: egui::Stroke) {
    dashed(
        painter,
        &[egui::pos2(x.min, y), egui::pos2(x.max, y)],
        stroke,
    );
}

/// 沿着这几个点画一串虚线。
fn dashed(painter: &egui::Painter, points: &[egui::Pos2], stroke: egui::Stroke) {
    let 段 = DASH_RATIO * stroke.width;
    painter.extend(egui::Shape::dashed_line(points, stroke, 段, 段));
}

/// 色条贴在卡片的哪一边。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarEdge {
    /// 左沿（设计稿 `box-shadow: inset 3px 0 0`）。
    Left,
    /// 顶上（设计稿 `box-shadow: inset 0 3px 0`）。
    Top,
}

/// 一张**带色条的卡片**的底：面板底、大圆角，`edge` 那一边一道 `bar` 色、宽 `tier-bar`（设计稿 `.batch`、`.cand`、`.mgroup`）。
/// 里头摆什么由 `add` 给；描边由调用方描（选中时颜色不同）。交回整张的 `InnerResponse`。
///
/// **底要垫在字底下，而多高要摆完才知道**：先占两层位置，摆完量出整张再填。
pub fn barred_card<R>(
    ui: &mut egui::Ui,
    bar: egui::Color32,
    edge: BarEdge,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let tokens = Tokens::builtin();
    let 面 = palette(ui).panel;
    let 圆角 = tokens.radius.large;
    ui.vertical(|ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 0.0;
        let 色底 = ui.painter().add(egui::Shape::Noop);
        let 面底 = ui.painter().add(egui::Shape::Noop);
        let inner = add(ui);
        let 整张 = ui.min_rect();
        let 面那一块 = match edge {
            BarEdge::Left => egui::Rect::from_min_max(
                egui::pos2(整张.left() + tokens.layout.tier_bar, 整张.top()),
                整张.max,
            ),
            BarEdge::Top => egui::Rect::from_min_max(
                egui::pos2(整张.left(), 整张.top() + tokens.layout.tier_bar),
                整张.max,
            ),
        };
        ui.painter()
            .set(色底, egui::epaint::RectShape::filled(整张, 圆角, bar));
        ui.painter()
            .set(面底, egui::epaint::RectShape::filled(面那一块, 圆角, 面));
        inner
    })
}

/// 一组**分段开关**（设计稿 `.seg`）：几颗挨着的按钮，每一颗是 `options` 里的一项（值与写在上面的字），`selected` 那一颗是选中的。
/// 交回这一帧按下的那一项的值。
///
/// 外框铺凹陷底（`sunken`）、描一圈分隔线色（`line`）、中圆角，与里头的按钮之间留 `seg-padding`；每一颗高
/// `seg-button-height`、左右留白 `seg-button-padding`、小圆角，字取说明字号 `size-small`。**选中那一颗**铺面板底
/// （`panel`）、字取正文色（`ink`）、描一圈 `line-2`；没选中的透明底、次要字色（`ink-2`），悬停时字换正文色。
/// 设计稿选中那一颗加粗，中文不加粗（字体预算），这里一律常规体。拿到焦点那一颗描一圈强调色。
///
/// 待确认屏屏头的「按批｜逐条」、一批里「按目录｜按候选作品｜按命名规律」那一排，以及子库「目标设置」弹层里
/// 的前端格式那一排（票 `gui-looks-like-the-design/21`），用的都是它。
///
/// 合票 18 与票 21 时两边**各自加过一个同名的 `segmented`**：一个交回下标、一个直接交回选中的那个值。
/// 留下这一个泛型的——它是超集，调用方不必再自己在下标与值之间来回映射。
pub fn segmented<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    options: &[(T, &str)],
    selected: T,
) -> Option<T> {
    let tokens = Tokens::builtin();
    let palette = palette(ui);
    let 外边 = tokens.layout.seg_padding;
    let 高 = tokens.layout.seg_button_height;
    let 留白 = tokens.layout.seg_button_padding;
    let 字号 = font_size(ui.ctx(), tokens.font.size_small);
    let 字: Vec<std::sync::Arc<egui::Galley>> = options
        .iter()
        .map(|(_, label)| {
            ui.painter().layout_no_wrap(
                (*label).to_owned(),
                egui::FontId::proportional(字号),
                palette.ink,
            )
        })
        .collect();
    let 宽: f32 = 字.iter().map(|one| one.size().x + 2.0 * 留白).sum();
    let (外框, _) = ui.allocate_exact_size(
        egui::vec2(宽 + 2.0 * 外边, 高 + 2.0 * 外边),
        egui::Sense::hover(),
    );
    let 线宽 = tokens.layout.control_stroke;
    ui.painter().rect(
        外框,
        tokens.radius.medium,
        palette.sunken,
        egui::Stroke::new(线宽, palette.line),
        egui::StrokeKind::Inside,
    );
    let mut 按下 = None;
    let mut 左 = 外框.left() + 外边;
    for (at, ((value, label), galley)) in options.iter().zip(字).enumerate() {
        let 这一颗 = egui::Rect::from_min_size(
            egui::pos2(左, 外框.top() + 外边),
            egui::vec2(galley.size().x + 2.0 * 留白, 高),
        );
        左 = 这一颗.right();
        let response = ui.interact(这一颗, ui.id().with(("分段开关", at)), egui::Sense::click());
        let 选中 = *value == selected;
        let enabled = ui.is_enabled();
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, enabled, 选中, *label)
        });
        let painter = ui.painter();
        if 选中 {
            painter.rect(
                这一颗,
                tokens.radius.small,
                palette.panel,
                egui::Stroke::new(线宽, palette.line_2),
                egui::StrokeKind::Inside,
            );
        }
        if response.has_focus() {
            painter.rect_stroke(
                这一颗,
                tokens.radius.small,
                egui::Stroke::new(2.0 * 线宽, palette.accent),
                egui::StrokeKind::Inside,
            );
        }
        let 字色 = if 选中 || response.hovered() {
            palette.ink
        } else {
            palette.ink_2
        };
        let 摆在 = 这一颗.center() - galley.size() / 2.0;
        painter.galley_with_override_text_color(摆在, galley, 字色);
        if response.clicked() {
            按下 = Some(*value);
        }
    }
    按下
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

    /// 令牌里**界面还没有一处用上**的颜色：页面背景（设计稿自己用，不进 egui）。
    /// 哪一屏第一个用上它，就从这张单子里划掉（下面那条变异测试会提醒）。`hi-soft` 与 `mid-soft`
    /// 由标签（[`tone_colors`]）用上了（票 `gui-looks-like-the-design/05`，开场与添加主库向导）；
    /// `none-soft` 与 `lo-soft` 由任务屏历史收场那一格用上了（票 `gui-looks-like-the-design/25`）；子库屏「未连接」那枚标签
    /// （[`Tone::Neutral`]）与删减建议表的表头（[`Tone::Bad`]）也用它们（票 `gui-looks-like-the-design/20`）。
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
        // 标签四种语气（[`tone_colors`]）：字与底各一格。
        for (tone, [字键, 底键]) in [
            (Tone::Good, ["hi", "hi-soft"]),
            (Tone::Caution, ["mid", "mid-soft"]),
            (Tone::Neutral, ["none", "none-soft"]),
            (Tone::Bad, ["lo", "lo-soft"]),
            (Tone::Accent, ["accent-ink", "accent-soft"]),
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
    fn 危险按钮的颜色取自令牌() {
        // 票 `gui-looks-like-the-design/20`：删除确认弹层上那一颗（`.btn.danger`）。卡底「删除子库」用的警示按钮取 main 上
        // 票 25 那一份，它的颜色由那边的测试钉着。
        for theme in [Theme::Dark, Theme::Light] {
            let p = Tokens::builtin().color.theme(theme);
            let mut 危险 = egui::Visuals::light();
            danger_button_in(p, &mut 危险);
            for (档, widget) in [
                ("inactive", &危险.widgets.inactive),
                ("hovered", &危险.widgets.hovered),
                ("active", &危险.widgets.active),
            ] {
                assert_eq!(widget.bg_fill, p.lo, "{theme:?} 危险按钮 {档} 的底");
                assert_eq!(widget.weak_bg_fill, p.lo, "{theme:?} 危险按钮 {档} 的底");
                assert_eq!(
                    widget.fg_stroke.color, p.on_accent,
                    "{theme:?} 危险按钮 {档} 的字"
                );
            }
            assert_eq!(
                危险.widgets.active.bg_stroke.color, p.on_accent,
                "{theme:?} 焦点圈"
            );
        }
    }

    #[test]
    fn 按不动的危险按钮换成浅底而不是把实心底调淡() {
        // 票 `gui-looks-like-the-design/26`：照背景调淡那一条（稿上 `.btn[disabled]{opacity:.45}`）
        // 在暗色里分不出能不能按——暗色 `lo` 比面板**亮**，兑一半仍旧是一整块实心色。
        for theme in [Theme::Dark, Theme::Light] {
            let p = Tokens::builtin().color.theme(theme);
            let mut 按不动 = egui::Visuals::light();
            disabled_danger_button_in(p, &mut 按不动);
            for (档, widget) in [
                ("inactive", &按不动.widgets.inactive),
                ("hovered", &按不动.widgets.hovered),
                ("active", &按不动.widgets.active),
            ] {
                assert_eq!(
                    widget.bg_fill, p.lo_soft,
                    "{theme:?} 按不动那一档 {档} 的底"
                );
                assert_eq!(
                    widget.weak_bg_fill, p.lo_soft,
                    "{theme:?} 按不动那一档 {档} 的底"
                );
                assert_eq!(
                    widget.fg_stroke.color, p.lo,
                    "{theme:?} 按不动那一档 {档} 的字"
                );
            }
            // **两档不许撞脸**：撞上了，屏上就分不出能按与不能按。
            let mut 能按 = egui::Visuals::light();
            danger_button_in(p, &mut 能按);
            assert_ne!(
                能按.widgets.inactive.bg_fill, 按不动.widgets.inactive.bg_fill,
                "{theme:?} 能按与按不动的底色撞脸了"
            );
        }
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

    /// 这一帧画出来的每一段字：`(字, 摆在哪儿, 字号, 颜色)`。
    fn 画出来的段(output: &egui::FullOutput) -> Vec<(String, egui::Rect, f32, Color32)> {
        fn 收(shape: &egui::Shape, out: &mut Vec<(String, egui::Rect, f32, Color32)>) {
            match shape {
                egui::Shape::Text(text) => {
                    let format = text
                        .galley
                        .job
                        .sections
                        .first()
                        .map(|section| section.format.clone())
                        .unwrap_or_default();
                    out.push((
                        text.galley.text().to_owned(),
                        egui::Rect::from_min_size(text.pos, text.galley.size()),
                        format.font_id.size,
                        format.color,
                    ));
                }
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|one| 收(one, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for clipped in &output.shapes {
            收(&clipped.shape, &mut out);
        }
        out
    }

    /// 这一帧画出来的每一条横线：`(竖向位置, 横向范围, 颜色)`。
    fn 画出来的横线(output: &egui::FullOutput) -> Vec<(f32, egui::Rangef, Color32)> {
        fn 收(shape: &egui::Shape, out: &mut Vec<(f32, egui::Rangef, Color32)>) {
            match shape {
                egui::Shape::LineSegment { points, stroke } if points[0].y == points[1].y => {
                    out.push((
                        points[0].y,
                        egui::Rangef::new(points[0].x, points[1].x),
                        stroke.color,
                    ));
                }
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|one| 收(one, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for clipped in &output.shapes {
            收(&clipped.shape, &mut out);
        }
        out
    }

    /// 画一帧屏头，右侧那一段是一颗写着 `动作` 的按钮。交回这一帧的产出、**交给屏头的那一块**多大、按钮多大。
    ///
    /// 量的是交给它的那一块，不是屏头自己交回来的 `rect`：里头的东西摆出界时，面板的 `rect` 跟着撑宽，
    /// 拿它当右沿，「靠右」这条断言就永远成立。
    fn 画屏头(ctx: &egui::Context, 动作: &str) -> (egui::FullOutput, egui::Rect, egui::Rect) {
        let mut 量到 = None;
        let output = headless::frame(ctx, headless::input(), |ui| {
            let 交给它的 = ui.max_rect();
            let 头 = screen_header(ui, "屏头", "标题", "一句副标题", |ui| {
                ui.button(动作).rect
            });
            量到 = Some((交给它的.intersect(头.response.rect), 头.inner));
        });
        let (头, 钮) = 量到.expect("画过屏头");
        (output, 头, 钮)
    }

    #[test]
    fn 屏头照令牌摆_标题副标题靠左_右侧那一段靠右_底下一道线() {
        // 票 `gui-looks-like-the-design/32`：设计稿 `.scrhead`——标题 18、副标题 12.5 的弱字、右侧动作、
        // 下边一道线、内边距 14/20、彼此隔 12。
        let tokens = Tokens::builtin();
        let [上下, 左右] = tokens.space.screen_header_padding;
        let ctx = headless::context();
        install(&ctx);
        let p = tokens.color.theme(ctx.theme());
        // 头一帧 egui 还在量尺寸，第二帧才是摆稳的样子。
        画屏头(&ctx, "动作");
        let (output, 头, 钮) = 画屏头(&ctx, "动作");
        let 段 = 画出来的段(&output);
        let 找 = |字: &str| {
            段.iter()
                .find(|(画的, ..)| 画的 == 字)
                .cloned()
                .unwrap_or_else(|| panic!("屏头上没画「{字}」：{段:?}"))
        };

        let (_, 标题, 标题字号, 标题色) = 找("标题");
        assert_eq!(标题字号, tokens.font.size_page, "标题字号");
        assert_eq!(标题色, p.ink, "标题是强调字");
        assert!(
            (标题.left() - (头.left() + 左右)).abs() < 0.5,
            "标题左边该离屏头左沿 {左右}：标题 {标题:?}，屏头 {头:?}",
        );
        let (_, 副标题, 副标题字号, 副标题色) = 找("一句副标题");
        assert_eq!(
            副标题字号,
            font_size(&ctx, tokens.font.size_small_plus),
            "副标题字号（半号按倍率取整）"
        );
        assert_eq!(副标题色, p.ink_3, "副标题是弱字");
        assert!(
            (副标题.left() - 标题.right() - tokens.space.screen_header_gap).abs() < 0.5,
            "副标题该紧跟标题、隔 {}：标题 {标题:?}，副标题 {副标题:?}",
            tokens.space.screen_header_gap,
        );

        assert!(
            (钮.right() - (头.right() - 左右)).abs() < 0.5,
            "右侧那一段该靠右、离右沿 {左右}：按钮 {钮:?}，屏头 {头:?}",
        );
        assert!(
            (钮.top() - (头.top() + 上下)).abs() < 0.5,
            "右侧那一段该离上沿 {上下}：按钮 {钮:?}，屏头 {头:?}",
        );
        let 底线 = 画出来的横线(&output).into_iter().find(|(y, 横, 色)| {
            *色 == p.line
                && *y > 钮.bottom() + 上下 - 0.5
                && *y <= 头.bottom()
                && 横.span() >= 头.width() - 0.5
        });
        assert!(
            底线.is_some(),
            "屏头底下该有一道 line 色、横贯整个屏头的线：{:?}",
            画出来的横线(&output)
        );

        // **右侧那一段换了宽度，同一帧就靠右**：按钮上的字变长的那一帧不许先画在左边、下一帧才挪过去
        // ——测试照上一帧的位置去点，点的正是那一帧。
        let (_, 头, 钮) = 画屏头(&ctx, "长得多的一段动作");
        assert!(
            (钮.right() - (头.right() - 左右)).abs() < 0.5,
            "右侧那一段换了宽度的那一帧没靠右：按钮 {钮:?}，屏头 {头:?}",
        );
    }

    #[test]
    fn 屏头右侧那一段摆不下时照稿折到下一行_头一帧就画得出来() {
        // 设计稿 `.scrhead{flex-wrap:wrap;gap:12px}`：右侧那一段在标题、副标题后面摆不下，整段折到下一行，
        // 从左边内边距起摆，与上一行隔 12。不折的话它溢出屏头右沿，后半截被裁掉。
        //
        // **头一帧就得画出来**：面板这一帧按它上一帧的高裁剪，折下来的那一行落在裁剪框外，egui 的字干脆不画
        // ——测试只跑一帧就读屏上的字，读不到的正是那一行。
        let tokens = Tokens::builtin();
        let [上下, 左右] = tokens.space.screen_header_padding;
        let ctx = headless::context();
        install(&ctx);
        let mut 量到 = None;
        let mut 头一帧 = None;
        for _ in 0..2 {
            let output = headless::frame(&ctx, headless::input(), |ui| {
                let 交给它的 = ui.max_rect();
                let 头 = screen_header(ui, "屏头", "标题", "一句副标题", |ui| {
                    ui.label("折下来的字");
                    // 连上前面那几个字，在标题、副标题后面摆不下，自己一行摆得下。
                    ui.allocate_exact_size(
                        egui::vec2(交给它的.width() - 2.0 * 左右 - 100.0, 20.0),
                        egui::Sense::hover(),
                    );
                    // 这一样在折下来的那一行里也摆不下：再往下折一行，不溢出右沿（`flex-wrap` 折的是每一样，不是整段）。
                    ui.label("再折一行的字");
                });
                量到 = Some((交给它的, 头.response.rect));
            });
            头一帧.get_or_insert(output);
        }
        let 头一帧 = 头一帧.expect("跑过帧");
        assert!(
            画出来的段(&头一帧)
                .iter()
                .any(|(画的, ..)| 画的 == "折下来的字"),
            "头一帧上折下来的那一行没画出来：{:?}",
            画出来的段(&头一帧),
        );
        let (区, 头) = 量到.expect("画过屏头");
        let (右侧, _) = 那一段(&头一帧, "折下来的字");
        assert!(
            (右侧.left() - (区.left() + 左右)).abs() < 0.5,
            "折下来的那一段该从左边内边距起摆：{右侧:?}，交给屏头的 {区:?}",
        );
        let 第一行底 = 区.top() + 上下 + tokens.layout.button_height;
        assert!(
            右侧.top() >= 第一行底 + tokens.space.screen_header_gap - 0.5,
            "折下来的那一段该在第一行底下、隔 {}：{右侧:?}，第一行底在 {第一行底}",
            tokens.space.screen_header_gap,
        );
        assert!(
            右侧.bottom() <= 头.bottom(),
            "屏头该跟着长高把它包住：{右侧:?}，屏头 {头:?}"
        );
        let (再折, _) = 那一段(&头一帧, "再折一行的字");
        assert!(
            (再折.left() - (区.left() + 左右)).abs() < 0.5 && 再折.top() > 右侧.bottom(),
            "折下来那一行里摆不下的那一样该再往下折一行、从左边起摆：{再折:?}，上一行 {右侧:?}",
        );
        assert!(
            再折.bottom() <= 头.bottom(),
            "屏头该跟着长高把它包住：{再折:?}，屏头 {头:?}"
        );
    }

    /// 画一帧屏体：头一行、中间两屏高的一截、末一行。交回交给屏体的那一块、头一行、末一行、屏体里剩下多宽。
    fn 画屏体(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> (egui::Rect, egui::Rect, egui::Rect, f32) {
        let mut 量到 = None;
        let mut input = headless::input();
        input.events = events;
        headless::frame(ctx, input, |ui| {
            let 交给它的 = ui.max_rect();
            let (头一行, 末一行, 宽) = screen_body(ui, "屏体", |ui| {
                let 宽 = ui.available_width();
                let 头一行 = ui.label("头一行").rect;
                ui.add_space(headless::VIEWPORT[1] * 2.0);
                (头一行, ui.label("末一行").rect, 宽)
            });
            量到 = Some((交给它的, 头一行, 末一行, 宽));
        });
        量到.expect("画过屏体")
    }

    #[test]
    fn 屏体照令牌留内边距_滚到底下面还留着那一截() {
        // 票 `gui-looks-like-the-design/32`：设计稿 `.scrbody{padding:18px 20px 28px;overflow:auto}`。
        // 底下那 28 点只有滚到底才看得见，所以真发滚轮事件滚到底再量（`tests/queue.rs` 的办法）。
        let [上, 左右, 下] = Tokens::builtin().space.screen_body_padding;
        let ctx = headless::context();
        install(&ctx);
        画屏体(&ctx, Vec::new());
        let (区, 头一行, _, 宽) = 画屏体(&ctx, Vec::new());
        assert!(
            (头一行.left() - (区.left() + 左右)).abs() < 0.5,
            "头一行该离左沿 {左右}：{头一行:?}，交给屏体的 {区:?}",
        );
        assert!(
            (头一行.top() - (区.top() + 上)).abs() < 0.5,
            "头一行该离上沿 {上}：{头一行:?}，交给屏体的 {区:?}",
        );
        assert!(
            (宽 - (区.width() - 2.0 * 左右)).abs() < 0.5,
            "屏体里剩下的宽该是左右各让出 {左右}：剩 {宽}，交给屏体的 {区:?}",
        );

        // 往下滚，滚到末一行不再动为止（等的是它停下，不是等一段时间）。
        let 滚 = || {
            vec![
                egui::Event::PointerMoved(区.center()),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -headless::VIEWPORT[1]),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        let mut 上一帧 = 画屏体(&ctx, 滚()).2;
        let mut 停了 = false;
        for _ in 0..200 {
            let 这一帧 = 画屏体(&ctx, 滚()).2;
            if 这一帧 == 上一帧 {
                停了 = true;
                break;
            }
            上一帧 = 这一帧;
        }
        assert!(停了, "滚了两百帧末一行还在动");
        assert!(
            (上一帧.bottom() - (区.bottom() - 下)).abs() < 0.5,
            "滚到底时末一行该离下沿 {下}：{上一帧:?}，交给屏体的 {区:?}",
        );
    }

    /// 一帧的输入：视口照旧，这块屏的像素倍率是 `倍率`。
    fn 倍率输入(倍率: f32) -> egui::RawInput {
        let mut input = headless::input();
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(倍率);
        input
    }

    /// 这一帧里正好写着 `字` 的那一段画成几号字、摆在哪儿。
    fn 那一段(output: &egui::FullOutput, 字: &str) -> (egui::Rect, f32) {
        let (_, 在, 字号, _) = 画出来的段(output)
            .into_iter()
            .find(|(画的, ..)| 画的 == 字)
            .unwrap_or_else(|| panic!("没画「{字}」"));
        (在, 字号)
    }

    #[test]
    fn 一段说明折行时段末不留孤字_末行至少两个字() {
        // 第十四版库屏候选图审稿打回：空库时数据源那一句最后折出一个孤零零的「们。」。
        let ctx = headless::context();
        install(&ctx);
        let 字 = "甲乙丙丁戊己庚辛壬癸子丑。";
        /// 这一帧画出来的 `字` 那一段，各行的字。
        fn 各行(output: &egui::FullOutput, 字: &str) -> Vec<String> {
            fn 找(shape: &egui::epaint::Shape, 字: &str) -> Option<Vec<String>> {
                match shape {
                    egui::epaint::Shape::Text(text) if text.galley.text() == 字 => Some(
                        text.galley
                            .rows
                            .iter()
                            .map(|row| row.glyphs.iter().map(|glyph| glyph.chr).collect())
                            .collect(),
                    ),
                    egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|one| 找(one, 字)),
                    _ => None,
                }
            }
            output
                .shapes
                .iter()
                .find_map(|clipped| 找(&clipped.shape, 字))
                .unwrap_or_else(|| panic!("没画「{字}」"))
        }
        // 折行宽卡在「丑」的正中：照 egui 自己折，「丑」连着句号落到第二行，成了孤字。
        let mut 宽 = None;
        headless::frame(&ctx, headless::input(), |ui| {
            let 一行 = egui::WidgetText::from(字).into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::FontSelection::Default,
            );
            let 丑 = 一行.rows[0].glyphs[11];
            宽 = Some(丑.pos.x + 丑.advance_width / 2.0);
        });
        let 宽 = 宽.expect("量过");
        let 画 = |段: fn(&mut egui::Ui, &str) -> egui::Response| {
            headless::frame(&ctx, headless::input(), |ui| {
                ui.scope(|ui| {
                    ui.set_max_width(宽);
                    段(ui, 字);
                });
            })
        };
        let 照常 = 各行(&画(|ui, 字| ui.weak(字)), 字);
        assert_eq!(
            照常,
            ["甲乙丙丁戊己庚辛壬癸子", "丑。"],
            "前提：照常折行末行是孤字"
        );

        let 不留孤字 = 各行(&画(weak_paragraph), 字);
        assert_eq!(
            不留孤字,
            ["甲乙丙丁戊己庚辛壬癸", "子丑。"],
            "上一行末尾那个字该挪下来，末行凑够两个字、行数不变",
        );

        // 本来就不折行的一句，宽度原样。
        let 一行 = 各行(
            &headless::frame(&ctx, headless::input(), |ui| {
                weak_paragraph(ui, 字);
            }),
            字,
        );
        assert_eq!(一行, [字], "放得下的一句不该被折开");
    }

    #[test]
    fn 半号字号按像素倍率取整到整像素() {
        // 拿主意的人 2026-09-14 定（挂单 `Q863`）：1 倍屏上 egui 把中文字形的横坐标取整，12.5 像素宽的字只能
        // 12、13 交替落，字距忽宽忽窄。于是字号先乘倍率取整到整像素再除回去：1 倍屏画 13 / 12，2 倍屏画 25 / 23 像素。
        for (字号, 倍率, 画成) in [
            (12.5, 1.0, 13.0),
            (11.5, 1.0, 12.0),
            (12.5, 2.0, 12.5),
            (11.5, 2.0, 11.5),
            (13.0, 1.0, 13.0),
            (13.0, 2.0, 13.0),
        ] {
            let ctx = headless::context();
            let mut 取到 = None;
            for _ in 0..2 {
                headless::frame(&ctx, 倍率输入(倍率), |ui| {
                    取到 = Some(font_size(ui.ctx(), 字号));
                });
            }
            assert_eq!(取到, Some(画成), "{字号} 号字在 {倍率} 倍屏上");
        }
    }

    #[test]
    fn 三档按钮上的字照稿_默认那一档是半号按倍率取整_不进那一块的按钮与正文一样大() {
        // 设计稿 `.btn` 12.5（令牌 `size-small-plus`）、`.btn.sm` 12、`.btn.lg` 14，拿主意的人 2026-09-14 定照稿。
        // **不进那一块的按钮与正文一样大**：egui 0.36 的按钮取字只认 `override_font_id`，没有就用正文那一档——
        // 这一条钉着它，哪天 egui 改了取法，默认那一档就能装成全窗口的默认（挂单 `Q862`）。
        let font = &Tokens::builtin().font;
        let ctx = headless::context();
        install(&ctx);
        for (倍率, 默认) in [(1.0, 13.0), (2.0, 12.5)] {
            let mut 输出 = None;
            for _ in 0..2 {
                输出 = Some(headless::frame(&ctx, 倍率输入(倍率), |ui| {
                    drop(ui.button("正文里的"));
                    buttons(ui, |ui| ui.button("默认"));
                    small_buttons(ui, |ui| ui.button("小号"));
                    large_buttons(ui, |ui| ui.button("大号"));
                }));
            }
            let 输出 = 输出.expect("跑过帧");
            assert_eq!(
                那一段(&输出, "正文里的").1,
                font.size_body,
                "{倍率} 倍屏上没进那一块的按钮"
            );
            assert_eq!(
                那一段(&输出, "默认").1,
                默认,
                "{倍率} 倍屏上默认那一档按钮的字"
            );
            assert_eq!(
                那一段(&输出, "小号").1,
                font.size_small,
                "{倍率} 倍屏上小号按钮的字"
            );
            assert_eq!(
                那一段(&输出, "大号").1,
                font.size_button_large,
                "{倍率} 倍屏上大号按钮的字"
            );
        }
    }

    #[test]
    fn 标签照设计稿_高与左右留白_圆点与字的间距_字取半号那一档() {
        // 设计稿 `.chip{gap:5px;height:20px;padding:0 7px;font-size:11.5px}`，圆点 6（拿主意的人 2026-09-14 定照稿）。
        let t = Tokens::builtin();
        let ctx = headless::context();
        install(&ctx);
        let mut 量到 = None;
        let mut 输出 = None;
        for _ in 0..2 {
            输出 = Some(headless::frame(&ctx, headless::input(), |ui| {
                量到 = Some(chip(ui, Tone::Caution, "版本不兼容").rect);
            }));
        }
        let (框, 输出) = (量到.expect("画过标签"), 输出.expect("跑过帧"));
        let (字, 字号) = 那一段(&输出, "版本不兼容");
        assert_eq!(框.height(), t.layout.chip_height, "标签的高");
        assert!(
            (字.left()
                - (框.left() + t.layout.chip_padding + t.layout.chip_dot + t.layout.chip_gap))
                .abs()
                < 0.5,
            "字该在左留白、圆点、间距之后：框 {框:?}，字 {字:?}",
        );
        assert!(
            (框.right() - 字.right() - t.layout.chip_padding).abs() < 0.5,
            "字右边该留 {}：框 {框:?}，字 {字:?}",
            t.layout.chip_padding,
        );
        assert_eq!(字号, font_size(&ctx, t.font.size_caption_plus), "标签的字");
    }

    #[test]
    fn 只读标签照设计稿_比标签高一截_字是说明字号_高置信那一对颜色() {
        // 设计稿 `.ro{gap:6px;height:22px;padding:0 9px;font-size:12px;background:var(--hi-soft);color:var(--hi)}`，
        // 圆点 6（`.ro::before`）。拿主意的人 2026-09-14 定：与 `.chip` 单列一档（挂单 `Q862`）。
        let t = Tokens::builtin();
        let ctx = headless::context();
        install(&ctx);
        let p = t.color.theme(ctx.theme());
        let mut 量到 = None;
        let mut 输出 = None;
        for _ in 0..2 {
            输出 = Some(headless::frame(&ctx, headless::input(), |ui| {
                量到 = Some(read_only(ui, "只读").rect);
            }));
        }
        let (框, 输出) = (量到.expect("画过只读标签"), 输出.expect("跑过帧"));
        let (_, 字, 字号, 字色) = 画出来的段(&输出)
            .into_iter()
            .find(|(画的, ..)| 画的 == "只读")
            .expect("画了「只读」");
        assert_eq!(框.height(), t.layout.ro_height, "只读标签的高");
        assert!(
            (字.left() - (框.left() + t.layout.ro_padding + t.layout.chip_dot + t.layout.ro_gap))
                .abs()
                < 0.5,
            "字该在左留白、圆点、间距之后：框 {框:?}，字 {字:?}",
        );
        assert!(
            (框.right() - 字.right() - t.layout.ro_padding).abs() < 0.5,
            "字右边该留 {}：框 {框:?}，字 {字:?}",
            t.layout.ro_padding,
        );
        assert_eq!(字号, t.font.size_small, "只读标签的字");
        assert_eq!(字色, p.hi, "只读标签的字是高置信色");
    }

    #[test]
    fn 警示按钮的颜色取自令牌() {
        // 设计稿 `.btn.warn`（拿主意的人定）：白底 `panel`、红字红描边 `lo`，悬停也是红描边。两套主题各查一遍。
        for theme in [Theme::Dark, Theme::Light] {
            let p = Tokens::builtin().color.theme(theme);
            let mut 警示 = theme.default_visuals();
            warn_button(&mut 警示);
            let w = &警示.widgets;
            assert_eq!(w.inactive.weak_bg_fill, p.panel, "{theme:?}");
            assert_eq!(w.inactive.bg_stroke.color, p.lo, "{theme:?}");
            assert_eq!(w.inactive.fg_stroke.color, p.lo, "{theme:?}");
            assert_eq!(w.hovered.bg_stroke.color, p.lo, "{theme:?}");
        }
    }

    #[test]
    fn 幽灵按钮的颜色取自令牌() {
        // 设计稿 `.btn.ghost`：底与描边透明、字 `ink-2`，悬停出 `sunken` 底、字 `ink`。两套主题各查一遍。
        for theme in [Theme::Dark, Theme::Light] {
            let p = Tokens::builtin().color.theme(theme);
            let mut 幽灵 = theme.default_visuals();
            ghost_button(&mut 幽灵);
            let w = &幽灵.widgets;
            assert_eq!(w.inactive.weak_bg_fill, Color32::TRANSPARENT, "{theme:?}");
            assert_eq!(
                w.inactive.bg_stroke.color,
                Color32::TRANSPARENT,
                "{theme:?}"
            );
            assert_eq!(w.inactive.fg_stroke.color, p.ink_2, "{theme:?}");
            assert_eq!(w.hovered.weak_bg_fill, p.sunken, "{theme:?}");
            assert_eq!(w.hovered.fg_stroke.color, p.ink, "{theme:?}");
        }
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
