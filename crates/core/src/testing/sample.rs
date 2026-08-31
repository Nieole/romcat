//! 带真实魔数的样本字节。
//!
//! 探针测试、内存主库、真实 fixture 主库都用同一份构造函数——三处各写一遍，
//! 迟早会出现「这边的 zip 头对、那边的不对」。

/// 一个 zip **透明容器**的开头，其余补零到 `len` 字节。
#[must_use]
pub fn zip(len: usize) -> Vec<u8> {
    let mut data = vec![0u8; len.max(4)];
    data[..4].copy_from_slice(b"PK\x03\x04");
    data
}

/// 一个 CHD v5 **压缩镜像**的头。
#[must_use]
pub fn chd() -> Vec<u8> {
    let mut data = vec![0u8; 128];
    data[..8].copy_from_slice(b"MComprHD");
    data[12..16].copy_from_slice(&5u32.to_be_bytes());
    data
}

/// 一个带 ISO9660 主卷描述符的光盘镜像（`CD001` 在 0x8001）。
#[must_use]
pub fn iso() -> Vec<u8> {
    let mut data = vec![0u8; 0x8806];
    data[0x8001..0x8006].copy_from_slice(b"CD001");
    data
}

/// 一个 iNES 卡带。
#[must_use]
pub fn nes() -> Vec<u8> {
    let mut data = vec![0u8; 40976];
    data[..4].copy_from_slice(b"NES\x1a");
    data[4] = 2;
    data[5] = 1;
    data
}

/// 一个 GBA 卡带：0x04 处的 Nintendo logo 开头、0xB2 处的 0x96、0xAC 处的 game code。
#[must_use]
pub fn gba(game_code: &[u8; 4]) -> Vec<u8> {
    let mut data = vec![0u8; 0x200];
    data[0x04..0x08].copy_from_slice(&[0x24, 0xFF, 0xAE, 0x51]);
    data[0xAC..0xB0].copy_from_slice(game_code);
    data[0xB2] = 0x96;
    data
}

/// 一个 NDS 卡带：0x0C 处的 gamecode、0x15C 处的 logo CRC 0xCF56。
#[must_use]
pub fn nds(game_code: &[u8; 4]) -> Vec<u8> {
    let mut data = vec![0u8; 0x200];
    data[0x0C..0x10].copy_from_slice(game_code);
    data[0x15C..0x15E].copy_from_slice(&[0x56, 0xCF]);
    data
}
