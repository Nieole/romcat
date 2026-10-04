//! **筛选器**：浏览屏上那棵可嵌套的**条件组**树，也就是子库的**规则**。
//!
//! 同一套语言两处用（票 `gui-redesign/04`）：在这儿筛到满意按「存成子库」，条件原样
//! 变成那个子库的规则；反过来，子库屏规则行上点「✎」跳回来，那一条规则预填进这棵树
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
//! 前两条都比第三条坏，而第三条只要说出口就不坏——所以写错的那一条底下贴一句核心库给的话（红的），
//! 条件组框底下常驻一行计数「N 个子句未填写，不会生效；N 个子句写错了，改正前不会生效」
//! （照稿 `.tnote.bad` 与 `#unset`；票 `gui-draws-the-rest-of-the-design/23`，收挂单 `Q1477`）。

use egui::ComboBox;
use romcat_core::sublibrary::{
    Clause, Dimension, Group, Join, KnownValues, Node, Op, Rule, RuleError,
};

use crate::look;
use crate::tokens::Tokens;

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
    /// 没填完、写错了的各有几条。**它们不在规则里**。
    not_in_effect: NotInEffect,
}

/// 没生效的那几条：没填完、写错了的各有几条（折规则那一趟顺手数出来）。条件组框底下那一行计数说的就是它。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct NotInEffect {
    /// 还没填值的。
    unfilled: usize,
    /// 填了、但核心库读不成子句的。
    broken: usize,
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
    /// 下一帧把焦点放进它的值框：刚按「+ 子句」「+ 组」加出来的那一条（照稿加完就能打字）。画过一帧就放下。
    focus: bool,
}

impl Leaf {
    /// 这一条读成子句。`None` 是**还没填值**，`Some(Err(..))` 是读不成——两种都不进规则。
    ///
    /// **读一条子句只有这一处**：折成规则（[`group_of`]）与「这一条筛不筛得出东西」（[`thin_clause_ui`]）
    /// 都走它，于是屏上贴着提示的那一条，就是规则里那一条。
    fn clause(&self) -> Option<Result<Clause, RuleError>> {
        (!self.value.trim().is_empty()).then(|| Clause::build(self.dimension, self.op, &self.value))
    }
}

impl Leaf {
    /// 按「+ 子句」「+ 组」加出来的一条：这一维、这个运算符、值空着。
    fn added(dimension: Dimension, op: Op) -> Self {
        Self {
            dimension,
            op,
            value: String::new(),
            focus: false,
        }
    }

    /// 同一条，下一帧把焦点放进它的值框（加完就能打字的那一条）。
    fn focused(self) -> Self {
        Self {
            focus: true,
            ..self
        }
    }
}

impl Filter {
    /// 眼下这棵树折出来的那条规则。一个条件都没有时是 `None`——那是「不筛」。
    #[must_use]
    pub fn rule(&self) -> Option<&Rule> {
        self.rule.as_ref()
    }

    /// 没填完、或者写错了的一共几条。**它们不在规则里**。
    #[must_use]
    pub fn not_in_effect(&self) -> usize {
        self.not_in_effect.unfilled + self.not_in_effect.broken
    }

    /// 把一条规则预填进这棵树。
    ///
    /// 子库屏规则行上点「✎」跳回浏览屏时走的就是它（票 `gui-redesign/11`）：**规则原样
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
    ///
    /// `known` 是判「这条子句筛不筛得出东西」要的那点上下文（平台清单、库里有哪几个
    /// 合集）。**判在核心库**（`sublibrary::thin`），这一层只把它交出去、再把交回来的
    /// 话印出来（ADR-0024）。
    pub fn ui(&mut self, ui: &mut egui::Ui, known: &KnownValues<'_>) -> bool {
        let mut changed = false;
        // 顶层那一组没有「×」，交回来的永远是「不去掉」。
        group_ui(ui, &mut self.root, 0, known, &mut changed);
        if changed {
            self.refresh();
        }
        // **筛不出东西的、写错了的都不在这儿汇总**：各自贴在出问题的那一条子句底下（[`leaf_ui`]）；
        // 框底下那一行计数由 [`Self::not_in_effect_ui`] 画，摆在框外（稿上 `#unset` 是 `.gtree` 的下一格）。
        changed
    }

    /// 条件组框**底下**那一行计数（设计稿 `#unset`）：「N 个子句未填写，不会生效；N 个子句写错了，改正前不会生效」。
    /// 有写错的用 `lo`，只有没填完的用 `mid`；两样都没有就不画。
    ///
    /// 写错的那一条为什么错，贴在它自己底下（`leaf_ui`）；这一行只数个数——条件组长过一屏时，
    /// 人在框底下也知道上头还有几条没起作用（票 `gui-draws-the-rest-of-the-design/23`，收挂单 `Q1477`）。
    pub fn not_in_effect_ui(&self, ui: &mut egui::Ui) {
        let NotInEffect { unfilled, broken } = self.not_in_effect;
        let mut 几段 = Vec::new();
        if unfilled > 0 {
            几段.push(format!("{unfilled} 个子句未填写，不会生效"));
        }
        if broken > 0 {
            几段.push(format!("{broken} 个子句写错了，改正前不会生效"));
        }
        if 几段.is_empty() {
            return;
        }
        let palette = look::palette(ui);
        let 色 = if broken > 0 { palette.lo } else { palette.mid };
        let 字号 = look::font_size(ui.ctx(), Tokens::builtin().font.size_small);
        ui.label(egui::RichText::new(几段.join("；")).size(字号).color(色))
            .on_hover_text(
                "没填完或者写错了的子句不进筛选。悄悄扔掉不说的话，筛出来的会比你以为的宽。",
            );
    }

    /// 把草稿折成规则，顺带数出没填完、写错了的各有几条。
    fn refresh(&mut self) {
        let mut not_in_effect = NotInEffect::default();
        let root = group_of(&self.root, &mut not_in_effect);
        self.not_in_effect = not_in_effect;
        self.rule = root
            .filter(|group| !group.nodes.is_empty())
            .map(Rule::from_group);
    }
}

/// 把一个组的草稿折成核心库那棵树。**空组整个丢掉**——一个空的「任一满足」组
/// 一条都选不中，留着它等于把整份筛选清零。
fn group_of(draft: &Draft, not_in_effect: &mut NotInEffect) -> Option<Group> {
    let mut nodes = Vec::new();
    for slot in &draft.nodes {
        match slot {
            Slot::Leaf(leaf) => match leaf.clause() {
                None => not_in_effect.unfilled += 1,
                Some(Ok(clause)) => nodes.push(Node::Clause(clause)),
                Some(Err(_)) => not_in_effect.broken += 1,
            },
            Slot::Group(inner) => {
                if let Some(group) = group_of(inner, not_in_effect) {
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
                    focus: false,
                }),
                Node::Group(inner) => Slot::Group(draft_of(inner)),
            })
            .collect(),
    }
}

/// 画一个组（设计稿 `.tg`，票 `gui-draws-the-rest-of-the-design/23`）。返回**该不该把它去掉**（只有套进来的组有那颗「×」）。
///
/// 照稿：**左边一道竖线**（顶层强调色，套进来的 `mid` 色）、**没有框**；头一行一句弱字后接组合方式
/// （[`group_head_ui`]），然后一条条子句与套进来的组，最底下「+ 子句」「+ 组」（[`add_buttons_ui`]）。
/// 套进来的组缩在外层竖线右边那一截里，于是一层套一层看得出括号。
fn group_ui(
    ui: &mut egui::Ui,
    draft: &mut Draft,
    depth: usize,
    known: &KnownValues<'_>,
    changed: &mut bool,
) -> bool {
    let tokens = Tokens::builtin();
    let [上下, 竖线右边] = tokens.space.rule_group_padding;
    let 竖线宽 = tokens.layout.rule_group_bar;
    let palette = look::palette(ui);
    let 竖线色 = if depth == 0 {
        palette.accent
    } else {
        palette.mid
    };
    let mut 去掉这个组 = false;
    let shown = egui::Frame::new()
        .inner_margin(egui::Margin {
            left: (竖线宽 + 竖线右边) as i8,
            right: 0,
            top: 上下 as i8,
            bottom: 上下 as i8,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = tokens.space.rule_group_gap;
            去掉这个组 = group_head_ui(ui, draft, depth, changed);
            let mut drop_at: Option<usize> = None;
            for (index, slot) in draft.nodes.iter_mut().enumerate() {
                ui.push_id(index, |ui| {
                    let 去掉 = match slot {
                        Slot::Leaf(leaf) => leaf_ui(ui, leaf, known, changed),
                        Slot::Group(inner) => group_ui(ui, inner, depth + 1, known, changed),
                    };
                    if 去掉 {
                        drop_at = Some(index);
                    }
                });
            }
            if let Some(index) = drop_at {
                draft.nodes.remove(index);
                *changed = true;
            }
            // 顶层一项都没有时说一句这一块是干什么的（照稿 `groupHTML` 的 `!d&&!g.items.length`）。措辞不照稿写「…等条件筛」：
            // 拿主意的人 2026-10-04 裁（差距清单 `F-1` 选 B），与组头那一句同一个理由。
            if depth == 0 && draft.nodes.is_empty() {
                look::help(ui, EMPTY_HINT);
            }
            add_buttons_ui(ui, draft, depth, changed);
        });
    // **竖线等整个组摆完才画**：它从组头一直伸到底下那两颗加号，多高要摆完才知道。
    let 整块 = shown.response.rect;
    ui.painter().rect_filled(
        egui::Rect::from_min_size(整块.left_top(), egui::vec2(竖线宽, 整块.height())),
        0.0,
        竖线色,
    );
    去掉这个组
}

/// 组头那一行（设计稿 `.tgh`）：一句弱字（顶层「另加子句：」，套进来的「组：」）、组合方式那颗下拉，
/// 套进来的组右头再一颗「×」。返回那颗「×」按没按。
///
/// 顶层那一句**不照稿写「另加条件：」**：拿主意的人 2026-10-04 裁（差距清单 `F-1` 选 B）——词表**子句**、**规则**
/// 两条的 `_Avoid_` 都列着「条件」，而这一句说的正是这一组里的子句。
fn group_head_ui(ui: &mut egui::Ui, draft: &mut Draft, depth: usize, changed: &mut bool) -> bool {
    let mut 去掉 = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = Tokens::builtin().space.rule_gap;
        look::help(
            ui,
            if depth == 0 {
                "另加子句："
            } else {
                "组："
            },
        );
        join_combo(ui, draft, changed);
        if depth > 0 {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if look::icon_button(ui, "×")
                    .on_hover_text("删除这个组")
                    .clicked()
                {
                    去掉 = true;
                }
            });
        }
    });
    去掉
}

/// 组合方式那颗下拉（设计稿 `.tg .tgh select`）：矮一截（`rule-join-height`）、说明字号（`size-caption-plus`），
/// `accent-soft` 底、`line-2` 描边、`accent-ink` 字，小圆角。稿上那几个字是粗的——中文照全仓的字体预算用常规体。
fn join_combo(ui: &mut egui::Ui, draft: &mut Draft, changed: &mut bool) {
    let tokens = Tokens::builtin();
    let palette = look::palette(ui);
    ui.scope(|ui| {
        小控件(
            ui,
            tokens.layout.rule_join_height,
            tokens.layout.rule_join_padding,
        );
        let visuals = ui.visuals_mut();
        for widget in [
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.bg_fill = palette.accent_soft;
            widget.weak_bg_fill = palette.accent_soft;
            widget.fg_stroke.color = palette.accent_ink;
        }
        visuals.widgets.inactive.bg_stroke.color = palette.line_2;
        let 字号 = look::font_size(ui.ctx(), tokens.font.size_caption_plus);
        let before = draft.join;
        ComboBox::from_id_salt("组合方式")
            .selected_text(
                egui::RichText::new(draft.join.label())
                    .size(字号)
                    .color(palette.accent_ink),
            )
            .width(0.0)
            .wrap_mode(egui::TextWrapMode::Extend)
            .show_ui(ui, |ui| {
                for join in Join::ALL {
                    ui.selectable_value(&mut draft.join, join, join.label());
                }
            })
            .response
            .on_hover_text("这一组里的几项怎么算数");
        if draft.join != before {
            *changed = true;
        }
    });
}

/// 顶层一项都没有时，组里那句说明。
const EMPTY_HINT: &str = "在平台、中文等筛选之外，再按年份、类型、作品名等维度加子句。组可以嵌套。";

/// 套到第几层（顶层是 0）就不再给「+ 组」：照稿 `groupHTML` 的 `d<2`，最多三层。
///
/// **只在界面上限**：核心库的组嵌多深都行，命令行与子库屏「✎」读进来的更深的规则照样摊开、照样改得动删得掉，
/// 只是不能再往下加。限它是因为左栏 232 宽，竖线加缩进每深一层吃掉 10 点，到第三层两颗下拉只剩六七十点宽。
const DEEPEST: usize = 2;

/// 条件组里那几颗**小控件**的尺寸（组合方式那颗下拉、一条子句里两颗下拉与值框）：高 `高`、左右留白 `左右`、
/// 四档都是小圆角（设计稿 `border-radius:var(--r-s)`）。只改这一块 `ui` 的样子，外头不受影响。
fn 小控件(ui: &mut egui::Ui, 高: f32, 左右: f32) {
    let spacing = ui.spacing_mut();
    spacing.interact_size.y = 高;
    spacing.button_padding = egui::vec2(左右, 0.0);
    let 小圆角 = egui::CornerRadius::same(Tokens::builtin().radius.small);
    let widgets = &mut ui.visuals_mut().widgets;
    for widget in [
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.open,
    ] {
        widget.corner_radius = 小圆角;
    }
}

/// 组底下那一排（设计稿 `.btn.ghost.sm` 两颗）：「+ 子句」「+ 组」。字与词表**子句**、**组**同词。
///
/// **加出来的样子照稿**（拿主意的人 2026-10-04 裁，差距清单 `F-3` 选 A）：「+ 子句」是一条「年份 >=」——平台、中文在上头
/// 已经有分面，条件组里加的多半是年份、类型、作品名；「+ 组」是一个「任一满足」组、先摆两条空子句「类型 ~」「作品 ^」——
/// 套一个组多半是为了在「全部满足」里放一段「或」，一个空的「全部满足」组既不进规则、人还得先去改组合方式。
/// 加完焦点落进新那一条的值框（新组落进头一条）。
///
/// **最多套三层**：第三层（[`DEEPEST`]）只给「+ 子句」（拿主意的人 2026-10-04 裁，差距清单 `F-2` 选 A）。
fn add_buttons_ui(ui: &mut egui::Ui, draft: &mut Draft, depth: usize, changed: &mut bool) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = Tokens::builtin().space.rule_gap;
        if look::small_ghost_button(ui, "+ 子句")
            .on_hover_text("加一条「维度 运算符 值」。")
            .clicked()
        {
            draft
                .nodes
                .push(Slot::Leaf(Leaf::added(Dimension::Year, Op::Ge).focused()));
            *changed = true;
        }
        if depth < DEEPEST
            && look::small_ghost_button(ui, "+ 组")
                .on_hover_text("加一个组：里面自己选「全部满足 / 任一满足 / 都不满足」。")
                .clicked()
        {
            draft.nodes.push(Slot::Group(Draft {
                join: Join::Any,
                nodes: vec![
                    Slot::Leaf(Leaf::added(Dimension::Genre, Op::Contains).focused()),
                    Slot::Leaf(Leaf::added(Dimension::Work, Op::StartsWith)),
                ],
            }));
            *changed = true;
        }
    });
}

/// 画一条子句（设计稿 `.tc`）。返回**该不该把它去掉**。
///
/// 照稿两栏：左栏两行——「维度 ▾」「运算符 ▾」一行（两格按 `rule-clause-columns` 分宽），值框一行占满；
/// 右栏一颗「×」（[`look::icon_button`]），对着左栏竖直居中。三格一样高（`input-small-height`）、说明字号、
/// 小圆角、面板底、`line-2` 描边；值框是等宽字。
///
/// **这一条筛不出东西时，那句话就贴在它底下**（照稿 `.tnote`：说明字号、`mid` 色，是那一条子句 `.tc` 里的最后一格；
/// 票 `gui-draws-the-rest-of-the-design/07`，收挂单 `Q1104`）。从前那几句汇总在条件组框最底下，
/// 默认窗口里正好落在折线以下；贴着出问题的那一条，人改的是哪一条、看见的就是哪一条的话。
fn leaf_ui(
    ui: &mut egui::Ui,
    leaf: &mut Leaf,
    known: &KnownValues<'_>,
    changed: &mut bool,
) -> bool {
    let tokens = Tokens::builtin();
    let 缝 = tokens.space.rule_gap;
    let 叉边长 = tokens.layout.icon_button;
    let 左宽 = (ui.available_width() - 叉边长 - 缝).max(0.0);
    // 三格怎么描边看这一条**这一帧开头**读得成读不成（这一帧里改的字下一帧才折进规则，描边跟着它走）。
    let 这一条 = 这一条怎样::of(&leaf.clause());
    let mut drop_me = false;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 缝;
        let 左栏 = ui.allocate_ui_with_layout(
            egui::vec2(左宽, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(左宽);
                ui.spacing_mut().item_spacing.y = 缝;
                clause_controls_ui(ui, leaf, 左宽, 这一条, changed);
            },
        );
        // 「×」对着左栏两行竖直居中（稿上 `.tc` 的 `align-items:center`）：左栏多高要摆完才知道。
        let 左高 = 左栏.response.rect.height();
        ui.vertical(|ui| {
            ui.add_space(((左高 - 叉边长) / 2.0).max(0.0));
            if look::icon_button(ui, "×")
                .on_hover_text("删除这个子句")
                .clicked()
            {
                drop_me = true;
            }
        });
    });
    // 底下贴的那一句看**改完之后**这一条读成了什么（与折规则那一趟同一处，[`Leaf::clause`]）：
    // **写错了的，为什么错贴在它底下**（照稿 `.tnote.bad`：说明字号、`lo` 色）；那句话是核心库 `RuleError` 那一句，
    // 与命令行同一句（拿主意的人 2026-10-04 裁，差距清单 `F-5` 选 A）。读得成的才轮到「筛不出东西」那一问；没填的不贴。
    match leaf.clause() {
        Some(Err(error)) => {
            ui.label(
                egui::RichText::new(error.to_string())
                    .text_style(egui::TextStyle::Name(look::CAPTION.into()))
                    .color(ui.visuals().error_fg_color),
            );
        }
        Some(Ok(clause)) => thin_clause_ui(ui, &clause, known),
        None => {}
    }
    drop_me
}

/// 一条子句眼下怎样：决定那三格怎么描边（设计稿 `.tc.bad` / `.tc.unfilled`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum 这一条怎样 {
    /// 读得成，进了规则。
    生效,
    /// 还没填值：值那一格描虚线。
    没填,
    /// 填了但读不成：三格描 `lo`。
    写错,
}

impl 这一条怎样 {
    /// 照 [`Leaf::clause`] 读出来的那一样分档。
    fn of(读: &Option<Result<Clause, RuleError>>) -> Self {
        match 读 {
            None => Self::没填,
            Some(Ok(_)) => Self::生效,
            Some(Err(_)) => Self::写错,
        }
    }
}

/// 一条子句左栏那两行：维度、运算符两颗下拉一行，值框一行。`宽` 是左栏的宽。
fn clause_controls_ui(
    ui: &mut egui::Ui,
    leaf: &mut Leaf,
    宽: f32,
    这一条: 这一条怎样,
    changed: &mut bool,
) {
    let tokens = Tokens::builtin();
    let 缝 = tokens.space.rule_gap;
    let [维度份, 运算符份] = tokens.layout.rule_clause_columns;
    let 维度宽 = ((宽 - 缝) * 维度份 / (维度份 + 运算符份)).max(0.0);
    let 运算符宽 = (宽 - 缝 - 维度宽).max(0.0);
    let 字号 = look::font_size(ui.ctx(), tokens.font.size_caption_plus);
    let 高 = tokens.layout.input_small_height;
    let 留白 = tokens.layout.rule_control_padding;
    小控件(ui, 高, 留白);
    let 红 = look::palette(ui).lo;
    let visuals = ui.visuals_mut();
    // 写错了：三格的描边都换 `lo`（稿上 `.tc.bad input,.tc.bad select{border-color:var(--lo)}`）；拿到焦点那一圈照旧强调色。
    if 这一条 == 这一条怎样::写错 {
        for widget in [
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.open,
        ] {
            widget.bg_stroke.color = 红;
        }
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 缝;
        let before = leaf.dimension;
        ComboBox::from_id_salt("维度")
            .selected_text(egui::RichText::new(leaf.dimension.label()).size(字号))
            .width(维度宽)
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
        // 收起与展开都写短词（「= 是」，核心库 `Op::short`），长句（「= 是其中之一」）挪去悬停：
        // 拿主意的人 2026-10-04 裁（差距清单 `F-4` 选 A）。
        let before = leaf.op;
        ComboBox::from_id_salt("运算符")
            .selected_text(egui::RichText::new(leaf.op.short()).size(字号))
            .width(运算符宽)
            .show_ui(ui, |ui| {
                for op in Op::for_dimension(leaf.dimension) {
                    ui.selectable_value(&mut leaf.op, op, op.short())
                        .on_hover_text(op.hint());
                }
            })
            .response
            .on_hover_text(leaf.op.hint());
        if leaf.op != before {
            *changed = true;
        }
    });
    // 没填值：值那一格描**虚线**（稿上 `.tc.unfilled input{border-style:dashed}`）——实线那一圈先让成透明，摆完再描虚线；
    // 指针停着、拿到焦点时照旧是那两档的实线。
    let 没填 = 这一条 == 这一条怎样::没填;
    let 值框 = ui
        .scope(|ui| {
            if 没填 {
                ui.visuals_mut().widgets.inactive.bg_stroke.color = egui::Color32::TRANSPARENT;
            }
            ui.add_sized(
                [宽, 高],
                egui::TextEdit::singleline(&mut leaf.value)
                    .font(egui::FontId::monospace(字号))
                    .background_color(look::palette(ui).panel)
                    .margin(egui::Margin::from(egui::vec2(留白, 0.0)))
                    .vertical_align(egui::Align::Center)
                    .hint_text(value_hint(leaf.dimension)),
            )
        })
        .inner;
    if leaf.focus {
        值框.request_focus();
        leaf.focus = false;
    }
    if 没填 && !值框.has_focus() && !值框.hovered() {
        let palette = look::palette(ui);
        look::dashed_outline(
            ui.painter(),
            值框.rect,
            egui::Stroke::new(tokens.layout.control_stroke, palette.line_2),
        );
    }
    if 值框.changed() {
        *changed = true;
    }
}

/// 这一条**筛不出东西**时贴在它底下的那几句（票 `gui-looks-like-the-design/12` 立的口径，这一票挪了位置）。
///
/// 与写错那一句（同一个位置、`lo` 色）、框底下那一行计数是两件事，颜色也不一样——**这一条是生效的**，只是眼下一个都选不中。
/// 当成错印成红的，人会去「改正」一条本来没错的规则（合集可以是待会儿才建的）。
///
/// **判在核心库**（`sublibrary::thin_clause`，ADR-0024），这一层只把交回来的话印出来。`clause` 是 [`leaf_ui`]
/// 走 [`Leaf::clause`] 读出来的那一条——与折成规则那一趟（[`group_of`]）同一处，所以这儿点名的就是规则里那一条；
/// 没填完、读不成的那几条不在规则里，轮不到这一问（读不成的贴它自己那一句，两样都数进框底下那一行）。
fn thin_clause_ui(ui: &mut egui::Ui, clause: &Clause, known: &KnownValues<'_>) {
    for thin in romcat_core::sublibrary::thin_clause(clause, known) {
        ui.label(
            egui::RichText::new(thin.advice())
                .text_style(egui::TextStyle::Name(crate::look::CAPTION.into()))
                .color(ui.visuals().warn_fg_color),
        )
        .on_hover_text(
            "这一条读得成、也存得进子库的规则，只是眼下一个变体都选不中。\
             不拦着你——合集可以是待会儿才建的，平台清单也会长。",
        );
    }
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
            assert_eq!(filter.not_in_effect(), 0);
        }
        filter.clear();
        assert!(filter.rule().is_none());
        assert!(filter.is_empty());
    }
}
