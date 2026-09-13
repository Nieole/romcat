//! 界面上不许出现豆腐块。
//!
//! 问的是 egui 自己的字形查找，而不是直接读字体的 `cmap`——真正决定屏幕上是不是豆腐块的
//! 是整条回退链。

use std::collections::BTreeMap;

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

/// 拿 `family` 那条回退链上「字 → 带着它的那几份字体（按链上的次序）」的表问一件事。
///
/// 这是 egui 自己按字族建出来的表，与它画字时走的是同一条回退链。
fn with_characters<R>(
    ctx: &egui::Context,
    family: &egui::FontFamily,
    ask: impl FnOnce(&BTreeMap<char, Vec<String>>) -> R,
) -> R {
    ctx.fonts_mut(|fonts| ask(fonts.fonts.font(family).characters()))
}

/// 在 `family` 里画 `ch` 的是哪一份字体：回退链上头一个带着它的那份，一份都没有就是 `None`。
fn owner(ctx: &egui::Context, family: &egui::FontFamily, ch: char) -> Option<String> {
    with_characters(ctx, family, |characters| {
        characters.get(&ch).and_then(|names| names.first().cloned())
    })
}

/// 拉丁粗体与等宽两份子集收的那张表：ASCII 可见字符，加 U+00C0–017F 里的字母（跳过 `×` `÷`）。
///
/// 与 `tools/font-subset.py` 里的 `LATIN_RANGES` 是同一张表，两边都得过。
fn 拉丁那张表里有(ch: char) -> bool {
    matches!(ch, ' '..='~' | 'À'..='ſ') && !matches!(ch, '×' | '÷')
}

/// `face` 这份字体在 `family` 里带着的字当中，不在拉丁那张表里的那些。
fn outside_latin(ctx: &egui::Context, family: &egui::FontFamily, face: &str) -> String {
    with_characters(ctx, family, |characters| {
        characters
            .iter()
            .filter(|(ch, names)| !拉丁那张表里有(**ch) && names.iter().any(|name| name == face))
            .map(|(ch, _)| *ch)
            .collect()
    })
}

/// 裁定（2026-09-13）：不打包中文粗体。粗体族里那份粗体**只有拉丁与数字**。
#[test]
fn 粗体族里只有拉丁与数字() {
    let ctx = ready(true);
    let strong = font::strong_family();
    for ch in ('A'..='Z')
        .chain('a'..='z')
        .chain('0'..='9')
        .chain("éō".chars())
    {
        assert_eq!(
            owner(&ctx, &strong, ch).as_deref(),
            Some(font::STRONG_FACE),
            "粗体族里的 {ch} 不是粗体画的",
        );
    }
    let foreign = outside_latin(&ctx, &strong, font::STRONG_FACE);
    assert!(
        foreign.is_empty(),
        "粗体子集里混进了拉丁那张表之外的字：{foreign}"
    );
}

/// 粗体族里拉丁那张表之外的字**回退到常规体**——不是豆腐块，也不是别的哪份字体。
///
/// 除了中文、假名与符号，还专门带上中文标题里当分隔号的 `·` 与 `× ° ©` 这类拉丁补充里的
/// 符号：`根 · 3 个` 换进粗体族之后，那个点得是常规字重，一句中文中间才不夹一个半粗的点。
#[test]
fn 粗体族里的中文回退常规体() {
    let ctx = ready(true);
    let strong = font::strong_family();
    let texts = [font::REQUIRED, "·×÷°©¥«»—…“”"]
        .into_iter()
        .chain(font::SAMPLE.iter().map(|(_, text)| *text));
    for ch in texts.flat_map(str::chars).filter(|ch| !拉丁那张表里有(*ch)) {
        assert_eq!(
            owner(&ctx, &strong, ch).as_deref(),
            Some(font::REGULAR_FACE),
            "粗体族里的 {ch} 没有回退到常规体",
        );
    }
}

/// 屏上没有合成出来的假粗体：粗体族里的中文与常规体里的中文是**同一个字形**——
/// 取的是图集里同一块纹理、步进一样宽，一个像素都不差。
///
/// 后一半反过来断拉丁**不一样**：这条比较若分不出粗体与常规体，前一半就是白证。
#[test]
fn 屏上没有合成出来的假粗体() {
    let ctx = ready(true);
    let glyphs = |family: egui::FontFamily, text: &str| {
        ctx.fonts_mut(|fonts| {
            let galley = fonts.layout_no_wrap(
                text.to_owned(),
                egui::FontId::new(14.0, family),
                egui::Color32::WHITE,
            );
            galley
                .rows
                .iter()
                .flat_map(|row| row.glyphs.iter())
                .map(|glyph| (glyph.chr, glyph.uv_rect, glyph.advance_width))
                .collect::<Vec<_>>()
        })
    };

    // 不放空格：空格归粗体子集管，它一进来这一串就不全是「回退到常规体」的字了。
    let chinese = "简体中文·繁體龍あカ，。《》★♪Ⅲ①°";
    assert_eq!(
        glyphs(font::strong_family(), chinese),
        glyphs(egui::FontFamily::Proportional, chinese),
        "粗体族里的中文与常规体画得不一样——有东西在给中文加粗",
    );

    let latin = "FinalFantasy1997";
    assert_ne!(
        glyphs(font::strong_family(), latin),
        glyphs(egui::FontFamily::Proportional, latin),
        "粗体族里的拉丁与常规体画得一模一样——粗体没装上，这条比较也分不出两份字体",
    );
}

/// 等宽族：路径、哈希、序列号、数量与容量里的字母与数字由等宽子集画，**一列数字对得齐**；
/// 等宽族里同样不放中文，中文回退常规体。
#[test]
fn 等宽族里数字同宽中文回退常规体() {
    let ctx = ready(true);
    let mono = egui::FontFamily::Monospace;
    for ch in ('A'..='Z')
        .chain('a'..='z')
        .chain('0'..='9')
        .chain("-_./\\:éō".chars())
    {
        assert_eq!(
            owner(&ctx, &mono, ch).as_deref(),
            Some(font::MONO_FACE),
            "等宽族里的 {ch} 不是等宽子集画的",
        );
    }
    let foreign = outside_latin(&ctx, &mono, font::MONO_FACE);
    assert!(
        foreign.is_empty(),
        "等宽子集里混进了拉丁那张表之外的字：{foreign}"
    );

    // 数字之外再带上最宽的 `W` 与最窄的 `i`：它们也一样宽，才是真的等宽。
    let widths: Vec<f32> = ctx.fonts_mut(|fonts| {
        "0123456789Wi"
            .chars()
            .map(|ch| fonts.glyph_width(&egui::FontId::monospace(14.0), ch))
            .collect()
    });
    assert!(
        widths.windows(2).all(|pair| pair[0] == pair[1]),
        "等宽族里数字不同宽：{widths:?}",
    );

    for ch in "简繁龍あカ，。《》".chars() {
        assert_eq!(
            owner(&ctx, &mono, ch).as_deref(),
            Some(font::REGULAR_FACE),
            "等宽族里的 {ch} 没有回退到常规体",
        );
    }
}

/// 常规、粗体、等宽三个字族，**哪一个里都不许有豆腐块**：必备字符与样张整张都画得出来。
///
/// 屏上用到哪个族，哪个族就得过——粗体族或等宽族的回退链漏接了常规体，汉字在标题里
/// 或路径里就成了豆腐块，而只问常规族的检查看不见。
///
/// 问的是「回退链上有没有哪份字体带着这个字」，不是 egui 的 `has_glyph`——后者有误报，
/// 见下一条。
#[test]
fn 三个字族里都没有豆腐块() {
    let ctx = ready(true);
    let texts = std::iter::once(font::REQUIRED).chain(font::SAMPLE.iter().map(|(_, text)| *text));
    let families = font::families();
    assert_eq!(
        families.len(),
        3,
        "界面用的是常规、粗体、等宽三个族：{families:?}"
    );
    for family in families {
        for text in texts.clone() {
            let missing: String = text
                .chars()
                .filter(|ch| owner(&ctx, &family, *ch).is_none())
                .collect();
            assert!(missing.is_empty(), "{family} 族里画不出来：{missing}");
        }
    }
}

/// egui 的 `has_glyph` 把「落到替换字那份字体上的字」一律报成画不出来，哪怕那份字体
/// 真带着它。等宽族里头一份带替换字 `◻` 的是 egui 内置的 Hack，而等宽子集里没有的
/// `… — → ∀ ♪` 正是 Hack 画的——屏上是真字形，字体检查不许把它们报成豆腐块。
///
/// 常规族里是同一个毛病换了一份字体：头一份带 `◻` 的是 NotoEmoji，`🎮` 只有它带着。
#[test]
fn 替换字那份字体画的字不算豆腐块() {
    let ctx = ready(true);
    assert_eq!(
        owner(&ctx, &egui::FontFamily::Monospace, '♪').as_deref(),
        Some("Hack"),
        "前提变了：等宽族里的 ♪ 不再由 Hack 画，这条测试要照新的回退链重写",
    );
    assert_eq!(
        owner(&ctx, &egui::FontFamily::Proportional, '🎮').as_deref(),
        Some("NotoEmoji-Regular"),
        "前提变了：常规族里的 🎮 不再由 NotoEmoji 画，这条测试要照新的回退链重写",
    );
    let missing = font::missing(&ctx, "…—→∀♪🎮");
    assert!(
        missing.is_empty(),
        "替换字那份字体画得出来的字被报成了豆腐块：{}",
        missing.into_iter().collect::<String>()
    );
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
/// 没有这一条，上面那几条测试就可能是在验一件本来就成立的事——那样的话哪天字体装错了、
/// 装漏了，测试照样绿。ADR-0005 说「egui 内置字体不只缺汉字」，这里把那句话钉成断言：
/// 汉字、假名、中文标点、罗马数字、带圈数字、`→ ※ Ⓡ` 一个都画不出来。
///
/// **`★ ☆ ♪ ♥` 不在这张表里**——实测内置的 emoji 字体带着它们。装子集字体仍然把它们
/// 一并接过来，图的是风格统一（同一套字形，而不是正文一套、符号一套）。`♥` 从前在表里，
/// 是 egui `has_glyph` 的误报让它看着像豆腐块（见 `替换字那份字体画的字不算豆腐块`）。
#[test]
fn 内置字体确实缺这些字() {
    let ctx = ready(false);
    for expected in [
        "简", "繁", "龍", "あ", "カ", "，", "。", "《", "・", "Ⅲ", "①", "②", "→", "※", "Ⓡ",
    ] {
        let missing = font::missing(&ctx, expected);
        assert_eq!(
            missing,
            expected.chars().collect::<Vec<_>>(),
            "内置字体居然画得出 {expected}——那这几条测试就白测了",
        );
    }
}

/// 使用入口：`font::mono` 画出来的字落在等宽族，`font::strong` 画出来的字落在粗体族。
///
/// 看的是这一帧**真画出来**的那一段字带着哪个字族，而不是入口里写了什么。
#[test]
fn 入口画出来的字落在对应的字族() {
    fn collect(shape: &egui::epaint::Shape, out: &mut Vec<(String, egui::FontFamily)>) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                for section in &text.galley.job.sections {
                    out.push((
                        text.galley.text().to_owned(),
                        section.format.font_id.family.clone(),
                    ));
                }
            }
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    collect(one, out);
                }
            }
            _ => {}
        }
    }

    let ctx = headless::context();
    // 字体从装上之后的下一帧起才生效。
    headless::frame(&ctx, headless::input(), |_| {});
    let output = headless::frame(&ctx, headless::input(), |ui| {
        ui.label(font::mono("SLPS-02170 1,024 MB"));
        ui.label(font::strong("Final Fantasy VII 幻想"));
    });
    let mut painted = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut painted);
    }

    for (text, family) in [
        ("SLPS-02170 1,024 MB", egui::FontFamily::Monospace),
        ("Final Fantasy VII 幻想", font::strong_family()),
    ] {
        assert!(
            painted.contains(&(text.to_owned(), family.clone())),
            "「{text}」没有画在 {family} 族里；这一帧画出来的是：{painted:?}",
        );
    }
}

/// 许可跟着二进制走：OFL 要求分发字体时随附许可，而这几份字体是嵌进可执行文件的。
/// 两家的版权行不同，许可副本就得各带一份——只带 Noto 那一份的话，等宽体是不合规地分发的。
#[test]
fn 许可跟着二进制走() {
    let all: String = font::LICENSES.iter().map(|(_, text)| *text).collect();
    for holder in [
        "Copyright 2014-2021 Adobe",
        "Copyright 2020 The JetBrains Mono Project Authors",
    ] {
        assert!(
            all.contains(holder),
            "打包的字体许可里没有「{holder}」那一家"
        );
    }
    for (family, text) in font::LICENSES {
        assert!(
            text.contains("SIL Open Font License, Version 1.1"),
            "{family} 那一份不是 OFL 1.1 的全文",
        );
    }
}

/// 拉丁粗体与等宽两份子集是这一档字体预算的全部增量。规格说的量级是「几百 KB」：
/// 钉一个上界，免得哪天哪一份混进中文、涨回兆字节；下界钉住它不是一份空文件。
#[test]
fn 拉丁两份子集在几百千字节以内() {
    let bytes = font::latin_bytes();
    assert!(
        (10_000..500_000).contains(&bytes),
        "拉丁粗体与等宽两份子集共 {bytes} 字节，出了「几百 KB」那一档",
    );
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
