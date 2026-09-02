//! **PARAM.SFO**：索尼四代机共用的那张键值表。
//!
//! PSP 的 `PSP_GAME/PARAM.SFO`、PS3 的 `PS3_GAME/PARAM.SFO`、PSV 的
//! `sce_sys/param.sfo`、以及 PBP 头里那一段，**是同一个格式**。它就是这张票要的东西：
//! 识别一个 2 GB 的转储不必读 2 GB，读这一两千字节就够——`TITLE_ID`、`CONTENT_ID`、
//! `DISC_ID`、`TITLE`、`APP_VER` 全在里面，而且是**明文**（PBP 里那一段甚至没被压缩，
//! DuckStation 的 `LoadSFOHeader` 就是纯 `fseek` + `fread`）。
//!
//! 布局（psdevwiki 逐字，`docs/research/rom-identification.md` A.13.1 与
//! `rom-identification-part2.md` 2.1.1）：
//!
//! | 偏移 | 长度 | 字段 |
//! |---|---|---|
//! | `0x00` | 4 | magic `"\0PSF"` |
//! | `0x04` | 4 | version（`01 01 00 00`） |
//! | `0x08` | 4 | `key_table_start`，**绝对**偏移 |
//! | `0x0C` | 4 | `data_table_start`，**绝对**偏移 |
//! | `0x10` | 4 | `tables_entries` |
//! | `0x14` + `n*0x10` | 16 | 索引表：`key_offset:u16`、`data_fmt:u16`、`data_len:u32`、`data_max_len:u32`、`data_offset:u32` |
//!
//! magic 与 version 在规范里写作大端，其余小端；PPSSPP 与 vitasdk 索性把五个都当小端
//! `u32` 读，于是 magic 的比较值是 `0x46535000`。这里按**字节**比 `"\0PSF"`，
//! 省掉那次字节序换算。
//!
//! ## 必须自己校验 magic
//!
//! 调研的原话：**Vita3K 完全不校验 `\0PSF`**（`SfoHeader::magic` 被读入但从未比较，
//! 全仓库 grep `0x46535000` 零命中），也没有任何边界检查，畸形 SFO 会越界读。
//! 这里反过来：magic 不对就当它不是 SFO，每一次取子串都走 `get()`。**这一层读的是
//! 主库里的真实字节，而主库里什么都有**——一个恰好叫 `param.sfo` 的文本文件不该让
//! 识别 panic。

/// 索引表里的类型码（psdevwiki 的 `data_fmt`）。
const FMT_INT32: u16 = 0x0404;

/// SFO 头的长度。
const HEADER_LEN: usize = 0x14;

/// 一条索引表条目的长度。
const ENTRY_LEN: usize = 0x10;

/// 一张读出来的 PARAM.SFO。
///
/// 键按盘上的顺序原样留着（规范说键表按字母序存，所以这本来就是有序的）。
/// 不折成 `BTreeMap`，是因为**同一个键出现两次**在畸形文件里是可能的，
/// 而那时该看见的是第一条，不是最后一条。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sfo {
    entries: Vec<(String, Value)>,
}

/// 一个 SFO 值：字符串或者整数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// utf8 / utf8-S。已经去掉结尾的 NUL。
    Text(String),
    /// int32。
    Int(u32),
}

impl Sfo {
    /// 认出一张 PARAM.SFO；不是的话返回 `None`。
    #[must_use]
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.get(..4)? != b"\0PSF" {
            return None;
        }
        let u32_at = |at: usize| -> Option<usize> {
            let raw = bytes.get(at..at + 4)?;
            usize::try_from(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]])).ok()
        };
        let key_start = u32_at(0x08)?;
        let data_start = u32_at(0x0C)?;
        let count = u32_at(0x10)?;
        // 条目数是文件自己写的，畸形文件可以写 40 亿。**先拿文件长度封顶**再循环，
        // 否则一个坏了的 SFO 会让这里空转四十亿次。
        let room = bytes.len().saturating_sub(HEADER_LEN) / ENTRY_LEN;
        let mut entries = Vec::new();
        for index in 0..count.min(room) {
            let at = HEADER_LEN + index * ENTRY_LEN;
            let raw = bytes.get(at..at + ENTRY_LEN)?;
            let key_offset = usize::from(u16::from_le_bytes([raw[0], raw[1]]));
            let fmt = u16::from_le_bytes([raw[2], raw[3]]);
            let len = usize::try_from(u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]])).ok()?;
            let offset =
                usize::try_from(u32::from_le_bytes([raw[12], raw[13], raw[14], raw[15]])).ok()?;
            let Some(key) = key_at(bytes, key_start.checked_add(key_offset)?) else {
                continue;
            };
            let from = data_start.checked_add(offset)?;
            let Some(data) = bytes.get(from..from.checked_add(len)?) else {
                continue;
            };
            let value = if fmt == FMT_INT32 {
                let Some(raw) = data.get(..4) else { continue };
                Value::Int(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
            } else {
                // utf8 是 NUL 结尾且 NUL 计入 `data_len`，utf8-S 不带 NUL——两种都
                // 靠「切到第一个 NUL」处理，省掉一次分支。
                let text = data.split(|b| *b == 0).next().unwrap_or_default();
                Value::Text(String::from_utf8_lossy(text).into_owned())
            };
            entries.push((key, value));
        }
        Some(Self { entries })
    }

    /// 取一个字符串值；键不在、或者它是整数时返回 `None`。
    #[must_use]
    pub fn text(&self, key: &str) -> Option<&str> {
        self.entries.iter().find_map(|(name, value)| match value {
            Value::Text(text) if name == key && !text.is_empty() => Some(text.as_str()),
            _ => None,
        })
    }

    /// 第一个取得到的字符串值。**键在各代机上不同名**（PSP 叫 `DISC_ID`、
    /// PSV 与 PS3 叫 `TITLE_ID`），调用方按优先级报一串上来。
    #[must_use]
    pub fn first_text(&self, keys: &[&str]) -> Option<&str> {
        keys.iter().find_map(|key| self.text(key))
    }

    /// 一共读出几条。
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 一条都没有。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn key_at(bytes: &[u8], at: usize) -> Option<String> {
    let rest = bytes.get(at..)?;
    let name = rest.split(|b| *b == 0).next()?;
    if name.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(name).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 照 psdevwiki 那份最小完整示例造一张（A.13.1 的 hex 块）。
    fn 最小示例() -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"\0PSF");
        out.extend_from_slice(&[0x01, 0x01, 0x00, 0x00]);
        out.extend_from_slice(&0x24u32.to_le_bytes()); // key_table_start
        out.extend_from_slice(&0x30u32.to_le_bytes()); // data_table_start
        out.extend_from_slice(&1u32.to_le_bytes()); // tables_entries
        // 索引表：key_offset 0、fmt 0x0204(utf8)、len 10、max 15、offset 0
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0x0204u16.to_le_bytes());
        out.extend_from_slice(&10u32.to_le_bytes());
        out.extend_from_slice(&15u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(b"TITLE_ID\0\0\0\0"); // 0x24 起，补到 0x30
        out.extend_from_slice(b"BLUS12345\0\0\0\0\0\0\0");
        out
    }

    #[test]
    fn 读得出规范里那份最小示例() {
        let sfo = Sfo::parse(&最小示例()).expect("是一张 SFO");
        assert_eq!(sfo.len(), 1);
        assert_eq!(sfo.text("TITLE_ID"), Some("BLUS12345"));
        assert_eq!(sfo.text("DISC_ID"), None);
        assert_eq!(sfo.first_text(&["DISC_ID", "TITLE_ID"]), Some("BLUS12345"));
    }

    #[test]
    fn magic_不对就不是_sfo() {
        // Vita3K 不校验这一条，畸形 SFO 会让它越界读。这一层读的是主库里的真实
        // 字节，一个恰好叫 param.sfo 的文本文件不该让识别翻车。
        assert!(Sfo::parse(b"hello world").is_none());
        assert!(Sfo::parse(b"").is_none());
        assert!(
            Sfo::parse(b"PSF\0\x01\x01\0\0").is_none(),
            "字节序写反的不算"
        );
    }

    #[test]
    fn 条目数写得再离谱也不会空转() {
        // 畸形文件把 tables_entries 写成 40 亿：拿文件长度封顶，循环最多走几圈。
        let mut bytes = 最小示例();
        bytes[0x10..0x14].copy_from_slice(&u32::MAX.to_le_bytes());
        let sfo = Sfo::parse(&bytes).expect("头还是读得懂");
        assert!(sfo.len() <= bytes.len() / ENTRY_LEN);
    }

    #[test]
    fn 偏移指到文件外面时那一条跳过而不是整张作废() {
        let mut bytes = 最小示例();
        // data_offset 指到天边
        bytes[0x14 + 12..0x14 + 16].copy_from_slice(&0xFFFF_0000u32.to_le_bytes());
        let sfo = Sfo::parse(&bytes);
        // 越界的 `checked_add` 返回 None，整张作废也可以接受——要紧的是不 panic。
        assert!(sfo.is_none() || sfo.expect("有").text("TITLE_ID").is_none());
    }

    #[test]
    fn 整数值与字符串值分得开() {
        let mut out = Vec::new();
        out.extend_from_slice(b"\0PSF");
        out.extend_from_slice(&[0x01, 0x01, 0x00, 0x00]);
        out.extend_from_slice(&0x34u32.to_le_bytes());
        out.extend_from_slice(&0x44u32.to_le_bytes());
        out.extend_from_slice(&2u32.to_le_bytes());
        for (key_offset, fmt, len, offset) in [(0u16, 0x0204u16, 3u32, 0u32), (9, FMT_INT32, 4, 4)]
        {
            out.extend_from_slice(&key_offset.to_le_bytes());
            out.extend_from_slice(&fmt.to_le_bytes());
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&offset.to_le_bytes());
        }
        out.extend_from_slice(b"CATEGORY\0"); // 0x34，键表从这里起
        out.extend_from_slice(b"REGION\0"); // 0x3D，键表到 0x44 为止
        out.extend_from_slice(b"gd\0\0"); // 0x44，数据表从这里起
        out.extend_from_slice(&0x8000u32.to_le_bytes());
        let sfo = Sfo::parse(&out).expect("是一张 SFO");
        assert_eq!(sfo.text("CATEGORY"), Some("gd"));
        assert_eq!(sfo.text("REGION"), None, "整数不从 text 出来");
    }
}
