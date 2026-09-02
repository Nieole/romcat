//! zip 的零解压层：**只读中央目录**。
//!
//! 出处是 PKWARE APPNOTE 6.3.10。中央目录条目（§4.3.12，定长 46 字节）里
//! `crc-32`（`+0x10`）、`uncompressed size`（`+0x18`）与 `file name` 三样齐全，
//! 于是打开一个 zip 只要**两次 seek**：从尾部找到 EOCD，再按它给的偏移把中央目录
//! 整块读进来。34,808 个 zip 因此几乎不花钱。
//!
//! ## 为什么绝不读 local header 的 CRC 与大小
//!
//! APPNOTE §4.4.4 通用标志位第 3 位原文：置位时 *the fields crc-32, compressed size
//! and uncompressed size are set to zero in the local header*，真值在数据之后的
//! data descriptor 里。流式写出来的 zip（下载得到的那些）大多如此。**读 local header
//! 的这三个字段会静默地拿到三个零**——不是报错，是错得没声音。因此本模块只在取内容
//! 时才碰 local header，且只取那里的 `file name length` 与 `extra field length`
//! （数据起点要靠它们算），三个大小与 CRC 一律以中央目录为准。
//!
//! ## ZIP64
//!
//! APPNOTE §4.5.3：放不下 32 位的大小与偏移被置成 `0xFFFFFFFF`（磁盘号是 `0xFFFF`），
//! 真值在 extra field 的 `0x0001` 记录里。规范原话是这些字段 *MUST only appear if the
//! corresponding … record field is set to 0xFFFF or 0xFFFFFFFF* —— **只有哨兵字段才
//! 出现**，顺序固定为 未压缩大小、压缩后大小、local header 偏移、起始磁盘号。
//! 按「固定 28 字节」去读会全盘错位。库里 PS3 / Wii U 的镜像超过 4 GB，这不是可选项。

use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use flate2::read::DeflateDecoder;

use super::{
    ContainerError, ContainerKind, Contents, InnerEntry, Listing, Locator, ReadPlan, ReadStats,
    hand_entry,
};
use crate::fs::{LibraryFs, ReadSeek};
use crate::path::nfc;

const SIG_LOCAL: [u8; 4] = [b'P', b'K', 3, 4];
const SIG_CENTRAL: [u8; 4] = [b'P', b'K', 1, 2];
const SIG_EOCD: [u8; 4] = [b'P', b'K', 5, 6];
const SIG_ZIP64_EOCD: [u8; 4] = [b'P', b'K', 6, 6];
const SIG_ZIP64_LOCATOR: [u8; 4] = [b'P', b'K', 6, 7];

/// EOCD 定长部分。
const EOCD_LEN: usize = 22;
/// ZIP64 EOCD locator 定长。
const ZIP64_LOCATOR_LEN: usize = 20;
/// ZIP64 EOCD 记录里本模块要读到的最远字段（`+0x30` 起 8 字节）。
const ZIP64_EOCD_LEN: usize = 56;
/// 中央目录条目定长部分（APPNOTE §4.3.12）。
const CENTRAL_LEN: usize = 46;
/// local header 定长部分（APPNOTE §4.3.7）。
const LOCAL_LEN: usize = 30;

/// 从尾部往回找 EOCD 最多看多少字节：定长 22 + 注释上限 65,535 + locator 20。
const EOCD_SEARCH: u64 = (EOCD_LEN + 0xFFFF + ZIP64_LOCATOR_LEN) as u64;

/// 中央目录最大读多少。真库里够用；再大就当结构坏了，不然一个坏字段能让扫描器
/// 一口气申请几个 GB。
const MAX_CENTRAL_DIRECTORY: u64 = 512 << 20;

/// 32 位字段的哨兵值。
const U32_SENTINEL: u32 = 0xFFFF_FFFF;
/// 16 位字段的哨兵值。
const U16_SENTINEL: u16 = 0xFFFF;

/// 通用标志位第 0 位：条目加密。
const FLAG_ENCRYPTED: u16 = 1 << 0;

/// 压缩方法：原样存放。
const METHOD_STORED: u16 = 0;
/// 压缩方法：deflate。
const METHOD_DEFLATE: u16 = 8;

/// 回头取内容时要用的定位信息。**不含 CRC 与未压缩大小**——那两样只认中央目录。
#[derive(Debug, Clone, Copy)]
pub(super) struct EntryLocator {
    local_offset: u64,
    compressed_size: u64,
    method: u16,
    encrypted: bool,
}

fn le16(buf: &[u8], at: usize) -> Option<u16> {
    buf.get(at..at + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn le32(buf: &[u8], at: usize) -> Option<u32> {
    buf.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn le64(buf: &[u8], at: usize) -> Option<u64> {
    buf.get(at..at + 8)
        .map(|b| u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
}

fn malformed(detail: impl Into<String>) -> ContainerError {
    ContainerError::Malformed(detail.into())
}

fn read_exact_at(file: &mut dyn ReadSeek, offset: u64, len: usize) -> io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; len];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

/// 中央目录在哪儿、有多长、记了几条。
struct Directory {
    /// 在文件里的**实际**起点。
    start: u64,
    /// 长度。
    size: u64,
    /// 条目数，只用来在一条都读不出来时说清楚哪儿不对。
    entries: u64,
    /// 自解压模块造成的整体位移：条目里记的 local header 偏移要加上它。
    shift: u64,
}

/// 零解压读出一个 zip 的内部构成。
pub(super) fn list(library: &dyn LibraryFs, path: &Path) -> Result<Listing, ContainerError> {
    let mut file = library.open(path)?;
    let len = file.seek(SeekFrom::End(0))?;
    let directory = locate_directory(file.as_mut(), len)?;

    if directory.size > MAX_CENTRAL_DIRECTORY {
        return Err(malformed(format!(
            "中央目录声称有 {} 字节，超过上限",
            directory.size
        )));
    }
    let size =
        usize::try_from(directory.size).map_err(|_| malformed("中央目录长度装不进本机的 usize"))?;
    let raw = read_exact_at(file.as_mut(), directory.start, size)?;

    let mut entries = Vec::new();
    let mut locators = Vec::new();
    let mut cursor = 0usize;
    // 块号只发给**真有数据的条目**：目录条目与零字节的空文件不占块。拿条目序号当块号
    // 会让「这个容器有几个块」把它们也数进去。
    let mut blocks = 0usize;
    while cursor + CENTRAL_LEN <= raw.len() {
        if raw[cursor..cursor + 4] != SIG_CENTRAL {
            // 中央目录之后紧跟着 ZIP64 EOCD 或 EOCD，扫到别的签名就是走完了。
            break;
        }
        let (entry, locator, next) = parse_central(&raw, cursor, &mut blocks, directory.shift)?;
        entries.push(entry);
        locators.push(locator);
        cursor = next;
    }

    if entries.is_empty() && directory.entries > 0 {
        return Err(malformed(format!(
            "EOCD 说有 {} 条，中央目录里一条也读不出来",
            directory.entries
        )));
    }

    Ok(Listing {
        kind: ContainerKind::Zip,
        contents: Contents { entries, blocks },
        locator: Locator::Zip(locators),
    })
}

/// 找到中央目录：在尾部窗口里从后往前找 EOCD，命中哨兵就再走一趟 ZIP64。
///
/// **一个签名不算数，要整条链都对得上才算。** 光认 `PK\x05\x06` 会被压缩数据里
/// 碰巧出现的四个字节骗到；而只认「EOCD 正好收在文件末尾」又会漏掉真实存在的
/// 一类文件——主库 GB 目录里那批 2002 年的 zip，EOCD 之后还挂着 3 个填充字节，
/// 系统 `unzip` 读得出来。于是判据是：候选往后放得下它自己声明的注释，且顺着它
/// 算出来的中央目录起点上真的坐着一条 `PK\x01\x02`。对不上就继续往前找。
fn locate_directory(file: &mut dyn ReadSeek, len: u64) -> Result<Directory, ContainerError> {
    let window_len = EOCD_SEARCH.min(len);
    let window_start = len - window_len;
    let window = read_exact_at(
        file,
        window_start,
        usize::try_from(window_len).map_err(|_| malformed("文件尾部窗口装不进 usize"))?,
    )?;

    let mut last_error: Option<ContainerError> = None;
    for eocd_at in eocd_candidates(&window) {
        match read_directory_at(file, &window, window_start, eocd_at, len) {
            Ok(directory) => return Ok(directory),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or(ContainerError::NotAContainer {
        kind: "zip",
        detail: "尾部找不到 EOCD（PK\\x05\\x06）".to_string(),
    }))
}

/// 从尾部往回扫，逐个交出可能的 EOCD 位置。
///
/// 唯一的预筛是「注释放得下」：EOCD 之后剩的字节不能少于它自己声明的注释长度。
/// 剩得多是允许的——那就是尾部还挂着别的东西。
fn eocd_candidates(window: &[u8]) -> impl Iterator<Item = usize> + '_ {
    (0..window.len().saturating_sub(EOCD_LEN - 1))
        .rev()
        .filter(|at| {
            window[*at..*at + 4] == SIG_EOCD
                && at + EOCD_LEN + le16(window, at + 20).unwrap_or(0) as usize <= window.len()
        })
}

fn read_directory_at(
    file: &mut dyn ReadSeek,
    window: &[u8],
    window_start: u64,
    eocd_at: usize,
    len: u64,
) -> Result<Directory, ContainerError> {
    let eocd = &window[eocd_at..];
    let eocd_pos = window_start + eocd_at as u64;

    let entries16 = le16(eocd, 10).ok_or_else(|| malformed("EOCD 截断"))?;
    let size32 = le32(eocd, 12).ok_or_else(|| malformed("EOCD 截断"))?;
    let offset32 = le32(eocd, 16).ok_or_else(|| malformed("EOCD 截断"))?;

    let needs_zip64 =
        entries16 == U16_SENTINEL || size32 == U32_SENTINEL || offset32 == U32_SENTINEL;
    let (entries, size, recorded, directory_end) = if needs_zip64 {
        let record = read_zip64_eocd(file, window, window_start, eocd_at)?;
        (record.entries, record.size, record.offset, record.position)
    } else {
        (
            u64::from(entries16),
            u64::from(size32),
            u64::from(offset32),
            eocd_pos,
        )
    };

    // 自解压（SFX）的 zip 前面挂着一个可执行模块，于是记下来的偏移全都少了一截。
    // 中央目录紧挨在 EOCD（ZIP64 时是 ZIP64 EOCD）之前，据此把位移算出来。
    let start = if directory_start_looks_right(file, recorded, size, len) {
        recorded
    } else {
        directory_end
            .checked_sub(size)
            .ok_or_else(|| malformed("中央目录声称的长度比它到文件尾的距离还长"))?
    };
    if start.saturating_add(size) > len {
        return Err(malformed("中央目录跑出了文件末尾"));
    }
    // 这条才是「这个 EOCD 是真的」的证据：算出来的起点上坐着一条中央目录记录。
    // 空容器没有记录可验，那时长度为零本身就是自洽的。
    if size > 0 && !directory_start_looks_right(file, start, size, len) {
        return Err(ContainerError::NotAContainer {
            kind: "zip",
            detail: "EOCD 指向的位置上不是中央目录".to_string(),
        });
    }
    Ok(Directory {
        start,
        size,
        entries,
        shift: start.saturating_sub(recorded),
    })
}

struct Zip64Eocd {
    entries: u64,
    size: u64,
    offset: u64,
    /// ZIP64 EOCD 记录在文件里的实际位置，也就是中央目录的终点。
    position: u64,
}

fn read_zip64_eocd(
    file: &mut dyn ReadSeek,
    window: &[u8],
    window_start: u64,
    eocd_at: usize,
) -> Result<Zip64Eocd, ContainerError> {
    // locator 紧挨在 EOCD 之前，定长 20 字节。
    let locator_at = eocd_at
        .checked_sub(ZIP64_LOCATOR_LEN)
        .ok_or_else(|| malformed("有 ZIP64 哨兵，却没有 ZIP64 EOCD locator"))?;
    if window[locator_at..locator_at + 4] != SIG_ZIP64_LOCATOR {
        return Err(malformed("有 ZIP64 哨兵，locator 的签名却对不上"));
    }
    let recorded = le64(window, locator_at + 8).ok_or_else(|| malformed("ZIP64 locator 截断"))?;

    // 先按 locator 记下来的偏移读；SFX 让它偏了的话，回到尾部窗口里现找。
    let (raw, position) = match read_exact_at(file, recorded, ZIP64_EOCD_LEN) {
        Ok(raw) if raw[..4] == SIG_ZIP64_EOCD => (raw, recorded),
        _ => {
            let at = window[..locator_at]
                .windows(4)
                .rposition(|w| w == SIG_ZIP64_EOCD)
                .ok_or_else(|| malformed("找不到 ZIP64 EOCD 记录"))?;
            let raw = window
                .get(at..at + ZIP64_EOCD_LEN)
                .ok_or_else(|| malformed("ZIP64 EOCD 记录截断"))?
                .to_vec();
            (raw, window_start + at as u64)
        }
    };

    Ok(Zip64Eocd {
        entries: le64(&raw, 32).ok_or_else(|| malformed("ZIP64 EOCD 截断"))?,
        size: le64(&raw, 40).ok_or_else(|| malformed("ZIP64 EOCD 截断"))?,
        offset: le64(&raw, 48).ok_or_else(|| malformed("ZIP64 EOCD 截断"))?,
        position,
    })
}

fn directory_start_looks_right(file: &mut dyn ReadSeek, offset: u64, size: u64, len: u64) -> bool {
    if size == 0 || offset.saturating_add(size) > len {
        return false;
    }
    read_exact_at(file, offset, 4).is_ok_and(|head| head == SIG_CENTRAL)
}

/// 解析一条中央目录记录，返回条目、定位信息与下一条的起点。
fn parse_central(
    raw: &[u8],
    at: usize,
    blocks: &mut usize,
    shift: u64,
) -> Result<(InnerEntry, EntryLocator, usize), ContainerError> {
    let truncated = || malformed("中央目录记录截断");
    let flags = le16(raw, at + 0x08).ok_or_else(truncated)?;
    let method = le16(raw, at + 0x0A).ok_or_else(truncated)?;
    let crc = le32(raw, at + 0x10).ok_or_else(truncated)?;
    let mut compressed = u64::from(le32(raw, at + 0x14).ok_or_else(truncated)?);
    let mut uncompressed = u64::from(le32(raw, at + 0x18).ok_or_else(truncated)?);
    let name_len = le16(raw, at + 0x1C).ok_or_else(truncated)? as usize;
    let extra_len = le16(raw, at + 0x1E).ok_or_else(truncated)? as usize;
    let comment_len = le16(raw, at + 0x20).ok_or_else(truncated)? as usize;
    let mut disk = le16(raw, at + 0x22).ok_or_else(truncated)?;
    let mut local_offset = u64::from(le32(raw, at + 0x2A).ok_or_else(truncated)?);

    let name_at = at + CENTRAL_LEN;
    let extra_at = name_at + name_len;
    let comment_at = extra_at + extra_len;
    let next = comment_at + comment_len;
    if next > raw.len() {
        return Err(truncated());
    }
    let name_raw = &raw[name_at..extra_at];
    let extra = &raw[extra_at..comment_at];

    // 哨兵才去 extra field 里取真值，顺序固定，缺席的字段不占位。
    let sentinels = Zip64Sentinels {
        uncompressed: uncompressed == u64::from(U32_SENTINEL),
        compressed: compressed == u64::from(U32_SENTINEL),
        local_offset: local_offset == u64::from(U32_SENTINEL),
        disk: disk == U16_SENTINEL,
    };
    if sentinels.any() {
        apply_zip64(
            extra,
            &sentinels,
            &mut uncompressed,
            &mut compressed,
            &mut local_offset,
            &mut disk,
        )?;
    }

    let (path, name_lossy) = decode_name(name_raw);
    let is_dir = path.ends_with('/');

    // zip 每个条目独立压缩，自成一块：可以任意顺序、任意并行地读。
    let block = (!is_dir && uncompressed > 0).then(|| {
        let block = *blocks;
        *blocks += 1;
        block
    });
    let entry = InnerEntry {
        path,
        size: uncompressed,
        // 中央目录的 CRC 永远是真值——**这正是绕开通用标志位第 3 位的办法**。
        crc32: Some(crc),
        is_dir,
        block,
        name_lossy,
    };
    let locator = EntryLocator {
        local_offset: local_offset.saturating_add(shift),
        compressed_size: compressed,
        method,
        encrypted: flags & FLAG_ENCRYPTED != 0,
    };
    Ok((entry, locator, next))
}

struct Zip64Sentinels {
    uncompressed: bool,
    compressed: bool,
    local_offset: bool,
    disk: bool,
}

impl Zip64Sentinels {
    fn any(&self) -> bool {
        self.uncompressed || self.compressed || self.local_offset || self.disk
    }
}

/// 走一遍 extra field，从 `0x0001` 记录里补回被哨兵占掉的真值。
///
/// APPNOTE §4.5.3 说这些字段 *MUST only appear if the corresponding … field is set to
/// 0xFFFF or 0xFFFFFFFF*，也就是**只有哨兵字段才占位**。但现实里确实有工具把前面几个
/// 字段无条件写满。两种写法用长度区分得开：记录里装得下的 8 字节槽位比哨兵的个数还多，
/// 就说明它是按固定顺序写满的——那时按位置取，只把哨兵那几个填回去。
fn apply_zip64(
    extra: &[u8],
    sentinels: &Zip64Sentinels,
    uncompressed: &mut u64,
    compressed: &mut u64,
    local_offset: &mut u64,
    disk: &mut u16,
) -> Result<(), ContainerError> {
    let mut at = 0usize;
    while at + 4 <= extra.len() {
        let tag = le16(extra, at).unwrap_or(0);
        let size = le16(extra, at + 2).unwrap_or(0) as usize;
        let body_at = at + 4;
        if body_at + size > extra.len() {
            break;
        }
        if tag == 0x0001 {
            let body = &extra[body_at..body_at + size];
            let wanted = [
                sentinels.uncompressed,
                sentinels.compressed,
                sentinels.local_offset,
            ];
            let slots = [uncompressed, compressed, local_offset];
            let written = (size / 8).min(slots.len());
            let positional = written > wanted.iter().filter(|it| **it).count();
            let mut cursor = 0usize;
            for (index, slot) in slots.into_iter().enumerate() {
                if !positional && !wanted[index] {
                    continue;
                }
                if positional && index >= written {
                    break;
                }
                let value = le64(body, cursor)
                    .ok_or_else(|| malformed("ZIP64 extra field 比哨兵要求的短"))?;
                cursor += 8;
                if wanted[index] {
                    *slot = value;
                }
            }
            if sentinels.disk {
                *disk = le32(body, cursor)
                    .ok_or_else(|| malformed("ZIP64 extra field 里没有磁盘号"))?
                    as u16;
            }
            return Ok(());
        }
        at = body_at + size;
    }
    Err(malformed("有 ZIP64 哨兵，却没有 0x0001 的 extra field"))
}

/// 名字按 UTF-8 认，不是 UTF-8 的**先探编码再解码**（[`charset`](super::charset)）。
///
/// 通用标志位第 11 位声明「名字是 UTF-8」，但直接验一遍字节比信那一位更稳：库里
/// 大量中文名是 GBK 且没置这一位。
///
/// **返回的那个布尔是「连编码都探不出来」，不是「不是 UTF-8」**（票 11 改的正是这个）：
/// 票 03 时名字不参与命中，有损转换无所谓；而文件名那一层要拿名字去撞中文数据源，
/// 一个 `U+FFFD` 就把整条名字废掉，而且不可逆。探得出编码的现在解得对，
/// 探不出来的才如实标成有损。
fn decode_name(raw: &[u8]) -> (String, bool) {
    let (text, charset) = super::charset::decode(raw);
    // 规范要求用 `/`，但确实有工具写 `\`。键的分隔符统一成 `/`（与 ADR-0020 同一条）。
    (
        nfc(&text.replace('\\', "/")).into_owned(),
        charset == super::charset::Charset::Lossy,
    )
}

/// 按计划读一遍：zip 每个条目独立压缩，块与条目一一对应，没有 solid 可言。
pub(super) fn read_entries(
    library: &dyn LibraryFs,
    path: &Path,
    contents: &Contents,
    locators: &[EntryLocator],
    plan: &ReadPlan,
    each: &mut dyn FnMut(&InnerEntry, &mut dyn Read) -> io::Result<()>,
) -> Result<ReadStats, ContainerError> {
    let mut stats = ReadStats::default();
    let mut file = library.open(path)?;
    for (index, entry) in contents.entries.iter().enumerate() {
        let demand = plan.demand(index);
        if !demand.wanted() {
            continue;
        }
        if !entry.has_content() {
            // 目录与零字节的空文件不必碰盘，直接给一个空流。
            each(entry, &mut io::empty())?;
            stats.entries_read += 1;
            continue;
        }
        let locator = locators
            .get(index)
            .ok_or_else(|| malformed("条目没有对应的定位信息"))?;
        if locator.encrypted {
            return Err(ContainerError::Encrypted);
        }
        let start = data_start(file.as_mut(), locator)?;
        file.seek(SeekFrom::Start(start))?;
        let packed = (&mut file).take(locator.compressed_size);
        let mut raw: Box<dyn Read> = match locator.method {
            METHOD_STORED => Box::new(packed),
            METHOD_DEFLATE => Box::new(DeflateDecoder::new(packed)),
            other => {
                return Err(ContainerError::UnsupportedMethod(format!(
                    "zip 压缩方法 {other}"
                )));
            }
        };
        let counter = hand_entry(entry, &mut raw, demand, each)?;
        stats.bytes_decompressed = stats.bytes_decompressed.saturating_add(counter);
        stats.entries_read += 1;
        stats.blocks_decoded += 1;
    }
    Ok(stats)
}

/// 数据起点 = local header 偏移 + 30 + 名字长 + extra 长。
///
/// **这里只取 local header 的两个长度字段。** CRC 与两个大小一律来自中央目录，
/// 通用标志位第 3 位置位时那三个字段就是零（见模块文档）。而两个长度字段本身
/// 与那一位无关，且 local 与 central 的 extra field 允许不一样长——必须现读。
fn data_start(file: &mut dyn ReadSeek, locator: &EntryLocator) -> Result<u64, ContainerError> {
    let head = read_exact_at(file, locator.local_offset, LOCAL_LEN)?;
    if head[..4] != SIG_LOCAL {
        return Err(malformed(format!(
            "local header 签名对不上（偏移 {}）",
            locator.local_offset
        )));
    }
    let name_len = u64::from(le16(&head, 26).ok_or_else(|| malformed("local header 截断"))?);
    let extra_len = u64::from(le16(&head, 28).ok_or_else(|| malformed("local header 截断"))?);
    Ok(locator.local_offset + LOCAL_LEN as u64 + name_len + extra_len)
}
