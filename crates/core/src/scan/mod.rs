//! 只读遍历主库，把每条记录写进**中立库**，再由中立库出**库体检报告**。
//!
//! 四条约束决定了这里的形状：
//!
//! - **只读**。一切磁盘接触走 [`LibraryFs`]，它没有写的办法（ADR-0004）。断点与中立库
//!   写在本机工作目录，断点落在主库内时直接拒绝开工。
//! - **增量**。中立库里记着上次每个文件的 `(路径, 大小, 修改时间)`。三元组没变的文件
//!   跳过——不重新归类、不重新读头部，上次的结论原样留着。真正省下的活会随着票 07
//!   的哈希越来越重，那时不重算 7.84 TiB 才是这条接缝的价值所在。
//! - **可中断可续跑**。协调线程持有队列，断点里的 `pending` 同时包含队列里的和正在扫的
//!   目录，因此中断只会让少量目录被重扫，绝不会算重——计数是从中立库里**数**出来的，
//!   不是攒在内存里的。
//! - **报告不必扫盘**。报告一律由 [`Catalog::aggregate`] 从中立库折出来，扫描刚跑完
//!   也一样。于是「扫完出的报告」与「盘不在位时出的报告」不可能是两套数字。
//!
//! 平台由目录给出（ADR-0011）：键去掉**根名**之后第一级目录名就是平台目录。认不出
//! 平台的照常计入报告——平台未知不构成跳过的理由。
//!
//! **一趟扫一个根。** 主库是一组根（`CONTEXT.md`），几个根扫进同一份中立库；一趟扫描
//! 只走其中一个，键上带着它的名字，收尾时也只收它那一支（[`Catalog::sweep`]）。
//! **遍历那条记录、它的批注、以及断点的身份也一样按根分**：扫一遍乙盘不许动甲盘那一支
//! 的任何一样东西——不然甲盘的断点会被悄悄作废，甲盘那条「这个目录列不开」也会从
//! 报告里消失（[`Catalog::begin_scan`]、本模块的 `load_start_state`）。

pub mod aggregate;
pub mod checkpoint;
pub mod names;
pub mod probe;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use std::{fs, thread};

use crate::catalog::roots::{self, AddRootError, RootScan};
use crate::catalog::{Baseline, Catalog, CatalogError, EntryRecord, ScanDelta, Traversal, Verdict};
use crate::classify;
use crate::container::{self, ContainerKind, Penetration};
use crate::fs::{DirEntry, EntryKind, EntryMeta, LibraryFs};
use crate::header::{self, ProbeClass};
use crate::path;
use crate::platform::Manifest;
use crate::report::{HealthReport, ReportMeta};
use crate::shape;
use crate::task::Handle;

use aggregate::{Aggregate, Limits, SampleResult};
use checkpoint::{Checkpoint, CheckpointError};

/// 认「还是同一个主库」需要的顶层条目重合比例。
///
/// 半数是个务实的线：主库的顶层本来就会增删几个目录，卡太严会把正常的换挂载点也拦下来；
/// 而两个真正不同的主库要凑够半数同名的顶层条目，得是刻意造出来的巧合。
const SAME_LIBRARY_OVERLAP: f64 = 0.5;

/// 比对顶层条目时最多取几条。真库的顶层是 73 条，取 512 条绰绰有余。
const TOP_LEVEL_SAMPLE: usize = 512;

/// 一趟扫描分几步报进度：认根、遍历、收尾、成型。
///
/// **中断的那一趟走不到收尾与成型**（半个库上收出来的删除与成出来的变体是错的），
/// 于是进度条停在第二步——那正是实情。
const SCAN_STEPS: u32 = 4;

/// 攒够多少条记录写一次中立库。
///
/// 一条一条写会让每条都开一次事务；整趟扫完再写则中断时全丢。几千条一批是两者之间。
const WRITE_BATCH: usize = 4_096;

/// 扫描过程中的致命错误。读不到某个目录这类问题不在此列——那些计入报告，不中断扫描。
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    /// 扫描根不可用。
    #[error("扫描根不可用：{path}（{source}）")]
    Root {
        /// 出问题的路径。
        path: String,
        /// 底层错误。
        source: std::io::Error,
    },
    /// 断点或中立库落在主库内。
    #[error("{what} {path} 落在主库内。主库只读，它必须写在本机的工作目录里")]
    WritesInsideLibrary {
        /// 是断点还是中立库。
        what: &'static str,
        /// 出问题的路径。
        path: String,
    },
    /// 同一个**根**底下换了另一块盘。
    ///
    /// 根跟名字走而不跟路径走（挂账 D16），于是换挂载点还找得回同一支记录——这正是要的。
    /// 代价是「名字一样、盘不一样」变得可能，而那会让两块盘的记录挤进同一串前缀。
    #[error(
        "根「{name}」记的是 {recorded}，这次指的是 {current}——\
         顶层条目只有 {common}/{recorded_count} 条对得上，两边多半不是同一块盘。\
         中立库的键是「根名 + 相对那个根的路径」（ADR-0020），\
         两块盘挤进同一个根名会直接撞车。\
         换个根名把它当成新的一个根加进来，或者先移除「{name}」再重扫"
    )]
    DifferentLibrary {
        /// 这个根叫什么。
        name: String,
        /// 这个根上次记着在哪。
        recorded: String,
        /// 这次指的是哪。
        current: String,
        /// 顶层条目对得上几条。
        common: usize,
        /// 库里记着的顶层条目共几条。
        recorded_count: usize,
    },
    /// 这个**根**不在位：目录还在，库里记着的顶层条目却一条都不在。
    ///
    /// **挂载点目录永远都在**——Linux 的 `/mnt/x` 是先建出来的，macOS 卸盘之后
    /// `/Volumes/x` 也偶尔残留一个空目录。于是路径一个字没变、`is_dir()` 照样为真，
    /// 而遍历会顺利跑完、收尾把整个根当成「这次没见到」抹掉。看不见不等于不存在
    /// （ADR-0021），所以这一趟根本不该开工。
    ///
    /// 它与 [`DifferentLibrary`](Self::DifferentLibrary) **不是同一档**：那边是用户
    /// 主动把根指到了别处、指错了盘，出路是换个根名；这边用户什么都没改，是那块盘
    /// 自己不在，出路是插上盘。
    #[error(
        "根「{name}」记在 {path}，可库里记着的 {recorded_count} 条顶层条目\
         一条都不在（这一层眼下只有 {present} 条）。\
         挂载点目录一直都在，盘一拔它就剩个空壳——那块盘多半没挂上。插上那块盘再扫。\
         上次扫出来的东西照样看得见：它住在中立库里，不跟着盘走。\
         真是自己把这个根清空了的话，先把这个根移除再加回来"
    )]
    RootNotInPlace {
        /// 这个根叫什么。
        name: String,
        /// 这一趟指的是哪个目录。
        path: String,
        /// 这一层眼下有几条。
        present: usize,
        /// 库里记着的顶层条目共几条。
        recorded_count: usize,
    },
    /// 这个根加不进来。
    #[error(transparent)]
    AddRoot(#[from] AddRootError),
    /// 断点读写失败。
    #[error(transparent)]
    Checkpoint(#[from] CheckpointError),
    /// 中立库读写失败。
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

/// 中断信号。命令行把 Ctrl-C 接到它上面，界面把停止按钮接到它上面。
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// 新建一个未触发的中断信号。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 请求中断。
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// 是否已请求中断。
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// 断点配置。
#[derive(Debug, Clone)]
pub struct CheckpointOptions {
    /// 断点文件路径。必须在主库之外。
    pub path: PathBuf,
    /// 两次存盘的最小间隔。
    pub interval: Duration,
    /// 是否尝试从已有断点续跑。
    pub resume: bool,
}

/// 并发档：手动点名，还是开扫前按介质探测。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Jobs {
    /// 开扫前探一探目录读取的延迟，据此定（[`probe`]）。默认走这条。
    Adaptive,
    /// 用户用 `-j` 点名的并发数。点了就照办，探测不再插手。
    Fixed(usize),
}

/// 扫描参数。
#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// 这一趟扫哪个目录。
    pub root: PathBuf,
    /// 给这个**根**起的名字，也是它下面所有键的第一段。
    ///
    /// `None` 时按目录自己的名字取。这个根还没在中立库里时会被**加进去**（校验走
    /// [`roots::add_root`]）；已经在了就按名字对上，然后核一下还是不是同一块盘。
    pub root_name: Option<String>,
    /// 工作目录。加一个新根时拿它守住「中立库不许被圈进主库」那条线（ADR-0004）；
    /// `None` 表示这一趟没有工作目录可守（只活在内存里的库）。
    pub workspace: Option<PathBuf>,
    /// 并发档。
    pub jobs: Jobs,
    /// 每类文件抽样读多少个头部。0 表示不抽样。
    pub samples_per_class: usize,
    /// 例子列表与索引的上限。
    pub limits: Limits,
    /// 断点配置；`None` 表示不写断点（也就不能续跑）。
    pub checkpoint: Option<CheckpointOptions>,
    /// 按 `(路径, 大小, 修改时间)` 跳过未变的文件。
    ///
    /// 关掉它就是「当作从没扫过」重看一遍：每个文件重新抽样、重新归类，中立库里的旧
    /// 结论一律作废。判据出了问题、或怀疑中立库与磁盘对不上时才需要。
    pub incremental: bool,
    /// 读**透明容器**内部装着什么：内部每个文件的名字、大小与可得的 CRC-32。
    ///
    /// 库里 91.1% 的容量装在透明容器里，不穿透的话报告只看得见「这里有一个 3GB 的容器」，
    /// 后面的识别也无从谈起（ADR-0014）。关掉它只在一种场合有意义：怀疑穿透本身
    /// 有问题，想先把遍历跑通。
    pub penetrate_containers: bool,
    /// 为 `.zst` 付**全量解压**的代价，把它的内部构成也读出来。
    ///
    /// **它不叫穿透**（`CONTEXT.md`：穿透是零解压那件事）。zip 与 7z 穿得透，几乎不花钱；
    /// `.zst` 穿不了——格式里就没有内部清单，列全清单只能把整条流解一遍
    /// （ADR-0014 的第二段修订）。主库里这是 2,685 个文件、2.50 TiB，
    /// 真机实测约 125 MB/s，一趟约 **5.8 小时**（瓶颈全在磁盘）。
    ///
    /// **默认关**：让一条 `romcat scan` 默认从半小时变成一整夜，是不能不打招呼就做的事
    /// （挂账 D94）。打开一次即可——结论按三元组落进中立库，往后的扫描原样沿用
    /// （`Baseline::penetrated`），实测二次扫描 1.0 秒。没打开时那批容器只是
    /// **还没读过**，既不是穿透了也不是穿不透，报告单列一栏。
    pub decompress_zst: bool,
    /// **平台清单与成型规则**。平台由目录给出，而哪些目录算平台写在这里（ADR-0011）。
    pub manifest: Manifest,
}

impl ScanOptions {
    /// 用默认参数扫描 `root`，根名按目录自己的名字取。
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            root_name: None,
            workspace: None,
            jobs: Jobs::Adaptive,
            samples_per_class: 32,
            limits: Limits::default(),
            checkpoint: None,
            incremental: true,
            penetrate_containers: true,
            decompress_zst: false,
            manifest: Manifest::builtin(),
        }
    }

    /// 用默认参数扫描 `root`，并**点名**这个根叫什么。
    #[must_use]
    pub fn named(root: impl Into<PathBuf>, name: impl Into<String>) -> Self {
        Self {
            root_name: Some(name.into()),
            ..Self::new(root)
        }
    }
}

/// 探测下不了判断时的并发数：按 CPU 数取，夹在 2 到 8 之间。
///
/// **它是退路而不是默认值**。按 CPU 数取是照 CPU 密集型工作设计的，而扫盘是高延迟
/// I/O 密集型——真机实测 CPU 占用只有 2.7%（挂账 D9）。介质量得出来时一律听
/// [`probe::probe`] 的；量不出来（目录太少、被中断）才退到这里。
#[must_use]
pub fn default_jobs() -> usize {
    thread::available_parallelism()
        .map_or(4, std::num::NonZero::get)
        .clamp(2, 8)
}

/// 一次扫描的产出。
#[derive(Debug)]
pub struct ScanOutcome {
    /// 体检报告。中断时它是「到目前为止」的报告，仍然可读。
    pub report: HealthReport,
    /// 统计状态本身，供测试与后续票使用。
    pub aggregate: Aggregate,
    /// 这次扫描相对中立库上一次状态的差异。
    ///
    /// **续跑时它只涵盖这一趟**：中断之前那部分的结论早已写进中立库，这一趟看到它们
    /// 时三元组已经对得上，于是算作未变。
    pub delta: ScanDelta,
    /// 是否被中断。
    pub interrupted: bool,
    /// 断点文件位置（中断且写了断点时）。
    pub checkpoint_path: Option<PathBuf>,
    /// 开扫前那次介质探测的读数；`-j` 点了名就没探测，是 `None`。
    pub probe: Option<probe::Probe>,
    /// 这一趟有没有重新**成型**。中断的扫描不成型——半个库成出来的变体是错的。
    pub shaped: bool,
}

/// 扫一遍主库里的**一个根**，把结论写进中立库。
///
/// `task` 是那个「报进度 + 能停」的把手：这一趟按四步报进度，停下来的地方是干净的
/// （断点已经写下，续跑接着来）。命令行把 Ctrl-C 接在它的
/// [`CancelToken`](crate::task::Handle::cancel) 上，界面把「停下」按钮接在它上面。
///
/// # Errors
/// 扫描根打不开、这个根加不进来、断点落在主库内、断点读写失败或中立库读写失败时
/// 返回错误。
pub fn scan(
    library: &dyn LibraryFs,
    catalog: &mut Catalog,
    options: &ScanOptions,
    task: &Handle,
) -> Result<ScanOutcome, ScanError> {
    let started = Instant::now();
    let cancel = task.cancel();
    task.steps(SCAN_STEPS);
    let root = library
        .canonicalize(&options.root)
        .map_err(|source| ScanError::Root {
            path: path::display(&options.root),
            source,
        })?;

    // 断点是这条流程里唯一写进文件系统的东西，它必须落在主库之外。比较前两边都化成
    // 绝对形态，否则 `/var` 与 `/private/var` 这类链接会让守卫形同虚设。
    //
    // **两边化开走的必须是同一套文件系统**：扫描根上面刚过了 `library.canonicalize`，
    // 断点这一侧就也得问 `library`。问两套的话，`/lib` 是指向 `usr/lib` 的符号链接
    // （一切 merged-usr 的发行版）时，根折出来还是 `/lib`、断点折出来成了
    // `/usr/lib/…`，`is_inside` 只比前缀，于是这道闸静默失效——扫描照跑，带着断点
    // 往只读的主库里写（ADR-0004）。
    let mut guarded: Vec<(&'static str, &Path)> = Vec::new();
    if let Some(config) = &options.checkpoint {
        guarded.push(("断点文件", &config.path));
    }
    if let Some(file) = catalog.file() {
        guarded.push(("中立库", file));
    }
    for (what, target) in guarded {
        let folded = path::normalize_existing_in(target, |p| library.canonicalize(p));
        if path::is_inside(&root, &folded) {
            return Err(ScanError::WritesInsideLibrary {
                what,
                path: path::display(target),
            });
        }
    }

    // 这一趟扫的是哪个根：认下名字、拦掉「同一个根名底下换了另一块盘」。
    let _ = task.step("认根");
    let root_name = resolve_root(library, catalog, options, &root)?;

    // 并发按介质定而不是按 CPU 数猜（挂账 D9）。`-j` 点了名就照办，一个目录都不多读。
    let measured = match options.jobs {
        Jobs::Fixed(_) => None,
        Jobs::Adaptive => Some(probe::probe(library, &root, cancel)),
    };
    let resolved_jobs = match options.jobs {
        Jobs::Fixed(jobs) => jobs.max(1),
        Jobs::Adaptive => measured.map_or_else(default_jobs, |probe| probe.jobs),
    };

    let mut start = load_start_state(options, &root, &root_name, catalog)?;
    let mut traversal = Traversal {
        scan: start.scan,
        root_name: root_name.clone(),
        root: path::display(&root),
        elapsed_ms: elapsed(start.elapsed_base, started),
        jobs: resolved_jobs,
        samples_per_class: options.samples_per_class,
        penetrated_containers: options.penetrate_containers,
        interrupted: true,
        resumed: start.resumed,
    };
    if !start.resumed {
        catalog.begin_scan(start.scan, &root_name)?;
    }
    // 先把这次扫描的行占上，代号才不会因为进程半路被杀而被下次重用。
    catalog.save_traversal(&traversal)?;
    // 根自己也是一条记录，键就是它的名字——「扫过几个目录」是从表里数出来的。
    catalog.write(start.scan, &[root_record(&root_name, &root)])?;
    let _ = task.step("遍历");

    let baseline = if options.incremental {
        catalog.baseline()?
    } else {
        Baseline::empty()
    };
    let budget = SampleBudget::new(options.samples_per_class);
    let queue = Queue::new(std::mem::take(&mut start.pending));

    let (results_tx, results_rx) = mpsc::channel::<DirResult>();
    let jobs = traversal.jobs;
    let mut progress = Progress::default();

    let interrupted = thread::scope(|scope| -> Result<bool, ScanError> {
        for _ in 0..jobs {
            let tx = results_tx.clone();
            let queue = &queue;
            let budget = &budget;
            let rooted = Rooted {
                name: root_name.as_str(),
                path: &root,
            };
            let baseline = &baseline;
            scope.spawn(move || {
                while let Some(dir) = queue.pop() {
                    let result =
                        process_dir(library, rooted, dir, options, budget, baseline, cancel);
                    if tx.send(result).is_err() {
                        break;
                    }
                }
            });
        }
        drop(results_tx);

        let mut last_save = Instant::now();
        let outcome = (|| -> Result<bool, ScanError> {
            let mut interrupted = false;
            loop {
                if cancel.is_cancelled() {
                    interrupted = true;
                    break;
                }
                if queue.is_drained() {
                    break;
                }
                match results_rx.recv_timeout(Duration::from_millis(100)) {
                    // 被中断打断的目录只扫了一半，整份丢掉：它仍留在 `active` 里，
                    // 会原样进断点，续跑时重扫一遍。合并半份结果会让那个目录里剩下的
                    // 文件与子目录**永久消失**——重做一个目录，好过少算一个目录。
                    Ok(result) if result.partial => {}
                    Ok(result) => {
                        let done = merge(catalog, start.scan, &root_name, &mut progress, result)?;
                        queue.finish_and_push(done);
                        // 遍历说不出分母（走完才知道有多少条目），于是只报分子：
                        // 总数填 0，界面据此画一条来回跑的条而不是一条假装知道进度的条。
                        task.tick(progress.delta.total(), 0);
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
                if let Some(config) = &options.checkpoint
                    && last_save.elapsed() >= config.interval
                {
                    traversal.elapsed_ms = elapsed(start.elapsed_base, started);
                    save_progress(catalog, options, &root, &queue, &mut progress, &traversal)?;
                    last_save = Instant::now();
                }
            }
            Ok(interrupted)
        })();
        // **出错这条路也要关队列。** 工作线程阻塞在 `Queue::pop` 的条件变量上，只有
        // `close` 叫得醒它们；而 `thread::scope` 退出前一定要 join。协调这一头带着
        // `?` 直接跳出去的话，谁都不再 `close`，于是「断点写不进去」这条本该说出口的
        // 错误变成整个进程挂住——挂单 Q8 的第二个症状正是这个。
        queue.close();
        outcome
    })?;

    traversal.elapsed_ms = elapsed(start.elapsed_base, started);
    traversal.interrupted = interrupted;
    catalog.write(start.scan, &progress.records)?;
    progress.records.clear();

    // 删除只在完整扫完一遍之后判。中断的扫描没走完整个库，「这次没见到」不等于
    // 「不存在」——那时候清一遍会把还没扫到的那半个库当成已删除抹掉。
    if !interrupted {
        let _ = task.step("收尾");
        progress.delta.removed = catalog.sweep(start.scan, &root_name)?;
    }
    catalog.save_traversal(&traversal)?;
    // 这个根上次扫的结果落在它自己那一行上——**盘没挂上时它照样看得见**。
    catalog.record_root_scan(
        &root_name,
        &RootScan {
            at: crate::catalog::now_secs(),
            elapsed_ms: traversal.elapsed_ms,
            entries: catalog.root_stats(&root_name)?.files,
            interrupted,
        },
    )?;
    // **成型也只在完整扫完一遍之后跑**，理由与删除同源：半个库上成出来的变体是错的。
    // 一份 PSV 转储只扫到 `app/` 就成型，`patch/` 那一半会在下一趟变成第二个变体。
    let shaped = !interrupted;
    if shaped {
        let _ = task.step("成型");
        shape::reshape(catalog, &options.manifest, start.scan)?;
    }
    let checkpoint_path =
        finish_checkpoint(options, &root, &queue, start.scan, &traversal, interrupted)?;

    let aggregate = catalog.aggregate(&options.limits, &options.manifest)?;
    let meta = ReportMeta {
        root_name: traversal.root_name.clone(),
        root: traversal.root.clone(),
        scan: start.scan,
        interrupted,
        resumed: start.resumed,
        jobs,
        samples_per_class: options.samples_per_class,
        penetrated_containers: options.penetrate_containers,
        delta: Some(progress.delta),
    };
    Ok(ScanOutcome {
        report: HealthReport::build(&aggregate, &meta),
        aggregate,
        delta: progress.delta,
        interrupted,
        checkpoint_path,
        probe: measured,
        shaped,
    })
}

fn root_record(root_name: &str, root: &Path) -> EntryRecord {
    EntryRecord {
        key: root_name.to_string(),
        kind: EntryKind::Dir,
        meta: EntryMeta::Known {
            len: 0,
            modified: None,
        },
        non_utf8: !path::is_utf8(root),
        verdict: Verdict::Added,
        sample: None,
        container: None,
    }
}

fn elapsed(base: Duration, started: Instant) -> u64 {
    u64::try_from((base + started.elapsed()).as_millis()).unwrap_or(u64::MAX)
}

/// 这次扫描从哪儿开始。
struct StartState {
    pending: Vec<PathBuf>,
    scan: i64,
    elapsed_base: Duration,
    resumed: bool,
}

/// 这一趟扫的是哪个**根**：认下名字，再核一遍它还是不是原来那块盘。
///
/// 名字没给就按目录自己的名字取。库里还没有这个根就**加进来**（校验走
/// [`roots::add_root`]：不许重名、不许与已有的根套在一起、不许与工作目录纠缠）。
///
/// **守卫每一趟都跑，路径变没变都跑。** 它只花一次 `read_dir`，而扫描本来就要读根
/// 这一层——真机上那是 27 分钟里的一次目录读取。反过来「路径没变就直接放行」看着
/// 省事，代价是这条最常走的路上一道闸都没有：盘不在位时挂载点目录还在、路径一个字
/// 没变，于是遍历顺利跑完、收尾把整个根抹掉。
///
/// # Errors
/// 名字不能用、根加不进来、这个根不在位、这个根名底下换了另一块盘，或者中立库
/// 读写失败时返回错误。
fn resolve_root(
    library: &dyn LibraryFs,
    catalog: &mut Catalog,
    options: &ScanOptions,
    root: &Path,
) -> Result<String, ScanError> {
    let wanted = match &options.root_name {
        Some(name) => name.clone(),
        None => default_root_name(root),
    };
    let name = path::root_name(&wanted).map_err(AddRootError::Name)?;
    let current = path::display(root);
    let Some(existing) = catalog.root(&name)? else {
        // 这个路径可能已经是**另一个名字**的根：那不是新根，是同一支记录换了个叫法。
        // 交给 `add_root` 去拦，它说得出撞上的是哪一个。
        roots::add_root(catalog, options.workspace.as_deref(), &name, root)?;
        return Ok(name);
    };
    let moved = existing.path != current;
    guard_same_root(library, catalog, &name, &existing.path, root, moved)?;
    if !moved {
        return Ok(name);
    }
    // **改指到别处也要过摆位那三道校验**：把「主库」从 `/盘/Game` 重指到 `/盘`
    // （而 `/盘/Game/FC` 已经是另一个根）不拦的话，同一批文件从此在两个根下各数一遍。
    roots::check_placement(catalog, options.workspace.as_deref(), &name, root)?;
    catalog.set_root_path(&name, &current)?;
    Ok(name)
}

/// 没给名字时，一个根默认叫什么：目录自己的名字。
///
/// **公开出去，因为断点文件名要带根名**（`workspace::checkpoint_path`），而算断点路径
/// 那一步在开扫之前。两处各猜一遍的话，`--resume` 会去找一个不存在的断点。
///
/// **传进来的必须是化开之后的根**（[`path::normalize_existing`]，或者
/// [`LibraryFs::canonicalize`] 的结果——`resolve_root` 走的就是后者）。用户敲的原串
/// 没有末级名字的写法不止一种：`.`、`x/..`、单独一个 `/`，`file_name()` 一律给 `None`，
/// 这里就退成「主库」——于是两个毫不相干的目录用 `scan .` 扫会共用一个断点文件。
#[must_use]
pub fn default_root_name(root: &Path) -> String {
    root.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "主库".to_string())
}

/// 这个**根名**对着的那块盘还在不在、还是不是原来那一块。
///
/// 根跟名字走而不跟路径走（挂账 D16），于是换挂载点、换盘符都还能找回同一支记录——
/// 这正是要的。代价是两件事变得可能，而两件事都不会报错、只会静默撞车或静默抹掉：
///
/// - **盘不在位**：挂载点目录永远都在，盘一拔它就剩个空壳。路径一个字没变，遍历
///   顺利跑完，收尾把整个根当成「这次没见到」删光。
/// - **名字一样、盘不一样**：这个根下面的键都以它的名字打头，两块盘的记录会挤进
///   同一串前缀。
///
/// 判据是顶层条目：库里记着的这个根的顶层名，与眼前这个目录 `read_dir` 出来的名字比一比。
/// 它只花一次 `read_dir`（扫描本来也要读这一层），却足够把三种形状分开。**分开它们靠的
/// 是「路径变没变」**，因为那说的是用户改没改主意：
///
/// - **路径没变、顶层一条都对不上** → 盘不在位。用户什么都没改，是那块盘自己不在。
///   判据是绝对的「一条都不剩」而不是比例——顶层只有两三个目录、用户合法删了其中
///   大半时，只要还认得出一条就照常放行，不该拿阈值去拦用户自己动的手。
/// - **路径变了、顶层大半对不上** → 指错了盘（[`ScanError::DifferentLibrary`]）。
/// - 其余照常放行。
///
/// **它认不出的那种情况**：两块盘恰好有过半同名的顶层目录——那得是刻意造的巧合，真出现
/// 了也还有「换个根名」这条路。
///
/// **按根生效**：别的根一个字都不受影响，那正是「主库是一组根」要的形状。
fn guard_same_root(
    library: &dyn LibraryFs,
    catalog: &Catalog,
    name: &str,
    recorded: &str,
    root: &Path,
    moved: bool,
) -> Result<(), ScanError> {
    let recorded_keys = catalog.top_level_keys(name, TOP_LEVEL_SAMPLE)?;
    // 一条都还没扫过的根没什么可撞的。
    if recorded_keys.is_empty() {
        return Ok(());
    }
    let entries = match library.read_dir(root) {
        Ok(entries) => entries,
        // **列不开给不出任何判据**，而遍历本来就会照实记一条错误、把整棵子树原样留着
        // （`Catalog::keep_subtree`，ADR-0021）——守卫不该抢在它前面把扫描打断。
        // 路径变了那一趟例外：那时还要往库里改「这个根现在挂在哪」，一个列都列不开的
        // 目录不配当那个答案。
        Err(_) if !moved => return Ok(()),
        Err(source) => {
            return Err(ScanError::Root {
                path: path::display(root),
                source,
            });
        }
    };
    let actual: std::collections::HashSet<String> = entries
        .iter()
        .map(|entry| path::catalog_key(root, &entry.path))
        .collect();
    let common = recorded_keys
        .iter()
        .filter(|key| actual.contains(*key))
        .count();
    // **盘不在位**。两种形状：这一层什么都没有（不论路径变没变——指到一个空目录上去
    // 从来不是「换了另一块盘」），或者路径压根没变而库里记着的顶层一条都不在。
    if entries.is_empty() || (!moved && common == 0) {
        return Err(ScanError::RootNotInPlace {
            name: name.to_string(),
            path: path::display(root),
            present: entries.len(),
            recorded_count: recorded_keys.len(),
        });
    }
    // 路径没变、顶层还认得出几条：盘在位，剩下的差异是用户自己在这块盘上动的手，
    // 照常扫、照常收尾。
    if !moved {
        return Ok(());
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "顶层条目至多 512 条，转 f64 精确"
    )]
    let ratio = common as f64 / recorded_keys.len() as f64;
    if ratio < SAME_LIBRARY_OVERLAP {
        return Err(ScanError::DifferentLibrary {
            name: name.to_string(),
            recorded: recorded.to_string(),
            current: path::display(root),
            common,
            recorded_count: recorded_keys.len(),
        });
    }
    Ok(())
}

/// 这一趟从哪儿开始：接着断点跑，还是从根重来。
///
/// **断点的身份是「这个根在中立库里最后走的那一趟，就是断点说的那一趟」。**
/// 它守的是一件很具体的事：续跑沿用断点里的代号收尾，而收尾删的是「这个根底下这次
/// 没见到的」——断点写下之后这个根要是又被扫过一趟，那一趟记下的东西就会被当成
/// 没见到整批删掉。判据直接说这句话，两种对不上的情况因此都落在同一条线上：
///
/// - 中间**扫过别的根**：甲那一支从中断之后一个字节都没动过，甲最后一趟就是断点那一趟
///   ——照旧续跑。老判据是「断点的代号 + 1 等于下一个代号」，主库变成一组根之后
///   （`CONTEXT.md` 的**根**）扫一遍乙盘就把代号推走了，于是甲整根重扫。
/// - **移除再加回**同名同路径的根：那个根的遍历行随 [`Catalog::remove_root`] 一起没了，
///   断点再也对不上谁——从头扫一遍。老判据在这里恰好放行，只扫 `pending` 那一半，
///   收尾什么都删不到，扫完却报「完整」。
///
/// 对不上一律**从头扫**而不是报错：移除再加回本来就是「当它是新的」的意思，加回来
/// 第一趟本该从头走；报错只会逼用户去找一个他不知道在哪的断点文件。
fn load_start_state(
    options: &ScanOptions,
    root: &Path,
    root_name: &str,
    catalog: &Catalog,
) -> Result<StartState, ScanError> {
    let next = catalog.next_scan()?;
    let fresh = || StartState {
        pending: vec![root.to_path_buf()],
        scan: next,
        elapsed_base: Duration::ZERO,
        resumed: false,
    };
    let Some(config) = &options.checkpoint else {
        return Ok(fresh());
    };
    if !config.resume || !config.path.exists() {
        return Ok(fresh());
    }
    let checkpoint = Checkpoint::load(&config.path, root)?;
    let pending = checkpoint.pending();
    if pending.is_empty() {
        return Ok(fresh());
    }
    // 这个根在中立库里最后走的那一趟，得就是断点说的那一趟。这个根还没有遍历行
    // （从没扫过，或者刚被移除又加回来）时一样对不上——那就是从头扫。
    let last = catalog.last_traversal_of(root_name)?;
    if last.is_none_or(|traversal| traversal.scan != checkpoint.scan) {
        return Ok(fresh());
    }
    Ok(StartState {
        pending,
        // 续跑必须沿用同一个代号，否则收尾时「这次没见到」会把中断之前扫到的记录全删掉。
        scan: checkpoint.scan,
        elapsed_base: Duration::from_millis(checkpoint.elapsed_ms),
        resumed: true,
    })
}

/// 存一次进度。
///
/// 顺序是有讲究的：**先把攒着的记录写进中立库，再写断点**。反过来的话，断点会说
/// 「这个目录扫完了」而中立库里没有它的记录，续跑就会永久漏掉那一批文件。
fn save_progress(
    catalog: &mut Catalog,
    options: &ScanOptions,
    root: &Path,
    queue: &Queue,
    progress: &mut Progress,
    traversal: &Traversal,
) -> Result<(), ScanError> {
    catalog.write(traversal.scan, &progress.records)?;
    progress.records.clear();
    catalog.save_traversal(traversal)?;
    save_checkpoint(options, root, queue, traversal.scan, traversal.elapsed_ms)
}

fn save_checkpoint(
    options: &ScanOptions,
    root: &Path,
    queue: &Queue,
    scan: i64,
    elapsed_ms: u64,
) -> Result<(), ScanError> {
    let Some(config) = &options.checkpoint else {
        return Ok(());
    };
    let pending = queue.pending_left();
    Checkpoint::new(root, options.samples_per_class, &pending, scan, elapsed_ms)
        .save(&config.path)?;
    Ok(())
}

fn finish_checkpoint(
    options: &ScanOptions,
    root: &Path,
    queue: &Queue,
    scan: i64,
    traversal: &Traversal,
    interrupted: bool,
) -> Result<Option<PathBuf>, ScanError> {
    let Some(config) = &options.checkpoint else {
        return Ok(None);
    };
    if interrupted {
        save_checkpoint(options, root, queue, scan, traversal.elapsed_ms)?;
        return Ok(Some(config.path.clone()));
    }
    // 扫完了就把断点删掉：留着它只会让下次 `--resume` 误以为还有活没干完。
    let _ = fs::remove_file(&config.path);
    Ok(None)
}

/// 协调线程手上的进度：攒着待写的记录，以及这一趟的差异计数。
#[derive(Debug, Default)]
struct Progress {
    records: Vec<EntryRecord>,
    delta: ScanDelta,
}

fn merge(
    catalog: &mut Catalog,
    scan: i64,
    root_name: &str,
    progress: &mut Progress,
    result: DirResult,
) -> Result<DirDone, CatalogError> {
    for dir in &result.skipped_system_dirs {
        catalog.note_skipped_dir(root_name, dir)?;
    }
    for failed in &result.unlistable {
        catalog.note_error(root_name, &failed.display, &failed.message)?;
        // 列不开的目录下面那些记录这一趟一条也写不到。不在这儿把它们标成「见过」，
        // 收尾时就会被当成已删除抹掉——一次拒绝访问抹掉整棵子树（ADR-0021）。
        catalog.keep_subtree(scan, &failed.key)?;
    }
    for record in result.entries {
        // 差异只数文件。目录与链接也进中立库，但「新增了 3 个」说的该是内容，
        // 不该被目录冲淡。
        if record.kind == EntryKind::File {
            progress.delta.record(record.verdict);
        }
        progress.records.push(record);
    }
    if progress.records.len() >= WRITE_BATCH {
        catalog.write(scan, &progress.records)?;
        progress.records.clear();
    }
    Ok(DirDone {
        dir: result.dir,
        subdirs: result.subdirs,
    })
}

struct DirDone {
    dir: PathBuf,
    subdirs: Vec<PathBuf>,
}

/// 一个列不开的目录。
///
/// 它下面的记录这一趟一条也写不到，因此必须**原样留着**——看不见不等于不存在。
struct UnlistableDir {
    /// 中立库里这棵子树的键前缀。
    key: String,
    /// 展示用路径。
    display: String,
    /// 列不开的原因。
    message: String,
}

struct DirResult {
    dir: PathBuf,
    subdirs: Vec<PathBuf>,
    entries: Vec<EntryRecord>,
    skipped_system_dirs: Vec<String>,
    unlistable: Vec<UnlistableDir>,
    /// 这个目录是被中断打断的，只扫了一部分，不能并入统计。
    partial: bool,
}

/// 这一趟扫的那个**根**：名字与它眼下挂在哪。
///
/// 两样捆在一起交下去，因为工作线程每算一条键都同时要它们——路径用来剥前缀，
/// 名字用来当键的第一段（[`path::library_key`]）。
#[derive(Debug, Clone, Copy)]
struct Rooted<'a> {
    name: &'a str,
    path: &'a Path,
}

fn process_dir(
    library: &dyn LibraryFs,
    rooted: Rooted<'_>,
    dir: PathBuf,
    options: &ScanOptions,
    budget: &SampleBudget,
    baseline: &Baseline,
    cancel: &CancelToken,
) -> DirResult {
    let mut result = DirResult {
        dir,
        subdirs: Vec::new(),
        entries: Vec::new(),
        skipped_system_dirs: Vec::new(),
        unlistable: Vec::new(),
        partial: false,
    };

    let entries = match library.read_dir(&result.dir) {
        Ok(entries) => entries,
        Err(error) => {
            result.unlistable.push(UnlistableDir {
                key: path::library_key(rooted.name, rooted.path, &result.dir),
                display: path::display(&result.dir),
                message: error.to_string(),
            });
            return result;
        }
    };

    for entry in entries {
        if cancel.is_cancelled() {
            result.partial = true;
            break;
        }
        if entry.kind == EntryKind::Dir {
            let name = path::file_name_lower(&entry.path);
            if classify::is_skipped_system_dir(&name) {
                // 跳过什么都要说出来，不能悄悄少扫。整棵跳过的目录不进中立库，
                // 于是它也不会被算进「库里有几个目录」。
                result.skipped_system_dirs.push(path::display(&entry.path));
                continue;
            }
            result.subdirs.push(entry.path.clone());
        }
        // 符号链接不跟随：跟随会引入环，也会让同一份内容被算两次。它照样进中立库，
        // 因为报告要说出库里有几个链接。
        result
            .entries
            .push(observe(library, rooted, &entry, options, budget, baseline));
    }
    result
}

fn observe(
    library: &dyn LibraryFs,
    rooted: Rooted<'_>,
    entry: &DirEntry,
    options: &ScanOptions,
    budget: &SampleBudget,
    baseline: &Baseline,
) -> EntryRecord {
    // 键要带根名、要过 NFC，读盘用的仍是系统给的原始路径（ADR-0020）。
    let key = path::library_key(rooted.name, rooted.path, &entry.path);
    let verdict = baseline.verdict(&key, &entry.meta);
    // 未变的文件不再打开一次：上次抽到的头部结论与内部构成都留在中立库里，
    // 报告照样用得上。容器不必重穿一遍，那正是这条接缝在票 07 之后真正省下的活。
    let fresh =
        entry.kind == EntryKind::File && matches!(verdict, Verdict::Added | Verdict::Changed);
    let sample = if fresh {
        sample_header(
            library,
            &entry.path,
            entry.meta.byte_len().unwrap_or(0),
            header::probe_class_for(&entry.path),
            options,
            budget,
        )
    } else {
        None
    };
    // 已经穿透过、且三元组没变的容器不必重穿——那正是这条接缝省下的活。但中立库里
    // 压根没有它的穿透结论时（上次是 `--no-containers` 扫的）必须补上，
    // 否则那批容器永远不会被穿透，报告会静悄悄地少报一批内部文件。
    let container = if options.penetrate_containers
        && entry.kind == EntryKind::File
        && (fresh || !baseline.penetrated(&key))
    {
        penetrate(library, &entry.path, options)
    } else {
        None
    };
    EntryRecord {
        key,
        kind: entry.kind,
        meta: entry.meta,
        non_utf8: !path::is_utf8(&entry.path),
        verdict,
        sample,
        container,
    }
}

/// 零解压穿透一个**透明容器**。
///
/// **穿不透绝不中断扫描**：一个坏掉的 zip、一个要密码的 7z、一个 `stat` 都失败的
/// 文件，都只是如实记一笔原因（ADR-0021 的道理，粒度在容器上）。库里约 1.59% 的
/// 文件在 macOS 的 fskit 驱动下连元数据都读不到，那批会全部落在这里。
fn penetrate(library: &dyn LibraryFs, file: &Path, options: &ScanOptions) -> Option<Penetration> {
    let kind = ContainerKind::for_path(file)?;
    // 穿不透的格式（今天只有 zst）得真的解一遍才读得出内部构成，不打招呼不做——那是
    // 几小时而不是几毫秒。**返回 `None` 而不是记一笔穿不透**：它不是穿不透，是还没读过，
    // 而这两句话指向完全不同的下一步（`CONTEXT.md`）。
    if !kind.is_penetrable() && !options.decompress_zst {
        return None;
    }
    match container::list_as(library, file, kind) {
        Ok(listing) => Some(Penetration::listed(&listing)),
        Err(error) => Some(Penetration::failed(kind, &error)),
    }
}

fn sample_header(
    library: &dyn LibraryFs,
    file: &Path,
    len: u64,
    class: Option<ProbeClass>,
    options: &ScanOptions,
    budget: &SampleBudget,
) -> Option<(ProbeClass, SampleResult)> {
    if options.samples_per_class == 0 {
        return None;
    }
    let class = class?;
    if !budget.try_take(class) {
        return None;
    }
    let head = match library.read_head(file, class.head_len()) {
        Ok(head) => head,
        Err(error) => return Some((class, SampleResult::Unreadable(error.to_string()))),
    };
    let tail = if class.tail_len() == 0 {
        Vec::new()
    } else {
        match library.read_tail(file, class.tail_len()) {
            Ok(tail) => tail,
            Err(error) => return Some((class, SampleResult::Unreadable(error.to_string()))),
        }
    };
    Some((
        class,
        SampleResult::Probed(header::probe(class, &head, &tail, len)),
    ))
}

/// 每类文件的抽样配额。
///
/// 它限的是**这一趟扫描开几个文件**，不是中立库里攒了几个样本。反过来（跨扫描累加）
/// 会让配额一旦填满，后来新增的文件**永远不被抽样**，成功率就此冻结在首扫那批上。
/// 而只抽新增与已变的文件，本身就把没变的那些排除在外了——配额每趟从零算，
/// 既不会重复开同一个文件，新东西也总有机会被看一眼。
///
/// 被丢弃的结果（中断时正在扫的那个目录）会白占配额，但那至多是一个目录的量。
struct SampleBudget {
    per_class: usize,
    taken: Mutex<std::collections::BTreeMap<ProbeClass, usize>>,
}

impl SampleBudget {
    fn new(per_class: usize) -> Self {
        Self {
            per_class,
            taken: Mutex::new(std::collections::BTreeMap::new()),
        }
    }

    fn try_take(&self, class: ProbeClass) -> bool {
        let mut taken = self.taken.lock().unwrap_or_else(|e| e.into_inner());
        let count = taken.entry(class).or_insert(0);
        if *count >= self.per_class {
            return false;
        }
        *count += 1;
        true
    }
}

struct QueueState {
    stack: Vec<PathBuf>,
    active: Vec<PathBuf>,
    closed: bool,
}

/// 待扫目录队列。
///
/// `active` 是正在被工作线程扫的目录。它必须和 `stack` 一起进断点：那些目录的记录还没
/// 写进中立库，续跑时重扫一遍才不会漏。
struct Queue {
    state: Mutex<QueueState>,
    ready: Condvar,
}

impl Queue {
    fn new(initial: Vec<PathBuf>) -> Self {
        Self {
            state: Mutex::new(QueueState {
                stack: initial,
                active: Vec::new(),
                closed: false,
            }),
            ready: Condvar::new(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, QueueState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn pop(&self) -> Option<PathBuf> {
        let mut state = self.lock();
        loop {
            // 关了就立刻停手，哪怕栈里还有东西——那些东西要原样留在栈里进断点。
            if state.closed {
                return None;
            }
            if let Some(dir) = state.stack.pop() {
                state.active.push(dir.clone());
                return Some(dir);
            }
            state = self.ready.wait(state).unwrap_or_else(|e| e.into_inner());
        }
    }

    /// 一个目录的结果已经并入进度：把它从 `active` 摘掉，并把它的子目录入队。
    fn finish_and_push(&self, done: DirDone) {
        let mut state = self.lock();
        if let Some(index) = state.active.iter().position(|dir| *dir == done.dir) {
            state.active.swap_remove(index);
        }
        state.stack.extend(done.subdirs);
        drop(state);
        self.ready.notify_all();
    }

    fn is_drained(&self) -> bool {
        let state = self.lock();
        state.stack.is_empty() && state.active.is_empty()
    }

    fn pending_left(&self) -> Vec<PathBuf> {
        let state = self.lock();
        let mut pending = state.stack.clone();
        pending.extend(state.active.iter().cloned());
        pending
    }

    fn close(&self) {
        let mut state = self.lock();
        state.closed = true;
        drop(state);
        self.ready.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::{Category, SuspectReason};
    use crate::fs::{DirEntry, MemFs};
    use crate::platform::Manifest;
    use crate::report::UNKNOWN_PLATFORM_LABEL;
    use crate::scan::aggregate::UNKNOWN_PLATFORM;
    use crate::testing::sample::{chd, iso, zip};
    use std::sync::atomic::AtomicUsize;

    /// 一个够小、但把这张票关心的每种情况都摆上一份的 fixture 主库。
    fn 建库() -> MemFs {
        建库于("/lib")
    }

    /// 同一棵树，搭在任意一个根下面。换挂载点那几条测试要拿它比。
    fn 建库于(root: &str) -> MemFs {
        let mut library = MemFs::new();
        library
            .dir(root)
            .file(format!("{root}/FC/超级马里奥.zip"), zip(2048))
            .file(format!("{root}/FC/魂斗罗.zip"), vec![0u8; 512]) // 扩展名说 zip，内容不是
            .file(
                format!("{root}/FC/说明.txt"),
                "随便写点什么".as_bytes().to_vec(),
            )
            .file(format!("{root}/FC/备份/超级马里奥.zip"), zip(2048)) // 与上面同名同大小
            .file(format!("{root}/PS1/最终幻想.chd"), chd())
            .file(format!("{root}/PS1/模拟器/epsxe.exe"), vec![0u8; 100])
            .file(format!("{root}/PSP/游戏.iso"), iso())
            .file(format!("{root}/PSP/半截下载.iso.part"), vec![0u8; 10])
            .file(format!("{root}/散落的游戏.gba"), vec![0u8; 16])
            .file(format!("{root}/.DS_Store"), vec![0u8; 8])
            .file(format!("{root}/__MACOSX/垃圾"), vec![0u8; 8]) // 整棵跳过
            .file(format!("{root}/空文件.zip"), Vec::new())
            .symlink(format!("{root}/指向别处"));
        library
    }

    fn 新中立库() -> Catalog {
        Catalog::open_in_memory().expect("能开中立库")
    }

    fn 扫入(
        catalog: &mut Catalog,
        library: &dyn LibraryFs,
        options: &ScanOptions,
    ) -> ScanOutcome {
        scan(library, catalog, options, &Handle::new()).expect("扫描不该失败")
    }

    fn 扫(library: &dyn LibraryFs, options: &ScanOptions) -> ScanOutcome {
        扫入(&mut 新中立库(), library, options)
    }

    /// 耗时是唯一每次都不同的字段。
    fn 规范化(aggregate: &mut Aggregate) {
        aggregate.elapsed_ms = 0;
    }

    /// 断点按**根**分文件（`workspace::checkpoint_path`），测试里也照这个形状取。
    fn 断点选项(dir: &Path, root_name: &str) -> CheckpointOptions {
        CheckpointOptions {
            path: dir
                .join("scans")
                .join(format!("{root_name}.checkpoint.json")),
            interval: Duration::ZERO,
            resume: true,
        }
    }

    #[test]
    fn 按平台目录给出文件数与容量() {
        let library = 建库();
        let outcome = 扫(&library, &ScanOptions::named("/lib", "库"));
        let report = &outcome.report;

        let fc = report
            .platforms
            .iter()
            .find(|p| p.name == "FC")
            .expect("有 FC");
        assert_eq!(fc.files, 4);
        assert_eq!(fc.bytes, 2048 + 512 + "随便写点什么".len() as u64 + 2048);

        let ps1 = report
            .platforms
            .iter()
            .find(|p| p.name == "PS1")
            .expect("有 PS1");
        assert_eq!(ps1.files, 2);
    }

    #[test]
    fn 认不出平台的文件照常计入报告() {
        let library = 建库();
        let outcome = 扫(&library, &ScanOptions::named("/lib", "库"));
        let unknown = outcome
            .report
            .platforms
            .iter()
            .find(|p| p.unknown)
            .expect("有平台未知这一组");
        assert_eq!(unknown.name, UNKNOWN_PLATFORM_LABEL);
        // 散落的游戏.gba、.DS_Store、空文件.zip
        assert_eq!(unknown.files, 3);
        assert!(outcome.aggregate.platforms.contains_key(UNKNOWN_PLATFORM));
    }

    #[test]
    fn 三类构成分得开() {
        let library = 建库();
        let outcome = 扫(&library, &ScanOptions::named("/lib", "库"));
        let 取 = |category: Category| {
            outcome
                .report
                .categories
                .iter()
                .find(|c| c.category == category)
                .expect("类别都在")
                .files
        };
        // zip×4（含空文件.zip）
        assert_eq!(取(Category::TransparentContainer), 4);
        // chd
        assert_eq!(取(Category::CompressedImage), 1);
        // iso、gba
        assert_eq!(取(Category::BareFile), 2);
    }

    #[test]
    fn 疑似不该入库的四类都报得出来() {
        let library = 建库();
        let outcome = 扫(&library, &ScanOptions::named("/lib", "库"));
        let 取 = |reason: SuspectReason| {
            outcome
                .report
                .suspects
                .by_reason
                .iter()
                .find(|s| s.reason == reason)
                .expect("理由都在")
                .files
        };
        assert_eq!(取(SuspectReason::DuplicateCopy), 2, "同名同大小的两份 zip");
        assert_eq!(取(SuspectReason::Document), 1, "说明.txt");
        assert_eq!(取(SuspectReason::EmulatorBinary), 1, "epsxe.exe");
        assert_eq!(取(SuspectReason::PartialDownload), 1, "半截下载.iso.part");
        assert_eq!(取(SuspectReason::SystemJunk), 1, ".DS_Store");
        assert_eq!(outcome.report.suspects.duplicate_groups, 1);
        assert_eq!(outcome.report.suspects.duplicate_reclaimable_bytes, 2048);
    }

    #[test]
    fn 头部抽样给出各类的解析成功率() {
        let library = 建库();
        let outcome = 扫(&library, &ScanOptions::named("/lib", "库"));
        let 取 = |class: ProbeClass| {
            outcome
                .report
                .samples
                .iter()
                .find(|s| s.class == class)
                .unwrap_or_else(|| panic!("抽到了 {}", class.label()))
                .clone()
        };
        let zip = 取(ProbeClass::Zip);
        assert_eq!(zip.sampled, 4);
        assert_eq!(zip.parsed, 2, "只有两个真的是 zip");
        assert_eq!(zip.mismatched, 2);
        assert!((zip.success_rate - 50.0).abs() < f64::EPSILON);
        assert_eq!(zip.failures.len(), 2);

        assert_eq!(取(ProbeClass::Chd).parsed, 1);
        assert_eq!(取(ProbeClass::DiscImage).parsed, 1);
    }

    #[test]
    fn 抽样配额按类生效() {
        let mut library = MemFs::new();
        library.dir("/lib");
        for i in 0..50 {
            library.file(format!("/lib/FC/{i}.zip"), zip(64));
        }
        let mut options = ScanOptions::named("/lib", "库");
        options.samples_per_class = 5;
        let outcome = 扫(&library, &options);
        assert_eq!(outcome.report.samples[0].sampled, 5);
        assert_eq!(outcome.report.totals.files, 50);
    }

    #[test]
    fn 关掉抽样就一个头部都不读() {
        let library = 建库();
        let mut options = ScanOptions::named("/lib", "库");
        options.samples_per_class = 0;
        let outcome = 扫(&library, &options);
        assert!(outcome.report.samples.is_empty());
        assert_eq!(outcome.report.totals.files, 11);
    }

    #[test]
    fn 符号链接不跟随系统目录整棵跳过() {
        let library = 建库();
        let outcome = 扫(&library, &ScanOptions::named("/lib", "库"));
        assert_eq!(outcome.report.anomalies.symlinks, 1);
        assert_eq!(
            outcome.report.anomalies.skipped_system_dirs, 1,
            "__MACOSX 整棵跳过，里面的东西不计入"
        );
        assert_eq!(outcome.report.anomalies.zero_length, 1);
    }

    #[test]
    fn 读不动的地方计入报告而不是中断扫描() {
        let mut library = 建库();
        library.unreadable_content("/lib/FC/读不动.zip", 128);
        let outcome = 扫(&library, &ScanOptions::named("/lib", "库"));
        assert_eq!(outcome.report.totals.files, 12);
        let zip = outcome
            .report
            .samples
            .iter()
            .find(|s| s.class == ProbeClass::Zip)
            .expect("有 zip 这一类");
        assert_eq!(zip.unreadable, 1);
    }

    #[test]
    fn 并发数不影响结论() {
        let library = 建库();
        let mut single = ScanOptions::named("/lib", "库");
        single.jobs = Jobs::Fixed(1);
        let mut many = ScanOptions::named("/lib", "库");
        many.jobs = Jobs::Fixed(8);

        let mut a = 扫(&library, &single).aggregate;
        let mut b = 扫(&library, &many).aggregate;
        规范化(&mut a);
        规范化(&mut b);
        assert_eq!(a, b);
    }

    // ── 中立库与增量 ─────────────────────────────────────────────────────────

    #[test]
    fn 第二次扫描按三元组跳过未变的文件() {
        let library = 建库();
        let mut catalog = 新中立库();
        let options = ScanOptions::named("/lib", "库");

        let 首扫 = 扫入(&mut catalog, &library, &options);
        assert_eq!(首扫.delta.added, 11, "第一次全是新增");
        assert_eq!(首扫.delta.unchanged, 0);

        let 再扫 = 扫入(&mut catalog, &library, &options);
        assert_eq!(再扫.delta.unchanged, 11, "一个文件都没变，全跳过");
        assert_eq!(再扫.delta.added, 0);
        assert_eq!(再扫.delta.changed, 0);
        assert_eq!(再扫.delta.removed, 0);
        assert_eq!(
            再扫.report.totals, 首扫.report.totals,
            "跳过不等于漏算：报告的数字必须一模一样"
        );
    }

    #[test]
    fn 增量认得出新增删除与内容变化() {
        let mut library = 建库();
        let mut catalog = 新中立库();
        let options = ScanOptions::named("/lib", "库");
        扫入(&mut catalog, &library, &options);

        library.file("/lib/FC/新来的.zip", zip(64));
        library.remove("/lib/PSP/半截下载.iso.part");
        // 大小一模一样，只有修改时间变了——只比大小的话这条会被整个漏掉
        library.touch("/lib/PS1/最终幻想.chd", 3600);

        let 再扫 = 扫入(&mut catalog, &library, &options);
        assert_eq!(再扫.delta.added, 1, "新来的.zip");
        assert_eq!(再扫.delta.removed, 1, "半截下载.iso.part 没了");
        assert_eq!(再扫.delta.changed, 1, "最终幻想.chd 的修改时间变了");
        assert_eq!(再扫.delta.unchanged, 9, "11 个里删了 1 个、变了 1 个");
        assert_eq!(再扫.report.totals.files, 11);

        let names: Vec<&str> = 再扫
            .report
            .extensions
            .iter()
            .map(|e| e.extension.as_str())
            .collect();
        assert!(!names.contains(&"part"), "删掉的文件不该还留在报告里");
    }

    #[test]
    fn 未变的文件不再打开一次() {
        // fixture 里的 zip 都是「扩展名对、内容不是」的，一个也穿不透。穿不透的容器
        // 下次照样要再试一遍（见下一条测试），所以这里把它们排除在外单看别的文件。
        let mut 库 = 建库();
        for name in [
            "/lib/FC/超级马里奥.zip",
            "/lib/FC/魂斗罗.zip",
            "/lib/FC/备份/超级马里奥.zip",
            "/lib/空文件.zip",
        ] {
            库.remove(name);
        }
        let library = 挂钩::new(库);
        let mut catalog = 新中立库();
        let options = ScanOptions::named("/lib", "库");

        扫入(&mut catalog, &library, &options);
        let 首扫读了 = library.reads();
        assert!(首扫读了 > 0, "第一次要抽样读头部");

        扫入(&mut catalog, &library, &options);
        assert_eq!(
            library.reads(),
            首扫读了,
            "三元组没变就不该再打开任何一个文件"
        );
    }

    #[test]
    fn 穿透成功的容器不再穿第二遍穿不透的要再试() {
        use crate::testing::container::{ZipEntrySpec, zip_container};

        let mut 库 = MemFs::new();
        库.dir("/lib")
            .file(
                "/lib/FC/真的.zip",
                zip_container(&[ZipEntrySpec::stored("超级马里奥.nes", vec![7u8; 64])]),
            )
            // 扩展名说 zip，内容不是——这一个每次都要再试。
            .file("/lib/FC/假的.zip", vec![0u8; 512]);
        let library = 挂钩::new(库);
        let mut catalog = 新中立库();
        // 关掉头部抽样，读的次数里就只剩下穿透这一项。
        let mut options = ScanOptions::named("/lib", "库");
        options.samples_per_class = 0;

        扫入(&mut catalog, &library, &options);
        assert_eq!(library.reads(), 2, "两个容器各打开一次");

        扫入(&mut catalog, &library, &options);
        assert_eq!(
            library.reads(),
            3,
            "穿透成功的那个不再碰；穿不透的那个要再试一遍——\
             一次性的读失败不能被当成永久结论（ADR-0021 的道理，粒度在容器上）"
        );
    }

    #[test]
    fn 全量重扫会把每个文件重新看一遍() {
        let library = 挂钩::new(建库());
        let mut catalog = 新中立库();
        let mut options = ScanOptions::named("/lib", "库");

        扫入(&mut catalog, &library, &options);
        let 首扫读了 = library.reads();

        options.incremental = false;
        let 重扫 = 扫入(&mut catalog, &library, &options);
        assert!(library.reads() > 首扫读了, "全量重扫要重新读头部");
        assert_eq!(重扫.delta.unchanged, 0, "全量重扫不认未变");
        assert_eq!(重扫.delta.added, 11);

        // 全量重扫写下的三元组照样要能当下一次的基线，否则 `--full` 跑一次
        // 就把增量废掉了
        options.incremental = true;
        let 再增量 = 扫入(&mut catalog, &library, &options);
        assert_eq!(再增量.delta.unchanged, 11, "全量重扫之后增量要接得上");
        assert_eq!(再增量.delta.added, 0);
        assert_eq!(再增量.delta.removed, 0);
    }

    #[test]
    fn 元数据读不到的文件既不算已变也不算已删() {
        let mut library = 建库();
        library.unreadable_meta("/lib/Wii/读不到元数据.zip");
        let mut catalog = 新中立库();
        let options = ScanOptions::named("/lib", "库");

        let 首扫 = 扫入(&mut catalog, &library, &options);
        assert_eq!(首扫.delta.unreadable, 1);
        assert_eq!(首扫.report.anomalies.unreadable, 1, "报告里要单独计一栏");

        let 再扫 = 扫入(&mut catalog, &library, &options);
        assert_eq!(再扫.delta.unreadable, 1, "还是读不到");
        assert_eq!(再扫.delta.changed, 0, "读不到不算已变，否则永远重扫");
        assert_eq!(再扫.delta.removed, 0, "读不到不算已删，否则会从库里消失");
        assert_eq!(再扫.report.totals.files, 12, "它照样在库里");
    }

    #[test]
    fn 列不开的目录下面那些记录不算已删除() {
        let mut catalog = 新中立库();
        let options = ScanOptions::named("/lib", "库");
        let 首扫 = 扫入(&mut catalog, &建库(), &options);
        assert_eq!(首扫.report.totals.files, 11);

        // PS1 这一趟列不开了：它下面有 最终幻想.chd 与 模拟器/epsxe.exe
        let mut 列不开 = 建库();
        列不开.unlistable_dir("/lib/PS1");
        let 再扫 = 扫入(&mut catalog, &列不开, &options);

        assert_eq!(
            再扫.delta.removed, 0,
            "看不见不等于不存在：一次拒绝访问不许抹掉整棵子树"
        );
        assert_eq!(再扫.report.totals.files, 11, "11 个文件一个都不能少");
        assert_eq!(再扫.report.anomalies.errors, 1, "列不开这件事要报出来");
        assert!(
            catalog.contains("库/PS1/模拟器/epsxe.exe").expect("查得到"),
            "隔了一层的记录也要留着"
        );

        // 目录恢复之后，那些记录还在，也没被当成新增
        let 三扫 = 扫入(&mut catalog, &建库(), &options);
        assert_eq!(三扫.delta.added, 0);
        assert_eq!(三扫.delta.unchanged, 11);
        assert_eq!(三扫.report.anomalies.errors, 0);
    }

    #[test]
    fn 主库根都列不开时一条记录都不删() {
        let mut catalog = 新中立库();
        let options = ScanOptions::named("/lib", "库");
        扫入(&mut catalog, &建库(), &options);

        let mut 空壳 = MemFs::new();
        空壳.dir("/lib").unlistable_dir("/lib");
        let 再扫 = 扫入(&mut catalog, &空壳, &options);
        assert_eq!(再扫.delta.removed, 0);
        assert_eq!(再扫.report.totals.files, 11, "整个库不许凭空消失");
    }

    #[test]
    fn 新增的文件照样会被抽样() {
        let mut library = MemFs::new();
        library.dir("/lib");
        for i in 0..40 {
            library.file(format!("/lib/FC/{i}.zip"), zip(64));
        }
        let mut options = ScanOptions::named("/lib", "库");
        options.samples_per_class = 5;
        let mut catalog = 新中立库();

        let 首扫 = 扫入(&mut catalog, &library, &options);
        assert_eq!(
            首扫.report.samples[0].sampled, 5,
            "配额限的是这一趟开几个文件"
        );

        // 后来又添了几个。配额跨扫描累加的话，它们永远不会被看一眼。
        library.file("/lib/FC/新来的.zip", zip(64));
        let 再扫 = 扫入(&mut catalog, &library, &options);
        assert_eq!(
            再扫.report.samples[0].sampled, 6,
            "新增的文件要有机会被抽到，成功率不能冻结在首扫那批上"
        );
    }

    #[test]
    fn 大小未知不等于大小为零() {
        let mut library = 建库();
        library.unreadable_meta("/lib/Wii/读不到元数据.zip");
        let outcome = 扫(&library, &ScanOptions::named("/lib", "库"));
        assert_eq!(outcome.report.anomalies.zero_length, 1, "只有 空文件.zip");
        assert_eq!(outcome.report.anomalies.unreadable, 1);
    }

    #[test]
    fn 键是_nfc_而读盘用的仍是原始形式() {
        // 「が」的分解形：macOS 的 NTFS 驱动交出来的就是这个（ADR-0020）。
        let 分解 = "\u{304B}\u{3099}";
        let 预组合 = "\u{304C}";
        let mut library = MemFs::new();
        library
            .dir("/lib")
            .file(format!("/lib/PSP/{分解}me.iso"), iso());

        let mut catalog = 新中立库();
        let options = ScanOptions::named("/lib", "库");
        let 首扫 = 扫入(&mut catalog, &library, &options);
        assert_eq!(首扫.delta.added, 1);
        assert!(
            首扫
                .report
                .samples
                .iter()
                .any(|s| s.class == ProbeClass::DiscImage && s.parsed == 1),
            "读盘用的是系统给的原始路径，抽样必须读得到"
        );

        assert!(
            catalog
                .contains(&format!("库/PSP/{预组合}me.iso"))
                .expect("查得到"),
            "入库的键必须是 NFC 形"
        );
        assert!(
            !catalog
                .contains(&format!("库/PSP/{分解}me.iso"))
                .expect("查得到"),
            "分解形不该出现在中立库里"
        );

        // 同一块盘换台机器接，名字换成预组合形交出来——不该被判成新文件
        let mut 另一台机器 = MemFs::new();
        另一台机器
            .dir("/lib")
            .file(format!("/lib/PSP/{预组合}me.iso"), iso());
        let 再扫 = 扫入(&mut catalog, &另一台机器, &options);
        assert_eq!(再扫.delta.added, 0, "规范化之后它就是同一个文件");
        assert_eq!(再扫.delta.unchanged, 1);
        assert_eq!(再扫.delta.removed, 0);
    }

    #[test]
    fn 同一份中立库换个挂载点判为全部未变() {
        // macOS 重挂一次盘就可能从 `/Volumes/新加卷` 变成 `/Volumes/新加卷 1`。
        // 中立库跟名字走之后，同一份库会对上换了路径的同一个主库——键本来就是相对的
        // （ADR-0020），换挂载点对键没有任何影响（挂账 D16）。
        let mut catalog = 新中立库();
        let 先 = 扫入(
            &mut catalog,
            &建库于("/Volumes/盘"),
            &ScanOptions::named("/Volumes/盘", "库"),
        );
        assert!(先.delta.added > 0);

        let 再 = 扫入(
            &mut catalog,
            &建库于("/Volumes/盘 1"),
            &ScanOptions::named("/Volumes/盘 1", "库"),
        );
        assert_eq!(再.delta.added, 0, "换挂载点不该把整个库判成新增");
        assert_eq!(再.delta.removed, 0, "更不该把旧的那份判成已删");
        assert_eq!(再.delta.unchanged, 先.delta.added);
        assert_eq!(再.report.totals.files, 先.report.totals.files);
    }

    #[test]
    fn 同一个根名底下换了另一块盘时报错而不是混表_而且按根生效() {
        // 键的第一段是根名（ADR-0020），两块盘挤进同一个根名不会报错，只会静默撞车。
        let mut catalog = 新中立库();
        扫入(
            &mut catalog,
            &建库于("/Volumes/甲"),
            &ScanOptions::named("/Volumes/甲", "库"),
        );
        // 另一个根照旧扫得进来——**这道闸按根生效**，不同的根之间互不干涉。
        let 另一个根 = 扫入(
            &mut catalog,
            &建库于("/Volumes/丙"),
            &ScanOptions::named("/Volumes/丙", "第二个根"),
        );
        assert!(
            另一个根.delta.added > 0,
            "换个根名就是新的一个根，拦都不该拦"
        );

        let mut 另一个主库 = MemFs::new();
        另一个主库
            .dir("/Volumes/乙")
            .file("/Volumes/乙/漫画/第一话.zip", zip(64))
            .file("/Volumes/乙/照片/去年.jpg", vec![0u8; 64]);
        let 错 = scan(
            &另一个主库,
            &mut catalog,
            &ScanOptions::named("/Volumes/乙", "库"),
            &Handle::new(),
        )
        .expect_err("顶层条目全不一样，该拦下来");
        let ScanError::DifferentLibrary {
            name,
            recorded,
            current,
            common,
            ..
        } = &错
        else {
            panic!("该是 DifferentLibrary，实际是 {错:?}");
        };
        assert_eq!(name, "库");
        assert_eq!(recorded, "/Volumes/甲");
        assert_eq!(current, "/Volumes/乙");
        assert_eq!(*common, 0);
        assert!(错.to_string().contains("换个根名"), "得说清出路：{错}");
        // 拦下来那一趟一个字都没写：第二个根那一支原样在。
        assert!(
            catalog
                .contains("第二个根/FC/超级马里奥.zip")
                .expect("查得到")
        );
    }

    #[test]
    fn 顶层还对得上就认成同一个主库() {
        // 主库的顶层本来就会增删几个目录，卡太严会把正常的换挂载点也拦下来。
        let mut catalog = 新中立库();
        扫入(
            &mut catalog,
            &建库于("/Volumes/甲"),
            &ScanOptions::named("/Volumes/甲", "库"),
        );

        let mut 少了一个平台 = 建库于("/Volumes/乙");
        少了一个平台.remove("/Volumes/乙/PSP");
        少了一个平台.file("/Volumes/乙/SFC/塞尔达.zip", zip(128));
        let 再 = 扫入(
            &mut catalog,
            &少了一个平台,
            &ScanOptions::named("/Volumes/乙", "库"),
        );
        assert!(再.delta.added > 0, "新加的那个平台是新增");
        assert!(再.delta.unchanged > 0, "没动的那些还是未变");
    }

    #[test]
    fn 根还记在原地而盘不在位时一趟扫描不许抹掉整个根() {
        // 挂载点目录永远都在：Linux 的 `/mnt/x` 是先建出来的，macOS 卸盘之后
        // `/Volumes/x` 也偶尔残留一个空目录。**路径一个字没变**，于是遍历顺利跑完、
        // 收尾把整个根当成「这次没见到」抹掉——看不见不等于不存在（ADR-0021）。
        let mut catalog = 新中立库();
        let options = ScanOptions::named("/Volumes/主库", "主库");
        let 首扫 = 扫入(&mut catalog, &建库于("/Volumes/主库"), &options);
        let 扫到的文件 = 首扫.report.totals.files;
        assert!(扫到的文件 > 0);

        // 盘拔了：挂载点还在，里面什么都没有。
        let mut 空壳 = MemFs::new();
        空壳.dir("/Volumes/主库");
        let 错 = scan(&空壳, &mut catalog, &options, &Handle::new())
            .expect_err("盘不在位该拦下来，而不是把整个根当成删光了");
        let ScanError::RootNotInPlace {
            name,
            path,
            present,
            recorded_count,
        } = &错
        else {
            panic!("该是 RootNotInPlace，实际是 {错:?}");
        };
        assert_eq!(name, "主库");
        assert_eq!(path, "/Volumes/主库");
        assert_eq!(*present, 0, "这一层什么都没有");
        assert!(*recorded_count > 0);
        // 措辞要让用户去插盘，而不是去换根名——那是另一档（`DifferentLibrary`）的出路。
        assert!(
            错.to_string().contains("插上那块盘再扫"),
            "得说清出路：{错}"
        );
        assert!(!错.to_string().contains("换个根名"), "别把人指错路：{错}");

        // 拦下来那一趟中立库一个字都没动。
        assert_eq!(
            catalog.root_stats("主库").expect("数得出").files,
            扫到的文件
        );
        assert!(
            catalog.contains("主库/FC/超级马里奥.zip").expect("查得到"),
            "整个根不许凭空消失"
        );

        // 顺手把这个根改指到另一个还没挂上的挂载点：**指到一个空目录上去**从来不是
        // 「换了另一块盘」，出路照旧是插盘。
        let mut 另一个空壳 = MemFs::new();
        另一个空壳.dir("/Volumes/主库 1");
        let 换个挂载点 = scan(
            &另一个空壳,
            &mut catalog,
            &ScanOptions::named("/Volumes/主库 1", "主库"),
            &Handle::new(),
        )
        .expect_err("空的挂载点照样拦");
        assert!(
            matches!(换个挂载点, ScanError::RootNotInPlace { .. }),
            "该是 RootNotInPlace，实际是 {换个挂载点:?}"
        );
    }

    #[test]
    fn 用户真把根清空了就先移除这个根再加回来() {
        // 「盘不在位」拦下来之后得留一条出路，否则用户真清空了一个根就再也扫不动它。
        // 出路是**移除这个根**：那是唯一一处工具会主动丢掉扫描结果的地方，由用户按下、
        // 事先看得见会去掉多少变体，而不是一趟扫描替他决定。
        let mut catalog = 新中立库();
        let options = ScanOptions::named("/主库", "主库");
        扫入(&mut catalog, &建库于("/主库"), &options);

        let mut 清空了 = MemFs::new();
        清空了.dir("/主库");
        scan(&清空了, &mut catalog, &options, &Handle::new()).expect_err("先拦一道");

        catalog.remove_root("主库").expect("移得掉");
        let 再扫 = 扫入(&mut catalog, &清空了, &options);
        assert_eq!(再扫.report.totals.files, 0, "加回来是个空的根，扫得动");
        assert_eq!(
            再扫.delta.removed, 0,
            "库里已经没有它那一支了，不该再数一遍"
        );
    }

    #[test]
    fn 顶层还认得出一条就放行而不看比例() {
        // 判据是绝对的「一条都不剩」而不是重叠比例：顶层只有三个目录、用户合法删掉
        // 其中两个时，重叠率掉到 1/3、远低于阈值，可盘明明在位——拿阈值去拦用户
        // 自己动的手是误伤。
        let mut library = MemFs::new();
        library
            .dir("/lib")
            .file("/lib/FC/超级马里奥.zip", zip(64))
            .file("/lib/PS1/最终幻想.chd", chd())
            .file("/lib/PSP/游戏.iso", iso());
        let mut catalog = 新中立库();
        let options = ScanOptions::named("/lib", "主库");
        扫入(&mut catalog, &library, &options);

        library
            .remove("/lib/PS1/最终幻想.chd")
            .remove("/lib/PS1")
            .remove("/lib/PSP/游戏.iso")
            .remove("/lib/PSP");
        let 再扫 = 扫入(&mut catalog, &library, &options);
        assert_eq!(再扫.delta.removed, 2, "用户自己删的照常收掉");
        assert_eq!(再扫.report.totals.files, 1);
    }

    #[test]
    fn 两个根的变体互不覆盖各自的键带得出自己的根名() {
        // 两块盘上**同名同大小**的东西：只按相对路径当键的话它们是同一条记录，
        // 一条静默覆盖另一条，而中立库是事实来源（ADR-0001）。
        let mut catalog = 新中立库();
        let 甲 = 扫入(
            &mut catalog,
            &建库于("/Volumes/甲"),
            &ScanOptions::named("/Volumes/甲", "主库"),
        );
        let 乙 = 扫入(
            &mut catalog,
            &建库于("/Volumes/乙"),
            &ScanOptions::named("/Volumes/乙", "元数据库"),
        );

        assert_eq!(
            乙.delta.added, 甲.delta.added,
            "第二个根一条都不该被认成已有"
        );
        assert_eq!(乙.delta.unchanged, 0);
        assert_eq!(乙.delta.removed, 0, "扫乙盘绝不许动甲盘那一支");

        for 根 in ["主库", "元数据库"] {
            assert!(
                catalog
                    .contains(&format!("{根}/FC/超级马里奥.zip"))
                    .expect("查得到"),
                "{根} 那一支得独立存在"
            );
        }
        // 两个根加起来才是这份库的全部。
        assert_eq!(
            乙.report.totals.files,
            甲.report.totals.files * 2,
            "报告数的是整份中立库"
        );
        assert_eq!(catalog.roots().expect("读得出").len(), 2);
    }

    #[test]
    fn 重扫一个根不会把另一个根的记录当成已删() {
        // `sweep` 删的是「这次没见到的」，而一趟只扫一个根——不划范围的话，
        // 扫一遍甲盘会把乙盘整支抹掉。
        let mut catalog = 新中立库();
        扫入(
            &mut catalog,
            &建库于("/Volumes/甲"),
            &ScanOptions::named("/Volumes/甲", "主库"),
        );
        let 乙 = 扫入(
            &mut catalog,
            &建库于("/Volumes/乙"),
            &ScanOptions::named("/Volumes/乙", "元数据库"),
        );
        let 全部 = 乙.report.totals.files;

        let 重扫甲 = 扫入(
            &mut catalog,
            &建库于("/Volumes/甲"),
            &ScanOptions::named("/Volumes/甲", "主库"),
        );
        assert_eq!(重扫甲.delta.removed, 0, "乙盘那一支一条都不许删");
        assert_eq!(重扫甲.report.totals.files, 全部);
        assert!(
            catalog
                .contains("元数据库/FC/超级马里奥.zip")
                .expect("查得到")
        );
    }

    #[test]
    fn 移除一个根时说得出会去掉多少变体并且只去掉它自己那一支() {
        let mut catalog = 新中立库();
        扫入(
            &mut catalog,
            &建库于("/Volumes/甲"),
            &ScanOptions::named("/Volumes/甲", "主库"),
        );
        扫入(
            &mut catalog,
            &建库于("/Volumes/乙"),
            &ScanOptions::named("/Volumes/乙", "元数据库"),
        );
        let 会去掉 = catalog.root_stats("元数据库").expect("数得出").variants;
        assert!(会去掉 > 0);

        let 去掉了 = catalog.remove_root("元数据库").expect("移得掉");
        assert_eq!(去掉了, 会去掉, "按下去之前看见的那个数，就是真去掉的那个数");
        assert_eq!(
            catalog.root_stats("元数据库").expect("数得出"),
            crate::catalog::RootStats::default(),
            "变体与条目一条都不许剩下——剩下的会在浏览屏上变成指不着文件的幽灵行"
        );
        assert!(
            !catalog
                .contains("元数据库/FC/超级马里奥.zip")
                .expect("查得到")
        );
        assert!(catalog.contains("主库/FC/超级马里奥.zip").expect("查得到"));
        assert_eq!(catalog.roots().expect("读得出").len(), 1);
    }

    #[test]
    fn 盘没挂上时这个根的上次结果仍然看得见() {
        // 结果住在中立库里而不是跟着盘走（ADR-0009 那条道理）。
        let mut catalog = 新中立库();
        扫入(&mut catalog, &建库(), &ScanOptions::named("/lib", "库"));
        // 这里连 MemFs 都没有了，照样读得出上次扫了什么。
        let 根 = catalog.root("库").expect("读得出").expect("有这个根");
        let scan = 根.scan.expect("扫过一趟");
        assert!(!scan.interrupted);
        assert_eq!(scan.entries, 11, "上次扫到多少个文件");
        assert!(scan.at > 0, "上次什么时候扫的");
        assert_eq!(catalog.root_stats("库").expect("数得出").files, 11);
    }

    #[test]
    fn 体检报告能在主库不在位时从中立库出() {
        let mut catalog = 新中立库();
        let 扫出来的 = {
            let library = 建库();
            扫入(&mut catalog, &library, &ScanOptions::named("/lib", "库")).report
        };
        // 盘拔了：这里连 MemFs 都没有了，中立库照样出得来
        let mut 库里的 = catalog
            .aggregate(&Limits::default(), &Manifest::builtin())
            .expect("从中立库折得出统计");
        规范化(&mut 库里的);
        let 报告 = HealthReport::build(&库里的, &catalog.report_meta().expect("元信息读得出来"));

        assert_eq!(报告.totals, 扫出来的.totals);
        assert_eq!(报告.platforms, 扫出来的.platforms);
        assert_eq!(报告.extensions, 扫出来的.extensions);
        assert_eq!(报告.suspects, 扫出来的.suspects);
        assert_eq!(报告.samples, 扫出来的.samples, "抽样结论也留在中立库里");
        assert_eq!(报告.delta, None, "没扫盘就没有增量可说");
    }

    #[test]
    fn 中立库落在本机而不是主库里() {
        let mut catalog = Catalog::open(
            &crate::testing::temp_dir("catalog")
                .path()
                .join("库.sqlite3"),
        )
        .expect("能开中立库");
        let 之前 = 扫入(&mut catalog, &建库(), &ScanOptions::named("/lib", "库")).report;
        drop(catalog);

        // 关掉再打开：重启工具后结论不丢
        let path = crate::testing::temp_dir("catalog2")
            .path()
            .join("库.sqlite3");
        let mut catalog = Catalog::open(&path).expect("能开中立库");
        扫入(&mut catalog, &建库(), &ScanOptions::named("/lib", "库"));
        drop(catalog);
        let catalog = Catalog::open(&path).expect("能再打开");
        let aggregate = catalog
            .aggregate(&Limits::default(), &Manifest::builtin())
            .expect("读得出来");
        assert_eq!(aggregate.totals.files, 之前.totals.files);
        assert_eq!(aggregate.totals.bytes, 之前.totals.bytes);
        assert!(path.exists(), "中立库是本机上的一个文件");
    }

    /// 一个能数「开了几次文件」、也能在列目录时挂钩子的替身。
    struct 挂钩<'a> {
        inner: MemFs,
        hook: Box<dyn Fn(&Path) + Send + Sync + 'a>,
        reads: AtomicUsize,
    }

    impl 挂钩<'static> {
        fn new(inner: MemFs) -> Self {
            Self {
                inner,
                hook: Box::new(|_: &Path| {}),
                reads: AtomicUsize::new(0),
            }
        }

        fn reads(&self) -> usize {
            self.reads.load(Ordering::SeqCst)
        }
    }

    impl LibraryFs for 挂钩<'_> {
        fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
            self.inner.canonicalize(path)
        }

        fn read_dir(&self, dir: &Path) -> std::io::Result<Vec<DirEntry>> {
            (self.hook)(dir);
            self.inner.read_dir(dir)
        }

        fn read_head(&self, file: &Path, limit: usize) -> std::io::Result<Vec<u8>> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            self.inner.read_head(file, limit)
        }

        fn read_tail(&self, file: &Path, limit: usize) -> std::io::Result<Vec<u8>> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            self.inner.read_tail(file, limit)
        }

        fn open(&self, file: &Path) -> std::io::Result<Box<dyn crate::fs::ReadSeek + '_>> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            self.inner.open(file)
        }
    }

    #[test]
    fn 中断再续跑与一次扫完结论相同() {
        let workspace = crate::testing::temp_dir("scan");
        let mut options = ScanOptions::named("/lib", "库");
        options.jobs = Jobs::Fixed(1);
        options.checkpoint = Some(断点选项(workspace.path(), "库"));
        let checkpoint = options.checkpoint.as_ref().expect("有断点").path.clone();

        // 一次扫完，作为对照
        let mut 对照 = 扫(&建库(), &options).aggregate;

        // 扫到第三个目录时按下中断
        let task = Handle::new();
        let cancel = task.cancel().clone();
        let seen = AtomicUsize::new(0);
        let 中断的库 = 挂钩 {
            inner: 建库(),
            hook: Box::new(|_: &Path| {
                if seen.fetch_add(1, Ordering::SeqCst) >= 2 {
                    cancel.cancel();
                }
            }),
            reads: AtomicUsize::new(0),
        };
        let mut catalog = 新中立库();
        let first = scan(&中断的库, &mut catalog, &options, &task).expect("中断也算正常返回");
        assert!(first.interrupted, "应当报告被中断");
        assert!(checkpoint.exists(), "断点应当落盘");
        assert!(
            first.report.totals.files < 对照.totals.files,
            "中断时只扫了一部分"
        );
        assert_eq!(first.delta.removed, 0, "中断的扫描一条记录都不许删");

        // 续跑，直到扫完
        let mut 续跑结果 =
            scan(&建库(), &mut catalog, &options, &Handle::new()).expect("续跑不该失败");
        assert!(续跑结果.report.resumed, "应当认出这是续跑");
        assert!(!续跑结果.interrupted);
        assert!(!checkpoint.exists(), "扫完后断点应当被清掉");

        规范化(&mut 对照);
        规范化(&mut 续跑结果.aggregate);
        assert_eq!(续跑结果.aggregate, 对照);
    }

    #[test]
    fn 中断落在目录中间时整个目录重扫而不是丢掉半个() {
        let workspace = crate::testing::temp_dir("scan");
        let mut options = ScanOptions::named("/lib", "库");
        options.jobs = Jobs::Fixed(1);
        options.checkpoint = Some(断点选项(workspace.path(), "库"));

        let mut 对照 = 扫(&建库(), &options).aggregate;

        // PSP 是根目录之后第一个被扫的。钩子先等一会儿再按下中断：等这一下是有意的，
        // 它让协调线程先把上一个目录并完、进到等结果的状态，中断才落下——于是
        // 「半个目录的结果被送到协调线程手上」这条最危险的路径必然被走到，
        // 而真实的 Ctrl-C 正是这个时序。
        let task = Handle::new();
        let cancel = task.cancel().clone();
        let 中断的库 = 挂钩 {
            inner: 建库(),
            hook: Box::new(|dir: &Path| {
                if dir == Path::new("/lib/PSP") {
                    thread::sleep(Duration::from_millis(25));
                    cancel.cancel();
                }
            }),
            reads: AtomicUsize::new(0),
        };
        let mut catalog = 新中立库();
        let first = scan(&中断的库, &mut catalog, &options, &task).expect("中断也算正常返回");
        assert!(first.interrupted);

        let mut 续跑结果 =
            scan(&建库(), &mut catalog, &options, &Handle::new()).expect("续跑不该失败");
        assert!(续跑结果.report.resumed);
        规范化(&mut 对照);
        规范化(&mut 续跑结果.aggregate);
        assert_eq!(
            续跑结果.aggregate, 对照,
            "被打断的那个目录必须整个重扫，一个文件都不能少"
        );
    }

    #[test]
    fn 比中立库旧的断点不会被续跑() {
        let workspace = crate::testing::temp_dir("scan");
        let mut options = ScanOptions::named("/lib", "库");
        options.jobs = Jobs::Fixed(1);
        options.checkpoint = Some(断点选项(workspace.path(), "库"));
        let checkpoint = options.checkpoint.as_ref().expect("有断点").path.clone();

        // 中断一次，留下断点
        let task = Handle::new();
        task.stop();
        let mut catalog = 新中立库();
        let first = scan(&建库(), &mut catalog, &options, &task).expect("中断也算正常返回");
        assert!(first.interrupted);
        assert!(checkpoint.exists());

        // 中间插一次不写断点的完整扫描：中立库整个被刷新了一遍
        let mut 不写断点 = ScanOptions::named("/lib", "库");
        不写断点.jobs = Jobs::Fixed(1);
        扫入(&mut catalog, &建库(), &不写断点);

        // 那份断点现在比中立库旧。照它续跑会把上一次完整扫描的记录全删掉。
        let 再扫 = scan(&建库(), &mut catalog, &options, &Handle::new()).expect("扫描不该失败");
        assert!(!再扫.report.resumed, "过期的断点不该被当成续跑");
        assert_eq!(再扫.report.totals.files, 11, "一个文件都不许丢");
        assert_eq!(再扫.delta.removed, 0);
    }

    #[test]
    fn 断点落在主库内直接拒绝开工() {
        let library = 建库();
        let mut options = ScanOptions::named("/lib", "库");
        options.checkpoint = Some(CheckpointOptions {
            path: PathBuf::from("/lib/.romcat/checkpoint.json"),
            interval: Duration::ZERO,
            resume: false,
        });
        let err = scan(&library, &mut 新中立库(), &options, &Handle::new()).expect_err("必须拒绝");
        assert!(matches!(err, ScanError::WritesInsideLibrary { .. }));
    }

    /// 这道闸曾经在一切 **merged-usr** 的发行版上静默失效：那里 `/lib` 是指向
    /// `usr/lib` 的符号链接，而闸的两边问的是两套文件系统——扫描根走
    /// `LibraryFs::canonicalize` 折出来还是 `/lib`，断点走真文件系统折出来成了
    /// `/usr/lib/…`，`is_inside` 只比前缀，于是判成「断点不在主库里」，扫描照跑
    /// （挂单 Q8）。
    ///
    /// 这里在临时目录里造出同一副形状——一个指向别处的根——好让这条回归在任何机器上
    /// 都成立，而不是碰运气看跑测试的这台机器上恰好有没有 `/lib`。
    #[cfg(unix)]
    #[test]
    fn 根在真盘上是符号链接时断点落在主库内照样拦得下() {
        let temp = crate::testing::temp_dir("scan-链接根");
        let 真身 = temp.path().join("usr").join("lib");
        let 链接 = temp.path().join("lib");
        std::fs::create_dir_all(&真身).expect("能建真身目录");
        std::os::unix::fs::symlink(&真身, &链接).expect("能建符号链接");

        // 主库这一侧只认 `链接` 这条路径——`LibraryFs` 不跟随符号链接。
        let library = 建库于(&链接.to_string_lossy());
        let mut options = ScanOptions::named(&链接, "库");
        options.jobs = Jobs::Fixed(1);
        options.checkpoint = Some(CheckpointOptions {
            path: 链接.join(".romcat").join("checkpoint.json"),
            interval: Duration::ZERO,
            resume: false,
        });

        let err = scan(&library, &mut 新中立库(), &options, &Handle::new()).expect_err("必须拒绝");
        assert!(
            matches!(err, ScanError::WritesInsideLibrary { .. }),
            "断点写在主库里，闸必须响；实际是 {err}"
        );
        assert!(
            !链接.join(".romcat").exists(),
            "主库只读（ADR-0004）：连断点的那个目录都不许建出来"
        );
    }

    /// 断点写不进去要**报错**，不是挂住。
    ///
    /// 协调这一头带着 `?` 跳出 `thread::scope` 而没人 `close` 队列时，工作线程会永远
    /// 卡在 `Queue::pop` 的条件变量上，`scope` 又非等它们不可——一次写失败于是变成
    /// 整个进程挂死（挂单 Q8 的第二个症状）。这条测试自己带表：真挂住的话它超时失败，
    /// 而不是把整趟门禁拖住。
    #[test]
    fn 断点写不进去时报错而不是挂住() {
        let temp = crate::testing::temp_dir("scan-断点写不进去");
        // 拿一个**普通文件**当断点的上级目录：`create_dir_all` 到这一级必然失败，
        // 而这条路径在主库之外，闸不会抢在前面把它拦下来。
        let 挡路的文件 = temp.path().join("这是个文件");
        std::fs::write(&挡路的文件, b"x").expect("能写临时文件");
        let 断点 = 挡路的文件.join("checkpoint.json");

        let (tx, rx) = mpsc::channel();
        let 跑 = thread::spawn(move || {
            let mut options = ScanOptions::named("/lib", "库");
            options.jobs = Jobs::Fixed(1);
            options.checkpoint = Some(CheckpointOptions {
                path: 断点,
                // 每转一圈都存一次：写失败要在第一圈就撞上，不必等 15 秒。
                interval: Duration::ZERO,
                resume: false,
            });
            let result = scan(&建库(), &mut 新中立库(), &options, &Handle::new());
            let _ = tx.send(result.err().map(|error| error.to_string()));
        });

        let err = rx
            .recv_timeout(Duration::from_secs(30))
            .expect("扫描挂住了：断点写不进去该停下并说清，不该等在这里")
            .expect("断点写不进去必须报错，不能当没事发生");
        跑.join().expect("跑测试的那个线程正常结束");
        assert!(
            err.contains("断点文件读写失败"),
            "错误得说清是断点写不进去；实际是 {err}"
        );
    }

    #[test]
    fn 扫描根不存在时报得明白() {
        let library = 建库();
        let err = scan(
            &library,
            &mut 新中立库(),
            &ScanOptions::named("/不存在", "库"),
            &Handle::new(),
        )
        .expect_err("必须报错");
        assert!(matches!(err, ScanError::Root { .. }));
    }

    #[test]
    fn 报告能渲染成文本也能存成_json() {
        let library = 建库();
        let outcome = 扫(&library, &ScanOptions::named("/lib", "库"));
        let text = outcome.report.render_text();
        assert!(text.contains("库体检报告"));
        assert!(text.contains("透明容器"));
        assert!(text.contains(UNKNOWN_PLATFORM_LABEL));
        assert!(text.contains("这次扫描的增量"));

        let json = serde_json::to_string(&outcome.report).expect("能序列化");
        let back: crate::report::HealthReport = serde_json::from_str(&json).expect("能读回");
        assert_eq!(back, outcome.report);
    }

    /// 一个中途停下的甲盘：断点落在盘上，队列里还剩东西。
    fn 中断一趟(catalog: &mut Catalog, options: &ScanOptions, root: &str) -> ScanOutcome {
        let task = Handle::new();
        let cancel = task.cancel().clone();
        let seen = AtomicUsize::new(0);
        let 中断的库 = 挂钩 {
            inner: 建库于(root),
            hook: Box::new(|_: &Path| {
                if seen.fetch_add(1, Ordering::SeqCst) >= 2 {
                    cancel.cancel();
                }
            }),
            reads: AtomicUsize::new(0),
        };
        let outcome = scan(&中断的库, catalog, options, &task).expect("中断也算正常返回");
        assert!(outcome.interrupted, "这一趟本该被中断");
        outcome
    }

    #[test]
    fn 扫过另一个根之后前一个根的断点还认得出是续跑() {
        // 主库是一组根：扫甲盘中途停下 → 整整扫完一遍乙盘 → 回来续甲盘。
        // 甲那一支从中断之后一个字节都没动过，断点一点都不旧;
        // 「扫描代号相邻」这条身份判据主库变成一组根之后不再成立，
        // 拿它当判据会把甲整根重扫（真机 27 分钟）。
        let workspace = crate::testing::temp_dir("scan");
        let mut catalog = 新中立库();

        let mut 甲 = ScanOptions::named("/Volumes/甲", "甲盘");
        甲.jobs = Jobs::Fixed(1);
        甲.checkpoint = Some(断点选项(workspace.path(), "甲盘"));
        let 中断 = 中断一趟(&mut catalog, &甲, "/Volumes/甲");
        assert!(中断.report.totals.files < 11, "中断时只扫了一部分");

        let mut 乙 = ScanOptions::named("/Volumes/乙", "乙盘");
        乙.jobs = Jobs::Fixed(1);
        乙.checkpoint = Some(断点选项(workspace.path(), "乙盘"));
        let 扫乙 = 扫入(&mut catalog, &建库于("/Volumes/乙"), &乙);
        assert!(!扫乙.interrupted, "乙盘这一趟完整跑完");

        let 续甲 = 扫入(&mut catalog, &建库于("/Volumes/甲"), &甲);
        assert!(续甲.report.resumed, "中间扫过别的根不该让甲盘的断点作废");
        assert!(!续甲.interrupted);
        assert_eq!(
            catalog.root_stats("甲盘").expect("数得出").files,
            11,
            "续跑接着扫完，甲盘一个文件都不许少"
        );
        assert_eq!(续甲.report.totals.files, 22, "两个根合起来才是这份库的全部");
    }

    #[test]
    fn 扫过另一个根不清掉前一个根的遍历批注() {
        // 报告里的异常是从遍历的批注折出来的。开一次新扫描把**全部**批注清掉的话，
        // 扫完乙盘的报告会说甲盘那个列不开的目录不存在——而它的子树还被
        // `keep_subtree` 保在库里（ADR-0021）。报告与中立库从此各说各话。
        let mut catalog = 新中立库();
        let mut 甲 = 建库于("/Volumes/甲");
        甲.unlistable_dir("/Volumes/甲/PS1/进不去");
        let 扫甲 = 扫入(
            &mut catalog,
            &甲,
            &ScanOptions::named("/Volumes/甲", "甲盘"),
        );
        assert_eq!(扫甲.report.anomalies.errors, 1, "甲盘那个目录列不开");

        let 扫乙 = 扫入(
            &mut catalog,
            &建库于("/Volumes/乙"),
            &ScanOptions::named("/Volumes/乙", "乙盘"),
        );
        assert_eq!(扫乙.report.totals.files, 22, "报告数的是整份中立库");
        assert_eq!(
            扫乙.report.anomalies.errors, 1,
            "扫乙盘不许让甲盘那条「列不开」从报告里消失"
        );
        assert!(
            扫乙
                .report
                .anomalies
                .error_examples
                .iter()
                .any(|例| 例.contains("进不去")),
            "说得出是哪个目录：{:?}",
            扫乙.report.anomalies.error_examples
        );
    }

    #[test]
    fn 一个根移除再加回来之后旧断点不许被当成续跑() {
        // 移除一个根是「它在中立库里的一切都不算数了」。同名同路径加回来是**新的一个根**，
        // 而工作目录里那份断点是按根名取的、还躺在原地：认它当续跑就只扫 `pending`
        // 那一半，收尾还什么都删不到——扫完报「完整」却少文件。
        let workspace = crate::testing::temp_dir("scan");
        let mut catalog = 新中立库();
        let mut options = ScanOptions::named("/Volumes/甲", "甲盘");
        options.jobs = Jobs::Fixed(1);
        options.checkpoint = Some(断点选项(workspace.path(), "甲盘"));
        let checkpoint = options.checkpoint.as_ref().expect("有断点").path.clone();

        中断一趟(&mut catalog, &options, "/Volumes/甲");
        assert!(checkpoint.exists(), "断点落在工作目录里");

        catalog.remove_root("甲盘").expect("移得掉");
        assert_eq!(
            catalog.root_stats("甲盘").expect("数得出"),
            crate::catalog::RootStats::default(),
            "移除之后这个根下面一条记录都不剩"
        );

        let 再扫 = 扫入(&mut catalog, &建库于("/Volumes/甲"), &options);
        assert!(
            !再扫.report.resumed,
            "加回来的是新的一个根，旧断点不许接着跑"
        );
        assert!(!再扫.interrupted);
        assert!(再扫.shaped);
        assert_eq!(
            再扫.report.totals.files, 11,
            "扫完就得与一次扫完的对照相等，一个文件都不许少"
        );
        assert_eq!(catalog.root_stats("甲盘").expect("数得出").files, 11);
    }
}
