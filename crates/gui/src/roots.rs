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
use romcat_core::report::{human_duration, thousands};
use romcat_core::scan::{self, CheckpointOptions, Jobs, ScanOptions};
use romcat_core::site::Site;
use romcat_core::sources::{self, Source, SourceState, SourceStatus};
use romcat_core::stage::Stage;
use romcat_core::task::{Cutoff, Ending};

use crate::clock::Clock;
use crate::font;
use crate::layout::{FOLD_EXPORT, FOLD_HEALTH, FOLD_ROOTS, FOLD_SOURCES, Fold};
use crate::look::{self, Tone};
use crate::task::{Product, Tasks};
use crate::tokens::Tokens;

/// 数据源那一块标题栏右边那颗按钮上写的字（设计稿 `data-fetch-all`，挂单 `Q826`、`Q881` 已裁：照稿）。
pub const FETCH_ALL: &str = "全部下载";

/// 同一颗按钮在**每个源都下载过了**时写的字：按下去把每个源重新下载一遍（拿主意的人 2026-09-14 答）。
pub const REFETCH_ALL: &str = "全部更新";

/// 根那张表里扫过的根那颗按钮（设计稿「重新扫描」，与工序段扫描那一行做完时那颗同一个说法）。
const RESCAN: &str = "重新扫描";

/// 盘不在位的根，上次扫描那一格挂的那枚小标签（设计稿原话，挂单 `Q829` 已裁：照稿；词表里原来那个「连接」改名「组合方式」）。
const UNMOUNTED: &str = "未连接";

/// 盘不在位的根，上次扫描那一格小标签旁边那句（设计稿原话）：这一行的数是上次扫出来的。
const LAST_SCAN_SHOWN: &str = "显示上次扫描结果";

/// 根那一块一个根都没有时画的那一句，不画一张只有表头的表（票 `gui-looks-like-the-design/06`）。**只留后半句**
/// （拿主意的人 2026-09-14 答）：去哪儿加由工序段扫描那一行说（「还没有根，先点右上角「添加根…」选一个目录」），
/// 这一块不再重复一遍。
pub const ROOTS_EMPTY: &str = "还没有根——几块盘都能加进同一个库。";

/// 屏头「添加根…」那个选目录窗口什么都没交回来时屏上那一句（[`Screen::picked_root`]）。取消了与弹不出来分不开
/// （`crate::pick` 的模块文档），所以说到弹不出来时去哪儿办——加根是 `romcat scan` 的活。
pub const PICK_ROOT_NONE: &str = "没有选目录。选择窗口弹不出来时（例如远程会话），可以在命令行用 `romcat scan` \
     扫这个目录，把它加成这个库的一个根。";

/// 数据源那一块在**还没扫描**、又有源没取回时画的那一句：扫描与取回互不挡道，下一步可以两件一起办。
pub const SOURCES_BEFORE_SCAN: &str =
    "还没扫描。扫描的时候就可以先把这几个源下载下来——识别和刮削要用它们。";

/// 根那张表里一条路径**最多折几行**：两颗按钮并排时剩下的宽不够，按钮就竖着叠（`Screen::roots_ui`）。
/// 第十四版候选图审稿定的：稿上同样的宽度，路径折成两行。
const ROOT_PATH_ROWS: usize = 2;

/// 屏头那颗按钮上写的字：弹系统的选目录窗口，选中的加成一个根（`Screen::picked_root`）。
pub const ADD_ROOT: &str = "添加根…";

/// 库屏两栏里左边（工序）占多少：设计稿 `.libgrid` 是 `1.25fr : 1fr`。**设计稿不给断点**，
/// 窗口窄了两栏一起收窄，不叠成一栏。
const LEFT_SHARE: f32 = 1.25 / 2.25;

/// 贴一个根的目录那个框里的提示字（[开场那条向导](crate::claim)第二步那一框）。
///
/// **摆在这一屏**：加根那条路（[`add_root_from_fields`]）在这儿，向导与库屏走的是同一条——同一件事在相邻两处换个说法，
/// 人就会以为它们是两件事。
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
    /// 正在跑的那几趟活的任务号，用来禁掉重复按下。
    running: Vec<(u64, Job)>,
    /// 点了「移除」还没点头的那个根。
    removing: Option<String>,
    error: Option<String>,
    notice: Option<String>,
    /// 右边那一栏里眼下收着的那几块（[`Fold`]）。开窗时从版式偏好里交进来、每帧画完抄回去
    /// （`App::new` / `App::ui`）——这一屏自己不读写文件。
    folded: BTreeSet<&'static str>,
    /// 画时刻用的钟（[`Clock`]）：本地时间短格式；截图测试钉死此刻与偏移（[`Screen::set_clock`]）。
    clock: Clock,
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
            running: Vec::new(),
            removing: None,
            error: None,
            notice: None,
            folded: BTreeSet::new(),
            clock: Clock::default(),
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

    /// 根那几行**换成调用方交进来的**，这一下不碰盘、不读中立库。
    ///
    /// **截图门要它**（`tests/snapshot.rs`），与开场那一屏的 [`crate::opening::Screen::listed`] 同一个办法：
    /// 根那一行画着那个目录的路径、盘在不在位与上次扫描的时刻，从真库上读的话，临时目录在每台机器、
    /// 每一趟上都是另一串，时刻是扫的那一刻，像素跟着变。交进来的仍是 [`RootRow`]，画法与
    /// [`Self::reload`] 读出来的走同一条路；下一次重读（加根、扫完一趟……）照旧回到真库。
    pub fn list_roots(&mut self, roots: Vec<RootRow>) {
        self.roots = roots;
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
    /// 库屏屏头「添加根…」不看这个返回值（[`Self::picked_root`]）：接着画的那一帧本来就照着重读过的那张表画。
    pub fn add_root(&mut self, site: &Site, path: &str, name: &str) -> Option<String> {
        match add_root_from_fields(&site.catalog, &self.workspace, path, name) {
            Ok(root) => {
                self.error = None;
                self.notice = Some(format!(
                    "加上了根「{}」。按「扫描」把它收进这份中立库。",
                    root.name
                ));
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

    /// 换一个画时刻用的钟（[`Clock`]）：截图测试钉死此刻与偏移，截图里才没有当前时间。
    pub fn set_clock(&mut self, clock: Clock) {
        self.clock = clock;
    }

    /// 每个源是不是**都下载过了**。一个源都没列出来时不算。
    fn all_fetched(&self) -> bool {
        !self.sources.is_empty() && self.sources.iter().all(SourceStatus::ready)
    }

    /// 标题栏那颗按下去要排的那几个源：都下载过了（[`Self::all_fetched`]）就是每个源，否则是还没下载的那几个；
    /// 已经在台上的不重排。
    fn to_fetch_all(&self) -> impl Iterator<Item = Source> + '_ {
        let 每个 = self.all_fetched();
        Source::all()
            .into_iter()
            .zip(&self.sources)
            .filter(move |(source, status)| {
                (每个 || !status.ready())
                    && !self
                        .running
                        .iter()
                        .any(|(_, job)| *job == Job::Fetch(*source))
            })
            .map(|(source, _)| source)
    }

    /// **全部下载 / 全部更新**（数据源那一块标题栏右边那颗，[`FETCH_ALL`] / [`REFETCH_ALL`]）：有源没下载时把那几个
    /// 逐个排上任务台；每个源都下载过了，就把每个源重新下载一遍（`Self::to_fetch_all`）。
    ///
    /// **走的就是 [`Self::fetch`]**，不另写一条：任务台一次只跑一趟，排上去的依次跑；已经在台上的不重排。
    pub fn fetch_all(&mut self, tasks: &mut Tasks) {
        let 要取: Vec<Source> = self.to_fetch_all().collect();
        for source in 要取 {
            self.fetch(tasks, source);
        }
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

    /// **屏头右侧**那一段（屏头归窗口本体，`look::screen_header`；票 `gui-looks-like-the-design/32` 立好、挂单 `Q866` /
    /// `Q868` 交给本票接手）：照稿一颗小号的「添加根…」（设计稿 `.scrhead` 里那颗 `btn sm`）。按下去弹系统的选目录窗口，
    /// 选中了就加（[`Self::picked_root`]，挂单 `Q890`）。
    ///
    /// **原来顶栏上那句「N 个根 · N 个数据源还没下载」照稿不要了**：几个根写在左栏导航上，哪个数据源没下载在数据源那一块里
    /// 说。屏头右侧这一段一帧调两遍（先在看不见、按不动的地方量一遍宽），只有真摆的那一遍按得动，选目录窗口不会弹两次。
    pub fn header_actions(&mut self, ui: &mut egui::Ui, site: &Site) {
        let 按了 = look::small_buttons(ui, |ui| {
            ui.button(ADD_ROOT)
                .on_hover_text("选一个目录，加成这个库的一个根")
                .clicked()
        });
        if 按了 {
            let 起点 = self
                .roots
                .last()
                .and_then(|row| Path::new(&row.root.path).parent().map(Path::to_path_buf))
                .unwrap_or_default();
            let 选中 = crate::pick::directory("选一个目录作为根", &起点);
            self.picked_root(site, 选中);
        }
    }

    /// 画一帧的**屏体**（`look::screen_body`：屏头底下剩下的整块，竖着滚，内边距取令牌 `screen-body-padding`）。
    /// 屏名、副标题与「添加根…」在窗口本体画的屏头里（[`Self::header_actions`]），这一屏正文里不再画一遍（挂单 `Q866`）。
    pub fn ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        look::screen_body(ui, "库屏", |ui| self.body(ui, site, tasks));
    }

    fn body(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        let 间距 = panel_gap();
        // 这一屏自己的错与回执（加根被拦下、扫完一个根……）：有才画，画在两栏上头。
        let 有话说 = self.error.is_some() || self.notice.is_some();
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if let Some(notice) = &self.notice {
            ui.weak(notice);
        }
        if 有话说 {
            ui.add_space(间距);
        }

        // 两栏（设计稿 `.libgrid`）：左边工序段，右边根、数据源、导出设置三块。
        ui.horizontal_top(|ui| {
            let 左宽 = (ui.available_width() - 间距) * LEFT_SHARE;
            ui.vertical(|ui| {
                ui.set_width(左宽);
                // 左边那一整张卡（设计稿 `#stages-panel`）：卡里头的几块各自铺满卡宽、自己垫内边距（`stages::Section::ui`）。
                panel_frame(ui).show(ui, |ui| {
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
        // 两栏底下通栏的**库体检**那一块（设计稿 `data-panel="health"`，离两栏隔一个 `library-gap`）。
        ui.add_space(间距);
        self.health_ui(ui);
        // 工序段扫描那一行按下去只留记号：这一屏自己那条扫描的路接着排（`Self::take_scan`）。
        self.take_scan(site, tasks);
    }

    /// 这个库有没有一个根**完整扫过一趟**（[`LibraryRoot::fully_scanned`]，ADR-0024）：数据源那一块「还没扫描」那一句、
    /// 库体检那一块的空态都照它。
    fn scanned(&self) -> bool {
        self.roots.iter().any(|row| row.root.fully_scanned())
    }

    /// 两栏底下通栏的**库体检**那一块（`crate::health`）：标题栏照稿「库体检」、那句说明、「重新体检」、折叠标，收得起来。
    fn health_ui(&mut self, ui: &mut egui::Ui) {
        let 扫过 = self.scanned();
        let mut 收着 = self.folded(FOLD_HEALTH);
        let mut 标题栏的按钮 = |ui: &mut egui::Ui| {
            look::small_buttons(ui, |ui| ui.button(crate::health::RECHECK));
        };
        foldable_panel(
            ui,
            "库体检",
            crate::health::READ_ONLY,
            &mut 收着,
            Some(&mut 标题栏的按钮),
            |ui| crate::health::body_ui(ui, 扫过),
        );
        self.set_folded(FOLD_HEALTH, 收着);
    }

    /// 右边那一栏：根、数据源、导出设置三块，**各自收得起来**（[`Fold`]），次序照设计稿。
    fn side_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        let 间距 = panel_gap();
        let mut 收着 = self.folded(FOLD_ROOTS);
        // 标题照稿只写「根」（挂单 `Q888` 已裁）：几个根写在左栏导航上（票 `gui-looks-like-the-design/32`）。
        foldable_panel(
            ui,
            "根",
            "同一个主库可以包含多块盘或多个目录",
            &mut 收着,
            None,
            |ui| self.roots_ui(ui, site, tasks),
        );
        self.set_folded(FOLD_ROOTS, 收着);
        ui.add_space(间距);

        let mut 收着 = self.folded(FOLD_SOURCES);
        // 标题栏右边那颗（挂单 `Q826` 已裁：照稿）：有源没下载时写「全部下载」，都下载过了写「全部更新」、按下去把每个源
        // 重新下载一遍（拿主意的人 2026-09-14 答）。按下去只记一笔，画完再排——排活要的是这一屏自己（[`Self::fetch_all`]），
        // 而那颗按钮画在标题栏里。
        let mut 要全取 = false;
        let (字, 悬停) = if self.all_fetched() {
            (
                REFETCH_ALL,
                "每个源逐个重新下载一遍，排到任务台上，一次只跑一趟；联网下载",
            )
        } else {
            (
                FETCH_ALL,
                "未下载的那几个源逐个排到任务台上，一次只跑一趟；联网下载",
            )
        };
        let 能全取 = self.to_fetch_all().next().is_some();
        let mut 标题栏的按钮 = |ui: &mut egui::Ui| {
            要全取 = look::small_buttons(ui, |ui| {
                ui.add_enabled(能全取, egui::Button::new(字))
                    .on_hover_text(悬停)
                    .clicked()
            });
        };
        foldable_panel(
            ui,
            "数据源",
            "识别和刮削要用的本地数据",
            &mut 收着,
            Some(&mut 标题栏的按钮),
            |ui| self.sources_ui(ui, tasks),
        );
        self.set_folded(FOLD_SOURCES, 收着);
        if 要全取 {
            self.fetch_all(tasks);
        }
        ui.add_space(间距);

        let mut 收着 = self.folded(FOLD_EXPORT);
        foldable_panel(
            ui,
            "导出设置",
            "设置一次，之后在工序段上直接运行导出",
            &mut 收着,
            None,
            |ui| padded(ui, |ui| self.stages.export_setup_ui(ui, site)),
        );
        self.set_folded(FOLD_EXPORT, 收着);
    }

    /// 屏头「添加根…」弹的系统选目录窗口交回来的那一个（`crate::pick::directory`；挂单 `Q890` 已裁：照稿，不另开弹层）。
    ///
    /// - **选中了一个目录**：加成一个根，根名取目录自己的名字（[`Self::add_root`]）。被核心库拦下时（套在已有的根里、已经
    ///   加过……）那句原话摆在屏上，任务历史不多一条——加根不是任务（照票 `gui-looks-like-the-design/07` 被拒的写法）。
    /// - **交回 `None`**：取消了，或者窗口压根弹不出来（远程会话；`crate::pick` 的模块文档——两样分不开），屏上说一句去哪儿办
    ///   （[`PICK_ROOT_NONE`]）。
    ///
    /// **测试从这儿递路径进去**：系统窗口测试点不了，选中之后那一半照 `tests/pick.rs` 的做法验。
    pub fn picked_root(&mut self, site: &Site, picked: Option<PathBuf>) {
        match picked {
            Some(path) => {
                self.add_root(site, &path.to_string_lossy(), "");
            }
            None => {
                self.error = None;
                self.notice = Some(PICK_ROOT_NONE.to_string());
            }
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
        if !roots.is_empty() {
            // **照稿五列**（设计稿根那张表：根名称、路径、变体、上次扫描、按钮；挂单 `Q828` 已裁）。这一块在右边
            // 那一栏里、窄：不折行的那几列（根名称、变体、上次扫描那一刻、按钮）照它们最宽那一格画，**路径拿剩下的
            // 宽度、在格子里折行**（[`remaining_width`]），用时落上次扫描那一格第二行——整张表宽不过这一栏，
            // 「重新扫描」「移除」一定落在栏里（`tests/snapshot.rs` 那条断言钉着）。剩下的宽不够路径折在
            // [`ROOT_PATH_ROWS`] 行以内，两颗按钮就竖着叠、让出宽来（见下面的 `叠起`）。列与列、行与行之间的缝取令牌
            // `cell-padding`；表头说明字号、弱色；每行左边一条状态竖条，行与行之间一条分隔线（设计稿 `.tbl`）。
            let tokens = Tokens::builtin();
            let [行缝, 列缝] = tokens.space.cell_padding;
            let 弱色 = ui.visuals().weak_text_color();
            let 警示色 = ui.visuals().error_fg_color;
            let 表头 = |字: &str| {
                font::strong(字)
                    .size(tokens.font.size_caption_plus)
                    .color(弱色)
            };
            let 名宽 = widest(
                ui,
                std::iter::once(egui::WidgetText::from(表头("根名称"))).chain(
                    roots
                        .iter()
                        .map(|row| egui::WidgetText::from(row.root.name.as_str())),
                ),
            );
            let 变体宽 =
                widest(
                    ui,
                    std::iter::once(egui::WidgetText::from(表头("变体"))).chain(roots.iter().map(
                        |row| egui::WidgetText::from(font::mono(thousands(row.stats.variants))),
                    )),
                );
            let clock = self.clock;
            let 扫描宽 = widest(
                ui,
                std::iter::once(egui::WidgetText::from(表头("上次扫描")))
                    .chain(
                        roots
                            .iter()
                            .filter(|row| row.mounted)
                            .map(|row| egui::WidgetText::from(last_scan(row, clock).0)),
                    )
                    .chain(roots.iter().any(|row| !row.mounted).then(|| {
                        egui::WidgetText::from(egui::RichText::new(LAST_SCAN_SHOWN).weak())
                    })),
            );
            // 路径用等宽，字号照稿（设计稿 `.mono` 是正文的 0.92 倍，令牌 `size-small`）：一列扫下来位数对得齐。
            let 路径字 = |path: &str| font::mono(path).size(tokens.font.size_small).weak();
            // **表格通栏**（设计稿 `.tbl` 直接放在 `.panel` 里）：分隔线从面板左沿画到右沿，状态竖条贴着面板左沿；格子的
            // 内边距取令牌 `cell-padding`——表的四周垫一份，列与列、行与行之间垫两份（两边的格子各一份），分隔线画在正中。
            let 表 = ui.available_rect_before_wrap();
            let 线 = ui.visuals().widgets.noninteractive.bg_stroke;
            egui::Frame::new()
                .inner_margin(egui::Margin::from(egui::vec2(列缝, 行缝)))
                .show(ui, |ui| {
                    // 格子里一段挨一段照回主题的缝（面板正文那一层把竖向的缝收成了零，`foldable_panel`）。
                    let 缝 = ui.ctx().style_of(ui.ctx().theme()).spacing.item_spacing;
                    ui.spacing_mut().item_spacing = 缝;
                    // 根名那一列不超过令牌 `root-name-max`（挂单 `Q911` 已裁：截断加悬停）：目录名再长，路径那一列也留得出地方。
                    let 名宽 = 名宽.min(tokens.layout.root_name_max);
                    // **两颗按钮并排还是竖着叠**：稿上每行只有一颗按钮，按钮那一列窄，路径拿得到的宽也多；这里每个根
                    // 两颗都给（挂单 `Q884` 已裁），并排时右栏一窄，路径那一列只剩一个词宽——第十四版候选图上
                    // 「/Volumes/新加卷/Game」折成四行、「/」一个人占一行。于是先照并排量：有哪条路径在剩下的宽里折过
                    // [`ROOT_PATH_ROWS`] 行，两颗按钮就竖着叠、贴右（照稿 `td.r`），让出一颗按钮的宽给路径。
                    let 并排宽 = look::small_button_width(ui, RESCAN)
                        .max(look::small_button_width(ui, "扫描"))
                        + ui.spacing().item_spacing.x
                        + look::small_button_width(ui, "移除");
                    let 叠起宽 = [RESCAN, "扫描", "移除"]
                        .into_iter()
                        .map(|字| look::small_button_width(ui, 字))
                        .fold(0.0, f32::max);
                    let 并排时路径宽 =
                        remaining_width(ui, &[名宽, 变体宽, 扫描宽, 并排宽], 2.0 * 列缝);
                    let 叠起 = roots.iter().any(|row| {
                        egui::WidgetText::from(路径字(&row.root.path))
                            .into_galley(
                                ui,
                                Some(egui::TextWrapMode::Wrap),
                                并排时路径宽,
                                egui::FontSelection::Default,
                            )
                            .rows
                            .len()
                            > ROOT_PATH_ROWS
                    });
                    let 按钮宽 = if 叠起 { 叠起宽 } else { 并排宽 };
                    let 路径宽 = remaining_width(ui, &[名宽, 变体宽, 扫描宽, 按钮宽], 2.0 * 列缝);
                    // **列宽下限收成零**：egui 的表格默认每列至少一个可点区域那么宽（40 点），「变体」那一列于是被撑到 40，
                    // 比算好的宽出十几点、整栏跟着宽出去（第六版候选图扫过两个根那两张）。列宽只照量出来的那一格。
                    egui::Grid::new("根")
                        .num_columns(5)
                        .min_col_width(0.0)
                        .spacing(egui::vec2(2.0 * 列缝, 2.0 * 行缝))
                        .show(ui, |ui| {
                            let 表头底 = ["根名称", "路径", "变体", "上次扫描", ""]
                                .into_iter()
                                .map(|header| ui.label(表头(header)).rect.bottom())
                                .fold(f32::NEG_INFINITY, f32::max);
                            ui.end_row();
                            ui.painter().hline(表.x_range(), 表头底 + 行缝, 线);
                            for (at, row) in roots.iter().enumerate() {
                                // 根名**不折行，超过那一列的宽就截断**，末尾「…」，悬停看全名（egui 截断的标签自己挂上全名的悬停；
                                // 挂单 `Q911` 已裁）。
                                let 名格 = ui
                                    .vertical(|ui| {
                                        ui.set_min_width(名宽);
                                        ui.set_max_width(名宽);
                                        ui.add(egui::Label::new(row.root.name.as_str()).truncate());
                                    })
                                    .response
                                    .rect;
                                // 路径长了在格子里折行（字见上面的 `路径字`）。
                                let 路径格 = ui
                                    .vertical(|ui| {
                                        // 这一格**占满**算给它的宽：整张表于是铺满这一栏，按钮那一列贴着表的右内边距（照稿，
                                        // 与数据源那张表的按钮右沿对齐）。
                                        ui.set_min_width(路径宽);
                                        ui.set_max_width(路径宽);
                                        ui.add(egui::Label::new(路径字(&row.root.path)).wrap());
                                    })
                                    .response
                                    .rect;
                                // 变体数**不折行**。容量不在这一格（挂单 `Q828` 已裁：几个根一共多大在工序段扫描那一行的小字里）。
                                let 变体格 = single_line_cell(
                                    ui,
                                    变体宽,
                                    [egui::WidgetText::from(font::mono(thousands(
                                        row.stats.variants,
                                    )))],
                                );
                                let 扫描格 = ui
                                    .vertical(|ui| {
                                        // 这一格照最宽那一段量（那一刻、「显示上次扫描结果」都不折行）。
                                        ui.set_min_width(扫描宽);
                                        ui.set_max_width(扫描宽);
                                        if row.mounted {
                                            let (那一刻, 其余) = last_scan(row, clock);
                                            ui.add(egui::Label::new(那一刻).extend());
                                            // 第二行小字（用了多久、部分完成）在格子里折。
                                            if let Some(其余) = 其余 {
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(其余).small().weak(),
                                                    )
                                                    .wrap(),
                                                );
                                            }
                                        } else {
                                            // **盘没挂上照样看得见上次结果**——那是这一行存在的一半理由。照稿（设计稿根那张表
                                            // 不在位那一行 `.chip.t-none.plain`）：一枚不带圆点的中性小标签，底下一句弱色的
                                            // 「显示上次扫描结果」，**那一句本身不折行**；不画用时（稿上那一行没有）。
                                            look::plain_chip(ui, Tone::Neutral, UNMOUNTED);
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(LAST_SCAN_SHOWN).weak(),
                                                )
                                                .extend(),
                                            );
                                        }
                                    })
                                    .response
                                    .rect;
                                // **按钮那一格**：并排时折行——确认移除那一态多一句话、两颗按钮，放不下就换行，不挤出这一栏；
                                // 路径那一列不够宽时竖着叠、贴右（见上面的 `叠起`）。按钮是小号（设计稿 `.btn.sm`）；「移除」照稿用
                                // 警示样式（`.btn.warn`：`lo` 的字、透明底）。
                                let mut 摆按钮 = |ui: &mut egui::Ui| {
                                    look::small_buttons(ui, |ui| {
                                        let 忙 = 忙的.contains(&row.root.name);
                                        let 标签 = if row.root.scan.is_some() {
                                            RESCAN
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
                                        if self.removing.as_deref() == Some(row.root.name.as_str())
                                        {
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
                                            .add_enabled(
                                                !忙,
                                                egui::Button::new(
                                                    egui::RichText::new("移除").color(警示色),
                                                )
                                                .fill(egui::Color32::TRANSPARENT),
                                            )
                                            .clicked()
                                        {
                                            要点头 = Some(row.root.name.clone());
                                        }
                                    });
                                };
                                let 按钮格 = if 叠起 {
                                    ui.vertical(|ui| {
                                        ui.set_min_width(按钮宽);
                                        ui.set_max_width(按钮宽);
                                        ui.with_layout(
                                            egui::Layout::top_down(egui::Align::Max),
                                            摆按钮,
                                        );
                                    })
                                    .response
                                    .rect
                                } else {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.set_max_width(按钮宽);
                                        摆按钮(ui);
                                    })
                                    .response
                                    .rect
                                };
                                // 这一行左边那条状态竖条（设计稿 `.tbl td.st`）：完整扫过、盘又在位是 `hi`，别的是 `none`。
                                let 语气 = if row.mounted && row.root.fully_scanned() {
                                    Tone::Good
                                } else {
                                    Tone::Neutral
                                };
                                let 下 = paint_row_stripe(
                                    ui,
                                    表.left(),
                                    &[名格, 路径格, 变体格, 扫描格, 按钮格],
                                    (行缝, 行缝),
                                    look::tone_colors(语气, ui.visuals()).0,
                                );
                                ui.end_row();
                                if at + 1 < roots.len() {
                                    ui.painter().hline(表.x_range(), 下 + 行缝, 线);
                                }
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

        // 一个根都没有时这一块说清去哪儿加（照稿这一块里不摆贴路径的表单，加根在屏头「添加根…」，`Self::picked_root`）。
        // 表格通栏，这一句照面板内边距垫（[`padded`]）。
        if roots.is_empty() {
            padded(ui, |ui| look::weak_paragraph(ui, ROOTS_EMPTY));
        }
    }

    fn sources_ui(&mut self, ui: &mut egui::Ui, tasks: &mut Tasks) {
        let mut 要取 = None;
        // **还没扫描**：一个根都没扫过（或者一个根都没有）。那时有源没取回，就说下一步可以先取回它们。
        // 判据与工序段扫描那一行同一句（`LibraryRoot::fully_scanned`，ADR-0024）：一个根都没完整扫过。
        let 还没扫描 = !self.scanned();
        if 还没扫描 && self.sources.iter().any(|status| !status.ready()) {
            // 段末不留孤字（第十四版候选图上这一句最后折出一个孤零零的「们。」）。
            padded(ui, |ui| look::weak_paragraph(ui, SOURCES_BEFORE_SCAN));
        }
        // **照稿四列**（设计稿数据源那张表：数据源、记录数、更新时间 / 说明、按钮）：不折行的那几列（名字、记录数、
        // 按钮）照它们最宽那一格画，**说明拿剩下的宽度、在格子里折行**（[`remaining_width`]）——整张表宽不过这一栏，
        // 「取回」「重取」一定落在栏里。缝、表头、竖条、分隔线与根那张表同一套（设计稿 `.tbl`）：竖条取回了是 `hi`，
        // 还没取回是 `mid`，读不动是 `lo`。
        let tokens = Tokens::builtin();
        let clock = self.clock;
        let [行缝, 列缝] = tokens.space.cell_padding;
        let 弱色 = ui.visuals().weak_text_color();
        let 警告色 = ui.visuals().warn_fg_color;
        let 出错色 = ui.visuals().error_fg_color;
        let 表头 = |字: &str| {
            font::strong(字)
                .size(tokens.font.size_caption_plus)
                .color(弱色)
        };
        let 名宽 = widest(
            ui,
            std::iter::once(egui::WidgetText::from(表头("数据源"))).chain(
                self.sources
                    .iter()
                    .map(|status| egui::WidgetText::from(status.name)),
            ),
        );
        let 记录宽 =
            widest(
                ui,
                [
                    egui::WidgetText::from(表头("记录数")),
                    "未下载".into(),
                    "读不动".into(),
                ]
                .into_iter()
                .chain(self.sources.iter().filter_map(
                    |status| match &status.state {
                        SourceState::Ready { records, .. } => {
                            Some(egui::WidgetText::from(thousands(*records)))
                        }
                        SourceState::Missing | SourceState::Broken { .. } => None,
                    },
                )),
            );
        let 按钮宽 = look::small_button_width(ui, "下载").max(look::small_button_width(ui, "更新"));
        // 表格通栏，缝与分隔线同根那张表（`roots_ui`）。
        let 表 = ui.available_rect_before_wrap();
        let 线 = ui.visuals().widgets.noninteractive.bg_stroke;
        let 行数 = self.sources.len();
        egui::Frame::new()
            .inner_margin(egui::Margin::from(egui::vec2(列缝, 行缝)))
            .show(ui, |ui| {
                // 格子里照回主题的缝；列宽下限收成零——理由同根那张表（`roots_ui`）。
                let 缝 = ui.ctx().style_of(ui.ctx().theme()).spacing.item_spacing;
                ui.spacing_mut().item_spacing = 缝;
                let 说明宽 = remaining_width(ui, &[名宽, 记录宽, 按钮宽], 2.0 * 列缝);
                egui::Grid::new("数据源")
                    .num_columns(4)
                    .min_col_width(0.0)
                    .spacing(egui::vec2(2.0 * 列缝, 2.0 * 行缝))
                    .show(ui, |ui| {
                        let 表头底 = ["数据源", "记录数", "更新时间 / 说明", ""]
                            .into_iter()
                            .map(|header| ui.label(表头(header)).rect.bottom())
                            .fold(f32::NEG_INFINITY, f32::max);
                        ui.end_row();
                        ui.painter().hline(表.x_range(), 表头底 + 行缝, 线);
                        for (at, (source, status)) in
                            Source::all().into_iter().zip(&self.sources).enumerate()
                        {
                            let 名格 =
                                single_line_cell(ui, 名宽, [egui::WidgetText::from(status.name)]);
                            let (记录格, 说明格, 竖条色) = match &status.state {
                                SourceState::Ready {
                                    records,
                                    fetched_at,
                                } => (
                                    single_line_cell(
                                        ui,
                                        记录宽,
                                        [egui::WidgetText::from(thousands(*records))],
                                    ),
                                    ui.vertical(|ui| {
                                        // 占满算给这一格的宽：按钮那一列才贴着表的右内边距（照稿，第八版候选图上空出一大截）。
                                        ui.set_min_width(说明宽);
                                        ui.set_max_width(说明宽);
                                        ui.label(fetched_at.map_or_else(
                                            || "——".to_string(),
                                            |at| clock.short(at),
                                        ));
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&status.coverage)
                                                    .small()
                                                    .weak(),
                                            )
                                            .wrap(),
                                        );
                                    })
                                    .response
                                    .rect,
                                    look::tone_colors(Tone::Good, ui.visuals()).0,
                                ),
                                // **还没取回的要被明确标出来**：那正是「扫完了怎么没认出来」
                                // 的答案。
                                SourceState::Missing => (
                                    single_line_cell(
                                        ui,
                                        记录宽,
                                        [egui::WidgetText::from(
                                            egui::RichText::new("未下载").color(警告色),
                                        )],
                                    ),
                                    ui.vertical(|ui| {
                                        // 占满算给这一格的宽：按钮那一列才贴着表的右内边距（照稿，第八版候选图上空出一大截）。
                                        ui.set_min_width(说明宽);
                                        ui.set_max_width(说明宽);
                                        // 说明那一句照稿用弱色（设计稿 `td.dim`）：「未下载」那一格与竖条已经是警示色。
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(status.cost).color(弱色),
                                            )
                                            .wrap(),
                                        );
                                    })
                                    .response
                                    .rect,
                                    警告色,
                                ),
                                // **读不动不等于空**（ADR-0021）：那要人去看一眼，不是按一下取回。
                                SourceState::Broken { why } => (
                                    single_line_cell(
                                        ui,
                                        记录宽,
                                        [egui::WidgetText::from(
                                            egui::RichText::new("读不动").color(出错色),
                                        )],
                                    ),
                                    ui.vertical(|ui| {
                                        // 占满算给这一格的宽：按钮那一列才贴着表的右内边距（照稿，第八版候选图上空出一大截）。
                                        ui.set_min_width(说明宽);
                                        ui.set_max_width(说明宽);
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(why).color(出错色),
                                            )
                                            .wrap(),
                                        );
                                    })
                                    .response
                                    .rect,
                                    出错色,
                                ),
                            };
                            let 忙 = self
                                .running
                                .iter()
                                .any(|(_, job)| *job == Job::Fetch(source));
                            let 按钮 = look::small_buttons(ui, |ui| {
                                // 按钮上的字照稿（挂单 `Q881` 已裁）：没下载的「下载」；下载过的是弱化的「更新」（设计稿 `.btn.ghost`）。
                                let 按钮 = if status.ready() {
                                    let 弱化字色 =
                                        ui.visuals().widgets.noninteractive.fg_stroke.color;
                                    egui::Button::new(egui::RichText::new("更新").color(弱化字色))
                                        .frame_when_inactive(false)
                                } else {
                                    egui::Button::new("下载")
                                };
                                ui.add_enabled(!忙, 按钮).on_hover_text(
                                    "联网下载一趟，排到任务台上跑。\
                             中文离线源那一条按得停——按下之后在当前这一块读完就收手，\
                             不等那 435 MB 下完。另外两条开始之后还停不下来。",
                                )
                            });
                            if 按钮.clicked() {
                                要取 = Some(source);
                            }
                            // 末行贴着面板底（底下没有「扫完没认出来」那句）时竖条不伸进底边那份内边距：面板底角是圆的，伸下去会戳出圆角。
                            let 下伸 = if at + 1 == 行数 && 还没扫描 {
                                0.0
                            } else {
                                行缝
                            };
                            let 下 = paint_row_stripe(
                                ui,
                                表.left(),
                                &[名格, 记录格, 说明格, 按钮.rect],
                                (行缝, 下伸),
                                竖条色,
                            );
                            ui.end_row();
                            if at + 1 < 行数 {
                                ui.painter().hline(表.x_range(), 下 + 行缝, 线);
                            }
                        }
                    });
            });
        if let Some(source) = 要取 {
            self.fetch(tasks, source);
        }
        if !还没扫描 {
            padded(ui, |ui| {
                look::weak_paragraph(ui, "扫完没认出来？先看这一屏——多半是某个源还没下载。")
            });
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

/// 面板与面板之间、两栏之间隔多宽（设计稿 `.libgrid` 与右边那一栏的 `gap:18px`，令牌 `library-gap`）。
fn panel_gap() -> f32 {
    Tokens::builtin().space.library_gap
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

/// 一块**收得起来的面板**（设计稿 `.panel` 与 `.phead`）：头上一条标题栏——标题（令牌 `size-panel-title`）、一句说明；
/// `actions` 给了就再摆一排按钮、把折叠标推到最右（设计稿数据源那一块），没给时折叠标紧跟说明（根、导出设置那两块）。
/// 标题栏里的间距取令牌 `panel-head-gap`。**点标题、说明或折叠标收起、再点摊开**，标题栏里的按钮照常按；收着时只画
/// 标题栏，好让它还点得开。`folded` 是这一块眼下收着没有，点了当场翻过来——记不记得住由调用方交给版式偏好（[`Fold`]）。
fn foldable_panel(
    ui: &mut egui::Ui,
    title: &str,
    sub: &str,
    folded: &mut bool,
    actions: Option<&mut dyn FnMut(&mut egui::Ui)>,
    body: impl FnOnce(&mut egui::Ui),
) {
    let tokens = Tokens::builtin();
    panel_frame(ui).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let spacing = ui.spacing().item_spacing;
        // 标题栏、分隔线、正文之间不留缝：分隔线就是缝。段里头照旧用原来的间距。
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut 点了 = false;
        egui::Frame::new()
            .inner_margin(panel_padding())
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(tokens.space.panel_head_gap, spacing.y);
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let 悬停 = if *folded {
                        "点一下摊开"
                    } else {
                        "点一下收起"
                    };
                    let 标题 = ui
                        .add(
                            egui::Label::new(
                                font::strong(title).size(tokens.font.size_panel_title),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .on_hover_text(悬停);
                    let 说明 = ui
                        .add(
                            egui::Label::new(egui::RichText::new(sub).small().weak())
                                .sense(egui::Sense::click()),
                        )
                        .on_hover_text(悬停);
                    let 标 = match actions {
                        None => fold_icon(ui, *folded),
                        Some(actions) => {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let 标 = fold_icon(ui, *folded);
                                actions(ui);
                                标
                            })
                            .inner
                        }
                    };
                    点了 = 标题.clicked() || 说明.clicked() || 标.on_hover_text(悬停).clicked();
                });
            });
        if 点了 {
            *folded = !*folded;
        }
        if *folded {
            return;
        }
        look::divider(ui);
        // **正文不垫内边距**：表格照稿通栏（设计稿 `.tbl` 直接放在 `.panel` 里），别的内容自己垫一份（[`padded`]）。
        // **段与段之间不另留缝**：表格四周自带格子的内边距，别的段垫着面板内边距，缝就是那两份——再加一份主题的缝，
        // 「还没扫描……」那一句与表头之间就空出一大截（第六版候选图空库那两张）。
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(spacing.x, 0.0);
            ui.set_width(ui.available_width());
            body(ui);
        });
    });
}

/// 面板正文里**不是表格**的那几段：垫一份面板内边距（[`panel_padding`]）。表格通栏，不走它。段里头照回主题的缝。
fn padded<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .inner_margin(panel_padding())
        .show(ui, |ui| {
            let 缝 = ui.ctx().style_of(ui.ctx().theme()).spacing.item_spacing;
            ui.spacing_mut().item_spacing = 缝;
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// 面板标题栏里那枚**折叠标**（设计稿 `.iconbtn` 里那个 `▾`，收着时转成 `▸`）：可点的一格边长取令牌 `icon-button`，
/// 三角宽取令牌 `fold-mark`、高取宽的一半（直角等腰，尖朝下；收着时转九十度，尖朝右）。平常弱色（`.iconbtn` 的
/// `ink-3`），悬停时垫凹陷底、换正文色（`.iconbtn:hover`）。打包的字形子集里没有 `▾`，所以是画的。
fn fold_icon(ui: &mut egui::Ui, folded: bool) -> egui::Response {
    let tokens = Tokens::builtin();
    let 边长 = tokens.layout.icon_button;
    let (格, response) = ui.allocate_exact_size(egui::vec2(边长, 边长), egui::Sense::click());
    let visuals = ui.visuals();
    let 色 = if response.hovered() {
        ui.painter()
            .rect_filled(格, tokens.radius.small, visuals.extreme_bg_color);
        visuals.text_color()
    } else {
        visuals.weak_text_color()
    };
    let (中, 半) = (格.center(), tokens.layout.fold_mark / 2.0);
    let 三点 = if folded {
        vec![
            中 + egui::vec2(-半 / 2.0, -半),
            中 + egui::vec2(-半 / 2.0, 半),
            中 + egui::vec2(半 / 2.0, 0.0),
        ]
    } else {
        vec![
            中 + egui::vec2(-半, -半 / 2.0),
            中 + egui::vec2(半, -半 / 2.0),
            中 + egui::vec2(0.0, 半 / 2.0),
        ]
    };
    ui.painter()
        .add(egui::Shape::convex_polygon(三点, 色, egui::Stroke::NONE));
    response
}

/// 表里这一行左边那条**状态竖条**（设计稿 `.tbl td.st` 的 `inset 3px`，宽取令牌 `row-stripe`）：贴着 `左`（面板左沿）画，
/// 从这一行最高那一格的顶再往上伸 `伸.0`、画到最低那一格的底再往下伸 `伸.1`——格子的内边距里也还是这一行。交回这几格的底，
/// 好在底下画分隔线。
fn paint_row_stripe(
    ui: &egui::Ui,
    左: f32,
    这一行的格: &[egui::Rect],
    伸: (f32, f32),
    色: egui::Color32,
) -> f32 {
    let 上 = 这一行的格
        .iter()
        .map(|格| 格.top())
        .fold(f32::INFINITY, f32::min);
    let 下 = 这一行的格
        .iter()
        .map(|格| 格.bottom())
        .fold(f32::NEG_INFINITY, f32::max);
    if !这一行的格.is_empty() {
        ui.painter().rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(左, 上 - 伸.0),
                egui::pos2(左 + Tokens::builtin().layout.row_stripe, 下 + 伸.1),
            ),
            0.0,
            色,
        );
    }
    下
}

/// 一个根上次扫描那两句：头一句是**那一刻**，第二句（小字，设计稿上次扫描那一格的第二行）是用了多久、
/// 那一趟是不是部分完成。**没扫过与扫过是两件事**，说清楚——没扫过就只有头一句。
fn last_scan(row: &RootRow, clock: Clock) -> (String, Option<String>) {
    let Some(scan) = &row.root.scan else {
        return ("还没扫过".to_string(), None);
    };
    let mut 其余 = format!("用时 {}", human_duration(scan.elapsed_ms));
    if scan.interrupted {
        其余.push_str("（那一趟部分完成，数字是下界）");
    }
    (clock.short(scan.at), Some(其余))
}

/// 一张表里**折行的那一列**能拿多宽：这一栏眼下的宽度，扣掉不折行那几列各自的宽与列之间的缝。
///
/// 不折行的那几列（名字、数、按钮）照它们最宽那一格画（[`widest`]、[`look::small_button_width`]），剩下的全给折行那一列：
/// 整张表宽不过这一栏，按钮不被挤出去；折行那一列也不会像等分那样被压得比别的列还窄、把整块撑高
/// （第二段第二版候选图上导出设置那一块就这样被挤出了窗口）。不写一个像素：栏宽跟着窗口走，缝与按钮内边距跟着
/// 主题走，字宽现量。
fn remaining_width(ui: &egui::Ui, 不折行那几列: &[f32], 列缝: f32) -> f32 {
    let 缝 = 列缝 * 不折行那几列.len() as f32;
    (ui.available_width() - 不折行那几列.iter().sum::<f32>() - 缝).max(0.0)
}

/// 表里**不折行**的那一格：至少有量出来的那个宽（[`widest`]），里头每一段字都画成一行，一段一行竖着摆。
///
/// 表格沿用上一帧量出来的列宽，格子里的字默认又会折行——不钉住下限的话，一列会卡在表头那两个字的宽度上，
/// 容量「14.00 KiB」就被折成三行（第二段第三版候选图；`tests/snapshot.rs` 的 `画成一行` 钉着）。
fn single_line_cell(
    ui: &mut egui::Ui,
    宽: f32,
    那几段: impl IntoIterator<Item = egui::WidgetText>,
) -> egui::Rect {
    ui.vertical(|ui| {
        ui.set_min_width(宽);
        for 一段 in 那几段 {
            ui.add(egui::Label::new(一段).extend());
        }
    })
    .response
    .rect
}

/// 这几段字**不折行**画出来，最宽那一段多宽（点）。字体、字号照各段自己带的样式，没带的按正文。
fn widest(ui: &egui::Ui, 那几段: impl IntoIterator<Item = egui::WidgetText>) -> f32 {
    那几段
        .into_iter()
        .map(|一段| {
            一段
                .into_galley(
                    ui,
                    Some(egui::TextWrapMode::Extend),
                    f32::INFINITY,
                    egui::TextStyle::Body,
                )
                .size()
                .x
        })
        .fold(0.0, f32::max)
}
