//! 打包进二进制的字体子集。
//!
//! ## 为什么不读系统字体
//!
//! egui 内置的 Ubuntu-Light + NotoEmoji **一个汉字都没有**，而且缺的**不只是汉字**：
//! 中文标点 `，。《》`、假名 `あカ`、罗马数字 `Ⅲ`、带圈数字 `①`、`→ ※ Ⓡ` 全是豆腐块。
//! 补上它只有两条路，读系统字体或者打包一份，这里选后者，理由是三条实测（ADR-0005 的
//! 修订段、`docs/research/egui-viability.md`）：
//!
//! 1. **路径不可硬编码**。macOS 的苹方躺在哈希命名的目录下，各机器不同。
//! 2. **覆盖不一致**。实测苹方缺 `♪`、冬青黑缺 `♪` 与 `Ⓡ`——而这些符号在标题里真实存在。
//!    走系统字体等于「哪些字显示得出来」在每台机器上都是另一个答案，还没法在自己机器上
//!    复现。
//! 3. **更贵**。`FontData` 要求整份字体进内存，没有按需通路；系统中文字体动辄
//!    20–75 MB，比这份 7.3 MB 的子集更占地方。
//!
//! ## 子集怎么来的
//!
//! `tools/font-subset.py` 从完整的 `NotoSansSC[wght].ttf`（17,772,300 字节）裁出来：
//! 字符集取 **GBK + Big5 + 中日韩标点 + 假名 + U+2100–27BF 符号区 + 拉丁扩展**，
//! 字重固定在 400。产物 **7,670,804 字节、22,534 个码位**，同样的输入产出同样的字节。
//! 改字符集就重跑那个脚本，别手工动这份二进制。
//!
//! 另两份也出自那个脚本，字符集都只有 **ASCII + 带变音符的拉丁字母**（`· × °` 这类拉丁补充里的
//! 标点与符号不收，中文标题里用的正是它们）：
//! **拉丁粗体**从同一个 `NotoSansSC[wght].ttf` 固定在 600（28,272 字节），
//! **拉丁等宽**从 `JetBrainsMono[wght].ttf` 固定在 400（53,512 字节）。
//!
//! ## 三个字族
//!
//! 界面只用三个族。**名字与回退顺序是稳定接口**：屏上每一处字形都由它们定，
//! 动一处，每一屏的字都跟着变。
//!
//! | 字族 | 回退链（前一份里没有的字往后找） | 设计稿令牌 `[font]` |
//! |---|---|---|
//! | `FontFamily::Proportional` | 常规体 → egui 内置 | `sans` |
//! | `FontFamily::Name("strong")` | 拉丁粗体 → 常规体 → egui 内置 | `weight-strong = 600`、`bold-coverage = "latin-digits-only"` |
//! | `FontFamily::Monospace` | 拉丁等宽 → egui 内置（Hack 排头）→ 常规体 | `mono` |
//!
//! **不打包中文粗体**（2026-09-13 裁定）：中文的层次靠字号、颜色与间距，一份中文粗体要多背
//! 7 MB。粗体与等宽两份子集一个汉字都没有，中文在这两个族里回退常规体。
//! **没有字重合成**：egui 0.36 不会自己给字形加粗，粗体族里的中文与常规族里的是同一个字形
//! ——`tests/font.rs` 逐字比过图集纹理。
//!
//! **子集之外的字仍然是豆腐块**（CJK 扩展 B 的生僻字、谚文等）。这不是失手，是取舍：
//! 完整字体 16.9 MB。要知道自己的库会不会撞上，跑 `romcat-gui --font-check --catalog …`，
//! 它把中立库里全部变体的键过一遍，缺字直接报出来。

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily, FontId};

/// 子集字体本体。
///
/// `include_bytes!` 而不是运行时读文件：少一个「字体找不到」的失败模式。代价是许可也得
/// 跟着走，见 [`LICENSES`]。
const SUBSET: &[u8] = include_bytes!("../assets/NotoSansSC-Subset.ttf");

/// 打包字体的许可全文（SIL Open Font License 1.1），一家一份：`(字体家族, 全文)`。
///
/// **跟着二进制走**。OFL 要求分发字体时随附许可，而这几份字体是嵌进可执行文件的——许可
/// 若只躺在源码树里，被拷出去的那个可执行文件就不合规。两家的版权行不同，所以各带一份：
/// 常规体与拉丁粗体出自 Noto Sans SC，拉丁等宽出自 JetBrains Mono。界面的字体样张里看得到它们。
pub const LICENSES: &[(&str, &str)] = &[
    ("Noto Sans SC", include_str!("../assets/OFL.txt")),
    (
        "JetBrains Mono",
        include_str!("../assets/OFL-JetBrainsMono.txt"),
    ),
];

/// 常规体子集在 egui 里的名字。
pub const REGULAR_FACE: &str = "NotoSansSC-Subset";

/// 拉丁与数字的粗体子集本体：与常规体同一个源，固定在 600，**一个汉字都没有**。
const STRONG_SUBSET: &[u8] = include_bytes!("../assets/NotoSansSC-SemiBold-Latin.ttf");

/// 粗体子集在 egui 里的名字。
pub const STRONG_FACE: &str = "NotoSansSC-SemiBold-Latin";

/// 拉丁等宽子集本体：JetBrains Mono 固定在 400，**一个汉字都没有**。
const MONO_SUBSET: &[u8] = include_bytes!("../assets/JetBrainsMono-Latin.ttf");

/// 等宽子集在 egui 里的名字。
pub const MONO_FACE: &str = "JetBrainsMono-Latin";

/// 粗体族在 egui 里的名字（`FontFamily::Name`）。设计稿令牌 `[font] weight-strong` 那一档
/// 落在这个族上。
const STRONG: &str = "strong";

/// 粗体族：拉丁字母与数字是粗体，其余的字回退常规体（回退链见模块文档那张表）。
#[must_use]
pub fn strong_family() -> FontFamily {
    FontFamily::Name(STRONG.into())
}

/// 等宽的使用入口：**路径、哈希、序列号、数量与容量**用它，一列扫下来位数对得齐。
///
/// 只换字族，字号跟着所在处的正文走——等宽的数字与同一行旁边的字一样大。等宽只给上面
/// 这几样用；混在里面的中文照旧回退常规体。
#[must_use]
pub fn mono(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).family(FontFamily::Monospace)
}

/// 粗体的使用入口：标题里要粗的拉丁字母与数字用它。照着 `ui.strong` 的样子带上强调色，
/// 所以界面上原来写 `ui.strong(x)` 的地方换成 `ui.label(font::strong(x))`，只多出拉丁粗体这一样。
///
/// 中文落回常规体（不打包中文粗体），一句纯中文的标题换过来，屏上一个像素都不变。
#[must_use]
pub fn strong(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).family(strong_family()).strong()
}

/// 界面用得到的三个字族：常规、粗体、等宽。
///
/// 字体检查（[`missing`]）逐个问它们——屏上用到哪个族，哪个族就不许有豆腐块。
#[must_use]
pub fn families() -> [FontFamily; 3] {
    [
        FontFamily::Proportional,
        strong_family(),
        FontFamily::Monospace,
    ]
}

/// 界面上必须显示得出来的字符，**一个豆腐块都不许有**。
///
/// 这不是随手挑的：前四组是 egui 内置字体**确实缺**的四类（汉字、假名、中文标点、
/// 符号），后一组是标题里真实出现的记号。裁字体时若把某个区段漏掉，
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

/// 把三份子集字体与三个字族（[`families`]）装进这个 [`egui::Context`]，下一帧起生效。
///
/// 开窗与不开窗跑帧走的都是这一句，屏上与测试里画的是同一套字。
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions());
}

/// 界面用的全部字体，与三个字族各自的回退链。
///
/// - **常规体插在比例字体的最前面**：这样连 `★ ♪` 也从 egui 内置的 emoji 图标字体手里
///   夺回来，整个界面的字形风格是一套的。
/// - **粗体族 = 拉丁粗体 + 整条比例字体的链**：粗体子集里没有的字（中文、假名、标点、符号）
///   一律落回常规体——不是豆腐块，也不是哪份字体合成出来的假粗体。
/// - **等宽族里拉丁等宽排头、常规体垫底**：常规体是比例字体，插到等宽前面会让本来对齐的
///   哈希值与字节数错开；等宽子集里没有的 `… — →` 先落到 egui 内置的 Hack（也是等宽），
///   汉字才落到常规体。
///
/// 插在最前面不会挡住兜底：epaint 是**逐字符**沿回退链找字形的，前一份里没有的字照样
/// 落到后面那份上。
#[must_use]
fn definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        REGULAR_FACE.to_owned(),
        Arc::new(FontData::from_static(SUBSET)),
    );
    fonts.font_data.insert(
        STRONG_FACE.to_owned(),
        Arc::new(FontData::from_static(STRONG_SUBSET)),
    );

    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    proportional.insert(0, REGULAR_FACE.to_owned());
    let sans = proportional.clone();

    fonts.font_data.insert(
        MONO_FACE.to_owned(),
        Arc::new(FontData::from_static(MONO_SUBSET)),
    );
    let monospace = fonts.families.entry(FontFamily::Monospace).or_default();
    monospace.insert(0, MONO_FACE.to_owned());
    monospace.push(REGULAR_FACE.to_owned());

    let strong = std::iter::once(STRONG_FACE.to_owned())
        .chain(sans)
        .collect();
    fonts.families.insert(strong_family(), strong);
    fonts
}

/// `text` 里哪些字画不出来（会变成豆腐块），按出现次序、去重。**三个字族**（[`families`]）
/// 里任何一个画不出来就算：屏上用到哪个族，哪个族就得过。
///
/// 问的是 egui 自己的回退链，而不是直接读某一份字体的 `cmap`：真正决定屏幕上是不是豆腐块的
/// 是**整条回退链**（子集字体 → 内置字体 → emoji 字体），只读子集那一份会低估覆盖面，
/// 也验不出「装反了」这类错。
///
/// **egui 的 `has_glyph` 单独用不得。** 它的判法是「这个字落到的那份字体不是替换字 `◻`
/// 那份」，于是落到替换字那份字体上的字——哪怕那份字体真带着它——一律被报成画不出来。
/// 等宽族里替换字那份是内置的 Hack，`… — → ∀ ♪` 正由它画；常规族里是 NotoEmoji，`🎮`
/// 这类由它画。所以 `has_glyph` 说没有时，再查一遍回退链上有没有哪份字体带着这个字。
/// 反过来 `has_glyph` 说有就是有：零宽字符这类 `cmap` 里没有、egui 照样当画得出来的，靠它。
///
/// # Panics
/// `ctx` 还没跑过一帧时 egui 里没有字体，这个函数会 panic。装完字体先跑一帧再问。
pub fn missing(ctx: &egui::Context, text: &str) -> Vec<char> {
    let mut seen = Vec::new();
    ctx.fonts_mut(|fonts| {
        // 没装进这个 `Context` 的族不问：没装子集字体时就没有粗体族，问了 egui 当场 panic。
        let ids: Vec<FontId> = families()
            .into_iter()
            .filter(|family| fonts.definitions().families.contains_key(family))
            .map(|family| FontId::new(14.0, family))
            .collect();
        for ch in text.chars() {
            if seen.contains(&ch) {
                continue;
            }
            let drawable = ids.iter().all(|id| {
                fonts.has_glyph(id, ch)
                    || fonts.fonts.font(&id.family).characters().contains_key(&ch)
            });
            if !drawable {
                seen.push(ch);
            }
        }
    });
    seen
}

/// 常规体子集本身有多大，字节。打包字体的大头就是这一份。
#[must_use]
pub fn subset_bytes() -> usize {
    SUBSET.len()
}

/// 拉丁粗体与等宽两份子集一共多大，字节——不打包中文粗体，字体预算里多出来的就只有这些。
#[must_use]
pub fn latin_bytes() -> usize {
    STRONG_SUBSET.len() + MONO_SUBSET.len()
}
