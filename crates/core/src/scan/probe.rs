//! 开扫之前先探一探介质，据此定并发。
//!
//! ## 为什么不按 CPU 数取
//!
//! 票 01 的默认值是「按 CPU 数取，夹在 2 到 8」——**那是按 CPU 密集型工作设计的**。
//! 真机实测扫 NTFS 外置盘时 CPU 占用只有 **2.7%**，线程绝大部分时间在等文件系统驱动
//! 返回，是**高延迟 I/O 密集型**。两者的最优并发差一个数量级，拿 CPU 数去猜没有依据
//! （挂账 D9）。
//!
//! ## 探测怎么分得开两种相反的情况
//!
//! 提高并发在两种介质上的效果正相反：
//!
//! - **磁头寻道受限**：请求一多，磁头来回抖，吞吐不涨甚至下跌。并发越高越慢。
//! - **驱动软件开销受限**：每次调用的时间都花在等驱动返回上，多开几个线程能把等待
//!   叠起来。并发越高越快。
//!
//! 探测就是**同时**量这两件事：拿一组目录串行读一遍，再拿**另一组**目录并发读一遍，
//! 按「目录/秒」比。比值大于 1 是软件开销受限，比值不涨或下跌是寻道受限。
//!
//! ## 两组目录必须是两组
//!
//! 这里有个踩过的坑，值得写死在代码里：**同一组目录跑两次永远测不出并发的效果**，
//! 因为第二次是热缓存。上一次试图证明「提高并发有用」的实验就是这么失败的——同一个
//! 目录 `-j 32` 冷缓存对 `-j 4` 热缓存，热缓存快 11.8 倍，缓存效应把并发效应完全
//! 淹没，那个设计根本分不开两个变量。
//!
//! 所以 [`probe`] 先凑出一批**都还没读过**的兄弟目录，按下标奇偶劈成两组：一组给串行
//! 档，一组给并发档，**每个目录只读一次**。奇偶交错而不是前后对半，是为了让两组在树里
//! 的位置分布一样——前一半后一半可能一边全是大目录。
//!
//! ## 探测本身要多便宜
//!
//! 它只读 2n 个目录（n 至多 24），一次都不递归。真机上量到的串行档是每秒几个
//! 目录，于是探测的开销是秒级，而它要决定的是一趟几十分钟的扫描的并发档。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::fs::{EntryKind, LibraryFs};

use super::{CancelToken, default_jobs};

/// 并发上限。再往上加线程只是让等待队列更长，且断点里 `pending` 会跟着变大。
pub const MAX_JOBS: usize = 32;

/// 并发下限。
///
/// 留 2 而不是 1，是因为**探测只量了目录读取这一件事**，而一趟真扫描还要算 CRC-32、
/// 解 7z 的块——那些是 CPU 活，与读盘重叠得起来。真要压到单线程，`-j 1` 一直都在。
pub const MIN_JOBS: usize = 2;

/// **一档**（串行档或并发档）至多量几个目录。
const MAX_DIRS_PER_ARM: usize = 24;

/// 一档至少要有几个目录才下判断。
const MIN_DIRS_PER_ARM: usize = 4;

/// 探测用的并发档。量到的提速倍数以它为天花板。
const PROBE_JOBS: usize = 16;

/// 凑目录时最多往下走几层。
const MAX_DEPTH: usize = 4;

/// 单档的时间预算。慢介质上量满 24 个目录可能要十几秒，不值得。
const ARM_TIME_BUDGET: Duration = Duration::from_secs(3);

/// 单次目录读取快过这个数就别当延迟看了：**没有等待可叠**，那点时间是 CPU 与锁的开销，
/// 多开线程只会去抢同一份 CPU。这种介质的并发该由 CPU 数决定。
///
/// 门槛落在「一次目录读取」而不是「一档总时长」上：一档量几个目录是会变的，
/// 单次延迟才是介质的性质。真机那块 NTFS 外置盘是每次约 150 毫秒，差着两个数量级。
const FAST_DIR: Duration = Duration::from_millis(1);

/// 提速倍数低于它就算并发没用。留 5% 是给测量噪声的。
const NO_GAIN: f64 = 1.05;

/// 提速倍数达到探测并发档的这个比例，就认为还在线性区，值得再往上加。
const STILL_SCALING: f64 = 0.7;

/// 收益拐点：取吞吐够到并发档这个比例的最小线程数。
const KNEE: f64 = 0.9;

/// 探测给出的介质判断。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Medium {
    /// **磁头寻道受限**：并发档没比串行档快，甚至更慢。多开线程只会让磁头来回抖。
    SeekBound,
    /// **驱动软件开销受限**：线程绝大部分时间在等驱动返回，多开几个能把等待叠起来。
    LatencyBound,
    /// **低延迟介质**：一次目录读取快到测不出等待，并发该由 CPU 数决定。
    Fast,
    /// 样本不够——目录太少、被中断、或者凑目录时把候选都读热了。
    Inconclusive,
}

impl Medium {
    /// 报告里写的那句话。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::SeekBound => "寻道受限",
            Self::LatencyBound => "软件开销受限",
            Self::Fast => "低延迟介质",
            Self::Inconclusive => "样本不足",
        }
    }
}

/// 一档的读数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading {
    /// 这一档读了几个目录。
    pub dirs: usize,
    /// 这一档的并发线程数。串行档是 1。
    pub jobs: usize,
    /// 耗时。
    pub elapsed: Duration,
}

impl Reading {
    /// 目录/秒。**这是两档唯一可比的量**——两档读的目录个数可能不同。
    #[must_use]
    pub fn dirs_per_second(&self) -> f64 {
        let seconds = self.elapsed.as_secs_f64();
        if seconds <= 0.0 || self.dirs == 0 {
            return 0.0;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "目录数至多 MAX_DIRS_PER_ARM = 24，转 f64 精确"
        )]
        let dirs = self.dirs as f64;
        dirs / seconds
    }
}

/// 一次探测的全部读数与结论。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Probe {
    /// 串行档。
    pub serial: Reading,
    /// 并发档。
    pub parallel: Reading,
    /// 并发档比串行档快多少倍。
    pub speedup: f64,
    /// 介质判断。
    pub medium: Medium,
    /// 据此定下的并发数。
    pub jobs: usize,
}

impl Probe {
    /// 一句话说清这次探测量到了什么、据此定了多少并发。
    #[must_use]
    pub fn describe(&self) -> String {
        if self.medium == Medium::Inconclusive {
            return format!("介质探测：样本不足，并发按 CPU 数取 {} 个", self.jobs);
        }
        if self.medium == Medium::Fast {
            return format!(
                "介质探测：串行 {} 个目录 {:.2} 秒（{:.1} 目录/秒），一次目录读取不到一毫秒 → {}，\
                 并发按 CPU 数取 {} 个",
                self.serial.dirs,
                self.serial.elapsed.as_secs_f64(),
                self.serial.dirs_per_second(),
                self.medium.label(),
                self.jobs,
            );
        }
        format!(
            "介质探测：串行 {} 个目录 {:.2} 秒（{:.1} 目录/秒），并发 {} 档 {} 个目录 {:.2} 秒（{:.1} 目录/秒），\
             提速 {:.2} 倍 → {}，并发定为 {} 个",
            self.serial.dirs,
            self.serial.elapsed.as_secs_f64(),
            self.serial.dirs_per_second(),
            self.parallel.jobs,
            self.parallel.dirs,
            self.parallel.elapsed.as_secs_f64(),
            self.parallel.dirs_per_second(),
            self.speedup,
            self.medium.label(),
            self.jobs,
        )
    }

    /// 样本不足时的退路：并发回到按 CPU 数取。
    fn inconclusive() -> Self {
        let zero = Reading {
            dirs: 0,
            jobs: 1,
            elapsed: Duration::ZERO,
        };
        Self {
            serial: zero,
            parallel: Reading { jobs: 0, ..zero },
            speedup: 0.0,
            medium: Medium::Inconclusive,
            jobs: default_jobs(),
        }
    }
}

/// 探一探 `root` 这块介质，给出该用多少并发。
///
/// 只读，只读目录，一次都不递归进去。被中断时返回 [`Medium::Inconclusive`]。
pub fn probe(library: &dyn LibraryFs, root: &Path, cancel: &CancelToken) -> Probe {
    let Some(pool) = cold_dirs(library, root, cancel) else {
        return Probe::inconclusive();
    };
    // 奇偶交错劈成两组：**每个目录只读一次**，两组在树里的位置分布也一样。
    // 前一半后一半的劈法会让一边全是大目录，那是另一种混进来的变量。
    let mut serial_arm: Vec<&PathBuf> = Vec::new();
    let mut parallel_arm: Vec<&PathBuf> = Vec::new();
    // 第 0 个当热身，两档都不算它：介质刚被碰到的第一次读常常带着上电、缓存预热这类
    // 一次性成本，落在哪一档就冤枉哪一档。
    for (index, dir) in pool.iter().skip(1).enumerate() {
        if index % 2 == 0 {
            serial_arm.push(dir);
        } else {
            parallel_arm.push(dir);
        }
    }
    let arm = serial_arm
        .len()
        .min(parallel_arm.len())
        .min(MAX_DIRS_PER_ARM);
    if arm < MIN_DIRS_PER_ARM {
        return Probe::inconclusive();
    }
    let _ = library.read_dir(&pool[0]);
    if cancel.is_cancelled() {
        return Probe::inconclusive();
    }

    let serial = read_serially(library, &serial_arm[..arm], cancel);
    if serial.dirs < MIN_DIRS_PER_ARM || cancel.is_cancelled() {
        return Probe::inconclusive();
    }
    // 快到量不出等待的介质，别硬解释那个比值：它全是噪声。
    if serial.elapsed / u32::try_from(serial.dirs).unwrap_or(1) < FAST_DIR {
        return Probe {
            serial,
            parallel: Reading {
                dirs: 0,
                jobs: 0,
                elapsed: Duration::ZERO,
            },
            speedup: 0.0,
            medium: Medium::Fast,
            jobs: default_jobs(),
        };
    }

    // 两档读一样多的目录，比值才干净。串行档没读满预算时并发档也只读这么多。
    let jobs = PROBE_JOBS.min(serial.dirs);
    let parallel = read_in_parallel(library, &parallel_arm[..serial.dirs], jobs, cancel);
    if parallel.dirs < MIN_DIRS_PER_ARM || cancel.is_cancelled() {
        return Probe::inconclusive();
    }

    let speedup = if serial.dirs_per_second() > 0.0 {
        parallel.dirs_per_second() / serial.dirs_per_second()
    } else {
        0.0
    };
    let (medium, jobs) = decide(speedup, jobs);
    Probe {
        serial,
        parallel,
        speedup,
        medium,
        jobs,
    }
}

/// 由提速倍数定介质与并发。
///
/// ## 两个点能说出多少
///
/// 探测只给出两个点：一个线程的吞吐，和 `probe_jobs` 个线程的吞吐（比值 `speedup`）。
/// 把这块介质当成一个**会饱和的服务台**——每多一个在飞的请求，每个请求的延迟就涨一点
/// （磁头多走一趟、驱动多排一次队）：
///
/// ```text
/// 吞吐(n) / 吞吐(1) = n / (1 + (n-1)·k)
/// ```
///
/// `k` 是「每多一个并发请求，延迟涨多少」，以单线程延迟为单位。把 `n = probe_jobs`
/// 那个点代进去就解得出来：
///
/// ```text
/// k = (probe_jobs / speedup - 1) / (probe_jobs - 1)
/// ```
///
/// 真机实测的三个 `k` 正好把两种介质分在两边：`Game/ps` 那片 k = 0.16（多一个请求只贵
/// 一成六，越并发越赚），`Game/SFC` 那片 k = 6.2（多一个请求贵六倍，**并发直接是负收益**）。
///
/// ## 由 k 定并发
///
/// - **k ≥ 1，或者提速根本不到 [`NO_GAIN`]**：多开一个线程带来的延迟涨幅比吞吐涨幅还大，
///   吞吐从第二个线程起就往下走。这就是**寻道受限**，退到下限。两道闸都要，是因为 k 在
///   提速接近 1 时对噪声极敏感——实测 `Game/pc` 提速 1.02 算出 k = 0.98，只差一点点就
///   越过判据，而那 2% 完全在测量噪声里。**噪声线优先于模型**。
/// - **提速倍数已经接近探测档本身**：天花板还没到，翻一倍再试。
/// - **中间那段**：取「吞吐够到并发档的九成」的最小线程数——那是收益拐点，再往上加线程
///   换来的不到一成，却让在飞的请求翻倍。宁可少要那一成：过量并发的代价是**不对称的**，
///   SFC 那片 16 线程比串行慢 5.9 倍，而并发不足最多少赚几成。
fn decide(speedup: f64, probe_jobs: usize) -> (Medium, usize) {
    #[expect(
        clippy::cast_precision_loss,
        reason = "probe_jobs 至多 PROBE_JOBS = 16，转 f64 精确"
    )]
    let ceiling = probe_jobs as f64;
    if speedup <= 0.0 {
        return (Medium::SeekBound, MIN_JOBS);
    }
    if speedup >= STILL_SCALING * ceiling {
        return (Medium::LatencyBound, (probe_jobs * 2).min(MAX_JOBS));
    }
    // 每多一个在飞的请求，延迟涨多少（以单线程延迟为单位）。
    let contention = (ceiling / speedup - 1.0) / (ceiling - 1.0);
    if contention >= 1.0 || speedup < NO_GAIN {
        return (Medium::SeekBound, MIN_JOBS);
    }
    (Medium::LatencyBound, knee(speedup, contention))
}

/// 吞吐够到并发档九成的最小线程数。
///
/// 由 `n / (1 + (n-1)·k) ≥ r·s` 解出：
///
/// ```text
/// n ≥ r·s·(1-k) / (1 - r·s·k)
/// ```
///
/// 分母在 `k < 1` 且 `s < probe_jobs` 时必然为正（[`decide`] 已经把这两条挡在外面）。
fn knee(speedup: f64, contention: f64) -> usize {
    let target = KNEE * speedup;
    let denominator = 1.0 - target * contention;
    if denominator <= 0.0 {
        return MAX_JOBS;
    }
    let n = target * (1.0 - contention) / denominator;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "紧接着就夹在 MIN_JOBS 与 MAX_JOBS 之间"
    )]
    let jobs = n.ceil() as usize;
    jobs.clamp(MIN_JOBS, MAX_JOBS)
}

/// 凑一批**还没被读过**的兄弟目录。
///
/// 从主库根往下走：这一层的目录不够用时，挑开头几个读开，把它们的下一层收进来。
/// **读开的那几个就热了，必须从池子里剔掉**；没读开的那些还是冷的，留着接着用。
/// 只读到够为止，不把整层都读开——多读一个就多废掉一个冷目录。
///
/// 走到 `MAX_DEPTH` 层还是不够就放弃：那种库小到并发怎么定都无所谓，退回按 CPU 数取。
fn cold_dirs(library: &dyn LibraryFs, root: &Path, cancel: &CancelToken) -> Option<Vec<PathBuf>> {
    let want = MAX_DIRS_PER_ARM * 2 + 1;
    let need = MIN_DIRS_PER_ARM * 2 + 1;
    let mut pool = subdirs(library, root);
    let mut depth = 1;
    while pool.len() < need && depth < MAX_DEPTH {
        if cancel.is_cancelled() {
            return None;
        }
        let mut gained = Vec::new();
        let mut opened = 0;
        for dir in &pool {
            opened += 1;
            gained.extend(subdirs(library, dir));
            if pool.len() - opened + gained.len() >= want {
                break;
            }
        }
        if gained.is_empty() {
            break;
        }
        pool.drain(..opened);
        pool.extend(gained);
        depth += 1;
    }
    if pool.len() < need {
        return None;
    }
    pool.truncate(want);
    Some(pool)
}

fn subdirs(library: &dyn LibraryFs, dir: &Path) -> Vec<PathBuf> {
    library
        .read_dir(dir)
        .map(|entries| {
            entries
                .into_iter()
                .filter(|entry| entry.kind == EntryKind::Dir)
                .map(|entry| entry.path)
                .collect()
        })
        .unwrap_or_default()
}

fn read_serially(library: &dyn LibraryFs, dirs: &[&PathBuf], cancel: &CancelToken) -> Reading {
    let started = Instant::now();
    let mut read = 0;
    for dir in dirs {
        let _ = library.read_dir(dir);
        read += 1;
        if cancel.is_cancelled() || started.elapsed() >= ARM_TIME_BUDGET {
            break;
        }
    }
    Reading {
        dirs: read,
        jobs: 1,
        elapsed: started.elapsed(),
    }
}

fn read_in_parallel(
    library: &dyn LibraryFs,
    dirs: &[&PathBuf],
    jobs: usize,
    cancel: &CancelToken,
) -> Reading {
    let next = AtomicUsize::new(0);
    let started = Instant::now();
    std::thread::scope(|scope| {
        for _ in 0..jobs {
            let next = &next;
            scope.spawn(move || {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(dir) = dirs.get(index) else {
                        break;
                    };
                    let _ = library.read_dir(dir);
                    if cancel.is_cancelled() {
                        break;
                    }
                }
            });
        }
    });
    Reading {
        dirs: dirs.len().min(next.load(Ordering::Relaxed)),
        jobs,
        elapsed: started.elapsed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemFs;

    fn 建库(dirs: usize) -> MemFs {
        let mut library = MemFs::new();
        for n in 0..dirs {
            library.file(format!("/lib/目录{n:03}/文件.zip"), vec![0u8; 64]);
        }
        library
    }

    #[test]
    fn 目录太少时不下判断而是回到按_cpu_数取() {
        let probe = probe(&建库(3), Path::new("/lib"), &CancelToken::new());
        assert_eq!(probe.medium, Medium::Inconclusive);
        assert_eq!(probe.jobs, default_jobs());
    }

    #[test]
    fn 内存里的库快到测不出等待() {
        // 一次目录读取连一毫秒都不到，比值全是噪声——这种介质并发该由 CPU 数决定。
        let probe = probe(&建库(64), Path::new("/lib"), &CancelToken::new());
        assert!(
            matches!(probe.medium, Medium::Fast | Medium::Inconclusive),
            "{:?}",
            probe.medium
        );
        assert_eq!(probe.jobs, default_jobs());
    }

    #[test]
    fn 两档拿的是两组不重叠的目录() {
        // 同一组目录跑两次永远是「第二次热缓存」，测不出并发的效果。
        let library = 建库(64);
        let pool = cold_dirs(&library, Path::new("/lib"), &CancelToken::new()).expect("凑得出来");
        let 串行: Vec<_> = pool.iter().skip(1).step_by(2).collect();
        let 并发: Vec<_> = pool.iter().skip(2).step_by(2).collect();
        for dir in &串行 {
            assert!(!并发.contains(dir), "{dir:?} 两档都读了");
        }
        assert!(串行.len() >= MIN_DIRS_PER_ARM && 并发.len() >= MIN_DIRS_PER_ARM);
    }

    #[test]
    fn 一层不够就往下走且读开过的那几个不再当样本() {
        // 顶层只有 2 个目录，不够劈成两档；往下一层就够了。
        let mut library = MemFs::new();
        for a in 0..2 {
            for b in 0..8 {
                library.file(format!("/lib/顶{a}/次{b}/文件.zip"), vec![0u8; 8]);
            }
        }
        let pool = cold_dirs(&library, Path::new("/lib"), &CancelToken::new()).expect("凑得出来");
        assert!(pool.len() > MIN_DIRS_PER_ARM * 2, "{pool:?}");
        for dir in &pool {
            assert!(
                dir.to_string_lossy().contains("次"),
                "读开过的 {dir:?} 已经热了，不能再当样本"
            );
        }
    }

    #[test]
    fn 一个目录都没有时不下判断() {
        let mut library = MemFs::new();
        library.file("/lib/散文件.zip", vec![0u8; 8]);
        let probe = probe(&library, Path::new("/lib"), &CancelToken::new());
        assert_eq!(probe.medium, Medium::Inconclusive);
    }

    #[test]
    fn 中断时不下判断() {
        let cancel = CancelToken::new();
        cancel.cancel();
        let probe = probe(&建库(64), Path::new("/lib"), &cancel);
        assert_eq!(probe.medium, Medium::Inconclusive);
    }

    #[test]
    fn 并发没帮上忙就是寻道受限() {
        // 真机 `Game/SFC` 那片量到的就是 0.17：16 线程比串行慢 5.9 倍。
        assert_eq!(decide(0.17, 16), (Medium::SeekBound, MIN_JOBS));
        assert_eq!(decide(1.0, 16), (Medium::SeekBound, MIN_JOBS));
        // 真机 `Game/pc` 那片量到的是 1.02：并发一点没帮上忙。
        assert_eq!(decide(1.02, 16), (Medium::SeekBound, MIN_JOBS));
        // 正好在噪声线上：不算有用。
        assert_eq!(decide(1.04, 16), (Medium::SeekBound, MIN_JOBS));
    }

    #[test]
    fn 并发叠得起来就取收益拐点() {
        // 真机 `Game/ps` 那片量到 4.71 倍：拐点在 12 个线程，再往上加换不到一成。
        assert_eq!(decide(4.71, 16), (Medium::LatencyBound, 12));
        // 真机 `Game` 根量到 1.42 倍：饱和得很快，4 个线程就够到九成。
        assert_eq!(decide(1.42, 16), (Medium::LatencyBound, 4));
        // 真机 `Game/gba` 那片量到 1.24 倍。
        assert_eq!(decide(1.24, 16), (Medium::LatencyBound, 3));
        // 真机 `Game/MD` 那片只凑得出每组 4 个目录，探测档跟着降到 4；3.52 倍还在线性区。
        assert_eq!(decide(3.52, 4), (Medium::LatencyBound, 8));
        // 还在线性区（>= 0.7 × 16 = 11.2）：天花板没到，翻一倍再试。
        assert_eq!(decide(12.0, 16), (Medium::LatencyBound, 32));
    }

    #[test]
    fn 拐点永远不超过探测档本身() {
        // 拐点是「够到并发档九成的最小线程数」，它不该反过来超过量出那九成的那个档。
        for speedup in [1.06_f64, 1.5, 2.0, 4.0, 8.0, 11.0] {
            let (_, jobs) = decide(speedup, 16);
            assert!(jobs <= 16, "{speedup} 定出 {jobs}");
        }
    }

    #[test]
    fn 并发数永远夹在上下限之间() {
        for speedup in [1.06_f64, 1.4, 2.0, 9.9, 15.9, 1e9] {
            let (_, jobs) = decide(speedup, 16);
            assert!(
                (MIN_JOBS..=MAX_JOBS).contains(&jobs),
                "{speedup} 定出 {jobs}"
            );
        }
    }

    #[test]
    fn 目录每秒是两档唯一可比的量() {
        let 串行 = Reading {
            dirs: 12,
            jobs: 1,
            elapsed: Duration::from_secs(2),
        };
        let 并发 = Reading {
            dirs: 6,
            jobs: 8,
            elapsed: Duration::from_secs(1),
        };
        // 并发档只读了一半的目录，但每秒的目录数一样——归一化之后才比得了。
        assert!((串行.dirs_per_second() - 并发.dirs_per_second()).abs() < 1e-9);
    }
}
