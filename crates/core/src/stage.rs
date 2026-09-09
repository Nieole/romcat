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

use crate::catalog::Catalog;
use crate::report::{human_time, thousands};

/// 一道**工序**。
///
/// **眼下是识别、折标题与导出三支**。**加一支要写五处，两处不在这个 crate 里**：
///
/// 1. 这个枚举一个变体，加 [`Stage::label`] 那个 `match` 一支；
/// 2. [`Stage::ALL`] 一项——[`Stages::survey`] 照它走，漏了就整支不出现；
/// 3. 一个折得出 [`StageRow`] 的函数，挂进 `row_of` 那个 `match`；
/// 4. [`StageRow::render`] 那个 `match` 一支（说不出还差多少时走
///    [`Behind::Unmeasured`]，那一支与工序无关，不必动）；
/// 5. **界面那一侧两处**：`romcat_gui::stages` 里排活那个 `match`（这一支排一趟什么活
///    上任务台），以及 `romcat_gui::app::App::poll_tasks` 里那个 `match`
///    （跑完之后还有哪一屏要重读）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    /// **识别**：撞 DAT、撞**沉淀库**、撞名字，给每个**变体**一条结论。
    Identify,
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
    /// **实测**（`crates/core` 的 debug 构建，2,000 个作品 / 2,000 个变体 /
    /// 4,000 条叫法的合成库）：[`title::fold`](crate::title::fold) 一趟 **73–81 毫秒**，
    /// 一趟走过约 1.2 万行；照真库 46,483 个变体线性折算是 **1.7 秒上下**，而
    /// [`Stages::survey`] 跑在**画帧那条线程**上、每次重读库屏都要跑一遍——
    /// 一帧的预算是 16 毫秒。真机上 `romcat titles` 整趟不到 3 秒
    /// （`docs/library-facts.md`），那三秒的大头正是这一折。
    ///
    /// 所以这一支交出 [`Behind::Unmeasured`]，带上[上次重折的时刻](Catalog::titles_folded_at)。
    /// **要不要为它加一张变更计数表由拿主意的人裁**（挂单 `Q426`）——票面写着
    /// 「不为了整齐去加一张计数表」。
    FoldTitles,
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
    /// - **合成库**（`crates/core` 的 debug 构建，4,000 个变体摊在 10 个平台上）：
    ///   [`converge::run`](crate::adapter::converge::run) 一趟 **45–49 毫秒**，整趟
    ///   [`transfer::export`](crate::adapter::transfer::export)（`dry_run`，一个字节
    ///   都不写盘）**166–199 毫秒**；照 46,483 个变体线性折算是 **0.5 秒与 2.0 秒**
    ///   ——与真机那 2.7 秒同一个量级。这份合成库里一个作品、一条刮削值都没有，
    ///   所以它是个**下界**。
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
    pub const ALL: [Self; 3] = [Self::Identify, Self::FoldTitles, Self::Export];

    /// 打给用户的那个词。**与词表逐字一样**（`CONTEXT.md` 的**工序**条）。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Identify => "识别",
            Self::FoldTitles => "折标题",
            Self::Export => "导出",
        }
    }
}

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
                Stage::Identify if *left == 0 => "每个变体都跑过识别了".to_string(),
                Stage::Identify => format!("{} 个变体连识别都还没跑过", thousands(*left)),
                // **折标题眼下折不出这一支**——它走的是退路（见 [`Stage::FoldTitles`]）。
                // 这两句话是那张变更计数表真被认下来之后这一行该说的话，钉在
                // `crates/core/tests/stage.rs` 上：换度量的人不必再想一遍措辞，
                // 也不会顺手写成「N 个作品还没折标题」——那是另一件事。
                Stage::FoldTitles if *left == 0 => "标题集合都是重折过的".to_string(),
                Stage::FoldTitles => {
                    format!("{} 个作品的标题集合变过、还没重折", thousands(*left))
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
}

impl Stages {
    /// 问一遍这个库：每一道工序还差多少。**一个字节都不读主库**（ADR-0001）——
    /// 外置盘不在位时工序那几行照样答得出话。
    ///
    /// **它不返回 `Result`**：某一支读不动库时降级成
    /// [`Behind::Unmeasured`] 那一支，别的支照旧报数。见模块文档。
    #[must_use]
    pub fn survey(catalog: &Catalog) -> Self {
        Self {
            rows: Stage::ALL
                .iter()
                .map(|stage| row_of(*stage, catalog))
                .collect(),
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
}

/// 某一道工序那一行。**加一支就在这儿多挂一个函数。**
fn row_of(stage: Stage, catalog: &Catalog) -> StageRow {
    match stage {
        Stage::Identify => identify_row(catalog),
        Stage::FoldTitles => fold_titles_row(catalog),
        Stage::Export => export_row(catalog),
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
            "要把整份标题集合重折一遍才比得出来，而那一趟正是这道工序自己".to_string(),
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
