//! 读出**透明容器**内部装着什么：每个内部文件的名字、未压缩大小与可得的校验和。
//!
//! zip 与 7z 走的是**穿透**——零解压，容器的元数据里就写着 CRC-32。zst 走不了那条路，
//! 只能真的解一遍（见下）。两条路的产出是同一个形状，代价差 500–900 倍。
//!
//! **这是整个项目性能可行性的支点。** 主库 8.60 TiB 里有 91.1% 的容量装在透明容器里，
//! 其中 7z 独占 4.71 TiB（`docs/library-facts.md`）。DAT 的每条记录都带
//! `size` + `crc`，因此第一命中层用 **CRC-32 + 未压缩大小**（ADR-0002 的再修订）就能
//! 完全不解压地认出绝大部分内容。这一条不成立的话，7.84 TiB 得先解压一遍。
//!
//! **rar 还不在这里**：它是票 04，要自己写头部解析器以避开 UnRAR 的许可传染。
//!
//! **zst 在这里，但走的是另一条路。** `.zst` / `.tar.zst` 是**第一个进不了零解压快路的
//! 容器**——zstd 帧格式里根本没有 CRC-32 这个字段，tar 的 `chksum` 只保护头部元数据，
//! 两层都没有「成员 → 内容哈希」的映射表（ADR-0014 的第二段修订）。于是它的内部条目
//! 一律没有校验和，列全清单要真的把整条流解一遍。**接口不因此变形**：调用方拿到的
//! 仍是「名字、大小、可得的校验和」，只是 zst 的校验和那一栏空着。详见 [`zst`]。
//!
//! ## 三个必须显式处理的坑
//!
//! 1. **ZIP 通用标志位第 3 位**。置位时 local header 里的 CRC 与两个大小**全是零**，
//!    真值在数据之后的 data descriptor 里。因此零解压层只读**中央目录**，
//!    [`zip`] 连 local header 的这三个字段都不去看一眼。
//! 2. **ZIP64 哨兵值**。大小或偏移放不下 32 位时原字段被置成 `0xFFFF` / `0xFFFFFFFF`，
//!    真值在 extra field 的 `0x0001` 记录里，且**只有哨兵字段才出现**、顺序固定。
//! 3. **7z 的 solid block**。一个 block（规范里叫 folder）里的多个文件是连着压的，
//!    读其中一个就得从块头解起。朴素实现「一个文件解一趟」在 200 个文件的块上是
//!    **约 100 倍**放大。好在 `文件 → 块`、`块内偏移`、`每块几个子流` 全部零解压可得，
//!    于是可以**按块分组、一块一趟**，把它压回 1 倍——这就是 [`read_entries`] 的形状，
//!    它报出的 [`ReadStats::blocks_decoded`] 让「一块只解了一次」变成可断言的事实。
//!
//! 出处见 `docs/research/containers-and-compressed-images.md` 第 1.1、1.2、1.6 节。

pub mod sevenz;
pub mod zip;
pub mod zst;

use std::io::{self, Read};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::fs::LibraryFs;
use crate::path::extension_lower;

/// 认得的**透明容器**格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ContainerKind {
    /// zip（含 cbz）。元数据集中在文件末尾的中央目录里，明文、每个条目独立压缩。
    Zip,
    /// 7z。元数据集中在文件末尾，**可能被压缩过**（`kEncodedHeader`），且压缩单元是块。
    SevenZip,
    /// zst（含 tar.zst）。**没有元数据可言**：zstd 是单流压缩器，不是归档器。
    /// 内部构成只能靠解压读出来，而且一个校验和都拿不到（[`zst`]）。
    Zstd,
}

impl ContainerKind {
    /// 报告里用的名字。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::SevenZip => "7z",
            Self::Zstd => "zst",
        }
    }

    /// 存进**中立库**用的短码。
    ///
    /// 与 [`label`](Self::label) 分开写：那个是给人看的，改它不该让已经存进库里的
    /// 记录读不回来。
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::SevenZip => "7z",
            Self::Zstd => "zst",
        }
    }

    /// 从短码读回来。
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "zip" => Some(Self::Zip),
            "7z" => Some(Self::SevenZip),
            "zst" => Some(Self::Zstd),
            _ => None,
        }
    }

    /// 按扩展名判断这个文件该按哪种容器读；本程序还读不了的格式是 `None`。
    ///
    /// `.rar` 在 [`classify`](crate::classify) 里同样是透明容器，但它是票 04 的活，
    /// 这里**故意**认不出来。
    #[must_use]
    pub fn for_path(path: &Path) -> Option<Self> {
        match extension_lower(path)?.as_str() {
            "zip" | "cbz" => Some(Self::Zip),
            "7z" => Some(Self::SevenZip),
            "zst" => Some(Self::Zstd),
            _ => None,
        }
    }

    /// 这种格式**穿得透吗**——也就是不解压就读得出内部构成吗（`CONTEXT.md`）。
    ///
    /// zip 与 7z 穿得透：中央目录与头部把「名字、大小、CRC-32」显式存了下来。zst 穿不了，
    /// 而且不是「差一点」——格式里就没有这些字段（ADR-0014 的第二段修订），
    /// 它的内部构成只能靠完整解压读出来。调度器与报告都要说得出这个区别：
    /// 一个容器要花 30 秒还是 30 毫秒，全看这一条。
    #[must_use]
    pub fn is_penetrable(self) -> bool {
        match self {
            Self::Zip | Self::SevenZip => true,
            Self::Zstd => false,
        }
    }
}

/// 容器里的一个内部文件。
///
/// 三个字段就是零解压层的全部产出：名字、未压缩大小、CRC-32。**格式里没有 SHA-1 与
/// MD5**（ADR-0014），要它们只能解压，那是识别未命中之后的事。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InnerEntry {
    /// 内部路径，分隔符统一成 `/`。
    pub path: String,
    /// 未压缩大小。
    pub size: u64,
    /// CRC-32（IEEE，与 DAT 里的 `crc` 同一个算法）。
    ///
    /// `None` 表示**容器自己没记**——7z 的 `kCRC` 是可选块，逐流还有 `Defined` 位。
    /// 没有 CRC 的条目进不了第一命中层，只能解压。
    pub crc32: Option<u32>,
    /// 这个条目是不是目录。目录条目没有内容，不参与识别。
    pub is_dir: bool,
    /// 这个条目的数据在哪个块里。
    ///
    /// zip 每个条目自成一块（可以任意顺序、任意并行读）；7z 是 folder，
    /// 一块可以装多个文件——那就是 solid。`None` 表示这个条目根本没有数据流
    /// （目录，或者零字节的空文件）。
    pub block: Option<usize>,
    /// 名字不是合法 UTF-8，这里存的是有损转换的结果。
    ///
    /// zip 只有在通用标志位第 11 位置位时才保证 UTF-8，其余是「本地代码页」——
    /// 库里的中文名多半是 GBK。名字不是识别的判据（判据是 CRC-32 + 大小），
    /// 但报告要说得出「有多少条名字是猜的」。
    pub name_lossy: bool,
}

impl InnerEntry {
    /// 这个条目有没有可读的内容。
    #[must_use]
    pub fn has_content(&self) -> bool {
        !self.is_dir && self.size > 0
    }
}

/// 一个容器内部构成的**轻量**形态：只有条目与块数。
///
/// 它与 [`Listing`] 分开，是因为落进**中立库**、在扫描的写批次里排队的是它：
/// `Listing` 还揣着回头读内容用的定位信息，7z 那份已经解好的头部几十 KB 起，
/// 几千条排在一起就是几百 MB。
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contents {
    /// 内部条目，顺序与容器里的顺序一致。
    pub entries: Vec<InnerEntry>,
    /// 一共有几个块。
    pub blocks: usize,
}

impl Contents {
    /// 内部文件数（目录条目不算）。
    #[must_use]
    pub fn file_count(&self) -> u64 {
        u64::try_from(self.entries.iter().filter(|e| !e.is_dir).count()).unwrap_or(u64::MAX)
    }

    /// 内部文件的未压缩字节合计。
    #[must_use]
    pub fn total_size(&self) -> u64 {
        self.entries
            .iter()
            .filter(|e| !e.is_dir)
            .fold(0u64, |sum, e| sum.saturating_add(e.size))
    }

    /// 有几个内部文件是容器没记 CRC-32 的。它们进不了零解压的第一命中层。
    #[must_use]
    pub fn without_crc(&self) -> u64 {
        u64::try_from(
            self.entries
                .iter()
                .filter(|e| !e.is_dir && e.crc32.is_none())
                .count(),
        )
        .unwrap_or(u64::MAX)
    }

    /// 某个块里装了几个有内容的条目。
    #[must_use]
    pub fn entries_in_block(&self, block: usize) -> usize {
        self.entries
            .iter()
            .filter(|e| e.block == Some(block))
            .count()
    }

    /// 这个容器是不是 solid：**有任何一个块装了多于一个条目**。
    ///
    /// 判据零解压可得（7z 看 `NumUnPackStreamsInFolders[i] > 1`），因此调度器在
    /// 决定怎么读之前就知道自己面对什么。zip 永远不是 solid。
    #[must_use]
    pub fn is_solid(&self) -> bool {
        (0..self.blocks).any(|block| self.entries_in_block(block) > 1)
    }
}

/// 一个容器的零解压清单。
///
/// 它比 [`Contents`] 多揣着**回头去读内容**所需的定位信息（zip 的条目偏移、
/// 7z 已经解好的头部），因此 [`read_entries`] 不必把头部再解析一遍。
#[derive(Debug)]
pub struct Listing {
    /// 容器格式。
    pub kind: ContainerKind,
    /// 内部构成。
    pub contents: Contents,
    locator: Locator,
}

#[derive(Debug)]
enum Locator {
    Zip(Vec<zip::EntryLocator>),
    SevenZip(Box<sevenz_rust2::Archive>),
    Zstd(zst::Shape),
}

/// 一个容器穿不透的原因，按类分好。
///
/// 分类不是为了好看：**「不是这个格式」与「要密码」的后续处置完全不同**——前者是
/// 该去看看这文件到底是什么，后者是等人给密码。存一句自由文本的话，报告只能把原因
/// 一条条打印出来，永远数不出「有多少个是要密码的」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FailureReason {
    /// 文件读不动。
    Unreadable,
    /// 扩展名说是这个格式，字节不是。
    WrongFormat,
    /// 是这个格式，但结构读不下去。
    Malformed,
    /// 要密码。头部加密时连内部文件名都看不到。
    NeedsPassword,
    /// 压缩方法本程序解不了。
    UnsupportedMethod,
    /// 要外部字典。zstd 的帧头可以写一个 `Dictionary_ID`，那本字典不在库里就永远解不开。
    NeedsDictionary,
}

impl FailureReason {
    /// 报告里用的名字。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Unreadable => "读不动",
            Self::WrongFormat => "不是这个格式",
            Self::Malformed => "结构读不下去",
            Self::NeedsPassword => "需要密码",
            Self::UnsupportedMethod => "压缩方法解不了",
            Self::NeedsDictionary => "需要外部字典",
        }
    }

    /// 存进**中立库**用的短码。与 [`label`](Self::label) 分开：那个是给人看的。
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Unreadable => "unreadable",
            Self::WrongFormat => "wrong-format",
            Self::Malformed => "malformed",
            Self::NeedsPassword => "needs-password",
            Self::UnsupportedMethod => "unsupported-method",
            Self::NeedsDictionary => "needs-dictionary",
        }
    }

    /// 从短码读回来。
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        Self::all().into_iter().find(|it| it.code() == code)
    }

    /// 报告里固定的排列顺序。
    #[must_use]
    pub fn all() -> [Self; 6] {
        [
            Self::Unreadable,
            Self::WrongFormat,
            Self::Malformed,
            Self::NeedsPassword,
            Self::UnsupportedMethod,
            Self::NeedsDictionary,
        ]
    }
}

/// 一次穿不透：归到哪一类，以及具体是什么。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    /// 归到哪一类。
    pub reason: FailureReason,
    /// 具体是什么，给人看的。
    pub detail: String,
}

/// 穿透一个透明容器的结论：读出来了，或者没读出来以及为什么。
///
/// **穿不透不是致命错误**，是一种如实记录的状态——与 ADR-0021 对不可读文件的态度
/// 是同一条。报告要说得出有多少容器穿不透、各自卡在哪。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Penetration {
    /// 按扩展名判定的容器格式。
    pub kind: ContainerKind,
    /// 内部构成；穿不透时是空的。
    pub contents: Contents,
    /// 穿不透的原因；穿透了就是 `None`。
    pub failure: Option<Failure>,
}

impl Penetration {
    /// 从一份清单折出要落库的那部分。
    #[must_use]
    pub fn listed(listing: &Listing) -> Self {
        Self {
            kind: listing.kind,
            contents: listing.contents.clone(),
            failure: None,
        }
    }

    /// 记下一次穿不透。
    #[must_use]
    pub fn failed(kind: ContainerKind, error: &ContainerError) -> Self {
        Self {
            kind,
            contents: Contents::default(),
            failure: Some(Failure {
                reason: error.reason(),
                detail: error.to_string(),
            }),
        }
    }

    /// 穿透成功了没有。
    #[must_use]
    pub fn is_listed(&self) -> bool {
        self.failure.is_none()
    }
}

/// 穿透容器时出的岔子。
///
/// 它们都**不是**扫描的致命错误：一个穿不透的容器照样进报告，报告说得出它为什么
/// 穿不透。这与 ADR-0021 对不可读文件的态度是同一条——如实记录，不猜。
#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    /// 文件读不动。
    #[error("读不动：{0}")]
    Io(#[from] io::Error),
    /// 扩展名说是这个格式，字节不是。
    #[error("不是 {kind} 容器：{detail}")]
    NotAContainer {
        /// 按扩展名以为的格式。
        kind: &'static str,
        /// 具体哪儿对不上。
        detail: String,
    },
    /// 结构坏了或者本程序读不懂。
    #[error("结构读不下去：{0}")]
    Malformed(String),
    /// 要密码。头部加密时连文件名都读不到。
    #[error("需要密码")]
    Encrypted,
    /// 压缩方法本程序解不了（零解压层照样给得出 CRC 与大小，只有取内容时才卡住）。
    #[error("解不了这个压缩方法：{0}")]
    UnsupportedMethod(String),
    /// 帧头写着要一本外部字典。
    ///
    /// 字典内容参与 LZ 匹配的「历史」，没有它 sequence 指令引用的偏移无从解析——
    /// **这个文件无法独立解压**。真库里实测 0 个，但零成本就查得出来，查出来就如实说。
    #[error("需要外部字典 {0}，无法独立解压")]
    NeedsDictionary(u32),
    /// 扩展名不是本程序读得了的透明容器。
    #[error("不是 zip、7z 或 zst")]
    NotSupportedHere,
}

impl ContainerError {
    /// 这次穿不透该归到哪一类。
    #[must_use]
    pub fn reason(&self) -> FailureReason {
        match self {
            Self::Io(_) => FailureReason::Unreadable,
            Self::NotAContainer { .. } | Self::NotSupportedHere => FailureReason::WrongFormat,
            Self::Malformed(_) => FailureReason::Malformed,
            Self::Encrypted => FailureReason::NeedsPassword,
            Self::UnsupportedMethod(_) => FailureReason::UnsupportedMethod,
            Self::NeedsDictionary(_) => FailureReason::NeedsDictionary,
        }
    }
}

/// 零解压读出一个容器的内部构成。
///
/// # Errors
/// 文件读不动、不是那个格式、结构坏了、或者需要密码时返回错误。
pub fn list(library: &dyn LibraryFs, path: &Path) -> Result<Listing, ContainerError> {
    let kind = ContainerKind::for_path(path).ok_or(ContainerError::NotSupportedHere)?;
    list_as(library, path, kind)
}

/// 按指定格式零解压读出内部构成。扩展名不可信时用它。
///
/// # Errors
/// 同 [`list`]。
pub fn list_as(
    library: &dyn LibraryFs,
    path: &Path,
    kind: ContainerKind,
) -> Result<Listing, ContainerError> {
    match kind {
        ContainerKind::Zip => zip::list(library, path),
        ContainerKind::SevenZip => sevenz::list(library, path),
        ContainerKind::Zstd => zst::list(library, path),
    }
}

/// 调用方对一个内部文件想要多少字节。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Demand {
    /// 不要。
    Skip,
    /// 只要前 N 字节，拿够就停。
    ///
    /// 这就是**部分解压**：卡带的内部头多在前 `0x200` 字节内，读够就中止，
    /// 代价与文件总大小无关（`docs/research/containers-and-compressed-images.md` 1.5）。
    Prefix(u64),
    /// 整个文件。
    All,
}

impl Demand {
    fn wanted(self) -> bool {
        !matches!(self, Self::Skip)
    }

    fn limit(self) -> u64 {
        match self {
            Self::Skip => 0,
            Self::Prefix(n) => n,
            Self::All => u64::MAX,
        }
    }
}

/// 一趟读取的计划：每个内部条目要多少字节。
///
/// 计划**先整份定下来再动手**，不是边解边问。7z 必须如此：要知道哪些块根本不用碰，
/// 就得先看完所有条目的需求。这也让「这一趟解了几个块」在开工前就是确定的。
#[derive(Debug, Clone)]
pub struct ReadPlan {
    demands: Vec<Demand>,
}

impl ReadPlan {
    /// 逐条问一遍要多少。
    pub fn new(listing: &Listing, mut pick: impl FnMut(&InnerEntry) -> Demand) -> Self {
        Self {
            demands: listing.contents.entries.iter().map(&mut pick).collect(),
        }
    }

    /// 每个有内容的条目都整份要。
    #[must_use]
    pub fn all(listing: &Listing) -> Self {
        Self::new(listing, |entry| {
            if entry.has_content() {
                Demand::All
            } else {
                Demand::Skip
            }
        })
    }

    /// 每个有内容的条目只要前 `bytes` 字节。
    #[must_use]
    pub fn prefix(listing: &Listing, bytes: u64) -> Self {
        Self::new(listing, |entry| {
            if entry.has_content() {
                Demand::Prefix(bytes)
            } else {
                Demand::Skip
            }
        })
    }

    /// 只要指定的一个条目，整份。
    #[must_use]
    pub fn only(listing: &Listing, index: usize) -> Self {
        Self::new(listing, |_| Demand::Skip).with(index, Demand::All)
    }

    /// 改掉某一条的需求。
    #[must_use]
    pub fn with(mut self, index: usize, demand: Demand) -> Self {
        if let Some(slot) = self.demands.get_mut(index) {
            *slot = demand;
        }
        self
    }

    fn demand(&self, index: usize) -> Demand {
        self.demands.get(index).copied().unwrap_or(Demand::Skip)
    }

    /// 这一趟要碰哪几个块，去重且有序。调度器与代价估算走的是同一份判据。
    pub(crate) fn blocks(&self, contents: &Contents) -> Vec<usize> {
        let mut blocks: Vec<usize> = contents
            .entries
            .iter()
            .enumerate()
            .filter(|(index, entry)| entry.has_content() && self.demand(*index).wanted())
            .filter_map(|(_, entry)| entry.block)
            .collect();
        blocks.sort_unstable();
        blocks.dedup();
        blocks
    }

    /// 这一趟要碰几个块。**这就是解压的代价上界**，开工前就算得出来。
    #[must_use]
    pub fn blocks_needed(&self, listing: &Listing) -> usize {
        self.blocks(&listing.contents).len()
    }
}

/// 一趟按块调度的读取的实际代价。
///
/// [`blocks_decoded`](Self::blocks_decoded) 是这张票最要紧的那个数：solid 的 7z 上，
/// 「一块只解一次」与「一个文件解一趟」相差约 100 倍，而这个字段让两者的区别
/// 变成可断言的事实，而不是一句口头承诺。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReadStats {
    /// 解开了几个块。
    pub blocks_decoded: u64,
    /// 交给调用方几个内部条目。
    pub entries_read: u64,
    /// 一共解压出多少字节，**含为了让后面的条目对齐而丢掉的那些**。
    pub bytes_decompressed: u64,
}

/// 按块调度地读一遍容器内部：**一个块最多解一次**。
///
/// `each` 拿到的是一个已经按 [`Demand`] 截好的流。**它是流**，不是缓冲：调用方一边读
/// 一边喂给几个 hasher 即可，整块内容不必驻留内存。读够想要的就 `return`，剩下的字节
/// 由这里负责处理——后面还有想要的条目就顺着排空（格式使然，解压器跳不过去），
/// 没有了就整块中止。
///
/// # Errors
/// 文件读不动、结构坏了、压缩方法解不了，或者 `each` 自己出错时返回错误。
pub fn read_entries(
    library: &dyn LibraryFs,
    path: &Path,
    listing: &Listing,
    plan: &ReadPlan,
    each: &mut dyn FnMut(&InnerEntry, &mut dyn Read) -> io::Result<()>,
) -> Result<ReadStats, ContainerError> {
    match &listing.locator {
        Locator::Zip(locators) => {
            zip::read_entries(library, path, &listing.contents, locators, plan, each)
        }
        Locator::SevenZip(archive) => {
            sevenz::read_entries(library, path, &listing.contents, archive, plan, each)
        }
        Locator::Zstd(shape) => {
            zst::read_entries(library, path, &listing.contents, *shape, plan, each)
        }
    }
}

/// 把一个内部条目按 [`Demand`] 截好交给调用方，并把它实际拉走的字节记进账。
///
/// 三种格式共用这一份：**它是「读够就停」这条纪律唯一的落点**。截断这一步一旦在某个
/// 格式里漏掉，那个格式就会把整个条目解出来——而这件事在报告里看不出来，只会表现为
/// 「这台机器怎么这么慢」。放在一处，就没有第二个地方能漏。
///
/// 返回值是拉走了多少字节，由调用方并进 [`ReadStats::bytes_decompressed`]——
/// 各格式还要往里加自己「为了对齐而丢掉」的那些，那部分只有它们自己知道。
fn hand_entry(
    entry: &InnerEntry,
    reader: &mut dyn Read,
    demand: Demand,
    each: &mut dyn FnMut(&InnerEntry, &mut dyn Read) -> io::Result<()>,
) -> io::Result<u64> {
    let mut bytes = 0u64;
    {
        // 借用限制在这个块里：出了块 `bytes` 才好读回来。
        let mut counted = Counting {
            inner: reader,
            counter: &mut bytes,
        };
        // 只要前若干字节时截一截就够——三种格式的解码器都是流，读够就停，
        // 代价与条目总大小无关（调研 1.5.1）。
        let mut bounded = (&mut counted).take(demand.limit());
        each(entry, &mut bounded)?;
    }
    Ok(bytes)
}

/// 数着字节走的读取器。`bytes_decompressed` 就是它数出来的。
struct Counting<'a, R> {
    inner: R,
    counter: &'a mut u64,
}

impl<R: Read> Read for Counting<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buf)?;
        *self.counter = self.counter.saturating_add(read as u64);
        Ok(read)
    }
}

/// 把剩下的字节顺着排空，返回排掉了多少。
///
/// solid 块里的条目是连着压的，解压器**跳不过**中间的字节——想读后面那个，
/// 前面这个就得走完。这不是浪费，是格式使然（1.5.2：没有任何编解码器支持跳到中间）。
fn drain(reader: &mut dyn Read) -> io::Result<u64> {
    io::copy(reader, &mut io::sink())
}
