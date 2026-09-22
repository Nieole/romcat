//! **截图门**：界面观感的像素层（规格 `.scratch/gui-looks-like-the-design/spec.md`「三层门」第三层）。
//!
//! 结构与文案那一层读的是这一帧**画出来的字**（`shared::画出来的字`），读不出间距、颜色、圆角与
//! 对齐——「每张票都验收通过了，成品还是难看」正是漏在那儿。这一层把一屏真的渲染成 PNG
//! （`egui_kittest` 的 `snapshot` + `wgpu`），与入库的**基线**逐像素比：谁挪了一个像素，这里就红。
//!
//! ## 基线从哪来
//!
//! 第一版由拿主意的人对着设计稿点头才入库；之后改基线必须显式提交，差异摆在提交里。基线住在
//! `tests/snapshots/<屏>/<那一态>-<主题>.png`，对不上时旁边落一张 `.new.png` 与一张 `.diff.png`。
//! 重批：`UPDATE_SNAPSHOTS=1 cargo test -p romcat-gui --all-features --test snapshot`。
//! **什么时候该重批、什么时候是回归**，写在票 `gui-looks-like-the-design/05` 里。
//!
//! ## 出基线的机器、阈值、CI 上为什么跳过
//!
//! 拿主意的人 2026-09-14 定（票 `gui-looks-like-the-design/05`）：**基线出自维护者本机**（Apple M1，
//! wgpu 走 Metal），阈值取 `egui_kittest` 的缺省（[`比对`]）。本机重复出图逐像素一致；变异实测把主库那一行里
//! 名字与说明的间距挪 2 点（`catalog-row-gap` 2→4），有库那两张各有一万四千多个像素对不上，缺省阈值抓得住。
//!
//! **CI 上如实跳过**（[`该跳过`]）：GitHub Actions 的 runner 上没有能用的显卡，wgpu 找不到适配器，
//! 这几条会当场炸成红——而那不是界面的错。像素层由**合并之前在本机跑的门禁**守。egui 自己的 CI
//! 也只在带显卡的 macOS runner 上跑截图测试。
//!
//! ## 喂的是合成数据，视口定死
//!
//! 屏上画着的每一样都得是定值：工作目录的路径、主库原名、变体数、上次扫描时刻。从真盘上列的话，
//! 临时目录每一趟都是另一串，像素跟着变。所以开场那几态走 [`Screen::listed`]，把核心库那个类型
//! （[`Listing`]）直接交进去。视口是 [`headless::VIEWPORT`]，一点一个像素。**例外眼下有两对**，都是 1280×960
//! （[`开一扇`]），都因为「那一屏要拍全的东西 800 高装不下」：
//!
//! - 子库屏超限那两张（`sublibrary/over-capacity-*`）——整张卡连删减表底下的灰框与按钮都要拍全
//!   （拿主意的人 2026-09-14 定，挂单 `Q895`）。
//! - 待确认屏下钻那两张（`queue/drill-*`）——就地那一框把底下那一排顶出了 800，而那一排上
//!   「通过剩余的 N 条」正是票 `gui-looks-like-the-design/19` 第 4 条验收的原话（票 19 加，照前一对的先例）。
//!
//! 两对都把**「看得全」写成断言**（[`拍超限`]、[`拍下钻`]）：该在画面里的每一样都得整个在视口内，
//! 哪天那一屏长高把它们挤出去，这里当场红，不会悄悄拍一张截掉半截的基线。
//!
//! **浅色与暗色各拍一张**：两套主题各取令牌里的一套，只拍一套的话，另一套颜色接错了没人看得见。
//!
//! ## 往后每一屏加一张
//!
//! 一张基线是一个 `#[test]`：搭好那一屏、调一次 [`拍`]，名字写成 `<屏>/<那一态>-<主题>`。要先点一下
//! 再拍的（弹层），用 [`开一个`] + [`按`]，再 [`拍下`]。
//!
//! ## 主窗口外壳
//!
//! 左栏与屏头（票 `gui-looks-like-the-design/32`）拍的是整扇主窗口，垫在**任务屏的空台态**上（拿主意的人 2026-09-14 定：
//! main 上各屏眼下都是旧正文，任务屏空台东西最少，最看得清左栏与屏头），喂的是
//! 测试手搭的那份小库（`shared::小库`）：主库原名、沉淀库在哪、队列那几批的样本都是定值。**正文那一块
//! 归各屏自己的票**，它们照稿重排时这几张跟着重批。

use std::path::{Path, PathBuf};
#[cfg(feature = "demo")]
use std::time::Duration;

use egui::Theme;
use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions};
use romcat_core::catalog::browse::PlatformFilter;
use romcat_core::catalog::identify::{Candidate, Identification, Provenance, Tier};
use romcat_core::catalog::roots::{self, LibraryRoot, RootScan};
use romcat_core::catalog::scrape::{Harvested, HarvestedMedia, HarvestedValue};
use romcat_core::catalog::{Catalog, CatalogError, Confidence, SCHEMA_VERSION, State};
use romcat_core::dat::Convention;
use romcat_core::fs::RealFs;
use romcat_core::platform::Manifest;
use romcat_core::scan::{self, Jobs, ScanOptions};
use romcat_core::scrape::measure::Measured;
use romcat_core::scrape::{AnchorKind, Field, MediaKind};
use romcat_core::shape::{SINGLE_FILE_RULE, Variant};
use romcat_core::site::Site;
use romcat_core::sublibrary::{Exception, Rule, Sublibrary};
#[cfg(feature = "demo")]
use romcat_core::task::Cutoff;
use romcat_core::task::Handle;
use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::verdict::Store;
use romcat_core::workspace::{CatalogEntry, CatalogFacts, CatalogState, DirUnreadable, Listing};
use romcat_gui::app::{App, View};
use romcat_gui::browse::work::Tab;
#[cfg(feature = "demo")]
use romcat_gui::demo;
use romcat_gui::layout::{FOLD_EXPORT, FOLD_ROOTS, FOLD_SOURCES};
use romcat_gui::opening::Screen;
use romcat_gui::roots::RootRow;
use romcat_gui::settings::Section;
#[cfg(feature = "demo")]
use romcat_gui::task::{Clock, Product};
use romcat_gui::{font, headless, layout, look, rail};

mod shared;
// **`等任务台空了` 不带门**：它只问任务台忙不忙（`App::poll_tasks` / `App::tasks`），与合成数据
// 无关，而**不带 `demo` 也要编的那几张**要它——库屏那几个夹具开窗之前都先体检一趟
// （`先体检一趟`）。从前它搭着旁边两个的顺风车写在同一行 `#[cfg(feature = "demo")]` 上，
// 于是 `cargo check --all-targets`（不带 `--all-features`）当场红，而门禁一步都不跑这个组合
// （挂单 `Q1081` / `Q1082`）。
use shared::等任务台空了;
// 这两个是占位活那一路的，只有 `demo` 开着时才有测试用得上。
#[cfg(feature = "demo")]
use shared::{一对信号, 占位活};

/// 比对阈值：一个像素的色差过了多少算坏（每像素 YIQ 色距 0.6）、坏几个像素算红（0 个）。
///
/// 就是 `egui_kittest` 的缺省，拿主意的人 2026-09-14 定：基线与比对都在本机（Apple M1 · Metal）上，
/// 本机重复出图逐像素一致；变异实测挪 2 点间距（`catalog-row-gap` 2→4），从 0 到 10 每一档阈值上都是一万四千多个像素（票 `gui-looks-like-the-design/05`）。
fn 比对() -> SnapshotOptions {
    SnapshotOptions::new()
}

/// **CI 上跳过这一张**，并印一行说清为什么、在哪儿守。交回跳不跳。
///
/// 认的是 GitHub Actions 自己设的 `GITHUB_ACTIONS=true`。**不按「找不找得到显卡」跳**：本机哪天
/// 显卡驱动坏了，这几条该红给人看，而不是悄悄变绿。
fn 该跳过(名字: &str) -> bool {
    if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true") {
        return false;
    }
    println!(
        "跳过截图门 {名字}：GitHub Actions 的 runner 上没有能用的显卡，渲不出图；基线出自维护者本机，\
         像素层由合并之前在本机跑的门禁守（票 gui-looks-like-the-design/05）"
    );
    true
}

/// 装上界面自己的字体与观感基线，**一个窗口只装一次**。交回**这一帧画不画得了**。
///
/// `Harness` 自己造 `egui::Context`，不走 [`headless::context`]，而且造好当场就跑头一帧。字体在
/// 那一帧里装、下一帧才生效——那一帧就去画的话，粗体族还没绑上字体，egui 当场 panic。所以头一帧
/// 只装不画，并要一次重画。
///
/// 观感基线整套换掉 `Visuals` 时会把 `Harness` 关掉的光标闪烁带回来——闪烁的光标每帧都要重画，
/// 这一屏就永远跑不稳，所以装完再关一次。
fn 装好(ctx: &egui::Context) -> bool {
    let 装过 = egui::Id::new("截图门装过字体与观感基线");
    if ctx
        .data(|data| data.get_temp::<bool>(装过))
        .unwrap_or(false)
    {
        return true;
    }
    ctx.data_mut(|data| data.insert_temp(装过, true));
    font::install(ctx);
    // 走 `install_once`：主窗口那一路自己的第一帧还会再问一遍（`App::ui`），问到装过就不再装——
    // 再装一遍会把底下刚关掉的光标闪烁带回来。
    look::install_once(ctx);
    ctx.all_styles_mut(|style| style.visuals.text_cursor.blink = false);
    ctx.request_repaint();
    false
}

/// 搭一扇窗：视口 [`headless::VIEWPORT`]、一点一个像素、这一套主题，先跑两帧。
///
/// 台上有活在跑的那几张用它自己数帧（[`拍正在跑`]）：台上有活时主窗口每一帧都请求下一帧，
/// 永远跑不到「不要重画」。别的一律走 [`开一个`]。
fn 搭一个<'a>(主题: Theme, 画一帧: impl FnMut(&mut egui::Ui) + 'a) -> Harness<'a> {
    搭一扇(主题, headless::VIEWPORT, 画一帧)
}

/// 同 [`搭一个`]，画面多大由调用方给（点）。**只给模块文档「视口定死」那一节写着的例外用。**
fn 搭一扇<'a>(
    主题: Theme,
    画面: [f32; 2],
    mut 画一帧: impl FnMut(&mut egui::Ui) + 'a,
) -> Harness<'a> {
    let mut harness = Harness::builder()
        .with_size(画面)
        .with_pixels_per_point(1.0)
        .with_theme(主题)
        .wgpu()
        .build_ui(move |ui| {
            if !装好(ui.ctx()) {
                return;
            }
            // **铺满整个窗口画**：`Harness` 把这个闭包包在一层外边距 8 点的面板框里
            // （`egui_kittest` 的 `AppKind::run_ui`），而开窗那一路（`eframe`）交给程序的是不带边距的
            // 根 `ui`。不跳出来的话，基线四周一圈 8 点是透明的——那不是程序的样子。
            let 整个窗口 = ui.ctx().content_rect();
            ui.scope_builder(egui::UiBuilder::new().max_rect(整个窗口), |ui| {
                ui.set_clip_rect(整个窗口);
                画一帧(ui);
            });
        });
    // 头一帧只装字体（见 [`装好`]），弹层这类浮层画出来的第二帧才摆稳：先跑两帧。跑到不要重画
    // 为止是 [`开一个`] 的事（跑不稳时 `run` 当场炸，并说清是谁一直在要重画）。
    harness.run_steps(2);
    harness
}

/// 开一扇窗（[`搭一个`]），跑到这一屏不再要重画为止。
fn 开一个<'a>(主题: Theme, 画一帧: impl FnMut(&mut egui::Ui) + 'a) -> Harness<'a> {
    let mut harness = 搭一个(主题, 画一帧);
    harness.run();
    harness
}

/// 同 [`开一个`]，画面多大由调用方给（点，[`搭一扇`]）。**只给模块文档「视口定死」那一节写着的例外用。**
fn 开一扇<'a>(
    主题: Theme,
    画面: [f32; 2],
    画一帧: impl FnMut(&mut egui::Ui) + 'a,
) -> Harness<'a> {
    let mut harness = 搭一扇(主题, 画面, 画一帧);
    harness.run();
    harness
}

/// 按一下屏上**正好**写着 `那几个字`、最后画出来的那一处，再把指针挪走、跑到不要重画为止。
///
/// **指针要挪走**：`Harness` 出图时会在指针所在处画一枚指针三角，悬停也会换按钮的底色——
/// 基线里不该有这两样。**跑满帧**：弹层打开时要淡入，没跑完就拍，颜色只有一半深。
fn 按(harness: &mut Harness<'_>, 那几个字: &str) {
    let Some(在) = 最后一处正好画着(harness.output(), 那几个字) else {
        panic!("屏上没有正好写着「{那几个字}」的地方，没处按");
    };
    let 键 = |pressed: bool| egui::Event::PointerButton {
        pos: 在,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    harness.event(egui::Event::PointerMoved(在));
    harness.event(键(true));
    harness.event(键(false));
    harness.event(egui::Event::PointerGone);
    harness.step();
    harness.run_steps(5);
    harness.run();
}

/// 按一下屏上**正好**写着 `那几个字`、**头一处**画出来的地方（其余同 [`按`]）。
///
/// 同一句话屏上摆着好几处、要按的是**上面**那一处时用它——成型存疑那一层里一行一颗「处理…」。
fn 按头一处(harness: &mut Harness<'_>, 那几个字: &str) {
    let Some(在) = 正好画着的每一处(harness.output(), 那几个字)
        .first()
        .map(egui::Rect::center)
    else {
        panic!("屏上没有正好写着「{那几个字}」的地方，没处按");
    };
    let 键 = |pressed: bool| egui::Event::PointerButton {
        pos: 在,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    harness.event(egui::Event::PointerMoved(在));
    harness.event(键(true));
    harness.event(键(false));
    harness.event(egui::Event::PointerGone);
    harness.step();
    harness.run_steps(5);
    harness.run();
}

/// 屏上**正好**写着 `那几个字`、按画出来的次序**最后**那一处的中心点。
fn 最后一处正好画着(output: &egui::FullOutput, 那几个字: &str) -> Option<egui::Pos2> {
    正好画着的每一处(output, 那几个字)
        .last()
        .map(|rect| rect.center())
}

/// 屏上**正好**写着 `那几个字` 的每一处画在哪儿，按画出来的次序。
///
/// egui 不画整个落在裁剪区外的控件，所以一处都没画出来的就不在里头。
fn 正好画着的每一处(output: &egui::FullOutput, 那几个字: &str) -> Vec<egui::Rect> {
    fn 找(shape: &egui::epaint::Shape, 那几个字: &str, 每一处: &mut Vec<egui::Rect>) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                if text.galley.text() == 那几个字 {
                    每一处.push(egui::Rect::from_min_size(text.pos, text.galley.size()));
                }
            }
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    找(one, 那几个字, 每一处);
                }
            }
            _ => {}
        }
    }
    let mut 每一处 = Vec::new();
    for clipped in &output.shapes {
        找(&clipped.shape, 那几个字, &mut 每一处);
    }
    每一处
}

/// 屏上**含有** `那一段` 的每一段画在哪儿（整段的外框，折了行也算在里头），按画出来的次序。
///
/// [`正好画着的每一处`] 认的是**整段一字不差**，而「会怎样」那几条是
/// `look::impact` 把几截拼成的**一个** `LayoutJob`、还会折行——按整段去认认不出来，
/// 所以另有这一支。要断言「这一段看得全」时拿它交回的那个框去比视口。
fn 画着的每一处含(output: &egui::FullOutput, 那一段: &str) -> Vec<egui::Rect> {
    fn 找(shape: &egui::epaint::Shape, 那一段: &str, 每一处: &mut Vec<egui::Rect>) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                if text.galley.text().contains(那一段) {
                    每一处.push(egui::Rect::from_min_size(text.pos, text.galley.size()));
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
    每一处
}

/// **弹层里**认得下的每一段画在哪儿，按画出来的次序。
///
/// 为什么要把弹层切出来：弹层与它底下那一屏画在同一张图上，而「4.00 MiB」「1995」这种字
/// 两边都有（底下那张主列表的容量、年份那两列也这么写）。按整段的字去认分不开，按画出来的
/// 次序也分不开。**按裁剪框分得开**：弹层里每一段的裁剪框横着铺满这一层（左右两边都贴着
/// 这一层的边），而底下那一屏的每一块各自被自己那一栏夹着——正中那一栏起在四百多点上，
/// 够不着弹层的左边。
///
/// [`正好画着的每一处`] 与 [`画着的每一处含`] 认的是整段或子串；一列里那几段的字各不相同时
/// （「保留作品自带」与「来自「某某」」）只能按这一支挑。
fn 弹层里的每一段<'a>(
    output: &egui::FullOutput,
    画面: [f32; 2],
    认: impl Fn(&str) -> bool + 'a,
) -> Vec<egui::Rect> {
    fn 找(
        shape: &egui::epaint::Shape, 认: &dyn Fn(&str) -> bool, 每一处: &mut Vec<egui::Rect>
    ) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                if 认(text.galley.text()) {
                    每一处.push(egui::Rect::from_min_size(text.pos, text.galley.size()));
                }
            }
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    找(one, 认, 每一处);
                }
            }
            _ => {}
        }
    }
    // 这一层多宽、摆在哪儿：共用弹层那一层自己说了算（居中、宽取令牌里最宽那一档）。
    let 宽 = romcat_gui::dialog::Width::Widest.points();
    let (左, 右) = ((画面[0] - 宽) / 2.0, (画面[0] + 宽) / 2.0);
    let mut 每一处 = Vec::new();
    for clipped in &output.shapes {
        if clipped.clip_rect.left() > 左 + 4.0 || clipped.clip_rect.right() < 右 - 4.0 {
            continue;
        }
        找(&clipped.shape, &认, &mut 每一处);
    }
    每一处
}

/// 屏上那几道**横线**画在哪儿（`painter.hline` 留下的线段），只看竖直位置与 `只看这一行`
/// 差不到几点的那几道——一屏上横线多得很，「走到第几问」那一排只占一行。
fn 横线们(
    output: &egui::FullOutput, 只看这一行: f32
) -> Vec<std::ops::RangeInclusive<f32>> {
    fn 找(
        shape: &egui::epaint::Shape,
        只看这一行: f32,
        每一道: &mut Vec<std::ops::RangeInclusive<f32>>,
    ) {
        match shape {
            egui::epaint::Shape::LineSegment { points, .. } => {
                let [甲, 乙] = points;
                if (甲.y - 乙.y).abs() < 0.5 && (甲.y - 只看这一行).abs() < 4.0 {
                    每一道.push(甲.x.min(乙.x)..=甲.x.max(乙.x));
                }
            }
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    找(one, 只看这一行, 每一道);
                }
            }
            _ => {}
        }
    }
    let mut 每一道 = Vec::new();
    for clipped in &output.shapes {
        找(&clipped.shape, 只看这一行, &mut 每一道);
    }
    每一道
}

/// **一列里那几段的左缘对得齐吗**：全等才算齐（差半个点都不算）。
///
/// 对齐是这几张图的**验收**，所以写成断言而不是只靠基线：基线只说「与上次一样」，
/// 而上次也可能是歪的。比的是**这几段画在哪儿**（`Galley` 的位置），不是像素。
#[track_caller]
fn 一列上对得齐(每一处: &[egui::Rect], 该有几段: usize, 哪一列: &str) {
    assert_eq!(
        每一处.len(),
        该有几段,
        "{哪一列}该有 {该有几段} 段，屏上画出了 {} 段：{每一处:?}",
        每一处.len(),
    );
    let 左缘: Vec<f32> = 每一处.iter().map(|rect| rect.left()).collect();
    assert!(
        左缘.windows(2).all(|两个| (两个[0] - 两个[1]).abs() < 0.5),
        "{哪一列}没立成一列，几段的左缘是 {左缘:?}",
    );
}

/// 把这扇窗此刻的样子与 `tests/snapshots/<名字>.png` 比。对不上时当场红。
#[track_caller]
fn 拍下(mut harness: Harness<'_>, 名字: &str) {
    harness.snapshot_options(名字, &比对());
}

/// 开一扇窗、拍一张。CI 上跳过（[`该跳过`]）。
#[track_caller]
fn 拍<'a>(名字: &str, 主题: Theme, 画一帧: impl FnMut(&mut egui::Ui) + 'a) {
    if 该跳过(名字) {
        return;
    }
    拍下(开一个(主题, 画一帧), 名字);
}

// ——— 开场 ———

/// 基线里画着的工作目录：设计稿上那一串。**这个目录不存在，也不会被碰**——
/// [`Screen::listed`] 不去盘上列。
fn 工作目录() -> PathBuf {
    PathBuf::from("~/.local/share/romcat")
}

/// 一份开得了的库：主库原名、变体数、上次扫描时刻（UNIX 纪元起的秒，屏上按 UTC 画）。
fn 开得了(
    工作目录: &Path, 原名: &str, 主库标识: &str, 变体: u64, 扫于: i64
) -> CatalogEntry {
    CatalogEntry {
        path: 工作目录.join("catalog").join(format!("{主库标识}.sqlite3")),
        name: 原名.to_owned(),
        state: CatalogState::Openable(Ok(CatalogFacts {
            variants: 变体,
            scanned_at: Some(扫于),
        })),
    }
}

/// **有库**：两份开得了的，一份结构版本对不上的。规模照真库（46,444 个变体，
/// `docs/library-facts.md`）。次序照核心库排好的交（上次扫描倒排，说不上来的垫底）。
fn 有库(工作目录: &Path) -> Listing {
    let 旧库 = 工作目录
        .join("catalog")
        .join("旧库-0824-5e0b1c2d3a4f6978.sqlite3");
    // 那句话是**核心库的原话**，这里拿核心库那个错误类型折出来，不抄一段字。
    let 原话 = CatalogError::Version {
        path: romcat_core::path::display(&旧库),
        found: 4,
        expected: SCHEMA_VERSION,
    }
    .to_string();
    Listing::Catalogs(vec![
        // 2026-09-03 14:58（UTC）
        开得了(
            工作目录,
            "主库",
            "主库-f5c61109a2b3c4d5",
            46_444,
            1_788_447_480,
        ),
        // 2026-08-29 21:10（UTC）
        开得了(
            工作目录,
            "掌机整理",
            "掌机整理-0c9e7d1b24a6f358",
            3_102,
            1_788_037_800,
        ),
        CatalogEntry {
            path: 旧库,
            name: "旧库-0824".to_owned(),
            state: CatalogState::SchemaMismatch {
                found: 4,
                expected: SCHEMA_VERSION,
                said: 原话,
            },
        },
    ])
}

/// **读不动**：中立库住的那个目录在，却列不开。系统那半句取 `PermissionDenied` 本身的说法，
/// 各平台一样。
fn 读不动(工作目录: &Path) -> Listing {
    Listing::Unreadable(DirUnreadable {
        dir: 工作目录.join("catalog"),
        source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
    })
}

#[test]
fn 开场_有库_浅色() {
    let mut 开场 = Screen::listed(工作目录(), 有库(&工作目录()));
    拍("opening/catalogs-light", Theme::Light, move |ui| {
        drop(开场.ui(ui));
    });
}

#[test]
fn 开场_有库_暗色() {
    let mut 开场 = Screen::listed(工作目录(), 有库(&工作目录()));
    拍("opening/catalogs-dark", Theme::Dark, move |ui| {
        drop(开场.ui(ui));
    });
}

#[test]
fn 开场_空的_浅色() {
    let mut 开场 = Screen::listed(工作目录(), Listing::Empty);
    拍("opening/empty-light", Theme::Light, move |ui| {
        drop(开场.ui(ui));
    });
}

#[test]
fn 开场_空的_暗色() {
    let mut 开场 = Screen::listed(工作目录(), Listing::Empty);
    拍("opening/empty-dark", Theme::Dark, move |ui| {
        drop(开场.ui(ui));
    });
}

#[test]
fn 开场_读不动_浅色() {
    let mut 开场 = Screen::listed(工作目录(), 读不动(&工作目录()));
    拍("opening/unreadable-light", Theme::Light, move |ui| {
        drop(开场.ui(ui));
    });
}

#[test]
fn 开场_读不动_暗色() {
    let mut 开场 = Screen::listed(工作目录(), 读不动(&工作目录()));
    拍("opening/unreadable-dark", Theme::Dark, move |ui| {
        drop(开场.ui(ui));
    });
}

#[test]
fn 开场_添加主库向导盖在上面_浅色() {
    const 名字: &str = "opening/claiming-light";
    if 该跳过(名字) {
        return;
    }
    // 空的工作目录上按下那颗「添加主库」：向导刚打开，起名那一问，焦点落在那一框里。
    // 这一路一个字节都不碰盘——起名那一框空着时不问核心库（`claim` 模块文档）。
    let mut 开场 = Screen::listed(工作目录(), Listing::Empty);
    let mut harness = 开一个(Theme::Light, move |ui| {
        drop(开场.ui(ui));
    });
    按(&mut harness, "添加主库");
    拍下(harness, 名字);
}

// ——— 浏览（票 `gui-looks-like-the-design/09`） ———
//
// 浏览屏住在主窗口里（[`App`]），喂一份**手搭的小库**：五个认出来的作品、三个认不出作品的变体，
// 名字与路径照真库的样子（DAT 条目名、汉化组记号、整理目录）。库与沉淀库全在内存里。
//
// 工作目录是一个临时目录——版式偏好往那儿读写，**屏上一个字都不画它**。媒体池不在那儿，
// 于是封面一律走「没有封面」那一档（详情头上的字卡、行首的平台色块）：没有后台解码要等，
// 跑到不要重画为止就是稳的那一帧。

/// 基线那份库的根名：变体的键第一段就是它。
const 浏览的根: &str = "主库";

/// 那一行**认不出作品**、路径长到一格画不下的变体（根底下那一截）。文件名是剥离规则模块自己钉着的
/// 那一例（剥完是 `超级机器人大战R`），前面垫两层真库里常见的整理目录。
const 长路径: &str = "【全部汉化】/GBA 汉化合集 第一辑（按首字排好）/超级机器人大战R[星组](v1.2+)(简)(JP)(68.92Mb).zip";

/// 点开来看侧边详情的那个作品。
const 点开的作品: &str = "Chrono Trigger (Japan)";

/// 一个变体识别落在哪一档。
#[derive(Debug, Clone, Copy)]
enum 档 {
    /// 命中：一条自动通过的候选，候选说的就是它挂的那个作品。
    命中(Confidence),
    /// 未命中，带一条低置信、没采纳的候选（候选说的是这个名字）——进待确认队列的那种。
    待裁决(&'static str),
    /// 识别跑过了，一条候选都没有。
    没有候选,
    /// 结论表里一行都不写。
    还没识别,
}

/// 基线里的一个变体。
struct 一个变体 {
    平台: &'static str,
    /// 根底下的相对路径。
    路径: &'static str,
    字节: u64,
    落在: 档,
    /// 挂在哪个作品上；`None` 是认不出作品。
    作品: Option<&'static str>,
}

const MIB: u64 = 1024 * 1024;

/// 八个变体收成八行：五个作品（两个作品底下各两个变体）、三个认不出作品的。
const 浏览的变体: &[一个变体] = &[
    一个变体 {
        平台: "SFC",
        路径: "Chrono Trigger (Japan).zip",
        字节: 4 * MIB,
        落在: 档::命中(Confidence::High),
        作品: Some(点开的作品),
    },
    一个变体 {
        平台: "SFC",
        路径: "汉化/时空之轮 (简体中文 v1.2).zip",
        字节: 4 * MIB,
        落在: 档::命中(Confidence::Medium),
        作品: Some(点开的作品),
    },
    一个变体 {
        平台: "GBA",
        路径: "Gyakuten Saiban (Japan).zip",
        字节: 8 * MIB,
        落在: 档::命中(Confidence::High),
        作品: Some("Gyakuten Saiban (Japan)"),
    },
    一个变体 {
        平台: "GB",
        路径: "Pocket Monsters - Aka (Japan).zip",
        字节: MIB,
        落在: 档::命中(Confidence::High),
        作品: Some("Pocket Monsters - Aka (Japan)"),
    },
    一个变体 {
        平台: "GB",
        路径: "汉化/口袋妖怪 红 (口袋汉化组).zip",
        字节: MIB,
        落在: 档::命中(Confidence::Medium),
        作品: Some("Pocket Monsters - Aka (Japan)"),
    },
    一个变体 {
        平台: "FC",
        路径: "Rockman 2 - Dr. Wily no Nazo (Japan).nes",
        字节: 256 * 1024,
        落在: 档::命中(Confidence::High),
        作品: Some("Rockman 2 - Dr. Wily no Nazo (Japan)"),
    },
    一个变体 {
        平台: "SFC",
        路径: "Seiken Densetsu 2 (Japan).sfc",
        字节: 2 * MIB,
        落在: 档::命中(Confidence::High),
        作品: Some("Seiken Densetsu 2 (Japan)"),
    },
    一个变体 {
        平台: "GBA",
        路径: 长路径,
        字节: 64 * MIB,
        落在: 档::没有候选,
        作品: None,
    },
    一个变体 {
        平台: "GBC",
        路径: "汉化/精灵宝可梦 银[简正确精灵名](完美LOGO+背包等汉化-sss888+RickyL1213).7z",
        字节: 2 * MIB,
        落在: 档::待裁决("Pocket Monsters - Gin (Japan)"),
        作品: None,
    },
    一个变体 {
        平台: "FC",
        路径: "【中文游戏】/0152 - 1942 - MS汉化组.nes",
        字节: 40 * 1024,
        落在: 档::还没识别,
        作品: None,
    },
];

/// 一个作品：`(作品名, 采到的元数据, 有没有封面)`。
type 一个作品 = (&'static str, &'static [(Field, &'static str)], bool);

/// 五个作品各采到了哪几样元数据、有没有封面。**元数据那一栏齐与缺各有几种**：齐、缺一样、缺几样、缺全部。
const 浏览的作品: &[一个作品] = &[
    (
        点开的作品,
        &[
            (Field::Year, "1995"),
            (Field::Publisher, "Square"),
            (Field::Developer, "Square"),
            (Field::Genre, "角色扮演"),
            (Field::Description, "穿越时空、改写结局的角色扮演游戏。"),
        ],
        true,
    ),
    (
        "Gyakuten Saiban (Japan)",
        &[
            (Field::Year, "2001"),
            (Field::Publisher, "Capcom"),
            (Field::Developer, "Capcom"),
            (Field::Genre, "文字冒险"),
        ],
        false,
    ),
    (
        "Pocket Monsters - Aka (Japan)",
        &[(Field::Year, "1996"), (Field::Genre, "角色扮演")],
        true,
    ),
    (
        "Rockman 2 - Dr. Wily no Nazo (Japan)",
        &[(Field::Year, "1988")],
        false,
    ),
    ("Seiken Densetsu 2 (Japan)", &[], false),
];

/// 有中文译名的那几个作品：`(作品名, 译名)`。主列表那几行主栏印显示标题、第二行小字印作品名；
/// 其余作品取不到显示标题，主栏印作品名、第二行不写——两种样子同一张图上都有。
const 浏览的译名: &[(&str, &str)] = &[
    (点开的作品, "超时空之钥"),
    ("Pocket Monsters - Aka (Japan)", "精灵宝可梦 红"),
    ("Seiken Densetsu 2 (Japan)", "圣剑传说 2"),
];

/// 浏览屏那一份现场。
struct 浏览现场 {
    app: App,
    /// 工作目录：版式偏好在里头，得活到拍完。
    目录: TempDir,
}

/// 一条候选：挂在这个变体自己的主文件上。
fn 候选(
    变体: &Variant,
    accepted: bool,
    confidence: Confidence,
    source: &str,
    game: &str,
    evidence: &str,
) -> Candidate {
    Candidate {
        member_key: 变体.main_key.clone(),
        inner: String::new(),
        confidence,
        accepted,
        source: source.to_owned(),
        dat: format!("{source}.dat"),
        platform: 变体.platform.clone().unwrap_or_default(),
        game: game.to_owned(),
        rom: "rom.bin".to_owned(),
        hashed_as: Convention::AsIs,
        dat_convention: Convention::AsIs,
        evidence: evidence.to_owned(),
        chinese: None,
        serial: None,
        release_id: None,
    }
}

/// 搭浏览屏那份库，开一个停在浏览屏上的主窗口。`收起两栏` 时先往工作目录的版式偏好里写上
/// 左右两栏都收着——走的是开窗时读偏好那一条真路，不是在帧里硬按。
fn 浏览现场(收起两栏: bool) -> 浏览现场 {
    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    romcat_core::catalog::roots::add_root(&catalog, None, 浏览的根, Path::new("/主库"))
        .expect("建得出根");

    let 变体: Vec<Variant> = 浏览的变体
        .iter()
        .map(|one| {
            let key = format!("{浏览的根}/{}/{}", one.平台, one.路径);
            Variant {
                main_key: key.clone(),
                platform: Some(one.平台.to_owned()),
                rule: SINGLE_FILE_RULE.to_owned(),
                manual: false,
                files: 1,
                bytes: one.字节,
                unreadable_files: 0,
                // 变体成员的角色（核心库的 `shape::Role`）；这个文件里单写的 `Role` 是无障碍树的角色（库屏那一段用）。
                members: vec![(key.clone(), romcat_core::shape::Role::Main)],
                key,
            }
        })
        .collect();
    catalog
        .replace_variants(&变体, 1, &Manifest::default())
        .expect("写得进变体");

    let mut 作品号 = Vec::new();
    for (名字, _, _) in 浏览的作品 {
        let id = catalog
            .add_work(名字, Provenance::Identified)
            .expect("建得出作品");
        作品号.push((*名字, id));
    }
    let 译名: Vec<romcat_core::catalog::TitleRow> = 浏览的译名
        .iter()
        .map(|(作品, 译名)| romcat_core::catalog::TitleRow {
            work: (*作品).to_owned(),
            value: (*译名).to_owned(),
            language: romcat_core::title::Language::Chinese,
            kind: romcat_core::title::TitleKind::Translated,
            source: "中文离线源".to_owned(),
            region: None,
            variant_key: None,
            confidence: Confidence::High,
            seam: None,
            evidence: "基线里摆的".to_owned(),
            seen: 1,
        })
        .collect();
    catalog.put_titles(&译名).expect("写得进标题集合");
    let 号 = |名字: &str| {
        作品号
            .iter()
            .find(|(it, _)| *it == 名字)
            .map(|(_, id)| *id)
            .expect("作品表里有这个作品")
    };

    let 结论: Vec<Identification> = 浏览的变体
        .iter()
        .zip(&变体)
        .filter_map(|(one, variant)| {
            let (state, candidates) = match one.落在 {
                档::命中(confidence) => (
                    State::Matched,
                    vec![候选(
                        variant,
                        true,
                        confidence,
                        "No-Intro",
                        one.作品.unwrap_or(one.路径),
                        "CRC-32 与文件大小一致",
                    )],
                ),
                档::待裁决(game) => (
                    State::Unmatched,
                    vec![候选(
                        variant,
                        false,
                        Confidence::Low,
                        "中文离线源",
                        game,
                        "名称模糊匹配，平台一致；文件内容改过，对不上 DAT",
                    )],
                ),
                档::没有候选 => (State::Unmatched, Vec::new()),
                档::还没识别 => return None,
            };
            Some(Identification {
                variant_key: variant.key.clone(),
                platform: None,
                standalone: None,
                edition: None,
                state,
                reason: None,
                units: 1,
                nkit: 0,
                read_bytes: 0,
                work_id: one.作品.map(&号),
                release_id: None,
                candidates,
            })
        })
        .collect();
    catalog.write_identifications(&结论).expect("写得进结论");

    let mut 采到的 = Vec::new();
    for (at, (名字, 字段, 有封面)) in 浏览的作品.iter().enumerate() {
        let mut media = Vec::new();
        if *有封面 {
            let hash = format!("{:040x}", at + 1);
            // 尺寸就是 [`详情页的封面图`] 那张真图的宽高——媒体那一格照稿写
            // 「来源 · 尺寸 · 大小」，基线上那个数得与池里真躺着的那张对得上。
            catalog
                .put_media(
                    &hash,
                    "png",
                    86 * 1024,
                    Measured {
                        width: Some(300),
                        height: Some(400),
                        duration_ms: None,
                    },
                )
                .expect("记得进媒体");
            media.push(HarvestedMedia {
                kind: MediaKind::Cover.label().to_owned(),
                hash,
                evidence: "基线里摆的一张封面".to_owned(),
            });
        }
        采到的.push(Harvested {
            anchor: AnchorKind::Work.label().to_owned(),
            subject: (*名字).to_owned(),
            source: "No-Intro".to_owned(),
            input: "基线".to_owned(),
            values: 字段
                .iter()
                .map(|(field, value)| HarvestedValue {
                    field: field.label().to_owned(),
                    value: (*value).to_owned(),
                    evidence: "基线里摆的".to_owned(),
                })
                .collect(),
            media,
        });
    }
    catalog.put_scraped(&采到的).expect("写得进刮削值");

    let 目录 = temp_dir("gui-截图门-浏览");
    if 收起两栏 {
        let at = romcat_core::workspace::gui_layout_path(目录.path());
        std::fs::create_dir_all(at.parent().expect("版式偏好有上一级目录")).expect("建得出目录");
        std::fs::write(
            &at,
            format!(
                "{} 收起 = 是\n{} 收起 = 是\n",
                layout::FILTER.id,
                layout::DETAIL.id
            ),
        )
        .expect("写得下版式偏好");
    }
    let site = Site::in_memory(catalog, Store::in_memory().expect("开得出沉淀库"), 浏览的根);
    let mut app = App::new(site, 目录.path().to_path_buf());
    // 底部状态栏右边印着工作目录（票 25）：这儿是临时目录，每一趟都不一样，照实画的话同一张图一趟一个样——
    // 与任务屏、主窗口外壳那几张一样定死成设计稿上那一串。
    app.set_workspace_label(工作目录().display().to_string());
    app.show_view(View::Browse);
    浏览现场 { app, 目录 }
}

/// 浏览屏拍哪一态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum 浏览态 {
    /// 三栏摊开，点一下那一行认不出作品的：那一行铺着选中的浅底，侧边详情头上是字卡。
    三栏,
    /// 打开「在每行开头显示封面」、点一下一个作品：没有媒体池，行首一律是平台色块。
    行首封面,
    /// 筛一个库里没有的平台：一行都不剩，表头底下是空态与「清除筛选」。
    筛空,
    /// 左右两栏都收着（从工作目录的版式偏好读出来的），点一下那一行认不出作品的。
    两栏收起,
    /// 卡片墙：按平台分组，混合有封面和无封面的字卡。
    卡片,
}

/// 卡片墙往下滚几个点再拍（[`拍浏览`] 的卡片那一态）。
///
/// 刚够**第二排整排连卡面下半截一起露出来**——认不出作品的那几张排在那儿，那枚「未关联作品」
/// 就画在下半截那一行上。再多滚就把头一排整个推出去了，这一张同时要说得清「墙从头是什么样」。
const 卡片墙滚一截: f32 = 110.0;

/// 那一行认不出作品的（长路径那一个）元数据那一格写着的字：一条候选都没有、一样元数据都没采到。
/// 整张表只有它一行是这个词，按它就是点那一行。
const 长路径那一行: &str = "仅文件名";

/// 点开的那个作品（五样元数据都齐）元数据那一格写着的字，整张表只有它一行是这个词。
const 点开的作品那一行: &str = "完整";

/// 搭好浏览屏的那一态、拍一张。CI 上跳过（[`该跳过`]）。
#[track_caller]
fn 拍浏览(名字: &str, 主题: Theme, 态: 浏览态) {
    if 该跳过(名字) {
        return;
    }
    let 浏览现场 { mut app, 目录 } = 浏览现场(态 == 浏览态::两栏收起);
    if 态 == 浏览态::卡片 {
        app.browse_and_site().0.show_cards();
    }
    if 态 == 浏览态::筛空 {
        let (browse, _) = app.browse_and_site();
        browse.query_mut().platform = Some(PlatformFilter::from_label("PS2"));
    }
    let mut harness = 开一个(主题, move |ui| app.ui(ui));
    // **点开一行照人的操作点那一行**：走表格自己那条选中的路，那一行铺上选中的浅底（设计稿
    // `.wtbl tr[aria-selected]`），侧边详情跟着点开。按的是那一行元数据那一格的字——那一枚标签只认悬停，
    // 按下去落在那一行上。
    match 态 {
        浏览态::三栏 | 浏览态::两栏收起 => 按(&mut harness, 长路径那一行),
        浏览态::行首封面 => {
            按(&mut harness, "在每行开头显示封面");
            按(&mut harness, 点开的作品那一行);
        }
        浏览态::筛空 | 浏览态::卡片 => {}
    }
    // **卡片墙滚到第二排整排看得见**（拿主意的人 2026-09-20；办法同详情页媒体那一面，协调人 2026-09-15）：
    // 不滚的话屏上只露得出头一排整卡与第二排的封面，而认不出作品的那几张都排在第二排往后——
    // 卡面下半截那一行（连着那枚「未关联作品」）落在视口以下，**基线就盖不住这条新行为**。
    // 像人一样把指针停在卡片墙上往下滚一截：滚的只是墙（工具条那两行在滚动区外头，滚不走），
    // 滚的量刚够第二排连标签一起露出来，头一排照旧读得出它的标题与置信度。
    if 态 == 浏览态::卡片 {
        harness.event(egui::Event::PointerMoved(egui::pos2(700.0, 500.0)));
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -卡片墙滚一截),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        });
        harness.event(egui::Event::PointerGone);
        harness.step();
        harness.run_steps(5);
    }
    控件都落在所在那一栏里(&harness, 名字);
    // [`带标签的行正题露得出字`] 查的是**表格那几行**被截成什么样（正题一行、标签领着路径一行）：
    // 筛空那一态一行都不剩，卡片那一态屏上根本没有表格。这两态跳过。
    //
    // **已裁：卡面也画这枚标签**（稿上没画，拿主意的人 2026-09-20 定）——与表格同一句词、同一枚标签，
    // 判据也同一处出（`table::unlinked_title`）。可卡面上它摆在下半截那一行的最左边，没有「正题在它
    // 正上方、路径在它右边」这副结构，拿这把尺子量不着。卡片那一路由
    // `tests/browse.rs` 的 `卡片墙上认不出作品的那几张挂着未关联作品标签_认出的不挂` 守着。
    if !matches!(态, 浏览态::筛空 | 浏览态::卡片) {
        带标签的行正题露得出字(&harness, 名字);
    }
    拍下(harness, 名字);
    // 拍完才收工作目录：版式偏好一直在里头读写。
    drop(目录);
}

// ——— 右键菜单与快捷键表（票 `gui-looks-like-the-design/14`） ———

/// **右键**按一下屏上正好写着 `那几个字`、最后画出来的那一处；其余同 [`按`]。
fn 右键按(harness: &mut Harness<'_>, 那几个字: &str) {
    let Some(在) = 最后一处正好画着(harness.output(), 那几个字) else {
        panic!("屏上没有正好写着「{那几个字}」的地方，没处右键");
    };
    let 键 = |pressed: bool| egui::Event::PointerButton {
        pos: 在,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    harness.event(egui::Event::PointerMoved(在));
    harness.event(键(true));
    harness.event(键(false));
    harness.event(egui::Event::PointerGone);
    harness.step();
    // 菜单是下一帧才摊开的（`browse::Screen::settle_menu`），浮层还要一帧才摆稳。
    harness.run_steps(6);
    harness.run();
}

/// **右键菜单摊在一行上**那一张：三栏摆着，菜单贴着右键那一下。
///
/// **指针先挪走再拍**（同 [`按`]）：基线里不该有指针三角，也不该有哪一项挂着悬停底色
/// ——那一项每换一次指针位置就换一次样子，拍出来的基线跟着飘。
#[track_caller]
fn 拍右键菜单(名字: &str, 主题: Theme) {
    if 该跳过(名字) {
        return;
    }
    let 浏览现场 { mut app, 目录 } = 浏览现场(false);
    let mut harness = 开一个(主题, move |ui| app.ui(ui));
    // **按在作品名那一格上**（不是最右边那一格）——挑这一格有三条理由，都是量过／看过的
    // （协调人 2026-09-22 裁定这一手留着，挂单 `Q1141`）：
    //
    // 1. 菜单贴着右键那一下摊开。按最右边那一格的话它**整层落在右边那块详情栏上头**，
    //    挡住的正是右键那一下刚换过去的那份详情——这一张就说不清「右键之后屏上什么样」。
    // 2. **暗色里那样拍看不出菜单的边**：菜单与详情栏同是令牌 `panel`。那一圈描边补上了
    //    （`menu::菜单框`），可基线该拍的是稿上常见的那种落法——菜单落在**表**上。
    // 3. 顺带把「卡面／格子里的字不许接住点击」那一条也走了一遍：这一格是字，那一下得
    //    归整行（挂单 `Q1147` 在卡片墙上撞的是同一件事）。
    右键按(&mut harness, 右键那一行);
    菜单那一列提示立成一列(&harness, 名字);
    拍下(harness, 名字);
    drop(目录);
}

/// 右键按在哪一行：那一行**作品名那一格**上写着的字。整张表只有它一行是这个名字。
const 右键那一行: &str = "超级机器人大战R";

/// 菜单上那几项的字，照设计稿的次序（`browse::menu` 那一份清单）。
const 菜单那几项: [&str; 8] = [
    "打开详情",
    "编辑元数据",
    "勾选",
    "收藏",
    "合并…",
    "刮削此作品",
    "在文件系统中打开",
    "复制名称",
];

/// **菜单上那一列快捷键提示立得住**：每一项占满整个菜单的内宽，提示一律贴着
/// 「右缘 − `menu-item-padding`」摆——于是那一列的右缘是同一条线（设计稿
/// `.ctx button span` 的 `margin-left:auto`）。
///
/// **量的是控件自己的矩形**（无障碍树上那一份），不是像素比、也不是测试里量到的
/// 那一段字的外框：`Shape::Text` 的 `pos` 在这条路上不是最终屏幕坐标（票 11 差点
/// 据此报一个不存在的错位）。提示那几段字是拿画笔直接画的，没有自己的控件矩形——
/// 而它们摆在哪儿完全由**所在那一项**的矩形定，所以量那一项就够了：
/// 各项的左缘、右缘都是同一条线，那一列提示的右缘就也是。
#[track_caller]
fn 菜单那一列提示立成一列(harness: &Harness<'_>, 名字: &str) {
    // **屏上同一句话不止一处时取最后那一处**：菜单是一层浮层（`Order::Foreground`），
    // 无障碍树上排在最后——而「编辑元数据」这一句，右边那栏详情上也有一颗按钮写着它
    // （右键那一下把详情换成了这一行）。
    let 各项: Vec<egui::Rect> = 菜单那几项
        .iter()
        .map(|一项| {
            harness
                .query_all_by_role_and_label(Role::Button, 一项)
                .map(|node| node.rect())
                .next_back()
                .unwrap_or_else(|| panic!("{名字}：菜单上没有「{一项}」"))
        })
        .collect();
    assert_eq!(
        各项.len(),
        菜单那几项.len(),
        "{名字}：菜单上该有 {} 项",
        菜单那几项.len()
    );
    let 左缘: Vec<f32> = 各项.iter().map(|rect| rect.left()).collect();
    let 右缘: Vec<f32> = 各项.iter().map(|rect| rect.right()).collect();
    assert!(
        左缘.windows(2).all(|两个| (两个[0] - 两个[1]).abs() < 0.5),
        "{名字}：菜单各项的左缘不是同一条线，是 {左缘:?}"
    );
    assert!(
        右缘.windows(2).all(|两个| (两个[0] - 两个[1]).abs() < 0.5),
        "{名字}：菜单各项的右缘不是同一条线——那一列提示就立不住，是 {右缘:?}"
    );
}

/// **按 `?` 摊开的那层快捷键表**那一张：盖在浏览屏上头。
#[track_caller]
fn 拍快捷键表(名字: &str, 主题: Theme) {
    if 该跳过(名字) {
        return;
    }
    let 浏览现场 { mut app, 目录 } = 浏览现场(false);
    let mut harness = 开一个(主题, move |ui| app.ui(ui));
    harness.event(egui::Event::Key {
        key: egui::Key::Questionmark,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.step();
    harness.run_steps(6);
    harness.run();
    拍下(harness, 名字);
    drop(目录);
}

#[test]
fn 浏览_右键菜单_浅色() {
    拍右键菜单("browse/context-menu-light", Theme::Light);
}

#[test]
fn 浏览_右键菜单_暗色() {
    拍右键菜单("browse/context-menu-dark", Theme::Dark);
}

#[test]
fn 浏览_快捷键表_浅色() {
    拍快捷键表("browse/keys-sheet-light", Theme::Light);
}

#[test]
fn 浏览_快捷键表_暗色() {
    拍快捷键表("browse/keys-sheet-dark", Theme::Dark);
}

/// 带「未关联作品」标签的那一行，正题至少露出这么多个字（不算截断补上的「…」）。
const 正题至少露出: usize = 3;

/// **带「未关联作品」标签的那几行，第一行的正题没被截成只剩「…」**（拿主意的人 2026-09-14：标签挪到
/// 第二行、跟路径放在一起，正题单独占第一行，挂单 `Q878`）。
///
/// 表上每画一枚「未关联作品」标签，就找它正上方、同一格里画的那一段字——那就是这一行的正题。
/// 数的是**真画出来的字形**，不是那一段的原文：egui 截断时原文照旧整段留在排版结果里，只是后头的字
/// 没排、最后一个换成「…」。
///
/// **只查整枚看得见的标签**：表格是滚动区，视口外头那几行的正题是 `Label`，egui 不画；标签是拿画笔
/// 直接画的，照样交出一段字、只是被裁剪矩形裁掉——拿它去找正题只会扑空（snap-13 行首封面那两张：
/// 第三行带标签的整行在表格视口底下）。看不见的行不是「正题被截没了」。
///
/// **第二行的路径也查**（协调人 2026-09-15 审行首封面那两张打回「未关联作品 …s」）：标签右边、同一格
/// （同一个裁剪矩形，表格每一列各裁各的）、同一条横带里的那一段就是路径。没画路径可以——放不下时只画
/// 标签；画了就至少露出 [`romcat_gui::table::PATH_MIN_CHARS`] 个字，不许只剩一两个。
#[track_caller]
fn 带标签的行正题露得出字(harness: &Harness<'_>, 名字: &str) {
    use romcat_gui::table::UNLINKED_LABEL;

    // 每一段字：`(外框, 裁剪矩形, 排版结果)`。
    type 一段 = (egui::Rect, egui::Rect, std::sync::Arc<egui::Galley>);
    let mut 各段: Vec<一段> = Vec::new();
    fn 收(shape: &egui::epaint::Shape, clip: egui::Rect, out: &mut Vec<一段>) {
        match shape {
            egui::epaint::Shape::Text(text) => out.push((
                egui::Rect::from_min_size(text.pos, text.galley.size()),
                clip,
                text.galley.clone(),
            )),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|one| 收(one, clip, out)),
            _ => {}
        }
    }
    for clipped in &harness.output().shapes {
        收(&clipped.shape, clipped.clip_rect, &mut 各段);
    }
    let tag_padding = romcat_gui::tokens::Tokens::builtin().layout.tag_padding;
    let tag_height = romcat_gui::tokens::Tokens::builtin().layout.tag_height;

    let mut 查过 = 0;
    let mut 截没了 = Vec::new();
    let 露出的字 = |galley: &egui::Galley| -> String {
        galley
            .rows
            .iter()
            .flat_map(|row| row.glyphs.iter().map(|glyph| glyph.chr))
            .filter(|chr| *chr != '…')
            .collect()
    };
    for (标签, 标签裁剪, _) in 各段
        .iter()
        .filter(|(框, 裁剪, galley)| galley.text() == UNLINKED_LABEL && 裁剪.contains_rect(*框))
    {
        let 路径 = 各段
            .iter()
            .filter(|(框, 裁剪, _)| {
                裁剪 == 标签裁剪
                    && 框.min.x >= 标签.max.x
                    && (框.center().y - 标签.center().y).abs() <= tag_height / 2.0
            })
            .min_by(|(甲, _, _), (乙, _, _)| 甲.min.x.total_cmp(&乙.min.x));
        if let Some((路径框, _, 路径)) = 路径 {
            let 露出来的 = 露出的字(路径);
            if 露出来的.chars().count() < romcat_gui::table::PATH_MIN_CHARS {
                截没了.push(format!(
                    "标签后头的路径「{}」只露出「{露出来的}」（画在 {路径框:?}）",
                    路径.text()
                ));
            }
        }
        // 标签那一枚的字画在底色正中，底色左沿比字再往左一份 `tag-padding`；正题与底色左沿对齐。
        let 左沿 = 标签.min.x - tag_padding;
        let Some((正题框, _, 正题)) = 各段
            .iter()
            .filter(|(框, _, _)| {
                (框.min.x - 左沿).abs() <= 1.0
                    && 框.max.y <= 标签.min.y + 0.5
                    && 标签.min.y - 框.max.y <= tag_height
            })
            .max_by(|(甲, _, _), (乙, _, _)| 甲.max.y.total_cmp(&乙.max.y))
        else {
            截没了.push(format!("标签 {标签:?} 正上方没画正题"));
            continue;
        };
        查过 += 1;
        let 露出来的 = 露出的字(正题);
        if 露出来的.chars().count() < 正题至少露出 {
            截没了.push(format!(
                "「{}」只露出「{露出来的}」（画在 {正题框:?}）",
                正题.text()
            ));
        }
    }
    assert!(查过 > 0, "{名字}：表上一枚「{UNLINKED_LABEL}」标签都没画");
    assert!(
        截没了.is_empty(),
        "{名字}：带标签的行被截得太短（正题至少 {正题至少露出} 个字，路径没画或至少 {} 个字）：\n{}",
        romcat_gui::table::PATH_MIN_CHARS,
        截没了.join("\n"),
    );
}

/// **每一个可交互的控件都整个落在它所在那一栏的可见区里**（拿主意的人看浏览屏：「按钮都没显示全」）。
///
/// 从**无障碍树**读：egui 给每个控件挂一个节点，外框就是它摆出来的那一块——画出界、被旁边一栏或
/// 窗沿盖掉的那一截，外框里照样算着。「可交互」认的是**点得了或者聚焦得了**；面板本身、拖边界的
/// 把手与滚动条不算，它们本来就骑在边上。
///
/// 一栏是哪一块，问 egui 自己存的面板尺寸：屏头、左边的导航（票 `gui-looks-like-the-design/32` 的外壳）、
/// 底部状态栏（票 `gui-looks-like-the-design/25`）、
/// 筛选那一栏（收起时是那条窄条）、侧边详情（同），剩下的是表格那一块。控件**上沿的中点**落在哪一栏，就归哪一栏；哪一栏都不落的，本身就是
/// 问题。按上沿不按中心：滚动区最底下那一个被窗沿截掉一半时，中心已经出了窗，上沿还在它那一栏里。
///
/// 面板边上那条拖动把手不算：egui 给它的节点没有角色、只有两份 `resize_grab_radius_side` 那么宽，
/// 本来就骑在两栏交界上。
///
/// **左右两沿必须整个在栏里，竖着只查上沿**：左栏、右栏与表格都是滚动区，最底下那一个
/// 被滚动区的下沿截掉一截是滚动区本来的样子（设计稿里左栏最底下那一格也截着），滚一下就整个露出来；
/// 屏头、导航与状态栏不滚，上下两沿都查（挂单 `Q871`）。
#[track_caller]
fn 控件都落在所在那一栏里(harness: &Harness<'_>, 名字: &str) {
    use egui::accesskit::{Action, Role};
    use egui_kittest::kittest::NodeT;

    let ctx = &harness.ctx;
    let 面板 = |id: egui::Id| egui::PanelState::load(ctx, id).map(|state| state.outer_rect);
    let 侧栏 = |boundary: layout::Boundary| {
        if boundary.collapsed(ctx) {
            面板(egui::Id::new(boundary.id).with("窄条"))
        } else {
            面板(egui::Id::new(boundary.id))
        }
    };
    let 窗 = egui::Rect::from_min_size(egui::Pos2::ZERO, headless::VIEWPORT.into());
    let 屏头 = 面板(egui::Id::new(("屏头", View::Browse))).expect("屏头画过");
    let 导航 = 面板(egui::Id::new("左栏")).expect("导航画过");
    let 状态栏 = 面板(egui::Id::new("状态栏")).expect("状态栏画过");
    let 左栏 = 侧栏(layout::FILTER).expect("左栏画过");
    let 右栏 = 侧栏(layout::DETAIL).expect("右栏画过");
    let 表格 = egui::Rect::from_min_max(
        egui::pos2(左栏.max.x, 屏头.max.y),
        egui::pos2(右栏.min.x, 状态栏.min.y),
    );
    // 次序有讲究：屏头横跨导航右边整个宽，先认。
    let 各栏 = [
        ("屏头", 屏头, true),
        ("导航", 导航, true),
        ("状态栏", 状态栏, true),
        ("左栏", 左栏, false),
        ("右栏", 右栏, false),
        ("表格", 表格, false),
    ];

    let 把手宽 = 2.0
        * ctx
            .style_of(ctx.theme())
            .interaction
            .resize_grab_radius_side;

    let mut 没显示全 = Vec::new();
    for node in harness.root().children_recursive() {
        let data = node.accesskit_node();
        let 可交互 = data.data().supports_action(Action::Click)
            || data.data().supports_action(Action::Focus);
        let 骑在边上 = matches!(
            data.role(),
            Role::Pane | Role::Splitter | Role::ScrollBar | Role::Window
        );
        if !可交互 || 骑在边上 {
            continue;
        }
        let rect = node.rect();
        if !rect.is_positive() || !rect.intersects(窗) {
            continue;
        }
        if data.role() == Role::Unknown && rect.width().min(rect.height()) <= 把手宽 + 0.5 {
            continue;
        }
        // **滚出了视口、被状态栏挡住的那一个不算**（合进票 25 之后）：左右两栏的下沿就是状态栏的上沿，
        // 滚动区最底下那几个整个滚到了视口底下，上沿中点落进状态栏那一带——那不是状态栏里的控件没摆下，是还没
        // 滚到（与上面「整个在窗外的不算」同一类）。状态栏只有一行高、贴着窗口底沿：顶沿在状态栏里、却伸出窗口
        // 底沿的，只能是上头某个滚动区里的；整个落在状态栏里的照旧当状态栏的控件查。
        // 代价：状态栏里的控件**竖着**伸出窗口底沿的，这一条抓不到（横着伸出去照旧抓得到）。egui 没把每个
        // 控件被裁掉之后剩多少交给无障碍树，只能按几块面板的外框判。
        let 滚出视口 = rect.min.y >= 状态栏.min.y - 0.5
            && rect.max.y > 状态栏.max.y + 0.5
            && [左栏, 右栏]
                .iter()
                .any(|栏| 栏.x_range().contains(rect.center().x) && 栏.max.y <= rect.min.y + 0.5);
        if 滚出视口 {
            continue;
        }
        let 叫什么 = data
            .label()
            .or_else(|| data.value())
            .unwrap_or_else(|| format!("{:?}", data.role()));
        let 上沿中点 = egui::pos2(rect.center().x, rect.min.y + 0.5);
        let Some((栏名, 栏, 上下都查)) = 各栏.iter().find(|(_, 栏, _)| 栏.contains(上沿中点))
        else {
            没显示全.push(format!("「{叫什么}」{rect:?} 不在任何一栏里"));
            continue;
        };
        let 左右在 = rect.min.x >= 栏.min.x - 0.5 && rect.max.x <= 栏.max.x + 0.5;
        let 上沿在 = rect.min.y >= 栏.min.y - 0.5;
        let 下沿在 = !上下都查 || rect.max.y <= 栏.max.y + 0.5;
        if !(左右在 && 上沿在 && 下沿在) {
            没显示全.push(format!("「{叫什么}」{rect:?} 伸出了{栏名} {栏:?}"));
        }
    }
    assert!(
        没显示全.is_empty(),
        "{名字}：{} 个控件没整个落在所在那一栏里：\n{}",
        没显示全.len(),
        没显示全.join("\n"),
    );
}

#[test]
fn 浏览_三栏_浅色() {
    拍浏览("browse/rows-light", Theme::Light, 浏览态::三栏);
}

#[test]
fn 浏览_三栏_暗色() {
    拍浏览("browse/rows-dark", Theme::Dark, 浏览态::三栏);
}

#[test]
fn 浏览_行首封面_浅色() {
    拍浏览("browse/covers-light", Theme::Light, 浏览态::行首封面);
}

#[test]
fn 浏览_行首封面_暗色() {
    拍浏览("browse/covers-dark", Theme::Dark, 浏览态::行首封面);
}

#[test]
fn 浏览_筛空_浅色() {
    拍浏览("browse/empty-light", Theme::Light, 浏览态::筛空);
}

#[test]
fn 浏览_筛空_暗色() {
    拍浏览("browse/empty-dark", Theme::Dark, 浏览态::筛空);
}

#[test]
fn 浏览_两栏收起_浅色() {
    拍浏览("browse/collapsed-light", Theme::Light, 浏览态::两栏收起);
}

#[test]
fn 浏览_两栏收起_暗色() {
    拍浏览("browse/collapsed-dark", Theme::Dark, 浏览态::两栏收起);
}

#[test]
fn 浏览_卡片_浅色() {
    拍浏览("browse/cards-light", Theme::Light, 浏览态::卡片);
}

#[test]
fn 浏览_卡片_暗色() {
    拍浏览("browse/cards-dark", Theme::Dark, 浏览态::卡片);
}

// ——— 作品详情页（票 `gui-looks-like-the-design/15`） ———
//
// 同一份浏览屏的现场：点一下「Chrono Trigger (Japan)」那一行（五样元数据都齐、两个变体）——走表格自己那条选中的路，
// 顶上「第几个 / 共几个」跟着有——再从「查看详情」那个入口（`Screen::open_page`）打开作品详情页、停到要拍的那一面。
// 双击打开那条路由 `tests/work.rs` 的「双击主列表一行打开作品详情页…」守着；这里拍的是页面长什么样。
// 媒体池不在工作目录里，封面照旧是字卡。

/// 等后台那几份图解完**最多跑几帧**。
///
/// 解一张 300×400 的 PNG 是毫秒级的事，而视频那一格连进程都拉不起来（抽帧程序换成了一个
/// 不存在的，[`挂上媒体池`]），当场就退化成占位。几十帧就该齐，这个数是留给慢机器的余量。
/// **给得明确是为了等不到时当场说话**：从前写的是十万帧，一条要空转四五分钟才肯报错，
/// 而报出来的那句话还不说是哪一项没满足。
const 等图最多几帧: usize = 2_000;

/// 详情页那几张的一张封面：竖版、上下两块色，解出来一眼看得出是一张图而不是占位。
fn 详情页的封面图() -> Vec<u8> {
    let image = image::RgbImage::from_fn(300, 400, |_, y| {
        if y < 260 {
            image::Rgb([52, 96, 150])
        } else {
            image::Rgb([214, 160, 72])
        }
    });
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut out, image::ImageFormat::Png)
        .expect("编得出 PNG");
    out.into_inner()
}

/// 给详情页那几张**挂上真的媒体池**（协调人 2026-09-15：图里要是用户真会看到的样子，不是「没查池子」）：点开的作品那张封面
/// 照库里记着的内容哈希落一张真图，另挂一段「视频」——抽帧程序换成一个不存在的，那一格就是没有 ffmpeg 的占位。
/// 浏览屏那几张不走这里，照旧没有媒体池（行首是平台色块）。
fn 挂上媒体池(app: &mut App, 目录: &Path) {
    use romcat_core::scrape::pool::MediaPool;

    let pool =
        MediaPool::open(&romcat_core::workspace::media_pool_dir(目录)).expect("开得出媒体池");
    let 落 = |hash: &str, ext: &str, bytes: &[u8]| {
        let at = pool.path_of(hash, ext);
        std::fs::create_dir_all(at.parent().expect("落点有上一级目录")).expect("建得出目录");
        std::fs::write(&at, bytes).expect("写得下");
    };
    let 点开的 = 浏览的作品
        .iter()
        .position(|(名字, _, _)| *名字 == 点开的作品)
        .expect("点开的作品在表里");
    落(&format!("{:040x}", 点开的 + 1), "png", &详情页的封面图());
    let 视频 = format!("{:040x}", 0xF1D0_u32);
    落(&视频, "mp4", &[0u8; 256]);
    let (browse, site) = app.browse_and_site();
    // **这一段的尺寸与时长是量过的**：量尺在**入池**那一刻跑（`scrape::measure`），
    // 而这台机器眼下没有 ffmpeg 只挡得住**抽首帧**——库里记着的那三格照样读得回来。
    // 基线上因此是「视频那一格是占位、底下那行却写着 640 × 480 · 0:30」，
    // 那正是真库里换过机器之后会看到的样子。
    site.catalog
        .put_media(
            &视频,
            "mp4",
            256,
            Measured {
                width: Some(640),
                height: Some(480),
                duration_ms: Some(30_000),
            },
        )
        .expect("记得进媒体");
    site.catalog
        .put_scraped(&[Harvested {
            anchor: AnchorKind::Work.label().to_owned(),
            subject: 点开的作品.to_owned(),
            source: "ScreenScraper".to_owned(),
            input: "基线".to_owned(),
            values: Vec::new(),
            media: vec![HarvestedMedia {
                kind: MediaKind::Video.label().to_owned(),
                hash: 视频,
                evidence: "基线里摆的一段视频".to_owned(),
            }],
        }])
        .expect("写得进刮削值");
    browse
        .gallery_mut()
        .set_program(romcat_core::scrape::preview::NO_SUCH_PROGRAM);
    browse.set_pool(Some(pool));
}

/// 基线里那一趟导出是什么时候（UNIX 纪元起的秒，屏上按 UTC 画成 `2026-09-18 11:08`）。
///
/// **定死**：拿挂钟出来的基线每重出一次就变一次，那种基线拦不住任何东西——所以走
/// `Catalog::mark_exported_at` 而不是 `mark_exported`，与屏上别处那几个钉死的时刻同一个做法。
const 导出于: i64 = 1_789_729_680;

/// 详情页那几张的**第几版、子库、导出**三样（票 `gui-looks-like-the-design/34`）。
///
/// 三样都摆成**用户真会看到的样子**（协调人 2026-09-15 那条「图里要是用户真会看到的样子」）：
///
/// - **第几版**：汉化那个变体记一条裁决说的 `v1.2`（它的文件名里就写着），原版那个一条
///   修订标记都没有——于是变体卡那一面**两种情形各有一个**。画面里只看得见「说不出」那一档
///   （带 `v1.2` 的那张卡在折叠线以下），那一档有 `tests/work.rs` 里的文字测试钉着。
/// - **子库**：一个收得住 SFC 的子库，名字照设计稿。
/// - **导出**：这个作品真写出去过一趟，时刻与整库那个数是同一个（[`导出于`]）。
fn 摆上第几版子库与导出(app: &mut App) {
    use romcat_core::catalog::export::ExportedEntry;
    use romcat_core::catalog::identify::Identification;
    use romcat_core::sublibrary::{Rule, Sublibrary};

    let (_, site) = app.browse_and_site();
    let 汉化那个 = "主库/SFC/汉化/时空之轮 (简体中文 v1.2).zip";
    let 原来的 = site
        .catalog
        .variant(汉化那个)
        .expect("读得动")
        .expect("基线里有这个变体");
    // **候选原样写回去**：`write_identifications` 先把这个变体的候选整批删掉再插——
    // 不带上原来那几条的话，这一行的置信度标签、识别结论与「识别依据」那一面全跟着变，
    // 而这张票动的只是「版本」那一格。
    let 原来的候选 = site.catalog.candidates_of(汉化那个).expect("读得动");
    site.catalog
        .write_identifications(&[Identification {
            variant_key: 汉化那个.to_owned(),
            platform: None,
            standalone: None,
            // 裁决那一层：「这是谁汉化的第几版」只有人说得出（ADR-0008、词表**第几版**）。
            edition: Some("v1.2".to_owned()),
            state: site
                .catalog
                .identification_of(汉化那个)
                .expect("读得动")
                .map_or(State::Matched, |(state, _)| state),
            reason: None,
            units: 1,
            nkit: 0,
            read_bytes: 0,
            work_id: 原来的.work_id,
            release_id: 原来的.release_id,
            candidates: 原来的候选,
        }])
        .expect("写得进结论");
    site.catalog
        .put_sublibrary(&Sublibrary {
            name: "RG35XX Plus".to_owned(),
            target: "/Volumes/SDCARD/Roms".to_owned(),
            target_raw: Some("/Volumes/SDCARD/Roms".to_owned()),
            format: "Pegasus".to_owned(),
            capacity: None,
            capability: None,
            capacity_by_device: false,
        })
        .expect("子库写得进");
    site.catalog
        .add_rule("RG35XX Plus", &Rule::parse("平台=SFC").expect("规则读得懂"))
        .expect("规则写得进");
    site.catalog
        .mark_exported_at(
            1,
            false,
            "Pegasus",
            &[ExportedEntry {
                anchor: AnchorKind::Work,
                subject: 点开的作品.to_owned(),
                platform: "SFC".to_owned(),
            }],
            导出于,
        )
        .expect("记得下");
}

/// 搭好浏览屏的现场，点开那个作品、打开作品详情页停在 `面` 那一面，拍一张。CI 上跳过（[`该跳过`]）。
///
/// **拍之前先认一眼真打开了**：头一趟出图时双击没打开，概览那两张拍成了浏览屏，而两遍核对照样绿——基线比的是
/// 自己，拍错了屏它看不出来。
#[track_caller]
fn 拍详情页(名字: &str, 主题: Theme, 面: Tab) {
    if 该跳过(名字) {
        return;
    }
    let 浏览现场 { mut app, 目录 } = 浏览现场(false);
    挂上媒体池(&mut app, 目录.path());
    摆上第几版子库与导出(&mut app);
    // 打开与换面走的是界面上「查看详情」、点一面的同一个入口（`Screen::open_page`）；交给画帧那个闭包在下一帧开头办。
    let 换面 = std::rc::Rc::new(std::cell::Cell::new(None::<Tab>));
    let 要换 = std::rc::Rc::clone(&换面);
    // 后台那几份图解完没有（封面解出来、视频那一格因为没有 ffmpeg 退成占位）：画帧那个闭包每帧报一次。
    let 图齐了 = std::rc::Rc::new(std::cell::Cell::new(false));
    let 报图 = std::rc::Rc::clone(&图齐了);
    // 等不到时得说得出是哪一项不满足：跑着几件、解出几张、有没有「没装 ffmpeg」那一档。
    let 图况 = std::rc::Rc::new(std::cell::Cell::new((usize::MAX, usize::MAX, false)));
    let 报况 = std::rc::Rc::clone(&图况);
    let mut harness = 开一个(主题, move |ui| {
        if let Some(面) = 要换.take() {
            app.browse_and_site().0.open_page(面);
        }
        app.ui(ui);
        let gallery = app.browse().gallery();
        报况.set((gallery.busy(), gallery.ready(), gallery.lacks_ffmpeg()));
        报图.set(gallery.busy() == 0 && gallery.ready() >= 1 && gallery.lacks_ffmpeg());
    });
    // 点一下那一行（同 [`按`]，但先不跑到停：后台解图时一直要重画，等图齐了再跑）。
    let Some(那一行) = 最后一处正好画着(harness.output(), 点开的作品那一行)
    else {
        panic!("{名字}：屏上没有「{点开的作品那一行}」那一行");
    };
    let 键 = |pressed: bool| egui::Event::PointerButton {
        pos: 那一行,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    harness.event(egui::Event::PointerMoved(那一行));
    harness.event(键(true));
    harness.event(键(false));
    harness.event(egui::Event::PointerGone);
    harness.step();
    换面.set(Some(面));
    harness.run_steps(2);
    // 等后台解完：只跑帧、不看挂钟（挂钟在忙机器上不稳，帧数各处一样）。
    // **上限给得明确**：够用就好，等不到是有毛病，不是慢——从前那十万帧要烧四五分钟才肯说话。
    let mut 跑了几帧 = 0;
    while !图齐了.get() && 跑了几帧 < 等图最多几帧 {
        harness.step();
        跑了几帧 += 1;
        std::thread::yield_now();
    }
    let (跑着, 解出, 缺编解码) = 图况.get();
    assert!(
        图齐了.get(),
        "{名字}：媒体池里那几份图一直没解完——跑满 {跑了几帧} 帧之后后台还跑着 {跑着} 件、\
         解出 {解出} 张、「没装 ffmpeg」那一档是 {缺编解码}（要的是：跑着 0、解出 ≥1、没装 ffmpeg 为真）"
    );
    harness.run();
    // **媒体面滚到整格看得见**（协调人 2026-09-15）：竖版封面那一格连底下「来源 · 大小」一行比头上那一块底下剩的地方高，
    // 像人一样把指针停在正文里往下滚一截再拍。
    if 面 == Tab::Media {
        harness.event(egui::Event::PointerMoved(egui::pos2(800.0, 600.0)));
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -220.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        });
        harness.event(egui::Event::PointerGone);
        harness.step();
        harness.run_steps(5);
        harness.run();
    }
    assert!(
        最后一处正好画着(harness.output(), "← 返回浏览").is_some(),
        "{名字}：作品详情页没打开，拍下来的不是它"
    );
    拍下(harness, 名字);
    drop(目录);
}

#[test]
fn 详情页_概览_浅色() {
    拍详情页("work/overview-light", Theme::Light, Tab::Overview);
}

#[test]
fn 详情页_概览_暗色() {
    拍详情页("work/overview-dark", Theme::Dark, Tab::Overview);
}

#[test]
fn 详情页_变体与文件_浅色() {
    拍详情页("work/variants-light", Theme::Light, Tab::Variants);
}

#[test]
fn 详情页_变体与文件_暗色() {
    拍详情页("work/variants-dark", Theme::Dark, Tab::Variants);
}

#[test]
fn 详情页_元数据_浅色() {
    拍详情页("work/metadata-light", Theme::Light, Tab::Metadata);
}

#[test]
fn 详情页_元数据_暗色() {
    拍详情页("work/metadata-dark", Theme::Dark, Tab::Metadata);
}

#[test]
fn 详情页_标题_浅色() {
    拍详情页("work/titles-light", Theme::Light, Tab::Titles);
}

#[test]
fn 详情页_标题_暗色() {
    拍详情页("work/titles-dark", Theme::Dark, Tab::Titles);
}

#[test]
fn 详情页_媒体_浅色() {
    拍详情页("work/media-light", Theme::Light, Tab::Media);
}

#[test]
fn 详情页_媒体_暗色() {
    拍详情页("work/media-dark", Theme::Dark, Tab::Media);
}

#[test]
fn 详情页_识别依据_浅色() {
    拍详情页("work/evidence-light", Theme::Light, Tab::Evidence);
}

#[test]
fn 详情页_识别依据_暗色() {
    拍详情页("work/evidence-dark", Theme::Dark, Tab::Evidence);
}

// ——— 合并作品与移出此作品 ———
//
// 两层都垫在浏览屏那份现场上（[`浏览现场`]）：屏上画着的每一样都是定值。
// 合并向导拍**三步各一对**，移出那一层拍**刚开那一下**（稿上 `st.mode='new'`：
// 「新建一个作品」选着、名字框里是默认名）。

/// 合并向导那几张勾的是**哪三个作品**。
///
/// 三个而不是两个，是为了让这几张图各自示范得出该示范的东西：
///
/// - **两个 SFC 的**（`Chrono Trigger` 与 `Seiken Densetsu 2`）：合并之后 SFC 那一侧
///   真的少一个前端条目——第三步那句「前端条目 N → M」这才示范得出「从多少变多少」。
///   只勾跨平台的两个时，收敛按**作品 × 平台**走，条目数一个不减（屏上是「8 → 8」），
///   而那是这张图唯一该说清的数。
/// - **外加一个 GBA 的**（`Gyakuten Saiban`）：第一步那条「这些作品分属不同平台」的提示、
///   第二步按平台分两组各挑一个首选，都靠它。
/// - **三个**还让第一步每一行右头那颗「移除」露出来（稿上 `m.ids.length>2` 才画）。
const 合并的那三个: [&str; 3] = [
    点开的作品,
    "Seiken Densetsu 2 (Japan)",
    "Gyakuten Saiban (Japan)",
];

/// 勾上 [`合并的那三个`]，不经表格——表上勾选框那一格在基线里是个小方块，
/// 点它要先滚到那一行，而这几张要看的是弹层。
fn 勾上那三个(app: &mut App) {
    let 行: Vec<romcat_core::catalog::browse::WorkAnchor> = {
        let (_, site) = app.browse_and_site();
        合并的那三个
            .iter()
            .map(|名字| {
                let id = site
                    .catalog
                    .work_named(名字)
                    .expect("读得动")
                    .expect("作品表里有它");
                romcat_core::catalog::browse::WorkAnchor::Work(id)
            })
            .collect()
    };
    let (browse, _) = app.browse_and_site();
    for anchor in 行 {
        browse.picked_mut().toggle(&anchor);
    }
}

/// 确认那一对拍多高（点）：第三步那一层装得下字段冲突表、两个勾选框与「合并后会发生什么」
/// 整块——**最后一条不许被页脚切半行**（截图门这一关的判据是**看得全**，同子库屏超限那一对
/// 与队列屏下钻那一对，模块文档「视口定死」那一节的例外）。底下那条断言把这件事写死。
const 确认那一对的画面: [f32; 2] = [1280.0, 960.0];

/// 给 SFC 那组第二个变体（`汉化/时空之轮 (简体中文 v1.2).zip`）记一条**带汉化记号**的
/// 已接受候选——**只给合并向导那几张**，不动别的屏的基线。
///
/// 为什么非有它不可：第二步那句说明写着默认选中的那一个是按「汉化 > 官中 > 日版 > 其他」
/// 选出来的，而这是这条规则在整个界面上**唯一**的示范。夹具里两个 SFC 变体身上一条汉化记号
/// 都没有时，核心库照规则答的是「两个都是原版、平手、按键排」——屏上两行都标「原版」，
/// 默认落在头一个，**与「取头一个」那种错做法一模一样**：图示范不出规则，断言也钉不住它。
///
/// 记号落在**候选**上而不是从文件名剥：「是哪一种」与首选变体同一处判（词表**变体简称**），
/// 文件名里那个「汉化」一个字都不作数。
fn 记上汉化记号(app: &mut App) {
    let (_, site) = app.browse_and_site();
    let key = "主库/SFC/汉化/时空之轮 (简体中文 v1.2).zip";
    let 原来的 = site
        .catalog
        .variant(key)
        .expect("读得动")
        .expect("夹具里有这一份");
    let mut 那一条 = 候选(
        &romcat_core::shape::Variant {
            key: key.to_owned(),
            main_key: key.to_owned(),
            platform: Some("SFC".to_owned()),
            rule: SINGLE_FILE_RULE.to_owned(),
            manual: false,
            files: 1,
            bytes: 原来的.bytes,
            unreadable_files: 0,
            members: Vec::new(),
        },
        true,
        Confidence::Medium,
        "中文离线源",
        "Chrono Trigger (Japan)",
        "名称模糊匹配，平台一致",
    );
    那一条.chinese = Some(romcat_core::dat::chinese::ChineseMark::FanTranslated);
    site.catalog
        .write_identifications(&[Identification {
            variant_key: key.to_owned(),
            state: State::Matched,
            reason: None,
            platform: None,
            standalone: None,
            edition: None,
            units: 1,
            nkit: 0,
            read_bytes: 0,
            work_id: 原来的.work_id,
            release_id: 原来的.release_id,
            candidates: vec![那一条],
        }])
        .expect("识别结论写得进");
}

/// **合并向导**走到第 `第几步` 步（从 1 数），拍一张。CI 上跳过（[`该跳过`]）。
#[track_caller]
fn 拍合并向导(名字: &str, 主题: Theme, 第几步: usize) {
    if 该跳过(名字) {
        return;
    }
    let 浏览现场 { mut app, 目录 } = 浏览现场(false);
    记上汉化记号(&mut app);
    勾上那三个(&mut app);
    // 第三步那一层比另外两步高一截，1280×800 装不下（见 [`确认那一对的画面`]）。
    let 画面 = if 第几步 == 3 {
        确认那一对的画面
    } else {
        headless::VIEWPORT
    };
    let mut harness = 开一扇(主题, 画面, move |ui| app.ui(ui));
    按(&mut harness, romcat_gui::browse::merge::MERGE);
    for _ in 1..第几步 {
        按(&mut harness, "下一步");
    }
    // **步骤条贯通左右、中间不断**（拿主意的人 2026-09-21 定，**与设计稿不同**）。
    //
    // 这里断的是**三段与两道连线之间的几何**，不写死弹层摆在哪儿：每一道连线的左端接着
    // 上一问名字的右缘、右端接着下一问圆点的左沿（两头各一道令牌里那个缝），而且**两道等长**。
    // 「中间断没断」才是「贯通」的直接判据——上一版按「几格等宽」摆，两端对上了、中间却
    // 空出一百九十点。
    //
    // **「两端就是这一层的左右内缘」另有一条钉着**
    // （`tests/dialog.rs::走到第几问那一排贯通左右而且中间不断`）：那一条量得到 `Shown::rect`，
    // 绝对位置在那儿断。这里不重复写死坐标——弹层摆在哪儿不是这一票的事。
    {
        let 每一问: Vec<egui::Rect> = ["选择作品", "核对变体", "确认合并"]
            .iter()
            .map(|那几个字| {
                let 每一处 = 正好画着的每一处(harness.output(), 那几个字);
                assert_eq!(每一处.len(), 1, "「{那几个字}」该正好画一段");
                每一处[0]
            })
            .collect();
        let 直径 = romcat_gui::tokens::Tokens::builtin().layout.page_dot;
        let 缝 = look::step(1);
        let 每一道 = 横线们(harness.output(), 每一问[0].center().y);
        assert_eq!(
            每一道.len(),
            2,
            "三问之间该有两道连线，画出了 {} 道",
            每一道.len(),
        );
        for (i, 这一道) in 每一道.iter().enumerate() {
            let 该从 = 每一问[i].right() + 缝;
            let 该到 = 每一问[i + 1].left() - 直径 - 缝 - 缝;
            assert!(
                (这一道.start() - 该从).abs() < 0.5 && (这一道.end() - 该到).abs() < 0.5,
                "第 {} 道连线没接满：画在 {:?}，该是 {该从}..={该到}",
                i + 1,
                这一道,
            );
        }
        let 长 = |这一道: &std::ops::RangeInclusive<f32>| 这一道.end() - 这一道.start();
        assert!(
            (长(&每一道[0]) - 长(&每一道[1])).abs() < 0.5,
            "两道连线不等长：{} 与 {}",
            长(&每一道[0]),
            长(&每一道[1]),
        );
    }
    // **对齐写成断言**（拿主意的人 2026-09-21 看图提的版式返工）：同一列那几段的左缘得全等。
    // 只靠基线拦不住——基线只说「与上次一样」，而上次也可能是歪的。
    match 第几步 {
        1 => 一列上对得齐(
            &弹层里的每一段(harness.output(), 画面, |text| text.contains(" 个变体 · ")),
            3,
            "第一步那三行的「平台 · 年份 · 几个变体 · 置信度」",
        ),
        2 => {
            一列上对得齐(
                &弹层里的每一段(harness.output(), 画面, |text| {
                    text == "保留作品自带" || text.starts_with("来自「")
                }),
                4,
                "第二步「来自哪儿」那一列",
            );
            // 置信度那一列写的是**四档里的哪一个词**（核心库 `Tier::label`，
            // 「没有候选」那一档不带「置信」两个字）——照四档认，别按字尾猜。
            let 四档: Vec<&str> = [Tier::High, Tier::Medium, Tier::Low, Tier::Unidentified]
                .iter()
                .map(|档| 档.label())
                .collect();
            一列上对得齐(
                &弹层里的每一段(harness.output(), 画面, |text| 四档.contains(&text)),
                4,
                "第二步置信度那一列",
            );
            一列上对得齐(
                &弹层里的每一段(harness.output(), 画面, |text| text.ends_with(" MiB")),
                4,
                "第二步体积那一列",
            );
        }
        _ => {
            // 字段冲突那张表**摊满这一层**：三列的表头就是那三列的左缘，头一列（字段）窄、
            // 后两列分余下的。由着 `Grid` 按内容收窄的话，三列会挤在左边三分之二里、
            // 右边空着一大块（那正是这次返工要治的）。
            let 表头 = |那几个字: &str| {
                let 每一处 = 弹层里的每一段(harness.output(), 画面, |text| text == 那几个字);
                assert_eq!(每一处.len(), 1, "表头「{那几个字}」该正好画一段");
                每一处[0]
            };
            let (字段, 保留, 其他) = (表头("字段"), 表头("保留作品"), 表头("其他作品"));
            let 头一列 = 保留.left() - 字段.left();
            let 第二列 = 其他.left() - 保留.left();
            assert!(
                第二列 > 头一列 * 2.0,
                "后两列没分掉余下的宽：字段那一列占 {头一列}，保留作品那一列只占 {第二列}",
            );
            // 每一列的值立成一列，而且与它的表头对齐：不是跟着前面的字流走。
            let 保留列 = 弹层里的每一段(harness.output(), 画面, |text| {
                ["超时空之钥", "1995", "Square", "角色扮演"].contains(&text)
            });
            一列上对得齐(&保留列, 5, "第三步「保留作品」那一列");
            assert!(
                (保留列[0].left() - 保留.left()).abs() < 24.0,
                "「保留作品」那一列的值（{}）没跟它的表头（{}）对齐",
                保留列[0].left(),
                保留.left(),
            );
        }
    }
    if 第几步 == 3 {
        // **看得全**：「会怎样」那几条里最后那一条整段都得在视口里，不许被页脚切掉半行。
        let 视口 = egui::Rect::from_min_size(egui::Pos2::ZERO, 画面.into());
        let 最后一条 = 画着的每一处含(harness.output(), "各自撤得掉");
        assert_eq!(
            最后一条.len(),
            1,
            "「会怎样」最后一条该正好画出一段来，画出了 {} 段",
            最后一条.len(),
        );
        assert!(
            视口.contains_rect(最后一条[0]),
            "「会怎样」最后一条被切了：{:?} 不在 {视口:?} 里",
            最后一条[0],
        );
    }
    拍下(harness, 名字);
    drop(目录);
}

#[test]
fn 合并向导_选择作品_浅色() {
    拍合并向导("merge/step1-light", Theme::Light, 1);
}

#[test]
fn 合并向导_选择作品_暗色() {
    拍合并向导("merge/step1-dark", Theme::Dark, 1);
}

#[test]
fn 合并向导_核对变体_浅色() {
    拍合并向导("merge/step2-light", Theme::Light, 2);
}

#[test]
fn 合并向导_核对变体_暗色() {
    拍合并向导("merge/step2-dark", Theme::Dark, 2);
}

#[test]
fn 合并向导_确认合并_浅色() {
    拍合并向导("merge/step3-light", Theme::Light, 3);
}

#[test]
fn 合并向导_确认合并_暗色() {
    拍合并向导("merge/step3-dark", Theme::Dark, 3);
}

/// **移出此作品**那一层：打开作品详情页停在「变体与文件」，按头一张变体卡上那颗
/// 「移出此作品…」（[`按`] 点的是**最后一处**，也就是最底下那张卡的那一颗）。
#[track_caller]
fn 拍移出此作品(名字: &str, 主题: Theme) {
    if 该跳过(名字) {
        return;
    }
    let 浏览现场 { mut app, 目录 } = 浏览现场(false);
    {
        let anchor = {
            let (_, site) = app.browse_and_site();
            romcat_core::catalog::browse::WorkAnchor::Work(
                site.catalog
                    .work_named(点开的作品)
                    .expect("读得动")
                    .expect("作品表里有它"),
            )
        };
        let (browse, site) = app.browse_and_site();
        browse.open_work(&site.catalog, &anchor);
        browse.open_page(Tab::Variants);
    }
    let mut harness = 开一个(主题, move |ui| app.ui(ui));
    按(&mut harness, romcat_gui::browse::merge::SPLIT);
    拍下(harness, 名字);
    drop(目录);
}

#[test]
fn 移出此作品弹层_浅色() {
    拍移出此作品("merge/split-light", Theme::Light);
}

#[test]
fn 移出此作品弹层_暗色() {
    拍移出此作品("merge/split-dark", Theme::Dark);
}

// ——— 库 ———

/// 库屏那几张基线用的现场：一份落在临时工作目录里的中立库，交出**整个窗口**（左栏、屏头加库屏的屏体）。
/// 临时目录跟着它活到拍完。
///
/// **屏上画着的都得是定值。** 变体数、容量、工序那几行说的话、数据源那几行照真库读——那几样只随摆进去的
/// 文件变，每一趟都一样。**根那几行**的路径、盘在不在位与上次扫描时刻由这里交进去
/// （`roots::Screen::list_roots`，与开场的 [`Screen::listed`] 同一个办法）：临时目录每一趟都是另一串，
/// 时刻是扫的那一刻。**等的是一趟扫描真跑完**（核心库那个入口是同步的），不是挂钟。底部状态栏上那一截工作目录
/// 同样是临时目录，定死成基线里那一串（`App::set_workspace_label`，同任务屏那几张）。
struct 库屏 {
    app: App,
    _工作区: TempDir,
    _盘: Vec<TempDir>,
}

impl 库屏 {
    /// 一份刚建出来、一个根都没有的库：库屏上每一块都是空态（票 `gui-looks-like-the-design/06` 验收第 4 条）。
    fn 空的() -> Self {
        let 工作区 = temp_dir("snapshot-库屏-空的");
        let site = 开库(工作区.path());
        let mut app = App::new(site, 工作区.path().to_path_buf());
        app.show_view(View::Library);
        app.roots_site_and_tasks().0.set_clock(库屏的钟());
        app.set_workspace_label(工作目录().display().to_string());
        Self {
            app,
            _工作区: 工作区,
            _盘: Vec::new(),
        }
    }

    /// 两个根都完整扫过一趟、识别还没跑：顶上指着识别，六行工序各报各的数，数据源都还没取回。
    ///
    /// 两个根照设计稿库屏那张表：主库在位，元数据库那块盘没接上。
    fn 扫过两个根() -> Self {
        let 工作区 = temp_dir("snapshot-库屏-扫过");
        let mut site = 开库(工作区.path());
        let 甲 = 摆一块盘(
            "snapshot-库屏-甲",
            &[
                ("SFC/幻想传说 汉化版.zip", 4_096),
                ("FC/魂斗罗.zip", 2_048),
                ("GBA/黄金太阳.zip", 8_192),
            ],
        );
        let 乙 = 摆一块盘("snapshot-库屏-乙", &[("MD/梦幻模拟战.zip", 3_072)]);
        for (根名, 盘, 扫于, 用时) in [
            // 东八区 2026-09-03 14:58（设计稿那一格），用时 37 分钟。
            ("主库", &甲, 1_788_418_680, 2_220_000),
            // 东八区 2026-08-29 21:10，用时 1 分 35 秒。
            ("元数据库", &乙, 1_788_009_000, 95_000),
        ] {
            let 目录 = romcat_core::path::normalize_existing(盘.path());
            roots::add_root(&site.catalog, Some(工作区.path()), 根名, &目录).expect("加得上根");
            let mut options = ScanOptions::named(&目录, 根名);
            options.workspace = Some(工作区.path().to_path_buf());
            options.jobs = Jobs::Fixed(1);
            scan::scan(&RealFs::new(), &mut site.catalog, &options, &Handle::new())
                .expect("扫得完");
            // **上次扫描那一格要是定值**：扫描自己记下的是扫完那一刻。别的几样（记了几个条目）照它记下的留着。
            let 记下的 = site
                .catalog
                .root(根名)
                .expect("读得出根")
                .and_then(|root| root.scan)
                .expect("扫完记下了上次扫描");
            site.catalog
                .record_root_scan(
                    根名,
                    &RootScan {
                        at: 扫于,
                        elapsed_ms: 用时,
                        ..记下的
                    },
                )
                .expect("记得下");
        }
        let mut app = App::new(site, 工作区.path().to_path_buf());
        app.show_view(View::Library);
        let (屏, _, _) = app.roots_site_and_tasks();
        let 画的: Vec<RootRow> = 屏
            .roots()
            .iter()
            .map(|row| {
                let (位置, 在位) = match row.root.name.as_str() {
                    "主库" => ("/Volumes/新加卷/Game", true),
                    _ => ("/Volumes/备份/Pegasus", false),
                };
                RootRow {
                    root: LibraryRoot {
                        path: 位置.to_owned(),
                        ..row.root.clone()
                    },
                    stats: row.stats,
                    mounted: 在位,
                }
            })
            .collect();
        屏.list_roots(画的);
        屏.set_clock(库屏的钟());
        app.set_workspace_label(工作目录().display().to_string());
        先体检一趟(&mut app);
        Self {
            app,
            _工作区: 工作区,
            _盘: vec![甲, 乙],
        }
    }

    /// 同一份扫过两个根的库，右边那三块都收着（验收第 3 条「可折叠」那一半的样子）。
    fn 三块收起() -> Self {
        let mut 现场 = Self::扫过两个根();
        let (屏, _, _) = 现场.app.roots_site_and_tasks();
        for 那一块 in [FOLD_ROOTS, FOLD_SOURCES, FOLD_EXPORT] {
            屏.set_folded(那一块, true);
        }
        现场
    }

    /// 一个根完整扫过一趟、**体检报告里有两组「目录说 A、内容是 B」**（票 `gui-looks-like-the-design/28`）：
    /// `fc/` 底下两份 `.fds`（FC 跑不了磁碟机的游戏，**只能改**），`3ds/` 底下一份头部字节是真的 `.nds`
    /// （3DS **向下兼容** NDS，**改不改都行**）。根那一行的路径与上次扫描时刻交成定值，同 [`Self::有体检发现`]。
    fn 有平台不符() -> Self {
        let 工作区 = temp_dir("snapshot-库屏-平台纠正");
        let site = 开库(工作区.path());
        let 盘 = temp_dir("snapshot-库屏-平台纠正盘");
        for (相对, 字节) in [
            ("fc/日版/塞尔达传说.fds", vec![1_u8; 64]),
            ("fc/合集/银河战士.fds", vec![2_u8; 64]),
            (
                "3ds/合集/雷顿教授与最后的时间旅行.nds",
                romcat_core::testing::cart::padded(
                    &romcat_core::testing::cart::NDS_GYAKUTEN_KENJI,
                    1 << 16,
                ),
            ),
        ] {
            let 落点 = 盘.path().join(相对);
            std::fs::create_dir_all(落点.parent().expect("有上级目录")).expect("建得出目录");
            std::fs::write(&落点, 字节).expect("写得进");
        }
        Self::扫一个根摆好(工作区, site, 盘)
    }

    /// 一个根完整扫过一趟、**体检报告里几格都有东西**（票 `gui-looks-like-the-design/27`）：两组重复拷贝、两面磁碟各成
    /// 一个变体、一份落单的存档、一份 BIOS、一个未纳入管理的目录。不可读与平台不符在临时目录里造不出来，那两格是 0。
    /// 根那一行的路径与上次扫描时刻交成定值，同 [`Self::扫过两个根`]。
    fn 有体检发现() -> Self {
        let 工作区 = temp_dir("snapshot-库屏-体检");
        let site = 开库(工作区.path());
        let 盘 = temp_dir("snapshot-库屏-体检盘");
        for (相对, 字节) in [
            ("FC/魂斗罗.zip", zip(2_048)),
            ("FC/备份/魂斗罗.zip", zip(2_048)),
            ("GBA/汉化/逆转裁判.gba", vec![7_u8; 8_192]),
            ("GBA/备份一/逆转裁判.gba", vec![7_u8; 8_192]),
            ("GBA/备份二/逆转裁判.gba", vec![7_u8; 8_192]),
            ("FDS/某游戏/某游戏 (Disk 1).fds", vec![1_u8; 64]),
            ("FDS/某游戏/某游戏 (Disk 2).fds", vec![2_u8; 64]),
            ("GBA/汉化/火焰之纹章.sav", vec![3_u8; 64]),
            ("PS1/bios/scph1001.bin", vec![4_u8; 512]),
            ("杂物/说明.txt", vec![5_u8; 16]),
        ] {
            let 落点 = 盘.path().join(相对);
            std::fs::create_dir_all(落点.parent().expect("有上级目录")).expect("建得出目录");
            std::fs::write(&落点, 字节).expect("写得进");
        }
        Self::扫一个根摆好(工作区, site, 盘)
    }
    /// 一个根完整扫过一趟、**体检报告里两种成型存疑各有一处**（票 `gui-looks-like-the-design/29`）：
    /// `FDS/某游戏/` 底下两面磁碟各成一个变体（**多碟没合在一起**），`ps3/动作合集/` 是一棵目录树、
    /// 却直接躺着两份各自独立的内容（**一个目录被当成一个变体**）。根那一行的路径与上次扫描时刻交成
    /// 定值，同 [`Self::有体检发现`]。
    fn 有成型存疑() -> Self {
        let 工作区 = temp_dir("snapshot-库屏-成型纠正");
        let site = 开库(工作区.path());
        let 盘 = temp_dir("snapshot-库屏-成型纠正盘");
        for (相对, 字节) in [
            ("FDS/最终幻想/最终幻想 (Disk 1).fds", vec![1_u8; 64]),
            ("FDS/最终幻想/最终幻想 (Disk 2).fds", vec![2_u8; 64]),
            ("ps3/动作游戏合集/PS3_GAME/USRDIR/EBOOT.BIN", vec![3_u8; 64]),
            ("ps3/动作游戏合集/怪物猎人 携带版.iso", vec![4_u8; 2_048]),
            ("ps3/动作游戏合集/铁拳 黑暗复苏.iso", vec![5_u8; 2_048]),
        ] {
            let 落点 = 盘.path().join(相对);
            std::fs::create_dir_all(落点.parent().expect("有上级目录")).expect("建得出目录");
            std::fs::write(&落点, 字节).expect("写得进");
        }
        Self::扫一个根摆好(工作区, site, 盘)
    }

    /// 一个根、一块盘：扫一遍、把根那一行的路径与上次扫描时刻交成定值、体检一趟。
    ///
    /// **三处 fixture 共用**（[`Self::有体检发现`]、[`Self::有平台不符`]、[`Self::有成型存疑`]）：
    /// 它们只在盘上摆什么不同，摆好之后那十几行逐字相同——抄第三遍时那几行就会各漂各的。
    fn 扫一个根摆好(工作区: TempDir, mut site: Site, 盘: TempDir) -> Self {
        let 目录 = romcat_core::path::normalize_existing(盘.path());
        roots::add_root(&site.catalog, Some(工作区.path()), "主库", &目录).expect("加得上根");
        let mut options = ScanOptions::named(&目录, "主库");
        options.workspace = Some(工作区.path().to_path_buf());
        options.jobs = Jobs::Fixed(1);
        scan::scan(&RealFs::new(), &mut site.catalog, &options, &Handle::new()).expect("扫得完");
        let 记下的 = site
            .catalog
            .root("主库")
            .expect("读得出根")
            .and_then(|root| root.scan)
            .expect("扫完记下了上次扫描");
        site.catalog
            .record_root_scan(
                "主库",
                &RootScan {
                    // 东八区 2026-09-03 14:58，用时 37 分钟（同扫过两个根那一份）。
                    at: 1_788_418_680,
                    elapsed_ms: 2_220_000,
                    ..记下的
                },
            )
            .expect("记得下");
        let mut app = App::new(site, 工作区.path().to_path_buf());
        app.show_view(View::Library);
        let (屏, _, _) = app.roots_site_and_tasks();
        let 画的: Vec<RootRow> = 屏
            .roots()
            .iter()
            .map(|row| RootRow {
                root: LibraryRoot {
                    path: "/Volumes/新加卷/Game".to_owned(),
                    ..row.root.clone()
                },
                stats: row.stats,
                mounted: true,
            })
            .collect();
        屏.list_roots(画的);
        屏.set_clock(库屏的钟());
        app.set_workspace_label(工作目录().display().to_string());
        先体检一趟(&mut app);
        Self {
            app,
            _工作区: 工作区,
            _盘: vec![盘],
        }
    }
}

/// **开窗之前先体检一趟、等它收场**（票 `gui-looks-like-the-design/27`）：有扫过的根、还没有报告时，库屏头一帧会自动排一趟
/// 体检上任务台，而台上有活时主窗口每一帧都要重画，截图就跑不到「不要重画」。「上次体检」照库屏那只定死的钟记。
/// **等的是台上空了这个信号**，一轮一轮问，不看挂钟。
fn 先体检一趟(app: &mut App) {
    let (屏, site, tasks) = app.roots_site_and_tasks();
    屏.check_health(site, tasks);
    等任务台空了(app);
}

/// **滚到库屏底下**：指针停在正文里、真发滚轮事件往下滚，每一下都跑到不要重画为止；滚到头之后再滚也不动，于是多滚几下不改
/// 那一帧的样子。最后把指针挪走——基线里不该有指针三角与悬停底色。库体检那一块在库屏最底下，1280×800 里要滚才看得见。
fn 滚到库屏底下(harness: &mut Harness<'_>) {
    let 指在 = egui::pos2(headless::VIEWPORT[0] * 0.6, headless::VIEWPORT[1] * 0.75);
    for _ in 0..20 {
        harness.event(egui::Event::PointerMoved(指在));
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -headless::VIEWPORT[1]),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run();
    }
    harness.event(egui::Event::PointerGone);
    harness.run();
}

/// 库屏那几张画时刻用的钟：此刻钉在 2026-09-14 08:00（UTC），本地钉在东八区。**截图里不许有当前时间**，
/// 「今年的不带年份」也不跟着跑测试的那一天变。
fn 库屏的钟() -> romcat_gui::clock::Clock {
    /// 2026-09-14 08:00（UTC）。
    const 此刻: i64 = 1_789_372_800;
    /// 东八区比 UTC 快八小时。
    const 东八区: i32 = 28_800;
    romcat_gui::clock::Clock::fixed(此刻, 东八区)
}

/// 在工作目录里建一份叫「主库」的中立库，开成现场。
fn 开库(工作区: &Path) -> Site {
    let 库文件 = 工作区.join("catalog").join("主库.sqlite3");
    drop(Catalog::create(&库文件, "主库").expect("建得出中立库"));
    Site::open_file(工作区, &库文件, None).expect("开得出现场")
}

/// 一块只往临时目录里写的「盘」：每份文件是一个定长的小 zip。**一个字节都不碰真盘。**
fn 摆一块盘(tag: &str, 文件: &[(&str, usize)]) -> TempDir {
    let 盘 = temp_dir(tag);
    for (相对, 大小) in 文件 {
        let 落点 = 盘.path().join(相对);
        std::fs::create_dir_all(落点.parent().expect("有上级目录")).expect("建得出目录");
        std::fs::write(&落点, zip(*大小)).expect("写得进");
    }
    盘
}

/// **库屏右边那一栏里每一颗按钮的外框都落在这一栏里**，而且点名的那几颗一颗不少。
///
/// 第二段头一版候选图上，根那张表与数据源那张表比这一栏宽，「重扫」「移除」「取回」被挤到栏外、屏上看不见，
/// 导出设置那一行的「记下」被截掉——读字的那几条测试一条都没抓到：按钮的字照样在那一帧的树里。这里读的是
/// 无障碍树里每颗按钮的**外框**（`egui_kittest` 的 `Node::rect`，逻辑坐标）。
///
/// 这一栏的两条边从屏上现量，不写像素：**右边**是屏体可见区的右沿减去令牌 `screen-body-padding` 左右那一份（右栏靠着这条
/// 内容边）；
/// **左边**是右栏头一块的标题往左让出面板内边距（令牌 `space.panel-padding`），再让一点给面板的描边。
/// 只量横向：右栏竖着长过窗口时整屏往下滚得到，那不算落在栏外。
#[track_caller]
fn 右栏的按钮都落在右栏里(
    harness: &Harness<'_>,
    右栏头一块的标题: &str,
    点名: &[(&str, usize)],
) {
    /// 面板描边的余量，点。描边宽度是 egui 的缺省线宽，令牌里没有这一格。
    const 描边余量: f32 = 1.5;
    // **右沿从屏体的可见区算**：屏体右沿（窗口的右沿）减去令牌 `screen-body-padding` 左右那一份——右栏与屏体靠同一条
    // 内容边（`look::screen_body`）。不再借屏头上那颗「添加根…」量：它挪进了屏头（票 `gui-looks-like-the-design/32`），
    // 屏头右侧那一段一帧摆两遍（先在看不见的地方量一遍宽），无障碍树里有两颗。
    let tokens = romcat_gui::tokens::Tokens::builtin();
    let [_, 屏体左右, _] = tokens.space.screen_body_padding;
    let 栏右 = harness.ctx.content_rect().max.x - 屏体左右;
    let [面板上下, 左右内边距] = tokens.space.panel_padding;
    let 标题 = harness.get_by_label(右栏头一块的标题).rect();
    let 栏左 = 标题.min.x - 左右内边距 - 描边余量;
    // 只量右栏里的按钮：右栏头一块的顶以下，屏头右侧那一段不算。
    let 栏顶 = 标题.min.y - 面板上下 - 描边余量;
    let 越界的: Vec<egui::Rect> = harness
        .query_all_by_role(Role::Button)
        .map(|node| node.rect())
        .filter(|外框| 外框.min.y >= 栏顶)
        .filter(|外框| 外框.center().x > 栏左)
        .filter(|外框| 外框.min.x < 栏左 || 外框.max.x > 栏右 + 描边余量)
        .collect();
    assert!(
        越界的.is_empty(),
        "右栏（{栏左:.1}..{栏右:.1}）里有按钮的外框落在栏外：{越界的:?}",
    );
    // **右栏没被撑宽**：导出设置那一块的「选择…」贴着那一块的右内边距。表格里哪一列比算好的宽，整栏就跟着宽出去、这颗按钮
    // 往右挪——第六版候选图扫过两个根那两张就是这样（表格每列默认至少 40 点，「变体」那一列被撑宽，整栏宽出 8 点），
    // 按钮外框却都还落在上面那个余量里，那一条没抓到。
    let 选择 = harness.get_by_role_and_label(Role::Button, "选择…").rect();
    let 该在 = 栏右 - 左右内边距;
    assert!(
        (选择.max.x - 该在).abs() <= 描边余量,
        "右栏被撑宽了：导出设置那一块「选择…」的右沿在 {:.1}，该在 {该在:.1}",
        选择.max.x,
    );
    // **表里的按钮那一列贴着表的右内边距**（令牌 `cell-padding` 左右那一份）：数据源那张表的「下载」与根那张表的「移除」
    // 右沿对齐，照稿。第八版候选图上「下载」那一列没贴右边，按钮右边空出一大截——上面两条都没抓到。
    let [_, 格子左右] = romcat_gui::tokens::Tokens::builtin().space.cell_padding;
    let 表右 = 栏右 - 格子左右;
    for 字 in ["下载", "移除"] {
        for node in harness.query_all_by_role_and_label(Role::Button, 字) {
            let 右沿 = node.rect().max.x;
            assert!(
                (右沿 - 表右).abs() <= 描边余量,
                "「{字}」的右沿在 {右沿:.1}，该贴着表的右内边距 {表右:.1}",
            );
        }
    }
    for (字, 几颗) in 点名 {
        // 只数右栏里的：工序段扫描那一行做完时也有一颗「重新扫描」，在左边那张卡里。
        let 数到 = harness
            .query_all_by_role_and_label(Role::Button, 字)
            .filter(|node| node.rect().center().x > 栏左)
            .count();
        assert_eq!(数到, *几颗, "右栏里写着「{字}」的按钮该有 {几颗} 颗");
    }
}

/// 屏上**正好**写着 `那几个字` 的每一处都**画成一行**，而且至少画了一处。
///
/// 第二段第三版候选图上，根那张表「变体」那一格的容量「14.00 KiB」被折成了三行：那一列沿用上一帧量出来的窄宽度，
/// 格子里的字默认又会折行。数、容量、「还没取回」这类字折开来就读不成一个数、一个词了。读的是那一帧画出来的
/// 那一段字排成了几行（`Galley` 的行数），与「字的外框高不过一行」是同一件事，不必另猜行高。
#[track_caller]
fn 画成一行(harness: &Harness<'_>, 那几个字: &str) {
    折行不超过(harness, 那几个字, 1);
}

/// 屏上**正好**写着 `那几个字` 的每一处都**最多折成 `最多几行` 行**，而且至少画了一处。读法同 [`画成一行`]。
///
/// 第十四版候选图上，根那张表「/Volumes/新加卷/Game」折成了四行、「/」一个人占一行：两颗按钮并排、根名与上次扫描
/// 那几列一挤，路径那一列只剩一个词宽。
#[track_caller]
fn 折行不超过(harness: &Harness<'_>, 那几个字: &str, 最多几行: usize) {
    let 行数: Vec<usize> = 画着的段(harness, 那几个字)
        .iter()
        .map(|galley| galley.rows.len())
        .collect();
    assert!(!行数.is_empty(), "屏上没画出「{那几个字}」");
    assert!(
        行数.iter().all(|几行| *几行 <= 最多几行),
        "「{那几个字}」该最多折成 {最多几行} 行，各处分别折成了 {行数:?} 行",
    );
}

/// 屏上**正好**写着 `那一句` 的每一处，折行时**末行至少两个字**（标点不算），而且至少画了一处。
///
/// 第十四版候选图上，空库时数据源那一句最后折出一个孤零零的「们。」——中文排版说的「孤字」。
#[track_caller]
fn 段末不留孤字(harness: &Harness<'_>, 那一句: &str) {
    let 各处 = 画着的段(harness, 那一句);
    assert!(!各处.is_empty(), "屏上没画出「{那一句}」");
    for galley in 各处 {
        let 末行: String = galley
            .rows
            .last()
            .map(|row| row.glyphs.iter().map(|glyph| glyph.chr).collect())
            .unwrap_or_default();
        let 字数 = 末行.chars().filter(|c| c.is_alphanumeric()).count();
        assert!(
            galley.rows.len() < 2 || 字数 >= 2,
            "「{那一句}」折成 {} 行，末行只剩「{末行}」",
            galley.rows.len(),
        );
    }
}

/// 这一帧里**正好**写着 `那几个字` 的每一段排好的字（`Galley`）。
fn 画着的段(harness: &Harness<'_>, 那几个字: &str) -> Vec<std::sync::Arc<egui::Galley>> {
    fn 收(
        shape: &egui::epaint::Shape,
        那几个字: &str,
        各段: &mut Vec<std::sync::Arc<egui::Galley>>,
    ) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                if text.galley.text() == 那几个字 {
                    各段.push(text.galley.clone());
                }
            }
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    收(one, 那几个字, 各段);
                }
            }
            _ => {}
        }
    }
    let mut 各段 = Vec::new();
    for clipped in &harness.output().shapes {
        收(&clipped.shape, 那几个字, &mut 各段);
    }
    各段
}

/// 一个根都没有时右栏里该看得见的那几颗：数据源标题栏的「全部下载」、三个源各一颗「下载」、导出设置的「选择…」。
/// 贴路径的表单照稿不在右栏里，加根在屏头「添加根…」。
const 空库右栏的按钮: &[(&str, usize)] = &[("全部下载", 1), ("下载", 3), ("选择…", 1)];

/// 扫过两个根时右栏里该看得见的那几颗：每个根一颗「重新扫描」一颗「移除」，外加空库时那几颗。
const 扫过的库右栏的按钮: &[(&str, usize)] = &[
    ("重新扫描", 2),
    (romcat_gui::roots::REMOVE, 2),
    ("全部下载", 1),
    ("下载", 3),
    ("选择…", 1),
];

#[test]
fn 库屏_空的_浅色() {
    const 名字: &str = "library/empty-light";
    if 该跳过(名字) {
        return;
    }
    let mut 现场 = 库屏::空的();
    let harness = 开一个(Theme::Light, move |ui| 现场.app.ui(ui));
    右栏的按钮都落在右栏里(&harness, "根", 空库右栏的按钮);
    // 数据源那张表记录数那一列不该折行。
    画成一行(&harness, "未下载");
    // 数据源那一块还没扫描时那一句，末行不只剩一个字。
    段末不留孤字(&harness, romcat_gui::roots::SOURCES_BEFORE_SCAN);
    拍下(harness, 名字);
}

#[test]
fn 库屏_空的_暗色() {
    const 名字: &str = "library/empty-dark";
    if 该跳过(名字) {
        return;
    }
    let mut 现场 = 库屏::空的();
    let harness = 开一个(Theme::Dark, move |ui| 现场.app.ui(ui));
    右栏的按钮都落在右栏里(&harness, "根", 空库右栏的按钮);
    // 数据源那张表记录数那一列不该折行。
    画成一行(&harness, "未下载");
    // 数据源那一块还没扫描时那一句，末行不只剩一个字。
    段末不留孤字(&harness, romcat_gui::roots::SOURCES_BEFORE_SCAN);
    拍下(harness, 名字);
}

#[test]
fn 库屏_扫过两个根_浅色() {
    const 名字: &str = "library/scanned-light";
    if 该跳过(名字) {
        return;
    }
    let mut 现场 = 库屏::扫过两个根();
    let harness = 开一个(Theme::Light, move |ui| 现场.app.ui(ui));
    右栏的按钮都落在右栏里(&harness, "根", 扫过的库右栏的按钮);
    // 不该折行的字画成一行：上次扫描那一刻、数据源那张表记录数那一列。
    画成一行(&harness, "09-03 14:58");
    画成一行(&harness, "未下载");
    // 根那张表的路径最多折两行（稿上同样的宽度折成两行）。
    折行不超过(&harness, "/Volumes/新加卷/Game", 2);
    折行不超过(&harness, "/Volumes/备份/Pegasus", 2);
    拍下(harness, 名字);
}

#[test]
fn 库屏_扫过两个根_暗色() {
    const 名字: &str = "library/scanned-dark";
    if 该跳过(名字) {
        return;
    }
    let mut 现场 = 库屏::扫过两个根();
    let harness = 开一个(Theme::Dark, move |ui| 现场.app.ui(ui));
    右栏的按钮都落在右栏里(&harness, "根", 扫过的库右栏的按钮);
    // 不该折行的字画成一行：上次扫描那一刻、数据源那张表记录数那一列。
    画成一行(&harness, "09-03 14:58");
    画成一行(&harness, "未下载");
    // 根那张表的路径最多折两行（稿上同样的宽度折成两行）。
    折行不超过(&harness, "/Volumes/新加卷/Game", 2);
    折行不超过(&harness, "/Volumes/备份/Pegasus", 2);
    拍下(harness, 名字);
}

#[test]
fn 库屏_三块收起_浅色() {
    let mut 现场 = 库屏::三块收起();
    拍("library/folded-light", Theme::Light, move |ui| {
        现场.app.ui(ui)
    });
}

#[test]
fn 库屏_三块收起_暗色() {
    let mut 现场 = 库屏::三块收起();
    拍("library/folded-dark", Theme::Dark, move |ui| {
        现场.app.ui(ui)
    });
}

/// **库体检那一块**（票 `gui-looks-like-the-design/27`）：一个根扫过、体检过，八格各有数；滚到库屏底下拍。
fn 拍体检(名字: &str, 主题: Theme) {
    if 该跳过(名字) {
        return;
    }
    let mut 现场 = 库屏::有体检发现();
    let mut harness = 开一个(主题, move |ui| 现场.app.ui(ui));
    滚到库屏底下(&mut harness);
    拍下(harness, 名字);
}

#[test]
fn 库屏_体检_浅色() {
    拍体检("library/health-light", Theme::Light);
}

#[test]
fn 库屏_体检_暗色() {
    拍体检("library/health-dark", Theme::Dark);
}

/// **重复拷贝明细弹层**：同一份库，滚到库屏底下按「重复拷贝」那一格（照「目标设置弹层」那两张的写法：[`开一个`] + [`按`]）。
fn 拍重复拷贝明细(名字: &str, 主题: Theme) {
    if 该跳过(名字) {
        return;
    }
    let mut 现场 = 库屏::有体检发现();
    let mut harness = 开一个(主题, move |ui| 现场.app.ui(ui));
    滚到库屏底下(&mut harness);
    按(&mut harness, "重复拷贝");
    拍下(harness, 名字);
}

/// **平台纠正那一层**（票 `gui-looks-like-the-design/28`，设计稿 `DLG.platfix`）：滚到库屏底下按
/// 「目录与内容平台不符」那一格——它点进去开的不是明细弹层，是这一层。
///
/// **拍的是刚开那一下**：两组都还没定，各摆着「按内容改」与「保持」两颗按钮，改不改都行的那一组
/// 底下多一句——稿上就是这个样子，而「每组两条出路」正是这一票的验收第 2 条。
fn 拍平台纠正(名字: &str, 主题: Theme) {
    if 该跳过(名字) {
        return;
    }
    let mut 现场 = 库屏::有平台不符();
    let mut harness = 开一个(主题, move |ui| 现场.app.ui(ui));
    滚到库屏底下(&mut harness);
    按(&mut harness, "目录与内容平台不符");
    拍下(harness, 名字);
}

#[test]
fn 库屏_平台纠正_浅色() {
    拍平台纠正("library/platfix-light", Theme::Light);
}

#[test]
fn 库屏_平台纠正_暗色() {
    拍平台纠正("library/platfix-dark", Theme::Dark);
}

/// **成型存疑那一格的明细**（票 `gui-looks-like-the-design/29`，设计稿 `DLG.shapes`）：滚到库屏底下
/// 按「成型存疑」那一格——一行一处，路径在上、凭什么在下，右头一颗「处理…」。
///
/// **拍的是刚开那一下**：两处都还没处理，各摆着一颗「处理…」——那正是这一票验收第 1 条前半句
/// （从库体检的「成型存疑」进得去）与第 6 条（逐处确认，不一次性全改）的样子。
fn 拍成型存疑(名字: &str, 主题: Theme) {
    if 该跳过(名字) {
        return;
    }
    let mut 现场 = 库屏::有成型存疑();
    let mut harness = 开一个(主题, move |ui| 现场.app.ui(ui));
    滚到库屏底下(&mut harness);
    按(&mut harness, "成型存疑");
    拍下(harness, 名字);
}

/// **调整成型 · 多碟那一支**（设计稿 `DLG.shape` 的 `disc` 那一支）：明细里**头一处**那颗「处理…」
/// ——同一个目录里两面磁碟各成一个变体。勾选、线索、纠正后的主文件与附属文件都在这一屏里。
fn 拍合成多碟(名字: &str, 主题: Theme) {
    if 该跳过(名字) {
        return;
    }
    let mut 现场 = 库屏::有成型存疑();
    let mut harness = 开一个(主题, move |ui| 现场.app.ui(ui));
    滚到库屏底下(&mut harness);
    按(&mut harness, "成型存疑");
    按头一处(&mut harness, "处理…");
    拍下(harness, 名字);
}

/// **调整成型 · 目录那一支**（设计稿 `DLG.shape` 的 `dir` 那一支）：明细里**最后**那颗「处理…」
/// ——一棵目录树里直接躺着两份各自独立的内容。
fn 拍拆开目录(名字: &str, 主题: Theme) {
    if 该跳过(名字) {
        return;
    }
    let mut 现场 = 库屏::有成型存疑();
    let mut harness = 开一个(主题, move |ui| 现场.app.ui(ui));
    滚到库屏底下(&mut harness);
    按(&mut harness, "成型存疑");
    按(&mut harness, "处理…");
    拍下(harness, 名字);
}

#[test]
fn 库屏_成型存疑明细_浅色() {
    拍成型存疑("library/shaping-doubts-light", Theme::Light);
}

#[test]
fn 库屏_成型存疑明细_暗色() {
    拍成型存疑("library/shaping-doubts-dark", Theme::Dark);
}

#[test]
fn 库屏_调整成型_合成多碟_浅色() {
    拍合成多碟("library/shaping-merge-light", Theme::Light);
}

#[test]
fn 库屏_调整成型_合成多碟_暗色() {
    拍合成多碟("library/shaping-merge-dark", Theme::Dark);
}

#[test]
fn 库屏_调整成型_拆开目录_浅色() {
    拍拆开目录("library/shaping-split-light", Theme::Light);
}

#[test]
fn 库屏_调整成型_拆开目录_暗色() {
    拍拆开目录("library/shaping-split-dark", Theme::Dark);
}

#[test]
fn 库屏_重复拷贝明细弹层_浅色() {
    拍重复拷贝明细("library/health-duplicates-light", Theme::Light);
}

#[test]
fn 库屏_重复拷贝明细弹层_暗色() {
    拍重复拷贝明细("library/health-duplicates-dark", Theme::Dark);
}

/// **「移除根」那一层**（票 `gui-looks-like-the-design/26`，设计稿 `DLG.rmroot`）：同一份扫过两个根的库，
/// 按第二个根那一行的「移除…」——[`按`] 点的是**最后一处**写着那几个字的地方，正好是稿上那一行
/// （盘不在位的「元数据库」）。
///
/// **拍的是刚开那一下**：那一格还没勾上，页脚「移除根」按不动——稿上 `st.ok=false` 就是这个样子，
/// 而「勾了才按得动」正是这一票的验收第 3 条。
fn 拍移除根(名字: &str, 主题: Theme) {
    if 该跳过(名字) {
        return;
    }
    let mut 现场 = 库屏::扫过两个根();
    let mut harness = 开一个(主题, move |ui| 现场.app.ui(ui));
    按(&mut harness, romcat_gui::roots::REMOVE);
    拍下(harness, 名字);
}

#[test]
fn 库屏_移除根弹层_浅色() {
    拍移除根("library/remove-root-light", Theme::Light);
}

#[test]
fn 库屏_移除根弹层_暗色() {
    拍移除根("library/remove-root-dark", Theme::Dark);
}

// ——— 子库 ———
//
// 子库屏那几张拍的是**整个窗口**（`App`：左栏、屏头连子库屏）。屏上画着的每一样都得是定值（票
// `gui-looks-like-the-design/20`）：
//
// - **主库**扫进一份**内存里的**中立库，变体的键是「库/<平台>/<名字>」，与临时目录落在哪儿无关；
// - **卡头上的目标路径**是一串定值（`/Volumes/…`）。要看得见卡上现占多少的那一台，读盘那一份
//   （`target_raw`）指向一个临时目录——屏上只画 `target`、读盘只走 `target_raw`（ADR-0020 那两份形式），
//   于是像素里没有临时路径；
// - **工作目录**是临时目录，这一屏不画它；
// - 容量是「算一遍容量」**真算出来**的：内存里的库分不出第二份连接，那一趟就地跑完。

/// 子库那几张的现场：几个临时目录（跟着窗口一起活到拍完）与窗口本身。
struct 子库现场 {
    主库: TempDir,
    /// 第二个**根**（主库是一组根）：只有差量异常那两对要它——两个根里同一条相对路径
    /// 剥掉根名之后落在卡上同一个文件上，那正是**落点撞车**。别的几张摆 `None`。
    _另一块盘: Option<TempDir>,
    _工作区: TempDir,
    卡: TempDir,
    app: App,
}

impl 子库现场 {
    /// 扫好一份小主库、开出窗口、换到子库屏。一个子库都还没有。
    fn 摆好() -> Self {
        Self::摆好带(None)
    }

    /// 同 [`Self::摆好`]，外加**第二个根**：里头摆着 `又一份` 那几条相对路径，与头一个根
    /// 扫进**同一份中立库**（主库是一组根）。两个根里同一条相对路径会在差量预览里撞车。
    fn 摆好带另一块盘(又一份: &[(&str, usize)]) -> Self {
        let 另一块盘 = temp_dir("snap-sub-lib2");
        for (相对, 多大) in 又一份 {
            let 在 = 另一块盘.path().join(相对);
            std::fs::create_dir_all(在.parent().expect("有上级目录")).expect("能建目录");
            std::fs::write(&在, zip(*多大)).expect("能写文件");
        }
        Self::摆好带(Some(另一块盘))
    }

    fn 摆好带(另一块盘: Option<TempDir>) -> Self {
        let 主库 = temp_dir("snap-sub-lib");
        for (相对, 多大) in [
            ("SFC/幻想传说 汉化版.zip", 4096),
            ("SFC/圣剑传说 3 汉化版.zip", 8192),
            ("GBA/口袋妖怪 绿宝石.zip", 2048),
            ("GBA/黄金太阳 开启的封印.zip", 3072),
        ] {
            let 在 = 主库.path().join(相对);
            std::fs::create_dir_all(在.parent().expect("有上级目录")).expect("能建目录");
            std::fs::write(&在, zip(多大)).expect("能写文件");
        }
        let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
        let mut roots: Vec<(&str, &Path)> = vec![("库", 主库.path())];
        if let Some(第二个) = &另一块盘 {
            roots.push(("另一块盘", 第二个.path()));
        }
        for (名字, 根) in roots {
            let mut options = ScanOptions::named(根, 名字);
            options.jobs = Jobs::Fixed(2);
            scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");
        }
        let site = Site::in_memory(catalog, Store::in_memory().expect("开得出沉淀库"), "库");
        let 工作区 = temp_dir("snap-sub-ws");
        let 卡 = temp_dir("snap-sub-card");
        // 维护者自己拷进卡里的一份存档：清单之外那一段有一个定值。
        std::fs::write(卡.path().join("存档.sav"), [0_u8; 1536]).expect("能写存档");
        let mut app = App::new(site, 工作区.path().to_path_buf());
        // 底部状态栏右边那一段工作目录定死成基线里那一串（同任务屏、主窗口那几张）：临时目录每一趟都不一样。
        app.set_workspace_label(工作目录().display().to_string());
        app.show_view(View::Sublibraries);
        Self {
            主库,
            _另一块盘: 另一块盘,
            _工作区: 工作区,
            卡,
            app,
        }
    }

    /// 记一台设备。`在手边` 为真时读盘那一份指向临时卡目录；卡头上画的一律是 `目标` 那串定值。
    fn 记一台(
        &mut self,
        名字: &str,
        目标: &str,
        在手边: bool,
        上限: Option<u64>,
        档案: Option<&str>,
        规则: &[&str],
    ) {
        let target_raw = 在手边.then(|| {
            self.卡
                .path()
                .to_str()
                .expect("临时目录是 UTF-8")
                .to_string()
        });
        let (screen, site) = self.app.sublibrary_and_site();
        site.catalog
            .put_sublibrary(&Sublibrary {
                name: 名字.to_owned(),
                target: 目标.to_owned(),
                target_raw,
                format: "Pegasus".to_owned(),
                capacity: 上限,
                capability: 档案.map(ToString::to_string),
                capacity_by_device: false,
            })
            .expect("写得进子库");
        for 那条 in 规则 {
            site.catalog
                .add_rule(名字, &Rule::parse(那条).expect("读得懂"))
                .expect("写得进规则");
        }
        screen.reload(site);
    }

    /// 换一台设备的容量上限，别的一格不动。
    fn 换上限(&mut self, 名字: &str, 上限: Option<u64>) {
        let (screen, site) = self.app.sublibrary_and_site();
        let mut 那一台 = site.catalog.sublibrary(名字).expect("读得动").expect("在");
        那一台.capacity = 上限;
        site.catalog.put_sublibrary(&那一台).expect("写得进子库");
        screen.reload(site);
    }

    /// 把几个变体挂到同一个作品底下（识别结论）——例外那张表的「作品」一列才写得出名字。
    fn 挂到作品(&mut self, 作品: &str, keys: &[&str]) {
        let (screen, site) = self.app.sublibrary_and_site();
        let work = site
            .catalog
            .add_work(作品, Provenance::Identified)
            .expect("建得出作品");
        let records: Vec<Identification> = keys
            .iter()
            .map(|key| Identification {
                variant_key: (*key).to_string(),
                platform: None,
                standalone: None,
                edition: None,
                state: State::Matched,
                reason: None,
                units: 1,
                nkit: 0,
                read_bytes: 0,
                work_id: Some(work),
                release_id: None,
                candidates: Vec::new(),
            })
            .collect();
        site.catalog
            .write_identifications(&records)
            .expect("识别结论写得进");
        screen.reload(site);
    }

    /// 往库里记一条例外。
    fn 记一条例外(&mut self, name: &str, key: &str, kind: Exception, note: Option<&str>) {
        let (screen, site) = self.app.sublibrary_and_site();
        site.catalog
            .set_exception(name, key, kind, note)
            .expect("例外写得进");
        screen.reload(site);
    }

    /// 直接把一份**清单**摆进中立库（`Catalog::put_manifest`）。
    ///
    /// **不走真的同步**：同步会往屏上写一句「同步用了 0.0 秒」，那个数跟着挂钟走，
    /// 基线里就不定了。三类异常是摆出来的、不是跑出来的——它们各自由核心与界面那几条
    /// 测试钉着，这一对要的是**画成什么样**。
    fn 记一份清单(&mut self, name: &str, 几条: &[(&str, u64)]) {
        use romcat_core::sync::{FileKind, Manifest, ManifestFile, Stamp};

        let files = 几条
            .iter()
            .map(|(相对, 多大)| ManifestFile {
                path: (*相对).to_string(),
                kind: FileKind::Rom,
                stamp: Stamp {
                    bytes: *多大,
                    mtime_ns: None,
                },
                source: format!("库/{相对}"),
                source_stamp: Stamp {
                    bytes: *多大,
                    mtime_ns: None,
                },
                variant: format!("库/{相对}"),
                absent: false,
            })
            .collect();
        let (screen, site) = self.app.sublibrary_and_site();
        site.catalog
            .put_manifest(name, &Manifest { files })
            .expect("清单写得进");
        screen.reload(site);
    }

    /// 摊开一张卡：差量预览摆在摊开那一张底下。
    fn 摊开(&mut self, name: &str) {
        let (screen, site) = self.app.sublibrary_and_site();
        screen.open(site, name);
    }

    /// 排一趟差量预览，等它收回来；跟着把「排它用了 N ms」钉死。
    fn 排一遍差量(&mut self) {
        {
            let (screen, site, tasks) = self.app.sublibrary_site_and_tasks();
            screen.preview(site, tasks);
        }
        for _ in 0..8 {
            self.app.poll_tasks();
            if self.app.sublibrary().previewing().is_none() && !self.app.tasks().busy() {
                break;
            }
        }
        let screen = self.app.sublibrary_and_site().0;
        assert!(
            screen.prepared().is_some(),
            "差量没排出来：{:?}",
            screen.error(),
        );
        screen.pin_prepare_ms(排它用了多少毫秒);
    }

    /// 按一下「算一遍容量」，等它收回来。内存里的库就地跑完，认领在 `App::poll_tasks` 里。
    fn 算一遍容量(&mut self) {
        {
            let (screen, site, tasks) = self.app.sublibrary_site_and_tasks();
            screen.evaluate(site, tasks);
        }
        for _ in 0..8 {
            self.app.poll_tasks();
            if self.app.sublibrary().evaluating().is_none() && !self.app.tasks().busy() {
                break;
            }
        }
        assert!(
            self.app.sublibrary().evaluating().is_none(),
            "算一遍容量没收回来：{:?}",
            self.app.sublibrary().error(),
        );
    }
}

/// 「卡不在手边」那一台卡头上画的目标路径——**这一台真去读盘**：它没有 `target_raw`，`read_path` 就是这一串，
/// 卡头那枚「未连接」与容量底下那一行都照「这个目录在不在」说话（`Screen::look_at_targets`）。
///
/// 所以它得保证**哪台机器上都不在**。原先用的 `/Volumes/SDCARD` 是掌机存储卡最常见的卷名，维护者正好
/// 插着一张叫 SDCARD 的卡时，那枚标签就换成「尚未生成差量预览」、那一行也没了——基线莫名其妙变红，
/// 而界面一点毛病都没有。取一个真卡不会起的卷名。卡在手边的那几台读的是 `target_raw` 指的临时目录，
/// 卡头上画的那串路径不去读，照旧用常见的写法。
const 不在位的目标: &str = "/Volumes/ROMCAT-NO-SUCH-CARD";

/// **两台设备**，两台都算过一遍容量：一台卡不在手边（清单之外画成未知），一台卡在手边
/// （清单之外有数、挑了一份能力档案）。头一台两条规则互相重叠，合计那一行写得出去掉了几个。
fn 两台设备() -> 子库现场 {
    let mut 现场 = 子库现场::摆好();
    现场.记一台(
        "RG35XX Plus",
        不在位的目标,
        false,
        Some(64_000_000_000),
        None,
        &["平台=SFC", "平台=SFC,GBA"],
    );
    现场.记一台(
        "Retroid Pocket 5",
        "/Volumes/RP5",
        true,
        Some(128_000_000_000),
        Some("retroarch-fat32"),
        &["平台=GBA"],
    );
    现场.算一遍容量();
    现场
}

/// **超限的一台**：上限正好比同步之后少「最大那一个」那么多——删减建议表排除到头一项就放得下。
/// 同步之后多大先不设限算一遍，由核心说（目标现占 ＋ 净变化）。
fn 超限的一台() -> 子库现场 {
    const 名字: &str = "RG35XX Plus";
    let mut 现场 = 子库现场::摆好();
    现场.记一台(名字, "/Volumes/SDCARD", true, None, None, &["平台=SFC,GBA"]);
    现场.算一遍容量();
    let 同步之后 = 现场
        .app
        .sublibrary()
        .evaluated(名字)
        .and_then(|report| report.fit.known())
        .expect("卡在手边，装不装得下算得出")
        .after_bytes;
    let 最大的 = std::fs::metadata(现场.主库.path().join("SFC/圣剑传说 3 汉化版.zip"))
        .expect("在")
        .len();
    现场.换上限(名字, Some(同步之后 - 最大的));
    现场.算一遍容量();
    现场
}

#[test]
fn 子库_空的_浅色() {
    let mut 现场 = 子库现场::摆好();
    拍("sublibrary/empty-light", Theme::Light, move |ui| {
        现场.app.ui(ui);
    });
}

#[test]
fn 子库_空的_暗色() {
    let mut 现场 = 子库现场::摆好();
    拍("sublibrary/empty-dark", Theme::Dark, move |ui| {
        现场.app.ui(ui);
    });
}

#[test]
fn 子库_两台设备_浅色() {
    let mut 现场 = 两台设备();
    拍("sublibrary/cards-light", Theme::Light, move |ui| {
        现场.app.ui(ui);
    });
}

#[test]
fn 子库_两台设备_暗色() {
    let mut 现场 = 两台设备();
    拍("sublibrary/cards-dark", Theme::Dark, move |ui| {
        现场.app.ui(ui);
    });
}

/// 超限那一对的画面（点）：宽照旧，高 960——整张卡连灰框与按钮都要拍全（拿主意的人 2026-09-14 定，挂单 `Q895`）。
const 超限那一对的画面: [f32; 2] = [1280.0, 960.0];

/// **超限的一台**那一对：画面 1280×960（[`超限那一对的画面`]），卡片顶边贴着可视区顶部（不滚），整张卡连删减表、灰框与按钮都在画面里。
///
/// 早先是把卡片那一列滚到底再拍：滚动位置夹在最大那一格上，图顶上「选择集 · 1 条规则」那一行只露出下半截，
/// 字被切掉一半、看着像画坏了（协调人对稿打回，票 `gui-looks-like-the-design/20` 第二段）。改成不滚之后 800 高里灰框与按钮
/// 落在画面外，拿主意的人定这一对用高一点的画面（挂单 `Q895`）。
///
/// **「看得全」写成断言**：删减表有几项，屏上就得画出几颗「排除」，而且每一颗都整个在视口里（egui 不画整个
/// 落在裁剪区外的控件，被挤出去的那几行一颗都不会画）；卡片的名字画在屏头底下。哪天卡片长高、表被挤出视口，
/// 这里当场红，不会悄悄拍一张截掉半张表的基线。不滚，也就没有要等它停下的滚动。
fn 拍超限(名字: &str, 主题: Theme) {
    const 那一台: &str = "RG35XX Plus";
    if 该跳过(名字) {
        return;
    }
    let mut 现场 = 超限的一台();
    let 几项 = 现场
        .app
        .sublibrary()
        .evaluated(那一台)
        .and_then(|report| report.fit.known())
        .expect("卡在手边，装不装得下算得出")
        .trim_suggestions
        .len();
    let harness = 开一扇(主题, 超限那一对的画面, move |ui| {
        现场.app.ui(ui);
    });
    let 视口 = egui::Rect::from_min_size(egui::Pos2::ZERO, 超限那一对的画面.into());
    let 排除 = 正好画着的每一处(harness.output(), "排除");
    assert_eq!(
        排除.len(),
        几项,
        "删减表 {几项} 项，屏上只画出 {} 颗「排除」——有几行被挤出了视口",
        排除.len()
    );
    assert!(
        排除.iter().all(|rect| 视口.contains_rect(*rect)),
        "删减表有一行被视口截了一半：{排除:?}"
    );
    let 屏头 = 正好画着的每一处(harness.output(), "新建子库");
    let 卡名 = 正好画着的每一处(harness.output(), 那一台);
    assert!(
        matches!((屏头.first(), 卡名.first()), (Some(头), Some(名)) if 名.min.y >= 头.max.y),
        "卡片顶上那一截没整个露出来：屏头 {屏头:?}，卡名 {卡名:?}"
    );
    // 灰框底下那一排按钮也整个在画面里：卡片一张拍全了。
    for 按钮 in ["生成差量预览", "删除子库"] {
        let 在 = 正好画着的每一处(harness.output(), 按钮);
        assert!(
            在.len() == 1 && 视口.contains_rect(在[0]),
            "卡底那一排「{按钮}」没整个在画面里：{在:?}"
        );
    }
    拍下(harness, 名字);
}

#[test]
fn 子库_超限给删减建议_浅色() {
    拍超限("sublibrary/over-capacity-light", Theme::Light);
}

#[test]
fn 子库_超限给删减建议_暗色() {
    拍超限("sublibrary/over-capacity-dark", Theme::Dark);
}

/// **目标设置弹层**：两台设备那一屏上，按右边那张卡（「Retroid Pocket 5」）的「目标设置…」。
/// 照开场「添加主库向导盖在上面」那张的写法：[`开一个`] + [`按`]，再 [`拍下`]。
fn 拍目标设置弹层(名字: &str, 主题: Theme) {
    if 该跳过(名字) {
        return;
    }
    let mut 现场 = 钉死今天(两台设备());
    let mut harness = 开一个(主题, move |ui| {
        现场.app.ui(ui);
    });
    按(&mut harness, "目标设置…");
    拍下(harness, 名字);
}

/// 平台表判「陈旧」用的「今天」钉死（票 `gui-looks-like-the-design/21`）：截图里才没有当前日期。内置档案的核实日期是
/// 2026-08-31，钉在 2026-09-15 不陈旧。
fn 钉死今天(mut 现场: 子库现场) -> 子库现场 {
    现场.app.sublibrary_and_site().0.set_today("2026-09-15");
    现场
}

/// **目标设置弹层滚到底**（拿主意的人 2026-09-15 定，F11）：同上打开右边那张卡的「目标设置…」，在弹层内容区上滚到底再拍——
/// 能力档案表、容量上限与「设备上的位置」都在这一张里。
///
/// **「看得全」写成断言**：「设备上的位置」正好画了一处、整个在视口里。哪天弹层内容长到滚不下、或者滚动没停下来就拍，
/// 这里当场红。
fn 拍目标设置弹层滚到底(名字: &str, 主题: Theme) {
    if 该跳过(名字) {
        return;
    }
    let mut 现场 = 钉死今天(两台设备());
    let mut harness = 开一个(主题, move |ui| {
        现场.app.ui(ui);
    });
    按(&mut harness, "目标设置…");
    let 正中 = egui::pos2(headless::VIEWPORT[0] / 2.0, headless::VIEWPORT[1] / 2.0);
    harness.event(egui::Event::PointerMoved(正中));
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -100_000.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    // egui 的滚轮带平滑，一下要分好几帧走完：`Harness::run` 最多跑四步，碰上它就停不下来。逐帧跑到
    // `InputState::is_scrolling` 说停了为止（与 `tests/sublibrary.rs` 的 `滚一下` 同一个等法），不看挂钟。
    for _ in 0..240 {
        harness.step();
        if !harness.ctx.input(|input| input.is_scrolling()) {
            break;
        }
    }
    harness.event(egui::Event::PointerGone);
    harness.run();
    let 视口 = egui::Rect::from_min_size(egui::Pos2::ZERO, headless::VIEWPORT.into());
    let 位置 = 正好画着的每一处(harness.output(), "设备上的位置");
    assert!(
        位置.len() == 1 && 视口.contains_rect(位置[0]),
        "滚到底了「设备上的位置」却没整个在画面里：{位置:?}"
    );
    拍下(harness, 名字);
}

/// **新建子库弹层**（F11）：屏头「新建子库」按下去，停在弹层顶部。
fn 拍新建子库弹层(名字: &str, 主题: Theme) {
    if 该跳过(名字) {
        return;
    }
    let mut 现场 = 钉死今天(两台设备());
    let mut harness = 开一个(主题, move |ui| {
        现场.app.ui(ui);
    });
    按(&mut harness, "新建子库");
    拍下(harness, 名字);
}

#[test]
fn 子库_目标设置弹层_浅色() {
    拍目标设置弹层("sublibrary/target-settings-light", Theme::Light);
}

#[test]
fn 子库_目标设置弹层_暗色() {
    拍目标设置弹层("sublibrary/target-settings-dark", Theme::Dark);
}

#[test]
fn 子库_目标设置弹层滚到底_浅色() {
    拍目标设置弹层滚到底("sublibrary/target-settings-bottom-light", Theme::Light);
}

#[test]
fn 子库_目标设置弹层滚到底_暗色() {
    拍目标设置弹层滚到底("sublibrary/target-settings-bottom-dark", Theme::Dark);
}

#[test]
fn 子库_新建子库弹层_浅色() {
    拍新建子库弹层("sublibrary/new-sublibrary-light", Theme::Light);
}

#[test]
fn 子库_新建子库弹层_暗色() {
    拍新建子库弹层("sublibrary/new-sublibrary-dark", Theme::Dark);
}

/// **删掉一台之后的提示条**：两台设备那一屏上删掉右边那张卡（「Retroid Pocket 5」），底边提示条上一颗「撤销」
/// （拿主意的人 2026-09-14 定）。提示条按 egui 那一帧的时刻计时，拍的那几帧远不到停够的时候。
fn 拍删除后提示条(名字: &str, 主题: Theme) {
    if 该跳过(名字) {
        return;
    }
    let mut 现场 = 两台设备();
    {
        let (screen, site) = 现场.app.sublibrary_and_site();
        screen.open(site, "Retroid Pocket 5");
        screen.remove(site);
    }
    let harness = 开一个(主题, move |ui| {
        现场.app.ui(ui);
    });
    拍下(harness, 名字);
}

#[test]
fn 子库_删除后提示条_浅色() {
    拍删除后提示条("sublibrary/deleted-toast-light", Theme::Light);
}

#[test]
fn 子库_删除后提示条_暗色() {
    拍删除后提示条("sublibrary/deleted-toast-dark", Theme::Dark);
}

/// **手动例外那两对里定死的「此刻」**：2026-09-03 14:58（UTC），本地钉成东八区——屏上画「09-03 22:58」那一天。
///
/// 例外记下的那一刻是中立库照挂钟写下的，照实画一趟一个样，所以整张表那一列由
/// `sublibrary::Screen::pin_exception_time` 钉死（与待确认屏定死裁决记录的时刻同一个用处、同一处实现）。
const 例外那两对的此刻: i64 = 1_788_447_480;

/// 这两对拍的是哪一台。
const 记着例外的那一台: &str = "RG35XX Plus";

/// **手动例外那两对的现场**：一台设备、一条规则，库里挂了两个作品，两样跟着挂钟走的都钉死。
///
/// 第三个变体（黄金太阳）**故意不挂作品**：认不出作品的那一行主栏写的是变体的键，
/// 两种样子要在同一张基线上并排。
fn 记着例外的一台(带上例外: bool) -> 子库现场 {
    let mut 现场 = 子库现场::摆好();
    现场.记一台(
        记着例外的那一台,
        不在位的目标,
        false,
        Some(64_000_000_000),
        None,
        &["平台=SFC"],
    );
    现场.挂到作品("圣剑传说 3", &["库/SFC/圣剑传说 3 汉化版.zip"]);
    现场.挂到作品("口袋妖怪 绿宝石", &["库/GBA/口袋妖怪 绿宝石.zip"]);
    if 带上例外 {
        for (键, 方向, 为什么) in [
            (
                "库/GBA/口袋妖怪 绿宝石.zip",
                Exception::Include,
                Some("小时候玩的就是这一版"),
            ),
            ("库/GBA/黄金太阳 开启的封印.zip", Exception::Include, None),
            (
                "库/SFC/圣剑传说 3 汉化版.zip",
                Exception::Exclude,
                Some("有新版汉化，旧版不带"),
            ),
        ] {
            现场.记一条例外(记着例外的那一台, 键, 方向, 为什么);
        }
    }
    现场.算一遍容量();
    {
        let screen = 现场.app.sublibrary_and_site().0;
        screen.set_clock(romcat_gui::clock::Clock::fixed(例外那两对的此刻, 8 * 3_600));
        screen.pin_exception_time(例外那两对的此刻 - 3_600);
    }
    现场
}

/// **手动例外弹层**（票 `gui-looks-like-the-design/22`）：卡上例外那一行按「管理」，停在「包含」那一栏——
/// 表上一行一条，作品（认不出作品的那一行写变体的键）、平台、体积、备注、时间与行尾「撤销」都在。
///
/// **「看得全」写成断言**：包含那一栏两条，屏上就得画出两颗「撤销」，而且每一颗都整个在视口里。
fn 拍手动例外弹层(名字: &str, 主题: Theme) {
    if 该跳过(名字) {
        return;
    }
    let mut 现场 = 记着例外的一台(true);
    let mut harness = 开一个(主题, move |ui| {
        现场.app.ui(ui);
    });
    按(&mut harness, "管理");
    let 视口 = egui::Rect::from_min_size(egui::Pos2::ZERO, headless::VIEWPORT.into());
    let 撤销 = 正好画着的每一处(harness.output(), "撤销");
    assert_eq!(
        撤销.len(),
        2,
        "包含那一栏两条，屏上画出了 {} 颗「撤销」",
        撤销.len()
    );
    assert!(
        撤销.iter().all(|rect| 视口.contains_rect(*rect)),
        "表里有一行被视口截了一半：{撤销:?}"
    );
    拍下(harness, 名字);
}

/// **手动例外弹层的空态与搜作品**：一条例外都没有的一台，停在「排除」那一栏（空态写明这一栏平时从哪儿来），
/// 搜索框里打着字、底下摆着命中的那一行。
///
/// 开弹层、换栏、搜一下都**走界面上那几条路**（`open_exceptions` / `show_exception_tab` /
/// `set_exception_search`），只是不靠敲键盘——搜索框里的字靠事件打进去要好几帧，而这一张要的是打完的样子。
fn 拍手动例外空态(名字: &str, 主题: Theme) {
    if 该跳过(名字) {
        return;
    }
    let mut 现场 = 记着例外的一台(false);
    {
        let (screen, site) = 现场.app.sublibrary_and_site();
        screen.open_exceptions(site, 记着例外的那一台);
        screen.show_exception_tab(Exception::Exclude);
        screen.set_exception_search(site, "口袋");
    }
    let harness = 开一个(主题, move |ui| {
        现场.app.ui(ui);
    });
    let 命中 = 正好画着的每一处(
        harness.output(),
        "还没有排除的作品。容量超限时，删减建议里的「排除」会记在这里；也可以在下面搜索添加。",
    );
    assert_eq!(命中.len(), 1, "排除那一栏的空态没画出来");
    拍下(harness, 名字);
}

#[test]
fn 子库_手动例外弹层_浅色() {
    拍手动例外弹层("sublibrary/exceptions-light", Theme::Light);
}

#[test]
fn 子库_手动例外弹层_暗色() {
    拍手动例外弹层("sublibrary/exceptions-dark", Theme::Dark);
}

#[test]
fn 子库_手动例外空态_浅色() {
    拍手动例外空态("sublibrary/exceptions-empty-light", Theme::Light);
}

#[test]
fn 子库_手动例外空态_暗色() {
    拍手动例外空态("sublibrary/exceptions-empty-dark", Theme::Dark);
}

// ——— 差量预览里的异常（票 `gui-looks-like-the-design/24`）———
//
// 差量预览摆在卡片下半截，**800 高的画面里整块落在画面外**，而 egui 不画整个落在裁剪区外的
// 东西。与超限那一对同一个处置（挂单 `Q895`）：画面放高，整张卡一次拍全，不滚——滚到底拍的话
// 图顶上那一行只露出下半截，字被切掉一半，看着像画坏了（票 20 第二段对稿时被打回过）。

/// 差量异常那两对拍的是哪一台。
const 摆着异常的那一台: &str = "RG35XX Plus";

/// 差量账旁边那句「排它用了 N ms」在基线里定死成这个数（`sublibrary::Screen::pin_prepare_ms`）。
/// 照实画的话一趟一个样——与例外那张表上的时刻同一个用处。
const 排它用了多少毫秒: f64 = 343.0;

/// 差量异常那两对的画面：宽照旧，高 1180——整张卡连差量账、步骤、异常那一块与底下的
/// 「同步」都要拍全（[`搭一扇`]，模块文档「视口定死」那一节的第二处例外）。
const 差量那几对的画面: [f32; 2] = [1280.0, 1180.0];

/// **差量异常那两对的现场**：一台在位的设备，库是**两个根**，清单里记着两条，卡上被人动过手脚。
///
/// 四类异常在这一屏上各有一条：
///
/// - **设备上缺失**：清单记着圣剑传说，卡上没有——人在掌机上删了它。
/// - **被修改过**：卡上那份黄金太阳与清单记的大小对不上，已经不是工具放的那一份。
/// - **目标位置被占用**：口袋妖怪的落点上挡着一个清单之外的文件。
/// - **放不进目标**：两个根里同一条相对路径（幻想传说）撞在卡上同一个文件上。
///
/// **元数据读不到**那一栏故意空着：造一个 `stat` 不动的文件各平台做法不一样，造出来也只是在验
/// 平台。空着那一栏画成什么样，正是这一对要守的东西之一。
fn 摆着异常的一台() -> 子库现场 {
    // 第二个根里同一条相对路径：剥掉根名之后与头一个根的那一份落在卡上同一个文件上。
    let mut 现场 = 子库现场::摆好带另一块盘(&[("SFC/幻想传说 汉化版.zip", 9_000)]);
    现场.记一台(
        摆着异常的那一台,
        "/Volumes/SDCARD",
        true,
        Some(64_000_000_000),
        None,
        &["平台=SFC,GBA"],
    );
    // 卡上：黄金太阳被别的工具改过（大小对不上清单），口袋妖怪的落点被一份清单之外的文件占着，
    // 圣剑传说被人删了。
    for (相对, 内容) in [
        ("GBA/黄金太阳 开启的封印.zip", "别的工具改过它"),
        (
            "GBA/口袋妖怪 绿宝石.zip",
            "我自己拷进来的，工具连看都不该看",
        ),
    ] {
        let 在 = 现场.卡.path().join(相对);
        std::fs::create_dir_all(在.parent().expect("有上级目录")).expect("能建目录");
        std::fs::write(&在, 内容.as_bytes()).expect("能写文件");
    }
    现场.记一份清单(
        摆着异常的那一台,
        &[
            ("GBA/黄金太阳 开启的封印.zip", 3_072),
            ("SFC/圣剑传说 3 汉化版.zip", 8_192),
        ],
    );
    现场.摊开(摆着异常的那一台);
    现场.排一遍差量();
    现场
}

/// 拍差量预览那一块：停在 `摆在哪一栏` 那一栏上。
fn 拍差量异常(名字: &str, 主题: Theme, 摆在哪一栏: romcat_gui::sublibrary::Anomaly) {
    if 该跳过(名字) {
        return;
    }
    let mut 现场 = 摆着异常的一台();
    现场.app.sublibrary_and_site().0.show_anomaly(摆在哪一栏);
    let harness = 开一扇(主题, 差量那几对的画面, move |ui| {
        现场.app.ui(ui);
    });
    // **「看得全」写成断言**：五个栏名与底下那颗「同步」都整个在画面里。哪天卡片长高、
    // 异常那一块被挤出画面，这里当场红，不会悄悄拍一张截掉半块的基线。
    let 视口 = egui::Rect::from_min_size(egui::Pos2::ZERO, 差量那几对的画面.into());
    for 该在画面里 in ["异常", "同步"] {
        let 在 = 正好画着的每一处(harness.output(), 该在画面里);
        assert!(
            在.iter().any(|rect| 视口.contains_rect(*rect)),
            "「{该在画面里}」没整个在画面里：{在:?}"
        );
    }
    拍下(harness, 名字);
}

#[test]
fn 子库_差量异常_设备上缺失_浅色() {
    拍差量异常(
        "sublibrary/anomalies-missing-light",
        Theme::Light,
        romcat_gui::sublibrary::Anomaly::Surprise(romcat_core::sync::SurpriseKind::Gone),
    );
}

#[test]
fn 子库_差量异常_设备上缺失_暗色() {
    拍差量异常(
        "sublibrary/anomalies-missing-dark",
        Theme::Dark,
        romcat_gui::sublibrary::Anomaly::Surprise(romcat_core::sync::SurpriseKind::Gone),
    );
}

#[test]
fn 子库_差量异常_放不进目标_浅色() {
    拍差量异常(
        "sublibrary/anomalies-nofit-light",
        Theme::Light,
        romcat_gui::sublibrary::Anomaly::NoFit,
    );
}

#[test]
fn 子库_差量异常_放不进目标_暗色() {
    拍差量异常(
        "sublibrary/anomalies-nofit-dark",
        Theme::Dark,
        romcat_gui::sublibrary::Anomaly::NoFit,
    );
}

// ——— 任务屏（票 `gui-looks-like-the-design/25`）———
//
// **这一段垫的是合成数据（`demo` 模块），只在 `demo` 特性下编**：截图照正式构建跑时（不开 `demo`）
// 这几张不跑，门禁带 `--all-features` 时照跑。夹具里别的几段一样都不靠 `demo`。
//
// 走整扇主窗口（左栏、屏头、状态栏都在），停在任务屏上。库是合成数据，任务屏上一个变体都不画。
// **跟着挂钟走的数一律定死**：工作目录（`App::set_workspace_label`）、已用与剩余约、耗时与收场时刻
// （`App::pin_task_clock`）。进度条只画走了几成的那种，不画来回跑的那种。

/// 任务屏那几张里定死的钟：已用 3 分 12 秒（剩余约由它折），历史每一趟收场于 2026-09-13 14:05（UTC）；
/// 本地时区钉成东八区，屏上画「09-13 22:05」。「此刻」也钉在同一天，于是不带年份——
/// 照实取机器的时区与今年的话，换一台机器、跨一个年，同一张基线就对不上了。
#[cfg(feature = "demo")]
fn 任务屏的钟() -> Clock {
    Clock {
        elapsed: Duration::from_secs(192),
        ended_at: 1_789_308_300,
        utc_offset: 8 * 3_600,
        now: 1_789_308_300,
    }
}

/// 一扇停在任务屏上的主窗口：合成数据的库，工作目录与钟都定死。
///
/// `临时目录名` 各张各用一个：主窗口会往工作目录里写版式偏好，几张共用的话一张写的会落到
/// 另一张打开的窗口上。
#[cfg(feature = "demo")]
fn 任务屏(临时目录名: &str) -> App {
    let mut app = App::new(
        demo::site(demo::synthetic(200).expect("造得出合成数据")).expect("开得出现场"),
        std::env::temp_dir().join(临时目录名),
    );
    app.show_view(View::Tasks);
    app.set_workspace_label(工作目录().display().to_string());
    app.pin_task_clock(任务屏的钟());
    app
}

/// 一份不占地方的产物：这几张画的是收场，不是产物里装了什么。
#[cfg(feature = "demo")]
fn 一份产物() -> Product {
    Product::Evaluated(Box::default())
}

/// 历史里摆上四档收场各一趟：**就地跑完**（`Board::run_here`，不开线程），于是一帧都不必等。
/// 部分完成那一句是核心库同步那一侧的原话（落了 12 件）。
#[cfg(feature = "demo")]
fn 摆上四档收场(app: &mut App) {
    let tasks = app.tasks_mut();
    tasks.run_here("算一遍容量", |_| Ok(一份产物()));
    tasks.run_here("排差量预览 · 掌机", |task| {
        task.stop();
        task.step("读选择集")?;
        Ok(一份产物())
    });
    tasks.run_here("同步 · 掌机", |task| {
        task.steps(3);
        task.step("新增 SFC/幻想传说 汉化版.zip")?;
        task.stop();
        task.halfway(
            "按停时落了 12 件，清单记着到这儿为止目标上真实有什么；\
             下一趟同步照这份清单接着来，落过的不再重落",
        );
        Ok(一份产物())
    });
    tasks.run_here("扫描 · 主库", |task| {
        task.steps(3);
        task.step("认根")?;
        Err(Cutoff::failed("根「主库」不在位：/Volumes/新加卷/Game"))
    });
}

/// 台上有一趟在跑、后面排着一趟时拍一张。
///
/// 跑着的那一趟是共享夹具的占位活：报完进度（四步走到第二步、这一步走了 96,064 / 256,128 件，
/// 即 34%）就停在那儿等信号，**等它报完再开窗**，不数挂钟。台上有活时主窗口每一帧都请求下一帧，
/// 跑不到「不要重画」，于是数帧：头两帧装字体与观感（[`搭一个`]），再跑几帧让历史表与卡片的列宽
/// 摆稳；这一屏上没有会动的东西（进度条是走了几成的那种）。
#[cfg(feature = "demo")]
fn 拍正在跑(名字: &str, 主题: Theme, 临时目录名: &str) {
    if 该跳过(名字) {
        return;
    }
    let mut app = 任务屏(临时目录名);
    let (报完了, 等它报完) = 一对信号();
    let 占位 = 占位活::照这样排上(app.tasks_mut(), "扫描 · 主库", move |task, 等收场| {
        task.steps(4);
        task.step("认根")?;
        task.step("挨个文件过一遍")?;
        task.tick(96_064, 256_128);
        报完了.发();
        等收场.等();
        task.check()?;
        Err(Cutoff::failed("占位活放行了"))
    });
    等它报完.等();
    app.tasks_mut().queue("识别 · 全部变体", |_| Ok(一份产物()));
    let mut harness = 搭一个(主题, move |ui| app.ui(ui));
    harness.run_steps(6);
    拍下(harness, 名字);
    占位.放行();
}

#[cfg(feature = "demo")]
#[test]
fn 任务屏_空台_浅色() {
    let mut app = 任务屏("romcat-截图-任务屏-空台-浅色");
    拍("tasks/empty-light", Theme::Light, move |ui| app.ui(ui));
}

#[cfg(feature = "demo")]
#[test]
fn 任务屏_空台_暗色() {
    let mut app = 任务屏("romcat-截图-任务屏-空台-暗色");
    拍("tasks/empty-dark", Theme::Dark, move |ui| app.ui(ui));
}

#[cfg(feature = "demo")]
#[test]
fn 任务屏_正在跑_浅色() {
    拍正在跑(
        "tasks/running-light",
        Theme::Light,
        "romcat-截图-任务屏-正在跑-浅色",
    );
}

#[cfg(feature = "demo")]
#[test]
fn 任务屏_正在跑_暗色() {
    拍正在跑(
        "tasks/running-dark",
        Theme::Dark,
        "romcat-截图-任务屏-正在跑-暗色",
    );
}

#[cfg(feature = "demo")]
#[test]
fn 任务屏_四档收场_浅色() {
    let mut app = 任务屏("romcat-截图-任务屏-四档收场-浅色");
    摆上四档收场(&mut app);
    拍("tasks/history-light", Theme::Light, move |ui| app.ui(ui));
}

#[cfg(feature = "demo")]
#[test]
fn 任务屏_四档收场_暗色() {
    let mut app = 任务屏("romcat-截图-任务屏-四档收场-暗色");
    摆上四档收场(&mut app);
    拍("tasks/history-dark", Theme::Dark, move |ui| app.ui(ui));
}

// ——— 主窗口外壳（票 `gui-looks-like-the-design/32`） ———

/// 主窗口外壳那几张垫的库：一个根，五个变体各落一档，两个进得了待确认队列。
///
/// 屏上画着的每一样都是定值：主库原名（只活在内存里的那份，名字就是主库标识「主库」）、沉淀库在哪
/// （「（内存）」）、队列那几批的样本（种子从 0 起）。**工作目录是临时目录，屏上不画它**：底部状态栏上那一截工作目录
/// 由 [`拍主窗口`] 定死成基线里那一串（同任务屏那几张）；收起那一下往临时目录里写版式文件。
fn 垫的主窗口(工作目录: &Path) -> App {
    use shared::档;

    shared::小库(
        &[
            ("SFC", "幻想传说 (汉化).zip", 档::待裁决),
            ("SFC", "幻想传说 (修正版).zip", 档::待裁决),
            ("GBA", "命中.zip", 档::命中),
            ("GBA", "一条候选都没有.zip", 档::没有候选),
            ("FC", "还没轮到它.zip", 档::还没识别),
        ],
        工作目录.to_path_buf(),
    )
}

/// 开一扇主窗口，换到任务屏（台上空着）；`收起` 时先按一下栏底那颗「« 收起」，再拍。左栏的计数照旧来自那份小库。
#[track_caller]
fn 拍主窗口(名字: &str, 主题: Theme, 收起: bool) {
    if 该跳过(名字) {
        return;
    }
    let 临时目录 = romcat_core::testing::temp_dir("截图门-主窗口");
    let mut app = 垫的主窗口(临时目录.path());
    app.show_view(View::Tasks);
    // 状态栏右边画工作目录（票 `gui-looks-like-the-design/25`）：照实画的是临时目录，带进程号与时刻，一趟一个样。
    app.set_workspace_label(工作目录().display().to_string());
    let mut harness = 开一个(主题, move |ui| app.ui(ui));
    if 收起 {
        按(&mut harness, rail::FOLD);
    }
    拍下(harness, 名字);
}

#[test]
fn 主窗口_左栏展开_浅色() {
    拍主窗口("main-window/rail-expanded-light", Theme::Light, false);
}

#[test]
fn 主窗口_左栏展开_暗色() {
    拍主窗口("main-window/rail-expanded-dark", Theme::Dark, false);
}

#[test]
fn 主窗口_左栏收起_浅色() {
    拍主窗口("main-window/rail-collapsed-light", Theme::Light, true);
}

#[test]
fn 主窗口_左栏收起_暗色() {
    拍主窗口("main-window/rail-collapsed-dark", Theme::Dark, true);
}

// ——— 待确认屏（票 `gui-looks-like-the-design/18`）———
//
// 走整扇主窗口（左栏、屏头、状态栏都在），停在待确认屏上。按批、逐条、裁决记录三态垫的是合成数据（`demo::queue`，
// 真机形状的一万六千多条），**只在 `demo` 特性下编**；空态那一态垫测试手搭的小库（`shared::小库`），不靠 `demo`。
// **跟着挂钟走的数一律定死**：工作目录（`App::set_workspace_label`），裁决记录的时刻与「此刻」
// （`queue::Screen::set_clock`、`queue::Screen::pin_record_time`）。

/// 待确认屏那几张里定死的「此刻」：2026-09-13 14:05（UTC），本地钉成东八区——屏上画「09-13 22:05」那一天。
#[cfg(feature = "demo")]
const 待确认屏的此刻: i64 = 1_789_308_300;

/// 一扇停在待确认屏上的主窗口：合成数据的队列，工作目录、「此刻」与裁决记录的时刻都定死。
///
/// `临时目录名` 各张各用一个：主窗口会往工作目录里写版式偏好，几张共用的话一张写的会落到另一张打开的窗口上。
#[cfg(feature = "demo")]
fn 待确认屏(临时目录名: &str) -> App {
    let mut app = App::new(
        demo::site(demo::queue(demo::QUEUE_ROWS).expect("造得出合成数据")).expect("开得出现场"),
        std::env::temp_dir().join(临时目录名),
    );
    app.show_view(View::Queue);
    app.set_workspace_label(工作目录().display().to_string());
    let (screen, _) = app.queue_and_site();
    screen.set_clock(romcat_gui::clock::Clock::fixed(待确认屏的此刻, 8 * 3_600));
    screen.pin_record_time(待确认屏的此刻 - 25 * 60);
    app
}

/// 逐条那一屏：照稿走屏头「逐条」那条路进来——待选列表栏头「有多个候选 N 条」，光标停在头一条上。
///
/// 合成数据里中文离线源那一次匹配落在一个**单候选**的变体上，不在「有多个候选」那一栏里，这一张照这条路拍不到那一堆
/// （那一堆由 `tests/queue.rs` 的几条测试钉着）。
#[cfg(feature = "demo")]
fn 停在逐条(app: &mut App) {
    app.queue_and_site().0.show_multiple();
}

/// 裁决记录那一块要有东西可画：整批通过最小的那一批能整批通过的、整批拒绝最小的那一批没有候选的，再撤掉头一批——
/// 抽屉里在册的与撤过的各一行。走的是界面上按下去的那几条路（`pass` / `reject` / `commit` / `undo`）。
#[cfg(feature = "demo")]
fn 落两批撤一批(app: &mut App) {
    use romcat_core::triage::{Fanout, Scope};

    let batches = app.queue().queue().batches().to_vec();
    let 能过 = batches
        .iter()
        .filter(|batch| batch.passable())
        .min_by_key(|batch| batch.count)
        .cloned()
        .expect("合成数据里该有能整批通过的一批");
    let 没有候选 = batches
        .iter()
        .filter(|batch| batch.shape.fanout() == Fanout::None)
        .min_by_key(|batch| batch.count)
        .cloned()
        .expect("合成数据里该有一批没有候选的");
    let (screen, site) = app.queue_and_site();
    screen.pass(site, &Scope::whole(能过.shape));
    screen.commit(site);
    let 头一批 = screen.applied().expect("整批通过该落下一批").batch;
    screen.reject(site, &Scope::whole(没有候选.shape));
    screen.commit(site);
    screen.undo(site, 头一批);
    assert!(screen.error().is_none(), "{:?}", screen.error());
}

#[cfg(feature = "demo")]
#[test]
fn 待确认_按批_浅色() {
    let mut app = 待确认屏("romcat-截图-待确认-按批-浅色");
    拍("queue/batches-light", Theme::Light, move |ui| app.ui(ui));
}

#[cfg(feature = "demo")]
#[test]
fn 待确认_按批_暗色() {
    let mut app = 待确认屏("romcat-截图-待确认-按批-暗色");
    拍("queue/batches-dark", Theme::Dark, move |ui| app.ui(ui));
}

#[cfg(feature = "demo")]
#[test]
fn 待确认_逐条_浅色() {
    let mut app = 待确认屏("romcat-截图-待确认-逐条-浅色");
    停在逐条(&mut app);
    拍("queue/one-by-one-light", Theme::Light, move |ui| app.ui(ui));
}

#[cfg(feature = "demo")]
#[test]
fn 待确认_逐条_暗色() {
    let mut app = 待确认屏("romcat-截图-待确认-逐条-暗色");
    停在逐条(&mut app);
    拍("queue/one-by-one-dark", Theme::Dark, move |ui| app.ui(ui));
}

/// **一批里再下钻**那一态（票 `gui-looks-like-the-design/19`）：展开一批能整批通过、又切得出好几项的，
/// 就地把头一项整批通过掉，再下钻到第二项。一张图里同时有这几样：
///
/// - 每一项底下那条**占比条**，分母是这一批本来多少条；
/// - 裁完的那一项**划着删除线、旁边一枚「已通过」**，点不动；
/// - 「细分」那一排**锁在当初那个轴上**，另两颗淡着，底下一句为什么；
/// - 第二项底下**就地那一框**：「只看：…」、这一部分的样本、「通过这 N 条 ｜ 逐条处理 ｜ 拒绝这 N 条」；
/// - 底下那一排改口说**「通过剩余的 N 条」「拒绝剩余的 N 条」**。
///
/// 挑哪一批、哪个轴由数据当场定（与 `tests/queue.rs` 那几条同一个挑法），不写死——合成数据是定值，
/// 挑法定死了，挑出来的那一批就是定值。
#[cfg(feature = "demo")]
fn 停在下钻(app: &mut App) {
    use romcat_core::triage::{Axis, Scope, Shape};

    let 能过: Vec<Shape> = app
        .queue()
        .queue()
        .batches()
        .iter()
        .filter(|batch| batch.passable())
        .map(|batch| batch.shape.clone())
        .collect();
    let (shape, axis) = 能过
        .into_iter()
        .find_map(|shape| {
            let axis = Axis::ALL.into_iter().find(|axis| {
                app.queue()
                    .queue()
                    .drill(&Scope::whole(shape.clone()), *axis)
                    .rows
                    .len()
                    > 2
            })?;
            Some((shape, axis))
        })
        .expect("合成数据里该有一批能整批通过、又切得出好几项的");
    // `open_batch` 是开关：已经展开着的那一批再点一次是收起来。
    if app.queue().scope().map(|scope| scope.shape).as_ref() != Some(&shape) {
        app.queue_and_site().0.open_batch(&shape);
    }
    app.queue_and_site().0.set_axis(axis);
    let 几项: Vec<String> = app
        .queue()
        .breakdown()
        .expect("展开了就该有细分")
        .rows
        .iter()
        .map(|row| row.label.clone())
        .collect();
    let (screen, site) = app.queue_and_site();
    screen.pass(site, &Scope::under(shape, axis, &几项[0]));
    screen.commit(site);
    assert!(screen.error().is_none(), "{:?}", screen.error());
    screen.drill_into(&几项[1]);
}

/// 下钻那一对的画面（点）：宽照旧，高 960——**就地那一框把底下那一排顶出了 800**，
/// 而那一排上「通过剩余的 N 条」正是这一票第 4 条验收的原话，800 高拍不到它
/// （与子库超限那一对同一个理由，见 [`超限那一对的画面`]）。
#[cfg(feature = "demo")]
const 下钻那一对的画面: [f32; 2] = [1280.0, 960.0];

/// 下钻那一对：画面 1280×960，这一票的整条链一张拍全。
///
/// **「看得全」写成断言**（照子库超限那一对的做法）：裁完那一项的「已通过」、底下那一排
/// 两颗「剩余」按钮，每一样都得**整个**在画面里——egui 不画整个落在裁剪区外的控件，
/// 哪天细分那一栏长高把它们挤出去，这里当场红，不会悄悄拍一张截掉半截的基线。
#[cfg(feature = "demo")]
fn 拍下钻(名字: &str, 主题: Theme, 临时目录名: &str) {
    if 该跳过(名字) {
        return;
    }
    let mut app = 待确认屏(临时目录名);
    停在下钻(&mut app);
    let 剩下 =
        romcat_core::report::thousands(app.queue().breakdown().expect("下钻着就该有细分").left);
    let harness = 开一扇(主题, 下钻那一对的画面, move |ui| app.ui(ui));
    let 视口 = egui::Rect::from_min_size(egui::Pos2::ZERO, 下钻那一对的画面.into());
    for 那一段 in [
        format!("通过剩余的 {剩下} 条"),
        format!("拒绝剩余的 {剩下} 条"),
        "已通过".to_owned(),
    ] {
        let 在 = 正好画着的每一处(harness.output(), &那一段);
        assert!(
            在.len() == 1 && 视口.contains_rect(在[0]),
            "「{那一段}」没整个在画面里：{在:?}"
        );
    }
    拍下(harness, 名字);
}

#[cfg(feature = "demo")]
#[test]
fn 待确认_下钻_浅色() {
    拍下钻(
        "queue/drill-light",
        Theme::Light,
        "romcat-截图-待确认-下钻-浅色",
    );
}

#[cfg(feature = "demo")]
#[test]
fn 待确认_下钻_暗色() {
    拍下钻(
        "queue/drill-dark",
        Theme::Dark,
        "romcat-截图-待确认-下钻-暗色",
    );
}

/// 落两批、撤一批，点屏头那颗「裁决记录 1」（数的是还在册的）打开右边那块抽屉，再拍。
#[cfg(feature = "demo")]
fn 拍裁决记录(名字: &str, 主题: Theme, 临时目录名: &str) {
    if 该跳过(名字) {
        return;
    }
    let mut app = 待确认屏(临时目录名);
    落两批撤一批(&mut app);
    let mut harness = 开一个(主题, move |ui| app.ui(ui));
    按(&mut harness, "裁决记录 1");
    拍下(harness, 名字);
}

#[cfg(feature = "demo")]
#[test]
fn 待确认_裁决记录_浅色() {
    拍裁决记录(
        "queue/records-light",
        Theme::Light,
        "romcat-截图-待确认-裁决记录-浅色",
    );
}

#[cfg(feature = "demo")]
#[test]
fn 待确认_裁决记录_暗色() {
    拍裁决记录(
        "queue/records-dark",
        Theme::Dark,
        "romcat-截图-待确认-裁决记录-暗色",
    );
}

/// 一趟识别都还没跑过的小库：待确认屏上是那张空态卡。
#[track_caller]
fn 拍待确认空态(名字: &str, 主题: Theme) {
    use shared::档;

    if 该跳过(名字) {
        return;
    }
    let 临时目录 = romcat_core::testing::temp_dir("截图门-待确认空态");
    let mut app = shared::小库(
        &[
            ("FC", "魂斗罗.nes", 档::还没识别),
            ("SFC", "幻想传说.sfc", 档::还没识别),
        ],
        临时目录.path().to_path_buf(),
    );
    app.show_view(View::Queue);
    app.set_workspace_label(工作目录().display().to_string());
    拍下(开一个(主题, move |ui| app.ui(ui)), 名字);
}

#[test]
fn 待确认_空态_浅色() {
    拍待确认空态("queue/empty-light", Theme::Light);
}

#[test]
fn 待确认_空态_暗色() {
    拍待确认空态("queue/empty-dark", Theme::Dark);
}

// ——— 设置（票 `gui-looks-like-the-design/31`） ———

/// **设置那一屏**：一份刚建出来的库开在设置上，八节里挑一节拍。
///
/// 三样钉死，照旧是为了「每台机器每一趟都一样」：底部状态栏与工作目录那一节印的那一段短写
/// （`App::set_workspace_label`）、数据源那张表的时刻（`库屏的钟`），以及**探 ffmpeg 那个程序名**
/// ——那一格照实探这台机器，而门禁那几台有的装了有的没装（`Screen::probe_with`，指一个必定
/// 不存在的名字，画出来一律是「没找到」那一档，设计稿上画的也是这一档）。
struct 设置屏 {
    app: App,
    _工作区: TempDir,
}

impl 设置屏 {
    fn 停在(节: Section) -> Self {
        let 工作区 = temp_dir("snapshot-设置屏");
        let site = 开库(工作区.path());
        let mut app = App::new(site, 工作区.path().to_path_buf());
        app.show_view(View::Settings);
        app.set_workspace_label(工作目录().display().to_string());
        let screen = app.settings_mut();
        screen.show_section(节);
        screen.probe_with(romcat_core::scrape::preview::NO_SUCH_PROGRAM);
        screen.pin_account(false);
        screen.pin_clock(库屏的钟());
        Self {
            app,
            _工作区: 工作区,
        }
    }
}

#[track_caller]
fn 拍设置屏(名字: &str, 主题: Theme, 节: Section) {
    if 该跳过(名字) {
        return;
    }
    let mut 现场 = 设置屏::停在(节);
    拍下(开一个(主题, move |ui| 现场.app.ui(ui)), 名字);
}

#[test]
fn 设置_常规_浅色() {
    拍设置屏("settings/general-light", Theme::Light, Section::General);
}

#[test]
fn 设置_常规_暗色() {
    拍设置屏("settings/general-dark", Theme::Dark, Section::General);
}

#[test]
fn 设置_工作目录_浅色() {
    拍设置屏("settings/workspace-light", Theme::Light, Section::Workspace);
}

#[test]
fn 设置_工作目录_暗色() {
    拍设置屏("settings/workspace-dark", Theme::Dark, Section::Workspace);
}

#[test]
fn 设置_数据源_浅色() {
    拍设置屏("settings/sources-light", Theme::Light, Section::Sources);
}

#[test]
fn 设置_数据源_暗色() {
    拍设置屏("settings/sources-dark", Theme::Dark, Section::Sources);
}

#[test]
fn 设置_刮削_浅色() {
    拍设置屏("settings/scrape-light", Theme::Light, Section::Scrape);
}

#[test]
fn 设置_刮削_暗色() {
    拍设置屏("settings/scrape-dark", Theme::Dark, Section::Scrape);
}

#[test]
fn 设置_导出_浅色() {
    拍设置屏("settings/export-light", Theme::Light, Section::Export);
}

#[test]
fn 设置_导出_暗色() {
    拍设置屏("settings/export-dark", Theme::Dark, Section::Export);
}

#[test]
fn 设置_工具_浅色() {
    拍设置屏("settings/tools-light", Theme::Light, Section::Tools);
}

#[test]
fn 设置_工具_暗色() {
    拍设置屏("settings/tools-dark", Theme::Dark, Section::Tools);
}

#[test]
fn 设置_关于_浅色() {
    拍设置屏("settings/about-light", Theme::Light, Section::About);
}

#[test]
fn 设置_关于_暗色() {
    拍设置屏("settings/about-dark", Theme::Dark, Section::About);
}

#[test]
fn 设置_快捷键_浅色() {
    拍设置屏("settings/keys-light", Theme::Light, Section::Keys);
}

#[test]
fn 设置_快捷键_暗色() {
    拍设置屏("settings/keys-dark", Theme::Dark, Section::Keys);
}
