//! **库**那一屏：这个库由什么构成——**一组根**，加上让识别能干活的**数据源**。
//!
//! ## 两半东西为什么同屏
//!
//! 它们是同一件事：**让识别能干活的原料**。新用户最容易卡的「扫完了怎么没认出来」，
//! 在这一屏上一眼看得见——某个源那一行是空的。
//!
//! ## 领域判断一条都不在这里
//!
//! 加一个根要拦哪几种情况（重名、套在一起、圈进工作目录）在
//! [`romcat_core::catalog::roots::add_root`]；一个源取回来了没有、有多少条、
//! 上次什么时候取的在 [`romcat_core::sources`]。这一层只画、只转发（ADR-0005）。
//!
//! ## 长活一律排到任务台上
//!
//! 扫一趟真库 **37.1 分钟**，取一趟 DAT 是几百 MB 的下载。它们跑在画帧那条线程上的话
//! 窗口就是一块白板。所以这一屏一个长活都不自己跑，全排到 [`Tasks`] 上——
//! 与子库屏排差量预览走的是同一条路。
//!
//! ## 后台那条线程写的是哪一份库
//!
//! 扫描要**写**中立库，而 `rusqlite::Connection` 不是 `Sync`。于是后台线程自己
//! 按文件路径再开一份（`Catalog::open`），跑完这一屏 `reload` 一次。
//! 两份连接同时写同一个文件由 SQLite 的 WAL 与 `busy_timeout` 兜着。
//! **只活在内存里的库（合成数据）没有文件**，那时候直说扫不了——不偷偷开一份空库。

use std::path::PathBuf;
use std::time::Duration;

use romcat_core::catalog::Catalog;
use romcat_core::catalog::roots::{self, LibraryRoot, RootStats};
use romcat_core::fs::RealFs;
use romcat_core::report::{human_bytes, human_duration, human_time, thousands};
use romcat_core::scan::{self, CheckpointOptions, Jobs, ScanOptions};
use romcat_core::site::Site;
use romcat_core::sources::{self, Source, SourceState, SourceStatus};
use romcat_core::task::Halted;

use crate::task::{Product, Tasks};

/// 两次存**断点**的最小间隔。**与命令行同一个数**（`romcat scan` 默认 15 秒）：
/// 界面停下的那一趟与命令行 `--resume` 接的是同一个文件，两边攒的活也该一样多。
const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(15);

/// 一个根在这一屏上要画的那几格。
#[derive(Debug, Clone)]
pub struct RootRow {
    /// 根本身：名字、位置、上次扫描的结果。
    pub root: LibraryRoot,
    /// 它现在装着多少东西。**从库里现折，不碰磁盘**——盘没挂上时照样有数。
    pub stats: RootStats,
    /// 这个目录眼下在不在位。不在位不妨碍上面两样看得见。
    pub mounted: bool,
}

/// 库那一屏。
pub struct Screen {
    /// 工作目录：加根时拿它守住「中立库不许被圈进主库」（ADR-0004），
    /// 数据源也住在它下面。
    workspace: PathBuf,
    roots: Vec<RootRow>,
    sources: Vec<SourceStatus>,
    /// 「添加目录」那两个输入框。
    new_path: String,
    new_name: String,
    /// 正在跑的那几趟活的任务号，用来禁掉重复按下。
    running: Vec<(u64, Job)>,
    /// 点了「移除」还没点头的那个根。
    removing: Option<String>,
    error: Option<String>,
    notice: Option<String>,
}

/// 这一屏排上去的活是哪一种。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    /// 扫一个根。
    Scan(String),
    /// 取一个数据源。
    Fetch(Source),
}

impl Default for Screen {
    fn default() -> Self {
        Self::new(PathBuf::new())
    }
}

impl Screen {
    /// 开一个。`workspace` 是**工作目录**。
    #[must_use]
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            roots: Vec::new(),
            sources: Vec::new(),
            new_path: String::new(),
            new_name: String::new(),
            running: Vec::new(),
            removing: None,
            error: None,
            notice: None,
        }
    }

    /// 从库里重新读一遍：一组根、每个根装着多少、三个数据源什么状况。
    pub fn reload(&mut self, site: &Site) {
        self.roots.clear();
        match site.catalog.roots() {
            Ok(found) => {
                for root in found {
                    let stats = site.catalog.root_stats(&root.name).unwrap_or_default();
                    let mounted = std::path::Path::new(&root.path).is_dir();
                    self.roots.push(RootRow {
                        root,
                        stats,
                        mounted,
                    });
                }
            }
            Err(error) => self.error = Some(format!("这份中立库读不动：{error}")),
        }
        self.sources = sources::survey(&self.workspace);
    }

    /// 这个库由哪几个根构成。测试拿它核对。
    #[must_use]
    pub fn roots(&self) -> &[RootRow] {
        &self.roots
    }

    /// 三个数据源现在什么状况。
    #[must_use]
    pub fn sources(&self) -> &[SourceStatus] {
        &self.sources
    }

    /// 这一屏眼下报出来的那句错；没有就是 `None`。
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// 这一屏眼下报出来的那句话（不是错）。
    #[must_use]
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// 加一个根。**校验在核心里**（[`roots::add_root`]），这里只把话转出来。
    ///
    /// 加进来之后**还没扫过**：那一行照实写「还没扫过」，按「扫描」才真去读盘。
    pub fn add_root(&mut self, site: &Site, path: &str, name: &str) {
        let path = std::path::Path::new(path.trim());
        if path.as_os_str().is_empty() {
            self.error = Some("先填一个目录。".to_string());
            return;
        }
        if !path.is_dir() {
            self.error = Some(format!(
                "{} 不是一个目录。加根只认目录——主库是一组目录（`CONTEXT.md` 的**根**）。",
                romcat_core::path::display(path)
            ));
            return;
        }
        // 化成可比较的绝对形态之后再交给核心：套没套在一起是拿路径比出来的，
        // 而 `/var` 与 `/private/var` 这类链接不化开就比不出来。
        let normalized = romcat_core::path::normalize_existing(path);
        let name = if name.trim().is_empty() {
            normalized
                .file_name()
                .map(|it| it.to_string_lossy().into_owned())
                .unwrap_or_else(|| "主库".to_string())
        } else {
            name.trim().to_string()
        };
        match roots::add_root(&site.catalog, Some(&self.workspace), &name, &normalized) {
            Ok(root) => {
                self.error = None;
                self.notice = Some(format!(
                    "加上了根「{}」。按「扫描」把它收进这份中立库。",
                    root.name
                ));
                self.new_path.clear();
                self.new_name.clear();
                self.reload(site);
            }
            Err(why) => self.error = Some(why.to_string()),
        }
    }

    /// 移除一个根：**这个根下面的记录整批删掉**，返回去掉了多少变体。
    ///
    /// 丢掉的全是可再生的（中立库整份可再生）；**沉淀库一个字都不动**。
    pub fn remove_root(&mut self, site: &mut Site, name: &str) {
        match site.catalog.remove_root(name) {
            Ok(gone) => {
                self.error = None;
                self.removing = None;
                self.notice = Some(format!(
                    "移除了根「{name}」，从中立库里去掉 {} 个变体。\
                     沉淀库一条都没动——那里面是你亲手定的东西。",
                    thousands(gone)
                ));
                self.reload(site);
            }
            Err(error) => self.error = Some(format!("移不掉：{error}")),
        }
    }

    /// 扫一个根，**排到任务台上**。
    ///
    /// 后台那条线程按文件路径自己开一份中立库：`rusqlite::Connection` 不是 `Sync`，
    /// 界面这条线程手里那一份交不过去。只活在内存里的库没有文件，那时直说扫不了。
    ///
    /// ## 两件事与命令行折齐
    ///
    /// - **写断点**（[`CheckpointOptions`]）：按停之后那句「下次接着跑」得有依据，
    ///   而依据只能是断点文件。路径由 [`Site::checkpoint_path`] 折，与
    ///   `romcat scan --resume` 找的是同一个文件。
    /// - **被按停的那一趟折成 [`Halted`]**：`scan` 中断时交出来的仍是
    ///   `Ok(ScanOutcome { interrupted: true, .. })`——那份「到目前为止」的体检报告
    ///   命令行还要拿去印，它退 130 也靠这一位，所以核心那边不动。任务台认「停了」
    ///   的判据是**那句话正是 [`Halted`] 交出来的那一句**
    ///   （`romcat_core::task::Board::settle`），于是折这一下的活落在这里：
    ///   不折的话，按停的那一趟会在任务屏历史里记成「完成」、库屏说「跑完了」，
    ///   而根那一行写着「那一趟被中断」——同一趟活三处各说各的。
    pub fn scan(&mut self, site: &Site, tasks: &mut Tasks, name: &str) {
        if self.job_of(name).is_some() {
            return;
        }
        let Some(root) = self.roots.iter().find(|row| row.root.name == name) else {
            self.error = Some(format!("没有叫「{name}」的根。"));
            return;
        };
        let Some(file) = site.catalog.file().map(std::path::Path::to_path_buf) else {
            self.error = Some(
                "这份中立库只活在内存里，扫描写不进去。\
                 开一份落在磁盘上的库再来（`romcat-gui --library <名字>`）。"
                    .to_string(),
            );
            return;
        };
        let path = PathBuf::from(&root.root.path);
        if !path.is_dir() {
            self.error = Some(format!(
                "根「{name}」不在位：{}。插上那块盘再扫。\
                 上次扫出来的东西照样看得见——它住在中立库里，不跟着盘走。",
                root.root.path
            ));
            return;
        }
        let workspace = self.workspace.clone();
        let owned = name.to_string();
        let title = format!("扫描 · {owned}");
        // **断点路径在这条线程上折**：后台那条线程手里没有现场（`Site` 交不过去）。
        let checkpoint = site.checkpoint_path(&self.workspace, name);
        let id = tasks.queue(title, move |task| {
            // 后台这条线程自己开一份写得动的中立库：`rusqlite::Connection` 不是 `Sync`，
            // 界面那条线程手里那一份交不过来。
            let mut catalog = Catalog::open(&file).map_err(|error| error.to_string())?;
            let mut options = ScanOptions::named(&path, &owned);
            options.workspace = Some(workspace);
            // 界面上不给并发档：开扫前探一探介质自己定，与命令行默认那条路一样
            // （挂账 D9）。
            options.jobs = Jobs::Adaptive;
            // **按停之后停在的地方要是干净的**（规格 58）：干净的依据就是这个文件。
            // `resume: true` 是界面上「重扫」的意思——上一趟停在哪儿，这一趟接着走；
            // 完整扫完的那一趟会把它删掉（`scan::finish_checkpoint`），所以它不会
            // 让下一趟误以为还有活没干完。
            options.checkpoint = Some(CheckpointOptions {
                path: checkpoint,
                interval: CHECKPOINT_INTERVAL,
                resume: true,
            });
            match scan::scan(&RealFs::new(), &mut catalog, &options, task) {
                // 中断的那一趟折成 [`Halted`] 那句话——任务台认的正是它。
                Ok(outcome) if outcome.interrupted => Err(Halted.to_string()),
                Ok(outcome) => Ok(Product::Scanned(Box::new(outcome))),
                Err(error) => Err(error.to_string()),
            }
        });
        self.error = None;
        self.running.push((id, Job::Scan(name.to_string())));
    }

    /// 取回一个数据源，**排到任务台上**。
    pub fn fetch(&mut self, tasks: &mut Tasks, source: Source) {
        if self
            .running
            .iter()
            .any(|(_, job)| *job == Job::Fetch(source))
        {
            return;
        }
        let workspace = self.workspace.clone();
        let title = format!("取回 · {}", source.label());
        let id = tasks.queue(title, move |task| {
            sources::refetch(source, &workspace, task).map(Product::Fetched)
        });
        self.error = None;
        self.running.push((id, Job::Fetch(source)));
    }

    /// 任务台交回来一趟跑完的活。**不是自己那一趟就放过去。**
    ///
    /// 返回「认领了没有」，好让上一层知道要不要接着问别的屏。
    pub fn settle(&mut self, site: &Site, done: &romcat_core::task::Finished<Product>) -> bool {
        let Some(at) = self.running.iter().position(|(id, _)| *id == done.id) else {
            return false;
        };
        let (_, job) = self.running.remove(at);
        match (&done.ended, &job) {
            // **「跑完了」只说给真的跑完的那一趟听。** 被按停的那一趟在
            // [`Screen::scan`] 里就折成了 [`Halted`]，落到这儿是 `Stopped` 那一支
            // ——不折的话它会长着 `Product` 的样子走这一支，屏上说「跑完了」，
            // 而根那一行同时写着「那一趟被中断」。
            (romcat_core::task::Done::Product(_), _) => {
                self.notice = Some(format!("{} 跑完了。", done.name));
            }
            // 扫描停下来的地方是干净的，依据是那个**断点**文件（[`Screen::scan`] 设的）：
            // 再按一次「重扫」从停下的地方接着走，命令行 `romcat scan --resume` 也认它。
            (romcat_core::task::Done::Stopped, Job::Scan(_)) => {
                self.notice = Some(format!(
                    "{} 被按停了。停下来的地方是干净的：断点已经写下，\
                     再按「重扫」从那儿接着跑。",
                    done.name
                ));
            }
            (romcat_core::task::Done::Stopped, Job::Fetch(_)) => {
                self.notice = Some(format!("{} 被按停了。", done.name));
            }
            (romcat_core::task::Done::Failed { step, why }, _) => {
                self.error = Some(format!("{} 在「{step}」这一步失败了：{why}", done.name));
            }
        }
        // 扫完与取完都会改库或改数据源，两样都重读一遍。**认领哪一种都一样**：
        // 这一屏画的两张表各有一半靠对方那一趟才准（扫完变体数变了，取完记录数变了）。
        // **被按停的那一趟照样要重读**：它写进中立库的那半份记录是真的。
        self.reload(site);
        true
    }

    /// 哪几个根上有活在跑。
    fn busy_roots(&self) -> Vec<String> {
        self.running
            .iter()
            .filter_map(|(_, job)| match job {
                Job::Scan(name) => Some(name.clone()),
                Job::Fetch(_) => None,
            })
            .collect()
    }

    /// 这个根上有没有正在跑的活。
    #[must_use]
    pub fn job_of(&self, name: &str) -> Option<u64> {
        self.running
            .iter()
            .find(|(_, job)| *job == Job::Scan(name.to_string()))
            .map(|(id, _)| *id)
    }

    /// 顶栏上属于这一屏的那一段。
    pub fn status(&mut self, ui: &mut egui::Ui, _site: &Site) {
        let 没取回 = self.sources.iter().filter(|it| !it.ready()).count();
        let mut line = format!("{} 个根", self.roots.len());
        if 没取回 > 0 {
            line.push_str(&format!(" · {没取回} 个数据源还没取回"));
        }
        ui.label(line);
    }

    /// 画一帧。
    pub fn ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("库屏")
                .show(ui, |ui| self.body(ui, site, tasks));
        });
    }

    fn body(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        ui.heading("库");
        ui.weak("这个库由什么构成：根 + 数据源");
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if let Some(notice) = &self.notice {
            ui.weak(notice);
        }
        ui.separator();

        self.roots_ui(ui, site, tasks);
        ui.add_space(18.0);
        self.sources_ui(ui, tasks);
    }

    fn roots_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        ui.horizontal(|ui| {
            ui.strong(format!("根 · {} 个", self.roots.len()));
            ui.weak("主库是一组根：几块盘都能加进同一个库，扫完收进同一份中立库");
        });

        let mut 要扫 = None;
        let mut 要移除 = None;
        let mut 要点头 = None;
        let mut 要收回 = false;
        // 画的时候不改自己：按下去的那几下先记下来，画完再动
        // （借用检查器要的，也让「按一下发生什么」读起来是一条直线）。
        let roots = self.roots.clone();
        let 忙的 = self.busy_roots();
        egui::Grid::new("根")
            .num_columns(6)
            .striped(true)
            .show(ui, |ui| {
                for header in ["根名", "路径", "变体", "容量", "上次扫描", ""] {
                    ui.strong(header);
                }
                ui.end_row();
                for row in &roots {
                    ui.label(&row.root.name);
                    if row.mounted {
                        ui.weak(&row.root.path);
                    } else {
                        // **盘没挂上照样看得见上次结果**——那是这一行存在的一半理由。
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            format!("{}（不在位）", row.root.path),
                        );
                    }
                    ui.label(thousands(row.stats.variants));
                    ui.label(human_bytes(row.stats.bytes));
                    ui.label(last_scan(row));
                    ui.horizontal(|ui| {
                        let 忙 = 忙的.contains(&row.root.name);
                        let 标签 = if row.root.scan.is_some() {
                            "重扫"
                        } else {
                            "扫描"
                        };
                        if ui
                            .add_enabled(!忙, egui::Button::new(标签))
                            .on_hover_text("排到任务台上跑，期间照常用别的屏")
                            .clicked()
                        {
                            要扫 = Some(row.root.name.clone());
                        }
                        if self.removing.as_deref() == Some(row.root.name.as_str()) {
                            ui.colored_label(
                                ui.visuals().warn_fg_color,
                                format!("会去掉 {} 个变体", thousands(row.stats.variants)),
                            );
                            if ui.button("确认移除").clicked() {
                                要移除 = Some(row.root.name.clone());
                            }
                            // **「算了」得真的算了。** 一个只能前进不能后退的破坏性确认，
                            // 比不加确认更坏。
                            if ui.button("算了").clicked() {
                                要收回 = true;
                            }
                        } else if ui.add_enabled(!忙, egui::Button::new("移除")).clicked() {
                            要点头 = Some(row.root.name.clone());
                        }
                    });
                    ui.end_row();
                }
            });

        if 要收回 {
            self.removing = None;
        }
        if let Some(name) = 要点头 {
            self.removing = Some(name);
        }
        if let Some(name) = 要扫 {
            self.scan(site, tasks, &name);
        }
        if let Some(name) = 要移除 {
            self.remove_root(site, &name);
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("添加目录");
            ui.add(
                egui::TextEdit::singleline(&mut self.new_path)
                    .hint_text("那块盘上的目录")
                    .desired_width(320.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.new_name)
                    .hint_text("根名（不填就按目录名取）")
                    .desired_width(180.0),
            );
            if ui.button("+ 添加目录").clicked() {
                let (path, name) = (self.new_path.clone(), self.new_name.clone());
                self.add_root(site, &path, &name);
            }
        });
    }

    fn sources_ui(&mut self, ui: &mut egui::Ui, tasks: &mut Tasks) {
        ui.horizontal(|ui| {
            ui.strong("数据源 · 让识别能干活的原料");
        });
        let mut 要取 = None;
        egui::Grid::new("数据源")
            .num_columns(5)
            .striped(true)
            .show(ui, |ui| {
                for header in ["源", "记录", "上次取回", "覆盖", ""] {
                    ui.strong(header);
                }
                ui.end_row();
                for (source, status) in Source::all().into_iter().zip(&self.sources) {
                    ui.label(status.name);
                    match &status.state {
                        SourceState::Ready {
                            records,
                            fetched_at,
                        } => {
                            ui.label(thousands(*records));
                            ui.label(fetched_at.map_or_else(|| "——".to_string(), human_time));
                            ui.weak(&status.coverage);
                        }
                        // **还没取回的要被明确标出来**：那正是「扫完了怎么没认出来」
                        // 的答案。
                        SourceState::Missing => {
                            ui.colored_label(ui.visuals().warn_fg_color, "还没取回");
                            ui.weak("——");
                            ui.colored_label(ui.visuals().warn_fg_color, status.cost);
                        }
                        // **读不动不等于空**（ADR-0021）：那要人去看一眼，不是按一下取回。
                        SourceState::Broken { why } => {
                            ui.colored_label(ui.visuals().error_fg_color, "读不动");
                            ui.weak("——");
                            ui.colored_label(ui.visuals().error_fg_color, why);
                        }
                    }
                    let 忙 = self
                        .running
                        .iter()
                        .any(|(_, job)| *job == Job::Fetch(source));
                    let 标签 = if status.ready() { "重取" } else { "取回" };
                    if ui
                        .add_enabled(!忙, egui::Button::new(标签))
                        .on_hover_text(
                            "联网取一趟，排到任务台上跑。\
                             开跑之后停不下来——底下那三个入口不收中断信号（挂单 Q62）。",
                        )
                        .clicked()
                    {
                        要取 = Some(source);
                    }
                    ui.end_row();
                }
            });
        if let Some(source) = 要取 {
            self.fetch(tasks, source);
        }
        ui.add_space(6.0);
        ui.weak("扫完没认出来？先看这一屏——多半是某个源还没取回。");
    }
}

/// 一个根上次扫描那一句。**没扫过与扫过是两件事**，说清楚。
fn last_scan(row: &RootRow) -> String {
    let Some(scan) = &row.root.scan else {
        return "还没扫过".to_string();
    };
    let mut line = format!(
        "{} · {}",
        human_time(scan.at),
        human_duration(scan.elapsed_ms)
    );
    if scan.interrupted {
        line.push_str("（那一趟被中断，数字是下界）");
    }
    line
}
