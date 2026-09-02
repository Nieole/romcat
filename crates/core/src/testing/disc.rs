//! 造一份**光盘形态**的样本字节：ISO9660、PARAM.SFO、PBP、CSO、WBFS、GC/Wii 光盘头。
//!
//! 单元测试与集成测试都要用同一份构造函数——两处各写一遍，迟早会出现「这边的
//! 主卷描述符对、那边的不对」，而那种错读起来像是解析器坏了。
//!
//! 造出来的东西都是**最小可信形态**：够 [`identify::disc`](crate::identify::disc) 认出来，
//! 不追求能被真正的模拟器加载。

/// 逻辑扇区大小。**与解析器共用同一个常量**——两处各写一个 2048，改一次就会有一边
/// 造出解析器读不懂的样本，而那种错读起来像是解析器坏了。
use crate::identify::iso9660::SECTOR;

/// 一张 PARAM.SFO。`entries` 是 `(键, 值)`，一律按 utf8 存。
///
/// 键表按字母序存是规范要求的，但**这里不排序**：解析器不该依赖顺序，
/// 而一份乱序的 SFO 在真库里是可能的。
#[must_use]
pub fn param_sfo(entries: &[(&str, &str)]) -> Vec<u8> {
    let key_start = 0x14 + entries.len() * 0x10;
    let key_bytes: usize = entries.iter().map(|(key, _)| key.len() + 1).sum();
    // 数据表 4 字节对齐，与 vitasdk 的 `vita-mksfoex.c` 一致。
    let data_start = (key_start + key_bytes).div_ceil(4) * 4;

    let mut index = Vec::new();
    let mut keys = Vec::new();
    let mut data = Vec::new();
    for (key, value) in entries {
        let len = u32::try_from(value.len() + 1).expect("样本里的值不会那么长");
        index.extend_from_slice(&u16::try_from(keys.len()).expect("短").to_le_bytes());
        index.extend_from_slice(&0x0204u16.to_le_bytes());
        index.extend_from_slice(&len.to_le_bytes());
        index.extend_from_slice(&len.to_le_bytes());
        index.extend_from_slice(&u32::try_from(data.len()).expect("短").to_le_bytes());
        keys.extend_from_slice(key.as_bytes());
        keys.push(0);
        data.extend_from_slice(value.as_bytes());
        data.push(0);
    }

    let mut out = Vec::new();
    out.extend_from_slice(b"\0PSF");
    out.extend_from_slice(&[0x01, 0x01, 0x00, 0x00]);
    out.extend_from_slice(&u32::try_from(key_start).expect("短").to_le_bytes());
    out.extend_from_slice(&u32::try_from(data_start).expect("短").to_le_bytes());
    out.extend_from_slice(&u32::try_from(entries.len()).expect("短").to_le_bytes());
    out.extend_from_slice(&index);
    out.extend_from_slice(&keys);
    out.resize(data_start, 0);
    out.extend_from_slice(&data);
    out
}

/// 一份 PBP：`param_sfo_offset` 指向 `0x28`，那儿的 `PARAM.SFO` **未压缩**。
#[must_use]
pub fn pbp(sfo: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; 0x28];
    out[..4].copy_from_slice(b"\0PBP");
    out[4..8].copy_from_slice(&1u32.to_le_bytes());
    out[8..12].copy_from_slice(&0x28u32.to_le_bytes());
    out.extend_from_slice(sfo);
    out
}

/// 一张最小的 ISO9660 光盘：主卷描述符 + 一层根目录 + 若干文件。
///
/// `files` 是 `(路径, 内容)`，路径最多两级（`SYSTEM.CNF`、`PSP_GAME/PARAM.SFO`）——
/// 真正要找的那几个文件就这两种形状。
#[must_use]
pub fn iso9660(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    // 布局：扇区 16 主卷描述符、扇区 20 根目录、扇区 21 起子目录、扇区 32 起文件内容。
    let mut image = vec![0u8; 256 * SECTOR];
    image[16 * SECTOR] = 0x01;
    image[16 * SECTOR + 1..16 * SECTOR + 6].copy_from_slice(b"CD001");
    let root = 16 * SECTOR + 156;
    image[root] = 34;
    image[root + 2..root + 6].copy_from_slice(&20u32.to_le_bytes());
    image[root + 10..root + 14].copy_from_slice(&(SECTOR as u32).to_le_bytes());

    let mut root_at = 20 * SECTOR;
    let mut dir_sector = 21u32;
    let mut content_sector = 32u32;
    // 同一个目录名出现两次时共用一个扇区。
    let mut dirs: Vec<(String, u32, usize)> = Vec::new();
    for (path, bytes) in files {
        let at = content_sector as usize * SECTOR;
        image[at..at + bytes.len()].copy_from_slice(bytes);
        let len = u32::try_from(bytes.len()).expect("样本不会那么大");
        match path.split_once('/') {
            None => {
                root_at = record(&mut image, root_at, path, false, content_sector, len);
            }
            Some((dir, file)) => {
                let slot = match dirs.iter().position(|(name, ..)| name == dir) {
                    Some(index) => index,
                    None => {
                        root_at = record(
                            &mut image,
                            root_at,
                            dir,
                            true,
                            dir_sector,
                            u32::try_from(SECTOR).expect("小"),
                        );
                        dirs.push((dir.to_string(), dir_sector, dir_sector as usize * SECTOR));
                        dir_sector += 1;
                        dirs.len() - 1
                    }
                };
                let cursor = dirs[slot].2;
                dirs[slot].2 = record(&mut image, cursor, file, false, content_sector, len);
            }
        }
        content_sector += u32::try_from(bytes.len().div_ceil(SECTOR).max(1)).expect("小");
    }
    image
}

/// 往目录扩展区里写一条记录，返回下一条该写在哪。
fn record(image: &mut [u8], at: usize, name: &str, is_dir: bool, lba: u32, len: u32) -> usize {
    // 文件带 `;1` 版本后缀，目录不带（ECMA-119）。
    let name = if is_dir {
        name.to_uppercase()
    } else {
        format!("{};1", name.to_uppercase())
    };
    let record_len = 33 + name.len() + usize::from(name.len() % 2 == 0);
    image[at] = u8::try_from(record_len).expect("名字不会那么长");
    image[at + 2..at + 6].copy_from_slice(&lba.to_le_bytes());
    image[at + 10..at + 14].copy_from_slice(&len.to_le_bytes());
    image[at + 25] = u8::from(is_dir) << 1;
    image[at + 32] = u8::try_from(name.len()).expect("短");
    image[at + 33..at + 33 + name.len()].copy_from_slice(name.as_bytes());
    at + record_len
}

/// 往一张 ISO9660 的主卷描述符里写 PSP 那串明文序列号（Application Used 区，`0x373`）。
pub fn with_psp_application_use(image: &mut [u8], text: &str) {
    let at = 16 * SECTOR + 0x373;
    image[at..at + text.len()].copy_from_slice(text.as_bytes());
}

/// 一份 GC 或 Wii 的光盘头（前 `0x400` 字节）。`wii` 为真时写 Wii 的魔数。
#[must_use]
pub fn disc_header(id: &str, title: &str, wii: bool) -> Vec<u8> {
    let mut out = vec![0u8; 0x440];
    out[..id.len().min(6)].copy_from_slice(&id.as_bytes()[..id.len().min(6)]);
    if wii {
        out[0x18..0x1C].copy_from_slice(&[0x5D, 0x1C, 0x9E, 0xA3]);
    } else {
        out[0x1C..0x20].copy_from_slice(&[0xC2, 0x33, 0x9F, 0x3D]);
    }
    out[0x20..0x20 + title.len()].copy_from_slice(title.as_bytes());
    out
}

/// 一份 WBFS：`hd_sector_shift` 为 9，Wii 光盘头的 256 字节从 `0x200` 起。
#[must_use]
pub fn wbfs(disc: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; 0x300];
    out[..4].copy_from_slice(b"WBFS");
    out[8] = 9;
    out[9] = 21;
    let take = disc.len().min(256);
    out[0x200..0x200 + take].copy_from_slice(&disc[..take]);
    out
}

/// 一份 RVZ：光盘头的 `0x80` 字节原样躺在文件偏移 `0x58`。
#[must_use]
pub fn rvz(disc: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; 0x400];
    out[..4].copy_from_slice(b"RVZ\x01");
    let take = disc.len().min(0x80);
    out[0x58..0x58 + take].copy_from_slice(&disc[..take]);
    out
}

/// 一份 v1 的 CSO：每块 2048 字节，全部原样存（高位置 1）。
///
/// 不压是故意的——这个构造函数要证的是**索引读得对**，而不是 deflate 能不能解开
/// （那有 `disc.rs` 自己的单元测试）。
#[must_use]
pub fn cso(plain: &[u8]) -> Vec<u8> {
    let block = SECTOR;
    let blocks = plain.len().div_ceil(block);
    let data_at = 0x18 + (blocks + 1) * 4;
    let mut out = vec![0u8; data_at];
    out[..4].copy_from_slice(b"CISO");
    out[4..8].copy_from_slice(&0x18u32.to_le_bytes());
    out[8..16].copy_from_slice(&(plain.len() as u64).to_le_bytes());
    out[16..20].copy_from_slice(&u32::try_from(block).expect("小").to_le_bytes());
    out[20] = 1;
    for index in 0..=blocks {
        let at = data_at + index * block;
        let entry = u32::try_from(at).expect("样本不会那么大") | 0x8000_0000;
        out[0x18 + index * 4..0x18 + index * 4 + 4].copy_from_slice(&entry.to_le_bytes());
    }
    out.extend_from_slice(plain);
    out.resize(data_at + blocks * block, 0);
    out
}
