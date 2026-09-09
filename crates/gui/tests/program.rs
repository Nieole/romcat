//! **启动那条路**：定位要开哪份库、开出**现场**、进主窗口。
//!
//! 这几条验的是那条路本身，而不是它跑完之后的五屏。从前它整条摊在 `main.rs` 里，
//! 一条测试都够不着——「三种给法行为一致」只能靠人手敲三遍命令。现在它住在
//! [`romcat_gui::program::Program`] 上，于是**开进主窗口**这段过渡可测。
//!
//! **一个字节都不碰真库**：中立库一律现建在临时目录里（ADR-0004）。

use std::path::Path;

use romcat_core::catalog::roots::{self, RootScan};
use romcat_core::catalog::{Catalog, SCHEMA_VERSION};
use romcat_core::platform::Manifest;
use romcat_core::report::human_time;
use romcat_core::shape::{Role, SINGLE_FILE_RULE, Variant};
use romcat_core::testing::{self, TempDir, temp_dir};
use romcat_core::workspace::{self, Slug};
use romcat_gui::headless;
use romcat_gui::program::Program;
use romcat_gui::recent::Recent;
use romcat_gui::site::Locate;

mod shared;

/// 在这个工作目录里现建一份中立库，返回它的文件路径。
///
/// **建的是磁盘上那一份**，不是内存里那一份：启动那条路认的正是「这个文件在不在」
/// （`Site::open` 在 `catalog.exists()` 那一步挡下），只活在内存里的库它一眼都看不见。
fn 建一份库(workspace: &Path, slug: Slug<'_>) -> std::path::PathBuf {
    let path = workspace::catalog_path(workspace, slug);
    drop(Catalog::open(&path).expect("开得出中立库"));
    path
}

/// 那份中立库的主文件名——**主库在路径锚里叫什么名字**（`Site::library`）。
///
/// 它是**标识符**，带着十六位哈希后缀。屏上与标题上都不该出现它，几条测试拿它做反向
/// 断言（`Site::display_name` 交出来的是另一个）。
fn 主库名(catalog: &Path) -> String {
    catalog
        .file_stem()
        .expect("有主文件名")
        .to_string_lossy()
        .into_owned()
}

/// 那份库**给人看的**名字——窗口标题与开场那一行上写着的那个。
///
/// 这几份夹具库是拿 `Catalog::open` 建的（没记过名字），于是它就是主文件名剥掉哈希
/// 后缀剩下的那一半（`workspace::readable_half`，票 01 那条退路）。
fn 给人看的名字(catalog: &Path) -> String {
    workspace::readable_half(&主库名(catalog)).to_string()
}

/// 一个工作目录，连它里头那份现成的库。
fn 摆好一份库(tag: &str) -> (TempDir, std::path::PathBuf) {
    let 工作目录 = temp_dir(tag);
    let 库文件 = 建一份库(工作目录.path(), Slug::Named("测试库"));
    (工作目录, 库文件)
}

// ——— 开场那一屏 ———

/// 开场那一行要画的三样全在这份库里：**主库名**（元数据表那一行）、**变体数**、
/// **上次扫描时刻**。
///
/// 根指着一块**没挂上的盘**：这三个数住在中立库里，画它们一个字节都不许碰主库
/// （ADR-0009）。
fn 建一份像样的库(
    工作目录: &Path,
    名字: &str,
    变体数: usize,
    扫于: i64,
) -> std::path::PathBuf {
    let slug = Slug::Named(名字);
    let 库文件 = workspace::catalog_path(工作目录, slug);
    let mut catalog = Catalog::open_named(&库文件, &slug.display_name()).expect("开得出中立库");
    roots::add_root(
        &catalog,
        Some(工作目录),
        "主库",
        Path::new("/没挂上的那块盘/主库"),
    )
    .expect("加得上根");
    let variants: Vec<Variant> = (0..变体数).map(变体).collect();
    catalog
        .replace_variants(&variants, 1, &Manifest::default())
        .expect("写得进变体");
    catalog
        .record_root_scan(
            "主库",
            &RootScan {
                at: 扫于,
                elapsed_ms: 1_000,
                entries: u64::try_from(变体数).unwrap_or(0),
                interrupted: false,
            },
        )
        .expect("记得下上次扫描");
    drop(catalog);
    库文件
}

/// 造第 `n` 个变体：单文件规则，一个主成员。
fn 变体(n: usize) -> Variant {
    let key = format!("主库/FC/第{n}个.zip");
    Variant {
        main_key: key.clone(),
        platform: Some("FC".to_string()),
        rule: SINGLE_FILE_RULE.to_string(),
        manual: false,
        files: 1,
        bytes: 4_096,
        unreadable_files: 0,
        members: vec![(key.clone(), Role::Main)],
        key,
    }
}

/// 一个参数都不给、只说工作目录：这就是**开场**那条路。
///
/// **那份「上次开的那份」由调用方给**（[`记在临时处`]）：默认那一处落在维护者真正的
/// 默认工作目录里（[`Recent::here`]），一条测试都不许读它、更不许写它。
fn 开场(工作目录: &Path, 记的: Recent) -> Program {
    Program::start_with(
        &Locate {
            workspace: Some(工作目录),
            ..Locate::default()
        },
        记的,
    )
    .expect("开场自己开得起来——它要的正是「还没说开哪份库」")
}

/// 这一趟测试的那份「**上次开的那份**」记在哪儿：一个专门的临时目录。
///
/// **它不在工作目录里**——那正是这张票的一条验收（这份记忆不属于任何工作目录，
/// 换工作目录不会把它弄丢），摆在工作目录里的话那一条就验不着了。
fn 记在临时处(tag: &str) -> (TempDir, Recent) {
    let dir = temp_dir(tag);
    let recent = Recent::at(dir.path().join("上次开的那份.txt"));
    (dir, recent)
}

/// 跑一帧，交出屏上画出来的字。
fn 跑一帧(ctx: &egui::Context, program: &mut Program) -> String {
    let out = headless::frame(ctx, headless::input(), |ui| program.ui(ui));
    shared::画出来的字(&out)
}

/// 按一下屏上写着 `那一段` 的地方（移过去、按下、松开），返回**松开之后**那一帧画出来的字。
///
/// 多跑一帧才收，是因为两态那一下换在帧末：开场挑中一份、开出现场、换成主窗口，
/// 主窗口下一帧才画得出来。
fn 点一下(ctx: &egui::Context, program: &mut Program, 那一段: &str) -> String {
    let 头一帧 = headless::frame(ctx, headless::input(), |ui| program.ui(ui));
    let Some(位置) = shared::那一段画在哪儿(&头一帧, 那一段) else {
        panic!(
            "屏上没有「{那一段}」，没处点：\n{}",
            shared::画出来的字(&头一帧)
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
    headless::frame(ctx, input, |ui| program.ui(ui));
    let mut input = headless::input();
    input.events.push(按(false));
    headless::frame(ctx, input, |ui| program.ui(ui));
    跑一帧(ctx, program)
}

#[test]
fn 一个参数都不给看见的是开场列着这个工作目录里的库() {
    // 双击图标、一个参数都不给：从前这条路是死路（报「说清要开哪份库」并退出），
    // 现在它通到**开场**——这个工作目录里有哪些**中立库**，一份一行（ADR-0023）。
    let 工作目录 = temp_dir("gui-program-开场列出来");
    let 库文件 = 建一份像样的库(工作目录.path(), "我的主库", 3, 1_700_000_000);

    let (_记忆, 记的) = 记在临时处("gui-program-开场列出来-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();
    let 屏上 = 跑一帧(&ctx, &mut program);

    assert!(屏上.contains("我的主库"), "屏上没有主库名：\n{屏上}");
    // 断的是整段而不是那个数字：`human_time` 画出来的时刻自己就含数字，
    // 光断一个 `'3'` 那条断言几乎不可能红。
    assert!(屏上.contains("3 个变体"), "屏上没有变体数：\n{屏上}");
    assert!(
        屏上.contains(&human_time(1_700_000_000)),
        "屏上没有上次扫描的时刻：\n{屏上}",
    );
    // **画的是给人看的那个名字，不是中立库的主文件名**：那一串带着十六位哈希，
    // 人认不出自己的哪份库（承票 01 的退路，ADR-0023）。
    assert!(
        !屏上.contains(&主库名(&库文件)),
        "屏上画的是带哈希的那一串：\n{屏上}",
    );
}

#[test]
fn 给一份现成的库就走完启动进主窗口() {
    let (_工作目录, 库文件) = 摆好一份库("gui-program-开进主窗口");
    let (_记忆, 记的) = 记在临时处("gui-program-开进主窗口-记忆");

    let mut program = Program::start_with(
        &Locate {
            catalog: Some(&库文件),
            ..Locate::default()
        },
        记的,
    )
    .expect("走得完启动");

    // **标题写着开的是哪一份**：任务栏上并排两个 romcat 时，那是唯一分得开的地方。
    // 写的是**给人看的**那个名字，不是那串带哈希的主文件名——开场那一屏上画着的与这里
    // 是同一个字，同一份库不该在相邻两屏上长两个样子（ADR-0023、规格 User Story 5）。
    assert!(
        program.window_title().contains(&给人看的名字(&库文件)),
        "标题里没写开的是哪一份库：{}",
        program.window_title(),
    );
    assert!(
        !program.window_title().contains(&主库名(&库文件)),
        "标题里写的是带哈希的那一串：{}",
        program.window_title(),
    );

    // 主窗口真的画出来了：顶栏上五屏的名字都在。
    let ctx = headless::context();
    let out = headless::frame(&ctx, headless::input(), |ui| program.ui(ui));
    let 屏上 = shared::画出来的字(&out);
    for 屏 in ["待确认队列", "库", "浏览", "子库", "任务"] {
        assert!(屏上.contains(屏), "顶栏上没有「{屏}」这一屏：\n{屏上}");
    }
}

#[test]
fn 三种给法开出来的是同一份库() {
    // 三种给法（主库根 / `--library <名字>` / `--catalog <文件>`）折的是同一条算法
    // （`workspace::catalog_path`）。它们在启动那条路上从来没有一起被验过——从前那段
    // 摊在 `main.rs` 里，只能靠人手敲三遍命令。
    let 工作目录 = temp_dir("gui-program-三种给法");
    let 主库根 = temp_dir("gui-program-主库根");

    // 按名字那份与直接给文件那份**是同一个文件**：两条路开出来的主库名必须一模一样，
    // 不然同一份库裁出来的**路径锚**会记在两个名字下。
    let 按名字 = 建一份库(工作目录.path(), Slug::Named("测试库"));
    let 按根 = 建一份库(工作目录.path(), Slug::AtPath(主库根.path()));

    let (_记忆, 记的) = 记在临时处("gui-program-三种给法-记忆");
    let 开出的标题 = |locate: &Locate<'_>| {
        Program::start_with(locate, 记的.clone())
            .expect("走得完启动")
            .window_title()
    };

    let 名字那条 = 开出的标题(&Locate {
        library: Some("测试库"),
        workspace: Some(工作目录.path()),
        ..Locate::default()
    });
    let 文件那条 = 开出的标题(&Locate {
        catalog: Some(&按名字),
        ..Locate::default()
    });
    let 根那条 = 开出的标题(&Locate {
        root: Some(主库根.path()),
        workspace: Some(工作目录.path()),
        ..Locate::default()
    });

    assert_eq!(
        名字那条, 文件那条,
        "`--library` 与 `--catalog` 指着同一个文件，开出来的却是两个名字",
    );
    assert!(
        名字那条.contains(&给人看的名字(&按名字)),
        "标题里写的不是那份库：{名字那条}",
    );
    assert!(
        根那条.contains(&给人看的名字(&按根)),
        "给主库根开出来的不是它那一份：{根那条}",
    );

    // 标题里写的是**给人看的**那个名字，于是**标识符另比一次**——两条路指着同一个文件，
    // `Site::library` 那一串必须一模一样，不然同一份库裁出来的**路径锚**会记在两个名字下
    // （这一条本来靠标题捎带验着，标题改画人名之后它得自己站出来）。
    let 开出的标识符 = |locate: &Locate<'_>| locate.open().expect("开得出现场").library;
    assert_eq!(
        开出的标识符(&Locate {
            library: Some("测试库"),
            workspace: Some(工作目录.path()),
            ..Locate::default()
        }),
        开出的标识符(&Locate {
            catalog: Some(&按名字),
            ..Locate::default()
        }),
        "同一个文件，两条路记出来的路径锚名字不一样",
    );
}

#[test]
fn 工作目录里一份库都没有时开场说还没有库认领一个主库开始() {
    // 票 02 那一版这条路是死路（报「说清要开哪份库」并退出），这一张把它引到**开场**。
    // 空着的时候那一屏不该只是空的：它得说出这时候唯一做得下去的下一步。
    // **不擅自造一份合成的糊弄人**——假数据与真库在界面上长得一模一样（ADR-0023）。
    let 工作目录 = temp_dir("gui-program-空工作目录");

    let (_记忆, 记的) = 记在临时处("gui-program-空工作目录-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();
    let 屏上 = 跑一帧(&ctx, &mut program);

    // **字面量，不引 `opening::NO_CATALOG`**：引常量的话两边同一个出处，谁把那句话改成
    // 别的，两边一起变而这条测试照样绿。这一条钉的正是「人看见的是这句话」。
    assert!(
        屏上.contains("还没有库，认领一个主库开始"),
        "空工作目录上没说下一步该干什么：\n{屏上}",
    );
}

#[test]
fn 库不在就如实说而不是空手开一个窗() {
    let 工作目录 = temp_dir("gui-program-库不在");
    let (_记忆, 记的) = 记在临时处("gui-program-库不在-记忆");
    let Err(说的) = Program::start_with(
        &Locate {
            library: Some("压根不存在的那一份"),
            workspace: Some(工作目录.path()),
            ..Locate::default()
        },
        记的,
    ) else {
        panic!("那份库根本不在，却开起来了");
    };
    // 断的是**它说得出你要开的是哪一份**，不是核心库那句话的措辞——措辞归
    // `SiteError::NoCatalog` 管，抄一段过来只会在核心改一个字时红。
    assert!(
        说的.contains("压根不存在的那一份"),
        "没说清开不起来的是哪一份：{说的}",
    );
}

#[test]
fn 在开场上选中一行就进主窗口标题写着那一份库() {
    // **这一段过渡整条可测**，正是把接缝抬到顶层换来的东西（票 02、ADR-0023）：
    // 选中一份 → 开出**现场** → 进主窗口 → 标题写着开的是哪一份。
    let 工作目录 = temp_dir("gui-program-选中一行");
    let 库文件 = 建一份像样的库(工作目录.path(), "我的主库", 3, 1_700_000_000);

    let (_记忆, 记的) = 记在临时处("gui-program-选中一行-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();
    assert_eq!(program.window_title(), "romcat — 开场", "开场那一态的标题");

    let 屏上 = 点一下(&ctx, &mut program, "打开");

    // 开场退场，主窗口在画：顶栏上五屏的名字都在。
    for 屏 in ["待确认队列", "库", "浏览", "子库", "任务"] {
        assert!(屏上.contains(屏), "顶栏上没有「{屏}」这一屏：\n{屏上}");
    }
    // **标题写的与开场那一行画的是同一个字**：人在开场看见「我的主库」，进去标题还是
    // 「我的主库」，不会变成一串带哈希的文件名（验收第 6 条，票 01 的名字在这儿兑现）。
    assert!(
        program.window_title().contains("我的主库"),
        "标题里没写开的是哪一份库：{}",
        program.window_title(),
    );
    assert!(
        !program.window_title().contains(&主库名(&库文件)),
        "标题里写的是带哈希的那一串：{}",
        program.window_title(),
    );
}

#[test]
fn 结构版本对不上的库照列并说清是哪个版本对哪个版本() {
    // 用户机器上真有三份这样的库（ADR-0023：三个工作目录各一份，版本 4，本程序认 7），
    // 开场那一屏上线第一眼看见的就是它们。**照列不误**——从列表里静静消失才是最难查的
    // 那种错；而**一份打不开不许连累其余**，好的那一份照样列得出、开得了。
    let 工作目录 = temp_dir("gui-program-版本对不上");
    建一份像样的库(工作目录.path(), "开得了的库", 2, 1_700_000_000);
    let 旧的 = workspace::catalog_path(工作目录.path(), Slug::Named("版本对不上的库"));
    testing::catalog_at_version(&旧的, 4);

    let (_记忆, 记的) = 记在临时处("gui-program-版本对不上-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();
    let 屏上 = 跑一帧(&ctx, &mut program);

    assert!(
        屏上.contains("版本对不上的库"),
        "那一份从列表里静静消失了：\n{屏上}",
    );
    assert!(
        屏上.contains("结构版本是 4"),
        "没说清库里是哪个版本：\n{屏上}"
    );
    assert!(
        屏上.contains(&format!("本程序认得的是 {SCHEMA_VERSION}")),
        "没说清本程序认哪个版本：\n{屏上}",
    );
    assert!(屏上.contains("删掉它重扫一遍"), "没说该怎么办：\n{屏上}");

    // 其余照列，连它那几个数一起。
    assert!(屏上.contains("开得了的库"), "好的那一份被连累了：\n{屏上}");
    assert!(屏上.contains("2 个变体"), "好的那一份数没画出来：\n{屏上}");
}

/// 往屏上那个写着 `框上写着` 的输入框里打一段字。
///
/// 先点一下把焦点放进去，再发一条文本事件——egui 把文本事件交给**拿着焦点**的那个
/// 控件，没有第二条路把字送进去。
fn 打字(ctx: &egui::Context, program: &mut Program, 框上写着: &str, 字: &str) {
    点一下(ctx, program, 框上写着);
    let mut input = headless::input();
    input.events.push(egui::Event::Text(字.to_string()));
    headless::frame(ctx, input, |ui| program.ui(ui));
}

#[test]
fn 在开场上换一个工作目录立刻列出那个目录里的库() {
    // 不换的话，那些不在默认位置的库一份都开不出来——默认工作目录那条链认的是环境变量
    // （`workspace::default_dir`），而盘在两台机器之间来回接时它常常不是人想开的那个
    // （ADR-0018）。**换完立刻看见那个目录里的库**，不必重启。
    let 甲 = temp_dir("gui-program-换目录-甲");
    let 乙 = temp_dir("gui-program-换目录-乙");
    建一份像样的库(甲.path(), "甲那边的库", 1, 1_700_000_000);
    建一份像样的库(乙.path(), "乙那边的库", 2, 1_700_000_000);

    let (_记忆, 记的) = 记在临时处("gui-program-换目录-记忆");
    let mut program = 开场(甲.path(), 记的);
    let ctx = headless::context();
    let 屏上 = 跑一帧(&ctx, &mut program);
    assert!(屏上.contains("甲那边的库"), "一开始该看着甲：\n{屏上}");
    assert!(!屏上.contains("乙那边的库"), "还没换就看见乙了：\n{屏上}");

    打字(
        &ctx,
        &mut program,
        "换一个工作目录：把路径贴在这儿",
        &romcat_core::path::display(乙.path()),
    );
    let 屏上 = 点一下(&ctx, &mut program, "换过去");

    assert!(屏上.contains("乙那边的库"), "换过去了却没列出来：\n{屏上}");
    assert!(
        !屏上.contains("甲那边的库"),
        "换过去了，甲那边的库还挂在屏上：\n{屏上}",
    );
}

#[test]
fn 列出来之后那一份被挪走了就在开场上说清而不是空手换屏() {
    // 列一遍与按下「打开」之间隔着人的一次犹豫，那段时间里文件可能被挪走、被删掉。
    // 这时候**留在开场并说清为什么**，而不是换到一扇空手开出来的窗——
    // 「开不了」得当场说得出（ADR-0023：列得出、开得起、说得清为什么开不了）。
    let 工作目录 = temp_dir("gui-program-列完被挪走");
    let 库文件 = 建一份像样的库(工作目录.path(), "待会儿就没了的库", 1, 1_700_000_000);

    let (_记忆, 记的) = 记在临时处("gui-program-列完被挪走-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();
    let 屏上 = 跑一帧(&ctx, &mut program);
    assert!(屏上.contains("待会儿就没了的库"), "先得列出来：\n{屏上}");

    std::fs::remove_file(&库文件).expect("删得掉那个文件");
    let 屏上 = 点一下(&ctx, &mut program, "打开");

    assert_eq!(
        program.window_title(),
        "romcat — 开场",
        "开不出来却换到主窗口去了",
    );
    assert!(屏上.contains("还没有"), "没说清为什么开不出来：\n{屏上}");
}

// ——— 上次开的那份 ———

#[test]
fn 开过一份库之后再启动直接进主窗口不经过开场() {
    // **开场不该每天挡在你前面**（ADR-0023）：开过一份之后，下一趟启动直接进主窗口。
    // 记的是**那份中立库的完整路径**，而工作目录由它反推得到——记一条路径两样都拿得到，
    // 也不会两样对不上（验收第 1、2 条）。
    let 工作目录 = temp_dir("gui-program-记住上次那份");
    let 库文件 = 建一份像样的库(工作目录.path(), "我的主库", 3, 1_700_000_000);
    let (_记忆, 记的) = 记在临时处("gui-program-记住上次那份-记忆");
    let 记在哪儿 = 记的.path().to_path_buf();

    // 头一趟：一个参数都不给，看见的是开场，挑一份开进去。
    let mut 头一趟 = 开场(工作目录.path(), 记的.clone());
    assert_eq!(头一趟.window_title(), "romcat — 开场", "头一趟该看见开场");
    点一下(&headless::context(), &mut 头一趟, "打开");

    // 第二趟：同样一个参数都不给——这一回开场不再挡在前面。
    let mut 第二趟 = 开场(工作目录.path(), 记的);
    assert!(
        第二趟.window_title().contains("我的主库"),
        "没直接进上次那份库：{}",
        第二趟.window_title(),
    );
    let 屏上 = 跑一帧(&headless::context(), &mut 第二趟);
    for 屏 in ["待确认队列", "库", "浏览", "子库", "任务"] {
        assert!(屏上.contains(屏), "顶栏上没有「{屏}」这一屏：\n{屏上}");
    }
    assert!(
        !屏上.contains("挑一份库开进去"),
        "又被开场挡了一道：\n{屏上}",
    );
    // **工作目录是从那条路径反推出来的**：沉淀库跟着工作目录走（`工作目录/verdict/…`），
    // 顶栏上写着它在哪儿——反推错了这一句就指到别处去了（验收第 2 条）。
    assert!(
        屏上.contains(&romcat_core::path::display(工作目录.path())),
        "反推出来的不是那份库住的工作目录：\n{屏上}",
    );
    // 记下的是**完整路径**，不是工作目录、也不是库名。
    assert!(
        std::fs::read_to_string(&记在哪儿)
            .expect("记得下来")
            .contains(&romcat_core::path::display(&库文件)),
        "记下的不是那份中立库的完整路径",
    );
}

#[test]
fn 记的那份被挪走了就退回开场并说清是哪一条原因() {
    // 验收第 4 条：**不是报一句语焉不详的错就没了下文**。人期待的是直接进主窗口，
    // 突然看见开场就得当场说得出为什么——而那句话由核心库一处出（ADR-0005）。
    let 工作目录 = temp_dir("gui-program-记的那份没了");
    let 库文件 = 建一份像样的库(工作目录.path(), "待会儿就没了的库", 1, 1_700_000_000);
    建一份像样的库(工作目录.path(), "还在的那份库", 2, 1_700_000_000);
    let (_记忆, 记的) = 记在临时处("gui-program-记的那份没了-记忆");

    let mut 头一趟 = 开场(工作目录.path(), 记的.clone());
    点一下(&headless::context(), &mut 头一趟, "打开");
    assert!(
        头一趟.window_title().contains("待会儿就没了的库"),
        "头一趟没开进那一份：{}",
        头一趟.window_title(),
    );

    std::fs::remove_file(&库文件).expect("删得掉那个文件");
    let mut 第二趟 = 开场(工作目录.path(), 记的);

    assert_eq!(
        第二趟.window_title(),
        "romcat — 开场",
        "记的那份都没了，却还是开进了主窗口",
    );
    let 屏上 = 跑一帧(&headless::context(), &mut 第二趟);
    assert!(
        屏上.contains("上次开的那份现在开不了"),
        "没说人为什么会看见这一屏：\n{屏上}",
    );
    // 是**哪一条原因**：核心库那句原话说的正是「还没有这一份」——文件不在了。
    assert!(屏上.contains("还没有"), "没说清是哪一条原因：\n{屏上}");
    // 退回开场不是死路：这个工作目录里其余那些照列不误，挑一份就能接着干活。
    assert!(
        屏上.contains("还在的那份库"),
        "退回开场却列不出别的库：\n{屏上}"
    );
}

#[test]
fn 记的那份结构版本对不上时说的是核心库那句原话() {
    // 用户机器上真有三份这样的库（ADR-0023：版本 4，本程序认 7）。**措辞由核心库一处
    // 出**，界面一个字都不改写——两处各写一套，人在终端里看见的与在界面上看见的迟早
    // 分家（ADR-0005）。
    let 工作目录 = temp_dir("gui-program-记的那份版本对不上");
    let 库文件 = 建一份像样的库(工作目录.path(), "会被换掉的库", 1, 1_700_000_000);
    let (_记忆, 记的) = 记在临时处("gui-program-记的那份版本对不上-记忆");

    let mut 头一趟 = 开场(工作目录.path(), 记的.clone());
    点一下(&headless::context(), &mut 头一趟, "打开");

    // 就地把它换成一份版本对不上的：记的那条路径没变，变的是那个文件。
    std::fs::remove_file(&库文件).expect("删得掉那个文件");
    testing::catalog_at_version(&库文件, 4);

    let mut 第二趟 = 开场(工作目录.path(), 记的);
    assert_eq!(
        第二趟.window_title(),
        "romcat — 开场",
        "版本对不上却开进去了"
    );
    let 屏上 = 跑一帧(&headless::context(), &mut 第二趟);

    assert!(
        屏上.contains("上次开的那份现在开不了"),
        "没说人为什么会看见这一屏：\n{屏上}",
    );
    assert!(
        屏上.contains("结构版本是 4"),
        "没说清库里是哪个版本：\n{屏上}"
    );
    assert!(
        屏上.contains(&format!("本程序认得的是 {SCHEMA_VERSION}")),
        "没说清本程序认哪个版本：\n{屏上}",
    );
    assert!(屏上.contains("删掉它重扫一遍"), "没说该怎么办：\n{屏上}");
}

#[test]
fn 在主窗口里回到开场换一份库换过之后记的是新那份() {
    // 一条完整的路：进主窗口 → **主动回开场**（验收第 5 条）→ 换一个工作目录 → 开另一份
    // → 下一趟启动进的是**新那份**（验收第 6 条）。中间那一步换过工作目录，而这份记忆
    // **不属于任何工作目录**，所以它没被弄丢（验收第 3 条）。
    let 甲 = temp_dir("gui-program-换一份库-甲");
    let 乙 = temp_dir("gui-program-换一份库-乙");
    建一份像样的库(甲.path(), "甲那边的库", 1, 1_700_000_000);
    建一份像样的库(乙.path(), "乙那边的库", 2, 1_700_000_000);
    let (_记忆, 记的) = 记在临时处("gui-program-换一份库-记忆");
    // 那份记忆**不住在这两个工作目录里的任何一个**——这一条是下面「换过工作目录也没丢」
    // 之所以成立的原因。
    assert!(
        !记的.path().starts_with(甲.path()),
        "那份记忆落进甲那个工作目录里了"
    );
    assert!(
        !记的.path().starts_with(乙.path()),
        "那份记忆落进乙那个工作目录里了"
    );

    let ctx = headless::context();
    let mut program = 开场(甲.path(), 记的.clone());
    点一下(&ctx, &mut program, "打开");
    assert!(
        program.window_title().contains("甲那边的库"),
        "先得开进甲那边那一份：{}",
        program.window_title(),
    );

    // **主窗口里那条回开场的路**：不必关掉程序重开。
    let 屏上 = 点一下(&ctx, &mut program, "换一份库");
    assert_eq!(program.window_title(), "romcat — 开场", "没回到开场");
    assert!(屏上.contains("甲那边的库"), "回到的开场没列出库：\n{屏上}");

    打字(
        &ctx,
        &mut program,
        "换一个工作目录：把路径贴在这儿",
        &romcat_core::path::display(乙.path()),
    );
    点一下(&ctx, &mut program, "换过去");
    点一下(&ctx, &mut program, "打开");
    assert!(
        program.window_title().contains("乙那边的库"),
        "没开进乙那边那一份：{}",
        program.window_title(),
    );

    // 下一趟启动：一个参数都不给（双击图标那一下），进的是**乙**那一份。
    let mut 下一趟 = Program::start_with(&Locate::default(), 记的).expect("走得完启动");
    assert!(
        下一趟.window_title().contains("乙那边的库"),
        "记着的还是换库之前那一份：{}",
        下一趟.window_title(),
    );
    let 屏上 = 跑一帧(&headless::context(), &mut 下一趟);
    assert!(屏上.contains("待确认队列"), "没直接进主窗口：\n{屏上}");
}

#[test]
fn 说了要开哪一份时参数说了算记着的那份看都不看() {
    // 验收第 7 条。人当场说的话比几天前那一次更清楚——而且**命令行那条路正是为了
    // 「今天要开的不是常开的那一份」而在**（ADR-0018 的双机工作流）。
    let 工作目录 = temp_dir("gui-program-参数优先");
    let 记着的那份 = 建一份像样的库(工作目录.path(), "记着的那份库", 1, 1_700_000_000);
    建一份像样的库(工作目录.path(), "参数说的那份库", 2, 1_700_000_000);
    let (_记忆, 记的) = 记在临时处("gui-program-参数优先-记忆");

    // 先开一趟，把「上次开的那份」坐实。
    let 头一趟 = Program::start_with(
        &Locate {
            catalog: Some(&记着的那份),
            ..Locate::default()
        },
        记的.clone(),
    )
    .expect("走得完启动");
    assert!(头一趟.window_title().contains("记着的那份库"));

    // 直接给中立库文件那一条。
    let 文件那条 = Program::start_with(
        &Locate {
            catalog: Some(&workspace::catalog_path(
                工作目录.path(),
                Slug::Named("参数说的那份库"),
            )),
            ..Locate::default()
        },
        记的.clone(),
    )
    .expect("走得完启动");
    assert!(
        文件那条.window_title().contains("参数说的那份库"),
        "`--catalog` 说的那一份被记着的那份盖过去了：{}",
        文件那条.window_title(),
    );

    // 按名字那一条。**记着的那份此刻记的已经是「参数说的那份库」**——上一句开成了，
    // 那就是「上次开的那份」（下面那一条断言钉的正是这个）。
    let 名字那条 = Program::start_with(
        &Locate {
            library: Some("记着的那份库"),
            workspace: Some(工作目录.path()),
            ..Locate::default()
        },
        记的.clone(),
    )
    .expect("走得完启动");
    assert!(
        名字那条.window_title().contains("记着的那份库"),
        "`--library` 说的那一份被记着的那份盖过去了：{}",
        名字那条.window_title(),
    );

    // **参数开出来的那一份照样记下来**：「上次开的那份」说的就是上一次真的开进主窗口的
    // 那一份，不论它是怎么被指出来的。不然拿命令行开过一趟之后，双击图标进的会是更早
    // 之前那一份——而人刚刚换过库这件事，程序看得见。
    assert_eq!(记的.read().as_deref(), Some(记着的那份.as_path()));
}

#[test]
fn 说了工作目录时记的那份不在那儿就不算数() {
    // `--workspace <目录>` 说的是「这一趟在这儿干活」，而**换一个工作目录等于换一整套
    // 工具状态**（词表**工作目录**那一条）。记着的那份落在别的工作目录里时听记忆的，
    // 就等于把人当场说的话当没听见。对不上就退回开场，列出来的正是他说的那个目录里的库。
    let 甲 = temp_dir("gui-program-记的不在这个目录-甲");
    let 乙 = temp_dir("gui-program-记的不在这个目录-乙");
    建一份像样的库(甲.path(), "甲那边的库", 1, 1_700_000_000);
    建一份像样的库(乙.path(), "乙那边的库", 2, 1_700_000_000);
    let (_记忆, 记的) = 记在临时处("gui-program-记的不在这个目录-记忆");

    let ctx = headless::context();
    let mut 头一趟 = 开场(甲.path(), 记的.clone());
    点一下(&ctx, &mut 头一趟, "打开");
    assert!(
        头一趟.window_title().contains("甲那边的库"),
        "先得开进甲那一份"
    );

    // 记着的是甲那边那一份，这一趟说的却是乙那个工作目录。
    let mut 第二趟 = 开场(乙.path(), 记的);
    assert_eq!(
        第二趟.window_title(),
        "romcat — 开场",
        "记的那份不在这个工作目录里，却照样开了进去",
    );
    let 屏上 = 跑一帧(&headless::context(), &mut 第二趟);
    assert!(
        屏上.contains("乙那边的库"),
        "没列出他说的那个目录里的库：\n{屏上}"
    );
    assert!(
        !屏上.contains("甲那边的库"),
        "列出了别的工作目录里的库：\n{屏上}"
    );
}

#[test]
fn 记的那份读不动时说的也是核心库那句原话() {
    // 验收第 4 条那三种原因里的**第三种**：文件还在、结构版本也无从谈起——它压根不是
    // 一份中立库了（人手改坏、拷贝拷了一半、被别的东西占着写坏）。这一条与前两条画的
    // 是同一处，说的仍旧是核心库那句原话。
    let 工作目录 = temp_dir("gui-program-记的那份读不动");
    let 库文件 = 建一份像样的库(工作目录.path(), "会被改坏的库", 1, 1_700_000_000);
    let (_记忆, 记的) = 记在临时处("gui-program-记的那份读不动-记忆");

    let mut 头一趟 = 开场(工作目录.path(), 记的.clone());
    点一下(&headless::context(), &mut 头一趟, "打开");

    std::fs::write(&库文件, "这不是一份 SQLite 库").expect("改得坏那个文件");

    let mut 第二趟 = 开场(工作目录.path(), 记的);
    assert_eq!(
        第二趟.window_title(),
        "romcat — 开场",
        "那份库已经不是一份库了，却还是开了进去",
    );
    let 屏上 = 跑一帧(&headless::context(), &mut 第二趟);
    assert!(
        屏上.contains("上次开的那份现在开不了"),
        "没说人为什么会看见这一屏：\n{屏上}",
    );
    // **不是「还没有」也不是「结构版本」**：三种原因说出来的不是同一句话，人照着这句话
    // 该做的事也不一样（去找找挪哪儿去了 / 删掉重扫 / 这份文件坏了）。
    assert!(
        !屏上.contains("还没有") && !屏上.contains("结构版本"),
        "把「读不动」说成了另外两种原因：\n{屏上}",
    );
    assert!(
        屏上.contains("中立库打不开"),
        "没说清是哪一条原因：\n{屏上}"
    );
}
