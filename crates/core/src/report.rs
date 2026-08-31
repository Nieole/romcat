//! 库体检报告：把扫描累积的状态整理成可读、可存档的结论。
//!
//! 报告是这张票的交付物本身——所有「先做哪几个平台」的排序都等它出数据。因此它同时
//! 提供两种形态：给人看的文本，与给后面几票（以及界面）吃的 JSON。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::classify::{Category, SuspectReason};
use crate::header::ProbeClass;
use crate::scan::aggregate::{
    Aggregate, Anomalies, Counts, ExtensionAcc, SampleAcc, UNKNOWN_PLATFORM,
};

/// 平台未知时在报告里的显示名。
pub const UNKNOWN_PLATFORM_LABEL: &str = "（平台未知）";

/// 报告里每个平台展示几个扩展名。
const TOP_EXTENSIONS_PER_PLATFORM: usize = 5;
/// 报告里全库展示几个扩展名。
const TOP_EXTENSIONS_GLOBAL: usize = 25;
/// 报告里展示几组重复拷贝。
const TOP_DUPLICATE_GROUPS: usize = 10;

/// 生成报告时需要的、统计之外的信息。
#[derive(Debug, Clone)]
pub struct ReportMeta {
    /// 扫描根。
    pub root: String,
    /// 这次扫描是否被中断。
    pub interrupted: bool,
    /// 这次扫描是否从断点续跑。
    pub resumed: bool,
    /// 并发线程数。
    pub jobs: usize,
    /// 每类文件的抽样配额。
    pub samples_per_class: usize,
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

/// 一个平台目录的统计。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlatformStats {
    /// 平台目录名；平台未知时是 [`UNKNOWN_PLATFORM_LABEL`]。
    pub name: String,
    /// 是否是「平台未知」这一组。
    pub unknown: bool,
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
    /// 路径样例。
    pub examples: Vec<String>,
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
    /// 份数最多的几组。
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

/// 库体检报告。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthReport {
    /// 扫描根。
    pub root: String,
    /// 这次扫描是否被中断。中断的报告仍然可读，只是不完整。
    pub interrupted: bool,
    /// 是否从断点续跑。
    pub resumed: bool,
    /// 并发线程数。
    pub jobs: usize,
    /// 每类文件的抽样配额。
    pub samples_per_class: usize,
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
            .map(|(name, acc)| PlatformStats {
                name: if name == UNKNOWN_PLATFORM {
                    UNKNOWN_PLATFORM_LABEL.to_string()
                } else {
                    name.clone()
                },
                unknown: name == UNKNOWN_PLATFORM,
                files: acc.totals.files,
                bytes: acc.totals.bytes,
                categories: category_stats(&acc.categories, acc.totals),
                top_extensions: extension_stats(&acc.extensions, TOP_EXTENSIONS_PER_PLATFORM),
                cjk_files: acc.cjk.files,
            })
            .collect();
        platforms.sort_by(|a, b| {
            b.bytes
                .cmp(&a.bytes)
                .then_with(|| b.files.cmp(&a.files))
                .then_with(|| a.name.cmp(&b.name))
        });

        Self {
            root: meta.root.clone(),
            interrupted: meta.interrupted,
            resumed: meta.resumed,
            jobs: meta.jobs,
            samples_per_class: meta.samples_per_class,
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

fn suspect_summary(aggregate: &Aggregate) -> SuspectSummary {
    let mut groups: Vec<DuplicateGroupStats> = aggregate
        .duplicate_index
        .values()
        .filter(|group| group.count > 1)
        .map(|group| DuplicateGroupStats {
            size: group.size,
            count: group.count,
            examples: group.examples.clone(),
        })
        .collect();
    groups.sort_by(|a, b| {
        let a_waste = a.size * (a.count - 1);
        let b_waste = b.size * (b.count - 1);
        b_waste
            .cmp(&a_waste)
            .then_with(|| b.count.cmp(&a.count))
            .then_with(|| a.examples.cmp(&b.examples))
    });
    let duplicate_groups = groups.len() as u64;
    let duplicate_files: u64 = groups.iter().map(|g| g.count).sum();
    let duplicate_reclaimable_bytes: u64 = groups.iter().map(|g| g.size * (g.count - 1)).sum();
    groups.truncate(TOP_DUPLICATE_GROUPS);

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

mod render;

pub use render::{human_bytes, thousands};
