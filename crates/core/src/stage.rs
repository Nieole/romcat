//! **工序进度**：一次问出这个库每一道工序**还差多少**。
//!
//! ## 为什么收成一处
//!
//! **工序**是一个库要走的那几道步骤（`CONTEXT.md` 的**工序**条）：扫描、识别、刮削、
//! 折标题、裁决、**导出**。库屏上一道工序一行，而那一行要说的是**还差多少**，
//! 不是「上次几点跑的」——加了一块盘重扫之后那个数自己就涨上去，人不必再自己推理
//! 「是不是该重跑识别了」，而时间戳答不了这个问题。
//!
//! 三道工序的度量散着挂的话，界面要调三个地方、测三处。收成一处就是**一个接缝、
//! 一个测试文件**：[`Stages::survey`] 一趟问完，界面那一层只画。
//!
//! ## 「这一支交不出数」是常态，不是意外
//!
//! 识别那一支现成（[`Catalog::not_run_count`]），另外两支要新造，而它们的度量可能贵到
//! 算不动（比如要全表比对）。所以**每一行各自说得出自己是报数还是退回时刻**
//! （[`Behind`] 那两支），**退路只对单支生效**：一支退了不许把现成的识别那一支也降级
//! ——它明明数得出来。
//!
//! **折标题与导出两支就是走退路的那两支**，理由与实测代价各写在
//! [`Stage::FoldTitles`] 与 [`Stage::Export`] 上。它们各自把「上次跑是几点」那条数据
//! 通路带了进来（[`Catalog::titles_folded_at`]、[`Catalog::exported_at`]）
//! ——退回时刻的那一行没有它就画不出时刻。
//!
//! 这也是 [`Stages::survey`] **不返回 `Result`** 的理由：某一支读不动库时，它降级的是
//! 那一行，不是整段。整段失败的话，一个算不出来的度量会让库屏上连识别那一行都消失。
//!
//! ## 领域判断在这儿，措辞也在这儿
//!
//! 那一行画出来的是哪句话由 [`StageRow::render`] 折（ADR-0005：界面只画、只转发）。
//! 识别那一句与**待确认队列**屏、与命令行 `triage list` 印的**是同一个数**
//! ——三处都取 [`Catalog::not_run_count`]，没有第二份算法。

use crate::catalog::identify::IdentifyTally;
use crate::catalog::roots::LibraryTotals;
use crate::catalog::title::TitleTally;
use crate::catalog::{Catalog, ExportSetup};
use crate::path;
use crate::report::{human_bytes, human_time, thousands};
use crate::triage;
use crate::triage::batch::Coverage;
use crate::verdict::{self, Store};

/// 一道**工序**。
///
/// **眼下是扫描、识别、刮削、整理标题、裁决与导出六支**，次序照设计稿
/// （`.scratch/gui-looks-like-the-design/prototype.html` 的 `stageRows()`）。
/// **加一支要写五处，最后一处不在这个 crate 里**：
///
/// 1. 这个枚举一个变体，加 [`Stage::label`] 与 [`Stage::prerequisite`] 那两个 `match` 各一支；
/// 2. [`Stage::ALL`] 一项——[`Stages::survey`] 照它走，漏了就整支不出现；
/// 3. 一个折得出 [`StageRow`] 的函数，挂进 `row_of` 那个 `match`；
/// 4. [`StageRow::render`] 那个 `match` 一支（说不出还差多少时走
///    [`Behind::Unmeasured`]，那一支与工序无关，不必动）；
/// 5. **界面那一侧**：`romcat_gui::stages` 里认工序的那几个 `match`——`Section::start`（这一支排一趟
///    什么活上任务台，或者像扫描、裁决那样交给够得着的那一处：扫描交库屏自己那条扫描的路，裁决换到
///    待确认队列屏）、`Section::refusal`（按下去之前就判得出的那句拒绝）、`run`（后台那条线程跑哪一个
///    长入口）与 `go_label`（顶上「下一步」那颗按钮上的字）；以及 `romcat_gui::app::App::poll_tasks`
///    里那个 `match`（跑完之后还有哪一屏要重读）。它们都是穷尽匹配，漏一处编译器当场点名。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    /// **扫描**：遍历主库的一组**根**，把盘上有什么收进中立库
    /// （[`scan::scan`](crate::scan::scan)）。
    ///
    /// ## 这一支数的是什么
    ///
    /// **还没完整扫过一趟的根**：从没扫过的，加上上次那一趟**部分完成**的（每个根记着的上次扫描，
    /// [`Catalog::roots`]）。一句查询，不碰盘——外置盘不在位时照样数得出来。
    ///
    /// 它**不数**「盘上变了多少」：那要把整棵树再走一遍才知道，**算这个数就是跑这道工序**
    /// （真库一趟 37.1 分钟）。所以扫完一趟之后这一行归零，往根里再拷东西它不会自己涨；
    /// 加一个根会（挂单 `Q821`）。
    ///
    /// **一个根都没有时交不出数**：那时说「每个根都扫过了」是一句空话，下一步明明是添加根
    /// ——这一支走 [`Behind::Unmeasured`]，没有上次。
    Scan,
    /// **识别**：撞 DAT、撞**沉淀库**、撞名字，给每个**变体**一条结论。
    Identify,
    /// **刮削**：在识别结论的基础上去数据源取标题、简介、封面这些元数据
    /// （[`scrape::run`](crate::scrape::run)）。
    ///
    /// ## 这一支数的是什么
    ///
    /// **一条刮削结论都没有的变体**（[`Catalog::unscraped_variant_count`]）——挂单 `Q447`
    /// 当场裁定的口径，一句 SQL，与识别那一支一样便宜。
    ///
    /// 它**不是**「按刮削面板眼下那套旋钮还差多少」：真判据是逐（锚点 × 源）比
    /// **输入指纹**，而指纹把「这一趟要哪些字段」折了进去——**同一个变体在窄字段那一趟算
    /// 「刮过」、宽字段那一趟算「没刮过」**。按旋钮算的那个数是刮削面板那本估算账的事
    /// （[`scrape::estimate`](crate::scrape::estimate)），**两本账不混**。
    ///
    /// 数**变体**而不数作品：作品锚点只有被识别确认过的变体才挂得上，数作品会把还没认出来的
    /// 那一批整个漏掉；而文件名那个源给每个变体都落一条标题，所以全库刮过一趟，这个数就归零。
    Scrape,
    /// **折标题**：把识别与刮削的结论折成每个作品的**标题集合**
    /// （[`title::run`](crate::title::run)）。
    ///
    /// ## 这一支为什么退回显示时刻
    ///
    /// 它要说的数是「**标题集合变过、但还没重折的作品数**」，而这个数**只有把整份集合
    /// 重折一遍再逐个作品比对才算得出来**——也就是说，**算这个数就是跑这道工序**。
    /// 中立库里没有第二条便宜的路：`title` 那张表上没有时刻列（加一列要升结构版本、
    /// 让人删掉重扫），`candidate` 那张表连时刻都没有，于是拿 `scrape_value.at` 去
    /// 兜的话，重跑一趟识别之后那个数会一直报零——**报零比报不出来坏得多**，
    /// 人会以为这道工序已经跑完了。
    ///
    /// 还有一层拦在前面：[`Stages::survey`] 手里只有中立库，够不着**沉淀库**，
    /// 而**压掉的叫法**正筛在写库那一层（[`title::refold`](crate::title::refold)）。
    /// 拿现折的结果直接比，人删过叫法的那几个作品会永远算成「变过还没重折」——
    /// 一个折完也归不了零的数。
    ///
    /// **实测**（`crates/core/tests/magnitude.rs` 那条挂着 `#[ignore]` 的量级测量，
    /// debug 构建，4,000 个变体 / 4,000 个作品 / 8,000 条叫法的合成库，2026-09-13 与
    /// 14 日）：[`title::fold`](crate::title::fold) 一趟 **90–108 毫秒**（同一天另一趟
    /// 机器更忙，慢到 311 毫秒）；照真库变体数的量级（5 万）线性折算是 **1.1–1.4 秒**，而
    /// [`Stages::survey`] 跑在**画帧那条线程**上、每次重读库屏都要跑一遍——
    /// 一帧的预算是 16 毫秒。真机上 `romcat titles` 整趟不到 3 秒
    /// （`docs/library-facts.md`），那三秒的大头正是这一折。复核就跑那份文件里的
    /// `整理标题一趟的量级`（跑法在它的模块文档里）。
    ///
    /// 所以这一支交出 [`Behind::Unmeasured`]，带上[上次重折的时刻](Catalog::titles_folded_at)。
    /// **要不要为它加一张变更计数表由拿主意的人裁**（挂单 `Q426`）——票面写着
    /// 「不为了整齐去加一张计数表」。
    FoldTitles,
    /// **裁决**：人在候选之间做出选择，或者判定「都不对」——在**待确认队列**屏上做。
    ///
    /// ## 这一支数的是什么
    ///
    /// **待确认队列里等着裁决的变体**，与待确认队列屏报的是**同一个数**
    /// （[`triage::pending_count`]；判据与 [`Queue::pending`](crate::triage::Queue::pending)
    /// 只有一处，挂单 `Q822`）。它要读**沉淀库**：钉在路径上的裁决只记在那儿，只问中立库会把
    /// 人已经裁过的那几个再数一遍。
    ///
    /// **这一道不排任务**：裁决是人自己一批批做的，库屏上那一行的按钮把人带去待确认队列屏。
    Triage,
    /// **导出**：把中立库写成前端能读的元数据，铺在主库上
    /// （[`transfer::export`](crate::adapter::transfer::export)）。
    /// **只写元数据文件，一个 ROM 都不搬**（ADR-0004）。
    ///
    /// ## 这一支为什么也退回显示时刻
    ///
    /// 它要说的数是「**上次导出之后库里改了多少条**」，而中立库里**没有一条便宜的路
    /// 算得出它**：条目不是库里的一行，是**收敛**出来的一份投影——一个条目由作品 ×
    /// 平台底下那一批变体、发行版、刮削值与标题集合合折而成
    /// （[`converge::run`](crate::adapter::converge::run)）。而「变没变」的判据只有
    /// 一个：把这一趟收敛出来的字节与上次写出去的那份**底本**逐份比。
    /// 也就是说，**算这个数就是跑一遍这道工序**（只差最后那句 `fs::write`）。
    ///
    /// 便宜的替代路一条都不成立：`variant`、`work`、`release`、`title` 四张表上
    /// **一列时刻都没有**（加一列要升结构版本、让人删掉重扫），而带时刻的
    /// `scrape_value.at` 只盖得住刮削那一侧——拿它兜的话，重跑一趟识别、改一条裁决、
    /// 折一趟标题都不会让那个数动一下，于是它会长期报零，而**报零比报不出来坏得多**：
    /// 人会以为导出的元数据是最新的。
    ///
    /// **实测有两处，一处就在真库上**：
    ///
    /// - **真机**（`docs/library-facts.md`，2026-09-01）：一趟 `romcat export` 是
    ///   **2.7 秒**——46,444 个变体作品级收敛成 28,529 个条目、22 份文件、14 MiB。
    /// - **合成库**（`crates/core/tests/magnitude.rs` 那条挂着 `#[ignore]` 的量级测量，
    ///   debug 构建，4,000 个变体摊在 10 个平台上、每个都认出了作品并整理过标题，
    ///   2026-09-13 与 14 日）：[`converge::run`](crate::adapter::converge::run) 一趟
    ///   **174–212 毫秒**，整趟 [`transfer::export`](crate::adapter::transfer::export)
    ///   （`dry_run`，一个字节都不写盘）**312–350 毫秒**（同一天另一趟机器更忙，两项
    ///   慢到 539 与 945 毫秒）；照真库变体数的量级（5 万）线性折算是 **2.2–2.7 秒与
    ///   3.9–4.4 秒**——与真机那 2.7 秒同一个量级。复核就跑那份文件里的 `导出一趟的量级`。
    ///
    /// 而 [`Stages::survey`] 跑在**画帧那条线程**上、每次重读库屏都要跑一遍
    /// ——一帧的预算是 16 毫秒，差了两个数量级。
    ///
    /// 所以这一支交出 [`Behind::Unmeasured`]，带上[上次导出的时刻](Catalog::exported_at)。
    /// **要不要为它加一张变更计数表由拿主意的人裁**（挂单 `Q436`）。
    Export,
}

impl Stage {
    /// 全部工序。**库屏上从上到下就是这个次序**，也是主干六步里的先后。
    pub const ALL: [Self; 6] = [
        Self::Scan,
        Self::Identify,
        Self::Scrape,
        Self::FoldTitles,
        Self::Triage,
        Self::Export,
    ];

    /// 打给用户的那个词。**与词表逐字一样**（`CONTEXT.md` 的**工序**条）。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Scan => "扫描",
            Self::Identify => "识别",
            Self::Scrape => "刮削",
            Self::FoldTitles => "整理标题",
            Self::Triage => "裁决",
            Self::Export => "导出",
        }
    }

    /// 这一行那个数的**口径**：要跟那个数一起画在屏上的那句话。数说的就是字面上那件事的
    /// 那几支交 `None`。
    ///
    /// **眼下只有刮削带一句**（[`SCRAPE_BASIS`]）：识别那句「N 个变体连识别都还没跑过」
    /// 读不岔，而刮削那个数一眼看去很像「按我眼下那套旋钮还差多少」——它不是。
    #[must_use]
    pub fn basis(self) -> Option<&'static str> {
        (self == Self::Scrape).then_some(SCRAPE_BASIS)
    }

    /// 这一道的**前置**：它要等哪一道做完（[`Stages::waiting_on`]）。扫描是头一道，不等谁。
    ///
    /// **照设计稿逐行定**（`stageRows()` 里每一行 `wait` 那一支等的是哪一道；拿主意的人 2026-09-14 照设计稿定，挂单
    /// `Q830`）：识别等扫描、刮削等识别、整理标题等刮削、**裁决等识别**、**导出等整理标题**。前置**不是**「排在它上面的
    /// 那一道」——裁决不等刮削与整理标题。词表**工序**条写着这张对应表。
    #[must_use]
    pub fn prerequisite(self) -> Option<Self> {
        match self {
            Self::Scan => None,
            Self::Identify => Some(Self::Scan),
            Self::Scrape => Some(Self::Identify),
            Self::FoldTitles => Some(Self::Scrape),
            Self::Triage => Some(Self::Identify),
            Self::Export => Some(Self::FoldTitles),
        }
    }
}

/// **刮削**那一行的口径（[`Stage::basis`]）。**屏上要明写**（票 `gui-answers-all-six/04`）：
/// 不写的话，人会把那个数读成「按我眼下那套旋钮还差多少」，而那是刮削面板那本估算账
/// ——为什么两本账不混，写在 [`Stage::Scrape`] 上。
///
/// **措辞照设计稿逐字**（`stageRows()` 刮削那一行的 `small`；票 `gui-looks-like-the-design/06` 第二段，拿主意的人
/// 2026-09-14 看候选图后要求）：从前那一长句是写给开发者看的，屏上只留稿上这一句，「不按旋钮算」的来龙去脉留在
/// [`Stage::Scrape`] 的文档里。摆成一个有名字的常量：界面、测试查得到它还在不在。
pub const SCRAPE_BASIS: &str = "统计的是没有任何刮削结果的变体";

/// 一道工序**还差多少**。
///
/// 两支而不是一个数：**某一支交不出度量是认可的退路**（规格「工序进度收成一处」）。
/// 退回时刻的那一行至少不骗人，而硬凑一个零出来会让人以为这道工序已经跑完了。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Behind {
    /// 说得出还差多少。`0` 就是这道工序眼下不差什么。
    Left(u64),
    /// **这一支交不出度量**：退回上次跑的时刻，并说清为什么算不出来。
    Unmeasured {
        /// 上次跑是什么时候（Unix 秒）。**没跑过就是 `None`**——那时画一个时刻出来
        /// 是凭空捏造。
        at: Option<i64>,
        /// 为什么交不出数。这句话要说给人听：它决定人接下来信不信这一行。
        why: String,
    },
}

/// 库屏**工序段**上的一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageRow {
    /// 哪一道工序。
    pub stage: Stage,
    /// 还差多少。
    pub behind: Behind,
}

impl StageRow {
    /// 这一道算不算**做完了**。库屏顶上那一行「下一步」照它挑（[`Stages::next_up`]）。
    ///
    /// - **报得出数的**：还差 0 才算做完。
    /// - **算不出数、退回时刻的**：**跑过就算**。库变过之后该不该重跑，这一支说不出来——硬指着它，
    ///   「下一步」会永远停在整理标题上；该不该重跑，人看那一行上次跑的时刻（挂单 `Q823`）。
    ///   **从没跑过的不算**。读不动库而退回的那几支也落在这一档：那是一件该去查的事，指着它不亏。
    #[must_use]
    pub fn settled(&self) -> bool {
        match &self.behind {
            Behind::Left(left) => *left == 0,
            Behind::Unmeasured { at, .. } => at.is_some(),
        }
    }

    /// 这一行画出来的那句话。
    ///
    /// **措辞在核心里**（ADR-0005）：识别那一句与待确认队列屏、与命令行的队列报告
    /// 印的是同一件事的同一种说法，散到界面上手写一遍，两处迟早会差着字。
    #[must_use]
    pub fn render(&self) -> String {
        match &self.behind {
            // **一道工序一支**：加一支只动这一个 `match`。
            // **不差什么了也得说话**——一行空白读起来像出了什么事，所以「零」那一档
            // 也在这儿各说各的话，不另开一个 `match`。
            Behind::Left(left) => match self.stage {
                // 数的是**根**，不是变体：盘上变了多少不在这个数里（见 [`Stage::Scan`]）。
                Stage::Scan if *left == 0 => "每个根都完整扫过一趟了".to_string(),
                Stage::Scan => format!("{} 个根还没完整扫过一趟", thousands(*left)),
                Stage::Identify if *left == 0 => "每个变体都跑过识别了".to_string(),
                Stage::Identify => format!("{} 个变体连识别都还没跑过", thousands(*left)),
                // 刮削那一支数的是变体，**不是**「按眼下那套旋钮还差多少」
                // ——口径见 [`Stage::Scrape`]。
                Stage::Scrape if *left == 0 => "每个变体都有刮削结论了".to_string(),
                Stage::Scrape => format!("{} 个变体一条刮削结论都还没有", thousands(*left)),
                // **折标题眼下折不出这一支**——它走的是退路（见 [`Stage::FoldTitles`]）。
                // 这两句话是那张变更计数表真被认下来之后这一行该说的话，钉在
                // `crates/core/tests/stage.rs` 上：换度量的人不必再想一遍措辞，
                // 也不会顺手写成「N 个作品还没折标题」——那是另一件事。
                Stage::FoldTitles if *left == 0 => "标题集合都整理过了".to_string(),
                Stage::FoldTitles => {
                    format!("{} 个作品的标题集合变过、还没重新整理", thousands(*left))
                }
                // 与待确认队列屏同一个数（[`Stage::Triage`]）；还没跑过识别时队列本来就是空的。
                Stage::Triage if *left == 0 => "待确认队列里没有等着裁决的变体".to_string(),
                Stage::Triage => {
                    format!("{} 个变体在待确认队列里等着裁决", thousands(*left))
                }
                // **导出眼下也折不出这一支**——同上，它走的是退路（见 [`Stage::Export`]）。
                // 这两句话钉在 `crates/core/tests/stage.rs` 上，理由与折标题那两句一样：
                // 换度量的人不必再想一遍措辞，也不会顺手写成「N 个条目还没导出」
                // ——导出是整库级的，从来不是「有几个条目没导过」。
                Stage::Export if *left == 0 => "导出的元数据都是最新的".to_string(),
                Stage::Export => {
                    format!("{} 个条目在上次导出之后变过", thousands(*left))
                }
            },
            // **退回时刻的那一行不许画零**：「还差 0」与「算不出还差多少」是两件事。
            // 这一支**与是哪道工序无关**，所以它不跟着工序分支——眼下走退路的折标题与
            // 导出两支画的都是它，下一支加进来照样白拿。
            // **扫描那一支交不出数时直说为什么**（一个根都没有、或者读不动库）：它没有「上次跑是几点」这一说——
            // 每个根记着各自的上次扫描，那几格画在库屏根那张表里。空库那一句照拿主意的人 2026-09-14 的答复写
            // （[`scan_row`]），不套「还没跑过（这一趟算不出……）」那层给开发者看的壳。
            Behind::Unmeasured { at: None, why } if self.stage == Stage::Scan => why.clone(),
            Behind::Unmeasured { at, why } => {
                let 上次 = at.map_or_else(
                    || "还没跑过".to_string(),
                    |at| format!("上次跑是 {}", human_time(at)),
                );
                format!("{上次}（这一趟算不出还差多少：{why}）")
            }
        }
    }
}

/// 全部工序的进度，**一次问出来**。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stages {
    rows: Vec<StageRow>,
    /// 那几行底下那句小字要用的数（[`Stages::detail`]），与那几行同一趟问出来。
    facts: Facts,
}

/// 工序那几行底下那句小字要用的数。**每个数都有自己的查询函数**，这里只一趟问齐；读不动的那一个是 `None`，那一句
/// 就不画——与 [`Stages::survey`] 不返回 `Result` 同一条理由。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Facts {
    totals: Option<LibraryTotals>,
    identified: Option<IdentifyTally>,
    titled: Option<TitleTally>,
    export_setup: Option<ExportSetup>,
    exported_entries: Option<u64>,
    exported_media: Option<bool>,
    /// 前几批盖住多少：**不在 [`Stages::survey`] 里问**，由调用方算一次交进来（[`Stages::set_queue_head`]）。
    queue_head: Option<Coverage>,
}

impl Stages {
    /// 问一遍这个库：每一道工序还差多少。**一个字节都不读主库**（ADR-0001）——
    /// 外置盘不在位时工序那几行照样答得出话。
    ///
    /// **它不返回 `Result`**：某一支读不动库时降级成
    /// [`Behind::Unmeasured`] 那一支，别的支照旧报数。见模块文档。
    ///
    /// `store` 与 `library` 是**沉淀库**与这份主库的**主库标识**：裁决那一行要问沉淀库
    /// 对哪些变体说过话（[`Stage::Triage`]）。
    #[must_use]
    pub fn survey(catalog: &Catalog, store: &Store, library: &str) -> Self {
        Self {
            rows: Stage::ALL
                .iter()
                .map(|stage| row_of(*stage, catalog, store, library))
                .collect(),
            facts: Facts {
                totals: catalog.library_totals().ok(),
                identified: catalog.identify_tally().ok(),
                titled: catalog.title_tally().ok(),
                export_setup: catalog.export_setup().ok().flatten(),
                exported_entries: catalog.exported_entries().ok().flatten(),
                exported_media: catalog.exported_with_media().ok().flatten(),
                queue_head: None,
            },
        }
    }

    /// 一道工序一行，与 [`Stage::ALL`] 同序。
    #[must_use]
    pub fn rows(&self) -> &[StageRow] {
        &self.rows
    }

    /// 某一道工序那一行；没有就是 `None`。
    #[must_use]
    pub fn of(&self, stage: Stage) -> Option<&StageRow> {
        self.rows.iter().find(|row| row.stage == stage)
    }

    /// 库屏顶上那一行「**下一步**」指着的那一道：从上往下头一道还没做完的
    /// （[`StageRow::settled`]）；六道都做完了就是 `None`。
    ///
    /// **判断在核心里**（ADR-0005）：「哪一道算做完了」是领域判断，界面只画它指着的那一行。
    #[must_use]
    pub fn next_up(&self) -> Option<&StageRow> {
        self.rows.iter().find(|row| !row.settled())
    }

    /// 这一行**在等哪一道**：它的前置（[`Stage::prerequisite`]）还没做完时交那一道；前置做完了、或者它没有前置，
    /// 交 `None`。
    ///
    /// **等的是自己那一道前置，不是头一道没做完的**（挂单 `Q830` 已裁：照稿）：识别一跑完，裁决就不再在等，哪怕刮削
    /// 还没跑。**「前置做完了」要连着往上问**：前置那一行自己数出来不差什么、却还在等它的前置时，照样算没做完——一个根
    /// 都没扫过的库里识别那一行数出来是零（库里一个变体都没有），刮削那一行照样说「等待识别完成」，不说空话（挂单
    /// `Q827`）。**判断在核心里**（ADR-0005）：界面照这个答案决定那一行画不画按钮、用不用弱色。
    #[must_use]
    pub fn waiting_on(&self, row: &StageRow) -> Option<Stage> {
        let before = row.stage.prerequisite()?;
        let done = self
            .of(before)
            .is_some_and(|it| it.settled() && self.waiting_on(it).is_none());
        (!done).then_some(before)
    }

    /// 这一行底下那句**小字**（设计稿 `stageRows()` 每行的 `small`）；这一行写不出第二句时交 `None`。**在等的那几行
    /// 不画**（设计稿 `wait` 那一支没有小字）。
    ///
    /// - **扫描**做完了：几个根一共装着几份文件、多少字节（[`Catalog::library_totals`]）。
    /// - **识别**做完了：命中、未命中、无判据、跳过各几个，几条候选来自已问过的模型答案（[`Catalog::identify_tally`]）。
    /// - **整理标题**跑过了：几个作品、其中几个有中文标题（[`Catalog::title_tally`]）。
    /// - **裁决**：前几批可一次处理多少（[`triage::head_coverage`]）。它贵，不在 [`Self::survey`] 里问：调用方算一次交进来
    ///   （[`Self::set_queue_head`]），没交就不画。
    /// - **导出**选过格式与目录时：跑过的说格式、上次那一趟收敛出几个条目（[`Catalog::exported_entries`]），以及那一趟
    ///   **照实**是仅写入元数据还是连媒体也写了（[`Catalog::exported_with_media`]，挂单 `Q889` 已裁）；记这两样之前导出的库
    ///   说不出，那几段不画。没跑过的说格式与目录。
    /// - **刮削**那一句是口径（[`Stage::basis`]），界面照前几张票挨着那个数画，不走这里。
    ///
    /// **数与措辞都在核心里**（ADR-0005）：每个数各有一个查询函数，[`Self::survey`] 那一趟一起问齐；读不动的那一个
    /// 不画，别的照旧。
    #[must_use]
    pub fn detail(&self, row: &StageRow) -> Option<String> {
        if self.waiting_on(row).is_some() {
            return None;
        }
        let facts = &self.facts;
        match row.stage {
            Stage::Scan => facts
                .totals
                .filter(|totals| row.settled() && totals.roots > 0)
                .map(|totals| {
                    format!(
                        "{} 个文件 · {} · {} 个根",
                        thousands(totals.files),
                        human_bytes(totals.bytes),
                        thousands(totals.roots),
                    )
                }),
            Stage::Identify => facts.identified.filter(|_| row.settled()).map(|tally| {
                format!(
                    "命中 {} · 未命中 {} · 无判据 {} · 跳过 {} · {} 条候选来自已问过的模型答案",
                    thousands(tally.matched),
                    thousands(tally.unmatched),
                    thousands(tally.no_evidence),
                    thousands(tally.skipped),
                    thousands(tally.model_candidates),
                )
            }),
            Stage::FoldTitles => facts.titled.filter(|_| row.settled()).map(|tally| {
                format!(
                    "{} 个作品 · {} 个有中文标题",
                    thousands(tally.works),
                    thousands(tally.chinese),
                )
            }),
            Stage::Triage => {
                facts
                    .queue_head
                    .filter(|coverage| coverage.head > 0)
                    .map(|coverage| {
                        format!(
                            "前 {} 批可一次处理 {} 个",
                            coverage.head_batches,
                            thousands(coverage.head)
                        )
                    })
            }
            Stage::Export => facts.export_setup.as_ref().map(|setup| {
                if !row.settled() {
                    return format!("{} · {}", setup.format, path::display(&setup.out));
                }
                match (facts.exported_entries, facts.exported_media) {
                    (Some(entries), Some(media)) => format!(
                        "{} · {} 个条目 · {}",
                        setup.format,
                        thousands(entries),
                        if media {
                            "写入元数据与媒体"
                        } else {
                            "仅写入元数据"
                        },
                    ),
                    (Some(entries), None) => {
                        format!("{} · {} 个条目", setup.format, thousands(entries))
                    }
                    (None, _) => setup.format.clone(),
                }
            }),
            Stage::Scrape => None,
        }
    }

    /// 交进来裁决那一行底下那句小字要的**前几批盖住多少**（[`triage::head_coverage`]）。
    ///
    /// **它不在 [`Self::survey`] 里问**：那要把整个队列连候选读一遍，跟着每次重读库屏跑付不起——界面算一次存着、只在队列可能
    /// 变了时重算，每次问完工序那几行再把存着的那一份交进来。交 `None`（没算过、算不出来）就不画那一句。
    pub fn set_queue_head(&mut self, head: Option<Coverage>) {
        self.facts.queue_head = head;
    }

    /// 这一行**放在整段里**画出来的那句话：在等它的前置（[`Self::waiting_on`]）就说「等待某某完成」，
    /// 别的时候就是它自己那句 [`StageRow::render`]。
    #[must_use]
    pub fn line(&self, row: &StageRow) -> String {
        match self.waiting_on(row) {
            Some(earlier) => format!("等待{}完成", earlier.label()),
            None => row.render(),
        }
    }

    /// 六道都做完时（[`Self::next_up`] 交 `None`）补的那一句：哪几道**只看跑没跑过**——它们算不出还差多少，
    /// 库变过之后要不要重跑得看那几行上次跑的时刻。一道都没有退回时刻时交 `None`。
    ///
    /// **照眼下那几行现折**，不写死是哪两道：度量真做出来、那一行换回报数时，这句话自己就少一道。
    #[must_use]
    pub fn settled_by_running(&self) -> Option<String> {
        let 只看跑没跑过: Vec<&str> = self
            .rows
            .iter()
            .filter(|row| matches!(row.behind, Behind::Unmeasured { .. }))
            .map(|row| row.stage.label())
            .collect();
        (!只看跑没跑过.is_empty()).then(|| {
            format!(
                "{}算不出还差多少，只看跑没跑过——库变过之后要不要重跑，看那几行上次跑的时刻。",
                只看跑没跑过.join("与")
            )
        })
    }
}

/// 某一道工序那一行。**加一支就在这儿多挂一个函数。**
fn row_of(stage: Stage, catalog: &Catalog, store: &Store, library: &str) -> StageRow {
    match stage {
        Stage::Scan => scan_row(catalog),
        Stage::Identify => identify_row(catalog),
        Stage::Scrape => scrape_row(catalog),
        Stage::FoldTitles => fold_titles_row(catalog),
        Stage::Triage => triage_row(catalog, store, library),
        Stage::Export => export_row(catalog),
    }
}

/// **扫描**那一行：还有几个**根**没完整扫过一趟——从没扫过的，加上上次那一趟部分完成的。
///
/// 口径、为什么不数「盘上变了多少」，写在 [`Stage::Scan`] 上。**一个字节都不读主库**：
/// 每个根上次扫描的结果住在中立库里。
fn scan_row(catalog: &Catalog) -> StageRow {
    let behind = match catalog.roots() {
        // **一个根都没有就交不出数**：「每个根都扫过了」在这时是一句空话。那一句照拿主意的人 2026-09-14 的答复写
        // （库屏顶上「下一步：扫描」底下与扫描那一行都画它，[`StageRow::render`] 不给扫描这一支套「还没跑过」那层壳）。
        Ok(roots) if roots.is_empty() => Behind::Unmeasured {
            at: None,
            why: "还没有根，先点右上角「添加根…」选一个目录".to_string(),
        },
        Ok(roots) => Behind::Left(roots.iter().filter(|root| !root.fully_scanned()).count() as u64),
        // 读不动就退回那一支——**只降这一行**，与识别那一行同一个口径。
        Err(error) => Behind::Unmeasured {
            at: None,
            why: format!("中立库读不动：{error}"),
        },
    };
    StageRow {
        stage: Stage::Scan,
        behind,
    }
}

/// **裁决**那一行：待确认队列里还有几个变体等着裁决。
///
/// **不另造一份数**：它就是 [`triage::pending_count`]，与待确认队列屏
/// （[`Queue::pending`](crate::triage::Queue::pending)）同一句判据。
fn triage_row(catalog: &Catalog, store: &Store, library: &str) -> StageRow {
    // **两份库各说各的读不动**：沉淀库读不出来与中立库读不出来是两件该去查的事。**只降这一行。**
    let behind = match verdict::Index::load(store, library) {
        Ok(verdicts) => match triage::pending_count(catalog, &verdicts) {
            Ok(left) => Behind::Left(left),
            Err(error) => Behind::Unmeasured {
                at: None,
                why: format!("中立库读不动：{error}"),
            },
        },
        Err(error) => Behind::Unmeasured {
            at: None,
            why: format!("沉淀库读不动：{error}"),
        },
    };
    StageRow {
        stage: Stage::Triage,
        behind,
    }
}

/// **识别**那一行：库里还有多少个**变体**连识别都还没跑过。
///
/// **不另造一份数**：它就是 [`Catalog::not_run_count`]——待确认队列
/// （[`Queue::not_run`](crate::triage::Queue::not_run)）与命令行队列报告
/// （[`QueueReport::not_run`](crate::triage::report::QueueReport::not_run)）取的是
/// 同一处。造第二份的话，同一份库在库屏与队列屏上会报出两个数。
fn identify_row(catalog: &Catalog) -> StageRow {
    let behind = match catalog.not_run_count() {
        Ok(left) => Behind::Left(left),
        // 读不动就退回那一支——**只降这一行**。整段一起失败的话，一个算不出来的度量
        // 会让库屏上连识别那一行都消失。
        Err(error) => Behind::Unmeasured {
            at: None,
            why: format!("中立库读不动：{error}"),
        },
    };
    StageRow {
        stage: Stage::Identify,
        behind,
    }
}

/// **刮削**那一行：库里还有多少个**变体**一条刮削结论都没有。
///
/// 口径、为什么不按旋钮算，写在 [`Stage::Scrape`] 上。数只有
/// [`Catalog::unscraped_variant_count`] 一处算得出来——一句 SQL，跑在画帧那条线程上也不卡。
fn scrape_row(catalog: &Catalog) -> StageRow {
    let behind = match catalog.unscraped_variant_count() {
        Ok(left) => Behind::Left(left),
        // 读不动就退回那一支——**只降这一行**，与识别那一行同一个口径。
        Err(error) => Behind::Unmeasured {
            at: None,
            why: format!("中立库读不动：{error}"),
        },
    };
    StageRow {
        stage: Stage::Scrape,
        behind,
    }
}

/// **折标题**那一行：**退回上次重折的时刻**。
///
/// 为什么不报数、代价实测了多少，全写在 [`Stage::FoldTitles`] 上。这一层只做两件事：
/// 把[上次重折的时刻](Catalog::titles_folded_at)取出来，把「为什么算不出来」这句话
/// 说给人听——那句话决定人接下来信不信这一行。
///
/// **时刻读不出来也不失败**：那时 `at` 是 `None`，[`StageRow::render`] 画的是
/// 「还没跑过」。一份库读不动不该让工序段上连识别那一行都消失。
fn fold_titles_row(catalog: &Catalog) -> StageRow {
    // **「这份库读不动」与「还没折过」是两句话**，别都画成「还没跑过」——前者是一件
    // 该去查的事，后者是一件该去做的事。与识别那一行同一个口径：**只降这一行**。
    let (at, why) = match catalog.titles_folded_at() {
        Ok(at) => (
            at,
            "要把整份标题集合重新整理一遍才比得出来，而那一趟正是这道工序自己".to_string(),
        ),
        Err(error) => (None, format!("中立库读不动：{error}")),
    };
    StageRow {
        stage: Stage::FoldTitles,
        behind: Behind::Unmeasured { at, why },
    }
}

/// **导出**那一行：**退回上次导出的时刻**。
///
/// 为什么不报数、代价实测了多少，全写在 [`Stage::Export`] 上。这一层与
/// [`fold_titles_row`] 一个形状：把[上次导出的时刻](Catalog::exported_at)取出来，
/// 把「为什么算不出来」这句话说给人听。
///
/// **一个字节都不读那些元数据文件**：外置盘不在位、导出目录不在了，这一行照样答得出话
/// ——它读的是中立库里那个键。
fn export_row(catalog: &Catalog) -> StageRow {
    // **「这份库读不动」与「还没导过」是两句话**，与识别、折标题两行同一个口径：
    // 前者是一件该去查的事，后者是一件该去做的事。**只降这一行。**
    let (at, why) = match catalog.exported_at() {
        Ok(at) => (
            at,
            "要把整库收敛一遍、再逐份与上次写出去的底本比对才算得出来，\
             而那一趟正是这道工序自己"
                .to_string(),
        ),
        Err(error) => (None, format!("中立库读不动：{error}")),
    };
    StageRow {
        stage: Stage::Export,
        behind: Behind::Unmeasured { at, why },
    }
}
