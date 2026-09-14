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
//! （[`Listing`]）直接交进去。视口是 [`headless::VIEWPORT`]，一点一个像素。**浅色与暗色各拍一张**：
//! 两套主题各取令牌里的一套，只拍一套的话，另一套颜色接错了没人看得见。
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
use std::time::Duration;

use egui::Theme;
use egui_kittest::{Harness, SnapshotOptions};
use romcat_core::catalog::{CatalogError, SCHEMA_VERSION};
use romcat_core::task::Cutoff;
use romcat_core::workspace::{CatalogEntry, CatalogFacts, CatalogState, DirUnreadable, Listing};
use romcat_gui::app::{App, View};
use romcat_gui::opening::Screen;
use romcat_gui::task::{Clock, Product};
use romcat_gui::{demo, font, headless, look, rail};

mod shared;
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
fn 搭一个<'a>(主题: Theme, mut 画一帧: impl FnMut(&mut egui::Ui) + 'a) -> Harness<'a> {
    let mut harness = Harness::builder()
        .with_size(headless::VIEWPORT)
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

/// 屏上**正好**写着 `那几个字`、按画出来的次序**最后**那一处的中心点。
fn 最后一处正好画着(output: &egui::FullOutput, 那几个字: &str) -> Option<egui::Pos2> {
    fn 找(shape: &egui::epaint::Shape, 那几个字: &str, 最后: &mut Option<egui::Pos2>) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                if text.galley.text() == 那几个字 {
                    *最后 = Some(egui::Rect::from_min_size(text.pos, text.galley.size()).center());
                }
            }
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    找(one, 那几个字, 最后);
                }
            }
            _ => {}
        }
    }
    let mut 最后 = None;
    for clipped in &output.shapes {
        找(&clipped.shape, 那几个字, &mut 最后);
    }
    最后
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

// ——— 任务屏（票 `gui-looks-like-the-design/25`）———
//
// 走整扇主窗口（左栏、屏头、状态栏都在），停在任务屏上。库是合成数据，任务屏上一个变体都不画。
// **跟着挂钟走的数一律定死**：工作目录（`App::set_workspace_label`）、已用与剩余约、耗时与收场时刻
// （`App::pin_task_clock`）。进度条只画走了几成的那种，不画来回跑的那种。

/// 任务屏那几张里定死的钟：已用 3 分 12 秒（剩余约由它折），历史每一趟收场于 2026-09-13 14:05（UTC）；
/// 本地时区钉成东八区，屏上画「09-13 22:05」。「此刻」也钉在同一天，于是不带年份——
/// 照实取机器的时区与今年的话，换一台机器、跨一个年，同一张基线就对不上了。
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
fn 一份产物() -> Product {
    Product::Evaluated(Box::default())
}

/// 历史里摆上四档收场各一趟：**就地跑完**（`Board::run_here`，不开线程），于是一帧都不必等。
/// 部分完成那一句是核心库同步那一侧的原话（落了 12 件）。
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

#[test]
fn 任务屏_空台_浅色() {
    let mut app = 任务屏("romcat-截图-任务屏-空台-浅色");
    拍("tasks/empty-light", Theme::Light, move |ui| app.ui(ui));
}

#[test]
fn 任务屏_空台_暗色() {
    let mut app = 任务屏("romcat-截图-任务屏-空台-暗色");
    拍("tasks/empty-dark", Theme::Dark, move |ui| app.ui(ui));
}

#[test]
fn 任务屏_正在跑_浅色() {
    拍正在跑(
        "tasks/running-light",
        Theme::Light,
        "romcat-截图-任务屏-正在跑-浅色",
    );
}

#[test]
fn 任务屏_正在跑_暗色() {
    拍正在跑(
        "tasks/running-dark",
        Theme::Dark,
        "romcat-截图-任务屏-正在跑-暗色",
    );
}

#[test]
fn 任务屏_四档收场_浅色() {
    let mut app = 任务屏("romcat-截图-任务屏-四档收场-浅色");
    摆上四档收场(&mut app);
    拍("tasks/history-light", Theme::Light, move |ui| app.ui(ui));
}

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
