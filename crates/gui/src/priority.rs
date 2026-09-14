//! **数据源优先级**：同一个字段有好几个源给了值时，显示哪一个（票 `gui-looks-like-the-design/30`）。
//!
//! 画成一层弹层（[`crate::dialog`]）。眼下从[刮削面板](crate::scrape)那句「不该靠重采」旁边
//! 打开——那句话本来就在说「想换一个显示的值，调这张表」；设计稿上的正式入口在设置屏
//! （票 `gui-looks-like-the-design/31`）。
//!
//! ## 这一层只摆、只转发（ADR-0005）
//!
//! | 屏上那件事 | 核心库里是谁 |
//! |---|---|
//! | 左边列哪几个字段、什么次序 | [`priority::listed_fields`] |
//! | 谁固定、挪不动 | [`priority::is_pinned`] |
//! | 上移、下移按不按得动，按下去挪不挪 | [`Priorities::can_raise`] / [`Priorities::raise`] 那一对 |
//! | 「已调整」 | [`Priorities::differs_in`]，对着内置那一份 |
//! | 保存后哪几处显示值会变 | [`priority::shifts`] |
//! | 保存 | [`Priorities::save`] 写到 [`workspace::priorities_path`]——读的那一侧拿的是同一个路径 |
//!
//! **这一层里一行「按优先级挑值」的代码都没有**：屏上「由谁的什么改为谁的什么」那几句，
//! 是核心库交出来的。
//!
//! ## 保存只写那一份文件
//!
//! 刮削结果按「锚点 × 字段 × 源」三元组并存，换一份表只是换一次排序：**不排任何刮削任务、
//! 不重采**。读这份表的那几条路（刮削、整理标题、导出、同步）每一趟开头读一次，下一趟就是
//! 新的；浏览屏手上缓着的那一份由主窗口换掉（[`Editor::take_saved`]）。

use std::path::PathBuf;

use romcat_core::catalog::Catalog;
use romcat_core::report::thousands;
use romcat_core::scrape::priority::{self, FieldShifts, Said, VERDICT};
use romcat_core::scrape::{Field, Priorities};
use romcat_core::workspace;

use crate::dialog::{Button, Dialog, Footer, Width};
use crate::font;

/// 刮削面板上打开这一层的那颗按钮。
pub const OPEN: &str = "调整优先级…";

/// 标题底下那句说明。
pub const NOTE: &str = "同一个字段可以同时存着好几个数据源的值，这里决定显示哪一个。\
     保存到工作目录的 priorities.toml，立即生效，不需要重新刮削。";

/// 每一栏顶上那句：顺序怎么读。
pub const HOW_TO_READ: &str = "从上往下，第一个有值的数据源说了算。";

/// 没列出的数据源怎么排：**标题之外的字段**（`Priorities::pick` 的第二层排序键）。
pub const UNLISTED: &str = "没有列出的数据源排在最后，彼此按采集时刻先后（新的优先）。";

/// 没列出的数据源怎么排：**标题那一栏**。作品的显示标题由标题集合挑，名次之后比的是有几个
/// 变体这么叫（`title::choose` 的第六层），不是采集时刻。
pub const TITLE_UNLISTED: &str = "没有列出的数据源排在最后。作品的显示标题里，它们之间按有几个变体这么叫排（多的优先）；\
     没认出作品的变体，按采集时刻先后（新的优先）。";

/// 标题那一栏另说一句：这张表在显示标题里管到哪儿。
///
/// **不说「整理标题之后才反映」**：显示标题不落库，导出与详情面板每次照手上这份表现挑
/// （`title::choose`），换了表下一次就是新的。
pub const TITLE_REACH: &str = "作品的显示标题由标题集合按语言、类型与置信度挑，这里的顺序只在这几样都一样时才分先后；\
     没认出作品的变体，标题直接照这里的顺序挑。";

/// 保存不排活那一句。
pub const NO_RESCRAPE: &str = "保存只换排序：不排任何刮削任务，不重采。";

/// 保存成了之后刮削面板上那句回话。
pub const SAVED: &str = "已保存数据源优先级：立即生效，不需要重新刮削。";

/// 按平台单独那一栏的说明。
#[must_use]
pub fn override_note(platform: &str) -> String {
    format!(
        "{platform}上这一栏整条替换通用顺序，不是插进去。它只管没认出作品的变体——\
         认出了作品的，显示标题由标题集合挑，眼下不看平台。"
    )
}

/// 固定那几家各自旁边写的字，与指针停上去时说的理由。
///
/// 字短、理由长：顺序那一栏要给右边的「保存后的变化」留出地方。
#[must_use]
pub fn pinned_note(source: &str) -> (&'static str, &'static str) {
    if source == VERDICT {
        (
            "固定在第一位",
            "你亲手定过的值（裁决）：人改过的东西不许被任何数据源覆盖。",
        )
    } else {
        (
            "手工维护，固定在数据源之前",
            "导入的手工维护的元数据：维护者自己养的那一份，排到任何数据源后面，\
             一次刮削就把它盖掉了。",
        )
    }
}

/// 算过的「保存后的变化」，连它是照哪一份草稿算的。
struct Counted {
    /// 照这一份草稿算的。草稿一变就作废。
    draft: Priorities,
    /// 算出来的，或者算不出来那句话。
    result: Result<Vec<FieldShifts>, String>,
}

/// 数据源优先级那一层弹层。
pub struct Editor {
    /// 工作目录：那份 `priorities.toml` 在它里头。
    workspace: PathBuf,
    /// 开着没有。
    open: bool,
    /// 内置那一份：「已调整」与「恢复默认」对着它。
    builtin: Priorities,
    /// 眼下生效的那一份：打开时从工作目录读的，没有就是内置的。「保存后的变化」对着它比。
    current: Priorities,
    /// 工作目录里那份读不动。
    unreadable: bool,
    /// 改到一半的那一份。
    draft: Priorities,
    /// 正在看哪个字段。
    field: String,
    /// 正在看哪个平台的单独顺序；`None` 是通用那条。
    platform: Option<String>,
    /// 算过的「保存后的变化」。
    counted: Option<Counted>,
    /// 读或写出的错。
    error: Option<String>,
    /// 保存成了、还没被取走的那一份。
    saved: Option<Priorities>,
}

impl Editor {
    /// 开一块，关着。
    #[must_use]
    pub fn new(workspace: PathBuf) -> Self {
        let builtin = Priorities::builtin();
        Self {
            workspace,
            open: false,
            current: builtin.clone(),
            draft: builtin.clone(),
            builtin,
            unreadable: false,
            field: Field::Title.label().to_string(),
            platform: None,
            counted: None,
            error: None,
            saved: None,
        }
    }

    /// **打开**：读工作目录里生效的那一份（没有就是内置的），从标题那一栏看起。
    ///
    /// 那份读不动时照样打开，摆的是内置那一份，读不动那句话写在屏上——按「保存」会把
    /// 坏的那份换掉。不打开的话，人在界面上没有任何办法把它修好（挂单 `Q787`）。
    pub fn open(&mut self) {
        match romcat_core::sync::prepare::priorities(None, &self.workspace) {
            Ok(current) => {
                self.current = current;
                self.unreadable = false;
                self.error = None;
            }
            Err(error) => {
                self.current = self.builtin.clone();
                self.unreadable = true;
                self.error = Some(format!(
                    "{error}。下面摆的是内置那一份；按「保存」会把工作目录里那份换掉。"
                ));
            }
        }
        self.draft = self.current.clone();
        self.field = Field::Title.label().to_string();
        self.platform = None;
        self.counted = None;
        self.open = true;
    }

    /// 开着没有。
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// 关上。改到一半的那一份丢掉——下次打开从工作目录重读。
    pub fn close(&mut self) {
        self.open = false;
    }

    /// 改到一半的那一份。
    #[must_use]
    pub fn draft(&self) -> &Priorities {
        &self.draft
    }

    /// 正在看哪个字段。
    #[must_use]
    pub fn field(&self) -> &str {
        &self.field
    }

    /// 正在看哪个平台的单独顺序；`None` 是通用那条。
    #[must_use]
    pub fn platform(&self) -> Option<&str> {
        self.platform.as_deref()
    }

    /// 读或写出的错。
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// 左边那一列：[`priority::listed_fields`]。
    #[must_use]
    pub fn fields(&self) -> Vec<String> {
        priority::listed_fields(&[&self.draft])
    }

    /// 正在看的这个字段在哪几个平台上有单独的顺序。
    #[must_use]
    pub fn platforms(&self) -> Vec<String> {
        self.draft
            .platform_overrides()
            .into_iter()
            .filter(|(_, field, _)| *field == self.field)
            .map(|(platform, _, _)| platform.to_string())
            .collect()
    }

    /// 换去看另一个字段，从通用那条看起。
    pub fn select_field(&mut self, field: &str) {
        self.field = field.to_string();
        self.platform = None;
    }

    /// 换去看某个平台的单独顺序（`None` 是通用那条）。这个字段在那个平台上没有单独的顺序时
    /// 退回通用那条——**不替人新开一条覆盖**。
    pub fn select_platform(&mut self, platform: Option<&str>) {
        self.platform = platform
            .filter(|platform| self.platforms().iter().any(|known| known == platform))
            .map(ToString::to_string);
    }

    /// 把正在看的这条链上第 `at` 家往前挪一位。挪不挪得动由核心库说了算。
    pub fn raise(&mut self, at: usize) {
        self.draft.raise(&self.field, self.platform.as_deref(), at);
    }

    /// 往后挪一位。
    pub fn lower(&mut self, at: usize) {
        self.draft.lower(&self.field, self.platform.as_deref(), at);
    }

    /// 这个字段（连同它的按平台覆盖）与内置那一份排得不一样。
    #[must_use]
    pub fn adjusted(&self, field: &str) -> bool {
        self.draft.differs_in(&self.builtin, field)
    }

    /// **恢复默认**：回到内置那一份。按「保存」之前工作目录里什么都不变。
    pub fn reset(&mut self) {
        self.draft = self.builtin.clone();
    }

    /// 按得下「保存」没有：改过了，或者工作目录里那份读不动、要换掉。
    #[must_use]
    pub fn can_save(&self) -> bool {
        self.draft != self.current || self.unreadable
    }

    /// **保存**：写到工作目录的 `priorities.toml`，关上这一层，留一份等主窗口取走。
    ///
    /// 只写那一份文件：不排任何刮削任务、不重采。
    pub fn save(&mut self) {
        let path = workspace::priorities_path(&self.workspace);
        match self.draft.save(&path) {
            Ok(()) => {
                self.current = self.draft.clone();
                self.unreadable = false;
                self.error = None;
                self.saved = Some(self.draft.clone());
                self.open = false;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// 保存成了、还没被取走的那一份在不在。
    #[must_use]
    pub fn has_saved(&self) -> bool {
        self.saved.is_some()
    }

    /// 取走保存成了的那一份：主窗口拿它换掉浏览屏手上缓着的那一份。
    pub fn take_saved(&mut self) -> Option<Priorities> {
        self.saved.take()
    }

    /// 「保存后的变化」：照眼下这份草稿算。**草稿变了才重算**——算一遍要把库走一遍。
    fn recount(&mut self, catalog: &Catalog) {
        if self.draft == self.current {
            self.counted = None;
            return;
        }
        if self
            .counted
            .as_ref()
            .is_some_and(|counted| counted.draft == self.draft)
        {
            return;
        }
        let result = priority::shifts(catalog, &self.current, &self.draft)
            .map_err(|error| format!("中立库读不动：{error}"));
        self.counted = Some(Counted {
            draft: self.draft.clone(),
            result,
        });
    }

    /// 画一帧。关着就什么都不画。
    pub fn show(&mut self, ctx: &egui::Context, catalog: &Catalog) {
        /// 页脚上按下去的是哪一颗。
        enum Pressed {
            /// 「取消」：关上，改到一半的丢掉。
            Cancel,
            /// 「恢复默认」。
            Reset,
            /// 「保存」。
            Save,
        }
        if !self.open {
            return;
        }
        self.recount(catalog);
        let footer = Footer::new(Button::new("取消", Pressed::Cancel))
            .button(
                Button::new("恢复默认", Pressed::Reset)
                    .enabled(self.draft != self.builtin)
                    .hover("回到内置那一份。按「保存」之前工作目录里什么都不变。"),
            )
            .button(
                Button::new("保存", Pressed::Save)
                    .primary()
                    .enabled(self.can_save())
                    .hover("写到工作目录的 priorities.toml，立即生效。不排任何刮削任务，不重采。"),
            );
        let shown = Dialog::new("数据源优先级", "数据源优先级", footer)
            .note(NOTE)
            .width(Width::Widest)
            .show(ctx, |ui| self.body_ui(ui));
        match shown.pressed {
            None => {}
            Some(Pressed::Cancel) => self.close(),
            Some(Pressed::Reset) => self.reset(),
            Some(Pressed::Save) => self.save(),
        }
    }

    /// 内容区：字段、顺序、说明与保存后的变化，**三栏并排**。
    ///
    /// 不摞成上下两块：标题那一栏有十二家，「保存后的变化」摞在顺序底下的话要滚到内容区
    /// 外头去——而那正是按「保存」之前该看见的东西。
    fn body_ui(&mut self, ui: &mut egui::Ui) {
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| self.fields_ui(ui));
            ui.separator();
            ui.vertical(|ui| self.order_ui(ui));
            ui.separator();
            ui.vertical(|ui| self.notes_ui(ui));
        });
    }

    /// 左边那一列：字段，改过的标「已调整」。
    fn fields_ui(&mut self, ui: &mut egui::Ui) {
        for field in self.fields() {
            let text = if self.adjusted(&field) {
                format!("{field} · 已调整")
            } else {
                field.clone()
            };
            if ui.selectable_label(self.field == field, text).clicked() {
                self.select_field(&field);
            }
        }
    }

    /// 中间那一栏：这个字段的顺序。
    fn order_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(font::strong(self.field.clone()));
            ui.weak(HOW_TO_READ);
        });
        let platforms = self.platforms();
        if !platforms.is_empty() {
            let mut chosen = self.platform.clone();
            let label = |platform: &Option<String>| match platform {
                Some(platform) => format!("{platform}（单独的顺序）"),
                None => "所有平台".to_string(),
            };
            egui::ComboBox::from_id_salt("优先级适用平台")
                .selected_text(label(&chosen))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut chosen, None, label(&None));
                    for platform in &platforms {
                        let option = Some(platform.clone());
                        let text = label(&option);
                        ui.selectable_value(&mut chosen, option, text);
                    }
                });
            if chosen != self.platform {
                self.select_platform(chosen.as_deref());
            }
        }

        let platform = self.platform.as_deref();
        let order = self.draft.order(&self.field, platform).to_vec();
        // 按不按得动问核心库：挪的那一下（`raise` / `lower`）问的是同一处。
        let movable: Vec<(bool, bool)> = (0..order.len())
            .map(|at| {
                (
                    self.draft.can_raise(&self.field, platform, at),
                    self.draft.can_lower(&self.field, platform, at),
                )
            })
            .collect();
        let (mut raise, mut lower) = (None, None);
        egui::Grid::new("优先级顺序")
            .num_columns(3)
            .striped(true)
            .show(ui, |ui| {
                for (at, (source, (up, down))) in order.iter().zip(&movable).enumerate() {
                    ui.label((at + 1).to_string());
                    ui.label(font::strong(source.clone()));
                    if priority::is_pinned(source) {
                        let (note, why) = pinned_note(source);
                        ui.weak(note).on_hover_text(why);
                    } else {
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(*up, egui::Button::new("↑"))
                                .on_hover_text(format!("上移 {source}"))
                                .clicked()
                            {
                                raise = Some(at);
                            }
                            if ui
                                .add_enabled(*down, egui::Button::new("↓"))
                                .on_hover_text(format!("下移 {source}"))
                                .clicked()
                            {
                                lower = Some(at);
                            }
                        });
                    }
                    ui.end_row();
                }
            });
        if let Some(at) = raise {
            self.raise(at);
        }
        if let Some(at) = lower {
            self.lower(at);
        }
    }

    /// 右边那一栏：这一栏的顺序管到哪儿，与保存后哪几处显示值会变。
    fn notes_ui(&self, ui: &mut egui::Ui) {
        if self.field == Field::Title.label() {
            ui.weak(TITLE_UNLISTED);
            ui.weak(TITLE_REACH);
        } else {
            ui.weak(UNLISTED);
        }
        let platforms = self.platforms();
        match &self.platform {
            Some(platform) => {
                ui.weak(override_note(platform));
            }
            None if !platforms.is_empty() => {
                ui.weak(format!(
                    "{}另有单独的顺序：在左边那个选择里切过去单独编辑。",
                    platforms.join("、")
                ));
            }
            None => {}
        }
        ui.separator();
        ui.label(font::strong("保存后的变化"));
        match &self.counted {
            _ if self.draft == self.current => {
                ui.weak("还没改动。");
            }
            None => {}
            Some(Counted {
                result: Err(error), ..
            }) => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("算不出会变什么：{error}"),
                );
            }
            Some(Counted {
                result: Ok(fields), ..
            }) => {
                for one in fields {
                    ui.label(shift_line(one));
                }
            }
        }
        ui.weak(NO_RESCRAPE);
    }
}

/// 「保存后的变化」里一个字段那一行。
#[must_use]
pub fn shift_line(shifts: &FieldShifts) -> String {
    let Some(example) = &shifts.example else {
        return format!(
            "{}：这份库里没有哪处显示值会变（这些地方只有一个数据源给了值，或者几家说的一样）。",
            shifts.field
        );
    };
    let mut counts = Vec::new();
    if shifts.works > 0 {
        counts.push(format!("{} 个作品", thousands(shifts.works)));
    }
    if shifts.variants > 0 {
        counts.push(format!("{} 个变体", thousands(shifts.variants)));
    }
    format!(
        "{}：{}的显示值会变，例如{}「{}」由 {}改为 {}。",
        shifts.field,
        counts.join("、"),
        example.anchor.label(),
        example.subject,
        said(&example.before),
        said(&example.after),
    )
}

/// 「谁的什么」。
fn said(said: &Said) -> String {
    format!(
        "{} 的「{}」",
        said.source.as_deref().unwrap_or("作品名"),
        clip(&said.values.join("、"))
    )
}

/// 一句话里摆得下的那一截：简介动辄几百字，照原样摆会把这一行撑成一整屏。
fn clip(text: &str) -> String {
    /// 最多摆几个字。
    const CLIP: usize = 24;
    let flat = text.replace('\n', " ");
    if flat.chars().count() <= CLIP {
        flat
    } else {
        format!("{}…", flat.chars().take(CLIP).collect::<String>())
    }
}
