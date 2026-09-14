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
//! （[`Listing`]）直接交进去。视口是 [`headless::VIEWPORT`]，一点一个像素。**例外只有一对**：子库屏超限那两张
//! （`sublibrary/over-capacity-*`）是 1280×960——整张卡连删减表底下的灰框与按钮都要拍全，800 高装不下（拿主意的人
//! 2026-09-14 定，挂单 `Q895`；[`开一扇`]）。**浅色与暗色各拍一张**：
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
use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions};
use romcat_core::catalog::roots::{self, LibraryRoot, RootScan};
use romcat_core::catalog::{Catalog, CatalogError, SCHEMA_VERSION};
use romcat_core::fs::RealFs;
use romcat_core::scan::{self, Jobs, ScanOptions};
use romcat_core::site::Site;
use romcat_core::sublibrary::{Rule, Sublibrary};
use romcat_core::task::{Cutoff, Handle};
use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::verdict::Store;
use romcat_core::workspace::{CatalogEntry, CatalogFacts, CatalogState, DirUnreadable, Listing};
use romcat_gui::app::{App, View};
use romcat_gui::layout::{FOLD_EXPORT, FOLD_ROOTS, FOLD_SOURCES};
use romcat_gui::opening::Screen;
use romcat_gui::roots::RootRow;
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
    ("移除", 2),
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
    _工作区: TempDir,
    卡: TempDir,
    app: App,
}

impl 子库现场 {
    /// 扫好一份小主库、开出窗口、换到子库屏。一个子库都还没有。
    fn 摆好() -> Self {
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
        let mut options = ScanOptions::named(主库.path(), "库");
        options.jobs = Jobs::Fixed(2);
        scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");
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
    let mut 现场 = 两台设备();
    let mut harness = 开一个(主题, move |ui| {
        现场.app.ui(ui);
    });
    按(&mut harness, "目标设置…");
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
