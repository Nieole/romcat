//! 只读遍历主库，产出**库体检报告**。
//!
//! 三条约束决定了这里的形状：
//!
//! - **只读**。一切磁盘接触走 [`LibraryFs`]，它没有写的办法（ADR-0004）。断点写在
//!   本机工作目录，且落在主库内时直接拒绝开工。
//! - **并发到打满磁盘带宽**。工作线程只做「列一层目录 + 抽样读头部」这种纯 IO 的事，
//!   统计全部交给协调线程单线程合并——统计不是瓶颈，而单线程合并让断点有一个明确的
//!   一致点。
//! - **可中断可续跑**。协调线程持有队列与统计，断点里的 `pending` 同时包含队列里的和
//!   正在扫的目录，因此中断只会让少量目录被重扫，绝不会算重。
//!
//! 平台由目录给出（ADR-0011）：文件相对扫描根的第一级目录名就是平台目录。认不出平台
//! 的照常计入报告——平台未知不构成跳过的理由。

pub mod aggregate;
pub mod checkpoint;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use std::{fs, thread};

use crate::classify::{self, classify};
use crate::fs::{EntryKind, LibraryFs};
use crate::header::{self, ProbeClass};
use crate::path;
use crate::report::{HealthReport, ReportMeta};

use aggregate::{Aggregate, FileObservation, Limits, SampleResult};
use checkpoint::{Checkpoint, CheckpointError};

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
    /// 断点落在主库内。
    #[error("断点文件 {0} 落在主库内。主库只读，断点必须写在本机的工作目录里")]
    CheckpointInsideLibrary(String),
    /// 断点读写失败。
    #[error(transparent)]
    Checkpoint(#[from] CheckpointError),
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

/// 扫描参数。
#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// 主库根目录。
    pub root: PathBuf,
    /// 并发线程数。
    pub jobs: usize,
    /// 每类文件抽样读多少个头部。0 表示不抽样。
    pub samples_per_class: usize,
    /// 例子列表与索引的上限。
    pub limits: Limits,
    /// 断点配置；`None` 表示不写断点（也就不能续跑）。
    pub checkpoint: Option<CheckpointOptions>,
}

impl ScanOptions {
    /// 用默认参数扫描 `root`。
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            jobs: default_jobs(),
            samples_per_class: 32,
            limits: Limits::default(),
            checkpoint: None,
        }
    }
}

/// 默认并发数。
///
/// 机械硬盘上线程开太多只会让磁头来回抖，因此夹在 2 到 8 之间；真要压满某块盘，
/// 用 `--jobs` 按盘调。
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
    /// 是否被中断。
    pub interrupted: bool,
    /// 断点文件位置（中断且写了断点时）。
    pub checkpoint_path: Option<PathBuf>,
}

/// 扫一遍主库。
///
/// # Errors
/// 扫描根打不开、断点落在主库内、或断点读写失败时返回错误。
pub fn scan(
    library: &dyn LibraryFs,
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

    // 断点是这条流程里唯一的写操作，它必须落在主库之外。比较前两边都化成绝对形态，
    // 否则 `/var` 与 `/private/var` 这类链接会让守卫形同虚设。
    if let Some(config) = &options.checkpoint
        && path::is_inside(&root, &path::normalize_existing(&config.path))
    {
        return Err(ScanError::CheckpointInsideLibrary(path::display(
            &config.path,
        )));
    }

    let (mut aggregate, pending, resumed) = load_start_state(options, &root)?;
    let elapsed_base = Duration::from_millis(aggregate.elapsed_ms);
    let budget = SampleBudget::resume_from(&aggregate, options.samples_per_class);
    let queue = Queue::new(pending);

    let (results_tx, results_rx) = mpsc::channel::<DirResult>();
    let jobs = options.jobs.max(1);

    let interrupted = thread::scope(|scope| -> Result<bool, ScanError> {
        for _ in 0..jobs {
            let tx = results_tx.clone();
            let queue = &queue;
            let budget = &budget;
            let root = &root;
            scope.spawn(move || {
                while let Some(dir) = queue.pop() {
                    let result = process_dir(library, root, dir, options, budget, cancel);
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
                    let subdirs = merge(&mut aggregate, result, &options.limits);
                    queue.finish_and_push(subdirs);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if let Some(config) = &options.checkpoint
                && last_save.elapsed() >= config.interval
            {
                aggregate.elapsed_ms = elapsed(elapsed_base, started);
                save_checkpoint(options, &root, &queue, &aggregate)?;
                last_save = Instant::now();
            }
        }
        queue.close();
        Ok(interrupted)
    })?;

    aggregate.elapsed_ms = elapsed(elapsed_base, started);

    let checkpoint_path = finish_checkpoint(options, &root, &queue, &aggregate, interrupted)?;
    let meta = ReportMeta {
        root: path::display(&root),
        interrupted,
        resumed,
        jobs,
        samples_per_class: options.samples_per_class,
    };
    Ok(ScanOutcome {
        report: HealthReport::build(&aggregate, &meta),
        aggregate,
        interrupted,
        checkpoint_path,
    })
}

fn elapsed(base: Duration, started: Instant) -> u64 {
    u64::try_from((base + started.elapsed()).as_millis()).unwrap_or(u64::MAX)
}

type StartState = (Aggregate, Vec<PathBuf>, bool);

fn load_start_state(options: &ScanOptions, root: &Path) -> Result<StartState, ScanError> {
    let fresh = || (Aggregate::default(), vec![root.to_path_buf()], false);
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
    Ok((checkpoint.aggregate, pending, true))
}

fn save_checkpoint(
    options: &ScanOptions,
    root: &Path,
    queue: &Queue,
    aggregate: &Aggregate,
) -> Result<(), ScanError> {
    let Some(config) = &options.checkpoint else {
        return Ok(());
    };
    let pending = queue.pending_snapshot();
    Checkpoint::new(root, options.samples_per_class, &pending, aggregate.clone())
        .save(&config.path)?;
    Ok(())
}

fn finish_checkpoint(
    options: &ScanOptions,
    root: &Path,
    queue: &Queue,
    aggregate: &Aggregate,
    interrupted: bool,
) -> Result<Option<PathBuf>, ScanError> {
    let Some(config) = &options.checkpoint else {
        return Ok(None);
    };
    if interrupted {
        save_checkpoint(options, root, queue, aggregate)?;
        return Ok(Some(config.path.clone()));
    }
    // 扫完了就把断点删掉：留着它只会让下次 `--resume` 误以为还有活没干完。
    let _ = fs::remove_file(&config.path);
    Ok(None)
}

fn merge(aggregate: &mut Aggregate, result: DirResult, limits: &Limits) -> DirDone {
    aggregate.record_dir();
    for _ in 0..result.symlinks {
        aggregate.record_symlink();
    }
    for _ in 0..result.others {
        aggregate.record_other_entry();
    }
    for dir in &result.skipped_system_dirs {
        aggregate.record_skipped_system_dir(dir, limits);
    }
    for (path, error) in &result.errors {
        aggregate.record_error(path, error, limits);
    }
    for observation in &result.files {
        aggregate.record_file(observation, limits);
    }
    DirDone {
        dir: result.dir,
        subdirs: result.subdirs,
    }
}

struct DirDone {
    dir: PathBuf,
    subdirs: Vec<PathBuf>,
}

struct DirResult {
    dir: PathBuf,
    subdirs: Vec<PathBuf>,
    files: Vec<FileObservation>,
    symlinks: u64,
    others: u64,
    skipped_system_dirs: Vec<PathBuf>,
    errors: Vec<(PathBuf, String)>,
    /// 这个目录是被中断打断的，只扫了一部分，不能并入统计。
    partial: bool,
}

fn process_dir(
    library: &dyn LibraryFs,
    root: &Path,
    dir: PathBuf,
    options: &ScanOptions,
    budget: &SampleBudget,
    cancel: &CancelToken,
) -> DirResult {
    let mut result = DirResult {
        dir,
        subdirs: Vec::new(),
        files: Vec::new(),
        symlinks: 0,
        others: 0,
        skipped_system_dirs: Vec::new(),
        errors: Vec::new(),
        partial: false,
    };

    let entries = match library.read_dir(&result.dir) {
        Ok(entries) => entries,
        Err(error) => {
            result.errors.push((result.dir.clone(), error.to_string()));
            return result;
        }
    };

    for entry in entries {
        if cancel.is_cancelled() {
            result.partial = true;
            break;
        }
        match entry.kind {
            EntryKind::Dir => {
                let name = path::file_name_lower(&entry.path);
                if classify::is_skipped_system_dir(&name) {
                    result.skipped_system_dirs.push(entry.path);
                } else {
                    result.subdirs.push(entry.path);
                }
            }
            // 不跟随符号链接：跟随会引入环，也会让同一份内容被算两次。
            EntryKind::Symlink => result.symlinks += 1,
            EntryKind::Other => result.others += 1,
            EntryKind::File => {
                result.files.push(observe_file(
                    library,
                    root,
                    &entry.path,
                    entry.len,
                    options,
                    budget,
                ));
            }
        }
    }
    result
}

fn observe_file(
    library: &dyn LibraryFs,
    root: &Path,
    file: &Path,
    len: u64,
    options: &ScanOptions,
    budget: &SampleBudget,
) -> FileObservation {
    let classification = classify(file);
    let probe_class = header::probe_class_for(file);
    let sample = sample_header(library, file, len, probe_class, options, budget);
    FileObservation {
        display_path: path::display(file),
        name_lower: path::file_name_lower(file),
        platform: path::platform_dir(root, file).map(|name| name.to_string_lossy().into_owned()),
        extension: path::extension_lower(file),
        len,
        classification,
        cjk: classify::has_cjk(file),
        non_utf8: !path::is_utf8(file),
        over_max_path: path::exceeds_max_path(file),
        has_probe: probe_class.is_some(),
        sample,
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
/// 续跑时从已有统计里恢复，否则每续跑一次就会多抽一轮。被丢弃的结果（中断时正在扫的
/// 那个目录）会白占配额，但那至多是一个目录的量。
struct SampleBudget {
    per_class: usize,
    taken: Mutex<std::collections::BTreeMap<ProbeClass, usize>>,
}

impl SampleBudget {
    fn resume_from(aggregate: &Aggregate, per_class: usize) -> Self {
        let mut taken = std::collections::BTreeMap::new();
        for class in aggregate.samples.keys() {
            taken.insert(*class, aggregate.sampled_count(*class));
        }
        Self {
            per_class,
            taken: Mutex::new(taken),
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
/// `active` 是正在被工作线程扫的目录。它必须和 `stack` 一起进断点：那些目录的统计还没
/// 并进来，续跑时重扫一遍才不会漏。
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

    /// 一个目录的结果已经并入统计：把它从 `active` 摘掉，并把它的子目录入队。
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

    fn pending_snapshot(&self) -> Vec<PathBuf> {
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
        let mut library = MemFs::new();
        library
            .dir("/lib")
            .file("/lib/FC/超级马里奥.zip", zip(2048))
            .file("/lib/FC/魂斗罗.zip", vec![0u8; 512]) // 扩展名说 zip，内容不是
            .file("/lib/FC/说明.txt", "随便写点什么".as_bytes().to_vec())
            .file("/lib/FC/备份/超级马里奥.zip", zip(2048)) // 与上面同名同大小
            .file("/lib/PS1/最终幻想.chd", chd())
            .file("/lib/PS1/模拟器/epsxe.exe", vec![0u8; 100])
            .file("/lib/PSP/游戏.iso", iso())
            .file("/lib/PSP/半截下载.iso.part", vec![0u8; 10])
            .file("/lib/散落的游戏.gba", vec![0u8; 16])
            .file("/lib/.DS_Store", vec![0u8; 8])
            .file("/lib/__MACOSX/垃圾", vec![0u8; 8]) // 整棵跳过
            .file("/lib/空文件.zip", Vec::new())
            .symlink("/lib/指向别处");
        library
    }

    fn 扫(library: &dyn LibraryFs, options: &ScanOptions) -> ScanOutcome {
        scan(library, options, &CancelToken::new()).expect("扫描不该失败")
    }

    fn 规范化(aggregate: &mut Aggregate) {
        aggregate.elapsed_ms = 0;
        for examples in aggregate.suspect_examples.values_mut() {
            examples.sort();
        }
        for group in aggregate.duplicate_index.values_mut() {
            group.examples.sort();
        }
        for acc in aggregate.samples.values_mut() {
            acc.failures.sort();
        }
        aggregate.anomalies.error_examples.sort();
        aggregate.anomalies.over_max_path_examples.sort();
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
        library.unreadable_file("/lib/FC/读不动.zip");
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
        single.jobs = 1;
        let mut many = ScanOptions::new("/lib");
        many.jobs = 8;

        let mut a = 扫(&library, &single).aggregate;
        let mut b = 扫(&library, &many).aggregate;
        规范化(&mut a);
        规范化(&mut b);
        assert_eq!(a, b);
    }

    /// 一个只在「列目录」这一步挂钩子的替身。
    ///
    /// 中断要发生在哪一刻，是这些测试唯一需要控制的东西，因此钩子只有一个。
    struct 读目录时挂钩<'a> {
        inner: MemFs,
        hook: Box<dyn Fn(&Path) + Send + Sync + 'a>,
    }

    impl LibraryFs for 读目录时挂钩<'_> {
        fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
            self.inner.canonicalize(path)
        }

        fn read_dir(&self, dir: &Path) -> std::io::Result<Vec<DirEntry>> {
            (self.hook)(dir);
            self.inner.read_dir(dir)
        }

        fn read_head(&self, file: &Path, limit: usize) -> std::io::Result<Vec<u8>> {
            self.inner.read_head(file, limit)
        }

        fn read_tail(&self, file: &Path, limit: usize) -> std::io::Result<Vec<u8>> {
            self.inner.read_tail(file, limit)
        }
    }

    #[test]
    fn 中断再续跑与一次扫完结论相同() {
        let workspace = crate::testing::temp_dir("scan");
        let checkpoint = workspace.path().join("scans").join("checkpoint.json");
        let mut options = ScanOptions::new("/lib");
        options.jobs = 1;
        options.checkpoint = Some(CheckpointOptions {
            path: checkpoint.clone(),
            interval: Duration::ZERO,
            resume: true,
        });

        // 一次扫完，作为对照
        let mut 对照 = 扫(&建库(), &options).aggregate;

        // 扫到第二个目录时中断
        // 扫到第三个目录时按下中断
        let cancel = CancelToken::new();
        let seen = AtomicUsize::new(0);
        let 中断的库 = 读目录时挂钩 {
            inner: 建库(),
            hook: Box::new(|_: &Path| {
                if seen.fetch_add(1, Ordering::SeqCst) >= 2 {
                    cancel.cancel();
                }
            }),
        };
        let first = scan(&中断的库, &options, &cancel).expect("中断也算正常返回");
        assert!(first.interrupted, "应当报告被中断");
        assert!(checkpoint.exists(), "断点应当落盘");
        assert!(
            first.aggregate.totals.files < 对照.totals.files,
            "中断时只扫了一部分"
        );

        // 续跑，直到扫完
        let mut 续跑结果 = scan(&建库(), &options, &CancelToken::new()).expect("续跑不该失败");
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
        let checkpoint = workspace.path().join("checkpoint.json");
        let mut options = ScanOptions::new("/lib");
        options.jobs = 1;
        options.checkpoint = Some(CheckpointOptions {
            path: checkpoint.clone(),
            interval: Duration::ZERO,
            resume: true,
        });

        let mut 对照 = 扫(&建库(), &options).aggregate;

        // PSP 是根目录之后第一个被扫的。钩子先等一会儿再按下中断：等这一下是有意的，
        // 它让协调线程先把上一个目录并完、进到等结果的状态，中断才落下——于是
        // 「半个目录的结果被送到协调线程手上」这条最危险的路径必然被走到，
        // 而真实的 Ctrl-C 正是这个时序。
        let cancel = CancelToken::new();
        let 中断的库 = 读目录时挂钩 {
            inner: 建库(),
            hook: Box::new(|dir: &Path| {
                if dir == Path::new("/lib/PSP") {
                    thread::sleep(Duration::from_millis(25));
                    cancel.cancel();
                }
            }),
        };
        let first = scan(&中断的库, &options, &cancel).expect("中断也算正常返回");
        assert!(first.interrupted);

        let mut 续跑结果 = scan(&建库(), &options, &CancelToken::new()).expect("续跑不该失败");
        assert!(续跑结果.report.resumed);
        规范化(&mut 对照);
        规范化(&mut 续跑结果.aggregate);
        assert_eq!(
            续跑结果.aggregate, 对照,
            "被打断的那个目录必须整个重扫，一个文件都不能少"
        );
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
        let err = scan(&library, &options, &CancelToken::new()).expect_err("必须拒绝");
        assert!(matches!(err, ScanError::CheckpointInsideLibrary(_)));
    }

    #[test]
    fn 扫描根不存在时报得明白() {
        let library = 建库();
        let err = scan(&library, &ScanOptions::new("/不存在"), &CancelToken::new())
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

        let json = serde_json::to_string(&outcome.report).expect("能序列化");
        let back: crate::report::HealthReport = serde_json::from_str(&json).expect("能读回");
        assert_eq!(back, outcome.report);
    }
}
