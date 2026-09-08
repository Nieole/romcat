//! 把中立库、**媒体池**与目标设备折成一份 [`Plan`]——也就是**差量预览**。
//!
//! ## 为什么这一步在核心里
//!
//! 它是十来个步骤串起来的一条线：读选择集 → 折事实 → 求值 → 折期望状态 → 铺媒体 →
//! 折前端元数据 → 按目标存储筛一遍 → 读清单 → 看一眼目标 → 排计划。每一步都是领域
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

use crate::adapter;
use crate::capability::{Roster, today};
use crate::catalog::{Catalog, Roots};
use crate::fs::RealFs;
use crate::path;
use crate::scrape::Priorities;
use crate::scrape::pool::MediaPool;
use crate::sublibrary::{self, Selected, Sublibrary};
use crate::task::{Cutoff, Handle};
use crate::workspace;

use super::{Desired, Manifest, Options, Plan, TargetState};

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
    /// 生成物：相对子库根的路径 → 内容。前端元数据走这条。
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
    /// 子库记着的能力档案在眼下这份名册里找不到——退回了「不作声称」。
    pub missing_capability: Option<String>,
    /// 这份档案里有几条声明已经陈旧。
    pub stale_claims: usize,
}

impl Prepared {
    /// 折期望状态时那几件**要说出口**的怪事，每条一段话。
    ///
    /// **命令行与界面印同一份。** 各写一遍的话，界面上会少掉其中一两条——而这几条正是
    /// 「为什么这一趟少选出来这么多」的答案，少印一条就等于让人对着一个说不通的数字发呆。
    #[must_use]
    pub fn concerns(&self) -> Vec<String> {
        let name = &self.sublibrary.name;
        let mut out = Vec::new();
        if self.broken > 0 {
            out.push(format!(
                "⚠️ 有 {} 条规则读不懂、这一趟没参与求值——少选出来的东西全在它们里面。\n\
                 `romcat sublibrary show {name}` 看是哪几条。",
                crate::report::thousands(self.broken as u64),
            ));
        }
        if self.media_not_in_pool > 0 {
            out.push(format!(
                "⚠️ 有 {} 条媒体引用在**媒体池**里找不到那个文件，这一趟一张都不铺。\n\
                 重新跑一次 `romcat scrape` 把它们收回池里。",
                crate::report::thousands(self.media_not_in_pool),
            ));
        }
        if self.media_unknown_kind > 0 {
            out.push(format!(
                "认不出是什么的图有 {} 张，**一张都没铺**——猜错了就是把说明书当封面。",
                crate::report::thousands(self.media_unknown_kind),
            ));
        }
        if self.media_crowded_out > 0 {
            out.push(format!(
                "有 {} 张图被同类挤掉、没铺出去：这个格式靠**文件名**找媒体，\n\
                 一个游戏的一个类型只放得下一张。挤掉的是同一个游戏的第二张起。",
                crate::report::thousands(self.media_crowded_out),
            ));
        }
        if let Some(missing) = &self.missing_capability {
            out.push(format!(
                "⚠️ 子库记着的能力档案「{missing}」在眼下这份名册里**找不到**，这一趟退回了\n\
                 「不作声称」：**不转换、也不检查**。别以为它替你查过了。\n\
                 `romcat capability` 看还有哪些，`romcat sublibrary set {name} --capability <名字>` 重挑一份。",
            ));
        }
        if self.stale_claims > 0 {
            out.push(format!(
                "⚠️ 这份能力档案里有 {} 条声明**超过半年没核实**。模拟器一年发好几版，\n\
                 而矩阵错了比不转换更糟（ADR-0017）——`romcat capability <档案名>` 看是哪几条。",
                crate::report::thousands(self.stale_claims as u64),
            ));
        }
        out
    }
}

/// 工作目录里那份可选的优先级表叫什么。
const PRIORITIES_IN_WORKSPACE: &str = "priorities.toml";

/// 挑出这一趟用哪一份优先级表：给了的 > 工作目录里那份 > 内置的。
///
/// **刮削、标题、导出与同步共用它，而且必须共用**：几条路对「哪个源说了算」的答案
/// 不一样的话，报告里合并出来的标题与导出时挑出来的标题就会对不上。
///
/// # Errors
/// 那份表读不动或者写坏了时返回一句给人看的话。
pub fn priorities(given: Option<&Path>, workspace: &Path) -> Result<Priorities, String> {
    let path = match given {
        Some(path) => path.to_path_buf(),
        None => {
            let candidate = workspace.join(PRIORITIES_IN_WORKSPACE);
            if !candidate.exists() {
                return Ok(Priorities::builtin());
            }
            candidate
        }
    };
    Priorities::load(&path).map_err(|error| format!("{error}"))
}

/// 这一条线一共几步。**改了下面的 `task.step` 就得改这个数**，不然进度条会走过头。
/// `tests/task.rs::排差量预览一路报得出走到第几步` 盯着这两个数对不对得上。
const STEPS: u32 = 12;

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
/// # Errors
/// 子库不在、前端格式没有适配器、中立库读不动、目标看不了时返回
/// [`Cutoff::Failed`]；被叫停时返回
/// [`Cutoff::Halted`]——**那是两个不同的支，不是两句
/// 不同的话**，任务台按它分「停了」与「失败」。
pub fn prepare(
    catalog: &Catalog,
    workspace: &Path,
    name: &str,
    request: &Request<'_>,
    task: &Handle,
) -> Result<Prepared, Cutoff> {
    task.steps(STEPS);
    task.step("读子库")?;
    let mut sublibrary = catalog
        .sublibrary(name)
        .map_err(|error| format!("中立库读不动：{error}"))?
        .ok_or_else(|| {
            format!(
                "没有叫「{name}」的子库。\n\
                 建一个：`romcat sublibrary set {name} --target <目标设备上的目录>`"
            )
        })?;
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
    let adapter = adapter::find(&sublibrary.format).ok_or_else(|| {
        format!(
            "子库「{name}」的前端格式是「{}」，可这一版没带这个适配器。",
            sublibrary.format
        )
    })?;
    // **能力档案**：目标吃得下什么、这张卡放得下什么（票 21、ADR-0017）。
    // 子库记的是名字，档案本身是一份可以整份换掉的数据。
    task.step("读能力档案")?;
    let roster = Roster::in_workspace(workspace).map_err(|error| format!("{error}"))?;
    let missing_capability = sublibrary
        .capability
        .as_deref()
        .filter(|name| roster.find(name).is_none())
        .map(ToString::to_string);
    let profile = roster.find_or_unclaimed(sublibrary.capability.as_deref());
    // **报告里印真正生效的那一份，不是子库上记着的那个名字。** 记着的名字在名册里
    // 找不到时上面已经退回了「不作声称」——这时预览与 `--json` 还印着原来那个名字的话，
    // 用户会以为它替自己查过了，而实际上一条都没查（ADR-0017：矩阵错误比不转换更糟）。
    sublibrary.capability = Some(profile.name.clone());
    let priorities = priorities(request.priorities, workspace)?;
    // **不建目录**：排计划那条命令说的是「一个文件都没写」。
    let pool = MediaPool::at(&workspace::media_pool_dir(workspace));

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

    task.step("折期望状态")?;
    let mut desired = super::desired(catalog, &selected, &profile)
        .map_err(|error| format!("中立库读不动：{error}"))?;
    task.step("铺媒体")?;
    let mut media = super::media::lay(catalog, adapter.as_ref(), &pool, &selected)
        .map_err(|error| format!("中立库读不动：{error}"))?;
    task.step("折前端元数据")?;
    let mut frontend = super::frontend::lay(
        catalog,
        adapter.as_ref(),
        &priorities,
        &selected,
        &media.assets,
    )
    .map_err(|error| format!("元数据折不出来：{error}"))?;
    desired.files.extend(media.files.iter().cloned());
    desired.files.extend(frontend.files.iter().cloned());
    desired.files.sort_by(|a, b| a.path.cmp(&b.path));
    // **放不进目标存储的在这里就被拦下来**（ADR-0017 补充段）：FAT32 那 4 GiB 的
    // 单文件上限、文件名不收的字符、路径太长。拦在排计划**之前**，于是它们连成为一条
    // 步骤的路径都没有——「传到一半失败」这件事在构造上不会发生。
    //
    // 路径上限比的是**完整路径**，因此把子库根那串的长度也交进去。
    task.step("按目标存储筛一遍")?;
    desired.screen(
        &profile.filesystem,
        path::display(&root).encode_utf16().count(),
    );

    task.step("读清单")?;
    let manifest = catalog
        .manifest(name)
        .map_err(|error| format!("中立库读不动：{error}"))?;
    task.step("看一眼目标")?;
    let actual = super::observe(&RealFs, &root).map_err(|error| format!("{error}"))?;
    // **落点的目录段先与目标折齐**（`sync::align`）。卡上那个 `gb/` 与我们键里的
    // `GB/`，在不分大小写的目标上是同一个目录：不折的话文件落进 `gb/`、清单记成
    // `GB/`，第二趟起工具就认不出自己放的那一份。改名表要原样落到媒体与生成物那两张
    // 以落点为键的表上——挪了这边不挪那边，执行时会报「在媒体池里找不到落点」。
    let realign = super::align(&mut desired, &actual);
    realign.apply(&mut media.from_pool);
    realign.apply(&mut frontend.bytes);
    task.step("排计划")?;
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
        selected,
        desired,
        manifest,
        actual,
        plan,
        from_pool: media.from_pool,
        generated: frontend.bytes,
        scratch: pool.scratch(),
        broken: loaded.broken.len(),
        media_not_in_pool: media.not_in_pool,
        media_unknown_kind: media.unknown_kind,
        media_crowded_out: media.crowded_out,
        entries: frontend.entries,
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

/// 这几个根不在位时该对人说的那句话。
#[must_use]
pub fn missing_roots_message(missing: &[String]) -> String {
    format!(
        "主库这几个根不在位：{}。\n\
         搬 ROM 要真的去读它们。插上外置盘，或者给 `--library-root 根名=路径`。",
        missing.join("、")
    )
}

/// 目标落在主库里就拦下来。
///
/// **只有真要动手那一步需要这一道。** 排计划从头到尾只读，指哪儿都无所谓；而同步是真的
/// 往目标上建目录、写文件、删文件——一个手滑的目标路径就会在那块 10 TiB 不可再生的盘里
/// 动手（ADR-0004）。判据用中立库记着的主库根，于是给不给主库根都拦得住。
/// **取不到主库根时不拦**：那说明这份库还没扫过，没有边界可守。
///
/// 它在核心里而不在命令行里，是因为**界面也有一个「同步」按钮**——这道红线不能靠
/// 每个壳自己记得写一遍。
///
/// **判据先把两边折成可比形态**（[`path::is_inside_place`]）：目标过了
/// [`path::normalize_existing`]，Windows 上于是是 `\\?\D:\…`，而库里的根存的是
/// display 形态 `D:\…`——不折的话这道红线恒为 false，等于没有。
///
/// # Errors
/// 目标落在主库里、或者中立库读不动时返回一句给人看的话。
pub fn refuse_target_in_library(
    catalog: &Catalog,
    overrides: &[(Option<String>, PathBuf)],
    target: &Path,
) -> Result<(), String> {
    let roots = library_roots(catalog, overrides)?;
    let target = path::normalize_existing(target);
    // **每个根都要拦。** 一份中立库装着几块盘，只拦其中一块等于另外几块没人守。
    for (name, root) in roots.iter() {
        if path::is_inside_place(root, &target) {
            return Err(format!(
                "目标 {} 落在主库的根「{name}」（{}）里。**主库只读**（ADR-0004）：\n\
                 同步会往目标上写文件、删文件，绝不能指着那块盘。\n\
                 子库要导到别处去——一律走读卡器（ADR-0015）。",
                path::display(&target),
                path::display(root),
            ));
        }
    }
    Ok(())
}
