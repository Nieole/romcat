//! **刮削报告**：这一趟到底补上了什么，以及**补不上什么**。
//!
//! 它和体检报告、命中率报告是同一个形状——**从中立库折出来，不重跑一遍刮削**
//! （ADR-0001）。因此盘不在位时上一趟的结论照样看得见。
//!
//! ## 缺口那一节不是装饰
//!
//! 离线档补不上简介、类型与开发商——这是调研早就写明的结论（Bangumi 的离线 dump 不含
//! 图片、Wikidata 只有标签没有简介）。**报告必须把这件事说出来**：空着的字段如果不点名，
//! 用户看到的就是「刮削跑完了」，而实际上前端里一半的格子是空的。这一节正是**在线档**
//! 存在的理由，所以它也要说清「换 `--profile 在线` 跑一趟才补得上」。
//!
//! ## 在线那一节是给「对着真账号跑之前」看的
//!
//! 发了几个请求、服务端说还剩多少、并发上限是几、**未识别的变体一个请求都没发**——
//! 这几个数必须在报告里，因为在线档赌上的是用户的账号与 IP（ADR-0007）。
//! 只在代码里限流而不把限到多少说出来，用户没有任何办法在跑之前判断这一趟安不安全。

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde::Serialize;

use crate::catalog::{Catalog, CatalogError};
use crate::report::{human_bytes, pad, thousands, width};

use super::priority::Priorities;
use super::{Field, MediaKind, Options, PlanCounts, online};

/// 一趟跑完之后，报告要的那几样「这一趟」的事实。
///
/// 捏成一个结构而不是四个参数：[`ScrapeReport::build`] 本来就已经拿着中立库、
/// 优先级表与选项三样了。
pub struct Run<'a> {
    /// 这一档有哪些源。
    pub sources: &'a [&'a str],
    /// 这一趟看了些什么。
    pub plan: &'a PlanCounts,
    /// 在线那一侧用掉了多少；离线档是 `None`。
    pub online: Option<online::Usage>,
    /// 这一趟停了没有，为什么。
    pub halted: Option<String>,
}

/// 在线那一侧的账。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct OnlineRow {
    /// 一共发了几个请求。
    pub requests: u64,
    /// 其中几个**条目查询**是「查过、没有」。**431 盯的就是这一类。**
    ///
    /// 下媒体时的 404 不算在内：那说的是「这份媒体没有」，不是「这个 ROM 我不认识」。
    pub not_found: u64,
    /// 几个 URL 被**取数闸门**拦下、没有发出去。正常是 0。
    pub refused: u64,
    /// 服务端说今天还剩几个请求；它没说就是 `None`。
    pub requests_left: Option<u64>,
    /// 服务端说今天还剩几个「未识别 ROM」请求。
    pub ko_left: Option<u64>,
    /// 服务端说这个账号可以开几个线程。
    ///
    /// **我们照样只开一个**（`concurrency`）。两个数并排报着是有用的：它让「我们比
    /// 服务端允许的还保守」成为看得见的事实，而不是一句注释。
    pub server_threads: Option<u64>,
    /// 并发上限。**恒为 1。**
    pub concurrency: usize,
}

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
    /// **有判据可以拿去发在线查询**的作品锚点几个；离线档不算这个数，是 `None`。
    pub queryable_works: Option<u64>,
    /// 未被识别确认、因此一个在线请求都不会为它发出去的变体有几个。
    pub unconfirmed_variants: u64,
    /// 在线那一侧的账；离线档是 `None`。
    pub online: Option<OnlineRow>,
    /// 这一趟停了没有，为什么。**不是错误**——已经采完的那部分留在中立库里。
    pub halted: Option<String>,
    /// 这个程序认得、而这一档没参加的源。
    ///
    /// 与 `unknown_sources` 是两件事：那个多半是打错字，这个是**换个档案就有**。
    /// 混成一件事，用户每跑一趟离线档都会看见一句「ScreenScraper 不存在」。
    pub idle_sources: Vec<String>,
    /// 有刮削结论的锚点：`(锚点种类, 个数)`。
    pub scraped: Vec<(String, u64)>,
    /// 按字段。
    pub fields: Vec<FieldRow>,
    /// **一个值都没采到的字段**。离线档跑的时候，这正是「在线档存在的理由」那一份清单。
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
        run: &Run<'_>,
    ) -> Result<Self, CatalogError> {
        let plan = run.plan;
        let mut report = Self {
            catalog: catalog.location().to_string(),
            pool: crate::path::display(&options.pool),
            profile: options.profile.label().to_string(),
            sources: run.sources.iter().map(|s| (*s).to_string()).collect(),
            works: plan.works,
            variants: plan.variants,
            local_media: plan.local_media,
            queryable_works: plan.queryable_works,
            unconfirmed_variants: plan.unconfirmed_variants,
            halted: run.halted.clone(),
            online: run.online.map(|usage| OnlineRow {
                requests: usage.requests,
                not_found: usage.not_found,
                refused: usage.refused,
                requests_left: usage.server.requests_left,
                ko_left: usage.server.ko_left,
                server_threads: usage.server.max_threads,
                concurrency: online::MAX_CONCURRENCY,
            }),
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
        // **打错字**与**这一档没参加**分开报：前者是要改的，后者换个档案就有。
        let known = super::all_source_names();
        report.unknown_sources = priorities.sources_not_in(&known);
        report.idle_sources = known
            .into_iter()
            .filter(|name| !run.sources.contains(name))
            .map(ToString::to_string)
            .collect();
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

        if let Some(online) = &self.online {
            heading(&mut out, "在线那一侧的账");
            let _ = writeln!(
                out,
                "请求            {} 个\n条目查询没命中    {} 个\
                 ——**431 盯的就是这一类**；一张图没有不算在内",
                thousands(online.requests),
                thousands(online.not_found),
            );
            let _ = writeln!(
                out,
                "并发            {}{}",
                online.concurrency,
                online
                    .server_threads
                    .map_or_else(String::new, |threads| format!(
                        "（服务端允许 {threads}——我们照样只开一个）"
                    )),
            );
            let 说 = |left: Option<u64>| left.map_or_else(|| "服务端没说".to_string(), thousands);
            let _ = writeln!(
                out,
                "服务端说的剩余  今日请求 {}、今日「未识别 ROM」{}\n\
                 （**一个数字都不写死**：三份官方文档给了三个不同的日配额，只有响应里的作数）",
                说(online.requests_left),
                说(online.ko_left),
            );
            if online.refused > 0 {
                let _ = writeln!(
                    out,
                    "**有 {} 个媒体 URL 被取数闸门拦下**——源给回来的地址在白名单之外，\
                     这是要看一眼的。",
                    thousands(online.refused)
                );
            }
            let _ = writeln!(
                out,
                "只对**已确认**的条目发请求：{} 个作品锚点拿得出撞过 DAT 的判据，\
                 一部作品只查一次；\n\
                 另有 {} 个变体没被识别确认，**一个请求都没有为它们发出去**——\
                 那些在 ScreenScraper 眼里是「未识别 ROM」，\n\
                 每问一次扣一份专门的配额，撞穿了连账号带 IP 一起封（ADR-0007）。",
                self.queryable_works
                    .map_or_else(|| "（没算）".to_string(), thousands),
                thousands(self.unconfirmed_variants),
            );
        }

        if let Some(halted) = &self.halted {
            heading(&mut out, "这一趟停了");
            let _ = writeln!(
                out,
                "{halted}\n已经采完的那部分**留在中立库里**，重跑会从这儿接着采。"
            );
        }

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
            heading(&mut out, "一个值都没采到的字段");
            let _ = writeln!(
                out,
                "{}——{}",
                self.gaps.join("、"),
                if self.online.is_some() {
                    "在线源这一趟也没给出这些：要么条目本身没有，要么请求还没轮到它们。"
                } else {
                    "本地数据源里没有这些东西。换 `--profile 在线` 跑一趟才补得上。"
                }
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
            heading(&mut out, "优先级表里点名了、而这个程序根本没有的源");
            let _ = writeln!(
                out,
                "{}——不是错误（列了不存在的源只是排序时永远轮不到它），\
                 但十有八九是打错了字，而打错的后果是静默的。",
                self.unknown_sources.join("、")
            );
        }

        if !self.idle_sources.is_empty() {
            heading(&mut out, "这一档没参加的源");
            let _ = writeln!(
                out,
                "{}——不是错误，换个策略档案（或者去掉 --no-media）它们就有了。",
                self.idle_sources.join("、")
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
