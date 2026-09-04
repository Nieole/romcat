//! **刮削报告**：这一趟到底补上了什么，以及**补不上什么**。
//!
//! 它和体检报告、命中率报告是同一个形状——**从中立库折出来，不重跑一遍刮削**
//! （ADR-0001）。因此盘不在位时上一趟的结论照样看得见。
//!
//! ## 缺口那一节不是装饰，可它一度在说假话
//!
//! 这一节原先写的是「离线档补不上简介、类型与开发商——换 `--profile 在线` 跑一趟才
//! 补得上」。**那三样全在本机那份数据里**：调研的原话是中文离线源那份 dump「不含
//! **图片**」，两个离线数据源各缺一样（这一份缺图、另一份缺简介），在报告里被并成了
//! 一句。代价是实打实的：用户被推去烧在线配额换英文简介，而中文简介就躺在本地。
//!
//! 现在如实说：**离线档补不上的是图**——本地数据源里一张图片都没有，那正是**在线档**
//! 唯一不可替代的地方（`CONTEXT.md` 的「中文离线源」词条）。空着的字段照旧点名，但
//! 不再把它们算在「离线档补不上」头上：简介、类型、开发商、发行商撞上一条中文条目
//! 就有（票 02–05），空着说的是**没撞上**或者**索引还没取**，不是这一档做不到。
//!
//! ## 中文离线源那一层到底值多少，报告要答得出
//!
//! 光说「补得上」还不够。报告因此多两节：**补上了哪几个字段、各多少条**（贡献与
//! **合并之后胜出**两个数并排——发行商那一栏 TOSEC 排在前面，中文离线源常常轮不到，
//! 只报贡献就是在替它邀功），以及**按平台的覆盖**。按平台那一栏是给「老平台补不上」
//! 一个交代：N64、DC 这些平台在数据源里条目数以几十计，那是**数据源本身浅**，
//! 不是匹配算法的锅——报告不点名的话，用户会怪错地方。
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
use crate::identify::fuzzy;
use crate::report::{UNKNOWN_PLATFORM_LABEL, heading, human_bytes, pad, share, thousands};

use super::priority::Priorities;
use super::{Field, MediaKind, Options, PlanCounts, online};

/// 文本报告里最多逐条列出几个**被截断的简介**。
///
/// 超出的只报个数——文本报告是给人扫一眼的，`--json` 那一份里名单是全的。
const TRUNCATED_SHOWN: usize = 20;

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

/// **中文离线源**那一层补上了什么：一个「源 × 字段」一行（票 06）。
///
/// 它与[按字段那一节](FieldRow)不重复：那一节是全库口径、一个字段一行，这一节盯的是
/// **这一层自己**——用户要判断的是「取全字段这件事到底值不值」，而那个判断要的是
/// 「它给了几条」与「其中几条真的胜出」两个数，不是全库的合计。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ZhFieldRow {
    /// 哪个源：中文名与作品级那几栏走 `中文离线源`，别名另占 `中文离线源·别名`。
    pub source: String,
    /// 字段。
    pub field: String,
    /// 落在几个锚点上。
    pub subjects: u64,
    /// 值一共几条。**它大于锚点数**：一个键写了几个值就拆成几条（票 04）。
    pub values: u64,
    /// **按优先级合并之后真正胜出的**有几个锚点。
    ///
    /// 与 `values` 分开报是这一节的要害：发行商那一栏 TOSEC 在场而且排在前面，
    /// 中文离线源只在 TOSEC 认不出那个文件时才轮得到（挂单 Q28）。只报贡献，
    /// 报告就是在替一个几乎不出场的源邀功。
    pub winners: u64,
}

/// **中文离线源**在一个平台上的覆盖（票 06）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ZhPlatformRow {
    /// 平台。认不出平台的那些归在「（平台未知）」底下。
    pub platform: String,
    /// 这个平台一共几个变体。**分母是库里的变体数**，不是这一趟采过的个数。
    pub variants: u64,
    /// 其中几个变体撞上了一条中文条目。
    pub matched: u64,
}

impl ZhPlatformRow {
    /// 这个平台的变体里，撞上中文条目的占多少。
    #[must_use]
    pub fn rate(&self) -> f64 {
        share(self.matched, self.variants)
    }
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
    /// **一个值都没采到的字段**。
    ///
    /// **它不等于「离线档补不上的东西」**（票 06）：简介、类型、开发商、发行商撞上一条
    /// 中文条目就有，空着说的是没撞上、或者索引还没取。离线档真正补不上的是**图**，
    /// 而图不在这份清单里——那一份是[媒体](Self::media)那一节的事。
    pub gaps: Vec<String>,
    /// **中文离线源补上了哪几个字段、各多少条**（票 06）。
    pub zh_fields: Vec<ZhFieldRow>,
    /// **中文离线源按平台的覆盖**（票 06），变体多的排前面。
    pub zh_platforms: Vec<ZhPlatformRow>,
    /// 上面那张表的合计行：全库几个变体、其中几个撞上了中文条目。
    ///
    /// 单独一格而不是让读的人自己去加：`--json` 那一份里「撞上的变体数」是要被引用的
    /// 一个数（规格的「真机验收」那一节点名要它），加出来的数与报告里印的数万一
    /// 对不上，谁也说不清哪个是真的。
    pub zh_total: ZhPlatformRow,
    /// **简介被截断了的那些锚点**：`(锚点种类, 锚点)`（票 03）。
    ///
    /// 数据源实测最长一条 9,962 字，超过 [`zh::DESCRIPTION_LIMIT`](super::zh::DESCRIPTION_LIMIT)
    /// 的那些会被截到闸上。**截断不许是悄悄发生的**：值里留着记号，报告在这儿把它们
    /// 逐条点出来，人要核对哪一条被砍了，照着这份名单就查得回去。
    pub truncated_descriptions: Vec<(String, String)>,
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
            // **中文离线源那一层单独记一笔**（票 06）：贡献与胜出并排，缺一个数这一节
            // 就会撒谎（挂单 Q28 的发行商那一栏）。
            if count.source == fuzzy::SOURCE || count.source == fuzzy::ALIAS_SOURCE {
                report.zh_fields.push(ZhFieldRow {
                    source: count.source.clone(),
                    field: count.field.clone(),
                    subjects: count.subjects,
                    values: count.values,
                    winners: winners
                        .get(&count.field)
                        .and_then(|by_source| by_source.get(&count.source))
                        .copied()
                        .unwrap_or(0),
                });
            }
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
        // 这一节按 `Field::all()` 那个固定顺序排，同一个字段的两个源挨着——同一份库
        // 出的报告每次长得一样，人才能对着上一次看差异（同上面那一段）。
        report.zh_fields.sort_by_key(|row| {
            (
                Field::from_label(&row.field).map_or(usize::MAX, |field| {
                    Field::all()
                        .into_iter()
                        .position(|it| it == field)
                        .unwrap_or(usize::MAX)
                }),
                row.field.clone(),
                row.source.clone(),
            )
        });
        // **按平台的覆盖**（票 06）：只数变体这一层——撞只发生在那儿，作品锚点跨平台，
        // 按平台归不动（`Catalog::source_by_platform`）。
        report.zh_platforms = catalog
            .source_by_platform(fuzzy::SOURCE, UNKNOWN_PLATFORM_LABEL)?
            .into_iter()
            .map(|coverage| ZhPlatformRow {
                platform: coverage.platform,
                variants: coverage.variants,
                matched: coverage.matched,
            })
            .collect();
        report.zh_total = ZhPlatformRow {
            platform: "合计".to_string(),
            variants: report.zh_platforms.iter().map(|row| row.variants).sum(),
            matched: report.zh_platforms.iter().map(|row| row.matched).sum(),
        };
        // 变体多的排前面：那正是「这一层还差多少」最该先看的顺序（同命中率报告）。
        report.zh_platforms.sort_by(|a, b| {
            b.variants
                .cmp(&a.variants)
                .then_with(|| a.platform.cmp(&b.platform))
        });
        // **被截断的简介**照旧从中立库折出来（ADR-0001）：不重跑一遍刮削，于是上一趟
        // 截掉的那些这一趟照样点得出名。
        report.truncated_descriptions =
            catalog.values_marked(Field::Description.label(), super::zh::TRUNCATED_MARK)?;

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

        self.render_chinese(&mut out);

        heading(&mut out, "缺口");
        if self.gaps.is_empty() {
            let _ = writeln!(out, "**一个值都没采到的字段**：没有，每个字段都有值。");
        } else {
            let _ = writeln!(out, "**一个值都没采到的字段**：{}。", self.gaps.join("、"));
            if self.online.is_some() {
                let _ = writeln!(
                    out,
                    "在线源这一趟也没给出这些：要么条目本身没有，要么请求还没轮到它们。"
                );
            } else {
                // **两拨分开说。** 中文离线源给得出的那几栏空着，去取一份索引是有用的；
                // 而年份与汉化组这一层根本不产出（`zh::FIELDS`）——对着它们说「跑一次
                // `romcat zh sync`」，是把人支去做一件永远不会有结果的事，与这一节
                // 原先那句假话是同一类。
                let (可补, 补不了): (Vec<&String>, Vec<&String>) =
                    self.gaps.iter().partition(|label| {
                        Field::from_label(label)
                            .is_some_and(|field| super::zh::FIELDS.contains(&field))
                    });
                if !可补.is_empty() {
                    let _ = writeln!(
                        out,
                        "**这不等于「离线档补不上」**：{}撞上一条中文条目就有（上一节）。\n\
                         空着说的是这些锚点名下的变体一个都没撞上，或者索引还没取\
                         ——那就先跑一次 `romcat zh sync`。",
                        可补.iter()
                            .map(|label| label.as_str())
                            .collect::<Vec<_>>()
                            .join("、"),
                    );
                }
                if !补不了.is_empty() {
                    let _ = writeln!(
                        out,
                        "{}**不在中文离线源的产出里**，取索引补不上它们：年份走 DAT 那几家\
                         与在线源，\n汉化组只有 TOSEC 的 `[tr zh <组>]` 说得出——\
                         那正是官方数据库补不上的那一块。",
                        补不了
                            .iter()
                            .map(|label| label.as_str())
                            .collect::<Vec<_>>()
                            .join("、"),
                    );
                }
            }
        }
        // **这一段与 gaps 空不空无关，照打。** 「离线档补不上什么」是这一节存在的理由，
        // 而它恰恰不在 `gaps` 那份清单里：图不是字段（票 06）。
        let _ = writeln!(
            out,
            "{}",
            if self.online.is_some() {
                "**在线档补的就是图**：封面、截图、视频——本地数据源里一张图片都没有。\n\
                 文字那几栏离线档自己都有，而且是**中文**的那一份；在线源那边的简介是\
                 英文的，还要赌上账号与 IP（ADR-0007）。"
            } else {
                "**离线档补不上的是图**：本地数据源里一张图片都没有，中文离线源那份 dump \
                 也不含图。\n这正是在线档存在的理由——`--profile 在线` 补的是封面、截图\
                 与视频，别的字段离线档自己都有。\n这一趟的**网络请求数是 0**：离线档只收\
                 本地源，混进一个联网源会当场被拒——闸门查的是每个源自报的本地还是联网，\
                 不是参数表长什么样。"
            }
        );

        if !self.truncated_descriptions.is_empty() {
            heading(&mut out, "被截断的简介");
            let _ = writeln!(
                out,
                "{} 个锚点的简介超过了 {} 字这道闸，落库的是前 {} 字，末尾留着一句\
                 说明——**前端里读到的那一段不会无缘无故地断在半路**。",
                thousands(u64::try_from(self.truncated_descriptions.len()).unwrap_or(u64::MAX)),
                thousands(super::zh::DESCRIPTION_LIMIT as u64),
                thousands(super::zh::DESCRIPTION_LIMIT as u64),
            );
            for (anchor, subject) in self.truncated_descriptions.iter().take(TRUNCATED_SHOWN) {
                let _ = writeln!(out, "  {}{subject}", pad(anchor, 6));
            }
            if self.truncated_descriptions.len() > TRUNCATED_SHOWN {
                let _ = writeln!(
                    out,
                    "  ……另有 {} 个（整份清单在 `--json` 那一份里）",
                    thousands(
                        u64::try_from(self.truncated_descriptions.len() - TRUNCATED_SHOWN)
                            .unwrap_or(u64::MAX)
                    ),
                );
            }
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

    /// **中文离线源那一层**的两节：补上了哪几个字段各多少条、按平台的覆盖（票 06）。
    ///
    /// 单独一个函数是因为它自成一段话：上一节答的是「全库这一趟有什么」，这两节答的是
    /// 「**取全字段这件事值多少**」，而那正是这一整批票要交代的东西。
    fn render_chinese(&self, out: &mut String) {
        heading(out, "中文离线源补上了什么");
        if self.zh_fields.is_empty() {
            let _ = writeln!(
                out,
                "库里一条都没有。这一层撞的是文件名剥出来的**正题**，先跑一次 \
                 `romcat zh sync` 取回索引它才参加；\n\
                 取过了还是空，那就是这些变体的正题一个都没撞上那份数据——\
                 **撞不上就一个字段都不产出**，不猜。"
            );
        } else {
            let _ = writeln!(
                out,
                "{}{}{}{}合并之后胜出",
                pad("源", 16),
                pad("字段", 10),
                pad("锚点", 9),
                pad("值", 9),
            );
            for row in &self.zh_fields {
                let _ = writeln!(
                    out,
                    "{}{}{}{}{}",
                    pad(&row.source, 16),
                    pad(&row.field, 10),
                    pad(&thousands(row.subjects), 9),
                    pad(&thousands(row.values), 9),
                    thousands(row.winners),
                );
            }
            let _ = writeln!(
                out,
                "（**贡献与胜出是两回事**：发行商那一栏 TOSEC 在场而且排在优先级链的前面，\
                 中文离线源\n\
                 只在 TOSEC 认不出那个文件时才轮得到——只报贡献，等于替一个几乎不出场的\
                 源邀功）"
            );
            let _ = writeln!(
                out,
                "（值多于锚点是对的：`|开发= 甲、乙` 拆成两条，别名一条条目能给好几个）"
            );
            let _ = writeln!(
                out,
                "（这一层的结论**全部离线**，一个网络请求都不发，也不扣任何在线配额；\n\
                 它是模糊匹配来的**中置信**结论，照旧进待确认队列）"
            );
        }

        // 一条都没撞上时不摆这张表：几十行全零说不出任何事，只会把报告冲长。
        if self.zh_total.matched == 0 {
            return;
        }
        heading(out, "中文离线源按平台的覆盖");
        let _ = writeln!(
            out,
            "{}{}{}覆盖",
            pad("平台", 10),
            pad("变体", 9),
            pad("撞上", 9),
        );
        for row in self
            .zh_platforms
            .iter()
            .chain(std::iter::once(&self.zh_total))
        {
            let _ = writeln!(
                out,
                "{}{}{}{:.1}%",
                pad(&row.platform, 10),
                pad(&thousands(row.variants), 9),
                pad(&thousands(row.matched), 9),
                row.rate(),
            );
        }
        let _ = writeln!(
            out,
            "（分母是**库里这个平台的变体数**，不是这一趟采过的个数；只数变体这一层——\
             撞只发生在那儿，\n作品那一层是顺着名下变体撞到的条目号推上去的）"
        );
        let _ = writeln!(
            out,
            "（**老平台覆盖低多半是数据源本身浅**，不是匹配算法的锅：N64、DC 这些平台在\
             那份数据里条目数以几十计。\n\
             要判断是哪一种，拿这一列与命中率报告里同一个平台的变体数对着看）"
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 一份只填了缺口那几栏的报告——这一节的渲染不依赖库里别的东西。
    fn 只有缺口(gaps: &[&str]) -> ScrapeReport {
        ScrapeReport {
            gaps: gaps.iter().map(|it| (*it).to_string()).collect(),
            // 字段一个都没有时 `render_text` 会早早收尾，所以摆一行进去。
            fields: vec![FieldRow {
                field: Field::Title.label().to_string(),
                ..FieldRow::default()
            }],
            ..ScrapeReport::default()
        }
    }

    #[test]
    fn 中文离线源产出不了的那几栏不许被推去取索引() {
        // 「跑一次 `romcat zh sync`」只对这一层真的产出的那几栏成立（`zh::FIELDS`）。
        // **年份与汉化组这一层根本不产出**：年份走 DAT 那几家与在线源，汉化组只有
        // TOSEC 的 `[tr zh]`。对着它们叫用户去取 435 MB 的索引，是把人支去做一件
        // 永远不会有结果的事——与这一节原先那句假话是同一类，只是指向反了。
        let text = 只有缺口(&["年份", "汉化组"]).render_text();
        assert!(text.contains("**不在中文离线源的产出里**"), "{text}");
        // 缺口这一节里那句指路的话不许出现（上一节那句「先跑一次 `romcat zh sync`
        // 取回索引它才参加」说的是另一件事——那一层这一趟一条都没有）。
        assert!(
            !text.contains("**这不等于「离线档补不上」**"),
            "这两栏取索引也补不上，不该拿那句话指路：\n{text}"
        );
        assert!(
            !text.contains("空着说的是这些锚点名下的变体一个都没撞上"),
            "这两栏空着与撞不撞得上无关：\n{text}"
        );

        // 给得出的那几栏照旧指路。
        let text = 只有缺口(&["简介", "开发商"]).render_text();
        assert!(text.contains("romcat zh sync"), "{text}");
        assert!(text.contains("**这不等于「离线档补不上」**"), "{text}");
        assert!(!text.contains("**不在中文离线源的产出里**"), "{text}");

        // 两拨都有时**两句都说**，各点各的名。
        let text = 只有缺口(&["简介", "汉化组"]).render_text();
        assert!(text.contains("简介撞上一条中文条目就有"), "{text}");
        assert!(text.contains("汉化组**不在中文离线源的产出里**"), "{text}");
    }
}
