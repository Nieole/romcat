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

use crate::catalog::identify::PlatformConflict;
use crate::catalog::{Catalog, CatalogError, State};
use crate::classify::has_cjk;
use crate::dat::DatRepo;
use crate::report::{heading, pad, thousands};

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
    /// 命中里带**官中**记号的变体数（票 10 的中文占比统计）。
    pub official_chinese: u64,
    /// 命中里带**汉化**记号的变体数。
    pub fan_translated: u64,
    /// **文件名含汉字**的变体数——票 01 用的那个粗略代理，摆在旁边好看出差多少。
    pub cjk_named: u64,
    /// DAT 库里这个平台索引了多少条**序列号**（票 09）。
    ///
    /// 它与 `dat_games` 是两种弹药，缺哪一种都会让命中率低而**与识别准不准无关**。
    /// 真库实测 Redump 的官方 DAT 一条序列号都不写：PS2 / NGC / WII 三个平台的
    /// 序列号读得出来也无处可撞。这一列就是为了让那件事一眼看得见。
    pub dat_serials: u64,
}

/// 一个百分比，分母是 0 时算 0。**这份报告里每一个占比都走它**——四个算式各写一遍
/// 的话，哪天要改「分母是 0 怎么办」就得记得改四处。
fn rate(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    {
        part as f64 * 100.0 / whole as f64
    }
}

impl PlatformRow {
    /// 撞了 DAT 的里面命中多少。
    #[must_use]
    pub fn hit_rate(&self) -> f64 {
        rate(self.matched, self.matched + self.unmatched)
    }

    /// 认出中文的占这个平台多少。**官中版与汉化版加在一起**——这一条问的是
    /// 「库里有多少东西是中文的」，那两种都算；分开数的那两列在上面（ADR-0012）。
    #[must_use]
    pub fn chinese_rate(&self) -> f64 {
        rate(self.official_chinese + self.fan_translated, self.variants)
    }

    /// 文件名含汉字的占这个平台多少（票 01 的粗略代理）。
    #[must_use]
    pub fn cjk_rate(&self) -> f64 {
        rate(self.cjk_named, self.variants)
    }

    /// 全部变体里命中多少。
    #[must_use]
    pub fn coverage(&self) -> f64 {
        rate(self.matched, self.variants)
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
    /// **内部头说的平台与目录声明的平台对不上**的变体数（ADR-0011）。
    pub platform_conflicts: u64,
    /// 其中几条的样子，好让人一眼看出是下错了还是放错了。
    pub conflict_examples: Vec<PlatformConflict>,
}

const UNKNOWN_PLATFORM: &str = "（未知）";

/// 每一类理由、每一种冲突各举几个例子。三条够看出「判得对不对」，多了淹掉报告。
const EXAMPLES: usize = 3;

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
        let serials = repo.serial_counts().unwrap_or_default();
        let chinese = catalog.chinese_by_platform(UNKNOWN_PLATFORM)?;
        let mut rows: BTreeMap<String, PlatformRow> = BTreeMap::new();
        catalog.for_each_identification(&mut |platform, state, reason, key, read_bytes| {
            let platform = platform.unwrap_or(UNKNOWN_PLATFORM).to_string();
            let row = rows.entry(platform.clone()).or_insert_with(|| PlatformRow {
                platform: platform.clone(),
                dat_games: ammo.get(&platform).copied().unwrap_or(0),
                dat_serials: serials.get(&platform).copied().unwrap_or(0),
                official_chinese: chinese.get(&platform).map_or(0, |it| it.0),
                fan_translated: chinese.get(&platform).map_or(0, |it| it.1),
                ..PlatformRow::default()
            });
            row.variants += 1;
            // 票 01 那个粗略代理就地算出来：报告要能并排给出「文件名猜的」与
            // 「识别认出来的」，不然「这一层还差多少」只能靠感觉。
            if has_cjk(std::path::Path::new(key)) {
                row.cjk_named += 1;
            }
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
            report.total.dat_serials += row.dat_serials;
            report.total.official_chinese += row.official_chinese;
            report.total.fan_translated += row.fan_translated;
            report.total.cjk_named += row.cjk_named;
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
        report.platform_conflicts = catalog.platform_conflict_count()?;
        report.conflict_examples = catalog.platform_conflicts(EXAMPLES)?;
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
        let _ = writeln!(
            out,
            "识别：CRC-32 加大小撞 DAT，撞不上的读光盘序列号与卡带内部头"
        );
        let _ = writeln!(out, "{}", "═".repeat(40));
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
            "中文            命中里官中版 {} 个（{:.1}%）、汉化版 {} 个（{:.1}%）；\
             文件名含汉字的有 {} 个（{:.1}%，票 01 的粗略代理）",
            thousands(self.total.official_chinese),
            rate(self.total.official_chinese, self.total.variants),
            thousands(self.total.fan_translated),
            rate(self.total.fan_translated, self.total.variants),
            thousands(self.total.cjk_named),
            self.total.cjk_rate(),
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
                "NKit            {} 份镜像验出是 NKit 处理过的，一律不许自动通过——\
                 它的 CRC32 可能与好转储相同（Dolphin），要先转回 ISO",
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
            "{}{}{}{}{}{}{}{}序列号",
            pad("平台", 10),
            pad("变体", 9),
            pad("命中", 9),
            pad("未命中", 9),
            pad("无判据", 9),
            pad("跳过", 8),
            pad("命中率", 9),
            pad("DAT 条目", 11),
        );
        for row in &self.platforms {
            let _ = writeln!(
                out,
                "{}{}{}{}{}{}{}{}{}",
                pad(&row.platform, 10),
                pad(&thousands(row.variants), 9),
                pad(&thousands(row.matched), 9),
                pad(&thousands(row.unmatched), 9),
                pad(&thousands(row.no_evidence), 9),
                pad(&thousands(row.skipped), 8),
                pad(&format!("{:.1}%", row.hit_rate()), 9),
                pad(&thousands(row.dat_games), 11),
                thousands(row.dat_serials),
            );
        }
        let _ = writeln!(
            out,
            "（命中率的分母是「撞了 DAT 的」，跳过与无判据不在里面——混进去命中率就失真了）"
        );
        let _ = writeln!(
            out,
            "（最后两列是两种**弹药**：DAT 条目撞哈希，序列号撞光盘内部标识。哪一种为 0，\
             这个平台的命中率低就与识别准不准无关）"
        );

        heading(&mut out, "中文（这是识别结论，不是从文件名猜的）");
        let _ = writeln!(
            out,
            "{}{}{}{}{}文件名含汉字",
            pad("平台", 10),
            pad("变体", 9),
            pad("官中版", 9),
            pad("汉化版", 9),
            pad("中文占比", 11),
        );
        for row in &self.platforms {
            if row.official_chinese + row.fan_translated + row.cjk_named == 0 {
                continue;
            }
            let _ = writeln!(
                out,
                "{}{}{}{}{}{}（{:.1}%）",
                pad(&row.platform, 10),
                pad(&thousands(row.variants), 9),
                pad(&thousands(row.official_chinese), 9),
                pad(&thousands(row.fan_translated), 9),
                pad(&format!("{:.1}%", row.chinese_rate()), 11),
                thousands(row.cjk_named),
                row.cjk_rate(),
            );
        }
        let _ = writeln!(
            out,
            "（**官中版与汉化版不许加成一个数**：在卡带与光盘世代官中是一次独立的官方\
             **发行版**，汉化版是改过字节的**变体**——ADR-0012、ADR-0019）"
        );
        let _ = writeln!(
            out,
            "（最后一列是票 01 用文件名做的粗略代理。两列并排看，差出来的那部分就是\
             这一层还没认出来的中文）"
        );

        if self.platform_conflicts > 0 {
            heading(
                &mut out,
                "内部头与目录声明的平台对不上（ADR-0011：目录只是强先验）",
            );
            let _ = writeln!(
                out,
                "{}个变体：文件内容说的平台与它躺着的目录不一致。**这不是错误**，\
                 是下错、放错或压缩包混装，也是库体检最该报告的产出之一",
                thousands(self.platform_conflicts)
            );
            for it in &self.conflict_examples {
                let _ = writeln!(
                    out,
                    "         · {}｜目录说 {}，内部头说 {}（{}）",
                    it.variant_key, it.declared, it.found, it.member
                );
            }
        }

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
    if row.examples.len() < EXAMPLES {
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
