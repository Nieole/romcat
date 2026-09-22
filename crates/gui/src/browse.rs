//! **浏览屏**：找到这一批，然后对它施加操作。
//!
//! ## 主列表一个游戏一行
//!
//! 真库上 46,444 个变体收敛成一万出头的行——**元数据本来就锚在作品这一层**，所以这个
//! 粒度与数据模型天然对齐。六成作品下面挂着不止一个变体，它们在详情面板里挑。
//! 收敛在中立库里做（[`romcat_core::catalog::browse`]），这一层只画和转发（ADR-0005）。
//!
//! **认不出作品的变体自成一行**，与导出那一侧同一条口径（`adapter::converge` 的
//! `Anchor::Loose`）：真库上那是一多半，折进「未知」那一行等于让人看不见自己一半的库。
//!
//! ## 这一屏要回答的三个问题
//!
//! 1. **我有哪些游戏**——中间那张表，虚拟化，十万行滚起来的代价与总行数无关。
//! 2. **我要找的那一批在哪**——左边那一栏（票 `gui-looks-like-the-design/09` 照稿排）。几簇带着条数的
//!    分面标签：**平台**、**中文**、**识别结论**、**收藏与合集**、**语言**；一棵可嵌套的**条件组**
//!    （[`crate::filter`]），三种组合方式、九个运算符。**两套之间是且**，一律下推到中立库的
//!    `WHERE`，内存里永远只有当前视口那几十行。
//!
//!    **那棵条件组就是子库的规则**：筛到满意按「存成子库」，条件原样变成那个子库的
//!    规则（[`WorkQuery::to_rule`]）；反过来子库屏点「改选择」跳回来，规则预填进筛选器
//!    （[`Screen::begin_editing`]），调完按「更新到子库」原样换回去
//!    （[`Screen::update_sublibrary`]）。**例外也在这一趟里加减**——「哪一份」只有在
//!    详情面板里才指得准（[`Screen::set_exception`]）。
//!
//!    左栏顶上还有一个**搜索框**（票 `gui-redesign/05`）。**它与筛选器不是一类
//!    东西**：筛选器管集合，它管**顺序**——打几个字，匹配得好的排前面，权重内置、
//!    不用配。三条路都找：屏上这个名字、**标题集合**里别的叫法（中文名就在这儿）、
//!    **简介**；命中在哪一条决定这一行排哪一档（`SearchHit`，折在中立库那一层）。
//!    **它进不了子库的规则**——子库要的是集合不是顺序，所以搜索框里还有字的时候
//!    「存成子库」当场挡住（挂单 Q70）。
//! 3. **这一行到底是什么**——右边那块面板：**作品** → **变体**（每个带置信度与
//!    **依据**）→ **媒体**（封面与截图内嵌画出来，视频是一张抽出来的首帧加一个播放标，
//!    [`crate::media`]）。一部作品的全部情况——变体与文件、元数据、标题、媒体、识别依据——在
//!    **作品详情页**（[`work`]，双击一行或点「查看详情」打开，票 `gui-looks-like-the-design/15`）。
//!
//! ## 收藏与合集：同一套成员关系
//!
//! **收藏是一颗星**：勾几行按一下，那一批底下的变体全进去；`收藏=是` 随后筛得出来。
//! 它走的是**合集**那套成员关系（[`romcat_core::collection`]），所以同一批按钮顺带
//! 给出自建合集（「通关过的」「送朋友的」），按 `合集=某某` 筛。
//!
//! 领域判断一条都不在这里（ADR-0005）：落沉淀库、挑哪种锚、投影回中立库，全在核心库
//! 那个模块里。这一层只把「勾中的那一批」交过去，再把它交回来的那本账原样印出来
//! ——**其中几个只钉得住本机路径、挪了位置会飘，屏上照直写**。
//!
//! ## 选中语义：这一屏要钉死的东西
//!
//! - 选中主列表的行 ＝ 选中这些**作品**，批量操作作用于它们的变体
//!   （[`Catalog::scoped_variants`]，随当前筛选收窄；不筛的时候就是全部变体）。
//! - 在详情面板里选中某一个**变体** ＝ 变体级的操作只作用于它：改选择那一趟里给它记一条
//!   例外、看它凭什么落在这一档。
//!
//! 两件事各有各的状态（[`Picked`] 与 [`Screen::variant_key`]），混成一件的话，
//! 翻着看就会把批量操作的范围改掉。
//!
//! ## 所有元数据编辑收敛在这里（ADR-0001 的修订段）
//!
//! 主库的 Pegasus 文件不再是编辑入口。于是这一屏必须真的改得动库：加一条**裁决**来源的
//! 叫法、隐藏一条叫法、指定**首选变体**、手动改写一个字段值。这几件事都在**作品详情页**上做
//! （[`work`]；浏览屏底下那块编辑面板拆掉了，收挂单 `Q804`），各自都只是一次写入——领域判断一条都不在这里。
//!
//! ## 中文输入不进虚拟化的区域
//!
//! 一个 [`egui::TextEdit`] 都不进表格单元格：表格是虚拟化的，正在组字的那一行一旦滚出
//! 视口，那个控件就不存在了（ADR-0005 的修订段）。文本框只在不虚拟化的那几处：左栏、右栏选中那张
//! 变体卡底下的例外备注、作品详情页的元数据与标题两面。
//!
//! 左边那一栏上半的五个维度是**选**出来的不是打出来的，值从中立库现问
//! （[`Catalog::facets`]）；下半那棵条件组里每条子句有一个值要打，而**那一栏不虚拟化**
//! ——一个 `ScrollArea` 把里面每一行都画出来，正在组字的那一行不会凭空消失。
//! 这条界线不是「表格 vs 面板」，是**虚拟化 vs 不虚拟化**。

use std::collections::BTreeMap;

use egui::{Align, Layout};
use romcat_core::catalog::CatalogError;
use romcat_core::catalog::browse::{
    Facets, NON_GAME_ASSET_LABEL, NonGameAssets, PlatformFilter, Scope, WorkAnchor, WorkDetail,
    WorkOrder, WorkQuery, WorkVariant,
};
use romcat_core::catalog::identify::{NOT_RUN_LABEL, Tier};
use romcat_core::catalog::{Catalog, VariantDetail};
use romcat_core::collection::{self, Applied, FAVORITE};
use romcat_core::filename::Rules;
use romcat_core::report::{human_bytes, thousands};
use romcat_core::scrape::Priorities;
use romcat_core::scrape::pool::MediaPool;
use romcat_core::scrape::priority::VERDICT;
use romcat_core::site::Site;
use romcat_core::stage::Stage;
use romcat_core::sublibrary::{
    BrokenRule, Dimension, Discarded, Exception, ExceptionRow, LoadedSelection, Rule, Sublibrary,
};
use romcat_core::task::{Cutoff, Ending, Finished};
use romcat_core::title::{self, Language, TitleKind};
use romcat_core::verdict::TitleSuppression;

use crate::filter::Filter;
use crate::font;
use crate::layout;
use crate::look;
use crate::media::{Gallery, Shelf};
use crate::scrape;
use crate::table::{Picked, SPAN, Table, UNLINKED_LABEL, Window, tail_fit, unlinked_title};
use crate::task::{Product, Tasks};
use crate::toast::{self, Toast};
use crate::tokens::Tokens;

pub mod menu;
pub mod merge;
pub mod work;

/// 搜索框那个认得出的 id：`⌘/Ctrl+F` 要把光标放进去，得先叫得出它的名字。
fn 搜索框() -> egui::Id {
    egui::Id::new("浏览屏搜索框")
}

/// 侧边详情变体卡片上**首选变体**那一枚标签上的字（词表**首选变体**条）。哪一个是首选由核心库答
/// （[`VariantDetail::preferred_now`]），这里只是屏上怎么写。
const PREFERRED_TAG: &str = "首选变体";

/// 浏览屏两种不改变集合的呈现方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowseView {
    Table,
    Cards,
}

/// 卡片视图的三档封面宽度。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CardSize {
    Small,
    Medium,
    Large,
}

/// 卡片工具条那个排序下拉里，一档叫什么（设计稿 `#csort`）。
///
/// **`None` 是「默认」那一档，不是某一列**（挂单 `Q1098`，拿主意的人 2026-09-21 定照稿
/// 拆回两个）：稿上「默认」与「名称」本来就是两个选项，票 09 把它们合成了一个，于是
/// `(作品, 倒着)` 在这个下拉上照样显示「默认」——而那一刻库里排的并不是默认那一种。
///
/// 两档的差别是**方向**：「默认」把排法整个按回 [`WorkQuery::default`]（作品、正着），
/// 「名称」只把列换成作品、方向照旧。所以从表头倒着排过来的人选「名称」还是倒着的，
/// 选「默认」才回到正着。
fn card_order_label(order: Option<WorkOrder>) -> &'static str {
    match order {
        None => "默认",
        Some(WorkOrder::Name) => "名称",
        Some(other) => other.label(),
    }
}

impl CardSize {
    fn width(self) -> f32 {
        Tokens::builtin().layout.card_widths[match self {
            Self::Small => 0,
            Self::Medium => 1,
            Self::Large => 2,
        }]
    }
}

/// 卡面右上角的身份叠层与底边置信度线。它们压在封面上，而不是占用信息区；这是卡片
/// 能先被视觉扫描、再读文字的关键层次。
fn paint_card_overlay(
    ui: &egui::Ui,
    card: egui::Rect,
    cover: egui::Vec2,
    cover_radius: u8,
    row: &romcat_core::catalog::browse::WorkRow,
    chosen: bool,
    focused: bool,
) {
    let cover = egui::Rect::from_min_size(card.min, cover);
    let painter = ui.painter_at(card);
    let tokens = Tokens::builtin();
    let platform = row.platforms.first().map_or("未知", String::as_str);
    let color = tokens.color.platform.of(platform);
    let font = egui::FontId::new(tokens.font.size_caption_plus, font::strong_family());
    let galley = painter.layout_no_wrap(platform.to_owned(), font.clone(), egui::Color32::WHITE);
    let badge = egui::Rect::from_min_size(
        egui::pos2(cover.right() - galley.size().x - 14.0, cover.top() + 8.0),
        galley.size() + egui::vec2(12.0, 6.0),
    );
    painter.rect_filled(badge, tokens.radius.small, color);
    painter.galley(
        badge.center() - galley.size() / 2.0,
        galley,
        egui::Color32::WHITE,
    );
    if !row.chinese.is_empty() {
        let text = row.chinese.join(" / ");
        let galley = painter.layout_no_wrap(
            text,
            egui::FontId::proportional(tokens.font.size_caption_plus),
            ui.visuals().strong_text_color(),
        );
        let badge = egui::Rect::from_min_size(
            egui::pos2(cover.right() - galley.size().x - 14.0, cover.top() + 34.0),
            galley.size() + egui::vec2(12.0, 6.0),
        );
        painter.rect_filled(
            badge,
            tokens.radius.small,
            ui.visuals().window_fill.gamma_multiply(0.85),
        );
        painter.galley(
            badge.center() - galley.size() / 2.0,
            galley,
            ui.visuals().strong_text_color(),
        );
    }
    // 设计稿里的 `.cv-tier` 是被封面圆角裁掉的 3px 色带，不是一条另起圆角的横线。
    // 先用整张封面画圆角，再只留下最下方那一带，才会和有/无封面两种卡面的圆角严丝合缝。
    let tier_band = egui::Rect::from_min_max(
        egui::pos2(cover.left(), cover.bottom() - tokens.layout.tier_bar),
        cover.right_bottom(),
    );
    painter.with_clip_rect(tier_band).rect_filled(
        cover,
        cover_radius,
        look::tier_color(row.tier(), ui.visuals()),
    );
    if chosen || focused {
        // 选中态和键盘焦点都只落在 `.cover`，且只画一圈：整卡焦点框和双层光晕会把
        // 信息区误认成卡面的一部分。
        painter.rect_stroke(
            cover,
            cover_radius,
            ui.visuals().selection.stroke,
            // 卡面贴着分配区边缘；往外画会被裁掉三条边。描在卡面内侧才能完整围住它。
            egui::StrokeKind::Inside,
        );
    }
}

/// 界面上人工写下的叫法，**依据**里写这一句。
///
/// 没有依据的结论事后无法复核（ADR-0002）。人工写的那条依据只能是「谁在哪儿写的」，
/// 但那也比空着强——半年后看见一个来路不明的中文名，至少知道它是自己敲的。
const HAND_WRITTEN: &str = "作品详情页上人工写的";

/// 底下那块面板里，**标题集合**最多列几条。
///
/// 一部作品的叫法在真库里能攒到几十条（每个源一条、每种语言一条），而面板上那一栏
/// 只有半屏高。列到这儿打住，末尾说清还有多少条没列。
const TOP_TITLES: usize = 24;

/// 面板上**文件成员**最多列几条。
///
/// 与标题分开定：一个变体可以是**一整个目录**（`CONTEXT.md` 的「变体」词条），
/// PSV 那批目录树转储一个变体底下就是上千个文件（真库上最大的一份有 21,436 个）。
/// 这个数管的是「扫一眼看得完」，与「一部作品有几个叫法」不是同一件事，
/// 共用一个常量迟早会为了一边把另一边调坏。
const TOP_MEMBERS: usize = 40;

/// 详情面板上**候选的依据**一个变体最多列几条。
///
/// 一个变体可以撞上好几条 DAT 记录（真库上多候选那批有 3,584 个），而这一栏是给人
/// 「判断得出哪个版本更可信」用的，不是给人读完的。
const TOP_CANDIDATES: usize = 8;

/// 刮削字段值那一列，一行最多画多少个字。
///
/// **这是画法，不是产品决定**：一条简介在中立库里最多 4,000 字
/// （`scrape::zh::DESCRIPTION_LIMIT`），而这一行画在一条**横排**里——横排不折行，
/// 4,000 个汉字就是四五万像素宽的一行，面板会被它撑出一条横向滚动条，那份清单也就
/// 滚不动了。原文一个字都没动，整段挂在悬停里。
const VALUE_SHOWN: usize = 60;

/// 把一个字段值收成**一行画得下的那一截**。
///
/// 返回 `None` 表示原样画得下，不必动它。两件事都要管：
///
/// - **换行**。数据源的排版原样留在值里（规格 18），可横排里一个换行就把那一行撑高。
/// - **长度**。超过 `VALUE_SHOWN` 个字就用省略号收住。
///
/// **原文一个字都没动**——收窄的只是画出来的那一行，整段挂在悬停里。
///
/// 它是公开的，因为「原样画得下的**一个字都不动**」这一条只有从**函数这一侧**看得见：
/// 屏上画的是同一串字，中间换没换过一份字符串出去，看画出来的那一帧看不出来。
///
/// 三样都钉在 `crates/gui/tests/browse.rs` 的 `一行画得下的值原样不动_带换行的折平_顶到闸上的收住` 上。浏览屏底下
/// 那块编辑面板拆掉之后（票 `gui-looks-like-the-design/15`），拿它画刮削来的值的是待确认屏；作品详情页上的值折行画全。
#[must_use]
pub fn one_line(value: &str) -> Option<String> {
    let flat = value.replace(['\n', '\r'], " ");
    if flat.chars().count() > VALUE_SHOWN {
        let head: String = flat.chars().take(VALUE_SHOWN).collect();
        return Some(format!("{head}…"));
    }
    (flat != value).then_some(flat)
}

/// 加一条叫法时界面上那份草稿。
#[derive(Debug, Clone)]
pub struct TitleDraft {
    /// 这一串字。**唯一会碰到输入法的地方**，所以它在详情面板里。
    pub value: String,
    /// 哪种语言。
    pub language: Language,
    /// 哪一种叫法。
    pub kind: TitleKind,
}

impl Default for TitleDraft {
    fn default() -> Self {
        Self {
            value: String::new(),
            // 人在这儿手敲的绝大多数是中文译名——那正是这个项目缺的那一半。
            language: Language::Chinese,
            kind: TitleKind::Translated,
        }
    }
}

/// 「**改选择**」跳过来之后，这一屏正在改的是哪个子库。
///
/// 子库屏管「送到哪」、浏览屏管「选什么」（票 `gui-redesign/11`）。跳过来时那个子库的
/// 规则已经预填进筛选器，人在这儿改的时候**看得见它真的筛出了什么**；调完按
/// 「更新到子库」原样带回去。
///
/// **例外也在这儿加减**：规则表达不了的个人口味落在某一个变体上（ADR-0016），
/// 而「某一个变体」只有在详情面板里才指得准。
///
/// **读不懂的那几条规则也在这儿处置**（票 `gui-redesign/14`）：它们搬不进筛选器
/// ——那是一棵读得懂的树，一条读不回来的原文在里头没有位置——所以原样摆在筛选栏顶上
/// 那条横幅里（`Screen::broken_rules_ui`），只给一个「扔掉这条」。
/// 子库屏照旧一个写的动作都没有（票 `gui-redesign/11`）。
#[derive(Debug, Clone, Default)]
pub struct Editing {
    /// 改的是哪个子库。
    pub sublibrary: String,
    /// 这个子库**读不懂**的那几条规则，原样带过来的。
    ///
    /// 它们没参与求值，「更新到子库」也不会碰它们（`Catalog::replace_rules` 只换
    /// 读得懂的那几条）；这一栏给的是另一条路——[`Screen::discard_broken_rule`]。
    pub broken: Vec<BrokenRule>,
    /// 这个子库眼下的例外：变体的键 → 方向与那句话。
    pub exceptions: BTreeMap<String, ExceptionRow>,
    /// 记一条例外时写的那句话。**口味半年后就想不起来了**，留一句话的位置。
    ///
    /// 它在详情面板里，那儿不虚拟化——**唯一会碰到输入法的位置**（ADR-0005 的修订段）。
    pub note: String,
    /// 子库屏规则行上「✎」跳过来的：只改这个子库的**第几条**规则，「更新到子库」只换回这一条
    /// （`Catalog::replace_rule`，票 `gui-looks-like-the-design/20`）。`None` 是「改选择」那种整批改。
    pub ordinal: Option<i64>,
}

/// 「**存成子库**」那两个格子。
///
/// 前端格式与容量上限**不在这儿**：这一栏管的是「把这批选中存下来」，
/// 那两样是设备的属性，去子库屏调。
#[derive(Debug, Clone, Default)]
pub struct SaveDraft {
    /// 子库叫什么。一台目标设备一个。
    pub name: String,
    /// 目标设备上的子库根。**卡不在位也存得下**——子库是持久实体。
    pub target: String,
}

/// 浏览屏。
pub struct Screen {
    window: Window,
    /// 卡片墙的虚拟窗口；仅封面模式只改它，不污染主选择集。
    card_window: Window,
    /// 卡片工具栏的覆盖率统计。它永远只取有封面的作品，因而不能复用会受开关影响的卡片窗。
    cover_window: Window,
    /// 卡片墙滚到哪一组就把哪一组钉在网格上沿；避免每一排重复一遍平台标题。
    card_group_header: Option<(String, u64)>,
    /// 筛选与排序。**界面上这一份是源头**，[`Window`] 里那一份是它的副本，每帧同步一次。
    query: WorkQuery,
    /// 五个维度各有哪些值可选。换库或改过元数据才重问。
    facets: Facets,
    /// [`Self::facets`] 是照开关拨在哪一档问的。与眼下那一档不一样就重问（[`Self::sync_window`]）。
    facets_for: Option<NonGameAssets>,
    /// **筛选器**：那棵可嵌套的条件组。它折出来的规则每帧同步进 [`Self::query`]。
    filter: Filter,
    /// **当前筛选下一共多少个变体**。`None` 是数不出来，不是零。
    ///
    /// 与 [`Self::scope`] 不是一个数：这一个是**筛出来的全部**，那一个是**选中的那几行
    /// 展开出来的**。屏上两个都写，因为按批量操作之前要分得清「筛出来多少」与
    /// 「我勾了多少」。
    filtered: Option<u64>,
    /// **库里一共几个作品**：什么都不筛时主列表有几行（非游戏资产照开关那一档算）。顶栏那句
    /// 「N 个作品（共 M）」的 M，照稿。`None` 是数不出来，不是零。
    all_works: Option<u64>,
    /// **当前筛选下整行都是非游戏资产的有几行**（[`Catalog::non_game_asset_rows`]）：
    /// 收起时屏上说「收起了几个」，列出时说「列出了几个」。`None` 是数不出来，不是零。
    ///
    /// 数是核心库数的、哪几行算也是核心库判的（ADR-0024）：这一层只把开关拨到哪一档交过去。
    non_game_assets: Option<u64>,
    /// 「存成子库」那两个格子：名字与目标路径。
    save: SaveDraft,
    /// **刮削面板**：屏头那颗「刮削…」摊开的就是它（票 `gui-redesign/10`）。
    ///
    /// 它住在这一屏里而不是自成一屏，是因为它的**范围**就是这一屏筛出来的那一批——
    /// 挪到别处去，那批东西就得再传一遍，而传着传着两边的数就对不上了。
    scrape: scrape::Panel,
    /// **成型纠正**那一层（票 `gui-looks-like-the-design/29`）：作品详情「变体」那一面
    /// 「调整成型…」与「撤销成型纠正」开的就是它。
    fixer: crate::shaping::Fixer,
    /// 刚落过一笔人工纠正、等窗口去库屏那一处排一趟重新成型（[`Screen::take_reshaped`]）。
    ///
    /// **全窗口只有库屏那一处排它**：两处各排一趟的话，报告会有两份各说各的。
    reshaped: bool,
    /// 「**改选择**」跳过来了，正在改这个子库的选择集。`None` 是平常的浏览。
    editing: Option<Editing>,
    /// 「更新到子库」按完了，等窗口把人送回子库屏（[`crate::app::App::route`]）。
    returned: Option<String>,
    /// 这一屏刚**动过哪个子库的选择集**（换规则、记例外、撤例外），等窗口转告子库屏。
    ///
    /// 与 [`Self::returned`] 不是一回事：那一个说「人要回去了」，这一个说
    /// 「那台设备缓着的账过期了」。例外是**一按就落库**的，而人可以按完例外就点
    /// 「不改了」、或者直接从顶栏切回子库屏——那两条路上没有「更新到子库」，
    /// 只认 `returned` 的话，子库屏会摆着一份按旧选择集排出来的差量，而「同步」认的正是它。
    touched: Option<String>,
    /// **合并作品向导**开着时是 `Some`（票 `gui-looks-like-the-design/16`）。
    merging: Option<merge::Wizard>,
    /// **移出此作品**那一层开着时是 `Some`。它从作品详情页某一张变体卡上开。
    splitting: Option<merge::Split>,
    /// 刚落下的那一**批裁决**是第几批：底边那条提示条上那颗「撤销」撤的就是它
    /// （设计稿 `doMerge` / `doSplit` 末尾那一下）。提示条停够了就跟着没了——
    /// 之后照旧走待确认屏的**裁决记录**。
    just_landed: Option<i64>,
    /// 高亮的是哪一行（全序下标）。
    focused: Option<u64>,
    /// 选中了哪几行——**批量操作的作用范围**。与 [`Self::focused`] 不是一回事。
    picked: Picked,
    /// 点开的那一行是谁。**记身份不记下标**：换个筛选表就重排了，下标会指到别人身上。
    opened: Option<WorkAnchor>,
    /// 点开那一行的详情：作品 → 变体。
    work: Option<WorkDetail>,
    /// [`Self::work`] 底下每个变体的**变体简称**，次序与 `work.variants` 一样。核心库拼的
    /// （`Catalog::variant_short_names`），点开那一行时问一次，卡片照印。
    short_names: Vec<String>,
    /// 详情面板里**选中的那一个变体**。变体级的操作只作用于它。
    variant: Option<String>,
    /// 选中那个变体的详情：文件、媒体、标题集合、首选变体、刮削字段。
    detail: Option<VariantDetail>,
    /// 眼下这份选中展开成多少个变体。**批量操作按下去会动这么多**。
    ///
    /// `None` 是**数不出来**（读库出的错在 [`Self::error`] 里），不是零——
    /// 屏上写着「作用于 0 个变体」而按下去真会动一百多个，那正是这一栏要防的事。
    scope: Option<u64>,
    /// [`Self::scope`] 是照哪一份选中数出来的。与眼下这份不一样就重数一遍。
    ///
    /// **不拿一个 `dirty` 标记**：选中是在表格里点的，也可以由实测与测试直接改
    /// （[`Self::picked_mut`]），标记会漏掉后一条路——而漏掉的后果是屏上写着
    /// 「作用于 0 个变体」，人却按下了一个真会动 185 个的按钮。
    scoped: Option<Picked>,
    /// 挑**显示标题**用的优先级表。与导出走同一份，于是面板上写着的就是导出会写的。
    priorities: Priorities,
    /// **剥离规则**：认不出作品的那一行拿它剥正题（`WorkRow::title`）。
    ///
    /// 读**工作目录里那份**（`sources::rules`，刮削与命令行同一条查法）：屏上那一行写的
    /// 正题，就是刮削撞中文离线源时剥出来的那一个。
    rules: Rules,
    /// **媒体池**：查「这张图在不在」用它。池子整个不在位时是 `None`——
    /// 那时面板如实说「没查池子」，而不是报一句「一张都没有」。
    pool: Option<MediaPool>,
    /// 详情面板那几格**缩略图**（票 `gui-redesign/07`）。解码与抽首帧全在核心库，
    /// 这里只握着一条后台线程的把手与传上显卡的那几张纹理（[`crate::media`]）。
    gallery: Gallery,
    /// 「**在每行开头显示封面**」那颗开关（票 `gui-looks-like-the-design/09`）。默认关着。
    list_covers: bool,
    /// 浏览屏的呈现方式；只影响界面，不参与子库的选择集规则。
    view: BrowseView,
    /// 卡片的大小与是否按平台分段，同样只是视图偏好。
    card_size: CardSize,
    group_cards: bool,
    only_covers: bool,
    /// 主列表行首那几格封面：问过的、解着的（[`Shelf`]）。与 [`Self::gallery`] 分开一份。
    shelf: Shelf,
    /// 左栏平台那一簇**摊没摊开**：没摊开时只摆头几个（令牌 `platforms-visible`），其余收在
    /// 「更多（N）」后头（设计稿 `#more-plat`）。
    more_platforms: bool,
    /// 点开那一行的**封面**，侧边详情头上那一格贴它：核心库挑的（`Catalog::cover_of`，与表上
    /// 那一行行首问的是同一处），点开一行时问一次。`None` 是这一行一张封面都没有——那时画字卡。
    cover: Option<romcat_core::catalog::detail::MediaItem>,
    /// **合集**那个格子：往哪个合集里加、从哪个合集里拿。收藏不用它——那一组的名字
    /// 是定死的（[`FAVORITE`]）。
    collection: String,
    /// 详情面板里选中那个变体**在哪几个合集里，各钉在哪种锚上**。
    ///
    /// 读的是**沉淀库**不是投影：投影那张表只记「在不在里面」，记不着它靠什么认出来的，
    /// 而「挪了位置会不会飘」这句话的依据恰恰是后者。
    standing: Vec<(String, &'static str)>,
    /// [`Self::standing`] 是照哪个变体算的。与眼下选中的那个不一样就重算。
    standing_for: Option<String>,
    /// 选中那个变体所属的**作品**上，眼下**压掉的叫法**有哪几条（票 `parking-3/13`）。
    ///
    /// 读的是**沉淀库**：那是人的动作住的地方，中立库删掉重扫也不丢。面板要靠它把
    /// 「**压掉了**」与「**没采到**」分开说——两者对维护者是不同的意思。
    suppressed: Vec<TitleSuppression>,
    /// [`Self::suppressed`] 是照哪个作品读的。与眼下这个不一样就重读。
    suppressed_for: Option<String>,
    /// 台上那趟**整批收藏**（或者自建合集的加减）是第几号。`None` 是眼下没排着。
    ///
    /// **按号认领**，与别的屏一个写法：台上跑的可能是别人排的活。
    collecting: Option<u64>,
    /// 加一条叫法的草稿。
    title_draft: TitleDraft,
    /// 上一次动作的回执。
    notice: Option<String>,
    /// **刚撤掉一条压制**那一句，连它旁边那颗就地的「折标题」。
    ///
    /// **与 [`Self::notice`] 分开一格**，因为它带着一颗按钮：撤掉一条压制这一下唯一的
    /// 下文就是折一趟标题（那条叫法要等下一趟重折才回得来），而别处那些回执没有下文
    /// ——合成一格的话，那颗「折标题」会挂在「首选变体裁给了 X」这类回执旁边。
    /// 换一个作品就收掉（[`Self::sync_suppressed`]）：那时人已经不在刚才那条叫法上了。
    lift_notice: Option<String>,
    /// 回执浮在窗口底边那条**提示条**上（设计稿 `.toast`，[`crate::toast`]）：[`Self::notice`] 换了一句就换一条，
    /// 停够了连那句回执一起收掉。
    toast: Option<Toast>,
    /// 人在这一屏按下的那道**工序**的捷径，等窗口取走。
    ///
    /// **这一屏排不了活**：排一趟工序要同时够得着库屏那一段与**任务台**，而屏与屏
    /// 之间不该互相拿着对方（ADR-0005）。所以按下去只留一个记号，由
    /// [`App::route`](crate::app::App::route) 取走、交给
    /// [`App::start_stage`](crate::app::App::start_stage)——与 [`Self::returned`]、
    /// [`Self::touched`] 同一个办法。这么一来捷径排的就是**与库屏工序段那一行完全
    /// 同一趟活**（票 `gui-self-sufficient/09`）。
    asked: Option<Stage>,
    /// 上一次出的错。
    error: Option<String>,
    /// 字体样张开着没有。
    sample: bool,
    /// **打开外部程序**那一下：播放视频、看原图、打开位置，都交给它。默认是系统默认程序
    /// （`preview::open_externally`）；测试换成只记下交出去的是哪个文件（[`Screen::set_opener`]），不拉起真的播放器。
    opener: Opener,
    /// **作品详情页**开着时是 `Some`（[`work`]）：那时这一屏只画它，三栏与屏头都被它盖住。
    page: Option<work::Page>,
    /// 把表格的滚动位置强按到这个像素偏移。**只有量帧率时才设**，真界面上永远是 `None`。
    pub scroll_to: Option<f32>,
    /// **右键菜单**那一层（[`menu`]）：表格与卡片墙共用这一层。
    menu: menu::Menu,
    /// 这一帧右键按在哪一行；表格与卡片墙各自落一份进来，这一屏取走、开出菜单
    /// （[`Self::settle_menu`]）。
    menu_click: Option<crate::table::RightClicked>,
    /// `↑` `↓` 刚挪过高亮：下一帧画表格时把那一行滚进视口。
    scroll_to_focused: bool,
    /// `⌘/Ctrl+F` 按过了：画搜索框的那一帧把光标放进去（[`Self::focus_search`]）。
    focus_search: bool,
}

impl Screen {
    /// 开一个空屏幕。
    ///
    /// `workspace` 是**工作目录**：刮削面板要它找媒体池、优先级表、中文离线索引与
    /// 沉淀库（`scrape::Panel`）。
    #[must_use]
    pub fn new(workspace: std::path::PathBuf) -> Self {
        // **规则文件写坏了也开得了屏**：那几行先照内置那份剥，并把原因摆在屏上——
        // 屏上的正题与刮削剥出来的不是同一个，这件事得让人知道。
        let (rules, error) = match romcat_core::sources::rules(&workspace) {
            Ok(rules) => (rules, None),
            Err(why) => (
                Rules::builtin(),
                Some(format!(
                    "剥离规则读不进来，认不出作品的那几行先照内置规则剥正题：{why}"
                )),
            ),
        };
        Self {
            window: Window::new(SPAN),
            card_window: Window::new(SPAN),
            cover_window: Window::new(SPAN),
            card_group_header: None,
            query: WorkQuery::default(),
            facets: Facets::default(),
            facets_for: None,
            filter: Filter::default(),
            filtered: None,
            all_works: None,
            non_game_assets: None,
            save: SaveDraft::default(),
            scrape: scrape::Panel::new(workspace),
            fixer: crate::shaping::Fixer::default(),
            reshaped: false,
            editing: None,
            returned: None,
            touched: None,
            merging: None,
            splitting: None,
            just_landed: None,
            focused: None,
            picked: Picked::default(),
            opened: None,
            work: None,
            variant: None,
            detail: None,
            scope: None,
            scoped: None,
            priorities: Priorities::builtin(),
            rules,
            pool: None,
            gallery: Gallery::new(),
            short_names: Vec::new(),
            list_covers: false,
            view: BrowseView::Table,
            card_size: CardSize::Medium,
            group_cards: false,
            only_covers: false,
            shelf: Shelf::default(),
            more_platforms: false,
            cover: None,
            collection: String::new(),
            standing: Vec::new(),
            standing_for: None,
            suppressed: Vec::new(),
            suppressed_for: None,
            collecting: None,
            title_draft: TitleDraft::default(),
            notice: None,
            lift_notice: None,
            toast: None,
            asked: None,
            error,
            sample: false,
            opener: Box::new(romcat_core::scrape::preview::open_externally),
            page: None,
            scroll_to: None,
            menu: menu::Menu::default(),
            menu_click: None,
            scroll_to_focused: false,
            focus_search: false,
        }
    }

    /// 换掉**打开外部程序**那一下（默认是系统默认程序，`romcat_core::scrape::preview::open_externally`）。
    ///
    /// 测试拿它记下「交给系统的是哪个文件」，不拉起真的播放器；真窗口那一路不必调。
    pub fn set_opener(
        &mut self,
        opener: impl FnMut(&std::path::Path) -> Result<(), String> + 'static,
    ) {
        self.opener = Box::new(opener);
    }

    /// 换一份优先级表。**导出用哪一份，这里就该用哪一份**——两份不一样的话，
    /// 面板上写着的显示标题就不是同步到掌机上会看见的那个。
    pub fn set_priorities(&mut self, priorities: Priorities) {
        // 主列表每行的显示标题也照这一份挑（`Window::set_priorities`）：表上与详情头上得是同一个名字。
        self.window.set_priorities(priorities.clone());
        self.priorities = priorities;
    }

    /// **眼下摆的是卡片墙吗**：键盘那几下要问它（`App::browse_keys`）。
    ///
    /// 挪高亮那几下走的是表格背后那扇窗的**行序号**（[`Self::step_focus`]），而卡片墙背后
    /// 是另一扇窗、另一份查询——同一个数在两边指的不是同一行（挂单 `Q1142`）。
    #[must_use]
    pub fn showing_cards(&self) -> bool {
        self.view == BrowseView::Cards
    }

    /// 换成卡片墙（实测与截图门用）。    /// 切到卡片视图；供窗口恢复偏好与界面测试走同一份状态。
    pub fn show_cards(&mut self) {
        self.view = BrowseView::Cards;
    }

    /// 从工作目录的版式偏好恢复浏览屏呈现方式；这些值绝不参与选择集规则。
    pub fn restore_view_preferences(&mut self, layout: &layout::Layout) {
        self.view = if layout.preference("浏览视图") == Some("卡片") {
            BrowseView::Cards
        } else {
            BrowseView::Table
        };
        self.list_covers = layout.preference("列表封面") == Some("是");
        self.group_cards = layout.preference("卡片分组") == Some("平台");
        self.only_covers = layout.preference("仅封面") == Some("是");
        self.card_size = match layout.preference("卡片大小") {
            Some("小") => CardSize::Small,
            Some("大") => CardSize::Large,
            _ => CardSize::Medium,
        };
    }

    /// 把浏览屏呈现方式交给统一的版式偏好文件落盘。
    pub fn save_view_preferences(&self, layout: &mut layout::Layout) {
        layout.set_preference(
            "浏览视图",
            if self.view == BrowseView::Cards {
                "卡片"
            } else {
                "表格"
            },
        );
        layout.set_preference("列表封面", if self.list_covers { "是" } else { "否" });
        layout.set_preference(
            "卡片分组",
            if self.group_cards {
                "平台"
            } else {
                "不分组"
            },
        );
        layout.set_preference("仅封面", if self.only_covers { "是" } else { "否" });
        layout.set_preference(
            "卡片大小",
            match self.card_size {
                CardSize::Small => "小",
                CardSize::Medium => "中",
                CardSize::Large => "大",
            },
        );
    }

    /// 指一份**媒体池**。**目录不在就不指**——「没查」与「查了、没有」得分得开。
    pub fn set_pool(&mut self, pool: Option<MediaPool>) {
        // **两处一起换。** 那一栏问「在不在池子里」用 `pool`，画那几格图用 `gallery`
        // 手里那条后台线程——只换一处的话，屏上会一边说「池里有」一边一格图都画不出。
        self.gallery.set_pool(pool.clone());
        // 行首那几格封面同一个池子——只换一处的话，列表里说有封面、详情里一张都画不出。
        self.shelf.set_pool(pool.clone());
        self.pool = pool;
    }

    /// 详情面板那几格缩略图。测试与实测拿它查「图解出来了没」。
    #[must_use]
    pub fn gallery(&self) -> &Gallery {
        &self.gallery
    }

    /// 同上，可改。**测试拿它走「ffmpeg 不在」那条路。**
    pub fn gallery_mut(&mut self) -> &mut Gallery {
        &mut self.gallery
    }

    /// **库里的内容被改过了，整屏重读一遍。** 刮削跑完走它。
    ///
    /// 与 [`reload`](Self::reload) 分开：那一条重问的是筛选面板上的可选值，而这一条
    /// 连**表格里那几行**一起作废——刮削写进去的正是行上那几列（元数据齐不齐、年份），
    /// 不作废的话人要滚出视口再滚回来才看得见。
    pub fn refresh(&mut self, site: &Site) {
        self.window.invalidate();
        // 刮削写进去的可能正是封面：行首那几格问过的全部作废。
        self.shelf.forget();
        self.reload(site);
        self.load_work(&site.catalog);
        self.load_detail(&site.catalog);
    }

    /// 按屏头那颗「**合并作品…**」：勾中的那几行开一层向导。
    ///
    /// **勾两个以上才开得了**，而且**全选那一档开不了**（设计稿 `#merge-btn` 的
    /// `S.pickAll?[]:[...S.picked]`）：合并要人逐个核对变体与字段，一万多行核对不过来，
    /// 而「全选」本身不是一批身份、是一个筛选条件。
    pub fn open_merge(&mut self, site: &Site) {
        let Scope::Rows(rows) = self.picked.scope() else {
            self.notice = Some(merge::NEED_TWO.to_string());
            return;
        };
        if rows.len() < 2 {
            self.notice = Some(merge::NEED_TWO.to_string());
            return;
        }
        let rows = rows.to_vec();
        self.open_merge_rows(site, &rows);
    }

    /// 作品详情页头上那颗「**合并…**」：从这一个作品起头开一层向导，第一步再搜别的作品加进来。
    pub fn open_merge_here(&mut self, site: &Site) {
        let Some(anchor) = self.opened.clone() else {
            return;
        };
        self.open_merge_rows(site, &[anchor]);
    }

    /// 变体卡头一行右头那颗「**移出此作品…**」。
    pub fn open_split(&mut self, site: &Site, key: &str) {
        let Some(work) = self.work.as_ref() else {
            return;
        };
        let Some(at) = work
            .variants
            .iter()
            .position(|variant| variant.row.key == key)
        else {
            return;
        };
        let Some(variant) = work.variants.get(at) else {
            return;
        };
        let short = self
            .short_names
            .get(at)
            .cloned()
            .unwrap_or_else(|| variant.row.key.clone());
        self.splitting = Some(merge::Split::open(
            &site.catalog,
            work,
            &self.rules,
            &variant.row.key,
            &short,
        ));
    }

    /// 画合并向导与「移出此作品」那两层；按下「合并」「移出」就落下去。
    fn merge_ui(&mut self, ctx: &egui::Context, site: &mut Site) {
        if let Some(wizard) = self.merging.as_mut() {
            match wizard.ui(ctx, site, &self.priorities) {
                None => {}
                Some(merge::Done::Close) => self.merging = None,
                Some(merge::Done::Apply) => self.apply_merge(site),
            }
        }
        if let Some(split) = self.splitting.as_mut() {
            match split.ui(ctx, site, &self.priorities) {
                None => {}
                Some(merge::Done::Close) => self.splitting = None,
                Some(merge::Done::Apply) => self.apply_split(site),
            }
        }
    }

    /// 真合并：**先落那一批裁决**，再办顺手的那三样（别名、字段选值、首选变体）。
    ///
    /// 次序是有意的：裁决那一批是主干，落不下去就一样都不该做——那三样各自都是中立库里
    /// 独立的一笔账，撤销那一批不连带（挂单 `Q1012`），先做的话会留下
    /// 「名字并了、变体没并」那种半吊子。
    fn apply_merge(&mut self, site: &mut Site) {
        // **落成了才把这一层收掉**：`triage::apply` 真会拒（计划过期，`StalePlan`），
        // 那时先收掉的话，人在三步里勾的、挑的、选的一并没了，只剩一行错
        // （自审挑出，2026-09-21 照改）。
        let Some(wizard) = self.merging.as_ref() else {
            return;
        };
        let Some(planned) = wizard.planned() else {
            return;
        };
        let keep = planned.into_work().to_string();
        let 几个 = planned.verdicts();
        let batch =
            match romcat_core::triage::merge::apply(&mut site.catalog, &mut site.store, planned) {
                Ok(账) => 账.batch,
                Err(failed) => {
                    self.error = Some(format!("合并落不下去：{failed}"));
                    return;
                }
            };
        let Some(wizard) = self.merging.take() else {
            return;
        };
        self.just_landed = Some(batch);
        let mut 顺手 = Vec::new();
        if let Err(failed) =
            romcat_core::triage::merge::keep_aliases(&mut site.catalog, &keep, &wizard.aliases())
        {
            顺手.push(format!("别名没留下：{failed}"));
        }
        for (field, offer) in wizard.adopted() {
            if let Err(failed) = romcat_core::triage::merge::adopt(
                &mut site.catalog,
                &self.priorities,
                &keep,
                field,
                &offer,
            ) {
                顺手.push(format!("{} 那一格没改成：{failed}", field.label()));
            }
        }
        // **只写人亲手点过的那几条**：没点过的平台照规则来，写下去等于把规则冻成覆盖。
        for one in wizard.preferred() {
            if let Err(failed) =
                site.catalog
                    .set_preferred_variant(&one.work, &one.platform, &one.variant_key)
            {
                顺手.push(format!("{} 的首选变体没记下：{failed}", one.platform));
            }
        }
        self.error = (!顺手.is_empty()).then(|| 顺手.join("；"));
        self.picked.clear();
        self.notice = Some(format!(
            "已合并：{} 个变体归入「{keep}」。要撤销去「待确认 → 裁决记录」。",
            thousands(几个)
        ));
        self.after_regroup(site);
    }

    /// 真移出：一条裁决，同一条撤销路。
    fn apply_split(&mut self, site: &mut Site) {
        // 同 [`Self::apply_merge`]：落成了才收掉这一层。
        let Some(split) = self.splitting.as_ref() else {
            return;
        };
        let Some(planned) = split.planned() else {
            return;
        };
        let into = planned.into_work().to_string();
        match romcat_core::triage::merge::apply(&mut site.catalog, &mut site.store, planned) {
            Ok(账) => {
                self.splitting = None;
                self.just_landed = Some(账.batch);
                self.notice = Some(format!("已移到「{into}」。要撤销去「待确认 → 裁决记录」。"));
                self.after_regroup(site);
            }
            Err(failed) => self.error = Some(format!("移不出去：{failed}")),
        }
    }

    /// 合并 / 移出落下之后：手上缓着的那几份全过期了。
    ///
    /// 作品那一行的身份（`work_id`）可能刚刚没了（那个作品一个变体都不剩），所以
    /// **点开的那一行也放掉**——留着它，详情面板会一直读一个已经不在的作品。
    fn after_regroup(&mut self, site: &Site) {
        if let Some(page) = self.page.as_mut() {
            page.forget();
        }
        self.refresh(site);
    }

    /// 重问一次筛选面板上的可选值。开库时与改过元数据之后各一次。
    pub fn reload(&mut self, site: &Site) {
        self.reload_facets(&site.catalog);
    }

    /// 照眼下那颗「显示非游戏资产」开关，重问一次左栏那几档各有哪些值、各多少个。
    ///
    /// **条数跟着开关走**：收起时左栏写「PS 7」、点进去却只列 6 个，是这一屏自己说了两个数
    /// （挂单 `Q775`）。数是核心库数的（[`Catalog::facets`]），这里只把开关交过去。
    ///
    /// 读不动时照旧只记下那句错、不每帧重试——与改之前一个样。
    fn reload_facets(&mut self, catalog: &Catalog) {
        let switch = self.query.non_game_assets;
        self.facets_for = Some(switch);
        match catalog.facets(switch) {
            Ok(facets) => {
                self.facets = facets;
                self.error = None;
            }
            Err(error) => self.error = Some(format!("中立库读不动：{error}")),
        }
    }

    /// **中立库底下变了**：这一屏缓着的东西全部作废，下一帧照新的库重取。
    ///
    /// [`Screen::reload`] 只补筛选面板那几档；这一屏另外三样也是缓着的，一样会过期：
    ///
    /// - **窗**里那 512 行连同总行数（[`Window::invalidate`]）。它们平时只在**换查询**
    ///   时作废，而扫完一个根、裁完一批的时候查询一个字都没改——于是库屏说 3 个变体，
    ///   浏览屏还画着 0 行。
    /// - 「筛出来多少」与「作用于多少个变体」这两个数。
    /// - 点开那一行的详情（置信度、结论那几栏正是裁决改的东西）。
    ///
    /// **谁来调**：不由这一屏自己判断——它没法知道后台那条线程写完了没有。窗口那一层
    /// 认领完一趟任务、或者取到队列那一屏「刚落下一批」的记号之后转告它
    /// （`crate::app::App::poll_tasks` 与 `App::route`）。屏与屏之间不互相拿着对方
    /// （ADR-0005）。
    pub fn invalidate(&mut self, site: &Site) {
        self.reload(site);
        self.window.invalidate();
        self.shelf.forget();
        self.filtered = None;
        self.non_game_assets = None;
        self.scoped = None;
        self.load_work(&site.catalog);
    }

    /// 窗口，供测试查「内存里装了几行」。
    #[must_use]
    pub fn window(&self) -> &Window {
        &self.window
    }

    /// 眼下的筛选与排序。
    #[must_use]
    pub fn query(&self) -> &WorkQuery {
        &self.query
    }

    /// 改筛选与排序。实测与测试拿它当界面上点的那一下。
    pub fn query_mut(&mut self) -> &mut WorkQuery {
        &mut self.query
    }

    /// 五个维度各有哪些值可选。
    #[must_use]
    pub fn facets(&self) -> &Facets {
        &self.facets
    }

    /// **筛选器**那棵条件组。
    #[must_use]
    pub fn filter(&self) -> &Filter {
        &self.filter
    }

    /// 左栏那颗「**全清**」：这一栏的条件全部清掉。
    ///
    /// **排序与搜索框不动。** 这颗按钮的标签只说筛选，而搜索框压根不在这一栏里
    /// （它在底下那块面板上，票 `gui-redesign/05`）——顺手清掉别人面板上的东西，
    /// 正是「搜索管排序、筛选器管集合」这句话立不住的样子。
    ///
    /// 界面上那颗按钮走的就是它，测试拿它当那一下。
    pub fn clear_filter(&mut self) {
        self.query = WorkQuery {
            search: std::mem::take(&mut self.query.search),
            order: self.query.order,
            descending: self.query.descending,
            // **「显示非游戏资产」那颗开关也不动**（票 `gui-looks-like-the-design/09`）：它是个
            // 视图开关，不是人加上去的条件——空态上那颗「清除筛选」按下去把它拨回收起，
            // 人会以为自己刚打开的东西坏了。设计稿的「清除」同样不碰它。
            non_game_assets: self.query.non_game_assets,
            ..WorkQuery::default()
        };
        self.filter.clear();
    }

    /// 把一条**规则**预填进筛选器，并当场按它筛。
    ///
    /// 子库屏点「改选择」跳回浏览屏时走的就是它（票 `gui-redesign/11`）：**规则原样摊在
    /// 筛选器里**，人改的时候看得见它真的筛出了什么。反过来那一半是
    /// [`WorkQuery::to_rule`]。
    pub fn set_filter_rule(&mut self, rule: Option<Rule>) {
        self.filter.set_rule(rule.clone());
        self.query.rule = rule;
    }

    /// **当前筛选下一共多少个变体**。`None` 是数不出来，不是零。
    #[must_use]
    pub fn filtered_total(&self) -> Option<u64> {
        self.filtered
    }

    /// 「存成子库」那两个格子，供实测与测试填。
    pub fn save_draft_mut(&mut self) -> &mut SaveDraft {
        &mut self.save
    }

    /// 正在改哪个子库的选择集；`None` 是平常的浏览。
    #[must_use]
    pub fn editing(&self) -> Option<&Editing> {
        self.editing.as_ref()
    }

    /// 「**改选择**」跳过来了：把这个子库的规则预填进筛选器，并当场按它筛。
    ///
    /// **整份筛选换成这一条**，不是往现有的筛选上再叠一层：屏上摆着的必须正好是
    /// 这个子库选出来的那一批，多一个档、多一条搜索词，人核对的就不是同一件事了。
    /// 排序留着——它不属于筛选（`WorkQuery::from_rule` 的文档说的就是这件事）。
    ///
    /// 窗口按下「改选择」时走的就是它（[`crate::app::App::route`]），
    /// 实测与测试拿它当那一下。
    pub fn begin_editing(
        &mut self,
        site: &Site,
        sublibrary: &str,
        rule: Option<Rule>,
        broken: Vec<BrokenRule>,
    ) {
        let (order, descending) = (self.query.order, self.query.descending);
        self.query = WorkQuery {
            order,
            descending,
            ..WorkQuery::default()
        };
        self.filter.clear();
        self.set_filter_rule(rule);
        self.editing = Some(Editing {
            sublibrary: sublibrary.to_string(),
            broken,
            exceptions: BTreeMap::new(),
            note: String::new(),
            ordinal: None,
        });
        self.reload_exceptions(site);
    }

    /// 这一趟**只改第 `ordinal` 条**（子库屏规则行上「✎」，票 `gui-looks-like-the-design/20`）：「更新到子库」时只换回
    /// 这一条（[`Catalog::replace_rule`](romcat_core::catalog::Catalog::replace_rule)）。紧跟在 [`Self::begin_editing`]
    /// 后面调；没在改的时候什么都不做。窗口按下「✎」时走的就是它（[`crate::app::App::route`]）。
    pub fn edit_only(&mut self, ordinal: i64) {
        if let Some(editing) = &mut self.editing {
            editing.ordinal = Some(ordinal);
        }
    }

    /// 重读正在改的那个子库的例外。记一条、撤一条之后都走一趟。
    fn reload_exceptions(&mut self, site: &Site) {
        let Some(editing) = &self.editing else {
            return;
        };
        match site.catalog.sublibrary_exceptions(&editing.sublibrary) {
            Ok(rows) => {
                let map = rows
                    .into_iter()
                    .map(|row| (row.variant_key.clone(), row))
                    .collect();
                if let Some(editing) = &mut self.editing {
                    editing.exceptions = map;
                }
                self.error = None;
            }
            Err(error) => self.error = Some(format!("中立库读不动：{error}")),
        }
    }

    /// **别处改过某个子库的例外**，手上缓着的那一份跟着重读（挂单 `Q812`）。
    ///
    /// 例外是**一按就落库**的，而这一屏「改选择」开着时手上缓着一份——子库屏那层
    /// 「手动例外」弹层（票 `gui-looks-like-the-design/22`）与删减建议表上的「排除」都写得动它。
    /// 不转告的话这儿画的是改之前那几条，而人正对着同一个子库。
    ///
    /// 与 [`Self::take_touched`] **反向同形**：那一条是这一屏改了、转告子库屏把账丢掉，
    /// 这一条是子库屏改了、转告这一屏重读。两条都只有窗口够得着两屏（ADR-0005），
    /// 所以都走 [`App::route`](crate::app::App::route)。
    ///
    /// 改的不是这一趟正在改的那个子库、或者压根没在改，就什么都不做。
    pub fn exceptions_changed(&mut self, site: &Site, name: &str) {
        if self
            .editing
            .as_ref()
            .is_some_and(|editing| editing.sublibrary == name)
        {
            self.reload_exceptions(site);
        }
    }

    /// 「**不改了**」：放下这一趟，筛选留在屏上不动。
    ///
    /// **不回滚已经记下的例外**：例外是一记就落库的独立决定（ADR-0016 说它「永久
    /// 记住」），把它们跟着一次「不改了」一起撤掉，等于替人做了一个他没做的决定。
    pub fn cancel_editing(&mut self) {
        self.editing = None;
    }

    /// 「**更新到子库**」：把屏上这份筛选原样折回一条规则，换掉那个子库读得懂的规则。
    ///
    /// 换而不是加（`Catalog::replace_rules`）：筛选器是一棵树，它折出来的本来就是
    /// **一条**；往上加的话旧那几条还在，子库选出来的就比屏上多——而那正是
    /// 「筛选就是子库的规则」这条约定要消灭的东西。
    ///
    /// **写不成规则的条件当场挡住**（`Unruly`），不是少写一条了事。
    ///
    /// 界面上按那个按钮走的就是它，实测与测试拿它当那一下。
    pub fn update_sublibrary(&mut self, site: &mut Site) {
        let Some(name) = self
            .editing
            .as_ref()
            .map(|editing| editing.sublibrary.clone())
        else {
            return;
        };
        let rule = match self.query.to_rule() {
            Ok(Some(rule)) => rule,
            Ok(None) => {
                self.error = Some(format!(
                    "一个条件都没筛：这样带回去，子库「{name}」选中的会是整个库。\
                     真要清空它的规则，走命令行。"
                ));
                return;
            }
            Err(unruly) => {
                self.error = Some(format!("这份筛选带不回子库：{}", unruly.advice()));
                return;
            }
        };
        // 「✎」跳过来的只换那一条（`Editing::ordinal`），「改选择」跳过来的整批换。
        let written = match self.editing.as_ref().and_then(|editing| editing.ordinal) {
            Some(ordinal) => match site.catalog.replace_rule(&name, ordinal, &rule) {
                Ok(true) => Ok(0),
                Ok(false) => {
                    self.error = Some(format!(
                        "子库「{name}」的第 {ordinal} 条规则已经不在了，没换。"
                    ));
                    return;
                }
                Err(error) => Err(error),
            },
            None => site.catalog.replace_rules(&name, &rule),
        };
        match written {
            Ok(_) => {
                self.notice = Some(format!(
                    "子库「{name}」的规则换成了：{rule}。屏上这 {} 行 · {} 个变体原样带过去。",
                    thousands(self.window.total()),
                    scope_label(self.filtered),
                ));
                self.error = None;
                self.editing = None;
                self.touched = Some(name.clone());
                self.returned = Some(name);
            }
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 把「更新到子库」那一下取走。**取过就没了**：窗口一帧问一次。
    pub fn take_return(&mut self) -> Option<String> {
        self.returned.take()
    }

    /// 把「刚动过哪个子库的选择集」取走。**取过就没了**：窗口一帧问一次。
    pub fn take_touched(&mut self) -> Option<String> {
        self.touched.take()
    }

    /// 按下撤掉压制那句回执旁边那颗**折标题**的捷径。界面上点那一下走的就是它，
    /// 实测与测试拿它当那一下。
    ///
    /// **它不在这儿排活**，只留一个记号（`asked` 那一格）——排的是与库屏工序段
    /// 那一行完全同一趟。
    pub fn ask_fold_titles(&mut self) {
        self.asked = Some(Stage::FoldTitles);
        self.lift_notice = None;
        // **这一句在两种情形下都得是真的**：`Section::start` 撞上同一道工序已经在跑
        // 就什么都不做，那时「排上去了」是假话。所以后半句把那一档一并说出来。
        self.notice = Some(
            "整理标题交给任务台了——跑完那条叫法就回到标题集合里。\
             台上已经在跑同一趟的话，不会再排一遍。"
                .to_string(),
        );
    }

    /// 把上面那一下取走。**取过就没了**：窗口一帧问一次
    /// （[`App::route`](crate::app::App::route)）。
    pub fn take_asked(&mut self) -> Option<Stage> {
        self.asked.take()
    }

    /// 给正在改的那个子库记一条**例外**：把这个变体含进来，或者排除掉。
    ///
    /// **优先于规则、永久记住**（ADR-0016）。落在**变体**这一层——「这个我小时候玩过」
    /// 说的是某一份，不是某个作品下面全部那几份。
    pub fn set_exception(&mut self, site: &mut Site, key: &str, kind: Exception) {
        let Some(editing) = &self.editing else {
            return;
        };
        let (name, note) = (editing.sublibrary.clone(), editing.note.trim().to_string());
        let note = (!note.is_empty()).then_some(note);
        match site
            .catalog
            .set_exception(&name, key, kind, note.as_deref())
        {
            Ok(()) => {
                self.notice = Some(format!("给「{name}」记下了一条{}例外：{key}", kind.shown()));
                if let Some(editing) = &mut self.editing {
                    editing.note.clear();
                }
                // **一按就落库**，所以子库屏那边缓着的差量与容量当场就过期了。
                self.touched = Some(name);
                self.reload_exceptions(site);
            }
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 撤掉这个变体上那条例外。
    pub fn clear_exception(&mut self, site: &mut Site, key: &str) {
        let Some(name) = self
            .editing
            .as_ref()
            .map(|editing| editing.sublibrary.clone())
        else {
            return;
        };
        match site.catalog.clear_exception(&name, key) {
            Ok(true) => {
                self.notice = Some(format!("撤掉了 {key} 上那条例外。"));
                self.touched = Some(name);
                self.reload_exceptions(site);
            }
            Ok(false) => self.notice = Some("那一条已经不在了。".to_string()),
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// **扔掉一条读不懂的规则**——界面上处置它们的那条路（票 `gui-redesign/14`）。
    ///
    /// 为什么落在这一屏而不是子库屏：子库屏上**一个写的动作都没有**
    /// （票 `gui-redesign/11` 把八个概念降到三个靠的就是这条），而这一屏本来就是
    /// 这个子库的规则唯一改得动的地方。读不懂的那几条搬不进那棵筛选树，
    /// 于是原样摆在筛选栏顶上那条横幅里（`Self::broken_rules_ui`），
    /// 只给「扔掉」这一个动作。
    ///
    /// **只删这一条**：读得懂的那几条与全部例外一个都不碰。判据在核心里
    /// （[`Catalog::discard_broken_rule`]，先读一遍再决定删不删），这一屏只把序号递
    /// 过去、把它说的话印出来（ADR-0005）。
    ///
    /// 扔掉**不改变这个子库选出什么**——它本来就没参与求值
    /// （[`LoadedSelection::from_stored`] 早把它挑出来另放了）。但子库屏那张卡上印的
    /// 正是这几条，所以照旧留一个记号让它重读一遍（[`Self::take_touched`]）。
    ///
    /// **代价说清楚**：那个记号同时会把子库屏缓着的差量预览与容量账丢掉
    /// （`sublibrary::Screen::forget`）——排过一趟预览的人扔掉一条坏规则之后要重排。
    /// **有意留成这样**：那条通道只有一根，而它挡的是票 `11` 收尾审查报的那一条
    /// （例外一按就落库、子库屏还攥着旧差量，而「同步」认的正是它，挂单 `Q91` 第 2 条）。
    /// 为「只重读、不作废」另开一根轻通道，等于给「真改过选择却没作废」留一个新入口，
    /// 而那一类的代价是把过期的计划传上卡。记在挂单 `Q144`。
    ///
    /// 界面上按那个按钮走的就是它，实测与测试拿它当那一下。
    pub fn discard_broken_rule(&mut self, site: &mut Site, ordinal: i64) {
        let Some(name) = self
            .editing
            .as_ref()
            .map(|editing| editing.sublibrary.clone())
        else {
            return;
        };
        match site.catalog.discard_broken_rule(&name, ordinal) {
            Ok(Discarded::Gone) => {
                self.notice = Some(format!(
                    "扔掉了子库「{name}」第 {ordinal} 条读不懂的规则。\
                     读得懂的那几条与例外一条都没动。"
                ));
                self.error = None;
            }
            // **也留记号**：库里已经没有这一条了（另一个窗口先删过一遍），
            // 子库屏那张卡照旧红着印它，不重读一遍人会以为按了没用。
            Ok(Discarded::Absent) => {
                self.notice = Some(format!("第 {ordinal} 条已经不在了。"));
                self.error = None;
            }
            // 这一栏列的全是读不懂的，所以界面上摆不出这一种。它是那道闸的回声：
            // 真撞上了说明屏上这份与库里的对不上了——重读一遍就对上。
            Ok(Discarded::Readable) => {
                self.error = Some(format!(
                    "第 {ordinal} 条读得懂——这条路只扔读不懂的。\
                     改读得懂的那几条走上面的筛选器，按「更新到子库」。"
                ));
            }
            // **读与写各一趟**（先读一遍再决定删不删），所以这句话不说「写不动」
            // ——中立库被锁住时头一个失败的是那趟读。
            Err(error) => {
                self.error = Some(format!("中立库读写不动：{error}"));
                return;
            }
        }
        // 屏上这一栏与子库屏那张卡都要跟着库走：那一栏自己重读，卡由窗口转告
        // （[`Self::take_touched`] → `sublibrary::Screen::forget`）。
        self.touched = Some(name);
        self.reload_broken(site);
    }

    /// 重读正在改的那个子库里**读不懂**的那几条。扔掉一条之后走一趟。
    ///
    /// 与 [`Self::reload_exceptions`] 同一条理由：库是事实来源，屏上这份是它的副本，
    /// 写完不重读的话，那一栏会一直摆着已经不在库里的那一条。
    fn reload_broken(&mut self, site: &Site) {
        let Some(name) = self
            .editing
            .as_ref()
            .map(|editing| editing.sublibrary.clone())
        else {
            return;
        };
        match site.catalog.sublibrary_rules(&name) {
            Ok(stored) => {
                let broken = LoadedSelection::from_stored(&stored).broken;
                if let Some(editing) = &mut self.editing {
                    editing.broken = broken;
                }
            }
            Err(error) => self.error = Some(format!("中立库读不动：{error}")),
        }
    }

    /// 选中了哪几行。
    #[must_use]
    pub fn picked(&self) -> &Picked {
        &self.picked
    }

    /// 改选中。实测与测试拿它当界面上勾的那一下。
    pub fn picked_mut(&mut self) -> &mut Picked {
        &mut self.picked
    }

    /// 点开的那一行的详情（作品 → 变体）。
    #[must_use]
    pub fn work(&self) -> Option<&WorkDetail> {
        self.work.as_ref()
    }

    /// 详情面板里选中的那一个**变体**的键。变体级的操作只作用于它。
    #[must_use]
    pub fn variant_key(&self) -> Option<&str> {
        self.variant.as_deref()
    }

    /// 选中那个变体的详情。
    #[must_use]
    pub fn detail(&self) -> Option<&VariantDetail> {
        self.detail.as_ref()
    }

    /// 眼下这份选中展开成多少个变体——**批量操作按下去会动这么多**。
    ///
    /// `None` 是数不出来（读库出的错在 [`Self::error`] 里），不是零。
    #[must_use]
    pub fn scope_total(&self) -> Option<u64> {
        self.scope
    }

    /// **批量操作作用于哪些变体**：选中的那几行底下的变体，按当前筛选。
    ///
    /// 刮削、存成子库、加收藏（票 `10` / `11` / `06`）按下去要动的就是这一批。
    /// 这一票只把这条路铺到位并钉住语义，几个按钮各自在各自的票里接上来。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn batch_variants(
        &self,
        catalog: &Catalog,
    ) -> Result<Vec<String>, romcat_core::catalog::CatalogError> {
        catalog.scoped_variants(&self.query, self.picked.scope())
    }

    /// 上一次出的错。
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// 上一次动作的回执。
    #[must_use]
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// 加一条叫法的草稿，供实测与测试填。
    pub fn title_draft_mut(&mut self) -> &mut TitleDraft {
        &mut self.title_draft
    }

    /// 点开主列表的一行。**界面上点那一行走的就是它**，测试拿它当那一下。
    ///
    /// 顺带把这一行底下的**第一个变体**选中：详情面板的第二层与第三层总得有东西摆，
    /// 而「第一个」在核心库那边是按键排的，同一份库点两次结果一样。
    pub fn open_work(&mut self, catalog: &Catalog, anchor: &WorkAnchor) {
        self.opened = Some(anchor.clone());
        self.load_work(catalog);
    }

    /// 在详情面板里选中某一个**变体**。**变体级的操作只作用于它。**
    pub fn pick(&mut self, catalog: &Catalog, key: &str) {
        self.variant = Some(key.to_string());
        self.load_detail(catalog);
    }

    /// 把草稿里那条叫法写进**标题集合**，**来源记作裁决**。
    ///
    /// 界面上「加进集合」那个按钮走的就是它。
    pub fn add_title(&mut self, site: &mut Site, work: &str) {
        let Some(detail) = self.detail.clone() else {
            return;
        };
        if self.write_title(site, work, &detail) {
            self.load_detail(&site.catalog);
        }
    }

    /// 从**标题集合**里删掉一条叫法，并把「这条被压掉了」记进**沉淀库**。
    ///
    /// 界面上那个「删」走的就是它，**刮削来的也能删**——删一条明显错的中文名是当下就想
    /// 做的事，把按钮灰掉反而要向人解释一整套来源规则（挂账 D157）。
    ///
    /// 两件事捏在核心库那一个动作里（[`title::suppress`]）：只删中立库那一行，
    /// 下一趟重折就把它折回来了；只记压制不删行，人得等到下一趟重折才看得见效果。
    pub fn suppress_title(&mut self, site: &mut Site, row: &romcat_core::catalog::TitleRow) {
        let value = row.value.clone();
        let done = title::suppress(&mut site.catalog, &mut site.store, row);
        // **不论成没成都重读一遍。** 那个动作是两步（先记号、后删行），中间撕得开一次
        // ——出了错也可能已经动过库了，面板上摆着旧的那一份会让人以为什么都没发生。
        self.suppressed_for = None;
        self.sync_suppressed(site);
        self.load_detail(&site.catalog);
        match done {
            Ok(done) => {
                self.notice = Some(match (done.recorded, done.removed, row.is_verdict()) {
                    (true, _, _) => {
                        format!(
                            "删掉了叫法「{value}」，并记下这一下——重新整理标题也不会把它加回来。"
                        )
                    }
                    (false, true, true) => format!(
                        "删掉了你自己写下的叫法「{value}」。它本来就不经过整理标题，\
                         没有什么会把它加回来。"
                    ),
                    (false, true, false) => {
                        format!("删掉了叫法「{value}」。它本来就压着。")
                    }
                    (false, false, true) => {
                        format!("你自己写下的那条叫法「{value}」已经不在了。")
                    }
                    (false, false, false) => {
                        format!("叫法「{value}」已经不在了，压制照旧记着。")
                    }
                });
                self.error = None;
            }
            // **先记号、后删行**（`title::suppress`），所以出错时最坏也只是「记号在、
            // 那一行还在」——下一趟重折会自己把它收干净，不会留下「行没了而没人记得」。
            Err(error) => self.error = Some(format!("这条叫法压不掉：{error}")),
        }
    }

    /// **撤掉一条压制**：那条叫法下一趟重折就回来了。界面上那个「恢复」走的就是它。
    ///
    /// **不当场把它折回来**：折一趟要走遍全库的发行版、变体与刮削值，那是画帧线程上
    /// 几秒钟的事（真库 46,483 个变体、38,963 条自动通过的候选）。所以这里只撤记号，
    /// 回执里说清它什么时候回来。
    pub fn lift_title(&mut self, site: &mut Site, one: &TitleSuppression) {
        match site.store.lift_title_suppression(&one.key()) {
            Ok(true) => {
                self.suppressed_for = None;
                // **先重读再说话**：这一句会把上一条 `lift_notice` 收掉（换作品那一条路
                // 走的也是它），所以这一趟要说的话得排在它后头。
                self.sync_suppressed(site);
                self.notice = None;
                self.lift_notice = Some(format!(
                    "撤掉了对「{}」的压制。这一屏不当场整理，那要走遍全库\
                     ——点旁边那颗「整理标题」排一趟，跑完它就回到标题集合里。",
                    one.value,
                ));
            }
            // **这两条也得把上一句连它那颗按钮收掉**：不收的话「那条压制已经撤过了」
            // 会与上一条「撤掉了对「X」的压制」同屏叠着画，而那两句说的是反话。
            Ok(false) => {
                self.lift_notice = None;
                self.notice = Some("那条压制已经撤过了。".to_string());
            }
            Err(error) => {
                self.lift_notice = None;
                self.error = Some(format!("沉淀库写不动：{error}"));
            }
        }
    }

    /// 把**首选变体**裁给这一个。界面上点那一行走的就是它。
    pub fn set_preferred(&mut self, site: &mut Site, work: &str, platform: &str, key: &str) {
        match site.catalog.set_preferred_variant(work, platform, key) {
            Ok(()) => {
                self.notice = Some(format!("首选变体裁给了 {key}。"));
                self.load_detail(&site.catalog);
            }
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 撤掉**首选变体**裁决，回到规则算的那一个。
    pub fn clear_preferred(&mut self, site: &mut Site, work: &str, platform: &str) {
        match site.catalog.clear_preferred_variant(work, platform) {
            Ok(true) => {
                self.notice = Some("撤掉了首选变体裁决，回到规则算的那一个。".to_string());
                self.load_detail(&site.catalog);
            }
            Ok(false) => self.notice = Some("本来就没人裁过。".to_string()),
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 重新读一遍那一行的详情，**并且把选中的那个变体归位**。
    ///
    /// 归位这一步不能省：换一套筛选之后，原先选中的那个变体可能已经不在这一行底下了
    /// （按平台筛掉的那些），而底下那块面板还照着它画、改元数据还落在它头上——
    /// 人看见的是一行，动到的是另一行。不在了就退回这一行的第一个变体；
    /// 一个都不剩就一起清掉。
    fn load_work(&mut self, catalog: &Catalog) {
        // 作品详情页手上那几份变体详情是照上一趟读的：这一趟重读了，它们跟着作废。
        if let Some(page) = self.page.as_mut() {
            page.forget();
        }
        let Some(anchor) = self.opened.clone() else {
            self.work = None;
            self.cover = None;
            self.variant = None;
            self.detail = None;
            return;
        };
        match catalog.work_detail(&self.query, &anchor) {
            Ok(work) => {
                self.work = work;
                self.error = None;
            }
            Err(error) => {
                self.work = None;
                self.error = Some(format!("中立库读不动：{error}"));
            }
        }
        // **变体简称由核心库拼**（词表「变体简称」）：点开时问一次，卡片照印。拼不出来时卡片退回文件名。
        self.short_names = match &self.work {
            Some(work) => match catalog.variant_short_names(work, &self.priorities) {
                Ok(names) => names,
                Err(error) => {
                    self.error = Some(format!("中立库读不动：{error}"));
                    Vec::new()
                }
            },
            None => Vec::new(),
        };
        // **头上那一格贴哪张，点开时问核心库一次**：与表上那一行行首是同一处挑的。
        let cover = match &self.work {
            Some(work) => catalog.cover_of(&work.anchor, &work.name, self.pool.as_ref()),
            None => Ok(None),
        };
        match cover {
            Ok(cover) => self.cover = cover,
            Err(error) => {
                self.cover = None;
                self.error = Some(format!("中立库读不动：{error}"));
            }
        }
        let keep = self.variant.as_deref().is_some_and(|key| {
            self.work
                .as_ref()
                .is_some_and(|work| work.variants.iter().any(|v| v.row.key == key))
        });
        if keep {
            return;
        }
        let first = self
            .work
            .as_ref()
            .and_then(|work| work.variants.first())
            .map(|variant| variant.row.key.clone());
        match first {
            Some(key) => self.pick(catalog, &key),
            None => {
                self.variant = None;
                self.detail = None;
            }
        }
    }

    /// 重新读一遍选中那个变体的详情。改过元数据之后要走一趟——面板上摆的必须是
    /// 库里现在的样子。
    fn load_detail(&mut self, catalog: &Catalog) {
        let Some(key) = self.variant.clone() else {
            self.detail = None;
            return;
        };
        match catalog.variant_detail(&key, &self.priorities, self.pool.as_ref()) {
            Ok(detail) => {
                self.detail = detail;
                self.error = None;
            }
            Err(error) => {
                self.detail = None;
                self.error = Some(format!("中立库读不动：{error}"));
            }
        }
    }

    /// **屏头右侧**属于这一屏的那一段（[`crate::look::screen_header`]）。
    ///
    /// **照稿它几乎是空的**（拿主意的人 2026-09-22 对着 `prototype.html` 裁的，挂单 `Q1102`）：
    /// 稿上 `#s-browse .scrhead` 里只有标题与那句副标题，另有两样**默认不画**的——
    /// 「正在编辑子库「…」的选择集」那句话与那颗「更新子库」（`hidden`，**归票 `23`**：
    /// 从浏览屏改子库的选择集是那一票的活，位置照稿在这儿，条件是「正在改哪个子库」）。
    ///
    /// **那几颗批量按钮不在这儿**：稿上它们从来就在表格上方那一条的右端
    /// （`.tbar .acts`，见 `Screen::action_bar`）。票 09 把它们整组挪进了屏头
    /// （挂单 `Q876`／`Q877`，那时拿主意的人点过头），这一票照稿挪了回去。
    ///
    /// 于是这儿只剩**开发用的那一个开关**——只在 `demo` 为真（这扇窗带 `--demo` 启动，
    /// [`crate::app::App::mark_demo`]）时摆，挂单 `Q874`。**不看编译开关**：门禁带
    /// `--all-features` 跑截图，照编译开关藏的话门禁那一路的截图里就画着它。
    pub fn status(&mut self, ui: &mut egui::Ui, demo: bool) {
        if demo {
            ui.toggle_value(&mut self.sample, "字体样张");
        }
    }

    /// 「列表」那一条右端那一句（设计稿 `.tbar .cnt`）：没勾的时候是「**N** 个作品（共 M）」，勾了是
    /// 「**已选 N 个作品** · N 个变体」；读库出错时是那一句错。
    ///
    /// **写法照稿**：从前这一句在顶栏上，还跟着「内存里几行、读库几次」，那是写给开发者的。
    fn count_line(&self, ui: &egui::Ui) -> std::sync::Arc<egui::Galley> {
        let tokens = Tokens::builtin();
        let (次, 弱, 错) = {
            let visuals = ui.visuals();
            (
                visuals.text_color(),
                visuals.weak_text_color(),
                visuals.error_fg_color,
            )
        };
        // 照稿 `.tbar .cnt`：半号 `size-small-plus`、次一级的字色，数字加粗、括号里那半句弱字色（`.dim`）。
        // 半号字号走 [`look::font_size`] 取整（票 `gui-looks-like-the-design/32` 定的统一入口）。
        let 字号 = look::font_size(ui.ctx(), tokens.font.size_small_plus);
        let 常规 = egui::FontId::proportional(字号);
        let 加粗 = egui::FontId::new(字号, font::strong_family());
        let mut job = egui::text::LayoutJob::default();
        let mut 接 = |字: &str, 字体: &egui::FontId, 色: egui::Color32| {
            job.append(字, 0.0, egui::TextFormat::simple(字体.clone(), 色));
        };
        if let Some(error) = self.window.error() {
            接(error, &常规, 错);
        } else {
            let total = self.window.total();
            let picked = self.picked.count(total);
            if picked == 0 {
                接(&thousands(total), &加粗, 次);
                接(" 个作品", &常规, 次);
                接(
                    &format!("（共 {}）", scope_label(self.all_works)),
                    &常规,
                    弱,
                );
            } else {
                接(&format!("已选 {} 个作品", thousands(picked)), &加粗, 次);
                if self.picked.is_all() {
                    接("（全部筛选结果）", &常规, 次);
                }
                接(&format!(" · {} 个变体", scope_label(self.scope)), &常规, 次);
            }
        }
        ui.painter().layout_job(job)
    }

    /// 换过筛选或排序就把窗口作废重取。没换过是空操作。
    ///
    /// **换排序与换筛选丢掉的东西不一样**：
    ///
    /// - 换**排序**只是把同一批行重排。高亮那一份是个全序下标，重排之后它会指到另一行
    ///   身上，所以丢掉；**选中的那几行一条都不动**——筛出来的还是同一批，人勾了两百行
    ///   再按一下「容量」表头，选中不该凭空消失。
    /// - 换**筛选**才是换了一批行。这时全选说的「当前筛出来的这一批」已经不是同一批，
    ///   留着它会让批量操作作用到人根本没看见的行上，所以连选中一起清掉；点开的那一行
    ///   在新的筛选下也可能一个变体都不剩，详情跟着重读一次。
    fn sync_window(&mut self, catalog: &Catalog) {
        // 左栏那几档的条数跟着「显示非游戏资产」那颗开关走（[`Self::reload_facets`]）。
        if self.facets_for != Some(self.query.non_game_assets) {
            self.reload_facets(catalog);
        }
        let refiltered = !self.window.query().same_filter(&self.query);
        if self.window.query() != &self.query {
            self.focused = None;
            self.window.set_query(self.query.clone());
        }
        if refiltered {
            self.picked.clear();
            self.load_work(catalog);
        }
        self.window.sync(catalog);
        let card_query = self.query.clone().with_covers_only(self.only_covers);
        self.card_window.set_query(card_query);
        self.card_window.set_priorities(self.priorities.clone());
        self.card_window.sync(catalog);
        let cover_query = self.query.clone().with_covers_only(true);
        self.cover_window.set_query(cover_query);
        self.cover_window.set_priorities(self.priorities.clone());
        self.cover_window.sync(catalog);
        if refiltered || self.filtered.is_none() {
            // **筛出来多少**与**选中多少**是两个数，各数各的：前者换筛选才变，
            // 后者每勾一行就变。合成一个的话，屏上「筛出 N 行」会跟着勾选跳。
            match catalog.scoped_variant_total(&self.query, Scope::AllExcept(&[])) {
                Ok(total) => self.filtered = Some(total),
                Err(error) => {
                    self.filtered = None;
                    self.error = Some(format!("中立库读不动：{error}"));
                }
            }
            // 「（共 M）」：同一个开关、什么都不筛。数是核心库数的（`Catalog::work_total`）。
            let 不筛 = WorkQuery {
                non_game_assets: self.query.non_game_assets,
                ..WorkQuery::default()
            };
            match catalog.work_total(&不筛) {
                Ok(total) => self.all_works = Some(total),
                Err(error) => {
                    self.all_works = None;
                    self.error = Some(format!("中立库读不动：{error}"));
                }
            }
        }
        if refiltered || self.non_game_assets.is_none() {
            // **收起了几个由核心库数**，与表上那几行走同一份筛选——数与表对得上靠的是这个，
            // 不是这一层自己去数。拨一下开关就是换了筛选（`WorkQuery::same_filter`）。
            match catalog.non_game_asset_rows(&self.query) {
                Ok(rows) => self.non_game_assets = Some(rows),
                Err(error) => {
                    self.non_game_assets = None;
                    self.error = Some(format!("中立库读不动：{error}"));
                }
            }
        }
        if refiltered || self.scoped.as_ref() != Some(&self.picked) {
            self.scoped = Some(self.picked.clone());
            match catalog.scoped_variant_total(&self.query, self.picked.scope()) {
                Ok(total) => self.scope = Some(total),
                Err(error) => {
                    // **数不出来就说数不出来**，不摆一个 0 出去：屏上写着
                    // 「作用于 0 个变体」而按下去真会动一百多个，比不写更坏。
                    self.scope = None;
                    self.error = Some(format!("中立库读不动：{error}"));
                }
            }
        }
    }

    /// **刮削面板**，供测试与实测查旋钮的位置、那本账、任务号。
    #[must_use]
    pub fn scrape(&self) -> &scrape::Panel {
        &self.scrape
    }

    /// 刮削面板，供测试与实测拨旋钮、按「开始刮削」。
    pub fn scrape_mut(&mut self) -> &mut scrape::Panel {
        &mut self.scrape
    }

    /// **成型纠正**那一层（测试拿它核对）。
    #[must_use]
    pub fn fixer(&self) -> &crate::shaping::Fixer {
        &self.fixer
    }

    /// 刚落过一笔人工纠正：窗口据此去库屏那一处排一趟重新成型。问过就清掉。
    pub fn take_reshaped(&mut self) -> bool {
        std::mem::take(&mut self.reshaped)
    }

    /// **按「刮削…」那一下**：把这一批展开成变体的键，摊开刮削面板。
    ///
    /// 范围就是[批量操作作用的那一批](Self::batch_variants)——屏上写几个、面板列几个、
    /// 按下去动几个，三处同一个数（票 `gui-redesign/03` 的口径，这一票不另算一份）。
    ///
    /// 界面上那个按钮走的就是它，测试拿它当那一下。
    pub fn open_scrape(&mut self, catalog: &Catalog) {
        match self.batch_variants(catalog) {
            // **一个都没勾就别摊开面板。** 摆一块「作用于 0 个变体」的面板出来，
            // 人会去按那个按钮，然后对着一份什么都没干的报告猜哪儿出了问题。
            Ok(keys) if keys.is_empty() => {
                self.error = Some(
                    "一行都没勾。在列表里勾几行，或者按表头那个全选——\
                     刮削的作用范围就是勾中的那一批。"
                        .to_string(),
                );
            }
            Ok(keys) => {
                // **屏上那个数与这份名单必须对得上。** 对不上就是这一层出了问题，
                // 而那正是「按下去动的比屏上写的多」那种事故。
                let shown = self
                    .scope
                    .unwrap_or(u64::try_from(keys.len()).unwrap_or(u64::MAX));
                self.scrape.open(keys, shown);
                self.error = None;
            }
            Err(error) => self.error = Some(format!("中立库读不动：{error}")),
        }
    }

    // ── 收藏与合集（票 `gui-redesign/06`） ────────────────────────────────────

    /// 「**合集**」那个格子，供实测与测试填。
    pub fn collection_draft_mut(&mut self) -> &mut String {
        &mut self.collection
    }

    /// 详情面板里选中那个变体在哪几个合集里，各钉在哪种锚上（`内容` / `路径`）。
    #[must_use]
    pub fn standing(&self) -> &[(String, &'static str)] {
        &self.standing
    }

    /// 选中那个变体所属的作品上，眼下**压掉的叫法**有哪几条。
    ///
    /// 它与[标题集合](VariantDetail::titles)是两份东西：那一份是**眼下有的**，
    /// 这一份是**人删掉、不许再折回来的**。摆在一起，屏上才分得出「压掉了」与「没采到」。
    #[must_use]
    pub fn suppressed(&self) -> &[TitleSuppression] {
        &self.suppressed
    }

    /// 按「**★ 收藏**」那一下：把勾中的那一批排上[任务台](crate::task)，放进[收藏](FAVORITE)。
    ///
    /// **它返回的时候两份库一个字都还没写**（票 `parking-3/09`）：这一下只是**排一趟活**，
    /// 落库在[认领](Self::settle_collection)那一步。测试与实测要断库里有没有，
    /// 得先把台上那一趟跑完并认领（`App::poll_tasks`）。
    ///
    /// 界面上那颗按钮走的就是它。
    pub fn favorite(&mut self, site: &Site, tasks: &mut Tasks) {
        self.join(site, tasks, FAVORITE);
    }

    /// 按「**☆ 取消收藏**」那一下。**同样是排一趟活**，见 [`Self::favorite`]。
    pub fn unfavorite(&mut self, site: &Site, tasks: &mut Tasks) {
        self.part(site, tasks, FAVORITE);
    }

    /// 按「**加入合集**」那一下：格子里那个名字。**同样是排一趟活**，见 [`Self::favorite`]。
    pub fn join_collection(&mut self, site: &Site, tasks: &mut Tasks) {
        let name = self.collection.trim().to_string();
        self.join(site, tasks, &name);
    }

    /// 按「**移出合集**」那一下。**同样是排一趟活**，见 [`Self::favorite`]。
    pub fn leave_collection(&mut self, site: &Site, tasks: &mut Tasks) {
        let name = self.collection.trim().to_string();
        self.part(site, tasks, &name);
    }

    /// 把勾中的那一批放进一个合集。
    ///
    /// **一个都没勾就别动库**：与「刮削…」同一条规矩——摆出一份「作用于 0 个变体」
    /// 的回执，人只会对着它猜哪儿出了问题。
    fn join(&mut self, site: &Site, tasks: &mut Tasks, name: &str) {
        self.queue_collection(site, tasks, name, true, "加收藏");
    }

    /// 把勾中的那一批从一个合集里拿出来。
    fn part(&mut self, site: &Site, tasks: &mut Tasks, name: &str) {
        self.queue_collection(site, tasks, name, false, "取消收藏");
    }

    /// **把这一下排上[任务台](crate::task)**：读那一半跑在画帧那条线程之外。
    ///
    /// ## 为什么非搬走不可
    ///
    /// 「全选 46,483 行 → ★ 收藏」那一下，读那一半要为每个变体折出它的锚
    /// （`collection::plan`），实测在画帧那条线程上跑 **6.7 秒**（挂单 `Q119`）——
    /// 期间窗口是一块白板，切不了屏、滚不动列表、连「停下」都点不着。
    ///
    /// **只有读那一半上台**：写那一半（沉淀库那些成员关系、中立库那份投影）在
    /// [认领](Self::settle_collection)那一步落，台上那条线拿的是只读连接、写不动
    /// （`crate::task::Product` 的文档）。
    ///
    /// ## 后台那条线程读的是哪一份库
    ///
    /// 与子库屏排差量预览同一条路（`sublibrary::Screen::preview`）：**同一个文件的
    /// 第二份只读连接**。只活在内存里的那种库（合成数据）分不出第二份，那一趟就
    /// **就地跑完**——那份库上它是几毫秒的事。**别的原因分不出来就直说**，
    /// 不偷偷退回画帧那条线程：那既会僵住窗口，又把真正的问题盖住了。
    fn queue_collection(
        &mut self,
        site: &Site,
        tasks: &mut Tasks,
        name: &str,
        joining: bool,
        doing: &str,
    ) {
        if self.上一趟还在跑() {
            return;
        }
        let Some(keys) = self.scoped_keys(&site.catalog, doing) else {
            return;
        };
        self.queue_collection_keys(site, tasks, name, joining, keys, doing);
    }

    /// **一趟没跑完就别排第二趟**：两趟一起落，后一趟算的是前一趟落库之前那份库。
    /// 还在跑就摆一句话出来，并交回 `true`。
    ///
    /// 这句话里**不带动词**：按的那一下是加收藏还是取消收藏，说的是**这一次**，而还在跑的是
    /// **上一趟**——先按 ★ 再按 ☆，写成「上一趟取消收藏还在跑」就是句假话。
    ///
    /// **那句话只有这一处**：排活有两个入口（[`Self::queue_collection`] 按勾中的那一批，
    /// [`Self::queue_collection_keys`] 按交进来的那几个键），两处各写一份迟早说成两句话。
    fn 上一趟还在跑(&mut self) -> bool {
        if self.collecting.is_none() {
            return false;
        }
        self.error = Some(
            "上一趟还在跑（收藏与合集一次只排一趟）。\
             任务屏上看得见它走到哪儿了，也按得停。"
                .to_string(),
        );
        true
    }

    /// 同 [`Self::queue_collection`]，只是**作用范围由调用方交进来**。
    ///
    /// 分出这一半，是因为收藏这一下有两种范围：屏头那颗 ★ 与左栏那几颗作用于**勾中的那一批**
    /// （[`Self::scoped_keys`]），而右键菜单里那一项与 `F` 那一下作用于**光标底下这一行**
    /// ——范围不同，排活、落库、那本账一个字都不该不同。
    fn queue_collection_keys(
        &mut self,
        site: &Site,
        tasks: &mut Tasks,
        name: &str,
        joining: bool,
        keys: Vec<String>,
        doing: &str,
    ) {
        if self.上一趟还在跑() {
            return;
        }
        let title = format!(
            "{}「{name}」· {} 个变体",
            if joining { "放进" } else { "拿出" },
            thousands(keys.len() as u64),
        );
        let library_identity = site.library_identity.clone();
        let name = name.to_string();
        self.collecting = Some(match site.catalog.read_only() {
            Ok(reader) => tasks.queue(title, move |task| {
                collection::plan(&reader, &library_identity, &name, &keys, joining, task)
                    .map(|planned| Product::Planned(Box::new(planned)))
                    // **被按停不折成一句「失败」**：`CollectionError` 自己认得
                    // 那一支，折过来就是 `Cutoff::Halted`（`collection` 那一处的
                    // `From`）。这一层一个字都不用凑。
                    .map_err(Cutoff::from)
            }),
            // **只活在内存里的库分不出第二份连接**（合成数据走这条），那是意料之中的。
            Err(CatalogError::NotOnDisk { .. }) => tasks.run_here(title, |task| {
                collection::plan(
                    &site.catalog,
                    &library_identity,
                    &name,
                    &keys,
                    joining,
                    task,
                )
                .map(|planned| Product::Planned(Box::new(planned)))
                .map_err(Cutoff::from)
            }),
            Err(why) => {
                self.error = Some(format!(
                    "{doing}没开跑：读中立库要另开一份只读连接，这一下没开出来（{why}）。\n\
                     先确认中立库那个文件还在、结构版本对得上，再按一次。"
                ));
                return;
            }
        });
        self.error = None;
    }

    /// 任务台交回来一趟跑完的活。**不是自己那一趟就放过去**（返回 `false`）。
    ///
    /// **写那一半在这儿落**：台上那条线拿的是只读连接，写不动。所以这一层收的是可写的
    /// 那份现场（`crate::task::Product::Planned` 的文档）。
    pub fn settle_collection(
        &mut self,
        site: &mut Site,
        done: Finished<Product>,
    ) -> Option<Finished<Product>> {
        if self.collecting != Some(done.id) {
            return Some(done);
        }
        self.collecting = None;
        match done.ended {
            // **排出来的那份计划就落下去。**（**停在半路**那一档它到不了：排那一趟整条
            // 只读，停下来什么都不留下。并进这一支只为把那条轴配全。）
            Ending::Done(Product::Planned(planned))
            | Ending::Halfway {
                product: Product::Planned(planned),
                ..
            } => {
                let name = planned.name.clone();
                let joining = planned.joining;
                match collection::commit(site, &planned) {
                    Ok(applied) => self.settle(site, &name, applied, joining),
                    Err(error) => self.error = Some(format!("{error}")),
                }
            }
            // 别的屏排上去的活轮不到这儿——上面那道判断已经挡掉了，这一支只为把
            // `Product` 那个枚举配全。
            Ending::Done(_) | Ending::Halfway { .. } => {}
            // **停下来的地方是干净的，就得这么说。** 说成「失败」会让人去找哪儿坏了。
            Ending::Stopped => {
                self.notice = Some(format!(
                    "{}。这一趟整条只读——沉淀库、中立库一个字节都没动，\
                     再按一次就是。",
                    Ending::<()>::Stopped.render(),
                ));
                self.error = None;
            }
            // **不静默结束**：哪一步、为什么，两样都说出来。
            Ending::Failed { step, why } => {
                self.error = Some(format!(
                    "这一趟{}",
                    Ending::<()>::Failed { step, why }.render()
                ));
            }
        }
        None
    }

    /// 勾中的那一批展开成的变体键；一个都没勾（或者读不动库）时给一句话并返回 `None`。
    fn scoped_keys(&mut self, catalog: &Catalog, doing: &str) -> Option<Vec<String>> {
        match self.batch_variants(catalog) {
            Ok(keys) if keys.is_empty() => {
                self.error = Some(format!(
                    "一行都没勾。在列表里勾几行，或者按表头那个全选——\
                     {doing}的作用范围就是勾中的那一批。"
                ));
                None
            }
            Ok(keys) => Some(keys),
            Err(error) => {
                self.error = Some(format!("中立库读不动：{error}"));
                None
            }
        }
    }

    /// 动完之后：整屏重读一遍，再把核心库交回来的那本账**原样印出来**。
    ///
    /// **两种锚各说一句**（验收第 7 条）：拿不到内容判据的那些只钉得住本机的位置，
    /// 挪了地方收藏会飘。含糊成一句「收藏了 N 个」，人就会以为每一条都稳
    /// （ADR-0021 那条纪律在这一屏上的样子）。
    fn settle(&mut self, site: &mut Site, name: &str, applied: Applied, joining: bool) {
        // 合集这一维的可选值、以及 `收藏=是` 筛出来的那批都变了——整屏重读。
        self.refresh(site);
        self.standing_for = None;
        let 这一下 = if joining { "放进" } else { "拿出" };
        let mut line = format!(
            "「{name}」{这一下} {} 个变体（真动了 {} 条）。",
            thousands(applied.touched() as u64),
            thousands(applied.changed as u64),
        );
        if joining && applied.path > 0 {
            line.push_str(&format!(
                "其中 {} 个只钉得住本机的路径——那些变体拿不到内容判据（无判据那一档），\
                 改名或挪到别的目录就认不出来了；另外 {} 个钉在内容上，\
                 重扫、改名、挪目录都还认得出。",
                thousands(applied.path as u64),
                thousands(applied.content as u64),
            ));
        } else if joining {
            line.push_str("全部钉在内容上——重扫、改名、挪目录都还认得出。");
        }
        if applied.missing > 0 {
            line.push_str(&format!(
                "另有 {} 个键在中立库里已经不在了，跳过。",
                thousands(applied.missing as u64),
            ));
        }
        self.notice = Some(line);
        self.error = None;
    }

    /// 选中的变体换了就重算一次它的合集落点。没换过是空操作。
    fn sync_standing(&mut self, site: &Site) {
        if self.standing_for.as_deref() == self.variant.as_deref() {
            return;
        }
        self.standing_for = self.variant.clone();
        self.standing = match &self.variant {
            None => Vec::new(),
            Some(key) => match collection::standing(site, key) {
                Ok(rows) => rows,
                Err(error) => {
                    self.error = Some(format!("{error}"));
                    Vec::new()
                }
            },
        };
    }

    /// 缓着的那份压制清单，**只在它确实是这个作品的时候才算数**。
    ///
    /// [`Self::sync_suppressed`] 每帧开头才跑一趟，而点开一行是在同一帧的画表格过程中
    /// 改 [`Self::detail`] 的——换作品的那一帧，缓着的还是**上一个作品**的那几条。
    /// 只读展示错一帧无伤，可这一栏每行挂着一颗**「恢复」**：按下去撤的会是另一个作品
    /// 的记录。所以这里比一次，对不上就当这个作品一条压制都没有，下一帧自然就对了。
    fn suppressed_of(&self, work: &str) -> &[TitleSuppression] {
        if self.suppressed_for.as_deref() == Some(work) {
            &self.suppressed
        } else {
            &[]
        }
    }

    /// 点开的那个变体换了作品就重读一次它**压掉的叫法**。没换过是空操作。
    ///
    /// **不每帧查一次库**：这是画帧线程，而沉淀库那份连接撞上写锁要等十秒
    /// （同 [`Self::sync_standing`] 的道理）。压过一条之后由那个动作把
    /// [`Self::suppressed_for`] 清掉，下一帧自然重读。
    fn sync_suppressed(&mut self, site: &Site) {
        let work = self.detail.as_ref().and_then(|detail| detail.work.clone());
        if self.suppressed_for == work {
            return;
        }
        // **换一个作品就把「撤掉了压制」那一句连它那颗按钮收掉**：人已经不在刚才那条
        // 叫法上了，那颗「折标题」摆在这儿只会让人以为它说的是眼下这一个。
        self.lift_notice = None;
        self.suppressed_for = work.clone();
        self.suppressed = match &work {
            None => Vec::new(),
            Some(work) => match site.store.title_suppressions_of(work) {
                Ok(rows) => rows,
                Err(error) => {
                    self.error = Some(format!("沉淀库读不动：{error}"));
                    Vec::new()
                }
            },
        };
    }

    // ── 右键菜单与键盘（票 `gui-looks-like-the-design/14`） ───────────────────────

    /// **这一帧右键按在哪一行**，就贴着那一下摊开一层菜单。
    ///
    /// 表格与卡片墙各自认出那一下、落一份 [`crate::table::RightClicked`] 进来，
    /// 这里取走。菜单要的那几个事实**在这一下问一次**，不是每帧问一遍：
    /// 「收没收藏」是一次读库（[`collection::favorite_of`]），摊开的那几秒里它不会变。
    fn settle_menu(&mut self, ctx: &egui::Context, site: &Site) {
        let Some(按的) = self.menu_click.take() else {
            return;
        };
        // **右键那一行跟着点开**（设计稿 `S.sel=i; S.varSel=0; render()`）：菜单上那几项
        // 动的就是它，侧边详情摆的也得是它。表格那一路已经把高亮挪过去了。
        self.open_work(&site.catalog, &按的.row.anchor);
        let keys = self.row_keys(&site.catalog, &按的.row.anchor);
        // **收没收藏由核心库答**（ADR-0024）：与作品详情页状态块那一行照的是同一处。
        // 读不动就当没收藏——菜单上那一项照旧按得动，按下去那条路自己会说话。
        let favorited = collection::favorite_of(site, &keys)
            .unwrap_or(None)
            .is_some();
        self.menu.open(
            ctx,
            menu::Facts {
                at: 按的.at,
                title: crate::table::row_name(&按的.row, &self.rules)
                    .text()
                    .to_owned(),
                picked: self.picked.contains(&按的.row.anchor),
                merging: self.merge_rows_with(&按的.row.anchor).len() as u64,
                anchor: 按的.row.anchor,
                favorited,
            },
        );
    }

    /// 菜单里「合并」那一项**要带上哪几行**：勾中的那一批，连光标底下这一行算在内。
    ///
    /// **屏上写的那句话与按下去真合的那一批出自这一处**（挂单 `Q1148`）：菜单上写着
    /// 「合并勾选的 N 个作品…」，按下去开的向导里就该是那 N 个——两处各数一遍，迟早不一样
    /// （ADR-0024）。
    ///
    /// **全选那一档只带这一行**（设计稿 `openMerge(S.pickAll?[i]:…)`）：「全选」不是一批身份，
    /// 是一个筛选条件，而合并要人逐个核对变体（[`Self::open_merge`]）。
    fn merge_rows_with(&self, anchor: &WorkAnchor) -> Vec<WorkAnchor> {
        let Scope::Rows(rows) = self.picked.scope() else {
            return vec![anchor.clone()];
        };
        let mut rows = rows.to_vec();
        if !rows.contains(anchor) {
            rows.push(anchor.clone());
        }
        rows
    }

    /// 这一行底下那几个变体的键。读不动库时是空的——调用方各自说话。
    fn row_keys(&self, catalog: &Catalog, anchor: &WorkAnchor) -> Vec<String> {
        catalog
            .scoped_variants(&self.query, Scope::Rows(std::slice::from_ref(anchor)))
            .unwrap_or_default()
    }

    /// 画那一层菜单；按下去的那一项当场办。
    fn menu_ui(&mut self, ctx: &egui::Context, site: &mut Site, tasks: &mut Tasks) {
        let Some((pressed, facts)) = self.menu.ui(ctx) else {
            return;
        };
        self.apply_menu(ctx, site, tasks, pressed, &facts);
    }

    /// 菜单上按下去的那一项。
    ///
    /// **每一项走的都是屏上别处那颗按钮走的同一个入口**：打开详情走 [`Self::open_work`]、
    /// 编辑元数据走作品详情页那一处、刮削走那层弹层、在文件系统中打开走
    /// [`Self::reveal_row`]。菜单不另开一条路——另开一条，两条迟早不一样
    /// （ADR-0005：界面不许自己长出第二份判断）。
    fn apply_menu(
        &mut self,
        ctx: &egui::Context,
        site: &mut Site,
        tasks: &mut Tasks,
        pressed: menu::Pressed,
        facts: &menu::Facts,
    ) {
        let anchor = &facts.anchor;
        match pressed {
            menu::Pressed::OpenDetail => {
                self.open_work(&site.catalog, anchor);
                self.open_page(work::Tab::Overview);
            }
            menu::Pressed::EditMeta => self.edit_meta_of(&site.catalog, anchor),
            menu::Pressed::TogglePick => self.picked.toggle(anchor),
            menu::Pressed::ToggleFavorite => self.toggle_favorite_of(site, tasks, anchor),
            menu::Pressed::Merge => {
                let rows = self.merge_rows_with(anchor);
                self.open_merge_rows(site, &rows);
            }
            menu::Pressed::Scrape => {
                let keys = self.row_keys(&site.catalog, anchor);
                if keys.is_empty() {
                    self.notice = Some("这个作品底下一个变体都没有，没什么可刮的。".to_owned());
                } else {
                    let shown = u64::try_from(keys.len()).unwrap_or(u64::MAX);
                    self.scrape.open(keys, shown);
                }
            }
            menu::Pressed::Reveal => self.reveal_row(site, anchor),
            // **复制的是菜单顶上那一行写着的那个名字**：屏上摆着什么就复制什么
            // （设计稿 `toast(\`已复制：${w.t}\`)`）。
            menu::Pressed::CopyName => {
                ctx.copy_text(facts.title.clone());
                self.notice = Some(format!("已复制：{}", facts.title));
            }
        }
    }

    /// 「**编辑元数据**」：打开这个作品的详情页、停在元数据那一面、当场进编辑态。
    /// 与详情页头上那颗按钮走的是同一处（`work::Screen::begin_meta_edit`）。
    fn edit_meta_of(&mut self, catalog: &Catalog, anchor: &WorkAnchor) {
        self.open_work(catalog, anchor);
        self.open_page(work::Tab::Metadata);
        self.begin_meta_edit();
    }

    /// 「**在文件系统中打开**」：这一行底下头一个变体所在的目录。
    ///
    /// 键折回盘上真名走核心库那一处（`Roots::real_path`，ADR-0020），与作品详情页头上
    /// 那颗按钮同一个函数（`Screen::reveal`）。**只读**，一个字节都不碰（ADR-0004）。
    fn reveal_row(&mut self, site: &Site, anchor: &WorkAnchor) {
        let keys = self.row_keys(&site.catalog, anchor);
        let Some(key) = keys.first().cloned() else {
            self.notice = Some("这个作品底下一个变体都没有，盘上没有对应的位置。".to_owned());
            return;
        };
        self.reveal(site, &key);
    }

    /// 「**收藏 / 取消收藏**」这一行：范围就是这一行底下那几个变体。
    ///
    /// **收没收藏由核心库答**（[`collection::favorite_of`]，ADR-0024），不是界面自己记一个
    /// 标志；排活、落库、那本账与屏头那颗 ★ 走的是同一条路（[`Self::queue_collection_keys`]）。
    fn toggle_favorite_of(&mut self, site: &Site, tasks: &mut Tasks, anchor: &WorkAnchor) {
        let keys = self.row_keys(&site.catalog, anchor);
        if keys.is_empty() {
            self.notice = Some("这个作品底下一个变体都没有，没什么可收藏的。".to_owned());
            return;
        }
        let 收着的 = match collection::favorite_of(site, &keys) {
            Ok(收着的) => 收着的,
            Err(error) => {
                self.error = Some(format!("{error}"));
                return;
            }
        };
        let joining = 收着的.is_none();
        let doing = if joining { "加收藏" } else { "取消收藏" };
        self.queue_collection_keys(site, tasks, FAVORITE, joining, keys, doing);
    }

    /// **开一层合并向导，就这一处。** 屏头那颗「合并作品…」（勾中那一批，
    /// [`Self::open_merge`]）、作品详情页头上那颗「合并…」（[`Self::open_merge_here`]）、
    /// 右键菜单里那一项，三处交进来的只是**带上哪几行**不同。
    ///
    /// 拦在前头的那几条（勾不够两个、全选那一档开不了）归 [`Self::open_merge`]：那是
    /// 「屏头那颗按下去算不算数」，不是「向导怎么开」。
    pub fn open_merge_rows(&mut self, site: &Site, rows: &[WorkAnchor]) {
        self.merging = Some(merge::Wizard::open(
            &site.catalog,
            &self.query,
            &self.rules,
            &self.priorities,
            rows,
        ));
    }

    // ── 键盘那几下（`App::shortcuts` 调，票 `gui-looks-like-the-design/14`） ──

    /// **高亮往上／往下挪一行**（`↑` `↓`）。一行都没高亮时落在头一行上。
    ///
    /// 只在**表格**那一路走得动：卡片墙背后那扇窗是另一份查询（只看有封面的那一批），
    /// 下标不在同一个空间里（挂单 `Q1142`）。
    pub fn step_focus(&mut self, catalog: &Catalog, 往下: bool) {
        let 总数 = self.window.total();
        if 总数 == 0 {
            return;
        }
        let at = match self.focused {
            None => 0,
            Some(at) if 往下 => (at + 1).min(总数 - 1),
            Some(at) => at.saturating_sub(1),
        };
        self.focused = Some(at);
        self.scroll_focused_into_view();
        if let Some(anchor) = self.window.row(catalog, at).map(|row| row.anchor.clone()) {
            self.open_work(catalog, &anchor);
        }
    }

    /// 挪到的那一行要看得见：交给表格那一层下一帧滚过去。
    fn scroll_focused_into_view(&mut self) {
        // 表格那一层每帧按 `focused` 画选中底色；滚过去由 egui 的 `scroll_to_rect` 办，
        // 而那要拿得到那一行的矩形——只有画它的那一帧才有。这里留个记号就够了。
        self.scroll_to_focused = true;
    }

    /// **打开高亮那一行的作品详情页**（`Enter`）。
    pub fn open_focused(&mut self, catalog: &Catalog) {
        let Some(anchor) = self.focused_anchor(catalog) else {
            return;
        };
        self.open_work(catalog, &anchor);
        self.open_page(work::Tab::Overview);
    }

    /// **勾选 / 取消勾选高亮那一行**（`空格`）。
    pub fn toggle_pick_focused(&mut self, catalog: &Catalog) {
        if let Some(anchor) = self.focused_anchor(catalog) {
            self.picked.toggle(&anchor);
        }
    }

    /// **全选筛出来的那一批**（`⌘/Ctrl+A`）：选中集就是当前这个筛选本身（ADR-0016）。
    pub fn select_all(&mut self) {
        self.picked.select_all();
    }

    /// **收藏 / 取消收藏高亮那一行**（`F`）。
    pub fn toggle_favorite_focused(&mut self, site: &Site, tasks: &mut Tasks) {
        let Some(anchor) = self.focused_anchor(&site.catalog) else {
            return;
        };
        self.toggle_favorite_of(site, tasks, &anchor);
    }

    /// **编辑高亮那一行的元数据**（`E`）。
    pub fn edit_focused(&mut self, catalog: &Catalog) {
        if let Some(anchor) = self.focused_anchor(catalog) {
            self.edit_meta_of(catalog, &anchor);
        }
    }

    /// 高亮那一行是谁；一行都没高亮（或者读不到）时是 `None`。
    fn focused_anchor(&mut self, catalog: &Catalog) -> Option<WorkAnchor> {
        let at = self.focused?;
        self.window.row(catalog, at).map(|row| row.anchor.clone())
    }

    /// **把光标放进搜索框**（`⌘/Ctrl+F`）：留个记号，画那一框的那一帧落实。
    ///
    /// 不在这儿直接 `request_focus`：那一框这一帧还没画出来（筛选栏收起来时连画都不画），
    /// 而焦点只给得了已经在这一帧里摆过的控件。
    pub fn focus_search(&mut self) {
        self.focus_search = true;
    }

    /// 画一帧。
    pub fn ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        self.sync_window(&site.catalog);
        self.sync_standing(site);
        self.sync_suppressed(site);
        // **任务台上有活在跑就先不写库**：那时后台正拿着另一份写得动的连接（扫描），
        // 这条线程上的写会在 `busy_timeout` 上等最长十秒——那是画帧线程的十秒。
        let writable = !tasks.busy();
        self.sync_media(ui.ctx(), site, writable);
        // **刮削是一层弹层**（[`crate::dialog`]），不占这一屏的地方：摊开着才画，盖在整屏上头。
        self.scrape.show(ui.ctx(), site, tasks);
        // **合并向导与「移出此作品」也是弹层**：盖在整屏上头，三栏与作品详情页都由这一处画。
        self.merge_ui(ui.ctx(), site);
        // **右键菜单**（票 `gui-looks-like-the-design/14`）：上一帧表格或卡片墙认下的那一下，
        // 这一帧摊开、这一帧画。两句挨着摆，按下右键与菜单出现之间才只差一帧。
        self.settle_menu(ui.ctx(), site);
        self.menu_ui(ui.ctx(), site, tasks);
        self.notice_toast(ui.ctx(), site);
        // **作品详情页开着就只画它**（票 `gui-looks-like-the-design/15`）：稿上它盖住整块屏。
        if self.page.is_some() {
            self.page_ui(ui, site);
            return;
        }
        // **三栏：左筛选 / 中表格 / 右详情**（票 `gui-looks-like-the-design/09`）。左右两栏
        // 从顶到底，拖得动、收得起来、下次打开还记得；怎么拖、收起来长什么样、记在哪儿，全在
        // [`crate::layout`] 那一份声明里（票 `gui-redesign/12`）。两栏的底色与内边距照稿：
        // 左栏次级底色（`.fpane`）、右栏面板底色（`.dpane`），正中那一栏是窗口底色。
        let tokens = Tokens::builtin();
        let (左栏底, 右栏底, 正中底) = (
            ui.visuals().faint_bg_color,
            ui.visuals().window_fill,
            ui.visuals().panel_fill,
        );
        let 左栏 = egui::Frame::new()
            .fill(左栏底)
            .inner_margin(egui::Margin::from(egui::vec2(
                tokens.space.filter_pane_padding,
                tokens.space.filter_pane_padding,
            )));
        let 右栏 = egui::Frame::new()
            .fill(右栏底)
            .inner_margin(egui::Margin::from(egui::vec2(
                tokens.space.detail_pane_padding,
                tokens.space.detail_pane_padding,
            )));
        // **收起之后那根窄条上写着筛了几个条件**（照稿 `.fstrip` 与 `renderFilterCount`，
        // 挂单 `Q806`）：收起来正是「看不见自己筛了什么」最危险的那一刻——屏上少了一半行，
        // 而左栏已经卷起来了。数法在核心库一处（`WorkQuery::filter_count`），
        // **与筛空时那句空态印的是同一个数**。
        //
        // **一个都没有时照稿写「无条件」**，不是什么都不写：窄条上空着读起来是
        // 「这一栏没话说」，而它其实有话说——「眼下没筛」。
        let 筛了几个 = self.query.filter_count();
        let 窄条上 = if 筛了几个 > 0 {
            format!("{筛了几个}个条件")
        } else {
            "无条件".to_string()
        };
        layout::FILTER.show_collapsible(ui, "筛选", Some(&窄条上), 左栏, |ui| {
            self.filter_panel(ui, site, tasks);
        });
        layout::DETAIL
            .show_collapsible(ui, "详情", None, 右栏, |ui| self.detail_panel(ui, site));
        // **底下那块编辑面板拆掉了**（票 `gui-looks-like-the-design/15` 收挂单 `Q804`）：改元数据归作品详情页，
        // 选中数在表格上方那一条，改选择时的例外摆在右栏选中那张变体卡底下。正中那一栏从上到下就是表。
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(正中底))
            .show(ui, |ui| {
                // 「列表」那一条、表头、表身一块接一块，中间不留缝（设计稿 `.tpane`）。
                ui.spacing_mut().item_spacing.y = 0.0;
                if self.sample {
                    self.font_sample(ui);
                    ui.separator();
                }
                // **按下去要干的事在这一层做**：那一组摆在哪一行要先量宽再定，
                // 而那段摆位的活不该连着两份库的可变借用一起拖进去。
                if let Some(按了) = self.list_bar(ui) {
                    match 按了 {
                        Action::Scrape => self.open_scrape(&site.catalog),
                        Action::Favorite => self.favorite(site, tasks),
                        Action::Merge => self.open_merge(site),
                    }
                }
                let opened = match self.view {
                    BrowseView::Table => Table {
                        catalog: &site.catalog,
                        window: &mut self.window,
                        query: &mut self.query,
                        focused: &mut self.focused,
                        picked: &mut self.picked,
                        scroll_to: self.scroll_to,
                        rules: &self.rules,
                        shelf: if self.list_covers {
                            Some(&mut self.shelf)
                        } else {
                            None
                        },
                        menu: &mut self.menu_click,
                        scroll_focused: std::mem::take(&mut self.scroll_to_focused),
                    }
                    .show(ui),
                    BrowseView::Cards => {
                        self.card_grid(ui, &site.catalog)
                            .map(|row| crate::table::Opened {
                                row,
                                index: 0,
                                page: false,
                            })
                    }
                };
                // **画完表才问封面**：这一帧画到了哪几行，表画完才知道。
                if self.list_covers || self.view == BrowseView::Cards {
                    self.shelf
                        .sync(ui.ctx(), &mut site.catalog, self.pool.as_ref(), writable);
                }
                // **一行都没有时说清为什么空着**：表头照旧在（排序、全选都还点得着），
                // 空态那一句摆在表头底下（照稿 `.empty` 是表里的一行）。
                if self.window.total() == 0 && self.window.error().is_none() {
                    self.empty_state(ui);
                }
                if let Some(opened) = opened {
                    self.open_work(&site.catalog, &opened.row.anchor);
                    // **双击的打开作品详情页**，停在概览（设计稿双击一行 → 概览）。
                    if opened.page {
                        self.open_page(work::Tab::Overview);
                    }
                }
            });
    }

    /// 上一次动作的回执照稿浮在窗口底边那条**提示条**上（[`crate::toast`]）：三栏与作品详情页都走这一处。
    /// 回执换了一句就换一条提示条；停够了收起，那句回执也跟着收掉（[`Self::notice`] 回到 `None`）。
    fn notice_toast(&mut self, ctx: &egui::Context, site: &mut Site) {
        let Some(说的) = self.notice.as_deref() else {
            self.toast = None;
            self.just_landed = None;
            return;
        };
        if self.toast.as_ref().is_none_or(|toast| toast.text() != 说的) {
            // **刚落下一批裁决时那条提示条上多一颗「撤销」**（设计稿 `doMerge` / `doSplit`
            // 末尾那一下）：刚按完那一秒是最可能改主意的一秒，而那时人还没想到要去裁决记录里找。
            self.toast = Some(match self.just_landed {
                Some(_) => Toast::new(说的).action(UNDO),
                None => Toast::new(说的),
            });
        }
        let Some(toast) = self.toast.as_mut() else {
            return;
        };
        match toast.show(ctx) {
            toast::Shown::Showing => {}
            toast::Shown::Expired => {
                self.toast = None;
                self.notice = None;
                self.just_landed = None;
            }
            toast::Shown::Pressed => {
                self.toast = None;
                if let Some(batch) = self.just_landed.take() {
                    self.undo_batch(site, batch);
                }
            }
        }
    }

    /// 撤掉刚落下的那一**批裁决**：提示条上那颗「撤销」按的就是它。
    ///
    /// **走的是既有那条按批撤销的路**（`triage::undo_batch`，与待确认屏的**裁决记录**、
    /// 命令行 `romcat triage undo` 同一处）：沉淀库与中立库两边一起退回这一批落下之前的样子。
    /// 别名、第三步选的字段值与首选变体**不跟着撤**（挂单 `Q1012`）——屏上第三步那一段
    /// 已经把这句话说全了。
    fn undo_batch(&mut self, site: &mut Site, batch: i64) {
        match romcat_core::triage::undo_batch(&mut site.catalog, &mut site.store, batch) {
            Ok(账) => {
                self.notice = Some(format!(
                    "已撤销：{} 个变体回到原来的作品。别名、选过的字段值与首选变体留着。",
                    thousands(账.variants)
                ));
                self.after_regroup(site);
            }
            Err(failed) => self.error = Some(format!("撤不掉：{failed}")),
        }
    }

    /// **主列表一行都没有时**正中那一栏摆什么（票 `gui-looks-like-the-design/09`）。
    ///
    /// 三种空各说各的，因为下一步不一样：
    ///
    /// - **筛选把行筛没了**：说清楚，旁边给一颗「清除筛选」——走的是左栏「清除」同一个入口
    ///   （[`Self::clear_filter`]），搜索框照旧不动。
    /// - **只有搜索框里有字**：说没搜到什么、搜的是哪几条路。
    /// - **什么都没筛**：库里本来就没有能列的；全是收起着的非游戏资产时点名说。
    ///
    /// 哪些字段算筛选由核心库答（[`WorkQuery::same_filter`]），收起了几个也是核心库数的——
    /// 这一层只挑一句话。
    fn empty_state(&mut self, ui: &mut egui::Ui) {
        let steps = &Tokens::builtin().space.steps;
        let search = self.query.search.trim().to_string();
        let 只搜了 = WorkQuery {
            search: self.query.search.clone(),
            ..WorkQuery::default()
        };
        let 筛过 = !self.query.same_filter(&只搜了);
        let 收起的 = self.non_game_assets.unwrap_or(0);
        // 空态那一块上方的留白照稿 `.empty`。
        ui.add_space(Tokens::builtin().space.empty_padding);
        ui.vertical_centered(|ui| {
            if 筛过 {
                // **空态那句带上条件数**（照稿 `.empty` 那句，挂单 `Q806`；数法见
                // `WorkQuery::filter_count`）：「筛不出东西」与「筛了几样才筛不出东西」是
                // 两件事——人得先知道自己叠了几层，才知道该松哪一层。**这个数与收起后那根
                // 窄条上的是同一个**。
                //
                // **数出来是 0 就不写那个数**：`筛过` 问的是 `WorkQuery::same_filter`，
                // 它把搜索词与「只显示有封面的」也算进「筛过」，而那两样按定下来的口径
                // **不算条件**（搜索管顺序不管集合；封面是卡片墙的临时呈现）。于是只拨了
                // 「只显示有封面的」而筛空时，两处问的不是同一件事，照写就会在屏上印出
                // 「没有符合当前 0 个筛选条件的作品」——一句自相矛盾的话。
                let 几个 = self.query.filter_count();
                if 几个 > 0 {
                    ui.weak(format!("没有符合当前 {几个} 个筛选条件的作品。"));
                } else {
                    ui.weak("没有符合当前筛选条件的作品。");
                }
                ui.add_space(steps[1]);
                let 清除 = look::small_buttons(ui, |ui| {
                    ui.button("清除筛选")
                        .on_hover_text(
                            "把左栏的条件全部清掉，与左栏那颗「清除」是同一下。搜索框不动。",
                        )
                        .clicked()
                });
                if 清除 {
                    self.clear_filter();
                }
            } else if !search.is_empty() {
                ui.weak(format!(
                    "没有找到「{search}」。搜的是屏上的名字、别的叫法与简介。"
                ));
            } else if 收起的 > 0 {
                ui.weak(format!(
                    "库里能列的只有 {} 个非游戏资产，默认收起着——在左栏打开「显示非游戏资产」看。",
                    thousands(收起的),
                ));
            } else {
                ui.weak("库里还没有能列出来的东西。");
            }
        });
    }

    /// 工具条右端那一组**批量操作**要多宽（设计稿 `.tbar .acts`）。
    ///
    /// **在 `look::small_buttons` 那一块外头量**（`look::small_button_width` 的文档写着
    /// 为什么）：那一块是个 `ui.scope`，哪怕里头什么都不摆，它也在横排里占一格间距——
    /// 进去再量，量的那一下就把后面摆的东西往右推了一格。
    ///
    /// **字照按钮取字的规矩取**：egui 0.36 的按钮只认 `override_font_id`，
    /// `TextStyle::Button` 那一格它根本不读（`look::sized_buttons` 的文档）。头一版在这儿
    /// 拿 `TextStyle::Button` 量，量的是正文那一档 13、画的是小号 12，三颗十来个字**高估
    /// 十点上下**——整组右端贴不住右沿，而且比该折的时候早折。
    fn action_width(ui: &egui::Ui) -> f32 {
        let 间距 = ui.spacing().item_spacing.x;
        ACTIONS
            .iter()
            .map(|一颗| look::small_button_width(ui, 一颗.label()))
            .sum::<f32>()
            + 间距 * (ACTIONS.len() as f32 - 1.0)
    }

    /// 画那一组，交回**按了哪一颗**。
    ///
    /// **只画、不动库**：按下去要干的事（摊开刮削弹层、放进收藏、开合并向导）由
    /// [`Self::ui`] 那一层去做。这么分是因为这一组摆在哪一行要先量宽再定
    /// （[`Self::list_bar`]），而那段摆位的活不该连着两份库的可变借用一起拖进来。
    fn action_buttons(ui: &mut egui::Ui) -> Option<Action> {
        look::small_buttons(ui, |ui| {
            let mut 按了 = None;
            for 一颗 in ACTIONS {
                if ui.button(一颗.label()).on_hover_text(一颗.hint()).clicked() {
                    按了 = Some(一颗);
                }
            }
            按了
        })
    }

    /// 主列表上方那一条（设计稿 `.tbar` 与 `.cbar` 两层叠在一个框里）：次级底色、
    /// 底下一条分隔线。
    ///
    /// **第一层照稿是 `.tbar`**：视图切换、「N 个作品（共 M）」那一句
    /// （[`Self::count_line`]）、选中数与「清除选择」一路从左往右，那几颗批量操作
    /// **整组靠右**（[`Self::action_bar`]）。**第二层是 `.cbar`**：当前这种呈现方式
    /// 自己的那几样（表格是「在每行开头显示封面」，卡片是分组／排序／大小）。
    ///
    /// ⚠️ **「N 个作品（共 M）」这一票挪回了左边**（挂单 `Q1102`）。票 09 把它摆在这一条的
    /// **右端**（挂单 `Q876`，拿主意的人 2026-09-14 定；同一处落点 09-15 又确认过一次），
    /// 当时的依据写着「批量按钮挪进了屏头，稿上那一条只剩这一句」——**那几颗按钮这一票
    /// 挪回来了，依据跟着不成立**，而稿上 `.cnt` 明写着紧跟在 `.seg` 后头。
    fn list_bar(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let tokens = Tokens::builtin();
        let [上下, 左右] = tokens.space.list_bar_padding;
        let 线 = ui.visuals().widgets.noninteractive.bg_stroke;
        let 这一句 = self.count_line(ui);
        let 有封面 = self.cover_window.total();
        let 作品总数 = self.window.total();
        let mut 按了 = None;
        let 这一条 = egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing =
                    egui::vec2(tokens.space.list_bar_gap, look::step(0));
                ui.vertical(|ui| {
                    // **照稿这一条的次序**（`.tbar`）：视图切换、作品数、选中数与「清除选择」
                    // 一路从左往右，那几颗批量操作**整组靠右**（`.acts` 的 `margin-left:auto`）。
                    //
                    // ## 摆不下的时候整组换一行——**自己分行，不交给 egui 折**
                    //
                    // 稿上是 `.tbar{flex-wrap:wrap}` 加 `.acts{flex-wrap:nowrap}`。
                    // 头一版想拿 `horizontal_wrapped` 加「把这一行剩下的宽占掉」去逼它折行，
                    // **那是错的**：`add_space` 只把光标往前推，不触发换行，于是那三颗被摆到
                    // 行外、连画都没画出来（合并向导那条测试当场红——**屏上压根没有「合并作品…」**）。
                    // 那正是票 `10` 栽过的头一条路。
                    //
                    // 所以这儿**先量后分行**：量出那一组要多宽（[`Self::action_width`]），
                    // 这一行的剩余宽度装得下就摆同一行的右头，装不下就另起一行、在那一行里靠右。
                    // 两条路都是普通的 `ui.horizontal`，摆哪儿由算出来的数说了算，不靠布局器的脾气。
                    let 要多宽 = Self::action_width(ui);
                    let mut 同一行摆得下 = true;
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(self.view == BrowseView::Table, "表格")
                            .clicked()
                        {
                            self.view = BrowseView::Table;
                        }
                        if ui
                            .selectable_label(self.view == BrowseView::Cards, "卡片")
                            .clicked()
                        {
                            self.view = BrowseView::Cards;
                        }
                        // **作品数照稿紧跟在视图切换后头**（`.tbar` 里 `.seg` 之后就是
                        // `.cnt`），不在这一条的右端。票 09 把它摆到右端，理由是
                        // 「批量按钮挪进了屏头，稿上那一条只剩这一句」（挂单 `Q876`）
                        // ——那几颗按钮这一票挪回来了，那条理由跟着不成立。
                        ui.label(这一句.clone());
                        if self.picked.count(self.window.total()) > 0
                            && look::small_buttons(ui, |ui| {
                                ui.scope(|ui| {
                                    look::ghost_button(ui.visuals_mut());
                                    ui.button(CLEAR_PICK)
                                })
                                .inner
                                .clicked()
                            })
                        {
                            self.picked.clear();
                        }
                        // 比的时候松一像素：宽是浮点算出来的，差一丝就白换一行，
                        // 而白换一行比挤一丝难看得多。**只松在「换不换行」这一步**，
                        // 靠右那一步照旧按算出来的数垫，所以松这一点不会把按钮推出行外。
                        同一行摆得下 = ui.available_width() + 1.0 >= 要多宽;
                        if 同一行摆得下 {
                            ui.add_space((ui.available_width() - 要多宽).max(0.0));
                            按了 = Self::action_buttons(ui);
                        }
                    });
                    if !同一行摆得下 {
                        ui.horizontal(|ui| {
                            ui.add_space((ui.available_width() - 要多宽).max(0.0));
                            按了 = Self::action_buttons(ui);
                        });
                    }
                    ui.add_space(look::step(1));
                    // 第二层才是当前呈现方式的控制。卡片不会再和视图、计数争一行。
                    ui.horizontal_wrapped(|ui| {
                        if self.view == BrowseView::Table {
                            ui.checkbox(
                                &mut self.list_covers,
                                egui::RichText::new("在每行开头显示封面")
                                    .size(look::font_size(ui.ctx(), tokens.font.size_small_plus)),
                            );
                            look::help(ui, "双击一行打开作品详情");
                            if let Some(说的) = &self.error {
                                ui.colored_label(ui.visuals().error_fg_color, 说的);
                            }
                        } else {
                            look::section(ui, "分组");
                            if ui.selectable_label(!self.group_cards, "不分组").clicked() {
                                self.group_cards = false;
                                self.card_group_header = None;
                            }
                            if ui.selectable_label(self.group_cards, "按平台").clicked() {
                                self.group_cards = true;
                                self.card_group_header = None;
                                // 分组的次序由中立库排序，不能只把当前页的卡片在界面里重排。
                                self.query.order = WorkOrder::Platform;
                            }
                            look::section(ui, "排序");
                            // **「默认」与「名称」是两档，不是一档**（照稿 `#csort`，
                            // 挂单 `Q1098`）：眼下是不是默认那一种由核心库答
                            // （`WorkQuery::sorted_by_default`），这一层只照着显示——
                            // 界面自己再比一遍 `order == Name` 的话，从表头倒着排过来的
                            // `(作品, 倒着)` 会在这儿显示成「默认」，而库里排的并不是它
                            // （ADR-0024）。
                            let mut 选了: Option<Option<WorkOrder>> = None;
                            let 眼下 = if self.group_cards || self.query.sorted_by_default() {
                                // 平台分组本身就是主排序，组内不另排——界面上同样叫「默认」。
                                None
                            } else {
                                Some(self.query.order)
                            };
                            egui::ComboBox::from_id_salt("卡片排序")
                                .selected_text(card_order_label(眼下))
                                .show_ui(ui, |ui| {
                                    for one in std::iter::once(None).chain(WorkOrder::ALL.map(Some))
                                    {
                                        if ui
                                            .selectable_label(眼下 == one, card_order_label(one))
                                            .clicked()
                                        {
                                            选了 = Some(one);
                                        }
                                    }
                                });
                            // **分组着的时候点「默认」＝什么都不做**：那一档此刻显示的就是
                            // 它，而按平台分组本身就是主排序。不挡这一下的话，人点一下
                            // 当前显示的那一项，分组会**静悄悄**关掉（票 09 原先专门留了
                            // 一支挡它，这一票重做下拉时漏掉了，`/code-review` 抓出来的）。
                            if 选了 == Some(None) && self.group_cards {
                                选了 = None;
                            }
                            if let Some(one) = 选了 {
                                self.group_cards = false;
                                self.card_group_header = None;
                                match one {
                                    // **「默认」把排法整个按回默认那一种**——方向也回正着，
                                    // 否则它与「名称」是同一个状态，两档就白拆了。
                                    None => {
                                        let 默认 = WorkQuery::default();
                                        self.query.order = 默认.order;
                                        self.query.descending = 默认.descending;
                                    }
                                    // 挑一列只换列，**方向照旧**：从表头倒着排过来的人
                                    // 在这儿挑一列，不该被顺手翻回正着。
                                    Some(order) => self.query.order = order,
                                }
                            }
                            look::section(ui, "大小");
                            for (size, label) in [
                                (CardSize::Small, "小"),
                                (CardSize::Medium, "中"),
                                (CardSize::Large, "大"),
                            ] {
                                if ui.selectable_label(self.card_size == size, label).clicked() {
                                    self.card_size = size;
                                }
                            }
                            ui.checkbox(&mut self.only_covers, "只显示有封面的");
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.weak(format!(
                                    "有封面 {} / {}",
                                    thousands(有封面),
                                    thousands(作品总数)
                                ));
                            });
                        }
                    });
                    if let Some(说的) = self.shelf.error() {
                        ui.colored_label(ui.visuals().error_fg_color, 说的);
                    }
                });
            })
            .response
            .rect;
        ui.painter()
            .hline(这一条.x_range(), 这一条.bottom() - 线.width / 2.0, 线);
        按了
    }

    /// 卡片墙走自己的分页窗：仅封面开关不影响主列表；封面与无封面字卡都复用 [`Shelf`]。
    fn card_grid(
        &mut self,
        ui: &mut egui::Ui,
        catalog: &Catalog,
    ) -> Option<romcat_core::catalog::browse::WorkRow> {
        // 设计稿 `.cgrid`：横向 16、纵向 20；不能借全局控件间距，否则卡片墙会挤成表格。
        const CARD_GAP_X: f32 = 16.0;
        const CARD_GAP_Y: f32 = 20.0;
        let min_width = self.card_size.width();
        // 左右留白属于可滚动内容；滚动条本身必须贴着中栏右边界，不能被留白再往里推。
        let grid_width = (ui.available_width() - 32.0).max(min_width);
        let columns = ((grid_width + CARD_GAP_X) / (min_width + CARD_GAP_X))
            .floor()
            .max(1.0) as u64;
        // 与设计稿 `repeat(auto-fill, minmax(--cw, 1fr))` 同义：档位是最小宽度，余宽由
        // 当前行的所有卡均分。否则第三张卡后会留下比右侧留白大得多的一块空区。
        let width = (grid_width - CARD_GAP_X * (columns - 1) as f32) / columns as f32;
        let cover = egui::vec2(width, width / Tokens::builtin().layout.card_cover_ratio);
        let card_height = cover.y + Tokens::builtin().layout.card_info_height;
        let card_row_height = card_height + CARD_GAP_Y;
        let card_rows = self.card_window.total().div_ceil(columns) as usize;
        let mut opened = None;
        // `.cgrid` 的上内边距：即使不显示组头，工具条与第一排卡也不能贴在一起。
        ui.add_space(14.0);
        if self.group_cards {
            let header = self.card_group_header.clone().or_else(|| {
                self.card_window.row(catalog, 0).map(|row| {
                    let platform = row
                        .platforms
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "未知".into());
                    let count = self
                        .facets
                        .platforms
                        .iter()
                        .find(|facet| facet.value == platform)
                        .map(|facet| facet.count)
                        .unwrap_or(row.variants);
                    (platform, count)
                })
            });
            if let Some((platform, count)) = header {
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    let (dot, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter().rect_filled(
                        dot,
                        Tokens::builtin().radius.small,
                        Tokens::builtin().color.platform.of(&platform),
                    );
                    ui.label(font::strong(&platform));
                    ui.weak(format!("{} 个作品", thousands(count)));
                });
                let separator_y = ui.cursor().top();
                ui.painter().hline(
                    (ui.min_rect().left() + 16.0)..=(ui.max_rect().right() - 16.0),
                    separator_y,
                    ui.visuals().widgets.noninteractive.bg_stroke,
                );
                ui.add_space(look::step(1));
            }
        }
        // `ScrollArea` 默认按内容收缩；卡片恰好排满三列时会把滚动轨留在第三张卡旁边，
        // 看起来像中栏右侧凭空多了一片空白。两轴都禁止收缩，轨道才会贴到详情栏分隔线。
        egui::ScrollArea::vertical()
            .id_salt("卡片墙")
            .auto_shrink([false, false])
            .show_rows(ui, card_row_height, card_rows, |ui, visible| {
                if self.group_cards {
                    let first = visible.start as u64 * columns;
                    if let Some(row) = self.card_window.row(catalog, first) {
                        let platform = row
                            .platforms
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "未知".into());
                        let count = self
                            .facets
                            .platforms
                            .iter()
                            .find(|facet| facet.value == platform)
                            .map(|facet| facet.count)
                            .unwrap_or(row.variants);
                        let header = (platform, count);
                        if self.card_group_header.as_ref() != Some(&header) {
                            self.card_group_header = Some(header);
                            ui.ctx().request_repaint();
                        }
                    }
                }
                for card_row in visible {
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        ui.spacing_mut().item_spacing.x = CARD_GAP_X;
                        for column in 0..columns {
                            let index = card_row as u64 * columns + column;
                            let Some(row) = self.card_window.row(catalog, index).cloned() else {
                                break;
                            };
                            // **认不认得出作品，问表格那一路同一处**（[`unlinked_title`]）：
                            // 判据在核心库（ADR-0024），卡片这边不另写一套，不然有一天两处判得不一样。
                            let 未关联 = unlinked_title(&row, &self.rules);
                            // 这一行屏上叫什么，问表格那一路同一处（`table::row_name`）。
                            let title = crate::table::row_name(&row, &self.rules).text().to_owned();
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(
                                    width,
                                    cover.y + Tokens::builtin().layout.card_info_height,
                                ),
                                egui::Sense::click(),
                            );
                            // 这不是一块只能点鼠标的画布。把整张卡申报为按钮，egui 才会
                            // 把它放进 Tab 顺序，也让辅助技术能读出它是什么作品。
                            response.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::Button,
                                    true,
                                    self.picked.contains(&row.anchor),
                                    &title,
                                )
                            });
                            let mut card = ui.new_child(
                                egui::UiBuilder::new()
                                    .max_rect(rect)
                                    .layout(Layout::top_down(Align::Min)),
                            );
                            // **卡面上的字不许接住点击**（表格那一路早就这么干了，
                            // `table::Table::show`）：egui 的标签默认可选中，会把按在标题或
                            // 那几行小字上的那一下当成选字——于是点在卡面的字上整张卡收不到，
                            // 右键更是摊不开菜单。整张卡才是那个按钮。
                            card.style_mut().interaction.selectable_labels = false;
                            let cover_radius = if self.shelf.has_cover(&row) == Some(true) {
                                Tokens::builtin().radius.medium
                            } else {
                                Tokens::builtin().radius.large
                            };
                            self.shelf.card(&mut card, cover, &row, &title);
                            let chosen = self.picked.contains(&row.anchor);
                            paint_card_overlay(
                                &card,
                                rect,
                                cover,
                                cover_radius,
                                &row,
                                chosen,
                                response.has_focus(),
                            );
                            card.add_space(look::step(2));
                            // 卡面里可以有标题，卡面外仍要有稳定的文字区：滚动时才不会只剩
                            // 一大片色块，也让有封面与无封面卡的扫描节奏一致。
                            card.add(egui::Label::new(font::strong(&title)).truncate());
                            card.weak(format!(
                                "{} · {} 个变体 · {}",
                                row.year.as_deref().unwrap_or("年份未知"),
                                thousands(row.variants),
                                human_bytes(row.bytes)
                            ));
                            card.horizontal(|ui| {
                                // **卡面也挂那枚「未关联作品」**（稿上没画，拿主意的人 2026-09-20 定）：
                                // 与表格那一路同一句词、同一枚标签（[`UNLINKED_LABEL`] 与
                                // [`crate::table::tag`]，照稿 `.tag`）。摆在置信度前头，跟表上标签领着第二行一个位置。
                                // 挂在这一行而不另起一行，是因为卡面下半截高度是定死的
                                // （令牌 `card-info-height`），多一行会把最后一行挤出卡外。
                                if 未关联.is_some() {
                                    crate::table::tag(ui, UNLINKED_LABEL);
                                }
                                ui.colored_label(
                                    look::tier_color(row.tier(), ui.visuals()),
                                    row.confidence_label(),
                                );
                            });
                            // 未选卡只在鼠标靠近时露出选择框；已选卡必须常驻勾选，不能让人移开
                            // 鼠标就看不出哪些卡被选中了。
                            let mut 点了选择 = false;
                            if response.hovered() || chosen {
                                let check_rect = egui::Rect::from_min_size(
                                    rect.min + egui::vec2(8.0, 8.0),
                                    egui::vec2(22.0, 22.0),
                                );
                                let check_fill = if chosen {
                                    ui.visuals().selection.bg_fill
                                } else {
                                    ui.visuals().window_fill.gamma_multiply(0.75)
                                };
                                ui.painter().rect_filled(
                                    check_rect,
                                    Tokens::builtin().radius.small,
                                    check_fill,
                                );
                                ui.painter().rect_stroke(
                                    check_rect,
                                    Tokens::builtin().radius.small,
                                    ui.visuals().widgets.active.bg_stroke,
                                    egui::StrokeKind::Inside,
                                );
                                if chosen {
                                    ui.painter().text(
                                        check_rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "✓",
                                        egui::FontId::proportional(16.0),
                                        ui.visuals().strong_text_color(),
                                    );
                                }
                                // 选择框压在整卡点击区里；egui 只会把那一下归给先注册的整卡。
                                // 因此按整卡响应给出的命中坐标二次判定，而不是再注册一个竞争响应。
                                if response.clicked()
                                    && response
                                        .interact_pointer_pos()
                                        .is_some_and(|pos| check_rect.contains(pos))
                                {
                                    self.picked.toggle(&row.anchor);
                                    点了选择 = true;
                                }
                            }
                            // **右键摊菜单**：与表格那一路认的是同一件事，交出来的也是同一份
                            // （`table::RightClicked`）——菜单上摆哪几项由 [`menu`] 一处说了算。
                            if response.secondary_clicked()
                                && let Some(at) = response.interact_pointer_pos()
                            {
                                response.request_focus();
                                self.menu_click = Some(crate::table::RightClicked {
                                    row: row.clone(),
                                    at,
                                });
                            }
                            if response.clicked() && !点了选择 {
                                response.request_focus();
                                opened = Some(row.clone());
                            }
                            if response.has_focus()
                                && ui.input(|input| input.key_pressed(egui::Key::Enter))
                            {
                                opened = Some(row.clone());
                            }
                            if response.has_focus()
                                && ui.input(|input| input.key_pressed(egui::Key::Space))
                            {
                                self.picked.toggle(&row.anchor);
                            }
                        }
                    });
                }
            });
        opened
    }

    /// 左边那栏，照稿 `.fpane` 从上到下：标题行（「清除」与收起）、搜索框、平台、中文、识别结论、
    /// 收藏与合集、条件组、「显示非游戏资产」那颗开关；稿上没画、这一票之前就有的那几样（取消收藏、
    /// 合集的加减、语言、存成子库）缩成小号垫在最底下（拿主意的人第 6 问的答复，挂单 `Q873`）。
    ///
    /// **两套筛法各管一段**（挂单 `Q73`）：分面标签各自带着条数（`Catalog::facets` 一次
    /// `GROUP BY` 问出来的），那是用来**摸清库里有什么**的；条件组里的值要打出来，那是用来
    /// **说清楚要哪一批**的。没有条数的下拉框，人只能一个个点开试。两套之间是「且」：一层层收窄。
    fn filter_panel(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        // **扔掉一条坏规则那一下不能在画的中途走**：`self.editing` 那会儿还借着。
        // 记下序号，这一栏画完再动手。
        let mut 扔掉 = None;
        // **标题行钉在滚动区外头**：这一栏比一屏长，滚动条浮在右沿，而那颗「收起」正摆在右沿
        // ——摆进滚动区里，按下去那一下落在滚动条上。
        ui.horizontal(|ui| {
            ui.label(font::strong("筛选"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                layout::FILTER.collapse_button(ui);
                let 清除 = look::small_buttons(ui, |ui| {
                    // 幽灵按钮那一档的颜色只有 `look::ghost_button` 一处回答（与任务屏「清空历史」同一种写法）。
                    ui.scope(|ui| {
                        look::ghost_button(ui.visuals_mut());
                        ui.button("清除")
                    })
                    .inner
                    .on_hover_text(
                        "把这一栏的条件全部清掉。排序与搜索框不动——\
                             搜索管排序、筛选器管集合，这颗按钮只管后者。",
                    )
                    .clicked()
                });
                if 清除 {
                    // **按钮体只有这一句。** 把那几行抄在这儿的话，钉着
                    // [`Self::clear_filter`] 的那条测试就钉不到界面上这一下——
                    // 改了这儿它照样绿，而那正是这条修复要防的漂移。
                    self.clear_filter();
                }
            });
        });
        pane_gap(ui);
        egui::ScrollArea::vertical()
            .id_salt("筛选栏")
            .show(ui, |ui| {
                if let Some(editing) = &self.editing {
                    // **正在替谁改，一进屏就看得见**：这一栏的每一下都会落到那个子库上。
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!("正在改子库「{}」的选择集", editing.sublibrary),
                    );
                    look::help(ui, "调完去底下那块面板按「更新到子库」。");
                    扔掉 = Self::broken_rules_ui(ui, &editing.broken);
                    pane_gap(ui);
                }

                // **搜索框**（设计稿 `.field`）：管顺序不管集合，所以它不在条件里、进不了子库的规则。
                let width = ui.available_width();
                let 搜索框 = look::text_input(
                    ui,
                    width,
                    egui::TextEdit::singleline(&mut self.query.search)
                        .id(搜索框())
                        .hint_text("搜索名称、别名或简介"),
                );
                // **`⌘/Ctrl+F` 那一下在这儿落实**（[`Self::focus_search`]）：焦点只给得了
                // 这一帧已经摆过的控件，而这一框在筛选栏收起来时连画都不画。
                if std::mem::take(&mut self.focus_search) {
                    搜索框.request_focus();
                }
                搜索框.on_hover_text(
                    "搜索管排序，筛选器管集合。三条路都找：屏上这个名字、\
                     标题集合里别的叫法（中文名就在这儿）、简介。\
                     命中在哪一条决定这一行排哪一档，权重内置、不用配。\n\
                     它进不了子库的规则——子库要的是集合不是顺序，\
                     「存成子库」之前得先把它清空。",
                );
                // **搜索框里有字的时候才画这一句**（拿主意的人 2026-09-21 定，挂单 `Q1099`）。
                //
                // 两层道理叠在一起：
                //
                // 1. **没搜索的时候它本来就是废话**——「搜索结果默认按匹配程度排序」，
                //    可还没搜。一句当下不成立的话常驻在那儿，读的人得先分辨它说的是不是
                //    此刻，那正是票 `03` 要赶走的东西。
                // 2. 它**要占两行**。这一句得连「默认」一起说（票 `11` 改了口径：搜索着的
                //    时候按匹配程度排的只是**默认那一种**排法，人点过表头就以他点的那一列
                //    为准，见 `WorkQuery::sorted_by_default`），而左栏只有两百来点宽。
                //    常驻两行的话，它底下压着的平台、中文、识别结论、收藏与合集、条件组
                //    一路下移，最底下「语言」那一段被顶出视口。
                //
                // **「怎么回到默认」放悬停**，不往屏上那一行里塞：塞进去就是三行。
                if !self.query.search.trim().is_empty() {
                    look::help(ui, "搜索结果默认按匹配程度排序；点表头可以改成按那一列排。")
                        .on_hover_text(
                            "「默认」那一种排法就是按匹配程度排。点过表头之后以你点的那一列为准；\
                             在同一个表头上点到第三下就回到默认那一种——不必把搜索词删掉重打。",
                        );
                }

                pane_gap(ui);
                section_title(ui, "平台", None)
                    .on_hover_text("变体所属的硬件系统。认不出平台的内容照常入库。");
                section_gap(ui);
                platform_chips(
                    ui,
                    &self.facets.platforms,
                    &mut self.query.platform,
                    &mut self.more_platforms,
                );

                pane_gap(ui);
                section_title(ui, "中文", None)
                    .on_hover_text("变体的中文身份：汉化 / 官中。这个库最要紧的那批全在这儿。");
                section_gap(ui);
                facet_chips(ui, "中文", &self.facets.chinese, &mut self.query.chinese);

                pane_gap(ui);
                section_title(ui, "识别结论", Some("只用于浏览，不写入规则"));
                section_gap(ui);
                let mut state = self.query.state;
                chip_cluster(ui, |ui| {
                    for (filter, count) in &self.facets.states {
                        let on = state == Some(*filter);
                        if look::facet_chip(ui, on, filter.label(), &thousands(*count)).clicked() {
                            state = if on { None } else { Some(*filter) };
                        }
                    }
                });
                self.query.state = state;

                pane_gap(ui);
                section_title(ui, "收藏与合集", None).on_hover_text(
                    "加收藏那一下在屏头（「★ 收藏」），因为它按得最勤：\
                     勾一批、按一下、接着筛下一批。取消收藏与自建合集的加减在这一栏最底下。",
                );
                section_gap(ui);
                // 库里有哪几个合集、各几条（照稿 `#coll-facet`），点一下按它收窄。
                facet_chips(
                    ui,
                    "合集",
                    &self.facets.collections,
                    &mut self.query.collection,
                );

                pane_gap(ui);
                section_title(ui, "条件组", Some("存成子库时就是规则")).on_hover_text(
                    "可嵌套的条件组，每组选「全部满足 / 任一满足 / 都不满足」，组里还能再套组，\
                     九个运算符。上头那几簇标签存成子库时也会折进同一条规则里\
                     （「识别结论」那一簇折不进去）。",
                );
                section_gap(ui);
                if self.rule_box(ui) {
                    // 条件组一改就是换了一批行——同步进查询，`sync_window` 那一趟
                    // 会把窗口作废重取，选中也跟着清掉。
                    self.query.rule = self.filter.rule().cloned();
                }
                // **折出来的那条规则当场写出来**（设计稿 `.ruletext`）：按「存成子库」之前心里有数。
                // 正在改子库的那一趟，底下「更新到子库」那一块自己说规则会变成什么。
                if self.editing.is_none() {
                    match self.query.to_rule() {
                        Ok(Some(rule)) => rule_text(ui, &rule.text),
                        Ok(None) => {}
                        Err(unruly) => {
                            ui.colored_label(ui.visuals().error_fg_color, unruly.advice());
                        }
                    }
                }

                pane_gap(ui);
                self.non_game_asset_switch(ui);

                // ── 稿上没画、这一票之前就有的那几样，缩成小号垫在最底下（挂单 `Q873`）──
                pane_gap(ui);
                self.collection_actions(ui, site, tasks);

                pane_gap(ui);
                section_title(ui, "语言", None).on_hover_text(
                    "发行版标着的语言码。汉化版不在这一维里——它是变体，底版多半是日版。",
                );
                section_gap(ui);
                facet_chips(ui, "语言", &self.facets.languages, &mut self.query.language);

                pane_gap(ui);
                self.save_panel(ui, site);
            });
        // **这一栏画完了再动手**：搁在中途走的话，这一帧余下的半栏是照旧那份数据画的。
        if let Some(ordinal) = 扔掉 {
            self.discard_broken_rule(site, ordinal);
        }
    }

    /// 条件组那个框（设计稿 `.gtree`）：面板底色、分隔线描边、中圆角，里头是 [`Filter::ui`]。
    /// 返回「改过没有」。
    fn rule_box(&mut self, ui: &mut egui::Ui) -> bool {
        let tokens = Tokens::builtin();
        let 留白 = tokens.space.rule_box_padding;
        // **判「这条子句筛不筛得出东西」要的那点上下文**（票 `gui-looks-like-the-design/12`）：
        // 平台认不认得出由**平台清单**说了算（界面一律用内置那一份，挂单 `Q1031`）；
        // 库里有哪几个合集由中立库那份投影说了算（分面那一趟已经问回来了，不另查一遍）。
        // **判在核心库**（`sublibrary::thin`），这一层只把这两样交出去。
        // **内置清单只建一次**：`Manifest::builtin()` 要解一遍 TOML、编一遍那批模式，
        // 而这儿是**画帧线**——每帧建一份的话，光摆着不动也在烧 CPU。
        static 平台清单: std::sync::OnceLock<romcat_core::platform::Manifest> =
            std::sync::OnceLock::new();
        let platforms = 平台清单.get_or_init(romcat_core::platform::Manifest::builtin);
        let collections: Vec<String> = self
            .facets
            .collections
            .iter()
            .map(|facet| facet.value.clone())
            .collect();
        let known = romcat_core::sublibrary::KnownValues {
            platforms,
            collections: &collections,
        };
        egui::Frame::new()
            .fill(ui.visuals().window_fill)
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .corner_radius(tokens.radius.medium)
            .inner_margin(egui::Margin::from(egui::vec2(留白, 留白)))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.filter.ui(ui, &known)
            })
            .inner
    }

    /// 「**显示非游戏资产**」那颗开关，照稿 `.opt`：勾选框上的字是 `size-small-plus`，底下一句小字说
    /// 共有几个、会不会导出（票 `gui-looks-like-the-design/08`；字与位置是拿主意的人照稿定的）。
    ///
    /// **非游戏资产默认收起**。哪几行算、共有几个，都是核心库说的（`WorkQuery::non_game_assets`、
    /// `Catalog::non_game_asset_rows`）——这一层只拨开关、照着印（ADR-0024）。
    fn non_game_asset_switch(&mut self, ui: &mut egui::Ui) {
        const 说明: &str = "BIOS 这类：模拟器要它，它本身不是游戏。默认收起；\
                            打开之后列出来，行上标着「非游戏资产」。\
                            照旧入库、永不导出——这颗开关只管列不列出来。";
        let tokens = Tokens::builtin();
        let mut listed = self.query.non_game_assets == NonGameAssets::Listed;
        // 两档半号字号走 `look::font_size` 取整（票 `gui-looks-like-the-design/32` 定的统一入口）。
        let 字 = egui::RichText::new("显示非游戏资产")
            .size(look::font_size(ui.ctx(), tokens.font.size_small_plus));
        let 小字 = egui::RichText::new(non_game_asset_label(self.non_game_assets))
            .size(look::font_size(ui.ctx(), tokens.font.size_caption_plus))
            .color(ui.visuals().weak_text_color());
        // 照稿 `.opt`：勾选框在左，右边一栏两行——开关上的字、底下那句小字，**两行左沿对齐**。
        // 勾选框自己不带字，字摆在右边那一栏里，点字与点勾选框是同一下。
        let mut 拨了 = false;
        ui.scope(|ui| {
            ui.spacing_mut().interact_size.y = 0.0;
            ui.horizontal_top(|ui| {
                拨了 |= ui.checkbox(&mut listed, "").on_hover_text(说明).changed();
                ui.vertical(|ui| {
                    let 点字 = ui
                        .add(egui::Label::new(字).sense(egui::Sense::click()))
                        .on_hover_text(说明);
                    if 点字.clicked() {
                        listed = !listed;
                        拨了 = true;
                    }
                    ui.label(小字);
                });
            });
        });
        if 拨了 {
            self.query.non_game_assets = if listed {
                NonGameAssets::Listed
            } else {
                NonGameAssets::Hidden
            };
        }
    }

    /// **读不懂的那几条规则**：摆出来，各给一个「扔掉这条」。返回按下去的是哪一条。
    ///
    /// 票 `gui-redesign/14` 走的是这条路——**横幅摆在筛选栏顶上，点开就处置**。
    /// 三条能走的路里选它的理由：
    ///
    /// - 它**不在子库屏上开口子**。票 `gui-redesign/11` 立的「这一屏不选内容」
    ///   （规则增删、例外记撤的控件全搬走）一个字没动：出路开在浏览屏，而浏览屏
    ///   本来就是这个子库的规则唯一改得动的地方。
    /// - 它**不往筛选树里塞一个读不回来的节点**。那棵树是「读得懂」的具象，
    ///   加一个不参与求值的异类，`WorkQuery::to_rule` 与「原样带回」两条口径都要跟着开洞。
    /// - 它**摆在人一进屏就看得见的地方**。同一趟的账（正在改谁）本来就印在这儿；
    ///   摆到底下那块面板里要滚一屏才看得见，而「看不出下一步」正是挂单 `Q86` 里
    ///   最贵的那一半。
    ///
    /// **只给「扔掉」，不给「改」**：改一条读不回来的原文要的是一个规则语言的文本框，
    /// 那正是这一版界面拆掉的东西。改对了的那一条走上面的筛选器重筛一遍。
    fn broken_rules_ui(ui: &mut egui::Ui, broken: &[BrokenRule]) -> Option<i64> {
        if broken.is_empty() {
            return None;
        }
        let mut 扔掉 = None;
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!(
                "这个子库另有 {} 条规则读不懂：没参与求值，「更新到子库」也不碰它们。",
                thousands(broken.len() as u64),
            ),
        );
        egui::CollapsingHeader::new(format!("处置读不懂的那 {} 条", broken.len()))
            .id_salt("读不懂的规则")
            // **默认摊开**：这一段只在真有坏规则时才出现，而「看不出下一步」正是挂单
            // `Q86` 里最贵的那一半——收起来等于把出路又藏回一次点击后面。
            .default_open(true)
            .show(ui, |ui| {
                ui.weak(
                    "它们进不了下面的筛选器——那是一棵读得懂的树。这儿只给一个动作：\
                     扔掉。要的东西改对了再筛一遍，按「更新到子库」带回去。",
                );
                for row in broken {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        format!("{}. {}（读不懂：{}）", row.ordinal, row.text, row.error),
                    );
                    let 按了 = look::small_buttons(ui, |ui| {
                        ui.button("扔掉这条")
                            .on_hover_text(
                                "只删这一条：读得懂的那几条与全部例外一个都不碰\
                                 （先读一遍再决定删不删，\
                                 读得懂的那几条不会被删）。扔掉不改变这个子库选出什么\
                                 ——它本来就没参与求值。",
                            )
                            .clicked()
                    });
                    if 按了 {
                        扔掉 = Some(row.ordinal);
                    }
                }
            });
        扔掉
    }

    /// **取消收藏与自建合集的加减**（票 `gui-redesign/06`）：稿上没画，缩成小号垫在左栏最底下，
    /// 等票 `13` 的弹层接走（挂单 `Q873`）。库里有哪几个合集那一簇标签照稿留在「收藏与合集」那一段。
    ///
    /// 作用范围是勾中的那一批，不是筛出来的全部；成员关系落沉淀库。这几句不写在屏上（票 `03`：
    /// 屏上不出现写给开发者的解释），留在这里和各颗按钮的悬停里。
    ///
    /// **加收藏那一下不在这儿，在屏头**（原型钉的位置）：它按得最勤，不该藏在左栏底下。
    fn collection_actions(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        let 取消 = look::small_buttons(ui, |ui| {
            ui.button("☆ 取消收藏")
                .on_hover_text("把勾中的那一批从收藏里拿出来。两种锚都拿，星星不会点不灭。")
                .clicked()
        });
        if 取消 {
            self.unfavorite(site, tasks);
        }
        let width = ui.available_width();
        look::small_text_input(
            ui,
            width,
            egui::TextEdit::singleline(&mut self.collection).hint_text("合集的名字，例如 通关过的"),
        );
        let name = self.collection.trim().to_string();
        // **名字里带着规则语言的记号就当场说清。** 建得出来而筛不出来，比建不出来更坏
        // ——那时人只会以为收藏这件事坏了（折规则那一步的判据在核心库里，一处说了算）。
        let 写得进规则 = !name.is_empty()
            && romcat_core::catalog::browse::writable_value(Dimension::Collection, &name);
        if !name.is_empty() && !写得进规则 {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "这个名字写不进规则（带着逗号、括号、或者两侧带空白的连接词）。\
                 加得进去，但 `合集=这个名字` 筛不出来，存成子库时也会被挡下。",
            );
        }
        let (加入, 移出) = look::small_buttons(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let 加入 = ui
                    .add_enabled(!name.is_empty(), egui::Button::new("加入合集"))
                    .on_hover_text("没有这个合集就顺手建出来——一个合集就是它那些成员。")
                    .clicked();
                let 移出 = ui
                    .add_enabled(!name.is_empty(), egui::Button::new("移出合集"))
                    .on_hover_text("一条成员都不剩的合集，从筛选栏那一维里消失。")
                    .clicked();
                (加入, 移出)
            })
            .inner
        });
        if 加入 {
            self.join_collection(site, tasks);
        }
        if 移出 {
            self.leave_collection(site, tasks);
        }
    }

    /// 「**存成子库**」：把当前筛选原样变成一条规则。
    ///
    /// 规格里那条贯穿全局的约定落在这一个按钮上——**主列表的筛选就是子库的规则**。
    /// 折规则那一步在核心库（[`WorkQuery::to_rule`]），这里只把两个格子交给它，
    /// 再把它说的话原样印出来。
    ///
    /// **写不成规则的条件当场挡住**，不是少写一条了事：少一条，子库选出来的就比屏上多。
    fn save_panel(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        if self.editing.is_some() {
            self.update_panel(ui, site);
            return;
        }
        section_title(ui, "存成子库", None);
        section_gap(ui);
        // **筛出多少条当场写出来**：按下去之前心里有数。
        look::help(
            ui,
            &format!(
                "筛出 {} 行 · {} 个变体",
                thousands(self.window.total()),
                scope_label(self.filtered),
            ),
        )
        .on_hover_text(
            "行数照的是「作品数 ＋ 还没认出作品的变体数」；\
             变体数是这批行底下的全部变体，按当前筛选。",
        );
        // 折出来的规则、折不成的原因都写在条件组底下那一条里（[`rule_text`]）；这儿只补一句
        // 「一个条件都没筛」——那时那一条不画。
        let folded = self.query.to_rule();
        if matches!(folded, Ok(None)) {
            look::help(ui, "一个条件都没筛——存出来的子库就是整个库。先筛一批。");
        }
        for (value, hint) in [
            (&mut self.save.name, "名字：一台目标设备一个"),
            (&mut self.save.target, "目标路径：读卡器挂上来的那个目录"),
        ] {
            let width = ui.available_width();
            look::small_text_input(ui, width, egui::TextEdit::singleline(value).hint_text(hint));
        }
        let ready = !self.save.name.trim().is_empty()
            && !self.save.target.trim().is_empty()
            && matches!(folded, Ok(Some(_)));
        let 存 = look::small_buttons(ui, |ui| {
            ui.add_enabled(ready, egui::Button::new("存成子库"))
                .on_hover_text("把当前筛选原样变成这个子库的规则。前端格式与容量上限去子库屏调。")
                .clicked()
        });
        if 存 {
            self.save_as_sublibrary(site);
        }
    }

    /// 「**改选择**」跳过来之后，那一栏换成的样子：改的是谁、折出来会是什么、带不带得回去。
    ///
    /// 与「存成子库」共用一个位置**是故意的**：这两个按钮是同一条约定的两个方向
    /// （筛选就是子库的规则），摆成两处会让人以为它们是两件事。
    fn update_panel(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        let Some(editing) = &self.editing else {
            return;
        };
        let (name, broken) = (editing.sublibrary.clone(), editing.broken.len());
        ui.label(font::strong(format!("正在改子库「{name}」的选择集")));
        ui.weak("这个子库的规则已经预填在上面的筛选器里。调完按「更新到子库」原样带回。");
        if broken > 0 {
            // **处置它们的地方在这一栏顶上**（票 `gui-redesign/14`）：这儿说的是
            // 「更新到子库」的承诺——那一趟只换读得懂的那几条，坏的一条都不碰。
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!(
                    "这个子库另有 {} 条规则读不懂：它们没参与求值，也不会被这一趟改掉。\
                     扔掉它们在这一栏顶上。",
                    thousands(broken as u64),
                ),
            );
        }
        let folded = self.query.to_rule();
        match &folded {
            Ok(None) => {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    "一个条件都没筛——这样带回去，这个子库选中的会是整个库。",
                );
            }
            Ok(Some(rule)) => {
                ui.weak(format!("规则会变成：{rule}"));
            }
            Err(unruly) => {
                ui.colored_label(ui.visuals().error_fg_color, unruly.advice());
            }
        }
        let (更新, 不改) = look::small_buttons(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let 更新 = ui
                    .add_enabled(
                        matches!(folded, Ok(Some(_))),
                        egui::Button::new("更新到子库"),
                    )
                    .on_hover_text(
                        "把屏上这份筛选原样换成那个子库的规则，然后回子库屏。\
                         换掉而不是加上去：加的话子库选出来的会比屏上多。",
                    )
                    .clicked();
                let 不改 = ui
                    .button("不改了")
                    .on_hover_text(
                        "放下这一趟，筛选留在屏上不动。已经记下的例外不撤——那是各自独立的决定。",
                    )
                    .clicked();
                (更新, 不改)
            })
            .inner
        });
        if 更新 {
            self.update_sublibrary(site);
        }
        if 不改 {
            self.cancel_editing();
        }
    }

    /// 真的建那个子库。**测试拿它当按下去那一下。**
    pub fn save_as_sublibrary(&mut self, site: &mut Site) {
        let rule = match self.query.to_rule() {
            Ok(Some(rule)) => rule,
            Ok(None) => {
                self.error = Some("一个条件都没筛：存出来的子库会是整个库。".to_string());
                return;
            }
            Err(unruly) => {
                self.error = Some(format!("这份筛选存不成子库：{}", unruly.advice()));
                return;
            }
        };
        let name = self.save.name.trim().to_string();
        // **重名不覆盖。** `put_sublibrary` 按名字更新，而 `add_rule` 是往上加一条——
        // 撞上一个已有的子库，等于悄悄把它的目标路径改掉、再给它的选择集并上一批。
        match site.catalog.sublibrary(&name) {
            Ok(Some(_)) => {
                self.error = Some(format!(
                    "已经有一个叫「{name}」的子库了。换个名字——改已有子库的选择去子库屏点「改选择」。"
                ));
                return;
            }
            Err(error) => {
                self.error = Some(format!("中立库读不动：{error}"));
                return;
            }
            Ok(None) => {}
        }
        let target = std::path::PathBuf::from(self.save.target.trim());
        // **目标落在主库里当场拦下**（ADR-0004）：判据在核心里，与同步那一道是同一条。
        // 拦在建出来这一步而不是等到同步，是因为一个指着主库的子库定义放在库里，
        // 下一次点同步之前谁都不知道它错了。
        if let Err(message) =
            romcat_core::sync::prepare::refuse_target_in_library(&site.catalog, &[], &target)
        {
            self.error = Some(message);
            return;
        }
        // 两种路径形式怎么折，**由核心的 `Sublibrary::at` 一处说了算**（ADR-0020）。
        let sublibrary = Sublibrary::at(&name, &target, "Pegasus", None);
        if let Err(error) = site.catalog.put_sublibrary(&sublibrary) {
            self.error = Some(format!("子库写不进中立库：{error}"));
            return;
        }
        match site.catalog.add_rule(&name, &rule) {
            Ok(_) => {
                self.notice = Some(format!(
                    "子库「{name}」已建好，规则是：{rule}。屏上这 {} 行 · {} 个变体原样带过去。",
                    thousands(self.window.total()),
                    scope_label(self.filtered),
                ));
                self.error = None;
                self.save = SaveDraft::default();
            }
            Err(error) => {
                // **两步得当一步用**：规则没写进去的话，刚建的那个子库一条规则都没有
                // ——同步过去是空的，而这个名字还被它占着，人按原名重试会被上面那段
                // 重名检查挡住。所以退回去，把名字还回来。
                let 退回 = site.catalog.remove_sublibrary(&name);
                self.error = Some(match 退回 {
                    Ok(_) => format!(
                        "规则写不进中立库：{error}。刚建的子库「{name}」已经退掉，这个名字还能用。"
                    ),
                    Err(second) => format!(
                        "规则写不进中立库：{error}。而刚建的子库「{name}」也退不掉（{second}）\
                         ——它眼下一条规则都没有，同步过去会是空的，去子库屏删掉它。"
                    ),
                });
            }
        }
    }

    /// **每帧一次**：跟后台那条解码线程对一次账——跑完的图收进来、这一屏缺的排出去，
    /// 顺带把刚抽出来的首帧记进中立库（票 `gui-redesign/07`）。
    ///
    /// 摆在这一屏的开头而不是详情面板里面，是因为它**与哪块面板正在画无关**：
    /// 选中哪个变体决定要哪几份图，而详情面板画不画得出来是另一回事。排在画之后的话，
    /// 刚点开的那个变体还要白等一帧才开始解。
    ///
    /// `writable` 是「眼下动得动中立库吗」——任务台上有活在跑时是 `false`
    /// （[`Gallery::sync`](crate::media::Gallery::sync) 的文档写着为什么）。
    fn sync_media(&mut self, ctx: &egui::Context, site: &mut Site, writable: bool) {
        // **没选中变体也照跑一趟。** 抽帧要几百毫秒，人点开一段视频、抽到一半切走，
        // 那份已经落进池里的首帧就得有人收——不收的话池里多一个孤儿文件，
        // 两张表里一行都没有，下次打开照样重抽。
        //
        // 抄一份：问后台那一下要动 `self.gallery`，而 `items` 是从 `self.detail` 借的。
        // **作品详情页开着时解那一页列出来的那几份**（每个变体的都在里头）；平常解侧边详情选中那个变体的。
        let items = self.page_media_items().unwrap_or_else(|| {
            self.detail
                .as_ref()
                .map(|detail| detail.media_items.clone())
                .unwrap_or_default()
        });
        self.gallery.sync(ctx, &mut site.catalog, &items, writable);
    }

    /// 右边那块面板：**作品 → 变体 → 判定依据 → 媒体 → 合集**；改选择那一趟里，选中那张变体卡底下多一块例外。
    ///
    /// **这一份不复制一遍再画**：一行底下可以挂着上百个变体、每个又带着几条候选，
    /// 每帧克隆一次就是每帧几百次分配。所以画的时候只借（`as_ref`），点中哪个变体、按了哪颗例外
    /// 攒在外头，出了这个闭包再去改自己。
    fn detail_panel(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        // 标题行：这一栏叫什么，右头那颗「收起」。点没点开一行都在。
        ui.horizontal(|ui| {
            ui.label(font::strong("详情"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                layout::DETAIL.collapse_button(ui);
            });
        });
        pane_gap(ui);
        if self.work.is_none() {
            look::help(ui, "点主列表里的一行，看它包含哪几个变体。");
            return;
        }
        let mut pick: Option<String> = None;
        // 点了哪一格图。
        let mut open: Option<crate::media::Clicked> = None;
        // 点了「查看详情」或「编辑元数据」：要打开作品详情页的哪一面、进不进编辑态。
        let mut 去详情页: Option<(work::Tab, bool)> = None;
        // **改选择那一趟里例外摆在选中那张变体卡底下**（拿主意的人 2026-09-15 定）：备注框要改得动，
        // 先抄一份出来画，画完有改动再写回去。平常浏览时一样都不摆，也不留空位。
        let mut 备注 = self.editing.as_ref().map(|editing| editing.note.clone());
        let mut 例外: Option<Option<Exception>> = None;
        let this = &*self;
        let Some(work) = this.work.as_ref() else {
            return;
        };
        // **照稿的次序**（票 `gui-looks-like-the-design/09`）：头上那一块（封面或字卡、它是什么、
        // 叫什么、哪个平台哪一年、几个变体）→ 变体 → 判定依据 → 媒体；合集垫在后头，合集那一票接走之前
        // 它照旧在这儿看得到。文件表归作品详情页「变体与文件」那一面（票 `gui-looks-like-the-design/15`）。
        egui::ScrollArea::vertical()
            .id_salt("作品详情")
            .show(ui, |ui| {
                this.detail_head(ui, work);

                // 头上那一块底下一排两颗小号按钮（设计稿 `.dhead` 后头那一排）：打开作品详情页，停在概览或元数据那一面。
                pane_gap(ui);
                去详情页 = page_buttons(ui);
                // 认不出作品的那一行：名字是怎么来的——没有作品链接**本身就是一条信息**（照稿摆在两颗按钮底下）。
                if matches!(work.anchor, WorkAnchor::Loose(_)) {
                    pane_gap(ui);
                    look::note_box(ui, |ui| {
                        ui.label(
                            "这个名字是从文件名剥出来的正题（剥掉了汉化组、版本号这类记号）：\
                             识别还没认出它属于哪个作品，所以这一行就是它自己。",
                        );
                    });
                }

                pane_gap(ui);
                section_title(
                    ui,
                    &format!("变体 {} 个", work.variants.len()),
                    Some("下方操作作用于选中的变体"),
                );
                section_gap(ui);
                let preferred = this.detail.as_ref().and_then(VariantDetail::preferred_now);
                for (at, variant) in work.variants.iter().enumerate() {
                    if at > 0 {
                        section_gap(ui);
                    }
                    let 简称 = this.short_names.get(at).map(String::as_str);
                    if this.variant_card(ui, variant, preferred, 简称) {
                        pick = Some(variant.row.key.clone());
                    }
                    let 选中的 = this.variant.as_deref() == Some(variant.row.key.as_str());
                    if let (true, Some(editing), Some(备注)) =
                        (选中的, &this.editing, 备注.as_mut())
                    {
                        section_gap(ui);
                        if let Some(按了) = exception_block(ui, editing, &variant.row.key, 备注)
                        {
                            例外 = Some(按了);
                        }
                    }
                }

                pane_gap(ui);
                this.basis_ui(ui, work);
                pane_gap(ui);
                open = this.media_ui(ui);
                pane_gap(ui);
                this.collections_ui(ui);
            });
        if let (Some(editing), Some(备注)) = (self.editing.as_mut(), 备注) {
            editing.note = 备注;
        }
        if let (Some(按了), Some(key)) = (例外, self.variant.clone()) {
            match 按了 {
                Some(kind) => self.set_exception(site, &key, kind),
                None => self.clear_exception(site, &key),
            }
        }
        if let Some(key) = pick {
            self.pick(&site.catalog, &key);
        }
        if let Some((tab, 编辑)) = 去详情页 {
            self.open_page(tab);
            if 编辑 {
                self.begin_meta_edit();
            }
        }
        // **窗口里一个字节都不解码**：播放与看原图都交给系统默认程序，与作品详情页同一条路（`open_media`）。
        if let Some(clicked) = open {
            self.open_media(clicked);
        }
    }

    /// 点开那一行**屏上叫什么**：认不出作品的是正题（`WorkDetail::title`，与表上那一行主栏同一处剥），认出的是
    /// 选中那个变体的详情里挑好的**显示标题**（与表上那一行主栏同一处 `title::choose`），取不到时作品名。
    /// 侧边详情头上与作品详情页顶上印的是同一个。
    fn work_title(&self, work: &WorkDetail) -> String {
        work.title(&self.rules)
            .or_else(|| {
                self.detail
                    .as_ref()
                    .and_then(|detail| detail.display.as_ref())
                    .map(|chosen| chosen.display.clone())
            })
            .unwrap_or_else(|| work.name.clone())
    }

    /// 侧边详情**头上那一块**（设计稿 `.dhead`）：左边封面或字卡（令牌 `detail-cover-width` 那么宽、
    /// 高按 `card-cover-ratio` 折），右边它是什么（作品 / 未关联作品的变体）、叫什么、哪个平台哪一年、
    /// 底下几个变体。
    ///
    /// 认不出作品的那一行**标题是正题**（`WorkDetail::title`，与表上那一行主栏同一处剥）；说这个名字是怎么来的
    /// 那块提示框摆在这一块底下两颗按钮之后（`Self::detail_panel`，照稿的次序）。
    fn detail_head(&self, ui: &mut egui::Ui, work: &WorkDetail) {
        let tokens = Tokens::builtin();
        let loose = matches!(work.anchor, WorkAnchor::Loose(_));
        // 认出作品的：**显示标题**（选中那个变体的详情里挑好的，与表上那一行主栏同一处 `title::choose`）；
        // 取不到时作品名。
        let title = self.work_title(work);
        let platform = work.platforms.first().map_or("", String::as_str);
        let 宽 = tokens.layout.detail_cover_width;
        let size = egui::vec2(宽, 宽 / tokens.layout.card_cover_ratio);
        // 字卡的水印伸出圆角外头那一点要刷回这一栏的底色（设计稿 `.dpane` 的面板底色）。
        let 底色 = ui.visuals().window_fill;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = tokens.space.detail_head_gap;
            // **解码借详情面板那几格的**：这一张挂在这一行自己的锚点上，选中变体的逐条清单里本来就有它；
            // 还在后台解的那一帧先画字卡，解完了下一帧自然换上。
            match self
                .cover
                .as_ref()
                .and_then(|item| self.gallery.texture(item))
            {
                Some(texture) => {
                    crate::media::paint_cover(ui, size, tokens.radius.medium, texture);
                }
                None => crate::media::title_card(ui, size, &title, platform, 底色),
            }
            ui.vertical(|ui| {
                // 四行字之间照稿 `gap:4px`，取最小那一档间距。
                ui.spacing_mut().item_spacing.y = look::step(0);
                look::section(
                    ui,
                    &if loose {
                        format!("{UNLINKED_LABEL}的变体")
                    } else {
                        "作品".to_string()
                    },
                );
                // 字号**直接问令牌**，不走具名字号档：那几档要等观感基线装上之后的下一帧才有，
                // 而点开一行正好可能发生在头一帧（跳过来的「改选择」、测试里先点开再跑帧）。
                ui.label(
                    egui::RichText::new(&title)
                        .size(tokens.font.size_detail_title)
                        .family(font::strong_family())
                        .color(ui.visuals().strong_text_color()),
                );
                ui.weak(format!(
                    "{} · {}",
                    if work.platforms.is_empty() {
                        "—".to_string()
                    } else {
                        work.platforms.join(" / ")
                    },
                    work.year.as_deref().unwrap_or("年份未知"),
                ));
                ui.weak(format!("{} 个变体", work.variants.len()));
            });
        });
    }

    /// **判定依据**：选中那个变体凭什么落在这一档——识别结论、没定下来的理由、每条候选与它的依据。
    ///
    /// 从前它只挂在变体那一行的悬停里；照稿摊在栏里的一块提示框里。没有依据的结论事后无法复核
    /// （ADR-0002），而悬停得先知道去哪儿停。
    fn basis_ui(&self, ui: &mut egui::Ui, work: &WorkDetail) {
        section_title(ui, "判定依据", None);
        section_gap(ui);
        let Some(variant) = work
            .variants
            .iter()
            .find(|variant| self.variant.as_deref() == Some(variant.row.key.as_str()))
        else {
            look::help(ui, "点一个变体，看它凭什么落在这一档。");
            return;
        };
        // 这一块提示框的字比别的提示框小半档：稿上那一块写着 `font-size:12px`。
        look::note_box(ui, |ui| {
            ui.style_mut().override_font_id = Some(egui::FontId::proportional(
                Tokens::builtin().font.size_small,
            ));
            basis_lines(ui, variant);
        });
    }

    /// 侧边详情里的一个变体，照稿 `.var` 画成一张卡片：左沿一道置信度色条；头一行是**变体简称**，跟着
    /// 「首选变体」「非游戏资产」两枚标签，右头一枚置信度标签；底下一行是它的键，等宽、哪儿都能折行。
    /// 选中的那张描强调色、铺强调浅底。点中了返回 `true`。**依据**摊在底下「判定依据」那一块
    /// （[`Self::basis_ui`]）。
    ///
    /// **置信度那一档走六屏共用的那一份**（[`crate::look`]，票 `gui-redesign/12`）：色条与标签出自一处，
    /// 而且**两样一起出现**——色觉障碍下读得出来的只有词。
    ///
    /// **那个词由核心库挑**（[`WorkVariant::confidence_label`]，票 `gui-redesign/17`）：一条候选都没有时
    /// 它是**没有候选**还是**还没识别**，取决于这个变体跑没跑过识别，而那是一条领域判断，不是画法
    /// （ADR-0005）。色条照旧只认四档——两者的区别由词说，不由颜色说。哪一个是首选变体
    /// （[`VariantDetail::preferred_now`]）、是不是非游戏资产（[`WorkVariant::non_game_asset`]）同样由核心库答。
    ///
    /// 头一行的**变体简称**（「原版」「汉化版 · 口袋汉化组」，词表同名词条）由核心库拼好交进来
    /// （`short_name`，`Catalog::variant_short_names`）；没拼出来时印文件名。都从左边截到画得下。
    /// 底下那一行照稿是「根名 · 相对路径」，拆键由核心库做（挂单 `Q809`）。
    fn variant_card(
        &self,
        ui: &mut egui::Ui,
        variant: &WorkVariant,
        preferred: Option<&str>,
        short_name: Option<&str>,
    ) -> bool {
        let tokens = Tokens::builtin();
        let key = variant.row.key.as_str();
        let on = self.variant.as_deref() == Some(key);
        let tier = Tier::of(variant.confidence());
        let [上下, 左右] = tokens.space.variant_card_padding;
        let [行距, 列距] = tokens.space.variant_card_gap;
        let (底色, 描边色, 条色, 弱, 强) = {
            let visuals = ui.visuals();
            let (底色, 描边色) = if on {
                (visuals.selection.bg_fill, visuals.selection.stroke.color)
            } else {
                (
                    egui::Color32::TRANSPARENT,
                    visuals.widgets.noninteractive.bg_stroke.color,
                )
            };
            (
                底色,
                描边色,
                look::tier_color(tier, visuals),
                visuals.weak_text_color(),
                visuals.strong_text_color(),
            )
        };
        let mut 标签 = Vec::new();
        if preferred == Some(key) {
            标签.push(PREFERRED_TAG);
        }
        if variant.non_game_asset() {
            标签.push(NON_GAME_ASSET_LABEL);
        }
        let 卡片 = egui::Frame::new()
            .fill(底色)
            .stroke(egui::Stroke::new(tokens.layout.control_stroke, 描边色))
            .corner_radius(tokens.radius.medium)
            .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = egui::vec2(列距, 行距);
                ui.spacing_mut().interact_size.y = tokens.layout.tag_height;
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        look::chip(ui, look::tier_tone(tier), variant.confidence_label());
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            let 名字体 =
                                egui::FontId::new(tokens.font.size_body, font::strong_family());
                            let 标签宽: f32 = 标签
                                .iter()
                                .map(|text| {
                                    crate::table::tag_width(ui, text) + ui.spacing().item_spacing.x
                                })
                                .sum();
                            let 地方 = (ui.available_width() - 标签宽).max(0.0);
                            let 名 = tail_fit(
                                ui,
                                short_name
                                    .unwrap_or_else(|| romcat_core::path::file_name_of_key(key)),
                                &名字体,
                                地方,
                            );
                            ui.label(egui::RichText::new(名).font(名字体).color(强));
                            for text in &标签 {
                                crate::table::tag(ui, text);
                            }
                        });
                    });
                });
                // 「根名 · 相对路径」：拆键由核心库做（`path::split_root`），这儿只接起来。
                let 路径 = match romcat_core::path::split_root(key) {
                    (根名, "") => 根名.to_owned(),
                    (根名, 相对) => format!("{根名}{}{相对}", crate::table::ROOT_SEPARATOR),
                };
                let mut 键 = egui::text::LayoutJob::simple(
                    路径,
                    egui::FontId::monospace(tokens.font.size_path),
                    弱,
                    ui.available_width(),
                );
                键.wrap.break_anywhere = true;
                let galley = ui.painter().layout_job(键);
                ui.label(galley);
            })
            .response
            .rect;
        // 左沿那一道色条（设计稿 `box-shadow:inset 3px 0 0 var(--c)`）：把描边里头那块圆角矩形用那一档的
        // 颜色再铺一遍，只留左边令牌 `tier-bar` 那么宽一截——两个左角跟着卡片的圆角走。
        let 里头 = 卡片.shrink(tokens.layout.control_stroke);
        ui.painter()
            .with_clip_rect(egui::Rect::from_min_size(
                里头.min,
                egui::vec2(tokens.layout.tier_bar, 里头.height()),
            ))
            .rect_filled(
                里头,
                (f32::from(tokens.radius.medium) - tokens.layout.control_stroke).max(0.0),
                条色,
            );
        let response = ui.interact(卡片, ui.id().with(("变体卡片", key)), egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, on, key)
        });
        let response = response.on_hover_text(key);
        look::focus_ring(ui.ctx(), ui.clip_rect(), &response);
        response.clicked()
    }

    /// 选中那个变体在哪几个合集里——**连它钉在哪种锚上一起说**。
    ///
    /// 这是验收第 7 条落在屏上的地方：钉在**路径**上的那些只在本机成立，改个名字、
    /// 挪个目录就认不出来了。**不含糊成一句「在收藏里」**——那会让人以为每一条都稳
    /// （ADR-0021 那条纪律：说得出「这一条换台机器还认不认得出」，比让人以为都认得出强）。
    fn collections_ui(&self, ui: &mut egui::Ui) {
        if self.variant.is_none() {
            ui.weak("选一个变体，看它在哪几个合集里。");
            return;
        }
        section_title(
            ui,
            &format!("收藏与合集 · {} 个", self.standing.len()),
            None,
        );
        section_gap(ui);
        if self.standing.is_empty() {
            ui.weak("一个都没进。勾几行按屏头那颗「★ 收藏」，或者在左栏底下加进自建合集。");
            return;
        }
        for (name, anchor) in &self.standing {
            let 星 = if name == FAVORITE { "★ " } else { "" };
            if *anchor == romcat_core::verdict::ANCHOR_CONTENT {
                ui.label(format!("{星}{name}｜钉在内容上"))
                    .on_hover_text("锚是这份内容本身（CRC-32 加大小）：删掉中立库重扫、改名、挪目录、换根，都还认得出。");
            } else {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("{星}{name}｜只钉得住本机路径，挪了位置会飘"),
                )
                .on_hover_text(
                    "这个变体拿不到内容判据（无判据那一档：容器穿不透、压缩镜像、\
                     目录树转储），所以只钉得住它眼下这个位置。改名或挪到别的目录之后，\
                     这一条就认不出来了。与裁决是同一个限制。",
                );
            }
        }
    }

    /// 详情面板的**媒体**那一块：几格缩略图，底下那份逐条清单收在折叠里。
    ///
    /// **图直接画出来**（票 `gui-redesign/07`）：jpg 与 png 内嵌显示，视频是一张抽出来的
    /// 首帧加一个播放标。点一格就用**系统默认程序**打开，返回的就是那一下算什么
    /// （[`crate::media::Clicked`]）——窗口里一个字节都不解码视频（规格的 Out of Scope）。
    ///
    /// 逐条那份清单**一条都没删**（票 `gui-redesign/03` 立的）：媒体池按内容哈希存
    /// （ADR-0009），人问「那张封面到底落在哪个文件」时要的正是它。只是收进折叠里——
    /// 一屏 6 件媒体，先看图后看账。
    fn media_ui(&self, ui: &mut egui::Ui) -> Option<crate::media::Clicked> {
        let detail = self.detail.as_ref()?;
        section_title(ui, &format!("媒体 {} 个", detail.media_items.len()), None);
        section_gap(ui);
        // **几格图**：照稿 `.thumbs` 一行摆令牌 `thumbs-per-row` 格、格与格之间 `thumb-gap`，
        // 格子的宽跟着这块面板走（挂单 `Q126`）——拖宽了就每格大一点，而不是右边空出一条。
        let tokens = Tokens::builtin();
        let (每行, 缝) = (
            tokens.layout.thumbs_per_row.max(1.0),
            tokens.space.thumb_gap,
        );
        let 宽 = ((ui.available_width() - 缝 * (每行 - 1.0)) / 每行)
            .floor()
            .max(0.0);
        let size = egui::vec2(宽, (宽 / tokens.layout.card_cover_ratio).floor());
        let mut open = None;
        if detail.media_items.is_empty() {
            // 一格都没有就不摆那一排：空的横排照样占一行高，标题底下平白空出一截。
            look::help(ui, "一条媒体引用都没有。");
        } else {
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(缝, 缝);
                ui.horizontal_wrapped(|ui| {
                    for item in &detail.media_items {
                        if let Some(点的) = self.gallery.cell(ui, item, size) {
                            open = Some(点的);
                        }
                    }
                });
            });
        }
        if self.pool.is_none() {
            ui.weak("媒体池不在工作目录里，「在不在池子里」这一栏查不了，图也画不出。");
        } else if self.gallery.lacks_ffmpeg() {
            // **只说一遍**：真库里 178 个 mp4，每格各摆一句是噪音。
            ui.weak("这台机器上没有 ffmpeg，视频抽不出首帧——那几格是占位，点下去照样放得了。");
        }
        // **一件一件说清**：哪一格没有图、为什么。汇总的那句「有 N 条引用找不到文件」
        // 说不出是哪一件，而这条验收要的正是后者。
        for (是哪一件, 为什么) in self.gallery.troubles(&detail.media_items) {
            ui.colored_label(ui.visuals().warn_fg_color, format!("{是哪一件}：{为什么}"));
        }
        if let Some(说的) = self.gallery.error() {
            ui.colored_label(ui.visuals().error_fg_color, 说的);
        }
        // **逐条那份账**：类型、锚点、源、它在池里的落点、依据。
        egui::CollapsingHeader::new("一条条看")
            .id_salt("媒体逐条")
            .show(ui, |ui| {
                for item in &detail.media_items {
                    let where_at = match (&item.at, item.in_pool) {
                        (Some(at), Some(true)) => romcat_core::path::display(at),
                        (Some(_), _) => "池里没有这个文件".to_string(),
                        _ => "（媒体池没查）".to_string(),
                    };
                    let line = format!(
                        "{} · {}｜{}｜{}",
                        item.kind.label(),
                        item.anchor.label(),
                        item.source,
                        where_at,
                    );
                    if item.in_pool == Some(false) {
                        ui.colored_label(ui.visuals().warn_fg_color, line)
                    } else {
                        ui.label(line)
                    }
                    .on_hover_text(format!(
                        "{}.{}｜依据：{}",
                        item.hash, item.ext, item.evidence
                    ));
                }
            });
        let missing = detail.missing_media();
        if missing.is_empty() {
            ui.label("不缺媒体。");
        } else {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!(
                    "缺：{}",
                    missing
                        .iter()
                        .map(|kind| kind.label())
                        .collect::<Vec<_>>()
                        .join("、"),
                ),
            );
        }
        if detail.dangling_media() > 0 {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!(
                    "有 {} 条引用在媒体池里找不到那个文件，导出时一张都铺不出去。",
                    thousands(detail.dangling_media()),
                ),
            );
        }
        open
    }

    /// 把草稿里那条叫法写进标题集合，**来源记作裁决**。写成了就返回 `true`。
    fn write_title(&mut self, site: &mut Site, work: &str, detail: &VariantDetail) -> bool {
        let value = self.title_draft.value.trim().to_string();
        if value.is_empty() {
            return false;
        }
        let row = romcat_core::catalog::TitleRow {
            work: work.to_string(),
            value: value.clone(),
            language: self.title_draft.language,
            kind: self.title_draft.kind,
            // **裁决**：重折标题集合时一行都不碰（`Catalog::clear_titles`）。
            source: VERDICT.to_string(),
            region: detail
                .release
                .as_ref()
                .and_then(|release| release.region.clone()),
            variant_key: Some(detail.row.key.clone()),
            confidence: romcat_core::catalog::Confidence::High,
            seam: None,
            evidence: HAND_WRITTEN.to_string(),
            seen: 1,
        };
        match site.catalog.put_titles(&[row]) {
            Ok(()) => {
                self.notice = Some(format!("已添加名称「{value}」（记为裁决）"));
                self.title_draft.value.clear();
                true
            }
            Err(error) => {
                self.error = Some(format!("中立库写不动：{error}"));
                false
            }
        }
    }

    /// 字体样张：把 egui 内置字体缺的那几类字**摆出来给人看**。
    fn font_sample(&self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(font::strong("字体样张"));
            ui.label(format!(
                "子集 {}，另有拉丁粗体与等宽 {}",
                human_bytes(font::subset_bytes() as u64),
                human_bytes(font::latin_bytes() as u64),
            ));
        });
        // 末尾两行摆粗体与等宽：粗体只落在拉丁与数字上、中文照旧常规字重，等宽的数字同宽。
        let sample = font::SAMPLE
            .iter()
            .map(|(what, text)| (*what, egui::RichText::new(*text)));
        let faces = [
            ("粗体", font::strong("Final Fantasy VII 1997 最终幻想")),
            ("等宽", font::mono("SLPS-02170  1,234,567  888.8 MB")),
        ];
        for (what, text) in sample.chain(faces) {
            ui.horizontal(|ui| {
                ui.add_sized([90.0, 18.0], egui::Label::new(what));
                ui.label(text);
            });
        }
        // OFL 要求分发字体时随附许可，而这几份字体是嵌在可执行文件里的——许可得跟着走，
        // 两家版权行不同，一家一份。全文用常规字体：等宽只给路径、哈希、序列号、数量与容量。
        for (family, text) in font::LICENSES.iter().copied() {
            ui.collapsing(
                format!("字体许可 · {family}（SIL Open Font License 1.1）"),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(family)
                        .max_height(160.0)
                        .show(ui, |ui| ui.label(text));
                },
            );
        }
    }
}

/// 一个变体的**判定依据**那几行：识别结论、没定下来的理由、每条候选凭什么。
///
/// 一条依据能有一整句话，所以一条一行、在栏里折行，不挤进变体那一行。
fn basis_lines(ui: &mut egui::Ui, variant: &WorkVariant) {
    ui.label(format!(
        "识别结论：{}",
        variant.state.map_or(NOT_RUN_LABEL, |state| state.label()),
    ));
    if let Some(reason) = &variant.reason {
        ui.label(format!("为什么没定下来：{reason}"));
    }
    // **一条候选都没有分两种**（`CONTEXT.md` 的**还没识别**与**没有候选**）：
    // 连识别都还没跑过，该做的事是跑一趟识别；跑过了却一个字都没说得出来，
    // 该做的事是人自己来。两种印同一句话的话，屏上就指错了下一步。
    // **哪一句由核心库挑**（`WorkVariant::no_candidate_hint`）——与那一行印哪个词
    // 同一条判据，这儿一个 `if` 都不写（ADR-0005）。
    if let Some(说一句) = variant.no_candidate_hint() {
        ui.label(说一句);
    }
    for candidate in variant.candidates.iter().take(TOP_CANDIDATES) {
        ui.label(format!(
            "{} · {}｜{}｜{}",
            candidate.confidence.label(),
            if candidate.accepted {
                "自动通过"
            } else {
                "等人裁决"
            },
            candidate.source,
            candidate.game,
        ));
        ui.weak(&candidate.evidence);
    }
    if variant.candidates.len() > TOP_CANDIDATES {
        ui.weak(format!(
            "……另有 {} 条候选没列",
            variant.candidates.len() - TOP_CANDIDATES,
        ));
    }
}

/// 「显示非游戏资产」底下那句小字：共几个、不会被导出（拿主意的人照稿定的字）。开没开都是这一句
/// ——开没开，勾选框自己说。
///
/// **数是核心库数的**（[`Catalog::non_game_asset_rows`]），这里只挑一句话印。数不出来时
/// 不写一个 0——读库的错另在屏上说。
fn non_game_asset_label(rows: Option<u64>) -> String {
    match rows {
        None => "非游戏资产数不出来".to_string(),
        Some(rows) => format!("BIOS 等文件共 {} 个，不会被导出", thousands(rows)),
    }
}

/// 右栏选中那张变体卡底下的**例外**那一块，只在「改选择」那一趟里摆（拿主意的人 2026-09-15 定）：把这个变体
/// 收进来或者排除掉，旁边留一句为什么。按了哪一颗交回那一下：`Some(方向)` 是记一条，`None` 是撤掉。
///
/// **优先于规则、永久记住**（ADR-0016）：规则表达不了「这个我小时候玩过」「这个太占地方先不带」这类个人口味。
/// 落在**变体**这一层——规则说「要什么内容」，例外说「另外还要 / 偏不要这一份」。它在浏览屏而不在子库屏，
/// 因为「哪一份」只有在详情里才指得准：子库屏上人手里只有一串键。
///
/// 备注框在右栏那块不虚拟化的滚动区里，组字时不会凭空消失（ADR-0005）；它的身份钉在固定的名字上。
fn exception_block(
    ui: &mut egui::Ui,
    editing: &Editing,
    key: &str,
    note: &mut String,
) -> Option<Option<Exception>> {
    let current = editing.exceptions.get(key);
    let mut 按了 = None;
    ui.label(font::strong(format!(
        "例外 · 子库「{}」",
        editing.sublibrary
    )));
    match current {
        None => {
            look::help(ui, "这个变体上还没有例外：进不进选择集，眼下由规则说了算。");
        }
        Some(row) => {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!(
                    "眼下：{}{}",
                    row.kind.shown(),
                    row.note
                        .as_deref()
                        .map(|note| format!("（{note}）"))
                        .unwrap_or_default(),
                ),
            );
        }
    }
    ui.add(
        egui::TextEdit::singleline(note)
            .id_salt("例外备注")
            .desired_width(ui.available_width())
            .hint_text("为什么（半年后你会想知道）"),
    );
    look::small_buttons(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for kind in [Exception::Include, Exception::Exclude] {
                let on = current.is_some_and(|row| row.kind == kind);
                if ui
                    .add_enabled(!on, egui::Button::new(format!("{}它", kind.shown())))
                    .on_hover_text(match kind {
                        Exception::Include => "规则没选中也带上它。",
                        Exception::Exclude => "规则选中了也不带。容量超限时砍谁，落点就是这一条。",
                    })
                    .clicked()
                {
                    按了 = Some(Some(kind));
                }
            }
            if ui
                .add_enabled(current.is_some(), egui::Button::new("撤掉"))
                .on_hover_text("撤掉之后这个变体进不进选择集重新由规则说了算。")
                .clicked()
            {
                按了 = Some(None);
            }
        });
    });
    按了
}

/// **打开外部程序**那一下：拿到一个路径交给系统（播放视频、看原图、打开位置），不成时交回一句为什么。
type Opener = Box<dyn FnMut(&std::path::Path) -> Result<(), String>>;

/// 表格上方那一条上「清除选择」那颗按钮上的字（设计稿 `#clear-pick`）。
///
/// 照稿它在**左边那一流**里（`.seg`、`.cnt`、选中数之后，那一组批量操作之前），
/// 不在这一条的右端——右端是 `.acts`（挂单 `Q1102`）。
const CLEAR_PICK: &str = "清除选择";

/// 表格上方那一条右端那一组**批量操作**里的一颗（设计稿 `.tbar .acts`）。
///
/// **摆成一个闭集合**：这一组要先量宽、再决定摆在哪一行（`Screen::action_width` 与
/// `Screen::list_bar`），而**量的与画的必须是同一批按钮、同一串字**——各写一遍的话，
/// 加了一颗却没加进量的那一处，摆出来就差一截，而那种错在屏上是「最右一颗贴着边」
/// 或者「早折了一行」这种说不清的样子。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// 「刮削…」（稿上 `#open-scrape`）。
    Scrape,
    /// 「★ 收藏」（稿上 `#fav`）。
    Favorite,
    /// 「合并作品…」（稿上 `#merge-btn`，票 `16` 做的）。
    Merge,
}

/// 这一组照稿的次序，**量宽与画都走它**。
///
/// 稿上是五颗：**刮削… / ★ 收藏 / 加入合集… / 合并作品… / 加入子库…**。
/// 今天摆得出三颗，另外两颗**位置照稿留着**，各归一张票：
///
/// - **「加入合集…」归票 `13`**（收藏与合集的弹层），摆在「★ 收藏」与「合并作品…」之间；
/// - **「加入子库…」归票 `23`**（从浏览屏加入子库），摆在最右，稿上是 `btn sm pri`
///   ——**这一组里唯一的主按钮**。
///
/// 写在这儿是为了那两票不必各自再想一遍摆在哪：次序是稿定的，不是先到先得。
const ACTIONS: [Action; 3] = [Action::Scrape, Action::Favorite, Action::Merge];

impl Action {
    /// 按钮上的字。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Scrape => "刮削…",
            Self::Favorite => "★ 收藏",
            Self::Merge => merge::MERGE,
        }
    }

    /// 悬停里那一段。
    fn hint(self) -> &'static str {
        match self {
            // **「刮削…」只摊开弹层**——真按下去那一下在弹层底下，因为按之前该先看清
            // 那本账；作用于哪一批，弹层标题上写着。
            Self::Scrape => {
                "对勾中的那几个作品取元数据与媒体。四个旋钮定清楚要干什么，\
                 按下去之前就看得见会发多少网络请求、大概多久。"
            }
            Self::Favorite => {
                "把勾中的那一批全放进收藏。落沉淀库、锚在内容上——\
                 删掉中立库重扫、改名、挪目录都还在。无判据的那些只钉得住本机路径，\
                 按完的回执里会点名说有几个。\n\n\
                 取消收藏与自建合集在左栏最底下：加收藏按得最勤，所以只有它在这一组里。"
            }
            Self::Merge => {
                "把被识别成不同作品、其实是同一个游戏的变体归到一起。\n\n\
                 勾两个或更多作品再按。只写入裁决记录，不会移动或修改任何文件。"
            }
        }
    }
}

/// 刚落下一批裁决时，底边那条提示条上那颗按钮上的字（设计稿 `doMerge` 末尾那个 `toast`）。
const UNDO: &str = "撤销";

/// 作用范围那个数画成什么。**数不出来就说数不出来**，不摆一个 0 出去。
fn scope_label(scope: Option<u64>) -> String {
    scope.map_or_else(|| "？".to_string(), thousands)
}

/// 左右两栏里**一段与一段之间**的留白（设计稿 `.fpane` / `.dpane` 的 `gap`）：扣掉 egui 自己在两件
/// 东西之间留的那一份竖向间距，合起来正好是令牌 `pane-gap`。
/// 侧边详情头上那一块底下那一排：「查看详情」（主按钮）与「编辑元数据」，都是小号（设计稿 `.btn.sm`）。
/// 按了哪一颗，交回要打开作品详情页的哪一面、进不进编辑态（「编辑元数据」直接进，设计稿 `openWD(i,'edit')`）。
fn page_buttons(ui: &mut egui::Ui) -> Option<(work::Tab, bool)> {
    look::small_buttons(ui, |ui| {
        ui.horizontal(|ui| {
            // 两颗之间照稿 `.row` 的 `gap:8px`。
            ui.spacing_mut().item_spacing.x = look::step(1);
            let 看 = ui
                .scope(|ui| {
                    look::primary_button(ui.visuals_mut());
                    ui.button("查看详情")
                })
                .inner
                .clicked();
            let 改 = ui.button("编辑元数据").clicked();
            if 看 {
                Some((work::Tab::Overview, false))
            } else if 改 {
                Some((work::Tab::Metadata, true))
            } else {
                None
            }
        })
        .inner
    })
}

fn pane_gap(ui: &mut egui::Ui) {
    let gap = Tokens::builtin().space.pane_gap - ui.spacing().item_spacing.y;
    ui.add_space(gap.max(0.0));
}

/// 一段里**小标题与底下内容之间**的留白（设计稿 `.col` 的 `gap`）：同 [`pane_gap`] 的算法，
/// 取令牌 `section-gap`。
fn section_gap(ui: &mut egui::Ui) {
    let gap = Tokens::builtin().space.section_gap - ui.spacing().item_spacing.y;
    ui.add_space(gap.max(0.0));
}

/// 一段的**小标题**（设计稿 `.sec`）；带着帮助字时是一行，左小标题、右帮助字（设计稿 `.row` 里的
/// `.sec` 与 `.help`），这一栏窄到摆不下时帮助字从尾巴截掉。交回小标题那一块的 `Response`，挂悬停用。
fn section_title(ui: &mut egui::Ui, title: &str, help: Option<&str>) -> egui::Response {
    ui.scope(|ui| {
        // 这一行只有两段字：不要按钮那么高的最矮行高。
        ui.spacing_mut().interact_size.y = 0.0;
        ui.horizontal(|ui| {
            let response = look::section(ui, title);
            if let Some(help) = help {
                // **帮助字靠右**：量出它多宽，先空出这一行剩下的那一截再摆，摆不下就不空、从尾巴截掉。
                // 不走从右往左的横排——那一种在右栏的滚动区里有一回把帮助字摆到了这一栏外头
                // （第二段第三趟，`侧边详情摆封面或字卡…` 那条测试抓到的）。
                let 宽 =
                    crate::table::text_width(ui, help, &egui::TextStyle::Small.resolve(ui.style()));
                let 剩 = ui.available_size_before_wrap().x;
                if 宽 < 剩 {
                    ui.add_space((剩 - 宽).floor());
                }
                ui.add(egui::Label::new(egui::RichText::new(help).small().weak()).truncate());
            }
            response
        })
        .inner
    })
    .inner
}

/// 一簇**分面标签**（设计稿 `.facet`）：横着排、排不下就折行，标签与标签之间横竖都是令牌 `facet-gap`。
fn chip_cluster(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    let tokens = Tokens::builtin();
    ui.scope(|ui| {
        let gap = tokens.space.facet_gap;
        ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
        ui.spacing_mut().interact_size.y = tokens.layout.facet_chip_height;
        ui.horizontal_wrapped(add);
    });
}

/// 一维的**分面标签**：库里有哪几个值、各选中多少个变体（[`look::facet_chip`]）。点一下收窄到这个值，
/// 再点一下放开。
///
/// 空列表也画出一句话，而不是整块消失——「这个维度一条数据都没有」与「这个维度不在界面上」是两件事，
/// 后者会让人以为工具不支持按它筛。
fn facet_chips(
    ui: &mut egui::Ui,
    what: &str,
    facets: &[romcat_core::catalog::Facet],
    picked: &mut Option<String>,
) {
    if facets.is_empty() {
        look::help(ui, &format!("库里还没有{what}这一维的数据。"));
        return;
    }
    chip_cluster(ui, |ui| {
        for facet in facets {
            let on = picked.as_deref() == Some(facet.value.as_str());
            if look::facet_chip(ui, on, &facet.value, &thousands(facet.count)).clicked() {
                *picked = (!on).then(|| facet.value.clone());
            }
        }
    });
}

/// 平台那一维。它比别的多两样：**平台未知**在表里是 `NULL`，只能是独立的一支（[`PlatformFilter`]）；
/// 平台多，照稿先只摆令牌 `platforms-visible` 那么多个，其余收在「更多（N）」后头——选中了的照旧摆着
/// （设计稿 `renderPlatMore`）。
fn platform_chips(
    ui: &mut egui::Ui,
    facets: &[romcat_core::catalog::Facet],
    picked: &mut Option<PlatformFilter>,
    more: &mut bool,
) {
    if facets.is_empty() {
        look::help(ui, "库里还没有平台这一维的数据。");
        return;
    }
    let 先摆 = Tokens::builtin().layout.platforms_visible;
    let 收着 = facets.len().saturating_sub(先摆);
    chip_cluster(ui, |ui| {
        for (at, facet) in facets.iter().enumerate() {
            let filter = PlatformFilter::from_label(&facet.value);
            let on = picked.as_ref() == Some(&filter);
            if at >= 先摆 && !*more && !on {
                continue;
            }
            if look::facet_chip(ui, on, &facet.value, &thousands(facet.count)).clicked() {
                *picked = (!on).then_some(filter);
            }
        }
        if 收着 > 0 {
            let 字 = if *more {
                "收起".to_string()
            } else {
                format!("更多（{收着}）")
            };
            if look::more_chip(ui, &字).clicked() {
                *more = !*more;
            }
        }
    });
}

/// 条件组底下那一条**规则原文**（设计稿 `.ruletext`）：凹陷底、小圆角，前头一句弱字色的「存成子库规则：」，
/// 后头是核心库折出来的那条规则，等宽，哪儿都能折行。
fn rule_text(ui: &mut egui::Ui, rule: &str) {
    let tokens = Tokens::builtin();
    let [上下, 左右] = tokens.space.rule_text_padding;
    let (弱, 次) = (ui.visuals().weak_text_color(), ui.visuals().text_color());
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .corner_radius(tokens.radius.small)
        .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut job = egui::text::LayoutJob::default();
            job.append(
                "存成子库规则：",
                0.0,
                egui::TextFormat::simple(egui::FontId::proportional(tokens.font.size_path), 弱),
            );
            job.append(
                rule,
                0.0,
                egui::TextFormat::simple(egui::FontId::monospace(tokens.font.size_path), 次),
            );
            job.wrap.max_width = ui.available_width();
            job.wrap.break_anywhere = true;
            let galley = ui.painter().layout_job(job);
            ui.label(galley);
        });
}
