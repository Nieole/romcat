//! **待确认队列**：工具的主界面（ADR-0002）。
//!
//! ## 这个屏幕要回答的三个问题
//!
//! 1. **哪些等着裁决**——中间那张表，虚拟化，一万六千条滚起来的代价与总条数无关。
//! 2. **从哪一批下手**——左边那三张分组表。每一行就是一次批量裁决能覆盖多少条，
//!    点一下就把它变成选择器。ADR-0002 的原话：**只能逐条点的队列在几千条规模下
//!    等于没有**，所以「一次盖住几百条」不是快捷方式，是这件事成不成立的分界。
//! 3. **这一条凭什么这么定**——底下那块面板：全部**候选**、各自的**置信度**与**依据**，
//!    以及这一条的裁决会钉在**内容**上还是只钉得住**路径**。
//!
//! ## 中文输入全在底下那块面板里
//!
//! 一个 [`egui::TextEdit`] 都不进表格单元格。表格是虚拟化的，正在组字的那一行一旦滚出
//! 视口，那个控件就不存在了，输入法上屏时会没人接（ADR-0005 的修订段）。
//! 连**搜索框**也在那块面板里而不在顶栏：Windows 上的输入法还没实机验过（票 23），
//! 把会碰到输入法的控件全收进同一块**不虚拟化**的面板，是眼下能把风险面缩到最小的做法。
//! `tests/queue.rs::表格里画多少行文本输入框都是那几个` 钉住这一条。
//!
//! ## 领域判断一条都不在这里
//!
//! 队列怎么筛、按什么分组、一次裁决说得成不成立、落下之后哪些该从队列里消失——
//! 全在 [`romcat_core::triage`]（[`Queue`]、[`Axis`]、[`Draft`]）。这一层只做三件事：
//! 把要来的画出来、把点的那一下写回去、把中文输入放在对的位置上。ADR-0005 说得清楚，
//! 若 Windows 真机的输入法验证没过，换掉的只是这个 crate。

use std::fmt::Write as _;

use egui::{Align, Layout};
use egui_extras::{Column, TableBuilder};
use romcat_core::catalog::State;
use romcat_core::dat::chinese::ChineseMark;
use romcat_core::report::{capacity, thousands};
use romcat_core::triage::{Applied, Axis, Draft, Filter, Overrides, Plan, Queue};
use romcat_core::verdict;

use crate::table::ROW_HEIGHT;
use romcat_core::site::Site;

/// 分组表一个轴最多列几行。再多就不是给人看的了（与命令行报告同一个数）。
const TOP: usize = 12;

/// 「主库根那一层」在**按目录**那个框里写成什么。
///
/// 空框的意思是「这个轴不筛」，而主库根那一组的标签正好**是空串**——两件事必须分得开，
/// 不然那一行既永远显示为选中、又永远点不动。`/` 折进选择器时会被 [`Filter::under`]
/// 剥掉，落到核心库那边就是空前缀，也就是主库根。
const ROOT: &str = "/";

/// 待确认队列这个屏幕。
pub struct Screen {
    queue: Queue,
    /// 选中的是哪一条。**记键不记下标**：换个选择器表就重排了，下标会指到别人身上。
    picked: Option<String>,
    /// 上一次找到它的下标。对得上就直接用，省得每帧扫一遍一万六千条。
    picked_at: usize,
    /// 界面上那份选择器草稿。
    picks: Picks,
    /// 界面上那份裁决草稿。
    form: Form,
    /// 排出来还没落下的计划。**先出计划再动手**（与同步那一侧的差量预览同源）。
    pending: Option<Plan>,
    /// 上一次落下的账。
    applied: Option<Applied>,
    /// 上一次出的错。
    error: Option<String>,
    /// **只裁选中的那一条**。
    ///
    /// 批量是这件事成不成立的分界（ADR-0002），但「采用第 N 条候选」天生是逐条的动作
    /// ——同一批里各人的候选不是同一部游戏。勾上它，选择器就多一条「点名这个变体」
    /// （[`Filter::keys`]），队列、分组表、计划书全都跟着只剩这一条。
    only_picked: bool,
    /// 把表格的滚动位置强按到这个像素偏移。**只有量帧率时才设**（[`crate::bench`]），
    /// 真界面上永远是 `None`。
    pub scroll_to: Option<f32>,
}

impl Screen {
    /// 开一个空屏幕：还没列过队列。
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: Queue::empty(),
            picked: None,
            picked_at: 0,
            picks: Picks::default(),
            form: Form::default(),
            pending: None,
            applied: None,
            error: None,
            only_picked: false,
            scroll_to: None,
        }
    }

    /// 队列本身，供测试查「列出多少条、选中多少条」。
    #[must_use]
    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    /// 上一次出的错。
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// 列一次队列。**一个字节都不读主库**——原料全在中立库与沉淀库里（ADR-0001）。
    pub fn reload(&mut self, site: &Site) {
        match verdict::Index::load(&site.store, &site.library)
            .map_err(|error| format!("沉淀库读不动：{error}"))
            .and_then(|index| {
                Queue::load(&site.catalog, &index).map_err(|error| format!("中立库读不动：{error}"))
            }) {
            Ok(queue) => {
                self.queue = queue;
                self.queue.set_filter(self.picks.filter());
                self.error = None;
            }
            Err(message) => self.error = Some(message),
        }
        self.pending = None;
        self.picked = None;
    }

    /// 画一帧。
    pub fn ui(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        self.sync();
        egui::Panel::bottom("裁决面板")
            .default_size(268.0)
            .min_size(120.0)
            .show(ui, |ui| self.decide_panel(ui, site));
        egui::Panel::left("批量")
            .default_size(320.0)
            .min_size(180.0)
            .show(ui, |ui| self.batch_panel(ui));
        egui::CentralPanel::default().show(ui, |ui| self.table(ui));
        if self.pending.is_some() {
            self.plan_modal(ui.ctx(), site);
        }
    }

    /// 排出来还没落下的那份计划。
    #[must_use]
    pub fn pending(&self) -> Option<&Plan> {
        self.pending.as_ref()
    }

    /// 上一次落下的账。
    #[must_use]
    pub fn applied(&self) -> Option<&Applied> {
        self.applied.as_ref()
    }

    /// 裁决表单，供实测与测试填。
    pub fn form_mut(&mut self) -> &mut Form {
        &mut self.form
    }

    /// 点中分组表的一行。**界面上点下去走的就是它**，实测与测试拿它当那一下。
    pub fn pick(&mut self, axis: Axis, label: &str) {
        self.picks.pick(axis, label);
    }

    /// 点中表里的一行。界面上点那一下之后剩下的那半段就是它。
    pub fn pick_row(&mut self, key: &str) {
        self.picked = Some(key.to_string());
    }

    /// 只裁选中的那一条，还是整批。
    pub fn set_only_picked(&mut self, only: bool) {
        self.only_picked = only;
    }

    /// 把界面上那份选择器草稿写进队列，再把「选中的是哪一行」对到下标上。
    ///
    /// 顶栏与正文各画各的，而顶栏先画——不先同步一次，状态栏上那两个数就永远比表格慢
    /// 一帧。没换过选择器时它是空操作。
    fn sync(&mut self) {
        if self.picked.is_none() {
            self.only_picked = false;
        }
        // **一帧只换一次选择器**：换一次就是一次重新分区加三次重新分组，一万六千条上
        // 那是十几毫秒。所以「只裁这一条」不是在筛完之后再收一道口，而是从一开始就换成
        // 另一个选择器——点名那一个变体（[`Filter::keys`]），三个轴一概不管。于是
        // 「只裁这一条」就真的只是那一条，与眼下选择器里写着什么无关。
        let filter = match (self.only_picked, &self.picked) {
            (true, Some(key)) => Filter {
                keys: vec![key.clone()],
                states: self.picks.filter().states,
                ..Filter::default()
            },
            _ => self.picks.filter(),
        };
        self.queue.set_filter(filter);
        self.resolve_picked();
    }

    /// 顶栏上属于队列的那一段：队列多少条、选中多少条、重新列一次。
    pub fn status(&mut self, ui: &mut egui::Ui, site: &Site) {
        self.sync();
        if ui
            .button("重新列队列")
            .on_hover_text("识别跑过一趟之后点它。一个字节都不读主库。")
            .clicked()
        {
            self.reload(site);
        }
        ui.separator();
        if !self.queue.identified() {
            ui.label("还没跑过识别，队列无从谈起。先跑一次 `romcat identify`。");
            return;
        }
        let mut line = format!("队列 {} 条待裁决", thousands(self.queue.pending()));
        // **跳过**不算在待裁决里（它不是「拿不定主意」），但勾一下就连它们一起复核，
        // 那时选中的条数会大过待裁决数——不把这个数说出来，那两个数看着就是错的。
        if self.queue.skipped() > 0 {
            let _ = write!(line, "，另有 {} 条跳过", thousands(self.queue.skipped()));
        }
        let _ = write!(
            line,
            "；选中 {} 条",
            thousands(self.queue.selected().len() as u64)
        );
        ui.label(line);
    }

    /// 左边那三张分组表：**一行就是一次批量裁决能覆盖多少**。
    fn batch_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("从哪一批下手");
                if ui.button("整个队列").clicked() {
                    self.picks.clear_axes();
                }
            });
            ui.label("点一行就是一条覆盖几百条的选择器。");
            ui.separator();

            ui.strong("按识别结论");
            for (at, state) in State::ALL.iter().enumerate() {
                ui.checkbox(&mut self.picks.states[at], state.label());
            }

            for axis in Axis::ALL {
                ui.separator();
                ui.strong(axis.label());
                let rows = self.queue.groups(axis);
                if rows.is_empty() {
                    ui.weak(empty_axis(axis));
                    continue;
                }
                let mut clicked = None;
                for row in rows.iter().take(TOP) {
                    let label = if row.label.is_empty() {
                        "（主库根）"
                    } else {
                        row.label.as_str()
                    };
                    let on = self.picks.holds(axis, &row.label);
                    if ui
                        .selectable_label(on, format!("{}  {}", thousands(row.count), label))
                        .on_hover_text(format!(
                            "点它就只看这一批；命令行上是 `{} {}`",
                            axis.selector(),
                            row.label
                        ))
                        .clicked()
                    {
                        clicked = Some(row.label.clone());
                    }
                }
                if rows.len() > TOP {
                    ui.weak(format!("……另有 {} 组没列", thousands_len(rows.len() - TOP)));
                }
                if let Some(label) = clicked {
                    self.picks.pick(axis, &label);
                }
            }
        });
    }

    /// 中间那张表。**一个文本框都没有**：见模块文档。
    fn table(&mut self, ui: &mut egui::Ui) {
        if !self.queue.identified() {
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.label("还没跑过识别，队列无从谈起。先跑一次 `romcat identify`。");
            });
            return;
        }
        if self.queue.selected().is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.label("一条都没选中。选择器写宽一点，或者点「整个队列」。");
            });
            return;
        }
        let mut picked = None;
        let at = self.picked_index();
        let mut builder = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click())
            .column(Column::initial(420.0).at_least(180.0).clip(true))
            .column(Column::initial(70.0).at_least(50.0).clip(true))
            .column(Column::initial(90.0).at_least(60.0).clip(true))
            .column(Column::initial(60.0).at_least(50.0))
            .column(Column::remainder().at_least(90.0));
        if let Some(offset) = self.scroll_to {
            builder = builder.vertical_scroll_offset(offset);
        }
        builder
            .header(24.0, |mut header| {
                for title in ["变体", "结论", "平台", "候选", "容量"] {
                    header.col(|ui| {
                        ui.strong(title);
                    });
                }
            })
            .body(|body| {
                let items = self.queue.selected();
                body.rows(ROW_HEIGHT, items.len(), |mut row| {
                    let index = row.index();
                    let Some(item) = items.get(index) else {
                        return;
                    };
                    row.set_selected(at == Some(index));
                    row.col(|ui| {
                        ui.label(&item.variant.key);
                    });
                    row.col(|ui| {
                        ui.label(item.state.label());
                    });
                    row.col(|ui| {
                        ui.label(item.variant.platform.as_deref().unwrap_or("—"));
                    });
                    row.col(|ui| {
                        ui.label(item.candidates.len().to_string());
                    });
                    row.col(|ui| {
                        ui.label(capacity(item.variant.bytes, item.variant.unreadable_files));
                    });
                    if row.response().clicked() {
                        picked = Some((index, item.variant.key.clone()));
                    }
                });
            });
        if let Some((index, key)) = picked {
            self.picked_at = index;
            self.picked = Some(key);
        }
    }

    /// 底下那块面板：详情、选择器、裁决表单。**全部中文输入都在这里。**
    fn decide_panel(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        ui.add_space(4.0);
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if let Some(applied) = &self.applied {
            ui.colored_label(ui.visuals().warn_fg_color, applied_text(applied));
        }
        let available = ui.available_width();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2((available * 0.52).max(240.0), ui.available_height()),
                Layout::top_down(Align::Min),
                |ui| self.detail(ui, site),
            );
            ui.separator();
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), ui.available_height()),
                Layout::top_down(Align::Min),
                |ui| self.form_ui(ui, site),
            );
        });
    }

    /// 左半：这一条的**候选**、**置信度**与**依据**。
    fn detail(&mut self, ui: &mut egui::Ui, site: &Site) {
        let at = self.picked_index();
        let detail = match at.map(|at| self.queue.detail(&site.catalog, at)) {
            None => None,
            Some(Ok(item)) => item,
            Some(Err(error)) => {
                self.error = Some(format!("中立库读不动：{error}"));
                None
            }
        };
        let Some(item) = detail else {
            ui.weak("点表里的一行看它的候选与依据。选择器与中文输入都在右边这一栏。");
            return;
        };
        // 不写 `**内容**`：那是命令行报告里的记法，`ui.label` 会把星号照着画出来。
        let anchored = if item.print.is_some() {
            format!("{}——换台机器也认得出，可导出分享", verdict::ANCHOR_CONTENT)
        } else {
            format!("{}——只在本机成立", verdict::ANCHOR_PATH)
        };
        let (key, state, platform, bytes) = (
            item.variant.key.clone(),
            item.state.label(),
            item.variant.platform.clone(),
            capacity(item.variant.bytes, item.variant.unreadable_files),
        );
        let reason = item.reason.clone();
        let candidates: Vec<String> = item
            .candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| {
                format!(
                    "{}. [{}] {} 《{}》{}\n    依据：{}",
                    index + 1,
                    candidate.confidence.label(),
                    candidate.source,
                    candidate.game,
                    candidate
                        .chinese
                        .map(|mark| format!("  {}", mark.label()))
                        .unwrap_or_default(),
                    candidate.evidence,
                )
            })
            .collect();
        ui.checkbox(&mut self.only_picked, "只裁选中的这一条")
            .on_hover_text(
                "「采用第 N 条候选」天生是逐条的动作：同一批里各人的候选不是同一部游戏。",
            );
        egui::ScrollArea::vertical().id_salt("详情").show(ui, |ui| {
            ui.strong(&key);
            ui.label(format!(
                "{state}｜平台 {}｜容量 {bytes}",
                platform.as_deref().unwrap_or("未知"),
            ));
            ui.label(format!("裁决钉在：{anchored}"));
            if let Some(reason) = reason {
                ui.label(format!("为什么没定下来：{reason}"));
            }
            ui.separator();
            if candidates.is_empty() {
                ui.label(
                    "候选：一条都没有——要裁决就得手工指定作品。\
                         那是队列的常态，不是异常。",
                );
            }
            for line in &candidates {
                ui.label(line);
            }
        });
    }

    /// 右半：选择器与裁决表单。**这一栏里的每一个文本框都会碰到输入法。**
    fn form_ui(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        egui::ScrollArea::vertical().id_salt("裁决").show(ui, |ui| {
            egui::Grid::new("选择器")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    for axis in Axis::ALL {
                        ui.label(axis.label());
                        ui.add(
                            egui::TextEdit::singleline(self.picks.text_mut(axis))
                                .desired_width(f32::INFINITY)
                                .hint_text(axis.hint()),
                        )
                        .on_hover_text(format!("命令行上是 `{}`", axis.selector()));
                        ui.end_row();
                    }
                });
            ui.separator();

            ui.horizontal_wrapped(|ui| {
                ui.strong("裁成");
                for how in How::ALL {
                    ui.radio_value(&mut self.form.how, how, how.label());
                }
            });
            if self.form.how == How::Pick {
                ui.horizontal(|ui| {
                    ui.label("第几条候选");
                    ui.add(egui::DragValue::new(&mut self.form.pick).range(1..=9));
                });
            }

            egui::Grid::new("裁决事实")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    let facts = self.form.how.wants_facts();
                    text_row(ui, "作品", &mut self.form.work, self.form.how.wants_work());
                    text_row(ui, "汉化组", &mut self.form.team, facts);
                    text_row(ui, "版本", &mut self.form.version, facts);
                    text_row(ui, "平台", &mut self.form.platform, facts);
                    text_row(ui, "地区", &mut self.form.region, facts);
                    text_row(ui, "序列号", &mut self.form.serial, facts);
                    text_row(ui, "语言", &mut self.form.languages, facts);
                    ui.label("中文身份");
                    ui.add_enabled_ui(facts, |ui| {
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut self.form.chinese, None, "不记");
                            for mark in [ChineseMark::FanTranslated, ChineseMark::Official] {
                                ui.radio_value(&mut self.form.chinese, Some(mark), mark.label());
                            }
                        });
                    });
                    ui.end_row();
                });

            ui.label("备注（半年后你会想知道当初凭什么这么定）");
            ui.add(
                egui::TextEdit::multiline(&mut self.form.note)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );

            let draft = self.form.draft();
            let complaint = draft.check().err();
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                let ready = complaint.is_none() && !self.queue.selected().is_empty();
                if ui
                    .add_enabled(ready, egui::Button::new("预览这一批"))
                    .on_hover_text(
                        "先出计划再动手：一条命令改几百条记录，看不见就按下去，错了没处找。",
                    )
                    .clicked()
                {
                    self.preview(site, &draft);
                }
                ui.label(format!(
                    "选中 {} 条",
                    thousands(self.queue.selected().len() as u64)
                ));
            });
            if let Some(complaint) = complaint {
                ui.colored_label(ui.visuals().warn_fg_color, complaint);
            }
        });
    }

    /// 排一次计划。**界面上「预览这一批」按下去走的就是它。**
    pub fn preview(&mut self, site: &mut Site, draft: &Draft) {
        self.applied = None;
        match draft.build(&site.library).and_then(|decide| {
            self.queue
                .plan(&site.catalog, &site.store, &decide)
                .map_err(|error| format!("排不出计划：{error}"))
        }) {
            Ok(plan) => {
                self.error = None;
                self.pending = Some(plan);
            }
            Err(message) => self.error = Some(message),
        }
    }

    /// **差量预览**：这一趟会改什么，看过了才落得下去。
    fn plan_modal(&mut self, ctx: &egui::Context, site: &mut Site) {
        let Some(plan) = self.pending.take() else {
            return;
        };
        let mut keep = true;
        let mut go = false;
        egui::Modal::new(egui::Id::new("裁决计划")).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.heading("批量裁决计划");
            ui.label(format!(
                "要落下 {} 条：钉在内容上的 {} 条（可导出分享），只钉得住本机路径的 {} 条；\
                 其中盖掉已有裁决的 {} 条。",
                thousands(plan.decided.len() as u64),
                thousands(plan.content_anchored() as u64),
                thousands(plan.path_anchored() as u64),
                thousands(plan.replacing() as u64),
            ));
            if !plan.blocked.is_empty() {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("{} 条落不下去：", thousands(plan.blocked.len() as u64)),
                );
            }
            egui::ScrollArea::vertical()
                .id_salt("计划明细")
                .max_height(260.0)
                .show(ui, |ui| {
                    for row in plan.blocked.iter().take(20) {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            format!("{}：{}", row.key, row.why),
                        );
                    }
                    for row in plan.decided.iter().take(200) {
                        ui.label(format!(
                            "{}{}",
                            row.key,
                            if row.replaces {
                                "（盖掉已有的）"
                            } else {
                                ""
                            }
                        ));
                    }
                    if plan.decided.len() > 200 {
                        ui.weak(format!(
                            "……还有 {} 条没列",
                            thousands_len(plan.decided.len() - 200)
                        ));
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!plan.decided.is_empty(), egui::Button::new("落下"))
                    .clicked()
                {
                    go = true;
                    keep = false;
                }
                if ui.button("取消").clicked() {
                    keep = false;
                }
            });
        });
        if go {
            self.apply_plan(site, &plan);
        } else if keep {
            self.pending = Some(plan);
        }
    }

    /// 落下等着的那份计划。**模态框里「落下」按下去走的就是它。**
    pub fn commit(&mut self, site: &mut Site) {
        if let Some(plan) = self.pending.take() {
            self.apply_plan(site, &plan);
        }
    }

    /// 真的落下：写沉淀库、当场在中立库里兑现、把裁完的从队列里去掉。
    fn apply_plan(&mut self, site: &mut Site, plan: &Plan) {
        match self.queue.apply(&mut site.catalog, &mut site.store, plan) {
            Ok(applied) => {
                self.error = None;
                self.applied = Some(applied);
                self.picked = None;
            }
            Err(error) => self.error = Some(format!("裁决写不进去：{error}")),
        }
    }

    /// 把「选中的是哪一条」重新对到下标上。
    ///
    /// **记的是键，不是下标**：换个选择器表就重排了，下标会指到别人身上。对得上就直接
    /// 用，对不上才扫一遍；扫不着说明它被这一次筛选筛掉了，那就**放掉**——留着的话
    /// 每帧都要为一个已经不在表里的键把一万六千条重扫一遍。
    fn resolve_picked(&mut self) {
        let Some(key) = self.picked.clone() else {
            return;
        };
        let items = self.queue.selected();
        if items
            .get(self.picked_at)
            .map(|item| item.variant.key.as_str())
            == Some(key.as_str())
        {
            return;
        }
        match items.iter().position(|item| item.variant.key == key) {
            Some(at) => self.picked_at = at,
            None => self.picked = None,
        }
    }

    /// 选中的那一条眼下排在第几行；这一帧开头 [`Screen::resolve_picked`] 已经对准过了。
    fn picked_index(&self) -> Option<usize> {
        self.picked.as_ref().map(|_| self.picked_at)
    }
}

impl Default for Screen {
    fn default() -> Self {
        Self::new()
    }
}

/// 界面上那份**选择器**草稿。
///
/// 它与 [`Filter`] 的关系是「界面上的样子」与「领域里的样子」：三个轴各一个文本框、
/// 四档结论各一个勾。**折算只有一条路**（[`Picks::filter`]），分组表点一下走的也是它。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picks {
    under: String,
    name: String,
    candidate_work: String,
    /// 四档结论要不要，与 [`State::ALL`] 同序。
    states: [bool; State::ALL.len()],
}

impl Default for Picks {
    fn default() -> Self {
        Self {
            under: String::new(),
            name: String::new(),
            candidate_work: String::new(),
            // 默认那三档：**跳过**不在队列里——它不是「拿不定主意」，是「不该撞 DAT」。
            states: [true, true, true, false],
        }
    }
}

impl Picks {
    /// 折成一个**选择器**。
    #[must_use]
    pub fn filter(&self) -> Filter {
        let one = |text: &str| {
            let text = text.trim();
            if text.is_empty() {
                Vec::new()
            } else {
                vec![text.to_string()]
            }
        };
        Filter {
            under: one(&self.under),
            name_contains: one(&self.name),
            candidate_work: one(&self.candidate_work),
            states: State::ALL
                .iter()
                .zip(self.states)
                .filter_map(|(state, on)| on.then_some(*state))
                .collect(),
            ..Filter::default()
        }
    }

    /// 这个轴的文本框。
    fn text_mut(&mut self, axis: Axis) -> &mut String {
        match axis {
            Axis::Directory => &mut self.under,
            Axis::CandidateWork => &mut self.candidate_work,
            Axis::NameMark => &mut self.name,
        }
    }

    /// 点中分组表的一行：**换成这一批**；再点一次同一行就取消，回到整个队列。
    pub fn pick(&mut self, axis: Axis, label: &str) {
        let already = self.holds(axis, label);
        self.clear_axes();
        if !already {
            *self.text_mut(axis) = written(label);
        }
    }

    /// 这个轴眼下选的就是这一组吗。
    fn holds(&self, axis: Axis, label: &str) -> bool {
        self.text(axis) == written(label)
    }

    /// 这个轴框里现在写着什么。
    fn text(&self, axis: Axis) -> &str {
        match axis {
            Axis::Directory => &self.under,
            Axis::CandidateWork => &self.candidate_work,
            Axis::NameMark => &self.name,
        }
    }

    /// 三个轴全清掉，结论那四个勾不动。
    fn clear_axes(&mut self) {
        self.under.clear();
        self.name.clear();
        self.candidate_work.clear();
    }
}

/// 裁成哪一种。命令行上是四个开关，这里是四个单选钮，**说的是同一件事**。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum How {
    /// 采用第几条**候选**。
    Pick,
    /// **手工指定**作品。队列里绝大多数条目一条候选都没有，所以这是主路径。
    #[default]
    Manual,
    /// 确认它**没有发行版**：同人移植、homebrew。
    NoRelease,
    /// 都不对，而且认不出。
    Unknown,
}

impl How {
    const ALL: [Self; 4] = [Self::Manual, Self::Pick, Self::NoRelease, Self::Unknown];

    fn label(self) -> &'static str {
        match self {
            Self::Pick => "采用候选",
            Self::Manual => "手工指定作品",
            Self::NoRelease => "没有发行版",
            Self::Unknown => "认不出",
        }
    }

    /// 这一种说得出**作品**吗。
    fn wants_work(self) -> bool {
        matches!(self, Self::Manual | Self::NoRelease)
    }

    /// 这一种记得下汉化组、版本那几样事实吗。
    ///
    /// 「没有发行版」与「认不出」说的是**它不成其为一次发行**，那就没有平台、地区、
    /// 汉化组、版本可记。这里把那几个框灰掉，核心库那边照样会当场拦下
    /// （[`Draft::check`]）——界面只是让人不必先撞一次墙。
    fn wants_facts(self) -> bool {
        matches!(self, Self::Pick | Self::Manual)
    }
}

/// 界面上那份**裁决**草稿。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    /// 裁成哪一种。
    pub how: How,
    /// 采用第几条**候选**。
    pub pick: usize,
    /// **作品**名。裁决说不出这一样就什么也没定下来。
    pub work: String,
    /// 平台；不填就用变体自己的那个（目录是强先验，ADR-0011）。
    pub platform: String,
    /// 地区。
    pub region: String,
    /// 序列号。
    pub serial: String,
    /// 语言标记组。
    pub languages: String,
    /// 中文身份：**汉化版**还是**官中版**（ADR-0012）。
    pub chinese: Option<ChineseMark>,
    /// **汉化组**。自动识别只做到发行版级，这一样只有人说得出（ADR-0008）。
    pub team: String,
    /// 版本。
    pub version: String,
    /// 记一句为什么。
    pub note: String,
}

impl Default for Form {
    fn default() -> Self {
        Self {
            how: How::default(),
            // **候选的序号从 1 数起**（0 会被 `triage::Draft` 当场挡下）。
            pick: 1,
            work: String::new(),
            platform: String::new(),
            region: String::new(),
            serial: String::new(),
            languages: String::new(),
            chinese: None,
            team: String::new(),
            version: String::new(),
            note: String::new(),
        }
    }
}

impl Form {
    /// 折成一份 [`Draft`]。**说得成不成立由核心库判**，这里只负责搬。
    #[must_use]
    pub fn draft(&self) -> Draft {
        let some = |text: &String| {
            let text = text.trim();
            (!text.is_empty()).then(|| text.to_string())
        };
        let facts = self.how.wants_facts();
        Draft {
            pick: (self.how == How::Pick).then_some(self.pick),
            work: self.how.wants_work().then(|| some(&self.work)).flatten(),
            no_release: self.how == How::NoRelease,
            unknown: self.how == How::Unknown,
            overrides: if facts {
                Overrides {
                    platform: some(&self.platform),
                    region: some(&self.region),
                    serial: some(&self.serial),
                    languages: some(&self.languages),
                    chinese: self.chinese,
                    team: some(&self.team),
                    version: some(&self.version),
                }
            } else {
                Overrides::default()
            },
            note: some(&self.note),
        }
    }
}

/// 一行「标签 + 文本框」。灰掉的那些照画不误——位置在那儿，人才看得出还有这一样可填。
fn text_row(ui: &mut egui::Ui, label: &str, value: &mut String, enabled: bool) {
    ui.label(label);
    ui.add_enabled(
        enabled,
        egui::TextEdit::singleline(value).desired_width(f32::INFINITY),
    );
    ui.end_row();
}

/// 一组的标签在框里写成什么。**主库根那一组的标签是空串**，而空框的意思是「不筛」。
fn written(label: &str) -> String {
    if label.is_empty() {
        ROOT.to_string()
    } else {
        label.to_string()
    }
}

/// 这个轴上一组都没有时说的那句话。
fn empty_axis(axis: Axis) -> &'static str {
    match axis {
        Axis::Directory => "（选中的这些不在任何目录下）",
        Axis::CandidateWork => "（选中的这些一条候选都没有——那正是队列的常态，走手工指定）",
        Axis::NameMark => "（选中的这些名字里一个记号都没有）",
    }
}

/// 落下之后那一句账。
fn applied_text(applied: &Applied) -> String {
    format!(
        "裁决已沉淀 {} 条（新增 {}、盖掉 {}）：钉在内容上的 {} 条、只钉得住本机路径的 {} 条；\
         中立库当场兑现：{} 条转成命中、{} 条转成跳过。沉淀库不跟中立库走，删库重扫也不丢。",
        thousands(applied.verdicts),
        thousands(applied.added),
        thousands(applied.replaced),
        thousands(applied.content_anchored),
        thousands(applied.path_anchored),
        thousands(applied.matched),
        thousands(applied.skipped),
    )
}

fn thousands_len(value: usize) -> String {
    thousands(u64::try_from(value).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 默认那三档不含跳过() {
        let filter = Picks::default().filter();
        assert!(filter.keeps_state(State::Unmatched));
        assert!(filter.keeps_state(State::NoEvidence));
        assert!(filter.keeps_state(State::Matched));
        assert!(
            !filter.keeps_state(State::Skipped),
            "跳过不是「拿不定主意」，默认不该进队列",
        );
    }

    #[test]
    fn 点一行折出来的选择器就是那个轴说的那一个() {
        // 这是 `Axis` 存在的全部意义：表上写「852 条」，点下去就该是那 852 条。
        // 界面这一侧多绕一道（文本框），所以要单独钉一次。
        for (axis, label) in [
            (Axis::Directory, "gba/【全部汉化】"),
            (Axis::CandidateWork, "勇者斗恶龙"),
            (Axis::NameMark, "ACG汉化组"),
        ] {
            let mut picks = Picks::default();
            picks.pick(axis, label);
            let 界面 = picks.filter();
            let 领域 = axis.filter(label);
            assert_eq!(界面.under, 领域.under, "{label}");
            assert_eq!(界面.candidate_work, 领域.candidate_work, "{label}");
            assert_eq!(界面.name_contains, 领域.name_contains, "{label}");
        }
    }

    #[test]
    fn 主库根那一组点得动而且不默认高亮() {
        // 空框的意思是「这个轴不筛」，而主库根那一组的标签正好是空串。两件事不分开的话，
        // 那一行既永远显示为选中、又永远点不动——`Axis` 的不变式当场破掉。
        let mut picks = Picks::default();
        assert!(
            !picks.holds(Axis::Directory, ""),
            "什么都没选的时候主库根那一行不该是高亮的",
        );
        picks.pick(Axis::Directory, "");
        assert!(picks.holds(Axis::Directory, ""), "点了却没选中");
        // 框里写的是哨兵 `/`；它与领域侧的空前缀是同一件事，那一条钉在核心库的
        // `根那一层写成斜杠还是空串都是同一批` 上。
        assert_eq!(picks.filter().under, vec![ROOT.to_string()]);
        picks.pick(Axis::Directory, "");
        assert!(picks.filter().under.is_empty(), "再点一次该取消");
    }

    #[test]
    fn 点分组表换成那一批再点一次回到整个队列() {
        let mut picks = Picks::default();
        picks.pick(Axis::NameMark, "ACG汉化组");
        assert_eq!(picks.filter().name_contains, vec!["ACG汉化组".to_string()]);
        picks.pick(Axis::Directory, "gba/【全部汉化】");
        assert!(
            picks.filter().name_contains.is_empty(),
            "换一个轴该把上一个轴清掉，不然两条交起来只剩几条",
        );
        assert_eq!(picks.filter().under, vec!["gba/【全部汉化】".to_string()]);
        picks.pick(Axis::Directory, "gba/【全部汉化】");
        assert!(picks.filter().under.is_empty(), "再点一次该取消");
    }

    #[test]
    fn 说它不成其为一次发行就不收汉化组() {
        let mut form = Form {
            how: How::NoRelease,
            team: "外星科技".to_string(),
            ..Form::default()
        };
        // 界面把那几个框灰掉，草稿里也就没有它——收下再默默扔掉是最坏的一种「实现了」。
        assert!(form.draft().overrides.is_empty());
        form.how = How::Manual;
        form.work = "某作品".to_string();
        assert_eq!(form.draft().overrides.team.as_deref(), Some("外星科技"));
    }
}
