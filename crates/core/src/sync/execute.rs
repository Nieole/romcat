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

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use ring::digest::{Context, SHA256};

use crate::capability::Conversion;
use crate::catalog::{Roots, mtime_ns};
use crate::convert::{self, ConvertError};
use crate::fs::{LibraryFs, real_path};
use crate::scan::CancelToken;
use crate::scrape::pool::hex;

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
/// # Errors
/// 目标根建不出来时返回错误。单个文件写不进去**不是错误**：那一步记进
/// [`Outcome::failures`]，整趟继续（连着失败太多次才停，见 [`GIVE_UP_AFTER`]）。
pub fn run(
    plan: &Plan,
    desired: &Desired,
    actual: &TargetState,
    previous: &Manifest,
    sources: &Sources<'_>,
    cancel: &CancelToken,
) -> io::Result<Outcome> {
    std::fs::create_dir_all(sources.target_root)?;

    let wants_media = plan
        .steps
        .iter()
        .any(|step| step.kind == FileKind::Media && step.act != Act::Delete);
    let placement = match (wants_media, sources.link_probe_dir) {
        (true, Some(from)) => Some(probe(from, sources.target_root)),
        (true, None) => Some(Placement::Copy),
        (false, _) => None,
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

    for step in &plan.steps {
        if cancel.is_cancelled() {
            out.interrupted = true;
            break;
        }
        let outcome = match step.act {
            Act::Delete => erase(sources, step).map(|()| None),
            Act::Add | Act::Update => place(sources, step, placement, cancel, &mut out)
                .map(|(stamp, how)| Some((stamp, how))),
        };
        match outcome {
            Ok(None) => {
                consecutive = 0;
                removed.insert(step.path.clone());
                out.deleted.files += 1;
                out.deleted.bytes += step.was;
            }
            Ok(Some((stamp, how))) => {
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
                done.insert(step.path.clone(), recorded(step, stamp));
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                // 写到一半收到中断：临时文件已经清掉了，落点上还是原来那一份。
                out.interrupted = true;
                break;
            }
            Err(error) => {
                consecutive += 1;
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

/// 把一份文件放到目标上。返回**从目标上读回来的**戳，以及实际用的办法。
fn place(
    sources: &Sources<'_>,
    step: &Step,
    placement: Option<Placement>,
    cancel: &CancelToken,
    out: &mut Outcome,
) -> io::Result<(Stamp, Placement)> {
    let target = landing(sources, &step.path);
    let parent = target.parent().unwrap_or(sources.target_root).to_path_buf();
    std::fs::create_dir_all(&parent)?;
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
            let from = roots.real_path(sources.library, &step.source).ok_or_else(|| {
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

/// 这一份该**落在**目标上的哪条路径。
///
/// ADR-0020 在写这一侧同样成立，而且分两半：
///
/// - **已经在了就用它自己在盘上的名字**。目标把名字存成分解形式（HFS+ 就会）而查找
///   又分解敏感时，拿 NFC 的键去 `rename`，会在维护者那份**旁边新造一份**，旧那份留在
///   卡上从此变成「清单之外」——票 16 刚被这个 bug 咬过（挂账 D82）。
/// - **还不在就用我们自己选的那个名字**（NFC 的键），但**目录要用盘上真实那个**：
///   上级目录若已存在且是分解形式，照键拼会在它旁边再建一个同名目录。
fn landing(sources: &Sources<'_>, key: &str) -> PathBuf {
    if let Some(at) = on_target(sources, key) {
        return at;
    }
    let (dir, name) = match key.rsplit_once('/') {
        Some((dir, name)) => (Some(dir), name),
        None => (None, key),
    };
    let parent = dir.map_or_else(
        || sources.target_root.to_path_buf(),
        |dir| {
            on_target(sources, dir)
                .unwrap_or_else(|| sources.target_root.join(dir.replace('/', SEPARATOR)))
        },
    );
    parent.join(name)
}

/// 一条写成了的步骤在清单里长什么样。
fn recorded(step: &Step, stamp: Stamp) -> ManifestFile {
    ManifestFile {
        path: step.path.clone(),
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
