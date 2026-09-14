//! **版式可调与视觉一致性**（票 `gui-redesign/12`）。
//!
//! 这几条断言不看代码长什么样，看的是**跑出来的结果**：真的往那条边界上按下去、拖过去、
//! 松手，然后问 egui 那块面板现在多宽、问文件系统那个数落在哪儿、问这一帧到底画出了
//! 哪几个字。
//!
//! - **拖得动**：三屏六条边界，每一条按住往外拖 60 点就宽 60 点。
//! - **记得住**：拖完关掉再开，还是那个样子；那个数落在**工作目录**里，
//!   中立库那份文件一个字节都不多。
//! - **挤不没**：往里拖到底停在下限，往外拖到底也留得住正中那块。
//! - **逐字一致**：置信度四档的词在浏览屏与待确认屏上是同一个词，
//!   而且**每一处上色的地方都跟着那个词**。
//! - **窗口标题**：写明开的是哪一份库、看的是哪一屏。
//! - **键盘焦点**：拿到焦点的控件与没拿到的画得不一样。

use romcat_gui::app::{App, View};
use romcat_gui::layout::{self, Boundary, Side};
use romcat_gui::{demo, headless, look};

mod shared;
use shared::画出来的字;

/// 浏览那一屏的规模。**照真库的形状来**：`demo::BROWSE_VARIANTS` 个变体收敛成
/// `demo::BROWSE_LINES` 行主列表（`docs/library-facts.md`）。
///
/// 版式那几条量的是面板宽窄与屏上摆得下几行，而**屏上摆的是主列表那一行**——
/// 从前这儿的变体数对了、作品数却是写死的二十个，收出来只有 3,596 行（挂单 `Q156`）。
const ROWS: u64 = demo::BROWSE_VARIANTS;

/// 待确认那一屏的规模。真机上一万八千多条（票 08 实测 16,656）。
///
/// **引合成数据自己那一份，不再另写一遍**：同一个数写两处，改一处就悄悄分了岔。
const QUEUE_ROWS: u64 = demo::QUEUE_ROWS;

/// 这一趟测试自己的**工作目录**。
///
/// **不共用 `demo::workspace()`**：版式偏好是往工作目录里写文件的，共用一个的话，
/// 这一条测试拖出来的宽度会落到另一条测试打开的窗口上。每条测试一个目录，
/// 而且先清干净——上一趟跑剩下的版式不该影响这一趟。
fn 工作目录(名字: &str) -> std::path::PathBuf {
    let at = std::env::temp_dir().join(format!("romcat-版式-{名字}"));
    let _ = std::fs::remove_dir_all(&at);
    at
}

/// 浏览屏那一路的界面。
fn 浏览(workspace: &std::path::Path) -> App {
    let site = demo::site(demo::browse(ROWS).expect("造得出合成数据")).expect("开得出现场");
    let mut app = App::new(site, workspace.to_path_buf());
    app.show_view(View::Browse);
    app
}

/// 待确认屏那一路的界面。
fn 待确认(workspace: &std::path::Path) -> App {
    let site = demo::site(demo::queue(QUEUE_ROWS).expect("造得出合成数据")).expect("开得出现场");
    let mut app = App::new(site, workspace.to_path_buf());
    app.show_view(View::Queue);
    app
}

/// 跑一帧，带上这些事件。
fn 跑一帧(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) -> egui::FullOutput {
    let mut input = headless::input();
    input.events = events;
    headless::frame(ctx, input, |ui| app.ui(ui))
}

/// 跑几帧空的。
fn 跑(ctx: &egui::Context, app: &mut App, frames: u32) {
    for _ in 0..frames {
        跑一帧(ctx, app, Vec::new());
    }
}

/// 这条边界眼下多宽（多高）。问的是 egui 自己存的那一份。
fn 多宽(ctx: &egui::Context, boundary: Boundary) -> f32 {
    let state = egui::PanelState::load(ctx, egui::Id::new(boundary.id))
        .unwrap_or_else(|| panic!("这一帧没画「{}」那块面板", boundary.id));
    boundary.side.of(state.size())
}

/// 按住这条边界拖到「这么宽」，然后松手。返回松手之后它真的多宽。
///
/// **按下去那一下往面板里头偏 [`抓偏`] 点**：把手正压在边界线上，而边界线也是正中那块
/// 的边——正中那块是后画的，压在把手上头，按在线上那一下会被它接走。
///
/// **一帧一件事**：挪过去、按下去、拖过去、松开。egui 判「这是在拖不是在点」要看按下
/// 之后走了多远，挤在一帧里它只会当成一次点击。
fn 拖到(ctx: &egui::Context, app: &mut App, boundary: Boundary, 目标: f32) -> f32 {
    let rect = egui::PanelState::load(ctx, egui::Id::new(boundary.id))
        .unwrap_or_else(|| panic!("这一帧没画「{}」那块面板", boundary.id))
        .outer_rect;
    // 面板的**固定边**在窗口那一侧，把手在另一侧；拖到「这么宽」就是把把手放到
    // 「固定边 ± 目标」那个位置上。
    let (from, to) = match boundary.side {
        Side::Left => (
            egui::pos2(rect.max.x - 抓偏, rect.center().y),
            egui::pos2(rect.min.x + 目标, rect.center().y),
        ),
        Side::Right => (
            egui::pos2(rect.min.x + 抓偏, rect.center().y),
            egui::pos2(rect.max.x - 目标, rect.center().y),
        ),
        Side::Bottom => (
            egui::pos2(rect.center().x, rect.min.y + 抓偏),
            egui::pos2(rect.center().x, rect.max.y - 目标),
        ),
    };
    let modifiers = egui::Modifiers::default();
    let 按 = |pos: egui::Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    };
    跑一帧(ctx, app, vec![egui::Event::PointerMoved(from)]);
    跑一帧(ctx, app, vec![按(from, true)]);
    跑一帧(ctx, app, vec![egui::Event::PointerMoved(to)]);
    跑一帧(ctx, app, vec![egui::Event::PointerMoved(to), 按(to, false)]);
    跑一帧(ctx, app, Vec::new());
    多宽(ctx, boundary)
}

/// 按下去那一下往面板里头偏几点。
const 抓偏: f32 = 3.0;

/// 按一下 Tab：焦点往下一个可操作件走。
fn 跳格键() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    }
}

/// 这条边界在这个视口上的上限：**整个窗口那一维的几成**（`Boundary::share`）。
fn 上限(boundary: Boundary) -> f32 {
    (boundary.side.of(egui::Vec2::from(headless::VIEWPORT)) * boundary.share).max(boundary.min)
}

/// 这一帧画出来的那些**框的描边颜色**。焦点看不看得见就看它。
fn 描边(output: &egui::FullOutput) -> Vec<egui::Color32> {
    fn 收(shape: &egui::epaint::Shape, out: &mut Vec<egui::Color32>) {
        match shape {
            egui::epaint::Shape::Rect(rect) if rect.stroke.width > 0.0 => {
                out.push(rect.stroke.color);
            }
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    收(one, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        收(&clipped.shape, &mut out);
    }
    out
}

/// 把这一屏摆成「这一屏的边界都在屏上」的样子。
///
/// **浏览屏不必摆**：从前要先摊开刮削面板才有第四条边界，如今刮削是一层弹层，不占屏上的
/// 地方——而且它开着的时候底下那几条边界反倒拖不动（弹层盖在整屏上头）。
fn 摆开(app: &mut App, screen: View) {
    app.show_view(screen);
    // 逐条那一路才有左栏与裁决面板；分批那一路整屏就是一列卡片。
    if screen == View::Queue {
        app.queue_and_site().0.show_one_by_one();
    }
}

#[test]
fn 三屏的面板边界都拖得动() {
    // 验收第 1 条。`egui::Panel::bottom` **默认是拖不动的**，这一票之前那三块底栏
    // （浏览编辑、裁决面板）正是这么钉死的；而光打开 `resizable` 还不够——
    // 内容不把地方占满的话，拖宽了下一帧又缩回去（[`Boundary::show`]）。
    for screen in [View::Browse, View::Queue, View::Sublibraries] {
        let workspace = 工作目录(&format!("拖得动-{screen:?}"));
        let mut app = if screen == View::Queue {
            待确认(&workspace)
        } else {
            浏览(&workspace)
        };
        摆开(&mut app, screen);
        let ctx = headless::context();
        跑(&ctx, &mut app, 3);
        for boundary in Boundary::ALL.into_iter().filter(|it| it.screen == screen) {
            // 拖到**上限往里 40 点**：稳稳落在下限与上限之间，而且离默认那个数够远，
            // 「其实没动」骗不过去。
            let 目标 = 上限(boundary) - 40.0;
            let 拖之前 = 多宽(&ctx, boundary);
            let 拖之后 = 拖到(&ctx, &mut app, boundary, 目标);
            assert!(
                (拖之后 - 目标).abs() <= 4.0,
                "「{}」从 {拖之前} 拖向 {目标}，落在了 {拖之后}",
                boundary.id,
            );
        }
    }
}

#[test]
fn 拖动后的位置存在工作目录里不存在中立库里() {
    // 验收第 2 条。中立库整份可再生，界面偏好放进去会被某一次重扫抹掉。
    let workspace = 工作目录("存工作目录");
    let mut app = 浏览(&workspace);
    let ctx = headless::context();
    跑(&ctx, &mut app, 3);
    let 宽 = 拖到(&ctx, &mut app, layout::FILTER, 300.0);

    let at = app.layout().path().to_path_buf();
    assert!(
        at.starts_with(&workspace),
        "版式落在了工作目录之外：{}",
        at.display(),
    );
    let 写下的 = std::fs::read_to_string(&at).expect("拖完就该落盘");
    assert!(
        写下的.contains(&format!("{} = {宽:.0}", layout::FILTER.id)),
        "文件里没有刚拖出来的那个数：\n{写下的}",
    );
    // **中立库那一侧一个字节都不多**：合成数据那份库整个在内存里，
    // 而工作目录里除了版式那一份，这一趟没建过别的东西。
    assert!(!写下的.contains("sqlite"), "版式偏好不该跟中立库住在一起",);
}

#[test]
fn 关掉再打开还是那个样子() {
    // 验收第 2 条的另一半。**新开一个 `App`** 走的就是关掉再打开那条路：
    // 版式在构造时从工作目录读出来，开窗第一帧塞回 egui。
    let workspace = 工作目录("关掉再开");
    let 拖出来的 = {
        let mut app = 浏览(&workspace);
        let ctx = headless::context();
        跑(&ctx, &mut app, 3);
        拖到(&ctx, &mut app, layout::DETAIL, 430.0)
    };

    let mut 再开 = 浏览(&workspace);
    // **新的上下文**：egui 自己那份内存里的面板尺寸表跟着旧窗口一起没了，
    // 这一趟只能靠工作目录里那个文件。
    let ctx = headless::context();
    跑(&ctx, &mut 再开, 2);
    assert!(
        (多宽(&ctx, layout::DETAIL) - 拖出来的).abs() <= 1.0,
        "再打开变成了 {}，拖出来的是 {拖出来的}",
        多宽(&ctx, layout::DETAIL),
    );
}

#[test]
fn 拖到极限时不塌陷也不把正中那块挤没() {
    // 验收第 3 条。往里拖到底：**塌不下去**——下限拦一道，而面板本来也窄不过它自己
    // 最窄的那个子控件（那不是缺陷，是「不塌陷」的另一半）。
    // 往外拖到底：**正中那张表还留得住**，因为每条边界的上限是整个窗口的几成，
    // 而同一维上几条加起来不超过八成。
    let workspace = 工作目录("极限");
    let mut app = 浏览(&workspace);
    let ctx = headless::context();
    跑(&ctx, &mut app, 3);

    for boundary in [layout::FILTER, layout::DETAIL, layout::EDIT] {
        let 拖到底 = 拖到(&ctx, &mut app, boundary, -2000.0);
        assert!(
            拖到底 >= boundary.min,
            "「{}」往里拖到底塌成了 {拖到底}，下限是 {}",
            boundary.id,
            boundary.min,
        );
    }
    // 那三块的内容还在：塌陷了的话这几句抬头一个都画不出来。
    let 屏上 = 画出来的字(&跑一帧(&ctx, &mut app, Vec::new()));
    for 抬头 in ["一按就有的档", "变体", "元数据"] {
        assert!(
            屏上.contains(抬头),
            "往里拖到底之后「{抬头}」那一块没了：\n{屏上}"
        );
    }

    for boundary in [layout::FILTER, layout::DETAIL, layout::EDIT] {
        let 拖到底 = 拖到(&ctx, &mut app, boundary, 5000.0);
        assert!(
            拖到底 <= 上限(boundary) + 1.0,
            "「{}」往外拖到底涨到了 {拖到底}，上限该是 {}",
            boundary.id,
            上限(boundary),
        );
    }
    let 视口 = egui::Vec2::from(headless::VIEWPORT);
    let 正中宽 = 视口.x - 多宽(&ctx, layout::FILTER) - 多宽(&ctx, layout::DETAIL);
    let 正中高 = 视口.y - 多宽(&ctx, layout::EDIT);
    assert!(
        正中宽 >= layout::FLOOR,
        "左右两栏拖到底之后，正中那张表只剩 {正中宽} 点宽",
    );
    assert!(
        正中高 >= layout::FLOOR,
        "底栏拖到底之后，正中那张表只剩 {正中高} 点高",
    );
}

#[test]
fn 窗口变小再变回来不会把拖出来的宽度削掉() {
    // **记的是「人拖到哪儿」，不是「这一帧画成多宽」**（`Layout::harvest`）。
    // 面板的上限跟着窗口走，而窗口尺寸本身不持久化（`eframe` 的 `persistence` 没开）
    // ——照单全收的话，大窗口上拖出来的宽度会在下一次开窗（1280×800）被夹一刀，
    // 而且那一刀写进文件，再也回不来。
    let workspace = 工作目录("窗口变小");
    let mut app = 浏览(&workspace);
    let ctx = headless::context();
    跑(&ctx, &mut app, 3);
    let 拖出来的 = 拖到(&ctx, &mut app, layout::DETAIL, 500.0);
    assert!(
        (拖出来的 - 500.0).abs() <= 4.0,
        "先拖到 500，实际 {拖出来的}"
    );

    // 把视口缩到最小窗口那么大，连画几帧：「浏览详情」会被上限夹到 500 以下。
    let 小 = |ctx: &egui::Context, app: &mut App| {
        let mut input = headless::input();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(720.0, 480.0),
        ));
        headless::frame(ctx, input, |ui| app.ui(ui));
    };
    for _ in 0..3 {
        小(&ctx, &mut app);
    }
    assert!(
        多宽(&ctx, layout::DETAIL) < 拖出来的,
        "窗口小到 720 宽，这块面板本该被夹窄",
    );
    // 关掉再打开（新的上下文、原样的视口）：拖出来的那个数原样回来。
    let mut 再开 = 浏览(&workspace);
    let ctx = headless::context();
    跑(&ctx, &mut 再开, 2);
    assert!(
        (多宽(&ctx, layout::DETAIL) - 拖出来的).abs() <= 1.0,
        "窗口缩过一趟之后再打开变成了 {}，拖出来的是 {拖出来的}",
        多宽(&ctx, layout::DETAIL),
    );
}

#[test]
fn 置信度四档在两屏上是同一个词() {
    // 验收第 4 条。收之前：主列表那一栏写「高 · 缺 3 样」、详情面板的变体行写
    // 「高｜某某.zip｜1.2 MB」，而待确认屏写「高置信」——同一个变体，两个词。
    let 词: Vec<&str> = romcat_core::catalog::identify::Tier::ALL
        .iter()
        .map(|tier| tier.label())
        .collect();
    assert_eq!(词, vec!["高置信", "中置信", "低置信", "没有候选"]);

    let workspace = 工作目录("四档");
    let mut app = 浏览(&workspace);
    let ctx = headless::context();
    跑(&ctx, &mut app, 2);
    // 点开一行，详情面板里才有变体行——那正是从前写「高 / 中 / 低」的地方。
    let anchor = {
        let (browse, site) = app.browse_and_site();
        site.catalog
            .work_page(browse.query(), 0, 1)
            .expect("读得动")
            .first()
            .expect("有行")
            .anchor
            .clone()
    };
    {
        let (browse, site) = app.browse_and_site();
        browse.open_work(&site.catalog, &anchor);
    }
    let 浏览屏上 = 画出来的字(&跑一帧(&ctx, &mut app, Vec::new()));

    let mut 队列 = 待确认(&工作目录("四档-队列"));
    let ctx = headless::context();
    let 队列屏上 = 画出来的字(&跑一帧(&ctx, &mut 队列, Vec::new()));

    let 两屏 = format!("{浏览屏上}\n{队列屏上}");
    for 一档 in &词 {
        assert!(
            两屏.contains(一档),
            "两屏上一处都没画出「{一档}」：\n{两屏}",
        );
    }
    // **旧写法一处都不许剩**：那三个短词从前是这么摆的——「高｜某某.zip｜…」。
    for 旧的 in ["高｜", "中｜", "低｜"] {
        assert!(
            !两屏.contains(旧的),
            "屏上还留着旧写法「{旧的}」：\n{浏览屏上}",
        );
    }
}

#[test]
fn 每一处上了色的置信度都跟着那个词() {
    // 验收第 5 条。收之前，逐条那张表的「候选」一栏是一个**光染了色的数字**——
    // 色觉障碍下那一栏就只剩一个数，读不出它是稳还是悬。
    let mut app = 待确认(&工作目录("颜色不是唯一线索"));
    app.queue_and_site().0.show_one_by_one();
    let ctx = headless::context();
    let 屏上 = 画出来的字(&跑一帧(&ctx, &mut app, Vec::new()));
    assert!(
        屏上.contains("候选 · 置信度"),
        "那一栏的表头该说清它画的是什么：\n{屏上}",
    );
    // 那一栏的每一格是「3 · 高置信」这个样子：数字后面跟着档名。
    let 有一格 = romcat_core::catalog::identify::Tier::ALL
        .iter()
        .any(|tier| 屏上.contains(&format!(" · {}", tier.label())));
    assert!(有一格, "那一栏还是光一个数：\n{屏上}");
}

#[test]
fn 窗口标题写明哪一份库与哪一屏() {
    // 验收第 6 条。**两样都要**：任务栏上并排两个 romcat 时「哪个是哪份库」，
    // 截图发出来时「这是哪一屏」。
    let mut app = 浏览(&工作目录("标题"));
    for screen in View::ALL {
        app.show_view(screen);
        let 标题 = app.window_title();
        assert!(
            标题.contains(demo::LIBRARY),
            "标题里没写开的是哪一份库：{标题}",
        );
        assert!(
            标题.contains(screen.label()),
            "标题里没写看的是哪一屏：{标题}",
        );
    }
    // 合成数据那一路另给一个名字：假数据与真库在界面上长得一模一样，
    // 标题是唯一一直看得见的区分处。
    app.set_library_label("合成数据（演示）");
    assert!(app.window_title().contains("合成数据（演示）"));
}

#[test]
fn 换一屏就把窗口标题改掉() {
    // 上一条查的是那串字对不对，这一条查它**真的发出去了**。
    let mut app = 浏览(&工作目录("标题命令"));
    let ctx = headless::context();
    let 头一帧 = 跑一帧(&ctx, &mut app, Vec::new());
    assert!(
        改标题(&头一帧).is_some_and(|title| title.contains("浏览")),
        "开窗第一帧就该把标题定下来",
    );
    // 同一屏上再画一帧，不该重发。
    assert!(
        改标题(&跑一帧(&ctx, &mut app, Vec::new())).is_none(),
        "没换屏还发标题，等于每帧给窗口系统递一条命令",
    );
    app.show_view(View::Tasks);
    assert!(
        改标题(&跑一帧(&ctx, &mut app, Vec::new())).is_some_and(|title| title.contains("任务")),
        "换了屏，标题该跟着改",
    );
}

/// 这一帧有没有发「改标题」那条命令，发的是哪一串。
fn 改标题(output: &egui::FullOutput) -> Option<String> {
    output.viewport_output.values().find_map(|viewport| {
        viewport.commands.iter().find_map(|command| match command {
            egui::ViewportCommand::Title(title) => Some(title.clone()),
            _ => None,
        })
    })
}

#[test]
fn 拿到键盘焦点的控件画得不一样() {
    // 验收第 7 条。egui 把「拿到焦点」与「正被按下」并成同一档 `widgets.active`，
    // 默认那一档的描边只是白（暗色）或黑（亮色）——与悬停那一档的灰只差一点点。
    // `look::install` 把它换成主题自己的强调色，也就是 `TextEdit` 拿到焦点时的那一圈。
    let ctx = headless::context();
    look::install(&ctx);
    let 强调色 = ctx.style_of(ctx.theme()).visuals.selection.stroke.color;

    let mut 按钮 = None;
    let 没焦点 = headless::frame(&ctx, headless::input(), |ui| {
        按钮 = Some(ui.button("按一下").id);
    });
    assert!(!描边(&没焦点).contains(&强调色), "没焦点的时候不该画那一圈",);

    let mut input = headless::input();
    input.events = vec![跳格键()];
    headless::frame(&ctx, input, |ui| {
        let _ = ui.button("按一下");
    });
    let 有焦点 = headless::frame(&ctx, headless::input(), |ui| {
        let _ = ui.button("按一下");
    });
    assert_eq!(
        ctx.memory(egui::Memory::focused),
        按钮,
        "按一下 Tab，焦点该落到那颗按钮上",
    );
    assert!(
        描边(&有焦点).contains(&强调色),
        "拿到焦点的按钮上没画出那一圈",
    );
}

#[test]
fn 自己画底色的可点件也描得出焦点那一圈() {
    // 表格的行与缩略图那几格自己画底色，走不了 egui 按钮那条路——而它们**是点得中的**，
    // 于是 Tab 走得到（`Sense::click` 自带 `FOCUSABLE`）。走得到又看不见是最坏的一种，
    // 所以它们各自描一圈（[`look::focus_ring`]）。这一条跑的就是那个函数。
    let ctx = headless::context();
    look::install(&ctx);
    let 强调色 = ctx.style_of(ctx.theme()).visuals.selection.stroke.color;
    let id = egui::Id::new("一行");
    let 画 = |ctx: &egui::Context, events: Vec<egui::Event>| {
        let mut input = headless::input();
        input.events = events;
        headless::frame(ctx, input, |ui| {
            let rect = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(200.0, 24.0));
            let response = ui.interact(rect, id, egui::Sense::click());
            look::focus_ring(ui.ctx(), ui.clip_rect(), &response);
        })
    };
    let 没焦点 = 画(&ctx, Vec::new());
    assert!(!描边(&没焦点).contains(&强调色), "没焦点的时候不该画那一圈",);
    画(&ctx, vec![跳格键()]);
    let 有焦点 = 画(&ctx, Vec::new());
    assert_eq!(
        ctx.memory(egui::Memory::focused),
        Some(id),
        "Tab 该走得到它"
    );
    assert!(
        描边(&有焦点).contains(&强调色),
        "拿到焦点的那一行上没描出那一圈",
    );
}

#[test]
fn 五屏上按一下跳格键焦点都落得下去() {
    // 「所有可操作件上都有可见状态」的前提是**焦点走得到它们**：
    // 一屏上一个可聚焦的控件都没有的话，可见不可见根本无从谈起。
    for screen in View::ALL {
        let mut app = if screen == View::Queue {
            待确认(&工作目录(&format!("焦点-{screen:?}")))
        } else {
            浏览(&工作目录(&format!("焦点-{screen:?}")))
        };
        app.show_view(screen);
        let ctx = headless::context();
        跑(&ctx, &mut app, 2);
        跑一帧(&ctx, &mut app, vec![跳格键()]);
        跑(&ctx, &mut app, 1);
        assert!(
            ctx.memory(egui::Memory::focused).is_some(),
            "{screen:?} 上按 Tab 焦点没落到任何控件上",
        );
    }
}

#[test]
fn 五屏上画出来的字里没有星号也没有文档编号() {
    // 屏上的每一句都得是维护者用得上的话（票 `gui-looks-like-the-design/03`）：
    // egui 不认 markdown，`**` 原样印出来；ADR 编号是写给开发者的出处。
    for screen in View::ALL {
        let mut app = if screen == View::Queue {
            待确认(&工作目录(&format!("文案-{screen:?}")))
        } else {
            浏览(&工作目录(&format!("文案-{screen:?}")))
        };
        app.show_view(screen);
        let ctx = headless::context();
        跑(&ctx, &mut app, 2);
        let 屏上 = 画出来的字(&跑一帧(&ctx, &mut app, Vec::new()));
        assert!(
            !屏上.trim().is_empty(),
            "{screen:?} 上一个字都没画，这条什么都没验",
        );
        for 不该有 in ["**", "ADR-"] {
            assert!(
                !屏上.contains(不该有),
                "{screen:?} 上画出了「{不该有}」：\n{屏上}",
            );
        }
    }
}

// ——— 左栏与屏头（票 `gui-looks-like-the-design/32`） ———

/// 屏上认得下的每一段字画在哪儿，按画出来的次序。
fn 画在哪几处(output: &egui::FullOutput, 认: &dyn Fn(&str) -> bool) -> Vec<egui::Rect> {
    每一段(output)
        .into_iter()
        .filter(|(画的, _)| 认(画的))
        .map(|(_, 在)| 在)
        .collect()
}

/// 屏上**正好**写着 `字` 的每一处，按画出来的次序。
fn 正好画在哪几处(output: &egui::FullOutput, 字: &str) -> Vec<egui::Rect> {
    画在哪几处(output, &|画的| 画的 == 字)
}

/// 左栏展开时多宽（令牌 `rail-width`）。
fn 左栏宽() -> f32 {
    romcat_gui::tokens::Tokens::builtin().layout.rail_width
}

/// 屏头的下沿：上下内边距、按钮那么高的一行、底下那道一点宽的线（`look::screen_header`）。
fn 屏头底() -> f32 {
    let tokens = romcat_gui::tokens::Tokens::builtin();
    2.0 * tokens.space.screen_header_padding[0] + tokens.layout.button_height + 1.0
}

/// 右侧那一段在标题后面摆不下、折到第二行时屏头的下沿：再多一行按钮高，加一档行距。
fn 折成两行的屏头底() -> f32 {
    let tokens = romcat_gui::tokens::Tokens::builtin();
    屏头底() + tokens.space.screen_header_gap + tokens.layout.button_height
}

/// 左栏里（整段落在左栏宽以内）正好写着 `字` 的那一处。
fn 左栏里的(output: &egui::FullOutput, 字: &str) -> Option<egui::Rect> {
    正好画在哪几处(output, 字)
        .into_iter()
        .find(|rect| rect.right() <= 左栏宽())
}

/// 按一下左栏里正好写着 `字` 的那一处（移过去、按下、松开），再画一帧，交回那一帧。
fn 点左栏(ctx: &egui::Context, app: &mut App, 字: &str) -> egui::FullOutput {
    点左栏_窗口宽(ctx, app, headless::VIEWPORT[0], 字)
}

#[test]
fn 导航在左栏_三组五个入口照稿排好_顶栏上的换一份库不在了() {
    // 规格「测试决定」：导航在左栏且入口齐。设置屏还没有（票 31），新手引导主程序眼下没有：两样都不摆。
    let mut app = 待确认(&工作目录("左栏入口"));
    let ctx = headless::context();
    跑(&ctx, &mut app, 2);
    let out = 跑一帧(&ctx, &mut app, Vec::new());
    let 屏上 = 画出来的字(&out);

    let 库名 = app.site().display_name();
    let mut 上一项 = f32::NEG_INFINITY;
    for 字 in [
        库名.as_str(),
        "切换主库",
        "整理",
        "库",
        "待确认",
        "浏览",
        "输出",
        "子库",
        "后台",
        "任务",
    ] {
        let 在 = 左栏里的(&out, 字).unwrap_or_else(|| panic!("左栏里没有「{字}」：\n{屏上}"));
        assert!(在.center().y > 上一项, "「{字}」没排在上一项底下：{在:?}");
        上一项 = 在.center().y;
    }
    assert!(
        !屏上.contains("换一份库"),
        "顶栏那颗「换一份库」还在：\n{屏上}"
    );
    for 不摆 in ["设置", "新手引导"] {
        assert!(
            正好画在哪几处(&out, 不摆).is_empty(),
            "「{不摆}」不该摆出来：\n{屏上}"
        );
    }
}

#[test]
fn 点左栏入口就换到那一屏_屏头写着那一屏_右侧是它原来在顶栏上的那一段() {
    let mut app = 浏览(&工作目录("左栏切屏"));
    let ctx = headless::context();
    跑(&ctx, &mut app, 2);
    let 根数 = format!("{} 个根", app.roots().roots().len());
    for (view, 入口, 右侧那一段) in [
        (View::Library, "库", 根数.as_str()),
        (View::Queue, "待确认", "重新列队列"),
        (View::Browse, "浏览", "刮削选中…"),
        (View::Sublibraries, "子库", "重新列一遍"),
        // 任务屏照稿重排之后（票 `gui-looks-like-the-design/25`）右侧只放「清空历史」，顶栏那句摘要照稿不要了。
        (View::Tasks, "任务", "清空历史"),
    ] {
        let out = 点左栏(&ctx, &mut app, 入口);
        assert_eq!(app.view(), view, "点了左栏的「{入口}」");
        let 在屏头里 = |rect: &egui::Rect| rect.left() > 左栏宽() && rect.bottom() <= 屏头底();
        assert!(
            正好画在哪几处(&out, 入口).iter().any(在屏头里),
            "{view:?} 的屏头上没写「{入口}」：\n{}",
            画出来的字(&out),
        );
        // 右侧那一段摆不下时会折到屏头第二行（设计稿 `.scrhead` 的 `flex-wrap`）。
        let 右侧 = 画在哪几处(&out, &|画的| 画的.contains(右侧那一段));
        assert!(
            右侧
                .iter()
                .any(|rect| rect.left() > 左栏宽() && rect.bottom() <= 折成两行的屏头底()),
            "{view:?} 的屏头里没有它原来在顶栏上的「{右侧那一段}」（画在 {右侧:?}）：\n{}",
            画出来的字(&out),
        );
        // 子库屏的屏头右侧照稿只剩「重新列一遍」与「新建子库」（票 `gui-looks-like-the-design/20`，拿主意的人
        // 2026-09-14 选 B，挂单 `Q896`）：原来顶栏上的「N 台设备」拿掉了——左栏角标已经写着几台。
        if view == View::Sublibraries {
            assert!(
                正好画在哪几处(&out, "新建子库")
                    .iter()
                    .any(|rect| rect.left() > 左栏宽() && rect.bottom() <= 折成两行的屏头底()),
                "子库屏的屏头里没有「新建子库」：\n{}",
                画出来的字(&out),
            );
            // 只看屏头那一截、只认「数字 台设备」那种写法：空态卡上那句「子库是为一台设备（通常是掌机）……」与
            // 屏头副标题「……同步到各台设备」也带着这几个字。
            let 几台 = 画在哪几处(&out, &|画的| {
                画的.ends_with(" 台设备") && 画的.starts_with(|c: char| c.is_ascii_digit())
            });
            assert!(
                !几台
                    .iter()
                    .any(|rect| rect.left() > 左栏宽() && rect.bottom() <= 折成两行的屏头底()),
                "子库屏的屏头里还写着几台设备（画在 {几台:?}）：\n{}",
                画出来的字(&out),
            );
        }
    }
}

/// 这一帧画出来的每一段字与它画在哪儿，按画出来的次序。
fn 每一段(output: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
    fn 收(shape: &egui::Shape, out: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::Shape::Text(text) => out.push((
                text.galley.text().to_owned(),
                egui::Rect::from_min_size(text.pos, text.galley.size()),
            )),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|one| 收(one, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        收(&clipped.shape, &mut out);
    }
    out
}

/// 左栏里 `入口` 那一行右边画着的计数：与入口的字在同一行、在它右边、整段落在左栏以内。没画就是 `None`。
fn 左栏计数(output: &egui::FullOutput, 入口: &str) -> Option<String> {
    let 字 = 左栏里的(output, 入口)?;
    每一段(output)
        .into_iter()
        .find(|(_, 在)| {
            在.right() <= 左栏宽() && 在.left() > 字.right() && 字.y_range().contains(在.center().y)
        })
        .map(|(画的, _)| 画的)
}

/// 跑一帧，窗口宽 `宽`（高照旧 [`headless::VIEWPORT`]），带上这些事件。
fn 跑一帧_窗口宽(
    ctx: &egui::Context,
    app: &mut App,
    宽: f32,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut input = headless::input();
    input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(宽, headless::VIEWPORT[1]),
    ));
    input.events = events;
    headless::frame(ctx, input, |ui| app.ui(ui))
}

#[test]
fn 左栏的计数取各屏与核心库现成的那个数() {
    // 不另算一份（票 `gui-looks-like-the-design/32`）：待确认是队列里待裁决的条数——屏头右侧那一段
    // 「队列 N 条待裁决」说的是同一个数；库是根数；子库是子库屏列出来的个数；台上没活时任务不画数。
    let mut app = 待确认(&工作目录("左栏计数-待确认"));
    let ctx = headless::context();
    跑(&ctx, &mut app, 2);
    let out = 跑一帧(&ctx, &mut app, Vec::new());
    let 屏上 = 画出来的字(&out);
    let 待裁 = 左栏计数(&out, "待确认").unwrap_or_else(|| panic!("待确认那一项没画数：\n{屏上}"));
    assert!(
        屏上.contains(&format!("队列 {待裁} 条待裁决")),
        "左栏说 {待裁}，屏头右侧那一段说的不是这个数：\n{屏上}",
    );
    assert_eq!(
        左栏计数(&out, "库"),
        Some(format!("{} 个根", app.roots().roots().len()))
    );
    assert_eq!(
        左栏计数(&out, "子库"),
        Some(app.sublibrary().list().len().to_string())
    );
    assert_eq!(左栏计数(&out, "任务"), None, "台上没活时任务那一项不画数");

    // 浏览是作品数：与浏览屏没筛过时自己说的「N 个作品」是同一个数。
    let mut app = 浏览(&工作目录("左栏计数-浏览"));
    let ctx = headless::context();
    跑(&ctx, &mut app, 3);
    let out = 跑一帧(&ctx, &mut app, Vec::new());
    let 屏上 = 画出来的字(&out);
    let 作品 = 左栏计数(&out, "浏览").unwrap_or_else(|| panic!("浏览那一项没画数：\n{屏上}"));
    assert!(
        屏上.contains(&format!("{作品} 个作品；")),
        "左栏说 {作品} 个作品，浏览屏自己说的不是这个数：\n{屏上}",
    );

    // 一个根都还没扫过、库里一个作品都没有：浏览那一项画「—」（设计稿 `!S.scanDone?'—'`）——此刻的 0 说的是
    // 「还不知道」，不是「这份库有 0 个作品」。
    let mut app = shared::小库(&[], 工作目录("左栏计数-没扫过"));
    let ctx = headless::context();
    跑(&ctx, &mut app, 2);
    let out = 跑一帧(&ctx, &mut app, Vec::new());
    assert_eq!(
        左栏计数(&out, "浏览"),
        Some("—".to_owned()),
        "还没扫过的库，浏览那一项该画「—」：\n{}",
        画出来的字(&out),
    );
}

#[test]
fn 落下一批之后左栏的已保存裁决数与待确认数当场跟着变() {
    use romcat_core::triage::{Axis, Draft, Overrides};

    let mut app = 待确认(&工作目录("左栏裁决数"));
    let ctx = headless::context();
    跑(&ctx, &mut app, 2);
    let out = 跑一帧(&ctx, &mut app, Vec::new());
    assert!(
        左栏里的(&out, "已保存 0 条裁决").is_some(),
        "左栏底下没说已保存几条裁决：\n{}",
        画出来的字(&out),
    );
    let 原有 = app.queue().queue().pending();

    // 走 `tests/queue.rs` 那条「裁决即时写进沉淀库并从队列移除」的路：点一个汉化组记号，手工指定作品，落下。
    let (screen, _) = app.queue_and_site();
    screen.pick(Axis::NameMark, "ACG汉化组");
    跑(&ctx, &mut app, 1);
    let draft = Draft {
        work: Some("勇者斗恶龙".to_string()),
        overrides: Overrides {
            team: Some("ACG汉化组".to_string()),
            ..Overrides::default()
        },
        ..Draft::default()
    };
    let (screen, site) = app.queue_and_site();
    screen.preview(site, &draft);
    screen.commit(site);
    assert!(screen.error().is_none(), "{:?}", screen.error());
    跑(&ctx, &mut app, 1);
    let out = 跑一帧(&ctx, &mut app, Vec::new());
    assert!(
        左栏里的(&out, "已保存 129 条裁决").is_some(),
        "真机上 [ACG汉化组] 带 129 条，落下之后左栏该说已保存 129 条：\n{}",
        画出来的字(&out),
    );
    assert_eq!(
        左栏计数(&out, "待确认"),
        Some(romcat_core::report::thousands(原有 - 129)),
        "落下之后待确认那一项该少 129 条",
    );
}

#[test]
fn 台上有活时任务那一项带着强调色圆点与跑着加排着的数() {
    use shared::占位活;

    let mut app = 待确认(&工作目录("左栏任务"));
    let ctx = headless::context();
    跑(&ctx, &mut app, 1);
    let 跑着的 = 占位活::排上(app.tasks_mut(), "装作在扫一趟库");
    跑(&ctx, &mut app, 1);
    let out = 跑一帧(&ctx, &mut app, Vec::new());
    assert_eq!(左栏计数(&out, "任务"), Some("1".to_owned()));
    let 任务那一行 = 左栏里的(&out, "任务").expect("左栏里有任务");
    let 点 = romcat_gui::tokens::Tokens::builtin().layout.rail_dot;
    let 有圆点 = out.shapes.iter().any(|clipped| match &clipped.shape {
        egui::Shape::Circle(circle) => {
            circle.center.x < 左栏宽()
                && 任务那一行.y_range().contains(circle.center.y)
                && (circle.radius - 点 / 2.0).abs() < 0.01
        }
        _ => false,
    });
    assert!(有圆点, "任务那一项的数前面没有那枚圆点");

    let 排着的 = 占位活::排上(app.tasks_mut(), "装作在排着队");
    跑(&ctx, &mut app, 1);
    let out = 跑一帧(&ctx, &mut app, Vec::new());
    assert_eq!(
        左栏计数(&out, "任务"),
        Some("2".to_owned()),
        "数的是跑着的加排着的"
    );
    排着的.按停(app.tasks_mut());
    跑着的.按停(app.tasks_mut());
}

#[test]
fn 收起左栏变成窄条_关掉再打开还是收着的_展开回去那一行就去掉() {
    let 目录 = 工作目录("左栏收起");
    let 窄条宽 = romcat_gui::tokens::Tokens::builtin().layout.rail_collapsed;
    let mut app = 待确认(&目录);
    let ctx = headless::context();
    跑(&ctx, &mut app, 2);
    let out = 点左栏(&ctx, &mut app, "« 收起");
    let 屏上 = 画出来的字(&out);
    assert!(
        左栏里的(&out, "»").is_some_and(|在| 在.right() <= 窄条宽),
        "收起之后窄条底下该是「»」：\n{屏上}"
    );
    for 入口 in ["库", "待确认", "浏览", "子库", "任务"] {
        assert!(
            正好画在哪几处(&out, 入口)
                .iter()
                .any(|在| 在.right() <= 窄条宽),
            "窄条里没有「{入口}」：\n{屏上}",
        );
    }
    for 不在窄条里 in ["切换主库", "已保存"] {
        assert!(
            !屏上.contains(不在窄条里),
            "窄条里不该有「{不在窄条里}」：\n{屏上}"
        );
    }
    let 文件 = std::fs::read_to_string(app.layout().path()).expect("收起之后该落盘");
    assert!(文件.contains("左栏 = 收起"), "{文件}");

    drop(app);
    let mut 再开 = 待确认(&目录);
    let ctx = headless::context();
    跑(&ctx, &mut 再开, 2);
    let out = 跑一帧(&ctx, &mut 再开, Vec::new());
    assert!(
        左栏里的(&out, "»").is_some(),
        "关掉再打开左栏没收着：\n{}",
        画出来的字(&out)
    );
    let out = 点左栏(&ctx, &mut 再开, "»");
    assert!(
        左栏里的(&out, "切换主库").is_some(),
        "展开回去没有切换主库那张卡：\n{}",
        画出来的字(&out)
    );
    let 文件 = std::fs::read_to_string(再开.layout().path()).expect("那份文件还在");
    assert!(!文件.contains("左栏"), "展开回去那一行该去掉：{文件}");
}

#[test]
fn 窗口窄于门槛左栏自动收起_宽回来照人选的_自动收起不写进文件() {
    // 拿主意的人 2026-09-14 定：窗口宽不到 `rail-collapse-below` 时自动收成窄条，宽回来恢复人自己选的；
    // 人手动收起、展开记在版式文件里，自动收起只看当下窗口宽，不写进文件。
    let 门槛 = romcat_gui::tokens::Tokens::builtin()
        .layout
        .rail_collapse_below;
    let (窄, 宽) = (门槛 - 40.0, headless::VIEWPORT[0]);
    let 目录 = 工作目录("左栏自动收起");
    let mut app = 待确认(&目录);
    let ctx = headless::context();
    跑一帧_窗口宽(&ctx, &mut app, 窄, Vec::new());
    let out = 跑一帧_窗口宽(&ctx, &mut app, 窄, Vec::new());
    assert!(
        左栏里的(&out, "»").is_some() && !画出来的字(&out).contains("切换主库"),
        "{窄} 宽的窗口里左栏该自动收着：\n{}",
        画出来的字(&out),
    );

    跑一帧_窗口宽(&ctx, &mut app, 宽, Vec::new());
    let out = 跑一帧_窗口宽(&ctx, &mut app, 宽, Vec::new());
    assert!(
        左栏里的(&out, "切换主库").is_some(),
        "人没收起过，宽回来该照旧展开：\n{}",
        画出来的字(&out),
    );
    let 记着的 = std::fs::read_to_string(app.layout().path()).unwrap_or_default();
    assert!(!记着的.contains("左栏"), "自动收起不该写进文件：{记着的}");

    // 人收起了：窗口窄了再宽回来，还是收着——自动收起没把人选的那一份覆盖掉。
    点左栏(&ctx, &mut app, "« 收起");
    跑一帧_窗口宽(&ctx, &mut app, 窄, Vec::new());
    跑一帧_窗口宽(&ctx, &mut app, 宽, Vec::new());
    let out = 跑一帧_窗口宽(&ctx, &mut app, 宽, Vec::new());
    assert!(
        左栏里的(&out, "»").is_some() && !画出来的字(&out).contains("切换主库"),
        "人收起的，窗口窄了再宽回来还该收着：\n{}",
        画出来的字(&out),
    );
    let 记着的 = std::fs::read_to_string(app.layout().path()).expect("人收起之后该落盘");
    assert!(记着的.contains("左栏 = 收起"), "{记着的}");
}

/// 同 [`点左栏`]，只是窗口宽 `宽`。
fn 点左栏_窗口宽(
    ctx: &egui::Context, app: &mut App, 宽: f32, 字: &str
) -> egui::FullOutput {
    let 头一帧 = 跑一帧_窗口宽(ctx, app, 宽, Vec::new());
    let Some(在) = 左栏里的(&头一帧, 字) else {
        panic!("左栏里没有「{字}」，没处点：\n{}", 画出来的字(&头一帧));
    };
    let 按 = |pressed: bool| egui::Event::PointerButton {
        pos: 在.center(),
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    跑一帧_窗口宽(
        ctx,
        app,
        宽,
        vec![egui::Event::PointerMoved(在.center()), 按(true)],
    );
    跑一帧_窗口宽(ctx, app, 宽, vec![按(false)]);
    跑一帧_窗口宽(ctx, app, 宽, Vec::new())
}

#[test]
fn 窗口窄时点左栏底下那颗临时展开_不写文件_窗口宽度一变又收回去() {
    // 拿主意的人 2026-09-14 定（挂单 `Q867`）：窗口窄于门槛、左栏自动收着时，点「»」临时展开，不写进版式文件；
    // 临时展开时再点「« 收起」收回去；窗口宽度一变，就回到自动收起的规则。
    let 门槛 = romcat_gui::tokens::Tokens::builtin()
        .layout
        .rail_collapse_below;
    let 窄 = 门槛 - 40.0;
    let mut app = 待确认(&工作目录("左栏临时展开"));
    let ctx = headless::context();
    let 记着的 = |app: &App| std::fs::read_to_string(app.layout().path()).unwrap_or_default();
    跑一帧_窗口宽(&ctx, &mut app, 窄, Vec::new());

    let out = 点左栏_窗口宽(&ctx, &mut app, 窄, "»");
    assert!(
        左栏里的(&out, "切换主库").is_some(),
        "窄窗口里点「»」该临时展开：\n{}",
        画出来的字(&out),
    );
    assert!(
        !记着的(&app).contains("左栏"),
        "临时展开不该写进文件：{}",
        记着的(&app)
    );

    let out = 点左栏_窗口宽(&ctx, &mut app, 窄, "« 收起");
    assert!(
        左栏里的(&out, "»").is_some() && !画出来的字(&out).contains("切换主库"),
        "临时展开时点「« 收起」该收回去：\n{}",
        画出来的字(&out),
    );
    assert!(
        !记着的(&app).contains("左栏"),
        "收回去也不该写进文件：{}",
        记着的(&app)
    );

    点左栏_窗口宽(&ctx, &mut app, 窄, "»");
    跑一帧_窗口宽(&ctx, &mut app, 窄 + 10.0, Vec::new());
    let out = 跑一帧_窗口宽(&ctx, &mut app, 窄 + 10.0, Vec::new());
    assert!(
        左栏里的(&out, "»").is_some() && !画出来的字(&out).contains("切换主库"),
        "临时展开之后窗口宽度一变，该回到自动收起：\n{}",
        画出来的字(&out),
    );
    assert!(
        !记着的(&app).contains("左栏"),
        "文件一直没被动过：{}",
        记着的(&app)
    );
}

#[test]
fn 收起窄条里的入口照稿按行高撑高() {
    // 设计稿 `.main.rcol .nav{padding:7px 0;gap:1px;font-size:12px}` 与 `.main.rcol .nav .badge{font-size:10px}`，
    // 行高继承 `body` 的 1.55（令牌 `line-height`）：一项高 = 上下留白 + 字号 × 行高 + 间距 + 计数字号 × 行高。
    // 量相邻两项（库、待确认，都带计数）的字相差多少：该是一项高再加栏里一格间距。
    let t = romcat_gui::tokens::Tokens::builtin();
    let mut app = 待确认(&工作目录("窄条行高"));
    let ctx = headless::context();
    跑(&ctx, &mut app, 2);
    let out = 点左栏(&ctx, &mut app, "« 收起");
    let 库 = 左栏里的(&out, "库").expect("窄条里有库");
    let 待确认那一项 = 左栏里的(&out, "待确认").expect("窄条里有待确认");
    let 一项 = 2.0 * t.space.nav_padding_collapsed
        + t.font.size_small * t.font.line_height
        + t.space.nav_gap_collapsed
        + t.font.size_badge_narrow * t.font.line_height;
    let 差 = 待确认那一项.top() - 库.top();
    assert!(
        (差 - (一项 + t.space.rail_gap)).abs() < 0.5,
        "窄条里相邻两项隔 {差}，照稿该是 {}：库 {库:?}，待确认 {待确认那一项:?}",
        一项 + t.space.rail_gap,
    );
}
