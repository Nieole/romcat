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
    /// 名字的**原始字节**，不是 `String`。
    ///
    /// zip 的名字只有在通用标志位第 11 位置位时才保证是 UTF-8，其余是「本地代码页」
    /// ——真库里大量中文名是 GBK。`String` 装不下那种字节，而
    /// [编码探测](crate::container::charset)恰恰只能拿那种字节去测
    /// （见 [`stored_raw`](Self::stored_raw)）。
    name: Vec<u8>,
    data: Vec<u8>,
    deflate: bool,
    data_descriptor: bool,
    zip64_extra: bool,
    zip64_extra_full: bool,
    /// 这一条的数据在第几段上（APPNOTE §8 的 split archive）。默认 0，也就是不分段。
    disk: u16,
}

impl ZipEntrySpec {
    /// 原样存放（压缩方法 0）。
    #[must_use]
    pub fn stored(name: &str, data: impl Into<Vec<u8>>) -> Self {
        Self {
            name: name.as_bytes().to_vec(),
            data: data.into(),
            deflate: false,
            data_descriptor: false,
            zip64_extra: false,
            zip64_extra_full: false,
            disk: 0,
        }
    }

    /// 把这一条的数据放到**别的分段**上（ZIP 官方 split，APPNOTE §8）。
    ///
    /// 真库里 WIIU 那 4 组 `XenobladeX-…-WUP.z01…z04 + .zip` 就是这个样子：
    /// 中央目录在末段那个 `.zip` 里，条目的数据分散在前面几段上。
    #[must_use]
    pub fn on_disk(mut self, disk: u16) -> Self {
        self.disk = disk;
        self
    }

    /// 名字按**原始字节**给：造非 UTF-8 的内部名字用它（真库里那批 GBK 名字）。
    #[must_use]
    pub fn stored_raw(name: &[u8], data: impl Into<Vec<u8>>) -> Self {
        Self {
            name: name.to_vec(),
            ..Self::stored("", data)
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
        out.extend_from_slice(&spec.name);
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
        out.extend_from_slice(&item.spec.disk.to_le_bytes()); // disk start
        out.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        out.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        out.extend_from_slice(&offset_field.to_le_bytes());
        out.extend_from_slice(&item.spec.name);
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

// ───────────────────────────── rar ─────────────────────────────

/// 一个待写进 rar 的内部文件。
///
/// **故意不借第三方 rar 库**（也没有干净的那一个可借——`unrar` crate 内嵌的
/// C++ 源码许可传染，ADR-0014）。这里一个字节一个字节地摆，偏移与字段顺序照
/// <https://www.rarlab.com/technote.htm>（RAR5）与 unrar 的 `arcread.cpp`（RAR4）。
///
/// 这样才造得出真正要验的那几种样本：**分卷的非末段**（校验和记的是打包后数据的）、
/// **只写 BLAKE2sp 的 RAR5**（一条 CRC-32 都没有）、**带密码时被搅过的校验和**。
/// 现成的库只会压出规规矩矩的容器，这三种一种都造不出来。
#[derive(Debug, Clone)]
pub struct RarEntrySpec {
    name: Vec<u8>,
    /// 数据区的字节。原样存放时它就是内容本身。
    data: Vec<u8>,
    /// 未压缩大小。分卷的一段上它是**整个文件**的大小，不是这一段的长度。
    size: u64,
    crc32: Option<u32>,
    blake2: bool,
    stored: bool,
    solid: bool,
    is_dir: bool,
    split_before: bool,
    split_after: bool,
    /// 带密码，且校验和被密钥搅过（`unrar lt` 印成 `CRC32 MAC`）。
    tweaked: bool,
    /// RAR4 专用：名字里带 unrar 私有的 Unicode 编码段。
    unicode: Option<Vec<u8>>,
}

impl RarEntrySpec {
    /// 原样存放（压缩方法 0），CRC-32 按内容算。
    #[must_use]
    pub fn stored(name: &str, data: impl Into<Vec<u8>>) -> Self {
        let data = data.into();
        Self {
            name: name.as_bytes().to_vec(),
            size: data.len() as u64,
            crc32: Some(crc32(&data)),
            data,
            blake2: false,
            stored: true,
            solid: false,
            is_dir: false,
            split_before: false,
            split_after: false,
            tweaked: false,
            unicode: None,
        }
    }

    /// 压缩过的条目：数据区的字节是什么无所谓——**零解压层从不去解它**。
    #[must_use]
    pub fn compressed(name: &str, data: impl Into<Vec<u8>>) -> Self {
        Self {
            stored: false,
            ..Self::stored(name, data)
        }
    }

    /// 名字按**原始字节**给：造 GBK 名字用它。
    #[must_use]
    pub fn stored_raw(name: &[u8], data: impl Into<Vec<u8>>) -> Self {
        Self {
            name: name.to_vec(),
            ..Self::stored("", data)
        }
    }

    /// RAR4 的名字带 unrar 私有的 Unicode 编码段（`LHD_UNICODE`）。
    #[must_use]
    pub fn with_unicode_name(mut self, encoded: &[u8]) -> Self {
        self.unicode = Some(encoded.to_vec());
        self
    }

    /// 未压缩大小与数据区的长度不一样时（压缩过的、或者分卷的一段）用它。
    #[must_use]
    pub fn with_size(mut self, size: u64) -> Self {
        self.size = size;
        self
    }

    /// 换掉校验和那一栏。分卷的末段要它——那一段记的才是未压缩数据的 CRC。
    #[must_use]
    pub fn with_crc32(mut self, crc: u32) -> Self {
        self.crc32 = Some(crc);
        self
    }

    /// 目录条目。
    #[must_use]
    pub fn dir(name: &str) -> Self {
        Self {
            is_dir: true,
            size: 0,
            data: Vec::new(),
            crc32: None,
            ..Self::stored(name, Vec::new())
        }
    }

    /// **只写 BLAKE2sp，不写 CRC-32**（`rar a -htb` 的产物）。
    #[must_use]
    pub fn blake2_only(mut self) -> Self {
        self.crc32 = None;
        self.blake2 = true;
        self
    }

    /// solid：接着用上一个文件留下的压缩字典。
    #[must_use]
    pub fn solid(mut self) -> Self {
        self.solid = true;
        self.stored = false;
        self
    }

    /// 数据续到下一卷。**这一段记的校验和是打包后数据的**，所以顺手把它换成别的值。
    #[must_use]
    pub fn split_after(mut self, packed_crc: u32, total_size: u64) -> Self {
        self.split_after = true;
        self.crc32 = Some(packed_crc);
        self.size = total_size;
        self
    }

    /// 数据从上一卷续来。
    #[must_use]
    pub fn split_before(mut self, total_size: u64) -> Self {
        self.split_before = true;
        self.size = total_size;
        self
    }

    /// 带密码，且校验和被密钥搅过。
    #[must_use]
    pub fn tweaked_checksum(mut self, mac: u32) -> Self {
        self.tweaked = true;
        self.crc32 = Some(mac);
        self
    }
}

/// RAR5 的变长整数。
fn vint(value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    let mut left = value;
    loop {
        let byte = (left & 0x7F) as u8;
        left >>= 7;
        if left == 0 {
            out.push(byte);
            return out;
        }
        out.push(byte | 0x80);
    }
}

/// 把一个 RAR5 头部封好：算出头长、算出头部自己的 CRC-32、拼起来。
///
/// technote 原文：头部 CRC 覆盖的是**从 Header size 字段起、到 extra area 结束**
/// 那一段。照着算，产出的样本就是真 rar 而不是「只够骗过自己解析器」的字节——
/// 真机上拿官方 `unrar lt` 读过，名字、大小与 CRC-32 一条不差。
fn rar5_header(kind: u64, flags: u64, body: &[u8], extra: &[u8], data_len: u64) -> Vec<u8> {
    let mut head = Vec::new();
    head.extend_from_slice(&vint(kind));
    head.extend_from_slice(&vint(flags));
    if flags & 0x0001 != 0 {
        head.extend_from_slice(&vint(extra.len() as u64));
    }
    if flags & 0x0002 != 0 {
        head.extend_from_slice(&vint(data_len));
    }
    head.extend_from_slice(body);
    head.extend_from_slice(extra);

    let mut sized = vint(head.len() as u64);
    sized.extend_from_slice(&head);
    let mut out = crc32(&sized).to_le_bytes().to_vec();
    out.extend_from_slice(&sized);
    out
}

/// 造一份 RAR5（不是分卷）。
#[must_use]
pub fn rar5_archive(entries: &[RarEntrySpec]) -> Vec<u8> {
    rar5_build(entries, None, false)
}

/// 造 RAR5 分卷里的一卷。
///
/// `number` 是卷号（从 0 起），`next_volume` 是「容器结束块说后面还有一卷」。
#[must_use]
pub fn rar5_volume(entries: &[RarEntrySpec], number: u64, next_volume: bool) -> Vec<u8> {
    rar5_build(entries, Some(number), next_volume)
}

fn rar5_build(entries: &[RarEntrySpec], number: Option<u64>, next_volume: bool) -> Vec<u8> {
    let mut out = b"Rar!\x1a\x07\x01\x00".to_vec();
    // 容器总头。分卷时置「分卷」（0x01）与「卷号字段存在」（0x02）两位。
    let mut main = vint(if number.is_some() { 0x0003 } else { 0x0000 });
    if let Some(number) = number {
        main.extend_from_slice(&vint(number));
    }
    out.extend_from_slice(&rar5_header(1, 0x0000, &main, &[], 0));

    for entry in entries {
        let mut file_flags = 0u64;
        if entry.is_dir {
            file_flags |= 0x0001;
        }
        if entry.crc32.is_some() {
            file_flags |= 0x0004;
        }
        let mut body = vint(file_flags);
        body.extend_from_slice(&vint(entry.size));
        body.extend_from_slice(&vint(0x20));
        if let Some(crc) = entry.crc32 {
            body.extend_from_slice(&crc.to_le_bytes());
        }
        // compression info：低 6 位版本，第 6 位 solid，第 7–9 位方法。
        let method: u64 = if entry.stored { 0 } else { 3 };
        let compression = (method << 7) | if entry.solid { 0x40 } else { 0 };
        body.extend_from_slice(&vint(compression));
        body.extend_from_slice(&vint(0));
        body.extend_from_slice(&vint(entry.name.len() as u64));
        body.extend_from_slice(&entry.name);

        let mut extra = Vec::new();
        if entry.tweaked {
            // 文件加密记录：版本、旗标（0x02 = 校验和被搅过）、KDF 次数、盐、IV。
            let mut record = vint(0x01);
            record.extend_from_slice(&vint(0));
            record.extend_from_slice(&vint(0x0002));
            record.push(15);
            record.extend_from_slice(&[0u8; 16]);
            record.extend_from_slice(&[0u8; 16]);
            extra.extend_from_slice(&vint(record.len() as u64));
            extra.extend_from_slice(&record);
        }
        if entry.blake2 {
            // 文件哈希记录：类型 0x02，哈希类型 0x00 = BLAKE2sp，32 字节摘要。
            let mut record = vint(0x02);
            record.extend_from_slice(&vint(0x00));
            record.extend_from_slice(&[0xABu8; 32]);
            extra.extend_from_slice(&vint(record.len() as u64));
            extra.extend_from_slice(&record);
        }

        let mut flags = 0u64;
        if !extra.is_empty() {
            flags |= 0x0001;
        }
        if !entry.data.is_empty() {
            flags |= 0x0002;
        }
        if entry.split_before {
            flags |= 0x0008;
        }
        if entry.split_after {
            flags |= 0x0010;
        }
        out.extend_from_slice(&rar5_header(
            2,
            flags,
            &body,
            &extra,
            entry.data.len() as u64,
        ));
        out.extend_from_slice(&entry.data);
    }

    // 服务头（quick open）：**不是内部文件**，认成文件的话每个容器都会凭空多一条。
    let mut service = vint(0);
    service.extend_from_slice(&vint(4));
    service.extend_from_slice(&vint(0));
    service.extend_from_slice(&vint(0));
    service.extend_from_slice(&vint(0));
    service.extend_from_slice(&vint(2));
    service.extend_from_slice(b"QO");
    out.extend_from_slice(&rar5_header(3, 0x0002, &service, &[], 4));
    out.extend_from_slice(&[0u8; 4]);

    let end = vint(u64::from(next_volume));
    out.extend_from_slice(&rar5_header(5, 0x0000, &end, &[], 0));
    out
}

/// 造一份 RAR4（不是分卷）。
#[must_use]
pub fn rar4_archive(entries: &[RarEntrySpec]) -> Vec<u8> {
    rar4_build(entries, false, false, false)
}

/// 造 RAR4 分卷里的一卷。`new_numbering` 决定下一卷叫 `.partN.rar` 还是 `.rNN`。
#[must_use]
pub fn rar4_volume(entries: &[RarEntrySpec], next_volume: bool, new_numbering: bool) -> Vec<u8> {
    rar4_build(entries, true, next_volume, new_numbering)
}

fn rar4_build(
    entries: &[RarEntrySpec],
    volume: bool,
    next_volume: bool,
    new_numbering: bool,
) -> Vec<u8> {
    let mut out = b"Rar!\x1a\x07\x00".to_vec();
    let mut main_flags = if volume { 0x0001u16 } else { 0 }; // MHD_VOLUME
    if new_numbering {
        main_flags |= 0x0010;
    }
    out.extend_from_slice(&rar4_block(0x73, main_flags, &[0u8; 6], &[]));

    for entry in entries {
        let mut flags = 0x8000u16; // LONG_BLOCK：后面跟着数据
        if entry.split_before {
            flags |= 0x0001;
        }
        if entry.split_after {
            flags |= 0x0002;
        }
        if entry.solid {
            flags |= 0x0010;
        }
        if entry.is_dir {
            flags |= 0x00E0;
        }
        let name = match &entry.unicode {
            Some(encoded) => {
                flags |= 0x0200;
                let mut name = entry.name.clone();
                name.push(0);
                name.extend_from_slice(encoded);
                name
            }
            None => entry.name.clone(),
        };

        let mut body = Vec::new();
        body.extend_from_slice(&(entry.data.len() as u32).to_le_bytes());
        body.extend_from_slice(&(entry.size as u32).to_le_bytes());
        body.push(2); // Host OS
        // **RAR4 的 CRC-32 无条件存在**：没有旗标可以省掉它。
        body.extend_from_slice(&entry.crc32.unwrap_or(0).to_le_bytes());
        body.extend_from_slice(&0u32.to_le_bytes()); // 时间
        body.push(29); // 解包版本
        body.push(if entry.stored { b'0' } else { b'3' });
        body.extend_from_slice(&(name.len() as u16).to_le_bytes());
        body.extend_from_slice(&0u32.to_le_bytes()); // 属性
        out.extend_from_slice(&rar4_block(0x74, flags, &body, &name));
        out.extend_from_slice(&entry.data);
    }

    let end_flags = 0x4000u16 | if next_volume { 0x0001 } else { 0 };
    out.extend_from_slice(&rar4_block(0x7B, end_flags, &[], &[]));
    out
}

/// 封一个 RAR4 块：头长写进去，再算头部自己的 CRC（取低 16 位）。
fn rar4_block(kind: u8, flags: u16, body: &[u8], tail: &[u8]) -> Vec<u8> {
    let head_size = (7 + body.len() + tail.len()) as u16;
    let mut head = vec![kind];
    head.extend_from_slice(&flags.to_le_bytes());
    head.extend_from_slice(&head_size.to_le_bytes());
    head.extend_from_slice(body);
    head.extend_from_slice(tail);
    let mut out = ((crc32(&head) & 0xFFFF) as u16).to_le_bytes().to_vec();
    out.extend_from_slice(&head);
    out
}
