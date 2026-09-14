//! **提示条**（设计稿 `.toast`）：窗口底边居中浮着的一条深色小条——一句话，可带一颗按钮，停一会儿自己收起。
//!
//! 这一层只画、只计时，**不记事**：那句话说的是哪件事、按钮按下去做什么、收起来之后要丢掉什么，都归叫它的
//! 那一屏。于是换走一屏时由那一屏自己决定丢不丢（子库屏删掉一个子库之后那颗「撤销」，换屏就丢）。
//!
//! 颜色与尺寸都取令牌：底是 `ink`、字是 `win`（正文色与窗口底色对调，设计稿 `.toast` 的
//! `background:var(--ink);color:var(--win)`），字号 `size-small-plus`，圆角 `large`，阴影与弹出菜单同一份
//! （`popup_shadow`，令牌 `[shadow.pop]`）；离窗口底边多远、四边留白、按钮描边多淡、停多久取 `[layout]` 里
//! `toast-` 开头那几格。
//!
//! **停多久按 egui 那一帧的时刻算**（`InputState::time`），不看挂钟：测试里递一个时刻进去，就走得到
//! 「停够了」那一支。

use std::time::Duration;

use crate::look::{self, step};
use crate::tokens::{Palette, Tokens};

/// 一条提示条。
#[derive(Debug, Clone)]
pub struct Toast {
    /// 那句话。
    text: String,
    /// 带的那颗按钮上写什么。
    action: Option<String>,
    /// 头一回画出来是哪一刻（egui 的时刻，秒）。还没画过是 `None`。
    since: Option<f64>,
}

/// 这一帧提示条怎么了。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    /// 照旧摆着。
    Showing,
    /// 那颗按钮按下去了。叫它的那一屏办完那件事、把它扔掉就是。
    Pressed,
    /// 停够了，这一帧起不再画。
    Expired,
}

impl Toast {
    /// 一条只有一句话的提示条。
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            action: None,
            since: None,
        }
    }

    /// 带一颗按钮。带按钮的停得久一些（令牌 `toast-action-seconds`）：人得来得及读完再去按。
    #[must_use]
    pub fn action(mut self, label: impl Into<String>) -> Self {
        self.action = Some(label.into());
        self
    }

    /// 那句话。
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// 画这一帧：盖在整个窗口最上面、底边居中。停够了就不画，交回 [`Shown::Expired`]。
    pub fn show(&mut self, ctx: &egui::Context) -> Shown {
        let tokens = Tokens::builtin();
        let layout = &tokens.layout;
        let now = ctx.input(|input| input.time);
        let since = *self.since.get_or_insert(now);
        let stay = f64::from(if self.action.is_some() {
            layout.toast_action_seconds
        } else {
            layout.toast_seconds
        });
        let left = stay - (now - since);
        if left <= 0.0 {
            return Shown::Expired;
        }
        // 没有输入事件时窗口不重画——到点那一刻得有一帧来把它收起。
        ctx.request_repaint_after(Duration::from_secs_f64(left));
        // 这一帧用的是哪一套主题的样式（与 `look` 里取焦点色那一处同一个写法）。
        let style = ctx.style_of(ctx.theme());
        let palette = tokens
            .color
            .theme(egui::Theme::from_dark_mode(style.visuals.dark_mode));
        let [上, 右, 下, 左] = layout.toast_padding;
        let mut pressed = false;
        egui::Area::new(egui::Id::new("提示条"))
            .order(egui::Order::Foreground)
            .anchor(
                egui::Align2::CENTER_BOTTOM,
                egui::vec2(0.0, -layout.toast_bottom),
            )
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(palette.ink)
                    .corner_radius(tokens.radius.large)
                    .shadow(style.visuals.popup_shadow)
                    .inner_margin(egui::Margin {
                        left: 左,
                        right: 右,
                        top: 上,
                        bottom: 下,
                    })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // 字与按钮之间照稿（`.toast` 的 `gap:12px`）。
                            ui.spacing_mut().item_spacing.x = step(3);
                            ui.label(
                                egui::RichText::new(&self.text)
                                    .size(tokens.font.size_small_plus)
                                    .color(palette.win),
                            );
                            if let Some(label) = &self.action {
                                pressed = look::small_buttons(ui, |ui| {
                                    ui.scope(|ui| {
                                        button_colors(
                                            palette,
                                            layout.toast_button_line,
                                            ui.visuals_mut(),
                                        );
                                        ui.button(label.as_str())
                                    })
                                    .inner
                                })
                                .clicked();
                            }
                        });
                    });
            });
        if pressed {
            Shown::Pressed
        } else {
            Shown::Showing
        }
    }
}

/// 提示条上那颗按钮（设计稿 `.toast .btn`）：透明底、字与提示条的字同色，描边是字色的几成（`line`）；
/// 悬停与拿到焦点那两档描边换成整份字色，看得出按得着、焦点在哪儿。
fn button_colors(palette: &Palette, line: f32, visuals: &mut egui::Visuals) {
    let widgets = &mut visuals.widgets;
    for (widget, stroke) in [
        (&mut widgets.inactive, palette.win.gamma_multiply(line)),
        (&mut widgets.hovered, palette.win),
        (&mut widgets.active, palette.win),
    ] {
        widget.bg_fill = egui::Color32::TRANSPARENT;
        widget.weak_bg_fill = egui::Color32::TRANSPARENT;
        widget.bg_stroke.color = stroke;
        widget.fg_stroke.color = palette.win;
    }
}
