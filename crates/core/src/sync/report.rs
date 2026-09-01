//! 把[操作计划](super::Plan)排一遍版。
//!
//! ## 这里**没有**一个「差量预览」类型
//!
//! 别的几层都是「从中立库折出一份报告结构」（`SelectionReport`、`HealthReport`……），
//! 这一层刻意不是。ADR-0016 要的预览就是 [`plan`](super::plan) 的返回值本身，
//! 另立一个报告类型等于把同一份账算两遍——而两遍算法迟早会漂开，那时用户看到的
//! 预览与同步真正会做的事就对不上了，正是这条硬要求要防的事。
//!
//! 于是这个文件里只有 [`Plan::render_text`]：一个 `&Plan -> String` 的排版函数。
//! `--json` 那一份直接就是 `Plan` 自己。

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::report::{heading, human_bytes, pad, thousands};

use super::{Act, FileKind, Plan, Step, SurpriseKind, Tally};

/// 每一类操作在报告里举几个例子。
const EXAMPLES: usize = 10;

/// 意外变化列几条。
const SURPRISE_EXAMPLES: usize = 20;

impl Plan {
    /// 打给人看的**差量预览**。
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "子库 {} · 差量预览", self.sublibrary);
        let _ = writeln!(out, "{}", "═".repeat(24));
        let _ = writeln!(out, "目标            {}", self.target);
        let _ = writeln!(out, "前端格式        {}", self.format);
        let _ = writeln!(
            out,
            "容量上限        {}",
            self.capacity
                .map_or_else(|| "不设限".to_string(), human_bytes)
        );

        heading(&mut out, "差量");
        line(&mut out, "新增", self.adds, None);
        line(&mut out, "更新", self.updates, Some(self.updates_before));
        line(&mut out, "删除", self.deletes, None);
        line(&mut out, "保持", self.keeps, None);
        let _ = writeln!(out, "{}{}", pad("净变化", 12), signed_bytes(self.net_bytes));
        let _ = writeln!(
            out,
            "{}{}（{} 个文件 / {} 个变体）",
            pad("子库共", 12),
            pad(&human_bytes(self.desired_bytes), 12),
            thousands(self.desired.files),
            thousands(self.desired.variants),
        );
        if self.touched() == 0 {
            let _ = writeln!(out, "**一个文件都不用动**：目标已经与选择集对齐。");
        }
        if self.unreadable_sources > 0 {
            let _ = writeln!(
                out,
                "⚠️ 主库侧有 {} 个成员元数据读不到，它们照样要搬，但**容量按 0 计**\n\
                 （ADR-0021）——上面这几个数因此是**下界**。",
                thousands(self.unreadable_sources),
            );
        }
        if !self.empty_variants.is_empty() {
            let _ = writeln!(
                out,
                "⚠️ 有 {} 个变体选中了、却一个文件成员都没有：",
                thousands(self.empty_variants.len() as u64),
            );
            for key in self.empty_variants.iter().take(EXAMPLES) {
                let _ = writeln!(out, "  {key}");
            }
        }
        // 按类别拆开：ROM / 媒体 / 元数据。眼下只产得出 ROM（票 20 补另外两类）。
        let by_kind = kinds(&self.steps);
        if by_kind.len() > 1 {
            heading(&mut out, "按类别");
            let _ = writeln!(out, "{}{}类别", pad("文件", 10), pad("容量", 12));
            for kind in FileKind::all() {
                let Some(tally) = by_kind.get(&kind) else {
                    continue;
                };
                let _ = writeln!(
                    out,
                    "{}{}{}",
                    pad(&thousands(tally.files), 10),
                    pad(&human_bytes(tally.bytes), 12),
                    kind.label(),
                );
            }
        }

        for act in [Act::Delete, Act::Update, Act::Add] {
            let mut steps: Vec<&Step> = self.steps.iter().filter(|s| s.act == act).collect();
            if steps.is_empty() {
                continue;
            }
            // 最大的排前面：要砍要等，先看得见大头。
            steps.sort_by(|a, b| {
                b.bytes
                    .max(b.was)
                    .cmp(&a.bytes.max(a.was))
                    .then_with(|| a.path.cmp(&b.path))
            });
            heading(&mut out, act.label());
            for step in steps.iter().take(EXAMPLES) {
                let _ = writeln!(
                    out,
                    "  {}{}{}",
                    pad(&human_bytes(step.bytes.max(step.was)), 12),
                    if step.restore { "（补回）" } else { "" },
                    step.path,
                );
            }
            if steps.len() > EXAMPLES {
                let _ = writeln!(
                    out,
                    "  …… 另有 {} 个。`--json` 出完整的一份。",
                    thousands((steps.len() - EXAMPLES) as u64),
                );
            }
        }

        heading(&mut out, "目标上对不上的");
        if self.surprises.is_empty() {
            let _ = writeln!(out, "没有。清单记的每一条在目标上都还是原样。");
        } else {
            for kind in SurpriseKind::all() {
                let rows: Vec<_> = self
                    .surprises
                    .iter()
                    .filter(|surprise| surprise.kind == kind)
                    .collect();
                if rows.is_empty() {
                    continue;
                }
                let _ = writeln!(
                    out,
                    "{}  {} 个",
                    pad(kind.label(), 10),
                    thousands(rows.len() as u64)
                );
                for surprise in rows.iter().take(SURPRISE_EXAMPLES) {
                    let _ = writeln!(
                        out,
                        "  {}{}",
                        if surprise.still_wanted {
                            "[还要] "
                        } else {
                            "[不要了] "
                        },
                        surprise.path,
                    );
                }
                if rows.len() > SURPRISE_EXAMPLES {
                    let _ = writeln!(
                        out,
                        "  …… 另有 {} 个。",
                        thousands((rows.len() - SURPRISE_EXAMPLES) as u64)
                    );
                }
            }
            let _ = writeln!(
                out,
                "这些**本次一律不动**。清单说有、实际没了的**不会静默补回**——那可能是你\n\
                 在掌机上有意删的（ADR-0015）。想让它别再回来，记一条例外：\n\
                 `romcat sublibrary except {} --exclude <变体的键>`；\n\
                 想补回来，这次加上 `--restore`。",
                self.sublibrary,
            );
        }
        if self.unlistable_dirs > 0 {
            let _ = writeln!(
                out,
                "⚠️ 目标上有 {} 个目录列不开，那几枝底下的东西全部说不清——说不清的一律不碰。",
                thousands(self.unlistable_dirs),
            );
        }

        heading(&mut out, "清单之外");
        if self.strangers == 0 {
            let _ = writeln!(out, "目标上没有清单之外的文件。");
        } else {
            let _ = writeln!(
                out,
                "{} 个文件、{}。**工具连碰都不碰**（ADR-0015）：手动拷进去的存档、\n\
                 金手指、截图落在清单之外，连成为一条计划步骤的路径都没有。",
                thousands(self.strangers),
                human_bytes(self.stranger_bytes),
            );
            if self.stranger_unreadable > 0 {
                let _ = writeln!(
                    out,
                    "其中 {} 个元数据读不到，容量按 0 计——上面那个数是下界。",
                    thousands(self.stranger_unreadable),
                );
            }
        }

        heading(&mut out, "装得下吗");
        // 比的是**目标现占加净变化**，不是子库该有多大：卡上的地方是共用的，
        // 手动拷进去的存档、落点被占传不上去的、被改过因而不删的，全都还占着位置。
        let _ = writeln!(
            out,
            "{}{}（其中清单之外 {}）",
            pad("目标现占", 12),
            pad(&human_bytes(self.actual_bytes), 12),
            human_bytes(self.stranger_bytes),
        );
        let _ = writeln!(
            out,
            "{}{}",
            pad("同步之后", 12),
            human_bytes(self.after_bytes),
        );
        match (self.capacity, self.over_capacity) {
            (None, _) => {
                let _ = writeln!(
                    out,
                    "没设容量上限。`romcat sublibrary set {} --capacity 512GB` 设一个\n\
                     就能在这里看到余量。",
                    self.sublibrary,
                );
            }
            (Some(limit), None) => {
                let _ = writeln!(
                    out,
                    "装得下：{} / {}，还剩 {}。",
                    human_bytes(self.after_bytes),
                    human_bytes(limit),
                    human_bytes(limit.saturating_sub(self.after_bytes)),
                );
            }
            (Some(limit), Some(over)) => {
                let _ = writeln!(
                    out,
                    "**装不下**：{} / {}，超出 {}。",
                    human_bytes(self.after_bytes),
                    human_bytes(limit),
                    human_bytes(over),
                );
                let _ = writeln!(
                    out,
                    "**不会自动截断**（ADR-0016）——同一套规则在两张不同容量的卡上会选出\n\
                     完全不同的东西，而你无从得知它砍掉了什么。砍谁由你定，最大的几个变体是：",
                );
                let _ = writeln!(out, "  {}{}变体", pad("腾出", 12), pad("累计", 12));
                for trim in &self.trim_suggestions {
                    let _ = writeln!(
                        out,
                        "  {}{}{}",
                        pad(&human_bytes(trim.bytes), 12),
                        pad(&human_bytes(trim.cumulative), 12),
                        trim.variant,
                    );
                }
                if let Some(last) = self.trim_suggestions.last()
                    && last.cumulative < over
                {
                    let _ = writeln!(
                        out,
                        "这几个**全砍掉也只腾出 {}，还差 {}**——`--json` 出完整的一份，\n\
                         或者改规则少选一些。",
                        human_bytes(last.cumulative),
                        human_bytes(over - last.cumulative),
                    );
                }
                let _ = writeln!(
                    out,
                    "排除一个：`romcat sublibrary except {} --exclude <变体的键>`",
                    self.sublibrary,
                );
                if self.stranger_bytes > 0 {
                    let _ = writeln!(
                        out,
                        "另一条路是自己清掉清单之外那 {}——**工具不会替你动它们**。",
                        human_bytes(self.stranger_bytes),
                    );
                }
            }
        }
        out
    }
}

/// 差量那一栏的一行。
fn line(out: &mut String, name: &str, tally: Tally, before: Option<u64>) {
    let _ = writeln!(
        out,
        "{}{}{}{}",
        pad(name, 12),
        pad(
            &format!(
                "{} 个文件 / {} 个变体",
                thousands(tally.files),
                thousands(tally.variants)
            ),
            26
        ),
        pad(&human_bytes(tally.bytes), 12),
        match before {
            Some(before) if tally.files > 0 => format!("（原先 {}）", human_bytes(before)),
            _ => String::new(),
        },
    );
}

/// 带符号的容量，给净变化用。
fn signed_bytes(net: i64) -> String {
    let sign = if net < 0 { "-" } else { "+" };
    format!("{sign}{}", human_bytes(net.unsigned_abs()))
}

/// 把步骤按类别折成账。
fn kinds(steps: &[Step]) -> BTreeMap<FileKind, Tally> {
    let mut out: BTreeMap<FileKind, Tally> = BTreeMap::new();
    for step in steps {
        let tally = out.entry(step.kind).or_default();
        tally.files += 1;
        tally.bytes += step.bytes.max(step.was);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sublibrary::Sublibrary;
    use crate::sync::{Desired, DesiredFile, Manifest, Options, TargetState, plan};

    fn 子库() -> Sublibrary {
        Sublibrary {
            name: "掌机".to_string(),
            target: "/Volumes/SDCARD/Games".to_string(),
            format: "Pegasus".to_string(),
            capacity: Some(4096),
        }
    }

    #[test]
    fn 预览印得出新增删除与净变化() {
        let desired = Desired {
            files: vec![DesiredFile {
                path: "GB/一.zip".to_string(),
                kind: FileKind::Rom,
                bytes: 1024,
                unreadable: false,
                source: "GB/一.zip".to_string(),
                source_stamp: crate::sync::Stamp {
                    bytes: 1024,
                    mtime_ns: Some(1),
                },
                variant: "GB/一.zip".to_string(),
            }],
            ..Desired::default()
        };
        let text = plan(
            &子库(),
            &desired,
            &Manifest::empty(),
            &TargetState::default(),
            Options::default(),
        )
        .render_text();
        assert!(text.contains("差量预览"), "{text}");
        assert!(text.contains("新增"), "{text}");
        assert!(text.contains("净变化"), "{text}");
        assert!(text.contains("+1.00 KiB"), "{text}");
        assert!(text.contains("装得下"), "{text}");
    }

    #[test]
    fn 什么都不用动时明说() {
        let text = plan(
            &子库(),
            &Desired::default(),
            &Manifest::empty(),
            &TargetState::default(),
            Options::default(),
        )
        .render_text();
        assert!(text.contains("一个文件都不用动"), "{text}");
    }
}
