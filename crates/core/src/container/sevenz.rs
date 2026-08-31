//! 7z 的零解压层，以及 **solid block 的按块调度**。
//!
//! 出处是 7-Zip 官方 `DOC/7zFormat.txt` 与 LZMA SDK 的 `C/7z.h`。头部集中在文件末尾，
//! 可能自己也被压过（`kEncodedHeader`，7-Zip 的默认行为），解一次几十 KB 就能拿到：
//!
//! - `SubStreamsInfo.kCRC` → **每个内部文件的 CRC-32**
//! - `SubStreamsInfo.kSize` → 每个内部文件的未压缩大小
//! - `FilesInfo.kNames` → 名字
//! - `NumUnPackStreamsInFolders` / `FileToFolder` → **文件属于哪个块**
//!
//! ## solid block：这张票最要紧的一条
//!
//! 一个块（规范叫 folder）里的多个文件是连着压的。官方 LZMA SDK 的
//! `SzArEx_Extract` 签名里直接带着 `UInt32 *blockIndex /* index of solid block */`，
//! 注释还写着 *You can consider "\*outBuffer" as cache of solid block* —— **一次解一整块
//! 是唯一路径**，官方 API 自己就这么说。于是「按文件名逐个解压」在一个装了 200 个
//! 卡带的块上会解出约 40 GB 而不是 400 MB，**约 100 倍**。
//!
//! 好消息是块的归属零解压可得，因此这里的做法是：**先按块把要读的条目分好组，
//! 一个块只开一次解码器，顺着流一趟走完，中途把每个条目的字节交给调用方**。
//! 块里不想要的条目只能顺着排空——没有任何编解码器支持跳到中间（调研 1.5.2）。
//! 最后一个想要的条目读完就整块中止，后面的字节一个都不解。

use std::io::{self, Read};
use std::path::Path;

use sevenz_rust2::{Archive, ArchiveEntry, BlockDecoder, Password};

use super::{
    ContainerError, ContainerKind, Contents, Counting, InnerEntry, Listing, Locator, ReadPlan,
    ReadStats, drain,
};
use crate::fs::LibraryFs;
use crate::path::nfc;

/// 7z 的签名（`7zFormat.txt` 的 `kSignature`）。
const SIGNATURE: [u8; 6] = [b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C];

/// 交给块解码器的线程数。
///
/// **一个**。并行度要设在「块」这一层，不能设在块内部——同一个块被多个线程碰，
/// 它就会被重复解开，正是这张票要避免的那件事（调研 1.6.4）。
const DECODER_THREADS: u32 = 1;

fn map_error(error: &sevenz_rust2::Error) -> ContainerError {
    match error {
        sevenz_rust2::Error::BadSignature(found) => ContainerError::NotAContainer {
            kind: "7z",
            detail: format!("签名是 {found:02X?}"),
        },
        sevenz_rust2::Error::PasswordRequired | sevenz_rust2::Error::MaybeBadPassword(_) => {
            ContainerError::Encrypted
        }
        sevenz_rust2::Error::UnsupportedCompressionMethod(method) => {
            ContainerError::UnsupportedMethod(format!("7z 编解码器 {method}"))
        }
        sevenz_rust2::Error::ExternalUnsupported | sevenz_rust2::Error::Unsupported(_) => {
            ContainerError::UnsupportedMethod(error.to_string())
        }
        other => ContainerError::Malformed(other.to_string()),
    }
}

/// 零解压读出一个 7z 的内部构成。
pub(super) fn list(library: &dyn LibraryFs, path: &Path) -> Result<Listing, ContainerError> {
    // 先看六个字节：扩展名说是 7z、内容不是，这是「不是这个格式」而不是「结构坏了」，
    // 报告里的处置完全不同。
    let head = library.read_head(path, SIGNATURE.len())?;
    if head != SIGNATURE {
        return Err(ContainerError::NotAContainer {
            kind: "7z",
            detail: format!("签名是 {head:02X?}"),
        });
    }

    let mut file = library.open(path)?;
    let archive = Archive::read(&mut file, &Password::empty()).map_err(|e| map_error(&e))?;

    let entries = archive
        .files
        .iter()
        .enumerate()
        .map(|(index, entry)| InnerEntry {
            path: nfc(&entry.name.replace('\\', "/")).into_owned(),
            size: entry.size,
            // `kCRC` 在规范里是可选块，逐流还有 `Defined` 位——没记就如实说没有，
            // 那样的条目进不了零解压的第一命中层。
            crc32: entry.has_crc.then_some(entry.crc as u32),
            is_dir: entry.is_directory,
            // 只有真的占着一条子流的条目才谈得上「在哪个块里」：目录与零字节的
            // 空文件根本没有数据。
            block: (entry.has_stream && !entry.is_directory && entry.size > 0)
                .then(|| {
                    archive
                        .stream_map
                        .file_block_index
                        .get(index)
                        .copied()
                        .flatten()
                })
                .flatten(),
            name_lossy: false,
        })
        .collect();

    Ok(Listing {
        kind: ContainerKind::SevenZip,
        contents: Contents {
            entries,
            blocks: archive.blocks.len(),
        },
        locator: Locator::SevenZip(Box::new(archive)),
    })
}

/// 按块调度地读一遍：**一个块只开一次解码器**。
pub(super) fn read_entries(
    library: &dyn LibraryFs,
    path: &Path,
    contents: &Contents,
    archive: &Archive,
    plan: &ReadPlan,
    each: &mut dyn FnMut(&InnerEntry, &mut dyn Read) -> io::Result<()>,
) -> Result<ReadStats, ContainerError> {
    let mut stats = ReadStats::default();

    // 没有数据流的条目（目录、空文件）不必碰盘，也不该让一个块白解一趟。
    for (index, entry) in contents.entries.iter().enumerate() {
        if plan.demand(index).wanted() && !entry.has_content() {
            each(entry, &mut io::empty())?;
            stats.entries_read += 1;
        }
    }

    let blocks = plan.blocks(contents);
    if blocks.is_empty() {
        return Ok(stats);
    }

    let mut file = library.open(path)?;
    // 头部已经在 `list` 时解过一次，这里直接用；重新解一遍是纯浪费。
    let password = Password::empty();
    for block in blocks {
        stats.blocks_decoded += 1;
        let start = *archive
            .stream_map
            .block_first_file_index
            .get(block)
            .ok_or_else(|| ContainerError::Malformed(format!("块 {block} 没有起始条目")))?;
        // 这一块里还有几个想要的：减到零就整块中止，后面的字节一个都不解。
        let mut left = contents
            .entries
            .iter()
            .enumerate()
            .filter(|(index, entry)| entry.block == Some(block) && plan.demand(*index).wanted())
            .count();

        let decoder = BlockDecoder::new(DECODER_THREADS, block, archive, &password, &mut file);
        let mut seen = 0usize;
        let mut caller: io::Result<()> = Ok(());
        let mut bytes = 0u64;
        let mut read_here = 0u64;

        let outcome = decoder.for_each_entries(&mut |_entry: &ArchiveEntry, reader| {
            let index = start + seen;
            seen += 1;
            let Some(inner) = contents.entries.get(index) else {
                return Ok(false);
            };
            let demand = plan.demand(index);
            if demand.wanted() && inner.has_content() {
                // 借用限制在这个块里：出了块 `bytes` 与 `reader` 才好接着用。
                let handed = {
                    let mut counted = Counting {
                        inner: &mut *reader,
                        counter: &mut bytes,
                    };
                    // 只要前若干字节时截一截就够——读够就停，块里后面的字节不解。
                    let mut bounded = (&mut counted).take(demand.limit());
                    each(inner, &mut bounded)
                };
                if let Err(error) = handed {
                    caller = Err(error);
                    return Ok(false);
                }
                read_here += 1;
                left = left.saturating_sub(1);
            }
            if left == 0 {
                // 想要的都拿到了 —— 提前中止，剩下的块内字节不再解。
                return Ok(false);
            }
            // 还有想要的在后面：当前条目剩下的字节必须顺着排空，
            // 否则块内的流会错位（解压器跳不过去，调研 1.5.2）。
            match drain(reader) {
                Ok(skipped) => bytes = bytes.saturating_add(skipped),
                Err(error) => {
                    caller = Err(error);
                    return Ok(false);
                }
            }
            Ok(true)
        });

        stats.bytes_decompressed = stats.bytes_decompressed.saturating_add(bytes);
        stats.entries_read += read_here;
        caller?;
        outcome.map_err(|e| map_error(&e))?;
    }

    Ok(stats)
}
