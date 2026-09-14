//! **令牌**：界面的颜色与尺寸，**只有一份来源**。
//!
//! 两套主题的颜色、平台色、字号、圆角、间距、版式尺寸写在同目录的 `tokens.toml` 里。
//! **数据不是代码**，与核心库的 `platforms.toml` / `profiles.toml` / `priorities.toml`
//! 同一条纪律：`include_str!` 编进二进制，运行时不读盘——少一个「令牌文件找不到」的失败模式。
//!
//! 同一份文件两边读：
//!
//! - **界面**：[`crate::look`] 从它装亮暗两套 `Visuals` 与 `Style`（字号、圆角），
//!   那儿一条测试逐项比对，偏一项门禁就红。
//! - **设计稿**：`.scratch/gui-looks-like-the-design/check_tokens.py` 拿它核对
//!   `prototype.html` 顶部的 CSS 变量。
//!
//! 于是改一个颜色只改这一份，两边没法各改各的。
//!
//! ## 读得严
//!
//! 少一项、多一项、颜色写法不对、版本对不上，**解析当场报错**并说出是哪一项。
//! 宽松地读等于允许一个拼错的键悄悄落回默认值——屏上颜色不对，而没有人知道为什么。
//!
//! 平台色是唯一一处形状不同的：键是**平台名**，由核心库的平台清单定，没法在类型里列死。
//! 那一节只强制要有兜底的 `other`；名字对不对得上清单，由测试守。
//!
//! ## 键名是稳定接口
//!
//! 后面每一屏都从这里取值。字段名就是令牌文件里的键（`-` 换成 `_`），**改名等于改接口**。

use std::collections::BTreeMap;
use std::fmt;
use std::sync::LazyLock;

use egui::Color32;
use serde::{Deserialize, Deserializer};

/// 内置的那一份令牌原文。
const BUILTIN: &str = include_str!("tokens.toml");

/// 本程序认得的令牌文件版本。
pub const TOKENS_VERSION: u32 = 1;

/// 解析过一次的内置令牌。
static PARSED: LazyLock<Tokens> =
    LazyLock::new(|| Tokens::parse(BUILTIN).expect("内置令牌必须是好的"));

/// 一整份令牌。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tokens {
    /// 令牌文件的版本。对不上 [`TOKENS_VERSION`] 的一份不读。
    version: u32,
    /// 颜色：两套主题与平台色。
    pub color: Colors,
    /// 圆角三档。
    pub radius: Radius,
    /// 字体预算与字号。
    pub font: Font,
    /// 间距。
    pub space: Space,
    /// 版式尺寸。
    pub layout: Layout,
    /// 调色比例。
    pub mix: Mix,
    /// 阴影：弹层与弹出菜单。
    pub shadow: Shadows,
}

impl Tokens {
    /// 内置的那一份。
    ///
    /// # Panics
    /// 内置令牌读不进来说明这次构建本身是坏的——直接 panic，好过让界面带着半套颜色开窗。
    #[must_use]
    pub fn builtin() -> &'static Self {
        &PARSED
    }

    /// 从一份 TOML 文本读出令牌。
    ///
    /// **先看版本再读正文**：版本不同的一份，正文的形状多半也不同，先报版本才说到点子上。
    ///
    /// # Errors
    /// 版本对不上、少一项、多一项、颜色写法不对、平台色缺兜底那一格时返回错误。
    pub fn parse(text: &str) -> Result<Self, TokensError> {
        #[derive(Deserialize)]
        struct Versioned {
            version: u32,
        }
        let parse = |source| TokensError::Parse(Box::new(source));
        let Versioned { version } = toml::from_str(text).map_err(parse)?;
        if version != TOKENS_VERSION {
            return Err(TokensError::Version { found: version });
        }
        toml::from_str(text).map_err(parse)
    }
}

/// 令牌读不进来。
#[derive(Debug)]
pub enum TokensError {
    /// TOML 写坏了，或者少一项、多一项、颜色写法不对。
    Parse(Box<toml::de::Error>),
    /// 版本对不上。
    Version {
        /// 文件里写的版本。
        found: u32,
    },
}

impl fmt::Display for TokensError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(source) => write!(f, "令牌文件解析失败：{source}"),
            Self::Version { found } => write!(
                f,
                "令牌文件的版本是 {found}，本程序认得的是 {TOKENS_VERSION}"
            ),
        }
    }
}

impl std::error::Error for TokensError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(source) => Some(source.as_ref()),
            Self::Version { .. } => None,
        }
    }
}

/// 颜色：两套主题各一份 [`Palette`]，外加两套主题共用的平台色。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Colors {
    /// 浅色主题。
    pub light: Palette,
    /// 深色主题。
    pub dark: Palette,
    /// 平台色：两套主题共用。卡片的平台标签、字卡底色、平台色块取这里。
    pub platform: Platforms,
    /// 视频格上的播放标：两套主题共用。
    pub video: VideoMark,
}

/// 视频格上那个**播放标**的两个颜色。
///
/// **两套主题共用**：它压在一帧视频画面上，不压在界面底色上——画面多亮多暗与主题无关。
/// 设计稿里没有这两格的 CSS 变量，`check_tokens.py` 不核它们。
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoMark {
    /// 压在首帧上的那层半透明底。
    #[serde(deserialize_with = "hex")]
    pub shade: Color32,
    /// 底上的 ▶。
    #[serde(deserialize_with = "hex")]
    pub mark: Color32,
}

impl Colors {
    /// 那一套主题的颜色。
    #[must_use]
    pub fn theme(&self, theme: egui::Theme) -> &Palette {
        match theme {
            egui::Theme::Dark => &self.dark,
            egui::Theme::Light => &self.light,
        }
    }
}

/// 一套主题的颜色：字段一个一个列死，少一个多一个都读不进来。
macro_rules! palette {
    ($( #[doc = $doc:literal] $field:ident = $key:literal, )*) => {
        /// 一套主题的颜色。字段名就是令牌文件里的键（`-` 换成 `_`）。
        #[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct Palette {
            $(
                #[doc = $doc]
                #[serde(rename = $key, deserialize_with = "hex")]
                pub $field: Color32,
            )*
        }

        #[cfg(test)]
        impl Palette {
            /// 全部键，按令牌文件里的次序。
            pub(crate) const KEYS: &'static [&'static str] = &[$($key),*];

            /// 按令牌文件里的键取颜色。
            pub(crate) fn get(&self, key: &str) -> Option<Color32> {
                match key {
                    $($key => Some(self.$field),)*
                    _ => None,
                }
            }

            /// 按令牌文件里的键取颜色，改它用（变异测试）。
            pub(crate) fn get_mut(&mut self, key: &str) -> Option<&mut Color32> {
                match key {
                    $($key => Some(&mut self.$field),)*
                    _ => None,
                }
            }
        }
    };
}

palette! {
    /// 窗口外的底色：设计稿的页面背景，**不进 egui**。
    ground = "ground",
    /// 窗口底色：侧栏、页面头。
    win = "win",
    /// 面板、卡片、对话框。
    panel = "panel",
    /// 面板里的次级底色：表头、分组头。
    panel_2 = "panel-2",
    /// 凹陷区：进度条槽、代码块。
    sunken = "sunken",
    /// 正文。
    ink = "ink",
    /// 次要文字。
    ink_2 = "ink-2",
    /// 说明文字（在 `panel` 上对比度不低于 4.5）。
    ink_3 = "ink-3",
    /// 占位、禁用。
    ink_4 = "ink-4",
    /// 一点宽的描边、分隔线。
    line = "line",
    /// 输入框描边、强一级的分隔。
    line_2 = "line-2",
    /// 强调色：主按钮、选中、焦点。
    accent = "accent",
    /// 主按钮悬停。
    accent_hover = "accent-hover",
    /// 选中行、选中分面的底色。
    accent_soft = "accent-soft",
    /// 强调色上的文字，或强调色的文字形态。
    accent_ink = "accent-ink",
    /// 主按钮上的文字。
    on_accent = "on-accent",
    /// 高置信、成功。
    hi = "hi",
    /// 高置信的浅底。
    hi_soft = "hi-soft",
    /// 中置信、提醒。
    mid = "mid",
    /// 中置信的浅底。
    mid_soft = "mid-soft",
    /// 低置信、错误、危险操作。
    lo = "lo",
    /// 低置信的浅底。
    lo_soft = "lo-soft",
    /// 没有候选、中性。
    none = "none",
    /// 没有候选的浅底。
    none_soft = "none-soft",
    /// 对话框遮罩（半透明）。
    scrim = "scrim",
    /// 弹层、弹出菜单的阴影（半透明）。
    pop_color = "pop-color",
}

/// 令牌里的一个颜色：`#RRGGBB`，半透明的写 `#RRGGBBAA`——**未预乘**，与 CSS 的 `rgba()` 同义。
///
/// `Color32::from_hex` 还认三位、四位的简写，这里不认：令牌文件要与设计稿逐字核对，
/// 一个颜色两种写法就对不上了。
#[derive(Debug, Clone, Copy)]
struct Hex(Color32);

impl<'de> Deserialize<'de> for Hex {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let digits = text.strip_prefix('#').unwrap_or_default();
        let well_formed =
            matches!(digits.len(), 6 | 8) && digits.bytes().all(|byte| byte.is_ascii_hexdigit());
        match Color32::from_hex(&text) {
            Ok(color) if well_formed => Ok(Self(color)),
            _ => Err(serde::de::Error::custom(format!(
                "颜色「{text}」不是 #RRGGBB 或 #RRGGBBAA"
            ))),
        }
    }
}

/// `deserialize_with` 用：把 [`Hex`] 拆成它包着的颜色。
fn hex<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Color32, D::Error> {
    Hex::deserialize(deserializer).map(|hex| hex.0)
}

/// 平台色：键是**平台名**（核心库平台清单里的 `FC`、`GBA`……），外加兜底的 `other`。
#[derive(Debug, Clone, PartialEq)]
pub struct Platforms {
    /// 平台名 → 颜色。
    by_name: BTreeMap<String, Color32>,
    /// 表里没有的平台。
    other: Color32,
}

impl<'de> Deserialize<'de> for Platforms {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut by_name = BTreeMap::<String, Hex>::deserialize(deserializer)?;
        let other = by_name
            .remove("other")
            .ok_or_else(|| serde::de::Error::missing_field("other"))?;
        Ok(Self {
            by_name: by_name
                .into_iter()
                .map(|(name, hex)| (name, hex.0))
                .collect(),
            other: other.0,
        })
    }
}

impl Platforms {
    /// 那个平台的颜色。表里没有的平台落到兜底那一格。
    #[must_use]
    pub fn of(&self, platform: &str) -> Color32 {
        self.by_name.get(platform).copied().unwrap_or(self.other)
    }

    /// 令牌里写了颜色的那些平台名，不含兜底那一格。
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.by_name.keys().map(String::as_str)
    }
}

/// 圆角三档，点。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Radius {
    /// 分面、标签、小按钮。
    pub small: u8,
    /// 按钮、输入框、列表项。
    pub medium: u8,
    /// 面板、卡片、对话框。
    pub large: u8,
}

/// 字体预算与字号六档。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Font {
    /// 设计稿里常规体的字体栈。界面不读系统字体（打包的子集见 [`crate::font`]），
    /// 这一栏只给设计稿用。
    pub sans: Vec<String>,
    /// 设计稿里等宽体的字体栈，同上。
    pub mono: Vec<String>,
    /// 粗体覆盖哪些字。
    pub bold_coverage: BoldCoverage,
    /// 常规字重。
    pub weight_regular: u16,
    /// 粗体字重：[`crate::font`] 的拉丁粗体子集固定在这一档。
    pub weight_strong: u16,
    /// 角标、分组标题。
    pub size_caption: f32,
    /// 表头、图例（设计稿 11.5）。
    pub size_caption_plus: f32,
    /// 说明文字、表格副行。
    pub size_small: f32,
    /// 屏头说明、表格行（设计稿 12.5）。
    pub size_small_plus: f32,
    /// 正文、按钮。
    pub size_body: f32,
    /// 卡片标题、对话框标题。
    pub size_title: f32,
    /// 页面标题。
    pub size_page: f32,
    /// 作品详情页的大标题。
    pub size_hero: f32,
    /// 分面标签里的条数。
    pub size_mini: f32,
    /// 等宽的路径。
    pub size_path: f32,
    /// 侧边详情头上的标题。
    pub size_detail_title: f32,
    /// 行首封面格里的平台代号。
    pub size_thumb_code: f32,
    /// 侧边详情字卡上的标题。
    pub size_cover_title: f32,
    /// 侧边详情字卡上的平台代号水印。
    pub size_cover_mark: f32,
    /// 表头上排序那枚小箭头。
    pub size_arrow: f32,
    /// 行高，字号的倍数。
    pub line_height: f32,
}

/// 粗体覆盖哪些字。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum BoldCoverage {
    /// 只有拉丁字母与数字是粗体，中文一律常规字重（2026-09-13 裁定）——
    /// [`crate::font`] 的粗体族里一个汉字都没有。
    #[serde(rename = "latin-digits-only")]
    LatinDigitsOnly,
}

/// 间距，点。**档位是约定**：版式里只从 `steps` 里取，由代码走查守，不写测试。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Space {
    /// 间距只从这几档里取。
    pub steps: Vec<f32>,
    /// 面板标题栏内边距：`[上下, 左右]`。
    pub panel_padding: [f32; 2],
    /// 对话框内容区内边距：`[上下, 左右]`。
    pub dialog_padding: [f32; 2],
    /// 表格单元格内边距：`[上下, 左右]`。
    pub cell_padding: [f32; 2],
    /// 开场左栏内边距：`[上下, 左右]`。
    pub opening_hero_padding: [f32; 2],
    /// 开场右栏内边距：`[上下, 左右]`。
    pub opening_side_padding: [f32; 2],
    /// 开场主库列表每一行的内边距：`[上下, 左右]`。
    pub catalog_row_padding: [f32; 2],
    /// 开场主库列表那一行里，名字与底下那句之间的竖向间距。
    pub catalog_row_gap: f32,
    /// 浏览屏左栏内边距。
    pub filter_pane_padding: f32,
    /// 浏览屏右栏内边距。
    pub detail_pane_padding: f32,
    /// 左右两栏里一段与一段之间。
    pub pane_gap: f32,
    /// 一簇分面标签之间。
    pub facet_gap: f32,
    /// 分面标签里名字与条数之间。
    pub facet_chip_gap: f32,
    /// 提示框内边距：`[上下, 左右]`。
    pub note_padding: [f32; 2],
    /// 侧边详情变体卡片内边距：`[上下, 左右]`。
    pub variant_card_padding: [f32; 2],
    /// 表格上方「列表」那一条的内边距：`[上下, 左右]`。
    pub list_bar_padding: [f32; 2],
    /// 表头一格的内边距：`[上下, 左右]`。
    pub table_head_padding: [f32; 2],
    /// 空态那一块上方的留白。
    pub empty_padding: f32,
    /// 侧边详情媒体格之间。
    pub thumb_gap: f32,
    /// 侧边详情头上封面与字之间。
    pub detail_head_gap: f32,
    /// 侧边详情字卡内边距：`[上下, 左右]`。
    pub title_card_padding: [f32; 2],
    /// 条件组那个框的内边距。
    pub rule_box_padding: f32,
    /// 规则原文那一条的内边距：`[上下, 左右]`。
    pub rule_text_padding: [f32; 2],
    /// 一段里小标题与底下内容之间。
    pub section_gap: f32,
    /// 变体卡片里上下两行之间、左右两块之间：`[竖, 横]`。
    pub variant_card_gap: [f32; 2],
    /// 收起那一栏的窄条上下留白。
    pub strip_padding: f32,
    /// 窄条上那颗箭头与竖排栏名之间。
    pub strip_gap: f32,
    /// 表格一格左右留白。
    pub table_cell_padding: f32,
    /// 表格「作品」那一格里行首封面与字之间。
    pub cell_gap: f32,
    /// 「列表」那一条里几样东西之间。
    pub list_bar_gap: f32,
    /// 表头上的字与排序箭头之间。
    pub sort_arrow_gap: f32,
}

/// 版式尺寸，点。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Layout {
    /// 左侧导航展开宽度。
    pub rail_width: f32,
    /// 左侧导航收起后的宽度。
    pub rail_collapsed: f32,
    /// 面板折叠后的窄条。
    pub strip_width: f32,
    /// 浏览页筛选栏宽度。
    pub filter_pane_width: f32,
    /// 浏览页侧边详情宽度。
    pub detail_pane_width: f32,
    /// 作品表格行高（等高行）。
    pub table_row: f32,
    /// 打开「在每行开头显示封面」后的行高。
    pub table_row_cover: f32,
    /// 置信度色条宽度。
    pub tier_bar: f32,
    /// 列表行首封面缩略图：`[宽, 高]`。
    pub thumb_list: [f32; 2],
    /// 卡片封面宽高比（3:4 写成 0.75）。
    pub card_cover_ratio: f32,
    /// 底部状态栏高度。
    pub statusbar: f32,
    /// 对话框只用这几档宽度。
    pub dialog_width: [f32; 4],
    /// 开场右栏宽度：`[最窄, 最宽]`。
    pub opening_side_width: [f32; 2],
    /// 开场左栏最窄宽度。
    pub opening_hero_min: f32,
    /// 开场左上角那枚标志的边长。
    pub mark: f32,
    /// 开场三条承诺前那枚字块的边长。
    pub promise_icon: f32,
    /// 弹层标头「走到第几问」那一圈的直径。
    pub page_dot: f32,
    /// 标签左边那枚圆点的直径（设计稿 `.chip::before`）。
    pub chip_dot: f32,
    /// 「名 → 值」两列排时名那一列的宽。
    pub kv_key_width: f32,
    /// 按钮的高。
    pub button_height: f32,
    /// 按钮左右留白。
    pub button_padding: f32,
    /// 小号按钮的高。
    pub button_small_height: f32,
    /// 小号按钮左右留白。
    pub button_small_padding: f32,
    /// 大号按钮的高。
    pub button_large_height: f32,
    /// 大号按钮左右留白。
    pub button_large_padding: f32,
    /// 控件描边的宽：未激活、悬停、按下、展开四档一样宽。
    pub control_stroke: f32,
    /// 单行输入框的高：与默认那一档按钮等高。
    pub input_height: f32,
    /// 小号单行输入框的高：与小号按钮等高。
    pub input_small_height: f32,
    /// 单行输入框左右留白。
    pub input_padding: f32,
    /// 图标按钮（收起、展开那两颗箭头）的边长。
    pub icon_button: f32,
    /// 分面标签的高。
    pub facet_chip_height: f32,
    /// 分面标签左右留白。
    pub facet_chip_padding: f32,
    /// 行内标签的高。
    pub tag_height: f32,
    /// 行内标签左右留白。
    pub tag_padding: f32,
    /// 表格勾选那一列的宽。
    pub check_column: f32,
    /// 表格定宽那五列：`[平台, 变体, 容量, 年份, 元数据]`。
    pub table_columns: [f32; 5],
    /// 行首封面格顶上那一道平台色的高。
    pub thumb_list_band: f32,
    /// 侧边详情头上封面那一格的宽。
    pub detail_cover_width: f32,
    /// 字卡顶上那一道平台色的高。
    pub title_card_band: f32,
    /// 字卡水印伸出格子多少：`[右, 下]`。
    pub title_card_mark_offset: [f32; 2],
    /// 侧边详情媒体一行几格。
    pub thumbs_per_row: f32,
    /// 表格勾选那一列左边留白。
    pub check_padding: f32,
    /// 平台那一簇先摆几个，其余收进「更多（N）」。
    pub platforms_visible: usize,
}

/// 调色比例：两套主题共用。设计稿里写在规则上的字面量（`color-mix` 的百分比、`opacity`）。
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Mix {
    /// 字卡底色里调进几成平台色。
    pub title_card_tint: f32,
    /// 行首平台色块底色里调进几成平台色。
    pub thumb_list_tint: f32,
    /// 字卡上平台代号水印的不透明度。
    pub watermark_opacity: f32,
}

/// 阴影：一种一格，**形状两套主题共用**，颜色是各主题里同名的那一格（`pop` → `pop-color`）。
///
/// 眼下只有 `pop` 一种。设计稿还有一种 `--shadow`（卡片、窗口外框），画它的那一屏照稿重排时
/// 在这里添一格。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shadows {
    /// 弹层与弹出菜单。
    pub pop: ShadowShape,
}

/// 一种阴影的形状，点。
///
/// 设计稿 `--pop` 的负扩散 egui 画不出（`egui::Shadow::spread` 只收非负数）：令牌里写的是照
/// egui 画得出的样子挑过的那一份，理由在 `tokens.toml` 那一节的注释里。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowShape {
    /// 往哪儿挪：`[横, 竖]`。
    pub offset: [i8; 2],
    /// 半影多宽。
    pub blur: u8,
    /// 往四周扩多少。
    pub spread: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 内置那份原文里把 `from` 换成 `to`——只换第一处。`from` 不在原文里就当场说，
    /// 免得令牌文件改过之后这几条测试悄悄变成什么都没改。
    fn 改(from: &str, to: &str) -> String {
        assert!(BUILTIN.contains(from), "内置令牌里没有「{from}」");
        BUILTIN.replacen(from, to, 1)
    }

    /// 读这一份，**必须**读不进来，返回那句报错。
    fn 报错(text: &str) -> String {
        match Tokens::parse(text) {
            Ok(_) => panic!("该报错的一份令牌读进来了"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn 内置令牌读得出来() {
        // 期望值抄自设计稿 `prototype.html` 顶部的 CSS 变量与 `PCOL` 那张表。
        let tokens = Tokens::builtin();
        assert_eq!(
            tokens.color.dark.accent,
            Color32::from_rgb(0x81, 0x90, 0xF6)
        );
        assert_eq!(
            tokens.color.light.panel_2,
            Color32::from_rgb(0xF0, 0xF2, 0xF6)
        );
        // rgba(0,0,0,.6) → `#00000099`：半透明的按**未预乘**读。
        assert_eq!(
            tokens.color.dark.scrim,
            Color32::from_rgba_unmultiplied(0, 0, 0, 0x99)
        );
        assert_eq!(
            tokens.color.platform.of("GBA"),
            Color32::from_rgb(0x44, 0x53, 0xC4)
        );
        assert_eq!(
            tokens.color.platform.of("清单里没有的平台"),
            Color32::from_rgb(0x55, 0x55, 0x55),
            "表里没有的平台落到 other 那一格",
        );
        // 视频播放标那两格设计稿里没有 CSS 变量：期望值是它们搬进令牌之前在
        // `media.rs` 里写死的样子——搬家不许变色。
        assert_eq!(tokens.color.video.shade, Color32::from_black_alpha(80));
        assert_eq!(tokens.color.video.mark, Color32::WHITE);
        assert_eq!(tokens.radius.medium, 6);
        assert_eq!(tokens.font.size_body, 13.0);
        assert_eq!(tokens.layout.dialog_width, [520.0, 620.0, 720.0, 840.0]);
        // 按钮三档：设计稿 `.btn` / `.btn.sm` / `.btn.lg` 的 height 与左右 padding。
        assert_eq!(
            [tokens.layout.button_height, tokens.layout.button_padding],
            [28.0, 12.0],
            "设计稿 .btn",
        );
        assert_eq!(
            [
                tokens.layout.button_small_height,
                tokens.layout.button_small_padding
            ],
            [24.0, 9.0],
            "设计稿 .btn.sm",
        );
        assert_eq!(
            [
                tokens.layout.button_large_height,
                tokens.layout.button_large_padding
            ],
            [36.0, 18.0],
            "设计稿 .btn.lg",
        );
        // 单行输入框：左右留白照设计稿 `.input`；高与同档按钮等高（拿主意的人 2026-09-14 第三次裁，
        // 设计稿 `.input` 写的 30 不照）。
        assert_eq!(tokens.layout.input_padding, 10.0, "设计稿 .input");
        // 浏览屏表头那五列的宽与两个调色比例：设计稿表头的 style="width:…"、`.tcard` 与 `.lthumb`
        // 的 `color-mix`、`.tc-wm` 的 `opacity`。
        assert_eq!(tokens.layout.table_columns, [56.0, 48.0, 86.0, 58.0, 108.0]);
        assert_eq!(
            [
                tokens.mix.title_card_tint,
                tokens.mix.thumb_list_tint,
                tokens.mix.watermark_opacity
            ],
            [0.22, 0.24, 0.3],
        );
        assert_eq!(
            [tokens.layout.input_height, tokens.layout.input_small_height],
            [28.0, 24.0],
            "与同档按钮等高",
        );
        // 开场主库列表每一行：设计稿 `.catrow` 的 padding 与 gap。
        assert_eq!(
            tokens.space.catalog_row_padding,
            [12.0, 14.0],
            "设计稿 .catrow"
        );
        assert_eq!(tokens.space.catalog_row_gap, 2.0, "设计稿 .catrow");
        // 设计稿 `--pop` 里那一色 rgba(16,20,30,.45)，暗色没有另写，照亮色那一份。
        for palette in [&tokens.color.light, &tokens.color.dark] {
            assert_eq!(
                palette.pop_color,
                Color32::from_rgba_unmultiplied(16, 20, 30, 0x73)
            );
        }
        assert_eq!(tokens.shadow.pop.offset, [0, 10], "设计稿 --pop 的偏移");
    }

    #[test]
    fn 少一项当场报错并说出是哪一项() {
        let 少个颜色 = 改(r##"panel-2     = "#1B2028""##, "");
        assert!(报错(&少个颜色).contains("panel-2"), "{}", 报错(&少个颜色));
        let 少个字号 = 改("size-title   = 15", "");
        assert!(
            报错(&少个字号).contains("size-title"),
            "{}",
            报错(&少个字号)
        );
    }

    #[test]
    fn 多一项当场报错并说出是哪一项() {
        let 多个颜色 = 改("[color.dark]\n", "[color.dark]\npanel-3 = \"#000000\"\n");
        assert!(报错(&多个颜色).contains("panel-3"), "{}", 报错(&多个颜色));
        let 多个尺寸 = 改("[layout]\n", "[layout]\nrail-height = 1\n");
        assert!(
            报错(&多个尺寸).contains("rail-height"),
            "{}",
            报错(&多个尺寸)
        );
    }

    #[test]
    fn 颜色只认六位与八位的十六进制() {
        for 写法 in ["#81F", "8190F6", "#8190FG", "#+190F6"] {
            let text = 改(
                r##"accent      = "#8190F6""##,
                &format!("accent = \"{写法}\""),
            );
            assert!(报错(&text).contains(写法), "{写法}：{}", 报错(&text));
        }
    }

    #[test]
    fn 平台色少了兜底那一格当场报错() {
        let text = 改(r##"other = "#555555""##, "");
        assert!(报错(&text).contains("other"), "{}", 报错(&text));
    }

    #[test]
    fn 版本对不上当场报错() {
        let text = 改("version = 1", "version = 2");
        let 说的 = 报错(&text);
        // 断言「版本是 2」而不是只找数字：解析报错自带行号，随便一句都带着数字。
        assert!(说的.contains("版本是 2"), "{说的}");
    }

    #[test]
    fn 平台色的键都是平台清单里的平台() {
        // 平台色按**平台名**取（`FC`、`GBA`……），名字由核心库的平台清单定。
        // 令牌里写了一个清单里没有的名字，那一格永远取不到——屏上只会默默落到 other。
        let manifest = romcat_core::platform::Manifest::builtin();
        for name in Tokens::builtin().color.platform.names() {
            assert!(
                manifest.platforms().iter().any(|p| p.name == name),
                "令牌里的平台色「{name}」不是平台清单里的平台",
            );
        }
    }
}
