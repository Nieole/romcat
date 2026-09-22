//! **作品详情页**（票 `gui-looks-like-the-design/15`，设计稿 `renderWD`）：一部作品的全部情况在这一层里看得完、
//! 改得动。双击主列表一行、或者在侧边详情里点「查看详情」打开；「← 返回浏览」回到三栏。
//!
//! ## 盖住的是整块屏
//!
//! 稿上 `.wd` 铺满 `.scr`：浏览屏的屏头与三栏都被它盖住，左栏与底部状态栏照旧。于是它开着时窗口不画浏览屏的
//! 屏头，它顶上那一条（「← 返回浏览」、这是哪个作品、第几个、上一个 / 下一个）就是这一层的屏头。
//!
//! ## 数据是侧边详情那一份，外加每个变体的详情
//!
//! 点开的是哪一行、那一行底下有哪些变体，是浏览屏手上现成的那一份——换「上一个 / 下一个」就是在表上换一行点开。
//! 侧边详情只读选中那一个变体的详情，详情页要把每个变体都摆出来，于是另读一份（`Page` 里那几份），点开的那一行
//! 一重读就跟着作废。**判断一条都不在这儿**：变体简称、首选变体、置信度那个词、汉化组挑哪一条，都问核心库。

use std::collections::{BTreeMap, BTreeSet};

use egui::{Align, Layout};
use romcat_core::catalog::browse::{WorkAnchor, WorkDetail, WorkRow, WorkVariant};
use romcat_core::catalog::detail::{FileLine, MediaItem};
use romcat_core::catalog::export::ExportMark;
use romcat_core::catalog::identify::{Candidate, NOT_RUN_LABEL, State, Tier};
use romcat_core::catalog::roots::Roots;
use romcat_core::catalog::scrape::ScrapedValue;
use romcat_core::catalog::{Catalog, Confidence, TitleRow, VariantDetail};
use romcat_core::dat::chinese::ChineseMark;
use romcat_core::fs::RealFs;
use romcat_core::report::{self, capacity, human_bytes, human_time, thousands};
use romcat_core::scrape::priority::{self, FieldShown, Said, VERDICT};
use romcat_core::scrape::{AnchorKind, Field, MediaKind, preview};
use romcat_core::shape::Role;
use romcat_core::site::Site;
use romcat_core::title::{Language, SortFrom, TitleKind, language_of};
use romcat_core::verdict::TitleSuppression;

use super::{
    HAND_WRITTEN, PREFERRED_TAG, Screen, TOP_CANDIDATES, TOP_MEMBERS, TOP_TITLES, TitleDraft,
};
use crate::font;
use crate::look;
use crate::media::{Clicked, Gallery, Look};
use crate::table;
use crate::tokens::Tokens;

/// 来源是**裁决**的值在屏上叫什么（设计稿 `srcBadge` 的「手动」，拿主意的人 2026-09-15 定照稿）。库里记的源名照旧是
/// `VERDICT`，只是屏上印这两个字。
const MANUAL: &str = "手动";

/// 那枚「手动」标签上悬停时说的话：它记成了什么、会怎样。
const MANUAL_HOVER: &str = "手动修改：记为一条裁决，优先于所有数据源，重新刮削不会覆盖。";

/// 显示标题是怎么选出来的，照标题集合挑它的那一处（`title::choose`）的回退链写（词表**显示标题**）：手动添加的（来源是裁决）压过一切，
/// 其后是中文译名、官方英文名、日文原名，最后才轮到文件名。
const DISPLAY_RULE: &str = "选取顺序：手动添加 > 中文译名 > 官方英文名 > 日文原名 > 文件名";

/// 有没存的改动时想离开这一页（返回、上一个、下一个）说的那一句（设计稿 `guardDirty`）。
const UNSAVED: &str = "有未保存的修改，请先保存或放弃";

/// 作品详情页上的一个**面**。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab {
    /// 概览：简介、基本信息、媒体、状态四块。
    #[default]
    Overview,
    /// 变体与文件：每个变体一张卡，发行版、位置、文件。
    Variants,
    /// 元数据：每个字段用的是哪个数据源的值，改写与撤销。
    Metadata,
    /// 标题：标题集合，显示标题与排序标题是怎么选出来的。
    Titles,
    /// 媒体：封面、截图、视频。
    Media,
    /// 识别依据：逐变体的候选、来源与置信度。
    Evidence,
}

impl Tab {
    /// 六个面，照稿上的次序。
    pub const ALL: [Self; 6] = [
        Self::Overview,
        Self::Variants,
        Self::Metadata,
        Self::Titles,
        Self::Media,
        Self::Evidence,
    ];

    /// 屏上这一面叫什么。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "概览",
            Self::Variants => "变体与文件",
            Self::Metadata => "元数据",
            Self::Titles => "标题",
            Self::Media => "媒体",
            Self::Evidence => "识别依据",
        }
    }
}

/// 作品详情页开着时记着的东西。
#[derive(Debug, Clone, Default)]
pub struct Page {
    /// 看的是哪一面。
    tab: Tab,
    /// 这个作品底下每个变体的详情，次序与点开那一行底下的变体一样。
    details: Vec<VariantDetail>,
    /// `details` 是照哪几个变体读的。与眼下点开那一行底下的变体不一样就重读。
    for_keys: Vec<String>,
    /// 这个条目上每个字段眼下写出去的是什么、各个源各说了什么（核心库 `priority::entry_fields`，与导出同一处）。
    /// 认出作品的挂作品，没认出的挂那个变体。
    fields: Vec<FieldShown>,
    /// 认出作品时，每个变体身上的汉化组那一格：`(变体的键, 那一格)`。汉化版、或者身上有汉化组值的才有一行。
    groups: Vec<(String, FieldShown)>,
    /// 「其他 N 个来源」摊开着的那几格：`(锚点, 字段)`。
    open_alts: BTreeSet<(String, Field)>,
    /// 「元数据」那一面**编辑态**时是 `Some`：每一格的草稿，键是 `(挂在哪一层, 作品名或变体的键, 字段)`。
    /// 草稿画到那一格时才从眼下写出去的值填进去；**只在这儿改**，库里的值一个字都不碰，直到按保存。
    editing: Option<BTreeMap<(AnchorKind, String, Field), Draft>>,
    /// 表上**这一行**（核心库 `Catalog::work_rows`，与表上那一行同一处算）：头上那几枚标签（置信度那个词、
    /// 「元数据：…」）与「变体 N 个 · 容量」那一格照它印。
    row: Option<WorkRow>,
    /// 头一个平台上的**首选变体**（核心库答的，`VariantDetail::preferred_now`）：「当前值」里汉化组从它身上取，
    /// 概览里「首选变体」写它。
    head: Option<String>,
    /// 每个变体的**文件表**那几行（核心库 `Catalog::file_lines`）：变体的键 → 那几行。
    files: BTreeMap<String, Vec<FileLine>>,
    /// 头上「平台」那一格印的字：作品的平台照核心库的平台表写全名（`Manifest::full_name`），没写全名的写代号，
    /// 几个平台之间「 / 」。
    platform_names: String,
    /// 这个作品**收没收藏**、收了的钉在哪种锚上（核心库 `collection::favorite_of`）：状态块里「收藏」那一行照它写，只读
    /// （拿主意的人 2026-09-15 定；收藏按钮与合集那一行归票 13）。没收藏是 `None`。
    favorite: Option<&'static str>,
    /// 这个作品的**中文版本**（核心库 `Catalog::work_chinese_mark`，与首选变体那条规则同一处判）：头上那枚标签与
    /// 基本信息里「中文版本」那一格照它印；一个中文的都没有是 `None`。
    chinese: Option<ChineseMark>,
    /// 这个作品底下那几个变体**落在哪几个子库的选择集里**（核心库 `sublibrary::holding`，
    /// 与子库屏、容量条、差量预览、真正同步那一趟**同一个求值函数**）：状态块里「子库」那一行照它写。
    /// 一个都没落进是空的。
    sublibraries: Vec<String>,
    /// 这个作品**上次写进前端格式是什么时候**（核心库 `Catalog::entry_exported`）：状态块里「导出」那一行照它写。
    /// **一趟都没导过、或者上次导出那会儿它还不在库里**都是 `None`——那与「整库导过了」不是同一件事。
    exported: Option<ExportMark>,
    /// 这几个变体跟前有没有一处**成型存疑**（核心库 `Catalog::shaping_doubts_near`，判据与库体检那一格
    /// 同一处，ADR-0024）：「变体」那一面头上那块建议照它画（票 `gui-looks-like-the-design/29`）。
    doubts: Vec<romcat_core::shape::Doubt>,
}

/// 编辑态下一格的草稿。
#[derive(Debug, Clone, Default)]
struct Draft {
    /// 进编辑态那一刻这一格写出去的是什么。
    original: String,
    /// 框里眼下是什么。
    text: String,
}

impl Draft {
    /// 改过没有：两头空白不算（设计稿 `dirtyKeys` 的 `trim`）。
    fn dirty(&self) -> bool {
        self.text.trim() != self.original.trim()
    }
}

impl Page {
    /// 看的是哪一面。
    #[must_use]
    pub fn tab(&self) -> Tab {
        self.tab
    }

    /// 编辑态下有没有改过、还没保存的格。
    fn dirty(&self) -> bool {
        self.editing
            .as_ref()
            .is_some_and(|drafts| drafts.values().any(Draft::dirty))
    }

    /// 手上那几份变体详情作废，下一帧照库里现在的样子重读。
    pub(super) fn forget(&mut self) {
        self.details.clear();
        self.for_keys.clear();
        self.fields.clear();
        self.groups.clear();
        self.row = None;
        self.head = None;
        self.files.clear();
        self.sublibraries.clear();
        self.exported = None;
        self.doubts.clear();
    }
}

impl Screen {
    /// 作品详情页开着时，那个作品屏上叫什么（与它顶上那一条「浏览 / 作品名」同一个）；没开着是 `None`。
    /// 窗口标题拿它（`App::window_title`）。
    #[must_use]
    pub fn page_title(&self) -> Option<String> {
        self.page.as_ref()?;
        self.work.as_ref().map(|work| self.work_title(work))
    }

    /// 作品详情页开着时是 `Some`。窗口拿它决定这一帧画不画浏览屏的屏头。
    #[must_use]
    pub fn page(&self) -> Option<&Page> {
        self.page.as_ref()
    }

    /// 打开**点开那一行**的作品详情页，停在 `tab` 那一面。一行都没点开时什么都不做。
    pub fn open_page(&mut self, tab: Tab) {
        if self.work.is_some() {
            self.page = Some(Page {
                tab,
                ..Page::default()
            });
        }
    }

    /// 关掉作品详情页，回到三栏。点开的那一行照旧点开着。
    ///
    /// **成型纠正那一层跟着关掉**：它只画在这一页上（`page_ui`），页关了它就没人画——
    /// 留着的话下次打开这一页会冒出一层上一次没关掉的弹层。
    pub fn close_page(&mut self) {
        self.page = None;
        self.fixer.close();
    }

    /// 换到表上的**下一个**（`forward`）或**上一个**作品：照表眼下的次序，首尾相接（设计稿 `stepWD`）。
    ///
    /// 走的是表格点开一行那一条路：高亮挪到那一行、侧边详情跟着换过去——详情页摆的就是那一份。
    /// 表上一行都没点中过（或者表是空的）时什么都不做。
    pub fn step_page(&mut self, catalog: &Catalog, forward: bool) {
        let total = self.window.total();
        let Some(at) = self.focused.filter(|_| total > 0) else {
            return;
        };
        let next = if forward {
            (at + 1) % total
        } else {
            (at + total - 1) % total
        };
        let Some(anchor) = self.window.row(catalog, next).map(|row| row.anchor.clone()) else {
            return;
        };
        self.focused = Some(next);
        self.open_work(catalog, &anchor);
        // 换了作品，上一个作品的编辑态收掉（设计稿 `openWD` 把 `medit` 置回去）。
        if let Some(page) = self.page.as_mut() {
            page.editing = None;
        }
    }

    /// 进入「元数据」那一面的**编辑态**（设计稿 `beginEdit`）：每一格一个输入框，框里先填着眼下写出去的值。
    pub(super) fn begin_meta_edit(&mut self) {
        if let Some(page) = self.page.as_mut() {
            page.tab = Tab::Metadata;
            page.editing.get_or_insert_with(BTreeMap::new);
        }
    }

    /// 画作品详情页：顶上那一条，底下一块竖着滚的正文。
    pub(super) fn page_ui(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        // 点开的那一行没了（库底下变了），详情页没东西可摆：回到三栏。
        let Some(title) = self.work.as_ref().map(|work| self.work_title(work)) else {
            self.close_page();
            return;
        };
        let (返回, 走) = self.page_bar(ui, &title);
        self.sync_page_details(site);
        // 保存那一条贴着底边，要赶在正文那一块之前占好地方。
        let 保存条 = self.save_bar(ui);
        let tokens = Tokens::builtin();
        let [上, 左右, 下] = tokens.space.tab_panel_padding;
        let 留白 = egui::Margin {
            left: 左右 as i8,
            right: 左右 as i8,
            top: 上 as i8,
            bottom: 下 as i8,
        };
        let 底色 = ui.visuals().panel_fill;
        let mut 卡片 = None;
        let mut 元数据 = None;
        let mut 标题 = None;
        let mut 媒体 = None;
        let mut 页动作 = None;
        let mut 概览 = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(底色))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("作品详情页")
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        页动作 = self.hero(ui);
                        self.tabs_ui(ui);
                        egui::Frame::NONE.inner_margin(留白).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            match self.page.as_ref().map(Page::tab) {
                                Some(Tab::Overview) => 概览 = self.overview_tab(ui),
                                Some(Tab::Variants) => 卡片 = self.variants_tab(ui),
                                Some(Tab::Metadata) => 元数据 = self.metadata_tab(ui),
                                Some(Tab::Titles) => 标题 = self.titles_tab(ui),
                                Some(Tab::Media) => 媒体 = self.media_tab(ui),
                                Some(Tab::Evidence) => self.evidence_tab(ui),
                                _ => {}
                            }
                        });
                    });
            });
        match 卡片 {
            Some((key, CardPress::Prefer)) => self.prefer(site, &key),
            Some((key, CardPress::Restore)) => self.restore_rule(site, &key),
            Some((key, CardPress::Split)) => self.open_split(site, &key),
            Some((key, CardPress::Reveal)) => self.reveal(site, &key),
            Some((_, CardPress::Adjust(第几处))) => self.adjust_shaping(site, 第几处),
            Some((key, CardPress::UndoShaping)) => self.undo_shaping(site, &key),
            None => {}
        }
        // **成型纠正那一层每一帧都画**：它开没开着记在这一屏上（`crate::dialog` 那条规矩）。
        self.fixer.ui(ui.ctx(), site);
        if self.fixer.take_applied() {
            self.reshaped = true;
            if let Some(page) = self.page.as_mut() {
                page.forget();
            }
        }
        if let Some(action) = 元数据 {
            self.apply_meta(site, action);
        }
        if let Some(action) = 标题 {
            self.apply_title(site, action);
        }
        if let Some(clicked) = 媒体 {
            self.open_media(clicked);
        }
        if let Some(action) = 页动作.or(概览) {
            self.apply_page(site, action);
        }
        match 保存条 {
            Some(SaveAction::Save) => self.save_meta(site),
            Some(SaveAction::Discard) => {
                if let Some(page) = self.page.as_mut() {
                    page.editing = None;
                }
            }
            None => {}
        }
        // **有没存的改动时不让走**（设计稿 `guardDirty`）：说一句，人自己去按保存或放弃。
        if (返回 || 走.is_some()) && self.page.as_ref().is_some_and(Page::dirty) {
            self.notice = Some(UNSAVED.to_owned());
            return;
        }
        if 返回 {
            self.close_page();
        }
        if let Some(forward) = 走 {
            self.step_page(&site.catalog, forward);
        }
    }

    /// 编辑态下底下那一条（设计稿 `.savebar`）：改了几个字段、保存之后会怎样，右头「放弃」「保存」。
    /// 只在「元数据」那一面编辑态时摆；交回按了哪一颗。
    fn save_bar(&self, ui: &mut egui::Ui) -> Option<SaveAction> {
        let page = self.page.as_ref()?;
        let drafts = page.editing.as_ref()?;
        if page.tab != Tab::Metadata {
            return None;
        }
        let tokens = Tokens::builtin();
        let 改了几个 = drafts.values().filter(|draft| draft.dirty()).count();
        let [上下, 左右] = tokens.space.save_bar_padding;
        let 框 = egui::Frame::new()
            .fill(ui.visuals().window_fill)
            .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)));
        egui::Panel::bottom("作品详情页保存那一条")
            .resizable(false)
            .frame(框)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = tokens.space.save_bar_gap;
                    ui.label(
                        egui::RichText::new(if 改了几个 > 0 {
                            format!("已修改 {改了几个} 个字段。")
                        } else {
                            "还没有修改。".to_owned()
                        })
                        .color(ui.visuals().strong_text_color()),
                    );
                    look::help(
                        ui,
                        "保存后记为手动修改（裁决），优先于所有数据源，重新刮削不会覆盖。",
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        look::buttons(ui, |ui| {
                            let 保存 = ui
                                .scope(|ui| {
                                    look::primary_button(ui.visuals_mut());
                                    ui.add_enabled(改了几个 > 0, egui::Button::new("保存"))
                                })
                                .inner
                                .clicked();
                            let 放弃 = ui.button("放弃").clicked();
                            if 保存 {
                                Some(SaveAction::Save)
                            } else if 放弃 {
                                Some(SaveAction::Discard)
                            } else {
                                None
                            }
                        })
                    })
                    .inner
                })
                .inner
            })
            .inner
    }

    /// 按了保存：改过的每一格写进中立库，**只走核心库那几个写入口**，写完收掉编辑态、整屏重读。
    ///
    /// - 认出作品的显示标题：往标题集合里加一条**裁决**来源的叫法（`put_titles`）——标题集合挑显示标题时裁决压过一切
    ///   （`title::choose` 第 1 层）。框里空着不算改。
    /// - 别的格：写成那一格的裁决（`put_verdict_value`，一个字段上只留一条）；框里清空就是撤掉裁决（`clear_verdict_value`）。
    fn save_meta(&mut self, site: &mut Site) {
        let Some(drafts) = self.page.as_mut().and_then(|page| page.editing.take()) else {
            return;
        };
        let mut 存了 = 0;
        // 清空了、可那一格本来就是数据源给的值（没有手动修改可撤）：库里什么都没变，不算进「已保存」。
        let mut 没动 = 0;
        for ((anchor, subject, field), draft) in drafts {
            if !draft.dirty() {
                continue;
            }
            let text = draft.text.trim().to_owned();
            let done = if field == Field::Title && anchor == AnchorKind::Work {
                if text.is_empty() {
                    continue;
                }
                site.catalog
                    .put_titles(&[TitleRow {
                        work: subject.clone(),
                        language: language_of(&text, None),
                        kind: TitleKind::Alias,
                        source: VERDICT.to_owned(),
                        region: None,
                        variant_key: None,
                        confidence: Confidence::High,
                        seam: None,
                        evidence: HAND_WRITTEN.to_owned(),
                        seen: 1,
                        value: text,
                    }])
                    .map(|()| true)
            } else if text.is_empty() {
                site.catalog.clear_verdict_value(anchor, &subject, field)
            } else {
                site.catalog
                    .put_verdict_value(anchor, &subject, field, &text, HAND_WRITTEN)
                    .map(|()| true)
            };
            match done {
                Ok(true) => 存了 += 1,
                Ok(false) => 没动 += 1,
                Err(error) => self.error = Some(format!("中立库写不动：{error}")),
            }
        }
        self.refresh(site);
        let 清空没用 = (没动 > 0).then(|| {
            format!("清空的 {没动} 个字段本来就是数据源的值，没有改动；要换掉它，请填写新值")
        });
        self.notice = Some(match (存了, 清空没用) {
            (0, Some(说的)) => 说的,
            (_, Some(说的)) => {
                format!("已保存 {存了} 个字段。手动修改优先于所有数据源，重新刮削不会覆盖。{说的}")
            }
            (_, None) => {
                format!("已保存 {存了} 个字段。手动修改优先于所有数据源，重新刮削不会覆盖")
            }
        });
    }

    /// 作品详情页手上那几份变体详情跟上点开那一行：底下的变体换了（换了作品、库底下变了）就照库里现在的样子重读。
    fn sync_page_details(&mut self, site: &Site) {
        let catalog = &site.catalog;
        let keys: Vec<String> = self
            .work
            .as_ref()
            .map(|work| {
                work.variants
                    .iter()
                    .map(|variant| variant.row.key.clone())
                    .collect()
            })
            .unwrap_or_default();
        let Some(page) = self.page.as_mut() else {
            return;
        };
        if page.for_keys == keys {
            return;
        }
        let mut details = Vec::new();
        for key in &keys {
            match catalog.variant_detail(key, &self.priorities, self.pool.as_ref()) {
                Ok(Some(detail)) => details.push(detail),
                Ok(None) => {}
                Err(error) => self.error = Some(format!("中立库读不动：{error}")),
            }
        }
        let mut fields = Vec::new();
        let mut groups = Vec::new();
        let mut 头 = None;
        if let Some(work) = self.work.as_ref() {
            let (anchor, subject) = work.anchor.scrape_anchor(&work.name);
            // 「当前值」照作品的**头一个平台**算（协调人 2026-09-15 定）：按平台的覆盖照它取，汉化组读那个平台上的首选变体。
            let platform = work.platforms.first().map_or("", String::as_str);
            let head = match anchor {
                AnchorKind::Work => head_of(work, &details),
                AnchorKind::Variant => Some(subject),
            };
            头 = head.map(str::to_owned);
            match priority::entry_fields(catalog, anchor, subject, platform, head, &self.priorities)
            {
                Ok(all) => fields = all,
                Err(error) => self.error = Some(format!("中立库读不动：{error}")),
            }
            if anchor == AnchorKind::Work {
                // **汉化组挂在变体上**（ADR-0012）：哪几个变体摆那一行、那一格写的是什么，都由核心库答
                // （`priority::translation_groups`）。
                match priority::translation_groups(catalog, work, &self.priorities) {
                    Ok(all) => groups = all,
                    Err(error) => self.error = Some(format!("中立库读不动：{error}")),
                }
            }
        }
        page.fields = fields;
        let row = match self.work.as_ref() {
            Some(work) => match catalog.work_rows(
                &self.query,
                &[(work.anchor.clone(), work.name.clone())],
                &self.priorities,
            ) {
                Ok(mut rows) => rows.pop(),
                Err(error) => {
                    self.error = Some(format!("中立库读不动：{error}"));
                    None
                }
            },
            None => None,
        };
        let mut files = BTreeMap::new();
        for key in &keys {
            match catalog.file_lines(key, TOP_MEMBERS) {
                Ok(lines) => {
                    files.insert(key.clone(), lines);
                }
                Err(error) => self.error = Some(format!("中立库读不动：{error}")),
            }
        }
        let favorite = match romcat_core::collection::favorite_of(site, &keys) {
            Ok(anchor) => anchor,
            Err(error) => {
                self.error = Some(format!("收藏读不动：{error}"));
                None
            }
        };
        let chinese = match self.work.as_ref() {
            Some(work) => match catalog.work_chinese_mark(work) {
                Ok(mark) => mark,
                Err(error) => {
                    self.error = Some(format!("中立库读不动：{error}"));
                    None
                }
            },
            None => None,
        };
        // **子库**与**导出**那两行（票 `gui-looks-like-the-design/34`）。两样的判断都在核心库：
        // 「落在哪几个子库里」走求值那一处（`sublibrary::holding`，ADR-0024），
        // 「上次几点写出去的」读导出那一趟逐条记下的账（`Catalog::entry_exported`）。
        let mut sublibraries = Vec::new();
        let mut exported = None;
        if let Some(work) = self.work.as_ref() {
            let 键: Vec<&str> = keys.iter().map(String::as_str).collect();
            match romcat_core::sublibrary::holding(catalog, &键) {
                Ok(names) => sublibraries = names,
                Err(error) => self.error = Some(format!("中立库读不动：{error}")),
            }
            let (anchor, subject) = work.anchor.scrape_anchor(&work.name);
            match catalog.entry_exported(anchor, subject) {
                Ok(mark) => exported = mark,
                Err(error) => self.error = Some(format!("中立库读不动：{error}")),
            }
        }
        page.files = files;
        page.chinese = chinese;
        page.sublibraries = sublibraries;
        page.exported = exported;
        page.platform_names = self.work.as_ref().map_or_else(String::new, |work| {
            let manifest = romcat_core::platform::Manifest::builtin();
            work.platforms
                .iter()
                .map(|code| manifest.full_name(code).unwrap_or(code))
                .collect::<Vec<_>>()
                .join(" / ")
        });
        // **成型存疑**（票 `gui-looks-like-the-design/29`）：判据在核心库一处，这里只把这几个变体
        // 跟前那一小块捞出来问一遍（`Catalog::shaping_doubts_near`）。
        let 存疑 = {
            let 借: Vec<&str> = keys.iter().map(String::as_str).collect();
            match catalog.shaping_doubts_near(&借, &romcat_core::platform::Manifest::builtin()) {
                Ok(doubts) => doubts,
                Err(error) => {
                    self.error = Some(format!("中立库读不动：{error}"));
                    Vec::new()
                }
            }
        };
        let Some(page) = self.page.as_mut() else {
            return;
        };
        page.doubts = 存疑;
        page.favorite = favorite;
        page.row = row;
        page.head = 头;
        page.groups = groups;
        page.details = details;
        page.for_keys = keys;
    }

    /// 把**首选变体**裁给这一个，照它自己那个平台：落的是侧边详情那一条同一种裁决（`set_preferred`）。
    fn prefer(&mut self, site: &mut Site, key: &str) {
        let Some((work, platform)) = self
            .page
            .as_ref()
            .and_then(|page| page.details.iter().find(|detail| detail.row.key == key))
            .and_then(|detail| Some((detail.work.clone()?, detail.row.platform.clone()?)))
        else {
            return;
        };
        self.set_preferred(site, &work, &platform, key);
        if let Some(page) = self.page.as_mut() {
            page.forget();
        }
    }

    /// 「调整成型…」：对着这一面上第 `at` 处**成型存疑**开**成型纠正**那一层（`crate::shaping`）。
    ///
    /// 这一面手上的存疑是核心库按这几个变体跟前那一小块判出来的（`Catalog::shaping_doubts_near`），
    /// 与库体检那一格同一处判据，装的就是**中立库的键**，原样交过去。
    ///
    /// **「同一种全库一共几处」这儿说不出**：那个数只有全库那份体检报告答得出，而这一面手上只有
    /// 这一个作品跟前那几处——交 `None`，那一层于是只说「其余几处列在库体检里」，不凑一个数
    /// （ADR-0024）。
    fn adjust_shaping(&mut self, site: &mut Site, at: usize) {
        let Some(doubt) = self
            .page
            .as_ref()
            .and_then(|page| page.doubts.get(at))
            .cloned()
        else {
            return;
        };
        self.fixer.open_doubt(&site.catalog, &doubt, None);
    }

    /// 「撤销成型纠正」：清掉**这一处**的人工纠正（核心库 `shape::fix::undo`），回到成型规则
    /// 原本的结果（票 29 验收第 5 条）。
    ///
    /// **撤的是一整处，不是这一张卡**：拆开一个目录落的是那几份内容各一行，只撤其中一份的话
    /// 剩下几份还各自成变体。同一处一起纠正出来的是哪几个由核心库答（`Catalog::shaping_fix_group`）。
    fn undo_shaping(&mut self, site: &mut Site, key: &str) {
        match site.catalog.shaping_fix_group(key) {
            Ok(spot) if spot.is_empty() => {
                self.error = Some("这个变体上没有人工纠正，没什么可撤的。".to_string());
            }
            Ok(spot) => self.fixer.undo(site, &spot),
            Err(error) => self.error = Some(format!("中立库读不动：{error}")),
        }
    }

    /// 「恢复规则选择」：撤掉这个变体所在作品、所在平台上的**首选变体裁决**，回到规则选的那一个（拿主意的人 2026-09-15 定）。
    /// 走的是侧边详情那一条同一种撤法（`clear_preferred` → `Catalog::clear_preferred_variant`）。
    fn restore_rule(&mut self, site: &mut Site, key: &str) {
        let Some((work, platform)) = self
            .page
            .as_ref()
            .and_then(|page| page.details.iter().find(|detail| detail.row.key == key))
            .and_then(|detail| Some((detail.work.clone()?, detail.row.platform.clone()?)))
        else {
            return;
        };
        self.clear_preferred(site, &work, &platform);
        if let Some(page) = self.page.as_mut() {
            page.forget();
        }
    }

    /// 顶上那一条（设计稿 `.wdbar`）：左边「← 返回浏览」「浏览 / 作品名」，右边「第几个 / 共几个」与「上一个」「下一个」。
    /// 交回按没按返回、要不要换一个（`Some(true)` 是下一个）。
    fn page_bar(&self, ui: &mut egui::Ui, title: &str) -> (bool, Option<bool>) {
        let tokens = Tokens::builtin();
        let [上下, 左右] = tokens.space.work_bar_padding;
        let 框 = egui::Frame::new()
            .fill(ui.visuals().panel_fill)
            .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)));
        egui::Panel::top("作品详情页顶上那一条")
            .resizable(false)
            .frame(框)
            .show(ui, |ui| {
                let 按了 = ui
                    .horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = tokens.space.work_bar_gap;
                        let 返回 = look::small_buttons(ui, |ui| ui.button("← 返回浏览").clicked());
                        // 「浏览 / 作品名」一段字（设计稿 `.crumb`）：半号字、弱字色，作品名换正文色，拉丁与数字加粗。
                        let 字号 = look::font_size(ui.ctx(), tokens.font.size_small_plus);
                        let (弱, 强) = (
                            ui.visuals().weak_text_color(),
                            ui.visuals().strong_text_color(),
                        );
                        let mut job = egui::text::LayoutJob::default();
                        job.append(
                            "浏览 / ",
                            0.0,
                            egui::TextFormat::simple(egui::FontId::proportional(字号), 弱),
                        );
                        job.append(
                            title,
                            0.0,
                            egui::TextFormat::simple(
                                egui::FontId::new(字号, font::strong_family()),
                                强,
                            ),
                        );
                        // 右头「第几个 / 共几个」（设计稿 `.help.num`）：表上这一行排第几，照表眼下的筛选与次序。
                        let 位置 = self.focused.map(|at| {
                            format!("{} / {}", thousands(at + 1), thousands(self.window.total()))
                        });
                        let 说明字号 = look::font_size(ui.ctx(), tokens.font.size_small);
                        let 走 = ui
                            .with_layout(Layout::right_to_left(Align::Center), |ui| {
                                // 右往左摆：先摆的在最右。两颗是小号幽灵按钮（设计稿 `.btn.sm.ghost`）。
                                let (下, 上) = look::small_buttons(ui, |ui| {
                                    ui.scope(|ui| {
                                        look::ghost_button(ui.visuals_mut());
                                        (
                                            ui.button("下一个").clicked(),
                                            ui.button("上一个").clicked(),
                                        )
                                    })
                                    .inner
                                });
                                if let Some(位置) = &位置 {
                                    ui.label(egui::RichText::new(位置).size(说明字号).color(弱));
                                }
                                // 剩下的宽给「浏览 / 作品名」，画不下截尾巴。
                                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                    ui.add(egui::Label::new(job).truncate());
                                });
                                if 下 {
                                    Some(true)
                                } else if 上 {
                                    Some(false)
                                } else {
                                    None
                                }
                            })
                            .inner;
                        (返回, 走)
                    })
                    .inner;
                // 上一次动作出的错：一直摆着，直到下一次动作成了（回执走窗口底边的提示条，`Screen::notice_toast`）。
                if let Some(说的) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, 说的);
                }
                按了
            })
            .inner
    }

    /// 六个面那一排（设计稿 `.tabs`）：一面一格，选中那一面正文色、底下一道强调色线；带数的面在名字后头跟一个小号等宽的数。
    fn tabs_ui(&mut self, ui: &mut egui::Ui) {
        let tokens = Tokens::builtin();
        let Some(眼下) = self.page.as_ref().map(Page::tab) else {
            return;
        };
        let 变体数 = self.work.as_ref().map(|work| work.variants.len());
        // 没认出作品的没有标题集合，那一面不跟数（设计稿 `w.unknown?null:…`）。
        let 标题数 = self
            .detail
            .as_ref()
            .filter(|detail| detail.work.is_some())
            .map(|detail| detail.titles.len());
        let 媒体数 = self.page_media_items().map(|items| items.len());
        let 数 = |tab: Tab| match tab {
            Tab::Variants => 变体数,
            Tab::Titles => 标题数,
            Tab::Media => 媒体数,
            _ => None,
        };
        let (字色, 强字色, 弱字色, 强调, 线) = {
            let visuals = ui.visuals();
            (
                visuals.text_color(),
                visuals.strong_text_color(),
                visuals.weak_text_color(),
                visuals.selection.stroke.color,
                visuals.widgets.noninteractive.bg_stroke,
            )
        };
        let mut 换到 = None;
        let 这一排 = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = tokens.space.tabs_gap;
                ui.add_space(tokens.space.tabs_padding);
                for tab in Tab::ALL {
                    let on = tab == 眼下;
                    let 名字体 = if on {
                        egui::FontId::new(tokens.font.size_body, font::strong_family())
                    } else {
                        egui::FontId::proportional(tokens.font.size_body)
                    };
                    let 名 = ui.painter().layout_no_wrap(
                        tab.label().to_owned(),
                        名字体,
                        egui::Color32::PLACEHOLDER,
                    );
                    let 数字 = 数(tab).map(|n| {
                        ui.painter().layout_no_wrap(
                            thousands(n as u64),
                            egui::FontId::monospace(tokens.font.size_caption),
                            弱字色,
                        )
                    });
                    let 宽 = 名.size().x
                        + 数字
                            .as_ref()
                            .map_or(0.0, |galley| tokens.space.tab_count_gap + galley.size().x)
                        + 2.0 * tokens.space.tab_padding;
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(宽, tokens.layout.tab_height),
                        egui::Sense::click(),
                    );
                    let 色 = if on || response.hovered() {
                        强字色
                    } else {
                        字色
                    };
                    let 名在 = egui::pos2(
                        rect.left() + tokens.space.tab_padding,
                        rect.center().y - 名.size().y / 2.0,
                    );
                    let 名宽 = 名.size().x;
                    ui.painter().galley(名在, 名, 色);
                    if let Some(数字) = 数字 {
                        let 在 = egui::pos2(
                            名在.x + 名宽 + tokens.space.tab_count_gap,
                            rect.center().y - 数字.size().y / 2.0,
                        );
                        ui.painter().galley(在, 数字, 弱字色);
                    }
                    if on {
                        ui.painter().rect_filled(
                            egui::Rect::from_min_max(
                                egui::pos2(
                                    rect.left(),
                                    rect.bottom() - tokens.layout.tab_underline,
                                ),
                                rect.max,
                            ),
                            0.0,
                            强调,
                        );
                    }
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::SelectableLabel,
                            true,
                            on,
                            tab.label(),
                        )
                    });
                    look::focus_ring(ui.ctx(), ui.clip_rect(), &response);
                    if response.clicked() {
                        换到 = Some(tab);
                    }
                }
            })
            .response
            .rect;
        // 这一排底下一条分隔线，横贯整块正文（设计稿 `.tabs` 的 `border-bottom`）。
        ui.painter().hline(
            ui.max_rect().x_range(),
            这一排.bottom() - 线.width / 2.0,
            线,
        );
        if let (Some(tab), Some(page)) = (换到, self.page.as_mut()) {
            page.tab = tab;
        }
    }

    /// 「变体与文件」那一面（设计稿 `varTab`）：一句说明，底下每个变体一张卡。
    /// 按了哪一张卡上的哪一颗，交回那个变体的键与那一颗。
    fn variants_tab(&self, ui: &mut egui::Ui) -> Option<(String, CardPress)> {
        let tokens = Tokens::builtin();
        let (work, page) = (self.work.as_ref()?, self.page.as_ref()?);
        ui.spacing_mut().item_spacing.y = 0.0;
        look::help(ui, "首选变体是前端默认启动的那一个。");
        ui.add_space(look::step(2));
        // **成型纠正那一层落过一笔之后那句回话**（票 `gui-looks-like-the-design/29`）：办成没办成、
        // 办成了什么，画在这一面上（同库体检那一处的做法——那一层自己已经关掉了，话得有地方说）。
        match self.fixer.said() {
            Some(Ok(said)) => {
                ui.weak(said);
                ui.add_space(look::step(2));
            }
            Some(Err(said)) => {
                ui.colored_label(ui.visuals().error_fg_color, said);
                ui.add_space(look::step(2));
            }
            None => {}
        }
        let mut 设首选 = None;
        // **成型存疑那一块建议**（设计稿 `shapeSuspect`，票 `gui-looks-like-the-design/29`）：
        // 哪一处存疑、凭什么，都由核心库答（`Catalog::shaping_doubts_near`，ADR-0024）。
        for (at, doubt) in page.doubts.iter().enumerate() {
            let 按了 = look::note_box(ui, |ui| {
                ui.spacing_mut().item_spacing.y = look::step(1);
                ui.label(crate::font::strong(doubt.kind.label()));
                look::help(ui, &doubt.reason());
                look::small_buttons(ui, |ui| {
                    ui.scope(|ui| {
                        look::primary_button(ui.visuals_mut());
                        ui.button(crate::shaping::ADJUST)
                            .on_hover_text(
                                "成型规则会出错，人工纠正是正门；记为人工纠正，盘上的文件一个字节都不动",
                            )
                            .clicked()
                    })
                    .inner
                })
            });
            if 按了 {
                设首选 = Some((String::new(), CardPress::Adjust(at)));
            }
            ui.add_space(tokens.space.work_card_gap);
        }
        for (at, variant) in work.variants.iter().enumerate() {
            let Some(detail) = page
                .details
                .iter()
                .find(|detail| detail.row.key == variant.row.key)
            else {
                continue;
            };
            // **变体简称由核心库拼**（`Catalog::variant_short_names`，点开那一行时问过一次）；拼不出来时印文件名。
            let 简称 = self.short_names.get(at).map_or_else(
                || romcat_core::path::file_name_of_key(&variant.row.key),
                String::as_str,
            );
            if at > 0 {
                ui.add_space(tokens.space.work_card_gap);
            }
            let 文件 = page
                .files
                .get(&variant.row.key)
                .map_or(&[][..], Vec::as_slice);
            // 汉化组那一格：认出作品的照核心库挑好的那几行（`priority::translation_groups`），没认出的就是这个条目自己
            // 那一格（`priority::entry_fields`）。
            let 汉化组 = match work.anchor {
                WorkAnchor::Work(_) => page
                    .groups
                    .iter()
                    .find(|(key, _)| *key == variant.row.key)
                    .map(|(_, group)| group),
                WorkAnchor::Loose(_) => page
                    .fields
                    .iter()
                    .find(|one| one.field == Field::TranslationGroup),
            };
            if let Some(按了) = variant_card(ui, variant, detail, 文件, 简称, 汉化组) {
                设首选 = Some((variant.row.key.clone(), 按了));
            }
        }
        设首选
    }

    /// 「识别依据」那一面（设计稿 `evTab`）：一句说明，底下每个变体一张卡——判定依据、候选、来源与置信度。
    fn evidence_tab(&self, ui: &mut egui::Ui) {
        let tokens = Tokens::builtin();
        let Some(work) = self.work.as_ref() else {
            return;
        };
        ui.spacing_mut().item_spacing.y = 0.0;
        // 稿上这句后半截「高置信自动通过；中、低置信进入待确认队列」与 ADR-0002 对不上（中置信是通过但标记），只印前半截。
        look::help(
            ui,
            "识别只看文件内容（哈希、文件头、序列号），文件名只在无法按内容匹配时作为参考。\
             高置信自动通过；中、低置信进入待确认队列。",
        );
        ui.add_space(look::step(2));
        for (at, variant) in work.variants.iter().enumerate() {
            let 简称 = self.short_names.get(at).map_or_else(
                || romcat_core::path::file_name_of_key(&variant.row.key),
                String::as_str,
            );
            if at > 0 {
                ui.add_space(tokens.space.work_card_gap);
            }
            evidence_card(ui, variant, 简称);
        }
    }

    /// 「元数据」那一面（设计稿 `metaTab`）：每个字段一行——眼下写出去的是什么、是哪个源说的（核心库
    /// `priority::entry_fields` 答，与导出同一处）；「其他 N 个来源」摊开别家的说法，「使用这个值」把其中一句记成裁决，
    /// 「撤销手动修改」回到数据源说了算。**编辑态**下每一格换成一个输入框，底下一排别家说过的值点一个就填进框里。
    ///
    /// 认出作品的：显示标题、简介、类型、开发商、发行商、年份挂作品，汉化组逐个变体各一行；没认出作品的：那个变体
    /// 自己身上的各格。
    ///
    /// 草稿住在 `Page` 里、画的这一趟先拿出来，画完放回去：画的时候还要读 `Page` 里别的几样。
    fn metadata_tab(&mut self, ui: &mut egui::Ui) -> Option<MetaAction> {
        let mut editing = self.page.as_mut().and_then(|page| page.editing.take());
        let 动作 = self.metadata_rows(ui, editing.as_mut());
        if let Some(page) = self.page.as_mut() {
            page.editing = editing;
        }
        动作
    }

    /// 「元数据」那一面的正文：标题行、一句说明、一行一格。`editing` 给了就是编辑态。
    fn metadata_rows(
        &self,
        ui: &mut egui::Ui,
        mut editing: Option<&mut BTreeMap<(AnchorKind, String, Field), Draft>>,
    ) -> Option<MetaAction> {
        let tokens = Tokens::builtin();
        let (work, page) = (self.work.as_ref()?, self.page.as_ref()?);
        let (anchor, subject) = work.anchor.scrape_anchor(&work.name);
        let 强 = ui.visuals().strong_text_color();
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut 动作 = None;
        let 编辑 = ui
            .horizontal(|ui| {
                ui.label(
                    egui::RichText::new("元数据")
                        .size(tokens.font.size_title)
                        .color(强),
                );
                editing.is_none()
                    && ui
                        .with_layout(Layout::right_to_left(Align::Center), |ui| {
                            look::small_buttons(ui, |ui| {
                                ui.scope(|ui| {
                                    look::primary_button(ui.visuals_mut());
                                    ui.button("编辑")
                                })
                                .inner
                            })
                            .clicked()
                        })
                        .inner
            })
            .inner;
        if 编辑 {
            动作 = Some(MetaAction::BeginEdit);
        }
        ui.add_space(look::step(1));
        look::help(
            ui,
            match work.anchor {
                WorkAnchor::Work(_) => {
                    "同一字段可以同时保存多个来源的值，按数据源优先级选用；手动修改优先于所有数据源。"
                }
                WorkAnchor::Loose(_) => {
                    "同一字段可以同时保存多个来源的值，按数据源优先级选用；手动修改优先于所有数据源。\
                     这个变体尚未关联作品，只能编辑变体级字段。"
                }
            },
        );
        ui.add_space(tokens.space.meta_row_gap[0]);
        // 照稿的次序：显示标题、简介、类型、开发商、发行商、年份；没认出作品的连汉化组也在这一串里。
        let 次序 = [
            Field::Title,
            Field::Description,
            Field::Genre,
            Field::Developer,
            Field::Publisher,
            Field::Year,
            Field::TranslationGroup,
        ];
        let 层 = match anchor {
            AnchorKind::Work => "作品级",
            AnchorKind::Variant => "变体级",
        };
        let mut 行们: Vec<(String, AnchorKind, &str, &FieldShown)> = 次序
            .iter()
            .filter(|field| anchor == AnchorKind::Variant || **field != Field::TranslationGroup)
            .filter_map(|field| page.fields.iter().find(|one| one.field == *field))
            .map(|one| (层.to_owned(), anchor, subject, one))
            .collect();
        for (key, group) in &page.groups {
            let 简称 = work
                .variants
                .iter()
                .position(|variant| &variant.row.key == key)
                .and_then(|at| self.short_names.get(at))
                .map_or_else(
                    || romcat_core::path::file_name_of_key(key).to_owned(),
                    Clone::clone,
                );
            行们.push((简称, AnchorKind::Variant, key.as_str(), group));
        }
        // 标题集合里的叫法：编辑态下显示标题那一格底下一排可以点的就是它们（设计稿 `titlesOf`）。
        let 叫法: Vec<TitleRow> = page
            .details
            .first()
            .map(|detail| detail.titles.to_vec())
            .unwrap_or_default();
        let 共 = 行们.len();
        for (at, (层, anchor, subject, one)) in 行们.into_iter().enumerate() {
            let 集合挑的 = one.field == Field::Title && anchor == AnchorKind::Work;
            let 名 = field_label(one.field);
            let 画线 = at + 1 < 共;
            if let Some(drafts) = editing.as_mut() {
                let 眼下 = one
                    .shown
                    .as_ref()
                    .map(|said| said.values.join("、"))
                    .unwrap_or_default();
                let draft = drafts
                    .entry((anchor, subject.to_owned(), one.field))
                    .or_insert_with(|| Draft {
                        original: 眼下.clone(),
                        text: 眼下,
                    });
                let 改过 = draft.dirty();
                let 可点的: Vec<(String, String)> = if 集合挑的 {
                    叫法
                        .iter()
                        .map(|row| (row.source.clone(), row.value.clone()))
                        .collect()
                } else {
                    one.offered
                        .iter()
                        .map(|value| (value.source.clone(), value.value.clone()))
                        .collect()
                };
                meta_row(
                    ui,
                    名,
                    &层,
                    画线,
                    改过,
                    |ui| {
                        field_edit(ui, draft, anchor, subject, one.field, &可点的);
                        None
                    },
                    |ui| {
                        if 改过 {
                            dirty_tag(ui);
                        }
                        None
                    },
                );
                continue;
            }
            let 是裁决 = one
                .shown
                .as_ref()
                .is_some_and(|said| said.source.as_deref() == Some(VERDICT));
            let open = page.open_alts.contains(&(subject.to_owned(), one.field));
            let 按了 = meta_row(
                ui,
                名,
                &层,
                画线,
                false,
                |ui| field_value(ui, one, anchor, subject, open, &叫法),
                |ui| {
                    if 是裁决 && ghost_small(ui, "撤销手动修改").clicked() {
                        if 集合挑的 {
                            叫法
                                .iter()
                                .find(|row| {
                                    row.is_verdict()
                                        && one
                                            .shown
                                            .as_ref()
                                            .is_some_and(|said| said.values.contains(&row.value))
                                })
                                .cloned()
                                .map(MetaAction::RevertTitle)
                        } else {
                            Some(MetaAction::Revert {
                                anchor,
                                subject: subject.to_owned(),
                                field: one.field,
                            })
                        }
                    } else {
                        None
                    }
                },
            );
            动作 = 动作.or(按了);
        }
        动作
    }

    /// 「标题」那一面（设计稿 `titleTab`）：显示标题与排序标题是哪一个、怎么选出来的；标题集合一张表，一条一颗
    /// 「隐藏」；「添加一个名称」；隐藏过的那几条列在底下，一条一颗「恢复」。
    ///
    /// 挑显示标题、排序标题的是核心库（`VariantDetail::display`，与导出同一处 `title::choose`）；隐藏记在**沉淀库**里
    /// （`title::suppress`），重新整理标题也不会把它加回来。那份加名称的草稿住在浏览屏上，画的这一趟先拿出来、
    /// 画完放回去：画的时候还要读浏览屏上别的几样。
    fn titles_tab(&mut self, ui: &mut egui::Ui) -> Option<TitleAction> {
        let mut draft = std::mem::take(&mut self.title_draft);
        let 动作 = self.titles_body(ui, &mut draft);
        self.title_draft = draft;
        动作
    }

    /// 「标题」那一面的正文。
    fn titles_body(&self, ui: &mut egui::Ui, draft: &mut TitleDraft) -> Option<TitleAction> {
        let tokens = Tokens::builtin();
        let detail = self.detail.as_ref()?;
        ui.spacing_mut().item_spacing.y = 0.0;
        let Some(work) = detail.work.as_deref() else {
            empty_card(
                ui,
                "这个变体还没认出属于哪个作品，还没有标题集合。可以在待确认里指定作品之后再来。",
            );
            return None;
        };
        let (强, 小, 半号) = (
            ui.visuals().strong_text_color(),
            look::font_size(ui.ctx(), tokens.font.size_small),
            look::font_size(ui.ctx(), tokens.font.size_small_plus),
        );
        if let Some(chosen) = &detail.display {
            section_card(ui, |ui| {
                info_row(ui, "显示标题", |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = look::step(1);
                        ui.label(
                            egui::RichText::new(&chosen.display)
                                .font(egui::FontId::new(半号, font::strong_family()))
                                .color(强),
                        );
                        look::help(ui, DISPLAY_RULE);
                    });
                });
                ui.add_space(tokens.space.info_list_gap[0]);
                info_row(ui, "排序标题", |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = look::step(1);
                        ui.label(
                            egui::RichText::new(&chosen.sort_shown)
                                .font(egui::FontId::monospace(小))
                                .color(强),
                        );
                        look::help(ui, &sort_note(chosen.sort_from));
                    });
                });
            });
            ui.add_space(look::step(3));
        }
        let mut 动作 = titles_table(ui, &detail.titles);
        ui.add_space(look::step(2));
        if title_add_row(ui, draft) {
            动作 = Some(TitleAction::Add);
        }
        ui.add_space(look::step(1));
        look::help(
            ui,
            "手动添加的名称记为裁决。隐藏的名称在下次整理标题时不会重新出现，也可以随时恢复。",
        );
        let 压掉的 = self.suppressed_of(work);
        if !压掉的.is_empty() {
            ui.add_space(look::step(3));
            if let Some(one) = suppressed_card(ui, 压掉的) {
                动作 = Some(TitleAction::Restore(one));
            }
        }
        // **恢复之后就地的下一步**：那条叫法要等下一趟整理标题才回到集合里（票 `gui-self-sufficient/09`）。
        if let Some(说的) = &self.lift_notice {
            ui.add_space(look::step(2));
            look::note_box(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(说的);
                    if look::small_buttons(ui, |ui| ui.button("整理标题").clicked()) {
                        动作 = Some(TitleAction::Fold);
                    }
                });
            });
        }
        动作
    }

    /// 作品详情页**头上那一块**（设计稿 `.hero`）：平台色调进窗口底色的底；左边封面（没有就是大字卡），右边它是什么、
    /// 叫什么、几枚标签、六格事实、一排按钮。按了哪一颗交回那一下。
    ///
    /// 那几格事实写的是**导出真会写出去的**那个值（`priority::entry_fields`，与元数据那一面同一份）；置信度那个词、
    /// 「元数据：…」与「变体 N 个 · 容量」照表上那一行（`Catalog::work_rows`）。
    fn hero(&self, ui: &mut egui::Ui) -> Option<PageAction> {
        let tokens = Tokens::builtin();
        let (work, page) = (self.work.as_ref()?, self.page.as_ref()?);
        let title = self.work_title(work);
        let loose = matches!(work.anchor, WorkAnchor::Loose(_));
        let platform = work.platforms.first().map_or("", String::as_str);
        let 底 = ui
            .visuals()
            .panel_fill
            .lerp_to_gamma(tokens.color.platform.of(platform), tokens.mix.hero_tint);
        let [上, 左右, 下] = tokens.space.hero_padding;
        let (强, 字, 线) = {
            let visuals = ui.visuals();
            (
                visuals.strong_text_color(),
                visuals.text_color(),
                visuals.widgets.noninteractive.bg_stroke,
            )
        };
        // 字卡上那一行副行：认出作品的写排序标题，没认出的写名字怎么来的（设计稿 `tcardHTML`）。
        let 副行 = if loose {
            "名称取自文件名".to_owned()
        } else {
            self.detail
                .as_ref()
                .and_then(|detail| detail.display.as_ref())
                .map(|chosen| chosen.sort_shown.clone())
                .unwrap_or_default()
        };
        let mut 动作 = None;
        let 块 = egui::Frame::new()
            .fill(底)
            .inner_margin(egui::Margin {
                left: 左右 as i8,
                right: 左右 as i8,
                top: 上 as i8,
                bottom: 下 as i8,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = tokens.space.hero_gap;
                    let 宽 = tokens.layout.hero_cover_width;
                    let size = egui::vec2(宽, 宽 / tokens.layout.card_cover_ratio);
                    match self
                        .cover
                        .as_ref()
                        .and_then(|item| self.gallery.texture(item))
                    {
                        Some(texture) => {
                            crate::media::paint_cover(ui, size, tokens.radius.large, texture);
                        }
                        None => crate::media::hero_card(ui, size, &title, &副行, platform, 底),
                    }
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = look::step(1);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = look::step(1);
                            if !platform.is_empty() {
                                platform_chip(ui, platform);
                            }
                            look::section(ui, if loose { "未关联作品的变体" } else { "作品" });
                        });
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&title)
                                    .font(egui::FontId::new(tokens.font.size_hero, font::strong_family()))
                                    .color(强),
                            )
                            .wrap(),
                        );
                        // 底下那一句（设计稿 `.hsub`）：没认出作品的说名字怎么来的；认出的、显示标题又不是作品名本身时写作品名。
                        let 这一句 = if loose {
                            Some("名称取自文件名")
                        } else {
                            (work.name != title).then_some(work.name.as_str())
                        };
                        if let Some(这一句) = 这一句 {
                            ui.label(egui::RichText::new(这一句).size(tokens.font.size_body).color(字));
                        }
                        if let Some(row) = &page.row {
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing.x = look::step(1);
                                look::chip(ui, look::tier_tone(row.tier()), row.confidence_label());
                                if let Some(mark) = page.chinese {
                                    table::tag(ui, mark.label());
                                }
                                table::tag(ui, &format!("元数据：{}", row.meta_label()));
                            });
                        }
                        hero_facts(ui, work, page);
                        动作 = look::small_buttons(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = look::step(1);
                                let 编辑 = ui
                                    .scope(|ui| {
                                        look::primary_button(ui.visuals_mut());
                                        ui.button("编辑元数据")
                                    })
                                    .inner
                                    .clicked();
                                let 刮削 = ui
                                    .button("刮削此作品")
                                    .on_hover_text(
                                        "只对这一个作品取元数据与媒体。按下去之前看得见要发多少请求、大概多久。",
                                    )
                                    .clicked();
                                // **「合并…」照稿摆在这儿**（稿上夹在「加入合集…」与
                                // 「在文件系统中打开」之间；收藏与合集那两颗归票 13，位置留着）。
                                let 合并 = ui
                                    .button(super::merge::MERGE_ONE)
                                    .on_hover_text(
                                        "把这个作品与别的作品合并成一个：第一步里搜出要并进来的那几个。\n\n\
                                         只写入裁决记录，不会移动或修改任何文件。",
                                    )
                                    .clicked();
                                let 打开 = ui
                                    .scope(|ui| {
                                        look::ghost_button(ui.visuals_mut());
                                        ui.button("在文件系统中打开")
                                    })
                                    .inner
                                    .clicked();
                                // 打开的是头一个平台上首选那个变体所在的目录；没有就是头一个变体的。
                                let 哪一个 = page.head.clone().or_else(|| {
                                    work.variants.first().map(|variant| variant.row.key.clone())
                                });
                                if 编辑 {
                                    Some(PageAction::EditMeta)
                                } else if 刮削 {
                                    Some(PageAction::Scrape)
                                } else if 合并 {
                                    Some(PageAction::Merge)
                                } else if 打开 {
                                    哪一个.map(PageAction::Reveal)
                                } else {
                                    None
                                }
                            })
                            .inner
                        });
                    });
                });
            })
            .response
            .rect;
        ui.painter()
            .hline(块.x_range(), 块.bottom() - 线.width / 2.0, 线);
        动作
    }

    /// 「概览」那一面（设计稿 `ovTab`）：左栏简介、基本信息，右栏媒体、状态四块。按了哪一颗交回那一下。
    fn overview_tab(&self, ui: &mut egui::Ui) -> Option<PageAction> {
        let tokens = Tokens::builtin();
        let (work, page) = (self.work.as_ref()?, self.page.as_ref()?);
        let 缝 = tokens.space.overview_gap;
        let 宽 = ui.available_width();
        // 左宽右窄，照稿 `.ov`（令牌 `overview-columns`）。
        let [左份, 右份] = tokens.layout.overview_columns;
        let 左宽 = ((宽 - 缝) * 左份 / (左份 + 右份)).floor().max(0.0);
        let 右宽 = (宽 - 缝 - 左宽).max(0.0);
        let mut 动作 = None;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 缝;
            ui.vertical(|ui| {
                ui.set_width(左宽);
                ui.spacing_mut().item_spacing.y = 缝;
                if let Some(按了) = description_card(ui, page) {
                    动作 = Some(按了);
                }
                self.basics_card(ui, work, page);
            });
            ui.vertical(|ui| {
                ui.set_width(右宽);
                ui.spacing_mut().item_spacing.y = 缝;
                if let Some(按了) = self.media_strip_card(ui) {
                    动作 = Some(按了);
                }
                status_card(ui, work, page);
            });
        });
        动作
    }

    /// 基本信息那一块（设计稿 `ovTab` 第二块 `.sect`）：显示标题、排序标题、平台、年份、类型、开发商、发行商（后四样跟着
    /// 来源标签）、首选变体（没人裁过时说照什么规则选的）。
    fn basics_card(&self, ui: &mut egui::Ui, work: &WorkDetail, page: &Page) {
        let tokens = Tokens::builtin();
        let 行距 = tokens.space.info_list_gap[0];
        let 字号 = look::font_size(ui.ctx(), tokens.font.size_small_plus);
        let 强 = ui.visuals().strong_text_color();
        let 字 = |ui: &mut egui::Ui, text: &str| {
            ui.add(egui::Label::new(egui::RichText::new(text).size(字号).color(强)).wrap());
        };
        let chosen = self
            .detail
            .as_ref()
            .and_then(|detail| detail.display.as_ref());
        let 平台 = if work.platforms.is_empty() {
            "—".to_owned()
        } else {
            work.platforms.join(" / ")
        };
        section_card(ui, |ui| {
            card_header(ui, "基本信息", |_| {});
            info_row(ui, "显示标题", |ui| 字(ui, &self.work_title(work)));
            ui.add_space(行距);
            info_row(ui, "排序标题", |ui| {
                ui.label(
                    egui::RichText::new(chosen.map_or("—", |chosen| chosen.sort_shown.as_str()))
                        .font(egui::FontId::monospace(字号))
                        .color(强),
                );
            });
            ui.add_space(行距);
            info_row(ui, "平台", |ui| 字(ui, &平台));
            for field in [
                Field::Year,
                Field::Genre,
                Field::Developer,
                Field::Publisher,
            ] {
                ui.add_space(行距);
                let said = page
                    .fields
                    .iter()
                    .find(|one| one.field == field)
                    .and_then(|one| one.shown.as_ref());
                info_row(ui, field.label(), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = look::step(1);
                        // 行高只按徽标那么高算：默认的控件高会把这一行撑高、值往下沉，与左边的名错开（岔路口 3 选 A）。
                        ui.spacing_mut().interact_size.y = tokens.layout.source_badge_height;
                        match said {
                            Some(said) => {
                                字(ui, &said.values.join("、"));
                                let 源 = said.source.as_deref().unwrap_or("作品名");
                                source_badge(ui, 源, 源 == VERDICT);
                            }
                            None => 字(ui, "—"),
                        }
                    });
                });
            }
            ui.add_space(行距);
            // 照稿写「汉化 / 官中」，一个中文的都没有写「无」。
            info_row(ui, "中文版本", |ui| {
                字(ui, page.chinese.map_or("无", ChineseMark::label));
            });
            ui.add_space(行距);
            info_row(ui, "首选变体", |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = look::step(1);
                    ui.spacing_mut().interact_size.y = tokens.layout.source_badge_height;
                    let Some(key) = page.head.as_deref() else {
                        字(ui, "—");
                        return;
                    };
                    let 简称 = work
                        .variants
                        .iter()
                        .position(|variant| variant.row.key == key)
                        .and_then(|at| self.short_names.get(at))
                        .map_or_else(|| romcat_core::path::file_name_of_key(key), String::as_str);
                    字(ui, 简称);
                    let 裁过 = page
                        .details
                        .iter()
                        .find(|detail| detail.row.key == key)
                        .is_some_and(|detail| detail.preferred.is_some());
                    look::help(
                        ui,
                        if 裁过 {
                            "裁决指定的"
                        } else {
                            "默认按「汉化 > 官中 > 日版 > 其他」选择"
                        },
                    );
                });
            });
        });
    }

    /// 媒体那一块（设计稿 `ovTab` 右边头一块 `.sect`）：标题右头「全部 N 个」换到媒体那一面；底下头几份一排
    /// `media-strip-columns` 格（4:3），没有媒体时说一句。
    fn media_strip_card(&self, ui: &mut egui::Ui) -> Option<PageAction> {
        let tokens = Tokens::builtin();
        let items = self.page_media_items().unwrap_or_default();
        section_card(ui, |ui| {
            let 全部 = card_header(ui, "媒体", |ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    !items.is_empty()
                        && ghost_small(ui, &format!("全部 {} 个", items.len())).clicked()
                })
                .inner
            });
            if items.is_empty() {
                look::help(ui, "暂无媒体。本地数据源不含图片，可以通过联网刮削获取。");
            } else {
                let 缝 = tokens.space.media_strip_gap;
                let 格数 = tokens.layout.media_strip_columns.max(1);
                let 格宽 = ((ui.available_width() - 缝 * (格数 - 1) as f32) / 格数 as f32)
                    .floor()
                    .max(0.0);
                let size = egui::vec2(格宽, 格宽 * tokens.layout.card_cover_ratio);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 缝;
                    for item in items.iter().take(格数) {
                        strip_thumb(ui, item, size, &self.gallery);
                    }
                });
            }
            全部.then_some(PageAction::ShowTab(Tab::Media))
        })
    }

    /// 「在文件系统中打开」：把这个变体在盘上所在的目录交给打开外部程序那一下（默认是系统的文件管理器）。
    ///
    /// 键折回盘上真名走核心库那一处（`Roots::real_path`，ADR-0020）；那块盘没插上、文件挪走了时说一句，不崩。
    /// **只读**：一个字节都不碰（ADR-0004）。
    pub(super) fn reveal(&mut self, site: &Site, key: &str) {
        let roots = match Roots::load(&site.catalog) {
            Ok(roots) => roots,
            Err(error) => {
                self.error = Some(format!("中立库读不动：{error}"));
                return;
            }
        };
        match roots.real_path(&RealFs, key) {
            Some(at) => {
                let 目录 = if at.is_dir() {
                    at
                } else {
                    let parent = at.parent().map(std::path::Path::to_path_buf);
                    parent.unwrap_or(at)
                };
                self.open_media(Clicked::Open(目录));
            }
            None => {
                self.notice = Some(format!(
                    "盘上找不到「{key}」：它所在的根未连接，或者文件已经挪走了。"
                ));
            }
        }
    }

    /// 办头上那一块与概览那一面上按下去的那一下。
    fn apply_page(&mut self, site: &Site, action: PageAction) {
        match action {
            PageAction::EditMeta => self.begin_meta_edit(),
            PageAction::Merge => self.open_merge_here(site),
            PageAction::Reveal(key) => self.reveal(site, &key),
            PageAction::ShowTab(tab) => {
                if let Some(page) = self.page.as_mut() {
                    page.tab = tab;
                }
            }
            // **范围就是这个作品底下那几个变体**：与屏头「刮削…」同一层弹层，只是交进去的名单不同。
            PageAction::Scrape => {
                let keys: Vec<String> = self
                    .work
                    .as_ref()
                    .map(|work| {
                        work.variants
                            .iter()
                            .map(|variant| variant.row.key.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                if !keys.is_empty() {
                    let shown = u64::try_from(keys.len()).unwrap_or(u64::MAX);
                    self.scrape.open(keys, shown);
                }
            }
        }
    }

    /// 作品详情页上列哪几份**媒体**：作品上挂着的，加上每个变体自己挂着的；同一份（同一层、同一类、同一个文件）只列一次。
    /// 详情页没开时是 `None`。后台解码那一份也照它解（`Screen::sync_media`）。
    pub(super) fn page_media_items(&self) -> Option<Vec<MediaItem>> {
        let page = self.page.as_ref()?;
        // 几个变体的清单并成一份、作品上那几份只留一遍，由核心库并（`detail::merge_media_items`）。
        Some(romcat_core::catalog::detail::merge_media_items(
            page.details
                .iter()
                .map(|detail| detail.media_items.as_slice()),
        ))
    }

    /// 「媒体」那一面（设计稿 `mediaTab`）：一句说明，底下一格一份——封面、截图、视频。格子最窄 `media-tile-min`，
    /// 这一栏摆得下几列就摆几列。按了「播放」「打开位置」交回那一下算什么。
    fn media_tab(&self, ui: &mut egui::Ui) -> Option<Clicked> {
        let tokens = Tokens::builtin();
        let items = self.page_media_items()?;
        ui.spacing_mut().item_spacing.y = 0.0;
        if items.is_empty() {
            empty_card(
                ui,
                "暂无媒体。本地数据源不含图片，可以勾选联网数据源刮削此作品。",
            );
            return None;
        }
        look::help(
            ui,
            "媒体按内容保存在媒体池中，同一张图不会重复存储。视频用系统默认播放器打开。",
        );
        ui.add_space(look::step(2));
        let 缝 = tokens.space.media_grid_gap;
        let 宽 = ui.available_width();
        let 列数 = ((宽 + 缝) / (tokens.layout.media_tile_min + 缝))
            .floor()
            .max(1.0);
        let 格宽 = ((宽 - 缝 * (列数 - 1.0)) / 列数).floor();
        let mut 动作 = None;
        for (at, 一排) in items.chunks(列数 as usize).enumerate() {
            if at > 0 {
                ui.add_space(缝);
            }
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 缝;
                for item in 一排 {
                    // 「当前封面」是核心库挑的那一张（`Catalog::cover_of`，与表上那一行行首、侧边详情头上同一处）。
                    let 当前封面 = self
                        .cover
                        .as_ref()
                        .is_some_and(|cover| cover.hash == item.hash && cover.ext == item.ext);
                    if let Some(按了) = media_tile(ui, item, 格宽, &self.gallery, 当前封面)
                    {
                        动作 = Some(按了);
                    }
                }
            });
        }
        if self.pool.is_none() {
            ui.add_space(look::step(2));
            look::help(ui, "媒体池不在工作目录里，图画不出来，也指不出文件在哪。");
        }
        动作
    }

    /// 「播放」「打开位置」、侧边详情点一格按下去那一下：**窗口里一个字节都不解码**，交给打开外部程序那一下
    /// （默认是系统默认程序）。调不起来时如实说一句，不崩；点了、可指不出文件时说一句为什么，别让人以为界面坏了。
    pub(super) fn open_media(&mut self, clicked: Clicked) {
        match clicked {
            Clicked::Open(at) => match (self.opener)(&at) {
                Ok(()) => {
                    self.notice = Some(format!(
                        "交给系统默认程序打开：{}",
                        romcat_core::path::display(&at)
                    ));
                }
                Err(说的) => self.error = Some(说的),
            },
            Clicked::Nothing(为什么) => self.notice = Some(为什么),
        }
    }

    /// 办「标题」那一面上按下去的那一下：走浏览屏上现成的那几个入口（隐藏记沉淀库、恢复、加一条裁决来源的叫法、
    /// 排一趟整理标题），办完元数据那一面手上那几份跟着重读。
    fn apply_title(&mut self, site: &mut Site, action: TitleAction) {
        match action {
            TitleAction::Hide(row) => self.suppress_title(site, &row),
            TitleAction::Restore(one) => self.lift_title(site, &one),
            TitleAction::Add => {
                if let Some(work) = self.detail.as_ref().and_then(|detail| detail.work.clone()) {
                    self.add_title(site, &work);
                }
            }
            TitleAction::Fold => self.ask_fold_titles(),
        }
        if let Some(page) = self.page.as_mut() {
            page.forget();
        }
    }

    /// 办「元数据」那一面上按下去的那一下。写进中立库的只走核心库那两个写入口（`put_verdict_value` /
    /// `clear_verdict_value`），写完整屏重读一遍：表上那一行的元数据那一格、详情页每一格都是照库里现在的样子画的。
    fn apply_meta(&mut self, site: &mut Site, action: MetaAction) {
        match action {
            MetaAction::BeginEdit => self.begin_meta_edit(),
            MetaAction::Toggle(subject, field) => {
                if let Some(page) = self.page.as_mut() {
                    let key = (subject, field);
                    if !page.open_alts.remove(&key) {
                        page.open_alts.insert(key);
                    }
                }
            }
            MetaAction::UseValue {
                anchor,
                subject,
                field,
                source,
                value,
            } => match site.catalog.put_verdict_value(
                anchor,
                &subject,
                field,
                &value,
                &format!("作品详情页上改用「{source}」的值"),
            ) {
                Ok(()) => {
                    self.refresh(site);
                    self.notice = Some(format!("已改为使用 {source} 的值（记为手动修改）"));
                }
                Err(error) => self.error = Some(format!("中立库写不动：{error}")),
            },
            MetaAction::UseTitle(row) => {
                let source = row.source.clone();
                let verdict = TitleRow {
                    source: VERDICT.to_owned(),
                    ..row
                };
                match site.catalog.put_titles(&[verdict]) {
                    Ok(()) => {
                        self.refresh(site);
                        self.notice = Some(format!("已改为使用 {source} 的名称（记为手动修改）"));
                    }
                    Err(error) => self.error = Some(format!("中立库写不动：{error}")),
                }
            }
            MetaAction::RevertTitle(row) => match site.catalog.remove_title(
                &row.work,
                row.language,
                row.kind,
                &row.source,
                &row.value,
            ) {
                Ok(_) => {
                    self.refresh(site);
                    self.notice = Some("已撤销手动修改，恢复为标题集合的选择".to_owned());
                }
                Err(error) => self.error = Some(format!("中立库写不动：{error}")),
            },
            MetaAction::Revert {
                anchor,
                subject,
                field,
            } => match site.catalog.clear_verdict_value(anchor, &subject, field) {
                Ok(_) => {
                    self.refresh(site);
                    self.notice = Some("已撤销手动修改，恢复为数据源的值".to_owned());
                }
                Err(error) => self.error = Some(format!("中立库写不动：{error}")),
            },
        }
    }
}

/// 作品详情页上一张**变体卡**的外壳（设计稿 `.vcard`）：面板底、一圈描边、大圆角，左沿一道置信度色条；头一行
/// （`.vhead`）左边摆 `head_left`、右头摆 `head_right`，底下一道线；再底下是身子，四周留 `body_padding`。
/// 交回 `head_right` 交回的东西。「变体与文件」与「识别依据」两面的卡都是它。
fn work_card<R>(
    ui: &mut egui::Ui,
    tier: Tier,
    head_right: impl FnOnce(&mut egui::Ui) -> R,
    head_left: impl FnOnce(&mut egui::Ui),
    body_padding: [f32; 2],
    body: impl FnOnce(&mut egui::Ui),
) -> R {
    let tokens = Tokens::builtin();
    let (底色, 线, 条色) = {
        let visuals = ui.visuals();
        (
            visuals.window_fill,
            visuals.widgets.noninteractive.bg_stroke,
            look::tier_color(tier, visuals),
        )
    };
    let shown = egui::Frame::new()
        .fill(底色)
        .stroke(egui::Stroke::new(tokens.layout.control_stroke, 线.color))
        .corner_radius(tokens.radius.large)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            let [头上下, 头左右] = tokens.space.work_card_head_padding;
            let 头 = egui::Frame::NONE
                .inner_margin(egui::Margin::from(egui::vec2(头左右, 头上下)))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = tokens.space.work_card_head_gap;
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let 右头 = head_right(ui);
                            ui.with_layout(Layout::left_to_right(Align::Center), head_left);
                            右头
                        })
                        .inner
                    })
                    .inner
                });
            let 线的位置 = 头.response.rect;
            ui.painter()
                .hline(线的位置.x_range(), 线的位置.bottom() - 线.width / 2.0, 线);
            let [身上下, 身左右] = body_padding;
            egui::Frame::NONE
                .inner_margin(egui::Margin::from(egui::vec2(身左右, 身上下)))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    body(ui);
                });
            头.inner
        });
    // 左沿那一道色条（设计稿 `box-shadow:inset 3px 0 0 var(--c)`）：把描边里头那块圆角矩形用那一档的颜色再铺一遍，
    // 只留左边令牌 `tier-bar` 那么宽一截——两个左角跟着卡片的圆角走。与侧边详情的变体卡片同一个画法。
    let 里头 = shown.response.rect.shrink(tokens.layout.control_stroke);
    ui.painter()
        .with_clip_rect(ui.clip_rect().intersect(egui::Rect::from_min_size(
            里头.min,
            egui::vec2(tokens.layout.tier_bar, 里头.height()),
        )))
        .rect_filled(
            里头,
            (f32::from(tokens.radius.large) - tokens.layout.control_stroke).max(0.0),
            条色,
        );
    shown.inner
}

/// 变体卡头一行左边那几样：变体简称（拉丁与数字加粗）、「首选变体」标签（`preferred` 为真时）、置信度标签、识别结论。
///
/// **那一档叫什么词、结论叫什么词，都是核心库答的**（`WorkVariant::confidence_label`、`State::label`）。
fn card_title(ui: &mut egui::Ui, variant: &WorkVariant, short_name: &str, preferred: bool) {
    let tokens = Tokens::builtin();
    let 强 = ui.visuals().strong_text_color();
    ui.label(
        egui::RichText::new(short_name)
            .font(egui::FontId::new(
                tokens.font.size_body,
                font::strong_family(),
            ))
            .color(强),
    );
    if preferred {
        table::tag(ui, PREFERRED_TAG);
    }
    look::chip(
        ui,
        look::tier_tone(Tier::of(variant.confidence())),
        variant.confidence_label(),
    );
    table::tag(ui, variant.state.map_or(NOT_RUN_LABEL, State::label));
}

/// 「变体与文件」那一面上一个变体的卡：头一行右头「设为首选变体」；身子左半是发行版那几格，右半是位置与文件表。
/// 右头还有一颗「在文件系统中打开」。按了哪一颗交回哪一颗。
///
/// 哪一个是首选由核心库答（`VariantDetail::is_preferred`）；「设为首选变体」只在这个变体眼下不是首选、作品与平台
/// 都认出来了的时候摆。
fn variant_card(
    ui: &mut egui::Ui,
    variant: &WorkVariant,
    detail: &VariantDetail,
    files: &[FileLine],
    short_name: &str,
    group: Option<&FieldShown>,
) -> Option<CardPress> {
    let tokens = Tokens::builtin();
    work_card(
        ui,
        Tier::of(variant.confidence()),
        |ui| {
            // 右往左摆：先摆的在最右（设计稿 `.vhead` 里「在文件系统中打开」排在最后）。
            let 打开 = ghost_small(ui, "在文件系统中打开").clicked();
            // **「移出此作品…」照稿排在它前面**：合并的反向操作，同样一条裁决、撤得回来。
            let 移出 = ghost_small(ui, super::merge::SPLIT)
                .on_hover_text(
                    "把这个变体从这个作品移出，放到新建的作品或另一个已有作品。\n\n\
                     同样只写入裁决记录，可以在「待确认 → 裁决记录」中撤销。",
                )
                .clicked();
            let 挑得了 = !detail.is_preferred()
                && !detail.siblings.is_empty()
                && detail.work.is_some()
                && detail.row.platform.is_some();
            let 设首选 = 挑得了
                && look::small_buttons(ui, |ui| {
                    ui.button("设为首选变体")
                        .on_hover_text(
                            "这个作品在这个平台上默认启动这一个。记为裁决：之后识别、刮削都不会把它改回去。",
                        )
                        .clicked()
                });
            // **只在这个变体就是那条首选裁决指的那一个时摆**：没人裁过，就没有可恢复的。
            let 裁过的 = detail.preferred.as_deref() == Some(detail.row.key.as_str())
                && detail.work.is_some()
                && detail.row.platform.is_some();
            let 恢复 = 裁过的
                && look::small_buttons(ui, |ui| {
                    ui.button("恢复规则选择")
                        .on_hover_text(
                            "撤掉这条首选变体裁决，照「汉化 > 官中 > 日版 > 其他」重新选。",
                        )
                        .clicked()
                });
            // **人工纠正出来的变体才摆撤销**（票 `gui-looks-like-the-design/29`）：是不是人工纠正
            // 出来的由核心库记着（`VariantRow::manual`），这一层不自己认。
            let 撤成型 = detail.row.manual
                && look::small_buttons(ui, |ui| {
                    ui.button(crate::shaping::UNDO)
                        .on_hover_text(
                            "清掉这一处的人工纠正，重新成型之后回到成型规则原本的结果；盘上的文件一个字节都不动。",
                        )
                        .clicked()
                });
            if 设首选 {
                Some(CardPress::Prefer)
            } else if 恢复 {
                Some(CardPress::Restore)
            } else if 撤成型 {
                Some(CardPress::UndoShaping)
            } else if 移出 {
                Some(CardPress::Split)
            } else if 打开 {
                Some(CardPress::Reveal)
            } else {
                None
            }
        },
        |ui| card_title(ui, variant, short_name, detail.is_preferred()),
        tokens.space.work_card_body_padding,
        |ui| {
            let 列距 = tokens.space.work_card_columns_gap;
            let 宽 = ui.available_width();
            // 左窄右宽，照稿 `.vbody`（令牌 `work-card-columns`）。
            let [左份, 右份] = tokens.layout.work_card_columns;
            let 左宽 = ((宽 - 列距) * 左份 / (左份 + 右份)).floor().max(0.0);
            let 右宽 = (宽 - 列距 - 左宽).max(0.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 列距;
                ui.vertical(|ui| {
                    ui.set_width(左宽);
                    release_facts(ui, variant, detail, group);
                });
                ui.vertical(|ui| {
                    ui.set_width(右宽);
                    location_and_files(ui, detail, files);
                });
            });
        },
    )
}

/// 「识别依据」那一面上一个变体的卡：头一行同「变体与文件」（不标首选）；身子是「判定依据」那一块提示框与候选表。
///
/// 判定依据写**置信度最高的那一条候选凭什么撞上的**，没定下来的连为什么；一条候选都没有、也没说为什么的，不摆那一块
/// ——候选表那一行照核心库那句说。
fn evidence_card(ui: &mut egui::Ui, variant: &WorkVariant, short_name: &str) {
    let tokens = Tokens::builtin();
    work_card(
        ui,
        Tier::of(variant.confidence()),
        |_| {},
        |ui| card_title(ui, variant, short_name, false),
        tokens.space.evidence_card_padding,
        |ui| {
            ui.spacing_mut().item_spacing.y = look::step(1);
            // 判定依据那一句照依据形状各段排，由核心库出字（`WorkVariant::basis_line`）。
            let 依据 = variant.basis_line();
            if 依据.is_some() || variant.reason.is_some() {
                look::note_box(ui, |ui| {
                    let 字号 = look::font_size(ui.ctx(), tokens.font.size_small_plus);
                    let (强, 次) = (ui.visuals().strong_text_color(), ui.visuals().text_color());
                    if let Some(依据) = &依据 {
                        let mut job = egui::text::LayoutJob::default();
                        job.append(
                            "判定依据：",
                            0.0,
                            egui::TextFormat::simple(egui::FontId::proportional(字号), 强),
                        );
                        job.append(
                            依据,
                            0.0,
                            egui::TextFormat::simple(egui::FontId::proportional(字号), 次),
                        );
                        ui.add(egui::Label::new(job).wrap());
                    }
                    if let Some(reason) = &variant.reason {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!("为什么没定下来：{reason}"))
                                    .size(字号)
                                    .color(次),
                            )
                            .wrap(),
                        );
                    }
                });
            }
            candidates_table(ui, variant);
        },
    );
}

/// 一个变体的候选表（设计稿识别依据那一面的 `table.ftbl`）：「候选」（撞上的条目，等宽、哪儿都能折）「来源」「置信度」，
/// 一条候选一行。最多列 `TOP_CANDIDATES` 条，多出来的说一句。
///
/// **一条候选都没有时是一行，照核心库那一句说**（`WorkVariant::no_candidate_hint`）：没有候选与还没识别，下一步不一样。
fn candidates_table(ui: &mut egui::Ui, variant: &WorkVariant) {
    let tokens = Tokens::builtin();
    let 格 = tokens.space.file_table_cell_padding;
    let 左右 = 格[1];
    let 字号 = look::font_size(ui.ctx(), tokens.font.size_small);
    let (底色, 线, 弱, 字) = {
        let visuals = ui.visuals();
        (
            visuals.window_fill,
            visuals.widgets.noninteractive.bg_stroke,
            visuals.weak_text_color(),
            visuals.strong_text_color(),
        )
    };
    let 常规 = egui::FontId::proportional(字号);
    let 列出的: Vec<&Candidate> = variant.candidates.iter().take(TOP_CANDIDATES).collect();
    let 来源宽 = 列出的
        .iter()
        .map(|candidate| candidate.source.as_str())
        .chain(["来源"])
        .map(|text| table::text_width(ui, text, &常规))
        .fold(0.0, f32::max)
        + 2.0 * 左右;
    let 置信度表头宽 = table::text_width(ui, "置信度", &常规);
    let 置信度宽 = 列出的
        .iter()
        .map(|candidate| chip_width(ui, candidate.confidence))
        .fold(置信度表头宽, f32::max)
        + 2.0 * 左右;
    egui::Frame::new()
        .fill(底色)
        .stroke(线)
        .corner_radius(tokens.radius.large)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            let 表宽 = ui.available_width();
            let 候选宽 = (表宽 - 来源宽 - 置信度宽).max(0.0);
            let 表头字 = |text: &str| egui::RichText::new(text).font(常规.clone()).color(弱);
            table_row(ui, 线, true, |ui| {
                cell(ui, 候选宽, 格, 表头字("候选").into());
                cell(ui, 来源宽, 格, 表头字("来源").into());
                cell(ui, 置信度宽, 格, 表头字("置信度").into());
            });
            if 列出的.is_empty() {
                let 那一句 = variant.no_candidate_hint().unwrap_or_default();
                table_row(ui, 线, false, |ui| {
                    cell(ui, 表宽, 格, 表头字(那一句).into());
                });
            }
            let 共 = 列出的.len();
            for (at, candidate) in 列出的.iter().enumerate() {
                table_row(ui, 线, at + 1 < 共, |ui| {
                    let mut job = egui::text::LayoutJob::simple(
                        candidate.game.clone(),
                        egui::FontId::monospace(字号),
                        字,
                        f32::INFINITY,
                    );
                    job.wrap.break_anywhere = true;
                    cell(ui, 候选宽, 格, job.into());
                    cell(
                        ui,
                        来源宽,
                        格,
                        egui::RichText::new(&candidate.source)
                            .font(常规.clone())
                            .color(字)
                            .into(),
                    );
                    cell_with(ui, 置信度宽, 格, |ui| {
                        look::chip(
                            ui,
                            look::tier_tone(Tier::of(Some(candidate.confidence))),
                            candidate.confidence.label(),
                        );
                    });
                });
            }
        });
    if variant.candidates.len() > TOP_CANDIDATES {
        look::help(
            ui,
            &format!(
                "……另有 {} 条候选没列",
                variant.candidates.len() - TOP_CANDIDATES
            ),
        );
    }
}

/// 那一档置信度的标签（`look::chip`）画出来多宽：在一块看不见、按不动的地方摆一遍量出来，不另算一遍它的几何。
fn chip_width(ui: &mut egui::Ui, confidence: romcat_core::catalog::Confidence) -> f32 {
    let mut 量 = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(("量置信度标签多宽", confidence.label()))
            .max_rect(ui.max_rect())
            .sizing_pass()
            .invisible(),
    );
    look::chip(
        &mut 量,
        look::tier_tone(Tier::of(Some(confidence))),
        confidence.label(),
    )
    .rect
    .width()
}

/// 表里一行：几格从左往右摆，`底下画线` 时底下画一道线（最后一行不画）。
fn table_row(
    ui: &mut egui::Ui, 线: egui::Stroke, 底下画线: bool, add: impl FnOnce(&mut egui::Ui)
) {
    let rect = ui.horizontal_top(add).response.rect;
    if 底下画线 {
        ui.painter()
            .hline(rect.x_range(), rect.bottom() - 线.width / 2.0, 线);
    }
}

/// 变体卡片左半：发行版那几格（设计稿 `.vbody` 左边那张 `dl.infol`）——发行版（已接受那条候选的 DAT 条目名）、地区、
/// 语言、序列号、汉化组、版本。没有发行版链接、或者那一格说不出来的写「—」。
///
/// 「版本」那一格写的是词表**第几版**（屏上照设计稿写「版本」）：**两层怎么挑由核心库一处判**
/// （`VariantDetail::edition`——裁决 > 发行版的修订 > 说不出），界面只把它印出来。
/// **说不出时写「—」**，不拿 `1.0` 去补（2026-09-20 拿主意的人定；稿上那一格画的是 `1.0`）。
fn release_facts(
    ui: &mut egui::Ui,
    variant: &WorkVariant,
    detail: &VariantDetail,
    group: Option<&FieldShown>,
) {
    let release = detail.release.as_ref();
    let 或破折号 = |value: Option<&str>| value.map_or_else(|| "—".to_owned(), str::to_owned);
    let 语言 = if detail.languages.is_empty() {
        "—".to_owned()
    } else {
        detail.languages.join(", ")
    };
    // **汉化组挂在变体上**（ADR-0012）：那一格写出去的是什么由核心库算好交进来（`priority::entry_fields`，与导出同一处）。
    let 汉化组 = group
        .and_then(|group| group.shown.as_ref())
        .map(|said| said.values.join("、"));
    info_list(
        ui,
        &[
            // 发行版：**已接受那条候选**撞上的 DAT 条目名（拿主意的人 2026-09-15 定，岔路口 4b）；一条都没接受时写「—」。
            (
                "发行版",
                或破折号(
                    variant
                        .accepted_candidate()
                        .map(|candidate| candidate.game.as_str()),
                ),
                true,
            ),
            (
                "地区",
                或破折号(release.and_then(|r| r.region.as_deref())),
                false,
            ),
            ("语言", 语言, false),
            (
                "序列号",
                或破折号(release.and_then(|r| r.serial.as_deref())),
                true,
            ),
            ("汉化组", 或破折号(汉化组.as_deref()), false),
            // **版本**：词表**第几版**。**不等宽**——稿上 `varTab` 那张 `dl.infol` 里只有
            // 发行版与序列号带 `class="mono"`，这一格没有。
            ("版本", 或破折号(detail.edition()), false),
        ],
    );
}

/// 「名 → 值」两列里的一行（设计稿 `dl.infol` 的一对 `dt` / `dd`）：名那一列令牌 `kv-key-width` 宽、弱字色、
/// 半号字，值那一栏摆 `add`。
fn info_row(ui: &mut egui::Ui, 名: &str, add: impl FnOnce(&mut egui::Ui)) {
    let tokens = Tokens::builtin();
    let 列距 = tokens.space.info_list_gap[1];
    let 字号 = look::font_size(ui.ctx(), tokens.font.size_small_plus);
    let 弱 = ui.visuals().weak_text_color();
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 列距;
        ui.allocate_ui_with_layout(
            egui::vec2(tokens.layout.kv_key_width, 0.0),
            Layout::left_to_right(Align::Min),
            |ui| {
                ui.set_min_width(tokens.layout.kv_key_width);
                ui.label(egui::RichText::new(名).size(字号).color(弱));
            },
        );
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
    });
}

/// 「名 → 值」两列（设计稿 `dl.infol`）：一行一对（[`info_row`]），值正文色、在这一栏里折行；行与行之间取令牌
/// `info-list-gap`，字是半号 `size-small-plus`。第三格为真的那几行，值用等宽。
fn info_list(ui: &mut egui::Ui, rows: &[(&str, String, bool)]) {
    let tokens = Tokens::builtin();
    let 行距 = tokens.space.info_list_gap[0];
    let 字号 = look::font_size(ui.ctx(), tokens.font.size_small_plus);
    let 字 = ui.visuals().strong_text_color();
    for (at, (名, 值, 等宽)) in rows.iter().enumerate() {
        if at > 0 {
            ui.add_space(行距);
        }
        let 字体 = if *等宽 {
            egui::FontId::monospace(字号)
        } else {
            egui::FontId::proportional(字号)
        };
        info_row(ui, 名, |ui| {
            ui.add(egui::Label::new(egui::RichText::new(值.as_str()).font(字体).color(字)).wrap());
        });
    }
}

/// 变体卡片右半（设计稿 `.vbody` 右边那一栏）：「位置」小标题底下是「根名 · 相对路径」，等宽、哪儿都能折；再底下是文件表。
/// 拆键由核心库做（`path::split_root`），这里只接起来。
fn location_and_files(ui: &mut egui::Ui, detail: &VariantDetail, files: &[FileLine]) {
    let tokens = Tokens::builtin();
    let 小 = look::font_size(ui.ctx(), tokens.font.size_small);
    look::section(ui, "位置");
    ui.add_space(look::step(0));
    let (根名, 相对) = romcat_core::path::split_root(&detail.row.key);
    let 路径 = if 相对.is_empty() {
        根名.to_owned()
    } else {
        format!("{根名}{}{相对}", table::ROOT_SEPARATOR)
    };
    let mut job = egui::text::LayoutJob::simple(
        路径,
        egui::FontId::monospace(小),
        ui.visuals().strong_text_color(),
        ui.available_width(),
    );
    job.wrap.break_anywhere = true;
    let galley = ui.painter().layout_job(job);
    ui.label(galley);
    ui.add_space(look::step(1));
    files_table(ui, detail, files);
}

/// 一个变体的文件表（设计稿 `.panel` 里的 `table.ftbl`）：类型、文件、大小（靠右）、CRC-32，一行一个文件，行与行之间一条线。
///
/// 那几行由核心库交出来（`Catalog::file_lines`）：透明容器是容器一行、里头的文件逐行跟着；成员那几行写它在这个变体里的
/// 位置（[`member_name`]），容器里头那几行写它在容器里的路径，都等宽、哪儿都能折。大小两位小数；CRC-32 八位大写十六进制，
/// 没算过写「—」。最多列 `TOP_MEMBERS` 个成员，多出来的说一句。
fn files_table(ui: &mut egui::Ui, detail: &VariantDetail, lines: &[FileLine]) {
    let tokens = Tokens::builtin();
    let 格 = tokens.space.file_table_cell_padding;
    let 左右 = 格[1];
    let 字号 = look::font_size(ui.ctx(), tokens.font.size_small);
    let (底色, 线, 弱, 字) = {
        let visuals = ui.visuals();
        (
            visuals.window_fill,
            visuals.widgets.noninteractive.bg_stroke,
            visuals.weak_text_color(),
            visuals.strong_text_color(),
        )
    };
    let 常规 = egui::FontId::proportional(字号);
    let 等宽 = egui::FontId::monospace(字号);
    let 类型字 = |line: &FileLine| -> &'static str { line.role.map_or("容器", Role::code) };
    let 大小字 = |line: &FileLine| line.size.map_or_else(|| "—".to_owned(), human_bytes);
    let 校验字 = |line: &FileLine| {
        line.crc32
            .map_or_else(|| "—".to_owned(), |crc| format!("{crc:08X}"))
    };
    let 类型宽 = lines
        .iter()
        .map(类型字)
        .chain(["类型"])
        .map(|text| table::text_width(ui, text, &常规))
        .fold(0.0, f32::max)
        + 2.0 * 左右;
    let 大小宽 = lines
        .iter()
        .map(|line| table::text_width(ui, &大小字(line), &常规))
        .fold(table::text_width(ui, "大小", &常规), f32::max)
        + 2.0 * 左右;
    let 校验宽 = table::text_width(ui, "FFFFFFFF", &等宽)
        .max(table::text_width(ui, "CRC-32", &常规))
        + 2.0 * 左右;
    egui::Frame::new()
        .fill(底色)
        .stroke(线)
        .corner_radius(tokens.radius.large)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            let 表宽 = ui.available_width();
            let 文件宽 = (表宽 - 类型宽 - 大小宽 - 校验宽).max(0.0);
            let 表头字 = |text: &str| -> egui::WidgetText {
                egui::RichText::new(text)
                    .font(常规.clone())
                    .color(弱)
                    .into()
            };
            table_row(ui, 线, !lines.is_empty(), |ui| {
                cell(ui, 类型宽, 格, 表头字("类型"));
                cell(ui, 文件宽, 格, 表头字("文件"));
                cell_with(ui, 大小宽, 格, |ui| {
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        ui.label(表头字("大小"));
                    });
                });
                cell(ui, 校验宽, 格, 表头字("CRC-32"));
            });
            let 共 = lines.len();
            for (at, line) in lines.iter().enumerate() {
                table_row(ui, 线, at + 1 < 共, |ui| {
                    cell(
                        ui,
                        类型宽,
                        格,
                        egui::RichText::new(类型字(line))
                            .font(常规.clone())
                            .color(字)
                            .into(),
                    );
                    let 名 = if line.inner {
                        line.name.clone()
                    } else {
                        member_name(&detail.row.key, &line.name)
                    };
                    let mut job =
                        egui::text::LayoutJob::simple(名, 等宽.clone(), 字, f32::INFINITY);
                    job.wrap.break_anywhere = true;
                    cell(ui, 文件宽, 格, job.into());
                    cell_with(ui, 大小宽, 格, |ui| {
                        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                            ui.label(
                                egui::RichText::new(大小字(line))
                                    .font(常规.clone())
                                    .color(字),
                            );
                        });
                    });
                    cell(
                        ui,
                        校验宽,
                        格,
                        egui::RichText::new(校验字(line))
                            .font(等宽.clone())
                            .color(字)
                            .into(),
                    );
                });
            }
        });
    if detail.members.len() > TOP_MEMBERS {
        look::help(
            ui,
            &format!("……另有 {} 个没列", detail.members.len() - TOP_MEMBERS),
        );
    }
}

/// 表里一格字：正好 `宽` 那么宽，四周留 `[上下, 左右]`，字在格里折行。
fn cell(ui: &mut egui::Ui, 宽: f32, padding: [f32; 2], text: egui::WidgetText) {
    cell_with(ui, 宽, padding, |ui| {
        ui.add(egui::Label::new(text).wrap());
    });
}

/// 表里一格：正好 `宽` 那么宽，四周留 `[上下, 左右]`，里头摆 `add`。
fn cell_with(
    ui: &mut egui::Ui, 宽: f32, [上下, 左右]: [f32; 2], add: impl FnOnce(&mut egui::Ui)
) {
    ui.allocate_ui_with_layout(egui::vec2(宽, 0.0), Layout::top_down(Align::Min), |ui| {
        ui.set_width(宽);
        egui::Frame::NONE
            .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
            .show(ui, |ui| {
                ui.set_width((宽 - 2.0 * 左右).max(0.0));
                add(ui);
            });
    });
}

/// 文件表里那一格写什么：这个成员在变体里的位置——变体是一整个目录时去掉目录那一截，单个文件就是文件名。
fn member_name(variant_key: &str, member_key: &str) -> String {
    member_key
        .strip_prefix(variant_key)
        .and_then(|rest| rest.strip_prefix('/'))
        .filter(|rest| !rest.is_empty())
        .map_or_else(
            || romcat_core::path::file_name_of_key(member_key).to_owned(),
            str::to_owned,
        )
}

/// 「元数据」那一面上按下去的那一下，画完这一帧再办（画的时候手上只有只读的那一份）。
#[derive(Debug, Clone, PartialEq, Eq)]
enum MetaAction {
    /// 「编辑」：进编辑态。
    BeginEdit,
    /// 「其他 N 个来源」摊开或收起：哪个锚点上的哪一格。
    Toggle(String, Field),
    /// 「使用这个值」：把那个源说的那一句写成这一格的裁决。
    UseValue {
        /// 挂在哪一层。
        anchor: AnchorKind,
        /// 作品名，或变体的键。
        subject: String,
        /// 哪一格。
        field: Field,
        /// 那一句是哪个源说的（记进裁决的依据里）。
        source: String,
        /// 那一句。
        value: String,
    },
    /// 「使用这个名称」：把标题集合里某个已有名称复制成裁决，成为显示标题。
    UseTitle(TitleRow),
    /// 撤掉这条手动加的显示标题，回到标题集合本来的选择。
    RevertTitle(TitleRow),
    /// 撤销这一格的裁决，回到数据源说了算。
    Revert {
        /// 挂在哪一层。
        anchor: AnchorKind,
        /// 作品名，或变体的键。
        subject: String,
        /// 哪一格。
        field: Field,
    },
}

/// 「元数据」那一面一行左边写的名：标题那一格写「显示标题」（词表**显示标题**），别的照字段的词。
fn field_label(field: Field) -> &'static str {
    match field {
        Field::Title => "显示标题",
        other => other.label(),
    }
}

/// 编辑态下底下那一条上按了哪一颗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveAction {
    /// 保存。
    Save,
    /// 放弃。
    Discard,
}

/// 「元数据」那一面的一行（设计稿 `.mrow`）：左边名那一列（名，底下一行小字说挂在哪一层）、正中值那一栏（`value`）、
/// 右头动作那一列（`act`，靠右）。名与动作两列的宽取令牌 `meta-row-columns`，四周留白 `meta-row-padding`；
/// `line` 时底下一道线（最后一行不画）。`dirty`（编辑态下改过）时铺一层强调色的浅底、左沿一道强调色竖条
/// （设计稿 `.mrow.dirty`）。交回两栏里按下去的那一下。
fn meta_row(
    ui: &mut egui::Ui,
    名: &str,
    层: &str,
    line: bool,
    dirty: bool,
    value: impl FnOnce(&mut egui::Ui) -> Option<MetaAction>,
    act: impl FnOnce(&mut egui::Ui) -> Option<MetaAction>,
) -> Option<MetaAction> {
    let tokens = Tokens::builtin();
    let [上下, 左右] = tokens.space.meta_row_padding;
    let [名宽, 动作宽] = tokens.layout.meta_row_columns;
    let 列距 = tokens.space.meta_row_gap[1];
    let (字, 弱, 线, 强调) = {
        let visuals = ui.visuals();
        (
            visuals.strong_text_color(),
            visuals.weak_text_color(),
            visuals.widgets.noninteractive.bg_stroke,
            visuals.selection.stroke.color,
        )
    };
    let 底 = if dirty {
        强调.gamma_multiply(tokens.mix.dirty_row_tint)
    } else {
        egui::Color32::TRANSPARENT
    };
    let shown = egui::Frame::NONE
        .fill(底)
        .corner_radius(tokens.radius.medium)
        .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 列距;
                ui.vertical(|ui| {
                    ui.set_width(名宽);
                    ui.spacing_mut().item_spacing.y = tokens.space.meta_label_gap;
                    ui.label(
                        egui::RichText::new(名)
                            .size(look::font_size(ui.ctx(), tokens.font.size_small_plus))
                            .color(字),
                    );
                    ui.label(
                        egui::RichText::new(层)
                            .size(tokens.font.size_caption)
                            .color(弱),
                    );
                });
                let 值宽 = (ui.available_width() - 动作宽 - 列距).max(0.0);
                let 值 = ui
                    .vertical(|ui| {
                        ui.set_width(值宽);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        value(ui)
                    })
                    .inner;
                let 动 = ui
                    .vertical(|ui| {
                        ui.set_width(动作宽);
                        ui.with_layout(Layout::right_to_left(Align::Min), act).inner
                    })
                    .inner;
                值.or(动)
            })
            .inner
        });
    let rect = shown.response.rect;
    if dirty {
        ui.painter().rect_filled(
            egui::Rect::from_min_size(
                rect.min,
                egui::vec2(tokens.layout.row_stripe, rect.height()),
            ),
            0.0,
            强调,
        );
    }
    if line {
        ui.painter()
            .hline(rect.x_range(), rect.bottom() - 线.width / 2.0, 线);
    }
    shown.inner
}

/// 编辑态下一行正中那一栏（设计稿 `metaTab` 编辑那一支）：一个输入框（简介是多行），底下一排这一格上各个源说过的值，
/// 点一个就把它填进框里；一个都没有时说一句可以直接填。显示标题那一格底下再说一句改了之后会怎样。
///
/// **框子的身份只由「哪一层、哪个锚点、哪个字段」定**（`id_salt`）：那一行冒出「已修改」、底下保存那一条的字变了，
/// 框子都还是同一个——正在组字时框子换了身份，上屏的字就没人接（ADR-0005）。
fn field_edit(
    ui: &mut egui::Ui,
    draft: &mut Draft,
    anchor: AnchorKind,
    subject: &str,
    field: Field,
    picks: &[(String, String)],
) {
    let tokens = Tokens::builtin();
    let id = ("作品详情页草稿", anchor.label(), subject, field.label());
    let 宽 = ui.available_width();
    if field == Field::Description {
        ui.add(
            egui::TextEdit::multiline(&mut draft.text)
                .id_salt(id)
                .desired_width(宽)
                .desired_rows(tokens.layout.meta_textarea_rows),
        );
    } else {
        look::text_input(
            ui,
            宽,
            egui::TextEdit::singleline(&mut draft.text).id_salt(id),
        );
    }
    ui.add_space(look::step(1));
    if picks.is_empty() {
        look::help(ui, "没有数据源提供这个字段，可以直接填写。");
    } else {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing =
                egui::vec2(tokens.space.pick_chip_gap, tokens.space.pick_chip_gap);
            for (source, value) in picks {
                if pick_chip(ui, source, value).clicked() {
                    draft.text = value.clone();
                }
            }
        });
    }
    if field == Field::Title && anchor == AnchorKind::Work {
        ui.add_space(look::step(1));
        look::help(
            ui,
            "修改后会作为手动添加的名称加入标题集合，并优先作为显示标题。",
        );
    }
}

/// 编辑态下一格底下那一排里的一个（设计稿 `.pickchip`）：一圈描边、面板底、小圆角，左边来源标签，右边那个值
/// （最宽 `pick-chip-max`，再长截尾巴）。整个点得中。
fn pick_chip(ui: &mut egui::Ui, source: &str, value: &str) -> egui::Response {
    let tokens = Tokens::builtin();
    let [左, 右] = tokens.space.pick_chip_padding;
    let (底, 线, 字) = {
        let visuals = ui.visuals();
        (
            visuals.window_fill,
            visuals.widgets.inactive.bg_stroke,
            visuals.strong_text_color(),
        )
    };
    let 框 = egui::Frame::new()
        .fill(底)
        .stroke(线)
        .corner_radius(tokens.radius.small)
        .inner_margin(egui::Margin {
            left: 左 as i8,
            right: 右 as i8,
            top: 0,
            bottom: 0,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = tokens.space.pick_chip_gap;
                ui.spacing_mut().interact_size.y = tokens.layout.pick_chip_height;
                ui.set_min_height(tokens.layout.pick_chip_height);
                source_badge(ui, source, source == VERDICT);
                ui.scope(|ui| {
                    ui.set_max_width(tokens.layout.pick_chip_max);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(value)
                                .size(look::font_size(ui.ctx(), tokens.font.size_small))
                                .color(字),
                        )
                        .selectable(false)
                        .truncate(),
                    );
                });
            });
        })
        .response;
    let response = ui.interact(
        框.rect,
        ui.id().with(("作品详情页可点的值", source, value)),
        egui::Sense::click(),
    );
    look::focus_ring(ui.ctx(), ui.clip_rect(), &response);
    response.on_hover_text(value)
}

/// 编辑态下改过的那一格右头那一枚「已修改」（强调浅底、强调字，设计稿编辑态 `.act` 里那一枚 `.tag`）。
fn dirty_tag(ui: &mut egui::Ui) {
    let tokens = Tokens::builtin();
    let palette = tokens
        .color
        .theme(egui::Theme::from_dark_mode(ui.visuals().dark_mode));
    let galley = ui.painter().layout_no_wrap(
        "已修改".to_owned(),
        egui::FontId::proportional(look::font_size(ui.ctx(), tokens.font.size_caption_plus)),
        palette.accent_ink,
    );
    let size = egui::vec2(
        galley.size().x + 2.0 * tokens.layout.tag_padding,
        tokens.layout.tag_height,
    );
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, tokens.radius.small, palette.accent_soft);
    ui.painter().galley(
        rect.center() - galley.size() / 2.0,
        galley,
        palette.accent_ink,
    );
}

/// 「元数据」那一面一行正中那一栏：写出去的值（没有就写「暂无」；简介最多三行），底下一排来源标签——显示标题那一格
/// 跟一句它怎么来的，别的格有别家说法时一颗「其他 N 个来源」；`open` 时再摊开别家说的那几句，一句一颗「使用这个值」。
fn field_value(
    ui: &mut egui::Ui,
    one: &FieldShown,
    anchor: AnchorKind,
    subject: &str,
    open: bool,
    title_alts: &[TitleRow],
) -> Option<MetaAction> {
    let tokens = Tokens::builtin();
    let 集合挑的 = one.field == Field::Title && anchor == AnchorKind::Work;
    let 字 = ui.visuals().strong_text_color();
    match &one.shown {
        Some(said) => {
            let 字号 = tokens.font.size_body;
            let mut job = egui::text::LayoutJob::simple(
                said.values.join("、"),
                egui::FontId::proportional(字号),
                字,
                ui.available_width(),
            );
            for section in &mut job.sections {
                section.format.line_height = Some(字号 * tokens.font.meta_value_line_height);
            }
            // 简介最多三行，余下的省略（设计稿 `.v.clamp`）；原文一个字都没动，编辑时整段在框里。
            if one.field == Field::Description {
                job.wrap.max_rows = tokens.layout.meta_clamp_rows;
                job.wrap.overflow_character = Some('…');
            }
            let galley = ui.painter().layout_job(job);
            ui.label(galley);
        }
        None => {
            look::help(ui, "暂无");
        }
    }
    ui.add_space(tokens.space.meta_row_gap[0]);
    let 别家: Vec<&ScrapedValue> = one
        .offered
        .iter()
        .filter(|value| !is_shown(one.shown.as_ref(), value))
        .collect();
    let 别的名称: Vec<&TitleRow> = title_alts
        .iter()
        .filter(|row| {
            !one.shown
                .as_ref()
                .is_some_and(|said| said.values.contains(&row.value))
        })
        .collect();
    let 别家数 = if 集合挑的 {
        别的名称.len()
    } else {
        别家.len()
    };
    let mut 动作 = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = look::step(1);
        if 集合挑的 {
            // 显示标题由标题集合按规则挑，徽标照稿写「标题集合」（拿主意的人 2026-09-15 定）。
            source_badge(ui, "标题集合", false);
        } else if let Some(said) = &one.shown {
            let 源 = said.source.as_deref();
            // 标题集合是空的、退回作品名时，那不是哪个源说的。
            source_badge(ui, 源.unwrap_or("作品名"), 源 == Some(VERDICT));
        }
        if 集合挑的 {
            look::help(ui, "由标题集合按规则选出");
        }
        if 别家数 > 0 {
            let 字 = format!("其他 {} 个来源 {}", 别家数, if open { "▴" } else { "▾" });
            if ghost_small(ui, &字).clicked() {
                动作 = Some(MetaAction::Toggle(subject.to_owned(), one.field));
            }
        }
    });
    if open && 别家数 > 0 {
        ui.add_space(look::step(1));
        for (at, value) in 别家.iter().enumerate() {
            if at > 0 {
                ui.add_space(tokens.space.alts_gap);
            }
            if alt_row(ui, &value.source, &value.value) {
                动作 = Some(MetaAction::UseValue {
                    anchor,
                    subject: subject.to_owned(),
                    field: one.field,
                    source: value.source.clone(),
                    value: value.value.clone(),
                });
            }
        }
        for (at, row) in 别的名称.iter().enumerate() {
            if at > 0 || !别家.is_empty() {
                ui.add_space(tokens.space.alts_gap);
            }
            if alt_row(ui, &row.source, &row.value) {
                动作 = Some(MetaAction::UseTitle((*row).clone()));
            }
        }
    }
    动作
}

/// 这一句是不是眼下写出去的那一句：同一个源说的，而且在写出去的那几句里。
fn is_shown(shown: Option<&Said>, value: &ScrapedValue) -> bool {
    shown.is_some_and(|said| {
        said.source.as_deref() == Some(value.source.as_str()) && said.values.contains(&value.value)
    })
}

/// 「其他来源」里的一句（设计稿 `.alt`）：次级底、一圈描边、中圆角；来源标签、那一句（折行）、右头「使用这个值」。
/// 按了交回 `true`。
fn alt_row(ui: &mut egui::Ui, source: &str, value: &str) -> bool {
    let tokens = Tokens::builtin();
    let [上下, 左右] = tokens.space.alt_padding;
    let (底色, 线, 字) = {
        let visuals = ui.visuals();
        (
            visuals.faint_bg_color,
            visuals.widgets.noninteractive.bg_stroke,
            visuals.strong_text_color(),
        )
    };
    egui::Frame::new()
        .fill(底色)
        .stroke(线)
        .corner_radius(tokens.radius.medium)
        .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = tokens.space.alt_gap;
                source_badge(ui, source, source == VERDICT);
                let 按钮宽 = look::small_button_width(ui, "使用这个值");
                let 值宽 = (ui.available_width() - 按钮宽 - tokens.space.alt_gap).max(0.0);
                ui.vertical(|ui| {
                    ui.set_width(值宽);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(value)
                                .size(look::font_size(ui.ctx(), tokens.font.size_small_plus))
                                .color(字),
                        )
                        .wrap(),
                    );
                });
                look::small_buttons(ui, |ui| ui.button("使用这个值").clicked())
            })
            .inner
        })
        .inner
}

/// 来源标签（设计稿 `.srcb`）：凹陷底、弱字色、小号字；来源是**裁决**的那一条照稿印「手动」、换强调浅底与强调字（`.srcb.man`），
/// 悬停说清它记成了裁决。
fn source_badge(ui: &mut egui::Ui, text: &str, verdict: bool) -> egui::Response {
    let tokens = Tokens::builtin();
    let palette = tokens
        .color
        .theme(egui::Theme::from_dark_mode(ui.visuals().dark_mode));
    let (底, 字) = if verdict {
        (palette.accent_soft, palette.accent_ink)
    } else {
        (palette.sunken, palette.ink_3)
    };
    let galley = ui.painter().layout_no_wrap(
        if verdict { MANUAL } else { text }.to_owned(),
        egui::FontId::proportional(tokens.font.size_caption),
        字,
    );
    let size = egui::vec2(
        galley.size().x + 2.0 * tokens.space.source_badge_padding,
        tokens.layout.source_badge_height,
    );
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_filled(rect, tokens.radius.small, 底);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, 字);
    if verdict {
        response.on_hover_text(MANUAL_HOVER)
    } else {
        response
    }
}

/// 一颗小号幽灵按钮（设计稿 `.btn.sm.ghost`）。
fn ghost_small(ui: &mut egui::Ui, text: &str) -> egui::Response {
    look::small_buttons(ui, |ui| {
        ui.scope(|ui| {
            look::ghost_button(ui.visuals_mut());
            ui.button(text)
        })
        .inner
    })
}

/// 「标题」那一面上按下去的那一下，画完这一帧再办。
#[derive(Debug, Clone)]
enum TitleAction {
    /// 「隐藏」这一条叫法（`title::suppress`：删掉那一行，并在沉淀库里记一条压制）。
    Hide(TitleRow),
    /// 「恢复」这一条压制。
    Restore(TitleSuppression),
    /// 「添加」：把草稿里那个名称加进标题集合，来源记作裁决。
    Add,
    /// 恢复之后就地那一颗「整理标题」：排一趟与库屏工序段同一趟的整理标题。
    Fold,
}

/// 排序标题从哪儿来那一句：带着核心库说的那个出处（`SortFrom::label`），一个拉丁标题都没有时说清得人工补一个。
fn sort_note(from: SortFrom) -> String {
    match from {
        SortFrom::Display => format!("取自{}：它本身就排得动", from.label()),
        SortFrom::LatinTitle | SortFrom::WorkName => {
            format!("取自{}，避免中文按编码排序", from.label())
        }
        SortFrom::None => format!(
            "{}：只能照显示标题排，按编码排等于乱排，得人工补一个",
            from.label()
        ),
    }
}

/// 一块**段落卡**（设计稿 `.sect`）：面板底、一圈描边、大圆角，四周留 `section-card-padding`，里头一块一块紧挨着摆。
fn section_card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let tokens = Tokens::builtin();
    let [上下, 左右] = tokens.space.section_card_padding;
    let (底色, 线) = (
        ui.visuals().window_fill,
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    egui::Frame::new()
        .fill(底色)
        .stroke(线)
        .corner_radius(tokens.radius.large)
        .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui)
        })
        .inner
}

/// 一块**空态卡**（设计稿 `.empty.card`）：卡片底、四周留 `empty-padding`，一句弱色的话居中。
fn empty_card(ui: &mut egui::Ui, text: &str) {
    let tokens = Tokens::builtin();
    let (底色, 线) = (
        ui.visuals().window_fill,
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    egui::Frame::new()
        .fill(底色)
        .stroke(线)
        .corner_radius(tokens.radius.large)
        .inner_margin(egui::Margin::same(tokens.space.empty_padding as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                look::help(ui, text);
            });
        });
}

/// 来源标签（[`source_badge`]）画出来多宽。
fn source_badge_width(ui: &egui::Ui, text: &str) -> f32 {
    let tokens = Tokens::builtin();
    table::text_width(
        ui,
        text,
        &egui::FontId::proportional(tokens.font.size_caption),
    ) + 2.0 * tokens.space.source_badge_padding
}

/// 标题集合那张表（设计稿 `titleTab` 的 `table.tbl`）：名称（拉丁与数字加粗）、语言、类型、来源、使用的变体（靠右）、
/// 右头「隐藏」，一条一行。最多列 `TOP_TITLES` 条，多出来的说一句。按了哪一行的「隐藏」交回那一条。
///
/// **语言与类型那两个词是核心库的**（`Language::label`、`TitleKind::label`，词表**标题集合**那一条的写法）。
fn titles_table(ui: &mut egui::Ui, rows: &[TitleRow]) -> Option<TitleAction> {
    let tokens = Tokens::builtin();
    let 头格 = tokens.space.table_head_padding;
    let 身格 = tokens.space.cell_padding;
    let 左右 = 身格[1];
    let 头字号 = look::font_size(ui.ctx(), tokens.font.size_caption_plus);
    let (底色, 线, 弱, 字) = {
        let visuals = ui.visuals();
        (
            visuals.window_fill,
            visuals.widgets.noninteractive.bg_stroke,
            visuals.weak_text_color(),
            visuals.strong_text_color(),
        )
    };
    let 常规 = egui::FontId::proportional(tokens.font.size_body);
    let 头 = egui::FontId::proportional(头字号);
    let 等宽 = egui::FontId::monospace(tokens.font.size_body);
    let 列出的: Vec<&TitleRow> = rows.iter().take(TOP_TITLES).collect();
    let 量 = |texts: &mut dyn Iterator<Item = String>, font: &egui::FontId, header: &str| {
        texts
            .map(|text| table::text_width(ui, &text, font))
            .fold(table::text_width(ui, header, &头), f32::max)
            + 2.0 * 左右
    };
    let 语言宽 = 量(
        &mut 列出的.iter().map(|row| row.language.label().to_owned()),
        &常规,
        "语言",
    );
    let 类型宽 = 量(
        &mut 列出的.iter().map(|row| row.kind.label().to_owned()),
        &常规,
        "类型",
    );
    let 来源宽 = 列出的
        .iter()
        .map(|row| source_badge_width(ui, &row.source))
        .fold(table::text_width(ui, "来源", &头), f32::max)
        + 2.0 * 左右;
    let 变体宽 = 量(
        &mut 列出的.iter().map(|row| thousands(row.seen)),
        &等宽,
        "使用的变体",
    );
    let 动作宽 = look::small_button_width(ui, "隐藏") + 2.0 * 左右;
    let mut 动作 = None;
    egui::Frame::new()
        .fill(底色)
        .stroke(线)
        .corner_radius(tokens.radius.large)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            let 表宽 = ui.available_width();
            // **照稿按比例分**（协调人 2026-09-15 定）：「名称」占令牌 `title-name-share` 那几成，语言、类型、来源、使用的变体
            // 均摊剩下的（哪一列的字比均摊的宽就照字宽），「隐藏」贴右；名称拿最后剩下的。
            let 名称占 = (表宽 * tokens.layout.title_name_share).floor();
            let 均摊 = ((表宽 - 名称占 - 动作宽) / 4.0).floor().max(0.0);
            let (语言宽, 类型宽, 来源宽, 变体宽) = (
                语言宽.max(均摊),
                类型宽.max(均摊),
                来源宽.max(均摊),
                变体宽.max(均摊),
            );
            let 名称宽 = (表宽 - 语言宽 - 类型宽 - 来源宽 - 变体宽 - 动作宽).max(0.0);
            let 表头字 = |text: &str| -> egui::WidgetText {
                egui::RichText::new(text).font(头.clone()).color(弱).into()
            };
            table_row(ui, 线, !列出的.is_empty(), |ui| {
                cell(ui, 名称宽, 头格, 表头字("名称"));
                cell(ui, 语言宽, 头格, 表头字("语言"));
                cell(ui, 类型宽, 头格, 表头字("类型"));
                cell(ui, 来源宽, 头格, 表头字("来源"));
                cell_with(ui, 变体宽, 头格, |ui| {
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        ui.label(表头字("使用的变体"));
                    });
                });
                cell(ui, 动作宽, 头格, 表头字(""));
            });
            let 共 = 列出的.len();
            for (at, row) in 列出的.iter().enumerate() {
                table_row(ui, 线, at + 1 < 共, |ui| {
                    cell(
                        ui,
                        名称宽,
                        身格,
                        egui::RichText::new(&row.value)
                            .font(egui::FontId::new(tokens.font.size_body, font::strong_family()))
                            .color(字)
                            .into(),
                    );
                    cell(
                        ui,
                        语言宽,
                        身格,
                        egui::RichText::new(row.language.label())
                            .font(常规.clone())
                            .color(字)
                            .into(),
                    );
                    cell(
                        ui,
                        类型宽,
                        身格,
                        egui::RichText::new(row.kind.label())
                            .font(常规.clone())
                            .color(字)
                            .into(),
                    );
                    cell_with(ui, 来源宽, 身格, |ui| {
                        source_badge(ui, &row.source, row.is_verdict());
                    });
                    cell_with(ui, 变体宽, 身格, |ui| {
                        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                            ui.label(
                                egui::RichText::new(thousands(row.seen))
                                    .font(等宽.clone())
                                    .color(字),
                            );
                        });
                    });
                    cell_with(ui, 动作宽, 身格, |ui| {
                        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                            if ghost_small(ui, "隐藏")
                                .on_hover_text(
                                    "从标题集合里去掉这一条。刮削来的也去得掉——记一条压制，重新整理标题也不会把它加回来，\
                                     底下「已隐藏的名称」里恢复得了。",
                                )
                                .clicked()
                            {
                                动作 = Some(TitleAction::Hide((*row).clone()));
                            }
                        });
                    });
                });
            }
        });
    if rows.len() > TOP_TITLES {
        ui.add_space(look::step(0));
        look::help(ui, &format!("……另有 {} 条没列", rows.len() - TOP_TITLES));
    }
    动作
}

/// 「添加一个名称」那一排（设计稿 `.tadd`）：输入框吃剩下的宽，语言、类型两个下拉照令牌 `title-add-columns` 宽，右头
/// 主按钮「添加」（框里空着时按不下去）。按了「添加」交回 `true`。
///
/// 输入框的身份定死（`id_salt`）：这一排在一块不虚拟化的滚动区里，正在组字时不会被收掉（ADR-0005）。
fn title_add_row(ui: &mut egui::Ui, draft: &mut TitleDraft) -> bool {
    let tokens = Tokens::builtin();
    let [语言宽, 类型宽] = tokens.layout.title_add_columns;
    let 缝 = look::step(1);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 缝;
        let 按钮宽 = look::button_width(ui, "添加");
        let 输入宽 = (ui.available_width() - 语言宽 - 类型宽 - 按钮宽 - 3.0 * 缝).max(0.0);
        look::text_input(
            ui,
            输入宽,
            egui::TextEdit::singleline(&mut draft.value)
                .id_salt("作品详情页添加一个名称")
                .hint_text("添加一个名称"),
        );
        egui::ComboBox::from_id_salt("作品详情页添加名称的语言")
            .width(语言宽)
            .selected_text(draft.language.label())
            .show_ui(ui, |ui| {
                for language in Language::all() {
                    ui.selectable_value(&mut draft.language, language, language.label());
                }
            });
        egui::ComboBox::from_id_salt("作品详情页添加名称的类型")
            .width(类型宽)
            .selected_text(draft.kind.label())
            .show_ui(ui, |ui| {
                for kind in TitleKind::all() {
                    ui.selectable_value(&mut draft.kind, kind, kind.label());
                }
            });
        let ready = !draft.value.trim().is_empty();
        look::buttons(ui, |ui| {
            ui.scope(|ui| {
                look::primary_button(ui.visuals_mut());
                ui.add_enabled(ready, egui::Button::new("添加"))
            })
            .inner
            .clicked()
        })
    })
    .inner
}

/// 「已隐藏的名称」那一块（设计稿 `titleTab` 底下那一块 `.sect`）：标题「已隐藏的名称 · N」，一条一行——名称、
/// 「语言 · 类型 · 来源」、右头「恢复」。按了哪一条的「恢复」交回那一条。
fn suppressed_card(ui: &mut egui::Ui, 压掉的: &[TitleSuppression]) -> Option<TitleSuppression> {
    let tokens = Tokens::builtin();
    let (强, 线) = (
        ui.visuals().strong_text_color(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    let mut 恢复 = None;
    section_card(ui, |ui| {
        ui.label(
            egui::RichText::new(format!("已隐藏的名称 · {}", 压掉的.len()))
                .size(look::font_size(ui.ctx(), tokens.font.size_section_title))
                .color(强),
        );
        ui.add_space(tokens.space.section_card_title_gap);
        for one in 压掉的.iter().take(TOP_TITLES) {
            let 顶 = ui.cursor().top();
            ui.painter().hline(ui.max_rect().x_range(), 顶, 线);
            ui.add_space(tokens.space.suppressed_row_padding);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = look::step(1);
                ui.label(egui::RichText::new(&one.value).color(强));
                look::help(
                    ui,
                    &format!(
                        "{} · {} · {}",
                        one.language.label(),
                        one.kind.label(),
                        one.source
                    ),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if look::small_buttons(ui, |ui| {
                        ui.button("恢复")
                            .on_hover_text("撤掉这条压制。下一趟整理标题之后它回到标题集合里。")
                            .clicked()
                    }) {
                        恢复 = Some(one.clone());
                    }
                });
            });
            ui.add_space(tokens.space.suppressed_row_padding);
        }
    });
    恢复
}

/// 媒体那一面的一格（设计稿 `.mtile`）：面板底、一圈描边、大圆角；上半截是预览（封面 3:4，别的 4:3，[`media_preview`]），
/// 底下写它是什么、哪个源给的，一排按钮——视频「播放」，核心库挑中的那张封面标「当前封面」，每一格都有「打开位置」。
/// 按了哪一颗交回那一下算什么。
fn media_tile(
    ui: &mut egui::Ui,
    item: &MediaItem,
    宽: f32,
    gallery: &Gallery,
    当前封面: bool,
) -> Option<Clicked> {
    let tokens = Tokens::builtin();
    let (底色, 线, 强) = {
        let visuals = ui.visuals();
        (
            visuals.window_fill,
            visuals.widgets.noninteractive.bg_stroke,
            visuals.strong_text_color(),
        )
    };
    let 高 = if item.kind == MediaKind::Cover {
        宽 / tokens.layout.card_cover_ratio
    } else {
        宽 * tokens.layout.card_cover_ratio
    };
    let mut 动作 = None;
    egui::Frame::new()
        .fill(底色)
        .stroke(线)
        .corner_radius(tokens.radius.large)
        .show(ui, |ui| {
            let 里宽 = (宽 - 2.0 * 线.width).max(0.0);
            ui.set_width(里宽);
            // **框里头竖着摆**：这一格摆在一排横着的格子里，`Frame` 的内容沿用外头那一排的横排——不另起竖排的话，
            // 预览与底下那几行字会并排挤成一长条，后头几格被挤出视口。
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                media_preview(ui, item, egui::vec2(里宽, 高), gallery);
                let [上下, 左右] = tokens.space.media_info_padding;
                egui::Frame::NONE
                    .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = look::step(0);
                        ui.label(
                            egui::RichText::new(item.kind.label())
                                .size(tokens.font.size_body)
                                .color(强),
                        );
                        // 「来源 · 尺寸 · 时长 · 大小」（设计稿 `.mi .help` 那一行，`mediaOf` 的
                        // `src` / `dim` / `size`）。中间两段是**入池那一刻量下来的**
                        // （`scrape::measure`）；量不出来的那几段整段不写，于是老库与没装
                        // ffmpeg 的机器上这一行退回「来源 · 大小」——不写「未知」，那是
                        // 拿一句废话占一格。
                        look::help(ui, &media_facts(item));
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = look::step(1);
                            if preview::is_video(&item.ext) {
                                if look::small_buttons(ui, |ui| ui.button("播放").clicked()) {
                                    动作 = Some(crate::media::clicked_on(item));
                                }
                            } else if 当前封面 {
                                table::tag(ui, "当前封面");
                            }
                            if ghost_small(ui, "打开位置").clicked() {
                                // 打开的是它在媒体池里所在的那个目录；指不出文件时照点一格那样说为什么。
                                动作 = Some(match crate::media::clicked_on(item) {
                                    Clicked::Open(at) => {
                                        Clicked::Open(at.parent().map_or_else(
                                            || at.clone(),
                                            std::path::Path::to_path_buf,
                                        ))
                                    }
                                    nothing => nothing,
                                });
                            }
                        });
                    });
            });
        });
    动作
}

/// 媒体那一格上半截的预览（设计稿 `.mtile .pv`）：解出来的盖满这一块（多出来的那一边两头裁掉），视频再压一层半透明底
/// 与 ▶；**这台机器没有 ffmpeg、抽不出首帧**时画斜纹占位并说清原因（设计稿 `.noff`）——不是错误，视频照样放得了；
/// 还在解、池子不在位、解不开时一块凹陷底写一句。
fn media_preview(ui: &mut egui::Ui, item: &MediaItem, size: egui::Vec2, gallery: &Gallery) {
    let tokens = Tokens::builtin();
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let palette = tokens
        .color
        .theme(egui::Theme::from_dark_mode(ui.visuals().dark_mode));
    let (凹, 次级, 弱) = (
        ui.visuals().extreme_bg_color,
        ui.visuals().faint_bg_color,
        ui.visuals().weak_text_color(),
    );
    let painter = ui.painter_at(rect);
    let 一句 = |text: String| {
        let 字号 = look::font_size(ui.ctx(), tokens.font.size_caption_plus);
        let mut job = egui::text::LayoutJob::simple(
            text,
            egui::FontId::proportional(字号),
            弱,
            (rect.width() - 2.0 * tokens.space.noff_padding).max(0.0),
        );
        job.halign = egui::Align::Center;
        painter.layout_job(job)
    };
    match gallery.look(item) {
        Look::Ready(texture) => {
            egui::Image::new(texture)
                .uv(crate::media::cover_uv(texture.size_vec2(), size))
                .paint_at(ui, rect);
            if preview::is_video(&item.ext) {
                let mark = &tokens.color.video;
                painter.rect_filled(rect, 0.0, mark.shade);
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "▶",
                    egui::FontId::proportional(tokens.font.size_play_mark),
                    mark.mark,
                );
            }
        }
        Look::Missing(why) if why.is_no_ffmpeg() => {
            stripes(&painter, rect, 凹, 次级, tokens.layout.noff_stripe);
            let 行们 = [
                painter.layout_no_wrap(
                    "▶".to_owned(),
                    egui::FontId::proportional(tokens.font.size_noff_mark),
                    palette.ink_4,
                ),
                一句("没有找到 ffmpeg，无法生成预览帧".to_owned()),
                一句("视频文件完好，可以直接播放".to_owned()),
            ];
            centered_lines(&painter, rect, &行们, tokens.space.noff_gap);
        }
        Look::Missing(why) => {
            painter.rect_filled(rect, 0.0, 凹);
            centered_lines(
                &painter,
                rect,
                &[一句(format!("{}：{}", item.kind.label(), why.render()))],
                0.0,
            );
        }
        Look::Waiting => {
            painter.rect_filled(rect, 0.0, 凹);
            centered_lines(&painter, rect, &[一句("…".to_owned())], 0.0);
        }
        Look::NoPool => {
            painter.rect_filled(rect, 0.0, 凹);
            centered_lines(&painter, rect, &[一句("没查池子".to_owned())], 0.0);
        }
    }
}

/// 几段排好的字竖着摞起来、在这一块里上下左右居中，段与段之间隔 `gap`。
///
/// **交给 `painter.galley` 的那个点是哪儿，由这一段自己的 `halign` 说了算**：`Min` 时是左上角，
/// `Center` 时是顶边中点，`Max` 时是右上角。不照它分，一段 `halign = Center` 的字会被这里再
/// 往左挪半个身位——那正是没装 ffmpeg 时那两句话左半截被格子裁掉的原因（右沿正好落在格子正中）。
fn centered_lines(
    painter: &egui::Painter,
    rect: egui::Rect,
    lines: &[std::sync::Arc<egui::Galley>],
    gap: f32,
) {
    let 总高: f32 = lines.iter().map(|galley| galley.size().y).sum::<f32>()
        + gap * lines.len().saturating_sub(1) as f32;
    let mut y = rect.center().y - 总高 / 2.0;
    for galley in lines {
        let 半宽 = galley.size().x / 2.0;
        let x = match galley.job.halign {
            egui::Align::Min => rect.center().x - 半宽,
            egui::Align::Center => rect.center().x,
            egui::Align::Max => rect.center().x + 半宽,
        };
        painter.galley(egui::pos2(x, y), galley.clone(), egui::Color32::PLACEHOLDER);
        y += galley.size().y + gap;
    }
}

/// 斜纹底（设计稿 `.noff` 的 `repeating-linear-gradient(135deg, sunken 0 10px, panel-2 10px 20px)`）：先铺一层次级底，
/// 再每隔一个来回画一道 45° 的凹陷色宽条（垂直方向正好 `宽` 那么粗），只画在这一块里（`painter` 已经裁在这一块上）。
fn stripes(
    painter: &egui::Painter,
    rect: egui::Rect,
    条色: egui::Color32,
    底色: egui::Color32,
    宽: f32,
) {
    painter.rect_filled(rect, 0.0, 底色);
    if 宽 <= 0.0 {
        return;
    }
    let 步 = 2.0 * 宽 * std::f32::consts::SQRT_2;
    let 高 = rect.height();
    let mut x = rect.left() - 高;
    while x < rect.right() + 步 {
        painter.line_segment(
            [egui::pos2(x, rect.bottom()), egui::pos2(x + 高, rect.top())],
            egui::Stroke::new(宽, 条色),
        );
        x += 步;
    }
}

/// 头上那一块与概览那一面上按下去的那一下，画完这一帧再办。
#[derive(Debug, Clone, PartialEq, Eq)]
enum PageAction {
    /// 「编辑元数据」「编辑」：换到元数据那一面，直接进编辑态。
    EditMeta,
    /// 「全部 N 个」：换到那一面。
    ShowTab(Tab),
    /// 「刮削此作品」：摊开刮削那一层弹层，范围就是这个作品底下那几个变体。
    Scrape,
    /// 「合并…」：从这一个作品起头开合并向导。
    Merge,
    /// 「在文件系统中打开」：这个变体在盘上所在的目录交给系统。
    Reveal(String),
}

/// 变体卡片头一行右头按下去的是哪一颗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CardPress {
    /// 「设为首选变体」。
    Prefer,
    /// 「恢复规则选择」：撤掉这个作品在这个平台上的首选变体裁决。
    Restore,
    /// 「移出此作品…」：摊开那一层弹层。
    Split,
    /// 「在文件系统中打开」。
    Reveal,
    /// 「调整成型…」：开**成型纠正**那一层，说的是这一面上第几处存疑
    /// （票 `gui-looks-like-the-design/29`）。
    Adjust(usize),
    /// 「撤销成型纠正」：清掉这个变体上那几行人工纠正，回到成型规则原本的结果。
    UndoShaping,
}

/// 头一个平台上的**首选变体**：那个平台上某个变体的详情里核心库排好的第一名（`VariantDetail::preferred_now`）。
fn head_of<'a>(work: &WorkDetail, details: &'a [VariantDetail]) -> Option<&'a str> {
    let platform = work.platforms.first()?;
    details
        .iter()
        .filter(|detail| detail.row.platform.as_deref() == Some(platform.as_str()))
        .find_map(VariantDetail::preferred_now)
}

/// 平台色块标签（设计稿 `.hplat`）：平台色底、小号加粗的平台代号。字取 `on-accent`（强色底上的字那一格），不照稿写死白字
/// ——同挂单 `Q894` 那一条。
fn platform_chip(ui: &mut egui::Ui, platform: &str) {
    let tokens = Tokens::builtin();
    let palette = tokens
        .color
        .theme(egui::Theme::from_dark_mode(ui.visuals().dark_mode));
    let galley = ui.painter().layout_no_wrap(
        platform.to_owned(),
        egui::FontId::new(
            look::font_size(ui.ctx(), tokens.font.size_mini),
            font::strong_family(),
        ),
        palette.on_accent,
    );
    let size = egui::vec2(
        galley.size().x + 2.0 * tokens.layout.tag_padding,
        tokens.layout.tag_height,
    );
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_filled(
        rect,
        tokens.radius.small,
        tokens.color.platform.of(platform),
    );
    ui.painter().galley(
        rect.center() - galley.size() / 2.0,
        galley,
        palette.on_accent,
    );
}

/// 头上那几格事实（设计稿 `.hfacts`）：平台、年份、类型、开发商、发行商、变体，一排 `hero-facts-columns` 格、整排最宽
/// `hero-facts-max`；名在上（小号弱字），值在下（正文、拉丁与数字加粗，一行放不下截尾巴）。
fn hero_facts(ui: &mut egui::Ui, work: &WorkDetail, page: &Page) {
    let tokens = Tokens::builtin();
    let 写出去的 = |field: Field| {
        page.fields
            .iter()
            .find(|one| one.field == field)
            .and_then(|one| one.shown.as_ref())
            .map_or_else(|| "—".to_owned(), |said| said.values.join("、"))
    };
    let 变体 = page.row.as_ref().map_or_else(
        || format!("{} 个", work.variants.len()),
        |row| {
            format!(
                "{} 个 · {}",
                row.variants,
                capacity(row.bytes, row.unreadable_files)
            )
        },
    );
    // 照稿写全名，由核心库的平台表给，表里没写全名的写代号（拿主意的人 2026-09-15 定；只在这一格）。
    let 平台 = if page.platform_names.is_empty() {
        "—".to_owned()
    } else {
        page.platform_names.clone()
    };
    let 格们 = [
        ("平台", 平台),
        ("年份", 写出去的(Field::Year)),
        ("类型", 写出去的(Field::Genre)),
        ("开发商", 写出去的(Field::Developer)),
        ("发行商", 写出去的(Field::Publisher)),
        ("变体", 变体),
    ];
    let 列距 = tokens.space.hero_facts_gap[1];
    let 列数 = tokens.layout.hero_facts_columns.max(1) as f32;
    let 宽 = ui.available_width().min(tokens.layout.hero_facts_max);
    let 格宽 = ((宽 - 列距 * (列数 - 1.0)) / 列数).floor().max(0.0);
    let (强, 弱) = (
        ui.visuals().strong_text_color(),
        ui.visuals().weak_text_color(),
    );
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 列距;
        for (名, 值) in &格们 {
            ui.vertical(|ui| {
                ui.set_width(格宽);
                ui.spacing_mut().item_spacing.y = tokens.space.hero_fact_gap;
                ui.label(
                    egui::RichText::new(*名)
                        .size(look::font_size(ui.ctx(), tokens.font.size_caption_plus))
                        .color(弱),
                );
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(值.as_str())
                            .font(egui::FontId::new(
                                tokens.font.size_body,
                                font::strong_family(),
                            ))
                            .color(强),
                    )
                    .truncate(),
                );
            });
        }
    });
}

/// 段落卡头一行（设计稿 `.sect > .row:first-child`）：标题（半号 `size-section-title`），`add` 摆在标题后头（来源标签、
/// 右头的按钮），底下空 `section-card-title-gap`。交回 `add` 交回的东西。
fn card_header<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let tokens = Tokens::builtin();
    let 强 = ui.visuals().strong_text_color();
    let inner = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = look::step(1);
            ui.label(
                egui::RichText::new(title)
                    .size(look::font_size(ui.ctx(), tokens.font.size_section_title))
                    .color(强),
            );
            add(ui)
        })
        .inner;
    ui.add_space(tokens.space.section_card_title_gap);
    inner
}

/// 简介那一块（设计稿 `ovTab` 头一块 `.sect`）：标题后头跟来源标签，右头「编辑」进元数据那一面的编辑态；底下那一段简介
/// （导出真会写出去的那一条），没有就说怎么补。
fn description_card(ui: &mut egui::Ui, page: &Page) -> Option<PageAction> {
    let tokens = Tokens::builtin();
    let 简介 = page
        .fields
        .iter()
        .find(|one| one.field == Field::Description)
        .and_then(|one| one.shown.as_ref());
    let 强 = ui.visuals().strong_text_color();
    section_card(ui, |ui| {
        let 编辑 = card_header(ui, "简介", |ui| {
            if let Some(said) = 简介 {
                let 源 = said.source.as_deref().unwrap_or("作品名");
                source_badge(ui, 源, 源 == VERDICT);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ghost_small(ui, "编辑").clicked()
            })
            .inner
        });
        match 简介 {
            Some(said) => {
                let 字号 = look::font_size(ui.ctx(), tokens.font.size_desc);
                let mut job = egui::text::LayoutJob::simple(
                    said.values.join("\n"),
                    egui::FontId::proportional(字号),
                    强,
                    ui.available_width(),
                );
                for section in &mut job.sections {
                    section.format.line_height = Some(字号 * tokens.font.desc_line_height);
                }
                let galley = ui.painter().layout_job(job);
                ui.label(galley);
            }
            None => {
                look::help(ui, "暂无简介。可以刮削此作品，或手动填写。");
            }
        }
        编辑.then_some(PageAction::EditMeta)
    })
}

/// 状态那一块（设计稿 `ovTab` 右边第二块 `.sect`）：识别——逐个变体的识别结论照出现的次序数个数（词是核心库的
/// `State::label` 与「还没识别」）；元数据——表上那一行的短标签；收藏、子库、导出各一行。
///
/// **稿上夹在「收藏」与「子库」中间的「合集」那一行不在这儿**：它连同那颗「加入合集…」归票 13，
/// 这一块照稿的次序把位置给它留着。
///
/// 「子库」与「导出」两行的判断**一个字都不在这儿**（票 `gui-looks-like-the-design/34`）：
/// 落在哪几个子库里由求值那一处答（`sublibrary::holding`），上次几点写出去的由导出那一趟
/// 逐条记下的账答（`Catalog::entry_exported`）。界面只把它们印出来。
fn status_card(ui: &mut egui::Ui, work: &WorkDetail, page: &Page) {
    let tokens = Tokens::builtin();
    let 字号 = look::font_size(ui.ctx(), tokens.font.size_small_plus);
    let 强 = ui.visuals().strong_text_color();
    let mut 数: Vec<(&'static str, usize)> = Vec::new();
    for variant in &work.variants {
        let 词 = variant.state.map_or(NOT_RUN_LABEL, State::label);
        match 数.iter_mut().find(|(one, _)| *one == 词) {
            Some(one) => one.1 += 1,
            None => 数.push((词, 1)),
        }
    }
    let 识别 = 数
        .iter()
        .map(|(词, n)| format!("{词} {n} 个"))
        .collect::<Vec<_>>()
        .join(" · ");
    section_card(ui, |ui| {
        card_header(ui, "状态", |_| {});
        info_row(ui, "识别", |ui| {
            ui.add(egui::Label::new(egui::RichText::new(&识别).size(字号).color(强)).wrap());
        });
        if let Some(row) = &page.row {
            ui.add_space(tokens.space.info_list_gap[0]);
            info_row(ui, "元数据", |ui| {
                ui.label(egui::RichText::new(row.meta_label()).size(字号).color(强));
            });
        }
        // 收藏：只读地写一行（设计稿状态块「收藏」那一格）；收没收藏、钉在哪种锚上由核心库答（`collection::favorite_of`）。
        ui.add_space(tokens.space.info_list_gap[0]);
        info_row(ui, "收藏", |ui| {
            let 留神 = look::tone_colors(look::Tone::Caution, ui.visuals()).0;
            let mut job = egui::text::LayoutJob::default();
            let mut 段 = |text: &str, color: egui::Color32| {
                job.append(
                    text,
                    0.0,
                    egui::TextFormat::simple(egui::FontId::proportional(字号), color),
                );
            };
            match page.favorite {
                None => 段("未收藏", 强),
                Some(romcat_core::verdict::ANCHOR_CONTENT) => 段("已收藏 · 按文件内容记录", 强),
                Some(_) => {
                    段("已收藏 · ", 强);
                    段("只按路径记录", 留神);
                    段("：变体没有内容判据，文件改名或移动后会丢失", 强);
                }
            }
            ui.add(egui::Label::new(job).wrap());
        });
        // 子库：落在哪几个子库的选择集里，名字之间「、」（设计稿 `ovTab` 的 `subs`）。
        // **一个都没落进写「—」**——与别处「那一格说不出来」同一个记号。
        ui.add_space(tokens.space.info_list_gap[0]);
        info_row(ui, "子库", |ui| {
            let 话 = if page.sublibraries.is_empty() {
                "—".to_owned()
            } else {
                page.sublibraries.join("、")
            };
            ui.add(egui::Label::new(egui::RichText::new(话).size(字号).color(强)).wrap());
        });
        // 导出：「已导出到 Pegasus · 09-03 15:41」（设计稿 `ovTab` 那一行）。
        //
        // **没有那一条就说「还没导出」**，不拿整库那个时刻顶上去：上次导出之后才扫进来的
        // 作品在这儿本来就该是「还没导出」，而整库那个数会让它跟着说「已导出」——
        // 那正是这一行最该答对的一种情形（`catalog::export` 的模块文档）。
        ui.add_space(tokens.space.info_list_gap[0]);
        info_row(ui, "导出", |ui| {
            let 话 = page.exported.as_ref().map_or_else(
                || "还没导出".to_owned(),
                |mark| format!("已导出到 {} · {}", mark.format, human_time(mark.at)),
            );
            ui.add(egui::Label::new(egui::RichText::new(话).size(字号).color(强)).wrap());
        });
    });
}

/// 媒体那一格底下那行小字：「来源 · 尺寸 · 时长 · 大小」（设计稿 `mediaOf` 的 `src · dim · size`）。
///
/// **中间两段量不出来就整段不写**：这一版不解的图片格式、半截文件、这台机器上没有 ffmpeg，
/// 以及老库里加那三列之前就已经入过池的那些（`catalog::scrape::add_columns` 不回填）——
/// 四种都归到同一个处置上，因为对这一行来说它们是同一件事：**没量过**。
///
/// 数怎么排由核心库说（`report::pixel_size` / `report::media_duration`），与报告、命令行
/// 印的是同一个数。
fn media_facts(item: &MediaItem) -> String {
    let mut 几段 = vec![item.source.clone()];
    if let Some((width, height)) = item.measured.size() {
        几段.push(report::pixel_size(width, height));
    }
    if let Some(ms) = item.measured.duration_ms {
        几段.push(report::media_duration(ms));
    }
    几段.push(human_bytes(item.bytes));
    几段.join(" · ")
}

/// 概览里媒体那一块的一格（设计稿 `.mstrip div`）：凹陷底、中圆角、一圈分隔线；解出来的盖满这一格，没有 ffmpeg 抽不出
/// 首帧的画斜纹与「▶ 视频」（设计稿 `.noff`），别的空着。
fn strip_thumb(ui: &mut egui::Ui, item: &MediaItem, size: egui::Vec2, gallery: &Gallery) {
    let tokens = Tokens::builtin();
    let look = gallery.look(item);
    if let Look::Ready(texture) = look {
        crate::media::paint_cover(ui, size, tokens.radius.medium, texture);
        return;
    }
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let (凹, 次级, 弱, 线) = {
        let visuals = ui.visuals();
        (
            visuals.extreme_bg_color,
            visuals.faint_bg_color,
            visuals.weak_text_color(),
            visuals.widgets.noninteractive.bg_stroke,
        )
    };
    let painter = ui.painter_at(rect);
    if matches!(look, Look::Missing(why) if why.is_no_ffmpeg()) {
        stripes(&painter, rect, 凹, 次级, tokens.layout.noff_stripe);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "▶ 视频",
            egui::FontId::proportional(look::font_size(ui.ctx(), tokens.font.size_caption_plus)),
            弱,
        );
    } else {
        painter.rect_filled(rect, tokens.radius.medium, 凹);
    }
    ui.painter()
        .rect_stroke(rect, tokens.radius.medium, 线, egui::StrokeKind::Outside);
}
