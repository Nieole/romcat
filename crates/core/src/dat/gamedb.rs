//! **BizHawk 随发行版附带的 gamedb** 的解析——GoodNES 那一族的落脚点。
//!
//! GoodTools 早已停更、官网也没了，但 BizHawk 把 GoodNES 与 GoodGen 转成纯文本
//! 跟着发行版一起发（`Assets/gamedb/`）。本工具实测 `gamedb_goodnes.txt` 22,095 条里有
//! **646 条** `[T+Chi]` / `[T-Chi]` 中文汉化条目，`gamedb_sega_md.txt` 8,149 条里有
//! **102 条**——这是 No-Intro 与 Redump 政策上永远不会收录的那部分。
//!
//! ⚠ 调研记的 MD「172 条」（`docs/research/rom-identification.md` C.4.3）把这 102 条
//! 与 70 条 `(Ch)`（GoodTools 的**中国区**国别码）加在了一起。`(Ch)` 说的是发行地区
//! 不是谁做的中文，两者不能相加（见 [`chinese`](super::chinese)）。
//!
//! ## 只有一个哈希，而且**不一定是哪一种**
//!
//! 一行四列：`哈希 ⇥ 状态 ⇥ 名字 ⇥ 系统`。**没有大小、没有 CRC-32**。
//!
//! 首列的哈希种类**逐行而定，不是逐文件**——实测 `gamedb_sega_md.txt` 里 5,774 行是
//! MD5（32 位十六进制）、2,375 行是 SHA-1（40 位），而 `gamedb_goodnes.txt` 全部
//! 22,095 行都是 SHA-1。按长度分辨是唯一可行的办法，而且必须两种都收：
//! 只认 SHA-1 会丢掉那份文件里 102 条中文条目中的 35 条。
//!
//! 哈希口径是**含头**：实测 `'89 Dennou Kyuusei Uranai (J) [f1]` 在这里的 SHA-1
//! `3918c52a…` 与 TOSEC 那份带头 `.nes` 的 SHA-1 逐位相同
//! （`docs/research/rom-identification.md` A.17.4）。

use super::logiqx::{DatHeader, GameRecord, ParseError, RomRecord};

/// 一行一行读出来交给 `on_game`，返回一个当头用的壳。
///
/// `what` 是这份文件叫什么（`gamedb_goodnes.txt`），同时用在报错与头的名字上。
///
/// # Errors
/// 一行有用的都没有时返回错误——那多半是取回来的不是这份文件（比如一个 404 页面）。
pub fn parse(
    text: &[u8],
    what: &str,
    on_game: &mut dyn FnMut(GameRecord),
) -> Result<DatHeader, ParseError> {
    let text = String::from_utf8_lossy(text);
    let mut header = DatHeader {
        name: what.to_string(),
        author: "GoodTools（经 BizHawk 转录）".to_string(),
        ..DatHeader::default()
    };
    let mut rows = 0_usize;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        // `;` 开头是注释。头两行写着这是哪个版本的库，留作描述。
        if let Some(comment) = line.strip_prefix(';') {
            if header.description.is_empty() {
                header.description = comment.trim().to_string();
            } else if header.version.is_empty() {
                header.version = comment.trim().to_string();
            }
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let mut columns = line.split('\t');
        let (Some(hash), Some(status), Some(name)) =
            (columns.next(), columns.next(), columns.next())
        else {
            continue;
        };
        let Some(hash) = Hash::of(hash.trim()) else {
            continue;
        };
        rows += 1;
        let mut rom = RomRecord {
            // 这一族没有文件名，条目名就是它能给的全部。
            name: name.trim().to_string(),
            status: Some(status.trim().to_string()).filter(|s| !s.is_empty()),
            ..RomRecord::default()
        };
        match hash {
            Hash::Md5(value) => rom.md5 = Some(value),
            Hash::Sha1(value) => rom.sha1 = Some(value),
        }
        on_game(GameRecord {
            name: name.trim().to_string(),
            roms: vec![rom],
            ..GameRecord::default()
        });
    }
    if rows == 0 {
        return Err(ParseError::NotADat {
            what: what.to_string(),
            expected: "一份 BizHawk gamedb（一行有用的都没有）".to_string(),
        });
    }
    Ok(header)
}

/// 首列那个哈希是哪一种。**按长度分辨**——这份格式不声明种类，而同一个文件里两种都有。
enum Hash {
    /// 32 位十六进制。
    Md5(String),
    /// 40 位十六进制。
    Sha1(String),
}

impl Hash {
    fn of(text: &str) -> Option<Self> {
        if !text.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        match text.len() {
            32 => Some(Self::Md5(text.to_ascii_lowercase())),
            40 => Some(Self::Sha1(text.to_ascii_lowercase())),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dat::chinese::{ChineseMark, mark_of};

    /// 逐字抄自 `TASEmulators/BizHawk` 的 `Assets/gamedb/gamedb_goodnes.txt`。
    const GOODNES: &[u8] = b";GoodNES SHA-1 List\n\
;This is Version 3.32a's Database.\n\
2d5b194357b46ad993a6ac716949fb1688a46939\tU\t!Clik! by Sly Dog Studios (2008) (PD)\tNES\n\
3918c52a5491695ed6b10b65f59622687dd7dc66\tH\t'89 Dennou Kyuusei Uranai (J) [f1]\tNES\n\
859a8b0459496fb4e998ffcf04414a69155c1eff\tT\t1942 (JU) [T+Chi_MS emumax]\tNES\n\
6d419c58b56098b94d5f665139fd84a9e0f2f60d\tT\t4 Nin Uchi Mahjong (J) (PRG1) [T+Chi]\tNES\n";

    #[test]
    fn 读得出_sha1_与汉化条目() {
        let mut games = Vec::new();
        let header =
            parse(GOODNES, "gamedb_goodnes.txt", &mut |game| games.push(game)).expect("该读得动");
        assert_eq!(header.description, "GoodNES SHA-1 List");
        assert_eq!(header.version, "This is Version 3.32a's Database.");
        assert_eq!(games.len(), 4);

        // 这一行的 SHA-1 与 TOSEC 那份带头 `.nes` 逐位相同——GoodNES 是含头口径的证据。
        assert_eq!(
            games[1].roms[0].sha1.as_deref(),
            Some("3918c52a5491695ed6b10b65f59622687dd7dc66")
        );
        assert_eq!(games[1].roms[0].crc32, None, "这一族没有 CRC-32");
        assert_eq!(games[1].roms[0].size, None, "也没有大小");

        let zh = games
            .iter()
            .filter(|game| mark_of(&game.name) == Some(ChineseMark::FanTranslated))
            .count();
        assert_eq!(zh, 2);
    }

    #[test]
    fn 取回来不是这份文件时要说清楚() {
        let error = parse(b"<html>404</html>", "gamedb_goodnes.txt", &mut |_| {})
            .expect_err("这不是 gamedb");
        assert!(matches!(error, ParseError::NotADat { .. }));
    }

    #[test]
    fn 同一个文件里_md5_与_sha1_两种行都要收() {
        // `gamedb_sega_md.txt` 实测 5,774 行 MD5、2,375 行 SHA-1，中文条目
        // 35 : 67 分布在两边。只认 SHA-1 会静默丢掉三分之一。
        let text = "\
C0F6A98BB593DF365A9862CF0A72AC92\t\tAdvanced Daisenryaku (J) (REV01) [T+Chi]\tGEN\n\
6d419c58b56098b94d5f665139fd84a9e0f2f60d\tT\t4 Nin Uchi Mahjong (J) [T+Chi]\tNES\n";
        let mut games = Vec::new();
        parse(text.as_bytes(), "gamedb_sega_md.txt", &mut |game| {
            games.push(game)
        })
        .expect("两行都读得动");
        assert_eq!(games.len(), 2);
        assert_eq!(
            games[0].roms[0].md5.as_deref(),
            Some("c0f6a98bb593df365a9862cf0a72ac92")
        );
        assert_eq!(games[0].roms[0].sha1, None);
        // 状态那一列是空的，别记成一个空字符串。
        assert_eq!(games[0].roms[0].status, None);
        assert!(games[1].roms[0].sha1.is_some());
        assert_eq!(games[1].roms[0].md5, None);
        for game in &games {
            assert_eq!(mark_of(&game.name), Some(ChineseMark::FanTranslated));
        }
    }

    #[test]
    fn 列数不够或哈希不像的行直接跳过() {
        let text = "aaa\tG\t名\tNES\n\
不是哈希\tG\t名\tNES\n\
0000000000000000000000000000000000000000\tG\t好的\tNES\n";
        let mut games = Vec::new();
        parse(text.as_bytes(), "x", &mut |game| games.push(game)).expect("有一行是好的");
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].name, "好的");
    }
}
