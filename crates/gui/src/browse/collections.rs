//! **收藏与合集那两个弹层**（票 `gui-looks-like-the-design/13`，设计稿 `DLG.coll`）。
//!
//! 「管理合集」（改名 / 删除 / 按它筛选）与「加入合集」（选已有或新建）。票 09 把这几样
//! 缩成小号垫在左栏最底下（挂单 `Q873`），这一票照稿搬进弹层，左栏只留标签簇与
//! 一颗「管理合集…」。
//!
//! ## 这一层只画和转发（ADR-0024）
//!
//! 这个文件里没有一句领域判断：
//!
//! - **名字用不用得上**走 [`collection::check_name`]（稿上 `collNameErr` 那四条判在一处）；
//! - **改名**走 [`collection::rename`]，它连引用那个合集的子库规则一起改；
//! - **删除**走 [`collection::drop_all`]（＝把成员全部移出），
//!   **写着它的规则一个字不改**，理由在那个函数的文档里；
//! - **有几条规则提到它**走 [`collection::rules_naming`]，删之前那句警告要它。
//!   （那一趟不看运算符，`合集!=X` 也算，所以屏上只说「提到」——见 `删除确认`。）
//!
//! ## 两处都不自己数数
//!
//! 每个合集有几个作品，读的是筛选栏那一趟已经问回来的分面（`Facets::collections`），
//! 不另查一遍库——**屏上两处写着不同的数，是这一屏最容易犯的错**（ADR-0024 在前三票
//! 各栽过一次）。

use romcat_core::catalog::browse::Facet;
use romcat_core::collection::{self, FAVORITE};

use crate::dialog::{Button, Dialog, Footer, Width};
use crate::font;
use crate::look;

/// 左栏「收藏与合集」那一行右头那颗按钮上写的字（稿上 `.btn.ghost.sm`）。
///
/// 摆它的人要先量它多宽才摆得到右边沿，量的与摆的得是同一串字——单摆一处，省得哪天
/// 改字只改了一边，按钮就离右边沿差出几个像素。
pub const MANAGE: &str = "管理合集…";

/// 「管理合集」那个弹层此刻的样子。
///
/// **改名与删除都是「就地展开」**（照稿）：不再叠一层弹层——两层叠着时 Esc 一下只退
/// 一层，而人按 Esc 想退的是整个「管理合集」。
#[derive(Debug, Clone, Default)]
pub struct Manage {
    /// 正在改名的是哪一个（原名），以及框里正打着的新名字。
    renaming: Option<(String, String)>,
    /// 正等着确认删掉的是哪一个，以及**有几条子库规则提到它**（`None` 是数不出来）。
    ///
    /// 那个数在按下「删除…」那一下问一次就存着：删除确认那一段每帧都要印它，
    /// 而它要走遍每个子库的每条规则（[`collection::rules_naming`]）——
    /// 每帧问一次就是把那趟遍历摆到画帧线上。
    deleting: Option<(String, Option<usize>)>,
}

/// 「管理合集」里按下去要干的那一件事。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Managed {
    /// 按它筛选：关掉弹层，把这个合集写进筛选。
    Filter(String),
    /// 改名。
    Rename {
        /// 原来那个名字。
        from: String,
        /// 改成的那个（已经过 [`collection::check_name`]、收拾过空白）。
        to: String,
    },
    /// 删掉这一个（＝成员全部移出）。
    Drop(String),
}

/// 「加入合集」那个弹层此刻的样子。
#[derive(Debug, Clone, Default)]
pub struct Join {
    /// 往哪个合集加；`None` 是「新建合集」那一档。
    target: Option<String>,
    /// 「新建合集」那一档里打的名字。
    fresh: String,
}

impl Join {
    /// 开一层：**库里已经有合集就默认头一个**，一个都没有就默认「新建合集」（照稿）。
    #[must_use]
    pub fn open(collections: &[Facet]) -> Self {
        Self {
            target: collections
                .iter()
                .find(|一个| 一个.value != FAVORITE)
                .map(|一个| 一个.value.clone()),
            fresh: String::new(),
        }
    }
}

/// 页脚上按的是哪一颗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pressed {
    /// 「完成」／「取消」——Esc 等于按它。
    Dismiss,
    /// 「加入」。
    Add,
}

impl Manage {
    /// 画「管理合集」。交回 `(还开着吗, 按下去要干什么)`。
    ///
    /// `collections` 是分面那一趟问回来的「哪几个合集、各几个作品」；
    /// `rules_naming` 是问「有几条子库规则提到这个合集」的那一下——**由调用方递进来**，
    /// 这一层够不着中立库（而且那一趟要遍历每个子库的每条规则，不能每帧跑）。
    /// 它交回 `None` 是**数不出来**，不是零（见 `删除确认`）。
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        collections: &[Facet],
        rules_naming: &mut dyn FnMut(&str) -> Option<usize>,
    ) -> (bool, Option<Managed>) {
        let footer =
            Footer::new(Button::new("完成", Pressed::Dismiss).primary()).dismiss_on_right();
        let shown = Dialog::new("管理合集", "管理合集", footer)
            .note(
                "合集是你自己起名的一组作品，与平台无关。成员按文件内容记录，\
                 文件改名或移动后仍然认得出。",
            )
            .width(Width::Standard)
            .show(ctx, |ui| self.body(ui, collections, rules_naming));
        let 关掉 = shown.pressed == Some(Pressed::Dismiss);
        // **按了「按它筛选」也关掉**：那一下的意思是「我要去看这一批」，
        // 而弹层盖着表格（照稿 `act:filter` 那一支 `closeDlg()`）。
        let 动作 = shown.inner;
        let 还开着 = !关掉 && !matches!(动作, Some(Managed::Filter(_)));
        (还开着, 动作)
    }

    /// 内容区：收藏那一行，然后每个合集一行。
    fn body(
        &mut self,
        ui: &mut egui::Ui,
        collections: &[Facet],
        rules_naming: &mut dyn FnMut(&str) -> Option<usize>,
    ) -> Option<Managed> {
        let mut 动作 = None;
        // ── 收藏那一行：**照稿写明它改不动也删不掉** ──────────────────────────
        //
        // ADR-0005 那条「不禁按钮」：这一行上本来就没有「改名」「删除」两颗，
        // 而屏上说得出为什么——它是默认的那一组。
        let 收藏几个 = 几个(collections, FAVORITE);
        ui.horizontal_top(|ui| {
            ui.label(font::strong(format!("★ {FAVORITE}")));
            look::help(ui, &format!("· {收藏几个} · 默认的一组，不能改名或删除"));
            if look::small_buttons(ui, |ui| ui.button("按它筛选").clicked()) {
                动作 = Some(Managed::Filter(FAVORITE.to_string()));
            }
        });

        // ── 自建的那几个 ────────────────────────────────────────────────────
        let 自建的: Vec<&Facet> = collections
            .iter()
            .filter(|一个| 一个.value != FAVORITE)
            .collect();
        if 自建的.is_empty() {
            一段之间(ui);
            ui.weak("还没有合集。");
        }
        for 一个 in 自建的 {
            一段之间(ui);
            let name = 一个.value.clone();
            // **正在改名的那一行换成一个输入框**（照稿就地展开）。
            if let Some((改的是, 打的字)) = &mut self.renaming
                && *改的是 == name
            {
                {
                    let 存了 = 改名那一行(ui, &name, 打的字, collections);
                    match 存了 {
                        Some(RenameRow::Cancel) => self.renaming = None,
                        Some(RenameRow::Save(到)) => {
                            动作 = Some(Managed::Rename {
                                from: name.clone(),
                                to: 到,
                            });
                            self.renaming = None;
                        }
                        None => {}
                    }
                    continue;
                }
            }
            let (筛, 改, 删) = 一行(ui, &name, 一个.count);
            if 筛 {
                动作 = Some(Managed::Filter(name.clone()));
            }
            if 改 {
                // 框里先填着原名：改名多半是改一两个字，不是从头打一遍。
                self.renaming = Some((name.clone(), name.clone()));
                self.deleting = None;
            }
            if 删 {
                // **那个数在这一下问一次就存着**，别每帧遍历子库（见 `deleting` 的文档）。
                self.deleting = Some((name.clone(), rules_naming(&name)));
                self.renaming = None;
            }
            // **删除确认就地展开在这一行底下**（照稿那个 `warnbox`）。
            if let Some((删的是, 几条规则)) = &self.deleting
                && *删的是 == name
            {
                {
                    match 删除确认(ui, &name, 一个.count, *几条规则) {
                        Some(true) => {
                            动作 = Some(Managed::Drop(name.clone()));
                            self.deleting = None;
                        }
                        Some(false) => self.deleting = None,
                        None => {}
                    }
                }
            }
        }

        一段之间(ui);
        look::help(
            ui,
            "新建合集：在浏览中勾选一批作品，点「加入合集…」，选择「新建合集」并起个名字。",
        );
        动作
    }
}

/// 改名那一行按下去的是哪一颗。
enum RenameRow {
    Cancel,
    Save(String),
}

/// 改名那一行：一个输入框、一句错、「取消」与「保存」。
///
/// **名字用不用得上由核心库答**（[`collection::check_name`]），这儿只把那句话印出来、
/// 并按它决定「保存」按不按得动。**把被改的那一个先剔掉**——不剔的话「改成它自己」
/// 会被判成重名。
fn 改名那一行(
    ui: &mut egui::Ui,
    原名: &str,
    打的字: &mut String,
    collections: &[Facet],
) -> Option<RenameRow> {
    let 已有: Vec<String> = collections
        .iter()
        .map(|一个| 一个.value.clone())
        .filter(|一个| 一个 != 原名)
        .collect();
    let 判 = collection::check_name(打的字, &已有);
    let mut 按了 = None;
    ui.horizontal_top(|ui| {
        let width = (ui.available_width() * 0.5).max(120.0);
        look::small_text_input(ui, width, egui::TextEdit::singleline(打的字));
        let (取消, 保存) = look::small_buttons(ui, |ui| {
            let 取消 = ui.button("取消").clicked();
            let 保存 = ui
                .add_enabled(判.is_ok(), egui::Button::new("保存"))
                .on_hover_text(match &判 {
                    Ok(_) => "改完名，写着这个合集的子库规则一并更新。".to_string(),
                    Err(不行) => 不行.advice(),
                })
                .clicked();
            (取消, 保存)
        });
        if 取消 {
            按了 = Some(RenameRow::Cancel);
        }
        if 保存 && let Ok(到) = &判 {
            按了 = Some(RenameRow::Save(到.clone()));
        }
    });
    // **为什么按不动，屏上说得出**（ADR-0005）：不只挂在悬停里——空着那一档不说，
    // 人还没打字就先见一句红的。
    if let Err(不行) = &判
        && !打的字.trim().is_empty()
    {
        ui.colored_label(ui.visuals().error_fg_color, 不行.advice());
    }
    按了
}

/// 一个合集一行：名字、几个作品、以及「按它筛选 / 改名 / 删除…」。
fn 一行(ui: &mut egui::Ui, name: &str, count: u64) -> (bool, bool, bool) {
    let mut out = (false, false, false);
    ui.horizontal_top(|ui| {
        ui.label(font::strong(name));
        look::help(ui, &format!("· {}", 多少个变体(count)));
        let (筛, 改, 删) = look::small_buttons(ui, |ui| {
            let 筛 = ui.button("按它筛选").clicked();
            let 改 = ui.button("改名").clicked();
            let 删 = ui
                .button("删除…")
                .on_hover_text("删除就是把成员全部移出，作品本身不受影响。")
                .clicked();
            (筛, 改, 删)
        });
        out = (筛, 改, 删);
    });
    out
}

/// 删除确认那一块（照稿那个 `warnbox`）：交回 `Some(true)` 是真删，
/// `Some(false)` 是取消，`None` 是还没定。
///
/// **写着它的那几条规则照实说**：这一层**不替人改它们**
/// （理由在 [`collection::drop_all`] 的文档里）。
///
/// `几条规则` 是 `None` 时说「数不出来」，**不写 0**：读不动中立库与「一条都没有」
/// 是两件事，把前者印成后者就是现编一个数（同一条规矩下 `Q1103` 拦掉过一个 N）。
///
/// ⚠️ **那句话不许说「写着 `合集=X`、删完筛不出东西」。** `Rule::names_collection`
/// 不看运算符，`合集!=X` 也算在内，而对它那三句全是反的：不是 `合集=`，
/// 删掉之后它选中的是**更多**，也没有什么可「救回来」。所以只说「提到」，
/// 不承诺方向——同一个形状上票 12 的 `thin_clause` 栽过一次（挂单 `Q1101`）。
fn 删除确认(
    ui: &mut egui::Ui,
    name: &str,
    count: u64,
    几条规则: Option<usize>,
) -> Option<bool> {
    let mut 定了 = None;
    let 头一句 = format!(
        "就是把它的 {} 全部移出，作品本身不受影响。",
        多少个变体(count)
    );
    let 那句话 = match 几条规则 {
        Some(0) => 头一句,
        Some(几条) => format!(
            "{头一句}子库里有 {几条} 条规则提到这个合集，删除后那几条筛出来的会跟着变\
             ——它们留着不动，你可以再建一个同名的合集把它们救回来。"
        ),
        None => format!("{头一句}有几条子库规则提到这个合集，这会儿数不出来（中立库读不动）。"),
    };
    look::warn_box(ui, &format!("删除合集「{name}」"), &那句话);
    ui.horizontal_top(|ui| {
        let (取消, 删) = look::small_buttons(ui, |ui| {
            let 取消 = ui.button("取消").clicked();
            let 删 = ui.button("删除合集").clicked();
            (取消, 删)
        });
        if 取消 {
            定了 = Some(false);
        }
        if 删 {
            定了 = Some(true);
        }
    });
    定了
}

impl Join {
    /// 画「加入合集」。交回 `(还开着吗, 加进哪个合集)`。
    ///
    /// `scope` 是屏上那句「把……加入合集」里的那一段，**调用方已经接好量词**
    /// （「勾中的 1,284 个变体」）。这一层不碰那个数，也不替它认单位。
    ///
    /// ⚠️ **稿上这儿还有一句「其中 N 个只能按路径记录」，这一版印不出那个 `N`**
    /// （挂单 `Q1103`）：那个数要为这一批每个变体折一次内容判据
    /// （`collection::anchor_of` ← `identify::content_prints`），而全选那一档在真库上是
    /// 46,444 个变体、秒级的读——那是**排上任务台**才做得起的事（`Screen::queue_collection`
    /// 正是这么排的），不是画一帧弹层时顺手算得出的。
    /// 所以这一层只指路：**按下去之后的回执里会点名说有几个**，而那句回执的数是准的。
    /// 宁可少印一个数，也不印一个现编的。
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        collections: &[Facet],
        scope: &str,
    ) -> (bool, Option<String>) {
        let 已有: Vec<String> = collections
            .iter()
            .map(|一个| 一个.value.clone())
            .filter(|一个| 一个 != FAVORITE)
            .collect();
        // 选了已有的那一档就一定用得上；「新建合集」那一档要过四条校验。
        let 加得进 = match &self.target {
            Some(_) => Ok(String::new()),
            None => collection::check_name(&self.fresh, &已有),
        };
        let footer = Footer::new(Button::new("取消", Pressed::Dismiss)).button(
            Button::new("加入", Pressed::Add)
                .primary()
                .enabled(加得进.is_ok())
                .hover(match &加得进 {
                    Ok(_) => {
                        "落沉淀库、锚在内容上——删掉中立库重扫、改名、挪目录都还在。".to_string()
                    }
                    Err(不行) => 不行.advice(),
                }),
        );
        let shown = Dialog::new("加入合集", "加入合集", footer)
            .note(format!("把{scope}加入合集。"))
            .width(Width::Narrow)
            .show(ctx, |ui| self.body(ui, collections));
        match shown.pressed {
            Some(Pressed::Dismiss) => (false, None),
            Some(Pressed::Add) => {
                let name = match &self.target {
                    Some(一个) => 一个.clone(),
                    None => match 加得进 {
                        Ok(收拾过的) => 收拾过的,
                        // 按不动的那一档走不到这儿（`enabled(false)`）；
                        // 真走到了宁可什么都不做，也不拿一个没过校验的名字去写库。
                        Err(_) => return (true, None),
                    },
                };
                (false, Some(name))
            }
            None => (true, None),
        }
    }

    /// 内容区：已有那几个各一档、「新建合集」一档，外加路径锚那句警告。
    fn body(&mut self, ui: &mut egui::Ui, collections: &[Facet]) {
        for 一个 in collections.iter().filter(|一个| 一个.value != FAVORITE) {
            let 选中 = self.target.as_deref() == Some(一个.value.as_str());
            if look::radio_option(ui, 选中, &一个.value, &多少个变体(一个.count)).clicked()
            {
                self.target = Some(一个.value.clone());
            }
        }
        if look::radio_option(
            ui,
            self.target.is_none(),
            "新建合集",
            "用勾选的作品建一个新合集",
        )
        .clicked()
        {
            self.target = None;
        }
        if self.target.is_none() {
            let width = ui.available_width();
            look::small_text_input(
                ui,
                width,
                egui::TextEdit::singleline(&mut self.fresh).hint_text("例如：通关过的"),
            );
            // **为什么「加入」按不动，屏上说得出**（ADR-0005）：空着那一档不说，
            // 人还没打字就先见一句红的。
            let 已有: Vec<String> = collections
                .iter()
                .map(|一个| 一个.value.clone())
                .filter(|一个| 一个 != FAVORITE)
                .collect();
            if let Err(不行) = collection::check_name(&self.fresh, &已有)
                && !self.fresh.trim().is_empty()
            {
                ui.colored_label(ui.visuals().error_fg_color, 不行.advice());
            }
        }
        // **挂不住内容锚那件事照实说，但不现编那个数**（挂单 `Q1103`，理由见 `ui` 的文档）：
        // 稿上这儿写的是「其中 N 个只能按路径记录」，而那个 N 要为这一批每个变体折一次
        // 内容判据——全选那一档在真库上是四万多个变体、秒级的读，排任务台才做得起。
        // **不含糊成一句「加好了」**（ADR-0021 那条纪律在这一屏上的样子），
        // 也不印一个现编的数：说清有这一档、并指向那句准的回执。
        一段之间(ui);
        look::warn_box(
            ui,
            "拿不到内容判据的那些只钉得住本机路径。",
            "无判据那一档的变体，成员关系只能按文件路径记下来——文件改名或挪到别的目录\
             之后，它们会从这个合集里消失（识别出作品之后自动改成按内容记录）。\
             这一批里有几个是这样，加进去之后的回执里会点名说。",
        );
    }
}

/// 「N 个变体」那半句。
///
/// ⚠️ **写「变体」不写「作品」，因为这个数数的就是变体。** 它来自
/// [`Facet::count`]，核心库那一侧写着「这个值选中多少个变体」，那条 SQL 是
/// `COUNT(*) FROM collection_variant`——一行一个变体。
/// 稿上 `DLG.coll` 那几处写的是「N 个作品」（`c.ids` 装的是作品下标），
/// **那是另一个数**：一部作品挂三个变体，稿上写 1、这儿的来源是 3。
///
/// 要照稿写「个作品」得在合集这一维上另问一趟 `COUNT(DISTINCT work_id)`，
/// 那就破了这一屏「屏上的数只有一个来源、不自己数第二遍」的规矩（模块头那一段）。
/// 与其印一个单位对不上的数，不如照实写它是什么——挂单 `Q1107`。
fn 多少个变体(count: u64) -> String {
    format!("{} 个变体", romcat_core::report::thousands(count))
}

/// 这个合集在分面上写着几个。分面里没有它就是 0——**不另查一遍库**。
fn 几个(collections: &[Facet], name: &str) -> String {
    多少个变体(
        collections
            .iter()
            .find(|一个| 一个.value == name)
            .map_or(0, |一个| 一个.count),
    )
}

/// 两段之间的留白：`browse.rs` 那个 `section_gap` 是私有的，这儿照同一个算法要一份。
fn 一段之间(ui: &mut egui::Ui) {
    let gap = crate::tokens::Tokens::builtin().space.section_gap - ui.spacing().item_spacing.y;
    ui.add_space(gap.max(0.0));
}
