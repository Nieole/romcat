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
//! **只换颜色，线宽一个点都不动**（理由见下面「键盘焦点」一节）；屏的版式由各屏自己的票重排。
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
//! **宽度一个点都不动。** egui 把描边宽度反过来从按钮的内边距里扣
//! （`Style::button_style`），加宽会让控件在得到焦点的那一帧缩一下——焦点在控件之间跳的
//! 时候，整排按钮跟着抖。

use egui::Color32;
use romcat_core::catalog::identify::Tier;
use romcat_core::task::Ending;

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
        });
    }
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

/// 那一套主题的 `Visuals`：**每一个颜色都取自令牌**；不是颜色的（线宽、手柄形状……）
/// 照 egui 原样。
///
/// **线宽一个点都不动。** egui 把描边宽度从按钮的内边距里扣，而「不画框的那一档」只留
/// 内边距不画描边——给哪一档加宽，可选标签就会在悬停前后差一个点。
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

/// **收场四档**画成什么颜色：`(字与圆点, 浅底)`。**全窗口只有这一处回答这个问题。**
///
/// 画它的是任务屏历史「收场」那一格（设计稿 `.chip`）。配色照设计稿：完成 `hi`、已取消
/// `none`、部分完成 `mid`、失败 `lo`，底色取各自的 `-soft`。与置信度四档同一个办法：
/// `Visuals` 里没有槽位，按 `visuals` 是哪一套主题挑那一套令牌。**颜色不是唯一线索**——
/// 格子里照样写着那一档的词（[`Ending::word`]）。
#[must_use]
pub fn ending_colors(ending: &Ending<()>, visuals: &egui::Visuals) -> (Color32, Color32) {
    let theme = egui::Theme::from_dark_mode(visuals.dark_mode);
    ending_colors_in(Tokens::builtin().color.theme(theme), ending)
}

/// 那一档在这一套颜色里取哪两个。拆出来的理由同 [`tier_color_in`]。
fn ending_colors_in(palette: &Palette, ending: &Ending<()>) -> (Color32, Color32) {
    match ending {
        Ending::Done(()) => (palette.hi, palette.hi_soft),
        Ending::Stopped => (palette.none, palette.none_soft),
        Ending::Halfway { .. } => (palette.mid, palette.mid_soft),
        Ending::Failed { .. } => (palette.lo, palette.lo_soft),
    }
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

    /// 令牌里**界面还没有一处用上**的颜色：页面背景（设计稿自己用）、四档的浅底
    /// ——egui 的 `Visuals` 里没有它们的槽位，而画它们的那几屏还没照稿重排。
    /// 哪一屏第一个用上它，就从这张单子里划掉（下面那条变异测试会提醒）。
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
        收场: impl Fn(&Ending<()>) -> (Color32, Color32),
        主按钮: &egui::style::Widgets,
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
        // 任务屏历史「收场」那一格（[`ending_colors`]）：字与圆点、浅底，照设计稿 `.chip`。
        for (ending, [ink, soft]) in [
            (Ending::Done(()), ["hi", "hi-soft"]),
            (Ending::Stopped, ["none", "none-soft"]),
            (
                Ending::Halfway {
                    product: (),
                    left_behind: String::new(),
                },
                ["mid", "mid-soft"],
            ),
            (
                Ending::Failed {
                    step: String::new(),
                    why: String::new(),
                },
                ["lo", "lo-soft"],
            ),
        ] {
            let (got_ink, got_soft) = 收场(&ending);
            颜色.push((format!("收场（{}）字", ending.word()), got_ink, ink));
            颜色.push((format!("收场（{}）底", ending.word()), got_soft, soft));
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
            |ending: &Ending<()>| ending_colors(ending, &style.visuals),
            &主按钮.widgets,
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
            let 收场 = |ending: &Ending<()>| ending_colors_in(改过.color.theme(theme), ending);
            let mut 主按钮 = 装出来.visuals.clone();
            primary_button_in(改过.color.theme(theme), &mut 主按钮);
            let 主按钮 = &主按钮.widgets;
            if !颜色_偏离(&改过, theme, &装出来.visuals, 四档, 遮罩, 收场, 主按钮).is_empty()
            {
                断了.push(*key);
            }
            if !颜色_偏离(
                Tokens::builtin(),
                theme,
                &装出来.visuals,
                四档,
                遮罩,
                收场,
                主按钮,
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
