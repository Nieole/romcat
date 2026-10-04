//! **刮削弹层**「刮削元数据」：字段、数据源、采法三栏，外加按下去之前那本账。
//!
//! 它挂在[浏览屏](crate::browse)上：勾中几个作品，按屏头那颗「刮削…」，弹层摊开——**画成一层弹层**
//! （[`crate::dialog`]），与添加主库那条向导同一种写法。作品详情页那颗「刮削此作品」、右键菜单那一项摊开的是同一层，
//! 范围换成那一个作品（拿主意的人 2026-10-02 裁 `F-15`）。版式照设计稿 `prototype.html` 刮削那一段
//! （票 `gui-draws-the-rest-of-the-design/17`）。
//!
//! ## 四个旋钮，两条轴分得清清楚楚
//!
//! | 旋钮 | 答的是 |
//! |---|---|
//! | **范围** | 对谁——勾中的这一批，**不可就地编辑**（标头那句说清要改去哪儿改） |
//! | **字段** | **我要什么** |
//! | **数据源** | **花多少代价**——本地源照跑、关不掉；联网源默认不勾，勾上即生效 |
//! | **采法** | 跑多久——[补缺 / 重采](Gather) |
//!
//! 「我要什么」与「花多少代价」是两条轴：字段选窄了省的是库里多几行少几行，
//! 源与采法选宽了花的是**配额**——而在线那份配额同时按账号与 IP 计，撞穿了是永久封禁
//! （ADR-0007）。两条轴混成一个「质量」滑块，人就没法在「我要简介」与「我不想赌账号」
//! 之间分别表态。
//!
//! ## 底下那本账
//!
//! 请求数与耗时由[核心库](romcat_core::scrape::estimate)算，这一层只把它画进预估框（设计稿 `.est`，ADR-0005）。
//! **这块弹层存在的全部理由就是那个数**：一个「预计 0 个请求」而按下去发了九千个的界面，比不给估算更坏。
//! 所以收媒体的联网那一档数后加「+」（那是下界），撞上这一趟自设上限的写真会发的那个数，右边那句把这两件事说出来。
//!
//! **勾上联网源即生效，警示常驻在预估框里**（拿主意的人 2026-10-02 裁 `F-6`，翻掉票 `gui-redesign/10` 当年那一层
//! 「我知道，勾上」）：警示从「点一下就消失的一层」换成「勾着就一直在、紧挨着请求数、就在按『开始刮削』之前」。
//! ADR-0007 要的限流与 431 硬停在核心里，不靠这一层。
//!
//! 底部提示框里那句话：**不会覆盖你的裁决和手动修改的元数据。** 它不只是一句安慰——刮削结果按「锚点 × 字段 × 源」
//! 三元组**并存**，没有覆盖这回事，而**裁决**排在每条链的第一位（`priorities.toml` 的第一条规则、ADR-0001）；
//! 重采清采集记录时也**一条裁决都不删**（`Catalog::clear_scraped`）。
//!
//! ## 按不动的时候
//!
//! 上一趟还在任务台上、范围是空的、那本账算不出来——这三样「开始刮削」画灰，**理由常驻在页脚上**（「取消」后头），唯一的入口
//! [`Panel::start`] 拒下时说的是同一句（ADR-0005「不禁按钮」那条的再修订，拿主意的人 2026-10-02 裁 `F-13`）。
//! 「缺一样东西」的拒绝（没有 ScreenScraper 账号）不画灰，按下去说话，指向屏上那一处（ADR-0005 修订段，`F-7`）。
//!
//! ## 排上就关，回执交给任务台
//!
//! 按下「开始刮削」，那一趟排上任务台，这一层**当场关上**（收挂单 `Q664`，与设计稿 `scr-go` 同一个动作）：
//! 一趟刮削动辄跑上几分钟，弹层开着就挡着整屏。跑到哪儿、怎么收的场，去任务屏看——任务台历史本来就记着每一趟的
//! **收场**，部分完成与失败连那半句说明一起（[`Ending::render`](romcat_core::task::Ending::render)）。这一层**不再画回执**：关上的弹层里画着的回执
//! 没人看得见，下次摊开时又已经是上一趟的事了。
//!
//! ## 「有值了但我想换一个」不该按这个按钮
//!
//! 那是**优先级**的事：改一次排序、零成本、不重跑。弹层底部提示框把这句话连那颗「调整优先级…」摆在一起，是因为
//! 不写的话，人唯一想得到的办法就是重采——那要花掉一整天的配额去换一件排序就能做到的事。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use romcat_core::catalog::{Catalog, CatalogError, Roots};
use romcat_core::dat::HttpFetcher;
use romcat_core::fs::RealFs;
use romcat_core::identify::fuzzy;
use romcat_core::report::{human_bytes, rough_duration, thousands};
use romcat_core::scrape::estimate::{self, Estimate};
use romcat_core::scrape::online::{self, Credentials, Limits, Net};
use romcat_core::scrape::{self, Field, Gather, Options, Profile, local};
use romcat_core::site::Site;
use romcat_core::sources::Source;
use romcat_core::task::{Caption, Cutoff, Finished, Handle};
use romcat_core::{verdict, workspace, zh};

use crate::dialog::{Button, Dialog, Footer, Width};
use crate::look;
use crate::priority::Editor;
use crate::table::Picked;
use crate::task::{Product, Tasks};
use crate::tokens::Tokens;

/// 底部提示框头一句（加粗那半句）。**行为上也成立**，不只是写着好看：
/// 裁决排在每条优先级链的第一位，重采清采集记录时一条裁决都不删。
pub const UNTOUCHED: &str = "不会覆盖你的裁决和手动修改的元数据。";

/// 底部提示框后半句：「有值了但我想换一个」该怎么办——旁边就是那颗「调整优先级…」。
pub const NOT_BY_RESCRAPE: &str = "如果只是想换一个显示的值，调整数据源优先级即可，无需重新刮削。";

/// 上一趟刮削还没跑完时又摊开这一层：「开始刮削」为什么按不动。页脚上常驻、[`Panel::start`] 拒下的都是它。
pub const STILL_RUNNING: &str =
    "上一趟刮削还在任务台上，跑完之前这儿排不了下一趟——进度去任务屏看。";

/// 那本账算不出来：「开始刮削」为什么按不动。页脚上常驻、[`Panel::start`] 拒下的都是它；
/// 为什么算不出来（读库那句错）画在预估框那个位置。
pub const CANNOT_COUNT: &str = "这本账算不出来——不知道要发多少请求，就不排这一趟。";

/// 范围是空的：「开始刮削」为什么按不动。浏览屏一行都没勾时根本不摊开这一层，这一句是给绕过界面直接调的那一路。
pub const EMPTY_SCOPE: &str = "这一批一个变体都没有。回筛选器筛一批，或者在列表里勾几行。";

/// 勾联网源那一档的长警告。**照 ADR-0007 的口径说**：挂在数据源那一栏 ScreenScraper 那一行的悬停上，
/// 设置屏那一段配额警示摆的也是这一句。
pub const QUOTA_WARNING: &str = "\
    联网源赌的是你的账号与 IP：ScreenScraper 的配额同时按账号与 IP 计，\
    撞穿了是永久封禁，而汉化版在它眼里正是「未识别 ROM」。\
    这一档默认限流、只对已确认的条目发请求、配额超限当场停下——不重试、不换账号。";

/// 预估框右边那句（勾着联网源、真会发请求时），照稿。长的那句是 [`QUOTA_WARNING`]。
pub const QUOTA_NOTE: &str = "将消耗 ScreenScraper 配额。配额按账号和 IP 计算，超出可能导致封禁。";

/// 预估框右边那句（只用本地源时），照稿。
pub const LOCAL_NOTE: &str = "仅使用本地数据源，不会产生网络请求。";

/// 刮削那一趟副标题里说用了哪几样源（只用本地源时），照稿 `TASKS.scrape` / `TASKS.scrapeOne` 的 `sub`。
const LOCAL_SOURCES: &str = "仅使用本地数据源";

/// 同上，勾着联网源时。**稿没画联网那一趟**（稿上那句写死了本地），照本地那句的说法补上 ScreenScraper。
const ONLINE_SOURCES: &str = "使用本地数据源与 ScreenScraper";

/// 刮削那一趟副标题里那半句「一个请求都不发」，照稿 `TASKS.scrape` 的 `sub`。
const NO_REQUESTS: &str = "不产生网络请求";

/// 只用本地源那一趟在任务台上名字底下那一行副标题：「仅使用本地数据源 · 不产生网络请求」（设计稿 `TASKS.scrape` /
/// `TASKS.scrapeSel` 的 `sub` 原话）。库屏工序段刮削那一行排的那一趟也是这一句：那一套只用本地源（[`whole_library`]）。
pub(crate) fn local_subtitle() -> String {
    format!("{LOCAL_SOURCES} · {NO_REQUESTS}")
}

/// 预估框右边那句（勾了联网，却一个可查的条目都没有时）：不警示（拿主意的人 2026-10-02 裁 `F-12`）——
/// 汉化版在 ScreenScraper 眼里是「未识别 ROM」，整批都是没确认的条目时这一态很常见，警示一个零请求的趟是狼来了。
pub const NOTHING_TO_ASK: &str =
    "这一批没有可查的条目（只对已确认的条目发请求），不会产生网络请求。";

/// 预估框右边那句（勾了联网、可查的条目也有，可补缺那一档一个都不会再问时）：上一趟都查过、输入没变。同样不警示。
pub const ALL_ASKED: &str = "这一批可查的条目上一趟都查过、输入没变，补缺不会产生网络请求。";

/// 预估框右边补的那句（收媒体的联网那一档）：请求数是下界（拿主意的人 2026-10-02 裁 `F-8`）。
pub const MEDIA_DOWNLOADS: &str = "另加下载图片的请求，有几份要查过才知道。";

/// 预估框右边补的那句（收媒体时）：「读取硬盘」那一格为什么写「—」（拿主意的人 2026-10-02 裁 `F-11` 的退路）。
pub const DISK_UNKNOWN: &str = "首趟要把图从主库读一遍，读多少要读过才知道。";

/// ScreenScraper 那一行的小字（没有账号时）：照样勾得上，按之前就说清（拿主意的人 2026-10-02 裁 `F-7`）。
pub const NO_ACCOUNT: &str = "没有账号——在设置屏填好之前，开始时会拒";

/// 勾着联网源、却没有账号时按「开始刮削」拒下的那一句：指向设置屏（`F-7`）；环境变量那条路在它的悬停上。
pub const NEEDS_ACCOUNT: &str =
    "联网源要一套 ScreenScraper 账号：先在设置屏「数据源」那一节填好，再开始刮削。";

/// 「开始刮削」的悬停字。排上就关之后的实话（票 `13`）；按不动的理由不在这儿，在页脚上常驻。
const START_HOVER: &str = "排到任务台上跑：按下就收起这一层，进度与收场去任务屏看；\
     浏览、筛选、看详情照常。";

/// 「字段」那个小标题的悬停：为什么没有「标题」这一格（拿主意的人 2026-10-02 裁 `F-4`：从常驻挪进悬停）。
const TITLES_ALWAYS: &str = "标题与汉化组不在这张单子上——它们永远采，而且一分代价都不多花。\
     中立库里标题永远是集合（标题集合），中文名与别名就落在那里；那几个源本来就是一次撞完一起带回来的。";

/// 本地那三行共用的那半句悬停：为什么关不掉。
const LOCAL_LOCKED: &str =
    "本地源一个网络请求都不发，也关不掉：它们不花任何配额，关掉只会让联网那一侧多背几个字段。";

/// **字段那个旋钮上摆得出来的那几个。**
///
/// 标题与汉化组**不在这张单子上，因此永远采**：中立库里标题永远是集合
/// （`CONTEXT.md` 的**标题集合**词条），而这个库最要紧的产出——中文名与别名——就落在
/// 那里。把它做成一个可关的开关，等于在界面上摆一个「把中文名关掉」的按钮，
/// 而它一分钱代价都省不下来（那几个源本来就是一次撞完，多带一栏不多花任何东西）。
pub const KNOBS: [Field; 5] = [
    Field::Description,
    Field::Genre,
    Field::Developer,
    Field::Publisher,
    Field::Year,
];

/// 字段那一栏**照稿摆的几行**：每一行拨的是哪几个字段。
///
/// 「开发商 / 发行商」并成一格，拨它同时拨两个字段（拿主意的人 2026-10-02 裁 `F-3`）：两样同源同代价，分开勾一分钱都
/// 省不下来——与 [`KNOBS`] 不让标题可关是同一个理由。**只在界面层折**，核心照旧是两个字段，[`KNOBS`] 照旧五个。
const FIELD_ROWS: [&[Field]; 4] = [
    &[Field::Description],
    &[Field::Genre],
    &[Field::Developer, Field::Publisher],
    &[Field::Year],
];

/// 算这本账时的**限流参数**。与命令行的默认那一份是同一个来处
/// （`online::DEFAULT_INTERVAL` / `DEFAULT_BUDGET`），于是屏上估的耗时就是真跑的节奏。
fn limits() -> Limits {
    Limits::default()
}

/// 摊开这一层时，**范围从哪一路来**：标头那句说明照它分三种说（拿主意的人 2026-10-02 裁 `F-1`）。
///
/// 屏上说的范围与按下去动的那一批是同一批：全选时那一批就是当前筛选结果，只勾了几行时是那几行，
/// 从「刮削此作品」进来时是那一个作品——各说各的那一种，要改范围去哪儿改也跟着说。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    /// 屏头「刮削…」、**全选着**：范围是当前筛选结果。`works` 是浏览屏那一句「已选 N 个作品」的 N。
    Filtered {
        /// 几个作品。
        works: u64,
    },
    /// 屏头「刮削…」、**只勾了几行**：范围是勾中的那几个作品。
    Picked {
        /// 几个作品。
        works: u64,
    },
    /// 「刮削此作品」（作品详情页那颗、右键菜单那一项）：范围是这一个作品底下的变体。
    Work {
        /// 这个作品屏上叫什么（详情页顶上、菜单顶上写的那一个）。
        name: String,
    },
}

impl Reach {
    /// 屏头「刮削…」那一路：浏览屏那份勾选勾中了几个作品，`total` 是当前筛选下一共几个。
    ///
    /// **一行都没点掉的全选**才是「当前筛选结果」：全选之后又点掉几行，勾中的就只是剩下那几行——照「勾选」那一种说，
    /// 要改也是改勾选（拿主意的人裁 `F-1` 时否掉的正是「部分勾选说成当前筛选结果」）。
    #[must_use]
    pub fn of_pick(picked: &Picked, total: u64) -> Self {
        let works = picked.count(total);
        if picked.is_all() && works == total {
            Self::Filtered { works }
        } else {
            Self::Picked { works }
        }
    }

    /// 标头那句说明，`variants` 是这一批有几个变体。
    fn note(&self, variants: u64) -> String {
        match self {
            Self::Filtered { works } => format!(
                "应用于当前筛选结果：{} 个作品（{} 个变体）。如需调整范围，请修改筛选条件。",
                thousands(*works),
                thousands(variants),
            ),
            Self::Picked { works } => format!(
                "应用于勾选的 {} 个作品（{} 个变体）。如需调整范围，请修改勾选。",
                thousands(*works),
                thousands(variants),
            ),
            Self::Work { name } => format!("应用于「{name}」：{} 个变体。", thousands(variants)),
        }
    }
}

/// 一组旋钮的位置。**账是照哪一组算的，记下来**——不然每帧都要重算一遍，
/// 而算一遍要问好几次中立库。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Knobs {
    fields: BTreeSet<Field>,
    media: bool,
    online: bool,
    sweep: Gather,
    scope: usize,
}

/// 摊开这一层那一下**问过一次**的几样：两个本地源取回来没有、有没有 ScreenScraper 账号。
///
/// 每帧问一遍要开几份库、读一份文件；摊开一次问一次，人在别屏改过（下载了、填了账号）下回摊开就对上了。
#[derive(Debug, Clone, Default)]
struct Readiness {
    /// 中文离线源没取回时屏上那个词（核心库给的，`SourceState::label`）；取回了是 `None`。
    chinese: Option<&'static str>,
    /// DAT 没取回时屏上那个词；取回了是 `None`。
    dat: Option<&'static str>,
    /// 有没有 ScreenScraper 账号（核心库那一处答：环境变量与工作目录里那份文件哪一条都算）。
    account: bool,
}

impl Readiness {
    fn now(workspace: &Path) -> Self {
        Self {
            chinese: Source::Chinese.survey(workspace).state.label(),
            dat: Source::Dat.survey(workspace).state.label(),
            account: online::has_account(workspace),
        }
    }
}

/// 刮削弹层。
pub struct Panel {
    /// 工作目录：媒体池、优先级表、中文离线索引、沉淀库都在里头。
    workspace: PathBuf,
    /// 弹层摊开着没有。
    open: bool,
    /// **范围**：这一批变体的键。弹层一摊开就定死——旋钮改的是「要什么」与
    /// 「花多少代价」，改范围要回筛选器或勾选改。
    scope: Vec<String>,
    /// 摊开这一层的时候，屏上写着的是多少个变体。
    ///
    /// 与 `scope.len()` 是同一个数，留一格是为了**说得出它是从哪儿来的**：
    /// 屏上写几个、这一层列几个、按下去动几个，三处同一个数（票 `gui-redesign/03`）。
    scope_shown: u64,
    /// 范围从哪一路来：标头那句照它说。
    reach: Reach,
    /// 摊开那一下问过一次的几样。
    readiness: Readiness,
    /// **字段**旋钮。
    fields: BTreeSet<Field>,
    /// 收不收**媒体**。默认不收：真库那块盘 10 TB，收媒体要回盘把图读一遍。
    media: bool,
    /// 勾了**联网源**没有。**默认不勾**（ADR-0007）。
    online: bool,
    /// **采法**。
    sweep: Gather,
    /// 上一次算出来的账。`None` 是还没算，或者算不出来（为什么在 [`Self::count_error`] 里）。
    estimate: Option<Estimate>,
    /// 那本账为什么算不出来（读库那句错），画在预估框那个位置。
    count_error: Option<String>,
    /// 那本账是照哪一组旋钮算的。
    counted: Option<Knobs>,
    /// 正在跑的那一趟的任务号。
    running: Option<u64>,
    /// 叠在上头的那一层（优先级）刚说的那句话：保存成了。刮削那一趟的回执不在这儿，在任务台上。
    notice: Option<String>,
    /// 上一次按「开始刮削」被拒下的那句话。
    error: Option<String>,
    /// **数据源优先级**那一层弹层：从底部提示框那颗「调整优先级…」打开，叠在这一层上头
    /// （[`crate::priority`]）。
    priority: Editor,
}

impl Panel {
    /// 开一块。
    #[must_use]
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            priority: Editor::new(workspace.clone()),
            workspace,
            open: false,
            scope: Vec::new(),
            scope_shown: 0,
            reach: Reach::Picked { works: 0 },
            readiness: Readiness::default(),
            // **默认全勾**：字段那一栏答的是「我要什么」，而这几样本来就是一次撞完
            // 一起带回来的，少勾一个省不下任何代价。
            fields: KNOBS.into_iter().collect(),
            media: false,
            online: false,
            sweep: Gather::default(),
            estimate: None,
            count_error: None,
            counted: None,
            running: None,
            notice: None,
            error: None,
        }
    }

    /// 弹层摊开着没有。
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// **摊开弹层**，范围就是交进来的这一批变体的键；`reach` 说这一批是从哪一路来的（标头那句照它说）。
    ///
    /// `shown` 是屏上那句「多少个变体」写着的数——两者对不上就是这一层出了问题，
    /// 而那正是「按下去动的比屏上写的多」那种事故。界面上按「刮削…」「刮削此作品」走的都是它。
    ///
    /// 两个本地源取回来没有、有没有 ScreenScraper 账号，**在这一下问一次**（不每帧问）。
    pub fn open(&mut self, keys: Vec<String>, shown: u64, reach: Reach) {
        self.scope_shown = shown;
        self.scope = keys;
        self.reach = reach;
        self.readiness = Readiness::now(&self.workspace);
        self.open = true;
        self.counted = None;
        self.error = None;
        self.notice = None;
    }

    /// 收起弹层。旋钮留在原位——人多半是回去改筛选，改完还想按同一套设置。
    pub fn close(&mut self) {
        self.open = false;
        self.priority.close();
    }

    /// 数据源优先级那一层。
    #[must_use]
    pub fn priority(&self) -> &Editor {
        &self.priority
    }

    /// 同上，可改。测试拿它当屏上那几下。
    pub fn priority_mut(&mut self) -> &mut Editor {
        &mut self.priority
    }

    /// 这一批有多少个变体。
    #[must_use]
    pub fn scope_total(&self) -> u64 {
        u64::try_from(self.scope.len()).unwrap_or(u64::MAX)
    }

    /// 摊开弹层时屏上写着的那个数。**与 [`Self::scope_total`] 必须一样。**
    #[must_use]
    pub fn scope_shown(&self) -> u64 {
        self.scope_shown
    }

    /// 范围从哪一路来。
    #[must_use]
    pub fn reach(&self) -> &Reach {
        &self.reach
    }

    /// 这一趟要哪几个字段。
    #[must_use]
    pub fn fields(&self) -> &BTreeSet<Field> {
        &self.fields
    }

    /// 拨一下某个字段。测试与实测拿它当界面上那一下。
    pub fn toggle_field(&mut self, field: Field) {
        if !self.fields.remove(&field) {
            self.fields.insert(field);
        }
    }

    /// 字段那一栏的一行拨成 `on`：那一行管的几个字段一起拨（「开发商 / 发行商」那一格是两个）。
    fn set_fields(&mut self, row: &[Field], on: bool) {
        for field in row {
            if on {
                self.fields.insert(*field);
            } else {
                self.fields.remove(field);
            }
        }
    }

    /// 收不收媒体。
    #[must_use]
    pub fn media(&self) -> bool {
        self.media
    }

    /// 拨一下媒体那个勾。
    pub fn set_media(&mut self, on: bool) {
        self.media = on;
    }

    /// 勾上联网源了没有。**默认没有。**
    #[must_use]
    pub fn online(&self) -> bool {
        self.online
    }

    /// 拨一下联网源那个勾。**勾上即生效**（拿主意的人 2026-10-02 裁 `F-6`）：警示不再是勾之前先摆的一层，
    /// 而是勾着就一直在预估框里——紧挨着请求数，就在按「开始刮削」之前。
    pub fn toggle_online(&mut self) {
        self.online = !self.online;
    }

    /// 采法。
    #[must_use]
    pub fn sweep(&self) -> Gather {
        self.sweep
    }

    /// 换一种采法。
    pub fn set_sweep(&mut self, sweep: Gather) {
        self.sweep = sweep;
    }

    /// 上一次算出来的那本账。
    #[must_use]
    pub fn estimate(&self) -> Option<&Estimate> {
        self.estimate.as_ref()
    }

    /// 上一次按「开始刮削」被拒下的那句话。
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// 叠在上头的那一层刚说的那句话（优先级保存成了）。刮削那一趟的回执在任务台上，不在这儿。
    #[must_use]
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// 正在跑的那一趟的任务号。
    #[must_use]
    pub fn running(&self) -> Option<u64> {
        self.running
    }

    /// 眼下排不了下一趟的理由：上一趟还在任务台上（挂单 `Q1538`）。没有就是 `None`。
    #[must_use]
    pub fn busy(&self) -> Option<&'static str> {
        self.running.is_some().then_some(STILL_RUNNING)
    }

    /// **「开始刮削」为什么按不动**：上一趟还在任务台上、范围是空的、那本账算不出来，三样有一样就交那一句；
    /// 按得动是 `None`。
    ///
    /// **守卫拒下时说的、屏上常驻的是这同一句**（ADR-0005「不禁按钮」那条的再修订：画灰的同时，唯一的入口
    /// [`Self::start`] 拒下要带着理由，屏上常驻着那条理由——页脚上「取消」后头，`Dialog::footer_note`——理由只有一处）。
    /// 看的是眼下这本账：没算过就是算不出来，[`Self::start`] 起手先算一遍。
    #[must_use]
    pub fn blocked(&self) -> Option<&'static str> {
        if let Some(why) = self.busy() {
            Some(why)
        } else if self.scope.is_empty() {
            Some(EMPTY_SCOPE)
        } else if self.estimate.is_none() {
            Some(CANNOT_COUNT)
        } else {
            None
        }
    }

    /// 这一趟的选项。**估算与真跑收的是同一份**——两边各摆一份的话，
    /// 「屏上说 0 个请求」与「按下去发了几个」就没有共同的前提了。
    ///
    /// # Errors
    /// 中立库读不出那一组根时返回错误。
    pub fn options(&self, catalog: &Catalog) -> Result<Options, CatalogError> {
        let mut options = self.knob_options(catalog)?;
        options.only = Some(estimate::only(self.scope.iter().cloned()));
        Ok(options)
    }

    /// 字段、源、媒体、采法那几个旋钮折成的选项，**范围不收窄**（整库）。
    ///
    /// 与 [`Self::options`] 分开，是为了让工序段那一趟从**同一个构造**里折出它那一套
    /// （[`whole_library`]），而不是另写一份默认值。
    fn knob_options(&self, catalog: &Catalog) -> Result<Options, CatalogError> {
        let mut options = Options::new(
            Roots::load(catalog)?,
            workspace::media_pool_dir(&self.workspace),
        );
        options.profile = if self.online {
            Profile::Online
        } else {
            Profile::Offline
        };
        options.media = self.media;
        // **标题与汉化组永远采**（见 [`KNOBS`]）：它们不在旋钮上，也就没有关掉的办法。
        options.fields = self
            .fields
            .iter()
            .copied()
            .chain([Field::Title, Field::TranslationGroup])
            .collect();
        self.sweep.apply(&mut options);
        Ok(options)
    }

    /// 旋钮动过就重算一遍那本账。**每帧调一次，没动过是空操作。**
    ///
    /// 不每帧无脑重算，是因为算一遍要问好几次中立库——按作品取判据那一步在真库上是
    /// 几千次查询（作品数见台账 `docs/library-facts.md`）。放在画帧那条线程上每帧跑一遍就是挂账 D156 那件事的翻版。
    pub fn recount(&mut self, catalog: &Catalog) {
        let knobs = Knobs {
            fields: self.fields.clone(),
            media: self.media,
            online: self.online,
            sweep: self.sweep,
            scope: self.scope.len(),
        };
        if self.counted.as_ref() == Some(&knobs) {
            return;
        }
        self.counted = Some(knobs);
        match self
            .options(catalog)
            .map_err(|error| format!("中立库读不动：{error}"))
            .and_then(|options| {
                estimate::estimate(catalog, &options, limits())
                    .map_err(|error| format!("中立库读不动：{error}"))
            }) {
            Ok(estimate) => {
                self.estimate = Some(estimate);
                self.count_error = None;
            }
            Err(why) => {
                // **算不出来就说算不出来**，不摆一个 0 出去：屏上写着「0 个网络请求」
                // 而按下去发了几千个，那正是这块弹层要防的事。
                self.estimate = None;
                self.count_error = Some(why);
            }
        }
    }

    /// **把这一趟排上任务台。**
    ///
    /// 后台那条线程按文件路径自己开一份现场（`rusqlite::Connection` 不是 `Sync`，
    /// 界面这条线程手里那一份交不过去），与扫描那一趟同一个套路。只活在内存里的库
    /// 分不出第二份，那时**就地跑完**——合成数据上这是几毫秒的事，与子库屏排差量预览
    /// 走的是同一条退路。
    ///
    /// **起手先拒按不动的那三样**（[`Self::blocked`]，带着屏上常驻的那同一句）：从前这里只拦「台上有一趟」与
    /// 「范围空」，绕过界面直接调就能在零估算下开跑（票 `gui-draws-the-rest-of-the-design/17` 的差距 `S-20`）。
    /// 那本账在这儿现算一遍——没画过一帧就直接调的那一路，也照屏上那同一本账判。
    ///
    /// **排上就关上这一层**（收挂单 `Q664`）：跑到哪儿、怎么收的场去任务屏看。排不上（按不动、中立库读不动、
    /// 联网源拿不到凭据）时这一层留着，那句话画在里头——人得在这儿改。
    ///
    /// 界面上按「开始刮削」走的就是它，测试与实测拿它当那一下。
    pub fn start(&mut self, site: &mut Site, tasks: &mut Tasks) {
        self.recount(&site.catalog);
        if let Some(why) = self.blocked() {
            self.error = Some(why.to_string());
            return;
        }
        let options = match self.options(&site.catalog) {
            Ok(options) => options,
            Err(error) => {
                self.error = Some(format!("中立库读不动：{error}"));
                return;
            }
        };
        // **在线档要一套凭据，拿不到就别启动。** 悄悄退回离线跑完，只会让人对着一份
        // 缺封面的报告以为「在线源也没有」（`ScrapeError::NoNetwork` 的道理）。
        // 账号只问核心库那一处（`online::find_account`）：环境变量优先，其次设置屏存进工作目录的那一套。
        // **这是「缺一样东西」的拒绝**（ADR-0005 修订段）：按钮不画灰，按下去说话，指向屏上那一处（`F-7`）。
        let credentials = if self.online {
            match online::find_account(&self.workspace) {
                Ok(Some((account, _))) => Some(account.credentials()),
                Ok(None) => {
                    self.error = Some(NEEDS_ACCOUNT.to_string());
                    return;
                }
                Err(why) => {
                    self.error = Some(format!("联网源起不来：{why}"));
                    return;
                }
            }
        } else {
            None
        };
        let workspace = self.workspace.clone();
        let caption = self.caption();
        self.error = None;
        self.notice = None;
        self.running = Some(match site.catalog.file().map(Path::to_path_buf) {
            Some(file) => tasks.queue(caption, move |task| {
                let mut site = Site::open_file(&workspace, &file, None)
                    .map_err(|error| format!("这份库在后台开不出来：{error}"))?;
                run(&mut site, &workspace, &options, credentials, task)
            }),
            // 只活在内存里的库（合成数据走这条）分不出第二份连接：**就地跑完**。
            // 那时窗口确实会僵一下，但那份库小到几毫秒就走完——真库一律走上面那条。
            None => tasks.run_here(caption, |task| {
                run(site, &workspace, &options, credentials, task)
            }),
        });
        self.close();
    }

    /// 这一趟在任务台上叫什么（差距 `S-21`，照设计稿 `TASKS.scrapeSel` 与 `TASKS.scrapeOne`）。
    ///
    /// - **名字说范围与采法**：勾选与筛选那两路是「刮削 · N 个作品（补缺）」——单位是作品，与标头那句、浏览屏「已选
    ///   N 个作品」同一个数；「刮削此作品」那一路是「刮削 · 作品名」。
    /// - **用了哪几样源、发不发请求交给副标题**，不塞进名字的括号里：只用本地源时照稿「仅使用本地数据源 · 不产生网络
    ///   请求」；「刮削此作品」那一路照稿把采法挪到副标题头上（「补缺 · 仅使用本地数据源」）。勾着联网源时说清还用了
    ///   ScreenScraper，请求数取预估框里那一格（[`estimate_face`]：与按下之前屏上写着的是同一个数）。
    /// - **补缺那一趟续得上**（[`Caption::resumable`]）：停下之前采完的那些落进了中立库，下一趟补缺照输入指纹跳过它们。
    ///   **重采那一趟不说**：再按一次重采绕过输入指纹全部重来，停下之前采完的也再采一遍。
    fn caption(&self) -> Caption {
        let sweep = self.sweep.label();
        let caption = match &self.reach {
            Reach::Filtered { works } | Reach::Picked { works } => {
                let subtitle = match (self.online, &self.estimate) {
                    (false, _) => local_subtitle(),
                    (true, Some(account)) if account.requests > 0 => format!(
                        "{ONLINE_SOURCES} · {} 个网络请求",
                        estimate_face(account, true).requests
                    ),
                    (true, _) => format!("{ONLINE_SOURCES} · {NO_REQUESTS}"),
                };
                Caption::new(format!("刮削 · {} 个作品（{sweep}）", thousands(*works)))
                    .with_subtitle(subtitle)
            }
            Reach::Work { name } => {
                let sources = if self.online {
                    ONLINE_SOURCES
                } else {
                    LOCAL_SOURCES
                };
                Caption::new(format!("刮削 · {name}")).with_subtitle(format!("{sweep} · {sources}"))
            }
        };
        match self.sweep {
            Gather::Fill => caption.resumable(),
            Gather::Refresh => caption,
        }
    }

    /// 任务台交回来一趟跑完的活。**不是自己那一趟就放过去**，返回「认领了没有」。
    ///
    /// **认领只销号、作废那本账**，不写回执：怎么收的场任务台历史上记着（完成、已取消、部分完成连留下了什么、
    /// 失败连哪一步为什么，[`Ending::render`](romcat_core::task::Ending::render)），这一层排上那一下就关了（收挂单 `Q664`）。产物里那份报告
    /// 没有别处要——浏览屏重读一遍由主窗口在认领之后做。
    pub fn settle(&mut self, done: &Finished<Product>) -> bool {
        if self.running != Some(done.id) {
            return false;
        }
        self.running = None;
        // 跑完一趟，采集记录变了，那本账跟着作废——重算一遍，屏上那个数才对得上。
        self.counted = None;
        true
    }

    /// 画一帧：**一层弹层**（[`crate::dialog`]），摊开着才画。
    ///
    /// 标题「刮削元数据」，底下那句说明说范围（[`Reach`]）；内容区是三栏旋钮、预估框、底部提示框；页脚是
    /// 「取消（幽灵按钮） ｜ 开始刮削」，按不动时理由常驻在页脚上「取消」后头。Esc 等于按「取消」。
    pub fn show(&mut self, ctx: &egui::Context, site: &mut Site, tasks: &mut Tasks) {
        /// 页脚上按下去的是哪一颗。
        enum Pressed {
            /// 「取消」：收起，旋钮留在原位。
            Close,
            /// 「开始刮削」。
            Start,
        }
        if !self.open {
            return;
        }
        self.recount(&site.catalog);
        let blocked = self.blocked();
        let footer = Footer::new(Button::new("取消", Pressed::Close).ghost()).button(
            Button::new("开始刮削", Pressed::Start)
                .primary()
                .enabled(blocked.is_none())
                .hover(START_HOVER),
        );
        let mut dialog = Dialog::new("刮削", "刮削元数据", footer)
            .note(self.reach.note(self.scope_total()))
            .width(Width::Wide);
        if let Some(why) = blocked {
            dialog = dialog.footer_note(why);
        }
        let shown = dialog.show(ctx, |ui| self.body_ui(ui));
        match shown.pressed {
            None => {}
            Some(Pressed::Close) => self.close(),
            Some(Pressed::Start) => self.start(site, tasks),
        }
        // **优先级那一层叠在这一层上头**：Esc 一下只退它（`crate::dialog`）。保存成了就在
        // 这一层上留一句回话——那一层关上之后人回到的是这儿。
        let editing = self.priority.is_open();
        self.priority.show(ctx, &site.catalog);
        if editing && !self.priority.is_open() && self.priority.has_saved() {
            self.notice = Some(crate::priority::SAVED.to_string());
        }
    }

    /// 内容区：三栏、预估框、底部提示框，块与块之间隔 `dialog-body-gap`、不画分隔线（设计稿 `.mbody`）；
    /// 再往下是叠在上头那一层的回话与按下去被拒的那句。
    fn body_ui(&mut self, ui: &mut egui::Ui) {
        let tokens = Tokens::builtin();
        let 块距 = tokens.space.dialog_body_gap;
        // 块与块之间的距离全由 `dialog-body-gap` 给，一栏里一行与一行之间由 `.opt` 自己的上下留白给（设计稿就是这么叠的）。
        ui.spacing_mut().item_spacing.y = 0.0;
        // **三栏等宽**（`Ui::columns`），栏距 `scrape-columns-gap`。不定宽的话，头一栏里那句长话照整块内容区的宽度排，
        // 后两栏只剩一条缝（票 `gui-looks-like-the-design/30` 撞上的）。
        let 原来的横距 = ui.spacing().item_spacing.x;
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = tokens.space.scrape_columns_gap;
            ui.columns(3, |columns| {
                for column in columns.iter_mut() {
                    column.spacing_mut().item_spacing.x = 原来的横距;
                }
                self.fields_ui(&mut columns[0]);
                self.sources_ui(&mut columns[1]);
                self.sweep_ui(&mut columns[2]);
            });
        });
        ui.add_space(块距);
        self.estimate_ui(ui);
        ui.add_space(块距);
        self.note_ui(ui);
        if let Some(notice) = &self.notice {
            ui.add_space(块距);
            look::help(ui, notice);
        }
        // 按下去被拒的那句。**按不动那三样拒下的就是页脚上常驻的那一句**，那一处已经画了就不再画一遍。
        if let Some(error) = &self.error
            && self.blocked() != Some(error.as_str())
        {
            ui.add_space(块距);
            let said = ui.colored_label(ui.visuals().error_fg_color, error);
            if error == NEEDS_ACCOUNT {
                said.on_hover_text(env_route());
            }
        }
    }

    /// **字段** · 我要什么。
    fn fields_ui(&mut self, ui: &mut egui::Ui) {
        column_heading(ui, "字段").on_hover_text(TITLES_ALWAYS);
        for row in FIELD_ROWS {
            let label = row
                .iter()
                .map(|field| field.label())
                .collect::<Vec<_>>()
                .join(" / ");
            let mut on = row.iter().all(|field| self.fields.contains(field));
            if look::checkbox_line(ui, &mut on, &label).changed() {
                self.set_fields(row, on);
            }
        }
        let mut media = self.media;
        if look::checkbox_option(ui, &mut media, "媒体", "封面、截图、视频")
            .on_hover_text(
                "本地那一半要回主库把图读一遍（真库上 539 份、71 秒，第二趟哈希从中立库\
                 取回）；联网那一半每份图各花一个请求。",
            )
            .changed()
        {
            self.media = media;
        }
    }

    /// **数据源** · 花多少代价。
    ///
    /// 照稿逐行（拿主意的人 2026-10-02 裁 `F-5`）：中文离线源、DAT、本地媒体三行本地源，**画灰按不动**（核心不加
    /// 逐源开关——本地源免费，关掉只会让联网源多背几个字段、多花配额）；ScreenScraper 一行，默认不勾。源名与优先级表、
    /// 报告是同一个名字（核心库那几个常量）。每行底下那句小字照稿。
    ///
    /// **勾着就是这一趟真的在用**：没取回的源不勾、小字换核心库给的状态词（拿主意的人 2026-10-04 裁）；本地媒体那一行
    /// **跟着「媒体」那一格走**——核心里本地媒体源只在收媒体时参加。
    fn sources_ui(&mut self, ui: &mut egui::Ui) {
        column_heading(ui, "数据源").on_hover_text(format!(
            "另有「{}」「{}」两个本地源照跑，不另列：文件名保证每个变体都有一个能显示的标题，\
             别名只进标题集合、只管搜得到。",
            local::FILENAME,
            fuzzy::ALIAS_SOURCE,
        ));
        let 本地 = "本地 · 不消耗配额";
        locked_row(
            ui,
            self.readiness.chinese.is_none(),
            fuzzy::SOURCE,
            self.readiness.chinese.unwrap_or(本地),
            &format!("中文名、简介、类型从这一份来。{LOCAL_LOCKED}"),
        );
        locked_row(
            ui,
            self.readiness.dat.is_none(),
            "DAT",
            self.readiness.dat.unwrap_or(本地),
            &format!(
                "这一行是 {} 这几个源，优先级表与报告里按这几个名字各列一行。{LOCAL_LOCKED}",
                scrape::DAT_SOURCES.join("、"),
            ),
        );
        locked_row(
            ui,
            self.media,
            local::LOCAL_MEDIA,
            if self.media {
                "本地 · 同名图片或独立目录中的图片"
            } else {
                "勾上「媒体」才参加"
            },
            &format!(
                "库里现成的图：同名兄弟、独占目录两条归属规则认得下的那些。不收媒体时它整个不参加，\
                 上一轮收好的一张都不动。{LOCAL_LOCKED}"
            ),
        );
        let mut online = self.online;
        let (小字, 悬停) = if self.readiness.account {
            (
                "联网 · 消耗配额（按账号和 IP 计算）",
                QUOTA_WARNING.to_string(),
            )
        } else {
            (NO_ACCOUNT, format!("{QUOTA_WARNING}\n\n{}", env_route()))
        };
        if look::checkbox_option(ui, &mut online, online::SCREEN_SCRAPER, 小字)
            .on_hover_text(悬停)
            .changed()
        {
            self.toggle_online();
        }
    }

    /// **采法** · 跑多久。
    ///
    /// 两颗单选照稿各是两行（拿主意的人 2026-10-01 裁）：名字，底下一行小字。小字取核心库 [`Gather::why`]
    /// 那一句，不抄稿上那两句——那一句与命令行 `--refresh` 说的是同一件事。小标题叫「采法」不叫稿上的「方式」
    /// （拿主意的人 2026-10-02 裁 `F-2`：词表的正名，与 `Gather::label` 那两档同出一处）。
    fn sweep_ui(&mut self, ui: &mut egui::Ui) {
        column_heading(ui, "采法");
        for sweep in Gather::all() {
            if look::radio_option(ui, self.sweep == sweep, sweep.label(), sweep.why()).clicked() {
                self.set_sweep(sweep);
            }
        }
    }

    /// **预估框**（设计稿 `.est`）：网络请求 / 预计耗时 / 读取硬盘三格，右边一句；勾着联网源、真会发请求时换警示样。
    /// 算不出来时那个位置写为什么。
    fn estimate_ui(&self, ui: &mut egui::Ui) {
        match &self.estimate {
            None => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    self.count_error.as_deref().unwrap_or(CANNOT_COUNT),
                );
            }
            Some(account) => {
                let face = estimate_face(account, self.online);
                look::estimate_box(
                    ui,
                    &[
                        (face.requests.as_str(), "网络请求"),
                        (face.elapsed.as_str(), "预计耗时"),
                        (face.disk.as_str(), "读取硬盘"),
                    ],
                    &face.note,
                    face.warn,
                );
            }
        }
    }

    /// **底部提示框**（设计稿 `.note.row`）：左边那两句（头一句加粗），右边那颗「调整优先级…」（[`look::note_row`]）。
    fn note_ui(&mut self, ui: &mut egui::Ui) {
        let (字, 按钮) = look::note_row(ui, UNTOUCHED, NOT_BY_RESCRAPE, crate::priority::OPEN);
        字.on_hover_text(
            "每个源采到的值各记一条、并存，没有覆盖这回事；裁决排在每条优先级链的第一位，\
             重采清采集记录时也一条裁决都不删。真正需要重采的只有两种：数据源更新了，或者解析逻辑改了。",
        );
        // **那句话说的那件事，就在旁边这颗按钮后头**（[`crate::priority`]）：不摆的话，
        // 人读完「调整数据源优先级即可」还是不知道去哪儿调。
        if 按钮
            .on_hover_text("按字段排数据源的先后：保存后立即生效，不排任何刮削任务。")
            .clicked()
        {
            self.priority.open();
        }
    }
}

/// 一栏的**小标题**（[`look::section`]），底下隔 `section-gap`（设计稿 `.sec` 的 `margin-bottom:6px`）。交回小标题那一段，
/// 悬停挂在它上头。
fn column_heading(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let response = look::section(ui, text);
    ui.add_space(Tokens::builtin().space.section_gap);
    response
}

/// 本地那三行里的一行：**勾着（或者照参不参加画）、画灰按不动**，整格调淡到 `muted-option-opacity`（与合并向导第三步
/// 那一格不可选的同一个调淡）。悬停照按不动的规矩挂（`on_disabled_hover_text`）。
fn locked_row(ui: &mut egui::Ui, on: bool, name: &str, note: &str, hover: &str) {
    let mut on = on;
    ui.scope(|ui| {
        ui.visuals_mut().disabled_alpha = Tokens::builtin().mix.muted_option_opacity;
        ui.add_enabled_ui(false, |ui| look::checkbox_option(ui, &mut on, name, note))
            .inner
            .on_disabled_hover_text(hover);
    });
}

/// 没有账号时的另一条路：环境变量。ScreenScraper 那一行的悬停、按下去拒下那句的悬停都挂它（`F-7`：「环境变量那句留在悬停」）。
fn env_route() -> String {
    format!(
        "也可以开工具之前给环境变量 {}（环境变量优先）。{}",
        online::ENV_KEYS.join(" / "),
        online::DEVID_NOTE,
    )
}

/// 预估框那一面：三格的字、右边那句、换不换警示样。
struct EstimateFace {
    requests: String,
    elapsed: String,
    disk: String,
    note: String,
    warn: bool,
}

/// 那本账排成预估框那一面。
///
/// - **网络请求**：收媒体的联网那一档是下界，数后加「+」（`F-8`）；撞上自设上限时核心交的就是这一趟真会发的数（`F-9`）。
/// - **预计耗时**：核心粗估，不到一秒写「不到 1 秒」（`F-10`，核心 [`rough_duration`]）。
/// - **读取硬盘**：不收媒体是 0；收媒体写「—」——读多少要读过才知道（`F-11` 的退路，挂单 `Q1757`：「这一批收媒体首趟要读多少」
///   那一问在真库个头上超过一两百毫秒，不在摊开弹层那一刻现算）。
/// - **右边那句**：只用本地源说不会产生网络请求；勾着联网、真会发请求时说配额（换警示样），撞上上限、请求数是下界、
///   收媒体各补一句；勾着联网却一个请求都不发时不警示，说清为什么（`F-12`）。
fn estimate_face(account: &Estimate, online: bool) -> EstimateFace {
    let requests = if account.media_downloads && account.requests > 0 {
        format!("{}+", thousands(account.requests))
    } else {
        thousands(account.requests)
    };
    let mut note: Vec<String> = Vec::new();
    let warn = online && account.requests > 0;
    if !online {
        note.push(LOCAL_NOTE.to_string());
    } else if account.requests == 0 {
        note.push(
            if account.queryable == 0 {
                NOTHING_TO_ASK
            } else {
                ALL_ASKED
            }
            .to_string(),
        );
    } else {
        note.push(QUOTA_NOTE.to_string());
        if let Some(wanted) = account.over_budget {
            note.push(format!(
                "这一批想问 {} 条，这一趟到 {} 就停，剩下的下一趟再来。",
                thousands(wanted),
                thousands(account.budget),
            ));
        }
        if account.media_downloads {
            note.push(MEDIA_DOWNLOADS.to_string());
        }
    }
    if account.media_disk_read {
        note.push(DISK_UNKNOWN.to_string());
    }
    EstimateFace {
        requests,
        elapsed: rough_duration(u64::try_from(account.elapsed.as_millis()).unwrap_or(u64::MAX)),
        disk: if account.media_disk_read {
            "—".to_string()
        } else {
            human_bytes(0)
        },
        note: note.concat(),
        warn,
    }
}

/// 库屏工序段**刮削**那一行排的那一趟用的选项：**整库，旋钮是弹层刚摊开时那一套**
/// ——全部字段、只用本地源、不收媒体、补缺（屏上那句话是 [`WHOLE_LIBRARY`]）。
///
/// ## 为什么是这一套，而不是弹层眼下拨到哪儿的那一套
///
/// - **按一下就走的那颗按钮不该花配额。** 弹层上勾联网源时预估框里一直挂着配额那句警示，按下去之前
///   底下那本账得算得出来（ADR-0007）；工序段那一行两样都没有。只用本地源的那一趟
///   一个请求都不发，也就不需要那本账。
/// - **弹层那几个旋钮是给「这一批」拨的**：人在浏览屏上为筛出来的几十个变体勾过联网源、
///   收过媒体，那一套悄悄套到整库上，就是几万个请求、或者把 10 TB 那块盘回读一遍。
/// - **不另写一套默认值**：从 [`Panel::new`] 那个构造里折出来，弹层刚摊开时就是它
///   ——两处不会一处改了另一处没改。
///
/// # Errors
/// 中立库读不出那一组根时返回错误。
pub(crate) fn whole_library(catalog: &Catalog, workspace: &Path) -> Result<Options, CatalogError> {
    Panel::new(workspace.to_path_buf()).knob_options(catalog)
}

/// 工序段刮削那一行那颗按钮按下去排的是哪一趟。**旋钮为什么是这一套**写在同一个模块的
/// `whole_library` 上。
pub const WHOLE_LIBRARY: &str = "刮削那一行排的是整库一趟：全部字段、只用本地源、不收媒体、补缺\
     ——一个网络请求都不发。要挑一批、联网或收媒体，去浏览屏屏头那颗「刮削…」。";

/// 这一趟没走完的话，**留下了什么**——[`Handle::halfway`] 要的就是这一句。
///
/// 走完了就交 `None`：那一趟记成「完成」。**收手的理由不改变这一档是什么**（词表
/// 「部分完成」）：人按的「停下」（`interrupted`）是一种，联网源自己收的手
/// （配额、凭据、网断了）是另一种，两种留下的都是「下一趟接着来」的东西。
///
/// **联网源自己收的手，这一句里说为什么**（[`online::Halt::describe`]）：刮削弹层排上就关、回执交给任务台
/// （收挂单 `Q664`），任务台历史上「部分完成」后头那半句就是这一句——配额超限是硬停、明天再来（ADR-0007），
/// 这件事人只能从这儿读到。
fn left_behind(outcome: &scrape::Outcome) -> Option<String> {
    if !outcome.interrupted && outcome.halted.is_none() {
        return None;
    }
    let 那一下 = if outcome.interrupted {
        "按停时"
    } else {
        "收手时"
    };
    let mut said = format!(
        "{那一下}已经采到的那些落进了中立库（这一趟收进媒体 {} 份），\
         再排一次从那儿接着采——不重做已经采完的部分。",
        thousands(outcome.new_blobs + outcome.deduped),
    );
    if let Some(halt) = &outcome.halted {
        said.push_str(&halted_by(halt));
    }
    Some(said)
}

/// 联网源自己收的手：为什么。任务台上「部分完成」那半句（[`left_behind`]）与库屏那句回执（[`finished`]）说的是这同一句。
fn halted_by(halt: &online::Halt) -> String {
    format!("这一趟是它自己收的手：{}", halt.describe())
}

/// 跑完那一趟排成一句回执。
///
/// **没走完的那一趟不许说「跑完了」**：它交出来的产物长得跟跑完的那一份一模一样，
/// 可它只走了一段（同 [`left_behind`]）。
///
/// **工序段刮削那一行认领时说的就是这一句**（`crate::stages::Section::settle`）。刮削弹层排的那一趟不说它：
/// 那一层排上就关，收场在任务台历史上（收挂单 `Q664`）。
pub(crate) fn finished(outcome: &scrape::Outcome) -> String {
    // **这一句得与那一档对得上。** 走到这儿又没走完的，任务台记的都是「部分完成」
    // （`left_behind` 报了那一句）——起头写「已取消」的话，屏上这一句与任务屏历史
    // 那一行说的是两档收场，而 `Ending::Stopped.render()` 正好就是「已取消」。
    // **为什么收的手**放到后半句去说：词表「部分完成」那一条说得清楚，
    // 收手的理由不改变这一档是什么。
    let 起头 = if outcome.interrupted || outcome.halted.is_some() {
        "刮削部分完成"
    } else {
        "刮削跑完了"
    };
    let mut out = format!(
        "{起头}：{} 项输入没变、整项跳过，收进媒体 {} 份。",
        thousands(outcome.reused_probes),
        thousands(outcome.new_blobs + outcome.deduped),
    );
    if let Some(usage) = outcome.online {
        out.push_str(&format!(
            " 联网发了 {} 个请求，其中 {} 个是「查过、没有」。",
            thousands(usage.requests),
            thousands(usage.not_found),
        ));
    }
    if let Some(halt) = &outcome.halted {
        out.push(' ');
        out.push_str(&halted_by(halt));
    }
    if outcome.interrupted {
        out.push_str(
            " 这一趟是被你按停的：已经采到的那些留在中立库里，\
             再排一次接着采——不重做已经采完的部分。",
        );
    }
    out
}

/// 后台那条线程真跑的那一趟。**装配全在这儿**：中文离线源、匹配裁决、优先级表、
/// 网络句柄。领域判断一条都不在这一层——它只是把核心库要的原料摆齐
/// （与命令行 `romcat scrape` 摆的是同一副）。
///
/// **工序段刮削那一行排的那一趟走的也是它**（`crate::stages`），只是选项换成
/// [整库那一套](whole_library)——装配只有这一份。
pub(crate) fn run(
    site: &mut Site,
    workspace: &Path,
    options: &Options,
    credentials: Option<Credentials>,
    task: &Handle,
) -> Result<Product, Cutoff> {
    task.steps(4);
    // **「被按停了」一个字都不用凑**：`?` 一下把手，`Halted` 自己折成
    // [`Cutoff::Halted`]，任务台按支记成「停了」。这几处从前各自手写一句「按停了」，
    // 而判据是「那句话正是核心库那一句」——差着字，于是按停整趟记成了失败（挂单 `Q151`）。
    task.step("读优先级表")?;
    let priorities = romcat_core::sync::prepare::priorities(None, workspace)?;

    task.step("开中文离线源")?;
    let parts = NamingParts::open(workspace, task)?;
    let naming = parts.naming();
    let summaries: Option<&dyn scrape::zh::Summaries> = parts
        .index
        .as_ref()
        .and(parts.store.as_ref())
        .map(|store| store as &dyn scrape::zh::Summaries);

    // **匹配裁决读不到就停下，不降级成「没人裁过」。** 当成没裁过跑下去，会把人否定掉
    // 的中文名整片撞回来——那正是沉淀库那条「宁可如实拒绝、绝不将就」要拦的事。
    task.step("摊平匹配裁决")?;
    let rulings = verdict::MatchIndex::load(&site.store, &site.library_identity)
        .map_err(|error| format!("沉淀库读不动：{error}"))
        .and_then(|index| {
            scrape::zh::Rulings::resolve(&site.catalog, &index, fuzzy::SOURCE)
                .map_err(|error| format!("匹配裁决摊不平：{error}"))
        })?;

    task.step("采集")?;
    let fetcher = credentials
        .as_ref()
        .map(|_| HttpFetcher::with_throttle(limits().interval));
    let net = match (fetcher.as_ref(), credentials) {
        (Some(fetcher), Some(credentials)) => {
            Some(Net::new(fetcher, limits(), credentials, task.cancel()))
        }
        _ => None,
    };
    let mut progress = |done: scrape::Progress| task.tick(done.done, done.total);
    scrape::run(
        &RealFs::new(),
        &mut site.catalog,
        &priorities,
        options,
        net.as_ref(),
        &mut scrape::RunContext {
            cancel: task.cancel(),
            progress: &mut progress,
            naming: &naming,
            summaries,
            rulings: &rulings,
        },
    )
    .map(|outcome| {
        // **被按停（或者联网源自己收的手）那一趟没走完，可它写过东西**：采到的那些
        // 已经落进中立库了。不报这一句的话它长着「跑完了」的样子进任务台——任务屏历史
        // 写「完成」，而这一屏同时说「按停了」，同一趟活在两处说两套话（挂单 `Q217`）。
        // **被按停在这条路上有两个出口**：走不到 `scrape::run` 的那个由 `?` 一下把手
        // 交出 `Cutoff::Halted`（记成「停了」，一个字节都没写），走到了的就是这一个。
        if let Some(said) = left_behind(&outcome) {
            task.halfway(said);
        }
        Product::Scraped(Box::new(outcome))
    })
    .map_err(|error| Cutoff::failed(format!("刮削失败：{error}")))
}

/// **名字那一层**要的那副原料：剥离规则、本机那份中文离线索引，以及索引背后那份库。
///
/// **刮削与识别摆的是同一副**（`romcat_core::identify::fuzzy` 与
/// `romcat_core::scrape` 认的是同一份规则、同一份索引），所以它只有这一处：两条路各开
/// 一遍的话，同一个变体在两条路上会撞到不同的条目——而那是**写进库里的结论**，
/// 不是显示上的差别。识别那一侧走 [`crate::stages`]。
///
/// 它交出来的是这三样本身而不是折好的 [`fuzzy::Naming`]：后者**只借不拥有**，
/// 得活在调用方的栈上。
pub(crate) struct NamingParts {
    /// **剥离规则**：拿去撞中文离线源的那一串字是它剥出来的。
    pub rules: romcat_core::filename::Rules,
    /// 索引背后那份库。**刮削那一侧还要拿它取简介**（`scrape::zh::Summaries`），
    /// 识别那一侧只要索引。
    pub store: Option<zh::store::Store>,
    /// 索引本身。**没取过数、或者取过但空的，都是 `None`**——那不是错误，只是少一层。
    pub index: Option<zh::Index>,
}

impl NamingParts {
    /// 开一副出来。
    ///
    /// # Errors
    /// 规则读不动、那份索引库打不开、或者索引读不出来时返回一句给人看的话；
    /// 重建那一趟被按停时交出 [`Cutoff::Halted`](romcat_core::task::Cutoff::Halted)。
    pub(crate) fn open(workspace: &Path, task: &Handle) -> Result<Self, Cutoff> {
        // **剥离规则读工作目录里那份**（`sources::rules`，与命令行同一条查法）。
        let rules = romcat_core::sources::rules(workspace)?;
        let store = open_zh(workspace, &rules, task)?;
        let index = match store.as_ref().map(zh::store::Store::load).transpose() {
            Ok(index) => index.filter(|index: &zh::Index| !index.is_empty()),
            Err(error) => return Err(Cutoff::failed(format!("中文索引读不出来：{error}"))),
        };
        Ok(Self {
            rules,
            store,
            index,
        })
    }

    /// 折出**名字那一层**认得的东西。借的是这副原料自己，所以它活不过这个 `self`。
    pub(crate) fn naming(&self) -> fuzzy::Naming<'_> {
        fuzzy::Naming {
            rules: &self.rules,
            index: self.index.as_ref(),
            tuning: zh::Tuning::default(),
        }
    }
}

/// 本机那份中文离线索引。**没取过数不是错误**——少一层而已，报告会说清楚。
///
/// **两条路都经 [`NamingParts::open`] 走这儿**：刮削与识别摆的是同一副原料。
///
/// 结构版本对不上时从本机那份原件就地重建（`zh::sync::rebuild`），一个网络请求都不发；
/// **重建不成也只是少一层**，与命令行同一条口径（那一侧的 `heal_zh_store`）。
/// 打不开那份库才是错——那说明它在，只是坏了或者比程序新，静悄悄当成「没取过数」跑下去，
/// 用户会对着一份缺了简介的报告以为数据源就是这么浅。
///
/// **重建要几分钟**（读+解 960 MB），所以把这一趟的把手交下去：进度接到
/// [`Handle::tick`]，「停下」接到它底下那个中断信号——界面上那个按钮于是也停得动它，
/// 而停下的地方在两条记录之间，那份索引原样等着下一趟（ADR-0005：判断在核心里，
/// 这一层只把把手转发进去）。
pub(crate) fn open_zh(
    workspace: &Path,
    rules: &romcat_core::filename::Rules,
    task: &Handle,
) -> Result<Option<zh::store::Store>, String> {
    let path = workspace::zh_store_path(workspace);
    if !path.exists() {
        return Ok(None);
    }
    let mut store =
        zh::store::Store::open(&path).map_err(|error| format!("中文索引打不开：{error}"))?;
    if store.rebuilding().is_some() {
        // 平台清单同样读工作目录里那份：重建要把数据源写的平台名折成本工具的平台名。
        let manifest = romcat_core::sources::manifest(workspace)?;
        let _ = zh::sync::rebuild(
            &RealFs,
            &mut store,
            &manifest,
            rules,
            &workspace::zh_cache_dir(workspace),
            &mut zh::sync::Context {
                cancel: Some(task.cancel()),
                progress: Some(&mut |at: zh::sync::Progress| task.tick(at.bytes, at.total)),
            },
        );
    }
    Ok(Some(store))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一趟**被按停**的刮削交回来的账：报告是真的，只是这一趟只走了一段。
    fn 按停的那一趟() -> scrape::Outcome {
        scrape::Outcome {
            interrupted: true,
            new_blobs: 7,
            deduped: 5,
            ..scrape::Outcome::default()
        }
    }

    #[test]
    fn 被按停的那一趟说得出留下了什么_而且不说跑完了() {
        // 挂单 `Q217`：刮削这条路上「被按停」有**两个出口**。走不到 `scrape::run` 的
        // 那个由把手交出「被按停了」，另一个走到了——它照旧返回 `Ok`，产物长得跟跑完的
        // 那一份一模一样。不报「停在半路」的话，任务屏历史写「完成」，而这一屏同时说
        // 「按停了」，同一趟活在两处说两套话。
        let 留下了 = left_behind(&按停的那一趟()).expect("被按停就该说得出留下了什么");
        assert!(留下了.contains("落进了中立库"), "{留下了}");
        assert!(留下了.contains("接着采"), "说不出下一趟怎么接：{留下了}");

        // 回执与任务屏历史那一行得说的是**同一档**：那一趟记的是「部分完成」，
        // 而 `Ending::Stopped.render()` 正好是「已取消」——起头写它就是两档撞脸。
        let 回执 = finished(&按停的那一趟());
        assert!(!回执.contains("跑完了"), "按停的那一趟说成了跑完了：{回执}");
        assert!(回执.starts_with("刮削部分完成"), "{回执}");
        assert!(回执.contains("被你按停的"), "说不出是谁收的手：{回执}");
    }

    #[test]
    fn 联网源自己收的手_任务台上那一句说得出为什么收手() {
        // 刮削弹层排上就关，回执交给任务台（收挂单 `Q664`）：任务台历史上「部分完成」后头那半句就是
        // `left_behind` 这一句。为什么收的手（配额超限是硬停、明天再来，ADR-0007）从前只写在弹层里那份回执上，
        // 回执不画了，这一句里就得有。
        let 收手的那一趟 = scrape::Outcome {
            halted: Some(online::Halt::Quota {
                source: "ScreenScraper".to_string(),
                why: "当日未识别 ROM 配额超限（431）".to_string(),
            }),
            new_blobs: 3,
            ..scrape::Outcome::default()
        };
        let 留下了 = left_behind(&收手的那一趟).expect("自己收了手就该说得出留下了什么");
        let 为什么 = 收手的那一趟
            .halted
            .as_ref()
            .map(online::Halt::describe)
            .unwrap_or_default();
        assert!(留下了.contains("落进了中立库"), "{留下了}");
        assert!(
            留下了.contains(&为什么),
            "任务台上说不出为什么收的手：{留下了}"
        );
    }

    /// 一本联网那一档的账：五个作品锚点有判据可查，`requests` 个这一趟真会问。
    fn 联网那本账(requests: u64) -> Estimate {
        Estimate {
            variants: 10,
            works: 5,
            requests,
            over_budget: None,
            budget: 200,
            media_downloads: false,
            media_disk_read: false,
            queryable: 5,
            elapsed: std::time::Duration::from_millis(5_000),
        }
    }

    #[test]
    fn 预估框那一面照那本账说_下界加号_撞上上限写真会发的数_几句照次序叠() {
        // 拿主意的人 2026-10-02 裁 `F-8`（下界加「+」）、`F-9`（撞上上限写这一趟真会发的数，右边说想问多少）、`F-10`、
        // `F-11` 的退路（收媒体写「—」），2026-10-04 裁「右边几句照次序全叠」。合成小库撞不上 200 的上限，
        // 这几样在这儿按核心交来的那本账逐字钉。
        let 撞上上限又收媒体 = Estimate {
            requests: 200,
            over_budget: Some(1_284),
            media_downloads: true,
            media_disk_read: true,
            queryable: 1_284,
            elapsed: std::time::Duration::from_millis(400),
            ..联网那本账(200)
        };
        let face = estimate_face(&撞上上限又收媒体, true);
        assert_eq!(face.requests, "200+");
        assert_eq!(face.elapsed, "不到 1 秒");
        assert_eq!(face.disk, "—");
        assert!(face.warn, "真会发请求时该换警示样");
        assert_eq!(
            face.note,
            "将消耗 ScreenScraper 配额。配额按账号和 IP 计算，超出可能导致封禁。\
             这一批想问 1,284 条，这一趟到 200 就停，剩下的下一趟再来。\
             另加下载图片的请求，有几份要查过才知道。\
             首趟要把图从主库读一遍，读多少要读过才知道。"
        );

        let 准数 = estimate_face(&联网那本账(5), true);
        assert_eq!(准数.requests, "5", "不收媒体时请求数是准数，不加「+」");
        assert_eq!(准数.elapsed, "5 秒");
        assert_eq!(准数.disk, "0 B");
        assert_eq!(准数.note, QUOTA_NOTE);
    }

    #[test]
    fn 勾着联网却一个请求都不发_两种缘故分开说_都不警示() {
        // 拿主意的人 2026-10-04 裁：一个可查的条目都没有（`F-12`）与可查的上一趟都查过、输入没变，说法不同——
        // 说成同一句，就有一种是假话。
        let 都查过了 = estimate_face(&联网那本账(0), true);
        assert!(!都查过了.warn);
        assert_eq!(都查过了.requests, "0");
        assert_eq!(都查过了.note, ALL_ASKED);

        let 没有可查的 = estimate_face(
            &Estimate {
                queryable: 0,
                ..联网那本账(0)
            },
            true,
        );
        assert!(!没有可查的.warn);
        assert_eq!(没有可查的.note, NOTHING_TO_ASK);

        // 收媒体时那一格也不加「+」：一个条目都不问，就没有图要下。
        let 收媒体 = estimate_face(
            &Estimate {
                media_downloads: true,
                media_disk_read: true,
                ..联网那本账(0)
            },
            true,
        );
        assert_eq!(收媒体.requests, "0");
        assert_eq!(收媒体.note, format!("{ALL_ASKED}{DISK_UNKNOWN}"));

        // 只用本地源：一个请求都不发，说的是本地那句。
        let 本地 = estimate_face(
            &Estimate {
                queryable: 0,
                ..联网那本账(0)
            },
            false,
        );
        assert_eq!(本地.note, LOCAL_NOTE);
        assert!(!本地.warn);
    }

    #[test]
    fn 真跑完的那一趟不报停在半路() {
        // 报了的话它就成了「停在半路」那一档——那一行会说「留下了什么」，
        // 而这一趟其实什么都没剩下要接着做的。
        let 跑完了 = scrape::Outcome {
            new_blobs: 12,
            ..scrape::Outcome::default()
        };
        assert!(left_behind(&跑完了).is_none());
        assert!(finished(&跑完了).contains("跑完了"));
    }
}
