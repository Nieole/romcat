//! **刮削报告**：这一趟到底补上了什么，以及**补不上什么**。
//!
//! 它和体检报告、命中率报告是同一个形状——**从中立库折出来，不重跑一遍刮削**
//! （ADR-0001）。因此盘不在位时上一趟的结论照样看得见。
//!
//! ## 缺口那一节不是装饰
//!
//! 离线档补不上简介、类型与开发商——这是调研早就写明的结论（Bangumi 的离线 dump 不含
//! 图片、Wikidata 只有标签没有简介）。**报告必须把这件事说出来**：空着的字段如果不点名，
//! 用户看到的就是「刮削跑完了」，而实际上前端里一半的格子是空的。这一节正是票 14
//! 存在的理由。

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde::Serialize;

use crate::catalog::{Catalog, CatalogError};
use crate::report::{human_bytes, pad, thousands, width};

use super::priority::Priorities;
use super::{Field, MediaKind, Options, PlanCounts};

/// 一个字段一行。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FieldRow {
    /// 字段。
    pub field: String,
    /// 有几个锚点拿到了这个字段的值（**按锚点数，不是按值的条数**）。
    pub subjects: u64,
    /// 值一共几条。它大于锚点数，因为多个源各存一份。
    pub values: u64,
    /// 各个源各贡献了几条：`(源, 条数)`，多的排前面。
    pub sources: Vec<(String, u64)>,
    /// **按优先级合并之后真正胜出的**：`(源, 在几个锚点上胜出)`。
    ///
    /// 它与 `sources` 是两回事，而两个都得看：一个源贡献了 6,800 条却几乎没胜出，
    /// 说明它排在链尾、而前面的源覆盖得比它全——那正是「某个源值不值得继续用」的判据。
    /// 这一栏也是**合并这件事在产品里真的跑了一遍**的证据，不是只在测试里跑。
    pub winners: Vec<(String, u64)>,
    /// 优先级链。
    pub order: Vec<String>,
}

/// 一种媒体一行。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MediaRow {
    /// 媒体类型。
    pub kind: String,
    /// 引用条数。
    pub refs: u64,
    /// 涉及几份不同的内容。
    pub blobs: u64,
}

/// 一份刮削报告。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ScrapeReport {
    /// 中立库在哪。
    pub catalog: String,
    /// **媒体池**在哪。
    pub pool: String,
    /// 用的哪个策略档案。
    pub profile: String,
    /// 这一档有哪些源。
    pub sources: Vec<String>,
    /// 作品锚点几个。
    pub works: u64,
    /// 变体锚点几个。
    pub variants: u64,
    /// 归得上某个变体的本地媒体有几份。
    pub local_media: u64,
    /// 有刮削结论的锚点：`(锚点种类, 个数)`。
    pub scraped: Vec<(String, u64)>,
    /// 按字段。
    pub fields: Vec<FieldRow>,
    /// **离线档补不上的字段**。
    pub gaps: Vec<String>,
    /// 按媒体类型。
    pub media: Vec<MediaRow>,
    /// 池里几份媒体。
    pub pool_blobs: u64,
    /// 池里多少字节。
    pub pool_bytes: u64,
    /// 媒体引用一共几条。
    pub pool_refs: u64,
    /// 被不止一个锚点引用的媒体有几份。
    pub pool_shared: u64,
    /// 按平台覆盖的优先级有哪几条：`(平台, 字段, 顺序)`。
    pub platform_overrides: Vec<(String, String, Vec<String>)>,
    /// 优先级表里点名了、而这一档根本没有的源。
    ///
    /// **不是错误**（没列到的源按时间戳兜底，列了不存在的源也只是排序时永远轮不到它），
    /// 但十有八九是打错了字，而打错的后果是静默的：那个源被排到链尾，用户以为自己
    /// 调了优先级，实际什么也没发生。所以报出来。
    pub unknown_sources: Vec<String>,
}

impl ScrapeReport {
    /// 从中立库折出报告。**不碰主库、不联网。**
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn build(
        catalog: &Catalog,
        priorities: &Priorities,
        options: &Options,
        sources: &[&str],
        plan: &PlanCounts,
    ) -> Result<Self, CatalogError> {
        let mut report = Self {
            catalog: catalog.location().to_string(),
            pool: crate::path::display(&options.pool),
            profile: options.profile.label().to_string(),
            sources: sources.iter().map(|s| (*s).to_string()).collect(),
            works: plan.works,
            variants: plan.variants,
            local_media: plan.local_media,
            scraped: catalog.scraped_subjects()?,
            ..Self::default()
        };

        let subjects = catalog.field_subjects()?;
        let winners = winners(catalog, priorities)?;
        let mut by_field: BTreeMap<String, FieldRow> = BTreeMap::new();
        for count in catalog.field_counts()? {
            let row = by_field
                .entry(count.field.clone())
                .or_insert_with(|| FieldRow {
                    field: count.field.clone(),
                    subjects: subjects.get(&count.field).copied().unwrap_or(0),
                    winners: winners
                        .get(&count.field)
                        .map(|by_source| {
                            let mut rows: Vec<(String, u64)> = by_source
                                .iter()
                                .map(|(source, count)| (source.clone(), *count))
                                .collect();
                            rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                            rows
                        })
                        .unwrap_or_default(),
                    order: priorities
                        .order(&count.field, None)
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                    ..FieldRow::default()
                });
            row.values += count.values;
            row.sources.push((count.source, count.values));
        }
        // 报告里的字段按 `Field::all()` 的固定顺序排，不按数量——同一份库出的报告
        // 每次长得一样，人才能对着上一次看差异。
        let mut filled: BTreeSet<String> = BTreeSet::new();
        for field in Field::all() {
            if let Some(row) = by_field.remove(field.label()) {
                filled.insert(row.field.clone());
                report.fields.push(row);
            }
        }
        // 优先级表之外的字段（用户自己加的）照样报，排在后面。
        for (_, row) in by_field {
            filled.insert(row.field.clone());
            report.fields.push(row);
        }
        report.gaps = Field::all()
            .into_iter()
            .map(|field| field.label().to_string())
            .filter(|label| !filled.contains(label))
            .collect();

        let counts = catalog.pool_counts()?;
        report.pool_blobs = counts.blobs;
        report.pool_bytes = counts.bytes;
        report.pool_refs = counts.refs;
        report.pool_shared = counts.shared;
        for kind in MediaKind::all() {
            let (refs, blobs) = catalog.media_kind_counts(kind.label())?;
            if refs > 0 {
                report.media.push(MediaRow {
                    kind: kind.label().to_string(),
                    refs,
                    blobs,
                });
            }
        }
        report.platform_overrides = priorities
            .platform_overrides()
            .into_iter()
            .map(|(platform, field, order)| {
                (platform.to_string(), field.to_string(), order.to_vec())
            })
            .collect();
        report.unknown_sources = priorities.sources_not_in(sources);
        Ok(report)
    }

    /// 渲染成给人看的文本。
    #[must_use]
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "刮削：{}", self.profile);
        let _ = writeln!(out, "{}", "═".repeat(28));
        let _ = writeln!(out, "中立库          {}", self.catalog);
        let _ = writeln!(out, "媒体池          {}", self.pool);
        let _ = writeln!(out, "数据源          {}", self.sources.join(" / "));
        let _ = writeln!(
            out,
            "锚点            作品 {} 个、变体 {} 个",
            thousands(self.works),
            thousands(self.variants),
        );
        let scraped: Vec<String> = self
            .scraped
            .iter()
            .map(|(kind, count)| format!("{kind} {}", thousands(*count)))
            .collect();
        let _ = writeln!(
            out,
            "有结论的        {}",
            if scraped.is_empty() {
                "（一条都没有）".to_string()
            } else {
                scraped.join("、")
            }
        );
        let _ = writeln!(
            out,
            "媒体池          {} 份、{}；引用 {} 条，其中 {} 份被不止一个锚点引用",
            thousands(self.pool_blobs),
            human_bytes(self.pool_bytes),
            thousands(self.pool_refs),
            thousands(self.pool_shared),
        );

        if self.fields.is_empty() {
            let _ = writeln!(
                out,
                "\n一个字段都没采到。先跑一次 `romcat identify`——刮削是在识别结论上做的。"
            );
            return out;
        }

        heading(&mut out, "按字段");
        let _ = writeln!(
            out,
            "{}{}{}各源贡献",
            pad("字段", 10),
            pad("锚点", 10),
            pad("值", 10),
        );
        for row in &self.fields {
            let by_source: Vec<String> = row
                .sources
                .iter()
                .map(|(source, count)| format!("{source} {}", thousands(*count)))
                .collect();
            let _ = writeln!(
                out,
                "{}{}{}{}",
                pad(&row.field, 10),
                pad(&thousands(row.subjects), 10),
                pad(&thousands(row.values), 10),
                by_source.join("、"),
            );
        }

        heading(&mut out, "合并之后谁说了算");
        let _ = writeln!(
            out,
            "{}{}（按优先级合并一遍数出来的；贡献多而胜出少，说明它排在链尾）",
            pad("字段", 10),
            pad("胜出", 10),
        );
        for row in &self.fields {
            let by_source: Vec<String> = row
                .winners
                .iter()
                .map(|(source, count)| format!("{source} {}", thousands(*count)))
                .collect();
            let _ = writeln!(
                out,
                "{}{}",
                pad(&row.field, 10),
                if by_source.is_empty() {
                    "（没有）".to_string()
                } else {
                    by_source.join("、")
                },
            );
        }

        if !self.gaps.is_empty() {
            heading(&mut out, "离线档补不上的字段");
            let _ = writeln!(
                out,
                "{}——本地数据源里没有这些东西，要等在线档（票 14）。",
                self.gaps.join("、")
            );
        }

        if self.media.is_empty() {
            heading(&mut out, "媒体");
            let _ = writeln!(
                out,
                "一份都没收进来。离线档的媒体只能是主库里现成的图与视频，\
                 而它们得躺在变体的独占目录里、或者与主文件同名。"
            );
        } else {
            heading(&mut out, "媒体");
            let _ = writeln!(out, "{}{}内容份数", pad("类型", 10), pad("引用", 10));
            for row in &self.media {
                let _ = writeln!(
                    out,
                    "{}{}{}",
                    pad(&row.kind, 10),
                    pad(&thousands(row.refs), 10),
                    thousands(row.blobs),
                );
            }
            let _ = writeln!(
                out,
                "\n归得上某个变体的本地媒体一共 {} 份。**文件名不是媒体的主键**——\
                 池里按内容哈希存，同一张图被多个条目引用只存一份（ADR-0009）。",
                thousands(self.local_media)
            );
        }

        if !self.unknown_sources.is_empty() {
            heading(&mut out, "优先级表里点名了、而这一档没有的源");
            let _ = writeln!(
                out,
                "{}——不是错误（列了不存在的源只是排序时永远轮不到它），\
                 但十有八九是打错了字，而打错的后果是静默的。",
                self.unknown_sources.join("、")
            );
        }

        heading(&mut out, "优先级");
        for row in &self.fields {
            if row.order.is_empty() {
                continue;
            }
            let _ = writeln!(out, "{}{}", pad(&row.field, 10), row.order.join(" > "));
        }
        for (platform, field, order) in &self.platform_overrides {
            let _ = writeln!(
                out,
                "{}{}（覆盖）{}",
                pad(platform, 10),
                pad(field, 10),
                order.join(" > "),
            );
        }
        out
    }
}

/// 把库里的值真正合并一遍，数出**每个字段各是哪个源胜出**。
///
/// 一个锚点上的值攒齐了才轮得到合并（优先级要在同一批候选值之间比），所以按
/// `(锚点种类, 锚点)` 分批——SQL 那边已经按这两列排好序，攒一个锚点的量就够。
///
/// 平台传 `None`：报告是全库口径，而按平台的覆盖是**按平台**的，混在一起报会既不是
/// 通用那条也不是覆盖那条。覆盖有哪几条另有一节列着。
fn winners(
    catalog: &Catalog,
    priorities: &Priorities,
) -> Result<BTreeMap<String, BTreeMap<String, u64>>, CatalogError> {
    let mut out: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut current: Option<(String, String)> = None;
    let mut batch: Vec<crate::catalog::scrape::ScrapedValue> = Vec::new();
    let tally = |batch: &[crate::catalog::scrape::ScrapedValue],
                 out: &mut BTreeMap<String, BTreeMap<String, u64>>| {
        let fields: BTreeSet<&str> = batch.iter().map(|value| value.field.as_str()).collect();
        for field in fields {
            if let Some(best) = priorities.pick(field, None, batch) {
                *out.entry(field.to_string())
                    .or_default()
                    .entry(best.source.clone())
                    .or_default() += 1;
            }
        }
    };
    catalog.for_each_scraped_value(&mut |anchor, subject, value| {
        let here = (anchor.to_string(), subject.to_string());
        if current.as_ref() != Some(&here) {
            tally(&batch, &mut out);
            batch.clear();
            current = Some(here);
        }
        batch.push(value);
    })?;
    tally(&batch, &mut out);
    Ok(out)
}

fn heading(out: &mut String, title: &str) {
    let _ = write!(out, "\n{title}\n{}\n", "─".repeat(width(title) / 2 + 8));
}
