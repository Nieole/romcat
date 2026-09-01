//! **MAME software list 的解析**。结构与 Logiqx 不同，得单写一个。
//!
//! 这个源的价值不在覆盖面而在**许可**：MAME 整体是 GPL-2.0，但 `hash/` 目录被
//! **单独献给公有领域**（`COPYING` 原文 "The contents of the hash directory are
//! dedicated to the public domain"，每个 XML 还自带 `license:CC0-1.0` 头）。
//! 这是全部数据源里唯一可以无条件商用、无署名义务、无 ShareAlike 传染的哈希源。
//!
//! ## 一条 `<rom>` 是一颗芯片，不是一个文件
//!
//! MAME 把一张卡拆成 `prg` / `chr` / `vram` 若干 `<dataarea>`，每条 `<rom>` 对应实体
//! 卡带上的一颗芯片，文件名用的是芯片丝印。所以它的哈希口径是
//! [`Convention::PerChip`](super::Convention)：单芯片卡上恰好等于去头哈希（实测
//! `'89 Dennou Kyuusei Uranai` 的 PRG dataarea 与 No-Intro headerless 的 SHA-1 完全
//! 一致），多芯片卡上则要拼起来才对得上。
//!
//! DTD 权威确认：`<rom>` **只有 `crc` 和 `sha1`**，没有 `md5`、没有 `serial`；
//! `<disk>` 只有 `sha1`。序列号在 `<info name="serial">` 里，是卡带丝印。

use quick_xml::events::Event;

use super::logiqx::{DatHeader, GameRecord, ParseError, RomRecord, parse_crc};
use super::xml;

/// 一条条读出来交给 `on_game`，返回这份 software list 的头。
///
/// 头里的 `name` 取 `<softwarelist name>`（如 `nes`），`description` 取它的
/// `description` 属性。这份格式没有版本字段——版本由取它的那个 git blob sha 代表。
///
/// # Errors
/// XML 坏了、或者根本不是一份 software list 时返回错误。
pub fn parse(
    bytes: &[u8],
    what: &str,
    on_game: &mut dyn FnMut(GameRecord),
) -> Result<DatHeader, ParseError> {
    let mut reader = xml::reader(bytes);
    let mut buf = Vec::new();
    let mut header = DatHeader::default();
    let mut saw_list = false;
    let mut software: Option<GameRecord> = None;
    let mut in_description = false;
    // 同 logiqx：`&amp;` 会拆成独立事件，文本得攒。
    let mut text = String::new();

    loop {
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|error| ParseError::Xml {
                what: what.to_string(),
                detail: error.to_string(),
            })?;
        match event {
            // `Start` 与 `Empty` 同路，理由同 `logiqx`：`<rom .../>` 与
            // `<rom ...></rom>` 是同一件事，只认一种会静默丢记录。
            Event::Start(start) | Event::Empty(start) => match xml::tag(&start).as_str() {
                "softwarelist" => {
                    saw_list = true;
                    header.name = xml::attribute(&start, "name").unwrap_or_default();
                    header.description = xml::attribute(&start, "description").unwrap_or_default();
                    header.author = "MAME（CC0-1.0）".to_string();
                }
                "software" => {
                    software = Some(GameRecord {
                        // 短名是 MAME 自己的键（`89denku`），人看的标题在
                        // `<description>` 里——下面读到就覆盖 `name`。
                        name: xml::attribute(&start, "name").unwrap_or_default(),
                        key: xml::attribute(&start, "name"),
                        cloneof: xml::attribute(&start, "cloneof"),
                        ..GameRecord::default()
                    });
                }
                "description" => {
                    in_description = software.is_some();
                    text.clear();
                }
                "rom" => {
                    if let Some(software) = software.as_mut() {
                        software.roms.push(RomRecord {
                            name: xml::attribute(&start, "name").unwrap_or_default(),
                            size: xml::attribute(&start, "size")
                                .and_then(|t| t.trim().parse().ok()),
                            crc32: xml::attribute(&start, "crc").and_then(|t| parse_crc(&t)),
                            sha1: xml::attribute(&start, "sha1").map(|t| t.to_ascii_lowercase()),
                            status: xml::attribute(&start, "status"),
                            ..RomRecord::default()
                        });
                    }
                }
                // CHD 走 `<disk>`，只有 sha1，没有大小也没有 crc。
                "disk" => {
                    if let Some(software) = software.as_mut() {
                        software.roms.push(RomRecord {
                            name: xml::attribute(&start, "name").unwrap_or_default(),
                            sha1: xml::attribute(&start, "sha1").map(|t| t.to_ascii_lowercase()),
                            status: xml::attribute(&start, "status"),
                            ..RomRecord::default()
                        });
                    }
                }
                // 卡带丝印序列号在这里，不在属性上。
                "info" => {
                    if let Some(software) = software.as_mut()
                        && xml::attribute(&start, "name").as_deref() == Some("serial")
                    {
                        software.serial = xml::attribute(&start, "value");
                    }
                }
                _ => {}
            },
            Event::Text(chunk) => {
                if in_description {
                    text.push_str(&chunk.xml_content().unwrap_or_default());
                }
            }
            Event::CData(chunk) => {
                if in_description {
                    text.push_str(&chunk.decode().unwrap_or_default());
                }
            }
            Event::GeneralRef(reference) => {
                if in_description {
                    text.push_str(&xml::entity_text(&reference));
                }
            }
            Event::End(end) => {
                let tag = xml::local_name(end.name().as_ref());
                if tag == "description" {
                    if in_description && let Some(software) = software.as_mut() {
                        software.name = std::mem::take(&mut text).trim().to_string();
                    }
                    in_description = false;
                } else if tag == "software"
                    && let Some(finished) = software.take()
                {
                    on_game(finished);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    if !saw_list {
        return Err(ParseError::NotADat {
            what: what.to_string(),
            expected: "一份 MAME software list（读完也没见到 <softwarelist>）".to_string(),
        });
    }
    Ok(header)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 照 `mamedev/mame` 的 `hash/nes.xml` 的形状写的，数值取自调研里那条
    /// 跨库交叉验证的证据链（`docs/research/rom-identification.md` A.17.4）。
    const NES: &[u8] = br#"<?xml version="1.0"?>
<!DOCTYPE softwarelist SYSTEM "softwarelist.dtd">
<!--
license:CC0-1.0
-->
<softwarelist name="nes" description="Nintendo Entertainment System cartridges">
	<software name="89denku">
		<description>'89 Dennou Kyuusei Uranai</description>
		<year>1988</year>
		<publisher>Jingukan</publisher>
		<info name="serial" value="IPC-J1-01"/>
		<part name="cart" interface="nes_cart">
			<dataarea name="prg" size="262144">
				<rom name="ipc-j1-0 prg" size="262144" crc="ba58ed29" sha1="56fe858d1035dce4b68520f457a0858bae7bb16d" offset="00000"/>
			</dataarea>
			<dataarea name="vram" size="8192"/>
		</part>
	</software>
	<software name="clone1" cloneof="89denku">
		<description>Another Dump</description>
		<part name="cart" interface="nes_cart">
			<dataarea name="prg" size="16">
				<rom name="x prg" size="16" crc="00000001" sha1="0000000000000000000000000000000000000001"/>
			</dataarea>
		</part>
	</software>
</softwarelist>"#;

    #[test]
    fn 芯片级的哈希与丝印序列号都读得出() {
        let mut games = Vec::new();
        let header = parse(NES, "nes.xml", &mut |game| games.push(game)).expect("该读得动");
        assert_eq!(header.name, "nes");
        assert_eq!(
            header.description,
            "Nintendo Entertainment System cartridges"
        );
        assert_eq!(games.len(), 2);

        let first = &games[0];
        // 人看的标题在 `<description>` 里，MAME 自己的键是短名。
        assert_eq!(first.name, "'89 Dennou Kyuusei Uranai");
        assert_eq!(first.key.as_deref(), Some("89denku"));
        assert_eq!(first.serial.as_deref(), Some("IPC-J1-01"));
        assert_eq!(first.roms.len(), 1);
        // 这一行与 No-Intro headerless 的 SHA-1 完全一致——单芯片卡上
        // 「逐芯片」恰好等于「去头」，这条断言就是那个证据链。
        assert_eq!(
            first.roms[0].sha1.as_deref(),
            Some("56fe858d1035dce4b68520f457a0858bae7bb16d")
        );
        assert_eq!(first.roms[0].crc32, Some(0xba58_ed29));
        // DTD 权威确认：没有 md5
        assert_eq!(first.roms[0].md5, None);

        assert_eq!(games[1].cloneof.as_deref(), Some("89denku"));
    }

    #[test]
    fn 空的_dataarea_不产出文件记录() {
        // `<dataarea name="vram" size="8192"/>` 是「这颗卡用 8K VRAM」，
        // 不是一份可比对的转储。
        let mut roms = 0;
        parse(NES, "nes.xml", &mut |game| roms += game.roms.len()).expect("该读得动");
        assert_eq!(roms, 2);
    }

    #[test]
    fn 不是_software_list_的东西要说清楚() {
        let error = parse(b"<datafile></datafile>", "某处", &mut |_| {}).expect_err("这不是");
        assert!(matches!(error, ParseError::NotADat { .. }));
    }
}
