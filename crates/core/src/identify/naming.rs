//! 从 DAT 条目名里读出**作品**、地区、语言与**修订**。
//!
//! DAT 不给结构化字段，语义全编码在名字里（`dat::chinese` 已经在这条路上认中文记号）。
//! 这里只做四件**保守**的事，认不出就留空——识别这一层宁可少说，**刮削**（票 13/14）
//! 才是补齐元数据的地方，而错的元数据比缺的元数据难查得多。
//!
//! - **作品名**：剥掉名字尾巴上的 `(…)` 与 `[…]` 标记组。`Chrono Trigger (USA)` 与
//!   `Chrono Trigger (Japan)` 是同一部**作品**的两个**发行版**。No-Intro 的 `cloneof`
//!   说的正是这件事（ADR-0010 拿它映射 MAME 的 parent/clone），有就优先用它。
//! - **地区**：第一个 `(…)` 组，且它得在一张已知地区表里。TOSEC 的名字里第一个括号是
//!   年份，认不出就留空——瞎认会把「1990」写成地区。
//! - **语言**：整组每一项都像语言码的那一组（`(En,Ja,Zh)`）。它装的是 ADR-0019 那道
//!   世代裂缝的数字世代一侧：中文在那里是**同一条发行版的语言属性**，不另成发行版。
//! - **修订**：`(Rev 1)` / `(v1.1)` 那一组（[`revision_mark`]）。它是词表**第几版**
//!   上面那一层——官方又发了一遍，是**发行版**那一层的事实，ADR-0008 划的线正落在这儿。
//!   **没有那样一组标记就是没有**，不拿「初版」去补。

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
    /// **修订**：`(Rev 1)` / `(Rev A)` / `(v1.1)` 那一组，原样（不含括号）。
    ///
    /// 它是词表**第几版**上面那一层——**发行版**那一层，说的是官方又发了一遍
    /// （ADR-0008 划的线正落在这儿）。没有这样一组标记就是 `None`，**不拿「初版」或
    /// `1.0` 去补**（2026-09-20 拿主意的人定）：那是把「不知道」伪装成「知道」，
    /// 与 [`tosec_year`] 不认 `199x` 是同一条道理。
    pub revision: Option<String>,
}

/// 从条目名（与它的 `cloneof`）读出作品、地区、语言与修订。
#[must_use]
pub fn parse(name: &str, cloneof: Option<&str>) -> Parsed {
    let work = cloneof.map_or_else(|| work_title(name), work_title);
    let mut region = None;
    let mut languages = None;
    let mut revision = None;
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
        let language_group = is_language_list(group);
        if languages.is_none() && language_group {
            languages = Some(group.trim().to_string());
        }
        // **语言那一判照旧排在前头、口径一个字没改**：`(Rev A)` 仍旧不许被当成语言
        // （[`is_language_list`] 的注释与那条测试说的就是这件事）。变的只是它被挡下来
        // 之后有人接手了——从前挡住就没了下文，于是「这是第几版」在库里没有任何落点。
        if revision.is_none()
            && !language_group
            && let Some(mark) = revision_mark(group)
        {
            revision = Some(mark);
        }
    }
    Parsed {
        work,
        region,
        languages,
        revision,
    }
}

/// 这一组标记是**修订**吗；是就交回原样（去掉两头空白）。
///
/// 只认 No-Intro / Redump 与 TOSEC 真在用的那两种写法，**认不出一律留空**——这一层的
/// 纪律是「宁可少说」（模块文档），而错的元数据比缺的元数据难查得多：
///
/// - `Rev 1` / `Rev A` / `Rev 10`：`Rev` 后面跟一段字母数字。
/// - `v1.1` / `v1.03`：小写 `v` 后面跟一段数字打头的字。
///
/// **不认光板的 `1.1`**：TOSEC 的括号里什么都有，一串带点的数字更可能是年份或容量。
///
/// ## 为什么不复用**剥离规则**里那张版本模式表
///
/// `filename::Rules` 的 `版本模式` 认的是**维护者自己起的文件名**——`v1.2+`、`第3版`、
/// `体验版`，那是一张用户随时在补的配置（`filename/rules.toml`）。这里读的是
/// **DAT 条目名**：No-Intro / Redump / TOSEC 三家定死的词汇，十几年只有 `Rev` 与 `v` 两种写法。
///
/// 两边**收的输入不是同一类东西**，所以不是同一个判断（ADR-0024 的判据：答案与它在哪一层
/// 被问无关吗——这两个问题的答案恰恰取决于那串字是谁写的）。把用户能改的那张表接到 DAT 这一侧，
/// 等于让人改一行配置就能改变「库里这条发行版是第几版」；而**第几版那条回退链本身**
/// 只有一处（`VariantDetail::edition`），这里只是它上面那一层的读法。
fn revision_mark(group: &str) -> Option<String> {
    let trimmed = group.trim();
    let body = match trimmed.split_once(' ') {
        Some((head, tail)) if head.eq_ignore_ascii_case("rev") => tail.trim(),
        _ => {
            let tail = trimmed.strip_prefix('v')?;
            if !tail.starts_with(|c: char| c.is_ascii_digit()) {
                return None;
            }
            tail
        }
    };
    let ok = !body.is_empty()
        && body
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    ok.then(|| trimmed.to_string())
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

/// TOSEC 名字里的发行年份。
///
/// 第一个 `(…)` 是发行日期，形如 `1985`、`1985-12-11`、`199x`、`19xx`。
/// **只认得出四位数字才产出**——`199x` 说的正是「不知道是哪一年」，把它当年份写进去
/// 等于把「不知道」伪装成「知道」。
///
/// 它住在这里而不是[刮削那一侧](crate::scrape::dat)，是因为**有两个消费者**：刮削拿它
/// 填年份字段，而识别的[文件名那一层](super::fuzzy)拿它做**年份交叉校验**——同一个字段
/// 两处各解析一遍，迟早会漂开。刮削那边原样再导出一次，调用处不必改。
#[must_use]
pub fn tosec_year(name: &str) -> Option<String> {
    let first = rounds(name).next()?;
    let head = first.get(..4)?;
    if head.len() == 4 && head.chars().all(|c| c.is_ascii_digit()) {
        Some(head.to_string())
    } else {
        None
    }
}

/// 一串 DAT 条目名里读得出年份吗，取第一个读得出的。
///
/// **只有 TOSEC 的名字里有年份**（[`tosec_year`]），No-Intro 与 Redump 的没有。
/// 识别的[文件名那一层](super::fuzzy)与[中文离线源](crate::scrape::zh)都要它——
/// 那是这个库里少数几处「变体这一侧真的说得出年份」的地方，两处各走一遍同样的路，
/// 迟早会一处改了另一处没改。
#[must_use]
pub fn year_in<'a>(names: impl Iterator<Item = &'a str>) -> Option<u16> {
    names
        .filter_map(tosec_year)
        .find_map(|year| year.parse::<u16>().ok())
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
    fn 条目名尾巴上的修订标记读得出来() {
        // 词表**第几版**上面那一层：官方又发了一遍。
        assert_eq!(
            parse("Foo (Japan) (Rev 1)", None).revision.as_deref(),
            Some("Rev 1")
        );
        assert_eq!(
            parse("Foo (USA) (Rev A)", None).revision.as_deref(),
            Some("Rev A")
        );
        assert_eq!(
            parse("Foo (Japan) (v1.1)", None).revision.as_deref(),
            Some("v1.1")
        );
    }

    #[test]
    fn 没有修订标记就是没有_不拿初版去补() {
        // 2026-09-20 拿主意的人定：设计稿在这一格画的是 `1.0`，而「没有修订标记」与
        // 「这是第一版」不是同一件事——编一个出来是把不知道伪装成知道。
        assert_eq!(parse("Foo (Japan)", None).revision, None);
        assert_eq!(parse("Chrono Trigger (USA)", None).revision, None);
    }

    #[test]
    fn 语言组地区与年份都不许被当成修订() {
        // 三样都在括号里，认岔一样就会有一个错的版本号画到屏上。
        assert_eq!(parse("Foo (Japan) (En,Ja,Zh)", None).revision, None);
        assert_eq!(parse("Foo (1990)(Publisher)", None).revision, None);
        // 光板的 `1.1` 不认：TOSEC 的括号里带点的数字更可能是年份或容量。
        assert_eq!(parse("Foo (Japan) (1.1)", None).revision, None);
        // `Video` 不是 `v` 开头那一档——后面得跟数字。
        assert_eq!(parse("Foo (Video)", None).revision, None);
    }

    #[test]
    fn tosec_名字里的年份读得出来而不确定的那种读不出() {
        // 两个消费者共用这一份：刮削填年份字段，识别的文件名那一层做交叉校验。
        assert_eq!(
            tosec_year("1942 (1985-12-11)(Capcom)(JP-US)").as_deref(),
            Some("1985")
        );
        // `199x` 说的正是「不知道是哪一年」。
        assert_eq!(tosec_year("1944 (199x)(-)(AS)[p]"), None);
        // No-Intro 的名字里第一个括号是地区，不是年份。
        assert_eq!(tosec_year("1942 (Japan, USA) (En)"), None);
    }

    #[test]
    fn 一串条目名里读得出第一个年份() {
        let names = [
            "1942 (Japan, USA) (En)",
            "1942 (1985-12-11)(Capcom)(JP-US)",
            "1942 (1990)(X)",
        ];
        assert_eq!(year_in(names.into_iter()), Some(1985));
        assert_eq!(year_in(["Foo (Japan)"].into_iter()), None);
    }

    #[test]
    fn 整个名字就是一个标记组时原样留着() {
        assert_eq!(work_title("(Unknown)"), "(Unknown)");
        assert_eq!(work_title("  Foo  "), "Foo");
    }
}
