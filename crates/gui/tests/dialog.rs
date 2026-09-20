//! **弹层的统一写法**（[`romcat_gui::dialog`]）：遮罩、标题与说明、可滚动的内容区、页脚按钮、
//! 四档宽度、Esc 与焦点。
//!
//! 这几条不借任何一屏：底下那一屏是这份文件自己搭的一块小屏（一颗「打开」、一个单键快捷键），
//! 于是验的是**这一套写法本身**，而不是哪一处恰好照它办了。各屏照它办的那一半在各屏自己的
//! 测试里（`program.rs` 的向导、`browse.rs` 的刮削、`queue.rs` 的计划书）。

use romcat_gui::dialog::{self, Button, Dialog, Footer, Width};
use romcat_gui::tokens::Tokens;
use romcat_gui::{headless, look};

mod shared;
use shared::{按键事件, 输入};

/// 页脚上按下去的是哪一颗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum 按的 {
    /// 退出那一颗：Esc 等于按它。
    关上,
    /// 主按钮那一颗。
    保存,
}

/// 底下那一屏：一颗「打开」、一个单键快捷键 `N`，外加它打开的那一层弹层。
#[derive(Default)]
struct 小屏 {
    /// `N` 在这一屏上生效过几次。
    按过几次: usize,
    /// 弹层开着没有。
    开着: bool,
}

impl 小屏 {
    fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        // 底下那一屏的单键快捷键：**问过那道门才接**。
        if dialog::screen_has_keys(&ctx) && ctx.input(|input| input.key_pressed(egui::Key::N)) {
            self.按过几次 += 1;
        }
        if ui.button("打开").clicked() {
            self.开着 = true;
        }
        if self.开着 {
            let shown = Dialog::new(
                "测试弹层",
                "一层弹层",
                Footer::new(Button::new("关上", 按的::关上)),
            )
            .show(&ctx, |ui| {
                ui.label("里头的一句话");
            });
            if shown.pressed == Some(按的::关上) {
                self.开着 = false;
            }
        }
    }
}

/// 带着这几个事件跑一帧。
fn 跑一帧(ctx: &egui::Context, 屏: &mut 小屏, events: Vec<egui::Event>) -> egui::FullOutput {
    headless::frame(ctx, 输入(events), |ui| 屏.ui(ui))
}

/// 一个装好字体与观感基线的上下文：弹层的标题要令牌那几档字号。
fn 上下文() -> egui::Context {
    let ctx = headless::context();
    look::install(&ctx);
    ctx
}

/// 指针挪到 `指针在`，滚轮往竖直方向滚 `竖着` 点（正数往回滚到顶、负数往下滚到底）。
fn 滚一下(指针在: egui::Pos2, 竖着: f32) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(指针在),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 竖着),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

#[test]
fn 弹层开着时底下那一屏的快捷键不生效_按退出键关得掉() {
    let ctx = 上下文();
    let mut 屏 = 小屏 {
        开着: true,
        ..小屏::default()
    };
    // 头一帧 egui 在量这一层多大（不画、也不接），第二帧才摆稳。
    跑一帧(&ctx, &mut 屏, Vec::new());
    跑一帧(&ctx, &mut 屏, Vec::new());

    跑一帧(&ctx, &mut 屏, vec![按键事件(egui::Key::N)]);
    assert_eq!(屏.按过几次, 0, "弹层开着，底下那一屏的 N 照样生效了");

    跑一帧(&ctx, &mut 屏, vec![按键事件(egui::Key::Escape)]);
    assert!(!屏.开着, "Esc 没关掉弹层");

    // 关上那一下补的那一帧（弹层要了一次重画）。
    跑一帧(&ctx, &mut 屏, Vec::new());
    跑一帧(&ctx, &mut 屏, vec![按键事件(egui::Key::N)]);
    assert_eq!(屏.按过几次, 1, "弹层关上之后底下那一屏的 N 还是不生效");
}

/// 页脚上一颗**危险按钮**，能不能按由外头拨（票 `gui-looks-like-the-design/26`）。
struct 危险那一颗屏 {
    /// 那一颗按得动吗。
    能按: bool,
    /// 真按下去过几次。
    按下过: usize,
}

impl 危险那一颗屏 {
    fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let footer = Footer::new(Button::new("取消", 按的::关上))
            .button(Button::new("删掉", 按的::保存).danger().enabled(self.能按));
        let shown = Dialog::new("危险", "删掉一样东西", footer).show(&ctx, |ui| {
            ui.label("按下去就真删了");
        });
        if shown.pressed == Some(按的::保存) {
            self.按下过 += 1;
        }
    }
}

/// 屏上正好写着那几个字的地方在哪儿。
fn 字画在哪儿(output: &egui::FullOutput, 那几个字: &str) -> Option<egui::Pos2> {
    fn 找(shape: &egui::epaint::Shape, 那几个字: &str, out: &mut Option<egui::Pos2>) {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == 那几个字 => {
                *out = Some(egui::Rect::from_min_size(text.pos, text.galley.size()).center());
            }
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    找(one, 那几个字, out);
                }
            }
            _ => {}
        }
    }
    let mut out = None;
    for shape in &output.shapes {
        找(&shape.shape, 那几个字, &mut out);
    }
    out
}

#[test]
fn 页脚上按不动的危险按钮真的按不下去_拨成能按就按得下去() {
    // 票 `gui-looks-like-the-design/26` 验收第 3 条那一半的底：颜色由截图门守，
    // 「按不动」这件事本身由这一条钉着。走的是屏上那条一模一样的路——真发指针事件去点它。
    let ctx = 上下文();
    let mut 屏 = 危险那一颗屏 {
        能按: false,
        按下过: 0,
    };
    // 头一帧在量这一层多大，第二帧才摆稳。
    跑一帧危险(&ctx, &mut 屏, Vec::new());
    let 稳了 = 跑一帧危险(&ctx, &mut 屏, Vec::new());
    let 在 = 字画在哪儿(&稳了, "删掉").expect("屏上有这颗按钮");

    跑一帧危险(&ctx, &mut 屏, 点一下(在));
    跑一帧危险(&ctx, &mut 屏, Vec::new());
    assert_eq!(屏.按下过, 0, "按不动的那一颗被按下去了");

    屏.能按 = true;
    跑一帧危险(&ctx, &mut 屏, Vec::new());
    跑一帧危险(&ctx, &mut 屏, 点一下(在));
    跑一帧危险(&ctx, &mut 屏, Vec::new());
    assert_eq!(屏.按下过, 1, "拨成能按之后还是按不下去");
}

/// 在 `在` 那一点按下再松开。
fn 点一下(在: egui::Pos2) -> Vec<egui::Event> {
    let 键 = |pressed: bool| egui::Event::PointerButton {
        pos: 在,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    vec![egui::Event::PointerMoved(在), 键(true), 键(false)]
}

/// 带着这几个事件跑一帧危险那一颗屏。
fn 跑一帧危险(
    ctx: &egui::Context,
    屏: &mut 危险那一颗屏,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    headless::frame(ctx, 输入(events), |ui| 屏.ui(ui))
}

// ——— 焦点与一层一层退 ———

/// 一颗「打开」，打开的那一层里头只有一个输入框。记下两样的 id。
#[derive(Default)]
struct 带输入框的屏 {
    开着: bool,
    打开那颗: Option<egui::Id>,
    里头那一框: Option<egui::Id>,
    草稿: String,
}

impl 带输入框的屏 {
    fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let 打开 = ui.button("打开");
        self.打开那颗 = Some(打开.id);
        if 打开.clicked() {
            self.开着 = true;
        }
        if self.开着 {
            let 草稿 = &mut self.草稿;
            let shown = Dialog::new(
                "焦点",
                "看焦点落哪儿",
                Footer::new(Button::new("关上", 按的::关上)),
            )
            .show(&ctx, |ui| ui.text_edit_singleline(草稿).id);
            self.里头那一框 = Some(shown.inner);
            if shown.pressed.is_some() {
                self.开着 = false;
            }
        }
    }
}

/// 眼下拿着键盘焦点的是谁。
fn 焦点在(ctx: &egui::Context) -> Option<egui::Id> {
    ctx.memory(egui::Memory::focused)
}

#[test]
fn 打开时焦点落进弹层_关掉之后回到原来那颗按钮() {
    let ctx = 上下文();
    let mut 屏 = 带输入框的屏::default();
    let 跑 = |屏: &mut 带输入框的屏, events: Vec<egui::Event>| {
        headless::frame(&ctx, 输入(events), |ui| 屏.ui(ui));
    };
    跑(&mut 屏, Vec::new());
    跑(&mut 屏, vec![按键事件(egui::Key::Tab)]);
    跑(&mut 屏, Vec::new());
    assert_eq!(
        焦点在(&ctx),
        屏.打开那颗,
        "前提：Tab 把焦点放到了「打开」上"
    );

    // 键盘上按下「打开」：整条路一下鼠标都不碰。
    跑(&mut 屏, vec![按键事件(egui::Key::Space)]);
    assert!(屏.开着, "前提：空格按下了「打开」");
    for _ in 0..3 {
        跑(&mut 屏, Vec::new());
    }
    assert_eq!(
        焦点在(&ctx),
        屏.里头那一框,
        "弹层开着，焦点却没落进里头那一框——人得先拿鼠标点进去才打得了字",
    );

    跑(&mut 屏, vec![按键事件(egui::Key::Escape)]);
    assert!(!屏.开着, "Esc 没关掉弹层");
    跑(&mut 屏, Vec::new());
    assert_eq!(
        焦点在(&ctx),
        屏.打开那颗,
        "弹层关上了，焦点没回到打开它的那颗按钮——键盘上的人得从头 Tab 一遍",
    );
}

/// 两层：底下那一层里一颗「再开一层」。
#[derive(Default)]
struct 两层 {
    下层开着: bool,
    上层开着: bool,
}

impl 两层 {
    fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.下层开着 {
            let mut 再开 = false;
            let shown = Dialog::new(
                "下层",
                "下层",
                Footer::new(Button::new("关上下层", 按的::关上)),
            )
            .show(&ctx, |ui| 再开 = ui.button("再开一层").clicked());
            if 再开 {
                self.上层开着 = true;
            }
            if shown.pressed.is_some() {
                self.下层开着 = false;
            }
        }
        if self.上层开着 {
            let shown = Dialog::new(
                "上层",
                "上层",
                Footer::new(Button::new("关上上层", 按的::关上)),
            )
            .width(Width::Narrow)
            .show(&ctx, |ui| {
                ui.label("上层里头");
            });
            if shown.pressed.is_some() {
                self.上层开着 = false;
            }
        }
    }
}

#[test]
fn 退出键一次只关最上面那一层() {
    let ctx = 上下文();
    let mut 屏 = 两层 {
        下层开着: true,
        上层开着: false,
    };
    let 跑 = |屏: &mut 两层, events: Vec<egui::Event>| {
        headless::frame(&ctx, 输入(events), |ui| 屏.ui(ui));
    };
    for _ in 0..3 {
        跑(&mut 屏, Vec::new());
    }
    屏.上层开着 = true;
    for _ in 0..3 {
        跑(&mut 屏, Vec::new());
    }

    跑(&mut 屏, vec![按键事件(egui::Key::Escape)]);
    assert!(!屏.上层开着, "Esc 没关掉上面那一层");
    assert!(屏.下层开着, "一下 Esc 把两层一起关了");

    跑(&mut 屏, Vec::new());
    跑(&mut 屏, vec![按键事件(egui::Key::Escape)]);
    assert!(!屏.下层开着, "第二下 Esc 没关掉剩下那一层");
}

/// 内容区里先是三百行字、最底下一个输入框。
#[derive(Default)]
struct 输入框在底下 {
    那一框: Option<egui::Id>,
    草稿: String,
}

impl 输入框在底下 {
    fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let 草稿 = &mut self.草稿;
        let shown = Dialog::new(
            "输入框在底下",
            "输入框在底下",
            Footer::new(Button::new("关上", 按的::关上)),
        )
        .show(&ctx, |ui| {
            for n in 0..300 {
                ui.label(format!("第{n}行"));
            }
            ui.text_edit_singleline(草稿).id
        });
        self.那一框 = Some(shown.inner);
    }
}

#[test]
fn 弹层里的输入框滚出视口也不被回收() {
    // ADR-0005：中文输入放在**不会被回收**的区域里——表格虚拟化会让正在组字的那一行滚出视口时
    // 控件消失。弹层的内容区每帧把整份内容都摆一遍，输入框滚到外头去也还在：焦点留在它身上
    // （控件哪一帧没摆，egui 当帧就把焦点收走）。
    let ctx = 上下文();
    let mut 层 = 输入框在底下 {
        草稿: "底下那一框里的字".to_string(),
        ..输入框在底下::default()
    };
    let 跑 = |层: &mut 输入框在底下, events: Vec<egui::Event>| {
        headless::frame(&ctx, 输入(events), |ui| 层.ui(ui))
    };
    for _ in 0..4 {
        跑(&mut 层, Vec::new());
    }
    assert_eq!(焦点在(&ctx), 层.那一框, "前提：焦点落进了那一框");

    // 滚回顶上：那一框出了视口。
    let 头一帧 = 跑(&mut 层, Vec::new());
    let 内容区 = shared::那一段画在哪儿(&头一帧, "第0行").expect("头一行画出来了");
    跑(&mut 层, 滚一下(内容区, 100_000.0));
    let mut 滚完 = 跑(&mut 层, Vec::new());
    for _ in 0..60 {
        滚完 = 跑(&mut 层, Vec::new());
    }
    assert!(看得见(&滚完, "第0行"), "前提：滚回了顶上");
    assert!(!看得见(&滚完, "底下那一框里的字"), "前提：那一框出了视口");
    assert_eq!(
        焦点在(&ctx),
        层.那一框,
        "输入框滚出视口，焦点跟着没了——控件被回收了"
    );
}

// ——— 页脚 ———

/// 这一帧画出来的那些**填着这个颜色的框**。
fn 填着这个颜色的框(output: &egui::FullOutput, color: egui::Color32) -> Vec<egui::Rect> {
    fn 收(shape: &egui::epaint::Shape, color: egui::Color32, out: &mut Vec<egui::Rect>) {
        match shape {
            egui::epaint::Shape::Rect(rect) if rect.fill == color => out.push(rect.rect),
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    收(one, color, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        收(&clipped.shape, color, &mut out);
    }
    out
}

#[test]
fn 页脚上的主按钮画成强调色_其余几颗不是() {
    // 设计稿 `.btn.pri`：往前走的那一颗（「开始扫描」「开始刮削」「落下」）是强调色底，
    // 退出那一颗不是。颜色由 `look` 从令牌取，这一条只认它落在了哪一颗上。
    let ctx = 上下文();
    let 画 = |ctx: &egui::Context| {
        headless::frame(ctx, headless::input(), |ui| {
            let ctx = ui.ctx().clone();
            let footer = Footer::new(Button::new("关上", 按的::关上))
                .button(Button::new("保存", 按的::保存).primary());
            Dialog::new("主按钮", "主按钮", footer).show(&ctx, |ui| {
                ui.label("里头");
            });
        })
    };
    // **先跑完淡入再看颜色**：egui 的弹层打开时从透明淡进来，时长是 `Style::animation_time`
    // （默认 1/12 秒），而这一层没有真的时钟——每帧往前推 1/60 秒，约五帧才淡完。淡到一半时
    // 整层每一格颜色都只有一半的不透明度，强调色的底也就不是强调色。按帧推，不等挂钟。
    for _ in 0..30 {
        画(&ctx);
    }
    let 帧 = 画(&ctx);
    let 强调色 = Tokens::builtin().color.theme(ctx.theme()).accent;
    let 强调色的框 = 填着这个颜色的框(&帧, 强调色);
    let 保存 = shared::正好那一段画在哪儿(&帧, "保存").expect("「保存」画出来了");
    let 关上 = shared::正好那一段画在哪儿(&帧, "关上").expect("「关上」画出来了");
    assert!(
        强调色的框.iter().any(|框| 框.contains(保存)),
        "主按钮「保存」底下没有强调色的底",
    );
    assert!(
        !强调色的框.iter().any(|框| 框.contains(关上)),
        "退出那一颗「关上」也画成了强调色",
    );
}

#[test]
fn 退出那一颗摆在右边的写法_退出那一颗靠右画成主按钮_其余几颗靠左是幽灵按钮_退出键照旧等于按它() {
    // 票 `gui-looks-like-the-design/27`：库体检明细弹层照稿（设计稿 `DLG.health` 的 `foot`）——「导出清单…」幽灵按钮在左、
    // 「关闭」主按钮在右（拿主意的人 2026-09-15 答，挂单 `Q958`）。那一层没有「往前走」的那一颗，关掉就是唯一的出口。
    // **弹层框架加一种写法**，已有的弹层照旧（退出那一颗靠左）。
    let ctx = 上下文();
    let mut 按下的 = None;
    let mut 画 = |ctx: &egui::Context, events: Vec<egui::Event>| {
        headless::frame(ctx, 输入(events), |ui| {
            let ctx = ui.ctx().clone();
            let footer = Footer::new(Button::new("关上", 按的::关上))
                .dismiss_on_right()
                .button(Button::new("导出", 按的::保存).ghost());
            let shown = Dialog::new("退出靠右", "退出靠右", footer).show(&ctx, |ui| {
                ui.label("里头");
            });
            if shown.pressed.is_some() {
                按下的 = shown.pressed;
            }
        })
    };
    // 先跑完淡入再看颜色（同上一条）。
    for _ in 0..30 {
        画(&ctx, Vec::new());
    }
    let 帧 = 画(&ctx, Vec::new());
    let 关上 = shared::正好那一段画在哪儿(&帧, "关上").expect("「关上」画出来了");
    let 导出 = shared::正好那一段画在哪儿(&帧, "导出").expect("「导出」画出来了");
    assert!(
        关上.x > 导出.x,
        "退出那一颗该靠右：关上在 {关上:?}，导出在 {导出:?}"
    );
    let 强调色 = Tokens::builtin().color.theme(ctx.theme()).accent;
    let 强调色的框 = 填着这个颜色的框(&帧, 强调色);
    assert!(
        强调色的框.iter().any(|框| 框.contains(关上)),
        "靠右的退出那一颗该画成主按钮（强调色底）",
    );
    assert!(
        !强调色的框.iter().any(|框| 框.contains(导出)),
        "靠左的那一颗是幽灵按钮，不填强调色",
    );

    画(&ctx, vec![按键事件(egui::Key::Escape)]);
    assert_eq!(按下的, Some(按的::关上), "退出键照旧等于按退出那一颗");
}

// ——— 宽度与内容区 ———

/// 一层给定宽度、内容区里摆着 `行数` 行字的弹层。
struct 一层 {
    width: Width,
    行数: usize,
    /// 上一帧这一层（连边框）画在哪儿。
    画在: Option<egui::Rect>,
}

impl 一层 {
    fn new(width: Width, 行数: usize) -> Self {
        Self {
            width,
            行数,
            画在: None,
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let 行数 = self.行数;
        let shown = Dialog::new(
            "量一量",
            "量一量",
            Footer::new(Button::new("关上", 按的::关上)),
        )
        .width(self.width)
        .show(&ctx, |ui| {
            for n in 0..行数 {
                ui.label(format!("第{n}行"));
            }
        });
        self.画在 = Some(shown.rect);
    }
}

/// 在 `视口` 那么大的窗口里带着这几个事件跑一帧。
fn 跑这一层(
    ctx: &egui::Context,
    层: &mut 一层,
    视口: [f32; 2],
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut input = 输入(events);
    input.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, 视口.into()));
    headless::frame(ctx, input, |ui| 层.ui(ui))
}

/// 屏上写着这一段字的那一处**看得见**没有：画在它自己那块裁剪区里头。
///
/// 滚动区外头那几行 egui 照样排版、照样交出来，只是裁掉——光看「画没画」分不出
/// 「滚到外头去了」与「整层撑高了」。
fn 看得见(output: &egui::FullOutput, 那一段: &str) -> bool {
    fn 找(shape: &egui::epaint::Shape, clip: egui::Rect, 那一段: &str) -> bool {
        match shape {
            egui::epaint::Shape::Text(text) => {
                text.galley.text().contains(那一段)
                    && clip.intersects(egui::Rect::from_min_size(text.pos, text.galley.size()))
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().any(|one| 找(one, clip, 那一段)),
            _ => false,
        }
    }
    output
        .shapes
        .iter()
        .any(|clipped| 找(&clipped.shape, clipped.clip_rect, 那一段))
}

#[test]
fn 宽度只取令牌里那四档() {
    // 期望值抄票面（`gui-looks-like-the-design/04`：520 / 620 / 720 / 840），不从令牌读——
    // 从令牌读就是拿实现去验实现。
    let 票面 = [520.0, 620.0, 720.0, 840.0];
    assert_eq!(Tokens::builtin().layout.dialog_width, 票面);
    assert_eq!(Width::ALL.len(), 票面.len(), "档数与令牌对不上");
    for (width, 应是) in Width::ALL.into_iter().zip(票面) {
        let ctx = 上下文();
        let mut 层 = 一层::new(width, 3);
        for _ in 0..3 {
            跑这一层(&ctx, &mut 层, headless::VIEWPORT, Vec::new());
        }
        let 宽 = 层.画在.expect("画过了").width();
        assert!(
            (宽 - 应是).abs() < 0.5,
            "{width:?} 那一档画成了 {宽} 点宽，令牌里是 {应是}",
        );
    }
}

#[test]
fn 窗口比那一档还窄时弹层收在窗口里() {
    // 窗口最小 720 点宽（`main.rs` 的 `with_min_inner_size`），而最宽那一档是 840。
    let 窄窗口 = [720.0, 480.0];
    let ctx = 上下文();
    let mut 层 = 一层::new(Width::Widest, 3);
    for _ in 0..3 {
        跑这一层(&ctx, &mut 层, 窄窗口, Vec::new());
    }
    let 画在 = 层.画在.expect("画过了");
    assert!(
        画在.left() >= 0.0 && 画在.right() <= 窄窗口[0],
        "弹层画出了窗口：{画在:?}",
    );
}

#[test]
fn 内容比一屏长时内容区滚动而页脚不被顶出去() {
    let ctx = 上下文();
    let 屏 = headless::VIEWPORT;
    let mut 层 = 一层::new(Width::Standard, 300);
    for _ in 0..3 {
        跑这一层(&ctx, &mut 层, 屏, Vec::new());
    }
    let 头一帧 = 跑这一层(&ctx, &mut 层, 屏, Vec::new());

    let 画在 = 层.画在.expect("画过了");
    assert!(
        画在.top() >= 0.0 && 画在.bottom() <= 屏[1],
        "三百行把弹层撑出了窗口：{画在:?}",
    );
    let 页脚 = shared::那一段画在哪儿(&头一帧, "关上").expect("页脚那颗画出来了");
    assert!(
        画在.contains(页脚) && 看得见(&头一帧, "关上"),
        "页脚被顶出去了：画在 {页脚:?}，弹层在 {画在:?}",
    );
    assert!(看得见(&头一帧, "第0行"), "内容区头一行看不见");
    assert!(
        !看得见(&头一帧, "第299行"),
        "最后一行也看得见——内容区没有滚动，是整层撑高了",
    );

    // 在内容区上滚到底：最后一行露出来，页脚还在原处。
    let 内容区 = shared::那一段画在哪儿(&头一帧, "第0行").expect("头一行画出来了");
    跑这一层(&ctx, &mut 层, 屏, 滚一下(内容区, -100_000.0));
    let mut 滚完 = 跑这一层(&ctx, &mut 层, 屏, Vec::new());
    for _ in 0..60 {
        滚完 = 跑这一层(&ctx, &mut 层, 屏, Vec::new());
    }
    assert!(看得见(&滚完, "第299行"), "滚到底了最后一行还看不见");
    assert_eq!(
        shared::那一段画在哪儿(&滚完, "关上"),
        Some(页脚),
        "滚的是内容区，页脚却跟着动了",
    );
}
