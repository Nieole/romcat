//! 把报告渲染成给人看的文本。
//!
//! 列宽按**显示宽度**而不是字符数算——一个汉字占两格，按字符数对齐的表格在中文库上
//! 会歪得没法看。

use std::fmt::Write as _;

use super::{HealthReport, UNKNOWN_PLATFORM_LABEL, share};

/// 一个变体占多大——**带上「这是个下界」那件事**。
///
/// 元数据读不到的成员按 0 计入字节合计（ADR-0021），于是有不可读成员时这个数**少算了**。
/// 画成确数等于把「我不知道」显示成「我知道，是这么多」，而**不可读**是如实记录的第三种
/// 状态，不是零。
///
/// 这条规矩在核心库里而不在界面里：它是「这个数怎么读」的判断，不是「怎么画」的判断。
#[must_use]
pub fn capacity(bytes: u64, unreadable_files: u64) -> String {
    let size = human_bytes(bytes);
    if unreadable_files == 0 {
        size
    } else {
        format!("≥ {size}")
    }
}

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

/// 一段时长排成人看得懂的样子。
///
/// **只给粗估用**，因此刻意粗：一小时以上不报秒、一分钟以上不报小数。一个看着精确的
/// 错数比一个明显粗的数更糟——「3 小时 12 分 07 秒」会让人以为工具算过，而它只是把
/// 字节数乘了一个常量（`capability::estimate_secs`）。
#[must_use]
pub fn human_duration(ms: u64) -> String {
    let secs = ms / 1000;
    if secs >= 3600 {
        format!("{} 小时 {} 分", secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        format!("{} 分 {} 秒", secs / 60, secs % 60)
    } else if ms >= 1000 {
        format!("{secs} 秒")
    } else {
        format!("{ms} 毫秒")
    }
}

/// 一个 UNIX 纪元起的秒排成 `YYYY-MM-DD HH:MM`（UTC）。
///
/// 公历换算走 [`capability::from_day_number`](crate::capability)——**公式只有一处**。
/// 抄两遍的两份公式迟早会在闰年上分家，而它们算的本来就是同一件事。
///
/// 用 UTC 而不是本地时区：这个数进报告、也进界面，两处得是同一个数，而本地时区
/// 会让同一份中立库在两台机器上印出不同的时刻。
#[must_use]
pub fn human_time(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    format!(
        "{} {:02}:{:02}",
        crate::capability::from_day_number(days),
        rest / 3_600,
        (rest % 3_600) / 60,
    )
}

/// 给数字加千位分隔符。
#[must_use]
pub fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
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

/// `text` 占多少个显示格。
#[must_use]
pub fn width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

/// 把 `text` 垫到 `target` 个**显示格**宽。一个汉字占两格。
///
/// 报告的表格全靠它对齐。DAT 仓库那份报告（`dat::report`）也用它——
/// 两份报告并排放在同一个终端里，列宽算法不该有两套。
#[must_use]
pub fn pad(text: &str, target: usize) -> String {
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

/// 报告里的一节标题：空一行、写标题、再画一条与它等宽的横线。
///
/// 四份报告（体检、命中率、刮削、标题）逐字节相同地写过四遍，收在这里一份。
/// 它与 `pad` / `thousands` / `width` 是同一档东西：**报告长什么样是一件事，
/// 报告说什么是另一件事**。
pub fn heading(out: &mut String, title: &str) {
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
    // **主库是一组根**：这一行说的是「这一趟扫的那一个」，不是整个主库。
    if report.root_name.is_empty() {
        let _ = writeln!(out, "主库            {}", report.root);
    } else {
        let _ = writeln!(
            out,
            "扫的根          「{}」{}",
            report.root_name, report.root
        );
    }
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
    let unreadable = report.anomalies.unreadable;
    let _ = writeln!(
        out,
        "文件            {} 个，{}{}",
        thousands(report.totals.files),
        human_bytes(report.totals.bytes),
        if unreadable > 0 {
            format!(
                "（其中 {} 个元数据读不到，大小未知、未计入容量）",
                thousands(unreadable)
            )
        } else {
            String::new()
        }
    );
    let _ = writeln!(out, "目录            {} 个", thousands(report.totals.dirs));
    let _ = writeln!(
        out,
        "文件名含汉字    {} 个（{:.1}%），{} —— 这是文件名层面的粗略代理，不是识别结论",
        thousands(report.totals.cjk_files),
        share(report.totals.cjk_files, report.totals.files),
        human_bytes(report.totals.cjk_bytes)
    );

    let anomalies = &report.anomalies;
    if let Some(delta) = report.delta {
        heading(&mut out, "这次扫描的增量（对比中立库上一次的状态）");
        let _ = writeln!(
            out,
            "{}",
            row(&[
                ("未变（跳过）", 18),
                ("新增", 12),
                ("内容变化", 12),
                ("已删除", 12),
                ("这次读不到", 14),
            ])
        );
        let _ = writeln!(
            out,
            "{}",
            row(&[
                (&thousands(delta.unchanged), 18),
                (&thousands(delta.added), 12),
                (&thousands(delta.changed), 12),
                (&thousands(delta.removed), 12),
                (&thousands(delta.unreadable), 14),
            ])
        );
        let _ = writeln!(
            out,
            "判据是 (路径, 大小, 修改时间)。「这次读不到」既不算已变也不算已删——它在 Windows 上是正常文件。"
        );
        if delta.unreadable > 0 && delta.unreadable != anomalies.unreadable {
            let _ = writeln!(
                out,
                "注：这次读不到 {} 个，而下面「元数据从没读到过」是 {} 个——差额是曾经在别的系统上读到过、大小已经记在中立库里的那些。",
                thousands(delta.unreadable),
                thousands(anomalies.unreadable)
            );
        }
        if report.interrupted {
            let _ = writeln!(
                out,
                "⚠ 这次扫描被中断，没有走完整个库，因此「已删除」一栏没有判——中立库里的记录一条都没删。"
            );
        }
        if report.resumed {
            let _ = writeln!(
                out,
                "注：这是续跑，以上只涵盖这一趟；中断之前扫到的那部分已经在中立库里了。"
            );
        }
    }

    heading(&mut out, "按平台");
    let _ = writeln!(
        out,
        "{}",
        row(&[
            ("平台", 22),
            ("变体", 10),
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
        // 范围之外的东西根本不成型，那一格印 0 会读成「一个变体都没成出来」，
        // 而实情是「压根没去成型」。空着更诚实（ADR-0011 修订段）。
        let variants = if platform.in_scope {
            thousands(platform.variants)
        } else {
            "—".to_string()
        };
        // 这一行现在按**平台**分组而不是按目录，因此目录名与平台名不同时要说出来——
        // 不然读的人对不上「盘上那个 `ps` 目录去哪儿了」。
        let name = match platform.dirs.as_slice() {
            [dir] if !dir.eq_ignore_ascii_case(&platform.name) => {
                format!("{}（{dir}）", platform.name)
            }
            dirs if dirs.len() > 1 => format!("{}（{}）", platform.name, dirs.join("、")),
            _ => platform.name.clone(),
        };
        let _ = writeln!(
            out,
            "{}",
            row(&[
                (&truncate(&name, 21), 22),
                (&variants, 10),
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
    if report.platforms.iter().any(|p| !p.in_scope && !p.unknown) {
        let _ = writeln!(
            out,
            "注：「变体」一栏是 — 的那几行不在识别范围内（见下面「范围边界」），库体检照样统计它们"
        );
    }

    render_shaping(&mut out, report);
    render_scope(&mut out, report);
    render_conflicts(&mut out, report);

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

    let containers = &report.containers;
    heading(&mut out, "透明容器：内部构成");
    if containers.containers == 0 {
        let _ = writeln!(
            out,
            "{}",
            if containers.penetrated_this_scan {
                "库里没有 zip、7z、rar 或 zst。"
            } else {
                "这次扫描没有穿透容器（--no-containers），中立库里也没有上次的结论。"
            }
        );
    } else {
        if !containers.penetrated_this_scan {
            let _ = writeln!(
                out,
                "⚠ 这次扫描没有穿透容器（--no-containers），以下是中立库里上次穿透的结论。"
            );
        }
        let _ = writeln!(
            out,
            "容器            {} 个，读出内部构成 {} 个，读不出 {} 个",
            thousands(containers.containers),
            thousands(containers.penetrated),
            thousands(containers.failed)
        );
        if containers.unread > 0 {
            // **还没读过不是穿不透。** 两句话指向完全不同的下一步：一个是再扫一趟，
            // 一个是这文件本身有问题。
            //
            // 而「为什么没读」也有两种，说错了同样害人：zip / 7z / rar 穿得透，没读只可能是
            // `--no-containers`；zst 穿不透，没读是因为读它要把整条流解一遍。
            let _ = writeln!(out, "还没读过        {} 个", thousands(containers.unread));
            if containers.unread_includes_impenetrable {
                let _ = writeln!(
                    out,
                    "                其中的 zst 穿不透——没有零解压那条路，列全清单要把整条流解一遍\n\
                     \x20               `romcat scan --zst` 读一次，结论落进中立库，往后的扫描原样沿用"
                );
            }
        }
        let _ = writeln!(
            out,
            "内部文件        {} 个，未压缩 {}",
            thousands(containers.inner_files),
            human_bytes(containers.inner_bytes)
        );
        let _ = writeln!(
            out,
            "带 CRC-32      {} 个（{:.1}%）—— 零解压就能拿去撞 DAT 的那一批",
            thousands(containers.inner_with_crc),
            share(containers.inner_with_crc, containers.inner_files)
        );
        if containers.needs_full_decompress > 0 {
            // **这一档不是「还没读过」**：它穿透了，名字与大小都在手上，只是第一命中层
            // 用不上——只写 BLAKE2sp 的 RAR5（DAT 一条 BLAKE2 都不记），或者带密码、
            // 校验和被密钥搅过的（`unrar lt` 印成 `CRC32 MAC`）。与 zst 同一档。
            let _ = writeln!(
                out,
                "要完整解压      {} 个容器有内容却一条 CRC-32 都没有——名字与大小读得出来，\
                 第一命中层用不上",
                thousands(containers.needs_full_decompress)
            );
        }
        if containers.inner_without_crc > 0 {
            let _ = writeln!(
                out,
                "没有 CRC-32    {} 个（容器自己没记，只能解压后再算）",
                thousands(containers.inner_without_crc)
            );
        }
        let _ = writeln!(
            out,
            "solid 容器      {} 个（块里装了不止一个文件，必须按块调度，否则解压量成倍放大）",
            thousands(containers.solid)
        );
        if containers.lossy_names > 0 {
            let _ = writeln!(
                out,
                "名字解不出来    {} 条内部条目连编码都探不出来（GBK / Big5 / Shift_JIS 都不是），\
                 只能有损转换。不影响命中（判据是 CRC-32 加大小），但**文件名那一层不撞它们**",
                thousands(containers.lossy_names)
            );
        }

        let _ = writeln!(
            out,
            "\n{}",
            row(&[
                ("格式", 10),
                ("容器", 10),
                ("读出", 10),
                ("读不出", 10),
                ("还没读", 10),
                ("成功率", 10),
                ("solid", 10),
                ("内部文件", 12),
                ("未压缩", 12),
            ])
        );
        for kind in &containers.by_kind {
            let _ = writeln!(
                out,
                "{}",
                row(&[
                    (&kind.label, 10),
                    (&thousands(kind.containers), 10),
                    (&thousands(kind.penetrated), 10),
                    (&thousands(kind.failed), 10),
                    (&thousands(kind.unread), 10),
                    (&format!("{:.1}%", kind.success_rate), 10),
                    (&thousands(kind.solid), 10),
                    (&thousands(kind.inner_files), 12),
                    (&human_bytes(kind.inner_bytes), 12),
                ])
            );
        }

        let _ = writeln!(out, "\n内部构成（按三类主线）");
        for category in &containers.inner_categories {
            if category.files == 0 {
                continue;
            }
            let _ = writeln!(
                out,
                "{}{} 个（{:.1}%），{}",
                pad(&category.label, 16),
                thousands(category.files),
                category.file_share,
                human_bytes(category.bytes)
            );
        }

        let _ = writeln!(out, "\n内部构成（容量最大的扩展名）");
        for extension in &containers.inner_extensions {
            let _ = writeln!(
                out,
                "{}{}{} 个，{}",
                pad(&extension.extension, 14),
                pad(extension.category.label(), 16),
                thousands(extension.files),
                human_bytes(extension.bytes)
            );
        }

        if !containers.failures_by_reason.is_empty() {
            let _ = writeln!(out, "\n穿不透的按原因分");
            for stats in &containers.failures_by_reason {
                let _ = writeln!(
                    out,
                    "{}{} 个",
                    pad(&stats.label, 18),
                    thousands(stats.containers)
                );
            }
        }
        for (path, reason) in &containers.failures {
            let _ = writeln!(out, "  穿不透：{path} — {reason}");
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
    let _ = writeln!(
        out,
        "目录列不开      {} 处（下面的记录原样留着，不算已删除）",
        thousands(anomalies.errors)
    );
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
        "空文件          {} 个（真的 0 字节）",
        thousands(anomalies.zero_length)
    );
    let _ = writeln!(
        out,
        "元数据从没读到  {} 个（名字列得出、属性读不到；不是空文件，也不是不存在）",
        thousands(anomalies.unreadable)
    );
    let _ = writeln!(
        out,
        "非 UTF-8 路径   {} 条",
        thousands(anomalies.non_utf8_paths)
    );
    let _ = writeln!(
        out,
        "疑似分卷        {} 段（不含入口卷——入口卷自己就是那个容器；\
         聚成一个变体在**成型**那一步，`romcat shape` 的「分卷压缩」那一行）",
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

fn render_shaping(out: &mut String, report: &HealthReport) {
    let shaping = &report.shaping;
    heading(out, "成型：从文件到变体");
    if !shaping.shaped {
        let _ = writeln!(
            out,
            "还没成型过。跑一次 `romcat scan` 或 `romcat shape`，库里才会有变体。"
        );
        return;
    }
    if shaping.stale {
        let _ = writeln!(
            out,
            "⚠ 这份变体表比中立库里最后一次遍历旧。扫过之后没再成型，下面的数字对不上现在的库。"
        );
    }
    if shaping.manifest_changed {
        let _ = writeln!(
            out,
            "⚠ 这份变体表是用**另一份平台清单**成的型。上面「按平台」那张表按眼下这份清单分组，\n\
             \x20 变体数却按成型时那份算——两边可能对不上。跑一次 `romcat shape` 对齐。"
        );
    }
    let _ = writeln!(
        out,
        "变体            {} 个，吃掉 {} 个文件、{}",
        thousands(shaping.variants),
        thousands(shaping.files),
        human_bytes(shaping.bytes)
    );
    let _ = writeln!(
        out,
        "每变体文件数    平均 {:.2} 个（范围之内共 {} 个文件）",
        shaping.files_per_variant,
        thousands(report.scope.in_scope_files)
    );
    if shaping.unreadable_files > 0 {
        let _ = writeln!(
            out,
            "                其中 {} 个成员元数据读不到，**容量是个下界**（ADR-0021）",
            thousands(shaping.unreadable_files)
        );
    }
    let not_in_variants: u64 = shaping.unshaped.iter().map(|c| c.files).sum();
    let _ = writeln!(
        out,
        "没进变体        {} 个（范围之内），按归类分：",
        thousands(not_in_variants)
    );
    for category in &shaping.unshaped {
        if category.files == 0 {
            continue;
        }
        let 注 = if category.category == crate::classify::Category::Unclassified {
            " ← 既不是媒体也不是文档垃圾，却没成型：**这一栏大就是成型的缺口**"
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "  {}{} 个，{}{注}",
            pad(&category.label, 16),
            thousands(category.files),
            human_bytes(category.bytes)
        );
    }
    if shaping.manual > 0 {
        let _ = writeln!(
            out,
            "人工纠正        {} 个变体（`romcat shape --merge`，重新成型不会冲掉）",
            thousands(shaping.manual)
        );
    }

    if !shaping.by_role.is_empty() {
        let _ = writeln!(out, "\n成员身份");
        for role in &shaping.by_role {
            let _ = writeln!(
                out,
                "{}{} 个",
                pad(role.role.code(), 16),
                thousands(role.members)
            );
        }
    }

    if !shaping.by_rule.is_empty() {
        let _ = writeln!(out, "\n哪条成型规则成的型");
        for rule in &shaping.by_rule {
            let _ = writeln!(
                out,
                "{}{} 个变体",
                pad(&truncate(&rule.rule, 19), 20),
                thousands(rule.variants)
            );
        }
    }

    if !shaping.by_platform.is_empty() {
        let _ = writeln!(
            out,
            "\n{}",
            row(&[
                ("平台", 16),
                ("变体", 10),
                ("文件", 12),
                ("容量", 12),
                ("文件/变体", 12)
            ])
        );
        for platform in &shaping.by_platform {
            let _ = writeln!(
                out,
                "{}",
                row(&[
                    (&truncate(&platform.name, 15), 16),
                    (&thousands(platform.variants), 10),
                    (&thousands(platform.files), 12),
                    (&human_bytes(platform.bytes), 12),
                    (&format!("{:.2}", platform.files_per_variant), 12),
                ])
            );
        }
    }
}

fn render_scope(out: &mut String, report: &HealthReport) {
    let scope = &report.scope;
    heading(out, "范围边界（识别管线管哪些）");
    let _ = writeln!(
        out,
        "范围之内        {} 个平台，{} 个文件，{}",
        thousands(scope.platforms),
        thousands(scope.in_scope_files),
        human_bytes(scope.in_scope_bytes)
    );
    let _ = writeln!(
        out,
        "还没映射        {} 个顶层目录，{} 个文件，{} —— 工具不认得这些目录名，整体不进识别管线；\n\
         \x20               库体检照样统计，那正是手动整理的依据（ADR-0011）",
        thousands(scope.unmapped_dirs),
        thousands(scope.unmapped_files),
        human_bytes(scope.unmapped_bytes)
    );
    for example in &scope.unmapped_examples {
        let _ = writeln!(out, "  {example}");
    }
    if !scope.excluded.is_empty() {
        let _ = writeln!(
            out,
            "明确排除        看过之后决定不做的，与「还没映射」不是一回事"
        );
        for dir in &scope.excluded {
            let _ = writeln!(
                out,
                "  {}（{} 个文件，{}）—— {}",
                dir.name,
                thousands(dir.files),
                human_bytes(dir.bytes),
                dir.reason
            );
        }
    }
    let _ = writeln!(
        out,
        "库根下的散文件  {} 个，{} —— 连顶层目录都没有，平台无从谈起",
        thousands(scope.root_level_files),
        human_bytes(scope.root_level_bytes)
    );
}

fn render_conflicts(out: &mut String, report: &HealthReport) {
    let conflicts = &report.conflicts;
    heading(out, "目录说的与文件说的对不上");
    if conflicts.total == 0 {
        let _ = writeln!(
            out,
            "没有。（判据是「只可能属于某一个平台」的扩展名，跨平台的 iso/bin/zip 不参与。）"
        );
        return;
    }
    let _ = writeln!(
        out,
        "共 {} 条。目录是强先验而非权威——下错、放错、一个容器里混装几个平台，都会这样（ADR-0011）。",
        thousands(conflicts.total)
    );
    for (_, label, count) in &conflicts.by_evidence {
        let _ = writeln!(out, "{}{} 条", pad(label, 16), thousands(*count));
    }
    for conflict in &conflicts.examples {
        let _ = writeln!(
            out,
            "  {} — 目录说 {}，文件说 {}（{}）",
            truncate(&conflict.path, 90),
            conflict.declared,
            conflict.implied,
            conflict.evidence.label()
        );
    }
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
