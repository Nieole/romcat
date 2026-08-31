//! 扫描过程中累积的统计状态。
//!
//! 它同时是**断点内容**：中断时把它连同待扫目录一起存盘，续跑时读回来接着加。
//! 因此它必须是有界的——例子列表、重复索引都带上限，10T 库不会把它撑爆。

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::classify::{Category, Classification, SuspectReason};
use crate::header::{ProbeClass, ProbeOutcome};
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
    /// 若干条路径样例。
    pub examples: Vec<String>,
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
    /// 空文件数。
    pub zero_length: u64,
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
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_examples: 10,
            max_duplicate_keys: 2_000_000,
        }
    }
}

/// 一个文件的观察结果。由工作线程算出，交给协调线程并入 [`Aggregate`]。
#[derive(Debug, Clone)]
pub struct FileObservation {
    /// 展示用路径。
    pub display_path: String,
    /// 文件名转小写，用于重复检测。
    pub name_lower: String,
    /// 所属平台目录；`None` 表示平台未知。
    pub platform: Option<String>,
    /// 小写扩展名。
    pub extension: Option<String>,
    /// 字节数。
    pub len: u64,
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

/// 一次头部抽样的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
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
        let len = *len;

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

        if len == 0 {
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
        if !is_content || observation.len == 0 {
            return;
        }
        let key = format!("{}|{}", observation.len, observation.name_lower);
        match self.duplicate_index.get_mut(&key) {
            Some(group) => {
                group.count += 1;
                push_capped(
                    &mut group.examples,
                    observation.display_path.clone(),
                    limits.max_examples,
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
                        size: observation.len,
                        count: 1,
                        examples: vec![observation.display_path.clone()],
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
    use crate::classify::classify;

    fn 观察(path: &str, len: u64) -> FileObservation {
        let p = Path::new(path);
        FileObservation {
            display_path: path.to_string(),
            name_lower: crate::path::file_name_lower(p),
            platform: crate::path::platform_dir(Path::new("/lib"), p)
                .map(|n| n.to_string_lossy().into_owned()),
            extension: crate::path::extension_lower(p),
            len,
            classification: classify(p),
            cjk: crate::classify::has_cjk(p),
            non_utf8: false,
            over_max_path: false,
            has_probe: crate::header::probe_class_for(p).is_some(),
            sample: None,
        }
    }

    #[test]
    fn 同名同大小的多份被归成一组重复拷贝() {
        let mut agg = Aggregate::default();
        let limits = Limits::default();
        agg.record_file(&观察("/lib/FC/马里奥.zip", 1024), &limits);
        agg.record_file(&观察("/lib/FC/备份/马里奥.zip", 1024), &limits);
        agg.record_file(&观察("/lib/MD/马里奥.zip", 2048), &limits);

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
        agg.record_file(&观察("/lib/FC/media/cover.png", 100), &limits);
        agg.record_file(&观察("/lib/MD/media/cover.png", 100), &limits);
        assert!(agg.duplicate_index.is_empty());
    }

    #[test]
    fn 平台未知的文件照常计入() {
        let mut agg = Aggregate::default();
        agg.record_file(&观察("/lib/散落的游戏.gba", 512), &Limits::default());
        assert_eq!(agg.totals.files, 1);
        assert_eq!(agg.platforms[UNKNOWN_PLATFORM].totals.files, 1);
    }

    #[test]
    fn 例子列表有上限() {
        let mut agg = Aggregate::default();
        let limits = Limits {
            max_examples: 2,
            ..Limits::default()
        };
        for i in 0..10 {
            agg.record_file(&观察(&format!("/lib/FC/说明{i}.txt"), 10), &limits);
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
        agg.record_file(&观察("/lib/FC/a.zip", 1), &limits);
        agg.record_file(&观察("/lib/FC/b.zip", 2), &limits);
        assert_eq!(agg.duplicate_index.len(), 1);
        assert!(agg.duplicate_index_truncated);
    }

    #[test]
    fn 状态可以序列化成_json_再读回来() {
        let mut agg = Aggregate::default();
        let limits = Limits::default();
        agg.record_file(&观察("/lib/FC/马里奥.zip", 1024), &limits);
        agg.record_file(&观察("/lib/PS1/游戏.chd", 4096), &limits);
        let text = serde_json::to_string(&agg).expect("能序列化");
        let back: Aggregate = serde_json::from_str(&text).expect("能反序列化");
        assert_eq!(agg, back);
    }
}
