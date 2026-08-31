//! 把报告渲染成给人看的文本。
//!
//! 列宽按**显示宽度**而不是字符数算——一个汉字占两格，按字符数对齐的表格在中文库上
//! 会歪得没法看。

use std::fmt::Write as _;

use super::{HealthReport, UNKNOWN_PLATFORM_LABEL, share};

/// 把字节数写成人能读的形态。
#[must_use]
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    #[allow(clippy::cast_precision_loss)]
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

/// 给数字加千位分隔符。
#[must_use]
pub fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn char_width(ch: char) -> usize {
    let code = u32::from(ch);
    let wide = matches!(code,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x2_0000..=0x3_FFFD
    );
    if wide { 2 } else { 1 }
}

fn width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

pub(super) fn pad(text: &str, target: usize) -> String {
    let current = width(text);
    if current >= target {
        format!("{text} ")
    } else {
        format!("{text}{}", " ".repeat(target - current))
    }
}

fn truncate(text: &str, max: usize) -> String {
    if width(text) <= max {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = char_width(ch);
        if used + w > max.saturating_sub(1) {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

fn heading(out: &mut String, title: &str) {
    let _ = write!(out, "\n{title}\n{}\n", "─".repeat(width(title) / 2 + 8));
}

fn row(cells: &[(&str, usize)]) -> String {
    let mut line = String::new();
    for (text, target) in cells {
        line.push_str(&pad(text, *target));
    }
    line.trim_end().to_string()
}

#[allow(clippy::too_many_lines)]
pub(super) fn render(report: &HealthReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "库体检报告");
    let _ = writeln!(out, "{}", "═".repeat(20));
    let _ = writeln!(out, "主库            {}", report.root);
    let status = if report.interrupted {
        "已中断（断点已保存，可续跑）"
    } else {
        "已完成"
    };
    let _ = writeln!(
        out,
        "状态            {status}{}",
        if report.resumed {
            "，本次由断点续跑"
        } else {
            ""
        }
    );
    #[allow(clippy::cast_precision_loss)]
    let seconds = report.elapsed_ms as f64 / 1000.0;
    #[allow(clippy::cast_precision_loss)]
    let rate = if seconds > 0.0 {
        report.totals.files as f64 / seconds
    } else {
        0.0
    };
    let _ = writeln!(
        out,
        "耗时            {seconds:.1} 秒（并发 {} 线程，{rate:.0} 文件/秒）",
        report.jobs
    );
    let _ = writeln!(
        out,
        "文件            {} 个，{}",
        thousands(report.totals.files),
        human_bytes(report.totals.bytes)
    );
    let _ = writeln!(out, "目录            {} 个", thousands(report.totals.dirs));
    let _ = writeln!(
        out,
        "文件名含汉字    {} 个（{:.1}%），{} —— 这是文件名层面的粗略代理，不是识别结论",
        thousands(report.totals.cjk_files),
        share(report.totals.cjk_files, report.totals.files),
        human_bytes(report.totals.cjk_bytes)
    );

    heading(&mut out, "按平台目录");
    let _ = writeln!(
        out,
        "{}",
        row(&[
            ("平台目录", 22),
            ("文件数", 12),
            ("容量", 12),
            ("透明容器", 10),
            ("压缩镜像", 10),
            ("裸文件", 10),
            ("含汉字", 8),
        ])
    );
    for platform in &report.platforms {
        let find = |category: crate::classify::Category| {
            platform
                .categories
                .iter()
                .find(|c| c.category == category)
                .map_or(0.0, |c| c.file_share)
        };
        let _ = writeln!(
            out,
            "{}",
            row(&[
                (&truncate(&platform.name, 21), 22),
                (&thousands(platform.files), 12),
                (&human_bytes(platform.bytes), 12),
                (
                    &format!(
                        "{:.1}%",
                        find(crate::classify::Category::TransparentContainer)
                    ),
                    10
                ),
                (
                    &format!("{:.1}%", find(crate::classify::Category::CompressedImage)),
                    10
                ),
                (
                    &format!("{:.1}%", find(crate::classify::Category::BareFile)),
                    10
                ),
                (
                    &format!("{:.1}%", share(platform.cjk_files, platform.files)),
                    8
                ),
            ])
        );
    }
    if report.platforms.iter().any(|p| p.unknown) {
        let _ = writeln!(
            out,
            "注：{UNKNOWN_PLATFORM_LABEL} 是直接躺在库根下的文件，没有平台目录可依据，但照常计入"
        );
    }

    heading(&mut out, "三类构成（识别管线会走哪条路径）");
    for category in &report.categories {
        let _ = writeln!(
            out,
            "{}",
            row(&[
                (&category.label, 16),
                (&format!("{} 个", thousands(category.files)), 16),
                (&format!("{:.1}%", category.file_share), 10),
                (&human_bytes(category.bytes), 14),
                (&format!("{:.1}%", category.byte_share), 10),
            ])
        );
    }

    heading(&mut out, "扩展名构成（按容量前 25）");
    let _ = writeln!(
        out,
        "{}",
        row(&[("扩展名", 16), ("文件数", 12), ("容量", 14), ("归类", 14)])
    );
    for extension in &report.extensions {
        let _ = writeln!(
            out,
            "{}",
            row(&[
                (&truncate(&extension.extension, 15), 16),
                (&thousands(extension.files), 12),
                (&human_bytes(extension.bytes), 14),
                (extension.category.label(), 14),
            ])
        );
    }

    heading(&mut out, "疑似不该入库");
    let _ = writeln!(
        out,
        "重复拷贝        {} 组、涉及 {} 个文件，只留一份可腾出 {}",
        thousands(report.suspects.duplicate_groups),
        thousands(report.suspects.duplicate_files),
        human_bytes(report.suspects.duplicate_reclaimable_bytes)
    );
    let _ = writeln!(
        out,
        "                判据是同名同大小；只发现并报告，绝不自动删除"
    );
    if report.suspects.index_truncated {
        let _ = writeln!(
            out,
            "                ⚠ 重复索引达到上限，报出来的重复少于实际"
        );
    }
    for group in &report.suspects.top_duplicates {
        // 报告只列每组的头几条路径。少列了就说出来——不说的话，这一行看着就像整组只有这几份。
        let missing = group.paths_missing();
        let _ = writeln!(
            out,
            "  ×{} 每份 {}：{}{}",
            group.count,
            human_bytes(group.size),
            group.paths.join("、"),
            if missing > 0 {
                format!("……另有 {missing} 份")
            } else {
                String::new()
            }
        );
    }
    for suspect in &report.suspects.by_reason {
        if suspect.reason == crate::classify::SuspectReason::DuplicateCopy {
            continue;
        }
        let _ = writeln!(
            out,
            "{}{} 个，{}",
            pad(&suspect.label, 16),
            thousands(suspect.files),
            human_bytes(suspect.bytes)
        );
        for example in &suspect.examples {
            let _ = writeln!(out, "  {example}");
        }
    }

    heading(&mut out, "头部抽样解析成功率");
    let _ = writeln!(
        out,
        "每类最多抽 {} 个。成功率低的排在前面——那正是要去看的。",
        report.samples_per_class
    );
    let _ = writeln!(
        out,
        "{}",
        row(&[
            ("类别", 28),
            ("抽样", 8),
            ("成功", 8),
            ("对不上", 8),
            ("太短", 8),
            ("读不到", 8),
            ("成功率", 8),
        ])
    );
    for sample in &report.samples {
        let _ = writeln!(
            out,
            "{}",
            row(&[
                (&truncate(&sample.label, 27), 28),
                (&sample.sampled.to_string(), 8),
                (&sample.parsed.to_string(), 8),
                (&sample.mismatched.to_string(), 8),
                (&sample.too_short.to_string(), 8),
                (&sample.unreadable.to_string(), 8),
                (&format!("{:.1}%", sample.success_rate), 8),
            ])
        );
        for (path, reason) in &sample.failures {
            let _ = writeln!(out, "  {path} — {reason}");
        }
    }
    if report.samples.is_empty() {
        let _ = writeln!(out, "（没有抽到任何样本）");
    }
    if report.content_files_without_probe > 0 {
        let _ = writeln!(
            out,
            "另有 {} 个透明容器/压缩镜像/裸文件的扩展名还没有探针，成功率覆盖不到它们",
            thousands(report.content_files_without_probe)
        );
    }

    heading(&mut out, "异常与限制");
    let anomalies = &report.anomalies;
    let _ = writeln!(out, "读取失败        {} 处", thousands(anomalies.errors));
    for example in &anomalies.error_examples {
        let _ = writeln!(out, "  {example}");
    }
    let _ = writeln!(
        out,
        "超长路径        {} 条超过 260 字符（Windows 上不加 \\\\?\\ 前缀就会失败）",
        thousands(anomalies.over_max_path)
    );
    for example in &anomalies.over_max_path_examples {
        let _ = writeln!(out, "  {}", truncate(example, 100));
    }
    let _ = writeln!(
        out,
        "符号链接        {} 个（不跟随）",
        thousands(anomalies.symlinks)
    );
    let _ = writeln!(
        out,
        "空文件          {} 个",
        thousands(anomalies.zero_length)
    );
    let _ = writeln!(
        out,
        "非 UTF-8 路径   {} 条",
        thousands(anomalies.non_utf8_paths)
    );
    let _ = writeln!(
        out,
        "疑似分卷        {} 个分卷（多个分卷合起来才是一个透明容器，票 04 才会聚合）",
        thousands(anomalies.split_volume_parts)
    );
    let _ = writeln!(
        out,
        "跳过的系统目录  {} 个（回收站、缩略图缓存这类，里面没有库的内容）",
        thousands(anomalies.skipped_system_dirs)
    );
    for example in &anomalies.skipped_system_dir_examples {
        let _ = writeln!(out, "  {example}");
    }
    if anomalies.other_entries > 0 {
        let _ = writeln!(
            out,
            "其他项          {} 个（既不是文件也不是目录）",
            thousands(anomalies.other_entries)
        );
    }
    if report.interrupted {
        let _ = writeln!(
            out,
            "\n⚠ 这次扫描被中断，以上是到中断为止的结论。用同一个断点续跑可以接着扫。"
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 字节数写成人能读的形态() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(1023), "1023 B");
        assert_eq!(human_bytes(1024), "1.00 KiB");
        assert_eq!(human_bytes(10 * 1024_u64.pow(4)), "10.00 TiB");
    }

    #[test]
    fn 数字带千位分隔符() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }

    #[test]
    fn 汉字按两格宽度对齐() {
        assert_eq!(width("平台"), 4);
        assert_eq!(width("FC"), 2);
        assert_eq!(pad("平台", 8), "平台    ");
        assert_eq!(pad("FC", 8), "FC      ");
    }
}
