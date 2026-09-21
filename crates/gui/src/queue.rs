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
//! 3. **随机样本**——可以换一组。三样齐了才敢按「全部通过（3,053 条）」。
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
//! 那是人按下去之前该看见的全部。照稿两栏（票 `gui-looks-like-the-design/18`）：左边待选列表，栏头收着「筛选…」与排序；
//! 右边这一条的详情，候选一张一张摆成卡片，四颗带键帽的按钮与键盘走的是同几条路。
//!
//! ## 批量的胆量来自撤销可信
//!
//! 「全部通过（3,053 条）」按错了要撤得干净，否则批量这件事本身不成立。撤销走的是
//! 票 `gui-redesign/08` 那条路（[`Queue::undo`]）：中立库与沉淀库两边一起回到这一批
//! 落下之前，**一个字节的 DAT 都不读**。逐条流里 `U` 撤的也是它——逐条落下的每一下
//! 自己就是一**批裁决**。
//!
//! 撤得掉的不只是刚落下的那一批：**裁决记录**（屏头右侧那颗按钮打开右边那块抽屉）从沉淀库列出落过的
//! 每一批裁决——什么时候落的、多少条、撤过没有——每一批旁边一颗撤销（撤过的是放回）。
//! 被后来还在册的一批盖住时核心库整份拒下，那句话原样画在那一块里（票
//! `gui-answers-all-six/06`）。**一批裁决**（[`verdict::Batch`]）与正文那一列卡片上的
//! **一批变体**（[`Batch`]）是词表**批**那一条分开的两件事，屏上两处措辞各说各的。
//!
//! ## 中文输入全在弹层与详情里
//!
//! 一个 [`egui::TextEdit`] 都不进待选列表的行里。列表是虚拟化的，正在组字的那一行一旦滚出
//! 视口，那个控件就不存在了，输入法上屏时会没人接（ADR-0005 的修订段）。手工指定那张表单、筛选那几个框
//! 各是一层弹层（[`dialog`]），匹配裁决那一格备注摆在详情里。
//! `tests/queue.rs::待选列表里画多少行文本输入框都是那几个` 钉住这一条。
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
use romcat_core::catalog::State;
use romcat_core::catalog::identify::{NOT_RUN_LABEL, Tier};
use romcat_core::dat::chinese::ChineseMark;
use romcat_core::report::{capacity, thousands};
use romcat_core::scrape::AnchorKind;
use romcat_core::scrape::zh::{Judged, MatchGroup, judge, matched_groups};
use romcat_core::stage::Stage;
use romcat_core::triage::batch::{Coverage, breakdown};
use romcat_core::triage::{
    Applied, Axis, Batch, Breakdown, Draft, Filter, ItemOrder, Overrides, PartKind, Parts, Plan,
    Queue, Sample, Scope, Shape, Slice, TriageError, Undone,
};
use romcat_core::verdict;

// **一行画得下的那一截**收在浏览屏那一处：一条简介在中立库里最多 4,000 字
// （`scrape::zh::DESCRIPTION_LIMIT`），两屏碰到的是同一个问题，各写一份迟早两种收法。
use crate::browse::one_line;
use crate::clock::{Clock, RecordClock};
use crate::dialog;
use crate::font;
use crate::layout;
use crate::look;
use crate::toast::{self, Toast};
use crate::tokens::Tokens;
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

/// 下钻到某一组之后，**就地那一框**里摆几条样本（设计稿 `drillHTML` 的 `Math.min(4, …)`）。
///
/// 比整批那一栏少一条：那一框是插在细分那一栏中间的，摆满五条就把底下那几项挤出屏外了。
const DRILL_SAMPLES: usize = 4;

/// 屏底那句「前几批盖住多少」按前几批算：与库屏工序段裁决那一行底下那句同一个数（核心库那一处）。
const HEADLINE: usize = romcat_core::triage::HEADLINE_BATCHES;

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
    /// 各批下面**已经就地裁完的那几部分**（[`Parts`]）。
    ///
    /// **它不是第二份账**：落了多少条、撤没撤都以沉淀库那一批裁决为准，这里存的是沉淀库
    /// 答不出的那一半——那一批裁决当初作用在哪一批的哪一组上。沉淀库只记落了哪些条，
    /// 折不回「按目录 · `GB/汉化/`」这一句，而屏上那一项要标着「已通过」正需要它。
    ///
    /// **裁决记录每重列一遍就跟着对一次**（[`Screen::refresh_records`]）：撤掉的那一批
    /// 对应的那一项当场作废，那一行重新点得动，细分方式也跟着解开。
    parts: Parts,
    /// 能整批通过的批**全都摆出来了没有**：默认只先摆前几批（与正文头一格说的是同几批），余下的收成一句，按「列出这 N 批」才摆。
    all_passable: bool,
    /// 样本那一组的号。**「换一组」就是把它加一**。
    seed: u64,
    /// 展开那一批算出来的东西：条数、细分那一栏、两处随机样本。
    ///
    /// **缓着而不是每帧重算**：这几样各要走一遍这一批的全部条目，一批一万两千条上，
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
    /// 屏底那条**提示条**：刚落下、刚撤掉或刚放回一批之后那一句，带一颗走回来的按钮（设计稿 `toast()`，[`Receipt`]）。
    receipt: Option<Receipt>,
    /// **裁决记录**：这份主库上落过的每一批裁决，新的在前，**从沉淀库列**。
    ///
    /// 从前这一屏只记得「本次进程里刚落下的那一批」（`applied` 那一格，一个可空的位置）
    /// ——窗口一关、或者那一批是命令行落的，屏上就一个字都没有，而沉淀库里每一批都还
    /// 躺着，撤销那个入口也本来就吃任意一批（[`Queue::undo`]）。
    ///
    /// **沉淀库每动一次重列一遍**（落下、撤销、放回、重新列队列），不是每帧一句查询。
    ///
    /// ⚠️ 装的是 [`verdict::Batch`]——**一批裁决**，不是 [`Batch`]（一级分批的**一批变体**）。
    /// 词表**批**那一条把两件事分开了，代码里两层各有一个同名的类型，这里永远带着模块名写。
    records: Vec<verdict::Batch>,
    /// 裁决记录那一块开着没有。
    records_open: bool,
    /// 「筛选…」那一层弹层开着没有。
    filter_open: bool,
    /// 「手工指定…」那一层弹层开着没有。
    manual_open: bool,
    /// 裁决记录里的时刻怎么画（[`RecordClock`]）：截图测试钉死「此刻」（[`Screen::set_clock`]）与落批时刻（[`Screen::pin_record_time`]）。
    record_clock: RecordClock,
    /// 裁决记录里那两颗按钮上一次的回话：撤不掉、放不回去时核心库那句话。
    ///
    /// 与 `error` 分开存，是因为两处**画在不同的地方、清在不同的时刻**：那句话要画在按下去
    /// 的那一块里，而整屏别处的错（列队列失败、`U` 无可撤）不该跑进裁决记录。
    records_refusal: Option<String>,
    /// 屏头那句「另有 N 个变体连识别都还没跑过」的 N。
    ///
    /// **数只有一处算**：[`Catalog::not_run_count`](romcat_core::catalog::Catalog::not_run_count)
    /// ——列队列、库屏工序段识别那一行、命令行队列报告取的都是它，这里只缓着读出来的数。
    /// 缓在这一屏而不只靠 [`Queue::not_run`]，是因为它过期的时机与队列不一样：扫完一个根
    /// 它就过期了，而队列本身一条都没变——为一个数把整份队列重列一遍，真库上是一万八千条
    /// （挂单 `Q419`）。所以队列列一次它跟着换，扫完一个根只重算它（[`Screen::recount_not_run`]）。
    not_run: u64,
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
    /// 人在这一屏按下的那道**工序**的捷径，等窗口取走。
    ///
    /// **这一屏排不了活**：排一趟工序要同时够得着库屏那一段与**任务台**，而屏与屏
    /// 之间不该互相拿着对方（ADR-0005）。所以按下去只留一个记号，由
    /// [`App::route`](crate::app::App::route) 取走、交给
    /// [`App::start_stage`](crate::app::App::start_stage)——与子库屏那几个跳转记号
    /// 同一个办法。这么一来捷径排的就是**与库屏工序段那一行完全同一趟活**：
    /// 同一个函数、同一趟任务、同一份产物（票 `gui-self-sufficient/09`）。
    asked: Option<Stage>,
    /// 把表格的滚动位置强按到这个像素偏移。**只有量帧率时才设**（[`crate::bench`]），
    /// 真界面上永远是 `None`。
    pub scroll_to: Option<f32>,
}

/// 那颗**就地跑识别**的捷径（设计稿「运行识别」）。按下去返回 `true`。`primary` 为真时是空态卡上那颗默认大小的主按钮，
/// 否则是提示条里那颗小号的。
///
/// 队列屏上一共画三处：一级分批那张空态、逐条那张空态，以及正文三格上方「另有 N 个变体连识别都还没跑过」那一条提示
/// （[`Screen::not_run_note`]）。三处从前写的都是「先跑一次 `romcat identify`」——人是在这一屏发现「没识别」的，让他跑回
/// 库屏是多余的一步（规格 46）。**三处画的是同一颗**：从前三处也是同一句话，换成几份写法迟早各说各的。
///
/// **它自己不排活**：按下去只让调用方留一个记号（[`Screen::asked`]），
/// 排的是与库屏工序段那一行完全同一趟。
fn identify_shortcut(ui: &mut egui::Ui, primary: bool) -> bool {
    let 按钮 = |ui: &mut egui::Ui| {
        ui.scope(|ui| {
            if primary {
                look::primary_button(ui.visuals_mut());
            }
            ui.button("运行识别").on_hover_text(
                "排到任务台上跑，期间照常用别的屏；按得停。\
                 与库屏上「识别」那一行是同一趟——跑完这一屏自己重新列过。",
            )
        })
        .inner
        .clicked()
    };
    if primary {
        look::buttons(ui, 按钮)
    } else {
        look::small_buttons(ui, 按钮)
    }
}

/// 这一屏**没有待确认项**时那张空态卡（设计稿 `#q-empty`）：一句标题、一句说明、一颗「运行识别」、底下一句帮助字。
/// 按下去返回 `true`。
///
/// 一级分批与逐条那张表**各有一张，画的是同一张**——两处各写一份的话，改一处就漏一处。标题照词表写「还没识别」
/// （拿主意的人 2026-09-14 定），常规体不加粗；库里一个还没识别的变体都没有时（识别跑过、队列裁空了）标题只写
/// 「暂无待确认项」，那颗按钮与底下那句也不画——没有可补的，按下去什么都不会多出来。
fn identify_empty_state(ui: &mut egui::Ui, not_run: u64, identified: bool) -> bool {
    let tokens = Tokens::builtin();
    let palette = look::palette(ui);
    let [标题后, 说明后, 按钮后] = tokens.space.empty_state_gaps;
    let mut 要跑 = false;
    ui.vertical_centered(|ui| {
        ui.add_space(tokens.space.empty_state_margin);
        let 宽 = tokens.layout.empty_state_width.min(ui.available_width());
        ui.allocate_ui(egui::vec2(宽, 0.0), |ui| {
            look::card(
                ui,
                egui::Vec2::splat(tokens.space.empty_state_padding),
                |ui| {
                    ui.vertical_centered(|ui| {
                        let 标题 = if not_run > 0 {
                            format!("暂无待确认项：{} 个变体还没识别", thousands(not_run))
                        } else {
                            "暂无待确认项".to_owned()
                        };
                        ui.label(
                            egui::RichText::new(标题)
                                .size(tokens.font.size_empty_title)
                                .color(palette.ink),
                        );
                        ui.add_space(标题后);
                        ui.label(
                            egui::RichText::new(
                                "识别完成后，无法自动确定的结果会出现在这里，并按判定依据自动分批。",
                            )
                            .color(palette.ink_2),
                        );
                        if !identified || not_run > 0 {
                            ui.add_space(说明后);
                            要跑 = identify_shortcut(ui, true);
                            ui.add_space(按钮后);
                            look::help(ui, "与「库」页面工序中的「识别」是同一个操作。");
                        }
                    });
                },
            );
        });
    });
    要跑
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
            parts: Parts::default(),
            all_passable: false,
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
            receipt: None,
            records: Vec::new(),
            records_open: false,
            filter_open: false,
            manual_open: false,
            record_clock: RecordClock::default(),
            records_refusal: None,
            not_run: 0,
            matches: None,
            judged: None,
            match_note: String::new(),
            error: None,
            changed: false,
            only_picked: false,
            asked: None,
            scroll_to: None,
        }
    }

    /// 换一个画时刻用的钟（[`Clock`]）：截图测试钉死此刻与偏移，截图里才没有当前时间。
    pub fn set_clock(&mut self, clock: Clock) {
        self.record_clock.clock = clock;
    }

    /// 截图测试用：裁决记录里每一批落下（撤过的撤下）的时刻一律画成 `at`（UNIX 纪元起的秒）。
    ///
    /// 那一刻是核心库落批时照挂钟记下的，截图里照实画的话一趟一个样——与任务屏定死收场时刻（`App::pin_task_clock`）
    /// 同一个用处。真窗口那一路不调它。
    pub fn pin_record_time(&mut self, at: i64) {
        self.record_clock.pinned = Some(at);
    }

    /// 打开「手工指定…」那一层弹层。界面上点那颗按钮走的就是它，实测与测试拿它当那一下。
    pub fn open_manual(&mut self) {
        self.manual_open = true;
    }

    /// 打开「筛选…」那一层弹层。界面上点待选列表栏头那颗按钮走的就是它。
    pub fn open_filter(&mut self) {
        self.filter_open = true;
    }

    /// 取走「**刚动过中立库**」那个记号。窗口每帧问一次，问到就转告浏览屏
    /// （`crate::app::App::route`）。
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// 按下这一屏上那颗**运行识别**的捷径。界面上点那一下走的就是它，实测与测试拿它当
    /// 那一下。
    ///
    /// **它不在这儿排活**，只留一个记号（`asked` 那一格）——排的是与库屏工序段
    /// 那一行完全同一趟。
    ///
    /// **按下去的回音不由这一屏出**，与浏览屏那颗（`browse::Screen::ask_fold_titles`
    /// 当场换掉自己那句回执）不一样：底部状态栏上任务台那一截哪一屏上都看得见
    /// （`App::top_bar`），而这一屏的空态本来就会在识别跑完那一刻自己变成队列
    /// ——为它另存一句回执，等于让同一件事在屏上有两个说法。
    pub fn ask_identify(&mut self) {
        self.asked = Some(Stage::Identify);
    }

    /// 把上面那一下取走。**取过就没了**：窗口一帧问一次
    /// （[`App::route`](crate::app::App::route)）。
    pub fn take_asked(&mut self) -> Option<Stage> {
        self.asked.take()
    }

    /// 队列本身，供测试查「列出多少条、分成几批」。
    #[must_use]
    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    /// 屏头那句「另有 N 个变体连识别都还没跑过」眼下说的 N。
    #[must_use]
    pub fn not_run(&self) -> u64 {
        self.not_run
    }

    /// **只重算屏头那个数，不重列整份队列**：扫完一个根之后窗口转告的就是它
    /// （[`App::poll_tasks`](crate::app::App::poll_tasks)）。
    ///
    /// 扫描不改变队列本身——新扫进来的变体连结论都还没有，进不了队列——变的只有「还没识别的
    /// 有几个」这一个数。取的是
    /// [`Catalog::not_run_count`](romcat_core::catalog::Catalog::not_run_count) 那一句 SQL，
    /// 与列队列、库屏工序段、命令行队列报告同一处，不另造一份。
    pub fn recount_not_run(&mut self, site: &Site) {
        match site.catalog.not_run_count() {
            Ok(count) => self.not_run = count,
            Err(error) => self.error = Some(format!("中立库读不动：{error}")),
        }
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
        self.picks.only = None;
        self.picks.clear_axes();
        self.refresh_opened();
    }

    /// 换到**逐条键盘流**。眼下展开着哪一批（下钻到哪一组），就只看那一批，
    /// 而且**从头看起**。
    ///
    /// 下钻那一层到这里才折进选择器：逐条要的正是「把队列收窄到这一组」，
    /// 而在分批那一屏上收窄整个队列是错的（见 `Screen::drill` 那个字段）。
    ///
    /// 光标一并放掉：不放的话它还钉在批优先那一屏上停过的某一条上，
    /// `Screen::resolve_cursor` 会把它捞回来，于是「逐条看」是从中间开始的。
    pub fn show_one_by_one(&mut self) {
        self.mode = Mode::OneByOne;
        self.picks.only = self.open.clone().map(Only::Batch);
        self.picks.clear_axes();
        if let Some(label) = self.drill.clone() {
            self.picks.pick(self.axis, &label);
        }
        self.cursor = None;
        self.at = 0;
        self.nth = 0;
    }

    /// 换到**逐条键盘流**，只看**有多个候选**的那几批：屏头「逐条」走的就是它（设计稿 `.obolist` 栏头「有多个候选」）。
    ///
    /// 那几批照整个队列眼下的一级分批取，哪几档算「有多个候选」由核心库说
    /// （[`Fanout::multiple`](romcat_core::triage::Fanout::multiple)）；队列里一条有多个候选的都没有时看整个队列。
    /// 展开着的那一批与下钻一并收起，光标从头看起。
    pub fn show_multiple(&mut self) {
        self.open = None;
        self.drill = None;
        self.picks.only = None;
        self.picks.clear_axes();
        self.queue.set_filter(self.picks.filter());
        let 几批: Vec<Shape> = self
            .queue
            .batches()
            .iter()
            .filter(|batch| batch.shape.fanout().multiple())
            .map(|batch| batch.shape.clone())
            .collect();
        self.picks.only = (!几批.is_empty()).then_some(Only::Multiple(几批));
        self.mode = Mode::OneByOne;
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
        self.axis = self.locked_or_default();
        self.drill = None;
        self.seed = 0;
        self.refresh_opened();
    }

    /// 展开着那一批锁在哪个轴上；没锁就是默认的**按目录**。
    ///
    /// **打开就停在锁着的那个轴上**：处理过一部分的那一批退回按目录的话，那一排上选中的
    /// 与底下真画的不是同一个轴，而另外两颗还按不动——屏上自相矛盾。
    fn locked_or_default(&self) -> Axis {
        self.open
            .as_ref()
            .and_then(|shape| self.parts.locked_axis(shape))
            .unwrap_or(Axis::Directory)
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
    ///
    /// **已经按一个轴裁掉一部分之后换不动**：两套切法会重叠，新那一栏里每一项含着多少条
    /// 已经裁掉的，谁也说不出——屏上的数当场变成假话。锁在哪个轴上由核心库说
    /// （[`Parts::locked_axis`]）。先在裁决记录里撤掉那几批就解开了。
    ///
    /// **这道守卫拒下时要说得出理由**，不是光秃地返回：那一排上另两颗虽然也画成了按不动的，
    /// 但拒绝的那一句得在这儿——有人绕过界面直接调它时也得听见原因（ADR-0005「再修订：
    /// 『不禁按钮』那一条什么时候允许同时画灰」，拿主意的人 2026-09-20 定）。
    /// 那句话与屏上那一排底下常驻的那一行**是同一句**（核心库的
    /// [`axis_refusal`](romcat_core::triage::axis_refusal)）。
    ///
    /// 交回**换成了没有**。换轴的入口只许有这一个（ADR-0005 那一节要求「真正挡住的是那个
    /// 唯一的入口」），别处要换轴一律走它、照它的回话决定后面那半步做不做
    /// （[`Screen::pick`]）。
    pub fn set_axis(&mut self, axis: Axis) -> bool {
        if let Some(shape) = &self.open
            && let Some(锁) = self.parts.locked_axis(shape)
            && 锁 != axis
        {
            self.error = Some(romcat_core::triage::axis_refusal(锁));
            return false;
        }
        // 换成了，上一次拒绝那句话跟着作废——留着的话屏上会一直挂着一条早就不成立的理由。
        self.error = None;
        self.axis = axis;
        self.drill = None;
        self.seed = 0;
        self.refresh_opened();
        true
    }

    /// **下钻**到二级的某一组；再点一次返回整批。
    pub fn drill_into(&mut self, label: &str) {
        self.drill = if self.drill.as_deref() == Some(label) {
            None
        } else {
            Some(label.to_string())
        };
        self.seed = 0;
        self.refresh_opened();
    }

    /// 返回整批：把下钻那一层放掉。屏上那颗「返回整批」走的就是它。
    pub fn drill_out(&mut self) {
        self.drill = None;
        self.seed = 0;
        self.refresh_opened();
    }

    /// **换一组样本**（界面上那颗「换一组」）。
    pub fn resample(&mut self) {
        self.seed = self.seed.wrapping_add(1);
        self.refresh_opened();
    }

    /// 眼下这一批屏上摆着的那几条样本。**数的始终是整批**（照稿，见 [`samples_column`]）。
    #[must_use]
    pub fn samples(&self) -> Vec<Sample> {
        self.opened
            .as_ref()
            .map(|opened| opened.samples.clone())
            .unwrap_or_default()
    }

    /// 下钻着的那一组屏上摆着的那几条样本——**就地那一框**里的（[`drill_box`]）；没下钻时是空的。
    #[must_use]
    pub fn drilled_samples(&self) -> Vec<Sample> {
        self.opened
            .as_ref()
            .map(|opened| opened.drilled_samples.clone())
            .unwrap_or_default()
    }

    /// 二级眼下按哪个轴分。锁住的时候它就是锁着的那个轴（[`Parts::locked_axis`]）。
    #[must_use]
    pub fn axis(&self) -> Axis {
        self.axis
    }

    /// 展开那一批的**细分**：各项、占比、哪几项已经就地裁完、还剩多少条、锁没锁住轴。
    /// 一批都没展开时是 `None`。
    #[must_use]
    pub fn breakdown(&self) -> Option<&Breakdown> {
        self.opened.as_ref().map(|opened| &opened.breakdown)
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
            parts: self.parts.revision(),
        };
        if self.opened.as_ref().map(|opened| &opened.basis) == Some(&basis) {
            return;
        }
        // 二级那张表数的是**整批**：下钻只是把整批操作收窄，那张表本身不该跟着只剩一行
        // ——不然下钻一次就再也回不去了，屏上看不见还有哪些组。
        let whole = Scope::whole(basis.scope.shape.clone());
        let drilled = self.queue.drill(&whole, basis.axis);
        let breakdown = breakdown(&drilled, &self.parts, &basis.scope.shape, basis.axis);
        // 下钻着的那一组的样本只在下钻之后要——就地那一框里摆的是它。
        let drilled_samples = if basis.scope.drill.is_some() {
            self.queue.sample(&basis.scope, basis.seed, DRILL_SAMPLES)
        } else {
            Vec::new()
        };
        self.opened = Some(Opened {
            count: self.queue.count(&basis.scope),
            breakdown,
            // **右栏那一栏始终数整批**（照稿）：下钻收窄的是整批操作的作用范围，
            // 而「这一批长什么样」那句话不该跟着只剩一组。
            samples: self.queue.sample(&whole, basis.seed, SAMPLES),
            drilled_samples,
            basis,
        });
    }

    /// 列一次队列。**一个字节都不读主库**——原料全在中立库与沉淀库里（ADR-0001）。
    pub fn reload(&mut self, site: &Site) {
        match verdict::Index::load(&site.store, &site.library_identity)
            .map_err(|error| format!("沉淀库读不动：{error}"))
            .and_then(|index| {
                Queue::load(&site.catalog, &index).map_err(|error| format!("中立库读不动：{error}"))
            }) {
            Ok(queue) => {
                self.queue = queue;
                self.not_run = self.queue.not_run();
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
                self.all_passable = false;
                self.seed = 0;
                self.refresh_opened();
            }
            Err(message) => self.error = Some(message),
        }
        self.refresh_records(site);
        self.records_refusal = None;
        self.pending = None;
        self.cursor = None;
        // 这两样都跟着光标那一条走，而光标刚放掉了。留着的话，人再走回那一条上时
        // 看见的是上一份库上的账。
        self.matches = None;
        self.judged = None;
    }

    /// 画一帧。`area` 是屏头连屏体那一整块：裁决记录那块抽屉贴着它的右沿摆。
    pub fn ui(&mut self, ui: &mut egui::Ui, site: &mut Site, area: egui::Rect) {
        self.sync();
        match self.mode {
            Mode::Batches => {
                // 屏体照稿（设计稿 `.scrbody`）：三格、批列表一起滚，内边距取 `screen-body-padding`（`look::screen_body`）。
                look::screen_body(ui, "待确认·按批", |ui| self.batches_ui(ui, site));
            }
            Mode::OneByOne => {
                // 照稿两栏（设计稿 `.obo`，拿主意的人 2026-09-15 定）：左边待选列表，右边这一条的详情。左栏那条边界拖得动
                // 也记得住，声明在 [`crate::layout`]（票 `gui-redesign/12`）。
                // 贴边铺面板底，不要 egui 面板那一圈默认边距（设计稿 `.obolist`）；「«」收起成一条「待选列表」窄条
                // （设计稿 `.lstrip`），与浏览屏两栏同一副。
                let 框 = egui::Frame::new().fill(look::palette(ui).panel);
                layout::QUEUE_LIST
                    .show_collapsible(ui, "待选列表", 框, |ui| self.item_list(ui));
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(ui.visuals().panel_fill))
                    .show(ui, |ui| self.detail_pane(ui, site));
                self.keyboard(ui.ctx(), site);
            }
        }
        if self.filter_open {
            self.filter_dialog(ui.ctx());
        }
        if self.manual_open {
            self.manual_dialog(ui.ctx(), site);
        }
        if self.records_open {
            self.records_drawer(ui.ctx(), site, area);
        }
        if self.pending.is_some() {
            self.plan_modal(ui.ctx(), site);
        }
        self.receipt_ui(ui.ctx(), site);
    }

    /// 屏底那条提示条（[`crate::toast`]）：停够了收起；按下那颗按钮就走回来——「撤销」走 [`Screen::undo`]、「放回」走
    /// [`Screen::redo`]，与裁决记录抽屉里那两颗同一条核心库入口。
    fn receipt_ui(&mut self, ctx: &egui::Context, site: &mut Site) {
        let Some(receipt) = &mut self.receipt else {
            return;
        };
        match receipt.toast.show(ctx) {
            toast::Shown::Showing => {}
            toast::Shown::Expired => self.receipt = None,
            toast::Shown::Pressed => {
                let back = receipt.back;
                self.receipt = None;
                match back {
                    Back::Undo(id) => self.undo(site, id),
                    Back::Redo(id) => self.redo(site, id),
                }
            }
        }
    }

    /// 把**裁决记录**重列一遍：从沉淀库列（[`verdict::Store::batches`]），这一屏自己
    /// 一批都不记。
    fn refresh_records(&mut self, site: &Site) {
        // 条数给 0 是**不限**：裁决记录列的是落过的每一批，行是虚拟化的（`records_drawer`）。
        match site.store.batches(&site.library_identity, 0) {
            Ok(records) => self.records = records,
            Err(error) => self.error = Some(format!("沉淀库读不动：{error}")),
        }
        // **撤没撤以沉淀库为准**：撤掉的那一批对应的那一部分当场作废——那一项重新点得动，
        // 细分方式跟着解开。自己另记一份撤销状态的话，命令行撤的那几批这里永远不知道。
        let 在册: std::collections::BTreeSet<i64> = self
            .records
            .iter()
            .filter(|batch| !batch.undone())
            .map(|batch| batch.id)
            .collect();
        self.parts.keep(|batch| 在册.contains(&batch));
        self.refresh_opened();
    }

    /// **裁决记录**那一块：落过的每一批裁决，新的在前。
    ///
    /// 照稿是一块**贴右边的抽屉**（设计稿 `aside.drawer#lots`）：浮在屏头与屏体上面、宽 `drawer-width`、从屏头顶上一直到
    /// 状态栏上沿（`area` 就是那一整块），铺面板底、左沿一道 `line-2`、弹层那一层阴影（拿主意的人 2026-09-15 定，收挂单
    /// `Q625`、`Q661`）。**不是模态**，也就不走 [`dialog`]：开着的时候照样在队列上裁、撤了看队列变，逐条流的键盘照接
    /// （[`dialog::screen_has_keys`] 只拦模态那一层）。**也不是一条面板边界**：宽度不拖、不记。
    ///
    /// 每一行两行字（拿主意的人 2026-09-15 定）：主行「第 N 批裁决 · N 条」，副行「本地短时刻 · 裁成什么」；撤过的主行
    /// 划删除线、旁边一枚「已撤销」、一颗「放回」，在册的旁边一颗「撤销」。页脚那句说明照稿，底下写沉淀库在哪。
    ///
    /// **行是虚拟化的**（`show_rows`）：逐条流里按一下 `Y` 就是一批，真库上攒出几千批
    /// 是寻常事，每帧把几千行全排一遍版不划算。
    fn records_drawer(&mut self, ctx: &egui::Context, site: &mut Site, area: egui::Rect) {
        /// 这一帧在裁决记录里按下了哪一颗。
        enum Pressed {
            /// 在册那一批旁边的「撤销」。
            Undo(i64),
            /// 撤过那一批旁边的「放回」。
            Redo(i64),
            /// 抽屉头上的「关闭」。
            Close,
        }
        let tokens = Tokens::builtin();
        let 宽 = tokens.layout.drawer_width.min(area.width());
        let 抽屉 = egui::Rect::from_min_max(
            egui::pos2(area.right() - 宽, area.top()),
            area.right_bottom(),
        );
        let 沉淀库在哪 = site.store.location().to_owned();
        // 画的时候不改自己：按下去的那一下先记下来，画完再动。
        let mut pressed: Option<Pressed> = None;
        egui::Area::new(egui::Id::new("裁决记录抽屉"))
            .order(egui::Order::Foreground)
            .fixed_pos(抽屉.min)
            .constrain(false)
            .show(ctx, |ui| {
                let visuals = ui.visuals().clone();
                let palette = look::palette(ui);
                let 线宽 = tokens.layout.control_stroke;
                let painter = ui.painter();
                painter.add(visuals.popup_shadow.as_shape(抽屉, 0));
                painter.rect_filled(抽屉, 0.0, palette.panel);
                painter.vline(
                    抽屉.left(),
                    抽屉.y_range(),
                    egui::Stroke::new(线宽, palette.line_2),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(抽屉), |ui| {
                    ui.set_min_size(抽屉.size());
                    let [头上下, 头左右] = tokens.space.panel_padding;
                    let [行上下, 行左右] = tokens.space.record_row_padding;
                    let 字号 = look::font_size(ui.ctx(), tokens.font.size_small);
                    // ——— 头：标题、一句副标题、「关闭」 ———
                    egui::Panel::top("裁决记录抽屉·头")
                        .resizable(false)
                        .frame(
                            egui::Frame::new()
                                .inner_margin(egui::Margin::from(egui::vec2(头左右, 头上下))),
                        )
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("裁决记录")
                                        .size(tokens.font.size_panel_title)
                                        .color(palette.ink),
                                );
                                ui.label(
                                    egui::RichText::new("撤销后，相关变体会回到待确认队列")
                                        .size(字号)
                                        .color(palette.ink_3),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let 关 = look::small_ghost_button(ui, "关闭");
                                    if 关.clicked() {
                                        pressed = Some(Pressed::Close);
                                    }
                                });
                            });
                        });
                    // ——— 脚：照稿那句说明，底下写沉淀库在哪 ———
                    egui::Panel::bottom("裁决记录抽屉·脚")
                        .resizable(false)
                        .frame(
                            egui::Frame::new()
                                .inner_margin(egui::Margin::from(egui::vec2(行左右, 行上下))),
                        )
                        .show(ui, |ui| {
                            look::help(
                                ui,
                                "撤销不会删除记录，只标记为「已撤销」。如果某一批的部分内容已被之后的裁决覆盖，\
                                 需要先撤销之后的那一批。",
                            );
                            ui.label(
                                egui::RichText::new(format!("沉淀库 {沉淀库在哪}"))
                                    .size(字号)
                                    .color(palette.ink_3),
                            );
                        });
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new())
                        .show(ui, |ui| {
                            // **撤不动时那句话就画在按下去的地方**：核心库说清了是哪一批盖的，而人要做的
                            // 下一步（先撤那一批）就在这一块里。只画这一块里那两颗按钮自己的回话——
                            // 整屏别处的错（列队列失败、`U` 无可撤）各自画在各自的地方。
                            if let Some(refusal) = &self.records_refusal {
                                egui::Frame::new()
                                    .inner_margin(egui::Margin::from(egui::vec2(行左右, 行上下)))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.label(
                                            egui::RichText::new(refusal)
                                                .size(字号)
                                                .color(palette.lo),
                                        );
                                    });
                            }
                            if self.records.is_empty() {
                                ui.add_space(tokens.space.empty_state_padding);
                                ui.vertical_centered(|ui| {
                                    ui.label(egui::RichText::new("还没有裁决记录。").color(palette.ink_3));
                                });
                                return;
                            }
                            let 主行高 = ui.text_style_height(&egui::TextStyle::Body);
                            let 副行高 = ui.text_style_height(&egui::TextStyle::Small);
                            let 行高 = (2.0 * 行上下 + 主行高 + look::step(0) + 副行高)
                                .max(tokens.layout.button_small_height + 2.0 * 行上下);
                            ui.spacing_mut().item_spacing.y = 0.0;
                            egui::ScrollArea::vertical()
                                .id_salt("裁决记录")
                                .auto_shrink(false)
                                .show_rows(ui, 行高, self.records.len(), |ui, rows| {
                                    for record in &self.records[rows] {
                                        if let Some(press) =
                                            record_row(ui, record, self.record_clock, 行高, palette)
                                        {
                                            pressed = Some(press);
                                        }
                                    }
                                });
                        });
                });
            });
        match pressed {
            Some(Pressed::Undo(id)) => self.undo(site, id),
            Some(Pressed::Redo(id)) => self.redo(site, id),
            Some(Pressed::Close) => self.records_open = false,
            None => {}
        }

        /// 裁决记录里一批裁决那一行：主行「第 N 批裁决 · N 条」、副行「本地短时刻 · 裁成什么」，右头「撤销」或
        /// 「已撤销」＋「放回」。交回这一帧按下的那一颗。
        fn record_row(
            ui: &mut egui::Ui,
            record: &verdict::Batch,
            clock: RecordClock,
            行高: f32,
            palette: &crate::tokens::Palette,
        ) -> Option<Pressed> {
            let tokens = Tokens::builtin();
            let [行上下, 行左右] = tokens.space.record_row_padding;
            let (行, _) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 行高), egui::Sense::hover());
            ui.painter().hline(
                行.x_range(),
                行.bottom() - tokens.layout.control_stroke / 2.0,
                egui::Stroke::new(tokens.layout.control_stroke, palette.line),
            );
            let 里头 = 行.shrink2(egui::vec2(行左右, 行上下));
            let mut pressed = None;
            let mut 子 = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(里头)
                    .layout(Layout::right_to_left(Align::Center)),
            );
            let 撤过 = record.undone();
            let (label, hint, press) = if 撤过 {
                (
                    "放回",
                    "把这一批原样放回去：当初落下的每一条都记在批里，一个字都不必重打。",
                    Pressed::Redo(record.id),
                )
            } else {
                (
                    "撤销",
                    "中立库与沉淀库两边都回到这一批落下之前，那些变体当场回到待确认队列——不必重跑识别。",
                    Pressed::Undo(record.id),
                )
            };
            if look::small_buttons(&mut 子, |ui| {
                ui.button(label).on_hover_text(hint).clicked()
            }) {
                pressed = Some(press);
            }
            if 撤过 {
                look::plain_chip(&mut 子, look::Tone::Neutral, "已撤销");
            }
            子.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = look::step(0);
                let mut 主行 = font::strong(format!(
                    "第 {} 批裁决 · {} 条",
                    record.id,
                    thousands(record.rows)
                ));
                if 撤过 {
                    主行 = 主行.strikethrough().color(palette.ink_3);
                }
                let 悬停 = {
                    let mut text = record.summary.clone();
                    if let Some(note) = &record.note {
                        text.push_str(&format!("\n「{note}」"));
                    }
                    if let Some(at) = record.undone_at {
                        text.push_str(&format!("\n撤销于 {}", clock.short(at)));
                    }
                    text
                };
                ui.add(egui::Label::new(主行).truncate())
                    .on_hover_text(悬停.clone());
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!(
                            "{} · {}",
                            clock.short(record.decided_at),
                            record.summary
                        ))
                        .small()
                        .color(palette.ink_3),
                    )
                    .truncate(),
                )
                .on_hover_text(悬停);
            });
            pressed
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
    ///
    /// **换轴那一半走 [`Screen::set_axis`]**，不自己赋值：`self.axis` 与展开那一批的细分轴
    /// 是同一格，而它可能锁着（已经就地裁过一部分）。两个入口各拼一道判断正是 ADR-0005
    /// 那一节要防的事——挡住了就整个不做，免得选择器换了而轴没换，屏上两头对不上。
    pub fn pick(&mut self, axis: Axis, label: &str) {
        if self.set_axis(axis) {
            self.picks.pick(axis, label);
        }
    }

    /// 点一下表头：**换一列排**。界面上点那一下走的就是它，实测与测试拿它当那一下。
    ///
    /// 排在内存里（[`ItemOrder`]），**一次库都不读**。
    pub fn sort_by(&mut self, order: ItemOrder, descending: bool) {
        self.queue.set_order(order, descending);
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
    /// 屏头与正文各画各的，而屏头先画——不先同步一次，屏头上那几个数就永远比正文慢
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

    /// 屏头右侧属于这一屏的那一段（设计稿 `.scrhead`，[`look::screen_header`] 的右侧）：高 / 中 / 低三枚置信度标签、
    /// 「按批｜逐条」、「裁决记录 N」，最右一颗小号幽灵「重新列队列」（拿主意的人 2026-09-15 定）。
    ///
    /// 原来那句「队列 N 条待裁决；选中 N 条 · N 条候选」与第四枚「没有候选 N」照稿删了：待确认几个写在屏头副标题上，
    /// 没有候选几个写在正文第三格，选中几条写在逐条那一屏的栏头上。「另有 N 个变体连识别都还没跑过」挪进了正文
    /// （正文三格上方那一条提示）——它们一条候选都没有，**队列里根本没有它们**，选择器也筛不到；措辞与命令行 `triage list`
    /// 印的是同一句（[`romcat_core::triage::report::QueueReport`]）。
    pub fn status(&mut self, ui: &mut egui::Ui, site: &Site) {
        self.sync();
        // **三档一眼看得出哪批稳、哪批悬**（规格 37）。颜色与标签同出一处（[`look::tier_tone`]、[`Tier::label`]）。
        // 第四档「**没有候选**」不在屏头（照稿，拿主意的人 2026-09-15 定）：它的数写在正文第三格。
        // 数的是**整个队列**，不随选择器动（设计稿 `.scrhead`）——切到逐条只看有多个候选的那几批时，这三个数照旧。
        for (tier, count) in self.queue.all_tiers() {
            if *tier == Tier::Unidentified {
                continue;
            }
            look::chip(
                ui,
                look::tier_tone(*tier),
                &format!("{} {}", tier.label(), thousands(*count)),
            );
        }
        match look::segmented(
            ui,
            &[(Mode::Batches, "按批"), (Mode::OneByOne, "逐条")],
            self.mode,
        ) {
            Some(Mode::Batches) if self.mode != Mode::Batches => self.show_batches(),
            // 屏头这一颗看的是**有多个候选**的那几批（设计稿 `data-qmode="obo"` 那一栏的栏头）。
            Some(Mode::OneByOne) if self.mode != Mode::OneByOne => self.show_multiple(),
            _ => {}
        }
        // 数的是**还在册**的那几批（设计稿 `#lots-n` 只数没撤过的）；撤过的照旧列在抽屉里。
        let 条数 = thousands_len(
            self.records
                .iter()
                .filter(|record| !record.undone())
                .count(),
        );
        let 开记录 = look::small_buttons(ui, |ui| {
            let 字 = records_button_text(ui, &条数);
            ui.button(字)
                .on_hover_text("落过的每一批裁决都列在这里：什么时候落的、多少条、撤过没有。")
                .clicked()
        });
        if 开记录 {
            self.records_open = !self.records_open;
        }
        // 最右一颗小号幽灵按钮（稿上没有；拿主意的人 2026-09-15 定留着）：命令行在别处裁过、别处跑完一趟刮削之后，
        // 这一屏自己不知道。
        let 重列 = look::small_ghost_button(ui, "重新列队列")
            .on_hover_text("识别跑过一趟、或者在别处裁过之后点它。一个字节都不读主库。")
            .clicked();
        if 重列 {
            self.reload(site);
        }
    }

    /// 正文三格上方那一条提示：库里**还有变体连识别都还没跑过**（词表「还没识别」、[`NOT_RUN_LABEL`]），旁边一颗小号
    /// 「运行识别」。按下去返回 `true`。原来在屏头右侧，照稿挪进正文（拿主意的人 2026-09-15 定）。
    ///
    /// 它们不在待确认那个数里——`variant JOIN identification` 一行都进不去，所以既不是「拿不定主意」，也不是三格里的
    /// 任何一格。措辞照命令行那份报告来（`romcat_core::triage::report`），同一份库两处印出来的是同一句话。
    fn not_run_note(&self, ui: &mut egui::Ui) -> bool {
        if self.not_run == 0 {
            return false;
        }
        let mut 要跑 = false;
        look::note_box(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!(
                        "{NOT_RUN_LABEL} 另有 {} 个变体连识别都还没跑过",
                        thousands(self.not_run)
                    ),
                )
                .on_hover_text(
                    "它们一条候选都没有，队列里根本没有它们，选择器也筛不到。\
                     点旁边那颗「运行识别」把它们补上。",
                );
                要跑 = identify_shortcut(ui, false);
            });
        });
        ui.add_space(Tokens::builtin().space.queue_summary_gap);
        要跑
    }

    /// **正文：一级分批那一列卡片。**
    fn batches_ui(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        if !self.queue.identified() {
            // 画的时候不改自己：按下去的那一下先记下来，画完再动。
            if identify_empty_state(ui, self.not_run, false) {
                self.ask_identify();
            }
            return;
        }
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if self.queue.batches().is_empty() {
            if identify_empty_state(ui, self.not_run, true) {
                self.ask_identify();
            }
            return;
        }
        if self.not_run_note(ui) {
            self.ask_identify();
        }
        // 数一个都不在这儿算——账由核心库交出来（ADR-0005）。
        let 账 = self.queue.coverage(HEADLINE);
        summary_cells(ui, &账);

        // 分批那一行（设计稿「可批量处理的排在前面 · 按判定依据分批」）：词照词表**依据形状**与**批**写（拿主意的人
        // 2026-09-15 定），「批变体」与裁决记录里的「批裁决」分得开。
        ui.horizontal(|ui| {
            look::section(
                ui,
                &format!(
                    "可整批处理的排在前面 · 按依据形状分成 {} 批变体",
                    thousands_len(账.batches)
                ),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                look::help(ui, "来源、DAT、哈希口径、候选数和置信度都相同的归为一批");
            });
        });
        // **能整批通过的先摆**（设计稿 `#batches`）：只先摆前几批（与头一格说的是同几批），余下的收成一句，点「列出这 N 批」
        // 再摆（稿上只写了那一句，没画怎么够得着它们）；接着是「没有候选」那个虚线框；框下是不能整批通过的批，样式同卡片
        // （拿主意的人 2026-09-15 定）。
        // **只把要画的那几张卡片拷出来**：全部批可能是好几百，每帧整份克隆等于白拷。借出来的那份还得让下面几行调得动
        // `&mut self`（展开、下钻、整批过）。能整批通过的排在最前面（核心库排的），前 `先摆` 张就是它们。
        let 先摆 = if self.all_passable {
            账.head_batches + 账.rest_answerable_batches
        } else {
            账.head_batches
        };
        let 能过: Vec<Batch> = self.queue.batches().iter().take(先摆).cloned().collect();
        // 不能整批通过的那几批也只摆前 `TOP_BATCHES` 张。**没摆出来的那两个数是这一屏自己的分页**（摆了几张卡片），
        // 不是领域里的账——领域里的账全在核心库的 `Coverage` 里。
        let 能过总数 = 账.head_batches + 账.rest_answerable_batches;
        let 不能过: Vec<Batch> = self
            .queue
            .batches()
            .iter()
            .skip(能过总数)
            .take(TOP_BATCHES)
            .cloned()
            .collect();
        let 没列的批 = (账.batches - 能过总数).saturating_sub(TOP_BATCHES);
        let 没列的条: u64 = self
            .queue
            .batches()
            .iter()
            .skip(能过总数 + TOP_BATCHES)
            .map(|batch| batch.count)
            .sum();
        let mut clicked: Option<Shape> = None;
        let mut 逐条 = false;
        for batch in &能过 {
            if self.card(ui, site, batch) {
                clicked = Some(batch.shape.clone());
            }
        }
        if !self.all_passable && 账.rest_answerable_batches > 0 {
            let mut 列出 = false;
            ui.horizontal(|ui| {
                look::help(
                    ui,
                    &format!(
                        "另有 {} 批（{} 条）同样只有一个候选。",
                        thousands_len(账.rest_answerable_batches),
                        thousands(账.rest_answerable),
                    ),
                );
                列出 = look::small_ghost_button(
                    ui,
                    format!("列出这 {} 批", thousands_len(账.rest_answerable_batches)),
                )
                .clicked();
            });
            if 列出 {
                self.all_passable = true;
            }
        }
        if bare_box(ui, &账) {
            逐条 = true;
        }
        for batch in &不能过 {
            if self.card(ui, site, batch) {
                clicked = Some(batch.shape.clone());
            }
        }
        if 逐条 {
            self.open = None;
            self.drill = None;
            self.show_one_by_one();
        }
        if 没列的批 > 0 {
            ui.weak(format!(
                "……另有 {} 批变体没列（共 {} 条）。先把上面这几批过完。",
                thousands_len(没列的批),
                thousands(没列的条),
            ));
        }
        if let Some(shape) = clicked {
            self.open_batch(&shape);
        }
    }

    /// 一批变体的卡片（设计稿 `.batch`）。返回「这一帧点了它的卡头」。
    ///
    /// 面板底、一圈分隔线、大圆角，左沿一道置信度色（`tier-bar` 那么宽，设计稿 `box-shadow: inset 3px 0 0`）——**色条从不单独出现**，
    /// 那一档的词就在卡头右边那枚标签上。卡头（[`batch_head`]）整块点得动，悬停垫次级底色。
    ///
    /// **这一批在命令行上是什么，就挂在卡头与判定依据的悬停上**（挂单 `Q177`）：印的是折算那一对折出来的那一串
    /// （`Shape::selector`），报告里那一行印的也是它——屏上点这一张与命令行敲那一条选中的是同一批（ADR-0005），
    /// 所以这儿不许另编一句像模像样的话。
    fn card(&mut self, ui: &mut egui::Ui, site: &mut Site, batch: &Batch) -> bool {
        let tokens = Tokens::builtin();
        let palette = look::palette(ui);
        let open = self.open.as_ref() == Some(&batch.shape);
        let 线宽 = tokens.layout.control_stroke;
        let 圆角 = tokens.radius.large;
        let 选择器 = format!("命令行上是 `--shape '{}'`", batch.shape.selector());
        let 色 = look::tier_color(batch.tier(), ui.visuals());
        // 这一批已经就地裁掉多少条（[`Parts::done_in`]）：卡头第二行那句「已处理 N 条」说的是它。
        let 裁掉的 = self.parts.done_in(&batch.shape);
        let mut hit = false;
        let 整张 = look::barred_card(ui, 色, look::BarEdge::Left, |ui| {
            // 悬停那一层要垫在字底下：先占位置，量完卡头再填。
            let 悬停底 = ui.painter().add(egui::Shape::Noop);
            let [上下, 左右] = tokens.space.batch_head_padding;
            let 头 = egui::Frame::new()
                .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.style_mut().interaction.selectable_labels = false;
                    batch_head(ui, batch, open, 裁掉的);
                });
            let 头响应 = ui.interact(
                头.response.rect,
                ui.id().with(("一批变体的卡头", batch.shape.selector())),
                egui::Sense::click(),
            );
            if 头响应.hovered() {
                let 角 = if open {
                    egui::CornerRadius {
                        nw: 圆角,
                        ne: 圆角,
                        sw: 0,
                        se: 0,
                    }
                } else {
                    egui::CornerRadius::same(圆角)
                };
                ui.painter().set(
                    悬停底,
                    egui::epaint::RectShape::filled(头.response.rect, 角, palette.panel_2),
                );
            }
            hit = 头响应
                .on_hover_text(if open {
                    选择器.clone()
                } else {
                    format!("点开看细分与随机样本。{选择器}")
                })
                .clicked();
            if open {
                self.opened_card(ui, site, batch, &选择器);
            }
        })
        .response
        .rect;
        ui.painter().rect_stroke(
            整张,
            圆角,
            egui::Stroke::new(线宽, palette.line),
            egui::StrokeKind::Inside,
        );
        ui.add_space(tokens.space.batch_gap);
        hit
    }

    /// 展开之后那一块（设计稿 `.bbody`）：判定依据、左「细分」右「随机样本」两栏、按钮一排。
    ///
    /// 判定依据那一框画「判定依据：」加 [`Batch::why`] 原话，不另编句子（拿主意的人 2026-09-15 定）。
    /// 能整批通过的那一批主按钮是「全部通过（N 条）」；不能的主按钮是「逐条处理」、不给「全部通过」，
    /// 「全部拒绝」照留（拿主意的人 2026-09-15 定）。
    ///
    /// **底下那一排始终作用于整批**（票 `gui-looks-like-the-design/19`，照稿）：下钻要落的那两下摆在
    /// 就地那一框里（[`drill_box`]），这一排说的是**剩下的部分**——已经就地裁掉一部分之后那两颗改写
    /// 「通过剩余的 N 条」「拒绝剩余的 N 条」，N 是核心库给的 [`Breakdown::left`]。
    /// 两处各作用于一层，正是「每一层都能整批过或整批拒」那句话的落点。
    fn opened_card(&mut self, ui: &mut egui::Ui, site: &mut Site, batch: &Batch, 选择器: &str) {
        let Some(opened) = self.opened.clone() else {
            return;
        };
        // 下钻着的那一组（就地那一框那两颗作用在它上面）与整批（底下那一排作用在它上面）。
        let scope = opened.basis.scope.clone();
        let whole = Scope::whole(batch.shape.clone());
        let tokens = Tokens::builtin();
        let palette = look::palette(ui);
        let 线宽 = tokens.layout.control_stroke;
        let [上, 左右, 下] = tokens.space.batch_body_padding;
        let 缝 = tokens.space.batch_body_gap;
        let 剩下 = opened.breakdown.left;
        let 裁过一部分 = opened.breakdown.partly_done();
        let passable = batch.passable();
        let 色 = look::tier_color(batch.tier(), ui.visuals());
        // 画的时候不改自己：按下去的那一下先记下来，画完再动。
        let mut 按下 = BodyPressed::default();
        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 左右 as i8,
                right: 左右 as i8,
                top: 上 as i8,
                bottom: 下 as i8,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                let [依上下, 依左右] = tokens.space.why_padding;
                egui::Frame::new()
                    .fill(palette.panel_2)
                    .stroke(egui::Stroke::new(线宽, palette.line))
                    .corner_radius(tokens.radius.medium)
                    .inner_margin(egui::Margin::from(egui::vec2(依左右, 依上下)))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let 字号 = look::font_size(ui.ctx(), tokens.font.size_small_plus);
                        let mut job = egui::text::LayoutJob::default();
                        for (段, 色) in [
                            ("判定依据：".to_owned(), palette.ink),
                            (batch.why(), palette.ink_2),
                        ] {
                            job.append(
                                &段,
                                0.0,
                                egui::TextFormat {
                                    font_id: egui::FontId::proportional(字号),
                                    color: 色,
                                    ..egui::TextFormat::default()
                                },
                            );
                        }
                        ui.label(job).on_hover_text(选择器);
                    });
                ui.add_space(缝);
                // 两栏宽比照令牌 `batch-body-columns`（设计稿 `.bbody` 的 `grid-template-columns`）。
                let 全宽 = ui.available_width();
                let [左份, 右份] = tokens.layout.batch_body_columns;
                let 左宽 = ((全宽 - 缝) * 左份 / (左份 + 右份)).max(0.0);
                let 右宽 = (全宽 - 缝 - 左宽).max(0.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 缝;
                    ui.allocate_ui_with_layout(
                        egui::vec2(左宽, 0.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(左宽);
                            drill_column(ui, &opened, 色, &mut 按下);
                        },
                    );
                    ui.allocate_ui_with_layout(
                        egui::vec2(右宽, 0.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(右宽);
                            按下.resample = samples_column(ui, &opened.samples);
                        },
                    );
                });
                ui.add_space(缝);
                ui.horizontal(|ui| {
                    look::buttons(ui, |ui| {
                        if passable {
                            按下.pass = ui
                                .scope(|ui| {
                                    look::primary_button(ui.visuals_mut());
                                    ui.add_enabled(
                                        剩下 > 0,
                                        egui::Button::new(if 裁过一部分 {
                                            format!("通过剩余的 {} 条", thousands(剩下))
                                        } else {
                                            format!("全部通过（{} 条）", thousands(剩下))
                                        }),
                                    )
                                })
                                .inner
                                .on_hover_text(
                                    "采用第一条候选——分批时那句共同依据说的正是它。先出计划再动手。",
                                )
                                .clicked();
                            按下.one_by_one = ui.button("逐条处理").clicked();
                        } else {
                            按下.one_by_one = ui
                                .scope(|ui| {
                                    look::primary_button(ui.visuals_mut());
                                    ui.button("逐条处理")
                                })
                                .inner
                                .on_hover_text(
                                    "这一批不是单候选：一条都没有时「通过」什么也没定下来，\
                                     好几条时它是替你挑了个没看过的答案。逐条处理。",
                                )
                                .clicked();
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            按下.reject = ui
                                .scope(|ui| {
                                    look::warn_button(ui.visuals_mut());
                                    ui.add_enabled(
                                        剩下 > 0,
                                        egui::Button::new(if 裁过一部分 {
                                            format!("拒绝剩余的 {} 条", thousands(剩下))
                                        } else {
                                            "全部拒绝".to_owned()
                                        }),
                                    )
                                })
                                .inner
                                .on_hover_text(
                                    "记成「我看过了，认不出」：这些条退出队列，不再问第二遍。撤得回来。",
                                )
                                .clicked();
                        });
                    });
                });
            });
        if let Some(axis) = 按下.axis {
            self.set_axis(axis);
        }
        if 按下.whole {
            self.drill_out();
        }
        if let Some(label) = 按下.into {
            self.drill_into(&label);
        }
        if 按下.resample {
            self.resample();
        }
        // **底下那一排作用于整批，就地那一框里那两颗只作用于下钻着的那一组**：
        // 「剩下的部分仍旧整批处理得动」就是这么成立的。
        if 按下.pass {
            self.pass(site, &whole);
        }
        if 按下.reject {
            self.reject(site, &whole);
        }
        if 按下.drilled_pass {
            self.pass(site, &scope);
        }
        if 按下.drilled_reject {
            self.reject(site, &scope);
        }
        // 底下那一排说的是**整批剩下的**，所以先把下钻那一层放掉再逐条看；
        // 就地那一框里那一颗逐条看的正是下钻着的那一组。
        if 按下.one_by_one {
            self.drill_out();
            self.show_one_by_one();
        }
        if 按下.drilled_one_by_one {
            self.show_one_by_one();
        }
    }

    /// 逐条那一屏左边那栏（设计稿 `.obolist`）：栏头写看的是哪一类（「有多个候选」、某一批的依据形状、整个队列）、几条，
    /// 一颗「筛选…」、一颗排序；底下一条一行——文件名（等宽）、「平台 · 几个候选」，左沿一道那一档的色，光标那一条垫强调浅底。
    /// **置信度只靠那一道色**，行里不写那一档的词（照稿，拿主意的人 2026-09-14 定，与浏览屏表格同一条，挂单 `Q872`）；
    /// 那个词写在右边每张候选卡片的标签上。
    ///
    /// **行是虚拟化的**（`show_rows`），**一个文本框都没有**：见模块文档。
    ///
    /// **排序收在栏头那一颗里，而排在内存里**（[`ItemOrder`]）——队列本来就整份在内存里，不为它另开一条查询。这与主列表
    /// 那张表（[`crate::table::Table`]，排序下推到 `ORDER BY`）是两条路，**分界在数据躺在哪**，不在哪张表更讲究。
    /// **那颗上的字照队列自己那份排序画**，界面不另存一份：存两份的下场是屏上写的与真排出来的次序漂开。
    fn item_list(&mut self, ui: &mut egui::Ui) {
        let tokens = Tokens::builtin();
        let palette = look::palette(ui);
        if !self.queue.identified() {
            ui.add_space(tokens.space.empty_state_margin);
            ui.vertical_centered(|ui| look::help(ui, "还没跑过识别。"));
            return;
        }
        let 线宽 = tokens.layout.control_stroke;
        let (sorted_by, descending) = self.queue.order();
        let 条数 = format!("{} 条", thousands_len(self.queue.selected().len()));
        let 标题 = self
            .picks
            .only
            .as_ref()
            .map_or_else(|| "整个队列".to_owned(), Only::label);
        let mut 开筛选 = false;
        let mut 换排序 = None;
        let [上, 右, 下, 左] = tokens.space.list_head_padding;
        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 左 as i8,
                right: 右 as i8,
                top: 上 as i8,
                bottom: 下 as i8,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let 小字 = look::font_size(ui.ctx(), tokens.font.size_small);
                let 数宽 = ui
                    .painter()
                    .layout_no_wrap(
                        条数.clone(),
                        egui::FontId::proportional(小字),
                        egui::Color32::PLACEHOLDER,
                    )
                    .size()
                    .x;
                let 缝 = ui.spacing().item_spacing.x;
                let 标题宽 = (ui.available_width()
                    - 数宽
                    - look::small_button_width(ui, "筛选…")
                    - tokens.layout.icon_button
                    - 4.0 * 缝)
                    .max(0.0);
                ui.horizontal(|ui| {
                    ui.scope(|ui| {
                        ui.set_max_width(标题宽);
                        // 从某一批点「逐条处理」进来时是那一批的依据形状，放不下就尾部截断、悬停看全文。
                        ui.add(egui::Label::new(font::strong(标题.clone())).truncate())
                            .on_hover_text(标题.as_str());
                    });
                    look::help(ui, &条数);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        layout::QUEUE_LIST.collapse_button(ui);
                        开筛选 = look::small_buttons(ui, |ui| {
                            ui.button("筛选…")
                                .on_hover_text("按识别结论、按目录、按候选作品、按命名规律收窄这一栏。")
                                .clicked()
                        });
                    });
                });
                ui.horizontal(|ui| {
                    look::help(ui, "排序");
                    let 箭头 = if descending { "▼" } else { "▲" };
                    look::small_buttons(ui, |ui| {
                        ui.menu_button(format!("{} {箭头}", sorted_by.label()), |ui| {
                            for order in ItemOrder::ALL {
                                if ui.button(order.label()).clicked() {
                                    // 再点一次正在排的那一列就翻方向。
                                    换排序 = Some((order, order == sorted_by && !descending));
                                    ui.close();
                                }
                            }
                        })
                        .response
                        .on_hover_text(
                            "换一列排；点正在排的那一列翻方向。队列本来就整份在内存里，这一下一次库都不读。",
                        );
                    });
                });
            });
        look::divider(ui);
        let items = self.queue.selected();
        let mut picked = None;
        if items.is_empty() {
            ui.add_space(tokens.space.empty_state_padding);
            ui.vertical_centered(|ui| {
                look::help(ui, "一条都没选中。「筛选…」里写宽一点，或者清除筛选。");
            });
        } else {
            let [项上, 项右, 项下, 项左] = tokens.space.list_item_padding;
            let 字号 = look::font_size(ui.ctx(), tokens.font.size_caption_plus);
            let 字高 = ui
                .painter()
                .layout_no_wrap(
                    "字".to_owned(),
                    egui::FontId::proportional(字号),
                    egui::Color32::PLACEHOLDER,
                )
                .size()
                .y;
            let 行高 = 项上 + 2.0 * 字高 + 项下;
            let at = self.at;
            // 行画完之后手上没有那一行的 `Ui` 了——先把上下文留一份，焦点那一圈要用。
            let ctx = ui.ctx().clone();
            let mut area = egui::ScrollArea::vertical()
                .id_salt("待选列表")
                .auto_shrink(false);
            if let Some(offset) = self.scroll_to {
                area = area.vertical_scroll_offset(offset);
            }
            area.show_rows(ui, 行高, items.len(), |ui, rows| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let 看得见的 = ui.clip_rect();
                for index in rows {
                    let Some(item) = items.get(index) else {
                        continue;
                    };
                    let (行, 响应) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 行高),
                        egui::Sense::click(),
                    );
                    let painter = ui.painter();
                    if index == at {
                        painter.rect_filled(行, 0.0, palette.accent_soft);
                    } else if 响应.hovered() {
                        painter.rect_filled(行, 0.0, palette.panel_2);
                    }
                    // **哪一档由核心库说**（`Item::tier`）——「看第一条候选」那条规则只该有一份。
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            行.min,
                            egui::vec2(tokens.layout.tier_bar, 行.height()),
                        ),
                        0.0,
                        look::tier_color(item.tier(), ui.visuals()),
                    );
                    painter.hline(
                        行.x_range(),
                        行.bottom() - 线宽 / 2.0,
                        egui::Stroke::new(线宽, palette.line),
                    );
                    let 里头 = egui::Rect::from_min_max(
                        egui::pos2(行.left() + 项左, 行.top() + 项上),
                        egui::pos2(行.right() - 项右, 行.bottom() - 项下),
                    );
                    let mut 字 = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(里头)
                            .layout(Layout::top_down(Align::Min)),
                    );
                    字.style_mut().interaction.selectable_labels = false;
                    字.spacing_mut().item_spacing.y = 0.0;
                    字.add(
                        egui::Label::new(
                            egui::RichText::new(item.name())
                                .family(egui::FontFamily::Monospace)
                                .size(字号)
                                .color(palette.ink),
                        )
                        .truncate(),
                    );
                    let 候选 = match item.candidates.len() {
                        0 => "没有候选".to_owned(),
                        n => format!("{n} 个候选"),
                    };
                    字.add(
                        egui::Label::new(
                            egui::RichText::new(format!(
                                "{} · {候选}",
                                item.variant.platform.as_deref().unwrap_or("—")
                            ))
                            .size(字号)
                            .color(palette.ink_3),
                        )
                        .truncate(),
                    );
                    // 焦点落在这一行上要看得见：行是点得中的，Tab 走得到它（票 `gui-redesign/12` 验收第 7 条）。
                    look::focus_ring(&ctx, 看得见的, &响应);
                    if 响应.clicked() {
                        picked = Some((index, item.variant.key.clone()));
                    }
                }
            });
        }
        if let Some((index, key)) = picked {
            self.at = index;
            self.cursor = Some(key);
            self.nth = 0;
        }
        // **排完当场把光标捞回来**：`at` 是「排在第几位」，排序一换它就指到别人身上了。
        // 不能等下一帧的 `sync`——`Screen::ui` 里键盘那一下排在这一栏**之后**，同一帧里
        // 既换了排序又按了 `Y` 的话，`decide_here` 拿的 `at` 是按旧次序算的下标，
        // 裁的就不是屏上那一条。光标记的是键，所以捞得回来：人换排序之前停在哪一条，
        // 换完还停在同一条。
        if let Some((order, descending)) = 换排序 {
            self.queue.set_order(order, descending);
            self.resolve_cursor();
        }
        if 开筛选 {
            self.filter_open = true;
        }
    }

    /// 逐条那一屏右边那一块（设计稿 `.obodet`）：这一条是什么、候选、按钮、键位提示、中文离线源的那次匹配，整块竖着滚。
    fn detail_pane(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        if !self.queue.identified() {
            // 画的时候不改自己：按下去的那一下先记下来，画完再动。
            if identify_empty_state(ui, self.not_run, false) {
                self.ask_identify();
            }
            return;
        }
        let [上下, 左右] = Tokens::builtin().space.detail_padding;
        egui::ScrollArea::vertical()
            .id_salt("详情")
            .auto_shrink(false)
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if let Some(error) = &self.error {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                        self.detail(ui, site);
                    });
            });
    }

    /// 这一条的**文件名、路径**与全部**候选**（设计稿 `.cands`：一张一张卡片，写着作品、那一档、来源、匹配、**依据**），
    /// 四颗带键帽的按钮（通过所选候选、都不对、先放着、撤销上一条）与一颗「手工指定…」，一框键位提示，底下接着这个变体身上
    /// **中文离线源那几次匹配**（[`Screen::matches_ui`]）。按钮走的与键盘同几条路（[`Screen::keyboard`]）。
    fn detail(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        let at = self.at;
        let 共 = self.queue.selected().len();
        let detail = match self.queue.detail(&site.catalog, at) {
            Ok(item) => item,
            Err(error) => {
                self.error = Some(format!("中立库读不动：{error}"));
                None
            }
        };
        let Some(item) = detail else {
            look::help(ui, "队列里一条都没有了。回到按批那一屏看看还剩什么。");
            return;
        };
        // 不写 `**内容**`：那是命令行报告里的记法，`ui.label` 会把星号照着画出来。
        // 那一句由核心库给（设计稿「裁决按文件内容记录，改名或移动后仍然有效」），命令行 `triage list` 印的是同一句。
        let anchored = verdict::anchor_sentence(item.print.is_some());
        let (key, name) = (item.variant.key.clone(), item.name().to_string());
        let (state, platform, bytes) = (
            item.state.label(),
            item.variant.platform.clone(),
            capacity(item.variant.bytes, item.variant.unreadable_files),
        );
        let reason = item.reason.clone();
        let candidates: Vec<CandidateCard> = item
            .candidates
            .iter()
            .map(|candidate| CandidateCard {
                tier: Tier::of(Some(candidate.confidence)),
                game: candidate.game.clone(),
                mark: candidate.chinese.map(|mark| mark.label().to_owned()),
                source: candidate.source.clone(),
                matched: format!("{} · {}", candidate.dat, candidate.hashed_as.label()),
                evidence: candidate.evidence.clone(),
            })
            .collect();
        let nth = self.nth.min(candidates.len().saturating_sub(1));
        let tokens = Tokens::builtin();
        let palette = look::palette(ui);
        let 缝 = tokens.space.detail_gap;
        ui.spacing_mut().item_spacing.y = 0.0;
        // ——— 抬头：第几条、文件名、路径，几枚标签 ———
        // **文件名与路径分两行**：人裁决时先认名字，路径是用来判「这一批是不是同一堆」的。
        look::section(
            ui,
            &format!(
                "第 {} 条 · 共 {} 条{}",
                thousands(at as u64 + 1),
                thousands_len(共),
                // 「共 N 条有多个候选」：后面跟的是左边那一栏的名字（设计稿 `.obodet` 抬头）。
                self.picks.only.as_ref().map_or("", Only::head_suffix),
            ),
        );
        ui.add_space(tokens.space.obo_head_gap);
        ui.label(
            egui::RichText::new(&name)
                .family(egui::FontFamily::Monospace)
                .size(tokens.font.size_title)
                .color(palette.ink),
        );
        ui.add_space(tokens.space.obo_head_gap);
        // 完整的「根名 · 相对路径」（设计稿 `主库 · GBA/汉化/…`），画不下从左边删字（票 09 那一处，`table::root_and_path`）。
        let 路径字 =
            egui::FontId::monospace(look::font_size(ui.ctx(), tokens.font.size_caption_plus));
        let 路径 = crate::table::root_and_path(ui, &key, &路径字, ui.available_width());
        ui.label(egui::RichText::new(路径).font(路径字).color(palette.ink_2))
            .on_hover_text(key.as_str());
        ui.add_space(look::step(0));
        ui.horizontal_wrapped(|ui| {
            look::inline_tag(ui, platform.as_deref().unwrap_or("平台未知"));
            look::inline_tag(ui, &bytes);
            // 结论那一枚只在一条候选都没有时画（拿主意的人 2026-09-15 定）：有候选的一律是命中，照稿不画；
            // 没有候选的分得出是未命中还是无判据，去掉就丢了。
            if candidates.is_empty() {
                look::inline_tag(ui, state);
            }
            look::inline_tag(ui, anchored);
        });
        if let Some(reason) = &reason {
            ui.add_space(look::step(0));
            look::help(ui, &format!("为什么没定下来：{reason}"));
        }
        ui.add_space(缝);
        // ——— 候选 ———
        let mut 点了候选 = None;
        if candidates.is_empty() {
            look::help(
                ui,
                "候选：一条都没有——要裁决就得手工指定作品。那是队列的常态，不是异常。",
            );
        } else {
            look::section(
                ui,
                &format!("候选 {} 个 · 选择一个，或选择「都不对」", candidates.len()),
            );
            ui.add_space(缝);
            点了候选 = candidate_cards(ui, &candidates, nth);
        }
        ui.add_space(缝);
        // ——— 按钮：与键盘同几条路 ———
        let 有候选 = !candidates.is_empty();
        let 撤得了 = self.applied.is_some();
        let mut 按下 = OneByOnePressed::default();
        ui.horizontal(|ui| {
            look::buttons(ui, |ui| {
                按下.pass = ui
                    .scope(|ui| {
                        look::primary_button(ui.visuals_mut());
                        look::key_button(ui, "通过所选候选", "Y", 有候选)
                    })
                    .inner
                    .on_hover_text("采用选中的那条候选，当场落下这一条。撤得回来。")
                    .clicked();
                按下.reject = look::key_button(ui, "都不对", "N", true)
                    .on_hover_text("记成「我看过了，认不出」：这一条退出队列，不再问第二遍。撤得回来。")
                    .clicked();
                按下.set_aside = look::key_button(ui, "先放着", "空格", true)
                    .on_hover_text("一个字都不写库，这一条下一轮还会撞见。")
                    .clicked();
                按下.manual = ui
                    .button("手工指定…")
                    .on_hover_text("指定作品、说它没有发行版，连汉化组、版本那几样事实一起记下。先出计划再动手。")
                    .clicked();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    按下.undo = ui
                        .scope(|ui| {
                            look::ghost_button(ui.visuals_mut());
                            look::key_button(ui, "撤销上一条", "U", 撤得了)
                        })
                        .inner
                        .on_hover_text("撤回这一趟最近落下的那一批裁决。更早落下的那些在「裁决记录」里撤。")
                        .clicked();
                });
            });
        });
        ui.add_space(缝);
        key_hints(ui);
        self.matches_ui(ui, site, &key);
        if let Some(j) = 点了候选 {
            self.nth = j;
        }
        if 按下.pass {
            self.pass_here(site);
        }
        if 按下.reject {
            self.reject_here(site);
        }
        if 按下.set_aside {
            self.set_aside_here();
        }
        if 按下.manual {
            self.manual_open = true;
        }
        if 按下.undo {
            self.undo_last(site);
        }
    }

    /// 「通过所选候选」（键盘 `Y`）：采用眼下切到的那条候选，**只裁光标底下这一条**，当场落下。
    fn pass_here(&mut self, site: &mut Site) {
        let candidates = self
            .queue
            .selected()
            .get(self.at)
            .map_or(0, |item| item.candidates.len());
        if candidates > 0 {
            self.decide_here(
                site,
                &Draft {
                    pick: Some(self.nth.min(candidates - 1) + 1),
                    ..Draft::default()
                },
            );
        } else {
            // **不许什么都不做还不吭声**：队列里一条候选都没有的是常态，
            // 那时 `Y` 无从采用——说清楚该走哪条路，而不是让人以为键盘坏了。
            self.error = Some(
                "这一条一条候选都没有，`Y` 没什么可采用的——「手工指定…」指定作品，\
                 或者 `N` 记成「我看过了，认不出」。"
                    .to_string(),
            );
        }
    }

    /// 「都不对」（键盘 `N`）：记成「我看过了，认不出」，只裁光标底下这一条，当场落下。
    fn reject_here(&mut self, site: &mut Site) {
        self.decide_here(
            site,
            &Draft {
                unknown: true,
                ..Draft::default()
            },
        );
    }

    /// 「先放着」（键盘 `空格`）：光标往下走一条。
    ///
    /// **「先放着」什么都不写**——它与识别结论那一档「跳过」不是一回事，后者说的是「不该撞 DAT」，这里说的是
    /// 「这一条我等会儿再看」（`CONTEXT.md` 的**先放着**与**跳过**两条词条）。
    fn set_aside_here(&mut self) {
        self.move_to(self.at + 1);
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
        let tokens = Tokens::builtin();
        ui.add_space(tokens.space.detail_gap);
        // 设计稿写「中文离线源的这次匹配」（一堆的样子）；这个变体身上撞出好几堆时说「那几次」。
        let 几堆 = self
            .matches
            .as_ref()
            .map_or(0, |matched| matched.groups.len());
        let 几个字段 = self
            .matches
            .as_ref()
            .and_then(|matched| matched.groups.first())
            .map_or(0, |group| group.values.len());
        ui.horizontal(|ui| {
            look::section(
                ui,
                if 几堆 > 1 {
                    "中文离线源那几次匹配"
                } else {
                    "中文离线源的这次匹配"
                },
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if 几堆 == 1 {
                    look::help(
                        ui,
                        &format!("一次裁决管住这次匹配带来的全部 {几个字段} 个字段"),
                    );
                } else if 几堆 > 1 {
                    look::help(ui, "一次裁决管住那一次匹配带来的全部字段");
                }
            });
        });
        ui.add_space(look::step(1));
        if let Some(账) = &账 {
            ui.colored_label(ui.visuals().warn_fg_color, 账);
            ui.add_space(look::step(1));
        }
        if 空 {
            return;
        }
        // 底下那句说的是那两颗按钮：这个变体身上一堆裁得动的都没有（全在作品那一层）时不画。
        let 裁得动 = self
            .matches
            .as_ref()
            .is_some_and(|matched| matched.groups.iter().any(|group| group.from_variant));
        let 按下 = match &self.matches {
            Some(matched) => match_groups_ui(ui, &matched.groups, &mut self.match_note),
            None => None,
        };
        if 裁得动 {
            look::help(
                ui,
                "「不是这条」会一起清除上面全部字段。匹配裁决不在裁决记录的批里，改主意时再裁一次即可覆盖。",
            );
        }
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
            &site.library_identity,
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

    /// 「手工指定…」那一层弹层（拿主意的人 2026-09-15 定，原来是逐条那一屏底下那块面板的右半）：只裁选中的这一条、
    /// 裁成哪一种、作品与那几样事实、备注（[`Screen::form_ui`]），页脚「取消 ｜ 预览这一批」。
    ///
    /// **先出计划再动手**：「预览这一批」走的是原来那条路（[`Screen::preview`]）——这一层关上，计划书那一层打开。
    fn manual_dialog(&mut self, ctx: &egui::Context, site: &mut Site) {
        /// 这一层页脚上按下去的是哪一颗。
        enum Pressed {
            /// 「取消」：退出那一颗，Esc 等于按它。
            Cancel,
            /// 「预览这一批」。
            Preview,
        }
        let draft = self.form.draft();
        let complaint = draft.check().err();
        let 选中 = self.queue.selected().len();
        let footer = dialog::Footer::new(dialog::Button::new("取消", Pressed::Cancel)).button(
            dialog::Button::new("预览这一批", Pressed::Preview)
                .primary()
                .enabled(complaint.is_none() && 选中 > 0)
                .hover("先出计划再动手：一条命令改几百条记录，看不见就按下去，错了没处找。"),
        );
        let shown = dialog::Dialog::new("手工指定", "手工指定", footer)
            .note(format!("作用于选中的 {} 条。", thousands_len(选中)))
            .width(dialog::Width::Wide)
            .show(ctx, |ui| {
                self.form_ui(ui);
                if let Some(complaint) = &complaint {
                    ui.colored_label(ui.visuals().warn_fg_color, complaint);
                }
            });
        match shown.pressed {
            Some(Pressed::Preview) => {
                self.manual_open = false;
                self.preview(site, &draft);
            }
            Some(Pressed::Cancel) => self.manual_open = false,
            None => {}
        }
    }

    /// 「筛选…」那一层弹层（拿主意的人 2026-09-15 定，原来是逐条那一屏左边那栏）：识别结论四个勾；三个轴各一个文本框，
    /// 底下列着这个轴上最大的几组——**点一组就是一条覆盖几百条的选择器**。页脚「完成 ｜ 清除筛选」。
    /// **这几个文本框都会碰到输入法**，它们全在这一层里（ADR-0005）。
    fn filter_dialog(&mut self, ctx: &egui::Context) {
        /// 这一层页脚上按下去的是哪一颗。
        enum Pressed {
            /// 「完成」：退出那一颗，Esc 等于按它。
            Done,
            /// 「清除筛选」：三个轴都清空、一级那一批也放掉，回到整个队列。
            Clear,
        }
        let footer = dialog::Footer::new(dialog::Button::new("完成", Pressed::Done)).button(
            dialog::Button::new("清除筛选", Pressed::Clear)
                .hover("三个轴都清空、放掉只看的那一批变体，回到整个队列。"),
        );
        let shown = dialog::Dialog::new("筛选队列", "筛选", footer)
            .note(format!(
                "选中 {} 条。",
                thousands_len(self.queue.selected().len())
            ))
            .width(dialog::Width::Wide)
            .show(ctx, |ui| {
                if let Some(only) = &self.picks.only {
                    look::help(ui, &format!("只看：{}", only.label()));
                }
                look::section(ui, "按识别结论");
                ui.horizontal_wrapped(|ui| {
                    for (at, state) in State::ALL.iter().enumerate() {
                        ui.checkbox(&mut self.picks.states[at], state.label());
                    }
                });
                for axis in Axis::ALL {
                    ui.add_space(look::step(2));
                    look::section(ui, axis.label());
                    let 宽 = ui.available_width();
                    look::text_input(
                        ui,
                        宽,
                        egui::TextEdit::singleline(self.picks.text_mut(axis))
                            .hint_text(axis.hint()),
                    )
                    .on_hover_text(format!("命令行上是 `{}`", axis.selector()));
                    let rows = self.queue.groups(axis);
                    if rows.is_empty() {
                        look::help(ui, empty_axis(axis));
                        continue;
                    }
                    let 没列 = rows.len().saturating_sub(TOP);
                    let mut clicked = None;
                    ui.horizontal_wrapped(|ui| {
                        for row in rows.iter().take(TOP) {
                            let label = if row.label.is_empty() {
                                "（主库根）"
                            } else {
                                row.label.as_str()
                            };
                            let on = self.picks.holds(axis, &row.label);
                            if ui
                                .selectable_label(
                                    on,
                                    format!("{}  {}", thousands(row.count), label),
                                )
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
                    });
                    if 没列 > 0 {
                        look::help(ui, &format!("……另有 {} 组没列", thousands_len(没列)));
                    }
                    if let Some(label) = clicked {
                        self.picks.pick(axis, &label);
                    }
                }
            });
        match shown.pressed {
            Some(Pressed::Done) => self.filter_open = false,
            Some(Pressed::Clear) => {
                self.picks.clear_axes();
                self.picks.only = None;
            }
            None => {}
        }
    }

    /// 「手工指定…」那一层里的表单：只裁选中的这一条、裁成哪一种、作品与那几样事实、备注。
    /// **这里的每一个文本框都会碰到输入法**，它们全在弹层里（ADR-0005）。
    fn form_ui(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.only_picked, "只裁选中的这一条")
            .on_hover_text(
                "「采用第 N 条候选」天生是逐条的动作：同一批里各人的候选不是同一部游戏。",
            );
        ui.scope(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(font::strong("裁成"));
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
        });
    }

    /// **逐条键盘流**：`←` `→` 切候选、`Y` 过、`N` 拒、`空格` 先放着、`U` 撤销上一条。
    ///
    /// 光标在文本框里时一个键都不接——那一栏里正打着中文，`Y` 是用户要的字母不是命令。
    ///
    /// **有一层弹层开着时也一个键都不接**——计划书就是一层。egui 的 [`egui::Modal`] 只拦得住
    /// 指针、拦不住键盘（0.36），于是计划书开着按 `N` 会当场落下光标那一条——而屏上那份
    /// 计划书还写着排它时的那一批，人再点「落下」时它已经过期了。那道门在弹层那一处
    /// （[`dialog::screen_has_keys`]），不在这一屏自己记着「计划书开着没有」：别处开出来的
    /// 弹层盖在这一屏上头时，照样得拦（[`Screen::drop_stale_plan`] 说了它与核心库那道门各管
    /// 各的什么）。
    fn keyboard(&mut self, ctx: &egui::Context, site: &mut Site) {
        if !dialog::screen_has_keys(ctx) || ctx.egui_wants_keyboard_input() {
            return;
        }
        let (mut pass, mut reject, mut set_aside, mut undo, mut back, mut forth) =
            (false, false, false, false, false, false);
        ctx.input(|input| {
            pass = input.key_pressed(egui::Key::Y);
            reject = input.key_pressed(egui::Key::N);
            set_aside = input.key_pressed(egui::Key::Space);
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
        if set_aside {
            self.set_aside_here();
        }
        if pass {
            self.pass_here(site);
        }
        if reject {
            self.reject_here(site);
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
        let decide = match draft.build(&site.library_identity) {
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
                self.receipt = Some(Receipt::applied(Verdicted::of(draft), &applied));
                self.applied = Some(applied);
                self.undone = None;
                self.changed = true;
                self.refresh_records(site);
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
                note: Self::scope_note(scope),
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
                note: Self::scope_note(scope),
                ..Draft::default()
            },
        );
    }

    /// 下钻那一层落下时记在那一批裁决上的**那一句为什么**：作用范围原话
    /// （[`Scope::label`]，形如「MAME / gameboy.xml / 含头 · 按目录 GB/汉化/」）。
    ///
    /// **裁决记录里那一行要认得出这是哪一组**（设计稿 `passPart` 给它的 label 带着同一段）：
    /// 挡住换轴那句话让人「先在裁决记录中撤销那几批」，而记录上只写「第 N 批裁决 · M 条」
    /// 的话，人根本挑不出该撤哪几批。整批那一层不写——那一行本来就是整批，没有可补的。
    fn scope_note(scope: &Scope) -> Option<String> {
        scope.drill.is_some().then(|| scope.label())
    }

    /// 给一个范围排一次计划。**整批操作按下去走的就是它。**
    fn preview_scope(&mut self, site: &mut Site, scope: &Scope, draft: &Draft) {
        self.applied = None;
        match draft.build(&site.library_identity).and_then(|decide| {
            self.queue
                .plan_scope(&site.catalog, &site.store, &decide, scope)
                .map_err(|error| format!("排不出计划：{error}"))
        }) {
            Ok(plan) => {
                self.error = None;
                // **记不记由核心库说**：整批那一层落下之后这一批整个从队列里消失，屏上没有
                // 「剩下的部分」可说，[`Parts::record`] 对它一个字都不记——所以这里一律把
                // 范围交下去，不在界面层再拼一道同样的判断。
                // 条数趁现在数：落下之后这一组就空了，再数是零。
                let drilled = Some(Drilled::new(scope.clone(), self.queue.count(scope)));
                self.pending = Some(self.hold(plan, Verdicted::of(draft), drilled));
            }
            Err(message) => self.error = Some(message),
        }
    }

    /// 排一次计划。**界面上「预览这一批」按下去走的就是它。**
    pub fn preview(&mut self, site: &mut Site, draft: &Draft) {
        self.applied = None;
        match draft.build(&site.library_identity).and_then(|decide| {
            self.queue
                .plan(&site.catalog, &site.store, &decide)
                .map_err(|error| format!("排不出计划：{error}"))
        }) {
            Ok(plan) => {
                self.error = None;
                self.pending = Some(self.hold(plan, Verdicted::of(draft), None));
            }
            Err(message) => self.error = Some(message),
        }
    }

    /// 把刚排出来的计划挂起来，**记下它是照着哪一版队列排的**，以及它作用在下钻出来的
    /// 哪一组上、那一组当时有多少条。
    fn hold(&self, plan: Plan, kind: Verdicted, drilled: Option<Drilled>) -> Pending {
        Pending {
            plan,
            revision: self.queue.revision(),
            kind,
            drilled,
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
        /// 计划书页脚上按下去的是哪一颗。
        enum Pressed {
            /// 「取消」：退出那一颗，Esc 等于按它。
            Cancel,
            /// 「落下」。
            Apply,
        }
        let (plan, kind, drilled) = (pending.plan, pending.kind, pending.drilled);
        // **一层弹层**（[`dialog`]）：说明是那句总账，内容区是明细，页脚「取消 ｜ 落下」。
        // 明细不再自己套一层滚动区——弹层的内容区本来就滚得动，页脚一直在屏上。
        let note = format!(
            "要落下 {} 条：钉在内容上的 {} 条（可导出分享），只钉得住本机路径的 {} 条；\
             其中盖掉已有裁决的 {} 条。",
            thousands(plan.decided.len() as u64),
            thousands(plan.content_anchored() as u64),
            thousands(plan.path_anchored() as u64),
            thousands(plan.replacing() as u64),
        );
        let footer = dialog::Footer::new(dialog::Button::new("取消", Pressed::Cancel)).button(
            dialog::Button::new("落下", Pressed::Apply)
                .primary()
                .enabled(!plan.decided.is_empty()),
        );
        let shown = dialog::Dialog::new("裁决计划", "批量裁决计划", footer)
            .note(note)
            .show(ctx, |ui| {
                if !plan.blocked.is_empty() {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!("{} 条落不下去：", thousands(plan.blocked.len() as u64)),
                    );
                }
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
        match shown.pressed {
            Some(Pressed::Apply) => self.apply_plan(site, &plan, kind, drilled),
            Some(Pressed::Cancel) => {}
            None => {
                self.pending = Some(Pending {
                    plan,
                    revision: pending.revision,
                    kind,
                    drilled,
                });
            }
        }
    }

    /// 落下等着的那份计划。**模态框里「落下」按下去走的就是它。**
    pub fn commit(&mut self, site: &mut Site) {
        if let Some(pending) = self.pending.take() {
            self.apply_plan(site, &pending.plan, pending.kind, pending.drilled);
        }
    }

    /// 真的落下：写沉淀库、当场在中立库里兑现、把裁完的从队列里去掉。
    ///
    /// 落的是**下钻出来的那一组**时还多一步：把这一部分记进 [`Parts`]。不记的话它那些条
    /// 一落下就从队列里消失，屏上那一项跟着不见——人只会以为自己刚才什么也没做。
    fn apply_plan(
        &mut self,
        site: &mut Site,
        plan: &Plan,
        kind: Verdicted,
        drilled: Option<Drilled>,
    ) {
        match self.queue.apply(&mut site.catalog, &mut site.store, plan) {
            Ok(applied) => {
                self.error = None;
                self.receipt = Some(Receipt::applied(kind, &applied));
                if let Some(drilled) = drilled
                    && let Some(kind) = kind.part_kind()
                    // **记不记由核心库说**：整批那一层它一个字都不记，交回 `false`。
                    && self
                        .parts
                        .record(drilled.scope, drilled.count, kind, applied.batch)
                {
                    // 这一组裁完了，就地那一框跟着收起：作用范围回到整批，
                    // 底下那两颗按钮写的就是「剩余的 N 条」。
                    self.drill = None;
                }
                self.applied = Some(applied);
                self.undone = None;
                self.changed = true;
                self.cursor = None;
                self.refresh_records(site);
            }
            // 计划过期那一句核心库已经说全了（连「两份库一个字都没动」都在里面），
            // 再前缀一句「写不进去」反而让人以为是库出了毛病。
            Err(error @ TriageError::StalePlan { .. }) => self.error = Some(error.to_string()),
            Err(error) => self.error = Some(format!("裁决写不进去：{error}")),
        }
    }

    /// **撤销一批裁决**——裁决记录里的任意一批，不只是刚落下的那一批。两边一起回到它
    /// 落下之前，队列当场重列，那一批里的变体当场回到队列里。
    ///
    /// **界面上点裁决记录里那颗「撤销」走的就是它**，实测与测试拿它当那一下。
    ///
    /// 领域判断一条都不在这里——[`Queue::undo`] 走的是命令行 `romcat triage undo --batch`
    /// 那条同一条路（ADR-0005）：没这一批、已经撤过了、被后来还在册的一批盖住了，全由
    /// 核心库拒下，这一层只把按下去的那一下转过去，再把账或那句话画出来。
    pub fn undo(&mut self, site: &mut Site, id: i64) {
        match self.queue.undo(
            &mut site.catalog,
            &mut site.store,
            &site.library_identity,
            id,
        ) {
            Ok(account) => {
                // 撤的正是「刚落下」的那一批，那一笔账就不作数了——留着的话，`U` 会把同一批再撤一次。
                // 撤的是别的批时，那一笔照旧是实话。
                if self.applied.is_some_and(|applied| applied.batch == id) {
                    self.applied = None;
                }
                self.receipt = Some(Receipt::undone(&account));
                self.undone = Some(account);
                self.after_roll(site);
            }
            Err(error) => self.refuse(format!("撤不掉：{error}")),
        }
    }

    /// 撤销或放回做成之后，这一屏跟着换的那几样。
    ///
    /// 两条路（[`Queue::undo`] 与 [`Queue::redo`]）都把队列整份重列过了：屏头那个数跟着
    /// 队列换、裁决记录重列、选择器写回去、光标放掉，再给浏览屏留个记号。
    fn after_roll(&mut self, site: &Site) {
        self.error = None;
        self.records_refusal = None;
        self.changed = true;
        self.cursor = None;
        self.queue.set_filter(self.picks.filter());
        self.not_run = self.queue.not_run();
        self.refresh_records(site);
    }

    /// 撤销或放回被核心库拒下：那句话原样记下，整屏那一处与裁决记录那一块各画一份。
    fn refuse(&mut self, message: String) {
        self.records_refusal = Some(message.clone());
        self.error = Some(message);
    }

    /// **撤回刚落下的那一批**：逐条流里的 `U` 与「撤销上一条」走的是它，撤走的是 [`Screen::undo`] 那条同一条路。
    ///
    /// 「刚落下」说的是**本进程里最近一次落下**的那一批——在裁决记录里放回一批也算一次落下
    /// （[`Screen::redo`]），放回之后 `U` 撤的就是它（挂单 `Q621`）。
    ///
    /// 没有可撤的那一批时**报一句**再回来：一声不吭地返回，人只会以为键盘坏了（`Y` 那一支写着同一句话，
    /// 命令行 `undo --last` 无批时也报错退 1）。
    pub fn undo_last(&mut self, site: &mut Site) {
        let Some(id) = self.applied.map(|applied| applied.batch) else {
            self.error = Some(
                "这一趟还没落下过一批裁决，`U` 没什么可撤的——先 `Y` 采用或 `N` 拒绝一条。\
                 更早落下的那些在「裁决记录」里撤：屏头右侧那颗按钮，每一批旁边都有一颗撤销。"
                    .to_string(),
            );
            return;
        };
        self.undo(site, id);
    }

    /// **把撤掉的一批裁决放回去**——裁决记录里任意一批撤过的，不只是刚撤掉的那一批。
    /// 撤销本身撤得回来：命令行 `romcat triage redo --batch` 吃的也是任意一批，界面上
    /// 做不到的话，撤错了更早的那一批就得回终端（ADR-0023 修订段那条判据）。
    ///
    /// **界面上点裁决记录里那颗「放回」走的就是它。** 放不回去（被后来还在册的一批盖住、
    /// 锚上眼下不是它撤掉时留下的样子）由核心库整份拒下（[`Queue::redo`]），这一层把
    /// 那句话原样画出来。
    pub fn redo(&mut self, site: &mut Site, id: i64) {
        match self.queue.redo(
            &mut site.catalog,
            &mut site.store,
            &site.library_identity,
            id,
        ) {
            Ok(account) => {
                // 与 [`Screen::undo`] 对称：放回的正是「刚撤回」的那一批，那一笔才不作数。
                if self.undone.is_some_and(|undone| undone.batch == id) {
                    self.undone = None;
                }
                // **放回也是一次落下**：本进程里最近落下的就是它，`U` 从此撤的是这一批——与 `undo` 把撤掉的
                // 那一批记成「刚撤回」是同一个对称。
                self.receipt = Some(Receipt::redone(&account));
                self.applied = Some(account);
                self.after_roll(site);
            }
            Err(error) => self.refuse(format!("放不回去：{error}")),
        }
    }

    /// **把刚撤掉的那一批放回去**：放回走的是 [`Screen::redo`] 那条同一条路。
    pub fn redo_last(&mut self, site: &mut Site) {
        let Some(id) = self.undone.map(|undone| undone.batch) else {
            return;
        };
        self.redo(site, id);
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
    /// 这份计划裁成哪一类：落下之后提示条上说「已通过」还是「已拒绝」（[`Verdicted`]）。
    kind: Verdicted,
    /// 这份计划作用在**下钻出来的哪一组**上；整批那一层与逐条那一路都是 `None`。
    drilled: Option<Drilled>,
}

/// 一份计划作用在**下钻出来的哪一组**上，连那一组**落下之前**有多少条。
///
/// 两样捆在一起，因为它们从排计划到落下一路同行（[`Screen::preview_scope`] → [`Screen::hold`]
/// → [`Pending::drilled`] → [`Screen::apply_plan`]），而且**只有凑齐了才记得成一部分**
/// （[`Parts::record`] 两样都要）。拆成一对散值传的话，四个签名各拆一遍元组，
/// 谁也说不出那个 `u64` 数的是什么。
///
/// **条数在排计划那一刻就记下**，不拿落下之后那笔账里的「落了几条」当它：几个变体共享同一条
/// **内容锚**（几份**重复拷贝**）时落成的是同一条裁决，那个数会小于这一组的变体数——而屏上
/// 那一项写的、占比条量的都是**变体**有多少条。
///
/// **跟着计划走，不在落下那一刻读屏上眼下的作用范围**：计划书开着的那段时间里人换得了下钻。
#[derive(Debug, Clone)]
struct Drilled {
    /// 哪一批下面的哪一组。
    scope: Scope,
    /// 那一组落下之前有多少条**变体**。
    count: u64,
}

impl Drilled {
    fn new(scope: Scope, count: u64) -> Self {
        Self { scope, count }
    }
}

/// 一次裁决裁成哪一类，只为提示条上那一句用（设计稿 `toast(\`已${kind} …\`)`）。**判断在草稿里**（[`Draft`]），这里只折成词。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdicted {
    /// 采用候选：「全部通过」、逐条的 `Y`。
    Passed,
    /// 记成「我看过了，认不出」：「全部拒绝」、逐条的 `N`。
    Rejected,
    /// 手工指定作品、确认没有发行版：「手工指定…」那一层。
    Decided,
}

impl Verdicted {
    /// 这份草稿裁成哪一类。
    fn of(draft: &Draft) -> Self {
        if draft.pick.is_some() {
            Self::Passed
        } else if draft.unknown {
            Self::Rejected
        } else {
            Self::Decided
        }
    }

    /// 提示条上那个动词。
    ///
    /// 前两档**从核心库取**（[`PartKind::label`]）：屏上那一项旁边那枚标签写的是同两个词，
    /// 各存一份字面量的话，改一处另一处就跟着说岔了。
    fn word(self) -> &'static str {
        match self.part_kind() {
            Some(kind) => kind.label(),
            None => "已裁决",
        }
    }

    /// 折成**一部分**裁成了什么（[`PartKind`]）；「手工指定」那一档折不出来——
    /// 它天生是逐条的动作，作用不到一整组上。
    fn part_kind(self) -> Option<PartKind> {
        match self {
            Self::Passed => Some(PartKind::Passed),
            Self::Rejected => Some(PartKind::Rejected),
            Self::Decided => None,
        }
    }
}

/// 屏底那条提示条连它那颗按钮按下去走哪一步（[`Screen::receipt_ui`]）。
#[derive(Debug, Clone)]
struct Receipt {
    /// 那条提示条。
    toast: Toast,
    /// 按钮按下去走回哪一步。
    back: Back,
}

/// 提示条上那颗按钮走回的那一步。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Back {
    /// 「撤销」这一批。
    Undo(i64),
    /// 「放回」这一批。
    Redo(i64),
}

impl Receipt {
    /// 落下一批之后：「已通过 N 条」，带「撤销」（设计稿 `passBatch` 那一条）。
    fn applied(kind: Verdicted, applied: &Applied) -> Self {
        Self {
            toast: Toast::new(format!(
                "{} {} 条",
                kind.word(),
                thousands(applied.verdicts)
            ))
            .action("撤销"),
            back: Back::Undo(applied.batch),
        }
    }

    /// 撤掉一批之后：「已撤销，N 个变体回到待确认队列」，带「放回」（设计稿 `undoLot` 那一条）。
    ///
    /// **「中立库那一半回没回去」必须说出口**：没回去（快照随重跑识别清掉了），人得再跑一趟识别才看得见——两种情形说同一句话是撒谎。
    fn undone(undone: &Undone) -> Self {
        let mut text = if undone.catalog_rolled_back {
            format!(
                "已撤销，{} 个变体回到待确认队列",
                thousands(undone.variants)
            )
        } else {
            format!(
                "已撤销 {} 条；这一批之后跑过识别，要让它们回到队列请再跑一趟识别",
                thousands(undone.removed)
            )
        };
        if undone.kept > 0 {
            let _ = write!(
                text,
                "；另有 {} 条没动——同一条锚上后来有人重新裁过",
                thousands(undone.kept)
            );
        }
        Self {
            toast: Toast::new(text).action("放回"),
            back: Back::Redo(undone.batch),
        }
    }

    /// 放回一批之后：「已放回 N 条」，带「撤销」。
    fn redone(applied: &Applied) -> Self {
        Self {
            toast: Toast::new(format!("已放回 {} 条", thousands(applied.verdicts))).action("撤销"),
            back: Back::Undo(applied.batch),
        }
    }
}

/// 展开那一批算出来的那几样是**照着什么**算的。五样凑齐才认得出「这一份还作数吗」。
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
    /// 已经裁完的那几部分换过几次样子（[`Parts::revision`]）：细分那一栏上哪几项标着
    /// 「已通过」由它定，而落下一部分、撤掉一部分都不必然换掉队列那一版
    /// （撤销之后队列重列过，落下之后也是——但这一格自己变，缓的那份才不会漏掉）。
    parts: u64,
}

/// 展开那一批算出来的那几样：条数、细分那一栏、两处随机样本。
///
/// **一起缓**，因为它们出自同一次「走一遍这一批」；分开缓的话，谁先谁后失效
/// 会让屏上那几样各说各的——「全部通过（3,053 条）」底下摆的是另一批的样本。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Opened {
    /// 这一份是照着什么算出来的。
    basis: Basis,
    /// **下钻着的那一组**有多少条——就地那一框里写的、那两颗按钮上写的都是它。
    /// 没下钻时它等于 [`Breakdown::left`]（两边都是「这一批还剩多少」）。
    count: u64,
    /// **细分那一栏画的就是它**：各项、占比、哪几项已经就地裁完、还剩多少条、锁没锁住轴，
    /// 连「加不加得起来」那句话。
    breakdown: Breakdown,
    /// **整批**的随机样本：右栏那一栏画的。下钻不收窄它——照稿，右栏说的始终是这一批。
    samples: Vec<Sample>,
    /// **下钻着的那一组**的样本：就地那一框里摆的。没下钻时是空的。
    ///
    /// 不叫 `part_samples`：词表**批**那一条里的**一部分**专指已经整批裁过的那一组，
    /// 而这几条样本是给人看**还没裁**的那一组的。
    drilled_samples: Vec<Sample>,
}

/// **一堆来自同一次匹配的字段**：条目号、依据、那几个值，连底下那两颗按钮。
///
/// 返回「这一帧按下了哪一堆的哪一档」——`(条目号, 就是这条吗)`。
///
/// 收成自由函数是因为借用：这一块要**读**缓着的那几堆（`&self.matches`），而按下去
/// 之后要**改**两份库；一个 `&mut self` 上过不去，也不该为了过去而把那几堆整份克隆
/// 一遍（一条简介 4,000 字，每帧一份）。
fn match_groups_ui(
    ui: &mut egui::Ui,
    groups: &[MatchGroup],
    note: &mut String,
) -> Option<(u32, bool)> {
    let tokens = Tokens::builtin();
    let palette = look::palette(ui);
    let 线宽 = tokens.layout.control_stroke;
    let 圆角 = tokens.radius.large;
    let mut 按下 = None;
    for group in groups {
        // 左沿那一道色（设计稿 `.mgroup` 的 `inset 3px`）：人确认过的是高置信那一色，还等着裁的是中置信那一色。
        let 色 = if group.confirmed {
            palette.hi
        } else {
            palette.mid
        };
        let 整块 = look::barred_card(ui, 色, look::BarEdge::Left, |ui| {
                // ——— 头：条目号、还等着裁没有、两颗按钮 ———
                let [头上下, 头左右] = tokens.space.match_head_padding;
                let 头 = egui::Frame::new()
                    .fill(palette.panel_2)
                    .inner_margin(egui::Margin::from(egui::vec2(头左右, 头上下)))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            // **条目号写在堆上**：它就是「同一次匹配」的判据，人要去数据源核对时，
                            // 那也是唯一查得回去的东西。
                            ui.label(
                                egui::RichText::new(format!("条目 {}", group.entry))
                                    .family(egui::FontFamily::Monospace)
                                    .color(palette.ink),
                            );
                            if group.confirmed {
                                look::plain_chip(ui, look::Tone::Good, "已由人裁决确认");
                            } else {
                                // **口径不放松**：模糊匹配来的仍是中置信、仍进待确认队列（ADR-0002）。
                                look::plain_chip(ui, look::Tone::Caution, "还等着裁")
                                    .on_hover_text("模糊匹配来的，中置信，不自动通过。");
                            }
                            if group.from_variant {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    look::small_buttons(ui, |ui| {
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
                                        if ui
                                            .scope(|ui| {
                                                look::primary_button(ui.visuals_mut());
                                                ui.button("就是这条")
                                            })
                                            .inner
                                            .on_hover_text(
                                                "这一次匹配带来的全部字段一并定下：下一趟刮削把它们的依据改写成\
                                                 「由人工裁决确认过」，不再进待确认队列。一个字都不清。",
                                            )
                                            .clicked()
                                        {
                                            按下 = Some((group.entry, true));
                                        }
                                    });
                                });
                            }
                        });
                    });
                ui.painter().hline(
                    头.response.rect.x_range(),
                    头.response.rect.bottom(),
                    egui::Stroke::new(线宽, palette.line),
                );
                // ——— 那几个字段：字段名、落在哪一层，值 ———
                let [行上下, 行左右] = tokens.space.match_row_padding;
                let [行竖, 行横] = tokens.space.match_row_gap;
                let 字号 = look::font_size(ui.ctx(), tokens.font.size_small_plus);
                let 层字号 = look::font_size(ui.ctx(), tokens.font.size_mini);
                egui::Frame::new()
                    .inner_margin(egui::Margin::from(egui::vec2(行左右, 行上下)))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        egui::Grid::new(("中文离线源那一堆", group.entry))
                            .num_columns(2)
                            .min_col_width(tokens.layout.match_key_width)
                            .spacing([行横, 行竖])
                            .show(ui, |ui| {
                                for value in &group.values {
                                    // **锚点那一层写出来**：同一堆里变体那几条与作品那几条，下一趟重跑时的
                                    // 去向完全不同（作品那一层按名下变体数票，见 `zh::judge` 的文档）。
                                    let (层, 落在) = match value.kind {
                                        AnchorKind::Variant => ("变体层", "变体".to_owned()),
                                        AnchorKind::Work => {
                                            ("作品层", format!("作品「{}」", value.subject))
                                        }
                                    };
                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = 0.0;
                                        ui.label(
                                            egui::RichText::new(value.field.label())
                                                .size(字号)
                                                .color(palette.ink_3),
                                        );
                                        ui.label(
                                            egui::RichText::new(层).size(层字号).color(palette.ink_3),
                                        );
                                    });
                                    let 值 = one_line(&value.value);
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(
                                                值.as_deref().unwrap_or(value.value.as_str()),
                                            )
                                            .size(字号)
                                            .color(palette.ink),
                                        )
                                        .wrap(),
                                    )
                                    .on_hover_text(format!(
                                        "{}\n落在{落在}，来源 {}",
                                        value.value, value.source
                                    ));
                                    ui.end_row();
                                }
                            });
                    });
                // ——— 依据（一堆里逐字一样的只写一遍，拿主意的人 2026-09-15 定），底下那一格备注 ———
                let mut 依据们: Vec<&str> = Vec::new();
                for value in &group.values {
                    if !依据们.contains(&value.evidence.as_str()) {
                        依据们.push(&value.evidence);
                    }
                }
                egui::Frame::new()
                    .inner_margin(egui::Margin {
                        left: 行左右 as i8,
                        right: 行左右 as i8,
                        top: 0,
                        bottom: 行上下 as i8,
                    })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = look::step(0);
                        let 小字 = look::font_size(ui.ctx(), tokens.font.size_small);
                        for 依据 in &依据们 {
                            let mut job = egui::text::LayoutJob::default();
                            for (段, 字色) in [("依据：", palette.ink), (*依据, palette.ink_3)] {
                                job.append(
                                    段,
                                    0.0,
                                    egui::TextFormat {
                                        font_id: egui::FontId::proportional(小字),
                                        color: 字色,
                                        ..egui::TextFormat::default()
                                    },
                                );
                            }
                            ui.label(job);
                        }
                        if group.from_variant {
                            ui.horizontal(|ui| {
                                look::help(ui, "备注");
                                let 宽 = ui.available_width();
                                look::small_text_input(
                                    ui,
                                    宽,
                                    egui::TextEdit::singleline(note)
                                        .hint_text("半年后你会想知道当初凭什么这么定"),
                                )
                                .on_hover_text(
                                    "写在这里的话跟着你按下的那一下记进裁决；命令行上是 `--note`。",
                                );
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
                    });
            })
            .response
            .rect;
        ui.painter().rect_stroke(
            整块,
            圆角,
            egui::Stroke::new(线宽, palette.line),
            egui::StrokeKind::Inside,
        );
        ui.add_space(look::step(2));
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
             屏上那一堆仍写着「还等着裁」——那句话在依据里，下一趟刮削才改写",
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
        "。改主意就再裁一次：这一条不在裁决记录那条路上（那条撤的是识别那边的一批裁决），\
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
             再采一趟刮削——浏览屏屏头那颗「刮削…」。",
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

/// 正文头上那三格（设计稿 `.qsum`）：前几批可直接批量处理的、有多个候选的、没有候选的。
///
/// 这三个数是这一屏存在的理由本身：18,241 条按 5 秒一条是 25 小时，而**前几批一次就能处理掉一大截**。**数全是核心库算的**
/// （[`romcat_core::triage::batch::coverage`]；头一格与库屏工序段裁决那一行「前 N 批可一次处理 N 个」是同一个数），这里只摆
/// ——界面只画和转发（ADR-0005）。头一格描强调色（设计稿 `.qcell.lead`：一道描边加一道内描边）。
fn summary_cells(ui: &mut egui::Ui, 账: &Coverage) {
    let tokens = Tokens::builtin();
    let palette = look::palette(ui);
    let 缝 = tokens.space.queue_summary_gap;
    let [上下, 左右] = tokens.space.queue_cell_padding;
    let 线宽 = tokens.layout.control_stroke;
    let 宽 = ((ui.available_width() - 2.0 * 缝) / 3.0).max(0.0);
    let 字号 = look::font_size(ui.ctx(), tokens.font.size_small);
    let 格 = [
        (
            账.head,
            // 一批能整批通过的都没有时不写「前 0 批」：与库屏工序段同一个数，那边这时一句都不说（`Stages::detail`），
            // 这一格照稿留着，只说它是哪一类。
            if 账.head_batches > 0 {
                format!("前 {} 批可直接批量处理 · 每条只有一个候选", 账.head_batches)
            } else {
                "可直接批量处理 · 每条只有一个候选".to_owned()
            },
            true,
        ),
        (账.multiple, "有多个候选 · 需要逐条选择".to_owned(), false),
        (账.bare, "没有候选 · 无法批量处理".to_owned(), false),
    ];
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 缝;
        for (数, 说明, 领头) in 格 {
            // 外层是横排：不点名竖排的话，数与说明会挤在同一行（设计稿 `.qcell` 是上下两行）。
            ui.allocate_ui_with_layout(egui::vec2(宽, 0.0), Layout::top_down(Align::Min), |ui| {
                let 这一格 = egui::Frame::new()
                    .fill(palette.panel)
                    .stroke(egui::Stroke::new(
                        线宽,
                        if 领头 { palette.accent } else { palette.line },
                    ))
                    .corner_radius(tokens.radius.large)
                    .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(
                            egui::RichText::new(thousands(数))
                                .family(egui::FontFamily::Monospace)
                                .size(tokens.font.size_summary_count)
                                .color(palette.ink),
                        );
                        ui.label(egui::RichText::new(说明).size(字号).color(palette.ink_3));
                    });
                if 领头 {
                    ui.painter().rect_stroke(
                        这一格.response.rect.shrink(线宽),
                        tokens.radius.large.saturating_sub(1),
                        egui::Stroke::new(线宽, palette.accent),
                        egui::StrokeKind::Inside,
                    );
                }
            });
        }
    });
    ui.add_space(tokens.space.queue_summary_margin);
}

/// 「没有候选」那个虚线框（设计稿 `.bare`）：「没有候选 · N 个」、一枚「没有候选」、一颗「逐条指定」，底下一句按识别结论各多少。
/// 按下「逐条指定」返回 `true`。队列里一条没有候选的都没有时不画。
///
/// 数全是核心库交的（[`Coverage::bare`] 与 [`Coverage::bare_by_state`]）。设计稿那句后头还有一句「约三分之二位于暂不支持读取的
/// zst / rar 压缩包内」，是稿上那份示例库的实情，核心库交不出这个数，不画。
fn bare_box(ui: &mut egui::Ui, 账: &Coverage) -> bool {
    if 账.bare == 0 {
        return false;
    }
    let tokens = Tokens::builtin();
    let palette = look::palette(ui);
    ui.add_space(tokens.space.bare_margin);
    let [上下, 左右] = tokens.space.bare_padding;
    let 线宽 = tokens.layout.control_stroke;
    let mut 按 = false;
    let 框 = egui::Frame::new()
        .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                ui.label(
                    font::strong(format!("没有候选 · {} 个", thousands(账.bare)))
                        .color(palette.ink),
                );
                look::chip(ui, look::Tone::Neutral, "没有候选");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    按 = look::small_buttons(ui, |ui| {
                        ui.button("逐条指定")
                            .on_hover_text(
                                "逐条看整个队列：没有候选的那些手工指定作品，或者记成认不出。",
                            )
                            .clicked()
                    });
                });
            });
            ui.add_space(tokens.space.bare_note_gap);
            let 分布: Vec<String> = State::ALL
                .iter()
                .zip(账.bare_by_state)
                .filter(|(_, count)| *count > 0)
                .map(|(state, count)| format!("{} {} 个", state.label(), thousands(count)))
                .collect();
            ui.label(
                egui::RichText::new(format!(
                    "其中{}。它们没有可供确认的候选，因此不能批量通过。",
                    分布.join("、")
                ))
                .size(look::font_size(ui.ctx(), tokens.font.size_small))
                .color(palette.ink_3),
            );
        });
    look::dashed_outline(
        ui.painter(),
        框.response.rect,
        egui::Stroke::new(线宽, palette.line_2),
    );
    按
}

/// 一批变体的卡头（设计稿 `.bhead`）：条数（等宽、`size-batch-count`）、两行字、那一档的标签、折叠标。
///
/// 两行字（拿主意的人 2026-09-15 定）：第一行照**依据形状**各段排——有候选的是「源 / DAT / 哈希口径 / 候选数」，DAT 那一段
/// 等宽；一条候选都没有的是核心库那半截（`Shape::label`）。第二行放那句共同依据：有候选的是各条逐字一样的那一段
/// （[`Batch::evidence`]），一条候选都没有的是「为什么没定下来」；凑不出来就只有一行。
///
/// **就地裁掉过一部分的那一批，第二行先说那件事**：「已处理 N 条，剩余 M 条 · 」再接那句共同依据
/// （`裁掉的` 是核心库的 [`Parts::done_in`]，票 `gui-looks-like-the-design/19`）。左边那个大数**本来就是剩余**
/// ——它数的是眼下队列里这一批还有多少条，裁掉的那些一落下就退出了队列。不说这一句的话，屏上那个数
/// 会毫无来由地变小。
fn batch_head(ui: &mut egui::Ui, batch: &Batch, open: bool, 裁掉的: u64) {
    let tokens = Tokens::builtin();
    let palette = look::palette(ui);
    let 形状字号 = look::font_size(ui.ctx(), tokens.font.size_small_plus);
    let 第二行字号 = look::font_size(ui.ctx(), tokens.font.size_small);
    let 第二行 = match &batch.shape {
        Shape::Candidates { .. } => batch.evidence.clone(),
        Shape::Bare { reason, .. } => reason.clone(),
    };
    let 第二行 = if 裁掉的 > 0 {
        let 前缀 = format!(
            "已处理 {} 条，剩余 {} 条",
            thousands(裁掉的),
            thousands(batch.count),
        );
        Some(match 第二行 {
            Some(那句) => format!("{前缀} · {那句}"),
            None => 前缀,
        })
    } else {
        第二行
    };
    let 量 = |字体: egui::FontId| {
        ui.painter()
            .layout_no_wrap("字".to_owned(), 字体, egui::Color32::PLACEHOLDER)
            .size()
            .y
    };
    let 两行高 = 量(egui::FontId::proportional(形状字号))
        + 第二行.as_ref().map_or(0.0, |_| {
            tokens.space.batch_line_gap + 量(egui::FontId::proportional(第二行字号))
        });
    let 条数字 = egui::FontId::monospace(tokens.font.size_batch_count);
    let 高 = 两行高.max(量(条数字.clone()));
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 高),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_min_height(高);
            ui.spacing_mut().item_spacing.x = tokens.space.batch_head_gap;
            let 条数宽 = tokens.layout.batch_count_width;
            ui.allocate_ui_with_layout(
                egui::vec2(条数宽, 高),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.set_width(条数宽);
                    ui.label(
                        egui::RichText::new(thousands(batch.count))
                            .font(条数字)
                            .color(palette.ink),
                    );
                },
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                chevron(ui, open);
                look::chip(ui, look::tier_tone(batch.tier()), batch.tier().label());
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = tokens.space.batch_line_gap;
                    shape_line(ui, &batch.shape, 形状字号);
                    if let Some(字) = &第二行 {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(字)
                                    .size(第二行字号)
                                    .color(palette.ink_3),
                            )
                            .truncate(),
                        );
                    }
                });
            });
        },
    );
}

/// 卡头第一行：依据形状各段，段与段之间一道弱色的「/」（设计稿 `.shape`）。DAT 那一段等宽。
fn shape_line(ui: &mut egui::Ui, shape: &Shape, 字号: f32) {
    let tokens = Tokens::builtin();
    let palette = look::palette(ui);
    let [竖, 横] = tokens.space.shape_gap;
    let 段: Vec<(String, bool)> = match shape {
        Shape::Candidates {
            source,
            dat,
            convention,
            fanout,
            ..
        } => vec![
            (source.clone(), false),
            (dat.clone(), true),
            (convention.label().to_owned(), false),
            (fanout.label().to_owned(), false),
        ],
        Shape::Bare { .. } => vec![(shape.label(), false)],
    };
    ui.scope(|ui| {
        // 横排一行最矮是 `interact_size.y`（按钮那么高）：这一行只摆字，不让它把卡头撑高。
        ui.spacing_mut().interact_size.y = 0.0;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(横, 竖);
            for (at, (字, 等宽)) in 段.into_iter().enumerate() {
                if at > 0 {
                    ui.label(egui::RichText::new("/").size(字号).color(palette.ink_4));
                }
                let mut 这一段 = egui::RichText::new(字).size(字号).color(palette.ink);
                if 等宽 {
                    这一段 = 这一段.family(egui::FontFamily::Monospace);
                }
                ui.label(这一段);
            }
        });
    });
}

/// 卡头最右那枚折叠标（设计稿 `.chev`）：收着时朝右、展开时朝下的一个折角，弱字色。
fn chevron(ui: &mut egui::Ui, open: bool) {
    let tokens = Tokens::builtin();
    let palette = look::palette(ui);
    let 列 = tokens.layout.batch_chevron_column;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(列, 列), egui::Sense::hover());
    // 稿上是一个边长 `chevron` 的方块只描右、下两边，转 45°：两道线各长 `chevron`，横竖各走它的 1/√2。
    let 半 = tokens.layout.chevron / std::f32::consts::SQRT_2;
    let 心 = rect.center();
    let 点 = if open {
        vec![
            心 + egui::vec2(-半, -半 / 2.0),
            心 + egui::vec2(0.0, 半 / 2.0),
            心 + egui::vec2(半, -半 / 2.0),
        ]
    } else {
        vec![
            心 + egui::vec2(-半 / 2.0, -半),
            心 + egui::vec2(半 / 2.0, 0.0),
            心 + egui::vec2(-半 / 2.0, 半),
        ]
    };
    ui.painter().add(egui::Shape::line(
        点,
        egui::Stroke::new(tokens.layout.chevron_stroke, palette.ink_3),
    ));
}

/// 展开之后那一块里这一帧按下了什么。画的时候不改自己，画完再动（[`Screen::opened_card`]）。
#[derive(Debug, Default)]
struct BodyPressed {
    /// 「细分」那一排换了轴。
    axis: Option<Axis>,
    /// 「返回整批」。
    whole: bool,
    /// 点了「细分」底下的哪一组。
    into: Option<String>,
    /// 「换一组」。
    resample: bool,
    /// 底下那一排的「全部通过（N 条）」／「通过剩余的 N 条」。**作用于整批**。
    pass: bool,
    /// 底下那一排的「逐条处理」：逐条看**这一批剩下的**。
    one_by_one: bool,
    /// 就地那一框里的「逐条处理」：逐条看**下钻着的那一组**。
    ///
    /// 与上面那一格分开，不是图清楚——**合着用会被覆盖掉**。这一框先画、底下那一排后画，
    /// 而那一排是平赋值（`= …clicked()`）：框里按下的那一下会被后画的那一颗抹成 `false`，
    /// 按钮从此是死的。两格各管一处，顺带把「逐条看哪一层」也分开了。
    drilled_one_by_one: bool,
    /// 底下那一排的「全部拒绝」／「拒绝剩余的 N 条」。**作用于整批**。
    reject: bool,
    /// 就地那一框里的「通过这 N 条」。**只作用于下钻着的那一组**。
    drilled_pass: bool,
    /// 就地那一框里的「拒绝这 N 条」。**只作用于下钻着的那一组**。
    drilled_reject: bool,
}

/// 展开之后左边那一栏（设计稿 `distHTML`）：「细分」、三个轴那一排分段开关、这个轴上各项一行
/// （项名等宽、条数靠右，一行底下一条**占比条**），点一项就在那一行底下**就地**开一框（[`drill_box`]），
/// 末尾一句照稿的说明。
///
/// 各项、占比与「哪几项已经裁完」都由核心库算（[`Breakdown`]）：占比条的分母是这一批**本来**多少条，
/// 不是眼下还剩多少——拿剩下的当分母，裁掉一项之后余下那几项的条会一起变长，而它们一条都没变。
///
/// **裁完的那几项照旧在栏上**：项名划一道删除线、点不动，旁边一枚「已通过」或「已拒绝」。那些条一落下
/// 就退出了队列，不记着的话那一行当场消失——人只会以为自己刚才什么也没做。
///
/// **处理过一部分之后那一排换不动轴**：两套切法会重叠，屏上的数当场变成假话（[`Breakdown::axis_refusal`]
/// 那一句画在那一排底下）。撤掉那几批就解开。
///
/// **下钻不再收窄底下那一排按钮**（票 `gui-looks-like-the-design/19`，照稿）：这一层要落的那两下摆在就地
/// 那一框里，底下那一排说的始终是「剩下的部分」。
fn drill_column(ui: &mut egui::Ui, opened: &Opened, 色: egui::Color32, pressed: &mut BodyPressed) {
    let tokens = Tokens::builtin();
    let palette = look::palette(ui);
    let axis = opened.basis.axis;
    let 细分 = &opened.breakdown;
    let 下钻着 = opened
        .basis
        .scope
        .drill
        .as_ref()
        .map(|(_, label)| label.as_str());
    let 挡住 = 细分.axis_refusal();
    ui.style_mut().interaction.selectable_labels = false;
    ui.horizontal(|ui| {
        look::section(ui, "细分");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let 各轴 = Axis::ALL.map(|one| (one, one.label()));
            // 挡住的时候只有眼下这一颗还按得动：另两颗按下去会换出一套重叠的切法。
            if let Some(换成) =
                look::segmented_where(ui, &各轴, axis, |one| 挡住.is_none() || one == axis)
                && 换成 != axis
            {
                pressed.axis = Some(换成);
            }
        });
    });
    if let Some(那一句) = &挡住 {
        ui.add_space(look::step(1));
        look::help(ui, 那一句);
    }
    ui.add_space(look::step(1));
    if 细分.rows.is_empty() {
        look::help(ui, empty_axis(axis));
    }
    let 字号 = look::font_size(ui.ctx(), tokens.font.size_caption_plus);
    let 行高 = ui
        .painter()
        .layout_no_wrap(
            "0".to_owned(),
            egui::FontId::monospace(字号),
            egui::Color32::PLACEHOLDER,
        )
        .size()
        .y;
    // **裁完的那几项一定摆出来**：排第几由条数定，而「我刚做过那一下」不该因为它碰巧是一小项
    // 就整个看不见——那时屏上只剩卡头那句「已处理 N 条」，人找不到它落在哪一项上。
    let 摆出来: Vec<&Slice> = 细分
        .rows
        .iter()
        .take(TOP)
        .chain(细分.rows.iter().skip(TOP).filter(|row| row.done.is_some()))
        .collect();
    for row in &摆出来 {
        let 名 = if row.label.is_empty() {
            "（主库根）"
        } else {
            row.label.as_str()
        };
        let 是它 = 下钻着 == Some(row.label.as_str());
        let 裁过 = row.done;
        // **裁干净了才点不动**：裁过却还剩着的那一项（少见，有几条落不下去）照旧点得进去，
        // 把剩下的几条再裁一次。
        let 裁干净了 = row.left == 0;
        // **带标签那一行按标签那么高**：那枚标签固定 `tag-height`，比等宽字那一行高
        // 一截，照字行高分配的话它会上下串到占比条与上一行上（设计稿那张 grid 的行高
        // 本来也是被标签撑开的）。
        let 这一行的高 = if 裁过.is_some() {
            行高.max(tokens.layout.tag_height)
        } else {
            行高
        };
        let (行, 响应) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 这一行的高),
            if 裁干净了 {
                egui::Sense::hover()
            } else {
                egui::Sense::click()
            },
        );
        if 是它 || (响应.hovered() && !裁干净了) {
            ui.painter().rect_filled(
                行.expand2(egui::vec2(look::step(0), tokens.space.dist_row_gap / 2.0)),
                tokens.radius.small,
                if 是它 {
                    palette.accent_soft
                } else {
                    palette.panel_2
                },
            );
        }
        let mut 这一行 = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(行)
                .layout(Layout::right_to_left(Align::Center)),
        );
        这一行.style_mut().interaction.selectable_labels = false;
        这一行.label(
            egui::RichText::new(thousands(row.count))
                .family(egui::FontFamily::Monospace)
                .size(字号)
                .color(palette.ink_2),
        );
        if let Some(kind) = 裁过 {
            look::inline_tag(&mut 这一行, kind.label());
        }
        这一行.with_layout(Layout::left_to_right(Align::Center), |ui| {
            let 字 = egui::RichText::new(名)
                .family(egui::FontFamily::Monospace)
                .size(字号);
            // 划删除线的是**裁干净了**的那几项；裁过还剩着的照旧是条点得动的链接，
            // 「裁过」那件事由旁边那枚标签说。
            let 字 = if 裁干净了 {
                字.color(palette.ink_3).strikethrough()
            } else {
                字.color(palette.accent_ink).underline()
            };
            ui.add(egui::Label::new(字).truncate());
        });
        // 裁干净那一行分的是 `Sense::hover`，`clicked()` 在它上面永远是假——两支于是
        // 走同一条路，只有悬停那句话不一样。
        let 悬停 = match 裁过.filter(|_| 裁干净了) {
            Some(kind) => format!(
                "这一部分{}了，落成了一批裁决——在裁决记录里撤得掉，撤掉之后它回到队列里。",
                kind.verb(),
            ),
            None => "下钻：只看这一组，就在这一层整批通过或拒绝；再点一次返回整批。\
                     剩下的照旧整批处理得动。"
                .to_owned(),
        };
        if 响应.on_hover_text(悬停).clicked() {
            pressed.into = Some(row.label.clone());
        }
        // 占比条紧贴在这一行底下（设计稿 `.dist .b` 的 `margin-top:-3px` 把行距吃掉条那么高）。
        ui.add_space(tokens.space.dist_row_gap - tokens.layout.dist_bar);
        share_bar(ui, row.share(细分.whole), 色);
        if 是它 {
            drill_box(ui, opened, pressed);
        }
        ui.add_space(tokens.space.dist_row_gap);
    }
    if 细分.rows.len() > 摆出来.len() {
        look::help(
            ui,
            &format!(
                "……另有 {} 项没列",
                thousands_len(细分.rows.len() - 摆出来.len())
            ),
        );
    }
    // **加不加得起来要说出口**：只有按目录那个轴一条只落一个组。话由核心库说
    // （[`Breakdown::tally_note`]）——它数的正是屏上这几项，裁掉一部分之后两边照旧对得上。
    if let Some(那一句) = 细分.tally_note() {
        look::help(ui, &那一句);
    }
    // 稿上这一句写的是「某一部分 / 这一部分」；词表**批**那一条把**一部分**定成了
    // 「已经整批裁过的那一组」，而这句说的是还没裁的，所以照词表改口叫**组**。
    look::help(
        ui,
        "问题通常集中在某一组。点一项只看这一组，并在这一层整批通过或拒绝；剩下的仍可整批处理。",
    );
}

/// 一项底下那条**占比条**（设计稿 `.dist .b`）：凹陷底的槽、左边填 `share` 那么长的一截，
/// 高 `dist-bar`、小圆角，填的那一截取这一批那一档的色、淡到 `dist-bar-opacity`。
///
/// **它是那一行的第二遍表达**：条数已经写在右边了，这条只为「一眼看出问题集中在哪一项」——
/// 所以它的分母是整批，各项的长短之间才比得出来。
fn share_bar(ui: &mut egui::Ui, share: f64, 色: egui::Color32) {
    let tokens = Tokens::builtin();
    let palette = look::palette(ui);
    let (槽, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), tokens.layout.dist_bar),
        egui::Sense::hover(),
    );
    // 圆角取 `legend-swatch-radius`（2）：稿上 `.dist .b` 与图例那一小块写的是同一个 2px，
    // `radius.small` 是 4，语义上不是同一格。
    let 圆角 = tokens.layout.legend_swatch_radius;
    ui.painter().rect_filled(槽, 圆角, palette.sunken);
    #[allow(clippy::cast_possible_truncation)]
    let 长 = (槽.width() * share.clamp(0.0, 1.0) as f32).max(0.0);
    if 长 > 0.0 {
        ui.painter().rect_filled(
            egui::Rect::from_min_size(槽.min, egui::vec2(长, 槽.height())),
            圆角,
            色.gamma_multiply(tokens.mix.dist_bar_opacity),
        );
    }
}

/// 下钻到某一项之后，**就地在那一行底下开的那一框**（设计稿 `.drill`）：描一圈强调色、底色是面板里
/// 调进 `drill-tint` 那么些强调色、大圆角。头一排「只看：这一项」、这一部分多少条、一颗「返回整批」；
/// 中间几条这一部分的样本；底下「通过这 N 条」「逐条处理」「拒绝这 N 条」。
///
/// **就地开在那一行底下**，不是另开一屏：人是顺着那一栏点下来的，看的是同一份分布，
/// 换个地方摆就要在两处之间来回对「我点的是哪一项」。
///
/// **这一层落下的仍旧是一批裁决**：与底下那一排走同一条路（[`Screen::pass`] / [`Screen::reject`]），
/// 照旧进裁决记录、照旧撤得掉——词表**批**那一条分开的两件事，屏上不另造第三种说法。
fn drill_box(ui: &mut egui::Ui, opened: &Opened, pressed: &mut BodyPressed) {
    let tokens = Tokens::builtin();
    let palette = look::palette(ui);
    let Some((_, label)) = &opened.basis.scope.drill else {
        return;
    };
    let 名 = if label.is_empty() {
        "（主库根）"
    } else {
        label.as_str()
    };
    let count = opened.count;
    let [上下, 左右] = tokens.space.drill_padding;
    ui.add_space(tokens.space.dist_row_gap);
    egui::Frame::new()
        .fill(
            palette
                .panel
                .lerp_to_gamma(palette.accent, tokens.mix.drill_tint),
        )
        .stroke(egui::Stroke::new(
            tokens.layout.control_stroke,
            palette.accent,
        ))
        .corner_radius(tokens.radius.large)
        .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = tokens.space.drill_gap;
            // 照稿（`drillHTML` 头一行）：「只看：…」与条数挨着摆在左边，弹簧在条数之后，
            // 「返回整批」贴右边。名字长了就截断，条数一个字都不许截。
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    pressed.whole = look::small_ghost_button(ui, "返回整批")
                        .on_hover_text("把整批操作放回整批；已经落下的那几批不受影响。")
                        .clicked();
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!("只看：{名}"))
                                    .size(look::font_size(ui.ctx(), tokens.font.size_small))
                                    .color(palette.ink),
                            )
                            .truncate(),
                        );
                        look::help(ui, &format!("{} 条", thousands(count)));
                    });
                });
            });
            sample_rows(ui, &opened.drilled_samples);
            // 照稿这三颗是**小号**（`.btn.sm`）：这一框是插在细分那一栏中间的，
            // 用默认那一档会把底下几项挤得太远。
            ui.horizontal(|ui| {
                look::small_buttons(ui, |ui| {
                    pressed.drilled_pass = ui
                        .scope(|ui| {
                            look::primary_button(ui.visuals_mut());
                            ui.add_enabled(
                                count > 0,
                                egui::Button::new(format!("通过这 {} 条", thousands(count))),
                            )
                        })
                        .inner
                        .on_hover_text(
                            "只通过这一组：采用第一条候选。落下的仍旧是一批裁决，撤得回来。",
                        )
                        .clicked();
                    pressed.drilled_one_by_one = ui
                        .button("逐条处理")
                        .on_hover_text("把队列收窄到这一组，一条一条看。")
                        .clicked();
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        pressed.drilled_reject = ui
                            .scope(|ui| {
                                look::warn_button(ui.visuals_mut());
                                ui.add_enabled(
                                    count > 0,
                                    egui::Button::new(format!("拒绝这 {} 条", thousands(count))),
                                )
                            })
                            .inner
                            .on_hover_text(
                                "只拒绝这一组：记成「我看过了，认不出」。落下的仍旧是一批裁决，撤得回来。",
                            )
                            .clicked();
                    });
                });
            });
        });
    ui.add_space(tokens.space.dist_row_gap);
}

/// 展开之后右边那一栏（设计稿 `.bbody` 右栏）：「随机样本 N 条」、一颗小号幽灵「换一组」，底下每条一行
/// （[`sample_rows`]）。按下「换一组」返回 `true`。
///
/// **它数的始终是整批**（照稿）：下钻收窄的是整批操作的作用范围，而「这一批长什么样」那句话不跟着
/// 只剩一组；下钻那一部分的样本摆在就地那一框里（[`drill_box`]）。
///
/// 那颗叫「换一组」不叫稿上的「换一批」：词表**批**不拿来说样本（拿主意的人 2026-09-15 定）。
fn samples_column(ui: &mut egui::Ui, samples: &[Sample]) -> bool {
    let mut 换 = false;
    ui.horizontal(|ui| {
        look::section(ui, &format!("随机样本 {} 条", samples.len()));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            换 = look::small_ghost_button(ui, "换一组").clicked();
        });
    });
    sample_rows(ui, samples);
    换
}

/// 样本那几行（设计稿 `.smp`）：一条一行——文件名（等宽）→ 第一条候选，两头放不下就截断、悬停看全文，
/// 行与行之间一道虚线。
///
/// **右栏与就地那一框摆的是同一种行**：两处各画一遍的话，一处改了字号另一处不会跟着，
/// 而它们是同一样东西的两处摆法。
fn sample_rows(ui: &mut egui::Ui, samples: &[Sample]) {
    // **行距由这一栏自己说了算**：行与行之间照稿只隔那一道虚线（间距在 `sample-row-padding`
    // 里），而这几行可能摆在一块把纵向间距撑开过的容器里（就地那一框的 `drill-gap` 是 8）
    // ——不按住的话样本之间会平白裂开一道缝。这一层 `scope` 就是为了把它按住。
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        rows(ui, samples);
    });

    fn rows(ui: &mut egui::Ui, samples: &[Sample]) {
        let tokens = Tokens::builtin();
        let palette = look::palette(ui);
        let 文件字号 = look::font_size(ui.ctx(), tokens.font.size_caption_plus);
        let 候选字号 = look::font_size(ui.ctx(), tokens.font.size_small);
        let 量 = |字体: egui::FontId| {
            ui.painter()
                .layout_no_wrap("字".to_owned(), 字体, egui::Color32::PLACEHOLDER)
                .size()
                .y
        };
        let 字高 =
            量(egui::FontId::monospace(文件字号)).max(量(egui::FontId::proportional(候选字号)));
        let 留白 = tokens.space.sample_row_padding;
        let 缝 = look::step(1);
        let 箭头宽 = tokens.layout.sample_arrow_column;
        let 线宽 = tokens.layout.control_stroke;
        for (at, one) in samples.iter().enumerate() {
            let (行, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 字高 + 2.0 * 留白),
                egui::Sense::hover(),
            );
            let 里 = 行.shrink2(egui::vec2(0.0, 留白));
            let 半 = ((里.width() - 箭头宽 - 2.0 * 缝) / 2.0).max(0.0);
            let 左格 = egui::Rect::from_min_size(里.min, egui::vec2(半, 里.height()));
            let 箭格 = egui::Rect::from_min_size(
                egui::pos2(左格.right() + 缝, 里.top()),
                egui::vec2(箭头宽, 里.height()),
            );
            let 右格 = egui::Rect::from_min_size(
                egui::pos2(箭格.right() + 缝, 里.top()),
                egui::vec2(半, 里.height()),
            );
            let 候选 = one
                .candidate
                .clone()
                .unwrap_or_else(|| "一条候选都没有".to_owned());
            for (格, 字) in [
                (
                    左格,
                    egui::RichText::new(&one.name)
                        .family(egui::FontFamily::Monospace)
                        .size(文件字号)
                        .color(palette.ink),
                ),
                (箭格, egui::RichText::new("→").color(palette.ink_4)),
                (
                    右格,
                    egui::RichText::new(候选)
                        .size(候选字号)
                        .color(palette.ink_2),
                ),
            ] {
                let mut 这一格 = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(格)
                        .layout(Layout::left_to_right(Align::Center)),
                );
                这一格.add(egui::Label::new(字).truncate());
            }
            if at + 1 < samples.len() {
                look::dashed_hline(
                    ui.painter(),
                    行.x_range(),
                    行.bottom(),
                    egui::Stroke::new(线宽, palette.line),
                );
            }
        }
    }
}

/// 逐条那一屏一张候选卡片上要写的几样：画的时候从核心库那条候选里抄出来，不借着光标底下那一条（它借着队列）。
#[derive(Debug, Clone)]
struct CandidateCard {
    /// 这条候选落在四档里的哪一档。
    tier: Tier,
    /// 作品。**只有作品名**：中文身份另起一枚标签（照稿，拿主意的人 2026-09-15 定）。
    game: String,
    /// 这条候选的**中文身份**（汉化版、官中），没有就是 `None`。
    mark: Option<String>,
    /// 哪个数据源。
    source: String,
    /// 撞的哪一份 DAT、按哪套哈希口径。
    matched: String,
    /// **依据**：这条候选是怎么来的，那句话原样。
    evidence: String,
}

/// 逐条那一屏详情里这一帧按下了哪一颗。画的时候不改自己，画完再动（[`Screen::detail`]）。
#[derive(Debug, Default)]
struct OneByOnePressed {
    /// 「通过所选候选」。
    pass: bool,
    /// 「都不对」。
    reject: bool,
    /// 「先放着」。
    set_aside: bool,
    /// 「手工指定…」。
    manual: bool,
    /// 「撤销上一条」。
    undo: bool,
}

/// 候选那几张卡片（设计稿 `.cands`）：一排三张，挨个往下排，**同一排一样高**。点一张就选它（与 `←` `→` 同一件事）。
/// 交回这一帧点了第几张。
fn candidate_cards(ui: &mut egui::Ui, cards: &[CandidateCard], nth: usize) -> Option<usize> {
    /// 一排几张（设计稿 `.cands` 的 `repeat(3, …)`）。
    const 每排: usize = 3;
    /// 量一张卡片多高时给它的地方有多高：足够高，不让它折行之外的地方受限。
    const 量的高: f32 = 100_000.0;
    let 缝 = Tokens::builtin().space.candidate_gap;
    #[allow(clippy::cast_precision_loss)]
    let 宽 = ((ui.available_width() - 缝 * (每排 - 1) as f32) / 每排 as f32).max(0.0);
    let mut 点了 = None;
    for (排, 这一排) in cards.chunks(每排).enumerate() {
        if 排 > 0 {
            ui.add_space(缝);
        }
        // **同一排一样高**（设计稿 `.cands` 是一张网格，一排里的格子一样高）：先在看不见、按不动的地方把这一排每张
        // 摆一遍量高（egui 的 `sizing_pass`），取最高的那张，再真摆。
        let 最高 = 这一排.iter().enumerate().fold(0.0_f32, |最高, (列, card)| {
            let mut 量 = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(("量候选卡片", 排, 列))
                    .max_rect(egui::Rect::from_min_size(
                        ui.cursor().min,
                        egui::vec2(宽, 量的高),
                    ))
                    .layout(Layout::top_down(Align::Min))
                    .sizing_pass()
                    .invisible(),
            );
            candidate_card(&mut 量, card, 排 * 每排 + 列, false, 0.0);
            最高.max(量.min_rect().height())
        });
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 缝;
            for (列, card) in 这一排.iter().enumerate() {
                let 第几张 = 排 * 每排 + 列;
                ui.allocate_ui_with_layout(
                    egui::vec2(宽, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_width(宽);
                        if candidate_card(ui, card, 第几张, 第几张 == nth, 最高) {
                            点了 = Some(第几张);
                        }
                    },
                );
            }
        });
    }
    点了
}

/// 一张候选卡片（设计稿 `.cand`）：面板底、一圈分隔线、大圆角，顶上一道那一档的色；作品、那一档的标签、
/// 「来源 / 匹配 / 依据」三行（「匹配」那一格等宽，别的常规体）。选中那一张描强调色、外头一圈强调浅色（`candidate-ring`）。
/// 整张至少 `最矮` 那么高（同一排取最高的那张，[`candidate_cards`]）。整张点得动，交回点了没有。
fn candidate_card(
    ui: &mut egui::Ui,
    card: &CandidateCard,
    第几张: usize,
    选中: bool,
    最矮: f32,
) -> bool {
    let tokens = Tokens::builtin();
    let palette = look::palette(ui);
    let 线宽 = tokens.layout.control_stroke;
    let 圆角 = tokens.radius.large;
    let 色 = look::tier_color(card.tier, ui.visuals());
    let 整张 = look::barred_card(ui, 色, look::BarEdge::Top, |ui| {
        ui.set_min_height(最矮);
        egui::Frame::new()
            .inner_margin(egui::Margin::same(tokens.space.candidate_padding as i8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.style_mut().interaction.selectable_labels = false;
                ui.spacing_mut().item_spacing.y = tokens.space.candidate_inner_gap;
                ui.add(
                    egui::Label::new(
                        font::strong(&card.game)
                            .size(look::font_size(ui.ctx(), tokens.font.size_candidate_title))
                            .color(palette.ink),
                    )
                    .wrap(),
                );
                ui.horizontal(|ui| {
                    look::chip(ui, look::tier_tone(card.tier), card.tier.label());
                    if let Some(mark) = &card.mark {
                        look::chip(ui, look::Tone::Neutral, mark);
                    }
                });
                let 字号 = look::font_size(ui.ctx(), tokens.font.size_caption_plus);
                let 等宽字号 = tokens.font.size_caption;
                let [行竖, 行横] = tokens.space.candidate_row_gap;
                egui::Grid::new(("候选卡片", 第几张))
                    .num_columns(2)
                    .min_col_width(tokens.layout.candidate_key_width)
                    .spacing([行横, 行竖])
                    .show(ui, |ui| {
                        for (名, 值, 等宽) in [
                            ("来源", &card.source, false),
                            ("匹配", &card.matched, true),
                            ("依据", &card.evidence, false),
                        ] {
                            ui.label(egui::RichText::new(名).size(字号).color(palette.ink_3));
                            let mut 字 = egui::RichText::new(值.as_str()).color(palette.ink);
                            字 = if 等宽 {
                                字.family(egui::FontFamily::Monospace).size(等宽字号)
                            } else {
                                字.size(字号)
                            };
                            ui.add(egui::Label::new(字).wrap());
                            ui.end_row();
                        }
                    });
            });
    })
    .response
    .rect;
    let painter = ui.painter();
    if 选中 {
        painter.rect_stroke(
            整张,
            圆角,
            egui::Stroke::new(tokens.layout.candidate_ring, palette.accent_soft),
            egui::StrokeKind::Outside,
        );
    }
    painter.rect_stroke(
        整张,
        圆角,
        egui::Stroke::new(线宽, if 选中 { palette.accent } else { palette.line }),
        egui::StrokeKind::Inside,
    );
    ui.interact(
        整张,
        ui.id().with(("候选卡片", 第几张)),
        egui::Sense::click(),
    )
    .on_hover_text("选它（键盘上是 ← →）")
    .clicked()
}

/// 那一框键位提示（设计稿 `.keys`）：「← → 切换候选」「Y 通过」「N 都不对」「空格 先放着」「U 撤销上一条」，键帽是 [`look::kbd`]。
fn key_hints(ui: &mut egui::Ui) {
    let tokens = Tokens::builtin();
    let palette = look::palette(ui);
    let [上下, 左右] = tokens.space.keys_padding;
    let [竖, 横] = tokens.space.keys_gap;
    let 字号 = look::font_size(ui.ctx(), tokens.font.size_small);
    egui::Frame::new()
        .fill(palette.panel_2)
        .stroke(egui::Stroke::new(
            tokens.layout.control_stroke,
            palette.line,
        ))
        .corner_radius(tokens.radius.medium)
        .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(横, 竖);
                for (键们, 说明) in [
                    (&["←", "→"][..], "切换候选"),
                    (&["Y"][..], "通过"),
                    (&["N"][..], "都不对"),
                    (&["空格"][..], "先放着（不保存，稍后仍会出现）"),
                    (&["U"][..], "撤销上一条"),
                ] {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = tokens.space.key_hint_gap;
                        for 键 in 键们 {
                            look::kbd(ui, 键);
                        }
                        ui.label(egui::RichText::new(说明).size(字号).color(palette.ink_3));
                    });
                }
            });
        });
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
    /// **按依据形状**只看哪一类（[`Only`]）：一级分批点「逐条处理」时填那一批，屏头「逐条」时填有多个候选的那几批。
    ///
    /// 它**没有文本框**——「MAME / gameboy.xml / 中置信 / 含头 / 1 个候选」不是人
    /// 打得出来的东西，屏上它只能从卡片上点。命令行那一侧收得下同一批
    /// （`romcat triage --shape`），但走的也不是手打：报告把每一批连
    /// [`Shape::selector`] 折出来的那串字一起印出来，人**照着抄**（票
    /// `queue-followups/08`）。
    only: Option<Only>,
}

/// 逐条那一屏只看队列里的哪一类：一级分批里的某一批，或者有多个候选的那几批。
#[derive(Debug, Clone, PartialEq, Eq)]
enum Only {
    /// 一级分批点「逐条处理」进来的那一批变体。
    Batch(Shape),
    /// 屏头「逐条」进来的：有多个候选的那几批（设计稿 `.obolist` 栏头「有多个候选」，[`Screen::show_multiple`]）。
    Multiple(Vec<Shape>),
}

impl Only {
    /// 待选列表栏头上写的那几个字。
    fn label(&self) -> String {
        match self {
            Self::Batch(shape) => shape.label(),
            Self::Multiple(_) => "有多个候选".to_owned(),
        }
    }

    /// 详情抬头「共 N 条…」后面跟的那几个字（设计稿 `.obodet` 抬头）：有多个候选那一栏自己有名字，
    /// 某一批那一栏的名字太长（整串依据形状），栏头上已经写着，这里不重复。
    fn head_suffix(&self) -> &'static str {
        match self {
            Self::Batch(_) => "",
            Self::Multiple(_) => "有多个候选",
        }
    }

    /// 折进选择器的那几个形状（[`Filter::shape`]，几个之间是并集）。
    fn shapes(&self) -> Vec<Shape> {
        match self {
            Self::Batch(shape) => vec![shape.clone()],
            Self::Multiple(shapes) => shapes.clone(),
        }
    }
}

impl Default for Picks {
    fn default() -> Self {
        Self {
            under: String::new(),
            name: String::new(),
            candidate_work: String::new(),
            // 默认那三档：**跳过**不在队列里——它不是「拿不定主意」，是「不该撞 DAT」。
            states: [true, true, true, false],
            only: None,
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
            shape: self.only.as_ref().map(Only::shapes).unwrap_or_default(),
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

/// 屏头「裁决记录 N」那颗按钮上的字（设计稿 `#open-lots`）：「裁决记录」常规体，那个数是等宽的，字号都是按钮那一档
/// （`size-small`）。字色留给按钮自己填（`PLACEHOLDER`）。
fn records_button_text(ui: &egui::Ui, 条数: &str) -> egui::text::LayoutJob {
    let 字号 = look::font_size(ui.ctx(), Tokens::builtin().font.size_small);
    let mut job = egui::text::LayoutJob::default();
    for (段, 字族) in [
        ("裁决记录 ", egui::FontFamily::Proportional),
        (条数, egui::FontFamily::Monospace),
    ] {
        job.append(
            段,
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::new(字号, 字族),
                color: egui::Color32::PLACEHOLDER,
                ..egui::TextFormat::default()
            },
        );
    }
    job
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
