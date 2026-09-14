//! **数据源优先级**那一层弹层（票 `gui-looks-like-the-design/30`）。
//!
//! 判断全在核心库（`scrape::priority`）：谁固定、怎么挪、会变什么、写到哪儿。这里钉的是
//! **屏上摆没摆出来、按下去交没交给核心库那一处**，外加从刮削面板打开、保存之后主窗口
//! 取没取走那一份、任务台上多没多一趟活。
//!
//! 弹层头一帧只量尺寸不画，所以每一处先空跑两帧再收字（`tests/dialog.rs` 同一条）。

use romcat_core::catalog::identify::{Identification, Provenance};
use romcat_core::catalog::scrape::{Harvested, HarvestedValue};
use romcat_core::catalog::{Catalog, State};
use romcat_core::platform::Manifest;
use romcat_core::scrape::priority::VERDICT;
use romcat_core::scrape::{AnchorKind, Priorities};
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::{sync, workspace};
use romcat_gui::app::View;
use romcat_gui::priority::{self, Editor};
use romcat_gui::{headless, look};

mod shared;
use shared::{档, 画出来的字, 跑一帧};

/// 装好观感基线的上下文：弹层标题要令牌那几档字号。
fn 上下文() -> egui::Context {
    let ctx = headless::context();
    look::install(&ctx);
    ctx
}

/// 画一帧弹层，交出画出来的字。**先空跑两帧**：头一帧只量尺寸。
fn 画(ctx: &egui::Context, editor: &mut Editor, catalog: &Catalog) -> String {
    for _ in 0..2 {
        跑一帧(ctx, |ui| editor.show(ui.ctx(), catalog));
    }
    跑一帧(ctx, |ui| editor.show(ui.ctx(), catalog))
}

/// 一份空的中立库、一个空的工作目录，打开着的弹层。
fn 打开着的(tag: &str) -> (TempDir, Catalog, Editor) {
    let 工作目录 = temp_dir(tag);
    let catalog = Catalog::open_in_memory().expect("开得出中立库");
    let mut editor = Editor::new(工作目录.path().to_path_buf());
    editor.open();
    (工作目录, catalog, editor)
}

/// 在这份库里认出一部作品：一个根、这个平台上一个变体、一部作品，变体挂到作品上。
fn 认出一部作品(catalog: &mut Catalog, 平台: &str, 作品: &str) {
    romcat_core::catalog::roots::add_root(
        catalog,
        None,
        shared::根,
        std::path::Path::new(&format!("/{}", shared::根)),
    )
    .expect("建得出根");
    let variant = shared::变体(平台, &format!("{作品}.bin"));
    catalog
        .replace_variants(std::slice::from_ref(&variant), 1, &Manifest::default())
        .expect("写得进变体");
    let work_id = catalog
        .add_work(作品, Provenance::Identified)
        .expect("建得出作品");
    catalog
        .write_identifications(&[Identification {
            variant_key: variant.key,
            platform: None,
            standalone: None,
            state: State::Matched,
            reason: None,
            units: 1,
            nkit: 0,
            read_bytes: 0,
            work_id: Some(work_id),
            release_id: None,
            candidates: Vec::new(),
        }])
        .expect("写得进识别结论");
}

#[test]
fn 七个字段各自的顺序看得见改得动_裁决与手工维护那两家固定在前面挪不动() {
    let ctx = 上下文();
    let (_工作目录, catalog, mut editor) = 打开着的("优先级-七个字段");
    let 内置 = Priorities::builtin();

    let 屏上 = 画(&ctx, &mut editor, &catalog);
    for field in ["标题", "年份", "发行商", "开发商", "类型", "简介", "汉化组"] {
        assert!(屏上.contains(field), "左边那一列少了「{field}」：\n{屏上}");
    }
    assert!(屏上.contains(priority::HOW_TO_READ), "{屏上}");
    assert!(
        屏上.contains(priority::pinned_note(VERDICT).0),
        "裁决旁边没写它固定：\n{屏上}"
    );
    assert!(
        屏上.contains(priority::pinned_note("Pegasus").0),
        "手工维护那两家旁边没写它们固定：\n{屏上}"
    );

    assert_eq!(editor.fields().len(), 7, "左边那一列该是七个字段");
    for field in editor.fields() {
        editor.select_field(&field);
        let 屏上 = 画(&ctx, &mut editor, &catalog);
        let order = editor.draft().order(&field, None).to_vec();
        for source in &order {
            assert!(
                屏上.contains(source.as_str()),
                "「{field}」那一栏少了 {source}：\n{屏上}"
            );
        }
        assert_eq!(order[..3], [VERDICT, "Pegasus", "ES-Gamelist"], "{field}");

        // **固定的那三家挪不动，别的也挪不进它们前面。**
        editor.raise(1);
        editor.raise(2);
        editor.lower(2);
        editor.raise(3);
        assert_eq!(
            editor.draft(),
            &内置,
            "「{field}」那一栏固定的那几家被挪动了"
        );
    }

    // 固定的那几家之后的挪得动，挪完标「已调整」。
    editor.select_field("标题");
    editor.lower(3);
    assert_eq!(
        editor.draft().order("标题", None)[3..5],
        ["Redump", "No-Intro"]
    );
    assert!(画(&ctx, &mut editor, &catalog).contains("标题 · 已调整"));
}

#[test]
fn 街机的标题单独编辑_屏上说明它整条替换通用顺序() {
    let ctx = 上下文();
    let (_工作目录, catalog, mut editor) = 打开着的("优先级-街机");
    let 内置 = Priorities::builtin();

    editor.select_field("标题");
    assert_eq!(editor.platforms(), ["街机"]);
    let 屏上 = 画(&ctx, &mut editor, &catalog);
    assert!(屏上.contains("街机另有单独的顺序"), "{屏上}");

    editor.select_platform(Some("街机"));
    let 屏上 = 画(&ctx, &mut editor, &catalog);
    assert!(
        屏上.contains(&priority::override_note("街机")),
        "屏上没说街机那一栏整条替换通用顺序：\n{屏上}"
    );

    // 街机那一条上，固定的那三家同样挪不动，别的也挪不进它们前面。
    for at in 0..3 {
        editor.raise(at);
        editor.lower(at);
    }
    editor.raise(3);
    assert_eq!(editor.draft(), &内置, "街机那一栏固定的那几家被挪动了");

    // 挪的是街机那一条，通用那条一个字不动。
    editor.lower(3);
    assert_eq!(
        editor.draft().order("标题", Some("街机"))[3..5],
        ["No-Intro", "MAME"]
    );
    assert_eq!(editor.draft().order("标题", None), 内置.order("标题", None));

    // 没有单独顺序的字段切不过去，不替人新开一条覆盖。
    editor.select_field("简介");
    editor.select_platform(Some("街机"));
    assert_eq!(editor.platform(), None);
}

#[test]
fn 保存前列出会让哪些作品的显示值变_举得出例子() {
    let ctx = 上下文();
    let (_工作目录, mut catalog, mut editor) = 打开着的("优先级-变化");
    // 导出只给做得成条目的算：简介挂在一部**名下有变体**的作品上。
    认出一部作品(&mut catalog, "FC", "幻想传说");
    let 采到 = |source: &str, value: &str| Harvested {
        anchor: AnchorKind::Work.label().to_string(),
        subject: "幻想传说".to_string(),
        source: source.to_string(),
        input: "测试".to_string(),
        values: vec![HarvestedValue {
            field: "简介".to_string(),
            value: value.to_string(),
            evidence: "手写的".to_string(),
        }],
        media: Vec::new(),
    };
    catalog
        .put_scraped(&[
            采到("ScreenScraper", "An RPG"),
            采到("中文离线源", "中文简介"),
        ])
        .expect("写得进刮削结果");

    let 屏上 = 画(&ctx, &mut editor, &catalog);
    assert!(屏上.contains("还没改动。"), "{屏上}");

    editor.select_field("简介");
    editor.lower(3);
    let 屏上 = 画(&ctx, &mut editor, &catalog);
    assert!(屏上.contains("保存后的变化"), "{屏上}");
    assert!(
        屏上.contains(
            "简介：1 个作品的显示值会变，例如作品「幻想传说」由 ScreenScraper 的「An RPG」\
             改为 中文离线源 的「中文简介」。"
        ),
        "屏上没列出会变的那一处：\n{屏上}"
    );
    assert!(屏上.contains(priority::NO_RESCRAPE), "{屏上}");
}

#[test]
fn 恢复默认回到内置那一份_没列出的数据源怎么排屏上说明() {
    let ctx = 上下文();
    let (工作目录, catalog, _) = 打开着的("优先级-恢复默认");
    // 工作目录里已经有一份人改过的。
    let mut 改过的 = Priorities::builtin();
    assert!(改过的.lower("简介", None, 3));
    改过的
        .save(&workspace::priorities_path(工作目录.path()))
        .expect("写得进去");

    let mut editor = Editor::new(工作目录.path().to_path_buf());
    editor.open();
    assert_eq!(editor.draft(), &改过的, "打开时该读工作目录里那一份");
    // 打开时看的是标题那一栏：作品的显示标题里，没列出的源之间不按采集时刻排。
    let 屏上 = 画(&ctx, &mut editor, &catalog);
    assert!(屏上.contains(priority::TITLE_UNLISTED), "{屏上}");
    assert!(!屏上.contains(priority::UNLISTED), "{屏上}");
    editor.select_field("简介");
    let 屏上 = 画(&ctx, &mut editor, &catalog);
    assert!(屏上.contains(priority::UNLISTED), "{屏上}");
    assert!(屏上.contains("恢复默认"), "{屏上}");

    editor.reset();
    assert_eq!(editor.draft(), &Priorities::builtin());
    assert!(editor.can_save(), "恢复默认之后该按得下保存");
    editor.save();
    assert_eq!(
        sync::prepare::priorities(None, 工作目录.path()).expect("读得动"),
        Priorities::builtin()
    );
}

/// 屏上**正好**写着 `那一段`、而且**画在最上面**的那一处（中心点）。
///
/// 两样都要：页脚那颗「保存」也出现在说明那句话里，按「含有」找会点到说明上去；而
/// 弹层底下那一屏也可能有一颗写着同样字的按钮，按「先画的那一处」找会点到遮罩底下去。
/// 画出来的次序是从底下往上叠的，最后那一处就是最上面那一层的。
fn 最上面那一处(output: &egui::FullOutput, 那一段: &str) -> Option<egui::Pos2> {
    fn 找(shape: &egui::epaint::Shape, 那一段: &str, out: &mut Option<egui::Pos2>) {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == 那一段 => {
                *out = Some(egui::Rect::from_min_size(text.pos, text.galley.size()).center());
            }
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    找(one, 那一段, out);
                }
            }
            _ => {}
        }
    }
    let mut out = None;
    for clipped in &output.shapes {
        找(&clipped.shape, 那一段, &mut out);
    }
    out
}

/// 按一下屏上正好写着 `那一段`、画在最上面的那一处，返回松开之后再画一帧画出来的字。
fn 正好点一下(
    ctx: &egui::Context,
    那一段: &str,
    mut 画一帧: impl FnMut(&mut egui::Ui),
) -> String {
    let 头一帧 = headless::frame(ctx, headless::input(), &mut 画一帧);
    let Some(位置) = 最上面那一处(&头一帧, 那一段) else {
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

#[test]
fn 从刮削面板打开_保存写回工作目录立即生效_不排任何刮削任务() {
    let ctx = 上下文();
    let 工作目录 = temp_dir("优先级-保存");
    let mut app = shared::小库(
        &[
            ("FC", "命中.nes", 档::命中),
            ("FC", "待裁决.nes", 档::待裁决),
        ],
        工作目录.path().to_path_buf(),
    );
    app.show_view(View::Browse);
    for _ in 0..2 {
        跑一帧(&ctx, |ui| app.ui(ui));
    }
    app.browse_and_site().0.picked_mut().select_all();
    跑一帧(&ctx, |ui| app.ui(ui));
    {
        let (browse, site) = app.browse_and_site();
        browse.open_scrape(&site.catalog);
    }
    for _ in 0..2 {
        跑一帧(&ctx, |ui| app.ui(ui));
    }

    // 刮削面板上那颗按钮打开这一层。
    shared::点一下(&ctx, priority::OPEN, |ui| app.ui(ui));
    assert!(
        app.browse().scrape().priority().is_open(),
        "按「{}」没打开",
        priority::OPEN
    );
    for _ in 0..2 {
        跑一帧(&ctx, |ui| app.ui(ui));
    }

    {
        let editor = app.browse_and_site().0.scrape_mut().priority_mut();
        editor.select_field("简介");
        editor.lower(3);
    }
    let 改过的 = app.browse().scrape().priority().draft().clone();
    assert_ne!(改过的, Priorities::builtin(), "简介那一栏该挪动了");
    let 台上原来的 = (app.tasks().queued().len(), app.tasks().history().len());
    正好点一下(&ctx, "保存", |ui| app.ui(ui));
    跑一帧(&ctx, |ui| app.ui(ui));

    assert!(
        !app.browse().scrape().priority().is_open(),
        "保存之后这一层该关上"
    );
    assert_eq!(
        sync::prepare::priorities(None, 工作目录.path()).expect("读得动"),
        改过的,
        "读的那一侧没读到写回的那一份"
    );
    // 主窗口取走了那一份，换进浏览屏。
    assert!(
        !app.browse().scrape().priority().has_saved(),
        "主窗口没取走保存的那一份"
    );
    assert_eq!(app.browse().scrape().notice(), Some(priority::SAVED));
    // **不排任何刮削任务。**
    assert!(!app.tasks().busy(), "保存优先级不该排活");
    assert_eq!(
        (app.tasks().queued().len(), app.tasks().history().len()),
        台上原来的,
        "保存优先级不该排活"
    );
}
