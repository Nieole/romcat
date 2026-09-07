//! **待确认队列**：工具的主界面（ADR-0002）。**打开看见的是分好的批，不是一万八千行的表。**
//!
//! ## 为什么是批而不是表
//!
//! 真机上待裁决 18,241 个变体，按 5 秒一条算是 **25 小时**——逐条不是可行路径。
//! 但其中八成只有**一个候选**：那不是「选哪个」，是「**对不对**」，而「对不对」
//! 可以按批回答。于是这一屏的正文是**工具已经分好的几十批**，每张卡片上常驻三样：
//!
//! 1. **条数**——这一批按下去会改多少条记录。
//! 2. **那句共同依据**——「工具凭什么这么认为」。整批通过时人验证的正是它。
//! 3. **随机样本**——可以换一组。三样齐了才敢按「整批通过 3,053 条」。
//!
//! 一级按**依据形状**分（源 / DAT / 置信度 / 哈希口径 / 候选数），二级按**目录或命名
//! 规律**下钻——坏的那几条往往集中在某一个目录里，随机抽样未必抽得到。
//! **每一层都能整批过或整批拒**，下钻到多细由人自己决定。
//!
//! ## 逐条是兜底不是主路径
//!
//! 多候选那些问的是「选哪个」，同一批里各人的候选不是同一部游戏——它们走
//! [`Mode::OneByOne`] 的键盘流：`←` `→` 切候选、`Y` 过、`N` 拒、`空格` 先放着、
//! `U` 撤销上一条。**逐条时文件名、路径与候选的完整依据都摆在屏上**，
//! 那是人按下去之前该看见的全部。
//!
//! ## 批量的胆量来自撤销可信
//!
//! 「整批通过 3,053 条」按错了要撤得干净，否则批量这件事本身不成立。撤销走的是
//! 票 `gui-redesign/08` 那条路（[`Queue::undo`]）：中立库与沉淀库两边一起回到这一批
//! 落下之前，**一个字节的 DAT 都不读**。逐条流里 `U` 撤的也是它——逐条落下的每一下
//! 自己就是一**批**。
//!
//! ## 中文输入全在底下那块面板里
//!
//! 一个 [`egui::TextEdit`] 都不进表格单元格。表格是虚拟化的，正在组字的那一行一旦滚出
//! 视口，那个控件就不存在了，输入法上屏时会没人接（ADR-0005 的修订段）。
//! `tests/queue.rs::表格里画多少行文本输入框都是那几个` 钉住这一条。
//!
//! ## 逐条时顺带裁得动**那一次匹配**
//!
//! 一条误撞的中文名带来的不只是中文名：简介、类型、开发商、发行商都来自变体那一次
//! 匹配、同一个条目号，它们**同生共死**。所以详情面板底下摆着这个变体身上那几堆
//! ——一堆一个条目号，堆上写着条目号与**依据**——就地裁一次，那一堆一并定下或一并
//! 失效（票 `queue-followups/06`）。**归堆与落裁决都在核心库**
//! （[`matched_groups`] 与 [`judge`]），命令行 `romcat zh matches` / `romcat zh judge`
//! 走的是同两条路。
//!
//! ## 领域判断一条都不在这里
//!
//! 队列怎么分批、二级怎么下钻、样本怎么抽、一次裁决说得成不成立、落下之后哪些该从
//! 队列里消失——全在 [`romcat_core::triage`]（[`Queue`]、[`Batch`]、[`Scope`]、
//! [`Axis`]、[`Draft`]）；哪几个字段来自同一次匹配、一条匹配裁决要清掉什么，
//! 在 [`romcat_core::scrape::zh`]。这一层只做三件事：把要来的画出来、把点的那一下
//! 写回去、把中文输入放在对的位置上（ADR-0005）。

use std::fmt::Write as _;

use egui::{Align, Layout};
use egui_extras::{Column, TableBuilder};
use romcat_core::catalog::State;
use romcat_core::catalog::identify::{NOT_RUN_LABEL, Tier};
use romcat_core::dat::chinese::ChineseMark;
use romcat_core::report::{capacity, thousands};
use romcat_core::scrape::AnchorKind;
use romcat_core::scrape::zh::{Judged, MatchGroup, judge, matched_groups};
use romcat_core::triage::batch::Coverage;
use romcat_core::triage::{
    Applied, Axis, Batch, Draft, Drill, Filter, Overrides, Plan, Queue, Sample, Scope, Shape,
    TriageError, Undone,
};
use romcat_core::verdict;

// **一行画得下的那一截**收在浏览屏那一处：一条简介在中立库里最多 4,000 字
// （`scrape::zh::DESCRIPTION_LIMIT`），两屏碰到的是同一个问题，各写一份迟早两种收法。
use crate::browse::one_line;
use crate::layout;
use crate::look;
use crate::table::ROW_HEIGHT;
use romcat_core::site::Site;

/// 分组表一个轴最多列几行。再多就不是给人看的了（与命令行报告同一个数）。
const TOP: usize = 12;

/// 一级分批最多摆几张卡片。
///
/// 剩下的**不是不算数**——屏头上写的是分成多少批、底下写的是没列的那些还剩多少条。
/// 卡片是给人一张一张看的东西，几百张摆出来等于回到那张一万八千行的表。
const TOP_BATCHES: usize = 60;

/// 每批屏上摆几条随机样本。
const SAMPLES: usize = 5;

/// 屏底那句「前几批盖住多少」按前几批算。
const HEADLINE: usize = 5;

/// 「主库根那一层」在**按目录**那个框里写成什么。
///
/// 空框的意思是「这个轴不筛」，而主库根那一组的标签正好**是空串**——两件事必须分得开，
/// 不然那一行既永远显示为选中、又永远点不动。`/` 折进选择器时会被 [`Filter::under`]
/// 剥掉，落到核心库那边就是空前缀，也就是主库根。
const ROOT: &str = "/";

/// 这一屏看的是**分好的批**还是**一条一条**。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    /// **批优先**：打开工具就是它。
    #[default]
    Batches,
    /// **逐条键盘流**：多候选那些的兜底。
    OneByOne,
}

/// 待确认队列这个屏幕。
pub struct Screen {
    queue: Queue,
    /// 看的是分批还是逐条。
    mode: Mode,
    /// 展开的是哪一批。**记形状不记下标**：裁完一批之后那一列卡片会重排。
    open: Option<Shape>,
    /// 二级用哪个轴分。**按目录是默认的**——只有它分得干净（一条只落一个目录）。
    axis: Axis,
    /// 在展开那一批里下钻到哪一组；`None` 就是整批。
    ///
    /// **它不是选择器**。借道 [`Picks`] 那三个文本框的话，下一帧 `sync` 会拿它去收窄
    /// **整个队列**——于是二级表塌成一行、别的批也跟着从屏上消失，而人只是想在这一批
    /// 里看细一点。下钻只该收窄**整批操作的作用范围**（[`Scope`]），一级那一列卡片
    /// 一个字都不该动。
    drill: Option<String>,
    /// 样本那一组的号。**「换一组样本」就是把它加一**。
    seed: u64,
    /// 展开那一批算出来的东西：条数、二级分组、随机样本。
    ///
    /// **缓着而不是每帧重算**：这三样各要走一遍这一批的全部条目，一批一万两千条上，
    /// 每帧重算就是每帧几万次分配。钥匙里的 [`Queue::revision`] 管住「队列变了」这一半
    /// ——裁完一批、撤回一批、换个选择器，它都会变。
    opened: Option<Opened>,
    /// **光标**停在哪一条：详情面板画的、键盘那几下作用的，都是它。
    ///
    /// **记键不记下标**：换个选择器表就重排了，下标会指到别人身上。
    cursor: Option<String>,
    /// 上一次找到它的下标。对得上就直接用，省得每帧扫一遍一万八千条。
    at: usize,
    /// 逐条流眼下切到第几条候选（从 0 数）。
    nth: usize,
    /// 界面上那份选择器草稿。
    picks: Picks,
    /// 界面上那份裁决草稿。
    form: Form,
    /// 排出来还没落下的计划，连**它是照着哪一版队列排的**。
    /// **先出计划再动手**（与同步那一侧的差量预览同源）。
    pending: Option<Pending>,
    /// 上一次落下的账。
    applied: Option<Applied>,
    /// 上一次撤回的账。
    undone: Option<Undone>,
    /// 光标底下那个变体身上，**中文离线源那几次匹配**各带来了哪些字段。
    ///
    /// **缓着而不是每帧重问**：归堆一趟要读两个锚点（变体与作品各一次查库），
    /// 而这一屏每秒画几十帧。
    matches: Option<Matched>,
    /// 上一次落下的那条**匹配裁决**的账。
    judged: Option<MatchJudged>,
    /// 匹配裁决那一格备注。**它会碰到输入法**，所以与别的文本框一样长在面板里。
    match_note: String,
    /// 上一次出的错。
    error: Option<String>,
    /// **这一屏刚动过中立库**（落下一批、撤回一批、放回一批），等窗口转告浏览屏。
    ///
    /// 裁决改的是结论与作品链接，而浏览屏那一列画的正是它们——不告诉它一声，它会一直
    /// 画着裁之前缓下来的那几行。与库屏扫完一个根走的是同一条路
    /// （[`crate::browse::Screen::invalidate`]），只是那一趟由任务台交回来，
    /// 这一趟就发生在本屏里，所以自己留个记号。
    changed: bool,
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
            mode: Mode::default(),
            open: None,
            axis: Axis::Directory,
            drill: None,
            seed: 0,
            opened: None,
            cursor: None,
            at: 0,
            nth: 0,
            picks: Picks::default(),
            form: Form::default(),
            pending: None,
            applied: None,
            undone: None,
            matches: None,
            judged: None,
            match_note: String::new(),
            error: None,
            changed: false,
            only_picked: false,
            scroll_to: None,
        }
    }

    /// 取走「**刚动过中立库**」那个记号。窗口每帧问一次，问到就转告浏览屏
    /// （`crate::app::App::route`）。
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// 队列本身，供测试查「列出多少条、分成几批」。
    #[must_use]
    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    /// 上一次出的错。
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// 看的是分批还是逐条。
    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// 回到分批那一屏。
    pub fn show_batches(&mut self) {
        self.mode = Mode::Batches;
        self.picks.shape = None;
        self.picks.clear_axes();
        self.refresh_opened();
    }

    /// 换到**逐条键盘流**。眼下展开着哪一批（下钻到哪一组），就只看那一批，
    /// 而且**从头看起**。
    ///
    /// 下钻那一层到这里才折进选择器：逐条要的正是「把队列收窄到这一组」，
    /// 而在分批那一屏上收窄整个队列是错的（见 [`Screen::drill`] 那个字段）。
    ///
    /// 光标一并放掉：不放的话它还钉在批优先那一屏上停过的某一条上，
    /// [`Screen::resolve_cursor`] 会把它捞回来，于是「逐条看」是从中间开始的。
    pub fn show_one_by_one(&mut self) {
        self.mode = Mode::OneByOne;
        self.picks.shape = self.open.clone();
        self.picks.clear_axes();
        if let Some(label) = self.drill.clone() {
            self.picks.pick(self.axis, &label);
        }
        self.cursor = None;
        self.at = 0;
        self.nth = 0;
    }

    /// 展开一批。**界面上点卡片走的就是它**，再点一次收起。
    pub fn open_batch(&mut self, shape: &Shape) {
        if self.open.as_ref() == Some(shape) {
            self.open = None;
        } else {
            self.open = Some(shape.clone());
        }
        self.axis = Axis::Directory;
        self.drill = None;
        self.seed = 0;
        self.refresh_opened();
    }

    /// 眼下展开的那一批连它的下钻，也就是**整批操作的作用范围**。
    #[must_use]
    pub fn scope(&self) -> Option<Scope> {
        let shape = self.open.clone()?;
        Some(match &self.drill {
            Some(label) => Scope::under(shape, self.axis, label),
            None => Scope::whole(shape),
        })
    }

    /// 二级换个轴分。换轴就是换了一套分法，下钻跟着作废。
    pub fn set_axis(&mut self, axis: Axis) {
        self.axis = axis;
        self.drill = None;
        self.seed = 0;
        self.refresh_opened();
    }

    /// **下钻**到二级的某一组；再点一次回到整批。
    pub fn drill_into(&mut self, label: &str) {
        self.drill = if self.drill.as_deref() == Some(label) {
            None
        } else {
            Some(label.to_string())
        };
        self.seed = 0;
        self.refresh_opened();
    }

    /// 回到整批：把下钻那一层放掉。
    pub fn drill_out(&mut self) {
        self.drill = None;
        self.seed = 0;
        self.refresh_opened();
    }

    /// **换一组样本**。
    pub fn resample(&mut self) {
        self.seed = self.seed.wrapping_add(1);
        self.refresh_opened();
    }

    /// 眼下这一批屏上摆着的那几条样本。
    #[must_use]
    pub fn samples(&self) -> Vec<Sample> {
        self.opened
            .as_ref()
            .map(|opened| opened.samples.clone())
            .unwrap_or_default()
    }

    /// 把展开那一批算出来的三样对齐到眼下的作用范围上。**算过的不再算。**
    fn refresh_opened(&mut self) {
        let Some(scope) = self.scope() else {
            self.opened = None;
            return;
        };
        let basis = Basis {
            revision: self.queue.revision(),
            scope,
            axis: self.axis,
            seed: self.seed,
        };
        if self.opened.as_ref().map(|opened| &opened.basis) == Some(&basis) {
            return;
        }
        // 二级那张表数的是**整批**：下钻只是把整批操作收窄，那张表本身不该跟着只剩一行
        // ——不然下钻一次就再也回不去了，屏上看不见还有哪些组。
        let whole = Scope::whole(basis.scope.shape.clone());
        self.opened = Some(Opened {
            count: self.queue.count(&basis.scope),
            drill: self.queue.drill(&whole, basis.axis),
            samples: self.queue.sample(&basis.scope, basis.seed, SAMPLES),
            basis,
        });
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
                // **头一批默认是展开的**（设计稿上就是这样）：那一批盖住的最多，
                // 而屏上常驻的三样里第三样——随机样本——只在展开的那张卡片上。
                // 一张都不展开的话，人打开这一屏一条样本都看不见。
                self.open = self
                    .queue
                    .batches()
                    .first()
                    .map(|batch| batch.shape.clone());
                self.drill = None;
                self.seed = 0;
                self.refresh_opened();
            }
            Err(message) => self.error = Some(message),
        }
        self.pending = None;
        self.cursor = None;
        // 这两样都跟着光标那一条走，而光标刚放掉了。留着的话，人再走回那一条上时
        // 看见的是上一份库上的账。
        self.matches = None;
        self.judged = None;
    }

    /// 画一帧。
    pub fn ui(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        self.sync();
        match self.mode {
            Mode::Batches => {
                egui::CentralPanel::default().show(ui, |ui| self.batches_ui(ui, site));
            }
            Mode::OneByOne => {
                // 两条边界拖得动也记得住，声明在 [`crate::layout`]（票 `gui-redesign/12`）。
                layout::DECIDE.show(ui, |ui| self.decide_panel(ui, site));
                layout::BATCHES.show(ui, |ui| self.batch_panel(ui));
                egui::CentralPanel::default().show(ui, |ui| self.table(ui));
                self.keyboard(ui.ctx(), site);
            }
        }
        if self.pending.is_some() {
            self.plan_modal(ui.ctx(), site);
        }
    }

    /// 排出来还没落下的那份计划。
    #[must_use]
    pub fn pending(&self) -> Option<&Plan> {
        self.pending.as_ref().map(|pending| &pending.plan)
    }

    /// 上一次撤回的账。
    #[must_use]
    pub fn undone(&self) -> Option<&Undone> {
        self.undone.as_ref()
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

    /// **匹配裁决**那一格备注，供实测与测试填。界面上是那一栏里的文本框
    /// （命令行上是 `romcat zh judge --note`）。
    pub fn match_note_mut(&mut self) -> &mut String {
        &mut self.match_note
    }

    /// 点中分组表的一行。**界面上点下去走的就是它**，实测与测试拿它当那一下。
    pub fn pick(&mut self, axis: Axis, label: &str) {
        self.axis = axis;
        self.picks.pick(axis, label);
    }

    /// 点中表里的一行：**把光标挪过去**。界面上点那一下之后剩下的那半段就是它。
    pub fn pick_row(&mut self, key: &str) {
        self.cursor = Some(key.to_string());
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
        if self.cursor.is_none() {
            self.only_picked = false;
        }
        // **一帧只换一次选择器**：换一次就是一次重新分区加几次重新分组，一万八千条上
        // 那是十几毫秒。所以「只裁这一条」不是在筛完之后再收一道口，而是从一开始就换成
        // 另一个选择器——点名那一个变体（[`Filter::keys`]），别的一概不管。
        let filter = match (self.only_picked, &self.cursor) {
            (true, Some(key)) => Filter {
                keys: vec![key.clone()],
                states: self.picks.filter().states,
                ..Filter::default()
            },
            _ => self.picks.filter(),
        };
        self.queue.set_filter(filter);
        self.drop_stale_plan();
        self.resolve_cursor();
        // 展开的那一批可能已经被裁光了——卡片没了，展开状态跟着收起来。
        if let Some(shape) = &self.open
            && !self
                .queue
                .batches()
                .iter()
                .any(|batch| batch.shape == *shape)
        {
            self.open = None;
        }
        self.refresh_opened();
    }

    /// 顶栏上属于队列的那一段：队列多少条、多少条候选、**库里还有多少个连识别都没跑过**、
    /// **四档各多少**、重新列一次。
    ///
    /// 「还没识别」那一句与命令行 `triage list` 印的是同一句话
    /// （[`romcat_core::triage::report::QueueReport`]）：那些变体一条候选都没有、
    /// **队列里根本没有它们**，选择器也筛不到。不说出来的话，「队列 N 条」会被读成
    /// 「库里只剩 N 条没定下来」，而该做的事也不一样——这一句指向 `identify`，
    /// 不是指向裁决。
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
            "；选中 {} 条 · {} 条候选",
            thousands(self.queue.selected().len() as u64),
            thousands(self.queue.candidates()),
        );
        ui.label(line);
        // **还没识别的那些单说一句**（词表「还没识别」条、[`NOT_RUN_LABEL`]）。
        // 它们不在上面那个数里——`variant JOIN identification` 一行都进不去，所以既不是
        // 「拿不定主意」，也不是底下四档里的任何一档。措辞照命令行那份报告来
        // （`romcat_core::triage::report`），同一份库两处印出来的是同一句话。
        if self.queue.not_run() > 0 {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!(
                    "{NOT_RUN_LABEL} 另有 {} 个变体连识别都还没跑过",
                    thousands(self.queue.not_run())
                ),
            )
            .on_hover_text(
                "它们一条候选都没有，队列里根本没有它们，选择器也筛不到。\
                 先跑一趟 `romcat identify` 把它们补上。",
            );
        }
        ui.separator();
        // **四档一眼看得出哪批稳、哪批悬**（规格 37）。颜色与标签同出一处
        // （[`tier_color`]、[`Tier::label`]），五屏对齐是票 `gui-redesign/12` 的活。
        // 第四档叫「**没有候选**」而不叫「还没识别」——上面那一句说的才是后者。
        // 词表两个词各立一条（`CONTEXT.md` 的**还没识别**与**没有候选**）。
        for (tier, count) in self.queue.tiers() {
            ui.colored_label(
                look::tier_color(*tier, ui.visuals()),
                format!("{} {}", tier.label(), thousands(*count)),
            );
        }
    }

    /// **正文：一级分批那一列卡片。**
    fn batches_ui(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        if !self.queue.identified() {
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.label("还没跑过识别，队列无从谈起。先跑一次 `romcat identify`。");
            });
            return;
        }
        self.applied_row(ui, site);
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        // **只把要画的那几张卡片拷出来**：全部批可能是好几百，每帧整份克隆等于白拷。
        // 借出来的那份还得让下面几行调得动 `&mut self`（展开、下钻、整批过）。
        // 数一个都不在这儿算——账由核心库交出来（ADR-0005）。
        let 头一句 = headline(&self.queue.coverage(HEADLINE));
        let 没列的 = self.queue.coverage(TOP_BATCHES);
        let batches: Vec<Batch> = self
            .queue
            .batches()
            .iter()
            .take(TOP_BATCHES)
            .cloned()
            .collect();
        if batches.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.label("队列是空的——该裁的都裁完了。");
            });
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.strong(format!(
                "一级 · 按依据形状分成 {} 批",
                thousands_len(没列的.batches)
            ));
            ui.weak("（源 / DAT / 置信度 / 哈希口径 / 候选数）");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .button("逐条看整个队列")
                    .on_hover_text("多候选那些问的是「选哪个」，走键盘流：←→ 切候选、Y 过、N 拒、空格 先放着、U 撤销上一条。")
                    .clicked()
                {
                    self.open = None;
                    self.show_one_by_one();
                }
            });
        });
        ui.label(头一句);
        ui.separator();
        let mut clicked: Option<Shape> = None;
        egui::ScrollArea::vertical().id_salt("分批").show(ui, |ui| {
            for batch in &batches {
                if self.card(ui, site, batch) {
                    clicked = Some(batch.shape.clone());
                }
            }
            if 没列的.rest_batches > 0 {
                ui.weak(format!(
                    "……另有 {} 批没列（共 {} 条）。先把上面这几批过完——它们盖住的最多。",
                    thousands_len(没列的.rest_batches),
                    thousands(没列的.rest),
                ));
            }
        });
        if let Some(shape) = clicked {
            self.open_batch(&shape);
        }
    }

    /// 一批的卡片。返回「这一帧点了它的标题栏」。
    fn card(&mut self, ui: &mut egui::Ui, site: &mut Site, batch: &Batch) -> bool {
        let open = self.open.as_ref() == Some(&batch.shape);
        let mut hit = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                // **置信度色条**：带置信度的行与卡，左边缘一条色带（规格 69）。
                // 那个词摆在这一行的右头（底下 `tier_label` 那一句）——**色条从不单独出现**。
                look::tier_bar(ui, batch.tier());
                let title = egui::RichText::new(thousands(batch.count)).strong();
                hit |= ui
                    .selectable_label(open, title)
                    .on_hover_text("点开看二级下钻与随机样本")
                    .clicked();
                hit |= ui.selectable_label(open, batch.why()).clicked();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    look::tier_label(ui, batch.tier());
                });
            });
            if open {
                self.opened_card(ui, site, batch);
            }
        });
        hit
    }

    /// 展开之后那一块：二级下钻、随机样本、整批操作。
    fn opened_card(&mut self, ui: &mut egui::Ui, site: &mut Site, batch: &Batch) {
        let Some(opened) = self.opened.clone() else {
            return;
        };
        let scope = opened.basis.scope.clone();
        ui.separator();
        // ——— 二级下钻 ———
        ui.horizontal_wrapped(|ui| {
            ui.strong("二级");
            let mut axis = self.axis;
            for one in Axis::ALL {
                ui.radio_value(&mut axis, one, one.label());
            }
            if axis != self.axis {
                self.set_axis(axis);
            }
            if self.drill.is_some() && ui.button("回到整批").clicked() {
                self.drill_out();
            }
        });
        let drilled = &opened.drill;
        if drilled.rows.is_empty() {
            ui.weak(empty_axis(self.axis));
        }
        let mut into: Option<String> = None;
        for row in drilled.rows.iter().take(TOP) {
            let label = if row.label.is_empty() {
                "（主库根）"
            } else {
                row.label.as_str()
            };
            let on = self.drill.as_deref() == Some(row.label.as_str());
            if ui
                .selectable_label(on, format!("{}  {label}", thousands(row.count)))
                .on_hover_text("下钻：只看这一组，整批操作也只作用于它")
                .clicked()
            {
                into = Some(row.label.clone());
            }
        }
        if drilled.rows.len() > TOP {
            ui.weak(format!(
                "……另有 {} 组没列",
                thousands_len(drilled.rows.len() - TOP)
            ));
        }
        // **加不加得起来要说出口**：只有按目录那个轴一条只落一个组。
        if !drilled.adds_up() {
            ui.weak(format!(
                "（这个轴上一条能落进好几组，所以各组加起来 {} 大过这一批的 {} 条；\
                 另有 {} 条一组都没落进。按目录那个轴是分得干净的。）",
                thousands(drilled.rows.iter().map(|row| row.count).sum::<u64>()),
                thousands(drilled.total),
                thousands(drilled.ungrouped),
            ));
        }
        if let Some(label) = into {
            self.drill_into(&label);
        }

        // ——— 随机样本 ———
        let count = opened.count;
        ui.separator();
        ui.strong(format!("随机样本 {} 条", opened.samples.len()));
        for one in &opened.samples {
            ui.horizontal_wrapped(|ui| {
                ui.label(&one.name);
                ui.weak(match &one.candidate {
                    Some(game) => format!("→ {game}"),
                    None => "→ 一条候选都没有".to_string(),
                });
            });
        }

        // ——— 整批操作 ———
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            let passable = batch.passable();
            if ui
                .add_enabled(
                    passable && count > 0,
                    egui::Button::new(format!("整批通过 {} 条", thousands(count))),
                )
                .on_hover_text("采用第一条候选——分批时那句共同依据说的正是它。先出计划再动手。")
                .on_disabled_hover_text(
                    "这一批不是单候选：一条都没有时「通过」什么也没定下来，\
                     好几条时它是替你挑了个没看过的答案。走「逐条看」。",
                )
                .clicked()
            {
                self.pass(site, &scope);
            }
            if ui.button("换一组样本").clicked() {
                self.resample();
            }
            if ui.button("逐条看").clicked() {
                self.show_one_by_one();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(count > 0, egui::Button::new("整批拒绝"))
                    .on_hover_text(
                        "记成「我看过了，认不出」：这些条退出队列，不再问第二遍。撤得回来。",
                    )
                    .clicked()
                {
                    self.reject(site, &scope);
                }
            });
        });
        ui.weak(format!("作用范围：{}", scope.label()));
    }

    /// 左边那三张分组表：**一行就是一次批量裁决能覆盖多少**（逐条那一屏用）。
    fn batch_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("← 回到分批").clicked() {
                    self.show_batches();
                }
                if ui.button("整个队列").clicked() {
                    self.picks.clear_axes();
                    self.picks.shape = None;
                }
            });
            if let Some(shape) = &self.picks.shape {
                ui.weak(format!("只看这一批：{}", shape.label()));
            }
            ui.separator();
            ui.strong("从哪一批下手");
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
        let at = self.at;
        // 行画完之后手上没有那一行的 `Ui` 了——先把上下文留一份，焦点那一圈要用。
        let ctx = ui.ctx().clone();
        let mut builder = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click())
            .column(Column::initial(420.0).at_least(180.0).clip(true))
            .column(Column::initial(70.0).at_least(50.0).clip(true))
            .column(Column::initial(90.0).at_least(60.0).clip(true))
            // 「候选」那一栏画的是「3 · 高置信」而不是光一个数——摆得下那个词才行。
            .column(Column::initial(120.0).at_least(84.0).clip(true))
            .column(Column::remainder().at_least(90.0));
        if let Some(offset) = self.scroll_to {
            builder = builder.vertical_scroll_offset(offset);
        }
        builder
            .header(24.0, |mut header| {
                for title in ["变体", "结论", "平台", "候选 · 置信度", "容量"] {
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
                    row.set_selected(at == index);
                    // 焦点那一圈要夹在滚动视口里，而只有格子里头拿得到那个裁剪矩形。
                    let mut 看得见的 = egui::Rect::NOTHING;
                    row.col(|ui| {
                        看得见的 = ui.clip_rect();
                        ui.label(&item.variant.key);
                    });
                    row.col(|ui| {
                        ui.label(item.state.label());
                    });
                    row.col(|ui| {
                        ui.label(item.variant.platform.as_deref().unwrap_or("—"));
                    });
                    row.col(|ui| {
                        // 候选那一栏也上四档的色：一眼看得出哪一条稳。
                        // **哪一档由核心库说**（`Item::tier`）——「看第一条候选」那条规则
                        // 只该有一份，界面再写一遍迟早与分批的键指着不同的候选。
                        //
                        // **数字后面跟着那一档的词**：只染色的话，色觉障碍下这一栏就只剩
                        // 一个孤零零的数（票 `gui-redesign/12` 验收第 5 条）。
                        ui.colored_label(
                            look::tier_color(item.tier(), ui.visuals()),
                            format!("{} · {}", item.candidates.len(), item.tier().label()),
                        );
                    });
                    row.col(|ui| {
                        ui.label(capacity(item.variant.bytes, item.variant.unreadable_files));
                    });
                    // 焦点落在这一行上要看得见：行是点得中的，Tab 走得到它
                    // （票 `gui-redesign/12` 验收第 7 条）。
                    let response = row.response();
                    look::focus_ring(&ctx, 看得见的, &response);
                    if response.clicked() {
                        picked = Some((index, item.variant.key.clone()));
                    }
                });
            });
        if let Some((index, key)) = picked {
            self.at = index;
            self.cursor = Some(key);
            self.nth = 0;
        }
    }

    /// 落下 / 撤回那一行账，连它右边那个走得回来的按钮。
    fn applied_row(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        // **落下之后那一行，右边就是「撤回这一批」。** 批量的胆量来自撤销可信
        // （票 gui-redesign/08）——走回来那一下要在按下去的地方，不该逼人去开命令行。
        let mut undo = false;
        if let Some(applied) = &self.applied {
            let batch = applied.batch;
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(ui.visuals().warn_fg_color, applied_text(applied));
                undo = ui
                    .button(format!("撤回第 {batch} 批"))
                    .on_hover_text(
                        "中立库与沉淀库两边都回到这一批落下之前，\
                         那些变体当场回到待裁决——不必重跑识别。",
                    )
                    .clicked();
            });
        }
        if undo {
            self.undo_last(site);
        }
        // 撤回之后那一行，右边就是「放回去」。**撤销本身也撤得回来**——按错了撤回、
        // 又发现撤错了，不该逼人把刚才那一批重打一遍。
        let mut redo = false;
        if let Some(undone) = &self.undone {
            let batch = undone.batch;
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(ui.visuals().warn_fg_color, undone_text(undone));
                redo = ui
                    .button(format!("放回第 {batch} 批"))
                    .on_hover_text(
                        "把这一批原样放回去：当初落下的每一条都记在批里，一个字都不必重打。",
                    )
                    .clicked();
            });
        }
        if redo {
            self.redo_last(site);
        }
    }

    /// 底下那块面板：详情、选择器、裁决表单。**全部中文输入都在这里。**
    fn decide_panel(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        ui.add_space(4.0);
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        self.applied_row(ui, site);
        ui.weak(
            "键盘：← → 切候选、Y 通过、N 拒绝、空格 先放着（不写库）、U 撤销上一条。\
             光标在文本框里时键盘归文本框。",
        );
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

    /// 左半：这一条的**文件名、路径**与全部**候选**、**置信度**、**依据**，
    /// 底下接着这个变体身上**中文离线源那几次匹配**（[`Screen::matches_ui`]）。
    fn detail(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        let at = self.at;
        let detail = match self.queue.detail(&site.catalog, at) {
            Ok(item) => item,
            Err(error) => {
                self.error = Some(format!("中立库读不动：{error}"));
                None
            }
        };
        let Some(item) = detail else {
            ui.weak("队列里一条都没有了。回到分批那一屏看看还剩什么。");
            return;
        };
        // 不写 `**内容**`：那是命令行报告里的记法，`ui.label` 会把星号照着画出来。
        let anchored = if item.print.is_some() {
            format!("{}——换台机器也认得出，可导出分享", verdict::ANCHOR_CONTENT)
        } else {
            format!("{}——只在本机成立", verdict::ANCHOR_PATH)
        };
        let (key, name, directory) = (
            item.variant.key.clone(),
            item.name().to_string(),
            item.directory().to_string(),
        );
        let (state, platform, bytes) = (
            item.state.label(),
            item.variant.platform.clone(),
            capacity(item.variant.bytes, item.variant.unreadable_files),
        );
        let reason = item.reason.clone();
        let candidates: Vec<(Tier, String)> = item
            .candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| {
                (
                    Tier::of(Some(candidate.confidence)),
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
                    ),
                )
            })
            .collect();
        let nth = self.nth.min(candidates.len().saturating_sub(1));
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.only_picked, "只裁选中的这一条")
                .on_hover_text(
                    "「采用第 N 条候选」天生是逐条的动作：同一批里各人的候选不是同一部游戏。",
                );
            ui.weak(format!(
                "第 {} / {} 条",
                thousands(at as u64 + 1),
                thousands(self.queue.selected().len() as u64),
            ));
        });
        egui::ScrollArea::vertical().id_salt("详情").show(ui, |ui| {
            // **文件名与路径分两行**：人裁决时先认名字，路径是用来判「这一批是不是同一堆」的。
            ui.strong(&name);
            ui.weak(format!("路径 {directory}"));
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
            for (index, (tier, line)) in candidates.iter().enumerate() {
                let color = look::tier_color(*tier, ui.visuals());
                if index == nth {
                    ui.colored_label(color, egui::RichText::new(line).strong());
                } else {
                    ui.colored_label(color, line);
                }
            }
            self.matches_ui(ui, site, &key);
        });
    }

    /// 详情底下那一块：**中文离线源那几次匹配**，一堆一次裁决。
    ///
    /// 归堆、落裁决、就地清库全在核心库（[`matched_groups`] 与 [`judge`]，
    /// 票 `offline-chinese-fields/05`）——这一层只把要来的那几堆画出来、把按下的那一下
    /// 转过去（ADR-0005）。命令行 `romcat zh matches` / `romcat zh judge` 走的是同两条路。
    ///
    /// **一堆都没有就一个字都不画**：绝大多数变体身上中文离线源一个字段都没产出，
    /// 画一句「没有」只是每一条都多一行废话。刚裁过的那一条例外——否定那一档当场把
    /// 那一堆清掉了，账那一行还得留在屏上，不然人按完什么都看不见。
    fn matches_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, key: &str) {
        self.refresh_matches(site, key);
        // 账先折成一句话，再去借那几堆：两样一个借 `self.judged`、一个借
        // `self.matches`，而底下那格备注还要 `&mut self.match_note`。
        let 账 = self
            .judged
            .as_ref()
            .filter(|judged| judged.key == key)
            .map(judged_text);
        let 空 = self
            .matches
            .as_ref()
            .is_none_or(|matched| matched.groups.is_empty());
        if 空 && 账.is_none() {
            return;
        }
        ui.separator();
        ui.strong("中文离线源那几次匹配");
        ui.weak(
            "一条裁决管住同一次匹配带来的全部字段——中文名、别名、类型、简介、\
             开发商、发行商同生共死，不必对同一次误撞裁五遍。",
        );
        if let Some(账) = &账 {
            ui.colored_label(ui.visuals().warn_fg_color, 账);
        }
        if 空 {
            return;
        }
        ui.horizontal(|ui| {
            ui.label("备注");
            ui.add(
                egui::TextEdit::singleline(&mut self.match_note)
                    .desired_width(f32::INFINITY)
                    .hint_text("半年后你会想知道当初凭什么这么定"),
            )
            .on_hover_text("写在这里的话跟着你按下的那一下记进裁决；命令行上是 `--note`。");
        });
        let 按下 = self
            .matches
            .as_ref()
            .and_then(|matched| match_groups_ui(ui, &matched.groups));
        if let Some((entry, accepted)) = 按下 {
            self.judge_match(site, key, entry, accepted);
        }
    }

    /// 把那几堆对齐到光标底下这一条上。**算过的不再算。**
    ///
    /// 钥匙是「哪个变体」加 [`Queue::revision`]（裁完一批、撤回一批、换个选择器它都会
    /// 变）。落下一条**匹配裁决**改的是库、队列的版号不动，所以那一下由
    /// [`Screen::judge_match`] 自己把这份缓存作废。
    ///
    /// **别处跑完一趟刮削它不会自己知道**：那一趟改的是中立库，队列的版号一个数都没动。
    /// 光标挪一下或者「重新列队列」就回来了——而跑刮削本来就是从别的屏出发的一趟长活。
    ///
    /// ⚠️ **版号本身不是单调的**（[`Queue::reload`] 会把它归零再由选择器推回 1）。
    /// 这份缓存作数，靠的是「按批裁决那条路一个字都不碰刮削那张表」，不是靠钥匙不重复；
    /// 真会改到那张表的那一下（[`Screen::judge_match`]）自己把它作废。
    fn refresh_matches(&mut self, site: &Site, key: &str) {
        let revision = self.queue.revision();
        if self
            .matches
            .as_ref()
            .is_some_and(|matched| matched.key == key && matched.revision == revision)
        {
            return;
        }
        match matched_groups(&site.catalog, key) {
            Ok(groups) => {
                self.matches = Some(Matched {
                    key: key.to_string(),
                    revision,
                    groups,
                });
            }
            Err(error) => {
                // **读不动也缓下来**：不缓的话下一帧再问一次注定失败的库，而那句
                // 「中立库读不动」会把屏上别的话（「裁不下去：……」，连同裁成功之后那次
                // 清错误）每帧盖掉一次——人看不见自己刚按的那一下到底怎么了。
                // 缓成空的，重试就跟着光标与版号走，不跟着帧走。
                self.matches = Some(Matched {
                    key: key.to_string(),
                    revision,
                    groups: Vec::new(),
                });
                self.error = Some(format!("中立库读不动：{error}"));
            }
        }
    }

    /// 落下一条**匹配裁决**：说这一次匹配就是它（`accepted`），或者说它不对。
    ///
    /// **界面上按那两颗按钮走的就是它。** 判断一条都不在这里——[`judge`] 就是命令行
    /// `romcat zh judge` 走的那一个函数（ADR-0005）：落沉淀库、锚在**内容锚**上、
    /// 否定那一档就地清库，全在核心库里。
    pub fn judge_match(&mut self, site: &mut Site, key: &str, entry: u32, accepted: bool) {
        let note = {
            let text = self.match_note.trim();
            (!text.is_empty()).then(|| text.to_string())
        };
        match judge(
            &mut site.catalog,
            &mut site.store,
            &site.library,
            key,
            entry,
            accepted,
            note,
        ) {
            Ok(judged) => {
                self.error = None;
                // **收下就用掉**：留着的话，下一条裁决会悄悄带上一句写给别人的话。
                self.match_note.clear();
                // 否定那一档当场清了库，而队列的版号一个数都没动——缓着的那几堆
                // 得重问一遍，不然屏上还摆着刚被清掉的那些值。
                self.matches = None;
                self.judged = Some(MatchJudged {
                    key: key.to_string(),
                    entry,
                    accepted,
                    judged,
                });
            }
            Err(error) => self.error = Some(format!("裁不下去：{error}")),
        }
    }

    /// 上一次落下的那条**匹配裁决**的账。
    #[must_use]
    pub fn judged(&self) -> Option<&Judged> {
        self.judged.as_ref().map(|judged| &judged.judged)
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

    /// **逐条键盘流**：`←` `→` 切候选、`Y` 过、`N` 拒、`空格` 先放着、`U` 撤销上一条。
    ///
    /// 光标在文本框里时一个键都不接——那一栏里正打着中文，`Y` 是用户要的字母不是命令。
    ///
    /// **计划书开着时也一个键都不接。** egui 的 [`egui::Modal`] 只拦得住指针、拦不住
    /// 键盘（0.36），于是计划书开着按 `N` 会当场落下光标那一条——而屏上那份计划书还
    /// 写着排它时的那一批，人再点「落下」时它已经过期了。模态框是这一层自己的东西，
    /// 所以这一道门也在这一层（[`Screen::drop_stale_plan`] 说了它与核心库那道门各管
    /// 各的什么）。
    fn keyboard(&mut self, ctx: &egui::Context, site: &mut Site) {
        if self.pending.is_some() || ctx.egui_wants_keyboard_input() {
            return;
        }
        let (mut pass, mut reject, mut skip, mut undo, mut back, mut forth) =
            (false, false, false, false, false, false);
        ctx.input(|input| {
            pass = input.key_pressed(egui::Key::Y);
            reject = input.key_pressed(egui::Key::N);
            skip = input.key_pressed(egui::Key::Space);
            undo = input.key_pressed(egui::Key::U);
            back = input.key_pressed(egui::Key::ArrowLeft);
            forth = input.key_pressed(egui::Key::ArrowRight);
        });
        let candidates = self
            .queue
            .selected()
            .get(self.at)
            .map_or(0, |item| item.candidates.len());
        if back {
            self.nth = self.nth.saturating_sub(1);
        }
        if forth && self.nth + 1 < candidates {
            self.nth += 1;
        }
        if skip {
            // **「先放着」什么都不写**——它与识别结论那一档「跳过」不是一回事，
            // 后者说的是「不该撞 DAT」，这里说的是「这一条我等会儿再看」
            // （`CONTEXT.md` 的**先放着**与**跳过**两条词条）。
            self.move_to(self.at + 1);
        }
        if pass {
            if candidates > 0 {
                self.decide_here(
                    site,
                    &Draft {
                        pick: Some(self.nth + 1),
                        ..Draft::default()
                    },
                );
            } else {
                // **不许什么都不做还不吭声**：队列里一条候选都没有的是常态，
                // 那时 `Y` 无从采用——说清楚该走哪条路，而不是让人以为键盘坏了。
                self.error = Some(
                    "这一条一条候选都没有，`Y` 没什么可采用的——右边手工指定作品，\
                     或者 `N` 记成「我看过了，认不出」。"
                        .to_string(),
                );
            }
        }
        if reject {
            self.decide_here(
                site,
                &Draft {
                    unknown: true,
                    ..Draft::default()
                },
            );
        }
        if undo {
            self.undo_last(site);
        }
    }

    /// 逐条流按下 `Y` / `N` 那一下：**只裁光标底下这一条**，当场落下。
    ///
    /// 逐条不走计划书那道门：一次只改一条，看得见的就是屏上这一条本身；而每一下自己
    /// 就是一**批**，`U` 撤的正是它。
    fn decide_here(&mut self, site: &mut Site, draft: &Draft) {
        let Some(item) = self.queue.selected().get(self.at) else {
            return;
        };
        let scope_keys = vec![item.variant.key.clone()];
        let decide = match draft.build(&site.library) {
            Ok(decide) => decide,
            Err(message) => {
                self.error = Some(message);
                return;
            }
        };
        let saved = self.picks.clone();
        self.queue.set_filter(Filter {
            keys: scope_keys,
            states: saved.filter().states,
            ..Filter::default()
        });
        let outcome = self
            .queue
            .plan(&site.catalog, &site.store, &decide)
            .map_err(|error| format!("排不出计划：{error}"))
            .and_then(|plan| {
                self.queue
                    .apply(&mut site.catalog, &mut site.store, &plan)
                    .map_err(|error| format!("裁决写不进去：{error}"))
            });
        self.picks = saved;
        self.queue.set_filter(self.picks.filter());
        match outcome {
            Ok(applied) => {
                self.error = None;
                self.applied = Some(applied);
                self.undone = None;
                self.changed = true;
                // 裁完这一条，**下一条自己滑到光标底下**——手不必动。
                self.move_to(self.at);
            }
            Err(message) => self.error = Some(message),
        }
    }

    /// **整批通过**：采用第一条候选。分批时那句共同依据说的正是它。
    pub fn pass(&mut self, site: &mut Site, scope: &Scope) {
        self.preview_scope(
            site,
            scope,
            &Draft {
                pick: Some(1),
                ..Draft::default()
            },
        );
    }

    /// **整批拒绝**：记成「我看过了，认不出」，这些条退出队列、不再问第二遍。
    pub fn reject(&mut self, site: &mut Site, scope: &Scope) {
        self.preview_scope(
            site,
            scope,
            &Draft {
                unknown: true,
                ..Draft::default()
            },
        );
    }

    /// 给一个范围排一次计划。**整批操作按下去走的就是它。**
    fn preview_scope(&mut self, site: &mut Site, scope: &Scope, draft: &Draft) {
        self.applied = None;
        match draft.build(&site.library).and_then(|decide| {
            self.queue
                .plan_scope(&site.catalog, &site.store, &decide, scope)
                .map_err(|error| format!("排不出计划：{error}"))
        }) {
            Ok(plan) => {
                self.error = None;
                self.pending = Some(self.hold(plan));
            }
            Err(message) => self.error = Some(message),
        }
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
                self.pending = Some(self.hold(plan));
            }
            Err(message) => self.error = Some(message),
        }
    }

    /// 把刚排出来的计划挂起来，**记下它是照着哪一版队列排的**。
    fn hold(&self, plan: Plan) -> Pending {
        Pending {
            plan,
            revision: self.queue.revision(),
        }
    }

    /// 队列变了样就把还没落下的那份计划**作废**。
    ///
    /// 计划书上那几行说的是**排它那一刻**队列里的那一批条目；逐条流里裁掉过其中一条、
    /// 换过一套选择器、撤回过一批之后，它描述的已经不是屏上这一批了
    /// （[`Queue::revision`] 三样都会变）。
    ///
    /// **作废而不是照着新的重排**：重排出来的是另一份承诺，而人点「落下」点的是他看过
    /// 的那一份（ADR-0016：先出计划再动手）。
    ///
    /// 这一道门与核心库那一道**各管各的**：核心库那道拦的是「发生了也不能两边各说各的」
    /// （`triage::apply` 整份拒掉，命令行与日后别的壳照样归它管）；这一道拦的是
    /// **别让人走到那一步**——过期的计划书留在屏上，人按下去才知道白按了。
    fn drop_stale_plan(&mut self) {
        let revision = self.queue.revision();
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.revision != revision)
        {
            self.pending = None;
            self.error = Some(
                "队列在排完计划之后变过样，那份计划书说的已经不是眼下这一批了。\
                 两份库一个字都没动——重排一份计划再落下。"
                    .to_string(),
            );
        }
    }

    /// **差量预览**：这一趟会改什么，看过了才落得下去。
    fn plan_modal(&mut self, ctx: &egui::Context, site: &mut Site) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        let plan = pending.plan;
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
            self.pending = Some(Pending {
                plan,
                revision: pending.revision,
            });
        }
    }

    /// 落下等着的那份计划。**模态框里「落下」按下去走的就是它。**
    pub fn commit(&mut self, site: &mut Site) {
        if let Some(pending) = self.pending.take() {
            self.apply_plan(site, &pending.plan);
        }
    }

    /// 真的落下：写沉淀库、当场在中立库里兑现、把裁完的从队列里去掉。
    fn apply_plan(&mut self, site: &mut Site, plan: &Plan) {
        match self.queue.apply(&mut site.catalog, &mut site.store, plan) {
            Ok(applied) => {
                self.error = None;
                self.applied = Some(applied);
                self.undone = None;
                self.changed = true;
                self.cursor = None;
            }
            // 计划过期那一句核心库已经说全了（连「两份库一个字都没动」都在里面），
            // 再前缀一句「写不进去」反而让人以为是库出了毛病。
            Err(error @ TriageError::StalePlan { .. }) => self.error = Some(error.to_string()),
            Err(error) => self.error = Some(format!("裁决写不进去：{error}")),
        }
    }

    /// **撤回刚落下的那一批**：两边一起回到它落下之前，队列当场重列。
    ///
    /// 领域判断一条都不在这里——[`Queue::undo`] 走的是命令行 `romcat triage undo --batch`
    /// 那条同一条路（ADR-0005）。这一层只负责把按下去的那一下转过去，再把账画出来。
    ///
    /// 没有可撤的那一批时**报一句**再回来。屏上那个「撤回第 N 批」的按钮只在有批的时候
    /// 才画得出来，所以走到这一支的一定是逐条流里按下的 `U`；一声不吭地返回，人只会
    /// 以为键盘坏了（`Y` 那一支写着同一句话，命令行 `undo --last` 无批时也报错退 1）。
    pub fn undo_last(&mut self, site: &mut Site) {
        let Some(batch) = self.applied.map(|applied| applied.batch) else {
            self.error = Some(
                "这一趟还没落下过一批裁决，`U` 没什么可撤的——先 `Y` 采用或 `N` 拒绝一条。\
                 更早落下的那些在命令行上撤：`romcat triage batches` 看有哪几批，\
                 `romcat triage undo --batch <号>` 撤其中一批。"
                    .to_string(),
            );
            return;
        };
        match self
            .queue
            .undo(&mut site.catalog, &mut site.store, &site.library, batch)
        {
            Ok(account) => {
                self.error = None;
                self.applied = None;
                self.undone = Some(account);
                self.changed = true;
                self.cursor = None;
                self.queue.set_filter(self.picks.filter());
            }
            Err(error) => self.error = Some(format!("撤不掉：{error}")),
        }
    }

    /// **把刚撤掉的那一批放回去**。与 [`Screen::undo_last`] 对称，走的也是核心库那条路。
    pub fn redo_last(&mut self, site: &mut Site) {
        let Some(batch) = self.undone.map(|undone| undone.batch) else {
            return;
        };
        match self
            .queue
            .redo(&mut site.catalog, &mut site.store, &site.library, batch)
        {
            Ok(account) => {
                self.error = None;
                self.undone = None;
                self.applied = Some(account);
                self.changed = true;
                self.cursor = None;
                self.queue.set_filter(self.picks.filter());
            }
            Err(error) => self.error = Some(format!("放不回去：{error}")),
        }
    }

    /// 逐条流眼下停在第几条（选中的那些里数）。
    #[must_use]
    pub fn at(&self) -> usize {
        self.at
    }

    /// 逐条流眼下切到第几条候选（从 0 数）。
    #[must_use]
    pub fn nth(&self) -> usize {
        self.nth
    }

    /// 把**光标**重新对到下标上。
    ///
    /// **记的是键，不是下标**：换个选择器表就重排了，下标会指到别人身上。对得上就直接
    /// 用，对不上才扫一遍。
    ///
    /// 扫不着说明它**被裁掉了或者被这一次筛选筛掉了**——那时光标**停在同一个位置上**，
    /// 认下滑进来的那一条。逐条流靠的正是这一条：裁完一条，下一条自己就落到光标底下，
    /// 手不必动。同时它也是那次 O(n) 扫描的出口：不认下新的一条，下一帧还要为一个
    /// 已经不在表里的键把一万八千条重扫一遍。
    fn resolve_cursor(&mut self) {
        let items = self.queue.selected();
        if items.is_empty() {
            self.at = 0;
            self.cursor = None;
            return;
        }
        if let Some(key) = &self.cursor {
            if items.get(self.at).map(|item| item.variant.key.as_str()) == Some(key.as_str()) {
                return;
            }
            if let Some(at) = items.iter().position(|item| item.variant.key == *key) {
                self.at = at;
                return;
            }
        }
        self.at = self.at.min(items.len() - 1);
        self.cursor = Some(items[self.at].variant.key.clone());
    }

    /// 把光标挪到第 `at` 条上（越界就停在最后一条）。
    fn move_to(&mut self, at: usize) {
        let items = self.queue.selected();
        if items.is_empty() {
            self.at = 0;
            self.cursor = None;
            return;
        }
        self.at = at.min(items.len() - 1);
        self.cursor = Some(items[self.at].variant.key.clone());
        self.nth = 0;
    }
}

impl Default for Screen {
    fn default() -> Self {
        Self::new()
    }
}

/// 排出来还没落下的那份计划，连**它是照着哪一版队列排的**。
///
/// 两样收在一处，因为「这份计划还作不作数」只有它们凑齐了才答得上来
/// （[`Screen::drop_stale_plan`]）——与 [`Basis`] 缓那三样是同一个道理。
#[derive(Debug, Clone)]
struct Pending {
    /// 计划本身。屏上那张计划书画的就是它。
    plan: Plan,
    /// 排它的时候队列是第几版（[`Queue::revision`]）：裁完一批、撤回一批、换个选择器
    /// 它都会变，而那三样每一样都让这份计划书不再描述屏上这一批。
    revision: u64,
}

/// 展开那一批算出来的三样是**照着什么**算的。四样凑齐才认得出「这一份还作数吗」。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Basis {
    /// 队列换过几次样子（[`Queue::revision`]）：裁完一批、撤回一批、换个选择器它都会变。
    revision: u64,
    /// 作用范围：哪一批，下钻到哪一组。
    scope: Scope,
    /// 二级用哪个轴分。
    axis: Axis,
    /// 样本是第几组。
    seed: u64,
}

/// 展开那一批算出来的三样：条数、二级分组、随机样本。
///
/// **三样一起缓**，因为它们出自同一次「走一遍这一批」；分开缓的话，谁先谁后失效
/// 会让屏上那三样各说各的——「整批通过 3,053 条」底下摆的是另一批的样本。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Opened {
    /// 这一份是照着什么算出来的。
    basis: Basis,
    /// 作用范围里有多少条。
    count: u64,
    /// **整批**在这个轴上的二级分组。下钻只是把整批操作收窄，这张表照旧数整批。
    drill: Drill,
    /// 作用范围里的随机样本。
    samples: Vec<Sample>,
}

/// **一堆来自同一次匹配的字段**：条目号、依据、那几个值，连底下那两颗按钮。
///
/// 返回「这一帧按下了哪一堆的哪一档」——`(条目号, 就是这条吗)`。
///
/// 收成自由函数是因为借用：这一块要**读**缓着的那几堆（`&self.matches`），而按下去
/// 之后要**改**两份库；一个 `&mut self` 上过不去，也不该为了过去而把那几堆整份克隆
/// 一遍（一条简介 4,000 字，每帧一份）。
fn match_groups_ui(ui: &mut egui::Ui, groups: &[MatchGroup]) -> Option<(u32, bool)> {
    let mut 按下 = None;
    for group in groups {
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            // **条目号写在堆上**：它就是「同一次匹配」的判据，人要去数据源核对时，
            // 那也是唯一查得回去的东西。
            ui.strong(format!("条目 {}", group.entry));
            if group.confirmed {
                ui.label("已由人裁决确认");
            } else {
                // **口径不放松**：模糊匹配来的仍是中置信、仍进待确认队列（ADR-0002）。
                ui.label("还等着裁：模糊匹配来的，中置信，不自动通过");
            }
            ui.weak(format!("{} 个字段，一条裁决全管", group.values.len()));
        });
        for value in &group.values {
            // **锚点那一层写出来**：同一堆里变体那几条与作品那几条，下一趟重跑时的
            // 去向完全不同（作品那一层按名下变体数票，见 `zh::judge` 的文档）。
            let 落在 = match value.kind {
                AnchorKind::Variant => "变体".to_string(),
                AnchorKind::Work => format!("作品「{}」", value.subject),
            };
            let 值 = one_line(&value.value);
            ui.label(format!(
                "{落在} · {}｜{} = {}\n    依据：{}",
                value.source,
                value.field.label(),
                值.as_deref().unwrap_or(value.value.as_str()),
                value.evidence,
            ))
            .on_hover_text(&value.value);
        }
        if group.from_variant {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("就是这条")
                    .on_hover_text(
                        "这一次匹配带来的全部字段一并定下：下一趟刮削把它们的依据改写成\
                         「由人工裁决确认过」，不再进待确认队列。一个字都不清。",
                    )
                    .clicked()
                {
                    按下 = Some((group.entry, true));
                }
                if ui
                    .button("不是这条")
                    .on_hover_text(
                        "这一次匹配带来的全部字段一并失效，就地清掉——错的东西不该在库里\
                         多躺一秒。这个变体重跑刮削也不会再撞回这条条目。",
                    )
                    .clicked()
                {
                    按下 = Some((group.entry, false));
                }
            });
        } else {
            // **裁决钉在内容上**：这一堆全在作品锚点上，撞它的是名下别的变体。钉在
            // 这个变体身上管不到那一层——下一趟那些变体照旧投它们的票（`zh::judge`）。
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "⚠️ 这一堆全在作品那一层：撞它的是名下别的变体，不是这一个。\
                 裁它要去裁那个变体。",
            );
        }
    }
    按下
}

/// 落下一条**匹配裁决**之后那一句账。
///
/// 命令行 `romcat zh judge` 印的是同一本账（`Judged` 那几栏一处说了算）：钉在什么上、
/// 是不是改主意、清掉了几条、作品那一层会不会自己回来。**一句都不许省**——省掉的每
/// 一句都是一次「按下去之后什么都没发生」。
fn judged_text(judged: &MatchJudged) -> String {
    let 账 = &judged.judged;
    let mut text = format!(
        "条目 {} {}。锚是{}——{}",
        judged.entry,
        if judged.accepted {
            "就是这条"
        } else {
            "不是这条"
        },
        账.anchor.label(),
        账.anchor.describe(),
    );
    if !账.anchor.is_shareable() {
        text.push_str(
            "。⚠️ 这一条只在本机成立：拿不到内容判据，退到了路径锚——改个名字、\
             换台机器就认不出了",
        );
    }
    if !账.fresh {
        text.push_str("。（这条锚上本来就裁过，这次是改主意——覆盖掉了老的那一条。）");
    }
    if !账.from_variant {
        text.push_str(
            "。⚠️ 这个变体自己撞的不是这条条目：裁决记下了，但它钉在这个变体的内容上，\
             作品那一层若是名下别的变体撞出来的，下一趟刮削它们照旧投回来",
        );
    }
    if judged.accepted {
        text.push_str(
            "。这一次匹配带来的字段一并定下，一个字都不清。\
             屏上那一堆仍写着「还等着裁」——那句话在依据里，下一趟 `romcat scrape` 才改写",
        );
    } else {
        let _ = write!(
            text,
            "。就地清掉了 {} 条字段值{}",
            thousands(账.cleared),
            match (&账.work, 账.cleared_work) {
                // **动过才说动过**：作品那一层一个字没动时，这半句一个字都不该印。
                (Some(work), n) if n > 0 =>
                    format!("（其中作品「{work}」那一层 {} 条）", thousands(n)),
                _ => String::new(),
            },
        );
        if 账.cleared_work > 0 {
            text.push_str(
                "。⚠️ 作品那一层可能自己回来：那几栏按名下变体数票定，\
                 名下还有别的变体撞着这条条目的话，下一趟它们照样投这一票——而那时它是对的",
            );
        }
    }
    // **走回来那一下要说清楚**：按批撤销（[`Screen::undo_last`]）管的是**识别**那一批
    // 裁决，匹配裁决不在那条路上——改主意的路是再裁一次，同一条锚上后一条盖掉前一条。
    text.push_str(
        "。改主意就再裁一次：这一条不在「撤回第 N 批」那条路上（那条管的是识别那一批），\
         同一条锚上后一条盖掉前一条",
    );
    // **「采得回来」有前提，说全它**。否定之后重跑刮削照旧不会撞回这条条目
    // （那条裁决进了输入指纹，`ChineseSource::hit` 当场跳过它）——值要回来，
    // 得先把这一条改判成「就是这条」。半句话会与那颗按钮的说明当场打架。
    if judged.accepted {
        text.push('。');
    } else {
        text.push_str(
            "；清掉的那些值要回来，得先改判成「就是这条」，\
             再跑一趟 `romcat scrape`。",
        );
    }
    text
}

/// 光标底下那个变体身上那几堆，连**它是照着什么读出来的**。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Matched {
    /// 哪个变体。
    key: String,
    /// 读它的时候队列是第几版（[`Queue::revision`]）。
    revision: u64,
    /// 按**条目号**归好的那几堆。**归堆在核心库**（[`matched_groups`]）。
    groups: Vec<MatchGroup>,
}

/// 上一次落下的那条**匹配裁决**：裁的是谁的哪一条，连核心库交回来的那本账。
#[derive(Debug, Clone, PartialEq, Eq)]
struct MatchJudged {
    /// 裁的是哪个变体。**账只画在它自己那一条上**——光标挪走之后还挂着，
    /// 那句话说的就是另一个变体了。
    ///
    /// 它跟着「重新列队列」一起放掉（[`Screen::reload`]），别的路不清：光标绕一圈
    /// 回到同一个变体，那句账原样回来。那句话仍旧是真的（这条裁决确实落下过），
    /// 所以留着比抹掉好。
    key: String,
    /// 哪一次匹配（条目号）。
    entry: u32,
    /// 说的是「就是这条」还是「不是这条」。
    accepted: bool,
    /// 核心库交回来的账。
    judged: Judged,
}

/// 屏底那句「前几批盖住多少」。
///
/// 这句话是这一屏存在的理由本身：18,241 条按 5 秒一条是 25 小时，而**前几批就能清掉
/// 大半**。**数是核心库算的**（[`romcat_core::triage::batch::coverage`]），这里只把它们
/// 摆成一句话——界面只画和转发（ADR-0005）。
fn headline(账: &Coverage) -> String {
    if 账.total == 0 {
        return "队列是空的。".to_string();
    }
    format!(
        "前 {} 批盖住 {} 条（{:.0}%）。其中按批答得了的（只有一个候选）{} 条；\
         剩下的多候选与一条候选都没有的走逐条。",
        账.head_batches,
        thousands(账.head),
        账.share(),
        thousands(账.answerable),
    )
}

/// 界面上那份**选择器**草稿。
///
/// 它与 [`Filter`] 的关系是「界面上的样子」与「领域里的样子」：三个轴各一个文本框、
/// 四档结论各一个勾，外加**一级分批**点下去那一批的形状。
/// **折算只有一条路**（[`Picks::filter`]），分组表点一下走的也是它。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picks {
    under: String,
    name: String,
    candidate_work: String,
    /// 四档结论要不要，与 [`State::ALL`] 同序。
    states: [bool; State::ALL.len()],
    /// **按依据形状**：一级分批点「逐条看」时填进来。
    ///
    /// 它**没有文本框**——「MAME / gameboy.xml / 中置信 / 含头 / 只有一个候选」不是人
    /// 打得出来的东西，屏上它只能从卡片上点。命令行那一侧收得下同一批
    /// （`romcat triage --shape`），但走的也不是手打：报告把每一批连
    /// [`Shape::selector`] 折出来的那串字一起印出来，人**照着抄**（票
    /// `queue-followups/08`）。
    shape: Option<Shape>,
}

impl Default for Picks {
    fn default() -> Self {
        Self {
            under: String::new(),
            name: String::new(),
            candidate_work: String::new(),
            // 默认那三档：**跳过**不在队列里——它不是「拿不定主意」，是「不该撞 DAT」。
            states: [true, true, true, false],
            shape: None,
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
            shape: self.shape.clone().into_iter().collect(),
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

    /// 三个轴全清掉，结论那四个勾与一级那一批不动。
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

/// 撤回之后那一句账。
///
/// **「中立库那一半回没回去」必须说出口**：回去了，那些变体当场就在队列里；没回去
/// （快照随重跑识别清掉了），人得再跑一趟识别才看得见——两种情形说同一句话是撒谎。
fn undone_text(undone: &Undone) -> String {
    let mut text = format!(
        "第 {} 批已撤回 {} 条（其中 {} 条把它盖掉的那条旧裁决放了回去）",
        undone.batch,
        thousands(undone.removed),
        thousands(undone.restored),
    );
    if undone.kept > 0 {
        let _ = write!(
            text,
            "；另有 {} 条没动——同一条锚上后来有人重新裁过",
            thousands(undone.kept)
        );
    }
    if undone.catalog_rolled_back {
        let _ = write!(
            text,
            "。中立库那一半也回去了（{} 个变体），它们已经回到队列里，不必重跑识别。",
            thousands(undone.variants),
        );
    } else {
        text.push_str(
            "。⚠️ 这一批之后跑过识别（或者中立库重建过），中立库那一半的快照已经清掉了\
             ——沉淀库这一半撤干净了，要让它们回到队列请再跑一趟识别。",
        );
    }
    text
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
    fn 四档各画各的颜色不串() {
        // 「置信度用颜色标出，四档含义与别处一致」（票 `gui-redesign/09` 验收第 6 条）。
        // 两档画成同一个色，那句话就是假的。
        let visuals = egui::Visuals::dark();
        let 颜色: Vec<egui::Color32> = Tier::ALL
            .into_iter()
            .map(|tier| look::tier_color(tier, &visuals))
            .collect();
        for (at, one) in 颜色.iter().enumerate() {
            for (other_at, other) in 颜色.iter().enumerate() {
                assert!(
                    at == other_at || one != other,
                    "{} 与 {} 画成了同一个颜色",
                    Tier::ALL[at].label(),
                    Tier::ALL[other_at].label(),
                );
            }
        }
        // 四档的词与核心库同出一处——别处写「高置信」这里写「高」，用户会以为是两件事。
        // 第四档**不许叫「还没识别」**：那四个字归词表那条「连识别都还没跑过」
        // （`CONTEXT.md` 的**还没识别**条），而屏头上两句话是并排印的。
        assert_eq!(
            Tier::ALL.map(Tier::label).to_vec(),
            vec!["高置信", "中置信", "低置信", "没有候选"],
        );
        assert_ne!(Tier::Unidentified.label(), NOT_RUN_LABEL);
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
