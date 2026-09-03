//! 打包进二进制的字体子集。
//!
//! ## 为什么不读系统字体
//!
//! egui 内置的 Ubuntu-Light + NotoEmoji **一个汉字都没有**，而且缺的**不只是汉字**：
//! 中文标点 `，。《》`、假名 `あカ`、罗马数字 `Ⅲ`、带圈数字 `①` 全是豆腐块。补上它
//! 只有两条路，读系统字体或者打包一份，这里选后者，理由是三条实测（ADR-0005 的修订段、
//! `docs/research/egui-viability.md`）：
//!
//! 1. **路径不可硬编码**。macOS 的苹方躺在哈希命名的目录下，各机器不同。
//! 2. **覆盖不一致**。实测苹方缺 `♪`、冬青黑缺 `♪` 与 `Ⓡ`——而这些符号在游戏标题里
//!    真实存在。走系统字体等于「哪些字显示得出来」在每台机器上都是另一个答案，
//!    还没法在自己机器上复现。
//! 3. **更贵**。`FontData` 要求整份字体进内存，没有按需通路；系统中文字体动辄
//!    20–75 MB，比这份 7.3 MB 的子集更占地方。
//!
//! ## 子集怎么来的
//!
//! `tools/font-subset.py` 从完整的 `NotoSansSC[wght].ttf`
//! （17,772,300 字节）裁出来：字符集取 **GBK + Big5 + 中日韩标点 + 假名 + U+2100–27BF
//! 符号区 + 拉丁扩展**，字重固定在 400。产物 **7,670,804 字节、22,534 个码位**。
//! 改字符集就重跑那个脚本，别手工动这份二进制。
//!
//! 许可是 SIL Open Font License 1.1，副本在 `assets/OFL.txt`。

use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use egui::{FontData, FontFamily, FontId};

/// 子集字体本体。
///
/// `include_bytes!` 而不是运行时读文件：界面在用户机器上是一个可以随便拷的单文件，
/// 少一个「字体找不到」的失败模式。
const SUBSET: &[u8] = include_bytes!("../assets/NotoSansSC-Subset.ttf");

/// 这份字体在 egui 里的名字。
pub const FAMILY: &str = "NotoSansSC-Subset";

/// 界面上必须显示得出来的字符，**一个豆腐块都不许有**。
///
/// 这不是随手挑的：前四组是 egui 内置字体**确实缺**的四类（汉字、假名、中文标点、
/// 符号），后一组是游戏标题里真实出现的记号。裁字体时若把某个区段漏掉，
/// [`missing`] 会当场指出来是哪一个字。
///
/// 与 `tools/font-subset.py` 里的 `MUST_COVER` 是同一份，两边都得过。
pub const REQUIRED: &str = "简繁龍鬱囧あカ，。、《》～〜・★☆♪♥Ⅲ①￥Ⓡ→∀ōé";

/// 界面上那份**字体样张**：一类字一行，摆出来给人看。
///
/// 它同时是自动化的靶子（`tests/font.rs` 逐字问 egui 画不画得出来）与肉眼的证据
/// （界面上「字体样张」那个开关）。两处用同一份，免得屏幕上摆的和测试查的漂开。
pub const SAMPLE: &[(&str, &str)] = &[
    ("汉字（简）", "简体中文 幻想传说 圣剑传说 轩辕剑"),
    ("汉字（繁）", "繁體中文 潛龍諜影 仙劍奇俠傳 太空戰士"),
    ("生僻", "龍 鬱 囧 燚 淼 犇 びゃ"),
    ("假名", "ゼルダの伝説 モンスターハンター ヴァ"),
    ("中文标点", "，。、；：？！《》「」（）〜・…—※"),
    ("罗马数字", "Ⅰ Ⅱ Ⅲ Ⅳ Ⅴ Ⅵ Ⅶ Ⅷ Ⅸ Ⅹ"),
    ("带圈数字", "① ② ③ ④ ⑤ ⑥ ⑦ ⑧ ⑨ ⑩ Ⓡ"),
    ("符号", "★ ☆ ♪ ♥ ♂ ♀ → ∀ ￥ € ® ° α Ω"),
    ("拉丁扩展", "Pokémon Ōkami Führer Añejo"),
];

/// 游戏标题里常见、但在 macOS 上**用系统字符检视器插不进去**的符号
/// （winit#3342，开了三年）。
///
/// 界面给它们一个小面板，点一下就填进输入框——这是绕开那个缺陷的办法，
/// 顺带也比让人去系统面板里翻要快。
pub const SYMBOLS: &[&str] = &[
    "★", "☆", "♪", "♥", "※", "・", "〜", "…", "—", "Ⅰ", "Ⅱ", "Ⅲ", "Ⅳ", "Ⅴ", "①", "②", "③", "→",
    "♂", "♀",
];

/// 把子集字体装进这个 [`egui::Context`]。
///
/// **插在比例字体的最前面**：这样连 `★ ♪` 也从 egui 内置的 emoji 图标字体手里夺回来，
/// 整个界面的字形风格是一套的。等宽家族里当**兜底**——把一份比例字体插到等宽最前面
/// 会让本来对齐的哈希值与字节数错开。
pub fn install(ctx: &egui::Context) {
    ctx.add_font(FontInsert::new(
        FAMILY,
        FontData::from_static(SUBSET),
        vec![
            InsertFontFamily {
                family: FontFamily::Proportional,
                priority: FontPriority::Highest,
            },
            InsertFontFamily {
                family: FontFamily::Monospace,
                priority: FontPriority::Lowest,
            },
        ],
    ));
}

/// `text` 里哪些字画不出来（会变成豆腐块），按出现次序、去重。
///
/// 问的是 egui 自己的字形查找，而不是直接读字体的 `cmap`：真正决定屏幕上是不是豆腐块的
/// 是**整条回退链**（子集字体 → 内置字体 → emoji 字体），只读子集那一份会低估覆盖面，
/// 也验不出「装反了」这类错。
///
/// # Panics
/// `ctx` 还没跑过一帧时 egui 里没有字体，这个函数会 panic。装完字体先跑一帧再问。
pub fn missing(ctx: &egui::Context, text: &str) -> Vec<char> {
    let font = FontId::proportional(14.0);
    let mut seen = Vec::new();
    ctx.fonts_mut(|fonts| {
        for ch in text.chars() {
            if !seen.contains(&ch) && !fonts.has_glyph(&font, ch) {
                seen.push(ch);
            }
        }
    });
    seen
}

/// 字体子集本身有多大，字节。报告里说的「打包了多少字体」就是这个数。
#[must_use]
pub fn subset_bytes() -> usize {
    SUBSET.len()
}
