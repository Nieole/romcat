//! **设置**那一屏（票 `gui-looks-like-the-design/31`）：八节切得动、改名走核心库那一处判据、
//! `⌘/Ctrl+,` 打得开，以及这一轮拿主意的人新立的那三条版式要求——同一列左缘一条线、
//! 数字列右对齐、横贯的东西撑满整宽。
//!
//! **对齐这几条验的是矩形，不是像素**：从这一帧画出来的每一段字取外框（[`每一段`]），
//! 断言该对齐的那几段左缘（或右缘）相等。像素比那一路在截图门里（`tests/snapshot.rs`）。

use romcat_core::site::Site;
use romcat_core::testing::{TempDir, temp_dir};
use romcat_gui::app::{App, View};
use romcat_gui::headless;
use romcat_gui::settings::{self, Section};

mod shared;
use shared::{点一下, 画出来的字, 跑一帧};

/// 点屏上**正好**写着这几个字的那一处（不是「含有」）。
///
/// 左边那一列上写着「工作目录」，而屏头那句副标题「工作目录、数据源、刮削与导出的默认值」
/// 画在它前头——按「含有」找，点到的是那句副标题，八节一节都翻不动。
fn 正好点一下(
    ctx: &egui::Context,
    那一段: &str,
    mut 画一帧: impl FnMut(&mut egui::Ui),
) -> String {
    let 头一帧 = headless::frame(ctx, headless::input(), &mut 画一帧);
    let Some(位置) = shared::正好那一段画在哪儿(&头一帧, 那一段) else {
        panic!(
            "屏上没有正好写着「{那一段}」的地方：\n{}",
            画出来的字(&头一帧)
        );
    };
    let 按 = |pressed: bool| egui::Event::PointerButton {
        pos: 位置,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut input = headless::input();
    input.events.push(egui::Event::PointerMoved(位置));
    input.events.push(按(true));
    headless::frame(ctx, input, &mut 画一帧);
    let mut input = headless::input();
    input.events.push(按(false));
    headless::frame(ctx, input, &mut 画一帧);
    跑一帧(ctx, 画一帧)
}

/// 截图门那一串定死的工作目录：**屏上绝不印这台机器上那条临时目录**。
const 工作目录字样: &str = "~/.local/share/romcat";

/// 一份真落盘的库 + 一扇开在设置屏上的窗。
///
/// 改名那几条要它落盘：在内存里开的那一份 `Site::display_name` 直接交回**主库标识**
/// （`catalog.file()` 是 `None` 那一支），改了名屏上也不会变——那验的就不是改名了。
fn 开在设置屏上(tag: &str) -> (App, TempDir) {
    let 工作区 = temp_dir(tag);
    let 库文件 = 工作区.path().join("catalog").join("主库.sqlite3");
    drop(romcat_core::catalog::Catalog::create(&库文件, "主库").expect("建得出中立库"));
    let site = Site::open_file(工作区.path(), &库文件, None).expect("开得出现场");
    let mut app = App::new(site, 工作区.path().to_path_buf());
    app.show_view(View::Settings);
    app.set_workspace_label(工作目录字样);
    (app, 工作区)
}

/// 这一帧画出来的每一段字与它的外框。
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

/// 屏上头一处画着 `那一段`（一字不差）的外框。
fn 画在哪儿(output: &egui::FullOutput, 那一段: &str) -> egui::Rect {
    每一段(output)
        .into_iter()
        .find(|(画的, _)| 画的 == 那一段)
        .unwrap_or_else(|| panic!("屏上没画「{那一段}」"))
        .1
}

/// 跑一帧，交出这一帧的全部产物（要矩形时用它，只要字用 [`跑一帧`]）。
fn 一帧(ctx: &egui::Context, app: &mut App) -> egui::FullOutput {
    headless::frame(ctx, headless::input(), |ui| app.ui(ui))
}

#[test]
fn 八节都在左边那一列上切得动() {
    let (mut app, _工作区) = 开在设置屏上("设置屏-八节");
    let ctx = headless::context();
    跑一帧(&ctx, |ui| app.ui(ui));
    for 节 in Section::ALL {
        let 屏上 = 正好点一下(&ctx, 节.label(), |ui| app.ui(ui));
        assert_eq!(
            app.settings().section(),
            节,
            "点了「{}」没翻过去：\n{屏上}",
            节.label()
        );
        // 八节的名字一直摆在左边那一列上，所以每一节上八个名字都读得到——
        // 这一条验的是**翻过去之后右边真画了东西**，不是那一列自己。
        assert!(
            !屏上.trim().is_empty(),
            "「{}」这一节上一个字都没画",
            节.label()
        );
    }
}

#[test]
fn 设置屏上不印这台机器上那条绝对路径() {
    let (mut app, 工作区) = 开在设置屏上("设置屏-不印绝对路径");
    let ctx = headless::context();
    app.settings_mut().show_section(Section::Workspace);
    跑一帧(&ctx, |ui| app.ui(ui));
    let 屏上 = 跑一帧(&ctx, |ui| app.ui(ui));
    let 那条 = 工作区.path().display().to_string();
    assert!(
        !屏上.contains(&那条),
        "工作目录那一节印出了这台机器上的绝对路径 {那条}：\n{屏上}",
    );
    assert!(
        屏上.contains(工作目录字样),
        "工作目录那一节没印那一段短写：\n{屏上}",
    );
}

#[test]
fn 改名收不收由核心库说了算() {
    let (mut app, _工作区) = 开在设置屏上("设置屏-改名");
    let ctx = headless::context();
    跑一帧(&ctx, |ui| app.ui(ui));

    // **空白不是名字**：拒的那句话是核心库说的（`CatalogError::BlankLibraryName`），
    // 这一层一个字都不自己编。
    app.settings_mut().draft_name("   ");
    let 屏上 = 点一下(&ctx, settings::RENAME, |ui| app.ui(ui));
    let 那句 = app
        .settings()
        .renamed()
        .expect("按过一次改名")
        .expect_err("空白不该收");
    assert!(
        屏上.contains(那句),
        "屏上没印核心库拒的那句话「{那句}」：\n{屏上}",
    );
    assert_eq!(app.site().display_name(), "主库", "拒掉的那一下把名字改了",);

    // 改成一个像样的名字：**开场屏与报告上印的那个**（`Site::display_name`）跟着换，
    // 窗口标题也跟着换（窗口取走了 `take_renamed` 那一下）。
    app.settings_mut().draft_name("我的主库");
    let 屏上 = 点一下(&ctx, settings::RENAME, |ui| app.ui(ui));
    assert_eq!(
        app.site().display_name(),
        "我的主库",
        "名字没改成：\n{屏上}"
    );
    assert!(
        app.window_title().contains("我的主库"),
        "改完名窗口标题还是旧的：{}",
        app.window_title(),
    );
    assert!(屏上.contains("我的主库"), "屏上没回话：\n{屏上}");
}

#[test]
fn 改名那一格常驻着改名不动路径锚那句话() {
    let (mut app, _工作区) = 开在设置屏上("设置屏-路径锚");
    let ctx = headless::context();
    跑一帧(&ctx, |ui| app.ui(ui));
    let 屏上 = 跑一帧(&ctx, |ui| app.ui(ui));
    assert!(
        屏上.contains(settings::RENAME_NOTE),
        "常规那一节没写「改名不动路径锚」：\n{屏上}",
    );
}

#[test]
fn 换工作目录先过主库只读那道判据() {
    // 一份根落在临时目录里的库：把工作目录换到那个根**底下**，核心库当场拒
    // （`Roots::refuse_writing_into` → `path::refuse_writing_into_library`，ADR-0004）。
    //
    // **这一条直接驱设置屏**，不绕窗口：弹系统选择窗口那一层测不到（`romcat_gui::pick`），
    // 而「挑回来的那个收不收」正是分出 `offer_workspace` 要验的那一半。
    let 工作区 = temp_dir("设置屏-换工作目录");
    let 盘 = temp_dir("设置屏-换工作目录-盘");
    let 库文件 = 工作区.path().join("catalog").join("主库.sqlite3");
    let catalog = romcat_core::catalog::Catalog::create(&库文件, "主库").expect("建得出中立库");
    romcat_core::catalog::roots::add_root(&catalog, None, "主库", 盘.path()).expect("加得上根");
    drop(catalog);
    let site = Site::open_file(工作区.path(), &库文件, None).expect("开得出现场");
    let facts = settings::Facts {
        site: &site,
        workspace_label: 工作目录字样,
        verdicts: None,
    };
    let mut screen = settings::Screen::new(工作区.path().to_path_buf());
    screen.show_section(Section::Workspace);
    let ctx = headless::context();
    跑一帧(&ctx, |ui| screen.ui(ui, &facts));

    screen.offer_workspace(盘.path().join("romcat-工作目录"), &facts);
    assert!(
        screen.take_workspace().is_none(),
        "落在主库根里的那个目录被收下了",
    );
    let 屏上 = 跑一帧(&ctx, |ui| screen.ui(ui, &facts));
    assert!(
        屏上.contains("主库只读") || 屏上.contains("主库内"),
        "屏上没说为什么不收：\n{屏上}",
    );

    // 换到主库之外的那一个：收得下，窗口取得走（取走之后整份退回开场）。
    let 别处 = temp_dir("设置屏-换工作目录-别处");
    screen.offer_workspace(别处.path().to_path_buf(), &facts);
    assert_eq!(
        screen.take_workspace().as_deref(),
        Some(别处.path()),
        "主库之外的那个目录没收下",
    );
}

#[test]
fn 按一下打开设置那一对键就换到设置屏() {
    let (mut app, _工作区) = 开在设置屏上("设置屏-快捷键");
    app.show_view(View::Queue);
    let ctx = headless::context();
    跑一帧(&ctx, |ui| app.ui(ui));
    let mut input = headless::input();
    input.events.push(egui::Event::Key {
        key: egui::Key::Comma,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    });
    headless::frame(&ctx, input, |ui| app.ui(ui));
    assert_eq!(app.view(), View::Settings, "⌘/Ctrl+, 没打开设置屏");
}

#[test]
fn 快捷键那一节摆的是全仓那一份表() {
    let (mut app, _工作区) = 开在设置屏上("设置屏-快捷键表");
    app.settings_mut().show_section(Section::Keys);
    let ctx = headless::context();
    跑一帧(&ctx, |ui| app.ui(ui));
    let 屏上 = 跑一帧(&ctx, |ui| app.ui(ui));
    let 表 = romcat_gui::keys::groups();
    assert!(!表.is_empty(), "那份表是空的，这一条什么都没验");
    for group in 表 {
        assert!(
            屏上.contains(group.title),
            "少了「{}」这一组：\n{屏上}",
            group.title
        );
        for (管什么, 键) in group.keys {
            assert!(屏上.contains(管什么), "少了「{管什么}」这一条：\n{屏上}");
            assert!(屏上.contains(&键), "「{管什么}」那一条没画键帽：\n{屏上}");
        }
    }
}

// ——— 版式：这一轮拿主意的人新立的那三条 ———

#[test]
fn 一节里值那一列每一行都从同一条线起() {
    // 名那一列宽是定死的（令牌 `settings-row-label`），于是值那一列不跟着名字的长短挪。
    // 跟着挪的话，「库文件结构」那一行的值就会比「版本」那一行往右一截。
    let (mut app, _工作区) = 开在设置屏上("设置屏-左缘");
    app.settings_mut().show_section(Section::About);
    let ctx = headless::context();
    一帧(&ctx, &mut app);
    let out = 一帧(&ctx, &mut app);
    let 版本 = 画在哪儿(&out, env!("CARGO_PKG_VERSION"));
    let 结构 = 画在哪儿(
        &out,
        &format!("版本 {}", romcat_core::catalog::SCHEMA_VERSION),
    );
    assert!(
        (版本.min.x - 结构.min.x).abs() <= 1.0,
        "值那一列没对齐：版本在 {:.1}，库文件结构在 {:.1}",
        版本.min.x,
        结构.min.x,
    );
    // 名那一列也一样：两行的名字同样从一条线起。
    let 名甲 = 画在哪儿(&out, "版本");
    let 名乙 = 画在哪儿(&out, "库文件结构");
    assert!(
        (名甲.min.x - 名乙.min.x).abs() <= 1.0,
        "名那一列没对齐：{:.1} 与 {:.1}",
        名甲.min.x,
        名乙.min.x,
    );
}

#[test]
fn 数字那一列右对齐() {
    // 工作目录那一节「里头装着」那张表：条数一律靠右，右对齐才比得出大小。
    let (mut app, _工作区) = 开在设置屏上("设置屏-右对齐");
    app.settings_mut().show_section(Section::Workspace);
    let ctx = headless::context();
    一帧(&ctx, &mut app);
    let out = 一帧(&ctx, &mut app);
    // **只认内容区里那几格**：左栏「浏览」那一项的计数也画着「—」，它不在这张表上。
    let 名 = 画在哪儿(&out, "里头装着");
    let 数字们: Vec<egui::Rect> = 每一段(&out)
        .into_iter()
        .filter(|(画的, _)| 画的 == "未下载" || 画的 == "—" || 画的.ends_with(" 条"))
        .filter(|(_, 在)| 在.min.x > 名.min.x)
        .map(|(_, 在)| 在)
        .collect();
    assert!(
        数字们.len() >= 2,
        "「里头装着」那张表上不到两行数，这一条什么都没验",
    );
    let 右 = 数字们[0].max.x;
    for 一格 in &数字们 {
        assert!(
            (一格.max.x - 右).abs() <= 1.0,
            "数字那一列没右对齐：{:.1} 与 {:.1}（{数字们:?}）",
            一格.max.x,
            右,
        );
    }
}

#[test]
fn 一行底下那道线横贯整个内容区() {
    // 行与行之间那道线（设计稿 `.srow` 的 `border-bottom`）停在中间的话，两列就像是各画各的。
    let (mut app, _工作区) = 开在设置屏上("设置屏-横贯");
    app.settings_mut().show_section(Section::About);
    let ctx = headless::context();
    一帧(&ctx, &mut app);
    let out = 一帧(&ctx, &mut app);
    let 名 = 画在哪儿(&out, "版本");
    // 那道线与名那一列从同一条线起（两样都从内容区左沿起），这么认得出是它、不是屏头那一道。
    let 几道 = 横线们(&out);
    let 这一节的: Vec<&egui::Rect> = 几道
        .iter()
        .filter(|线| (线.min.x - 名.min.x).abs() <= 2.0 && 线.min.y > 名.min.y)
        .collect();
    assert!(!这一节的.is_empty(), "一节里一道分隔线都没画：{几道:?}");
    let 内容区 = headless::VIEWPORT[0] - 名.min.x;
    for 一道 in &这一节的 {
        assert!(
            一道.width() >= 内容区 * 0.7,
            "那道线才 {:.1} 宽，内容区有 {内容区:.1}，停在半中间了",
            一道.width(),
        );
    }
}

/// 这一帧画出来的每一道横线（分隔线是一条极扁的矩形）。
fn 横线们(output: &egui::FullOutput) -> Vec<egui::Rect> {
    fn 收(shape: &egui::Shape, out: &mut Vec<egui::Rect>) {
        match shape {
            egui::Shape::LineSegment { points, .. } => {
                let 框 = egui::Rect::from_two_pos(points[0], points[1]);
                if 框.height() <= 1.5 && 框.width() > 1.0 {
                    out.push(框);
                }
            }
            egui::Shape::Rect(rect) if rect.rect.height() <= 1.5 && rect.rect.width() > 1.0 => {
                out.push(rect.rect);
            }
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

#[test]
fn 快捷键表同一列里的键帽右缘是一条线() {
    // 照稿那张表是两列（`.kgrid`），一条里说明靠左、键帽靠右。**一列里的键帽右缘是一条线**：
    // 于是屏上只有两条这样的线，一列一条。
    let (mut app, _工作区) = 开在设置屏上("设置屏-键帽");
    app.settings_mut().show_section(Section::Keys);
    let ctx = headless::context();
    一帧(&ctx, &mut app);
    let out = 一帧(&ctx, &mut app);
    let 每一条: Vec<String> = romcat_gui::keys::groups()
        .iter()
        .flat_map(|group| group.keys.iter().map(|(_, 键)| 键.clone()))
        .collect();
    let mut 右缘们: Vec<f32> = 每一段(&out)
        .into_iter()
        .filter(|(画的, _)| 每一条.contains(画的))
        .map(|(_, 在)| 在.max.x)
        .collect();
    assert!(右缘们.len() >= 4, "屏上不到四枚键帽：{右缘们:?}");
    右缘们.sort_by(f32::total_cmp);
    let mut 几列: Vec<(f32, usize)> = Vec::new();
    for 右缘 in 右缘们 {
        match 几列.last_mut() {
            Some((基准, 几枚)) if (右缘 - *基准).abs() <= 1.0 => *几枚 += 1,
            _ => 几列.push((右缘, 1)),
        }
    }
    assert_eq!(
        几列.len(),
        2,
        "键帽的右缘落成了 {} 条线，照稿只该有左右两列：{几列:?}",
        几列.len(),
    );
    for (_, 几枚) in &几列 {
        assert!(*几枚 >= 2, "有一列上只有一枚键帽，对不出齐：{几列:?}");
    }
}

/// 这一条盯着**别处**：设置屏上不许出现「这个库在哪一块盘上」那种绝对路径，连导出目录也一样。
#[test]
fn 导出那一节没设过时照实说还没设过() {
    let (mut app, _工作区) = 开在设置屏上("设置屏-导出");
    app.settings_mut().show_section(Section::Export);
    let ctx = headless::context();
    跑一帧(&ctx, |ui| app.ui(ui));
    let 屏上 = 跑一帧(&ctx, |ui| app.ui(ui));
    assert!(
        屏上.contains("还没设过"),
        "导出那一节没说还没设过：\n{屏上}"
    );
    assert!(
        屏上.contains(settings::EXTERNAL_EDIT),
        "导出那一节没写撞上外部修改怎么办：\n{屏上}",
    );
}

/// 工具那一节：**没有 ffmpeg 的那一档**照实说，而且说清没有它影响的只是视频预览帧。
#[test]
fn 没有ffmpeg时说清影响的只是视频预览帧() {
    let (mut app, _工作区) = 开在设置屏上("设置屏-ffmpeg");
    app.settings_mut().show_section(Section::Tools);
    app.settings_mut()
        .probe_with(romcat_core::scrape::preview::NO_SUCH_PROGRAM);
    let ctx = headless::context();
    跑一帧(&ctx, |ui| app.ui(ui));
    let 屏上 = 跑一帧(&ctx, |ui| app.ui(ui));
    assert!(屏上.contains("没找到"), "没说 ffmpeg 没找到：\n{屏上}");
    assert!(
        屏上.contains("预览帧") && 屏上.contains("照常"),
        "没说清没有它照常能用、影响的只是预览帧：\n{屏上}",
    );
    assert!(
        屏上.contains(settings::REPROBE),
        "没摆「重新检测」：\n{屏上}",
    );
}

/// 数据源那一节：配额那一段警示**与刮削面板上那一段是同一句**（ADR-0007 那条命脉只有一处写）。
#[test]
fn 配额那段警示与刮削面板是同一句() {
    let (mut app, _工作区) = 开在设置屏上("设置屏-配额");
    app.settings_mut().show_section(Section::Sources);
    let ctx = headless::context();
    跑一帧(&ctx, |ui| app.ui(ui));
    let 屏上 = 跑一帧(&ctx, |ui| app.ui(ui));
    for 一句 in ["永久封禁", "未识别 ROM"] {
        assert!(屏上.contains(一句), "配额那一段没说「{一句}」：\n{屏上}");
    }
}

/// 这一条不验设置屏，验的是**左栏上多了一项**：设置在「后台」那一组、紧跟任务。
#[test]
fn 左栏后台那一组里紧跟任务的是设置() {
    let (_, views) = romcat_gui::rail::GROUPS[2];
    assert_eq!(views, &[View::Tasks, View::Settings], "左栏后台那一组不对");
}

/// 媒体池那一行印的是**工作目录底下那一份**，同样走窗口交进来的那一句短写。
#[test]
fn 工作目录这一格读的是窗口交进来的那一句() {
    let (mut app, _工作区) = 开在设置屏上("设置屏-工作目录那一句");
    app.settings_mut().show_section(Section::Workspace);
    let ctx = headless::context();
    跑一帧(&ctx, |ui| app.ui(ui));
    let out = 一帧(&ctx, &mut app);
    let 媒体池 = format!("{工作目录字样}/media");
    assert!(
        每一段(&out).iter().any(|(画的, _)| 画的 == &媒体池),
        "媒体池那一行印的不是工作目录底下那一份：\n{}",
        画出来的字(&out),
    );
}
