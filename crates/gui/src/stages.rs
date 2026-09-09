//! 库屏上的**工序**那一段：这个库还差哪几道步骤，点一下排一趟**任务**上台。
//!
//! ## 为什么这一段说的是「还差多少」而不是「上次几点跑的」
//!
//! 人要的是**下一步该干什么**，时间戳答不了这个问题：加了一块盘重扫之后，识别那一行的
//! 数字自己就涨上去，不必再自己推理「是不是该重跑识别了」。**工序是那道步骤，
//! 任务是跑那一趟**（`CONTEXT.md` 的**工序**条）——点一道工序的按钮，排一趟任务上台。
//!
//! ## 领域判断一条都不在这里
//!
//! 「还差多少」怎么算、那一行画哪句话全在 [`romcat_core::stage`]：这一层只画、只转发
//! （ADR-0005）。识别那个数**不另造一份**——它与**待确认队列**屏、与命令行
//! `triage list` 印的是同一个（[`Catalog::not_run_count`](romcat_core::catalog::Catalog::not_run_count)）。
//!
//! ## 排一趟活的入口只有一个
//!
//! [`Section::start`] 是**这一段唯一的排活入口**，各屏空态上那几颗捷径调的也是它
//! （待确认队列屏四处、浏览屏一处，从前写的都是「去开终端跑一次」，
//! 票 `gui-self-sufficient/09` 换成了就地的按钮）——窗口上那一层由
//! [`App::start_stage`](crate::app::App::start_stage) 递过来。**同一趟活不许有第二份
//! 实现**：两份实现迟早会在「排的时候顺手做了什么」上分叉。
//!
//! ## 后台那条线程写的是哪一份库
//!
//! 识别与折标题都要**写**中立库（两者起手都先把上一轮折出来的清干净），而
//! `rusqlite::Connection` 不是 `Sync`。于是后台那条线程按文件路径自己再开一份现场
//! （`Site::open_file`），与扫描、刮削两条路一模一样；跑完这一段 `reload` 一次。
//! **只活在内存里的库（合成数据）没有文件**，那时就地跑完——那份库小到几毫秒就走完。
//!
//! ## 按停停在哪儿
//!
//! **三支在这件事上各不一样，而差别是真的**：识别一路往中立库写批，按停时已经算完的
//! 那些结论真的落了库，所以它报**停在半路**；折标题的写是「清掉再写回」，中间停下
//! 等于把整份**标题集合**丢掉——所以它的最后一个停下点摆在写回**之前**
//! （`romcat_core::title::run_task`），那一趟要么写完、要么一个字节都没写，
//! 按停记的是「停了」；导出**一份文件一份文件地写**，写完一份就把**底本**一起存进
//! 中立库，于是它两档都有——一份都没写就停下记「停了」，写过之后停下记**停在半路**
//! （`romcat_core::adapter::transfer::export_task`）。三支都由长入口自己说
//! （`romcat_core::task::Handle::halfway` 的文档写着这条分界）。

use std::path::{Path, PathBuf};

use romcat_core::adapter::transfer::{self, ExportOptions};
use romcat_core::catalog::{ExportSetup, Roots};
use romcat_core::fs::RealFs;
use romcat_core::identify;
use romcat_core::report::thousands;
use romcat_core::site::Site;
use romcat_core::stage::{Behind, Stage, StageRow, Stages};
use romcat_core::task::{Cutoff, Ending, Finished, Handle};
use romcat_core::{title, verdict, workspace};

use crate::task::{Product, Tasks};

/// 库屏上的**工序**那一段。
pub struct Section {
    /// 工作目录：DAT 库、中文离线源、TitleID 索引都住在它下面。
    workspace: PathBuf,
    /// 每一道工序还差多少。**从核心库现折**（[`Stages::survey`]），不自己攒一份。
    stages: Stages,
    /// 正在跑的那几趟活的任务号，用来禁掉重复按下。
    running: Vec<(u64, Stage)>,
    /// 记住的那套**导出**配置：往哪个前端格式、哪个目录写。
    /// **从中立库现读**（[`Catalog::export_setup`](romcat_core::catalog::Catalog::export_setup)），
    /// 与工序那几行同一趟 [`Section::reload`]——命令行 `romcat export` 改过之后
    /// 这一屏跟着变。
    setup: Option<ExportSetup>,
    /// 那两个键**读不出来**时的那句话；读得出来（哪怕是「还没选过」）就是 `None`。
    ///
    /// **「这份库读不动」与「还没选过」是两句话**，与工序那几行同一个口径
    /// （`romcat_core::stage::export_row` 逐字写着这一条）：前者是一件该去查的事，
    /// 后者是一件该去做的事。少了这一格，读不动的那份库会被画成「第一次导出之前先选
    /// 一次」——把该去查的事说成了该去做的事。
    ///
    /// **它不占 [`Self::error`] 那一格**：`settle` 收场时才写那一格，而这一趟重读发生在
    /// 它之后，占过去会把「有几份没写」那句话冲掉。
    setup_unreadable: Option<String>,
    /// 底下那两格里人正打着的字。
    ///
    /// **与 [`Self::setup`] 分开**：那一份是库里记着的，这一份是人手上还没按「记下」的。
    /// 合成一格的话，人改了一半切走再回来，屏上会显示一套并没有记进库的配置。
    ///
    /// **两格就是两个 `String`**，与库屏加根那两格一个写法
    /// （`roots::Screen` 的 `new_path` / `new_name`）——它们是输入框里的字，不是一套
    /// 立得住的配置；立得住的那一份叫 [`ExportSetup`]，由 [`ExportSetup::check`] 折出来。
    format_draft: String,
    out_draft: String,
    /// 刚跑完的那一道工序，等窗口取走。
    ///
    /// **跑完识别之后待确认队列得自己重新列过**，而这一段够不着那一屏（ADR-0005：
    /// 屏与屏之间不该互相拿着对方）。所以这儿只放一个记号，由
    /// [`App::poll_tasks`](crate::app::App::poll_tasks) 取走——与子库屏那几个跳转记号
    /// 同一个办法。
    ran: Option<Stage>,
    error: Option<String>,
    notice: Option<String>,
}

impl Section {
    /// 开一个。`workspace` 是**工作目录**。
    #[must_use]
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            stages: Stages::default(),
            running: Vec::new(),
            setup: None,
            setup_unreadable: None,
            format_draft: String::new(),
            out_draft: String::new(),
            ran: None,
            error: None,
            notice: None,
        }
    }

    /// 从库里重新问一遍：每一道工序还差多少。**一个字节都不读主库**——
    /// 外置盘不在位时这几行照样看得见。
    pub fn reload(&mut self, site: &Site) {
        self.stages = Stages::survey(&site.catalog);
        // **读不动与还没选过分两支说**（同 `stage::export_row`）：整段一起失败不成——
        // 一个读不出来的键会让工序段上连识别那一行都消失（与 `Stages::survey` 同一条）。
        match site.catalog.export_setup() {
            Ok(setup) => {
                self.setup = setup;
                self.setup_unreadable = None;
            }
            Err(error) => {
                self.setup = None;
                self.setup_unreadable = Some(format!("中立库读不动：{error}"));
            }
        }
        // **人正打着的字不覆盖**：只在两格都还空着的时候把库里记着的那套填进去。
        if self.format_draft.is_empty()
            && self.out_draft.is_empty()
            && let Some(setup) = &self.setup
        {
            self.format_draft = setup.format.clone();
            self.out_draft = setup.out.to_string_lossy().into_owned();
        }
    }

    /// 记住的那套**导出**配置；一次都没选过就是 `None`。测试拿它核对。
    #[must_use]
    pub fn export_setup(&self) -> Option<&ExportSetup> {
        self.setup.as_ref()
    }

    /// 选一次**前端格式**与**导出目录**，记进中立库。**下一趟不必再选。**
    ///
    /// **判据在核心里**（[`ExportSetup::check`]，ADR-0005）：这一层只把话转出来
    /// ——「有没有这个格式」散一份判断到界面上，命令行与界面迟早会对同一个字给出
    /// 两种答复。
    pub fn set_export_setup(&mut self, site: &Site, format: &str, out: &str) {
        match ExportSetup::check(format, out) {
            Ok(setup) => match site.catalog.set_export_setup(&setup) {
                Ok(()) => {
                    self.error = None;
                    self.notice = Some(format!(
                        "记下了：按 {} 的格式写进 {}。下一趟点「开跑」就重导。",
                        setup.format,
                        setup.out.display(),
                    ));
                    self.format_draft = setup.format.clone();
                    self.out_draft = setup.out.to_string_lossy().into_owned();
                    self.setup = Some(setup);
                    self.setup_unreadable = None;
                }
                Err(error) => {
                    self.notice = None;
                    self.error = Some(format!("这份中立库写不进去：{error}"));
                }
            },
            Err(error) => {
                self.notice = None;
                self.error = Some(error.to_string());
            }
        }
    }

    /// 一道工序一行。测试拿它核对。
    #[must_use]
    pub fn rows(&self) -> &[StageRow] {
        self.stages.rows()
    }

    /// 某一道工序那一行；没有就是 `None`。
    #[must_use]
    pub fn of(&self, stage: Stage) -> Option<&StageRow> {
        self.stages.of(stage)
    }

    /// 这道工序上有没有**任务**在台上（排着队也算）；有就是那一趟的任务号。
    /// **测试拿它核对「按钮按不下去」那一条。**
    ///
    /// 名字不叫 `job_of`：词表**任务**那一条的 `_Avoid_` 里逐字列着 `job`
    /// （`CONTEXT.md`）。库屏那一侧的 `Job` 是这条账在旧代码里的欠款，不往新代码里扩。
    #[must_use]
    pub fn task_of(&self, stage: Stage) -> Option<u64> {
        self.running
            .iter()
            .find(|(_, running)| *running == stage)
            .map(|(id, _)| *id)
    }

    /// 这一段眼下报出来的那句错；没有就是 `None`。
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// 这一段眼下报出来的那句话（不是错）。
    #[must_use]
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// 刚跑完的那一道工序，取走就没了。
    ///
    /// **跑完识别之后待确认队列得自己重新列过**，而这一段够不着那一屏（ADR-0005：
    /// 屏与屏之间不该互相拿着对方）。所以这儿只放一个记号，由
    /// [`App::poll_tasks`](crate::app::App::poll_tasks) 取走——与子库屏那几个跳转
    /// 记号同一个办法。
    pub fn take_ran(&mut self) -> Option<Stage> {
        self.ran.take()
    }

    /// 把一道工序**排到任务台上**。
    ///
    /// **这是这一段唯一的排活入口**：库屏工序段那颗按钮与别处的捷径（队列屏空态上那
    /// 几颗「跑识别」、浏览屏撤掉压制之后那颗「折标题」）调的都是它，排的是同一趟活。
    ///
    /// 同一道工序已经在跑就**什么都不做**——那一行的按钮本来就是禁着的，这一句是给
    /// 别处的捷径兜底的。
    pub fn start(&mut self, stage: Stage, site: &mut Site, tasks: &mut Tasks) {
        if self.task_of(stage).is_some() {
            return;
        }
        let workspace = self.workspace.clone();
        let title = stage.label().to_string();
        let id = match site.catalog.file().map(Path::to_path_buf) {
            Some(file) => tasks.queue(title, move |task| {
                // 后台这条线程自己开一份写得动的现场：`rusqlite::Connection` 不是
                // `Sync`，界面那条线程手里那一份交不过来。
                let mut site = Site::open_file(&workspace, &file, None)
                    .map_err(|error| format!("这份库在后台开不出来：{error}"))?;
                run(stage, &mut site, &workspace, task)
            }),
            // 只活在内存里的库（合成数据走这条）分不出第二份连接：**就地跑完**。
            // 那时窗口确实会僵一下，但那份库小到几毫秒就走完——真库一律走上面那条。
            None => tasks.run_here(title, |task| run(stage, site, &workspace, task)),
        };
        // **上一趟的回执一起收掉**：不收的话「识别 跑完了：…」会挂在新一趟正跑着的
        // 那一行旁边，读起来像这一趟已经跑完了。
        self.error = None;
        self.notice = None;
        self.running.push((id, stage));
    }

    /// 任务台交回来一趟跑完的活。**不是自己那一趟就放过去**，返回「认领了没有」。
    pub fn settle(&mut self, site: &Site, done: &Finished<Product>) -> bool {
        let Some(at) = self.running.iter().position(|(id, _)| *id == done.id) else {
            return false;
        };
        let (_, stage) = self.running.remove(at);
        match &done.ended {
            Ending::Done(Product::Identified(outcome)) => {
                self.error = None;
                self.notice = Some(format!(
                    "{} 跑完了：{} 个变体过了一遍，命中 {}。",
                    stage.label(),
                    thousands(outcome.report.total.variants),
                    thousands(outcome.report.total.matched),
                ));
            }
            Ending::Done(Product::Titled(report)) => {
                self.error = None;
                self.notice = Some(format!(
                    "{} 跑完了：{} 个作品折出 {} 条叫法，其中 {} 个作品有中文叫法。",
                    stage.label(),
                    thousands(report.works),
                    thousands(report.entries),
                    thousands(report.chinese_works),
                ));
            }
            Ending::Done(Product::Exported(report)) => {
                // **数的是真写出去的那几份**，不是整库收敛出来的总数：撞上外面有人动过
                // 的那几份一个字节都没写，把它们算进「写进了几份」等于虚报
                // （`ExportedFile::written` 就是这条界线）。
                let 写出去的: Vec<_> = report.files.iter().filter(|file| file.written).collect();
                let 条目 = 写出去的.iter().map(|file| file.entries).sum::<u64>();
                self.notice = Some(if 写出去的.is_empty() {
                    // 每一份都被挡下、或者库里压根没东西可导。**这一档也得说话**
                    // ——它与「写了几份」长得完全不一样，而底下那句红字说的是为什么。
                    format!("{} 跑完了，可一份元数据都没写出去。", stage.label())
                } else {
                    format!(
                        "{} 跑完了：{} 个条目写进 {} 份元数据文件，实测档位 {}。\
                         一个 ROM 都没搬。",
                        stage.label(),
                        thousands(条目),
                        thousands(写出去的.len() as u64),
                        report.tier,
                    )
                });
                // **撞上手改要说出口**（验收第 5 条）：跳过的那几份是「你要的事没做，
                // 去处理一下」，与上面那句「跑完了」意思相反，所以它走的是报错那一格
                // ——两句合成一句的话，那几份被吞掉的活会被读成一次顺利的导出。
                self.error = (!report.conflicts.is_empty()).then(|| {
                    format!(
                        "有 {} 份没写——外面有人动过那些文件，**没有静默覆盖**。\
                         先看一眼那几份，确认不要了再重导。",
                        thousands(report.conflicts.len() as u64),
                    )
                });
            }
            // **停在半路**：识别起手就把上一轮的结论清干净，所以它一定动过库
            // ——记成「可以当没跑过」是骗人的。那句话由核心库折
            // （`identify::run_task` 里 `Handle::halfway` 报的那一句），这一层原样转出来：
            // **识别没有断点**，下一趟从头再算一遍。
            Ending::Halfway { left_behind, .. } => {
                self.error = None;
                self.notice = Some(format!("{} 被按停了。{left_behind}", stage.label()));
            }
            // 还排着队就被撤掉的那一趟压根没开跑：一个字节都没写。
            // **停了，什么都没留下。** 两条路走到这一档：还排着队就被撤掉（压根没开跑），
            // 以及开跑了但停在写库之前（折标题走的正是这条，见 `title::run_task`）。
            // 两条留下的是同一件事——中立库一个字节都没动——所以这儿说的是同一句话。
            Ending::Stopped => {
                self.error = None;
                self.notice = Some(format!(
                    "{} 停下了。中立库一个字节都没动，再按一次就是。",
                    stage.label(),
                ));
            }
            // **不静默结束**：哪一步、为什么，两样都说出来。
            Ending::Failed { step, why } => {
                self.error = Some(if step.is_empty() {
                    why.clone()
                } else {
                    format!("{} 在「{step}」这一步停下了：{why}", stage.label())
                });
            }
            // 别的屏排上去的活轮不到这儿——上面那道任务号已经挡掉了。
            Ending::Done(_) => {}
        }
        // **跑完当场重问一遍**：那一行的数字就是这么刷新的（验收第 6 条）。
        // 被按停的那一趟照样要重问——它写进中立库的那半份结论是真的。
        self.reload(site);
        self.ran = Some(stage);
        true
    }

    /// 画这一段。
    pub fn ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        ui.horizontal(|ui| {
            ui.strong(format!("工序 · {} 道", self.stages.rows().len()));
            ui.weak("这个库还差哪几道步骤。点一下排一趟任务上台");
        });
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if let Some(notice) = &self.notice {
            ui.weak(notice);
        }

        let mut 要跑 = None;
        // 画的时候不改自己：按下去的那一下先记下来，画完再动（借用检查器要的，
        // 也让「按一下发生什么」读起来是一条直线）。
        let rows: Vec<StageRow> = self.stages.rows().to_vec();
        egui::Grid::new("工序")
            .num_columns(3)
            .striped(true)
            .show(ui, |ui| {
                for header in ["工序", "还差多少", ""] {
                    ui.strong(header);
                }
                ui.end_row();
                for row in &rows {
                    ui.label(row.stage.label());
                    match &row.behind {
                        // **还差东西的那一行标出来**：这一段存在的全部理由就是这个数。
                        Behind::Left(left) if *left > 0 => {
                            ui.colored_label(ui.visuals().warn_fg_color, row.render());
                        }
                        // 不差什么了、以及**这一支交不出度量退回时刻**的那一行，
                        // 都不该抢眼——后者说的是「我算不出来」，不是「你该动手了」。
                        Behind::Left(_) | Behind::Unmeasured { .. } => {
                            ui.weak(row.render());
                        }
                    }
                    let 忙 = self.task_of(row.stage).is_some();
                    if ui
                        .add_enabled(!忙, egui::Button::new(if 忙 { "跑着呢" } else { "开跑" }))
                        .on_hover_text(
                            "排到任务台上跑，期间照常用别的屏；\
                             按得停——停下来留下了什么，那一趟自己会在任务台上说。",
                        )
                        .clicked()
                    {
                        要跑 = Some(row.stage);
                    }
                    ui.end_row();
                }
            });
        self.export_setup_ui(ui, site);
        if let Some(stage) = 要跑 {
            self.start(stage, site, tasks);
        }
    }

    /// 底下那一行：**导出**往哪个前端格式、哪个目录写。
    ///
    /// **第一次导出之前选一次，之后一键重导**（验收第 2、3 条）。选完记进中立库的
    /// 元数据表，与主库名同一处——**纯加键、不升结构版本**，旧库拿新程序打开照样能用
    /// （`romcat_core::catalog::export` 的模块文档）。
    fn export_setup_ui(&mut self, ui: &mut egui::Ui, site: &Site) {
        ui.add_space(6.0);
        let mut 要记下 = false;
        ui.horizontal(|ui| {
            ui.label("导出去哪儿");
            // **格式是从适配器名单里挑的，不是手打的**：打错一个字母的代价是一趟活白跑，
            // 而这份名单本来就是核心库交出来的（`adapter::names`）。
            egui::ComboBox::from_id_salt("前端格式")
                .selected_text(if self.format_draft.is_empty() {
                    "选一个前端格式"
                } else {
                    &self.format_draft
                })
                .show_ui(ui, |ui| {
                    for name in romcat_core::adapter::names() {
                        ui.selectable_value(&mut self.format_draft, name.to_string(), name);
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut self.out_draft)
                    .hint_text("导出到哪个目录")
                    .desired_width(320.0),
            );
            if ui.button("记下").clicked() {
                要记下 = true;
            }
        });
        if let Some(why) = &self.setup_unreadable {
            // **不许画成「还没选过」**：那是一件该去做的事，而这是一件该去查的事。
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("导出那套配置读不出来：{why}。这**不是**「还没选过」。"),
            );
        } else if self.setup.is_none() {
            ui.weak(
                "第一次导出之前先选一次；选完记进这份库，下一趟点「开跑」就重导。\
                 那个目录是**主库根的替身**——放进主库根，前端直接就读得到。",
            );
        }
        if 要记下 {
            let (format, out) = (self.format_draft.clone(), self.out_draft.clone());
            self.set_export_setup(site, &format, &out);
        }
    }
}

/// 后台那条线程真跑的那一趟。
///
/// **装配全在这儿**：DAT 库、沉淀库、剥离规则、中文离线源、TitleID 索引。领域判断一条
/// 都不在这一层——它只是把核心库要的原料摆齐（与命令行 `romcat identify` 摆的是同一副，
/// 只差**模型推断兜底**那一层：界面上没有价目表与花费上限那几个旋钮，所以那一层整个
/// 关着，挂单 `Q418`）。
fn run(stage: Stage, site: &mut Site, workspace: &Path, task: &Handle) -> Result<Product, Cutoff> {
    match stage {
        Stage::Identify => identify_run(site, workspace, task),
        Stage::FoldTitles => fold_titles_run(site, workspace, task),
        Stage::Export => export_run(site, workspace, task),
    }
}

/// 跑一趟**识别**。
fn identify_run(site: &mut Site, workspace: &Path, task: &Handle) -> Result<Product, Cutoff> {
    // **没有弹药就没有命中率**：DAT 库不在时直说。偷偷让 `DatRepo::open` 当场建出一份
    // 空库跑下去的话，整库都会落成「未命中」并写进中立库——那是一条**假结论**，
    // 不是一次失败。命令行开头拦的是同一件事，但两处各说各的话（那一句指的是
    // `romcat dat sync`，这一句指的是上面「数据源」那一段）——挂单 `Q423`。
    let dat = workspace::dat_repo_path(workspace);
    if !dat.exists() {
        return Err(Cutoff::failed(
            "还没有 DAT 库。先在上面「数据源」那一段把它取回来——没有弹药就没有命中率。".to_string(),
        ));
    }
    let repo = romcat_core::dat::DatRepo::open(&dat)
        .map_err(|error| Cutoff::failed(format!("DAT 库打不开：{error}")))?;
    // **沉淀库先说话**：裁决过的内容直接精确命中，不再进队列（ADR-0008）。
    let verdicts = verdict::Index::load(&site.store, &site.library)
        .map_err(|error| Cutoff::failed(format!("沉淀库读不动：{error}")))?;
    // **名字那一层的原料只有一处**（[`crate::scrape::NamingParts`]）：刮削与识别摆的是
    // 同一副，两条路各开一遍的话，同一个变体在两条路上会撞到不同的条目——而那是写进
    // 库里的结论，不是显示上的差别。
    let parts = crate::scrape::NamingParts::open(workspace, task)?;
    let naming = parts.naming();
    // **第三方 TitleID 索引没取过照样跑**：容器的明文文件名表免密钥就说得出 TitleID，
    // 查表只是把结论从「哪个游戏」抬到「哪个游戏的哪个版本」。
    let titledb =
        romcat_core::titledb::store::Store::open(&workspace::titledb_store_path(workspace))
            .ok()
            .filter(|store| store.ready().unwrap_or(false));
    // 主库那一组根：库里记着的那份。**盘不在位不拦着**——那时回盘读不到的那几份落成
    // 「无判据」，与命令行一个口径（读不到不是结论，ADR-0021）。
    let roots = Roots::load(&site.catalog)
        .map_err(|error| Cutoff::failed(format!("这份中立库读不动：{error}")))?;
    identify::run_task(
        &RealFs::new(),
        &mut site.catalog,
        &identify::Ammo {
            repo: &repo,
            verdicts: &verdicts,
            naming: &naming,
            guessing: &identify::model::Guessing::off(),
            titledb: titledb.as_ref(),
        },
        &identify::Options::new(roots),
        task,
    )
    .map(|outcome| Product::Identified(Box::new(outcome)))
    .map_err(|error| Cutoff::failed(format!("识别失败：{error}")))
}

/// 跑一趟**折标题**：把识别与刮削的结论折成每个作品的**标题集合**。
///
/// **一个字节都不读主库、一个请求都不发**（ADR-0001）：要的东西全在中立库里躺着。
/// 装配只有两样——**优先级表**与**沉淀库**，与命令行 `romcat titles` 摆的是同一副。
///
/// ## 停下的地方只有开折之前那一处
///
/// 重折是「把折出来的那批清掉再写回去」（[`title::refold`]），**中间停下等于把整份
/// 标题集合丢掉**。所以这一段在开折之前 `?` 一下把手，之后一步都不看停下的信号：
/// 那一折在真库上是秒级的事（`romcat titles` 整趟不到 3 秒，`docs/library-facts.md`）,
/// 等它走完比留下一份空集合便宜得多。于是这一趟**要么写完、要么一个字节都没写**
/// ——被按停的那一趟走的是「停了」，不是**停在半路**。
///
/// ## 优先级表读不出来就停下，不退回内置那份
///
/// 挑**显示标题**用的是同一份表，而工作目录里那份 `priorities.toml` 正是人改过的
/// 说法。悄悄退回内置那份的话，人会看见一份自己没定过的显示标题，还查不出为什么。
fn fold_titles_run(site: &mut Site, workspace: &Path, task: &Handle) -> Result<Product, Cutoff> {
    // 装配那一步加上核心库自己那几步。**核心库那个数由它自己报**
    // （`title::TASK_STEPS`）——在这儿手写一个 3，那边加一步这儿的进度条就走过头了。
    task.steps(1 + title::TASK_STEPS);
    // **「被按停了」一个字都不用凑**：`?` 一下把手，`Halted` 自己折成 `Cutoff::Halted`，
    // 任务台按支记成「停了」（`Cutoff` 的文档）。底下 `run_task` 交上来的那一支同理，
    // 折它的是 `From<FoldTitlesError> for Cutoff`。
    task.step("读优先级表")?;
    let priorities = romcat_core::sync::prepare::priorities(None, workspace)?;
    // **能停到哪儿、压掉的叫法怎么筛，全在核心库那一段**（`title::run_task`）：
    // 这一层只把料摆齐、把把手递进去（ADR-0005）。
    let report = title::run_task(&mut site.catalog, &site.store, &priorities, task)?;
    Ok(Product::Titled(Box::new(report)))
}

/// 跑一趟**导出**：把中立库写成前端能读的元数据，铺在导出目录里。
///
/// **一个字节都不读主库、一个 ROM 都不搬**（ADR-0004、验收第 6 条）：要的东西全在
/// 中立库里躺着，写出去的只有元数据文件。装配只有两样——记住的那套**配置**与
/// **优先级表**，与命令行 `romcat export` 摆的是同一副。
///
/// ## 两个旋钮界面上一个都不给
///
/// 命令行那边有 `--dry-run` 与 `--force`，这儿两个都钉死在「关」上：
///
/// - **只排计划**在这一屏上没有落点——工序段那一行问的是「还差多少」，一次不写盘的
///   预演答不了它，反倒会让那一行说「上次跑是刚刚」而盘上什么都没有。
/// - **照写**（`--force`）是**丢掉一次手改**，那是不可逆的事。撞上外面有人动过时
///   这一趟停下来、逐份点名（验收第 5 条），要不要丢由人自己去看那几份文件再定。
fn export_run(site: &mut Site, workspace: &Path, task: &Handle) -> Result<Product, Cutoff> {
    // 装配那两步加上核心库自己那几步。**核心库那个数由它自己报**
    // （`transfer::TASK_STEPS`）——在这儿手写一个 3，那边加一步这儿的进度条就走过头了。
    task.steps(2 + transfer::TASK_STEPS);
    task.step("读导出的配置")?;
    let setup = site
        .catalog
        .export_setup()
        .map_err(|error| Cutoff::failed(format!("这份中立库读不动：{error}")))?
        // **没选过就如实拒绝**，不替人挑一个格式与目录：挑错一个目录就是往别人的盘上
        // 写一堆文件。这一句与上面那一行的空态说的是同一件事（挂单 `Q437`）。
        .ok_or_else(|| {
            Cutoff::failed(
                "还没选过导出的前端格式与目录。先在工序段底下那一行选一次\
                 ——选完记进这份库，下一趟点一下就重导。"
                    .to_string(),
            )
        })?;
    // **找不到那个格式该说哪句话在核心里**（`ExportSetup::adapter`，ADR-0005）。
    let adapter = setup
        .adapter()
        .map_err(|error| Cutoff::failed(error.to_string()))?;
    // **优先级表读不出来就停下，不退回内置那份**：挑**显示标题**用的是同一份表，
    // 而工作目录里那份 `priorities.toml` 正是人改过的说法。悄悄退回内置那份的话，
    // 导出去的名字会与他定过的对不上，还查不出为什么（与折标题那一支同一条）。
    task.step("读优先级表")?;
    let priorities = romcat_core::sync::prepare::priorities(None, workspace)?;
    // **能停到哪儿、撞上手改怎么办，全在核心库那一段**（`transfer::export_task`）：
    // 这一层只把料摆齐、把把手递进去（ADR-0005）。
    let report = transfer::export_task(
        &mut site.catalog,
        adapter.as_ref(),
        &priorities,
        &ExportOptions {
            out: setup.out.clone(),
            dry_run: false,
            force: false,
        },
        task,
    )?;
    Ok(Product::Exported(Box::new(report)))
}
