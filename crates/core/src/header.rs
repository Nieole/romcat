//! 头部抽样解析。
//!
//! 体检报告要回答的问题是：**在写完整识别器之前，头部解析在真实文件上到底能不能用。**
//! 因此这里只做「按已知偏移看魔数、看校验和」这一层，不做识别——识别是票 07 之后的事。
//! 每类文件抽样若干个，报告解析成功率；失败的样本连路径一起报出来，好去看是解析器错了
//! 还是文件本身就不是那个东西。
//!
//! 偏移与魔数的出处记在各个探针的注释里。没有内部头的格式（`.lyx`、`.pce` 等）不参与
//! 抽样，它们的解析成功率无从谈起。

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::path::extension_lower;

/// 一类可以抽样探测的文件。类是按**格式家族**分的，不是按扩展名——`.z64` 与 `.n64`
/// 是同一个探针的两种字节序。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProbeClass {
    /// zip 透明容器。
    Zip,
    /// 7z 透明容器。
    SevenZip,
    /// rar 透明容器。
    Rar,
    /// zstd 透明容器。
    Zstd,
    /// CHD 压缩镜像。
    Chd,
    /// Wii U 的 WUX 压缩镜像。
    Wux,
    /// CSO / ZSO / DAX / JISO 压缩镜像。
    CompressedIso,
    /// PSP 的 PBP。
    Pbp,
    /// WIA / RVZ 压缩镜像。
    RvzWia,
    /// WBFS 压缩镜像。
    Wbfs,
    /// GCZ 压缩镜像。
    Gcz,
    /// 未压缩光盘镜像（iso / gcm / wud / img）。
    DiscImage,
    /// `.bin`：可能是光盘轨道，也可能是 MD 卡带。
    BinTrack,
    /// `.cue` 表单。
    Cue,
    /// FC / NES 卡带。
    Nes,
    /// SFC / SNES 卡带。
    Snes,
    /// GB / GBC 卡带。
    GameBoy,
    /// GBA 卡带。
    Gba,
    /// NDS 卡带。
    Nds,
    /// N64 卡带。
    N64,
    /// MD / Genesis 卡带。
    Genesis,
    /// SMS / GG 卡带。
    MasterSystem,
    /// Lynx 卡带。
    Lynx,
    /// NGP / NGPC 卡带。
    NeoGeoPocket,
    /// WS / WSC 卡带（内部头在文件末尾）。
    WonderSwan,
    /// 3DS 的 CCI / NCCH。
    Nintendo3ds,
    /// 3DS 的 CIA。
    Cia,
}

impl ProbeClass {
    /// 报告里用的类名。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Zip => "zip 透明容器",
            Self::SevenZip => "7z 透明容器",
            Self::Rar => "rar 透明容器",
            Self::Zstd => "zst 透明容器",
            Self::Chd => "chd 压缩镜像",
            Self::Wux => "wux 压缩镜像",
            Self::CompressedIso => "cso/zso/dax 压缩镜像",
            Self::Pbp => "pbp 压缩镜像",
            Self::RvzWia => "rvz/wia 压缩镜像",
            Self::Wbfs => "wbfs 压缩镜像",
            Self::Gcz => "gcz 压缩镜像",
            Self::DiscImage => "光盘镜像 iso/gcm/wud",
            Self::BinTrack => "bin（光盘轨道或 MD 卡带）",
            Self::Cue => "cue 表单",
            Self::Nes => "FC 卡带",
            Self::Snes => "SFC 卡带",
            Self::GameBoy => "GB/GBC 卡带",
            Self::Gba => "GBA 卡带",
            Self::Nds => "NDS 卡带",
            Self::N64 => "N64 卡带",
            Self::Genesis => "MD 卡带",
            Self::MasterSystem => "SMS/GG 卡带",
            Self::Lynx => "Lynx 卡带",
            Self::NeoGeoPocket => "NGP/NGPC 卡带",
            Self::WonderSwan => "WS/WSC 卡带",
            Self::Nintendo3ds => "3DS 的 cci/ncch",
            Self::Cia => "3DS 的 cia",
        }
    }

    /// 探测这一类需要读的头部字节数。
    #[must_use]
    pub fn head_len(self) -> usize {
        match self {
            // SFC 的内部头在 0x7FC0 / 0xFFC0，还可能被 512 字节拷贝机头整体后移。
            Self::Snes => 0x1_0220,
            // ISO9660 的主卷描述符在 0x8000，`CD001` 在 0x8001。
            Self::DiscImage | Self::BinTrack => 0x8806,
            // SMS 的 `TMR SEGA` 可能在 0x1FF0 / 0x3FF0 / 0x7FF0。
            Self::MasterSystem => 0x8000,
            Self::Cue => 0x400,
            _ => 0x200,
        }
    }

    /// 探测这一类需要读的尾部字节数。0 表示不读尾部。
    #[must_use]
    pub fn tail_len(self) -> usize {
        match self {
            Self::WonderSwan => 16,
            _ => 0,
        }
    }
}

/// 按扩展名决定用哪个探针。返回 `None` 表示这类文件不参与头部抽样。
#[must_use]
pub fn probe_class_for(path: &Path) -> Option<ProbeClass> {
    let ext = extension_lower(path)?;
    Some(match ext.as_str() {
        "zip" | "cbz" => ProbeClass::Zip,
        "zst" => ProbeClass::Zstd,
        "7z" => ProbeClass::SevenZip,
        "rar" => ProbeClass::Rar,
        "chd" => ProbeClass::Chd,
        "wux" => ProbeClass::Wux,
        "cso" | "ciso" | "zso" | "dax" | "jso" => ProbeClass::CompressedIso,
        "pbp" => ProbeClass::Pbp,
        "rvz" | "wia" => ProbeClass::RvzWia,
        "wbfs" => ProbeClass::Wbfs,
        "gcz" => ProbeClass::Gcz,
        "iso" | "gcm" | "wud" | "img" => ProbeClass::DiscImage,
        "bin" => ProbeClass::BinTrack,
        "cue" => ProbeClass::Cue,
        "nes" | "unf" | "unif" => ProbeClass::Nes,
        "sfc" | "smc" | "swc" | "fig" => ProbeClass::Snes,
        "gb" | "gbc" | "sgb" => ProbeClass::GameBoy,
        "gba" | "agb" => ProbeClass::Gba,
        "nds" | "dsi" | "srl" => ProbeClass::Nds,
        "n64" | "z64" | "v64" => ProbeClass::N64,
        "md" | "gen" | "smd" | "32x" => ProbeClass::Genesis,
        "sms" | "gg" | "sg" => ProbeClass::MasterSystem,
        "lnx" => ProbeClass::Lynx,
        "ngp" | "ngc" => ProbeClass::NeoGeoPocket,
        "ws" | "wsc" => ProbeClass::WonderSwan,
        "3ds" | "cci" | "cxi" => ProbeClass::Nintendo3ds,
        "cia" => ProbeClass::Cia,
        _ => return None,
    })
}

/// 一次抽样探测的结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// 解析成功，附带认出来的形态。
    Parsed(String),
    /// 头部读到了，但对不上这一类应有的结构。
    Mismatch(String),
    /// 文件太短，够不到该看的偏移。
    TooShort,
}

impl ProbeOutcome {
    /// 是否解析成功。
    #[must_use]
    pub fn is_parsed(&self) -> bool {
        matches!(self, Self::Parsed(_))
    }
}

fn slice_at(buf: &[u8], offset: usize, len: usize) -> Option<&[u8]> {
    buf.get(offset..offset.checked_add(len)?)
}

fn matches_at(buf: &[u8], offset: usize, magic: &[u8]) -> bool {
    slice_at(buf, offset, magic.len()) == Some(magic)
}

fn parsed(text: impl Into<String>) -> ProbeOutcome {
    ProbeOutcome::Parsed(text.into())
}

fn mismatch(text: impl Into<String>) -> ProbeOutcome {
    ProbeOutcome::Mismatch(text.into())
}

fn ascii_field(buf: &[u8], offset: usize, len: usize) -> Option<String> {
    let raw = slice_at(buf, offset, len)?;
    if raw.iter().all(|b| (0x20..0x7F).contains(b)) {
        Some(String::from_utf8_lossy(raw).trim().to_string())
    } else {
        None
    }
}

fn magic_prefix(head: &[u8]) -> String {
    let shown: String = head
        .iter()
        .take(8)
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ");
    if shown.is_empty() {
        "（空文件）".to_string()
    } else {
        shown
    }
}

/// 对一个样本做头部探测。
///
/// `head` 是文件头部、`tail` 是文件尾部（按 [`ProbeClass::head_len`] 与
/// [`ProbeClass::tail_len`] 读到的），`len` 是文件总长度。
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn probe(class: ProbeClass, head: &[u8], tail: &[u8], len: u64) -> ProbeOutcome {
    match class {
        ProbeClass::Zip => {
            if matches_at(head, 0, b"PK\x03\x04") {
                parsed("zip 本地文件头")
            } else if matches_at(head, 0, b"PK\x05\x06") {
                parsed("zip 空容器")
            } else if matches_at(head, 0, b"PK\x07\x08") {
                parsed("zip 分卷标记")
            } else {
                mismatch(format!("不是 PK 开头：{}", magic_prefix(head)))
            }
        }
        ProbeClass::SevenZip => {
            if matches_at(head, 0, &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
                parsed("7z 签名头")
            } else {
                mismatch(format!("7z 签名对不上：{}", magic_prefix(head)))
            }
        }
        ProbeClass::Rar => {
            if matches_at(head, 0, b"Rar!\x1a\x07\x01\x00") {
                parsed("RAR5")
            } else if matches_at(head, 0, b"Rar!\x1a\x07\x00") {
                parsed("RAR4")
            } else {
                mismatch(format!("RAR 签名对不上：{}", magic_prefix(head)))
            }
        }
        // CHD V5 头部：magic `MComprHD`，版本号在偏移 12（大端）。
        ProbeClass::Chd => {
            if !matches_at(head, 0, b"MComprHD") {
                return mismatch(format!("CHD 签名对不上：{}", magic_prefix(head)));
            }
            match slice_at(head, 12, 4) {
                Some(v) => parsed(format!(
                    "CHD v{}",
                    u32::from_be_bytes([v[0], v[1], v[2], v[3]])
                )),
                None => ProbeOutcome::TooShort,
            }
        }
        ProbeClass::CompressedIso => {
            if matches_at(head, 0, b"CISO") {
                parsed("CISO")
            } else if matches_at(head, 0, b"ZISO") {
                parsed("ZSO")
            } else if matches_at(head, 0, b"DAX\0") {
                parsed("DAX")
            } else if matches_at(head, 0, b"JISO") {
                parsed("JISO")
            } else {
                mismatch(format!("压缩镜像签名对不上：{}", magic_prefix(head)))
            }
        }
        // PBP：`\0PBP`，随后 8 个 u32 偏移表，PARAM.SFO 在第一个偏移处且未压缩。
        ProbeClass::Pbp => {
            if !matches_at(head, 0, b"\0PBP") {
                return mismatch(format!("PBP 签名对不上：{}", magic_prefix(head)));
            }
            let Some(raw) = slice_at(head, 8, 4) else {
                return ProbeOutcome::TooShort;
            };
            let sfo_offset = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
            if matches_at(head, sfo_offset, b"\0PSF") {
                parsed(format!("PBP，PARAM.SFO 在 0x{sfo_offset:X}"))
            } else {
                mismatch(format!("PBP 头在，但 0x{sfo_offset:X} 处不是 PARAM.SFO"))
            }
        }
        // WIA / RVZ 把光盘头 0x80 字节原样放在文件偏移 0x58。
        ProbeClass::RvzWia => {
            let format = if matches_at(head, 0, b"RVZ\x01") {
                "RVZ"
            } else if matches_at(head, 0, b"WIA\x01") {
                "WIA"
            } else {
                return mismatch(format!("RVZ/WIA 签名对不上：{}", magic_prefix(head)));
            };
            match ascii_field(head, 0x58, 6) {
                Some(id) if !id.is_empty() => parsed(format!("{format}，光盘 ID {id}")),
                _ => parsed(format!("{format}，未取到光盘 ID")),
            }
        }
        // WBFS 把 Wii 光盘头放在 hd_sector_size（通常 0x200）处。
        ProbeClass::Wbfs => {
            if !matches_at(head, 0, b"WBFS") {
                return mismatch(format!("WBFS 签名对不上：{}", magic_prefix(head)));
            }
            match ascii_field(head, 0x200, 6) {
                Some(id) if !id.is_empty() => parsed(format!("WBFS，光盘 ID {id}")),
                _ => parsed("WBFS，未取到光盘 ID"),
            }
        }
        // zstd 的魔数是小端 0xFD2FB528。
        ProbeClass::Zstd => {
            if matches_at(head, 0, &[0x28, 0xB5, 0x2F, 0xFD]) {
                parsed("zstd 帧头")
            } else {
                mismatch(format!("zstd 魔数对不上：{}", magic_prefix(head)))
            }
        }
        // WUX 的魔数是 `WUX0`（小端 0x30585557）。
        ProbeClass::Wux => {
            if matches_at(head, 0, b"WUX0") {
                parsed("WUX")
            } else {
                mismatch(format!("WUX 魔数对不上：{}", magic_prefix(head)))
            }
        }
        // GCZ 的 magic 是小端 0xB10BC001。
        ProbeClass::Gcz => {
            if matches_at(head, 0, &[0x01, 0xC0, 0x0B, 0xB1]) {
                parsed("GCZ")
            } else {
                mismatch(format!("GCZ 签名对不上：{}", magic_prefix(head)))
            }
        }
        ProbeClass::DiscImage | ProbeClass::BinTrack => probe_disc(class, head),
        ProbeClass::Cue => {
            let text = String::from_utf8_lossy(head);
            let text = text.trim_start_matches('\u{feff}');
            match text
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map(str::to_ascii_uppercase)
            {
                Some(line) if line.starts_with("FILE") || line.starts_with("REM") => {
                    parsed("cue 表单")
                }
                Some(line) => mismatch(format!(
                    "首行不是 FILE/REM：{}",
                    line.chars().take(24).collect::<String>()
                )),
                None => mismatch("空的 cue".to_string()),
            }
        }
        // iNES 头 16 字节；NES 2.0 在第 7 字节的 bit2-3 上标记。
        ProbeClass::Nes => {
            if matches_at(head, 0, b"UNIF") {
                return parsed("UNIF");
            }
            if !matches_at(head, 0, b"NES\x1a") {
                return mismatch(format!("iNES 头对不上：{}", magic_prefix(head)));
            }
            match head.get(7) {
                Some(byte) if byte & 0x0C == 0x08 => parsed("NES 2.0"),
                Some(_) => parsed("iNES"),
                None => ProbeOutcome::TooShort,
            }
        }
        ProbeClass::Snes => probe_snes(head, len),
        ProbeClass::GameBoy => probe_game_boy(head),
        // GBA：Nintendo logo 在 0x04，0xB2 处固定为 0x96，game code 在 0xAC。
        ProbeClass::Gba => {
            if head.get(0xB2) != Some(&0x96) {
                return mismatch(format!(
                    "0xB2 处不是 0x96：{}",
                    head.get(0xB2)
                        .map_or("读不到".to_string(), |b| format!("{b:02X}"))
                ));
            }
            if !matches_at(head, 0x04, &[0x24, 0xFF, 0xAE, 0x51]) {
                return mismatch("0x04 处的 Nintendo logo 对不上".to_string());
            }
            match ascii_field(head, 0xAC, 4) {
                Some(code) => parsed(format!("GBA，game code {code}")),
                None => parsed("GBA，game code 非 ASCII"),
            }
        }
        // NDS：0x15C 处是 Nintendo logo 的 CRC16（正版恒为 0xCF56），gamecode 在 0x0C。
        ProbeClass::Nds => {
            if !matches_at(head, 0x15C, &[0x56, 0xCF]) {
                return mismatch("0x15C 处的 logo CRC 不是 0xCF56".to_string());
            }
            match ascii_field(head, 0x0C, 4) {
                Some(code) => parsed(format!("NDS，gamecode {code}")),
                None => parsed("NDS，gamecode 非 ASCII"),
            }
        }
        ProbeClass::N64 => match slice_at(head, 0, 4) {
            Some([0x80, 0x37, 0x12, 0x40]) => parsed("z64（大端，无需归一化）"),
            Some([0x37, 0x80, 0x40, 0x12]) => parsed("v64（字节交换，需归一化）"),
            Some([0x40, 0x12, 0x37, 0x80]) => parsed("n64（小端，需归一化）"),
            Some(_) => mismatch(format!("N64 字节序魔数对不上：{}", magic_prefix(head))),
            None => ProbeOutcome::TooShort,
        },
        // MD：0x100 处是 `SEGA`，序列号在 0x180。`.smd` 是 512 字节头 + 交错块。
        ProbeClass::Genesis => {
            if matches_at(head, 0x100, b"SEGA") {
                return match ascii_field(head, 0x180, 14) {
                    Some(serial) => parsed(format!("MD，序列号 {serial}")),
                    None => parsed("MD"),
                };
            }
            if len % 16384 == 512 {
                return parsed("SMD 交错格式（需归一化后才能读内部头）");
            }
            mismatch("0x100 处不是 SEGA".to_string())
        }
        ProbeClass::MasterSystem => {
            for offset in [0x7FF0, 0x3FF0, 0x1FF0] {
                if matches_at(head, offset, b"TMR SEGA") {
                    return parsed(format!("TMR SEGA @ 0x{offset:X}"));
                }
            }
            mismatch("三个候选偏移处都没有 TMR SEGA".to_string())
        }
        ProbeClass::Lynx => {
            if matches_at(head, 0, b"LYNX") {
                parsed("LNX 头")
            } else {
                mismatch(format!("不是 LYNX 头：{}", magic_prefix(head)))
            }
        }
        ProbeClass::NeoGeoPocket => {
            let text = String::from_utf8_lossy(&head[..head.len().min(0x20)]);
            if text.contains("COPYRIGHT BY SNK") || text.contains("LICENSED BY SNK") {
                parsed("NGP 版权串")
            } else {
                mismatch(format!("头部没有 SNK 版权串：{}", magic_prefix(head)))
            }
        }
        // WS/WSC 的内部头在文件末尾 16 字节：复位向量的远跳转 0xEA 打头，
        // 倒数第 9 字节是最低机型（0x00 = WS，0x01 = WSC）。
        ProbeClass::WonderSwan => {
            if tail.len() < 16 {
                return ProbeOutcome::TooShort;
            }
            if tail[0] != 0xEA {
                return mismatch(format!("尾部 16 字节不以 0xEA 开头：{:02X}", tail[0]));
            }
            match tail[7] {
                0x00 => parsed("WS 尾部头"),
                0x01 => parsed("WSC 尾部头"),
                other => parsed(format!("尾部头，机型位 0x{other:02X}")),
            }
        }
        ProbeClass::Nintendo3ds => {
            if matches_at(head, 0x100, b"NCSD") {
                parsed("NCSD（cci）")
            } else if matches_at(head, 0x100, b"NCCH") {
                parsed("NCCH")
            } else {
                mismatch("0x100 处既不是 NCSD 也不是 NCCH".to_string())
            }
        }
        // CIA 的头没有魔数，靠固定的头长度 0x2020 认。
        ProbeClass::Cia => {
            if matches_at(head, 0, &[0x20, 0x20, 0x00, 0x00]) {
                parsed("CIA 头")
            } else {
                mismatch(format!("头长度不是 0x2020：{}", magic_prefix(head)))
            }
        }
    }
}

fn probe_disc(class: ProbeClass, head: &[u8]) -> ProbeOutcome {
    // ⭐ NKit 必须前置：这种文件的 CRC32 可能与好 dump 相同（ADR-0014）。
    if matches_at(head, 0x200, b"NKIT") {
        return parsed("NKit 转储（不是完好转储）");
    }
    // Wii：0x18 处 0x5D1C9EA3；NGC：0x1C 处 0xC2339F3D。光盘 ID 在偏移 0。
    if matches_at(head, 0x18, &[0x5D, 0x1C, 0x9E, 0xA3]) {
        return match ascii_field(head, 0, 6) {
            Some(id) => parsed(format!("Wii 光盘，ID {id}")),
            None => parsed("Wii 光盘"),
        };
    }
    if matches_at(head, 0x1C, &[0xC2, 0x33, 0x9F, 0x3D]) {
        return match ascii_field(head, 0, 6) {
            Some(id) => parsed(format!("NGC 光盘，ID {id}")),
            None => parsed("NGC 光盘"),
        };
    }
    if matches_at(head, 0, b"WUP-") {
        return match ascii_field(head, 0, 22) {
            Some(id) => parsed(format!("Wii U 光盘，{id}")),
            None => parsed("Wii U 光盘"),
        };
    }
    // ISO9660：主卷描述符在 0x8000，标识串 `CD001` 在 0x8001。
    if matches_at(head, 0x8001, b"CD001") {
        return parsed("ISO9660");
    }
    if class == ProbeClass::BinTrack {
        // 2352 字节裸扇区轨道的同步头。
        if matches_at(
            head,
            0,
            &[
                0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00,
            ],
        ) {
            return parsed("裸扇区光盘轨道（2352 字节/扇区）");
        }
        if matches_at(head, 0x100, b"SEGA") {
            return parsed("MD 卡带（扩展名是 bin）");
        }
        return mismatch(format!(
            "既不是光盘轨道也不是 MD 卡带：{}",
            magic_prefix(head)
        ));
    }
    mismatch(format!(
        "没有认出光盘头（Wii/NGC/Wii U/ISO9660 都对不上）：{}",
        magic_prefix(head)
    ))
}

fn probe_snes(head: &[u8], len: u64) -> ProbeOutcome {
    // 带 512 字节拷贝机头的 `.smc` 在严格扫描下匹配不上，必须先认出来再整体后移。
    let copier = len % 1024 == 512;
    let base = usize::from(copier) * 512;
    for (offset, mapping) in [(0x7FC0_usize, "LoROM"), (0xFFC0, "HiROM")] {
        let Some(header) = slice_at(head, base + offset, 0x20) else {
            continue;
        };
        let complement = u16::from_le_bytes([header[0x1C], header[0x1D]]);
        let checksum = u16::from_le_bytes([header[0x1E], header[0x1F]]);
        if checksum != 0 && complement ^ checksum == 0xFFFF {
            let title = String::from_utf8_lossy(&header[..21]).trim().to_string();
            let copier_note = if copier { "，带拷贝机头" } else { "" };
            return parsed(format!("SFC {mapping}{copier_note}，标题 {title}"));
        }
    }
    if head.len() < base + 0x7FE0 {
        return ProbeOutcome::TooShort;
    }
    mismatch(format!(
        "LoROM/HiROM 两处的校验和与补码都对不上{}",
        if copier {
            "（已按拷贝机头后移）"
        } else {
            ""
        }
    ))
}

fn probe_game_boy(head: &[u8]) -> ProbeOutcome {
    // Nintendo logo 在 0x104，头部校验和在 0x14D，覆盖 0x134..=0x14C。
    if !matches_at(
        head,
        0x104,
        &[0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B],
    ) {
        return mismatch("0x104 处的 Nintendo logo 对不上".to_string());
    }
    let Some(range) = slice_at(head, 0x134, 0x19) else {
        return ProbeOutcome::TooShort;
    };
    let Some(&stored) = head.get(0x14D) else {
        return ProbeOutcome::TooShort;
    };
    let computed = range
        .iter()
        .fold(0u8, |acc, byte| acc.wrapping_sub(*byte).wrapping_sub(1));
    if computed == stored {
        let title = String::from_utf8_lossy(&range[..11]);
        let title = title.trim_matches(|c: char| c == '\0' || c.is_whitespace());
        parsed(format!("GB 头，标题 {title}"))
    } else {
        mismatch(format!(
            "头部校验和对不上：文件里 {stored:02X}，算出来 {computed:02X}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 空的(len: usize) -> Vec<u8> {
        vec![0; len]
    }

    fn 写入(buf: &mut [u8], offset: usize, bytes: &[u8]) {
        buf[offset..offset + bytes.len()].copy_from_slice(bytes);
    }

    #[test]
    fn 按扩展名分派探针() {
        assert_eq!(probe_class_for(Path::new("a.ZIP")), Some(ProbeClass::Zip));
        assert_eq!(probe_class_for(Path::new("a.z64")), Some(ProbeClass::N64));
        assert_eq!(probe_class_for(Path::new("a.smc")), Some(ProbeClass::Snes));
        // 没有内部头的格式不参与抽样
        assert_eq!(probe_class_for(Path::new("a.lyx")), None);
        assert_eq!(probe_class_for(Path::new("a.pce")), None);
        assert_eq!(probe_class_for(Path::new("说明.txt")), None);
    }

    #[test]
    fn 透明容器的签名认得出来也报得出错() {
        assert!(probe(ProbeClass::Zip, b"PK\x03\x04rest", &[], 9).is_parsed());
        assert!(probe(ProbeClass::SevenZip, b"7z\xbc\xaf\x27\x1c", &[], 6).is_parsed());
        assert!(probe(ProbeClass::Rar, b"Rar!\x1a\x07\x01\x00", &[], 8).is_parsed());
        // 扩展名是 zip 但内容不是——这正是报告要报出来的东西
        let outcome = probe(ProbeClass::Zip, b"\x00\x00\x00\x00", &[], 4);
        assert!(matches!(outcome, ProbeOutcome::Mismatch(_)));
    }

    #[test]
    fn 压缩镜像的明文结构读得到() {
        let mut chd = 空的(16);
        写入(&mut chd, 0, b"MComprHD");
        写入(&mut chd, 12, &5u32.to_be_bytes());
        assert_eq!(probe(ProbeClass::Chd, &chd, &[], 16), parsed("CHD v5"));

        let mut pbp = 空的(0x200);
        写入(&mut pbp, 0, b"\0PBP");
        写入(&mut pbp, 8, &0x28u32.to_le_bytes());
        写入(&mut pbp, 0x28, b"\0PSF");
        assert!(probe(ProbeClass::Pbp, &pbp, &[], 0x200).is_parsed());

        let mut rvz = 空的(0x200);
        写入(&mut rvz, 0, b"RVZ\x01");
        写入(&mut rvz, 0x58, b"GALE01");
        assert_eq!(
            probe(ProbeClass::RvzWia, &rvz, &[], 0x200),
            parsed("RVZ，光盘 ID GALE01")
        );
    }

    #[test]
    fn nkit_检测前置于其他光盘判定() {
        let mut disc = 空的(0x8806);
        // 同时摆上 Wii 魔数与 NKit 标记，NKit 必须赢
        写入(&mut disc, 0x18, &[0x5D, 0x1C, 0x9E, 0xA3]);
        写入(&mut disc, 0x200, b"NKIT");
        assert_eq!(
            probe(ProbeClass::DiscImage, &disc, &[], 0x8806),
            parsed("NKit 转储（不是完好转储）")
        );
    }

    #[test]
    fn 光盘镜像按已知偏移认出来() {
        let mut wii = 空的(0x8806);
        写入(&mut wii, 0, b"RVZE01");
        写入(&mut wii, 0x18, &[0x5D, 0x1C, 0x9E, 0xA3]);
        assert_eq!(
            probe(ProbeClass::DiscImage, &wii, &[], 0x8806),
            parsed("Wii 光盘，ID RVZE01")
        );

        let mut iso = 空的(0x8806);
        写入(&mut iso, 0x8001, b"CD001");
        assert_eq!(
            probe(ProbeClass::DiscImage, &iso, &[], 0x8806),
            parsed("ISO9660")
        );
    }

    #[test]
    fn bin_同时覆盖光盘轨道与_md_卡带() {
        let mut track = 空的(0x8806);
        写入(
            &mut track,
            0,
            &[
                0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00,
            ],
        );
        assert!(probe(ProbeClass::BinTrack, &track, &[], 0x8806).is_parsed());

        let mut cart = 空的(0x8806);
        写入(&mut cart, 0x100, b"SEGA");
        assert!(probe(ProbeClass::BinTrack, &cart, &[], 0x8806).is_parsed());
    }

    #[test]
    fn fc_头认得出_ines_与_nes2() {
        assert_eq!(
            probe(ProbeClass::Nes, b"NES\x1a\x02\x01\x00\x00", &[], 16),
            parsed("iNES")
        );
        assert_eq!(
            probe(ProbeClass::Nes, b"NES\x1a\x02\x01\x00\x08", &[], 16),
            parsed("NES 2.0")
        );
    }

    #[test]
    fn gb_头部校验和算得对() {
        let mut rom = 空的(0x200);
        写入(
            &mut rom,
            0x104,
            &[0xCE, 0xED, 0x66, 0x66, 0xCC, 0x0D, 0x00, 0x0B],
        );
        写入(&mut rom, 0x134, b"POKEMON RED");
        let checksum = rom[0x134..=0x14C]
            .iter()
            .fold(0u8, |acc, byte| acc.wrapping_sub(*byte).wrapping_sub(1));
        rom[0x14D] = checksum;
        assert_eq!(
            probe(ProbeClass::GameBoy, &rom, &[], 0x200),
            parsed("GB 头，标题 POKEMON RED")
        );

        rom[0x14D] = checksum.wrapping_add(1);
        assert!(matches!(
            probe(ProbeClass::GameBoy, &rom, &[], 0x200),
            ProbeOutcome::Mismatch(_)
        ));
    }

    #[test]
    fn gba_与_nds_取到内部识别码() {
        let mut gba = 空的(0x200);
        写入(&mut gba, 0x04, &[0x24, 0xFF, 0xAE, 0x51]);
        gba[0xB2] = 0x96;
        写入(&mut gba, 0xAC, b"AGBJ");
        assert_eq!(
            probe(ProbeClass::Gba, &gba, &[], 0x200),
            parsed("GBA，game code AGBJ")
        );

        let mut nds = 空的(0x200);
        写入(&mut nds, 0x0C, b"ADAJ");
        写入(&mut nds, 0x15C, &[0x56, 0xCF]);
        assert_eq!(
            probe(ProbeClass::Nds, &nds, &[], 0x200),
            parsed("NDS，gamecode ADAJ")
        );
    }

    #[test]
    fn n64_三种字节序都认得出来() {
        assert!(probe(ProbeClass::N64, &[0x80, 0x37, 0x12, 0x40], &[], 4).is_parsed());
        assert!(probe(ProbeClass::N64, &[0x37, 0x80, 0x40, 0x12], &[], 4).is_parsed());
        assert!(probe(ProbeClass::N64, &[0x40, 0x12, 0x37, 0x80], &[], 4).is_parsed());
        assert!(!probe(ProbeClass::N64, &[0, 0, 0, 0], &[], 4).is_parsed());
    }

    #[test]
    fn sfc_的拷贝机头被认出来并整体后移() {
        // 不带拷贝机头的 LoROM
        let mut rom = 空的(0x1_0220);
        写入(&mut rom, 0x7FC0, b"SUPER MARIO WORLD    ");
        写入(&mut rom, 0x7FC0 + 0x1C, &0x1234u16.to_le_bytes());
        写入(&mut rom, 0x7FC0 + 0x1E, &(0xFFFF ^ 0x1234u16).to_le_bytes());
        let outcome = probe(ProbeClass::Snes, &rom, &[], 0x8000);
        assert_eq!(outcome, parsed("SFC LoROM，标题 SUPER MARIO WORLD"));

        // 带 512 字节拷贝机头：内容整体后移，且文件长度 % 1024 == 512
        let mut copied = 空的(0x1_0220);
        写入(&mut copied, 512 + 0x7FC0, b"SUPER MARIO WORLD    ");
        写入(&mut copied, 512 + 0x7FC0 + 0x1C, &0x1234u16.to_le_bytes());
        写入(
            &mut copied,
            512 + 0x7FC0 + 0x1E,
            &(0xFFFF ^ 0x1234u16).to_le_bytes(),
        );
        let outcome = probe(ProbeClass::Snes, &copied, &[], 0x8000 + 512);
        assert_eq!(
            outcome,
            parsed("SFC LoROM，带拷贝机头，标题 SUPER MARIO WORLD")
        );
    }

    #[test]
    fn ws_的内部头在文件末尾() {
        let mut tail = 空的(16);
        tail[0] = 0xEA;
        tail[7] = 0x01;
        assert_eq!(
            probe(ProbeClass::WonderSwan, &[], &tail, 1024),
            parsed("WSC 尾部头")
        );
        assert!(matches!(
            probe(ProbeClass::WonderSwan, &[], &空的(16), 1024),
            ProbeOutcome::Mismatch(_)
        ));
    }

    #[test]
    fn cue_按文本判定() {
        assert!(probe(ProbeClass::Cue, b"FILE \"a.bin\" BINARY\n", &[], 20).is_parsed());
        assert!(
            probe(
                ProbeClass::Cue,
                "\u{feff}REM 注释\nFILE".as_bytes(),
                &[],
                20
            )
            .is_parsed()
        );
        assert!(!probe(ProbeClass::Cue, b"\x00\x01\x02", &[], 3).is_parsed());
    }

    #[test]
    fn 文件太短时说太短而不是说对不上() {
        assert_eq!(
            probe(ProbeClass::Chd, b"MComprHD", &[], 8),
            ProbeOutcome::TooShort
        );
        assert_eq!(
            probe(ProbeClass::WonderSwan, &[], &[0xEA], 1),
            ProbeOutcome::TooShort
        );
    }
}
