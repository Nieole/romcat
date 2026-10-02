//! **合并作品**向导与**移出此作品**弹层（票 `gui-looks-like-the-design/16`，设计稿 `renderMW` 与 `DLG.split`）。
//!
//! ## 界面只画和转发
//!
//! **一条领域判断都不在这儿**（ADR-0024）：落几条裁决、每条裁决说什么、哪几个字段有冲突、
//! 前端条目从多少变多少、哪几台子库要重排差量预览，全问核心库那一处
//! （[`romcat_core::triage::merge`]）。这一层管的是三步怎么走、勾了哪几个、每个平台挑了谁。
//!
//! ## 它不新造机制
//!
//! 落成的是一**批裁决**，与待确认队列里「手工指定作品」同一种，因此**撤销走既有那条
//! 按批撤销的路**（待确认屏的**裁决记录**）。屏上那句「会发生什么」因此要说清
//! **撤得回哪几样**：归属撤得回来，别名、字段选值与首选变体是另外三笔中立库的账，
//! 撤销不连带（挂单 `Q1012`）。
//!
//! ## 「以后扫描到的也自动归入」画成不可选
//!
//! 2026-09-13 裁定暂缓：它要在**沉淀库**新增一条作品级的「A 与 B 是同一作品」记录。
//! 那句为什么由核心库答（[`merge::AUTO_ABSORB_REASON`]），这一层只印出来。

use std::collections::{BTreeMap, BTreeSet};

use romcat_core::catalog::browse::{
    PlatformFilter, WorkAnchor, WorkDetail, WorkQuery, WorkVariant,
};
use romcat_core::catalog::identify::Tier;
use romcat_core::catalog::{Catalog, CatalogError};
use romcat_core::filename::Rules;
use romcat_core::report::{human_bytes, thousands};
use romcat_core::scrape::{Field, Priorities};
use romcat_core::site::Site;
use romcat_core::title::{self, TitleSet};
use romcat_core::triage::merge::{self, Kind};
use romcat_core::triage::same_work::Suspicion;

use crate::dialog::{Button, Dialog, Footer, Width};
use crate::font;
use crate::look;
use crate::media::Shelf;
use crate::table;
use crate::tokens::Tokens;

/// 浏览屏工具条上那颗按钮上的字（设计稿 `#merge-btn`）。
pub const MERGE: &str = "合并作品…";

/// 作品详情页头上那一排里那颗按钮上的字（设计稿 `renderWD` 的 `data-mw="open:"`）。
pub const MERGE_ONE: &str = "合并…";

/// 变体卡头一行右头那颗按钮上的字（设计稿 `data-dg="open:split|…"`）。
pub const SPLIT: &str = "移出此作品…";

/// 勾得不够时说的那一句（设计稿 `#merge-btn` 那条 toast）。
pub const NEED_TWO: &str = "请勾选两个或更多作品，或在作品详情中点「合并…」";

/// 取消勾选的那几行上，「首选」那颗单选钮为什么按不动。
///
/// 与 [`merge::ALREADY_HELD`] 同一条规矩（ADR-0005 再修订那一节）：**屏上常驻着这条理由**
/// ——那一行整行调淡、勾选框空着（照稿 `.vrow.off{opacity:.5}`），停到那颗上说的也是这一句。
const NO_PREFER: &str = "取消勾选的变体不归入，也就没有首选可挑";

/// 三步的名字，照稿（`steps`）。
const STEPS: [&str; 3] = ["选择作品", "核对变体", "确认合并"];

/// 两处搜索框（第一步「添加其他作品」、移出那一层「移入另一个作品」）**搜了**之后最多列几行（设计稿 `slice(0,q?8:…)`）。
const FOUND: usize = 8;

/// 第一步「添加其他作品」**没搜时**列几行（设计稿 `mwResults` 的 `slice(0,q?8:4)`，差距清单 `M-05`）。
/// 框最高 `search-list-max`、再多框里滚，少列几行不碍事；要别的就搜。
const PICK_BROWSED: usize = 4;

/// 移出那一层「移入另一个作品」**没搜时**列几行（设计稿 `DLG.split` 的 `slice(0,q?8:5)`）。
const SPLIT_BROWSED: usize = 5;

/// 框里打的是 `text` 时最多列几行：搜了 [`FOUND`]，没搜 `browsed`（空串、只有空白都算没搜，与 `WorkQuery::search` 同一条）。
fn 列几行(text: &str, browsed: usize) -> usize {
    if text.trim().is_empty() {
        browsed
    } else {
        FOUND
    }
}

/// 一格值最多印多少个字（设计稿第三步 `esc(r.kv).slice(0,80)`，那是拉丁字母的 80）：
/// 简介在中立库里最长 4,000 字，整段塞进那一格会把三栏撑塌。
///
/// **取的是汉字数**：这一层最宽 840 点、三栏分下来一栏三百出头，半号字的汉字一行摆得下
/// 二十几个。剪掉的那一截**停在指针上说得出来**（`on_hover_text` 交的是整句）。
const CELL: usize = 26;

/// 参与这一趟的**一行**：浏览表上的一行（一个**作品**，或一个还没认出作品的**变体**），
/// 连它底下那几个变体。
///
/// 叫「行」而不另起名字：屏上人勾的就是表上那几行，核心库那一侧认的也是
/// [`WorkAnchor`]（`WorkRow::anchor`）。
#[derive(Debug, Clone)]
struct Row {
    /// 表上那一行的身份。
    anchor: WorkAnchor,
    /// **作品名**——核心库那一侧认的是它；还没认出作品的那一行是 `None`，
    /// 它只能当被合并的一侧（保留的那一侧必须说得出作品名，不然没有名字可留作别名）。
    work: Option<String>,
    /// 屏上那个名字：认出作品的是**显示标题**（与作品详情页大标题同一个），认不出的是正题（[`title_of`]）。
    title: String,
    /// 头一个平台（没有封面时那张小字卡上的水印印它，与作品详情页头上那张同一个取法）；一个都说不出是空串。
    platform: String,
    /// 名字底下那一句副标题：与作品详情页头上**同一句**（`work::subtitle_line`）；一条别的叫法都没有时是 `None`。
    subtitle: Option<String>,
    /// 年份；一条都没刮到时是 `None`（屏上照词表那句写「年份未知」）。
    year: Option<String>,
    /// 底下那些变体里**最高的那档置信度**（与表上那一行同一条口径，`WorkRow::confidence`）。
    tier: Tier,
    /// 底下那几个变体（**照当前筛选**展开，与屏上那一行写着的变体数同一个数）。
    variants: Vec<RowVariant>,
}

/// 一行底下的一个变体。
#[derive(Debug, Clone)]
struct RowVariant {
    /// 变体的键。
    key: String,
    /// **变体简称**（核心库 `Catalog::variant_short_names` 拼的）；拼不出时是文件名。
    short: String,
    /// 落在哪个平台上；说不出时是「平台未知」那一档的词。
    platform: String,
    /// 置信度那一档。
    tier: Tier,
    /// 多大。
    bytes: u64,
}

impl Row {
    /// 读一行：名字、底下那几个变体。
    fn load(
        catalog: &Catalog,
        query: &WorkQuery,
        rules: &Rules,
        priorities: &Priorities,
        anchor: &WorkAnchor,
    ) -> Result<Option<Self>, CatalogError> {
        let Some(detail) = catalog.work_detail(query, anchor)? else {
            return Ok(None);
        };
        let shorts = catalog.variant_short_names(&detail, priorities)?;
        let work = match anchor {
            WorkAnchor::Work(_) => Some(detail.name.clone()),
            WorkAnchor::Loose(_) => None,
        };
        // 粗体写显示标题，不写作品名（拿主意的人 2026-10-01 裁：照稿 `.mwit` 的 `<b>${w.t}</b>`；底下那一句去掉的正是显示标题）。
        let (title, also_known_as) = 屏上的名字(catalog, &detail, rules, priorities)?;
        Ok(Some(Self {
            anchor: anchor.clone(),
            subtitle: super::work::subtitle_line(work.is_none(), also_known_as.as_ref()),
            platform: detail.platforms.first().cloned().unwrap_or_default(),
            work,
            title,
            year: detail.year.clone(),
            // **哪一档由核心库答**（`WorkDetail::confidence`，与表上那一行同一条口径）：
            // 界面不自己 match 一遍候选（ADR-0024）。一条候选都没有时 `Tier::of` 把它
            // 折成「没有候选」那一档。
            tier: Tier::of(detail.confidence()),
            variants: detail
                .variants
                .iter()
                .zip(shorts)
                .map(|(variant, short)| RowVariant {
                    key: variant.row.key.clone(),
                    short,
                    platform: variant
                        .row
                        .platform
                        .clone()
                        .unwrap_or_else(|| romcat_core::report::UNKNOWN_PLATFORM_LABEL.to_string()),
                    tier: Tier::of(variant.confidence()),
                    bytes: variant.row.bytes,
                })
                .collect(),
        }))
    }
}

impl Row {
    /// 搜出来、建议出来的那一行：**不过当前筛选**（要核对的是那个作品全部的变体）。
    fn load_whole(
        catalog: &Catalog,
        priorities: &Priorities,
        anchor: &WorkAnchor,
    ) -> Result<Option<Self>, CatalogError> {
        Self::load(
            catalog,
            &WorkQuery::default(),
            &Rules::builtin(),
            priorities,
            anchor,
        )
    }
}

/// 认不出作品的那一行叫什么：它的**正题**（与表上那一行主栏同一处剥）；万一剥不出来退回那一行的名字。
/// 认出作品的那一行屏上写显示标题（[`屏上的名字`]）。
fn title_of(detail: &WorkDetail, rules: &Rules) -> String {
    detail.title(rules).unwrap_or_else(|| detail.name.clone())
}

/// 一行**屏上叫什么、还叫什么**：认出作品的从它的标题集合挑——显示标题（`title::choose`）与头上那一句要的别的叫法
/// （`title::also_known_as`），与作品详情页大标题、头上那一句是同两个函数；认不出的是正题（[`title_of`]），没有别的叫法。
///
/// 合并向导三步与「移出此作品」那一层提到一个作品时都写这个名字（照稿：`renderMW` 与 `DLG.split` 里一律是 `w.t`）；
/// 落裁决、留别名认的照旧是作品名。
fn 屏上的名字(
    catalog: &Catalog,
    detail: &WorkDetail,
    rules: &Rules,
    priorities: &Priorities,
) -> Result<(String, Option<title::AlsoKnownAs>), CatalogError> {
    match detail.anchor {
        WorkAnchor::Work(_) => {
            let set = TitleSet {
                work: detail.name.clone(),
                entries: catalog.titles_of(&detail.name)?,
            };
            Ok((
                title::choose(&set, priorities).display,
                Some(title::also_known_as(&set, priorities)),
            ))
        }
        WorkAnchor::Loose(_) => Ok((title_of(detail, rules), None)),
    }
}

/// 页脚上按下去的是哪一颗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pressed {
    Cancel,
    Back,
    Next,
    Go,
}

/// 人在第二步上**亲手点下**的一条首选变体裁决：这个作品在这个平台上默认启动这一个。
///
/// 捏成一个结构而不是三个 `String` 的元组：三样本来就结伴出生、结伴落库，
/// 而三个同型的 `String` 挨在一起，调用处写反了编译器不会说话。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preferred {
    /// 哪个作品（合并之后保留的那个）。
    pub work: String,
    /// 哪个平台。
    pub platform: String,
    /// 默认启动哪个变体。
    pub variant_key: String,
}

/// 画完这一帧，摆它的那一屏要办的事。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Done {
    /// 关掉这一层。
    Close,
    /// 落下去：这一层已经把计划排好了。
    Apply,
}

/// **合并作品向导**开着时记着的东西。
#[derive(Debug, Clone)]
pub struct Wizard {
    /// 走到第几步（从 0 数）。
    step: usize,
    /// 参与的那几行。
    rows: Vec<Row>,
    /// 其中第几个是**保留的作品**。
    keep: usize,
    /// **取消勾选**的那几个变体：它们不归入，仍留在原来的作品里。
    excluded: BTreeSet<String>,
    /// **人真的点过**的首选变体：平台 → 变体的键。
    ///
    /// ⚠️ **只装人点过的那几个**。屏上默认选中的那一个由核心库照首选变体那条规则算
    /// （[`merge::preferred_pick`]，存在 [`Self::defaults`] 里），**人没动过就一个字都不写**
    /// ——把规则算出来的那一个写成「首选变体裁决」等于把一条规则冻成一条覆盖，而词表
    /// **首选变体**说的是规则「可被裁决覆盖」，不是反过来（两轴审查各挑出一次，
    /// 2026-09-21 照改）。
    picked_preferred: BTreeMap<String, String>,
    /// 每个平台**照规则**该选哪一个（核心库 [`merge::preferred_pick`] 算的）：平台 → 变体的键。
    /// 屏上那颗单选钮默认落在它身上；它**不落库**。
    defaults: BTreeMap<String, String>,
    /// [`Self::defaults`] 过期了吗（换了保留的作品、勾掉过变体）。真时下一次画第二步重算。
    defaults_stale: bool,
    /// 第一步「添加其他作品」框里打的字。
    query: String,
    /// 搜出来的那几行（不含已经在里头的）。
    found: Vec<Row>,
    /// `found` 是照哪几个字搜的。与框里的字不一样就重搜。
    found_for: Option<String>,
    /// 「建议一并合并」那几行（[`Suggested`]）。
    suggested: Vec<Suggested>,
    /// `suggested` 是照哪几个作品名算的（参与的那几行认出了作品的）。与眼下的不一样就重算——它要问库，不每帧算。
    suggested_for: Option<Vec<String>>,
    /// 把被合并作品的名称保留为**别名**。
    alias: bool,
    /// 第三步：有冲突的那几个字段。
    conflicts: Vec<merge::Conflict>,
    /// 逐项选了谁：字段 → 选的是哪一格。
    picks: BTreeMap<Field, Pick>,
    /// 第三步那份账（按「下一步」进第三步那一下算一次，**不每帧算**）。
    impact: Option<merge::Impact>,
    /// 排好的计划。
    planned: Option<merge::Regrouping>,
    /// 这一层自己那句错。
    error: Option<String>,
}

/// 第三步一个字段上**选的是哪一格**。
///
/// 一个枚举而不是一个哨兵值（原先是 `usize::MAX`）：`pick_of` / `adopted` / 画那张表三处
/// 都得记着那个约定，而枚举自己说得出它是什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pick {
    /// 保留作品那一格。
    Keep,
    /// 别的作品里的第几个（[`merge::Conflict::others`] 的下标）。
    Other(usize),
}

/// 第一步「建议一并合并」那一行（设计稿 `renderMW` 的 `extra`）：参与的作品里有一个与它是**疑似同一作品**、
/// 而它还没在向导里。
///
/// **哪两个作品疑似同一个、凭什么，全由核心库答**（`same_work::survey`，浏览屏缓着那一份，ADR-0024）：
/// 这一层只挑出「一边在里头、一边不在」的那几对，印它的名字与头一条理由。
#[derive(Debug, Clone)]
struct Suggested {
    /// 向导外的那一个，读成与参与的那几行同一个样子：按「添加」就原样进向导。
    row: Row,
    /// 核心库给的头一条理由（`Suspicion::reasons`），逐字。
    reason: String,
}

impl Wizard {
    /// 从浏览屏勾中的那几行、或者从作品详情页那一个作品开一层。
    ///
    /// 读不出来的行（筛掉了、库重扫过了）悄悄略过——第一步自己会说「至少需要两个作品」。
    pub fn open(
        catalog: &Catalog,
        query: &WorkQuery,
        rules: &Rules,
        priorities: &Priorities,
        anchors: &[WorkAnchor],
    ) -> Self {
        let mut rows = Vec::new();
        let mut error = None;
        for anchor in anchors {
            match Row::load(catalog, query, rules, priorities, anchor) {
                Ok(Some(row)) => rows.push(row),
                Ok(None) => {}
                Err(failed) => error = Some(format!("中立库读不动：{failed}")),
            }
        }
        // **默认保留变体最多的那一个**（拿主意的人 2026-09-21 定）：合并的本意就是把少的
        // 并进多的，而这句话屏上解释得清。**稿上那套四项加权分不照抄**
        // （`bestOf`：置信度×2 + 有封面 + 元数据完整 + 变体数×0.1）——它是一条会落进
        // 界面里的新领域判断，而且屏上解释不了「凭什么推荐这一个」。
        // 同数时按屏上那个名字定序，于是同一批行开两次向导，默认推荐的是同一个。
        let keep = best_keep(&rows);
        Self {
            step: 0,
            rows,
            keep,
            excluded: BTreeSet::new(),
            picked_preferred: BTreeMap::new(),
            defaults: BTreeMap::new(),
            defaults_stale: true,
            query: String::new(),
            found: Vec::new(),
            found_for: None,
            suggested: Vec::new(),
            suggested_for: None,
            alias: true,
            conflicts: Vec::new(),
            picks: BTreeMap::new(),
            impact: None,
            planned: None,
            error,
        }
    }

    /// 排好的那份计划；还没走到第三步时是 `None`。
    #[must_use]
    pub fn planned(&self) -> Option<&merge::Regrouping> {
        self.planned.as_ref()
    }

    /// 合并之后该留作**别名**的那几个名字（[`Impact::emptied`](merge::Impact::emptied) 那几个），
    /// 没勾「保留为别名」时是空的。
    #[must_use]
    pub fn aliases(&self) -> Vec<String> {
        if !self.alias {
            return Vec::new();
        }
        self.impact
            .as_ref()
            .map(|impact| impact.emptied.clone())
            .unwrap_or_default()
    }

    /// 第三步逐项选中的那几格：`(字段, 选了谁说的)`。选了保留作品自己那一格的不在里面。
    #[must_use]
    pub fn adopted(&self) -> Vec<(Field, merge::Offer)> {
        let mut out = Vec::new();
        for conflict in &self.conflicts {
            let Pick::Other(at) = self.pick_of(conflict) else {
                continue;
            };
            if let Some(offer) = conflict.others.get(at) {
                out.push((conflict.field, offer.clone()));
            }
        }
        out
    }

    /// 第二步里**人真的点过**的首选变体，一条一格。
    ///
    /// **人没点过的平台一条都不在里面**：屏上那颗默认落在核心库照规则算出来的那一个身上
    /// （`Wizard` 里那份 `defaults`），把它也写下去等于拿一条裁决把规则冻住。
    #[must_use]
    pub fn preferred(&self) -> Vec<Preferred> {
        let Some(work) = self.keep_work() else {
            return Vec::new();
        };
        self.picked_preferred
            .iter()
            .filter(|(_, key)| !self.excluded.contains(key.as_str()))
            .map(|(platform, key)| Preferred {
                work: work.to_string(),
                platform: platform.clone(),
                variant_key: key.clone(),
            })
            .collect()
    }

    /// 保留的那个作品名；保留的那一行还没认出作品时是 `None`。
    fn keep_work(&self) -> Option<&str> {
        self.rows.get(self.keep)?.work.as_deref()
    }

    /// 这个字段眼下选的是哪一格。
    ///
    /// 默认照稿：**保留作品有值就用它的；保留作品那一格空着，就用别人的补上。**
    fn pick_of(&self, conflict: &merge::Conflict) -> Pick {
        match self.picks.get(&conflict.field) {
            Some(pick) => *pick,
            None if conflict.keep.is_some() => Pick::Keep,
            None => Pick::Other(0),
        }
    }

    /// 这一批要归入的变体：参与的那几行底下的，减去保留作品自己的、减去取消勾选的。
    fn moving(&self) -> Vec<String> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(at, _)| *at != self.keep)
            .flat_map(|(_, row)| row.variants.iter())
            .filter(|variant| !self.excluded.contains(&variant.key))
            .map(|variant| variant.key.clone())
            .collect()
    }

    /// 参与的那几行涉及哪几个平台，照走到的次序。
    fn platforms(&self) -> Vec<String> {
        平台们(self.rows.iter().flat_map(|row| row.variants.iter()))
    }

    /// 每个平台上**照规则**该选哪一个，重算一遍——**过期了才算**（它要问库）。
    ///
    /// 规则本身一条都不在这儿（ADR-0024）：[`merge::preferred_pick`] 拿
    /// `converge::preference_for`（汉化 > 官中 > 日版 > 其他，人裁过的让路）在手上这几个
    /// 变体之间排一遍名，与详情面板那一处问的是同一句话。
    fn sync_defaults(&mut self, site: &Site) {
        if !self.defaults_stale {
            return;
        }
        self.defaults_stale = false;
        self.defaults.clear();
        let Some(keep) = self.keep_work().map(ToString::to_string) else {
            return;
        };
        for platform in self.platforms() {
            let keys: Vec<&str> = self
                .rows
                .iter()
                .enumerate()
                .flat_map(|(at, row)| row.variants.iter().map(move |one| (at, one)))
                .filter(|(at, one)| {
                    one.platform == platform
                        && (*at == self.keep || !self.excluded.contains(&one.key))
                })
                .map(|(_, one)| one.key.as_str())
                .collect();
            match merge::preferred_pick(&site.catalog, &keep, &platform, &keys) {
                Ok(Some(key)) => {
                    self.defaults.insert(platform, key);
                }
                Ok(None) => {}
                Err(failed) => self.error = Some(format!("中立库读不动：{failed}")),
            }
        }
    }

    /// 进第三步那一下要算的：字段冲突、会发生什么、计划。**只在这一下算一次**——
    /// 那一趟要走一遍全库（挂单 `Q1011`）。
    fn enter_confirm(&mut self, site: &Site, priorities: &Priorities) {
        self.error = None;
        self.conflicts.clear();
        self.impact = None;
        self.planned = None;
        let Some(keep) = self.keep_work().map(ToString::to_string) else {
            self.error = Some("保留的那一行还没认出作品，没有作品名可以归入。".to_string());
            return;
        };
        let sources: Vec<String> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(at, _)| *at != self.keep)
            .filter_map(|(_, row)| row.work.clone())
            .collect();
        match merge::conflicts(&site.catalog, priorities, &keep, &sources) {
            Ok(rows) => self.conflicts = rows,
            Err(failed) => self.error = Some(format!("中立库读不动：{failed}")),
        }
        let moving = self.moving();
        match merge::plan(
            &site.catalog,
            &site.store,
            &site.library_identity,
            Kind::Merge,
            &keep,
            &moving,
        ) {
            Ok(planned) => {
                match merge::impact(&site.catalog, &planned) {
                    Ok(impact) => self.impact = Some(impact),
                    Err(failed) => self.error = Some(format!("中立库读不动：{failed}")),
                }
                self.planned = Some(planned);
            }
            Err(failed) => self.error = Some(failed.to_string()),
        }
    }

    /// 画这一帧。交回这一帧要办的事。
    ///
    /// `shelf` 是浏览屏行首封面那一份（[`Shelf`]）：第一步每一行左边那一格贴哪张、后台解码都走它，
    /// 摆这一层的那一屏照旧每帧 `Shelf::sync` 一次。
    ///
    /// `suspicions` 是浏览屏缓着的那一份**疑似同一作品**（核心库 `same_work::survey` 交的）：第一步「建议一并合并」从它挑。
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        site: &Site,
        priorities: &Priorities,
        shelf: &mut Shelf,
        suspicions: &[Suspicion],
    ) -> Option<Done> {
        // **页脚先搭好再画内容区**（共用弹层那一层的规矩）：按不按得动看的是这一帧画之前的状态。
        let 走得了 = match self.step {
            0 => self.rows.len() >= 2 && self.keep_work().is_some(),
            1 => !self.moving().is_empty(),
            _ => self.planned.as_ref().is_some_and(|one| one.verdicts() > 0),
        };
        let 几个 = self.rows.len();
        // 「取消」是幽灵按钮（设计稿 `.mfoot` 的 `.btn ghost`，差距清单 `M-06`）。
        let mut footer = Footer::new(Button::new("取消", Pressed::Cancel).ghost());
        if self.step > 0 {
            footer = footer.button(Button::new("上一步", Pressed::Back));
        }
        footer = footer.button(if self.step < 2 {
            Button::new("下一步", Pressed::Next)
                .primary()
                .enabled(走得了)
        } else {
            Button::new(format!("合并 {几个} 个作品"), Pressed::Go)
                .primary()
                .enabled(走得了)
                .hover("只写入裁决记录，不会移动或修改任何文件")
        });
        let shown = Dialog::new("合并作品", "合并作品", footer)
            .note("把被识别成不同作品、其实是同一个游戏的变体归到一起。")
            .pages(STEPS, self.step)
            .width(Width::Widest)
            .show(ctx, |ui| {
                if let Some(说的) = self.error.clone() {
                    ui.colored_label(ui.visuals().error_fg_color, 说的);
                    ui.add_space(look::step(1));
                }
                match self.step {
                    0 => self.step_pick(ui, site, priorities, shelf, suspicions),
                    1 => self.step_variants(ui, site),
                    _ => self.step_confirm(ui),
                }
            });
        match shown.pressed {
            None => None,
            Some(Pressed::Cancel) => Some(Done::Close),
            Some(Pressed::Back) => {
                self.step = self.step.saturating_sub(1);
                None
            }
            Some(Pressed::Next) => {
                self.step += 1;
                if self.step == 2 {
                    self.enter_confirm(site, priorities);
                }
                None
            }
            Some(Pressed::Go) => Some(Done::Apply),
        }
    }

    /// 第一步：**选择要保留的作品**。
    fn step_pick(
        &mut self,
        ui: &mut egui::Ui,
        site: &Site,
        priorities: &Priorities,
        shelf: &mut Shelf,
        suspicions: &[Suspicion],
    ) {
        look::help(
            ui,
            "选择要保留的作品。其他作品的变体会归入它，它们的名称保留为别名。\
             只写入裁决记录，不会移动或修改任何文件。",
        );
        ui.add_space(look::step(1));
        let mut 换保留 = None;
        let mut 去掉 = None;
        let 卡距 = Tokens::builtin().layout.merge_card_gap;
        // 几张卡之间（设计稿 `.mwlist` 的 `gap:8px`）。
        let 原来的竖距 = ui.spacing().item_spacing.y;
        ui.spacing_mut().item_spacing.y = look::step(1);
        for (at, row) in self.rows.iter().enumerate() {
            let 是保留 = at == self.keep;
            // 还没认出作品的那一行**当不了保留的那一侧**：它没有作品名可以留作别名。
            let 当得了 = row.work.is_some();
            // **照稿那一行**（`.mwit` 里那句 `help`）：平台 · 年份 · 几个变体 · 置信度。
            //
            // **置信度那个词收在这一句里，不另摆一枚标签**：稿上它就是这句话的末一段
            // （`${TIER[w.tier]}`）。另摆一枚的话它紧跟在作品名后头，而名字长短不一，
            // 三行的标签就永远对不齐（拿主意的人 2026-09-21 看图挑出）。
            // 哪一档照旧由核心库答（`WorkDetail::confidence`），这里只印那个词。
            let 一句 = format!(
                "{} · {} · {} 个变体 · {}",
                平台那一句(row),
                row.year.as_deref().unwrap_or("年份未知"),
                thousands(row.variants.len() as u64),
                row.tier.label(),
            );
            // **整张卡是一颗按钮**（设计稿 `.mwit`，差距清单 `M-01`）：点哪儿都换保留的那一个；
            // 「移除」那颗摆在卡上头，照旧只管移除。
            let 卡 = look::choice_card(
                ui,
                是保留,
                &row.title,
                卡距,
                look::DotAt::Middle,
                |ui| {
                    // 右头那颗「移除」要留出来的宽：字那一栏截尾巴截到它左边（稿上 `.mwit` 的第四列 `auto`）。
                    let 右头 = if self.rows.len() > 2 {
                        look::small_button_width(ui, "移除") + 卡距
                    } else {
                        0.0
                    };
                    卡上那几样(ui, shelf, row, 是保留, &一句, 右头);
                    if !当得了 {
                        look::help(ui, "还没认出作品，当不了保留的那一个");
                    }
                    self.rows.len() > 2
                        && ui
                            .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                look::small_ghost_button(ui, "移除").clicked()
                            })
                            .inner
                },
            );
            if 卡.response.clicked() && 当得了 {
                换保留 = Some(at);
            }
            if 卡.inner {
                去掉 = Some(at);
            }
        }
        ui.spacing_mut().item_spacing.y = 原来的竖距;
        if let Some(at) = 换保留 {
            self.keep = at;
            // 换了保留的那一个，第二步挑的首选与第三步选的值都跟着作废。
            self.picked_preferred.clear();
            self.defaults_stale = true;
            self.picks.clear();
        }
        if let Some(at) = 去掉 {
            self.rows.remove(at);
            // **先认是不是移走了保留的那一个，再挪下标**：从前先问「下标出没出界」，保留的是最末一行、
            // 移走的是它前头某一行时，下标一出界就退回了变体最多的那一个——人挑的保留被悄悄换掉。
            if self.keep == at {
                self.keep = best_keep(&self.rows);
            } else if self.keep > at {
                self.keep -= 1;
            }
            self.picked_preferred.clear();
            self.defaults_stale = true;
        }
        if self.rows.len() < 2 {
            ui.add_space(look::step(1));
            ui.colored_label(ui.visuals().error_fg_color, "至少需要两个作品。");
        }
        if self.platforms().len() > 1 {
            ui.add_space(look::step(1));
            look::note(
                ui,
                "这些作品分属不同平台。作品可以跨平台，合并后每个平台各有自己的首选变体。",
            );
        }
        self.suggest_ui(ui, site, priorities, suspicions);
        ui.add_space(look::step(2));
        look::section(ui, "添加其他作品");
        let 宽 = ui.available_width();
        look::text_input(
            ui,
            宽,
            egui::TextEdit::singleline(&mut self.query).hint_text("搜索名称"),
        );
        self.search(site, priorities);
        // **一行就是一颗按钮**（设计稿 `mwResults` 的 `<button data-mw="add:…">`，差距清单 `M-04`）：平台色标、粗体名、
        // 「年份 · 几个变体」，整行按下去就添加。
        let 名字号 = look::font_size(ui.ctx(), Tokens::builtin().font.size_small_plus);
        let 加 = look::search_list(
            ui,
            "添加其他作品",
            self.found.len(),
            "没有匹配的作品",
            |ui, at| {
                let row = &self.found[at];
                for platform in 平台们(row.variants.iter()) {
                    super::work::platform_chip(ui, &platform);
                }
                ui.label(
                    font::strong(&row.title)
                        .size(名字号)
                        .color(look::palette(ui).ink),
                );
                look::help(
                    ui,
                    &format!(
                        "{} · {} 个变体",
                        row.year.as_deref().unwrap_or("年份未知"),
                        thousands(row.variants.len() as u64),
                    ),
                );
                false
            },
        )
        .map(|at| self.found[at].clone());
        if let Some(row) = 加 {
            self.rows.push(row);
            self.found_for = None;
            self.query.clear();
            self.defaults_stale = true;
        }
    }

    /// 第一步「建议一并合并」那一段（设计稿 `renderMW` 的 `extra`，差距清单 `M-03`）：小标题底下一行一个——
    /// 名字、核心库给的头一条理由、「添加」。一个都没有时整段不画。
    fn suggest_ui(
        &mut self,
        ui: &mut egui::Ui,
        site: &Site,
        priorities: &Priorities,
        suspicions: &[Suspicion],
    ) {
        self.sync_suggestions(site, priorities, suspicions);
        if self.suggested.is_empty() {
            return;
        }
        ui.add_space(look::step(2));
        look::section(ui, "建议一并合并");
        let mut 加 = None;
        for (at, one) in self.suggested.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(&one.row.title);
                // 理由一行放不下就截尾巴、停上去看整句：「添加」那颗得留在这一行右头。
                let 余下 = (ui.available_width()
                    - look::small_button_width(ui, "添加")
                    - ui.spacing().item_spacing.x)
                    .max(0.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(余下, ui.spacing().interact_size.y),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(&one.reason).small().weak())
                                .truncate(),
                        )
                        .on_hover_text(&one.reason);
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if look::small_buttons(ui, |ui| ui.button("添加").clicked()) {
                        加 = Some(at);
                    }
                });
            });
        }
        if let Some(at) = 加 {
            let one = self.suggested.remove(at);
            self.rows.push(one.row);
            self.found_for = None;
            self.defaults_stale = true;
        }
    }

    /// 「建议一并合并」那几行重算一遍——**参与的那几个作品变了才算**（它要问库）。
    ///
    /// 挑的是「一边在向导里、一边不在」的那几对（设计稿 `m.ids.includes(s.a)!==m.ids.includes(s.b)`）：照参与的作品一个个问
    /// 核心库那一对牵没牵着它、另一边是谁（`Suspicion::touches` / `other_than`）；两边都在的不提。同一个作品被两个参与的作品
    /// 各牵一次时只提一行，理由取头一对的。
    fn sync_suggestions(&mut self, site: &Site, priorities: &Priorities, suspicions: &[Suspicion]) {
        let 在里头: Vec<String> = self
            .rows
            .iter()
            .filter_map(|row| row.work.clone())
            .collect();
        if self.suggested_for.as_ref() == Some(&在里头) {
            return;
        }
        self.suggested.clear();
        let mut 提过: BTreeSet<&str> = BTreeSet::new();
        for work in &在里头 {
            for one in suspicions.iter().filter(|one| one.touches(work)) {
                let Some(外头那个) = one.other_than(work) else {
                    continue;
                };
                if 在里头.iter().any(|name| name == 外头那个) || !提过.insert(外头那个)
                {
                    continue;
                }
                let Some(reason) = one.reasons().into_iter().next() else {
                    continue;
                };
                let anchor = match site.catalog.work_named(外头那个) {
                    Ok(Some(id)) => WorkAnchor::Work(id),
                    // 缓着的那份建议过期了（那个作品已经被并掉）：不提。
                    Ok(None) => continue,
                    Err(failed) => {
                        self.error = Some(format!("中立库读不动：{failed}"));
                        continue;
                    }
                };
                match Row::load_whole(&site.catalog, priorities, &anchor) {
                    Ok(Some(row)) => self.suggested.push(Suggested { row, reason }),
                    Ok(None) => {}
                    Err(failed) => self.error = Some(format!("中立库读不动：{failed}")),
                }
            }
        }
        self.suggested_for = Some(在里头);
    }

    /// 「添加其他作品」搜一趟。**框里的字没变就不重搜**——这一层每帧都画。
    fn search(&mut self, site: &Site, priorities: &Priorities) {
        if self.found_for.as_deref() == Some(self.query.as_str()) {
            return;
        }
        self.found_for = Some(self.query.clone());
        self.found.clear();
        let 已有: BTreeSet<WorkAnchor> = self.rows.iter().map(|row| row.anchor.clone()).collect();
        let query = WorkQuery {
            search: self.query.clone(),
            ..WorkQuery::default()
        };
        let mut 搜到的 = Vec::new();
        if let Err(failed) = 收作品(
            &site.catalog,
            &query,
            列几行(&self.query, PICK_BROWSED),
            |row| !已有.contains(&row.anchor),
            &mut 搜到的,
        ) {
            self.error = Some(format!("中立库读不动：{failed}"));
            return;
        }
        for (anchor, _) in 搜到的 {
            match Row::load_whole(&site.catalog, priorities, &anchor) {
                Ok(Some(row)) => self.found.push(row),
                Ok(None) => {}
                Err(failed) => self.error = Some(format!("中立库读不动：{failed}")),
            }
        }
    }

    /// 第二步：**核对变体**，按平台分组，每组各挑一个首选。
    ///
    /// **默认选中的那一个由核心库照规则算**（[`merge::preferred_pick`]），算完攥在
    /// [`Self::defaults`] 里——它要问库，不能每帧算。人点过的那几个另存
    /// （[`Self::picked_preferred`]），**只有那几个才落库**。
    fn step_variants(&mut self, ui: &mut egui::Ui, site: &Site) {
        self.sync_defaults(site);
        look::help(
            ui,
            "取消勾选的变体不会合并，仍留在原来的作品中。首选变体是前端默认启动的那一个，每个平台一个。",
        );
        look::help(
            ui,
            "默认选中的那一个是按「汉化 > 官中 > 日版 > 其他」选出来的；不动它就照这条规则来，\
             点一下才记成裁决。",
        );
        ui.add_space(look::step(1));
        let mut 翻 = None;
        let mut 挑 = Vec::new();
        for platform in self.platforms() {
            let 这组: Vec<(usize, &RowVariant)> = self
                .rows
                .iter()
                .enumerate()
                .flat_map(|(at, row)| row.variants.iter().map(move |one| (at, one)))
                .filter(|(_, one)| one.platform == platform)
                .collect();
            let 还剩 = 这组
                .iter()
                .filter(|(at, one)| *at == self.keep || !self.excluded.contains(&one.key))
                .count();
            let 首选 = self
                .picked_preferred
                .get(&platform)
                .or_else(|| self.defaults.get(&platform))
                .cloned();
            // **一组一个框**（设计稿 `.vgrp`，差距清单 `M-07`）：头一排满宽 `panel-2` 底、底下一条线——平台色标、
            // **粗体全名**（核心库平台表 `Manifest::full_name`；表里没写全名的退回代号）、「N 个变体」；底下一行一个变体。
            let tokens = Tokens::builtin();
            let 线 = ui.visuals().widgets.noninteractive.bg_stroke;
            let 圆角 = tokens.radius.large;
            egui::Frame::new()
                .stroke(线)
                .corner_radius(圆角)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let [头上下, 头左右] = tokens.layout.variant_group_head_padding;
                    let 内角 = 圆角.saturating_sub(1);
                    egui::Frame::new()
                        .fill(ui.visuals().faint_bg_color)
                        .corner_radius(egui::CornerRadius {
                            nw: 内角,
                            ne: 内角,
                            sw: 0,
                            se: 0,
                        })
                        .inner_margin(egui::Margin::from(egui::vec2(头左右, 头上下)))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                super::work::platform_chip(ui, &platform);
                                ui.label(
                                    font::strong(super::work::platform_full_name(&platform))
                                        .color(look::palette(ui).ink),
                                );
                                look::help(ui, &format!("{} 个变体", thousands(还剩 as u64)));
                            });
                        });
                    look::divider(ui);
                    let 几行 = 这组.len();
                    for (第几行, (at, variant)) in 这组.into_iter().enumerate() {
                        let 自带 = at == self.keep;
                        let 勾着 = 自带 || !self.excluded.contains(&variant.key);
                        // 屏上常驻那句与停上去那句是同一句（ADR-0005 再修订：那句理由只许有一处）。
                        let 来自 = if 自带 {
                            merge::ALREADY_HELD.to_string()
                        } else {
                            format!("来自「{}」", self.rows[at].title)
                        };
                        let 是首选 = 首选.as_deref() == Some(variant.key.as_str());
                        let 按了 = 变体那一行(ui, variant, 自带, 勾着, &来自, 是首选);
                        if 按了.翻 {
                            翻 = Some(variant.key.clone());
                        }
                        if 按了.挑 {
                            挑.push((platform.clone(), variant.key.clone()));
                        }
                        if 第几行 + 1 < 几行 {
                            look::divider(ui);
                        }
                    }
                });
            // 组与组之间（设计稿 `.vgrp` 的 `margin-bottom:12px`）。
            ui.add_space(look::step(2));
        }
        if let Some(key) = 翻 {
            if !self.excluded.remove(&key) {
                self.excluded.insert(key);
            }
            // 勾掉一个之后规则该挑谁可能就变了，默认值跟着作废；人点过的那几个留着。
            self.defaults_stale = true;
        }
        for (platform, key) in 挑 {
            self.picked_preferred.insert(platform, key);
        }
        if self.moving().is_empty() {
            ui.add_space(look::step(1));
            ui.colored_label(
                ui.visuals().error_fg_color,
                "一个要归入的变体都没有：至少留下一个。",
            );
        }
    }

    /// 第三步：**字段冲突**与**合并后会发生什么**。
    ///
    /// 屏上提到保留的那个作品时写它的**显示标题**（与第一步那一行粗体、第二步「来自「…」」同一个名字，照稿 `kt=WORKS[m.keep].t`）；
    /// 别名那一句照旧写**作品名**——留作别名的正是这几个名字（`merge::keep_aliases`）。
    fn step_confirm(&mut self, ui: &mut egui::Ui) {
        let 保留的 = self
            .rows
            .get(self.keep)
            .map(|row| row.title.clone())
            .unwrap_or_default();
        look::section(ui, "字段冲突");
        look::help(
            ui,
            "默认保留作品的值；保留作品缺少的字段，默认用其他作品的值补上。",
        );
        if self.conflicts.is_empty() {
            look::help(ui, "没有冲突的字段。");
        }
        let 选 = self.conflict_table(ui);
        for (field, at) in 选 {
            self.picks.insert(field, at);
        }

        ui.add_space(look::step(1));
        // **说清留的是哪几个名字**（Spec 轴挑出）：留的是**一个变体都不剩**的那几个
        // （核心库 `Impact::emptied`）。第二步取消掉某个变体、那个作品还剩变体时，
        // 它照旧在浏览列表里、名字就是它自己，不必记成别人的别名——但屏上得说出来，
        // 不然人以为勾了就每个都留。说明挂在名字底下（设计稿 `.opt small`，差距清单 `M-16`）。
        let 留的 = match self.impact.as_ref().map(|impact| impact.emptied.clone()) {
            Some(names) if !names.is_empty() => format!(
                "留的是并空了的那几个：《{}》。搜这些名称仍能找到合并后的作品。",
                names.join("》《"),
            ),
            _ => "这一趟没有作品会被并空：它们都还剩变体、照旧在浏览列表里，名字不必留作别名。"
                .to_string(),
        };
        look::checkbox_option(ui, &mut self.alias, "把被合并作品的名称保留为别名", &留的);
        // **「以后扫描到的也自动归入」画成不可选**，并写明原因与眼下的走法（票面 ⚠️ 第二条）。
        // 整格调淡到 `muted-option-opacity`（设计稿 `<label class="opt" style="opacity:.65">`）：
        // 按不动那一层的调淡就是这一层（egui 禁用时照 `disabled_alpha` 调淡），不另叠一层。
        let mut 永不 = false;
        ui.scope(|ui| {
            ui.visuals_mut().disabled_alpha = Tokens::builtin().mix.muted_option_opacity;
            ui.add_enabled_ui(false, |ui| {
                look::checkbox_option(
                    ui,
                    &mut 永不,
                    &format!("以后扫描到被识别为这些作品的变体，也自动归入「{保留的}」"),
                    merge::AUTO_ABSORB_REASON,
                );
            });
        });

        ui.add_space(look::step(2));
        look::section(ui, "合并后会发生什么");
        let Some(impact) = self.impact.clone() else {
            look::help(ui, "算不出这一笔账。");
            return;
        };
        look::impact(
            ui,
            &[
                ("写入 ", false),
                (&format!("{} 条裁决", thousands(impact.verdicts)), true),
                (
                    &format!(
                        "：每个归入的变体一条，发行版信息不变，只把作品改为「{保留的}」。\
                         裁决按文件内容记录，改名或移动文件后仍然有效。"
                    ),
                    false,
                ),
            ],
        );
        look::impact(
            ui,
            &[
                ("前端条目 ", false),
                (
                    &format!(
                        "{} → {}",
                        thousands(impact.entries_before),
                        thousands(impact.entries_after)
                    ),
                    true,
                ),
                (
                    "：下次导出时合并为一个条目，默认启动各平台的首选变体。",
                    false,
                ),
            ],
        );
        if !impact.sublibraries.is_empty() {
            look::impact_warn(
                ui,
                &[
                    (
                        &format!("子库「{}」", impact.sublibraries.join("」「")),
                        true,
                    ),
                    (
                        "的选择集不变，但前端条目会变化，同步前需要重新生成差量预览。",
                        false,
                    ),
                ],
            );
        }
        look::impact(ui, &[("收藏、合集和手动修改过的元数据不受影响。", false)]);
        // **撤得回哪几样要说全**：批只装沉淀库的裁决，别名、字段选值与首选变体是另外三笔
        // 中立库的账（挂单 `Q1012`）。稿上那句只说「可以整批撤销」。
        look::impact(
            ui,
            &[
                ("可以在「待确认 → 裁决记录」中", false),
                ("整批撤销", true),
                (
                    "，变体回到原来的作品。别名、上面选的字段值与首选变体不跟着撤，\
                     在作品详情页里各自撤得掉。",
                    false,
                ),
            ],
        );
    }
}

impl Wizard {
    /// 第三步那张**冲突表**（设计稿 `.panel` 里的 `.tbl.ctbl`，差距清单 `M-11`、`M-12`、`M-15`）：字段 / 保留作品 / 其他作品三列，
    /// 头一列 `conflict-key-width`、后两列分余下的；表头照 `.tbl th`（`table::head_text`），底下一条线；一个字段一行、行间一条线，
    /// **两边都靠上**（`.ctbl td{vertical-align:top}`）。每个选项是一枚单独的圆点挨着它那几个字（`.ctbl label`），
    /// 一格里几个选项紧挨着摞——不是一行一个带空说明的 `.opt`，那样一行六十来点高，五个字段就把「合并后会发生什么」挤出折叠线。
    ///
    /// 交回这一帧选了哪几格。
    fn conflict_table(&self, ui: &mut egui::Ui) -> Vec<(Field, Pick)> {
        let tokens = Tokens::builtin();
        let palette = look::palette(ui);
        let 线 = ui.visuals().widgets.noninteractive.bg_stroke;
        let mut 选 = Vec::new();
        if self.conflicts.is_empty() {
            return 选;
        }
        egui::Frame::new()
            .fill(palette.panel)
            .stroke(线)
            .corner_radius(tokens.radius.large)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                let 字段宽 = tokens.layout.conflict_key_width;
                let 值宽 = ((ui.available_width() - 字段宽) / 2.0).max(0.0);
                let 头 = tokens.space.table_head_padding;
                let 格 = tokens.space.cell_padding;
                ui.horizontal_top(|ui| {
                    for (那几个字, 宽) in [("字段", 字段宽), ("保留作品", 值宽), ("其他作品", 值宽)]
                    {
                        表格一格(ui, 宽, 头, |ui| {
                            ui.label(table::head_text(ui, 那几个字));
                        });
                    }
                });
                look::divider(ui);
                let 几行 = self.conflicts.len();
                for (第几行, conflict) in self.conflicts.iter().enumerate() {
                    let 眼下 = self.pick_of(conflict);
                    ui.horizontal_top(|ui| {
                        表格一格(ui, 字段宽, 格, |ui| {
                            ui.label(
                                font::strong(冲突那一行叫什么(conflict.field))
                                    .size(look::font_size(ui.ctx(), tokens.font.size_small_plus))
                                    .color(palette.ink),
                            );
                        });
                        表格一格(ui, 值宽, 格, |ui| match &conflict.keep {
                            Some(said) => {
                                let 整句 = said.values.join("、");
                                if 一个选项(ui, 眼下 == Pick::Keep, &剪一段(&整句), None)
                                    .on_hover_text(&整句)
                                    .clicked()
                                {
                                    选.push((conflict.field, Pick::Keep));
                                }
                            }
                            // 保留作品这一格空着时那一格**选不了**（稿上那颗单选钮是 `disabled`，旁边写「（空）」）：
                            // 没有值可用，默认就是别人的那一个。
                            None => {
                                ui.horizontal_top(|ui| {
                                    ui.spacing_mut().item_spacing.x = tokens.layout.option_gap;
                                    ui.add_enabled_ui(false, |ui| look::radio_mark(ui, false));
                                    look::help(ui, "（空）");
                                });
                            }
                        });
                        表格一格(ui, 值宽, 格, |ui| {
                            for (at, offer) in conflict.others.iter().enumerate() {
                                let 整句 = offer.said.values.join("、");
                                let 是谁 = self.shown_name_of(&offer.work);
                                // **值与来源同字时不写两遍**：显示标题那一行的值常常就是那个作品的名字，
                                // 写成「某某 · 某某」读起来像出了错（审查挑出，2026-09-21 照改）。
                                let 来源 = (整句 != 是谁).then_some(是谁);
                                if 一个选项(ui, 眼下 == Pick::Other(at), &剪一段(&整句), 来源)
                                    .on_hover_text(&整句)
                                    .clicked()
                                {
                                    选.push((conflict.field, Pick::Other(at)));
                                }
                            }
                        });
                    });
                    if 第几行 + 1 < 几行 {
                        look::divider(ui);
                    }
                }
            });
        选
    }

    /// 参与的那几个作品里叫这个**作品名**的那一个，屏上叫什么（它那一行的显示标题）；不在里头时就是作品名。
    fn shown_name_of(&self, work: &str) -> String {
        self.rows
            .iter()
            .find(|row| row.work.as_deref() == Some(work))
            .map_or_else(|| work.to_string(), |row| row.title.clone())
    }
}

/// 冲突表那一行叫什么：标题那一行叫「**显示标题**」（这一行比的就是导出写进去的那一个，词表**显示标题**；
/// 照稿 `mwConflicts` 的 `l:'显示标题'`，差距清单 `M-13`），别的照字段自己的词。**只改这一处称呼**：
/// 存进库、打给命令行的仍是 [`Field::label`]。
fn 冲突那一行叫什么(field: Field) -> &'static str {
    match field {
        Field::Title => "显示标题",
        other => other.label(),
    }
}

/// 冲突表的**一格**：明说了宽，内边距 `[上下, 左右]`，里头从上往下摆、靠上（`.ctbl td{vertical-align:top}`）。
fn 表格一格(
    ui: &mut egui::Ui,
    宽: f32,
    [上下, 左右]: [f32; 2],
    add: impl FnOnce(&mut egui::Ui),
) {
    ui.allocate_ui_with_layout(
        egui::vec2(宽, 0.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(宽);
            egui::Frame::new()
                .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    add(ui);
                });
        },
    );
}

/// 冲突表一格里的**一个选项**（设计稿 `.ctbl label`）：一枚单独的圆点（[`look::radio_dot`]）挨着那几个字，隔 `option-gap`、
/// 两边靠上；字是正文那一档（12.5、`ink`），`来源` 给了就在后头接「 · 来源」（`.help` 的 12、`ink-3`）。字放不下折行。
/// 圆点与字哪一处按下去都算。
fn 一个选项(
    ui: &mut egui::Ui,
    selected: bool,
    值: &str,
    来源: Option<String>,
) -> egui::Response {
    let tokens = Tokens::builtin();
    let palette = look::palette(ui);
    let style = ui.style().clone();
    let mut job = egui::text::LayoutJob::default();
    egui::RichText::new(值)
        .size(look::font_size(ui.ctx(), tokens.font.size_small_plus))
        .color(palette.ink)
        .append_to(
            &mut job,
            &style,
            egui::FontSelection::Default,
            egui::Align::Min,
        );
    if let Some(来源) = 来源 {
        egui::RichText::new(format!(" · {来源}"))
            .size(look::font_size(ui.ctx(), tokens.font.size_small))
            .color(palette.ink_3)
            .append_to(
                &mut job,
                &style,
                egui::FontSelection::Default,
                egui::Align::Min,
            );
    }
    let response = ui
        .horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = tokens.layout.option_gap;
            let 点 = look::radio_dot(ui, selected);
            let 字 = ui.add(egui::Label::new(job).wrap().sense(egui::Sense::click()));
            点 | 字
        })
        .inner;
    let enabled = ui.is_enabled();
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::RadioButton, enabled, selected, 值)
    });
    response
}

/// 默认保留哪一个：**变体最多的那一个**，同数时按屏上那个名字定序（全序，开两次向导推荐的是同一个）。
/// 还没认出作品的那几行当不了保留的一侧，一律排在后面。
fn best_keep(rows: &[Row]) -> usize {
    rows.iter()
        .enumerate()
        .max_by_key(|(at, row)| {
            (
                row.work.is_some(),
                row.variants.len(),
                std::cmp::Reverse(row.title.clone()),
                std::cmp::Reverse(*at),
            )
        })
        .map_or(0, |(at, _)| at)
}

/// 这几个变体涉及哪几个平台，**去重、照走到的次序**。
///
/// 收成一处：第一步那一行写「GB / GBC」与第二步按平台分组问的是同一句话，
/// 各写一遍的话两处的次序迟早不一样。
fn 平台们<'a>(variants: impl Iterator<Item = &'a RowVariant>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for variant in variants {
        if !out.contains(&variant.platform) {
            out.push(variant.platform.clone());
        }
    }
    out
}

/// 一行涉及哪几个平台，写成屏上那一句（几个平台之间「 / 」）；一个都说不出时写平台未知那一档的词。
fn 平台那一句(row: &Row) -> String {
    let out = 平台们(row.variants.iter());
    if out.is_empty() {
        return romcat_core::report::UNKNOWN_PLATFORM_LABEL.to_string();
    }
    out.join(" / ")
}

/// 第一步那一张卡里**圆点右边那几样**（设计稿 `.mwit` 的后三列，圆点与整张卡由 [`look::choice_card`] 画）：一格封面缩略图
/// （没有封面画平台色块，[`Shelf::merge_cover`]），右边一栏字——显示标题（13 号、拉丁与数字粗，保留的那一个后头紧跟一枚「保留」）、
/// 底下「平台 · 年份 · 几个变体 · 置信度」那一句、再底下那一句副标题（与作品详情页头上**同一句**，`work::subtitle_line`；
/// 一行放不下截尾巴）。底下两句是说明那一档（`.help` 的 12 号、`ink-3`）。
///
/// 副标题那一句**用比例字**、与说明同一档（拿主意的人 2026-10-01 裁）：稿上那一行是等宽小字，可那是给罗马字的官方名
/// 用的；这一句拼进了中文、日文的别名，打包的等宽字体只有拉丁字符，汉字回落成常规体、一行里字宽不齐。
///
/// `右头` 是这一行右头还要摆的东西占多宽（「移除」那颗）：字那一栏截到它左边。
fn 卡上那几样(
    ui: &mut egui::Ui,
    shelf: &mut Shelf,
    row: &Row,
    是保留: bool,
    一句: &str,
    右头: f32,
) {
    let tokens = Tokens::builtin();
    let layout = &tokens.layout;
    let palette = look::palette(ui);
    let 名字号 = look::font_size(ui.ctx(), tokens.font.size_body);
    shelf.merge_cover(
        ui,
        &row.anchor,
        row.work.as_deref().unwrap_or_default(),
        &row.platform,
    );
    ui.vertical(|ui| {
        ui.set_max_width((ui.available_width() - 右头).max(0.0));
        ui.spacing_mut().item_spacing.y = 0.0;
        // 横排一行最矮是 `interact_size.y`（按钮那么高）：清掉它，名字那一行才不被撑高。
        ui.spacing_mut().interact_size.y = 0.0;
        // **「保留」那枚标签紧跟在作品名后头**（稿上 `.mwit` 的 `<b>作品名</b>` 后头就是 `.keepb`，
        // 挂单 `Q1015`）：摆在整块后头的话，它离名字隔着整句说明那么远、落在两行之间。
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = layout.option_gap;
            ui.add(
                egui::Label::new(font::strong(&row.title).size(名字号).color(palette.ink)).extend(),
            );
            if 是保留 {
                look::accent_tag(ui, "保留");
            } else {
                // 没挂标签的那几行也留出标签那么高：不然保留那一行比别的行高出一截，几张卡高矮不齐。
                ui.allocate_space(egui::vec2(0.0, layout.tag_height));
            }
        });
        look::help(ui, 一句);
        if let Some(副) = &row.subtitle {
            ui.add(egui::Label::new(egui::RichText::new(副).small().weak()).truncate());
        }
    });
}

/// 第二步一行变体上这一帧按了什么。
#[derive(Debug, Clone, Copy, Default)]
struct 一行按了 {
    /// 勾选框（或名字那一格）拨了一下。
    翻: bool,
    /// 「首选」那颗按了。
    挑: bool,
}

/// 第二步的**一行变体**（设计稿 `.vrow`，差距清单 `M-08`）：六列——勾选框 `variant-check-column`｜变体（占余下的）｜
/// `merge-row-columns` 那四列（来自哪儿、置信度、体积、首选），列距 `variant-row-gap`、内边距 `variant-row-padding`。
/// **整行竖直居中、每一格单行**：变体那一格是粗体简称摞在等宽路径上（「根名 · 相对路径」，`table::root_and_path`，
/// 拿主意的人 2026-10-01 裁 `F-8`），放不下截；「来自「…」」那一格放不下截尾巴、停上去看整句（`M-09`）；
/// 置信度是一枚标签（[`look::chip`]，`M-10`）。
///
/// **勾选框单独一列，名字那一格也接点击**：egui 里一个光秃秃的小方块是个很小的靶子，而这一步正是要人逐个点过去。
/// **保留作品自己的变体勾不掉**（停上去说 [`merge::ALREADY_HELD`]）；**取消勾选的那一行整行调淡**（照稿
/// `.vrow.off{opacity:.5}`）：「首选」那颗在这一行上按不动，而按不动的理由得在屏上常驻、不能只挂在悬停里
/// （ADR-0005 再修订那一节的第 2 条）。
fn 变体那一行(
    ui: &mut egui::Ui,
    variant: &RowVariant,
    自带: bool,
    勾着: bool,
    来自: &str,
    是首选: bool,
) -> 一行按了 {
    let tokens = Tokens::builtin();
    let layout = &tokens.layout;
    let palette = look::palette(ui);
    let [上下, 左右] = layout.variant_row_padding;
    let 缝 = layout.variant_row_gap;
    let [来自宽, 置信宽, 体积宽, 首选宽] = layout.merge_row_columns;
    let 名字体 = egui::FontId::new(
        look::font_size(ui.ctx(), tokens.font.size_small_plus),
        font::strong_family(),
    );
    let 路径体 = egui::FontId::monospace(tokens.font.size_path);
    // 一行多高：简称一行加路径一行（量的是排出来的一行字，与旁边真画的那两段同高）。
    let 行高 = (look::line_height(ui, &名字体) + look::line_height(ui, &路径体))
        .max(ui.spacing().interact_size.y);
    let mut 按了 = 一行按了::default();
    egui::Frame::new()
        .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !勾着 {
                ui.set_opacity(tokens.mix.variant_off_opacity);
            }
            ui.style_mut().interaction.selectable_labels = false;
            let 变体宽 = (ui.available_width()
                - layout.variant_check_column
                - 来自宽
                - 置信宽
                - 体积宽
                - 首选宽
                - 5.0 * 缝)
                .max(0.0);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 行高),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 缝;
                    let mut 勾 = 勾着;
                    let 框 = 一格(ui, layout.variant_check_column, 行高, |ui| {
                        ui.add_enabled_ui(!自带, |ui| look::checkbox(ui, &mut 勾, ""))
                            .inner
                            .on_disabled_hover_text(merge::ALREADY_HELD)
                    });
                    let 名格 = 一格(ui, 变体宽, 行高, |ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&variant.short)
                                        .font(名字体.clone())
                                        .color(palette.ink),
                                )
                                .truncate(),
                            );
                            let 路径 = table::root_and_path(ui, &variant.key, &路径体, 变体宽);
                            ui.label(
                                egui::RichText::new(路径)
                                    .font(路径体.clone())
                                    .color(palette.ink_3),
                            );
                        });
                    });
                    let 名点 = ui.interact(
                        名格.response.rect,
                        ui.id().with(("变体那一格", &variant.key)),
                        if 自带 {
                            egui::Sense::hover()
                        } else {
                            egui::Sense::click()
                        },
                    );
                    if 框.inner.changed() || 名点.clicked() {
                        按了.翻 = true;
                    }
                    一格(ui, 来自宽, 行高, |ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(来自).small().weak()).truncate(),
                        )
                        .on_hover_text(来自);
                    });
                    一格(ui, 置信宽, 行高, |ui| {
                        look::chip(ui, look::tier_tone(variant.tier), variant.tier.label());
                    });
                    一格(ui, 体积宽, 行高, |ui| {
                        look::help(ui, &human_bytes(variant.bytes));
                    });
                    一格(ui, 首选宽, 行高, |ui| {
                        if ui
                            .add_enabled_ui(勾着, |ui| look::radio(ui, 是首选, "首选"))
                            .inner
                            .on_disabled_hover_text(NO_PREFER)
                            .clicked()
                        {
                            按了.挑 = true;
                        }
                    });
                },
            );
        });
    按了
}

/// 一行里**明说了宽与高的一格**，里头的东西竖直居中（第二步那一行用：整行竖直居中，几列各自对得齐）。
fn 一格<R>(
    ui: &mut egui::Ui,
    宽: f32,
    高: f32,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.allocate_ui_with_layout(
        egui::vec2(宽, 高),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_size(egui::vec2(宽, 高));
            ui.set_max_width(宽);
            add(ui)
        },
    )
}

/// 一格里最多印这么多字，剪掉的补一个省略号（设计稿第三步那两格 `slice(0,80)`）。
fn 剪一段(text: &str) -> String {
    let mut out: String = text.chars().take(CELL).collect();
    if out.chars().count() < text.chars().count() {
        out.push('…');
    }
    out
}

/// **移出此作品**弹层开着时记着的东西（设计稿 `DLG.split`）。
#[derive(Debug, Clone)]
pub struct Split {
    /// 从哪个作品移出：屏上那个名字（认出作品的是**显示标题**，与作品详情页大标题同一个；认不出的是正题）。
    from: String,
    /// 从哪个作品移出：作品名（核心库认的那个）。
    from_work: Option<String>,
    /// 哪个变体。
    key: String,
    /// 那个变体的**变体简称**。
    short: String,
    /// 它所在的平台。
    platform: Option<String>,
    /// 它的置信度那一档（照稿摆在那张卡右头）。
    tier: Tier,
    /// 它多大。
    bytes: u64,
    /// 移到新建的作品（真）还是另一个已有作品（假）。
    fresh: bool,
    /// 新建那一档：框里打的名字。
    name: String,
    /// 已有那一档：搜索框里打的字。
    query: String,
    /// 搜出来的那几个作品。
    found: Vec<AnotherWork>,
    /// `found` 是照哪几个字搜的。
    found_for: Option<String>,
    /// 已有那一档：挑中的那个作品。
    target: Option<AnotherWork>,
    /// 这个作品移走这一个之后**还剩几个变体**——**核心库数的**（[`merge::remaining`]，
    /// 与合并第三步那句「并空了的那几个」同一处判，ADR-0024）。
    ///
    /// 不拿手上那一份 `WorkDetail::variants` 减一：那一份**随当前筛选收窄**，
    /// 一开筛屏上就会说错「没有变体了，会从浏览列表中消失」（Standards 轴挑出，2026-09-21 照改）。
    left: u64,
    /// 移走这一个之后，这个作品在这个平台上的**首选变体**照规则该是哪一个（核心库 [`merge::preferred_pick`]），
    /// 写成它的**变体简称**（差距清单 `M-19`）；说不出、或者移走的本来就不是首选时是 `None`。
    next_preferred: Option<String>,
    /// 排好的计划。
    planned: Option<merge::Regrouping>,
    /// 这一层自己那句错。
    error: Option<String>,
}

/// 「移入另一个作品」那一框里搜出来的一个**作品**（设计稿 `DLG.split` 的 `.srch` 一行）。
///
/// 不叫「候选」：词表里**候选**专指识别给变体找出来的那个可能的发行版。
#[derive(Debug, Clone)]
struct AnotherWork {
    /// 作品名：落裁决认的是它。
    work: String,
    /// 屏上那个名字：**显示标题**（`WorkRow::display`），取不到时是作品名。
    title: String,
    /// 横跨哪几个平台。
    platforms: Vec<String>,
    /// 年份；一条都没刮到时是 `None`。
    year: Option<String>,
    /// 底下几个变体（整个作品的，不过筛选）。
    variants: u64,
}

impl Split {
    /// 从作品详情页某一张变体卡上开一层。`shorts` 是那一页上每个变体的**变体简称**，与 `work.variants` 一一对齐
    /// （核心库 `Catalog::variant_short_names` 拼的，详情页那一份）。
    #[must_use]
    pub fn open(
        catalog: &Catalog,
        work: &WorkDetail,
        rules: &Rules,
        priorities: &Priorities,
        key: &str,
        shorts: &[String],
    ) -> Self {
        // 一个变体的**变体简称**：那一页上与 `work.variants` 对齐的那一份；那一页上没有它时是 `None`。
        let 简称 = |key: &str| {
            work.variants
                .iter()
                .position(|one| one.row.key == key)
                .and_then(|at| shorts.get(at))
                .cloned()
        };
        let 这一个 = work.variants.iter().find(|one| one.row.key == key);
        let short = 简称(key).unwrap_or_else(|| key.to_string());
        let platform = 这一个.and_then(|one| one.row.platform.clone());
        let from_work = match work.anchor {
            WorkAnchor::Work(_) => Some(work.name.clone()),
            WorkAnchor::Loose(_) => None,
        };
        let mut error = None;
        let mut 认: Box<dyn FnMut(CatalogError)> = Box::new(|failed: CatalogError| {
            error.get_or_insert_with(|| format!("中立库读不动：{failed}"));
        });
        let title = match 屏上的名字(catalog, work, rules, priorities) {
            Ok((title, _)) => title,
            Err(failed) => {
                认(failed);
                title_of(work, rules)
            }
        };

        // **默认名字里「是哪一种」由核心库答**（`variant_kind` → `variant_short_name`），
        // **不把变体简称按「 · 」剖回去**——那就成了「这是哪一种」的第二个答案，与词表
        // **变体简称**、**第几版**两条上「不从文件名剥」逐字同一条道理（ADR-0024，
        // Standards 轴挑出，2026-09-21 照改）。稿上那一手（`l.split(' · ')[0]`）不照抄。
        let 哪一种 = 这一个
            .and_then(|one| match catalog.variant_kind(one) {
                Ok(kind) => Some(romcat_core::catalog::browse::variant_short_name(
                    kind, None, key,
                )),
                Err(failed) => {
                    认(failed);
                    None
                }
            })
            .unwrap_or_else(|| short.clone());
        // 默认名字照稿拿**屏上那个名字**打头（`WORKS[i].t+'（'+…+'）'`）：与这一层标题底下那句、作品详情页大标题同一个。
        let 起名 = format!("{title}（{哪一种}）");

        // 移走之后还剩几个、首选变体照规则该换成谁——两样都问核心库。
        let mut left = 0;
        let mut next_preferred = None;
        if let Some(from) = from_work.as_deref() {
            match merge::remaining(catalog, &[key]) {
                Ok(rest) => left = rest.get(from).copied().unwrap_or(0),
                Err(failed) => 认(failed),
            }
            if let Some(platform) = platform.as_deref() {
                // 只有**移走的正是眼下那一个首选**时才说这句话：别的时候首选一个字没变。
                let 眼下 = match catalog.preferred_variant(from, platform) {
                    Ok(had) => had,
                    Err(failed) => {
                        认(failed);
                        None
                    }
                };
                if 眼下.as_deref() == Some(key) {
                    let 剩下的: Vec<&str> = work
                        .variants
                        .iter()
                        .filter(|one| {
                            one.row.key != key && one.row.platform.as_deref() == Some(platform)
                        })
                        .map(|one| one.row.key.as_str())
                        .collect();
                    match merge::preferred_pick(catalog, from, platform, &剩下的) {
                        // 写那一个的**变体简称**（与那一页变体卡头上同一个，`shorts`），不写文件名。
                        Ok(pick) => {
                            next_preferred = pick.map(|next| {
                                简称(&next).unwrap_or_else(|| {
                                    romcat_core::path::file_name_of_key(&next).to_string()
                                })
                            });
                        }
                        Err(failed) => 认(failed),
                    }
                }
            }
        }
        drop(认);

        Self {
            from: title,
            from_work,
            key: key.to_string(),
            short,
            platform,
            tier: Tier::of(这一个.and_then(WorkVariant::confidence)),
            bytes: 这一个.map_or(0, |one| one.row.bytes),
            fresh: true,
            name: 起名,
            query: String::new(),
            found: Vec::new(),
            found_for: None,
            target: None,
            left,
            next_preferred,
            planned: None,
            error,
        }
    }

    /// 排好的那份计划。
    #[must_use]
    pub fn planned(&self) -> Option<&merge::Regrouping> {
        self.planned.as_ref()
    }

    /// 移到哪个作品名下；两档都还没说出口时是 `None`（页脚那颗因此按不动）。
    fn target_work(&self) -> Option<String> {
        if self.fresh {
            let name = self.name.trim();
            return (!name.is_empty()).then(|| name.to_string());
        }
        self.target.as_ref().map(|one| one.work.clone())
    }

    /// 移到的那个作品屏上叫什么：新建那一档是框里打的名字，已有那一档是它的显示标题。
    fn target_title(&self) -> Option<String> {
        if self.fresh {
            return self.target_work();
        }
        self.target.as_ref().map(|one| one.title.clone())
    }

    /// 画这一帧。
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        site: &Site,
        priorities: &Priorities,
    ) -> Option<Done> {
        let 去处 = self.target_work();
        // 「取消」是幽灵按钮（设计稿 `DLG.split` 的 `.btn ghost`）。
        let footer = Footer::new(Button::new("取消", Pressed::Cancel).ghost()).button(
            Button::new("移出", Pressed::Go)
                .primary()
                .enabled(去处.is_some())
                .hover("只写入裁决记录，不移动或修改任何文件"),
        );
        let shown = Dialog::new("移出此作品", "移出此作品", footer)
            .note(format!(
                "把一个变体从「{}」移出，放到新建的作品或另一个已有作品中。\
                 只写入裁决记录，不移动或修改任何文件。",
                self.from
            ))
            // 稿上 `w:620`（差距清单 `M-17`）。
            .width(Width::Standard)
            .show(ctx, |ui| {
                if let Some(说的) = self.error.clone() {
                    ui.colored_label(ui.visuals().error_fg_color, 说的);
                    ui.add_space(look::step(1));
                }
                self.variant_card(ui);
                ui.add_space(look::step(1));
                self.destination_ui(ui, site, priorities);
                ui.add_space(look::step(2));
                self.impact_ui(ui);
            });
        match shown.pressed {
            None => None,
            Some(Pressed::Go) => {
                let into = 去处?;
                match merge::plan(
                    &site.catalog,
                    &site.store,
                    &site.library_identity,
                    Kind::Split,
                    &into,
                    std::slice::from_ref(&self.key),
                ) {
                    Ok(planned) => {
                        self.planned = Some(planned);
                        Some(Done::Apply)
                    }
                    Err(failed) => {
                        self.error = Some(failed.to_string());
                        None
                    }
                }
            }
            Some(_) => Some(Done::Close),
        }
    }

    /// 移的是哪一个（设计稿 `DLG.split` 头上那个 `.vrow`）：粗体简称摞在等宽的「根名 · 相对路径」上
    /// （`table::root_and_path`，拿主意的人 2026-10-01 裁 `F-8`），右头置信度标签（[`look::chip`]）与体积。
    fn variant_card(&self, ui: &mut egui::Ui) {
        let tokens = Tokens::builtin();
        let [上下, 左右] = tokens.layout.choice_card_padding;
        let palette = look::palette(ui);
        let 名字体 = egui::FontId::new(
            look::font_size(ui.ctx(), tokens.font.size_small_plus),
            font::strong_family(),
        );
        let 路径体 = egui::FontId::monospace(tokens.font.size_path);
        // 先明说这一行多高（简称一行加路径一行），右头那两样才对着同一条中线竖直居中。
        let 行高 = look::line_height(ui, &名字体) + look::line_height(ui, &路径体);
        look::card(ui, egui::vec2(左右, 上下), |ui| {
            // **右头先摆**（体积、置信度标签），左边那一摞占余下的宽：不拿字去估右头多宽。
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 行高),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = tokens.layout.variant_row_gap;
                    look::help(ui, &human_bytes(self.bytes));
                    look::chip(ui, look::tier_tone(self.tier), self.tier.label());
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let 宽 = ui.available_width();
                        ui.vertical(|ui| {
                            ui.set_max_width(宽);
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&self.short)
                                        .font(名字体.clone())
                                        .color(palette.ink),
                                )
                                .truncate(),
                            );
                            let 路径 = table::root_and_path(ui, &self.key, &路径体, 宽);
                            ui.label(
                                egui::RichText::new(路径)
                                    .font(路径体.clone())
                                    .color(palette.ink_3),
                            );
                        });
                    });
                },
            );
        });
    }

    /// 「移到」那两档（设计稿 `.asmode`，差距清单 `M-18`）：每档一张整张按得动的卡（[`look::choice_card`]），粗体名、底下一句说明；
    /// 选着的那一档底下缩进 `mode-indent` 挂它要填的那一格——新建那一档是名字框与一句说明，已有那一档是搜索框与搜出来的那一框。
    fn destination_ui(&mut self, ui: &mut egui::Ui, site: &Site, priorities: &Priorities) {
        let tokens = Tokens::builtin();
        let 卡距 = tokens.layout.mode_card_gap;
        look::section(ui, "移到");
        for (新建, 名, 说明) in [
            (
                true,
                "新建一个作品",
                "这个变体其实是另一个游戏（例如同名的不同作品、误归的改版）",
            ),
            (false, "移入另一个作品", "这个变体属于库里已有的另一个作品"),
        ] {
            let 选着 = self.fresh == 新建;
            let 卡 = look::choice_card(ui, 选着, 名, 卡距, look::DotAt::FirstLine, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(
                        font::strong(名)
                            .size(look::font_size(ui.ctx(), tokens.font.size_body))
                            .color(look::palette(ui).ink),
                    );
                    look::help(ui, 说明);
                });
            });
            if 卡.response.clicked() {
                self.fresh = 新建;
            }
            if 选着 {
                缩进一格(ui, |ui| {
                    if 新建 {
                        self.fresh_ui(ui);
                    } else {
                        self.move_ui(ui, site, priorities);
                    }
                });
            }
        }
    }

    /// 新建那一档底下：名字框与那一句说明。
    fn fresh_ui(&mut self, ui: &mut egui::Ui) {
        let 宽 = ui.available_width();
        look::text_input(
            ui,
            宽,
            egui::TextEdit::singleline(&mut self.name).hint_text("新作品的名字"),
        );
        look::help(
            ui,
            &match &self.platform {
                Some(platform) => format!(
                    "平台沿用 {platform}。新作品的元数据在下次刮削时补齐，\
                     也可以之后手动编辑。"
                ),
                None => "新作品的元数据在下次刮削时补齐，也可以之后手动编辑。".to_string(),
            },
        );
    }

    /// 已有那一档底下：搜索框与搜出来的那一框（设计稿 `.srch`，差距清单 `M-20`）——每行一枚圆点、平台色标、粗体名、
    /// 「年份 · N 个变体」，挑中的那一行 `accent-soft` 底；整行按下去就挑它。
    fn move_ui(&mut self, ui: &mut egui::Ui, site: &Site, priorities: &Priorities) {
        let 宽 = ui.available_width();
        look::text_input(
            ui,
            宽,
            egui::TextEdit::singleline(&mut self.query).hint_text("搜索作品名称"),
        );
        self.search(site, priorities);
        let 名字号 = look::font_size(ui.ctx(), Tokens::builtin().font.size_small_plus);
        let 挑中的 = self.target.as_ref().map(|one| one.work.clone());
        let 挑 = look::search_list(
            ui,
            "移入另一个作品",
            self.found.len(),
            "没有匹配的作品",
            |ui, at| {
                let one = &self.found[at];
                let 是它 = 挑中的.as_deref() == Some(one.work.as_str());
                look::radio_mark(ui, 是它);
                for platform in &one.platforms {
                    super::work::platform_chip(ui, platform);
                }
                ui.label(
                    font::strong(&one.title)
                        .size(名字号)
                        .color(look::palette(ui).ink),
                );
                look::help(
                    ui,
                    &format!(
                        "{} · {} 个变体",
                        one.year.as_deref().unwrap_or("年份未知"),
                        thousands(one.variants),
                    ),
                );
                是它
            },
        );
        if let Some(at) = 挑 {
            self.target = self.found.get(at).cloned();
        }
    }

    /// 「移出后会发生什么」那一块。
    fn impact_ui(&self, ui: &mut egui::Ui) {
        look::section(ui, "移出后会发生什么");
        let 去 = self.target_title().unwrap_or_else(|| "…".to_string());
        look::impact(
            ui,
            &[
                ("写入 ", false),
                ("1 条裁决", true),
                (
                    &format!(
                        "：把这个变体的作品改为「{去}」，发行版信息不变。\
                         裁决按文件内容记录，文件改名或移动后仍然有效。"
                    ),
                    false,
                ),
            ],
        );
        // 照稿 `DLG.split` 那一条：还剩几个 + 首选变体改成谁。
        // 两个数都是核心库答的（`merge::remaining` / `merge::preferred_pick`）；括号那截照旧留着（`F-9` 裁 B）。
        let mut 剩 = if self.left > 0 {
            format!("「{}」还剩 {} 个变体。", self.from, thousands(self.left))
        } else {
            format!("「{}」没有变体了，会从浏览列表中消失。", self.from)
        };
        if let Some(next) = &self.next_preferred {
            剩.push_str(&format!(
                "移走的正是它眼下的首选变体，改由「{next}」顶上（照「汉化 > 官中 > 日版 > 其他」重选）。"
            ));
        }
        if self.left > 0 {
            look::impact(ui, &[(&剩, false)]);
        } else {
            look::impact_warn(ui, &[(&剩, false)]);
        }
        look::impact(
            ui,
            &[(
                "收藏、合集按内容记录，跟着变体走；手动修改过的元数据留在原来的作品上。",
                false,
            )],
        );
        look::impact(
            ui,
            &[
                ("可以在「待确认 → 裁决记录」中", false),
                ("撤销", true),
                ("，变体回到原来的作品。", false),
            ],
        );
    }

    /// 「移入另一个作品」搜一趟。框里的字没变就不重搜。
    ///
    /// **同平台的排前**（设计稿 `DLG.split` 那一手 `sort`）：先照同一个搜索词、只要这个平台（`WorkQuery::platform`）取一页，
    /// 再照不筛平台取一页接在后头——「哪个作品在这个平台上」由核心库那一处查询答，这一层不另判。
    /// 屏上那几样（显示标题、平台、年份、变体数）再按**不过筛选**的口径取一遍（`Catalog::work_rows`）：
    /// 只筛平台那一页数出来的变体数是那个平台上的，不是整个作品的。
    fn search(&mut self, site: &Site, priorities: &Priorities) {
        if self.found_for.as_deref() == Some(self.query.as_str()) {
            return;
        }
        self.found_for = Some(self.query.clone());
        self.found.clear();
        let 最多 = 列几行(&self.query, SPLIT_BROWSED);
        let mut 次序: Vec<(WorkAnchor, String)> = Vec::new();
        let 不是自己 = |row: &romcat_core::catalog::browse::WorkRow| {
            Some(&row.name) != self.from_work.as_ref()
        };
        let 搜 = WorkQuery {
            search: self.query.clone(),
            ..WorkQuery::default()
        };
        let mut 读 = Ok(());
        if let Some(platform) = &self.platform {
            let 同平台 = WorkQuery {
                platform: Some(PlatformFilter::Named(platform.clone())),
                ..搜.clone()
            };
            读 = 收作品(&site.catalog, &同平台, 最多, 不是自己, &mut 次序);
        }
        读 = 读.and_then(|()| 收作品(&site.catalog, &搜, 最多, 不是自己, &mut 次序));
        let rows = 读.and_then(|()| {
            site.catalog
                .work_rows(&WorkQuery::default(), &次序, priorities)
        });
        match rows {
            Ok(rows) => {
                self.found = rows
                    .into_iter()
                    .map(|row| AnotherWork {
                        title: row.display.clone().unwrap_or_else(|| row.name.clone()),
                        work: row.name,
                        platforms: row.platforms,
                        year: row.year,
                        variants: row.variants,
                    })
                    .collect();
            }
            Err(failed) => self.error = Some(format!("中立库读不动：{failed}")),
        }
    }
}

/// 「移到」那一档底下缩进 `mode-indent` 的那一块（设计稿 `.field` 的 `padding-left:28px`）。
fn 缩进一格(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    let 缩进 = Tokens::builtin().layout.mode_indent;
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 缩进 as i8,
            ..egui::Margin::ZERO
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// 两处搜索框共用的那一趟：照 `query` 一页一页往下翻，把**认出了作品**、`留` 说要的那几行的身份接到 `次序` 后头，
/// 接到一共 `要几个` 或翻到底为止；`次序` 里已经有的不再接。
///
/// **只收认出了作品的那些**：还没认出作品的那一行在浏览屏上勾得中（合并的被合并一侧收它），
/// 但这两个框搜的是「作品」——搜出一堆路径来对人没有用。**它们在列表里一个变体一行**，真库里一页可能大半是它们，
/// 所以不能只取一页再滤（滤完常常凑不满）；也不能一直翻下去——框里每改一个字就搜一趟——最多翻 [`最多翻几页`] 页。
fn 收作品(
    catalog: &Catalog,
    query: &WorkQuery,
    要几个: usize,
    留: impl Fn(&romcat_core::catalog::browse::WorkRow) -> bool,
    次序: &mut Vec<(WorkAnchor, String)>,
) -> Result<(), CatalogError> {
    for 第几页 in 0..最多翻几页 {
        if 次序.len() >= 要几个 {
            break;
        }
        let 页 = catalog.work_page(query, 第几页 * 一页几行, 一页几行)?;
        let 到底了 = (页.len() as u64) < 一页几行;
        for row in 页 {
            if 次序.len() >= 要几个 {
                break;
            }
            let 认得出 = matches!(row.anchor, WorkAnchor::Work(_));
            if 认得出 && 留(&row) && !次序.iter().any(|(anchor, _)| *anchor == row.anchor) {
                次序.push((row.anchor, row.name));
            }
        }
        if 到底了 {
            break;
        }
    }
    Ok(())
}

/// [`收作品`] 一页取几行。
const 一页几行: u64 = 64;

/// [`收作品`] 最多翻几页：再往下还凑不满，就用手上那些。
const 最多翻几页: u64 = 16;
