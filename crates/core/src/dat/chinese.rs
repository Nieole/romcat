//! **从条目名里认出中文**。DAT 不给结构化的语言字段，全部编码在名字里。
//!
//! 认两件**不同的**事，绝不能并成一个数（ADR-0012、`CONTEXT.md`）：
//!
//! - **汉化版**是民间打补丁产生的中文版本，是一个**变体**。它对不上任何官方数据库，
//!   而 TOSEC 与 GoodNES 恰恰收录了它——这正是这两个源对这个库的全部价值。
//! - **官中版**是原厂发行的官方中文版本。在有独立序列号的卡带与光盘世代它是一个独立的
//!   **发行版**，No-Intro 与 Redump 用语言标记正常收录。
//!
//! 把两者加成一个「中文条目数」，会让「TOSEC 补的正是官方库覆盖不到的那部分」这个
//! 判断彻底失真——PSV 有 290 条官中、0 条汉化，而 NGPC 反过来。
//!
//! ## 记号从哪儿来
//!
//! | 记号 | 出处 | 实测 |
//! |---|---|---|
//! | `[tr zh …]` | TOSEC 命名规范，`tr` 后跟 ISO 639-1 语言码 | 本工具在 TOSEC 官方 release 的 309 份相关 DAT 里实测 499 条 |
//! | `[T+Chi…]` / `[T-Chi…]` | GoodTools。`GoodCodes.txt` 只定义 `[T-]`=旧译、`[T+]`=新译，**语言子码是事实惯例不是官方码表** | 本工具实测 `gamedb_goodnes.txt` 646 条、`gamedb_sega_md.txt` 102 条 |
//! | `(… Zh …)` | No-Intro / Redump 的语言标记组 | PSV 290 条、Xbox 360 约 305 条官中 |
//!
//! **数中文条目要数 `<game>` 而不是数字符串出现次数。** 调研记的 TOSEC「960 条」
//! 每个平台都恰好是实测值的两倍——那个计数把 `<game name>` 与 `<description>` 各数了
//! 一次。同一条条目在一份 DAT 里至少写三遍（条目名、描述、文件名）。
//!
//! **GoodTools 的 `(Ch)` 不算。** 它是国别码「中国区」，说的是发行地区不是谁做的中文；
//! `gamedb_sega_md.txt` 里有 70 条。调研记的「MD 172 条中文」正是 102 条汉化加这 70 条，
//! 两者混在一起，就分不出「TOSEC/GoodNES 补的是官方库覆盖不到的那部分」这个判断了。

/// 一条 DAT 条目上的中文记号。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ChineseMark {
    /// **汉化版**：民间补丁产生的中文版本，是一个**变体**。
    FanTranslated,
    /// **官中版**：原厂发行的官方中文版本。
    Official,
}

impl ChineseMark {
    /// 报告里写的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::FanTranslated => "汉化",
            Self::Official => "官中",
        }
    }
}

/// 这条 DAT 条目名说自己是中文的哪一种。
///
/// **汉化优先于官中**：一条 `[tr zh]` 条目完全可能同时带着底版的 `(Ja,Zh)` 语言标记，
/// 而它是汉化版——底版说什么语言不改变这件事。
#[must_use]
pub fn mark_of(name: &str) -> Option<ChineseMark> {
    if groups(name, '[', ']').any(is_fan_translation) {
        return Some(ChineseMark::FanTranslated);
    }
    if groups(name, '(', ')').any(has_chinese_language) {
        return Some(ChineseMark::Official);
    }
    None
}

/// 名字里由 `open` / `close` 括起来的每一段（不含括号本身）。
///
/// 不处理嵌套：DAT 的命名规范里这些标记本来就是平铺的，而按最近的 `close` 收口
/// 在畸形名字上也不会走飞。
fn groups(name: &str, open: char, close: char) -> impl Iterator<Item = &str> {
    let mut rest = name;
    std::iter::from_fn(move || {
        let start = rest.find(open)? + open.len_utf8();
        let body = &rest[start..];
        // 有开无合：后面没有完整的段了。
        let end = body.find(close)?;
        rest = &body[end + close.len_utf8()..];
        Some(&body[..end])
    })
}

/// `[…]` 里这一段是不是「中文汉化」。
fn is_fan_translation(group: &str) -> bool {
    let folded = group.to_ascii_lowercase();
    // TOSEC：`tr zh`、`tr zh-Hans`、`tr zh 某某组`。`tr` 与语言码之间必有空格。
    if let Some(rest) = folded.strip_prefix("tr ")
        && is_chinese_code(first_token(rest.trim_start()))
    {
        return true;
    }
    // GoodTools：`T+Chi`、`T-Chi`、`T+Chi1.0_组名`、`T+Chi_MS emumax`、
    // `T+ChiNewWordsAdded_Jiuban`。版本号、说明与组名**直接粘在语言码后面**
    // （`GoodCodes.txt` 只定义了 `T+`/`T-`，语言子码与后缀都是事实惯例），
    // 所以只能看前缀，且**不能对后面跟着什么做假设**——曾经为了躲开「Chinese」
    // 这个词排掉了 `chin` 开头的，结果把真实存在的 `[T+ChiNewWordsAdded_Jiuban]`
    // 两条一起排掉了。没有别的语言码以 `Chi` 开头，前缀就够。
    for prefix in ["t+", "t-"] {
        if let Some(rest) = folded.strip_prefix(prefix)
            && rest.starts_with("chi")
        {
            return true;
        }
    }
    false
}

/// `(…)` 里这一段是不是 No-Intro / Redump 的语言标记组，且含中文。
///
/// 整组每一项都得像个语言码才算——不然 `(Zhu Zhu Pets)` 这种游戏名会被当成语言标记。
fn has_chinese_language(group: &str) -> bool {
    let items: Vec<&str> = group.split(',').map(str::trim).collect();
    if items.is_empty() || !items.iter().all(|item| looks_like_language(item)) {
        return false;
    }
    items.iter().any(|item| is_chinese_code(item))
}

/// 语言码长这样：两个字母，或者两个字母加一个 `-` 加一段文字（`Zh-Hans`、`Pt-BR`）。
fn looks_like_language(item: &str) -> bool {
    let (head, tail) = match item.split_once('-') {
        Some((head, tail)) => (head, Some(tail)),
        None => (item, None),
    };
    head.len() == 2
        && head.chars().all(|c| c.is_ascii_alphabetic())
        && tail
            .is_none_or(|tail| !tail.is_empty() && tail.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// `zh`、`zh-hans`、`zh-hant` 之类。
fn is_chinese_code(item: &str) -> bool {
    let folded = item.to_ascii_lowercase();
    folded == "zh" || folded.starts_with("zh-")
}

/// 一段里的第一个词。
fn first_token(text: &str) -> &str {
    text.split_whitespace().next().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tosec_的汉化标记认得出() {
        // 这四条是调研在 TOSEC 官方 release 里实测到的**全部** `[tr zh]` 条目
        // （`docs/research/rom-identification-part2.md` 1.4）。
        for name in [
            "Dark Arms - Beast Buster 1999 (1999)(SNK)(en-ja)[tr zh]",
            "Metal Slug - 1st Mission (1999)(SNK)(en-ja)[tr zh]",
            "Metal Slug - 1st Mission (1999)(SNK)(en-ja)[tr zh][a]",
            "Kidou Senshi Gundam Seed (2003-03-15)(Bandai)[tr zh][v.20040120]",
        ] {
            assert_eq!(mark_of(name), Some(ChineseMark::FanTranslated), "{name}");
        }
        assert_eq!(
            mark_of("Some Game (1999)(SNK)[tr zh-Hans 某某汉化组]"),
            Some(ChineseMark::FanTranslated)
        );
    }

    #[test]
    fn 别的语言的翻译版不算中文() {
        // 同一批 DAT 里 85 条翻译版只有 4 条是中文，其余是俄英葡西法德。
        for name in [
            "Alone in the Dark (1994)(Pony Canyon)(JP)[tr ru aliast]",
            "Switchblade II (1992)(Atari Corp)[tr es Wave]",
            "Something (1990)(X)[tr en]",
            "Something (1990)(X)[tr de]",
        ] {
            assert_eq!(mark_of(name), None, "{name}");
        }
    }

    #[test]
    fn goodnes_的汉化标记认得出() {
        // 这三条抄自 `gamedb_goodnes.txt` 真实的行。
        for name in [
            "1942 (JU) [T+Chi_MS emumax]",
            "1944 [p1][T+Chi_MS emumax]",
            "4 Nin Uchi Mahjong (J) (PRG1) [T+Chi]",
            "Some Game (J) [T-Chi1.0_Group]",
            // 这两条真实存在于 `gamedb_goodnes.txt`。它们曾经被一条「躲开 Chinese」
            // 的规则连坐排掉，646 条只认出 644 条。
            "Shui Guo Li (Ch) [T+ChiNewWordsAdded_Jiuban]",
            "Tank 1990 (Ch) [T+ChiNewWordsAdded_Jiuban]",
        ] {
            assert_eq!(mark_of(name), Some(ChineseMark::FanTranslated), "{name}");
        }
        // 别的语言的 GoodTools 翻译版
        for name in [
            "Some Game (J) [T+Eng1.00_Group]",
            "Alone (1994)(X)[T+Rus]",
            "Something [T-Por]",
        ] {
            assert_eq!(mark_of(name), None, "{name}");
        }
    }

    #[test]
    fn no_intro_的官中语言标记认得出() {
        for name in [
            "Some Game (Japan) (Zh)",
            "Some Game (Asia) (En,Zh-Hans)",
            "Some Game (Taiwan) (Ja,Zh-Hant)",
        ] {
            assert_eq!(mark_of(name), Some(ChineseMark::Official), "{name}");
        }
    }

    #[test]
    fn 游戏名里恰好有个像语言码的词不算官中() {
        // 「整组每一项都得像语言码」这条防的就是这个。
        assert_eq!(mark_of("Zhu Zhu Pets (USA)"), None);
        assert_eq!(mark_of("Some Game (Zh Zh Something)"), None);
        assert_eq!(mark_of("Some Game (USA) (Rev 1)"), None);
    }

    #[test]
    fn 汉化压过官中() {
        // 汉化版是**变体**、官中版是**发行版**，一条条目只能算一次，
        // 而「谁做的这个中文」由汉化标记说了算。
        assert_eq!(
            mark_of("Some Game (Japan) (Ja,Zh)[tr zh 某组]"),
            Some(ChineseMark::FanTranslated)
        );
    }

    #[test]
    fn 括号没配对也不会走飞() {
        assert_eq!(mark_of("Some Game (Japan"), None);
        assert_eq!(mark_of("Some Game [tr zh"), None);
        assert_eq!(mark_of(""), None);
    }
}
