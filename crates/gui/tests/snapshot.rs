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
#[cfg(feature = "demo")]
use std::time::Duration;

use egui::Theme;
use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions};
use romcat_core::catalog::browse::PlatformFilter;
use romcat_core::catalog::identify::{Candidate, Identification, Provenance};
use romcat_core::catalog::roots::{self, LibraryRoot, RootScan};
use romcat_core::catalog::scrape::{Harvested, HarvestedMedia, HarvestedValue};
use romcat_core::catalog::{Catalog, CatalogError, Confidence, SCHEMA_VERSION, State};
use romcat_core::dat::Convention;
use romcat_core::fs::RealFs;
use romcat_core::platform::Manifest;
use romcat_core::scan::{self, Jobs, ScanOptions};
use romcat_core::scrape::{AnchorKind, Field, MediaKind};
use romcat_core::shape::{SINGLE_FILE_RULE, Variant};
use romcat_core::site::Site;
use romcat_core::sublibrary::{Rule, Sublibrary};
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
#[cfg(feature = "demo")]
use romcat_gui::task::{Clock, Product};
use romcat_gui::{font, headless, layout, look, rail};

mod shared;
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
            catalog
                .put_media(&hash, "png", 86 * 1024)
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
    控件都落在所在那一栏里(&harness, 名字);
    if 态 != 浏览态::筛空 {
        带标签的行正题露得出字(&harness, 名字);
    }
    拍下(harness, 名字);
    // 拍完才收工作目录：版式偏好一直在里头读写。
    drop(目录);
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
    site.catalog
        .put_media(&视频, "mp4", 256)
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

#[test]
fn 浏览_卡片_暗色() {
    拍浏览("browse/cards-dark", Theme::Dark, 浏览态::卡片);
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
