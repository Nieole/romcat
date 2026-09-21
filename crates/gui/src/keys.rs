//! **快捷键表**：屏上那一份「哪个键管什么」（设计稿 `DLG.keys` 的 `keysHTML`）。
//!
//! **这张表全仓只有这一份。** 设置屏「快捷键」那一节摆的是它（票 `gui-looks-like-the-design/31`），
//! 票 `14` 那层按 `?` 打开的弹层摆的也得是它——一份表抄两遍，改一处就有一处说的是旧话。
//!
//! **待确认屏逐条那一框行内键提示（`queue::key_hints`，设计稿 `.keys`）不是这张表**：那是贴着
//! 那一屏画的一条提示带，说的是同一批键，措辞更长（「先放着（不保存，稍后仍会出现）」）。
//! 两样今天各写各的键，挂单 `Q1075` 记着——要并的话得先定「行内提示与这张表说不说同一句话」。
//!
//! ## 这份表只列**按得动**的
//!
//! 设计稿上那三组里有一多半的键要等票 `14`（切屏 `⌘ 1–6`、搜索 `⌘ F`、`?`、浏览屏的
//! 上下键与 `F` / `E` / `⌘ A`）。那一票没来之前把它们印在屏上，屏上就写着一件做不到的事
//! ——人照着按，什么都不发生。**接一个列一个**：票 `14` 接上哪个，往这份表里添哪个
//! （挂单 `Q1061`）。
//!
//! ## 修饰键两种写法
//!
//! macOS 上是 `⌘`，Windows 与 Linux 上是 `Ctrl`（[`MODIFIER`]）。这一份跟着跑的这台机器写，
//! 屏上那句话把另一种也说出来（[`NOTE`]）。

/// 修饰键在这台机器上写成什么。
pub const MODIFIER: &str = if cfg!(target_os = "macos") {
    "⌘"
} else {
    "Ctrl"
};

/// 快捷键表底下那句话（设计稿 `DLG.keys` 的 `sub`）。
pub const NOTE: &str = "在 macOS 上用 ⌘，在 Windows 和 Linux 上用 Ctrl。输入框里不响应单键快捷键。";

/// 表上一条：`(这个键管什么, 键怎么写)`。
pub type Key = (&'static str, String);

/// 表上一组：组名加它底下那几条。
pub struct Group {
    /// 组名（设计稿 `.sec`）。
    pub title: &'static str,
    /// 这一组的几条，次序照设计稿。
    pub keys: Vec<Key>,
}

/// 眼下按得动的那几条，照设计稿的分组与次序。
#[must_use]
pub fn groups() -> Vec<Group> {
    vec![
        Group {
            title: "全局",
            keys: vec![
                ("打开设置", format!("{MODIFIER} ,")),
                ("关闭对话框或菜单", "Esc".to_owned()),
            ],
        },
        Group {
            title: "浏览",
            keys: vec![
                ("打开作品详情", "Enter".to_owned()),
                ("勾选 / 取消勾选", "空格".to_owned()),
            ],
        },
        Group {
            title: "待确认 · 逐条",
            keys: vec![
                ("切换候选", "← →".to_owned()),
                ("通过所选候选", "Y".to_owned()),
                ("都不对", "N".to_owned()),
                ("先放着", "空格".to_owned()),
                ("撤销上一条", "U".to_owned()),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 表上每一条都写了这个键管什么、键怎么写——两栏都不许空。
    #[test]
    fn 每一条两栏都有字() {
        for group in groups() {
            assert!(!group.keys.is_empty(), "{} 这一组是空的", group.title);
            for (什么, 键) in group.keys {
                assert!(
                    !什么.is_empty() && !键.is_empty(),
                    "{} 那一组有半条",
                    group.title
                );
            }
        }
    }

    /// 修饰键那一条照这台机器写，不是写死的。
    #[test]
    fn 修饰键跟着这台机器() {
        let 全局 = groups().remove(0);
        assert_eq!(全局.keys[0].1, format!("{MODIFIER} ,"));
    }
}
