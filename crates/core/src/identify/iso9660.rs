//! **ISO9660**：从光盘的前几百 KB 里把一个具体文件找出来。
//!
//! 这一层只回答一个问题：**`SYSTEM.CNF` / `PSP_GAME/PARAM.SFO` 在哪、有多长**。
//! 它不是一个文件系统实现——不认 Joliet、不认 Rock Ridge、不认多扩展区的文件，
//! 也从不递归整棵树。理由是这张票的全部意义：**识别一个 8 GB 的 ISO 不需要读 8 GB**，
//! 而那几个文件躺在盘的最前面。
//!
//! 布局（ECMA-119；DuckStation `IsoReader`、PPSSPP `ISOFileSystem` 与
//! `docs/research/rom-identification.md` A.11–A.13 都按这一份读）：
//!
//! - **主卷描述符**在逻辑扇区 16，即偏移 `0x8000`；`0x8000` 处是类型字节 `0x01`，
//!   `0x8001` 起五字节是标识串 `CD001`。
//! - 主卷描述符 `+156` 起 34 字节是**根目录记录**。
//! - 一条目录记录：`+0` 记录长度（`0` 表示这一扇区到头了，跳到下一扇区）、
//!   `+2` 扩展区起始扇区（双字节序，小端在前）、`+10` 数据长度（同）、
//!   `+25` 标志位（bit 1 是目录）、`+32` 文件名长度、`+33` 文件名。
//! - 文件名带 `;1` 版本后缀，比较时要剥掉。
//!
//! ## 只在手上这一段字节里找
//!
//! 调用方交上来的是**逻辑光盘的前若干字节**，不是整张盘。找到的文件超出这一段时
//! 返回 [`Found::TooFar`]——那和「盘里没有这个文件」是两件事，报告要分得开：前者
//! 加大前缀就读得到，后者加多少都没用。

use std::ops::Range;

/// 逻辑扇区大小。
pub const SECTOR: usize = 2048;

/// 主卷描述符在第几个扇区。
pub const PVD_SECTOR: usize = 16;

/// 主卷描述符的偏移。
pub const PVD_OFFSET: usize = PVD_SECTOR * SECTOR;

/// 主卷描述符里那块 512 字节的「Application Used」区的偏移（相对扇区起点）。
///
/// PSP 的 UMD 把 `ULUS-10339|…` 这样一串**明文**写在它的 `0x373` 处
/// （调研 A.13.3 的「免解析」那一行）。那是整条链上最便宜的一条：不必走目录树、
/// 不必解析 SFO，读到 `0x8373` 就有。
pub const APPLICATION_USE: usize = 0x373;

/// 找一个文件的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// 找到了，内容在这一段里。
    At(Range<usize>),
    /// 目录记录读到了，但内容落在手上这段字节之外。
    TooFar {
        /// 内容从哪儿开始。
        at: u64,
    },
    /// 盘里没有这个文件。
    Missing,
}

/// 这段字节的开头是不是一张 ISO9660 光盘。
#[must_use]
pub fn is_iso9660(image: &[u8]) -> bool {
    image.get(PVD_OFFSET) == Some(&0x01)
        && image.get(PVD_OFFSET + 1..PVD_OFFSET + 6) == Some(b"CD001")
}

/// 主卷描述符里那块「Application Used」区。
#[must_use]
pub fn application_use(image: &[u8]) -> Option<&[u8]> {
    if !is_iso9660(image) {
        return None;
    }
    image.get(PVD_OFFSET + APPLICATION_USE..PVD_OFFSET + APPLICATION_USE + 512)
}

/// 按路径找一个文件。`path` 是逐级的名字，大小写不敏感，不带 `;1` 后缀。
#[must_use]
pub fn find(image: &[u8], path: &[&str]) -> Found {
    if !is_iso9660(image) || path.is_empty() {
        return Found::Missing;
    }
    // 根目录记录：主卷描述符 +156。
    let Some(root) = image.get(PVD_OFFSET + 156..PVD_OFFSET + 156 + 34) else {
        return Found::Missing;
    };
    let (mut at, mut len) = match extent_of(root) {
        Some(pair) => pair,
        None => return Found::Missing,
    };
    for (depth, want) in path.iter().enumerate() {
        let last = depth + 1 == path.len();
        match entry_in(image, at, len, want) {
            Some((child_at, child_len, is_dir)) if is_dir != last => {
                at = child_at;
                len = child_len;
            }
            // 最后一级必须是文件、前面几级必须是目录。对不上就是没找到——
            // 认下一个同名的目录当文件，接下来读出来的是目录记录不是内容。
            Some(_) => return Found::Missing,
            None => {
                return if at.saturating_add(len) > image.len() as u64 {
                    Found::TooFar { at }
                } else {
                    Found::Missing
                };
            }
        }
        if last {
            let start = usize::try_from(at).unwrap_or(usize::MAX);
            let end = start.saturating_add(usize::try_from(len).unwrap_or(usize::MAX));
            return match image.get(start..end) {
                Some(_) => Found::At(start..end),
                None => Found::TooFar { at },
            };
        }
    }
    Found::Missing
}

/// 在一个目录的扩展区里找一条名字。返回 `(内容偏移, 长度, 是不是目录)`。
fn entry_in(image: &[u8], at: u64, len: u64, want: &str) -> Option<(u64, u64, bool)> {
    let start = usize::try_from(at).ok()?;
    let end = start.checked_add(usize::try_from(len).ok()?)?;
    let region = image.get(start..end.min(image.len()))?;
    let mut cursor = 0usize;
    while cursor < region.len() {
        let record_len = usize::from(*region.get(cursor)?);
        if record_len == 0 {
            // 目录记录不跨扇区：这一扇区剩下的是填充，跳到下一扇区继续。
            cursor = (cursor / SECTOR + 1) * SECTOR;
            continue;
        }
        let record = region.get(cursor..cursor.checked_add(record_len)?)?;
        cursor += record_len;
        let Some((child_at, child_len)) = extent_of(record) else {
            continue;
        };
        let is_dir = record.get(25).is_some_and(|flags| flags & 0x02 != 0);
        let name_len = usize::from(*record.get(32)?);
        let Some(raw) = record.get(33..33 + name_len) else {
            continue;
        };
        // `.` 与 `..` 是单字节 0x00 / 0x01，不是名字。
        if matches!(raw, [0x00] | [0x01]) {
            continue;
        }
        let name = String::from_utf8_lossy(raw);
        let name = name.split(';').next().unwrap_or("").trim_end_matches('.');
        if name.eq_ignore_ascii_case(want) {
            return Some((child_at, child_len, is_dir));
        }
    }
    None
}

/// 一条目录记录的 `(内容偏移, 长度)`；扇区号乘回字节。
fn extent_of(record: &[u8]) -> Option<(u64, u64)> {
    let lba = le_u32(record, 2)?;
    let len = le_u32(record, 10)?;
    Some((u64::from(lba) * SECTOR as u64, u64::from(len)))
}

fn le_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let raw = bytes.get(at..at + 4)?;
    Some(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一张最小的 ISO9660：主卷描述符 + 根目录一个扇区 + 一个文件。
    fn 小盘(entries: &[(&str, bool, u32, u32)], content: &[(u32, &[u8])]) -> Vec<u8> {
        let mut image = vec![0u8; 64 * SECTOR];
        image[PVD_OFFSET] = 0x01;
        image[PVD_OFFSET + 1..PVD_OFFSET + 6].copy_from_slice(b"CD001");
        // 根目录记录指向扇区 20，长一个扇区。
        let root = PVD_OFFSET + 156;
        image[root] = 34;
        image[root + 2..root + 6].copy_from_slice(&20u32.to_le_bytes());
        image[root + 10..root + 14].copy_from_slice(&(SECTOR as u32).to_le_bytes());
        let mut at = 20 * SECTOR;
        for (name, is_dir, lba, len) in entries {
            let record_len = 33 + name.len() + usize::from(name.len() % 2 == 0);
            image[at] = u8::try_from(record_len).expect("短");
            image[at + 2..at + 6].copy_from_slice(&lba.to_le_bytes());
            image[at + 10..at + 14].copy_from_slice(&len.to_le_bytes());
            image[at + 25] = u8::from(*is_dir) << 1;
            image[at + 32] = u8::try_from(name.len()).expect("短");
            image[at + 33..at + 33 + name.len()].copy_from_slice(name.as_bytes());
            at += record_len;
        }
        for (lba, bytes) in content {
            let from = *lba as usize * SECTOR;
            image[from..from + bytes.len()].copy_from_slice(bytes);
        }
        image
    }

    #[test]
    fn 根目录下的文件找得到() {
        let image = 小盘(
            &[("SYSTEM.CNF;1", false, 24, 40)],
            &[(24, b"BOOT = cdrom:\\SLPS_021.70;1\r\n")],
        );
        assert!(is_iso9660(&image));
        let Found::At(range) = find(&image, &["SYSTEM.CNF"]) else {
            panic!("该找得到");
        };
        assert!(image[range].starts_with(b"BOOT = cdrom:"));
    }

    #[test]
    fn 名字大小写与版本后缀都不影响() {
        let image = 小盘(&[("SYSTEM.CNF;1", false, 24, 8)], &[(24, b"12345678")]);
        assert!(matches!(find(&image, &["system.cnf"]), Found::At(_)));
    }

    #[test]
    fn 一级目录走得下去() {
        let mut image = 小盘(&[("PSP_GAME", true, 30, SECTOR as u32)], &[]);
        // 在扇区 30 放一条子目录记录。
        let at = 30 * SECTOR;
        let name = b"PARAM.SFO;1";
        image[at] = u8::try_from(33 + name.len() + 1).expect("短");
        image[at + 2..at + 6].copy_from_slice(&35u32.to_le_bytes());
        image[at + 10..at + 14].copy_from_slice(&4u32.to_le_bytes());
        image[at + 32] = u8::try_from(name.len()).expect("短");
        image[at + 33..at + 33 + name.len()].copy_from_slice(name);
        image[35 * SECTOR..35 * SECTOR + 4].copy_from_slice(b"\0PSF");
        let Found::At(range) = find(&image, &["PSP_GAME", "PARAM.SFO"]) else {
            panic!("该找得到");
        };
        assert_eq!(&image[range], b"\0PSF");
    }

    #[test]
    fn 目录当文件走不下去() {
        // 最后一级是目录、中间一级是文件，两种都该判「没找到」而不是硬读。
        let image = 小盘(&[("PSP_GAME", true, 30, 2048)], &[]);
        assert_eq!(find(&image, &["PSP_GAME"]), Found::Missing);
    }

    #[test]
    fn 内容超出手上这段字节时说得出是哪一种() {
        // 「读得到目录记录、读不到内容」与「盘里没有这个文件」是两件事：
        // 前者加大前缀就读得到，后者加多少都没用。
        let image = 小盘(&[("BIG.BIN;1", false, 10_000, 4096)], &[]);
        assert_eq!(find(&image, &["BIG.BIN"]), Found::TooFar { at: 20_480_000 });
        assert_eq!(find(&image, &["NOPE.BIN"]), Found::Missing);
    }

    #[test]
    fn 不是_iso9660_的字节一律不认() {
        assert!(!is_iso9660(&[0u8; 8]));
        assert!(!is_iso9660(&vec![0u8; 64 * SECTOR]));
        assert_eq!(find(&[0u8; 8], &["SYSTEM.CNF"]), Found::Missing);
    }

    #[test]
    fn 应用保留区取得出来() {
        let mut image = 小盘(&[], &[]);
        image[PVD_OFFSET + APPLICATION_USE..PVD_OFFSET + APPLICATION_USE + 11]
            .copy_from_slice(b"ULUS-10339|");
        assert!(
            application_use(&image)
                .expect("有")
                .starts_with(b"ULUS-10339|")
        );
    }
}
