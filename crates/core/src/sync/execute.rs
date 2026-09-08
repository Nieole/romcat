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
//! 预览与执行因此不可能对不上：它们读的是同一个 [`Plan`] 值。
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
//! 要转格式的那几步走 [`convert::run`]：从主库那道只读接缝读进来，
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
//! - 计划靠的是 [`observe`](super::observe()) 交出来的那份键的集合，而那份集合**可能是
//!   不全的**：列不开的目录（[`TargetState::unlistable_dirs`](super::TargetState)）
//!   底下一个键都拿不到，那一枝上的落点计划根本无从判断。**执行这一层对那一枝的答案
//!   是「答不出来」**，不是「空的」——见下面那一节。
//! - 计划算完到真的 `rename` 之间隔着整趟同步的时间，卡还插在机器上。
//!
//! 而只做这一层也不行：ADR-0016 定死**差量预览是硬要求**，一份写着「新增」、执行时
//! 却整批失败的预览本身就是谎。所以计划那一层负责**说真话**，这一层负责**兜住**。
//!
//! 判据问两遍，因为一遍答不全。`landing` 那一格走 [`real_path`]，而 `real_path`
//! 只折 NFC，大小写归目标文件系统自己认：**不敏感**的目标（卡上的 exFAT / FAT32、
//! Windows、macOS 默认的 APFS）上它顺带把别人那份 `GB/Tetris.zip` 也认了出来，
//! 敏感的 ext4 上它答「空的」。于是 `occupancy` 折起来再问一次——折法与计划那一层
//! 同一个函数（[`path::fold`](crate::path::fold)：小写 + NFC）。**同一张卡换台机器插，
//! 行为得是同一个**：挡不住的那一边会在维护者那份旁边另写一份并报「成功」。
//!
//! **折起来那一问不是两态**（`Occupancy`）：「有东西占着」「什么都没有」之外，还有
//! **列不开**——ADR-0021 的第三态。Unix 上一个 `0300` 的目录 `read_dir` 失败、按名字
//! `open` 照样成功；从前它被折进了「什么都没有」，于是上面那条头号理由在实现里是空的，
//! 大小写敏感的盘上会在维护者那份旁边多写一份只差大小写的。**答不出来就不写。**
//!
//! 问完这两遍还有**第三问**（`dir_blocked`）：**我们真要建的那条目录路径上，躺着个不是
//! 目录的东西吗。** 卡上有个文件叫 `GB`、这一趟要往 `GB/` 底下写十几份——目录根本建不
//! 出来，那一枝底下每一条新增都会以同一句话失败。它问的是**我们自己那条落点**，
//! 不是折起来撞上的别的枝：大小写敏感的盘上 `GB/sub` 是个文件，不代表 `gb/sub`
//! 建不出来，照那边的答案挡下来是误报——而这一格是**计数**的，误报十条就把整趟停住。
//!
//! 挡下来记成一条 [`Failure`] 而不是让整趟停住——与「单个文件写不进去不中断整趟」
//! 同一条纪律。**但挡下来的那几种之间要分开**（`Refusal`）：落点被占与读不动是
//! **这一个落点**的确定性条件，接着往下试有意义，不进「连着失败就停下来」那个计数；
//! 目录段上躺着个文件是**这一枝**的条件，底下每一条都会以同一句话失败，要进。
//! 从前那条豁免认的是 [`io::ErrorKind::AlreadyExists`]，而 `create_dir_all` 撞上同名
//! 文件报的正是那一种——于是后一格连计数都不涨，放弃机制永不触发。
//!
//! **没有改掉 `rename` 的覆盖语义**：更新那一条要的正是「原子地换掉我们自己那一份」，
//! 而 `rename` 在 Unix 与 Windows 上都替换，那是这条链路想要的性质。`create_new`
//! 占位能把「查完到改名之间」那道缝也焊死，代价是崩在中间会在落点上留一个 0 字节、
//! 名字还正正经经的文件——那比 `.romcat-part` 难认得多，而且从此挡住这个落点。
//! 眼下这个 bug 不是竞态（是「维护者早就拷进去了」），不值得换一种新的失败方式。
//!
//! ## 八、问目标的那几句话走一道**可注入的**接缝
//!
//! 上面那道闸问目标三句：**这条键在盘上是哪一条**（`on_target`，删除那一步找盘上真名
//! 走的也是它）、**折起来有没有东西占着**（`occupancy`）、**目录段真名是什么**
//! （`settled`）。三句都从 [`Sources::target`] 那道 [`LibraryFs`] 上问——与[看一遍
//! 目标](fn@super::observe)同一个 trait、同一条纪律：那个 trait 根本没有写的办法
//! （ADR-0004 的做法）。
//!
//! **为什么非有它不可**：这道闸的正确性在**两种折叠语义**上不是同一件事，而一台机器上
//! 只有一种。不分大小写的目标（ADR-0015 定的 exFAT / FAT32、ADR-0018 那台默认 APFS 的
//! 主力机、Windows）上，卡里那份 `GB/Tetris.zip` 与我们要写的 `GB/tetris.zip`
//! **就是同一个文件**——挡不住就是把维护者的东西顶掉；分大小写的 ext4 上它们是两个
//! 文件，挡不住只是在旁边多写一份。开发机造不出前一种挂载点，于是「敏感那边挡得住，
//! 不敏感那边只会更容易挡住」这句话挂了两轮都只是**推理**（挂单 `Q135`）。这是整条
//! 链路上唯一会弄丢维护者数据的地方，推理不够。造视图见
//! [`testing::target`](crate::testing::target)。
//!
//! **它不是什么**：
//!
//! - **不是一层通用的文件系统抽象。** 接缝只加在 [`run`] 对外那**一个**入口上，没有
//!   下沉到每一次文件操作：写那一侧——建目录、落 `.romcat-part`、`sync_all`、
//!   `rename`、写完读回戳——照旧是 `std::fs`，一个字节都不经过它。真下沉下去，测试
//!   就会在一份假的盘上验「原子改名」与「`sync_all` 不能省」，而那两条防的正是**真盘
//!   上**的断电与拔卡（本模块第二节），在内存里验等于没验。
//! - **不许拿它去替换别处的真实文件访问。** 主库那一侧有它自己的接缝
//!   （[`Sources::library`]，ADR-0004）；**媒体池**、转换缓存、硬链接探测都是工具自己
//!   的地盘，一律走真盘。
//! - **不是挂缓存的地方。** 一次落点判定之内共享的那份 listing（前两句走的是同一批
//!   目录）建在**那一次判定的栈上**——`Cached` 套在这个字段外面，活到 `place`
//!   返回为止。塞进这个字段的话作用域就成了整趟，而这道闸防的正是「计划算完到真的
//!   改名之间，卡还插在机器上」，一份跑到一半就过期的 listing 会把那道缝重新打开
//!   （挂单 `Q134`）。
//! - **不注入就是真盘**：调用方给 [`RealFs`](crate::fs::RealFs) 就是原来那条路，
//!   命令行与界面都这么给。

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ring::digest::{Context, SHA256};

use crate::capability::Conversion;
use crate::catalog::{Roots, mtime_ns};
use crate::convert::{self, ConvertError};
use crate::fs::{DirEntry, LibraryFs, ReadSeek, real_dir, real_path};
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
/// **算不算进来由 [`Refusal`] 说了算，不由错误种类说了算。** 落点被占、落点这一枝
/// 读不动——那是**这一个落点**的确定性条件，接着往下试有意义，不算：计划里新增是连在
/// 一起的（`steps` 按 [`Act`] 排过），维护者往卡里拷了十几个只差大小写的文件，
/// 一算进来第十条就 `gave_up`，其余几百步一步都不做，而那正是「撞上不许整趟停住」
/// 要防的事。**「该建目录的位置上躺着个文件」要算**：那一格底下每一条新增都会以同一句
/// 话失败，接着试没有任何意义，正是这个计数要认出来的那一类。
///
/// 从前这条豁免认的是 [`io::ErrorKind::AlreadyExists`]，而 `create_dir_all` 撞上一个
/// 同名文件报的正是那一种——于是上面那一格连计数都不涨，放弃机制永不触发。
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
    /// **目标设备的只读视图**：那道落点闸问盘的三句话走它（模块文档八）。
    ///
    /// 真跑一律给 [`RealFs`](crate::fs::RealFs)。测试拿一份指定**折叠语义**的
    /// [`MemFs`](crate::fs::MemFs) 塞进来，就能在一台造不出那种挂载点的机器上验
    /// 「不分大小写的卡上照样挡得住」——见 [`testing::target`](crate::testing::target)。
    ///
    /// **只读、而且只管读**：写那一侧照旧是 `std::fs`。它与 [`Self::library`] 是两道
    /// 接缝、两个东西——一道对着 10 TB 的主库，一道对着手里那张卡。
    pub target: &'a dyn LibraryFs,
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
/// 交出去的产物长得跟「跑完了」那一份一模一样，所以这一层被叫停时还会往把手上报一句
/// **停在半路**（[`Handle::halfway`]）：任务台照它把这一趟记成
/// [`Ending::Halfway`](crate::task::Ending::Halfway)，历史里那一行才不会写成「完成」。
///
/// 不想要把手的调用方给一个 [`Handle::new`](crate::task::Handle::new) 就行。
///
/// # Errors
/// 目标根建不出来时返回错误。单个文件写不进去**不是错误**：那一步记进
/// [`Outcome::failures`]，整趟继续（连着失败太多次才停，见 `GIVE_UP_AFTER`）。
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
            Act::Delete => erase(sources, step).map(|()| None).map_err(Refusal::Failed),
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
            Err(Refusal::Failed(error)) if error.kind() == io::ErrorKind::Interrupted => {
                // 写到一半收到中断：临时文件已经清掉了，落点上还是原来那一份。
                out.interrupted = true;
                break;
            }
            Err(refusal) => {
                // **认的是「这一步是被那道闸挡下来的哪一种」，不是错误种类**
                // （见 [`GIVE_UP_AFTER`]、[`Refusal::counted`]）。不算进来的那几种
                // 也**不清零**——清零会让真正的系统性故障被夹在中间的占用冲淡，
                // 而「连着」这个词说的正是不被冲淡。
                if refusal.counted() {
                    consecutive += 1;
                }
                out.failures.push(Failure {
                    path: step.path.clone(),
                    act: step.act,
                    why: refusal.why(&step.path),
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
    if out.interrupted {
        // **停在半路**：这一趟照旧交出产物，可它只走了一半——说出口，任务台才
        // 记得对（`Handle::halfway`）。不说的话那份清单会被当成「跑完了」的那一份，
        // 历史里写「完成」，而子库屏同时说「⚠️ 这一趟被你按停了」。
        task.halfway(left_behind(out.touched()));
    }
    Ok(out)
}

/// 一趟**停在半路**的同步留下了什么，一句话。任务屏历史那一行画的就是它。
///
/// **「为什么收的手」也在这句话里**：`Ending::Halfway` 那一层不替长入口写死它
/// （[`Handle::halfway`]），而这条路上收手的理由只有一个——人按了停下。
///
/// 清单永远在——它非落库不可（ADR-0015），所以哪怕一件都没落也得说清这一点：
/// 「什么都没留下」是另一档，那一档可以当没跑过。
fn left_behind(touched: u64) -> String {
    if touched == 0 {
        return "按停时一件都没落，只重折了那份清单".to_string();
    }
    format!(
        "按停时落了 {} 件，清单记着到这儿为止目标上真实有什么",
        crate::report::thousands(touched)
    )
}

/// 删掉目标上的一份文件。
fn erase(sources: &Sources<'_>, step: &Step) -> io::Result<()> {
    let Some(at) = on_target(sources.target, sources.target_root, &step.path) else {
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
) -> Result<(String, Stamp, Placement), Refusal> {
    // **这一次判定的那份目录记性**：底下问两遍，走的是同一批目录（见 [`Cached`]）。
    let cached = Cached::over(sources.target);
    let (target, taken) = landing(sources, &cached, dirs, &step.path);
    // **最后一道防线**（模块文档七）：新增这一步的落点上不该有任何东西。
    //
    // 问两遍，因为一遍答不全：`landing` 那一问在**大小写不敏感**的目标上连别人那份
    // 也认得出（`real_path` 拿 `GB/tetris.zip` 就开得了 `GB/Tetris.zip`），可在
    // **大小写敏感**的盘上它一无所知——同一张卡插在 Windows 上挡得住、插在 Linux 上
    // 就在维护者那份旁边另写一份并报「成功」。于是折起来再问一次（`occupancy`）。
    //
    // **先问折起来那一问，哪怕逐字那一问已经说「占着了」**：`landing` 交回来的路径是
    // `根 + 键` 拼出来的（`real_path` 的快路径就是直接拼），大小写还是我们自己那份，
    // 于是在**卡上**——正是这条缺陷的主场景——报出来会是「`GB/tetris.zip` 的落点上
    // 已经有东西了（…/GB/tetris.zip）」，两条一模一样，等于没说出是谁占着。
    // `occupancy` 的答案来自 listing，那才是盘上真实那个名字。
    if step.act == Act::Add {
        // **答不出来也不放行**（ADR-0021）：折起来那一问不是两态，「列不开」既不是
        // 「有」也不是「没有」。逐字那一问答「占着了」时用它的话说——那条路径是
        // [`on_target`] 从盘上认回来的，比一句「答不出来」说得清楚。
        let blocking = match occupancy(&cached, sources.target_root, &step.path) {
            Occupancy::Occupied(at) => Some(Refusal::Occupied(at)),
            Occupancy::Unreadable(dir) if !taken => Some(Refusal::Unreadable(dir)),
            Occupancy::Clear | Occupancy::Unreadable(_) => {
                taken.then(|| Refusal::Occupied(target.clone()))
            }
        }
        // 没人占着，也得问一句这条落点的目录建不建得出来（[`dir_blocked`]）。
        .or_else(|| dir_blocked(&cached, sources.target_root, &target).map(Refusal::NotADir));
        if let Some(refusal) = blocking {
            return Err(refusal);
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
        return Err(Refusal::Failed(error));
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
fn on_target(fs: &dyn LibraryFs, root: &Path, key: &str) -> Option<PathBuf> {
    real_path(fs, root, key)
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
fn landing(
    sources: &Sources<'_>,
    fs: &dyn LibraryFs,
    dirs: &BTreeMap<String, PathBuf>,
    key: &str,
) -> (PathBuf, bool) {
    if let Some(at) = on_target(fs, sources.target_root, key) {
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

/// 一步为什么没做成。
///
/// **「被那道闸挡下来」与别处冒出来的错误在类型上分开。** 「连着失败就停下来」那个
/// 计数（[`GIVE_UP_AFTER`]）认的是这个枚举，不是 [`io::ErrorKind`]——`place` 从前只
/// 交回一个 [`io::Error`]，于是「被闸挡下来」与 `create_dir_all` 撞上同名文件报的那条
/// 在类型上分不开，两者都是 [`io::ErrorKind::AlreadyExists`]（挂单 `Q252`）。
enum Refusal {
    /// **落点被占**：折起来或逐字，那儿已经有东西了。这是它在盘上真实的那条路径。
    Occupied(PathBuf),
    /// **落点这一枝读不动**：有一层列不开，那儿有没有东西答不出来（ADR-0021）。
    Unreadable(PathBuf),
    /// **该建目录的位置上躺着个不是目录的东西**：这条键的目录根本建不出来。
    NotADir(PathBuf),
    /// 别的：真的写不进去、读不出来，或者被中断。
    Failed(io::Error),
}

impl From<io::Error> for Refusal {
    fn from(error: io::Error) -> Self {
        Self::Failed(error)
    }
}

impl Refusal {
    /// 这一条进不进「连着失败就停下来」那个计数（[`GIVE_UP_AFTER`]）。
    ///
    /// **被占与读不动不进**：那是这一个落点的确定性条件，接着往下试有意义。
    /// **目录段上躺着个文件要进**：那一格底下每一条新增都以同一句话失败，
    /// 接着试没有任何意义。
    fn counted(&self) -> bool {
        match self {
            Self::Occupied(_) | Self::Unreadable(_) => false,
            Self::NotADir(_) | Self::Failed(_) => true,
        }
    }

    /// 报告里那句话。**说得出是谁挡的**：只说一句「已经有东西了」而不指出是哪一条，
    /// 维护者没法判断该动哪一份。
    fn why(&self, key: &str) -> String {
        match self {
            Self::Occupied(at) => format!(
                "{key} 的落点上已经有东西了（{}）：清单之外的文件一律不碰（ADR-0015）",
                crate::path::display(at),
            ),
            Self::Unreadable(dir) => format!(
                "{key} 的落点在一个列不开的目录底下（{}）：那儿有没有东西答不出来，\
                 答不出来就不写（ADR-0021）",
                crate::path::display(dir),
            ),
            Self::NotADir(at) => format!(
                "{key} 要落进的目录位置上躺着一个不是目录的东西（{}）：这条路径底下\
                 一份都写不进去",
                crate::path::display(at),
            ),
            Self::Failed(error) => format!("{error}"),
        }
    }
}

/// 折起来那一问的答案。**「读不动」是第三态**（ADR-0021）——它从前被折成了「没有」。
enum Occupancy {
    /// 折起来有东西占着这条落点，这是它在盘上真实的那条路径。
    Occupied(PathBuf),
    /// 这一枝上有一层**列不开**：底下有没有挡路的**答不出来**。既不是「有」也不是
    /// 「没有」，而这道闸要的是「证得出落点上是空的」——证不出来就不写。
    Unreadable(PathBuf),
    /// 列开了，没有挡路的。
    Clear,
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
///
/// **列不开的那一层交出的是第三态，不是「这一层没有挡路的」**（ADR-0021）。Unix 上一个
/// `0300` 的目录 `read_dir` 失败、按名字 `open` 照样成功：折成「没有」的话，大小写敏感
/// 的盘上就会在维护者那份 `GB/Tetris.zip` 旁边多写一份 `GB/tetris.zip`。一枝列不开只在
/// **别的枝都没找出挡路的**时候才成为答案——找得出来的话，说得出是谁挡的那一条更有用。
///
/// **每一层都不看 `kind`、折起来一样的全都跟下去**：`LibraryFs::read_dir` 不跟随符号链接
/// （`fs::real`），于是目标上一个指向别处的平台目录交出来的 `kind` 是
/// [`Symlink`](crate::fs::EntryKind::Symlink)——按 `kind` 挑「是不是目录」会把这一整枝漏掉，
/// 而它底下照样躺得住维护者那份。挡不挡得住由**下一层列不列得开**说了算
/// （[`list`]），不由这一层的 `kind` 说了算。
///
/// **「该建目录的位置上躺着个非目录」不在这一问里**：那问的是我们自己那条落点建不建得出
/// 目录，与「折起来撞上了谁」是两件事（大小写敏感的盘上 `GB/sub` 是个文件，不代表
/// `gb/sub` 建不出来）。它归 [`dir_blocked`]。
fn occupancy(fs: &dyn LibraryFs, root: &Path, key: &str) -> Occupancy {
    let mut at = vec![root.to_path_buf()];
    let mut unreadable: Option<PathBuf> = None;
    for segment in key.split('/') {
        if segment.is_empty() {
            continue;
        }
        let mut next: Vec<PathBuf> = Vec::new();
        for dir in &at {
            match list(fs, dir) {
                Listing::Entries(entries) => next.extend(folded_children(entries, segment)),
                // 不在、或者根本不是目录：这一枝底下没有东西挡路。
                Listing::Missing | Listing::NotADir => {}
                // 记下来接着看别的枝：找得着挡路的那一条比一句「答不出来」说得清楚。
                Listing::Unreadable => {
                    unreadable.get_or_insert_with(|| dir.clone());
                }
            }
        }
        if next.is_empty() {
            return unreadable.map_or(Occupancy::Clear, Occupancy::Unreadable);
        }
        next.sort();
        next.dedup();
        at = next;
    }
    at.into_iter()
        .next()
        .map_or(Occupancy::Clear, Occupancy::Occupied)
}

/// 我们真要建的那条目录路径上，躺着一个**不是目录**的东西吗；有的话，是哪一段。
///
/// 卡上有个文件叫 `GB`、这一趟要往 `GB/` 底下写十几份：`create_dir_all` 到这一段必然
/// 失败，那一枝底下每一条新增都写不进去。从前两层判定都答不出这一格（逐字那一问只问落点
/// 自己在不在，折起来那一问走到「不是目录」就当这一枝空的），一路走到建目录才炸——而它
/// 报的是 [`io::ErrorKind::AlreadyExists`]，正好撞上「落点被占不计数」那条豁免。
///
/// **问的是[我们自己那条落点](landing)，不是折起来撞上的别的枝。** 大小写敏感的盘上
/// `GB/` 与 `gb/` 能同时在，`GB/sub` 是个文件**不代表** `gb/sub` 建不出来——照那边的答案
/// 挡下来是误报，而这一格[计数](Refusal::counted)，十条就把整趟停住。
///
/// 判据也不猜，逐段问文件系统：
///
/// - **列得开**：这一段是目录，接着往下问。
/// - **不是目录**：`create_dir_all` 到这一段必然失败——就是它。
/// - **不在**：从这一段起都是要新建的，建得出来（分大小写的盘上 `gb` 这个文件旁边照样
///   建得出 `GB/`，正是这一档）。
/// - **列不开**：那个目录**在**，而建目录不需要列目录，接着往下问。
fn dir_blocked(fs: &dyn LibraryFs, root: &Path, target: &Path) -> Option<PathBuf> {
    let mut at = root.to_path_buf();
    for segment in target.parent()?.strip_prefix(root).ok()?.components() {
        at.push(segment);
        match list(fs, &at) {
            Listing::Entries(_) | Listing::Unreadable => {}
            Listing::NotADir => return Some(at),
            Listing::Missing => return None,
        }
    }
    None
}

/// 这一层里折起来与 `segment` 一样的那几条。
///
/// **一律整层列出来再比**：要认的正是「盘上那个名字与我们要写的只差大小写」，而只有
/// listing 交得出盘上真实那个名字。名字读不出 UTF-8 的条目跳过——折不了的东西也就
/// 无从比对。
fn folded_children(entries: Vec<DirEntry>, segment: &str) -> Vec<PathBuf> {
    let wanted = crate::path::fold(segment);
    entries
        .into_iter()
        .map(|entry| entry.path)
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| crate::path::fold(name) == wanted)
        })
        .collect()
}

/// 一次落点判定之内的那份**目录记性**：同一个目录只列一遍。
///
/// 这道闸问目标两遍——逐字那一问的退路（[`real_path`] 逐段列目录）与折起来那一问
/// （[`occupancy`] 逐段折着往下走）——两遍走的是**同一批目录**。各列各的等于把每个目录
/// 整层列两遍，而一个平台目录下几千份文件，那就是几千次多余的整层 listing。
///
/// **作用域是这一次 [`place`] 的栈，不是这一趟。** 包一层带记性的实现塞进
/// [`Sources::target`] 才是顺手的写法，可那样作用域就成了整趟——而这道闸防的正是
/// 「计划算完到真的改名之间，卡还插在机器上」，一份跑到一半就过期的 listing 会把那道缝
/// 重新打开（挂单 `Q134`、模块文档八）。建在栈上就不会：判定问的每一句都在这一步写字节
/// **之前**，这一步一写完它就没了。
///
/// **只记 `read_dir`**，别的几句原样转给底下那一层——记住的东西越少，能骗人的地方越少。
/// [`settled`] 那一问故意**不吃**这份记性：它排在 `create_dir_all` 之后，要看的正是刚
/// 建出来的那个目录。
struct Cached<'a> {
    /// 底下那一层：真跑是真盘，测试里是塞进来的视图。
    fs: &'a dyn LibraryFs,
    /// 列过的那些目录。失败只留下[种类](io::ErrorKind)——[`io::Error`] 复制不了，
    /// 而问它的那几处（[`list`]、[`real_path`]）看的也只是种类。
    seen: Mutex<BTreeMap<PathBuf, Result<Vec<DirEntry>, io::ErrorKind>>>,
}

impl<'a> Cached<'a> {
    /// 给这一层套上记性。
    fn over(fs: &'a dyn LibraryFs) -> Self {
        Self {
            fs,
            seen: Mutex::new(BTreeMap::new()),
        }
    }
}

impl LibraryFs for Cached<'_> {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.fs.canonicalize(path)
    }

    fn read_dir(&self, dir: &Path) -> io::Result<Vec<DirEntry>> {
        let mut seen = self
            .seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !seen.contains_key(dir) {
            let listing = self.fs.read_dir(dir).map_err(|error| error.kind());
            seen.insert(dir.to_path_buf(), listing);
        }
        match seen.get(dir) {
            Some(Ok(entries)) => Ok(entries.clone()),
            Some(Err(kind)) => Err(io::Error::from(*kind)),
            // 上面刚放进去过，这一支到不了。
            None => self.fs.read_dir(dir),
        }
    }

    fn read_head(&self, file: &Path, limit: usize) -> io::Result<Vec<u8>> {
        self.fs.read_head(file, limit)
    }

    fn read_tail(&self, file: &Path, limit: usize) -> io::Result<Vec<u8>> {
        self.fs.read_tail(file, limit)
    }

    fn open(&self, file: &Path) -> io::Result<Box<dyn ReadSeek + '_>> {
        self.fs.open(file)
    }
}

/// 列一层，**把 `read_dir` 那一个失败拆成三种结论**。
///
/// **「列不开」与「列开了没找到」是两态**（ADR-0021），折成一个的话，Unix 上一个 `0300`
/// 的目录——`read_dir` 失败、按名字 `open` 照样成功——底下这道闸整个失效：大小写敏感的盘
/// 上会在维护者那份 `GB/Tetris.zip` 旁边多写一份 `GB/tetris.zip`。而模块文档七点名
/// [`TargetState::unlistable_dirs`](super::TargetState) 正是执行这一层非有不可的头号理由，
/// 折掉它等于把那个理由自己作废。
///
/// **「不是目录」也单拎出来**：那一格挡的不是落点而是**建目录**，两件事的后果不一样
/// ——一条挡下来接着往下试有意义，另一条底下每一条都写不进去（[`Refusal::counted`]）。
fn list(fs: &dyn LibraryFs, dir: &Path) -> Listing {
    match fs.read_dir(dir) {
        Ok(entries) => Listing::Entries(entries),
        Err(error) => match error.kind() {
            io::ErrorKind::NotFound => Listing::Missing,
            io::ErrorKind::NotADirectory => Listing::NotADir,
            _ => Listing::Unreadable,
        },
    }
}

/// 列一层的结果。
enum Listing {
    /// 列开了。
    Entries(Vec<DirEntry>),
    /// 这条路径不在。
    Missing,
    /// 在，可它**不是目录**。
    NotADir,
    /// 在，可**列不开**：底下有什么答不出来（ADR-0021 的第三态）。
    Unreadable,
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
    let Some(at) = real_dir(sources.target, sources.target_root, dir) else {
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
