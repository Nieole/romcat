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
//! 那句为什么由核心库答（[`merge::auto_absorb_reason`]），这一层只印出来。

use std::collections::{BTreeMap, BTreeSet};

use romcat_core::catalog::browse::{WorkAnchor, WorkDetail, WorkQuery, WorkVariant};
use romcat_core::catalog::identify::Tier;
use romcat_core::catalog::{Catalog, CatalogError};
use romcat_core::filename::Rules;
use romcat_core::report::{human_bytes, thousands};
use romcat_core::scrape::{Field, Priorities};
use romcat_core::site::Site;
use romcat_core::triage::merge::{self, Kind};

use crate::dialog::{Button, Dialog, Footer, Width};
use crate::font;
use crate::look;
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

/// 第一步「添加其他作品」最多列几行。稿上没搜时 4 行、搜了 8 行；这里一律取 8——
/// 弹层内容区自己滚得动，而少列几行只会让人以为库里没有。
const FOUND: usize = 8;

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
    /// 屏上那个名字。
    title: String,
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
        Ok(Some(Self {
            anchor: anchor.clone(),
            work,
            title: title_of(&detail, rules),
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

/// 这一行屏上叫什么：认不出作品的那一行是它的**正题**（与表上那一行主栏同一处剥），
/// 认出作品的就是**作品名**——核心库那一侧按名字认，屏上印别的名字会让人对不上。
fn title_of(detail: &WorkDetail, rules: &Rules) -> String {
    detail.title(rules).unwrap_or_else(|| detail.name.clone())
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
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        site: &Site,
        priorities: &Priorities,
    ) -> Option<Done> {
        // **页脚先搭好再画内容区**（共用弹层那一层的规矩）：按不按得动看的是这一帧画之前的状态。
        let 走得了 = match self.step {
            0 => self.rows.len() >= 2 && self.keep_work().is_some(),
            1 => !self.moving().is_empty(),
            _ => self.planned.as_ref().is_some_and(|one| one.verdicts() > 0),
        };
        let 几个 = self.rows.len();
        let mut footer = Footer::new(Button::new("取消", Pressed::Cancel));
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
                    0 => self.step_pick(ui, site, priorities),
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
    fn step_pick(&mut self, ui: &mut egui::Ui, site: &Site, priorities: &Priorities) {
        look::help(
            ui,
            "选择要保留的作品。其他作品的变体会归入它，它们的名称保留为别名。\
             只写入裁决记录，不会移动或修改任何文件。",
        );
        ui.add_space(look::step(1));
        let mut 换保留 = None;
        let mut 去掉 = None;
        for (at, row) in self.rows.iter().enumerate() {
            let 是保留 = at == self.keep;
            // 还没认出作品的那一行**当不了保留的那一侧**：它没有作品名可以留作别名。
            let 当得了 = row.work.is_some();
            look::card(ui, egui::Vec2::splat(look::step(2)), |ui| {
                ui.horizontal(|ui| {
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
                    if look::radio_option(ui, 是保留, &row.title, &一句).clicked() && 当得了
                    {
                        换保留 = Some(at);
                    }
                    if 是保留 {
                        table::tag(ui, "保留");
                    }
                    if !当得了 {
                        look::help(ui, "还没认出作品，当不了保留的那一个");
                    }
                    if self.rows.len() > 2 {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if look::small_ghost_button(ui, "移除").clicked() {
                                去掉 = Some(at);
                            }
                        });
                    }
                });
            });
        }
        if let Some(at) = 换保留 {
            self.keep = at;
            // 换了保留的那一个，第二步挑的首选与第三步选的值都跟着作废。
            self.picked_preferred.clear();
            self.defaults_stale = true;
            self.picks.clear();
        }
        if let Some(at) = 去掉 {
            self.rows.remove(at);
            if self.keep >= self.rows.len() || self.keep == at {
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
        ui.add_space(look::step(2));
        look::section(ui, "添加其他作品");
        let 宽 = ui.available_width();
        look::text_input(
            ui,
            宽,
            egui::TextEdit::singleline(&mut self.query).hint_text("搜索名称"),
        );
        self.search(site, priorities);
        let mut 加 = None;
        for row in &self.found {
            ui.horizontal(|ui| {
                // 照稿 `mwResults` 那一行：平台标签 + 名字 + 年份 · 变体数。
                table::tag(ui, &平台那一句(row));
                ui.label(&row.title);
                look::help(
                    ui,
                    &format!(
                        "{} · {} 个变体",
                        row.year.as_deref().unwrap_or("年份未知"),
                        thousands(row.variants.len() as u64),
                    ),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if look::small_buttons(ui, |ui| ui.button("添加").clicked()) {
                        加 = Some(row.clone());
                    }
                });
            });
        }
        if self.found.is_empty() {
            look::help(ui, "没有匹配的作品");
        }
        if let Some(row) = 加 {
            self.rows.push(row);
            self.found_for = None;
            self.query.clear();
            self.defaults_stale = true;
        }
    }

    /// 「添加其他作品」搜一趟。**框里的字没变就不重搜**——这一层每帧都画。
    fn search(&mut self, site: &Site, priorities: &Priorities) {
        if self.found_for.as_deref() == Some(self.query.as_str()) {
            return;
        }
        self.found_for = Some(self.query.clone());
        self.found.clear();
        let 已有: BTreeSet<&WorkAnchor> = self.rows.iter().map(|row| &row.anchor).collect();
        let 搜到的 = match 搜作品(site, priorities, &self.query, 已有.len()) {
            Ok(rows) => rows,
            Err(failed) => {
                self.error = Some(format!("中立库读不动：{failed}"));
                return;
            }
        };
        for (anchor, _) in 搜到的 {
            if 已有.contains(&anchor) || self.found.len() >= FOUND {
                continue;
            }
            match Row::load(
                &site.catalog,
                &WorkQuery::default(),
                &Rules::builtin(),
                priorities,
                &anchor,
            ) {
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
            look::card(ui, egui::Vec2::splat(look::step(2)), |ui| {
                ui.horizontal(|ui| {
                    table::tag(ui, &platform);
                    look::help(ui, &format!("{} 个变体", thousands(还剩 as u64)));
                });
                look::divider(ui);
                // **一行摆成明说了宽的几列**（设计稿 `.vrow` 的 `grid-template-columns`）：
                // 跟着前面的字流走的话，名字长一点后面整排就往右挪一点，四行之间永远参差
                // （拿主意的人 2026-09-21 看图挑出）。列宽取令牌 `merge-row-columns`，
                // 变体那一列占余下的——与手动例外那张表（`sublibrary::exception_table_ui`）
                // 同一种摆法。
                let [来自宽, 置信宽, 体积宽, 首选宽] = Tokens::builtin().layout.merge_row_columns;
                let 变体宽 = (ui.available_width()
                    - 来自宽
                    - 置信宽
                    - 体积宽
                    - 首选宽
                    - 4.0 * ui.spacing().item_spacing.x)
                    .max(0.0);
                for (at, variant) in 这组 {
                    let 自带 = at == self.keep;
                    let mut 勾着 = 自带 || !self.excluded.contains(&variant.key);
                    ui.horizontal_top(|ui| {
                        // **取消勾选的那一行整行调淡**（照稿 `.vrow.off{opacity:.5}`）：
                        // 「首选」那颗在这一行上按不动，而按不动的理由得在屏上常驻、
                        // 不能只挂在悬停里（ADR-0005 再修订那一节的第 2 条）。
                        if !勾着 {
                            ui.set_opacity(0.5);
                        }
                        列(ui, 变体宽, &mut |ui| {
                            // **勾选框把变体简称当标签**（稿上那是分开的两栏）：egui 里一个
                            // 光秃秃的小方块是个很小的靶子，而这一步正是要人逐个点过去。
                            // **保留作品自己的变体勾不掉**：它们本来就在那儿，这一层不是删东西的地方。
                            if ui
                                .add_enabled(
                                    !自带,
                                    egui::Checkbox::new(&mut 勾着, font::strong(&variant.short)),
                                )
                                .on_disabled_hover_text(merge::ALREADY_HELD)
                                .changed()
                            {
                                翻 = Some(variant.key.clone());
                            }
                            look::help(ui, &variant.key);
                        });
                        列(ui, 来自宽, &mut |ui| {
                            look::help(
                                ui,
                                &if 自带 {
                                    "保留作品自带".to_string()
                                } else {
                                    format!("来自「{}」", self.rows[at].title)
                                },
                            );
                        });
                        列(ui, 置信宽, &mut |ui| {
                            // **横着摆**：那一枚是一道色条加一个词，竖排那一层会把它拆成两行。
                            ui.horizontal(|ui| look::tier_tag(ui, variant.tier));
                        });
                        列(ui, 体积宽, &mut |ui| {
                            look::help(ui, &human_bytes(variant.bytes));
                        });
                        列(ui, 首选宽, &mut |ui| {
                            let 是首选 = 首选.as_deref() == Some(variant.key.as_str());
                            if ui
                                .add_enabled(勾着, egui::RadioButton::new(是首选, "首选"))
                                .on_disabled_hover_text(NO_PREFER)
                                .clicked()
                            {
                                挑.push((platform.clone(), variant.key.clone()));
                            }
                        });
                    });
                }
            });
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
    fn step_confirm(&mut self, ui: &mut egui::Ui) {
        let keep = self.keep_work().unwrap_or_default().to_string();
        look::section(ui, "字段冲突");
        look::help(
            ui,
            "默认保留作品的值；保留作品缺少的字段，默认用其他作品的值补上。",
        );
        if self.conflicts.is_empty() {
            look::help(ui, "没有冲突的字段。");
        }
        let mut 选 = Vec::new();
        // **一张三栏表**（设计稿 `.ctbl`）：字段 / 保留作品 / 其他作品，一个字段一行。
        // 一个字段摆一张卡的话，五个冲突就把「合并后会发生什么」整块推到折叠线以下——
        // 而那一块正是这一步要人看清的东西。
        look::card(ui, egui::Vec2::splat(look::step(2)), |ui| {
            // **三列明说宽度、摊满这一层**（设计稿 `.ctbl` 的 `<th style="width:90px">` 加两列
            // 分余下的）：由着 `Grid` 按内容收窄的话，表框画满整宽而内容只到三分之二，
            // 右边空着一大块（拿主意的人 2026-09-21 看图挑出）。
            let 缝 = look::step(3);
            let 字段宽 = Tokens::builtin().layout.conflict_key_width;
            let 余下 = (ui.available_width() - 字段宽 - 2.0 * 缝).max(0.0);
            let 值宽 = 余下 / 2.0;
            // **一行一行自己摆，不走 `egui::Grid`**：网格把每一格竖直居中，于是「标题」那一行
            // （右边两条选项、行高一倍）的字段名浮在半空，而稿上 `.ctbl td{vertical-align:top}`
            // 是**两边都靠上**（拿主意的人 2026-09-21 定，照稿）。`horizontal_top` 加明说了宽的
            // 几格才摆得出「两边都靠上」。隔行底色自己画：先占一个位子，量完这一行再填回去。
            ui.spacing_mut().item_spacing.y = look::step(1);
            ui.horizontal_top(|ui| {
                for (那几个字, 宽) in [("字段", 字段宽), ("保留作品", 值宽), ("其他作品", 值宽)]
                {
                    列(ui, 宽, &mut |ui| {
                        look::section(ui, 那几个字);
                    });
                    ui.add_space(缝);
                }
            });
            let 隔行底 = ui.visuals().faint_bg_color;
            for (第几行, conflict) in self.conflicts.iter().enumerate() {
                let 眼下 = self.pick_of(conflict);
                let 底 = ui.painter().add(egui::Shape::Noop);
                let 这一行 = ui
                    .horizontal_top(|ui| {
                        列(ui, 字段宽, &mut |ui| {
                            ui.label(font::strong(conflict.field.label()));
                        });
                        ui.add_space(缝);
                        列(ui, 值宽, &mut |ui| match &conflict.keep {
                            Some(said) => {
                                let 整句 = said.values.join("、");
                                if look::radio_option(ui, 眼下 == Pick::Keep, &剪一段(&整句), "")
                                    .on_hover_text(&整句)
                                    .clicked()
                                {
                                    选.push((conflict.field, Pick::Keep));
                                }
                            }
                            // 保留作品这一格空着时那一格**选不了**（稿上那颗单选钮是 `disabled`）：
                            // 没有值可用，默认就是别人的那一个。
                            None => {
                                look::help(ui, "（空）");
                            }
                        });
                        ui.add_space(缝);
                        列(ui, 值宽, &mut |ui| {
                            for (at, offer) in conflict.others.iter().enumerate() {
                                let 整句 = offer.said.values.join("、");
                                // **值与来源同字时不写两遍**：标题那一行的值常常就是作品名，
                                // 写成「某某 · 某某」读起来像出了错（审查挑出，2026-09-21 照改）。
                                let 这一格 = if 整句 == offer.work {
                                    剪一段(&整句)
                                } else {
                                    format!("{}  · {}", 剪一段(&整句), offer.work)
                                };
                                if look::radio_option(ui, 眼下 == Pick::Other(at), &这一格, "")
                                    .on_hover_text(&整句)
                                    .clicked()
                                {
                                    选.push((conflict.field, Pick::Other(at)));
                                }
                            }
                        });
                    })
                    .response
                    .rect;
                if 第几行 % 2 == 0 {
                    ui.painter().set(
                        底,
                        egui::Shape::rect_filled(
                            这一行.expand2(egui::vec2(0.0, look::step(1) / 2.0)),
                            0.0,
                            隔行底,
                        ),
                    );
                }
            }
        });
        for (field, at) in 选 {
            self.picks.insert(field, at);
        }

        ui.add_space(look::step(2));
        ui.checkbox(&mut self.alias, "把被合并作品的名称保留为别名");
        // **说清留的是哪几个名字**（Spec 轴挑出）：留的是**一个变体都不剩**的那几个
        // （核心库 `Impact::emptied`）。第二步取消掉某个变体、那个作品还剩变体时，
        // 它照旧在浏览列表里、名字就是它自己，不必记成别人的别名——但屏上得说出来，
        // 不然人以为勾了就每个都留。
        look::help(
            ui,
            &match self.impact.as_ref().map(|impact| impact.emptied.clone()) {
                Some(names) if !names.is_empty() => format!(
                    "留的是并空了的那几个：《{}》。搜这些名称仍能找到合并后的作品。",
                    names.join("》《"),
                ),
                _ => "这一趟没有作品会被并空：它们都还剩变体、照旧在浏览列表里，名字不必留作别名。"
                    .to_string(),
            },
        );
        // **「以后扫描到的也自动归入」画成不可选**，并写明原因与眼下的走法（票面 ⚠️ 第二条）。
        let mut 永不 = false;
        ui.add_enabled_ui(false, |ui| {
            ui.checkbox(
                &mut 永不,
                format!("以后扫描到被识别为这些作品的变体，也自动归入「{keep}」"),
            );
        });
        look::help(ui, &merge::auto_absorb_reason(&keep));

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
                        "：每个归入的变体一条，发行版信息不变，只把作品改为「{keep}」。\
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

/// 一行里**明说了多宽的一格**：把一竖排的内容框在这么宽里，于是上下几行同一列对得齐。
///
/// 与手动例外那张表（`sublibrary::exception_table_ui` 里那个 `cell`）同一种写法——
/// 跟着前面的字流走的话，前面长一点后面整排就往右挪一点。**对齐不靠数空格、不靠填空白**。
fn 列(ui: &mut egui::Ui, 宽: f32, add: &mut dyn FnMut(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(宽, 0.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(宽);
            add(ui);
        },
    );
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
    /// 从哪个作品移出：屏上那个名字。
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
    /// 搜出来的那几个作品名。
    found: Vec<String>,
    /// `found` 是照哪几个字搜的。
    found_for: Option<String>,
    /// 已有那一档：挑中的那个作品名。
    target: Option<String>,
    /// 这个作品移走这一个之后**还剩几个变体**——**核心库数的**（[`merge::remaining`]，
    /// 与合并第三步那句「并空了的那几个」同一处判，ADR-0024）。
    ///
    /// 不拿手上那一份 `WorkDetail::variants` 减一：那一份**随当前筛选收窄**，
    /// 一开筛屏上就会说错「没有变体了，会从浏览列表中消失」（Standards 轴挑出，2026-09-21 照改）。
    left: u64,
    /// 移走这一个之后，这个作品在这个平台上的**首选变体**照规则该是哪一个
    /// （核心库 [`merge::preferred_pick`]）；说不出、或者移走的本来就不是首选时是 `None`。
    next_preferred: Option<String>,
    /// 排好的计划。
    planned: Option<merge::Regrouping>,
    /// 这一层自己那句错。
    error: Option<String>,
}

impl Split {
    /// 从作品详情页某一张变体卡上开一层。
    #[must_use]
    pub fn open(
        catalog: &Catalog,
        work: &WorkDetail,
        rules: &Rules,
        key: &str,
        short: &str,
    ) -> Self {
        let 这一个 = work.variants.iter().find(|one| one.row.key == key);
        let platform = 这一个.and_then(|one| one.row.platform.clone());
        let title = title_of(work, rules);
        let from_work = match work.anchor {
            WorkAnchor::Work(_) => Some(work.name.clone()),
            WorkAnchor::Loose(_) => None,
        };
        let mut error = None;
        let mut 认: Box<dyn FnMut(CatalogError)> = Box::new(|failed: CatalogError| {
            error.get_or_insert_with(|| format!("中立库读不动：{failed}"));
        });

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
            .unwrap_or_else(|| short.to_string());
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
                        Ok(pick) => next_preferred = pick,
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
            short: short.to_string(),
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
        self.target.clone()
    }

    /// 画这一帧。
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        site: &Site,
        priorities: &Priorities,
    ) -> Option<Done> {
        let 去处 = self.target_work();
        let footer = Footer::new(Button::new("取消", Pressed::Cancel)).button(
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
            .width(Width::Wide)
            .show(ctx, |ui| {
                if let Some(说的) = self.error.clone() {
                    ui.colored_label(ui.visuals().error_fg_color, 说的);
                    ui.add_space(look::step(1));
                }
                // 照稿那一行（`DLG.split` 里那个 `.vrow`）：简称与位置在左，
                // 置信度与体积在右头。
                look::card(ui, egui::Vec2::splat(look::step(2)), |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(font::strong(&self.short));
                            look::help(ui, &self.key);
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            look::help(ui, &human_bytes(self.bytes));
                            look::tier_tag(ui, self.tier);
                        });
                    });
                });
                ui.add_space(look::step(1));
                look::section(ui, "移到");
                if look::radio_option(
                    ui,
                    self.fresh,
                    "新建一个作品",
                    "这个变体其实是另一个游戏（例如同名的不同作品、误归的改版）",
                )
                .clicked()
                {
                    self.fresh = true;
                }
                if self.fresh {
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
                            None => {
                                "新作品的元数据在下次刮削时补齐，也可以之后手动编辑。".to_string()
                            }
                        },
                    );
                }
                if look::radio_option(
                    ui,
                    !self.fresh,
                    "移入另一个作品",
                    "这个变体属于库里已有的另一个作品",
                )
                .clicked()
                {
                    self.fresh = false;
                }
                if !self.fresh {
                    let 宽 = ui.available_width();
                    look::text_input(
                        ui,
                        宽,
                        egui::TextEdit::singleline(&mut self.query).hint_text("搜索作品名称"),
                    );
                    self.search(site, priorities);
                    let mut 挑 = None;
                    for name in &self.found {
                        if look::radio_option(ui, self.target.as_deref() == Some(name), name, "")
                            .clicked()
                        {
                            挑 = Some(name.clone());
                        }
                    }
                    if self.found.is_empty() {
                        look::help(ui, "没有匹配的作品");
                    }
                    if let Some(name) = 挑 {
                        self.target = Some(name);
                    }
                }

                ui.add_space(look::step(2));
                look::section(ui, "移出后会发生什么");
                let 去 = 去处.clone().unwrap_or_else(|| "…".to_string());
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
                // 两个数都是核心库答的（`merge::remaining` / `merge::preferred_pick`）。
                let mut 剩 = if self.left > 0 {
                    format!("「{}」还剩 {} 个变体。", self.from, thousands(self.left))
                } else {
                    format!("「{}」没有变体了，会从浏览列表中消失。", self.from)
                };
                if let Some(next) = &self.next_preferred {
                    剩.push_str(&format!(
                        "移走的正是它眼下的首选变体，改由「{}」顶上（照「汉化 > 官中 > 日版 > 其他」重选）。",
                        romcat_core::path::file_name_of_key(next),
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

    /// 「移入另一个作品」搜一趟。框里的字没变就不重搜。
    fn search(&mut self, site: &Site, priorities: &Priorities) {
        if self.found_for.as_deref() == Some(self.query.as_str()) {
            return;
        }
        self.found_for = Some(self.query.clone());
        self.found.clear();
        match 搜作品(site, priorities, &self.query, 1) {
            Ok(rows) => {
                for (_, name) in rows {
                    if Some(&name) == self.from_work.as_ref() || self.found.len() >= FOUND {
                        continue;
                    }
                    self.found.push(name);
                }
            }
            Err(failed) => self.error = Some(format!("中立库读不动：{failed}")),
        }
    }
}

/// 两处搜索框共用的那一趟：按名字搜**作品**，交回 `(那一行的身份, 作品名)`，最多
/// `FOUND + 还要滤掉几个` 行。
///
/// **只交认出了作品的那些**：还没认出作品的那一行在浏览屏上勾得中（合并的被合并一侧收它），
/// 但这两个框搜的是「作品」——搜出一堆路径来对人没有用。
///
/// 多取 `skip` 行是因为调用方还要再滤一遍（已经在向导里的、正在移出的那个作品自己），
/// 不多取的话滤完就不够 [`FOUND`] 行。
fn 搜作品(
    site: &Site,
    priorities: &Priorities,
    text: &str,
    skip: usize,
) -> Result<Vec<(WorkAnchor, String)>, CatalogError> {
    let query = WorkQuery {
        search: text.to_string(),
        ..WorkQuery::default()
    };
    Ok(site
        .catalog
        .work_page_with_titles(&query, 0, (FOUND + skip) as u64, priorities)?
        .into_iter()
        .filter(|row| matches!(row.anchor, WorkAnchor::Work(_)))
        .map(|row| (row.anchor, row.name))
        .collect())
}
