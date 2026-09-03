//! 合成数据：一份不碰主库就能撑起十万行表格的**中立库**。
//!
//! **主库只读**（ADR-0004），而验证「十万行滚得动」这件事不需要真库——它要的只是十万个
//! 变体。所以这里就地造一份内存里的中立库，走的是与真库**同一套表、同一条查询路径**：
//! 那正是这份合成数据的意义，若是绕过 SQLite 直接造一个 `Vec`，量出来的帧率就与真实
//! 情况无关了。
//!
//! 名字里**故意混进中日文、繁体、假名与符号**。表格里画的字若全是 ASCII，
//! 字体那条验收就等于没测——豆腐块只会在真的要画汉字时才出现。

use romcat_core::catalog::{Catalog, CatalogError};
use romcat_core::platform::Manifest;
use romcat_core::shape::{SINGLE_FILE_RULE, SPLIT_VOLUME_RULE, Variant};

/// 造名字用的作品名。繁简、假名、罗马数字、带圈数字、音符、星号各占几条。
const WORKS: &[&str] = &[
    "幻想传说",
    "潛龍諜影 Ⅲ",
    "ゼルダの伝説 ～時のオカリナ～",
    "勇者鬥惡龍Ⅺ",
    "皇家騎士團 ①",
    "最终幻想 Ⅶ ★特别版★",
    "洛克人 X ♪サウンドトラック付",
    "女神轉生 Ⅱ",
    "机器人大战 α",
    "圣剑传说 3 · 汉化版",
    "モンスターハンター ポータブル",
    "太空戰士 Ⅵ（繁中）",
    "秘密の花園 ♥",
    "三国志曹操传",
    "鬼武者 ～Onimusha～",
    "英雄伝説 空の軌跡 FC",
    "口袋妖怪 · 绿宝石 ①②③",
    "街霸 Ⅱ ターボ",
    "仙劍奇俠傳",
    "轩辕剑外传 · 天之痕",
];

/// 平台目录名。真库就是按平台分目录的（ADR-0011），合成数据照做。
const PLATFORMS: &[&str] = &[
    "SFC", "PS1", "PS2", "PSP", "NDS", "GBA", "MD", "N64", "SS", "DC", "WII", "PSV", "3DS", "NSW",
    "FC", "MAME", "PCE",
];

/// 汉化组的记号，真库里的文件名带这类后缀。
const MARKS: &[&str] = &[
    "汉化版",
    "官中",
    "日版",
    "美版",
    "繁中",
    "UnDUB",
    "英化",
    "同人移植",
];

/// 造一个装着 `rows` 个变体的内存中立库。
///
/// # Errors
/// 建库或写库失败时返回错误。
pub fn synthetic(rows: u64) -> Result<Catalog, CatalogError> {
    let mut catalog = Catalog::open_in_memory()?;
    let variants: Vec<Variant> = (0..rows)
        .map(|i| {
            let platform = PLATFORMS[(i as usize) % PLATFORMS.len()];
            let work = WORKS[(i as usize / 3) % WORKS.len()];
            let mark = MARKS[(i as usize / 7) % MARKS.len()];
            let key = format!("{platform}/{work}（{mark}）#{i:06}.zip");
            Variant {
                main_key: key.clone(),
                key,
                // 每十七个留一个**平台未知**：那是真库里存在的一档，
                // 筛选与排序都得能处理它。
                platform: (i % 17 != 3).then(|| platform.to_string()),
                // 成型规则只能是真有的那几条：库里绝大多数是一文件一变体，
                // 分卷压缩是少数（`docs/library-facts.md`）。
                rule: if i % 11 == 0 {
                    SPLIT_VOLUME_RULE.to_string()
                } else {
                    SINGLE_FILE_RULE.to_string()
                },
                manual: false,
                files: 1 + i % 9,
                // 乘一个质数再取模，让容量既不单调也不重复太多——按容量排序时
                // 那才是个真的排序。
                bytes: (i.wrapping_mul(2_654_435_761)) % 8_000_000_000,
                unreadable_files: 0,
                members: Vec::new(),
            }
        })
        .collect();
    catalog.replace_variants(&variants, 1, &Manifest::default())?;
    Ok(catalog)
}
