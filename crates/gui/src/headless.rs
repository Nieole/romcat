//! 不开窗跑 egui。
//!
//! `egui::Context` 不需要窗口也跑得完一帧的布局与三角化——那正是这张票的帧率与豆腐块
//! 两条验收能进门禁的原因：不开窗就不需要显示器。
//!
//! **每跑一帧都得认领纹理增量。** `epaint` 在丢弃没人处理的 `TexturesDelta` 时会 panic，
//! 那本是给「忘了把字体图集传给显卡」准备的保险；没有显卡的时候得自己认领。
//! 这件事在量帧率、查豆腐块、两个测试里各写一遍的话，迟早漏一处——收在 [`frame`] 里。

/// 视口大小，点。定死一个常见的窗口尺寸，免得量出来的数字随屏幕变。
pub const VIEWPORT: [f32; 2] = [1280.0, 800.0];

/// 一个装好字体、可以直接跑帧的 [`egui::Context`]。
#[must_use]
pub fn context() -> egui::Context {
    let ctx = egui::Context::default();
    crate::font::install(&ctx);
    ctx
}

/// 一帧的输入：视口 [`VIEWPORT`] 那么大，没有任何事件。
#[must_use]
pub fn input() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, VIEWPORT.into())),
        ..Default::default()
    }
}

/// 跑一帧，返回这一帧的产出（纹理增量已认领）。
pub fn frame(
    ctx: &egui::Context,
    input: egui::RawInput,
    run: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut output = ctx.run_ui(input, run);
    output.textures_delta.clear();
    output
}
