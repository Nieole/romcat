//! **待确认队列**是主界面：列得出、盖得住一批、裁得下去，而中文输入不在表格单元格里。
//!
//! 合成数据的**形状照真机来**（票 08 实测：队列 16,656 条，
//! `--under 合成库/gba/【全部汉化】` 852、`--under 合成库/GoodNES3.1` 1,543、`--name 汉化` 1,986）。
//! 前缀带着**根名**：键的第一段就是它（`path::library_key`）。
//! 这几个数在这里是**断言**而不是注释——ADR-0002 说批量裁决的覆盖面是这件事成不成立的
//! 分界，界面上点一行选中多少，就该是报告上印的那个数。

use egui::widgets::text_edit::TextEditState;
use romcat_core::catalog::State;
use romcat_core::scrape::AnchorKind;
use romcat_core::scrape::zh::{judge, matched_groups};
use romcat_core::triage::{self, Axis, Draft, Filter, Overrides, Scope, Shape};
use romcat_core::verdict::{self, Anchor, MatchVerdict};
use romcat_gui::app::{App, View};
use romcat_gui::queue::Mode;
use romcat_gui::table::ROW_HEIGHT;
use romcat_gui::{demo, headless};

mod shared;
use shared::画出来的字;

/// 这几条测试自己的**工作目录**。
///
/// **不共用 `demo::workspace()`**：那是 `--demo` 那个演示窗口用的目录，而窗口会往里写
/// **版式偏好**（面板拖到哪儿）。共用的话，维护者开一次演示窗口把某块面板拖高一截，
/// 下一次跑这几条测试屏上就少了几行——而断言数的正好是行数。
fn 工作目录() -> std::path::PathBuf {
    std::env::temp_dir().join("romcat-测试-队列")
}

fn 界面(rows: u64) -> App {
    let site = demo::site(demo::queue(rows).expect("造得出合成数据")).expect("开得出现场");
    App::new(site, 工作目录())
}

/// 跑几帧，返回这个上下文。
fn 跑(ctx: &egui::Context, app: &mut App, frames: u32) {
    for _ in 0..frames {
        headless::frame(ctx, headless::input(), |ui| app.ui(ui));
    }
}

/// 展开这一批（已经展开着就不动）。
///
/// 界面上点卡片那一下是**开关**（再点一次收起），而列完队列头一批本来就是展开的
/// ——测试要的是「让这一批开着」，所以不能无脑再点一下。
fn 展开(app: &mut App, shape: &romcat_core::triage::Shape) {
    if app.queue().scope().map(|scope| scope.shape).as_ref() != Some(shape) {
        app.queue_and_site().0.open_batch(shape);
    }
}

/// 造一份带着**那一次中文离线源匹配**的界面，连它落在谁身上（票 `queue-followups/06`）。
fn 带中文匹配的界面(rows: u64) -> (App, demo::ZhMatch) {
    let (catalog, zh) = demo::queue_with_zh(rows).expect("造得出合成数据");
    let zh = zh.expect("合成数据里该摆着那一次中文离线源匹配");
    let site = demo::site(catalog).expect("开得出现场");
    (App::new(site, 工作目录()), zh)
}

/// 「逐条看整个队列」那一下，再把**光标**停在这一条上。
///
/// 前两下就是屏上那颗按钮做的事（`batches_ui` 里那句「收起展开的那一批、切到逐条」）：
/// 不收起来的话逐条只看得见展开的那一批，而这几条要停的那个变体未必在里头。
fn 停在(ctx: &egui::Context, app: &mut App, key: &str) {
    {
        let (screen, _) = app.queue_and_site();
        if let Some(scope) = screen.scope() {
            screen.open_batch(&scope.shape);
        }
        screen.show_one_by_one();
        screen.pick_row(key);
    }
    跑(ctx, app, 1);
    assert_eq!(
        app.queue()
            .queue()
            .selected()
            .get(app.queue().at())
            .map(|item| item.variant.key.as_str()),
        Some(key),
        "光标没停在这一条上，底下那几条断言就全在测别人",
    );
}

/// 详情那一栏**滚一趟**，把这一路上画出来的字都收起来。
///
/// 底下那块面板默认 268 点高，而一堆匹配有六个字段、每条各带一整句**依据**——
/// 一屏摆不下是必然的，而 egui 不画视口之外的东西。所以这里滚的是**真的滚轮事件**
/// （`表格里画多少行文本输入框都是那几个` 也是这么滚一趟的），每一帧收的仍旧是
/// 那一帧真的画出来的字。
fn 详情滚一趟(ctx: &egui::Context, app: &mut App) -> String {
    const STEPS: u32 = 12;
    let mut out = String::new();
    for step in 0..=STEPS {
        let mut input = headless::input();
        // 指针停在底下那块面板的左半栏里——滚轮归指针底下那块滚动区。
        input
            .events
            .push(egui::Event::PointerMoved(egui::pos2(300.0, 700.0)));
        if step > 0 {
            input.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -150.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
        }
        out.push_str(&画出来的字(&headless::frame(ctx, input, |ui| app.ui(ui))));
    }
    out
}

/// 按一下这个键，跑一帧。**逐条键盘流走的就是它**——测试敲的是真的键盘事件，
/// 不是绕过界面直接调那个函数。
fn 按(ctx: &egui::Context, app: &mut App, key: egui::Key) {
    let mut input = headless::input();
    input.events.push(egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    headless::frame(ctx, input, |ui| app.ui(ui));
}

#[test]
fn 队列是打开工具后的默认界面() {
    // ADR-0002：GUI 的主界面是这个待确认队列，封面墙是次要视图。
    let app = 界面(demo::QUEUE_ROWS);
    assert_eq!(app.view(), View::Queue);
    assert_eq!(
        app.queue().queue().pending(),
        demo::QUEUE_ROWS,
        "打开就该已经列好了队列，而不是等人再点一次",
    );
    assert!(app.queue().error().is_none(), "{:?}", app.queue().error());
}

#[test]
fn 每条待裁决项都带着结论理由与候选() {
    let app = 界面(demo::QUEUE_ROWS);
    let items = app.queue().queue().selected();
    assert_eq!(items.len() as u64, demo::QUEUE_ROWS);
    // 「为什么没定下来」——无判据那一档必须说得出理由。
    let 无判据: Vec<_> = items
        .iter()
        .filter(|item| item.state == State::NoEvidence)
        .collect();
    assert!(!无判据.is_empty(), "合成数据里该有无判据那一档");
    assert!(
        无判据.iter().all(|item| item.reason.is_some()),
        "无判据却说不出为什么，队列里最该先读的那一行就空着",
    );
    // **候选带着依据**——没有依据的候选事后无法复核（ADR-0002）。
    let 有候选: Vec<_> = items
        .iter()
        .flat_map(|item| item.candidates.iter())
        .collect();
    assert!(!有候选.is_empty(), "合成数据里该有带候选的那一档");
    assert!(
        有候选
            .iter()
            .all(|candidate| !candidate.evidence.is_empty() && !candidate.accepted),
        "候选要么没有依据，要么已经自动通过——自动通过的不该进队列",
    );
    // 真机上 98.2% 的条目一条候选都没有，所以**手工指定**是主路径。
    let 没候选 = items
        .iter()
        .filter(|item| item.candidates.is_empty())
        .count();
    assert!(
        没候选 * 10 > items.len() * 9,
        "队列里没候选的只有 {没候选} 条，与真机的形状对不上",
    );
}

#[test]
fn 三个轴各能一次盖住一批() {
    // 票 08 在真机上实测的那三个数。界面上点一行选中多少，就该是报告上印的那个数。
    let ctx = headless::context();
    let mut app = 界面(demo::QUEUE_ROWS);
    // 分组表上「按候选作品」最大的那一组，先记下来——它是在**整个队列**上数出来的。
    let work = app
        .queue()
        .queue()
        .groups(Axis::CandidateWork)
        .first()
        .cloned()
        .expect("该有候选作品分组");

    let 点一行 = |app: &mut App, axis: Axis, label: &str| {
        let (screen, _) = app.queue_and_site();
        screen.pick(axis, label);
        跑(&ctx, app, 1);
        app.queue().queue().selected().len()
    };
    assert_eq!(
        点一行(&mut app, Axis::Directory, "合成库/gba/【全部汉化】"),
        852,
        "--under 合成库/gba/【全部汉化】",
    );
    assert_eq!(
        点一行(&mut app, Axis::Directory, "合成库/GoodNES3.1"),
        1_543,
        "--under 合成库/GoodNES3.1",
    );
    assert_eq!(
        点一行(&mut app, Axis::NameMark, "汉化"),
        1_986,
        "--name 汉化"
    );
    assert_eq!(
        点一行(&mut app, Axis::NameMark, "ACG汉化组"),
        129,
        "--name ACG汉化组",
    );
    let 选中 = 点一行(&mut app, Axis::CandidateWork, &work.label);
    assert_eq!(
        选中 as u64, work.count,
        "分组表上写着 {} 条，照着抄成选择器却选中 {选中} 条",
        work.count,
    );
}

#[test]
fn 表格里画多少行文本输入框都是那几个() {
    // ADR-0005 的修订段：中文输入放详情面板，不放表格单元格——表格是虚拟化的，
    // 正在组字的那一行滚出视口时控件就没了，输入法上屏时没人接。
    //
    // 这条断言不看代码长什么样，看的是**跑出来的结果**：egui 每画一个 `TextEdit` 就在
    // `ctx.data()` 里留下一份 `TextEditState`，于是「表格里有没有文本框」等价于
    // 「画的行数变了，这个数变不变」。
    let 数一遍 = |rows: u64| {
        let ctx = headless::context();
        let mut app = 界面(rows);
        // **表格在逐条那一屏上**：批优先是默认的（票 `gui-redesign/09`），
        // 而这条断言测的是那张虚拟化的表。
        app.queue_and_site().0.show_one_by_one();
        跑(&ctx, &mut app, 3);
        // 滚一整趟：虚拟化的表格会把不同的行画出来，若单元格里有文本框，这个数会涨。
        const STEPS: u32 = 24;
        #[allow(clippy::cast_precision_loss)]
        let travel = app.queue().queue().selected().len() as f32 * ROW_HEIGHT;
        for step in 0..=STEPS {
            let (screen, _) = app.queue_and_site();
            screen.scroll_to = Some(travel * step as f32 / STEPS as f32);
            headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
        }
        ctx.data(|data| data.count::<TextEditState>())
    };
    let 少 = 数一遍(200);
    let 多 = 数一遍(demo::QUEUE_ROWS);
    assert!(少 > 0, "一个文本框都没画出来的话这条断言等于没测");
    assert_eq!(
        少,
        多,
        "队列从 200 条涨到 {} 条、还滚了一整趟，文本输入框却从 {少} 个变成 {多} 个\
         ——那说明有文本框长在表格单元格里",
        demo::QUEUE_ROWS,
    );
}

#[test]
fn 裁决即时写进沉淀库并从队列移除() {
    let mut app = 界面(demo::QUEUE_ROWS);
    let 原有 = app.queue().queue().pending();

    // 点**按命名规律**里的一个汉化组记号——ADR-0002 点名的那种批量。
    let (screen, _) = app.queue_and_site();
    screen.pick(Axis::NameMark, "ACG汉化组");
    let ctx = headless::context();
    跑(&ctx, &mut app, 1);
    let 这一批 = app.queue().queue().selected().len();
    assert_eq!(这一批, 129, "真机上 [ACG汉化组] 带 129 条");

    // 手工指定作品，顺手补上**汉化组**——自动识别只做到发行版级（ADR-0008）。
    let draft = Draft {
        work: Some("勇者斗恶龙".to_string()),
        overrides: Overrides {
            team: Some("ACG汉化组".to_string()),
            version: Some("v1.2".to_string()),
            ..Overrides::default()
        },
        note: Some("按记号一次裁一批".to_string()),
        ..Draft::default()
    };
    let (screen, site) = app.queue_and_site();
    screen.preview(site, &draft);
    let plan = screen.pending().expect("排得出计划");
    assert_eq!(plan.decided.len(), 这一批, "选中几条就该裁几条");
    assert!(plan.blocked.is_empty(), "{:?}", plan.blocked);

    // **差量预览**画得出来，而且没人点按钮时那份计划留着——一条命令改一百多条记录，
    // 看不见它要改什么就按下去，错了没处找（与同步那一侧同源，ADR-0016）。
    跑(&ctx, &mut app, 2);
    assert!(
        app.queue().pending().is_some(),
        "预览画了两帧计划就没了，那等于没看就落了下去",
    );

    let (screen, site) = app.queue_and_site();
    screen.commit(site);
    assert!(screen.error().is_none(), "{:?}", screen.error());
    let applied = *screen.applied().expect("落下了就该有账");
    assert_eq!(applied.verdicts, 这一批 as u64);
    assert_eq!(applied.matched, 这一批 as u64, "中立库该当场兑现成命中");

    // 沉淀库里真的有这些条，而且**从队列里消失了**。
    let counts = app.site().store.counts().expect("读得出沉淀库");
    assert_eq!(counts.total, 这一批 as u64);
    assert_eq!(counts.with_team, 这一批 as u64, "汉化组要真的落库");
    assert_eq!(
        app.queue().queue().pending(),
        原有 - 这一批 as u64,
        "裁完了还留在队列里的话，人会被同一条问第二遍",
    );
    assert!(
        app.queue()
            .queue()
            .selected()
            .iter()
            .all(|item| !item.name().contains("ACG汉化组")),
        "刚裁完的那一批还在选中的队列里",
    );
}

#[test]
fn 撤回这一批之后那些变体当场回到队列里() {
    // 票 gui-redesign/08：**批量的胆量来自撤销可信**。界面这一层验的是那一下按得着，
    // 而且走的是与命令行同一条路（`Queue::undo`，ADR-0005）——领域判断一条都不在界面里。
    let mut app = 界面(demo::QUEUE_ROWS);
    let 原有 = app.queue().queue().pending();
    let (screen, _) = app.queue_and_site();
    screen.pick(Axis::NameMark, "ACG汉化组");
    let ctx = headless::context();
    跑(&ctx, &mut app, 1);
    let 这一批 = app.queue().queue().selected().len();
    assert_eq!(这一批, 129);

    let draft = Draft {
        work: Some("勇者斗恶龙".to_string()),
        ..Draft::default()
    };
    let (screen, site) = app.queue_and_site();
    screen.preview(site, &draft);
    screen.commit(site);
    let batch = screen.applied().expect("落下了就该有账").batch;
    assert!(batch > 0);
    assert_eq!(
        app.queue().queue().pending(),
        原有 - 这一批 as u64,
        "裁完了该从队列里消失",
    );

    let (screen, site) = app.queue_and_site();
    screen.undo_last(site);
    assert!(screen.error().is_none(), "{:?}", screen.error());
    let undone = *app.queue().undone().expect("撤回了就该有账");
    assert_eq!((undone.batch, undone.removed, undone.kept), (batch, 129, 0));
    assert!(
        undone.catalog_rolled_back,
        "中立库那一半没回去的话，人得先去跑一趟识别才看得见——那正是这一票要消掉的",
    );

    // **不重新列、不重跑识别**：撤回那一下自己已经把队列整份重列过了。
    assert_eq!(
        app.queue().queue().pending(),
        原有,
        "撤回之后队列该回到裁决之前那么多条",
    );
    assert_eq!(app.site().store.counts().expect("读得出").total, 0);
    assert!(
        app.queue().applied().is_none(),
        "撤回之后还挂着「已落下」那一行的话，那个按钮会把同一批再撤一次",
    );

    // **撤销本身也撤得回来**：按错了撤回、又发现撤错了，不该逼人把这一批重打一遍。
    let (screen, site) = app.queue_and_site();
    screen.redo_last(site);
    assert!(screen.error().is_none(), "{:?}", screen.error());
    assert_eq!(
        app.queue().applied().expect("放回去了就该有账").verdicts,
        129,
    );
    assert_eq!(app.queue().queue().pending(), 原有 - 这一批 as u64);
    assert!(app.queue().undone().is_none());
}

#[test]
fn 只裁选中的这一条也做得到() {
    // 批量是这件事成不成立的分界（ADR-0002），但「采用第 N 条候选」天生是逐条的动作：
    // 同一批里各人的候选不是同一部游戏。两种粒度都得有。
    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 1);
    let 整批 = app.queue().queue().selected().len();
    assert!(整批 > 1);

    // 点表里第一行——`pick_row` 走的就是界面上点那一下之后剩下的那半段。
    let key = app.queue().queue().selected()[0].variant.key.clone();
    {
        let (screen, _) = app.queue_and_site();
        screen.pick_row(&key);
        screen.set_only_picked(true);
    }
    跑(&ctx, &mut app, 1);
    assert_eq!(
        app.queue().queue().selected().len(),
        1,
        "勾了「只裁选中的这一条」，选中的却不止一条",
    );

    let draft = Draft {
        work: Some("单条指定的作品".to_string()),
        ..Draft::default()
    };
    let (screen, site) = app.queue_and_site();
    screen.preview(site, &draft);
    assert_eq!(screen.pending().expect("排得出计划").decided.len(), 1);
    screen.commit(site);
    assert!(screen.error().is_none(), "{:?}", screen.error());
    assert_eq!(app.site().store.counts().expect("读得出").total, 1);
    assert_eq!(
        app.queue().queue().pending(),
        整批 as u64 - 1,
        "只该少掉那一条",
    );
}

#[test]
fn 都不对可以手工指定也可以说它没有发行版() {
    let mut app = 界面(2_000);
    let (screen, _) = app.queue_and_site();
    screen.pick(Axis::Directory, "合成库/GoodNES3.1");
    let ctx = headless::context();
    跑(&ctx, &mut app, 1);
    let 这一批 = app.queue().queue().selected().len();
    assert!(这一批 > 0);

    // 「都不对」的第一种明说：它**没有发行版**（同人移植、homebrew）。
    let draft = Draft {
        no_release: true,
        work: Some("某同人移植".to_string()),
        ..Draft::default()
    };
    let (screen, site) = app.queue_and_site();
    screen.preview(site, &draft);
    screen.commit(site);
    assert!(screen.error().is_none(), "{:?}", screen.error());
    let applied = *screen.applied().expect("落下了就该有账");
    assert_eq!(applied.skipped, 这一批 as u64, "确认没有发行版该转成跳过");

    let counts = app.site().store.counts().expect("读得出沉淀库");
    assert_eq!(counts.no_release, 这一批 as u64);
}

#[test]
fn 说它不成其为一次发行就当场说不成立() {
    // 「没有发行版」与「认不出」说的是「它不成其为一次发行」，那就没有汉化组可记。
    // 收下再默默扔掉是最坏的一种「实现了」——所以按钮该是灰的，话该印在旁边。
    let draft = Draft {
        unknown: true,
        overrides: Overrides {
            team: Some("外星科技".to_string()),
            ..Overrides::default()
        },
        ..Draft::default()
    };
    let complaint = draft.check().expect_err("该被挡下");
    assert!(complaint.contains("不成其为一次发行"), "{complaint}");
}

#[test]
fn 打开看见的是分好的批而不是一万八千行的表() {
    // 票 `gui-redesign/09` 的正题。18,241 条按 5 秒一条是 25 小时——那张表根本没法用，
    // 所以打开这一屏看见的必须是**工具已经分好的几十批**。
    let app = 界面(demo::QUEUE_ROWS);
    assert_eq!(app.queue().mode(), Mode::Batches, "打开该是批优先");
    let queue = app.queue().queue();
    let batches = queue.batches();
    assert!(batches.len() > 3, "只分出 {} 批，那不叫分批", batches.len());
    assert_eq!(
        batches.iter().map(|batch| batch.count).sum::<u64>(),
        queue.selected().len() as u64,
        "各批条数加起来不等于队列的条数，屏上那个百分比就是编的",
    );
    // 每张卡片上**条数与那句共同依据**都得有——三样里的前两样。
    for batch in batches {
        assert!(batch.count > 0);
        assert!(!batch.why().trim().is_empty(), "{:?} 说不出共同依据", batch.shape);
    }
    // 单候选那几批是**按批答得了**的；一条候选都没有的与多候选的都不给「整批通过」。
    assert!(
        batches.iter().any(romcat_core::triage::Batch::passable),
        "一批按批答得了的都没有，那这一屏白分了",
    );
    assert!(
        batches.iter().any(|batch| !batch.passable()),
        "合成数据里该有走逐条的那几批，不然兜底那条路测不出来",
    );
    // 四档的账加起来也是整个队列——屏头上那几个数不能少算谁。
    assert_eq!(
        queue.tiers().iter().map(|(_, count)| *count).sum::<u64>(),
        queue.selected().len() as u64,
    );
}

#[test]
fn 二级下钻的条数加起来等于它所属的一级() {
    let ctx = headless::context();
    let mut app = 界面(demo::QUEUE_ROWS);
    let shape = app.queue().queue().batches()[0].shape.clone();
    let count = app.queue().queue().batches()[0].count;
    展开(&mut app, &shape);
    跑(&ctx, &mut app, 1);
    let scope = app.queue().scope().expect("展开了就该有作用范围");
    assert_eq!(app.queue().queue().count(&scope), count);

    // **按目录那个轴分得干净**：一条只落一个目录，各组之和就是这一批。
    let drilled = app.queue().queue().drill(&scope, Axis::Directory);
    assert!(drilled.adds_up(), "{drilled:?}");
    assert_eq!(
        drilled.rows.iter().map(|row| row.count).sum::<u64>(),
        count,
        "二级各组加起来不等于它所属的一级",
    );

    // 下钻到某一组，作用范围跟着收窄，条数与二级表上写的一模一样。
    let row = drilled.rows.first().cloned().expect("该有一组");
    {
        let (screen, _) = app.queue_and_site();
        screen.drill_into(&row.label);
    }
    跑(&ctx, &mut app, 1);
    let 那一组 = app.queue().scope().expect("下钻了还该有作用范围");
    assert_eq!(app.queue().queue().count(&那一组), row.count, "{}", row.label);
}

#[test]
fn 每批都有随机样本换一组每次不同但都在批内() {
    let ctx = headless::context();
    let mut app = 界面(demo::QUEUE_ROWS);
    let shape = app.queue().queue().batches()[0].shape.clone();
    展开(&mut app, &shape);
    跑(&ctx, &mut app, 1);
    let scope = Scope::whole(shape);
    let 批内: Vec<String> = app
        .queue()
        .queue()
        .members(&scope)
        .iter()
        .map(|item| item.variant.key.clone())
        .collect();
    assert!(批内.len() > 5);

    let mut 上一组 = app.queue().samples();
    assert_eq!(上一组.len(), 5, "屏上常驻三样里的第三样：随机样本");
    for round in 0..8 {
        {
            let (screen, _) = app.queue_and_site();
            screen.resample();
        }
        let 这一组 = app.queue().samples();
        assert_ne!(这一组, 上一组, "第 {round} 次「换一组样本」按下去还是同一组");
        assert!(
            这一组.iter().all(|one| 批内.contains(&one.key)),
            "样本跑到批外面去了",
        );
        上一组 = 这一组;
    }
}

#[test]
fn 一级与二级都能整批通过而且撤得回来() {
    // 验收第 4、5 条：**每一层都能整批过**，过完之后按批整个撤回（走票 08 那条路）。
    let ctx = headless::context();
    let mut app = 界面(demo::QUEUE_ROWS);
    let 原有 = app.queue().queue().pending();
    let batch = app
        .queue()
        .queue()
        .batches()
        .iter()
        .find(|batch| batch.passable())
        .cloned()
        .expect("该有一批是单候选的");
    展开(&mut app, &batch.shape);
    跑(&ctx, &mut app, 1);

    // ——— 一级：整批通过 ———
    let scope = app.queue().scope().expect("展开了就该有作用范围");
    let 这一批 = app.queue().queue().count(&scope);
    assert_eq!(这一批, batch.count);
    {
        let (screen, site) = app.queue_and_site();
        screen.pass(site, &scope);
    }
    // **先出计划再动手**：没人点「落下」时那份计划留在屏上。
    跑(&ctx, &mut app, 2);
    assert_eq!(
        app.queue().pending().expect("排得出计划").decided.len() as u64,
        这一批,
    );
    {
        let (screen, site) = app.queue_and_site();
        screen.commit(site);
    }
    assert!(app.queue().error().is_none(), "{:?}", app.queue().error());
    let applied = *app.queue().applied().expect("落下了就该有账");
    assert_eq!(applied.verdicts, 这一批);
    assert_eq!(app.queue().queue().pending(), 原有 - 这一批);

    // ——— 按批整个撤回 ———
    {
        let (screen, site) = app.queue_and_site();
        screen.undo_last(site);
    }
    let undone = *app.queue().undone().expect("撤回了就该有账");
    assert_eq!((undone.batch, undone.removed), (applied.batch, 这一批));
    assert!(undone.catalog_rolled_back, "中立库那一半没回去");
    assert_eq!(
        app.queue().queue().pending(),
        原有,
        "撤回之后队列该回到整批通过之前那么多条",
    );

    // ——— 二级：下钻之后只过那一组 ———
    // 撤回之后那一批回到屏上，卡片照旧是展开的——**不必再点一次**（`open_batch` 是
    // 开关：再点一次是收起来）。
    跑(&ctx, &mut app, 1);
    let whole = app.queue().scope().expect("撤回之后那一批该还展开着");
    let row = app
        .queue()
        .queue()
        .drill(&whole, Axis::Directory)
        .rows
        .first()
        .cloned()
        .expect("该有一组");
    {
        let (screen, _) = app.queue_and_site();
        screen.drill_into(&row.label);
    }
    跑(&ctx, &mut app, 1);
    let 那一组 = app.queue().scope().expect("下钻了还该有作用范围");
    {
        let (screen, site) = app.queue_and_site();
        screen.pass(site, &那一组);
    }
    assert_eq!(
        app.queue().pending().expect("排得出计划").decided.len() as u64,
        row.count,
        "二级整批通过该只作用于下钻出来的那一组",
    );
}

#[test]
fn 整批拒绝记成认不出并且退出队列() {
    let ctx = headless::context();
    let mut app = 界面(demo::QUEUE_ROWS);
    let 原有 = app.queue().queue().pending();
    let shape = app.queue().queue().batches()[0].shape.clone();
    展开(&mut app, &shape);
    跑(&ctx, &mut app, 1);
    let scope = app.queue().scope().expect("展开了就该有作用范围");
    let 这一批 = app.queue().queue().count(&scope);
    {
        let (screen, site) = app.queue_and_site();
        screen.reject(site, &scope);
        screen.commit(site);
    }
    assert!(app.queue().error().is_none(), "{:?}", app.queue().error());
    let counts = app.site().store.counts().expect("读得出沉淀库");
    assert_eq!(counts.unknown, 这一批, "整批拒绝记的是「我看过了，认不出」");
    assert_eq!(app.queue().queue().pending(), 原有 - 这一批);
    assert_eq!(
        app.queue().queue().count(&scope),
        0,
        "拒绝完了那一批还在队列里，人会被同一批问第二遍",
    );
}

#[test]
fn 二级下钻之后整批拒绝只作用于那一组() {
    // 「每一层都能整批过或整批拒」——拒绝那一半在二级上也得成立。
    let ctx = headless::context();
    let mut app = 界面(demo::QUEUE_ROWS);
    let 原有 = app.queue().queue().pending();
    let batch = app.queue().queue().batches()[0].clone();
    展开(&mut app, &batch.shape);
    跑(&ctx, &mut app, 1);
    let whole = app.queue().scope().expect("展开了就该有作用范围");
    let row = drilled_first(&app.queue().queue().drill(&whole, Axis::Directory));
    assert!(row.count < batch.count, "这一批只有一个目录，二级测不出来");
    {
        let (screen, _) = app.queue_and_site();
        screen.drill_into(&row.label);
    }
    跑(&ctx, &mut app, 1);
    let 那一组 = app.queue().scope().expect("下钻了该有作用范围");
    {
        let (screen, site) = app.queue_and_site();
        screen.reject(site, &那一组);
        assert_eq!(
            screen.pending().expect("排得出计划").decided.len() as u64,
            row.count,
            "二级整批拒绝该只作用于下钻出来的那一组",
        );
        screen.commit(site);
    }
    assert!(app.queue().error().is_none(), "{:?}", app.queue().error());
    assert_eq!(app.site().store.counts().expect("读得出").unknown, row.count);
    assert_eq!(app.queue().queue().pending(), 原有 - row.count);
}

#[test]
fn 逐条键盘流切候选通过拒绝跳过撤销上一条() {
    // 验收第 7、8 条。多候选那些走这条路：`←→` 切候选、`Y` 过、`N` 拒、
    // `空格` 先放着、`U` 撤销上一条。**敲的是真的键盘事件**。
    let ctx = headless::context();
    let mut app = 界面(demo::QUEUE_ROWS);
    // 找一批多候选的，切候选才有得切。
    let 多候选 = app
        .queue()
        .queue()
        .batches()
        .iter()
        .find(|batch| batch.shape.fanout() == romcat_core::triage::Fanout::Several)
        .cloned()
        .expect("合成数据里该有 4–10 个候选那一档");
    展开(&mut app, &多候选.shape);
    {
        let (screen, _) = app.queue_and_site();
        screen.show_one_by_one();
    }
    跑(&ctx, &mut app, 1);
    assert_eq!(app.queue().mode(), Mode::OneByOne);
    let 这一批 = app.queue().queue().selected().len() as u64;
    assert_eq!(这一批, 多候选.count, "逐条看该只看这一批");
    let 原有 = app.queue().queue().pending();

    // ——— `→` 切候选，`←` 切回来 ———
    assert_eq!(app.queue().nth(), 0);
    按(&ctx, &mut app, egui::Key::ArrowRight);
    assert_eq!(app.queue().nth(), 1, "`→` 没切到下一条候选");
    按(&ctx, &mut app, egui::Key::ArrowLeft);
    assert_eq!(app.queue().nth(), 0, "`←` 没切回上一条候选");

    // ——— `空格` 先放着：**一个字都不写库** ———
    let 头一条 = app.queue().queue().selected()[0].variant.key.clone();
    按(&ctx, &mut app, egui::Key::Space);
    assert_eq!(app.queue().at(), 1, "`空格` 没往下走一条");
    assert_eq!(
        app.site().store.counts().expect("读得出").total,
        0,
        "「先放着」写了库——那不是跳过，那是替人裁了一刀",
    );
    assert_eq!(app.queue().queue().pending(), 原有, "先放着不该动队列");
    assert!(
        app.queue().queue().selected()[0].variant.key == 头一条,
        "先放着把那一条从队列里弄丢了",
    );

    // ——— `Y` 通过：采用眼下切到的那条候选，当场落下 ———
    按(&ctx, &mut app, egui::Key::Y);
    assert!(app.queue().error().is_none(), "{:?}", app.queue().error());
    let applied = *app.queue().applied().expect("`Y` 该当场落下一条");
    assert_eq!(applied.verdicts, 1, "逐条一次只该裁一条");
    assert_eq!(app.queue().queue().pending(), 原有 - 1);

    // ——— `U` 撤销上一条：走的是按批撤那条路（票 08） ———
    按(&ctx, &mut app, egui::Key::U);
    assert!(app.queue().error().is_none(), "{:?}", app.queue().error());
    let undone = *app.queue().undone().expect("`U` 该撤得掉");
    assert_eq!((undone.batch, undone.removed), (applied.batch, 1));
    assert!(undone.catalog_rolled_back, "中立库那一半没回去");
    assert_eq!(app.queue().queue().pending(), 原有, "撤销之后该回到原样");

    // ——— `N` 拒绝：记成「认不出」 ———
    按(&ctx, &mut app, egui::Key::N);
    assert!(app.queue().error().is_none(), "{:?}", app.queue().error());
    assert_eq!(app.queue().applied().expect("`N` 该落下一条").verdicts, 1);
    assert_eq!(app.site().store.counts().expect("读得出").unknown, 1);
    assert_eq!(app.queue().queue().pending(), 原有 - 1);
}

#[test]
fn 逐条时屏上真的摆着文件名路径与候选的完整依据() {
    // 验收第 8 条：人按下去之前该看见的全部。**断言看的是这一帧真的画出来的字**
    // ——查队列里有没有这条数据是恒真的废话，这一屏要证的是那几样摆出来了没有。
    let ctx = headless::context();
    let mut app = 界面(demo::QUEUE_ROWS);
    let 有候选的 = app
        .queue()
        .queue()
        .batches()
        .iter()
        .find(|batch| batch.passable())
        .cloned()
        .expect("该有一批是带候选的");
    展开(&mut app, &有候选的.shape);
    {
        let (screen, _) = app.queue_and_site();
        screen.show_one_by_one();
    }
    跑(&ctx, &mut app, 2);
    let item = app
        .queue()
        .queue()
        .selected()
        .get(app.queue().at())
        .cloned()
        .expect("光标底下该有一条");
    assert!(!item.candidates.is_empty(), "这一批该是带候选的");

    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| app.ui(ui)));
    assert!(屏上.contains(item.name()), "屏上没有文件名：{}", item.name());
    assert!(
        屏上.contains(item.directory()),
        "屏上没有路径：{}",
        item.directory(),
    );
    let 依据 = &item.candidates[0].evidence;
    assert!(屏上.contains(依据.as_str()), "屏上没有那条候选的完整依据：{依据}");
    assert!(
        屏上.contains(item.candidates[0].confidence.label()),
        "屏上没标出这条候选的置信度",
    );
}

#[test]
fn 屏上常驻三样条数共同依据与随机样本() {
    // 验收第 3 条。**看的是这一帧真的画出来的字**：列完队列头一批本来就是展开的
    // （设计稿上就是这样），所以打开这一屏三样齐了。
    let ctx = headless::context();
    let mut app = 界面(demo::QUEUE_ROWS);
    let batch = app.queue().queue().batches()[0].clone();
    let 样本 = app.queue().samples();
    assert_eq!(样本.len(), 5, "打开这一屏头一批该是展开的，样本该摆着");

    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| app.ui(ui)));
    assert!(
        屏上.contains(&romcat_core::report::thousands(batch.count)),
        "屏上没有这一批的条数",
    );
    assert!(屏上.contains(&batch.why()), "屏上没有那句共同依据");
    for one in &样本 {
        assert!(屏上.contains(&one.name), "屏上没有这条样本：{}", one.name);
    }
}

#[test]
fn 下钻只收窄整批操作不把别的批从屏上筛掉() {
    // 下钻要是借道选择器（`Filter::under` 那三个文本框），下一帧就会把**整个队列**
    // 收窄——二级表塌成一行、别的批跟着从屏上消失，而人只是想在这一批里看细一点。
    let ctx = headless::context();
    let mut app = 界面(demo::QUEUE_ROWS);
    let batch = app.queue().queue().batches()[0].clone();
    展开(&mut app, &batch.shape);
    跑(&ctx, &mut app, 1);
    let 原有批数 = app.queue().queue().batches().len();
    let 原有条数 = app.queue().queue().selected().len();
    let 整批 = app.queue().queue().drill(&Scope::whole(batch.shape.clone()), Axis::Directory);
    let row = drilled_first(&整批);

    {
        let (screen, _) = app.queue_and_site();
        screen.drill_into(&row.label);
    }
    跑(&ctx, &mut app, 1);
    let 那一组 = app.queue().scope().expect("下钻了该有作用范围");
    assert_eq!(
        app.queue().queue().count(&那一组),
        row.count,
        "整批操作的作用范围该收到下钻那一组上",
    );
    assert_eq!(
        app.queue().queue().batches().len(),
        原有批数,
        "下钻把别的批也从屏上筛掉了",
    );
    assert_eq!(
        app.queue().queue().selected().len(),
        原有条数,
        "下钻把整个队列一起筛了",
    );
    // 二级那张表照旧数整批——不然下钻一次就再也回不去了。
    assert_eq!(
        app.queue()
            .queue()
            .drill(&Scope::whole(batch.shape.clone()), Axis::Directory)
            .rows
            .len(),
        整批.rows.len(),
        "二级那张表跟着塌成一行了",
    );
}

/// 二级表上最大的那一组。
fn drilled_first(drill: &romcat_core::triage::Drill) -> romcat_core::triage::GroupRow {
    drill.rows.first().cloned().expect("该有一组")
}

#[test]
fn 识别与刮削的待确认在同一条队列里() {
    // 验收第 9 条。中文离线源是**刮削那一侧的数据源**，它撞出来的候选与 DAT 的候选
    // 同表、同一条队列——于是它在这一屏上自己占一批，不必去第二个地方。
    let app = 界面(demo::QUEUE_ROWS);
    let batches = app.queue().queue().batches();
    let 有中文源 = batches.iter().any(|batch| {
        matches!(&batch.shape, romcat_core::triage::Shape::Candidates { source, .. }
            if source == "中文离线源")
    });
    let 有dat = batches.iter().any(|batch| {
        matches!(&batch.shape, romcat_core::triage::Shape::Candidates { source, .. }
            if source == "MAME")
    });
    assert!(有中文源 && 有dat, "两侧的待确认该在同一条队列里");
}

#[test]
fn 界面点开的那一批与命令行按同一串字选出来的一条不差() {
    // 验收第 3 条（票 `queue-followups/08`）。屏上点一张卡片、命令行敲一条 `--shape`，
    // 选中的必须是**同一批**（ADR-0005）。两边之间只有那一串字：界面把形状折成它
    // （`Shape::selector`），命令行把它认回一个形状（`Shape::parse`，`TriageFilterArgs`
    // 调的就是这个）——所以这条比的不是「两处各写一份逻辑碰巧一致」，是那一对折算
    // 在**每一批**上都不丢东西。
    let ctx = headless::context();
    let mut app = 界面(demo::QUEUE_ROWS);
    跑(&ctx, &mut app, 1);
    let batches = app.queue().queue().batches().to_vec();
    assert!(batches.len() > 3, "只分出 {} 批，比不出什么", batches.len());
    // 两支都得比到：有候选的那一支五段，一条候选都没有的那一支两到三段，认法不是同一条。
    assert!(
        batches
            .iter()
            .any(|one| matches!(one.shape, Shape::Candidates { .. })),
        "合成数据里该有带候选的批",
    );
    assert!(
        batches
            .iter()
            .any(|one| matches!(one.shape, Shape::Bare { .. })),
        "合成数据里该有一条候选都没有的批",
    );

    let index = verdict::Index::load(&app.site().store, &app.site().library).expect("读得出沉淀库");
    for batch in &batches {
        // 界面这一侧：屏上点开这张卡片，作用范围盖住的那些条。
        展开(&mut app, &batch.shape);
        跑(&ctx, &mut app, 1);
        let scope = app.queue().scope().expect("展开了就该有作用范围");
        assert_eq!(scope.shape, batch.shape);
        let mut 界面这批: Vec<String> = app
            .queue()
            .queue()
            .members(&scope)
            .iter()
            .map(|item| item.variant.key.clone())
            .collect();
        界面这批.sort();
        assert_eq!(界面这批.len() as u64, batch.count);
        // **空对空是恒真的废话**：每一批都得真盖住东西，比的才是真的一批。
        assert!(!界面这批.is_empty(), "这一批一条都没盖住：{:?}", batch.shape);

        // 命令行那一侧：只拿到那一串字，从中立库重折一遍队列。
        let 那串字 = batch.shape.selector();
        let 认回来 = Shape::parse(&那串字).unwrap_or_else(|why| panic!("{那串字}：{why}"));
        let mut 命令行这批: Vec<String> = triage::survey(
            &app.site().catalog,
            &index,
            &Filter {
                shape: vec![认回来],
                ..Filter::default()
            },
        )
        .expect("折得出队列")
        .items
        .into_iter()
        .map(|item| item.variant.key)
        .collect();
        // 比的是**哪些条**，不是它们排第几：两边各按各的次序走一遍队列，先后不是这条要钉的。
        命令行这批.sort();

        assert_eq!(命令行这批, 界面这批, "两边拿到的不是同一批：{那串字}");
    }
}

#[test]
fn 计划书开着时键盘一个字都不接落下的还是屏上那一份() {
    // egui 的 `Modal` 只拦得住指针、**拦不住键盘**（0.36）。从前计划书开着按 `N` 会当场
    // 落下光标那一条，人再点「落下」时那份计划已经过期——沉淀库照着它写下了，中立库那
    // 一半却不投影，同一条变体两边各说各的，直到下一趟识别才收得回来。
    //
    // 敲的是**真的键盘事件**（`egui::Event::Key` 进 `RawInput`），逐条流那几下走的就是它。
    let ctx = headless::context();
    let mut app = 界面(demo::QUEUE_ROWS);
    let 多候选 = app
        .queue()
        .queue()
        .batches()
        .iter()
        .find(|batch| batch.shape.fanout() == romcat_core::triage::Fanout::Several)
        .cloned()
        .expect("合成数据里该有 4–10 个候选那一档");
    展开(&mut app, &多候选.shape);
    {
        let (screen, _) = app.queue_and_site();
        screen.show_one_by_one();
    }
    跑(&ctx, &mut app, 1);
    assert_eq!(app.queue().mode(), Mode::OneByOne);
    let 这一批 = app.queue().queue().selected().len();
    let 光标那条 = app.queue().queue().selected()[0].variant.key.clone();

    // 整批手工指定作品——计划书弹出来。
    {
        let (screen, site) = app.queue_and_site();
        screen.preview(
            site,
            &Draft {
                work: Some("某作".to_string()),
                ..Draft::default()
            },
        );
    }
    跑(&ctx, &mut app, 1);
    assert_eq!(
        app.queue().pending().expect("计划书挂着").decided.len(),
        这一批,
    );

    // **计划书开着，按 `N`**：一个字都不该写进去，计划书自己也不该跟着没了。
    按(&ctx, &mut app, egui::Key::N);
    assert!(
        app.queue().applied().is_none(),
        "计划书开着时按 N 当场落下了一条",
    );
    assert_eq!(
        app.site().store.counts().expect("读得出沉淀库").total,
        0,
        "计划书开着时按 N 写了沉淀库",
    );
    assert!(app.queue().pending().is_some(), "那份计划书自己没了");

    // 再点「落下」：落下的还是屏上那一份，而且**两库对得上**。
    {
        let (screen, site) = app.queue_and_site();
        screen.commit(site);
    }
    assert!(app.queue().error().is_none(), "{:?}", app.queue().error());
    let applied = *app.queue().applied().expect("落下了就该有账");
    assert_eq!(applied.verdicts as usize, 这一批);
    assert_eq!(
        applied.matched as usize,
        这一批,
        "沉淀库落了几条，中立库就该当场兑现几条",
    );
    let counts = app.site().store.counts().expect("读得出沉淀库");
    assert_eq!((counts.unknown, counts.releases as usize), (0, 这一批));
    let (_, reason) = app
        .site()
        .catalog
        .identification_of(&光标那条)
        .expect("读得出")
        .expect("有结论");
    assert_eq!(reason, None, "中立库那条还写着「认不出」，两边各说各的");
    assert!(
        app.site()
            .catalog
            .variant(&光标那条)
            .expect("读得出")
            .expect("变体在")
            .work_id
            .is_some(),
        "沉淀库写下了、中立库没投影",
    );
}

#[test]
fn 换过选择器之后那份计划书作废而不是照旧落下() {
    // 队列一变样，计划书上那几行说的就不再是屏上这一批。**作废而不是照着新的重排**：
    // 重排出来的是另一份承诺，而人点「落下」点的是他看过的那一份（ADR-0016）。
    // 核心库那道门照旧在（`triage::apply` 把过期的整份拒回来，命令行归它管），
    // 这一道管的是**别让人走到那一步**。
    let ctx = headless::context();
    let mut app = 界面(demo::QUEUE_ROWS);
    {
        let (screen, _) = app.queue_and_site();
        screen.pick(Axis::NameMark, "ACG汉化组");
    }
    跑(&ctx, &mut app, 1);
    let 这一批 = app.queue().queue().selected().len();
    {
        let (screen, site) = app.queue_and_site();
        screen.preview(
            site,
            &Draft {
                work: Some("某作".to_string()),
                ..Draft::default()
            },
        );
    }
    assert_eq!(
        app.queue().pending().expect("排得出计划").decided.len(),
        这一批,
    );

    // 换一套选择器——再点一次那一行就回到整个队列。
    {
        let (screen, _) = app.queue_and_site();
        screen.pick(Axis::NameMark, "ACG汉化组");
    }
    跑(&ctx, &mut app, 1);
    assert!(
        app.queue().queue().selected().len() > 这一批,
        "选择器没真的换过，这条断言等于没测",
    );
    assert!(
        app.queue().pending().is_none(),
        "队列换过样子，那份过期的计划书还挂在屏上",
    );
    let 话 = app.queue().error().expect("作废了该说一句").to_string();
    assert!(话.contains("重排一份计划"), "这句话要让人去重排计划：{话}");
    assert_eq!(
        app.site().store.counts().expect("读得出沉淀库").total,
        0,
        "作废掉的那份计划不该写进任何一份库",
    );
}

/// 这几条中文匹配的测试摆多少条队列。
///
/// **不用整份 16,656 条**：这一块要的只是「那一堆摆在屏上、裁得动」，而摆得出那一堆
/// 要的是两个拿得到内容判据的变体——命中那一档在这个规模上有 3 条，够了。
const 中文匹配用的条数: u64 = 200;

#[test]
fn 待确认屏上看得出哪几个字段来自同一次匹配() {
    // 票 `queue-followups/06` 验收第 1 条。**断言看的是这一帧真的画出来的字**：
    // 查库里有没有这几条是恒真的废话，这一屏要证的是那一堆摆出来了没有。
    let ctx = headless::context();
    let (mut app, zh) = 带中文匹配的界面(中文匹配用的条数);
    停在(&ctx, &mut app, &zh.variant);

    // 归堆在核心库（`scrape::zh::matched_groups`），界面一条领域逻辑都没写。
    let groups = matched_groups(&app.site().catalog, &zh.variant).expect("读得出");
    assert_eq!(groups.len(), 1, "只撞了一次，就只有一堆");
    let group = &groups[0];
    assert_eq!(group.entry, zh.entry);
    assert!(group.from_variant, "这个变体自己撞的就是这条");
    assert_eq!(
        group.values.len(),
        6,
        "变体那两条（中文名、别名）加作品那四栏（类型、简介、开发商、发行商）",
    );

    let 屏上 = 详情滚一趟(&ctx, &mut app);
    assert!(
        屏上.contains(&format!("条目 {}", zh.entry)),
        "堆上没写条目号——那是「同一次匹配」唯一的判据",
    );
    // **依据**：没有依据的结论事后无法复核（ADR-0002）。
    let 依据 = &group.values[0].evidence;
    assert!(屏上.contains(依据.as_str()), "堆上没写依据：{依据}");
    // 同一堆里**两层锚点**上的字段都摆着——这一条就是「看得出它们来自同一次匹配」。
    assert!(屏上.contains("幻想傳說"), "变体那一层的别名没摆出来");
    assert!(屏上.contains("角色扮演"), "作品那一层的类型没摆出来");
    for field in ["标题", "类型", "简介", "开发商", "发行商"] {
        assert!(屏上.contains(field), "这一堆里没写着「{field}」这一栏");
    }
    // **就地裁得动**：不必切到终端把变体键拷过去。
    assert!(屏上.contains("就是这条"), "屏上没有下肯定裁决那一下");
    assert!(屏上.contains("不是这条"), "屏上没有下否定裁决那一下");
}

#[test]
fn 就地下一次否定裁决同一次匹配带来的全部字段一并失效() {
    // 验收第 2 条。清库在核心库（`scrape::zh::judge`），界面只把按下的那一下转过去。
    let ctx = headless::context();
    let (mut app, zh) = 带中文匹配的界面(中文匹配用的条数);
    停在(&ctx, &mut app, &zh.variant);
    let 依据 = matched_groups(&app.site().catalog, &zh.variant).expect("读得出")[0].values[0]
        .evidence
        .clone();
    {
        let (screen, site) = app.queue_and_site();
        *screen.match_note_mut() = "抽样核对过，撞的是同名的另一部".to_string();
        screen.judge_match(site, &zh.variant, zh.entry, false);
    }
    assert!(app.queue().error().is_none(), "{:?}", app.queue().error());

    let 账 = app.queue().judged().expect("按下去该交回一本账").clone();
    assert!(账.from_variant, "这个变体自己撞的就是这条");
    assert!(账.cleared >= 6, "同一次匹配带来的六个字段该一条不剩：{账:?}");
    assert!(账.cleared_work > 0, "作品那一层也该跟着清");
    // **裁决落沉淀库、锚在内容锚上**（验收第 5 条）：删掉中立库重扫也不丢。
    assert!(
        matches!(账.anchor, Anchor::Content { .. }) && 账.anchor.is_shareable(),
        "锚不是内容锚：{:?}",
        账.anchor,
    );

    // 库里一条不剩——错的东西不许在库里多躺一秒。
    assert!(
        matched_groups(&app.site().catalog, &zh.variant)
            .expect("读得出")
            .is_empty(),
        "变体那一层还留着这一次匹配的产出",
    );
    assert!(
        matched_groups(&app.site().catalog, &zh.sibling)
            .expect("读得出")
            .is_empty(),
        "作品那一层还留着这一次匹配的产出——名下别的变体照旧看得见它",
    );
    let 全部 = app.site().store.all_matches().expect("读得出沉淀库");
    assert_eq!(全部.len(), 1);
    assert!(!全部[0].accepted, "落下的该是「不是这条」");
    assert_eq!(全部[0].entry, zh.entry.to_string());
    assert_eq!(
        全部[0].note.as_deref(),
        Some("抽样核对过，撞的是同名的另一部"),
        "备注那一格没跟着按下的那一下记进去",
    );

    // **屏上那一堆当场没了，而账留着**：不然人按完什么都看不见。
    let 屏上 = 详情滚一趟(&ctx, &mut app);
    assert!(屏上.contains("就地清掉了"), "账没画出来：{屏上}");
    assert!(!屏上.contains(依据.as_str()), "那一堆该从屏上没了");
}

#[test]
fn 就地下一次肯定裁决那一堆的值一个字都不清() {
    // 验收第 3 条。**肯定那一档一个字都不清**：值是对的，变的只是它们的依据，
    // 而那靠输入指纹在下一趟刮削改写（`scrape::zh::judge` 的文档）。
    let ctx = headless::context();
    let (mut app, zh) = 带中文匹配的界面(中文匹配用的条数);
    停在(&ctx, &mut app, &zh.variant);
    let 原有 = matched_groups(&app.site().catalog, &zh.variant).expect("读得出");
    {
        let (screen, site) = app.queue_and_site();
        screen.judge_match(site, &zh.variant, zh.entry, true);
    }
    assert!(app.queue().error().is_none(), "{:?}", app.queue().error());
    let 账 = app.queue().judged().expect("按下去该交回一本账").clone();
    assert_eq!(账.cleared, 0, "肯定那一档一个字都不该清");
    assert!(账.fresh, "这条锚上本来没裁过");
    assert!(
        matches!(账.anchor, Anchor::Content { .. }) && 账.anchor.is_shareable(),
        "锚不是内容锚：{:?}",
        账.anchor,
    );
    assert_eq!(
        matched_groups(&app.site().catalog, &zh.variant).expect("读得出"),
        原有,
        "值被动过了",
    );

    // 沉淀库里那一条说的是「就是这条」。**下一趟不被分数更高的候选顶掉**那一半靠它：
    // 排序键上「人说过就是它」排在相似度前面（核心库 `人说过的那一条排在机器挑的前面`
    // 与 `作品那一层的章盖在人裁过的那个变体上而不是分数最高的那个`）。
    let 全部 = app.site().store.all_matches().expect("读得出沉淀库");
    assert_eq!(全部.len(), 1);
    assert!(全部[0].accepted);
    assert_eq!(全部[0].source, "中文离线源");
    assert_eq!(全部[0].entry, zh.entry.to_string());

    // **屏上不许把话说满**：那一堆仍写着「还等着裁」，因为那句话在依据里，
    // 下一趟刮削才改写——不说清楚，人会以为自己白按了。
    let 屏上 = 详情滚一趟(&ctx, &mut app);
    assert!(屏上.contains("一并定下"), "账没画出来：{屏上}");
    assert!(
        屏上.contains("下一趟 `romcat scrape` 才改写"),
        "没说清屏上那一堆为什么还写着「还等着裁」",
    );
}

#[test]
fn 界面与命令行裁同一条匹配落下的东西一模一样() {
    // 验收第 4 条。两边走的是**同一个函数**（`scrape::zh::judge`，命令行那侧是
    // `romcat zh judge`）；这条钉的是「同一条路」这句话在结果上成立。
    let ctx = headless::context();
    let (mut app, zh) = 带中文匹配的界面(中文匹配用的条数);
    停在(&ctx, &mut app, &zh.variant);
    let 一句 = "两边记同一句为什么";
    {
        let (screen, site) = app.queue_and_site();
        *screen.match_note_mut() = 一句.to_string();
        screen.judge_match(site, &zh.variant, zh.entry, false);
    }
    let 界面账 = app.queue().judged().expect("界面这一侧该有账").clone();
    // 作品那一层裁之前有几栏。**先钉住这个数**：底下那条比的是两边清完之后剩下什么，
    // 而拿两个空的比是恒真的废话（`/code-review` 报的第 1 条——早先那条把**作品名**
    // 递给了 `matched_groups`，那个函数收的是**变体键**，于是两边都读回空 vec）。
    let 作品那一层 = |catalog: &romcat_core::catalog::Catalog| -> Vec<(String, String, String)> {
        catalog
            .scraped_values(AnchorKind::Work.label(), &zh.work)
            .expect("读得出")
            .into_iter()
            .map(|value| (value.field, value.source, value.value))
            .collect()
    };

    // 命令行那一侧：同一份合成数据，同一个函数，参数是它从命令行收来的那几样。
    let (catalog, 另一份) = demo::queue_with_zh(中文匹配用的条数).expect("造得出合成数据");
    let 另一份 = 另一份.expect("合成数据里该摆着那一次匹配");
    assert_eq!(另一份, zh, "两份合成数据该一模一样，不然这条比的是两件事");
    let mut site = demo::site(catalog).expect("开得出现场");
    let 命令行账 = judge(
        &mut site.catalog,
        &mut site.store,
        &site.library,
        &zh.variant,
        zh.entry,
        false,
        Some(一句.to_string()),
    )
    .expect("裁得下去");

    assert_eq!(界面账, 命令行账, "两边交回来的账不一样");
    assert!(
        界面账.cleared >= 6 && 界面账.cleared_work > 0,
        "两边比的得是真清过的那一趟：{界面账:?}",
    );
    assert_eq!(
        作品那一层(&app.site().catalog),
        作品那一层(&site.catalog),
        "两边清完之后作品那一层剩下的东西不一样",
    );
    assert!(
        作品那一层(&app.site().catalog).is_empty(),
        "否定那一档该把作品那一层这一次匹配的产出清光",
    );
    assert_eq!(
        matched_groups(&app.site().catalog, &zh.variant).expect("读得出"),
        matched_groups(&site.catalog, &zh.variant).expect("读得出"),
        "两边清完之后那个变体身上剩下的东西不一样",
    );
    // 沉淀库里那一条也该一模一样。**时刻抹掉再比**：两次落下差了几毫秒是必然的，
    // 而这条要证的不是它们同一秒发生。
    let 抹掉时刻 = |mut rows: Vec<MatchVerdict>| {
        for one in &mut rows {
            one.decided_at = 0;
        }
        rows
    };
    assert_eq!(
        抹掉时刻(app.site().store.all_matches().expect("读得出")),
        抹掉时刻(site.store.all_matches().expect("读得出")),
        "两边落进沉淀库的不一样",
    );
}

#[test]
fn 自己没撞上那条条目的变体在屏上裁不动那一堆() {
    // 裁决钉在**这个变体的内容**上，而这一堆全在作品那一层——它是名下别的变体撞出来、
    // 在那一层数票胜出的。给它两颗按钮就是让人按一下、然后什么都不发生
    // （`scrape::zh::judge` 的文档：那一档一个字都不清）。
    let ctx = headless::context();
    let (mut app, zh) = 带中文匹配的界面(中文匹配用的条数);
    停在(&ctx, &mut app, &zh.sibling);

    let groups = matched_groups(&app.site().catalog, &zh.sibling).expect("读得出");
    assert_eq!(groups.len(), 1);
    assert!(!groups[0].from_variant, "这个变体自己不该撞上那条条目");

    let 屏上 = 详情滚一趟(&ctx, &mut app);
    assert!(
        屏上.contains(&format!("条目 {}", zh.entry)),
        "同一堆照旧看得见：名下变体看见的是同一堆作品级的值",
    );
    assert!(
        屏上.contains("裁它要去裁那个变体"),
        "没说清这一堆为什么在这儿裁不动：{屏上}",
    );
    assert!(!屏上.contains("就是这条"), "这一档不该给出裁决的按钮");
    assert!(!屏上.contains("不是这条"), "这一档不该给出裁决的按钮");
}

#[test]
fn 匹配裁决不另起一条撤销路() {
    // 验收第 6 条。**按批撤销**（票 `gui-redesign/08`）管的是**识别**那一批裁决：
    // 一次 `triage::apply` 就是一批，撤销把中立库与沉淀库两边一起放回去。匹配裁决
    // 不在那条路上——它落的是沉淀库里另一张表（挂单 Q33），一条批都不建。
    // 走回来那一下是**再裁一次**（同一条锚上后一条盖掉前一条），账那一行说的就是它。
    let ctx = headless::context();
    let (mut app, zh) = 带中文匹配的界面(中文匹配用的条数);
    停在(&ctx, &mut app, &zh.variant);
    {
        let (screen, site) = app.queue_and_site();
        screen.judge_match(site, &zh.variant, zh.entry, false);
    }
    assert!(app.queue().error().is_none(), "{:?}", app.queue().error());
    let library = app.site().library.clone();
    assert!(
        app.site()
            .store
            .batches(&library, 10)
            .expect("读得出")
            .is_empty(),
        "匹配裁决建出了一批——那会让「撤回第 N 批」去撤一件它管不了的事",
    );
    assert!(
        app.queue().applied().is_none(),
        "屏上不该冒出一颗撤回按钮：那颗按的是识别那一批",
    );
    let 屏上 = 详情滚一趟(&ctx, &mut app);
    assert!(
        屏上.contains("改主意就再裁一次"),
        "没说清走回来那一下该怎么走：{屏上}",
    );

    // 再裁一次，改主意那一条**盖掉**前一条：沉淀库里照旧只有一条。
    {
        let (screen, site) = app.queue_and_site();
        screen.judge_match(site, &zh.variant, zh.entry, true);
    }
    let 账 = app.queue().judged().expect("该有账").clone();
    assert!(!账.fresh, "这条锚上本来就裁过，这次该是覆盖");
    let 全部 = app.site().store.all_matches().expect("读得出");
    assert_eq!(全部.len(), 1, "同一条锚上攒出了两条打架的记录");
    assert!(全部[0].accepted);
}

#[test]
fn 队列缩到很小时那一堆的作品名照旧对得上文件名() {
    // `/code-review` 报的第 4 条：作品名硬写成头一个（`QUEUE_WORKS[0]`）的话，
    // `--queue-rows` 小的时候头两个够格的变体根本不是 n = 0、1——命中那一档按比例缩到
    // 零，够格的从「未命中」那一段起头，而那一段的文件名里带的是别的作品。
    // 屏上那一堆于是说着一部与文件名对不上的作品，正是这份合成数据最不该出的错。
    for rows in [50, 200, 2_000] {
        let (catalog, zh) = demo::queue_with_zh(rows).expect("造得出合成数据");
        let zh = zh.expect("这个规模上该摆得出那一次匹配");
        for key in [&zh.variant, &zh.sibling] {
            assert!(
                key.contains(&zh.work),
                "{rows} 条时 {key} 的文件名里没有作品「{}」",
                zh.work,
            );
            assert_eq!(
                catalog.work_of_variant(key).expect("读得出").as_deref(),
                Some(zh.work.as_str()),
                "{rows} 条时 {key} 没挂在那部作品底下",
            );
        }
        // 那一堆真的摆得出来：变体那两条加作品那四栏，归在同一个条目号底下。
        let groups = matched_groups(&catalog, &zh.variant).expect("读得出");
        assert_eq!(groups.len(), 1, "{rows} 条时那一堆没了");
        assert_eq!(groups[0].entry, zh.entry);
        assert_eq!(groups[0].values.len(), 6, "{rows} 条");
    }
}
