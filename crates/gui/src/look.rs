//! 五屏共用的**观感基线**：置信度四档标成什么样、键盘焦点长什么样。
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
//! ## 颜色不是唯一线索
//!
//! 每一处上色的地方**都跟着那个词**：色条旁边有标签、被染色的数字后面有档名。
//! 色觉障碍下颜色全糊成一片，那时读得出来的只有字。这条不是建议，是这一票的验收。
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

use romcat_core::catalog::identify::Tier;

/// 色条多宽，点。行左边缘那一条，照原型 `prototype.html` 里 `.conf` 那条竖线。
const BAR: f32 = 3.0;

/// 把这个窗口的**观感基线**装上去。开窗那一路与不开窗跑帧那一路走的是同一句
/// （[`crate::app::App::ui`] 每次开头问一遍，只装一次）。
///
/// **亮暗两套主题都装**：`all_styles_mut` 一次改两份，不然人换一次系统主题，
/// 焦点就悄悄退回默认那个几乎看不见的样子。
pub fn install(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        // **焦点用主题自己的强调色**，与 `TextEdit` 拿到焦点时的那一圈同出一处。
        let accent = style.visuals.selection.stroke.color;
        style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, accent);
    });
}

/// **置信度四档**画成什么颜色。**全窗口只有这一处回答这个问题。**
///
/// 颜色从 `Visuals` 里取而不是写死：亮色与暗色主题下同一串 RGB 不是同一个可读性。
///
/// **高置信取的是 `selection.stroke` 而不是 `selection.bg_fill`**：后者是给背景用的，
/// 当前景色用在暗色主题上对比度只有 **2.33**、亮色主题上 **1.55**（WCAG AA 要 4.5），
/// 「高置信」三个字几乎读不出来；换成前者是 **12.41 / 7.81**。
/// 这一条正是「颜色不是唯一线索」的另一半——字得先读得出来才算线索。
#[must_use]
pub fn tier_color(tier: Tier, visuals: &egui::Visuals) -> egui::Color32 {
    match tier {
        Tier::High => visuals.selection.stroke.color,
        Tier::Medium => visuals.warn_fg_color,
        Tier::Low => visuals.error_fg_color,
        Tier::Unidentified => visuals.weak_text_color(),
    }
}

/// 行左边缘那条**置信度色条**：宽 3 点（`BAR`）、与一行正文一样高。
///
/// 它**从不单独出现**——摆它的地方旁边一定跟着 [`tier_label`] 或那个词本身
/// （[`tier_tag`] 把两样一起给出来）。
pub fn tier_bar(ui: &mut egui::Ui, tier: Tier) {
    let height = ui.text_style_height(&egui::TextStyle::Body);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(BAR, height), egui::Sense::hover());
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
    let rect = response
        .rect
        .intersect(egui::Rect::from_x_y_ranges(
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
                        颜色[at], 颜色[other],
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
        // 这一档从前取的是 `selection.bg_fill`——那是给背景用的一串，当前景色用时
        // 暗色主题下对比度只有 2.33、亮色主题下 1.55，远在 WCAG AA 的 4.5 之下。
        // 另外三档取的是 egui 自己的 `warn` / `error` / `weak`，这一票不动它们。
        for visuals in [egui::Visuals::dark(), egui::Visuals::light()] {
            let 对比度 = contrast(tier_color(Tier::High, &visuals), visuals.panel_fill);
            assert!(
                对比度 >= 4.5,
                "高置信在这套主题下对比度只有 {对比度:.2}",
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
