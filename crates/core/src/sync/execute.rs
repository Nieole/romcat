//! **执行**：把[计划](super::Plan)真正落到目标设备上。
//!
//! 这是整个工具**第一个**往别人的设备上写字节的地方。前面每一层都只读主库、只写自己的
//! 产物；从这里开始，写错就是维护者的东西没了。于是这个模块里每一条规矩都写着它防的是
//! 哪一种损失。
//!
//! ## 一、只走计划里的那几步
//!
//! [`run`] 只认 [`Plan::steps`](super::Plan::steps)，不重新看一眼目标、不自己发明任何
//! 一次写入。而计划这一侧，删除与更新**只可能从清单里长出来**（`sync` 模块文档），
//! 于是「只碰清单里记录过的、工具自己导出的文件」这条硬约束（ADR-0015）在这里是**继承
//! 来的**，不是重新守一遍——执行不需要知道什么叫清单之外，它连那些路径都拿不到。
//!
//! 预览与执行因此不可能对不上：它们读的是同一个 [`Plan`](super::Plan) 值。
//!
//! ## 二、先写临时文件再改名
//!
//! 每一份都先落到同目录下的 `<名字>.romcat-part`，`sync_all` 之后才改名到位。三件事
//! 一起买到：
//!
//! - **中断后状态一致**：拔卡、Ctrl-C、写到一半没电，落点上要么是完整的旧文件，要么是
//!   完整的新文件，不会是半份。半份文件的可怕之处在于它的大小与时间戳看着都正常，
//!   下一趟同步会把它当成「放好了」。
//! - **`sync_all` 不能省**：目标是可移动介质。不落盘就改名，清单会记下一份还在页缓存里
//!   的文件——然后卡被拔掉。代价是每份文件多一次 flush，SD 卡上这不便宜，但「清单说有」
//!   与「卡上真有」不一致是这条链路上最不能接受的谎。
//! - **改名是同目录的**：跨目录改名可能跨设备而失败，同目录不会。
//!
//! ## 三、主库那一侧只读到底
//!
//! 读主库走 [`LibraryFs`]，那个 trait 根本没有写的办法（ADR-0004）。**连硬链接都不许
//! 对主库做**：`hard_link` 不改文件内容，但它会改源那一侧 inode 的链接数与 ctime——
//! 那是「绝不 touch」的一部分。何况真链上了更糟：子库里那份与主库里那份成了同一个 inode，
//! 掌机上改一下存档就改到了 10 TiB 主库里的原件。**ROM 一律复制**（挂账 D84）。
//!
//! ## 四、硬链接只用在媒体池 → 目标这一段
//!
//! 那是 ADR-0009 点名的那一段，也是唯一一段两头都归工具自己管的路径。同卷且支持链接
//! 就链接（零额外占用），不支持就复制——**SD 卡的 exFAT / FAT32 两者都不支持，
//! 这条降级不是优化项而是必需路径**。探测怎么做见 [`probe`]。
//!
//! ## 五、转换只产生新文件，而且默认不落第三份
//!
//! 要转格式的那几步走 [`convert::run`](crate::convert::run)：从主库那道只读接缝读进来，
//! 直接流进目标上的 `.romcat-part`。**主库一个字节不改**（ADR-0004），**中间不落盘**
//! （ADR-0017：转换产物默认不缓存——512 GB 的子库转一趟可能几小时，再占一份等同空间
//! 是灾难而不是优化）。
//!
//! 多台设备共用同一种格式时，[`Sources::convert_cache`] 给一个目录就开缓存：产物按
//! **源的键 + 源的戳 + 配方**寻址，第二台设备直接取，转一次用多次。缓存键带着源的戳，
//! 于是主库那份一改，键就变——**永远不会取到一份过期的产物**。
//!
//! ## 六、写完从目标上读回来
//!
//! 清单里记的戳是**写完之后 stat 目标**得到的那一个，不是主库侧那份的。于是 FAT32
//! 那 2 秒的时间戳刻度不构成问题：下一趟读到的是同一个被截断过的值。
//!
//! **清单里记的那条路径同理，是盘上真实的落点折出来的键**，不是步骤里那一条。
//! 目标不分大小写时，我们要的 `GB/` 可能就是卡上那个 `gb/`——`create_dir_all` 一个
//! 目录都没建，字节实实在在落在 `gb/` 里。照步骤那一条记，清单从这一刻起就在说谎，
//! 而下一趟 [`observe`](fn@super::observe) 交出来的是 `read_dir` 给的真名，于是工具
//! **认不出自己放的那一份**：报成「没了」、同时被数进清单之外。目录段折准的办法见
//! `settled`——`create_dir_all` 刚回来那一刻，列一次上一层就够，不必探文件系统。
//!
//! 排计划那一侧还有配套的一半（`sync` 模块文档「落点的目录段先与目标折齐」）：
//! 期望状态里的落点先折到盘上真实的写法，不然第二趟 `wanted` 与清单对不上，
//! 会长出「删了再加」的抖动。**这一层只保证清单不说谎**，两边都做齐才收敛。
//!
//! ## 七、新增这一步，落点上**必须是空的**
//!
//! 计划那一侧已经把「落点被占」判掉了（[`plan`](super::plan) 的函数文档），这里还要
//! 再确认一次——**两层都做**，因为它们各自堵的洞不一样：
//!
//! - 计划靠的是 [`observe`](super::observe) 交出来的那份键的集合，而那份集合**可能是
//!   不全的**：列不开的目录（[`TargetState::unlistable_dirs`](super::TargetState)）
//!   底下一个键都拿不到，那一枝上的落点计划根本无从判断。
//! - 计划算完到真的 `rename` 之间隔着整趟同步的时间，卡还插在机器上。
//!
//! 而只做这一层也不行：ADR-0016 定死**差量预览是硬要求**，一份写着「新增」、执行时
//! 却整批失败的预览本身就是谎。所以计划那一层负责**说真话**，这一层负责**兜住**。
//!
//! 判据问两遍，因为一遍答不全。[`landing`] 那一格走 [`real_path`]，而 `real_path`
//! 只折 NFC，大小写归目标文件系统自己认：**不敏感**的目标（卡上的 exFAT / FAT32、
//! Windows、macOS 默认的 APFS）上它顺带把别人那份 `GB/Tetris.zip` 也认了出来，
//! 敏感的 ext4 上它答「空的」。于是 [`occupied_by`] 折起来再问一次——折法与计划那一层
//! 同一个函数（[`path::fold`](crate::path::fold)：小写 + NFC）。**同一张卡换台机器插，
//! 行为得是同一个**：挡不住的那一边会在维护者那份旁边另写一份并报「成功」。
//!
//! 挡下来记成一条 [`Failure`] 而不是让整趟停住——与「单个文件写不进去不中断整趟」
//! 同一条纪律。
//!
//! **没有改掉 `rename` 的覆盖语义**：更新那一条要的正是「原子地换掉我们自己那一份」，
//! 而 `rename` 在 Unix 与 Windows 上都替换，那是这条链路想要的性质。`create_new`
//! 占位能把「查完到改名之间」那道缝也焊死，代价是崩在中间会在落点上留一个 0 字节、
//! 名字还正正经经的文件——那比 `.romcat-part` 难认得多，而且从此挡住这个落点。
//! 眼下这个 bug 不是竞态（是「维护者早就拷进去了」），不值得换一种新的失败方式。

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use ring::digest::{Context, SHA256};

use crate::capability::Conversion;
use crate::catalog::{Roots, mtime_ns};
use crate::convert::{self, ConvertError};
use crate::fs::{LibraryFs, real_dir, real_path};
use crate::scan::CancelToken;
use crate::scrape::pool::hex;
use crate::task::Handle;

use super::{Act, Desired, FileKind, Manifest, ManifestFile, Plan, Stamp, Step, TargetState};

/// 一次读写的块大小。与**媒体池**收字节时同一个数（`scrape::pool`）。
const CHUNK: usize = 64 * 1024;

/// 临时文件的后缀。改名到位之前它一直叫这个。
const PART: &str = ".romcat-part";

/// 这台机器上的路径分隔符。键一律用 `/`（ADR-0020），落到盘上才换成它。
const SEPARATOR: &str = std::path::MAIN_SEPARATOR_STR;

/// 连着这么多个写不进去就停下来。
///
/// 卡满了、卡被拔了、目标变成只读——这几种不是「这一个文件的问题」，一个一个试过去
/// 只会把同一句错误印上几百遍。单个文件失败照常跳过并记账（与扫描那一侧
/// 「读不到某个目录不中断整趟」同一条纪律），**连着**失败才是系统性故障的信号。
///
/// **落点被占不算进来**（[`io::ErrorKind::AlreadyExists`]）：那是**这一个落点**的确定性
/// 条件，不是「接着试也没用」的那一类。计划里新增是连在一起的（`steps` 按
/// [`Act`] 排过），维护者往卡里拷了十几个只差大小写的文件，一算进来第十条就
/// `gave_up`，其余几百步一步都不做——而那正是「撞上不许整趟停住」要防的事。
pub(super) const GIVE_UP_AFTER: u64 = 10;

/// 一份文件放到目标上的办法。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// 硬链接：同卷且目标文件系统支持，**零额外占用**。
    Link,
    /// 复制。SD 卡上的 exFAT / FAT32 只有这一条路。
    Copy,
}

impl Placement {
    /// 报告里用的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Link => "硬链接",
            Self::Copy => "复制",
        }
    }
}

/// 一步没做成。
#[derive(Debug, Clone)]
pub struct Failure {
    /// 目标上的哪个文件。
    pub path: String,
    /// 本来要干什么。
    pub act: Act,
    /// 怎么了。
    pub why: String,
}

/// 一类操作真的做成了多少。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Done {
    /// 几个文件。
    pub files: u64,
    /// 多少字节。
    pub bytes: u64,
}

/// 一趟同步做完之后的账。
#[derive(Debug, Clone)]
pub struct Outcome {
    /// 哪个子库。
    pub sublibrary: String,
    /// 目标设备上的子库根。
    pub target: String,
    /// 真的放上去了多少。
    pub added: Done,
    /// 真的重传了多少。
    pub updated: Done,
    /// 真的删掉了多少。
    pub deleted: Done,
    /// 媒体探测出来的办法；这一趟没有媒体要铺时是 `None`。
    pub placement: Option<Placement>,
    /// **媒体**里用硬链接放上去的有几份。
    ///
    /// 只数媒体：ROM 与元数据一律复制（模块文档三），把它们算进来只会让
    /// 「复制了几份」虚高，然后报告指着一条走对了的链接路径说它没生效。
    pub linked: u64,
    /// **媒体**里复制过去的有几份。
    pub copied: u64,
    /// 没做成的那几步。
    pub failures: Vec<Failure>,
    /// 这一趟是被中断的。
    pub interrupted: bool,
    /// 连着失败太多次，主动停了。
    pub gave_up: bool,
    /// 同步完之后的**清单**：目标的真实状态。
    pub manifest: Manifest,
    /// 清单里丢掉了几格——不要了而且目标上也确实没有的那些。
    pub dropped: u64,
    /// 清单里新标成「你删过、我不补」的有几格（挂账 D75）。
    pub withheld: u64,
    /// 真的**转了格式**的有几份、转出来多大（票 21）。
    pub converted: Done,
    /// 转换里有几份是**从缓存取的**，没有真转。开了缓存目录才可能不是 0。
    pub convert_cached: u64,
    /// 转换真的花了多少毫秒。
    ///
    /// 与计划里那个**粗估**摆在一起印出来：差得远就说明该去调
    /// [`Recipe`](crate::capability::Recipe) 的吞吐常量了。一个预估只有在能被回头
    /// 核对时才值得印。
    pub convert_ms: u64,
}

impl Outcome {
    /// 这一趟一共动了几个文件。
    #[must_use]
    pub fn touched(&self) -> u64 {
        self.added.files + self.updated.files + self.deleted.files
    }
}

/// 执行一趟同步要的那几样东西。
pub struct Sources<'a> {
    /// 主库的只读视图。
    pub library: &'a dyn LibraryFs,
    /// 主库那**一组根**：从变体的键第一段查出那块盘在哪。这一趟不搬 ROM 时是 `None`。
    ///
    /// 用 `Option` 而不是一份空表当哨兵：**盘不在位与「这趟不需要盘」是两件事**，
    /// 混成一个值之后，前者会变成一堆「主库里找不到 X」而不是一句「插上外置盘」。
    pub library_roots: Option<&'a Roots>,
    /// 子库根，**系统给的原始形式**（ADR-0020、挂账 D82）。
    pub target_root: &'a Path,
    /// **媒体池**里的落点：相对子库根的路径 → 池里那个文件。
    pub from_pool: &'a BTreeMap<String, PathBuf>,
    /// 生成物的字节：相对子库根的路径 → 内容。元数据走这条。
    pub generated: &'a BTreeMap<String, Vec<u8>>,
    /// 铺媒体时从哪儿探测硬链接。给 `None` 就一律复制。
    pub link_probe_dir: Option<&'a Path>,
    /// **转换缓存目录**；`None`（默认）就边转边流式写进目标，不落第三份。
    ///
    /// ADR-0017：转换很贵而缓存要再占一份等同空间，因此**默认不缓存**。给了目录才开，
    /// 用在「几台设备要的是同一种格式」那种场合。
    pub convert_cache: Option<&'a Path>,
}

/// 把计划落到目标设备上。
///
/// **严格按计划走**：一步不多、一步不少。返回的 [`Outcome`] 带着同步完之后的**清单**，
/// 调用方负责把它写回中立库——包括**被中断**的那一趟，那份清单描述的是「到中断为止
/// 目标上真实有什么」，于是下一趟接着跑就是。
///
/// ## 报进度、能停
///
/// `task` 是这一趟的**把手**（[`Handle`]）：**计划里一步就报一步**，报的正是眼下这个
/// 落点（`新增 GB/口袋妖怪.zip`）。一趟同步几十 GiB、几十分钟，只说「正在同步」等于
/// 什么都没说。
///
/// **被叫停不是错误**：这一层照旧把这一趟收完——清单要重折、要交给调用方写回中立库
/// （那份清单记的是「到中断为止目标上真实有什么」，下一趟才接得上）。
/// 于是这条线不像**差量预览**那样把 [`Halted`](crate::task::Halted) 抛出去，
/// 而是记一笔 [`Outcome::interrupted`] 照常返回——**与扫描同一条**（`scan::scan` 那几处
/// `let _ = task.step(…)` 也是这个道理：写过东西的活得留下续跑的依据才停得干净）。
///
/// 不想要把手的调用方给一个 [`Handle::new`](crate::task::Handle::new) 就行。
///
/// # Errors
/// 目标根建不出来时返回错误。单个文件写不进去**不是错误**：那一步记进
/// [`Outcome::failures`]，整趟继续（连着失败太多次才停，见 [`GIVE_UP_AFTER`]）。
pub fn run(
    plan: &Plan,
    desired: &Desired,
    actual: &TargetState,
    previous: &Manifest,
    sources: &Sources<'_>,
    task: &Handle,
) -> io::Result<Outcome> {
    // **一步内部也停得动**：转一个 40 GiB 的镜像、拷一份 8 GiB 的 ISO 都在一步里头，
    // 只在两步之间看一眼的话，「停下」按下去要等上几分钟。
    let cancel = task.cancel();
    task.steps(u32::try_from(plan.steps.len()).unwrap_or(u32::MAX));

    // **一个字节都没写之前先看一眼有没有被叫停。** 底下这两样都排在第一步之前，
    // 而它们都在目标设备上留痕：建目标根会在卡上留一个空目录（子库根还不存在时），
    // 硬链接探测会在卡上建一份探针文件再删掉（删不掉就留下了）。
    // 「停下来的地方是干净的」得从这儿算起，不是从第一步算起。
    let 已经叫停了 = task.check().is_err();
    if !已经叫停了 {
        std::fs::create_dir_all(sources.target_root)?;
    }

    let wants_media = plan
        .steps
        .iter()
        .any(|step| step.kind == FileKind::Media && step.act != Act::Delete);
    let placement = if 已经叫停了 {
        // 一步都不会做，就别为了「用链接还是复制」去碰那张卡。
        None
    } else {
        match (wants_media, sources.link_probe_dir) {
            (true, Some(from)) => Some(probe(from, sources.target_root)),
            (true, None) => Some(Placement::Copy),
            (false, _) => None,
        }
    };

    let mut out = Outcome {
        sublibrary: plan.sublibrary.clone(),
        target: plan.target.clone(),
        added: Done::default(),
        updated: Done::default(),
        deleted: Done::default(),
        placement,
        linked: 0,
        copied: 0,
        failures: Vec::new(),
        interrupted: false,
        gave_up: false,
        manifest: Manifest::empty(),
        dropped: 0,
        withheld: 0,
        converted: Done::default(),
        convert_cached: 0,
        convert_ms: 0,
    };
    let mut done: BTreeMap<String, ManifestFile> = BTreeMap::new();
    let mut removed: BTreeSet<String> = BTreeSet::new();
    let mut consecutive = 0_u64;
    // 认出来的目录真名（[`settled`]）：一个平台目录只列一次。
    let mut dirs: BTreeMap<String, PathBuf> = BTreeMap::new();

    for step in &plan.steps {
        // **报的是这一个落点，不是一句「正在同步」。** `step` 顺带就是那个分界处：
        // 被叫停时这一步一件事都还没做，于是停下来的地方永远在两个文件之间。
        if task
            .step(&format!("{} {}", step.act.label(), step.path))
            .is_err()
        {
            out.interrupted = true;
            break;
        }
        let outcome = match step.act {
            Act::Delete => erase(sources, step).map(|()| None),
            Act::Add | Act::Update => {
                place(sources, &mut dirs, step, placement, cancel, &mut out).map(Some)
            }
        };
        match outcome {
            Ok(None) => {
                consecutive = 0;
                removed.insert(step.path.clone());
                out.deleted.files += 1;
                out.deleted.bytes += step.was;
            }
            Ok(Some((path, stamp, how))) => {
                consecutive = 0;
                if step.convert.is_some() {
                    out.converted.files += 1;
                    out.converted.bytes += stamp.bytes;
                }
                if step.kind == FileKind::Media {
                    match how {
                        Placement::Link => out.linked += 1,
                        Placement::Copy => out.copied += 1,
                    }
                }
                let account = if step.act == Act::Add {
                    &mut out.added
                } else {
                    &mut out.updated
                };
                account.files += 1;
                account.bytes += stamp.bytes;
                done.insert(path.clone(), recorded(step, path, stamp));
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                // 写到一半收到中断：临时文件已经清掉了，落点上还是原来那一份。
                out.interrupted = true;
                break;
            }
            Err(error) => {
                // 落点被占**不进这个计数**（见 [`GIVE_UP_AFTER`]）：它是这一个落点的
                // 确定性条件，接着往下试是有意义的。也不清零——清零会让真正的系统性
                // 故障被夹在中间的占用冲淡，而「连着」这个词说的正是不被冲淡。
                if error.kind() != io::ErrorKind::AlreadyExists {
                    consecutive += 1;
                }
                out.failures.push(Failure {
                    path: step.path.clone(),
                    act: step.act,
                    why: format!("{error}"),
                });
                if consecutive >= GIVE_UP_AFTER {
                    out.gave_up = true;
                    break;
                }
            }
        }
    }

    let (manifest, dropped, withheld) = rebuild(previous, desired, actual, &done, &removed);
    out.manifest = manifest;
    out.dropped = dropped;
    out.withheld = withheld;
    Ok(out)
}

/// 删掉目标上的一份文件。
fn erase(sources: &Sources<'_>, step: &Step) -> io::Result<()> {
    let Some(at) = on_target(sources, &step.path) else {
        // 计划算出来的时候它还在。这一瞬间没了也不是灾难：要的结果本来就是「它不在」。
        return Ok(());
    };
    std::fs::remove_file(&at)
}

/// 把一份文件放到目标上。返回**盘上真实落点**折出来的键、**从目标上读回来的**戳，
/// 以及实际用的办法。
fn place(
    sources: &Sources<'_>,
    dirs: &mut BTreeMap<String, PathBuf>,
    step: &Step,
    placement: Option<Placement>,
    cancel: &CancelToken,
    out: &mut Outcome,
) -> io::Result<(String, Stamp, Placement)> {
    let (target, taken) = landing(sources, dirs, &step.path);
    // **最后一道防线**（模块文档七）：新增这一步的落点上不该有任何东西。
    //
    // 问两遍，因为一遍答不全：`landing` 那一问在**大小写不敏感**的目标上连别人那份
    // 也认得出（`real_path` 拿 `GB/tetris.zip` 就开得了 `GB/Tetris.zip`），可在
    // **大小写敏感**的盘上它一无所知——同一张卡插在 Windows 上挡得住、插在 Linux 上
    // 就在维护者那份旁边另写一份并报「成功」。于是折起来再问一次（`occupied_by`）。
    //
    // **先问折起来那一问，哪怕逐字那一问已经说「占着了」**：`landing` 交回来的路径是
    // `根 + 键` 拼出来的（`real_path` 的快路径就是直接拼），大小写还是我们自己那份，
    // 于是在**卡上**——正是这条缺陷的主场景——报出来会是「`GB/tetris.zip` 的落点上
    // 已经有东西了（…/GB/tetris.zip）」，两条一模一样，等于没说出是谁占着。
    // `occupied_by` 的答案来自 listing，那才是盘上真实那个名字。
    if step.act == Act::Add {
        let blocking = occupied_by(sources, &step.path).or_else(|| taken.then(|| target.clone()));
        if let Some(at) = blocking {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "{} 的落点上已经有东西了（{}）：清单之外的文件一律不碰（ADR-0015）",
                    step.path,
                    crate::path::display(&at),
                ),
            ));
        }
    }
    let parent = target.parent().unwrap_or(sources.target_root).to_path_buf();
    std::fs::create_dir_all(&parent)?;
    // 目录这一刻一定在盘上了：把落点的目录段换成它真实的那个名字（见 [`settled`]）。
    // **已经占着的那一份不动**：那条路径是 [`on_target`] 从盘上认回来的，它自己就是真的。
    let target = if taken {
        target
    } else {
        settled(sources, dirs, &step.path, target)
    };
    let temp = part_path(&target);
    // 上一趟被打断留下的半份：`create` 会截断它，但 `hard_link` 不会——先清掉。
    let _ = std::fs::remove_file(&temp);

    let how = match step.kind {
        FileKind::Metadata => {
            let bytes = sources
                .generated
                .get(&step.path)
                .ok_or_else(|| io::Error::other(format!("{} 的内容没折出来", step.path)))?;
            write_all(&temp, bytes)?;
            Placement::Copy
        }
        FileKind::Media => {
            let from = sources
                .from_pool
                .get(&step.path)
                .ok_or_else(|| io::Error::other(format!("{} 在媒体池里找不到落点", step.path)))?;
            match placement.unwrap_or(Placement::Copy) {
                // 链接成不了就当场降级复制：探测说得中不等于每一份都成
                // （目标上那一枝可能挂在别的卷上）。
                Placement::Link => match std::fs::hard_link(from, &temp) {
                    Ok(()) => Placement::Link,
                    Err(_) => {
                        let _ = std::fs::remove_file(&temp);
                        copy_local(from, &temp, cancel)?;
                        Placement::Copy
                    }
                },
                Placement::Copy => {
                    copy_local(from, &temp, cancel)?;
                    Placement::Copy
                }
            }
        }
        FileKind::Rom => {
            // **主库只读**：走那道没有写操作的接缝，而且**只复制不链接**（模块文档三）。
            let roots = sources
                .library_roots
                .ok_or_else(|| io::Error::other("这一趟要搬 ROM，可调用方没说主库在哪"))?;
            let from = roots
                .real_path(sources.library, &step.source)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("主库里找不到 {}", step.source),
                    )
                })?;
            match &step.convert {
                // 要转格式：读主库那份原始形态，写出一份**新文件**（ADR-0004）。
                Some(conversion) => {
                    let started = std::time::Instant::now();
                    convert_into(sources, step, conversion, &from, &temp, cancel, out)?;
                    out.convert_ms += u64::try_from(started.elapsed().as_millis()).unwrap_or(0);
                }
                None => {
                    let mut reader = sources.library.open(&from)?;
                    copy_stream(&mut reader, &temp, cancel)?;
                }
            }
            Placement::Copy
        }
    };

    if let Err(error) = std::fs::rename(&temp, &target) {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    let meta = std::fs::metadata(&target)?;
    Ok((
        crate::path::catalog_key(sources.target_root, &target),
        Stamp {
            bytes: meta.len(),
            mtime_ns: meta.modified().ok().and_then(mtime_ns),
        },
        how,
    ))
}

/// 转一份出来，落到目标上那个临时文件。
///
/// 没开缓存（默认）就直接转进 `temp`——**边转边流式写入目标，中间不落第三份**
/// （ADR-0017）。开了缓存就先在缓存里做一份，再从缓存链接或复制到目标；第二台设备
/// 要同一份产物时直接命中，转一次用多次。
fn convert_into(
    sources: &Sources<'_>,
    step: &Step,
    conversion: &Conversion,
    from: &Path,
    temp: &Path,
    cancel: &CancelToken,
    out: &mut Outcome,
) -> io::Result<()> {
    let to_io = |error: ConvertError| -> io::Error {
        if error.is_interrupted() {
            io::Error::from(io::ErrorKind::Interrupted)
        } else {
            io::Error::other(format!("{} 转不出来：{error}", step.source))
        }
    };
    let Some(cache) = sources.convert_cache else {
        convert::run(sources.library, from, conversion, temp, cancel).map_err(to_io)?;
        return Ok(());
    };

    let cached = cache_path(cache, step, conversion);
    if !cached.is_file() {
        let parent = cached.parent().unwrap_or(cache);
        std::fs::create_dir_all(parent)?;
        let staging = part_path(&cached);
        let _ = std::fs::remove_file(&staging);
        convert::run(sources.library, from, conversion, &staging, cancel).map_err(to_io)?;
        // 先落到 `.romcat-part` 再改名：中断留下的半份产物绝不能被下一趟当成缓存命中。
        if let Err(error) = std::fs::rename(&staging, &cached) {
            let _ = std::fs::remove_file(&staging);
            return Err(error);
        }
    } else {
        out.convert_cached += 1;
    }
    // 缓存与目标同卷就链接（零额外占用），不同卷（卡就是不同卷）落回复制。
    match std::fs::hard_link(&cached, temp) {
        Ok(()) => Ok(()),
        Err(_) => {
            let _ = std::fs::remove_file(temp);
            copy_local(&cached, temp, cancel)
        }
    }
}

/// 一份转换产物在缓存里叫什么。
///
/// 键里**必须有源的戳**：主库那份一改键就变，于是永远取不到一份过期的产物。
/// 分两级目录与**媒体池**同一条理由——几万份文件平铺在一个目录里，
/// 在 FAT 系文件系统上列一次目录就是灾难。
fn cache_path(cache: &Path, step: &Step, conversion: &Conversion) -> PathBuf {
    let mut context = Context::new(&SHA256);
    context.update(conversion.recipe.label().as_bytes());
    context.update(b"\0");
    context.update(step.source.as_bytes());
    context.update(b"\0");
    context.update(&step.source_stamp.bytes.to_le_bytes());
    context.update(&step.source_stamp.mtime_ns.unwrap_or(-1).to_le_bytes());
    context.update(&(conversion.inner.unwrap_or(usize::MAX) as u64).to_le_bytes());
    let hash = hex(context.finish().as_ref());
    let extension = conversion
        .path
        .rsplit_once('.')
        .map_or_else(String::new, |(_, ext)| format!(".{ext}"));
    cache.join(&hash[..2]).join(format!("{hash}{extension}"))
}

/// 临时文件叫什么。**同目录**：跨目录改名可能跨设备而失败。
fn part_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(PART);
    target.with_file_name(name)
}

/// 把一份现成的字节落到临时文件上。
fn write_all(temp: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut sink = std::fs::File::create(temp)?;
    if let Err(error) = sink.write_all(bytes).and_then(|()| sink.sync_all()) {
        drop(sink);
        let _ = std::fs::remove_file(temp);
        return Err(error);
    }
    Ok(())
}

/// 从本机的另一个文件复制过来（媒体池那一段）。
fn copy_local(from: &Path, temp: &Path, cancel: &CancelToken) -> io::Result<()> {
    let mut reader = std::fs::File::open(from)?;
    copy_stream(&mut reader, temp, cancel)
}

/// 一块一块地复制，**每块之间看一眼有没有被中断**。
///
/// 不逐块看的话，一份 662 MiB 的预览视频会让 Ctrl-C 等上几十秒——而在可移动介质上，
/// 「停不下来」会让人去拔卡。中断时把临时文件删掉：落点上还是原来那一份。
fn copy_stream(reader: &mut dyn Read, temp: &Path, cancel: &CancelToken) -> io::Result<()> {
    let mut sink = std::fs::File::create(temp)?;
    let mut buf = vec![0_u8; CHUNK];
    let result = (|| -> io::Result<()> {
        loop {
            if cancel.is_cancelled() {
                return Err(io::Error::from(io::ErrorKind::Interrupted));
            }
            let got = reader.read(&mut buf)?;
            if got == 0 {
                break;
            }
            sink.write_all(&buf[..got])?;
        }
        sink.sync_all()
    })();
    if let Err(error) = result {
        drop(sink);
        let _ = std::fs::remove_file(temp);
        return Err(error);
    }
    Ok(())
}

/// 目标上这条路径**真实存在的**那个名字；不在就是 `None`。
///
/// 走 [`real_path`] 而不是直接拼：清单里存的是 NFC 的键，而目标上那个名字在
/// 分解敏感的文件系统上可能是分解形式（ADR-0020）。拼出来打不开会被当成
/// 「它已经不在了」——而在删除这一侧，那意味着**该删的没删**。
fn on_target(sources: &Sources<'_>, key: &str) -> Option<PathBuf> {
    real_path(&crate::fs::RealFs, sources.target_root, key)
}

/// 这一份该**落在**目标上的哪条路径，以及**那儿现在是不是已经有东西了**。
///
/// ADR-0020 在写这一侧同样成立，而且分两半：
///
/// - **已经在了就用它自己在盘上的名字**。目标把名字存成分解形式（HFS+ 就会）而查找
///   又分解敏感时，拿 NFC 的键去 `rename`，会在维护者那份**旁边新造一份**，旧那份留在
///   卡上从此变成「清单之外」——票 16 刚被这个 bug 咬过（挂账 D82）。
/// - **还不在就用我们自己选的那个名字**（NFC 的键），目录先用 `dirs` 里已经认出来的
///   那个真名，没认过就照键拼——目录这一段真正折准是在 `create_dir_all` **之后**，
///   见 [`settled`]。这里拼出来的只是「往哪儿建目录」。
///
/// 第一格就是那道最后防线的依据：[`real_path`] 在**大小写不敏感**的目标上，
/// 拿 `GB/tetris.zip` 也开得了别人那份 `GB/Tetris.zip`，于是它答的正是
/// 「这条键会落到一个已经存在的文件上吗」——[`place`] 拿它挡住新增（模块文档七）。
fn landing(sources: &Sources<'_>, dirs: &BTreeMap<String, PathBuf>, key: &str) -> (PathBuf, bool) {
    if let Some(at) = on_target(sources, key) {
        return (at, true);
    }
    let Some((dir, name)) = key.rsplit_once('/') else {
        return (sources.target_root.join(key), false);
    };
    let parent = dirs
        .get(dir)
        .cloned()
        .unwrap_or_else(|| sources.target_root.join(dir.replace('/', SEPARATOR)));
    (parent.join(name), false)
}

/// 目标上有没有一个东西**折起来**占着这条落点；有的话，它在盘上真实的那条路径。
///
/// [`landing`] 那一问答不全这件事：它走 [`real_path`]，而 `real_path` 只折 NFC
/// （ADR-0020），大小写归目标文件系统自己认。卡是 exFAT / FAT32、主力机是 Windows、
/// macOS 默认的 APFS 也一样——**这几个都不敏感**，于是那一问在它们上面顺带把
/// `GB/Tetris.zip` 也认了出来；可同一份代码也跑在 ext4 上（ADR-0018），那儿
/// `GB/tetris.zip` 与 `GB/Tetris.zip` 是两个文件，那一问答「空的」，工具就在维护者
/// 那份旁边另写一份并报「成功」——**同一张卡换台机器插就换个行为**。
///
/// 这里补上的正是那半边：折法用 [`path::fold`](crate::path::fold)（小写 + NFC），
/// 与[计划那一层](super::plan)**同一个**函数。两处各写一套折法的话，会长出
/// 「计划说没事、执行却顶掉了」——那正是这条缺陷的形状。
///
/// **折的是整条键，不是最后那一段**：计划那一侧折的是 `gb/Tetris.zip` 这一整条，
/// 上一级目录只差大小写照样算撞上，于是这里也逐段折着往下走。
///
/// 大小写敏感的目标上这是**保守误报**（两份真能并存的文件只落一份），取舍与计划那一层
/// 同一笔账：误报的代价是少放一个文件加一条没做成，判反了的代价是维护者的东西没了。
///
/// **只用来发现挡路的东西，绝不用来认领它**：这个答案只挡[新增](Act::Add)。更新与删除
/// 照旧要求一模一样的键——那两条会动别人的文件，而「折起来一样」证明不了「就是我们放的
/// 那一份」。改大小写重落一份那种改名不会被它误伤：计划里删除排在新增前面
/// （`steps` 按 [`Act`] 排过），轮到新增时旧那份已经删掉了。
///
/// **一层里折起来一样的可能不止一个，于是每一层都得全都跟下去**：大小写敏感的盘上
/// `GB/` 与 `gb/` 能同时在（一个是工具写的，一个是维护者建的），只跟排在前面那个的话，
/// 另一枝底下挡路的那份就看不见了。挡路的答案取全部候选里排下来最先那个：报出来的是
/// 挡路的证据，谁挡的都一样，而这个选择要是确定的（与 [`plan`](super::plan) 那一侧
/// 「留最先那个」同一条口径）。
fn occupied_by(sources: &Sources<'_>, key: &str) -> Option<PathBuf> {
    let mut at = vec![sources.target_root.to_path_buf()];
    for segment in key.split('/') {
        if segment.is_empty() {
            continue;
        }
        let mut next: Vec<PathBuf> = at
            .iter()
            .flat_map(|dir| folded_children(dir, segment))
            .collect();
        if next.is_empty() {
            return None;
        }
        next.sort();
        next.dedup();
        at = next;
    }
    at.into_iter().next()
}

/// `dir` 这一层里，折起来与 `segment` 一样的那些名字，在盘上真实的那几条路径。
///
/// 一律整层列出来：要认的正是「盘上那个名字与我们要写的只差大小写」，而只有 listing
/// 交得出盘上真实那个名字。名字读不出 UTF-8 的条目跳过——折不了的东西也就无从比对。
///
/// **列不开就当这一层没有挡路的，而这是一个已知的洞**：Unix 上一个 `0300` 的目录列不开
/// 却写得进（`read_dir` 失败，按名字 `open` 照样成功），而模块文档七点名
/// [`TargetState::unlistable_dirs`](super::TargetState) 正是执行这一层非有不可的头号
/// 理由。漏的那一格窄得很，也**不丢数据**：逐字那半边不受影响（`real_path` 直接拼那一下
/// 在列不开的目录里照样开得了文件），于是大小写不敏感的目标——卡、Windows、APFS
/// ——上挡得住；剩下的只有「大小写敏感的盘 + 列不开的目录 + 只差大小写的占用」那一种，
/// 后果是在维护者那份**旁边**多写一份（那盘上两份本来就能并存），不是顶掉它。
/// 要补上只能把「列不开」与「列开了没找到」分成两态一路带上来，那是另一张票的形状。
fn folded_children(dir: &Path, segment: &str) -> Vec<PathBuf> {
    let wanted = crate::path::fold(segment);
    crate::fs::RealFs
        .read_dir(dir)
        .unwrap_or_default()
        .into_iter()
        .map(|entry| entry.path)
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| crate::path::fold(name) == wanted)
        })
        .collect()
}

/// 目录建好之后，把落点的**目录段**换成盘上**字节级真实**的那个名字。
///
/// 只有走到这里才答得准，而这一刻答得准是**免费**的：`create_dir_all` 刚刚回来，
/// 那个目录**一定在盘上**了，于是列一次它的上一层就够——
///
/// - 大小写敏感的目标上，`create_dir_all` 真的建出了 `GB/`，逐字那个名字就在那儿；
/// - 不分大小写的目标上它一个目录都没建（`gb/` 本来就在），列出来只有 `gb`，
///   那就是我们这一份真正的落点。
///
/// 判据因此**不必探文件系统、也不必猜**：[`real_dir`] 按 `read_dir` 的真实结果走。
///
/// 这一段答准了，[`place`] 才有资格拿落点去折**清单的键**——而清单的键必须与下一趟
/// [`observe`](super::observe) 交出来的那条一模一样，不然工具从第二趟起就认不出自己
/// 放的那一份（`sync` 模块文档「落点的目录段先与目标折齐」）。
///
/// 认出来的真名记在 `dirs` 里：一个子库几千份文件挤在同几个平台目录下，
/// 不记的话同一层会被列上几千遍。**这一趟里目录不会改名**，于是这份记性只增不改。
fn settled(
    sources: &Sources<'_>,
    dirs: &mut BTreeMap<String, PathBuf>,
    key: &str,
    fallback: PathBuf,
) -> PathBuf {
    let Some((dir, name)) = key.rsplit_once('/') else {
        return fallback;
    };
    if let Some(at) = dirs.get(dir) {
        return at.join(name);
    }
    let Some(at) = real_dir(&crate::fs::RealFs, sources.target_root, dir) else {
        return fallback;
    };
    dirs.insert(dir.to_string(), at.clone());
    at.join(name)
}

/// 一条写成了的步骤在清单里长什么样。
///
/// `path` 是**盘上真实落点**折出来的键，不一定是 [`Step::path`](super::Step)——
/// 见 [`settled`]。
fn recorded(step: &Step, path: String, stamp: Stamp) -> ManifestFile {
    ManifestFile {
        path,
        kind: step.kind,
        stamp,
        source: step.source.clone(),
        source_stamp: step.source_stamp,
        variant: step.variant.clone(),
        absent: false,
    }
}

/// 折出同步完之后的**清单**：目标的真实状态。
///
/// 四条规矩，每条防一种谎：
///
/// - **这一趟写成了的**：记从目标上读回来的那个戳。
/// - **这一趟删掉了的**：整条丢掉。
/// - **没碰的、目标上还是我们放的那一份**：原样留着。
/// - **没碰的、目标上没有了**：还要它就标成 `absent`（你删过、我不补，挂账 D75），
///   不要它了就整条丢掉——记着一个「不在目标上、也不再要」的路径，只会让同一条
///   「没了」每一趟都被报一遍。
///
/// **目标上被人改过的那些不改戳**：戳记的是「工具放上去的是什么样」，拿目标上现在
/// 那份去覆盖它，等于工具认领了别人的改动，下一趟就会把维护者亲手换上去的那份当成
/// 自己的东西删掉。
fn rebuild(
    previous: &Manifest,
    desired: &Desired,
    actual: &TargetState,
    done: &BTreeMap<String, ManifestFile>,
    removed: &BTreeSet<String>,
) -> (Manifest, u64, u64) {
    let wanted: BTreeSet<&str> = desired
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    let on_target: BTreeMap<&str, Option<Stamp>> = actual
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.stamp))
        .collect();

    let mut out = Manifest::empty();
    let mut dropped = 0;
    let mut withheld = 0;
    for file in &previous.files {
        if removed.contains(&file.path) || done.contains_key(&file.path) {
            continue;
        }
        let mut kept = file.clone();
        match on_target.get(file.path.as_str()) {
            // 还在，而且还是我们放的那一份。
            Some(Some(stamp)) if file.stamp.proves_same(stamp) => kept.absent = false,
            // 还有东西在那儿，但对不上、或者读不到：**不改结论**，戳仍然记着我们放的那份。
            Some(_) => {}
            None => {
                if !wanted.contains(file.path.as_str()) {
                    dropped += 1;
                    continue;
                }
                kept.absent = true;
            }
        }
        if kept.absent {
            withheld += 1;
        }
        out.files.push(kept);
    }
    out.files.extend(done.values().cloned());
    out.files.sort_by(|a, b| a.path.cmp(&b.path));
    (out, dropped, withheld)
}

/// 探一探：从 `from` 这个目录往 `to` 这个目录建得出硬链接吗。
///
/// **两头都是工具自己的地盘**：`from` 是**媒体池**的临时目录，`to` 是子库根。
/// 拿主库里的文件当链接源是不行的——`hard_link` 会改到源那一侧 inode 的链接数与 ctime，
/// 而主库连时间戳都不许碰（ADR-0004）。
///
/// 探测本身要写两个临时文件，两个都在同一趟里删掉。探不动（建不出探测文件）时一律
/// 报**复制**：探测失败不该让同步停下来，降级复制在任何文件系统上都成立。
#[must_use]
pub fn probe(from: &Path, to: &Path) -> Placement {
    let name = format!(".romcat-link-probe-{}", std::process::id());
    let source = from.join(&name);
    let link = to.join(&name);
    if std::fs::create_dir_all(from).is_err() || std::fs::write(&source, b"romcat").is_err() {
        return Placement::Copy;
    }
    let _ = std::fs::remove_file(&link);
    let placement = if std::fs::hard_link(&source, &link).is_ok() {
        Placement::Link
    } else {
        Placement::Copy
    };
    let _ = std::fs::remove_file(&link);
    let _ = std::fs::remove_file(&source);
    placement
}
