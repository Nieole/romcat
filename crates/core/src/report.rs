//! 库体检报告：把扫描累积的状态整理成可读、可存档的结论。
//!
//! 报告是这张票的交付物本身——所有「先做哪几个平台」的排序都等它出数据。因此它同时
//! 提供两种形态：给人看的文本，与给后面几票（以及界面）吃的 JSON。
//!
//! 报告只列容量最大的前几组重复拷贝。要照着它动手处理，用 [`DuplicateDetails`]——
//! 那是一份单独导出的完整明细，不塞进报告：大多数场景用不上它，塞进去只会让报告膨胀。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::catalog::ScanDelta;
use crate::classify::{Category, SuspectReason};
use crate::container::{ContainerKind, FailureReason};
use crate::header::ProbeClass;
use crate::scan::aggregate::{
    Aggregate, Anomalies, ConflictAcc, ConflictEvidence, ContainerAcc, Counts, ExtensionAcc,
    Placement, PlatformAcc, PlatformConflict, SampleAcc, ShapingAcc, UNKNOWN_PLATFORM,
};
use crate::shape::Role;

/// 平台未知时在报告里的显示名。
pub const UNKNOWN_PLATFORM_LABEL: &str = "（平台未知）";

/// 报告里每个平台展示几个扩展名。
const TOP_EXTENSIONS_PER_PLATFORM: usize = 5;
/// 报告里全库展示几个扩展名。
const TOP_EXTENSIONS_GLOBAL: usize = 25;
/// 报告里展示几个**容器内部**的扩展名。
const TOP_INNER_EXTENSIONS: usize = 15;
/// 报告里展示几组重复拷贝。
const TOP_DUPLICATE_GROUPS: usize = 10;
/// 报告里列出几个**还没映射**的顶层目录。真库顶层 73 个条目，一多半是这一类。
const TOP_UNMAPPED_DIRS: usize = 15;
/// 报告里每组重复拷贝展示几条路径。
///
/// 它与扫描时**记下**多少条（[`Limits::max_duplicate_paths_per_group`](crate::scan::aggregate::Limits::max_duplicate_paths_per_group)）
/// 是两回事：要导出明细就得记全，但报告不该跟着一起膨胀——JSON 会胖出几个数量级，
/// 文本报告会把整组路径挤在一行。
const TOP_DUPLICATE_PATHS_PER_GROUP: usize = 10;

/// 生成报告时需要的、统计之外的信息。
#[derive(Debug, Clone)]
pub struct ReportMeta {
    /// 这一趟扫的那个**根**叫什么。主库是一组根，一趟只扫一个。
    pub root_name: String,
    /// 那个根当时挂在哪。
    pub root: String,
    /// 这次扫描是否被中断。
    pub interrupted: bool,
    /// 这次扫描是否从断点续跑。
    pub resumed: bool,
    /// 并发线程数。
    pub jobs: usize,
    /// 每类文件的抽样配额。
    pub samples_per_class: usize,
    /// 这次扫描有没有穿透**透明容器**。
    pub penetrated_containers: bool,
    /// 中立库里最后一次遍历的代号。报告拿它与成型的代号比，说得出成型是不是旧的。
    pub scan: i64,
    /// 这次扫描相对上一次的差异；`None` 表示这份报告是直接从中立库出的，没有扫盘。
    pub delta: Option<ScanDelta>,
}

/// 全库合计。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Totals {
    /// 文件数。
    pub files: u64,
    /// 总字节数。
    pub bytes: u64,
    /// 目录数。
    pub dirs: u64,
    /// 文件名含汉字的文件数。
    pub cjk_files: u64,
    /// 文件名含汉字的字节数。
    pub cjk_bytes: u64,
}

/// 一类（透明容器 / 压缩镜像 / 裸文件 / …）的占比。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CategoryStats {
    /// 类别。
    pub category: Category,
    /// 中文名。
    pub label: String,
    /// 文件数。
    pub files: u64,
    /// 字节数。
    pub bytes: u64,
    /// 文件数占比（百分比）。
    pub file_share: f64,
    /// 容量占比（百分比）。
    pub byte_share: f64,
}

/// 一个扩展名的构成。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtensionStats {
    /// 扩展名。
    pub extension: String,
    /// 它属于哪一类。
    pub category: Category,
    /// 文件数。
    pub files: u64,
    /// 字节数。
    pub bytes: u64,
}

/// 一个平台（或者一个还没映射到平台的顶层目录）的统计。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlatformStats {
    /// 认出平台就是平台的规范名，没认出就是那个顶层目录名；
    /// 直接躺在库根下的散文件是 [`UNKNOWN_PLATFORM_LABEL`]。
    pub name: String,
    /// 是否是「平台未知」这一组。
    pub unknown: bool,
    /// 喂给这一组的顶层目录名。一个平台可以有好几个别名目录。
    pub dirs: Vec<String>,
    /// 这一组进不进识别管线（ADR-0011 修订段的范围边界）。
    pub in_scope: bool,
    /// 明确排除的话，为什么。与「还没映射」不是一回事。
    pub excluded_reason: Option<String>,
    /// 成型出了几个**变体**。范围之外的一律是 0——它们根本不成型。
    pub variants: u64,
    /// 文件数。
    pub files: u64,
    /// 字节数。
    pub bytes: u64,
    /// 三类构成。
    pub categories: Vec<CategoryStats>,
    /// 容量最大的几个扩展名。
    pub top_extensions: Vec<ExtensionStats>,
    /// 文件名含汉字的文件数。
    pub cjk_files: u64,
}

/// 一组重复拷贝。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicateGroupStats {
    /// 单份字节数。
    pub size: u64,
    /// 份数。
    pub count: u64,
    /// 组内的路径。可能少于 `count` 条——报告里每组只留几条，完整的在 [`DuplicateDetails`] 里。
    pub paths: Vec<String>,
}

impl DuplicateGroupStats {
    /// 这一组只留一份能腾出多少字节。
    #[must_use]
    pub fn reclaimable_bytes(&self) -> u64 {
        self.size.saturating_mul(self.count.saturating_sub(1))
    }

    /// 这一组有几份没记下路径。要处理这几份得放开
    /// [`Limits::max_duplicate_paths_per_group`](crate::scan::aggregate::Limits::max_duplicate_paths_per_group)
    /// 重扫一遍。
    #[must_use]
    pub fn paths_missing(&self) -> u64 {
        let listed = u64::try_from(self.paths.len()).unwrap_or(u64::MAX);
        self.count.saturating_sub(listed)
    }
}

/// 一类疑似不该入库的内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuspectStats {
    /// 理由。
    pub reason: SuspectReason,
    /// 中文名。
    pub label: String,
    /// 文件数。
    pub files: u64,
    /// 字节数。
    pub bytes: u64,
    /// 路径样例。
    pub examples: Vec<String>,
}

/// 疑似不该入库的汇总。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuspectSummary {
    /// 按理由分类。
    pub by_reason: Vec<SuspectStats>,
    /// 重复拷贝有多少组。
    pub duplicate_groups: u64,
    /// 重复拷贝涉及多少个文件。
    pub duplicate_files: u64,
    /// 只留一份的话能腾出多少字节。**只报告，不自动删**（ADR-0004）。
    pub duplicate_reclaimable_bytes: u64,
    /// 可腾出空间最多的几组。完整明细见 [`DuplicateDetails`]。
    pub top_duplicates: Vec<DuplicateGroupStats>,
    /// 重复索引是否因超过上限而截断。
    pub index_truncated: bool,
}

/// 一类文件的头部抽样结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampleStats {
    /// 探针类别。
    pub class: ProbeClass,
    /// 中文名。
    pub label: String,
    /// 抽了几个。
    pub sampled: u64,
    /// 成功几个。
    pub parsed: u64,
    /// 结构对不上几个。
    pub mismatched: u64,
    /// 文件太短几个。
    pub too_short: u64,
    /// 读不动几个。
    pub unreadable: u64,
    /// 成功率（百分比）。
    pub success_rate: f64,
    /// 失败样例：路径与原因。
    pub failures: Vec<(String, String)>,
}

/// 一种**透明容器**格式的穿透结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContainerKindStats {
    /// 容器格式。
    pub kind: ContainerKind,
    /// 报告里用的名字。
    pub label: String,
    /// 库里有几个这种容器。
    pub containers: u64,
    /// 穿透成功几个。
    pub penetrated: u64,
    /// 穿不透几个。
    pub failed: u64,
    /// **还没读过**几个。与穿不透是两件事：那个是试过读不出来，这个是压根还没试。
    pub unread: u64,
    /// 读出内部构成的成功率（百分比）。分母只算试过的那些——还没读过的不该拉低它。
    pub success_rate: f64,
    /// 其中 solid 的有几个。solid 的容器要按块调度才不至于成倍解压。
    pub solid: u64,
    /// 内部文件数。
    pub inner_files: u64,
    /// 内部文件的未压缩字节合计。
    pub inner_bytes: u64,
    /// 容器没记 CRC-32 的内部文件数。它们进不了零解压的第一命中层。
    pub inner_without_crc: u64,
    /// **要完整解压才认得出来**的容器数：有内容的条目一条 CRC-32 都没有。
    pub needs_full_decompress: u64,
    /// 块数合计。
    pub blocks: u64,
}

/// 穿不透的一类原因有多少个。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureReasonStats {
    /// 原因。
    pub reason: FailureReason,
    /// 中文名。
    pub label: String,
    /// 有几个容器卡在这一类上。
    pub containers: u64,
}

/// 穿透**透明容器**的汇总。
///
/// 它回答的是「容器里到底装着什么」：不解压就读出的内部文件数与构成，
/// 以及有多少内部文件带着可以直接拿去撞 DAT 的 CRC-32。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContainerSummary {
    /// 这次扫描有没有穿透容器。为假时下面的数字来自上一次穿透过的扫描。
    pub penetrated_this_scan: bool,
    /// 按格式分。
    pub by_kind: Vec<ContainerKindStats>,
    /// 容器数合计。
    pub containers: u64,
    /// 穿透成功的容器数。
    pub penetrated: u64,
    /// 穿不透的容器数。
    pub failed: u64,
    /// **还没读过**的容器数：zst 默认不读（穿不透，要全量解压），`--no-containers` 同理。
    pub unread: u64,
    /// 还没读过的那批里，有没有**穿不透的格式**（zst）。
    ///
    /// 报告要照着它说话：zip / 7z / rar 没读是因为 `--no-containers`，zst 没读是因为它没有
    /// 零解压那条路。对着一个 zip 说 zst 的话，读的人会照着一条不成立的结论去动手。
    pub unread_includes_impenetrable: bool,
    /// 内部文件数合计。
    pub inner_files: u64,
    /// 内部文件的未压缩字节合计。
    pub inner_bytes: u64,
    /// 带 CRC-32 的内部文件数——**零解压就能拿去撞 DAT 的那一批**。
    pub inner_with_crc: u64,
    /// 容器没记 CRC-32 的内部文件数。
    pub inner_without_crc: u64,
    /// **要完整解压才认得出来**的容器数：有内容的条目一条 CRC-32 都没有。
    ///
    /// 它与**还没读过**不是一回事：这一档是穿透了、名字与大小都拿到了，只是
    /// 第一命中层用不上（只写 BLAKE2sp 的 RAR5、带密码校验和被搅过的），
    /// 与 zst 同一档处置（ADR-0014）。
    pub needs_full_decompress: u64,
    /// solid 容器数。
    pub solid: u64,
    /// 内部文件按三类主线的构成。
    pub inner_categories: Vec<CategoryStats>,
    /// 内部文件里容量最大的几个扩展名。
    pub inner_extensions: Vec<ExtensionStats>,
    /// 名字只能有损转换的内部条目数。
    pub lossy_names: u64,
    /// 穿不透的按原因分类。**「不是这个格式」与「需要密码」的后续处置完全不同**，
    /// 合成一个数就没法照着它动手。
    pub failures_by_reason: Vec<FailureReasonStats>,
    /// 穿不透的样例：容器路径与原因。
    pub failures: Vec<(String, String)>,
}

/// 一个平台成型出来的变体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlatformShapeStats {
    /// 平台名。
    pub name: String,
    /// 变体数。
    pub variants: u64,
    /// 这些变体一共吃掉多少个文件。
    pub files: u64,
    /// 字节合计。
    pub bytes: u64,
    /// 平均每个变体吃掉多少个文件。
    ///
    /// 它是这张票最要紧的那个数：PSV 那 171,073 个文件若没聚起来，这一栏会是 1.0，
    /// 而那就说明目录树规则整个没生效。
    ///
    /// **不叫「收敛比」**：词表里 **收敛** 专指导出时把同一作品的多个变体合并成前端里的
    /// 一个条目，与这里说的「几个文件聚成一个变体」是两件事，共用一个词会互相污染。
    pub files_per_variant: f64,
}

/// 一条成型规则成了几个变体。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleStats {
    /// 规则名。
    pub rule: String,
    /// 变体数。
    pub variants: u64,
}

/// 一种成员身份有几个。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleStats {
    /// 身份。
    pub role: Role,
    /// 成员数。
    pub members: u64,
}

/// **成型**的汇总：从文件收敛到了多少个变体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapingSummary {
    /// 成型跑过没有。没跑过时下面全是 0，报告会明说。
    pub shaped: bool,
    /// 成型比中立库里最后一次遍历旧——扫过之后没再成型。
    pub stale: bool,
    /// 这份变体表是用**另一份平台清单**成的型。
    pub manifest_changed: bool,
    /// 变体数。
    pub variants: u64,
    /// 进了变体的文件数。
    pub files: u64,
    /// 进了变体的字节数。**这是个下界**：元数据读不到的成员按 0 计入（ADR-0021）。
    pub bytes: u64,
    /// 变体成员里有几个元数据读不到，因此容量少算了它们。
    pub unreadable_files: u64,
    /// 其中人工纠正出来的变体数。
    pub manual: u64,
    /// 范围之内、却没进任何变体的文件，按归类分。
    ///
    /// **「未归类」那一栏大就是成型的缺口**：既不是媒体也不是文档垃圾，却没成型。
    /// 与媒体、文档混成一个数的话，「收敛得好」与「压根没成型」会给出同一个答案。
    pub unshaped: Vec<CategoryStats>,
    /// 全库平均每个变体吃掉多少个文件（不叫「收敛比」，见 [`PlatformShapeStats`]）。
    pub files_per_variant: f64,
    /// 按平台分，变体多的排前面。
    pub by_platform: Vec<PlatformShapeStats>,
    /// 按成型规则分。
    pub by_rule: Vec<RuleStats>,
    /// 成员按身份分。
    pub by_role: Vec<RoleStats>,
}

/// 一个明确排除的顶层目录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExcludedDir {
    /// 目录名。
    pub name: String,
    /// 为什么排除。
    pub reason: String,
    /// 文件数。
    pub files: u64,
    /// 字节数。
    pub bytes: u64,
}

/// **范围边界**：哪些进识别管线、哪些不进（ADR-0011 修订段）。
///
/// 三格分得开是有讲究的：**明确排除**是看过之后决定不做的，**还没映射**是工具不认得
/// 那个目录名，**库根下的散文件**连顶层目录都没有。三者的后续处置完全不同，
/// 合成一个数就没法照着它动手。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeSummary {
    /// 范围之内有几个平台、多少文件、多少字节。
    pub platforms: u64,
    /// 同上。
    pub in_scope_files: u64,
    /// 同上。
    pub in_scope_bytes: u64,
    /// 还没映射的顶层目录数。
    pub unmapped_dirs: u64,
    /// 同上的文件数。
    pub unmapped_files: u64,
    /// 同上的字节数。
    pub unmapped_bytes: u64,
    /// 还没映射的目录，按容量从大到小，有上限。
    pub unmapped_examples: Vec<String>,
    /// 明确排除的目录。
    pub excluded: Vec<ExcludedDir>,
    /// 直接躺在库根下的散文件数。
    pub root_level_files: u64,
    /// 同上的字节数。
    pub root_level_bytes: u64,
}

/// 目录声明的平台与文件内容对不上的汇总。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictSummary {
    /// 一共几条。
    pub total: u64,
    /// 按凭据分类。
    pub by_evidence: Vec<(ConflictEvidence, String, u64)>,
    /// 样例。
    pub examples: Vec<PlatformConflict>,
}

/// 库体检报告。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthReport {
    /// 这一趟扫的那个**根**叫什么。主库是一组根，一趟只扫一个。
    #[serde(default)]
    pub root_name: String,
    /// 那个根当时挂在哪。
    pub root: String,
    /// 这次扫描是否被中断。中断的报告仍然可读，只是不完整。
    pub interrupted: bool,
    /// 是否从断点续跑。
    pub resumed: bool,
    /// 并发线程数。
    pub jobs: usize,
    /// 每类文件的抽样配额。
    pub samples_per_class: usize,
    /// 这次扫描相对中立库上一次状态的差异。
    ///
    /// `None` 表示这份报告直接从中立库折出来，没有碰过磁盘——外置盘不在位时就是这样。
    pub delta: Option<ScanDelta>,
    /// 累计耗时（毫秒），含此前几次续跑。
    pub elapsed_ms: u64,
    /// 全库合计。
    pub totals: Totals,
    /// 按平台目录。
    pub platforms: Vec<PlatformStats>,
    /// 三类构成。
    pub categories: Vec<CategoryStats>,
    /// 扩展名构成。
    pub extensions: Vec<ExtensionStats>,
    /// 疑似不该入库。
    pub suspects: SuspectSummary,
    /// 头部抽样。
    pub samples: Vec<SampleStats>,
    /// 穿透**透明容器**的结果。
    pub containers: ContainerSummary,
    /// **成型**：从文件收敛到了多少个变体。
    pub shaping: ShapingSummary,
    /// 范围边界：哪些进识别管线、哪些不进。
    pub scope: ScopeSummary,
    /// 目录声明的平台与文件内容对不上的那些。
    pub conflicts: ConflictSummary,
    /// 属于三类主线、却没有任何探针可用的文件数。抽样成功率覆盖不到它们。
    pub content_files_without_probe: u64,
    /// 异常与跨平台计数。
    pub anomalies: Anomalies,
}

/// 百分比，保留两位小数。
///
/// 取整不只是为了好看：报告要能存成 JSON 再原样读回来，而两位小数的浮点数有一个
/// 能精确往返的最短表示。
pub(crate) fn share(part: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let raw = part as f64 * 100.0 / total as f64;
    (raw * 100.0).round() / 100.0
}

fn platform_stats(name: &str, acc: &PlatformAcc) -> PlatformStats {
    PlatformStats {
        name: if name == UNKNOWN_PLATFORM {
            UNKNOWN_PLATFORM_LABEL.to_string()
        } else {
            name.to_string()
        },
        unknown: name == UNKNOWN_PLATFORM,
        dirs: acc.dirs.iter().cloned().collect(),
        in_scope: acc.placement.in_scope(),
        excluded_reason: acc.placement.excluded_reason().map(ToString::to_string),
        variants: acc.variants,
        files: acc.totals.files,
        bytes: acc.totals.bytes,
        categories: category_stats(&acc.categories, acc.totals),
        top_extensions: extension_stats(&acc.extensions, TOP_EXTENSIONS_PER_PLATFORM),
        cjk_files: acc.cjk.files,
    }
}

/// 平均每个变体吃掉多少个文件。
///
/// 一个变体都没有时返回 0 而不是无穷：报告里那一格印 `inf` 只会让人以为程序坏了。
fn per_variant(files: u64, variants: u64) -> f64 {
    if variants == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let raw = files as f64 / variants as f64;
    (raw * 100.0).round() / 100.0
}

fn shaping_summary(
    acc: &ShapingAcc,
    unshaped: &BTreeMap<Category, Counts>,
    scan: i64,
) -> ShapingSummary {
    let mut by_platform: Vec<PlatformShapeStats> = acc
        .by_platform
        .iter()
        .map(|(name, counts)| PlatformShapeStats {
            name: name.clone(),
            variants: counts.variants,
            files: counts.files,
            bytes: counts.bytes,
            files_per_variant: per_variant(counts.files, counts.variants),
        })
        .collect();
    by_platform.sort_by(|a, b| {
        b.variants
            .cmp(&a.variants)
            .then_with(|| b.files.cmp(&a.files))
            .then_with(|| a.name.cmp(&b.name))
    });
    let mut by_rule: Vec<RuleStats> = acc
        .by_rule
        .iter()
        .map(|(rule, variants)| RuleStats {
            rule: rule.clone(),
            variants: *variants,
        })
        .collect();
    by_rule.sort_by(|a, b| {
        b.variants
            .cmp(&a.variants)
            .then_with(|| a.rule.cmp(&b.rule))
    });
    ShapingSummary {
        shaped: acc.shaped(),
        // 成型代号比最后一次遍历小，说明扫过之后没再成型——报告要说出来，
        // 否则读的人会拿着一份旧变体表当新的看。
        stale: acc.shaped_scan.is_some_and(|shaped| shaped < scan),
        manifest_changed: acc.manifest_changed,
        variants: acc.variants,
        files: acc.files,
        bytes: acc.bytes,
        unreadable_files: acc.unreadable_files,
        manual: acc.manual,
        unshaped: category_stats(
            unshaped,
            Counts {
                files: unshaped.values().map(|counts| counts.files).sum(),
                bytes: unshaped.values().map(|counts| counts.bytes).sum(),
            },
        ),
        files_per_variant: per_variant(acc.files, acc.variants),
        by_platform,
        by_rule,
        by_role: Role::all()
            .into_iter()
            .filter_map(|role| {
                let members = acc.by_role.get(&role).copied()?;
                Some(RoleStats { role, members })
            })
            .collect(),
    }
}

fn scope_summary(aggregate: &Aggregate) -> ScopeSummary {
    let mut summary = ScopeSummary {
        platforms: 0,
        in_scope_files: 0,
        in_scope_bytes: 0,
        unmapped_dirs: 0,
        unmapped_files: 0,
        unmapped_bytes: 0,
        unmapped_examples: Vec::new(),
        excluded: Vec::new(),
        root_level_files: 0,
        root_level_bytes: 0,
    };
    let mut unmapped: Vec<(&str, Counts)> = Vec::new();
    // 四态直接照 `Placement` 分派：报告不再自己拿几个布尔字段把它拼回来。
    for (name, acc) in &aggregate.platforms {
        match &acc.placement {
            Placement::Platform(_) => {
                summary.platforms += 1;
                summary.in_scope_files += acc.totals.files;
                summary.in_scope_bytes += acc.totals.bytes;
            }
            Placement::Excluded { reason } => summary.excluded.push(ExcludedDir {
                name: name.clone(),
                reason: reason.clone(),
                files: acc.totals.files,
                bytes: acc.totals.bytes,
            }),
            Placement::Unmapped => {
                summary.unmapped_dirs += 1;
                summary.unmapped_files += acc.totals.files;
                summary.unmapped_bytes += acc.totals.bytes;
                unmapped.push((name, acc.totals));
            }
            Placement::RootLevel => {
                summary.root_level_files += acc.totals.files;
                summary.root_level_bytes += acc.totals.bytes;
            }
        }
    }
    unmapped.sort_by(|a, b| b.1.bytes.cmp(&a.1.bytes).then_with(|| a.0.cmp(b.0)));
    summary.unmapped_examples = unmapped
        .iter()
        .take(TOP_UNMAPPED_DIRS)
        .map(|(name, counts)| {
            format!(
                "{name}（{} 个文件，{}）",
                render::thousands(counts.files),
                render::human_bytes(counts.bytes)
            )
        })
        .collect();
    summary
        .excluded
        .sort_by_key(|dir| std::cmp::Reverse(dir.bytes));
    summary
}

fn conflict_summary(acc: &ConflictAcc) -> ConflictSummary {
    ConflictSummary {
        total: acc.total(),
        by_evidence: ConflictEvidence::all()
            .into_iter()
            .filter_map(|evidence| {
                let count = acc.by_evidence.get(&evidence).copied()?;
                Some((evidence, evidence.label().to_string(), count))
            })
            .collect(),
        examples: acc.examples.clone(),
    }
}

fn category_stats(counts: &BTreeMap<Category, Counts>, totals: Counts) -> Vec<CategoryStats> {
    Category::all()
        .into_iter()
        .map(|category| {
            let value = counts.get(&category).copied().unwrap_or_default();
            CategoryStats {
                category,
                label: category.label().to_string(),
                files: value.files,
                bytes: value.bytes,
                file_share: share(value.files, totals.files),
                byte_share: share(value.bytes, totals.bytes),
            }
        })
        .collect()
}

fn extension_stats(counts: &BTreeMap<String, ExtensionAcc>, limit: usize) -> Vec<ExtensionStats> {
    let mut list: Vec<ExtensionStats> = counts
        .iter()
        .map(|(extension, value)| ExtensionStats {
            extension: extension.clone(),
            category: value.category,
            files: value.counts.files,
            bytes: value.counts.bytes,
        })
        .collect();
    list.sort_by(|a, b| {
        b.bytes
            .cmp(&a.bytes)
            .then_with(|| b.files.cmp(&a.files))
            .then_with(|| a.extension.cmp(&b.extension))
    });
    list.truncate(limit);
    list
}

impl HealthReport {
    /// 从扫描状态整理出报告。
    #[must_use]
    pub fn build(aggregate: &Aggregate, meta: &ReportMeta) -> Self {
        let mut platforms: Vec<PlatformStats> = aggregate
            .platforms
            .iter()
            .map(|(name, acc)| platform_stats(name, acc))
            .collect();
        platforms.sort_by(|a, b| {
            b.bytes
                .cmp(&a.bytes)
                .then_with(|| b.files.cmp(&a.files))
                .then_with(|| a.name.cmp(&b.name))
        });

        Self {
            root_name: meta.root_name.clone(),
            root: meta.root.clone(),
            interrupted: meta.interrupted,
            resumed: meta.resumed,
            jobs: meta.jobs,
            samples_per_class: meta.samples_per_class,
            delta: meta.delta,
            elapsed_ms: aggregate.elapsed_ms,
            totals: Totals {
                files: aggregate.totals.files,
                bytes: aggregate.totals.bytes,
                dirs: aggregate.dirs,
                cjk_files: aggregate.cjk.files,
                cjk_bytes: aggregate.cjk.bytes,
            },
            platforms,
            categories: category_stats(&aggregate.categories, aggregate.totals),
            extensions: extension_stats(&aggregate.extensions, TOP_EXTENSIONS_GLOBAL),
            suspects: suspect_summary(aggregate),
            samples: sample_stats(&aggregate.samples),
            containers: container_summary(&aggregate.containers, meta.penetrated_containers),
            shaping: shaping_summary(&aggregate.shaping, &aggregate.unshaped, meta.scan),
            scope: scope_summary(aggregate),
            conflicts: conflict_summary(&aggregate.conflicts),
            content_files_without_probe: aggregate.content_files_without_probe,
            anomalies: aggregate.anomalies.clone(),
        }
    }

    /// 渲染成给人看的文本报告。
    #[must_use]
    pub fn render_text(&self) -> String {
        render::render(self)
    }
}

/// 全部重复分组，按只留一份可腾出的空间从大到小排。
///
/// 报告只展示前几组，[`DuplicateDetails`] 要的是全部——排序与判据必须是同一份，
/// 否则两处会给出不一样的「最该处理的那几组」。
fn duplicate_groups(aggregate: &Aggregate) -> Vec<DuplicateGroupStats> {
    let mut groups: Vec<DuplicateGroupStats> = aggregate
        .duplicate_index
        .values()
        .filter(|group| group.count > 1)
        .map(|group| DuplicateGroupStats {
            size: group.size,
            count: group.count,
            paths: group.paths.clone(),
        })
        .collect();
    groups.sort_by(|a, b| {
        b.reclaimable_bytes()
            .cmp(&a.reclaimable_bytes())
            .then_with(|| b.count.cmp(&a.count))
            .then_with(|| a.paths.cmp(&b.paths))
    });
    groups
}

fn suspect_summary(aggregate: &Aggregate) -> SuspectSummary {
    let mut groups = duplicate_groups(aggregate);
    let duplicate_groups = u64::try_from(groups.len()).unwrap_or(u64::MAX);
    let duplicate_files: u64 = groups.iter().map(|g| g.count).sum();
    let duplicate_reclaimable_bytes: u64 = groups
        .iter()
        .map(DuplicateGroupStats::reclaimable_bytes)
        .sum();
    groups.truncate(TOP_DUPLICATE_GROUPS);
    for group in &mut groups {
        group.paths.truncate(TOP_DUPLICATE_PATHS_PER_GROUP);
    }

    let by_reason = SuspectReason::all()
        .into_iter()
        .map(|reason| {
            let (files, bytes) = if reason == SuspectReason::DuplicateCopy {
                (duplicate_files, duplicate_reclaimable_bytes)
            } else {
                let counts = aggregate.suspects.get(&reason).copied().unwrap_or_default();
                (counts.files, counts.bytes)
            };
            SuspectStats {
                reason,
                label: reason.label().to_string(),
                files,
                bytes,
                examples: aggregate
                    .suspect_examples
                    .get(&reason)
                    .cloned()
                    .unwrap_or_default(),
            }
        })
        .collect();

    SuspectSummary {
        by_reason,
        duplicate_groups,
        duplicate_files,
        duplicate_reclaimable_bytes,
        top_duplicates: groups,
        index_truncated: aggregate.duplicate_index_truncated,
    }
}

fn container_summary(acc: &ContainerAcc, penetrated_this_scan: bool) -> ContainerSummary {
    let totals = acc.totals();
    let by_kind = acc
        .by_kind
        .iter()
        .map(|(kind, value)| ContainerKindStats {
            kind: *kind,
            label: kind.label().to_string(),
            containers: value.containers,
            penetrated: value.penetrated,
            failed: value.failed,
            unread: value.unread,
            success_rate: share(
                value.penetrated,
                value.containers.saturating_sub(value.unread),
            ),
            solid: value.solid,
            inner_files: value.inner_files,
            inner_bytes: value.inner_bytes,
            inner_without_crc: value.inner_without_crc,
            needs_full_decompress: value.needs_full_decompress,
            blocks: value.blocks,
        })
        .collect();
    let inner_totals = Counts {
        files: totals.inner_files,
        bytes: totals.inner_bytes,
    };
    ContainerSummary {
        penetrated_this_scan,
        by_kind,
        containers: totals.containers,
        penetrated: totals.penetrated,
        failed: totals.failed,
        unread: totals.unread,
        unread_includes_impenetrable: acc
            .by_kind
            .iter()
            .any(|(kind, value)| value.unread > 0 && !kind.is_penetrable()),
        inner_files: totals.inner_files,
        inner_bytes: totals.inner_bytes,
        inner_with_crc: totals.inner_files.saturating_sub(totals.inner_without_crc),
        inner_without_crc: totals.inner_without_crc,
        needs_full_decompress: totals.needs_full_decompress,
        solid: totals.solid,
        inner_categories: category_stats(&acc.inner_categories, inner_totals),
        inner_extensions: extension_stats(&acc.inner_extensions, TOP_INNER_EXTENSIONS),
        lossy_names: acc.lossy_names,
        failures_by_reason: FailureReason::all()
            .into_iter()
            .filter_map(|reason| {
                let containers = acc.failures_by_reason.get(&reason).copied()?;
                Some(FailureReasonStats {
                    reason,
                    label: reason.label().to_string(),
                    containers,
                })
            })
            .collect(),
        failures: acc.failures.clone(),
    }
}

fn sample_stats(samples: &BTreeMap<ProbeClass, SampleAcc>) -> Vec<SampleStats> {
    let mut list: Vec<SampleStats> = samples
        .iter()
        .map(|(class, acc)| SampleStats {
            class: *class,
            label: class.label().to_string(),
            sampled: acc.sampled,
            parsed: acc.parsed,
            mismatched: acc.mismatched,
            too_short: acc.too_short,
            unreadable: acc.unreadable,
            success_rate: share(acc.parsed, acc.sampled),
            failures: acc.failures.clone(),
        })
        .collect();
    // 成功率低的排前面——那正是要去看的
    list.sort_by(|a, b| {
        a.success_rate
            .partial_cmp(&b.success_rate)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.sampled.cmp(&a.sampled))
            .then_with(|| a.label.cmp(&b.label))
    });
    list
}

mod duplicates;
mod render;

pub use duplicates::DuplicateDetails;
pub use render::{
    capacity, heading, human_bytes, human_duration, human_time, pad, thousands, width,
};
