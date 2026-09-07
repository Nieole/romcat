//! **版式可调与视觉一致性**（票 `gui-redesign/12`）。
//!
//! 这几条断言不看代码长什么样，看的是**跑出来的结果**：真的往那条边界上按下去、拖过去、
//! 松手，然后问 egui 那块面板现在多宽、问文件系统那个数落在哪儿、问这一帧到底画出了
//! 哪几个字。
//!
//! - **拖得动**：三屏七条边界，每一条按住往外拖 60 点就宽 60 点。
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

/// 合成数据的规模。真库是 46,483 个变体（`docs/library-facts.md`），照它来。
const ROWS: u64 = 46_483;

/// 待确认那一屏的规模。真机上一万八千多条（票 08 实测 16,656）。
const QUEUE_ROWS: u64 = 16_656;

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

/// 把这一屏摆成「七条边界都在屏上」的样子。
fn 摆开(app: &mut App, screen: View) {
    app.show_view(screen);
    match screen {
        // 刮削面板摊开才有那条边界，而**一行都没勾就不摊开**（那时它会摆出一块
        // 「作用于 0 个变体」的面板，人按下去只会对着一份什么都没干的报告发愣）。
        View::Browse => {
            app.browse_and_site().0.picked_mut().select_all();
            let (browse, site) = app.browse_and_site();
            browse.open_scrape(&site.catalog);
            assert!(browse.scrape().is_open(), "{:?}", browse.error());
        }
        // 逐条那一路才有左栏与裁决面板；分批那一路整屏就是一列卡片。
        View::Queue => app.queue_and_site().0.show_one_by_one(),
        _ => {}
    }
}

#[test]
fn 三屏的面板边界都拖得动() {
    // 验收第 1 条。`egui::Panel::bottom` **默认是拖不动的**，这一票之前那三块底栏
    // （浏览编辑、裁决面板、配目标）正是这么钉死的；而光打开 `resizable` 还不够——
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
fn 刮削面板摊开时两块底栏一起拖到底也挤不没正中那块() {
    // 验收第 3 条最坏的那一路：**同一维上两块都在**。光按「整个窗口的几成」算，
    // 两块各吃四成、再扣掉顶栏，正中那张表在最小窗口上只剩七十来点（表头 24 加两行）。
    // 兜底的是 [`layout::Boundary::cap`] 里第二道——「眼下还剩多少减去 `FLOOR`」。
    let mut app = 浏览(&工作目录("两块底栏"));
    摆开(&mut app, View::Browse);
    let ctx = headless::context();
    跑(&ctx, &mut app, 3);
    for boundary in [layout::SCRAPE, layout::EDIT] {
        拖到(&ctx, &mut app, boundary, 5000.0);
    }
    let 正中高 = egui::Vec2::from(headless::VIEWPORT).y
        - 多宽(&ctx, layout::SCRAPE)
        - 多宽(&ctx, layout::EDIT);
    assert!(
        正中高 >= layout::FLOOR,
        "两块底栏都拖到底之后，正中那张表只剩 {正中高} 点高",
    );
    // 表还在：塌没了的话表头一个字都画不出来。
    let 屏上 = 画出来的字(&跑一帧(&ctx, &mut app, Vec::new()));
    assert!(屏上.contains("作品"), "正中那张表的表头没了：\n{屏上}");
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
