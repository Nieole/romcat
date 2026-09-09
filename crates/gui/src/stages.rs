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
//! [`Section::start`] 是**这一段唯一的排活入口**，各屏空态上的捷径调的也是它
//! （票 `gui-self-sufficient/09` 要把队列屏那三处「先跑一次 `romcat identify`」换成
//! 就地按钮）——窗口上那一层由
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
//! **识别与折标题在这件事上不一样，而差别是真的**：识别一路往中立库写批，按停时
//! 已经算完的那些结论真的落了库，所以它报**停在半路**；折标题的写是「清掉再写回」，
//! 中间停下等于把整份**标题集合**丢掉——所以它的最后一个停下点摆在写回**之前**
//! （`romcat_core::title::run_task`），那一趟要么写完、要么一个字节都没写，
//! 按停记的是「停了」。两者都由长入口自己说
//! （`romcat_core::task::Handle::halfway` 的文档写着这条分界）。

use std::path::{Path, PathBuf};

use romcat_core::catalog::Roots;
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
            ran: None,
            error: None,
            notice: None,
        }
    }

    /// 从库里重新问一遍：每一道工序还差多少。**一个字节都不读主库**——
    /// 外置盘不在位时这几行照样看得见。
    pub fn reload(&mut self, site: &Site) {
        self.stages = Stages::survey(&site.catalog);
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
    /// **这是这一段唯一的排活入口**：库屏工序段那颗按钮与别处的捷径（票
    /// `gui-self-sufficient/09` 要在队列屏空态上加的那几颗）调的都是它，排的是同一趟活。
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
        if let Some(stage) = 要跑 {
            self.start(stage, site, tasks);
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
