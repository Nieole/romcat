//! **命中率报告**：这一层到底认出了多少东西。
//!
//! 它和体检报告、DAT 仓库报告是同一个形状——**从中立库折出来，不重跑一遍识别**
//! （ADR-0001）。因此盘不在位、DAT 库不在手边时，上一趟的结论照样看得见。
//!
//! ## 命中率是两个数不是一个
//!
//! 分母有两种取法，报告**两个都给**，因为它们回答的是两个问题：
//!
//! - **撞了 DAT 的里面命中多少**（`命中 / (命中 + 未命中)`）——这一层准不准。
//! - **全部变体里命中多少**（`命中 / 变体总数`）——这个库现在被认出来多少。
//!
//! **跳过**（补丁、没有发行版链接）与**无判据**（容器穿不透、压缩镜像、目录树转储）
//! 都不进第一个分母：把它们混进未命中，等于拿「本来就不该撞」的东西去压低准确率。
//! 但它们**照样进第二个**——库里确实还有这么多东西没被认出来，那不该被藏起来。

use std::collections::BTreeMap;
use std::fmt::Write as _;

use rusqlite::params;
use serde::Serialize;

use crate::catalog::{Catalog, CatalogError, State};
use crate::dat::DatRepo;
use crate::report::{pad, thousands, width};

/// 一个平台一行。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PlatformRow {
    /// 平台名；认不出平台的归到「（未知）」。
    pub platform: String,
    /// 这个平台有多少个变体。
    pub variants: u64,
    /// 命中。
    pub matched: u64,
    /// 未命中。
    pub unmatched: u64,
    /// 无判据。
    pub no_evidence: u64,
    /// 跳过。
    pub skipped: u64,
    /// DAT 库里这个平台有多少条条目——**没有弹药的平台命中率低是另一回事**。
    pub dat_games: u64,
}

impl PlatformRow {
    /// 撞了 DAT 的里面命中多少。
    #[must_use]
    pub fn hit_rate(&self) -> f64 {
        let tried = self.matched + self.unmatched;
        if tried == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            self.matched as f64 * 100.0 / tried as f64
        }
    }

    /// 全部变体里命中多少。
    #[must_use]
    pub fn coverage(&self) -> f64 {
        if self.variants == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            self.matched as f64 * 100.0 / self.variants as f64
        }
    }
}

/// 一组按理由的计数，带几个例子。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ReasonRow {
    /// 理由。
    pub reason: String,
    /// 多少个变体。
    pub count: u64,
    /// 几个例子，好让人一眼看出判得对不对。
    pub examples: Vec<String>,
}

/// 一个数据源贡献了多少条候选。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SourceRow {
    /// 数据源。
    pub source: String,
    /// 候选条数。
    pub candidates: u64,
    /// 其中自动通过的。
    pub accepted: u64,
    /// 涉及多少个变体。
    pub variants: u64,
}

/// 识别的命中率报告。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct IdentifyReport {
    /// 中立库在哪。
    pub catalog: String,
    /// DAT 库在哪。
    pub dat: String,
    /// 按平台。
    pub platforms: Vec<PlatformRow>,
    /// 合计。
    pub total: PlatformRow,
    /// 跳过的按理由分。
    pub skipped: Vec<ReasonRow>,
    /// 无判据的按理由分。
    pub no_evidence: Vec<ReasonRow>,
    /// 候选按数据源分。
    pub sources: Vec<SourceRow>,
    /// 候选总数。
    pub candidates: u64,
    /// 自动通过的候选数。
    pub accepted: u64,
    /// 有不止一条候选的变体数。
    pub multi_candidate_variants: u64,
    /// 命中里带**汉化**记号的变体数。
    pub fan_translated: u64,
    /// 命中里带**官中**记号的变体数。
    pub official_chinese: u64,
    /// 验出是 NKit 处理过的镜像有几份。
    pub nkit: u64,
    /// 识别建出来的作品数。
    pub works: u64,
    /// 识别建出来的发行版数。
    pub releases: u64,
    /// 这一轮回盘读了多少字节。
    pub read_bytes: u64,
}

const UNKNOWN_PLATFORM: &str = "（未知）";

impl IdentifyReport {
    /// 从中立库与 DAT 库折出报告。**不碰主库、不联网。**
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn build(catalog: &Catalog, repo: &DatRepo) -> Result<Self, CatalogError> {
        let mut report = Self {
            catalog: catalog.location().to_string(),
            dat: repo.location(),
            ..Self::default()
        };
        let ammo = dat_games(repo);
        let mut rows: BTreeMap<String, PlatformRow> = BTreeMap::new();
        catalog.for_each_identification(&mut |platform, state, reason, key, read_bytes| {
            let platform = platform.unwrap_or(UNKNOWN_PLATFORM).to_string();
            let row = rows.entry(platform.clone()).or_insert_with(|| PlatformRow {
                platform: platform.clone(),
                dat_games: ammo.get(&platform).copied().unwrap_or(0),
                ..PlatformRow::default()
            });
            row.variants += 1;
            match state {
                State::Matched => row.matched += 1,
                State::Unmatched => row.unmatched += 1,
                State::NoEvidence => row.no_evidence += 1,
                State::Skipped => row.skipped += 1,
            }
            report.read_bytes += read_bytes;
            if let Some(reason) = reason {
                let bucket = match state {
                    State::Skipped => Some(&mut report.skipped),
                    State::NoEvidence => Some(&mut report.no_evidence),
                    _ => None,
                };
                if let Some(bucket) = bucket {
                    record_reason(bucket, reason, key);
                }
            }
        })?;

        report.platforms = rows.into_values().collect();
        // 变体多的排前面：那正是「还差多少」最该先看的顺序。
        report.platforms.sort_by(|a, b| {
            b.variants
                .cmp(&a.variants)
                .then_with(|| a.platform.cmp(&b.platform))
        });
        for row in &report.platforms {
            report.total.variants += row.variants;
            report.total.matched += row.matched;
            report.total.unmatched += row.unmatched;
            report.total.no_evidence += row.no_evidence;
            report.total.skipped += row.skipped;
            report.total.dat_games += row.dat_games;
        }
        report.total.platform = "合计".to_string();
        sort_reasons(&mut report.skipped);
        sort_reasons(&mut report.no_evidence);

        let counts = catalog.candidate_counts()?;
        report.candidates = counts.candidates;
        report.accepted = counts.accepted;
        report.multi_candidate_variants = counts.multi;
        report.fan_translated = counts.fan;
        report.official_chinese = counts.official;
        report.nkit = counts.nkit;
        report.works = counts.works;
        report.releases = counts.releases;
        report.sources = counts
            .sources
            .into_iter()
            .map(|count| SourceRow {
                source: count.source,
                candidates: count.candidates,
                accepted: count.accepted,
                variants: count.variants,
            })
            .collect();
        Ok(report)
    }

    /// 渲染成给人看的文本。
    #[must_use]
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "识别：CRC-32 加大小撞 DAT");
        let _ = writeln!(out, "{}", "═".repeat(28));
        let _ = writeln!(out, "中立库          {}", self.catalog);
        let _ = writeln!(out, "DAT 库          {}", self.dat);
        let _ = writeln!(
            out,
            "变体            {} 个：命中 {}、未命中 {}、无判据 {}、跳过 {}",
            thousands(self.total.variants),
            thousands(self.total.matched),
            thousands(self.total.unmatched),
            thousands(self.total.no_evidence),
            thousands(self.total.skipped),
        );
        let _ = writeln!(
            out,
            "命中率          撞了 DAT 的里面 {:.1}%（{} / {}），全部变体里 {:.1}%",
            self.total.hit_rate(),
            thousands(self.total.matched),
            thousands(self.total.matched + self.total.unmatched),
            self.total.coverage(),
        );
        let _ = writeln!(
            out,
            "候选            {} 条（自动通过 {}），{} 个变体有不止一条候选",
            thousands(self.candidates),
            thousands(self.accepted),
            thousands(self.multi_candidate_variants),
        );
        let _ = writeln!(
            out,
            "中文            命中里汉化版 {} 个、官中版 {} 个",
            thousands(self.fan_translated),
            thousands(self.official_chinese),
        );
        let _ = writeln!(
            out,
            "三层            建出作品 {} 个、发行版 {} 个",
            thousands(self.works),
            thousands(self.releases),
        );
        if self.nkit > 0 {
            let _ = writeln!(
                out,
                "NKit            {} 份镜像验出是 NKit 处理过的，一律不许自动通过（票 09）",
                thousands(self.nkit)
            );
        }

        if self.total.variants == 0 {
            let _ = writeln!(out, "\n库里还没有变体。先跑一次 `romcat scan`。");
            return out;
        }

        heading(&mut out, "按平台");
        let _ = writeln!(
            out,
            "{}{}{}{}{}{}{}DAT 条目",
            pad("平台", 10),
            pad("变体", 9),
            pad("命中", 9),
            pad("未命中", 9),
            pad("无判据", 9),
            pad("跳过", 8),
            pad("命中率", 9),
        );
        for row in &self.platforms {
            let _ = writeln!(
                out,
                "{}{}{}{}{}{}{}{}",
                pad(&row.platform, 10),
                pad(&thousands(row.variants), 9),
                pad(&thousands(row.matched), 9),
                pad(&thousands(row.unmatched), 9),
                pad(&thousands(row.no_evidence), 9),
                pad(&thousands(row.skipped), 8),
                pad(&format!("{:.1}%", row.hit_rate()), 9),
                thousands(row.dat_games),
            );
        }
        let _ = writeln!(
            out,
            "（命中率的分母是「撞了 DAT 的」，跳过与无判据不在里面——混进去命中率就失真了）"
        );

        reasons(&mut out, "跳过的（不该撞 DAT）", &self.skipped);
        reasons(&mut out, "无判据的（这一层拿不到判据）", &self.no_evidence);

        if !self.sources.is_empty() {
            heading(&mut out, "候选来自哪个源");
            let _ = writeln!(
                out,
                "{}{}{}涉及变体",
                pad("源", 12),
                pad("候选", 10),
                pad("自动通过", 12),
            );
            for row in &self.sources {
                let _ = writeln!(
                    out,
                    "{}{}{}{}",
                    pad(&row.source, 12),
                    pad(&thousands(row.candidates), 10),
                    pad(&thousands(row.accepted), 12),
                    thousands(row.variants),
                );
            }
        }
        out
    }
}

fn heading(out: &mut String, title: &str) {
    let _ = write!(out, "\n{title}\n{}\n", "─".repeat(width(title) / 2 + 8));
}

fn reasons(out: &mut String, title: &str, rows: &[ReasonRow]) {
    if rows.is_empty() {
        return;
    }
    heading(out, title);
    for row in rows {
        let _ = writeln!(out, "{}{}", pad(&thousands(row.count), 9), row.reason);
        for example in &row.examples {
            let _ = writeln!(out, "         · {example}");
        }
    }
}

fn record_reason(bucket: &mut Vec<ReasonRow>, reason: &str, key: &str) {
    // 理由前半截是分类（「补丁」「容器穿不透」），后半截是这一条的细节。
    // 按前半截归堆，细节留给例子——不归堆的话，几万条各不相同的理由会把报告淹掉。
    let head = reason.split_once('：').map_or(reason, |(head, _)| head);
    let row = match bucket.iter_mut().find(|row| row.reason == head) {
        Some(row) => row,
        None => {
            bucket.push(ReasonRow {
                reason: head.to_string(),
                ..ReasonRow::default()
            });
            bucket.last_mut().expect("刚推进去")
        }
    };
    row.count += 1;
    if row.examples.len() < 3 {
        row.examples.push(format!("{key}｜{reason}"));
    }
}

fn sort_reasons(rows: &mut [ReasonRow]) {
    rows.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.reason.cmp(&b.reason)));
}

fn dat_games(repo: &DatRepo) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    let Ok(mut statement) = repo
        .conn()
        .prepare("SELECT platform, SUM(games) FROM dat GROUP BY platform")
    else {
        return out;
    };
    let Ok(rows) = statement.query_map(params![], |row| {
        Ok((
            row.get::<_, String>(0)?,
            u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
        ))
    }) else {
        return out;
    };
    for row in rows.flatten() {
        out.insert(row.0, row.1);
    }
    out
}
