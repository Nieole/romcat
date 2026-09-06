//! 几份界面测试**共用**的那点东西。
//!
//! 它摆在**子目录**里：`tests/` 底下每个 `.rs` 都被当成一个独立的测试二进制，
//! 共用的东西直接放那儿会变成一个一条测试都没有的二进制。

/// 这一帧**真的画在屏上**的那些字，**一段一行**。
///
/// 「屏上摆得出来」「屏上写的是同一个词」「屏上没了」这三类断言只有看这个才算数：
/// 查数据结构里有没有这条是在测别的东西（库里写没写、删没删干净，另有断言管），
/// 而那几屏要证的正是它**画出来了**。egui 每画一段文字就留下一个 `Galley`，它带着原文。
///
/// 每段后面跟一个换行，于是 `lines()` 问得出「屏上那一行写的是什么」——浏览屏那条
/// 「一条顶到闸上的简介收成一行画得下的那一截」要的正是这个粒度。**一段自己带的换行
/// 照样劈成两行**：值里若留着换行，屏上那一行就断在换行处——那条测试断「折平了」
/// 靠的正是这一点。
pub fn 画出来的字(output: &egui::FullOutput) -> String {
    fn 收(shape: &egui::epaint::Shape, out: &mut String) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                out.push_str(text.galley.text());
                out.push('\n');
            }
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    收(one, out);
                }
            }
            _ => {}
        }
    }
    let mut out = String::new();
    for clipped in &output.shapes {
        收(&clipped.shape, &mut out);
    }
    out
}
