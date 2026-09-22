//! **右键菜单与全局快捷键**（票 `gui-looks-like-the-design/14`）。
//!
//! 几条断言看的都是**跑出来的结果**，不是代码长什么样：
//!
//! - 右键按在一行上，菜单摊开来是哪几项、每一项写的是哪一句话。
//! - **菜单上摆着的每一项都按得动**——按下去屏上真有事发生。摆一项按不动的，
//!   等于屏上写着一件做不到的事（`browse::menu` 那一节「一个灰项都没有」）。
//! - **菜单摊着时单键快捷键不接**：那时 `Esc` 该收的是菜单，`?` 不该再摊开一层。
//! - 切屏那六下数的是**左栏上从上往下**那个次序，不是另列的一份。
//! - **光标在输入框里时单键一个都不接**——那一栏里正打着中文。

use romcat_gui::app::{App, View};
use romcat_gui::headless;

mod shared;
use shared::{档, 正好那一段画在哪儿, 画出来的字};

/// 这几条测试自己的工作目录（同浏览屏那几条：不与演示窗口共用）。
fn 工作目录() -> std::path::PathBuf {
    shared::干净工作目录("romcat-测试-右键菜单")
}

/// 屏上那一行：主栏印的是它的**正题**（副行那条路径是从尾部截断的，认不得全名）。
const 一行: &str = "短";
/// 屏上另一行，同上。
const 另一行: &str = "另一个";

/// 摆一个停在浏览屏上的主窗口，底下垫着两行。
fn 界面() -> App {
    let mut app = shared::小库(
        &[("SFC", "短.zip", 档::命中), ("GBA", "另一个.zip", 档::命中)],
        工作目录(),
    );
    app.set_workspace_label("~/.local/share/romcat".to_owned());
    app.show_view(View::Browse);
    app
}

/// 跑一帧，交回这一帧画出来的字。
fn 跑(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) -> String {
    画出来的字(&headless::frame(ctx, shared::输入(events), |ui| {
        app.ui(ui)
    }))
}

/// 右键按一下屏上写着 `那一段` 的地方；交回**再跑几帧之后**画出来的字。
///
/// 多跑几帧才收：右键那一下由表格在这一帧认下，菜单是下一帧才摊开的
/// （`browse::Screen::settle_menu`），而浮层还要一帧才摆稳。
/// 屏上含有 `那一段` 的**最后**一处画在哪儿。
///
/// **取最后一处**：右键那一下会把侧边详情也换成这一行（设计稿 `S.sel=i; render()`），
/// 而右边那一栏在正中那张表**之前**画——按头一处找，第二次右键就按在详情栏上、不在表上。
fn 最后一处(output: &egui::FullOutput, 那一段: &str) -> Option<egui::Pos2> {
    fn 找(shape: &egui::epaint::Shape, 那一段: &str, 每一处: &mut Vec<egui::Pos2>) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                if text.galley.text().contains(那一段) {
                    每一处.push(egui::Rect::from_min_size(text.pos, text.galley.size()).center());
                }
            }
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    找(one, 那一段, 每一处);
                }
            }
            _ => {}
        }
    }
    let mut 每一处 = Vec::new();
    for clipped in &output.shapes {
        找(&clipped.shape, 那一段, &mut 每一处);
    }
    每一处.last().copied()
}

fn 右键(ctx: &egui::Context, app: &mut App, 那一段: &str) -> String {
    let 头一帧 = headless::frame(ctx, headless::input(), |ui| app.ui(ui));
    let Some(位置) = 最后一处(&头一帧, 那一段) else {
        panic!("屏上没有「{那一段}」，没处右键：\n{}", 画出来的字(&头一帧));
    };
    let 按 = |pressed: bool| egui::Event::PointerButton {
        pos: 位置,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    跑(ctx, app, vec![egui::Event::PointerMoved(位置), 按(true)]);
    跑(ctx, app, vec![按(false)]);
    let mut 画的 = String::new();
    for _ in 0..4 {
        画的 = 跑(ctx, app, Vec::new());
    }
    画的
}

/// 按一下菜单上写着 `那一项` 的地方；交回再跑几帧之后画出来的字。
fn 按菜单(ctx: &egui::Context, app: &mut App, 那一项: &str) -> String {
    let 这一帧 = headless::frame(ctx, headless::input(), |ui| app.ui(ui));
    let Some(位置) = 正好那一段画在哪儿(&这一帧, 那一项) else {
        panic!("菜单上没有「{那一项}」：\n{}", 画出来的字(&这一帧));
    };
    let 按 = |pressed: bool| egui::Event::PointerButton {
        pos: 位置,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    跑(ctx, app, vec![egui::Event::PointerMoved(位置), 按(true)]);
    跑(ctx, app, vec![按(false)]);
    let mut 画的 = String::new();
    for _ in 0..4 {
        画的 = 跑(ctx, app, Vec::new());
    }
    画的
}

/// 按一下某个键。
fn 键(key: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
    vec![egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }]
}

/// 这台机器上的修饰键（macOS 上是 ⌘，别处是 Ctrl）——与屏上那张表写的是同一件事。
fn 修饰键() -> egui::Modifiers {
    egui::Modifiers::COMMAND
}

// ——— 菜单摆的是哪几项 ———

/// 右键一行，菜单摊开来是设计稿那几项；**今天做得到的那八项一项不少**。
#[test]
fn 右键一行摊开菜单() {
    let ctx = headless::context();
    let mut app = 界面();
    let 画的 = 右键(&ctx, &mut app, 一行);
    for 一项 in [
        "打开详情",
        "编辑元数据",
        "勾选",
        "收藏",
        "合并…",
        "刮削此作品",
        "在文件系统中打开",
        "复制名称",
    ] {
        assert!(画的.contains(一项), "菜单上该有「{一项}」：\n{画的}");
    }
    // 右边那一列提示照设计稿写，取的是全仓那一份（`keys::打开键` 那几个常量）。
    for 提示 in [
        romcat_gui::keys::打开键,
        romcat_gui::keys::编辑键,
        romcat_gui::keys::勾选键,
        romcat_gui::keys::收藏键,
    ] {
        assert!(画的.contains(提示), "菜单上该有提示「{提示}」：\n{画的}");
    }
}

/// **那两项做得到了，于是摆上来**（票 `gui-looks-like-the-design/23`，挂单 `Q1140` 收口）。
///
/// 这一条从前叫「做不到的那两项一项都不摆」：票 14 定的原则是**接一个才列一个**
/// ——摆一项按下去没有下一步的，等于屏上写着一件做不到的事。当时「加入合集…」
/// 等票 `13`、「加入子库…」等票 `23`，两层弹层都还不存在。
///
/// **两票都落地了，前提满足，所以翻成正面断言**：两项都在、都按得动、
/// 各自走到该走的那一层。
///
/// ⚠️ **断的是「按下去真有下一步」，不是「屏上有这几个字」**：
/// 那两句话在表格上方那一条的按钮上也写着，光看屏上有没有等于没断
/// （票 13 那一趟正是这么错过一次的）。所以这儿按完之后断的是**那一层真的摊开了**。
#[test]
fn 那两项做得到了就摆上来_按下去各自摊开该摊的那一层() {
    let ctx = headless::context();
    let mut app = 界面();

    // 一、**两项都在菜单上**——拿「菜单摊着」与「按 Esc 收掉」两帧相减，
    // 差出来的才是菜单自己那几项（屏上别处也写着同样的字）。
    let 摊着 = 右键(&ctx, &mut app, 一行);
    跑(&ctx, &mut app, 键(egui::Key::Escape, egui::Modifiers::NONE));
    let mut 收掉 = String::new();
    for _ in 0..4 {
        收掉 = 跑(&ctx, &mut app, Vec::new());
    }
    let 数几处 = |屏上: &str, 那一句: &str| 屏上.matches(那一句).count();
    // 先拿一项确实在菜单上的验这把尺——相减减不出东西的话，底下那两条会永远绿。
    assert_eq!(
        数几处(&摊着, "复制名称") - 数几处(&收掉, "复制名称"),
        1,
        "相减没减出菜单自己那一项，这把尺是坏的：\n摊着：\n{摊着}\n收掉：\n{收掉}"
    );
    for 该有的 in ["加入合集…", "加入子库…"] {
        assert_eq!(
            数几处(&摊着, 该有的) - 数几处(&收掉, 该有的),
            1,
            "菜单上该有「{该有的}」了（`Q1140` 收口）：\n{摊着}"
        );
    }

    // 二、**「加入合集…」按下去那一层真摊开**。
    右键(&ctx, &mut app, 一行);
    let 屏上 = 按菜单(&ctx, &mut app, "加入合集…");
    // **只断那一档**：库里一个合集都没有，所以摊开的一定是「新建合集」那一版。
    // `A || B` 会把「摊开的是另一版」也放过去，等于松了一档。
    assert!(
        屏上.contains("新建合集"),
        "按了「加入合集…」，那一层没摊开（库里没有合集，该是「新建合集」那一版）：\n{屏上}"
    );
    // 收掉，别挡住下一步。
    跑(&ctx, &mut app, 键(egui::Key::Escape, egui::Modifiers::NONE));
    跑(&ctx, &mut app, Vec::new());

    // 三、**「加入子库…」按下去那一层真摊开**。库里一台子库都没有，
    // 所以摊开的是照稿那一版空态——**那也是「有下一步」**：它指着「新建子库」。
    右键(&ctx, &mut app, 一行);
    let 屏上 = 按菜单(&ctx, &mut app, "加入子库…");
    // 同上只断那一档：库里一台子库都没有，摊开的一定是照稿那一版空态。
    assert!(
        屏上.contains("还没有子库"),
        "按了「加入子库…」，那一层没摊开（库里没有子库，该是空态那一版）：\n{屏上}"
    );
}

/// 勾中了那一行，菜单上那一项写的是「取消勾选」——**问的是那一份选中集本身**。
#[test]
fn 勾中了就写取消勾选() {
    let ctx = headless::context();
    let mut app = 界面();
    // 先右键一下、按「勾选」，再右键一下看它改口没有。
    右键(&ctx, &mut app, 一行);
    按菜单(&ctx, &mut app, "勾选");
    let 画的 = 右键(&ctx, &mut app, 一行);
    assert!(
        画的.contains("取消勾选"),
        "勾中之后那一项该写「取消勾选」：\n{画的}"
    );
}

/// 勾了两行，在其中一行上右键，「合并」那一项写的是**带上几个作品**（设计稿 `many>=2`）。
#[test]
fn 勾了两行合并那一项数得出几个() {
    let ctx = headless::context();
    let mut app = 界面();
    右键(&ctx, &mut app, 一行);
    按菜单(&ctx, &mut app, "勾选");
    右键(&ctx, &mut app, 另一行);
    按菜单(&ctx, &mut app, "勾选");
    let 画的 = 右键(&ctx, &mut app, 一行);
    assert!(
        画的.contains("合并勾选的 2 个作品…"),
        "勾了两行时那一项该数得出 2 个：\n{画的}"
    );
}

/// **卡片墙上右键也摊得开**：验收那一条写的是「行**与卡片**上的右键菜单」，两路交出来的
/// 是同一份开单，菜单也只有这一层。
#[test]
fn 卡片墙上右键也摊得开() {
    let ctx = headless::context();
    let mut app = 界面();
    app.browse_and_site().0.show_cards();
    跑(&ctx, &mut app, Vec::new());
    // 按在**卡面的字**上：卡面上的字不许接住点击，那一下该归整张卡
    // （表格那一路早就这么干了）。按封面那一块是按不着这一条的。
    let 画的 = 右键(&ctx, &mut app, 一行);
    for 一项 in ["打开详情", "刮削此作品", "复制名称"] {
        assert!(画的.contains(一项), "卡片上右键也该有「{一项}」：\n{画的}");
    }
}

// ——— 菜单上按下去真有事发生 ———

/// 「打开详情」打开的是作品详情页——那一屏顶上写着「← 返回浏览」。
#[test]
fn 打开详情进得了作品详情页() {
    let ctx = headless::context();
    let mut app = 界面();
    右键(&ctx, &mut app, 一行);
    let 画的 = 按菜单(&ctx, &mut app, "打开详情");
    assert!(画的.contains("返回浏览"), "该进作品详情页了：\n{画的}");
}

/// 「复制名称」把屏上那个名字交给系统剪贴板，并留一句回执。
#[test]
fn 复制名称交给剪贴板并留一句回执() {
    let ctx = headless::context();
    let mut app = 界面();
    右键(&ctx, &mut app, 一行);
    // 交给系统的那一份走 `OutputCommand::CopyText`，从这一帧的产出里读得到。
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let Some(位置) = 正好那一段画在哪儿(&这一帧, "复制名称") else {
        panic!("菜单上没有「复制名称」：\n{}", 画出来的字(&这一帧));
    };
    let 按 = |pressed: bool| egui::Event::PointerButton {
        pos: 位置,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    跑(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(位置), 按(true)],
    );
    let 松开 = headless::frame(&ctx, shared::输入(vec![按(false)]), |ui| app.ui(ui));
    let 交出去的: Vec<String> = 松开
        .platform_output
        .commands
        .iter()
        .filter_map(|cmd| match cmd {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        交出去的.len(),
        1,
        "该往剪贴板交一份、且只交一份：{交出去的:?}"
    );
    let mut 画的 = String::new();
    for _ in 0..4 {
        画的 = 跑(&ctx, &mut app, Vec::new());
    }
    assert!(画的.contains("已复制："), "复制完该留一句回执：\n{画的}");
}

/// 「刮削此作品」摊开的是屏头那颗「刮削…」同一层面板，范围换成这一个作品。
#[test]
fn 刮削此作品摊开那层面板() {
    let ctx = headless::context();
    let mut app = 界面();
    右键(&ctx, &mut app, 一行);
    按菜单(&ctx, &mut app, "刮削此作品");
    let (browse, _) = app.browse_and_site();
    assert!(browse.scrape().is_open(), "该摊开刮削那一层了");
    assert_eq!(
        browse.scrape().scope_shown(),
        1,
        "范围该只有这一个作品底下那一个变体"
    );
}

/// 「在文件系统中打开」**点了要说话**（ADR-0005）：这份小库的根在盘上不存在，
/// 那就照实说一句，不许默默不动。
#[test]
fn 在文件系统中打开找不着也要说一句() {
    let ctx = headless::context();
    let mut app = 界面();
    右键(&ctx, &mut app, 一行);
    let 画的 = 按菜单(&ctx, &mut app, "在文件系统中打开");
    assert!(
        画的.contains("盘上找不到"),
        "找不着也得说一句，不许默默不动：\n{画的}"
    );
}

// ——— Esc 一层一层退 ———

/// `Esc` 收起菜单。**收的只是菜单那一层**：底下那一屏照旧摆着。
#[test]
fn esc收起菜单() {
    let ctx = headless::context();
    let mut app = 界面();
    let 摊开时 = 右键(&ctx, &mut app, 一行);
    assert!(摊开时.contains("刮削此作品"), "菜单该摊开了：\n{摊开时}");
    跑(&ctx, &mut app, 键(egui::Key::Escape, egui::Modifiers::NONE));
    let mut 画的 = String::new();
    for _ in 0..3 {
        画的 = 跑(&ctx, &mut app, Vec::new());
    }
    assert!(!画的.contains("刮削此作品"), "菜单该收起来了：\n{画的}");
    assert!(画的.contains(一行), "底下那一屏该照旧摆着：\n{画的}");
}

/// **菜单摊着时单键快捷键一个都不接**：`?` 不该在菜单上头再摊开一层快捷键表。
#[test]
fn 菜单摊着时单键快捷键不接() {
    let ctx = headless::context();
    let mut app = 界面();
    右键(&ctx, &mut app, 一行);
    跑(
        &ctx,
        &mut app,
        键(egui::Key::Questionmark, egui::Modifiers::NONE),
    );
    let mut 画的 = String::new();
    for _ in 0..3 {
        画的 = 跑(&ctx, &mut app, Vec::new());
    }
    assert!(
        !画的.contains("查看快捷键"),
        "菜单摊着时不该再摊开快捷键表：\n{画的}"
    );
}

// ——— 全局快捷键 ———

/// `?` 摊开快捷键表；`Esc` 收起来。
#[test]
fn 问号摊开快捷键表() {
    let ctx = headless::context();
    let mut app = 界面();
    跑(
        &ctx,
        &mut app,
        键(egui::Key::Questionmark, egui::Modifiers::NONE),
    );
    let mut 画的 = String::new();
    for _ in 0..3 {
        画的 = 跑(&ctx, &mut app, Vec::new());
    }
    // 摆的是全仓那唯一一份表：三组的组名都在。
    for 一组 in ["全局", "浏览", "待确认 · 逐条"] {
        assert!(
            画的.contains(一组),
            "快捷键表上该有「{一组}」那一组：\n{画的}"
        );
    }
    assert!(画的.contains("查看快捷键"), "该摊开快捷键表了：\n{画的}");
    跑(&ctx, &mut app, 键(egui::Key::Escape, egui::Modifiers::NONE));
    for _ in 0..3 {
        画的 = 跑(&ctx, &mut app, Vec::new());
    }
    assert!(!画的.contains("查看快捷键"), "该收起来了：\n{画的}");
}

/// **真键盘上 `?` 是 Shift 加一下**：那一下带着 `shift`，照样摊得开
/// （egui 的 `matches_logically`：`Modifiers::NONE` 不要求别的修饰键是空的）。
#[test]
fn 问号带着shift照样摊得开() {
    let ctx = headless::context();
    let mut app = 界面();
    跑(
        &ctx,
        &mut app,
        键(egui::Key::Questionmark, egui::Modifiers::SHIFT),
    );
    let mut 画的 = String::new();
    for _ in 0..3 {
        画的 = 跑(&ctx, &mut app, Vec::new());
    }
    assert!(画的.contains("查看快捷键"), "该摊开快捷键表了：\n{画的}");
}

/// **带着修饰键按下的不算单键**（设计稿 `mod&&k!=='a'` 那一支）：macOS 上 `Ctrl+F`
/// 是系统的「光标右移」，不该被当成收藏。
#[test]
fn 带着修饰键的单键不接() {
    let ctx = headless::context();
    let mut app = 界面();
    跑(&ctx, &mut app, Vec::new());
    跑(
        &ctx,
        &mut app,
        键(egui::Key::ArrowDown, egui::Modifiers::NONE),
    );
    跑(&ctx, &mut app, Vec::new());
    跑(&ctx, &mut app, 键(egui::Key::Space, egui::Modifiers::ALT));
    跑(&ctx, &mut app, Vec::new());
    assert_eq!(
        app.browse_and_site().0.picked().count(2),
        0,
        "带着 Alt 的空格不该勾中任何东西"
    );
}

/// **切屏那六下数的是左栏上从上往下那个次序**，不是另列的一份。
#[test]
fn 切屏六下照左栏那个次序() {
    let ctx = headless::context();
    let mut app = 界面();
    const 数字: [egui::Key; 6] = [
        egui::Key::Num1,
        egui::Key::Num2,
        egui::Key::Num3,
        egui::Key::Num4,
        egui::Key::Num5,
        egui::Key::Num6,
    ];
    for (at, 该到的) in romcat_gui::rail::order().into_iter().enumerate() {
        跑(&ctx, &mut app, 键(数字[at], 修饰键()));
        跑(&ctx, &mut app, Vec::new());
        assert_eq!(
            app.view(),
            该到的,
            "第 {} 下该走到左栏上从上往下第 {} 个",
            at + 1,
            at + 1
        );
    }
}

/// `⌘/Ctrl+F`：换到浏览屏、把光标放进搜索框。**进得去就打得出中文**——
/// 打进去的字落在搜索词上，而不是被当成单键快捷键吃掉。
#[test]
fn 搜索那一下把光标放进搜索框() {
    let ctx = headless::context();
    let mut app = 界面();
    app.show_view(View::Tasks);
    跑(&ctx, &mut app, 键(egui::Key::F, 修饰键()));
    // 换屏、展开筛选栏、把光标放进那一框，各要一帧。
    for _ in 0..4 {
        跑(&ctx, &mut app, Vec::new());
    }
    assert_eq!(app.view(), View::Browse, "该换到浏览屏了");
    跑(&ctx, &mut app, vec![egui::Event::Text("幻想".to_owned())]);
    跑(&ctx, &mut app, Vec::new());
    assert_eq!(
        app.browse_and_site().0.query().search,
        "幻想",
        "打进去的字该落在搜索词上"
    );
}

/// **光标在输入框里时单键一个都不接**：搜索框里打一个 `F`，收藏那一下不许跟着发生，
/// 那个字得老老实实落进框里（中文输入不被抢，ADR-0005）。
#[test]
fn 光标在输入框里时单键不接() {
    let ctx = headless::context();
    let mut app = 界面();
    跑(&ctx, &mut app, 键(egui::Key::F, 修饰键()));
    for _ in 0..4 {
        跑(&ctx, &mut app, Vec::new());
    }
    跑(&ctx, &mut app, 键(egui::Key::F, egui::Modifiers::NONE));
    跑(&ctx, &mut app, vec![egui::Event::Text("F".to_owned())]);
    let mut 画的 = String::new();
    for _ in 0..3 {
        画的 = 跑(&ctx, &mut app, Vec::new());
    }
    assert_eq!(
        app.browse_and_site().0.query().search,
        "F",
        "那个字该落进搜索框里"
    );
    assert!(
        !画的.contains("放进"),
        "光标在框里时不该排一趟收藏：\n{画的}"
    );
}

// ——— 浏览屏那几下 ———

/// `↓` 挪高亮、`空格` 勾中它：**高亮与勾选是两件事**，挪一行不改作用范围。
#[test]
fn 上下键挪高亮_空格勾中它() {
    let ctx = headless::context();
    let mut app = 界面();
    跑(&ctx, &mut app, Vec::new());
    跑(
        &ctx,
        &mut app,
        键(egui::Key::ArrowDown, egui::Modifiers::NONE),
    );
    跑(&ctx, &mut app, Vec::new());
    assert_eq!(
        app.browse_and_site().0.picked().count(2),
        0,
        "挪一行不该改作用范围"
    );
    跑(&ctx, &mut app, 键(egui::Key::Space, egui::Modifiers::NONE));
    跑(&ctx, &mut app, Vec::new());
    assert_eq!(
        app.browse_and_site().0.picked().count(2),
        1,
        "空格该勾中高亮那一行"
    );
}

/// `⌘/Ctrl+A` 全选**筛出来的那一批**：选中集就是这个筛选本身（ADR-0016）。
#[test]
fn 全选那一下选的是整个筛选() {
    let ctx = headless::context();
    let mut app = 界面();
    跑(&ctx, &mut app, Vec::new());
    跑(&ctx, &mut app, 键(egui::Key::A, 修饰键()));
    跑(&ctx, &mut app, Vec::new());
    let (browse, _) = app.browse_and_site();
    assert!(browse.picked().is_all(), "该是「整个筛选」那一档");
    assert_eq!(browse.picked().count(2), 2, "两行都该算在里头");
}

/// `Enter` 打开高亮那一行的作品详情页。
#[test]
fn 回车打开作品详情页() {
    let ctx = headless::context();
    let mut app = 界面();
    跑(&ctx, &mut app, Vec::new());
    跑(
        &ctx,
        &mut app,
        键(egui::Key::ArrowDown, egui::Modifiers::NONE),
    );
    跑(&ctx, &mut app, Vec::new());
    跑(&ctx, &mut app, 键(egui::Key::Enter, egui::Modifiers::NONE));
    let mut 画的 = String::new();
    for _ in 0..3 {
        画的 = 跑(&ctx, &mut app, Vec::new());
    }
    assert!(画的.contains("返回浏览"), "该进作品详情页了：\n{画的}");
}

/// **摆着卡片墙时，跟高亮走的那几下一下都不接**（挂单 `Q1142`）。
///
/// 高亮是**表格**背后那扇窗的行序号，而卡片墙背后是另一扇窗、另一份查询——照着按下去，
/// 勾中的会是人看不见的另一行，比什么都不发生坏得多。卡片墙自己那条路照旧走得通：
/// Tab 走到一张卡，`Enter` / `空格` 由那张卡自己接。
#[test]
fn 摆着卡片墙时跟高亮走的那几下不接() {
    let ctx = headless::context();
    let mut app = 界面();
    // 先在表格上挪一下高亮，好让「不接」不是因为压根没有高亮。
    跑(&ctx, &mut app, Vec::new());
    跑(
        &ctx,
        &mut app,
        键(egui::Key::ArrowDown, egui::Modifiers::NONE),
    );
    跑(&ctx, &mut app, Vec::new());
    app.browse_and_site().0.show_cards();
    跑(&ctx, &mut app, Vec::new());
    跑(&ctx, &mut app, 键(egui::Key::Space, egui::Modifiers::NONE));
    跑(&ctx, &mut app, Vec::new());
    assert_eq!(
        app.browse_and_site().0.picked().count(2),
        0,
        "卡片墙上那一下不该勾中表格那份高亮指着的行"
    );
    // **全选不在那道门里头**：它选的是当前这个筛选，与光标落在哪一行无关。
    跑(&ctx, &mut app, 键(egui::Key::A, 修饰键()));
    跑(&ctx, &mut app, Vec::new());
    assert!(
        app.browse_and_site().0.picked().is_all(),
        "卡片墙上 ⌘/Ctrl+A 照样全选"
    );
}

/// **切屏与搜索那几下先关掉作品详情页**（设计稿 `S.wd=false`）：它盖住整块屏，
/// 留着的话换回浏览屏看见的还是它。
#[test]
fn 切屏与搜索先关掉作品详情页() {
    let ctx = headless::context();
    let mut app = 界面();
    跑(&ctx, &mut app, Vec::new());
    跑(
        &ctx,
        &mut app,
        键(egui::Key::ArrowDown, egui::Modifiers::NONE),
    );
    跑(&ctx, &mut app, Vec::new());
    跑(&ctx, &mut app, 键(egui::Key::Enter, egui::Modifiers::NONE));
    for _ in 0..3 {
        跑(&ctx, &mut app, Vec::new());
    }
    assert!(app.browse_and_site().0.page().is_some(), "该开着详情页");
    跑(&ctx, &mut app, 键(egui::Key::F, 修饰键()));
    for _ in 0..4 {
        跑(&ctx, &mut app, Vec::new());
    }
    assert!(
        app.browse_and_site().0.page().is_none(),
        "⌘/Ctrl+F 该先把详情页关掉"
    );
}

/// **作品详情页开着时浏览屏那几下一个都不接**（设计稿 `S.wd`）：那一屏盖住整块屏，
/// 那时 `空格` 说的是另一件事。
#[test]
fn 详情页开着时浏览屏那几下不接() {
    let ctx = headless::context();
    let mut app = 界面();
    跑(&ctx, &mut app, Vec::new());
    跑(
        &ctx,
        &mut app,
        键(egui::Key::ArrowDown, egui::Modifiers::NONE),
    );
    跑(&ctx, &mut app, Vec::new());
    跑(&ctx, &mut app, 键(egui::Key::Enter, egui::Modifiers::NONE));
    for _ in 0..3 {
        跑(&ctx, &mut app, Vec::new());
    }
    跑(&ctx, &mut app, 键(egui::Key::Space, egui::Modifiers::NONE));
    跑(&ctx, &mut app, Vec::new());
    assert_eq!(
        app.browse_and_site().0.picked().count(2),
        0,
        "详情页开着时空格不该勾中任何东西"
    );
}
