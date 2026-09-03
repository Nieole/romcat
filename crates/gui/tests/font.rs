//! 界面上不许出现豆腐块。
//!
//! 问的是 egui 自己的字形查找，而不是直接读字体的 `cmap`——真正决定屏幕上是不是豆腐块的
//! 是整条回退链。

use romcat_gui::{font, headless};

/// 装完字体跑一帧，返回一个问得动字体的 [`egui::Context`]。
///
/// `install` 为假时**不装**子集字体——那是反证那条用的，见文末。
fn ready(install: bool) -> egui::Context {
    let ctx = if install {
        headless::context()
    } else {
        egui::Context::default()
    };
    // 跑一帧字体才建得出来。
    headless::frame(&ctx, egui::RawInput::default(), |_| {});
    ctx
}

#[test]
fn 必备字符一个豆腐块都没有() {
    let ctx = ready(true);
    let missing = font::missing(&ctx, font::REQUIRED);
    assert!(
        missing.is_empty(),
        "画不出来：{}",
        missing.into_iter().collect::<String>()
    );
}

#[test]
fn 字体样张整张都画得出来() {
    let ctx = ready(true);
    for (what, text) in font::SAMPLE.iter().copied() {
        let missing = font::missing(&ctx, text);
        assert!(
            missing.is_empty(),
            "样张「{what}」里画不出来：{}",
            missing.into_iter().collect::<String>()
        );
    }
}

/// 反证：不装子集字体的话，这些字**确实**是豆腐块。
///
/// 没有这一条，上面三条测试就可能是在验一件本来就成立的事——那样的话哪天字体装错了、
/// 装漏了，测试照样绿。ADR-0005 说「egui 内置字体不只缺汉字」，这里把那句话钉成断言：
/// 汉字、假名、中文标点、罗马数字、带圈数字、`♥ → ※ Ⓡ` 一个都画不出来。
///
/// **`★ ☆ ♪` 不在这张表里**——实测内置的 emoji 图标字体带着它们。装子集字体仍然把它们
/// 一并接过来，图的是风格统一（同一套字形，而不是正文一套、符号一套）。
#[test]
fn 内置字体确实缺这些字() {
    let ctx = ready(false);
    for expected in [
        "简", "繁", "龍", "あ", "カ", "，", "。", "《", "・", "Ⅲ", "①", "②", "♥", "→", "※", "Ⓡ",
    ] {
        let missing = font::missing(&ctx, expected);
        assert_eq!(
            missing,
            expected.chars().collect::<Vec<_>>(),
            "内置字体居然画得出 {expected}——那这几条测试就白测了",
        );
    }
}

/// 子集的体积是**打包字体**这条路的全部代价，钉一个上界免得哪天它悄悄涨回完整字体。
#[test]
fn 子集体积在预算内() {
    let bytes = font::subset_bytes();
    assert!(
        (7_000_000..9_000_000).contains(&bytes),
        "字体子集 {bytes} 字节，落在 GBK+Big5 那一档之外了（完整字体 16.9 MB）",
    );
}
