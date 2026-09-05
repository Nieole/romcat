//! **库浏览**：翻整个主库、按四个维度收窄、点开一行改它的元数据。
//!
//! ## 这一屏要回答的三个问题
//!
//! 1. **库里有什么**——中间那张表，虚拟化，四万多行滚起来的代价与总行数无关（票 22）。
//! 2. **我要找的那一批在哪**——左边那四个筛选：**平台**、**合集**、**语言**、
//!    **识别状态**。四条一律下推到中立库的 `WHERE`（[`romcat_core::catalog::browse`]），
//!    内存里永远只有当前视口那几十行。
//! 3. **这一条到底是什么**——底下那块面板：作品、发行版、**标题集合**、**首选变体**、
//!    **媒体**缺哪些。改也在那里改。
//!
//! ## 所有元数据编辑收敛在这里（ADR-0001 的修订段）
//!
//! 主库的 Pegasus 文件不再是编辑入口。于是这一屏必须真的改得动库：加一条**裁决**来源的
//! 叫法、删一条叫法、指定或撤销**首选变体**。这几件事各自都只是一次中立库写入——
//! 领域判断（怎么去重、裁决优先于任何数据源、规则算出来的首选是哪一个）一条都不在这里，
//! 全在 [`romcat_core::catalog::detail`]。
//!
//! ## ADR-0012：**首选变体与标题来源解耦**
//!
//! 这两样在面板上挨着，而它们恰恰不是一件事——首选变体决定**默认启动哪一个**，
//! 中文标题永远取**官中版的官方译名**，即使首选启动的是汉化版。面板上把这句话写成一行
//! 摆出来，而不是让人自己去推。
//!
//! ## 中文输入全在底下那块面板里
//!
//! 一个 [`egui::TextEdit`] 都不进表格单元格：表格是虚拟化的，正在组字的那一行一旦滚出
//! 视口，那个控件就不存在了（ADR-0005 的修订段）。左边的筛选栏里也一个都没有——
//! 那四个维度是**选**出来的不是打出来的，值从中立库现问（[`Catalog::facets`]）。

use egui::{Align, Layout};
use romcat_core::catalog::browse::{Facets, PlatformFilter};
use romcat_core::catalog::{Catalog, VariantDetail, VariantQuery};
use romcat_core::report::{capacity, human_bytes, thousands};
use romcat_core::scrape::pool::MediaPool;
use romcat_core::scrape::priority::VERDICT;
use romcat_core::scrape::{AnchorKind, Field, Priorities};
use romcat_core::site::Site;
use romcat_core::title::{Language, TitleKind};

use crate::font;
use crate::table::{SPAN, Table, Window};

/// 界面上人工写下的叫法，**依据**里写这一句。
///
/// 没有依据的结论事后无法复核（ADR-0002）。人工写的那条依据只能是「谁在哪儿写的」，
/// 但那也比空着强——半年后看见一个来路不明的中文名，至少知道它是自己敲的。
const HAND_WRITTEN: &str = "库浏览的详情面板上人工写的";

/// 底下那块面板里，**标题集合**最多列几条。
///
/// 一部作品的叫法在真库里能攒到几十条（每个源一条、每种语言一条），而面板上那一栏
/// 只有半屏高。列到这儿打住，末尾说清还有多少条没列。
const TOP_TITLES: usize = 24;

/// 面板上**文件成员**最多列几条。
///
/// 与标题分开定：一个变体可以是**一整个目录**（`CONTEXT.md` 的「变体」词条），
/// PSV 那批目录树转储一个变体底下就是上千个文件。这个数管的是「扫一眼看得完」，
/// 与「一部作品有几个叫法」不是同一件事，共用一个常量迟早会为了一边把另一边调坏。
const TOP_MEMBERS: usize = 40;

/// 刮削字段值那一列，一行最多画多少个字。
///
/// **这是画法，不是产品决定**：一条简介在中立库里最多 4,000 字
/// （`scrape::zh::DESCRIPTION_LIMIT`），而这一行画在一条**横排**里——横排不折行，
/// 4,000 个汉字就是四五万像素宽的一行，面板会被它撑出一条横向滚动条，那份清单也就
/// 滚不动了。原文一个字都没动，整段挂在悬停里。
const VALUE_SHOWN: usize = 60;

/// 把一个字段值收成**一行画得下的那一截**。
///
/// 返回 `None` 表示原样画得下，不必动它。两件事都要管：
///
/// - **换行**。数据源的排版原样留在值里（规格 18），可横排里一个换行就把那一行撑高。
/// - **长度**。超过 [`VALUE_SHOWN`] 个字就用省略号收住。
///
/// **原文一个字都没动**——收窄的只是画出来的那一行，整段挂在悬停里。
///
/// 它是公开的，因为「一条 4,000 字的简介不会把这块面板撑出去」这条验收就钉在它上面：
/// 那一行画在详情面板深处，egui 不画视口之外的文字，headless 的一帧里根本够不着它
/// （挂单 Q20）。
#[must_use]
pub fn one_line(value: &str) -> Option<String> {
    let flat = value.replace(['\n', '\r'], " ");
    if flat.chars().count() > VALUE_SHOWN {
        let head: String = flat.chars().take(VALUE_SHOWN).collect();
        return Some(format!("{head}…"));
    }
    (flat != value).then_some(flat)
}

/// 加一条叫法时界面上那份草稿。
#[derive(Debug, Clone)]
pub struct TitleDraft {
    /// 这一串字。**唯一会碰到输入法的地方**，所以它在详情面板里。
    pub value: String,
    /// 哪种语言。
    pub language: Language,
    /// 哪一种叫法。
    pub kind: TitleKind,
}

impl Default for TitleDraft {
    fn default() -> Self {
        Self {
            value: String::new(),
            // 人在这儿手敲的绝大多数是中文译名——那正是这个项目缺的那一半。
            language: Language::Chinese,
            kind: TitleKind::Translated,
        }
    }
}

/// 写下一个**刮削字段值**时界面上那份草稿。
#[derive(Debug, Clone)]
pub struct ValueDraft {
    /// 哪个字段。
    pub field: Field,
    /// 挂在**作品**上还是**变体**上。年份挂作品、汉化组挂变体（ADR-0012）。
    pub anchor: AnchorKind,
    /// 值本身。**会碰到输入法**，所以它在详情面板里。
    pub value: String,
}

impl Default for ValueDraft {
    fn default() -> Self {
        Self {
            // 简介是最想手写的那一个：离线档撞上一条中文条目就有（`scrape::zh`），
            // 而撞不上的那些正是没人替它写过一句话的。
            field: Field::Description,
            anchor: AnchorKind::Work,
            value: String::new(),
        }
    }
}

/// 库浏览这个屏幕。
pub struct Screen {
    window: Window,
    /// 筛选与排序。**界面上这一份是源头**，[`Window`] 里那一份是它的副本，每帧同步一次。
    query: VariantQuery,
    /// 四个维度各有哪些值可选。换库或改过元数据才重问。
    facets: Facets,
    /// 作品 id → 作品名。表里那一列作品名从它来（见 [`crate::table::Table::works`]）。
    works: std::collections::BTreeMap<i64, String>,
    /// 选中的是哪一行（全序下标）。
    selected: Option<u64>,
    /// 选中那一行的键。**记键不记下标**：换个筛选表就重排了，下标会指到别人身上。
    /// 底下那块面板认的是它。
    picked: Option<String>,
    /// 点开的那一条的详情。
    detail: Option<VariantDetail>,
    /// 挑**显示标题**用的优先级表。与导出走同一份，于是面板上写着的就是导出会写的。
    priorities: Priorities,
    /// **媒体池**：查「这张图在不在」用它。池子整个不在位时是 `None`——
    /// 那时面板如实说「没查池子」，而不是报一句「一张都没有」。
    pool: Option<MediaPool>,
    /// 加一条叫法的草稿。
    title_draft: TitleDraft,
    /// 写下一个刮削字段值的草稿。
    value_draft: ValueDraft,
    /// 上一次动作的回执。
    notice: Option<String>,
    /// 上一次出的错。
    error: Option<String>,
    /// 字体样张开着没有。
    sample: bool,
    /// 把表格的滚动位置强按到这个像素偏移。**只有量帧率时才设**，真界面上永远是 `None`。
    pub scroll_to: Option<f32>,
}

impl Default for Screen {
    fn default() -> Self {
        Self::new()
    }
}

impl Screen {
    /// 开一个空屏幕。
    #[must_use]
    pub fn new() -> Self {
        Self {
            window: Window::new(SPAN),
            query: VariantQuery::default(),
            facets: Facets::default(),
            works: std::collections::BTreeMap::new(),
            selected: None,
            picked: None,
            detail: None,
            priorities: Priorities::builtin(),
            pool: None,
            title_draft: TitleDraft::default(),
            value_draft: ValueDraft::default(),
            notice: None,
            error: None,
            sample: false,
            scroll_to: None,
        }
    }

    /// 换一份优先级表。**导出用哪一份，这里就该用哪一份**——两份不一样的话，
    /// 面板上写着的显示标题就不是同步到掌机上会看见的那个。
    pub fn set_priorities(&mut self, priorities: Priorities) {
        self.priorities = priorities;
    }

    /// 指一份**媒体池**。**目录不在就不指**——「没查」与「查了、没有」得分得开。
    pub fn set_pool(&mut self, pool: Option<MediaPool>) {
        self.pool = pool;
    }

    /// 重问一次筛选面板上的可选值。开库时与改过元数据之后各一次。
    pub fn reload(&mut self, site: &Site) {
        match site.catalog.facets() {
            Ok(facets) => {
                self.facets = facets;
                self.error = None;
            }
            Err(error) => self.error = Some(format!("中立库读不动：{error}")),
        }
        match site.catalog.work_names() {
            Ok(works) => self.works = works,
            Err(error) => self.error = Some(format!("中立库读不动：{error}")),
        }
    }

    /// 窗口，供测试查「内存里装了几行」。
    #[must_use]
    pub fn window(&self) -> &Window {
        &self.window
    }

    /// 眼下的筛选与排序。
    #[must_use]
    pub fn query(&self) -> &VariantQuery {
        &self.query
    }

    /// 改筛选与排序。实测与测试拿它当界面上点的那一下。
    pub fn query_mut(&mut self) -> &mut VariantQuery {
        &mut self.query
    }

    /// 四个维度各有哪些值可选。
    #[must_use]
    pub fn facets(&self) -> &Facets {
        &self.facets
    }

    /// 点开的那一条的详情。
    #[must_use]
    pub fn detail(&self) -> Option<&VariantDetail> {
        self.detail.as_ref()
    }

    /// 上一次出的错。
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// 上一次动作的回执。
    #[must_use]
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// 加一条叫法的草稿，供实测与测试填。
    pub fn title_draft_mut(&mut self) -> &mut TitleDraft {
        &mut self.title_draft
    }

    /// 写下一个刮削字段值的草稿，供实测与测试填。
    pub fn value_draft_mut(&mut self) -> &mut ValueDraft {
        &mut self.value_draft
    }

    /// 把草稿里那个字段值写下，**来源记作裁决**。
    ///
    /// 优先级表把裁决排在每个字段的最前，所以写下之后**导出真会用它**。
    /// 界面上「写下」那个按钮走的就是它。
    pub fn put_value(&mut self, site: &mut Site, subject: &str) {
        let value = self.value_draft.value.trim().to_string();
        if value.is_empty() {
            return;
        }
        match site.catalog.put_verdict_value(
            self.value_draft.anchor,
            subject,
            self.value_draft.field,
            &value,
            HAND_WRITTEN,
        ) {
            Ok(()) => {
                self.notice = Some(format!(
                    "{} 记成了「{value}」，来源是裁决——导出会用它。",
                    self.value_draft.field.label(),
                ));
                self.value_draft.value.clear();
                self.load_detail(&site.catalog);
            }
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 撤掉一条**裁决**来源的字段值，让别的源重新说了算。
    pub fn clear_value(
        &mut self,
        site: &mut Site,
        anchor: AnchorKind,
        subject: &str,
        field: Field,
    ) {
        match site.catalog.clear_verdict_value(anchor, subject, field) {
            Ok(true) => {
                self.notice = Some(format!("撤掉了人工写的{}。", field.label()));
                self.load_detail(&site.catalog);
            }
            Ok(false) => self.notice = Some("本来就没人写过。".to_string()),
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 点开一条变体。**界面上点那一行走的就是它**，测试拿它当那一下。
    pub fn pick(&mut self, catalog: &Catalog, key: &str) {
        self.picked = Some(key.to_string());
        self.load_detail(catalog);
    }

    /// 把草稿里那条叫法写进**标题集合**，**来源记作裁决**。
    ///
    /// 界面上「加进集合」那个按钮走的就是它。
    pub fn add_title(&mut self, site: &mut Site, work: &str) {
        let Some(detail) = self.detail.clone() else {
            return;
        };
        if self.write_title(site, work, &detail) {
            self.load_detail(&site.catalog);
        }
    }

    /// 从**标题集合**里删掉一条叫法。界面上那个「删」走的就是它。
    pub fn remove_title(
        &mut self,
        site: &mut Site,
        work: &str,
        language: Language,
        kind: TitleKind,
        source: &str,
        value: &str,
    ) {
        match site
            .catalog
            .remove_title(work, language, kind, source, value)
        {
            Ok(true) => {
                self.notice = Some(format!("删掉了叫法「{value}」。"));
                self.load_detail(&site.catalog);
            }
            Ok(false) => self.notice = Some("那一条已经不在了。".to_string()),
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 把**首选变体**裁给这一个。界面上点那一行走的就是它。
    pub fn set_preferred(&mut self, site: &mut Site, work: &str, platform: &str, key: &str) {
        match site.catalog.set_preferred_variant(work, platform, key) {
            Ok(()) => {
                self.notice = Some(format!("首选变体裁给了 {key}。"));
                self.load_detail(&site.catalog);
            }
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 撤掉**首选变体**裁决，回到规则算的那一个。
    pub fn clear_preferred(&mut self, site: &mut Site, work: &str, platform: &str) {
        match site.catalog.clear_preferred_variant(work, platform) {
            Ok(true) => {
                self.notice = Some("撤掉了首选变体裁决，回到规则算的那一个。".to_string());
                self.load_detail(&site.catalog);
            }
            Ok(false) => self.notice = Some("本来就没人裁过。".to_string()),
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 重新读一遍详情。改过元数据之后要走一趟——面板上摆的必须是库里现在的样子。
    fn load_detail(&mut self, catalog: &Catalog) {
        let Some(key) = self.picked.clone() else {
            self.detail = None;
            return;
        };
        match catalog.variant_detail(&key, &self.priorities, self.pool.as_ref()) {
            Ok(detail) => {
                self.detail = detail;
                self.error = None;
            }
            Err(error) => {
                self.detail = None;
                self.error = Some(format!("中立库读不动：{error}"));
            }
        }
    }

    /// 顶栏上属于这一屏的那一段。
    ///
    /// 先同步一次窗口再画：顶栏与正文各画各的，而顶栏**先画**——不先同步，
    /// 状态栏上那个行数就永远比表格慢一帧（与队列那一屏 `status` 同一条道理）。
    pub fn status(&mut self, ui: &mut egui::Ui, site: &Site) {
        self.sync_window(&site.catalog);
        ui.toggle_value(&mut self.sample, "字体样张");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if let Some(error) = self.window.error() {
                ui.colored_label(ui.visuals().error_fg_color, error);
            } else {
                ui.label(format!(
                    "{} 行；内存里 {} 行、读库 {} 次",
                    thousands(self.window.total()),
                    self.window.retained(),
                    self.window.reads(),
                ));
            }
        });
    }

    /// 换过筛选或排序就把窗口作废重取。没换过是空操作。
    ///
    /// **换了就把「选中第几行」也丢掉**：那是个全序下标，而换一套筛选等于换了一张表，
    /// 同一个下标会指到另一条变体身上，于是高亮的那一行与底下面板摆着的那一条对不上。
    /// 面板认的是**键**（[`Self::picked`]），它照旧留着。
    fn sync_window(&mut self, catalog: &Catalog) {
        if self.window.query() != &self.query {
            self.selected = None;
            self.window.set_query(self.query.clone());
        }
        self.window.sync(catalog);
    }

    /// 画一帧。
    pub fn ui(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        self.sync_window(&site.catalog);
        egui::Panel::bottom("库浏览详情")
            .default_size(300.0)
            .min_size(120.0)
            .show(ui, |ui| self.detail_panel(ui, site));
        egui::Panel::left("筛选")
            .default_size(240.0)
            .min_size(150.0)
            .show(ui, |ui| self.filter_panel(ui));
        egui::CentralPanel::default().show(ui, |ui| {
            if self.sample {
                self.font_sample(ui);
                ui.separator();
            }
            let picked = Table {
                catalog: &site.catalog,
                works: &self.works,
                window: &mut self.window,
                query: &mut self.query,
                selected: &mut self.selected,
                scroll_to: self.scroll_to,
            }
            .show(ui);
            if let Some(row) = picked {
                self.picked = Some(row.key.clone());
                self.load_detail(&site.catalog);
            }
        });
    }

    /// 左边那栏：五个筛选维度。**一个文本框都没有**——值是选的，不是打的。
    fn filter_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt("筛选栏")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("筛选");
                    if ui.button("全清").clicked() {
                        let order = (self.query.order, self.query.descending);
                        self.query = VariantQuery {
                            order: order.0,
                            descending: order.1,
                            ..VariantQuery::default()
                        };
                    }
                });
                ui.weak("各维之间是「且」：一层层收窄。全部下推到中立库。");
                ui.separator();

                platform_picker(ui, &self.facets.platforms, &mut self.query.platform);
                ui.separator();
                facet_picker(
                    ui,
                    "合集",
                    "用户自定义的一组游戏，与平台正交（ADR-0011）。",
                    &self.facets.collections,
                    &mut self.query.collection,
                );
                ui.separator();
                facet_picker(
                    ui,
                    "语言",
                    "**发行版**标着的语言码。汉化版不在这一维里——它是变体，底版多半是日版。",
                    &self.facets.languages,
                    &mut self.query.language,
                );
                ui.separator();
                facet_picker(
                    ui,
                    "中文",
                    "**变体**的中文身份：汉化 / 官中（ADR-0012）。这个库最要紧的那批全在这儿。",
                    &self.facets.chinese,
                    &mut self.query.chinese,
                );
                ui.separator();

                ui.strong("识别状态");
                let mut state = self.query.state;
                if ui.selectable_label(state.is_none(), "不筛").clicked() {
                    state = None;
                }
                for (filter, count) in &self.facets.states {
                    let on = state == Some(*filter);
                    if ui
                        .selectable_label(on, format!("{}  {}", thousands(*count), filter.label()))
                        .clicked()
                    {
                        state = if on { None } else { Some(*filter) };
                    }
                }
                self.query.state = state;
            });
    }

    /// 底下那块面板：这一条到底是什么，以及改它。**全部中文输入都在这里。**
    fn detail_panel(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        ui.add_space(4.0);
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if let Some(notice) = &self.notice {
            ui.colored_label(ui.visuals().warn_fg_color, notice);
        }
        // 筛选框在这块面板里而不在左栏，与队列那一屏同一条规矩：会碰到输入法的控件
        // 全收在**不虚拟化**的面板里（ADR-0005）。筛选本身照旧下推到中立库。
        ui.horizontal(|ui| {
            ui.label("键里含");
            ui.add(
                egui::TextEdit::singleline(&mut self.query.contains)
                    .desired_width(300.0)
                    .hint_text("变体的键里含这段文字"),
            );
        });
        ui.separator();
        if self.detail.is_none() {
            ui.weak("点表里的一行看它的元数据。改也在这块面板里改。");
            return;
        }
        let available = ui.available_width();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2((available * 0.44).max(240.0), ui.available_height()),
                Layout::top_down(Align::Min),
                |ui| self.facts_column(ui),
            );
            ui.separator();
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), ui.available_height()),
                Layout::top_down(Align::Min),
                |ui| self.edit_column(ui, site),
            );
        });
    }

    /// 左半：这一条是什么。识别结论、作品、发行版、合集、成员、**媒体缺哪些**。
    fn facts_column(&mut self, ui: &mut egui::Ui) {
        let Some(detail) = &self.detail else {
            return;
        };
        egui::ScrollArea::vertical()
            .id_salt("变体详情")
            .show(ui, |ui| {
                ui.strong(&detail.row.key);
                ui.label(format!(
                    "平台 {}｜成型规则 {}｜{} 个文件｜{}",
                    detail.row.platform.as_deref().unwrap_or("未知"),
                    detail.row.rule,
                    detail.row.files,
                    capacity(detail.row.bytes, detail.row.unreadable_files),
                ));
                ui.label(match detail.state {
                    Some(state) => format!("识别结论：{}", state.label()),
                    None => "识别结论：还没识别".to_string(),
                });
                if let Some(reason) = &detail.reason {
                    ui.label(format!("为什么没定下来：{reason}"));
                }
                ui.label(format!(
                    "作品：{}",
                    detail.work.as_deref().unwrap_or("还没认出来"),
                ));
                match &detail.release {
                    Some(release) => ui.label(format!(
                        "发行版：地区 {}｜序列号 {}｜语言 {}",
                        release.region.as_deref().unwrap_or("—"),
                        release.serial.as_deref().unwrap_or("—"),
                        if detail.languages.is_empty() {
                            "—".to_string()
                        } else {
                            detail.languages.join(", ")
                        },
                    )),
                    // 没有发行版链接**本身就是一条信息**：同人移植与 homebrew 直接挂在
                    // 作品下，识别管线不拿它们去撞 DAT（`CONTEXT.md` 的「变体」词条）。
                    None => ui.label("发行版：没有链接——同人移植与 homebrew 就是这样"),
                };
                ui.label(format!(
                    "合集：{}",
                    if detail.collections.is_empty() {
                        "—".to_string()
                    } else {
                        detail.collections.join("、")
                    },
                ));

                ui.separator();
                ui.strong("媒体");
                // **一条条列出来**，不只报一个数：人问的往往是「那张封面到底在哪」，
                // 而媒体池按内容哈希存（ADR-0009），只给个数字他连去哪儿找都说不出。
                for item in &detail.media_items {
                    let where_at = match (&item.at, item.in_pool) {
                        (Some(at), Some(true)) => romcat_core::path::display(at),
                        (Some(_), _) => "**池里没有这个文件**".to_string(),
                        _ => "（媒体池没查）".to_string(),
                    };
                    let line = format!(
                        "{} · {}｜{}｜{}",
                        item.kind.label(),
                        item.anchor.label(),
                        item.source,
                        where_at,
                    );
                    if item.in_pool == Some(false) {
                        ui.colored_label(ui.visuals().warn_fg_color, line)
                    } else {
                        ui.label(line)
                    }
                    .on_hover_text(format!(
                        "{}.{}｜依据：{}",
                        item.hash, item.ext, item.evidence
                    ));
                }
                if detail.media_items.is_empty() {
                    ui.weak("一条媒体引用都没有。");
                }
                for have in &detail.media {
                    let line = match have.in_pool {
                        Some(got) => format!(
                            "{}：库里 {} 条引用，池里躺着 {} 份",
                            have.kind.label(),
                            have.refs,
                            got,
                        ),
                        None => format!(
                            "{}：库里 {} 条引用（**媒体池没查**）",
                            have.kind.label(),
                            have.refs,
                        ),
                    };
                    if have.refs > 0 && have.in_pool == Some(0) {
                        ui.colored_label(ui.visuals().warn_fg_color, line);
                    } else {
                        ui.label(line);
                    }
                }
                if self.pool.is_none() {
                    ui.weak("媒体池不在工作目录里，「在不在池子里」这一栏查不了。");
                }
                let missing = detail.missing_media();
                if missing.is_empty() {
                    ui.label("不缺媒体。");
                } else {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!(
                            "缺：{}",
                            missing
                                .iter()
                                .map(|kind| kind.label())
                                .collect::<Vec<_>>()
                                .join("、"),
                        ),
                    );
                }
                if detail.dangling_media() > 0 {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!(
                            "有 {} 条引用在**媒体池**里找不到那个文件，导出时一张都铺不出去。",
                            thousands(detail.dangling_media()),
                        ),
                    );
                }

                ui.separator();
                ui.strong(format!("文件成员（{}）", detail.members.len()));
                for (key, role) in detail.members.iter().take(TOP_MEMBERS) {
                    ui.label(format!("{}  {key}", role.code()));
                }
                if detail.members.len() > TOP_MEMBERS {
                    ui.weak(format!(
                        "……另有 {} 个没列",
                        detail.members.len() - TOP_MEMBERS
                    ));
                }
            });
    }

    /// 右半：**改**。标题集合与首选变体。这一栏里的每一个文本框都会碰到输入法。
    fn edit_column(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        let Some(detail) = self.detail.clone() else {
            return;
        };
        let mut dirty = false;
        egui::ScrollArea::vertical()
            .id_salt("元数据编辑")
            .show(ui, |ui| {
                dirty |= self.titles_ui(ui, site, &detail);
                ui.separator();
                dirty |= self.preferred_ui(ui, site, &detail);
                ui.separator();
                dirty |= self.values_ui(ui, site, &detail);
            });
        if dirty {
            self.load_detail(&site.catalog);
        }
    }

    /// **标题集合**：全部叫法，加一条、删一条。
    fn titles_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, detail: &VariantDetail) -> bool {
        ui.strong("标题集合");
        let Some(work) = detail.work.clone() else {
            ui.weak("这个变体还没认出属于哪个作品，标题集合无从谈起。");
            return false;
        };
        if let Some(chosen) = &detail.display {
            ui.label(format!(
                "显示标题：{}（{}）｜排序标题：{}（来自{}）",
                chosen.display,
                chosen.language.label(),
                chosen.sort,
                chosen.sort_from.label(),
            ));
        }
        let mut dirty = false;
        let mut remove: Option<(Language, TitleKind, String, String)> = None;
        for row in detail.titles.iter().take(TOP_TITLES) {
            ui.horizontal(|ui| {
                if ui
                    .small_button("删")
                    .on_hover_text("从标题集合里去掉这一条叫法")
                    .clicked()
                {
                    remove = Some((
                        row.language,
                        row.kind,
                        row.source.clone(),
                        row.value.clone(),
                    ));
                }
                let line = format!(
                    "{}｜{} {}｜{}｜{} 个变体这么叫",
                    row.value,
                    row.language.label(),
                    row.kind.label(),
                    row.source,
                    row.seen,
                );
                if row.is_verdict() {
                    ui.strong(line).on_hover_text(&row.evidence);
                } else {
                    ui.label(line).on_hover_text(&row.evidence);
                }
            });
        }
        if detail.titles.len() > TOP_TITLES {
            ui.weak(format!(
                "……另有 {} 条没列",
                detail.titles.len() - TOP_TITLES
            ));
        }
        if detail.titles.is_empty() {
            ui.weak("一条叫法都没有——显示标题会退回作品名。");
        }
        if let Some((language, kind, source, value)) = remove {
            self.remove_title(site, &work, language, kind, &source, &value);
            dirty = true;
        }

        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.title_draft.value)
                    .desired_width(220.0)
                    .hint_text("加一条叫法"),
            );
            egui::ComboBox::from_id_salt("叫法语言")
                .selected_text(self.title_draft.language.label())
                .show_ui(ui, |ui| {
                    for language in Language::all() {
                        ui.selectable_value(
                            &mut self.title_draft.language,
                            language,
                            language.label(),
                        );
                    }
                });
            egui::ComboBox::from_id_salt("叫法类型")
                .selected_text(self.title_draft.kind.label())
                .show_ui(ui, |ui| {
                    for kind in TitleKind::all() {
                        ui.selectable_value(&mut self.title_draft.kind, kind, kind.label());
                    }
                });
            let ready = !self.title_draft.value.trim().is_empty();
            if ui
                .add_enabled(ready, egui::Button::new("加进集合"))
                .on_hover_text("来源记作「裁决」：人改过的东西不许被任何数据源覆盖")
                .clicked()
            {
                dirty |= self.write_title(site, &work, detail);
            }
        });
        dirty
    }

    /// 把草稿里那条叫法写进标题集合，**来源记作裁决**。写成了就返回 `true`。
    fn write_title(&mut self, site: &mut Site, work: &str, detail: &VariantDetail) -> bool {
        let value = self.title_draft.value.trim().to_string();
        if value.is_empty() {
            return false;
        }
        let row = romcat_core::catalog::TitleRow {
            work: work.to_string(),
            value: value.clone(),
            language: self.title_draft.language,
            kind: self.title_draft.kind,
            // **裁决**：重折标题集合时一行都不碰（`Catalog::clear_titles`）。
            source: VERDICT.to_string(),
            region: detail
                .release
                .as_ref()
                .and_then(|release| release.region.clone()),
            variant_key: Some(detail.row.key.clone()),
            confidence: romcat_core::catalog::Confidence::High,
            seam: None,
            evidence: HAND_WRITTEN.to_string(),
            seen: 1,
        };
        match site.catalog.put_titles(&[row]) {
            Ok(()) => {
                self.notice = Some(format!("「{value}」进了标题集合，来源是裁决。"));
                self.title_draft.value.clear();
                true
            }
            Err(error) => {
                self.error = Some(format!("中立库写不动：{error}"));
                false
            }
        }
    }

    /// **刮削来的字段值**：看得见，也改得动。
    ///
    /// 「所有元数据编辑收敛在这里完成」（ADR-0001 的修订段）说的不只是标题与首选变体
    /// ——年份、发行商、开发商、类型、简介、汉化组这几样也会写进导出条目
    /// （`adapter::converge`），主库的元数据文件既然不再是编辑入口，它们就得在这儿改。
    fn values_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, detail: &VariantDetail) -> bool {
        ui.strong("刮削来的元数据");
        ui.weak("同一个字段可以有好几条，三元组并存不互相覆盖；**裁决**排在最前，导出用它。");
        let mut dirty = false;
        let mut clear: Option<(AnchorKind, Field)> = None;
        for item in &detail.values {
            ui.horizontal(|ui| {
                if item.is_verdict() {
                    if ui
                        .small_button("撤")
                        .on_hover_text("撤掉这条人工写的，让别的源重新说了算")
                        .clicked()
                        && let Some(field) = Field::all()
                            .into_iter()
                            .find(|field| field.label() == item.value.field)
                    {
                        clear = Some((item.anchor, field));
                    }
                } else {
                    ui.add_space(24.0);
                }
                // **一行画得下的那一截**：简介能有 4,000 字，横排里不折行（见 `one_line`）。
                let short = one_line(&item.value.value);
                let line = format!(
                    "{} · {}｜{}｜{}",
                    item.value.field,
                    item.anchor.label(),
                    item.value.source,
                    short.as_deref().unwrap_or(&item.value.value),
                );
                let response = if item.is_verdict() {
                    ui.strong(line)
                } else {
                    ui.label(line)
                };
                if short.is_some() {
                    // 收窄过的那些，整段挂在悬停里——**面板上画不下不等于看不到**。
                    // 用 `on_hover_ui` 而不是拼一个大字符串：那个闭包只在真悬停时才跑。
                    response.on_hover_ui(|ui| {
                        // **限宽**。不限的话悬停框跟着最长那一行铺开——简介闸在 4,000 字
                        // （票 `offline-chinese-fields/03`），一段没有换行的中文会把这个
                        // 框拉成一条横穿屏幕的线，反倒比截断更看不清。
                        ui.set_max_width(420.0);
                        ui.label(&item.value.value);
                        ui.separator();
                        ui.label(&item.value.evidence);
                    });
                } else {
                    response.on_hover_text(&item.value.evidence);
                }
            });
        }
        if detail.values.is_empty() {
            ui.weak("一条刮削结论都没有。跑一次 `romcat scrape`，或者在这儿手写。");
        }
        // 作品未知时只挂得到变体那一层——**作品锚点是作品名**，没有名字就没有锚点。
        let anchors: Vec<AnchorKind> = if detail.work.is_some() {
            vec![AnchorKind::Work, AnchorKind::Variant]
        } else {
            vec![AnchorKind::Variant]
        };
        if !anchors.contains(&self.value_draft.anchor) {
            self.value_draft.anchor = AnchorKind::Variant;
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("字段")
                .selected_text(self.value_draft.field.label())
                .show_ui(ui, |ui| {
                    for field in Field::all() {
                        ui.selectable_value(&mut self.value_draft.field, field, field.label());
                    }
                });
            egui::ComboBox::from_id_salt("挂在哪一层")
                .selected_text(self.value_draft.anchor.label())
                .show_ui(ui, |ui| {
                    for anchor in &anchors {
                        ui.selectable_value(&mut self.value_draft.anchor, *anchor, anchor.label());
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut self.value_draft.value)
                    .desired_width(220.0)
                    .hint_text("写下这个字段的值"),
            );
            let subject = match self.value_draft.anchor {
                AnchorKind::Work => detail.work.clone(),
                AnchorKind::Variant => Some(detail.row.key.clone()),
            };
            let ready = !self.value_draft.value.trim().is_empty() && subject.is_some();
            if ui
                .add_enabled(ready, egui::Button::new("写下"))
                .on_hover_text("来源记作「裁决」：它排在每个字段的最前，导出真会用它")
                .clicked()
                && let Some(subject) = subject
            {
                self.put_value(site, &subject);
                dirty = true;
            }
        });
        if let Some((anchor, field)) = clear {
            let subject = match anchor {
                AnchorKind::Work => detail.work.clone(),
                AnchorKind::Variant => Some(detail.row.key.clone()),
            };
            if let Some(subject) = subject {
                self.clear_value(site, anchor, &subject, field);
                dirty = true;
            }
        }
        dirty
    }

    /// **首选变体**：这个作品在这个平台上默认启动哪一个。
    fn preferred_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, detail: &VariantDetail) -> bool {
        ui.strong("首选变体");
        let (Some(work), Some(platform)) = (detail.work.clone(), detail.row.platform.clone())
        else {
            ui.weak("作品或平台还没定下来，首选变体无从谈起。");
            return false;
        };
        // ADR-0012 那条**必须写在人眼前**：改首选不会改中文标题的来源。
        match detail.chinese_title() {
            Some(row) => ui.label(format!(
                "中文标题取的是「{}」（{}｜{}），**与首选变体无关**——\
                 首选启动汉化版，中文名照旧取官中版的官方译名。",
                row.value,
                row.kind.label(),
                row.source,
            )),
            None => ui.label(
                "这个作品还没有中文叫法。首选变体改成汉化版也不会凭空生出一个中文名——\
                 那两件事是分开的（ADR-0012）。",
            ),
        };
        let mut dirty = false;
        let mut clear = false;
        let mut set: Option<String> = None;
        for sibling in &detail.siblings {
            let first = detail.preferred_now() == Some(sibling.row.key.as_str());
            let label = format!(
                "{}{}  {}｜{}",
                if first { "▶ " } else { "   " },
                sibling.preference.label(),
                sibling.row.key,
                human_bytes(sibling.row.bytes),
            );
            if ui
                .selectable_label(first, label)
                .on_hover_text("点它就把首选变体裁给这一个")
                .clicked()
                && !first
            {
                set = Some(sibling.row.key.clone());
            }
        }
        if detail.siblings.len() <= 1 {
            ui.weak("这个平台上这部作品只有这一个变体，没得选。");
        }
        ui.horizontal(|ui| {
            match &detail.preferred {
                Some(key) => ui.label(format!("眼下是**裁决**指定的：{key}")),
                None => ui.label("眼下没人裁过，按规则算：汉化 > 官中 > 日版 > 其他"),
            };
            if ui
                .add_enabled(detail.preferred.is_some(), egui::Button::new("撤掉裁决"))
                .clicked()
            {
                clear = true;
            }
        });
        if clear {
            self.clear_preferred(site, &work, &platform);
            dirty = true;
        }
        if let Some(key) = set {
            self.set_preferred(site, &work, &platform, &key);
            dirty = true;
        }
        dirty
    }

    /// 字体样张：把 egui 内置字体缺的那几类字**摆出来给人看**。
    fn font_sample(&self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.strong("字体样张");
            ui.label(format!("子集 {}", human_bytes(font::subset_bytes() as u64)));
        });
        for (what, text) in font::SAMPLE.iter().copied() {
            ui.horizontal(|ui| {
                ui.add_sized([90.0, 18.0], egui::Label::new(what));
                ui.label(text);
            });
        }
        // OFL 要求分发字体时随附许可，而这份字体是嵌在可执行文件里的——许可得跟着走。
        ui.collapsing("字体许可（SIL Open Font License 1.1）", |ui| {
            egui::ScrollArea::vertical()
                .max_height(160.0)
                .show(ui, |ui| ui.monospace(font::LICENSE));
        });
    }
}

/// 一个维度的选项列表：一行一个值，左边是它选中多少个变体。
///
/// 空列表也画出标题与一句话，而不是整块消失——「这个维度一条数据都没有」与
/// 「这个维度不在界面上」是两件事，后者会让人以为工具不支持按它筛。
fn facet_picker(
    ui: &mut egui::Ui,
    what: &str,
    hint: &str,
    facets: &[romcat_core::catalog::Facet],
    picked: &mut Option<String>,
) {
    ui.strong(what).on_hover_text(hint);
    if facets.is_empty() {
        ui.weak(format!("库里还没有{what}这一维的数据。"));
        return;
    }
    if ui.selectable_label(picked.is_none(), "不筛").clicked() {
        *picked = None;
    }
    for facet in facets {
        let on = picked.as_deref() == Some(facet.value.as_str());
        if ui
            .selectable_label(on, format!("{}  {}", thousands(facet.count), facet.value))
            .clicked()
        {
            *picked = (!on).then(|| facet.value.clone());
        }
    }
}

/// 平台那一维。它比别的多一档——**平台未知**在表里是 `NULL`，只能是独立的一支
/// （[`PlatformFilter`]）。
fn platform_picker(
    ui: &mut egui::Ui,
    facets: &[romcat_core::catalog::Facet],
    picked: &mut Option<PlatformFilter>,
) {
    ui.strong("平台")
        .on_hover_text("变体所属的硬件系统。认不出平台的内容照常入库（ADR-0011）。");
    if facets.is_empty() {
        ui.weak("库里还没有平台这一维的数据。");
        return;
    }
    if ui.selectable_label(picked.is_none(), "不筛").clicked() {
        *picked = None;
    }
    for facet in facets {
        let filter = PlatformFilter::from_label(&facet.value);
        let on = picked.as_ref() == Some(&filter);
        if ui
            .selectable_label(on, format!("{}  {}", thousands(facet.count), facet.value))
            .clicked()
        {
            *picked = (!on).then_some(filter);
        }
    }
}
