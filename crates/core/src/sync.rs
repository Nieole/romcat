//! **同步计划器**：三方对比算出一份操作计划，而那份计划**就是差量预览**。
//!
//! `CONTEXT.md` 的**同步**词条把这件事定死成三方对比：**主库该有的 / 清单记录的 /
//! 目标上实际有的**。[`plan`] 的三个输入正是这三样——[`Desired`]、[`Manifest`]、
//! [`TargetState`]。
//!
//! ## 预览与计划是同一个东西，不是两套要保持同步的东西
//!
//! ADR-0016 把差量预览定成**硬要求不是优化项**：「永远不能点了同步就开始传」。
//! 这里**不另做一份预览**——[`plan`] 的返回值 [`Plan`] 自己带着全部的账（新增几个
//! 占多少容量、删除几个腾出多少容量、净变化多少），`--json` 打出来的就是它，
//! 文本那一份只是把它排一遍版（[`report`]）。于是「预览说的」与「同步会做的」
//! 不可能对不上：它们是同一个值。
//!
//! ## 零 IO，于是「只碰清单里记录过的文件」是一条**可断言的性质**
//!
//! [`plan`] 不碰磁盘、不碰中立库、不看时钟。把中立库折成期望状态的是 [`desired`]，
//! 把目标设备折成实际状态的是 [`observe`]，两个都在外面——与
//! [`sublibrary::select`](crate::sublibrary::select) 与
//! [`sublibrary::facts`](crate::sublibrary::facts) 是同一道缝。
//!
//! 这道缝是唯一能守住 ADR-0015 那条硬约束的地方：**只碰清单里记录过的、工具自己
//! 导出的文件**。在这里它是**构造上**成立的，而不是靠实现时记得——[`Act::Delete`]
//! 与 [`Act::Update`] 两类步骤一律从 [`Manifest::files`] 走出来，实际状态那一侧
//! 只用来**核对**，从来不是删除的来源。维护者手动拷进 SD 卡的存档、金手指、截图
//! 落在清单之外，于是它们连成为一条 [`Step`] 的路径都没有。
//!
//! ## 证明不了一致，就不动它
//!
//! 与增量扫描那句「证明不了没变，就不能说没变」（[`baseline`](crate::catalog::baseline)）
//! 是同一条纪律，只是这边的赌注更大：那边判错顶多多扫一遍，这边判错是删掉别人的东西。
//! 于是四种对不上的情况一律**报告 + 本次不动**（[`Surprise`]）：
//!
//! - 清单说有、实际**没了** → 那可能是维护者在掌机上有意删的，**不静默补回**（ADR-0015）。
//! - 清单说有、实际**变了** → 那已经不是工具放的那一份，不覆盖也不删。
//! - 落点上有个**清单之外**的文件挡着 → 那多半就是维护者自己拷进去的，不覆盖。
//! - 目标上这个文件**元数据读不到**（ADR-0021 的第三态）→ 既不算在、也不算不在。
//!
//! ## 期望状态由三样拼起来
//!
//! [`desired`] 折**选中变体的文件成员**，[`media::lay`] 铺**媒体池**里的封面截图视频，
//! [`frontend::lay`] 折**前端元数据**。三份都是 [`DesiredFile`]，[`plan`] 一视同仁
//! ——票 20 补上后两样时，[`plan`] 一个字都没动，正是当初把 [`FileKind`] 三类一次立
//! 齐的那笔账兑现了（清单是持久数据，往写满了的表上加一列贵得多）。
//!
//! 格式转换要等票 21，它进来时改的仍然只是折期望状态那一步。
//!
//! ## 落到目标上的是 [`execute::run`]
//!
//! 计划是纯的，执行不是。两者分开，于是「只碰清单里记录过的文件」这条硬约束由计划
//! 那一侧**构造性地**守住，执行只认计划里的步骤——它连清单之外的路径都拿不到。

pub mod execute;
pub mod frontend;
pub mod media;
pub mod observe;
pub mod prepare;
pub mod report;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::capability::{Conversion, Decision, Filesystem, Profile, RejectReason};
use crate::catalog::{Catalog, CatalogError};
use crate::sublibrary::{Selected, Sublibrary, Trim, over_capacity, trim_suggestions};

pub use execute::{Outcome, Placement, Sources};
pub use observe::{ObserveError, observe};
pub use prepare::{Prepared, Request, prepare};

/// 子库里一个文件是干什么的。
///
/// 三类从第一天就立着，见模块文档最后一段。眼下 [`desired`] 只产得出 [`Self::Rom`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FileKind {
    /// 变体的文件成员：主文件、附属文件、内部资源、附属内容都算。
    Rom,
    /// 从**媒体池**铺过去的封面、截图、视频（票 20）。
    Media,
    /// 前端元数据文件（票 20）。
    Metadata,
}

impl FileKind {
    /// 存进清单、也打给用户的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Rom => "ROM",
            Self::Media => "媒体",
            Self::Metadata => "元数据",
        }
    }

    /// 从词认回来。认不出时是 `None`——**不猜**：猜错一个类别，清单里那一条就归错了账。
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        Some(match label {
            "ROM" => Self::Rom,
            "媒体" => Self::Media,
            "元数据" => Self::Metadata,
            _ => return None,
        })
    }

    /// 报告里固定的排列顺序。
    #[must_use]
    pub fn all() -> [Self; 3] {
        [Self::Rom, Self::Media, Self::Metadata]
    }
}

/// 一个文件的**身份**：大小加修改时间。
///
/// 与增量扫描的三元组同一套办法（路径在外面当键）。清单里记的是**写完之后从目标上
/// 读回来的那一份**，不是主库侧的——于是 FAT32 那 2 秒的时间戳刻度不构成问题：
/// 两次读的是同一个被截断过的值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    /// 字节数。
    pub bytes: u64,
    /// 修改时间（UNIX 纪元起的纳秒）；取不到时是 `None`。
    pub mtime_ns: Option<i64>,
}

impl Stamp {
    /// 这两个戳能**证明**是同一份东西吗。
    ///
    /// 大小不同一定不是。大小相同而两边都拿得到时间时，时间也得相同。
    /// **有一边的时间取不到就算证明不了**——这与 `Baseline::verdict` 里那句
    /// 「证明不了没变，就不能说没变」是同一条纪律，只是这边判错的代价是删掉别人的东西。
    #[must_use]
    pub fn proves_same(&self, other: &Self) -> bool {
        self.bytes == other.bytes
            && match (self.mtime_ns, other.mtime_ns) {
                (Some(a), Some(b)) => a == b,
                _ => false,
            }
    }
}

/// **期望状态**里的一个文件：子库同步完之后目标上该有的一份东西。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DesiredFile {
    /// **相对子库根**的路径，`/` 分隔、NFC（ADR-0015、ADR-0020）。
    ///
    /// 眼下就是变体成员在主库里的键原样搬过来——**子库根在语义上是主库根的替身**，
    /// 与 `adapter::transfer` 那一侧同一条约定。卡插到别的设备、盘符变了都不受影响。
    pub path: String,
    /// 这是个什么文件。
    pub kind: FileKind,
    /// 字节数。**元数据读不到的成员按 0 计**（ADR-0021），见 [`Self::unreadable`]。
    pub bytes: u64,
    /// 主库侧元数据读不到，因此 [`Self::bytes`] 是 0 而不是真的 0 字节。
    pub unreadable: bool,
    /// 主库侧的键：搬的时候从这儿读。
    pub source: String,
    /// 主库侧那份**现在**的戳。
    ///
    /// 与清单里记的那个一比，就知道「主库这份变了没有」。**这不是 [`Self::bytes`]
    /// 的重复**：`bytes` 说的是它到了目标上占多少（票 21 转格式之后两者会分开），
    /// 而这个戳说的是它在主库里是不是还是上次那一份——只比大小的话，
    /// **原地改过、大小没变**的文件会被静默判成不用重传。
    pub source_stamp: Stamp,
    /// 属于哪个**变体**。报告按它把文件数折回用户认得的那个数。
    pub variant: String,
    /// 这一份是**转换产物**吗；`None` 就是原样搬（票 21、ADR-0017）。
    ///
    /// 有值时 [`Self::path`] 已经是**产物**的落点（`.7z` 变 `.zip`、容器变裸文件），
    /// 而 [`Self::source`] 仍然指着主库里那份**原始形态**——**转换只产生新文件，
    /// 主库一个字节不改**（ADR-0004、ADR-0015）。
    pub convert: Option<Conversion>,
}

/// 一份内容**放不进目标存储**。
///
/// ADR-0017 补充段点名的那件事：FAT32 有 4 GiB 单文件上限，PS2 / PSP 的大 ISO 直接
/// 放不进去。**这类必须在差量预览阶段就报出来**，而不是传到一半失败——传到一半会在
/// 卡上留下半份文件、在清单里留下一条谎。
///
/// 被拦下的**既不新增也不删除**：它进不了目标，可万一目标上已经有一份（换过档案、
/// 或者我们这条声明本身就是错的），那也不是它该被删掉的理由。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rejected {
    /// 本来要落在目标上的哪条路径。
    pub path: String,
    /// 这是个什么文件。
    pub kind: FileKind,
    /// 多大。
    pub bytes: u64,
    /// 属于哪个变体。
    pub variant: String,
    /// 哪一条约束拦下的。
    pub reason: RejectReason,
    /// 说清楚是怎么回事。
    pub detail: String,
    /// 上面那个 [`Self::bytes`] 是**估**出来的吗。
    ///
    /// 重打包成 zip 那一路填的是**未压缩总量**，也就是上界（[`Conversion::estimated`]）。
    /// 于是「超过单文件上限」这一条在它身上可能是**误判**——压完说不定就装得下了。
    /// 报告必须把这件事说出口，不然用户会照着一条其实不成立的结论去换卡。
    pub estimated: bool,
}

/// 一份内容**目标吃不下，而这一版转不了**。
///
/// 库里那批**裹着一整棵目录树**的 PSV `.tar.zst`（Vita3K 只认 zip/vpk/vci，而这一版只解
/// 得动单条目的容器，挂账 D91）、33.4% 容量的 Switch `.nsz`/`.xcz`（ES-DE 不认）、
/// 几乎无人支持的 `.rar` 都落在这里。
///
/// **照样搬过去**，只是点名说出口。不搬的话是静默丢掉用户亲手挑中的东西，那比白占
/// 一点地方糟得多；而假装已经处理妥当，正是 ADR-0017 那句「矩阵错误比不转换更糟」
/// 说的那种错。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Unsupported {
    /// 主库里那条键。
    pub path: String,
    /// 属于哪个变体。
    pub variant: String,
    /// 哪个平台。
    pub platform: Option<String>,
    /// 多大。
    pub bytes: u64,
    /// 目标要的是什么形态。
    pub want: String,
    /// 为什么转不了。
    pub why: String,
}

/// 一个子库的**期望状态**：目标上该有的全部文件。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Desired {
    /// 该有的文件，按路径排。
    pub files: Vec<DesiredFile>,
    /// 有几个成员元数据读不到，于是容量少算了它们（ADR-0021）。
    pub unreadable: u64,
    /// 跳过的**非文件**成员：目录树变体的那个目录本身、符号链接。
    ///
    /// 它们不是要搬的东西（目录由执行那一步顺手建），但要数出来——目录树变体
    /// 一个成员都不该凭空消失。
    pub non_files: u64,
    /// 选中了、却一个文件成员都没有的变体。**不是错误，是要说出口的怪事**。
    pub empty_variants: Vec<String>,
    /// 目标存储放不下的那些。**不在 [`Self::files`] 里**：传必然失败。
    pub rejected: Vec<Rejected>,
    /// 目标吃不下、而这一版转不了的那些。**在 [`Self::files`] 里**：照搬，但报出来。
    pub unsupported: Vec<Unsupported>,
}

impl Desired {
    /// 期望总量：同步完之后这个子库该占多少。
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.files.iter().map(|file| file.bytes).sum()
    }
}

/// **清单**里的一条：上次导出时工具往目标上放了什么。
///
/// 注意与**平台清单**（[`platform::Manifest`](crate::platform::Manifest)）不是一回事：
/// 那份说的是「哪个平台按什么规则成型」，这份说的是「上次往这台设备上放了哪些文件」。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestFile {
    /// 相对子库根的路径。
    pub path: String,
    /// 这是个什么文件。
    pub kind: FileKind,
    /// 写完之后从目标上读回来的那个戳。
    pub stamp: Stamp,
    /// 主库侧的键。
    pub source: String,
    /// 放上去的**那一刻**主库侧那份的戳。判「主库那份变了没有」用它。
    pub source_stamp: Stamp,
    /// 属于哪个变体。
    pub variant: String,
    /// **工具放过它，而维护者在目标设备上把它删了，工具没有补回。**
    ///
    /// 这一格是清单里唯一一条「记着的东西现在不在目标上」。留它的理由只有一条：
    /// 不留的话，同步完这条会整个从清单里消失，于是**下一趟它变成一条普通的新增
    /// 又长回来**——而用户故事 65 的原话是「在掌机上有意删掉的东西不会自己长回来」
    /// （挂账 D75）。[`Stamp`] 仍然记着**当初放上去时**那一份的样子：它又被拷回来时
    /// 拿它认得出「回来的是不是同一份」。
    pub absent: bool,
}

/// 某个子库**上次导出的完整记录**（`CONTEXT.md` 的**清单**）。
///
/// 它是增量同步的依据，**也是工具在目标设备上的行为边界**——清单之外的文件工具一律
/// 不碰（ADR-0015）。这个类型因此是 [`plan`] 里唯一能长出删除项的地方。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// 上次放上去的文件，按路径排。
    pub files: Vec<ManifestFile>,
}

impl Manifest {
    /// 一份空清单：这台设备还没同步过，于是一切都是新增，**一条删除项也长不出来**。
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// 清单记着的容量合计。**已经不在目标上的那些不算**——它们一个字节都没占。
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.files
            .iter()
            .filter(|file| !file.absent)
            .map(|file| file.stamp.bytes)
            .sum()
    }

    /// 清单里**眼下确实在目标上**的那几条。
    pub fn present(&self) -> impl Iterator<Item = &ManifestFile> {
        self.files.iter().filter(|file| !file.absent)
    }
}

/// 目标设备上实际躺着的一个文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TargetFile {
    /// 相对子库根的路径。
    pub path: String,
    /// 它的戳；**元数据读不到时是 `None`**（ADR-0021 的第三态）。
    ///
    /// 目录、符号链接这类「有东西挡在这儿、但说不清是什么」的也走这一支：
    /// 说不清就一律不动它。
    pub stamp: Option<Stamp>,
}

/// **目标设备的实际状态**：子库根底下扫出来的全部文件。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TargetState {
    /// 扫到的文件，按路径排。
    pub files: Vec<TargetFile>,
    /// 列不开的目录数。列不开就意味着这一枝底下的东西**全部说不清**。
    pub unlistable_dirs: u64,
}

/// 排计划时的几个开关。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    /// 把「清单说有、实际没了」的那些**补回**去。
    ///
    /// 默认 `false`，因为 ADR-0015 说的是「报告而非静默补回——那可能是用户有意删的」。
    /// 开着它时那些文件照样进 [`Plan::surprises`]，只是同时也进计划：**明知故犯不是静默**。
    pub restore_missing: bool,
}

/// 一条步骤要干什么。
///
/// 派生的顺序**就是执行顺序**：先删后传。ADR-0016 关心的峰值占用因此是
/// `max(同步前, 同步后)` 而不是两者之和——换一批游戏时这个差别就是能不能装下。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Act {
    /// 删掉。**只可能来自清单**。
    Delete,
    /// 重传：清单里有，但主库那一侧已经不是同一份东西了。
    Update,
    /// 新放上去。
    Add,
}

impl Act {
    /// 报告里用的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Delete => "删除",
            Self::Update => "更新",
            Self::Add => "新增",
        }
    }
}

/// 计划里的一步。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Step {
    /// 干什么。
    pub act: Act,
    /// 目标上的哪个文件（相对子库根）。
    pub path: String,
    /// 这是个什么文件。
    pub kind: FileKind,
    /// 干完之后它占多少字节；删除时是 0。
    pub bytes: u64,
    /// 干之前它占多少字节；新增时是 0。
    pub was: u64,
    /// 主库侧的键；删除时是清单记着的那个。
    pub source: String,
    /// 主库侧那份的戳。**执行完之后要连它一起记进清单**（票 20），
    /// 下一趟才判得出「主库那份变了没有」——计划因此自己就够执行，不必再回头查期望状态。
    pub source_stamp: Stamp,
    /// 属于哪个变体。
    pub variant: String,
    /// 这一步是在**补回**一个意外消失的文件（只有开了
    /// [`Options::restore_missing`] 才会有）。
    pub restore: bool,
    /// 要不要转格式；`None` 就是原样复制。见 [`DesiredFile::convert`]。
    ///
    /// **删除那一步永远是 `None`**：删掉一个转换产物不必知道它当初是怎么转出来的。
    pub convert: Option<Conversion>,
}

/// 目标上一件**对不上**的事。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum SurpriseKind {
    /// 清单说有、实际没了。**那可能是维护者在掌机上有意删的**（ADR-0015）。
    Gone,
    /// 清单说有、实际在，但已经不是工具放的那一份了。
    Changed,
    /// 落点上有个**清单之外**的文件挡着。多半就是维护者自己拷进去的，不覆盖。
    Occupied,
    /// 目标上这个文件的元数据读不到（ADR-0021 的第三态）：既不算在、也不算不在。
    Unreadable,
}

impl SurpriseKind {
    /// 报告里用的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Gone => "没了",
            Self::Changed => "被改过",
            Self::Occupied => "落点被占",
            Self::Unreadable => "读不到",
        }
    }

    /// 报告里固定的排列顺序。
    #[must_use]
    pub fn all() -> [Self; 4] {
        [Self::Gone, Self::Changed, Self::Occupied, Self::Unreadable]
    }
}

/// 一件对不上的事，连它的前因后果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Surprise {
    /// 哪一种。
    pub kind: SurpriseKind,
    /// 目标上的哪个文件。
    pub path: String,
    /// 清单说它该是什么样；[`SurpriseKind::Occupied`] 时没有（那条路径不在清单里）。
    pub expected: Option<Stamp>,
    /// 目标上实际是什么样；[`SurpriseKind::Gone`] 时没有。
    pub found: Option<Stamp>,
    /// 属于哪个变体；落点被占时是**要放进来的**那个变体。
    pub variant: String,
    /// 选择集里**还要不要**它。用户处置的办法完全不同，所以这一位必须在。
    pub still_wanted: bool,
}

/// 一类操作的账。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Tally {
    /// 几个文件。
    pub files: u64,
    /// 涉及几个**变体**——用户认得的是这个数，不是文件数。
    pub variants: u64,
    /// 多少字节。
    pub bytes: u64,
}

/// 一次同步的**操作计划**，也就是**差量预览**。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Plan {
    /// 哪个子库。
    pub sublibrary: String,
    /// 目标设备上的子库根。
    pub target: String,
    /// 前端格式。
    pub format: String,
    /// 容量上限；`None` 表示不设限。
    pub capacity: Option<u64>,

    /// 全部步骤，**按 `(干什么, 路径)` 排**——先删后传，同一批之内按路径。
    pub steps: Vec<Step>,
    /// 新增的账。
    pub adds: Tally,
    /// 更新的账；`bytes` 是**更新之后**的合计。
    pub updates: Tally,
    /// 更新**之前**那几个文件占多少——净变化要用它。
    pub updates_before: u64,
    /// 删除的账；`bytes` 是腾出来的容量。
    pub deletes: Tally,
    /// 原样留着、这次一个字节都不用动的账。
    pub keeps: Tally,
    /// 净变化：`新增 + 更新后 - 更新前 - 删除`。
    pub net_bytes: i64,
    /// 期望总量：同步完之后这个**子库**该占多少。
    pub desired_bytes: u64,
    /// 目标上**眼下**实际占多少（下界：元数据读不到的按 0 计）。
    pub actual_bytes: u64,
    /// 这次同步完之后**目标上**会占多少：`目标现占 + 净变化`。**容量账比的是这个数。**
    pub after_bytes: u64,
    /// 期望状态一共几个文件、几个变体。
    pub desired: Tally,

    /// 对不上的事，按路径排。
    pub surprises: Vec<Surprise>,
    /// **清单之外**的文件有几个——目标上一切不在清单里的东西，**落点被占的那几个也算**。
    ///
    /// ADR-0015 的原话是「清单之外的一切文件对工具不存在」，那就该按字面数：
    /// 漏数哪一个，报告都可能说出「目标上没有清单之外的文件」而卡上明明有。
    pub strangers: u64,
    /// 清单之外的文件占了多少字节；元数据读不到的按 0 计。
    pub stranger_bytes: u64,
    /// 清单之外、且元数据也读不到的有几个。
    pub stranger_unreadable: u64,
    /// **你删过、工具记着不补**的有几个：清单里 `absent` 的那些，这一趟仍然不在目标上，
    /// 而选择集还要它们。
    ///
    /// 它们不是[意外](Surprise)——上一趟已经报过一次了，这一趟只是**继续不补**。
    /// 想让它们回来，这次加 `--restore`；想让它们从此不再被念叨，记一条**例外**。
    pub withheld: u64,
    /// 目标上列不开的目录数。
    pub unlistable_dirs: u64,
    /// 主库侧元数据读不到的成员数：这几个的容量没算进账里（ADR-0021）。
    pub unreadable_sources: u64,
    /// 选中了、却一个文件成员都没有的变体。
    pub empty_variants: Vec<String>,

    /// 这一趟要**转格式**的有几个、转出来多大（票 21）。
    ///
    /// 只数真要动手的那些（新增与更新）——目标上已经有的那份转换产物不必再转一遍。
    pub converts: Tally,
    /// 这一趟转换要**读**多少源字节。耗时预估拿它算。
    pub convert_source_bytes: u64,
    /// 转换的**粗估**耗时，毫秒。见 [`estimate_secs`](crate::capability::estimate_secs)。
    ///
    /// 存整数而不是浮点，是为了让 [`Plan`] 保持 `Eq`——差量预览要能逐字段比对，
    /// 而「两份计划相不相等」上不该出现浮点那套「相等但不全等」的麻烦。
    /// 一个粗估本来也不需要亚毫秒精度。
    pub convert_ms: u64,
    /// 有几个转换产物的大小是**估**出来的（重打包成 zip 那一路）。
    pub convert_estimated: u64,
    /// 目标存储**放不下**的那些：本次既不新增也不删除，只报出来（ADR-0017 补充段）。
    pub rejected: Vec<Rejected>,
    /// 目标**吃不下、而这一版转不了**的那些：照搬，但点名说出口。
    pub unsupported: Vec<Unsupported>,
    /// 这份计划用的是哪份**能力档案**。
    pub capability: String,

    /// 超出容量上限多少字节；没超或没设上限时是 `None`。
    ///
    /// 比的是**子库占用加上清单之外的占用**：卡上的地方是共用的，只算自己那一半
    /// 会给出一个「装得下」然后传到一半没空间。
    pub over_capacity: Option<u64>,
    /// 超了的话，按体积排序的裁剪建议。**绝不自动截断**（ADR-0016）。
    pub trim_suggestions: Vec<Trim>,
}

impl Plan {
    /// 这次同步要动几个文件。零就是「什么都不用干」。
    #[must_use]
    pub fn touched(&self) -> u64 {
        self.steps.len() as u64
    }
}

/// 三方对比，排出计划。**纯函数**：不碰磁盘、不碰中立库、不看时钟。
///
/// 三个输入正是**同步**词条里的三方：`desired` 是主库该有的、`manifest` 是清单
/// 记录的、`actual` 是目标上实际有的。
///
/// 判定表（照这个次序读）：
///
/// | 期望 | 清单 | 实际 | 结论 |
/// |---|---|---|---|
/// | 有 | 无 | 无 | **新增** |
/// | 有 | 无 | 有 | **落点被占**——不覆盖，报告 |
/// | 有 | 有 | 对得上 | 一样就**保持**，主库那份变了就**更新** |
/// | 有 | 有 | 没了 | **报告**；默认不补回（ADR-0015） |
/// | 有 | 有 | 对不上 | **报告**，本次不动 |
/// | 无 | 有 | 对得上 | **删除** |
/// | 无 | 有 | 没了 / 对不上 | **报告**，不删——它已经不是工具放的那一份了 |
/// | 无 | 无 | 有 | **清单之外**，只数一数，永不出现在任何一步里 |
#[must_use]
pub fn plan(
    sublibrary: &Sublibrary,
    desired: &Desired,
    manifest: &Manifest,
    actual: &TargetState,
    options: Options,
) -> Plan {
    let wanted: BTreeMap<&str, &DesiredFile> = desired
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    let recorded: BTreeMap<&str, &ManifestFile> = manifest
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    let on_target: BTreeMap<&str, &TargetFile> = actual
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();

    // 目标存储放不下的那些**既不新增也不删除**：它们进不了目标（新增必然失败），
    // 可万一目标上已经有一份，那也不是它该被删掉的理由——我们这条「放不下」的声明
    // 本身就可能是错的，而删掉别人的东西是这条链路上唯一不可逆的动作。
    let barred: BTreeSet<&str> = desired
        .rejected
        .iter()
        .map(|file| file.path.as_str())
        .collect();

    let mut out = Plan {
        sublibrary: sublibrary.name.clone(),
        target: sublibrary.target.clone(),
        format: sublibrary.format.clone(),
        capacity: sublibrary.capacity,
        capability: sublibrary
            .capability
            .clone()
            .unwrap_or_else(|| crate::capability::DEFAULT_PROFILE.to_string()),
        desired_bytes: desired.bytes(),
        unreadable_sources: desired.unreadable,
        empty_variants: desired.empty_variants.clone(),
        unlistable_dirs: actual.unlistable_dirs,
        rejected: desired.rejected.clone(),
        unsupported: desired.unsupported.clone(),
        ..Plan::default()
    };
    let mut steps: Vec<Step> = Vec::new();
    let mut keeps: Vec<&DesiredFile> = Vec::new();

    // ── 期望这一侧：新增、更新、保持，以及落点被占。
    for (path, file) in &wanted {
        let Some(previous) = recorded.get(path) else {
            // 清单里没有这条路径。目标上有东西挡着就一定不碰——**那多半就是维护者
            // 自己拷进去的**，而工具在清单之外没有任何写的权利（ADR-0015）。
            if let Some(target) = on_target.get(path) {
                out.surprises.push(Surprise {
                    kind: if target.stamp.is_none() {
                        SurpriseKind::Unreadable
                    } else {
                        SurpriseKind::Occupied
                    },
                    path: (*path).to_string(),
                    expected: None,
                    found: target.stamp,
                    variant: file.variant.clone(),
                    still_wanted: true,
                });
            } else {
                steps.push(step(Act::Add, file, 0, false));
            }
            continue;
        };
        // 上一趟就已经知道它没了：那是维护者在掌机上删的，工具**记着不补**（故事 65）。
        // 目标上眼下还是没有它，就只数一数——不当意外报第二遍，也不重新变成一条新增。
        if previous.absent && !on_target.contains_key(path) {
            out.withheld += 1;
            if options.restore_missing {
                steps.push(step(Act::Add, file, 0, true));
            }
            continue;
        }
        // 它又落回目标上了（维护者自己拷回去的，或者换了一份）：底下这段核对照常走，
        // 比的是**当初放上去时**记下的那个戳——回来的是不是同一份，那一格答得出来。
        match verify(previous, on_target.get(path).copied()) {
            Verified::Same => {
                if source_unchanged(file, previous) {
                    keeps.push(file);
                } else {
                    steps.push(step(Act::Update, file, previous.stamp.bytes, false));
                }
            }
            Verified::Gone => {
                out.surprises.push(Surprise {
                    kind: SurpriseKind::Gone,
                    path: (*path).to_string(),
                    expected: Some(previous.stamp),
                    found: None,
                    variant: file.variant.clone(),
                    still_wanted: true,
                });
                if options.restore_missing {
                    steps.push(step(Act::Add, file, 0, true));
                }
            }
            Verified::Off(kind, stamp) => out.surprises.push(Surprise {
                kind,
                path: (*path).to_string(),
                expected: Some(previous.stamp),
                found: stamp,
                variant: file.variant.clone(),
                still_wanted: true,
            }),
        }
    }

    // ── 清单这一侧：删除。**删除项只可能从这个循环里长出来。**
    for (path, previous) in &recorded {
        if wanted.contains_key(path) || barred.contains(path) {
            continue;
        }
        // 早就知道它没了，这次也不要它了：没什么可删的，也没什么可报的。
        // 执行那一步会把清单里这一格丢掉——记着一个「不在目标上、也不再要」的路径，
        // 只会让同一条「没了」每一趟都被报一遍。
        if previous.absent && !on_target.contains_key(path) {
            continue;
        }
        match verify(previous, on_target.get(path).copied()) {
            Verified::Same => steps.push(Step {
                act: Act::Delete,
                path: (*path).to_string(),
                kind: previous.kind,
                bytes: 0,
                was: previous.stamp.bytes,
                source: previous.source.clone(),
                source_stamp: previous.source_stamp,
                variant: previous.variant.clone(),
                restore: false,
                // 删掉一个转换产物不必知道它当初是怎么转出来的。
                convert: None,
            }),
            // 已经不在了，而这次本来也要删掉它——结果一致，但仍然如实报出来：
            // 「清单说有、实际没了」是同一件事，用户想不想让它别再回来是另一回事。
            Verified::Gone => out.surprises.push(Surprise {
                kind: SurpriseKind::Gone,
                path: (*path).to_string(),
                expected: Some(previous.stamp),
                found: None,
                variant: previous.variant.clone(),
                still_wanted: false,
            }),
            // 被改过的**不删**：它已经不是工具放的那一份了。
            Verified::Off(kind, stamp) => out.surprises.push(Surprise {
                kind,
                path: (*path).to_string(),
                expected: Some(previous.stamp),
                found: stamp,
                variant: previous.variant.clone(),
                still_wanted: false,
            }),
        }
    }

    // ── 实际这一侧：清单之外的一切。只数一数，永不进 `steps`。
    //
    // **落点被占的那几个也算**：它们同样不在清单里、同样一个字节都不碰。漏数它们，
    // 报告就会说出「目标上没有清单之外的文件」而卡上明明有。
    for (path, target) in &on_target {
        if recorded.contains_key(path) {
            continue;
        }
        out.strangers += 1;
        match target.stamp {
            Some(stamp) => out.stranger_bytes += stamp.bytes,
            None => out.stranger_unreadable += 1,
        }
    }

    steps.sort_by(|a, b| a.act.cmp(&b.act).then_with(|| a.path.cmp(&b.path)));
    out.surprises
        .sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.kind.cmp(&b.kind)));

    out.adds = tally(steps.iter().filter(|s| s.act == Act::Add), |s| s.bytes);
    out.updates = tally(steps.iter().filter(|s| s.act == Act::Update), |s| s.bytes);
    out.updates_before = steps
        .iter()
        .filter(|s| s.act == Act::Update)
        .map(|s| s.was)
        .sum();
    out.deletes = tally(steps.iter().filter(|s| s.act == Act::Delete), |s| s.was);
    out.keeps = Tally {
        files: keeps.len() as u64,
        variants: distinct(keeps.iter().map(|file| file.variant.as_str())),
        bytes: keeps.iter().map(|file| file.bytes).sum(),
    };
    out.desired = Tally {
        files: desired.files.len() as u64,
        variants: distinct(desired.files.iter().map(|file| file.variant.as_str())),
        bytes: out.desired_bytes,
    };
    out.net_bytes =
        signed(out.adds.bytes + out.updates.bytes) - signed(out.updates_before + out.deletes.bytes);

    // ── 这一趟要转几个、要读多少、大概多久（票 21 的验收条目）。
    //
    // **只数真要动手的那些**：目标上已经躺着的那份转换产物不必再转一遍，把它算进
    // 「要转 N 个、约 M 分钟」只会让第二趟同步显示一个根本不会发生的等待。
    let converting: Vec<&Step> = steps
        .iter()
        .filter(|step| step.act != Act::Delete && step.convert.is_some())
        .collect();
    out.converts = Tally {
        files: converting.len() as u64,
        variants: distinct(converting.iter().map(|step| step.variant.as_str())),
        bytes: converting.iter().map(|step| step.bytes).sum(),
    };
    out.convert_source_bytes = converting
        .iter()
        .filter_map(|step| step.convert.as_ref())
        .map(|conversion| conversion.source_bytes)
        .sum();
    let secs: f64 = converting
        .iter()
        .filter_map(|step| step.convert.as_ref())
        // **拿未压缩量估，不是源文件大小**：解压器与压缩器要处理的是前者
        // （`capability::Recipe::throughput_mib` 记着这个数是怎么栽出来的）。
        .map(|conversion| crate::capability::estimate_secs(conversion.recipe, conversion.bytes))
        .sum();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        out.convert_ms = (secs * 1000.0).round().max(0.0) as u64;
    }
    out.convert_estimated = converting
        .iter()
        .filter_map(|step| step.convert.as_ref())
        .filter(|conversion| conversion.estimated)
        .count() as u64;

    out.steps = steps;

    // ── 装不装得下。**超限绝不自动截断**（ADR-0016）。
    //
    // 比的是「目标现占 + 净变化」而不是期望总量：卡上的地方是共用的，只算子库自己
    // 那一半会给出一个「装得下」，然后传到一半没空间（挂账 D76）。手动拷进去的存档、
    // 落点被占而这趟根本传不上去的、被改过因而不删的，全都还占着位置。
    out.actual_bytes = actual
        .files
        .iter()
        .filter_map(|file| file.stamp.map(|stamp| stamp.bytes))
        .sum();
    out.after_bytes = signed(out.actual_bytes)
        .saturating_add(out.net_bytes)
        .try_into()
        .unwrap_or(0);
    out.over_capacity = over_capacity(sublibrary.capacity, out.after_bytes);
    if out.over_capacity.is_some() {
        let mut by_variant: BTreeMap<&str, u64> = BTreeMap::new();
        for file in &desired.files {
            *by_variant.entry(file.variant.as_str()).or_default() += file.bytes;
        }
        out.trim_suggestions = trim_suggestions(
            by_variant
                .into_iter()
                .map(|(variant, bytes)| (variant.to_string(), bytes)),
        );
    }
    out
}

/// 清单记的那条，与目标上实际那条对得上吗。
enum Verified {
    /// 对得上：目标上还是工具上次放的那一份。
    Same,
    /// 目标上没有这条路径了。
    Gone,
    /// 目标上有，但对不上（被改过，或者元数据读不到）。
    Off(SurpriseKind, Option<Stamp>),
}

/// 拿清单里那条去核对目标上的实际状态。
fn verify(previous: &ManifestFile, target: Option<&TargetFile>) -> Verified {
    let Some(target) = target else {
        return Verified::Gone;
    };
    let Some(stamp) = target.stamp else {
        return Verified::Off(SurpriseKind::Unreadable, None);
    };
    if previous.stamp.proves_same(&stamp) {
        Verified::Same
    } else {
        Verified::Off(SurpriseKind::Changed, Some(stamp))
    }
}

/// 主库那一侧还是清单记着的那份东西吗。
///
/// 比的是**源的键与源的戳**，不是目标上那个戳——清单里 [`ManifestFile::stamp`]
/// 记的是目标上那份文件，与主库侧那份的大小、时间根本不是一回事（票 21 转起格式来
/// 连大小都会变）。
///
/// **元数据读不到的成员当作没变**：读都读不到，重传也传不成，判它要更新只会让
/// 每一趟同步都挂着一批走不完的步骤。报告已经点名说这几个的容量按 0 计。
fn source_unchanged(file: &DesiredFile, previous: &ManifestFile) -> bool {
    file.source == previous.source
        && file.kind == previous.kind
        && (file.unreadable || file.source_stamp.proves_same(&previous.source_stamp))
}

/// 一条步骤。
fn step(act: Act, file: &DesiredFile, was: u64, restore: bool) -> Step {
    Step {
        act,
        path: file.path.clone(),
        kind: file.kind,
        bytes: file.bytes,
        was,
        source: file.source.clone(),
        source_stamp: file.source_stamp,
        variant: file.variant.clone(),
        restore,
        convert: file.convert.clone(),
    }
}

/// 把一批步骤折成一笔账。
fn tally<'a>(steps: impl Iterator<Item = &'a Step> + Clone, bytes: fn(&Step) -> u64) -> Tally {
    Tally {
        files: steps.clone().count() as u64,
        variants: distinct(steps.clone().map(|step| step.variant.as_str())),
        bytes: steps.map(bytes).sum(),
    }
}

/// 数出有几个不同的值。
fn distinct<'a>(values: impl Iterator<Item = &'a str>) -> u64 {
    values.collect::<BTreeSet<&str>>().len() as u64
}

/// 字节数转成带符号的，好算净变化。库里最大的变体也远够不到 `i64` 的顶。
#[allow(clippy::cast_possible_wrap)]
fn signed(bytes: u64) -> i64 {
    bytes as i64
}

/// 把中立库折成**期望状态**：选中的变体，连它们的文件成员。
///
/// 与 [`sublibrary::facts`](crate::sublibrary::facts) 同一个位置——**折的这一步读库，
/// 算的那一步不读**。眼下折出来的只有 ROM 那一侧，见模块文档。
///
/// 三条不显然的处置：
///
/// - **成员的四种身份全都要搬**：主文件、附属文件、内部资源、附属内容。ADR-0013 说的
///   「附属内容不导出为前端条目」管的是**条目**，不是磁盘上的字节——PSV 的 `addcont/`
///   不搬过去，游戏在掌机上就少一半内容。
/// - **非文件成员跳过**：目录树变体的那个目录本身、符号链接。目录由执行那一步顺手建。
/// - **元数据读不到的照样搬**，只是容量按 0 计并数出来（ADR-0021）。
///
/// ## 格式转换在这一步定下来，而且**不解压一个字节**
///
/// `profile` 是这个子库的**能力档案**。判的是每个变体的**主文件**——那是「用来交给
/// 模拟器启动的那一个」（`CONTEXT.md`），能力矩阵描述的正是模拟器启动得了什么。
/// 判每一个成员的话，一个 PSV 目录树变体底下几千个**内部资源**会各自领一条「吃不下」。
///
/// 「转出来多大、转出来叫什么」全从中立库里那份零解压读进来的**内部构成**算出来
/// （ADR-0014、[`Catalog::container_contents`]），于是**差量预览排得出来而外置盘可以
/// 不在位**——与这一整层「折的时候读库、算的时候不读」是同一条缝。
///
/// 转不了的落进 [`Desired::unsupported`]：**照搬，但点名说出口**。
///
/// # Errors
/// 读中立库失败时返回错误。
pub fn desired(
    catalog: &Catalog,
    selected: &Selected,
    profile: &Profile,
) -> Result<Desired, CatalogError> {
    let picked: BTreeSet<String> = selected
        .picked
        .iter()
        .map(|variant| variant.key.clone())
        .collect();
    let platforms: BTreeMap<&str, Option<&str>> = selected
        .picked
        .iter()
        .map(|variant| (variant.key.as_str(), variant.platform.as_deref()))
        .collect();
    let members = catalog.variant_files(&picked)?;

    // 主文件里凡是**透明容器**的，把内部构成一次取回来。转换要不要得起、转出来多大，
    // 全看这一份——而它零解压就在中立库里躺着（调研第 5 部分 L4）。
    let container_mains: BTreeSet<String> = members
        .iter()
        .filter(|member| member.is_main && member.is_file)
        .filter(|member| {
            crate::container::ContainerKind::for_path(Path::new(&member.key)).is_some()
        })
        .map(|member| member.key.clone())
        .collect();
    let contents = catalog.container_contents(&container_mains)?;

    // 一个变体的主文件判出来的处置，全变体共用一份结论。
    let mut verdicts: BTreeMap<&str, Decision> = BTreeMap::new();
    let mut out = Desired::default();
    for member in &members {
        if !member.is_main || !member.is_file {
            continue;
        }
        let platform = platforms
            .get(member.variant_key.as_str())
            .copied()
            .flatten();
        let decision = crate::capability::decide(
            profile,
            platform,
            &member.key,
            member.len.unwrap_or(0),
            contents.get(&member.key),
        );
        if let Decision::Unsupported { want, why } = &decision {
            out.unsupported.push(Unsupported {
                path: member.key.clone(),
                variant: member.variant_key.clone(),
                platform: platform.map(ToString::to_string),
                bytes: member.len.unwrap_or(0),
                want: want.clone(),
                why: why.clone(),
            });
        }
        verdicts.insert(member.variant_key.as_str(), decision);
    }

    let mut has_file: BTreeSet<&str> = BTreeSet::new();
    for member in &members {
        if !member.is_file {
            out.non_files += 1;
            continue;
        }
        has_file.insert(member.variant_key.as_str());
        // **只有主文件会被转**。附属文件与内部资源原样搬：它们不是交给模拟器启动的
        // 那一份，转它们既没有依据也没有落点。
        let conversion = match verdicts.get(member.variant_key.as_str()) {
            Some(Decision::Convert(conversion)) if member.is_main => Some((**conversion).clone()),
            _ => None,
        };
        let (path, bytes) = match &conversion {
            Some(conversion) => (conversion.path.clone(), conversion.bytes),
            None => (member.key.clone(), member.len.unwrap_or(0)),
        };
        out.files.push(DesiredFile {
            path,
            kind: FileKind::Rom,
            bytes,
            unreadable: member.len.is_none(),
            source: member.key.clone(),
            source_stamp: Stamp {
                bytes: member.len.unwrap_or(0),
                mtime_ns: member.mtime_ns,
            },
            variant: member.variant_key.clone(),
            convert: conversion,
        });
        if member.len.is_none() {
            out.unreadable += 1;
        }
    }
    out.files.sort_by(|a, b| a.path.cmp(&b.path));
    out.unsupported.sort_by(|a, b| a.path.cmp(&b.path));
    out.empty_variants = picked
        .iter()
        .filter(|key| !has_file.contains(key.as_str()))
        .cloned()
        .collect();
    Ok(out)
}

impl Desired {
    /// 把**目标存储放不下**的那些从期望状态里挑出来，落进 [`Self::rejected`]。
    ///
    /// ADR-0017 补充段：**FAT32 有 4 GiB 单文件上限**，PS2 / PSP 的大 ISO 直接放不进去
    /// ——这类情况必须在差量预览阶段就报出来，而不是传到一半失败。文件名的字符限制与
    /// 路径长度限制同理。
    ///
    /// 单独一趟而不是揉进 [`desired`]，是因为它要看**全部三类文件**（ROM、媒体、元数据）
    /// 与**子库根那串路径有多长**——媒体与元数据是另外两个 `lay` 折出来的，而目标根
    /// 只有调用方知道。
    ///
    /// 顺手还查**落点撞车**：转换会把 `游戏.zip` 变成 `游戏.sfc`，两个不同的容器解出
    /// 同名内容时就撞上了。撞上的**一个都不放行**——留一个放行等于随排序决定谁赢，
    /// 而下一趟排序变了赢家就换人，卡上那份会莫名其妙地改内容。
    pub fn screen(&mut self, filesystem: &Filesystem, prefix_chars: usize) {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut collided: BTreeSet<String> = BTreeSet::new();
        for file in &self.files {
            if !seen.insert(file.path.as_str()) {
                collided.insert(file.path.clone());
            }
        }

        let mut keep = Vec::with_capacity(self.files.len());
        for file in std::mem::take(&mut self.files) {
            let verdict = if collided.contains(&file.path) {
                Some((
                    RejectReason::Collision,
                    format!("不止一份内容要落到这条路径上（{}）", file.source),
                ))
            } else {
                filesystem.screen(&file.path, file.bytes, prefix_chars)
            };
            match verdict {
                Some((reason, detail)) => self.rejected.push(Rejected {
                    path: file.path,
                    kind: file.kind,
                    bytes: file.bytes,
                    variant: file.variant,
                    reason,
                    detail,
                    estimated: file.convert.is_some_and(|conversion| conversion.estimated),
                }),
                None => keep.push(file),
            }
        }
        self.files = keep;
        self.rejected
            .sort_by(|a, b| a.reason.cmp(&b.reason).then_with(|| a.path.cmp(&b.path)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 子库(capacity: Option<u64>) -> Sublibrary {
        Sublibrary {
            name: "掌机".to_string(),
            target: "/Volumes/SDCARD/Games".to_string(),
            target_raw: Some("/Volumes/SDCARD/Games".to_string()),
            format: "Pegasus".to_string(),
            capacity,
            capability: None,
        }
    }

    fn 期望(path: &str, bytes: u64) -> DesiredFile {
        DesiredFile {
            path: path.to_string(),
            kind: FileKind::Rom,
            bytes,
            unreadable: false,
            source: path.to_string(),
            source_stamp: 戳(bytes),
            variant: path.to_string(),
            convert: None,
        }
    }

    fn 戳(bytes: u64) -> Stamp {
        Stamp {
            bytes,
            mtime_ns: Some(1_700_000_000_000_000_000),
        }
    }

    fn 清单条(path: &str, bytes: u64) -> ManifestFile {
        ManifestFile {
            path: path.to_string(),
            kind: FileKind::Rom,
            stamp: 戳(bytes),
            source: path.to_string(),
            source_stamp: 戳(bytes),
            variant: path.to_string(),
            absent: false,
        }
    }

    fn 在目标上(path: &str, bytes: u64) -> TargetFile {
        TargetFile {
            path: path.to_string(),
            stamp: Some(戳(bytes)),
        }
    }

    fn 期望状态(files: Vec<DesiredFile>) -> Desired {
        Desired {
            files,
            ..Desired::default()
        }
    }

    fn 实际状态(files: Vec<TargetFile>) -> TargetState {
        TargetState {
            files,
            unlistable_dirs: 0,
        }
    }

    /// 清单里那种「工具放过、你把它删了、工具没补」的格子。
    fn 删过的清单条(path: &str, bytes: u64) -> ManifestFile {
        ManifestFile {
            absent: true,
            ..清单条(path, bytes)
        }
    }

    #[test]
    fn 你在目标上删掉的东西不会自己长回来() {
        // 用户故事 65 的原话。清单里那一格记着「知道它没了、也知道你没让我补」，
        // 于是它既不再变成一条新增，也不再被当成意外报第二遍。
        let plan = plan(
            &子库(None),
            &期望状态(vec![期望("GB/口袋妖怪.zip", 1024)]),
            &Manifest {
                files: vec![删过的清单条("GB/口袋妖怪.zip", 1024)],
            },
            &实际状态(vec![]),
            Options::default(),
        );
        assert_eq!(plan.touched(), 0, "一步都不该有");
        assert_eq!(plan.withheld, 1);
        assert!(plan.surprises.is_empty(), "上一趟已经报过一次了");
    }

    #[test]
    fn 记着不补的那些_加了_restore_才补回来() {
        let plan = plan(
            &子库(None),
            &期望状态(vec![期望("GB/口袋妖怪.zip", 1024)]),
            &Manifest {
                files: vec![删过的清单条("GB/口袋妖怪.zip", 1024)],
            },
            &实际状态(vec![]),
            Options {
                restore_missing: true,
            },
        );
        assert_eq!(plan.adds.files, 1);
        assert!(plan.steps[0].restore, "补回来的那一步得标出来");
        assert_eq!(plan.withheld, 1, "照样数出来：明知故犯不是静默");
    }

    #[test]
    fn 早知道没了_这次也不要了_一句话都不用说() {
        let plan = plan(
            &子库(None),
            &期望状态(vec![]),
            &Manifest {
                files: vec![删过的清单条("GB/口袋妖怪.zip", 1024)],
            },
            &实际状态(vec![]),
            Options::default(),
        );
        assert_eq!(plan.touched(), 0);
        assert!(plan.surprises.is_empty(), "没什么可删，也没什么可报");
        assert_eq!(plan.withheld, 0, "它连「还要」都算不上");
    }

    #[test]
    fn 记着不补的那份又被拷回来了_照常核对() {
        // 维护者自己把它拷回去了。回来的是不是同一份，靠当初放上去时记的那个戳判。
        let plan = plan(
            &子库(None),
            &期望状态(vec![期望("GB/口袋妖怪.zip", 1024)]),
            &Manifest {
                files: vec![删过的清单条("GB/口袋妖怪.zip", 1024)],
            },
            &实际状态(vec![在目标上("GB/口袋妖怪.zip", 1024)]),
            Options::default(),
        );
        assert_eq!(plan.withheld, 0);
        assert_eq!(plan.keeps.files, 1, "是同一份，什么都不用做");
    }

    #[test]
    fn 清单里记着不补的那几条不算占地方() {
        let manifest = Manifest {
            files: vec![
                清单条("GB/在的.zip", 1000),
                删过的清单条("GB/没了的.zip", 500),
            ],
        };
        assert_eq!(manifest.bytes(), 1000, "没了的那份一个字节都没占");
        assert_eq!(manifest.present().count(), 1);
    }

    #[test]
    fn 头一次同步全是新增_一条删除也长不出来() {
        let plan = plan(
            &子库(None),
            &期望状态(vec![期望("GB/一.zip", 1024), 期望("GB/二.zip", 2048)]),
            &Manifest::empty(),
            &实际状态(vec![]),
            Options::default(),
        );
        assert_eq!(plan.adds.files, 2);
        assert_eq!(plan.adds.bytes, 3072);
        assert_eq!(plan.deletes.files, 0);
        assert_eq!(plan.net_bytes, 3072);
        assert_eq!(plan.desired_bytes, 3072);
        assert!(plan.steps.iter().all(|step| step.act == Act::Add));
    }

    #[test]
    fn 差量预览给出新增删除与净变化() {
        // 一个留着、一个不要了、一个新来的。
        let plan = plan(
            &子库(None),
            &期望状态(vec![期望("GB/留着.zip", 1000), 期望("GB/新来的.zip", 3000)]),
            &Manifest {
                files: vec![清单条("GB/留着.zip", 1000), 清单条("GB/不要了.zip", 500)],
            },
            &实际状态(vec![
                在目标上("GB/留着.zip", 1000),
                在目标上("GB/不要了.zip", 500),
            ]),
            Options::default(),
        );
        assert_eq!((plan.adds.files, plan.adds.bytes), (1, 3000));
        assert_eq!((plan.deletes.files, plan.deletes.bytes), (1, 500));
        assert_eq!((plan.keeps.files, plan.keeps.bytes), (1, 1000));
        assert_eq!(plan.net_bytes, 2500);
        assert_eq!(plan.desired_bytes, 4000);
    }

    #[test]
    fn 净变化在删得比加得多时是负的() {
        let plan = plan(
            &子库(None),
            &期望状态(vec![]),
            &Manifest {
                files: vec![清单条("GB/一.zip", 4096)],
            },
            &实际状态(vec![在目标上("GB/一.zip", 4096)]),
            Options::default(),
        );
        assert_eq!(plan.net_bytes, -4096);
        assert_eq!(plan.deletes.bytes, 4096);
    }

    #[test]
    fn 主库那份变了就更新_净变化只算差额() {
        let plan = plan(
            &子库(None),
            &期望状态(vec![期望("GB/一.zip", 4096)]),
            &Manifest {
                files: vec![清单条("GB/一.zip", 1024)],
            },
            &实际状态(vec![在目标上("GB/一.zip", 1024)]),
            Options::default(),
        );
        assert_eq!(plan.updates.files, 1);
        assert_eq!(plan.updates.bytes, 4096);
        assert_eq!(plan.updates_before, 1024);
        assert_eq!(plan.net_bytes, 3072);
    }

    #[test]
    fn 主库那份原地改过_大小没变照样要重传() {
        // 只比大小的话这一条会被静默漏掉：汉化补丁打完、重压一次，大小撞上是常事。
        let mut file = 期望("GB/一.zip", 1024);
        file.source_stamp.mtime_ns = Some(1_900_000_000_000_000_000);
        let plan = plan(
            &子库(None),
            &期望状态(vec![file]),
            &Manifest {
                files: vec![清单条("GB/一.zip", 1024)],
            },
            &实际状态(vec![在目标上("GB/一.zip", 1024)]),
            Options::default(),
        );
        assert_eq!(plan.updates.files, 1);
        assert_eq!(plan.net_bytes, 0, "大小没变，净变化就是 0");
    }

    #[test]
    fn 目标上占多少与主库那份变没变是两回事() {
        // 票 21 转起格式来，目标上那份的大小与主库侧那份根本对不上。判要不要重传
        // 只看**源的戳**，于是那时也不会每一趟都把整张卡重刷一遍。
        let mut previous = 清单条("GB/一.zip", 300);
        previous.source_stamp = 戳(1024);
        let plan = plan(
            &子库(None),
            &期望状态(vec![期望("GB/一.zip", 1024)]),
            &Manifest {
                files: vec![previous],
            },
            &实际状态(vec![在目标上("GB/一.zip", 300)]),
            Options::default(),
        );
        assert_eq!(plan.updates.files, 0, "源没变就不动它");
        assert_eq!(plan.keeps.files, 1);
    }

    #[test]
    fn 清单之外的文件永不出现在任何一步里() {
        // 存档、金手指、截图：清单之外的一切对工具不存在（ADR-0015）。
        let plan = plan(
            &子库(None),
            &期望状态(vec![期望("GB/一.zip", 1024)]),
            &Manifest::empty(),
            &实际状态(vec![
                在目标上("saves/口袋妖怪.sav", 32768),
                在目标上("cheats/金手指.txt", 128),
                在目标上("screenshots/2026-08-30.png", 4096),
            ]),
            Options::default(),
        );
        assert_eq!(plan.strangers, 3);
        assert_eq!(plan.stranger_bytes, 32768 + 128 + 4096);
        assert!(
            plan.steps.iter().all(|step| step.path == "GB/一.zip"),
            "{:?}",
            plan.steps
        );
        assert_eq!(plan.deletes.files, 0);
    }

    #[test]
    fn 落点被清单之外的文件占着就不覆盖() {
        let plan = plan(
            &子库(None),
            &期望状态(vec![期望("GB/一.zip", 1024)]),
            &Manifest::empty(),
            &实际状态(vec![在目标上("GB/一.zip", 999)]),
            Options::default(),
        );
        assert!(plan.steps.is_empty(), "不覆盖任何清单之外的文件");
        assert_eq!(plan.surprises.len(), 1);
        assert_eq!(plan.surprises[0].kind, SurpriseKind::Occupied);
        assert!(plan.surprises[0].still_wanted);
    }

    #[test]
    fn 清单说有实际没了_报告而不静默补回() {
        let desired = 期望状态(vec![期望("GB/一.zip", 1024)]);
        let manifest = Manifest {
            files: vec![清单条("GB/一.zip", 1024)],
        };
        let 计划 = plan(
            &子库(None),
            &desired,
            &manifest,
            &实际状态(vec![]),
            Options::default(),
        );
        assert!(计划.steps.is_empty(), "默认不补回——那可能是有意删的");
        assert_eq!(计划.surprises.len(), 1);
        assert_eq!(计划.surprises[0].kind, SurpriseKind::Gone);
        assert!(计划.surprises[0].still_wanted);

        // 明说要补回时才补，而且照样进报告：明知故犯不是静默。
        let 补回 = plan(
            &子库(None),
            &desired,
            &manifest,
            &实际状态(vec![]),
            Options {
                restore_missing: true,
            },
        );
        assert_eq!(补回.adds.files, 1);
        assert!(补回.steps[0].restore);
        assert_eq!(补回.surprises.len(), 1);
    }

    #[test]
    fn 目标上被改过的既不覆盖也不删() {
        let plan = plan(
            &子库(None),
            // 期望里不要 `改过的.zip` 了，但它在目标上已经不是我们放的那份。
            &期望状态(vec![期望("GB/也改过的.zip", 1024)]),
            &Manifest {
                files: vec![
                    清单条("GB/改过的.zip", 1024),
                    清单条("GB/也改过的.zip", 1024),
                ],
            },
            &实际状态(vec![
                在目标上("GB/改过的.zip", 2048),
                在目标上("GB/也改过的.zip", 2048),
            ]),
            Options::default(),
        );
        assert!(plan.steps.is_empty(), "两条都不动");
        assert_eq!(plan.surprises.len(), 2);
        assert!(
            plan.surprises
                .iter()
                .all(|s| s.kind == SurpriseKind::Changed)
        );
    }

    #[test]
    fn 元数据读不到的目标文件不动它() {
        let plan = plan(
            &子库(None),
            &期望状态(vec![期望("GB/一.zip", 1024)]),
            &Manifest {
                files: vec![清单条("GB/一.zip", 1024), 清单条("GB/二.zip", 1024)],
            },
            &实际状态(vec![
                TargetFile {
                    path: "GB/一.zip".to_string(),
                    stamp: None,
                },
                TargetFile {
                    path: "GB/二.zip".to_string(),
                    stamp: None,
                },
            ]),
            Options::default(),
        );
        assert!(plan.steps.is_empty(), "证明不了一致就不动");
        assert_eq!(plan.surprises.len(), 2);
        assert!(
            plan.surprises
                .iter()
                .all(|s| s.kind == SurpriseKind::Unreadable)
        );
    }

    #[test]
    fn 修改时间取不到就算证明不了一致() {
        let mut 清单 = 清单条("GB/一.zip", 1024);
        清单.stamp.mtime_ns = None;
        let plan = plan(
            &子库(None),
            &期望状态(vec![]),
            &Manifest {
                files: vec![清单]
            },
            &实际状态(vec![在目标上("GB/一.zip", 1024)]),
            Options::default(),
        );
        assert!(plan.steps.is_empty(), "大小对上了也不够");
        assert_eq!(plan.surprises[0].kind, SurpriseKind::Changed);
    }

    #[test]
    fn 超容量报出超出量与按体积排序的裁剪建议_不自动截断() {
        let plan = plan(
            &子库(Some(4096)),
            &期望状态(vec![
                期望("PSV/大.vpk", 4096),
                期望("GB/小.zip", 1024),
                期望("SFC/中.zip", 2048),
            ]),
            &Manifest::empty(),
            &实际状态(vec![]),
            Options::default(),
        );
        assert_eq!(plan.over_capacity, Some(3072));
        assert_eq!(plan.adds.files, 3, "**不自动截断**：三个全在计划里");
        let 建议: Vec<&str> = plan
            .trim_suggestions
            .iter()
            .map(|trim| trim.variant.as_str())
            .collect();
        assert_eq!(建议, vec!["PSV/大.vpk", "SFC/中.zip", "GB/小.zip"]);
    }

    #[test]
    fn 容量账比的是目标现占加净变化() {
        // 卡上的地方是共用的：手动拷进去的存档也占着位置，只算子库自己那一半会给出
        // 一个「装得下」，然后传到一半没空间。
        let plan = plan(
            &子库(Some(4096)),
            &期望状态(vec![期望("GB/一.zip", 2048)]),
            &Manifest::empty(),
            &实际状态(vec![在目标上("saves/存档.sav", 3072)]),
            Options::default(),
        );
        assert_eq!(plan.actual_bytes, 3072);
        assert_eq!(plan.after_bytes, 3072 + 2048);
        assert_eq!(plan.over_capacity, Some(1024));
        assert_eq!(plan.stranger_bytes, 3072);
    }

    #[test]
    fn 落点被占的文件也算清单之外() {
        // ADR-0015 的原话是「清单之外的一切文件对工具不存在」。漏数它，报告就会说出
        // 「目标上没有清单之外的文件」而卡上明明有一个挡在落点上的。
        let plan = plan(
            &子库(None),
            &期望状态(vec![期望("GB/一.zip", 1024)]),
            &Manifest::empty(),
            &实际状态(vec![在目标上("GB/一.zip", 999)]),
            Options::default(),
        );
        assert_eq!(plan.strangers, 1);
        assert_eq!(plan.stranger_bytes, 999);
        assert!(plan.steps.is_empty());
        // 那个文件还在卡上，而这次一步都不走：同步之后目标上就还是那 999。
        assert_eq!(plan.after_bytes, 999);
    }

    #[test]
    fn 删得比加得多时同步之后占得更少() {
        let plan = plan(
            &子库(Some(4096)),
            &期望状态(vec![]),
            &Manifest {
                files: vec![清单条("GB/一.zip", 3000)],
            },
            &实际状态(vec![
                在目标上("GB/一.zip", 3000),
                在目标上("saves/存档.sav", 500),
            ]),
            Options::default(),
        );
        assert_eq!(plan.actual_bytes, 3500);
        assert_eq!(plan.after_bytes, 500, "删掉 3000 之后只剩存档那 500");
        assert_eq!(plan.over_capacity, None);
    }

    #[test]
    fn 同一份输入排两次得到同一份计划() {
        let desired = 期望状态(vec![期望("SFC/二.zip", 2), 期望("GB/一.zip", 1)]);
        let manifest = Manifest {
            files: vec![清单条("MD/三.zip", 3)],
        };
        let actual = 实际状态(vec![在目标上("MD/三.zip", 3)]);
        let 甲 = plan(
            &子库(None),
            &desired,
            &manifest,
            &actual,
            Options::default(),
        );
        let 乙 = plan(
            &子库(None),
            &desired,
            &manifest,
            &actual,
            Options::default(),
        );
        assert_eq!(甲, 乙);
        // 先删后传：删除排在最前。
        assert_eq!(甲.steps[0].act, Act::Delete);
    }

    #[test]
    fn 元数据读不到的成员照样搬_容量按零计() {
        let mut file = 期望("PSV/读不到的.bin", 0);
        file.unreadable = true;
        let plan = plan(
            &子库(None),
            &Desired {
                files: vec![file],
                unreadable: 1,
                ..Desired::default()
            },
            &Manifest::empty(),
            &实际状态(vec![]),
            Options::default(),
        );
        assert_eq!(plan.adds.files, 1);
        assert_eq!(plan.adds.bytes, 0);
        assert_eq!(plan.unreadable_sources, 1);
    }

    #[test]
    fn 一个变体的几个文件折回一个变体数() {
        let mut cue = 期望("PS1/游戏.cue", 100);
        cue.variant = "PS1/游戏.cue".to_string();
        let mut bin = 期望("PS1/游戏.bin", 900);
        bin.variant = "PS1/游戏.cue".to_string();
        let plan = plan(
            &子库(None),
            &期望状态(vec![cue, bin]),
            &Manifest::empty(),
            &实际状态(vec![]),
            Options::default(),
        );
        assert_eq!(plan.adds.files, 2);
        assert_eq!(plan.adds.variants, 1);
        assert_eq!(plan.adds.bytes, 1000);
    }
}
