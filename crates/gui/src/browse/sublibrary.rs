//! **「加入子库」那个弹层**（票 `gui-looks-like-the-design/23`，设计稿 `openAddSub` / `renderAS`）。
//!
//! 筛到满意，把这一批加进某台设备的子库：可以**作为一条规则**加入（以后扫描到符合条件的
//! 新作品也自动落进来），也可以**只把勾中的那几个作品作为例外**加入。
//!
//! ## 这一层只画和转发（ADR-0024）
//!
//! 这个文件里没有一句领域判断，一个数都不自己算：
//!
//! - **预估那几个数**（新增多少、与已有规则重复多少、加入后多大、装不装得下）
//!   全部来自核心库一处 [`romcat_core::sublibrary::addition`]；设计稿那张对照表逐字
//!   写着「预估数字由核心库计算，界面只显示」。
//! - **「已经有同一条规则了」** 由 `Rule::same_one_in` 答（比树不比原文，挂单 `Q1180`）。
//! - **「装不装得下」** 由 `fit` 答，与差量预览同一条线。
//!
//! ## 算不出就写算不出
//!
//! 那一趟不便宜（`facts()` 走一遍全库，真机 343 毫秒），**摆不上画帧线**——
//! 所以它排任务台，算着的时候屏上写「正在算…」（[`Estimate::Working`]）。
//! 卡不在手边那一档核心库交回 `Fit::Unknown`，屏上照实写**「算不出」而不是 0**
//! （挂单 `Q591`、`Q1103` 同一条规矩）。

use romcat_core::report::{human_bytes, thousands};
use romcat_core::sublibrary::{Addition, Rule};

use crate::dialog::{Button, Dialog, Footer, Width};
use crate::font;
use crate::look;

/// 「加入到」那一列里的一台设备。**数都是外头算好递进来的。**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// 子库名。
    pub name: String,
    /// 眼下有几条规则。
    pub rules: usize,
    /// 眼下选中多少个变体。`None` 是**还没算出来**，不是零。
    pub picked: Option<u64>,
    /// 眼下选中多大。`None` 同上。
    pub bytes: Option<u64>,
    /// **这台里已经有同一条规则了**，就是它的序号。
    ///
    /// 判据 `Rule::same_one_in`（比树不比原文，`Q1180`），**由外头一次查库答**
    /// ——它只要读一遍这台的规则，不用折事实，所以不跟预估那一趟排队：
    /// 「按不动」的理由不该等一趟 343 毫秒的活。
    pub duplicate: Option<i64>,
    /// 容量上限；没设上限就是 `None`。
    pub capacity: Option<u64>,
}

impl Device {
    /// 副行那一句「N 条规则 · M 个变体 · X / 上限」（照稿）。
    ///
    /// **还没算出来的那几样写「正在算…」，不写 0**：写成「0 个变体」的话，
    /// 人会以为那个子库空了。「条数」不用算，一查就有，所以它一直是准的。
    #[must_use]
    pub fn line(&self) -> String {
        let 几个 = self.picked.map_or_else(
            || "正在算…".to_string(),
            |几个| format!("{} 个变体", thousands(几个)),
        );
        let 多大 = self
            .bytes
            .map_or_else(|| "正在算…".to_string(), human_bytes);
        let 上限 = self
            .capacity
            .map_or_else(|| "不限".to_string(), human_bytes);
        format!("{} 条规则 · {几个} · {多大} / {上限}", self.rules)
    }
}

/// 加入方式那两档（照稿 `.asmode`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// **作为规则加入**（稿上标着「推荐」）。
    Rule,
    /// **只加入勾中的那几个作品**，作为手动例外。
    Picked,
}

/// 预估那一块此刻是什么状况。
#[derive(Debug, Clone, PartialEq)]
pub enum Estimate {
    /// 正在算（那一趟排在任务台上）。
    Working,
    /// 算出来了。
    Done(Box<Addition>),
    /// 算不出，连同为什么。
    Failed(String),
}

/// 按下「加入」之后要干的那一件事。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deed {
    /// 加进哪一台。
    pub device: String,
    /// 作为规则，还是只加勾中的那几个。
    pub mode: Mode,
    /// 人给这条规则起的名字（**已经收拾过空白**）。空就是没起。
    pub rule_name: String,
    /// 按的是「加入并继续挑选」。
    pub keep_picking: bool,
}

impl Deed {
    /// 屏上这条规则叫什么：人起过就是那个，没起过拿 `Rule::label()` 现拼。
    ///
    /// **与 `StoredRule::shown_name` 同一条口径**——回执里写的那个名字，
    /// 必须与人下一眼在子库屏上看见的是同一个。
    #[must_use]
    pub fn rule_name_or(&self, rule: &Rule) -> String {
        if self.rule_name.is_empty() {
            rule.label()
        } else {
            self.rule_name.clone()
        }
    }
}

/// 画这一层要的那些**外头已经定好的事实**。
pub struct Facts<'a> {
    /// 库里有哪几台子库。空的话这一层只画空态。
    pub devices: &'a [Device],
    /// 当前筛选折成的那条规则。`None` 是**一个条件都没筛**。
    pub rule: Option<&'a Rule>,
    /// 条件组里有写错的子句时，那一句话。
    pub unruly: Option<&'a str>,
    /// 条件组里还有几条没填完（它们不进规则）。
    pub unfilled: usize,
    /// 只用于浏览、写不进规则的那几维（照稿那句说明）。
    pub browse_only: &'a [&'a str],
    /// 搜索框里还有字（搜索只影响排序，不写进规则）。
    pub searching: bool,
    /// 勾中了几个作品。
    pub picked: u64,
    /// 勾中的那几个摊开成多少个变体；`None` 是数不出来。
    pub picked_variants: Option<u64>,
    /// 预估那一块。
    pub estimate: &'a Estimate,
}

/// 按下去的是哪一颗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pressed {
    Dismiss,
    /// 「加入并继续挑选」。
    AddAndPick,
    /// 「加入」。
    Add,
    /// 空态那一颗「新建子库」。
    NewDevice,
}

/// 「加入子库」那个弹层此刻的样子。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AddTo {
    /// 选中哪一台；`None` 是库里一台都没有。
    target: Option<String>,
    /// 加入方式。默认「作为规则加入」（照稿）。
    mode: Option<Mode>,
    /// 「规则名称」那一格里打的字。
    name: String,
    /// 那一格**人动过没有**。没动过时它跟着当前筛选走（现拼的短名）。
    typed: bool,
    /// 这一帧按了「新建子库…」。下一句 `ui` 交回 [`Out::NewDevice`]。
    new_device: bool,
}

/// 这一层交回去的东西：要不要接着开着，以及按下去要干什么。
pub enum Out {
    /// 什么都没按。
    Open,
    /// 关掉。
    Closed,
    /// 加进去。
    Add(Deed),
    /// 空态那一颗「新建子库」：这一层关掉，由外头把人送去建子库。
    NewDevice,
}

impl AddTo {
    /// 开一层：**默认选头一台**（照稿 `S.as={dev:…}`），加入方式默认「作为规则」。
    #[must_use]
    pub fn open(devices: &[Device], toward: Option<&str>) -> Self {
        let target = toward
            .filter(|要去的| devices.iter().any(|一台| 一台.name == *要去的))
            .map(str::to_string)
            .or_else(|| devices.first().map(|一台| 一台.name.clone()));
        Self {
            target,
            mode: Some(Mode::Rule),
            name: String::new(),
            typed: false,
            new_device: false,
        }
    }

    /// 眼下选中哪一台。
    #[must_use]
    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    /// 眼下是哪一种加入方式。
    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode.unwrap_or(Mode::Rule)
    }

    /// 画这一层。
    pub fn ui(&mut self, ctx: &egui::Context, facts: &Facts<'_>) -> Out {
        if facts.devices.is_empty() {
            return self.empty_ui(ctx);
        }
        // **人没动过那一格时，名字跟着当前筛选走**（稿上 `S.as.name=r.name`）：
        // 换一个平台再开这一层，预填的就是新的短名，而不是上一趟那个。
        if !self.typed {
            self.name = facts.rule.map(Rule::label).unwrap_or_default();
        }
        let 拦住 = self.blocked(facts);
        let footer = Footer::new(Button::new("取消", Pressed::Dismiss))
            .button(
                Button::new("加入并继续挑选", Pressed::AddAndPick)
                    .enabled(拦住.is_none())
                    .hover(拦住.unwrap_or(
                        "加进去，并且留在这一屏接着换下一个平台——顶上那一条会记着累计。",
                    )),
            )
            .button(
                Button::new("加入", Pressed::Add)
                    .primary()
                    .enabled(拦住.is_none())
                    .hover(拦住.unwrap_or("加进去，然后回到子库屏。")),
            );
        let shown = Dialog::new("加入子库", "加入子库", footer)
            .note(
                "把筛选结果加入一台设备的子库。一个子库可以包含多条规则，结果取并集，\
                 适合按平台分几次加入。",
            )
            .width(Width::Wide)
            .show(ctx, |ui| self.body(ui, facts));
        if std::mem::take(&mut self.new_device) {
            return Out::NewDevice;
        }
        match shown.pressed {
            Some(Pressed::Dismiss) => Out::Closed,
            Some(Pressed::NewDevice) => Out::NewDevice,
            Some(按了 @ (Pressed::Add | Pressed::AddAndPick)) => {
                // 按不动的那两颗走不到这儿（`enabled(false)`）；真走到了宁可什么都不做，
                // 也不拿一条被拦下的规则去写库。
                if 拦住.is_some() {
                    return Out::Open;
                }
                let Some(device) = self.target.clone() else {
                    return Out::Open;
                };
                Out::Add(Deed {
                    device,
                    mode: self.mode(),
                    rule_name: self.name.trim().to_string(),
                    keep_picking: 按了 == Pressed::AddAndPick,
                })
            }
            None => Out::Open,
        }
    }

    /// **一台子库都没有那一档**（照稿）：不摆加入方式，只指一条路。
    fn empty_ui(&mut self, ctx: &egui::Context) -> Out {
        let footer = Footer::new(Button::new("取消", Pressed::Dismiss)).button(
            Button::new("新建子库", Pressed::NewDevice)
                .primary()
                .hover("一台设备一个子库。建好之后再回来把筛选结果加进去。"),
        );
        let shown = Dialog::new("加入子库", "加入子库", footer)
            .width(Width::Narrow)
            .show(ctx, |ui| {
                ui.label("还没有子库。先新建一个子库（对应一台设备），再把筛选结果加进去。");
            });
        match shown.pressed {
            Some(Pressed::NewDevice) => Out::NewDevice,
            Some(_) => Out::Closed,
            None => Out::Open,
        }
    }

    /// **按不动吗**；按不动就交回那句「为什么」（ADR-0005 那条「不禁按钮」：
    /// 灰着必须说得出为什么、以及怎么才能不灰）。
    fn blocked(&self, facts: &Facts<'_>) -> Option<&'static str> {
        if self.target.is_none() {
            return Some("先选一台设备。");
        }
        match self.mode() {
            Mode::Rule => {
                if facts.unruly.is_some() {
                    return Some("条件组里有写错的子句，改正之后才能加入。");
                }
                if facts.rule.is_none() {
                    return Some("一个条件都没筛——那样加进去的规则会把整个库收进这个子库。");
                }
                // **这一条不等预估那一趟**：判重只要读一遍这台的规则（`Q1180`），
                // 而预估要折一遍事实（343 毫秒）。让「按不动」等那么久，
                // 人会在它还亮着的时候按下去。
                if self
                    .chosen(facts)
                    .is_some_and(|一台| 一台.duplicate.is_some())
                {
                    return Some("这个子库里已经有同一条规则了。");
                }
                None
            }
            Mode::Picked => (facts.picked > 0).then_some(()).map_or(
                Some("先在列表里勾选作品，或者改用「作为规则加入」。"),
                |()| None,
            ),
        }
    }

    /// 眼下选中的是哪一台。
    fn chosen<'a>(&self, facts: &'a Facts<'_>) -> Option<&'a Device> {
        let target = self.target.as_deref()?;
        facts.devices.iter().find(|一台| 一台.name == target)
    }

    /// 内容区：加入到 → 加入方式 → 几句说明 → 预估。
    fn body(&mut self, ui: &mut egui::Ui, facts: &Facts<'_>) {
        look::section(ui, "加入到");
        for 一台 in facts.devices {
            let 选中 = self.target.as_deref() == Some(一台.name.as_str());
            if look::radio_option(ui, 选中, &一台.name, &一台.line()).clicked() {
                self.target = Some(一台.name.clone());
            }
        }
        // **「新建子库…」照稿摆在这一列底下**（`data-dg="open:subform|new"`）：
        // 一台设备一个子库，建的那一步在子库屏上（目标路径、前端格式、容量上限都在那儿问）。
        // **票 `23` 把左栏那块「存成子库」搬走之后，从浏览屏建一台走的就是这一条**（挂单 `Q873`）。
        if look::small_buttons(ui, |ui| {
            ui.scope(|ui| {
                look::ghost_button(ui.visuals_mut());
                ui.button("新建子库…")
            })
            .inner
            .on_hover_text("一台设备一个子库。去子库屏建一个，再回来把筛选结果加进去。")
            .clicked()
        }) {
            self.new_device = true;
        }

        一段之间(ui);
        look::section(ui, "加入方式");
        let 规则档 = look::radio_option(
            ui,
            self.mode() == Mode::Rule,
            "作为规则加入 · 推荐",
            "以后扫描到的新作品只要符合条件，也会自动加入。",
        );
        if 规则档.clicked() {
            self.mode = Some(Mode::Rule);
        }
        // **那条规则原样印出来**（照稿那一行等宽字）：按下去之前心里有数。
        if let Some(rule) = facts.rule {
            ui.label(font::mono(rule.text.clone()).weak());
        }
        if self.mode() == Mode::Rule {
            look::help(ui, "规则名称");
            let width = ui.available_width();
            if look::small_text_input(ui, width, egui::TextEdit::singleline(&mut self.name))
                .changed()
            {
                self.typed = true;
            }
        }

        let 勾了几个 = match facts.picked_variants {
            _ if facts.picked == 0 => "请先在列表中勾选作品。".to_string(),
            Some(几个) => format!(
                "作为手动例外加入（{} 个变体），不随筛选条件变化。",
                thousands(几个)
            ),
            // 数不出来就说数不出来（同 `Screen::scope` 那条：`None` 不是零）。
            None => "作为手动例外加入，不随筛选条件变化。".to_string(),
        };
        let 例外档 = look::radio_option(
            ui,
            self.mode() == Mode::Picked,
            &format!("只加入勾选的 {} 个作品", thousands(facts.picked)),
            &勾了几个,
        );
        if 例外档.clicked() && facts.picked > 0 {
            self.mode = Some(Mode::Picked);
        }

        // ── 几句照稿的说明 ─────────────────────────────────────────────────
        if facts.searching {
            look::note_box(ui, |ui| {
                look::weak_paragraph(ui, "搜索框中有内容：搜索只影响排序，不会写入规则。");
            });
        }
        if self.mode() == Mode::Rule {
            if facts.unfilled > 0 {
                look::note_box(ui, |ui| {
                    look::weak_paragraph(
                        ui,
                        &format!(
                            "条件组里有 {} 个未填写的子句，不会写入规则。",
                            facts.unfilled
                        ),
                    );
                });
            }
            if !facts.browse_only.is_empty() {
                look::note_box(ui, |ui| {
                    look::weak_paragraph(
                        ui,
                        &format!(
                            "「{}」只用于浏览，不会写入规则。",
                            facts.browse_only.join("」「")
                        ),
                    );
                });
            }
            if let Some(写错了) = facts.unruly {
                look::warn_box(ui, "条件组里有写错的子句", 写错了);
            }
        }

        // **「已经有这条规则了」不等预估那一趟**：它由 `Device::duplicate` 当场答
        // （读一遍那台的规则就有），而预估要折一遍事实。摆进预估那一块里的话，
        // 屏上会有一段时间既不说它重复、按钮却已经按不动了——**说不出为什么的灰**
        // 正是 ADR-0005 拦的那一种。
        if self.mode() == Mode::Rule
            && let Some(第几条) = self.chosen(facts).and_then(|一台| 一台.duplicate)
        {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!(
                    "「{}」中已经有这条规则了（第 {第几条} 条）。",
                    self.target.as_deref().unwrap_or_default()
                ),
            );
        }

        一段之间(ui);
        look::section(ui, "预估");
        self.estimate_ui(ui, facts);
    }

    /// 预估那一块（照稿 `.estbox` 三格）。**一个数都不在这儿算。**
    fn estimate_ui(&self, ui: &mut egui::Ui, facts: &Facts<'_>) {
        match facts.estimate {
            Estimate::Working => {
                look::help(ui, "正在算…");
            }
            Estimate::Failed(为什么) => {
                ui.colored_label(ui.visuals().error_fg_color, format!("算不出：{为什么}"));
            }
            Estimate::Done(账) => {
                let 已经有了 = self.chosen(facts).and_then(|一台| 一台.duplicate);
                let 重复那句 = if 已经有了.is_some() {
                    "这条规则已存在"
                } else {
                    "与已有规则重复（不重复计算）"
                };
                三格(
                    ui,
                    &[
                        (
                            format!("+{}", thousands(账.added)),
                            format!("新增变体 · {}", human_bytes(账.added_bytes)),
                        ),
                        (thousands(账.overlap), 重复那句.to_string()),
                        match 账.after.known() {
                            Some(room) => (
                                human_bytes(room.after_bytes),
                                room.capacity.map_or_else(
                                    || "加入后 / 不限".to_string(),
                                    |上限| format!("加入后 / {}", human_bytes(上限)),
                                ),
                            ),
                            // **`Fit::Unknown` 是答案的一种，不是零**（`Q591`）：
                            // 卡不在手边时照实写，不拿选中容量去冒充「加入后多大」。
                            None => ("算不出".to_string(), "加入后 · 目标不在位".to_string()),
                        },
                    ],
                );
                if let Some(room) = 账.after.known()
                    && let Some(超出) = room.over_capacity
                {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        format!(
                            "加入后超出容量上限 {}。不会自动删减，子库页会给出删减建议。",
                            human_bytes(超出)
                        ),
                    );
                }
            }
        }
    }
}

/// 预估那三格（照稿 `.estbox`）：大字一行、说明一行。
fn 三格(ui: &mut egui::Ui, 几格: &[(String, String)]) {
    ui.horizontal_top(|ui| {
        for (大字, 说明) in 几格 {
            ui.vertical(|ui| {
                ui.label(font::strong(大字.clone()));
                look::help(ui, 说明);
            });
        }
    });
}

/// 弹层里一段与一段之间的留白。
fn 一段之间(ui: &mut egui::Ui) {
    ui.add_space(crate::tokens::Tokens::builtin().space.pane_gap);
}
