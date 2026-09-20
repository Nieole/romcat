//! 翻页浏览：**排序、筛选、分页一律下推到中立库**。
//!
//! 界面上那张变体表在真机上是四万多行，做完识别后还要连上候选与标题——十万行是设计
//! 时就该按住的量级（ADR-0005 的修订段）。这一层存在的理由只有一条：
//! **内存里永远只有当前视口那几十行**。
//!
//! ## 为什么排序不能在内存里做
//!
//! `egui_extras` 完全没有排序能力（源码里 `sort` 零出现），于是排序必须由别人做。
//! 「别人」有两个候选：把全部变体读成一个 `Vec` 再 `sort_by`，或者写成 `ORDER BY`。
//! 选后者，因为前者与「绝不把全库载入内存」这条既定原则直接冲突——四万行的 `VariantRow`
//! 每行两个 `String`，读一次就是几十兆常驻，而这还只是今天的库。
//!
//! 分页同理：`LIMIT` 加 `OFFSET` 让「翻到第九万行」的代价与视口大小成正比，而不是与
//! 库的大小成正比。
//!
//! ## 排序键永远带上变体键
//!
//! 按容量排时有大量并列（同一个平台下一堆 0 字节的目录树），并列行的相对次序若不定死，
//! 翻页就会漏行与重行——第 100 页的最后一行可能在第 101 页再出现一次。所以每条
//! `ORDER BY` 后面都缀一个 `key`，而 `key` 是主键，全序由它兜住。
//!
//! **升降序对两段一起翻**（`bytes DESC, key DESC`），不是只翻第一段。这样
//! `(bytes, key)` 那条索引倒着扫就能直接出结果，不必落到临时表排序。
//!
//! ## 变体表那一列**作品名**也下推（挂账 D161，票 `parking-3/11`）
//!
//! 翻库时人认的是**作品**，不是那些各路来源攒出来的文件名——所以变体表上有作品名
//! 这一列。它一度是「取回来之后拿 [`Catalog::work_names`] 在内存里对」的：那张表真库
//! 9,226 行，整份读进来只为在页上补几十格，而且补出来的东西**进不了 `ORDER BY` /
//! `WHERE`**，于是想按作品找就只能退回「键里含」那个框。
//!
//! 现在它是 [`Catalog::variant_browse_page`] 那一趟 `LEFT JOIN work` 带回来的：
//! 排序落在 [`VariantOrder::Work`] 上、筛选落在 `作品^…` 那条子句上
//! （`Dimension::Work` → `catalog::filter`），两样都在库里。
//!
//! **浏览要的那一行另立了一个类型**（[`BrowseVariant`]），没往 [`VariantRow`] 上加列：
//! 那是「`variant` 表的一行」，识别、成型、待确认队列、导出收敛十几处都按这个意思
//! 读它，而作品名不在那张表上。
//!
//! ⚠️ **屏上眼下没有变体级的表**：票 `gui-redesign/03` 把它换成了作品级主列表
//! （那一屏的作品名走 [`WorkOrder::Name`]，早就下推着排、下推着筛）。所以上面那句
//! 「变体表上有作品名这一列」说的是**这个查询面**，不是当下某一屏。这一层现在服务的是
//! 字体那趟自检（作品名也要过一遍豆腐块）与测试；留着它是因为「按作品名翻变体」这件事
//! 本身还在，只是没人在屏上问它（挂单 `Q297`）。
//!
//! ## 四个筛选维度也一律下推
//!
//! 票 25 的库浏览要按**平台**、**合集**、**语言**、**识别状态**筛。四条都写成 `WHERE`
//! 而不是取回来再过一遍，理由与排序同一条：过一遍要求先把全库读进内存。合集与语言
//! 各走一条 `EXISTS` 子查询——它们是一对多，`JOIN` 会让同一个变体出现好几行，
//! 而那会让总行数与滚动条一起说谎。
//!
//! **筛选面板上那几个选项从哪来**：[`Catalog::facets`] 一次问出四个维度各有哪些值、
//! 各多少个变体。它是 `GROUP BY`，不是把全库读回来数——真库四万多个变体上，
//! 平台十几个、合集几十个，一次几毫秒。
//!
//! ## 那棵**条件组**也一律下推
//!
//! 票 `gui-redesign/04`：筛选器就是**规则**语言，可嵌套的组加三种连接。它同样折成
//! `WHERE`（`catalog::filter`）而不是取回来再过一遍，理由与上面那四条同一条。
//!
//! **它与那四个档之间是且**：那四个是一按就有的快捷档（带条数，供探索），
//! 这一条是手搭的表达式（供表达）。两边收窄的是同一批变体，
//! 而「存成子库」时两边一起折进同一条规则（[`WorkQuery::to_rule`]）。
//!
//! ## 作品级的主列表另是一个查询面
//!
//! [`WorkQuery`] 那一族按**作品**出行（票 `gui-redesign/03`）：一个游戏一行，
//! 变体在详情面板里挑。它与变体表**共用同一份筛选**（`VariantQuery::where_clause`），
//! 因为规格里那条贯穿全局的约定是「主列表的筛选就是子库的规则」，而子库选的是变体。
//!
//! **它比变体表贵，而且贵得有理由**：变体表按**自己那张表上的列**排时都有一条索引正好
//! 接住（`variant_bytes_key` 那几条），一次翻页是索引倒着扫（按**作品名**排是例外，
//! 见 [`VariantOrder::Work`]）；作品级那一条要先 `GROUP BY`
//! 把全表折成行，再按聚合出来的列排序。这不是可以绕开的实现细节：「这个作品有几个变体」
//! 这件事本身就要看过它的每一个变体。内存那一半照旧只有视口那几十行（[`MAX_PAGE`] 还在），
//! 涨的是每次翻页的时间。
//!
//! **票 `gui-redesign/13` 把这份代价量开、削掉了三分之二**（数字与量法见
//! `docs/library-facts.md`，量的命令是 `--bench-paging`）。三样各治一处：
//!
//! - **`GROUP BY` 那口临时 b 树**：分组键的第二项是个表达式（没认出作品时那个变体自己的
//!   键），索引里没有，于是四万多行要整个塞进一口临时 b 树才分得出组。把那个表达式
//!   原样写进索引（`variant_group`，见 `catalog::content`）之后那口 b 树就没了。
//! - **`ORDER BY` 那口临时 b 树**：它要把每一组连同 `SELECT` 里那一堆聚合一起搬进去排。
//!   于是**分两趟**：第一趟只挑「这一页是哪几行」（`Catalog::work_page_anchors`，
//!   按哪一列排就多算哪一列），第二趟只给这几百行算聚合
//!   （`Catalog::work_page_totals`）。搬得轻，排得快。
//! - **年份那张 `LEFT JOIN`**：它是一趟 `scrape_value` 全表扫加一次分组，而屏上那个年份
//!   与「元数据齐不齐」读的是同一张表的同一批行——并进 `Catalog::fill_scraped` 那一趟。
//!   **只有按年份排时才连它**：排序需要它，画不需要它。
//!
//! ## 搜索框：**它管排序，筛选器管集合**
//!
//! 票 `gui-redesign/05`。[`WorkQuery::search`] 里打的字折出一个**匹配质量的名次**
//! （[`SearchHit`]），而那个名次是排序的**第一把键**——「匹配得好的排前面」是这个框
//! 唯一的产品承诺（User Story 27）。权重写死在 `Search::rank` 里，**不暴露给用户配**：
//! 那是很少用得上却一直占着界面的东西。
//!
//! 它与筛选器**是两件事**，而这条界线在两处看得见：
//!
//! - **排序不参与子库的规则。** [`WorkQuery::to_rule`] 撞上搜索框当场挡住
//!   （[`Unruly::Search`]，挂单 Q70）——子库要的是集合不是顺序，把一个顺序存进规则，
//!   下次同步搬过去的那批不会因此变，但人会以为它变了。
//! - **两边叠加时各干各的**：筛选器收窄集合，搜索在那批里面再收一次并排序。
//!   所以「先筛后搜」的结果既满足条件，又按匹配质量排。
//!
//! 收窄这件事搜索框也做（不然「打几个字就找到那个游戏」无从谈起），但它收窄的依据
//! 是**三条命中路**（屏上那个名字、标题集合里别的叫法、简介），而这三条一条都写不成
//! 规则语言里的子句——那正是 Q70 挡住它的理由。
//!
//! **三条里有两条写成集合成员判定而不是相关子查询**（`Search::alias`、
//! `Search::description`，票 `gui-redesign/13`）：`EXISTS (… WHERE t.work = work.name …)`
//! 与外层绑死，SQLite 只能逐个变体行去探一次——真库形状上那是四万多次。写成
//! `IN (SELECT …)` 之后子查询与外层无关，一次算完存进一张临时索引，
//! 筛出来的**是同一批行**。数字见 `docs/library-facts.md` 与挂单 Q108。
//!
//! ## 非游戏资产默认收起
//!
//! 票 `gui-looks-like-the-design/08`：BIOS 这类**非游戏资产**（ADR-0010）浏览时默认不列出，
//! 一个开关（[`NonGameAssets`]）列出来，屏上说得出收起了几行
//! （[`Catalog::non_game_asset_rows`]）。**只改列不列出**：照旧入库、永不导出——
//! 导出那道闸（`adapter::converge`）不读这个开关。
//!
//! **判断只有一处**（`classify::non_game_asset`，ADR-0024），而收起这件事得落在 `WHERE`
//! 里：取回来再在 Rust 里筛，总数与页内容当场分家，滚动条指向不存在的行。于是那一处判断
//! **挂成 SQL 函数**（`register_non_game_asset`，交得出能浏览的中立库的两个入口各挂一次），`WHERE` 里问的
//! 是它，SQL 里一个字的判据都不写。
//!
//! 没走「识别或成型那一趟判一次、物化成一列」那条路（ADR-0024 推论 3 允许那样）：这条判断
//! 只看键，现问不贵；物化要改变体那张表、给老库补行，判据一改还要等下一趟扫描才跟上
//! （挂单 `Q771`）。
//!
//! **收起了几个按行数算**：屏上收起的是**行**，「收起时的行数 + 收起了几行 = 列出时的
//! 行数」这笔账才对得上。识别挑作品时跳过非游戏资产（`identify` 的 `hit_non_game_asset`），
//! 所以它们实际上各自成一行；只有被人手工挂进某部作品的那种，收起的是那一行底下的一个
//! 变体（挂单 `Q772`）。

use rusqlite::{ToSql, params_from_iter};

use super::content::{VARIANT_COLUMNS, VariantRow, read_variant_row};
use super::identify::{Candidate, Confidence, NOT_RUN_LABEL, State, Tier};
use super::{Catalog, CatalogError};
use crate::scrape::{AnchorKind, Field};
use crate::sublibrary::{Clause, Dimension, Group, Join, Node, Op, Rule};

/// 变体表按哪一列排。
///
/// 是个闭集合而不是一个字符串：拼进 `ORDER BY` 的列名只能来自这里，界面上点哪个表头
/// 都拼不出别的 SQL。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VariantOrder {
    /// 变体的键，也就是主库里的相对路径。默认按它排——那是人在文件管理器里看惯的次序。
    #[default]
    Key,
    /// 平台。
    Platform,
    /// 哪条**成型规则**成的型。
    Rule,
    /// 文件成员数。
    Files,
    /// 容量（字节合计，是个下界）。
    Bytes,
    /// **作品**名（票 `parking-3/11`，挂账 D161）。
    ///
    /// 翻库时人认的是作品，不是各路来源攒出来的文件名——所以这一列得排得了序。
    /// 它排的是 `work_name` 那个别名，也就是 `work.name`：**下推到中立库**
    /// （ADR-0005），不是把那张作品表整份读进内存再在页上补。
    ///
    /// **还没认出作品的那些排在末尾，正反两个方向都是**——`ORDER BY` 那一段
    /// 多一把 `work_name IS NULL` 的键，理由见那个函数。
    ///
    /// **这一档背后没有索引**，与同一个枚举里另外几档不一样：`work_name` 是 join 出来
    /// 的列，`variant` 上那几条 `(列, key)` 索引接不住它，`work(name)` 那条也接不住
    /// （排的是变体，不是作品）。于是它落到一口临时 b 树上——量级与作品级主列表那一条
    /// 同一档，而不是别的变体列那种索引倒着扫。**这不是可以绕开的实现细节**：
    /// 按另一张表上的列排，就是要把两张表连起来之后整个排一遍。记在挂单 `Q300`。
    Work,
}

impl VariantOrder {
    /// 全部可排的列的**规范次序**。
    ///
    /// **不是「界面照这个次序摆表头」**：屏上那张表是作品级的，摆的是 [`WorkOrder::ALL`]
    /// （`crates/gui/src/table.rs`），这个枚举在 `crates/gui/src/` 里零出现。它是给
    /// 「把每一档都过一遍」的调用方用的——眼下就是测试。
    pub const ALL: [Self; 6] = [
        Self::Key,
        Self::Work,
        Self::Platform,
        Self::Rule,
        Self::Files,
        Self::Bytes,
    ];

    /// 这一列在界面上叫什么。用**词表**里的词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Key => "变体",
            Self::Work => "作品",
            Self::Platform => "平台",
            Self::Rule => "成型规则",
            Self::Files => "文件数",
            Self::Bytes => "容量",
        }
    }

    /// 拼进 `ORDER BY` 的列名。**只有这个函数认得列名**。
    fn column(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Work => WORK_NAME,
            Self::Platform => "platform",
            Self::Rule => "rule",
            Self::Files => "files",
            Self::Bytes => "bytes",
        }
    }
}

/// **识别状态**那一维怎么筛。
///
/// 它比 [`State`] 多一档：**还没识别**。中立库里一个变体可以压根没有 `identification`
/// 那一行——那不是「未命中」，是识别还没跑到它头上。混成一档的话，界面上「未命中有
/// 多少」这个数会把没跑过的一起算进去，而那正是命中率最容易被说错的地方（ADR-0002）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateFilter {
    /// 有结论，而且是这一档。
    Concluded(State),
    /// **还没识别**：这个变体在 `identification` 里没有行。
    Unidentified,
}

impl StateFilter {
    /// 界面上照这个次序摆。
    pub const ALL: [Self; 5] = [
        Self::Concluded(State::Matched),
        Self::Concluded(State::Unmatched),
        Self::Concluded(State::NoEvidence),
        Self::Concluded(State::Skipped),
        Self::Unidentified,
    ];

    /// 打给用户的那个词。用**词表**里的词（`CONTEXT.md` 的**还没识别**条，
    /// 落在 [`NOT_RUN_LABEL`] 上）。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Concluded(state) => state.label(),
            Self::Unidentified => NOT_RUN_LABEL,
        }
    }

    /// 从词认回来；认不出是 `None`。
    ///
    /// **认的是 [`NOT_RUN_LABEL`] 那个常量，不是抄一遍那四个字**（票 `gui-redesign/17`）：
    /// 它与 [`label`](Self::label) 是一对，抄一遍的话改一处、断一处，而断了**没有
    /// 一条编译错误会说话**——`&str` 比 `&str` 永远编得过。
    ///
    /// ⚠️ **眼下这个函数没有生产调用方**：筛选面板拿的是 `StateFilter` 值本身
    /// （`romcat_gui::browse` 那一段直接比、直接赋回），一个字符串都不经手；
    /// 真走字符串往返的是 [`PlatformFilter::from_label`]。所以这一对的一致性
    /// **只有测试守着**（`crates/core/tests/browse.rs`），而不是「屏上点了没反应」
    /// 会自己冒出来——那正是它更该由常量而不是字面量钉住的理由。
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        if label == NOT_RUN_LABEL {
            return Some(Self::Unidentified);
        }
        State::from_label(label).map(Self::Concluded)
    }
}

/// **平台**那一维怎么筛。
///
/// 它比一个 `Option<String>` 多一档：**平台未知**。`variant.platform` 可空，而
/// 「认不出平台」在真库里是一档真实存在的内容，不是缺陷（ADR-0011）——`= NULL`
/// 永远不成立，所以它只能是独立的一支。
///
/// **不拿一个约定字符串当哨兵。** 借 [`UNKNOWN_PLATFORM_LABEL`] 那串字去表示 `NULL`
/// 的话，一个真的叫这个名字的平台就会把两件事撞在一起；而这个库里「两件事必须分得开」
/// 是一条反复出现的纪律（ADR-0021 的不可读、[`StateFilter`] 的还没识别）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformFilter {
    /// 就这个平台。
    Named(String),
    /// **平台未知**：这一列在库里是 `NULL`。
    Unknown,
}

impl PlatformFilter {
    /// 打给用户的那个词。平台未知那一档借报告里已经在用的那一串，两处印的字一样。
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Named(platform) => platform,
            Self::Unknown => UNKNOWN_PLATFORM_LABEL,
        }
    }

    /// 从筛选面板上那一行的字认回来。
    #[must_use]
    pub fn from_label(label: &str) -> Self {
        if label == UNKNOWN_PLATFORM_LABEL {
            Self::Unknown
        } else {
            Self::Named(label.to_string())
        }
    }
}

/// **平台未知**那一档在界面上印成什么。
pub use crate::report::UNKNOWN_PLATFORM_LABEL;

/// **非游戏资产**（ADR-0010）在浏览里列不列出来。
///
/// **只管列不列出**：它们照旧入库、永不导出——导出那道闸（`adapter::converge`）
/// 不读这一档。见模块文档「非游戏资产默认收起」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NonGameAssets {
    /// 收起：不列出来。**主列表（[`WorkQuery`]）默认这一档**——BIOS 这类东西人在找游戏时
    /// 不想看见。变体那一层（[`VariantQuery`]）默认全列，理由写在它的 `Default` 上。
    #[default]
    Hidden,
    /// 列出来，行上标着（[`WorkRow::non_game_asset`]）。
    Listed,
}

/// 屏上标在**非游戏资产**那一行、那个变体上的词。
///
/// **词落在核心库里**，同 [`NOT_RUN_LABEL`]：表上那一行、详情面板那一行、测试数标记，
/// 读的都是这一个。
pub const NON_GAME_ASSET_LABEL: &str = "非游戏资产";

/// 一次翻页要的是哪一段：筛什么、按什么排。
///
/// 它是**值**而不是游标：界面把它整个换掉就等于换了一张表，窗口据此作废重取。
///
/// 四个可选维度之间是**且**：选了平台又选了合集，两条都得满足。这是浏览而不是搜索
/// ——「一层层收窄」是人在文件管理器里的动作，而并集会让每多选一个条件行数反而变多。
///
/// 要并集就写进[那棵条件组](Self::rule)里，**亲手选出「任一满足」那一档**——
/// 行数变多之前人先看见了那四个字（挂账 D154 的裁决）。
#[derive(Debug, Clone, PartialEq)]
pub struct VariantQuery {
    /// 变体的键里含这个子串才算数；空串等于不筛。
    ///
    /// 大小写按 SQLite 的 `LIKE` 语义——**只对 ASCII 不敏感**，汉字与假名是逐字节比的。
    /// 中文本来就没有大小写，这个限制在这里不咬人。
    pub contains: String,
    /// 只要这个**平台**的；[`PlatformFilter::Unknown`] 选的是平台未知那一档。
    pub platform: Option<PlatformFilter>,
    /// 只要在这个**合集**里的。
    pub collection: Option<String>,
    /// 只要发行版标着这个语言的。**逐个语言码比**，不是子串——`zh` 不该匹配上 `zh-Hant`
    /// 之外的东西，而 `en` 更不该匹配上 `Danish`。
    pub language: Option<String>,
    /// 只要带这个**中文身份**记号的：`汉化` / `官中`（ADR-0012）。
    ///
    /// **它与[语言](Self::language)不是一回事，缺一不可。** 语言是**发行版**标着的
    /// 语言码（`Zh-Hans`），而**汉化版是变体**——它多半基于一条日版发行版，语言那一列
    /// 上一个中文字都没有。这个库里最要紧的那批内容恰恰全在这一档里，只按语言筛的话
    /// 它们一条都不出现。
    pub chinese: Option<String>,
    /// 只要**识别状态**是这一档的。
    pub state: Option<StateFilter>,
    /// **筛选器那棵条件树**：可嵌套的组，三种连接（票 `gui-redesign/04`）。
    ///
    /// 它与上面那几个维度之间是**且**——上面那几个是一按就有的快捷档，这一条是
    /// 手搭的表达式，两边收窄的是同一批变体。`None` 是「没搭任何条件」。
    ///
    /// **它就是子库的规则**：同一套语言、同一个求值口径（`catalog::filter`）。
    pub rule: Option<Rule>,
    /// **非游戏资产**列不列出来。**变体这一层默认全列**（见 `Default` 那一段），
    /// 主列表那一层（[`WorkQuery`]）默认收起。
    ///
    /// 它与上面那几维一样是**且**进 `WHERE` 的一条，于是数总数、取一页、详情面板、
    /// 批量操作的作用范围收起的是同一批。
    pub non_game_assets: NonGameAssets,
    /// 按哪一列排。
    pub order: VariantOrder,
    /// 倒着排。
    pub descending: bool,
}

/// 一页最多取多少行。
///
/// 视口撑死几十行，取这么多已经给预取留了十几倍余量。**存在的意义是拦住
/// `limit = u64::MAX` 这种把全库拉进内存的写法**——那正是这一层要防的事。
pub const MAX_PAGE: u64 = 4_096;

/// 变体表那一趟多取的那一列——**作品名**——在 SQL 里叫什么。
///
/// **只有这一处起这个名字**：[`VariantOrder::column`] 拼进 `ORDER BY` 的就是它，
/// [`Catalog::variant_browse_page`] 那条 `SELECT` 里起的别名也是它，那一行读回来
/// 按名字取的还是它。三处各写一遍的话，改一处就静默地按一列不存在的东西排。
const WORK_NAME: &str = "work_name";

/// 变体表翻页时的 `FROM`：变体连**它的作品**。
///
/// `work.id` 是主键，这条 `LEFT JOIN` 每行一次索引查——它换来的是**作品名下推**
/// （ADR-0005）。不这么做只剩两条路，两条都是这一层从头到尾在躲的事：
/// 把那张作品表整份读进内存在页上补（真库 9,226 行），或者逐行去问一次。
///
/// **`LEFT` 那半个字不能省**：还没认出作品的变体（`adapter::converge` 的
/// `Anchor::Loose`）会被内连接**整批**筛掉，一个都不剩——那不是排序，那是换了一批行。
/// 真库上这一批有一千七百多个（`docs/library-facts.md`：作品 9,226 个，
/// 而主列表 10,978 行，差出来的那些一行一个变体）。
///
/// **它同时是「行数一个不多一个不少」的依据**，而那正是
/// [`Catalog::variant_total`]（不连这张表，只 `COUNT(*) FROM variant`）与这一趟敢共用
/// 同一份 `WHERE` 的前提：`work.id` 是 `INTEGER PRIMARY KEY`，一个变体最多连上一行，
/// join 放大不了行数。**日后要往这条 `FROM` 上再连一张表，先回答这一句还成不成立**
/// ——连的若是一对多（比如发行版的语言），总数与页内容当场分家，而屏上的样子是
/// 滚动条指向不存在的行。
const VARIANT_BROWSE_FROM: &str = " FROM variant LEFT JOIN work ON work.id = variant.work_id";

/// **非游戏资产**那一处判断在 SQL 里叫什么。
///
/// **只有这一处写这个名字**：挂到连接上（`register_non_game_asset`）与每一条查询问它，
/// 都从这里取。名字带着程序名，免得哪天撞上 SQLite 自己或别的扩展的函数。
pub(super) const NON_GAME_ASSET_FN: &str = "romcat_non_game_asset";

/// 把**非游戏资产**那一处判断（[`crate::classify::non_game_asset`]）挂到这条连接上，
/// SQL 里问 `romcat_non_game_asset(键)`，答 `1` / `0`。
///
/// **这不是第二份判断**（ADR-0024）：挂上去的就是那个函数本身，SQL 里一个字的判据都
/// 不写。挂成函数而不是物化成一列，理由见模块文档「非游戏资产默认收起」。
///
/// **每条要浏览的连接都得挂**，SQLite 的函数不落在库文件里。交得出一份能浏览的
/// [`Catalog`] 的入口是两个——`Catalog::prepare`（建库、打开、内存库）与
/// `Catalog::read_only_at`（分出只读的一份、只读打开）——两处各调一次；日后添一个忘了调，
/// 浏览那几条查询当场报「没有这个函数」，`crates/core/tests/works.rs` 里几种开法各跑一遍的
/// 那条测试钉着。
///
/// `Catalog::stranded_shaping_overrides` 另开了一条只读连接：它只读旧库里那张旧表、
/// 从不浏览、造出来的那一份也不交出去，所以不挂。
pub(super) fn register_non_game_asset(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    use rusqlite::functions::FunctionFlags;

    conn.create_scalar_function(
        NON_GAME_ASSET_FN,
        1,
        // **同一个键永远同一个答案**：让 SQLite 在一条语句里放心复用它。
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            // 借着读，不拷一份：这一条要在翻页时逐行问一遍。
            let key = ctx
                .get_raw(0)
                .as_str()
                .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))?;
            Ok(crate::classify::non_game_asset(key))
        },
    )
}

/// 把 `LIKE` 的三个元字符转义掉。
///
/// 不转义的话，用户在筛选框里打一个 `%` 就等于「什么都匹配」，打 `_` 会悄悄多匹配一个
/// 字符——而主库里真有带 `%` 的文件名。
pub(super) fn escape_like(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    for ch in text.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// **变体这一层默认全列**，与主列表（[`WorkQuery`]，默认收起）不同。
///
/// 拿 `VariantQuery::default()` 的调用方问的都是**库里一共有什么**：开场那一行的变体数
/// （`workspace`）、刮削整库估算（`scrape::estimate`）、字体自检、测速、左栏「还没识别」
/// 那一档拿总数去减。默认收起的话，这几个数会悄悄少掉非游戏资产那几个，没有一处说话
/// （挂单 `Q776`）。要收起就亲手写 [`NonGameAssets::Hidden`]——主列表把它的开关传进
/// 变体那一层筛选时就是这么写的。
impl Default for VariantQuery {
    fn default() -> Self {
        Self {
            contains: String::new(),
            platform: None,
            collection: None,
            language: None,
            chinese: None,
            state: None,
            rule: None,
            non_game_assets: NonGameAssets::Listed,
            order: VariantOrder::default(),
            descending: false,
        }
    }
}

impl VariantQuery {
    /// 折出 `WHERE` 那一段与它的参数。两处（数总数、取一页）共用，免得筛选条件漂开——
    /// 总数与页内容用了不同的筛选，滚动条就会指向不存在的行。
    ///
    /// 每一条都是**参数化**的：拼进 SQL 的只有这个函数里写死的那些字，用户在筛选框里
    /// 打什么都只会落进 `?`。
    fn where_clause(&self) -> (String, Vec<Box<dyn ToSql>>) {
        let mut parts: Vec<&'static str> = Vec::new();
        let mut args: Vec<Box<dyn ToSql>> = Vec::new();
        if !self.contains.is_empty() {
            parts.push("variant.key LIKE ? ESCAPE '\\'");
            args.push(Box::new(format!("%{}%", escape_like(&self.contains))));
        }
        match &self.platform {
            None => {}
            // **平台未知那一档要选得中**：它在表里是 `NULL`，而 `= NULL` 永远不成立。
            Some(PlatformFilter::Unknown) => parts.push("variant.platform IS NULL"),
            Some(PlatformFilter::Named(platform)) => {
                parts.push("variant.platform = ?");
                args.push(Box::new(platform.clone()));
            }
        }
        if let Some(collection) = &self.collection {
            // `EXISTS` 而不是 `JOIN`：一个变体可以在好几个合集里，`JOIN` 会让它在结果里
            // 出现好几行——总行数与页内容会一起说谎。
            parts.push(
                "EXISTS (SELECT 1 FROM collection_variant cv
                         JOIN collection c ON c.id = cv.collection_id
                         WHERE cv.variant_key = variant.key AND c.name = ?)",
            );
            args.push(Box::new(collection.clone()));
        }
        if let Some(language) = &self.language {
            // 逐个语言码比：`languages` 是逗号分隔的一串，两头各补一个逗号之后
            // `instr` 找的就是完整的一段，`en` 不会撞上 `Danish`。
            parts.push(
                "EXISTS (SELECT 1 FROM release r
                         WHERE r.id = variant.release_id
                           AND instr(',' || replace(COALESCE(r.languages,''), ' ', '') || ',',
                                     ',' || ? || ',') > 0)",
            );
            args.push(Box::new(language.clone()));
        }
        if let Some(mark) = &self.chinese {
            // 记号从**自动通过**的候选上读回来，与 `sublibrary::facts` 和
            // `adapter::converge` 同一条路——三处答案不一样的话，界面上筛出来的那批
            // 与真正导出去的那批就对不上。
            parts.push(
                "EXISTS (SELECT 1 FROM candidate c
                         WHERE c.variant_key = variant.key AND c.accepted <> 0 AND c.chinese = ?)",
            );
            args.push(Box::new(mark.clone()));
        }
        match self.state {
            None => {}
            Some(StateFilter::Unidentified) => parts.push(
                "NOT EXISTS (SELECT 1 FROM identification i WHERE i.variant_key = variant.key)",
            ),
            Some(StateFilter::Concluded(state)) => {
                parts.push(
                    "EXISTS (SELECT 1 FROM identification i
                             WHERE i.variant_key = variant.key AND i.state = ?)",
                );
                args.push(Box::new(state.label().to_string()));
            }
        }
        let mut parts: Vec<String> = parts.into_iter().map(str::to_string).collect();
        if self.non_game_assets == NonGameAssets::Hidden {
            // 问的是挂在连接上的那一处判断（[`register_non_game_asset`]），不在这儿另写一份
            // 「键里有没有 `bios`」。
            parts.push(format!("NOT {NON_GAME_ASSET_FN}(variant.key)"));
        }
        if let Some(rule) = &self.rule {
            let (sql, mut more) = crate::catalog::filter::rule_sql(rule);
            parts.push(sql);
            args.append(&mut more);
        }
        if parts.is_empty() {
            return (String::new(), args);
        }
        (format!(" WHERE {}", parts.join(" AND ")), args)
    }

    /// 折出 `ORDER BY` 那一段。列名来自 [`VariantOrder::column`]，不含任何用户输入。
    ///
    /// **作品那一列多一把键**：`work_name IS NULL` 排在最前面。SQLite 给 `NULL` 的默认
    /// 次序是「正序最前、倒序最后」——那意味着人点一下表头翻个方向，**还没认出作品**的
    /// 那一批就从表尾跳到表头。**理由不在这一批有多大**（真库上一千七百多个），
    /// 在于它们在这一列上根本没有值可比：没有值的东西，位置不该由方向决定。
    /// 空格子待在表尾，两个方向都是（票 `parking-3/11` 验收第 5 条）。
    ///
    /// 这一句只对作品那一列写。别的可空列（`platform`）照旧走 SQLite 那套默认次序——
    /// 那是既有行为，改它是另一件事。
    fn order_clause(&self) -> String {
        let direction = if self.descending { "DESC" } else { "ASC" };
        let column = self.order.column();
        if column == "key" {
            format!(" ORDER BY key {direction}")
        } else if column == WORK_NAME {
            // 并列行的次序由主键兜住，否则翻页会漏行与重行。
            format!(" ORDER BY {WORK_NAME} IS NULL, {WORK_NAME} {direction}, key {direction}")
        } else {
            // 并列行的次序由主键兜住，否则翻页会漏行与重行。
            format!(" ORDER BY {column} {direction}, key {direction}")
        }
    }
}

/// **变体表画出来的一行**：变体自己那一行，加上它的**作品名**。
///
/// ## 为什么另立一个类型，而不是给 [`VariantRow`] 加一列
///
/// [`VariantRow`] 是「`variant` 表的一行」——识别、成型、待确认队列、导出收敛十几处
/// 都在构造它、都在按这个意思读它。作品名不是那张表上的东西（它在 `work` 上），
/// 给它加一列等于让那十几处各自回答「这一栏我该填什么」，而其中大多数压根不关心作品。
/// 所以浏览要的那一行单立一个（挂账 D161 的 B 路，票 `parking-3/11`）。
///
/// 形状照 [`WorkVariant`] 那一条来：**装着**那一行，而不是把它的字段抄一遍。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseVariant {
    /// 变体自己那一行。
    pub variant: VariantRow,
    /// 它属于哪个**作品**；**识别还没认出来时是 `None`**——屏上那一格是空的。
    ///
    /// `None` 不是「作品叫空字符串」：这一档是 `adapter::converge` 的 `Anchor::Loose`，
    /// 真库上一千七百多个变体落在里面。两件事混成一件，排序时它们就会插进真有作品的
    /// 那一段里——空字符串排得进去，`NULL` 不该。
    pub work: Option<String>,
}

impl Catalog {
    /// 满足这个筛选条件的变体一共几行。**滚动条的长度**由它来。
    ///
    /// 单独一次查询而不是「取一页顺便数一下」：滚动条要的是总数，而总数与视口无关，
    /// 换了筛选才变。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn variant_total(&self, query: &VariantQuery) -> Result<u64, CatalogError> {
        let (where_sql, args) = query.where_clause();
        let sql = format!("SELECT COUNT(*) FROM variant{where_sql}");
        let count: i64 = self
            .conn
            .query_row(&sql, params_from_iter(args.iter()), |row| row.get(0))
            .map_err(|source| self.err(source))?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    /// 取一页变体：从第 `offset` 行起、最多 `limit` 行，已排好序。
    ///
    /// `limit` 会被夹到 [`MAX_PAGE`]——这个入口不接受「把全库读出来」这种要求。
    ///
    /// **只要变体自己那一行**。要连着作品名一起画的走
    /// [`variant_browse_page`](Self::variant_browse_page)——两条走的是同一条 SQL，
    /// 于是筛的、排的绝不会漂开。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn variant_page(
        &self,
        query: &VariantQuery,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<VariantRow>, CatalogError> {
        Ok(self
            .variant_browse_page(query, offset, limit)?
            .into_iter()
            .map(|row| row.variant)
            .collect())
    }

    /// 取一页**变体表画出来的行**：变体自己那一行，连它的**作品名**。
    ///
    /// **作品名是这一趟查询自己带回来的**（`LEFT JOIN work`），不是取回来之后拿
    /// [`work_names`](Self::work_names) 在内存里对——那张表真库上 9,226 行，而这一层
    /// 从头到尾在躲的就是「先把全库读进来」（ADR-0005、挂账 D161）。于是
    /// [`VariantOrder::Work`] 那一档排得成，筛也筛得动（`作品^…` 走
    /// `catalog::filter`），两样都落在 `ORDER BY` / `WHERE` 里。
    ///
    /// `limit` 会被夹到 [`MAX_PAGE`]，同 [`variant_page`](Self::variant_page)。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn variant_browse_page(
        &self,
        query: &VariantQuery,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<BrowseVariant>, CatalogError> {
        let limit = limit.min(MAX_PAGE);
        if limit == 0 {
            return Ok(Vec::new());
        }
        let (where_sql, mut args) = query.where_clause();
        let order_sql = query.order_clause();
        // 列名一律不加限定，只有作品名那一列例外——`variant` 与 `work` 两张表上没有
        // 同名的列（`work` 只有 `id` / `name` / `origin`），[`VARIANT_COLUMNS`]
        // 原样搬得过来。
        let sql = format!(
            "SELECT {VARIANT_COLUMNS}, work.name AS {WORK_NAME}\
             {VARIANT_BROWSE_FROM}{where_sql}{order_sql} LIMIT ? OFFSET ?"
        );
        args.push(Box::new(i64::try_from(limit).unwrap_or(i64::MAX)));
        args.push(Box::new(i64::try_from(offset).unwrap_or(i64::MAX)));
        let mut statement = self.conn.prepare(&sql).map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params_from_iter(args.iter()), |row| {
                Ok(BrowseVariant {
                    variant: read_variant_row(row)?,
                    // **按名字取那一列**，不按下标：`VARIANT_COLUMNS` 里加一列就会
                    // 把下标推走，而那一下不会有任何编译错误说话。
                    work: row.get(WORK_NAME)?,
                })
            })
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }
}

/// 筛选面板上一个可选值，连它选中多少个变体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facet {
    /// 值本身。平台未知那一档写成 [`UNKNOWN_PLATFORM_LABEL`]。
    pub value: String,
    /// 这个值选中多少个变体。
    pub count: u64,
}

/// 四个筛选维度各有哪些值可选。
///
/// **每一档都带着条数**：一个选不出东西的筛选条件，与一个「这个平台只有 3 个变体」的
/// 事实，在界面上得看得出区别——没有条数的下拉框，用户只能一个个点开试。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facets {
    /// 平台，按条数从多到少。
    pub platforms: Vec<Facet>,
    /// **合集**（与平台正交，ADR-0011），按条数从多到少。
    pub collections: Vec<Facet>,
    /// 发行版标着的语言，按条数从多到少。
    pub languages: Vec<Facet>,
    /// **中文身份**：`汉化` / `官中`（ADR-0012）。与语言那一维分开，见
    /// [`VariantQuery::chinese`]。
    pub chinese: Vec<Facet>,
    /// 识别状态，按 [`StateFilter::ALL`] 的次序。
    ///
    /// **不是 [`Facet`]，也不按条数排**：另外三维的值是库里现有的字符串（库里没有 PSV
    /// 就不该有 PSV 那一档），而这一维是个**闭集合**——四档结论加「还没识别」，
    /// 一档为零也照样摆出来。零本身是句话：「这份库里一条无判据都没有」与
    /// 「这份库没跑过识别」看起来会是同一片空白，而它们是两回事（ADR-0002）。
    pub states: Vec<(StateFilter, u64)>,
}

/// 按条数从多到少、同数按值排。同一份库问两次必须是同一个次序。
fn rank_facets(mut facets: Vec<Facet>) -> Vec<Facet> {
    facets.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.value.cmp(&b.value)));
    facets
}

impl Catalog {
    /// 筛选面板上那四个维度各有哪些值可选。
    ///
    /// 四条 `GROUP BY`，**不把变体读进内存**。语言那一条例外地在 Rust 里拆了一次：
    /// `release.languages` 是逗号分隔的一串，而 SQLite 没有拆串的内置函数——
    /// 拆的是**去重之后的组合**（真库里几十种），不是四万多个变体。
    ///
    /// **条数跟着「列出非游戏资产」那颗开关走**（`non_game_assets`）：收起时左栏若照旧把
    /// 它们数进去，「PS 7」点进去只列 6 个，这一屏就自己说了两个数（挂单 `Q775`）。
    /// 收起那一档给每条查询添同一句——问的是挂在连接上的那一处判断，不另写判据。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn facets(&self, non_game_assets: NonGameAssets) -> Result<Facets, CatalogError> {
        let mut out = Facets::default();
        // 收起那一档：只数不是非游戏资产的。`key` 是那条查询里变体的键那一列；
        // 列出那一档恒真，与改之前一字不差。
        let kept = |key: &str| match non_game_assets {
            NonGameAssets::Hidden => format!("NOT {NON_GAME_ASSET_FN}({key})"),
            NonGameAssets::Listed => "1".to_string(),
        };

        let mut statement = self
            .conn
            .prepare(&format!(
                "SELECT COALESCE(platform, ?1), COUNT(*) FROM variant WHERE {} GROUP BY platform",
                kept("variant.key"),
            ))
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([UNKNOWN_PLATFORM_LABEL], |row| {
                Ok(Facet {
                    value: row.get(0)?,
                    count: u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                })
            })
            .map_err(|source| self.err(source))?;
        out.platforms = rank_facets(
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|source| self.err(source))?,
        );

        let mut statement = self
            .conn
            .prepare(&format!(
                "SELECT c.name, COUNT(*) FROM collection_variant cv
                 JOIN collection c ON c.id = cv.collection_id
                 WHERE {}
                 GROUP BY c.name",
                kept("cv.variant_key"),
            ))
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok(Facet {
                    value: row.get(0)?,
                    count: u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                })
            })
            .map_err(|source| self.err(source))?;
        out.collections = rank_facets(
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|source| self.err(source))?,
        );

        let mut statement = self
            .conn
            .prepare(&format!(
                "SELECT r.languages, COUNT(*) FROM variant v
                 JOIN release r ON r.id = v.release_id
                 WHERE COALESCE(r.languages, '') <> '' AND {}
                 GROUP BY r.languages",
                kept("v.key"),
            ))
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|source| self.err(source))?;
        let mut by_language: std::collections::BTreeMap<String, u64> =
            std::collections::BTreeMap::new();
        for row in rows {
            let (combination, count) = row.map_err(|source| self.err(source))?;
            let count = u64::try_from(count).unwrap_or(0);
            for language in combination
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
            {
                *by_language.entry(language.to_string()).or_default() += count;
            }
        }
        out.languages = rank_facets(
            by_language
                .into_iter()
                .map(|(value, count)| Facet { value, count })
                .collect(),
        );

        let mut statement = self
            .conn
            .prepare(&format!(
                "SELECT c.chinese, COUNT(DISTINCT c.variant_key) FROM candidate c
                 WHERE c.accepted <> 0 AND c.chinese IS NOT NULL AND {}
                 GROUP BY c.chinese",
                kept("c.variant_key"),
            ))
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok(Facet {
                    value: row.get(0)?,
                    count: u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                })
            })
            .map_err(|source| self.err(source))?;
        out.chinese = rank_facets(
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|source| self.err(source))?,
        );

        let mut statement = self
            .conn
            .prepare(&format!(
                "SELECT state, COUNT(*) FROM identification WHERE {} GROUP BY state",
                kept("variant_key"),
            ))
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|source| self.err(source))?;
        let mut by_state: std::collections::BTreeMap<String, u64> =
            std::collections::BTreeMap::new();
        let mut concluded: u64 = 0;
        for row in rows {
            let (label, count) = row.map_err(|source| self.err(source))?;
            let count = u64::try_from(count).unwrap_or(0);
            concluded += count;
            *by_state.entry(label).or_default() += count;
        }
        // 拿来减的总数与上面那一句同一档：收起时两边都不数非游戏资产，减出来才是真的「还没识别」。
        let total = self.variant_total(&VariantQuery {
            non_game_assets,
            ..VariantQuery::default()
        })?;
        out.states = StateFilter::ALL
            .into_iter()
            .map(|filter| match filter {
                StateFilter::Concluded(state) => {
                    (filter, by_state.get(state.label()).copied().unwrap_or(0))
                }
                // 「还没识别」数不出行来——它恰恰是**没有行**的那些，只能拿总数减。
                StateFilter::Unidentified => (filter, total.saturating_sub(concluded)),
            })
            .collect();

        Ok(out)
    }
}

// ══ 作品级的主列表 ═══════════════════════════════════════════════════════════
//
// 变体表是**磁盘上有什么**，主列表是**我有哪些游戏**。真库里 46,444 个变体收敛成
// 一万出头的行，六成作品下面挂着不止一个变体——翻库时人认的是后者，变体在详情面板里挑。
//
// ## 为什么不是把变体表在界面里聚合
//
// 聚合要先看见全部行才数得出「这个作品有几个变体」。那正是这一层从头到尾在躲的事
// （ADR-0005）。所以它是**新的一个查询面**：`GROUP BY` 在 SQLite 里做，界面照旧
// 只拿视口那几十行。
//
// ## 认不出作品的那些**自成一行**，不许被吞掉
//
// 与导出那一侧同一条口径（`adapter::converge` 的 `Anchor::Work` / `Anchor::Loose`）：
// 识别认出作品的按作品收敛，没认出来的一个变体一行。真库上后者是一多半——把它们
// 折进「未知」那一行，等于让人在界面上看不见自己一半的库。

/// 主列表这一行**是谁**。
///
/// 两支的分野正是 `converge` 那条：识别认出作品的按作品收敛，没认出来的一个变体一行。
///
/// **它跨得过一趟识别重跑，但仍然不是能存起来的东西。** [`Work`](Self::Work) 里那个数是
/// `work` 表的行号，而重跑识别从票 parking-3/10 起**按名字复用现成的那一行**
/// （[`Catalog::clear_identifications`]、[`Catalog::work_named`]），于是挂在这个 id 上的
/// 收藏、媒体与点开的那一行不再集体失联（挂单 `Q193`）。
///
/// 换不换号这件事从「每趟都换」变成了「**只在那一行真的没了的时候才换**」：那个作品
/// 底下最后一个变体没了（重扫收掉它），或者这一趟识别一条判据都没落在它身上
/// （[`Catalog::drop_unheld_identified_works`] 收掉它），下一次它回来拿的是新号。
/// 所以要**存**的东西（裁决、收藏、刮削结论）照旧一律挂**内容锚**或作品名，
/// 见 `catalog::scrape` 的模块文档；界面拿着它翻页、多选、点开详情是它该干的活。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorkAnchor {
    /// 认出了**作品**：这一行是那个作品，底下挂着它的全部变体。
    ///
    /// 那个数是 `work` 表的行号，**跨识别重跑不换**（见枚举文档）。
    Work(i64),
    /// **还没认出作品**：这一行就是那一个变体，键是它自己。
    Loose(String),
}

impl WorkAnchor {
    /// 这一行的**刮削锚点**：认出作品的挂**作品名**（`name`），没认出来的挂**变体的键**。
    ///
    /// 与 `converge`、SQL 那一侧的 `row_anchor!` 同一条口径；Rust 这一侧问「这一行的年份、
    /// 元数据、封面挂在哪儿」的几处（`Catalog::fill_scraped`、年份、`Catalog::cover_of`）
    /// 都从这儿取，不各写一个 `match`。
    #[must_use]
    pub fn scrape_anchor<'a>(&'a self, name: &'a str) -> (AnchorKind, &'a str) {
        match self {
            Self::Work(_) => (AnchorKind::Work, name),
            Self::Loose(key) => (AnchorKind::Variant, key),
        }
    }
}

/// 主列表按哪一列排。
///
/// 与 [`VariantOrder`] 一样是个闭集合：拼进 `ORDER BY` 的只能来自这里。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkOrder {
    /// 作品名。没认出作品的那些行用它自己的键。
    ///
    /// 那些行屏上主栏画的是**正题**（[`WorkRow::title`]），键画在副行——所以按这一列排时
    /// 它们照的是副行那条相对路径，屏上看得出凭什么排在这儿（挂单 `Q802`，交给按列排序那张票定）。
    #[default]
    Name,
    /// 平台。一行可以跨好几个平台（同一部作品在 GB 与 GBC 上各有变体），
    /// 排的是这一行平台集合里**排最前**的那个。
    Platform,
    /// 变体数。
    Variants,
    /// 容量合计（下界，ADR-0021）。
    Bytes,
    /// 年份。
    Year,
}

impl WorkOrder {
    /// 全部可排的列，界面照这个次序摆表头。
    pub const ALL: [Self; 5] = [
        Self::Name,
        Self::Platform,
        Self::Variants,
        Self::Bytes,
        Self::Year,
    ];

    /// 这一列在界面上叫什么。用**词表**里的词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "作品",
            Self::Platform => "平台",
            Self::Variants => "变体",
            Self::Bytes => "容量",
            Self::Year => "年份",
        }
    }

    /// 拼进 `ORDER BY` 的那个名字。**只有这个函数认得它们**，而且它们全是
    /// [`WORK_ANCHOR_COLUMNS`] 与 [`Self::select`] 里自己起的别名，一个字都不来自外面。
    fn column(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Platform => "platform",
            Self::Variants => "variants",
            Self::Bytes => "bytes",
            Self::Year => "year",
        }
    }

    /// 挑这一页那一趟要**多算的那一列**，连前面那个逗号。
    ///
    /// 排序要有值可比，所以按哪一列排就得算哪一列——但**一次只算一列**：
    /// 别的几样等挑完这一页再算（[`Catalog::work_page_totals`]），
    /// 那时只剩几百行，不是一万多组。
    ///
    /// 作品名那一档是空的：它本来就在 [`WORK_ANCHOR_COLUMNS`] 里。
    fn select(self) -> &'static str {
        match self {
            Self::Name => "",
            Self::Platform => ",\n    MIN(variant.platform) AS platform",
            Self::Variants => ",\n    COUNT(*) AS variants",
            Self::Bytes => ",\n    SUM(variant.bytes) AS bytes",
            // `MIN(year.value)` 只是个取值器：同一行里 `year` 是常数（作品名一样，
            // join 出来的就是同一条）。**只有这一档才连年份那张表**。
            Self::Year => ",\n    MIN(year.value) AS year",
        }
    }
}

/// 主列表上「**元数据齐不齐**」看的是作品这一层该有的哪几样。
///
/// **标题不在里面**：叫法是个集合不是单值（`catalog::title`），缺不缺由标题集合自己答。
/// **汉化组也不在里面**：它挂在**变体**上（ADR-0012），一个作品下有汉化版才谈得上，
/// 拿它当作品级的缺口会让一整排原版游戏都显示成「缺」。
pub const WORK_FIELDS: [Field; 5] = [
    Field::Year,
    Field::Publisher,
    Field::Developer,
    Field::Genre,
    Field::Description,
];

/// 主列表的一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkRow {
    /// 这一行是谁。批量操作的作用范围靠它展开（[`Catalog::scoped_variants`]）。
    pub anchor: WorkAnchor,
    /// 这一行的名字：作品名，或者那个变体的键。**排的、搜的是它**（[`WorkOrder::Name`]）。
    ///
    /// 认不出作品的那一行屏上主栏画的不是它，是 [`title`](Self::title) 那个正题；它自己
    /// （变体的键，也就是那份内容在主库里的相对路径）画在副行。
    pub name: String,
    /// 认出作品的那一行：这个作品的**显示标题**——[`title::choose`](crate::title::choose) 挑的那一个，
    /// 与详情面板（[`Catalog::variant_detail`]）、导出写进去的是同一个。
    ///
    /// **取不到时是 `None`**：标题集合是空的（`choose` 只能退回作品名），或者挑出来的就是作品名
    /// 本身——那时屏上主栏印作品名、第二行不写。认不出作品的那一行一律是 `None`，它屏上的名字是
    /// [`title`](Self::title) 那个正题。
    ///
    /// **只有 [`Catalog::work_page_with_titles`] 补它**：挑显示标题要一份优先级表，
    /// [`Catalog::work_page`] 手上没有，那一趟出来的行上这一格一律是 `None`。
    pub display: Option<String>,
    /// 认不出作品的那一行：那个变体**主文件**的键，[`title`](Self::title) 从它剥正题；
    /// 认出作品的那一行是 `None`。
    ///
    /// **不对外**：要的是正题就从 `title` 取。把它交出去，就会有调用方自己拿它去剥
    /// ——剥哪个名字、剥完空了怎么办，那是第二个判据（ADR-0024）。
    main_key: Option<String>,
    /// **平台集合**。一部作品可以横跨好几个平台，真库上 9,226 个作品里有 2,314 个如此。
    pub platforms: Vec<String>,
    /// 底下挂着几个变体。**按当前筛选算**——屏上写着几个，批量操作就作用于那几个。
    pub variants: u64,
    /// 容量合计。**是个下界**：元数据读不到的成员按 0 计入（ADR-0021）。
    pub bytes: u64,
    /// 其中有几个成员的元数据读不到。
    pub unreadable_files: u64,
    /// 年份；一条都没刮到时是 `None`。
    pub year: Option<String>,
    /// [`WORK_FIELDS`] 里**一个值都没有**的那几样。空着就是齐了。
    pub missing: Vec<Field>,
    /// 底下那些变体里**最高的那档置信度**（ADR-0002）；
    /// 一条候选都没有时是 `None`，那不是「没撞上」——是**没有候选**或者**还没识别**，
    /// 哪一个由 [`identified`](Self::identified) 分辨。
    pub confidence: Option<Confidence>,
    /// 已采纳候选的中文身份：汉化 / 官中。
    pub chinese: Vec<String>,
    /// 底下那些变体**是不是全都跑过识别**（按当前筛选）。
    ///
    /// **它是「还没识别」与「没有候选」之间那条界线**（`CONTEXT.md` 两条词条）：
    /// [`confidence`](Self::confidence) 那个 `None` 一个人装着两件事——一个变体连识别都
    /// 还没跑过（库里连它的结论都没有），和一个跑过了却一条候选都没有。这条界线正是
    /// **命中率的分母**那条界线（ADR-0002），分不出来，屏上就得挑一件事去撒谎。
    ///
    /// **一行是一批变体，所以这里问的是「是不是全都」而不是「有没有一个」**，
    /// 而这与 [`confidence`](Self::confidence) 取最高的那一档**不是同一条道理**：
    /// 候选是**正面事实**（有一条就是有，取最好的那条不冤枉谁），而「跑过没跑过」是
    /// **覆盖度**——折成「最好的那个」等于让跑过的那一条替还没跑过的那九条说话。
    /// 十个变体里一个跑过、九个还没轮到，屏上说「没有候选」就是在说
    /// 「识别跑过了、只是一个字都没说，接下来得你自己来」，而这一行真正该做的事是
    /// **先跑一趟 `romcat identify`**。所以**只要还剩一个没跑过，这一行就说还没识别**。
    ///
    /// 库里一共还剩多少个变体没跑过，另有一份按变体数的账
    /// （[`Catalog::not_run_count`](super::Catalog::not_run_count)），队列屏在屏头单说
    /// 一句，两处谁也不替谁说话。
    pub identified: bool,
    /// 搜索框打的那几个字**命中在哪儿**；没搜的时候是 `None`。
    ///
    /// 屏上要印得出来：一行名字里一个搜索词都没有的作品冒在前面，不说清它是**别名**
    /// 还是**简介**命中的，那就是这份规格从头到尾在消灭的那种「看不懂」。
    pub hit: Option<SearchHit>,
    /// 这一行是不是**非游戏资产**（ADR-0010）：它底下那些变体（按当前筛选）**全都是**。
    ///
    /// 那一处判断（`classify::non_game_asset`）在库里逐个变体答完、折成这一行的——界面照着
    /// 标，不自己判（ADR-0024）。收起时（[`NonGameAssets::Hidden`]）列着的行一行都不会是；
    /// 一部作品底下夹着一个被人手工挂进来的 BIOS，那一行不算，那个变体在详情面板里标着
    /// （[`WorkVariant::non_game_asset`]）。
    pub non_game_asset: bool,
}

impl WorkRow {
    /// 元数据齐了吗。
    #[must_use]
    pub fn complete(&self) -> bool {
        self.missing.is_empty()
    }

    /// **认不出作品的那一行叫什么**：那个变体的**正题**；认出作品的那一行是 `None`。
    ///
    /// 两件事都在这儿答，界面照着画（ADR-0005）：「认不认得出作品」看 [`anchor`](Self::anchor)
    /// 是不是 [`WorkAnchor::Loose`]；「正题是什么」从那个变体的**主文件名**剥
    /// （[`Rules::parse_main_key`](crate::filename::Rules::parse_main_key)，撞中文离线源的
    /// 那一处剥的也是它）。
    ///
    /// **规则由调用方交进来**，因为剥离规则是配置：工作目录里那份 `name-rules.toml`
    /// （[`sources::rules`](crate::sources::rules)）拿去刮削，屏上这一行就得拿同一份剥，
    /// 不然两处是两个正题。
    ///
    /// 整个文件名剥完一个字都不剩时退回**主文件名**：空着一行，比印一串记号更认不出它是谁。
    #[must_use]
    pub fn title(&self, rules: &crate::filename::Rules) -> Option<String> {
        Some(loose_title(rules, self.main_key.as_deref()?))
    }

    /// 「元数据」那一栏画成什么。齐了就一个字，缺了就点名缺哪几样。
    #[must_use]
    pub fn missing_label(&self) -> String {
        if self.missing.is_empty() {
            return "齐".to_string();
        }
        if self.missing.len() == WORK_FIELDS.len() {
            return "缺全部".to_string();
        }
        format!(
            "缺{}",
            self.missing
                .iter()
                .map(|field| field.label())
                .collect::<Vec<_>>()
                .join("、")
        )
    }

    /// 「元数据」那一栏那枚**短标签**上的字（设计稿主列表的 `.chip`；词是拿主意的人 2026-09-14
    /// 照稿定的）。
    ///
    /// 先看这一行**跑没跑过识别、认没认出作品**，再看**元数据齐不齐**：
    ///
    /// 1. 底下还有变体没跑过识别：**还没识别**——与 [`confidence_label`](Self::confidence_label)
    ///    那个词是同一条判据，这里直接问它，不另判一次（ADR-0024）；
    /// 2. 认不出作品、手上有候选却一条都没定下来（最高那档不到高置信）：**待确认**；
    /// 3. 认不出作品、一条候选都没有、一样元数据都没采到：**仅文件名**；
    /// 4. 其余照 [`missing`](Self::missing) 说：一样不缺是**完整**，缺一样点名是哪一样（「缺简介」），
    ///    缺几样是「缺 N 项」，全缺是**缺全部**。
    ///
    /// 颜色不在这儿：界面照这一行那一档置信度上色（[`tier`](Self::tier)）。
    #[must_use]
    pub fn meta_label(&self) -> String {
        if self.confidence_label() == NOT_RUN_LABEL {
            return NOT_RUN_LABEL.to_string();
        }
        if matches!(self.anchor, WorkAnchor::Loose(_)) {
            match self.confidence {
                Some(confidence) if confidence != Confidence::High => {
                    return "待确认".to_string();
                }
                None if self.missing.len() == WORK_FIELDS.len() => {
                    return "仅文件名".to_string();
                }
                _ => {}
            }
        }
        match self.missing.as_slice() {
            [] => "完整".to_string(),
            [one] => format!("缺{}", one.label()),
            all if all.len() == WORK_FIELDS.len() => "缺全部".to_string(),
            some => format!("缺 {} 项", some.len()),
        }
    }

    /// 置信度那一栏画成什么。**「还没识别」是独立的一档**（ADR-0002）。
    ///
    /// **词一个字都不自己写**（票 `gui-redesign/12`）：这一栏从前印的是「高 / 中 / 低」，
    /// 而待确认屏印的是「高置信 / 中置信 / 低置信」——同一件事在两屏上是两个词，
    /// 用户会以为那是两回事。三档置信度与「没有候选」走 [`Tier::label`]，
    /// 「还没识别」走 [`NOT_RUN_LABEL`]，两个常量各在核心库里只写一处。
    ///
    /// **两个词分开印**（票 `gui-redesign/17`）：一条候选都没有时，
    /// [`identified`](Self::identified) 说这一行底下的变体**是不是全都跑过识别**——
    /// 全都跑过了是**没有候选**（识别说完话了，只是一个字都没说得出来，接下来得人自己来），
    /// 还剩一个没跑过就是**还没识别**（该做的事是先跑一趟 `romcat identify`）。
    /// 两件事印同一个词的话，这一栏就会对着一整批压根没识别过的变体说
    /// 「识别跑过了、没找着」。
    #[must_use]
    pub fn confidence_label(&self) -> &'static str {
        if self.confidence.is_none() && !self.identified {
            return NOT_RUN_LABEL;
        }
        Tier::of(self.confidence).label()
    }

    /// 这一行落在**置信度四档**的哪一档。屏上要上色的地方拿它，不自己 `match`。
    ///
    /// **还没识别的那一行与没有候选的那一行同一个颜色**：那两件事的区别由
    /// [`confidence_label`](Self::confidence_label) 那个**词**说，不由颜色说。
    /// 色觉障碍下颜色全糊成一片，读得出来的只有字（`romcat_gui::look` 的
    /// 「颜色不是唯一线索」）——所以这一档没必要、也不该再分出第五个颜色来。
    #[must_use]
    pub fn tier(&self) -> Tier {
        Tier::of(self.confidence)
    }
}

/// 主列表这一行**画出来的那个名字**在 SQL 里怎么取。
///
/// **只有这一处写它**：[`WORK_ANCHOR_COLUMNS`] 里那一列、[`WORK_FROM`] 里年份那张
/// join、搜索框那条名字命中路（[`Search::name`]），全都从这儿展开——排的、画的、搜的
/// 不是同一串字的话，屏上会出现一行「凭什么排在这儿」看不出答案的结果。
///
/// 没认出作品的那一行**主栏**画的是从这串键里剥出来的正题（[`WorkRow::title`]），
/// 这串键本身画在副行，于是排的、搜的那一串照旧在屏上（挂单 `Q802`）。
///
/// **是个宏而不是常量**，因为那几处里有两处是 `const &str`：`const` 里拼不了
/// `format!`，而 `concat!` 只吃字面量与展开成字面量的宏。写成常量的话那两处只能各自
/// 再抄一遍，而「只有这一处写它」这句话就成了空话——那正是它要防的事。
macro_rules! row_name {
    () => {
        "COALESCE(work.name, variant.key)"
    };
}

/// 主列表这一行的**刮削锚点**在 SQL 里怎么取：认出作品的挂**作品名**，
/// 没认出来的挂**变体的键**（与 `converge`、[`Catalog::fill_scraped`] 同一条口径）。
///
/// 两个 `?` 按出现次序是**变体**、**作品**——与 [`WORK_FROM`] 里年份那张 join 写法一样。
macro_rules! row_anchor {
    () => {
        "CASE WHEN variant.work_id IS NULL THEN ? ELSE ? END"
    };
}

/// 搜索框打的这几个字**命中在哪儿**，也就是这一行排在哪一档。
///
/// 五档的次序就是票 `gui-redesign/05` 那句「以搜索词开头 > 含有 > 别名命中 >
/// 简介命中」，只是**别名那一档也分开头与含有**——中文搜索几乎整批落在别名上
/// （作品名来自 DAT，DAT 里没有中文），不分开的话「匹配得好的排前面」这条承诺
/// 在中文那一侧根本兑现不了（挂单 Q104）。
///
/// **枚举的次序就是排序的次序**：`derive` 出来的 `Ord` 与 `Search::rank` 折进 SQL 的
/// 那几个数一一对应，两处由 [`SearchHit::ALL`] 钉在一起。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SearchHit {
    /// 屏上那个名字**以搜索词开头**。
    TitleStart,
    /// 屏上那个名字**含有**搜索词。
    Title,
    /// **标题集合**里别的叫法（中文名、译名、汉化组译名……）以搜索词开头。
    AliasStart,
    /// 标题集合里别的叫法**含有**搜索词。
    Alias,
    /// **简介**里含有搜索词。排最后：一段话里出现过这几个字，离「就是它」最远。
    Description,
}

impl SearchHit {
    /// 五档，从匹配得最好的排到最差的。**下标就是折进 SQL 的那个数**。
    pub const ALL: [Self; 5] = [
        Self::TitleStart,
        Self::Title,
        Self::AliasStart,
        Self::Alias,
        Self::Description,
    ];

    /// 打给用户的那句话。屏上要看得出这一行**凭什么**排在这儿——一个名字里
    /// 一个搜索词都没有的行冒在前面，不说清就是「看不懂」。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::TitleStart => "标题以它开头",
            Self::Title => "标题含有它",
            Self::AliasStart => "别名以它开头",
            Self::Alias => "别名含有它",
            Self::Description => "简介里提到它",
        }
    }

    /// SQL 里那个名次折回来；认不出（真到不了）就是 `None`。
    fn from_rank(rank: i64) -> Option<Self> {
        usize::try_from(rank)
            .ok()
            .and_then(|at| Self::ALL.get(at))
            .copied()
    }
}

/// 搜索框那几个字折出来的两条 `LIKE` 模板。
///
/// **收窄**（`WHERE`）与**排第几档**（`SELECT` 里那个 `CASE`）两处都问它要谓词，
/// 而不是各写一遍：两处漂开的后果是「搜出来了却排在最后一档」，而那种走样是静默的。
struct Search {
    /// 以搜索词开头：`词%`。
    start: String,
    /// 含有搜索词：`%词%`。
    contains: String,
}

impl Search {
    /// 折出这两条模板；搜索框是空的（或者只打了空白）时是 `None`。
    ///
    /// **大小写只折 ASCII**，与 `catalog::filter` 那一侧同一个口径：SQLite 的 `lower()`
    /// 本来就只动 ASCII，这边跟着走 `to_ascii_lowercase` 才不会两处答案分家。
    /// 汉字没有大小写，这条限制在中文上不咬人。
    ///
    /// **繁简不折**：搜「合金弹头」搜不到「合金彈頭」。那是匹配算法那条线的活
    /// （挂账 D121），照现状，不在这一票里顺手做掉（挂单 Q107）。
    fn new(text: &str) -> Option<Self> {
        let needle = text.trim();
        if needle.is_empty() {
            return None;
        }
        let escaped = escape_like(&needle.to_ascii_lowercase());
        Some(Self {
            start: format!("{escaped}%"),
            contains: format!("%{escaped}%"),
        })
    }

    /// 「屏上那个名字撞上这条模板」。
    fn name(pattern: &str) -> (String, Box<dyn ToSql>) {
        (
            concat!("lower(", row_name!(), ") LIKE ? ESCAPE '\\'").to_string(),
            Box::new(pattern.to_string()),
        )
    }

    /// 「**标题集合**里有一条叫法撞上这条模板」。
    ///
    /// 锚点是**作品名**（`catalog::title` 的模块文档），于是**还没认出作品**的那些行
    /// 天然够不着这一条——它们压根没有标题集合，屏上那个名字就是它自己的键。
    /// `NULL IN (…)` 不成立，那一支自己就落了空，不必再写一句。
    ///
    /// **写成集合成员判定而不是相关子查询**（挂单 Q108）：`EXISTS` 里带着
    /// `t.work = work.name` 就与外层绑死了，SQLite 只能**逐个变体行**去 `title` 里探一次
    /// ——真库形状上那是 46,428 次探查，实测单这一条 52 毫秒。写成
    /// `work.name IN (SELECT …)` 之后子查询与外层无关，一次算完存进一张临时索引，
    /// 每一行只剩一次查表。**筛出来的是同一批行**：两种写法对每一行的真假完全一致。
    fn alias(pattern: &str) -> (String, Box<dyn ToSql>) {
        (
            "work.name IN (SELECT t.work FROM title t
                            WHERE lower(t.value) LIKE ? ESCAPE '\\')"
                .to_string(),
            Box::new(pattern.to_string()),
        )
    }

    /// 「这一行的**简介**里有这段文字」。
    ///
    /// **锚点走主列表自己那条口径**：认出作品的看**作品**锚点、比作品名，没认出来的看
    /// 它自己的**变体**锚点、比那个键——下面那两支就是这条口径摊开写的（它不再展开
    /// [`row_anchor!`]，那个宏如今只剩 [`WORK_FROM`] 一个用户）。而**不是**
    /// `catalog::filter` 那条「两个锚点合起来看」。两条口径不一样，这里必须挑主列表
    /// 这一条，有两个理由：
    ///
    /// 1. **它得是组内恒定的。** 收窄落在 `WHERE` 上、逐个变体行判，而
    ///    `catalog::filter` 那条的变体分支比的是 `sv.subject = variant.key`
    ///    ——**逐行不同**。一部挂着 6 个变体的作品，若只有其中一个变体身上写着简介，
    ///    分完组之后这一行就只剩那 1 个变体：屏上「变体数」写 1 而不是 6，
    ///    容量与平台集合跟着缩水，随后「全选 → 批量刮削」也只作用到那一个。
    ///    搜索是**找这一行**，不该顺手改掉这一行有几个变体。
    /// 2. **它得与同一屏上别处说的话一致。** 那一行「元数据齐不齐」里的**简介**
    ///    正是按这条锚点判的（[`Catalog::fill_scraped`]），年份那一列也是
    ///    （[`WORK_FROM`]）。挑另一条口径的话，屏上一行写着「缺简介」，
    ///    搜索却说它「简介里提到它」。
    ///
    /// **两支各自写成集合成员判定**，理由同 [`alias`](Self::alias)：相关子查询要
    /// 逐个变体行去 `scrape_value` 里探一次（实测单这一条 32 毫秒），
    /// 而这两条子查询与外层无关，各扫一遍那张表就算完。
    ///
    /// 两支合起来与原先那一条**逐行等价**：认出作品的行 `work.name` 非空、锚点是作品，
    /// 只可能落进前一支；没认出作品的行 `work.name` 是 `NULL`，前一支不成立，
    /// 由后一支按变体锚点判。后一支那句 `variant.work_id IS NULL` **不能省**——
    /// 一个挂在作品下的变体身上也可以写着变体锚点的简介，而屏上那一行看的是作品那一条。
    ///
    /// 参数按出现次序：字段、作品锚点、模板、字段、变体锚点、模板。
    fn description(pattern: &str) -> (String, Vec<Box<dyn ToSql>>) {
        (
            "(work.name IN (SELECT sv.subject FROM scrape_value sv
                             WHERE sv.field = ? AND sv.anchor = ?
                               AND lower(sv.value) LIKE ? ESCAPE '\\')
              OR (variant.work_id IS NULL
                  AND variant.key IN (SELECT sv.subject FROM scrape_value sv
                                       WHERE sv.field = ? AND sv.anchor = ?
                                         AND lower(sv.value) LIKE ? ESCAPE '\\')))"
                .to_string(),
            vec![
                Box::new(Field::Description.label().to_string()),
                Box::new(AnchorKind::Work.label().to_string()),
                Box::new(pattern.to_string()),
                Box::new(Field::Description.label().to_string()),
                Box::new(AnchorKind::Variant.label().to_string()),
                Box::new(pattern.to_string()),
            ],
        )
    }

    /// **收窄**那一条：三条命中路里任一条成立。
    ///
    /// 三条一律用「含有」那条模板——以词开头的必然也含有它，多写一条只是白扫一遍。
    ///
    /// **三条都是组内恒定的**：名字读的是 [`row_name!`]（作品名，或者没认出作品时那一个
    /// 变体自己的键），别名读的是 `work.name`，简介两支读的是 `work.name` 与
    /// `variant.key`——**全是这一组的身份本身**。这一条是 [`rank`](Self::rank) 那层
    /// `MIN` 成立的前提，是「搜索不改这一行有几个变体」成立的前提，也是
    /// [`Catalog::work_page_totals`] 那一趟敢不带搜索谓词的前提。
    ///
    /// **次序是从便宜排到贵的**：SQLite 的 `OR` 短路求值，名字就命中的行不必再去查
    /// 后两条那两张临时索引。**这只值一点点**——后两条如今是非相关子查询
    /// （[`alias`](Self::alias)、[`description`](Self::description)），临时索引一次就建好，
    /// 短路省下的只是每行一次查表，不是整趟扫描。次序照旧这么摆，因为它不花钱。
    fn filter(&self) -> (String, Vec<Box<dyn ToSql>>) {
        let (name_sql, name_arg) = Self::name(&self.contains);
        let (alias_sql, alias_arg) = Self::alias(&self.contains);
        let (desc_sql, desc_args) = Self::description(&self.contains);
        let mut args: Vec<Box<dyn ToSql>> = vec![name_arg, alias_arg];
        args.extend(desc_args);
        (format!("({name_sql} OR {alias_sql} OR {desc_sql})"), args)
    }

    /// **排第几档**那一条：一条 `CASE`，数小的排前面。
    ///
    /// 外面那层 `MIN` 只是取值器：这四条谓词读的都是**这一组里恒定的东西**
    /// （[`row_name!`]，或者别名那两条读的 `work.name`），组里每一行算出来都一样。
    /// [`filter`](Self::filter) 那三条同样如此——两处若有一条逐行不同，
    /// 这一行有几个变体就会跟着搜索词变，见 [`description`](Self::description)。
    ///
    /// **落到最后一档的只可能是简介命中**，所以那一路不必再写一遍：
    /// [`filter`](Self::filter) 已经保证了三条里至少一条成立，前四支都没接住，
    /// 剩下的就只有简介。省下的是 `scrape_value` 那两张临时索引再建一遍。
    fn rank(&self) -> (String, Vec<Box<dyn ToSql>>) {
        let mut parts = String::new();
        let mut args: Vec<Box<dyn ToSql>> = Vec::new();
        for (at, (sql, arg)) in [
            Self::name(&self.start),
            Self::name(&self.contains),
            Self::alias(&self.start),
            Self::alias(&self.contains),
        ]
        .into_iter()
        .enumerate()
        {
            parts.push_str(&format!(" WHEN {sql} THEN {at}"));
            args.push(arg);
        }
        let last = SearchHit::ALL.len() - 1;
        (format!("MIN(CASE{parts} ELSE {last} END) AS hit"), args)
    }
}

/// 主列表一次翻页要的是哪一段。
///
/// 六个筛选维度**加上那棵条件组**与变体表**共用一份**（`VariantQuery::where_clause`）：规格里那条
/// 贯穿全局的约定是「**主列表的筛选就是子库的规则**」，而子库选的是变体——两处筛的
/// 若不是同一批变体，界面上筛出来的那批与真正导出去的那批就对不上。
///
/// 只有 [`search`](Self::search) 是主列表自己的，而且它与那几维**不是一类东西**：
/// 那几维筛集合，它排顺序（见模块文档「搜索框」那一节）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkQuery {
    /// **搜索框**里打的那几个字：打完之后**匹配得好的排前面**（票 `gui-redesign/05`）。
    ///
    /// 三条命中路——屏上那个名字、**标题集合**里别的叫法、**简介**——里有一条撞上就留下，
    /// 撞在哪一条决定这一行排哪一档（[`SearchHit`]）。空串（或者只有空白）等于没搜。
    ///
    /// **它进不了子库的规则**（[`Unruly::Search`]）：子库要的是集合，不是顺序。
    pub search: String,
    /// 只要这个**平台**的变体；[`PlatformFilter::Unknown`] 选的是平台未知那一档。
    pub platform: Option<PlatformFilter>,
    /// 只要在这个**合集**里的变体。
    pub collection: Option<String>,
    /// 只要发行版标着这个语言的变体。
    pub language: Option<String>,
    /// 只要带这个**中文身份**记号的变体：`汉化` / `官中`（ADR-0012）。
    pub chinese: Option<String>,
    /// 只要**识别状态**是这一档的变体。
    pub state: Option<StateFilter>,
    /// **筛选器那棵条件树**。见 [`VariantQuery::rule`]——两处共用同一份。
    pub rule: Option<Rule>,
    /// **非游戏资产**列不列出来；默认收起。见 [`VariantQuery::non_game_assets`]——两处共用同一份。
    ///
    /// **它进不了子库的规则**（[`Self::to_rule`] 连读都不读）：它管的是屏上列不列出，
    /// 子库选的变体照旧由规则说了算。
    pub non_game_assets: NonGameAssets,
    /// 按哪一列排。
    pub order: WorkOrder,
    /// 倒着排。
    pub descending: bool,
    /// 卡片墙的临时呈现条件；不属于筛选器，也不写进子库规则。
    pub cover_only: bool,
}

/// 当前筛选里**写不成规则**的那一条。
///
/// 存成子库时它必须挡住而不是被悄悄漏掉：漏一条，子库选出来的就比屏上多，
/// 而「筛出来的这一批」与「导过去的那一批」对不上正是这份规格要消灭的东西。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unruly {
    /// **搜索框**里打了字。搜索与筛选是两件事：搜索管的是排序，筛选管的是集合
    /// （票 `gui-redesign/05`）。
    Search,
    /// 按**识别状态**筛了。规则语言里没有这一维——子库的规则是「我要什么内容」，
    /// 而「这个变体识别到哪一步了」是这一趟的进度，下次重跑就变。
    State,
    /// 选的是**平台未知**那一档。写成规则要把全部已知平台列一遍，而清单一变那条规则
    /// 就悄悄失效——宁可不写。
    UnknownPlatform,
    /// 这一维选中的值**里面带逗号**，而逗号是规则里的值分隔符，写进去会被读成两个值。
    Comma(Dimension),
    /// 这一维选中的值里有规则语言的记号（两侧带空白的连接词、或者没配对的括号），
    /// 写进去读回来就不是它自己了。
    ///
    /// **合集名是用户自己起的**，「送朋友的 或 备份」「口袋(日版」都合法——
    /// 折成规则那一步得挡住它，而不是折出一条读回来变了样的规则。
    Unwritable(Dimension),
}

impl Unruly {
    /// 界面上照原样印的那句话。**说清是哪一条、以及怎么办**。
    #[must_use]
    pub fn advice(self) -> String {
        match self {
            Self::Search => "搜索框里还有字。搜索管的是排序、筛选管的是集合，\
                             它进不了规则——先把搜索框清空。"
                .to_string(),
            Self::State => "还筛着「识别状态」。那是这一趟的进度不是内容，\
                            重跑识别就变，写不进子库的规则——先把它设回「不筛」。"
                .to_string(),
            Self::UnknownPlatform => "还筛着「平台未知」。写成规则要把全部已知平台列一遍，\
                                      而清单一变那条规则就悄悄失效——先把平台设回「不筛」。"
                .to_string(),
            Self::Comma(dimension) => format!(
                "选中的那个{}里带逗号，而逗号是规则里的值分隔符，写进去会被读成两个值。",
                dimension.label()
            ),
            Self::Unwritable(dimension) => format!(
                "选中的那个{}里有规则语言的记号（两侧带空白的「且」「或」，\
                 或者没配对的括号），写进规则读回来就不是它自己了。",
                dimension.label()
            ),
        }
    }
}

/// 这个值写进规则之后**读回来还是它自己**吗。
///
/// **建合集之前问的就是它**（票 `gui-redesign/06`）：合集名是用户自己起的，
/// 「送朋友的 或 备份」「口袋(日版」都合法，可 `合集=某某` 得筛得出来。
/// 与 `facet_clause` 同一条判据、同一个函数——两处各写一遍的话，
/// 界面上说得通的名字会在「存成子库」那一步被挡下，而那时人已经建了一百个成员了。
#[must_use]
pub fn writable_value(dimension: Dimension, value: &str) -> bool {
    facet_clause(dimension, value).is_ok()
}

/// 把左栏一个一按就有的维度折成子句。
fn facet_clause(dimension: Dimension, value: &str) -> Result<Node, Unruly> {
    if value.contains(',') {
        return Err(Unruly::Comma(dimension));
    }
    // **`Clause::build` 那两道闸也得翻过来**：合集名是用户自己起的，
    // 「送朋友的 或 备份」「口袋(日版」都合法，而它们折成规则读回来就不是自己了。
    // 静默少写一条的话，子库选出来的比屏上多——那正是 `Unruly` 要防的事。
    Clause::build(dimension, Op::Is, value)
        .map(Node::Clause)
        .map_err(|_| Unruly::Unwritable(dimension))
}

/// 一次批量操作作用在**哪些行**上。
///
/// 两支的形状照 ADR-0016 那条「**规则加手动例外**」来：全选不是把一万行的键抓进内存，
/// 它就是**当前这个筛选**本身，再减去人点掉的那几行。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope<'a> {
    /// 点选的这几行。空的就是一行都没选，展开出来一个变体都没有。
    Rows(&'a [WorkAnchor]),
    /// **全选**：当前筛选下的每一行，减去点掉的这几行。
    AllExcept(&'a [WorkAnchor]),
}

/// 主列表**第一趟**（挑这一页是哪几行）的 `SELECT` 列表。
///
/// **只有身份与名字，一个聚合都不算**：`ORDER BY` 那口临时 b 树要把每一组连同它的
/// `SELECT` 列表一起搬进去排，搬得越轻越快（票 `gui-redesign/13`，实测
/// 40.6 → 17.3 毫秒）。按哪一列排就由 [`WorkOrder::select`] 多添哪一列。
///
/// **只有这一处与 [`WORK_TOTAL_COLUMNS`] 写那些别名**——`ORDER BY` 拼的就是它们。
const WORK_ANCHOR_COLUMNS: &str = concat!(
    "\
    variant.work_id AS work_id,
    CASE WHEN variant.work_id IS NULL THEN variant.key END AS loose,
    ",
    row_name!(),
    " AS name",
);

/// 主列表**第二趟**（只给这一页那几百行算聚合）的 `SELECT` 列表。
///
/// 几处值得说明的写法：
///
/// - `group_concat` **不给 `DISTINCT`**：去重与定序反正要在 Rust 那边做一遍
///   （[`platform_set`] 排序定序——`group_concat` 不保证次序，而同一份库问两次必须
///   一样），SQLite 那一侧的 `DISTINCT` 聚合要为每一组多建一棵临时 b 树，白花的。
///   平台名里没有逗号（它来自平台清单），所以逗号拆得回来。
/// - 平台未知那一档**不塞一个约定字符串进 SQL**：`group_concat` 跳过 `NULL`，
///   另数一列 `unknowns` 出来，标签在 Rust 那边补（同 [`PlatformFilter`] 的道理）。
/// - **年份不在这里**：它由 [`Catalog::fill_scraped`] 顺路带回来。
/// - `identified` 那一列是**「还没识别」与「没有候选」之间那条界线**
///   （[`WorkRow::identified`]）：`EXISTS` 一行一行答「这个变体跑过识别没有」，
///   外面套 **`MIN`** 折成「这一组**是不是全都**跑过」。**是 `MIN` 不是 `MAX`**——
///   组里剩一个没跑过，这一行就该说还没识别，理由见 [`WorkRow::identified`]。
///   走相关子查询而不是多连一张 `identification`，是因为 `FROM` 那一段
///   （[`WORK_FROM_BASE`]）是**三条查询共用的常量**（还有[数总行数](Catalog::work_total)
///   与第一趟挑行），为这一列去动它等于让另外两条也多连一张表；这个 `SELECT` 列表
///   只有这一趟用，改动就关在这儿。`identification.variant_key` 是主键，
///   两种写法都是一次索引查，快慢上不分伯仲。
const WORK_TOTAL_COLUMNS: &str = "\
    variant.work_id AS work_id,
    CASE WHEN variant.work_id IS NULL THEN variant.key END AS loose,
    COUNT(*) AS variants,
    SUM(variant.bytes) AS bytes,
    SUM(variant.unreadable) AS unreadable,
    group_concat(variant.platform) AS platforms,
    SUM(variant.platform IS NULL) AS unknowns,
    MIN(EXISTS (SELECT 1 FROM identification i WHERE i.variant_key = variant.key))
        AS identified";

/// 主列表那条查询的 `FROM` 的头一半：变体连它的作品。
///
/// 单独拆出来是给[数总行数](Catalog::work_total)用的——**那一条不必连年份那一张**：
/// 年份既不进 `WHERE` 也不进它的 `ORDER BY`，多连一张表只是白扫一遍。
pub(super) const WORK_FROM_BASE: &str = "
    FROM variant
    LEFT JOIN work ON work.id = variant.work_id";

/// 主列表那条查询的 `FROM`：变体、它的作品、以及**年份**那一列。
///
/// **只有按年份排时才用它**（票 `gui-redesign/13`）。这张 join 是一趟 `scrape_value`
/// 全表扫加一次分组（查询计划里那句 `MATERIALIZE year`），真库形状上每翻一页多花
/// 十几毫秒；而屏上画的那个年份**由 [`Catalog::fill_scraped`] 顺路带回来**——那一趟
/// 本来就在读同一张表、同一批锚点、同一批 subject。**排序需要它，画不需要它。**
///
/// 年份要能排序，所以按年份排时它必须在这条查询里，不能留到取回来之后再补。它挂的
/// 锚点两支不同——认出作品的挂**作品名**，没认出来的挂**变体的键**（与 `converge`
/// 同一条口径）。
///
/// 同一个作品的年份可以有好几条（一个源一条，三元组并存不互相覆盖）。这里的取法是
/// **裁决优先，其次取最早的那一个**：裁决排在每个字段的最前是优先级表的第一条规则
/// （ADR-0001）；而一部作品跨地区先后发行好几次，**最早的那次才是它的年份**。
///
/// 参数按出现次序绑：`裁决`、`年份`、`变体`、`作品`。
const WORK_FROM: &str = concat!(
    "
    FROM variant
    LEFT JOIN work ON work.id = variant.work_id
    LEFT JOIN (SELECT anchor, subject,
                      COALESCE(MIN(CASE WHEN source = ? THEN value END), MIN(value)) AS value
                 FROM scrape_value WHERE field = ?
                GROUP BY anchor, subject) year
           ON year.subject = ",
    row_name!(),
    "
          AND year.anchor = ",
    row_anchor!(),
);

/// 一行一个作品：认出作品的按 `work_id` 归堆，没认出来的按自己的键各成一堆。
///
/// **不能只写 `GROUP BY work_id`**：`NULL` 在 `GROUP BY` 里是同一堆，那会把真库里
/// 一多半的变体压成一行。
///
/// **「浏览屏上一行是什么」只有这一句**（ADR-0024）：数总行数、翻一页、以及
/// [移除一个根之前数「会消失几行」](super::roots::RootRemoval) 读的都是它。
/// **那一支连 `WHERE` 也是从 [`WorkQuery::where_clause`] 取的**，不自己写一份
/// 「默认收起非游戏资产」——那正是 ADR-0024 推论 1 拦的那种第二份判据。
pub(super) const WORK_GROUP_BY: &str =
    " GROUP BY variant.work_id, CASE WHEN variant.work_id IS NULL THEN variant.key END";

/// 年份那条 join 的四个参数，按 [`WORK_FROM`] 里的出现次序。
fn year_args() -> Vec<Box<dyn ToSql>> {
    vec![
        Box::new(crate::scrape::priority::VERDICT.to_string()),
        Box::new(Field::Year.label().to_string()),
        Box::new(AnchorKind::Variant.label().to_string()),
        Box::new(AnchorKind::Work.label().to_string()),
    ]
}

/// 把 `? , ? , ?` 拼出 `n` 个来。
fn placeholders(n: usize) -> String {
    std::iter::repeat_n("?", n).collect::<Vec<_>>().join(", ")
}

impl WorkQuery {
    /// 两份查询**筛的是不是同一批**（排序不算）。
    ///
    /// 界面拿它分清两种「换过了」：换**排序**只是把同一批行重排，选中的那几行一条都没变；
    /// 换**筛选**才是换了一批行，那时全选说的「当前筛出来的这一批」已经不是同一批，
    /// 留着上一批的选中会让批量操作作用到人根本没看见的行上。
    ///
    /// 判断落在这一层而不是界面里：哪几个字段是**筛选**、哪几个是**排法**，
    /// 是这个查询面自己的事（ADR-0005）。
    #[must_use]
    pub fn same_filter(&self, other: &Self) -> bool {
        // **搜索框算在筛选这一边**：它虽然只承诺排序，但它同时把没命中的行挡在外面，
        // 于是换一个搜索词换的确实是**一批行**。算进排序那一边的话，全选说的
        // 「当前筛出来的这一批」会指向人根本没看见的行。
        //
        // **比的是掐掉两头空白之后那一串**，与 [`Search::new`] 同一个口径：多打一个
        // 空格筛出来的是同一批，不该把人勾了两百行的选中清掉。
        self.search.trim() == other.search.trim()
            && self.platform == other.platform
            && self.collection == other.collection
            && self.language == other.language
            && self.chinese == other.chinese
            && self.state == other.state
            && self.rule == other.rule
            // **非游戏资产那个开关也算筛选**：拨一下，列出来的就换了一批行。
            && self.non_game_assets == other.non_game_assets
            && self.cover_only == other.cover_only
    }

    /// **当前筛选原样变成的那条规则**——「存成子库」按下去时走的就是这里。
    ///
    /// 这是这份规格里那条贯穿全局的约定落成代码的地方：**主列表的筛选就是子库的规则**。
    /// 左栏那几个一按就有的维度折成子句，手搭的那棵条件树原样接上来，合起来是一个
    /// 「全部满足」组——与屏上「各维之间是且」写的是同一句话。
    ///
    /// 一个条件都没有时是 `Ok(None)`：那不是「选不中任何东西」，是「整个库」。
    /// 拿它去建子库是不是个好主意由调用方判断（多半不是）。
    ///
    /// # Errors
    /// 屏上有条件**写不成规则**时返回它，见 [`Unruly`]。**不是少写一条就算了**：
    /// 少一条的子库选出来的比屏上多，而那正是这条约定要防的事。
    pub fn to_rule(&self) -> Result<Option<Rule>, Unruly> {
        // **排序一个字都不进去**（票 `gui-redesign/05` 的验收第 5 条）：
        // [`order`](Self::order) 与 [`descending`](Self::descending) 这里连读都不读，
        // 而搜索框既排序又收窄，收窄那一半写不成子句，所以整个挡住（挂单 Q70）。
        if Search::new(&self.search).is_some() {
            return Err(Unruly::Search);
        }
        if self.state.is_some() {
            return Err(Unruly::State);
        }
        let mut nodes: Vec<Node> = Vec::new();
        match &self.platform {
            None => {}
            Some(PlatformFilter::Unknown) => return Err(Unruly::UnknownPlatform),
            Some(PlatformFilter::Named(platform)) => {
                nodes.push(facet_clause(Dimension::Platform, platform)?);
            }
        }
        for (dimension, picked) in [
            (Dimension::Collection, &self.collection),
            (Dimension::Language, &self.language),
            (Dimension::Chinese, &self.chinese),
        ] {
            if let Some(value) = picked {
                nodes.push(facet_clause(dimension, value)?);
            }
        }
        if let Some(rule) = &self.rule {
            // 手搭的那棵树若顶层本来就是「全部满足」，摊进来而不是再套一层括号——
            // 印出来的那行字是用户要照着核对的东西，白多的括号是噪音。
            if rule.root.join == Join::All {
                nodes.extend(rule.root.nodes.iter().cloned());
            } else {
                nodes.push(Node::Group(rule.root.clone()));
            }
        }
        if nodes.is_empty() {
            return Ok(None);
        }
        // 只剩一个组时**就是那个组**，不再往外套一层「全部满足」——套了的话印出来
        // 多一对括号（`(平台=GB 或 平台=SFC)`），而这行字是用户要照着核对的东西。
        // 与上面那条「顶层本来就是全部满足就摊进来」是同一条理由。
        if nodes.len() == 1
            && let Some(Node::Group(group)) = nodes.first()
        {
            return Ok(Some(Rule::from_group(group.clone())));
        }
        Ok(Some(Rule::from_group(Group::new(Join::All, nodes))))
    }

    /// 把一条规则预填进筛选器：子库屏点「改选择」跳回浏览屏时走的那条路
    /// （票 `gui-redesign/11`）。排序不属于筛选，留给调用方自己补。
    #[must_use]
    pub fn from_rule(rule: Rule) -> Self {
        Self {
            rule: Some(rule),
            ..Self::default()
        }
    }

    /// 变体那一层的筛选。**与变体表一字不差地共用**——见结构体文档。
    fn variant_filter(&self) -> VariantQuery {
        VariantQuery {
            // 变体表那一维按**键**取子串，主列表那个是**搜索框**（三条命中路加排序），
            // 两件事不共用一个字段。搜索那一条另外补在 [`Self::where_clause`] 里。
            contains: String::new(),
            platform: self.platform.clone(),
            collection: self.collection.clone(),
            language: self.language.clone(),
            chinese: self.chinese.clone(),
            state: self.state,
            rule: self.rule.clone(),
            non_game_assets: self.non_game_assets,
            order: VariantOrder::default(),
            descending: false,
        }
    }

    /// 折出 `WHERE` 那一段与它的参数。
    ///
    /// 搜索那一条**也落在 `WHERE` 而不是 `HAVING`**：三条命中路都是逐行判得了的，
    /// 搁在 `HAVING` 里就得先把全部行分完组才筛得掉。
    pub(super) fn where_clause(&self) -> (String, Vec<Box<dyn ToSql>>) {
        let (mut sql, mut args) = self.variant_filter().where_clause();
        if let Some(search) = Search::new(&self.search) {
            let (hit_sql, mut hit_args) = search.filter();
            sql.push_str(if sql.is_empty() { " WHERE " } else { " AND " });
            sql.push_str(&hit_sql);
            args.append(&mut hit_args);
        }
        if self.cover_only {
            sql.push_str(if sql.is_empty() { " WHERE " } else { " AND " });
            sql.push_str(
                "EXISTS (SELECT 1 FROM media_ref r WHERE r.kind = '封面'
                 AND ((r.anchor = '作品' AND r.subject = work.name)
                   OR (r.anchor = '变体' AND r.subject = variant.key)))",
            );
        }
        (sql, args)
    }

    /// 复制出仅供卡片墙使用的“有封面”查询；不改变原选择集。
    #[must_use]
    pub fn with_covers_only(mut self, covers_only: bool) -> Self {
        self.cover_only = covers_only;
        self
    }

    /// 折出 `SELECT` 里那一列**匹配质量的名次**，连它的参数。没搜索时是空的。
    ///
    /// 它拼在 [`WORK_ANCHOR_COLUMNS`] 与 [`WorkOrder::select`] 后面，所以它的参数排在
    /// 整条查询的**最前面**（`SELECT` 在 `FROM` 与 `WHERE` 之前）——
    /// [`Catalog::work_page_anchors`] 按这个次序绑。
    fn rank_column(&self) -> (String, Vec<Box<dyn ToSql>>) {
        match Search::new(&self.search) {
            None => (String::new(), Vec::new()),
            Some(search) => {
                let (sql, args) = search.rank();
                (format!(", {sql}"), args)
            }
        }
    }

    /// 折出 `ORDER BY` 那一段。
    ///
    /// **搜索框打了字时，匹配质量是第一把键**，人点的那个表头退成同一档里的次序
    /// ——「匹配得好的排前面」是这个框唯一的产品承诺，让位给「按容量排」就没了。
    /// 它**永远升序**（好的在前），不跟着 [`descending`](Self::descending) 翻。
    ///
    /// **末尾一律缀上那一行的身份**（名字、`work_id`、那个键），理由与变体表同一条：
    /// 并列行的次序不定死，翻页就会漏行与重行。`(work_id, loose)` 是这张表的主键，
    /// 全序由它兜住。
    fn order_clause(&self) -> String {
        let direction = if self.descending { "DESC" } else { "ASC" };
        let column = self.order.column();
        let mut parts = Vec::new();
        if Search::new(&self.search).is_some() {
            parts.push("hit ASC".to_string());
        }
        parts.push(format!("{column} {direction}"));
        for tail in ["name", "work_id", "loose"] {
            if tail != column {
                parts.push(format!("{tail} {direction}"));
            }
        }
        format!(" ORDER BY {}", parts.join(", "))
    }
}

/// 主列表**第二趟**给一行算出来的那几样聚合（`Catalog::work_page_totals`）。
///
/// **带字段名而不是一个元组**：六样里有两个 `bool` 挨着（跑没跑过识别、是不是非游戏资产），
/// 靠位置对的话把两格写反了编译照过，屏上就对着游戏说「非游戏资产」。
struct GroupTotals {
    variants: u64,
    bytes: u64,
    unreadable_files: u64,
    platforms: Vec<String>,
    identified: bool,
    non_game_asset: bool,
    /// 这一组里**主文件**键里最小的那个。只有认不出作品的那一组用得上——那一组就是一个变体。
    main_key: Option<String>,
}

impl GroupTotals {
    /// 挑着了却算不出聚合时这一行画成什么——两趟之间有人把这一组写没了。
    ///
    /// `identified` 是**空真**（零个变体「全都跑过了」），不是 `false`：`false` 会让这一行
    /// 印出「还没识别」，对一份刚被删掉的内容指错下一步。`non_game_asset` 走「不是」：
    /// 屏上不该给一行写没了的东西挂标记。
    fn vanished() -> Self {
        Self {
            variants: 0,
            bytes: 0,
            unreadable_files: 0,
            platforms: Vec::new(),
            identified: true,
            non_game_asset: false,
            main_key: None,
        }
    }
}

/// 把 `group_concat` 那一串拆回平台集合，排好序、去掉空的。
///
/// 排序在这里做而不是 SQL 里：`group_concat` 不保证次序，而同一份库问两次必须一样。
fn platform_set(joined: Option<String>, unknowns: u64) -> Vec<String> {
    let mut out: Vec<String> = joined
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect();
    out.sort();
    out.dedup();
    if unknowns > 0 {
        out.push(UNKNOWN_PLATFORM_LABEL.to_string());
    }
    out
}

/// 把候选那一列的置信度折成一个可比的名次；认不出的当**低置信**。
///
/// 那两个词**照旧走参数**（[`confidence_rank_args`]），与这一层别处一个规矩：
/// 拼进 SQL 的只有这个文件里写死的那些字。
const CONFIDENCE_RANK: &str =
    "MIN(CASE candidate.confidence WHEN ? THEN 0 WHEN ? THEN 1 ELSE 2 END)";

/// [`CONFIDENCE_RANK`] 的两个参数，按出现次序。
fn confidence_rank_args() -> Vec<Box<dyn ToSql>> {
    vec![
        Box::new(Confidence::High.label().to_string()),
        Box::new(Confidence::Medium.label().to_string()),
    ]
}

/// 名次折回置信度。
fn confidence_of_rank(rank: i64) -> Confidence {
    match rank {
        0 => Confidence::High,
        1 => Confidence::Medium,
        _ => Confidence::Low,
    }
}

impl Catalog {
    /// 满足这个筛选条件的主列表一共几行。**滚动条的长度**由它来。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn work_total(&self, query: &WorkQuery) -> Result<u64, CatalogError> {
        let (where_sql, args) = query.where_clause();
        let sql = format!(
            "SELECT COUNT(*) FROM \
             (SELECT variant.work_id{WORK_FROM_BASE}{where_sql}{WORK_GROUP_BY})"
        );
        let count: i64 = self
            .conn
            .query_row(&sql, params_from_iter(args.iter()), |row| row.get(0))
            .map_err(|source| self.err(source))?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    /// 这份筛选下**整行都是非游戏资产**的有几行——屏上那句「收起了几个」。
    ///
    /// **开关拨在哪一档都是同一个数**：收起时它就是收起了几行，列出时它就是行上标着的
    /// 几行（[`WorkRow::non_game_asset`]）。两档之间的账是
    /// 「收起时的 [`work_total`](Self::work_total) + 这个数 = 列出时的那个」。
    ///
    /// **按行数，不按变体数**：一行算不算，看它底下（按当前筛选）的变体**是不是全都是**。
    /// 一部作品底下夹着一个被人手工挂进来的 BIOS，那一行收起时照样列着，只是变体数少一个
    /// ——它不在这个数里（挂单 `Q772`）。
    ///
    /// 与 [`work_total`](Self::work_total) 同一个 `FROM`、同一个分组，外加一句 `HAVING`：
    /// 数的是**列出那一档**的分组里，每个变体都答「是」的那几组。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn non_game_asset_rows(&self, query: &WorkQuery) -> Result<u64, CatalogError> {
        let listed = WorkQuery {
            non_game_assets: NonGameAssets::Listed,
            ..query.clone()
        };
        let (where_sql, args) = listed.where_clause();
        let sql = format!(
            "SELECT COUNT(*) FROM \
             (SELECT variant.work_id{WORK_FROM_BASE}{where_sql}{WORK_GROUP_BY} \
              HAVING MIN({NON_GAME_ASSET_FN}(variant.key)))"
        );
        let count: i64 = self
            .conn
            .query_row(&sql, params_from_iter(args.iter()), |row| row.get(0))
            .map_err(|source| self.err(source))?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    /// 取主列表的一页：从第 `offset` 行起、最多 `limit` 行，已排好序。
    ///
    /// `limit` 会被夹到 [`MAX_PAGE`]——这个入口同样不接受「把全库读出来」。
    ///
    /// **两趟**：一趟 `GROUP BY` 出这一页的骨架，再拿这一页那几十行去补
    /// 「元数据齐不齐」「年份」与「最高置信度」。这几样各要连一张大表，摊在整库上做的话
    /// 每翻一页都要多扫两遍；而它们**这一页排不到**（前两样表头上压根没有；年份只有
    /// 按年份排时才要，那时才连那张表），所以补在后面不会让这一页的次序变。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn work_page(
        &self,
        query: &WorkQuery,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<WorkRow>, CatalogError> {
        let limit = limit.min(MAX_PAGE);
        if limit == 0 {
            return Ok(Vec::new());
        }
        let picked = self.work_page_anchors(query, offset, limit)?;
        if picked.is_empty() {
            return Ok(Vec::new());
        }
        let mut out = self.work_page_totals(query, picked)?;
        self.fill_scraped(&mut out)?;
        self.fill_confidence(query, &mut out)?;
        self.fill_chinese(query, &mut out)?;
        Ok(out)
    }

    /// 同 [`Self::work_page`]，再给认出作品的那几行补上**显示标题**（[`WorkRow::display`]）。
    ///
    /// **每页多读一次标题集合**：这一页那几十个作品的叫法一趟读回来（[`Self::titles_of_works`]），
    /// 逐个作品交给 [`title::choose`](crate::title::choose) 挑——与详情面板
    /// （[`Self::variant_detail`]）、导出挑的是同一处（ADR-0024）。`priorities` 要与那两处交的是
    /// 同一份（工作目录里那份优先级表），不然屏上这一行与详情头上是两个名字。
    ///
    /// 别的格子一个字都不动：补的只有这一格，结构版本不升。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn work_page_with_titles(
        &self,
        query: &WorkQuery,
        offset: u64,
        limit: u64,
        priorities: &crate::scrape::Priorities,
    ) -> Result<Vec<WorkRow>, CatalogError> {
        let mut rows = self.work_page(query, offset, limit)?;
        self.fill_titles(&mut rows, priorities)?;
        Ok(rows)
    }

    /// **主列表上这几行**：照交进来的身份（锚点连它屏上那个名字）补齐变体数、容量、元数据齐不齐、年份、
    /// 最高置信度与显示标题——与翻页取出来的那几行一模一样（同一趟 `work_page_totals`、`fill_scraped`、
    /// `fill_confidence`，显示标题同 [`Self::work_page_with_titles`]），次序照交进来的次序。
    ///
    /// 作品详情页头上那几枚标签（「元数据：缺简介」、置信度那个词）照它印：与表上那一行是同一处算的（ADR-0024）。
    /// 按当前筛选算——这一行底下挂着几个变体、容量多大，与屏上那张表写着的是同一个数。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn work_rows(
        &self,
        query: &WorkQuery,
        anchors: &[(WorkAnchor, String)],
        priorities: &crate::scrape::Priorities,
    ) -> Result<Vec<WorkRow>, CatalogError> {
        let picked = anchors
            .iter()
            .map(|(anchor, name)| (anchor.clone(), name.clone(), None))
            .collect();
        let mut rows = self.work_page_totals(query, picked)?;
        self.fill_scraped(&mut rows)?;
        self.fill_confidence(query, &mut rows)?;
        self.fill_titles(&mut rows, priorities)?;
        Ok(rows)
    }

    /// 给认出作品的那几行补上**显示标题**（[`WorkRow::display`]）：这几个作品的叫法一趟读回来
    /// （[`Self::titles_of_works`]），逐个交给 [`title::choose`](crate::title::choose) 挑。
    fn fill_titles(
        &self,
        rows: &mut [WorkRow],
        priorities: &crate::scrape::Priorities,
    ) -> Result<(), CatalogError> {
        let works: Vec<&str> = rows
            .iter()
            .filter(|row| matches!(row.anchor, WorkAnchor::Work(_)))
            .map(|row| row.name.as_str())
            .collect();
        let mut titles = self.titles_of_works(&works)?;
        for row in rows {
            if !matches!(row.anchor, WorkAnchor::Work(_)) {
                continue;
            }
            // 一条叫法都没有：`choose` 只能退回作品名，屏上主栏本来就印它。
            let Some(entries) = titles.remove(&row.name) else {
                continue;
            };
            let chosen = crate::title::choose(
                &crate::title::TitleSet {
                    work: row.name.clone(),
                    entries,
                },
                priorities,
            );
            if chosen.display != row.name {
                row.display = Some(chosen.display);
            }
        }
        Ok(())
    }

    /// 一份详情里每个变体的**变体简称**（[`variant_short_name`]），次序与 `detail.variants` 一样。
    ///
    /// 两样输入各从它们唯一的出处取（ADR-0024）：
    ///
    /// - **是哪一种**：[`preference_for`](crate::adapter::converge::preference_for)，与首选变体那条规则
    ///   同一处；喂的是这个变体**定下来的候选**身上的中文记号与它的发行版——导出那一步喂的也是这两样。
    ///   不指名裁决，只问是哪一种。一条定下来的候选都没有、也没有发行版链接时，说不出是哪一种。
    /// - **汉化组**：导出时这个字段写进去的那一个（`converge::shown`，从这个变体自己身上的刮削值与裁决里
    ///   照优先级表挑），只在是汉化版时才问。**不从文件名剥。**
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn variant_short_names(
        &self,
        detail: &WorkDetail,
        priorities: &crate::scrape::Priorities,
    ) -> Result<Vec<String>, CatalogError> {
        use crate::adapter::converge::{Preference, shown};

        let mut out = Vec::with_capacity(detail.variants.len());
        for variant in &detail.variants {
            let key = variant.row.key.as_str();
            let Some(kind) = self.variant_kind(variant)? else {
                out.push(variant_short_name(None, None, key));
                continue;
            };
            let team = if kind == Preference::FanTranslated {
                let values = self.scraped_values(AnchorKind::Variant.label(), key)?;
                shown(
                    Field::TranslationGroup,
                    variant.row.platform.as_deref().unwrap_or_default(),
                    &[],
                    &values,
                    None,
                    priorities,
                )
                .and_then(|said| said.values.into_iter().next())
            } else {
                None
            };
            out.push(variant_short_name(Some(kind), team.as_deref(), key));
        }
        Ok(out)
    }

    /// 一个作品的**中文版本**（作品详情页「中文版本」那一格，设计稿写「汉化 / 官中」，都不是写「无」）：底下各个变体
    /// 照[首选变体](crate::adapter::converge::preference_for)那条规则判是哪一种——与变体简称（[`Self::variant_short_names`]）
    /// 同一处判，不另判——有汉化版就是汉化，没有汉化版、有官中版就是官中，汉化压过官中也是那条规则的次序。
    ///
    /// **问规则时不带裁决**：首选变体被人指定时，那个变体在首选规则里排「裁决」那一档，可它是不是汉化版不因此改变。
    /// 一个中文的都没有是 `None`；认不出作品的那一行问的就是它自己那一个变体。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn work_chinese_mark(
        &self,
        detail: &WorkDetail,
    ) -> Result<Option<crate::dat::chinese::ChineseMark>, CatalogError> {
        use crate::adapter::converge::Preference;
        use crate::dat::chinese::ChineseMark;

        let mut best: Option<Preference> = None;
        for variant in &detail.variants {
            if let Some(kind @ (Preference::FanTranslated | Preference::OfficialChinese)) =
                self.variant_kind(variant)?
            {
                best = Some(best.map_or(kind, |best| best.min(kind)));
            }
        }
        Ok(best.map(|kind| match kind {
            Preference::FanTranslated => ChineseMark::FanTranslated,
            _ => ChineseMark::Official,
        }))
    }

    /// 一个变体照[首选变体](crate::adapter::converge::preference_for)那条规则算是**哪一种**（汉化 / 官中 / 日版 / 其他），
    /// **不带裁决**：被人指成首选的汉化版照样答汉化。一条定下来的候选都没有、也没有发行版链接时说不出是哪一种，
    /// 交回 `None`。
    ///
    /// 中文记号只读**定下来**的候选（与导出那一步同一条路）；变体简称、作品的中文版本、作品详情页上哪几个变体摆
    /// 汉化组那一行，都从这儿问。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn variant_kind(
        &self,
        variant: &WorkVariant,
    ) -> Result<Option<crate::adapter::converge::Preference>, CatalogError> {
        let settled = variant
            .candidates
            .iter()
            .any(|candidate| candidate.accepted);
        let release = match variant.row.release_id {
            Some(id) => self.release(id)?,
            None => None,
        };
        if !settled && release.is_none() {
            return Ok(None);
        }
        let marks: std::collections::BTreeSet<crate::dat::chinese::ChineseMark> = variant
            .candidates
            .iter()
            .filter(|candidate| candidate.accepted)
            .filter_map(|candidate| candidate.chinese)
            .collect();
        Ok(Some(crate::adapter::converge::preference_for(
            &variant.row.key,
            &marks,
            release.as_ref(),
            None,
        )))
    }

    /// 第一趟：**这一页是哪几行、按什么次序**。
    ///
    /// 只挑得出身份就够了，聚合一列都不算——那正是这一趟便宜的原因：`ORDER BY` 那口
    /// 临时 b 树要把每一组连同它的聚合结果一起搬进去排，而搬的若只是
    /// 「`work_id` + 那个键 + 名字」，同一份数据上实测 **40.6 → 17.3 毫秒**
    /// （票 `gui-redesign/13`）。
    ///
    /// 例外是**按哪一列排就多算哪一列**（[`WorkOrder::select`]）：按容量排就得有
    /// `SUM(bytes)`，不然排不出来。一次只多算一列，不是十列。
    fn work_page_anchors(
        &self,
        query: &WorkQuery,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<(WorkAnchor, String, Option<SearchHit>)>, CatalogError> {
        let (rank_sql, rank_args) = query.rank_column();
        // 搜索着没有。**这一个布尔量决定下面那一列取不取**，而不是拿
        // 「取不到就算了」去猜——那样「没搜索」与「这一列读不出来」会混成一档。
        let searching = !rank_sql.is_empty();
        // **年份那张表只在按年份排时才连**（票 `gui-redesign/13`）：它是一趟
        // `scrape_value` 全表扫加一次分组，而画出来的那个年份由 [`Self::fill_scraped`]
        // 顺路带回来——排序需要它，画不需要它。
        let by_year = query.order == WorkOrder::Year;
        let (from_sql, year_binds) = if by_year {
            (WORK_FROM, year_args())
        } else {
            (WORK_FROM_BASE, Vec::new())
        };
        let order_column = query.order.select();
        let (where_sql, where_args) = query.where_clause();
        let order_sql = query.order_clause();
        // **参数按它们在这条 SQL 里出现的次序绑**：名次那一列在 `SELECT` 里，
        // 年份那张 join 在 `FROM` 里，筛选在 `WHERE` 里，翻页在最后。
        let mut args = rank_args;
        args.extend(year_binds);
        args.extend(where_args);
        args.push(Box::new(i64::try_from(limit).unwrap_or(i64::MAX)));
        args.push(Box::new(i64::try_from(offset).unwrap_or(i64::MAX)));
        let sql = format!(
            "SELECT {WORK_ANCHOR_COLUMNS}{order_column}{rank_sql}{from_sql}{where_sql}\
             {WORK_GROUP_BY}{order_sql} LIMIT ? OFFSET ?"
        );
        let mut statement = self.conn.prepare(&sql).map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params_from_iter(args.iter()), |row| {
                let work_id: Option<i64> = row.get(0)?;
                let loose: Option<String> = row.get(1)?;
                // 名次那一列**按名字取**：写死一个下标的话，往
                // [`WORK_ANCHOR_COLUMNS`] 里插一列就会静默指到别人身上。没搜索时
                // 它压根不在 `SELECT` 里，那时一次都不问。
                let hit = if searching {
                    SearchHit::from_rank(row.get::<_, i64>("hit")?)
                } else {
                    None
                };
                Ok((
                    match (work_id, loose) {
                        (Some(id), _) => WorkAnchor::Work(id),
                        (None, Some(key)) => WorkAnchor::Loose(key),
                        // 分组键的两支必有其一；真到不了这里，兜个不会撞上的值。
                        (None, None) => WorkAnchor::Loose(String::new()),
                    },
                    row.get::<_, String>(2)?,
                    hit,
                ))
            })
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 第二趟：**只给这一页那几百行算聚合**——变体数、容量、读不到的成员、平台集合。
    ///
    /// **筛的是同一批变体**：这一行底下挂着几个变体是**按当前筛选算**的，屏上写的、
    /// 详情面板列的、批量操作动的是同一个数（票 `gui-redesign/03`）。所以这一趟原样
    /// 用那六维加那棵条件树（[`WorkQuery::variant_filter`]）。摊到全库上算的话每翻一页
    /// 都要给一万多组各算一遍，而其中一万组当场就被 `LIMIT` 扔了。
    ///
    /// **搜索那三条不带**，而这不是漏了一条：它们**组内恒定**——三条读的都是
    /// `work.name` 或者没认出作品时那个变体自己的键，也就是这一组的身份本身
    /// （见 [`Search::filter`]）。第一趟已经按它挑过了，这一趟再判一遍，
    /// 每一行的答案都一样，只是白扫一遍（实测那一遍要 36 毫秒）。
    /// 反过来说，若哪天有一条搜索路变成逐行不同的，屏上「变体数」就会跟着搜索词变
    /// ——那正是 [`Search::description`] 挑锚点时挡下的事，也是
    /// 「靠简介命中的那一行变体数一个都不少」那条测试钉住的事。
    ///
    /// 出来的次序照**第一趟**排好的那个，不看这一趟的：这一趟是按身份查回来的，
    /// 它自己没有次序。
    fn work_page_totals(
        &self,
        query: &WorkQuery,
        picked: Vec<(WorkAnchor, String, Option<SearchHit>)>,
    ) -> Result<Vec<WorkRow>, CatalogError> {
        // 一行都没挑着就没什么可算的。**这一句同时是下面那段 SQL 的前提**：
        // 两支都空的话拼出来的是 `AND ()`，那不是一条读得懂的 SQL。
        if picked.is_empty() {
            return Ok(Vec::new());
        }
        let works: Vec<i64> = picked
            .iter()
            .filter_map(|(anchor, _, _)| match anchor {
                WorkAnchor::Work(id) => Some(*id),
                WorkAnchor::Loose(_) => None,
            })
            .collect();
        let loose: Vec<&str> = picked
            .iter()
            .filter_map(|(anchor, _, _)| match anchor {
                WorkAnchor::Loose(key) => Some(key.as_str()),
                WorkAnchor::Work(_) => None,
            })
            .collect();
        let (where_sql, where_args) = query.variant_filter().where_clause();
        // 两支各挑各的：认出作品的按 `work_id`，没认出来的按它自己的键。
        // **空的那一支不写进去**——`IN ()` 恒不成立，写了只是让计划多一支。
        let mut branches: Vec<String> = Vec::new();
        if !works.is_empty() {
            branches.push(format!(
                "variant.work_id IN ({})",
                placeholders(works.len())
            ));
        }
        if !loose.is_empty() {
            branches.push(format!(
                "(variant.work_id IS NULL AND variant.key IN ({}))",
                placeholders(loose.len())
            ));
        }
        let sql = format!(
            // **行上那个「非游戏资产」标记也在这一趟折**：只给这一页那几百行问，
            // `MIN` 折成「这一组是不是全都是」，与 [`Catalog::non_game_asset_rows`]
            // 那句 `HAVING` 是同一个口径。
            //
            // **主文件键也在这一趟带回来**：认不出作品的那一行要从它剥正题（`WorkRow::title`），
            // 那一组就是一个变体，`MIN` 只是个取值器。
            "SELECT {WORK_TOTAL_COLUMNS},
                    MIN({NON_GAME_ASSET_FN}(variant.key)) AS non_game_asset,
                    MIN(variant.main_key) AS main_key\
             {WORK_FROM_BASE}{where_sql}{glue} ({branch})\
             {WORK_GROUP_BY}",
            glue = if where_sql.is_empty() {
                " WHERE"
            } else {
                " AND"
            },
            branch = branches.join(" OR "),
        );
        let mut args = where_args;
        for id in &works {
            args.push(Box::new(*id));
        }
        for key in &loose {
            args.push(Box::new((*key).to_string()));
        }
        let mut statement = self.conn.prepare(&sql).map_err(|source| self.err(source))?;
        let found = statement
            .query_map(params_from_iter(args.iter()), |row| {
                let work_id: Option<i64> = row.get(0)?;
                let key: Option<String> = row.get(1)?;
                let anchor = match (work_id, key) {
                    (Some(id), _) => WorkAnchor::Work(id),
                    (None, Some(key)) => WorkAnchor::Loose(key),
                    (None, None) => WorkAnchor::Loose(String::new()),
                };
                let unknowns = u64::try_from(row.get::<_, i64>(6)?).unwrap_or(0);
                Ok((
                    anchor,
                    GroupTotals {
                        variants: u64::try_from(row.get::<_, i64>(2)?).unwrap_or(0),
                        bytes: u64::try_from(row.get::<_, i64>(3)?).unwrap_or(0),
                        unreadable_files: u64::try_from(row.get::<_, i64>(4)?).unwrap_or(0),
                        platforms: platform_set(row.get(5)?, unknowns),
                        identified: row.get::<_, i64>(7)? != 0,
                        // 按名字取：这一列是拼在常量后头的，下标跟着常量变。
                        non_game_asset: row.get::<_, i64>("non_game_asset")? != 0,
                        main_key: row.get("main_key")?,
                    },
                ))
            })
            .map_err(|source| self.err(source))?;
        let mut totals: std::collections::BTreeMap<WorkAnchor, GroupTotals> =
            std::collections::BTreeMap::new();
        for row in found {
            let (anchor, total) = row.map_err(|source| self.err(source))?;
            totals.insert(anchor, total);
        }
        Ok(picked
            .into_iter()
            .map(|(anchor, name, hit)| {
                // 挑着了却算不出聚合，只有一种可能：**两趟之间有人把这一组写没了**
                // （扫描跑在另一条连接上，WAL 允许一写多读）。那时这一行画成
                // 「0 个变体」——下一次读库就没有它了。同一份暴露面
                // [`Self::fill_scraped`] 与 [`Self::fill_confidence`] 本来就有：
                // 这一层从来不是一条 SQL 出一整页。
                //
                // **那几格不走 `Default`**（`identified` 的默认是 `false`，也就是
                // 「还有变体没跑过识别」）：一组零个变体，「全都跑过了」是**空真**，
                // 而 `false` 会让这一行印出「还没识别」——对一份刚被删掉的内容说
                // 「先跑一趟 `romcat identify`」，是这一票专门要消灭的那种指错下一步。
                // 兜底的那一份见 [`GroupTotals::vanished`]。
                let sums = totals.remove(&anchor).unwrap_or_else(GroupTotals::vanished);
                // 认出作品的那一组里挂着一堆变体，它们的主文件键谁也代表不了这一行。
                let main_key = match anchor {
                    WorkAnchor::Loose(_) => sums.main_key,
                    WorkAnchor::Work(_) => None,
                };
                WorkRow {
                    anchor,
                    name,
                    // 挑显示标题要优先级表，这一趟手上没有（`work_page_with_titles` 补）。
                    display: None,
                    main_key,
                    platforms: sums.platforms,
                    variants: sums.variants,
                    bytes: sums.bytes,
                    unreadable_files: sums.unreadable_files,
                    identified: sums.identified,
                    hit,
                    non_game_asset: sums.non_game_asset,
                    // 这三样下面补。
                    year: None,
                    missing: WORK_FIELDS.to_vec(),
                    confidence: None,
                    chinese: Vec::new(),
                }
            })
            .collect())
    }

    /// 补上这一页每一行「**元数据齐不齐**」与那一列**年份**。
    ///
    /// 锚点两支分开问：认出作品的看**作品**锚点，没认出来的看**变体**锚点——
    /// 与 `converge` 挑值时走的是同一条岔路，于是屏上写着「齐」的那一行，导出时
    /// 真的填得满。
    ///
    /// **年份跟这一趟一起回来**（票 `gui-redesign/13`）：它本来是主查询里一张
    /// `LEFT JOIN`，而那张 join 是一趟 `scrape_value` 全表扫加一次分组——每翻一页都做
    /// 一遍。可这一趟读的**是同一张表、同一批锚点、同一批 subject**，年份又已经在
    /// [`WORK_FIELDS`] 里，多取一列值就有了。取法与那张 join 一字不差：
    /// **裁决优先，其次取最早的那一个**（ADR-0001；一部作品跨地区先后发行好几次，
    /// 最早的那次才是它的年份）。
    ///
    /// 按年份排时主查询照旧连那张表——`ORDER BY` 要有个 `year` 可指——但**画出来的
    /// 那一个一律是这里补的这个**：两处取法相同，屏上不会因为换了排序就换个年份。
    fn fill_scraped(&self, rows: &mut [WorkRow]) -> Result<(), CatalogError> {
        for anchor in [AnchorKind::Work, AnchorKind::Variant] {
            let subjects: Vec<&str> = rows
                .iter()
                .map(|row| row.anchor.scrape_anchor(&row.name))
                .filter(|(kind, _)| *kind == anchor)
                .map(|(_, subject)| subject)
                .collect();
            if subjects.is_empty() {
                continue;
            }
            let sql = format!(
                "SELECT subject, field,
                        COALESCE(MIN(CASE WHEN source = ? THEN value END), MIN(value)) AS value
                   FROM scrape_value
                  WHERE anchor = ? AND field IN ({}) AND subject IN ({})
                  GROUP BY subject, field",
                placeholders(WORK_FIELDS.len()),
                placeholders(subjects.len()),
            );
            let mut args: Vec<Box<dyn ToSql>> = vec![
                Box::new(crate::scrape::priority::VERDICT.to_string()),
                Box::new(anchor.label().to_string()),
            ];
            for field in WORK_FIELDS {
                args.push(Box::new(field.label().to_string()));
            }
            for subject in &subjects {
                args.push(Box::new((*subject).to_string()));
            }
            let mut statement = self.conn.prepare(&sql).map_err(|source| self.err(source))?;
            let found = statement
                .query_map(params_from_iter(args.iter()), |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })
                .map_err(|source| self.err(source))?;
            let mut have: std::collections::BTreeSet<(String, String)> =
                std::collections::BTreeSet::new();
            let mut years: std::collections::BTreeMap<String, String> =
                std::collections::BTreeMap::new();
            for row in found {
                let (subject, field, value) = row.map_err(|source| self.err(source))?;
                if field == Field::Year.label()
                    && let Some(value) = value
                {
                    years.insert(subject.clone(), value);
                }
                have.insert((subject, field));
            }
            for row in rows
                .iter_mut()
                .filter(|row| row.anchor.scrape_anchor(&row.name).0 == anchor)
            {
                row.missing = WORK_FIELDS
                    .into_iter()
                    .filter(|field| !have.contains(&(row.name.clone(), field.label().to_string())))
                    .collect();
                row.year = years.get(&row.name).cloned();
            }
        }
        Ok(())
    }

    /// 补上这一页每一行的**最高置信度**。
    ///
    /// 算的是**当前筛选下**那些变体上的候选：屏上那一行写着几个变体，这一档就是那几个
    /// 变体里最有把握的那条结论。一条候选都没有就留 `None`——那不是「撞过没撞上」
    /// （ADR-0002）；那一行印**没有候选**还是**还没识别**，由
    /// [`WorkRow::identified`] 那一列分辨，不由这一趟说。
    /// 补上这一页每行已采纳候选带着的中文身份；显示层不从标题或文件名猜。
    fn fill_chinese(&self, query: &WorkQuery, rows: &mut [WorkRow]) -> Result<(), CatalogError> {
        let works: Vec<i64> = rows
            .iter()
            .filter_map(|row| match row.anchor {
                WorkAnchor::Work(id) => Some(id),
                WorkAnchor::Loose(_) => None,
            })
            .collect();
        let loose: Vec<&str> = rows
            .iter()
            .filter_map(|row| match &row.anchor {
                WorkAnchor::Loose(key) => Some(key.as_str()),
                WorkAnchor::Work(_) => None,
            })
            .collect();
        let mut by_work =
            std::collections::BTreeMap::<i64, std::collections::BTreeSet<String>>::new();
        if !works.is_empty() {
            let (where_sql, mut args) = query.where_clause();
            let sql = format!(
                "SELECT variant.work_id, candidate.chinese FROM variant
                   JOIN candidate ON candidate.variant_key = variant.key
                   LEFT JOIN work ON work.id = variant.work_id
                  {where_sql}{glue} candidate.accepted <> 0
                    AND candidate.chinese IS NOT NULL
                    AND variant.work_id IN ({ids})
                  GROUP BY variant.work_id, candidate.chinese",
                glue = if where_sql.is_empty() {
                    " WHERE"
                } else {
                    " AND"
                },
                ids = placeholders(works.len()),
            );
            args.extend(works.iter().map(|id| Box::new(*id) as Box<dyn ToSql>));
            let mut statement = self.conn.prepare(&sql).map_err(|source| self.err(source))?;
            for found in statement
                .query_map(params_from_iter(args.iter()), |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|source| self.err(source))?
            {
                let (id, mark) = found.map_err(|source| self.err(source))?;
                by_work.entry(id).or_default().insert(mark);
            }
        }
        let mut by_key =
            std::collections::BTreeMap::<String, std::collections::BTreeSet<String>>::new();
        if !loose.is_empty() {
            let sql = format!(
                "SELECT candidate.variant_key, candidate.chinese FROM candidate
                 WHERE candidate.accepted <> 0 AND candidate.chinese IS NOT NULL
                   AND candidate.variant_key IN ({keys})
                 GROUP BY candidate.variant_key, candidate.chinese",
                keys = placeholders(loose.len()),
            );
            let args: Vec<Box<dyn ToSql>> = loose
                .iter()
                .map(|key| Box::new((*key).to_owned()) as Box<dyn ToSql>)
                .collect();
            let mut statement = self.conn.prepare(&sql).map_err(|source| self.err(source))?;
            for found in statement
                .query_map(params_from_iter(args.iter()), |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|source| self.err(source))?
            {
                let (key, mark) = found.map_err(|source| self.err(source))?;
                by_key.entry(key).or_default().insert(mark);
            }
        }
        for row in rows {
            row.chinese = match &row.anchor {
                WorkAnchor::Work(id) => by_work.remove(id),
                WorkAnchor::Loose(key) => by_key.remove(key),
            }
            .unwrap_or_default()
            .into_iter()
            .collect();
        }
        Ok(())
    }

    fn fill_confidence(&self, query: &WorkQuery, rows: &mut [WorkRow]) -> Result<(), CatalogError> {
        let works: Vec<i64> = rows
            .iter()
            .filter_map(|row| match row.anchor {
                WorkAnchor::Work(id) => Some(id),
                WorkAnchor::Loose(_) => None,
            })
            .collect();
        let loose: Vec<&str> = rows
            .iter()
            .filter_map(|row| match &row.anchor {
                WorkAnchor::Loose(key) => Some(key.as_str()),
                WorkAnchor::Work(_) => None,
            })
            .collect();
        let mut by_work: std::collections::BTreeMap<i64, Confidence> =
            std::collections::BTreeMap::new();
        if !works.is_empty() {
            let (where_sql, where_args) = query.where_clause();
            let sql = format!(
                "SELECT variant.work_id, {CONFIDENCE_RANK}
                   FROM variant
                   JOIN candidate ON candidate.variant_key = variant.key
                   LEFT JOIN work ON work.id = variant.work_id
                  {where_sql}{glue} variant.work_id IN ({ids})
                  GROUP BY variant.work_id",
                glue = if where_sql.is_empty() {
                    " WHERE"
                } else {
                    " AND"
                },
                ids = placeholders(works.len()),
            );
            let mut args = confidence_rank_args();
            args.extend(where_args);
            for id in &works {
                args.push(Box::new(*id));
            }
            let mut statement = self.conn.prepare(&sql).map_err(|source| self.err(source))?;
            let found = statement
                .query_map(params_from_iter(args.iter()), |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
                })
                .map_err(|source| self.err(source))?;
            for row in found {
                let (id, rank) = row.map_err(|source| self.err(source))?;
                by_work.insert(id, confidence_of_rank(rank));
            }
        }
        let mut by_key: std::collections::BTreeMap<String, Confidence> =
            std::collections::BTreeMap::new();
        if !loose.is_empty() {
            // 没认出作品的那些行本来就是一个变体一行，页上那几个键已经是筛过的，
            // 不必再把筛选条件带一遍。
            let sql = format!(
                "SELECT candidate.variant_key, {CONFIDENCE_RANK}
                   FROM candidate WHERE candidate.variant_key IN ({keys})
                  GROUP BY candidate.variant_key",
                keys = placeholders(loose.len()),
            );
            let mut args = confidence_rank_args();
            args.extend(
                loose
                    .iter()
                    .map(|key| Box::new((*key).to_string()) as Box<dyn ToSql>),
            );
            let mut statement = self.conn.prepare(&sql).map_err(|source| self.err(source))?;
            let found = statement
                .query_map(params_from_iter(args.iter()), |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })
                .map_err(|source| self.err(source))?;
            for row in found {
                let (key, rank) = row.map_err(|source| self.err(source))?;
                by_key.insert(key, confidence_of_rank(rank));
            }
        }
        for row in rows {
            row.confidence = match &row.anchor {
                WorkAnchor::Work(id) => by_work.get(id).copied(),
                WorkAnchor::Loose(key) => by_key.get(key).copied(),
            };
        }
        Ok(())
    }

    /// 一次批量操作**作用于哪些变体**。
    ///
    /// 这是这一票要钉死的那半条选中语义：**选中主列表的行 ＝ 选中这些作品，批量操作
    /// 作用于它们的变体**。另半条（在详情面板里选中某一个变体，变体级操作只作用于它）
    /// 不必经过这里——那时手上就是一个键。
    ///
    /// 作用范围随**当前筛选**收窄，与屏上那一行写着的变体数是同一个数。不筛的时候
    /// 它就是那些作品的**全部**变体。
    ///
    /// 它会把这一批的键整个列出来——**批量操作必须列得出作用范围才谈得上作用范围**。
    /// 浏览那一半照旧只有视口那几十行，两件事不共用一条路。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn scoped_variants(
        &self,
        query: &WorkQuery,
        scope: Scope<'_>,
    ) -> Result<Vec<String>, CatalogError> {
        let Some((sql, args)) = scoped_sql(query, scope, "variant.key") else {
            return Ok(Vec::new());
        };
        let mut statement = self.conn.prepare(&sql).map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params_from_iter(args.iter()), |row| row.get::<_, String>(0))
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 这份作用范围里有多少个变体。
    ///
    /// 与 [`scoped_variants`](Self::scoped_variants) 分开：屏上那句「作用于多少个变体」
    /// 每帧都要，而它只要一个数——把四万个键读回来只为数一遍，正是这一层要防的事。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn scoped_variant_total(
        &self,
        query: &WorkQuery,
        scope: Scope<'_>,
    ) -> Result<u64, CatalogError> {
        let Some((sql, args)) = scoped_sql(query, scope, "COUNT(*)") else {
            return Ok(0);
        };
        let count: i64 = self
            .conn
            .query_row(&sql, params_from_iter(args.iter()), |row| row.get(0))
            .map_err(|source| self.err(source))?;
        Ok(u64::try_from(count).unwrap_or(0))
    }
}

/// 折出「这份作用范围里的变体」那条查询；一行都没选中时是 `None`。
///
/// `pick` 是要取的那一列（键，或者一个 `COUNT(*)`）——两处共用一条 `WHERE`，
/// 免得「屏上说作用于多少个」与「按下去真动了多少个」漂开。
fn scoped_sql(
    query: &WorkQuery,
    scope: Scope<'_>,
    pick: &str,
) -> Option<(String, Vec<Box<dyn ToSql>>)> {
    {
        let (where_sql, where_args) = query.where_clause();
        let (anchors, negated) = match scope {
            Scope::Rows(anchors) => (anchors, false),
            Scope::AllExcept(anchors) => (anchors, true),
        };
        if anchors.is_empty() && !negated {
            return None;
        }
        let works: Vec<i64> = anchors
            .iter()
            .filter_map(|anchor| match anchor {
                WorkAnchor::Work(id) => Some(*id),
                WorkAnchor::Loose(_) => None,
            })
            .collect();
        let loose: Vec<&str> = anchors
            .iter()
            .filter_map(|anchor| match anchor {
                WorkAnchor::Loose(key) => Some(key.as_str()),
                WorkAnchor::Work(_) => None,
            })
            .collect();
        let mut args: Vec<Box<dyn ToSql>> = where_args;
        let mut sql = format!(
            "SELECT {pick} FROM variant LEFT JOIN work ON work.id = variant.work_id{where_sql}"
        );
        if !anchors.is_empty() {
            let mut parts: Vec<String> = Vec::new();
            if !works.is_empty() {
                // `work_id IN (…)` 在 `work_id` 为 `NULL` 时求值出 `NULL`，而
                // `NOT NULL` 还是 `NULL`——不裹这一层，全选减例外会把**还没认出作品**
                // 的那些行整批漏掉（真库上那是一多半）。
                parts.push(format!(
                    "COALESCE(variant.work_id IN ({}), 0)",
                    placeholders(works.len()),
                ));
                for id in &works {
                    args.push(Box::new(*id));
                }
            }
            if !loose.is_empty() {
                parts.push(format!(
                    "(variant.work_id IS NULL AND variant.key IN ({}))",
                    placeholders(loose.len()),
                ));
                for key in &loose {
                    args.push(Box::new((*key).to_string()));
                }
            }
            sql.push_str(if where_sql.is_empty() {
                " WHERE "
            } else {
                " AND "
            });
            if negated {
                sql.push_str("NOT ");
            }
            sql.push_str(&format!("({})", parts.join(" OR ")));
        }
        if pick == "variant.key" {
            sql.push_str(" ORDER BY variant.key");
        }
        Some((sql, args))
    }
}

/// 主列表点开一行之后，详情面板里那一个变体。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkVariant {
    /// 变体本身。
    pub row: VariantRow,
    /// 这一轮的识别结论；压根没识别过时是 `None`。
    pub state: Option<State>,
    /// 没定下来的话，**为什么**。
    pub reason: Option<String>,
    /// 全部候选，**每条带着置信度与依据**。没有依据的候选事后无法复核（ADR-0002）。
    pub candidates: Vec<Candidate>,
}

impl WorkVariant {
    /// 这个变体是不是**非游戏资产**（ADR-0010）。
    ///
    /// 判断只有 [`classify::non_game_asset`](crate::classify::non_game_asset) 那一处
    /// （ADR-0024），这里传变体的键去问它——与导出那道闸（`adapter::converge`）问的是
    /// 同一个输入。界面照着标，不自己判。
    #[must_use]
    pub fn non_game_asset(&self) -> bool {
        crate::classify::non_game_asset(&self.row.key)
    }

    /// 这个变体最高的那档置信度；一条候选都没有时是 `None`——**没有候选**或者
    /// **还没识别**，哪一个由 [`state`](Self::state) 分辨。
    #[must_use]
    pub fn confidence(&self) -> Option<Confidence> {
        self.candidates
            .iter()
            .map(|candidate| candidate.confidence)
            .min()
    }

    /// **定下来的**那条候选（自动通过或裁决接受的）；一条都没定下来是 `None`。
    ///
    /// 作品详情页「发行版」那一格印它撞上的 DAT 条目名（拿主意的人 2026-09-15 定）：挑哪一条由这里答，界面不自己挑（ADR-0024）。
    #[must_use]
    pub fn accepted_candidate(&self) -> Option<&Candidate> {
        self.candidates.iter().find(|candidate| candidate.accepted)
    }

    /// **判定依据那一句**：照依据形状各段排（[`Shape::basis`](crate::triage::Shape::basis)）——来源 / DAT / 哈希口径 ·
    /// 头一条候选自己的依据 · 候选数；一条候选都没有是 `None`（那时屏上照 [`Self::no_candidate_hint`] 说）。
    ///
    /// 头一条就是最可信的那一条：中立库交回候选的次序就是按可信程度排的。
    #[must_use]
    pub fn basis_line(&self) -> Option<String> {
        let lead = self.candidates.first()?;
        crate::triage::Shape::of_candidates(&self.candidates)
            .map(|shape| shape.basis(&lead.evidence, self.candidates.len()))
    }

    /// **判定依据**摆哪一条候选：置信度最高那一档（[`Self::confidence`]）里的头一条；一条候选都没有是 `None`。
    ///
    /// 作品详情页「识别依据」那一面上「判定依据：」后头跟的就是它，界面不自己挑（ADR-0024）。
    #[must_use]
    pub fn best_candidate(&self) -> Option<&Candidate> {
        let best = self.confidence()?;
        self.candidates
            .iter()
            .find(|candidate| candidate.confidence == best)
    }

    /// 详情面板里这一行印哪个词。与主列表那一栏
    /// （[`WorkRow::confidence_label`]）同一条口径，只是这里手上就是一个变体，
    /// 「跑过没跑过」直接由 [`state`](Self::state) 说：一行结论都没有就是
    /// **还没识别**，有结论而一条候选都没有就是**没有候选**。
    ///
    /// **词落在核心库里**（ADR-0005）：界面只把它印出来，不自己判这一格该说哪个词。
    ///
    /// 判的次序与主列表那一栏一字不差：**先看有没有候选**，一条都没有才去问跑没跑过。
    /// 反过来先问跑没跑过的话，一个「没有结论行、却有候选」的变体（真机上写不出来
    /// ——两张表同进同出——但这一层收的是两个独立的字段）会被印成还没识别，
    /// 而它的候选就摆在旁边的悬停里。
    #[must_use]
    pub fn confidence_label(&self) -> &'static str {
        let confidence = self.confidence();
        if confidence.is_none() && self.state.is_none() {
            return NOT_RUN_LABEL;
        }
        Tier::of(confidence).label()
    }

    /// 一条候选都没有时，**该说哪一句、指向哪一步**；有候选就是 `None`（不必说）。
    ///
    /// 与 [`confidence_label`](Self::confidence_label) 是同一条判据的两面：那边给一个词，
    /// 这边给一句话。**两处必须同进同出**，所以判据只写在这一处——界面照着印就行
    /// （ADR-0005）。从前这一句是界面自己 `if variant.state.is_none()` 判出来的，
    /// 而哪天判据变了（比如再加上「这条结论是不是这一份字节的」），核心改了、
    /// 界面没跟上，屏上那句「先跑一趟 `romcat identify`」就会指错，
    /// 而没有一条编译错误会说话。
    #[must_use]
    pub fn no_candidate_hint(&self) -> Option<&'static str> {
        if !self.candidates.is_empty() {
            return None;
        }
        Some(if self.state.is_none() {
            "连识别都还没跑过——那是还没识别，不是「撞过没撞上」。\
             先跑一趟 `romcat identify`。"
        } else {
            "识别跑过了，一条候选都没有——那是没有候选，不是「撞过没撞上」。"
        })
    }
}

/// **变体简称**（`CONTEXT.md` 同名词条，2026-09-14 拿主意的人定）：屏上一个变体给人认的那几个字。
///
/// - `kind` 是这个变体**是哪一种**，由 [`preference_for`](crate::adapter::converge::preference_for)
///   判（与首选变体那条规则同一处，见 [`Catalog::variant_short_names`]）；**说不出是哪一种时是
///   `None`**——那时退回文件名。
/// - `team` 是导出时写进去的那个**汉化组**（刮削来的值或裁决），不是从文件名剥的。
///
/// 汉化版说得出汉化组是「汉化版 · 汉化组」，说不出（或者只是一串空白）只写「汉化版」；官中版是
/// 「官中版」，不跟汉化组；其余一律「原版」。
#[must_use]
pub fn variant_short_name(
    kind: Option<crate::adapter::converge::Preference>,
    team: Option<&str>,
    key: &str,
) -> String {
    use crate::adapter::converge::Preference;

    match kind {
        None => crate::path::file_name_of_key(key).to_string(),
        Some(Preference::FanTranslated) => {
            match team.map(str::trim).filter(|team| !team.is_empty()) {
                Some(team) => format!("汉化版 · {team}"),
                None => "汉化版".to_string(),
            }
        }
        Some(Preference::OfficialChinese) => "官中版".to_string(),
        Some(Preference::Verdict | Preference::Japanese | Preference::Other) => "原版".to_string(),
    }
}

/// 主列表点开一行之后，详情面板上摆的那一份。
///
/// 三层里的头两层（**作品** → **变体**）在这儿；第三层**文件**跟着选中的那个变体走
/// （[`Catalog::variant_members`](Catalog::variant_members)），因为一个变体可以是
/// 一整个目录，真库上最大的一份底下有 21,436 个文件——不选中就整份读出来，
/// 点一行的代价会跟着最大的那个变体走。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkDetail {
    /// 这一行是谁。
    pub anchor: WorkAnchor,
    /// 画出来的那个名字。
    pub name: String,
    /// 年份；一条都没刮到时是 `None`。
    pub year: Option<String>,
    /// 平台集合。
    pub platforms: Vec<String>,
    /// 底下挂着的全部变体（按当前筛选），按键排。
    pub variants: Vec<WorkVariant>,
}

impl WorkDetail {
    /// 点开的是**认不出作品的那一行**时，它的**正题**；点开的是一个作品时是 `None`。
    ///
    /// 与主列表那一行（[`WorkRow::title`]）同一处剥：侧边详情头上写的，就是表上那一行主栏
    /// 写的。认不出作品的那一行底下只有它自己那一个变体，正题从那个变体的主文件名剥。
    #[must_use]
    pub fn title(&self, rules: &crate::filename::Rules) -> Option<String> {
        if !matches!(self.anchor, WorkAnchor::Loose(_)) {
            return None;
        }
        let variant = self.variants.first()?;
        Some(loose_title(rules, &variant.row.main_key))
    }
}

/// 认不出作品的那一行的**正题**：从它主文件名剥（[`Rules::parse_main_key`](crate::filename::Rules::parse_main_key)），
/// 剥完一个字都不剩时退回主文件名。
///
/// [`WorkRow::title`] 与 [`WorkDetail::title`] 都走这一处——「剥完空了怎么办」只说一次。
fn loose_title(rules: &crate::filename::Rules, main_key: &str) -> String {
    let parsed = rules.parse_main_key(main_key);
    if parsed.title.trim().is_empty() {
        crate::path::file_name_of_key(main_key).to_string()
    } else {
        parsed.title
    }
}

impl Catalog {
    /// 主列表某一行的详情；这一行在当前筛选下一个变体都不剩时是 `None`。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn work_detail(
        &self,
        query: &WorkQuery,
        anchor: &WorkAnchor,
    ) -> Result<Option<WorkDetail>, CatalogError> {
        let keys = self.scoped_variants(query, Scope::Rows(std::slice::from_ref(anchor)))?;
        if keys.is_empty() {
            return Ok(None);
        }
        let mut variants = Vec::with_capacity(keys.len());
        let mut platforms: Vec<String> = Vec::new();
        for key in &keys {
            let Some(row) = self.variant(key)? else {
                continue;
            };
            platforms.push(
                row.platform
                    .clone()
                    .unwrap_or_else(|| UNKNOWN_PLATFORM_LABEL.to_string()),
            );
            let (state, reason) = match self.identification_of(key)? {
                Some((state, reason)) => (Some(state), reason),
                None => (None, None),
            };
            variants.push(WorkVariant {
                candidates: self.candidates_of(key)?,
                row,
                state,
                reason,
            });
        }
        platforms.sort();
        platforms.dedup();
        let name = match anchor {
            WorkAnchor::Work(id) => self.work_name(*id)?.unwrap_or_default(),
            WorkAnchor::Loose(key) => key.clone(),
        };
        let year = self.work_year(anchor, &name)?;
        Ok(Some(WorkDetail {
            anchor: anchor.clone(),
            name,
            year,
            platforms,
            variants,
        }))
    }

    /// 一行的年份。取法与主列表那一列**一模一样**（裁决优先，其次最早的那一个），
    /// 否则面板上写的与列表上写的会是两个数。
    fn work_year(&self, anchor: &WorkAnchor, name: &str) -> Result<Option<String>, CatalogError> {
        let (kind, name) = anchor.scrape_anchor(name);
        self.conn
            .prepare_cached(
                "SELECT COALESCE(MIN(CASE WHEN source = ?3 THEN value END), MIN(value))
                   FROM scrape_value WHERE anchor = ?1 AND subject = ?2 AND field = ?4",
            )
            .and_then(|mut statement| {
                statement.query_row(
                    rusqlite::params![
                        kind.label(),
                        name,
                        crate::scrape::priority::VERDICT,
                        Field::Year.label(),
                    ],
                    |row| row.get::<_, Option<String>>(0),
                )
            })
            .map_err(|source| self.err(source))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一行主列表：只填短标签要看的那几格，其余给个不碍事的值。
    fn 一行(
        anchor: WorkAnchor,
        missing: &[Field],
        confidence: Option<Confidence>,
        identified: bool,
    ) -> WorkRow {
        WorkRow {
            anchor,
            name: "某作品".to_string(),
            display: None,
            main_key: None,
            platforms: Vec::new(),
            variants: 1,
            bytes: 0,
            unreadable_files: 0,
            year: None,
            missing: missing.to_vec(),
            confidence,
            chinese: Vec::new(),
            identified,
            hit: None,
            non_game_asset: false,
        }
    }

    fn 作品() -> WorkAnchor {
        WorkAnchor::Work(1)
    }

    fn 散的() -> WorkAnchor {
        WorkAnchor::Loose("主库/GBC/精灵宝可梦 银.7z".to_string())
    }

    #[test]
    fn 元数据那一栏的短标签照稿七个词() {
        let 高 = Some(Confidence::High);
        assert_eq!(一行(作品(), &[], 高, true).meta_label(), "完整");
        assert_eq!(
            一行(作品(), &[Field::Description], 高, true).meta_label(),
            format!("缺{}", Field::Description.label()),
        );
        assert_eq!(
            一行(
                作品(),
                &[Field::Year, Field::Genre, Field::Description],
                高,
                true
            )
            .meta_label(),
            "缺 3 项",
        );
        assert_eq!(一行(作品(), &WORK_FIELDS, 高, true).meta_label(), "缺全部");
        assert_eq!(
            一行(散的(), &WORK_FIELDS, None, true).meta_label(),
            "仅文件名"
        );
        assert_eq!(
            一行(散的(), &WORK_FIELDS, Some(Confidence::Low), true).meta_label(),
            "待确认",
        );
        assert_eq!(
            一行(散的(), &WORK_FIELDS, None, false).meta_label(),
            NOT_RUN_LABEL
        );
    }

    #[test]
    fn 变体简称照词表拼() {
        use crate::adapter::converge::Preference;

        let key =
            "主库/GBC/汉化/精灵宝可梦 银[简正确精灵名](完美LOGO+背包等汉化-sss888+RickyL1213).7z";
        assert_eq!(
            variant_short_name(Some(Preference::FanTranslated), Some("口袋汉化组"), key),
            "汉化版 · 口袋汉化组",
        );
        assert_eq!(
            variant_short_name(Some(Preference::FanTranslated), None, key),
            "汉化版"
        );
        assert_eq!(
            variant_short_name(Some(Preference::FanTranslated), Some("  "), key),
            "汉化版",
            "一串空白的汉化组等于说不出",
        );
        assert_eq!(
            variant_short_name(Some(Preference::OfficialChinese), Some("口袋汉化组"), key),
            "官中版",
            "官中版不跟汉化组",
        );
        assert_eq!(
            variant_short_name(Some(Preference::Japanese), None, key),
            "原版"
        );
        assert_eq!(
            variant_short_name(Some(Preference::Other), None, key),
            "原版"
        );
        assert_eq!(
            variant_short_name(None, Some("口袋汉化组"), key),
            "精灵宝可梦 银[简正确精灵名](完美LOGO+背包等汉化-sss888+RickyL1213).7z",
            "说不出是哪一种：退回文件名",
        );
    }

    #[test]
    fn 短标签先看跑没跑过与认没认出作品_再看元数据() {
        // 元数据齐了也还没识别：先说还没识别——与置信度那个词是同一条判据。
        let 没跑过 = 一行(作品(), &[], None, false);
        assert_eq!(没跑过.meta_label(), NOT_RUN_LABEL);
        assert_eq!(没跑过.meta_label(), 没跑过.confidence_label());
        // 认不出作品、一条候选都没有，却采到了元数据：不是「仅文件名」，照缺几样说。
        assert_eq!(
            一行(散的(), &[Field::Genre], None, true).meta_label(),
            format!("缺{}", Field::Genre.label()),
        );
        // 认不出作品、最高一条是中置信的候选：还等人裁，待确认。
        assert_eq!(
            一行(散的(), &[], Some(Confidence::Medium), true).meta_label(),
            "待确认",
        );
        // 认出了作品的那一行，候选置信度不高也照缺几样说：它已经挂上作品了。
        assert_eq!(
            一行(作品(), &[], Some(Confidence::Low), true).meta_label(),
            "完整"
        );
    }
}
