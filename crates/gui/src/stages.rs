//! 库屏上的**工序**那一段：这个库还差哪几道步骤，点一下排一趟**任务**上台。
//!
//! ## 为什么这一段说的是「还差多少」而不是「上次几点跑的」
//!
//! 人要的是**下一步该干什么**，时间戳答不了这个问题：加了一块盘重扫之后，识别那一行的
//! 数字自己就涨上去，不必再自己推理「是不是该重跑识别了」。**工序是那道步骤，
//! 任务是跑那一趟**（`CONTEXT.md` 的**工序**条）——点一道工序的按钮，排一趟任务上台。
//!
//! ## 领域判断一条都不在这里
//!
//! 「还差多少」怎么算、那一行画哪句话全在 [`romcat_core::stage`]：这一层只画、只转发
//! （ADR-0005）。识别那个数**不另造一份**——它与**待确认队列**屏、与命令行
//! `triage list` 印的是同一个（[`Catalog::not_run_count`](romcat_core::catalog::Catalog::not_run_count)）。
//!
//! ## 排一趟活的入口只有一个
//!
//! [`Section::start`] 是**这一段唯一的排活入口**，各屏空态上那几颗捷径调的也是它
//! （待确认队列屏四处、浏览屏一处，从前写的都是「去开终端跑一次」，
//! 票 `gui-self-sufficient/09` 换成了就地的按钮）——窗口上那一层由
//! [`App::start_stage`](crate::app::App::start_stage) 递过来。**同一趟活不许有第二份
//! 实现**：两份实现迟早会在「排的时候顺手做了什么」上分叉。
//!
//! 导出撞上外面有人动过之后那颗「我看过了，照写」（`Section::force_export`）也不另起
//! 一份：它与 `start` 走的是同一个 `queue`，只多压一格**只管这一趟**的旋钮。
//!
//! ## 导出那一支上那颗铺媒体开关
//!
//! **默认关着**（[`LAY_MEDIA`]），关着时导出那一趟与没有这颗开关时一样。打开时先排一趟
//! 「算一遍要铺多少媒体」上任务台，算出来的那句代价（[`media_cost`]）画在开关底下；
//! **那句话画出来之前按导出，当场说一句、不排**——人按下去的那一刻得已经知道要付多少，
//! 平常那一趟与照写那一趟一个口径。
//!
//! 数是核心库那一处算的**上界**（`romcat_core::adapter::transfer::media_to_lay`，界面不另算
//! 一份），缓存到库可能变了为止：这一段重读库、跑完一道工序（导出除外）、改导出配置、
//! 离开库屏再回来（挂单 `Q651`、`Q654`）。
//!
//! ## 后台那条线程写的是哪一份库
//!
//! 识别与折标题都要**写**中立库（两者起手都先把上一轮折出来的清干净），而
//! `rusqlite::Connection` 不是 `Sync`。于是后台那条线程按文件路径自己再开一份现场
//! （`Site::open_file`），与扫描、刮削两条路一模一样；跑完这一段 `reload` 一次。
//! **只活在内存里的库（合成数据）没有文件**，那时就地跑完——那份库小到几毫秒就走完。
//!
//! ## 按停停在哪儿
//!
//! **四支在这件事上各不一样，而差别是真的**：识别一路往中立库写批，按停时已经算完的
//! 那些结论真的落了库，所以它报**停在半路**；刮削两档都有——还没开采（读优先级表、
//! 开中文离线源那几下）就停下记**已取消**，采过之后停下记**部分完成**，已经采到的那些
//! 落进了中立库（`crate::scrape::run`，与浏览屏刮削面板那一趟同一个函数）；
//! 折标题的写是「清掉再写回」，中间停下
//! 等于把整份**标题集合**丢掉——所以它的最后一个停下点摆在写回**之前**
//! （`romcat_core::title::run_task`），那一趟要么写完、要么一个字节都没写，
//! 按停记的是「停了」；导出**一份文件一份文件地写**，写完一份就把**底本**一起存进
//! 中立库，于是它两档都有——一份都没写就停下记「停了」，写过之后停下记**停在半路**
//! （`romcat_core::adapter::transfer::export_task`）。四支都由长入口自己说
//! （`romcat_core::task::Handle::halfway` 的文档写着这条分界）。

use std::path::{Path, PathBuf};

use romcat_core::adapter::report::Conflict;
use romcat_core::adapter::transfer::{self, ExportOptions};
use romcat_core::catalog::{Catalog, CatalogError, ExportSetup, Roots};
use romcat_core::fs::RealFs;
use romcat_core::identify;
use romcat_core::identify::model::{Answers, DEFAULT_MODEL, Guessing, Limits, Price};
use romcat_core::report::{human_bytes, thousands};
use romcat_core::scrape::pool::MediaPool;
use romcat_core::site::Site;
use romcat_core::stage::{Stage, StageRow, Stages};
use romcat_core::task::{Cutoff, Ending, Finished, Handle};
use romcat_core::triage::{self, batch::Coverage};
use romcat_core::{title, verdict, workspace};

use crate::task::{Product, Tasks};
use crate::tokens::Tokens;
use crate::{font, look};

/// 裁决那一道的按钮上写的字（设计稿原话，挂单 `Q881` 已裁：照稿）：**它不排任务**，把人带去待确认队列屏（`Section::start`）。
pub const TO_QUEUE: &str = "去处理";

/// 识别那一行还没做完、DAT 库也还没下载时底下那句小字（设计稿原话，挂单 `Q882`）。
pub const IDENTIFY_NEEDS_DAT: &str = "需要先下载 DAT 仓库，否则只能按文件名识别";

/// 同上，下载 DAT 那一趟已经排在台上时（设计稿原话）。
pub const IDENTIFY_DAT_COMING: &str = "数据源下载中，完成后即可运行";

/// 识别那一行还没做完、DAT 库在时底下那句小字（设计稿原话）：工序段排的那一趟识别一个请求都不发——模型推断那一层只用
/// 库里已经问过的答案（`run` 的文档）。
pub const IDENTIFY_LOCAL: &str = "本地运行，不产生网络请求";

/// 导出设置那一块底下那句说明（设计稿原话，挂单 `Q887` 已裁：逐字照稿）。
pub const EXPORT_HELP: &str = "导出目录通常就是主库根目录，这样元数据里的相对路径可以直接使用。只写入元数据文件，不会移动或修改任何 ROM。";

/// 导出那一支上那颗**铺媒体**开关上写的字（票 `one-criterion-per-thing/09`）。
///
/// **摆成常量是给重排留的**：库屏照稿重排时（票 `gui-looks-like-the-design/06`）这颗开关
/// 一个字都不许丢，钉它的测试按这串字在屏上找。
pub const LAY_MEDIA: &str = "一起铺媒体";

/// 开着**铺媒体**时屏上先画的那句代价：这一趟**最多**要铺几份、共多大。
///
/// **按下导出之前就得说得出来**（票 `one-criterion-per-thing/09`）：不是按下去之后才发现
/// 在拷贝。两个数是核心库那一处算的（[`transfer::media_to_lay`]，ADR-0024），界面不另算
/// 一份。那是个**上界**——落点上已经有的真铺时不重铺——所以说成「最多」（挂单 `Q584`）。
///
/// **摆成函数是给重排留的**，理由同 [`LAY_MEDIA`]。
#[must_use]
pub fn media_cost(files: u64, bytes: u64) -> String {
    format!(
        "开着它，这一趟最多要铺 {} 份媒体，共 {}。落点上已经有的不重铺；\
         媒体池与导出目录不在同一块盘上时是整份复制。",
        thousands(files),
        human_bytes(bytes),
    )
}

/// 开着**铺媒体**、那句代价（[`media_cost`]）还没画出来时按导出，屏上挂的那一句。
/// 一处写、两处认：闸挡下时挂它，数出来或关掉开关时按它收掉（`Section::clear_media_refusal`）。
fn media_refusal() -> String {
    format!("开着「{LAY_MEDIA}」，屏上却还没说清这一趟最多要铺多少——先看清那句再按。")
}

/// 算「这一趟最多要铺多少」那一趟在任务台上叫什么。测试按它在任务台上找那一趟。
pub const COUNT_MEDIA: &str = "算一遍要铺多少媒体";

/// 开着**铺媒体**的那一趟走完之后回执里那一句：铺出去几份、怎么铺的、落点上本来就有几份。
///
/// **没铺出去的也得说出口**：落点被别的东西占着的一律不覆盖、没铺成的留在报告里
/// （`romcat_core::adapter::report::MediaReport`），它们不会出现在前端里——不说的话，
/// 人对着前端里缺的那几张封面查不出为什么。
fn media_laid(media: &romcat_core::adapter::report::MediaReport) -> String {
    let mut line = format!(
        "媒体：铺出去 {} 份（硬链接 {}、复制 {}），落点上本来就有 {} 份。",
        thousands(media.placed()),
        thousands(media.linked),
        thousands(media.copied),
        thousands(media.already),
    );
    if !media.occupied.is_empty() {
        line.push_str(&format!(
            "{} 份的落点上有别的东西，没覆盖。",
            thousands(media.occupied.len() as u64),
        ));
    }
    if !media.failures.is_empty() {
        line.push_str(&format!(
            "{} 份没铺成。",
            thousands(media.failures.len() as u64)
        ));
    }
    line
}

/// 开着**铺媒体**时那句代价（[`media_cost`]）眼下是什么样。
#[derive(Debug, Clone, PartialEq, Eq)]
enum MediaCost {
    /// 还没算过，或者算过的那个数作废了（[`Section::expire_media_cost`]）：开关开着时
    /// 下一帧排一趟去算。
    NotAsked,
    /// 正在任务台上算。
    Counting {
        /// 那一趟的任务号。
        id: u64,
        /// 算的时候库变了：交回来的那个数读的可能是变之前那一份，不用，再算一遍。
        expired: bool,
    },
    /// 算出来了。
    Counted {
        /// 最多几份。
        files: u64,
        /// 最多共多少字节。
        bytes: u64,
    },
    /// 没算出来：那句话。
    NotCounted(String),
    /// **没去算**：按下去之前就判得出算不出来（还没选过导出配置，或者记着的格式这一版没有
    /// 适配器，[`export_refusal`]）。那句话。
    ///
    /// 与 [`Self::NotCounted`] 分开，是因为两档收尾说的不一样：那一档说「关掉再打开就重算」，
    /// 这一档要人先去选——选好之后自己重算（`Section::set_export_setup` 让它作废），不必关掉再打开。
    Refused(String),
}

/// 库屏上的**工序**那一段。
pub struct Section {
    /// 工作目录：DAT 库、中文离线源、TitleID 索引都住在它下面。
    workspace: PathBuf,
    /// 每一道工序还差多少。**从核心库现折**（[`Stages::survey`]），不自己攒一份。
    stages: Stages,
    /// 正在跑的那几趟活的任务号，用来禁掉重复按下。
    running: Vec<(u64, Stage)>,
    /// 记住的那套**导出**配置：往哪个前端格式、哪个目录写。
    /// **从中立库现读**（[`Catalog::export_setup`](romcat_core::catalog::Catalog::export_setup)），
    /// 与工序那几行同一趟 [`Section::reload`]——命令行 `romcat export` 改过之后
    /// 这一屏跟着变。
    setup: Option<ExportSetup>,
    /// 那两个键**读不出来**时的那句话；读得出来（哪怕是「还没选过」）就是 `None`。
    ///
    /// **「这份库读不动」与「还没选过」是两句话**，与工序那几行同一个口径
    /// （`romcat_core::stage::export_row` 逐字写着这一条）：前者是一件该去查的事，
    /// 后者是一件该去做的事。少了这一格，读不动的那份库会被画成「第一次导出之前先选
    /// 一次」——把该去查的事说成了该去做的事。
    ///
    /// **它不占 [`Self::error`] 那一格**：`settle` 收场时才写那一格，而这一趟重读发生在
    /// 它之后，占过去会把「有几份没写」那句话冲掉。
    setup_unreadable: Option<String>,
    /// 导出设置那一块里人正挑着、打着的字。
    ///
    /// **与 [`Self::setup`] 分开**：那一份是库里记着的，这一份是人手上两样还没挑齐、还没记进库的。
    /// 合成一格的话，人改了一半切走再回来，屏上会显示一套并没有记进库的配置。
    ///
    /// **两格就是两个 `String`**：它们是下拉与输入框里的字，不是一套立得住的配置；立得住的那一份叫
    /// [`ExportSetup`]，由 [`ExportSetup::check`] 折出来。
    format_draft: String,
    out_draft: String,
    /// 刚跑完的那一道工序，等窗口取走。
    ///
    /// **跑完识别之后待确认队列得自己重新列过**，而这一段够不着那一屏（ADR-0005：
    /// 屏与屏之间不该互相拿着对方）。所以这儿只放一个记号，由
    /// [`App::poll_tasks`](crate::app::App::poll_tasks) 取走——与子库屏那几个跳转记号
    /// 同一个办法。
    ran: Option<Stage>,
    error: Option<String>,
    notice: Option<String>,
    /// 上一趟**导出**撞上「外面有人动过」而没写的那几份，逐份点名画在屏上。
    ///
    /// **名单是导出本来就交得出的那一份**（`ExportReport::conflicts`）：导出一份文件一份
    /// 文件地写、每写完把**底本**存进中立库，哪几份对不上是现成的，这一层不另比一遍。
    conflicts: Vec<Conflict>,
    /// 导出那一支上那颗**铺媒体**开关（[`LAY_MEDIA`]）。**默认关着**，只记在这一段上、
    /// 不记进库：媒体池住在工作目录里、主库多半在外置盘上，跨盘就是整份复制——
    /// 几十 GiB 的事，每次开窗都该由人自己再点一次。
    lay_media: bool,
    /// 开着铺媒体时这一趟最多要铺多少（[`media_cost`]）。**打开开关时算一次、库变了再算**
    /// （`Section::expire_media_cost`），不每帧重算：
    /// 算它要把全库变体的媒体引用逐个问一遍、再逐份问一遍媒体池
    /// （`romcat_core::sync::media::lay_for`），真库上要多久没量过——所以它排在任务台上跑，
    /// 不在画帧这条线程上。
    media_cost: MediaCost,
    /// 上一次画这一段是第几趟画帧（egui 的 `cumulative_pass_nr`）。**隔了帧没画，就是人离开过
    /// 库屏**：那句铺媒体的代价跟着作废（`Section::lay_media_ui`，挂单 `Q654`）。
    drawn_at: Option<u64>,
    /// 库屏排上的那一趟**取回 DAT** 眼下在不在台上（`Section::set_dat_on_board`）。
    dat_on_board: bool,
    /// 台上头一趟**扫描**的任务号（`Section::set_scan_on_board`）。扫描不在这一段排，这一段只照它
    /// 禁掉扫描那一行的按钮。
    scan_on_board: Option<u64>,
    /// 按下去了、却**不在这一段排**的那一道（扫描、裁决），等够得着的那一处取走
    /// （`Section::take_handoff`）。
    handoff: Option<Stage>,
    /// 裁决那一行底下那句「前几批可一次处理多少」要的数（`triage::head_coverage`）：**算一次存着**，只在队列可能变了时
    /// 重算（[`Section::recount_queue_head`]）；每次问完工序那几行把它交回去（`Stages::set_queue_head`）。
    queue_head: Option<Coverage>,
}

impl Section {
    /// 开一个。`workspace` 是**工作目录**。
    #[must_use]
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            stages: Stages::default(),
            running: Vec::new(),
            setup: None,
            setup_unreadable: None,
            format_draft: String::new(),
            out_draft: String::new(),
            ran: None,
            error: None,
            notice: None,
            conflicts: Vec::new(),
            lay_media: false,
            media_cost: MediaCost::NotAsked,
            drawn_at: None,
            dat_on_board: false,
            scan_on_board: None,
            handoff: None,
            queue_head: None,
        }
    }

    /// 导出那一支上那颗**铺媒体**开关眼下开着没有。**默认关着。** 测试拿它核对。
    #[must_use]
    pub fn lay_media(&self) -> bool {
        self.lay_media
    }

    /// 拨那颗**铺媒体**开关（界面上点 [`LAY_MEDIA`] 走的就是它）。
    ///
    /// **打开时当场排一趟去算这一趟最多要铺多少**：那句代价（[`media_cost`]）要在按下
    /// 导出**之前**画出来。上一趟没算出来（被撤掉、读不动库）时再打开就重算——屏上那句
    /// 「关掉再打开就重算」说的就是这一下；已经算出来、或者正在算的，不重排。
    pub fn set_lay_media(&mut self, on: bool, site: &Site, tasks: &mut Tasks) {
        self.lay_media = on;
        // 关掉了，「开着它却还没说清」那句话就不成立了。
        if !on {
            self.clear_media_refusal();
        }
        if on
            && matches!(
                self.media_cost,
                MediaCost::NotAsked | MediaCost::NotCounted(_) | MediaCost::Refused(_)
            )
        {
            self.count_media(site, tasks);
        }
    }

    /// 排一趟「这一趟最多要铺多少」上任务台（[`media_cost_run`]）。
    ///
    /// 与子库屏「算一遍容量」同一条路：整条只读，后台那条线程读的是同一个库文件的
    /// **第二份只读连接**（[`Catalog::read_only`]）；只活在内存里的库分不出第二份，
    /// 就地跑完——合成数据上那是几毫秒的事。
    fn count_media(&mut self, site: &Site, tasks: &mut Tasks) {
        // **没选过导出配置（或者记着的格式没有适配器），就不排**：媒体的布局随前端格式不同，
        // 那一趟一定算不出来——这是按下去之前就判得出的，排上去只会在任务历史里多一条压根没开跑
        // 的「失败」（票 `gui-looks-like-the-design/07`）。选好之后那个数作废
        // （`Self::set_export_setup`），开关开着时下一帧自己重算。
        if let Some(why) = export_refusal(&site.catalog) {
            self.media_cost = MediaCost::Refused(why);
            return;
        }
        let workspace = self.workspace.clone();
        let id = match site.catalog.read_only() {
            Ok(reader) => tasks.queue(COUNT_MEDIA, move |task| {
                media_cost_run(&reader, &workspace, task)
            }),
            Err(CatalogError::NotOnDisk { .. }) => tasks.run_here(COUNT_MEDIA, |task| {
                media_cost_run(&site.catalog, &workspace, task)
            }),
            // 别的原因是**意外**：直说，不退到画帧这条线程上偷偷算一遍（同子库屏那一处）。
            Err(why) => {
                self.media_cost = MediaCost::NotCounted(format!(
                    "要铺多少没算出来：读中立库要另开一份只读连接，这一下没开出来（{why}）"
                ));
                return;
            }
        };
        self.media_cost = MediaCost::Counting { id, expired: false };
    }

    /// 屏上挂的若是那句「开着铺媒体却还没说清」（[`media_refusal`]），收掉；别的话不碰。
    fn clear_media_refusal(&mut self) {
        if self.error.as_deref() == Some(media_refusal().as_str()) {
            self.error = None;
        }
    }

    /// 那句铺媒体的代价作废：库变了，或者可能变了。
    ///
    /// 开关开着时下一帧重排一趟去算（`Section::lay_media_ui`）。**正在算的那一趟不撤**，
    /// 只记一笔：它交回来的那个数不用，再算一遍——它读的可能是变之前那一份。
    fn expire_media_cost(&mut self) {
        self.media_cost = match self.media_cost {
            MediaCost::Counting { id, .. } => MediaCost::Counting { id, expired: true },
            _ => MediaCost::NotAsked,
        };
    }

    /// 从库里重新问一遍：每一道工序还差多少。**一个字节都不读主库**——
    /// 外置盘不在位时这几行照样看得见。
    ///
    /// **重读库就是库可能变了**（加根、移根、扫完、取完数据源都走这儿）：开着铺媒体时那句
    /// 代价跟着作废、重算。
    pub fn reload(&mut self, site: &Site) {
        self.resurvey(site);
        // **重读库就是队列可能变了**：前几批盖住多少跟着重算（`Self::recount_queue_head`）。
        self.recount_queue_head(site);
        self.expire_media_cost();
    }

    /// 裁决那一行底下那句「前几批可一次处理多少」要的数（`triage::head_coverage`）：**算一次存着**——它要把整个队列连候选
    /// 读一遍，跟着每次问工序那几行算付不起。**只在队列可能变了时重算**：重读库（[`Self::reload`]：加根、移根、扫完、开库，
    /// 以及待确认队列屏落下或撤回一批之后窗口那一句 `App::route`）与识别跑完（[`Self::settle`]）。别的时候问完工序那几行，
    /// 只把存着的那一份交回去（[`Self::resurvey`]）。沉淀库或中立库读不动就是 `None`，那一句不画。
    fn recount_queue_head(&mut self, site: &Site) {
        self.queue_head = verdict::Index::load(&site.store, &site.library_identity)
            .ok()
            .and_then(|verdicts| {
                triage::head_coverage(&site.catalog, &verdicts, triage::HEADLINE_BATCHES).ok()
            });
        self.stages.set_queue_head(self.queue_head);
    }

    /// [`Self::reload`] 里读库的那一半：工序那几行与导出配置。
    fn resurvey(&mut self, site: &Site) {
        self.stages = Stages::survey(&site.catalog, &site.store, &site.library_identity);
        // 前几批盖住多少不跟着这一趟问（它贵）：把存着的那一份交回去（`Self::recount_queue_head`）。
        self.stages.set_queue_head(self.queue_head);
        // **读不动与还没选过分两支说**（同 `stage::export_row`）：整段一起失败不成——
        // 一个读不出来的键会让工序段上连识别那一行都消失（与 `Stages::survey` 同一条）。
        match site.catalog.export_setup() {
            Ok(setup) => {
                self.setup = setup;
                self.setup_unreadable = None;
            }
            Err(error) => {
                self.setup = None;
                self.setup_unreadable = Some(format!("中立库读不动：{error}"));
            }
        }
        // **人正打着的字不覆盖**：只在两格都还空着的时候把库里记着的那套填进去。
        if self.format_draft.is_empty()
            && self.out_draft.is_empty()
            && let Some(setup) = &self.setup
        {
            self.format_draft = setup.format.clone();
            self.out_draft = setup.out.to_string_lossy().into_owned();
        }
    }

    /// 记住的那套**导出**配置；一次都没选过就是 `None`。测试拿它核对。
    #[must_use]
    pub fn export_setup(&self) -> Option<&ExportSetup> {
        self.setup.as_ref()
    }

    /// 选一次**前端格式**与**导出目录**，记进中立库。**下一趟不必再选。**
    ///
    /// **判据在核心里**（[`ExportSetup::check`]，ADR-0005）：这一层只把话转出来
    /// ——「有没有这个格式」散一份判断到界面上，命令行与界面迟早会对同一个字给出
    /// 两种答复。
    pub fn set_export_setup(&mut self, site: &Site, format: &str, out: &str) {
        match ExportSetup::check(format, out) {
            Ok(setup) => match site.catalog.set_export_setup(&setup) {
                Ok(()) => {
                    self.error = None;
                    self.notice = Some(format!(
                        "记下了：按 {} 的格式写进 {}。之后每一趟导出都照这一套写。",
                        setup.format,
                        setup.out.display(),
                    ));
                    self.format_draft = setup.format.clone();
                    self.out_draft = setup.out.to_string_lossy().into_owned();
                    self.setup = Some(setup);
                    self.setup_unreadable = None;
                    // **媒体的布局随前端格式不同**：换了格式，那句铺媒体的代价就不是这个数了。
                    self.expire_media_cost();
                }
                Err(error) => {
                    self.notice = None;
                    self.error = Some(format!("这份中立库写不进去：{error}"));
                }
            },
            Err(error) => {
                self.notice = None;
                self.error = Some(error.to_string());
            }
        }
    }

    /// 一道工序一行。测试拿它核对。
    #[must_use]
    pub fn rows(&self) -> &[StageRow] {
        self.stages.rows()
    }

    /// 某一道工序那一行；没有就是 `None`。
    #[must_use]
    pub fn of(&self, stage: Stage) -> Option<&StageRow> {
        self.stages.of(stage)
    }

    /// 这一行在工序段上真画出来的那句话（`Stages::line`：前面有一道没做完、它自己数出来是零时说在等谁）。
    /// 测试拿它核对屏上画的是哪一句。
    #[must_use]
    pub fn line(&self, row: &StageRow) -> String {
        self.stages.line(row)
    }

    /// 这道工序上有没有**任务**在台上（排着队也算）；有就是那一趟的任务号。
    /// **测试拿它核对「按钮按不下去」那一条。**
    ///
    /// 名字不叫 `job_of`：词表**任务**那一条的 `_Avoid_` 里逐字列着 `job`
    /// （`CONTEXT.md`）。库屏那一侧的 `Job` 是这条账在旧代码里的欠款，不往新代码里扩。
    #[must_use]
    pub fn task_of(&self, stage: Stage) -> Option<u64> {
        // 扫描那一趟由库屏自己排、自己认领（`roots::Screen::scan`），这一段只记着它的任务号。
        if stage == Stage::Scan {
            return self.scan_on_board;
        }
        self.running
            .iter()
            .find(|(_, running)| *running == stage)
            .map(|(id, _)| *id)
    }

    /// 这一段眼下报出来的那句错；没有就是 `None`。
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// 这一段眼下报出来的那句话（不是错）。
    #[must_use]
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// 刚跑完的那一道工序，取走就没了。
    ///
    /// **跑完识别之后待确认队列得自己重新列过**，而这一段够不着那一屏（ADR-0005：
    /// 屏与屏之间不该互相拿着对方）。所以这儿只放一个记号，由
    /// [`App::poll_tasks`](crate::app::App::poll_tasks) 取走——与子库屏那几个跳转
    /// 记号同一个办法。
    pub fn take_ran(&mut self) -> Option<Stage> {
        self.ran.take()
    }

    /// 把一道工序**排到任务台上**。
    ///
    /// **这是这一段唯一的排活入口**：库屏工序段那颗按钮与别处的捷径（队列屏空态上那
    /// 几颗「跑识别」、浏览屏撤掉压制之后那颗「折标题」）调的都是它，排的是同一趟活。
    /// 这一段自己那颗「我看过了，照写」也不另起一份：它走的是同一个 `queue`
    /// （`Self::force_export`），而且不对外。
    ///
    /// 同一道工序已经在跑就**什么都不做**——那一行的按钮本来就是禁着的，这一句是给
    /// 别处的捷径兜底的。
    ///
    /// **扫描与裁决两道不在这一段排**（`Self::hand_over`）：扫描交给库屏自己那条扫描的路，
    /// 裁决换到待确认队列屏。
    pub fn start(&mut self, stage: Stage, site: &mut Site, tasks: &mut Tasks) {
        match stage {
            Stage::Scan | Stage::Triage => self.hand_over(stage, site),
            Stage::Identify | Stage::Scrape | Stage::FoldTitles | Stage::Export => {
                let knobs = self.export_knobs();
                self.queue(stage, knobs, site, tasks);
            }
        }
    }

    /// 按下去了、却**不在这一段排**的那两道：留一个记号，由够得着的那一处取走
    /// （[`Self::take_handoff`]）。
    ///
    /// - **扫描**有自己那条现成的路（`roots::Screen::scan`：断点、并发档、按停怎么记账都在那儿，
    ///   而且是按根排的），这一段不另写一份——同一趟活不许有第二份实现。库屏取走记号，把还没完整
    ///   扫过一趟的根排上去（`roots::Screen::take_scan`）。
    /// - **裁决**不是一趟任务：人在待确认队列屏上一批批做。窗口取走记号换到那一屏
    ///   （`App::route`；屏与屏之间不互相拿着，ADR-0005）。
    ///
    /// 按下去之前就判得出的前提不在，照旧只在屏上说（[`Self::refusal`]）。
    fn hand_over(&mut self, stage: Stage, site: &Site) {
        if self.task_of(stage).is_some() {
            return;
        }
        if let Some(why) = self.refusal(stage, site) {
            self.error = Some(why);
            self.notice = None;
            return;
        }
        self.handoff = Some(stage);
    }

    /// 取走「按下去了、却不在这一段排」的那个记号（`Section::hand_over`）：记着的正是这一道就交
    /// `true`，取走就没了。
    pub fn take_handoff(&mut self, stage: Stage) -> bool {
        let 是它 = self.handoff == Some(stage);
        if 是它 {
            self.handoff = None;
        }
        是它
    }

    /// 库屏排上、收掉扫描时拨（`roots::Screen::scan` / `settle`）：台上头一趟扫描的任务号。
    pub(crate) fn set_scan_on_board(&mut self, id: Option<u64>) {
        self.scan_on_board = id;
    }

    /// 带着**照写**重排一趟**导出**：屏上那颗「我看过了，照写」按的就是它。
    ///
    /// **不对外**：它只长在这一段自己画的那份名单底下——没画出那份名单的地方，没有资格
    /// 替人点这一下。**只管这一趟**：照写不记进这一段、不记进库，下一趟撞上外面有人动过
    /// 照样停下来逐份点名——它丢掉的是人的一次手改，所以每次都得人当场点
    /// （票 `gui-answers-all-six/05`）。排的是与 [`Self::start`] 同一份实现，
    /// 带的是同一套旋钮，只多压一格照写。
    fn force_export(&mut self, site: &mut Site, tasks: &mut Tasks) {
        let mut knobs = self.export_knobs();
        knobs.force = true;
        self.queue(Stage::Export, knobs, site, tasks);
    }

    /// 这一段眼下拨着的**导出**旋钮：平常那一趟与照写那一趟带的是**同一套**。
    /// **照写不在这里头**——它从不记在这一段上（[`Self::force_export`]）。
    ///
    /// 这一段上拨得动的导出开关只有**铺媒体**那一颗（[`Self::lay_media`]），在这儿读出来：
    /// [`Self::start`] 与 [`Self::force_export`] 一行不用改，照写那一趟也带着它。
    fn export_knobs(&self) -> ExportKnobs {
        ExportKnobs {
            force: false,
            media: self.lay_media,
        }
    }

    /// 库屏排上一趟**取回 DAT** 时拨上、认领它时拨回（`roots::Screen::fetch` / `settle`）。
    ///
    /// **它在台上，「还没有 DAT 库」就不是按下去之前判得出的了**：轮到识别时它多半已经取回来了
    /// ——改之前识别就这样排在它后面跑成。这时识别照常排上；真取不回来，识别那一趟开跑时自己
    /// 撞上、照实记失败（`identify_run` 里那一问兜底）。
    pub(crate) fn set_dat_on_board(&mut self, on: bool) {
        self.dat_on_board = on;
    }

    /// 这道工序**按下去之前就判得出**的那句拒绝：为什么不行、去哪儿办；前提都在就是 `None`。
    ///
    /// **只收「原料还没备齐」那一类**（ADR-0005 修订段）：盘上/库里缺一样东西，查一眼就知道。
    /// 要读、要算、要跑一段才撞得上的（库读不动、DAT 库打不开）不在这里——那一趟排上去，
    /// 撞上了照实记失败（票 `gui-looks-like-the-design/07`：分开的是压根没开跑与跑了没成）。
    fn refusal(&self, stage: Stage, site: &Site) -> Option<String> {
        match stage {
            // 取回 DAT 那一趟已经排在台上：判不出来，放识别排在它后面（`Self::set_dat_on_board`）。
            Stage::Identify if self.dat_on_board => None,
            Stage::Identify => missing_dat(&self.workspace),
            Stage::Export => export_refusal(&site.catalog),
            Stage::Scan => no_roots(&site.catalog),
            Stage::Scrape | Stage::FoldTitles | Stage::Triage => None,
        }
    }

    /// 排一趟活**只有这一份实现**：[`Self::start`] 与 [`Self::force_export`] 都走它，
    /// 差的只是导出那一支这一趟带哪几个旋钮（[`ExportKnobs`]）。
    fn queue(&mut self, stage: Stage, knobs: ExportKnobs, site: &mut Site, tasks: &mut Tasks) {
        if self.task_of(stage).is_some() {
            return;
        }
        // **按下去之前就判得出的前提不在，就不排**（票 `gui-looks-like-the-design/07`）：只在屏上
        // 说一句为什么不行、去哪儿办。排上去再在那一趟里报失败的话，任务历史里就多一条压根没开跑
        // 的「失败」——那一栏只记真跑过的。**跑起来才撞上的**（库读不动、DAT 库打不开）照旧排上去，
        // 那一趟照实记失败。平常那一趟与照写那一趟都走这儿。
        if let Some(why) = self.refusal(stage, site) {
            self.error = Some(why);
            // 上一趟的回执一起收掉：「识别 跑完了：…」挨着「还没有 DAT 库」，两句读着互相打架。
            self.notice = None;
            return;
        }
        // **开着铺媒体、那句代价还没画出来，就不排**：人按下去的那一刻得已经知道要付多少，
        // 不是按下去之后才发现在拷贝（票 `one-criterion-per-thing/09`）。平常那一趟与照写
        // 那一趟都走这儿，所以两颗按钮一个口径。
        if stage == Stage::Export
            && knobs.media
            && !matches!(self.media_cost, MediaCost::Counted { .. })
        {
            self.error = Some(media_refusal());
            return;
        }
        let workspace = self.workspace.clone();
        // **照写那一趟在任务台历史上也看得出来**：丢掉手改的那一趟不许与平常那几趟长得一样。
        let title = if knobs.force {
            format!("{}（照写）", stage.label())
        } else {
            stage.label().to_string()
        };
        let id = match site.catalog.file().map(Path::to_path_buf) {
            Some(file) => tasks.queue(title, move |task| {
                // 后台这条线程自己开一份写得动的现场：`rusqlite::Connection` 不是
                // `Sync`，界面那条线程手里那一份交不过来。
                let mut site = Site::open_file(&workspace, &file, None)
                    .map_err(|error| format!("这份库在后台开不出来：{error}"))?;
                run(stage, knobs, &mut site, &workspace, task)
            }),
            // 只活在内存里的库（合成数据走这条）分不出第二份连接：**就地跑完**。
            // 那时窗口确实会僵一下，但那份库小到几毫秒就走完——真库一律走上面那条。
            None => tasks.run_here(title, |task| run(stage, knobs, site, &workspace, task)),
        };
        // **上一趟的回执一起收掉**：不收的话「识别 跑完了：…」会挂在新一趟正跑着的
        // 那一行旁边，读起来像这一趟已经跑完了。点名的那几份同理——那份名单说的是
        // 上一趟撞上的，留着它，人会对着一份过期的名单按「照写」。
        self.error = None;
        self.notice = None;
        self.conflicts.clear();
        self.running.push((id, stage));
        // **排上一道别的工序，那句铺媒体的代价当场作废**：刮削、识别都改得动媒体引用或作品
        // 归属，而导出若这时按下去就排在它后面跑——屏上的数说的却是它之前那一份库。作废之后
        // 数重新出来之前导出按不下去（上面那道闸）。导出自己不作废：它不改那个数读的东西。
        if stage != Stage::Export {
            self.expire_media_cost();
        }
    }

    /// 任务台交回来的是不是**算要铺多少媒体**那一趟（[`COUNT_MEDIA`]）；是就认领，
    /// 返回「认领了没有」。
    ///
    /// **与 [`Self::settle`] 分开认**：那一趟整条只读，认领它不等于库变了。窗口那一层认领完
    /// 库屏的活会转告浏览屏整页重读（`App::poll_tasks`），走那条路的话，每打开一次开关、每离开
    /// 库屏再回来一次，浏览屏就白读一遍——所以窗口先问这一句，认领了就不往下走。
    pub fn settle_media_cost(&mut self, done: &Finished<Product>) -> bool {
        let MediaCost::Counting { id, expired } = self.media_cost else {
            return false;
        };
        if id != done.id {
            return false;
        }
        self.media_cost = match &done.ended {
            // 算的时候库变了：这个数读的可能是变之前那一份，作废（下一帧重算）。
            _ if expired => MediaCost::NotAsked,
            Ending::Done(Product::MediaCounted { files, bytes }) => MediaCost::Counted {
                files: *files,
                bytes: *bytes,
            },
            // 已取消、失败：那一档的词从收场渲染出，这儿一个字都不另写。
            other => MediaCost::NotCounted(format!("{COUNT_MEDIA} {}", other.render())),
        };
        // **数出来了，「还没说清」那句话就收掉**：留着它，屏上一句说还没说清、底下一句已经说清。
        if matches!(self.media_cost, MediaCost::Counted { .. }) {
            self.clear_media_refusal();
        }
        true
    }

    /// 任务台交回来一趟跑完的活。**不是自己那一趟就放过去**，返回「认领了没有」。
    pub fn settle(&mut self, site: &Site, done: &Finished<Product>) -> bool {
        let Some(at) = self.running.iter().position(|(id, _)| *id == done.id) else {
            return false;
        };
        let (_, stage) = self.running.remove(at);
        match &done.ended {
            Ending::Done(Product::Identified(outcome)) => {
                self.error = None;
                let mut 回执 = format!(
                    "{} 跑完了：{} 个变体过了一遍，命中 {}。",
                    stage.label(),
                    thousands(outcome.report.total.variants),
                    thousands(outcome.report.total.matched),
                );
                // **用上了库里已经问过的答案就说出口**（挂单 `Q418`）。
                if let Some(line) = paid_answers_line(outcome) {
                    回执.push('\n');
                    回执.push_str(&line);
                }
                self.notice = Some(回执);
            }
            // **回执与刮削面板那一趟是同一句**（`crate::scrape::finished`）：同一个函数
            // 交出来的产物，两处各折一句的话迟早差着字。
            Ending::Done(Product::Scraped(outcome)) => {
                self.error = None;
                self.notice = Some(crate::scrape::finished(outcome));
            }
            Ending::Done(Product::Titled(report)) => {
                self.error = None;
                self.notice = Some(format!(
                    "{} 跑完了：{} 个作品折出 {} 条叫法，其中 {} 个作品有中文叫法。",
                    stage.label(),
                    thousands(report.works),
                    thousands(report.entries),
                    thousands(report.chinese_works),
                ));
            }
            Ending::Done(Product::Exported(report)) => {
                // **数的是真写出去的那几份**，不是整库收敛出来的总数：撞上外面有人动过
                // 的那几份一个字节都没写，把它们算进「写进了几份」等于虚报
                // （`ExportedFile::written` 就是这条界线）。
                let 写出去的: Vec<_> = report.files.iter().filter(|file| file.written).collect();
                let 条目 = 写出去的.iter().map(|file| file.entries).sum::<u64>();
                let mut 回执 = if 写出去的.is_empty() {
                    // 每一份都被挡下、或者库里压根没东西可导。**这一档也得说话**
                    // ——它与「写了几份」长得完全不一样，而底下那句红字说的是为什么。
                    format!("{} 跑完了，可一份元数据都没写出去。", stage.label())
                } else {
                    format!(
                        "{} 跑完了：{} 个条目写进 {} 份元数据文件，实测档位 {}。\
                         一个 ROM 都没搬。",
                        stage.label(),
                        thousands(条目),
                        thousands(写出去的.len() as u64),
                        report.tier,
                    )
                };
                // **开着铺媒体就说铺了什么**（`media_laid`）；关着时报告里没有这一半，一个字都不多。
                if let Some(media) = &report.media {
                    回执.push('\n');
                    回执.push_str(&media_laid(media));
                }
                // **照写掉了哪几份得说出口**（`ExportReport::forced`）：「不静默覆盖」说的是
                // 不许悄悄发生，不是不许发生。逐份点名，与撞上时那份名单一个粒度。
                if !report.forced.is_empty() {
                    回执.push_str(&format!(
                        "\n照写了 {} 份外面有人动过的文件，那几次手改已经没了：",
                        thousands(report.forced.len() as u64),
                    ));
                    for conflict in &report.forced {
                        回执.push_str("\n  ");
                        回执.push_str(&conflict.path);
                    }
                }
                self.notice = Some(回执);
                // **撞上手改要说出口**（验收第 5 条）：跳过的那几份是「你要的事没做，
                // 去处理一下」，与上面那句「跑完了」意思相反，所以它走的是报错那一格
                // ——两句合成一句的话，那几份被吞掉的活会被读成一次顺利的导出。
                self.error = (!report.conflicts.is_empty()).then(|| {
                    format!(
                        "有 {} 份没写——外面有人动过那些文件，没有静默覆盖。\
                         先去看一眼底下点名的那几份；确认那几次手改可以丢掉，\
                         再按「我看过了，照写」。",
                        thousands(report.conflicts.len() as u64),
                    )
                });
                self.conflicts.clone_from(&report.conflicts);
            }
            // **停在半路**：识别起手就把上一轮的结论清干净，所以它一定动过库
            // ——记成「可以当没跑过」是骗人的。那句话由核心库折
            // （`identify::run_task` 里 `Handle::halfway` 报的那一句），这一层原样转出来：
            // 下一趟接着算剩下的（票 `gui-answers-all-six/03`）。
            //
            // 识别那一趟停下时库里已经折进去的模型推断候选照样是真的，那一行与跑完那一支
            // 说的是同一句（`paid_answers_line`）。
            Ending::Halfway { product, .. } => {
                self.error = None;
                let mut 回执 = format!("{} {}", stage.label(), done.ended.render());
                if let Product::Identified(outcome) = product
                    && let Some(line) = paid_answers_line(outcome)
                {
                    回执.push('\n');
                    回执.push_str(&line);
                }
                self.notice = Some(回执);
            }
            // 还排着队就被撤掉的那一趟压根没开跑：一个字节都没写。
            // **停了，什么都没留下。** 两条路走到这一档：还排着队就被撤掉（压根没开跑），
            // 以及开跑了但停在写库之前（折标题走的正是这条，见 `title::run_task`）。
            // 两条留下的是同一件事——中立库一个字节都没动——所以这儿说的是同一句话。
            Ending::Stopped => {
                self.error = None;
                self.notice = Some(format!(
                    "{} {}。中立库一个字节都没动，再按一次就是。",
                    stage.label(),
                    done.ended.render(),
                ));
            }
            // **不静默结束**：哪一步、为什么，两样都说出来。
            Ending::Failed { .. } => {
                self.error = Some(format!("{} {}", stage.label(), done.ended.render()));
            }
            // 别的屏排上去的活轮不到这儿——上面那道任务号已经挡掉了。
            Ending::Done(_) => {}
        }
        // **跑完当场重问一遍**：那一行的数字就是这么刷新的（验收第 6 条）。
        // 被按停的那一趟照样要重问——它写进中立库的那半份结论是真的。
        self.resurvey(site);
        // **识别跑完，待确认队列变了**：前几批盖住多少跟着重算。别的工序不动队列，照旧用存着的那一份。
        if stage == Stage::Identify {
            self.recount_queue_head(site);
        }
        // **跑完一道工序，那句铺媒体的代价跟着作废**——导出除外：它只写底本与时刻戳，
        // `media_to_lay` 读的变体、作品归属、媒体引用、媒体池一样都没动，重算只是白排一趟。
        if stage != Stage::Export {
            self.expire_media_cost();
        }
        self.ran = Some(stage);
        true
    }

    /// 画这一段：**库屏左边那一整张卡里头的东西**（设计稿 `#stages-panel`）——顶上「下一步」、「工序」那条标题栏、
    /// 六行工序，一块一块铺满卡宽，块与块之间一条分隔线。卡的底、描边与圆角由摆它的那一屏画（`roots::Screen`）。
    pub fn ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        let tokens = Tokens::builtin();
        let 缝 = ui.spacing().item_spacing;
        // 块与块之间不留缝：分隔线就是缝。块里头照旧用原来的间距。
        ui.spacing_mut().item_spacing.y = 0.0;
        let 圆角 = tokens.radius.large;
        let 四边 = tokens.space.panel_padding[1];
        // 顶上那一块「下一步」（设计稿 `.nextline`）：`panel-2` 底、四边一样的内边距。它贴着卡顶，上边两个角跟着卡圆。
        let 下一步要跑 = egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .corner_radius(egui::CornerRadius {
                nw: 圆角,
                ne: 圆角,
                sw: 0,
                se: 0,
            })
            .inner_margin(egui::Margin::from(egui::vec2(四边, 四边)))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = 缝;
                ui.set_width(ui.available_width());
                self.next_up_ui(ui)
            })
            .inner;
        look::divider(ui);
        // 标题栏（设计稿 `.phead`）：「工序」加一句说明。
        egui::Frame::new()
            .inner_margin(crate::roots::panel_padding())
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(tokens.space.panel_head_gap, 缝.y);
                // 这一条标题栏里没有按钮：行高照字高，不垫可点控件的最小高（设计稿 `.phead` 只有字时就是字那么高）。
                ui.spacing_mut().interact_size.y = 0.0;
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(font::strong("工序").size(tokens.font.size_panel_title));
                    ui.label(
                        egui::RichText::new("按顺序完成，每一步显示还有多少需要处理")
                            .small()
                            .weak(),
                    );
                });
            });
        look::divider(ui);
        // 这一段自己的错、点名的那几份与回执（不属于哪一行）：有才画，垫同样的内边距。
        let mut 要照写 = false;
        if self.error.is_some() || self.notice.is_some() || !self.conflicts.is_empty() {
            egui::Frame::new()
                .inner_margin(crate::roots::panel_padding())
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = 缝;
                    ui.set_width(ui.available_width());
                    要照写 = self.messages_ui(ui);
                });
            look::divider(ui);
        }
        let mut 要跑 = 下一步要跑;
        // 画的时候不改自己：按下去的那一下先记下来，画完再动（借用检查器要的，
        // 也让「按一下发生什么」读起来是一条直线）。
        let rows: Vec<StageRow> = self.stages.rows().to_vec();
        for (at, row) in rows.iter().enumerate() {
            if at > 0 {
                look::divider(ui);
            }
            if let Some(stage) = self.row_ui(ui, row, site, tasks) {
                要跑 = Some(stage);
            }
        }
        ui.spacing_mut().item_spacing = 缝;
        if let Some(stage) = 要跑 {
            self.start(stage, site, tasks);
        }
        if 要照写 {
            self.force_export(site, tasks);
        }
    }

    /// 这一段自己的错、点名的那几份与回执；交回人按没按「我看过了，照写」。
    fn messages_ui(&self, ui: &mut egui::Ui) -> bool {
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        // **逐份点名**：人要去看的是哪几个文件，一个数答不了。
        let mut 要照写 = false;
        if !self.conflicts.is_empty() {
            ui.indent("外面有人动过的那几份", |ui| {
                for conflict in &self.conflicts {
                    ui.label(&conflict.path);
                    ui.weak(&conflict.why);
                }
                // **不做差量预览**：人拿到的是一份文件名单，看文件本身由他自己去。
                // 默认那一档按钮（设计稿 `.btn`，`look::buttons`）。
                if look::buttons(ui, |ui| {
                    ui.button("我看过了，照写")
                        .on_hover_text(
                            "带着照写重排一趟导出：上面点名的那几份会被写过去，那几次手改就没了。",
                        )
                        .clicked()
                }) {
                    要照写 = true;
                }
                // **画在屏上，不只藏在悬停里**：「每次都得当场点」是这颗按钮最要紧的一句，
                // 而人不会先悬停一颗按钮再按它。
                ui.weak("只管这一趟：下一趟撞上外面有人动过，照样停下来再问。");
            });
        }
        if let Some(notice) = &self.notice {
            ui.weak(notice);
        }
        要照写
    }

    /// 这一行画成哪一种样子（[`RowLook`]）。**判断全在核心里**（`Stages::next_up`、`Stages::waiting_on`、
    /// `StageRow::settled`），这里只把那几个答案折成一种画法。
    fn row_look(&self, row: &StageRow) -> RowLook {
        if self
            .stages
            .next_up()
            .is_some_and(|next| next.stage == row.stage)
        {
            RowLook::Next
        } else if self.stages.waiting_on(row).is_some() {
            RowLook::Waiting
        } else if row.settled() {
            RowLook::Done
        } else {
            RowLook::Pending
        }
    }

    /// 一道工序一行（设计稿 `.stage`）：四列——圆点、工序名、那一句（底下可能跟一行小字）、按钮。返回按下去的那一道。
    ///
    /// **列宽、圆点、竖条、字号取令牌**（`stage-columns`、`stage-dot`、`row-stripe`、`size-small-plus`、`size-caption-plus`）。
    /// 下一步那一行垫选中底色、左边一条强调色竖条（`.stage.next`）；在等的那几行名字与那一句都用弱色（`.stage.wait`）。
    /// **铺媒体那颗开关挂在导出那一行的第三列底下**：它是导出这一道**这一趟**的旋钮；导出往哪儿写不在这一段，是库屏
    /// 右边「导出设置」那一块（`roots::Screen`）。
    fn row_ui(
        &mut self,
        ui: &mut egui::Ui,
        row: &StageRow,
        site: &Site,
        tasks: &mut Tasks,
    ) -> Option<Stage> {
        let tokens = Tokens::builtin();
        let 样子 = self.row_look(row);
        let 序号 = self
            .stages
            .rows()
            .iter()
            .position(|it| it.stage == row.stage)
            .map_or(0, |at| at + 1);
        let 末行 = 序号 == self.stages.rows().len();
        let 圆角 = tokens.radius.large;
        let mut 框 = egui::Frame::new().inner_margin(crate::roots::panel_padding());
        if 样子 == RowLook::Next {
            框 = 框.fill(ui.visuals().selection.bg_fill);
            if 末行 {
                框 = 框.corner_radius(egui::CornerRadius {
                    nw: 0,
                    ne: 0,
                    sw: 圆角,
                    se: 圆角,
                });
            }
        }
        let 弱色 = ui.visuals().weak_text_color();
        let mut 按了 = None;
        let 这一行 = 框.show(ui, |ui| {
            ui.set_width(ui.available_width());
            // 列与列之间的缝取间距档位（设计稿 `.stage` 的 `gap:12px`）；第三列里主句与小字之间不另留缝。
            ui.spacing_mut().item_spacing = egui::vec2(look::step(2), 0.0);
            let [圆点列, 名字列] = tokens.layout.stage_columns;
            // **四列竖着居中**（设计稿 `.stage` 的 `align-items:center`）：第三列多高要画完才量得出，于是照上一帧量到的高摆
            // ——圆点、工序名、按钮对着这一行的高居中，第三列比圆点矮时往下垫半截；高变了就再要一帧（`request_repaint`）。
            let 量高 = egui::Id::new(("工序那一行第三列的高", row.stage.label()));
            let 字高 = ui.ctx().data(|data| data.get_temp::<f32>(量高));
            let 行高 = 字高.unwrap_or_default().max(tokens.layout.stage_dot);
            let 垫 = 字高.map_or(0.0, |高| (行高 - 高) / 2.0);
            ui.horizontal(|ui| {
                let (圆点格, _) =
                    ui.allocate_exact_size(egui::vec2(圆点列, 行高), egui::Sense::hover());
                paint_stage_dot(ui, 圆点格, 序号, 样子);
                ui.allocate_ui_with_layout(
                    egui::vec2(名字列, 行高),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_min_width(名字列);
                        let 名 = font::strong(row.stage.label());
                        ui.label(if 样子 == RowLook::Waiting {
                            名.color(弱色)
                        } else {
                            名
                        });
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    按了 = self.row_button(ui, row, 样子);
                    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                        ui.add_space(垫);
                        let 主句 = egui::RichText::new(self.stages.line(row))
                            .size(tokens.font.size_small_plus);
                        let 主句 = if 样子 == RowLook::Waiting {
                            主句.color(弱色)
                        } else {
                            主句
                        };
                        let 那一句 = ui.add(egui::Label::new(主句).wrap());
                        // **口径也挂在那个数上**：指针停在数上就读得到它数的是什么。
                        if let Some(basis) = row.stage.basis() {
                            那一句.on_hover_text(basis);
                        }
                        if let Some(小字) = self.row_detail(row, 样子) {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(小字)
                                        .size(tokens.font.size_caption_plus)
                                        .color(弱色),
                                )
                                .wrap(),
                            );
                        }
                        if row.stage == Stage::Export {
                            ui.add_space(look::step(0));
                            self.lay_media_ui(ui, site, tasks);
                        }
                        let 这一帧 = ui.min_rect().height() - 垫;
                        if 字高.is_none_or(|上一帧| (上一帧 - 这一帧).abs() > 0.5) {
                            ui.ctx().data_mut(|data| data.insert_temp(量高, 这一帧));
                            ui.ctx().request_repaint();
                        }
                    });
                });
            });
        });
        if 样子 == RowLook::Next {
            // 左边那条强调色竖条（设计稿 `.stage.next` 的 `inset 3px`）。
            let 行 = 这一行.response.rect;
            ui.painter().rect_filled(
                egui::Rect::from_min_size(
                    行.min,
                    egui::vec2(tokens.layout.row_stripe, 行.height()),
                ),
                egui::CornerRadius {
                    nw: 0,
                    ne: 0,
                    sw: if 末行 { 圆角 } else { 0 },
                    se: 0,
                },
                ui.visuals().selection.stroke.color,
            );
        }
        按了
    }

    /// 那一行底下那句小字（设计稿 `.stage .left small`）；写不出第二行的交 `None`。
    ///
    /// - **刮削**：那个数的口径——**挨着那个数画**（`romcat_core::stage::SCRAPE_BASIS`，收挂单 `Q554`；措辞照设计稿
    ///   逐字，在核心库改），在等的时候也画。
    /// - **识别**还没做完、又不在等：弹药在不在（设计稿原话，[`IDENTIFY_NEEDS_DAT`]、[`IDENTIFY_DAT_COMING`]、
    ///   [`IDENTIFY_LOCAL`]）。「查一眼有没有」不是判断（ADR-0005 修订段），与按下去时那句拒绝（[`missing_dat`]）问的是
    ///   同一件事。
    /// - 别的几句：**数与措辞都在核心库**（`Stages::detail`，挂单 `Q882`），这里只画。
    fn row_detail(&self, row: &StageRow, 样子: RowLook) -> Option<String> {
        if let Some(basis) = row.stage.basis() {
            return Some(basis.to_string());
        }
        if row.stage == Stage::Identify && matches!(样子, RowLook::Next | RowLook::Pending) {
            let 说 = if self.dat_on_board {
                IDENTIFY_DAT_COMING
            } else if missing_dat(&self.workspace).is_some() {
                IDENTIFY_NEEDS_DAT
            } else {
                IDENTIFY_LOCAL
            };
            return Some(说.to_string());
        }
        self.stages.detail(row)
    }

    /// 一道工序那一行右边那颗按钮（设计稿 `stageRows()` 的第四列，小号按钮）；按下去就交回那一道（走 [`Self::start`]，
    /// 与顶上「下一步」同一个入口）。
    ///
    /// - **在等前面那一道的不给按钮**（挂单 `Q827` 已裁：照稿）——那一趟真在台上的除外：那时按钮说「跑着呢」、按不下去。
    /// - **下一步那一行是主按钮**，字与顶上那颗同一个（[`go_label`]）；**做完的那几行是弱化的「重新扫描」「重新运行」
    ///   「重新导出」**（设计稿 `.btn.ghost`）。按钮上的字照稿（挂单 `Q881` 已裁）。
    /// - **裁决不排任务**：那一行的按钮把人带去待确认队列屏（`Self::hand_over`）。
    /// - **刮削那颗按钮说清排的是哪一趟**：旋钮是固定的整库那一套，不是浏览屏刮削面板眼下拨到哪儿的那一套
    ///   （`crate::scrape::whole_library`）。
    fn row_button(&self, ui: &mut egui::Ui, row: &StageRow, 样子: RowLook) -> Option<Stage> {
        let 忙 = self.task_of(row.stage).is_some();
        if 样子 == RowLook::Waiting && !忙 {
            return None;
        }
        let 字 = if 忙 {
            "跑着呢"
        } else {
            match (row.stage, 样子) {
                (Stage::Triage, _) => TO_QUEUE,
                (Stage::Scan, RowLook::Done) => "重新扫描",
                (Stage::Export, RowLook::Done) => "重新导出",
                (_, RowLook::Done) => "重新运行",
                _ => go_label(row.stage),
            }
        };
        let 悬停 = if row.stage == Stage::Triage {
            "裁决在待确认队列屏上一批批做，不排到任务台上".to_string()
        } else {
            let mut 悬停 = "排到任务台上跑，期间照常用别的屏；\
                          按得停——停下来留下了什么，那一趟自己会在任务台上说。"
                .to_string();
            if row.stage == Stage::Scrape {
                悬停.push('\n');
                悬停.push_str(crate::scrape::WHOLE_LIBRARY);
            }
            悬停
        };
        let 弱化字色 = ui.visuals().widgets.noninteractive.fg_stroke.color;
        look::small_buttons(ui, |ui| match 样子 {
            RowLook::Next if !忙 => {
                ui.scope(|ui| {
                    look::primary_button(ui.visuals_mut());
                    ui.add(egui::Button::new(字))
                })
                .inner
            }
            RowLook::Done if !忙 => ui.add(
                egui::Button::new(egui::RichText::new(字).color(弱化字色))
                    .frame_when_inactive(false),
            ),
            _ => ui.add_enabled(!忙, egui::Button::new(字)),
        })
        .on_hover_text(悬停)
        .clicked()
        .then_some(row.stage)
    }

    /// 顶上那一行「**下一步**」：核心库指着的那一道（`Stages::next_up`）、那一行说的话，与一颗
    /// 一按就办的按钮。返回按下去的那一道——它与那一行自己的按钮走同一个入口（[`Self::start`]）。
    ///
    /// **指哪一道不在这儿判**（ADR-0005）：「哪一道算做完了」是核心库的事，这里只画它。
    fn next_up_ui(&self, ui: &mut egui::Ui) -> Option<Stage> {
        // **字号直接问令牌**（`size-title`），不走具名字号 `look::TITLE`：开窗头一帧交进来的 `Ui`
        // 还带着装基线之前那份样式，具名字号在那一帧查不到、egui 当场 panic。
        let 标题 = |ui: &egui::Ui, text: String| {
            egui::RichText::new(text)
                .size(Tokens::builtin().font.size_title)
                .color(ui.visuals().strong_text_color())
        };
        let Some(row) = self.stages.next_up() else {
            ui.label(标题(ui, "所有工序都已完成".to_string()));
            // **哪几道只看跑没跑过由核心库照眼下那几行折**（`Stages::settled_by_running`），这里不写死。
            if let Some(只看跑没跑过) = self.stages.settled_by_running() {
                ui.weak(只看跑没跑过);
            }
            ui.weak("添加新的根之后，这里会再指出下一步。");
            return None;
        };
        let 忙 = self.task_of(row.stage).is_some();
        let 字 = if 忙 {
            "跑着呢".to_string()
        } else {
            go_label(row.stage).to_string()
        };
        // **照稿横排**（设计稿 `.nextline`）：标题与那一句在左、主按钮在右。按钮先摆（从右往左），剩下的宽度
        // 交给左边那两行，那一句长了在里面折行。
        let mut 按了 = false;
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // **主按钮**：这一屏上最该按的就是它。颜色只从令牌来（`look::primary_button`），高、留白、字号是默认那一档
                // （设计稿 `.btn.pri`，`look::buttons`）。
                按了 = look::buttons(ui, |ui| {
                    look::primary_button(ui.visuals_mut());
                    ui.add_enabled(!忙, egui::Button::new(字)).clicked()
                });
                ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    ui.label(标题(ui, format!("下一步：{}", row.stage.label())));
                    ui.add(egui::Label::new(egui::RichText::new(row.render()).weak()).wrap());
                });
            });
        });
        按了.then_some(row.stage)
    }

    /// 导出那一支上那颗**铺媒体**开关（**默认关着**，[`Self::lay_media`]），与打开之后
    /// 先画出来的那句代价（[`media_cost`]）。
    fn lay_media_ui(&mut self, ui: &mut egui::Ui, site: &Site, tasks: &mut Tasks) {
        // **离开过这一屏再回来，那个数作废**：别的屏改库的出口（裁决、合并作品、刮削面板收媒体）
        // 不经过这一段，而人要改它们就得先离开库屏（挂单 `Q654`）。认法是画帧的序号：上一次
        // 画这一段之后隔了至少一趟没画，就是离开过。
        let pass = ui.ctx().cumulative_pass_nr();
        if self.drawn_at.is_some_and(|last| pass > last + 1) {
            self.expire_media_cost();
        }
        self.drawn_at = Some(pass);
        // **那个数作废了、开关又开着，就重排一趟去算**：不每帧重算，作废之后只排这一次。
        if self.lay_media && self.media_cost == MediaCost::NotAsked {
            self.count_media(site, tasks);
        }
        let mut on = self.lay_media;
        let 拨了 = ui
            .checkbox(&mut on, LAY_MEDIA)
            .on_hover_text(
                "把媒体池里的封面、截图、视频照这个前端格式的布局铺进导出目录。\
                 默认关着：媒体池在工作目录里，主库多半在外置盘上，跨盘就是整份复制。",
            )
            .changed();
        if self.lay_media {
            let visuals = ui.visuals();
            let (颜色, 那一句) = match &self.media_cost {
                // **代价画在屏上、画得醒目**：人不会先悬停一颗开关再按导出。
                MediaCost::Counted { files, bytes } => {
                    (visuals.warn_fg_color, media_cost(*files, *bytes))
                }
                MediaCost::NotAsked | MediaCost::Counting { .. } => (
                    visuals.weak_text_color(),
                    "正在算这一趟最多要铺多少媒体（任务台上看得见它）；\
                     算出来之前导出按不下去。"
                        .to_string(),
                ),
                MediaCost::NotCounted(why) => (
                    visuals.error_fg_color,
                    format!(
                        "{why}。关掉再打开「{LAY_MEDIA}」就重算一遍；关着它，导出照常按得下去。"
                    ),
                ),
                // **没去算**：缺的那样东西补上之后自己重算，不叫人关掉再打开。
                MediaCost::Refused(why) => (
                    visuals.error_fg_color,
                    format!(
                        "这一趟最多要铺多少还算不出来：媒体的布局随前端格式不同。\
                         {why}选好之后这里自己重算。"
                    ),
                ),
            };
            let 画在 = ui.colored_label(颜色, &那一句).rect;
            scroll_in_when_new(ui, 画在, &那一句);
        }
        if 拨了 {
            self.set_lay_media(on, site, tasks);
        }
    }

    /// 库屏右边「导出设置」那一块（`roots::Screen` 摆它）：**导出**往哪个前端格式、哪个目录写。**逐字逐项照稿**（设计稿导出设置
    /// 那一块，挂单 `Q887` 已裁）：前端格式下拉（宽取令牌 `format-select-width`）、等宽的目录框、一颗「选择…」，底下一句说明
    /// （[`EXPORT_HELP`]）。
    ///
    /// **没有「记下」那颗按钮**（稿上没有）：格式一挑、目录一选（或者目录框打完字离开那一框），两样都齐了就当场记进中立库
    /// （[`Self::set_export_setup`]）——**第一次导出之前选一次，之后一键重导**（验收第 2、3 条）；**纯加键、不升结构版本**
    /// （`romcat_core::catalog::export` 的模块文档）。立不立得住照旧由核心库说（`ExportSetup::check`），立不住那句话摆在工序段上。
    pub(crate) fn export_setup_ui(&mut self, ui: &mut egui::Ui, site: &Site) {
        let tokens = Tokens::builtin();
        let mut 要记下 = false;
        let mut 选中 = None;
        ui.horizontal(|ui| {
            // **格式是从适配器名单里挑的，不是手打的**：打错一个字母的代价是一趟活白跑，
            // 而这份名单本来就是核心库交出来的（`adapter::names`）。
            let 挑之前 = self.format_draft.clone();
            egui::ComboBox::from_id_salt("前端格式")
                .width(tokens.layout.format_select_width)
                .selected_text(if self.format_draft.is_empty() {
                    "选一个前端格式"
                } else {
                    &self.format_draft
                })
                .show_ui(ui, |ui| {
                    for name in romcat_core::adapter::names() {
                        ui.selectable_value(&mut self.format_draft, name.to_string(), name);
                    }
                });
            要记下 |= self.format_draft != 挑之前;
            // **目录框用这一栏剩下的宽度**：「选择…」那一颗的宽先留出来，余下的都给目录框——写死一个宽度会把按钮挤出这一栏。
            let 留给按钮 = look::button_width(ui, "选择…") + ui.spacing().item_spacing.x;
            let 框宽 = (ui.available_width() - 留给按钮).max(0.0);
            let 框 = look::text_input(
                ui,
                框宽,
                egui::TextEdit::singleline(&mut self.out_draft)
                    .hint_text("导出到哪个目录")
                    .font(egui::TextStyle::Monospace),
            );
            要记下 |= 框.lost_focus()
                && self
                    .setup
                    .as_ref()
                    .is_none_or(|setup| setup.out.to_string_lossy() != self.out_draft);
            // 系统目录选择器（`crate::pick`）：选中的目录落进左边这个框，与贴路径同一处。默认那一档按钮（设计稿 `.btn`，
            // `look::buttons`）。
            if look::buttons(ui, |ui| ui.button("选择…").clicked()) {
                选中 = crate::pick::directory("选导出目录", Path::new(&self.out_draft));
            }
        });
        look::help(ui, EXPORT_HELP);
        if let Some(why) = &self.setup_unreadable {
            // **不许画成「还没选过」**：那是一件该去做的事，而这是一件该去查的事。
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("导出那套配置读不出来：{why}。这不是「还没选过」。"),
            );
        }
        if 选中.is_some() {
            self.picked_export_dir(site, 选中);
        } else if 要记下 {
            self.save_export_drafts(site);
        }
    }

    /// 导出设置那一块「选择…」弹的系统选目录窗口交回来的那一个（`crate::pick::directory`）：选中了就填进目录框，格式也挑过了
    /// 就当场记下。交回 `None`（取消了，或者弹不出来）什么都不动——目录框还在，贴路径照旧。
    ///
    /// **测试从这儿递路径进去**：系统窗口测试点不了，选中之后那一半照 `tests/pick.rs` 的做法验。
    pub fn picked_export_dir(&mut self, site: &Site, picked: Option<PathBuf>) {
        let Some(path) = picked else {
            return;
        };
        self.out_draft = path.to_string_lossy().into_owned();
        self.save_export_drafts(site);
    }

    /// 格式与目录两格都有字时记下（[`Self::set_export_setup`]）；有一格空着就先不记——人还没挑完。
    fn save_export_drafts(&mut self, site: &Site) {
        if self.format_draft.trim().is_empty() || self.out_draft.trim().is_empty() {
            return;
        }
        let (format, out) = (self.format_draft.clone(), self.out_draft.clone());
        self.set_export_setup(site, &format, &out);
    }
}

/// 后台那条线程真跑的那一趟。
///
/// **装配全在这儿**：DAT 库、沉淀库、剥离规则、中文离线源、TitleID 索引。领域判断一条
/// 都不在这一层——它只是把核心库要的原料摆齐（与命令行 `romcat identify` 不带 `--model`
/// 摆的原料是同一副；**模型推断**那一层只用库里已经问过的答案、一个请求都不发，比命令行
/// 少装价目表与念计划的回调两样，见 [`model_layer`]）。
fn run(
    stage: Stage,
    knobs: ExportKnobs,
    site: &mut Site,
    workspace: &Path,
    task: &Handle,
) -> Result<Product, Cutoff> {
    match stage {
        Stage::Identify => identify_run(site, workspace, task),
        Stage::Scrape => scrape_run(site, workspace, task),
        Stage::FoldTitles => fold_titles_run(site, workspace, task),
        Stage::Export => export_run(site, workspace, knobs, task),
        Stage::Scan | Stage::Triage => Err(Cutoff::failed(format!(
            "{}不排到任务台上：扫描从库屏根那一块排，裁决在待确认队列屏上做",
            stage.label()
        ))),
    }
}

/// 工序那一行眼下画成哪一种样子（设计稿 `stageRows()` 的 `st`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowLook {
    /// 做完了（`.stage.done`）：绿圆点打勾，弱化的「重新…」。
    Done,
    /// 顶上「下一步」指着的那一道（`.stage.next`）：垫选中底色、左边一条强调色竖条、实心圆点、主按钮。
    Next,
    /// 在等前面那一道（`.stage.wait`）：名字与那一句都用弱色，不给按钮。
    Waiting,
    /// 没做完、不在等谁、又不是下一步：六道从上往下排走不到这儿，照平常的样子画。
    Pending,
}

/// 工序那一行头上那枚**圆点**（设计稿 `.stage .dot`）：直径、描边宽取令牌 `stage-dot` / `stage-dot-stroke`，序号用
/// 角标字号的粗体（拉丁数字有粗体）。
///
/// 做完的是 `hi-soft` 底、`hi` 描边与对勾；下一步是强调色实心、`on-accent` 的序号；别的是面板底、`line-2` 描边、弱色
/// 序号。**对勾是画的两段线，不是字形**：打包的字形子集里没有 ✓。对勾三个点按圆的半径摆。
fn paint_stage_dot(ui: &egui::Ui, 格: egui::Rect, 序号: usize, 样子: RowLook) {
    let tokens = Tokens::builtin();
    let visuals = ui.visuals();
    let 调色板 = tokens
        .color
        .theme(egui::Theme::from_dark_mode(visuals.dark_mode));
    let 半径 = tokens.layout.stage_dot / 2.0;
    let 描边宽 = tokens.layout.stage_dot_stroke;
    let 圆心 = egui::pos2(格.left() + 半径, 格.center().y);
    let painter = ui.painter();
    let 序号字 = |色: egui::Color32| {
        painter.text(
            圆心,
            egui::Align2::CENTER_CENTER,
            序号.to_string(),
            egui::FontId::new(tokens.font.size_caption, font::strong_family()),
            色,
        );
    };
    match 样子 {
        RowLook::Done => {
            let (绿, 浅绿) = look::tone_colors(look::Tone::Good, visuals);
            let 笔 = egui::Stroke::new(描边宽, 绿);
            painter.circle(圆心, 半径 - 描边宽 / 2.0, 浅绿, 笔);
            let 左 = 圆心 + egui::vec2(-半径 * 0.45, 0.0);
            let 底 = 圆心 + egui::vec2(-半径 * 0.1, 半径 * 0.35);
            let 右 = 圆心 + egui::vec2(半径 * 0.45, -半径 * 0.35);
            painter.line_segment([左, 底], 笔);
            painter.line_segment([底, 右], 笔);
        }
        RowLook::Next => {
            painter.circle_filled(圆心, 半径, 调色板.accent);
            序号字(调色板.on_accent);
        }
        RowLook::Waiting | RowLook::Pending => {
            painter.circle(
                圆心,
                半径 - 描边宽 / 2.0,
                visuals.window_fill,
                egui::Stroke::new(描边宽, visuals.widgets.inactive.bg_stroke.color),
            );
            序号字(visuals.weak_text_color());
        }
    }
}

/// 下一步那一行、与顶上「下一步」那颗按钮上写的字（设计稿 `stageRows()` 的 `act`，挂单 `Q881` 已裁：照稿）：扫描写
/// 「开始扫描」，裁决写 [`TO_QUEUE`]，别的几道写「运行」。
///
/// **两处同一个字**：顶上已经写着「下一步：某某」，按钮不必再带工序名。
fn go_label(stage: Stage) -> &'static str {
    match stage {
        Stage::Scan => "开始扫描",
        Stage::Triage => TO_QUEUE,
        Stage::Identify | Stage::Scrape | Stage::FoldTitles | Stage::Export => "运行",
    }
}

/// **导出**那一支这一趟带哪几个旋钮。别的几支不看它。
///
/// **每排一趟现折一份**（`Section::export_knobs`），照写那一趟只在上面再压一格；
/// **照写那一格从不记在 [`Section`] 上**（`Section::force_export`）。
///
/// 给导出那一支再加开关：往这儿加一格，在 `Section::export_knobs` 里从这一段读出来——
/// `start` 与 `force_export` 一行不用改，照写那一趟也带着同一个开关。铺媒体那一格就是
/// 这么加的（票 `one-criterion-per-thing/09`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ExportKnobs {
    /// 外面有人动过也照写（[`ExportOptions::force`]）。**默认关**。
    force: bool,
    /// 把**媒体池**里的媒体一起铺进导出目录（[`ExportOptions::media`]）。**默认关**，
    /// 从这一段那颗开关读（[`LAY_MEDIA`]）。
    media: bool,
}

/// 开关底下那一句**一换就把它滚进视口，只管换了之后那几趟**。
///
/// 那一句摆在库屏最底下（导出那一行、「一起铺媒体」开关之下）。每一屏底下加了状态栏、按钮照稿
/// 加高之后（票 `gui-looks-like-the-design/25`、`/05`），它常常落在视口之外——开了开关、代价也
/// 算出来了，人却看不见那句「最多要铺多少」，而那句话存在的全部理由就是按导出之前被看见。
///
/// 与子库屏摊开差量表同一个办法（`sublibrary.rs` 那一段）：**只管那句话换了之后那几趟**，过了就
/// 不再管，人往哪儿滚都随他。**认的是那句话本身，不是屏上的高度**：库屏怎么重排，这一条都不跟着改。
/// 滚了就请 egui 这一帧再摆一趟（`request_discard`），多数时候换过的那一句当帧就画在视口里；一帧最多摆
/// `max_passes` 趟，不会一直重摆。**开窗头一帧例外**：egui 那一帧本来就摆两趟，那一句在头一趟里还在
/// 视口里、第二趟控件量好了尺寸才被挤到底下，这时已经没有下一趟可摆，滚动落在下一帧（实测：头一趟
/// y 634、第二趟 y 770，视口底 762）。
fn scroll_in_when_new(ui: &egui::Ui, 画在: egui::Rect, 那一句: &str) {
    let 记号 = egui::Id::new("铺媒体开关底下那一句从第几趟起");
    let 这一趟 = ui.ctx().cumulative_pass_nr();
    let 起 = ui.ctx().data_mut(|data| {
        let 记的 = data.get_temp_mut_or_insert_with(记号, || (那一句.to_string(), 这一趟));
        if 记的.0 != 那一句 {
            *记的 = (那一句.to_string(), 这一趟);
        }
        记的.1
    });
    let 那几趟 = u64::try_from(ui.ctx().options(|options| options.max_passes.get())).unwrap_or(1);
    if 这一趟 <= 起 + 那几趟 && !ui.clip_rect().contains_rect(画在) {
        ui.scroll_to_rect_animation(
            画在,
            Some(egui::Align::BOTTOM),
            egui::style::ScrollAnimation::none(),
        );
        ui.ctx().request_discard("铺媒体开关底下那一句滚进视口");
    }
}

/// 还没有 DAT 库时那句话：为什么不行、去哪儿取；有就是 `None`。
///
/// **不是判断，是查一眼有没有**（ADR-0005 修订段「原料还没备齐」），而那句话要指向屏上
/// 的哪一处——那正是核心库不该知道的东西，所以它留在这一层。
///
/// **两处认它、说同一句**：按下去那一刻（`Section::refusal`，那时不排、只在屏上说），
/// 与那一趟开跑时（[`identify_run`] 兜底）。
fn missing_dat(workspace: &Path) -> Option<String> {
    (!workspace::dat_repo_path(workspace).exists()).then(|| {
        "还没有 DAT 库。先在「数据源」那一块把它下载下来——没有弹药就没有命中率。".to_string()
    })
}

/// 一个根都还没有时按扫描，屏上那一句：为什么不行、去哪儿加。
const NO_ROOTS: &str = "这个库一个根都还没有，扫描无从下手。先按右上角「添加根…」选一个目录。";

/// 一个根都还没有时按扫描那句话；有根就是 `None`。
///
/// **不是判断，是查一眼有没有**（ADR-0005 修订段「原料还没备齐」），与 [`missing_dat`] 同形。
/// **读不动库交 `None`**：那不是缺一样东西，是一件该去查的事（同 [`export_refusal`]）——库屏那一侧照它
/// 手上那几个根排，一个都没有就什么都不排，而那份库读不动的话库屏自己已经说过了。
fn no_roots(catalog: &Catalog) -> Option<String> {
    catalog
        .roots()
        .ok()
        .filter(Vec::is_empty)
        .map(|_| NO_ROOTS.to_string())
}

/// 跑一趟**识别**。
fn identify_run(site: &mut Site, workspace: &Path, task: &Handle) -> Result<Product, Cutoff> {
    // **没有弹药就没有命中率**：DAT 库不在时直说。偷偷让 `DatRepo::open` 当场建出一份
    // 空库跑下去的话，整库都会落成「未命中」并写进中立库——那是一条**假结论**，
    // 不是一次失败。命令行开头拦的是同一件事，但两处各说各的话（那一句指的是
    // `romcat dat sync`，这一句指的是库屏「数据源」那一块）——挂单 `Q423`。
    //
    // **按下去那一刻已经问过一遍**（`Section::refusal`：不排，只在屏上说）。这里再问一遍是
    // 兜底——排上去之后、轮到它之前那份库被挪走了。那是真跑起来才撞上的，照实记失败；
    // 少了这一问，`DatRepo::open` 会当场建出一份空库。
    if let Some(why) = missing_dat(workspace) {
        return Err(Cutoff::failed(why));
    }
    let dat = workspace::dat_repo_path(workspace);
    let repo = romcat_core::dat::DatRepo::open(&dat)
        .map_err(|error| Cutoff::failed(format!("DAT 库打不开：{error}")))?;
    // **沉淀库先说话**：裁决过的内容直接精确命中，不再进队列（ADR-0008）。
    let verdicts = verdict::Index::load(&site.store, &site.library_identity)
        .map_err(|error| Cutoff::failed(format!("沉淀库读不动：{error}")))?;
    // **名字那一层的原料只有一处**（[`crate::scrape::NamingParts`]）：刮削与识别摆的是
    // 同一副，两条路各开一遍的话，同一个变体在两条路上会撞到不同的条目——而那是写进
    // 库里的结论，不是显示上的差别。
    let parts = crate::scrape::NamingParts::open(workspace, task)?;
    let naming = parts.naming();
    // **第三方 TitleID 索引没取过照样跑**：容器的明文文件名表免密钥就说得出 TitleID，
    // 查表只是把结论从「哪个游戏」抬到「哪个游戏的哪个版本」。
    let titledb =
        romcat_core::titledb::store::Store::open(&workspace::titledb_store_path(workspace))
            .ok()
            .filter(|store| store.ready().unwrap_or(false));
    // 主库那一组根：库里记着的那份。**盘不在位不拦着**——那时回盘读不到的那几份落成
    // 「无判据」，与命令行一个口径（读不到不是结论，ADR-0021）。
    let roots = Roots::load(&site.catalog)
        .map_err(|error| Cutoff::failed(format!("这份中立库读不动：{error}")))?;
    // **库里已经问过的答案照旧折成候选**（挂单 `Q418`）：那是中立库里唯一花过钱的一张表。
    // 整份读回来的写法与命令行一样（`Catalog::model_answers` → `Answers::build`）。
    let answers = Answers::build(
        site.catalog
            .model_answers()
            .map_err(|error| Cutoff::failed(format!("问过的答案读不动：{error}")))?,
    );
    // **平台纠正**：人在库屏上按组定过的那些「按内容改 / 保持目录的说法」（票
    // `gui-looks-like-the-design/28`）。判「这个变体按哪个平台算」时它先说话
    // （`identify::platform_of`）——这一趟不读它的话，人定完再跑一趟识别等于没定。
    let mut options = identify::Options::new(roots);
    let corrections = site
        .store
        .platform_corrections(&site.library_identity)
        .map_err(|error| Cutoff::failed(format!("沉淀库里的平台纠正读不动：{error}")))?;
    if !corrections.is_empty() {
        options.decided_platforms = Some(identify::DecidedPlatforms::new(
            &romcat_core::platform::Manifest::builtin(),
            &corrections,
        ));
    }
    identify::run_task(
        &RealFs::new(),
        &mut site.catalog,
        &identify::Ammo {
            repo: &repo,
            verdicts: &verdicts,
            naming: &naming,
            guessing: &model_layer(&answers),
            titledb: titledb.as_ref(),
        },
        &options,
        task,
    )
    .map(|outcome| Product::Identified(Box::new(outcome)))
    .map_err(|error| Cutoff::failed(format!("识别失败：{error}")))
}

/// 识别的**模型推断**那一层：**只用中立库里已经问过的答案**，一个请求都不发（挂单 `Q418`）。
///
/// 不用那些答案的话，人得回终端跑一趟 `romcat identify` 才捡得回自己付过的钱。**照命令行
/// 那样用字面量建**：核心库那个装配结构体的字段全部公开，不为界面另长一个构造器。与命令行
/// 不带 `--model` 那一副比，这里少装价目表与念计划的回调两样（见下）。
///
/// ## 四样一起钉死
///
/// - **不给凭据**（`net: None`）：一个请求都不发。缓存命中在核心库的主循环里、与网络无关；
///   发请求那一层一见没有网络句柄就返回。
/// - **不排计划**（`planning: false`）。
/// - **不装念计划的回调**（`announce: None`）：界面这一路从不印一份计划。
/// - **价目表不装，价钱一律传零**。命令行「价目表里查不到就不启动」那道拦，拦的是**印一份
///   零价的计划**；这一路既不排、也不念，零价到不了任何人眼前（回执只数候选）。日后真要在
///   界面上开推断，那时再装价目表——那时它才有意义。
///
/// ## 提问指纹为什么对得上命令行问过的那些
///
/// 答案按**提问指纹**存，指纹里有模型名与上限里会改变答复的那几档（每条要几个候选、力度、
/// 输出上限，`Limits::ask_fingerprint`）。这里取默认那个模型与 `Limits::default()`——命令行
/// `--model-id`、`--model-guesses`、`--model-effort`、`--model-max-tokens` 的默认值指的正是
/// 同一组核心库常量（花费上限与请求间隔在命令行上是字面量，但它们不进指纹），于是命令行
/// 不拨旋钮时问过的答案，这一趟一条不落地命中。**拨过那几档问出来的答案对不上**，界面上
/// 用不到（挂单 `Q751`）。
fn model_layer(answers: &Answers) -> Guessing<'_> {
    Guessing {
        answers,
        net: None,
        planning: false,
        announce: None,
        model: DEFAULT_MODEL.to_string(),
        price: Price {
            input_per_mtok: 0,
            output_per_mtok: 0,
        },
        checked: String::new(),
        limits: Limits::default(),
    }
}

/// 识别回执里「几条候选来自已经问过的答案」那一行；这份库上没有模型推断的候选时不说。
///
/// **数的是整份库**（识别报告里按数据源分的那一行），不是这一趟现折的那几条：接着上一趟算
/// 的那一趟不重算已经有候选的变体（`identify::run_task`），只数这一趟会少报。界面这一路
/// 一个请求都不发（[`model_layer`]），所以库里模型推断那一层的候选全部来自已经问过的答案。
/// **只数候选，不提计划与花费**——这一路价钱传的是零，印出来就是一句假话。
fn paid_answers_line(outcome: &identify::Outcome) -> Option<String> {
    let row = outcome
        .report
        .sources
        .iter()
        .find(|row| row.source == identify::model::SOURCE)?;
    (row.candidates > 0).then(|| {
        format!(
            "{} 条候选来自已经问过的答案（{} 个变体），这一趟一个请求都没发。",
            thousands(row.candidates),
            thousands(row.variants),
        )
    })
}

/// 跑一趟**刮削**：整库、全部字段、只用本地源、不收媒体、补缺——**一个请求都不发**。
///
/// **装配只有一处**（`crate::scrape::run`）：浏览屏刮削面板排的那一趟走的是同一个函数，
/// 这一支只把选项换成整库那一套（`crate::scrape::whole_library`，旋钮为什么是那一套
/// 写在那儿）。
fn scrape_run(site: &mut Site, workspace: &Path, task: &Handle) -> Result<Product, Cutoff> {
    let options = crate::scrape::whole_library(&site.catalog, workspace)
        .map_err(|error| Cutoff::failed(format!("这份中立库读不动：{error}")))?;
    crate::scrape::run(site, workspace, &options, None, task)
}

/// 跑一趟**折标题**：把识别与刮削的结论折成每个作品的**标题集合**。
///
/// **一个字节都不读主库、一个请求都不发**（ADR-0001）：要的东西全在中立库里躺着。
/// 装配只有两样——**优先级表**与**沉淀库**，与命令行 `romcat titles` 摆的是同一副。
///
/// ## 停下的地方只有开折之前那一处
///
/// 重折是「把折出来的那批清掉再写回去」（[`title::refold`]），**中间停下等于把整份
/// 标题集合丢掉**。所以这一段在开折之前 `?` 一下把手，之后一步都不看停下的信号：
/// 那一折在真库上是秒级的事（`romcat titles` 整趟不到 3 秒，`docs/library-facts.md`）,
/// 等它走完比留下一份空集合便宜得多。于是这一趟**要么写完、要么一个字节都没写**
/// ——被按停的那一趟走的是「停了」，不是**停在半路**。
///
/// ## 优先级表读不出来就停下，不退回内置那份
///
/// 挑**显示标题**用的是同一份表，而工作目录里那份 `priorities.toml` 正是人改过的
/// 说法。悄悄退回内置那份的话，人会看见一份自己没定过的显示标题，还查不出为什么。
fn fold_titles_run(site: &mut Site, workspace: &Path, task: &Handle) -> Result<Product, Cutoff> {
    // 装配那一步加上核心库自己那几步。**核心库那个数由它自己报**
    // （`title::TASK_STEPS`）——在这儿手写一个 3，那边加一步这儿的进度条就走过头了。
    task.steps(1 + title::TASK_STEPS);
    // **「被按停了」一个字都不用凑**：`?` 一下把手，`Halted` 自己折成 `Cutoff::Halted`，
    // 任务台按支记成「停了」（`Cutoff` 的文档）。底下 `run_task` 交上来的那一支同理，
    // 折它的是 `From<FoldTitlesError> for Cutoff`。
    task.step("读优先级表")?;
    let priorities = romcat_core::sync::prepare::priorities(None, workspace)?;
    // **能停到哪儿、压掉的叫法怎么筛，全在核心库那一段**（`title::run_task`）：
    // 这一层只把料摆齐、把把手递进去（ADR-0005）。
    let report = title::run_task(&mut site.catalog, &site.store, &priorities, task)?;
    Ok(Product::Titled(Box::new(report)))
}

/// 跑一趟**导出**：把中立库写成前端能读的元数据，铺在导出目录里。
///
/// **一个字节都不读主库、一个 ROM 都不搬**（ADR-0004、验收第 6 条）：要的东西全在
/// 中立库里躺着，写的只有元数据文件——开着**铺媒体**时再加上媒体目录里那几份。
/// 装配只有三样——记住的那套**配置**、**优先级表**与工作目录里那个**媒体池**，与命令行
/// `romcat export` 摆的是同一副。
///
/// ## 命令行那三个旋钮，界面上给两个、不给一个
///
/// - **只排计划**（`--dry-run`）**不给**，钉死在「关」上：工序段那一行问的是「还差多少」，
///   一次不写盘的排计划答不了它，反倒会让那一行说「上次跑是刚刚」而盘上什么都没有。
/// - **照写**（`--force`）**默认关**，只从 `knobs` 那一格来：撞上外面有人动过时这一趟
///   停下来、逐份点名，人自己去看过那几份文件，再在屏上按「我看过了，照写」重排一趟
///   （`Section::force_export`）。它丢掉的是一次手改，所以**每次都得当场点**，不记住。
/// - **铺媒体**（`--media`）**默认关**，只从 `knobs` 那一格来（[`LAY_MEDIA`]）。与照写不同，
///   它记在这一段上、跨趟不变：它不丢掉任何东西，要付的是时间与盘——那句代价按下之前
///   就画在屏上（[`media_cost`]）。
fn export_run(
    site: &mut Site,
    workspace: &Path,
    knobs: ExportKnobs,
    task: &Handle,
) -> Result<Product, Cutoff> {
    // **这一趟的选项开工时就定下**，总步数照它报（`ExportOptions::task_steps`）：开着铺媒体
    // 那一趟核心库多走两步，照 `transfer::TASK_STEPS` 手写的话进度条会走过头。导出目录要
    // 读出配置才知道，读出来再填进去。
    let mut options = ExportOptions {
        out: PathBuf::new(),
        dry_run: false,
        force: knobs.force,
        // **铺媒体只从 `knobs` 那一格来**，默认关：关着时这一趟与没有这颗开关时一样。
        media: knobs.media.then(|| media_pool(workspace)),
    };
    // 装配那两步加上核心库自己那几步。
    task.steps(2 + options.task_steps());
    task.step("读导出的配置")?;
    let (setup, adapter) = export_setup_of(&site.catalog)?;
    options.out = setup.out;
    // **优先级表读不出来就停下，不退回内置那份**：挑**显示标题**用的是同一份表，
    // 而工作目录里那份 `priorities.toml` 正是人改过的说法。悄悄退回内置那份的话，
    // 导出去的名字会与他定过的对不上，还查不出为什么（与折标题那一支同一条）。
    task.step("读优先级表")?;
    let priorities = romcat_core::sync::prepare::priorities(None, workspace)?;
    // **能停到哪儿、撞上手改怎么办，全在核心库那一段**（`transfer::export_task`）：
    // 这一层只把料摆齐、把把手递进去（ADR-0005）。
    let report = transfer::export_task(
        &mut site.catalog,
        adapter.as_ref(),
        &priorities,
        &options,
        task,
    )?;
    Ok(Product::Exported(Box::new(report)))
}

/// 还没选过导出的前端格式与目录时那句话：缺什么、去哪儿选。
///
/// **没选过就如实拒绝**，不替人挑一个格式与目录：挑错一个目录就是往别人的盘上写一堆文件。
/// 这一句与库屏「导出设置」那一块的空态说的是同一件事（挂单 `Q437`）。
const EXPORT_NOT_CHOSEN: &str =
    "还没选过导出的前端格式与目录。先在「导出设置」那一块选一次，选完记进这份库。";

/// 记着的那个前端格式这一版没有适配器时那句话：**为什么在核心里**（`ExportSetup::adapter`
/// 的 `Display`，ADR-0005），这一层只在后面补上去哪儿重选。
fn no_adapter(error: &romcat_core::catalog::ExportSetupError) -> String {
    format!("{error}先在「导出设置」那一块重选一次。")
}

/// **导出**那一支按下去之前就判得出的那句拒绝；前提都在就是 `None`。
///
/// 两样：**没选过**（库里那两个键在不在，查一眼就知道），与**记着的格式这一版没有适配器**
/// （判据在核心里，[`ExportSetup::adapter`]）。导出那一趟与算要铺多少那一趟问的都是它
/// （`Section::refusal`、`Section::count_media`）——媒体的布局随前端格式不同，没选过就算不出来。
///
/// **库读不动不在这里，交 `None`**：那不是缺一样东西，是一件该去查的事。排上去，那一趟在
/// [`export_setup_of`] 撞上它、照实记失败（票 `gui-looks-like-the-design/07`：分开的是
/// 压根没开跑与跑了没成）。
fn export_refusal(catalog: &Catalog) -> Option<String> {
    match catalog.export_setup() {
        Ok(None) => Some(EXPORT_NOT_CHOSEN.to_string()),
        Ok(Some(setup)) => setup.adapter().err().map(|error| no_adapter(&error)),
        Err(_) => None,
    }
}

/// 读出记住的那套**导出**配置与它的**适配器**。
///
/// 导出那一趟（[`export_run`]）与算要铺多少那一趟（[`media_cost_run`]）摆的是同一副料，
/// **同一句拒绝只写在这儿与 [`export_refusal`]**，两处说的是同一句（[`EXPORT_NOT_CHOSEN`]、
/// [`no_adapter`]）。
fn export_setup_of(
    catalog: &Catalog,
) -> Result<(ExportSetup, Box<dyn romcat_core::adapter::Adapter>), Cutoff> {
    let setup = catalog
        .export_setup()
        .map_err(|error| Cutoff::failed(format!("这份中立库读不动：{error}")))?
        // **按下去那一刻已经问过一遍**（[`export_refusal`]：不排，只在屏上说）。走到这儿还是
        // 没选过，是排上去之后、轮到它之前那两个键被抹掉了——真跑起来才撞上的，照实记失败。
        .ok_or_else(|| Cutoff::failed(EXPORT_NOT_CHOSEN))?;
    // **找不到那个格式该说哪句话在核心里**（`ExportSetup::adapter`，ADR-0005）。兜底同上。
    let adapter = setup
        .adapter()
        .map_err(|error| Cutoff::failed(no_adapter(&error)))?;
    Ok((setup, adapter))
}

/// 工作目录里那个**媒体池**，**一个目录都不建**（[`MediaPool::at`]）：算要铺多少那一趟
/// 整条只读，池不在时照实算出零份。导出那一趟铺的也从它取——两趟指的是同一个池，与命令行
/// `romcat export --media` 指的也是同一个。
fn media_pool(workspace: &Path) -> MediaPool {
    MediaPool::at(&workspace::media_pool_dir(workspace))
}

/// 算一遍开着**铺媒体**时这一趟**最多**要铺多少：几份、共多少字节（[`media_cost`]）。
///
/// **算法一行都不在这里**：数是核心库那一处交的（[`transfer::media_to_lay`]，ADR-0024），
/// 导出那一趟真铺的也是同一份。这一层只把料摆齐——记住的那套配置（布局随前端格式不同）
/// 与工作目录里那个媒体池。整条只读：停在哪儿都什么都没留下。
fn media_cost_run(catalog: &Catalog, workspace: &Path, task: &Handle) -> Result<Product, Cutoff> {
    task.steps(2);
    task.step("读导出的配置")?;
    let (_, adapter) = export_setup_of(catalog)?;
    task.step("折媒体的落点")?;
    let laid = transfer::media_to_lay(catalog, adapter.as_ref(), &media_pool(workspace))
        .map_err(|error| Cutoff::failed(format!("这份中立库读不动：{error}")))?;
    Ok(Product::MediaCounted {
        files: laid.files.len() as u64,
        bytes: laid.bytes(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一份**落在盘上**的空库，选好了 Pegasus 与导出目录。返回工作目录（得活到测试结束）与现场。
    fn 选好导出的空库(tag: &str) -> (romcat_core::testing::TempDir, Site) {
        let 工作区 = romcat_core::testing::temp_dir(tag);
        let 库文件 = 工作区.path().join("catalog").join("fixture.sqlite3");
        drop(Catalog::create(&库文件, "fixture").expect("能建中立库"));
        let site = Site::open_file(工作区.path(), &库文件, None).expect("开得出现场");
        let 导出去 = 工作区.path().join("导出去");
        let setup =
            ExportSetup::check("Pegasus", &导出去.to_string_lossy()).expect("有 Pegasus 这个格式");
        site.catalog.set_export_setup(&setup).expect("记得下");
        (工作区, site)
    }

    #[test]
    fn 识别那一层只用已经问过的答案_不给凭据_不排计划_不装念计划的回调_价钱传零() {
        // 挂单 `Q418` 那几样**一起**钉死，少一样这一路就不对：手里有网络句柄就会发请求；
        // 排了计划、装了念计划的回调，就会有一份零价的计划被念给人听。
        // **价目表一处都没读**：价钱是零、核实日期是空的——那两样只有价目表给得出。
        let answers = Answers::default();
        let layer = model_layer(&answers);
        assert!(
            layer.net.is_none() && !layer.asking(),
            "界面这一路手里有网络句柄"
        );
        assert!(!layer.planning, "界面这一路排了计划");
        assert!(layer.announce.is_none(), "界面这一路装了念计划的回调");
        assert_eq!(
            layer.price,
            Price {
                input_per_mtok: 0,
                output_per_mtok: 0,
            },
            "界面这一路的价钱不是零",
        );
        assert!(
            layer.checked.is_empty(),
            "界面这一路带着一个价目表核实日期：{}",
            layer.checked,
        );
    }

    #[test]
    fn 导出那一趟声明的总步数与真走过的对得上_开着铺媒体也不走过头() {
        // 票 `one-criterion-per-thing/09` 验收第 4 条「报得出跑到哪儿」：任务屏的进度条照
        // `Handle::steps` 声明的总数画。开着铺媒体那一趟核心库多走两步
        // （`ExportOptions::task_steps`），照不开时的数声明的话，进度条会走过头。
        //
        // 空库也走得完每一步（没有要写的、也没有要铺的），于是这一条不必摆一整份 fixture。
        for media in [false, true] {
            let (工作区, mut site) = 选好导出的空库("gui-stages-导出步数");
            let 把手 = Handle::new();
            export_run(
                &mut site,
                工作区.path(),
                ExportKnobs {
                    force: false,
                    media,
                },
                &把手,
            )
            .expect("空库也导得完");
            let 进度 = 把手.progress();
            assert_eq!(
                进度.at, 进度.steps,
                "开着铺媒体={media}：走过的步数与声明的对不上：{进度:?}",
            );
            let 最后一步 = if media {
                "把媒体铺出去"
            } else {
                "逐份写出去"
            };
            assert_eq!(
                进度.step, 最后一步,
                "开着铺媒体={media}：最后停在的那一步说错了",
            );
        }
    }
}
