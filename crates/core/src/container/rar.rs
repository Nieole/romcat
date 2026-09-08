//! rar 的零解压层：**头部解析自己实现**，不引入内嵌 UnRAR 源码的库。
//!
//! ## 为什么自己写
//!
//! `unrar` crate 把 UnRAR 的 C++ 源码内嵌了进来，那份许可传染：禁止用于开发兼容的
//! 压缩器、分发时必须在**许可文件、文档、以及产物包的源码注释**三处附上原文，
//! Debian 把它归在 non-free（ADR-0014 的「UnRAR 许可」一节）。
//!
//! 而这一层要的只是**读头**——内部文件的名字、未压缩大小与 CRC-32——根本不需要解压。
//! 头部结构是公开的（RAR5 有官方 technote，RAR4 只能照 unrar 源码里的读取顺序），
//! 自己写就彻底绕开了那份许可。**代价是解压这条路走不了**：
//! `read_entries` 只交得出「原样存放」的条目，压缩过的一律如实报「解不了」。
//!
//! ## 两种格式，两套布局
//!
//! - **RAR4**（签名 `Rar!\x1a\x07\x00`）：定长字段，`SIZEOF_FILEHEAD3 = 32`
//!   从块首算起。**CRC-32 无条件存在**（unrar `arcread.cpp` 里直接
//!   `hd->FileHash.Type=HASH_CRC32` 再 `Get4()`，没有任何旗标判断）。
//!   文件名可能带 unrar 私有的 Unicode 编码段（`LHD_UNICODE`），见 `decode_unicode_name`。
//! - **RAR5**（签名 `Rar!\x1a\x07\x01\x00`）：全部字段是变长整数，**没有固定偏移，
//!   只有固定顺序**。CRC-32 是**可选**的（file flag `0x0004`），名字是 UTF-8。
//!
//! ## 三个必须显式处理的坑
//!
//! 1. ⭐ **分卷时非末段的校验和是「打包后数据」的**。technote 原文：*For files split
//!    between volumes it contains CRC32 of file packed data contained in current volume
//!    for all file parts except the last.* 真机实测一份三卷的 RAR5：同一个
//!    `src/b.bin`，第一卷记 `DDFAE192`、第二卷记 `B31EA58F`、第三卷（末段）才记
//!    `9506C014`——而未分卷时那个文件的 CRC 正是 `9506C014`。`unrar lt` 把前两个
//!    直接印成 `Pack-CRC32`。**拿前两个去撞 DAT 必然落空**，所以这里只在见到末段
//!    （`SPLIT_AFTER` 未置位）时才把 CRC 记下来，见不到就是 `None`。
//! 2. **RAR5 只写 BLAKE2sp 时没有 CRC-32**。`-htb` 压出来的容器 file flag 的
//!    `0x0004` 位是 0，哈希搬进 extra area 的 `0x02` 记录（32 字节 BLAKE2sp）。
//!    而 No-Intro / Redump / TOSEC 一条 BLAKE2 都不记——这种容器
//!    **要完整解压才认得出来**（[`Contents::needs_full_decompress`]），
//!    与 zst 同一档处置。
//! 3. **solid 比 7z 更糟**：RAR 的头部**没有**等价于 7z `NumUnPackStreamsInFolders`
//!    的块映射，一个 solid 流只能从头顺序解到底（ADR-0014）。好在 solid 与否零解压
//!    读得出来（RAR5 的 compression info 第 6 位、RAR4 的 `LHD_SOLID`），
//!    所以报告说得出「这个容器解起来贵」。
//!
//! 出处：<https://www.rarlab.com/technote.htm>（RAR 5.0 官方规范）与 unrar 源码的
//! `headers.hpp` / `arcread.cpp` / `encname.cpp`，逐条见
//! `docs/research/containers-and-compressed-images.md` 1.3、1.7.3、1.8。

use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::{
    ContainerError, ContainerKind, Contents, InnerEntry, Listing, Locator, ReadPlan, ReadStats,
    charset, hand_entry, malformed, volume,
};
use crate::fs::{LibraryFs, ReadSeek};

/// RAR 1.4 的签名。认出来只为了说清楚「这是太老的 RAR，本程序读不了」。
const SIG_RAR14: [u8; 4] = *b"RE~^";
/// RAR4 的签名（7 字节）。
const SIG_RAR4: [u8; 7] = *b"Rar!\x1a\x07\x00";
/// RAR5 的签名（8 字节）。RAR7 沿用同一个。
const SIG_RAR5: [u8; 8] = *b"Rar!\x1a\x07\x01\x00";

/// 一个头部最多多长。真库里最长的头也只有几百字节；再大就当结构坏了，
/// 不然一个坏字段能让扫描器一口气申请几个 GB。
const MAX_HEADER: usize = 1 << 20;
/// 一卷最多认多少个块。防的是坏字节导致的原地打转。
const MAX_BLOCKS: usize = 1 << 20;
/// 一组分卷最多走多少卷。
const MAX_VOLUMES: usize = 10_000;

// ── RAR4 的旗标（unrar `headers.hpp`）─────────────────────────────────────────
/// 新式分卷命名（`.partNN.rar`）。
const MHD_NEWNUMBERING: u16 = 0x0010;
/// 头部加密：连文件名都看不到。
const MHD_PASSWORD: u16 = 0x0080;

/// 该文件的数据从上一卷续来。
const LHD_SPLIT_BEFORE: u16 = 0x0001;
/// 该文件的数据续到下一卷。
const LHD_SPLIT_AFTER: u16 = 0x0002;
/// 数据加密（名字与 CRC 照样读得到）。
const LHD_PASSWORD: u16 = 0x0004;
/// 该文件是 solid 流的一部分。
const LHD_SOLID: u16 = 0x0010;
/// 用 64 位大小字段。
const LHD_LARGE: u16 = 0x0100;
/// 文件名里带 unrar 私有的 Unicode 编码段。
const LHD_UNICODE: u16 = 0x0200;
/// 这个块后面跟着一段数据，长度写在头长之后的 4 字节里。
const LONG_BLOCK: u16 = 0x8000;
/// 目录的判据：窗口大小那三位全置。
const LHD_WINDOWMASK: u16 = 0x00E0;

/// 块类型：主头部。
const HEAD3_MAIN: u8 = 0x73;
/// 块类型：文件头。
const HEAD3_FILE: u8 = 0x74;
/// 块类型：容器结束（规范里叫 end of archive header）。
const HEAD3_ENDARC: u8 = 0x7B;
/// 容器结束块的旗标：后面还有一卷。
const EARC_NEXT_VOLUME: u16 = 0x0001;

/// 文件头定长部分的长度，**从块首算起**（unrar 的 `SIZEOF_FILEHEAD3`）。
const SIZEOF_FILEHEAD3: usize = 32;
/// 每个块前 7 字节的公共头（`SIZEOF_SHORTBLOCKHEAD`）。
const SIZEOF_SHORTBLOCKHEAD: usize = 7;

// ── RAR5 的旗标（technote.htm）───────────────────────────────────────────────
/// 头部里有 extra area。
const HFL_EXTRA: u64 = 0x0001;
/// 头部后面跟着数据。
const HFL_DATA: u64 = 0x0002;
/// 数据从上一卷续来。
const HFL_SPLIT_BEFORE: u64 = 0x0008;
/// 数据续到下一卷。
const HFL_SPLIT_AFTER: u64 = 0x0010;

/// 头部类型：容器总头（规范里叫 main archive header）。
const HEAD5_MAIN: u64 = 1;
/// 头部类型：文件头。
const HEAD5_FILE: u64 = 2;
/// 头部类型：服务头（quick open 记录、注释等）。**它不是内部文件。**
const HEAD5_SERVICE: u64 = 3;
/// 头部类型：容器头加密。见到它就是整个容器的头都加密了。
const HEAD5_CRYPT: u64 = 4;
/// 头部类型：容器结束。
const HEAD5_ENDARC: u64 = 5;

/// 容器旗标：卷号字段存在。
const MHFL_VOLNUMBER: u64 = 0x0002;

/// 文件旗标：这是个目录。
const FHFL_DIRECTORY: u64 = 0x0001;
/// 文件旗标：带 Unix 时间。
const FHFL_UTIME: u64 = 0x0002;
/// 文件旗标：**CRC32 字段存在**。
const FHFL_CRC32: u64 = 0x0004;
/// 文件旗标：未压缩大小未知。
const FHFL_UNPUNKNOWN: u64 = 0x0008;

/// extra area 记录：文件加密。
const FHEXTRA_CRYPT: u64 = 0x01;
/// 文件加密记录的旗标：**校验和被密钥搅过**。
///
/// technote 原文：*Use tweaked checksums… If this flag is set, all file checksums are
/// modified according to encryption key value.* `unrar lt` 把这种值印成
/// **`CRC32 MAC`** 而不是 `CRC32`——它不是那份数据的 CRC-32，拿去撞 DAT 必然落空。
const FHEXTRA_CRYPT_TWEAKED: u64 = 0x0002;
/// extra area 记录：文件哈希（眼下只有 BLAKE2sp）。
const FHEXTRA_HASH: u64 = 0x02;

/// 容器结束块的旗标：后面还有一卷。
const EHFL_NEXT_VOLUME: u64 = 0x0001;

/// compression info 里的 solid 位：**接着用上一个文件留下的压缩字典**。
const COMPINFO_SOLID: u64 = 0x0040;
/// compression info 里的压缩方法（0 = 原样存放）。
const COMPINFO_METHOD: u64 = 0x0380;

/// 一个内部条目的数据落在哪一卷的哪一段上。跨卷的文件有好几段。
#[derive(Debug, Clone, Copy)]
struct Piece {
    volume: usize,
    offset: u64,
    len: u64,
}

/// 回头取内容要用的定位信息。
#[derive(Debug, Clone)]
pub(super) struct EntryLocator {
    pieces: Vec<Piece>,
    /// 压缩方法。**只有 0（原样存放）读得了**——解压那条路要 UnRAR 的算法。
    stored: bool,
    encrypted: bool,
    /// 手里的字节是残的：这一条的头一段在别的卷上（从非入口卷读起时才会这样）。
    partial: bool,
}

/// 一组分卷的全部定位信息。
#[derive(Debug)]
pub(super) struct RarLocator {
    /// 这一组走过的卷，按顺序。第 0 卷就是调用方给的那条路径。
    volumes: Vec<PathBuf>,
    entries: Vec<EntryLocator>,
}

/// 按字节走的读头游标：越界一律返回 `None`，不 panic。
struct Cursor<'b> {
    bytes: &'b [u8],
    at: usize,
}

impl<'b> Cursor<'b> {
    fn new(bytes: &'b [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn u8(&mut self) -> Option<u8> {
        let byte = *self.bytes.get(self.at)?;
        self.at += 1;
        Some(byte)
    }

    fn u16(&mut self) -> Option<u16> {
        let raw = self.bytes.get(self.at..self.at + 2)?;
        self.at += 2;
        Some(u16::from_le_bytes([raw[0], raw[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        let raw = self.bytes.get(self.at..self.at + 4)?;
        self.at += 4;
        Some(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
    }

    /// RAR5 的变长整数：每字节 7 位有效，最高位是「还有下一字节」。
    ///
    /// 上限 10 字节——再多就装不进 64 位，那时与其悄悄截断不如说结构坏了。
    fn vint(&mut self) -> Option<u64> {
        let mut value = 0u64;
        for shift in (0..70).step_by(7) {
            let byte = self.u8()?;
            value |= u64::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    fn take(&mut self, len: usize) -> Option<&'b [u8]> {
        let raw = self.bytes.get(self.at..self.at + len)?;
        self.at += len;
        Some(raw)
    }

    fn skip_to(&mut self, at: usize) {
        self.at = at.min(self.bytes.len());
    }
}

/// 一个正在拼的内部条目。分卷时同一个文件会在好几卷的头里各出现一次。
#[derive(Debug)]
struct Building {
    path: String,
    name_lossy: bool,
    size: u64,
    /// **只在见到末段时才填**。非末段记的是打包后数据的 CRC（见模块文档第 1 条）。
    crc32: Option<u32>,
    is_dir: bool,
    solid: bool,
    stored: bool,
    encrypted: bool,
    pieces: Vec<Piece>,
    /// 上一段说了「续到下一卷」，还在等后面接上。
    open: bool,
    /// 这一条是从半路开始读的：头一段在别的卷上，手里的字节是残的。
    partial: bool,
}

/// 走完一卷之后要接着做什么。
struct VolumeOutcome {
    /// 这一卷说了后面还有一卷。
    next_volume: bool,
    /// 分卷用新式命名（`.partNN.rar`）还是旧式（`.rNN`）。
    new_numbering: bool,
}

/// 零解压读出一个 rar（或一整组 rar 分卷）的内部构成。
pub(super) fn list(library: &dyn LibraryFs, path: &Path) -> Result<Listing, ContainerError> {
    let mut volumes = vec![path.to_path_buf()];
    let mut building: Vec<Building> = Vec::new();
    let mut index = 0usize;
    loop {
        let outcome = read_volume(library, &volumes[index], index, &mut building)?;
        if !outcome.next_volume {
            break;
        }
        // **下一卷的名字按 unrar 那两套算法算**，编号方式由主头部的旗标给出，不是猜的。
        let next =
            volume::next_volume_path(&volumes[index], outcome.new_numbering).ok_or_else(|| {
                ContainerError::MissingVolume {
                    detail: format!(
                        "{} 说后面还有一卷，而这个名字算不出下一卷该叫什么",
                        display(&volumes[index])
                    ),
                }
            })?;
        // 开不开得了就是「那一卷在不在」。缺一卷时**不交半张清单**：
        // 跨卷的条目 CRC 还停在「打包后数据」那一档，而报告里「读出来了」的含义是
        // 「这就是里面的全部」。如实说缺了哪一卷，人才知道下一步是去找那一卷。
        if library.open(&next).is_err() {
            return Err(ContainerError::MissingVolume {
                detail: format!("缺 {}", display(&next)),
            });
        }
        volumes.push(next);
        index += 1;
        if volumes.len() > MAX_VOLUMES {
            return Err(malformed(format!("分卷超过 {MAX_VOLUMES} 卷，不再往下走")));
        }
    }

    let mut entries = Vec::with_capacity(building.len());
    let mut locators = Vec::with_capacity(building.len());
    let mut blocks = 0usize;
    let mut current_block: Option<usize> = None;
    for item in building {
        let has_content = !item.is_dir && item.size > 0;
        // RAR 的 solid 是一条**连着往下**的流：solid 位置位的条目接着用上一个条目
        // 留下的字典，于是它与前一条同属一个块。头部没有块映射索引（比 7z 更糟，
        // ADR-0014），所以块只能这么一路数下来。
        let block = if has_content {
            let block = match current_block {
                Some(block) if item.solid => block,
                _ => {
                    let block = blocks;
                    blocks += 1;
                    block
                }
            };
            current_block = Some(block);
            Some(block)
        } else {
            None
        };
        entries.push(InnerEntry {
            path: item.path,
            size: item.size,
            crc32: item.crc32,
            is_dir: item.is_dir,
            block,
            name_lossy: item.name_lossy,
        });
        locators.push(EntryLocator {
            pieces: item.pieces,
            stored: item.stored,
            encrypted: item.encrypted,
            partial: item.partial,
        });
    }

    Ok(Listing {
        kind: ContainerKind::Rar,
        contents: Contents { entries, blocks },
        locator: Locator::Rar(Box::new(RarLocator {
            volumes,
            entries: locators,
        })),
    })
}

fn display(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// 走一遍一卷的全部头部，把内部条目拼进 `building`。
fn read_volume(
    library: &dyn LibraryFs,
    path: &Path,
    volume_index: usize,
    building: &mut Vec<Building>,
) -> Result<VolumeOutcome, ContainerError> {
    let mut file = library.open(path)?;
    let len = file.seek(SeekFrom::End(0))?;
    let head = read_at(file.as_mut(), 0, SIG_RAR5.len().min(len as usize))?;
    if head.starts_with(&SIG_RAR5) {
        read_volume5(file.as_mut(), len, volume_index, building)
    } else if head.starts_with(&SIG_RAR4) {
        read_volume4(file.as_mut(), len, volume_index, building)
    } else if head.starts_with(&SIG_RAR14) {
        Err(ContainerError::UnsupportedMethod(
            "RAR 1.4 的旧格式，本程序只认 RAR4 与 RAR5".to_string(),
        ))
    } else {
        Err(ContainerError::NotAContainer {
            kind: "rar",
            detail: format!("签名是 {:02X?}", &head[..head.len().min(8)]),
        })
    }
}

fn read_at(file: &mut dyn ReadSeek, offset: u64, len: usize) -> io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; len];
    let mut filled = 0usize;
    while filled < len {
        match file.read(&mut buf[filled..])? {
            0 => break,
            read => filled += read,
        }
    }
    buf.truncate(filled);
    Ok(buf)
}

/// 一条文件头交出来的东西。两种格式解析完都折成它，接下来的合并只写一份。
struct Part {
    path: String,
    name_lossy: bool,
    size: u64,
    crc32: Option<u32>,
    is_dir: bool,
    solid: bool,
    stored: bool,
    encrypted: bool,
    piece: Option<Piece>,
    split_before: bool,
    split_after: bool,
}

/// 把一段接进 `building`：续上一条，或者新开一条。
///
/// **分卷的合并只在这一处**，RAR4 与 RAR5 共用：两边的旗标名字不同，
/// 「非末段的校验和不能要」这条规矩却是同一条，抄成两份迟早只改一份。
fn absorb(building: &mut Vec<Building>, part: Part) {
    if part.split_before
        && let Some(open) = building
            .iter_mut()
            .rev()
            .find(|item| item.open && item.path == part.path)
    {
        // 上一卷那一段的续集：往上接数据，**CRC 一律以末段的为准**。
        open.open = part.split_after;
        if let Some(piece) = part.piece {
            open.pieces.push(piece);
        }
        if !part.split_after {
            open.crc32 = part.crc32;
        }
        open.size = open.size.max(part.size);
        return;
    }
    building.push(Building {
        path: part.path,
        name_lossy: part.name_lossy,
        size: part.size,
        // 还要续到下一卷的话，这里记的是**打包后数据**的校验和——不能要。
        crc32: if part.split_after { None } else { part.crc32 },
        is_dir: part.is_dir,
        solid: part.solid,
        stored: part.stored,
        encrypted: part.encrypted,
        pieces: part.piece.into_iter().collect(),
        open: part.split_after,
        // 只见到中段：这一条的头一段在别的卷上，我们是从半路开始读的。
        // 那时**字节是残的**，取内容一律拒绝（见 `read_entries`）。
        partial: part.split_before,
    });
}

// ── RAR5 ────────────────────────────────────────────────────────────────────

fn read_volume5(
    file: &mut dyn ReadSeek,
    len: u64,
    volume_index: usize,
    building: &mut Vec<Building>,
) -> Result<VolumeOutcome, ContainerError> {
    let mut at = SIG_RAR5.len() as u64;
    let mut next_volume = false;
    for _ in 0..MAX_BLOCKS {
        if at >= len {
            break;
        }
        // 头部长度自己写在头里，所以先读一窗，不够再按它说的长度补读。
        let window = read_at(file, at, 4096.min((len - at) as usize))?;
        if window.len() < 7 {
            break;
        }
        let mut cursor = Cursor::new(&window);
        let _header_crc = cursor.u32().ok_or_else(|| malformed("RAR5 头部截断"))?;
        let size_at = cursor.at;
        let header_size = cursor.vint().ok_or_else(|| malformed("RAR5 头长截断"))?;
        let header_end = cursor.at as u64 + header_size;
        if header_size == 0 || header_end > MAX_HEADER as u64 {
            // 走到哪一条上出的事要说出来：真机上那一份 1.55 GiB 的
            // `[SJ9E41]舞力全开2…-DL.rar` 就卡在这里，而 `unrar` 自己也报
            // *Corrupt header is found*——**如实说读不下去，别把读到的半张清单
            // 当成全部交出去**。
            return Err(malformed(format!(
                "RAR5 头部声称 {header_size} 字节（偏移 {at}，已经读到 {} 条）",
                building.len()
            )));
        }
        let window = if (header_end as usize) > window.len() {
            read_at(file, at, (header_end as usize).min(MAX_HEADER))?
        } else {
            window
        };
        if (window.len() as u64) < header_end {
            return Err(malformed("RAR5 头部跑出了文件末尾"));
        }
        let mut cursor = Cursor::new(&window);
        cursor.skip_to(size_at);
        let _ = cursor.vint();
        let header_type = cursor
            .vint()
            .ok_or_else(|| malformed("RAR5 头部类型截断"))?;
        let header_flags = cursor
            .vint()
            .ok_or_else(|| malformed("RAR5 头部旗标截断"))?;
        let extra_size = if header_flags & HFL_EXTRA != 0 {
            cursor
                .vint()
                .ok_or_else(|| malformed("RAR5 extra 长截断"))?
        } else {
            0
        };
        let data_size = if header_flags & HFL_DATA != 0 {
            cursor.vint().ok_or_else(|| malformed("RAR5 数据长截断"))?
        } else {
            0
        };
        if extra_size > header_size {
            return Err(malformed("RAR5 的 extra area 比整个头部还长"));
        }

        match header_type {
            HEAD5_CRYPT => {
                // 整个容器的**头**都加密了，连文件名都看不到。
                return Err(ContainerError::Encrypted);
            }
            HEAD5_MAIN => {
                let flags = cursor
                    .vint()
                    .ok_or_else(|| malformed("RAR5 容器旗标截断"))?;
                // 卷号只是为了跳过它——**「这是分卷」与「后面还有一卷」不是同一件事**：
                // 前者整组每一卷都写着，后者只有真的还有下一卷时才为真，而我们要的是后者
                // （见 `read_volume` 结尾）。容器级的 solid 旗标同理不看：真正管用的是
                // 每个文件头上那一位，unrar 自己判 solid 也是看它。
                if flags & MHFL_VOLNUMBER != 0 {
                    let _ = cursor.vint();
                }
            }
            HEAD5_FILE => {
                parse_file5(
                    &mut cursor,
                    &Head5 {
                        window: &window,
                        end: header_end as usize,
                        extra: extra_size as usize,
                        flags: header_flags,
                        data: data_size,
                    },
                    Piece {
                        volume: volume_index,
                        offset: at + header_end,
                        len: data_size,
                    },
                    building,
                )?;
            }
            HEAD5_ENDARC => {
                let flags = cursor.vint().unwrap_or(0);
                next_volume = flags & EHFL_NEXT_VOLUME != 0;
                break;
            }
            // 服务头（quick open 的 `QO`、注释、恢复记录）**不是内部文件**：
            // 真机上每一卷末尾都坐着一条名叫 `QO` 的服务头，把它当文件列出来的话，
            // 每个容器都会凭空多一个内部条目。
            HEAD5_SERVICE => {}
            // 将来才有的类型：**跳过，不报错**。规范给的规矩就是这一条（头部旗标
            // 0x0004 = 认不得的块照样跳过），而报错会让一个多带了一条新记录的容器
            // 整个读不出来。
            _ => {}
        }

        let next = at + header_end + data_size;
        if next <= at {
            return Err(malformed("RAR5 块长为零，读不下去"));
        }
        at = next;
    }
    Ok(VolumeOutcome {
        // **两条判据取或**：容器结束块说「后面还有一卷」，或者最后一条数据还续着。
        // 后一条是兜底——结束块不是每个容器都写，而一条 `SPLIT_AFTER` 摆在那里
        // 本身就是「下一卷在别处」的硬证据。
        next_volume: next_volume || building.last().is_some_and(|item| item.open),
        // RAR5 的分卷一律是新式命名。
        new_numbering: true,
    })
}

/// 一条已经定位好的 RAR5 头部：公共字段读完之后，文件头那一段还要知道的东西。
///
/// 这五样总是一起走，所以打成一个类型而不是排成一串参数——RAR5 的头**没有固定
/// 偏移只有固定顺序**，「头从哪儿到哪儿、extra 占尾巴上多少」正是读它时唯一的锚。
struct Head5<'b> {
    /// 整个头部的字节，从块首起。extra area 要按下标回它上面取。
    window: &'b [u8],
    /// 头部在 `window` 里的终点，也就是 extra area 的终点。
    end: usize,
    /// extra area 有多长。
    extra: usize,
    /// 头部通用旗标（`SPLIT_BEFORE` / `SPLIT_AFTER` 在里面）。
    flags: u64,
    /// 头部后面跟着的数据有多长。
    data: u64,
}

fn parse_file5(
    cursor: &mut Cursor<'_>,
    head: &Head5<'_>,
    piece: Piece,
    building: &mut Vec<Building>,
) -> Result<(), ContainerError> {
    let truncated = || malformed("RAR5 文件头截断");
    let file_flags = cursor.vint().ok_or_else(truncated)?;
    let unpacked = cursor.vint().ok_or_else(truncated)?;
    let _attributes = cursor.vint().ok_or_else(truncated)?;
    if file_flags & FHFL_UTIME != 0 {
        cursor.u32().ok_or_else(truncated)?;
    }
    // **CRC-32 只在这一位置位时才存在**（technote 的 File hash record 一节）。
    let crc32 = if file_flags & FHFL_CRC32 != 0 {
        Some(cursor.u32().ok_or_else(truncated)?)
    } else {
        None
    };
    let compression = cursor.vint().ok_or_else(truncated)?;
    let _host_os = cursor.vint().ok_or_else(truncated)?;
    let name_len = cursor.vint().ok_or_else(truncated)?;
    let name_len = usize::try_from(name_len).map_err(|_| malformed("RAR5 名字长装不进 usize"))?;
    let raw_name = cursor.take(name_len).ok_or_else(truncated)?;

    // extra area 在头部末尾。只看两条记录：加密与哈希。
    let mut encrypted = false;
    let mut tweaked_checksum = false;
    let extra_at = head.end.saturating_sub(head.extra);
    if head.extra > 0 && extra_at < head.window.len() {
        let mut extra = Cursor::new(&head.window[extra_at..head.end.min(head.window.len())]);
        while extra.at < extra.bytes.len() {
            let start = extra.at;
            let Some(size) = extra.vint() else { break };
            let body_end = extra.at + usize::try_from(size).unwrap_or(0);
            let Some(kind) = extra.vint() else { break };
            match kind {
                FHEXTRA_CRYPT => {
                    encrypted = true;
                    let _version = extra.vint();
                    let flags = extra.vint().unwrap_or(0);
                    tweaked_checksum = flags & FHEXTRA_CRYPT_TWEAKED != 0;
                }
                // BLAKE2sp 在这里。**DAT 一条 BLAKE2 都不记**，所以读出来也撞不上
                // ——真正要紧的是它意味着「这一条没有 CRC-32」，而那已经由
                // `crc32 == None` 说清楚了。
                FHEXTRA_HASH => {}
                _ => {}
            }
            if body_end <= start || body_end > extra.bytes.len() {
                break;
            }
            extra.skip_to(body_end);
        }
    }

    let is_dir = file_flags & FHFL_DIRECTORY != 0;
    // 大小未知（一边写一边压、事先不知道多大）时不要装作知道：那一栏记 0，识别层看到 0 字节
    // 就不会拿它去撞 DAT。
    let size = if file_flags & FHFL_UNPUNKNOWN != 0 {
        0
    } else {
        unpacked
    };
    let (path, name_lossy) = charset::decode_path(raw_name);
    let method = (compression & COMPINFO_METHOD) >> 7;
    // ⭐ **加密时校验和被密钥搅过**，与分卷那一坑同一类：字段还在，含义变了。
    // 真机上这不是边角情形——随机 60 份 rar 里有 **20 份**是带密码的（多是
    // 汉化组连着一段广告注释一起打的包），`unrar lt` 把它们的那一栏印成
    // `CRC32 MAC`。不丢掉它，这 20 份会拿着一串谁也对不上的数去撞 DAT，
    // 然后被记成「未命中」——**一声不响地把命中率压低**。
    let crc32 = if tweaked_checksum { None } else { crc32 };
    absorb(
        building,
        Part {
            path,
            name_lossy,
            size,
            crc32,
            is_dir,
            solid: compression & COMPINFO_SOLID != 0,
            stored: method == 0,
            encrypted,
            piece: (head.data > 0).then_some(piece),
            split_before: head.flags & HFL_SPLIT_BEFORE != 0,
            split_after: head.flags & HFL_SPLIT_AFTER != 0,
        },
    );
    Ok(())
}

// ── RAR4 ────────────────────────────────────────────────────────────────────

fn read_volume4(
    file: &mut dyn ReadSeek,
    len: u64,
    volume_index: usize,
    building: &mut Vec<Building>,
) -> Result<VolumeOutcome, ContainerError> {
    let mut at = SIG_RAR4.len() as u64;
    let mut next_volume = false;
    let mut new_numbering = false;
    for _ in 0..MAX_BLOCKS {
        if at + SIZEOF_SHORTBLOCKHEAD as u64 > len {
            break;
        }
        let window = read_at(file, at, 4096.min((len - at) as usize))?;
        let mut cursor = Cursor::new(&window);
        let _head_crc = cursor.u16().ok_or_else(|| malformed("RAR4 块头截断"))?;
        let head_type = cursor.u8().ok_or_else(|| malformed("RAR4 块头截断"))?;
        let flags = cursor.u16().ok_or_else(|| malformed("RAR4 块头截断"))?;
        let head_size = cursor.u16().ok_or_else(|| malformed("RAR4 块头截断"))?;
        if (head_size as usize) < SIZEOF_SHORTBLOCKHEAD {
            return Err(malformed(format!(
                "RAR4 头长写着 {head_size}，比公共头还短"
            )));
        }
        let window = if head_size as usize > window.len() {
            read_at(file, at, head_size as usize)?
        } else {
            window
        };

        let mut data_size = 0u64;
        match head_type {
            HEAD3_MAIN => {
                if flags & MHD_PASSWORD != 0 {
                    return Err(ContainerError::Encrypted);
                }
                new_numbering = flags & MHD_NEWNUMBERING != 0;
                if flags & LONG_BLOCK != 0 {
                    let mut extra = Cursor::new(&window);
                    extra.skip_to(SIZEOF_SHORTBLOCKHEAD);
                    data_size = u64::from(extra.u32().unwrap_or(0));
                }
            }
            HEAD3_FILE => {
                data_size = parse_file4(
                    &window,
                    flags,
                    at,
                    head_size as usize,
                    volume_index,
                    building,
                )?;
            }
            HEAD3_ENDARC => {
                next_volume = flags & EARC_NEXT_VOLUME != 0;
                break;
            }
            _ => {
                if flags & LONG_BLOCK != 0 {
                    let mut extra = Cursor::new(&window);
                    extra.skip_to(SIZEOF_SHORTBLOCKHEAD);
                    data_size = u64::from(extra.u32().unwrap_or(0));
                }
            }
        }

        let next = at + u64::from(head_size) + data_size;
        if next <= at {
            return Err(malformed("RAR4 块长为零，读不下去"));
        }
        at = next;
    }
    Ok(VolumeOutcome {
        next_volume: next_volume || building.last().is_some_and(|item| item.open),
        new_numbering,
    })
}

/// 解析一条 RAR4 文件头，返回它后面跟着的数据有多长。
fn parse_file4(
    window: &[u8],
    flags: u16,
    block_at: u64,
    head_size: usize,
    volume_index: usize,
    building: &mut Vec<Building>,
) -> Result<u64, ContainerError> {
    let truncated = || malformed("RAR4 文件头截断");
    let mut cursor = Cursor::new(window);
    cursor.skip_to(SIZEOF_SHORTBLOCKHEAD);
    let mut packed = u64::from(cursor.u32().ok_or_else(truncated)?);
    let mut unpacked = u64::from(cursor.u32().ok_or_else(truncated)?);
    let _host_os = cursor.u8().ok_or_else(truncated)?;
    // **RAR4 的 CRC-32 无条件存在**：unrar 读到这里直接
    // `hd->FileHash.Type=HASH_CRC32`，没有任何旗标判断（调研 1.3.1）。
    let crc32 = cursor.u32().ok_or_else(truncated)?;
    let _file_time = cursor.u32().ok_or_else(truncated)?;
    let _unp_ver = cursor.u8().ok_or_else(truncated)?;
    let method = cursor.u8().ok_or_else(truncated)?;
    let name_size = cursor.u16().ok_or_else(truncated)? as usize;
    let _attributes = cursor.u32().ok_or_else(truncated)?;
    debug_assert_eq!(cursor.at, SIZEOF_FILEHEAD3);
    if flags & LHD_LARGE != 0 {
        // 64 位大小：高 32 位分别跟在定长部分后面。
        packed |= u64::from(cursor.u32().ok_or_else(truncated)?) << 32;
        unpacked |= u64::from(cursor.u32().ok_or_else(truncated)?) << 32;
    }
    let raw_name = cursor.take(name_size).ok_or_else(truncated)?;
    if cursor.at > head_size {
        return Err(malformed("RAR4 的名字跑出了头部"));
    }

    let (path, name_lossy) = decode_name4(raw_name, flags);
    let is_dir = flags & LHD_WINDOWMASK == LHD_WINDOWMASK;
    let piece = Piece {
        volume: volume_index,
        offset: block_at + head_size as u64,
        len: packed,
    };
    absorb(
        building,
        Part {
            path,
            name_lossy,
            size: unpacked,
            crc32: Some(crc32),
            is_dir,
            solid: flags & LHD_SOLID != 0,
            // 方法是 ASCII 的 `'0'`..`'5'`，`'0'` 是原样存放。
            stored: method == b'0',
            encrypted: flags & LHD_PASSWORD != 0,
            piece: (packed > 0).then_some(piece),
            split_before: flags & LHD_SPLIT_BEFORE != 0,
            split_after: flags & LHD_SPLIT_AFTER != 0,
        },
    );
    Ok(packed)
}

/// RAR4 的文件名：可能是本地代码页的字节，也可能带着 unrar 私有的 Unicode 编码段。
fn decode_name4(raw: &[u8], flags: u16) -> (String, bool) {
    if flags & LHD_UNICODE != 0
        && let Some(zero) = raw.iter().position(|byte| *byte == 0)
        && zero + 1 < raw.len()
        && let Some(decoded) = decode_unicode_name(&raw[..zero], &raw[zero + 1..])
    {
        let lossy = decoded.contains('\u{FFFD}');
        return (charset::normalize_path(&decoded), lossy);
    }
    // 没有 Unicode 段：那就是本地代码页的字节，交给编码探测（真库里多半是 GBK）。
    charset::decode_path(raw)
}

/// unrar `encname.cpp` 的 `DecodeFileName`。
///
/// 这是 RAR4 存中文名字的办法，**没有别的路**：非 Unicode 那一半是本地代码页的
/// 字节（真库里是 GBK），Unicode 那一半是它的一套私有编码——两位一个操作码，
/// 三种取字节的办法加一种「照着非 Unicode 那一半抄，可带一个统一的偏移」。
///
/// 真机验过：随机 30 份带中文名的 RAR4，逐条与 `unrar lb` 的输出对比，一字不差。
/// 少了它，`17.FC天使之翼2修复巴西的荣耀Ver2.1回段BUG.tsh` 只会解成一串 GBK 乱码
/// ——而名字那一层（票 11）要拿它去撞中文数据源。
fn decode_unicode_name(name: &[u8], encoded: &[u8]) -> Option<String> {
    /// 名字最长多少个字符。unrar 的 `NM` 是 1024。
    const MAX_CHARS: usize = 1024;
    let mut units: Vec<u16> = Vec::new();
    let mut cursor = Cursor::new(encoded);
    let high = u16::from(cursor.u8()?);
    let mut flag_bits = 0u32;
    let mut flags = 0u8;
    while cursor.at < encoded.len() && units.len() < MAX_CHARS {
        if flag_bits == 0 {
            flags = cursor.u8()?;
            flag_bits = 8;
        }
        flag_bits -= 2;
        match (flags >> flag_bits) & 3 {
            // 一个低字节，高字节按 0。
            0 => units.push(u16::from(cursor.u8()?)),
            // 一个低字节，配头一个字节给出的高字节。
            1 => units.push(u16::from(cursor.u8()?) | (high << 8)),
            // 整整两个字节，小端。
            2 => {
                let low = u16::from(cursor.u8()?);
                let high_byte = u16::from(cursor.u8()?);
                units.push(low | (high_byte << 8));
            }
            // 照着非 Unicode 那一半抄一段，可带一个统一的偏移。
            _ => {
                let length = cursor.u8()?;
                let (count, correction) = if length & 0x80 != 0 {
                    (usize::from(length & 0x7F) + 2, Some(cursor.u8()?))
                } else {
                    (usize::from(length) + 2, None)
                };
                for _ in 0..count {
                    if units.len() >= MAX_CHARS {
                        break;
                    }
                    let base = u16::from(name.get(units.len()).copied().unwrap_or(0));
                    units.push(match correction {
                        Some(correction) => ((base + u16::from(correction)) & 0xFF) | (high << 8),
                        None => base,
                    });
                }
            }
        }
    }
    if units.is_empty() {
        return None;
    }
    Some(String::from_utf16_lossy(&units))
}

// ── 取内容 ──────────────────────────────────────────────────────────────────

/// 按计划读一遍。**只交得出「原样存放」的条目。**
///
/// 压缩过的条目要 UnRAR 的解压算法，而这个项目刻意没有引进它（见模块文档）。
/// 那不是失败，是这一层的边界：如实报「解不了这个压缩方法」，别装作读到了。
/// 能交的先交出去——识别那一侧对每一条各记各的账，一条读不了不该把已经读到的
/// 那些也一起废掉。
pub(super) fn read_entries(
    library: &dyn LibraryFs,
    contents: &Contents,
    locator: &RarLocator,
    plan: &ReadPlan,
    each: &mut dyn FnMut(&InnerEntry, &mut dyn Read) -> io::Result<()>,
) -> Result<ReadStats, ContainerError> {
    let mut stats = ReadStats::default();
    let mut skipped: Option<String> = None;
    for (index, entry) in contents.entries.iter().enumerate() {
        let demand = plan.demand(index);
        if !demand.wanted() {
            continue;
        }
        if !entry.has_content() {
            each(entry, &mut io::empty())?;
            stats.entries_read += 1;
            continue;
        }
        let Some(item) = locator.entries.get(index) else {
            return Err(malformed("条目没有对应的定位信息"));
        };
        if item.encrypted {
            return Err(ContainerError::Encrypted);
        }
        if item.partial {
            // 头一段在别的卷上。**残的字节绝不交出去**——那会算出一个看着像模像样
            // 的假哈希，而假哈希比读不到糟得多。
            return Err(ContainerError::MissingVolume {
                detail: format!("{} 的头一段在这一组的前一卷上", entry.path),
            });
        }
        if !item.stored {
            skipped.get_or_insert_with(|| entry.path.clone());
            continue;
        }
        let mut reader = Pieces::new(library, locator, &item.pieces);
        let counter = hand_entry(entry, &mut reader, demand, each)?;
        stats.bytes_decompressed = stats.bytes_decompressed.saturating_add(counter);
        stats.entries_read += 1;
        stats.blocks_decoded += 1;
    }
    if let Some(path) = skipped {
        return Err(ContainerError::UnsupportedMethod(format!(
            "rar 的压缩数据这一层不解（自行实现头部解析器以避开 UnRAR 许可，ADR-0014），\
             只读得了原样存放的条目；卡在 {path}"
        )));
    }
    Ok(stats)
}

/// 把一个条目散在几卷上的几段接起来读。
///
/// 跨卷的文件在磁盘上就是几段字节，中间隔着下一卷的签名与头部。**原样存放时接起来
/// 就是原文**——这也是这一层唯一读得了的情形。
struct Pieces<'a> {
    library: &'a dyn LibraryFs,
    locator: &'a RarLocator,
    pieces: &'a [Piece],
    at: usize,
    open: Option<(Box<dyn ReadSeek + 'a>, u64)>,
}

impl<'a> Pieces<'a> {
    fn new(library: &'a dyn LibraryFs, locator: &'a RarLocator, pieces: &'a [Piece]) -> Self {
        Self {
            library,
            locator,
            pieces,
            at: 0,
            open: None,
        }
    }
}

impl Read for Pieces<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            if let Some((file, left)) = &mut self.open {
                if *left == 0 {
                    self.open = None;
                    continue;
                }
                let want = buf.len().min(usize::try_from(*left).unwrap_or(usize::MAX));
                let read = file.read(&mut buf[..want])?;
                if read == 0 {
                    // 这一段比头里写的短：文件被截断了。**如实报错**，别把半截
                    // 数据当成读完了——那会算出一个看着像模像样的假哈希。
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "rar 的数据段比头里写的短",
                    ));
                }
                *left -= read as u64;
                return Ok(read);
            }
            let Some(piece) = self.pieces.get(self.at) else {
                return Ok(0);
            };
            self.at += 1;
            let Some(path) = self.locator.volumes.get(piece.volume) else {
                return Err(io::Error::new(io::ErrorKind::NotFound, "分卷不在名单里"));
            };
            let mut file = self.library.open(path)?;
            file.seek(SeekFrom::Start(piece.offset))?;
            self.open = Some((file, piece.len));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 变长整数按小端每字节七位读() {
        let mut cursor = Cursor::new(&[0x7F]);
        assert_eq!(cursor.vint(), Some(0x7F));
        let mut cursor = Cursor::new(&[0x80, 0x01]);
        assert_eq!(cursor.vint(), Some(128));
        let mut cursor = Cursor::new(&[0xE0, 0xC0, 0x02]);
        assert_eq!(cursor.vint(), Some(0x60 | (0x40 << 7) | (2 << 14)));
        // 没有结尾字节：宁可说读不下去，也不能悄悄截断
        let mut cursor = Cursor::new(&[0x80, 0x80, 0x80]);
        assert_eq!(cursor.vint(), None);
    }

    #[test]
    fn 私有_unicode_名字解得出中文() {
        // 真库里那条 `17.FC天使之翼2…`：非 Unicode 那一半是 GBK 的
        // `使之翼2…`，Unicode 那一半照着它抄再补高字节。
        // 这里造一条最小的：`A` 加一个汉字。
        let name = b"A";
        // HighByte=0x59；flags 的头两位 = 00（取一个低字节 'A'），接着 01（低字节 + 高字节）
        let encoded = [0x59u8, 0b0001_0000, b'A', 0x29];
        let decoded = decode_unicode_name(name, &encoded).expect("解得出");
        assert_eq!(decoded, "A\u{5929}", "0x59 << 8 | 0x29 = 天");
    }
}
