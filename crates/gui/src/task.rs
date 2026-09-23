//! **任务**那一屏：排队、进度、可停、历史。**它不发起操作，只承接。**
//!
//! ## 这一屏为什么是基础设施而不是一屏
//!
//! 核心库那几个长入口一趟要跑很久——扫一遍真库 37.1 分钟、识别 65.5 秒、
//! 排一次**差量预览**在真机量级上 343 毫秒。它们跑在画帧那条线程上的话，窗口就是一块
//! 白板：期间切不了屏、滚不动列表、连「停下」都点不着。所以它们统统搬到画帧线程之外，
//! 这一屏是那件事在界面上的落点。
//!
//! **眼下接上来的是排差量预览、算一遍容量、同步、扫描、取数据源、刮削、识别与折标题**。
//! **导出**在它自己那张票里接——接的办法与这里一模一样：核心那一侧收一个
//! [`Handle`](romcat_core::task::Handle)，界面这一侧往 [`Board`] 上排一趟。
//!
//! ## 领域判断一条都不在这里
//!
//! 排队怎么排、进度怎么算、被按停了算「停了」还是算「失败」——全在
//! [`romcat_core::task`]。这一层只做两件事：把任务台上的账画出来，把「停下」那一下
//! 转发回去。

use std::collections::BTreeMap;
use std::time::Duration;

use romcat_core::collection;
use romcat_core::report::human_duration;
use romcat_core::scan::ScanOutcome;
use romcat_core::scrape::Outcome as ScrapeOutcome;
use romcat_core::sources::SourceStatus;
use romcat_core::sublibrary::report::SelectionReport;
use romcat_core::sync::{Outcome as SyncOutcome, Prepared};
use romcat_core::task::{Board, Ending, Live, Record};

use crate::font;
use crate::look::{self, Tone, step};
use crate::tokens::Tokens;

/// 一趟任务跑完之后交出来的东西。
///
/// **导出接上来时往这里再加一支**（票 `gui-self-sufficient/08`）——这个枚举就是
/// 「任务台上会跑哪几种活」的清单。
///
/// ## 要写库的那几支，写在**认领**那一步
///
/// 台上那条线拿的是中立库的**只读**连接（`Catalog::read_only`），写不动。所以
/// [`Synced`](Self::Synced) 那份**清单**是当作产物交回来、由认领它的那一屏落库的
/// （`sublibrary::Screen::settle`）。台上那条线自己写的话，两份连接会在同一个 SQLite
/// 文件上撞车。
#[derive(Debug)]
pub enum Product {
    /// 一份排好的**差量预览**。
    ///
    /// 装箱是因为它很大（选择集、期望状态、清单、目标状态、计划全在里头），
    /// 而这个枚举将来还要长出别的支。
    Preview(Box<Prepared>),
    /// 扫完了一个**根**。装箱同上：一趟扫描的产物里带着整份体检统计。
    Scanned(Box<ScanOutcome>),
    /// 取回了一个**数据源**。
    Fetched(SourceStatus),
    /// 跑完了一趟**刮削**。装箱同上：一趟刮削的产物里带着整份报告。
    Scraped(Box<ScrapeOutcome>),
    /// 跑完了一趟**同步**。装箱同上：这份账里带着同步完之后的整份**清单**。
    ///
    /// **被按停的那一趟也会走到这儿**（`Outcome::interrupted` 记着），而且**必须**走到
    /// ——那份清单记的是「到中断为止目标上真实有什么」，认领时落回中立库，下一趟才接得上。
    Synced(Box<SyncOutcome>),
    /// 算了一遍**每台设备的容量**：一台一份[选择集报告](SelectionReport)，按子库名。
    Evaluated(Box<BTreeMap<String, SelectionReport>>),
    /// 算了一遍「**把这一批加进那个子库之后会怎样**」（票 `gui-looks-like-the-design/23`
    /// 的「加入子库」弹层，核心库 `sublibrary::addition`）。
    ///
    /// **它排在台上而不是画帧线上**：那一趟要折一遍事实（真机 343 毫秒）。
    /// 弹层在它回来之前写「正在算…」，不写 0（挂单 `Q1181`）。
    Added(Box<romcat_core::sublibrary::Addition>),
    /// 跑完了一趟**识别**。装箱同上：这份账里带着整份命中率报告。
    ///
    /// **被按停的那一趟也会走到这儿**（`Outcome::interrupted` 记着），而且**必须**
    /// 走到——识别起手就把上一轮的结论清干净，已经算出来的那些真的落进了中立库，
    /// 库屏工序段那一行的数字要照它刷新。**它没有断点**：下一趟从头再算一遍
    /// （`romcat_core::identify::run_task` 报的那句「停在半路」说的就是这件事）。
    Identified(Box<romcat_core::identify::Outcome>),
    /// 跑完了一趟**折标题**：把识别与刮削的结论折成每个作品的**标题集合**。
    /// 装箱同上——这份报告里带着按语言、按类型、按来源的整份账。
    ///
    /// **它不走「写在认领那一步」那条**：重折是「把折出来的那批清掉再写回去」
    /// （`romcat_core::title::refold`），一路写批。台上那条线自己开一份写得动的库，
    /// 与扫描、刮削、识别同一条路（见本模块开头那段与 `crate::stages`）。
    ///
    /// **没有「停在半路」这一档**：重折在清空与写回之间一步都不停
    /// （停在那儿等于把整份标题集合丢掉），所以被按停的那一趟一个字节都没写
    /// ——那是[停了](romcat_core::task::Ending::Stopped)，不是半路。
    Titled(Box<romcat_core::title::TitleReport>),
    /// 跑完了一趟**导出**：把中立库写成前端能读的元数据，铺在导出目录里。
    /// 装箱同上——这份报告里带着逐份文件的账、被挡下的变体与实测**能力档位**。
    ///
    /// **它不走「写在认领那一步」那条**：一趟导出一份文件一份文件地写，每写完一份就把
    /// **底本**存进中立库（那是下一趟「外面有人动过没有」的唯一判据）。台上那条线
    /// 自己开一份写得动的库，与扫描、刮削、识别、折标题同一条路（见本模块开头那段与
    /// `crate::stages`）。
    ///
    /// **它有「停在半路」这一档**（与折标题正相反）：按停时已经写出去的那几份连底本
    /// 一起真的落了盘与库，下一趟拿它们当基线接着比
    /// ——`romcat_core::adapter::transfer::export_task` 报的那句「停在半路」说的就是
    /// 这件事。
    Exported(Box<romcat_core::adapter::report::ExportReport>),
    /// 排好了一趟**整批收藏**（或者自建合集的加减）：这一批各该钉在哪种锚上。
    ///
    /// **它要写两份库**（沉淀库那些成员关系、中立库那份投影），所以与
    /// [`Synced`](Self::Synced) 同一条：产物交回来，由认领它的那一屏落库
    /// （`browse::Screen::settle_collection`）。装箱是因为它带着整批的锚——
    /// 真库上「全选 46,483 行 → ★ 收藏」那一下就是四万多条。
    Planned(Box<collection::Plan>),
    /// 算了一遍**开着铺媒体时这一趟导出最多要铺多少**：几份、共多少字节
    /// （库屏工序段导出那一支，票 `one-criterion-per-thing/09`）。
    ///
    /// **是上界**（`romcat_core::adapter::transfer::media_to_lay`）：落点上已经有的真铺时
    /// 不重铺。只留两个数、不留整份落点表：屏上要画的只有这两个，而整库的落点表真库上
    /// 是几万条。整条只读，所以它与 [`Evaluated`](Self::Evaluated) 一样不必写在认领那一步。
    MediaCounted {
        /// 几份。
        files: u64,
        /// 共多少字节。
        bytes: u64,
    },
    /// 出了一份**库体检**（库屏「重新体检」，票 `gui-looks-like-the-design/27`）：报告，连同同一份统计折出来的重复拷贝
    /// 完整明细。整条只读，与 [`Evaluated`](Self::Evaluated) 一样不必写在认领那一步。装箱是因为两样都大。
    Checked {
        /// 体检报告。
        report: Box<romcat_core::report::HealthReport>,
        /// 重复拷贝的完整明细：每一组、记下的每一份。
        duplicates: Box<romcat_core::report::DuplicateDetails>,
        /// **疑似同一作品**那几条建议（票 `gui-looks-like-the-design/17`）。它不在体检报告
        /// 里——报告折的是扫描留下的那份统计，而这一条要走一遍识别之后的中立库。
        /// 跟着这一趟回来是因为两者同一个处境：都只读、都贵、都不许进画帧那条线程。
        suspicions: Box<Vec<romcat_core::triage::same_work::Suspicion>>,
    },
    /// 读好了一台设备的**脚印**（`romcat_core::sync::Footprint`，票 `gui-looks-like-the-design/21`）：目标设置弹层里平台表
    /// 列哪几个平台、换一份档案当场说得出放不下哪几份、落点预览照哪一份，都从它纯算。整条只读，与
    /// [`Evaluated`](Self::Evaluated) 一样不必写在认领那一步。装箱是因为它带着选择集里每个变体的成员。
    Footprint(Box<romcat_core::sync::Footprint>),
    /// 数完了一条目标路径上**清单之外**的文件（`romcat_core::sync::prepare::strangers_at`，票 `gui-looks-like-the-design/21`）：
    /// 目标设置弹层里那句「目录里已有 N 个文件，它们不在清单里」。整条只读遍历目标，不必写在认领那一步。
    Strangers(romcat_core::sync::Strangers),
}

/// 这个界面上那张**任务台**。
pub type Tasks = Board<Product>;

/// **截图那一路定死的钟**：任务屏上跟着挂钟走的那几个数——正在跑那一趟的已用（约剩由它折）、
/// 历史里每一趟的耗时与收场时刻——一律画成这里给的值。
///
/// 截图门逐像素比对基线（`tests/snapshot.rs`），这几个数照实画的话一趟一个样。**真窗口那一路
/// 不定**：默认没有，画的就是任务台上那一份。与 `App::set_workspace_label` 同一个用处。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clock {
    /// 正在跑那一趟已经跑了多久；历史里每一趟花了多久也画成它。
    pub elapsed: Duration,
    /// 历史里每一趟什么时候收的场：UNIX 纪元起的秒。
    pub ended_at: i64,
    /// 本地时区比 UTC 快多少秒（东八区是 28800）。真窗口那一路读系统时区库，按那一刻算。
    pub utc_offset: i32,
    /// 此刻：UNIX 纪元起的秒。拿来判断收场时刻是不是今年——不是今年的才带年份。
    pub now: i64,
}

/// 任务那一屏，连同每一屏底下那条状态栏里任务那一截。
///
/// **它自己只记两样东西：这一帧从任务台取的那一份快照，与截图那一路定死的钟**——要画的全在
/// 任务台上。
#[derive(Debug, Default)]
pub struct Screen {
    /// 这一帧的任务台快照（[`Board::running`]），连同是哪一帧取的。
    ///
    /// **状态栏与任务屏那张卡读的是同一份**：快照里的已用时间是取的那一刻现量的，同一帧里
    /// 问两次任务台，状态栏与卡上就会印出两个不一样的「剩余约」。状态栏先画，任务屏后画，
    /// 后画的那一处照帧号认出这一份已经取过了。
    snapshot: Option<(u64, Option<Live>)>,
    /// 截图那一路定死的钟（[`Clock`]）；真窗口那一路没有。
    clock: Option<Clock>,
}

impl Screen {
    /// 开一个。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 定死跟着挂钟走的那几个数（[`Clock`]）。**截图那一路要它**，真窗口那一路不调。
    pub fn pin_clock(&mut self, clock: Clock) {
        self.clock = Some(clock);
        self.snapshot = None;
    }

    /// 这一帧正在跑的那一趟：这一帧头一回问时从任务台取，之后同一帧里都给这同一份。
    fn running(&mut self, ctx: &egui::Context, tasks: &Tasks) -> Option<Live> {
        let pass = ctx.cumulative_pass_nr();
        if let Some((taken, live)) = &self.snapshot
            && *taken == pass
        {
            return live.clone();
        }
        let mut live = tasks.running();
        if let (Some(clock), Some(live)) = (self.clock, live.as_mut()) {
            live.elapsed = clock.elapsed;
        }
        self.snapshot = Some((pass, live.clone()));
        live
    }

    /// 要画的那份历史，每一行连同「时间」那一格画成的字。
    ///
    /// 定了钟就把耗时、收场时刻、时区与此刻一律换成钟上的；没定就是任务台上那一份，时区与此刻取
    /// 这台机器的。时刻画成本地短格式只走 [`crate::clock::Clock`] 那一处。
    fn shown_history(&self, history: &[Record]) -> Vec<(Record, String)> {
        history
            .iter()
            .cloned()
            .map(|mut record| {
                let 钟 = match self.clock {
                    Some(clock) => {
                        record.elapsed = clock.elapsed;
                        record.ended_at = clock.ended_at;
                        crate::clock::Clock::fixed(clock.now, clock.utc_offset)
                    }
                    None => crate::clock::Clock::System,
                };
                let at = 钟.short(record.ended_at);
                (record, at)
            })
            .collect()
    }

    /// 屏头右侧属于这一屏的那一段（[`look::screen_header`] 的右侧）：**照稿只放一颗小号幽灵「清空历史」**
    /// （设计稿 `.scrhead` 里那颗 `.btn.ghost.sm`）。原来顶栏上那句「N 个进行中 · M 条历史」照稿不要了
    /// ——跑着的、排着的、历史里有几条，这一屏正文里一眼就看得见。历史空着时这颗按钮灰掉。
    pub fn status(&mut self, ui: &mut egui::Ui, tasks: &mut Tasks) {
        let 有历史 = !tasks.history().is_empty();
        let 清空 = look::small_buttons(ui, |ui| {
            ui.scope(|ui| {
                look::ghost_button(ui.visuals_mut());
                ui.add_enabled(有历史, egui::Button::new("清空历史"))
            })
            .inner
        });
        if 清空.clicked() {
            tasks.clear_history();
        }
    }

    /// **底部状态栏**（设计稿 `.statusbar`）：每一屏底下都有这一条。左边是任务台那一小截，
    /// 右边是「ROM 只读」与工作目录。返回「点了任务那一句没有」——点了就去任务屏。
    ///
    /// **任务那一小截与任务屏那张卡是同一件事**：读的是同一帧里同一份任务台快照
    /// （`Screen::snapshot`），百分比、约剩多少走的是同一个排法（`percent`、`elapsed`）。
    /// 说不出走了几成就不印百分比、算不出约剩就一个字都不画，与卡上同一个口径。
    pub fn status_bar(&mut self, ui: &mut egui::Ui, tasks: &Tasks, workspace: &str) -> bool {
        let mut go = false;
        let running = self.running(ui.ctx(), tasks);
        ui.horizontal_centered(|ui| {
            match &running {
                None => {
                    ui.label(egui::RichText::new("任务台空闲").small().weak());
                }
                Some(live) => {
                    let line = egui::Label::new(egui::RichText::new(status_line(live)).small())
                        .sense(egui::Sense::click());
                    go = ui.add(line).on_hover_text("去任务屏").clicked();
                    // 设计稿那条小进度条：宽取令牌 `statusbar-bar`，高取间距最窄那一档。
                    ui.add(
                        progress_bar(ui.visuals(), live.progress.fraction())
                            .desired_width(Tokens::builtin().layout.statusbar_bar)
                            .desired_height(step(0)),
                    );
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(font::mono(workspace).small().weak());
                ui.label(egui::RichText::new("ROM 只读").small().weak());
            });
        });
        go
    }

    /// 画一帧：屏体（设计稿 `.scrbody`，[`look::screen_body`]）在屏头底下滚。屏头归主窗口画
    /// （[`look::screen_header`]；右侧那颗「清空历史」见 [`Self::status`]）。
    ///
    /// **留白全照设计稿、全从令牌取**：屏体内边距由 `look::screen_body` 给，块与块之间
    /// `screen-section-gap`，小标题与底下那块 `section-title-gap`。这一屏里控件之间不垫缺省的
    /// 竖向间距——留多少只由令牌说。
    pub fn ui(&mut self, ui: &mut egui::Ui, tasks: &mut Tasks) {
        look::screen_body(ui, "任务屏", |ui| {
            // 卡片里头照旧用缺省的间距，这里先记下来（与弹层框架 `dialog.rs` 同一个办法）。
            let spacing = ui.spacing().item_spacing;
            ui.spacing_mut().item_spacing.y = 0.0;
            self.body(ui, tasks, spacing);
        });
    }

    fn body(&mut self, ui: &mut egui::Ui, tasks: &mut Tasks, spacing: egui::Vec2) {
        let space = &Tokens::builtin().space;
        let 字号 = &Tokens::builtin().font;

        section(ui, "正在运行");
        let mut stop = None;
        let running = self.running(ui.ctx(), tasks);
        match &running {
            // **空台也摆一张卡**：设计稿 `.card.empty`——一句居中的弱字，四边留 `empty-padding`。
            None => {
                let 留白 = space.empty_padding;
                card(
                    ui,
                    false,
                    egui::Margin::from(egui::vec2(留白, 留白)),
                    spacing,
                    |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new(
                                    "当前没有运行中的任务。在「库」页面运行任意工序后，任务会显示在这里。",
                                )
                                .weak()
                                .line_height(lh(字号.size_body)),
                            );
                        });
                    },
                );
            }
            Some(live) => {
                let 留白 = space.card_padding;
                let 按了停下 = card(
                    ui,
                    true,
                    egui::Margin::from(egui::vec2(留白, 留白)),
                    spacing,
                    |ui| running_ui(ui, live),
                );
                if 按了停下 {
                    stop = Some(live.id);
                }
            }
        }

        ui.add_space(space.screen_section_gap);
        section(ui, "等待中");
        let queued = tasks.queued();
        if queued.is_empty() {
            help(ui, "没有等待中的任务。");
        }
        let [row_y, row_x] = space.queue_row_padding;
        for (at, (id, name)) in queued.into_iter().enumerate() {
            // 一趟一张卡，卡与卡之间与小标题底下同一档（设计稿 `.col` 的 `gap:8px`）。
            if at > 0 {
                ui.add_space(space.section_title_gap);
            }
            let 移除 = card(
                ui,
                false,
                egui::Margin::from(egui::vec2(row_x, row_y)),
                spacing,
                |ui| {
                    // 设计稿 `.row`：一行与小号按钮一样高。**在横排开出来之前给**：横排一开就按这一格
                    // 占好最矮的高，进了横排再改已经晚了（按钮那一档 28 点会把这一行撑高 4 点）。
                    ui.spacing_mut().interact_size.y = Tokens::builtin().layout.button_small_height;
                    ui.horizontal(|ui| {
                        // 格与格之间 8 点。
                        ui.spacing_mut().item_spacing.x = step(1);
                        // 排第几：从 1 数的位次，不是任务号——人问的是「前面还有几趟」。
                        ui.label(font::mono((at + 1).to_string()).weak());
                        ui.label(font::strong(&name));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // **撤掉排着的这一趟**走任务台现成的入口（`Board::stop`）：还没开跑的
                            // 直接撤下，照任务台原有的账在历史里记一条已取消、交回排它的那一屏
                            // ——不然那一屏会一直记着「我还有一趟在排」。正在跑的那一趟不受影响。
                            // 设计稿 `.btn.ghost.sm`：小号幽灵按钮。
                            look::small_buttons(ui, |ui| {
                                ui.scope(|ui| {
                                    look::ghost_button(ui.visuals_mut());
                                    ui.button("移除")
                                })
                                .inner
                            })
                            .clicked()
                        })
                        .inner
                    })
                    .inner
                },
            );
            if 移除 {
                stop = Some(id);
            }
        }
        if let Some(id) = stop {
            tasks.stop(id);
        }

        ui.add_space(space.screen_section_gap);
        section(ui, "历史");
        // **表头一直在，空了就在表里摆一行**（设计稿 `<td colspan="4" class="empty">`，挂单 `Q837`）。
        let shown = self.shown_history(tasks.history());
        card(ui, false, egui::Margin::ZERO, spacing, |ui| {
            history_table(ui, &shown);
        });

        // **一次只跑一趟**（词表**任务台**）：说出口，人才不会以为排着的那几趟卡住了。
        // 逐字照设计稿那一句（拿主意的人裁，挂单 `Q839`）。
        ui.add_space(space.screen_section_gap);
        help(
            ui,
            "任务依次运行，一次只运行一个：它们读写同一个库和同一块硬盘，并行只会互相拖慢。\
             任务运行期间可以正常使用其他页面。",
        );
    }
}

/// 历史那张表，四列：任务、结果、耗时、时间。
///
/// 第二列的表头照稿写「结果」（拿主意的人裁，挂单 `Q832`）；格子里摆的仍是收场四档的词。
///
/// **列宽照稿按比例分，放不下时后三列量出多宽就多宽、剩下的全给「任务」那一列**：名字底下那半句说明（留下了什么、
/// 下次从哪儿接着来）可能很长，得在这一列里折行。不先定宽的话，格子按上一帧量出的列宽折，
/// 头一帧列窄，中文一个字一行，一行历史能长到几百点高，后面几行直接掉出屏去。
fn history_table(ui: &mut egui::Ui, history: &[(Record, String)]) {
    let tokens = Tokens::builtin();
    // 格子内边距照设计稿：表头 `.tbl th` 取 `table-head-padding`，每一行 `.tbl td` 取 `cell-padding`。
    let [pad_y, pad_x] = tokens.space.cell_padding;
    let [head_y, _] = tokens.space.table_head_padding;
    // 两列之间的缝：左边那格的右内边距加右边那格的左内边距。
    let gap = 2.0 * pad_x;
    let [task_title, ending_title, took_title, at_title] = ["任务", "结果", "耗时", "时间"];
    let small = egui::TextStyle::Small;
    let body = egui::TextStyle::Body;
    // 字号照稿的两档半号（拿主意的人裁，挂单 `Q840`）：表头 `.tbl th` 是 11.5 号的弱字
    // （`size-caption-plus`），每一行 `.hist td` 是 12.5 号（`size-small-plus`）。列宽照画出来的那个字号量。
    let caption = tokens.font.size_caption_plus;
    let title = |text: &str| {
        egui::RichText::new(text)
            .size(caption)
            .weak()
            .line_height(lh(caption))
    };
    let 行字号 = tokens.font.size_small_plus;
    let 行字 = |text: &str| egui::RichText::new(text).size(行字号);
    // 收场那一格是一枚标签（[`look::chip`]）：两侧各一档内边距、圆点、一档缝，再加上词。
    let chip_extra = 2.0 * step(1) + tokens.layout.chip_dot + step(0);
    let chip = history
        .iter()
        .map(|(record, _)| text_width(ui, record.ending.word(), &small) + chip_extra)
        .fold(text_width(ui, title(ending_title), &body), f32::max);
    let took = history
        .iter()
        .map(|(record, _)| text_width(ui, 行字(elapsed(record.elapsed).as_str()), &body))
        .fold(text_width(ui, title(took_title), &body), f32::max);
    let at = history
        .iter()
        .map(|(_, at)| text_width(ui, 行字(at.as_str()), &body))
        .fold(text_width(ui, title(at_title), &body), f32::max);
    // 任务那一列不折行时多宽：名字一行、底下那半句说明一行，取宽的那个。
    let name = history
        .iter()
        .map(|(record, _)| {
            let 名字 = text_width(ui, font::strong(&record.name).size(行字号), &body);
            let 说明 = record
                .ending
                .detail()
                .map_or(0.0, |detail| text_width(ui, detail.as_str(), &small));
            名字.max(说明)
        })
        .fold(text_width(ui, title(task_title), &body), f32::max);
    let 可分 = ui.available_width() - 2.0 * pad_x - 3.0 * gap;
    let columns = if history.is_empty() {
        // **空表四列平分**：设计稿那张表一行内容都没有时，浏览器自动排版把四列摊成一样宽。
        let 一份 = 可分 / 4.0;
        Columns {
            name: 一份,
            chip: 一份,
            took: 一份,
            at: 一份,
        }
    } else if 可分 >= name + chip + took + at {
        // **放得下就照浏览器给表格排版的算法分**（设计稿那张表 `width:100%`、自动排版）：每一列先拿到
        // 它不折行时的宽（连两侧内边距），多出来的宽按这个宽的比例分给各列。拿稿上那几行验过：
        // 这样算出的四列宽与稿上量出来的差不到 2 点。
        let 宽 = [name, chip, took, at].map(|content| content + gap);
        let 总宽: f32 = 宽.iter().sum();
        let 多出 = 可分 - (name + chip + took + at);
        let [name, chip, took, at] = 宽.map(|width| width + 多出 * width / 总宽 - gap);
        Columns {
            name,
            chip,
            took,
            at,
        }
    } else {
        Columns {
            name: (可分 - chip - took - at).max(text_width(ui, title(task_title), &body)),
            chip,
            took,
            at,
        }
    };
    // 这张表自己排：一行一横排，格与格之间只留那一道缝，行与行之间一道分隔线。**不垫 egui 的缺省
    // 间距与最矮行高**（按钮那一档 28 点）——一行多高只由格子内边距与字的行高说。
    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
    ui.spacing_mut().interact_size.y = 0.0;
    // **表头四格走同一个格子**（[`cell`]）：同一行里竖直对齐是同一种，不会有一格高出一截。
    // **表头一律靠左**：设计稿 `.tbl th{text-align:left}` 比 `.r` 更具体，压过了它；靠右的只有
    // 每一行里的耗时、时间（`td.r`）。
    table_row(ui, head_y, pad_x, |ui| {
        cell(ui, columns.name, caption, false, title(task_title));
        ui.add_space(gap);
        cell(ui, columns.chip, caption, false, title(ending_title));
        ui.add_space(gap);
        cell(ui, columns.took, caption, false, title(took_title));
        ui.add_space(gap);
        cell(ui, columns.at, caption, false, title(at_title));
    });
    look::divider(ui);
    if history.is_empty() {
        // 逐字照设计稿（拿主意的人裁，挂单 `Q837`）。**内边距是格子那一档**（8 / 10）：设计稿里那一格同时
        // 带着 `.tbl td` 与 `.empty`，前者更具体，`.empty` 那 28 点没生效，稿上这一行就是 36 点高。
        egui::Frame::new()
            .inner_margin(egui::Margin::from(egui::vec2(pad_x, pad_y)))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("还没有已完成的任务。")
                            .size(行字号)
                            .weak()
                            .line_height(lh(行字号)),
                    );
                });
            });
        return;
    }
    // 设计稿 `.tbl td` 每一行下沿一道线、末一行不画（`tr:last-child td{border-bottom:0}`）。
    for (第几, (record, at)) in history.iter().enumerate() {
        if 第几 > 0 {
            look::divider(ui);
        }
        table_row(ui, pad_y, pad_x, |ui| {
            history_cells(ui, record, at, columns, gap);
        });
    }
}

/// 表里的一行：上下各留 `pad_y`、左边留 `pad_x`，格子横着摆，竖直居中（设计稿 `vertical-align:middle`）。
fn table_row(ui: &mut egui::Ui, pad_y: f32, pad_x: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(pad_y);
    ui.horizontal(|ui| {
        ui.add_space(pad_x);
        add(ui);
    });
    ui.add_space(pad_y);
}

/// 这一字号的一行多高：照设计稿的行高（令牌 `line-height`，稿上 `body{line-height:1.55}`）。
///
/// egui 缺省一行只有字那么高，稿上每一行字上下都有行距；不照着给，这一屏的卡片、表格行都比稿上矮。
fn lh(size: f32) -> Option<f32> {
    Some(size * Tokens::builtin().font.line_height)
}

/// 一行帮助字：设计稿 `.help`——说明字号、弱字色，行高照稿。
fn help(ui: &mut egui::Ui, text: &str) {
    let size = Tokens::builtin().font.size_small;
    ui.label(
        egui::RichText::new(text)
            .small()
            .weak()
            .line_height(lh(size)),
    );
}

/// 历史那张表四列各多宽。
#[derive(Debug, Clone, Copy)]
struct Columns {
    /// 任务那一列：放得下时照稿按比例分到宽；放不下时剩下的全给它，名字底下那半句说明在这么宽里折行。
    name: f32,
    /// 收场那一列：至少那枚标签那么宽。
    chip: f32,
    /// 耗时那一列。
    took: f32,
    /// 时间那一列。
    at: f32,
}

/// 表里的一格：`width` 那么宽、一行 `size` 号字那么高，字靠左或靠右（`right`）。
fn cell(ui: &mut egui::Ui, width: f32, size: f32, right: bool, text: impl Into<egui::WidgetText>) {
    let layout = if right {
        egui::Layout::right_to_left(egui::Align::Center)
    } else {
        egui::Layout::left_to_right(egui::Align::Center)
    };
    // 一行字照稿的行高（[`lh`]）：比这矮的话，这一格按矮的那个高度居中，字就比同一行别的格低一截。
    let height = size * Tokens::builtin().font.line_height;
    ui.allocate_ui_with_layout(egui::vec2(width, height), layout, |ui| {
        ui.set_min_width(width);
        ui.label(text);
    });
}

/// 这段字不折行时多宽。字上没说字号的，按 `style` 那一档量。
fn text_width(ui: &egui::Ui, text: impl Into<egui::WidgetText>, style: &egui::TextStyle) -> f32 {
    Into::<egui::WidgetText>::into(text)
        .into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            style.clone(),
        )
        .size()
        .x
}

/// 一块的小标题：设计稿 `.sec`（说明字号、弱字色，同 [`look::section`]），行高照稿；底下空出
/// `section-title-gap` 再摆那一块（设计稿 `.col` 的 `gap:8px`）。
fn section(ui: &mut egui::Ui, title: &str) {
    let tokens = Tokens::builtin();
    ui.label(
        egui::RichText::new(title)
            .small()
            .weak()
            .line_height(lh(tokens.font.size_small)),
    );
    ui.add_space(tokens.space.section_title_gap);
}

/// 一张卡：设计稿 `.card`——面板底、分隔线那一档描边、大圆角。颜色、描边与圆角取 `Visuals`
/// （[`crate::look::install`] 从令牌装上去的那几格），内边距由调用方照稿给（取令牌）。卡里头
/// 照旧用缺省的控件间距 `spacing`（这一屏外面把竖向间距清成了 0）。
///
/// `accent` 是设计稿 `.runcard` 那一张：描边换成强调色，左边再压一条强调色竖条（宽取令牌
/// `runcard-bar`），正在跑的那一趟一眼认得出。
///
/// **竖条怎么画**：稿上是 `box-shadow: inset 3px 0 0`，egui 的框画不出内阴影。于是框的底色铺成
/// 强调色，内容画之前先占一格，画完再把那一格填成一块面板底的圆角矩形——左边让出竖条那么宽、其余
/// 三边让出描边那么宽。圆角那一段的竖条跟着弧线走，与稿上的内阴影是同一个样子。
fn card<R>(
    ui: &mut egui::Ui,
    accent: bool,
    margin: egui::Margin,
    spacing: egui::Vec2,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let visuals = ui.visuals();
    let (fill, radius) = (visuals.window_fill, visuals.window_corner_radius);
    let mut stroke = visuals.widgets.noninteractive.bg_stroke;
    if !accent {
        return egui::Frame::new()
            .fill(fill)
            .stroke(stroke)
            .corner_radius(radius)
            .inner_margin(margin)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = spacing;
                ui.set_width(ui.available_width());
                add(ui)
            })
            .inner;
    }
    stroke.color = visuals.selection.stroke.color;
    let shown = egui::Frame::new()
        .fill(stroke.color)
        .stroke(stroke)
        .corner_radius(radius)
        .inner_margin(margin)
        .show(ui, |ui| {
            let 底 = ui.painter().add(egui::Shape::Noop);
            ui.spacing_mut().item_spacing = spacing;
            ui.set_width(ui.available_width());
            (底, add(ui))
        });
    let (底, inner) = shown.inner;
    let mut 面板 = shown.response.rect.shrink(stroke.width);
    面板.min.x = shown.response.rect.min.x + Tokens::builtin().layout.runcard_bar;
    ui.painter()
        .set(底, egui::epaint::RectShape::filled(面板, radius, fill));
    inner
}

/// 一条进度条，卡上与状态栏上走的是这同一个。
///
/// **说不出走了几成时画一条动着的条**，不是一条停在 0% 的条——后者看着像卡住了。
///
/// **填充取强调色**（令牌 `accent`，设计稿 `.bar i`）：egui 缺省取 `selection.bg_fill`（令牌
/// `accent-soft`），浅色主题下与轨道那一色（`sunken`）几乎一样，条走到哪儿看不出来。
fn progress_bar(visuals: &egui::Visuals, fraction: Option<f32>) -> egui::ProgressBar {
    let bar = match fraction {
        None => egui::ProgressBar::new(0.0).animate(true),
        Some(fraction) => egui::ProgressBar::new(fraction),
    };
    bar.fill(visuals.selection.stroke.color)
}

/// 正在跑那张卡底下那一排里的一格：名是弱字色，数照稿加粗、用强调字色（设计稿 `.runcard .meta b`）。
///
/// **名与数拼成一段字画**（一个 `LayoutJob`）：屏上读出来仍是「进度 34%」这一整句，粗的只有数。
///
/// **两段都在行里居中**（`valign`）：数是拉丁粗体，这一段字的一行比常规体高一截；缺省那样压在
/// 行底的话，常规体那几个字比同一排里只有常规体的「在做什么」低半截。居中之后，这一排横着居中
/// 摆开时常规体的字落在同一条基线上。
fn meta(ui: &mut egui::Ui, name: &str, value: &str) {
    let size = Tokens::builtin().font.size_small;
    let visuals = ui.visuals();
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &format!("{name} "),
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(size),
            line_height: lh(size),
            color: visuals.weak_text_color(),
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    job.append(
        value,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::new(size, font::strong_family()),
            line_height: lh(size),
            color: visuals.strong_text_color(),
            valign: egui::Align::Center,
            ..Default::default()
        },
    );
    ui.label(job);
}

/// 走了几成排成百分比，四舍五入到整数，如 `38%`。卡上与状态栏上走的是这同一个。
fn percent(fraction: f32) -> String {
    format!("{:.0}%", (fraction * 100.0).round())
}

/// 状态栏上任务那一句：「名字 · 38% · 剩余约 3 分 12 秒」。说不出的那几段不画。
fn status_line(live: &Live) -> String {
    let mut line = live.name.clone();
    if let Some(fraction) = live.progress.fraction() {
        line.push_str(&format!(" · {}", percent(fraction)));
    }
    if let Some(left) = live.remaining() {
        line.push_str(&format!(" · 剩余约 {}", elapsed(left)));
    }
    line
}

/// 正在跑的那一趟：设计稿 `.runcard`——名字与「停止」一排，一条进度条，底下一排
/// 进度、已用、约剩、在做什么。返回「按了停下没有」。
fn running_ui(ui: &mut egui::Ui, live: &Live) -> bool {
    let tokens = Tokens::builtin();
    // 设计稿 `.runcard` 是一张网格：名字与「停止」一排、进度条一排、底下那一排，排与排之间 8 点。
    ui.spacing_mut().item_spacing.y = tokens.space.card_row_gap;
    let mut stopped = false;
    ui.horizontal(|ui| {
        // 字号直接取令牌 `size-title`，**不走具名字号那一档**（`look::TITLE`）：开窗头一帧
        // 观感基线才装上去，而那一帧手上这个 `Ui` 还带着装之前的样式——台上头一帧就有活在跑时
        // （开场走完向导直接开扫就是这样），按名字找那一档会当场 panic。
        ui.label(font::strong(&live.name).size(tokens.font.size_title));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if live.stopping {
                // 按下停下到真的停之间隔着一步——**如实说出来**，不然人会以为按钮没反应。
                ui.colored_label(ui.visuals().warn_fg_color, "正在停……走到下一步就停");
            } else if look::buttons(ui, |ui| {
                ui.scope(|ui| {
                    // 设计稿 `.btn.warn`：白底、红字、红描边，与库屏「移除」同一档（拿主意的人裁，挂单 `Q834`）。
                    // 默认那一档按钮的字号（12.5）只在 `look::buttons` 里才装得上（挂单 `Q862`）。
                    look::warn_button(ui.visuals_mut());
                    ui.button("停止")
                        .on_hover_text("停在两步之间：不留半截状态。")
                })
                .inner
            })
            .clicked()
            {
                stopped = true;
            }
        });
    });
    let fraction = live.progress.fraction();
    ui.add(progress_bar(ui.visuals(), fraction).desired_height(tokens.layout.runcard_progress));
    // 四段横着居中摆开，基线靠 [`meta`] 里两段字各自在行里居中对上。**别换成 `with_layout`
    // 底边对齐**：那一块会占满这一屏剩下的高，卡片被撑到屏底，等待中与历史全挤出视口。
    // 这一排不垫按钮那一档的最矮行高（28 点）。**在横排开出来之前清掉**：横排一开就按这一格占好最矮的
    // 高，进了横排再清已经晚了，这一排会比稿上高出 9 点。
    ui.spacing_mut().interact_size.y = 0.0;
    ui.horizontal_wrapped(|ui| {
        // 设计稿 `.runcard .meta` 的 `gap:18px`。
        ui.spacing_mut().item_spacing.x = tokens.space.meta_gap;
        // 说不出走了几成就不印百分比——与那条动着的条同一个口径。
        if let Some(fraction) = fraction {
            meta(ui, "进度", &percent(fraction));
        }
        meta(ui, "已用", &elapsed(live.elapsed));
        // **算不出来就一格都不画**：核心那一侧交出「没有」的时候（总步数还没报、
        // 或者走了零成），这儿连个占位都不摆。一个会跳的「剩余约」比没有「剩余约」更坏
        // ——维护者会照它安排接下来一小时干什么。折算本身在
        // [`Live::remaining`](romcat_core::task::Live::remaining)：这一层只画。
        if let Some(left) = live.remaining() {
            meta(ui, "剩余约", &elapsed(left));
        }
        // 在做什么：第几步、叫什么、这一步里走了几件——那一句由核心库折（`Progress::render`）。
        ui.label(
            egui::RichText::new(live.progress.render())
                .small()
                .weak()
                .line_height(lh(tokens.font.size_small)),
        );
    });
    stopped
}

/// 历史里一行的四格：任务（名字，底下跟那半句说明，在那一列那么宽里折行）、结果、耗时、时间。
/// 格与格之间留 `gap`。
fn history_cells(ui: &mut egui::Ui, record: &Record, at: &str, columns: Columns, gap: f32) {
    let 字号 = &Tokens::builtin().font;
    // 这一行的字照稿 `.hist td` 12.5 号（`size-small-plus`，挂单 `Q840`）。
    let 行字号 = 字号.size_small_plus;
    // 结果那一格按标签自己那么高分出去（[`look::chip`]：说明字号一行，上下各半档留白）。给 0 的话这一格
    // 按零高居中，标签从行中线往下长，比同一行别的格低半个标签、整行也被撑高。
    let 标签高 = ui.text_style_height(&egui::TextStyle::Small) + step(0);
    let detail = record.ending.detail();
    // **只有名字一行时，名字在整行里竖直居中**（设计稿 `vertical-align:middle`）：这一栏最先摆，那时
    // 这一行还没被标签撑高，不垫的话名字顶着上沿，比同一行别的格高出一两点。上下各垫标签比一行字
    // 高出的那一半。底下还有一句说明时这一栏本来就最高，别的格照它居中，不垫。
    let 垫 = if detail.is_none() {
        ((标签高 - 行字号 * 字号.line_height) / 2.0).max(0.0)
    } else {
        0.0
    };
    ui.vertical(|ui| {
        ui.set_width(columns.name);
        ui.add_space(垫);
        ui.label(
            font::strong(&record.name)
                .size(行字号)
                .line_height(lh(行字号)),
        );
        // **说明整句由核心库说**（[`Ending::detail`]）：部分完成留下了什么、下次从哪儿接着来，
        // 失败停在哪一步、为什么。这一层一个字都不添。
        if let Some(detail) = detail {
            ui.label(
                egui::RichText::new(detail)
                    .small()
                    .weak()
                    .line_height(lh(字号.size_small)),
            );
        }
        ui.add_space(垫);
    });
    ui.add_space(gap);
    ui.allocate_ui_with_layout(
        egui::vec2(columns.chip, 标签高),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(columns.chip);
            look::chip(ui, tone(&record.ending), record.ending.word());
        },
    );
    ui.add_space(gap);
    cell(
        ui,
        columns.took,
        行字号,
        true,
        egui::RichText::new(elapsed(record.elapsed)).size(行字号),
    );
    ui.add_space(gap);
    // 本地时间的短格式（[`crate::clock::Clock::short`]），不是今年的才带年份（挂单 `Q831`）。
    cell(
        ui,
        columns.at,
        行字号,
        true,
        egui::RichText::new(at).size(行字号).weak(),
    );
}

/// 那一档收场的标签用哪种语气，照设计稿 `renderTasks()`：完成 `t-hi`、已取消 `t-none`、
/// 部分完成 `t-mid`、失败 `t-lo`。
///
/// **失败与已取消不许长得跟完成一样**：那句「跑了 X 秒」就成了骗人的话。**颜色又不是唯一
/// 线索**：标签上照样写着那一档的词（[`Ending::word`]，逐字是词表那四档）。颜色本身由
/// [`look::tone_colors`] 回答，这里只挑语气。
fn tone(ending: &Ending<()>) -> Tone {
    match ending {
        Ending::Done(()) => Tone::Good,
        Ending::Stopped => Tone::Neutral,
        Ending::Halfway { .. } => Tone::Caution,
        Ending::Failed { .. } => Tone::Bad,
    }
}

/// 一段时长排成人看得懂的样子。**与报告那一侧同一个算法**
/// （[`human_duration`]），免得同一趟活在两处印出不一样的数。
fn elapsed(elapsed: Duration) -> String {
    human_duration(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
}
