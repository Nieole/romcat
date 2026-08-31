//! 重复拷贝明细：一份能直接照着做人工处理的清点结果。
//!
//! 体检报告里只列可腾出空间最多的前几组、每组只列头几条路径——那是给人看一眼规模的。
//! 真要动手，需要的是**每一组、组内每一个文件的完整路径**，还要知道这一组处理掉能换回
//! 多少空间。因此它单独导出成一个文件：不塞进报告 JSON（大多数场景用不上，只会让报告
//! 膨胀），也不靠留着断点（留着会让下次续跑误以为还有活没干完）。
//!
//! **只发现并报告，绝不删除**（ADR-0004）。这里没有、也不会有任何删除能力：判据只是
//! 同名同大小，它认不出改过名的同一份，也会把碰巧同名同大小的不同东西报进来（挂账 D3）。
//! 留哪一份、删不删，由人看着明细决定。
//!
//! 「明细」不是「**清单**」：词表里的清单专指某个**子库**上次导出的完整记录，是同步的
//! 行为边界，与这里无关。

use std::fmt::Write as _;

use super::render::pad;
use super::{DuplicateGroupStats, HealthReport, duplicate_groups, human_bytes, thousands};
use crate::scan::aggregate::Aggregate;

/// 重复拷贝明细。
#[derive(Debug)]
pub struct DuplicateDetails {
    /// 扫描根。
    pub root: String,
    /// 这次扫描是否被中断。中断时明细只覆盖已经扫到的部分。
    pub interrupted: bool,
    /// 这次扫描是否从断点续跑。
    pub resumed: bool,
    /// 全部重复分组，按只留一份可腾出的空间从大到小排。**不截断**。
    pub groups: Vec<DuplicateGroupStats>,
    /// 涉及多少个文件。
    pub files: u64,
    /// 每组各留一份，合计能腾出多少字节。
    pub reclaimable_bytes: u64,
    /// 有几组的路径没记全。
    pub groups_with_missing_paths: u64,
    /// 重复索引是否因超过上限而截断——真截断了，实际重复比这份明细还多。
    pub index_truncated: bool,
}

impl DuplicateDetails {
    /// 从扫描状态整理出完整明细。
    ///
    /// 抬头信息取自**同一次扫描**的报告：明细与报告必须说同一件事，两份数据对不上
    /// 的明细没法拿来动手。
    #[must_use]
    pub fn build(aggregate: &Aggregate, report: &HealthReport) -> Self {
        let groups = duplicate_groups(aggregate);
        let files = groups.iter().map(|group| group.count).sum();
        let reclaimable_bytes = groups
            .iter()
            .map(DuplicateGroupStats::reclaimable_bytes)
            .sum();
        let groups_with_missing_paths = groups
            .iter()
            .filter(|group| group.paths_missing() > 0)
            .count();
        Self {
            root: report.root.clone(),
            interrupted: report.interrupted,
            resumed: report.resumed,
            groups,
            files,
            reclaimable_bytes,
            groups_with_missing_paths: u64::try_from(groups_with_missing_paths).unwrap_or(u64::MAX),
            index_truncated: aggregate.duplicate_index_truncated,
        }
    }

    /// 有几组。
    #[must_use]
    pub fn group_count(&self) -> u64 {
        u64::try_from(self.groups.len()).unwrap_or(u64::MAX)
    }

    /// 渲染成给人看的明细文本。
    #[must_use]
    pub fn render_text(&self) -> String {
        const LABEL: usize = 16;
        let mut out = String::new();
        let _ = writeln!(out, "重复拷贝明细");
        let _ = writeln!(out, "{}", "═".repeat(20));
        let _ = writeln!(out, "{}{}", pad("主库", LABEL), self.root);
        let _ = writeln!(
            out,
            "{}{}{}",
            pad("扫描", LABEL),
            if self.interrupted {
                "已中断（明细只覆盖已扫到的部分）"
            } else {
                "已完成"
            },
            if self.resumed {
                "，由断点续跑"
            } else {
                ""
            }
        );
        let _ = writeln!(
            out,
            "{}同名 + 同大小，不读内容也不算哈希",
            pad("判据", LABEL)
        );
        let _ = writeln!(
            out,
            "{}{} 组，涉及 {} 个文件",
            pad("分组", LABEL),
            thousands(self.group_count()),
            thousands(self.files)
        );
        let _ = writeln!(
            out,
            "{}可腾出 {}",
            pad("每组留一份", LABEL),
            human_bytes(self.reclaimable_bytes)
        );
        let _ = writeln!(out, "{}按可腾出的空间从大到小", pad("排序", LABEL));
        let _ = writeln!(
            out,
            "\n只发现并报告，绝不自动删除（ADR-0004）：留哪一份由你决定，工具不碰主库。"
        );
        let _ = writeln!(out, "同名同大小不等于同一份东西——动手前请自己核一眼。");
        if self.index_truncated {
            let _ = writeln!(
                out,
                "\n⚠ 重复索引在扫描时达到上限，实际重复比这份明细还多。"
            );
        }
        if self.groups_with_missing_paths > 0 {
            let _ = writeln!(
                out,
                "\n⚠ 有 {} 组没记全路径（扫描时每组路径有上限），组内标了「另有 N 份」。",
                thousands(self.groups_with_missing_paths)
            );
        }

        if self.groups.is_empty() {
            let _ = writeln!(out, "\n没有发现重复拷贝。");
            return out;
        }

        let _ = writeln!(out);
        for (index, group) in self.groups.iter().enumerate() {
            let _ = writeln!(
                out,
                "#{}  可腾 {} ｜ 每份 {} ｜ 共 {} 份",
                index + 1,
                human_bytes(group.reclaimable_bytes()),
                human_bytes(group.size),
                group.count
            );
            for path in &group.paths {
                let _ = writeln!(out, "    {path}");
            }
            let missing = group.paths_missing();
            if missing > 0 {
                let _ = writeln!(out, "    …另有 {missing} 份没记下路径");
            }
            let _ = writeln!(out);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::ReportMeta;
    use crate::scan::aggregate::{FileObservation, Limits};

    fn 观察(key: &str, len: u64) -> FileObservation {
        FileObservation::derive("/lib", key, Some(len), false, None)
    }

    fn 报告(aggregate: &Aggregate, interrupted: bool, resumed: bool) -> HealthReport {
        HealthReport::build(
            aggregate,
            &ReportMeta {
                root: "/lib".to_string(),
                interrupted,
                resumed,
                jobs: 1,
                samples_per_class: 0,
                penetrated_containers: true,
                delta: None,
            },
        )
    }

    fn 明细(aggregate: &Aggregate) -> DuplicateDetails {
        DuplicateDetails::build(aggregate, &报告(aggregate, false, false))
    }

    /// `count` 组重复拷贝，第 n 组 n+1 份、每份 n KiB——可腾出的空间彼此不同，
    /// 排序有得可排。
    fn 若干组重复(count: u64, limits: &Limits) -> Aggregate {
        let mut agg = Aggregate::default();
        for n in 1..=count {
            for copy in 0..=n {
                agg.record_file(
                    &观察(&format!("FC/备份{copy}/游戏{n}.zip"), n * 1024),
                    limits,
                );
            }
        }
        agg
    }

    #[test]
    fn 明细列出每一组而不是只列前十() {
        let agg = 若干组重复(24, &Limits::default());
        let details = 明细(&agg);
        assert_eq!(details.group_count(), 24, "24 组一组都不能少");

        let text = details.render_text();
        for n in 1..=24 {
            assert!(
                text.contains(&format!("游戏{n}.zip")),
                "第 {n} 组必须出现在明细里"
            );
        }
    }

    #[test]
    fn 每组给出大小与可腾出的空间且按可腾出的空间从大到小排() {
        let agg = 若干组重复(3, &Limits::default());
        let details = 明细(&agg);

        let 可腾: Vec<u64> = details
            .groups
            .iter()
            .map(DuplicateGroupStats::reclaimable_bytes)
            .collect();
        // 第 n 组是 n+1 份、每份 n KiB，只留一份可腾 n*n KiB
        assert_eq!(可腾, vec![9 * 1024, 4 * 1024, 1024]);
        assert!(可腾.windows(2).all(|w| w[0] >= w[1]), "从大到小");
        assert_eq!(details.reclaimable_bytes, 14 * 1024);
        assert_eq!(details.files, 2 + 3 + 4);

        let text = details.render_text();
        assert!(text.contains("#1  可腾 9.00 KiB ｜ 每份 3.00 KiB ｜ 共 4 份"));
        assert!(text.contains("#3  可腾 1.00 KiB ｜ 每份 1.00 KiB ｜ 共 2 份"));
    }

    #[test]
    fn 组内每个文件的完整路径都在明细里() {
        let limits = Limits {
            max_duplicate_paths_per_group: Limits::FULL_DUPLICATE_PATHS_PER_GROUP,
            ..Limits::default()
        };
        let mut agg = Aggregate::default();
        for copy in 0..25 {
            agg.record_file(&观察(&format!("FC/备份{copy}/魂斗罗.zip"), 4096), &limits);
        }
        let details = 明细(&agg);
        assert_eq!(details.groups_with_missing_paths, 0);

        let text = details.render_text();
        for copy in 0..25 {
            assert!(
                text.contains(&format!("/lib/FC/备份{copy}/魂斗罗.zip")),
                "第 {copy} 份的完整路径必须在明细里"
            );
        }
        assert!(!text.contains("另有"));
    }

    /// 明细要记全，报告却不能跟着膨胀——两处的上限是分开的。
    #[test]
    fn 明细记全路径时报告仍然只留每组前十条() {
        let limits = Limits {
            max_duplicate_paths_per_group: Limits::FULL_DUPLICATE_PATHS_PER_GROUP,
            ..Limits::default()
        };
        let mut agg = Aggregate::default();
        for copy in 0..25 {
            agg.record_file(&观察(&format!("FC/备份{copy}/魂斗罗.zip"), 4096), &limits);
        }
        let report = 报告(&agg, false, false);
        let details = DuplicateDetails::build(&agg, &report);

        assert_eq!(report.suspects.top_duplicates[0].paths.len(), 10);
        assert_eq!(report.suspects.top_duplicates[0].paths_missing(), 15);
        assert_eq!(details.groups[0].paths.len(), 25, "明细里一份不少");
        assert!(
            report.render_text().contains("……另有 15 份"),
            "文本报告要说清自己少列了"
        );
    }

    #[test]
    fn 路径没记全时明细说出来少了几份() {
        let limits = Limits {
            max_duplicate_paths_per_group: 3,
            ..Limits::default()
        };
        let mut agg = Aggregate::default();
        for copy in 0..10 {
            agg.record_file(&观察(&format!("FC/备份{copy}/魂斗罗.zip"), 4096), &limits);
        }
        let details = 明细(&agg);
        assert_eq!(details.groups_with_missing_paths, 1);
        assert_eq!(details.groups[0].paths_missing(), 7);

        let text = details.render_text();
        assert!(text.contains("有 1 组没记全路径"));
        assert!(text.contains("…另有 7 份没记下路径"));
    }

    #[test]
    fn 明细与报告数出来的是同一批重复() {
        let agg = 若干组重复(24, &Limits::default());
        let report = 报告(&agg, false, false);
        let details = DuplicateDetails::build(&agg, &report);

        assert_eq!(report.suspects.duplicate_groups, details.group_count());
        assert_eq!(report.suspects.duplicate_files, details.files);
        assert_eq!(
            report.suspects.duplicate_reclaimable_bytes,
            details.reclaimable_bytes
        );
        // 报告的前十组就是明细的前十组，排序同一份
        let 前十: Vec<(u64, u64)> = details.groups[..10]
            .iter()
            .map(|group| (group.size, group.count))
            .collect();
        let 报告前十: Vec<(u64, u64)> = report
            .suspects
            .top_duplicates
            .iter()
            .map(|group| (group.size, group.count))
            .collect();
        assert_eq!(报告前十, 前十);
    }

    #[test]
    fn 没有重复拷贝时明细也说得清楚() {
        let mut agg = Aggregate::default();
        agg.record_file(&观察("FC/独一份.zip", 4096), &Limits::default());
        let details = 明细(&agg);
        assert_eq!(details.group_count(), 0);
        assert_eq!(details.reclaimable_bytes, 0);
        assert!(details.render_text().contains("没有发现重复拷贝"));
    }

    #[test]
    fn 中断与索引截断都写在明细抬头上() {
        let mut agg = 若干组重复(2, &Limits::default());
        agg.duplicate_index_truncated = true;
        let report = 报告(&agg, true, true);
        let text = DuplicateDetails::build(&agg, &report).render_text();
        assert!(text.contains("已中断"));
        assert!(text.contains("由断点续跑"));
        assert!(text.contains("实际重复比这份明细还多"));
    }

    #[test]
    fn 明细不提供也不暗示删除() {
        let text = 明细(&若干组重复(2, &Limits::default())).render_text();
        assert!(text.contains("只发现并报告，绝不自动删除"));
    }
}
