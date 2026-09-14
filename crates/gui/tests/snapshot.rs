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

use std::path::{Path, PathBuf};

use egui::Theme;
use egui_kittest::{Harness, SnapshotOptions};
use romcat_core::catalog::browse::PlatformFilter;
use romcat_core::catalog::identify::{Candidate, Identification, Provenance};
use romcat_core::catalog::scrape::{Harvested, HarvestedMedia, HarvestedValue};
use romcat_core::catalog::{Catalog, CatalogError, Confidence, SCHEMA_VERSION, State};
use romcat_core::dat::Convention;
use romcat_core::platform::Manifest;
use romcat_core::scrape::{AnchorKind, Field, MediaKind};
use romcat_core::shape::{Role, SINGLE_FILE_RULE, Variant};
use romcat_core::site::Site;
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::verdict::Store;
use romcat_core::workspace::{CatalogEntry, CatalogFacts, CatalogState, DirUnreadable, Listing};
use romcat_gui::app::{App, View};
use romcat_gui::opening::Screen;
use romcat_gui::{font, headless, layout, look};

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
    // **走 `install_once`**：主窗口那一路（`App::ui` 的头一帧）自己也问一遍它。这里直接 `install`
    // 的话，那一问认不出装过，会再装一遍，把下面关掉的光标闪烁又带回来。
    look::install_once(ctx);
    ctx.all_styles_mut(|style| style.visuals.text_cursor.blink = false);
    ctx.request_repaint();
    false
}

/// 开一扇窗：视口 [`headless::VIEWPORT`]、一点一个像素、这一套主题，跑到这一屏不再要重画为止。
fn 开一个<'a>(主题: Theme, mut 画一帧: impl FnMut(&mut egui::Ui) + 'a) -> Harness<'a> {
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
    // 头一帧只装字体（见 [`装好`]），弹层这类浮层画出来的第二帧才摆稳：先跑两帧，再跑到不要重画
    // 为止（跑不稳时 `run` 当场炸，并说清是谁一直在要重画）。
    harness.run_steps(2);
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
                members: vec![(key.clone(), Role::Main)],
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
        浏览态::筛空 => {}
    }
    控件都落在所在那一栏里(&harness, 名字);
    拍下(harness, 名字);
    // 拍完才收工作目录：版式偏好一直在里头读写。
    drop(目录);
}

/// **每一个可交互的控件都整个落在它所在那一栏的可见区里**（拿主意的人看浏览屏：「按钮都没显示全」）。
///
/// 从**无障碍树**读：egui 给每个控件挂一个节点，外框就是它摆出来的那一块——画出界、被旁边一栏或
/// 窗沿盖掉的那一截，外框里照样算着。「可交互」认的是**点得了或者聚焦得了**；面板本身、拖边界的
/// 把手与滚动条不算，它们本来就骑在边上。
///
/// 一栏是哪一块，问 egui 自己存的面板尺寸：顶栏、左栏（收起时是那条窄条）、右栏（同）、底下那块
/// 编辑面板，剩下的是表格那一块。控件**上沿的中点**落在哪一栏，就归哪一栏；哪一栏都不落的，本身就是
/// 问题。按上沿不按中心：滚动区最底下那一个被窗沿截掉一半时，中心已经出了窗，上沿还在它那一栏里。
///
/// 面板边上那条拖动把手不算：egui 给它的节点没有角色、只有两份 `resize_grab_radius_side` 那么宽，
/// 本来就骑在两栏交界上。
///
/// **左右两沿必须整个在栏里，竖着只查上沿**：左栏、右栏、编辑面板与表格都是滚动区，最底下那一个
/// 被滚动区的下沿截掉一截是滚动区本来的样子（设计稿里左栏最底下那一格也截着），滚一下就整个露出来；
/// 顶栏不滚，上下两沿都查（挂单 `Q871`）。
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
    let 顶栏 = 面板(egui::Id::new("顶栏")).expect("顶栏画过");
    let 左栏 = 侧栏(layout::FILTER).expect("左栏画过");
    let 右栏 = 侧栏(layout::DETAIL).expect("右栏画过");
    let 底栏 = 面板(egui::Id::new(layout::EDIT.id)).expect("编辑面板画过");
    let 表格 = egui::Rect::from_min_max(
        egui::pos2(左栏.max.x, 顶栏.max.y),
        egui::pos2(右栏.min.x, 底栏.min.y),
    );
    // 次序有讲究：顶栏横跨整个窗宽，先认；编辑面板在表格底下，比表格先认。
    let 各栏 = [
        ("顶栏", 顶栏, true),
        ("左栏", 左栏, false),
        ("右栏", 右栏, false),
        ("编辑面板", 底栏, false),
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
