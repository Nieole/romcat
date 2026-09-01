//! **外挂头**与 **NKit**：撞 DAT 之前先得看清的两件事。
//!
//! ## 外挂头：同一份内容有两个哈希
//!
//! 转储工具会在真正的内容前面挂一段自己的头——iNES 的 16 字节、FDS 的 16 字节、
//! Lynx 的 64 字节、A7800 的 128 字节、SFC 拷贝机的 512 字节。而 DAT 的两套口径同时
//! 存在：**No-Intro 的 headerless 集按去头哈希，TOSEC 与 GoodNES 按含头原样**
//! （ADR-0002 的修订段）。只算一套就会整边落空——含头那边丢掉的正是 TOSEC 里全部的
//! 汉化条目。
//!
//! 检测表照 igir 的 skipper 表（`docs/research/rom-identification.md` B.5.4），
//! 那份表本身以 No-Intro 的 skipper 文件名为键：
//!
//! | 头 | 判据 | 去掉 |
//! |---|---|---|
//! | iNES | 偏移 0 是 `NES\x1A` | 16 |
//! | FDS | 偏移 0 是 `FDS\x1A` | 16 |
//! | Lynx | 偏移 0 是 `LYNX` | 64 |
//! | A7800 | 偏移 1 是 `ATARI7800` | 128 |
//! | SFC 拷贝机 | 扩展名是 SFC 家族，且**总长 % 1024 == 512** | 512 |
//!
//! 前四种有魔数，第五种没有——拷贝机头里没有任何固定字节，判据只能是那 512 字节的
//! 余数（`header::probe_snes` 与 MAME 的 `snes.cpp` 用的是同一条）。**因此它必须知道
//! 文件总长**，光看头部字节认不出来。
//!
//! ## NKit：CRC 层的头号误报源
//!
//! Dolphin 的原话（`VolumeVerifier.cpp`）：这个文件**的 CRC32 可能和好转储的相同，
//! 即使两个文件并不完全一样**。判据是光盘逻辑偏移 `0x200` 处的 4 字节 `NKIT`
//! （`VolumeDisc::IsNKit()`）。所以 GC / Wii 的镜像在撞 CRC **之前**先验这一条，
//! 命中了也不许自动通过——真正的处置（转回 ISO 再识别）是票 09 的活。

use crate::path::{extension_lower, file_name_of_key};
use std::path::Path;

/// 一种外挂在真正内容前面的转储头。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DumpHeader {
    /// iNES 的 16 字节头（`.nes`）。
    INes,
    /// FDS 的 16 字节头（`.fds`）。
    Fds,
    /// Lynx 的 64 字节头（`.lnx`）。
    Lynx,
    /// A7800 的 128 字节头（`.a78`）。
    Atari7800,
    /// SFC 拷贝机的 512 字节头（`.smc` 常见）。
    SnesCopier,
}

impl DumpHeader {
    /// 报告与**依据**里写的那个名字。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::INes => "iNES 16 字节头",
            Self::Fds => "FDS 16 字节头",
            Self::Lynx => "Lynx 64 字节头",
            Self::Atari7800 => "A7800 128 字节头",
            Self::SnesCopier => "SFC 拷贝机 512 字节头",
        }
    }

    /// 从存进中立库的那个名字认回来；认不出时是 `None`。
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        Self::all().into_iter().find(|it| it.label() == label)
    }

    /// 全部五种，顺序固定。
    #[must_use]
    pub fn all() -> [Self; 5] {
        [
            Self::INes,
            Self::Fds,
            Self::Lynx,
            Self::Atari7800,
            Self::SnesCopier,
        ]
    }

    /// 去头要跳过几个字节。
    #[must_use]
    pub fn bytes(self) -> u64 {
        match self {
            Self::INes | Self::Fds => 16,
            Self::Lynx => 64,
            Self::Atari7800 => 128,
            Self::SnesCopier => 512,
        }
    }
}

/// 认出外挂头。
///
/// `name` 是文件名（容器内部条目就是它的内部路径），`head` 是开头若干字节，
/// `len` 是这份内容的总长度。
///
/// 拿不准就返回 `None`：**多剥一次头会算出一个谁也对不上的哈希**，而少剥一次最多是
/// 少一条候选——两种错的代价不对称。
#[must_use]
pub fn dump_header(name: &str, head: &[u8], len: u64) -> Option<DumpHeader> {
    if head.starts_with(b"NES\x1A") {
        return Some(DumpHeader::INes);
    }
    if head.starts_with(b"FDS\x1A") {
        return Some(DumpHeader::Fds);
    }
    if head.starts_with(b"LYNX") {
        return Some(DumpHeader::Lynx);
    }
    if head.get(1..10) == Some(b"ATARI7800".as_slice()) {
        return Some(DumpHeader::Atari7800);
    }
    // 拷贝机头没有魔数，只有那 512 字节的余数。所以它是唯一一条要看扩展名与总长的。
    if is_snes(name) && len % 1024 == 512 {
        return Some(DumpHeader::SnesCopier);
    }
    None
}

/// 这个名字**有没有可能**带外挂头。
///
/// 用来决定「含头那套没撞上时，值不值得回盘再读一次算去头那套」。答否的文件一律
/// 不再碰盘——库里 91.1% 的容量在**透明容器**里，含头的 CRC-32 零解压就拿得到，
/// 为了一个不可能存在的头去解压整个容器是纯亏。
#[must_use]
pub fn may_have_header(name: &str) -> bool {
    let path = Path::new(file_name_of_key(name));
    let Some(ext) = extension_lower(path) else {
        return false;
    };
    is_snes_ext(&ext) || matches!(ext.as_str(), "nes" | "fds" | "lnx" | "lyx" | "a78")
}

/// **尺寸说不说得通有个外挂头**。
///
/// 容器里的条目看不见字节——魔数要解压才读得到。但外挂头都是「固定长度的头加上
/// 整齐的内容」，于是**未压缩大小**自己就能排除掉绝大多数：iNES 的内容是 16 KiB 的
/// PRG 与 8 KiB 的 CHR，总长必然是 1024 的整数倍加 16；拷贝机头那 512 字节后面跟着
/// 32 KiB 的整数倍。尺寸对不上就一定没有外挂头，**两套哈希落在同一串字节上**，
/// 含头那次已经把去头那档 DAT 一并撞过了，不必再为它解压一次。
///
/// FDS 是唯一一个不能按 1024 取余的：它的每一面是 65,500 字节，不是 2 的幂。
#[must_use]
pub fn size_suggests_header(name: &str, size: u64) -> bool {
    let Some(ext) = extension_lower(Path::new(file_name_of_key(name))) else {
        return false;
    };
    if is_snes_ext(&ext) {
        return size % 1024 == 512;
    }
    match ext.as_str() {
        "nes" => size % 1024 == 16,
        // 16 字节头 + 若干面，每面 65,500 字节。
        "fds" => size > 16 && (size - 16).is_multiple_of(65_500),
        "lnx" | "lyx" => size % 1024 == 64,
        "a78" => size % 1024 == 128,
        _ => false,
    }
}

fn is_snes(name: &str) -> bool {
    extension_lower(Path::new(file_name_of_key(name))).is_some_and(|ext| is_snes_ext(&ext))
}

fn is_snes_ext(ext: &str) -> bool {
    matches!(ext, "smc" | "sfc" | "swc" | "fig")
}

/// 验 NKit 要读到哪儿：光盘逻辑偏移 `0x200` 处的 4 字节。
pub const NKIT_PROBE_LEN: usize = 0x204;

/// 这份内容是不是 NKit 处理过的镜像。
///
/// 两条判据，任一成立即算：名字里写着 `.nkit.`（NKit 自己的命名），或者偏移 `0x200`
/// 处是 `NKIT`（Dolphin `VolumeDisc::IsNKit()`）。
#[must_use]
pub fn is_nkit(name: &str, head: &[u8]) -> bool {
    if file_name_of_key(name)
        .to_ascii_lowercase()
        .contains(".nkit.")
    {
        return true;
    }
    head.get(0x200..0x204) == Some(b"NKIT".as_slice())
}

/// 这份内容该不该在撞 CRC **之前**先验一遍 NKit。
///
/// 只有 GC / Wii 的镜像会被 NKit 处理，验一遍要回盘读 `0x204` 字节。整库无差别地验
/// 一遍等于把这笔读盘摊到几万个与 NKit 毫无关系的条目上，因此按平台与扩展名先筛。
#[must_use]
pub fn may_be_nkit(platform: Option<&str>, name: &str) -> bool {
    let file = file_name_of_key(name).to_ascii_lowercase();
    if file.contains(".nkit.") {
        return true;
    }
    let disc_image = extension_lower(Path::new(file.as_str()))
        .is_some_and(|ext| matches!(ext.as_str(), "iso" | "gcm" | "img"));
    disc_image && matches!(platform, Some("NGC" | "WII"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 外挂头的名字能来回折() {
        // 中立库里存的是这个名字，读回来认不出就只能当「没看过」重算——
        // 绝不能猜一个头顶上。
        for header in DumpHeader::all() {
            assert_eq!(DumpHeader::from_label(header.label()), Some(header));
        }
        assert_eq!(DumpHeader::from_label("拷贝机头"), None);
    }

    #[test]
    fn 五种外挂头各认得出来() {
        assert_eq!(
            dump_header("x.nes", b"NES\x1A\x02\x01", 40_976),
            Some(DumpHeader::INes)
        );
        assert_eq!(
            dump_header("x.fds", b"FDS\x1A\x01", 65_516),
            Some(DumpHeader::Fds)
        );
        assert_eq!(
            dump_header("x.lnx", b"LYNX\0\0", 131_136),
            Some(DumpHeader::Lynx)
        );
        let mut a78 = vec![0u8; 16];
        a78[1..10].copy_from_slice(b"ATARI7800");
        assert_eq!(
            dump_header("x.a78", &a78, 32_896),
            Some(DumpHeader::Atari7800)
        );
    }

    #[test]
    fn 拷贝机头没有魔数只能看总长的余数() {
        // 512 + 512 KiB：带头。
        assert_eq!(
            dump_header("x.smc", &[0u8; 16], 524_800),
            Some(DumpHeader::SnesCopier)
        );
        // 正好 512 KiB：没头。同样的字节、同样的扩展名，只有总长不同。
        assert_eq!(dump_header("x.smc", &[0u8; 16], 524_288), None);
        // 扩展名不是 SFC 家族的，余数再对也不算——那是别的平台的文件恰好这么大。
        assert_eq!(dump_header("x.gba", &[0u8; 16], 524_800), None);
    }

    #[test]
    fn 认不出头时宁可不剥() {
        // 多剥一次会算出一个谁也对不上的哈希，少剥一次最多少一条候选。
        assert_eq!(dump_header("x.nes", b"\x00\x01\x02\x03", 40_960), None);
        assert_eq!(dump_header("x.bin", b"", 0), None);
    }

    #[test]
    fn 值不值得为去头哈希回盘读一次() {
        assert!(may_have_header("FC/游戏.nes"));
        assert!(may_have_header("SFC/游戏.smc"));
        assert!(!may_have_header("PS1/游戏.bin"));
        assert!(!may_have_header("没有扩展名"));
    }

    #[test]
    fn 尺寸自己就能排掉绝大多数() {
        // 容器里的条目看不见字节，尺寸是唯一免费的判据。
        assert!(size_suggests_header("x.nes", 40_976), "16 + 40 KiB");
        assert!(
            !size_suggests_header("x.nes", 40_960),
            "整齐的 40 KiB，没有头"
        );
        assert!(size_suggests_header("x.smc", 524_800), "512 + 512 KiB");
        assert!(!size_suggests_header("x.smc", 524_288));
        assert!(size_suggests_header("x.fds", 65_516), "16 + 一面");
        assert!(size_suggests_header("x.fds", 131_016), "16 + 两面");
        assert!(!size_suggests_header("x.fds", 65_500));
        assert!(size_suggests_header("x.lnx", 131_136), "64 + 128 KiB");
        assert!(
            !size_suggests_header("x.gba", 40_976),
            "GBA 没有外挂头这一说"
        );
    }

    #[test]
    fn nkit_两条判据各自成立() {
        let mut head = vec![0u8; NKIT_PROBE_LEN];
        head[0x200..0x204].copy_from_slice(b"NKIT");
        assert!(is_nkit("WII/游戏.iso", &head), "偏移 0x200 处的魔数");
        assert!(is_nkit("WII/游戏.nkit.iso", &[]), "名字里就写着");
        assert!(!is_nkit("WII/游戏.iso", &[0u8; NKIT_PROBE_LEN]));
    }

    #[test]
    fn 只给_gc_与_wii_的镜像付这笔读盘() {
        assert!(may_be_nkit(Some("WII"), "WII/游戏.iso"));
        assert!(may_be_nkit(Some("NGC"), "NGC/游戏.gcm"));
        // PS2 的 iso 一样是镜像，但 NKit 不处理它。
        assert!(!may_be_nkit(Some("PS2"), "PS2/游戏.iso"));
        // 名字里写着的，哪个平台都验。
        assert!(may_be_nkit(None, "别处/游戏.nkit.iso"));
        assert!(!may_be_nkit(Some("WII"), "WII/游戏.7z"));
    }
}
