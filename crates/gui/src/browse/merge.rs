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

/// 浏览屏工具条上那颗按钮上的字（设计稿 `#merge-btn`）。
pub const MERGE: &str = "合并作品…";

/// 作品详情页头上那一排里那颗按钮上的字（设计稿 `renderWD` 的 `data-mw="open:"`）。
pub const MERGE_ONE: &str = "合并…";

/// 变体卡头一行右头那颗按钮上的字（设计稿 `data-dg="open:split|…"`）。
pub const SPLIT: &str = "移出此作品…";

/// 勾得不够时说的那一句（设计稿 `#merge-btn` 那条 toast）。
pub const NEED_TWO: &str = "请勾选两个或更多作品，或在作品详情中点「合并…」";

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

/// 参与合并的一**行**：浏览表上的一行（一个作品，或一个还没认出作品的变体）。
#[derive(Debug, Clone)]
struct Party {
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
    variants: Vec<PartyVariant>,
}

/// 一行底下的一个变体。
#[derive(Debug, Clone)]
struct PartyVariant {
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

impl Party {
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
            // 一条候选都没有时是 `None`，[`Tier::of`] 把它折成「没有候选」那一档。
            tier: Tier::of(
                detail
                    .variants
                    .iter()
                    .filter_map(WorkVariant::confidence)
                    .min(),
            ),
            variants: detail
                .variants
                .iter()
                .zip(shorts)
                .map(|(variant, short)| PartyVariant {
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
    parties: Vec<Party>,
    /// 其中第几个是**保留的作品**。
    keep: usize,
    /// **取消勾选**的那几个变体：它们不归入，仍留在原来的作品里。
    excluded: BTreeSet<String>,
    /// 每个平台挑了哪个**首选变体**：平台 → 变体的键。
    preferred: BTreeMap<String, String>,
    /// 第一步「添加其他作品」框里打的字。
    query: String,
    /// 搜出来的那几行（不含已经在里头的）。
    found: Vec<Party>,
    /// `found` 是照哪几个字搜的。与框里的字不一样就重搜。
    found_for: Option<String>,
    /// 把被合并作品的名称保留为**别名**。
    alias: bool,
    /// 第三步：有冲突的那几个字段。
    conflicts: Vec<merge::Conflict>,
    /// 逐项选了谁：字段 → `others` 里的第几个；[`KEEP_VALUE`] 是保留作品那一格。
    picks: BTreeMap<Field, usize>,
    /// 第三步那份账（按「下一步」进第三步那一下算一次，**不每帧算**）。
    impact: Option<merge::Impact>,
    /// 排好的计划。
    planned: Option<merge::Regrouping>,
    /// 这一层自己那句错。
    error: Option<String>,
}

/// [`Wizard::picks`] 里表示「用保留作品那一格」的那个数。
const KEEP_VALUE: usize = usize::MAX;

impl Wizard {
    /// 从浏览屏勾中的那几行、或者从作品详情页那一个作品开一层。
    ///
    /// 读不出来的行（筛掉了、库重扫过了）悄悄略过——第一步自己会说「至少需要两个作品」。
    pub fn open(
        catalog: &Catalog,
        query: &WorkQuery,
        rules: &Rules,
        priorities: &Priorities,
        rows: &[WorkAnchor],
    ) -> Self {
        let mut parties = Vec::new();
        let mut error = None;
        for anchor in rows {
            match Party::load(catalog, query, rules, priorities, anchor) {
                Ok(Some(party)) => parties.push(party),
                Ok(None) => {}
                Err(failed) => error = Some(format!("中立库读不动：{failed}")),
            }
        }
        // **默认保留变体最多的那一个**（拿主意的人 2026-09-21 定）：合并的本意就是把少的
        // 并进多的，而这句话屏上解释得清。**稿上那套四项加权分不照抄**
        // （`bestOf`：置信度×2 + 有封面 + 元数据完整 + 变体数×0.1）——它是一条会落进
        // 界面里的新领域判断，而且屏上解释不了「凭什么推荐这一个」。
        // 同数时按屏上那个名字定序，于是同一批行开两次向导，默认推荐的是同一个。
        let keep = best_keep(&parties);
        Self {
            step: 0,
            parties,
            keep,
            excluded: BTreeSet::new(),
            preferred: BTreeMap::new(),
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
            let at = self.pick_of(conflict);
            if let Some(offer) = conflict.others.get(at) {
                out.push((conflict.field, offer.clone()));
            }
        }
        out
    }

    /// 第二步逐平台挑的**首选变体**：`(作品名, 平台, 变体的键)`。
    #[must_use]
    pub fn preferred(&self) -> Vec<(String, String, String)> {
        let Some(work) = self.keep_work() else {
            return Vec::new();
        };
        self.preferred
            .iter()
            .filter(|(_, key)| !self.excluded.contains(key.as_str()))
            .map(|(platform, key)| (work.to_string(), platform.clone(), key.clone()))
            .collect()
    }

    /// 保留的那个作品名；保留的那一行还没认出作品时是 `None`。
    fn keep_work(&self) -> Option<&str> {
        self.parties.get(self.keep)?.work.as_deref()
    }

    /// 这个字段眼下选的是哪一格。
    ///
    /// 默认照稿：**保留作品有值就用它的；保留作品那一格空着，就用别人的补上。**
    fn pick_of(&self, conflict: &merge::Conflict) -> usize {
        match self.picks.get(&conflict.field) {
            Some(at) => *at,
            None if conflict.keep.is_some() => KEEP_VALUE,
            None => 0,
        }
    }

    /// 这一批要归入的变体：参与的那几行底下的，减去保留作品自己的、减去取消勾选的。
    fn moving(&self) -> Vec<String> {
        self.parties
            .iter()
            .enumerate()
            .filter(|(at, _)| *at != self.keep)
            .flat_map(|(_, party)| party.variants.iter())
            .filter(|variant| !self.excluded.contains(&variant.key))
            .map(|variant| variant.key.clone())
            .collect()
    }

    /// 参与的那几行涉及哪几个平台，照走到的次序。
    fn platforms(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for party in &self.parties {
            for variant in &party.variants {
                if !out.contains(&variant.platform) {
                    out.push(variant.platform.clone());
                }
            }
        }
        out
    }

    /// 这个平台上默认挑谁当**首选变体**：保留作品自己那几个里的头一个，
    /// 一个都没有就是这个平台上的头一个。**真正的首选规则在核心库**
    /// （`converge::preference_for`，汉化 > 官中 > 日版 > 其他）——这里只给一个起手的默认值，
    /// 落下去那一条裁决说的是「人挑了这一个」。
    fn default_preferred(&self, platform: &str) -> Option<String> {
        let mut first = None;
        for (at, party) in self.parties.iter().enumerate() {
            for variant in &party.variants {
                if variant.platform != platform || self.excluded.contains(&variant.key) {
                    continue;
                }
                if at == self.keep {
                    return Some(variant.key.clone());
                }
                first.get_or_insert_with(|| variant.key.clone());
            }
        }
        first
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
            .parties
            .iter()
            .enumerate()
            .filter(|(at, _)| *at != self.keep)
            .filter_map(|(_, party)| party.work.clone())
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
            0 => self.parties.len() >= 2 && self.keep_work().is_some(),
            1 => !self.moving().is_empty(),
            _ => self.planned.as_ref().is_some_and(|one| one.verdicts() > 0),
        };
        let 几个 = self.parties.len();
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
                    1 => self.step_variants(ui),
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
        for (at, party) in self.parties.iter().enumerate() {
            let 是保留 = at == self.keep;
            // 还没认出作品的那一行**当不了保留的那一侧**：它没有作品名可以留作别名。
            let 当得了 = party.work.is_some();
            look::card(ui, egui::Vec2::splat(look::step(2)), |ui| {
                ui.horizontal(|ui| {
                    // 照稿那一行（`.mwit` 里那句 `help`）：平台 · 年份 · 几个变体。
                    let 一句 = format!(
                        "{} · {} · {} 个变体",
                        平台们(party),
                        party.year.as_deref().unwrap_or("年份未知"),
                        thousands(party.variants.len() as u64),
                    );
                    if look::radio_option(ui, 是保留, &party.title, &一句).clicked() && 当得了
                    {
                        换保留 = Some(at);
                    }
                    // 置信度那一档也照稿摆在名字后头。哪一档由核心库答（`WorkVariant::confidence`
                    // 取最高的那一条，与表上那一行同一条口径），这里只印。
                    look::tier_tag(ui, party.tier);
                    if 是保留 {
                        table::tag(ui, "保留");
                    }
                    if !当得了 {
                        look::help(ui, "还没认出作品，当不了保留的那一个");
                    }
                    if self.parties.len() > 2 {
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
            self.preferred.clear();
            self.picks.clear();
        }
        if let Some(at) = 去掉 {
            self.parties.remove(at);
            if self.keep >= self.parties.len() || self.keep == at {
                self.keep = best_keep(&self.parties);
            } else if self.keep > at {
                self.keep -= 1;
            }
            self.preferred.clear();
        }
        if self.parties.len() < 2 {
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
        for party in &self.found {
            ui.horizontal(|ui| {
                ui.label(&party.title);
                look::help(
                    ui,
                    &format!("{} 个变体", thousands(party.variants.len() as u64)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if look::small_buttons(ui, |ui| ui.button("添加").clicked()) {
                        加 = Some(party.clone());
                    }
                });
            });
        }
        if self.found.is_empty() {
            look::help(ui, "没有匹配的作品");
        }
        if let Some(party) = 加 {
            self.parties.push(party);
            self.found_for = None;
            self.query.clear();
            self.preferred.clear();
        }
    }

    /// 「添加其他作品」搜一趟。**框里的字没变就不重搜**——这一层每帧都画。
    fn search(&mut self, site: &Site, priorities: &Priorities) {
        if self.found_for.as_deref() == Some(self.query.as_str()) {
            return;
        }
        self.found_for = Some(self.query.clone());
        self.found.clear();
        let query = WorkQuery {
            search: self.query.clone(),
            ..WorkQuery::default()
        };
        let 已有: BTreeSet<&WorkAnchor> = self.parties.iter().map(|party| &party.anchor).collect();
        let rows = match site.catalog.work_page_with_titles(
            &query,
            0,
            (FOUND + 已有.len()) as u64,
            priorities,
        ) {
            Ok(rows) => rows,
            Err(failed) => {
                self.error = Some(format!("中立库读不动：{failed}"));
                return;
            }
        };
        for row in rows {
            if 已有.contains(&row.anchor) || self.found.len() >= FOUND {
                continue;
            }
            // 只有认出作品的那些加得进来当被合并的一侧也好、保留的一侧也好——散着的那一行
            // 在浏览屏上勾得中，这个搜索框里不列：搜的是「作品」。
            if !matches!(row.anchor, WorkAnchor::Work(_)) {
                continue;
            }
            match Party::load(
                &site.catalog,
                &WorkQuery::default(),
                &Rules::builtin(),
                priorities,
                &row.anchor,
            ) {
                Ok(Some(party)) => self.found.push(party),
                Ok(None) => {}
                Err(failed) => self.error = Some(format!("中立库读不动：{failed}")),
            }
        }
    }

    /// 第二步：**核对变体**，按平台分组，每组各挑一个首选。
    fn step_variants(&mut self, ui: &mut egui::Ui) {
        look::help(
            ui,
            "取消勾选的变体不会合并，仍留在原来的作品中。首选变体是前端默认启动的那一个，每个平台一个。",
        );
        ui.add_space(look::step(1));
        let mut 翻 = None;
        let mut 挑 = Vec::new();
        for platform in self.platforms() {
            let 这组: Vec<(usize, &PartyVariant)> = self
                .parties
                .iter()
                .enumerate()
                .flat_map(|(at, party)| party.variants.iter().map(move |one| (at, one)))
                .filter(|(_, one)| one.platform == platform)
                .collect();
            let 还剩 = 这组
                .iter()
                .filter(|(at, one)| *at == self.keep || !self.excluded.contains(&one.key))
                .count();
            let 首选 = self
                .preferred
                .get(&platform)
                .cloned()
                .or_else(|| self.default_preferred(&platform));
            look::card(ui, egui::Vec2::splat(look::step(2)), |ui| {
                ui.horizontal(|ui| {
                    table::tag(ui, &platform);
                    look::help(ui, &format!("{} 个变体", thousands(还剩 as u64)));
                });
                look::divider(ui);
                for (at, variant) in 这组 {
                    let 自带 = at == self.keep;
                    let mut 勾着 = 自带 || !self.excluded.contains(&variant.key);
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            // **勾选框把变体简称当标签**（稿上那是分开的两栏）：egui 里一个
                            // 光秃秃的小方块是个很小的靶子，而这一步正是要人逐个点过去。
                            // **保留作品自己的变体勾不掉**：它们本来就在那儿，这一层不是删东西的地方。
                            if ui
                                .add_enabled(
                                    !自带,
                                    egui::Checkbox::new(&mut 勾着, font::strong(&variant.short)),
                                )
                                .on_disabled_hover_text("保留作品自己的变体")
                                .changed()
                            {
                                翻 = Some(variant.key.clone());
                            }
                            look::help(ui, &variant.key);
                        });
                        look::help(
                            ui,
                            &if 自带 {
                                "保留作品自带".to_string()
                            } else {
                                format!("来自「{}」", self.parties[at].title)
                            },
                        );
                        look::tier_tag(ui, variant.tier);
                        look::help(ui, &human_bytes(variant.bytes));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let 是首选 = 首选.as_deref() == Some(variant.key.as_str());
                            if ui
                                .add_enabled(勾着, egui::RadioButton::new(是首选, "首选"))
                                .clicked()
                            {
                                挑.push((platform.clone(), variant.key.clone()));
                            }
                        });
                    });
                }
            });
            // 默认那一个也记下来：落下去那一条裁决说的是「这个平台默认启动这一个」，
            // 人没动过时记的就是屏上画着的那一个。
            if let Some(key) = 首选 {
                self.preferred.entry(platform).or_insert(key);
            }
        }
        if let Some(key) = 翻 {
            if !self.excluded.remove(&key) {
                self.excluded.insert(key);
            }
            self.preferred.clear();
        }
        for (platform, key) in 挑 {
            self.preferred.insert(platform, key);
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
            egui::Grid::new("字段冲突")
                .num_columns(3)
                .striped(true)
                .spacing(egui::vec2(look::step(3), look::step(1)))
                .show(ui, |ui| {
                    look::section(ui, "字段");
                    look::section(ui, "保留作品");
                    look::section(ui, "其他作品");
                    ui.end_row();
                    for conflict in &self.conflicts {
                        let 眼下 = self.pick_of(conflict);
                        ui.label(font::strong(conflict.field.label()));
                        // **每一格里再起一竖**：[`look::radio_option`] 头一句是 `add_space`，
                        // 而 egui 的网格布局上 `add_space` 当场炸（「add_space makes no sense
                        // in a grid layout」）。竖排那一层不是网格，摆得下。
                        ui.vertical(|ui| match &conflict.keep {
                            Some(said) => {
                                let 整句 = said.values.join("、");
                                if look::radio_option(ui, 眼下 == KEEP_VALUE, &剪一段(&整句), "")
                                    .on_hover_text(&整句)
                                    .clicked()
                                {
                                    选.push((conflict.field, KEEP_VALUE));
                                }
                            }
                            // 保留作品这一格空着时那一格**选不了**（稿上那颗单选钮是 `disabled`）：
                            // 没有值可用，默认就是别人的那一个。
                            None => {
                                look::help(ui, "（空）");
                            }
                        });
                        ui.vertical(|ui| {
                            for (at, offer) in conflict.others.iter().enumerate() {
                                let 整句 = offer.said.values.join("、");
                                let 这一格 = format!("{}  · {}", 剪一段(&整句), offer.work);
                                if look::radio_option(ui, 眼下 == at, &这一格, "")
                                    .on_hover_text(&整句)
                                    .clicked()
                                {
                                    选.push((conflict.field, at));
                                }
                            }
                        });
                        ui.end_row();
                    }
                });
        });
        for (field, at) in 选 {
            self.picks.insert(field, at);
        }

        ui.add_space(look::step(2));
        ui.checkbox(&mut self.alias, "把被合并作品的名称保留为别名");
        look::help(ui, "搜索这些名称仍能找到合并后的作品");
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
fn best_keep(parties: &[Party]) -> usize {
    parties
        .iter()
        .enumerate()
        .max_by_key(|(at, party)| {
            (
                party.work.is_some(),
                party.variants.len(),
                std::cmp::Reverse(party.title.clone()),
                std::cmp::Reverse(*at),
            )
        })
        .map_or(0, |(at, _)| at)
}

/// 这一行涉及哪几个平台，写成一句。
fn 平台们(party: &Party) -> String {
    let mut out: Vec<&str> = Vec::new();
    for variant in &party.variants {
        if !out.contains(&variant.platform.as_str()) {
            out.push(&variant.platform);
        }
    }
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
    /// 这个作品移走这一个之后还剩几个变体。
    left: usize,
    /// 排好的计划。
    planned: Option<merge::Regrouping>,
    /// 这一层自己那句错。
    error: Option<String>,
}

impl Split {
    /// 从作品详情页某一张变体卡上开一层。
    #[must_use]
    pub fn open(
        work: &WorkDetail,
        rules: &Rules,
        key: &str,
        short: &str,
        platform: Option<&str>,
    ) -> Self {
        let 这一个 = work.variants.iter().find(|one| one.row.key == key);
        let title = title_of(work, rules);
        // 新建那一档的默认名字照稿：原作品名加上变体简称头一段（「幻想传说（汉化版）」）。
        let 起名 = format!("{title}（{}）", short.split(" · ").next().unwrap_or(short));
        Self {
            from: title,
            from_work: match work.anchor {
                WorkAnchor::Work(_) => Some(work.name.clone()),
                WorkAnchor::Loose(_) => None,
            },
            key: key.to_string(),
            short: short.to_string(),
            platform: platform.map(ToString::to_string),
            tier: Tier::of(这一个.and_then(WorkVariant::confidence)),
            bytes: 这一个.map_or(0, |one| one.row.bytes),
            fresh: true,
            name: 起名,
            query: String::new(),
            found: Vec::new(),
            found_for: None,
            target: None,
            left: work.variants.len().saturating_sub(1),
            planned: None,
            error: None,
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
                let 剩 = if self.left > 0 {
                    format!(
                        "「{}」还剩 {} 个变体。",
                        self.from,
                        thousands(self.left as u64)
                    )
                } else {
                    format!("「{}」没有变体了，会从浏览列表中消失。", self.from)
                };
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
        let query = WorkQuery {
            search: self.query.clone(),
            ..WorkQuery::default()
        };
        match site
            .catalog
            .work_page_with_titles(&query, 0, (FOUND + 1) as u64, priorities)
        {
            Ok(rows) => {
                for row in rows {
                    if !matches!(row.anchor, WorkAnchor::Work(_))
                        || Some(&row.name) == self.from_work.as_ref()
                        || self.found.len() >= FOUND
                    {
                        continue;
                    }
                    self.found.push(row.name);
                }
            }
            Err(failed) => self.error = Some(format!("中立库读不动：{failed}")),
        }
    }
}
