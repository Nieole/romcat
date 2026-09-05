//! 库体检报告的统计状态。
//!
//! 它**不是**持久状态——事实来源是中立库（ADR-0001）。这份统计每次都由
//! [`Catalog::aggregate`](crate::catalog::Catalog::aggregate) 从中立库里的记录现折出来，
//! 因此「边扫边出的报告」与「盘不在位时从中立库出的报告」必然是同一份数字。
//!
//! 它必须是有界的——例子列表、重复索引都带上限，10T 库不会把它撑爆。

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::catalog::Roots;
use crate::classify::{self, Category, Classification, SuspectReason, classify};
use crate::container::{ContainerKind, FailureReason};
use crate::header::{self, ProbeClass, ProbeOutcome};
use crate::path;
use crate::platform::{Manifest, Platform};
use crate::shape::{self, Role, Scope};

/// 「平台未知」在按平台分组时用的键。
pub const UNKNOWN_PLATFORM: &str = "";

/// 没有扩展名的文件在按扩展名分组时用的键。
pub const NO_EXTENSION: &str = "（无扩展名）";

/// 一组文件数与字节数。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    /// 文件数。
    pub files: u64,
    /// 字节数。
    pub bytes: u64,
}

impl Counts {
    fn add(&mut self, bytes: u64) {
        self.files += 1;
        self.bytes += bytes;
    }
}

/// 一个扩展名的统计。归类跟着计数一起存——报告因此不必拿扩展名字符串再去反推一次。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ExtensionAcc {
    /// 文件数与字节数。
    pub counts: Counts,
    /// 这个扩展名属于哪一类。
    pub category: Category,
}

/// 一条键落在**范围边界**的哪一格（ADR-0011 修订段）。
///
/// 它是 [`crate::shape::Scope`] 的自有版本：那个借着平台清单，这个要跟着统计一路存下去。
/// 四态收成一个类型而不是「组名 + 目录 + 在不在范围内 + 排除理由」四个字段结伴跑，
/// 是因为四者永远一起出现、也永远一起变；拆开之后每个用它的地方都得自己拼回来一次。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum Placement {
    /// 落在某个平台目录下，值是**平台的规范名**。
    Platform(String),
    /// 落在**明确排除**的目录下：看过之后决定不做的。
    Excluded {
        /// 排除理由。
        reason: String,
    },
    /// 落在一个还没映射到平台的顶层目录下。
    Unmapped,
    /// 直接躺在库根下，连顶层目录都没有——平台无从谈起。
    #[default]
    RootLevel,
}

impl Placement {
    /// 这一格进不进识别管线。
    #[must_use]
    pub fn in_scope(&self) -> bool {
        matches!(self, Self::Platform(_))
    }

    /// 明确排除的理由；不是那一格时是 `None`。
    #[must_use]
    pub fn excluded_reason(&self) -> Option<&str> {
        match self {
            Self::Excluded { reason } => Some(reason),
            _ => None,
        }
    }
}

/// 一个平台（或者一个还没映射到平台的顶层目录）下的统计。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PlatformAcc {
    /// 这一组的合计。
    pub totals: Counts,
    /// 喂给这一组的顶层目录名。一个平台可以有好几个别名目录（`ps` 与 `ps1`）。
    pub dirs: BTreeSet<String>,
    /// 这一组落在范围边界的哪一格。
    pub placement: Placement,
    /// 这一组里在范围之内、却没进任何变体的文件，按归类分。
    ///
    /// **「未归类」那一栏大就是成型的缺口**：既不是媒体也不是文档垃圾，却没成型，
    /// 说明这个平台的成型规则漏掉了一批东西。把它与媒体、文档混成一个数，
    /// 「收敛得好」与「压根没成型」会给出同一个答案。
    pub unshaped: BTreeMap<Category, Counts>,
    /// 按三类主线分组。
    pub categories: BTreeMap<Category, Counts>,
    /// 按扩展名分组。
    pub extensions: BTreeMap<String, ExtensionAcc>,
    /// 文件名含汉字的部分。
    pub cjk: Counts,
    /// 这一组成型出了几个**变体**。范围之外的一律是 0——它们根本不成型。
    pub variants: u64,
}

/// 一个平台名下的变体计数。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantCounts {
    /// 变体数。
    pub variants: u64,
    /// 这些变体一共吃掉多少个文件。
    pub files: u64,
    /// 字节合计。
    pub bytes: u64,
}

/// **成型**的统计，从中立库的变体表折出来。
///
/// 它回答这张票最要紧的那个问题：**从文件收敛到了多少个变体**。PSV 那 171,073 个文件
/// 若没收敛到几百个变体，说明成型规则漏掉了那批目录树转储。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ShapingAcc {
    /// 成型跑到哪一次遍历为止；从没成型过时是 `None`。
    pub shaped_scan: Option<i64>,
    /// 成型用的平台清单与眼下这一份不是同一份。
    pub manifest_changed: bool,
    /// 变体数。
    pub variants: u64,
    /// 进了变体的文件数。
    pub files: u64,
    /// 进了变体的字节数。**这是个下界**（见 `unreadable_files`）。
    pub bytes: u64,
    /// 变体成员里有几个元数据读不到，因此容量少算了它们（ADR-0021）。
    pub unreadable_files: u64,
    /// 其中人工纠正出来的变体数。
    pub manual: u64,
    /// 按平台分。
    pub by_platform: BTreeMap<String, VariantCounts>,
    /// 按成型规则分：哪条规则成了几个变体。
    pub by_rule: BTreeMap<String, u64>,
    /// 成员按身份分：主文件 / 附属文件 / 内部资源 / 附属内容各几个。
    pub by_role: BTreeMap<Role, u64>,
}

impl ShapingAcc {
    /// 成型跑过没有。
    #[must_use]
    pub fn shaped(&self) -> bool {
        self.shaped_scan.is_some()
    }
}

/// 一条「目录说 A、内容是 B」的冲突凭什么算数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ConflictEvidence {
    /// 扩展名说了话，**头部抽样也确认了**内容确实是那个格式。这一档最硬。
    Confirmed,
    /// 只有扩展名说了话——这个文件没被抽样到，内容没验过。
    ExtensionOnly,
    /// 冲突在**透明容器内部**：容器躺在 A 平台目录下，里面装着 B 平台的东西。
    InsideContainer,
}

impl ConflictEvidence {
    /// 报告里用的名字。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Confirmed => "内容已确认",
            Self::ExtensionOnly => "仅凭扩展名",
            Self::InsideContainer => "容器内部",
        }
    }

    /// 报告里固定的排列顺序。
    #[must_use]
    pub fn all() -> [Self; 3] {
        [Self::Confirmed, Self::ExtensionOnly, Self::InsideContainer]
    }
}

/// 一条平台冲突。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformConflict {
    /// 展示用路径。容器内部的写成 `容器路径 › 内部路径`。
    pub path: String,
    /// 目录声明的平台。
    pub declared: String,
    /// 文件说自己是哪个平台。
    pub implied: String,
    /// 凭什么算数。
    pub evidence: ConflictEvidence,
}

/// 目录声明的平台与文件内容对不上的那些。
///
/// 这不是错误而是**库体检最该报告的产出之一**（ADR-0011）：目录是强先验而非权威，
/// 下错、放错、压缩包混装都是真实会发生的。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ConflictAcc {
    /// 按凭据分类计数。
    pub by_evidence: BTreeMap<ConflictEvidence, u64>,
    /// 样例，有上限。
    pub examples: Vec<PlatformConflict>,
}

impl ConflictAcc {
    /// 一共几条。
    #[must_use]
    pub fn total(&self) -> u64 {
        self.by_evidence.values().sum()
    }
}

/// 一组重复拷贝：同名同大小的多份。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DuplicateGroup {
    /// 单份的字节数。
    pub size: u64,
    /// 这一组里有几份。
    pub count: u64,
    /// 组内的路径，最多 [`Limits::max_duplicate_paths_per_group`] 条。
    /// 少于 `count` 条就说明有几份没记下路径。
    pub paths: Vec<String>,
}

/// 一类文件的头部抽样结果。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SampleAcc {
    /// 抽了几个。
    pub sampled: u64,
    /// 解析成功几个。
    pub parsed: u64,
    /// 结构对不上几个。
    pub mismatched: u64,
    /// 文件太短几个。
    pub too_short: u64,
    /// 读不动几个。
    pub unreadable: u64,
    /// 失败样例：路径与原因。
    pub failures: Vec<(String, String)>,
}

/// 一种**透明容器**格式的穿透统计。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerKindAcc {
    /// 库里有几个这种容器（穿透了的加穿不透的）。
    pub containers: u64,
    /// 穿透成功几个。
    pub penetrated: u64,
    /// 穿不透几个。
    pub failed: u64,
    /// **还没读过**几个：中立库里没有这个容器的内部构成。
    ///
    /// 它与穿不透是两件事：穿不透是试过了读不出来，这个是压根还没试。zst 默认不读
    /// （穿不透，要全量解压，几小时起），`--no-containers` 扫过的那一趟也落在这里。
    /// 不单列一栏的话，这批容器会从报告里整个消失——那比数字难看糟得多。
    pub unread: u64,
    /// 其中 solid 的有几个（有块装了多于一个内部文件）。
    pub solid: u64,
    /// 内部文件数。
    pub inner_files: u64,
    /// 内部文件的未压缩字节合计。
    pub inner_bytes: u64,
    /// 容器没记 CRC-32 的内部文件数。它们进不了零解压的第一命中层。
    pub inner_without_crc: u64,
    /// **要完整解压才认得出来**的容器数：有内容的条目一条 CRC-32 都没有。
    ///
    /// 眼下有两种走到这一档的：只写 BLAKE2sp 的 RAR5（`rar a -htb`，而 DAT 一条
    /// BLAKE2 都不记），以及带密码、校验和被密钥搅过的（ADR-0014）。它们**穿透了**
    /// ——名字与大小都在——只是第一命中层用不上，与 zst 同一档处置。
    pub needs_full_decompress: u64,
    /// 一共有几个块。
    pub blocks: u64,
}

/// 一个穿透成功的容器折出来的、要并入统计的事实。
///
/// 与 [`ContainerKindAcc`] 分开：那个是累加器，这个是**一个**容器的事实。
/// 两者混用会让「这一条给的 `containers` 到底算不算数」变成读代码才知道的事。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ContainerFacts {
    /// 内部文件数（目录条目不算）。
    pub inner_files: u64,
    /// 内部文件的未压缩字节合计。
    pub inner_bytes: u64,
    /// 容器没记 CRC-32 的内部文件数。
    pub inner_without_crc: u64,
    /// 块数。
    pub blocks: u64,
    /// 是不是 solid。
    pub solid: bool,
}

/// 穿透**透明容器**的统计。
///
/// 它回答的是这张票的那句话：报告要看得见容器**内部的真实构成**，
/// 而不只是「这里有一个 3GB 的容器」。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ContainerAcc {
    /// 按格式分。
    pub by_kind: BTreeMap<ContainerKind, ContainerKindAcc>,
    /// 内部文件按三类主线的构成。
    pub inner_categories: BTreeMap<Category, Counts>,
    /// 内部文件按扩展名的构成。
    pub inner_extensions: BTreeMap<String, ExtensionAcc>,
    /// 名字**连编码都探不出来**、只能有损转换的内部条目数（票 11 收窄了这个含义，
    /// 见 `container::InnerEntry::name_lossy`）。
    pub lossy_names: u64,
    /// 穿不透的按原因分类计数。
    pub failures_by_reason: BTreeMap<FailureReason, u64>,
    /// 穿不透的样例：容器路径与原因。
    pub failures: Vec<(String, String)>,
}

impl ContainerAcc {
    /// 全部格式加起来的合计。
    #[must_use]
    pub fn totals(&self) -> ContainerKindAcc {
        let mut total = ContainerKindAcc::default();
        for acc in self.by_kind.values() {
            total.containers += acc.containers;
            total.penetrated += acc.penetrated;
            total.failed += acc.failed;
            total.unread += acc.unread;
            total.solid += acc.solid;
            total.inner_files += acc.inner_files;
            total.inner_bytes += acc.inner_bytes;
            total.inner_without_crc += acc.inner_without_crc;
            total.needs_full_decompress += acc.needs_full_decompress;
            total.blocks += acc.blocks;
        }
        total
    }
}

/// 扫描过程中遇到的异常，以及跨平台相关的计数。
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anomalies {
    /// 列目录或读文件失败的次数。
    pub errors: u64,
    /// 失败样例。
    pub error_examples: Vec<String>,
    /// 符号链接数（不跟随）。
    pub symlinks: u64,
    /// 空文件数。**只数真的 0 字节的**——大小未知的不算（ADR-0021）。
    pub zero_length: u64,
    /// 元数据读不到的文件数（ADR-0021 的第三态）。
    ///
    /// 它们**存在**，只是属性不可得：既不是「已变」也不是「已删」，也不是「空文件」。
    /// 单独计一栏，因为混进任何一栏都会说谎。
    pub unreadable: u64,
    /// 非 UTF-8 路径数。
    pub non_utf8_paths: u64,
    /// 超过 `MAX_PATH` 的路径数。
    pub over_max_path: u64,
    /// 超长路径样例。
    pub over_max_path_examples: Vec<String>,
    /// 疑似分卷压缩的分卷数。
    pub split_volume_parts: u64,
    /// 整棵跳过的系统目录数。
    pub skipped_system_dirs: u64,
    /// 被跳过的系统目录样例。跳过什么都要说出来，不能悄悄少扫。
    pub skipped_system_dir_examples: Vec<String>,
    /// 既不是文件也不是目录也不是链接的项。
    pub other_entries: u64,
}

/// 例子列表与索引的上限。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// 每个例子列表最多留几条。
    pub max_examples: usize,
    /// 重复检测索引最多留几个键。
    pub max_duplicate_keys: usize,
    /// 每组重复拷贝最多记几条路径。
    ///
    /// 它与 `max_examples` 分开，因为两者要回答的问题不同：例子列表是「给人看一眼
    /// 长什么样」，而重复拷贝的路径是「照着它决定删哪一份」，后者少一条就少一个
    /// 可处理的对象。要导出完整清单时把它放开到 [`Limits::FULL_DUPLICATE_PATHS_PER_GROUP`]。
    pub max_duplicate_paths_per_group: usize,
}

impl Limits {
    /// 导出完整重复拷贝清单时，每组保留的路径数上限。
    ///
    /// 不设成无上限：断点里存着这份索引，一个病态目录（同名同大小的几十万份）
    /// 能把它撑爆。一万条足够覆盖任何真实情况——真库里最大的一组也就个位数份——
    /// 且真被截断时清单会明说少了几份。
    pub const FULL_DUPLICATE_PATHS_PER_GROUP: usize = 10_000;
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_examples: 10,
            max_duplicate_keys: 2_000_000,
            max_duplicate_paths_per_group: 10,
        }
    }
}

/// 一个文件的观察结果。
///
/// 除了 `len`、`non_utf8` 与 `sample`，其余全是**键的纯函数**，由 [`FileObservation::derive`]
/// 现算。这不是省事：它保证「边扫边出的报告」与「从中立库出的报告」不可能给出两套数字，
/// 因为两条路走的是同一个函数。
#[derive(Debug, Clone)]
pub struct FileObservation {
    /// 展示用的完整路径。
    pub display_path: String,
    /// 文件名转小写，用于重复检测。
    pub name_lower: String,
    /// 这个文件落在范围边界的哪一格。
    pub placement: Placement,
    /// 键第一级的那个目录名。平台由目录给出，但一个平台可以有好几个别名目录。
    pub dir: Option<String>,
    /// 它进了哪个变体（的身份）；`None` 表示没进任何变体。
    pub role: Option<Role>,
    /// 目录声明的平台与文件说自己是什么对不上；对得上或说不准时是 `None`。
    pub conflict: Option<PlatformConflict>,
    /// 小写扩展名。
    pub extension: Option<String>,
    /// 字节数；`None` 表示**元数据读不到**（不是「0 字节」，见 ADR-0021）。
    pub len: Option<u64>,
    /// 归类结论。
    pub classification: Classification,
    /// 文件名是否含汉字。
    pub cjk: bool,
    /// 路径能否无损表示成 UTF-8。
    pub non_utf8: bool,
    /// 路径是否超过 `MAX_PATH`。
    pub over_max_path: bool,
    /// 这个文件所属的格式有没有探针。没有探针的类，抽样成功率无从谈起，
    /// 但报告必须说出「有多少内容根本没被抽样覆盖」。
    pub has_probe: bool,
    /// 头部抽样结果；`None` 表示这个文件没被抽到。
    pub sample: Option<(ProbeClass, SampleResult)>,
}

impl FileObservation {
    /// 从中立库的一条记录还原出观察结果。
    ///
    /// `key` 是那条记录的键（根名 + 相对那个根、NFC），`roots` 拿它第一段查出那个根
    /// 在盘上的位置，好把展示路径拼回来。平台由清单从**根名之后**那一级目录名折出来
    /// （ADR-0011）。
    #[must_use]
    pub fn derive(
        manifest: &Manifest,
        roots: &Roots,
        key: &str,
        len: Option<u64>,
        non_utf8: bool,
        sample: Option<(ProbeClass, SampleResult)>,
        role: Option<Role>,
    ) -> Self {
        let display_path = roots.display_key(key);
        // 归类只看文件名，因此这里传的是文件名而不是整条键——键里的 `/`
        // 交给 `Path` 拆会在 Windows 与 Unix 上给出不同答案。
        let name = Path::new(path::file_name_of_key(key));
        let scope = shape::scope_of(manifest, key);
        let dir = path::platform_of_key(key);
        let extension = path::extension_lower(name);
        let conflict = extension.as_deref().and_then(|extension| {
            let (declared, implied) = conflicting_platform(manifest, scope.platform(), extension)?;
            Some(PlatformConflict {
                path: display_path.clone(),
                declared,
                implied,
                evidence: evidence_of(sample.as_ref())?,
            })
        });
        Self {
            over_max_path: path::exceeds_max_path(&display_path),
            display_path,
            name_lower: path::file_name_lower(name),
            placement: match scope {
                Scope::Platform(platform) => Placement::Platform(platform.name.clone()),
                Scope::Excluded(reason) => Placement::Excluded {
                    reason: reason.to_string(),
                },
                Scope::Unmapped => Placement::Unmapped,
                Scope::RootLevel => Placement::RootLevel,
            },
            dir: dir.map(ToString::to_string),
            role,
            conflict,
            extension,
            len,
            classification: classify(name),
            cjk: classify::has_cjk(name),
            non_utf8,
            has_probe: header::probe_class_for(name).is_some(),
            sample,
        }
    }
}

/// 目录声明的平台与这个扩展名说的平台对不对得上；对得上或说不准时是 `None`。
///
/// 两条闸，少一条这份清单就成了噪音：
///
/// 1. **只看在范围内的东西**。未映射的顶层目录本来就不进识别管线，拿它报冲突没有意义。
/// 2. **扩展名说不准就不报**。`.iso` / `.bin` / `.zip` 跨平台，清单里那张表故意只填
///    「只可能属于这一个平台」的（见 `platform/platforms.toml`）。
///
/// 裸文件与**容器内部文件**共用这一个判据——两处各写一遍的话，改一条就会有一处漏改。
fn conflicting_platform(
    manifest: &Manifest,
    declared: Option<&Platform>,
    extension: &str,
) -> Option<(String, String)> {
    let declared = declared?;
    let implied = manifest.platform_for_extension(extension)?;
    (implied.name != declared.name).then(|| (declared.name.clone(), implied.name.clone()))
}

/// 这条冲突凭什么算数；`None` 表示**根本不算冲突**。
///
/// 头部抽样说「对不上」的一律不算：那说明扩展名本身在撒谎，是抽样成功率那一栏要回答的
/// 问题；把它记成平台冲突，等于拿一个假前提去下另一个结论。
fn evidence_of(sample: Option<&(ProbeClass, SampleResult)>) -> Option<ConflictEvidence> {
    match sample {
        Some((_, SampleResult::Probed(outcome))) if outcome.is_parsed() => {
            Some(ConflictEvidence::Confirmed)
        }
        Some((_, SampleResult::Probed(_))) => None,
        _ => Some(ConflictEvidence::ExtensionOnly),
    }
}

/// 并入一个容器内部文件时，那个容器的上下文。
///
/// 单独一个类型而不是三个参数：`record_inner_entry` 每个内部条目调一次，
/// 而这三样对同一个容器是不变的。
#[derive(Debug, Clone, Copy)]
pub struct InnerEntryContext<'a> {
    /// 平台清单。
    pub manifest: &'a Manifest,
    /// 容器的展示路径。
    pub display_path: &'a str,
    /// 容器落在范围的哪一格。
    pub scope: Scope<'a>,
}

/// 一次头部抽样的结果。存进中立库，未变的文件下次扫描直接沿用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SampleResult {
    /// 读到了头部，探针给出结论。
    Probed(ProbeOutcome),
    /// 文件读不动。
    Unreadable(String),
}

/// 扫描累积状态。
/// **它不是持久状态**——断点里不装它，中立库才是事实来源（ADR-0001）。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Aggregate {
    /// 全库合计。
    pub totals: Counts,
    /// 目录数。
    pub dirs: u64,
    /// 按**平台**分组：认出平台的用平台的规范名，没认出的用那个顶层目录名，
    /// [`UNKNOWN_PLATFORM`] 是直接躺在库根下的散文件。
    pub platforms: BTreeMap<String, PlatformAcc>,
    /// 全库扩展名构成。
    pub extensions: BTreeMap<String, ExtensionAcc>,
    /// 全库三类构成。
    pub categories: BTreeMap<Category, Counts>,
    /// 疑似不该入库的各类计数。
    pub suspects: BTreeMap<SuspectReason, Counts>,
    /// 疑似不该入库的样例路径。
    pub suspect_examples: BTreeMap<SuspectReason, Vec<String>>,
    /// 重复检测索引，键是「字节数 + 小写文件名」。
    pub duplicate_index: BTreeMap<String, DuplicateGroup>,
    /// 重复检测索引是否因为超过上限而不再收新键。
    pub duplicate_index_truncated: bool,
    /// 各类文件的头部抽样结果。
    pub samples: BTreeMap<ProbeClass, SampleAcc>,
    /// 穿透**透明容器**的统计。
    pub containers: ContainerAcc,
    /// **成型**的统计，从中立库的变体表折出来。
    pub shaping: ShapingAcc,
    /// 目录声明的平台与文件内容对不上的那些（ADR-0011）。
    pub conflicts: ConflictAcc,
    /// 范围之内、却没进任何变体的文件，按归类分。
    pub unshaped: BTreeMap<Category, Counts>,
    /// 异常与跨平台计数。
    pub anomalies: Anomalies,
    /// 文件名含汉字的部分。
    pub cjk: Counts,
    /// 属于三类主线、却没有任何探针可用的文件数。
    pub content_files_without_probe: u64,
    /// 累计耗时，含此前几次续跑。
    pub elapsed_ms: u64,
}

fn push_capped(list: &mut Vec<String>, value: String, limit: usize) {
    if list.len() < limit {
        list.push(value);
    }
}

impl Aggregate {
    /// 并入一个文件的观察结果。
    pub fn record_file(&mut self, observation: &FileObservation, limits: &Limits) {
        let FileObservation {
            len,
            classification,
            ..
        } = observation;
        // 大小未知的文件**存在**，因此照常计入文件数；但它的字节数无从得知，
        // 按 0 计入容量。报告会把这批的个数单独报出来，读数的人才知道容量是个下界。
        let len = len.unwrap_or(0);

        self.totals.add(len);
        // 分组名：认出平台就用平台的规范名，没认出就用那个顶层目录名，
        // 库根下的散文件归 [`UNKNOWN_PLATFORM`]。
        let platform_key = match &observation.placement {
            Placement::Platform(name) => name.clone(),
            _ => observation
                .dir
                .clone()
                .unwrap_or_else(|| UNKNOWN_PLATFORM.to_string()),
        };
        let platform = self.platforms.entry(platform_key).or_default();
        platform.totals.add(len);
        platform.placement.clone_from(&observation.placement);
        if let Some(dir) = &observation.dir {
            platform.dirs.insert(dir.clone());
        }
        // 范围之内、却没进任何变体的文件按归类记一笔。
        if observation.placement.in_scope() && observation.role.is_none() {
            platform
                .unshaped
                .entry(classification.category)
                .or_default()
                .add(len);
            self.unshaped
                .entry(classification.category)
                .or_default()
                .add(len);
        }
        platform
            .categories
            .entry(classification.category)
            .or_default()
            .add(len);
        self.categories
            .entry(classification.category)
            .or_default()
            .add(len);

        let ext_key = observation
            .extension
            .clone()
            .unwrap_or_else(|| NO_EXTENSION.to_string());
        for map in [&mut platform.extensions, &mut self.extensions] {
            let acc = map.entry(ext_key.clone()).or_default();
            acc.counts.add(len);
            acc.category = classification.category;
        }

        if observation.cjk {
            platform.cjk.add(len);
            self.cjk.add(len);
        }

        if let Some(reason) = classification.suspect {
            self.suspects.entry(reason).or_default().add(len);
            push_capped(
                self.suspect_examples.entry(reason).or_default(),
                observation.display_path.clone(),
                limits.max_examples,
            );
        }

        // 「大小未知」不是「大小为零」：库里有 4,317 个真正的空文件，
        // 混在一起两个数字都会说谎（ADR-0021）。
        if observation.len.is_none() {
            self.anomalies.unreadable += 1;
        } else if len == 0 {
            self.anomalies.zero_length += 1;
        }
        if observation.non_utf8 {
            self.anomalies.non_utf8_paths += 1;
        }
        if observation.over_max_path {
            self.anomalies.over_max_path += 1;
            push_capped(
                &mut self.anomalies.over_max_path_examples,
                observation.display_path.clone(),
                limits.max_examples,
            );
        }
        if classification.split_volume {
            self.anomalies.split_volume_parts += 1;
        }

        let is_content = matches!(
            classification.category,
            Category::TransparentContainer | Category::CompressedImage | Category::BareFile
        );
        if is_content && !observation.has_probe {
            self.content_files_without_probe += 1;
        }

        if let Some(conflict) = &observation.conflict {
            self.record_conflict(conflict.clone(), limits);
        }

        self.record_duplicate_candidate(observation, limits);
        self.record_sample(observation, limits);
    }

    /// 并入一条平台冲突。
    pub fn record_conflict(&mut self, conflict: PlatformConflict, limits: &Limits) {
        *self
            .conflicts
            .by_evidence
            .entry(conflict.evidence)
            .or_default() += 1;
        if self.conflicts.examples.len() < limits.max_examples {
            self.conflicts.examples.push(conflict);
        }
    }

    /// 重复检测只覆盖库的内容本身——媒体、元数据与垃圾文件同名同大小是常态，
    /// 报出来只会淹没真正的重复拷贝。
    fn record_duplicate_candidate(&mut self, observation: &FileObservation, limits: &Limits) {
        let is_content = matches!(
            observation.classification.category,
            Category::TransparentContainer | Category::CompressedImage | Category::BareFile
        );
        // 大小未知的不参与：判据是「同名同大小」，大小都没有就谈不上同不同。
        let Some(len) = observation.len.filter(|len| *len > 0) else {
            return;
        };
        if !is_content {
            return;
        }
        let key = format!("{len}|{}", observation.name_lower);
        match self.duplicate_index.get_mut(&key) {
            Some(group) => {
                group.count += 1;
                push_capped(
                    &mut group.paths,
                    observation.display_path.clone(),
                    limits.max_duplicate_paths_per_group,
                );
            }
            None => {
                if self.duplicate_index.len() >= limits.max_duplicate_keys {
                    self.duplicate_index_truncated = true;
                    return;
                }
                self.duplicate_index.insert(
                    key,
                    DuplicateGroup {
                        size: len,
                        count: 1,
                        paths: vec![observation.display_path.clone()],
                    },
                );
            }
        }
    }

    /// 并入一个穿透成功的容器。
    pub fn record_penetrated(&mut self, kind: ContainerKind, facts: &ContainerFacts) {
        let acc = self.containers.by_kind.entry(kind).or_default();
        acc.containers += 1;
        acc.penetrated += 1;
        acc.solid += u64::from(facts.solid);
        acc.inner_files += facts.inner_files;
        acc.inner_bytes += facts.inner_bytes;
        acc.inner_without_crc += facts.inner_without_crc;
        // 判据从已经落库的两个数里减出来，不必为它加一列：有内容的条目全都没有
        // CRC-32，这个容器就只能靠完整解压认出来。
        acc.needs_full_decompress +=
            u64::from(facts.inner_files > 0 && facts.inner_without_crc == facts.inner_files);
        acc.blocks += facts.blocks;
    }

    /// 并入一批**还没读过**的容器：库里有这么多个，中立库里没有它们的内部构成。
    ///
    /// 整批并入而不是逐个记，是因为这个数是**减出来的**——库里有多少个减去
    /// `container` 表里有多少行。逐个记就得为它单开一趟全表扫描，而 `entry` 那张表
    /// 在 10T 库上有二十几万行，白走一趟不划算。
    pub fn record_containers_unread(&mut self, kind: ContainerKind, count: u64) {
        if count == 0 {
            return;
        }
        let acc = self.containers.by_kind.entry(kind).or_default();
        acc.containers += count;
        acc.unread += count;
    }

    /// 并入一个穿不透的容器。
    pub fn record_penetration_failure(
        &mut self,
        display_path: &str,
        kind: ContainerKind,
        reason: FailureReason,
        detail: &str,
        limits: &Limits,
    ) {
        let acc = self.containers.by_kind.entry(kind).or_default();
        acc.containers += 1;
        acc.failed += 1;
        *self
            .containers
            .failures_by_reason
            .entry(reason)
            .or_default() += 1;
        if self.containers.failures.len() < limits.max_examples {
            self.containers
                .failures
                .push((display_path.to_string(), detail.to_string()));
        }
    }

    /// 并入一个容器内部文件。目录条目不进来——它们不是内容。
    ///
    /// `container` 是那个容器的展示路径与所在范围，用来认出「容器躺在 A 平台目录下，
    /// 里面装着 B 平台的东西」。库里 91.1% 的容量在透明容器里，把容器内部排除在
    /// 冲突检测之外等于放过大头。
    pub fn record_inner_entry(
        &mut self,
        container: &InnerEntryContext<'_>,
        inner_path: &str,
        size: u64,
        name_lossy: bool,
        limits: &Limits,
    ) {
        if name_lossy {
            self.containers.lossy_names += 1;
        }
        let name = Path::new(path::file_name_of_key(inner_path));
        if let Some(extension) = path::extension_lower(name)
            && let Some((declared, implied)) =
                conflicting_platform(container.manifest, container.scope.platform(), &extension)
        {
            self.record_conflict(
                PlatformConflict {
                    path: format!("{} › {inner_path}", container.display_path),
                    declared,
                    implied,
                    evidence: ConflictEvidence::InsideContainer,
                },
                limits,
            );
        }
        let classification = classify(name);
        self.containers
            .inner_categories
            .entry(classification.category)
            .or_default()
            .add(size);
        let key = path::extension_lower(name).unwrap_or_else(|| NO_EXTENSION.to_string());
        let acc = self.containers.inner_extensions.entry(key).or_default();
        acc.counts.add(size);
        acc.category = classification.category;
    }

    fn record_sample(&mut self, observation: &FileObservation, limits: &Limits) {
        let Some((class, result)) = &observation.sample else {
            return;
        };
        let acc = self.samples.entry(*class).or_default();
        acc.sampled += 1;
        let failure = match result {
            SampleResult::Probed(ProbeOutcome::Parsed(_)) => {
                acc.parsed += 1;
                None
            }
            SampleResult::Probed(ProbeOutcome::Mismatch(reason)) => {
                acc.mismatched += 1;
                Some(reason.clone())
            }
            SampleResult::Probed(ProbeOutcome::TooShort) => {
                acc.too_short += 1;
                Some("文件太短，够不到该看的偏移".to_string())
            }
            SampleResult::Unreadable(reason) => {
                acc.unreadable += 1;
                Some(reason.clone())
            }
        };
        if let Some(reason) = failure
            && acc.failures.len() < limits.max_examples
        {
            acc.failures
                .push((observation.display_path.clone(), reason));
        }
    }

    /// 记下一个整棵跳过的系统目录。跳过什么都要说出来，不能悄悄少扫。
    pub fn record_skipped_system_dir(&mut self, display_path: &str, limits: &Limits) {
        self.anomalies.skipped_system_dirs += 1;
        push_capped(
            &mut self.anomalies.skipped_system_dir_examples,
            display_path.to_string(),
            limits.max_examples,
        );
    }

    /// 记下一个列不开的目录。列不开不中断扫描——读不到的东西也是体检结论的一部分。
    pub fn record_unlistable_dir(&mut self, display_path: &str, error: &str, limits: &Limits) {
        self.anomalies.errors += 1;
        push_capped(
            &mut self.anomalies.error_examples,
            format!("{display_path} — {error}"),
            limits.max_examples,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 观察(key: &str, len: u64) -> FileObservation {
        FileObservation::derive(
            &Manifest::builtin(),
            &Roots::single("库", "/lib"),
            key,
            Some(len),
            false,
            None,
            None,
        )
    }

    fn 读不到元数据的观察(key: &str) -> FileObservation {
        FileObservation::derive(
            &Manifest::builtin(),
            &Roots::single("库", "/lib"),
            key,
            None,
            false,
            None,
            None,
        )
    }

    #[test]
    fn 同名同大小的多份被归成一组重复拷贝() {
        let mut agg = Aggregate::default();
        let limits = Limits::default();
        agg.record_file(&观察("FC/马里奥.zip", 1024), &limits);
        agg.record_file(&观察("FC/备份/马里奥.zip", 1024), &limits);
        agg.record_file(&观察("MD/马里奥.zip", 2048), &limits);

        let groups: Vec<_> = agg
            .duplicate_index
            .values()
            .filter(|g| g.count > 1)
            .collect();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].count, 2);
        assert_eq!(groups[0].size, 1024);
    }

    #[test]
    fn 媒体与元数据不参与重复检测() {
        let mut agg = Aggregate::default();
        let limits = Limits::default();
        agg.record_file(&观察("FC/media/cover.png", 100), &limits);
        agg.record_file(&观察("MD/media/cover.png", 100), &limits);
        assert!(agg.duplicate_index.is_empty());
    }

    #[test]
    fn 平台未知的文件照常计入() {
        let mut agg = Aggregate::default();
        agg.record_file(&观察("散落的游戏.gba", 512), &Limits::default());
        assert_eq!(agg.totals.files, 1);
        assert_eq!(agg.platforms[UNKNOWN_PLATFORM].totals.files, 1);
    }

    #[test]
    fn 大小未知不等于大小为零() {
        let mut agg = Aggregate::default();
        let limits = Limits::default();
        agg.record_file(&观察("FC/空文件.zip", 0), &limits);
        agg.record_file(&读不到元数据的观察("FC/读不到.zip"), &limits);

        assert_eq!(agg.anomalies.zero_length, 1, "只有真的 0 字节才算空文件");
        assert_eq!(agg.anomalies.unreadable, 1, "读不到的单独计一栏");
        assert_eq!(agg.totals.files, 2, "两个都存在，都要计入文件数");
        assert_eq!(agg.totals.bytes, 0, "未知大小按 0 计入容量，不凭空编数字");
    }

    #[test]
    fn 大小未知的文件不参与重复检测() {
        let mut agg = Aggregate::default();
        let limits = Limits::default();
        agg.record_file(&读不到元数据的观察("FC/马里奥.zip"), &limits);
        agg.record_file(&读不到元数据的观察("FC/备份/马里奥.zip"), &limits);
        assert!(
            agg.duplicate_index.is_empty(),
            "判据是同名同大小，大小都没有就谈不上同不同"
        );
    }

    #[test]
    fn 例子列表有上限() {
        let mut agg = Aggregate::default();
        let limits = Limits {
            max_examples: 2,
            ..Limits::default()
        };
        for i in 0..10 {
            agg.record_file(&观察(&format!("FC/说明{i}.txt"), 10), &limits);
        }
        assert_eq!(agg.suspects[&SuspectReason::Document].files, 10);
        assert_eq!(agg.suspect_examples[&SuspectReason::Document].len(), 2);
    }

    #[test]
    fn 重复索引超过上限后不再收新键() {
        let mut agg = Aggregate::default();
        let limits = Limits {
            max_duplicate_keys: 1,
            ..Limits::default()
        };
        agg.record_file(&观察("FC/a.zip", 1), &limits);
        agg.record_file(&观察("FC/b.zip", 2), &limits);
        assert_eq!(agg.duplicate_index.len(), 1);
        assert!(agg.duplicate_index_truncated);
    }

    #[test]
    fn 每组重复拷贝的路径数可以放开到记全() {
        let 存了几条 = |limit: usize| {
            let mut agg = Aggregate::default();
            let limits = Limits {
                max_duplicate_paths_per_group: limit,
                ..Limits::default()
            };
            for i in 0..12 {
                agg.record_file(&观察(&format!("FC/备份{i}/魂斗罗.zip"), 1024), &limits);
            }
            let group = agg
                .duplicate_index
                .values()
                .find(|g| g.count > 1)
                .expect("有一组重复")
                .clone();
            (group.count, group.paths.len())
        };

        assert_eq!(存了几条(10), (12, 10), "默认只留 10 条，其余 2 份没有路径");
        assert_eq!(
            存了几条(Limits::FULL_DUPLICATE_PATHS_PER_GROUP),
            (12, 12),
            "放开后每一份都有路径"
        );
    }

    #[test]
    fn 派生字段全由键算出() {
        let observation = FileObservation::derive(
            &Manifest::builtin(),
            &Roots::single("库", "/lib"),
            "库/PS1/某游戏/disc.cue",
            Some(64),
            false,
            None,
            None,
        );
        assert_eq!(observation.display_path, "/lib/PS1/某游戏/disc.cue");
        assert_eq!(
            observation.placement,
            Placement::Platform("PS1".to_string())
        );
        assert_eq!(observation.dir.as_deref(), Some("PS1"));
        assert!(observation.placement.in_scope());
        assert_eq!(observation.extension.as_deref(), Some("cue"));
        assert_eq!(observation.name_lower, "disc.cue");
        assert_eq!(observation.classification.category, Category::BareFile);
        assert!(!observation.cjk, "汉字在目录名里，不算文件名含汉字");
    }
}
