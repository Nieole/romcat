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

// ─────────────────────────── tar 与 zst ───────────────────────────
//
// **同样故意不借第三方打包器。** 这一侧要验的两件事恰恰是「别人打出来的 tar.zst
// 长什么样」：bsdtar 默认写 pax，第一个 512 字节块是 `PaxHeader/…` 伪条目
// （`typeflag='x'`），真条目在偏移 1024（调研 2.5 实测）；以及 `tar --zstd` 的帧头
// **既没有原始大小、也没有校验和**（调研 1.3 实测）。现成的打包器只会写出它自己那一种
// 形态，造不出这两种样本。偏移与字段照 POSIX.1-2024 的 `posix_header`。

/// 一个待写进 tar 的成员。
#[derive(Debug, Clone)]
pub struct TarEntrySpec {
    name: String,
    data: Vec<u8>,
    dir: bool,
    pax: bool,
    ustar: bool,
    long_link: bool,
}

impl TarEntrySpec {
    /// 一个普通文件条目（ustar 形态）。
    #[must_use]
    pub fn file(name: &str, data: impl Into<Vec<u8>>) -> Self {
        Self {
            name: name.to_string(),
            data: data.into(),
            dir: false,
            pax: false,
            ustar: true,
            long_link: false,
        }
    }

    /// 一个目录条目：`typeflag='5'`，没有内容。
    #[must_use]
    pub fn dir(name: &str) -> Self {
        Self {
            dir: true,
            ..Self::file(name, Vec::new())
        }
    }

    /// 在这个条目前面加一个 **pax 扩展头伪条目**（`typeflag='x'`）。
    ///
    /// 这就是那个坑：写完之后第一个 512 字节块的 `name` 是 `PaxHeader/<名字>`，
    /// 真条目要往后数到偏移 1024。「读前 512 字节取文件名」在这上面会拿到垃圾。
    #[must_use]
    pub fn with_pax_header(mut self) -> Self {
        self.pax = true;
        self
    }

    /// 写成老式 v7 头：**没有 `ustar` 魔数**，只能靠头部校验和认出它是 tar。
    #[must_use]
    pub fn as_v7(mut self) -> Self {
        self.ustar = false;
        self
    }

    /// 在这个条目前面加一个 **GNU longname 伪条目**（`././@LongLink`，
    /// `typeflag='L'`），并把真条目的 `name` 字段截到 100 字节。
    ///
    /// 伪条目本身写成**老式 v7 头**（magic 处全是零）——真库里就是这样：NDS 那批
    /// 559 个 `.tar.zst` 有 414 个如此。`tar` crate 对这种块视而不见（它要求伪条目
    /// 自己带 `ustar` 魔数），于是伪条目会被当成一个真文件交出来，而真条目的名字
    /// 停在 100 字节处。系统 `bsdtar` 认它。**造得出这种样本，才测得出有没有认。**
    #[must_use]
    pub fn with_gnu_long_name(mut self) -> Self {
        self.long_link = true;
        self
    }
}

const TAR_BLOCK: usize = 512;

/// 造一份 tar：各条目依次排开，末尾两个全零块收尾（POSIX 的 end-of-archive）。
#[must_use]
pub fn tar_archive(entries: &[TarEntrySpec]) -> Vec<u8> {
    let mut out = Vec::new();
    for entry in entries {
        if entry.pax {
            // pax 记录的格式是 `<长度> <键>=<值>\n`，长度把自己也算进去。
            let body = pax_record("mtime", "1788150628.836242432");
            let base = entry.name.rsplit('/').next().unwrap_or(&entry.name);
            out.extend_from_slice(&tar_header(
                &format!("PaxHeader/{base}"),
                body.len() as u64,
                b'x',
                true,
            ));
            push_padded(&mut out, body.as_bytes());
        }
        if entry.long_link {
            // 伪条目的内容就是真名字，以 NUL 收尾；头本身是 v7 形态。
            let mut name = entry.name.as_bytes().to_vec();
            name.push(0);
            out.extend_from_slice(&tar_header("././@LongLink", name.len() as u64, b'L', false));
            push_padded(&mut out, &name);
        }
        let typeflag = if entry.dir { b'5' } else { b'0' };
        out.extend_from_slice(&tar_header(
            &entry.name,
            entry.data.len() as u64,
            typeflag,
            entry.ustar,
        ));
        if !entry.dir {
            push_padded(&mut out, &entry.data);
        }
    }
    out.extend_from_slice(&[0u8; TAR_BLOCK * 2]);
    out
}

fn pax_record(key: &str, value: &str) -> String {
    let payload = format!(" {key}={value}\n");
    // 长度前缀把自己也算进去，所以要迭代到稳定为止。
    let mut len = payload.len() + 1;
    loop {
        let candidate = format!("{len}{payload}");
        if candidate.len() == len {
            return candidate;
        }
        len = candidate.len();
    }
}

fn push_padded(out: &mut Vec<u8>, data: &[u8]) {
    out.extend_from_slice(data);
    let rem = data.len() % TAR_BLOCK;
    if rem != 0 {
        out.extend(std::iter::repeat_n(0u8, TAR_BLOCK - rem));
    }
}

fn tar_header(name: &str, size: u64, typeflag: u8, ustar: bool) -> [u8; TAR_BLOCK] {
    let mut block = [0u8; TAR_BLOCK];
    let name = name.as_bytes();
    let take = name.len().min(100);
    block[..take].copy_from_slice(&name[..take]);
    put_octal(&mut block[100..108], 0o644, 7);
    put_octal(&mut block[108..116], 0, 7);
    put_octal(&mut block[116..124], 0, 7);
    put_octal(&mut block[124..136], size, 11);
    put_octal(&mut block[136..148], 0, 11);
    block[156] = typeflag;
    if ustar {
        block[257..263].copy_from_slice(b"ustar\0");
        block[263..265].copy_from_slice(b"00");
    }
    // 校验和最后算：算的时候 `chksum` 那 8 字节当作 8 个空格（POSIX.1-2024）。
    for slot in &mut block[148..156] {
        *slot = b' ';
    }
    let sum: u64 = block.iter().map(|b| u64::from(*b)).sum();
    let text = format!("{sum:06o}\0 ");
    block[148..156].copy_from_slice(text.as_bytes());
    block
}

fn put_octal(field: &mut [u8], value: u64, digits: usize) {
    let text = format!("{value:0digits$o}\0", digits = digits);
    field[..text.len()].copy_from_slice(text.as_bytes());
}

/// 压成一个 zstd 帧，**带** `Frame_Content_Size` 与 `Content_Checksum`。
///
/// 这是 `zstd -f a.tar -o a.tar.zst` 那一种：输入是常规文件，大小事先就知道。
#[must_use]
pub fn zstd_frame(plain: &[u8]) -> Vec<u8> {
    encode(plain, true, true)
}

/// 压成一个 zstd 帧，**既不写原始大小、也不写校验和**。
///
/// 这是 `tar --zstd -cf`（libarchive）那一种——调研 1.3 实测帧头只有 6 字节，
/// `Frame_Header_Descriptor = 0x00`。**约 47% 的真实文件长这样**，
/// 「零解压拿未压缩大小」在它们身上不成立。
#[must_use]
pub fn zstd_frame_without_size(plain: &[u8]) -> Vec<u8> {
    encode(plain, false, false)
}

/// 两个开关分开给：帧头里的**原始大小**与**内容校验和**是各自独立的两位
/// （`Frame_Header_Descriptor` 的 7-6 位与第 2 位）。真实文件里它们碰巧同进同出，
/// 但那是打包工具的巧合，不是格式的约束——一个参数管两件事会把这个巧合写死进样本里。
fn encode(plain: &[u8], content_size: bool, checksum: bool) -> Vec<u8> {
    let mut encoder = zstd::Encoder::new(Vec::new(), 3).expect("写进内存不会失败");
    encoder
        .include_checksum(checksum)
        .expect("写进内存不会失败");
    if content_size {
        encoder
            .set_pledged_src_size(Some(plain.len() as u64))
            .expect("写进内存不会失败");
    }
    encoder.write_all(plain).expect("写进内存不会失败");
    encoder.finish().expect("写进内存不会失败")
}

/// 造一个 `.tar.zst`：tar 套 zst，带原始大小与校验和。
#[must_use]
pub fn tar_zst(entries: &[TarEntrySpec]) -> Vec<u8> {
    zstd_frame(&tar_archive(entries))
}

/// 造一个 `tar --zstd` 形态的 `.tar.zst`：帧头没有原始大小，也没有校验和。
#[must_use]
pub fn tar_zst_without_size(entries: &[TarEntrySpec]) -> Vec<u8> {
    zstd_frame_without_size(&tar_archive(entries))
}

/// 造一个帧头写着**要外部字典**的 `.zst`。
///
/// 真库里实测 0 个，但格式允许，而少了这本字典这个文件就永远解不开。帧体是什么
/// 无所谓——零解压层在读到字典 ID 的那一刻就该停下。
#[must_use]
pub fn zst_needing_dictionary(id: u32, body: &[u8]) -> Vec<u8> {
    let mut out = vec![0x28, 0xB5, 0x2F, 0xFD];
    // 描述符 0x03：字典 ID 标志 3 → 4 字节；无内容大小、无校验和、非单段。
    out.push(0x03);
    // 窗口描述符：exponent 11、mantissa 0 → 2 MiB。
    out.push(0x58);
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(body);
    out
}
