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
//! **接一个列一个**——表上不许印按不动的键：印上去，人照着按，什么都不发生（挂单 `Q1061`）。
//! 票 `14` 把设计稿那三组里剩下的十条接通了，于是十条一并添了进来：切屏 `⌘ 1–6`、
//! 搜索 `⌘ F`、`?`、浏览屏的上下键与 `Enter` / `空格` / `⌘ A` / `F` / `E`，以及
//! 「右键——更多操作」。**眼下三组十七条，与设计稿逐条对得上**（`prototype.html` 的 `KEYS`）。
//!
//! 「右键」那一条列的不是键盘上的键，是设计稿 `KEYS` 里原样写着的一条——它指的是
//! [浏览屏那层右键菜单](crate::browse::menu)，同样是**接通了才列**。
//!
//! ## 表怎么画，也只有一处
//!
//! [`table`] 是画它的那一处：设置屏「快捷键」那一节与票 `14` 按 `?` 打开的那层弹层调的
//! 是同一个函数。**一份数据配一份画法**——分两处画的话，两处的列宽、虚线、那句注脚迟早
//! 各是各的，而屏上摆着的还是同一张表。
//!
//! ## 修饰键两种写法
//!
//! macOS 上是 `⌘`，Windows 与 Linux 上是 `Ctrl`（[`MODIFIER`]）。这一份跟着跑的这台机器写，
//! 屏上那句话把另一种也说出来（[`NOTE`]）。

use crate::look;
use crate::tokens::Tokens;

/// 修饰键在这台机器上写成什么。
pub const MODIFIER: &str = if cfg!(target_os = "macos") {
    "⌘"
} else {
    "Ctrl"
};

/// 浏览屏那几下**屏上写成什么**：这几个字在两处摆着——这张表上那几条，与
/// [右键菜单](crate::browse::menu)里那一列提示（设计稿 `.ctx button span`）。
///
/// **同一个键屏上只许有一种写法**：两处各写一份，哪天改了一处，屏上就有一处说的是旧话
/// ——而人是照着屏上那几个字去按的。
pub const OPEN: &str = "Enter";
/// 见 [`OPEN`]。
pub const PICK: &str = "空格";
/// 见 [`OPEN`]。
pub const FAVORITE: &str = "F";
/// 见 [`OPEN`]。
pub const EDIT: &str = "E";

/// **按 `?` 摊开那层弹层的副标题**（设计稿 `DLG.keys` 的 `sub`）。
///
/// 它不进这张表本身（[`table`]）：表在两处摆着，而这句话只归弹层那一处说
/// ——设置屏那一节底下写的是另一句（[`SEE_SHEET`]，设计稿 `t==='keys'` 那一支）。
/// 两处都印，屏上同一句话就说了两遍。
pub const NOTE: &str = "在 macOS 上用 ⌘，在 Windows 和 Linux 上用 Ctrl。输入框里不响应单键快捷键。";

/// **设置屏「快捷键」那一节底下那句话**（设计稿 `t==='keys'` 那一支的 `.help`）。
pub const SEE_SHEET: &str = "随时按 ? 查看这张表。";

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
                (
                    "切换到库、待确认、浏览、子库、任务、设置",
                    format!("{MODIFIER} 1 – 6"),
                ),
                ("打开设置", format!("{MODIFIER} ,")),
                ("搜索作品", format!("{MODIFIER} F")),
                ("查看快捷键", "?".to_owned()),
                ("关闭对话框或菜单", "Esc".to_owned()),
            ],
        },
        Group {
            title: "浏览",
            keys: vec![
                ("上一个 / 下一个作品", "↑ ↓".to_owned()),
                ("打开作品详情", OPEN.to_owned()),
                ("勾选 / 取消勾选", PICK.to_owned()),
                ("全选筛选结果", format!("{MODIFIER} A")),
                ("收藏 / 取消收藏", FAVORITE.to_owned()),
                ("编辑元数据", EDIT.to_owned()),
                ("更多操作", "右键".to_owned()),
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

/// **把这份表画出来**——这是画它的**唯一一处**。
///
/// 摆它的有两处：设置屏「快捷键」那一节（[`crate::settings`]）、按 `?` 打开的那层弹层
/// （[`crate::app`]）。两处调的是这一个函数。
///
/// **表底下那句话不在这儿**：两处各写各的一句（[`NOTE`] 与 [`SEE_SHEET`]），由摆它的那一处添。
///
/// 照稿分两列（`.kgrid`）：一条里说明靠左、键帽靠右，底下一道虚线。**左右各是一列**——
/// 同一列里那几枚键帽的右缘是同一条线（数字与键帽一类靠右对齐才比得出来），截图门钉着这一条。
/// 靠左靠右走 [`egui::Sides`]，不自己算位置：上一轮在浏览屏上自算位置把按钮挤出过行外。
pub fn table(ui: &mut egui::Ui) {
    let tokens = Tokens::builtin();
    let [行缝, 列缝] = tokens.space.keys_grid_gap;
    let 列宽 = ((ui.available_width() - 列缝) / 2.0).max(1.0);
    for group in groups() {
        look::section(ui, group.title);
        ui.add_space(行缝);
        for 一排 in group.keys.chunks(2) {
            // **顶对齐**（`horizontal_top`，不是 `horizontal`）：`horizontal` 是
            // `Align::Center`，而这一排里每一格要的高是现长出来的——左边那一格先长完，
            // 右边那一格就被按着那个高居中，整格往下掉半格（实测 15 点），两列对不上。
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 列缝;
                for (管什么, 键) in 一排 {
                    一条(ui, 列宽, 管什么, 键);
                }
            });
            ui.add_space(行缝);
        }
        ui.add_space(tokens.space.settings_body_gap);
    }
}

/// 快捷键表上的一条：说明靠左、键帽靠右，底下一道虚线（设计稿 `.kgrid div`）。
///
/// **那句说明摆不下就折行**，不截断（设计稿 `.kgrid div>span` 没有 `nowrap`，长的那一句
/// 自己折成两行）。这一格有多窄由摆它的那一层定：设置屏那一节一格四百多点，
/// 按 `?` 那层弹层只有二百七十来点——全局第一条「切换到库、待确认、浏览、子库、任务、设置」
/// 在后者里正好摆不下。**截断在这儿尤其坏**：屏上会写成「切换到……任务」，人照着数，
/// 第 6 下按下去会以为按错了。
fn 一条(ui: &mut egui::Ui, 宽: f32, 管什么: &str, 键: &str) {
    let tokens = Tokens::builtin();
    ui.allocate_ui_with_layout(
        egui::vec2(宽, 0.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(宽);
            ui.add_space(tokens.space.keys_row_padding);
            // 键帽多宽先量出来（[`look::kbd_size`]），剩下的才是那句话摆得开的宽。
            let 说明宽 = (宽 - look::kbd_size(ui, 键).x - tokens.space.keys_row_gap).max(1.0);
            egui::Sides::new().spacing(tokens.space.keys_row_gap).show(
                ui,
                |ui| {
                    ui.set_max_width(说明宽);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(管什么).size(tokens.font.size_small_plus),
                        )
                        .wrap(),
                    );
                },
                |ui| {
                    look::kbd(ui, 键);
                },
            );
            ui.add_space(tokens.space.keys_row_padding);
            let (线框, _) = ui.allocate_exact_size(egui::vec2(宽, 1.0), egui::Sense::hover());
            // **这道线画满这一格，与那枚键帽的右缘同一条线**（两样都落在 830.000，实测）。
            //
            // 它**看着**比键帽短一截：虚线的段与空当都是 3 点，402 点正好是 67 个来回，
            // 于是最后那 3 点落在空当上——最后一段实线停在 827，而键帽右缘在 830。
            // 那是虚线的排法，不是没对齐（CSS 的 `border-bottom:dashed` 一样会这样）。
            // 这一处编排者读图时按「两种右缘」报过一次，量过：**键帽五枚全在 830.000**。
            look::dashed_hline(
                ui.painter(),
                线框.x_range(),
                线框.center().y,
                ui.visuals().widgets.noninteractive.bg_stroke,
            );
        },
    );
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

    /// 修饰键那几条照这台机器写，不是写死的。
    #[test]
    fn 修饰键跟着这台机器() {
        let 全局 = groups().remove(0);
        let 打开设置 = 全局
            .keys
            .iter()
            .find(|(什么, _)| *什么 == "打开设置")
            .expect("全局那一组里有「打开设置」");
        assert_eq!(打开设置.1, format!("{MODIFIER} ,"));
        let 切屏 = 全局
            .keys
            .iter()
            .find(|(什么, _)| 什么.starts_with("切换到"))
            .expect("全局那一组里有切屏那一条");
        assert_eq!(切屏.1, format!("{MODIFIER} 1 – 6"));
    }

    /// **浏览那一组里那几个键，与右键菜单里那一列提示取的是同一份**（[`OPEN`] 那几个常量）。
    ///
    /// 表上写一份、菜单里又写一份的话，改了一处屏上就有一处说的是旧话——而人正是照着
    /// 屏上那几个字去按的。
    #[test]
    fn 浏览那几个键取的是共用的那一份() {
        let 浏览 = groups().remove(1);
        assert_eq!(浏览.title, "浏览");
        for (什么, 该是) in [
            ("打开作品详情", OPEN),
            ("勾选 / 取消勾选", PICK),
            ("收藏 / 取消收藏", FAVORITE),
            ("编辑元数据", EDIT),
        ] {
            let 这一条 = 浏览
                .keys
                .iter()
                .find(|(名, _)| *名 == 什么)
                .unwrap_or_else(|| panic!("浏览那一组里有「{什么}」"));
            assert_eq!(这一条.1, 该是, "「{什么}」那一条");
        }
    }

    /// **表上一条都不许是印着好看的**：每一条都得真有人接。这里钉的是「接一个列一个」
    /// 那条纪律的下半句——三组的条数与设计稿 `KEYS` 逐条对得上（挂单 `Q1061`）。
    #[test]
    fn 三组与设计稿逐条对得上() {
        let groups = groups();
        let 有几条: Vec<(&str, usize)> = groups
            .iter()
            .map(|group| (group.title, group.keys.len()))
            .collect();
        assert_eq!(有几条, vec![("全局", 5), ("浏览", 7), ("待确认 · 逐条", 5)],);
    }
}
