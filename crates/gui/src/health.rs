//! 库屏底下的**库体检**那一块（票 `gui-looks-like-the-design/27`）：对主库跑一遍只读扫描得到的报告，
//! 摆成一块概要，每一格点进去看明细。
//!
//! ## 只报告、不处理
//!
//! 体检是只读检查（词表**库体检**）：重复拷贝只发现不删除，主库一个字节都不写（ADR-0004）。这一块上
//! 没有任何改动主库的入口，要清理请人自己去文件系统里做。
//!
//! ## 数不在这里算
//!
//! 每一格的数、明细与判据都在核心库的体检报告里（`romcat_core::report::HealthReport`），这一层只画
//! （ADR-0005、ADR-0024）。
//!
//! 标题栏（「库体检」、说明、「重新体检」、折叠标）与面板的样子由库屏摆（`roots::Screen`，与根、数据源、导出设置
//! 三块同一个画法），这里画正文。

use crate::tokens::Tokens;

/// 标题栏里那句说明（设计稿 `#health-sub` 原话）。
pub const READ_ONLY: &str = "只读检查，只报告、不处理";

/// 标题栏右边那颗按钮上的字（设计稿 `data-act="task:health"`）。
pub const RECHECK: &str = "重新体检";

/// 还没扫描时这一块画的那一句（设计稿 `renderHealth` 空态原话）。
pub const BEFORE_SCAN: &str = "扫描完成后生成体检报告。";

/// 画正文。`scanned`：这个库有没有一个根完整扫过一趟（`LibraryRoot::fully_scanned`，判据由库屏交进来）。
pub(crate) fn body_ui(ui: &mut egui::Ui, scanned: bool) {
    if !scanned {
        empty_ui(ui);
    }
}

/// 还没扫描时那一句（设计稿 `.empty`）：居中的弱字，四边留令牌 `health-empty-padding`。
fn empty_ui(ui: &mut egui::Ui) {
    let 留白 = Tokens::builtin().space.health_empty_padding;
    egui::Frame::new()
        .inner_margin(egui::Margin::from(egui::vec2(留白, 留白)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| ui.label(egui::RichText::new(BEFORE_SCAN).weak()));
        });
}
