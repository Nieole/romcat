//! **疑似同一作品**那张建议卡片，连左栏「整理建议」那一簇
//! （票 `gui-looks-like-the-design/17`，设计稿 `.sugg` / `suggHTML` 与 `#dup-chip`）。
//!
//! ## 界面只画和转发
//!
//! **一条领域判断都不在这儿**（ADR-0024）：哪两个作品疑似是同一个、凭什么、那几条理由
//! 逐字怎么写，全问核心库那一处（[`romcat_core::triage::same_work`]）。这一层管的是
//! 那张卡片长什么样、两颗按钮按下去转给谁。
//!
//! 命令行 `romcat triage same-work` 读的是同一处，于是两边说的是同一句话。
//!
//! ## 两颗按钮落在两条路上
//!
//! - **「合并…」**：开合并向导（[`merge::Wizard`](super::merge::Wizard)），落成一批**裁决**，
//!   撤销走**裁决记录**。
//! - **「不是同一个」**：记进沉淀库那张 `not_same_work`，什么都不合。**它不是一条裁决**，
//!   所以屏上那句回话不照设计稿写「（记为裁决）」——那正是挂单 `Q1032` 在平台纠正上
//!   裁过的同一件事：对用户说了另一样东西的名字。

use std::collections::BTreeMap;

use romcat_core::report::thousands;
use romcat_core::triage::same_work::Suspicion;

use crate::font;
use crate::look;
use crate::tokens::Tokens;

/// 左栏那一簇的小标题（设计稿 `.sec`）。
pub const SECTION: &str = "整理建议";

/// 那一簇里那颗分面标签上的字（设计稿 `#dup-chip`）。
pub const FACET: &str = "疑似同一作品";

/// 那一簇底下那句说明：它与「识别结论」那一簇同一个处境——**只用于浏览，不写进规则**
/// （票 12 的验收第 3 条点名要屏上说出来）。
pub const BROWSE_ONLY: &str = "只用于浏览，不写入规则";

/// 卡片上「不是同一个」那一颗（设计稿 `data-mw="no:"`）。
pub const NOT_SAME: &str = "不是同一个";

/// 按下「不是同一个」之后那句回话。
///
/// 设计稿那句后头还缀着「（记为裁决）」，**这里不照抄**：它落的是沉淀库里自己那张表，
/// 撤销不走**裁决记录**，说成裁决是对用户说了另一样东西的名字（挂单 `Q1032` 在平台纠正
/// 上裁过同一件事）。
pub const NOT_SAME_SAID: &str = "已标记为不是同一个作品，不会再提示";

/// 人在卡片上按了哪一颗——**这一层不动库**，交给浏览屏去办。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Deed {
    /// 开合并向导：这两个作品。
    Merge([String; 2]),
    /// 记一条「不是同一个」：这两个作品。
    NotSame([String; 2]),
}

/// 那颗分面标签上的条数写成什么（设计稿 `#dup-n` 的「2 组」）。
#[must_use]
pub fn groups(count: usize) -> String {
    format!("{} 组", thousands(count as u64))
}

/// 这个作品身上**有没有**建议：屏上摆不摆这一块、摆之前那道缝留不留，都问它。
///
/// **摆之前先问**：一条都没有时那道缝也不许留——留了的话，整份库里没有一条建议的人
/// 每一屏都白白多出一道空当（票 16 那几张基线正是这么被顶下去 14 点的）。
#[must_use]
pub fn any_for(found: &[Suspicion], work: &str) -> bool {
    found.iter().any(|one| one.touches(work))
}

/// 这个作品身上那几张建议卡，一张一条建议（设计稿 `suggHTML`）。按了哪一颗交回来。
///
/// `work` 是**作品名**；还没认出作品的那一行传不进来，它不参与这件事——
/// 合并的两侧都得说得出作品名。**一张都没有时一个点都不占**（[`any_for`]）。
pub fn cards(
    ui: &mut egui::Ui,
    found: &[Suspicion],
    work: &str,
    shown: &BTreeMap<String, String>,
) -> Option<Deed> {
    let mut deed = None;
    for one in found.iter().filter(|one| one.touches(work)) {
        if let Some(按了) = card(ui, one, work, shown) {
            deed = Some(按了);
        }
    }
    deed
}

/// 一张卡片（设计稿 `.sugg`）：一圈强调色**虚线**、底色里调进一点强调色、大圆角；
/// 里头是一行粗体「可能与《某某》是同一个作品」、逐条理由、底下两颗小号按钮。
///
/// **作品名写成《某某》**，不照稿那对直角引号：全仓屏上说到作品一律是书名号
/// （合并向导第三步那句「留的是并空了的那几个：《…》」），一处换一种写法读的人会以为
/// 说的是两样东西。
fn card(
    ui: &mut egui::Ui,
    one: &Suspicion,
    work: &str,
    shown: &BTreeMap<String, String>,
) -> Option<Deed> {
    let tokens = Tokens::builtin();
    let palette = look::palette(ui);
    let 另一个 = one.other_than(work)?;
    // **屏上那个名字**：主列表与详情面板印的都是显示标题，卡片照作品名写的话，
    // 同一部作品在同一屏上会有两个名字。
    let 另一个 = shown.get(另一个).map_or(另一个, String::as_str);
    let [上下, 左右] = tokens.layout.suspicion_padding;
    let mut deed = None;
    let 框 = egui::Frame::new()
        .fill(
            palette
                .panel
                .lerp_to_gamma(palette.accent, tokens.mix.suspicion_tint),
        )
        .corner_radius(tokens.radius.large)
        .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = tokens.layout.suspicion_gap;
            ui.vertical(|ui| {
                ui.label(font::strong(format!("可能与《{另一个}》是同一个作品")));
                // **理由逐条列出来**，一条一行（设计稿 `.sugg ul`）。走 `look::impact`
                // 那一处：行首那枚圆点、那一列宽与那一档字号全仓只有它一份，
                // 于是这几行的左缘由它一处管着，不在这儿另摆一套缩进（挂单 `Q1042`）。
                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing.y = tokens.layout.suspicion_reason_gap;
                    for reason in one.reasons() {
                        look::impact(ui, &[(reason.as_str(), false)]);
                    }
                });
                ui.horizontal(|ui| {
                    let (合并, 不是) = look::small_buttons(ui, |ui| {
                        let 合并 = ui
                            .scope(|ui| {
                                look::primary_button(ui.visuals_mut());
                                ui.button(super::merge::MERGE_ONE)
                            })
                            .inner
                            .on_hover_text(
                                "把这两个作品合并成一个：第一步里核对要保留哪一个。\n\n\
                                 只写入裁决记录，不会移动或修改任何文件。",
                            )
                            .clicked();
                        let 不是 = look::small_ghost_button(ui, NOT_SAME)
                            .on_hover_text(
                                "记下这两个作品不是同一个，以后不再提这一对。\n\n\
                                 它不是一条裁决，撤销不走裁决记录；\
                                 要撤回请用命令行 `romcat triage same-work --not-same … --undo`。",
                            )
                            .clicked();
                        (合并, 不是)
                    });
                    if 合并 {
                        deed = Some(Deed::Merge(one.works.clone()));
                    }
                    if 不是 {
                        deed = Some(Deed::NotSame(one.works.clone()));
                    }
                });
            });
        });
    // 一圈**虚线**：`egui::Frame` 只画得出实线，所以描边自己画在框的外沿上
    // （与队列屏那个「没有候选」的虚线框同一条路）。
    look::dashed_outline(
        ui.painter(),
        框.response.rect,
        egui::Stroke::new(tokens.layout.control_stroke, palette.accent),
    );
    deed
}
