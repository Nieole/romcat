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
//! ## 一次读取算完两套
//!
//! 裸文件本来就要整份读一遍（容器元数据里没有它），那就顺手把两套一起算出来——
//! 读一次、两个 hasher，边际成本几乎为零（调研 D.1：RomVault 的 `Alt*` 与 igir
//! 都是这么做的）。容器内部条目反过来：含头那套白拿，去头那套要解压，因此
//! [`super`] 只在含头没撞上、且这个扩展名**可能**带头时才回头去解那一条。

use std::io::{self, Read};

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

    /// 整份字节都在手上时的判据：两套哈希一起算出来，顺带验 NKit。
    #[must_use]
    pub fn of_bytes(name: &str, data: &[u8]) -> Self {
        let mut reader = data;
        // 读内存不会失败。
        of_reader(name, data.len() as u64, &mut reader).unwrap_or_else(|_| Self::as_is(0, 0))
    }
}

/// 流式算一份内容的判据：**读一遍，两套哈希一起出来**。
///
/// `len` 是这份内容的总长度，必须由调用方给——SFC 的拷贝机头没有魔数，唯一的判据
/// 是「总长 % 1024 == 512」，边读边算的时候还不知道总长（见 [`super::header`]）。
///
/// # Errors
/// 读不动时返回底层错误。
pub fn of_reader(name: &str, len: u64, reader: &mut dyn Read) -> io::Result<Fingerprint> {
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
    whole.update(&head);
    bare.update(head.get(skip.min(head.len())..).unwrap_or_default());

    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        whole.update(&buffer[..read]);
        bare.update(&buffer[..read]);
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
    })
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
        let got = Fingerprint::of_bytes("Foo.nes", &bytes);
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
        let got = Fingerprint::of_bytes("Foo.gba", &bytes);
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
        let 分块 = of_reader("Foo.nes", bytes.len() as u64, &mut reader).expect("读得出");
        assert_eq!(分块, Fingerprint::of_bytes("Foo.nes", &bytes));
    }

    #[test]
    fn 比探头长度还短的内容也算得出来() {
        let bytes = vec![0x01u8, 0x02, 0x03];
        let got = Fingerprint::of_bytes("小.bin", &bytes);
        assert_eq!((got.size, got.crc32), (3, crc32(&bytes)));
        assert_eq!(got.headerless, Headerless::Absent);
    }

    #[test]
    fn nkit_在算哈希的同一趟里就验了() {
        let mut bytes = vec![0u8; 0x400];
        bytes[0x200..0x204].copy_from_slice(b"NKIT");
        let got = Fingerprint::of_bytes("WII/游戏.iso", &bytes);
        assert_eq!(got.nkit, Some(true));
    }
}
