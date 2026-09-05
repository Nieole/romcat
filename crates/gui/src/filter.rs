//! **筛选器**：浏览屏上那棵可嵌套的**条件组**树，也就是子库的**规则**。
//!
//! 同一套语言两处用（票 `gui-redesign/04`）：在这儿筛到满意按「存成子库」，条件原样
//! 变成那个子库的规则；反过来，子库屏点「改选择」跳回来，规则预填进这棵树
//! （[`Filter::set_rule`]，票 `11`）。
//!
//! ## 这一层只搭形状，判断全在核心库
//!
//! ADR-0005。这个文件里没有一句「这条规则选中了什么」：
//!
//! - **读一个子句**走 [`Clause::build`]，与命令行手写那行字走的是同一条路；
//! - **印回文本**走 [`Rule::from_group`]；
//! - **求值**在中立库里（`catalog::filter` 下推成 `WHERE`）。
//!
//! 这里留下的只有一样东西核心库里没有：**没填完的那几条**。界面上新按一下「+ 子句」
//! 就多一行空值，那一行还不是一个子句——它是一个正在打字的位置。
//!
//! ## 没填完与写错了都不许悄悄生效
//!
//! 两种半成品同一个待遇：**不进规则，但在屏上点名**。
//!
//! - 悄悄把它当成一条子句，会当场报「值是空的」，人还没打完字就满屏红字；
//! - 悄悄把它**当成生效的**，筛出来的会比人以为的窄；
//! - 悄悄**扔掉**而不说，筛出来的会比人以为的宽。
//!
//! 前两条都比第三条坏，而第三条只要说出口就不坏——所以这一栏底下常驻一句
//! 「还有 N 条没生效」，红的。

use egui::ComboBox;
use romcat_core::sublibrary::{Clause, Dimension, Group, Join, Node, Op, Rule};

/// 界面上那棵**条件组**树。
///
/// 与核心库那棵 [`Group`] 的差别只有一处：叶子上装的是**正在打的字**而不是读通了的
/// 子句。折成规则是 [`Self::rule`]，那一步之后才轮到核心库。
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// 顶层那个组。
    root: Draft,
    /// 折出来的那条规则；跟着编辑走。一个条件都没有时是 `None`。
    rule: Option<Rule>,
    /// 没填完、或者填错了的那几条。**它们不在规则里**。
    pending: Vec<String>,
}

/// 一个组的草稿。
#[derive(Debug, Clone)]
struct Draft {
    join: Join,
    nodes: Vec<Slot>,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            // 默认「全部满足」：一层层收窄是人在文件管理器里的动作。
            join: Join::All,
            nodes: Vec::new(),
        }
    }
}

/// 组里的一项：一条正在填的子句，或者又一个组。
#[derive(Debug, Clone)]
enum Slot {
    Leaf(Leaf),
    Group(Draft),
}

/// 一条正在填的子句：三个控件各交出一样。
#[derive(Debug, Clone)]
struct Leaf {
    dimension: Dimension,
    op: Op,
    value: String,
}

impl Default for Leaf {
    fn default() -> Self {
        Self {
            dimension: Dimension::Platform,
            op: Op::Is,
            value: String::new(),
        }
    }
}

impl Filter {
    /// 眼下这棵树折出来的那条规则。一个条件都没有时是 `None`——那是「不筛」。
    #[must_use]
    pub fn rule(&self) -> Option<&Rule> {
        self.rule.as_ref()
    }

    /// 没填完、或者填错了的那几条。**它们不在规则里**，屏上照原样点名。
    #[must_use]
    pub fn pending(&self) -> &[String] {
        &self.pending
    }

    /// 把一条规则预填进这棵树。
    ///
    /// 子库屏点「改选择」跳回浏览屏时走的就是它（票 `gui-redesign/11`）：**规则原样
    /// 摊在筛选器里**，人改的时候看得见它真的筛出了什么。
    pub fn set_rule(&mut self, rule: Option<Rule>) {
        self.root = match &rule {
            Some(rule) => draft_of(&rule.root),
            None => Draft::default(),
        };
        self.refresh();
    }

    /// 全清。
    pub fn clear(&mut self) {
        self.root = Draft::default();
        self.refresh();
    }

    /// 空不空——一个条件都没搭过。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.root.nodes.is_empty()
    }

    /// 画这一栏。**返回「改过没有」**：改过就得把窗口作废重取。
    pub fn ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        group_ui(ui, &mut self.root, 0, &mut changed);
        if changed {
            self.refresh();
        }
        if !self.pending.is_empty() {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("还有 {} 条没生效：", self.pending.len()),
            )
            .on_hover_text("没填完或者填错了的子句**不进筛选**。悄悄扔掉不说的话，筛出来的会比你以为的宽。");
            for line in &self.pending {
                ui.colored_label(ui.visuals().error_fg_color, format!("　{line}"));
            }
        }
        changed
    }

    /// 把草稿折成规则，顺带把没填完的那几条挑出来。
    fn refresh(&mut self) {
        let mut pending = Vec::new();
        let root = group_of(&self.root, &mut pending);
        self.pending = pending;
        self.rule = root
            .filter(|group| !group.nodes.is_empty())
            .map(Rule::from_group);
    }
}

/// 把一个组的草稿折成核心库那棵树。**空组整个丢掉**——一个空的「任一满足」组
/// 一条都选不中，留着它等于把整份筛选清零。
fn group_of(draft: &Draft, pending: &mut Vec<String>) -> Option<Group> {
    let mut nodes = Vec::new();
    for slot in &draft.nodes {
        match slot {
            Slot::Leaf(leaf) => {
                if leaf.value.trim().is_empty() {
                    pending.push(format!(
                        "{}{} —— 还没填值",
                        leaf.dimension.label(),
                        leaf.op.label()
                    ));
                    continue;
                }
                match Clause::build(leaf.dimension, leaf.op, &leaf.value) {
                    Ok(clause) => nodes.push(Node::Clause(clause)),
                    Err(error) => pending.push(format!(
                        "{}{}{} —— {error}",
                        leaf.dimension.label(),
                        leaf.op.label(),
                        leaf.value.trim()
                    )),
                }
            }
            Slot::Group(inner) => {
                if let Some(group) = group_of(inner, pending) {
                    nodes.push(Node::Group(group));
                }
            }
        }
    }
    (!nodes.is_empty()).then(|| Group::new(draft.join, nodes))
}

/// 反过来：把核心库那棵树摊成草稿。
fn draft_of(group: &Group) -> Draft {
    Draft {
        join: group.join,
        nodes: group
            .nodes
            .iter()
            .map(|node| match node {
                Node::Clause(clause) => Slot::Leaf(Leaf {
                    dimension: clause.dimension,
                    op: clause.op,
                    // **值那一段取原文**：`体积<=64MiB` 摊回筛选器还得是 `64MiB`，
                    // 不是折算过的 67108864。
                    value: clause.raw.clone(),
                }),
                Node::Group(inner) => Slot::Group(draft_of(inner)),
            })
            .collect(),
    }
}

/// 画一个组：连接词、里面那几项、以及「+ 子句」「+ 分组」。
fn group_ui(ui: &mut egui::Ui, draft: &mut Draft, depth: usize, changed: &mut bool) {
    ui.horizontal(|ui| {
        let before = draft.join;
        ComboBox::from_id_salt(("连接", depth, ui.id()))
            .selected_text(draft.join.label())
            .width(96.0)
            .show_ui(ui, |ui| {
                for join in Join::ALL {
                    ui.selectable_value(&mut draft.join, join, join.label());
                }
            });
        if draft.join != before {
            *changed = true;
        }
        if ui
            .small_button("+ 子句")
            .on_hover_text("加一条「维度 运算符 值」。")
            .clicked()
        {
            draft.nodes.push(Slot::Leaf(Leaf::default()));
            *changed = true;
        }
        if ui
            .small_button("+ 分组")
            .on_hover_text("加一个组：里面自己选「全部满足 / 任一满足 / 都不满足」，还能再套。")
            .clicked()
        {
            draft.nodes.push(Slot::Group(Draft::default()));
            *changed = true;
        }
    });

    let mut drop_at: Option<usize> = None;
    for (index, slot) in draft.nodes.iter_mut().enumerate() {
        ui.push_id(index, |ui| match slot {
            Slot::Leaf(leaf) => {
                if leaf_ui(ui, leaf, changed) {
                    drop_at = Some(index);
                }
            }
            Slot::Group(inner) => {
                // 套进来的组画成一个框：**括号在纸面上就是那个组**。
                let response = egui::Frame::group(ui.style()).show(ui, |ui| {
                    let mut drop_me = false;
                    ui.horizontal(|ui| {
                        if ui.small_button("×").on_hover_text("去掉这个组。").clicked() {
                            drop_me = true;
                        }
                        ui.weak("分组");
                    });
                    group_ui(ui, inner, depth + 1, changed);
                    drop_me
                });
                if response.inner {
                    drop_at = Some(index);
                }
            }
        });
    }
    if let Some(index) = drop_at {
        draft.nodes.remove(index);
        *changed = true;
    }
}

/// 画一条子句。返回**该不该把它去掉**。
fn leaf_ui(ui: &mut egui::Ui, leaf: &mut Leaf, changed: &mut bool) -> bool {
    let mut drop_me = false;
    ui.horizontal(|ui| {
        if ui.small_button("×").on_hover_text("去掉这一条。").clicked() {
            drop_me = true;
        }
        let before = leaf.dimension;
        ComboBox::from_id_salt("维度")
            .selected_text(leaf.dimension.label())
            .width(76.0)
            .show_ui(ui, |ui| {
                for dimension in Dimension::all() {
                    ui.selectable_value(&mut leaf.dimension, dimension, dimension.label())
                        .on_hover_text(dimension.hint());
                }
            });
        if leaf.dimension != before {
            // **换了维度，运算符可能就配不上了**：`体积~大` 说不清是什么意思。
            // 当场换成这一维摆得出的头一个，而不是留一条读不懂的子句在那儿。
            let fits = Op::for_dimension(leaf.dimension);
            if !fits.contains(&leaf.op) {
                leaf.op = fits.first().copied().unwrap_or(Op::Is);
            }
            *changed = true;
        }
        let before = leaf.op;
        ComboBox::from_id_salt("运算符")
            .selected_text(leaf.op.label())
            .width(56.0)
            .show_ui(ui, |ui| {
                for op in Op::for_dimension(leaf.dimension) {
                    ui.selectable_value(&mut leaf.op, op, op.hint());
                }
            });
        if leaf.op != before {
            *changed = true;
        }
        if ui
            .add(
                egui::TextEdit::singleline(&mut leaf.value)
                    .desired_width(120.0)
                    .hint_text(value_hint(leaf.dimension)),
            )
            .changed()
        {
            *changed = true;
        }
    });
    drop_me
}

/// 值那一格里的灰字。**说清这一维收什么**，比让人对着空框猜强。
fn value_hint(dimension: Dimension) -> &'static str {
    match dimension {
        Dimension::Platform => "GB, GBA",
        Dimension::Language => "Zh",
        Dimension::Chinese => "汉化",
        Dimension::Collection => "通关过的",
        Dimension::Favorite => "是",
        Dimension::Genre => "RPG",
        Dimension::Work => "火焰纹章",
        Dimension::Year => "1990",
        Dimension::Rating => "80%",
        Dimension::Size => "64MiB",
        Dimension::Developer => "Game Freak",
        Dimension::Publisher => "Nintendo",
        Dimension::Description => "勇者",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 规则预填进来再折回去还是同一条() {
        let mut filter = Filter::default();
        for text in [
            "平台=GB,GBA 且 中文=汉化",
            "平台=GB 且 (作品^口袋 或 类型~RPG)",
            "都不(语言=En 或 中文=汉化)",
            "体积<=64MiB",
        ] {
            let rule = Rule::parse(text).expect("读得懂");
            filter.set_rule(Some(rule.clone()));
            let back = filter.rule().expect("折得回来");
            assert_eq!(back.text, text, "「{text}」摊进筛选器再折回去变了");
            assert!(filter.pending().is_empty());
        }
        filter.clear();
        assert!(filter.rule().is_none());
        assert!(filter.is_empty());
    }
}
