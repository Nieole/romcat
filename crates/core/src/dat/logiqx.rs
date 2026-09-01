//! **Logiqx DAT 的解析**。Redump、TOSEC 与 No-Intro 都长这样。
//!
//! Redump 与 TOSEC 用的是 Logiqx 的 DTD（`http://www.logiqx.com/Dats/datafile.dtd`），
//! No-Intro 用自有 XSD 但结构兼容，只是多了 `sha256` 与 `serial`
//! （`docs/research/scraper-sources.md` 8.1/8.2）。差异都在**可选属性**上，
//! 一个解析器吃得下三家。
//!
//! ## 流式，不建整棵树
//!
//! 一条条交给回调而不是攒成 `Vec`。理由是真实数据的尺寸：
//! `Unofficial - Sony - PlayStation Vita (NoNpDrm)` 一份 DAT 里 452 个条目、
//! **431,605 个文件记录**。这一档的 DAT 就算不入库，解析器也不该是那个先崩的地方。

use quick_xml::events::{BytesStart, Event};

use super::xml;

/// DAT 头里那几行。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DatHeader {
    /// DAT 自称的名字，如 `Sony - PlayStation`。
    pub name: String,
    /// 描述，通常带条目数与生成时刻。
    pub description: String,
    /// 版本。Redump 是时间戳，No-Intro 是 `20260625-122811` 这种。
    pub version: String,
    /// 谁做的。
    pub author: String,
    /// `<clrmamepro header="…"/>` 声明的那个 skipper 文件名。
    ///
    /// **只记下来，不据此判断哈希口径。** 口径由数据源清单说了算——
    /// clrmamepro 那套 skipper 的语义没有一手文档，猜错的代价是整份 DAT 全落空。
    pub skipper: Option<String>,
}

/// 一条 DAT 条目。
///
/// 它**通常**是一个**发行版**，但 TOSEC 的 `[tr zh]` 条目是**汉化版**，那是**变体**。
/// 所以这一层只叫「条目」，挂到哪一层由票 07 定。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GameRecord {
    /// 条目名。语义信息（地区、语言、汉化组）全编码在这里面。
    pub name: String,
    /// 这个源自己的标识：No-Intro 的 `id`、MAME software list 的短名。
    pub key: Option<String>,
    /// 父条目名。No-Intro 的 parent/clone 关系。
    pub cloneof: Option<String>,
    /// 序列号。No-Intro 有，Redump 与 TOSEC 都没有（实测确认）。
    pub serial: Option<String>,
    /// `<category>`：Redump 用它分 Games / Demos / Applications。
    pub category: Option<String>,
    /// 这条条目下的文件记录。
    pub roms: Vec<RomRecord>,
}

/// 一条文件记录。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RomRecord {
    /// 文件名。
    pub name: String,
    /// 字节数。
    pub size: Option<u64>,
    /// CRC-32。票 07 的主键。
    pub crc32: Option<u32>,
    /// MD5。
    pub md5: Option<String>,
    /// SHA-1。
    pub sha1: Option<String>,
    /// SHA-256。**只有 No-Intro 有**，Redump 与 TOSEC 都没有。
    pub sha256: Option<String>,
    /// `status`：实测取值 `verified` / `baddump` / `nodump`。
    pub status: Option<String>,
}

/// DAT 读不动。
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// XML 本身坏了。
    #[error("{what} 不是能读的 XML：{detail}")]
    Xml {
        /// 在读哪一份。
        what: String,
        /// 底层说了什么。
        detail: String,
    },
    /// 读完了也没见到该有的东西。
    ///
    /// 三种格式共用这一条，所以**期待的是什么由调用方说**——最常撞上它的场景是
    /// 取回来的其实是个 404 页面，那时打给用户的话必须说清「我以为这是什么」。
    #[error("{what} 不像{expected}")]
    NotADat {
        /// 在读哪一份。
        what: String,
        /// 期待的是什么。
        expected: String,
    },
}

/// 一条条读出来交给 `on_game`，返回 DAT 头。
///
/// `what` 只用在报错里，写这份 DAT 是从哪儿来的。
///
/// # Errors
/// XML 坏了、或者根本不是一份 DAT 时返回错误。
pub fn parse(
    bytes: &[u8],
    what: &str,
    on_game: &mut dyn FnMut(GameRecord),
) -> Result<DatHeader, ParseError> {
    let mut reader = xml::reader(bytes);
    let mut buf = Vec::new();
    let mut header = DatHeader::default();
    let mut saw_datafile = false;
    let mut game: Option<GameRecord> = None;
    // 当前在 `<header>` 或 `<game>` 里的哪个子元素上，用来收文本。
    let mut field: Option<String> = None;
    // 文本要**攒**而不是拿一次就用：quick-xml 把 `&amp;` 拆成独立的 `GeneralRef`
    // 事件，于是 `Nintendo Famicom &amp; Entertainment System` 会分三次交过来。
    // 拿第一段就用，名字会被腰斩成 `Nintendo Famicom `。
    let mut text = String::new();
    let mut in_header = false;

    loop {
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|error| ParseError::Xml {
                what: what.to_string(),
                detail: error.to_string(),
            })?;
        match event {
            // **`Start` 与 `Empty` 走同一条路。** `<rom .../>` 与
            // `<rom ...></rom>` 在 XML 里是同一件事，而 DAT 两种写法都真实存在；
            // 只认自闭合的那种，会把 `<rom>` 当成一个要收文本的字段，整条记录静默丢掉。
            Event::Start(start) | Event::Empty(start) => {
                let tag = xml::tag(&start);
                match tag.as_str() {
                    "datafile" => saw_datafile = true,
                    "header" => in_header = true,
                    "game" | "machine" | "software" => game = Some(game_from(&start)),
                    "rom" => {
                        if let Some(game) = game.as_mut() {
                            game.roms.push(rom_from(&start));
                        }
                    }
                    "clrmamepro" => header.skipper = xml::attribute(&start, "header"),
                    _ => {
                        field = Some(tag);
                        text.clear();
                    }
                }
            }
            Event::Text(chunk) => {
                if field.is_some() {
                    text.push_str(&chunk.xml_content().unwrap_or_default());
                }
            }
            Event::CData(chunk) => {
                if field.is_some() {
                    text.push_str(&chunk.decode().unwrap_or_default());
                }
            }
            Event::GeneralRef(reference) => {
                if field.is_some() {
                    text.push_str(&xml::entity_text(&reference));
                }
            }
            Event::End(end) => {
                let tag = xml::local_name(end.name().as_ref());
                match tag.as_str() {
                    "header" => in_header = false,
                    "game" | "machine" | "software" => {
                        if let Some(finished) = game.take() {
                            on_game(finished);
                        }
                    }
                    _ => {}
                }
                if field.as_deref() == Some(tag.as_str()) {
                    let value = std::mem::take(&mut text).trim().to_string();
                    if in_header {
                        match tag.as_str() {
                            "name" => header.name = value,
                            "description" => header.description = value,
                            "version" => header.version = value,
                            "author" => header.author = value,
                            _ => {}
                        }
                    } else if let Some(game) = game.as_mut()
                        && tag == "category"
                    {
                        game.category = Some(value);
                    }
                }
                field = None;
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    if !saw_datafile {
        return Err(ParseError::NotADat {
            what: what.to_string(),
            expected: "一份 Logiqx DAT（读完也没见到 <datafile>）".to_string(),
        });
    }
    Ok(header)
}

fn game_from(start: &BytesStart<'_>) -> GameRecord {
    GameRecord {
        name: xml::attribute(start, "name").unwrap_or_default(),
        key: xml::attribute(start, "id"),
        cloneof: xml::attribute(start, "cloneof"),
        serial: xml::attribute(start, "serial"),
        category: None,
        roms: Vec::new(),
    }
}

fn rom_from(tag: &BytesStart<'_>) -> RomRecord {
    RomRecord {
        name: xml::attribute(tag, "name").unwrap_or_default(),
        size: xml::attribute(tag, "size").and_then(|text| text.trim().parse().ok()),
        crc32: xml::attribute(tag, "crc").and_then(|text| parse_crc(&text)),
        md5: xml::attribute(tag, "md5").map(|text| text.to_ascii_lowercase()),
        sha1: xml::attribute(tag, "sha1").map(|text| text.to_ascii_lowercase()),
        sha256: xml::attribute(tag, "sha256").map(|text| text.to_ascii_lowercase()),
        status: xml::attribute(tag, "status"),
    }
}

/// CRC-32 在 DAT 里是十六进制字符串，可能不补齐到 8 位。
#[must_use]
pub fn parse_crc(text: &str) -> Option<u32> {
    let text = text.trim().trim_start_matches("0x");
    if text.is_empty() || text.len() > 8 || !text.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(text, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 抄自 `https://redump.info/datfile/PSX` 真实的开头（2026-08-31 实测）。
    const REDUMP: &[u8] = br#"<?xml version="1.0"?>
<!DOCTYPE datafile PUBLIC "-//Logiqx//DTD ROM Management Datafile//EN" "http://www.logiqx.com/Dats/datafile.dtd">
<datafile>
	<header>
		<name>Sony - PlayStation</name>
		<description>Sony - PlayStation - Datfile (10974) (2026-08-31 07-59-02)</description>
		<version>2026-08-31 07-59-02</version>
		<author>redump.info</author>
		<homepage>redump.info</homepage>
		<url>https://redump.info/</url>
	</header>
	<game name="'98 Koushien (Japan) (Demo)" id="36628">
		<category>Demos</category>
		<description>'98 Koushien (Japan) (Demo)</description>
		<rom name="'98 Koushien (Japan) (Demo).cue" size="93" crc="6ab3e4ce" md5="ba3aedf4b27af49f6761cd000817f6a8" sha1="9d667ca1e3e166b6f395efb6a27f6e812eb08b57"/>
		<rom name="'98 Koushien (Japan) (Demo).bin" size="443733024" crc="6af29073" md5="0aa29df886d9f4f3ec5340bdd3b4f3be" sha1="6bdd288b5768bdbd2f0f345f77dd6c283bb7ec47"/>
	</game>
</datafile>"#;

    fn games_of(xml: &[u8]) -> (DatHeader, Vec<GameRecord>) {
        let mut games = Vec::new();
        let header = parse(xml, "测试", &mut |game| games.push(game)).expect("该读得动");
        (header, games)
    }

    #[test]
    fn redump_的一份_dat_读得出头与条目() {
        let (header, games) = games_of(REDUMP);
        assert_eq!(header.name, "Sony - PlayStation");
        assert_eq!(header.author, "redump.info");
        assert_eq!(header.version, "2026-08-31 07-59-02");
        assert_eq!(games.len(), 1);
        let game = &games[0];
        assert_eq!(game.name, "'98 Koushien (Japan) (Demo)");
        assert_eq!(game.key.as_deref(), Some("36628"));
        assert_eq!(game.category.as_deref(), Some("Demos"));
        // Redump 官方 DAT 不含 serial（实测确认）
        assert_eq!(game.serial, None);
        assert_eq!(game.roms.len(), 2);
        assert_eq!(game.roms[1].size, Some(443_733_024));
        assert_eq!(game.roms[1].crc32, Some(0x6af2_9073));
        assert_eq!(
            game.roms[1].sha1.as_deref(),
            Some("6bdd288b5768bdbd2f0f345f77dd6c283bb7ec47")
        );
        // Redump 没有 sha256（与 No-Intro 的四套哈希形成对比）
        assert_eq!(game.roms[1].sha256, None);
    }

    #[test]
    fn no_intro_的四套哈希与_serial_都读得出() {
        // 抄自 `docs/research/scraper-sources.md` 8.1 的实测条目，补上 sha256。
        let xml = br#"<datafile><header><name>Nintendo - Game Boy Advance</name></header>
<game name="007 - Everything or Nothing (Japan)" id="1468" cloneofid="1256" cloneof="007 - Everything or Nothing (USA)">
  <description>007 - Everything or Nothing (Japan)</description>
  <rom name="007.gba" size="8388608" crc="caf2e99f" md5="55354d9e3bc9c1fa682b5110e5ed1544"
       sha1="6E4E9BE9A07580EF267BE9C2EA1BD0730B3BE44A"
       sha256="0000000000000000000000000000000000000000000000000000000000000001"
       status="verified" serial="BJBJ"/>
</game></datafile>"#;
        let (_, games) = games_of(xml);
        let game = &games[0];
        assert_eq!(
            game.cloneof.as_deref(),
            Some("007 - Everything or Nothing (USA)")
        );
        let rom = &game.roms[0];
        // 哈希一律折成小写：DAT 之间大小写不统一，票 07 要拿字符串直接比。
        assert_eq!(
            rom.sha1.as_deref(),
            Some("6e4e9be9a07580ef267be9c2ea1bd0730b3be44a")
        );
        assert!(rom.sha256.is_some());
        assert_eq!(rom.status.as_deref(), Some("verified"));
    }

    #[test]
    fn 条目名里的实体引用要还原() {
        // TOSEC 的 DAT 里真有 `&amp;`：`Nintendo Famicom & Entertainment System`。
        // 这正是不自己手写 XML 解析器的理由。
        let xml = br#"<datafile><header><name>Nintendo Famicom &amp; Entertainment System</name></header>
<game name="Tom &amp; Jerry (1991)(Hi Tech)"><rom name="a.nes" size="16" crc="0"/></game></datafile>"#;
        let (header, games) = games_of(xml);
        assert_eq!(header.name, "Nintendo Famicom & Entertainment System");
        assert_eq!(games[0].name, "Tom & Jerry (1991)(Hi Tech)");
    }

    #[test]
    fn 非自闭合的_rom_标签也认() {
        // `<rom .../>` 与 `<rom ...></rom>` 在 XML 里是同一件事。只认自闭合那种，
        // 另一种写法会被当成一个要收文本的字段，整条文件记录静默消失。
        let xml = br#"<datafile><header><name>X</name></header>
<game name="G"><rom name="a.bin" size="16" crc="0000ffff"></rom></game></datafile>"#;
        let (_, games) = games_of(xml);
        assert_eq!(games[0].roms.len(), 1);
        assert_eq!(games[0].roms[0].crc32, Some(0x0000_ffff));
    }

    #[test]
    fn 不是_dat_的东西要说清楚() {
        let error =
            parse(b"<html><body>404</body></html>", "某处", &mut |_| {}).expect_err("这不是 DAT");
        assert!(matches!(error, ParseError::NotADat { .. }));
    }

    #[test]
    fn crc_的各种写法() {
        assert_eq!(parse_crc("caf2e99f"), Some(0xcaf2_e99f));
        assert_eq!(parse_crc("CAF2E99F"), Some(0xcaf2_e99f));
        assert_eq!(parse_crc("0x0cd0bab6"), Some(0x0cd0_bab6));
        assert_eq!(parse_crc("0"), Some(0));
        assert_eq!(parse_crc(""), None);
        assert_eq!(parse_crc("zzzz"), None);
        assert_eq!(parse_crc("caf2e99f0"), None);
    }
}
