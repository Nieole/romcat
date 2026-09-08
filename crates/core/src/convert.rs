//! **格式转换**：按[能力档案](crate::capability)的配方，从主库读一份、往子库写一份新的。
//!
//! ## 一条不可动摇的边界：只产生新文件
//!
//! 读主库走 [`LibraryFs`]，那个 trait **根本没有写的办法**（ADR-0004）。这里没有任何
//! 一行能改到主库——不是「记得别改」，是**拿不到改的办法**。ADR-0015 把转换的位置从
//! 「整理主库」挪到了「导出管线」，正是为了让这条边界成为构造上的事实：同一个游戏导到
//! 不同掌机可以是不同格式，而主库始终保持原始形态。
//!
//! ## 边转边流式写入目标，默认不落第三份
//!
//! ADR-0017：**转换产物默认不缓存**。转换很贵（512 GB 的子库可能几小时），而缓存要再占
//! 一份等同空间——本机剩 24 GiB 的时候那不是优化是灾难。于是这里的出口就是目标上那个
//! `.romcat-part` 临时文件，转完 `sync_all` 再改名到位（`sync::execute`）。多设备共用
//! 同一种格式时另开一个可选的**转换缓存目录**，那是 [`sync::execute::Sources`](crate::sync::execute::Sources) 上的一个
//! `Option`，不给就没有。
//!
//! ## 这一版做得到的只有透明容器这一层
//!
//! [`Recipe::Unpack`] 把容器里那一份原样取出来，[`Recipe::Rezip`] 重打包成 zip。
//! **压缩镜像**之间的互转（cue+bin→chd、wbfs→rvz、zso→cso）要 chdman / DolphinTool /
//! maxcso，工具做不到——做不到的由 [`capability::decide`](crate::capability::decide)
//! 判成「吃不下且转不了」，在**差量预览**里点名说出口，而不是悄悄搬过去让用户在掌机上
//! 才发现打不开。
//!
//! ## 按块调度，一个块最多解一次
//!
//! 两条配方都只读**一趟**容器：`Unpack` 走 [`ReadPlan::only`]（碰一个块），
//! `Rezip` 走 [`ReadPlan::all`]（每个块最多解一次）。7z 的 solid 块上「一条一趟」
//! 与「一块一趟」相差约 100 倍（调研 1.6.2），而 [`container::read_entries`] 已经
//! 把调度做在里面了——这里不必也不该自己再排一遍。

use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::capability::{Conversion, Recipe, ZIP_LIMIT_SAYS};
use crate::container::{self, ContainerError, InnerEntry, ReadPlan};
use crate::fs::LibraryFs;
use crate::scan::CancelToken;

/// 一次读写的块大小。与执行那一侧、与**媒体池**收字节时是同一个数。
const CHUNK: usize = 64 * 1024;

/// 转换没做成。
#[derive(Debug, thiserror::Error)]
pub enum ConvertError {
    /// 容器读不动或者穿不透。
    #[error("源读不动：{0}")]
    Container(#[from] ContainerError),
    /// 写产物时出错。
    #[error("产物写不出来：{0}")]
    Io(#[from] io::Error),
    /// 容器里没有配方指着的那一条。
    #[error("容器里没有第 {0} 条内部条目——中立库里记的内部构成与盘上这一份对不上了，重扫一次")]
    NoSuchEntry(usize),
    /// 被中断。
    #[error("转换被中断")]
    Interrupted,
}

impl ConvertError {
    /// 是被中断吗。执行那一侧靠它把「中断」与「这一个失败了」分开。
    #[must_use]
    pub fn is_interrupted(&self) -> bool {
        match self {
            Self::Interrupted => true,
            Self::Io(error) => error.kind() == io::ErrorKind::Interrupted,
            _ => false,
        }
    }
}

/// 按配方把 `source` 转成一份新文件写到 `into`，返回产物有多大。
///
/// `source` 是主库里那份文件的**真实路径**（已经过 [`fs::real_path`](crate::fs::real_path)），
/// `into` 是目标上那个临时文件。**主库一个字节不改**：`library` 是只读接缝。
///
/// # Errors
/// 容器穿不透、产物写不出来、内部条目对不上、或者被中断时返回错误。
pub fn run(
    library: &dyn LibraryFs,
    source: &Path,
    conversion: &Conversion,
    into: &Path,
    cancel: &CancelToken,
) -> Result<u64, ConvertError> {
    let listing = container::list(library, source)?;
    let mut sink = std::fs::File::create(into)?;
    let written = match conversion.recipe {
        Recipe::Unpack => {
            let index = conversion.inner.ok_or(ConvertError::NoSuchEntry(0))?;
            if index >= listing.contents.entries.len() {
                return Err(ConvertError::NoSuchEntry(index));
            }
            let plan = ReadPlan::only(&listing, index);
            let mut written = 0_u64;
            container::read_entries(library, source, &listing, &plan, &mut |_, reader| {
                written = pipe(reader, &mut sink, cancel)?;
                Ok(())
            })?;
            written
        }
        Recipe::Rezip => {
            let plan = ReadPlan::all(&listing);
            let mut zip = ZipBuilder::new(&mut sink);
            container::read_entries(library, source, &listing, &plan, &mut |entry, reader| {
                zip.add(entry, reader, cancel)
            })?;
            zip.finish()?
        }
    };
    sink.sync_all()?;
    if cancel.is_cancelled() {
        return Err(ConvertError::Interrupted);
    }
    Ok(written)
}

/// 一块一块地搬，**每块之间看一眼有没有被中断**。
///
/// 不逐块看的话，一份几百 MiB 的镜像会让 Ctrl-C 等上几十秒——而在可移动介质上，
/// 「停不下来」会让人去拔卡。
fn pipe(reader: &mut dyn Read, sink: &mut dyn Write, cancel: &CancelToken) -> io::Result<u64> {
    let mut buf = vec![0_u8; CHUNK];
    let mut written = 0_u64;
    loop {
        if cancel.is_cancelled() {
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        let got = reader.read(&mut buf)?;
        if got == 0 {
            return Ok(written);
        }
        sink.write_all(&buf[..got])?;
        written += got as u64;
    }
}

// ── 一个够用的 ZIP 写入器 ──────────────────────────────────────────────────
//
// 只写这一版需要的那一种 zip：deflate、无加密、无 ZIP64、无数据描述符。
// 引一个 zip 写库要多拉一串依赖，而这里需要的字段不到二十个，且**格式细节必须由我们
// 自己拿捏**——转出来的 zip 还要能被本仓库的零解压层读回去（中央目录存 CRC-32，
// ADR-0014），那正是 `container::zip` 已经逐字节校对过的那份布局。

/// DOS 时间戳：固定 1980-01-01 00:00:00。
///
/// **故意不写源文件的时间。** 一来主库那份的 mtime 与「这份内部条目什么时候造的」
/// 根本不是一回事；二来固定值让同一个源转两次得到**逐字节相同**的产物，于是
/// 「产物变了没有」这个问题永远由源那一侧的戳来回答，不会被时间戳搅浑。
const DOS_DATE: u16 = 0x0021;
const DOS_TIME: u16 = 0;

/// 一条写进中央目录的记录。
struct Central {
    name: Vec<u8>,
    crc32: u32,
    compressed: u32,
    uncompressed: u32,
    offset: u32,
}

/// 边收内部条目边写一份 zip。
struct ZipBuilder<'a> {
    sink: &'a mut std::fs::File,
    offset: u64,
    entries: Vec<Central>,
}

impl<'a> ZipBuilder<'a> {
    fn new(sink: &'a mut std::fs::File) -> Self {
        Self {
            sink,
            offset: 0,
            entries: Vec::new(),
        }
    }

    /// 收一条内部条目。
    ///
    /// 先写一份留白的 local header，流式压完再**回头填**上 CRC 与两个大小。
    /// 走这条而不是数据描述符（flag bit 3），是因为带数据描述符的 zip 在 local header
    /// 里 CRC 与大小全是 0——本仓库的零解压层为此专门写了「必须读中央目录」那一段
    /// （调研 1.1.3），没必要自己再造一个同样的坑给别的工具踩。
    fn add(
        &mut self,
        entry: &InnerEntry,
        reader: &mut dyn Read,
        cancel: &CancelToken,
    ) -> io::Result<()> {
        let name = entry.path.as_bytes().to_vec();
        let name_len = u16::try_from(name.len())
            .map_err(|_| io::Error::other(format!("内部条目名太长：{}", entry.path)))?;
        let header_at = self.offset;

        let mut head = Vec::with_capacity(30 + name.len());
        head.extend_from_slice(&0x0403_4b50_u32.to_le_bytes());
        head.extend_from_slice(&20_u16.to_le_bytes()); // 需要的版本
        head.extend_from_slice(&0x0800_u16.to_le_bytes()); // 名字是 UTF-8
        head.extend_from_slice(&8_u16.to_le_bytes()); // deflate
        head.extend_from_slice(&DOS_TIME.to_le_bytes());
        head.extend_from_slice(&DOS_DATE.to_le_bytes());
        head.extend_from_slice(&0_u32.to_le_bytes()); // CRC，回头填
        head.extend_from_slice(&0_u32.to_le_bytes()); // 压缩后，回头填
        head.extend_from_slice(&0_u32.to_le_bytes()); // 未压缩，回头填
        head.extend_from_slice(&name_len.to_le_bytes());
        head.extend_from_slice(&0_u16.to_le_bytes()); // 没有 extra
        head.extend_from_slice(&name);
        self.sink.write_all(&head)?;
        self.offset += head.len() as u64;

        let mut crc = flate2::Crc::new();
        let mut encoder = flate2::write::DeflateEncoder::new(
            CountingSink {
                inner: &mut *self.sink,
                written: 0,
            },
            flate2::Compression::default(),
        );
        let mut buf = vec![0_u8; CHUNK];
        let mut uncompressed = 0_u64;
        loop {
            if cancel.is_cancelled() {
                return Err(io::Error::from(io::ErrorKind::Interrupted));
            }
            let got = reader.read(&mut buf)?;
            if got == 0 {
                break;
            }
            crc.update(&buf[..got]);
            encoder.write_all(&buf[..got])?;
            uncompressed += got as u64;
        }
        let compressed = encoder.finish()?.written;
        self.offset += compressed;

        let crc32 = crc.sum();
        // 判在这里而不是判在 `capability::decide`：那一层判的是**中立库里记着的**
        // 未压缩大小，而这里量的是盘上真的解出来多少。两者对不上说明库该重扫了，
        // 而不是可以硬写一份越界的 zip。
        let (compressed, uncompressed) =
            match (u32::try_from(compressed), u32::try_from(uncompressed)) {
                (Ok(compressed), Ok(uncompressed)) => (compressed, uncompressed),
                _ => {
                    return Err(io::Error::other(format!(
                        "{} 越过了 ZIP 的 4 GiB 边界，而这一版不写 ZIP64",
                        entry.path
                    )));
                }
            };
        let offset = u32::try_from(header_at)
            .map_err(|_| io::Error::other("产物越过了 ZIP 的 4 GiB 边界，而这一版不写 ZIP64"))?;

        // 回头把 CRC 与两个大小填进 local header。
        let mut patch = Vec::with_capacity(12);
        patch.extend_from_slice(&crc32.to_le_bytes());
        patch.extend_from_slice(&compressed.to_le_bytes());
        patch.extend_from_slice(&uncompressed.to_le_bytes());
        self.sink.seek(SeekFrom::Start(header_at + 14))?;
        self.sink.write_all(&patch)?;
        self.sink.seek(SeekFrom::Start(self.offset))?;

        self.entries.push(Central {
            name,
            crc32,
            compressed,
            uncompressed,
            offset,
        });
        Ok(())
    }

    /// 写中央目录与 EOCD，返回整份产物多大。
    fn finish(self) -> io::Result<u64> {
        let start = self.offset;
        let mut directory = Vec::new();
        for entry in &self.entries {
            let name_len =
                u16::try_from(entry.name.len()).map_err(|_| io::Error::other("内部条目名太长"))?;
            directory.extend_from_slice(&0x0201_4b50_u32.to_le_bytes());
            directory.extend_from_slice(&20_u16.to_le_bytes()); // 谁造的
            directory.extend_from_slice(&20_u16.to_le_bytes()); // 需要的版本
            directory.extend_from_slice(&0x0800_u16.to_le_bytes());
            directory.extend_from_slice(&8_u16.to_le_bytes());
            directory.extend_from_slice(&DOS_TIME.to_le_bytes());
            directory.extend_from_slice(&DOS_DATE.to_le_bytes());
            directory.extend_from_slice(&entry.crc32.to_le_bytes());
            directory.extend_from_slice(&entry.compressed.to_le_bytes());
            directory.extend_from_slice(&entry.uncompressed.to_le_bytes());
            directory.extend_from_slice(&name_len.to_le_bytes());
            directory.extend_from_slice(&0_u16.to_le_bytes()); // extra
            directory.extend_from_slice(&0_u16.to_le_bytes()); // comment
            directory.extend_from_slice(&0_u16.to_le_bytes()); // 起始盘号
            directory.extend_from_slice(&0_u16.to_le_bytes()); // 内部属性
            directory.extend_from_slice(&0_u32.to_le_bytes()); // 外部属性
            directory.extend_from_slice(&entry.offset.to_le_bytes());
            directory.extend_from_slice(&entry.name);
        }
        let count = u16::try_from(self.entries.len())
            .map_err(|_| io::Error::other("条目数越过了 ZIP 中央目录的 65535 条边界"))?;
        let size = u32::try_from(directory.len()).map_err(|_| over_zip_limit("中央目录"))?;
        let at = u32::try_from(start).map_err(|_| over_zip_limit("产物"))?;
        directory.extend_from_slice(&0x0605_4b50_u32.to_le_bytes());
        directory.extend_from_slice(&0_u16.to_le_bytes()); // 本盘号
        directory.extend_from_slice(&0_u16.to_le_bytes()); // 中央目录所在盘号
        directory.extend_from_slice(&count.to_le_bytes());
        directory.extend_from_slice(&count.to_le_bytes());
        directory.extend_from_slice(&size.to_le_bytes());
        directory.extend_from_slice(&at.to_le_bytes());
        directory.extend_from_slice(&0_u16.to_le_bytes()); // 没有注释
        self.sink.write_all(&directory)?;
        Ok(start + directory.len() as u64)
    }
}

/// 越过 ZIP64 边界那句话，与 [`capability::decide`](crate::capability::decide) 排计划时
/// 说的**是同一句**——那一层拿中立库里记着的未压缩大小判，这一层拿盘上真解出来多少判，
/// 两个时刻两个数，但用户看到的话必须一致。
fn over_zip_limit(what: &str) -> io::Error {
    io::Error::other(format!("{what}{ZIP_LIMIT_SAYS}"))
}

/// 数着字节走的写入器：压缩后到底出了多少由它数。
struct CountingSink<'a> {
    inner: &'a mut std::fs::File,
    written: u64,
}

impl Write for CountingSink<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let wrote = self.inner.write(buf)?;
        self.written += wrote as u64;
        Ok(wrote)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
