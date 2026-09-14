//! **左栏**：主窗口左边那条导航（设计稿 `.rail`，票 `gui-looks-like-the-design/32`）。
//!
//! 从上往下：顶上一张切换主库的卡（标志、主库原名、「切换主库」）；「整理 / 输出 / 后台」三组入口，每个入口
//! 右边一个计数；栏底是已保存多少条裁决与收起按钮。收起之后是一条窄条（设计稿 `.main.rcol`）：卡上只剩标志，
//! 分组标题变成一道线，入口的字与计数上下摞着，栏底只剩「»」。
//!
//! ## 这一层只画，不数
//!
//! 每个入口的计数由窗口那一层交进来（[`Facts::badge`]）——那几个数是各屏与核心库**现成的**那一份
//! （[`crate::app`] 模块文档「左栏的几个数从哪来」）。这一层不碰库、不碰屏，按下去的那一下交回一个
//! [`Pressed`]，由窗口那一层去换屏、回开场、记下收没收起。
//!
//! ## 收不收成窄条
//!
//! 人按的「收起 / 展开」记在工作目录的版式文件里；窗口宽不到令牌 `rail-collapse-below` 时自动收起，不记
//! （[`crate::layout::rail_folded`]）。**自动收着的时候那颗按钮按不下去**：按下去改的是人记下的那一份，
//! 屏上却一点不变——等窗口宽回来才突然展开或收起，那比按不动更难懂。
//!
//! 设置入口等设置屏（票 31）来了再摆；新手引导主程序眼下没有，栏底不摆那一颗。

use egui::{Color32, Rect, Sense, vec2};
use romcat_core::report::thousands;

use crate::app::View;
use crate::look;
use crate::tokens::Tokens;

/// 三组入口，照设计稿的次序。
pub const GROUPS: [(&str, &[View]); 3] = [
    ("整理", &[View::Library, View::Queue, View::Browse]),
    ("输出", &[View::Sublibraries]),
    ("后台", &[View::Tasks]),
];

/// 切换主库那张卡上的那句话。
pub const SWITCH: &str = "切换主库";
/// 展开时栏底那颗按钮上的字。
pub const FOLD: &str = "« 收起";
/// 收成窄条时栏底那颗按钮上的字。
pub const UNFOLD: &str = "»";

/// 任务跑着时计数前那枚圆点闪一下多久，秒（设计稿 `.nav .badge.live::before` 的 `animation:pulse 1.4s`）。
const PULSE_SECONDS: f64 = 1.4;

/// 一个入口右边那个计数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Badge {
    /// 不画数（任务台空着时的「任务」）。
    Nothing,
    /// 一个数，或者数不出来时的「—」。
    Count(String),
    /// **有活在跑**：前面带一枚强调色的圆点（「任务」那一项）。
    Live(String),
}

/// 画左栏要知道的几样。
pub struct Facts<'a> {
    /// 卡上写的主库原名。
    pub library: &'a str,
    /// 眼下看的是哪一屏。
    pub current: View,
    /// 每个入口右边那个计数。
    pub badge: &'a dyn Fn(View) -> Badge,
    /// 沉淀库里已保存多少条裁决；读不出来是 `None`。
    pub verdicts: Option<u64>,
    /// 人是不是把左栏收起了（版式文件里记的那一份）。
    pub chosen_collapsed: bool,
}

/// 左栏上按下去的那一下。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressed {
    /// 换到这一屏。
    Go(View),
    /// 回开场换一份库。
    SwitchLibrary,
    /// 人要把左栏收起（`true`）或展开（`false`）。
    Collapse(bool),
}

/// 画左栏：占掉这块 `ui` 左边那一截（一块 `egui::Panel::left`）。交回按下去的那一下。
pub fn show(ui: &mut egui::Ui, facts: &Facts<'_>) -> Option<Pressed> {
    let tokens = Tokens::builtin();
    let 窗口宽 = ui.ctx().content_rect().width();
    let folded = crate::layout::rail_folded(facts.chosen_collapsed, 窗口宽);
    let 自动收着 = crate::layout::rail_folded(false, 窗口宽);
    let [上下, 左右] = if folded {
        tokens.space.rail_padding_collapsed
    } else {
        tokens.space.rail_padding
    };
    let 框 = egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(egui::Margin::from(vec2(左右, 上下)));
    egui::Panel::left("左栏")
        .exact_size(crate::layout::rail_width(folded))
        .resizable(false)
        .frame(框)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, tokens.space.rail_gap);
            let mut pressed = None;
            if switch_card(ui, facts.library, folded).clicked() {
                pressed = Some(Pressed::SwitchLibrary);
            }
            ui.add_space(tokens.space.rail_switch_margin);
            for (group, views) in GROUPS {
                group_label(ui, group, folded);
                for view in views {
                    if nav_item(ui, *view, facts, folded).clicked() {
                        pressed = Some(Pressed::Go(*view));
                    }
                }
            }
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let 按钮 = ui
                    .add_enabled_ui(!自动收着, |ui| {
                        ghost_button(ui, if folded { UNFOLD } else { FOLD }, folded)
                    })
                    .inner;
                let 按钮 = if folded {
                    按钮.on_hover_text("展开侧栏")
                } else {
                    按钮.on_hover_text("收起侧栏")
                };
                let 按钮 = 按钮.on_disabled_hover_text(format!(
                    "窗口宽不到 {} 时侧栏自动收着",
                    tokens.layout.rail_collapse_below
                ));
                if 按钮.clicked() {
                    pressed = Some(Pressed::Collapse(!folded));
                }
                if !folded {
                    ui.add_space(tokens.space.rail_foot_gap);
                    verdict_note(ui, facts.verdicts);
                }
                ui.add_space(tokens.space.rail_foot_padding);
                let (线, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                ui.painter().hline(
                    线.x_range(),
                    线.center().y,
                    ui.visuals().widgets.noninteractive.bg_stroke,
                );
            });
            pressed
        })
        .inner
}

/// 照这个字号、颜色排一段不折行的字。
fn galley(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    color: Color32,
) -> std::sync::Arc<egui::Galley> {
    egui::WidgetText::from(egui::RichText::new(text).font(font).color(color)).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Body,
    )
}

/// 顶上那张**切换主库**的卡（设计稿 `.libsw`）：面板底、一圈分隔线色描边（悬停时深一档），标志 + 主库原名 +
/// 「切换主库」。收成窄条时只剩标志，居中。
fn switch_card(ui: &mut egui::Ui, library: &str, folded: bool) -> egui::Response {
    let tokens = Tokens::builtin();
    let [上下, 左右] = tokens.space.rail_switch_padding;
    let 标志 = tokens.layout.rail_mark;
    let 宽 = ui.available_width();
    let 字宽 = 宽 - 2.0 * 左右 - 标志 - tokens.space.rail_switch_gap;
    let visuals = ui.visuals().clone();
    let 名 = egui::WidgetText::from(
        egui::RichText::new(library)
            .font(egui::FontId::proportional(tokens.font.size_body))
            .color(visuals.strong_text_color()),
    )
    .into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        字宽.max(0.0),
        egui::TextStyle::Body,
    );
    let 说 = galley(
        ui,
        SWITCH,
        egui::FontId::proportional(tokens.font.size_caption),
        visuals.weak_text_color(),
    );
    let 字高 = 名.size().y + 说.size().y;
    let 高 = 2.0 * 上下 + if folded { 标志 } else { 标志.max(字高) };
    let (rect, response) = ui.allocate_exact_size(vec2(宽, 高), Sense::click());
    let 描边 = if response.hovered() {
        visuals.widgets.inactive.bg_stroke.color
    } else {
        visuals.widgets.noninteractive.bg_stroke.color
    };
    let painter = ui.painter();
    painter.rect(
        rect,
        tokens.radius.medium,
        visuals.window_fill,
        egui::Stroke::new(tokens.layout.control_stroke, 描边),
        egui::StrokeKind::Inside,
    );
    let 标志左 = if folded {
        rect.center().x - 标志 / 2.0
    } else {
        rect.left() + 左右
    };
    let 标志处 = Rect::from_min_size(
        egui::pos2(标志左, rect.center().y - 标志 / 2.0),
        vec2(标志, 标志),
    );
    look::mark(painter, 标志处, tokens.radius.medium, &visuals);
    if !folded {
        let 字左 = 标志处.right() + tokens.space.rail_switch_gap;
        let 字顶 = rect.center().y - 字高 / 2.0;
        let 名高 = 名.size().y;
        painter.galley(egui::pos2(字左, 字顶), 名, visuals.strong_text_color());
        painter.galley(egui::pos2(字左, 字顶 + 名高), 说, visuals.weak_text_color());
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, SWITCH));
    look::focus_ring(ui.ctx(), rect, &response);
    // 设计稿那张卡的 `title` 两态都是「切换主库」；收成窄条时屏上只剩标志，靠它说这一下是什么。
    response.on_hover_text(SWITCH)
}

/// 一组的**分组标题**（设计稿 `.grp`）：角标字号、弱字、带字距。收成窄条时是一道分隔线。
fn group_label(ui: &mut egui::Ui, text: &str, folded: bool) {
    let tokens = Tokens::builtin();
    let 宽 = ui.available_width();
    let 线色 = ui.visuals().widgets.noninteractive.bg_stroke;
    if folded {
        let [上下, 左右] = tokens.space.rail_group_margin_collapsed;
        let (rect, _) = ui.allocate_exact_size(vec2(宽, 2.0 * 上下 + 1.0), Sense::hover());
        ui.painter().hline(
            (rect.left() + 左右)..=(rect.right() - 左右),
            rect.center().y,
            线色,
        );
        return;
    }
    let [上, 左右, 下] = tokens.space.rail_group_padding;
    let 字号 = tokens.font.size_caption;
    let job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat {
            font_id: egui::FontId::proportional(字号),
            color: ui.visuals().weak_text_color(),
            extra_letter_spacing: tokens.font.group_tracking * 字号,
            ..Default::default()
        },
    );
    let 字 = egui::WidgetText::from(job).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Small,
    );
    let (rect, _) = ui.allocate_exact_size(vec2(宽, 上 + 字.size().y + 下), Sense::hover());
    let 色 = ui.visuals().weak_text_color();
    ui.painter()
        .galley(egui::pos2(rect.left() + 左右, rect.top() + 上), 字, 色);
}

/// 一个**入口**（设计稿 `.nav`）：选中时强调色浅底、强调色的字；悬停时凹陷底、强调字；右边是计数。
/// 收成窄条时字与计数上下摞着、居中，字小一号。
fn nav_item(ui: &mut egui::Ui, view: View, facts: &Facts<'_>, folded: bool) -> egui::Response {
    let tokens = Tokens::builtin();
    let 选中 = facts.current == view;
    let 名字 = view.nav_label();
    let badge = (facts.badge)(view);
    let visuals = ui.visuals().clone();
    let 宽 = ui.available_width();
    let 数色 = if 选中 {
        visuals.hyperlink_color
    } else {
        visuals.weak_text_color()
    };
    let 数号 = if folded {
        tokens.font.size_badge_narrow
    } else {
        tokens.font.size_caption
    };
    let (数字, 在跑) = match &badge {
        Badge::Nothing => (None, false),
        Badge::Count(数) => (Some(数.as_str()), false),
        Badge::Live(数) => (Some(数.as_str()), true),
    };
    let 数 = 数字.map(|数| {
        galley(
            ui,
            数,
            egui::FontId::monospace(数号),
            if 在跑 {
                visuals.hyperlink_color
            } else {
                数色
            },
        )
    });
    let 字号 = if folded {
        tokens.font.size_small
    } else {
        tokens.font.size_body
    };
    // 字色要先知道悬没悬停，而悬停要先占了地方才问得出：先排一份量高，占完地方再按悬停换色重排。
    let 量 = galley(
        ui,
        名字,
        egui::FontId::proportional(字号),
        Color32::PLACEHOLDER,
    );
    let 点 = tokens.layout.rail_dot;
    let 点宽 = if 在跑 {
        点 + tokens.space.live_badge_gap
    } else {
        0.0
    };
    let 高 = if folded {
        let 数高 = 数
            .as_ref()
            .map_or(0.0, |数| tokens.space.nav_gap_collapsed + 数.size().y);
        2.0 * tokens.space.nav_padding_collapsed + 量.size().y + 数高
    } else {
        tokens.layout.nav_height
    };
    let (rect, response) = ui.allocate_exact_size(vec2(宽, 高), Sense::click());
    let (底, 字色) = if 选中 {
        (Some(visuals.selection.bg_fill), visuals.hyperlink_color)
    } else if response.hovered() {
        (Some(visuals.extreme_bg_color), visuals.strong_text_color())
    } else {
        (None, visuals.widgets.noninteractive.fg_stroke.color)
    };
    let 字 = galley(ui, 名字, egui::FontId::proportional(字号), 字色);
    let painter = ui.painter();
    if let Some(底) = 底 {
        painter.rect_filled(rect, tokens.radius.medium, 底);
    }
    // 圆点照稿闪：透明度在 1 与 0.25 之间来回（`@keyframes pulse{50%{opacity:.25}}`）。台上有活时窗口本来就每帧重画。
    let 点色 = {
        let 秒 = ui.input(|input| input.time);
        let 相位 = (秒 / PULSE_SECONDS * std::f64::consts::TAU).cos();
        visuals
            .selection
            .stroke
            .color
            .gamma_multiply((0.625 + 0.375 * 相位) as f32)
    };
    if folded {
        let 字顶 = rect.top() + tokens.space.nav_padding_collapsed;
        let 字高 = 字.size().y;
        painter.galley(
            egui::pos2(rect.center().x - 字.size().x / 2.0, 字顶),
            字,
            字色,
        );
        if let Some(数) = 数 {
            let 顶 = 字顶 + 字高 + tokens.space.nav_gap_collapsed;
            let 左 = rect.center().x - (点宽 + 数.size().x) / 2.0;
            if 在跑 {
                painter.circle_filled(
                    egui::pos2(左 + 点 / 2.0, 顶 + 数.size().y / 2.0),
                    点 / 2.0,
                    点色,
                );
            }
            painter.galley(egui::pos2(左 + 点宽, 顶), 数, 数色);
        }
    } else {
        let 左右 = tokens.space.nav_padding;
        painter.galley(
            egui::pos2(rect.left() + 左右, rect.center().y - 字.size().y / 2.0),
            字,
            字色,
        );
        if let Some(数) = 数 {
            let 左 = rect.right() - 左右 - 数.size().x;
            let 顶 = rect.center().y - 数.size().y / 2.0;
            if 在跑 {
                painter.circle_filled(
                    egui::pos2(左 - tokens.space.live_badge_gap - 点 / 2.0, rect.center().y),
                    点 / 2.0,
                    点色,
                );
            }
            painter.galley(egui::pos2(左, 顶), 数, 数色);
        }
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, 选中, 名字)
    });
    look::focus_ring(ui.ctx(), rect, &response);
    response
}

/// 栏底那颗**幽灵小按钮**（设计稿 `.btn.ghost.sm`）：平时没底没框，悬停时凹陷底、强调字；占满这一栏宽，
/// 字靠左（`centered` 时居中）。所在那块 `ui` 被禁用时整颗变淡、按不下去。
fn ghost_button(ui: &mut egui::Ui, text: &str, centered: bool) -> egui::Response {
    let tokens = Tokens::builtin();
    let visuals = ui.visuals().clone();
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), tokens.layout.button_small_height),
        Sense::click(),
    );
    let 悬停 = response.hovered();
    let 字色 = if 悬停 {
        visuals.strong_text_color()
    } else {
        visuals.widgets.noninteractive.fg_stroke.color
    };
    let 字 = galley(
        ui,
        text,
        egui::FontId::proportional(tokens.font.size_small),
        字色,
    );
    let painter = ui.painter();
    if 悬停 {
        painter.rect_filled(rect, tokens.radius.medium, visuals.extreme_bg_color);
    }
    let 左 = if centered {
        rect.center().x - 字.size().x / 2.0
    } else {
        rect.left() + tokens.layout.button_small_padding
    };
    painter.galley(
        egui::pos2(左, rect.center().y - 字.size().y / 2.0),
        字,
        字色,
    );
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), text));
    look::focus_ring(ui.ctx(), rect, &response);
    response
}

/// 栏底那一行「**已保存 N 条裁决**」（设计稿 `.railfoot .st`）：一枚高置信色的圆点，半号的弱字。
fn verdict_note(ui: &mut egui::Ui, verdicts: Option<u64>) {
    let tokens = Tokens::builtin();
    let 数 = verdicts.map_or_else(|| "—".to_owned(), thousands);
    let 字 = galley(
        ui,
        &format!("已保存 {数} 条裁决"),
        egui::FontId::proportional(tokens.font.size_caption_plus),
        ui.visuals().weak_text_color(),
    );
    let 点 = tokens.layout.rail_dot;
    let (rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), 字.size().y.max(点)),
        Sense::hover(),
    );
    let 左 = rect.left() + tokens.space.rail_note_padding;
    let (点色, _) = look::tone_colors(look::Tone::Good, ui.visuals());
    let painter = ui.painter();
    painter.circle_filled(egui::pos2(左 + 点 / 2.0, rect.center().y), 点 / 2.0, 点色);
    let 字色 = ui.visuals().weak_text_color();
    painter.galley(
        egui::pos2(
            左 + 点 + tokens.space.rail_note_gap,
            rect.center().y - 字.size().y / 2.0,
        ),
        字,
        字色,
    );
}
