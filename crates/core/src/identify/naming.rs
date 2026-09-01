//! 从 DAT 条目名里读出**作品**、地区与语言。
//!
//! DAT 不给结构化字段，语义全编码在名字里（`dat::chinese` 已经在这条路上认中文记号）。
//! 这里只做三件**保守**的事，认不出就留空——识别这一层宁可少说，**刮削**（票 13/14）
//! 才是补齐元数据的地方，而错的元数据比缺的元数据难查得多。
//!
//! - **作品名**：剥掉名字尾巴上的 `(…)` 与 `[…]` 标记组。`Chrono Trigger (USA)` 与
//!   `Chrono Trigger (Japan)` 是同一部**作品**的两个**发行版**。No-Intro 的 `cloneof`
//!   说的正是这件事（ADR-0010 拿它映射 MAME 的 parent/clone），有就优先用它。
//! - **地区**：第一个 `(…)` 组，且它得在一张已知地区表里。TOSEC 的名字里第一个括号是
//!   年份，认不出就留空——瞎认会把「1990」写成地区。
//! - **语言**：整组每一项都像语言码的那一组（`(En,Ja,Zh)`）。它装的是 ADR-0019 那道
//!   世代裂缝的数字世代一侧：中文在那里是**同一条发行版的语言属性**，不另成发行版。

/// No-Intro / Redump 名字里出现的地区词。认不出的一律留空。
const REGIONS: &[&str] = &[
    "Japan",
    "USA",
    "Europe",
    "World",
    "Asia",
    "China",
    "Korea",
    "Taiwan",
    "Hong Kong",
    "Australia",
    "Brazil",
    "Canada",
    "France",
    "Germany",
    "Italy",
    "Netherlands",
    "Russia",
    "Spain",
    "Sweden",
    "Unknown",
];

/// 一条 DAT 条目名读出来的东西。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    /// **作品**名：剥掉尾巴上那些标记组之后的正题。
    pub work: String,
    /// 地区；认不出就是 `None`。
    pub region: Option<String>,
    /// 语言；没有语言标记组就是 `None`。
    pub languages: Option<String>,
}

/// 从条目名（与它的 `cloneof`）读出作品、地区与语言。
#[must_use]
pub fn parse(name: &str, cloneof: Option<&str>) -> Parsed {
    let work = cloneof.map_or_else(|| work_title(name), work_title);
    let mut region = None;
    let mut languages = None;
    for (index, group) in rounds(name).enumerate() {
        if index == 0 && region.is_none() {
            let trimmed = group.trim();
            if REGIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(trimmed))
            {
                region = Some(trimmed.to_string());
            }
        }
        if languages.is_none() && is_language_list(group) {
            languages = Some(group.trim().to_string());
        }
    }
    Parsed {
        work,
        region,
        languages,
    }
}

/// 剥掉尾巴上的标记组，留下正题。
///
/// 只剥**第一个标记组之后的一切**：`Final Fantasy VII (Japan) (Disc 1)` 的正题是
/// `Final Fantasy VII`。名字整个就是一个标记组时原样留着——剥成空串比留着噪音更糟。
#[must_use]
pub fn work_title(name: &str) -> String {
    let cut = [" (", " ["]
        .iter()
        .filter_map(|marker| name.find(marker))
        .min();
    let title = match cut {
        Some(at) if at > 0 => &name[..at],
        _ => name,
    };
    title.trim().to_string()
}

/// 名字里 `(…)` 括起来的每一组。
fn rounds(name: &str) -> impl Iterator<Item = &str> {
    let mut rest = name;
    std::iter::from_fn(move || {
        let start = rest.find('(')? + 1;
        let body = &rest[start..];
        let end = body.find(')')?;
        rest = &body[end + 1..];
        Some(&body[..end])
    })
}

/// 整组每一项都像语言码才算——不然 `(Rev A)` 这种也会被当成语言。
fn is_language_list(group: &str) -> bool {
    let items: Vec<&str> = group.split(',').map(str::trim).collect();
    !items.is_empty() && items.iter().all(|item| looks_like_language(item))
}

/// 语言码长这样：两个字母，或者两个字母加一个 `-` 加一段文字（`Zh-Hans`、`Pt-BR`）。
fn looks_like_language(item: &str) -> bool {
    let (head, tail) = match item.split_once('-') {
        Some((head, tail)) => (head, Some(tail)),
        None => (item, None),
    };
    let head_ok = head.len() == 2
        && head.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && head.chars().nth(1).is_some_and(|c| c.is_ascii_lowercase());
    let tail_ok =
        tail.is_none_or(|tail| !tail.is_empty() && tail.chars().all(|c| c.is_ascii_alphanumeric()));
    head_ok && tail_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 同一部作品的两个发行版读出同一个作品名() {
        assert_eq!(parse("Chrono Trigger (USA)", None).work, "Chrono Trigger");
        assert_eq!(parse("Chrono Trigger (Japan)", None).work, "Chrono Trigger");
    }

    #[test]
    fn cloneof_优先当作品名() {
        // No-Intro 的 parent/clone 说的就是「同一部作品的多个发行版」（ADR-0010）。
        let parsed = parse("Foo (Japan) (Rev 1)", Some("Foo (World)"));
        assert_eq!(parsed.work, "Foo");
    }

    #[test]
    fn 地区认得出来而年份认不出来() {
        assert_eq!(parse("Foo (Japan)", None).region.as_deref(), Some("Japan"));
        // TOSEC 的第一个括号是年份，不是地区——瞎认会把 1990 写成地区。
        assert_eq!(parse("Foo (1990)(Publisher)", None).region, None);
    }

    #[test]
    fn 语言标记组认得出来而修订号认不出来() {
        assert_eq!(
            parse("Foo (Japan) (En,Ja,Zh)", None).languages.as_deref(),
            Some("En,Ja,Zh")
        );
        assert_eq!(parse("Foo (Japan) (Rev A)", None).languages, None);
        assert_eq!(parse("Foo (Japan)", None).languages, None);
    }

    #[test]
    fn 整个名字就是一个标记组时原样留着() {
        assert_eq!(work_title("(Unknown)"), "(Unknown)");
        assert_eq!(work_title("  Foo  "), "Foo");
    }
}
