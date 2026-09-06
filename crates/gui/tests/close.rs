//! 退出前先关输入法。
//!
//! macOS 上组字过程中直接关窗会 abort（winit#4626），规避办法是**窗口还活着的时候**先
//! `set_ime_allowed(false)`。这条通路自动化验得了——验的不是输入法本身（那是票 23，
//! 只能真机人工做），而是**命令发出去的次序**：`IMEAllowed(false)` 必须先于 `Close`。

use egui::{RawInput, ViewportCommand, ViewportEvent, ViewportId, ViewportInfo};
use romcat_gui::app::{App, Closing};
use romcat_gui::{demo, headless};

/// 这几条测试自己的**工作目录**。
///
/// **不共用 `demo::workspace()`**：那是 `--demo` 那个演示窗口用的目录，而窗口会往里写
/// **版式偏好**（面板拖到哪儿）。共用的话，维护者开一次演示窗口把某块面板拖高一截，
/// 下一次跑这几条测试屏上就少了几行——而断言数的正好是行数。
fn 工作目录() -> std::path::PathBuf {
    std::env::temp_dir().join("romcat-测试-关窗")
}

/// 一帧的输入：`closing` 为真时带上「窗口被要求关闭」这个事件。
fn 输入(closing: bool) -> RawInput {
    let mut input = headless::input();
    let mut viewport = ViewportInfo::default();
    if closing {
        viewport.events.push(ViewportEvent::Close);
    }
    input.viewports.insert(ViewportId::ROOT, viewport);
    input
}

/// 跑一帧，返回这一帧对根视口发出的全部命令。
fn 跑一帧(ctx: &egui::Context, app: &mut App, closing: bool) -> Vec<ViewportCommand> {
    headless::frame(ctx, 输入(closing), |ui| app.ui(ui))
        .viewport_output
        .get(&ViewportId::ROOT)
        .map(|viewport| viewport.commands.clone())
        .unwrap_or_default()
}

#[test]
fn 关窗分两拍先关输入法再关窗() {
    let ctx = headless::context();
    let mut app = App::new(
        demo::site(demo::synthetic(200).expect("造得出合成数据")).expect("开得出现场"),
        工作目录(),
    );

    // 第 0 帧：没人要关，什么都不该发。
    let 平常 = 跑一帧(&ctx, &mut app, false);
    assert_eq!(app.closing(), Closing::No);
    assert!(
        !平常.contains(&ViewportCommand::Close),
        "没人要关窗却发了关闭命令",
    );

    // 第 1 帧：关窗请求来了。撤销掉，把输入法关了，这一帧**不**关窗。
    let 第一拍 = 跑一帧(&ctx, &mut app, true);
    assert_eq!(app.closing(), Closing::Deferred);
    assert!(
        第一拍.contains(&ViewportCommand::CancelClose),
        "第一拍没撤销关闭，窗口会在输入法还开着的时候被销毁：{第一拍:?}",
    );
    assert!(
        第一拍.contains(&ViewportCommand::IMEAllowed(false)),
        "第一拍没关输入法：{第一拍:?}",
    );
    assert!(
        !第一拍.contains(&ViewportCommand::Close),
        "第一拍就把窗关了，那这两拍等于没做：{第一拍:?}",
    );

    // 第 2 帧：撤销生效了（不再有关窗事件），这一帧才真的关。
    let 第二拍 = 跑一帧(&ctx, &mut app, false);
    assert_eq!(app.closing(), Closing::Sent);
    assert!(
        第二拍.contains(&ViewportCommand::Close),
        "第二拍没把窗关掉，用户点了叉却关不上：{第二拍:?}",
    );
}
