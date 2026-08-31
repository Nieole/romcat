//! 库体检报告的统计状态。
//!
//! 它**不是**持久状态——事实来源是中立库（ADR-0001）。这份统计每次都由
//! [`Catalog::aggregate`](crate::catalog::Catalog::aggregate) 从中立库里的记录现折出来，
//! 因此「边扫边出的报告」与「盘不在位时从中立库出的报告」必然是同一份数字。
//!
//! 它必须是有界的——例子列表、重复索引都带上限，10T 库不会把它撑爆。

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::classify::{self, Category, Classification, SuspectReason, classify};
use crate::header::{self, ProbeClass, ProbeOutcome};
use crate::path;

/// 「平台未知」在按平台分组时用的键。
pub const UNKNOWN_PLATFORM: &str = "";

/// 没有扩展名的文件在按扩展名分组时用的键。
pub const NO_EXTENSION: &str = "（无扩展名）";

/// 一组文件数与字节数。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionAcc {
    /// 文件数与字节数。
    pub counts: Counts,
    /// 这个扩展名属于哪一类。
    pub category: Category,
}

/// 一个平台目录下的统计。
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformAcc {
    /// 该平台目录的合计。
    pub totals: Counts,
    /// 按三类主线分组。
    pub categories: BTreeMap<Category, Counts>,
    /// 按扩展名分组。
    pub extensions: BTreeMap<String, ExtensionAcc>,
    /// 文件名含汉字的部分。
    pub cjk: Counts,
}

/// 一组重复拷贝：同名同大小的多份。
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// 所属平台目录；`None` 表示平台未知。
    pub platform: Option<String>,
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
    /// `root` 是主库根的展示形态，`key` 是那条记录的键（相对根、NFC）。
    #[must_use]
    pub fn derive(
        root: &str,
        key: &str,
        len: Option<u64>,
        non_utf8: bool,
        sample: Option<(ProbeClass, SampleResult)>,
    ) -> Self {
        let display_path = path::display_key(root, key);
        // 归类只看文件名，因此这里传的是文件名而不是整条键——键里的 `/`
        // 交给 `Path` 拆会在 Windows 与 Unix 上给出不同答案。
        let name = Path::new(path::file_name_of_key(key));
        Self {
            over_max_path: path::exceeds_max_path(&display_path),
            display_path,
            name_lower: path::file_name_lower(name),
            platform: path::platform_of_key(key).map(ToString::to_string),
            extension: path::extension_lower(name),
            len,
            classification: classify(name),
            cjk: classify::has_cjk(name),
            non_utf8,
            has_probe: header::probe_class_for(name).is_some(),
            sample,
        }
    }
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
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Aggregate {
    /// 全库合计。
    pub totals: Counts,
    /// 已扫目录数。
    pub dirs: u64,
    /// 按平台目录分组，键为目录名，[`UNKNOWN_PLATFORM`] 表示平台未知。
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
        let platform_key = observation
            .platform
            .clone()
            .unwrap_or_else(|| UNKNOWN_PLATFORM.to_string());
        let platform = self.platforms.entry(platform_key).or_default();
        platform.totals.add(len);
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

        self.record_duplicate_candidate(observation, limits);
        self.record_sample(observation, limits);
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

    /// 记下一个已扫完的目录。
    pub fn record_dir(&mut self) {
        self.dirs += 1;
    }

    /// 记下一个整棵跳过的系统目录。
    pub fn record_skipped_system_dir(&mut self, path: &Path, limits: &Limits) {
        self.anomalies.skipped_system_dirs += 1;
        push_capped(
            &mut self.anomalies.skipped_system_dir_examples,
            path::display(path),
            limits.max_examples,
        );
    }

    /// 记下一个符号链接（不跟随）。
    pub fn record_symlink(&mut self) {
        self.anomalies.symlinks += 1;
    }

    /// 记下一个既不是文件也不是目录也不是链接的项。
    pub fn record_other_entry(&mut self) {
        self.anomalies.other_entries += 1;
    }

    /// 记下一次失败。失败不中断扫描——读不到的东西也是体检结论的一部分。
    pub fn record_error(&mut self, path: &Path, error: &str, limits: &Limits) {
        self.anomalies.errors += 1;
        push_capped(
            &mut self.anomalies.error_examples,
            format!("{} — {error}", path::display(path)),
            limits.max_examples,
        );
    }

    /// 已经抽过样的数量，用于续跑时恢复抽样配额。
    #[must_use]
    pub fn sampled_count(&self, class: ProbeClass) -> usize {
        self.samples
            .get(&class)
            .map_or(0, |acc| usize::try_from(acc.sampled).unwrap_or(usize::MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 观察(key: &str, len: u64) -> FileObservation {
        FileObservation::derive("/lib", key, Some(len), false, None)
    }

    fn 读不到元数据的观察(key: &str) -> FileObservation {
        FileObservation::derive("/lib", key, None, false, None)
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
        let observation =
            FileObservation::derive("/lib", "PS1/某游戏/disc.cue", Some(64), false, None);
        assert_eq!(observation.display_path, "/lib/PS1/某游戏/disc.cue");
        assert_eq!(observation.platform.as_deref(), Some("PS1"));
        assert_eq!(observation.extension.as_deref(), Some("cue"));
        assert_eq!(observation.name_lower, "disc.cue");
        assert_eq!(observation.classification.category, Category::BareFile);
        assert!(!observation.cjk, "汉字在目录名里，不算文件名含汉字");
    }
}
