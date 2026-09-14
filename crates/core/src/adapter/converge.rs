//! **收敛**：把中立库折成一份份中立文档，也就是前端里的条目。
//!
//! `CONTEXT.md` 的**收敛**词条：导出时把同一作品的多个变体合并成前端里的一个条目。
//! 当前策略是作品级收敛——**一个条目、多个文件、默认启动首选变体**。
//!
//! ## 一个合集一个文件，收敛因此是按「作品 × 平台」
//!
//! Pegasus 有一条容易误解的语义：**一个 `game` 会被加入到该文件中此前定义过的所有
//! collection**（源码 `for (coll : ps.all_colls) sctx.game_add_to(...)`）。于是
//! **写文件的顺序本身就有语义**，两个合集写进同一个文件，里面每个游戏就同时属于两个。
//!
//! 唯一说得清的排法是**一个合集一个文件**（`*.metadata.pegasus.txt` 同目录可放多个，
//! 且同名合集跨文件自动合并）。合集这一层当前按**平台**切——这是前端里用户实际浏览的
//! 维度，也是 ES 的 system 到 Pegasus 的 collection 那条 1:1 映射（调研 C.6）。
//!
//! 平台一切开，收敛就只能是**作品 × 平台**：一部横跨 SFC 与 PSP 的作品在两个平台上
//! 各是一个条目。真库上 9,226 个作品里有 2,314 个横跨多个平台，所以这不是边角情况
//! （挂账 D63）。
//!
//! ## 首选变体：汉化 > 官中 > 日版 > 其他，可被裁决覆盖
//!
//! ADR-0012。它与**标题来源**是两条解耦的链路：即使默认启动的是民间汉化版，
//! 中文标题仍取官中版的官方译名——那一条由票 15 的 [`title::choose`]
//! 决定，这里一个字都不碰。
//!
//! 首选变体落到 Pegasus 上就是 **`files:` 列表里的第一条**。这个格式没有「默认文件」
//! 这个键（`m_game_attribs` 里没有），多文件条目由主题给用户挑，而列表第一条是它
//! 呈现的第一个。
//!
//! ## 什么不导出
//!
//! ADR-0013 与 ADR-0010：判据是**能否独立运行**。
//!
//! - **附属内容**（PSV 的 `addcont/`）：它是变体的**成员身份**，不是变体。导出只拿
//!   每个变体的主文件当 `file:`，所以它天然不会成为条目；真正要证的是「主文件本身
//!   是附属内容」的那种变体也挡得住。
//! - **非游戏资产**（模拟器 BIOS、街机的 device set）：判断在
//!   [`classify::non_game_asset`]，这里传变体的键去问它。真库上是 11 个
//!   （`街机/FBA-ROMS/BIOS/neogeo.zip`、`ps2/…/Bios/SCPH-10000.BIN`）。
//! - **补丁**，以及 Switch 容器的 TitleID 说出来的**附属内容**：判断在
//!   `identify::scope::standalone`（名字与 TitleID 两条依据，TitleID 优先），识别那一趟
//!   判一次、落进 `identification.standalone`，这里读那一列（ADR-0024 推论 3）。
//!   **不 parse 那句理由**——TitleID 判出来的补丁根本不走「跳过」，理由里一个字都没有。

use std::collections::{BTreeMap, BTreeSet};

use crate::catalog::identify::Standalone;
use crate::catalog::scrape::ScrapedValue;
use crate::catalog::{Catalog, CatalogError, ReleaseRow, VariantRow};
use crate::classify;
use crate::dat::chinese::ChineseMark;
use crate::path::file_name_of_key;
use crate::scrape::priority::{Priorities, Said};
use crate::scrape::{AnchorKind, Field};
use crate::shape::Role;
use crate::title::{self, Chosen, SortFrom};

use super::report::EXAMPLES;
use super::{Adapter, Body, Collection, Document, Entry, Game, ReleaseDate};

/// 一个变体为什么做不成前端条目。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NotAnEntry {
    /// **附属内容**：真正的追加内容，归属某个作品但不能独立运行（ADR-0013）。
    ExtraContent,
    /// **非游戏资产**：模拟器要它、但它本身不是游戏（ADR-0010）。
    NonGameAsset,
    /// **补丁**：把一个变体变换成另一个变体的指令文件，自己不可运行。
    Patch,
}

impl NotAnEntry {
    /// 报告里用的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::ExtraContent => "附属内容",
            Self::NonGameAsset => "非游戏资产",
            Self::Patch => "补丁",
        }
    }

    /// 报告里固定的排列顺序。
    #[must_use]
    pub fn all() -> [Self; 3] {
        [Self::ExtraContent, Self::NonGameAsset, Self::Patch]
    }
}

/// 一个变体凭什么当上**首选变体**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Preference {
    /// **裁决**：人指名的那一个。规则一律让路（ADR-0012）。
    Verdict,
    /// **汉化版**：民间补丁做出来的中文版本，是一个变体。
    FanTranslated,
    /// **官中版**：原厂发行的官方中文版本。
    OfficialChinese,
    /// **日版**。
    Japanese,
    /// 其余。
    Other,
}

impl Preference {
    /// 报告里用的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Verdict => "裁决",
            Self::FanTranslated => "汉化",
            Self::OfficialChinese => "官中",
            Self::Japanese => "日版",
            Self::Other => "其他",
        }
    }

    /// 报告里固定的排列顺序，**也就是优先级本身**。
    #[must_use]
    pub fn all() -> [Self; 5] {
        [
            Self::Verdict,
            Self::FanTranslated,
            Self::OfficialChinese,
            Self::Japanese,
            Self::Other,
        ]
    }
}

/// 收敛出来的一个合集：一个文件、一个合集段、若干条目。
#[derive(Debug, Clone, PartialEq)]
pub struct CollectionFile {
    /// 合集叫什么（当前是平台名）。
    pub collection: String,
    /// 这一份写成哪个**相对导出目录的路径**。
    ///
    /// 是路径不是文件名：ES-DE 那一套要 `gamelists/<平台目录>/gamelist.xml`，落点带目录。
    pub file_name: String,
    /// 里面的内容。
    pub doc: Document,
}

/// 收敛一遍的产物与它的账。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Converged {
    /// 一个合集一份。
    pub files: Vec<CollectionFile>,
    /// 库里一共几个变体。
    pub variants: u64,
    /// 收敛成了几个条目。
    pub entries: u64,
    /// 其中几个是**作品级**收敛出来的（识别认出了作品）。
    pub work_entries: u64,
    /// 其中几个是**一个变体一个条目**（识别还没认出作品）。
    pub loose_entries: u64,
    /// 真的被合进条目的变体有几个。
    pub exported_variants: u64,
    /// 被挡下的变体：理由 → 几个。
    pub excluded: BTreeMap<&'static str, u64>,
    /// 被挡下的例子：`(理由, 变体的键)`。
    pub excluded_examples: Vec<(&'static str, String)>,
    /// 首选变体各凭什么当上的：理由 → 几个条目。
    pub preferred: BTreeMap<&'static str, u64>,
    /// 库里有几个**附属内容**成员——它们入库，但一个都不导出（ADR-0013）。
    pub extra_content_members: u64,
    /// 多于一个变体、也就是**收敛真的起了作用**的条目有几个。
    pub converged_entries: u64,
}

impl Converged {
    fn count_excluded(&mut self, why: NotAnEntry, key: &str) {
        *self.excluded.entry(why.label()).or_insert(0) += 1;
        if self.excluded_examples.len() < EXAMPLES {
            self.excluded_examples.push((why.label(), key.to_string()));
        }
    }
}

/// **首选变体的键**写进哪个 `x-` 扩展键（写进文件时是 `x-romcat-variant`）。
///
/// 它是**机器读得动的那一半**：下次导入时靠它精确对回中立库里的那个变体，
/// 而不是拿标题去猜——标题在这个库里大量重名，净化之后更是多对一
/// （调研 D.4 第 2 条：主键用路径，不要用标题）。
pub const VARIANT_KEY: &str = "romcat-variant";

/// **作品名**写进哪个 `x-` 扩展键（写进文件时是 `x-romcat-work`）。
pub const WORK_KEY: &str = "romcat-work";

/// 首选变体的中文身份（`汉化` / `官中`）写进哪个 `x-` 扩展键。
pub const CHINESE_KEY: &str = "romcat-chinese";

/// **汉化组**写进哪个 `x-` 扩展键。
pub const GROUP_KEY: &str = "romcat-translation-group";

/// 导出的元数据文件在导出目录里叫什么。
///
/// **一个合集一个文件**，全部落在导出目录的根上——Pegasus 支持同目录多个
/// `*.metadata.pegasus.txt`，而 `file:` 是相对元数据文件所在目录解析的，
/// 于是文件里的路径就是中立库的键本身（相对主库根，ADR-0020），
/// 把这几份文件放到主库根下就直接生效。
///
/// 这是**默认**的摆法，不是唯一的：落点归适配器答（[`Adapter::metadata_path`]），
/// 因为它是格式的一部分——ES-DE 要的是 `gamelists/<平台目录>/gamelist.xml`。
#[must_use]
pub fn file_name_for(collection: &str, suffix: &str) -> String {
    format!("{}.{suffix}", safe_segment(collection))
}

/// 把一个合集名化成**一段安全的路径**：分隔符与保留字符换成 `_`。
///
/// 合集名里可能有路径分隔符（平台名不会，但清单是数据、用户能改），而它要么当文件名
/// 用、要么当目录名用（ES-DE 的 `gamelists/<平台目录>/`）——不换掉就能造出别处的文件。
#[must_use]
pub fn safe_segment(collection: &str) -> String {
    collection
        .chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect()
}

/// 把铺出来的**资源槽**填进条目：**取首选变体那一个的**。
///
/// 一个条目底下可能挂着好几个变体，而条目在前端里只有一张封面——取首选变体的，
/// 与「默认启动首选变体」是同一个选择（ADR-0012）。首选变体是哪一个，读的是
/// [`VARIANT_KEY`] 那一格（收敛时写进去的）。
///
/// **子库与导出走的是这同一处**（`sync::frontend::lay`、`transfer::export_task`）：
/// 两边各填一遍的话，「封面取哪个变体的」迟早长出两个答案（ADR-0024、挂账 `D64`）。
///
/// `assets` 是 [`Laid::assets`](crate::sync::media::Laid) 那一份：变体的键 → 资源槽 →
/// 相对路径。**靠文件名找媒体的格式这一份是空的**（ES-DE），于是一个字都不填。
pub fn attach_assets(
    doc: &mut Document,
    assets: &BTreeMap<String, BTreeMap<&'static str, Vec<String>>>,
) {
    for entry in &mut doc.entries {
        let Body::Game(game) = &mut entry.body else {
            continue;
        };
        let Some(head) = game.extra.get(VARIANT_KEY).and_then(|keys| keys.first()) else {
            continue;
        };
        let Some(slots) = assets.get(head) else {
            continue;
        };
        for (slot, paths) in slots {
            game.assets.insert((*slot).to_string(), paths.clone());
        }
    }
}

/// 把中立库收敛成一份份中立文档。
///
/// **不碰主库、不联网**：要的东西全在中立库里躺着（ADR-0001）。
///
/// # Errors
/// 读中立库失败时返回错误。
pub fn run(
    catalog: &Catalog,
    priorities: &Priorities,
    adapter: &dyn Adapter,
) -> Result<Converged, CatalogError> {
    run_within(catalog, priorities, adapter, None)
}

/// 只收敛这几个变体，别的一个都不进来。
///
/// **子库的元数据只该写卡上真有的那些游戏**：整库那一份写过去，前端会列出一堆
/// 点了打不开的条目。`only` 是 `None` 时收敛整个库，也就是 [`run`]。
///
/// 收敛的粒度不变（作品 × 平台）：一部作品在卡上只带了汉化版，卡上那个条目就只有
/// 汉化版这一个文件——而这正是用户挑变体而不是挑条目的理由（`sublibrary` 模块文档）。
///
/// # Errors
/// 读中立库失败时返回错误。
pub fn run_within(
    catalog: &Catalog,
    priorities: &Priorities,
    adapter: &dyn Adapter,
    only: Option<&BTreeSet<String>>,
) -> Result<Converged, CatalogError> {
    let layout = layout(catalog, only)?;
    // 票 15 挑出来的**显示标题**与**排序标题**。这一层一个字都不改它。
    let chosen = chosen_titles(catalog, priorities)?;

    let mut out = Converged {
        variants: layout.variants,
        extra_content_members: catalog
            .member_role_counts()?
            .get(Role::ExtraContent.code())
            .copied()
            .unwrap_or(0),
        ..Converged::default()
    };
    for (why, key) in &layout.excluded {
        out.count_excluded(*why, key);
    }

    for (platform, planned) in &layout.platforms {
        // 这个平台的内容住在哪个**平台目录**下。**从键上数出来，不从平台清单上猜**：
        // 清单里一个平台可以映射好几个目录别名（`FC` 收 `fc`/`nes`/`famicom`），
        // 而这里要的是「这份库里实际用的是哪一个」。散在多个目录里就交白卷——
        // 那时说不出唯一的那一个，路径整条原样写出去。
        //
        // 真库上 22 个平台各自都只用一个目录，而其中 **12 个的目录名与平台名对不上**
        // （`WII` 的目录叫 `Wii`、`PS1` 的叫 `ps`、`WS` 的叫 `wsc`）。ES 家族的
        // `es_systems.xml` 拿它当 `<name>`，拿平台名顶上去的话，那 12 个在 Android 与
        // Linux 上（大小写敏感）一个都指不着。
        let dirs: BTreeSet<&str> = planned
            .iter()
            .flat_map(|one| &one.members)
            .filter_map(|variant| crate::path::platform_of_key(&variant.key))
            .collect();
        let directory = match dirs.len() {
            1 => dirs.iter().next().map(|dir| (*dir).to_string()),
            _ => None,
        };
        // **不给 `shortname`。** Pegasus 拿它去对第三方资源目录（Skraper、ES 的
        // system 名），而那套名字与我们的平台名不是一回事——FC 在那边叫 `nes`。
        // 按平台名折一个 `fc` 出来，等于让前端去一个不存在的目录里找封面；
        // 更糟的是维护者自己写对了的那一行会被这个猜测覆盖掉。**猜不准就不写。**
        let mut entries = vec![Entry::new(Body::Collection(Collection {
            name: platform.clone(),
            directory: directory.clone(),
            ..Collection::default()
        }))];
        for one in planned {
            *out.preferred.entry(one.why.label()).or_insert(0) += 1;
            out.entries += 1;
            out.exported_variants += one.members.len() as u64;
            if one.members.len() > 1 {
                out.converged_entries += 1;
            }
            match one.anchor {
                Anchor::Work(_) => out.work_entries += 1,
                Anchor::Loose(_) => out.loose_entries += 1,
            }
            entries.push(Entry::new(Body::Game(build_game(
                &one.anchor,
                &one.members,
                platform,
                layout.values(one),
                layout.head_values(one),
                chosen.get(one.anchor.name()),
                priorities,
                one.why,
            ))));
        }
        out.files.push(CollectionFile {
            // **落点按平台目录，不按平台名。** ES-DE 的 `es_systems.xml` 里
            // `<name>` 就是那个目录名（`<path>%ROMPATH%/<name>`），gamelist 摆在
            // `gamelists/<name>/` 下。数不出唯一目录时退回平台名。
            file_name: adapter.metadata_path(directory.as_deref().unwrap_or(platform)),
            collection: platform.clone(),
            doc: Document { entries },
        });
    }

    Ok(out)
}

/// **收敛之前摆齐的料**：哪几个变体做得成条目、按「平台 × 作品（或还没认出作品的变体）」
/// 分好组、组里首选变体排在最前，外加刮削那一侧按锚点攒好的值。
///
/// **导出与「换一份优先级表之前哪几处显示值会变」共用这一份**（[`run_within`]、
/// [`scrape::priority::shifts`](crate::scrape::priority::shifts)）：哪些变体不导出、条目挂在
/// 作品上还是变体上、首选变体是谁、条目读哪几条值——两边各拼一次的话，「那一处算不算
/// 显示值」迟早长出两个答案（ADR-0024）。
pub(crate) struct Layout {
    /// 库里（或 `only` 圈出来的那批里）一共几个变体，连不导出的在内。
    pub(crate) variants: u64,
    /// 不导出的那些：理由与变体的键，照走到的次序。
    pub(crate) excluded: Vec<(NotAnEntry, String)>,
    /// 平台 → 那个平台上的条目，照条目挂的地方排（作品在前，没认出作品的变体在后）。
    pub(crate) platforms: BTreeMap<String, Vec<Planned>>,
    /// 作品锚点上的刮削值：作品名 → 那几条。
    by_work: BTreeMap<String, Vec<ScrapedValue>>,
    /// 变体锚点上的刮削值：变体的键 → 那几条。
    by_variant: BTreeMap<String, Vec<ScrapedValue>>,
}

/// 摆好的一个条目。
pub(crate) struct Planned {
    /// 挂在哪儿。
    pub(crate) anchor: Anchor,
    /// 名下的变体，**首选变体排在最前**。
    pub(crate) members: Vec<VariantRow>,
    /// 首选变体凭什么当上的。
    pub(crate) why: Preference,
}

impl Layout {
    /// 这个条目读哪几条值：认出了作品的读作品上的，没认出的读那个变体自己的。
    pub(crate) fn values(&self, planned: &Planned) -> &[ScrapedValue] {
        match &planned.anchor {
            Anchor::Work(work) => self.by_work.get(work),
            Anchor::Loose(key) => self.by_variant.get(key),
        }
        .map_or(&[], Vec::as_slice)
    }

    /// 首选变体自己身上的值。**汉化组**从这儿取：它挂在变体上（ADR-0012）。
    pub(crate) fn head_values(&self, planned: &Planned) -> &[ScrapedValue] {
        planned
            .members
            .first()
            .and_then(|head| self.by_variant.get(&head.key))
            .map_or(&[], Vec::as_slice)
    }
}

/// 每部作品的**显示标题**与**排序标题**（票 15 的 [`title::choose`]），键是作品名。
///
/// 与 [`Layout`] 一样，导出与「换一份优先级表之前哪几处显示值会变」共用。
pub(crate) fn chosen_titles(
    catalog: &Catalog,
    priorities: &Priorities,
) -> Result<BTreeMap<String, Chosen>, CatalogError> {
    Ok(title::work_titles(catalog)?
        .into_iter()
        .map(|set| {
            let picked = title::choose(&set, priorities);
            (set.work, picked)
        })
        .collect())
}

/// 摆料：见 [`Layout`]。`only` 给了就只摆这几个变体。
pub(crate) fn layout(
    catalog: &Catalog,
    only: Option<&BTreeSet<String>>,
) -> Result<Layout, CatalogError> {
    let variants: Vec<VariantRow> = match only {
        Some(only) => catalog
            .variants()?
            .into_iter()
            .filter(|variant| only.contains(&variant.key))
            .collect(),
        None => catalog.variants()?,
    };
    let works = catalog.work_names()?;
    let releases = catalog.releases()?;
    let overrides = catalog.preferred_variants()?;
    let abnormal = catalog.abnormal_main_members()?;

    // 识别那一趟判过的「自己不能独立运行」，读落了库的结论即可（ADR-0024 推论 3）。
    let not_standalone = catalog.not_standalone()?;

    // 变体上的中文记号：**汉化压过官中**（`dat::chinese::mark_of` 同一条纪律）。
    let mut marks: BTreeMap<String, BTreeSet<ChineseMark>> = BTreeMap::new();
    catalog.for_each_accepted_candidate(&mut |candidate| {
        if let Some(mark) = candidate.chinese {
            marks
                .entry(candidate.variant_key.to_string())
                .or_default()
                .insert(mark);
        }
    })?;

    // 刮削那一侧：作品锚点与变体锚点分开攒。
    let mut by_work: BTreeMap<String, Vec<crate::catalog::scrape::ScrapedValue>> = BTreeMap::new();
    let mut by_variant: BTreeMap<String, Vec<crate::catalog::scrape::ScrapedValue>> =
        BTreeMap::new();
    catalog.for_each_scraped_value(&mut |anchor, subject, value| {
        let bucket = if anchor == AnchorKind::Work.label() {
            &mut by_work
        } else {
            &mut by_variant
        };
        bucket.entry(subject.to_string()).or_default().push(value);
    })?;

    let mut out = Layout {
        variants: variants.len() as u64,
        excluded: Vec::new(),
        platforms: BTreeMap::new(),
        by_work,
        by_variant,
    };

    // 平台 → 作品名（或变体的键）→ 那几个变体。
    let mut grouped: BTreeMap<String, BTreeMap<Anchor, Vec<VariantRow>>> = BTreeMap::new();
    for variant in variants {
        if let Some(why) = excluded(&variant, &abnormal, &not_standalone) {
            out.excluded.push((why, variant.key));
            continue;
        }
        let platform = variant
            .platform
            .clone()
            .unwrap_or_else(|| crate::report::UNKNOWN_PLATFORM_LABEL.to_string());
        let anchor = match variant.work_id.and_then(|id| works.get(&id)) {
            Some(work) => Anchor::Work(work.clone()),
            None => Anchor::Loose(variant.key.clone()),
        };
        grouped
            .entry(platform)
            .or_default()
            .entry(anchor)
            .or_default()
            .push(variant);
    }

    for (platform, anchors) in grouped {
        let planned = anchors
            .into_iter()
            .map(|(anchor, mut members)| {
                // **首选变体排在最前。** 同一档之内按键排，同一份库跑两次结果必须一样。
                let picked = overrides.get(&(anchor.name().to_string(), platform.clone()));
                members.sort_by(|a, b| {
                    let rank = |v: &VariantRow| {
                        (preference_of(v, &marks, &releases, picked), v.key.clone())
                    };
                    rank(a).cmp(&rank(b))
                });
                let why = preference_of(&members[0], &marks, &releases, picked);
                Planned {
                    anchor,
                    members,
                    why,
                }
            })
            .collect();
        out.platforms.insert(platform, planned);
    }

    Ok(out)
}

/// 一个条目挂在哪儿：一个**作品**，还是一个还没认出作品的**变体**。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Anchor {
    /// 作品名。
    Work(String),
    /// 还没认出作品的那个变体的键。
    Loose(String),
}

impl Anchor {
    /// 作品名，或变体的键。
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Work(name) | Self::Loose(name) => name,
        }
    }
}

/// 这个变体做不成前端条目吗。
fn excluded(
    variant: &VariantRow,
    abnormal: &BTreeMap<String, String>,
    not_standalone: &BTreeMap<String, Standalone>,
) -> Option<NotAnEntry> {
    if abnormal.get(&variant.key).map(String::as_str) == Some(Role::ExtraContent.code()) {
        return Some(NotAnEntry::ExtraContent);
    }
    // 判断只有一处（ADR-0024），识别挑作品时问的也是它。这一侧是变体级，传变体的键。
    if classify::non_game_asset(&variant.key) {
        return Some(NotAnEntry::NonGameAsset);
    }
    // 补丁与附属内容：识别那一趟判过（`identify::scope::standalone`），读的是落了库的结论。
    // 「没有发行版链接」不在里面——同人移植与 homebrew 照样能跑，挡掉等于把用户的自制
    // 游戏从前端里抹掉。
    match not_standalone.get(&variant.key)? {
        Standalone::Runs => None,
        Standalone::Patch => Some(NotAnEntry::Patch),
        Standalone::ExtraContent => Some(NotAnEntry::ExtraContent),
    }
}

/// 这个变体凭什么当首选：汉化 > 官中 > 日版 > 其他，裁决压过全部。
fn preference_of(
    variant: &VariantRow,
    marks: &BTreeMap<String, BTreeSet<ChineseMark>>,
    releases: &BTreeMap<i64, ReleaseRow>,
    picked: Option<&String>,
) -> Preference {
    static NONE: BTreeSet<ChineseMark> = BTreeSet::new();
    preference_for(
        &variant.key,
        marks.get(&variant.key).unwrap_or(&NONE),
        variant.release_id.and_then(|id| releases.get(&id)),
        picked.map(String::as_str),
    )
}

/// 一个变体凭什么当上**首选变体**：汉化 > 官中 > 日版 > 其他，人裁过的一律让路。
///
/// **这条规则只有这一处实现。** 导出那一步走它（`preference_of`），库浏览的详情面板
/// 也走它（[`Catalog::variant_detail`](crate::catalog::Catalog::variant_detail)）——
/// 面板上排第一的那个，就得是同步到掌机上会默认启动的那个，两处各写一遍必然漂开。
///
/// 输入是**这一个变体自己的**记号与发行版，不是整份库的映射表：详情面板一次只看几个
/// 变体，为它整读一遍 38,963 条自动通过的候选是几十毫秒的卡顿。
#[must_use]
pub fn preference_for(
    key: &str,
    marks: &BTreeSet<ChineseMark>,
    release: Option<&ReleaseRow>,
    picked: Option<&str>,
) -> Preference {
    if picked == Some(key) {
        return Preference::Verdict;
    }
    // **汉化压过官中**：一个汉化版完全可能基于一条带中文语言标记的发行版，
    // 底版说什么语言不改变「这是民间汉化版」这件事（同 `dat::chinese::mark_of`）。
    if marks.contains(&ChineseMark::FanTranslated) {
        return Preference::FanTranslated;
    }
    if marks.contains(&ChineseMark::Official)
        || release.is_some_and(|release| {
            release
                .languages
                .as_deref()
                .is_some_and(has_chinese_language)
        })
    {
        return Preference::OfficialChinese;
    }
    if release.is_some_and(|release| {
        release
            .region
            .as_deref()
            .is_some_and(|region| region.eq_ignore_ascii_case("Japan"))
    }) {
        return Preference::Japanese;
    }
    Preference::Other
}

/// 语言标记组里有中文吗。`En,Zh-Hans` 与 `Ja,Zh` 都算。
fn has_chinese_language(languages: &str) -> bool {
    languages.split(',').any(|code| {
        let code = code.trim();
        code.eq_ignore_ascii_case("zh") || code.to_ascii_lowercase().starts_with("zh-")
    })
}

/// 一个条目上一个字段**写出去的是什么**：谁说的、说的哪几句。这个字段写出去是空的——
/// 或者退回兜底（标题退回文件名，那不是哪个源说的）——时是 `None`。
///
/// **「一个条目的这个字段显示哪个值」只有这一处**：折条目（`build_game`）照它写，换一份
/// 优先级表之前算「哪几处显示值会变」照它比
/// （[`scrape::priority::shifts`](crate::scrape::priority::shifts)，ADR-0024）。
///
/// - **标题**：认出了作品的，是标题集合挑出来的那一个（`chosen`，票 15）；没认出作品的，
///   照这张表挑一条。
/// - **汉化组**：挂在变体上（ADR-0012），从首选变体身上取（`head_values`）。
/// - **开发商、发行商、类型**：集合字段，胜出那个源的全部值（[`Priorities::pick_all`]）。
/// - **年份、简介**：挑一条（[`Priorities::pick`]）。
pub(crate) fn shown(
    field: Field,
    platform: &str,
    values: &[ScrapedValue],
    head_values: &[ScrapedValue],
    chosen: Option<&Chosen>,
    priorities: &Priorities,
) -> Option<Said> {
    let one = |value: &ScrapedValue| Said {
        source: Some(value.source.clone()),
        values: vec![value.value.clone()],
    };
    let label = field.label();
    match field {
        Field::Title => match chosen {
            Some(chosen) => Some(Said {
                source: chosen.source.clone(),
                values: vec![chosen.display.clone()],
            }),
            None => priorities.pick(label, Some(platform), values).map(one),
        },
        Field::TranslationGroup => priorities.pick(label, Some(platform), head_values).map(one),
        Field::Developer | Field::Publisher | Field::Genre => {
            let all = priorities.pick_all(label, Some(platform), values);
            let source = all.first()?.source.clone();
            Some(Said {
                source: Some(source),
                values: all.into_iter().map(|value| value.value.clone()).collect(),
            })
        }
        Field::Year | Field::Description => priorities.pick(label, Some(platform), values).map(one),
    }
}

/// 折出一个条目。
#[allow(clippy::too_many_arguments)]
fn build_game(
    anchor: &Anchor,
    members: &[VariantRow],
    platform: &str,
    values: &[crate::catalog::scrape::ScrapedValue],
    head_values: &[crate::catalog::scrape::ScrapedValue],
    chosen: Option<&Chosen>,
    priorities: &Priorities,
    why: Preference,
) -> Game {
    let head = &members[0];
    // **每个字段写出去的是什么只问 [`shown`]**：换一份优先级表之前算「哪几处显示值会变」
    // 问的也是它（ADR-0024）。
    let written = |field: Field| shown(field, platform, values, head_values, chosen, priorities);
    let one = |field: Field| written(field).and_then(|said| said.values.into_iter().next());

    // **显示标题**：作品级的由票 15 挑（中文优先、官中的官方译名优先）；
    // 还没认出作品的那些只有一条路——刮削收上来的变体级标题，兜底是文件名。
    //
    // **排序标题排不动就不写。** 中文按码位排等于乱排（`CONTEXT.md` 的**排序标题**
    // 词条），写一个中文的 `sort-by` 进去与不写是同一个效果——Pegasus 没有它就按标题排。
    // 而「与不写同效」的一行**不是无害的**：它会盖掉维护者自己写对了的那一行
    // （他给中文条目配的正是拉丁排序键）。折不出更好的就别写。
    let (title, sort) = match chosen {
        Some(chosen) => (
            chosen.display.clone(),
            (chosen.sort_from != SortFrom::None).then(|| chosen.sort.clone()),
        ),
        None => {
            let display =
                one(Field::Title).unwrap_or_else(|| file_name_of_key(anchor.name()).to_string());
            let sort = if title::sortable(&display) {
                Some(title::sort_title(&display))
            } else {
                // 显示标题排不动：另找一个排得动的叫法（同 `title::choose` 的做法）。
                let latin: Vec<_> = values
                    .iter()
                    .filter(|value| {
                        value.field == Field::Title.label() && title::sortable(&value.value)
                    })
                    .cloned()
                    .collect();
                priorities
                    .pick(Field::Title.label(), Some(platform), &latin)
                    .map(|value| title::sort_title(&value.value))
            };
            (display, sort)
        }
    };

    let mut extra = BTreeMap::new();
    // **中文这件事写进我们自己的扩展键，不写进 `tag:`。**
    //
    // `tag` 是维护者自己的词表——他可能拿它分「通关了的」「适合双人的」。我们往里塞
    // 一个 `汉化`，基线合并那一步就会把他整行标签换成我们这一行（那一层比的是整个
    // 列表，不是逐个标签）。而 `x-` 是格式**官方指定**给程序存私有数据的通道
    // （调研 D.3 第一级），主题侧照样读得到（`game.extra.*`），两边的词表互不打架。
    // 要不要把它提升成用户看得见的 `tag:`，挂在 D67。
    match why {
        Preference::FanTranslated | Preference::OfficialChinese => {
            extra.insert(CHINESE_KEY.to_string(), vec![why.label().to_string()]);
        }
        _ => {}
    }
    // **汉化组挂在变体上**（汉化版是变体不是发行版，ADR-0012），所以从首选变体那一侧取。
    if let Some(group) = one(Field::TranslationGroup) {
        extra.insert(GROUP_KEY.to_string(), vec![group]);
    }

    if let Anchor::Work(work) = anchor {
        extra.insert(WORK_KEY.to_string(), vec![work.clone()]);
    }
    // **首选变体是哪一个**写进去。它是机器读得动的那一半：下次导入时靠它精确对回
    // 中立库里的那个变体，而不是拿标题去猜（标题在净化后多对一，调研 D.4 第 2 条）。
    extra.insert(VARIANT_KEY.to_string(), vec![head.key.clone()]);

    Game {
        title,
        sort_title: sort,
        // **`file:` 剥掉根名。** 它是给前端看的路径，相对元数据文件所在目录解析
        // （`converge` 的模块文档），而根名是**中立库这一侧**的东西——写进去的话，
        // 主库导出的那份多一层不存在的目录，子库那份则与卡上的布局对不上
        // （`sync` 里落点也剥了同一段）。
        //
        // 机器读的那一半在 `x-romcat-variant` 里，那一格**带着根名**：它要精确对回
        // 中立库里的那个变体，而两块盘上同名的东西只靠相对路径分不开。
        files: members
            .iter()
            .map(|v| crate::path::relative_of_key(&v.main_key).to_string())
            .collect(),
        // **开发商、发行商与类型按集合读**：数据源一个键写了几家就是几家，挑一条等于换掉
        // 一家公司（`Priorities::pick_all` 的文档、挂单 Q27）。
        //
        // **类型同一条路**：维护者的 `genres:` 底下写了两行，导出只读得出一条的话，
        // 基线合并那一步会拿这一条去比他那两条、判成「变了」，再把整段续行重写成一行
        // ——与开发商那一处是同一个错，所以是同一个修法。
        developers: written(Field::Developer)
            .map(|said| said.values)
            .unwrap_or_default(),
        publishers: written(Field::Publisher)
            .map(|said| said.values)
            .unwrap_or_default(),
        genres: written(Field::Genre)
            .map(|said| said.values)
            .unwrap_or_default(),
        tags: Vec::new(),
        players: None,
        summary: None,
        description: one(Field::Description),
        release: one(Field::Year).and_then(|year| year.parse().ok().map(ReleaseDate::year_only)),
        rating: None,
        launch: None,
        workdir: None,
        assets: BTreeMap::new(),
        extra,
        unknown: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 语言标记组里的中文认得出() {
        assert!(has_chinese_language("En,Zh-Hans"));
        assert!(has_chinese_language("Ja,Zh"));
        assert!(!has_chinese_language("En,Ja"));
        // `Zhuang` 之类不是中文码。
        assert!(!has_chinese_language("Zhuang"));
    }

    #[test]
    fn 合集名里的路径分隔符造不出别处的文件() {
        assert_eq!(
            file_name_for("../etc", "metadata.pegasus.txt"),
            ".._etc.metadata.pegasus.txt"
        );
    }

    fn 变体(key: &str, release: Option<i64>) -> VariantRow {
        VariantRow {
            key: key.to_string(),
            platform: Some("FC".to_string()),
            rule: "裸文件".to_string(),
            main_key: key.to_string(),
            files: 1,
            bytes: 1024,
            unreadable_files: 0,
            manual: false,
            work_id: Some(1),
            release_id: release,
        }
    }

    #[test]
    fn 首选变体是汉化优先于官中优先于日版() {
        let mut marks = BTreeMap::new();
        marks.insert(
            "FC/汉化.zip".to_string(),
            BTreeSet::from([ChineseMark::FanTranslated]),
        );
        marks.insert(
            "FC/官中.zip".to_string(),
            BTreeSet::from([ChineseMark::Official]),
        );
        let mut releases = BTreeMap::new();
        releases.insert(
            9,
            ReleaseRow {
                id: 9,
                work_id: 1,
                platform: Some("FC".to_string()),
                region: Some("Japan".to_string()),
                serial: None,
                languages: Some("Ja".to_string()),
            },
        );
        let 日 = 变体("FC/日版.zip", Some(9));
        let 汉 = 变体("FC/汉化.zip", None);
        let 官 = 变体("FC/官中.zip", None);
        let 别 = 变体("FC/美版.zip", None);
        assert_eq!(
            preference_of(&汉, &marks, &releases, None),
            Preference::FanTranslated
        );
        assert_eq!(
            preference_of(&官, &marks, &releases, None),
            Preference::OfficialChinese
        );
        assert_eq!(
            preference_of(&日, &marks, &releases, None),
            Preference::Japanese
        );
        assert_eq!(
            preference_of(&别, &marks, &releases, None),
            Preference::Other
        );
        // 顺序就是这四档本身。
        assert!(Preference::FanTranslated < Preference::OfficialChinese);
        assert!(Preference::OfficialChinese < Preference::Japanese);
        assert!(Preference::Japanese < Preference::Other);
    }

    #[test]
    fn 裁决压过全部规则() {
        let marks = BTreeMap::from([(
            "FC/汉化.zip".to_string(),
            BTreeSet::from([ChineseMark::FanTranslated]),
        )]);
        let releases = BTreeMap::new();
        let 汉 = 变体("FC/汉化.zip", None);
        let 别 = 变体("FC/美版.zip", None);
        let 人说的 = "FC/美版.zip".to_string();
        assert_eq!(
            preference_of(&别, &marks, &releases, Some(&人说的)),
            Preference::Verdict
        );
        assert!(
            Preference::Verdict < preference_of(&汉, &marks, &releases, Some(&人说的)),
            "人指名的那个要排在汉化版前面"
        );
    }
}
