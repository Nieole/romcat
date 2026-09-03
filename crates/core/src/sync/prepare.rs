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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::adapter;
use crate::capability::{Roster, today};
use crate::catalog::Catalog;
use crate::fs::RealFs;
use crate::path;
use crate::scrape::Priorities;
use crate::scrape::pool::MediaPool;
use crate::sublibrary::{self, Selected, Sublibrary};
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

/// 把中立库、媒体池与目标设备折成一份计划。**除了目标目录，什么都不写。**
///
/// # Errors
/// 子库不在、前端格式没有适配器、中立库读不动、目标看不了时返回一句给人看的话。
pub fn prepare(
    catalog: &Catalog,
    workspace: &Path,
    name: &str,
    request: &Request<'_>,
) -> Result<Prepared, String> {
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

    let loaded = catalog
        .selection(name)
        .map_err(|error| format!("中立库读不动：{error}"))?;
    let facts = sublibrary::facts(catalog).map_err(|error| format!("中立库读不动：{error}"))?;
    let selected = sublibrary::select(&loaded.selection, &facts);

    let mut desired = super::desired(catalog, &selected, &profile)
        .map_err(|error| format!("中立库读不动：{error}"))?;
    let media = super::media::lay(catalog, adapter.as_ref(), &pool, &selected)
        .map_err(|error| format!("中立库读不动：{error}"))?;
    let frontend = super::frontend::lay(
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
    desired.screen(
        &profile.filesystem,
        path::display(&root).encode_utf16().count(),
    );

    let manifest = catalog
        .manifest(name)
        .map_err(|error| format!("中立库读不动：{error}"))?;
    let actual = super::observe(&RealFs, &root).map_err(|error| format!("{error}"))?;
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

/// 这份中立库对着的主库根：给了就用给的，否则用扫描时记下的那个。
///
/// # Errors
/// 中立库读不动时返回一句给人看的话。
pub fn recorded_library_root(
    catalog: &Catalog,
    given: Option<&Path>,
) -> Result<Option<PathBuf>, String> {
    match given {
        Some(root) => Ok(Some(path::normalize_existing(root))),
        None => catalog
            .library_root()
            .map(|root| root.map(PathBuf::from))
            .map_err(|error| format!("中立库读不动：{error}")),
    }
}

/// 这一趟去哪儿读主库；说不出来或者盘不在位时返回一句给人看的话。
///
/// # Errors
/// 这份中立库没记着主库在哪、或者那个目录不在位时返回错误。
pub fn library_root(catalog: &Catalog, given: Option<&Path>) -> Result<PathBuf, String> {
    let root = recorded_library_root(catalog, given)?.ok_or_else(|| {
        "这份中立库没记着主库在哪。给 `--library-root <主库根目录>`，\n\
         或者先跑一次 `romcat scan` 让它记下来。"
            .to_string()
    })?;
    if !root.is_dir() {
        return Err(format!(
            "主库不在位：{}\n\
             搬 ROM 要真的去读它。插上外置盘，或者给 `--library-root <主库根目录>`。",
            path::display(&root)
        ));
    }
    Ok(root)
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
/// # Errors
/// 目标落在主库里、或者中立库读不动时返回一句给人看的话。
pub fn refuse_target_in_library(
    catalog: &Catalog,
    given: Option<&Path>,
    target: &Path,
) -> Result<(), String> {
    let Some(root) = recorded_library_root(catalog, given)? else {
        return Ok(());
    };
    if path::is_inside(&root, &path::normalize_existing(target)) {
        return Err(format!(
            "目标 {} 落在主库里。**主库只读**（ADR-0004）：同步会往目标上写文件、\n\
             删文件，绝不能指着那块盘。子库要导到别处去——一律走读卡器（ADR-0015）。",
            path::display(target)
        ));
    }
    Ok(())
}
