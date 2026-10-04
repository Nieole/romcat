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

use romcat_core::catalog::browse::WorkAnchor;
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
    按最后一处(ctx, app, 那一段, egui::PointerButton::Secondary)
}

/// 拿 `哪个键` 按一下屏上含有 `那一段` 的**最后一处**；交回再跑几帧之后画出来的字（同 [`右键`]）。
fn 按最后一处(
    ctx: &egui::Context,
    app: &mut App,
    那一段: &str,
    哪个键: egui::PointerButton,
) -> String {
    let 头一帧 = headless::frame(ctx, headless::input(), |ui| app.ui(ui));
    let Some(位置) = 最后一处(&头一帧, 那一段) else {
        panic!("屏上没有「{那一段}」，没处按：\n{}", 画出来的字(&头一帧));
    };
    let 按 = |pressed: bool| egui::Event::PointerButton {
        pos: 位置,
        button: 哪个键,
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

// ——— 高亮存作品身份，表格与卡片墙共用（挂单 `Q1142`） ———

/// 摆一个停在浏览屏上的主窗口，底下垫着**四行**：「第三部」「下一部」要数得出来。
///
/// 工作目录各条测试各用一块（`名字`）：切卡片墙那一下会把「浏览视图 = 卡片」记进工作目录的版式偏好，
/// 与别的测试共用一块的话，另一条测试打开的窗口会落在卡片墙上。
fn 四行的界面(名字: &str) -> App {
    let mut app = shared::小库(
        &[
            ("SFC", "甲.zip", 档::命中),
            ("SFC", "乙.zip", 档::命中),
            ("GBA", "丙.zip", 档::命中),
            ("GBA", "丁.zip", 档::命中),
        ],
        shared::干净工作目录(&format!("romcat-测试-高亮-{名字}")),
    );
    app.set_workspace_label("~/.local/share/romcat".to_owned());
    app.show_view(View::Browse);
    app
}

/// 眼下这个筛选下**中立库排出来的**那几行依次是谁：卡片墙与表格摆的都是这个次序（同一份查询）。
///
/// 拿核心库那一处当准星，不拿屏上画的字去猜次序：同一张卡的标题画两遍（封面里一遍、卡面下半截一遍），
/// 左右两栏里还有别的字。
fn 次序(app: &mut App) -> Vec<WorkAnchor> {
    let query = app.browse().query().clone();
    let (_, site) = app.browse_and_site();
    site.catalog
        .work_page(&query, 0, 64)
        .expect("读得出主列表")
        .into_iter()
        .map(|row| row.anchor)
        .collect()
}

/// 那一行在卡上叫什么：夹具里每一行都认不出作品，卡上印的是从文件名剥出来的正题（`甲.zip` → 「甲」）。
fn 卡上的名字(anchor: &WorkAnchor) -> String {
    let WorkAnchor::Loose(key) = anchor else {
        panic!("夹具里每一行都认不出作品：{anchor:?}");
    };
    std::path::Path::new(key)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .expect("键里有文件名")
        .to_owned()
}

/// 按一下这个键，再跑一帧让它落稳。
fn 按(ctx: &egui::Context, app: &mut App, key: egui::Key) {
    跑(ctx, app, 键(key, egui::Modifiers::NONE));
    跑(ctx, app, Vec::new());
}

/// **卡片墙上 `↑` `↓` 挪得动高亮**，照墙上那个次序一张一张走，侧边详情跟着它（设计稿 `keydown`
/// 那一段不分表格与卡片：`S.sel` 存的是作品号，`↑` `↓` 走同一份 `browseList`）。
///
/// **`⌘/Ctrl+A` 照样全选**：它选的是当前这个筛选（ADR-0016），与高亮落在哪一张无关。
#[test]
fn 卡片墙上上下键一张一张挪高亮_侧边详情跟着() {
    let ctx = headless::context();
    let mut app = 四行的界面("卡片墙上下键");
    let 次序 = 次序(&mut app);
    assert_eq!(次序.len(), 4, "夹具该摆出四行");
    app.browse_and_site().0.show_cards();
    跑(&ctx, &mut app, Vec::new());
    assert_eq!(app.browse().highlighted(), None, "一进来什么都没高亮");

    按(&ctx, &mut app, egui::Key::ArrowDown);
    assert_eq!(
        app.browse().highlighted(),
        Some(&次序[0]),
        "卡片墙上头一下 ↓ 该落在头一张卡上"
    );
    按(&ctx, &mut app, egui::Key::ArrowDown);
    assert_eq!(
        app.browse().highlighted(),
        Some(&次序[1]),
        "再按一下 ↓ 该挪到下一张"
    );
    assert_eq!(
        app.browse().work().map(|work| &work.anchor),
        Some(&次序[1]),
        "侧边详情该跟着高亮走"
    );
    按(&ctx, &mut app, egui::Key::ArrowUp);
    assert_eq!(app.browse().highlighted(), Some(&次序[0]), "↑ 该挪回上一张");

    跑(&ctx, &mut app, 键(egui::Key::A, 修饰键()));
    跑(&ctx, &mut app, Vec::new());
    assert!(app.browse().picked().is_all(), "卡片墙上 ⌘/Ctrl+A 照样全选");
}

/// 左键点一下屏上写着 `那一段` 的**最后一处**（卡片墙上就是卡面下半截那一行标题）；再跑几帧落稳。
fn 点卡(ctx: &egui::Context, app: &mut App, 那一段: &str) {
    按最后一处(ctx, app, 那一段, egui::PointerButton::Primary);
}

/// 这一帧上**描着强调色、正好是一张封面那个形状**（宽高比照令牌 `card-cover-ratio`）的那几个框——封面外头那一圈，
/// 连着它描边的宽与描法。
///
/// 只认封面那个形状：屏头那颗主按钮、侧边详情里选中那张变体卡、勾上的那枚选择框描的也是强调色。
fn 封面外头那一圈(
    ctx: &egui::Context,
    output: &egui::FullOutput,
) -> Vec<(egui::Rect, f32, egui::StrokeKind)> {
    fn 收(
        shape: &egui::epaint::Shape,
        color: egui::Color32,
        宽高比: f32,
        out: &mut Vec<(egui::Rect, f32, egui::StrokeKind)>,
    ) {
        match shape {
            egui::epaint::Shape::Rect(rect)
                if rect.stroke.width > 0.0
                    && rect.stroke.color == color
                    && (rect.rect.aspect_ratio() - 宽高比).abs() < 0.01 =>
            {
                out.push((rect.rect, rect.stroke.width, rect.stroke_kind));
            }
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().for_each(|one| 收(one, color, 宽高比, out));
            }
            _ => {}
        }
    }
    let 强调 = ctx.style_of(ctx.theme()).visuals.selection.stroke.color;
    let 宽高比 = romcat_gui::tokens::Tokens::builtin()
        .layout
        .card_cover_ratio;
    let mut out = Vec::new();
    for clipped in &output.shapes {
        收(&clipped.shape, 强调, 宽高比, &mut out);
    }
    out
}

/// 这一帧上**填着这个颜色、正好一枚选择框那么大**的那几个框（封面上那枚选择框，设计稿 `.cv-ck`）。
fn 选择框(output: &egui::FullOutput, 底: egui::Color32) -> Vec<egui::Rect> {
    let 边长 = romcat_gui::tokens::Tokens::builtin().layout.cover_check;
    shared::填着这个颜色的框(output, 底)
        .into_iter()
        .filter(|rect| (rect.width() - 边长).abs() < 0.5 && (rect.height() - 边长).abs() < 0.5)
        .collect()
}

/// **勾着的卡封面不描圈，只亮左上角那枚选择框；墙上有一张勾着时每张都露出选择框**（设计稿 `.cv-ck`、
/// `.cv-ck.on`、`.cgrid.picking`；拿主意的人 2026-10-04 裁照稿，收挂单 `Q1418`）。
#[test]
fn 勾着的卡不描圈只亮选择框_墙上有一张勾着时每张都露出选择框() {
    let ctx = headless::context();
    let mut app = 四行的界面("勾着的卡");
    let 次序 = 次序(&mut app);
    app.browse_and_site().0.show_cards();
    跑(&ctx, &mut app, Vec::new());
    // 头一张勾上，再把高亮挪到第二张：头一张只剩「勾着」这一态。
    按(&ctx, &mut app, egui::Key::ArrowDown);
    按(&ctx, &mut app, egui::Key::Space);
    按(&ctx, &mut app, egui::Key::ArrowDown);
    assert!(app.browse().picked().contains(&次序[0]), "头一张该勾着");
    assert_eq!(app.browse().highlighted(), Some(&次序[1]), "高亮该在第二张");

    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 圈 = 封面外头那一圈(&ctx, &这一帧);
    let 第二张 = 卡上的名字(&次序[1]);
    let 第二张标题 = 最后一处正好是(&这一帧, &第二张).expect("墙上有第二张卡");
    assert_eq!(
        圈.len(),
        1,
        "只有高亮那一张描强调色那一圈，勾着的那一张不描：{圈:?}"
    );
    assert!(
        圈[0].0.x_range().contains(第二张标题.center().x),
        "那一圈该在高亮的第二张上：{圈:?}"
    );

    let 令牌 = romcat_gui::tokens::Tokens::builtin();
    let 强调 = ctx.style_of(ctx.theme()).visuals.selection.stroke.color;
    let 勾着的 = 选择框(&这一帧, 强调);
    assert_eq!(
        勾着的.len(),
        1,
        "勾着的那一张左上角该亮着一枚强调色底的选择框：{勾着的:?}"
    );
    let 头一张标题 = 最后一处正好是(&这一帧, &卡上的名字(&次序[0])).expect("墙上有头一张卡");
    assert!(
        勾着的[0]
            .x_range()
            .contains(头一张标题.left() + 令牌.layout.cover_check_inset),
        "亮着的那枚该在头一张卡上：{勾着的:?}，头一张标题 {头一张标题:?}"
    );
    // 里头那道白勾。
    let 白勾 = 这一帧.shapes.iter().any(|clipped| {
        matches!(&clipped.shape, egui::epaint::Shape::Path(path)
            if matches!(path.stroke.color, egui::epaint::ColorMode::Solid(c) if c == 令牌.color.cover_check.tick)
                && path.points.iter().all(|p| 勾着的[0].contains(*p)))
    });
    assert!(白勾, "勾着的那枚选择框里该有一道白勾");
    // 墙上有一张勾着：其余三张都露出没勾的那一枚（半透明深底）。
    let 没勾的 = 选择框(&这一帧, 令牌.color.cover_check.shade);
    assert_eq!(
        没勾的.len(),
        3,
        "墙上有一张勾着时，其余每张都该露出选择框：{没勾的:?}"
    );
}

/// **卡面上那两枚标照稿**（设计稿 `.cv-plat`、`.cv-zh`，拿主意的人 2026-10-04 裁，收挂单 `Q1237`）：
/// 中文标是深色半透明底、白字（两套主题共用），右沿与平台标对齐、都离封面右沿 8 点；平台标离顶 8 点、中文标 32 点，一样高 20 点。
#[test]
fn 卡面上的中文标照稿_深色半透明底白字_与平台标右沿对齐() {
    use romcat_core::catalog::identify::Identification;
    use romcat_core::catalog::{Confidence, State};

    let ctx = headless::context();
    let mut app = 四行的界面("中文标");
    let 次序 = 次序(&mut app);
    // 给头一张记上汉化记号：那一条已采纳的候选说它是汉化版。
    let WorkAnchor::Loose(key) = 次序[0].clone() else {
        panic!("夹具里每一行都认不出作品");
    };
    let 名字 = std::path::Path::new(&key)
        .file_name()
        .and_then(|name| name.to_str())
        .expect("键里有文件名")
        .to_owned();
    let 平台 = key.split('/').nth(1).expect("键里有平台").to_owned();
    let 变体 = shared::变体(&平台, &名字);
    let mut 候选 = shared::候选(&变体, true, Confidence::High);
    候选.chinese = Some(romcat_core::dat::chinese::ChineseMark::FanTranslated);
    app.browse_and_site()
        .1
        .catalog
        .write_identifications(&[Identification {
            variant_key: 变体.key.clone(),
            platform: None,
            standalone: None,
            edition: None,
            state: State::Matched,
            reason: None,
            units: 1,
            nkit: 0,
            read_bytes: 0,
            work_id: None,
            release_id: None,
            candidates: vec![候选],
        }])
        .expect("写得进结论");
    app.browse_and_site().0.show_cards();
    跑(&ctx, &mut app, Vec::new());
    // 高亮头一张：高亮那一圈正好就是它的封面那一框，拿来当尺子。
    按(&ctx, &mut app, egui::Key::ArrowDown);
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let [(封面, _)] = 高亮那一圈(&ctx, &这一帧)[..] else {
        panic!("头一张该描着高亮那一圈");
    };

    let 令牌 = romcat_gui::tokens::Tokens::builtin();
    let 版式 = &令牌.layout;
    let 中文标们 = shared::填着这个颜色的框(&这一帧, 令牌.color.cover_tag.shade);
    let [中文标] = 中文标们[..] else {
        panic!("墙上该正好一枚深色半透明底的中文标：{中文标们:?}");
    };
    let 平台标们 = shared::填着这个颜色的框(&这一帧, 令牌.color.platform.of(&平台));
    let 平台标 = 平台标们
        .iter()
        .copied()
        // 字卡底子那一大块也填着平台色（照平台色调过的那几块不算），只认封面上那一小枚。
        .find(|rect| 封面.contains_rect(*rect) && rect.height() < 封面.height() / 4.0)
        .unwrap_or_else(|| panic!("头一张封面上该有平台标：{平台标们:?}"));
    let 差不多 = |甲: f32, 乙: f32| (甲 - 乙).abs() < 0.5;
    let [平台标顶, 中文标顶] = 版式.cover_tag_top;
    assert!(
        差不多(中文标.right(), 封面.right() - 版式.cover_tag_inset)
            && 差不多(平台标.right(), 中文标.right()),
        "两枚标的右沿都该离封面右沿 {} 点：封面 {封面:?}，平台标 {平台标:?}，中文标 {中文标:?}",
        版式.cover_tag_inset
    );
    assert!(
        差不多(平台标.top(), 封面.top() + 平台标顶)
            && 差不多(中文标.top(), 封面.top() + 中文标顶),
        "平台标该离封面顶 {平台标顶} 点、中文标 {中文标顶} 点：封面 {封面:?}，平台标 {平台标:?}，中文标 {中文标:?}"
    );
    assert!(
        差不多(平台标.height(), 版式.cover_tag_height)
            && 差不多(中文标.height(), 版式.cover_tag_height),
        "两枚标都该高 {} 点：平台标 {平台标:?}，中文标 {中文标:?}",
        版式.cover_tag_height
    );
    // 标上的字：白字，落在那枚中文标里头。
    let 白字 = 这一帧.shapes.iter().any(|clipped| {
        matches!(&clipped.shape, egui::epaint::Shape::Text(text)
            if text.galley.text().contains("汉化")
                && text.fallback_color == 令牌.color.cover_tag.ink
                && 中文标.contains(text.pos + text.galley.size() / 2.0))
    });
    assert!(白字, "中文标上该是白字「汉化」：{}", 画出来的字(&这一帧));
}

/// **点一下卡，高亮落在它上面，`↓` 接着从它往下走**（设计稿点一张卡就是 `S.sel=i`）。
///
/// 点卡**不抢键盘焦点**：一张卡拿着焦点时窗口那一层的单键整个让给它（焦点那条路），点完 `↓` 就挪不动了。
#[test]
fn 点一下卡高亮落在它上面_上下键接着从它走() {
    let ctx = headless::context();
    let mut app = 四行的界面("点卡");
    let 次序 = 次序(&mut app);
    app.browse_and_site().0.show_cards();
    跑(&ctx, &mut app, Vec::new());

    点卡(&ctx, &mut app, &卡上的名字(&次序[1]));
    assert_eq!(
        app.browse().highlighted(),
        Some(&次序[1]),
        "点了哪张卡，高亮就该落在哪张上"
    );
    assert_eq!(
        app.browse().work().map(|work| &work.anchor),
        Some(&次序[1]),
        "侧边详情该摆它"
    );
    按(&ctx, &mut app, egui::Key::ArrowDown);
    assert_eq!(
        app.browse().highlighted(),
        Some(&次序[2]),
        "点完卡按 ↓ 该从它往下走"
    );
}

/// 这一帧上**描着高亮那一圈**（设计稿 `.gcard[aria-selected] .cover` 外头那一圈 `accent-soft`）的那几个框，
/// 各连着它画的时候裁到哪儿：`(框, 裁剪框)`。
fn 高亮那一圈(
    ctx: &egui::Context,
    output: &egui::FullOutput,
) -> Vec<(egui::Rect, egui::Rect)> {
    fn 收(
        shape: &egui::epaint::Shape,
        color: egui::Color32,
        裁到: egui::Rect,
        out: &mut Vec<(egui::Rect, egui::Rect)>,
    ) {
        match shape {
            egui::epaint::Shape::Rect(rect)
                if rect.stroke.width > 0.0 && rect.stroke.color == color =>
            {
                out.push((rect.rect, 裁到));
            }
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().for_each(|one| 收(one, color, 裁到, out));
            }
            _ => {}
        }
    }
    let 浅强调 = ctx.style_of(ctx.theme()).visuals.selection.bg_fill;
    let mut out = Vec::new();
    for clipped in &output.shapes {
        收(&clipped.shape, 浅强调, clipped.clip_rect, &mut out);
    }
    out
}

/// 屏上**正好**写着这几个字的最后一处（同一张卡的标题画两遍，卡面下半截那一遍后画）。
fn 最后一处正好是(output: &egui::FullOutput, 字: &str) -> Option<egui::Rect> {
    fn 找(shape: &egui::epaint::Shape, 字: &str, 每一处: &mut Vec<egui::Rect>) {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == 字 => {
                每一处.push(egui::Rect::from_min_size(text.pos, text.galley.size()));
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|one| 找(one, 字, 每一处)),
            _ => {}
        }
    }
    let mut 每一处 = Vec::new();
    for clipped in &output.shapes {
        找(&clipped.shape, 字, &mut 每一处);
    }
    每一处.last().copied()
}

/// **表格上高亮第三部，切到卡片墙，高亮还是那一部**：墙上那张卡描着选中那一圈，`空格` 勾中的是它，
/// `↓` 从它往下走（设计稿 `S.sel` 是作品号，两种视图共用）。
#[test]
fn 表格上高亮第三部_切卡片墙高亮还是那一部() {
    let ctx = headless::context();
    let mut app = 四行的界面("切卡片墙");
    let 次序 = 次序(&mut app);
    跑(&ctx, &mut app, Vec::new());
    for _ in 0..3 {
        按(&ctx, &mut app, egui::Key::ArrowDown);
    }
    assert_eq!(
        app.browse().highlighted(),
        Some(&次序[2]),
        "表格上按三下 ↓ 该高亮第三部"
    );

    app.browse_and_site().0.show_cards();
    跑(&ctx, &mut app, Vec::new());
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    assert_eq!(
        app.browse().highlighted(),
        Some(&次序[2]),
        "切到卡片墙，高亮该还在那一部上"
    );
    // **墙上看得出是哪一张**：那一圈正好一个，套在那张卡的封面上——卡面下半截那一行标题就在它正下方。
    let 圈 = 高亮那一圈(&ctx, &这一帧);
    assert_eq!(圈.len(), 1, "卡片墙上该正好一张卡描着高亮那一圈：{圈:?}");
    let 名字 = 卡上的名字(&次序[2]);
    let 标题 = 最后一处正好是(&这一帧, &名字)
        .unwrap_or_else(|| panic!("卡片墙上没有「{名字}」这张卡：\n{}", 画出来的字(&这一帧)));
    assert!(
        圈[0].0.x_range().contains(标题.center().x) && 圈[0].0.bottom() <= 标题.top(),
        "那一圈该套在「{名字}」那张卡的封面上：圈 {:?}，标题 {标题:?}",
        圈[0].0
    );
    // **墙顶上那一排的那两圈也画得全**：那张卡在头一排、墙没滚，封面顶贴着卡片墙视口的上沿——
    // 外扩那几点得准它越过上沿画进上内边距里，不然上半边被裁掉。
    let [_, 外圈] = romcat_gui::tokens::Tokens::builtin().layout.card_ring;
    let (封面, 裁到) = 圈[0];
    assert!(
        裁到.contains_rect(封面.expand(外圈)),
        "头一排那张卡的那两圈该整个画得出来：封面 {封面:?}，裁到 {裁到:?}"
    );

    按(&ctx, &mut app, egui::Key::Space);
    assert!(
        app.browse().picked().contains(&次序[2]),
        "卡片墙上空格该勾中高亮那一部"
    );
    assert_eq!(app.browse().picked().count(4), 1, "只勾中那一部");
    按(&ctx, &mut app, egui::Key::ArrowDown);
    assert_eq!(
        app.browse().highlighted(),
        Some(&次序[3]),
        "卡片墙上 ↓ 该从那一部往下走"
    );
}

/// 二十行的小库，名字是二十个不重样的字（卡上印的正题就是它）。
fn 二十行的界面(名字: &str) -> App {
    const 字: [&str; 20] = [
        "甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸", "子", "丑", "寅", "卯", "辰",
        "巳", "午", "未", "申", "酉",
    ];
    let 文件: Vec<String> = 字.iter().map(|one| format!("{one}.zip")).collect();
    let 手上的: Vec<(&str, &str, 档)> = 文件
        .iter()
        .map(|one| ("SFC", one.as_str(), 档::命中))
        .collect();
    let mut app = shared::小库(
        &手上的,
        shared::干净工作目录(&format!("romcat-测试-高亮-{名字}")),
    );
    app.show_view(View::Browse);
    app
}

/// **找高亮那一部先翻窗里缓着的那一段，再去上一回见到它的那一行取**（`Window::find`）。
///
/// 次序反过来的话：高亮从另一扇窗带过来（表格上的下标，卡片墙只摆有封面的那一批），那个下标在这扇窗里
/// 指着别处——先去那儿取一段，视口那几行被冲掉，眼前那一张反倒找不着，`空格` `Enter` 都不动。
/// 窗开得小（一次四行）好让「冲掉」在二十行里就造得出来。
#[test]
fn 窗里找高亮先翻缓着的那一段_不拿另一扇窗的下标把视口冲掉() {
    use romcat_core::catalog::browse::WorkQuery;
    use romcat_gui::table::{Highlight, Window};

    let mut app = 二十行的界面("窗里找高亮");
    let 次序 = 次序(&mut app);
    let (_, site) = app.browse_and_site();
    let mut 窗 = Window::new(4);
    窗.set_query(WorkQuery::default());
    窗.sync(&site.catalog);
    // 视口摆着头几行：窗里缓着的就是那一段。
    for at in 0..3 {
        assert!(窗.row(&site.catalog, at).is_some(), "第 {at} 行该读得出");
    }
    let 读过 = 窗.reads();

    // 眼前那一张（第 2 行），带着的下标却是另一扇窗里的 12：照样认得出，而且一次库都不读。
    let 带过来的 = Highlight {
        anchor: 次序[2].clone(),
        near: 12,
    };
    assert_eq!(窗.find(&site.catalog, &带过来的), Some(2));
    assert_eq!(窗.reads(), 读过, "眼前那一段里就有，不该为它去库里取一段");

    // 滚远了的那一部（不在缓着的那一段里）：去上一回见到它的那一行取回来认。
    let 滚远了的 = Highlight {
        anchor: 次序[15].clone(),
        near: 15,
    };
    assert_eq!(窗.find(&site.catalog, &滚远了的), Some(15));
}

/// **卡片墙滚起来不跳**：拿滚轮一截一截往下滚，墙上同一张卡每一截挪的一样多。
///
/// 从前卡片墙报给滚动区的一排比真摆出来的高 20 点（那道纵向缝报了、没摆），头一排一换整面墙往下跳 20 点
/// ——`↓` 照那个数折出来的位置滚，也就滚不准（挂单 `Q1468`）。
#[test]
fn 卡片墙拿滚轮一截一截滚_同一张卡每截挪得一样多() {
    let ctx = headless::context();
    let mut app = 二十行的界面("卡片墙滚轮");
    app.browse_and_site().0.show_cards();
    跑(&ctx, &mut app, Vec::new());
    let 盯着 = "庚";
    let 滚 = |ctx: &egui::Context, app: &mut App, 多少: f32| -> f32 {
        跑(
            ctx,
            app,
            vec![
                egui::Event::PointerMoved(egui::pos2(700.0, 500.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -多少),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        for _ in 0..30 {
            跑(ctx, app, Vec::new());
        }
        let 这一帧 = headless::frame(ctx, headless::input(), |ui| app.ui(ui));
        最后一处正好是(&这一帧, 盯着)
            .unwrap_or_else(|| panic!("墙上没有「{盯着}」：\n{}", 画出来的字(&这一帧)))
            .top()
    };
    let mut 上一回 = 滚(&ctx, &mut app, 290.0);
    let mut 每截 = Vec::new();
    for _ in 0..5 {
        let 这一回 = 滚(&ctx, &mut app, 20.0);
        每截.push(上一回 - 这一回);
        上一回 = 这一回;
    }
    assert!(
        每截
            .iter()
            .all(|one| (one - 每截[0]).abs() <= 0.5 && *one > 0.0),
        "每滚一截，同一张卡该往上挪一样多：{每截:?}"
    );
}

/// **卡片墙上 `↓` 走出一屏，墙跟着滚**：高亮那一张整个落在视口里（设计稿 `scrollIntoView({block:'nearest'})`，
/// 表格那一路同一件事）。
#[test]
fn 卡片墙上下键走出一屏时墙跟着滚到高亮那一张() {
    let mut app = 二十行的界面("卡片墙滚");
    let ctx = headless::context();
    let 次序 = 次序(&mut app);
    assert_eq!(次序.len(), 20, "夹具该摆出二十行");
    app.browse_and_site().0.show_cards();
    跑(&ctx, &mut app, Vec::new());
    for _ in 0..18 {
        按(&ctx, &mut app, egui::Key::ArrowDown);
    }
    assert_eq!(app.browse().highlighted(), Some(&次序[17]), "按了十八下 ↓");
    // 滚过去是带动画的（egui 的 `scroll_animation`，最长三成秒）；这一层每帧走 1/60 秒，多跑几帧让它滚完。
    for _ in 0..30 {
        跑(&ctx, &mut app, Vec::new());
    }

    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 圈 = 高亮那一圈(&ctx, &这一帧);
    let [(封面, 裁到)] = 圈[..] else {
        panic!("卡片墙上该正好一张卡描着高亮那一圈：{圈:?}");
    };
    // 那一圈只裁在视口里：封面整个露着，裁剪框就是封面外扩那一圈那么大，把封面整个包住。
    assert!(
        裁到.contains_rect(封面),
        "高亮那一张该整个滚进视口：封面 {封面:?}，裁到 {裁到:?}"
    );
}

/// **卡片墙上那三道门照样关得住**（挂单 `Q1143`）：有一层弹层开着、有一层浮层摊着、光标在搜索框里时，
/// `↓` 一下都不挪高亮——与表格那一路问的是同一处（`keys::allowed`）。
#[test]
fn 卡片墙上有弹层有浮层光标在搜索框里时上下键不挪高亮() {
    let ctx = headless::context();
    let mut app = 四行的界面("卡片墙三道门");
    let 次序 = 次序(&mut app);
    app.browse_and_site().0.show_cards();
    跑(&ctx, &mut app, Vec::new());
    按(&ctx, &mut app, egui::Key::ArrowDown);
    assert_eq!(
        app.browse().highlighted(),
        Some(&次序[0]),
        "先在墙上高亮头一张"
    );

    // 一、**有弹层**：按 `?` 摊开快捷键表那一层。
    跑(
        &ctx,
        &mut app,
        键(egui::Key::Questionmark, egui::Modifiers::NONE),
    );
    let mut 画的 = String::new();
    for _ in 0..3 {
        画的 = 跑(&ctx, &mut app, Vec::new());
    }
    assert!(画的.contains("查看快捷键"), "快捷键表该摊开了：\n{画的}");
    按(&ctx, &mut app, egui::Key::ArrowDown);
    assert_eq!(
        app.browse().highlighted(),
        Some(&次序[0]),
        "弹层开着时 ↓ 不该挪高亮"
    );
    跑(&ctx, &mut app, 键(egui::Key::Escape, egui::Modifiers::NONE));
    for _ in 0..3 {
        跑(&ctx, &mut app, Vec::new());
    }

    // 二、**有浮层**：右键第三张卡，菜单摊着（右键本身把高亮挪到那一张，设计稿 `contextmenu` 先 `S.sel=i`）。
    let 名字 = 卡上的名字(&次序[2]);
    let 画的 = 右键(&ctx, &mut app, &名字);
    assert!(画的.contains("刮削此作品"), "菜单该摊开了：\n{画的}");
    assert_eq!(
        app.browse().highlighted(),
        Some(&次序[2]),
        "右键那一张该成了高亮"
    );
    按(&ctx, &mut app, egui::Key::ArrowDown);
    assert_eq!(
        app.browse().highlighted(),
        Some(&次序[2]),
        "菜单摊着时 ↓ 不该挪高亮"
    );
    跑(&ctx, &mut app, 键(egui::Key::Escape, egui::Modifiers::NONE));
    for _ in 0..3 {
        跑(&ctx, &mut app, Vec::new());
    }

    // 三、**光标在搜索框里**：`⌘/Ctrl+F` 把光标放进去，`↓` 归那一框。
    跑(&ctx, &mut app, 键(egui::Key::F, 修饰键()));
    for _ in 0..4 {
        跑(&ctx, &mut app, Vec::new());
    }
    按(&ctx, &mut app, egui::Key::ArrowDown);
    assert_eq!(
        app.browse().highlighted(),
        Some(&次序[2]),
        "光标在搜索框里时 ↓ 不该挪高亮"
    );
}

/// 键盘焦点眼下落在哪个控件上：读屏那一层（AccessKit）报的角色与名字。卡片墙上一张卡申报为按钮，名字是卡上的标题。
fn 焦点落在(output: &egui::FullOutput) -> Option<(egui::accesskit::Role, String)> {
    let tree = output.platform_output.accesskit_update.as_ref()?;
    let (_, node) = tree.nodes.iter().find(|(id, _)| *id == tree.focus)?;
    Some((node.role(), node.label().unwrap_or_default().to_owned()))
}

/// **Tab + Enter 那条无障碍路在卡片墙上照旧走得通**（挂单 `Q1143` 裁定留着它）：Tab 走到一张卡，`Enter`
/// 打开它（侧边详情摆它、高亮挪到它）、`空格` 勾选它——由那张卡自己接。
///
/// **那张卡问的也是那三道门**：右键它摊开菜单（焦点照旧在它手上），这时 `空格` 不许再勾一下——
/// 浮层摊着时单键归浮层（今天之前这一处一道门都不问）。
#[test]
fn 卡片墙上tab走到一张卡_回车打开_空格勾选_浮层摊着时不接() {
    let ctx = headless::context();
    ctx.enable_accesskit();
    let mut app = 四行的界面("卡片墙跳格");
    let 次序 = 次序(&mut app);
    let 名字们: Vec<String> = 次序.iter().map(卡上的名字).collect();
    app.browse_and_site().0.show_cards();
    跑(&ctx, &mut app, Vec::new());

    // 一下一下按 Tab，直到焦点落在一张卡上（它前头是左栏导航、筛选栏、卡片那一条上的几颗）。
    let mut 落在 = None;
    for _ in 0..200 {
        let 这一帧 = headless::frame(
            &ctx,
            shared::输入(vec![shared::按键事件(egui::Key::Tab)]),
            |ui| app.ui(ui),
        );
        if let Some((egui::accesskit::Role::Button, 名字)) = 焦点落在(&这一帧)
            && let Some(at) = 名字们.iter().position(|one| *one == 名字)
        {
            落在 = Some(at);
            break;
        }
    }
    let 第几张 = 落在.expect("按了两百下 Tab，焦点一次都没落在卡片墙的卡上");
    let 那一部 = &次序[第几张];
    // **拿着焦点的那一张封面外头描一圈强调色**（设计稿 `.gcard:focus-visible .cover{box-shadow:0 0 0 2px var(--accent)}`），
    // 只这一圈、不带外头那一圈强调浅色（那是高亮的）。
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 圈 = 封面外头那一圈(&ctx, &这一帧);
    let [强调宽, _] = romcat_gui::tokens::Tokens::builtin().layout.card_ring;
    assert!(
        matches!(圈[..], [(_, 宽, egui::StrokeKind::Outside)] if (宽 - 强调宽).abs() < 0.01),
        "拿着焦点的那一张封面外头该描一圈 {强调宽} 点强调色：{圈:?}"
    );
    assert!(
        高亮那一圈(&ctx, &这一帧).is_empty(),
        "只拿着焦点、没高亮，不该有外头那一圈强调浅色"
    );

    按(&ctx, &mut app, egui::Key::Enter);
    assert_eq!(
        app.browse().work().map(|work| &work.anchor),
        Some(那一部),
        "Tab 走到的那张卡上按 Enter，侧边详情该摆它"
    );
    assert_eq!(app.browse().highlighted(), Some(那一部), "高亮该挪到它");
    按(&ctx, &mut app, egui::Key::Space);
    assert!(
        app.browse().picked().contains(那一部),
        "Tab 走到的那张卡上按空格该勾中它"
    );
    assert_eq!(app.browse().picked().count(4), 1, "只勾中它一部");

    // 右键它：菜单摊开，焦点照旧在它手上。这时空格不许再动勾选。
    let 画的 = 右键(&ctx, &mut app, &名字们[第几张]);
    assert!(画的.contains("刮削此作品"), "菜单该摊开了：\n{画的}");
    按(&ctx, &mut app, egui::Key::Space);
    assert!(
        app.browse().picked().contains(那一部),
        "菜单摊着时那张卡上的空格不该把勾去掉"
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
