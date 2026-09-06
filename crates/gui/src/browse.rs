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
//! 2. **我要找的那一批在哪**——左边那一栏。上半是五个一按就有的档：**平台**、
//!    **合集**、**语言**、**中文**、**识别状态**；下半是一棵可嵌套的**条件组**
//!    （[`crate::filter`]），三种连接、九个运算符。**两半之间是且**，一律下推到中立库的
//!    `WHERE`，内存里永远只有当前视口那几十行。
//!
//!    **那棵条件组就是子库的规则**：筛到满意按「存成子库」，条件原样变成那个子库的
//!    规则（[`WorkQuery::to_rule`]）；反过来子库屏点「改选择」跳回来，规则预填进筛选器
//!    （[`Screen::begin_editing`]），调完按「更新到子库」原样换回去
//!    （[`Screen::update_sublibrary`]）。**例外也在这一趟里加减**——「哪一份」只有在
//!    详情面板里才指得准（[`Screen::set_exception`]）。
//!
//!    底下那块面板上还有一个**搜索框**（票 `gui-redesign/05`）。**它与筛选器不是一类
//!    东西**：筛选器管集合，它管**顺序**——打几个字，匹配得好的排前面，权重内置、
//!    不用配。三条路都找：屏上这个名字、**标题集合**里别的叫法（中文名就在这儿）、
//!    **简介**；命中在哪一条决定这一行排哪一档（`SearchHit`，折在中立库那一层）。
//!    **它进不了子库的规则**——子库要的是集合不是顺序，所以搜索框里还有字的时候
//!    「存成子库」当场挡住（挂单 Q70）。
//! 3. **这一行到底是什么**——右边那块面板的三层：**作品** → **变体**（每个带置信度与
//!    **依据**）→ **文件**（含附属文件与内部资源）→ **媒体**（封面与截图内嵌画出来，
//!    视频是一张抽出来的首帧加一个播放标，[`crate::media`]）。
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
//! - 在详情面板里选中某一个**变体** ＝ 变体级的操作只作用于它：改它的元数据、
//!   把首选变体裁给它、看它的文件。
//!
//! 两件事各有各的状态（[`Picked`] 与 [`Screen::variant_key`]），混成一件的话，
//! 翻着看就会把批量操作的范围改掉。
//!
//! ## 所有元数据编辑收敛在这里（ADR-0001 的修订段）
//!
//! 主库的 Pegasus 文件不再是编辑入口。于是这一屏必须真的改得动库：加一条**裁决**来源的
//! 叫法、删一条叫法、指定或撤销**首选变体**、写下一个刮削字段值。这几件事各自都只是
//! 一次中立库写入——领域判断一条都不在这里。
//!
//! ## 中文输入全在底下那块面板里
//!
//! 一个 [`egui::TextEdit`] 都不进表格单元格：表格是虚拟化的，正在组字的那一行一旦滚出
//! 视口，那个控件就不存在了（ADR-0005 的修订段）。
//!
//! 左边那一栏上半的五个维度是**选**出来的不是打出来的，值从中立库现问
//! （[`Catalog::facets`]）；下半那棵条件组里每条子句有一个值要打，而**那一栏不虚拟化**
//! ——一个 `ScrollArea` 把里面每一行都画出来，正在组字的那一行不会凭空消失。
//! 这条界线不是「表格 vs 面板」，是**虚拟化 vs 不虚拟化**。

use std::collections::BTreeMap;

use egui::{Align, Layout};
use romcat_core::catalog::browse::{
    Facets, PlatformFilter, Scope, WorkAnchor, WorkDetail, WorkQuery, WorkVariant,
};
use romcat_core::catalog::identify::Tier;
use romcat_core::catalog::{Catalog, VariantDetail};
use romcat_core::collection::{self, Applied, FAVORITE};
use romcat_core::report::{capacity, human_bytes, thousands};
use romcat_core::scrape::pool::MediaPool;
use romcat_core::scrape::priority::VERDICT;
use romcat_core::scrape::{AnchorKind, Field, Priorities};
use romcat_core::site::Site;
use romcat_core::sublibrary::{
    BrokenRule, Dimension, Discarded, Exception, ExceptionRow, LoadedSelection, Rule, Sublibrary,
};
use romcat_core::title::{Language, TitleKind};

use crate::filter::Filter;
use crate::layout;
use crate::look;
use crate::font;
use crate::media::Gallery;
use crate::scrape;
use crate::task::Tasks;
use crate::table::{Picked, SPAN, Table, Window};

/// 界面上人工写下的叫法，**依据**里写这一句。
///
/// 没有依据的结论事后无法复核（ADR-0002）。人工写的那条依据只能是「谁在哪儿写的」，
/// 但那也比空着强——半年后看见一个来路不明的中文名，至少知道它是自己敲的。
const HAND_WRITTEN: &str = "浏览屏的详情面板上人工写的";

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
/// - **长度**。超过 [`VALUE_SHOWN`] 个字就用省略号收住。
///
/// **原文一个字都没动**——收窄的只是画出来的那一行，整段挂在悬停里。
///
/// 它是公开的，因为「原样画得下的**一个字都不动**」这一条只有从**函数这一侧**看得见：
/// 屏上画的是同一串字，中间换没换过一份字符串出去，看画出来的那一帧看不出来。
///
/// 「收成一行画得下的那一截」那一半**钉在画出来的那一帧上**：那一行画在详情面板深处，
/// egui 不画视口之外的文字，但指针停进那一栏再滚真的滚轮就够得着它
/// （`crates/gui/tests/browse.rs` 的 `一条顶到闸上的简介收成一行画得下的那一截`，
/// 挂单 Q20）。
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

/// 写下一个**刮削字段值**时界面上那份草稿。
#[derive(Debug, Clone)]
pub struct ValueDraft {
    /// 哪个字段。
    pub field: Field,
    /// 挂在**作品**上还是**变体**上。年份挂作品、汉化组挂变体（ADR-0012）。
    pub anchor: AnchorKind,
    /// 值本身。**会碰到输入法**，所以它在详情面板里。
    pub value: String,
}

impl Default for ValueDraft {
    fn default() -> Self {
        Self {
            // 简介是最想手写的那一个：离线档撞上一条中文条目就有（`scrape::zh`），
            // 而撞不上的那些正是没人替它写过一句话的。
            field: Field::Description,
            anchor: AnchorKind::Work,
            value: String::new(),
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
/// 那条横幅里（[`Screen::broken_rules_ui`]），只给一个「扔掉这条」。
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
    /// 筛选与排序。**界面上这一份是源头**，[`Window`] 里那一份是它的副本，每帧同步一次。
    query: WorkQuery,
    /// 五个维度各有哪些值可选。换库或改过元数据才重问。
    facets: Facets,
    /// **筛选器**：那棵可嵌套的条件组。它折出来的规则每帧同步进 [`Self::query`]。
    filter: Filter,
    /// **当前筛选下一共多少个变体**。`None` 是数不出来，不是零。
    ///
    /// 与 [`Self::scope`] 不是一个数：这一个是**筛出来的全部**，那一个是**选中的那几行
    /// 展开出来的**。屏上两个都写，因为按批量操作之前要分得清「筛出来多少」与
    /// 「我勾了多少」。
    filtered: Option<u64>,
    /// 「存成子库」那两个格子：名字与目标路径。
    save: SaveDraft,
    /// **刮削面板**：抬头那个「刮削选中…」摊开的就是它（票 `gui-redesign/10`）。
    ///
    /// 它住在这一屏里而不是自成一屏，是因为它的**范围**就是这一屏筛出来的那一批——
    /// 挪到别处去，那批东西就得再传一遍，而传着传着两边的数就对不上了。
    scrape: scrape::Panel,
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
    /// 高亮的是哪一行（全序下标）。
    focused: Option<u64>,
    /// 选中了哪几行——**批量操作的作用范围**。与 [`Self::focused`] 不是一回事。
    picked: Picked,
    /// 点开的那一行是谁。**记身份不记下标**：换个筛选表就重排了，下标会指到别人身上。
    opened: Option<WorkAnchor>,
    /// 点开那一行的详情：作品 → 变体。
    work: Option<WorkDetail>,
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
    /// **媒体池**：查「这张图在不在」用它。池子整个不在位时是 `None`——
    /// 那时面板如实说「没查池子」，而不是报一句「一张都没有」。
    pool: Option<MediaPool>,
    /// 详情面板那几格**缩略图**（票 `gui-redesign/07`）。解码与抽首帧全在核心库，
    /// 这里只握着一条后台线程的把手与传上显卡的那几张纹理（[`crate::media`]）。
    gallery: Gallery,
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
    /// 加一条叫法的草稿。
    title_draft: TitleDraft,
    /// 写下一个刮削字段值的草稿。
    value_draft: ValueDraft,
    /// 上一次动作的回执。
    notice: Option<String>,
    /// 上一次出的错。
    error: Option<String>,
    /// 字体样张开着没有。
    sample: bool,
    /// 把表格的滚动位置强按到这个像素偏移。**只有量帧率时才设**，真界面上永远是 `None`。
    pub scroll_to: Option<f32>,
}

impl Screen {
    /// 开一个空屏幕。
    ///
    /// `workspace` 是**工作目录**：刮削面板要它找媒体池、优先级表、中文离线索引与
    /// 沉淀库（`scrape::Panel`）。
    #[must_use]
    pub fn new(workspace: std::path::PathBuf) -> Self {
        Self {
            window: Window::new(SPAN),
            query: WorkQuery::default(),
            facets: Facets::default(),
            filter: Filter::default(),
            filtered: None,
            save: SaveDraft::default(),
            scrape: scrape::Panel::new(workspace),
            editing: None,
            returned: None,
            touched: None,
            focused: None,
            picked: Picked::default(),
            opened: None,
            work: None,
            variant: None,
            detail: None,
            scope: None,
            scoped: None,
            priorities: Priorities::builtin(),
            pool: None,
            gallery: Gallery::new(),
            collection: String::new(),
            standing: Vec::new(),
            standing_for: None,
            title_draft: TitleDraft::default(),
            value_draft: ValueDraft::default(),
            notice: None,
            error: None,
            sample: false,
            scroll_to: None,
        }
    }

    /// 换一份优先级表。**导出用哪一份，这里就该用哪一份**——两份不一样的话，
    /// 面板上写着的显示标题就不是同步到掌机上会看见的那个。
    pub fn set_priorities(&mut self, priorities: Priorities) {
        self.priorities = priorities;
    }

    /// 指一份**媒体池**。**目录不在就不指**——「没查」与「查了、没有」得分得开。
    pub fn set_pool(&mut self, pool: Option<MediaPool>) {
        // **两处一起换。** 那一栏问「在不在池子里」用 `pool`，画那几格图用 `gallery`
        // 手里那条后台线程——只换一处的话，屏上会一边说「池里有」一边一格图都画不出。
        self.gallery.set_pool(pool.clone());
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
        self.reload(site);
        self.load_work(&site.catalog);
        self.load_detail(&site.catalog);
    }

    /// 重问一次筛选面板上的可选值。开库时与改过元数据之后各一次。
    pub fn reload(&mut self, site: &Site) {
        match site.catalog.facets() {
            Ok(facets) => {
                self.facets = facets;
                self.error = None;
            }
            Err(error) => self.error = Some(format!("中立库读不动：{error}")),
        }
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
        });
        self.reload_exceptions(site);
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
        let Some(name) = self.editing.as_ref().map(|editing| editing.sublibrary.clone()) else {
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
        match site.catalog.replace_rules(&name, &rule) {
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
                self.notice = Some(format!("给「{name}」记下了一条{}例外：{key}", kind.label()));
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
        let Some(name) = self.editing.as_ref().map(|editing| editing.sublibrary.clone()) else {
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
    /// 于是原样摆在筛选栏顶上那条横幅里（[`Self::broken_rules_ui`]），
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
        let Some(name) = self.editing.as_ref().map(|editing| editing.sublibrary.clone()) else {
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
        let Some(name) = self.editing.as_ref().map(|editing| editing.sublibrary.clone()) else {
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

    /// 写下一个刮削字段值的草稿，供实测与测试填。
    pub fn value_draft_mut(&mut self) -> &mut ValueDraft {
        &mut self.value_draft
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

    /// 把草稿里那个字段值写下，**来源记作裁决**。
    ///
    /// 优先级表把裁决排在每个字段的最前，所以写下之后**导出真会用它**。
    /// 界面上「写下」那个按钮走的就是它。
    pub fn put_value(&mut self, site: &mut Site, subject: &str) {
        let value = self.value_draft.value.trim().to_string();
        if value.is_empty() {
            return;
        }
        match site.catalog.put_verdict_value(
            self.value_draft.anchor,
            subject,
            self.value_draft.field,
            &value,
            HAND_WRITTEN,
        ) {
            Ok(()) => {
                self.notice = Some(format!(
                    "{} 记成了「{value}」，来源是裁决——导出会用它。",
                    self.value_draft.field.label(),
                ));
                self.value_draft.value.clear();
                self.load_detail(&site.catalog);
            }
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 撤掉一条**裁决**来源的字段值，让别的源重新说了算。
    pub fn clear_value(
        &mut self,
        site: &mut Site,
        anchor: AnchorKind,
        subject: &str,
        field: Field,
    ) {
        match site.catalog.clear_verdict_value(anchor, subject, field) {
            Ok(true) => {
                self.notice = Some(format!("撤掉了人工写的{}。", field.label()));
                self.load_detail(&site.catalog);
            }
            Ok(false) => self.notice = Some("本来就没人写过。".to_string()),
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
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

    /// 从**标题集合**里删掉一条叫法。界面上那个「删」走的就是它。
    pub fn remove_title(
        &mut self,
        site: &mut Site,
        work: &str,
        language: Language,
        kind: TitleKind,
        source: &str,
        value: &str,
    ) {
        match site
            .catalog
            .remove_title(work, language, kind, source, value)
        {
            Ok(true) => {
                self.notice = Some(format!("删掉了叫法「{value}」。"));
                self.load_detail(&site.catalog);
            }
            Ok(false) => self.notice = Some("那一条已经不在了。".to_string()),
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
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
        let Some(anchor) = self.opened.clone() else {
            self.work = None;
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

    /// 顶栏上属于这一屏的那一段。
    ///
    /// 先同步一次窗口再画：顶栏与正文各画各的，而顶栏**先画**——不先同步，
    /// 状态栏上那个行数就永远比表格慢一帧（与队列那一屏 `status` 同一条道理）。
    pub fn status(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        self.sync_window(&site.catalog);
        ui.toggle_value(&mut self.sample, "字体样张");
        // **「刮削选中…」摆在抬头**，与原型同一个位置。它只摊开面板——真按下去那一下
        // 在面板底下，因为按之前该先看清那本账。
        if ui
            .button("刮削选中…")
            .on_hover_text(
                "对筛出来的这一批取元数据与媒体。四个旋钮定清楚要干什么，\
                 **按下去之前就看得见会发多少网络请求、大概多久**。",
            )
            .clicked()
        {
            self.open_scrape(&site.catalog);
        }
        // **「★ 收藏」也摆在抬头**，与原型同一个位置：它是这一屏按得最勤的一下
        // （勾一批、按一下、接着筛下一批）。取消收藏与自建合集是低频的，
        // 摆在左栏底下那块「把这批选中变成持久的东西」里，与「存成子库」做邻居。
        if ui
            .button("★ 收藏")
            .on_hover_text(
                "把勾中的那一批全放进**收藏**。落**沉淀库**、锚在**内容**上——\
                 删掉中立库重扫、改名、挪目录都还在。**无判据**的那些只钉得住本机路径，\
                 按完的回执里会点名说有几个。\n\n\
                 **取消收藏**与自建合集在左栏底下那块「收藏与合集」里：\
                 加收藏按得最勤，所以只有它在抬头（挂单 Q117）。",
            )
            .clicked()
        {
            self.favorite(site);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if let Some(error) = self.window.error() {
                ui.colored_label(ui.visuals().error_fg_color, error);
            } else {
                let picked = self.picked.count(self.window.total());
                ui.label(format!(
                    "{} 个作品；选中 {} 条、作用于 {} 个变体｜内存里 {} 行、读库 {} 次",
                    thousands(self.window.total()),
                    thousands(picked),
                    scope_label(self.scope),
                    self.window.retained(),
                    self.window.reads(),
                ));
            }
        });
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

    /// 刮削面板，供测试与实测拨旋钮、按「加入任务队列」。
    pub fn scrape_mut(&mut self) -> &mut scrape::Panel {
        &mut self.scrape
    }

    /// **按「刮削选中…」那一下**：把这一批展开成变体的键，摊开刮削面板。
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

    /// 按「**★ 收藏**」那一下：把勾中的那一批全放进[收藏](FAVORITE)。
    ///
    /// 界面上那颗按钮走的就是它，测试拿它当那一下。
    pub fn favorite(&mut self, site: &mut Site) {
        self.join(site, FAVORITE);
    }

    /// 按「**☆ 取消收藏**」那一下。
    pub fn unfavorite(&mut self, site: &mut Site) {
        self.part(site, FAVORITE);
    }

    /// 按「**加入合集**」那一下：格子里那个名字。
    pub fn join_collection(&mut self, site: &mut Site) {
        let name = self.collection.trim().to_string();
        self.join(site, &name);
    }

    /// 按「**移出合集**」那一下。
    pub fn leave_collection(&mut self, site: &mut Site) {
        let name = self.collection.trim().to_string();
        self.part(site, &name);
    }

    /// 把勾中的那一批放进一个合集。
    ///
    /// **一个都没勾就别动库**：与「刮削选中…」同一条规矩——摆出一份「作用于 0 个变体」
    /// 的回执，人只会对着它猜哪儿出了问题。
    fn join(&mut self, site: &mut Site, name: &str) {
        let Some(keys) = self.scoped_keys(&site.catalog, "加收藏") else {
            return;
        };
        match collection::add(site, name, &keys) {
            Ok(applied) => self.settle(site, name, applied, true),
            Err(error) => self.error = Some(format!("{error}")),
        }
    }

    /// 把勾中的那一批从一个合集里拿出来。
    fn part(&mut self, site: &mut Site, name: &str) {
        let Some(keys) = self.scoped_keys(&site.catalog, "取消收藏") else {
            return;
        };
        match collection::remove(site, name, &keys) {
            Ok(applied) => self.settle(site, name, applied, false),
            Err(error) => self.error = Some(format!("{error}")),
        }
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
                "其中 {} 个只钉得住**本机的路径**——那些变体拿不到内容判据（**无判据**那一档），\
                 改名或挪到别的目录就认不出来了；另外 {} 个钉在**内容**上，\
                 重扫、改名、挪目录都还认得出。",
                thousands(applied.path as u64),
                thousands(applied.content as u64),
            ));
        } else if joining {
            line.push_str("全部钉在**内容**上——重扫、改名、挪目录都还认得出。");
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

    /// 画一帧。
    pub fn ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        self.sync_window(&site.catalog);
        self.sync_standing(site);
        // **任务台上有活在跑就先不写库**：那时后台正拿着另一份写得动的连接（扫描），
        // 这条线程上的写会在 `busy_timeout` 上等最长十秒——那是画帧线程的十秒。
        self.sync_media(ui.ctx(), site, !tasks.busy());
        // **四条边界都拖得动，四条都记得住**：怎么拖、拖到哪儿为止、拖到哪儿记在哪儿，
        // 全在 [`crate::layout`] 那一份声明里（票 `gui-redesign/12`）。
        if self.scrape.is_open() {
            layout::SCRAPE.show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("刮削面板")
                    .show(ui, |ui| self.scrape.ui(ui, site, tasks));
            });
        }
        layout::EDIT.show(ui, |ui| self.edit_panel(ui, site));
        layout::FILTER.show(ui, |ui| self.filter_panel(ui, site));
        layout::DETAIL.show(ui, |ui| self.detail_panel(ui, site));
        egui::CentralPanel::default().show(ui, |ui| {
            if self.sample {
                self.font_sample(ui);
                ui.separator();
            }
            let opened = Table {
                catalog: &site.catalog,
                window: &mut self.window,
                query: &mut self.query,
                focused: &mut self.focused,
                picked: &mut self.picked,
                scroll_to: self.scroll_to,
            }
            .show(ui);
            if let Some(row) = opened {
                self.open_work(&site.catalog, &row.anchor);
            }
        });
    }

    /// 左边那栏：上半五个一按就有的档，下半那棵**条件组**。
    fn filter_panel(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        // **扔掉一条坏规则那一下不能在画的中途走**：`self.editing` 那会儿还借着。
        // 记下序号，这一栏画完再动手。
        let mut 扔掉 = None;
        egui::ScrollArea::vertical()
            .id_salt("筛选栏")
            .show(ui, |ui| {
                if let Some(editing) = &self.editing {
                    // **正在替谁改，一进屏就看得见**：这一栏的每一下都会落到那个子库上。
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!("正在改子库「{}」的选择集", editing.sublibrary),
                    );
                    ui.weak("调完去底下那块面板按「更新到子库」。");
                    扔掉 = Self::broken_rules_ui(ui, &editing.broken);
                    ui.separator();
                }
                ui.horizontal(|ui| {
                    ui.strong("筛选");
                    if ui
                        .button("全清")
                        .on_hover_text(
                            "把这一栏的条件全部清掉。**排序与搜索框不动**——\
                             搜索管排序、筛选器管集合，这颗按钮只管后者。",
                        )
                        .clicked()
                    {
                        // **按钮体只有这一句。** 把那几行抄在这儿的话，钉着
                        // [`Self::clear_filter`] 的那条测试就钉不到界面上这一下——
                        // 改了这儿它照样绿，而那正是这条修复要防的漂移。
                        self.clear_filter();
                    }
                });
                ui.weak("上下两半之间是「且」：一层层收窄。全部下推到中立库。")
                    .on_hover_text(
                        "**这就是子库的规则**：筛到满意按「存成子库」，条件原样变成那个\
                         子库的规则；反过来子库屏点「改选择」跳回这里，规则预填进筛选器。",
                    );
                ui.separator();

                // **两半各管一段，屏上说清**（挂单 `Q73`）：上半那五个档，条件组里
                // 大多也写得出来（`平台=GB`……），两套并存看着像同一件事有两个地方点。
                // 留着两套是因为它们回答的不是同一个问题——上半**各自带着条数**
                // （`Catalog::facets` 一次 `GROUP BY` 问出来的），那是用来**摸清库里有
                // 什么**的；条件组里的值要打出来，那是用来**说清楚要哪一批**的。
                // 没有条数的下拉框，人只能一个个点开试。
                ui.strong("一按就有的档").on_hover_text(
                    "**探索用的那一半**：五个维度各带着条数，点一下就收窄一层，\
                     不必先知道值长什么样。「识别状态」这一维只在这儿有，\
                     条件组里写不出来。",
                );
                ui.weak("各带着条数——点一下就知道库里有多少。");

                platform_picker(ui, &self.facets.platforms, &mut self.query.platform);
                ui.separator();
                facet_picker(
                    ui,
                    "合集",
                    "用户自定义的一组游戏，与平台正交（ADR-0011）。",
                    &self.facets.collections,
                    &mut self.query.collection,
                );
                ui.separator();
                facet_picker(
                    ui,
                    "语言",
                    "**发行版**标着的语言码。汉化版不在这一维里——它是变体，底版多半是日版。",
                    &self.facets.languages,
                    &mut self.query.language,
                );
                ui.separator();
                facet_picker(
                    ui,
                    "中文",
                    "**变体**的中文身份：汉化 / 官中（ADR-0012）。这个库最要紧的那批全在这儿。",
                    &self.facets.chinese,
                    &mut self.query.chinese,
                );
                ui.separator();

                ui.strong("识别状态");
                let mut state = self.query.state;
                if ui.selectable_label(state.is_none(), "不筛").clicked() {
                    state = None;
                }
                for (filter, count) in &self.facets.states {
                    let on = state == Some(*filter);
                    if ui
                        .selectable_label(on, format!("{}  {}", thousands(*count), filter.label()))
                        .clicked()
                    {
                        state = if on { None } else { Some(*filter) };
                    }
                }
                self.query.state = state;
                ui.separator();

                ui.strong("条件组")
                    .on_hover_text(
                        "**表达用的那一半**：可嵌套的条件组，每组选「全部满足 / 任一满足 /\
                         都不满足」，组里还能再套组，九个运算符。**这就是子库的规则**——\
                         上头那五个档存成子库时也会折进同一条规则里\
                         （「识别状态」那一维折不进去，挂单 Q70）。",
                    );
                ui.weak("值要自己打——说得清楚，也存得成子库的规则。");
                if self.filter.ui(ui) {
                    // 条件组一改就是换了一批行——同步进查询，`sync_window` 那一趟
                    // 会把窗口作废重取，选中也跟着清掉。
                    self.query.rule = self.filter.rule().cloned();
                }
                ui.separator();

                // **筛出多少条当场写出来**：按批量操作之前心里有数。
                ui.label(format!(
                    "筛出 {} 行 · {} 个变体",
                    thousands(self.window.total()),
                    scope_label(self.filtered),
                ))
                .on_hover_text(
                    "行数照的是「作品数 ＋ 还没认出作品的变体数」；\
                     变体数是这批行底下的全部变体，按当前筛选。",
                );
                ui.separator();

                self.collection_panel(ui, site);
                ui.separator();

                self.save_panel(ui, site);
            });
        // **这一栏画完了再动手**：搁在中途走的话，这一帧余下的半栏是照旧那份数据画的。
        if let Some(ordinal) = 扔掉 {
            self.discard_broken_rule(site, ordinal);
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
                "这个子库另有 {} 条规则**读不懂**：没参与求值，「更新到子库」也不碰它们。",
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
                    "它们进不了下面的筛选器——那是一棵**读得懂**的树。这儿只给一个动作：\
                     扔掉。要的东西改对了再筛一遍，按「更新到子库」带回去。",
                );
                for row in broken {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        format!("{}. {}（读不懂：{}）", row.ordinal, row.text, row.error),
                    );
                    if ui
                        .button("扔掉这条")
                        .on_hover_text(
                            "**只删这一条**：读得懂的那几条与全部例外一个都不碰\
                             （`Catalog::discard_broken_rule` 先读一遍再决定删不删，\
                             读得懂的它拒绝）。扔掉不改变这个子库选出什么\
                             ——它本来就没参与求值。",
                        )
                        .clicked()
                    {
                        扔掉 = Some(row.ordinal);
                    }
                }
            });
        扔掉
    }

    /// 「**收藏与合集**」那一块：取消收藏，以及往自建合集里加减（票 `gui-redesign/06`）。
    ///
    /// 它与「存成子库」做邻居**是故意的**：这两块管的是同一件事的两种落法——把屏上这一批
    /// 变成持久的东西。一个存成规则（子库要的是集合），一个存成成员关系（收藏是人亲手
    /// 点的，规则表达不了）。
    ///
    /// **加收藏那一下不在这儿，在抬头**（原型钉的位置）：它按得最勤，不该藏在左栏底下。
    fn collection_panel(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        ui.strong("收藏与合集")
            .on_hover_text(
                "**加收藏那一下在抬头**（「★ 收藏」），因为它按得最勤：\
                 勾一批、按一下、接着筛下一批。这儿是它的另一半——取消，\
                 以及自己起名的合集（挂单 Q117）。",
            );
        ui.weak("作用范围是**勾中的那一批**（不是筛出来的全部）。落沉淀库，删掉中立库重扫也不丢。")
            .on_hover_text(
                "收藏走的就是合集那套成员关系——收藏是名字定死的那一组，\
                 自建合集是自己起名的那些。筛的时候写 `收藏=是` 或 `合集=某某`。",
            );
        if ui
            .button("☆ 取消收藏")
            .on_hover_text("把勾中的那一批从收藏里拿出来。**两种锚都拿**，星星不会点不灭。")
            .clicked()
        {
            self.unfavorite(site);
        }
        ui.horizontal(|ui| {
            ui.label("合集");
            ui.add(
                egui::TextEdit::singleline(&mut self.collection)
                    .desired_width(140.0)
                    .hint_text("通关过的"),
            );
        });
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
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!name.is_empty(), egui::Button::new("加入合集"))
                .on_hover_text("没有这个合集就顺手建出来——**一个合集就是它那些成员**。")
                .clicked()
            {
                self.join_collection(site);
            }
            if ui
                .add_enabled(!name.is_empty(), egui::Button::new("移出合集"))
                .on_hover_text("一条成员都不剩的合集，从筛选栏那一维里消失。")
                .clicked()
            {
                self.leave_collection(site);
            }
        });
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
        ui.strong("存成子库");
        let folded = self.query.to_rule();
        match &folded {
            Ok(None) => {
                ui.weak("一个条件都没筛——存出来的子库就是整个库。先筛一批。");
            }
            Ok(Some(rule)) => {
                ui.weak(format!("规则会是：{rule}"));
            }
            Err(unruly) => {
                ui.colored_label(ui.visuals().error_fg_color, unruly.advice());
            }
        }
        for (label, value, hint) in [
            ("名字", &mut self.save.name, "一台目标设备一个"),
            ("目标路径", &mut self.save.target, "读卡器挂上来的那个目录"),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.add(
                    egui::TextEdit::singleline(value)
                        .desired_width(140.0)
                        .hint_text(hint),
                );
            });
        }
        let ready = !self.save.name.trim().is_empty()
            && !self.save.target.trim().is_empty()
            && matches!(folded, Ok(Some(_)));
        if ui
            .add_enabled(ready, egui::Button::new("存成子库"))
            .on_hover_text("把当前筛选原样变成这个子库的规则。前端格式与容量上限去子库屏调。")
            .clicked()
        {
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
        ui.strong(format!("正在改子库「{name}」的选择集"));
        ui.weak("这个子库的规则已经预填在上面的筛选器里。调完按「更新到子库」原样带回。");
        if broken > 0 {
            // **处置它们的地方在这一栏顶上**（票 `gui-redesign/14`）：这儿说的是
            // 「更新到子库」的承诺——那一趟只换读得懂的那几条，坏的一条都不碰。
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!(
                    "这个子库另有 {} 条规则读不懂：它们没参与求值，也**不会**被这一趟改掉。\
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
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    matches!(folded, Ok(Some(_))),
                    egui::Button::new("更新到子库"),
                )
                .on_hover_text(
                    "把屏上这份筛选原样换成那个子库的规则，然后回子库屏。\
                     **换掉而不是加上去**：加的话子库选出来的会比屏上多。",
                )
                .clicked()
            {
                self.update_sublibrary(site);
            }
            if ui
                .button("不改了")
                .on_hover_text("放下这一趟，筛选留在屏上不动。**已经记下的例外不撤**——那是各自独立的决定。")
                .clicked()
            {
                self.cancel_editing();
            }
        });
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
                    Ok(_) => format!("规则写不进中立库：{error}。刚建的子库「{name}」已经退掉，这个名字还能用。"),
                    Err(second) => format!(
                        "规则写不进中立库：{error}。而刚建的子库「{name}」也退不掉（{second}）\
                         ——它眼下一条规则都没有，同步过去会是空的，去子库屏删掉它。"
                    ),
                });
            }
        }
    }

    /// 右边那块面板：**作品 → 变体 → 文件**。
    ///
    /// **这一份不复制一遍再画**：一行底下可以挂着上百个变体、每个又带着几条候选，
    /// 每帧克隆一次就是每帧几百次分配。所以画的时候只借（`as_ref`），点中哪个变体
    /// 攒在 `pick` 里，出了这个闭包再去改自己。
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
        let items = self
            .detail
            .as_ref()
            .map(|detail| detail.media_items.clone())
            .unwrap_or_default();
        self.gallery.sync(ctx, &mut site.catalog, &items, writable);
    }

    fn detail_panel(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        if self.work.is_none() {
            ui.add_space(4.0);
            ui.weak("点主列表里的一行，看它包含哪几个变体。");
            return;
        }
        let mut pick: Option<String> = None;
        // 点了哪一格图。
        let mut open: Option<crate::media::Clicked> = None;
        let this = &*self;
        let Some(work) = this.work.as_ref() else {
            return;
        };
        egui::ScrollArea::vertical()
            .id_salt("作品详情")
            .show(ui, |ui| {
                ui.strong(&work.name);
                ui.label(format!(
                    "{}｜{}｜{} 个变体",
                    if work.platforms.is_empty() {
                        "—".to_string()
                    } else {
                        work.platforms.join(" / ")
                    },
                    work.year.as_deref().unwrap_or("年份不详"),
                    work.variants.len(),
                ));
                if matches!(work.anchor, WorkAnchor::Loose(_)) {
                    // 没有作品链接**本身就是一条信息**：识别还没认出它属于哪个作品，
                    // 于是它自成一行（与导出那一侧同一条口径）。
                    ui.weak("识别还没认出它属于哪个作品，所以这一行就是它自己。");
                }

                ui.separator();
                ui.strong(format!("变体 {} 个", work.variants.len()));
                ui.weak("点一个：底下的文件、媒体与元数据编辑就只作用于它。");
                for variant in &work.variants {
                    if this.variant_row(ui, variant) {
                        pick = Some(variant.row.key.clone());
                    }
                }

                ui.separator();
                this.collections_ui(ui);
                ui.separator();
                this.files_ui(ui);
                ui.separator();
                open = this.media_ui(ui);
            });
        if let Some(key) = pick {
            self.pick(&site.catalog, &key);
        }
        match open {
            None => {}
            // **窗口里一个字节都不解码**：播放与看原图都交给系统默认程序
            // （规格的 Out of Scope）。调不起来时如实说一句，不崩。
            Some(crate::media::Clicked::Open(at)) => {
                match romcat_core::scrape::preview::open_externally(&at) {
                    Ok(()) => {
                        self.notice = Some(format!(
                            "交给系统默认程序打开：{}",
                            romcat_core::path::display(&at),
                        ));
                    }
                    Err(说的) => self.error = Some(说的),
                }
            }
            // 点了、可这一格指不出文件。**说一句为什么**，别让人以为界面坏了。
            Some(crate::media::Clicked::Nothing(为什么)) => self.notice = Some(为什么),
        }
    }

    /// 详情面板里的一个变体：置信度、**依据**、点得中。点中了返回 `true`。
    ///
    /// **置信度那一档走五屏共用的那一份**（[`crate::look`]，票 `gui-redesign/12`）：
    /// 从前这儿手写「高 / 中 / 低」，而中间那张表与待确认屏写的是「高置信 / 中置信 /
    /// 低置信」——同一个变体在两处是两个词。眼下色条与词都出自一处，
    /// 而且**两样一起出现**：色觉障碍下读得出来的只有词。
    fn variant_row(&self, ui: &mut egui::Ui, variant: &WorkVariant) -> bool {
        let on = self.variant.as_deref() == Some(variant.row.key.as_str());
        let tier = Tier::of(variant.confidence());
        let line = format!(
            "{}｜{}｜{}",
            tier.label(),
            variant.row.key,
            capacity(variant.row.bytes, variant.row.unreadable_files),
        );
        // 色条与那一行摆在同一条横排里：条在左边缘，与主列表、待确认屏的卡一个样子。
        let response = ui
            .horizontal(|ui| {
                look::tier_bar(ui, tier);
                ui.selectable_label(on, line)
            })
            .inner;
        // **依据挂在悬停里**：没有依据的候选事后无法复核（ADR-0002），
        // 而一条依据能有一整句话，摆在行上会把这一栏撑开。
        let response = response.on_hover_ui(|ui| {
            ui.set_max_width(420.0);
            ui.label(format!(
                "识别结论：{}",
                variant.state.map_or("还没识别", |state| state.label()),
            ));
            if let Some(reason) = &variant.reason {
                ui.label(format!("为什么没定下来：{reason}"));
            }
            if variant.candidates.is_empty() {
                ui.label("一条候选都没有——那是**还没识别**，不是「撞过没撞上」。");
            }
            for candidate in variant.candidates.iter().take(TOP_CANDIDATES) {
                ui.separator();
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
        });
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
        ui.strong(format!("收藏与合集 · {} 个", self.standing.len()));
        if self.standing.is_empty() {
            ui.weak("一个都没进。勾几行按抬头那颗「★ 收藏」，或者在左栏底下加进自建合集。");
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
                    "这个变体拿不到**内容判据**（**无判据**那一档：容器穿不透、压缩镜像、\
                     目录树转储），所以只钉得住它眼下这个位置。改名或挪到别的目录之后，\
                     这一条就认不出来了。与**裁决**是同一个限制。",
                );
            }
        }
    }

    /// 详情面板第三层：选中那个变体的**全部文件**，含附属文件与内部资源。
    fn files_ui(&self, ui: &mut egui::Ui) {
        let Some(detail) = &self.detail else {
            ui.weak("选一个变体，看它有哪些文件。");
            return;
        };
        ui.strong(format!("文件 · {} 个", detail.members.len()));
        for (key, role) in detail.members.iter().take(TOP_MEMBERS) {
            ui.label(format!("{}  {key}", role.code()));
        }
        if detail.members.len() > TOP_MEMBERS {
            ui.weak(format!(
                "……另有 {} 个没列",
                detail.members.len() - TOP_MEMBERS
            ));
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
        ui.strong(format!("媒体 · {} 件", detail.media_items.len()));
        // **几格图**：一行摆得下几格摆几格，照原型 `prototype.html` 那张 `.thumbs` 网格。
        // 格子的大小跟着这块面板的宽度走（[`crate::media::cell_size`]，挂单 `Q126`）——
        // 拖宽了就每格大一点，而不是右边空出一条。
        let size = crate::media::cell_size(ui.available_width(), ui.spacing().item_spacing.x);
        let mut open = None;
        ui.horizontal_wrapped(|ui| {
            for item in &detail.media_items {
                if let Some(点的) = self.gallery.cell(ui, item, size) {
                    open = Some(点的);
                }
            }
        });
        if detail.media_items.is_empty() {
            ui.weak("一条媒体引用都没有。");
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
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!("{是哪一件}：{为什么}"),
            );
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
                        (Some(_), _) => "**池里没有这个文件**".to_string(),
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
                    "有 {} 条引用在**媒体池**里找不到那个文件，导出时一张都铺不出去。",
                    thousands(detail.dangling_media()),
                ),
            );
        }
        open
    }

    /// 底下那块面板：**改**选中那个变体的元数据。这一栏里的每一个文本框都会碰到输入法。
    fn edit_panel(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        ui.add_space(4.0);
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if let Some(notice) = &self.notice {
            ui.colored_label(ui.visuals().warn_fg_color, notice);
        }
        // 筛选框在这块面板里而不在左栏，与队列那一屏同一条规矩：会碰到输入法的控件
        // 全收在**不虚拟化**的面板里（ADR-0005）。筛选本身照旧下推到中立库。
        ui.horizontal(|ui| {
            ui.label("搜索");
            ui.add(
                egui::TextEdit::singleline(&mut self.query.search)
                    .desired_width(260.0)
                    .hint_text("打几个字，匹配得好的排前面"),
            )
            .on_hover_text(
                "**搜索管排序，筛选器管集合。** 三条路都找：屏上这个名字、\
                 **标题集合**里别的叫法（中文名就在这儿）、**简介**。\
                 命中在哪一条决定这一行排哪一档，权重内置、不用配。\n\
                 它**进不了子库的规则**——子库要的是集合不是顺序，\
                 「存成子库」之前得先把它清空。",
            );
            ui.separator();
            ui.label(format!(
                "选中 {} 条，作用于 {} 个变体",
                thousands(self.picked.count(self.window.total())),
                scope_label(self.scope),
            ))
            .on_hover_text(
                "**选中主列表的行 ＝ 选中这些作品，批量操作作用于它们的变体。**\
                 刮削（票 10）、存成子库（票 11）、加收藏（票 06）按下去动的就是这一批。",
            );
            if ui.button("全不选").clicked() {
                self.picked.clear();
            }
        });
        ui.separator();
        if self.detail.is_none() {
            ui.weak("在右边选一个变体，改它的元数据。**改动只作用于那一个变体。**");
            return;
        }
        let available = ui.available_width();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2((available * 0.42).max(240.0), ui.available_height()),
                Layout::top_down(Align::Min),
                |ui| self.facts_column(ui),
            );
            ui.separator();
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), ui.available_height()),
                Layout::top_down(Align::Min),
                |ui| self.edit_column(ui, site),
            );
        });
    }

    /// 左半：选中那个变体是什么。识别结论、作品、发行版、合集。
    fn facts_column(&mut self, ui: &mut egui::Ui) {
        let Some(detail) = &self.detail else {
            return;
        };
        egui::ScrollArea::vertical()
            .id_salt("变体详情")
            .show(ui, |ui| {
                ui.strong(&detail.row.key);
                ui.label(format!(
                    "平台 {}｜成型规则 {}｜{} 个文件｜{}",
                    detail.row.platform.as_deref().unwrap_or("未知"),
                    detail.row.rule,
                    detail.row.files,
                    capacity(detail.row.bytes, detail.row.unreadable_files),
                ));
                ui.label(match detail.state {
                    Some(state) => format!("识别结论：{}", state.label()),
                    None => "识别结论：还没识别".to_string(),
                });
                if let Some(reason) = &detail.reason {
                    ui.label(format!("为什么没定下来：{reason}"));
                }
                ui.label(format!(
                    "作品：{}",
                    detail.work.as_deref().unwrap_or("还没认出来"),
                ));
                match &detail.release {
                    Some(release) => ui.label(format!(
                        "发行版：地区 {}｜序列号 {}｜语言 {}",
                        release.region.as_deref().unwrap_or("—"),
                        release.serial.as_deref().unwrap_or("—"),
                        if detail.languages.is_empty() {
                            "—".to_string()
                        } else {
                            detail.languages.join(", ")
                        },
                    )),
                    // 没有发行版链接**本身就是一条信息**：同人移植与 homebrew 直接挂在
                    // 作品下，识别管线不拿它们去撞 DAT（`CONTEXT.md` 的「变体」词条）。
                    None => ui.label("发行版：没有链接——同人移植与 homebrew 就是这样"),
                };
                ui.label(format!(
                    "合集：{}",
                    if detail.collections.is_empty() {
                        "—".to_string()
                    } else {
                        detail.collections.join("、")
                    },
                ));
            });
    }

    /// 右半：**改**。标题集合、首选变体、刮削字段值。
    fn edit_column(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        let Some(detail) = self.detail.clone() else {
            return;
        };
        let mut dirty = false;
        egui::ScrollArea::vertical()
            .id_salt("元数据编辑")
            .show(ui, |ui| {
                // **例外只在「改选择」那一趟里露面**：它是子库的东西，不是变体的属性。
                // 平常浏览时摆一个「排除掉」在这儿，人会问「排除出哪儿」。
                if self.editing.is_some() {
                    self.exception_ui(ui, site, &detail);
                    ui.separator();
                }
                dirty |= self.titles_ui(ui, site, &detail);
                ui.separator();
                dirty |= self.preferred_ui(ui, site, &detail);
                ui.separator();
                dirty |= self.values_ui(ui, site, &detail);
            });
        if dirty {
            self.load_detail(&site.catalog);
        }
    }

    /// **例外**：把选中这个变体含进来，或者排除掉。
    ///
    /// **优先于规则、永久记住**（ADR-0016）：规则表达不了「这个我小时候玩过」
    /// 「这个太占地方先不带」这类个人口味。落在**变体**这一层——那正是例外与规则的
    /// 分工：规则说「要什么内容」，例外说「另外还要 / 偏不要这一份」。
    ///
    /// 它在**这一屏**而不在子库屏，因为「哪一份」只有在详情面板里才指得准：
    /// 子库屏上人手里只有一串键。
    fn exception_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, detail: &VariantDetail) {
        let Some(editing) = &self.editing else {
            return;
        };
        let name = editing.sublibrary.clone();
        let key = detail.row.key.clone();
        let current = editing.exceptions.get(&key).cloned();
        ui.strong(format!("例外 · 子库「{name}」"));
        match &current {
            None => {
                ui.weak("这个变体上还没有例外：进不进选择集，眼下由规则说了算。");
            }
            Some(row) => {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!(
                        "眼下：{}{}",
                        row.kind.label(),
                        row.note
                            .as_deref()
                            .map(|note| format!("（{note}）")
                            )
                            .unwrap_or_default(),
                    ),
                );
            }
        }
        if let Some(editing) = &mut self.editing {
            ui.add(
                egui::TextEdit::singleline(&mut editing.note)
                    .desired_width(240.0)
                    .hint_text("为什么（半年后你会想知道）"),
            );
        }
        ui.horizontal(|ui| {
            for kind in [Exception::Include, Exception::Exclude] {
                let on = current.as_ref().is_some_and(|row| row.kind == kind);
                if ui
                    .add_enabled(!on, egui::Button::new(format!("{}它", kind.label())))
                    .on_hover_text(match kind {
                        Exception::Include => "规则没选中也带上它。",
                        Exception::Exclude => "规则选中了也不带。容量超限时砍谁，落点就是这一条。",
                    })
                    .clicked()
                {
                    self.set_exception(site, &key, kind);
                }
            }
            if ui
                .add_enabled(current.is_some(), egui::Button::new("撤掉"))
                .on_hover_text("撤掉之后这个变体进不进选择集重新由规则说了算。")
                .clicked()
            {
                self.clear_exception(site, &key);
            }
        });
    }

    /// **标题集合**：全部叫法，加一条、删一条。
    fn titles_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, detail: &VariantDetail) -> bool {
        ui.strong("标题集合");
        let Some(work) = detail.work.clone() else {
            ui.weak("这个变体还没认出属于哪个作品，标题集合无从谈起。");
            return false;
        };
        if let Some(chosen) = &detail.display {
            ui.label(format!(
                "显示标题：{}（{}）｜排序标题：{}（来自{}）",
                chosen.display,
                chosen.language.label(),
                chosen.sort,
                chosen.sort_from.label(),
            ));
        }
        let mut dirty = false;
        let mut remove: Option<(Language, TitleKind, String, String)> = None;
        for row in detail.titles.iter().take(TOP_TITLES) {
            ui.horizontal(|ui| {
                if ui
                    .small_button("删")
                    .on_hover_text("从标题集合里去掉这一条叫法")
                    .clicked()
                {
                    remove = Some((
                        row.language,
                        row.kind,
                        row.source.clone(),
                        row.value.clone(),
                    ));
                }
                let line = format!(
                    "{}｜{} {}｜{}｜{} 个变体这么叫",
                    row.value,
                    row.language.label(),
                    row.kind.label(),
                    row.source,
                    row.seen,
                );
                if row.is_verdict() {
                    ui.strong(line).on_hover_text(&row.evidence);
                } else {
                    ui.label(line).on_hover_text(&row.evidence);
                }
            });
        }
        if detail.titles.len() > TOP_TITLES {
            ui.weak(format!(
                "……另有 {} 条没列",
                detail.titles.len() - TOP_TITLES
            ));
        }
        if detail.titles.is_empty() {
            ui.weak("一条叫法都没有——显示标题会退回作品名。");
        }
        if let Some((language, kind, source, value)) = remove {
            self.remove_title(site, &work, language, kind, &source, &value);
            dirty = true;
        }

        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.title_draft.value)
                    .desired_width(220.0)
                    .hint_text("加一条叫法"),
            );
            egui::ComboBox::from_id_salt("叫法语言")
                .selected_text(self.title_draft.language.label())
                .show_ui(ui, |ui| {
                    for language in Language::all() {
                        ui.selectable_value(
                            &mut self.title_draft.language,
                            language,
                            language.label(),
                        );
                    }
                });
            egui::ComboBox::from_id_salt("叫法类型")
                .selected_text(self.title_draft.kind.label())
                .show_ui(ui, |ui| {
                    for kind in TitleKind::all() {
                        ui.selectable_value(&mut self.title_draft.kind, kind, kind.label());
                    }
                });
            let ready = !self.title_draft.value.trim().is_empty();
            if ui
                .add_enabled(ready, egui::Button::new("加进集合"))
                .on_hover_text("来源记作「裁决」：人改过的东西不许被任何数据源覆盖")
                .clicked()
            {
                dirty |= self.write_title(site, &work, detail);
            }
        });
        dirty
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
                self.notice = Some(format!("「{value}」进了标题集合，来源是裁决。"));
                self.title_draft.value.clear();
                true
            }
            Err(error) => {
                self.error = Some(format!("中立库写不动：{error}"));
                false
            }
        }
    }

    /// **刮削来的字段值**：看得见，也改得动。
    ///
    /// 「所有元数据编辑收敛在这里完成」（ADR-0001 的修订段）说的不只是标题与首选变体
    /// ——年份、发行商、开发商、类型、简介、汉化组这几样也会写进导出条目
    /// （`adapter::converge`），主库的元数据文件既然不再是编辑入口，它们就得在这儿改。
    fn values_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, detail: &VariantDetail) -> bool {
        ui.strong("刮削来的元数据");
        ui.weak("同一个字段可以有好几条，三元组并存不互相覆盖；**裁决**排在最前，导出用它。");
        let mut dirty = false;
        let mut clear: Option<(AnchorKind, Field)> = None;
        for item in &detail.values {
            ui.horizontal(|ui| {
                if item.is_verdict() {
                    if ui
                        .small_button("撤")
                        .on_hover_text("撤掉这条人工写的，让别的源重新说了算")
                        .clicked()
                        && let Some(field) = Field::all()
                            .into_iter()
                            .find(|field| field.label() == item.value.field)
                    {
                        clear = Some((item.anchor, field));
                    }
                } else {
                    ui.add_space(24.0);
                }
                // **一行画得下的那一截**：简介能有 4,000 字，横排里不折行（见 `one_line`）。
                let short = one_line(&item.value.value);
                let line = format!(
                    "{} · {}｜{}｜{}",
                    item.value.field,
                    item.anchor.label(),
                    item.value.source,
                    short.as_deref().unwrap_or(&item.value.value),
                );
                let response = if item.is_verdict() {
                    ui.strong(line)
                } else {
                    ui.label(line)
                };
                if short.is_some() {
                    // 收窄过的那些，整段挂在悬停里——**面板上画不下不等于看不到**。
                    // 用 `on_hover_ui` 而不是拼一个大字符串：那个闭包只在真悬停时才跑。
                    response.on_hover_ui(|ui| {
                        // **限宽**。不限的话悬停框跟着最长那一行铺开——简介闸在 4,000 字
                        // （票 `offline-chinese-fields/03`），一段没有换行的中文会把这个
                        // 框拉成一条横穿屏幕的线，反倒比截断更看不清。
                        ui.set_max_width(420.0);
                        ui.label(&item.value.value);
                        ui.separator();
                        ui.label(&item.value.evidence);
                    });
                } else {
                    response.on_hover_text(&item.value.evidence);
                }
            });
        }
        if detail.values.is_empty() {
            ui.weak("一条刮削结论都没有。跑一次 `romcat scrape`，或者在这儿手写。");
        }
        // 作品未知时只挂得到变体那一层——**作品锚点是作品名**，没有名字就没有锚点。
        let anchors: Vec<AnchorKind> = if detail.work.is_some() {
            vec![AnchorKind::Work, AnchorKind::Variant]
        } else {
            vec![AnchorKind::Variant]
        };
        if !anchors.contains(&self.value_draft.anchor) {
            self.value_draft.anchor = AnchorKind::Variant;
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("字段")
                .selected_text(self.value_draft.field.label())
                .show_ui(ui, |ui| {
                    for field in Field::all() {
                        ui.selectable_value(&mut self.value_draft.field, field, field.label());
                    }
                });
            egui::ComboBox::from_id_salt("挂在哪一层")
                .selected_text(self.value_draft.anchor.label())
                .show_ui(ui, |ui| {
                    for anchor in &anchors {
                        ui.selectable_value(&mut self.value_draft.anchor, *anchor, anchor.label());
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut self.value_draft.value)
                    .desired_width(220.0)
                    .hint_text("写下这个字段的值"),
            );
            let subject = match self.value_draft.anchor {
                AnchorKind::Work => detail.work.clone(),
                AnchorKind::Variant => Some(detail.row.key.clone()),
            };
            let ready = !self.value_draft.value.trim().is_empty() && subject.is_some();
            if ui
                .add_enabled(ready, egui::Button::new("写下"))
                .on_hover_text("来源记作「裁决」：它排在每个字段的最前，导出真会用它")
                .clicked()
                && let Some(subject) = subject
            {
                self.put_value(site, &subject);
                dirty = true;
            }
        });
        if let Some((anchor, field)) = clear {
            let subject = match anchor {
                AnchorKind::Work => detail.work.clone(),
                AnchorKind::Variant => Some(detail.row.key.clone()),
            };
            if let Some(subject) = subject {
                self.clear_value(site, anchor, &subject, field);
                dirty = true;
            }
        }
        dirty
    }

    /// **首选变体**：这个作品在这个平台上默认启动哪一个。
    fn preferred_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, detail: &VariantDetail) -> bool {
        ui.strong("首选变体");
        let (Some(work), Some(platform)) = (detail.work.clone(), detail.row.platform.clone())
        else {
            ui.weak("作品或平台还没定下来，首选变体无从谈起。");
            return false;
        };
        // ADR-0012 那条**必须写在人眼前**：改首选不会改中文标题的来源。
        match detail.chinese_title() {
            Some(row) => ui.label(format!(
                "中文标题取的是「{}」（{}｜{}），**与首选变体无关**——\
                 首选启动汉化版，中文名照旧取官中版的官方译名。",
                row.value,
                row.kind.label(),
                row.source,
            )),
            None => ui.label(
                "这个作品还没有中文叫法。首选变体改成汉化版也不会凭空生出一个中文名——\
                 那两件事是分开的（ADR-0012）。",
            ),
        };
        let mut dirty = false;
        let mut clear = false;
        let mut set: Option<String> = None;
        for sibling in &detail.siblings {
            let first = detail.preferred_now() == Some(sibling.row.key.as_str());
            let label = format!(
                "{}{}  {}｜{}",
                if first { "▶ " } else { "   " },
                sibling.preference.label(),
                sibling.row.key,
                human_bytes(sibling.row.bytes),
            );
            if ui
                .selectable_label(first, label)
                .on_hover_text("点它就把首选变体裁给这一个")
                .clicked()
                && !first
            {
                set = Some(sibling.row.key.clone());
            }
        }
        if detail.siblings.len() <= 1 {
            ui.weak("这个平台上这部作品只有这一个变体，没得选。");
        }
        ui.horizontal(|ui| {
            match &detail.preferred {
                Some(key) => ui.label(format!("眼下是**裁决**指定的：{key}")),
                None => ui.label("眼下没人裁过，按规则算：汉化 > 官中 > 日版 > 其他"),
            };
            if ui
                .add_enabled(detail.preferred.is_some(), egui::Button::new("撤掉裁决"))
                .clicked()
            {
                clear = true;
            }
        });
        if clear {
            self.clear_preferred(site, &work, &platform);
            dirty = true;
        }
        if let Some(key) = set {
            self.set_preferred(site, &work, &platform, &key);
            dirty = true;
        }
        dirty
    }

    /// 字体样张：把 egui 内置字体缺的那几类字**摆出来给人看**。
    fn font_sample(&self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.strong("字体样张");
            ui.label(format!("子集 {}", human_bytes(font::subset_bytes() as u64)));
        });
        for (what, text) in font::SAMPLE.iter().copied() {
            ui.horizontal(|ui| {
                ui.add_sized([90.0, 18.0], egui::Label::new(what));
                ui.label(text);
            });
        }
        // OFL 要求分发字体时随附许可，而这份字体是嵌在可执行文件里的——许可得跟着走。
        ui.collapsing("字体许可（SIL Open Font License 1.1）", |ui| {
            egui::ScrollArea::vertical()
                .max_height(160.0)
                .show(ui, |ui| ui.monospace(font::LICENSE));
        });
    }
}

/// 作用范围那个数画成什么。**数不出来就说数不出来**，不摆一个 0 出去。
fn scope_label(scope: Option<u64>) -> String {
    scope.map_or_else(|| "？".to_string(), thousands)
}

/// 一个维度的选项列表：一行一个值，左边是它选中多少个变体。
///
/// 空列表也画出标题与一句话，而不是整块消失——「这个维度一条数据都没有」与
/// 「这个维度不在界面上」是两件事，后者会让人以为工具不支持按它筛。
fn facet_picker(
    ui: &mut egui::Ui,
    what: &str,
    hint: &str,
    facets: &[romcat_core::catalog::Facet],
    picked: &mut Option<String>,
) {
    ui.strong(what).on_hover_text(hint);
    if facets.is_empty() {
        ui.weak(format!("库里还没有{what}这一维的数据。"));
        return;
    }
    if ui.selectable_label(picked.is_none(), "不筛").clicked() {
        *picked = None;
    }
    for facet in facets {
        let on = picked.as_deref() == Some(facet.value.as_str());
        if ui
            .selectable_label(on, format!("{}  {}", thousands(facet.count), facet.value))
            .clicked()
        {
            *picked = (!on).then(|| facet.value.clone());
        }
    }
}

/// 平台那一维。它比别的多一档——**平台未知**在表里是 `NULL`，只能是独立的一支
/// （[`PlatformFilter`]）。
fn platform_picker(
    ui: &mut egui::Ui,
    facets: &[romcat_core::catalog::Facet],
    picked: &mut Option<PlatformFilter>,
) {
    ui.strong("平台")
        .on_hover_text("变体所属的硬件系统。认不出平台的内容照常入库（ADR-0011）。");
    if facets.is_empty() {
        ui.weak("库里还没有平台这一维的数据。");
        return;
    }
    if ui.selectable_label(picked.is_none(), "不筛").clicked() {
        *picked = None;
    }
    for facet in facets {
        let filter = PlatformFilter::from_label(&facet.value);
        let on = picked.as_ref() == Some(&filter);
        if ui
            .selectable_label(on, format!("{}  {}", thousands(facet.count), facet.value))
            .clicked()
        {
            *picked = (!on).then_some(filter);
        }
    }
}
