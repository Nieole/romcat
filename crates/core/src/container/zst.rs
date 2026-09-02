//! zst 与 tar.zst：**第一个进不了「零解压读 CRC-32」快路的透明容器**。
//!
//! 主库里 `.zst` 是容量第二大的扩展名（2,685 个文件、2.50 TiB，仅次于 7z 的 4.71 TiB，
//! `docs/library-facts.md`），实际形态几乎全是 `.tar.zst`——tar 套 zst 的双层结构
//! （实测 2,681 / 2,685）。ADR-0014 的第二段修订定下了对它的态度：**工具支持，
//! 主库一个字节不动**。
//!
//! ## 为什么它进不了快路：三层都是硬的
//!
//! 1. **zstd 帧格式规范全文「CRC」出现 0 次**（v0.4.5 1777 行，RFC 8878 同样）。
//!    唯一的完整性字段是可选的 `Content_Checksum` = **XXH64 的低 32 位**，
//!    且覆盖**整个流**而不是某个成员，而 DAT 用的是 CRC-32。
//! 2. **tar 的 `chksum` 是 512 字节头部的无符号字节简单求和**（POSIX.1-2024），
//!    只保护元数据，与文件内容无关。
//! 3. **两层都没有「成员 → 内容哈希」的映射表。** zip 的中央目录与 7z 的头部把这张表
//!    显式存了下来，而 zstd 是单流压缩器、tar 是顺序磁带格式，设计里就没有这个概念。
//!    规范自陈 *does not attempt to allow random access to compressed data*。
//!
//! 于是 [`InnerEntry::crc32`](super::InnerEntry::crc32) 在这里恒为 `None`——
//! **接口不变形**，调用方拿到的仍是「名字、大小、可得的校验和」，只是校验和这一栏空着。
//!
//! ## 两条路径，代价差 500–900 倍
//!
//! | 路径 | 拿到什么 | 单文件 I/O | 2,685 个文件 |
//! |---|---|---|---|
//! | [`peek`]：帧头 + 第一个块 | 帧头全部字段；tar 的**第一条**的名字与大小 | ≤ [`PEEK_LIMIT`] ≈ 131 KB | 约 30 秒 |
//! | [`list`]：完整解压 | **全部**条目的名字与大小 | 整个文件 | 3.8–7.6 小时 |
//!
//! 上界是规范算出来的，不是估的：`Block_Maximum_Size = min(Window_Size, 128 KiB)`，
//! 而「解压的最小粒度就是一个完整 block」有实测（少读 1 字节吐 0 字节，够了一次吐满
//! 128 KiB，调研 2.3）。**所以「读第一条」与 zip 读中央目录是同一量级的廉价操作。**
//!
//! 而列全清单没有捷径：tar 是顺序格式，要找到第 N+1 条就得跳过第 N 条的全部内容，
//! 而对 zstd 流「跳过」只能靠真正解压再丢掉（调研 2.4 附 `tar` crate 的 `skip()` 源码）。
//! **因此这条路径的结论必须落库**——它按 `(路径, 大小, 修改时间)` 与增量扫描共用同一个
//! 判据，三元组没变就不再解第二遍（`catalog` 的 `container` / `container_entry` 两张表）。
//!
//! ## 三个必须显式处理的坑
//!
//! 1. **`Frame_Content_Size` 只有约 53% 的文件带它**（真盘 43 个样本、32 个可解析）。
//!    它是**可选**字段：输入来自管道时 zstd 事先不知道大小，`tar --zstd` 产生的帧头
//!    实测只有 6 字节、大小与校验和双双缺席。**不能假设它存在。**
//! 2. **pax 陷阱**。bsdtar / libarchive 默认写 pax：第一个 512 字节块是
//!    `PaxHeader/…` 伪条目（`typeflag='x'`），真条目在偏移 1024。
//!    「读前 512 字节取文件名」是错的——所以 tar 那一层交给 `tar` crate 走条目链。
//! 3. **外部字典**。带 `Dictionary_ID` 的帧没有那本字典就永远解不开（字典内容参与
//!    LZ 匹配的「历史」）。真库里实测 0 个，但零成本就能查，查出来如实说
//!    ——[`FailureReason::NeedsDictionary`](super::FailureReason::NeedsDictionary)。
//!
//! 出处见 `docs/research/zstandard-containers.md` 第 1、2 部分。

use std::io::{self, Cursor, Read};
use std::path::Path;

use super::{
    ContainerError, ContainerKind, Contents, InnerEntry, Listing, Locator, ReadPlan, ReadStats,
    drain, hand_entry,
};
use crate::fs::LibraryFs;
use crate::path::nfc;

/// Zstandard 帧的魔数，小端 `0xFD2FB528`（磁盘上 `28 B5 2F FD`）。
const MAGIC: u32 = 0xFD2F_B528;
/// v0.8 之前的旧帧格式，魔数落在这个闭区间里。本程序不解它们，但认得出来——
/// 「旧版帧格式」与「根本不是 zstd」是两句不同的话。
const LEGACY_MAGIC: std::ops::RangeInclusive<u32> = 0xFD2F_B522..=0xFD2F_B527;

/// `Block_Maximum_Size` 的上限：规范定为 `Window_Size` 与 128 KiB 中较小的那个。
const BLOCK_MAX_CAP: u64 = 128 << 10;
/// `Frame_Header` 连魔数在内最长 14 字节（魔数 4 + 描述符 1 + 窗口 1 + 字典 ID 4 + 大小 8）。
const MAX_FRAME_HEADER: usize = 14;
/// `Block_Header` 定长 3 字节。
const BLOCK_HEADER: usize = 3;

/// 「读第一条」这条路径最多读多少字节：帧头 + 块头 + 一个块。
///
/// **这个上界是规范算出来的**：`Block_Maximum_Size` 同时限制块的压缩后大小与解压后
/// 大小，因此第一个块无论如何装不下超过 128 KiB 的压缩字节。窗口更小的帧上界更低
/// （[`FrameHeader::peek_limit`]），这里取的是全局最坏情况。
pub const PEEK_LIMIT: usize = MAX_FRAME_HEADER + BLOCK_HEADER + BLOCK_MAX_CAP as usize;

/// 「读第一条」时最多解出多少明文。
///
/// 一个 tar 条目的头部只有 512 字节，pax 伪条目加上它自己的数据体通常也就一两 KB。
/// 留 64 KiB 是给 GNU longname 那类把长名字放进独立数据块的写法留的余量。
const PEEK_PLAINTEXT: usize = 64 << 10;

/// tar 的块大小（POSIX.1-2024）。
const TAR_BLOCK: usize = 512;
/// `chksum` 字段在头部里的偏移与长度。
const TAR_CHKSUM_AT: usize = 148;
const TAR_CHKSUM_LEN: usize = 8;
/// `magic` 字段：ustar / pax / GNU 都在这里写 `ustar`。
const TAR_MAGIC_AT: usize = 257;
/// GNU longname 伪条目里那个名字最长读多少。真实的路径远远够用；再长就当它坏了。
const MAX_LONG_NAME: u64 = 64 << 10;

/// zstd 帧头里零解压读得出的全部事实。
///
/// 只读文件前 14 字节就能确定性地拿到这些——规范原文：*Decoding this byte is enough
/// to tell the size of `Frame_Header`*（指描述符那一字节）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    /// 未压缩总大小。**`None` 是常态而不是异常**：真盘实测只有约 53% 的文件带它。
    pub content_size: Option<u64>,
    /// 带不带 `Content_Checksum`（XXH64 低 32 位）。
    ///
    /// **它对 DAT 匹配毫无用处**：算法不是 CRC-32，作用域是整个流而不是某个成员。
    /// 记它只是为了报告说得出这批文件是什么工具打的（无大小 + 无校验和 ≈ libarchive
    /// 的 `tar --zstd`）。
    pub has_checksum: bool,
    /// 外部字典的 ID；`None` 表示帧头没写。
    ///
    /// 写了非 0 的值就意味着**没有那本字典就解不开**。注意反过来不成立：规范原文说
    /// ID 为 0 时 *the frame may or may not need a dictionary*，解码器得靠别的途径知道
    /// ——那种只能试着解，解不开时报错。
    pub dictionary_id: Option<u32>,
    /// 解压窗口大小；`Single_Segment_flag` 置位时等于 `Frame_Content_Size`。
    pub window_size: u64,
    /// 帧头连魔数在内一共几字节。
    pub header_len: usize,
}

impl FrameHeader {
    /// 这个帧解压的最小粒度：`min(Window_Size, 128 KiB)`。
    #[must_use]
    pub fn block_maximum_size(&self) -> u64 {
        self.window_size.min(BLOCK_MAX_CAP)
    }

    /// 读到第一个块为止最多要读多少字节。
    #[must_use]
    pub fn peek_limit(&self) -> u64 {
        self.header_len as u64 + BLOCK_HEADER as u64 + self.block_maximum_size()
    }
}

/// 一个 `.zst` 里装的是什么。
///
/// **判据是字节不是扩展名**：`.tar.zst` 这个后缀只是习惯，而第一个块解出来的前 512
/// 字节到底是不是一个 tar 头，是看得出来的（`ustar` 魔数或者头部校验和自洽）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inner {
    /// 里面是一个 tar，附**第一个真条目**——pax 与 GNU longname 那些伪条目已经跳过。
    Tar(TarHead),
    /// 里面是一个 tar，但第一条读不出来（前缀不够、或者条目链坏了）。
    TarUnreadable(String),
    /// 不是 tar：整个流就是一个文件。
    ///
    /// 内部名按外层文件名去掉 `.zst` 推导——RetroArch 的 zstd 后端也是这么做的
    /// （源码注释原文 *Derive the inner filename from the .zst path by stripping the
    /// .zst extension*）。
    Single,
    /// 连第一个块都解不出来。
    Opaque(String),
}

/// tar 里一个条目的头部事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TarHead {
    /// 内部路径，分隔符统一成 `/`、折成 NFC。
    pub path: String,
    /// 未压缩大小。
    pub size: u64,
    /// 名字不是合法 UTF-8，这里存的是有损转换的结果。
    pub name_lossy: bool,
}

/// 「读第一条」这条路径的结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peek {
    /// 零解压读到的帧头。
    pub frame: FrameHeader,
    /// 里面装的是什么。
    pub inner: Inner,
}

impl Peek {
    /// 这个 tar 是不是**只裹了一份内容**——零额外成本的判定。
    ///
    /// 只有一个成员的 tar，总长必然是 512（头）+ `ceil(S/512)*512`（内容与对齐填充）+ 1024
    /// （两个全零结束块）。`Frame_Content_Size` 就是这个总长，而 `S` 来自已经解出来的
    /// 第一条，两样都已经在手上，于是这一问不需要任何额外 I/O 或解压。
    ///
    /// `None` 表示答不了：帧头没带大小（约 47% 的文件），或者里面根本不是 tar。
    #[must_use]
    pub fn single_member(&self) -> Option<bool> {
        let Inner::Tar(head) = &self.inner else {
            return None;
        };
        let total = self.frame.content_size?;
        let block = TAR_BLOCK as u64;
        let padded = head.size.div_ceil(block).checked_mul(block)?;
        Some(block.checked_add(padded)?.checked_add(2 * block)? == total)
    }
}

/// 零解压解出 zstd 的帧头。
///
/// # Errors
/// 魔数对不上（含旧版帧与可跳过帧）、保留位非零、或者字节不够解完帧头时返回错误。
pub fn parse_frame(head: &[u8]) -> Result<FrameHeader, ContainerError> {
    let magic = le32(head, 0).ok_or_else(|| not_zstd(head))?;
    if magic != MAGIC {
        if LEGACY_MAGIC.contains(&magic) {
            return Err(ContainerError::UnsupportedMethod(
                "zstd v0.8 之前的旧版帧格式".to_string(),
            ));
        }
        return Err(not_zstd(head));
    }
    let descriptor = *head.get(4).ok_or_else(|| truncated("帧头缺描述符字节"))?;
    // 规范的位定义（bit 7 是最高位）：7-6 大小标志、5 单段、4 未用、3 保留、
    // 2 内容校验和、1-0 字典 ID 标志。
    let fcs_flag = descriptor >> 6;
    let single_segment = descriptor & 0b0010_0000 != 0;
    if descriptor & 0b0000_1000 != 0 {
        return Err(ContainerError::Malformed(
            "zstd 帧头的保留位是 1，规范要求为 0".to_string(),
        ));
    }
    let has_checksum = descriptor & 0b0000_0100 != 0;
    let did_flag = descriptor & 0b0000_0011;

    let mut at = 5usize;
    // `Single_Segment_flag` 置位时没有 `Window_Descriptor`——窗口就是内容本身那么大。
    let window_from_descriptor = if single_segment {
        None
    } else {
        let raw = *head
            .get(at)
            .ok_or_else(|| truncated("帧头缺 Window_Descriptor"))?;
        at += 1;
        Some(window_size(raw))
    };

    let did_len = match did_flag {
        0 => 0usize,
        3 => 4,
        other => other as usize,
    };
    let dictionary_id = read_le(head, at, did_len)
        .ok_or_else(|| truncated("帧头缺 Dictionary_ID"))?
        .and_then(|raw| u32::try_from(raw).ok())
        .filter(|id| *id != 0);
    at += did_len;

    // 大小标志为 0 时字段长度取决于单段位：置位是 1 字节，否则**根本不提供**。
    let fcs_len = match fcs_flag {
        0 => usize::from(single_segment),
        1 => 2,
        2 => 4,
        _ => 8,
    };
    let content_size = read_le(head, at, fcs_len)
        .ok_or_else(|| truncated("帧头缺 Frame_Content_Size"))?
        // 2 字节那一档规范要求加 256，其余原样。
        .map(|raw| if fcs_len == 2 { raw + 256 } else { raw });
    at += fcs_len;

    Ok(FrameHeader {
        content_size,
        has_checksum,
        dictionary_id,
        window_size: window_from_descriptor
            .or(content_size)
            .unwrap_or(BLOCK_MAX_CAP),
        header_len: at,
    })
}

/// `Window_Descriptor` 一字节解出窗口大小（规范给的四行算式）。
fn window_size(raw: u8) -> u64 {
    let exponent = u64::from(raw >> 3);
    let mantissa = u64::from(raw & 0b0000_0111);
    let base = 1u64 << (10 + exponent);
    base + (base / 8) * mantissa
}

fn le32(buf: &[u8], at: usize) -> Option<u32> {
    buf.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// 读一个 `len` 字节的小端整数；`len` 为 0 时是 `Some(None)`（字段不存在，不是错）。
fn read_le(buf: &[u8], at: usize, len: usize) -> Option<Option<u64>> {
    if len == 0 {
        return Some(None);
    }
    let raw = buf.get(at..at + len)?;
    let mut value = 0u64;
    for (index, byte) in raw.iter().enumerate() {
        value |= u64::from(*byte) << (8 * index);
    }
    Some(Some(value))
}

fn not_zstd(head: &[u8]) -> ContainerError {
    let shown: Vec<String> = head.iter().take(4).map(|b| format!("{b:02X}")).collect();
    ContainerError::NotAContainer {
        kind: "zst",
        detail: if shown.is_empty() {
            "空文件".to_string()
        } else {
            format!("魔数是 {}", shown.join(" "))
        },
    }
}

fn truncated(detail: &str) -> ContainerError {
    ContainerError::Malformed(detail.to_string())
}

/// 帧头带外部字典 ID 时该报的那个错。
fn needs_dictionary(id: u32) -> ContainerError {
    ContainerError::NeedsDictionary(id)
}

/// **便宜那条路**：读帧头，再解第一个块看看里面装的是什么。
///
/// 读盘上界是 [`PEEK_LIMIT`]（约 131 KB），与文件本身多大无关。
///
/// # Errors
/// 文件读不动、不是 zstd 帧、或者帧头说需要外部字典时返回错误。
pub fn peek(library: &dyn LibraryFs, path: &Path) -> Result<Peek, ContainerError> {
    let head = library.read_head(path, PEEK_LIMIT)?;
    peek_bytes(&head)
}

/// 同 [`peek`]，但字节已经在手上——头部抽样那一侧走的是这条（`crate::header`）。
///
/// # Errors
/// 同 [`peek`]。
pub fn peek_bytes(head: &[u8]) -> Result<Peek, ContainerError> {
    let frame = parse_frame(head)?;
    if let Some(id) = frame.dictionary_id {
        return Err(needs_dictionary(id));
    }
    // 只喂这个帧自己的上界那么多字节：窗口小的帧不必把 131 KB 全塞进去。
    let limit = usize::try_from(frame.peek_limit()).unwrap_or(PEEK_LIMIT);
    let prefix = &head[..head.len().min(limit)];
    let (plain, failure) = decompress_prefix(prefix, PEEK_PLAINTEXT);
    let inner = if plain.len() < TAR_BLOCK {
        // 明文连一个 tar 块都不够。**判据与 [`list`] 那边的 `got == TAR_BLOCK` 一致**：
        // 只有「顺顺当当读到末尾、就是没有 512 字节」才敢说它不是 tar——那时这个流
        // 本来就短，短过一个 tar 头。中途出了错就什么都不敢说，如实报读不出来。
        match failure {
            Some(why) => Inner::Opaque(why),
            None => Inner::Single,
        }
    } else if looks_like_tar(&plain[..TAR_BLOCK]) {
        match first_tar_entry(&plain) {
            Ok(Some(head)) => Inner::Tar(head),
            Ok(None) => Inner::TarUnreadable("tar 里一条也没有".to_string()),
            Err(error) => Inner::TarUnreadable(error.to_string()),
        }
    } else {
        Inner::Single
    };
    Ok(Peek { frame, inner })
}

/// 解一个**截断的前缀**，拿到多少算多少。
///
/// 前缀本来就不完整，解到末尾必然报错——那不是文件坏了，是我们故意只读了这么多。
/// 于是错误只在「一个字节都没解出来」时才有意义，返回值把两者分开。
fn decompress_prefix(prefix: &[u8], limit: usize) -> (Vec<u8>, Option<String>) {
    let mut decoder = match zstd::Decoder::new(Cursor::new(prefix)) {
        Ok(decoder) => decoder,
        Err(error) => return (Vec::new(), Some(error.to_string())),
    };
    let mut out = vec![0u8; limit];
    let mut filled = 0usize;
    let mut failure = None;
    while filled < out.len() {
        match decoder.read(&mut out[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(error) => {
                failure = Some(error.to_string());
                break;
            }
        }
    }
    out.truncate(filled);
    (out, failure)
}

/// 这 512 字节像不像一个 tar 头。
///
/// 两条判据取其一：ustar / pax / GNU 都在偏移 257 写 `ustar`；更老的 v7 tar 没有魔数，
/// 那就验头部校验和——POSIX.1-2024 定义它是「512 字节的无符号字节简单求和，
/// 算的时候 `chksum` 那 8 字节当作 8 个空格」。
fn looks_like_tar(block: &[u8]) -> bool {
    if block.len() < TAR_BLOCK {
        return false;
    }
    if block.get(TAR_MAGIC_AT..TAR_MAGIC_AT + 5) == Some(b"ustar") {
        return true;
    }
    checksum_ok(block)
}

fn checksum_ok(block: &[u8]) -> bool {
    let Some(stored) = octal(&block[TAR_CHKSUM_AT..TAR_CHKSUM_AT + TAR_CHKSUM_LEN]) else {
        return false;
    };
    let sum: u64 = block
        .iter()
        .enumerate()
        .map(|(at, byte)| {
            if (TAR_CHKSUM_AT..TAR_CHKSUM_AT + TAR_CHKSUM_LEN).contains(&at) {
                u64::from(b' ')
            } else {
                u64::from(*byte)
            }
        })
        .sum();
    sum == stored
}

/// tar 的数值字段：前导零填充的八进制 ASCII，以空格或 NUL 收尾。
fn octal(raw: &[u8]) -> Option<u64> {
    let text = raw
        .iter()
        .take_while(|b| **b != 0 && **b != b' ')
        .copied()
        .collect::<Vec<u8>>();
    let text = String::from_utf8(text).ok()?;
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    u64::from_str_radix(text, 8).ok()
}

/// 从一段明文里取 tar 的第一个真条目。
///
/// **不自己数 512 字节**：pax 的第一个块是 `PaxHeader/…` 伪条目，真条目在偏移 1024，
/// 而 GNU longname 又是另一种前缀块。走条目链才认得出它们（见 [`member`]）。
fn first_tar_entry(plain: &[u8]) -> io::Result<Option<TarHead>> {
    let mut archive = tar::Archive::new(Cursor::new(plain));
    let mut pending = None;
    for entry in archive.entries()? {
        let mut entry = entry?;
        if let Some(head) = member(&mut entry, &mut pending)? {
            return Ok(Some(head));
        }
    }
    Ok(None)
}

/// 把 tar 条目链上的一条折成一个内部文件；**伪条目返回 `None`**。
///
/// ## 为什么不能全交给 `tar` crate
///
/// 它对 GNU longname 的处理带一个前提：那个伪条目自己得是**认得出来的头**
/// （`as_gnu()` 或 `as_ustar()` 非空，即 magic 处写着 `ustar`）。**真库里不满足。**
/// NDS 那批 559 个 `.tar.zst` 实测有 414 个（74%）的 `././@LongLink` 块 magic 处
/// 全是零——老式 v7 头配 GNU 的 `typeflag='L'`。于是 `tar` crate 把伪条目当成了一个
/// 真文件交出来，而真条目的名字停在 100 字节处被截断：
/// `KORG DS-10 电子乐合成模拟软件(JP)` 而不是 `…(Kulabbc)(64Mb).nds`。
/// 系统 `bsdtar` 认这种块（它只看 `typeflag`），所以这不是文件坏了。
///
/// 名字错了，票 09 与票 10 就拿不到它们要的那个文件——**所以这一层自己认**。
fn member<R: Read>(
    entry: &mut tar::Entry<'_, R>,
    pending: &mut Option<(String, bool)>,
) -> io::Result<Option<TarHead>> {
    let kind = entry.header().entry_type();
    if kind.is_gnu_longname() {
        // 伪条目的**内容**就是下一条的真名字，以 NUL 收尾。
        // **截着读**：这个长度来自文件里的一个字段，坏了的话它能让扫描器一口气申请几个 GB
        // （与 `zip` 那一侧对中央目录长度设上限是同一条道理）。
        let mut raw = Vec::new();
        entry.take(MAX_LONG_NAME).read_to_end(&mut raw)?;
        while raw.last() == Some(&0) {
            raw.pop();
        }
        *pending = Some(super::charset::decode_path(&raw));
        return Ok(None);
    }
    if kind.is_gnu_longlink() || kind.is_pax_local_extensions() || kind.is_pax_global_extensions() {
        // 同样是伪条目：它们描述的是**下一条**，本身没有内容身份。
        // （magic 正常时 `tar` crate 已经吃掉了，这里根本看不到。）
        return Ok(None);
    }
    let (path, name_lossy) = match pending.take() {
        Some(long) => long,
        None => super::charset::decode_path(&entry.path_bytes()),
    };
    Ok(Some(TarHead {
        path,
        size: entry.size(),
        name_lossy,
    }))
}

/// 单文件 `.zst` 的内部名：外层文件名去掉 `.zst`。
///
/// **后缀按大小写不敏感剥**，与 [`ContainerKind::for_path`] 认扩展名的那一步同一个判据
/// （它走 `extension_lower`）。两边不一致的话，`游戏.vpk.Zst` 会被认成容器，
/// 内部名却还挂着 `.Zst`。
fn derived_name(path: &Path) -> String {
    let name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    // `get` 而不是索引：切点落在一个汉字中间时它给 `None`，索引会 panic。
    // 后缀是纯 ASCII，所以真命中时切点必然是字符边界。
    let cut = name
        .len()
        .checked_sub(4)
        .filter(|at| {
            name.get(*at..)
                .is_some_and(|tail| tail.eq_ignore_ascii_case(".zst"))
        })
        .unwrap_or(name.len());
    nfc(&name[..cut]).into_owned()
}

/// 一个 `.zst` 内部是怎么组织的。回头取内容时要按它分派。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Shape {
    /// 里面是一个 tar，条目按顺序排在同一条流上。
    Tar,
    /// 整个流就是一个文件。
    Single,
}

/// **贵那条路**：完整解压，读出全部内部条目。
///
/// tar 是顺序格式，没有捷径（调研 2.4）。代价 ≈ 把这个文件读一遍——zstd 解压
/// 1550 MB/s 对机械盘 100–200 MB/s，解压在流水线里等于免费，瓶颈全在磁盘。
///
/// **调用方必须把结论存下来**，否则每次扫描都要重付。扫描器走的是
/// `catalog` 的 `container` / `container_entry` 两张表，键与增量扫描的三元组一致。
pub(super) fn list(library: &dyn LibraryFs, path: &Path) -> Result<Listing, ContainerError> {
    let head = library.read_head(path, MAX_FRAME_HEADER)?;
    let frame = parse_frame(&head)?;
    if let Some(id) = frame.dictionary_id {
        return Err(needs_dictionary(id));
    }

    let file = library.open(path)?;
    let mut decoder = zstd::Decoder::new(file).map_err(ContainerError::Io)?;
    // 先取 512 字节明文认一认里面是不是 tar，再把这 512 字节接回流的前面——
    // 解压器退不回去，但 `Chain` 可以（`tar` crate 对底层只要求 `Read`）。
    let mut lead = [0u8; TAR_BLOCK];
    let got = read_up_to(&mut decoder, &mut lead)?;
    let stream = Cursor::new(lead[..got].to_vec()).chain(decoder);

    let (entries, shape) = if got == TAR_BLOCK && looks_like_tar(&lead) {
        (list_tar(stream)?, Shape::Tar)
    } else {
        // 不是 tar：整个流就是一个文件。大小优先信帧头；帧头没写就只能量一遍
        // ——量一遍等于解一遍，而这正是 `Frame_Content_Size` 缺席时躲不掉的代价。
        let size = match frame.content_size {
            Some(size) => size,
            None => measure(stream)?,
        };
        (
            vec![InnerEntry {
                path: derived_name(path),
                size,
                crc32: None,
                is_dir: false,
                block: (size > 0).then_some(0),
                name_lossy: false,
            }],
            Shape::Single,
        )
    };

    // 整条流是一个顺序单元：**它天生就是 solid**，读中间那个条目躲不开前面的字节。
    // 于是全部有内容的条目都记在第 0 块上，`Contents::is_solid` 因此如实报 true。
    let blocks = usize::from(entries.iter().any(|entry| entry.block.is_some()));
    Ok(Listing {
        kind: ContainerKind::Zstd,
        contents: Contents { entries, blocks },
        locator: Locator::Zstd(shape),
    })
}

fn list_tar(stream: impl Read) -> Result<Vec<InnerEntry>, ContainerError> {
    let mut archive = tar::Archive::new(stream);
    let mut entries = Vec::new();
    let mut pending = None;
    for entry in archive.entries().map_err(map_tar_error)? {
        let mut entry = entry.map_err(map_tar_error)?;
        let is_dir = entry.header().entry_type().is_dir();
        let Some(head) = member(&mut entry, &mut pending).map_err(map_tar_error)? else {
            continue;
        };
        entries.push(InnerEntry {
            path: head.path,
            size: head.size,
            // **格式里没有这一栏。** 两层都没有「成员 → 内容哈希」的映射表，
            // 想要 CRC-32 只能把这一条解出来自己算，那是识别未命中之后的事。
            crc32: None,
            is_dir,
            block: (!is_dir && head.size > 0).then_some(0),
            name_lossy: head.name_lossy,
        });
    }
    Ok(entries)
}

/// 把整条流排空，返回一共多少字节。`Frame_Content_Size` 缺席时只能这么量。
fn measure(mut stream: impl Read) -> Result<u64, ContainerError> {
    io::copy(&mut stream, &mut io::sink()).map_err(ContainerError::Io)
}

/// 尽量把 `buf` 填满；到流末尾就停。返回实际填了多少。
fn read_up_to(reader: &mut dyn Read, buf: &mut [u8]) -> Result<usize, ContainerError> {
    let mut filled = 0usize;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(ContainerError::Io(error)),
        }
    }
    Ok(filled)
}

/// tar 那一层的错误统一归到「结构读不下去」——除非底层的读本身就失败了。
fn map_tar_error(error: io::Error) -> ContainerError {
    if matches!(
        error.kind(),
        io::ErrorKind::UnexpectedEof | io::ErrorKind::InvalidData
    ) {
        ContainerError::Malformed(format!("tar 条目链读不下去：{error}"))
    } else {
        ContainerError::Io(error)
    }
}

/// 按计划读一遍内部条目。
///
/// **整条流只走一趟**：`.tar.zst` 是一个顺序单元，想要的条目按次序拿，中间不想要的
/// 顺着排空（解压器跳不过去），最后一个想要的拿完就整条中止。
pub(super) fn read_entries(
    library: &dyn LibraryFs,
    path: &Path,
    contents: &Contents,
    shape: Shape,
    plan: &ReadPlan,
    each: &mut dyn FnMut(&InnerEntry, &mut dyn Read) -> io::Result<()>,
) -> Result<ReadStats, ContainerError> {
    let mut stats = ReadStats::default();

    // 没有内容的条目（目录、空文件、符号链接）不必碰盘，也不该让整条流白解一趟。
    for (index, entry) in contents.entries.iter().enumerate() {
        if plan.demand(index).wanted() && !entry.has_content() {
            each(entry, &mut io::empty())?;
            stats.entries_read += 1;
        }
    }

    let mut left = contents
        .entries
        .iter()
        .enumerate()
        .filter(|(index, entry)| entry.has_content() && plan.demand(*index).wanted())
        .count();
    if left == 0 {
        return Ok(stats);
    }

    let file = library.open(path)?;
    let mut decoder = zstd::Decoder::new(file).map_err(ContainerError::Io)?;
    // 整条流是一个顺序单元，一趟走完——**它就是这里的「一个块」**。
    stats.blocks_decoded = 1;

    match shape {
        Shape::Single => {
            let Some((index, entry)) = contents
                .entries
                .iter()
                .enumerate()
                .find(|(_, entry)| entry.has_content())
            else {
                return Ok(stats);
            };
            let handed = hand_entry(entry, &mut decoder, plan.demand(index), each)?;
            stats.bytes_decompressed = stats.bytes_decompressed.saturating_add(handed);
            stats.entries_read += 1;
        }
        Shape::Tar => {
            let mut archive = tar::Archive::new(decoder);
            let mut pending = None;
            let mut index = 0usize;
            for entry in archive.entries().map_err(map_tar_error)? {
                let mut entry = entry.map_err(map_tar_error)?;
                // **伪条目在这里也要跳掉**，否则序号会与清单错位——错位就意味着
                // 把甲的字节当成乙的交出去，那是最坏的一种错。
                if member(&mut entry, &mut pending)
                    .map_err(map_tar_error)?
                    .is_none()
                {
                    continue;
                }
                let this = index;
                index += 1;
                let Some(inner) = contents.entries.get(this) else {
                    break;
                };
                if inner.has_content() && plan.demand(this).wanted() {
                    let handed = hand_entry(inner, &mut entry, plan.demand(this), each)?;
                    stats.bytes_decompressed = stats.bytes_decompressed.saturating_add(handed);
                    stats.entries_read += 1;
                    left -= 1;
                    if left == 0 {
                        // 想要的都拿到了——整条流就此中止，后面的字节一个都不解。
                        // **这里不排空**：排空等于把剩下的全解一遍，正是要省掉的那件事。
                        break;
                    }
                }
                // 后面还有想要的：这一条剩下的字节必须顺着走完——顺序流跳不过去
                // （调研 2.4：对 zstd 流「跳过」只能靠真正解压再丢掉）。**这些字节
                // 要算进账里**：它们是真的解出来了，只是被丢掉，而这正是
                // `bytes_decompressed` 想让人看见的那个代价。
                let skipped = drain(&mut entry)?;
                stats.bytes_decompressed = stats.bytes_decompressed.saturating_add(skipped);
            }
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 帧头四个字段各自解得出来() {
        // 描述符 0x84：FCS 标志 2（4 字节）、非单段、带校验和、无字典。
        // 窗口描述符 0x58 → exponent 11、mantissa 0 → 2 MiB。
        let mut head = vec![0x28, 0xB5, 0x2F, 0xFD, 0x84, 0x58];
        head.extend_from_slice(&18_011_648u32.to_le_bytes());
        let frame = parse_frame(&head).expect("解得出来");
        assert_eq!(frame.content_size, Some(18_011_648));
        assert!(frame.has_checksum);
        assert_eq!(frame.dictionary_id, None);
        assert_eq!(frame.window_size, 2 << 20);
        assert_eq!(frame.header_len, 10);
        assert_eq!(frame.block_maximum_size(), BLOCK_MAX_CAP);
    }

    #[test]
    fn tar_zstd_那种帧头没有大小也没有校验和() {
        // 实测 `tar --zstd -cf` 的产物：描述符 0x00，帧头总长 6 字节。
        // **这就是「不能假设 Frame_Content_Size 存在」的那 47%。**
        let head = [0x28, 0xB5, 0x2F, 0xFD, 0x00, 0x58];
        let frame = parse_frame(&head).expect("解得出来");
        assert_eq!(frame.content_size, None);
        assert!(!frame.has_checksum);
        assert_eq!(frame.header_len, 6);
    }

    #[test]
    fn 单段帧的大小只有一字节而且就是窗口() {
        // 描述符 0x20：单段位置位、FCS 标志 0 → 大小占 1 字节、没有窗口描述符。
        let head = [0x28, 0xB5, 0x2F, 0xFD, 0x20, 0x40];
        let frame = parse_frame(&head).expect("解得出来");
        assert_eq!(frame.content_size, Some(0x40));
        assert_eq!(frame.window_size, 0x40);
        assert_eq!(frame.header_len, 6);
    }

    #[test]
    fn 两字节的大小要加二百五十六() {
        // 描述符 0x40：FCS 标志 1 → 2 字节，规范要求读出来的值加 256。
        let head = [0x28, 0xB5, 0x2F, 0xFD, 0x40, 0x58, 0x10, 0x00];
        assert_eq!(parse_frame(&head).unwrap().content_size, Some(0x10 + 256));
    }

    #[test]
    fn 字典_id_零解压就查得出来() {
        // 描述符 0x03：字典 ID 标志 3 → 4 字节。
        let mut head = vec![0x28, 0xB5, 0x2F, 0xFD, 0x03, 0x58];
        head.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        let frame = parse_frame(&head).expect("解得出来");
        assert_eq!(frame.dictionary_id, Some(0xDEAD_BEEF));
        assert_eq!(frame.header_len, 10);
        // ID 为 0 与「没写」是同一件事：那时帧到底要不要字典，帧头答不了。
        let zero = [0x28, 0xB5, 0x2F, 0xFD, 0x01, 0x58, 0x00];
        assert_eq!(parse_frame(&zero).unwrap().dictionary_id, None);
    }

    #[test]
    fn 旧版帧与不是_zstd_是两句不同的话() {
        let legacy = [0x22, 0xB5, 0x2F, 0xFD, 0x00, 0x00];
        assert!(matches!(
            parse_frame(&legacy),
            Err(ContainerError::UnsupportedMethod(_))
        ));
        let alien = [b'P', b'K', 3, 4, 0, 0];
        assert!(matches!(
            parse_frame(&alien),
            Err(ContainerError::NotAContainer { .. })
        ));
    }

    #[test]
    fn 保留位为一是结构坏了() {
        let head = [0x28, 0xB5, 0x2F, 0xFD, 0x08, 0x58];
        assert!(matches!(
            parse_frame(&head),
            Err(ContainerError::Malformed(_))
        ));
    }

    #[test]
    fn 读第一条的上界是规范算出来的() {
        assert_eq!(PEEK_LIMIT, 14 + 3 + 131_072);
        // 窗口比 128 KiB 小的帧，上界跟着窗口走而不是跟着这个全局上限走。
        let head = [0x28, 0xB5, 0x2F, 0xFD, 0x00, 0x00];
        let frame = parse_frame(&head).expect("解得出来");
        assert_eq!(frame.window_size, 1 << 10);
        assert_eq!(frame.peek_limit(), 6 + 3 + 1024);
    }

    #[test]
    fn tar_头部校验和按_posix_那条算法复算() {
        // 造一个最小的 v7 头（没有 ustar 魔数），只靠校验和认出来。
        let mut block = [0u8; TAR_BLOCK];
        block[..5].copy_from_slice(b"a.bin");
        block[124..135].copy_from_slice(b"00000000144");
        block[156] = b'0';
        let sum: u32 = block
            .iter()
            .enumerate()
            .map(|(at, byte)| {
                if (TAR_CHKSUM_AT..TAR_CHKSUM_AT + TAR_CHKSUM_LEN).contains(&at) {
                    u32::from(b' ')
                } else {
                    u32::from(*byte)
                }
            })
            .sum();
        let text = format!("{sum:06o}\0 ");
        block[TAR_CHKSUM_AT..TAR_CHKSUM_AT + TAR_CHKSUM_LEN].copy_from_slice(text.as_bytes());
        assert!(looks_like_tar(&block), "校验和自洽就该认出来");
        // 改一个字节，校验和立刻对不上——这条判据不是摆设。
        block[0] = b'z';
        assert!(!looks_like_tar(&block));
    }

    #[test]
    fn 单文件的内部名是外层名去掉后缀() {
        assert_eq!(derived_name(Path::new("/库/PSV/游戏.vpk.zst")), "游戏.vpk");
        assert_eq!(derived_name(Path::new("/库/A.ZST")), "A");
        assert_eq!(derived_name(Path::new("/库/游戏.vpk.Zst")), "游戏.vpk");
        assert_eq!(derived_name(Path::new("/库/没有后缀")), "没有后缀");
    }
}
