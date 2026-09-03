//! **第三方 TitleID 数据库**：Switch 那一层的弹药，而它**不是一份 DAT**。
//!
//! ## 为什么不是 DAT
//!
//! 调研把那个每日镜像里的 **334 份 DAT 全查了一遍，Switch 命中 0 个**（`no-intro.xml`
//! 里 Nintendo 相关 70 份，最新的世代只到 Wii U）；Redump 没有 Switch 也永远不会有
//! （Switch 是卡带加数字发行，没有光盘）；libretro-database 里零个 Switch 平台。
//! DAT-o-MATIC 站点上确实有 Switch 集，但**绝不直连它**——一次参数畸形的请求就会触发
//! 永久 IP 封禁（ADR-0007，`dat::guard`）。
//!
//! 所以 Switch 的识别依据只能来自别处。这一份就是那个别处：
//! [blawar/titledb](https://github.com/blawar/titledb)。
//!
//! ## 许可与可自动化获取性（票 27 要求实现时确认的那一条）
//!
//! | 问 | 答 | 出处 |
//! |---|---|---|
//! | 许可 | **MIT**（Copyright (c) 2019 Blake Warner），可再分发派生索引 | 仓库根的 `LICENSE` |
//! | 怎么取 | `raw.githubusercontent.com` 单文件 `GET`，无需 token、无速率问题 | 调研 §4.3 实测 |
//! | **绝不** | `git clone`——仓库体积约 44 GB，而要的只有几个文件 | 调研的实现陷阱第 5 条 |
//! | 更新节奏 | 每日推送 | 调研日与 `pushed_at` 只差 1 天 |
//! | 走哪道闸门 | [`dat::guard`](crate::dat::guard)，`raw.githubusercontent.com` **本来就在白名单上** | 票 06 |
//!
//! 最后一行是有意的：这一层**一个主机都不往白名单里加**。加一条白名单是一个需要想清楚
//! 的决定，不该由一份新数据源顺手带进来（与[中文离线源那一层](crate::zh::sync)同理）。
//!
//! ## 取哪几个文件
//!
//! | 文件 | 键 | 干什么 |
//! |---|---|---|
//! | `cnmts.json`（约 50 MB） | `titleId` → `version` → `contentEntries[].ncaId` | **反查的核心**：ContentId → (TitleID, 版本) |
//! | `{区}.{语}.json`（各 50–95 MB） | `nsuId` | 名字、发行商、**语言**——[`Facts`](crate::identify::switch::Facts) 说得出 TitleID 之后，作品叫什么、是不是官中，靠它 |
//!
//! **`ncas.json`（87 MB）不取。** 它是 ContentId → titleId 的更大一张表（235,227 条），
//! 但**它说不出版本**，而 `cnmts.json` 里那 173,502 个 ContentId 已经 100% 唯一映射到
//! 单个 (titleId, version)。多下 87 MB 换来的只是「说得出是哪个游戏、说不出是哪个版本」
//! 那一档——而那一档 `.tik` 免费给了（[`switch`](crate::identify::switch)）。
//!
//! ## ⭐ 中文在这里是**语言属性**，不是一条独立的发行版（ADR-0019）
//!
//! 这是这份数据源存在的第二个理由，也是它必须收**区域文件**而不是只收 `cnmts.json`
//! 的原因。调研实测：
//!
//! - 官中覆盖 **8,447 个 TitleID**（US ∪ HK 去重），是本项目全部平台里中文覆盖最好的；
//! - 而**港服与美服 69.4% 共用同一个 TitleID**——同一个文件、多语言内嵌、靠主机语言
//!   设置切换。中文在那里不是一次独立发行。
//!
//! 于是 [`Title::region`] 是这么定的：**一个 TitleID 出现在几个区的 eShop 上，它的地区
//! 就是「多区共用」那一条**（写成 `World`）；只在一个区出现的才写那个区。落到下游，
//! 前者的中文会落进发行版的 `languages` 一栏、判成
//! [`Seam::LanguageField`](crate::title::Seam::LanguageField)，后者才是
//! [`Seam::OwnRelease`](crate::title::Seam::OwnRelease)——那正是那 2,606 个亚洲独占
//! SKU 与 89 个国行 TitleID 应得的待遇（它们与其他区**零交集**）。
//!
//! 拿不到的那一档也说清楚：**繁中与简中分不开**。titledb 的 `languages` 把两种中文都
//! 写成 `zh`，要精确区分得读 NACP 的 `SupportedLanguageFlag`（bit 13 / 14）——**而那要
//! 密钥**（调研 §5.1）。这一层不走那条路。

pub mod store;
pub mod sync;

/// 一条内容反查出来的东西：哪个游戏的哪个版本。
///
/// **零冲突**：调研对 `cnmts.json` 里全部 173,502 个 `ncaId` 实测，**100.00% 只属于
/// 唯一一个 (titleId, version) 二元组**。ContentId 本身是整个 NCA 的 SHA-256 前 16 字节，
/// 天然抗碰撞——所以这一条命中等同于一次精确哈希命中。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Content {
    /// TitleID，16 位 hex 大写。
    pub title_id: String,
    /// 版本号（`65536` = v1.0.1 那种，Nintendo 的编法）。
    pub version: u32,
}

/// 一个 TitleID 在 eShop 上的样子。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Title {
    /// TitleID，16 位 hex 大写。
    pub title_id: String,
    /// eShop 上的名字。
    pub name: String,
    /// 发行商。
    pub publisher: Option<String>,
    /// 语言，折成 No-Intro 那种写法（`En,Ja,Zh`）。**中文落在这儿**（ADR-0019）。
    pub languages: Option<String>,
    /// 它在哪几个区的 eShop 上出现过，按字典序。
    pub regions: Vec<String>,
}

impl Title {
    /// 这个 TitleID 的**地区**该写什么。
    ///
    /// **多个区共用同一个 TitleID 时写 `World`**：那不是「香港版」，那是同一次发行卖到
    /// 好几个区（港服与美服实测 69.4% 如此）。只在一个区出现的才写那个区——那才是真正
    /// 的亚洲独占 SKU 与国行。
    #[must_use]
    pub fn region(&self) -> Option<&str> {
        match self.regions.len() {
            0 => None,
            1 => self.regions.first().map(String::as_str),
            _ => Some("World"),
        }
    }

    /// 折成一条 No-Intro 风格的条目名：`正题 (地区) (语言)`。
    ///
    /// **为什么要折成这个形状**：下游那一整条路（[`naming::parse`](crate::identify::naming::parse)
    /// 读作品名与语言、[`chinese::mark_of`](crate::dat::chinese::mark_of) 认官中、
    /// `title` 判世代裂缝）读的都是这一种名字。另造一套字段等于让 Switch 在下游处处
    /// 分叉，而 ADR-0019 要的恰恰相反——**模型跟着现实分层，而分层已经在 `region` 上
    /// 表达完了**。
    #[must_use]
    pub fn entry_name(&self) -> String {
        let mut out = self.name.clone();
        if let Some(region) = self.region() {
            out.push_str(&format!(" ({region})"));
        }
        if let Some(languages) = &self.languages {
            out.push_str(&format!(" ({languages})"));
        }
        out
    }
}

/// 要收哪几个区的 eShop 元数据。
///
/// 四个区各有各的理由，不是随手挑的（调研 §4.3、§5.1、§5.2）：
///
/// | 文件 | 地区 | 为什么要它 |
/// |---|---|---|
/// | `US.en.json` | `USA` | 最大的一份（37,179 条），英文名的主来源；含中文的 TitleID 6,946 个 |
/// | `HK.zh.json` | `Hong Kong` | **港服就是繁中区**（`cdn.regions.json` 里 `TWN → ["HK"]`），含中文 5,207 个 |
/// | `JP.ja.json` | `Japan` | 日文原名，含中文 6,554 个 |
/// | `CN.zh.json` | `China` | **国行**，89 个 TitleID 与其他区**零交集**，是真正独立的一档 |
pub const REGIONS: &[(&str, &str)] = &[
    ("US.en.json", "USA"),
    ("HK.zh.json", "Hong Kong"),
    ("JP.ja.json", "Japan"),
    ("CN.zh.json", "China"),
];

/// 把 titledb 的 `languages`（`["en","ja","zh"]`）折成 No-Intro 那种写法（`En,Ja,Zh`）。
///
/// **去重**：HK 区实测 5,366 个含 `zh` 的条目里有 4,377 条把 `"zh"` 写了两次（繁中与
/// 简中两个 bit），而这一层区分不了它们（要 NACP，要密钥）。写两遍只会让下游以为
/// 有两种语言。
#[must_use]
pub fn languages_of(codes: &[String]) -> Option<String> {
    let mut out: Vec<String> = Vec::new();
    for code in codes {
        let mut chars = code.chars();
        let folded = match chars.next() {
            Some(first) => format!(
                "{}{}",
                first.to_ascii_uppercase(),
                chars.as_str().to_ascii_lowercase()
            ),
            None => continue,
        };
        if !out.contains(&folded) {
            out.push(folded);
        }
    }
    (!out.is_empty()).then(|| out.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 一条(regions: &[&str], languages: Option<&str>) -> Title {
        Title {
            title_id: "0100A0C01BED8000".to_string(),
            name: "Ys X - Nordics".to_string(),
            publisher: None,
            languages: languages.map(ToString::to_string),
            regions: regions.iter().map(ToString::to_string).collect(),
        }
    }

    #[test]
    fn 多区共用同一个_title_id_时地区是多区不是港版() {
        // ⭐ ADR-0019：港服与美服 69.4% 共用同一个 TitleID，中文在那里是语言属性。
        let 共用 = 一条(&["Hong Kong", "USA"], Some("En,Ja,Zh"));
        assert_eq!(共用.region(), Some("World"));
        assert_eq!(共用.entry_name(), "Ys X - Nordics (World) (En,Ja,Zh)");
    }

    #[test]
    fn 只在一个区出现的才是那个区独占的一次发行() {
        // 那 2,606 个亚洲独占 SKU 与 89 个国行 TitleID 就是这一档。
        assert_eq!(一条(&["Hong Kong"], None).region(), Some("Hong Kong"));
        assert_eq!(一条(&["China"], None).region(), Some("China"));
        assert_eq!(一条(&[], None).region(), None);
    }

    #[test]
    fn 语言码折成_no_intro_那种写法并且去重() {
        // titledb 把繁中与简中都写成 zh，写两遍会让下游以为有两种语言。
        let codes = ["en", "ja", "zh", "zh"].map(ToString::to_string);
        assert_eq!(languages_of(&codes).as_deref(), Some("En,Ja,Zh"));
        assert_eq!(languages_of(&[]), None);
    }
}
