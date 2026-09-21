//! **子库屏**：管住这几台设备。**不增删规则**——改选择跳回浏览屏；手挑的那一半（**例外**）在这一屏上
//! 管得了：超限时删减建议上按得出一条排除例外，「手动例外」那层弹层里包含与排除两栏逐条增撤。
//!
//! ## 一屏三件事，不是八件
//!
//! 用户对上一版的原话是「子库页面的操作逻辑看不懂」。根因不在控件长得好不好看：
//! 那一屏**把「选什么」和「送到哪」混在了一起**——子库列表、表单、规则增删、例外记撤、
//! 差量步骤、容量账、同步、删除确认，八样东西挤在一处，而且没有先后顺序。
//!
//! 拆开之后，**规则归浏览屏，送到哪归这一屏**，于是这一屏只剩三件事：
//!
//! 1. **选哪台**——中间那一列卡片，一台设备一张。
//! 2. **配目标**——「目标设置」那一层弹层（屏头「新建子库」、卡上「目标设置…」打开）：名字、目标路径、
//!    前端格式、容量上限、能力档案。
//! 3. **排差量后同步**——卡片下半截。
//!
//! 手挑的例外摆在第 1 件与第 2 件之间那层自己的弹层里，卡上不铺开：它是**一台设备一份账**，
//! 而卡片那一列要摆得下好几台。
//!
//! **规则在这一屏上只读**。规则怎么改全在浏览屏上做：卡上点
//! 「改选择」跳过去、这个子库的规则预填进筛选器，调完按「更新到子库」原样带回来。
//! 这不是为了少写几个控件——**在浏览屏上改规则，人看得见它真的筛出了什么**；
//! 在这一屏上改，改完只看得见一行字。
//!
//! **例外这一半在这一屏上管得了**（票 `gui-looks-like-the-design/22`）：规则那一行底下「手动例外」那一行按
//! 「管理」，开出包含与排除两栏那层弹层（[`Screen::open_exceptions`]）——逐条写清作品、平台、体积、
//! 备注与时间，搜作品直接加，每一条都撤得掉。**它与浏览屏详情面板里记的是同一种例外**
//! （`Catalog::set_exception`），不是第二套机制；超限时删减建议表上的「排除」、差量预览里落点撞车时的
//! 「排除这一份」落的也是它。两处各有各的长处，所以两处都留着：在这一屏上看得见**这台设备一共手挑了什么**，
//! 在浏览屏详情里指得准**是哪一份**。
//!
//! ## 顺序是硬要求，不是排版
//!
//! ADR-0016：**同步前必须预览差量，且这是硬要求不是优化项**——「永远不能点了同步就开始
//! 传」。ADR-0015 再加一条：**删除前必须干跑预览**。这一屏把两句话变成一个构造上的事实：
//! [`Screen::sync`] 只认 [`Screen::prepared`] 里那份计划，而那份计划是
//! [`romcat_core::sync::prepare`](fn@romcat_core::sync::prepare) 排出来的、界面上正摆着的同一个值。**没预览就没有可传的
//! 东西**；改过选择、改过目标之后那份预览当场作废（[`Screen::invalidate`]），
//! 「同步」按钮跟着灰掉。**搬上任务台之后这条一个字都没松**——台上排着的那一趟认的仍是
//! 排它时那份计划，见底下「三条长活全走任务台」。
//!
//! ## 差量步骤一步都不截
//!
//! 摊开之后那张表（[`steps_table`]）**列得全**：真机量级上一次同步动上万个文件（实测
//! 合成数据 **21,571 步**，`docs/library-facts.md`），而 ADR-0016 那句「同步前必须看一遍
//! 它要做什么」不该有一半落在界面之外。不截的代价是零——计划整份本来就在 [`Screen::prepared`] 里，而
//! `TableBody::rows` 只画视口里那几十行；[`Screen::steps_drawn`] 把「这一帧真的画了
//! 几行」数出来，于是这句话是被数出来的（挂账 `D158`）。
//!
//! ## 容量条三段：选中的、清单之外的、上限
//!
//! 三个数各有各的出处，而且**出处不同这件事要说得出口**（[`Gauge`]）：**选中**只问
//! 中立库，卡不在手边也算得出来；**清单之外**要目标设备在位，没看过目标时它是
//! 「还不知道」而不是零。画成同一个零的话，人会以为卡上是空的。
//!
//! 超没超由核心一处算——同步计划器，底是目标现占 ＋ 净变化；「算一遍容量」那份报告里的
//! 是从计划抄来的（`sublibrary::Fit`，挂账 D76）。条子自己不算第二遍。
//! **容量超限只给建议，绝不自动截断**（ADR-0016）——删减建议表上按「排除」，落的就是这个子库的
//! 一条**排除例外**（与浏览屏详情面板里记的是同一种，票 `gui-looks-like-the-design/20`），
//! 盘上的文件一个都不动。
//!
//! ## 领域判断一条都不在这里
//!
//! 规则怎么读（`Rule::parse`）、选择集怎么求值（`sublibrary::select`）、三方对比怎么排
//! （`sync::plan`）、目标落在主库里要不要拦（`sync::prepare::refuse_target_in_library`）
//! ——全在核心。这一层只做三件事：把要来的画出来、把点的那一下写回去、把中文输入放在
//! 对的位置上。
//!
//! ## 三条长活全走任务台
//!
//! 这一屏上会跑一会儿的有三条：**排差量预览**（真机量级 343 毫秒）、**算一遍容量**
//! （同一趟全库事实，343 毫秒）、**同步**（几十 GiB、可能几十分钟）。三条都跑在画帧
//! 那条线程之外，而且都走同一张[任务台](crate::task)——点那三个按钮等于各往台上排一趟活，
//! 跑完了台上按号把产物交回来（[`Screen::settle`]）。于是规格里那句「扫描、识别、刮削、
//! 同步统一排队，一处看得见」在这一屏上是真的：名字、进度、已用时间、按得停、跑完那条
//! 带耗时的历史，五样都在任务屏上，而这一屏在按钮旁边摆的是**同一份**快照。
//!
//! **台上那一趟认的是排它时那份计划。** 按下同步的那一刻，那份 [`Prepared`] 就整份交给
//! 了台上那趟活（闭包自己拿着一份，不是屏上这一份的借用）。此后规则怎么改、屏上那份
//! 预览怎么作废，都动不了已经排出去的那一趟——ADR-0016 那句「同步前必须预览差量」
//! 因此照旧是**构造上的事实**：跑的正是人点头时看过的那一份。反过来也说得通：改过规则
//! 之后屏上那份预览当场作废，想把改完的那一批传上去就得**重排一次差量**。
//!
//! **要写中立库的那一步在认领里做，不在台上。** 台上那条线拿的是同一个库文件的
//! **只读**连接，写不动；而一趟同步跑完——**包括被按停的那一趟**——必须把**清单**落回
//! 库里：那份清单记的是「到中断为止目标上真实有什么」，下一趟才接得上。所以它是当作
//! 产物交回来的（[`Product::Synced`]），落库发生在 [`Screen::settle`]。

use std::collections::BTreeMap;
use std::path::PathBuf;

use egui::{Align, Layout};
use romcat_core::capability::{DEFAULT_PROFILE, Entry, Override, Profile, Recipe, Roster};
use romcat_core::catalog::CatalogError;
use romcat_core::catalog::browse::{Scope, WorkAnchor, WorkQuery};
use romcat_core::catalog::sublibrary::{RemovedSublibrary, Renamed};
use romcat_core::filename::Rules;
use romcat_core::report::{decimal_bytes, decimal_gigabytes, human_bytes, thousands};
use romcat_core::scrape::Priorities;
use romcat_core::site::Site;
use romcat_core::sublibrary::report::SelectionReport;
use romcat_core::sublibrary::target::{self, NameRefusal, Presence, TargetRefusal};
use romcat_core::sublibrary::{
    BrokenRule, Exception, ExceptionDetail, ExceptionRow, Fit, Gauge, LoadedSelection, Room, Rule,
    StoredRule, Sublibrary, rule,
};
use romcat_core::sync::{self, Act, Outcome, Prepared};
use romcat_core::task::{Cutoff, Ending, Finished, Handle};

use crate::clock::{Clock, RecordClock};
use crate::dialog::{Button, Dialog, Footer, Width};
use crate::look::step;
use crate::table::ROW_HEIGHT;
use crate::task::{Product, Tasks};
use crate::toast::{self, Toast};
use crate::tokens::Tokens;
use crate::{font, look};

/// 没摊开时卡上先摆几条步骤。**摆得出样子就够**：这几条回答的是「它大概要干什么」，
/// 「一共几步」那个数写在旁边，「到底哪几步」按「全部展开」。
const STEP_SAMPLE: usize = 6;

/// 意外与放不下的那几类，最多各列几条。
const TOP_NOTES: usize = 20;

/// 手动例外弹层里搜作品，最多摆几行（设计稿 `DLG.excl` 的 `.slice(0,6)`）。
///
/// **不给「下一页」**：这儿要的是「把我想起来的那一部找出来」，不是浏览——翻页那条路在浏览屏上。
const SEARCH_HITS: u64 = 6;

/// 容量条上「清单之外：还不知道」那一段画多长，占整条的几成。
///
/// 取的是设计稿 `devCard` 里那个数（没看过目标时那一段 `min(12%, 余下的)`）：它不是一个量出来的
/// 容量——还不知道就没有数可画——只是让「这儿有一段不知道的」看得见，而不是缩成零。
const UNKNOWN_SHARE: f32 = 0.12;

/// 还没排过差量预览时卡上那条提示框（设计稿 `diffHTML` 的 `.note`）。
const PREVIEW_NOTE: &str = "同步前需要先生成差量预览，确认将要进行的更改。\
     生成预览只读取数据，不会写入任何文件。选择集修改后，已有的预览会失效。";

/// 卡不在位时容量图例底下那一行普通小字（设计稿 `devCard`；逐字照稿、样式也照稿，拿主意的人定）。
const ABSENT_NOTE: &str = "设备未连接时仍可计算已选容量；\
     清单外文件需要连接设备并生成差量预览后才能统计，未知不代表为零。";

/// 卡片底下（没有子库时是空态卡底下）那一行帮助字（设计稿子库屏的 `.help`，逐字照稿）。
const TRIM_HELP: &str = "空间不足时只给出删减建议，不会自动删除任何内容。\
     删减建议里的「排除」会记为这个子库的手动例外。";

/// 「**目标设置**」那层弹层开着时，开的是哪一种。
#[derive(Debug, Clone, PartialEq, Eq)]
enum TargetDialog {
    /// 屏头「新建子库」：一份空草稿。
    New,
    /// 卡上「目标设置…」：这一台的草稿。
    Of(String),
}

/// 目标路径那一格**上一回判的是哪一串、判出来什么**（[`target::vet`]）。
///
/// **不在每一帧里判**：判一次要化开路径、看那个卷，挂载点卡住时整个窗口会跟着卡。框里的字、正在改的是哪一台，两样都没变就
/// 照用上一回的（拿主意的人 2026-09-15 定：只在字改了、或者「选择…」交回来时查一次盘）。
#[derive(Debug, Clone)]
struct Vetted {
    /// 判的是框里哪一串。
    text: String,
    /// 判的时候正在改哪一台（新建是 `None`）。
    editing: Option<String>,
    /// 判出来什么；中立库读不动时是那句话。
    verdict: Result<Result<Presence, TargetRefusal>, String>,
}

/// 名字那一格上一回判的是哪一串、判出来什么（[`target::vet_name`]）。同 [`Vetted`]，只在字变了时再判。
#[derive(Debug, Clone)]
struct NameVetted {
    /// 判的是哪一串。
    text: String,
    /// 判的时候正在改哪一台。
    editing: Option<String>,
    /// 判出来什么；中立库读不动时是那句话。
    verdict: Result<Result<(), NameRefusal>, String>,
}

/// 新建或改一个子库时界面上那份草稿。
///
/// **每一格都会碰到输入法**（目标路径里有中文目录名是常态），所以这几个控件摆在「目标设置」那层弹层的
/// 内容区里：内容区每帧把整份内容都摆一遍，正在组字的那一格不会凭空消失（[`crate::dialog`]、ADR-0005）。
#[derive(Debug, Clone)]
pub struct Form {
    /// 子库叫什么。一台目标设备一个。
    pub name: String,
    /// 目标设备上的子库根：读卡器挂上来的那个盘上的目录。
    pub target: String,
    /// 前端格式（适配器名）。空着就是 Pegasus。
    pub format: String,
    /// 容量上限，如 `58`（光一个数按十进制 GB 读）、`476GiB`。空着就是不设限。从现成的子库填进来时写一位小数的十进制 GB 数
    /// （`511.1`，挂单 `Q856`；后面那个「GB」弹层上写着）。
    pub capacity: String,
    /// **能力档案**的名字。空着就是「不作声称」——不转换、不检查。
    pub capability: String,
    /// 容量上限是不是**按设备容量**那一档（拿主意的人 2026-09-15 照稿定）：跟着设备总容量走，换卡跟着变
    /// （`Sublibrary::capacity_by_device`）。关着是「自定义」，上限照 [`Self::capacity`] 那一格。
    pub capacity_by_device: bool,
    /// 这一台的**按平台覆盖**：平台名 → 覆盖成什么，只影响这个子库（票 `gui-looks-like-the-design/21`）。平台表里改的就是它，
    /// 「保存」时整份存进中立库（`Catalog::set_capability_overrides`）。「目标设置…」打开时照库里那一份填（[`Screen::edit_target`]）。
    pub overrides: BTreeMap<String, Override>,
    /// 从现成的子库填草稿时（[`Self::of`]），容量那一格**填进去的那串字与它原来的字节数**。
    ///
    /// 那一格写的是一位小数（`511.1 GB`），读回来是 511,100,000,000——人没碰那一格就按保存的话，上限会被
    /// 悄悄改掉。所以字没改过就沿用原来的字节数，不重新解析（拿主意的人 2026-09-14 定）。
    kept_capacity: Option<(String, u64)>,
}

impl Default for Form {
    /// 新建那层弹层上的空草稿。
    ///
    /// **容量上限默认落在「按设备容量」那一档**（设计稿）：新建时多半还没插上卡，这一档等于「插上之后跟着卡走」，
    /// 换一张卡上限跟着变；「自定义」是人填的那一档，空着虽然同样不设限，但换卡不会跟着变，不是一件事。
    fn default() -> Self {
        Self {
            name: String::new(),
            target: String::new(),
            format: String::new(),
            capacity: String::new(),
            capability: String::new(),
            capacity_by_device: true,
            overrides: BTreeMap::new(),
            kept_capacity: None,
        }
    }
}

impl Form {
    /// 从一个现成的子库填一份草稿。
    #[must_use]
    pub fn of(sublibrary: &Sublibrary) -> Self {
        // 按设备容量那一档里 `capacity` 记的是上次读到的总量，不是人填的数：自定义那一格空着。
        let kept_capacity = sublibrary
            .capacity
            .filter(|_| !sublibrary.capacity_by_device)
            .map(|bytes| (decimal_gigabytes(bytes), bytes));
        Self {
            name: sublibrary.name.clone(),
            target: sublibrary.target.clone(),
            format: sublibrary.format.clone(),
            capacity: kept_capacity
                .as_ref()
                .map(|(text, _)| text.clone())
                .unwrap_or_default(),
            capability: sublibrary.capability.clone().unwrap_or_default(),
            capacity_by_device: sublibrary.capacity_by_device,
            overrides: BTreeMap::new(),
            kept_capacity,
        }
    }
}

/// 「**改选择**」按下去之后要交给浏览屏的那一份东西。
///
/// 规则在这一屏上是**只读**的，改它的地方是浏览屏的筛选器。跳过去时带的正是这个：
/// 哪个子库、它的规则并成的那一条、以及读不懂的那几条原样。
#[derive(Debug, Clone)]
pub struct Jump {
    /// 改的是哪个子库。
    pub sublibrary: String,
    /// 这个子库的规则并成一条（多条之间是**任一满足**，与求值同一条口径）。
    ///
    /// 一条都没有时是 `None`——那是「没有任何条件」，不是「一条都选不中」。
    pub rule: Option<Rule>,
    /// 读不懂的那几条**原样带过去**（序号、原文、错在哪）。
    ///
    /// 它们没参与求值，「更新到子库」也不会碰它们；带过去是为了**在那一屏上扔得掉**
    /// （票 `gui-redesign/14`）。只带一个数的话，浏览屏摆得出「有 N 条」却指不出是哪几条。
    pub broken: Vec<BrokenRule>,
    /// 规则行上「✎」跳过去的：只改这个子库的**第几条**（`rule` 就是那一条），浏览屏「更新到子库」只换回这一条
    /// （票 `gui-looks-like-the-design/20`）。`None` 是「从浏览添加…」那种整批改。
    pub ordinal: Option<i64>,
}

/// 规则行尾那两颗图标按钮这一帧按了哪一颗、哪一条（序号）。
#[derive(Debug, Clone, Copy)]
enum RulePressed {
    /// 「✎」：去浏览屏只改这一条。
    Edit(i64),
    /// 「×」：移除这一条（先问一层）。
    Remove(i64),
    /// 例外那一行行尾「管理」：开出「手动例外」那层弹层（票 `gui-looks-like-the-design/22`）。
    ManageExceptions,
}

/// 一台设备卡头上的**能力档案**与**文件系统**：名册里解出来的那一份的名字。
#[derive(Debug, Clone)]
struct Profiled {
    /// 真会用上的那份档案叫什么。
    profile: String,
    /// 那份档案声明的文件系统叫什么。
    filesystem: String,
    /// 子库记着一份档案的名字、名册里却查不到（[`Roster::find`]）时，记着的那个名字。
    missing: Option<String>,
}

/// 一台设备**存在中立库里的那份选择集**：规则原文（连序号）、读不懂的那几条、例外。
/// 卡上的规则列表照它摆。
#[derive(Debug, Clone, Default)]
struct StoredSelection {
    /// 读得懂的规则原文，连库里的序号。
    rules: Vec<StoredRule>,
    /// 读得懂的每一条规则的短名（[`Rule::label`]，设计稿 `autoName`），按序号。读回来时拼一次，不在画帧里拼。
    labels: BTreeMap<i64, String>,
    /// 读不懂的那几条。
    broken: Vec<BrokenRule>,
    /// 例外。
    exceptions: Vec<ExceptionRow>,
}

/// 删掉一台之后留着的那一份撤销：核心交回来的整份子库，与底边那条提示条。
#[derive(Debug)]
struct Undo {
    /// 删之前整份留下来的那一份（[`Catalog::take_sublibrary`](romcat_core::catalog::Catalog::take_sublibrary)）。
    removed: RemovedSublibrary,
    /// 「已删除子库「…」，设备上的文件没有改动」，带一颗「撤销」。
    toast: Toast,
}

/// 「**手动例外**」那层弹层开着时手上的那点东西（设计稿 `DLG.excl`）。
///
/// **读库的三样都缓在这儿**（例外那张表、搜出来的那几行、优先级表），因为它们一样都不许在画帧里读：
/// 例外一屏几十条、搜一次要问两趟库，而这一屏每秒画几十帧。打开时读一遍，加减之后重读
/// （[`Screen::reload_exception_rows`]），搜索框的字变了才搜一次（[`Screen::search_exceptions`]）。
#[derive(Debug, Clone)]
struct Exceptions {
    /// 管的是哪一台。
    name: String,
    /// 停在哪一栏：包含那一栏还是排除那一栏。
    tab: Exception,
    /// 搜索框里的字。**它会碰到输入法**，所以与别的文本框一样长在弹层内容区里（ADR-0005）。
    query: String,
    /// 备注框里的字：加一条例外时随手写的那句「为什么」。
    note: String,
    /// 这一台的例外，连库里那个变体眼下是谁（`Catalog::sublibrary_exception_details`）。
    rows: Vec<ExceptionDetail>,
    /// 上一回搜的是哪几个字、搜出来哪几行。字没变就照用上一回的。
    found: Option<(String, Vec<Found>)>,
    /// 挑显示标题那份优先级表：打开弹层时读一次（与浏览屏、导出、同步读的是同一份）。
    priorities: Priorities,
}

/// 搜索出来这一行**是谁**：拿它去认「这一行眼下有没有例外、是哪一向」。
///
/// 与 [`WorkAnchor`] 装的是同一件事，只是换成例外那一侧问得动的形状——例外记在**变体**上
/// （`ExceptionRow::variant_key`），而它属于哪个作品记在 `ExceptionDetail::work` 上。
#[derive(Debug, Clone, PartialEq, Eq)]
enum Who {
    /// 认出了作品：`work` 表里那个名字。
    Work(String),
    /// 没认出作品：这一行就是那一个变体，键是它自己。
    Loose(String),
}

/// 搜索框底下的一条命中（设计稿 `.srch` 一行）：一个**作品**。
///
/// 搜的是作品而不是变体——人记得住的是「口袋妖怪 红」，不是一串相对路径。真正记下去的例外照旧
/// 落在**变体**这一层（ADR-0016、`Catalog::set_exceptions`）：按下去把这个作品底下那几个变体整批记上。
#[derive(Debug, Clone)]
struct Found {
    /// 这一行是谁。**作用范围靠它展开**（`Catalog::scoped_variants`），与浏览屏批量操作同一处。
    anchor: WorkAnchor,
    /// 屏上写的名字：认出作品的是**显示标题**，认不出的是那个变体的**正题**
    /// （`table::unlinked_title`，与浏览屏主列表同一支剥）。
    name: String,
    /// 这一行**是谁**，问「它眼下有没有例外」比的是它：认出作品的是 `work` 表里那个名字，
    /// 认不出的是那个变体的键。**不拿屏上那串字比**——两个同名的作品是真实存在的。
    who: Who,
    /// 横跨哪几个平台。
    platforms: Vec<String>,
    /// 底下几个变体。
    variants: u64,
    /// 容量合计（下界，ADR-0021）。
    bytes: u64,
}

/// 子库这个屏幕。
pub struct Screen {
    /// 工作目录：中立库、**媒体池**、能力档案名册都在这儿。
    workspace: PathBuf,
    /// 库里现有的子库。**一台设备一张卡，就是这一串。**
    list: Vec<Sublibrary>,
    /// 每台设备卡头上那两格：**能力档案**与它的**文件系统**，照核心的名册解出来
    /// （[`Roster::find_or_unclaimed`]，与排差量那一趟用的是同一处）。随 [`Self::reload`] 重读，
    /// 不在画帧里读盘。
    profiled: BTreeMap<String, Profiled>,
    /// 能力档案名册读不动时那句话。**那时卡头不编一份档案出来。**
    roster_error: Option<String>,
    /// 上一回看的时候，**目标不在位**的那几台（[`target_absent`] 那一个口径）。卡头那枚标签与「装不装得下
    /// 算不出」那一行照它说话。**不在画帧里查盘**：随 [`Self::reload`] 与任务台交回一趟活时各查一遍
    /// （[`Self::look_at_targets`]），于是插上卡之后要等下一回才换过来（挂单 `Q855`）。
    absent: std::collections::BTreeSet<String>,
    /// 眼下摊开的是哪一张卡。
    picked: Option<String>,
    /// 编辑草稿：「目标设置」那层弹层里那几格。
    form: Form,
    /// 「目标设置」那层弹层开没开着、开的是哪一种（[`Self::begin_new`] / [`Self::edit_target`]）。
    target_dialog: Option<TargetDialog>,
    /// 目标路径那一格上一回判的结果（[`Vetted`]）。
    vetted: Option<Vetted>,
    /// 名字那一格上一回判的结果（[`NameVetted`]）。
    name_vetted: Option<NameVetted>,
    /// 每台设备的**按平台覆盖**（`Catalog::capability_overrides`），随 [`Self::reload`] 读。「目标设置…」照它填草稿。
    overrides: BTreeMap<String, BTreeMap<String, Override>>,
    /// 目标设置弹层里那一台的**脚印**：为哪一台读的、读回来的那一份（`romcat_core::sync::Footprint`）。
    footprint: Option<(String, romcat_core::sync::Footprint)>,
    /// 正在台上读的那一趟脚印：任务号、为哪一台读。**它同时是认领凭据**（与 [`Self::evaluating`] 一个写法）。
    reading_footprint: Option<(u64, String)>,
    /// 这一台的脚印读失败或被停过：弹层重开之前不再自己排——不然每一帧都往台上排一趟、每一趟都失败。
    footprint_failed: Option<String>,
    /// 能力档案名册（`Roster::in_workspace`），随 [`Self::reload`] 读。目标设置弹层里的下拉与平台表照它，不在画帧里读盘。
    roster: Option<Roster>,
    /// **剥离规则**（`sources::rules`，工作目录里那份 `name-rules.toml`）：认不出作品的那一行屏上叫什么，
    /// 从它剥（`table::unlinked_title`／`WorkRow::title`，与浏览屏主列表同一支、同一份配置）。
    /// 开屏时读一次；读不动时是那句话（[`Self::rules_error`]），手动例外那层弹层开不出来。
    rules: Option<Rules>,
    /// 剥离规则读不动时那句话。
    rules_error: Option<String>,
    /// 「今天」：平台表判「陈旧」用（`Claim::is_stale`）。`None` 是照系统时钟（`capability::today`）；截图测试钉死它
    /// （[`Self::set_today`]），截图里才没有当前日期。
    today: Option<String>,
    /// 这一台选择集在眼下挑的档案（叠上覆盖）下**放不下哪几份**：档案名、覆盖、结果（`Footprint::too_big`）。
    /// 纯算，但与选择集一样大——只在档案或覆盖变了时重算。
    too_big: Option<(
        String,
        BTreeMap<String, Override>,
        Vec<romcat_core::sync::Rejected>,
    )>,
    /// 目标设置弹层里那条路径上**清单之外**的文件数完了：数的是框里哪一串、数出来多少（`sync::strangers`）。
    strangers: Option<(String, romcat_core::sync::Strangers)>,
    /// 正在台上数的那一趟：任务号、数的是哪一串。**它同时是认领凭据**。
    counting: Option<(u64, String)>,
    /// 这一串数失败或被停过：字改了之前不再自己排。
    counting_failed: Option<String>,
    /// 目标设置里存下之后底边那条提示条（拿主意的人 2026-09-15 定，F9）：「已创建子库…」「已保存…差量预览已失效…」。
    /// **一次只摆一条**：摆它时删除之后那条带「撤销」的就收了。
    saved: Option<Toast>,
    /// 库里头一个有平台的变体住的平台目录（`Catalog::sample_platform_directory`）：前端格式那句说明拿它举例。弹层打开时读一次，
    /// 外层 `None` 是还没读。
    sample_directory: Option<Option<String>>,
    /// **每台设备**的选择集原文：规则（连库里的序号）、读不懂的那几条、例外。
    ///
    /// 每张卡都摆它自己的规则列表（票 `gui-looks-like-the-design/20`），所以一台不落全读回来
    /// （[`Self::reload`]），不只读摊开那一张。**一条坏的不该让另外五条一起用不了**：
    /// 读不懂的另放一栏。
    selections: BTreeMap<String, StoredSelection>,
    /// 排出来的那份计划，**它就是差量预览**。
    prepared: Option<Prepared>,
    /// 正在排的那一趟差量预览是任务台上的第几号。
    ///
    /// **它同时是认领凭据**：跑完的那一趟按号对得上才收（[`Screen::settle`]）。
    /// 中途改过选择的话这个号会被 [`Screen::invalidate`] 抹掉，那一趟排出来的差量
    /// 说的已经不是眼下这套选择集会做的事了，收回来反而是骗人。
    previewing: Option<u64>,
    /// 排它用了多久，毫秒。
    prepare_ms: f64,
    /// 差量步骤摊开了没有。**默认不摊**：几百上千步铺满一屏，把它下面的按钮挤没了。
    expanded: bool,
    /// 刚摊开、还没把那张表滚进视口。
    ///
    /// 卡上差量表上面摆着卡头、规则、容量条与差量账，**摊开时那张表多半在视口底下**——
    /// 虚拟化的表只画视口里的行，于是按了「全部展开」却一行都看不见。摊开那一下把它滚到
    /// 视口顶上：看得见几行由视口多高决定，与上面那几段（按钮多高、规则几条）无关。
    reveal_steps: bool,
    /// 上一帧那张步骤表**真的画了几行**。
    ///
    /// 它是「翻行的代价与总步数无关」那句话的**量具**，与 [`crate::table::Window::reads`]
    /// 同一个路子：不掐表，数一帧真的做了多少事。挂钟在门禁上是一张彩票
    /// （票 `parking-3/01` 拿掉过一条挂钟断言），而这个数机器忙不忙一个字都不影响。
    steps_drawn: usize,
    /// 每台设备各求一次**选择集**的结果：选中哪些、多大、超限多少、砍谁。
    ///
    /// **一趟折一次事实，全部子库共用**——折事实是走一遍全库（343 ms，挂账 D156），
    /// 而按选择集求值是内存里的事。一台一折的话，五张卡就是五趟全库。
    /// 整趟活在核心里（`sublibrary::survey`），于是屏上摆着的与 `romcat sublibrary show`
    /// 印出来的是同一个值。
    ///
    /// **与差量预览分开**：「这套规则选出多少、多大」只要中立库，子库是持久实体，不是
    /// 「插上卡才存在的东西」——卡不在手边时照样该看得见。**装不装得下**两边走的是同一条
    /// 排计划的线（目标现占 ＋ 净变化，挂账 D76），只是这里一趟问全部设备、不留可同步的
    /// 计划；卡不在手边的那台如实说算不出。
    evaluated: BTreeMap<String, SelectionReport>,
    /// 正在算的那一趟容量是任务台上的第几号。**它同时是认领凭据**
    /// （与 [`Self::previewing`] 一个写法）。
    evaluating: Option<u64>,
    /// 计划里有删除时，要先勾这一格才动得了手。
    acknowledged: bool,
    /// 「改选择」按下去了，等窗口把它送去浏览屏（[`crate::app::App::route`]）。
    jump: Option<Jump>,
    /// 「删除子库」那层确认弹层开着时，删的是哪一台（[`Self::ask_remove`]）。
    ///
    /// **删的是选择集，不是文件**：规则、例外、清单跟着那条定义一起没。例外是**永久记住**的手挑决定
    /// （ADR-0016），规则也不在这一屏上重打得回来——所以按下去先问一层，问的时候把名字与目标路径写明。
    delete_dialog: Option<String>,
    /// 规则行上「×」那层确认弹层开着时，移除的是哪一台的第几条规则（[`Self::ask_remove_rule`]）。
    rule_dialog: Option<(String, i64)>,
    /// 「手动例外」那层弹层开着时手上那点东西（[`Self::open_exceptions`]，票 `gui-looks-like-the-design/22`）。
    exceptions: Option<Exceptions>,
    /// 例外那张表「时间」一列怎么画（[`RecordClock`]）：本地短格式；截图测试钉死此刻、偏移
    /// （[`Self::set_clock`]）与那一刻（[`Self::pin_exception_time`]），截图里才没有当前时间。
    exception_clock: RecordClock,
    /// **这一屏刚动过哪个子库的例外**，等窗口转告浏览屏（[`Self::take_touched`]，挂单 `Q812`）。
    ///
    /// 与浏览屏那个同名的记号**反向同形**：那一条是浏览屏改了例外、转告这一屏把缓着的账丢掉
    /// （`browse::Screen::take_touched` → [`Self::forget`]），这一条是这一屏改了、转告浏览屏重读
    /// 它手上缓着的那一份。两条都只有窗口够得着两屏（ADR-0005）。
    touched: Option<String>,
    /// 刚删掉的那一台**整份留着**，提示条上那颗「撤销」按下去原样放回去（[`Self::undo_remove`]）。
    ///
    /// 提示条收起来、或者换到别的屏（[`Self::leave`]）就丢掉：那之后删除就是真的删了。
    undo: Option<Undo>,
    /// 每台设备的**清单**记着几条（[`Self::read_manifests`]）：卡头那枚「已同步 · 清单 N 条」照它写。
    manifest_rows: BTreeMap<String, usize>,
    /// 排上任务台的那一趟同步是第几号。
    ///
    /// **它同时是认领凭据**：跑完的那一趟按号对得上才收（[`Screen::settle`]）。
    /// **但它不是「那一趟认哪份计划」的凭据**——计划在排它那一刻就整份交给了台上那趟活，
    /// 屏上这一份此后怎么变都影响不到它（模块文档「台上那一趟认的是排它时那份计划」）。
    syncing: Option<u64>,
    /// 上一趟同步的账。
    outcome: Option<Outcome>,
    /// 上一次动作的回执。
    notice: Option<String>,
    /// 上一趟同步有没有出岔子（被停、放弃、有步骤失败）。回执照红的画。
    failed: bool,
    /// 上一次出的错。
    error: Option<String>,
}

impl Screen {
    /// 开一个空屏幕。
    #[must_use]
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            list: Vec::new(),
            profiled: BTreeMap::new(),
            roster_error: None,
            absent: std::collections::BTreeSet::new(),
            picked: None,
            form: Form::default(),
            target_dialog: None,
            selections: BTreeMap::new(),
            prepared: None,
            previewing: None,
            prepare_ms: 0.0,
            expanded: false,
            reveal_steps: false,
            steps_drawn: 0,
            evaluated: BTreeMap::new(),
            evaluating: None,
            acknowledged: false,
            jump: None,
            delete_dialog: None,
            rule_dialog: None,
            exceptions: None,
            exception_clock: RecordClock::default(),
            touched: None,
            undo: None,
            manifest_rows: BTreeMap::new(),
            syncing: None,
            outcome: None,
            notice: None,
            vetted: None,
            name_vetted: None,
            overrides: BTreeMap::new(),
            footprint: None,
            reading_footprint: None,
            footprint_failed: None,
            roster: None,
            rules: None,
            rules_error: None,
            today: None,
            too_big: None,
            strangers: None,
            counting: None,
            counting_failed: None,
            saved: None,
            sample_directory: None,
            failed: false,
            error: None,
        }
    }

    /// 重新列一遍库里有哪些子库。
    pub fn reload(&mut self, site: &Site) {
        // **判过的名字与路径跟着作废**：库里的子库变了（存了、删了、改了名），上一回判的「能用」「被谁占着」说的是变之前。
        self.vetted = None;
        self.name_vetted = None;
        match site.catalog.sublibraries() {
            Ok(list) => {
                self.list = list;
                self.error = None;
            }
            Err(error) => self.error = Some(format!("中立库读不动：{error}")),
        }
        // 每台设备的按平台覆盖：「目标设置…」照它填草稿。读不动的那一台当它一行都没覆盖（照名册判）。
        self.overrides = self
            .list
            .iter()
            .filter_map(|sublibrary| {
                let rows = site.catalog.capability_overrides(&sublibrary.name).ok()?;
                Some((sublibrary.name.clone(), rows))
            })
            .collect();
        // **档案名解到哪一份由核心说**：记着的名字在名册里没有时退回「不作声称」
        // ——卡头写的是真会用上的那一份，不是记着的那个名字。
        match Roster::in_workspace(&self.workspace) {
            Ok(roster) => {
                self.profiled = self
                    .list
                    .iter()
                    .map(|sublibrary| {
                        let recorded = sublibrary.capability.as_deref();
                        let profile = roster.find_or_unclaimed(recorded);
                        (
                            sublibrary.name.clone(),
                            Profiled {
                                profile: profile.name,
                                filesystem: profile.filesystem.name,
                                missing: recorded
                                    .filter(|name| roster.find(name).is_none())
                                    .map(ToString::to_string),
                            },
                        )
                    })
                    .collect();
                self.roster = Some(roster);
                self.roster_error = None;
            }
            Err(error) => {
                self.profiled.clear();
                self.roster = None;
                self.roster_error = Some(format!("能力档案名册读不动：{error}"));
            }
        }
        // **剥离规则**：认不出作品的那一行屏上叫什么从它剥。与浏览屏读的是同一份
        // （工作目录里那份 `name-rules.toml`），交错了同一份内容在两屏上就是两个名字。
        match romcat_core::sources::rules(&self.workspace) {
            Ok(rules) => {
                self.rules = Some(rules);
                self.rules_error = None;
            }
            Err(error) => {
                self.rules = None;
                self.rules_error = Some(format!("剥离规则读不动：{error}"));
            }
        }
        self.look_at_targets();
        self.read_manifests(site);
        self.selections.clear();
        // **摊开的那一张先由 `open` 读**（它会重设这一屏那句错），别的卡随后读：
        // 次序反过来的话，别的卡读不动的那句话会被 `open` 抹掉。
        match self.picked.clone() {
            Some(name) if self.list.iter().any(|row| row.name == name) => self.open(site, &name),
            Some(_) => self.picked = None,
            None => {}
        }
        let others: Vec<String> = self
            .list
            .iter()
            .map(|row| row.name.clone())
            .filter(|name| self.picked.as_deref() != Some(name.as_str()))
            .collect();
        for name in &others {
            if let Err(why) = self.read_selection(site, name) {
                self.error = Some(why);
            }
        }
    }

    /// 查一眼每台设备的目标**此刻在不在位**，记进 [`Self::absent`]。
    ///
    /// 口径与排差量预览、同步按下去那一刻拦的是同一个（[`target_absent`]：查一眼那个目录有没有），
    /// 查的是子库自己那条读盘路径（`Sublibrary::read_path`，ADR-0020）。
    fn look_at_targets(&mut self) {
        self.absent = self
            .list
            .iter()
            .filter(|sublibrary| target_absent(&sublibrary.read_path()).is_some())
            .map(|sublibrary| sublibrary.name.clone())
            .collect();
    }

    /// 读一遍每台设备的**清单**记着几条，记进 [`Self::manifest_rows`]。
    ///
    /// 走核心现成的读清单入口（`Catalog::manifest`）。**读不动的、还是空清单的那一台不记**：卡头那枚标签
    /// 退回「已经对齐」，不编一个数。**不在画帧里读库**：随 [`Self::reload`] 与任务台交回一趟活时各读一遍。
    fn read_manifests(&mut self, site: &Site) {
        self.manifest_rows = self
            .list
            .iter()
            .filter_map(|sublibrary| {
                let manifest = site.catalog.manifest(&sublibrary.name).ok()?;
                (!manifest.files.is_empty())
                    .then(|| (sublibrary.name.clone(), manifest.files.len()))
            })
            .collect();
    }

    /// **换到别的屏了**：删掉一台之后留着的那一份撤销跟着丢掉——从这一刻起删除就是真的删了（拿主意的人
    /// 2026-09-14 定）。窗口在画别的屏的每一帧里调它（[`crate::app::App::ui`]）。
    pub fn leave(&mut self) {
        self.undo = None;
    }

    /// 眼下还撤销得了的那一台叫什么（提示条还摆着）；没有就是 `None`。
    #[must_use]
    pub fn undo_pending(&self) -> Option<&str> {
        self.undo.as_ref().map(|undo| undo.removed.name())
    }

    /// 库里现有的子库。
    #[must_use]
    pub fn list(&self) -> &[Sublibrary] {
        &self.list
    }

    /// 眼下摊开的是哪一张卡。
    #[must_use]
    pub fn picked(&self) -> Option<&str> {
        self.picked.as_deref()
    }

    /// 摊开那个子库的规则原文。**只读**：改它去浏览屏。
    #[must_use]
    pub fn rules(&self) -> &[StoredRule] {
        self.picked_selection().map_or(&[], |chosen| &chosen.rules)
    }

    /// 摊开那个子库读不懂的那几条规则。
    #[must_use]
    pub fn broken(&self) -> &[BrokenRule] {
        self.picked_selection().map_or(&[], |chosen| &chosen.broken)
    }

    /// 摊开那个子库的例外。
    #[must_use]
    pub fn exceptions(&self) -> &[ExceptionRow] {
        self.picked_selection()
            .map_or(&[], |chosen| &chosen.exceptions)
    }

    /// 摊开那一台的选择集原文。
    fn picked_selection(&self) -> Option<&StoredSelection> {
        self.picked
            .as_ref()
            .and_then(|name| self.selections.get(name))
    }

    /// 把一台设备的选择集原文从中立库读回来，替掉缓着的那一份。读不动时返回那句话，
    /// 缓着的那一份照旧丢掉——**不留一份说不清是哪一刻的旧规则**摆在卡上。
    fn read_selection(&mut self, site: &Site, name: &str) -> Result<(), String> {
        self.selections.remove(name);
        let loaded = site
            .catalog
            .selection(name)
            .map_err(|error| format!("中立库读不动：{error}"))?;
        let stored = site
            .catalog
            .sublibrary_rules(name)
            .map_err(|error| format!("中立库读不动：{error}"))?;
        // **读得懂的与读不懂的分两栏摆**：合在一起的话，读不懂那几条会被印两遍
        // ——一遍在规则里当正常的，一遍在下面当坏的。
        let broken: std::collections::BTreeSet<i64> =
            loaded.broken.iter().map(|row| row.ordinal).collect();
        let labels = loaded
            .selection
            .rules
            .iter()
            .zip(&loaded.ordinals)
            .map(|(rule, ordinal)| (*ordinal, rule.label()))
            .collect();
        self.selections.insert(
            name.to_string(),
            StoredSelection {
                rules: stored
                    .into_iter()
                    .filter(|row| !broken.contains(&row.ordinal))
                    .collect(),
                labels,
                broken: loaded.broken,
                exceptions: loaded.selection.exceptions,
            },
        );
        Ok(())
    }

    /// 排出来的那份差量预览。
    #[must_use]
    pub fn prepared(&self) -> Option<&Prepared> {
        self.prepared.as_ref()
    }

    /// 排一次预览用了多久，毫秒。**任务台记的那个数**，不是界面自己掐的表。
    #[must_use]
    pub fn prepare_ms(&self) -> f64 {
        self.prepare_ms
    }

    /// 差量步骤摊开了没有。
    #[must_use]
    pub fn expanded(&self) -> bool {
        self.expanded
    }

    /// 摊开或收起差量步骤。**界面上按「全部展开」走的就是它**，测试拿它当那一下。
    ///
    /// 收起来的时候卡上只摆头几条（`STEP_SAMPLE`）——几百上千步铺满一屏，
    /// 会把它下面的「同步」挤到看不见的地方。摊开之后那张表是**虚拟化**的，
    /// **一步都不截**（[`steps_table`]）。
    pub fn expand(&mut self, on: bool) {
        self.expanded = on;
        self.reveal_steps = on;
    }

    /// 上一帧那张步骤表真的画了几行。见这个字段的文档：它是那句「翻行的代价与总步数
    /// 无关」的量具。摊开着才有值，没摊开是 0。
    #[must_use]
    pub fn steps_drawn(&self) -> usize {
        self.steps_drawn
    }

    /// 正在排的那一趟差量预览是任务台上的第几号；没排着就是 `None`。
    #[must_use]
    pub fn previewing(&self) -> Option<u64> {
        self.previewing
    }

    /// 某一台设备那份求过的**选择集报告**：选出多少、多大、超限多少、砍谁。
    #[must_use]
    pub fn evaluated(&self, name: &str) -> Option<&SelectionReport> {
        self.evaluated.get(name)
    }

    /// 正在算的那一趟容量是任务台上的第几号。
    #[must_use]
    pub fn evaluating(&self) -> Option<u64> {
        self.evaluating
    }

    /// 排上任务台的那一趟同步是第几号。
    #[must_use]
    pub fn syncing(&self) -> Option<u64> {
        self.syncing
    }

    /// 某一台设备卡上那根**容量条**：选中的、清单之外的、上限。
    ///
    /// 三个数从哪儿来，[`Gauge`] 的文档写着。**清单之外要看过目标才知道**：排过差量预览的
    /// 那一台、或者算过容量时卡在手边的那几台，条子才有第二段。
    ///
    /// ## 条子与旁边那行「超出容量上限」必须是同一笔账
    ///
    /// 超没超由核心一处算（ADR-0016），这一屏两处摆它：排过差量的照
    /// [`Plan::over_capacity`]，没排过的照报告里从计划抄来的那一份（[`Fit`]）——同一个底。
    /// 于是条子的总量得**照同一条口径**填，不然会出现「条子画到九成、旁边说超了
    /// 200 MiB」——那正是 [`Gauge`] 的文档说不该发生的事。
    ///
    /// - 排过差量、或者算过容量时卡在手边：`选中 = after_bytes − stranger_bytes`，于是
    ///   `taken()` 正好是 `after_bytes`——计划算超出量用的就是它，报告里那份是从计划抄的。
    ///   **它不等于「期望总量」**：卡上还留着那些「对不上、因此本次不动」的文件，
    ///   它们照样占地方（挂账 D76）。
    /// - 没看过目标：`选中 = report.bytes`、清单之外是 `None`，而旁边**不给超出量**
    ///   （算不出）——条子只画选中那一段与上限。
    ///
    /// [`Plan::over_capacity`]: romcat_core::sync::Plan::over_capacity
    /// [`Fit`]: romcat_core::sublibrary::Fit
    #[must_use]
    pub fn gauge(&self, name: &str) -> Gauge {
        let room = self.room_of(name);
        Gauge {
            picked: room.as_ref().map_or_else(
                || self.evaluated.get(name).map_or(0, |report| report.bytes),
                |room| room.after_bytes.saturating_sub(room.stranger_bytes),
            ),
            strangers: room.as_ref().map(|room| room.stranger_bytes),
            // **排过差量、算过容量的照计划里真用上的那个上限画**（`Room::capacity`：按设备容量是卡此刻的总量，本机磁盘不设
            // 上限时按剩余空间算）——与旁边「超出容量上限」同一个底；都没有时照库里记着的。
            capacity: room.as_ref().map_or_else(
                || {
                    self.list
                        .iter()
                        .find(|row| row.name == name)
                        .and_then(|row| row.capacity)
                },
                |room| room.capacity,
            ),
        }
    }

    /// 这一台卡上那笔**装得下吗**的账：排过差量的照那份计划，没排过的照「算一遍容量」
    /// 报告里从计划抄来的那份——两份同底（目标现占 ＋ 净变化，挂账 D76）。
    /// 卡不在手边、也没排过差量时是 `None`：**不是「装得下」，是算不出**。
    fn room_of(&self, name: &str) -> Option<romcat_core::sublibrary::Room> {
        self.plan_of(name)
            .map(romcat_core::sublibrary::Room::of)
            .or_else(|| {
                self.evaluated
                    .get(name)
                    .and_then(|report| report.fit.known())
                    .cloned()
            })
    }

    /// 上一趟同步的账。
    #[must_use]
    pub fn outcome(&self) -> Option<&Outcome> {
        self.outcome.as_ref()
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

    /// 「目标设置」那层弹层里那份草稿，供实测与测试填。
    pub fn form_mut(&mut self) -> &mut Form {
        &mut self.form
    }

    /// 「目标设置」那层弹层开着没有。
    #[must_use]
    pub fn target_settings_open(&self) -> bool {
        self.target_dialog.is_some()
    }

    /// 摊开一张卡：读它的规则与例外。**排出来的那份预览当场作废**——
    /// 换了子库还留着上一个的差量，是这一屏最容易骗到人的一种写法。
    pub fn open(&mut self, site: &Site, name: &str) {
        self.picked = Some(name.to_string());
        self.invalidate();
        if let Some(sublibrary) = self.list.iter().find(|row| row.name == name) {
            self.form = Form::of(sublibrary);
        }
        self.error = self.read_selection(site, name).err();
    }

    /// 某一台设备的选择集或它自己被**别处**改过了：把这一屏为它缓着的东西全丢掉。
    ///
    /// 这一屏缓两样派生的东西：那份**差量预览**（[`Self::prepared`]）与**算过的选择集**
    /// （[`Self::evaluated`]）。两样的输入都躺在中立库里，而这一票之后**改它们的地方
    /// 在另一屏上**——浏览屏换规则、记例外、撤例外（票 `gui-redesign/11`）。
    /// 没有这一趟的话，屏上会摆着一份按旧规则算出来的容量与一份按旧选择集排出来的差量，
    /// 而「同步」按钮认的正是后者（ADR-0016 的「改过选择之后那份预览当场作废」）。
    ///
    /// 由 [`crate::app::App::route`] 每帧转过来——**只有窗口同时够得着两屏**。
    ///
    /// 只丢这一台的：折一趟事实全部设备共用，别人那几张卡的数还是好的。
    pub fn forget(&mut self, site: &Site, name: &str) {
        self.evaluated.remove(name);
        // 选择集变了，为它读的脚印说的已经不是眼下这一批了。
        if self.footprint.as_ref().is_some_and(|(of, _)| of == name) {
            self.footprint = None;
        }
        if let Some(说一句) = self.drop_survey() {
            self.notice = Some(说一句.to_string());
        }
        // 摊开的正是它的话，规则与例外也要重读一遍——卡上那几行印的就是它们。
        // `open` 自己会把差量预览作废（那份差量只可能是摊开这一台的）。
        // 没摊开的那一张也摆着自己的规则列表，同样要重读。
        if self.picked.as_deref() == Some(name) {
            self.open(site, name);
        } else if let Err(why) = self.read_selection(site, name) {
            self.error = Some(why);
        }
    }

    /// 把排出来的那份预览作废。
    ///
    /// 改过选择、改过目标、改过子库本身之后都要走一趟：那份差量说的已经不是眼下这套
    /// 选择集会做的事了，而「同步」按钮认的正是它（ADR-0016）。
    ///
    /// **正在台上排着的那一趟也一并不认了**：它是照旧那套规则排的，收回来同样是骗人。
    /// 那趟活自己会跑完（整条只读，跑完也没有副作用），只是没人认领它。
    pub fn invalidate(&mut self) {
        self.prepared = None;
        self.previewing = None;
        // **耗时跟着那份差量一起作废。** 留着上一趟的数，下一趟被按停时旁边就摆着一个
        // 「排它用了 120 ms」——那说的是一份已经不在了的差量。
        self.prepare_ms = 0.0;
        self.expanded = false;
        self.reveal_steps = false;
        self.acknowledged = false;
        self.outcome = None;
    }

    /// **台上那趟还没认领的「算一遍容量」一并不认了**，返回要对人说的那半句话。
    ///
    /// 三处要走它：改过某一台的选择集（[`Self::forget`]）、存过一个子库
    /// （[`Self::save`]）、删掉一个子库（[`Self::remove`]）。理由是同一条：**那一趟折
    /// 报告用的是排它时那份子库与那时候的规则**——上限、目标、能力档案全在里头。改完之后
    /// 把它收回来，卡上就会摆出「上限写着 64 GB、旁边说超了 200 GiB」，那正是 [`Gauge`]
    /// 的文档说不该发生的事。删掉一台之后收回来更糟：认领是**整份替换**，
    /// 刚删掉那一台的报告会又长回来。
    ///
    /// **丢得不吭声是不行的**：人按过那个按钮、可能等了几分钟（真库上它排在扫描后面），
    /// 屏上却一个数都没长出来，那看着就像按钮坏了。
    ///
    /// 被弃认的那趟活自己会跑完——它整条只读，跑完也没有副作用，只是没人认领它。
    /// **弃认的是整趟而不是那一台**：产物是一份「全部设备」的表，按台拆开认领要多立一种
    /// 合并口径（「这一台按新的、那几台按旧的」），而那份账本来就是一趟折出来的
    /// ——多算一遍全库比多等一趟诚实。
    fn drop_survey(&mut self) -> Option<&'static str> {
        self.evaluating
            .take()
            .map(|_| "正在算的那一趟容量不认了——它算的是改之前那一套；再按一次「算一遍容量」。")
    }

    /// **每台设备各求一次选择集**：这套规则加例外选出什么、多大、装不装得下。
    /// 往[任务台](crate::task)上排一趟，跑在画帧那条线程之外。
    ///
    /// 选中多少只问中立库；**装不装得下对着目标排一遍计划**（与差量预览同一条线，挂账 D76），
    /// 卡不在手边的那台如实说算不出、不给数。折事实那一趟走一遍
    /// 全库，**全部子库共用它**：一台一折的话，五张卡就是五趟全库。整趟活整份交给核心
    /// （[`survey`](romcat_core::sublibrary::survey)），于是界面上摆着的与
    /// `romcat sublibrary show` 印出来的是同一个值。容量超限时**只给建议，一个都不砍**
    /// （ADR-0016）。
    ///
    /// **它原先跑在画帧那条线程上**：真机量级上按一下窗口僵 343 毫秒，期间连「停下」
    /// 都没有（挂单 Q87、挂账 D156）。搬上任务台之后进度看得见、停得动，而它整条只读，
    /// 所以停在哪儿都是干净的——一个字节都没写，再算一次就是。
    ///
    /// 后台那条线程读的是同一个库文件的**第二份只读连接**，与 [`Self::preview`] 同一条路
    /// （分不出来的那种库就地跑完，理由见那一处）。
    ///
    /// 界面上按那个按钮走的就是它，实测与测试拿它当那一下。
    pub fn evaluate(&mut self, site: &Site, tasks: &mut Tasks) {
        if self.evaluating.is_some() {
            return;
        }
        self.error = None;
        // **算的是排它这一刻库里摆着的那几台设备**，这一份名单跟着那趟活走。
        let list = self.list.clone();
        let workspace = self.workspace.clone();
        let title = "算一遍容量".to_string();
        self.evaluating = Some(match site.catalog.read_only() {
            Ok(reader) => tasks.queue(title, move |task| {
                romcat_core::sublibrary::survey(&reader, &workspace, &list, task)
                    .map(|reports| Product::Evaluated(Box::new(reports)))
            }),
            Err(CatalogError::NotOnDisk { .. }) => tasks.run_here(title, |task| {
                romcat_core::sublibrary::survey(&site.catalog, &workspace, &list, task)
                    .map(|reports| Product::Evaluated(Box::new(reports)))
            }),
            Err(why) => {
                self.error = Some(no_second_connection("算一遍容量", &why));
                return;
            }
        });
    }

    /// **排一次差量预览**：往[任务台](crate::task)上排一趟，跑在画帧那条线程之外。
    ///
    /// 只读：中立库读一遍、目标设备走只读接缝看一遍，一个文件都不写。因此中途按停下
    /// 停在哪儿都是干净的——**再排一次就是从头排一次**，几百毫秒的活，没有半截状态
    /// 要收拾（挂账 D156）。
    ///
    /// ## 后台那条线程读的是哪一份库
    ///
    /// `rusqlite::Connection` 不是 `Sync`，所以后台拿不到界面这条线程手里那一份。
    /// 它拿的是**同一个文件的第二份只读连接**（[`Catalog::read_only`]）：写不动、
    /// 不建表、跑完就丢，于是「两份库不一致」这条路在构造上就不存在。
    /// 只活在内存里的那种库（合成数据）分不出第二份连接，那一趟就**就地跑完**——
    /// 合成数据上它是几毫秒的事。**别的原因分不出来就直说**，不偷偷退到画帧那条线程上
    /// 跑一趟：那既会僵住窗口，又把真正的问题盖住了。
    ///
    /// 界面上按那个按钮走的就是它，实测与测试拿它当那一下。
    ///
    /// [`Catalog::read_only`]: romcat_core::catalog::Catalog::read_only
    pub fn preview(&mut self, site: &Site, tasks: &mut Tasks) {
        let Some(name) = self.picked.clone() else {
            self.error = Some("先摊开一张卡。".to_string());
            return;
        };
        if self.previewing.is_some() {
            return;
        }
        // **卡不在位是按下去之前就判得出的**（票 `gui-looks-like-the-design/07`）：不排，只在屏上
        // 说为什么不行、去哪儿办。排上去的话那一趟在「看一眼目标」那一下撞上、记一条失败——任务
        // 历史里就多一条压根没开跑的「失败」。查的是子库自己那条读盘路径（`Sublibrary::read_path`，
        // ADR-0020），与那一趟看的是同一个目录。
        //
        // **记着的前端格式这一版没有适配器，不在这里拦**：找不到时该说哪句话是核心库的事，而核心库
        // 对子库没有对外的那一问（`ExportSetup::adapter` 的文档：界面那一层不自己去
        // `adapter::find`）。它照旧排上去，在那一趟里记失败（挂单 `Q797`）。
        if let Some(why) = self
            .list
            .iter()
            .find(|sublibrary| sublibrary.name == name)
            .and_then(|sublibrary| target_absent(&sublibrary.read_path()))
        {
            self.error = Some(why);
            return;
        }
        self.invalidate();
        self.error = None;
        let workspace = self.workspace.clone();
        let title = format!("排差量预览 · {name}");
        self.previewing = Some(match site.catalog.read_only() {
            Ok(reader) => tasks.queue(title, move |task| {
                sync::prepare(&reader, &workspace, &name, &sync::Request::default(), task)
                    .map(|prepared| Product::Preview(Box::new(prepared)))
            }),
            // **只活在内存里的库分不出第二份连接**（合成数据走这条），那是意料之中的：
            // 这一趟就地跑完，几毫秒的事。
            Err(CatalogError::NotOnDisk { .. }) => tasks.run_here(title, |task| {
                sync::prepare(
                    &site.catalog,
                    &workspace,
                    &name,
                    &sync::Request::default(),
                    task,
                )
                .map(|prepared| Product::Preview(Box::new(prepared)))
            }),
            // 别的原因是**意外**——开这份库的时候它还好好的，文件却没了、或者结构版本
            // 对不上。这时**直说，不要退到画帧这条线程上偷偷跑一趟**：那既会僵住窗口，
            // 又把真正的问题盖在一句「怎么卡了一下」底下。
            Err(why) => {
                self.error = Some(no_second_connection("排差量预览", &why));
                return;
            }
        });
    }

    /// 任务台交回来一趟跑完的活。**不是自己那一趟就放过去。**
    ///
    /// 这一屏在台上排三种活——排差量预览、算一遍容量、同步——各按自己那个号认领。
    /// **同步那一支要写中立库**（把**清单**落回去），所以这一层收的是可写的那份现场：
    /// 台上那条线拿的是只读连接，写不动。
    pub fn settle(&mut self, site: &mut Site, done: Finished<Product>) {
        if self.previewing == Some(done.id) {
            self.previewing = None;
            self.settle_preview(done);
        } else if self.evaluating == Some(done.id) {
            self.evaluating = None;
            self.settle_evaluate(done);
        } else if self.syncing == Some(done.id) {
            self.syncing = None;
            self.settle_sync(site, done);
        } else if let Some((_, name)) = self.reading_footprint.take_if(|(id, _)| *id == done.id) {
            self.settle_footprint(name, done);
        } else if let Some((_, text)) = self.counting.take_if(|(id, _)| *id == done.id) {
            self.settle_strangers(text, done);
        }
        // 那一趟刚看过目标（或者往上写过）：卡头说的「在不在位」与清单记着几条跟着换过来。
        self.look_at_targets();
        self.read_manifests(site);
    }

    /// 排差量预览那一趟回来了。
    fn settle_preview(&mut self, done: Finished<Product>) {
        match done.ended {
            Ending::Done(Product::Preview(prepared))
            | Ending::Halfway {
                product: Product::Preview(prepared),
                ..
            } => {
                // **只有真排出来那一趟才记耗时。** 被撤掉、出错的那趟什么都没排出来，
                // 摆一个「排它用了 120 ms」在旁边等于给一份不存在的差量记账。
                // （**停在半路**那一档它到不了：排差量整条只读，停下来什么都不留下。
                // 并进这一支只为把那条轴配全。）
                self.prepare_ms = done.elapsed.as_secs_f64() * 1000.0;
                self.prepared = Some(*prepared);
                self.error = None;
            }
            // 别的屏排上去的活轮不到这儿——`previewing` 那道判断已经挡掉了，
            // 这一支只为把 `Product` 那个枚举配全。
            Ending::Done(_) | Ending::Halfway { .. } => {}
            // **停下来的地方是干净的，就得这么说。** 说成「失败」会让人去找哪儿坏了。
            Ending::Stopped => {
                self.notice = Some(format!(
                    "排差量预览{}。这一趟整条只读——中立库、媒体池、目标设备\
                     一个字节都没动，再排一次就是。",
                    Ending::<()>::Stopped.render(),
                ));
                self.failed = false;
            }
            // **不静默结束**：哪一步、为什么，两样都说出来。
            Ending::Failed { step, why } => {
                self.error = Some(format!(
                    "排差量预览{}",
                    Ending::<()>::Failed { step, why }.render()
                ));
            }
        }
    }

    /// 算一遍容量那一趟回来了。
    fn settle_evaluate(&mut self, done: Finished<Product>) {
        match done.ended {
            Ending::Done(Product::Evaluated(reports))
            | Ending::Halfway {
                product: Product::Evaluated(reports),
                ..
            } => {
                self.evaluated = *reports;
                self.error = None;
            }
            Ending::Done(_) | Ending::Halfway { .. } => {}
            // **停下来的地方是干净的，就得这么说。** 上一趟算出来的那几个数照旧摆着
            // ——它们没有因为这一趟被停而变得不对。
            Ending::Stopped => {
                self.notice = Some(format!(
                    "算容量{}。这一趟整条只读——中立库、主库、目标设备一个字节都没动，\
                     再算一次就是。",
                    Ending::<()>::Stopped.render(),
                ));
                self.failed = false;
            }
            Ending::Failed { step, why } => {
                self.error = Some(format!(
                    "算一遍容量{}",
                    Ending::<()>::Failed { step, why }.render()
                ));
            }
        }
    }

    /// 同步那一趟回来了：**清单落回中立库**，然后把这一趟的账摆出来。
    ///
    /// **清单要在这一步写**——台上那条线拿的是只读连接，写不动（[`Product::Synced`]
    /// 的文档）。**被按停的那一趟也要落清单**：那份清单记的是「到中断为止目标上真实有
    /// 什么」，下一趟才接得上。
    fn settle_sync(&mut self, site: &mut Site, done: Finished<Product>) {
        let elapsed = done.elapsed.as_secs_f64();
        match done.ended {
            // **跑完的那一趟与「停在半路」那一趟走同一条**：后者交出来的清单同样是真的
            // ——它记着「到中断为止目标上真实有什么」，不落库下一趟就接不上（ADR-0015）。
            // 回执里那句「⚠️ 这一趟被你按停了」由 `sync_notice` 照 `interrupted` 印，
            // 与任务屏历史那一行说的是同一件事。
            Ending::Done(Product::Synced(outcome))
            | Ending::Halfway {
                product: Product::Synced(outcome),
                ..
            } => {
                if let Err(error) = site
                    .catalog
                    .put_manifest(&outcome.sublibrary, &outcome.manifest)
                {
                    self.error = Some(format!(
                        "⚠️ 清单写不回中立库：{error}\n\
                         目标上的文件已经动过了，而清单还是旧的那一份——下一趟同步会把这次\n\
                         放上去的东西当成「清单之外」，于是碰都不敢碰。先修好中立库再跑一次。"
                    ));
                }
                self.notice = Some(sync_notice(&outcome, elapsed));
                self.failed =
                    outcome.interrupted || outcome.gave_up || !outcome.failures.is_empty();
                // 传完之后那份预览说的已经是过去时了：目标现在是另一个样子。
                //
                // **只清掉这一台的那一份。** 台上排着队可能排上几十分钟，认领回来的时候
                // 屏上摊开的多半已经是另一台、摆着的是那一台刚排好的差量——那一份还没传呢，
                // 一并清掉等于让人白排一趟。
                if self
                    .prepared
                    .as_ref()
                    .is_some_and(|prepared| prepared.sublibrary.name == outcome.sublibrary)
                {
                    self.prepared = None;
                    self.acknowledged = false;
                }
                self.outcome = Some(*outcome);
            }
            Ending::Done(_) | Ending::Halfway { .. } => {}
            // **走到这儿的只有「还排着队就被撤掉」那一种**：真跑起来的那一趟被按停时
            // 照旧交出产物（[`Product::Synced`] 的文档），走的是上面那一支
            // ——那是「停了，留下了产物」，这一支是「停了，什么都没留下」。
            // 于是这一句敢说「一个字节都没动」。
            Ending::Stopped => {
                self.notice = Some(
                    "同步还没轮到就被撤掉了。目标设备上一个字节都没动，那份差量还摆着，\
                     再按一次同步就是。"
                        .to_string(),
                );
                self.failed = false;
            }
            Ending::Failed { step, why } => {
                self.error = Some(format!(
                    "同步{}",
                    Ending::<()>::Failed { step, why }.render()
                ));
            }
        }
    }

    /// **删减建议表上按「排除」**：把这个变体记成这台设备的一条**排除例外**，然后重算一遍容量。
    ///
    /// ADR-0016：超限只给建议，砍谁由人定。落的就是浏览屏详情面板里那种例外
    /// （`Catalog::set_exception`），不是第二套机制；**盘上的文件一个都不动**——主库只读
    /// （ADR-0004），卡上的文件等下一趟差量预览与同步照清单去对。
    ///
    /// 记完之后这一台缓着的差量与容量账全部作废（[`Self::forget`]），并且**当场重排一趟
    /// 「算一遍容量」**：人对着建议表一项项往下排除，下一项还要不要排由核心重新说，屏上不留
    /// 一份排除之前的账。
    ///
    /// 界面上按那颗按钮走的就是它，实测与测试拿它当那一下。
    pub fn exclude(&mut self, site: &mut Site, tasks: &mut Tasks, name: &str, key: &str) {
        match site
            .catalog
            .set_exception(name, key, Exception::Exclude, None)
        {
            Ok(()) => {
                // **收尾走与那层弹层同一条路**（[`Self::after_exception_changed`]）：缓着的差量与容量账
                // 作废、当场重算、转告浏览屏、差量失效说一句。这儿原先自己抄了一遍，于是同一条硬要求
                // （「改过例外之后差量预览失效**并说明**」）在两条路上不一样——删减建议这条静静地把
                // 预览作废掉、那句话不说（收尾审查 Spec 轴挑出，也是 ADR-0024 意义上的第二份实现）。
                self.after_exception_changed(site, tasks, name);
                self.notice = Some(format!(
                    "给「{name}」记下了一条排除例外：{key}。盘上的文件一个都没动；容量正在重算。"
                ));
            }
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 「**改选择**」：把这个子库的规则并成一条，交给窗口送去浏览屏。
    ///
    /// 这一下**什么都没写**——它只是把要改的东西装好（规则与例外的增减在浏览屏上做）。
    /// 真正的跳转由 [`crate::app::App::route`] 走：那儿才同时够得着两屏。
    ///
    /// 界面上按那个按钮走的就是它，实测与测试拿它当那一下。
    pub fn edit_selection(&mut self) {
        let Some(name) = self.picked.clone() else {
            self.error = Some("先摊开一张卡。".to_string());
            return;
        };
        // 读得懂的并成一条，读不懂的只数一数：它们本来就没参与求值，这一趟也不碰。
        let chosen = self.selections.get(&name).cloned().unwrap_or_default();
        let loaded = LoadedSelection::from_stored(&chosen.rules);
        self.jump = Some(Jump {
            sublibrary: name,
            rule: Rule::any_of(loaded.selection.rules),
            broken: chosen.broken,
            ordinal: None,
        });
    }

    /// 规则行上「**✎**」：把这一台的**第 `ordinal` 条**规则交给窗口送去浏览屏，只预填这一条；调完按「更新到子库」只换回
    /// 这一条（拿主意的人 2026-09-14 定）。与 [`Self::edit_selection`] 一样这一下什么都没写。
    ///
    /// 界面上按那颗按钮走的就是它，实测与测试拿它当那一下。
    pub fn edit_rule(&mut self, name: &str, ordinal: i64) {
        let chosen = self.selections.get(name).cloned().unwrap_or_default();
        let Some(rule) = chosen
            .rules
            .iter()
            .find(|stored| stored.ordinal == ordinal)
            .and_then(|stored| Rule::parse(&stored.text).ok())
        else {
            self.error = Some(format!(
                "子库「{name}」的第 {ordinal} 条规则不在了，或者读不懂。"
            ));
            return;
        };
        self.jump = Some(Jump {
            sublibrary: name.to_string(),
            rule: Some(rule),
            broken: chosen.broken,
            ordinal: Some(ordinal),
        });
    }

    /// 把「改选择」那一下取走。**取过就没了**：窗口一帧问一次。
    pub fn take_jump(&mut self) -> Option<Jump> {
        self.jump.take()
    }

    /// **这一屏刚动过哪个子库的例外**，取走那个记号（挂单 `Q812`）。**取过就没了**：窗口一帧问一次。
    ///
    /// 窗口拿它去转告浏览屏重读（`browse::Screen::exceptions_changed`）——那儿「改选择」
    /// 可能正开着同一个子库，手上缓着的是改之前那几条。
    pub fn take_touched(&mut self) -> Option<String> {
        self.touched.take()
    }

    /// **同步**：把差量真正落到目标设备上，往[任务台](crate::task)上排一趟。
    ///
    /// 四道闸一道都不能少：**得先有预览**（ADR-0016）、**目标得在位**（票
    /// `gui-looks-like-the-design/07`）、**有删除就得先点头**（ADR-0015）、
    /// **目标不许落在主库里**（ADR-0004，判据在核心里）。四道都过在**排它之前**——
    /// 排上去之后没人再看第二眼。
    ///
    /// ## 台上那一趟认的是排它时那份计划
    ///
    /// 那份 [`Prepared`] 与主库那一组根都是**整份交给**台上那趟活的（闭包自己拿着一份，
    /// 不是屏上这一份的借用）。于是排上去之后规则怎么改、屏上那份预览怎么作废，跑的仍然
    /// 是人点头时看过的那一份——ADR-0016 那句「同步前必须预览差量」因此照旧是构造上的
    /// 事实。**要把改过规则之后的那一批传上去，就得重排一次差量预览再按一次。**
    ///
    /// 界面上按那个按钮走的就是它，实测与测试拿它当那一下。
    pub fn sync(&mut self, site: &Site, tasks: &mut Tasks) {
        if self.syncing.is_some() {
            return;
        }
        let Some(prepared) = self.prepared.clone() else {
            // 这句话不是提示，是这一屏的规矩：没预览就没有可传的东西。
            self.error = Some("还没排过差量预览。先看一遍它要做什么。".to_string());
            return;
        };
        // **排完差量之后卡被拔了，就不排**：同步那一趟起手就把目标根建出来，卡拔了之后那个路径
        // 指着的是本机的盘——一份子库会被悄悄写进本机一个新建的空目录里。这是按下去之前就判得出
        // 的，只在屏上说（票 `gui-looks-like-the-design/07`）。**写到一半写不进**（卡满了、中途
        // 被拔）是跑起来才撞上的，照旧记失败（[`run_sync`]）。
        if let Some(why) = target_absent(&prepared.root) {
            self.error = Some(why);
            return;
        }
        if prepared.plan.deletes.files > 0 && !self.acknowledged {
            self.error = Some(format!(
                "这份计划里有 {} 个删除（{}）。看过上面的预览之后，勾上「我看过删除清单」再来。",
                thousands(prepared.plan.deletes.files),
                human_bytes(prepared.plan.deletes.bytes),
            ));
            return;
        }
        if let Err(message) =
            sync::prepare::refuse_target_in_library(&site.catalog, &[], &prepared.root)
        {
            self.error = Some(message);
            return;
        }
        // 搬 ROM 要真的去读主库——**只有真要搬时才需要**：一趟只删文件、只重写元数据的
        // 同步，盘不在位照样跑得完（ADR-0009）。
        let library_roots = if prepared.needs_library() {
            match sync::prepare::library_roots(&site.catalog, &[]) {
                Ok(roots) => {
                    // 只看这一趟真要搬的那几个根在不在位——**别的盘挂没挂上与这趟无关**。
                    let missing = sync::prepare::missing_roots(&prepared, &roots);
                    if !missing.is_empty() {
                        self.error = Some(sync::prepare::missing_roots_message(&missing));
                        return;
                    }
                    Some(roots)
                }
                Err(message) => {
                    self.error = Some(message);
                    return;
                }
            }
        } else {
            None
        };

        let name = prepared.sublibrary.name.clone();
        // **计划在这一刻整份交出去。** 闭包拿的是它自己那一份，屏上那一份此后作废也好、
        // 重排也好，都改不了台上这一趟要做的事（这条正是 ADR-0016 在搬上任务台之后
        // 还成立的原因）。
        self.syncing = Some(tasks.queue(format!("同步「{name}」"), move |task| {
            run_sync(&prepared, library_roots.as_ref(), task)
                .map(|outcome| Product::Synced(Box::new(outcome)))
        }));
        self.error = None;
        self.notice = None;
        self.failed = false;
    }

    /// 画一帧。
    ///
    /// **不必在这儿问「跑完没有」**：三条长活全在任务台上，窗口每帧问它一次
    /// （`App::poll_tasks`），台上有活时那一帧自己会请求下一帧。
    pub fn ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        // 屏头由窗口画（[`crate::look::screen_header`]：标题、副标题，右侧是 [`Self::status`]）；这一屏只画屏体
        // （[`look::screen_body`]：内边距照稿，整块竖着滚）。
        look::screen_body(ui, "子库屏体", |ui| self.cards_ui(ui, site, tasks));
        // 「目标设置」开着时盖在上面（[`crate::dialog`]）：遮罩盖住整个窗口，底下那一屏点不动。
        let ctx = ui.ctx().clone();
        self.read_footprint(site, tasks);
        self.target_dialog_ui(&ctx, site);
        self.count_strangers(site, tasks);
        // 「删除子库」那层确认弹层同一个路子；删掉之后底边那条提示条盖在最上面（[`crate::toast`]）。
        self.delete_dialog_ui(&ctx, site);
        self.rule_dialog_ui(&ctx, site);
        self.exceptions_dialog_ui(&ctx, site, tasks);
        self.toast_ui(&ctx, site);
    }

    /// 屏头右侧属于这一屏的那一段（窗口调进 [`crate::look::screen_header`]）：一颗小号幽灵按钮「重新列一遍」，
    /// 最右头一颗小号主按钮「新建子库」（设计稿 `.scrhead`）。
    ///
    /// **「重新列一遍」照留**（拿主意的人 2026-09-14 选 B，挂单 `Q896`）：窗口只在开窗与从浏览屏回来时重读这一屏，
    /// 命令行在窗口开着时改过的子库、刚插上的卡，不按它就要等下一趟活才看得见。原来顶栏上的「N 台设备」（左栏角标
    /// 已经写着）与台上那一趟的进度、「停止」（卡上按钮旁边与任务屏上都按得着）照稿拿掉。
    pub fn status(&mut self, ui: &mut egui::Ui, site: &Site) {
        look::small_buttons(ui, |ui| {
            if ghost_button(ui, "重新列一遍")
                .on_hover_text("重读中立库里的子库，并查一眼每台设备的目标在不在位。")
                .clicked()
            {
                self.reload(site);
            }
            // 「算一遍容量」挪进了每张卡「容量」那一段的抬头（挂单 `Q857`）。
            if primary_button(ui, "新建子库")
                .on_hover_text("填名字与目标路径。选择集去浏览屏筛：筛到满意按「存成子库」。")
                .clicked()
            {
                self.begin_new();
            }
        });
    }

    /// 台上那一趟的进度与「停下」，摆在按下它的那个按钮旁边。
    ///
    /// **人是在这一屏点的，不该逼他先切去任务屏才知道跑到哪儿了。** 摆的是任务台那一份
    /// 快照（与任务屏上那一条同一个来源）；还排着队没轮到时说清它在等——不然按钮灰着、
    /// 屏上一个字没有，看着就像按坏了。
    fn live_ui(ui: &mut egui::Ui, tasks: &mut Tasks, id: Option<u64>, 干什么: &str) {
        let Some(id) = id else {
            return;
        };
        let Some(live) = tasks.running().filter(|live| live.id == id) else {
            if tasks.queued().iter().any(|(queued, _)| *queued == id) {
                ui.weak(format!("{干什么}排在任务台上等着（第 {id} 号）"));
                if look::buttons(ui, |ui| ui.button("撤掉")).clicked() {
                    tasks.stop(id);
                }
            }
            return;
        };
        ui.weak(format!(
            "正在{干什么}：{}（已用 {:.1} 秒）",
            live.progress.render(),
            live.elapsed.as_secs_f64(),
        ));
        if live.stopping {
            ui.colored_label(ui.visuals().warn_fg_color, "正在停……");
        } else if look::buttons(ui, |ui| ui.button("停下")).clicked() {
            tasks.stop(id);
        }
    }

    /// 「**新建子库**」：打开「新建子库」那层弹层，草稿换成一份空的，没有哪一张卡算摊开着。
    fn begin_new(&mut self) {
        self.forget_strangers();
        self.vetted = None;
        self.name_vetted = None;
        self.sample_directory = None;
        self.picked = None;
        self.form = Form::default();
        self.invalidate();
        self.error = None;
        self.target_dialog = Some(TargetDialog::New);
    }

    /// 卡上「**目标设置…**」：草稿换成这一台的，打开那层弹层。**不摊开这张卡**：改目标设置与看差量是两件事，
    /// 弹层底下那张卡照旧是原来的样子（设计稿；第二段头一版在底下摊开了一张，露出了删除那一颗）。
    ///
    /// 界面上按那颗按钮走的就是它，实测与测试拿它当那一下。
    pub fn edit_target(&mut self, name: &str) {
        if let Some(sublibrary) = self.list.iter().find(|row| row.name == name) {
            self.form = Form::of(sublibrary);
        }
        self.form.overrides = self.overrides.get(name).cloned().unwrap_or_default();
        self.footprint_failed = None;
        self.forget_strangers();
        self.vetted = None;
        self.name_vetted = None;
        self.sample_directory = None;
        self.error = None;
        self.target_dialog = Some(TargetDialog::Of(name.to_string()));
    }

    /// 目录选择器交回来一个路径（票 `gui-answers-all-six/01` 那条薄封装，[`crate::pick::directory`]）：**填进目标路径那一格，
    /// 与贴进框里走同一条路**——下一帧照框里的字当场判一遍（`vet_form`）。取消（`None`）什么都不动。
    ///
    /// 界面上「选择…」交回来走的就是它，测试拿它当那一下（对话框那一层不测，理由在 `pick` 的模块文档里）。
    pub fn picked_target(&mut self, picked: Option<PathBuf>) {
        if let Some(path) = picked {
            self.form.target = romcat_core::path::display(&path);
        }
    }

    /// 目标设置弹层里平台表列哪几个平台：这一台选择集里出现的那几个（读回来的脚印，`sync::Footprint::platforms`）。
    /// 还在读、或者开的是「新建子库」时是 `None`。
    #[must_use]
    pub fn target_platforms(&self) -> Option<Vec<String>> {
        let Some(TargetDialog::Of(name)) = &self.target_dialog else {
            return None;
        };
        self.footprint
            .as_ref()
            .filter(|(of, _)| of == name)
            .map(|(_, footprint)| footprint.platforms())
    }

    /// 「目标设置…」开着、手上还没有这一台的脚印时，往任务台上排一趟**不留历史**的活去读（`sync::prepare::footprint`，
    /// `Board::queue_quiet`：打开弹层时顺带跑的，不是人点起来的一趟，拿主意的人 2026-09-15 定）。读过的、正在读的、
    /// 这回读失败过的都不重排。**折事实走一遍全库**，所以不在画帧那条线程上读（与 [`Self::evaluate`] 同一条路）。
    fn read_footprint(&mut self, site: &Site, tasks: &mut Tasks) {
        let Some(TargetDialog::Of(name)) = self.target_dialog.clone() else {
            return;
        };
        let have = self.footprint.as_ref().is_some_and(|(of, _)| *of == name);
        let reading = self
            .reading_footprint
            .as_ref()
            .is_some_and(|(_, of)| *of == name);
        if have || reading || self.footprint_failed.as_deref() == Some(name.as_str()) {
            return;
        }
        let title = format!("读「{name}」的选择集");
        let id = match site.catalog.read_only() {
            Ok(reader) => {
                let of = name.clone();
                tasks.queue_quiet(title, move |task| {
                    sync::prepare::footprint(&reader, &of, task)
                        .map(|footprint| Product::Footprint(Box::new(footprint)))
                })
            }
            Err(CatalogError::NotOnDisk { .. }) => tasks.run_here_quiet(title, |task| {
                sync::prepare::footprint(&site.catalog, &name, task)
                    .map(|footprint| Product::Footprint(Box::new(footprint)))
            }),
            Err(why) => {
                self.footprint_failed = Some(name);
                self.error = Some(no_second_connection("读选择集", &why));
                return;
            }
        };
        self.reading_footprint = Some((id, name));
    }

    /// 读脚印那一趟回来了。
    fn settle_footprint(&mut self, name: String, done: Finished<Product>) {
        match done.ended {
            Ending::Done(Product::Footprint(footprint))
            | Ending::Halfway {
                product: Product::Footprint(footprint),
                ..
            } => self.footprint = Some((name, *footprint)),
            Ending::Done(_) | Ending::Halfway { .. } => {}
            Ending::Stopped => self.footprint_failed = Some(name),
            Ending::Failed { step, why } => {
                self.footprint_failed = Some(name);
                self.error = Some(format!(
                    "读选择集{}",
                    Ending::<()>::Failed { step, why }.render()
                ));
            }
        }
    }

    /// 钉死平台表判「陈旧」用的「今天」（`YYYY-MM-DD`）：截图测试钉死它，截图里才没有当前日期。
    pub fn set_today(&mut self, today: &str) {
        self.today = Some(today.to_string());
    }

    /// 眼下挑的档案叠上覆盖之后，这一台选择集**放不下哪几份**（`Footprint::too_big`）。档案名与覆盖都没变就照用上一回的。
    fn refresh_too_big(&mut self, profile: Option<&Profile>) {
        let editing = match &self.target_dialog {
            Some(TargetDialog::Of(name)) => name.as_str(),
            _ => {
                self.too_big = None;
                return;
            }
        };
        let (Some(profile), Some((of, footprint))) = (profile, self.footprint.as_ref()) else {
            self.too_big = None;
            return;
        };
        if of != editing {
            self.too_big = None;
            return;
        }
        let fresh = self.too_big.as_ref().is_some_and(|(name, overrides, _)| {
            *name == profile.name && *overrides == self.form.overrides
        });
        if fresh {
            return;
        }
        let rows = footprint.too_big(&profile.with_overrides(&self.form.overrides));
        self.too_big = Some((profile.name.clone(), self.form.overrides.clone(), rows));
    }

    /// 丢掉上一回数的清单外文件数：重开弹层时重数（设备可能换过、拷进去过东西）。
    fn forget_strangers(&mut self) {
        self.strangers = None;
        self.counting = None;
        self.counting_failed = None;
    }

    /// 目标设置开着、框里那条路径**此刻在位**、还没数过它时，往任务台上排一趟**只读、不留历史**的活数清单外文件
    /// （`sync::prepare::strangers_at`，拿主意的人 2026-09-15 定）。清单是正在改的那一台的；新建时还没有清单，卡上的都算。
    /// 路径的字改了、或者判出来从不在变成在（设备重新连上），数的那一串对不上，就重数。
    fn count_strangers(&mut self, site: &Site, tasks: &mut Tasks) {
        if self.target_dialog.is_none() {
            return;
        }
        let Some(vetted) = self.vetted.as_ref() else {
            return;
        };
        if !matches!(vetted.verdict, Ok(Ok(Presence::Present(_)))) {
            return;
        }
        let text = vetted.text.clone();
        let done = self.strangers.as_ref().is_some_and(|(of, _)| *of == text);
        let running = self.counting.as_ref().is_some_and(|(_, of)| *of == text);
        if done || running || self.counting_failed.as_deref() == Some(text.as_str()) {
            return;
        }
        let name = self
            .editing()
            .unwrap_or_else(|| self.form.name.trim().to_string());
        let title = "数目标上清单之外的文件".to_string();
        let path = PathBuf::from(&text);
        let id = match site.catalog.read_only() {
            Ok(reader) => tasks.queue_quiet(title, move |task| {
                sync::prepare::strangers_at(&reader, &name, &path, task).map(Product::Strangers)
            }),
            Err(CatalogError::NotOnDisk { .. }) => tasks.run_here_quiet(title, |task| {
                sync::prepare::strangers_at(&site.catalog, &name, &path, task)
                    .map(Product::Strangers)
            }),
            Err(why) => {
                self.counting_failed = Some(text);
                self.error = Some(no_second_connection("数清单之外的文件", &why));
                return;
            }
        };
        self.counting = Some((id, text));
    }

    /// 数清单外文件那一趟回来了。
    fn settle_strangers(&mut self, text: String, done: Finished<Product>) {
        match done.ended {
            Ending::Done(Product::Strangers(counted))
            | Ending::Halfway {
                product: Product::Strangers(counted),
                ..
            } => self.strangers = Some((text, counted)),
            Ending::Done(_) | Ending::Halfway { .. } | Ending::Stopped | Ending::Failed { .. } => {
                self.counting_failed = Some(text);
            }
        }
    }

    /// 不存、关上「目标设置」那层弹层：页脚上「取消」、Esc 走的就是它，测试拿它当那一下。
    pub fn leave_target_settings(&mut self) {
        self.target_dialog = None;
        self.error = None;
    }

    /// 这张卡此刻读得出的**总量**：上一回判目标路径时（[`Self::vet_form`]）目标在位才有。
    fn live_total(&self) -> Option<u64> {
        match self.vetted.as_ref().map(|vetted| &vetted.verdict) {
            Some(Ok(Ok(Presence::Present(volume)))) => volume.total,
            _ => None,
        }
    }

    /// 正在改的那一台**上次读到的总量**：它原来就在按设备容量那一档时，库里 `capacity` 记的就是它。
    fn stored_total(&self) -> Option<u64> {
        let editing = self.editing()?;
        self.list
            .iter()
            .find(|row| row.name == editing && row.capacity_by_device)
            .and_then(|row| row.capacity)
    }

    /// 眼下正在改的是哪一台：「目标设置…」开的那一台；「新建子库」是 `None`；弹层没开时是摊开的那一张
    /// （原先那块「配目标」面板改的就是摊开那一台，程序里直接调 [`Self::save`] 的照旧这么认）。
    fn editing(&self) -> Option<String> {
        match &self.target_dialog {
            Some(TargetDialog::Of(name)) => Some(name.clone()),
            Some(TargetDialog::New) => None,
            None => self.picked.clone(),
        }
    }

    /// 草稿里的名字与目标路径**各判一遍**，字与正在改的那一台都没变就照用上一回的（[`Vetted`]、[`NameVetted`]）。
    /// 判断全在核心（[`target::vet`] / [`target::vet_name`]），这里只记下来。
    fn vet_form(&mut self, site: &Site) {
        let editing = self.editing();
        let text = self.form.target.trim().to_string();
        if self
            .vetted
            .as_ref()
            .is_none_or(|last| last.text != text || last.editing != editing)
        {
            let verdict = target::vet(
                &site.catalog,
                &self.workspace,
                editing.as_deref(),
                std::path::Path::new(&text),
            )
            .map_err(|error| format!("中立库读不动：{error}"));
            self.vetted = Some(Vetted {
                text,
                editing: editing.clone(),
                verdict,
            });
        }
        let name = self.form.name.trim().to_string();
        if self
            .name_vetted
            .as_ref()
            .is_none_or(|last| last.text != name || last.editing != editing)
        {
            let verdict = target::vet_name(&site.catalog, editing.as_deref(), &name)
                .map_err(|error| format!("中立库读不动：{error}"));
            self.name_vetted = Some(NameVetted {
                text: name,
                editing,
                verdict,
            });
        }
    }

    /// 草稿过没过那两道判（[`Self::vet_form`] 之后问）：名字与目标路径都能用才算过。
    fn form_ready(&self) -> bool {
        matches!(
            self.name_vetted.as_ref().map(|vetted| &vetted.verdict),
            Some(Ok(Ok(())))
        ) && matches!(
            self.vetted.as_ref().map(|vetted| &vetted.verdict),
            Some(Ok(Ok(_)))
        )
    }

    /// 中间那一列：**一台设备一张卡**。
    fn cards_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if let Some(notice) = &self.notice {
            let color = if self.failed {
                ui.visuals().error_fg_color
            } else {
                ui.visuals().warn_fg_color
            };
            ui.colored_label(color, notice);
        }
        if self.list.is_empty() {
            // **空态，不是示例设备**（票 `gui-looks-like-the-design/20`）：设计稿脚本里那两台是给稿子
            // 看的数据。一台都没有时说清子库是什么，给一颗「新建子库」。
            if empty_ui(ui) {
                self.begin_new();
            }
            // 空态卡底下照稿也是那一行（稿上 `margin-top:14px`，间距档里最近的是 12）。
            ui.add_space(step(3));
            look::help(ui, TRIM_HELP);
            return;
        }
        let names: Vec<String> = self.list.iter().map(|row| row.name.clone()).collect();
        // **两列**（设计稿 `.devs`）；窄到并排放不下两张卡时一列。一张卡至少多宽取令牌里弹层的
        // 头一档——卡上那几行（规则、容量条图例、差量账）在那个宽度上摆得开。
        let columns = if ui.available_width() >= 2.0 * Tokens::builtin().layout.dialog_width[0] {
            2
        } else {
            1
        };
        // 竖着滚的是整块屏体（[`look::screen_body`]），这里不再套一层滚动区。
        ui.columns(columns, |cols| {
            for (at, name) in names.iter().enumerate() {
                let col = &mut cols[at % columns];
                self.card_ui(col, site, tasks, name);
                col.add_space(step(3));
            }
        });
        // 卡片底下那一行帮助字（设计稿子库屏）：超限只给建议，排除记成手动例外（ADR-0016）。
        look::help(ui, TRIM_HELP);
    }

    /// 一台设备那一张卡。
    fn card_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks, name: &str) {
        let Some(sublibrary) = self.list.iter().find(|row| row.name == name).cloned() else {
            return;
        };
        let open = self.picked.as_deref() == Some(name);
        // 卡片（设计稿 `.dev`：内边距 16）照 `look::card` 那一处画，底色、描边、圆角都从令牌来。
        look::card(ui, egui::Vec2::splat(step(3)), |ui| {
            ui.horizontal(|ui| {
                ui.label(font::strong(&sublibrary.name).size(Tokens::builtin().font.size_title));
                self.state_chip_ui(ui, &sublibrary);
                // 右边两颗照稿（设计稿 `devCard`）：`btn sm pri` 在左、`btn sm ghost`「目标设置…」靠右。
                // 主按钮的字照稿写「从浏览添加…」（拿主意的人定，挂单 `Q892`），行为不变：跳去浏览屏、
                // 这一台的规则预填进筛选器。
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    look::small_buttons(ui, |ui| {
                        if ghost_button(ui, "目标设置…")
                            .on_hover_text(
                                "改这台设备的名字、目标路径、前端格式、容量上限与能力档案。",
                            )
                            .clicked()
                        {
                            self.edit_target(name);
                        }
                        // 它带走的是「这一台的规则」，规则要等 `open` 读回来才在手上——没摊开的先摊开。
                        if primary_button(ui, "从浏览添加…")
                            .on_hover_text(
                                "跳去浏览屏，这个子库的规则预填进筛选器。\
                                 在那儿改得见它真的筛出了什么；调完按「更新到子库」原样带回。",
                            )
                            .clicked()
                        {
                            if !open {
                                self.open(site, name);
                            }
                            self.edit_selection();
                        }
                    });
                });
            });
            self.head_ui(ui, &sublibrary);
            ui.add_space(step(2));
            // 规则行尾按了「✎」就去浏览屏只改那一条；按了「×」先问一层（「移除规则」那层弹层）。那一层只画。
            match self.selection_ui(ui, name) {
                Some(RulePressed::Edit(ordinal)) => self.edit_rule(name, ordinal),
                Some(RulePressed::Remove(ordinal)) => self.ask_remove_rule(name, ordinal),
                Some(RulePressed::ManageExceptions) => self.open_exceptions(site, name),
                None => {}
            }
            ui.add_space(step(2));
            // 「容量」那一段的抬头：左边小标题，右边「算一遍容量」——从屏头挪下来的（挂单 `Q857`）。
            // 按下去照旧一趟算全部设备；台上那一趟的进度与「停下」摆在它旁边。
            ui.horizontal(|ui| {
                look::section(ui, "容量");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let 按了 = look::small_buttons(ui, |ui| {
                        ui.add_enabled(self.evaluating.is_none(), egui::Button::new("算一遍容量"))
                            .on_hover_text(
                                "每台设备的选择集各选出多少、多大，装不装得下要看一眼那张卡——\
                                 卡不在手边的那台如实说算不出。\
                                 它排到任务台上跑，期间这一屏照常用。",
                            )
                            .clicked()
                    });
                    if 按了 {
                        self.evaluate(site, tasks);
                    }
                    Self::live_ui(ui, tasks, self.evaluating, "算容量");
                });
            });
            // 删减建议表上按了「排除」：写库与重算在这一层做，画容量条的那一层不动库。
            if let Some(key) = self.gauge_ui(ui, name) {
                self.exclude(site, tasks, name, &key);
            }
            ui.add_space(step(2));
            if open {
                self.delta_ui(ui, site, tasks);
                return;
            }
            // 没排过差量预览、目标又在位时，照稿先摆一条提示框（设计稿 `diffHTML` 的 `.note`）。
            if !self.absent.contains(name) {
                note_ui(ui, PREVIEW_NOTE);
                ui.add_space(step(2));
            }
            // **差量预览一次只摆一台的**（它是同步认的那一份计划，见 `prepared`）。别的卡底下照稿摆一排
            // 「生成差量预览 / 同步 / 删除子库」，按哪一颗都先换成这一台（[`Self::actions_ui`]）。
            self.actions_ui(ui, site, tasks, name, false);
        });
    }

    /// 卡底那一排（设计稿 `devCard` 底下）：「生成差量预览」「同步」，卡不在位时旁边一句「请先连接设备」，
    /// 右头一颗小号「删除子库」。
    ///
    /// **按哪一颗都先把这张卡摊开**（拿主意的人 2026-09-14 定，挂单 `Q859`）：差量预览、同步认的都是摊开那一台，
    /// 删除那层弹层问的也是它。**摊开那一张不在这一排摆「同步」**：它的「同步」在差量底下，与「我看过删除清单」
    /// 那一格摆在一起（[`Self::sync_ui`]）。
    ///
    /// 卡不在位时只写「请先连接设备」（照稿，拿主意的人定），路径与怎么办放进悬停；算过容量的话核心那句原话
    /// 也在悬停里（挂单 `Q851`）。**按钮不因为卡不在位就按不动**：拦在 `preview` 与 `sync` 里、只在屏上说
    /// （ADR-0005，票 `gui-looks-like-the-design/07`）。
    ///
    /// **包进横排**：卡片摆在 `ui.columns` 分出来的那一栏里，那一栏的版式是「撑满」，直接往里摆一颗按钮会被
    /// 拉成整栏宽（第二段头一版候选图上就是这样）。
    fn actions_ui(
        &mut self,
        ui: &mut egui::Ui,
        site: &mut Site,
        tasks: &mut Tasks,
        name: &str,
        open: bool,
    ) {
        /// 卡底按下去的是哪一颗。
        enum Pressed {
            /// 「生成差量预览」。
            Preview,
            /// 「同步」。
            Sync,
            /// 「删除子库」。
            Remove,
        }
        let mut pressed = None;
        let busy = self.syncing.is_some() || self.previewing.is_some();
        ui.horizontal(|ui| {
            // 默认那一档按钮照稿（设计稿 `.btn`，字取半号：[`look::buttons`]）。
            look::buttons(ui, |ui| {
                if ui
                    .add_enabled(!busy, egui::Button::new("生成差量预览"))
                    .on_hover_text(
                        "只读：中立库读一遍、目标设备看一遍，一个文件都不写。\
                     别的卡上摆着的那份差量会换成这一台的。它排到任务台上跑，期间这一屏照常用。",
                    )
                    .clicked()
                {
                    pressed = Some(Pressed::Preview);
                }
                if open {
                    // 正排着的时候把进度摆在按钮旁边：人是在这一屏点的，不该逼他先切去任务屏
                    // 才知道排到哪儿了。**停下也在这儿按得着。**
                    Self::live_ui(ui, tasks, self.previewing, "排差量");
                } else if ui
                    .add_enabled_ui(!busy, |ui| primary_button(ui, "同步"))
                    .inner
                    .on_hover_text(
                        "把这一台排过的那份差量真的落到目标设备上，只碰清单里记录过的文件。\
                     没排过差量预览的话先说一句，一个文件都不动。",
                    )
                    .clicked()
                {
                    pressed = Some(Pressed::Sync);
                }
            });
            if let Some(sublibrary) = self.list.iter().find(|row| row.name == name)
                && self.absent.contains(name)
            {
                let mut 悬停 = format!(
                    "未连接：{}。插上读卡器，或者按卡上「目标设置…」换一个目录。",
                    sublibrary.target
                );
                if let Some(Fit::Unknown { why }) =
                    self.evaluated.get(name).map(|report| &report.fit)
                {
                    悬停.push_str("\n\n");
                    悬停.push_str(why);
                }
                look::help(ui, "请先连接设备").on_hover_text(悬停);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if look::small_buttons(ui, |ui| {
                    ui.scope(|ui| {
                        look::warn_button(ui.visuals_mut());
                        ui.button("删除子库")
                    })
                    .inner
                })
                .on_hover_text(
                    "删掉这个子库的定义与它的规则、例外、清单；设备上的文件一个字节都不动。\
                         按下去先问一层。",
                )
                .clicked()
                {
                    pressed = Some(Pressed::Remove);
                }
            });
        });
        match pressed {
            Some(Pressed::Preview) => {
                if !open {
                    self.open(site, name);
                }
                self.preview(site, tasks);
            }
            Some(Pressed::Sync) => {
                if !open {
                    self.open(site, name);
                }
                self.sync(site, tasks);
            }
            Some(Pressed::Remove) => self.ask_remove(site, name),
            None => {}
        }
    }

    /// 卡底「**删除子库**」：先把这张卡摊开，再打开那层确认弹层（按下去真删走 [`Self::remove`]）。
    ///
    /// 界面上按那颗按钮走的就是它。
    pub fn ask_remove(&mut self, site: &Site, name: &str) {
        if self.picked.as_deref() != Some(name) {
            self.open(site, name);
        }
        self.delete_dialog = Some(name.to_string());
    }

    /// 「删除子库」那层确认弹层开着没有。
    #[must_use]
    pub fn delete_dialog_open(&self) -> bool {
        self.delete_dialog.is_some()
    }

    /// 卡头那枚**状态标签**（设计稿 `devState`），颜色照稿：
    ///
    /// - **未连接**——中性色、带圆点（稿上 `t-none`）。字照稿：拿主意的人裁了，词表里原来那个**连接**
    ///   （规则组里那几项怎么算数）改名（挂单 `Q853`）。**先看它**：稿上也是先问连没连着。
    /// - **待同步 N 步**——强调色、不带圆点（`t-acc plain`）。排过差量的那一台才有。
    /// - **已同步 · 清单 N 条**——放心色、不带圆点（`t-hi plain`）。排过差量、一步都不用做的那一台；N 是这一台
    ///   清单记着几条（[`Self::read_manifests`]）。清单读不动或者还是空的时候退回「已经对齐」，不编一个数。
    /// - **尚未生成差量预览**——强调色、不带圆点。没排过就说没排过，不摆一个 0——「一步都不用做」与
    ///   「还不知道要做什么」是两件事。
    fn state_chip_ui(&self, ui: &mut egui::Ui, sublibrary: &Sublibrary) {
        let name = sublibrary.name.as_str();
        if self.absent.contains(name) {
            look::chip(ui, look::Tone::Neutral, "未连接").on_hover_text(format!(
                "上一回看的时候 {} 不在。插上读卡器，或者按「目标设置…」换一个目录。",
                sublibrary.target
            ));
            return;
        }
        match self.plan_of(name) {
            Some(plan) if plan.touched() > 0 => {
                look::plain_chip(
                    ui,
                    look::Tone::Accent,
                    &format!("待同步 {} 个文件", thousands(plan.touched())),
                )
                .on_hover_text("排过差量预览：这一趟要动这么多个文件（一个文件一步）。");
            }
            Some(_) => match self.manifest_rows.get(name) {
                Some(rows) => {
                    look::plain_chip(
                        ui,
                        look::Tone::Good,
                        &format!("已同步 · 清单 {} 条", thousands(*rows as u64)),
                    )
                    .on_hover_text(
                        "排过差量预览：目标已经和选择集对齐，一步都不用做。\
                         清单记着上次同步放上去的这么多个文件。",
                    );
                }
                None => {
                    look::plain_chip(ui, look::Tone::Good, "已经对齐")
                        .on_hover_text("排过差量预览：目标已经和选择集对齐，一步都不用做。");
                }
            },
            None => {
                look::plain_chip(ui, look::Tone::Accent, "尚未生成差量预览").on_hover_text(
                    "同步前必须先看一遍它要做什么——那是硬要求，不是可以跳过的一步。",
                );
            }
        }
    }

    /// 卡头名字底下那一行：**路径 · 前端格式 · 文件系统 · 能力档案**（设计稿 `devCard`）。
    ///
    /// 档案与文件系统是名册解出来的那一份（[`Self::reload`]）。记着的名字在名册里没有时，
    /// 真会用上的是「不作声称」——照实写出来，并点名记着的是哪个：不说的话人会以为那份
    /// 档案生效了（ADR-0017：矩阵错了比不转换更糟）。
    fn head_ui(&self, ui: &mut egui::Ui, sublibrary: &Sublibrary) {
        let Some(profiled) = self.profiled.get(&sublibrary.name) else {
            ui.label(
                font::mono(format!(
                    "{} · {}",
                    sublibrary.target,
                    format_label(&sublibrary.format)
                ))
                .size(Tokens::builtin().font.size_small)
                .weak(),
            );
            if let Some(why) = &self.roster_error {
                ui.colored_label(ui.visuals().warn_fg_color, why);
            }
            return;
        };
        ui.label(
            font::mono(format!(
                "{} · {} · {} · 能力档案：{}",
                sublibrary.target,
                format_label(&sublibrary.format),
                profiled.filesystem,
                profiled.profile,
            ))
            // 设计稿 `.mono` 是正文的 0.92 倍，落在说明字号那一档。
            .size(Tokens::builtin().font.size_small)
            .weak(),
        );
        if let Some(recorded) = &profiled.missing {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!(
                    "名册里没有「{recorded}」这份能力档案，眼下按「{}」走。",
                    profiled.profile
                ),
            );
        }
    }

    /// 排出来那份计划，若它正好是这台设备的。
    fn plan_of(&self, name: &str) -> Option<&romcat_core::sync::Plan> {
        self.prepared
            .as_ref()
            .filter(|prepared| prepared.sublibrary.name == name)
            .map(|prepared| &prepared.plan)
    }

    /// **选择集**那一块（设计稿 `.rules`）：每条规则的名称、条件、命中多少、多大，例外那一行，
    /// 去重之后的合计。**每张卡都摆**，不只摊开那一张。
    ///
    /// 数整份来自核心折的 [`SelectionReport`]（与 `romcat sublibrary show` 印的是同一个值），
    /// 按过「算一遍容量」才有；没算过的那几格写「—」，不写 0——「还没算」与「一个都没选中」
    /// 是两件事。**名称是从条件拼出来的短名**（[`Rule::label`]，照设计稿 `autoName`）：中立库里一条规则
    /// 只存原文与序号，没有名字那一列（挂单 `Q811`）。第二行印规则原文。
    ///
    /// 改规则按卡头「从浏览添加…」去浏览屏：在那儿改得见它真的筛出了什么（票 `gui-redesign/11`）。
    fn selection_ui(&self, ui: &mut egui::Ui, name: &str) -> Option<RulePressed> {
        // 规则行尾「×」这一帧按了哪一条（序号）。**这一层只画**：问一层、删规则由卡片那一层做。
        let mut 按了 = None;
        let chosen = self.selections.get(name);
        let rules = chosen.map_or(&[][..], |chosen| &chosen.rules[..]);
        let broken = chosen.map_or(&[][..], |chosen| &chosen.broken[..]);
        let exceptions = chosen.map_or(&[][..], |chosen| &chosen.exceptions[..]);
        let report = self.evaluated.get(name);
        let caption = Tokens::builtin().font.size_caption;
        let visuals = ui.visuals().clone();
        egui::Frame::new()
            .stroke(visuals.widgets.noninteractive.bg_stroke)
            .corner_radius(visuals.window_corner_radius)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                // 行与行之间不留缝：分隔线就是缝。
                ui.spacing_mut().item_spacing.y = 0.0;
                egui::Frame::new()
                    .fill(visuals.faint_bg_color)
                    .corner_radius(egui::CornerRadius {
                        sw: 0,
                        se: 0,
                        ..visuals.window_corner_radius
                    })
                    .inner_margin(block_margin())
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let mut head = format!(
                                "选择集 · {} 条规则",
                                thousands((rules.len() + broken.len()) as u64)
                            );
                            if !exceptions.is_empty() {
                                head.push_str(&format!(
                                    " · {} 条例外",
                                    thousands(exceptions.len() as u64)
                                ));
                            }
                            look::section(ui, &head);
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                look::help(ui, "规则之间取并集");
                            });
                        });
                    });
                if rules.is_empty() && broken.is_empty() {
                    block_row(
                        ui,
                        "·",
                        |ui| {
                            ui.label(
                                egui::RichText::new("还没有规则。点「从浏览添加…」，按平台筛选后加入。")
                                    .size(look::font_size(ui.ctx(), Tokens::builtin().font.size_small_plus))
                                    .weak(),
                            );
                        },
                        |_| {},
                    );
                }
                for stored in rules {
                    // **逐条各自算，不扣例外也不扣重叠**：这个数回答的是「我这条规则写对了吗」。
                    let line = report.and_then(|report| {
                        report
                            .rules
                            .iter()
                            .find(|line| line.ordinal == stored.ordinal)
                    });
                    block_row(
                        ui,
                        &stored.ordinal.to_string(),
                        |ui| {
                            ui.label(font::strong(
                                chosen
                                    .and_then(|chosen| chosen.labels.get(&stored.ordinal))
                                    .cloned()
                                    .unwrap_or_else(|| format!("规则 {}", stored.ordinal)),
                            ));
                            ui.label(font::mono(&stored.text).size(caption).weak());
                        },
                        |ui| {
                            // 行尾照稿两颗图标按钮「✎」「×」（设计稿 `devCard` 的 `.ra`）：右起先摆的在最右头，数量与容量在它
                            // 左边，再往左是「✎」（拿主意的人 2026-09-14 要求，挂单 `Q893`）。
                            if look::icon_button(ui, "×")
                                .on_hover_text("按了这条规则")
                                .clicked()
                            {
                                按了 = Some(RulePressed::Remove(stored.ordinal));
                            }
                            if look::pencil_button(ui)
                                .on_hover_text("在浏览中编辑这条规则")
                                .clicked()
                            {
                                按了 = Some(RulePressed::Edit(stored.ordinal));
                            }
                            match line {
                            Some(line) => {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} 个 · {}",
                                        thousands(line.hits),
                                        human_bytes(line.bytes)
                                    ))
                                    .small(),
                                )
                                .on_hover_text(
                                    "这一条自己命中多少个变体、一共多大——不扣例外，也不扣与别条重叠的。",
                                );
                            }
                            None => {
                                ui.weak("—").on_hover_text("按「算一遍容量」之后才有。");
                            }
                            }
                        },
                    );
                }
                for row in broken {
                    block_row(
                        ui,
                        &row.ordinal.to_string(),
                        |ui| {
                            ui.label(font::strong(format!("规则 {}", row.ordinal)));
                            ui.colored_label(
                                visuals.error_fg_color,
                                format!(
                                    "{}. {}（读不懂：{}）——少选出来的东西全在它里面；\
                                     它没参与求值，「从浏览添加…」那一趟也不会碰它",
                                    row.ordinal, row.text, row.error,
                                ),
                            );
                        },
                        |_| {},
                    );
                }
                let (包含, 排除) = exception_tally(exceptions);
                block_row(
                    ui,
                    "+",
                    |ui| {
                        if exceptions.is_empty() {
                            ui.label(egui::RichText::new("没有手动例外").size(look::font_size(ui.ctx(), Tokens::builtin().font.size_small_plus)).weak());
                            return;
                        }
                        let 顶用的 = report.map_or_else(String::new, |report| {
                            format!(
                                "（其中 {} 条包含是多余的、{} 条排除真起了作用）",
                                thousands(report.forced_in_redundant),
                                thousands(report.forced_out_effective),
                            )
                        });
                        ui.label(
                            egui::RichText::new(format!(
                                "手动例外：包含 {} 条、排除 {} 条{顶用的}",
                                thousands(包含),
                                thousands(排除),
                            ))
                            .size(look::font_size(ui.ctx(), Tokens::builtin().font.size_small_plus)),
                        )
                        .on_hover_text(
                            "优先于规则、永久记住：规则表达不了的个人口味。\
                             按右边「管理」逐条增撤。",
                        );
                    },
                    |ui| {
                        // 行尾照稿一颗幽灵按钮「管理」（设计稿 `devCard` 的 `.rule.exc`）：**没有例外时也摆**
                        // ——那时屏上写「没有手动例外」，而人正是要从这儿加第一条。
                        if look::small_ghost_button(ui, "管理")
                            .on_hover_text("包含与排除两栏：逐条看、逐条撤，搜作品直接加。")
                            .clicked()
                        {
                            按了 = Some(RulePressed::ManageExceptions);
                        }
                    },
                );
                ui.add(egui::Separator::default().spacing(0.0));
                egui::Frame::new()
                    .inner_margin(block_margin())
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| match report {
                            Some(report) => {
                                ui.label(font::strong(format!(
                                    "合计 {} 个变体 · {}",
                                    thousands(report.picked),
                                    human_bytes(report.bytes),
                                )).size(look::font_size(ui.ctx(), Tokens::builtin().font.size_small_plus)));
                                // 被几条规则同时选中、只算一次的有几个由核心数（`Selected::overlaps`），
                                // 这儿只照着写。话照设计稿（拿主意的人定）：这里的「重复」说的是几条规则
                                // 选中了同一个变体，与词表**重复拷贝**不是一回事。
                                ui.weak(if report.overlaps > 0 {
                                    format!(
                                        "已去除 {} 个被多条规则同时选中的变体",
                                        thousands(report.overlaps),
                                    )
                                } else {
                                    "规则之间没有重复".to_string()
                                });
                            }
                            None => {
                                ui.label(font::strong("合计 —").size(look::font_size(ui.ctx(), Tokens::builtin().font.size_small_plus)));
                                ui.weak("按「算一遍容量」之后才有");
                            }
                        });
                    });
            });
        if !broken.is_empty() {
            // **处置它的那条路在浏览屏上**（票 `gui-redesign/14`，挂单 `Q86`）：筛选器摆的是一棵
            // 读得懂的树，一条读不回来的原文在那儿没有位置，所以它跟着「改选择」那一趟整条带过去，
            // 摆在**筛选栏顶上**那条横幅里逐条扔。
            ui.weak(format!(
                "读不懂的这 {} 条要扔掉：按上面「从浏览添加…」跳去浏览屏，\
                 筛选栏顶上那条横幅里逐条扔得掉。改对了再来一条，走那儿的筛选器。",
                broken.len(),
            ))
            .on_hover_text(
                "命令行那条路也还在：`romcat sublibrary rule <子库> --remove <序号>`，\
                 序号就是上面印着的那个。",
            );
        }
        let Some(report) = report else {
            return 按了;
        };
        if !report.missing_exceptions.is_empty() {
            // **不是错误，也不删**：盘没插、目录改了名，例外照旧记着（ADR-0016）。
            ui.weak(format!(
                "有 {} 条例外指着库里眼下没有的变体——照旧记着，不删。",
                thousands(report.missing_exceptions.len() as u64),
            ));
        }
        if !report.thin_dimensions.is_empty() {
            // **「选不出东西」有两个原因，得分得开**：规则写错了，还是这份库里根本
            // 没有这一维的数据。不说的话人会去改规则，而问题在刮削还没跑。
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!(
                    "规则引到了这几维，而这份库里一条数据都没有：{}。选不出东西是\
                     缺数据，不是规则写错了。",
                    report.thin_dimensions.join("、"),
                ),
            );
        }
        按了
    }

    /// **容量条**：选中的、清单之外的、上限，三段各自标得出数；超限时底下是删减建议表。
    ///
    /// 返回删减建议表上这一帧按了「排除」的那个变体的键——**这一层只画**，写库与重算由卡片那一层
    /// 调 [`Self::exclude`]。
    fn gauge_ui(&self, ui: &mut egui::Ui, name: &str) -> Option<String> {
        let gauge = self.gauge(name);
        // 超没超由核心一处算（同步计划器），条子只拿它换选中那一段的颜色（`room_of`）。
        let room = self.room_of(name);
        let over = room.as_ref().and_then(|room| room.over_capacity).is_some();
        // 选中那一段**算过才有数**（算过容量或排过差量）：没算过写「还没算」，不写 0。
        let counted = room.is_some() || self.evaluated.contains_key(name);
        let visuals = ui.visuals().clone();
        let 选中色 = if over {
            visuals.error_fg_color
        } else {
            visuals.selection.stroke.color
        };
        let 之外色 = visuals.warn_fg_color;
        let 未知描边 = visuals.widgets.inactive.bg_stroke;
        // 条高照稿（设计稿 `.gauge` 的 10 点，令牌 `gauge-height`）。「容量」那个小标题摆在卡片那一层的抬头里。
        let height = Tokens::builtin().layout.gauge_height;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            egui::Sense::hover(),
        );
        let rounding = height / 2.0;
        let painter = ui.painter();
        painter.rect_filled(rect, rounding, visuals.extreme_bg_color);
        let picked = rect.width() * gauge.picked_share();
        if picked > 0.0 {
            painter.rect_filled(
                egui::Rect::from_min_size(rect.min, egui::vec2(picked, height)),
                rounding,
                选中色,
            );
        }
        let rest_left = rect.left() + picked;
        match gauge.strangers {
            Some(_) => {
                let width = rect.width() * gauge.stranger_share();
                if width > 0.0 {
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(rest_left, rect.top()),
                            egui::vec2(width, height),
                        ),
                        rounding,
                        之外色,
                    );
                }
            }
            // **「还不知道」画成一段斜纹，不画成零**（设计稿 `.gauge .unk`）：卡不在手边时目标上
            // 有什么本来就没看过，一段也不画等于说「卡上是空的」。
            None => {
                let width = (rect.right() - rest_left).min(rect.width() * UNKNOWN_SHARE);
                hatch(
                    painter,
                    egui::Rect::from_min_size(
                        egui::pos2(rest_left, rect.top()),
                        egui::vec2(width, height),
                    ),
                    未知描边,
                );
            }
        }
        ui.horizontal_wrapped(|ui| {
            legend_swatch(ui, 选中色);
            // 图例带「（N 个变体）」（设计稿 `.legend`）。几个由核心数：算过容量的照报告，排过差量的照那份计划。
            let 几个 = self
                .evaluated
                .get(name)
                .map(|report| report.picked)
                .or_else(|| {
                    self.prepared
                        .as_ref()
                        .filter(|prepared| prepared.sublibrary.name == name)
                        .map(|prepared| prepared.selected.picked.len() as u64)
                });
            let 选中 = match (counted, 几个) {
                (true, Some(几个)) => format!(
                    "已选 {}（{} 个变体）",
                    human_bytes(gauge.picked),
                    thousands(几个)
                ),
                (true, None) => format!("已选 {}", human_bytes(gauge.picked)),
                (false, _) => "已选 还没算".to_string(),
            };
            // 图例的字与字号都照稿（设计稿 `.legend` 的 12px：「已选 / 清单外文件 / 容量上限」，拿主意的人
            // 照稿定，挂单 `Q891`）。
            ui.label(egui::RichText::new(选中).small()).on_hover_text(
                "这个子库在卡上占的地方。看过目标之后（算过容量或排过差量预览）算的是同步完的样子\
                 ——元数据与媒体也要占地方，转换又省下来一些，而卡上还留着那些\
                 「对不上、本次不动」的文件。卡不在手边时就是选择集选出来那批变体一共多大。",
            );
            ui.add_space(step(3));
            match gauge.strangers {
                None => {
                    legend_swatch(ui, 未知描边.color);
                    ui.label(egui::RichText::new("清单外文件：未知").small());
                }
                Some(bytes) => {
                    legend_swatch(ui, 之外色);
                    ui.label(
                        egui::RichText::new(format!("清单外文件 {}", human_bytes(bytes))).small(),
                    )
                    .on_hover_text(
                        "工具没放过的文件：维护者自己拷进去的存档、金手指、截图。连看都不看。",
                    );
                }
            }
            ui.add_space(step(3));
            match gauge.capacity {
                None => ui.label(egui::RichText::new("容量上限 不设限").small().weak()),
                Some(bytes) => ui.label(
                    egui::RichText::new(format!("容量上限 {}", decimal_bytes(bytes)))
                        .small()
                        .weak(),
                ),
            };
        });
        // 卡不在位时图例底下一行普通小字（设计稿 `devCard` 的 `.help`；逐字照稿、样式也照稿，拿主意的人定）。
        if self.absent.contains(name) {
            look::help(ui, ABSENT_NOTE);
        }
        if !counted {
            note_ui(ui, "还没算过：按上面「算一遍容量」，或者排一次差量预览。");
        }
        // 超出多少、砍谁由核心一处算（同步计划器），这儿只把它摆出来。
        let excluded = room.as_ref().and_then(|room| trim_ui(ui, room));
        // 算过容量却算不出装不装得下：**不给数**，不拿选中容量去冒充「装得下」。
        //
        // **卡上说这一层自己的话，核心那句原话放进悬停**：原话带着系统错误的原文（各平台不一样）与
        // 命令行命令，屏上不该出现（票 `gui-looks-like-the-design/03`；挂单 `Q851`）。卡不在位那一种不在这儿说：
        // 卡底按钮旁边照稿写「请先连接设备」，路径、怎么办与这句原话都在它的悬停里（[`Self::actions_ui`]）。
        if room.is_none()
            && !self.absent.contains(name)
            && let Some(Fit::Unknown { why }) = self.evaluated.get(name).map(|report| &report.fit)
        {
            look::help(ui, "装不装得下算不出（指针停在这儿看为什么）").on_hover_text(why.as_str());
        }
        excluded
    }

    /// 卡的下半截：**排差量、看步骤、按同步**。
    fn delta_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        // 头一排与没摊开的卡底下是同一排（[`Self::actions_ui`]）；摊开这一张的「同步」在差量底下。
        let name = self.picked.clone().unwrap_or_default();
        self.actions_ui(ui, site, tasks, &name, true);
        let Some(prepared) = self.prepared.clone() else {
            // 差量作废了、而同步还在台上跑着的话，这一句底下还得摆得出那一趟的进度
            // ——不然人一按下同步，屏上就什么都没有了。
            Self::live_ui(ui, tasks, self.syncing, "同步");
            note_ui(ui, PREVIEW_NOTE);
            return;
        };
        self.plan_ui(ui, site, tasks, &prepared);
    }

    /// 那份计划本身：账、要说出口的怪事、步骤，以及「真的传」。
    fn plan_ui(&mut self, ui: &mut egui::Ui, site: &Site, tasks: &mut Tasks, prepared: &Prepared) {
        let plan = &prepared.plan;
        ui.separator();
        tally_ui(ui, plan, self.prepare_ms);
        concerns_ui(ui, prepared);
        self.steps_ui(ui, plan);
        self.sync_ui(ui, site, tasks, plan);
    }

    /// 步骤那一段：收起来时摆头几条，摊开是那张虚拟化的表。
    fn steps_ui(&mut self, ui: &mut egui::Ui, plan: &romcat_core::sync::Plan) {
        ui.add_space(4.0);
        let header = ui
            .horizontal(|ui| {
                ui.label(font::strong(format!(
                    "这一趟要动的 {} 步（先删后传）",
                    thousands(plan.touched())
                )));
                if plan.steps.len() > STEP_SAMPLE {
                    let label = if self.expanded {
                        "收起来"
                    } else {
                        "全部展开"
                    };
                    if look::buttons(ui, |ui| ui.button(label)).clicked() {
                        self.expand(!self.expanded);
                    }
                }
            })
            .response;
        if self.expanded {
            // **表里不按滚动位置**：人自己滚。`scroll_to` 是给实测与测试的
            // （[`steps_table`]）。
            self.steps_drawn = steps_table(ui, plan, None);
            // 刚摊开：把卡片那一列滚到这张表的表头（`reveal_steps` 的文档）。**要在表画完之后问**：
            // 表自己也是一块滚动区，在它之前问的话，那一下被它先收走，滚的是表里头而不是卡片那一列。
            // **不带动画**：一下到位。带动画的话要跑上十几帧才滚到，而这一下是替人把刚摊开的表
            // 摆到眼前，不是一段要看的过渡。
            if self.reveal_steps {
                ui.scroll_to_rect_animation(
                    header.rect,
                    Some(Align::Min),
                    egui::style::ScrollAnimation::none(),
                );
                self.reveal_steps = false;
            }
            return;
        }
        self.steps_drawn = 0;
        for step in plan.steps.iter().take(STEP_SAMPLE) {
            ui.horizontal(|ui| {
                if step.act == Act::Delete {
                    ui.colored_label(ui.visuals().error_fg_color, step.act.label());
                } else {
                    ui.label(step.act.label());
                }
                ui.weak(step.kind.label());
                ui.label(&step.path);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.weak(human_bytes(if step.act == Act::Delete {
                        step.was
                    } else {
                        step.bytes
                    }));
                });
            });
        }
        if plan.steps.len() > STEP_SAMPLE {
            ui.weak(format!(
                "……另有 {} 步没列，按「全部展开」看全",
                plan.steps.len() - STEP_SAMPLE
            ));
        }
    }

    /// 「真的传」那一行，连**有删除就得先点头**那一格（ADR-0015）。
    fn sync_ui(
        &mut self,
        ui: &mut egui::Ui,
        site: &Site,
        tasks: &mut Tasks,
        plan: &romcat_core::sync::Plan,
    ) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if plan.deletes.files > 0 {
                ui.checkbox(
                    &mut self.acknowledged,
                    format!(
                        "我看过删除清单（{} 个，{}）",
                        thousands(plan.deletes.files),
                        human_bytes(plan.deletes.bytes),
                    ),
                );
            }
            let ready = self.syncing.is_none()
                && plan.touched() > 0
                && (plan.deletes.files == 0 || self.acknowledged);
            if look::buttons(ui, |ui| ui.add_enabled(ready, egui::Button::new("同步")))
                .on_hover_text(
                    "把上面这份差量真的落到目标设备上。只碰清单里记录过的文件。\
                     它排到任务台上跑：进度、已用时间、停下都在任务屏上，\
                     跑的是这一刻摆着的这一份计划——排上去之后改规则也改不了它。",
                )
                .clicked()
            {
                self.sync(site, tasks);
            }
            Self::live_ui(ui, tasks, self.syncing, "同步");
            if plan.touched() == 0 {
                ui.label("一步都不用做：目标已经和选择集对齐了。");
            }
        });
    }
}

/// 差量的账：新增 / 更新 / 删除 / 原样留着各几个文件、几个变体、多大，加上净变化。
fn tally_ui(ui: &mut egui::Ui, plan: &romcat_core::sync::Plan, prepare_ms: f64) {
    egui::Grid::new(format!("差量账 · {}", plan.sublibrary))
        .num_columns(4)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            ui.label(font::strong(""));
            ui.label(font::strong("文件"));
            ui.label(font::strong("变体"));
            ui.label(font::strong("容量"));
            ui.end_row();
            for (what, tally) in [
                ("新增", plan.adds),
                ("更新", plan.updates),
                ("删除", plan.deletes),
                ("原样留着", plan.keeps),
            ] {
                ui.label(what);
                // 数量与容量用等宽：四行账竖着比大小。
                ui.label(font::mono(thousands(tally.files)));
                ui.label(font::mono(thousands(tally.variants)));
                ui.label(font::mono(human_bytes(tally.bytes)));
                ui.end_row();
            }
        });
    let net = if plan.net_bytes >= 0 {
        format!("＋{}", human_bytes(plan.net_bytes.unsigned_abs()))
    } else {
        format!("－{}", human_bytes(plan.net_bytes.unsigned_abs()))
    };
    ui.label(format!(
        "净变化 {net}；同步完之后目标上占 {}（眼下 {}）｜排它用了 {prepare_ms:.0} ms",
        human_bytes(plan.after_bytes),
        human_bytes(plan.actual_bytes),
    ));
}

/// 折期望状态与排计划时那几件**要说出口**的怪事：核心报的那几条、**放不进目标的**、
/// **目标吃不下而这一版转不了的**、以及目标上对不上的那些。
///
/// 头一段整份来自核心（[`Prepared::concerns`]），**命令行与界面印同一份**：
/// 各写一遍的话，界面上会少掉其中一两条——而这几条正是「为什么这一趟少选出来这么多」
/// 的答案。
///
/// 后面几段以前这一屏一条都不画（只有命令行的 `Plan::render_text` 印）。
/// **放不进目标的**里头就有**落点撞车**（挂单 Q57 点名要这一票把它报出来）：
/// 两个根里同一条相对路径落在卡上同一个文件上——子库里的落点剥掉了根名（ADR-0013），
/// 于是它们撞在一起，而**撞上的一个都不放行**。人不知道这件事的话，
/// 会对着「明明选中了却没传过去」发呆。
fn concerns_ui(ui: &mut egui::Ui, prepared: &Prepared) {
    let plan = &prepared.plan;
    for concern in prepared.concerns() {
        ui.colored_label(ui.visuals().warn_fg_color, concern);
    }
    if !plan.rejected.is_empty() {
        ui.colored_label(
            ui.visuals().error_fg_color,
            format!(
                "{} 份放不进目标，这一趟既不新增也不删除——它们进不了卡，\
                 而「放不进去」这个判断本身也可能是错的，删掉别人的东西不可逆：",
                thousands(plan.rejected.len() as u64),
            ),
        );
        for row in plan.rejected.iter().take(TOP_NOTES) {
            ui.label(format!(
                "{}｜{}｜{}{}",
                row.reason.label(),
                row.path,
                human_bytes(row.bytes),
                if row.estimated { "（估的）" } else { "" },
            ))
            .on_hover_text(&row.detail);
        }
        if plan.rejected.len() > TOP_NOTES {
            ui.weak(format!("……另有 {} 份没列", plan.rejected.len() - TOP_NOTES));
        }
    }
    if !plan.unsupported.is_empty() {
        // **照搬，但点名说出口**（ADR-0017：矩阵错了比不转换更糟）。不说的话，
        // 人会以为工具已经替他处理妥当，直到在掌机上打不开才发现。
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!(
                "{} 份目标吃不下、而这一版转不了：照样传过去，但它在这台设备上多半打不开。",
                thousands(plan.unsupported.len() as u64),
            ),
        );
        for row in plan.unsupported.iter().take(TOP_NOTES) {
            ui.label(format!(
                "{}｜{}｜要的是 {}",
                row.path,
                human_bytes(row.bytes),
                row.want
            ))
            .on_hover_text(&row.why);
        }
        if plan.unsupported.len() > TOP_NOTES {
            ui.weak(format!(
                "……另有 {} 份没列",
                plan.unsupported.len() - TOP_NOTES
            ));
        }
    }
    if plan.surprises.is_empty() {
        return;
    }
    ui.colored_label(
        ui.visuals().warn_fg_color,
        format!(
            "{} 件对不上的事，本次一律不动它们：",
            thousands(plan.surprises.len() as u64)
        ),
    );
    for surprise in plan.surprises.iter().take(TOP_NOTES) {
        ui.label(format!(
            "{}｜{}｜{}",
            surprise.kind.label(),
            surprise.path,
            if surprise.still_wanted {
                "选择集还要它"
            } else {
                "选择集已经不要它了"
            },
        ));
    }
    if plan.surprises.len() > TOP_NOTES {
        ui.weak(format!(
            "……另有 {} 件没列",
            plan.surprises.len() - TOP_NOTES
        ));
    }
}

impl Screen {
    /// 「**目标设置**」那层弹层（[`crate::dialog`]）：名字、目标路径、前端格式、容量上限、能力档案。
    ///
    /// 屏头「新建子库」开一份空草稿，卡上「目标设置…」开那一台的（[`Self::begin_new`] /
    /// [`Self::edit_target`]）。字段与存法照原先底下那块「配目标」面板原样搬来（票
    /// `gui-looks-like-the-design/20` 第二段）；逐平台列能力档案、路径三种校验、落点预览归票 `21`，
    /// 在这层弹层上接着做。标题、说明、按钮上的字与宽照稿（设计稿 `DLG.subform` 的 `w:720`：令牌第三档）。
    ///
    /// **中文输入在这层的内容区里**：内容区每帧把整份内容都摆一遍，正在组字的那一格不会凭空消失
    /// （[`crate::dialog`] 模块文档、ADR-0005）。
    fn target_dialog_ui(&mut self, ctx: &egui::Context, site: &mut Site) {
        /// 页脚上按下去的是哪一颗。
        enum Pressed {
            /// 「取消」。
            Cancel,
            /// 「创建子库」或「保存」。
            Save,
        }
        let Some(which) = self.target_dialog.clone() else {
            return;
        };
        // **判在画之前**：页脚那颗按不按得动看的是这一帧判出来的（只在字变了时真去判，[`Self::vet_form`]）。
        self.vet_form(site);
        let ready = self.form_ready();
        let target_verdict = self.vetted.as_ref().map(|vetted| vetted.verdict.clone());
        let name_verdict = self
            .name_vetted
            .as_ref()
            .map(|vetted| vetted.verdict.clone());
        let mut pick_pressed = false;
        let (title, note, save_label) = match &which {
            TargetDialog::New => (
                "新建子库".to_string(),
                "子库对应一台设备（通常是掌机）。创建后在「浏览」中按平台筛选，把结果分几次加入它的选择集。",
                "创建子库",
            ),
            TargetDialog::Of(name) => (
                format!("目标设置 · {name}"),
                "修改目标路径、前端格式或能力档案后，已有的差量预览会失效，同步前需要重新生成。",
                "保存",
            ),
        };
        let footer = Footer::new(Button::new("取消", Pressed::Cancel)).button(
            Button::new(save_label, Pressed::Save)
                .primary()
                .enabled(ready)
                .hover("新建或改写这台设备。目标设备不在位也存得下——子库是持久实体。"),
        );
        // **能力档案**：名册随 `reload` 读好了；挑的那一份叠上覆盖之后放不下哪几份，只在档案或覆盖变了时重算。
        let today = self
            .today
            .clone()
            .unwrap_or_else(romcat_core::capability::today);
        let profile = self.roster.as_ref().map(|roster| {
            roster.find_or_unclaimed(
                Some(self.form.capability.trim()).filter(|name| !name.is_empty()),
            )
        });
        let roster_names: Vec<String> = self.roster.as_ref().map_or_else(Vec::new, |roster| {
            roster
                .names()
                .into_iter()
                .map(ToString::to_string)
                .collect()
        });
        self.refresh_too_big(profile.as_ref());
        let too_big = self
            .too_big
            .as_ref()
            .is_some_and(|(_, _, rows)| !rows.is_empty());
        let editing_one = matches!(which, TargetDialog::Of(_));
        // 「设备上的位置」（拿主意的人 2026-09-15 定：照实际规则）：改一台时是这一台头一个变体的真实落点（`Footprint::landing`），
        // 新建时拿示例名走同一条规则（`Landing::example`）；元数据位置由选的那个前端格式答。还在读选择集时先不画。
        let format_name = if self.form.format.trim().is_empty() {
            PEGASUS.to_string()
        } else {
            self.form.format.trim().to_string()
        };
        let landing = romcat_core::adapter::find(&format_name).and_then(|adapter| match &which {
            TargetDialog::Of(name) => {
                let overridden = profile.as_ref().map_or_else(Profile::unclaimed, |profile| {
                    profile.with_overrides(&self.form.overrides)
                });
                self.footprint
                    .as_ref()
                    .filter(|(of, _)| of == name)
                    .and_then(|(_, footprint)| footprint.landing(&overridden, adapter.as_ref()))
            }
            TargetDialog::New => Some(romcat_core::sync::Landing::example(
                adapter.as_ref(),
                EXAMPLE_DIRECTORY,
                EXAMPLE_FILE,
            )),
        });
        // 前端格式那句说明拿哪个平台目录举例：改一台时是它头一个变体真实落在的目录，读选择集之前与新建时是库里头一个有平台的
        // 变体住的目录（`Catalog::sample_platform_directory`，弹层打开时读一次）。都说不出就不举例。
        if self.sample_directory.is_none() {
            self.sample_directory = Some(site.catalog.sample_platform_directory().ok().flatten());
        }
        let example_directory = match (&which, &landing) {
            (TargetDialog::Of(_), Some(landing)) => Some(landing.directory.clone()),
            _ => self.sample_directory.clone().flatten(),
        };
        let landing_root = if self.form.target.trim().is_empty() {
            EXAMPLE_ROOT.to_string()
        } else {
            self.form.target.trim().to_string()
        };
        // 路径底下那一行（设计稿 `probePath`）：在位照稿写连接状态，清单外文件数数完了才接上那半句；不在位说未连接。
        let presence_line = match self
            .vetted
            .as_ref()
            .map(|vetted| (&vetted.text, &vetted.verdict))
        {
            Some((text, Ok(Ok(Presence::Present(volume))))) => {
                let counted = self
                    .strangers
                    .as_ref()
                    .filter(|(of, _)| of == text)
                    .map(|(_, counted)| counted.count);
                Some((connected_line(volume, counted), true))
            }
            Some((_, Ok(Ok(Presence::Absent)))) => Some((
                "未连接。设备不在时也能建，插上之后再生成差量预览。".to_string(),
                false,
            )),
            _ => None,
        };
        // 「按设备容量」那一格写哪个数：这条规矩在核心库（`sublibrary::device_limit`）——卡在位是此刻的总量，
        // 不在位是上次读到的，都没有就说不设上限。
        let device_label =
            match romcat_core::sublibrary::device_limit(self.stored_total(), self.live_total()) {
                Some(bytes) => format!("按设备容量（{}）", decimal_bytes(bytes)),
                None => "按设备容量（没读过，不设上限）".to_string(),
            };
        let platforms = self.target_platforms();
        let error = self.error.clone();
        let form = &mut self.form;
        let shown = Dialog::new("目标设置", title, footer)
            .note(note)
            .width(Width::Wide)
            .show(ctx, |ui| {
                if let Some(error) = &error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                let layout = &Tokens::builtin().layout;
                // 名那一列靠左（设计稿弹层表单）：`add_sized` 会把字摆在格子正中。
                let field_label = |ui: &mut egui::Ui, label: &str| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(layout.form_label_width, layout.input_height),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            ui.set_min_size(egui::vec2(layout.form_label_width, layout.input_height));
                            ui.label(label);
                        },
                    );
                };
                // 值那一列底下那一行（设计稿 `.help` / `.err`）：与输入框左边对齐，**照可用宽折行**——横排里的字默认不折，
                // 长一点的说明会把整层弹层撑得比令牌那一档还宽。
                let under = |ui: &mut egui::Ui, text: &str, error: bool| {
                    ui.horizontal(|ui| {
                        ui.add_space(layout.form_label_width + ui.spacing().item_spacing.x);
                        let color = if error {
                            ui.visuals().error_fg_color
                        } else {
                            ui.visuals().weak_text_color()
                        };
                        ui.add(
                            egui::Label::new(egui::RichText::new(text).small().color(color)).wrap(),
                        );
                    });
                };

                ui.horizontal(|ui| {
                    field_label(ui, "名称");
                    let width = ui.available_width();
                    look::text_input(
                        ui,
                        width,
                        egui::TextEdit::singleline(&mut form.name)
                            .hint_text("例如设备型号：RG35XX Plus"),
                    );
                });
                match name_verdict.as_ref() {
                    Some(Ok(Err(NameRefusal::Empty))) => under(ui, "用来区分不同设备。", false),
                    Some(Ok(Err(NameRefusal::Taken))) => under(ui, "已经有同名的子库。", true),
                    Some(Err(why)) => under(ui, why, true),
                    Some(Ok(Ok(()))) | None => {}
                }

                // 字段之间照稿隔一档（设计稿 `.frm` 的 `gap:12px`）。
                ui.add_space(step(2));
                ui.horizontal(|ui| {
                    field_label(ui, "目标路径");
                    let pick_width = look::button_width(ui, "选择…");
                    let width = ui.available_width() - pick_width - ui.spacing().item_spacing.x;
                    look::text_input(
                        ui,
                        width,
                        egui::TextEdit::singleline(&mut form.target)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("选择或粘贴路径"),
                    );
                    if ui
                        .button("选择…")
                        .on_hover_text(crate::pick::FALLBACK_HINT)
                        .clicked()
                    {
                        pick_pressed = true;
                    }
                });
                match target_verdict.as_ref() {
                    // 框还空着不是错：照稿一句弱字说目标路径通常是什么（设计稿 `probePath` 空串那一支）。
                    Some(Ok(Err(TargetRefusal::Empty))) => under(
                        ui,
                        "通常是 SD 卡或掌机存储的根目录。无法弹出选择窗口时，也可以直接粘贴路径。",
                        false,
                    ),
                    Some(Ok(Err(refusal))) => under(ui, &refusal_line(refusal), true),
                    Some(Err(why)) => under(ui, why, true),
                    Some(Ok(Ok(_))) | None => {}
                }
                if let Some((line, connected)) = &presence_line {
                    if *connected {
                        ui.horizontal(|ui| {
                            ui.add_space(layout.form_label_width + ui.spacing().item_spacing.x);
                            let (good, _) = look::tone_colors(look::Tone::Good, ui.visuals());
                            ui.add(
                                egui::Label::new(egui::RichText::new(line).small().color(good))
                                    .wrap(),
                            );
                        });
                    } else {
                        under(ui, line, false);
                    }
                }

                // 字段之间照稿隔一档（设计稿 `.frm` 的 `gap:12px`）。
                ui.add_space(step(2));
                ui.horizontal(|ui| {
                    field_label(ui, "前端格式");
                    // 这一版带了哪几个适配器由核心库答（`adapter::names`）：界面不另写一份清单，
                    // 添一个适配器这一排就多一格（ADR-0024）。
                    let adapters = romcat_core::adapter::names();
                    let chosen = if form.format.trim().is_empty() {
                        PEGASUS
                    } else {
                        form.format.trim()
                    };
                    let 这一排: Vec<(&str, &str)> = adapters
                        .iter()
                        .map(|name| (*name, format_label(name)))
                        .collect();
                    let 选中 = adapters
                        .iter()
                        .find(|name| name.eq_ignore_ascii_case(chosen))
                        .copied()
                        .unwrap_or(adapters[0]);
                    if let Some(picked) = look::segmented(ui, &这一排, 选中) {
                        form.format = picked.to_string();
                    }
                });
                under(ui, &format_help(&form.format, example_directory.as_deref()), false);

                // 字段之间照稿隔一档（设计稿 `.frm` 的 `gap:12px`）。
                ui.add_space(step(2));
                ui.horizontal(|ui| {
                    field_label(ui, "能力档案");
                    let chosen = if form.capability.trim().is_empty() {
                        DEFAULT_PROFILE.to_string()
                    } else {
                        form.capability.trim().to_string()
                    };
                    let width = ui.available_width();
                    egui::ComboBox::from_id_salt("能力档案")
                        .width(width)
                        .selected_text(chosen.clone())
                        .show_ui(ui, |ui| {
                            for name in &roster_names {
                                if ui.selectable_label(*name == chosen, name).clicked() {
                                    form.capability.clone_from(name);
                                }
                            }
                        });
                });
                if let Some(profile) = &profile {
                    under(ui, &profile_help(profile), false);
                    profile_table_ui(
                        ui,
                        profile,
                        editing_one.then_some(platforms.as_deref()),
                        &mut form.overrides,
                        &today,
                    );
                    if too_big && let Some(limit) = profile.filesystem.max_file_bytes {
                        ui.horizontal(|ui| {
                            ui.add_space(layout.form_label_width + ui.spacing().item_spacing.x);
                            ui.vertical(|ui| {
                                look::warn_box(
                                    ui,
                                    &format!("{} 单文件上限 {}。", profile.filesystem.name, human_bytes(limit)),
                                    "选择集里有超过这个大小的变体，它们会列在差量预览的「放不进目标」里。",
                                );
                            });
                        });
                    }
                }

                // 容量上限照稿二选一（共用的单选件 `look::radio_option`，设计稿 `.opt`）；自定义时底下一格填数（十进制 GB）。
                ui.add_space(step(2));
                ui.horizontal_top(|ui| {
                    field_label(ui, "容量上限");
                    ui.vertical(|ui| {
                        if look::radio_option(ui, form.capacity_by_device, &device_label, "设备连接时自动读取").clicked() {
                            form.capacity_by_device = true;
                        }
                        if look::radio_option(ui, !form.capacity_by_device, "自定义", "给存档、截图等留出空间").clicked() {
                            form.capacity_by_device = false;
                        }
                        if !form.capacity_by_device {
                            ui.horizontal(|ui| {
                                look::text_input(
                                    ui,
                                    layout.capacity_input_width,
                                    egui::TextEdit::singleline(&mut form.capacity).hint_text("例如 58"),
                                );
                                ui.label("GB");
                            });
                        }
                        look::help(ui, "超出上限时只给出删减建议，不会自动删除。");
                    });
                });
                if let Some(landing) = &landing {
                    ui.add_space(step(2));
                    ui.horizontal_top(|ui| {
                        field_label(ui, "设备上的位置");
                        ui.vertical(|ui| {
                            landing_box_ui(ui, &landing_root, landing);
                            look::help(
                                ui,
                                "按平台分目录，不带根名：两个根里相同的相对路径会在差量预览中报为落点撞车。",
                            );
                        });
                    });
                }
            });
        if pick_pressed {
            // 起点：框里那一串是个目录就从那儿打开，否则交给系统（`pick::directory` 的文档）。
            let start = PathBuf::from(self.form.target.trim());
            self.picked_target(crate::pick::directory("选择目标目录", &start));
        }
        match shown.pressed {
            None => {}
            Some(Pressed::Cancel) => self.leave_target_settings(),
            Some(Pressed::Save) => {
                // 存下来才关；没存下来时那句话画在弹层里（[`Self::save`]）。先存再判，不把有副作用的一下写进分支守卫。
                let saved = self.save(site);
                if saved {
                    self.target_dialog = None;
                }
            }
        }
    }

    /// 存下这个子库。**界面上按那个按钮走的就是它**，实测与测试拿它当那一下。
    ///
    /// **目标落在主库里当场拦下**（ADR-0004、验收第 8 条）：判据在核心里
    /// （`sync::prepare::refuse_target_in_library`），与同步那一道是同一条。
    /// 拦在存下来这一步而不是等到同步，是因为一个指着主库的子库定义放在库里，
    /// 下一次点同步之前谁都不知道它错了。
    ///
    /// 交回存没存下来：「目标设置」那层弹层按它决定关不关——没存下来时弹层开着，那句话画在弹层里。
    pub fn save(&mut self, site: &mut Site) -> bool {
        let written = self.form.capacity.trim();
        let capacity = match &self.form.kept_capacity {
            // 按设备容量那一档：数在下面判完目标路径之后定（此刻的总量，或者上次读到的）。
            _ if self.form.capacity_by_device => None,
            // 字没改过：沿用原来的字节数，不重新解析（[`Form::kept_capacity`]）。
            Some((shown, bytes)) if written == shown.trim() => Some(*bytes),
            _ if written.is_empty() => None,
            // 光一个数：照稿按十进制 GB 读（「58」就是 58 GB，与屏上一位小数的写法同一个单位）。
            _ => match written.parse::<f64>() {
                Ok(gigabytes) if gigabytes.is_finite() && gigabytes >= 0.0 => {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let bytes = (gigabytes * 1_000_000_000.0).round() as u64;
                    Some(bytes)
                }
                _ => match rule::parse_size(written) {
                    Some(bytes) => Some(bytes),
                    None => {
                        self.error = Some(format!(
                            "看不懂容量「{written}」。写一个数（按 GB 算，如 58），或者带上单位（如 476GiB）。",
                        ));
                        return false;
                    }
                },
            },
        };
        let name = self.form.name.trim().to_string();
        let target = std::path::PathBuf::from(self.form.target.trim());
        // **与弹层当场判的是同一道**（[`target::vet`] / [`target::vet_name`]）：页脚那颗按不动时这里本来到不了，
        // 程序里直接调它的也一样拦下。
        self.vetted = None;
        self.name_vetted = None;
        self.vet_form(site);
        if !self.form_ready() {
            let target_line = match self.vetted.as_ref().map(|vetted| &vetted.verdict) {
                Some(Ok(Err(refusal))) => Some(refusal_line(refusal)),
                Some(Err(why)) => Some(why.clone()),
                _ => None,
            };
            let name_line = match self.name_vetted.as_ref().map(|vetted| &vetted.verdict) {
                Some(Ok(Err(NameRefusal::Empty))) => Some("名称没填。".to_string()),
                Some(Ok(Err(NameRefusal::Taken))) => Some("已经有同名的子库。".to_string()),
                Some(Err(why)) => Some(why.clone()),
                _ => None,
            };
            self.error = Some(
                [name_line, target_line]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            return false;
        }
        let format = if self.form.format.trim().is_empty() {
            "Pegasus".to_string()
        } else {
            self.form.format.trim().to_string()
        };
        // 两种路径形式怎么折，**由核心的 `Sublibrary::at` 一处说了算**（ADR-0020）。
        // **改名**（拿主意的人 2026-09-15 定，照稿名字可改）：只在「目标设置…」开着、名字改了的时候。核心一个事务里把规则、例外、
        // 覆盖、清单挪到新名下（`Catalog::rename_sublibrary`），再照新名存下这一次改的那几格。
        let dialog_open = self.target_dialog.is_some();
        let renaming = match &self.target_dialog {
            Some(TargetDialog::Of(old)) if *old != name => Some(old.clone()),
            _ => None,
        };
        let creating = matches!(self.target_dialog, Some(TargetDialog::New));
        let before = match &self.target_dialog {
            Some(TargetDialog::Of(old)) => old.clone(),
            _ => name.clone(),
        };
        let had_preview = self
            .prepared
            .as_ref()
            .is_some_and(|prepared| prepared.sublibrary.name == before);
        if let Some(old) = &renaming {
            match site.catalog.rename_sublibrary(old, &name) {
                Ok(Renamed::Done) => {
                    self.evaluated.remove(old);
                    if let Some((of, _)) = self.footprint.as_mut().filter(|(of, _)| of == old) {
                        of.clone_from(&name);
                    }
                }
                Ok(Renamed::Missing) => {
                    self.error = Some(format!("改不了名：已经没有叫「{old}」的子库了。"));
                    return false;
                }
                Ok(Renamed::Refused(_)) => {
                    self.error = Some("已经有同名的子库。".to_string());
                    return false;
                }
                Err(error) => {
                    self.error = Some(format!("中立库写不动：{error}"));
                    return false;
                }
            }
        }
        // 按设备容量那一档：卡此刻在位就把总量记下来（换卡跟着变），不在位照旧留着上次读到的——
        // 判在核心库那一处（`sublibrary::device_limit`），屏上那一行写的也是它。
        let capacity = if self.form.capacity_by_device {
            romcat_core::sublibrary::device_limit(self.stored_total(), self.live_total())
        } else {
            capacity
        };
        let mut sublibrary = Sublibrary::at(&name, &target, &format, capacity);
        sublibrary.capacity_by_device = self.form.capacity_by_device;
        sublibrary.capability =
            Some(self.form.capability.trim().to_string()).filter(|value| !value.is_empty());
        match site.catalog.put_sublibrary(&sublibrary) {
            Ok(()) => {
                // **按平台覆盖整份存下**：弹层里改回「按档案」的那几行就是没了（`Catalog::set_capability_overrides`）。
                if let Err(error) = site
                    .catalog
                    .set_capability_overrides(&name, &self.form.overrides)
                {
                    self.error = Some(format!("中立库写不动：{error}"));
                    return false;
                }
                // **算过的那份跟着作废**：容量上限改了，报告里的「超出多少、砍谁」
                // 说的还是上一个上限——卡上会出现「上限写着 1 TB、旁边说超了 200 GiB」。
                // **台上那趟还没认领的也一样**（[`Self::drop_survey`]）：它折报告用的正是
                // 改之前那份子库，收回来等于把刚改掉的上限又摆回屏上。
                self.evaluated.remove(&name);
                let mut line = format!("存下了子库「{name}」。");
                if let Some(说一句) = self.drop_survey() {
                    line.push(' ');
                    line.push_str(说一句);
                }
                self.notice = Some(line);
                self.reload(site);
                self.open(site, &name);
                // **底边提示条**（F9，照稿）：只在弹层里存下时摆。改过目标设置之后原来那份差量预览已经作废（`open` 走了
                // `invalidate`），要说出来——「同步」认的正是那一份。
                if dialog_open {
                    let text = if creating {
                        format!("已创建子库「{name}」")
                    } else if had_preview {
                        format!("已保存「{name}」的目标设置。差量预览已失效，同步前需要重新生成。")
                    } else {
                        format!("已保存「{name}」的目标设置。")
                    };
                    self.undo = None;
                    self.saved = Some(Toast::new(text));
                }
                true
            }
            Err(error) => {
                self.error = Some(format!("中立库写不动：{error}"));
                false
            }
        }
    }

    /// 删掉摊开那一台的定义。**目标设备上的文件一个都不碰。** 「删除子库」那层弹层上按「删除子库」走的就是它，
    /// 实测与测试拿它当那一下。
    ///
    /// 删之前核心把这一台整份留下来交回
    /// （[`Catalog::take_sublibrary`](romcat_core::catalog::Catalog::take_sublibrary)），这一屏拿着它摆一条带
    /// 「撤销」的提示条（[`Self::undo_remove`]）。再删一台的话上一台那一份就丢了——提示条只摆一条。
    pub fn remove(&mut self, site: &mut Site) {
        let Some(name) = self.picked.clone() else {
            return;
        };
        self.delete_dialog = None;
        match site.catalog.take_sublibrary(&name) {
            Ok(Some(removed)) => {
                self.picked = None;
                self.evaluated.remove(&name);
                // 台上那趟还没认领的也不认了——认领是整份替换，刚删掉这一台的报告会
                // 又长回来（[`Self::drop_survey`]）。删掉了什么由提示条说，这儿只留那一句。
                self.notice = self.drop_survey().map(ToString::to_string);
                self.undo = Some(Undo {
                    toast: Toast::new(format!("已删除子库「{name}」，设备上的文件没有改动"))
                        .action("撤销"),
                    removed,
                });
                self.invalidate();
                self.reload(site);
            }
            Ok(None) => self.notice = Some("那个子库已经不在了。".to_string()),
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 提示条上按「**撤销**」：把刚删掉的那一台**原样放回去**——规则、例外、清单与删之前一样
    /// （[`Catalog::restore_sublibrary`](romcat_core::catalog::Catalog::restore_sublibrary)）。提示条跟着收起。
    ///
    /// 界面上按那颗按钮走的就是它。提示条已经收起、或者换过屏之后（[`Self::leave`]）什么都不做。
    pub fn undo_remove(&mut self, site: &mut Site) {
        let Some(undo) = self.undo.take() else {
            return;
        };
        let name = undo.removed.name().to_string();
        match site.catalog.restore_sublibrary(&undo.removed) {
            Ok(true) => {
                self.notice = None;
                self.reload(site);
            }
            // 删完之后又建了一个同名的：核心一行都没写，两份不揉在一起。
            Ok(false) => {
                self.error = Some(format!(
                    "放不回去：这会儿已经又有一个叫「{name}」的子库了。\
                     两份揉在一起谁的清单都说不清，所以一行都没写。"
                ));
            }
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 「**删除子库**」那层确认弹层（设计稿 `DLG.delsub`）：说清删的是这一份子库的定义——规则、例外、清单记录——
    /// 设备上的文件一个字节都不动；删之前把名字（标题上）与目标路径写明。按「删除子库」才真删（[`Self::remove`]）。
    fn delete_dialog_ui(&mut self, ctx: &egui::Context, site: &mut Site) {
        /// 页脚上按下去的是哪一颗。
        enum Pressed {
            /// 「取消」。
            Cancel,
            /// 「删除子库」。
            Remove,
        }
        let Some(name) = self.delete_dialog.clone() else {
            return;
        };
        let Some(target) = self
            .list
            .iter()
            .find(|row| row.name == name)
            .map(|row| row.target.clone())
        else {
            // 弹层开着的时候那一台已经没了（别处删掉、重新列一遍）：没东西可问。
            self.delete_dialog = None;
            return;
        };
        let chosen = self.selections.get(&name);
        let rules = chosen.map_or(0, |chosen| chosen.rules.len() + chosen.broken.len());
        let exceptions = chosen.map_or(0, |chosen| chosen.exceptions.len());
        let picked = self
            .evaluated
            .get(&name)
            .map(|report| (report.picked, report.bytes));
        let footer = Footer::new(Button::new("取消", Pressed::Cancel)).button(
            Button::new("删除子库", Pressed::Remove).danger().hover(
                "只删中立库里的这一份定义；设备上的文件一个字节都不动。\
                 删完之后底边提示条上按「撤销」原样放回来。",
            ),
        );
        let tokens = Tokens::builtin();
        let shown = Dialog::new("删除子库", format!("删除子库「{name}」"), footer)
            .width(Width::Narrow)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "只删除这个子库的定义：{} 条规则、{} 条例外和清单记录。",
                        thousands(rules as u64),
                        thousands(exceptions as u64),
                    ))
                    .size(look::font_size(ui.ctx(), tokens.font.size_small_plus)),
                );
                ui.horizontal_wrapped(|ui| {
                    look::help(ui, "目标路径");
                    ui.label(font::mono(&target).size(tokens.font.size_small));
                });
                ui.add_space(step(2));
                look::impact(
                    ui,
                    &[
                        ("设备上已经同步的文件", false),
                        ("不会删除", true),
                        ("。删除后它们成为清单外的文件，工具不再管理。", false),
                    ],
                );
                look::impact(
                    ui,
                    &[(
                        "如果想同时清空设备：先移除全部规则并同步一次，再删除子库。",
                        false,
                    )],
                );
                look::impact(ui, &[("主库和元数据不受影响。", false)]);
                if let Some((picked, bytes)) = picked {
                    ui.add_space(step(2));
                    look::help(
                        ui,
                        &format!(
                            "当前选择集：{} 个变体 · {}",
                            thousands(picked),
                            human_bytes(bytes)
                        ),
                    );
                }
            });
        match shown.pressed {
            None => {}
            Some(Pressed::Cancel) => self.delete_dialog = None,
            Some(Pressed::Remove) => {
                if self.picked.as_deref() != Some(name.as_str()) {
                    self.open(site, &name);
                }
                self.remove(site);
            }
        }
    }

    /// 规则行上「**×**」：打开那层确认弹层，移除的是这一台的第 `ordinal` 条规则。界面上按那颗按钮走的就是它。
    pub fn ask_remove_rule(&mut self, name: &str, ordinal: i64) {
        self.rule_dialog = Some((name.to_string(), ordinal));
    }

    /// 「移除规则」那层确认弹层开着没有。
    #[must_use]
    pub fn rule_dialog_open(&self) -> bool {
        self.rule_dialog.is_some()
    }

    /// 「移除规则」那层弹层上按「移除这条规则」：从中立库里删掉这一台的第 `ordinal` 条规则（`Catalog::remove_rule`，
    /// 删掉的号不再发），这一台缓着的容量账与差量预览当场作废、规则列表重读（[`Self::forget`]）。
    /// **设备上的文件与主库一个字节都不动。** 界面上按那颗按钮走的就是它，实测与测试拿它当那一下。
    pub fn remove_rule(&mut self, site: &mut Site, name: &str, ordinal: i64) {
        self.rule_dialog = None;
        match site.catalog.remove_rule(name, ordinal) {
            Ok(true) => {
                self.forget(site, name);
                self.notice = Some(format!(
                    "从子库「{name}」移除了第 {ordinal} 条规则。选中的变体跟着变了：\
                     容量要重算，差量预览要重新生成。"
                ));
            }
            Ok(false) => self.notice = Some("那一条规则已经不在了。".to_string()),
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 「**移除规则**」那层确认弹层：写清移除的是哪一台的哪一条（短名与原文）、移除之后选中的变体会变（算过容量的话
    /// 写出这一条自己命中多少），差量预览跟着作废；设备上的文件不动。按「移除这条规则」才真删（[`Self::remove_rule`]）。
    fn rule_dialog_ui(&mut self, ctx: &egui::Context, site: &mut Site) {
        /// 页脚上按下去的是哪一颗。
        enum Pressed {
            /// 「取消」。
            Cancel,
            /// 「移除这条规则」。
            Remove,
        }
        let Some((name, ordinal)) = self.rule_dialog.clone() else {
            return;
        };
        let chosen = self.selections.get(&name);
        let Some(text) = chosen
            .and_then(|chosen| chosen.rules.iter().find(|rule| rule.ordinal == ordinal))
            .map(|rule| rule.text.clone())
        else {
            // 弹层开着的时候那一条已经没了（别处改过、重新列一遍）：没东西可问。
            self.rule_dialog = None;
            return;
        };
        let label = chosen
            .and_then(|chosen| chosen.labels.get(&ordinal))
            .cloned()
            .unwrap_or_else(|| format!("规则 {ordinal}"));
        let 命中 = self.evaluated.get(&name).and_then(|report| {
            report
                .rules
                .iter()
                .find(|line| line.ordinal == ordinal)
                .map(|line| (line.hits, line.bytes))
        });
        let footer = Footer::new(Button::new("取消", Pressed::Cancel))
            .button(Button::new("移除这条规则", Pressed::Remove).danger().hover(
            "只从中立库里删掉这一条规则；设备上的文件一个字节都不动，下一趟同步照新的选择集去对。",
        ));
        let tokens = Tokens::builtin();
        let shown = Dialog::new("移除规则", format!("移除规则「{label}」"), footer)
            .width(Width::Narrow)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!("从子库「{name}」的选择集里移除第 {ordinal} 条规则："))
                        .size(look::font_size(ui.ctx(), tokens.font.size_small_plus)),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.label(font::strong(&label));
                    ui.label(font::mono(&text).size(tokens.font.size_small));
                });
                ui.add_space(step(2));
                let 会怎样 = match 命中 {
                    Some((hits, bytes)) => format!(
                        "移除之后选中的变体会变：这一条自己命中 {} 个（{}），其中别的规则也选中的那几个照旧留着。",
                        thousands(hits),
                        human_bytes(bytes)
                    ),
                    None => "移除之后选中的变体会变；少多少，按「算一遍容量」之后才看得见。".to_string(),
                };
                look::impact(ui, &[(会怎样.as_str(), false)]);
                look::impact(ui, &[("排过的差量预览跟着作废，同步前要重新生成。", false)]);
                look::impact(ui, &[("设备上的文件与主库都不受影响。", false)]);
            });
        match shown.pressed {
            None => {}
            Some(Pressed::Cancel) => self.rule_dialog = None,
            Some(Pressed::Remove) => self.remove_rule(site, &name, ordinal),
        }
    }

    /// 换一个画时刻用的钟（[`Clock`]）：截图测试钉死此刻与偏移，截图里才没有当前时间。
    pub fn set_clock(&mut self, clock: Clock) {
        self.exception_clock.clock = clock;
    }

    /// 截图测试用：手动例外那张表上每一条记下的时刻一律画成 `at`（UNIX 纪元起的秒）。
    ///
    /// 那一刻是中立库记这条例外时照挂钟写下的，截图里照实画的话一趟一个样——与待确认屏钉死落批时刻
    /// （`crate::queue::Screen::pin_record_time`）同一个用处、同一处实现（[`RecordClock`]）。真窗口那一路不调它。
    pub fn pin_exception_time(&mut self, at: i64) {
        self.exception_clock.pinned = Some(at);
    }

    /// 「手动例外」那一行上按「**管理**」：开出那层弹层，管的是 `name` 这一台（票 `gui-looks-like-the-design/22`）。
    ///
    /// 打开这一下就把**要读的两样**读齐：优先级表（挑显示标题，与浏览屏、导出、同步读的是同一份），
    /// 与这一台的例外连库里那个变体是谁。之后画帧不再碰库。
    ///
    /// ## 那两份配置读不出来就不开这一层
    ///
    /// **不退回内置那份**（与折标题、导出、刮削同一条规矩，`stages.rs` 那一处写着为什么）：挑显示标题、
    /// 剥正题用的正是工作目录里那两份——那是人**改过的说法**。悄悄换成内置的，屏上这几行的名字就与
    /// 他导出去看见的对不上，而且查不出为什么。读不动时把那句话摆在屏上（[`Self::error`]），这一层不开。
    ///
    /// 界面上按那颗按钮走的就是它，实测与测试拿它当那一下。
    pub fn open_exceptions(&mut self, site: &Site, name: &str) {
        if let Some(why) = &self.rules_error {
            self.error = Some(format!(
                "{why}。手动例外那一层要拿它剥认不出作品的那几行的名字——\
                 换一份剥出来的名字与导出去的对不上，所以不开。"
            ));
            return;
        }
        let priorities = match romcat_core::sync::prepare::priorities(None, &self.workspace) {
            Ok(priorities) => priorities,
            Err(why) => {
                self.error = Some(format!(
                    "优先级表读不动：{why}。手动例外那一层要拿它挑屏上写的作品名——\
                     换一份挑出来的名字与导出去的对不上，所以不开。"
                ));
                return;
            }
        };
        self.exceptions = Some(Exceptions {
            name: name.to_string(),
            // **默认停在「包含」那一栏**，照设计稿 `DLG.excl` 的 `st.tab='包含'`。
            tab: Exception::Include,
            query: String::new(),
            note: String::new(),
            rows: Vec::new(),
            found: None,
            priorities,
        });
        self.reload_exception_rows(site);
    }

    /// 「手动例外」那层弹层开着时管的是哪一台。
    #[must_use]
    pub fn exceptions_open(&self) -> Option<&str> {
        self.exceptions.as_ref().map(|open| open.name.as_str())
    }

    /// 弹层停在哪一栏。
    #[must_use]
    pub fn exception_tab(&self) -> Option<Exception> {
        self.exceptions.as_ref().map(|open| open.tab)
    }

    /// 换一栏。界面上按那两颗分段按钮走的就是它。
    pub fn show_exception_tab(&mut self, tab: Exception) {
        if let Some(open) = &mut self.exceptions {
            open.tab = tab;
        }
    }

    /// 关上「手动例外」那层弹层。
    pub fn close_exceptions(&mut self) {
        self.exceptions = None;
    }

    /// 弹层里那张表眼下摆着哪几条（两栏合在一起，按记下的时刻倒着排）。
    #[must_use]
    pub fn exception_rows(&self) -> &[ExceptionDetail] {
        self.exceptions.as_ref().map_or(&[], |open| &open.rows)
    }

    /// 搜索框里打字那一下：换掉框里的字并搜一遍（**字变了才真去问库**，见 [`Self::exception_hits`]）。
    /// 界面上打字走的就是它，测试拿它当打字那一下。
    pub fn set_exception_search(&mut self, site: &Site, text: &str) {
        if let Some(open) = &mut self.exceptions {
            open.query = text.to_string();
        }
        self.search_exceptions(site);
    }

    /// 搜出来那几行屏上写的名字，按次序。
    #[must_use]
    pub fn exception_hits(&self) -> Vec<&str> {
        self.exceptions
            .as_ref()
            .and_then(|open| open.found.as_ref())
            .map_or_else(Vec::new, |(_, hits)| {
                hits.iter().map(|hit| hit.name.as_str()).collect()
            })
    }

    /// 备注框里那句「为什么」。界面上打字走的就是它。
    pub fn set_exception_note(&mut self, text: &str) {
        if let Some(open) = &mut self.exceptions {
            open.note = text.to_string();
        }
    }

    /// 搜出来的**第 `at` 行按下去**：把那个作品底下那几个变体整批记成眼下这一栏的例外，备注是框里那句。
    ///
    /// 例外落在**变体**这一层（ADR-0016），一个作品底下常常挂着好几个——整批一个事务写完
    /// （`Catalog::set_exceptions`）。**同一个作品从一栏换到另一栏不会留下两条**：那是同一条
    /// upsert 的事，包含与排除是同一个决定的两面。
    ///
    /// 记完之后这一台缓着的差量与容量账全部作废（[`Self::forget`]）并当场重算一遍——屏上不留一份改之前的账。
    ///
    /// 界面上按那一行走的就是它，实测与测试拿它当那一下。
    pub fn add_exception(&mut self, site: &mut Site, tasks: &mut Tasks, at: usize) {
        let Some((name, tab, note, anchor, shown)) = self.exceptions.as_ref().and_then(|open| {
            let hit = open.found.as_ref()?.1.get(at)?;
            let note = open.note.trim().to_string();
            Some((
                open.name.clone(),
                open.tab,
                (!note.is_empty()).then_some(note),
                hit.anchor.clone(),
                hit.name.clone(),
            ))
        }) else {
            return;
        };
        // **作用范围靠核心展开**（`Catalog::scoped_variants`，与浏览屏批量操作同一处）：屏上这一行写着
        // 几个变体，按下去就记几条——两处各数一遍的话，写进去的与屏上写着的迟早对不上。
        let keys = match site.catalog.scoped_variants(
            &WorkQuery::default(),
            Scope::Rows(std::slice::from_ref(&anchor)),
        ) {
            Ok(keys) => keys,
            Err(error) => {
                self.error = Some(format!("中立库读不动：{error}"));
                return;
            }
        };
        if keys.is_empty() {
            self.error = Some(format!("「{shown}」底下一个变体都没有，没东西可记。"));
            return;
        }
        let borrowed: Vec<&str> = keys.iter().map(String::as_str).collect();
        match site
            .catalog
            .set_exceptions(&name, &borrowed, tab, note.as_deref())
        {
            Ok(written) => {
                // **加完把两个框清空**（设计稿 `DLG.excl` 的 `st.q=''; st.note=''`）：备注是**这一条**
                // 的「为什么」，留着的话下一次添加会默默带上上一条那句话。
                if let Some(open) = &mut self.exceptions {
                    open.query.clear();
                    open.note.clear();
                    open.found = None;
                }
                self.after_exception_changed(site, tasks, &name);
                self.notice = Some(format!(
                    "给「{name}」{}了「{shown}」：{} 个变体。盘上的文件一个都没动；容量正在重算。",
                    tab.shown(),
                    thousands(written as u64),
                ));
            }
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 一条例外上按「**撤销**」：忘掉它——从此这个变体进不进选择集重新由规则说了算。
    ///
    /// 盘上的文件一个字节都不动（ADR-0004、ADR-0015）：下一趟差量预览与同步照新的选择集去对。
    ///
    /// 界面上按那颗按钮走的就是它，实测与测试拿它当那一下。
    pub fn undo_exception(&mut self, site: &mut Site, tasks: &mut Tasks, key: &str) {
        let Some(name) = self.exceptions.as_ref().map(|open| open.name.clone()) else {
            return;
        };
        match site.catalog.clear_exception(&name, key) {
            Ok(true) => {
                self.after_exception_changed(site, tasks, &name);
                self.notice = Some(format!(
                    "撤销了「{name}」上的一条例外：{key}。这一份进不进选择集重新由规则说了算；\
                     盘上的文件一个都没动，容量正在重算。"
                ));
            }
            Ok(false) => self.notice = Some("那一条例外已经不在了。".to_string()),
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 例外动过之后收的那几样：缓着的差量与容量账作废、当场重算一遍、弹层里那张表重读，
    /// 原先排过差量的话底边提示条上说一句。
    ///
    /// **一起收**，因为它们描述的都是改之前那一套：合计、容量条与差量预览各留一份旧的，
    /// 屏上就会同时摆着三个互相矛盾的数。**那份差量失效要说出来**（票面那一条）——
    /// 「同步」认的正是它（ADR-0016），不说的话人下一步会去按一颗已经灰掉的按钮。
    fn after_exception_changed(&mut self, site: &mut Site, tasks: &mut Tasks, name: &str) {
        let 排过差量 = self.prepared.is_some();
        // **浏览屏可能正开着同一个子库**（挂单 `Q812`）：它手上缓着一份例外，留个记号让窗口转告它重读。
        self.touched = Some(name.to_string());
        self.forget(site, name);
        self.reload_exception_rows(site);
        self.evaluate(site, tasks);
        if 排过差量 {
            self.undo = None;
            self.saved = Some(Toast::new(format!(
                "改过「{name}」的手动例外。差量预览已失效，同步前需要重新生成。"
            )));
        }
    }

    /// 把弹层里那张表从中立库重读一遍。**不在画帧里读**：只有打开与加减之后各一趟。
    fn reload_exception_rows(&mut self, site: &Site) {
        let Some((name, priorities)) = self
            .exceptions
            .as_ref()
            .map(|open| (open.name.clone(), open.priorities.clone()))
        else {
            return;
        };
        let Some(rules) = self.rules.clone() else {
            return;
        };
        match site
            .catalog
            .sublibrary_exception_details(&name, &priorities, &rules)
        {
            Ok(rows) => {
                if let Some(open) = &mut self.exceptions {
                    open.rows = rows;
                }
            }
            Err(error) => self.error = Some(format!("中立库读不动：{error}")),
        }
    }

    /// 搜索框里的字变了才搜一遍。**不在画帧里查库**：一次要问两趟库，而这一屏每秒画几十帧。
    fn search_exceptions(&mut self, site: &Site) {
        let Some((text, priorities, 搜过)) = self.exceptions.as_ref().map(|open| {
            (
                open.query.trim().to_string(),
                open.priorities.clone(),
                open.found.as_ref().map(|(was, _)| was.clone()),
            )
        }) else {
            return;
        };
        if 搜过.as_deref() == Some(text.as_str()) {
            return;
        }
        if text.is_empty() {
            if let Some(open) = &mut self.exceptions {
                open.found = Some((text, Vec::new()));
            }
            return;
        }
        let Some(rules) = self.rules.clone() else {
            return;
        };
        let query = WorkQuery {
            search: text.clone(),
            ..WorkQuery::default()
        };
        match site
            .catalog
            .work_page_with_titles(&query, 0, SEARCH_HITS, &priorities)
        {
            Ok(rows) => {
                let hits = rows
                    .into_iter()
                    .map(|row| Found {
                        // **认不出作品的那一行叫什么由核心答**（`WorkRow::title`，浏览屏主列表
                        // 走的是同一支）：那是那个变体的**正题**，不是整串相对路径。这一层不另写一套
                        // ——写了，同一份内容在两屏上就是两个名字（ADR-0024）。
                        name: crate::table::unlinked_title(&row, &rules)
                            .or_else(|| row.display.clone())
                            .unwrap_or_else(|| row.name.clone()),
                        who: match &row.anchor {
                            WorkAnchor::Work(_) => Who::Work(row.name.clone()),
                            WorkAnchor::Loose(key) => Who::Loose(key.clone()),
                        },
                        anchor: row.anchor,
                        platforms: row.platforms,
                        variants: row.variants,
                        bytes: row.bytes,
                    })
                    .collect();
                if let Some(open) = &mut self.exceptions {
                    open.found = Some((text, hits));
                }
            }
            Err(error) => self.error = Some(format!("中立库读不动：{error}")),
        }
    }

    /// 「**手动例外**」那层弹层（设计稿 `DLG.excl`）：包含与排除两栏，逐条写清作品、平台、体积、备注与时间，
    /// 每一条都撤得掉；底下搜作品直接加。
    ///
    /// **屏上写明例外优先于规则、永久记住**（ADR-0016）——这不是一句客套话：例外表达的是规则表达不了的
    /// 个人口味，人得知道它不会被下一次改规则冲掉。**改完之后差量预览失效**也写在屏上：同步前必须重排一趟。
    fn exceptions_dialog_ui(&mut self, ctx: &egui::Context, site: &mut Site, tasks: &mut Tasks) {
        let Some(open) = &self.exceptions else {
            return;
        };
        let (name, tab) = (open.name.clone(), open.tab);
        let (包含, 排除) = exception_tally_of(&open.rows);
        // **页脚只有一颗**（设计稿 `DLG.excl` 的 `foot`），所以它就是退出那一颗：Esc 与它按下去是同一件事。
        // 这一层没有「取消」——例外是**一按就记下的**，没有半截状态要回滚。
        // 照稿摆法：说明字靠左、「完成」贴右当主按钮（[`Footer::dismiss_on_right`] + [`Dialog::footer_note`]）。
        let footer =
            Footer::new(Button::new("完成", ()).hover("例外是一按就记下的，这颗只是关上这一层。"))
                .dismiss_on_right();
        let tokens = Tokens::builtin();
        let clock = self.exception_clock;
        let mut 换栏 = None;
        let mut 撤了 = None;
        let mut 加了 = None;
        let mut 搜的 = open.query.clone();
        let mut 备注 = open.note.clone();
        // 两栏摆在**标头底下、分隔线上头**那一排里（设计稿 `DLG.excl` 的 `head` 那个 `.dtabs`）：
        // 内容区滚下去之后还看得出自己在哪一栏。栏名后面跟着这一栏几条。
        let 栏 = [
            (Exception::Include, format!("包含 {}", thousands(包含))),
            (Exception::Exclude, format!("排除 {}", thousands(排除))),
        ];
        let shown = Dialog::new("手动例外", format!("手动例外 · {name}"), footer)
            .note(
                "例外优先于规则，并且永久记住。「包含」是规则没选中也要带上的，\
                 「排除」是规则选中了也不带的。",
            )
            .head(|ui| {
                let 摆法: Vec<(Exception, &str)> = 栏
                    .iter()
                    .map(|(kind, label)| (*kind, label.as_str()))
                    .collect();
                if let Some(按了) = look::segmented(ui, &摆法, tab) {
                    换栏 = Some(按了);
                }
            })
            // **改完之后差量预览失效要说在屏上**（票面那一条，照稿摆在页脚左边）：同步认的正是那一份
            // （ADR-0016）。这一句一直摆着，不等人真的改了才冒出来——它说的是这一层弹层的规矩。
            .footer_note("修改例外后，同步前需要重新生成差量预览。")
            .width(Width::Wide)
            .show(ctx, |ui| {
                let Some(open) = &self.exceptions else {
                    return;
                };
                let 这一栏: Vec<&ExceptionDetail> = open
                    .rows
                    .iter()
                    .filter(|detail| detail.row.kind == tab)
                    .collect();
                if 这一栏.is_empty() {
                    // **空态写明从哪儿加**（票面那一条）：这一栏的例外眼下都是从哪儿来的。
                    look::note_box(ui, |ui| {
                        ui.label(
                            egui::RichText::new(match tab {
                                // **这一栏这一版不照稿**（票面 F3）：稿上写的是「在浏览中勾选作品，
                                // 『加入子库…』时选『只加入勾选的作品』」，而那层对话框是票 23、眼下
                                // 还不存在——照稿写等于在空态上指一条按不着的路，而空态的全部价值
                                // 就是告诉人下一步去哪儿。票 23 落地后换回稿上那句。
                                Exception::Include => {
                                    "还没有手动包含的作品。在下面搜作品直接添加；\
                                     也可以在浏览屏的详情面板里对着某一份按「包含它」。"
                                }
                                // 这一栏**逐字照稿**：它指的两条路眼下都有。
                                Exception::Exclude => {
                                    "还没有排除的作品。容量超限时，删减建议里的「排除」会记在这里；\
                                     也可以在下面搜索添加。"
                                }
                            })
                            .size(look::font_size(ui.ctx(), tokens.font.size_small_plus)),
                        );
                    });
                } else {
                    撤了 = exception_table_ui(ui, &这一栏, clock);
                }
                ui.add_space(step(2));
                look::section(ui, &format!("添加{}", tab.shown()));
                ui.horizontal(|ui| {
                    let 备注宽 = tokens.layout.exception_note_width;
                    let 搜索宽 = (ui.available_width() - 备注宽 - ui.spacing().item_spacing.x)
                        .max(tokens.layout.exception_note_width);
                    look::text_input(
                        ui,
                        搜索宽,
                        egui::TextEdit::singleline(&mut 搜的)
                            .id_salt("例外搜作品")
                            .hint_text("搜索作品名称"),
                    );
                    look::text_input(
                        ui,
                        备注宽,
                        egui::TextEdit::singleline(&mut 备注)
                            .id_salt("例外备注")
                            .hint_text("备注（可选）"),
                    );
                });
                if !open.query.trim().is_empty() {
                    let hits = open.found.as_ref().map_or(&[][..], |(_, hits)| &hits[..]);
                    if hits.is_empty() {
                        look::help(ui, "没有匹配的作品");
                    }
                    for (at, hit) in hits.iter().enumerate() {
                        // 这一行**眼下有没有例外、是哪一向**：照稿在那一行末尾说一句，免得人以为自己加了两遍。
                        // **按身份数，不按屏上那串字比**（两个同名的作品是真实存在的）。
                        if hit_row_ui(ui, hit, 这一行的例外(&open.rows, hit)) {
                            加了 = Some(at);
                        }
                    }
                }
            });
        // **画完这一帧才改**：上面那一段借着 `self.exceptions`，写库与重读都得等它还回来。
        //
        // 次序要紧：两个框里的字先写回去（`add_exception` 记下的备注就是人这一帧打的那句），
        // 按下去的那几下跟着办，**搜一遍摆在最后**——搜出来的那一列会整份换掉，
        // 而 `加了` 是这一帧那一列里的第几行。
        self.set_exception_note(&备注);
        if let Some(按了) = 换栏 {
            self.show_exception_tab(按了);
        }
        if let Some(key) = 撤了 {
            self.undo_exception(site, tasks, &key);
        }
        if let Some(at) = 加了 {
            self.add_exception(site, tasks, at);
        }
        // **搜一遍摆在最后**，而且走的是与打字同一条路（[`Self::set_exception_search`]）：
        // 搜出来的那一列会整份换掉，而上面 `加了` 是这一帧那一列里的第几行。
        // 加完那一下把框清空了，这儿照着清空之后的字搜——正好把那一列收掉。
        let 框里 = self
            .exceptions
            .as_ref()
            .map_or_else(String::new, |open| open.query.clone());
        self.set_exception_search(site, if 加了.is_some() { &框里 } else { &搜的 });
        if shown.pressed.is_some() {
            self.close_exceptions();
        }
    }

    /// 底边那条提示条（[`crate::toast`]）：删掉一台之后那一条，带「撤销」。停够了收起，撤销也跟着没了。
    fn toast_ui(&mut self, ctx: &egui::Context, site: &mut Site) {
        if self.undo.is_none() {
            if let Some(saved) = &mut self.saved
                && saved.show(ctx) == toast::Shown::Expired
            {
                self.saved = None;
            }
            return;
        }
        let Some(undo) = &mut self.undo else {
            return;
        };
        match undo.toast.show(ctx) {
            toast::Shown::Showing => {}
            toast::Shown::Pressed => self.undo_remove(site),
            toast::Shown::Expired => self.undo = None,
        }
    }
}

/// 一颗**主按钮**：强调色底。颜色只从 [`look::primary_button`] 那一处来，换在一个 `scope` 里，
/// 别的控件不受影响。
fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.scope(|ui| {
        look::primary_button(ui.visuals_mut());
        ui.button(label)
    })
    .inner
}

/// 一条**提示框**（设计稿 `.note`）：框照 [`look::note`] 画，字照稿是半号那一档（令牌 `size-small-plus`，稿 12.5）。
///
/// 不改共用的 [`look::note`]：开场那几张基线里也有它，字号跟着变就得重批那几张。
fn note_ui(ui: &mut egui::Ui, text: &str) {
    // 半号字按像素倍率取整（[`look::font_size`]）：1 倍屏上 12.5 的中文字距忽宽忽窄。
    let 字号 = look::font_size(ui.ctx(), Tokens::builtin().font.size_small_plus);
    look::note(ui, egui::RichText::new(text).size(字号));
}

/// 一颗**幽灵按钮**（设计稿 `.btn.ghost`）：没底没框、次要字色。颜色只从 [`look::ghost_button`] 那一处来。
fn ghost_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.scope(|ui| {
        look::ghost_button(ui.visuals_mut());
        ui.button(label)
    })
    .inner
}

/// **还没有子库**时那一块：一张居中的卡（[`look::card`]），说清子库是什么，给一颗「新建子库」。
/// 返回按没按。
///
/// 宽照设计稿那张空态卡（620 点），与弹层第二档同宽，取令牌里那一档；内边距照稿（令牌 `empty-card-padding`）。
fn empty_ui(ui: &mut egui::Ui) -> bool {
    let width = Tokens::builtin().layout.dialog_width[1].min(ui.available_width());
    let mut clicked = false;
    ui.add_space(step(4));
    ui.horizontal(|ui| {
        ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
        ui.vertical(|ui| {
            ui.set_width(width);
            look::card(
                ui,
                egui::Vec2::splat(Tokens::builtin().layout.empty_card_padding),
                |ui| {
                    ui.vertical_centered(|ui| {
                        // **字号直接取令牌，不取具名档 `look::TITLE`**：空库上窗口的头一帧就画到
                        // 这里，而具名档是 `App::prepare` 在那一帧里才装上的——这一帧的 `Ui`
                        // 拿的还是装之前那份样式，按名字找会当场 panic。
                        ui.label(
                            font::strong("还没有子库")
                                .size(Tokens::builtin().font.size_empty_title),
                        );
                        ui.add_space(step(1));
                        ui.label(
                            egui::RichText::new(
                                "子库是为一台设备（通常是掌机）挑出来的一部分内容。\
                                 新建一个子库，然后在「浏览」中按平台分几次加入。",
                            )
                            .weak(),
                        );
                        // 说明与按钮之间照稿（`margin:8px 0 16px`）。
                        ui.add_space(step(3));
                        clicked = look::buttons(ui, |ui| primary_button(ui, "新建子库")).clicked();
                    });
                },
            );
        });
    });
    clicked
}

/// 容量条上「还不知道」那一段的斜纹（设计稿 `.gauge .unk`）：强一级描边色的斜条，一个来回一道，一半有色。
///
/// 描边颜色取 `widgets.inactive.bg_stroke`，由 [`look::install`] 照令牌 `line-2` 装好；一个来回多宽取令牌
/// `gauge-hatch`，斜条宽是它的一半。
fn hatch(painter: &egui::Painter, rect: egui::Rect, stroke: egui::Stroke) {
    if rect.width() <= 0.0 {
        return;
    }
    let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let gap = Tokens::builtin().layout.gauge_hatch;
    let stroke = egui::Stroke::new(gap / 2.0, stroke.color);
    let mut x = rect.left() - rect.height();
    while x < rect.right() {
        painter.line_segment(
            [
                egui::pos2(x, rect.bottom()),
                egui::pos2(x + rect.height(), rect.top()),
            ],
            stroke,
        );
        x += gap;
    }
}

/// 容量条图例前那一小块颜色（设计稿 `.legend i`）。**颜色不是唯一线索**：后面一定跟着那一段的名字。
fn legend_swatch(ui: &mut egui::Ui, color: egui::Color32) {
    let layout = &Tokens::builtin().layout;
    let side = layout.legend_swatch;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, layout.legend_swatch_radius, color);
}

/// 规则块里一行的内边距（设计稿 `.rule` 的 8 × 12）。
fn block_margin() -> egui::Margin {
    egui::Margin::from(egui::vec2(step(2), step(1)))
}

/// 规则块里的一行（设计稿 `.rule`）：上面一道分隔线，行首一个序号圆，中间一段，右边一段。
///
/// **序号圆画在中间那一段的竖向中线上**（稿上 `.rule` 是 `align-items:center`）：横排按行高摆的话，中间只有
/// 一行字时圆比字低半截（第二段「没有手动例外」那一行就这么歪过）。所以圆先只占一格宽，等中间那一段摆完再画。
/// 这一行最矮是序号圆那么高，不是按钮那么高——行里没有按钮。
fn block_row(
    ui: &mut egui::Ui,
    badge: &str,
    middle: impl FnOnce(&mut egui::Ui),
    right: impl FnOnce(&mut egui::Ui),
) {
    ui.add(egui::Separator::default().spacing(0.0));
    egui::Frame::new()
        .inner_margin(block_margin())
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let side = Tokens::builtin().layout.rule_badge;
            ui.spacing_mut().interact_size.y = side;
            ui.horizontal(|ui| {
                let (slot, _) =
                    ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
                let middle = ui.vertical(middle).response.rect;
                ui.with_layout(Layout::right_to_left(Align::Center), right);
                badge_ui(ui, egui::pos2(slot.center().x, middle.center().y), badge);
            });
        });
}

/// 行首那个序号圆（设计稿 `.rule .rn`）：凹陷底、弱字色、等宽角标字，圆心在 `center`。
fn badge_ui(ui: &egui::Ui, center: egui::Pos2, text: &str) {
    let tokens = Tokens::builtin();
    let visuals = ui.visuals();
    ui.painter().circle_filled(
        center,
        tokens.layout.rule_badge / 2.0,
        visuals.extreme_bg_color,
    );
    ui.painter().text(
        center,
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::monospace(tokens.font.size_caption),
        visuals.weak_text_color(),
    );
}

/// 例外分两向各有几条：`(包含, 排除)`。
fn exception_tally(rows: &[ExceptionRow]) -> (u64, u64) {
    tally_kinds(rows.iter().map(|row| row.kind))
}

/// 同 [`exception_tally`]，数的是弹层里那张表上的那几条（[`ExceptionDetail`]）。
fn exception_tally_of(rows: &[ExceptionDetail]) -> (u64, u64) {
    tally_kinds(rows.iter().map(|detail| detail.row.kind))
}

/// 一串方向分两向各有几条。**只在这一处数**：卡上那一行与弹层两栏上的角标得是同一个口径。
fn tally_kinds(kinds: impl Iterator<Item = Exception>) -> (u64, u64) {
    let mut 包含 = 0;
    let mut 排除 = 0;
    for kind in kinds {
        match kind {
            Exception::Include => 包含 += 1,
            Exception::Exclude => 排除 += 1,
        }
    }
    (包含, 排除)
}

/// 手动例外那张表（设计稿 `DLG.excl` 的 `.ltbl`）：一行一条例外——作品、平台、体积、备注、时间，行尾一颗「撤销」。
///
/// 返回这一帧按了「撤销」的那一条指着的**变体的键**；这一层只画，忘掉它由调用方做
/// （[`Screen::undo_exception`]）。
///
/// **一行是一个变体不是一个作品**：例外落在变体这一层（ADR-0016），一个作品底下那几份各自进出。
/// 主栏写那一行**屏上叫什么**（[`ExceptionDetail::display`]：认出作品的是显示标题，认不出的是那个
/// 变体的**正题**——两样都由核心一处答，界面不另写一套，ADR-0024），副行一律写**变体的键**，
/// 那是那份内容在主库里的相对路径、指得准是哪一份。
///
/// **库里眼下没有那个变体**（盘没插、目录改了名）时体积那一格写「—」并在悬停里说清，**不写 0**：
/// 「没有这一份」与「这一份是空的」不是一件事，而例外照旧记着、不删。
fn exception_table_ui(
    ui: &mut egui::Ui,
    rows: &[&ExceptionDetail],
    clock: RecordClock,
) -> Option<String> {
    let mut 撤了 = None;
    let tokens = Tokens::builtin();
    let [平台宽, 体积宽, 时间宽, 撤销宽] = tokens.layout.exception_table_columns;
    let caption = tokens.font.size_caption;
    let visuals = ui.visuals().clone();
    egui::Frame::new()
        .stroke(visuals.widgets.noninteractive.bg_stroke)
        .corner_radius(tokens.radius.large)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            // 「作品」与「备注」分余下的宽，作品那一列宽一些：它底下还压着一行变体的键。
            let 余下 = (ui.available_width()
                - 平台宽
                - 体积宽
                - 时间宽
                - 撤销宽
                - 5.0 * ui.spacing().item_spacing.x
                - 2.0 * f32::from(block_margin().left))
            .max(0.0);
            let 作品宽 = 余下 * 0.6;
            let 备注宽 = 余下 - 作品宽;
            let cell = |ui: &mut egui::Ui, width: f32, add: &mut dyn FnMut(&mut egui::Ui)| {
                ui.allocate_ui_with_layout(
                    egui::vec2(width, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_width(width);
                        add(ui);
                    },
                );
            };
            egui::Frame::new()
                .inner_margin(block_margin())
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        for (text, width) in [
                            ("作品", 作品宽),
                            ("平台", 平台宽),
                            ("体积", 体积宽),
                            ("备注", 备注宽),
                            ("时间", 时间宽),
                            ("", 撤销宽),
                        ] {
                            cell(ui, width, &mut |ui| {
                                ui.label(egui::RichText::new(text).small().weak());
                            });
                        }
                    });
                });
            for detail in rows {
                look::divider(ui);
                egui::Frame::new()
                    .inner_margin(block_margin())
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            cell(ui, 作品宽, &mut |ui| {
                                ui.add(
                                    egui::Label::new(font::strong(detail.display.as_str())).wrap(),
                                );
                                // 副行一律印**变体的键**。认不出作品的那一行主栏印的是那个变体的
                                // **正题**（剥过噪音的那串字），与键不是同一串——两行各说一件事，
                                // 与浏览屏主列表一个样。
                                ui.add(
                                    egui::Label::new(
                                        font::mono(&detail.row.variant_key).size(caption).weak(),
                                    )
                                    .wrap(),
                                );
                            });
                            cell(ui, 平台宽, &mut |ui| {
                                ui.label(detail.platform.as_deref().unwrap_or("—"));
                            });
                            cell(ui, 体积宽, &mut |ui| {
                                // **「库里眼下有没有这一份」由核心答**（`ExceptionDetail::missing`），
                                // 这儿不自己拿容量在不在去猜。
                                if detail.missing() {
                                    ui.weak("—").on_hover_text(
                                        "这个变体眼下不在库里（盘没插、目录改了名、重新成型换了键）。\
                                         例外是永久记住的，照旧记着、不删。",
                                    );
                                } else {
                                    ui.label(human_bytes(detail.bytes.unwrap_or(0)));
                                }
                            });
                            cell(ui, 备注宽, &mut |ui| match detail.row.note.as_deref() {
                                Some(note) if !note.is_empty() => {
                                    ui.add(egui::Label::new(note).wrap());
                                }
                                _ => {
                                    ui.weak("—");
                                }
                            });
                            cell(ui, 时间宽, &mut |ui| {
                                ui.label(
                                    font::mono(clock.short(detail.row.at))
                                        .size(caption)
                                        .weak(),
                                );
                            });
                            cell(ui, 撤销宽, &mut |ui| {
                                if look::small_ghost_button(ui, "撤销")
                                    .on_hover_text(
                                        "忘掉这一条：这一份进不进选择集重新由规则说了算。\
                                         设备上的文件一个字节都不动。",
                                    )
                                    .clicked()
                                {
                                    撤了 = Some(detail.row.variant_key.clone());
                                }
                            });
                        });
                    });
            }
        });
    撤了
}

/// 搜出来这一行底下**眼下记着几条例外、各是哪一向**：`(包含几条, 排除几条)`。
///
/// **按身份数**（[`Who`]）：认出作品的比 `work` 表里那个名字，认不出的比变体的键。
/// 拿屏上那串字比会把两个同名的作品当成一个——`work` 表的 `name` 上刻意没有 `UNIQUE`
/// （`catalog::content` 里那段注释），同名异作是真实存在的。
fn 这一行的例外(rows: &[ExceptionDetail], hit: &Found) -> (u64, u64) {
    tally_kinds(
        rows.iter()
            .filter(|detail| match &hit.who {
                Who::Work(name) => detail.work.as_deref() == Some(name.as_str()),
                Who::Loose(key) => detail.row.variant_key == *key,
            })
            .map(|detail| detail.row.kind),
    )
}

/// 搜索框底下的一行命中（设计稿 `.srch` 的一颗按钮）：平台、作品名、底下几个变体多大，
/// 已经有例外的那一行说清**现在是哪一栏**。按下去返回 `true`。
///
/// `记着的` 是这一行底下眼下记着几条例外（[`这一行的例外`]）。**说得准**：整份都在一向时才写
/// 「现在是「X」」；只有一部分、或者两向都有时如实写记着几条——一个作品底下五个变体只排除了一个，
/// 写「现在是「排除」」是在骗人。
fn hit_row_ui(ui: &mut egui::Ui, hit: &Found, 记着的: (u64, u64)) -> bool {
    let 平台 = if hit.platforms.is_empty() {
        "平台未知".to_string()
    } else {
        hit.platforms.join("、")
    };
    let 现在 = match 记着的 {
        (0, 0) => String::new(),
        (包含, 0) if 包含 == hit.variants => {
            format!(" · 现在是「{}」", Exception::Include.shown())
        }
        (0, 排除) if 排除 == hit.variants => {
            format!(" · 现在是「{}」", Exception::Exclude.shown())
        }
        (包含, 排除) => format!(
            " · 底下已经记着 {} 条例外",
            thousands(包含.saturating_add(排除))
        ),
    };
    let 一行 = format!(
        "{平台} · {} · {} 个变体 · {}{现在}",
        hit.name,
        thousands(hit.variants),
        human_bytes(hit.bytes),
    );
    look::small_buttons(ui, |ui| {
        ui.add(egui::Button::new(一行).wrap())
            .on_hover_text(
                "把这个作品底下那几个变体整批记成这一栏的例外。\
                 已经在另一栏的会换过来，不会留下两条。",
            )
            .clicked()
    })
}

/// 摊开之后那张步骤表。**虚拟化，而且一步都不截**；返回这一帧真的画了几行。
///
/// ## 为什么不再截在 2,000 步（挂账 `D158`）
///
/// 真机量级上一次同步动上万个文件（实测合成数据 **21,571 步**，`--bench-sublibrary`
/// 跑出来的；挂账 `D158` 记的 8,206 是票 `parking-3/17` 换合成数据形状之前那个数）。
/// 截断的代价不是「少看几行」——是「想确认第 5,000 步是什么就得转去命令行」，
/// 而**同步前必须看一遍它要做什么**是 ADR-0016 的硬要求，那一眼不该有一半落在界面之外。
///
/// 不截的代价是零：计划整份本来就在界面状态里（[`Screen::prepared`]），而
/// `TableBody::rows` 只调用视口里那几十行的闭包——**一帧画几行只跟视口有多高有关，
/// 与总步数无关**。返回的正是这个数，[`Screen::steps_drawn`] 把它交给实测与测试，
/// 于是这句话是被数出来的，不是被相信的。
///
/// `scroll_to` 把滚动位置强按到某个像素偏移，**只有量帧率与测试才给**。
pub fn steps_table(
    ui: &mut egui::Ui,
    plan: &romcat_core::sync::Plan,
    scroll_to: Option<f32>,
) -> usize {
    let shown = plan.steps.len();
    let mut drawn = 0;
    let mut builder = egui_extras::TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .id_salt(format!("差量步骤 · {}", plan.sublibrary))
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(egui_extras::Column::initial(60.0).at_least(50.0))
        .column(egui_extras::Column::initial(70.0).at_least(50.0))
        .column(egui_extras::Column::initial(90.0).at_least(70.0))
        .column(egui_extras::Column::remainder().at_least(160.0).clip(true));
    if let Some(offset) = scroll_to {
        builder = builder.vertical_scroll_offset(offset);
    }
    builder
        .header(22.0, |mut header| {
            for title in ["干什么", "类别", "容量", "目标上的路径"] {
                header.col(|ui| {
                    ui.label(font::strong(title));
                });
            }
        })
        .body(|body| {
            body.rows(ROW_HEIGHT, shown, |mut row| {
                let index = row.index();
                let Some(step) = plan.steps.get(index) else {
                    return;
                };
                drawn += 1;
                row.col(|ui| {
                    if step.act == Act::Delete {
                        ui.colored_label(ui.visuals().error_fg_color, step.act.label());
                    } else {
                        ui.label(step.act.label());
                    }
                });
                row.col(|ui| {
                    ui.label(step.kind.label());
                });
                row.col(|ui| {
                    ui.label(human_bytes(if step.act == Act::Delete {
                        step.was
                    } else {
                        step.bytes
                    }));
                });
                row.col(|ui| {
                    ui.label(&step.path);
                });
            });
        });
    drawn
}

/// 「分不出第二份只读连接」那句话。
///
/// **不退到画帧那条线程上偷偷跑一趟**：那既会僵住窗口，又把真正的问题盖在一句
/// 「怎么卡了一下」底下。排差量预览与算一遍容量两处说的是同一件事，所以话也只写一处。
fn no_second_connection(什么活: &str, why: &CatalogError) -> String {
    format!(
        "{什么活}没开跑：读中立库要另开一份只读连接，这一下没开出来（{why}）。\n\
         先确认中立库那个文件还在、结构版本对得上，再按一次。"
    )
}

/// 一趟同步跑完之后摆在屏上的那句回执。
///
/// **中断、失败、主动停了，一样都不许吞。** 全成功的一趟与半数写失败的一趟若在界面上
/// 长得一样，那句「同步用了 X 秒」就是在骗人（命令行那一侧靠 `Outcome::render_text`
/// 把这几样印全）。
///
/// 耗时取的是**任务台记下的那个数**——与任务屏历史里那一行同一个来源，两处各掐一次表的话，
/// 同一趟活会在两屏上报出两个数。
fn sync_notice(outcome: &Outcome, elapsed: f64) -> String {
    let mut line = format!(
        // **说得出是哪一台**：这句话可能是几十分钟前排上去的那一趟交回来的，
        // 而那会儿摊开的多半是另一张卡了。
        "「{}」同步用了 {elapsed:.1} 秒：动了 {} 个文件（新增 {}、更新 {}、删除 {}）。",
        outcome.sublibrary,
        thousands(outcome.touched()),
        thousands(outcome.added.files),
        thousands(outcome.updated.files),
        thousands(outcome.deleted.files),
    );
    if outcome.interrupted {
        line.push_str(
            "\n⚠️ 这一趟部分完成：按停时目标上没留下写了一半的文件，清单记的是\
             停下那一刻目标上真实有什么，再跑一趟就接上。",
        );
    }
    if outcome.gave_up {
        line.push_str("\n⚠️ 连着失败太多次，主动停了：多半是卡拔了或者写满了。");
    }
    if !outcome.failures.is_empty() {
        line.push_str(&format!(
            "\n⚠️ 有 {} 步没做成：",
            thousands(outcome.failures.len() as u64),
        ));
        for failure in outcome.failures.iter().take(TOP_NOTES) {
            line.push_str(&format!("\n  {}：{}", failure.path, failure.why));
        }
        if outcome.failures.len() > TOP_NOTES {
            line.push_str(&format!(
                "\n  ……另有 {} 步没列",
                outcome.failures.len() - TOP_NOTES,
            ));
        }
    }
    line
}

/// Pegasus 适配器的标识（`romcat_core::adapter::find` 认的那个名字）。
const PEGASUS: &str = "Pegasus";

/// ES 家族那个适配器的标识。**界面上写「ES-DE」**（[`format_label`]），库里存的、命令行认的照旧是它。
const ES_GAMELIST: &str = "ES-Gamelist";

/// 前端格式在界面上**写成什么**（拿主意的人 2026-09-15 定）：ES 家族那个适配器写「ES-DE」，别的照适配器标识写。
/// 只换给人看的那几个字：存进库里的、命令行旗标认的、差量预览排的都还是适配器标识。
fn format_label(adapter: &str) -> &str {
    if adapter.eq_ignore_ascii_case(ES_GAMELIST) {
        "ES-DE"
    } else {
        adapter
    }
}

/// 前端格式底下那一句：**照实际布局写**（拿主意的人 2026-09-15 定，不照稿上的示意）。末尾那半句「前端里的游玩记录和收藏不会被
/// 覆盖」两边都有代码钉着才照稿写：Pegasus 的收藏与游玩时长在它自己的配置目录里，ES-DE 在卡上改过的 gamelist 同步只报不写回
/// （`crates/core/tests/sync.rs` 那两条）。元数据落在哪由适配器答
/// （`Adapter::metadata_path`），媒体目录取适配器模块里那两个常量——界面不另写一份文件名。
fn format_help(adapter: &str, example_directory: Option<&str>) -> String {
    let name = if adapter.trim().is_empty() {
        PEGASUS
    } else {
        adapter.trim()
    };
    let Some(found) = romcat_core::adapter::find(name) else {
        return format!("这一版没带「{name}」这个前端格式。");
    };
    // 举例的那一份由适配器按那个平台目录折出来（`Adapter::metadata_path`）；说不出平台目录时只说文件名。
    let example = example_directory.map(|directory| found.metadata_path(directory));
    if name.eq_ignore_ascii_case(ES_GAMELIST) {
        let where_ = example.map_or_else(
            || format!("每个平台一份 {}", found.file_name()),
            |metadata| format!("每个平台一份，例如 {metadata}"),
        );
        format!(
            "{where_}；媒体放在 {} 目录。前端里的游玩记录和收藏不会被覆盖。",
            romcat_core::adapter::gamelist::MEDIA_DIR
        )
    } else {
        let where_ = example.map_or_else(
            || format!("每个平台一份 {}", found.file_name()),
            |metadata| format!("每个平台一份，例如 {metadata}"),
        );
        format!(
            "{where_}，摊在子库根上；媒体放在 {} 目录。前端里的游玩记录和收藏不会被覆盖。",
            romcat_core::adapter::pegasus::MEDIA_DIR
        )
    }
}

/// 目标在位时路径底下那一句（设计稿 `probePath` 那句「已连接 · 可移动存储 · exFAT · 容量 …，可用 …」）：读得到的才写，
/// 读不到的那几格不编；清单外文件数数完了（`counted`）才接上后半句。
fn connected_line(
    volume: &romcat_core::sublibrary::target::Volume,
    counted: Option<u64>,
) -> String {
    let mut parts = vec![
        "已连接".to_string(),
        if volume.removable {
            "可移动存储".to_string()
        } else {
            "本机磁盘".to_string()
        },
    ];
    if let Some(filesystem) = &volume.filesystem {
        parts.push(filesystem.clone());
    }
    let mut line = parts.join(" · ");
    if let (Some(total), Some(available)) = (volume.total, volume.available) {
        line.push_str(&format!(
            " · 容量 {}，可用 {}",
            human_bytes(total),
            human_bytes(available)
        ));
    }
    line.push('。');
    // 本机磁盘不设上限时按剩余空间算（`Sublibrary::limit_on`，拿主意的人 2026-09-15 照稿定）：照稿说一句。
    if !volume.removable {
        line.push_str("本机磁盘不设容量上限时，按剩余空间计算。");
    }
    if let Some(count) = counted {
        line.push_str(&format!(
            "目录里已有 {} 个文件，它们不在清单里，工具不会改动。",
            thousands(count)
        ));
    }
    line
}

/// 新建子库时「设备上的位置」拿来举例的那一个：设计稿 `DLG.subform` 的示例。目录照实际规则落在平台目录下（`Landing::example`）。
const EXAMPLE_DIRECTORY: &str = "GBA";

/// 同上，示例文件名。
const EXAMPLE_FILE: &str = "火焰之纹章 烈火之剑.gba";

/// 目标路径还没填时「设备上的位置」拿来举例的那个根（设计稿 `DLG.subform` 的 `/Volumes/SDCARD`）。
const EXAMPLE_ROOT: &str = "/Volumes/SDCARD";

/// 把相对子库根的落点接到根后面：根里写的是反斜杠（Windows 盘符路径）就用反斜杠，否则用 `/`。
fn join_under(root: &str, relative: &str) -> String {
    let separator = if root.contains('\\') { '\\' } else { '/' };
    let relative = if separator == '/' {
        relative.to_string()
    } else {
        relative.replace('/', "\\")
    };
    format!(
        "{}{separator}{relative}",
        root.trim_end_matches(['/', '\\'])
    )
}

/// 「设备上的位置」那一块（设计稿 `.ruletext`）：凹陷底、小圆角、等宽小字，两行——ROM 落在哪、元数据落在哪。
/// 内边距与浏览屏那条规则原文同一个令牌（`rule-text-padding`）。
fn landing_box_ui(ui: &mut egui::Ui, root: &str, landing: &romcat_core::sync::Landing) {
    let tokens = Tokens::builtin();
    let [上下, 左右] = tokens.space.rule_text_padding;
    let visuals = ui.visuals().clone();
    egui::Frame::new()
        .fill(visuals.extreme_bg_color)
        .corner_radius(tokens.radius.small)
        .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            for line in [
                join_under(root, &landing.rom),
                join_under(root, &landing.metadata),
            ] {
                ui.add(
                    egui::Label::new(
                        font::mono(line)
                            .size(tokens.font.size_path)
                            .color(visuals.widgets.noninteractive.fg_stroke.color),
                    )
                    .wrap(),
                );
            }
        });
}

/// 能力档案下拉底下那一句：**照核心的事实拼**（这份档案的文件系统与单文件上限），不照名册里的「说明」——那一格是写给
/// 维护者看的，带着 Markdown 记号与票号。
fn profile_help(profile: &Profile) -> String {
    let filesystem = &profile.filesystem;
    // 这份档案对卡一条约束都不说（「不作声称」那一份）时，「卡是 …」那半句整个不写（`Filesystem::claims_nothing`）。
    if filesystem.claims_nothing() {
        return "决定每个平台放到设备上时要不要转换格式。".to_string();
    }
    let limit = filesystem
        .max_file_bytes
        .map(|bytes| format!("，单文件上限 {}", human_bytes(bytes)))
        .unwrap_or_default();
    format!(
        "决定每个平台放到设备上时要不要转换格式。卡是 {}{limit}。",
        filesystem.name
    )
}

/// 名册里那几格给人看之前去掉 Markdown 记号（`**`、反引号）：来源原文是写给维护者核对的，界面上照字面露出来不像话。
fn plain(text: &str) -> String {
    text.replace("**", "").replace('`', "")
}

/// **能力档案平台表**（设计稿 `DLG.subform` 那张表）：逐平台写「设备直接能用」「不能用时」，每行底下一句来源里说了的「说明」
/// 与「核实日期 …」（陈旧时换成警示色「陈旧」，来源去掉记号放悬停）。
///
/// `rows_of` 是 `Some(平台表)` 时照这台设备选择集里出现的平台列、带「覆盖」那一列（`None` 是还在读）；是 `None` 时（新建子库）
/// 按这份档案的条目列、不带覆盖列（拿主意的人 2026-09-15 定）。判断全在核心：哪个平台走哪一条（`Matrix::entry_for`）、
/// 陈不陈旧（`Claim::is_stale`）。
fn profile_table_ui(
    ui: &mut egui::Ui,
    profile: &Profile,
    rows_of: Option<Option<&[String]>>,
    overrides: &mut BTreeMap<String, Override>,
    today: &str,
) {
    let tokens = Tokens::builtin();
    let layout = &tokens.layout;
    let rows: Vec<(String, Option<&Entry>)> = match rows_of {
        Some(Some(platforms)) => platforms
            .iter()
            .map(|platform| (platform.clone(), profile.matrix.entry_for(Some(platform))))
            .collect(),
        Some(None) => {
            ui.horizontal(|ui| {
                ui.add_space(layout.form_label_width + ui.spacing().item_spacing.x);
                look::help(ui, "正在读这台设备的选择集……");
            });
            return;
        }
        None => profile
            .matrix
            .entries
            .iter()
            .map(|entry| {
                let platforms = if entry.platforms.iter().any(|platform| platform == "*") {
                    "其余平台".to_string()
                } else {
                    entry.platforms.join("、")
                };
                (platforms, Some(entry))
            })
            .collect(),
    };
    let with_overrides = rows_of.is_some();
    let [平台宽, 转成宽, 覆盖宽] = layout.platform_table_columns;
    ui.horizontal(|ui| {
        ui.add_space(layout.form_label_width + ui.spacing().item_spacing.x);
        ui.vertical(|ui| {
            let visuals = ui.visuals().clone();
            egui::Frame::new()
                .stroke(visuals.widgets.noninteractive.bg_stroke)
                .corner_radius(tokens.radius.large)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let total = ui.available_width();
                    let gaps = if with_overrides { 3.0 } else { 2.0 };
                    let 吃宽 = (total
                        - 平台宽
                        - 转成宽
                        - if with_overrides { 覆盖宽 } else { 0.0 }
                        - gaps * ui.spacing().item_spacing.x
                        - 2.0 * f32::from(block_margin().left))
                    .max(0.0);
                    let cell = |ui: &mut egui::Ui, width: f32, add: &mut dyn FnMut(&mut egui::Ui)| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(width, 0.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_width(width);
                                add(ui);
                            },
                        );
                    };
                    egui::Frame::new()
                        .inner_margin(block_margin())
                        .show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                for (text, width) in [("平台", 平台宽), ("设备直接能用", 吃宽), ("不能用时", 转成宽)] {
                                    cell(ui, width, &mut |ui| {
                                        ui.label(egui::RichText::new(text).small().weak());
                                    });
                                }
                                if with_overrides {
                                    cell(ui, 覆盖宽, &mut |ui| {
                                        ui.label(egui::RichText::new("覆盖").small().weak());
                                    });
                                }
                            });
                        });
                    for (platform, entry) in &rows {
                        look::divider(ui);
                        egui::Frame::new()
                            .inner_margin(block_margin())
                            .show(ui, |ui| {
                                ui.horizontal_top(|ui| {
                                    cell(ui, 平台宽, &mut |ui| {
                                        ui.add(egui::Label::new(font::strong(platform.as_str())).wrap());
                                    });
                                    cell(ui, 吃宽, &mut |ui| entry_cell_ui(ui, *entry, today));
                                    cell(ui, 转成宽, &mut |ui| {
                                        ui.label(convert_text(*entry));
                                    });
                                    if with_overrides {
                                        cell(ui, 覆盖宽, &mut |ui| {
                                            let mut chosen = overrides.get(platform).copied();
                                            egui::ComboBox::from_id_salt(("覆盖", platform.as_str()))
                                                .width(覆盖宽)
                                                .selected_text(chosen.map_or("按档案", Override::label))
                                                .show_ui(ui, |ui| {
                                                    ui.selectable_value(&mut chosen, None, "按档案");
                                                    for choice in Override::all() {
                                                        ui.selectable_value(&mut chosen, Some(choice), choice.label());
                                                    }
                                                });
                                            match chosen {
                                                Some(choice) => {
                                                    overrides.insert(platform.clone(), choice);
                                                }
                                                None => {
                                                    overrides.remove(platform);
                                                }
                                            }
                                        });
                                    }
                                });
                            });
                    }
                });
            look::help(
                ui,
                if with_overrides {
                    "只列出这个子库选择集中出现的平台。覆盖只影响这个子库。压缩镜像之间的转换（cue/bin → chd 等）需要外部工具，这一版做不到，遇到时会在差量预览中如实列出。"
                } else {
                    "按这份档案的条目列出；创建后按选择集中实际出现的平台列出，并可以按平台覆盖。压缩镜像之间的转换（cue/bin → chd 等）需要外部工具，这一版做不到，遇到时会在差量预览中如实列出。"
                },
            );
        });
    });
}

/// 平台表「设备直接能用」那一格：吃什么（名册里的写法）、来源里说了的那一句、核实日期（陈旧时一枚「陈旧」）。
fn entry_cell_ui(ui: &mut egui::Ui, entry: Option<&Entry>, today: &str) {
    let Some(entry) = entry.filter(|entry| !entry.accepts.is_anything()) else {
        ui.label("不作声称");
        look::help(ui, "没有核实过，不转换也不检查");
        return;
    };
    ui.add(egui::Label::new(entry.declared.join("、")).wrap());
    if !entry.note.is_empty() {
        look::help(ui, &entry.note);
    }
    ui.horizontal(|ui| {
        look::help(ui, &format!("核实日期 {}", entry.claim.verified))
            .on_hover_text(plain(&entry.claim.cite));
        if entry.claim.is_stale(today) {
            look::plain_chip(ui, look::Tone::Caution, "陈旧");
        }
    });
}

/// 平台表「不能用时」那一格：转成什么（`Recipe`）；不作声称或者转不了是「—」。
fn convert_text(entry: Option<&Entry>) -> &'static str {
    match entry.and_then(|entry| {
        (!entry.accepts.is_anything())
            .then_some(entry.convert_to)
            .flatten()
    }) {
        Some(Recipe::Rezip) => "zip",
        Some(Recipe::Unpack) => "取出为裸文件",
        None => "—",
    }
}

/// 目标路径被核心拦下时，弹层里路径底下那一句（设计稿 `probePath`，稿上画了的三句逐字照稿）。
///
/// **判断不在这儿**（[`target::vet`]）：这里只把核心交回来的理由说成屏上那句话。
fn refusal_line(refusal: &TargetRefusal) -> String {
    match refusal {
        TargetRefusal::Empty => "通常是 SD 卡或掌机存储的根目录。".to_string(),
        TargetRefusal::InLibrary { around: false, .. } => {
            "这个目录在主库的根之内。子库需要写入文件，不能放在只读的主库里。".to_string()
        }
        TargetRefusal::InLibrary { around: true, .. } => {
            "这个目录包含主库的根。子库需要写入文件，不能放在只读的主库里。".to_string()
        }
        TargetRefusal::InWorkspace { .. } => "这个目录属于工作目录，请选择其他目录。".to_string(),
        TargetRefusal::Taken { by } => format!("已被子库「{by}」使用。"),
        TargetRefusal::NotADirectory => "这条路径是一份文件，不是目录。".to_string(),
    }
}

/// 目标设备那个目录不在时那句话：为什么不行、去哪儿办；在就是 `None`。
///
/// **不是判断，是查一眼有没有**（ADR-0005 修订段「原料还没备齐」：盘上缺一样东西）。「在不在」
/// 照核心那一趟看目标时的口径（`sync::observe` 化不开这条路径就报目标不在位），话却是这一层
/// 自己说的：它要指向屏上的哪一处——卡上的「目标设置…」——而那是核心库不该知道的。
/// 排差量预览与同步两颗按钮说的是同一句。
///
/// **在、却不是目录**（那条路径上是一份文件）不归这里：那不是缺一样东西，是一件该去查的事
/// ——那一趟在「看一眼目标」真去列它时撞上（`sync::observe` 报「列不开」），照实记失败。
fn target_absent(root: &std::path::Path) -> Option<String> {
    (!root.exists()).then(|| {
        format!(
            "未连接：{}。插上读卡器再按；目标路径不对的话，按卡上「目标设置…」改。",
            romcat_core::path::display(root)
        )
    })
}

/// 任务台上那趟同步干的活：把计划落到目标上。
///
/// **它拿到的只有计划里那些步骤**——清单之外的路径连进来的门都没有（ADR-0015）。
/// 那份计划是**排它时**那一份：整份搬进了这个闭包，屏上那一份后来怎么变都够不着它。
fn run_sync(
    prepared: &Prepared,
    library_roots: Option<&romcat_core::catalog::Roots>,
    task: &Handle,
) -> Result<Outcome, Cutoff> {
    let sources = sync::Sources {
        library: &romcat_core::fs::RealFs,
        library_roots,
        // 真跑一律给真盘：那道可注入的接缝只为测试造得出的那两格而存在
        // （`sync::execute` 模块文档八）。
        target: &romcat_core::fs::RealFs,
        target_root: &prepared.root,
        from_pool: &prepared.from_pool,
        generated: &prepared.generated,
        // 探测的源那一头是**媒体池自己的临时目录**：两头都得是工具的地盘，
        // 拿主库里的文件去试链接会改到主库那一侧的 inode（ADR-0004）。
        link_probe_dir: Some(&prepared.scratch),
        // **默认不缓存**（ADR-0017）：边转边流式写进目标。
        convert_cache: None,
    };
    sync::execute::run(
        &prepared.plan,
        &prepared.desired,
        &prepared.actual,
        &prepared.manifest,
        &sources,
        task,
    )
    // **同步被按停时不走这条**：它照旧返回 `Ok`，另报一句「停在半路」，
    // 好让那份**清单**留得下来（ADR-0015）。走到这儿的都是真出错了。
    .map_err(|error| Cutoff::failed(format!("目标写不了：{error}")))
}

/// **删减建议表**（设计稿 `trimHTML`）：超出多少，按体积排的每一项释放多少、累计多少，排除到哪一项
/// 就放得下，全排除也还差多少。没超限时什么都不画。返回这一帧按了「排除」的那个变体的键。
///
/// 只求选择集与排完差量预览两处摆的是同一段（[`Room`] 两处都抄自同步计划器），画法也就只写一处。
/// **四个数与两句判断全是核心说的**：释放与累计是 [`romcat_core::sublibrary::Trim`]，「放得下」
/// 「还差」是 [`Room::fits_after`] / [`Room::short_after_all`]，这儿只画（ADR-0005、ADR-0024）。
///
/// **只建议，绝不自动删减**（ADR-0016）：「排除」记成这个子库的一条手动例外，由调用方落库
/// （[`Screen::exclude`]）。
///
/// ## 画法照稿（`.trim`）
///
/// 一圈描边的框：头上一条 `lo-soft` 底（[`look::Tone::Bad`]）写超出多少；表每一行之间一道分隔线，
/// 格子内边距取令牌 `cell-padding`（稿子写的是 6，令牌里没有这一档，挂单 `Q854`），「排除」是小号按钮；
/// 放得下之后的那几行**整行**淡下去（按钮一起）；「排除到这一项就能放下」那一行垫 `hi-soft`
/// （[`look::Tone::Good`]）；全排除也不够时底下再一条 `lo-soft` 底说还差多少。
fn trim_ui(ui: &mut egui::Ui, room: &Room) -> Option<String> {
    let over = room.over_capacity?;
    let fits = room.fits_after();
    let visuals = ui.visuals().clone();
    let (超字, 超底) = look::tone_colors(look::Tone::Bad, &visuals);
    let (放下字, 放下底) = look::tone_colors(look::Tone::Good, &visuals);
    let tokens = Tokens::builtin();
    let [格上下, 格左右] = tokens.space.cell_padding;
    let 格 = egui::Margin::from(egui::vec2(格左右, 格上下));
    // 右边三栏各多宽：照这张表里最宽的那一格量（说明字号），按钮照小号按钮量。
    let 数字 = egui::FontId::proportional(tokens.font.size_small);
    let 量 = |ui: &egui::Ui, text: &str| {
        egui::WidgetText::from(text)
            .into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                数字.clone(),
            )
            .size()
            .x
    };
    let 宽 = [
        room.trim_suggestions
            .iter()
            .map(|trim| 量(ui, &human_bytes(trim.bytes)))
            .fold(量(ui, "释放"), f32::max),
        room.trim_suggestions
            .iter()
            .map(|trim| 量(ui, &human_bytes(trim.cumulative)))
            .fold(量(ui, "累计"), f32::max),
        look::small_button_width(ui, "排除"),
    ];
    let 等宽 = |text: String| font::mono(text).size(tokens.font.size_small);
    // 释放与累计两栏照稿是界面字（设计稿 `.num`）；变体那一栏写的是键，照旧等宽。
    let 数目 = |text: String| egui::RichText::new(text).size(tokens.font.size_small);
    let mut clicked = None;
    egui::Frame::new()
        .stroke(visuals.widgets.noninteractive.bg_stroke)
        .corner_radius(visuals.window_corner_radius)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // 行与行之间不留缝：分隔线就是缝。
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::Frame::new()
                .fill(超底)
                .corner_radius(egui::CornerRadius {
                    sw: 0,
                    se: 0,
                    ..visuals.window_corner_radius
                })
                .inner_margin(格)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            font::strong(format!("超出容量上限 {}", human_bytes(over)))
                                .size(look::font_size(ui.ctx(), Tokens::builtin().font.size_small_plus))
                                .color(超字),
                        );
                        ui.label(
                            egui::RichText::new(
                                "按大小列出可以排除的变体；排除会记为这个子库的手动例外，不会自动删减。",
                            )
                            .size(look::font_size(ui.ctx(), Tokens::builtin().font.size_small_plus))
                            .weak(),
                        );
                    });
                });
            look::divider(ui);
            trim_row(
                ui,
                None,
                宽,
                |ui| {
                    look::help(ui, "变体");
                },
                |ui| {
                    look::help(ui, "释放");
                },
                |ui| {
                    look::help(ui, "累计");
                },
                |_| {},
            );
            for (at, trim) in room.trim_suggestions.iter().enumerate() {
                look::divider(ui);
                // 已经放得下之后的那几项**整行**淡下去（按钮一起）：再往下排除就多砍了。
                let faded = fits.is_some_and(|fits| at > fits);
                ui.scope(|ui| {
                    if faded {
                        ui.multiply_opacity(visuals.disabled_alpha);
                    }
                    trim_row(
                        ui,
                        None,
                        宽,
                        |ui| {
                            ui.add(egui::Label::new(等宽(trim.variant.clone())).truncate());
                        },
                        |ui| {
                            ui.label(数目(human_bytes(trim.bytes)));
                        },
                        |ui| {
                            ui.label(数目(human_bytes(trim.cumulative)));
                        },
                        |ui| {
                            let 按了 = look::small_buttons(ui, |ui| {
                                ui.button("排除")
                                    .on_hover_text(
                                        "记成这个子库的一条排除例外：规则选中了也不带。优先于规则、\
                                         永久记住；盘上的文件一个都不动。",
                                    )
                                    .clicked()
                            });
                            if 按了 {
                                clicked = Some(trim.variant.clone());
                            }
                        },
                    );
                });
                if fits == Some(at) {
                    look::divider(ui);
                    trim_row(
                        ui,
                        Some(放下底),
                        宽,
                        |ui| {
                            ui.colored_label(放下字, "排除到这一项就能放下").on_hover_text(
                                "照计划器记的账估的：作品共用的媒体记在头一个变体名下，同一作品还有\
                                 别的变体留着时，排除它未必省下那几份。按下「排除」之后会当场重算。",
                            );
                        },
                        |_| {},
                        |_| {},
                        |_| {},
                    );
                }
            }
            if let Some(short) = room.short_after_all() {
                look::divider(ui);
                egui::Frame::new()
                    .fill(超底)
                    .corner_radius(egui::CornerRadius {
                        nw: 0,
                        ne: 0,
                        ..visuals.window_corner_radius
                    })
                    .inner_margin(格)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            egui::RichText::new(format!(
                                "全部排除也还差 {}，需要移除规则或换一张更大的卡。",
                                human_bytes(short)
                            ))
                            .size(look::font_size(ui.ctx(), Tokens::builtin().font.size_small_plus)),
                        );
                    });
            }
        });
    clicked
}

/// 删减建议表里的一行：左边一栏占满余下的宽（放不下截断），右边三栏定宽（`宽`：释放、累计、按钮），
/// 数字右对齐。`底` 给了就垫一层底色。
///
/// 格子内边距取令牌 `cell-padding`；一行至少一颗小号按钮高（令牌 `button-small-height`），
/// 于是有按钮的行与没按钮的行一样高——稿子里按钮在格子里，行高不等于默认按钮高。
fn trim_row(
    ui: &mut egui::Ui,
    底: Option<egui::Color32>,
    宽: [f32; 3],
    左: impl FnOnce(&mut egui::Ui),
    释放: impl FnOnce(&mut egui::Ui),
    累计: impl FnOnce(&mut egui::Ui),
    按钮: impl FnOnce(&mut egui::Ui),
) {
    let tokens = Tokens::builtin();
    let [格上下, 格左右] = tokens.space.cell_padding;
    let 行高 = tokens.layout.button_small_height;
    egui::Frame::new()
        .fill(底.unwrap_or(egui::Color32::TRANSPARENT))
        .inner_margin(egui::Margin::from(egui::vec2(格左右, 格上下)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let 缝 = ui.spacing().item_spacing.x;
                let 左宽 = (ui.available_width() - 宽.iter().sum::<f32>() - 3.0 * 缝).max(0.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(左宽, 行高),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.set_min_size(egui::vec2(左宽, 行高));
                        左(ui);
                    },
                );
                for (多宽, 这一格) in [
                    (宽[0], Box::new(释放) as Box<dyn FnOnce(&mut egui::Ui)>),
                    (宽[1], Box::new(累计)),
                    (宽[2], Box::new(按钮)),
                ] {
                    ui.allocate_ui_with_layout(
                        egui::vec2(多宽, 行高),
                        Layout::right_to_left(Align::Center),
                        |ui| {
                            ui.set_min_size(egui::vec2(多宽, 行高));
                            这一格(ui);
                        },
                    );
                }
            });
        });
}
