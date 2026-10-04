//! 把中立库、**媒体池**与目标设备折成一份 [`Plan`]——也就是**差量预览**。
//!
//! ## 为什么这一步在核心里
//!
//! 它是十来个步骤串起来的一条线：读子库 → 读选择集 → 折事实 → 求值 → 看一眼目标 →
//! 读能力档案 → 折期望状态 → 按目标存储筛一遍（连多碟变体的播放列表）→ 铺媒体 → 折前端元数据 →
//! 读清单 → 排计划。每一步都是领域
//! 判断，而**命令行与界面必须得到同一份计划**——`romcat sublibrary plan` 印出来的那份
//! 差量，与界面上按钮旁边显示的那份，不能是两条各自演化的代码。
//!
//! 票 25 之前这条线只住在 `romcat-cli` 的 `main.rs` 里。界面要呈现差量预览
//! （ADR-0016 的硬要求），于是它搬到这儿来：命令行与界面都只是调它一次。
//! 这也正是 ADR-0005 那句「换掉的只是 GUI 那一层」得以成立的前提——
//! `crates/gui` 里一条领域判断都没有。
//!
//! ## 除了目标目录，什么都不写
//!
//! [`prepare`] 是**只读**的：选择集从中立库折出来，目标设备走 [`observe`](mod@super::observe)
//! 那道只读接缝走一遍，排计划的那一步是纯函数。连**媒体池**的目录都不建——
//! 「排计划这条命令一个文件都没写」是可以照字面核对的一句话。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::adapter::NoAdapter;
use crate::capability::{RejectReason, Roster, today};
use crate::catalog::{Catalog, Roots};
use crate::fs::RealFs;
use crate::path;
use crate::platform::Manifest as PlatformManifest;
use crate::scrape::Priorities;
use crate::scrape::pool::MediaPool;
use crate::sublibrary::target::{TargetRefusal, library_overlap};
use crate::sublibrary::{self, Selected, Sublibrary};
use crate::task::{Cutoff, Halted, Handle};
use crate::workspace;

use super::{Desired, Manifest, ObserveError, Options, Plan, TargetState};

/// **排不出计划的原因**，结构化地交出去（挂单 `Q851` `Q622`）。
///
/// 两个壳照种类各补各的去处：命令行在印之前补自己那条命令，界面补屏上的那一处（卡上的「目标设置…」）。
/// **`Display` 只说事实与去处的名字，不带命令行命令**——界面原样画它时，屏上就不出现一句终端命令。
///
/// 「装得下吗」算不出时，[`Fit::Unknown`](crate::sublibrary::Fit::Unknown) 带的就是它：
/// 界面照种类挑话，不再自己查一眼目标在不在位（ADR-0024）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unplanned {
    /// 没有这个名字的子库。
    #[error("没有叫「{name}」的子库。")]
    NoSublibrary {
        /// 点的那个名字。
        name: String,
    },
    /// 看一眼目标没看成：未连接，或者列不开（[`ObserveError`]）。
    #[error(transparent)]
    Target(#[from] ObserveError),
    /// 子库记着的前端格式这一版没带适配器（[`Sublibrary::adapter`]，挂单 `Q797`）。
    #[error(transparent)]
    NoAdapter(#[from] NoAdapter),
    /// 别的原因：中立库读不动、能力档案名册或优先级表读不动……一句给人看的话。
    #[error("{0}")]
    Failed(String),
}

impl Unplanned {
    /// 原因是**目标未连接**时，那条目标路径；别的原因是 `None`。
    ///
    /// 两个壳都要按「是不是未连接」挑话（界面写「请先连接设备」、命令行补改目标路径的命令），
    /// 给这一问一个名字，调用方就不必一层层拆 [`ObserveError`]。
    #[must_use]
    pub fn absent(&self) -> Option<&str> {
        match self {
            Self::Target(ObserveError::Absent { path }) => Some(path),
            _ => None,
        }
    }
}

/// `--json` 里照旧是**那一句话**：报告里 `why` 那一格从来是给人读的一句（只说事实、不带命令），
/// 种类留给两个壳在内存里分支用。
impl Serialize for Unplanned {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// 排计划交不出一份 [`Prepared`] 时交上来的：被叫停了，或者排不出（[`Unplanned`]）。
///
/// 与 [`Cutoff`] 是同一道两选一，只是「排不出」那一支带着结构化的原因而不是一句话——命令行要照种类补命令，
/// 「装得下吗」要把原因原样交给界面。任务台上那一趟收的是 [`Cutoff`]：`From` 折过去（折的是支，不是话）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanCutoff {
    /// 被叫停了，停在两步之间，什么都没留下。
    Halted,
    /// 排不出：为什么。
    Unplanned(Unplanned),
}

impl From<Halted> for PlanCutoff {
    fn from(_: Halted) -> Self {
        Self::Halted
    }
}

impl From<Unplanned> for PlanCutoff {
    fn from(why: Unplanned) -> Self {
        Self::Unplanned(why)
    }
}

impl From<ObserveError> for PlanCutoff {
    fn from(why: ObserveError) -> Self {
        Self::Unplanned(Unplanned::Target(why))
    }
}

impl From<NoAdapter> for PlanCutoff {
    fn from(why: NoAdapter) -> Self {
        Self::Unplanned(Unplanned::NoAdapter(why))
    }
}

impl From<String> for PlanCutoff {
    fn from(why: String) -> Self {
        Self::Unplanned(Unplanned::Failed(why))
    }
}

impl From<PlanCutoff> for Cutoff {
    /// **被叫停不折成一句「失败」**：折的是支，不是话。
    fn from(cut: PlanCutoff) -> Self {
        match cut {
            PlanCutoff::Halted => Self::Halted,
            PlanCutoff::Unplanned(why) => Self::Failed(why.to_string()),
        }
    }
}

/// 排一次计划要说清的几件事。
#[derive(Debug, Clone, Default)]
pub struct Request<'a> {
    /// 这一趟临时指到别处去。不给就用子库自己记着的目标路径。
    ///
    /// **给的是系统交出来的原始形式，原样拿去读盘**（ADR-0020）。
    pub target: Option<&'a Path>,
    /// 清单说有、目标上没了的，这一趟补回去。
    ///
    /// **默认不补**：那可能是维护者在掌机上有意删的（ADR-0015）。
    pub restore_missing: bool,
    /// 另指一份字段级优先级表。不给就用工作目录里那份，再没有就用内置的。
    pub priorities: Option<&'a Path>,
}

/// 排好的一份计划，连折它时那几件要说出口的怪事。
#[derive(Debug, Clone)]
pub struct Prepared {
    /// 这一趟对着的子库，**能力档案那一列已经换成真正生效的那一份**。
    pub sublibrary: Sublibrary,
    /// 目标根，**系统给的原始形式**——读盘走它（ADR-0020）。
    pub root: PathBuf,
    /// 选择集求值的产物与它的账。
    pub selected: Selected,
    /// 期望状态：主库该有的。
    pub desired: Desired,
    /// **清单**：上次导出记下的。
    pub manifest: Manifest,
    /// 目标上实际有的。
    pub actual: TargetState,
    /// 三方对比出来的计划，**它就是差量预览**。
    pub plan: Plan,
    /// 媒体：相对子库根的路径 → 它在**媒体池**里的落点。
    pub from_pool: BTreeMap<String, PathBuf>,
    /// 生成物：相对子库根的路径 → 内容。前端元数据与多碟变体的播放列表走这条。
    pub generated: BTreeMap<String, Vec<u8>>,
    /// **媒体池**自己的临时目录。执行那一步拿它当硬链接探测的源那一头。
    pub scratch: PathBuf,
    /// 读不懂的规则有几条。
    pub broken: usize,
    /// 库里记着、池里却没有那个文件的媒体引用有几条。
    pub media_not_in_pool: u64,
    /// 认不出是什么、因此一张都没铺的图有几张。
    pub media_unknown_kind: u64,
    /// 靠文件名找媒体的格式里，被同类挤掉、因此没铺出去的图有几张。
    pub media_crowded_out: u64,
    /// 折出了几个前端条目。
    pub entries: u64,
    /// **没上卡**的变体 → 拦下它主文件的那一条（[`OnCard::left_off`](super::OnCard::left_off)）：卡上的前端元数据不列、
    /// 媒体也不铺（票 `verdict-store-and-sync/21`）。差量预览照它说一句：命令行印 [`Concern::LeftOffCard`]，界面在
    /// 「放不进目标」那一栏的说明后头接 [`Self::left_off_note`]。
    pub left_off: BTreeMap<String, RejectReason>,
    /// 子库记着的能力档案在眼下这份名册里找不到——退回了「不作声称」。
    pub missing_capability: Option<String>,
    /// 这份档案里有几条声明已经陈旧。
    pub stale_claims: usize,
}

impl Prepared {
    /// 折期望状态时那几件**要说出口**的怪事，一件一条（[`Concern`]，它的 `Display` 是那一段话）。
    ///
    /// **命令行与界面印同一份。** 各写一遍的话，界面上会少掉其中一两条——而这几条正是
    /// 「为什么这一趟少选出来这么多」的答案，少印一条就等于让人对着一个说不通的数字发呆。
    /// 那段话只说事实与去处的名字、**不带命令**（挂单 `Q622` 那一族）：命令行照种类补自己那条命令，
    /// 界面照种类补屏上的去处。
    ///
    /// **一条例外是 [`Concern::LeftOffCard`]**：界面不在这几行里画它，在「放不进目标」那一栏的说明后头接一句
    /// [`Self::left_off_note`]（拿主意的人 2026-10-04 裁，挂单 `Q1877`）；命令行照旧印它。
    #[must_use]
    pub fn concerns(&self) -> Vec<Concern> {
        let mut out = Vec::new();
        if self.broken > 0 {
            out.push(Concern::BrokenRules(self.broken));
        }
        if self.media_not_in_pool > 0 {
            out.push(Concern::MediaNotInPool(self.media_not_in_pool));
        }
        if self.media_unknown_kind > 0 {
            out.push(Concern::MediaUnknownKind(self.media_unknown_kind));
        }
        if self.media_crowded_out > 0 {
            out.push(Concern::MediaCrowdedOut(self.media_crowded_out));
        }
        if let Some(missing) = &self.missing_capability {
            out.push(Concern::MissingCapability(missing.clone()));
        }
        if self.stale_claims > 0 {
            out.push(Concern::StaleClaims(self.stale_claims));
        }
        if !self.left_off.is_empty() {
            out.push(Concern::LeftOffCard(self.left_off.clone()));
        }
        out
    }

    /// 「放不进目标」那一栏的说明后头**接的那一句**：有变体没上卡时是 [`LEFT_OFF_NOTE`]，没有时不说。
    ///
    /// 拿主意的人 2026-10-04 裁（挂单 `Q1877`）：屏上那一句摆进那一栏、接在「这些文件这一趟不会复制……」后头，
    /// 不重复个数、不用警示色——是哪几份那一栏自己列着。「什么时候说」与「说什么」都在核心这一处：被拦下的只是附属
    /// 文件、媒体或元数据时变体照样上了卡，那时说「前端里也不列」是句假话。
    #[must_use]
    pub fn left_off_note(&self) -> Option<&'static str> {
        (!self.left_off.is_empty()).then_some(LEFT_OFF_NOTE)
    }
}

/// 「放不进目标」那一栏的说明后头接的那一句（[`Prepared::left_off_note`]）：只说事实，几个、是哪几份那一栏自己列着。
pub const LEFT_OFF_NOTE: &str = "前端里也不列它们。";

/// 折期望状态时一件**要说出口**的怪事（[`Prepared::concerns`]）。
///
/// **是哪一种交成结构化的，那段话只说事实与去处的名字**（挂单 `Q622` 那一族）：命令行照种类在印之前补自己那条
/// 命令，界面照种类补屏上的那一处——核心库不知道屏上的位置，也不该把终端命令画到屏上。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Concern {
    /// 有几条规则读不懂、这一趟没参与求值。
    BrokenRules(usize),
    /// 库里记着、媒体池里却没有那个文件的媒体引用有几条。
    MediaNotInPool(u64),
    /// 认不出是什么、因此一张都没铺的图有几张。
    MediaUnknownKind(u64),
    /// 靠文件名找媒体的格式里，被同类挤掉、没铺出去的图有几张。
    MediaCrowdedOut(u64),
    /// 子库记着的能力档案在眼下这份名册里找不到——退回了「不作声称」。带着记着的那个名字。
    MissingCapability(String),
    /// 这份能力档案里有几条声明超过半年没核实。
    StaleClaims(usize),
    /// 有几个变体的主文件**放不进目标**、压根没上卡，卡上的前端元数据里也不列它们（[`Prepared::left_off`]，
    /// 票 `verdict-store-and-sync/21`）。带着是哪几个（变体的键）、各被哪一类拦下：命令行列出键、补上排除的命令。
    /// 那段话只说几个、各是哪一类。**界面不画这一条**，在「放不进目标」那一栏接 [`Prepared::left_off_note`]（挂单 `Q1877`）。
    LeftOffCard(BTreeMap<String, RejectReason>),
}

impl std::fmt::Display for Concern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::report::thousands;
        match self {
            Self::BrokenRules(count) => write!(
                f,
                "⚠️ 有 {} 条规则读不懂、这一趟没参与求值——少选出来的东西全在它们里面。",
                thousands(*count as u64),
            ),
            Self::MediaNotInPool(count) => write!(
                f,
                "⚠️ 有 {} 条媒体引用在媒体池里找不到那个文件，这一趟一张都不铺。\n\
                 重新刮削一次把它们收回池里。",
                thousands(*count),
            ),
            Self::MediaUnknownKind(count) => write!(
                f,
                "认不出是什么的图有 {} 张，一张都没铺——猜错了就是把说明书当封面。",
                thousands(*count),
            ),
            Self::MediaCrowdedOut(count) => write!(
                f,
                "有 {} 张图被同类挤掉、没铺出去：这个格式靠文件名找媒体，\n\
                 一个游戏的一个类型只放得下一张。挤掉的是同一个游戏的第二张起。",
                thousands(*count),
            ),
            Self::MissingCapability(missing) => write!(
                f,
                "⚠️ 子库记着的能力档案「{missing}」在眼下这份名册里找不到，这一趟退回了\n\
                 「不作声称」：不转换、也不检查。别以为它替你查过了。重挑一份能力档案。",
            ),
            Self::StaleClaims(count) => write!(
                f,
                "⚠️ 这份能力档案里有 {} 条声明超过半年没核实。模拟器一年发好几版，\n\
                 而矩阵错了比不转换更糟。",
                thousands(*count as u64),
            ),
            // 「放不进目标」与几类的名字照屏上那一栏的说法（`RejectReason::label`）；前端里不列是这一趟替人做了的事，
            // 说出口，不然人会在前端里找它。
            Self::LeftOffCard(left_off) => {
                let 几类: Vec<String> = RejectReason::all()
                    .into_iter()
                    .filter_map(|reason| {
                        let 几个 = left_off.values().filter(|one| **one == reason).count();
                        (几个 > 0)
                            .then(|| format!("{} {} 个", reason.label(), thousands(几个 as u64)))
                    })
                    .collect();
                write!(
                    f,
                    "有 {} 个变体放不进目标、没上卡，前端里也不列：{}。",
                    thousands(left_off.len() as u64),
                    几类.join("、"),
                )
            }
        }
    }
}

/// 挑出这一趟用哪一份优先级表：给了的 > 工作目录里那份 > 内置的。
///
/// **刮削、标题、导出与同步共用它，而且必须共用**：几条路对「哪个源说了算」的答案
/// 不一样的话，报告里合并出来的标题与导出时挑出来的标题就会对不上。
///
/// 工作目录里那份在哪由 [`workspace::priorities_path`] 定——界面上改完优先级写回的
/// 也是那一处（ADR-0024）。
///
/// # Errors
/// 那份表读不动或者写坏了时返回一句给人看的话。
pub fn priorities(given: Option<&Path>, workspace: &Path) -> Result<Priorities, String> {
    let path = match given {
        Some(path) => path.to_path_buf(),
        None => {
            let candidate = workspace::priorities_path(workspace);
            if !candidate.exists() {
                return Ok(Priorities::builtin());
            }
            candidate
        }
    };
    Priorities::load(&path).map_err(|error| format!("{error}"))
}

/// 读一台设备的**脚印**（[`Footprint`](super::Footprint)）：读选择集、折事实、求值、读成员与内部构成。
///
/// 目标设置弹层打开时往任务台上排的就是这一趟（票 `gui-looks-like-the-design/21`）：折事实走一遍全库，
/// 不能跑在画帧那条线程上。拿到之后换档案、改按平台覆盖都是纯的（[`Footprint::desired`](super::Footprint::desired)）。
///
/// `workspace` 是工作目录：认多碟变体的各张碟要那里的平台清单（与 [`prepare_selected`] 同一份），弹层里说「放不下」
/// 的那几张碟于是与差量预览逐碟对得上（票 `verdict-store-and-sync/19`）。
///
/// 整条只读；被叫停时停在哪儿都是干净的。
///
/// # Errors
/// 中立库或平台清单读不动时返回 [`Cutoff::Failed`]，被叫停时返回 [`Cutoff::Halted`]。
pub fn footprint(
    catalog: &Catalog,
    workspace: &Path,
    name: &str,
    task: &Handle,
) -> Result<super::Footprint, Cutoff> {
    task.steps(FOOTPRINT_STEPS);
    task.step("读选择集")?;
    let loaded = catalog
        .selection(name)
        .map_err(|error| format!("中立库读不动：{error}"))?;
    task.step("折事实")?;
    let facts = sublibrary::facts(catalog).map_err(|error| format!("中立库读不动：{error}"))?;
    task.step("求值选择集")?;
    let selected = sublibrary::select(&loaded.selection, &facts);
    task.step("读成员")?;
    Ok(gather(catalog, workspace, &selected)?.0)
}

/// 读平台清单、再读脚印：[`footprint`] 与 [`prepare_selected`] 两条路认各张碟用的都是工作目录里那同一份平台清单
/// （票 `verdict-store-and-sync/19`）。清单交回去，给多碟变体的播放列表起名还要它。
///
/// **平台清单哪个前端都要读**：多碟变体每张碟都照能力档案转格式，认各张碟要它（`shape::discs`，与扫描、成型同一条
/// 查法）——转格式问的是模拟器吃不吃，与前端用不用得上播放列表无关。
fn gather(
    catalog: &Catalog,
    workspace: &Path,
    selected: &Selected,
) -> Result<(super::Footprint, PlatformManifest), String> {
    let platform_manifest =
        crate::sources::manifest(workspace).map_err(|error| format!("平台清单读不动：{error}"))?;
    let footprint = super::Footprint::gather(catalog, selected, &platform_manifest)
        .map_err(|error| format!("中立库读不动：{error}"))?;
    Ok((footprint, platform_manifest))
}

/// [`footprint`] 一共几步。**改了它里头的 `task.step` 就得改这个数。**
pub const FOOTPRINT_STEPS: u32 = 4;

/// 数 `target` 上**清单之外**的文件有几个、多大（[`strangers`](super::strangers)）：清单是 `name` 这个子库记着的那一份，
/// 还没建的子库没有清单，卡上的都算。目标设置弹层里那句「目录里已有 N 个文件，它们不在清单里，工具不会改动」数的就是它
/// （票 `gui-looks-like-the-design/21`，拿主意的人 2026-09-15 定）。`target` 可以是框里还没存下来的那一条。
///
/// **只读遍历目标**（[`observe`](super::observe())）：卡上文件多时是一趟长活，排到任务台上跑，不在画帧那条线程上跑；
/// 被叫停时一个字节都没写。
///
/// # Errors
/// 目标列不开、中立库读不动时返回 [`Cutoff::Failed`]，被叫停时返回 [`Cutoff::Halted`]。
pub fn strangers_at(
    catalog: &Catalog,
    name: &str,
    target: &Path,
    task: &Handle,
) -> Result<super::Strangers, Cutoff> {
    task.steps(STRANGERS_STEPS);
    task.step("读清单")?;
    let manifest = catalog
        .manifest(name)
        .map_err(|error| format!("中立库读不动：{error}"))?;
    task.step("看一眼目标")?;
    let actual = super::observe(&RealFs, target).map_err(|error| format!("{error}"))?;
    Ok(super::strangers(&manifest, &actual))
}

/// [`strangers_at`] 一共几步。**改了它里头的 `task.step` 就得改这个数。**
pub const STRANGERS_STEPS: u32 = 2;

/// 这一条线一共几步。**改了下面的 `task.step` 就得改这个数**，不然进度条会走过头。
/// `tests/task.rs::排差量预览一路报得出走到第几步` 盯着这两个数对不对得上。
const STEPS: u32 = 4 + PLAN_STEPS;

/// [`prepare_selected`] 那半条线一共几步。**改了它里头的 `step` 就得改这个数。**
///
/// 调它的人各自报总步数（[`prepare`] 是读选择集那四步加这几步，
/// [`sublibrary::survey`] 是折事实一步加每台设备各这几步），所以它得是个说得出口的数。
pub const PLAN_STEPS: u32 = 8;

/// 把中立库、媒体池与目标设备折成一份计划。**除了目标目录，什么都不写。**
///
/// ## 报进度、能停
///
/// `task` 是这一趟的**把手**（[`crate::task::Handle`]）：每走完一步报一次，
/// 每两步之间看一眼有没有被叫停。**这条线整条只读**，所以被叫停时停在哪儿都是干净的
/// ——中立库、媒体池、目标设备三处一个字节都没动，那份没排完的计划直接丢掉就是。
/// 也因此它没有「续跑」这回事：再排一次就是从头排一次（几百毫秒的活）。
///
/// 不想要把手的调用方给一个 [`Handle::new`](crate::task::Handle::new) 就行——
/// 没人按停下，它就只是白记几行进度。
///
/// ## 两半
///
/// 前半截读子库、求值选择集；后半截对着那份求过值的选择集排计划
/// （[`prepare_selected`]）。拆开是因为**「装得下吗」也要走后半截**：子库报告
/// （[`sublibrary::survey`]）一趟折一次事实、全部设备共用，再各自走一遍后半截——
/// 于是报告里那个数与这里排出来的计划是**同一条线**算的，不是两条各自演化的代码（挂账 D76）。
///
/// # Errors
/// 子库不在、前端格式没有适配器、中立库读不动、目标看不了（或者平台清单读不动，见 [`prepare_selected`]）时返回
/// [`PlanCutoff::Unplanned`]（带着结构化的原因）；被叫停时返回
/// [`PlanCutoff::Halted`]——**那是两个不同的支，不是两句
/// 不同的话**，任务台按它分「停了」与「失败」。
pub fn prepare(
    catalog: &Catalog,
    workspace: &Path,
    name: &str,
    request: &Request<'_>,
    task: &Handle,
) -> Result<Prepared, PlanCutoff> {
    task.steps(STEPS);
    task.step("读子库")?;
    let sublibrary = catalog
        .sublibrary(name)
        .map_err(|error| format!("中立库读不动：{error}"))?
        .ok_or_else(|| Unplanned::NoSublibrary {
            name: name.to_string(),
        })?;
    task.step("读选择集")?;
    let loaded = catalog
        .selection(name)
        .map_err(|error| format!("中立库读不动：{error}"))?;
    // **这一步是最长的那一步**（真机量级上占大头：它走一遍全库）。把手在两步之间生效，
    // 所以「按下停下」到「真的停了」之间最坏就是这一步的长度。
    task.step("折事实")?;
    let facts = sublibrary::facts(catalog).map_err(|error| format!("中立库读不动：{error}"))?;
    task.step("求值选择集")?;
    let selected = sublibrary::select(&loaded.selection, &facts);
    let mut prepared = prepare_selected(
        catalog,
        workspace,
        sublibrary,
        &selected,
        request,
        &|step| task.step(step),
    )?;
    // 读不懂的规则是选择集那半截的账，后半截看不见它。
    prepared.broken = loaded.broken.len();
    Ok(prepared)
}

/// **对着一份已经求过值的选择集排计划**：看一眼目标 → 读能力档案 → 折期望状态 →
/// 按目标存储筛一遍（连多碟变体的播放列表）→ 铺媒体 → 折前端元数据 → 读清单 → 排计划。
///
/// [`prepare`] 的后半截，也是「装得下吗」唯一的那条线（[`sublibrary::fit`]）。
/// 收 [`Selected`] 而不是去库里读选择集，是因为问「装得下吗」的不止存着的那一套：
/// 一趟折一次事实、好几台设备共用，或者「加上这条规则之后装不装得下」。
///
/// **先看一眼目标**：卡不在位时后面几步全是白折（期望状态、媒体、前端元数据都要读中立库），
/// 而一次问好几台设备时，不在手边的往往不止一台。
///
/// `step` 是每走一步报一次的那个口子，[`PLAN_STEPS`] 步。给的是闭包而不是把手，
/// 是因为调用方要在步名前面加上是哪一台设备——把手自己不知道。
///
/// # Errors
/// 目标看不了、前端格式没有适配器、中立库读不动、平台清单读不动时返回
/// [`PlanCutoff::Unplanned`]；`step` 说停下时返回 [`PlanCutoff::Halted`]。
pub fn prepare_selected(
    catalog: &Catalog,
    workspace: &Path,
    mut sublibrary: Sublibrary,
    selected: &Selected,
    request: &Request<'_>,
    step: &dyn Fn(&str) -> Result<(), Halted>,
) -> Result<Prepared, PlanCutoff> {
    // **读盘用的路径与入库比较用的键分开**（ADR-0020）：`--target` 给的是系统给的
    // 原始形式，就拿它原样去读；子库自己那条走 `read_path`，它取的正是存进库的
    // 那一份原始形式。混用会让带假名或带音标的目标目录 `canonicalize` 失败，
    // 然后被报成「卡不在位」——那正是最不该说的谎（挂账 D82）。
    let root = match request.target {
        Some(target) => path::normalize_existing(target),
        None => sublibrary.read_path(),
    };
    if request.target.is_some() {
        sublibrary.target = path::nfc(&path::display(&root)).into_owned();
    }
    step("看一眼目标")?;
    let actual = super::observe(&RealFs, &root)?;
    let adapter = sublibrary.adapter()?;
    // **能力档案**：目标吃得下什么、这张卡放得下什么（票 21、ADR-0017）。
    // 子库记的是名字，档案本身是一份可以整份换掉的数据。
    step("读能力档案")?;
    let roster = Roster::in_workspace(workspace).map_err(|error| format!("{error}"))?;
    let missing_capability = sublibrary
        .capability
        .as_deref()
        .filter(|name| roster.find(name).is_none())
        .map(ToString::to_string);
    // **这个子库自己的按平台覆盖叠上去**（票 `gui-looks-like-the-design/21`）：只影响这一台，名册里那份不动。
    let overrides = catalog
        .capability_overrides(&sublibrary.name)
        .map_err(|error| format!("中立库读不动：{error}"))?;
    let profile = roster
        .find_or_unclaimed(sublibrary.capability.as_deref())
        .with_overrides(&overrides);
    // **报告里印真正生效的那一份，不是子库上记着的那个名字。** 记着的名字在名册里
    // 找不到时上面已经退回了「不作声称」——这时预览与 `--json` 还印着原来那个名字的话，
    // 用户会以为它替自己查过了，而实际上一条都没查（ADR-0017：矩阵错误比不转换更糟）。
    sublibrary.capability = Some(profile.name.clone());
    // **这一趟真用上的容量上限**（票 `gui-looks-like-the-design/21`）：按设备容量那一档跟着这张卡此刻的总量；本机磁盘不设上限
    // 时按剩余空间算（目标现占加上还写得下的）。判断只有一处（`Sublibrary::limit_on`），计划里比「超没超」用的就是这个数。
    // 设了数的自定义那一档不必去看卷。
    let volume = (sublibrary.capacity_by_device || sublibrary.capacity.is_none())
        .then(|| crate::sublibrary::target::volume(&root));
    sublibrary.capacity = sublibrary.limit_on(volume.as_ref(), actual.bytes());
    let priorities = priorities(request.priorities, workspace)?;
    // **不建目录**：排计划那条命令说的是「一个文件都没写」。
    let pool = MediaPool::at(&workspace::media_pool_dir(workspace));

    step("折期望状态")?;
    // 脚印留着（与 `super::desired` 是同一条线，ADR-0024）：给多碟变体折播放列表还要它认好的各张碟与平台。
    let (footprint, platform_manifest) = gather(catalog, workspace, selected)?;
    let mut desired = footprint.desired(&profile);
    // **放不进目标存储的在这里就被拦下来**（ADR-0017 补充段）：FAT32 那 4 GiB 的
    // 单文件上限、文件名不收的字符、路径太长。拦在排计划**之前**，于是它们连成为一条
    // 步骤的路径都没有——「传到一半失败」这件事在构造上不会发生。
    //
    // 路径上限比的是**完整路径**，因此把子库根那串的长度也交进去。
    //
    // **先筛 ROM 那一类**（票 `verdict-store-and-sync/18`）：多碟变体的播放列表只列筛下来还在的碟，而前端条目与
    // 媒体的名字又要先知道哪几份播放列表真落得下——三样排成一串，ROM 打头，后面每进来一截再筛一遍
    // （`Desired::add_and_screen`）。
    step("按目标存储筛一遍")?;
    let prefix_chars = path::display(&root).encode_utf16().count();
    desired.screen(&profile.filesystem, prefix_chars);
    // 生成物：相对子库根的路径 → 字节。多碟变体的播放列表先进来，前端元数据后进来。
    let mut generated = BTreeMap::new();
    // **多碟变体的播放列表排在筛过之后**（`playlist` 模块文档）：它列的是卡上真落着的那几张碟，有一张放不进目标
    // 的那一套就不生成（用不上播放列表的前端一份都不生成）。折出来的那几份照同一份文件系统声明再筛一遍——它们自己也
    // 可能撞车、名字太长。
    let playlists = super::playlist::lay(
        &footprint,
        &desired,
        adapter.as_ref(),
        &profile,
        &platform_manifest,
    );
    if !playlists.files.is_empty() {
        desired.add_and_screen(
            playlists.files.iter().cloned(),
            &profile.filesystem,
            prefix_chars,
        );
    }
    // **选中的变体在卡上落成什么样**照筛过之后的期望状态定（[`super::Footprint::launching`]）：卡上有播放列表的启动它，
    // 主文件转了格式的启动转出来那一份（票 `verdict-store-and-sync/20`）；主文件放不进目标的压根没上卡（票
    // `verdict-store-and-sync/21`）。前端元数据照它写条目、不列没上卡的，媒体照它起名、不铺没上卡的。
    let on_card = footprint.launching(&desired, &playlists);
    generated.extend(playlists.bytes);
    step("铺媒体")?;
    let mut media = super::media::lay(catalog, adapter.as_ref(), &pool, selected, &on_card)
        .map_err(|error| format!("中立库读不动：{error}"))?;
    step("折前端元数据")?;
    let frontend = super::frontend::lay(
        catalog,
        adapter.as_ref(),
        &priorities,
        selected,
        &media.assets,
        &on_card,
    )
    .map_err(|error| format!("元数据折不出来：{error}"))?;
    // 媒体与前端元数据最后进来、再筛一遍。ROM 与播放列表在这一遍里结论不变（`on_card` 因此不必重取）：它们落在平台目录里，
    // 媒体落在 `downloaded_media/`、`media/` 下，元数据落在 `gamelists/` 下或子库根上，路径撞不到一起。
    desired.add_and_screen(
        media.files.iter().chain(&frontend.files).cloned(),
        &profile.filesystem,
        prefix_chars,
    );
    generated.extend(frontend.bytes);

    step("读清单")?;
    let manifest = catalog
        .manifest(&sublibrary.name)
        .map_err(|error| format!("中立库读不动：{error}"))?;
    // **落点的目录段先与目标折齐**（`sync::align`）。卡上那个 `gb/` 与我们键里的
    // `GB/`，在不分大小写的目标上是同一个目录：不折的话文件落进 `gb/`、清单记成
    // `GB/`，第二趟起工具就认不出自己放的那一份。改名表要原样落到媒体与生成物那两张
    // 以落点为键的表上——挪了这边不挪那边，执行时会报「在媒体池里找不到落点」。
    let realign = super::align(&mut desired, &actual);
    realign.apply(&mut media.from_pool);
    realign.apply(&mut generated);
    step("排计划")?;
    let plan = super::plan(
        &sublibrary,
        &desired,
        &manifest,
        &actual,
        Options {
            restore_missing: request.restore_missing,
        },
    );
    Ok(Prepared {
        sublibrary,
        root,
        selected: selected.clone(),
        desired,
        manifest,
        actual,
        plan,
        from_pool: media.from_pool,
        generated,
        scratch: pool.scratch(),
        // 读不懂的规则几条是求值那半截的账：这里收的已经是求过值的选择集，由 [`prepare`] 填。
        broken: 0,
        media_not_in_pool: media.not_in_pool,
        media_unknown_kind: media.unknown_kind,
        media_crowded_out: media.crowded_out,
        entries: frontend.entries,
        left_off: on_card.left_off,
        missing_capability,
        stale_claims: profile.stale_claims(&today()),
    })
}

impl Prepared {
    /// 这一趟要不要真的去读**主库**。
    ///
    /// **只有真要搬 ROM 时才需要**：一趟只删文件、只重写元数据、或者一步都不用做的同步，
    /// 盘不在位照样跑得完（ADR-0009 那句「扫描是唯一需要盘在位的操作」）。
    #[must_use]
    pub fn needs_library(&self) -> bool {
        self.plan
            .steps
            .iter()
            .any(|step| step.kind == super::FileKind::Rom && step.act != super::Act::Delete)
    }
}

/// 这份中立库对着的**一组根**：先按扫描时记下的那份，再让 `overrides` 覆盖上去。
///
/// `overrides` 是 `(根名, 路径)`：命令行的 `--library-root [根名=]路径` 走这条。根名给
/// `None` 时只在**这份库只有一个根**的时候算数——那时「哪个根」没有歧义；多于一个根
/// 却不说名字，覆盖谁都是猜。
///
/// **覆盖只换得动已有的根**（[`Roots::relocate`]）：点到一个库里没有的名字就当场说
/// 「没有这个根」并把有的那几个列出来。加一个根是 `scan` 的活——从这条缝溜进来的话，
/// `--library-root 主庫=/新位置`（打错一个字）会静默多出一个根，而后面报的是
/// 「根『主库』不在位」，指的是另一件事。
///
/// 一个根都还没有时是唯一的例外：那份库从没扫过，`--library-root 路径` 就是在说
/// 主库在哪，收下它没有任何歧义。
///
/// # Errors
/// 中立库读不动、点了一个不存在的根名、或者不说名字却有不止一个根时返回一句给人看的话。
pub fn library_roots(
    catalog: &Catalog,
    overrides: &[(Option<String>, PathBuf)],
) -> Result<Roots, String> {
    let mut roots = Roots::load(catalog).map_err(|error| format!("中立库读不动：{error}"))?;
    for (name, path) in overrides {
        let path = path::normalize_existing(path);
        if name.is_none() && roots.is_empty() {
            roots.set("主库", path);
            continue;
        }
        roots
            .relocate(name.as_deref(), path)
            .map_err(|error| error.hint("点名换位置：`--library-root 根名=路径`"))?;
    }
    Ok(roots)
}

/// 这一趟真要读的那几个**根**里，哪些不在位。
///
/// 只看这一趟真的要搬的那些 ROM 落在哪个根上——**别的根挂没挂上与这一趟无关**。
/// 那正是「主库是一组根」比「一个目录」好的地方：甲盘不在位不该挡住只动乙盘的同步。
#[must_use]
pub fn missing_roots(prepared: &Prepared, roots: &Roots) -> Vec<String> {
    let mut missing: BTreeSet<String> = BTreeSet::new();
    for step in &prepared.plan.steps {
        if step.kind != super::FileKind::Rom || step.act == super::Act::Delete {
            continue;
        }
        let name = path::root_of_key(&step.source);
        let present = roots.path_of(name).is_some_and(Path::is_dir);
        if !present {
            missing.insert(name.to_string());
        }
    }
    missing.into_iter().collect()
}

/// 这几个根未连接时该对人说的那句话。
///
/// 界面与命令行都印它，所以**只说事实，不带命令行的旗标**（挂单 `Q622` 那一族）：
/// 「或者给 `--library-root`」是命令行自己的去处，由命令行补。
#[must_use]
pub fn missing_roots_message(missing: &[String]) -> String {
    format!(
        "主库这几个根未连接：{}。\n搬 ROM 要真的去读它们，插上外置盘再来。",
        missing.join("、")
    )
}

/// 目标落在主库里、或者把主库的根包在里面，就拦下来。
///
/// **只有真要动手那一步需要这一道。** 排计划从头到尾只读，指哪儿都无所谓；而同步是真的
/// 往目标上建目录、写文件、删文件——一个手滑的目标路径就会在那块好几 TiB、不可再生的盘里
/// 动手（ADR-0004；多大见台账 `docs/library-facts.md`）。判据用中立库记着的主库根，于是给不给主库根都拦得住。
/// **取不到主库根时不拦**：那说明这份库还没扫过，没有边界可守。
///
/// **把根包在里面也拦**：同步往 `<平台目录>/…` 写，平台目录与根同名时就写进了主库。
///
/// 它在核心里而不在命令行里，是因为**界面也有一个「同步」按钮**——这道红线不能靠
/// 每个壳自己记得写一遍。判的那一下是 [`library_overlap`]：新建子库、改目标设置时当场判的也是它
/// （[`sublibrary::target::vet`]，ADR-0024）。
///
/// **判据先把两边折成可比形态**（[`path::is_inside_place`]）：目标过了
/// [`path::normalize_existing`]，Windows 上于是是 `\\?\D:\…`，而库里的根存的是
/// display 形态 `D:\…`——不折的话这道红线恒为 false，等于没有。
///
/// # Errors
/// 目标与主库的根撞在一起、或者中立库读不动时返回一句给人看的话。
pub fn refuse_target_in_library(
    catalog: &Catalog,
    overrides: &[(Option<String>, PathBuf)],
    target: &Path,
) -> Result<(), String> {
    let roots = library_roots(catalog, overrides)?;
    let target = path::normalize_existing(target);
    // **每个根都要拦。** 一份中立库装着几块盘，只拦其中一块等于另外几块没人守。
    let Some(overlap) = library_overlap(&roots, &target) else {
        return Ok(());
    };
    // **话也只有一份**：与存下来那一刻拦下时同一句（[`TargetRefusal`] 的 `Display`，设计稿 `probePath`），
    // 不在同步那一刻另说一句长话；连着路径说的那一句与命令行 `sublibrary set` 拦下时同一处（[`TargetRefusal::sentence`]）。
    let refusal = TargetRefusal::InLibrary {
        root: overlap.root,
        place: path::display(&overlap.place),
        around: overlap.around,
    };
    Err(refusal.sentence(&target))
}
