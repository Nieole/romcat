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
    /// 表头、图例：设计稿上的半号（11.5）。不挂进 egui 的字号表，用到的那一处直接取。
    pub size_caption_plus: f32,
    /// 说明文字、表格副行。
    pub size_small: f32,
    /// 屏头说明、表格行：设计稿上的半号（12.5）。不挂进 egui 的字号表，用到的那一处直接取
    /// （屏头的副标题见 [`crate::look::screen_header`]）。
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
    /// 面板标题（设计稿 `.phead h3`）。
    pub size_panel_title: f32,
    /// 库体检那几格里的数（设计稿 `.htile b`，等宽）。
    pub size_health_value: f32,
    /// 空态那张卡的标题（设计稿子库屏空态卡与待确认屏 `#q-empty` 的 `h3` 都是 16px）。不挂成具名档。
    pub size_empty_title: f32,
    /// 行高，字号的倍数。
    pub line_height: f32,
    /// 左栏收成窄条后入口底下那个计数的字号。
    pub size_badge_narrow: f32,
    /// 左栏分组标题的字距，字号的倍数。
    pub group_tracking: f32,
    /// 大号按钮上的字。
    pub size_button_large: f32,
    /// 待确认屏正文头上三格里的数（设计稿 `.qcell .v`）。
    pub size_summary_count: f32,
    /// 一批变体卡头上那个条数（设计稿 `.bhead .cnt`）。
    pub size_batch_count: f32,
    /// 候选卡片上作品那一行（设计稿 `.cand .t`）。
    pub size_candidate_title: f32,
    /// 作品详情页元数据那一面值那几行的行高，字号的倍数（设计稿 `.mrow .v`）。
    pub meta_value_line_height: f32,
    /// 作品详情页段落卡的标题：设计稿上的半号（14.5），用到时走 [`crate::look::font_size`]。
    pub size_section_title: f32,
    /// 没有 ffmpeg 那一块里那个 ▶（设计稿 `.noff i`）。
    pub size_noff_mark: f32,
    /// 媒体那一格视频上压的 ▶（设计稿 `.mtile .pv .play`）。
    pub size_play_mark: f32,
    /// 作品详情页头上那张字卡的标题（设计稿 `.tc-t`）。
    pub size_hero_card_title: f32,
    /// 那张字卡的平台代号水印（设计稿 `.tc-wm`）。
    pub size_hero_card_mark: f32,
    /// 概览里简介那一段：设计稿上的半号（13.5），用到时走 [`crate::look::font_size`]。
    pub size_desc: f32,
    /// 那一段的行高，字号的倍数（设计稿 `.desc`）。
    pub desc_line_height: f32,
}

/// 粗体覆盖哪些字。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum BoldCoverage {
    /// 只有拉丁字母与数字是粗体，中文一律常规字重（2026-09-13 裁定）——
    /// [`crate::font`] 的粗体族里一个汉字都没有。
    #[serde(rename = "latin-digits-only")]
    LatinDigitsOnly,
}

/// 间距，点。
///
/// `steps` 是**通用间距的档位**：版式里随手要一个间距时从这几档里取，由代码走查守，不写测试。
/// 设计稿上写死的字面值（面板、对话框、开场、屏头、屏体、左栏……）不硬凑进档位，各立一个具名令牌，
/// 由 `check_tokens.py` 逐项对着设计稿核。
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
    /// 表头一格的内边距：`[上下, 左右]`。浏览屏表头、任务屏历史表头共用。
    pub table_head_padding: [f32; 2],
    /// 空态那一块的留白。浏览屏筛空、任务屏空台共用。
    pub empty_padding: f32,
    /// 库屏**库体检**那一块还没扫描时那一句的留白（设计稿在那一处行内写成 18，不是 `.empty` 的 28）。
    pub health_empty_padding: f32,
    /// 库体检那几格外面那一圈：`[上下, 左右]`（设计稿 `.health`）。
    pub health_grid_padding: [f32; 2],
    /// 库体检格与格之间（设计稿 `.health` 的 `gap`）。
    pub health_grid_gap: f32,
    /// 库体检一格的内边距：`[上下, 左右]`（设计稿 `.htile`）。
    pub health_tile_padding: [f32; 2],
    /// 一格里标题、数、小字之间（设计稿 `.htile` 的 `gap`）。
    pub health_tile_gap: f32,
    /// 体检明细一行的内边距：`[上下, 左右]`（设计稿 `.lst>div`）。
    pub health_list_padding: [f32; 2],
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
    /// 面板标题栏里标题、说明、按钮之间的横向间距（设计稿 `.phead` 的 `gap`）。
    pub panel_head_gap: f32,
    /// 库屏两栏之间、右栏几块之间的间距（设计稿 `.libgrid` 的 `gap`）。
    pub library_gap: f32,
    /// 屏头的内边距：`[上下, 左右]`。
    pub screen_header_padding: [f32; 2],
    /// 屏头里标题、副标题、右侧那一段彼此隔多远。
    pub screen_header_gap: f32,
    /// 屏体的内边距：`[上, 左右, 下]`。
    pub screen_body_padding: [f32; 3],
    /// 左栏的内边距：`[上下, 左右]`。
    pub rail_padding: [f32; 2],
    /// 左栏收成窄条后的内边距：`[上下, 左右]`。
    pub rail_padding_collapsed: [f32; 2],
    /// 左栏里一项与下一项隔多远。
    pub rail_gap: f32,
    /// 左栏顶上切换主库那张卡的内边距：`[上下, 左右]`。
    pub rail_switch_padding: [f32; 2],
    /// 那张卡里标志与字之间。
    pub rail_switch_gap: f32,
    /// 那张卡底下再空多少。
    pub rail_switch_margin: f32,
    /// 左栏分组标题的内边距：`[上, 左右, 下]`。
    pub rail_group_padding: [f32; 3],
    /// 收成窄条后分组标题变成一道线，线四周空多少：`[上下, 左右]`。
    pub rail_group_margin_collapsed: [f32; 2],
    /// 左栏入口左右留白。
    pub nav_padding: f32,
    /// 收成窄条后入口上下留白。
    pub nav_padding_collapsed: f32,
    /// 收成窄条后入口的字与计数之间。
    pub nav_gap_collapsed: f32,
    /// 任务跑着时入口计数前那枚圆点与数之间。
    pub live_badge_gap: f32,
    /// 左栏栏底几样之间。
    pub rail_foot_gap: f32,
    /// 栏底那道线底下空多少。
    pub rail_foot_padding: f32,
    /// 栏底「已保存 N 条裁决」那一行左右留白。
    pub rail_note_padding: f32,
    /// 那一行圆点与字之间。
    pub rail_note_gap: f32,
    /// 屏体里一块与一块之间的竖向间距。
    pub screen_section_gap: f32,
    /// 小标题与它底下那一块之间的竖向间距。
    pub section_title_gap: f32,
    /// 正在跑那张卡的内边距。
    pub card_padding: f32,
    /// 正在跑那张卡里一排与一排之间的竖向间距。
    pub card_row_gap: f32,
    /// 等待中那一行卡片的内边距：`[上下, 左右]`。
    pub queue_row_padding: [f32; 2],
    /// 正在跑那张卡底下那一排，格与格之间的间距。
    pub meta_gap: f32,
    /// 待确认屏正文头上三格之间（设计稿 `.qsum` 的 `gap`）。
    pub queue_summary_gap: f32,
    /// 三格底下空多少（设计稿 `.qsum` 的 `margin-bottom`）。
    pub queue_summary_margin: f32,
    /// 三格每一格的内边距：`[上下, 左右]`（设计稿 `.qcell`）。
    pub queue_cell_padding: [f32; 2],
    /// 裁决记录抽屉里一行、页脚那句说明的内边距：`[上下, 左右]`（设计稿 `.lot`）。
    pub record_row_padding: [f32; 2],
    /// 待确认屏空态那张卡上面空多少（设计稿 `#q-empty .card` 的 `margin`）。
    pub empty_state_margin: f32,
    /// 那张卡的内边距（设计稿 `#q-empty .card` 的 `padding`）。
    pub empty_state_padding: f32,
    /// 那张卡里标题后、说明后、按钮后各空多少：`[标题后, 说明后, 按钮后]`。
    pub empty_state_gaps: [f32; 3],
    /// 一批变体卡片与卡片之间（设计稿 `.batch` 的 `margin-bottom`）。
    pub batch_gap: f32,
    /// 卡头内边距：`[上下, 左右]`（设计稿 `.bhead`）。
    pub batch_head_padding: [f32; 2],
    /// 卡头里几列之间（设计稿 `.bhead` 的 `gap`）。
    pub batch_head_gap: f32,
    /// 展开之后那一块的内边距：`[上, 左右, 下]`（设计稿 `.bbody`）。
    pub batch_body_padding: [f32; 3],
    /// 展开之后那一块里几排之间、两栏之间（设计稿 `.bbody` 的 `gap`）。
    pub batch_body_gap: f32,
    /// 判定依据那一框的内边距：`[上下, 左右]`（设计稿 `.why`）。
    pub why_padding: [f32; 2],
    /// 随机样本一行上下留白（设计稿 `.smp`）。
    pub sample_row_padding: f32,
    /// 「没有候选」那个虚线框上面空多少（设计稿 `.bare` 的 `margin-top`）。
    pub bare_margin: f32,
    /// 那个虚线框的内边距：`[上下, 左右]`（设计稿 `.bare`）。
    pub bare_padding: [f32; 2],
    /// 虚线框里头一行与底下那句说明之间（设计稿 `.bare .help` 的 `margin-top`）。
    pub bare_note_gap: f32,
    /// 卡头第一行依据形状各段之间：`[上下, 左右]`（设计稿 `.shape` 的 `gap`）。
    pub shape_gap: [f32; 2],
    /// 卡头两行字之间（设计稿 `.bhead .why1` 的 `margin-top`）。
    pub batch_line_gap: f32,
    /// 候选卡片里「来源 / 匹配 / 依据」那几行之间：`[上下, 左右]`（设计稿 `.cand dl` 的 `gap`）。
    pub candidate_row_gap: [f32; 2],
    /// 展开之后「细分」底下各组一行与一行之间（设计稿 `.dist` 的 `gap` 竖向那一半）。
    pub dist_row_gap: f32,
    /// 下钻到某一组之后**就地那一框**的内边距：`[上下, 左右]`（设计稿 `.drill` 的 `padding`）。
    pub drill_padding: [f32; 2],
    /// 就地那一框里一排与一排之间（设计稿 `.drill` 的 `gap`）。
    pub drill_gap: f32,
    /// 逐条那一屏待选列表栏头的内边距：`[上, 右, 下, 左]`（设计稿 `.obolist .ptitle`）。
    pub list_head_padding: [f32; 4],
    /// 待选列表一条的内边距：`[上, 右, 下, 左]`（设计稿 `.oboit`）。
    pub list_item_padding: [f32; 4],
    /// 逐条那一屏详情的内边距：`[上下, 左右]`（设计稿 `.obodet`）。
    pub detail_padding: [f32; 2],
    /// 详情里一块与一块之间（设计稿 `.obodet` 的 `gap`）。
    pub detail_gap: f32,
    /// 逐条那一屏详情抬头那几行之间（设计稿 `.obodet` 头一块的 `gap`）。
    pub obo_head_gap: f32,
    /// 候选卡片之间（设计稿 `.cands` 的 `gap`）。
    pub candidate_gap: f32,
    /// 候选卡片的内边距（设计稿 `.cand`）。
    pub candidate_padding: f32,
    /// 候选卡片里一排与一排之间（设计稿 `.cand` 的 `gap`）。
    pub candidate_inner_gap: f32,
    /// 键位提示那一框的内边距：`[上下, 左右]`（设计稿 `.keys`）。
    pub keys_padding: [f32; 2],
    /// 键位提示里一组与一组之间：`[上下, 左右]`（设计稿 `.keys` 的 `gap`）。
    pub keys_gap: [f32; 2],
    /// 一组键位提示里键帽与字之间（设计稿 `.keys span` 的 `gap`）。
    pub key_hint_gap: f32,
    /// 键帽的内边距：`[上下, 左右]`（设计稿 `.kbd`）。
    pub kbd_padding: [f32; 2],
    /// 带键帽的按钮里字与键帽之间（设计稿 `.btn` 的 `gap`）。
    pub key_button_gap: f32,
    /// 中文离线源那一堆头上那一条的内边距：`[上下, 左右]`（设计稿 `.mgroup .gh`）。
    pub match_head_padding: [f32; 2],
    /// 那一堆里字段那几行的内边距：`[上下, 左右]`（设计稿 `.mgroup dl`）。
    pub match_row_padding: [f32; 2],
    /// 那几行之间、字段名与值之间：`[上下, 左右]`（设计稿 `.mgroup dl` 的 `gap`）。
    pub match_row_gap: [f32; 2],
    /// 作品详情页顶上那一条的内边距：`[上下, 左右]`（设计稿 `.wdbar`）。
    pub work_bar_padding: [f32; 2],
    /// 那一条里几样东西之间（设计稿 `.wdbar` 的 `gap`）。
    pub work_bar_gap: f32,
    /// 作品详情页六个面那一排左右留白（设计稿 `.tabs`）。
    pub tabs_padding: f32,
    /// 面与面之间（设计稿 `.tabs` 的 `gap`）。
    pub tabs_gap: f32,
    /// 一个面左右留白（设计稿 `.tabs button`）。
    pub tab_padding: f32,
    /// 面名与后头那个数之间（设计稿 `.tabs button small`）。
    pub tab_count_gap: f32,
    /// 作品详情页一面的正文内边距：`[上, 左右, 下]`（设计稿 `.tabp`）。
    pub tab_panel_padding: [f32; 3],
    /// 作品详情页一张卡与下一张之间（设计稿 `.vcard` 的 `margin-bottom`）。
    pub work_card_gap: f32,
    /// 那张卡头一行的内边距：`[上下, 左右]`（设计稿 `.vhead`）。
    pub work_card_head_padding: [f32; 2],
    /// 头一行里几样东西之间（设计稿 `.vhead` 的 `gap`）。
    pub work_card_head_gap: f32,
    /// 那张卡身子的内边距：`[上下, 左右]`（设计稿 `.vbody`）。
    pub work_card_body_padding: [f32; 2],
    /// 身子左右两栏之间（设计稿 `.vbody` 的 `gap` 横着那一半）。
    pub work_card_columns_gap: f32,
    /// 「名 → 值」两列：`[行与行之间, 名与值之间]`（设计稿 `.infol` 的 `gap`）。
    pub info_list_gap: [f32; 2],
    /// 文件表一格的内边距：`[上下, 左右]`（设计稿 `.ftbl th/td`）。
    pub file_table_cell_padding: [f32; 2],
    /// 识别依据那一面卡片身子的内边距：`[上下, 左右]`（设计稿 `evTab` 卡里那一块）。
    pub evidence_card_padding: [f32; 2],
    /// 作品详情页元数据那一面一行的内边距：`[上下, 左右]`（设计稿 `.mrow`）。
    pub meta_row_padding: [f32; 2],
    /// 那一行里 `[上下两块之间, 三列之间]`（设计稿 `.mrow` 的 `gap`）。
    pub meta_row_gap: [f32; 2],
    /// 那一行名那一列里名与底下那行小字之间（设计稿 `.mrow .fl small`）。
    pub meta_label_gap: f32,
    /// 「其他来源」里一句的内边距：`[上下, 左右]`（设计稿 `.alt`）。
    pub alt_padding: [f32; 2],
    /// 那一句里来源、值、按钮之间（设计稿 `.alt` 的 `gap`）。
    pub alt_gap: f32,
    /// 「其他来源」一句与下一句之间（设计稿 `.alts` 的 `gap`）。
    pub alts_gap: f32,
    /// 来源标签左右留白（设计稿 `.srcb`）。
    pub source_badge_padding: f32,
    /// 编辑态下一格底下那一排值之间、一个里来源与值之间（设计稿 `.pickchips` / `.pickchip` 的 `gap`）。
    pub pick_chip_gap: f32,
    /// 那一排里一个的左右留白：`[左, 右]`（设计稿 `.pickchip`）。
    pub pick_chip_padding: [f32; 2],
    /// 作品详情页编辑态底下那一条的内边距：`[上下, 左右]`（设计稿 `.savebar`）。
    pub save_bar_padding: [f32; 2],
    /// 那一条里几样东西之间（设计稿 `.savebar` 的 `gap`）。
    pub save_bar_gap: f32,
    /// 作品详情页段落卡的内边距：`[上下, 左右]`（设计稿 `.sect`）。
    pub section_card_padding: [f32; 2],
    /// 段落卡标题那一行底下空多少（设计稿 `.sect>.row:first-child`）。
    pub section_card_title_gap: f32,
    /// 已隐藏的名称一行上下留白（设计稿 `titleTab` 里那一行）。
    pub suppressed_row_padding: f32,
    /// 作品详情页媒体那一面一格与一格之间（设计稿 `.mgrid` 的 `gap`）。
    pub media_grid_gap: f32,
    /// 媒体那一格底下字那一块的内边距：`[上下, 左右]`（设计稿 `.mtile .mi`）。
    pub media_info_padding: [f32; 2],
    /// 没有 ffmpeg 那一块里几行之间（设计稿 `.noff` 的 `gap`）。
    pub noff_gap: f32,
    /// 那一块四周留白（设计稿 `.noff` 的 `padding`）。
    pub noff_padding: f32,
    /// 作品详情页头上那一块的内边距：`[上, 左右, 下]`（设计稿 `.hero`）。
    pub hero_padding: [f32; 3],
    /// 那一块里封面与右边那一栏之间（设计稿 `.hero` 的 `gap`）。
    pub hero_gap: f32,
    /// 头上那几格事实：`[行与行之间, 格与格之间]`（设计稿 `.hfacts` 的 `gap`）。
    pub hero_facts_gap: [f32; 2],
    /// 头上那张字卡的内边距：`[上下, 左右]`（设计稿 `.hcover .tcard`）。
    pub hero_card_padding: [f32; 2],
    /// 那张字卡里标题、副行之间（设计稿 `.tcard` 的 `gap`）。
    pub hero_card_gap: f32,
    /// 概览那一面两栏之间、一块与一块之间（设计稿 `.ov` / `.col` 的 `gap`）。
    pub overview_gap: f32,
    /// 概览里媒体那一块三格之间（设计稿 `.mstrip` 的 `gap`）。
    pub media_strip_gap: f32,
    /// 头上那几格事实里名与值之间（设计稿 `.hfacts div` 的 `gap`）。
    pub hero_fact_gap: f32,
    /// 设置屏左边那一列与右边内容之间（设计稿 `.sets` 的 `gap`）。
    pub settings_gap: f32,
    /// 设置屏左边那一列两节之间（设计稿 `.sets nav` 的 `gap`）。
    pub settings_nav_gap: f32,
    /// 设置屏左边那一列一节的左右留白（设计稿 `.sets nav button` 的 `padding`）。
    pub settings_nav_padding: f32,
    /// 设置屏左边那一列与那道竖线之间（设计稿 `.sets nav` 的 `padding-right`）。
    pub settings_nav_divider: f32,
    /// 设置屏右边内容里一行与一行之间（设计稿 `.sset` 的 `gap`）。
    pub settings_body_gap: f32,
    /// 设置屏一行里：`[上下两样之间, 名与值两列之间]`（设计稿 `.srow` 的 `gap`）。
    pub settings_row_gap: [f32; 2],
    /// 设置屏一行底下那道线上头留多少（设计稿 `.srow` 的 `padding-bottom`）。
    pub settings_row_bottom: f32,
    /// 设置屏名那一列比值那一列低多少（设计稿 `.srow>b` 的 `padding-top`）。
    pub settings_label_top: f32,
    /// 开关那个小滑块与它旁边的字之间（设计稿 `.switch` 的 `gap`）。
    pub settings_switch_gap: f32,
    /// 快捷键表：`[行与行之间, 左右两列之间]`（设计稿 `.kgrid` 的 `gap`）。
    pub keys_grid_gap: [f32; 2],
    /// 快捷键表一条的上下留白（设计稿 `.kgrid div` 的 `padding`）。
    pub keys_row_padding: f32,
    /// 快捷键表一条里说明与键帽之间至少留多少（设计稿 `.kgrid div` 的 `gap`）。
    pub keys_row_gap: f32,
}

/// 版式尺寸，点。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Layout {
    /// 左侧导航展开宽度。
    pub rail_width: f32,
    /// 左侧导航收起后的宽度。
    pub rail_collapsed: f32,
    /// 窗口宽不到这么多时左栏自动收成窄条。怎么算出来的写在令牌文件那一行的注释里。
    pub rail_collapse_below: f32,
    /// 左栏顶上切换主库那张卡上标志的边长。
    pub rail_mark: f32,
    /// 左栏入口的高。
    pub nav_height: f32,
    /// 左栏里那两枚圆点的直径：任务跑着时、栏底裁决数前。
    pub rail_dot: f32,
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
    /// 卡片视图三档封面宽：小、中、大。
    pub card_widths: [f32; 3],
    /// 卡片封面下信息区高度。
    pub card_info_height: f32,
    /// 按平台分组时组头高度。
    pub card_group_height: f32,
    /// 底部状态栏高度。
    pub statusbar: f32,
    /// 底部状态栏里任务那条小进度条的宽度。
    pub statusbar_bar: f32,
    /// 正在跑那张卡左边那条强调色竖条的宽。
    pub runcard_bar: f32,
    /// 正在跑那张卡上进度条的高。
    pub runcard_progress: f32,
    /// 「细分」底下每一项那条占比条的高（设计稿 `.dist .b` 的 `height`）。
    pub dist_bar: f32,
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
    /// 标签的高。
    pub chip_height: f32,
    /// 标签左右留白。
    pub chip_padding: f32,
    /// 标签里圆点与字之间。
    pub chip_gap: f32,
    /// 只读标签的高（设计稿 `.ro`）。
    pub ro_height: f32,
    /// 只读标签左右留白。
    pub ro_padding: f32,
    /// 只读标签里圆点与字之间。
    pub ro_gap: f32,
    /// 「名 → 值」两列排时名那一列的宽。
    pub kv_key_width: f32,
    /// 弹层表单名那一列的宽（设计稿 `.frm`：子库目标设置）。与 [`Self::kv_key_width`]（设计稿 `.kv`）是两格：稿上一个 96、一个 84。
    pub form_label_width: f32,
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
    /// 图标按钮的边长（设计稿 `.iconbtn`）：浏览屏收起、展开那两颗箭头，库屏面板标题栏里那枚折叠标，子库屏规则行尾那颗「×」「✎」。
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
    /// 工序那一行头两列的宽：`[圆点, 工序名]`（设计稿 `.stage` 的 `grid-template-columns`）。
    pub stage_columns: [f32; 2],
    /// 工序圆点的直径（设计稿 `.stage .dot`）。
    pub stage_dot: f32,
    /// 工序圆点描边的宽（设计稿 `.stage .dot` 的 `border`）。
    pub stage_dot_stroke: f32,
    /// 行左边那条状态竖条的宽（设计稿 `.stage.next`、`.tbl td.st` 的 `inset 3px`）。
    pub row_stripe: f32,
    /// 面板标题栏那枚折叠标三角的宽（设计稿 `.iconbtn` 里 13px 字号的 `▾`，照稿图量约 7 点）。
    pub fold_mark: f32,
    /// 导出设置那一块前端格式下拉的宽（设计稿 `select.input` 的 `width:150px`）。
    pub format_select_width: f32,
    /// 库屏根那张表根名称那一列的宽度上限：超过就截断加「…」、悬停看全名，路径那一列始终留得出地方
    /// （拿主意的人 2026-09-14 定，稿上没有这一格）。
    pub root_name_max: f32,
    /// 库体检重复拷贝明细表后四列的宽：平台、份数、单份大小、展开标（设计稿 `dups` 那张表的 `th`）。
    pub health_dup_columns: [f32; 4],
    /// 库体检明细列表最多多高：超过就在列表里滚、只画看得见的那几行（虚拟化列表）。
    pub health_list_max_height: f32,
    /// 子库卡上容量条的高（设计稿 `.gauge`）。
    pub gauge_height: f32,
    /// 容量条上「清单之外：还不知道」那一段斜纹一个来回多宽（设计稿 `.gauge .unk`：一半有色、一半空）。
    pub gauge_hatch: f32,
    /// 容量条图例前那一小块颜色的边长（设计稿 `.legend i`）。
    pub legend_swatch: f32,
    /// 那一小块颜色的圆角（设计稿 `.legend i`）。
    pub legend_swatch_radius: f32,
    /// 规则行首那枚序号圆的直径（设计稿 `.rule .rn`）。
    pub rule_badge: f32,
    /// 子库屏空态那张卡的内边距（设计稿空态卡的 `padding`）。
    pub empty_card_padding: f32,
    /// 提示条离窗口底边多远（设计稿 `.toast` 的 `bottom`）。
    pub toast_bottom: f32,
    /// 提示条四边留白：`[上, 右, 下, 左]`（设计稿 `.toast` 的 `padding`）。
    pub toast_padding: [i8; 4],
    /// 提示条上那颗按钮的描边是字色的几成（设计稿 `.toast .btn` 的描边）。
    pub toast_button_line: f32,
    /// 只有一句话的提示条停多久，秒（设计稿 `toast()`）。
    pub toast_seconds: f32,
    /// 带一颗按钮的提示条停多久，秒（设计稿 `toast()`）。
    pub toast_action_seconds: f32,
    /// 弹层里「会怎样」那几条行首圆点的直径（设计稿 `.impact li::before`）。
    pub impact_dot: f32,
    /// 行首圆点那一列多宽（设计稿 `.impact li`）。
    pub impact_column: f32,
    /// 右侧抽屉的宽：待确认屏的裁决记录（设计稿 `.drawer`）。
    pub drawer_width: f32,
    /// 分段开关外框与里头按钮之间（设计稿 `.seg` 的 `padding`）。
    pub seg_padding: f32,
    /// 分段开关里一颗的高（设计稿 `.seg button`）。
    pub seg_button_height: f32,
    /// 分段开关里一颗的左右留白（设计稿 `.seg button`）。
    pub seg_button_padding: f32,
    /// 待确认屏空态那张卡最宽多宽（设计稿 `#q-empty .card` 的 `max-width`）。
    pub empty_state_width: f32,
    /// 一批变体展开之后左「细分」右「随机样本」两栏的宽比（设计稿 `.bbody`）。
    pub batch_body_columns: [f32; 2],
    /// 一批变体卡头上条数那一列的宽（设计稿 `.bhead`）。
    pub batch_count_width: f32,
    /// 卡头最右折叠标那一列的宽（设计稿 `.bhead`）。
    pub batch_chevron_column: f32,
    /// 折叠标那个折角的边长（设计稿 `.chev`）。
    pub chevron: f32,
    /// 折叠标那两道线的宽（设计稿 `.chev` 的 `border`）。
    pub chevron_stroke: f32,
    /// 随机样本一行中间那枚箭头那一列的宽（设计稿 `.smp`）。
    pub sample_arrow_column: f32,
    /// 候选卡片里「来源 / 匹配 / 依据」那一列的宽（设计稿 `.cand dl`）。
    pub candidate_key_width: f32,
    /// 选中那张候选卡片外头那一圈强调浅色的宽（设计稿 `.cand[aria-selected]`）。
    pub candidate_ring: f32,
    /// 键帽底边那一道的宽（设计稿 `.kbd` 的 `border-bottom-width`）。
    pub kbd_bottom: f32,
    /// 中文离线源那一堆里字段名那一列的宽（设计稿 `.mgroup dl`）。
    pub match_key_width: f32,
    /// 警示框内边距 `[上下, 左右]`（设计稿 `.warnbox` 的 `padding`）。
    pub warn_box_padding: [f32; 2],
    /// 能力档案平台表「平台 / 不能用时 / 覆盖」三列的宽；「设备直接能用」占余下的（设计稿 `DLG.subform` 的表头）。
    pub platform_table_columns: [f32; 3],
    /// 目标设置里「自定义」容量上限那一格的宽（设计稿 `DLG.subform`）。
    pub capacity_input_width: f32,
    /// 手动例外表「平台 / 体积 / 时间 / 撤销」四列的宽；「作品」与「备注」分余下的（设计稿 `DLG.excl` 的表头）。
    pub exception_table_columns: [f32; 4],
    /// 手动例外弹层里「备注（可选）」那一格的宽（设计稿 `DLG.excl`）。
    pub exception_note_width: f32,
    /// 单选圆点的直径（设计稿 `.opt input`）。
    pub radio_diameter: f32,
    /// 选中时正中那一粒的直径。
    pub radio_dot: f32,
    /// 选中时圆心与外圈之间那一道缝的宽。
    pub radio_gap: f32,
    /// 单选那一行圆点与名字之间（设计稿 `.opt` 的 `gap`）。
    pub option_gap: f32,
    /// 单选那一行上下留白（设计稿 `.opt` 的 `padding`）。
    pub option_padding: f32,
    /// 作品详情页六个面那一排一格的高（设计稿 `.tabs button`）。
    pub tab_height: f32,
    /// 选中那一面底下那道强调色线的粗（设计稿 `.tabs button[aria-selected]`）。
    pub tab_underline: f32,
    /// 作品详情页元数据那一面一行里 `[名那一列, 动作那一列]` 的宽（设计稿 `.mrow`）。
    pub meta_row_columns: [f32; 2],
    /// 来源标签的高（设计稿 `.srcb`）。
    pub source_badge_height: f32,
    /// 编辑态下那一排里一个的高（设计稿 `.pickchip`）。
    pub pick_chip_height: f32,
    /// 那一个里值最宽摆多少，再长截尾巴（设计稿 `.pickchip b`）。
    pub pick_chip_max: f32,
    /// 编辑态下简介那一框几行高（设计稿 `metaTab` 里 `textarea` 的 `rows`）。
    pub meta_textarea_rows: usize,
    /// 添加一个名称那一排两个下拉的宽：`[语言, 类型]`（设计稿 `.tadd`）。
    pub title_add_columns: [f32; 2],
    /// 作品详情页媒体那一面一格最窄（设计稿 `.mgrid` 的 `minmax`）。
    pub media_tile_min: f32,
    /// 没有 ffmpeg 那一块斜纹一道多宽（设计稿 `.noff` 的斜纹）。
    pub noff_stripe: f32,
    /// 作品详情页头上封面那一格的宽（设计稿 `.hero`）。
    pub hero_cover_width: f32,
    /// 头上那几格事实整排最宽（设计稿 `.hfacts` 的 `max-width`）。
    pub hero_facts_max: f32,
    /// 头上那几格事实一排几格（设计稿 `.hfacts`）。
    pub hero_facts_columns: usize,
    /// 头上那张字卡的水印伸出格子多少：`[右, 下]`（设计稿 `.tc-wm`）。
    pub hero_card_mark_offset: [f32; 2],
    /// 头上那张字卡的标题至多几行（设计稿 `.tc-t`）。
    pub hero_card_title_rows: usize,
    /// 概览里媒体那一块摆几格（设计稿 `.mstrip`）。
    pub media_strip_columns: usize,
    /// 概览左右两栏的比例：`[左, 右]`（设计稿 `.ov`）。
    pub overview_columns: [f32; 2],
    /// 变体卡身子左右两半的比例：`[左, 右]`（设计稿 `.vbody`）。
    pub work_card_columns: [f32; 2],
    /// 元数据那一面简介至多几行（设计稿 `.mrow .v.clamp`）。
    pub meta_clamp_rows: usize,
    /// 侧边详情头上那张字卡的标题至多几行（设计稿 `.dcover .tc-t`）。
    pub card_title_rows: usize,
    /// 字卡副行至多几行（设计稿 `.tc-s`）。
    pub card_subtitle_rows: usize,
    /// 标题面那张表「名称」一列占表宽几成，其余几列均摊、「隐藏」贴右。
    pub title_name_share: f32,
    /// 设置屏左边那一列的宽（设计稿 `.sets` 的 `grid-template-columns`）。
    pub settings_nav_width: f32,
    /// 设置屏左边那一列一节的高（设计稿 `.sets nav button` 的 `height`）。
    pub settings_nav_height: f32,
    /// 设置屏右边内容里名那一列的宽（设计稿 `.srow` 的 `grid-template-columns`）。
    pub settings_row_label: f32,
    /// 开关那个小滑块：`[外框宽, 外框高, 里头圆点的直径]`（设计稿 `.switch i` 与 `.switch i::after`）。
    pub settings_switch: [f32; 3],
    /// 设置屏「主库原名」那一格输入框的宽。设计稿上没有这一格，取这一档的理由写在令牌文件那一行。
    pub settings_name_width: f32,
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
    /// 按不动的控件淡到几成（设计稿 `.btn[disabled]` 的 `opacity`）。
    pub disabled_opacity: f32,
    /// 编辑态下改过的那一行底色里调进几成强调色（设计稿 `.mrow.dirty`）。
    pub dirty_row_tint: f32,
    /// 作品详情页头上那一块底色里调进几成平台色（设计稿 `.hero`）。
    pub hero_tint: f32,
    /// 「细分」底下那条占比条上填的那一截淡到几成（设计稿 `.dist .b i` 的 `opacity`）。
    pub dist_bar_opacity: f32,
    /// 下钻到某一组之后就地那一框底色里调进几成强调色（设计稿 `.drill` 的 `color-mix`）。
    pub drill_tint: f32,
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
        assert_eq!(tokens.font.size_caption_plus, 11.5);
        assert_eq!(tokens.font.size_small_plus, 12.5);
        assert_eq!(tokens.layout.dialog_width, [520.0, 620.0, 720.0, 840.0]);
        // 设计稿 `.statusbar .mini .bar{width:120px}`。
        assert_eq!(tokens.layout.statusbar_bar, 120.0);
        // 设计稿 `.runcard{box-shadow:inset 3px 0 0 var(--accent)}`。
        assert_eq!(tokens.layout.runcard_bar, 3.0);
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
        // 屏头、屏体（票 `gui-looks-like-the-design/32`），与任务屏（票 `gui-looks-like-the-design/25`）的块与块、卡片、表格、空态的留白。
        assert_eq!(
            tokens.space.screen_header_padding,
            [14.0, 20.0],
            "设计稿 .scrhead"
        );
        assert_eq!(
            tokens.space.screen_body_padding,
            [18.0, 20.0, 28.0],
            "设计稿 .scrbody"
        );
        assert_eq!(
            tokens.space.screen_section_gap, 18.0,
            "设计稿任务屏 .scrbody.col"
        );
        assert_eq!(tokens.space.section_title_gap, 8.0, "设计稿任务屏 .col");
        assert_eq!(tokens.space.card_padding, 16.0, "设计稿 .runcard");
        assert_eq!(tokens.space.card_row_gap, 8.0, "设计稿 .runcard");
        assert_eq!(
            tokens.space.queue_row_padding,
            [10.0, 14.0],
            "设计稿等待中那一行"
        );
        assert_eq!(tokens.space.empty_padding, 28.0, "设计稿 .empty");
        assert_eq!(tokens.space.meta_gap, 18.0, "设计稿 .runcard .meta");
        assert_eq!(
            tokens.space.table_head_padding,
            [7.0, 10.0],
            "设计稿 .tbl th"
        );
        assert_eq!(tokens.space.cell_padding, [8.0, 10.0], "设计稿 .tbl td");
        assert_eq!(tokens.layout.runcard_progress, 8.0, "设计稿 .runcard .bar");
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
