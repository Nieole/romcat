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
///
/// 记下的主库原名取**主文件名剥掉哈希后缀剩下的那一半**，好让 `给人看的名字` 照着文件名
/// 就说得出它来。
fn 建一份库(workspace: &Path, slug: Slug<'_>) -> std::path::PathBuf {
    let path = workspace::catalog_path(workspace, slug);
    drop(Catalog::create(&path, workspace::readable_half(&slug.text())).expect("开得出中立库"));
    path
}

/// 那份中立库的主文件名——这份主库的**主库标识**，路径锚里记的就是它（`Site::library_identity`）。
///
/// 它带着十六位哈希后缀。屏上与标题上都不该出现它，几条测试拿它做反向
/// 断言（`Site::display_name` 交出来的是另一个）。
fn 主库标识(catalog: &Path) -> String {
    catalog
        .file_stem()
        .expect("有主文件名")
        .to_string_lossy()
        .into_owned()
}

/// 那份库**给人看的**名字——窗口标题与开场那一行上写着的那个。
///
/// 这几份夹具库建库时记下的就是主文件名剥掉哈希后缀剩下的那一半（`建一份库`），
/// 与票 01 那条退路（`workspace::readable_half`）交出来的是同一串。
fn 给人看的名字(catalog: &Path) -> String {
    workspace::readable_half(&主库标识(catalog)).to_string()
}

/// 一个工作目录，连它里头那份现成的库。
fn 摆好一份库(tag: &str) -> (TempDir, std::path::PathBuf) {
    let 工作目录 = temp_dir(tag);
    let 库文件 = 建一份库(工作目录.path(), Slug::Named("测试库"));
    (工作目录, 库文件)
}

// ——— 开场那一屏 ———

/// 开场那一行要画的三样全在这份库里：**主库原名**（元数据表那一行）、**变体数**、
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
    let mut catalog = Catalog::create(&库文件, &slug.display_name()).expect("开得出中立库");
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

    assert!(屏上.contains("我的主库"), "屏上没有主库原名：\n{屏上}");
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
        !屏上.contains(&主库标识(&库文件)),
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
        !program.window_title().contains(&主库标识(&库文件)),
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

    // 按名字那份与直接给文件那份**是同一个文件**：两条路开出来的主库标识必须一模一样，
    // 不然同一份库裁出来的**路径锚**会记在两个标识下。
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

    // 标题里写的是**给人看的**那个名字，于是**主库标识另比一次**——两条路指着同一个文件，
    // `Site::library_identity` 那一串必须一模一样，不然同一份库裁出来的**路径锚**会记在两个标识下
    // （这一条本来靠标题捎带验着，标题改画人名之后它得自己站出来）。
    let 开出的主库标识 = |locate: &Locate<'_>| locate.open().expect("开得出现场").library_identity;
    assert_eq!(
        开出的主库标识(&Locate {
            library: Some("测试库"),
            workspace: Some(工作目录.path()),
            ..Locate::default()
        }),
        开出的主库标识(&Locate {
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
        屏上.contains("还没有库，添加一个主库开始"),
        "空工作目录上没说下一步该干什么：\n{屏上}",
    );
}

#[test]
fn 工作目录读不动时开场说的是读不动而不是还没有库() {
    // ADR-0021 那条修订、挂单 `Q388`：**读不动与空的是两态**。从前那个目录列不开就当空的，
    // 屏上说「还没有库，添加一个主库开始」——指着人去建第二份库，而该做的是去修那个目录的权限。
    //
    // 造的是**真的**读不动的目录：里头明明有一份库，权限位一收就列不开。造不出来（不是
    // Unix、或者跑测试的是 root）就如实跳过，`testing::revoke` 印出为什么。
    let 工作目录 = temp_dir("gui-program-收了读权限");
    let 库文件 = 建一份像样的库(工作目录.path(), "列不开也在的库", 1, 1_700_000_000);
    let 目录 = 库文件.parent().expect("有那个目录").to_path_buf();
    let Some(_还回去) = testing::revoke(&目录, testing::Revoke::Read) else {
        return;
    };

    let (_记忆, 记的) = 记在临时处("gui-program-收了读权限-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();
    let 屏上 = 跑一帧(&ctx, &mut program);

    assert!(
        !屏上.contains("还没有库，添加一个主库开始"),
        "读不动的目录被说成了空的：\n{屏上}",
    );
    assert!(屏上.contains("读不动"), "屏上没说那个目录读不动：\n{屏上}");
    assert!(
        屏上.contains(&romcat_core::path::display(&目录)),
        "屏上没说清是哪个目录读不动：\n{屏上}",
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
        !program.window_title().contains(&主库标识(&库文件)),
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
    // 断的是开场左边那句大标题：开场在画，它就在。
    assert!(
        !屏上.contains("管理你的模拟器游戏库"),
        "又被开场挡了一道：\n{屏上}",
    );
    // **工作目录是从那条路径反推出来的**：沉淀库跟着工作目录走（`工作目录/verdict/…`），
    // 顶栏上写着它在哪儿——反推错了这一句就指到别处去了（验收第 2 条）。
    assert!(
        屏上.contains(&romcat_core::path::display(工作目录.path())),
        "反推出来的不是那份库住的工作目录：\n{屏上}",
    );
    // 记下的是**完整路径**，不是工作目录、也不是主库标识或主库原名。
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

// ——— 认领一个新主库 ———

#[test]
fn 开场上按下认领新主库看见的是起名那一步() {
    // **开场不是一个只能选中已有库的列表**：它同时是「这个工作目录里从零开始」的入口
    // （ADR-0023：界面自足，从第一步起不必开终端）。空工作目录上那句「还没有库，
    // 认领一个主库开始」指的正是这颗按钮。
    let 工作目录 = temp_dir("gui-program-认领入口");
    let (_记忆, 记的) = 记在临时处("gui-program-认领入口-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();

    let 屏上 = 点一下(&ctx, &mut program, "添加主库");

    assert!(
        屏上.contains("给这个主库起个名字"),
        "按下去没看见起名那一步：\n{屏上}",
    );
}

#[test]
fn 起名那一步就拦下这个工作目录里已经在用的名字说的是核心库那句原话() {
    // 验收第 3 条：**当场说，而不是等你选完目录、按下开始才发现**。收不收问的是核心库建库
    // 入口那一处判断（`Catalog::refuse_create`，挂单 `Q524`）——向导自己另判一份的话，
    // 拦得住的与建库时拦的迟早不是一回事。
    let 工作目录 = temp_dir("gui-program-认领撞名");
    建一份像样的库(工作目录.path(), "我的主库", 1, 1_700_000_000);
    let (_记忆, 记的) = 记在临时处("gui-program-认领撞名-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();

    点一下(&ctx, &mut program, "添加主库");
    打字(&ctx, &mut program, "主库原名，例如", "我的主库");
    let 屏上 = 点一下(&ctx, &mut program, "下一步");

    // 那份库的主库标识与原名都是「我的主库」：按这个名字折出来的**正是它那个文件**，于是核心库
    // 答的是「那个文件已经在了」——与建库那一下碰盘时撞上的是同一句。只撞原名的那一种由
    // `向导起的名字撞上一份改过名的库时起名那一步就拦下` 钉着。
    let 那份 = workspace::catalog_path(工作目录.path(), Slug::Named("我的主库"));
    assert!(
        屏上.contains("已经在了") && 屏上.contains(&romcat_core::path::display(&那份)),
        "撞名了却放它过去、或者说的不是核心库那句原话：\n{屏上}",
    );
    // **还站在第一步**：过了这一步再拦，人就已经填完目录了。
    assert!(
        屏上.contains("给这个主库起个名字"),
        "撞名了却走到了下一步：\n{屏上}",
    );
    assert!(
        !屏上.contains("选第一个根"),
        "撞名了却走到了下一步：\n{屏上}"
    );
}

#[test]
fn 向导起名留空时说的是核心库那句空白() {
    // 挂单 `Q524`：「空白不是名字」只在核心库那一处判，向导不再自己另判一份——两处各判
    // 一次，迟早判出两个答案（ADR-0024）。
    let 工作目录 = temp_dir("gui-program-起名什么都不填");
    let (_记忆, 记的) = 记在临时处("gui-program-起名什么都不填-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();

    点一下(&ctx, &mut program, "添加主库");
    let 屏上 = 点一下(&ctx, &mut program, "下一步");

    assert!(
        屏上.contains("主库原名不能是空白"),
        "名字空着却没说、或者说的不是核心库那句原话：\n{屏上}",
    );
    assert!(
        屏上.contains("给这个主库起个名字") && !屏上.contains("选第一个根"),
        "名字空着却走到了下一步：\n{屏上}",
    );
}

#[test]
fn 工作目录写不动时向导起名那一步就说清() {
    // 票 04：添加主库那条路正要往工作目录里写。目录写不动时从前要到「开始扫描」那一下才撞上
    // 一句「建不出来」——人已经把名字、目录都填完了。现在起名那一步就说清，而且**问的时候
    // 一个字节都不写**（词表**添加主库**：点「开始扫描」之前一个文件都不写）。
    //
    // 造的是**真的**写不动的目录；造不出来（不是 Unix、或者跑测试的是 root）就如实跳过，
    // `testing::revoke` 印出为什么。
    let 工作目录 = temp_dir("gui-program-收了写权限");
    let Some(_还回去) = testing::revoke(工作目录.path(), testing::Revoke::Write) else {
        return;
    };
    let (_记忆, 记的) = 记在临时处("gui-program-收了写权限-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();

    点一下(&ctx, &mut program, "添加主库");
    打字(&ctx, &mut program, "主库原名，例如", "我的主库");
    let 屏上 = 点一下(&ctx, &mut program, "下一步");

    assert!(屏上.contains("写不动"), "工作目录写不动却没说：\n{屏上}");
    assert!(
        屏上.contains("给这个主库起个名字") && !屏上.contains("选第一个根"),
        "工作目录写不动却走到了下一步：\n{屏上}",
    );
    // 「问一遍一个字节都不写」在写不动的目录上验不出来（本来就写不进去），由
    // `向导走到第二步就放弃时工作目录里一个文件都不多` 在写得动的工作目录上钉着——
    // 那一条走过的正是起名这一问。
}

/// 一块**摆好东西的盘**：向导第二步要选的那个目录。
///
/// **只往临时目录里写**，一个字节都不碰真库（ADR-0004）。里头得真有东西——空目录扫得完,
/// 但扫出零个变体，那时「扫描真的跑起来了」就验不着了。
fn 摆一块盘(tag: &str) -> TempDir {
    let dir = temp_dir(tag);
    for (相对, 大小) in [("FC/魂斗罗.zip", 2048), ("SFC/幻想传说.zip", 4096)] {
        let path = dir.path().join(相对);
        std::fs::create_dir_all(path.parent().expect("有上级目录")).expect("建得出目录");
        std::fs::write(&path, romcat_core::testing::sample::zip(大小)).expect("写得下");
    }
    dir
}

/// 跑一帧，交出这一帧的产出——要问「那一段画在哪儿」时用。
fn 跑一帧的产出(ctx: &egui::Context, program: &mut Program) -> egui::FullOutput {
    headless::frame(ctx, headless::input(), |ui| program.ui(ui))
}

/// 屏上**正好**写着 `那一段` 的每一处（中心点），按画出来的次序。
///
/// 「含着」不够用的几处要它：弹层标头那一排里的「开始扫描」与页脚上那一颗、开场上几颗按钮与
/// 旁边句子里恰好含着同几个字的那几句。
fn 正好画着的(output: &egui::FullOutput, 那一段: &str) -> Vec<egui::Pos2> {
    fn 找(shape: &egui::epaint::Shape, 那一段: &str, out: &mut Vec<egui::Pos2>) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                if text.galley.text() == 那一段 {
                    out.push(egui::Rect::from_min_size(text.pos, text.galley.size()).center());
                }
            }
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    找(one, 那一段, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        找(&clipped.shape, 那一段, &mut out);
    }
    out
}

/// 按一下弹层页脚上写着 `那几个字` 的那一颗，返回松开之后再画一帧画出来的字。
///
/// 不用 [`点一下`]：那一个找的是**头一处含着**这几个字的地方，而「开始扫描」在弹层标头那一排
/// 里也正好有一处（向导走到第几问），标头那句说明里也含着它。页脚是弹层里**最后画**的那一段，
/// 弹层又画在开场那一屏之上，于是按最后一处正好是这几个字的。
fn 按页脚上的(ctx: &egui::Context, program: &mut Program, 那几个字: &str) -> String {
    let 头一帧 = 跑一帧的产出(ctx, program);
    let Some(位置) = 正好画着的(&头一帧, 那几个字).last().copied() else {
        panic!(
            "屏上没有正好写着「{那几个字}」的地方，没处按：\n{}",
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

/// 走到向导第三问：起名 → 选根 → 按「下一步」。返回**按下「下一步」之后**那一帧画出来的字。
///
/// `根名` 空着就是「不填」——那时按目录自己的名字取（验收第 5 条）。选根那一问上被拦下时就停在
/// 那一问上，交回的那一帧页脚上还是「下一步」。
fn 走到第三问(
    ctx: &egui::Context,
    program: &mut Program,
    主库原名: &str,
    根: &Path,
    根名: &str,
) -> String {
    点一下(ctx, program, "添加主库");
    打字(ctx, program, "主库原名，例如", 主库原名);
    点一下(ctx, program, "下一步");
    打字(
        ctx,
        program,
        "那块盘上的目录",
        &romcat_core::path::display(根),
    );
    if !根名.is_empty() {
        打字(ctx, program, "根名（不填", 根名);
    }
    点一下(ctx, program, "下一步")
}

/// 走一趟向导：起名 → 选根 → 读哪儿写哪儿 → 开始扫描。返回**按下之后**那一帧画出来的字。
///
/// 选根那一问上被拦下时没有第三问可走，交回的是被拦下的那一帧。
fn 走一趟向导(
    ctx: &egui::Context,
    program: &mut Program,
    主库原名: &str,
    根: &Path,
    根名: &str,
) -> String {
    let 屏上 = 走到第三问(ctx, program, 主库原名, 根, 根名);
    if 屏上.contains("下一步") {
        return 屏上;
    }
    按页脚上的(ctx, program, "开始扫描")
}

#[test]
fn 在开场上认领一个新主库走完向导就进主窗口() {
    // 验收第 1 条。**全程不必开终端**（ADR-0023）：从一个一份库都没有的工作目录起,
    // 三步走完就是主窗口。
    let 工作目录 = temp_dir("gui-program-认领走完");
    let 盘 = 摆一块盘("gui-program-认领走完-盘");
    let (_记忆, 记的) = 记在临时处("gui-program-认领走完-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();

    let 屏上 = 走一趟向导(&ctx, &mut program, "我的主库", 盘.path(), "主库");

    // 开场退场，主窗口在画。
    for 屏 in ["待确认队列", "库", "浏览", "子库", "任务"] {
        assert!(屏上.contains(屏), "顶栏上没有「{屏}」这一屏：\n{屏上}");
    }
    // **标题写的是人刚起的那个名字**：它这一趟真的落进了中立库的元数据表
    // （`Catalog::create`），而不是从带哈希的文件名截出来的。
    assert!(
        program.window_title().contains("我的主库"),
        "标题里没写认领出来的是哪一份库：{}",
        program.window_title(),
    );
    // **建出来的就是命令行折出来的那个文件**（验收第 7 条的一半）：同一条算法
    // （`workspace::catalog_path` + `Slug::Named`），不是向导另折的一个。
    let 库文件 = workspace::catalog_path(工作目录.path(), Slug::Named("我的主库"));
    assert!(
        库文件.is_file(),
        "向导建出来的库不在命令行找的那个位置：{}",
        库文件.display(),
    );
}

/// 一块**大到几帧之内扫不完**的盘：三十个目录，各一千个小文件。
///
/// 「报得出进度、按得下停下」只有在扫描还**在跑**的时候才验得到，而这几条是从界面进去
/// 的——按一下要跑四帧（找位置、按下、松开、再画一帧）。四百个文件那块盘 121 毫秒就扫完了，
/// 正好落在那四帧里，于是屏上只剩一条历史；后来加到三千个。
///
/// **又加到三万个**（票 `gui-looks-like-the-design/25`，挂单 `Q833`）：任务屏照稿重排之后，
/// 进主窗口那头两帧要头一回排版、光栅化一屏新的中文字，不开优化的构建上实测 289 与
/// 231 毫秒（重排之前 72 与 2 毫秒），三千个文件那一趟 205 毫秒就在这两帧里扫完了。
/// 三万个把扫描拉到两秒上下，比那两帧长出好几倍。**只往临时目录里写**（ADR-0004）。
fn 摆一块大盘(tag: &str) -> TempDir {
    let dir = temp_dir(tag);
    for 组 in 0..30 {
        let 目录 = dir.path().join(format!("SFC/第{组:02}组"));
        std::fs::create_dir_all(&目录).expect("建得出目录");
        for i in 0..1000 {
            std::fs::write(
                目录.join(format!("游戏{i:03}.zip")),
                romcat_core::testing::sample::zip(512),
            )
            .expect("写得下");
        }
    }
    dir
}

/// 跑帧跑到任务台上那一趟收场为止，交出最后那一帧画出来的字。
fn 等台上那一趟收场(ctx: &egui::Context, program: &mut Program) -> String {
    for _ in 0..600 {
        let 屏上 = 跑一帧(ctx, program);
        if 屏上.contains("眼下没有任务在跑。") {
            return 屏上;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("这一趟活迟迟不收场");
}

#[test]
fn 走完向导之后扫描已经排在任务台上报得出进度也按得下停下() {
    // 验收第 2 条。**排扫描走的是库屏那条现成的路**（`roots::Screen::scan`），
    // 不另造第二条——另造一条的话，界面上会有两种扫描：一种写**断点**，一种不写。
    let 工作目录 = temp_dir("gui-program-认领就开扫");
    let 盘 = 摆一块大盘("gui-program-认领就开扫-盘");
    let (_记忆, 记的) = 记在临时处("gui-program-认领就开扫-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();

    let 屏上 = 走一趟向导(&ctx, &mut program, "我的主库", 盘.path(), "主库");

    // **进主窗口的那一下人就看着它在跑**：报得出是哪一趟、跑了多久，也按得下停下。
    assert!(
        屏上.contains("扫描 · 主库"),
        "走完向导，扫描没排在任务台上：\n{屏上}",
    );
    assert!(屏上.contains("已用"), "那一趟报不出进度：\n{屏上}");
    assert!(屏上.contains("停下"), "那一趟按不下停下：\n{屏上}");

    点一下(&ctx, &mut program, "停下");
    let 屏上 = 等台上那一趟收场(&ctx, &mut program);
    // 收场之后台上留一条历史——**那一趟真的排上去过**，不是屏上画了一行好看的。
    assert!(屏上.contains("1 条历史"), "台上没留下这一趟：\n{屏上}");
    assert!(屏上.contains("扫描 · 主库"), "历史里没有这一趟：\n{屏上}");
}

#[test]
fn 向导里不给根名就按那个目录自己的名字取() {
    // 验收第 5 条：常见情况下少填一个框。**兜底那一下不在向导里**——它在库屏加根那个
    // 函数里（`roots::Screen::add_root`），向导只是把两个空框原样递过去。在这儿也折一遍
    // 的话，「不填按什么取」就有两套算法了。
    let 工作目录 = temp_dir("gui-program-认领不给根名");
    let 上级 = temp_dir("gui-program-认领不给根名-上级");
    let 盘 = 上级.path().join("甲盘");
    std::fs::create_dir_all(盘.join("FC")).expect("建得出目录");
    std::fs::write(
        盘.join("FC/魂斗罗.zip"),
        romcat_core::testing::sample::zip(2048),
    )
    .expect("写得下");

    let (_记忆, 记的) = 记在临时处("gui-program-认领不给根名-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();

    let 屏上 = 走一趟向导(&ctx, &mut program, "我的主库", &盘, "");

    // 那一趟活的名字里写着这个根叫什么——它就是那个目录自己的名字。
    assert!(屏上.contains("甲盘"), "根名没按目录自己的名字取：\n{屏上}",);
    // **不是那个兜底词**：`add_root` 只在目录连末级名字都没有时才退成「主库」。
    assert!(
        !屏上.contains("扫描 · 主库"),
        "根名退成兜底那个词了：\n{屏上}",
    );
}

/// 这个目录底下现在有些什么，一条一行，排过序。
///
/// **晚落盘那几条靠它**：断言的是「一个文件都不多」，而那要连子目录一起看——
/// 中立库落在 `<工作目录>/catalog/` 底下，只数顶层的话建出来也看不见。
fn 底下有什么(dir: &Path) -> Vec<String> {
    fn 走(at: &Path, 前缀: &str, out: &mut Vec<String>) {
        let Ok(读到) = std::fs::read_dir(at) else {
            return;
        };
        for 一条 in 读到.flatten() {
            let 名字 = format!("{前缀}{}", 一条.file_name().to_string_lossy());
            if 一条.path().is_dir() {
                走(&一条.path(), &format!("{名字}/"), out);
            }
            out.push(名字);
        }
    }
    let mut out = Vec::new();
    走(dir, "", &mut out);
    out.sort();
    out
}

#[test]
fn 向导里选的根圈进工作目录时被拦下说的是加根那一处的原话() {
    // 验收第 4 条。**中立库不许被圈进主库**（ADR-0004）：它、断点与媒体池都写在工作
    // 目录里。这一条在核心库那一层早就有测试，这儿验的是另外两件事——**向导走完之后
    // 调到的是同一个判断**（那句话逐字来自 `AddRootError::Workspace`），以及**被拦下时
    // 磁盘上什么都没多出来**：判断走在建库之前，不是建完再回头拦。
    let 工作目录 = temp_dir("gui-program-认领圈进工作目录");
    let 手滑 = 工作目录.path().join("roms");
    std::fs::create_dir_all(&手滑).expect("建得出目录");
    let 原样 = 底下有什么(工作目录.path());

    let (_记忆, 记的) = 记在临时处("gui-program-认领圈进工作目录-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();

    let 屏上 = 走一趟向导(&ctx, &mut program, "我的主库", &手滑, "手滑");

    // **核心库那句原话**，界面一个字都不改写（ADR-0005）：人在库屏上加根撞上同一条时
    // 看见的是同一句话。
    assert!(屏上.contains("主库只读"), "没点名那条纪律：\n{屏上}");
    assert!(!屏上.contains("ADR-"), "屏上印着写给开发者的出处：\n{屏上}");
    assert!(
        屏上.contains("与工作目录"),
        "没说清是与什么套在一起：\n{屏上}",
    );
    // **还站在向导里**，没被推进一扇开着空库的主窗口。
    assert_eq!(program.window_title(), "romcat — 开场", "被拦下了却换了屏");
    assert!(
        屏上.contains("选第一个根"),
        "被拦下了却离开了那一步：\n{屏上}"
    );
    // **拦下的那一次一个文件都没留下**：判断走在建库之前。留下的话，那份零根空库会
    // 永久占着开场的一行，而人重走一遍向导还会在第一步撞上自己刚留下的那个名字。
    assert_eq!(
        底下有什么(工作目录.path()),
        原样,
        "被拦下了，工作目录里却多出了东西",
    );
}

#[test]
fn 向导起的名字撞上一份改过名的库时起名那一步就拦下() {
    // **界面与命令行建库问同一处判断**，名字收不收只在核心库判一次（挂单 `Q472`）。改名只换
    // 主库原名、不动主库标识，于是从前第一步只问「这个标识占没占」拦不住它，要到「开始扫描」
    // 那一下才由建库入口拦下——人已经把目录都填完了（挂单 `Q524`）。现在第一步问的就是建库
    // 入口那一处（`Catalog::refuse_create`）。**拦下时说的是核心库那句原话，一份库都没多**。
    let 工作目录 = temp_dir("gui-program-向导撞原名");
    let 改过名的 = 建一份像样的库(工作目录.path(), "起错了的名字", 1, 1_700_000_000);
    Catalog::open(&改过名的)
        .expect("开得出那份库")
        .set_library_name("我的主库")
        .expect("改得了名");
    let (_记忆, 记的) = 记在临时处("gui-program-向导撞原名-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();

    点一下(&ctx, &mut program, "添加主库");
    打字(&ctx, &mut program, "主库原名，例如", "我的主库");
    let 屏上 = 点一下(&ctx, &mut program, "下一步");

    assert!(
        屏上.contains("已经有一份主库叫「我的主库」"),
        "撞上了那份改过名的库却没说：\n{屏上}",
    );
    assert!(
        屏上.contains("给这个主库起个名字") && !屏上.contains("选第一个根"),
        "撞上了那份改过名的库却走到了下一步：\n{屏上}",
    );
    assert_eq!(program.window_title(), "romcat — 开场", "被拦下了却换了屏");
    assert_eq!(
        workspace::catalogs(工作目录.path())
            .entries()
            .expect("列得开")
            .len(),
        1,
        "被拦下了，工作目录里却多出一份库",
    );
    assert!(
        !workspace::catalog_path(工作目录.path(), Slug::Named("我的主库")).exists(),
        "被拦下了，按新名字折出来的那份库却建出来了",
    );
}

#[test]
fn 向导走到第二步就放弃时工作目录里一个文件都不多() {
    // 验收第 6 条，**晚落盘**那一条：起名与选根两步只在内存里攒，按下「开始扫描」才
    // 真的开中立库（开一份库这个动作本身就是建库）。零根的库仍然是合法状态——晚落盘
    // 不是因为零根非法，而是因为**一份零根空库对人没有用处，却会永久占着开场的一行**，
    // 而开场没有删库那条路（ADR-0023）。
    let 工作目录 = temp_dir("gui-program-认领半路放弃");
    let 盘 = 摆一块盘("gui-program-认领半路放弃-盘");
    assert!(
        底下有什么(工作目录.path()).is_empty(),
        "这个工作目录一开始就该是空的",
    );
    let ctx = headless::context();

    // 一、**中途关窗**：走到第二步、两个框都填好，就是不按「开始扫描」。
    {
        let (_记忆, 记的) = 记在临时处("gui-program-认领半路放弃-记忆甲");
        let mut program = 开场(工作目录.path(), 记的);
        点一下(&ctx, &mut program, "添加主库");
        打字(&ctx, &mut program, "主库原名，例如", "我的主库");
        点一下(&ctx, &mut program, "下一步");
        打字(
            &ctx,
            &mut program,
            "那块盘上的目录",
            &romcat_core::path::display(盘.path()),
        );
        打字(&ctx, &mut program, "根名（不填", "主库");
        drop(program);
    }
    assert!(
        底下有什么(工作目录.path()).is_empty(),
        "中途关窗，工作目录里却多出了 {:?}",
        底下有什么(工作目录.path()),
    );

    // 二、**按「算了」**：同一条路，只是人明说不认领了。
    let (_记忆, 记的) = 记在临时处("gui-program-认领半路放弃-记忆乙");
    let mut program = 开场(工作目录.path(), 记的);
    点一下(&ctx, &mut program, "添加主库");
    打字(&ctx, &mut program, "主库原名，例如", "我的主库");
    点一下(&ctx, &mut program, "下一步");
    let 屏上 = 点一下(&ctx, &mut program, "算了");
    assert!(
        屏上.contains("还没有库，添加一个主库开始"),
        "按了「算了」没回到那张表：\n{屏上}",
    );
    assert!(
        底下有什么(工作目录.path()).is_empty(),
        "按了「算了」，工作目录里却多出了 {:?}",
        底下有什么(工作目录.path()),
    );

    // **那个名字还空着**：留下一份空库的话，人重走一遍向导会在第一步撞上自己刚才
    // 那一趟——被自己拦在门外，还看不出为什么。
    点一下(&ctx, &mut program, "添加主库");
    打字(&ctx, &mut program, "主库原名，例如", "我的主库");
    let 屏上 = 点一下(&ctx, &mut program, "下一步");
    assert!(
        屏上.contains("选第一个根"),
        "放弃过一趟之后，那个名字被自己占住了：\n{屏上}",
    );
}

/// 按一下这个键：一帧。
fn 按键(ctx: &egui::Context, program: &mut Program, key: egui::Key) {
    let input = shared::输入(vec![shared::按键事件(key)]);
    headless::frame(ctx, input, |ui| program.ui(ui));
}

#[test]
fn 向导开着时按退出键等于按了算了_工作目录里一个文件都不多() {
    // 向导是一层弹层（票 `gui-looks-like-the-design/04`）：Esc 关最上面那一层，等于按页脚上
    // 退出那一颗——在这条向导上就是「算了」，**磁盘上什么都没留下**。
    let 工作目录 = temp_dir("gui-program-向导按退出键");
    let (_记忆, 记的) = 记在临时处("gui-program-向导按退出键-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();

    点一下(&ctx, &mut program, "添加主库");
    打字(&ctx, &mut program, "主库原名，例如", "我的主库");
    let 第二步 = 点一下(&ctx, &mut program, "下一步");
    assert!(
        第二步.contains("选第一个根"),
        "前提：走到了第二步：\n{第二步}"
    );

    按键(&ctx, &mut program, egui::Key::Escape);
    let 屏上 = 跑一帧(&ctx, &mut program);
    assert!(
        屏上.contains("还没有库，添加一个主库开始") && !屏上.contains("选第一个根"),
        "按了 Esc 没回到那张表：\n{屏上}",
    );
    assert!(
        底下有什么(工作目录.path()).is_empty(),
        "按了 Esc，工作目录里却多出了 {:?}",
        底下有什么(工作目录.path()),
    );
}

#[test]
fn 按下添加主库焦点就落在起名那一框里_不用先拿鼠标点进去() {
    let 工作目录 = temp_dir("gui-program-向导焦点");
    let (_记忆, 记的) = 记在临时处("gui-program-向导焦点-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();

    点一下(&ctx, &mut program, "添加主库");
    跑一帧(&ctx, &mut program);
    // **不点那一框，直接打字**：字该落进起名那一框。
    let mut input = headless::input();
    input.events.push(egui::Event::Text("我的主库".to_string()));
    headless::frame(&ctx, input, |ui| program.ui(ui));
    let 屏上 = 点一下(&ctx, &mut program, "下一步");

    assert!(
        屏上.contains("给「我的主库」选第一个根"),
        "打的字没落进起名那一框：\n{屏上}",
    );
}

#[test]
fn 向导建出来的库与命令行扫出来的库一样命令行接得上() {
    // 验收第 7 条：**同一份现场、同一套路径锚**。向导走的是核心库那两条现成的路
    // （`Catalog::create` 把原名记进元数据表、`workspace::catalog_path` 折文件名），
    // 与 `romcat scan` 建库那一段同源。折出第二个文件名的话，界面建的库命令行按名字
    // 找不着，界面裁出来的**路径锚**命令行也认不出。
    let 工作目录 = temp_dir("gui-program-认领后命令行接得上");
    let 盘 = 摆一块盘("gui-program-认领后命令行接得上-盘");
    let (_记忆, 记的) = 记在临时处("gui-program-认领后命令行接得上-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();

    走一趟向导(&ctx, &mut program, "我的主库", 盘.path(), "主库");
    等台上那一趟收场(&ctx, &mut program);

    // **命令行那三种给法里的「按名字找」那一条**，一字不改地接上来。
    let site = Locate {
        library: Some("我的主库"),
        workspace: Some(工作目录.path()),
        ..Locate::default()
    }
    .open()
    .expect("命令行按名字开得出向导建的那份库");

    // **同一套路径锚**：`Site::library_identity` 就是中立库的主文件名，裁决记在它名下。
    let 库文件 = workspace::catalog_path(工作目录.path(), Slug::Named("我的主库"));
    assert_eq!(
        site.library_identity,
        主库标识(&库文件),
        "两条路记出来的主库标识不一样"
    );
    // **原名真的落进了元数据表**：不落的话它只活在人刚才敲过的那一下里——文件名折过
    // 一道滤字符、截断、缀哈希，谁也从那串字里认不回来（票 01）。
    assert_eq!(site.display_name(), "我的主库", "那份库说不出自己叫什么");
    // **同一份现场**：向导扫进去的东西，命令行这一条看得见。
    assert!(
        site.catalog
            .variant_total(&romcat_core::catalog::VariantQuery::default())
            .expect("数得出")
            > 0,
        "命令行开出来的是另一份空库",
    );
    // **断点也锚在同一处**：界面停下的那一趟，`romcat scan --resume` 接得上。
    assert_eq!(
        site.checkpoint_path(工作目录.path(), "主库"),
        workspace::checkpoint_path(工作目录.path(), Slug::Named("我的主库"), "主库"),
        "界面折出来的断点路径与命令行找的不是同一个文件",
    );
}

#[test]
fn 向导只管第一个根第二个根仍然从库屏加() {
    // 验收第 8 条。**向导不做成第二条加根的路**：它只在「这个工作目录里从零开始」那一下
    // 出现，而那一下每份库只有一次。加第二块盘是库屏的活——那一屏的自述就是「这个库由
    // 什么构成」，而向导这时候连屏都不在了。
    let 工作目录 = temp_dir("gui-program-认领只管第一个根");
    let 甲 = 摆一块盘("gui-program-认领只管第一个根-甲");
    let 乙 = 摆一块盘("gui-program-认领只管第一个根-乙");
    let (_记忆, 记的) = 记在临时处("gui-program-认领只管第一个根-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();

    走一趟向导(&ctx, &mut program, "我的主库", 甲.path(), "甲盘");
    等台上那一趟收场(&ctx, &mut program);

    // **主窗口里没有那颗按钮**：认领是开场的事，进来之后再摆一颗只会让人以为
    // 「加第二块盘」也走它。
    let 屏上 = 点一下(&ctx, &mut program, "库");
    assert!(
        !屏上.contains("添加主库"),
        "主窗口里也摆着添加主库那颗按钮：\n{屏上}",
    );

    // 第二个根从库屏那两个框加进去。
    打字(
        &ctx,
        &mut program,
        "那块盘上的目录",
        &romcat_core::path::display(乙.path()),
    );
    打字(&ctx, &mut program, "根名（不填", "乙盘");
    let 屏上 = 点一下(&ctx, &mut program, "+ 添加目录");

    assert!(屏上.contains("甲盘"), "向导加的那个根没了：\n{屏上}");
    assert!(屏上.contains("乙盘"), "第二个根没加上：\n{屏上}");
    assert!(屏上.contains("根 · 2 个"), "这个库该有两个根了：\n{屏上}");
}

#[test]
fn 向导建完库进主窗口之后那一份被记住下一趟直接进它() {
    // 认领出来的那一份就是**上次开的那份**（票 `gui-self-sufficient/04`）：认领完第二天
    // 再打开，开场不该又挡在前面——而这一份恰恰是人最会天天开的那一份。
    // 记这一下走的是进主窗口那条已有的路，向导没有自己记一遍。
    let 工作目录 = temp_dir("gui-program-认领后记住");
    let 盘 = 摆一块盘("gui-program-认领后记住-盘");
    let (_记忆, 记的) = 记在临时处("gui-program-认领后记住-记忆");
    let mut program = 开场(工作目录.path(), 记的.clone());
    let ctx = headless::context();

    走一趟向导(&ctx, &mut program, "我的主库", 盘.path(), "主库");
    等台上那一趟收场(&ctx, &mut program);

    // 记的是**那份中立库的完整路径**，工作目录由它反推。
    let 库文件 = workspace::catalog_path(工作目录.path(), Slug::Named("我的主库"));
    assert_eq!(
        记的.read().as_deref(),
        Some(库文件.as_path()),
        "刚认领出来的那一份没被记住",
    );

    // 下一趟启动：一个参数都不给（双击图标那一下），直接进它。
    let mut 下一趟 = Program::start_with(&Locate::default(), 记的).expect("走得完启动");
    assert!(
        下一趟.window_title().contains("我的主库"),
        "没直接进刚认领的那一份：{}",
        下一趟.window_title(),
    );
    let 屏上 = 跑一帧(&headless::context(), &mut 下一趟);
    assert!(屏上.contains("待确认队列"), "没直接进主窗口：\n{屏上}");
}

#[test]
fn 向导里根名不能用时说的也是加根那一处的原话() {
    // 验收第 4 条的另一半：那几种情况**不是各拦各的**，它们是同一个函数
    // （`roots::add_root_from_fields` → `romcat_core::catalog::roots::add_root`）的几支。
    // 这一条与「圈进工作目录」那一条各撞一支，撞出来的话都是核心库那一处写的
    // ——两处各写一套措辞，人在库屏上与在向导里看见的迟早分家（ADR-0005）。
    let 工作目录 = temp_dir("gui-program-认领根名不能用");
    let 盘 = 摆一块盘("gui-program-认领根名不能用-盘");
    let 原样 = 底下有什么(工作目录.path());
    let (_记忆, 记的) = 记在临时处("gui-program-认领根名不能用-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();

    // `/` 是**变体的键**里那个分隔符：根名里再有一个，那条键就切不开了。
    let 屏上 = 走一趟向导(&ctx, &mut program, "我的主库", 盘.path(), "甲/乙");

    assert!(
        屏上.contains("这个根名不能用"),
        "根名带分隔符却放它过去了：\n{屏上}",
    );
    assert!(屏上.contains("键的分隔符"), "没说清为什么不能用：\n{屏上}");
    assert_eq!(program.window_title(), "romcat — 开场", "被拦下了却换了屏");
    assert_eq!(
        底下有什么(工作目录.path()),
        原样,
        "被拦下了，工作目录里却多出了东西",
    );
}

#[test]
fn 开场与添加主库向导上画出来的字里没有星号也没有文档编号() {
    // 屏上的每一句都得是维护者用得上的话（票 `gui-looks-like-the-design/03`）：
    // egui 不认 markdown，`**` 原样印出来；ADR 编号是写给开发者的出处。
    let 工作目录 = temp_dir("gui-program-文案");
    let (_记忆, 记的) = 记在临时处("gui-program-文案-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();

    let mut 屏上 = 跑一帧(&ctx, &mut program);
    屏上.push_str(&点一下(&ctx, &mut program, "添加主库"));
    打字(&ctx, &mut program, "主库原名，例如", "我的主库");
    let 第二步 = 点一下(&ctx, &mut program, "下一步");
    assert!(第二步.contains("选第一个根"), "没走到第二步：\n{第二步}");
    屏上.push_str(&第二步);
    for 不该有 in ["**", "ADR-"] {
        assert!(
            !屏上.contains(不该有),
            "开场或向导上画出了「{不该有}」：\n{屏上}",
        );
    }
}

// ——— 照设计稿排的开场与向导（票 `gui-looks-like-the-design/05`）———
//
// 断的仍是这一帧**画出来的字**与它们**画在哪儿**；间距、颜色、圆角归截图门（`tests/snapshot.rs`）。

#[test]
fn 开场左边讲清这个工具做什么外加三条承诺_右边是工作目录() {
    // 设计稿开场那一屏：左边一句话讲清工具做什么、三条承诺；右边是工作目录与主库那一栏。
    let 工作目录 = temp_dir("gui-program-开场三条承诺");
    let (_记忆, 记的) = 记在临时处("gui-program-开场三条承诺-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();
    跑一帧(&ctx, &mut program);
    let 产出 = 跑一帧的产出(&ctx, &mut program);
    let 屏上 = shared::画出来的字(&产出);

    for 该有 in [
        "管理你的模拟器游戏库",
        "只读访问 ROM",
        "扫描、识别和导出都不会修改、移动或重命名任何 ROM 文件",
        "默认离线运行",
        "识别和刮削优先使用本地数据源，不产生网络请求",
        "不确定的结果由你确认",
        "无法自动确定的进入待确认队列",
    ] {
        assert!(屏上.contains(该有), "开场上没有「{该有}」：\n{屏上}");
    }
    let (Some(讲清的), Some(工作目录那一栏)) = (
        正好画着的(&产出, "管理你的模拟器游戏库").first().copied(),
        正好画着的(&产出, "工作目录").first().copied(),
    ) else {
        panic!("开场上找不着那句大标题或工作目录那一栏：\n{屏上}");
    };
    assert!(
        讲清的.x < 工作目录那一栏.x,
        "大标题该在左、工作目录那一栏该在右：{讲清的:?} 对 {工作目录那一栏:?}",
    );
    assert!(
        工作目录那一栏.x > headless::VIEWPORT[0] / 2.0,
        "工作目录那一栏不在右半边：{工作目录那一栏:?}",
    );
}

#[test]
fn 空工作目录上右边只有一个主要操作_添加主库() {
    let 工作目录 = temp_dir("gui-program-开场空的只有一个主要操作");
    let (_记忆, 记的) = 记在临时处("gui-program-开场空的只有一个主要操作-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();
    跑一帧(&ctx, &mut program);
    let 产出 = 跑一帧的产出(&ctx, &mut program);
    let 屏上 = shared::画出来的字(&产出);

    let 那几颗 = 正好画着的(&产出, "添加主库");
    assert_eq!(
        那几颗.len(),
        1,
        "空工作目录上摆着的「添加主库」不是正好一颗：\n{屏上}",
    );
    let Some(那一句) = 正好画着的(&产出, "还没有库，添加一个主库开始")
        .first()
        .copied()
    else {
        panic!("空工作目录上没有那句空态：\n{屏上}");
    };
    assert!(
        那一句.x > headless::VIEWPORT[0] / 2.0,
        "那句空态不在右边那一栏里：{那一句:?}",
    );
    assert!(
        那几颗[0].y > 那一句.y,
        "「添加主库」该摆在那句空态底下、跟着它说的下一步：{:?} 对 {那一句:?}",
        那几颗[0],
    );
    assert!(
        屏上.contains("把工作目录指向那份数据所在的位置"),
        "没说命令行扫过的库怎么在这儿找回来：\n{屏上}",
    );
}

#[test]
fn 有库时右边抬头写着几份_版本对不上的那一份标着版本不兼容() {
    let 工作目录 = temp_dir("gui-program-开场有库");
    建一份像样的库(工作目录.path(), "开得了的库", 2, 1_700_000_000);
    建一份像样的库(工作目录.path(), "另一份开得了的库", 1, 1_700_000_100);
    let 旧的 = workspace::catalog_path(工作目录.path(), Slug::Named("版本对不上的库"));
    testing::catalog_at_version(&旧的, 4);
    let (_记忆, 记的) = 记在临时处("gui-program-开场有库-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();
    跑一帧(&ctx, &mut program);
    let 产出 = 跑一帧的产出(&ctx, &mut program);
    let 屏上 = shared::画出来的字(&产出);

    assert!(
        屏上.contains("主库 · 3 个"),
        "抬头没写这个工作目录里有几份：\n{屏上}"
    );
    assert!(
        屏上.contains("版本不兼容"),
        "版本对不上的那一份没单独标出来：\n{屏上}",
    );
    let Some(那一行) = 正好画着的(&产出, "开得了的库").first().copied() else {
        panic!("那一份库的名字没画出来：\n{屏上}");
    };
    assert!(
        那一行.x > headless::VIEWPORT[0] / 2.0,
        "库那几行不在右边那一栏里：{那一行:?}",
    );
    assert_eq!(
        正好画着的(&产出, "添加主库").len(),
        1,
        "有库时「添加主库」只在抬头那一行上摆一颗：\n{屏上}",
    );
}

#[test]
fn 工作目录读不动时右边不摆添加主库() {
    // 核心库那句原话说的是「先去看它的权限，别急着再建一份」；这时候再摆一颗「添加主库」，按下去
    // 起名那一问就会撞上同一个列不开的目录（列不开就查不了重名，挂单 `Q612`）。
    let 工作目录 = temp_dir("gui-program-开场读不动不摆添加");
    let 库文件 = 建一份像样的库(工作目录.path(), "列不开也在的库", 1, 1_700_000_000);
    let 目录 = 库文件.parent().expect("有那个目录").to_path_buf();
    let Some(_还回去) = testing::revoke(&目录, testing::Revoke::Read) else {
        return;
    };
    let (_记忆, 记的) = 记在临时处("gui-program-开场读不动不摆添加-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();
    跑一帧(&ctx, &mut program);
    let 产出 = 跑一帧的产出(&ctx, &mut program);
    let 屏上 = shared::画出来的字(&产出);

    assert!(屏上.contains("读不动"), "前提：屏上说了读不动：\n{屏上}");
    assert!(
        正好画着的(&产出, "添加主库").is_empty(),
        "工作目录读不动，屏上却摆着「添加主库」：\n{屏上}",
    );
}

#[test]
fn 走向导时开场那一屏照常画在遮罩底下_底下那一行点不动() {
    // 挂单 `Q667`：从前走向导时那张表与换目录那一行收起来，弹层底下只剩抬头。弹层的遮罩本来就挡着
    // 点击，收起来那两条理由（在认领新的与开那一份之间犹豫、走到一半换了目录）它已经替着挡了。
    let 工作目录 = temp_dir("gui-program-向导盖在开场上");
    建一份像样的库(工作目录.path(), "底下那一份库", 3, 1_700_000_000);
    let (_记忆, 记的) = 记在临时处("gui-program-向导盖在开场上-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();

    点一下(&ctx, &mut program, "添加主库");
    let 屏上 = 跑一帧(&ctx, &mut program);
    assert!(
        屏上.contains("给这个主库起个名字"),
        "前提：向导开着：\n{屏上}"
    );
    for 该有 in ["底下那一份库", "3 个变体", "换一个工作目录：把路径贴在这儿"]
    {
        assert!(
            屏上.contains(该有),
            "走向导时开场那一屏收起了「{该有}」：\n{屏上}",
        );
    }

    let 屏上 = 点一下(&ctx, &mut program, "打开");
    assert_eq!(
        program.window_title(),
        "romcat — 开场",
        "遮罩底下那一行点得动：向导开着就开进了一份库",
    );
    assert!(
        屏上.contains("给这个主库起个名字"),
        "点了遮罩底下那一行，向导没了：\n{屏上}",
    );
}

#[test]
fn 向导起名当场就说收不收_不必先按下一步() {
    // 设计稿「名称当场校验」：打完字就说，不等按「下一步」。问的仍是核心库建库入口那一处
    // （`Catalog::refuse_create`），**问一遍一个字节都不写**。
    let 工作目录 = temp_dir("gui-program-起名当场校验");
    建一份像样的库(工作目录.path(), "已经在用的名字", 1, 1_700_000_000);
    // 从开场把这个目录列过一遍之后记起：比的是「起名那一问问过几遍」多没多东西，不算开场列库那一下。
    let 原样 = {
        let (_记忆, 记的) = 记在临时处("gui-program-起名当场校验-记忆列库");
        let mut program = 开场(工作目录.path(), 记的);
        跑一帧(&headless::context(), &mut program);
        底下有什么(工作目录.path())
    };

    // 一、撞上这个工作目录里已经在用的名字。
    {
        let (_记忆, 记的) = 记在临时处("gui-program-起名当场校验-记忆甲");
        let mut program = 开场(工作目录.path(), 记的);
        let ctx = headless::context();
        点一下(&ctx, &mut program, "添加主库");
        打字(&ctx, &mut program, "主库原名，例如", "已经在用的名字");
        let 屏上 = 跑一帧(&ctx, &mut program);
        let 那份 = workspace::catalog_path(工作目录.path(), Slug::Named("已经在用的名字"));
        assert!(
            屏上.contains("已经在了") && 屏上.contains(&romcat_core::path::display(&那份)),
            "打完一个撞了的名字，没按下一步就不说：\n{屏上}",
        );
        assert!(
            屏上.contains("给这个主库起个名字") && !屏上.contains("选第一个根"),
            "没按下一步就离开了起名那一问：\n{屏上}",
        );
    }

    // 二、收得下的名字：当场说收得下，并写清那份库会建在哪儿。
    {
        let (_记忆, 记的) = 记在临时处("gui-program-起名当场校验-记忆乙");
        let mut program = 开场(工作目录.path(), 记的);
        let ctx = headless::context();
        点一下(&ctx, &mut program, "添加主库");
        打字(&ctx, &mut program, "主库原名，例如", "新起的名字");
        let 屏上 = 跑一帧(&ctx, &mut program);
        assert!(
            屏上.contains("名称可用"),
            "打完一个收得下的名字没当场说：\n{屏上}"
        );
        let 那份 = workspace::catalog_path(工作目录.path(), Slug::Named("新起的名字"));
        assert!(
            屏上.contains(&romcat_core::path::display(&那份)),
            "没写清这份库会建在哪儿：\n{屏上}",
        );
        assert!(
            屏上.contains("给这个主库起个名字") && !屏上.contains("选第一个根"),
            "没按下一步就离开了起名那一问：\n{屏上}",
        );
    }

    assert_eq!(
        底下有什么(工作目录.path()),
        原样,
        "起名那一问当场问过几遍，工作目录里却多出了东西",
    );
}

#[test]
fn 向导选根那一问当场说这个目录收不收_弹不出选择窗口时的退路常驻在框底下() {
    let 工作目录 = temp_dir("gui-program-选根当场校验");
    let 手滑 = 工作目录.path().join("roms");
    std::fs::create_dir_all(&手滑).expect("建得出目录");
    let 原样 = 底下有什么(工作目录.path());
    let (_记忆, 记的) = 记在临时处("gui-program-选根当场校验-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();

    点一下(&ctx, &mut program, "添加主库");
    打字(&ctx, &mut program, "主库原名，例如", "我的主库");
    let 屏上 = 点一下(&ctx, &mut program, "下一步");
    // 挂单 `Q695`：退路不靠悬停——指针不在「选择…」上，那句话也在框底下。
    assert!(
        屏上.contains("弹不出选择窗口时（例如远程会话）"),
        "选根那一问上，弹不出选择窗口时的退路不在屏上：\n{屏上}",
    );

    打字(
        &ctx,
        &mut program,
        "那块盘上的目录",
        &romcat_core::path::display(&手滑),
    );
    let 屏上 = 跑一帧(&ctx, &mut program);
    // 说的是加根那一处的原话（`AddRootError::Workspace`），与按下去之后拦下时说的是同一句。
    assert!(
        屏上.contains("主库只读") && 屏上.contains("与工作目录"),
        "填了一个圈进工作目录的目录，没按下一步就不说：\n{屏上}",
    );
    assert!(
        屏上.contains("选第一个根") && 屏上.contains("下一步"),
        "没按下一步就离开了选根那一问：\n{屏上}",
    );
    assert_eq!(
        底下有什么(工作目录.path()),
        原样,
        "选根那一问当场问过一遍，工作目录里却多出了东西",
    );
}

#[test]
fn 向导第三问写清扫描读哪儿写哪儿_走到这一问工作目录里还一个文件都没有() {
    // 设计稿第三步：主库、根、读取（只读）、写入。**写哪儿**说的是核心库折出来的那个文件
    // （`workspace::catalog_path`），与按下「开始扫描」那一下建出来的是同一个。
    let 工作目录 = temp_dir("gui-program-第三问读哪儿写哪儿");
    let 盘 = 摆一块盘("gui-program-第三问读哪儿写哪儿-盘");
    let (_记忆, 记的) = 记在临时处("gui-program-第三问读哪儿写哪儿-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();

    let 屏上 = 走到第三问(&ctx, &mut program, "我的主库", 盘.path(), "甲盘");
    assert!(!屏上.contains("下一步"), "没走到第三问：\n{屏上}");
    let 产出 = 跑一帧的产出(&ctx, &mut program);
    for 那一栏 in ["读取", "写入"] {
        assert!(
            !正好画着的(&产出, 那一栏).is_empty(),
            "第三问上没有「{那一栏}」那一栏：\n{屏上}",
        );
    }
    let 库文件 = workspace::catalog_path(工作目录.path(), Slug::Named("我的主库"));
    for 该有 in [
        romcat_core::path::display(盘.path()),
        romcat_core::path::display(&库文件),
        "甲盘".to_string(),
    ] {
        assert!(屏上.contains(&该有), "第三问上没写「{该有}」：\n{屏上}");
    }
    assert!(
        底下有什么(工作目录.path()).is_empty(),
        "还没按「开始扫描」，工作目录里却多出了 {:?}",
        底下有什么(工作目录.path()),
    );

    点一下(&ctx, &mut program, "算了");
    assert!(
        底下有什么(工作目录.path()).is_empty(),
        "在第三问上按了「算了」，工作目录里却多出了 {:?}",
        底下有什么(工作目录.path()),
    );
}

#[test]
fn 向导标头上排着三问_命名_选择目录_开始扫描() {
    let 工作目录 = temp_dir("gui-program-向导标头三问");
    let (_记忆, 记的) = 记在临时处("gui-program-向导标头三问-记忆");
    let mut program = 开场(工作目录.path(), 记的);
    let ctx = headless::context();

    点一下(&ctx, &mut program, "添加主库");
    let 产出 = 跑一帧的产出(&ctx, &mut program);
    let 屏上 = shared::画出来的字(&产出);
    for 那一问 in ["命名", "选择目录", "开始扫描"] {
        assert!(
            !正好画着的(&产出, 那一问).is_empty(),
            "向导标头上没有「{那一问}」：\n{屏上}",
        );
    }
    assert!(屏上.contains("共三步"), "向导标头没说一共几步：\n{屏上}");
}
