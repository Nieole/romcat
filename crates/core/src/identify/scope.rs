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
//! ## 「裁决说它没有发行版」不在这里
//!
//! 那一条走**沉淀库**：`verdict::Decision::NoRelease` 是一条**明说**的裁决记录，
//! 由 [`identify`](super) 在撞库之前读掉。这里曾经拿「作品有、发行版空」去推它，
//! 而那个形状同时也是「裁决定了作品、识别认出了发行版」的样子——两者混着读，
//! 后一种会被误判成 homebrew 跳过（原挂账 D48）。**判据只认明说的那一条。**
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

/// 一个变体里**能看见的内容名字**，连一句「这份名单靠不靠得住」。
///
/// 两样必须一起走，所以捏成一个类型：**穿不透**的容器交出来的是一份空名单，而空在那里
/// 的意思是「没看见」，不是「里面没有」（`CONTEXT.md` 的「穿不透」条）。只把名单交过来
/// 的话，[`decide`] 会对着一个根本没打开过的包说「里面也没有可运行的内容」。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Visible {
    /// 看得见的名字：**透明容器**穿透出来的内部文件名，加上变体自己的非容器成员。
    pub names: Vec<String>,
    /// 这个变体里的容器**全都**看进去了吗。
    ///
    /// 一个容器都没有时是真；**读出来了、里面确实空**也是真（那是看过的结论）；
    /// 只有**穿不透**与**还没读过**才是假——那两种交出来的空名单是「没看见」。
    pub saw_inside: bool,
}

impl Visible {
    /// 一个还什么都没看见、但也还没碰上看不进去的东西的名单。
    #[must_use]
    pub fn new() -> Self {
        Self {
            names: Vec::new(),
            saw_inside: true,
        }
    }
}

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
            Self::Patch(_) => PATCH,
            Self::NoRelease(_) => NO_RELEASE,
        }
    }

    /// 具体是凭什么判的，给人看的一句。
    #[must_use]
    pub fn detail(&self) -> &str {
        match self {
            Self::Patch(detail) | Self::NoRelease(detail) => detail,
        }
    }

    /// 落进中立库 `identification.reason` 那一列的那句话。
    ///
    /// **写与读必须共用它。** 导出要挡下补丁（补丁不可运行，做不成前端条目），
    /// 判据只能从这一列读回来——识别那一趟才有容器内容可看，导出这一趟没有。
    /// 两处各写一遍格式串，改一次就会有一处对不上。
    #[must_use]
    pub fn recorded(&self) -> String {
        format!("{}：{}", self.kind(), self.detail())
    }

    /// 从库里那句话认回它是哪一类；认不出是 `None`。
    #[must_use]
    pub fn kind_in(recorded: &str) -> Option<&'static str> {
        [PATCH, NO_RELEASE]
            .into_iter()
            .find(|kind| recorded.starts_with(&format!("{kind}：")))
    }
}

/// 「补丁」那一类在报告与库里叫什么。
pub const PATCH: &str = "补丁";

/// 「没有发行版链接」那一类在报告与库里叫什么。
pub const NO_RELEASE: &str = "没有发行版链接";

/// 补丁的扩展名。它们一个都不在 [`classify`] 的三类主线里——补丁不是内容。
const PATCH_EXTENSIONS: &[&str] = &[
    "ips", "ups", "bps", "aps", "ppf", "xdelta", "xdelta3", "vcdiff", "rup", "dps", "ebp",
];

/// 名字里出现这些词，说明它自称是补丁。
const PATCH_WORDS: &[&str] = &["补丁", "patch"];

/// TitleID 那条判据只对这个平台生效。
///
/// **不能对所有平台生效**：`ULUS10041.iso`（PSP）、`BLJS10250`（PS3）、
/// `BCAS20228`（PS3 的**官中版**）在形态上都是「四个字母加五位数字」，而下面那张
/// 官方前缀表只列了 PSV 的。不限定平台的话，这条判据会把整批官中版挡在识别管线之外
/// ——那正是 ADR-0012 说「官中版走精确哈希直接过」要避免的事。
const TITLE_ID_PLATFORM: &str = "PSV";

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
/// 补丁的判据要看 [`Visible`]——一个名字叫「汉化补丁」的 zip，里面若装着一份完整的
/// ROM，那它就不是补丁；而里面**看不进去**时，这句话根本说不出口。
#[must_use]
pub fn decide(variant: &VariantRow, visible: &Visible) -> Option<Skip> {
    let name = file_name_of_key(&variant.key);
    if let Some(skip) = patch_of(name, &variant.main_key, visible) {
        return Some(skip);
    }
    homebrew_of(name, variant.platform.as_deref())
}

/// 这是不是一个**补丁**。
fn patch_of(name: &str, main_key: &str, visible: &Visible) -> Option<Skip> {
    let contents = &visible.names;
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
    // 名字自称是补丁，而里面**确实**没有可运行的东西。
    //
    // **看不进去就不算**：容器穿不透时 `contents` 是空的，而那是「没看见」不是
    // 「里面没有」。真机上 39 个 `.tar.zst`（`怪物猎人4G[ACG汉化组]+最终汉化补丁.tar.zst`
    // 之流）正是这样被判成补丁的——它们是游戏**加**补丁，不是补丁。错跳一个的代价是
    // 一个本来认得出来的变体永远不进识别管线，而漏跳一个只是一条落空的撞库。
    if visible.saw_inside
        && let Some(word) = patch_word(name)
    {
        return Some(Skip::Patch(format!(
            "名字里写着「{word}」，里面也没有可运行的内容"
        )));
    }
    None
}

/// 这是不是**没有发行版链接**的那一类：同人移植与 homebrew。
fn homebrew_of(name: &str, platform: Option<&str>) -> Option<Skip> {
    if platform == Some(TITLE_ID_PLATFORM)
        && let Some(id) = title_id_prefix(name)
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
///
/// 形状本身认在 [`serial::title_id_head`](super::serial::title_id_head)——那儿还拿它
/// 当**依据**（票 09）。**同一个形状只写一处**：两边各写一遍，改一次判据就会有一边
/// 跳过、另一边当依据，而那两个结论互相矛盾。
fn title_id_prefix(name: &str) -> Option<String> {
    super::serial::title_id_head(name).map(|id| id.chars().take(4).collect())
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

    fn 看得见(names: &[&str], saw_inside: bool) -> Visible {
        Visible {
            names: names.iter().map(ToString::to_string).collect(),
            saw_inside,
        }
    }

    #[test]
    fn 汉化补丁包被认出来并跳过() {
        // 真库里就躺着这一个（票面点名的例子）。
        let v = 变体("PSV/《夏莉的炼金工房 ~黄昏海洋的炼金术士~ Plus》汉化补丁 Ver.1.0.zip");
        let skip = decide(&v, &看得见(&["ATELIER.ppf", "使用说明.txt"], true)).expect("该跳过");
        assert_eq!(skip.kind(), "补丁");
        assert!(skip.detail().contains("ppf"), "{}", skip.detail());
    }

    #[test]
    fn 名字自称补丁但里面装着_rom_的不算补丁() {
        // 判据是能否独立运行，不是名字。名字说了算的话，装着完整 ROM 的
        // 「汉化补丁合集.zip」会被整个挡在识别管线之外。
        let v = 变体("FC/某某汉化补丁合集.zip");
        assert_eq!(
            decide(&v, &看得见(&["游戏 (Japan).nes", "readme.txt"], true)),
            None
        );
    }

    #[test]
    fn 主文件本身是补丁格式的直接跳过() {
        let v = 变体("FC/某游戏汉化.ips");
        let skip = decide(&v, &看得见(&[], true)).expect("该跳过");
        assert_eq!(skip.kind(), "补丁");
    }

    #[test]
    fn 自造_titleid_是同人移植不是发行版() {
        // `AIME` 不是 Sony 发出去的前缀，No-Intro 的 PSV 集里不会有它。
        let v = 变体("PSV/AIME00001(wan华镜 v3.1)");
        let skip = decide(&v, &看得见(&[], true)).expect("该跳过");
        assert_eq!(skip.kind(), "没有发行版链接");
        assert!(skip.detail().contains("AIME"), "{}", skip.detail());

        // 官方前缀照常进识别管线。
        assert_eq!(
            decide(&变体("PSV/PCSG00718(恋爱复仇战)"), &看得见(&[], true)),
            None
        );
    }

    #[test]
    fn 别的平台的序列号形态不当成自造_titleid() {
        // `ULUS10041`（PSP）、`BCAS20228`（PS3 的官中版）形态上都是四字母加五位数字。
        // 那张官方前缀表只列了 PSV 的，不限定平台就会把整批官中版挡在管线之外
        // ——ADR-0012 说的正是「官中版走精确哈希直接过」。
        let mut psp = 变体("psp/ULUS10041.iso");
        psp.platform = Some("PSP".to_string());
        assert_eq!(decide(&psp, &看得见(&[], true)), None);

        let mut ps3 = 变体("ps3/BCAS20228");
        ps3.platform = Some("PS3".to_string());
        assert_eq!(decide(&ps3, &看得见(&[], true)), None);
    }

    #[test]
    fn 自制应用没有发行版() {
        let v = 变体("PSV/mGBA-0.10.0-vita.7z");
        let skip = decide(&v, &看得见(&["mGBA.vpk"], true)).expect("该跳过");
        assert_eq!(skip.kind(), "没有发行版链接");
    }

    #[test]
    fn 作品有发行版空这个形状本身不再当判据() {
        // 原挂账 D48：这个形状既可能是「裁决说它没有发行版」，也可能是「裁决定了作品、
        // 识别认出了发行版」——照着它跳过，后一种就被误判成 homebrew 了。
        // 「没有发行版」如今是沉淀库里一条明说的裁决，由识别在撞库之前读掉。
        let mut v = 变体("PS1/某同人移植.chd");
        v.work_id = Some(7);
        v.release_id = None;
        assert_eq!(decide(&v, &看得见(&[], true)), None);
        assert_eq!(decide(&变体("PS1/某游戏.chd"), &看得见(&[], true)), None);
    }

    #[test]
    fn 看不进去的包不许被说成里面没有可运行的内容() {
        // 真机上 39 个 `.tar.zst` 撞的正是这一条：容器穿不透，`contents` 是空的，
        // 而那是「没看见」不是「里面没有」。它们是游戏**加**补丁，不是补丁。
        let v = 变体("3ds/怪物猎人4G[ACG汉化组]+最终汉化补丁.tar.zst");
        assert_eq!(
            decide(&v, &看得见(&[], false)),
            None,
            "看不进去就不该下这个判断"
        );
        // 看得进去、里面确实只有说明文档，那才算。
        let skip = decide(&v, &看得见(&["说明.txt"], true)).expect("该跳过");
        assert_eq!(skip.kind(), "补丁");
        // **主文件本身就是补丁格式**那一条不受影响——它压根不看包里有什么。
        let ips = 变体("FC/某游戏汉化.ips");
        assert!(decide(&ips, &看得见(&[], false)).is_some());
    }

    #[test]
    fn 整词才算不然_dispatcher_也成了补丁() {
        assert!(has_word("game.patch.zip", "patch"));
        assert!(!has_word("dispatcher.zip", "patch"));
        assert_eq!(
            decide(&变体("PSV/dispatcher.zip"), &看得见(&[], true)),
            None
        );
    }

    #[test]
    fn 普通游戏一律照撞() {
        assert_eq!(
            decide(&变体("FC/超级马里奥.zip"), &看得见(&["smb.nes"], true)),
            None
        );
        assert_eq!(decide(&变体("PS1/游戏.chd"), &看得见(&[], true)), None);
    }
}
