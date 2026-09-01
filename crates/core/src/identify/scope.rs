//! **哪些变体不该拿去撞 DAT**。
//!
//! 库体检证实这两类东西真实存在，而且它们撞 DAT **必然**落空：
//!
//! - **补丁**（`《夏莉的炼金工房…》汉化补丁 Ver.1.0.zip`）是把一个变体变换成另一个变体的
//!   指令文件，**自己不可运行**（`CONTEXT.md`）。DAT 收的是可运行的东西，补丁不在里面。
//! - **没有发行版链接的变体**（`AIME00001(wan华镜 v3.1)`、`mGBA-0.10.0-vita.7z`）——
//!   同人移植与 homebrew 不在任何官方数据库里。
//!
//! 让它们一路掉到模型推断（票 12）是白烧一遍，而且会产生**自信的错误候选**。所以在
//! 第一命中层就认出来、跳过，并且**单独计数**——混进「未命中」会让命中率失真。
//!
//! ## 判据宁可窄不宜宽
//!
//! 每条规则都要求**两件事同时成立**（一个标记加上「没有可运行的内容」），或者要求一个
//! 结构性的事实（TitleID 的前缀不是官方前缀）。理由是两种错的代价不对称：漏跳一个，
//! 代价是一条落空的撞库；错跳一个，代价是一个**本来认得出来的变体永远不进识别管线**。

use std::path::Path;

use crate::catalog::VariantRow;
use crate::classify::{self, Category};
use crate::path::{extension_lower, file_name_of_key};

/// 一个变体被挡在 DAT 匹配之外的理由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    /// **补丁**：不可运行，撞 DAT 必然落空。
    Patch(String),
    /// **没有发行版链接的变体**：同人移植与 homebrew 不在任何官方数据库里。
    NoRelease(String),
}

impl Skip {
    /// 报告里分组用的那个词。
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Patch(_) => "补丁",
            Self::NoRelease(_) => "没有发行版链接",
        }
    }

    /// 具体是凭什么判的，给人看的一句。
    #[must_use]
    pub fn detail(&self) -> &str {
        match self {
            Self::Patch(detail) | Self::NoRelease(detail) => detail,
        }
    }
}

/// 补丁的扩展名。它们一个都不在 [`classify`] 的三类主线里——补丁不是内容。
const PATCH_EXTENSIONS: &[&str] = &[
    "ips", "ups", "bps", "aps", "ppf", "xdelta", "xdelta3", "vcdiff", "rup", "dps", "ebp",
];

/// 名字里出现这些词，说明它自称是补丁。
const PATCH_WORDS: &[&str] = &["补丁", "patch"];

/// PSV 的**官方** TitleID 前缀。
///
/// Sony 给零售与 PSN 发行版的前缀是这几个（`PCSA`–`PCSI` 按地区分，`VCAS` / `VLAS` /
/// `VCJS` / `VLJS` / `VLJM` 是 PSP / PS1 在 Vita 上的那几档）。**同人移植与 homebrew 自造
/// 前缀**——真库里的 `AIME00001(wan华镜 v3.1)` 就是一例，`AIME` 不是 Sony 发出去的。
/// No-Intro 的 PSV 集只按官方 TitleID 收录，因此自造前缀撞 DAT 必然落空。
const OFFICIAL_TITLE_ID_PREFIXES: &[&str] = &[
    "PCSA", "PCSB", "PCSC", "PCSD", "PCSE", "PCSF", "PCSG", "PCSH", "PCSI", "VCAS", "VCJS", "VLAS",
    "VLJS", "VLJM",
];

/// 点名的 homebrew 应用。
///
/// 这是一张**实测名单**而不是通用规则：真库的 `PSV/` 顶层就躺着 `mGBA-0.10.0-vita.7z`、
/// `OpenBOR`、`NPS-0.95中文修复版.rar` 这些东西。它们是自制应用，没有发行版，
/// 任何官方数据库里都没有。名单短、只认**整词**，宁可漏不可错。
const HOMEBREW_APPS: &[&str] = &[
    "mgba",
    "openbor",
    "retroarch",
    "vitashell",
    "adrenaline",
    "ppsspp",
    "daedalusx64",
    "nps",
];

/// 这个变体该不该拿去撞 DAT；`None` 表示该撞。
///
/// `contents` 是这个变体里能看见的内容名字：**透明容器**穿透出来的内部文件名，
/// 加上变体自己的成员名。补丁的判据要看它——一个名字叫「汉化补丁」的 zip，
/// 里面若装着一份完整的 ROM，那它就不是补丁。
#[must_use]
pub fn decide(variant: &VariantRow, contents: &[String]) -> Option<Skip> {
    // 一、裁决说了它没有发行版。**这是词表定义的那条判据**：同人移植与 homebrew
    // 直接挂在作品下，没有发行版链接这件事本身就在说「别拿它去撞 DAT」。
    // 识别自己挂上去的链接不会落到这里——重跑识别的第一件事就是把它们清掉。
    if variant.work_id.is_some() && variant.release_id.is_none() {
        return Some(Skip::NoRelease("裁决记着它没有发行版".to_string()));
    }

    let name = file_name_of_key(&variant.key);
    if let Some(skip) = patch_of(name, &variant.main_key, contents) {
        return Some(skip);
    }
    homebrew_of(name)
}

/// 这是不是一个**补丁**。
fn patch_of(name: &str, main_key: &str, contents: &[String]) -> Option<Skip> {
    // 主文件本身就是补丁格式：`.ips` / `.bps` / `.ppf` 之流。
    if let Some(ext) = patch_extension(main_key) {
        return Some(Skip::Patch(format!("主文件是 .{ext} 补丁")));
    }
    // 容器里装的全是补丁：有补丁文件，且没有任何可运行的内容。
    // 两个条件缺一不可——装着补丁**也**装着 ROM 的包是个变体，不是补丁。
    let runnable = contents.iter().any(|inner| is_runnable(inner));
    if runnable {
        return None;
    }
    if let Some(inner) = contents.iter().find_map(|inner| {
        patch_extension(inner).map(|ext| (file_name_of_key(inner).to_string(), ext))
    }) {
        return Some(Skip::Patch(format!(
            "里面装的是 .{} 补丁（{}），没有可运行的内容",
            inner.1, inner.0
        )));
    }
    // 名字自称是补丁，而里面确实没有可运行的东西。
    if let Some(word) = patch_word(name) {
        return Some(Skip::Patch(format!(
            "名字里写着「{word}」，里面也没有可运行的内容"
        )));
    }
    None
}

/// 这是不是**没有发行版链接**的那一类：同人移植与 homebrew。
fn homebrew_of(name: &str) -> Option<Skip> {
    if let Some(id) = title_id_prefix(name)
        && !OFFICIAL_TITLE_ID_PREFIXES.contains(&id.as_str())
    {
        return Some(Skip::NoRelease(format!(
            "TitleID 前缀 {id} 不是官方前缀，是自造的（同人移植 / homebrew）"
        )));
    }
    let folded = name.to_lowercase();
    HOMEBREW_APPS
        .iter()
        .find(|app| has_word(&folded, app))
        .map(|app| Skip::NoRelease(format!("{app} 是自制应用，没有发行版")))
}

/// 名字开头是不是 `@@@@#####` 形态的 TitleID；是的话返回那四个字母（大写）。
fn title_id_prefix(name: &str) -> Option<String> {
    let bytes: Vec<char> = name.chars().take(9).collect();
    if bytes.len() < 9 {
        return None;
    }
    if !bytes[..4].iter().all(|c| c.is_ascii_alphabetic())
        || !bytes[4..9].iter().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    Some(bytes[..4].iter().collect::<String>().to_uppercase())
}

/// 这个名字的扩展名是不是补丁格式。
fn patch_extension(name: &str) -> Option<String> {
    let ext = extension_lower(Path::new(file_name_of_key(name)))?;
    PATCH_EXTENSIONS.contains(&ext.as_str()).then_some(ext)
}

/// 这份内容能不能独立运行——**透明容器**、**压缩镜像**与**裸文件**都算（ADR-0013：
/// 判据是能否独立运行）。媒体、文档、模拟器本体都不算。
fn is_runnable(name: &str) -> bool {
    let classification = classify::classify(Path::new(file_name_of_key(name)));
    matches!(
        classification.category,
        Category::TransparentContainer | Category::CompressedImage | Category::BareFile
    ) && classification.suspect.is_none()
}

/// 名字里有没有自称补丁的词。
fn patch_word(name: &str) -> Option<&'static str> {
    let folded = name.to_lowercase();
    PATCH_WORDS.iter().copied().find(|word| {
        if word.is_ascii() {
            has_word(&folded, word)
        } else {
            folded.contains(word)
        }
    })
}

/// ASCII 的词要按**整词**认。`patch_mobile` 是个补丁工具，而 `dispatcher` 不是。
fn has_word(haystack: &str, needle: &str) -> bool {
    let bytes = haystack.as_bytes();
    let mut from = 0;
    while let Some(at) = haystack[from..].find(needle) {
        let start = from + at;
        let end = start + needle.len();
        let before_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let after_ok = end == bytes.len() || !bytes[end].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        from = end;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::SINGLE_FILE_RULE;

    fn 变体(key: &str) -> VariantRow {
        VariantRow {
            key: key.to_string(),
            platform: Some("PSV".to_string()),
            rule: SINGLE_FILE_RULE.to_string(),
            main_key: key.to_string(),
            files: 1,
            bytes: 1,
            unreadable_files: 0,
            manual: false,
            work_id: None,
            release_id: None,
        }
    }

    fn 内容(names: &[&str]) -> Vec<String> {
        names.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn 汉化补丁包被认出来并跳过() {
        // 真库里就躺着这一个（票面点名的例子）。
        let v = 变体("PSV/《夏莉的炼金工房 ~黄昏海洋的炼金术士~ Plus》汉化补丁 Ver.1.0.zip");
        let skip = decide(&v, &内容(&["ATELIER.ppf", "使用说明.txt"])).expect("该跳过");
        assert_eq!(skip.kind(), "补丁");
        assert!(skip.detail().contains("ppf"), "{}", skip.detail());
    }

    #[test]
    fn 名字自称补丁但里面装着_rom_的不算补丁() {
        // 判据是能否独立运行，不是名字。名字说了算的话，装着完整 ROM 的
        // 「汉化补丁合集.zip」会被整个挡在识别管线之外。
        let v = 变体("FC/某某汉化补丁合集.zip");
        assert_eq!(decide(&v, &内容(&["游戏 (Japan).nes", "readme.txt"])), None);
    }

    #[test]
    fn 主文件本身是补丁格式的直接跳过() {
        let v = 变体("FC/某游戏汉化.ips");
        let skip = decide(&v, &[]).expect("该跳过");
        assert_eq!(skip.kind(), "补丁");
    }

    #[test]
    fn 自造_titleid_是同人移植不是发行版() {
        // `AIME` 不是 Sony 发出去的前缀，No-Intro 的 PSV 集里不会有它。
        let v = 变体("PSV/AIME00001(wan华镜 v3.1)");
        let skip = decide(&v, &[]).expect("该跳过");
        assert_eq!(skip.kind(), "没有发行版链接");
        assert!(skip.detail().contains("AIME"), "{}", skip.detail());

        // 官方前缀照常进识别管线。
        assert_eq!(decide(&变体("PSV/PCSG00718(恋爱复仇战)"), &[]), None);
    }

    #[test]
    fn 自制应用没有发行版() {
        let v = 变体("PSV/mGBA-0.10.0-vita.7z");
        let skip = decide(&v, &内容(&["mGBA.vpk"])).expect("该跳过");
        assert_eq!(skip.kind(), "没有发行版链接");
    }

    #[test]
    fn 裁决说了没有发行版就不再撞_dat() {
        // 词表定义的那条判据：作品有、发行版没有，本身就是「别撞 DAT」的信号。
        let mut v = 变体("PS1/某同人移植.chd");
        v.work_id = Some(7);
        v.release_id = None;
        let skip = decide(&v, &[]).expect("该跳过");
        assert_eq!(skip.kind(), "没有发行版链接");

        // 两个都空是「还没识别过」，不是「没有发行版」。
        assert_eq!(decide(&变体("PS1/某游戏.chd"), &[]), None);
    }

    #[test]
    fn 整词才算不然_dispatcher_也成了补丁() {
        assert!(has_word("game.patch.zip", "patch"));
        assert!(!has_word("dispatcher.zip", "patch"));
        assert_eq!(decide(&变体("PSV/dispatcher.zip"), &[]), None);
    }

    #[test]
    fn 普通游戏一律照撞() {
        assert_eq!(
            decide(&变体("FC/超级马里奥.zip"), &内容(&["smb.nes"])),
            None
        );
        assert_eq!(decide(&变体("PS1/游戏.chd"), &[]), None);
    }
}
