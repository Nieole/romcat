//! **判据**：一份内容拿去撞 DAT 的那几个数。
//!
//! 一份内容有**两套**哈希，不是一套（ADR-0002 修订段）：
//!
//! - **含头**：磁盘上原样的整份内容。TOSEC、Redump、GoodNES 与 No-Intro 的
//!   `(Headered)` 集按它记。**它零解压就拿得到**——zip 与 7z 的元数据里就有 CRC-32
//!   与未压缩大小（票 03），库里 91.1% 的容量因此不必解压。
//! - **去头**：剥掉[外挂头](super::header::DumpHeader)之后的内容。No-Intro 的
//!   `(Headerless)` 集与 SFC 全集按它记。**它一定要看见字节**——外挂头在内容里面，
//!   容器的元数据说不出有没有。
//!
//! 于是两套的成本天差地别，而 [`Headerless`] 把这件事写进了类型：**「没有外挂头」与
//! 「还没看过」是两回事**，混成一个 `Option` 就会把「这文件本来就没头」说成
//! 「去头那套撞不了」。
//!
//! ## SHA-1 是**按需才付**的第三样（票 10）
//!
//! DAT 库里有一批记录**只有 SHA-1**：GoodNES 那两份（`gamedb_goodnes.txt` 22,095 条、
//! `gamedb_sega_md.txt` 8,149 条，其中 748 条是中文汉化）连 `crc32` 与 `size` 两列都是
//! 空的。它们在第一命中层上撞不到，不是因为撞不上，是因为**根本没有可以对的那一列**。
//!
//! 但 SHA-1 要把内容整份读一遍，而 ADR-0008 的修订段已经把这笔账算过：**对光盘世代
//! 不值**。对卡带世代反过来——真机上 FC 未命中那 3,491 个变体合计 0.45 GiB，MD 那 70 个
//! 0.22 GiB。所以它既不是默认开着，也不是永远不做，而是**由调用方按平台决定**
//! （[`Want`]）：只有 DAT 库里真有 SHA-1 弹药的平台才付这笔钱。
//!
//! ## 一次读取算完两套
//!
//! 裸文件本来就要整份读一遍（容器元数据里没有它），那就顺手把两套一起算出来——
//! 读一次、两个 hasher，边际成本几乎为零（调研 D.1：RomVault 的 `Alt*` 与 igir
//! 都是这么做的）。容器内部条目反过来：含头那套白拿，去头那套要解压，因此
//! [`super`] 只在含头没撞上、且这个扩展名**可能**带头时才回头去解那一条。

use std::io::{self, Read};

use ring::digest::{Context, SHA1_FOR_LEGACY_USE_ONLY};

use super::header::{self, DumpHeader, NKIT_PROBE_LEN};

/// 探一份内容的开头要读多少字节：够认出全部五种外挂头，也够验 NKit。
pub const PROBE_LEN: usize = NKIT_PROBE_LEN;

/// 去头那套哈希的三种状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Headerless {
    /// **还没看过**：只拿到了容器元数据，一个字节都没解压。
    Unknown,
    /// 看过了，**没有外挂头**——去头与含头落在同一串字节上，撞哪一档 DAT 都用同一个数。
    Absent,
    /// 有外挂头，剥掉之后是这个。
    Present {
        /// 剥掉的是哪种头。
        header: DumpHeader,
        /// 去头之后的字节数。
        size: u64,
        /// 去头之后的 CRC-32。
        crc32: u32,
    },
}

/// SHA-1 这一样算不算，以及**肯为它多读一趟吗**。
///
/// 三档而不是一个 `bool`，因为这里有两笔完全不同的钱：
///
/// - [`Self::Free`]：这一份**本来就要整份读一遍**（裸文件，或者要算去头哈希的那些）。
///   多挂一个摘要在同一趟读上，边际成本几乎为零——那正是[模块文档](self)里
///   「读一次、两个 hasher」那句话。
/// - [`Self::Pay`]：这一份本来不必读（容器里那条 CRC-32 是零解压白拿的），要算 SHA-1
///   就得**专门为它解压一次**。这笔钱只在第一命中层已经落空、而这个平台真有 SHA-1
///   弹药时才出。
///
/// 把两者混成一个 `bool`，「顺手算」就会变成「为它多读一趟」——真机上那是把 FC 那
/// 2.01 GiB 读两遍。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sha1 {
    /// 不算。
    #[default]
    Skip,
    /// **顺手算**：这一份本来就要读，绝不为它多读一次。
    Free,
    /// **专门为它读一趟**。
    Pay,
}

impl Sha1 {
    /// 这一档要不要算出摘要来。
    #[must_use]
    pub fn wanted(self) -> bool {
        !matches!(self, Self::Skip)
    }

    /// 这一档肯不肯为了它多读一趟。
    #[must_use]
    pub fn pays_for_a_read(self) -> bool {
        matches!(self, Self::Pay)
    }
}

/// 这一趟要算哪几样。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Want {
    /// SHA-1 这一样。
    pub sha1: Sha1,
}

impl Want {
    /// 只算 CRC-32 那两套。
    #[must_use]
    pub fn crc_only() -> Self {
        Self { sha1: Sha1::Skip }
    }

    /// 该读的照读，读到了顺手把 SHA-1 也算出来。
    #[must_use]
    pub fn sha1_if_free() -> Self {
        Self { sha1: Sha1::Free }
    }

    /// 为 SHA-1 专门读一趟。
    #[must_use]
    pub fn pay_for_sha1() -> Self {
        Self { sha1: Sha1::Pay }
    }
}

/// 一份内容的判据。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fingerprint {
    /// 含头（原样）的字节数。
    pub size: u64,
    /// 含头（原样）的 CRC-32。
    pub crc32: u32,
    /// 去头那一套。
    pub headerless: Headerless,
    /// 这份内容是不是 NKit 处理过的镜像；没验过时是 `None`。
    pub nkit: Option<bool>,
    /// 含头（原样）的 SHA-1。**`None` 是「没算」不是「算不出」**——这一层按平台决定
    /// 付不付这笔钱（[`Want`]）。
    pub sha1: Option<[u8; 20]>,
    /// 去头之后的 SHA-1。没有外挂头时与 [`sha1`](Self::sha1) 是同一串。
    pub bare_sha1: Option<[u8; 20]>,
}

impl Fingerprint {
    /// 只有含头那套：容器元数据零解压给出的 CRC-32 与未压缩大小。
    #[must_use]
    pub fn as_is(size: u64, crc32: u32) -> Self {
        Self {
            size,
            crc32,
            headerless: Headerless::Unknown,
            nkit: None,
            sha1: None,
            bare_sha1: None,
        }
    }

    /// 去头那套的 `(大小, CRC-32)`；没有外挂头时与含头相同，还没看过时是 `None`。
    #[must_use]
    pub fn headerless_pair(&self) -> Option<(u64, u32)> {
        match self.headerless {
            Headerless::Unknown => None,
            Headerless::Absent => Some((self.size, self.crc32)),
            Headerless::Present { size, crc32, .. } => Some((size, crc32)),
        }
    }

    /// 剥掉的是哪种外挂头。
    #[must_use]
    pub fn header(&self) -> Option<DumpHeader> {
        match self.headerless {
            Headerless::Present { header, .. } => Some(header),
            _ => None,
        }
    }

    /// 去头那套的 SHA-1；没有外挂头时与含头那套相同，没算过是 `None`。
    #[must_use]
    pub fn headerless_sha1(&self) -> Option<[u8; 20]> {
        match self.headerless {
            Headerless::Unknown => None,
            Headerless::Absent => self.sha1,
            Headerless::Present { .. } => self.bare_sha1,
        }
    }

    /// 整份字节都在手上时的判据：两套哈希一起算出来，顺带验 NKit。
    #[must_use]
    pub fn of_bytes(name: &str, data: &[u8], want: Want) -> Self {
        let mut reader = data;
        // 读内存不会失败。
        of_reader(name, data.len() as u64, &mut reader, want).unwrap_or_else(|_| Self::as_is(0, 0))
    }
}

/// 一串 SHA-1 写成 DAT 里那种四十位小写十六进制。
#[must_use]
pub fn hex(digest: [u8; 20]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 从四十位十六进制读回一串 SHA-1；不是那个形状就是 `None`。
#[must_use]
pub fn from_hex(text: &str) -> Option<[u8; 20]> {
    if text.len() != 40 {
        return None;
    }
    let mut out = [0u8; 20];
    for (at, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(at * 2..at * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

/// 流式算一份内容的判据：**读一遍，要算的几套一起出来**。
///
/// `len` 是这份内容的总长度，必须由调用方给——SFC 的拷贝机头没有魔数，唯一的判据
/// 是「总长 % 1024 == 512」，边读边算的时候还不知道总长（见 [`super::header`]）。
///
/// # Errors
/// 读不动时返回底层错误。
pub fn of_reader(
    name: &str,
    len: u64,
    reader: &mut dyn Read,
    want: Want,
) -> io::Result<Fingerprint> {
    let mut head = vec![0u8; PROBE_LEN];
    let mut filled = 0;
    while filled < head.len() {
        let read = reader.read(&mut head[filled..])?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    head.truncate(filled);

    let header = header::dump_header(name, &head, len);
    let nkit = header::is_nkit(name, &head);
    let skip = usize::try_from(header.map_or(0, DumpHeader::bytes)).unwrap_or(usize::MAX);

    let mut whole = flate2::Crc::new();
    let mut bare = flate2::Crc::new();
    // SHA-1 那两个摘要**只在要算的时候才建**：`ring::Context` 建出来就开始占内存与
    // 初始化状态，而绝大多数平台走的是不算那一档。
    let mut whole_sha = want
        .sha1
        .wanted()
        .then(|| Context::new(&SHA1_FOR_LEGACY_USE_ONLY));
    let mut bare_sha = want
        .sha1
        .wanted()
        .then(|| Context::new(&SHA1_FOR_LEGACY_USE_ONLY));
    let tail = head.get(skip.min(head.len())..).unwrap_or_default();
    whole.update(&head);
    bare.update(tail);
    if let (Some(whole_sha), Some(bare_sha)) = (whole_sha.as_mut(), bare_sha.as_mut()) {
        whole_sha.update(&head);
        bare_sha.update(tail);
    }

    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        whole.update(&buffer[..read]);
        bare.update(&buffer[..read]);
        if let (Some(whole_sha), Some(bare_sha)) = (whole_sha.as_mut(), bare_sha.as_mut()) {
            whole_sha.update(&buffer[..read]);
            bare_sha.update(&buffer[..read]);
        }
    }

    let size = u64::from(whole.amount());
    let crc32 = whole.sum();
    let headerless = match header {
        None => Headerless::Absent,
        Some(header) => Headerless::Present {
            header,
            size: size.saturating_sub(header.bytes()),
            crc32: bare.sum(),
        },
    };
    Ok(Fingerprint {
        size,
        crc32,
        headerless,
        nkit: Some(nkit),
        sha1: whole_sha.map(finish),
        bare_sha1: bare_sha.map(finish),
    })
}

/// 收尾一个 SHA-1 摘要。SHA-1 就是 20 字节，长度对不上只可能是换错了算法。
fn finish(context: Context) -> [u8; 20] {
    let digest = context.finish();
    let mut out = [0u8; 20];
    out.copy_from_slice(&digest.as_ref()[..20]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::container::crc32;

    fn ines(payload: &[u8]) -> Vec<u8> {
        let mut out = b"NES\x1A\x02\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec();
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn 一次读取算出含头与去头两套() {
        let payload = vec![0xABu8; 40_960];
        let bytes = ines(&payload);
        let got = Fingerprint::of_bytes("Foo.nes", &bytes, Want::crc_only());
        assert_eq!(got.size, 40_976, "含头是整个文件");
        assert_eq!(got.crc32, crc32(&bytes));
        assert_eq!(
            got.headerless,
            Headerless::Present {
                header: DumpHeader::INes,
                size: 40_960,
                crc32: crc32(&payload),
            },
            "去头是剥掉 16 字节 iNES 头之后的那一段"
        );
        assert_eq!(got.headerless_pair(), Some((40_960, crc32(&payload))));
    }

    #[test]
    fn 没有外挂头时两套落在同一串字节上() {
        let bytes = vec![0x11u8; 1024];
        let got = Fingerprint::of_bytes("Foo.gba", &bytes, Want::crc_only());
        assert_eq!(got.headerless, Headerless::Absent);
        assert_eq!(got.headerless_pair(), Some((1024, crc32(&bytes))));
        assert_eq!(got.header(), None);
    }

    #[test]
    fn 没看过与没有头是两回事() {
        // 容器元数据只给得出含头那套。说成「没有外挂头」，去头那档 DAT 就会拿
        // 含头的数去撞，撞出来的还是一条错的候选。
        let 只有元数据 = Fingerprint::as_is(40_976, 0x1234_5678);
        assert_eq!(只有元数据.headerless, Headerless::Unknown);
        assert_eq!(只有元数据.headerless_pair(), None);
        assert_eq!(只有元数据.nkit, None);
    }

    #[test]
    fn 分块读与整份读算出同一套数() {
        // 外挂头横跨读取边界时最容易出错：探头的那一读只填了一半缓冲。
        struct 一次一字节<'a>(&'a [u8]);
        impl Read for 一次一字节<'_> {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                if self.0.is_empty() || buf.is_empty() {
                    return Ok(0);
                }
                buf[0] = self.0[0];
                self.0 = &self.0[1..];
                Ok(1)
            }
        }
        let bytes = ines(&vec![0x5Au8; 8192]);
        let mut reader = 一次一字节(&bytes);
        let 分块 = of_reader("Foo.nes", bytes.len() as u64, &mut reader, Want::crc_only())
            .expect("读得出");
        assert_eq!(
            分块,
            Fingerprint::of_bytes("Foo.nes", &bytes, Want::crc_only())
        );
    }

    #[test]
    fn 比探头长度还短的内容也算得出来() {
        let bytes = vec![0x01u8, 0x02, 0x03];
        let got = Fingerprint::of_bytes("小.bin", &bytes, Want::crc_only());
        assert_eq!((got.size, got.crc32), (3, crc32(&bytes)));
        assert_eq!(got.headerless, Headerless::Absent);
    }

    #[test]
    fn nkit_在算哈希的同一趟里就验了() {
        let mut bytes = vec![0u8; 0x400];
        bytes[0x200..0x204].copy_from_slice(b"NKIT");
        let got = Fingerprint::of_bytes("WII/游戏.iso", &bytes, Want::crc_only());
        assert_eq!(got.nkit, Some(true));
    }
}
