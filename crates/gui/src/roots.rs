//! **库**那一屏：这个库由什么构成——**一组根**，加上让识别能干活的**数据源**，
//! 再加上这个库还差哪几道[**工序**](crate::stages)。
//!
//! ## 工序那一段为什么也在这一屏
//!
//! 「扫完了怎么没认出来」这个问题的答案有两种：**数据源是空的**（底下那一段画着），
//! 与**识别根本没跑过**。后者从前在这一屏上看不见，于是这一屏只答得出一半。
//! 工序那一段答的是另一半，而它与前两段是同一件事的三面——这个库由什么构成、
//! 还差什么。那一段自己住在 [`crate::stages`]，这一屏只把它摆进来。
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

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use romcat_core::catalog::Catalog;
use romcat_core::catalog::roots::{self, LibraryRoot, RootStats};
use romcat_core::fs::RealFs;
use romcat_core::report::{human_bytes, human_duration, human_time, thousands};
use romcat_core::scan::{self, CheckpointOptions, Jobs, ScanOptions};
use romcat_core::site::Site;
use romcat_core::sources::{self, Source, SourceState, SourceStatus};
use romcat_core::stage::Stage;
use romcat_core::task::{Cutoff, Ending};

use crate::font;
use crate::layout::{FOLD_EXPORT, FOLD_ROOTS, FOLD_SOURCES, Fold};
use crate::task::{Product, Tasks};
use crate::tokens::Tokens;

/// 根那一块一个根都没有时画的那一句。**空着的那一块说清下一步去哪儿办**，不画一张只有表头的表
/// （票 `gui-looks-like-the-design/06`）。
const ROOTS_EMPTY: &str = "还没有根。按右上角「添加根…」选一个目录，或者把目录贴进底下那个框\
     ——几块盘都能加进同一个库。";

/// 数据源那一块在**还没扫描**、又有源没取回时画的那一句：扫描与取回互不挡道，下一步可以两件一起办。
const SOURCES_BEFORE_SCAN: &str =
    "还没扫描。扫描的时候就可以先把这几个源取回来——识别和刮削要用它们。";

/// 屏头那颗按钮上写的字：弹系统的选目录对话框，选中的加成一个根（`Screen::pick_root`）。
pub const ADD_ROOT: &str = "添加根…";

/// 库屏两栏里左边（工序）占多少：设计稿 `.libgrid` 是 `1.25fr : 1fr`。**设计稿不给断点**，
/// 窗口窄了两栏一起收窄，不叠成一栏。
const LEFT_SHARE: f32 = 1.25 / 2.25;

/// 「添加目录」那两个框里的提示字。
///
/// **摆成常量是因为它们有第二个用处**：[开场那条向导](crate::claim)第二步问的是同样
/// 两样东西，用的必须是同一句话——同一件事在相邻两处换个说法，人就会以为它们是两件事。
pub const ROOT_HINT: &str = "那块盘上的目录";
/// 同上，根名那个框。**不填就按目录自己的名字取**，那一下在
/// [`add_root_from_fields`] 里。
pub const ROOT_NAME_HINT: &str = "根名（不填就按目录名取）";

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
    /// 第三段：**工序**——这个库还差哪几道步骤。
    stages: crate::stages::Section,
    /// 「添加目录」那两个输入框。
    new_path: String,
    new_name: String,
    /// 正在跑的那几趟活的任务号，用来禁掉重复按下。
    running: Vec<(u64, Job)>,
    /// 点了「移除」还没点头的那个根。
    removing: Option<String>,
    error: Option<String>,
    notice: Option<String>,
    /// 右边那一栏里眼下收着的那几块（[`Fold`]）。开窗时从版式偏好里交进来、每帧画完抄回去
    /// （`App::new` / `App::ui`）——这一屏自己不读写文件。
    folded: BTreeSet<&'static str>,
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
            stages: crate::stages::Section::new(workspace.clone()),
            workspace,
            roots: Vec::new(),
            sources: Vec::new(),
            new_path: String::new(),
            new_name: String::new(),
            running: Vec::new(),
            removing: None,
            error: None,
            notice: None,
            folded: BTreeSet::new(),
        }
    }

    /// 右边那一栏这一块收着没有。
    #[must_use]
    pub fn folded(&self, fold: Fold) -> bool {
        self.folded.contains(fold.id)
    }

    /// 记下右边那一栏这一块收着还是摊开。点那一块的标题栏走的就是它。
    pub fn set_folded(&mut self, fold: Fold, folded: bool) {
        if folded {
            self.folded.insert(fold.id);
        } else {
            self.folded.remove(fold.id);
        }
    }

    /// 从库里重新读一遍：一组根、每个根装着多少、三个数据源什么状况。
    pub fn reload(&mut self, site: &Site) {
        self.roots.clear();
        match site.catalog.roots() {
            Ok(found) => {
                for root in found {
                    let stats = site.catalog.root_stats(&root.name).unwrap_or_default();
                    let mounted = Path::new(&root.path).is_dir();
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
        self.stages.reload(site);
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

    /// **工序**那一段：这个库还差哪几道步骤。测试拿它核对。
    #[must_use]
    pub fn stages(&self) -> &crate::stages::Section {
        &self.stages
    }

    /// **工序**那一段，改得动的那一份：排一趟工序上台、取走「刚跑完的是哪一道」
    /// 都走它（[`crate::stages::Section`]）。
    ///
    /// 露出这一份而不是各包一层转发：包一层的话，同一趟活要穿三层壳
    /// （窗口 → 库屏 → 工序段），而中间那层一个字都不加。
    pub fn stages_mut(&mut self) -> &mut crate::stages::Section {
        &mut self.stages
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

    /// 加一个根。**判断在核心里**，这一层只把话转出来（[`add_root_from_fields`]）。
    ///
    /// 加进来之后**还没扫过**：那一行照实写「还没扫过」，按「扫描」才真去读盘。
    ///
    /// 交出**加上去的那个根名**；被拦下就是 `None`，那句话摆在 [`Self::error`] 上。
    /// 名字没填时它是从目录名取的那一个，而排一趟扫描要的正是这个名字
    /// （[`Self::scan`]）——不交出来的话，调用方就得自己再折一遍「名字空着时按目录名
    /// 取」，那是第二套算法（[开场那条向导](crate::claim)就在这条路上）。
    ///
    /// 库屏自己那颗「+ 添加目录」不看这个返回值：它接着画的那一帧本来就照着重读过的
    /// 那张表画。
    pub fn add_root(&mut self, site: &Site, path: &str, name: &str) -> Option<String> {
        match add_root_from_fields(&site.catalog, &self.workspace, path, name) {
            Ok(root) => {
                self.error = None;
                self.notice = Some(format!(
                    "加上了根「{}」。按「扫描」把它收进这份中立库。",
                    root.name
                ));
                self.new_path.clear();
                self.new_name.clear();
                self.reload(site);
                Some(root.name)
            }
            Err(why) => {
                self.error = Some(why);
                None
            }
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
    /// - **被按停的那一趟是「停在半路」**（[`Ending::Halfway`]）：`scan` 中断时交出来的仍是
    ///   `Ok(ScanOutcome { interrupted: true, .. })`——那份「到目前为止」的体检报告
    ///   命令行还要拿去印，它退 130 也靠这一位。核心那边被叫停时自己报一句
    ///   [`Handle::halfway`](romcat_core::task::Handle::halfway)，任务台照它把这一趟
    ///   记成「停在半路」，于是这一层**一个字都不用凑**：库屏说「被按停了」、任务屏
    ///   历史说「停在半路」、根那一行写着「那一趟被中断」——三处说的是同一件事。
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
        // **人工纠正也在这条线程上取**：它住沉淀库（票 `one-criterion-per-thing/07`），
        // 后台那条线程手里没有沉淀库。旧库里记着的那几条，开现场时已经搬过去了。
        let shaping_overrides = match site.shaping_overrides() {
            Ok(overrides) => overrides,
            Err(error) => {
                self.error = Some(format!("沉淀库读不出来：{error}"));
                return;
            }
        };
        let id = tasks.queue(title, move |task| {
            // 后台这条线程自己开一份写得动的中立库：`rusqlite::Connection` 不是 `Sync`，
            // 界面那条线程手里那一份交不过来。
            let mut catalog = Catalog::open(&file).map_err(|error| error.to_string())?;
            let mut options = ScanOptions::named(&path, &owned);
            options.workspace = Some(workspace);
            options.shaping_overrides = shaping_overrides;
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
                // **被按停的那一趟照旧交出产物**：那份「到目前为止」的体检报告是真的，
                // 而 `scan` 自己已经报过「停在半路」了，任务台不会把它记成「完成」。
                Ok(outcome) => Ok(Product::Scanned(Box::new(outcome))),
                // **扫描被按停时不走这条**：它照旧返回 `Ok`，另报一句「停在半路」，
                // 好让**断点**留得下来（ADR-0015）。走到这儿的都是真出错了。
                Err(error) => Err(Cutoff::failed(error.to_string())),
            }
        });
        self.error = None;
        self.running.push((id, Job::Scan(name.to_string())));
        self.sync_scan_board();
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
        // **取回 DAT 排在台上，识别就不当场拒「还没有 DAT 库」**：轮到识别时它多半已经取回来了
        // （`stages::Section::set_dat_on_board`）。认领它时拨回。
        if source == Source::Dat {
            self.stages.set_dat_on_board(true);
        }
    }

    /// 任务台交回来一趟跑完的活。**不是自己那一趟就放过去。**
    ///
    /// 返回「认领了没有」，好让上一层知道要不要接着问别的屏。
    pub fn settle(&mut self, site: &Site, done: &romcat_core::task::Finished<Product>) -> bool {
        // **工序那一段自己认领自己排的那几趟**：它按任务号认，不是它的就往下走。
        if self.stages.settle(site, done) {
            return true;
        }
        let Some(at) = self.running.iter().position(|(id, _)| *id == done.id) else {
            return false;
        };
        let (_, job) = self.running.remove(at);
        self.sync_scan_board();
        if job == Job::Fetch(Source::Dat) {
            self.stages.set_dat_on_board(false);
        }
        match (&done.ended, &job) {
            // **「跑完了」只说给真的跑完的那一趟听。** 被按停的那一趟交出来的产物
            // 长得一模一样，分开的是 `scan` 自己报的那句「停在半路」——不分的话
            // 屏上说「跑完了」，而根那一行同时写着「那一趟部分完成」。
            (Ending::Done(_), _) => {
                self.notice = Some(format!("{} 跑完了。", done.name));
            }
            // **停在半路**那一趟真跑起来过：停下来的地方是干净的，依据是那个**断点**
            // 文件（[`Screen::scan`] 设的），再按一次「重扫」从那儿接着走，命令行
            // `romcat scan --resume` 认的也是它。
            (Ending::Halfway { .. }, Job::Scan(_)) => {
                self.notice = Some(format!(
                    "{} {}。再按「重扫」从那儿接着跑。",
                    done.name,
                    done.ended.render(),
                ));
            }
            // **还排着队就被撤掉的那一趟压根没开跑**，所以这儿不许说「断点已经写下」
            // ——它一个字节都没写，哪来的断点。两档并成一句的话，那句话对这一种就是
            // 骗人的（改之前两档长得一样，这句话只能这么写；轴分开之后就不必了）。
            (Ending::Stopped, Job::Scan(_)) => {
                self.notice = Some(format!(
                    "{} 还没轮到就被撤掉了。中立库与那块盘一个字节都没动，\
                     再按一次「重扫」就是。",
                    done.name
                ));
            }
            (Ending::Halfway { .. } | Ending::Stopped, Job::Fetch(_)) => {
                self.notice = Some(format!("{} {}。", done.name, done.ended.render()));
            }
            (Ending::Failed { .. }, _) => {
                self.error = Some(format!("{} {}", done.name, done.ended.render()));
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

    /// 工序段扫描那一行（或者顶上「下一步」）按下去之后留的记号：取走就把该扫的根排上任务台
    /// （`stages::Section::take_handoff`）。**排扫描只有 [`Self::scan`] 这一份实现。**
    ///
    /// 排哪几个根：**还没完整扫过一趟的**（[`LibraryRoot::fully_scanned`]），也就是那一行数的那几个。
    /// 都扫过了，这一下就是「重新扫描」：每个根都排一趟。
    pub fn take_scan(&mut self, site: &Site, tasks: &mut Tasks) {
        if !self.stages.take_handoff(Stage::Scan) {
            return;
        }
        let 没扫完的: Vec<String> = self
            .roots
            .iter()
            .filter(|row| !row.root.fully_scanned())
            .map(|row| row.root.name.clone())
            .collect();
        let 要扫 = if 没扫完的.is_empty() {
            self.roots.iter().map(|row| row.root.name.clone()).collect()
        } else {
            没扫完的
        };
        for name in 要扫 {
            self.scan(site, tasks, &name);
        }
    }

    /// 把台上头一趟扫描的任务号告诉工序段：扫描那一行的按钮照它禁（`stages::Section::task_of`）。
    fn sync_scan_board(&mut self) {
        let id = self
            .running
            .iter()
            .find_map(|(id, job)| matches!(job, Job::Scan(_)).then_some(*id));
        self.stages.set_scan_on_board(id);
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
        let 间距 = panel_gap();
        // 屏头（设计稿 `.scrhead`）：屏名、一句说明，右边一颗「添加根…」。
        let mut 要选根 = false;
        ui.horizontal(|ui| {
            ui.heading("库");
            ui.weak("根、数据源和处理进度");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                要选根 = ui
                    .button(ADD_ROOT)
                    .on_hover_text(format!(
                        "选一个目录，加成这个库的一个根。{}",
                        crate::pick::FALLBACK_HINT
                    ))
                    .clicked();
            });
        });
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if let Some(notice) = &self.notice {
            ui.weak(notice);
        }
        ui.add_space(间距);

        // 两栏（设计稿 `.libgrid`）：左边工序段，右边根、数据源、导出设置三块。
        ui.horizontal_top(|ui| {
            let 左宽 = (ui.available_width() - 间距) * LEFT_SHARE;
            ui.vertical(|ui| {
                ui.set_width(左宽);
                panel_frame(ui)
                    .inner_margin(panel_padding())
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        self.stages.ui(ui, site, tasks);
                    });
            });
            ui.add_space((间距 - ui.spacing().item_spacing.x).max(0.0));
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                self.side_ui(ui, site, tasks);
            });
        });
        // 工序段扫描那一行按下去只留记号：这一屏自己那条扫描的路接着排（`Self::take_scan`）。
        self.take_scan(site, tasks);
        if 要选根 {
            self.pick_root(site);
        }
    }

    /// 右边那一栏：根、数据源、导出设置三块，**各自收得起来**（[`Fold`]），次序照设计稿。
    fn side_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        let 间距 = panel_gap();
        let mut 收着 = self.folded(FOLD_ROOTS);
        // 标题带着几个根，与工序段上「工序 · N 道」同一个写法：收着的时候也看得出这个库有几个根。
        let 标题 = format!("根 · {} 个", self.roots.len());
        foldable_panel(
            ui,
            &标题,
            "同一个主库可以包含多块盘或多个目录",
            &mut 收着,
            |ui| self.roots_ui(ui, site, tasks),
        );
        self.set_folded(FOLD_ROOTS, 收着);
        ui.add_space(间距);

        let mut 收着 = self.folded(FOLD_SOURCES);
        foldable_panel(
            ui,
            "数据源",
            "识别和刮削要用的本地数据",
            &mut 收着,
            |ui| self.sources_ui(ui, tasks),
        );
        self.set_folded(FOLD_SOURCES, 收着);
        ui.add_space(间距);

        let mut 收着 = self.folded(FOLD_EXPORT);
        foldable_panel(
            ui,
            "导出设置",
            "设置一次，之后在工序段上直接运行导出",
            &mut 收着,
            |ui| self.stages.export_setup_ui(ui, site),
        );
        self.set_folded(FOLD_EXPORT, 收着);
    }

    /// 屏头那颗「添加根…」：弹系统的选目录对话框，选中的交给 [`Self::add_root`]——**加根只有那一条路**
    /// （挂单 `Q694`，与开场那条向导选第一个根同一个办法）。名字不填，按目录自己的名字取；取消了什么都
    /// 不发生。对话框弹不出来时，根那一块底下那两个框照旧贴得进路径（`crate::pick` 的模块文档）。
    fn pick_root(&mut self, site: &Site) {
        let 起点 = self
            .roots
            .last()
            .and_then(|row| Path::new(&row.root.path).parent().map(Path::to_path_buf))
            .unwrap_or_default();
        if let Some(path) = crate::pick::directory("选一个目录作为根", &起点) {
            self.add_root(site, &path.to_string_lossy(), "");
        }
    }

    fn roots_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        let mut 要扫 = None;
        let mut 要移除 = None;
        let mut 要点头 = None;
        let mut 要收回 = false;
        // 画的时候不改自己：按下去的那几下先记下来，画完再动
        // （借用检查器要的，也让「按一下发生什么」读起来是一条直线）。
        let roots = self.roots.clone();
        let 忙的 = self.busy_roots();
        if roots.is_empty() {
            ui.weak(ROOTS_EMPTY);
        } else {
            // 表比这一栏宽时（路径长）横着滚，不把整屏撑宽（设计稿 `.scrollx`）。
            egui::ScrollArea::horizontal()
                .id_salt("根那一块")
                .show(ui, |ui| {
                    egui::Grid::new("根")
                        .num_columns(6)
                        .striped(true)
                        .show(ui, |ui| {
                            for header in ["根名", "路径", "变体", "容量", "上次扫描", ""]
                            {
                                ui.label(font::strong(header));
                            }
                            ui.end_row();
                            for row in &roots {
                                ui.label(&row.root.name);
                                // 路径、变体数与容量用等宽：一列扫下来位数对得齐。
                                if row.mounted {
                                    ui.label(font::mono(&row.root.path).weak());
                                } else {
                                    // **盘没挂上照样看得见上次结果**——那是这一行存在的一半理由。
                                    ui.colored_label(
                                        ui.visuals().warn_fg_color,
                                        font::mono(format!("{}（不在位）", row.root.path)),
                                    );
                                }
                                ui.label(font::mono(thousands(row.stats.variants)));
                                ui.label(font::mono(human_bytes(row.stats.bytes)));
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
                                            format!(
                                                "会去掉 {} 个变体",
                                                thousands(row.stats.variants)
                                            ),
                                        );
                                        if ui.button("确认移除").clicked() {
                                            要移除 = Some(row.root.name.clone());
                                        }
                                        // **「算了」得真的算了。** 一个只能前进不能后退的破坏性确认，
                                        // 比不加确认更坏。
                                        if ui.button("算了").clicked() {
                                            要收回 = true;
                                        }
                                    } else if ui
                                        .add_enabled(!忙, egui::Button::new("移除"))
                                        .clicked()
                                    {
                                        要点头 = Some(row.root.name.clone());
                                    }
                                });
                                ui.end_row();
                            }
                        });
                });
        }

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

        ui.add_space(step(1));
        // **对话框弹不出来时的退路**（`crate::pick` 的模块文档）：把目录贴进来。这一块在右边那一栏里，
        // 窄——两个框**跟着这一栏的宽度摆**，不写死宽度：写死的话按钮被挤出这一栏，画都画不出来。
        ui.label("添加目录");
        ui.add(
            egui::TextEdit::singleline(&mut self.new_path)
                .hint_text(ROOT_HINT)
                .desired_width(f32::INFINITY),
        );
        let mut 要加 = false;
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                要加 = ui.button("+ 添加目录").clicked();
                let 余下 = ui.available_width();
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_name)
                        .hint_text(ROOT_NAME_HINT)
                        .desired_width(余下),
                );
            });
        });
        if 要加 {
            let (path, name) = (self.new_path.clone(), self.new_name.clone());
            self.add_root(site, &path, &name);
        }
    }

    fn sources_ui(&mut self, ui: &mut egui::Ui, tasks: &mut Tasks) {
        let mut 要取 = None;
        // **还没扫描**：一个根都没扫过（或者一个根都没有）。那时有源没取回，就说下一步可以先取回它们。
        // 判据与工序段扫描那一行同一句（`LibraryRoot::fully_scanned`，ADR-0024）：一个根都没完整扫过。
        let 还没扫描 = !self.roots.iter().any(|row| row.root.fully_scanned());
        if 还没扫描 && self.sources.iter().any(|status| !status.ready()) {
            ui.weak(SOURCES_BEFORE_SCAN);
        }
        egui::ScrollArea::horizontal()
            .id_salt("数据源那一块")
            .show(ui, |ui| {
                egui::Grid::new("数据源")
                    .num_columns(5)
                    .striped(true)
                    .show(ui, |ui| {
                        for header in ["源", "记录", "上次取回", "覆盖", ""] {
                            ui.label(font::strong(header));
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
                                    ui.label(
                                        fetched_at.map_or_else(|| "——".to_string(), human_time),
                                    );
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
                             中文离线源那一条按得停——按下之后在当前这一块读完就收手，\
                             不等那 435 MB 下完。另外两条开跑之后还停不下来。",
                                )
                                .clicked()
                            {
                                要取 = Some(source);
                            }
                            ui.end_row();
                        }
                    });
            });
        if let Some(source) = 要取 {
            self.fetch(tasks, source);
        }
        if !还没扫描 {
            ui.add_space(step(0));
            ui.weak("扫完没认出来？先看这一屏——多半是某个源还没取回。");
        }
    }
}

/// 把「一个目录 + 一个可以不填的根名」折成核心库那一次加根调用，拦下时交出那句话。
///
/// **判断一条都不在这儿**：这儿只做两个框到那次调用之间的折算——目录空着、目录不是
/// 目录、名字不填时按目录自己的名字取；拦不拦得住全由
/// [`roots::add_root`] 说了算，而它那句话原样交出去（ADR-0005）。
///
/// **库屏与[开场那条向导](crate::claim)走的是同一条。** 加一个根要拦的三种情况
/// （根名重复、与已有的根套在一起、圈进**工作目录**）于是在两处是同一个答案、同一句
/// 措辞——两套判断只会分叉出两套行为。
///
/// # Errors
/// 目录空着、那不是一个目录、或者核心库把这个根拦下时，返回那句给人看的话。
pub fn add_root_from_fields(
    catalog: &Catalog,
    workspace: &Path,
    path: &str,
    name: &str,
) -> Result<LibraryRoot, String> {
    let path = Path::new(path.trim());
    if path.as_os_str().is_empty() {
        return Err("先填一个目录。".to_string());
    }
    if !path.is_dir() {
        return Err(format!(
            "{} 不是一个目录。加根只认目录——主库是一组目录。",
            romcat_core::path::display(path)
        ));
    }
    // 化成可比较的绝对形态之后再交给核心：套没套在一起是拿路径比出来的，
    // 而 `/var` 与 `/private/var` 这类链接不化开就比不出来。
    let normalized = romcat_core::path::normalize_existing(path);
    // **不填就按目录自己的名字取**：常见情况下少填一个框。取的是**化开之后**那个目录名
    // ——`.` 与尾斜杠自己没有名字。连末级名字都取不出来（根就是 `/`）时退成一个词。
    let name = if name.trim().is_empty() {
        normalized
            .file_name()
            .map(|it| it.to_string_lossy().into_owned())
            .unwrap_or_else(|| "主库".to_string())
    } else {
        name.trim().to_string()
    };
    roots::add_root(catalog, Some(workspace), &name, &normalized).map_err(|why| why.to_string())
}

/// 令牌里间距那几档的第 `at` 档（`space.steps`，从窄到宽）。**这一屏的间距只从这几档里取。**
fn step(at: usize) -> f32 {
    Tokens::builtin()
        .space
        .steps
        .get(at)
        .copied()
        .unwrap_or_default()
}

/// 面板与面板之间、两栏之间隔多宽：设计稿 `.libgrid` 与右边那一栏都是 18 点，取令牌间距里最近那一档（16）。
fn panel_gap() -> f32 {
    step(3)
}

/// 面板内边距（令牌 `space.panel-padding`：上下、左右）。工序段那几行也用它（`stages::Section::ui`）。
pub(crate) fn panel_padding() -> egui::Margin {
    let [pad_y, pad_x] = Tokens::builtin().space.panel_padding;
    egui::Margin::from(egui::vec2(pad_x, pad_y))
}

/// 设计稿里的一块**面板**（`.panel`）：`panel` 底、`line` 描边、`large` 圆角。颜色取主题里由令牌装上去
/// 的那几格（`look.rs` 那张槽位表），圆角取令牌。
fn panel_frame(ui: &egui::Ui) -> egui::Frame {
    let visuals = ui.visuals();
    egui::Frame::new()
        .fill(visuals.window_fill)
        .stroke(visuals.widgets.noninteractive.bg_stroke)
        .corner_radius(egui::CornerRadius::same(Tokens::builtin().radius.large))
}

/// 一块**收得起来的面板**：头上一条标题栏（设计稿 `.phead`：标题、一句说明、右边一个折叠标），
/// **点标题栏收起、再点摊开**；收着时只画标题栏，好让它还点得开。`folded` 是这一块眼下收着没有，
/// 点了当场翻过来——记不记得住由调用方交给版式偏好（[`Fold`]）。
fn foldable_panel(
    ui: &mut egui::Ui,
    title: &str,
    sub: &str,
    folded: &mut bool,
    body: impl FnOnce(&mut egui::Ui),
) {
    panel_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let spacing = ui.spacing().item_spacing;
        // 标题栏、分隔线、正文之间不留缝：分隔线就是缝。段里头照旧用原来的间距。
        ui.spacing_mut().item_spacing.y = 0.0;
        let header = egui::Frame::new()
            .inner_margin(panel_padding())
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = spacing;
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(font::strong(title));
                    ui.label(egui::RichText::new(sub).small().weak());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let 边长 = ui.spacing().icon_width;
                        let (_, 标) =
                            ui.allocate_exact_size(egui::vec2(边长, 边长), egui::Sense::hover());
                        let 摊开 = if *folded { 0.0 } else { 1.0 };
                        egui::collapsing_header::paint_default_icon(ui, 摊开, &标);
                    });
                });
            });
        let 标题栏 = header
            .response
            .interact(egui::Sense::click())
            .on_hover_text(if *folded {
                "点一下摊开"
            } else {
                "点一下收起"
            });
        if 标题栏.clicked() {
            *folded = !*folded;
        }
        if *folded {
            return;
        }
        ui.add(egui::Separator::default().spacing(0.0));
        egui::Frame::new()
            .inner_margin(panel_padding())
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = spacing;
                ui.set_width(ui.available_width());
                body(ui);
            });
    });
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
        line.push_str("（那一趟部分完成，数字是下界）");
    }
    line
}
