//! 两个 XML 解析器共用的那几件小事。
//!
//! [`logiqx`](super::logiqx) 与 [`softlist`](super::softlist) 读的是两种结构完全不同的
//! XML，但**怎么读一个 XML** 是同一件事：剥 BOM、关掉会磨掉空格的 `trim_text`、
//! 按 local name 认标签、把属性里的实体引用还原。这几件各写一遍的代价不是多几行——
//! 是两边会**悄悄分叉**：一边按 local name 比标签、另一边按原始字节全等，
//! 于是带命名空间前缀的 DAT 在一个解析器里读得动、在另一个里读不动。

use quick_xml::Reader;
use quick_xml::events::{BytesRef, BytesStart};

/// 开一个读 DAT 用的 reader。
///
/// **不开 `trim_text`。** 它会把每一段文本各自去空白，而 `&amp;` 会把一段文本拆成
/// 三个事件——`Nintendo Famicom &amp; Entertainment System` 于是被磨成
/// `Nintendo Famicom&Entertainment System`。攒完之后整体 `trim` 才是对的。
///
/// 也关掉结束标签校验：DAT 的 DOCTYPE 指向 `logiqx.com` 上的 DTD，而**解析器不该联网**
/// 去取它，那个域名也早就不在闸门的名单上。
#[must_use]
pub fn reader(xml: &[u8]) -> Reader<&[u8]> {
    let mut reader = Reader::from_reader(strip_bom(xml));
    reader.config_mut().check_end_names = false;
    reader
}

/// UTF-8 BOM 会让 quick-xml 在声明那一行上绊一下。
#[must_use]
pub fn strip_bom(xml: &[u8]) -> &[u8] {
    xml.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(xml)
}

/// 标签名，剥掉命名空间前缀并折成小写。
#[must_use]
pub fn tag(start: &BytesStart<'_>) -> String {
    local_name(start.name().as_ref())
}

/// 同 [`tag`]，但拿的是原始字节（结束标签那边只给得出这个）。
#[must_use]
pub fn local_name(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    match text.rsplit_once(':') {
        Some((_, local)) => local.to_ascii_lowercase(),
        None => text.to_ascii_lowercase(),
    }
}

/// 取一个属性，实体引用已还原。名字按 local name 比。
#[must_use]
pub fn attribute(tag: &BytesStart<'_>, name: &str) -> Option<String> {
    for attribute in tag.attributes().flatten() {
        if local_name(attribute.key.as_ref()) == name {
            return attribute
                .unescape_value()
                .ok()
                .map(|value| value.into_owned());
        }
    }
    None
}

/// 一个实体引用展开成什么。
///
/// quick-xml 不替调用方展开命名实体（文档类型可以自定义），但 XML 的**预定义**那五个
/// 是标准里写死的，DAT 里出现的也只有这五个（`Tom &amp; Jerry`）。认不出的原样留下
/// `&名字;`，比悄悄吞掉一段文字好。
#[must_use]
pub fn entity_text(reference: &BytesRef<'_>) -> String {
    if let Ok(Some(ch)) = reference.resolve_char_ref() {
        return ch.to_string();
    }
    let name = reference.decode().unwrap_or_default();
    match name.as_ref() {
        "amp" => "&".to_string(),
        "lt" => "<".to_string(),
        "gt" => ">".to_string(),
        "quot" => "\"".to_string(),
        "apos" => "'".to_string(),
        other => format!("&{other};"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quick_xml::events::Event;

    #[test]
    fn 标签与属性都按_local_name_认() {
        // 带命名空间前缀的 DAT 少见但存在。两个解析器**必须**一致地处理它——
        // 一边按 local name 比、另一边按原始字节全等，就会一个读得动一个读不动。
        let mut reader = reader(b"\xEF\xBB\xBF<ns:ROM ns:Name=\"a.bin\" size=\"16\"/>");
        let mut buf = Vec::new();
        let Ok(Event::Empty(start)) = reader.read_event_into(&mut buf) else {
            panic!("该是一个自闭合标签");
        };
        assert_eq!(tag(&start), "rom");
        assert_eq!(attribute(&start, "name").as_deref(), Some("a.bin"));
        assert_eq!(attribute(&start, "size").as_deref(), Some("16"));
        assert_eq!(attribute(&start, "crc"), None);
    }
}
