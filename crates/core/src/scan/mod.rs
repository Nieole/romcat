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
//! 平台由目录给出（ADR-0011）：文件的键相对扫描根的第一级目录名就是平台目录。认不出
//! 平台的照常计入报告——平台未知不构成跳过的理由。

pub mod aggregate;
pub mod checkpoint;
pub mod probe;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use std::{fs, thread};

use crate::catalog::{Baseline, Catalog, CatalogError, EntryRecord, ScanDelta, Traversal, Verdict};
use crate::classify;
use crate::container::{self, ContainerKind, Penetration};
use crate::fs::{DirEntry, EntryKind, EntryMeta, LibraryFs};
use crate::header::{self, ProbeClass};
use crate::path;
use crate::report::{HealthReport, ReportMeta};

use aggregate::{Aggregate, Limits, SampleResult};
use checkpoint::{Checkpoint, CheckpointError};

/// 认「还是同一个主库」需要的顶层条目重合比例。
///
/// 半数是个务实的线：主库的顶层本来就会增删几个目录，卡太严会把正常的换挂载点也拦下来；
/// 而两个真正不同的主库要凑够半数同名的顶层条目，得是刻意造出来的巧合。
const SAME_LIBRARY_OVERLAP: f64 = 0.5;

/// 比对顶层条目时最多取几条。真库的顶层是 73 条，取 512 条绰绰有余。
const TOP_LEVEL_SAMPLE: usize = 512;

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
    /// 同一份中立库底下换了另一个主库。
    ///
    /// 只有 `--library` 给中立库起了名字才可能出现：名字一样、主库不一样。
    #[error(
        "中立库 {catalog} 记的主库是 {recorded}，这次要扫的是 {current}——\
         顶层条目只有 {common}/{recorded_count} 条对得上，两边多半不是同一个主库。\
         中立库的键是相对主库根的路径（ADR-0020），两个主库挤进同一份库会直接撞车。\
         换一个 --library 名字，或者删掉那份中立库重扫一遍"
    )]
    DifferentLibrary {
        /// 中立库文件。
        catalog: String,
        /// 库里记着的主库根。
        recorded: String,
        /// 这次要扫的主库根。
        current: String,
        /// 顶层条目对得上几条。
        common: usize,
        /// 库里记着的顶层条目共几条。
        recorded_count: usize,
    },
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
    /// 主库根目录。
    pub root: PathBuf,
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
    /// 穿透**透明容器**：零解压读出 zip 与 7z 内部每个文件的 CRC-32、大小与名字。
    ///
    /// 库里 91.1% 的容量装在透明容器里，不穿透的话报告只看得见「这里有一个 3GB 的容器」，
    /// 后面的识别也无从谈起（ADR-0014）。关掉它只在一种场合有意义：怀疑穿透本身
    /// 有问题，想先把遍历跑通。
    pub penetrate_containers: bool,
}

impl ScanOptions {
    /// 用默认参数扫描 `root`。
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            jobs: Jobs::Adaptive,
            samples_per_class: 32,
            limits: Limits::default(),
            checkpoint: None,
            incremental: true,
            penetrate_containers: true,
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
}

/// 扫一遍主库，把结论写进中立库。
///
/// # Errors
/// 扫描根打不开、断点落在主库内、断点读写失败或中立库读写失败时返回错误。
pub fn scan(
    library: &dyn LibraryFs,
    catalog: &mut Catalog,
    options: &ScanOptions,
    cancel: &CancelToken,
) -> Result<ScanOutcome, ScanError> {
    let started = Instant::now();
    let root = library
        .canonicalize(&options.root)
        .map_err(|source| ScanError::Root {
            path: path::display(&options.root),
            source,
        })?;

    // 断点是这条流程里唯一写进文件系统的东西，它必须落在主库之外。比较前两边都化成
    // 绝对形态，否则 `/var` 与 `/private/var` 这类链接会让守卫形同虚设。
    let mut guarded: Vec<(&'static str, &Path)> = Vec::new();
    if let Some(config) = &options.checkpoint {
        guarded.push(("断点文件", &config.path));
    }
    if let Some(file) = catalog.file() {
        guarded.push(("中立库", file));
    }
    for (what, target) in guarded {
        if path::is_inside(&root, &path::normalize_existing(target)) {
            return Err(ScanError::WritesInsideLibrary {
                what,
                path: path::display(target),
            });
        }
    }

    // 名字一样、主库不一样的话，两边的记录会挤进同一张表——键是相对的（ADR-0020），
    // 撞车之后没法分开。开扫之前拦下来。
    guard_same_library(library, catalog, &root)?;

    // 并发按介质定而不是按 CPU 数猜（挂账 D9）。`-j` 点了名就照办，一个目录都不多读。
    let measured = match options.jobs {
        Jobs::Fixed(_) => None,
        Jobs::Adaptive => Some(probe::probe(library, &root, cancel)),
    };
    let resolved_jobs = match options.jobs {
        Jobs::Fixed(jobs) => jobs.max(1),
        Jobs::Adaptive => measured.map_or_else(default_jobs, |probe| probe.jobs),
    };

    let mut start = load_start_state(options, &root, catalog)?;
    let mut traversal = Traversal {
        scan: start.scan,
        root: path::display(&root),
        elapsed_ms: elapsed(start.elapsed_base, started),
        jobs: resolved_jobs,
        samples_per_class: options.samples_per_class,
        penetrated_containers: options.penetrate_containers,
        interrupted: true,
        resumed: start.resumed,
    };
    if !start.resumed {
        catalog.begin_scan(start.scan)?;
    }
    // 先把这次扫描的行占上，代号才不会因为进程半路被杀而被下次重用。
    catalog.save_traversal(&traversal)?;
    // 主库根自己也是一条记录，键是空串——「扫过几个目录」是从表里数出来的。
    catalog.write(start.scan, &[root_record(&root)])?;

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
            let root = &root;
            let baseline = &baseline;
            scope.spawn(move || {
                while let Some(dir) = queue.pop() {
                    let result = process_dir(library, root, dir, options, budget, baseline, cancel);
                    if tx.send(result).is_err() {
                        break;
                    }
                }
            });
        }
        drop(results_tx);

        let mut last_save = Instant::now();
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
                    let done = merge(catalog, start.scan, &mut progress, result)?;
                    queue.finish_and_push(done);
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
        queue.close();
        Ok(interrupted)
    })?;

    traversal.elapsed_ms = elapsed(start.elapsed_base, started);
    traversal.interrupted = interrupted;
    catalog.write(start.scan, &progress.records)?;
    progress.records.clear();

    // 删除只在完整扫完一遍之后判。中断的扫描没走完整个库，「这次没见到」不等于
    // 「不存在」——那时候清一遍会把还没扫到的那半个库当成已删除抹掉。
    if !interrupted {
        progress.delta.removed = catalog.sweep(start.scan)?;
    }
    catalog.save_traversal(&traversal)?;
    let checkpoint_path =
        finish_checkpoint(options, &root, &queue, start.scan, &traversal, interrupted)?;

    let aggregate = catalog.aggregate(&options.limits)?;
    let meta = ReportMeta {
        root: traversal.root.clone(),
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
    })
}

fn root_record(root: &Path) -> EntryRecord {
    EntryRecord {
        key: String::new(),
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

/// 这份中立库对着的还是不是同一个主库。
///
/// `--library` 让中立库跟名字走而不跟路径走（挂账 D16），于是换挂载点、换盘符都还能找回
/// 同一份库——这正是要的。代价是「名字一样、主库不一样」这种情况变得可能，而中立库的键
/// 是**相对**主库根的（ADR-0020），两个主库挤进同一份库不会报错，只会静默撞车。
///
/// 判据是顶层条目：库里记着的顶层键，与眼前这个根 `read_dir` 出来的名字比一比。它只花
/// 一次 `read_dir`（扫描本来也要读这一层），却足够分开「同一块盘换了挂载点」（顶层全对得上）
/// 与「换了另一个主库」（顶层几乎全不同）。**它认不出的那种情况**：两个主库恰好有过半
/// 同名的顶层目录——那得是刻意造的巧合，真出现了也还有 `--library` 换个名字这条路。
fn guard_same_library(
    library: &dyn LibraryFs,
    catalog: &Catalog,
    root: &Path,
) -> Result<(), ScanError> {
    let current = path::display(root);
    // 从没记过的库直接认下来：票 29 之前建的中立库都走这条，不该因为升级就打不开。
    let Some(recorded) = catalog.library_root()? else {
        catalog.set_library_root(&current)?;
        return Ok(());
    };
    if recorded == current {
        return Ok(());
    }
    let recorded_keys = catalog.top_level_keys(TOP_LEVEL_SAMPLE)?;
    // 空库没什么可撞的。
    if recorded_keys.is_empty() {
        catalog.set_library_root(&current)?;
        return Ok(());
    }
    let entries = library.read_dir(root).map_err(|source| ScanError::Root {
        path: current.clone(),
        source,
    })?;
    let actual: std::collections::HashSet<String> = entries
        .iter()
        .map(|entry| path::catalog_key(root, &entry.path))
        .collect();
    let common = recorded_keys
        .iter()
        .filter(|key| actual.contains(*key))
        .count();
    #[expect(
        clippy::cast_precision_loss,
        reason = "顶层条目至多 512 条，转 f64 精确"
    )]
    let ratio = common as f64 / recorded_keys.len() as f64;
    if ratio < SAME_LIBRARY_OVERLAP {
        return Err(ScanError::DifferentLibrary {
            catalog: catalog.location().to_string(),
            recorded,
            current,
            common,
            recorded_count: recorded_keys.len(),
        });
    }
    catalog.set_library_root(&current)?;
    Ok(())
}

fn load_start_state(
    options: &ScanOptions,
    root: &Path,
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
    // 断点比中立库旧就当它不存在。这种断点只可能来自「中断之后又跑过一次不写断点的
    // 完整扫描」：照着它续跑会以那个旧代号收尾，于是那次完整扫描记下的东西会被当成
    // 「这次没见到」整批删掉。宁可重扫一遍。
    if checkpoint.scan + 1 != next {
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
    progress: &mut Progress,
    result: DirResult,
) -> Result<DirDone, CatalogError> {
    for dir in &result.skipped_system_dirs {
        catalog.note_skipped_dir(dir)?;
    }
    for failed in &result.unlistable {
        catalog.note_error(&failed.display, &failed.message)?;
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

fn process_dir(
    library: &dyn LibraryFs,
    root: &Path,
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
                key: path::catalog_key(root, &result.dir),
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
            .push(observe(library, root, &entry, options, budget, baseline));
    }
    result
}

fn observe(
    library: &dyn LibraryFs,
    root: &Path,
    entry: &DirEntry,
    options: &ScanOptions,
    budget: &SampleBudget,
    baseline: &Baseline,
) -> EntryRecord {
    // 键要过 NFC，读盘用的仍是系统给的原始路径（ADR-0020）。
    let key = path::catalog_key(root, &entry.path);
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
        penetrate(library, &entry.path)
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
fn penetrate(library: &dyn LibraryFs, file: &Path) -> Option<Penetration> {
    let kind = ContainerKind::for_path(file)?;
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
        scan(library, catalog, options, &CancelToken::new()).expect("扫描不该失败")
    }

    fn 扫(library: &dyn LibraryFs, options: &ScanOptions) -> ScanOutcome {
        扫入(&mut 新中立库(), library, options)
    }

    /// 耗时是唯一每次都不同的字段。
    fn 规范化(aggregate: &mut Aggregate) {
        aggregate.elapsed_ms = 0;
    }

    fn 断点选项(dir: &Path) -> CheckpointOptions {
        CheckpointOptions {
            path: dir.join("scans").join("checkpoint.json"),
            interval: Duration::ZERO,
            resume: true,
        }
    }

    #[test]
    fn 按平台目录给出文件数与容量() {
        let library = 建库();
        let outcome = 扫(&library, &ScanOptions::new("/lib"));
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
        let outcome = 扫(&library, &ScanOptions::new("/lib"));
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
        let outcome = 扫(&library, &ScanOptions::new("/lib"));
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
        let outcome = 扫(&library, &ScanOptions::new("/lib"));
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
        let outcome = 扫(&library, &ScanOptions::new("/lib"));
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
        let mut options = ScanOptions::new("/lib");
        options.samples_per_class = 5;
        let outcome = 扫(&library, &options);
        assert_eq!(outcome.report.samples[0].sampled, 5);
        assert_eq!(outcome.report.totals.files, 50);
    }

    #[test]
    fn 关掉抽样就一个头部都不读() {
        let library = 建库();
        let mut options = ScanOptions::new("/lib");
        options.samples_per_class = 0;
        let outcome = 扫(&library, &options);
        assert!(outcome.report.samples.is_empty());
        assert_eq!(outcome.report.totals.files, 11);
    }

    #[test]
    fn 符号链接不跟随系统目录整棵跳过() {
        let library = 建库();
        let outcome = 扫(&library, &ScanOptions::new("/lib"));
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
        let outcome = 扫(&library, &ScanOptions::new("/lib"));
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
        let mut single = ScanOptions::new("/lib");
        single.jobs = Jobs::Fixed(1);
        let mut many = ScanOptions::new("/lib");
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
        let options = ScanOptions::new("/lib");

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
        let options = ScanOptions::new("/lib");
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
        let options = ScanOptions::new("/lib");

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
        let mut options = ScanOptions::new("/lib");
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
        let mut options = ScanOptions::new("/lib");

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
        let options = ScanOptions::new("/lib");

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
        let options = ScanOptions::new("/lib");
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
            catalog.contains("PS1/模拟器/epsxe.exe").expect("查得到"),
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
        let options = ScanOptions::new("/lib");
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
        let mut options = ScanOptions::new("/lib");
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
        let outcome = 扫(&library, &ScanOptions::new("/lib"));
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
        let options = ScanOptions::new("/lib");
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
                .contains(&format!("PSP/{预组合}me.iso"))
                .expect("查得到"),
            "入库的键必须是 NFC 形"
        );
        assert!(
            !catalog
                .contains(&format!("PSP/{分解}me.iso"))
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
            &ScanOptions::new("/Volumes/盘"),
        );
        assert!(先.delta.added > 0);

        let 再 = 扫入(
            &mut catalog,
            &建库于("/Volumes/盘 1"),
            &ScanOptions::new("/Volumes/盘 1"),
        );
        assert_eq!(再.delta.added, 0, "换挂载点不该把整个库判成新增");
        assert_eq!(再.delta.removed, 0, "更不该把旧的那份判成已删");
        assert_eq!(再.delta.unchanged, 先.delta.added);
        assert_eq!(再.report.totals.files, 先.report.totals.files);
    }

    #[test]
    fn 两个不同的主库共用一份中立库时报错而不是混表() {
        // 键是相对主库根的（ADR-0020），两个主库挤进同一份库不会报错，只会静默撞车。
        let mut catalog = 新中立库();
        扫入(
            &mut catalog,
            &建库于("/Volumes/甲"),
            &ScanOptions::new("/Volumes/甲"),
        );

        let mut 另一个主库 = MemFs::new();
        另一个主库
            .dir("/Volumes/乙")
            .file("/Volumes/乙/漫画/第一话.zip", zip(64))
            .file("/Volumes/乙/照片/去年.jpg", vec![0u8; 64]);
        let 错 = scan(
            &另一个主库,
            &mut catalog,
            &ScanOptions::new("/Volumes/乙"),
            &CancelToken::new(),
        )
        .expect_err("顶层条目全不一样，该拦下来");
        let ScanError::DifferentLibrary {
            recorded,
            current,
            common,
            ..
        } = &错
        else {
            panic!("该是 DifferentLibrary，实际是 {错:?}");
        };
        assert_eq!(recorded, "/Volumes/甲");
        assert_eq!(current, "/Volumes/乙");
        assert_eq!(*common, 0);
    }

    #[test]
    fn 顶层还对得上就认成同一个主库() {
        // 主库的顶层本来就会增删几个目录，卡太严会把正常的换挂载点也拦下来。
        let mut catalog = 新中立库();
        扫入(
            &mut catalog,
            &建库于("/Volumes/甲"),
            &ScanOptions::new("/Volumes/甲"),
        );

        let mut 少了一个平台 = 建库于("/Volumes/乙");
        少了一个平台.remove("/Volumes/乙/PSP");
        少了一个平台.file("/Volumes/乙/SFC/塞尔达.zip", zip(128));
        let 再 = 扫入(
            &mut catalog,
            &少了一个平台,
            &ScanOptions::new("/Volumes/乙"),
        );
        assert!(再.delta.added > 0, "新加的那个平台是新增");
        assert!(再.delta.unchanged > 0, "没动的那些还是未变");
    }

    #[test]
    fn 票_29_之前建的中立库照样打得开() {
        // 老库的 `meta` 里没有主库根这一条，不该因为升级就被当成「另一个主库」拦下。
        let mut catalog = 新中立库();
        扫入(&mut catalog, &建库(), &ScanOptions::new("/lib"));
        catalog.forget_library_root();
        let 再 = 扫入(&mut catalog, &建库(), &ScanOptions::new("/lib"));
        assert_eq!(再.delta.added, 0);
    }

    #[test]
    fn 体检报告能在主库不在位时从中立库出() {
        let mut catalog = 新中立库();
        let 扫出来的 = {
            let library = 建库();
            扫入(&mut catalog, &library, &ScanOptions::new("/lib")).report
        };
        // 盘拔了：这里连 MemFs 都没有了，中立库照样出得来
        let mut 库里的 = catalog
            .aggregate(&Limits::default())
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
        let 之前 = 扫入(&mut catalog, &建库(), &ScanOptions::new("/lib")).report;
        drop(catalog);

        // 关掉再打开：重启工具后结论不丢
        let path = crate::testing::temp_dir("catalog2")
            .path()
            .join("库.sqlite3");
        let mut catalog = Catalog::open(&path).expect("能开中立库");
        扫入(&mut catalog, &建库(), &ScanOptions::new("/lib"));
        drop(catalog);
        let catalog = Catalog::open(&path).expect("能再打开");
        let aggregate = catalog.aggregate(&Limits::default()).expect("读得出来");
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
        let mut options = ScanOptions::new("/lib");
        options.jobs = Jobs::Fixed(1);
        options.checkpoint = Some(断点选项(workspace.path()));
        let checkpoint = options.checkpoint.as_ref().expect("有断点").path.clone();

        // 一次扫完，作为对照
        let mut 对照 = 扫(&建库(), &options).aggregate;

        // 扫到第三个目录时按下中断
        let cancel = CancelToken::new();
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
        let first = scan(&中断的库, &mut catalog, &options, &cancel).expect("中断也算正常返回");
        assert!(first.interrupted, "应当报告被中断");
        assert!(checkpoint.exists(), "断点应当落盘");
        assert!(
            first.report.totals.files < 对照.totals.files,
            "中断时只扫了一部分"
        );
        assert_eq!(first.delta.removed, 0, "中断的扫描一条记录都不许删");

        // 续跑，直到扫完
        let mut 续跑结果 =
            scan(&建库(), &mut catalog, &options, &CancelToken::new()).expect("续跑不该失败");
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
        let mut options = ScanOptions::new("/lib");
        options.jobs = Jobs::Fixed(1);
        options.checkpoint = Some(断点选项(workspace.path()));

        let mut 对照 = 扫(&建库(), &options).aggregate;

        // PSP 是根目录之后第一个被扫的。钩子先等一会儿再按下中断：等这一下是有意的，
        // 它让协调线程先把上一个目录并完、进到等结果的状态，中断才落下——于是
        // 「半个目录的结果被送到协调线程手上」这条最危险的路径必然被走到，
        // 而真实的 Ctrl-C 正是这个时序。
        let cancel = CancelToken::new();
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
        let first = scan(&中断的库, &mut catalog, &options, &cancel).expect("中断也算正常返回");
        assert!(first.interrupted);

        let mut 续跑结果 =
            scan(&建库(), &mut catalog, &options, &CancelToken::new()).expect("续跑不该失败");
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
        let mut options = ScanOptions::new("/lib");
        options.jobs = Jobs::Fixed(1);
        options.checkpoint = Some(断点选项(workspace.path()));
        let checkpoint = options.checkpoint.as_ref().expect("有断点").path.clone();

        // 中断一次，留下断点
        let cancel = CancelToken::new();
        cancel.cancel();
        let mut catalog = 新中立库();
        let first = scan(&建库(), &mut catalog, &options, &cancel).expect("中断也算正常返回");
        assert!(first.interrupted);
        assert!(checkpoint.exists());

        // 中间插一次不写断点的完整扫描：中立库整个被刷新了一遍
        let mut 不写断点 = ScanOptions::new("/lib");
        不写断点.jobs = Jobs::Fixed(1);
        扫入(&mut catalog, &建库(), &不写断点);

        // 那份断点现在比中立库旧。照它续跑会把上一次完整扫描的记录全删掉。
        let 再扫 =
            scan(&建库(), &mut catalog, &options, &CancelToken::new()).expect("扫描不该失败");
        assert!(!再扫.report.resumed, "过期的断点不该被当成续跑");
        assert_eq!(再扫.report.totals.files, 11, "一个文件都不许丢");
        assert_eq!(再扫.delta.removed, 0);
    }

    #[test]
    fn 断点落在主库内直接拒绝开工() {
        let library = 建库();
        let mut options = ScanOptions::new("/lib");
        options.checkpoint = Some(CheckpointOptions {
            path: PathBuf::from("/lib/.romcat/checkpoint.json"),
            interval: Duration::ZERO,
            resume: false,
        });
        let err =
            scan(&library, &mut 新中立库(), &options, &CancelToken::new()).expect_err("必须拒绝");
        assert!(matches!(err, ScanError::WritesInsideLibrary { .. }));
    }

    #[test]
    fn 扫描根不存在时报得明白() {
        let library = 建库();
        let err = scan(
            &library,
            &mut 新中立库(),
            &ScanOptions::new("/不存在"),
            &CancelToken::new(),
        )
        .expect_err("必须报错");
        assert!(matches!(err, ScanError::Root { .. }));
    }

    #[test]
    fn 报告能渲染成文本也能存成_json() {
        let library = 建库();
        let outcome = 扫(&library, &ScanOptions::new("/lib"));
        let text = outcome.report.render_text();
        assert!(text.contains("库体检报告"));
        assert!(text.contains("透明容器"));
        assert!(text.contains(UNKNOWN_PLATFORM_LABEL));
        assert!(text.contains("这次扫描的增量"));

        let json = serde_json::to_string(&outcome.report).expect("能序列化");
        let back: crate::report::HealthReport = serde_json::from_str(&json).expect("能读回");
        assert_eq!(back, outcome.report);
    }
}
