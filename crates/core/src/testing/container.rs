//! 手写的 zip **透明容器**样本构造器。
//!
//! **故意不借第三方 zip 库。** 这张票要验的三件事里有两件是「别人写坏的 zip 长什么样」：
//! 通用标志位第 3 位置位时 local header 的 CRC 与大小全是零、ZIP64 的哨兵值与只放
//! 哨兵字段的 extra field。现成的库只会写出规规矩矩的 zip，造不出这两种样本，
//! 拿它造出来的样本去测也只是在测那个库。这里一个字节一个字节地摆，
//! 偏移与字段顺序照 PKWARE APPNOTE 6.3.10 §4.3.7 / §4.3.12 / §4.3.16 / §4.5.3。

use std::io::Write as _;

use flate2::Compression;
use flate2::write::DeflateEncoder;

/// 一个待写进 zip 的内部文件。
#[derive(Debug, Clone)]
pub struct ZipEntrySpec {
    name: String,
    data: Vec<u8>,
    deflate: bool,
    data_descriptor: bool,
    zip64_extra: bool,
    zip64_extra_full: bool,
}

impl ZipEntrySpec {
    /// 原样存放（压缩方法 0）。
    #[must_use]
    pub fn stored(name: &str, data: impl Into<Vec<u8>>) -> Self {
        Self {
            name: name.to_string(),
            data: data.into(),
            deflate: false,
            data_descriptor: false,
            zip64_extra: false,
            zip64_extra_full: false,
        }
    }

    /// deflate 压缩（压缩方法 8）。
    #[must_use]
    pub fn deflated(name: &str, data: impl Into<Vec<u8>>) -> Self {
        Self {
            deflate: true,
            ..Self::stored(name, data)
        }
    }

    /// 置**通用标志位第 3 位**：local header 里的 CRC 与两个大小写成零，真值只在
    /// 中央目录与数据之后的 data descriptor 里。
    ///
    /// 这是流式写出来的 zip 的常态，也是零解压层最容易静默出错的地方。
    #[must_use]
    pub fn with_data_descriptor(mut self) -> Self {
        self.data_descriptor = true;
        self
    }

    /// 中央目录里把大小与 local header 偏移写成**哨兵值**，真值放进
    /// `0x0001` 的 extra field。
    ///
    /// 真实的 ZIP64 是文件大到装不下 32 位才这样；测试里不必真造 4 GB，
    /// 哨兵与 extra field 的读法与文件多大无关。
    #[must_use]
    pub fn with_zip64_extra(mut self) -> Self {
        self.zip64_extra = true;
        self
    }

    /// 中央目录里**只有 local header 偏移**是哨兵，`0x0001` 的 extra field 却把三个
    /// 字段全写满了。
    ///
    /// 规范说只有哨兵字段才该出现，但现实里确实有工具无条件写满前几个字段。按
    /// 「哨兵顺序」去读这种记录，会把未压缩大小当成偏移，然后 seek 到一个错的地方。
    #[must_use]
    pub fn with_zip64_extra_written_in_full(mut self) -> Self {
        self.zip64_extra_full = true;
        self
    }
}

/// 算一段字节的 CRC-32（IEEE，与 DAT 里的 `crc` 同一个算法）。
#[must_use]
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = flate2::Crc::new();
    crc.update(data);
    crc.sum()
}

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).expect("写进内存不会失败");
    encoder.finish().expect("写进内存不会失败")
}

const U32_SENTINEL: u32 = 0xFFFF_FFFF;

/// 造一个 zip 容器。
#[must_use]
pub fn zip_container(entries: &[ZipEntrySpec]) -> Vec<u8> {
    build(entries, false, &[])
}

/// 造一个 zip 容器，并且用 **ZIP64 的 EOCD**：EOCD 里的条目数、中央目录大小与偏移
/// 全写成哨兵值，真值放进 ZIP64 EOCD 记录，末尾再加一条 locator。
#[must_use]
pub fn zip_container_with_zip64_eocd(entries: &[ZipEntrySpec]) -> Vec<u8> {
    build(entries, true, &[])
}

/// 造一个前面挂了 `prefix` 字节的自解压形态 zip：记下来的偏移全都少了一截。
#[must_use]
pub fn zip_container_with_prefix(entries: &[ZipEntrySpec], prefix: &[u8]) -> Vec<u8> {
    build(entries, false, prefix)
}

/// 造一个 EOCD 之后还挂着 `trailing` 字节的 zip。
///
/// 真库里就有：GB 目录下那批 2002 年的 zip，EOCD 后面跟着 3 个填充字节，系统
/// `unzip` 读得出来。按「EOCD 必须正好收在文件末尾」去找会把它们全判成不是 zip。
#[must_use]
pub fn zip_container_with_trailing(entries: &[ZipEntrySpec], trailing: &[u8]) -> Vec<u8> {
    let mut out = build(entries, false, &[]);
    out.extend_from_slice(trailing);
    out
}

struct Placed {
    spec: ZipEntrySpec,
    crc: u32,
    packed: Vec<u8>,
    local_offset: u64,
}

#[allow(clippy::too_many_lines)]
fn build(entries: &[ZipEntrySpec], zip64_eocd: bool, prefix: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = prefix.to_vec();
    let mut placed = Vec::new();

    for spec in entries {
        let crc = crc32(&spec.data);
        let packed = if spec.deflate {
            deflate(&spec.data)
        } else {
            spec.data.clone()
        };
        // 记下来的偏移是「相对 zip 自己的起点」，前缀那一截不算——这正是自解压
        // 容器里所有偏移都少一截的原因。
        let local_offset = (out.len() - prefix.len()) as u64;

        let flags: u16 = if spec.data_descriptor { 1 << 3 } else { 0 };
        let method: u16 = u16::from(spec.deflate) * 8;
        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // time
        out.extend_from_slice(&0u16.to_le_bytes()); // date
        // 第 3 位置位时这三个字段按规范必须是零。零解压层要是读了它们就会拿到三个零。
        let (local_crc, local_packed, local_plain) = if spec.data_descriptor {
            (0u32, 0u32, 0u32)
        } else {
            (crc, packed.len() as u32, spec.data.len() as u32)
        };
        out.extend_from_slice(&local_crc.to_le_bytes());
        out.extend_from_slice(&local_packed.to_le_bytes());
        out.extend_from_slice(&local_plain.to_le_bytes());
        out.extend_from_slice(&(spec.name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra len
        out.extend_from_slice(spec.name.as_bytes());
        out.extend_from_slice(&packed);
        if spec.data_descriptor {
            out.extend_from_slice(b"PK\x07\x08");
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            out.extend_from_slice(&(spec.data.len() as u32).to_le_bytes());
        }

        placed.push(Placed {
            spec: spec.clone(),
            crc,
            packed,
            local_offset,
        });
    }

    let directory_start = (out.len() - prefix.len()) as u64;
    for item in &placed {
        let flags: u16 = if item.spec.data_descriptor { 1 << 3 } else { 0 };
        let method: u16 = u16::from(item.spec.deflate) * 8;
        let plain = item.spec.data.len() as u64;
        let compressed = item.packed.len() as u64;

        let mut extra = Vec::new();
        if item.spec.zip64_extra_full {
            let mut body = Vec::new();
            body.extend_from_slice(&plain.to_le_bytes());
            body.extend_from_slice(&compressed.to_le_bytes());
            body.extend_from_slice(&item.local_offset.to_le_bytes());
            extra.extend_from_slice(&0x0001u16.to_le_bytes());
            extra.extend_from_slice(&(body.len() as u16).to_le_bytes());
            extra.extend_from_slice(&body);
        }
        let (plain_field, compressed_field, offset_field) = if item.spec.zip64_extra_full {
            (plain as u32, compressed as u32, U32_SENTINEL)
        } else if item.spec.zip64_extra {
            // 哨兵字段才进 extra field，顺序固定：未压缩、压缩后、local header 偏移。
            let mut body = Vec::new();
            body.extend_from_slice(&plain.to_le_bytes());
            body.extend_from_slice(&compressed.to_le_bytes());
            body.extend_from_slice(&item.local_offset.to_le_bytes());
            extra.extend_from_slice(&0x0001u16.to_le_bytes());
            extra.extend_from_slice(&(body.len() as u16).to_le_bytes());
            extra.extend_from_slice(&body);
            (U32_SENTINEL, U32_SENTINEL, U32_SENTINEL)
        } else {
            (plain as u32, compressed as u32, item.local_offset as u32)
        };

        out.extend_from_slice(b"PK\x01\x02");
        out.extend_from_slice(&20u16.to_le_bytes()); // version made by
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // time
        out.extend_from_slice(&0u16.to_le_bytes()); // date
        out.extend_from_slice(&item.crc.to_le_bytes());
        out.extend_from_slice(&compressed_field.to_le_bytes());
        out.extend_from_slice(&plain_field.to_le_bytes());
        out.extend_from_slice(&(item.spec.name.len() as u16).to_le_bytes());
        out.extend_from_slice(&(extra.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment len
        out.extend_from_slice(&0u16.to_le_bytes()); // disk start
        out.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        out.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        out.extend_from_slice(&offset_field.to_le_bytes());
        out.extend_from_slice(item.spec.name.as_bytes());
        out.extend_from_slice(&extra);
    }
    let directory_size = (out.len() - prefix.len()) as u64 - directory_start;
    let count = placed.len() as u64;

    if zip64_eocd {
        let record_at = (out.len() - prefix.len()) as u64;
        out.extend_from_slice(b"PK\x06\x06");
        out.extend_from_slice(&44u64.to_le_bytes()); // 记录剩余长度
        out.extend_from_slice(&45u16.to_le_bytes()); // version made by
        out.extend_from_slice(&45u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0u32.to_le_bytes()); // 本盘号
        out.extend_from_slice(&0u32.to_le_bytes()); // 中央目录起始盘号
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&directory_size.to_le_bytes());
        out.extend_from_slice(&directory_start.to_le_bytes());

        out.extend_from_slice(b"PK\x06\x07");
        out.extend_from_slice(&0u32.to_le_bytes()); // locator 所在盘号
        out.extend_from_slice(&record_at.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes()); // 总盘数
    }

    out.extend_from_slice(b"PK\x05\x06");
    out.extend_from_slice(&0u16.to_le_bytes()); // 本盘号
    out.extend_from_slice(&0u16.to_le_bytes()); // 中央目录起始盘号
    let (count_field, size_field, offset_field) = if zip64_eocd {
        (0xFFFFu16, U32_SENTINEL, U32_SENTINEL)
    } else {
        (count as u16, directory_size as u32, directory_start as u32)
    };
    out.extend_from_slice(&count_field.to_le_bytes());
    out.extend_from_slice(&count_field.to_le_bytes());
    out.extend_from_slice(&size_field.to_le_bytes());
    out.extend_from_slice(&offset_field.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // 注释长度
    out
}
