//! **待确认队列**是主界面：列得出、盖得住一批、裁得下去，而中文输入不在表格单元格里。
//!
//! 合成数据的**形状照真机来**（票 08 实测：队列 16,656 条，
//! `--under 合成库/gba/【全部汉化】` 852、`--under 合成库/GoodNES3.1` 1,543、`--name 汉化` 1,986）。
//! 前缀带着**根名**：键的第一段就是它（`path::library_key`）。
//! 这几个数在这里是**断言**而不是注释——ADR-0002 说批量裁决的覆盖面是这件事成不成立的
//! 分界，界面上点一行选中多少，就该是报告上印的那个数。

use egui::widgets::text_edit::TextEditState;
use romcat_core::catalog::State;
use romcat_core::catalog::identify::Tier;
use romcat_core::report::thousands;
use romcat_core::scrape::AnchorKind;
use romcat_core::scrape::zh::{judge, matched_groups};
use romcat_core::triage::{self, Axis, Draft, Filter, ItemOrder, Overrides, Scope, Shape};
use romcat_core::verdict::{self, Anchor, MatchVerdict};
use romcat_gui::app::{App, View};
use romcat_gui::clock::Clock;
use romcat_gui::queue::Mode;
use romcat_gui::table::ROW_HEIGHT;
use romcat_gui::tokens::Tokens;
use romcat_gui::{demo, headless};

mod shared;
use shared::画出来的字;

/// 这几条测试自己的**工作目录**。
///
/// **不共用 `demo::workspace()`**：那是 `--demo` 那个演示窗口用的目录，而窗口会往里写
/// **版式偏好**（面板拖到哪儿）。共用的话，维护者开一次演示窗口把某块面板拖高一截，
/// 下一次跑这几条测试屏上就少了几行——而断言数的正好是行数。
fn 工作目录() -> std::path::PathBuf {
    shared::干净工作目录("romcat-测试-队列")
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
/// 逐条那一屏右边那一块详情里，候选卡片、键位提示、一堆匹配六个字段各带一句**依据**——
/// 一屏摆不下是必然的，而 egui 不画视口之外的东西。所以这里滚的是**真的滚轮事件**，
/// 每一帧收的仍旧是那一帧真的画出来的字。
fn 详情滚一趟(ctx: &egui::Context, app: &mut App) -> String {
    const STEPS: u32 = 12;
    let mut out = String::new();
    for step in 0..=STEPS {
        let mut input = headless::input();
        // 指针停在右边那一块详情里——滚轮归指针底下那块滚动区。
        input
            .events
            .push(egui::Event::PointerMoved(egui::pos2(900.0, 500.0)));
        if step > 0 {
            input.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -150.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
        }
        out.push_str(&画出来的字(
            &headless::frame(ctx, input, |ui| app.ui(ui)),
        ));
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
fn 待选列表里画多少行文本输入框都是那几个() {
    // ADR-0005 的修订段：中文输入放弹层与详情，不放待选列表的行里——列表是虚拟化的，
    // 正在组字的那一行滚出视口时控件就没了，输入法上屏时没人接。「手工指定…」那一层开着，
    // 数得到文本框（表单那几格）；列表滚一整趟，这个数不许变。
    //
    // 这条断言不看代码长什么样，看的是**跑出来的结果**：egui 每画一个 `TextEdit` 就在
    // `ctx.data()` 里留下一份 `TextEditState`，于是「表格里有没有文本框」等价于
    // 「画的行数变了，这个数变不变」。
    let 数一遍 = |rows: u64| {
        let ctx = headless::context();
        let mut app = 界面(rows);
        // **表格在逐条那一屏上**：批优先是默认的（票 `gui-redesign/09`），
        // 而这条断言测的是那张虚拟化的表。**看整个队列**（收起默认展开的头一批）：只看头一批的话，
        // 光标停的那一条落在哪一批跟着规模变——能整批通过的排到前面之后，大的那一份头一批是中文离线源那批，
        // 详情里多出匹配那一堆的备注框，数的就不是表格了。
        {
            let (screen, _) = app.queue_and_site();
            if let Some(scope) = screen.scope() {
                screen.open_batch(&scope.shape);
            }
            screen.show_one_by_one();
            screen.open_manual();
        }
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
         ——那说明有文本框长在待选列表的行里",
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
    // **这一批全钉在内容上**：[ACG汉化组] 那 129 条落在「命中但一条都没自动通过」
    // 那一档，各自都有 CRC-32 加大小。补 `entry` 行之前这个数是 **2**（挂单 `Q168`
    // ——全库只有中文离线源撞上的那两个变体有 `entry` 行），另外 127 条
    // 「可导出分享」的裁决被记成只在本机成立。
    assert_eq!(
        applied.content_anchored, 这一批 as u64,
        "这一批该整批钉在内容上，实际只有 {} 条",
        applied.content_anchored,
    );
    assert_eq!(applied.path_anchored, 0, "这一批里不该有退到路径锚的");

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
        assert!(
            !batch.why().trim().is_empty(),
            "{:?} 说不出共同依据",
            batch.shape
        );
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
    assert_eq!(
        app.queue().queue().count(&那一组),
        row.count,
        "{}",
        row.label
    );
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
        assert_ne!(
            这一组, 上一组,
            "第 {round} 次「换一组样本」按下去还是同一组"
        );
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
    // 挑**分得出不止一个目录**的那一批：能整批通过的排在前面之后（票 `gui-looks-like-the-design/18`），头一批
    // 是单候选的一小批，只落在一个目录里，二级测不出来。
    let batch = app
        .queue()
        .queue()
        .batches()
        .iter()
        .find(|batch| {
            app.queue()
                .queue()
                .drill(&Scope::whole(batch.shape.clone()), Axis::Directory)
                .rows
                .len()
                > 1
        })
        .cloned()
        .expect("合成数据里该有一批落在好几个目录里");
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
    assert_eq!(
        app.site().store.counts().expect("读得出").unknown,
        row.count
    );
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
    assert!(
        屏上.contains(item.name()),
        "屏上没有文件名：{}",
        item.name()
    );
    // 路径照稿写完整的「根名 · 相对路径」（票 09 那一处写法）。
    let (根名, 相对路径) = romcat_core::path::split_root(&item.variant.key);
    let 路径 = format!("{根名} · {相对路径}");
    assert!(屏上.contains(&路径), "屏上没有完整路径：{路径}");
    let 依据 = &item.candidates[0].evidence;
    assert!(
        屏上.contains(依据.as_str()),
        "屏上没有那条候选的完整依据：{依据}"
    );
    assert!(
        屏上.contains(item.candidates[0].confidence.label()),
        "屏上没标出这条候选的置信度",
    );
}

/// 拿得到判据的那些，详情里说的是**内容锚**——不再每一条都退到路径（挂单 `Q168`）。
///
/// 内容判据走 `identify::content_print`，而那条路先问一句 `entry_fact`：合成数据从前
/// 只给中文离线源那两个变体写了 `entry` 行，别的一万六千多条一律答「压根没有」，
/// 于是 `--demo` 打开的队列里**每一条**详情都写着「裁决钉在：路径——只在本机成立」。
///
/// **两种锚都要有得看**：无判据那一档拿不到内容判据（那正是它落进那一档的原因），
/// 它只钉得住本机路径——一屏全是内容锚与一屏全是路径锚一样，都说明这份数据是假的。
#[test]
fn 拿得到判据的那些钉在内容上而不是每一条都退到路径() {
    let ctx = headless::context();
    let mut app = 界面(demo::QUEUE_ROWS);
    // 先切到**逐条看整个队列**：底下要停的那两条未必落在展开着的那一批里。
    {
        let (screen, _) = app.queue_and_site();
        if let Some(scope) = screen.scope() {
            screen.open_batch(&scope.shape);
        }
        screen.show_one_by_one();
    }
    跑(&ctx, &mut app, 1);
    let 选中 = app.queue().queue().selected().to_vec();
    let 挑一条 = |要的: bool| {
        选中
            .iter()
            .find(|item| (item.state == State::NoEvidence) == 要的)
            .map(|item| item.variant.key.clone())
    };
    let 有判据的 = 挑一条(false).expect("队列里该有拿得到判据的");
    let 无判据的 = 挑一条(true).expect("队列里该有无判据的");

    停在(&ctx, &mut app, &有判据的);
    let 屏上 = 详情滚一趟(&ctx, &mut app);
    assert!(
        屏上.contains(verdict::ANCHOR_CONTENT_SENTENCE),
        "拿得到判据的这一条，详情里没写它钉在内容上：{有判据的}",
    );

    停在(&ctx, &mut app, &无判据的);
    let 屏上 = 详情滚一趟(&ctx, &mut app);
    assert!(
        屏上.contains(verdict::ANCHOR_PATH_SENTENCE),
        "无判据的这一条该如实说只钉得住本机路径：{无判据的}",
    );
    // 一条候选都没有的这一条画结论那一枚：分得出是未命中还是无判据。
    assert!(
        屏上.lines().any(|line| line == State::NoEvidence.label()),
        "没有候选的这一条没画结论标签：{无判据的}",
    );
}

/// 整条队列里**钉得住内容锚的有多少条**——补 `entry` 行之前全库只有 **2** 条
/// （中文离线源撞上的那两个变体，挂单 `Q168`）。
///
/// 屏上那句「要落下 N 条：钉在内容上的 X 条（可导出分享），只钉得住本机路径的 Y 条」
/// 印的就是这两个数。**它们该分在无判据那道线上**：无判据那一档拿不到内容判据，
/// 那正是它落进这一档的原因；别的都有 CRC-32 加大小，也都该折得出内容锚。
#[test]
fn 整条队列里只有无判据那一档退到路径锚() {
    let app = 界面(demo::QUEUE_ROWS);
    let catalog = &app.site().catalog;
    let (mut 内容, mut 路径) = (0_u64, 0_u64);
    for row in catalog.queue_rows().expect("读得动队列") {
        if romcat_core::identify::content_print(catalog, &row.variant)
            .expect("算得动内容判据")
            .is_some()
        {
            内容 += 1;
        } else {
            路径 += 1;
        }
    }
    assert_eq!(内容 + 路径, demo::QUEUE_ROWS, "队列该是这么长");
    assert_eq!(路径, demo::NO_EVIDENCE, "退到路径锚的该正好是无判据那一档",);
    assert_eq!(
        内容,
        demo::QUEUE_ROWS - demo::NO_EVIDENCE,
        "钉得住内容锚的条数不对——`entry` 行是不是又只写了一部分",
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
    let 整批 = app
        .queue()
        .queue()
        .drill(&Scope::whole(batch.shape.clone()), Axis::Directory);
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

// ——— 一批里再下钻，并在任意一层整批处理（票 `gui-looks-like-the-design/19`）———

/// 一份合成数据的界面，展开着一批**能整批通过、而且切得出不止一项**的，连那一批的依据形状与切得动它的那个轴。
///
/// 「不止一项」是这几条测试的前提：只切得出一项的那一批，裁掉那一项就等于裁掉整批
/// ——卡片当场收起来，「剩下的部分仍能整批处理」根本无从验起。
///
/// **轴不写死成按目录**：一批的键是依据形状，而源与 DAT 多半只覆盖一个平台，于是合成数据里
/// 能整批通过的那几批按目录往往只落在一个目录下。哪个轴切得动由这里当场挑。
fn 展开一批能过的(ctx: &egui::Context) -> (App, Shape, Axis) {
    let mut app = 界面(demo::QUEUE_ROWS);
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
                    > 1
            })?;
            Some((shape, axis))
        })
        .expect("合成数据里该有一批能整批通过、又切得出好几项的");
    展开(&mut app, &shape);
    跑(ctx, &mut app, 1);
    (app, shape, axis)
}

/// 下钻到这个轴上最大的那一项，交回它的名字与条数。
fn 下钻到头一项(ctx: &egui::Context, app: &mut App, axis: Axis) -> (String, u64) {
    {
        let (screen, _) = app.queue_and_site();
        screen.set_axis(axis);
    }
    let 细分 = app
        .queue()
        .breakdown()
        .expect("展开了就该有细分")
        .rows
        .first()
        .cloned()
        .expect("这个轴上该切得出一项");
    {
        let (screen, _) = app.queue_and_site();
        screen.drill_into(&细分.label);
    }
    跑(ctx, app, 1);
    (细分.label, 细分.count)
}

/// 就地把下钻着的那一组整批裁掉（`通过` 为假时是拒绝），交回落下的那一批裁决的编号。
fn 就地裁掉(app: &mut App, 通过: bool) -> i64 {
    let scope = app.queue().scope().expect("下钻了才裁得动一部分");
    {
        let (screen, site) = app.queue_and_site();
        if 通过 {
            screen.pass(site, &scope);
        } else {
            screen.reject(site, &scope);
        }
        screen.commit(site);
    }
    assert!(app.queue().error().is_none(), "{:?}", app.queue().error());
    app.queue().applied().expect("落下了就该有账").batch
}

#[test]
fn 三种细分都切得动_每一项都写着条数与占比() {
    // 验收第 1 条。占比的分母是这一批**本来**多少条，所以一项都没裁掉时，各项加起来正好是一整批。
    let ctx = headless::context();
    let (mut app, _, _) = 展开一批能过的(&ctx);
    for axis in Axis::ALL {
        {
            let (screen, _) = app.queue_and_site();
            screen.set_axis(axis);
        }
        let 屏上 = 画一帧(&ctx, &mut app);
        let 细分 = app.queue().breakdown().expect("展开了就该有细分").clone();
        assert!(
            !细分.rows.is_empty(),
            "{} 这个轴上一项都切不出来",
            axis.label(),
        );
        assert_eq!(细分.whole, 细分.left, "一项都没裁掉时还剩的就是整批");
        for row in 细分.rows.iter().take(3) {
            assert!(
                屏上.contains(&thousands(row.count)),
                "{} 这个轴上「{}」那一项的条数没写在屏上：\n{屏上}",
                axis.label(),
                row.label,
            );
            let 占比 = row.share(细分.whole);
            assert!(
                (0.0..=1.0).contains(&占比) && 占比 > 0.0,
                "占比算歪了：{row:?} 占 {占比}",
            );
        }
        // 屏上摆得下的那几项，占比加起来不该大过一整批。
        let 加起来: f64 = 细分.rows.iter().map(|row| row.share(细分.whole)).sum();
        if 细分.locked.is_none() && axis == Axis::Directory {
            assert!(
                (加起来 - 1.0).abs() < 1e-9,
                "按目录那个轴一条只落一个组，各项占比该正好凑成一整批：{加起来}",
            );
        }
    }
}

#[test]
fn 点一项只看这一部分_就地那一框给的是这一部分的样本() {
    // 验收第 2 条。右栏那一栏照稿数的始终是整批——下钻收窄的是整批操作的作用范围，
    // 而「这一批长什么样」那句话不跟着只剩一组。
    let ctx = headless::context();
    let (mut app, shape, axis) = 展开一批能过的(&ctx);
    let 整批的样本 = app.queue().samples();
    let (那一项, 条数) = 下钻到头一项(&ctx, &mut app, axis);

    let 那一组 = app.queue().scope().expect("下钻了就该有作用范围");
    assert_eq!(那一组, Scope::under(shape, axis, &那一项));
    assert_eq!(app.queue().queue().count(&那一组), 条数);

    let 这一部分的样本 = app.queue().drilled_samples();
    assert!(!这一部分的样本.is_empty(), "就地那一框里一条样本都没有");
    let 这一部分的键: std::collections::BTreeSet<String> = app
        .queue()
        .queue()
        .selected()
        .iter()
        .filter(|item| 那一组.holds(item))
        .map(|item| item.variant.key.clone())
        .collect();
    assert_eq!(这一部分的键.len() as u64, 条数);
    for one in &这一部分的样本 {
        assert!(
            这一部分的键.contains(&one.key),
            "就地那一框里摆着不属于这一部分的样本：{one:?}",
        );
    }
    assert_eq!(
        app.queue().samples(),
        整批的样本,
        "下钻把右栏那一栏的样本也收窄了",
    );

    let 屏上 = 画一帧(&ctx, &mut app);
    assert!(
        屏上.contains(&format!("只看：{那一项}")),
        "就地那一框没说在只看哪一部分：\n{屏上}",
    );
    assert!(
        屏上.contains("返回整批"),
        "就地那一框里没有返回整批那一颗：\n{屏上}",
    );
}

#[test]
fn 就地整批通过之后那一项标着已通过_剩下的仍旧整批处理得动() {
    // 验收第 3、4 条。这一层落下的仍旧是**一批裁决**，那一项的条退出队列——而屏上那一项
    // 必须还在，不然人只会以为自己刚才什么也没做；底下那一排跟着改口说「剩余」。
    let ctx = headless::context();
    let (mut app, shape, axis) = 展开一批能过的(&ctx);
    let 原有 = app.queue().queue().pending();
    let 整批 = app.queue().breakdown().expect("展开了就该有细分").whole;
    let (那一项, 条数) = 下钻到头一项(&ctx, &mut app, axis);
    就地裁掉(&mut app, true);
    跑(&ctx, &mut app, 1);

    assert_eq!(
        app.queue().queue().pending(),
        原有 - 条数,
        "就地整批通过该只落下这一部分",
    );
    let 细分 = app.queue().breakdown().expect("还展开着就该有细分").clone();
    assert_eq!(
        (细分.whole, 细分.done, 细分.left),
        (整批, 条数, 整批 - 条数)
    );
    let 标着 = 细分
        .rows
        .iter()
        .find(|row| row.label == 那一项)
        .expect("裁完的那一项该还在细分那一栏上");
    assert_eq!(标着.done, Some(triage::PartKind::Passed));
    assert_eq!(标着.count, 条数, "标着的那一项该记落下时的条数");

    let 屏上 = 画一帧(&ctx, &mut app);
    // **按行精确比**：屏底那条提示条这会儿正写着「已通过 N 条」，`contains("已通过")`
    // 被它满足——把那一项旁边那枚标签整个删掉，那样的断言照样绿。
    assert!(
        屏上.lines().any(|line| line == "已通过"),
        "裁完的那一项旁边没标出那枚「已通过」：\n{屏上}",
    );
    assert!(
        屏上.lines().any(|line| line == 那一项),
        "裁完的那一项从细分那一栏上消失了：\n{屏上}",
    );
    // 剩下的部分照旧整批处理得动，按钮上写的是还剩多少。
    assert!(
        屏上.contains(&format!("通过剩余的 {} 条", thousands(细分.left))),
        "底下那一排没写明还剩多少：\n{屏上}",
    );
    assert!(
        屏上.contains(&format!("拒绝剩余的 {} 条", thousands(细分.left))),
        "拒绝那一颗没写明还剩多少：\n{屏上}",
    );
    assert!(
        !屏上.contains(&format!("全部通过（{} 条）", thousands(细分.left))),
        "裁过一部分之后还说「全部通过」：\n{屏上}",
    );
    // 按下去落的是整批剩下的那些，不是刚裁过的那一组。
    {
        let (screen, site) = app.queue_and_site();
        screen.pass(site, &Scope::whole(shape));
    }
    assert_eq!(
        app.queue().pending().expect("排得出计划").decided.len() as u64,
        细分.left,
        "底下那一排该作用在剩下的部分上",
    );
}

#[test]
fn 就地整批拒绝也落成一批裁决_进裁决记录也撤得掉_撤完那一项回到栏上() {
    // 验收第 3 条那一半：每一层落下的都是**一批裁决**，不另造一套。
    let ctx = headless::context();
    let (mut app, _, axis) = 展开一批能过的(&ctx);
    let 原有 = app.queue().queue().pending();
    let (那一项, 条数) = 下钻到头一项(&ctx, &mut app, axis);
    let id = 就地裁掉(&mut app, false);
    跑(&ctx, &mut app, 1);

    let 那一批 = 册子上的(&app, id);
    assert_eq!(那一批.rows, 条数);
    assert!(!那一批.undone());
    打开裁决记录(&ctx, &mut app);
    let 屏上 = 画一帧(&ctx, &mut app);
    assert!(
        屏上.contains(&在册那一行(&那一批)),
        "就地落下的那一批没进裁决记录：\n{屏上}",
    );

    {
        let (screen, site) = app.queue_and_site();
        screen.undo(site, id);
    }
    跑(&ctx, &mut app, 1);
    assert_eq!(
        app.queue().queue().pending(),
        原有,
        "撤掉之后那一部分该整个回到队列里",
    );
    let 细分 = app.queue().breakdown().expect("还展开着就该有细分").clone();
    assert!(!细分.partly_done(), "撤掉了还算裁过一部分");
    assert_eq!(
        细分
            .rows
            .iter()
            .find(|row| row.label == 那一项)
            .expect("那一项该回到栏上")
            .done,
        None,
        "撤掉之后那一项还标着已裁完",
    );
}

#[test]
fn 就地那一框里的逐条处理按下去真的收窄到这一组() {
    // 这一颗原先是**死的**：它与底下那一排共用一格记号，而那一排后画、是平赋值
    // （`= …clicked()`），把框里按下的那一下抹成了 `false`。一格记号管一颗之后才活。
    //
    // 只钉框里这一颗：底下那一排作用在整批剩下的那些上，由
    // `就地整批通过之后那一项标着已通过_剩下的仍旧整批处理得动` 那条钉着（它按下去之后
    // 断言排出的计划条数正好是 `breakdown.left`）。
    let ctx = headless::context();
    let (mut app, shape, axis) = 展开一批能过的(&ctx);
    let 整批 = app.queue().queue().count(&Scope::whole(shape));
    let (那一项, 条数) = 下钻到头一项(&ctx, &mut app, axis);
    assert!(
        条数 < 整批,
        "这一组该只是整批的一部分，不然分不出两颗的差别"
    );

    // **按框头定位**：就地那一框被细分那一栏顶在屏幕中段，而底下那一排这时多半已经被
    // 推出视口了——「屏上恰好两颗」是靠不住的判据。框里那颗是「只看：…」底下最近的那一颗。
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 框头 = 正好是它的每一处(&这一帧, &format!("只看：{那一项}"))
        .first()
        .copied()
        .expect("下钻之后就地那一框该开着");
    let 框里那颗 = 正好是它的每一处(&这一帧, "逐条处理")
        .into_iter()
        .filter(|一处| 一处.y > 框头.y)
        .min_by(|a, b| a.y.total_cmp(&b.y))
        .expect("就地那一框里该有一颗「逐条处理」");
    按在(&ctx, &mut app, 框里那颗);
    跑(&ctx, &mut app, 1);

    assert_eq!(app.queue().mode(), Mode::OneByOne, "按了没换到逐条");
    assert_eq!(
        app.queue().queue().selected().len() as u64,
        条数,
        "框里那一颗该把队列收窄到「{那一项}」这一组，而不是整批那 {整批} 条",
    );
}

/// 用一扇**更高的**窗跑一帧，事件照给。
///
/// 就地那一框把底下那一排顶出了默认那 800 高的视口，而 **egui 不画整个落在裁剪区外的控件**
/// ——点不到的按钮测不出它接在哪一层。截图门那边同一个问题是拿 1280×960 解的
/// （`snapshot.rs` 的 `下钻那一对的画面`），这里同理。
fn 高窗一帧(ctx: &egui::Context, app: &mut App, 事件: Vec<egui::Event>) -> egui::FullOutput {
    let mut input = headless::input();
    input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(headless::VIEWPORT[0], 1200.0),
    ));
    input.events = 事件;
    headless::frame(ctx, input, |ui| app.ui(ui))
}

/// 在那扇高窗里点一下**正好**写着 `那一段` 的地方。
fn 在高窗里点(ctx: &egui::Context, app: &mut App, 那一段: &str) {
    let 这一帧 = 高窗一帧(ctx, app, Vec::new());
    let Some(位置) = shared::正好那一段画在哪儿(&这一帧, 那一段) else {
        panic!(
            "屏上没有正好写着「{那一段}」的那一段，没处点：\n{}",
            画出来的字(&这一帧)
        );
    };
    let 按 = |pressed: bool| egui::Event::PointerButton {
        pos: 位置,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    高窗一帧(ctx, app, vec![egui::Event::PointerMoved(位置), 按(true)]);
    高窗一帧(ctx, app, vec![按(false)]);
    高窗一帧(ctx, app, Vec::new());
}

/// 眼下那份计划盖住多少条。
fn 计划条数(app: &App) -> u64 {
    app.queue()
        .pending()
        .expect("按下去该排出一份计划")
        .decided
        .len() as u64
}

#[test]
fn 两颗整批按钮各作用于一层_框里那颗只盖这一组_底下那一排盖剩下的() {
    // **点真按钮**。这两颗接反了（框里那颗接整批、底下那一排接下钻着的那一组），光靠
    // 「测试自己把 scope 交进去调 `pass()`」是照样全绿的——那种断言验的是自己刚设进去的值。
    // 而「每一层都能整批过」与「剩下的仍能整批处理」正是这张票第 3、4 条验收本身。
    let ctx = headless::context();
    let (mut app, shape, axis) = 展开一批能过的(&ctx);
    let 整批 = app.queue().queue().count(&Scope::whole(shape));
    let (那一项, 条数) = 下钻到头一项(&ctx, &mut app, axis);
    assert!(
        条数 < 整批,
        "这一组该只是整批的一部分，不然两颗盖住的数一样，分不出接没接反"
    );

    // **两次都在下钻着的时候点**：落下之后 `apply_plan` 会把下钻那一层放掉，那一刻
    // 作用范围本来就等于整批——那时再点，两颗接反了也看不出来。
    // ——— 就地那一框里那颗：只该盖住这一组 ———
    在高窗里点(&ctx, &mut app, &format!("通过这 {} 条", thousands(条数)));
    assert_eq!(
        计划条数(&app),
        条数,
        "框里那颗该只盖住「{那一项}」这一组，不是整批那 {整批} 条",
    );
    在高窗里点(&ctx, &mut app, "取消");
    assert!(app.queue().pending().is_none(), "按了取消，计划书该关上");

    // ——— 底下那一排那颗：照旧盖整批，不受下钻影响 ———
    assert!(
        app.queue()
            .scope()
            .is_some_and(|scope| scope.drill.is_some()),
        "这一步要在下钻着的时候点，不然分不出两颗接没接反",
    );
    在高窗里点(
        &ctx,
        &mut app,
        &format!("全部通过（{} 条）", thousands(整批)),
    );
    assert_eq!(
        计划条数(&app),
        整批,
        "底下那一排该盖住整批，不是下钻着的那一组",
    );
}

#[test]
fn 就地落下的那一批裁决在记录上认得出是哪一组() {
    // 挡住换轴那句话让人「先在裁决记录中撤销那几批」——记录上只写「第 N 批裁决 · M 条」
    // 的话，人根本挑不出该撤哪几批（设计稿 `passPart` 给那一行的 label 也带着这一段）。
    let ctx = headless::context();
    let (mut app, _, axis) = 展开一批能过的(&ctx);
    let 那一组 = {
        let (那一项, _) = 下钻到头一项(&ctx, &mut app, axis);
        那一项
    };
    let 作用范围 = app.queue().scope().expect("下钻了就该有作用范围").label();
    let id = 就地裁掉(&mut app, true);
    跑(&ctx, &mut app, 1);

    let 那一批 = 册子上的(&app, id);
    assert_eq!(
        那一批.note.as_deref(),
        Some(作用范围.as_str()),
        "就地落下的那一批没记下它作用在哪一组上",
    );
    assert!(
        作用范围.contains(&那一组),
        "作用范围那句话里该有这一组的名字：{作用范围}",
    );

    // ⚠️ **只钉到这里**：裁决记录那一行眼下把备注画在**悬停**里（`records_drawer` 里那个
    // `悬停`），屏上那两行字是「第 N 批裁决 · M 条」与「时刻 · summary」——`画出来的字`
    // 读不到悬停，所以「人在屏上认不认得出是哪一组」这一半没法在这一层钉。
    // 要它上屏得动记录那一行的样式（票 18 定的），那是另一个岔路口，记在挂单 `Q966`。
    打开裁决记录(&ctx, &mut app);
    let 屏上 = 画一帧(&ctx, &mut app);
    assert!(
        屏上.contains(&在册那一行(&那一批)),
        "就地落下的那一批没进裁决记录：\n{屏上}",
    );
}

#[test]
fn 处理过一部分之后换细分方式被挡住并说清为什么_撤掉那一批就解开() {
    // 验收第 5 条。两套切法会重叠，数就对不上了——挡住，并说得出为什么。
    let ctx = headless::context();
    let (mut app, _, axis) = 展开一批能过的(&ctx);
    let _ = 下钻到头一项(&ctx, &mut app, axis);
    let id = 就地裁掉(&mut app, true);
    跑(&ctx, &mut app, 1);

    let 那句话 = app
        .queue()
        .breakdown()
        .expect("还展开着就该有细分")
        .axis_refusal()
        .expect("裁过一部分就该挡住换轴");
    assert_eq!(
        那句话,
        format!(
            "已{}处理了一部分；换细分方式前，请先在裁决记录中撤销那几批",
            axis.label(),
        ),
    );
    let 屏上 = 画一帧(&ctx, &mut app);
    assert!(屏上.contains(&那句话), "挡住了却没说为什么：\n{屏上}");

    // 换轴一个字都不动。
    let 换成 = Axis::ALL
        .into_iter()
        .find(|one| *one != axis)
        .expect("三个轴里该有别的");
    {
        let (screen, _) = app.queue_and_site();
        screen.set_axis(换成);
    }
    跑(&ctx, &mut app, 1);
    assert_eq!(
        app.queue().axis(),
        axis,
        "换细分方式没被挡住：那一排真换过去了",
    );
    // **守卫那一侧拒下时也得说得出理由**，不许是光秃的返回（ADR-0005「再修订：『不禁按钮』
    // 那一条什么时候允许同时画灰」，拿主意的人 2026-09-20 定）；而且它与屏上那一排底下
    // 常驻的那一行**是同一句**——各写一份迟早两个说法。
    assert_eq!(
        app.queue().error(),
        Some(那句话.as_str()),
        "换轴被挡下了却一声不吭",
    );
    assert_eq!(
        app.queue().breakdown().expect("还展开着").locked,
        Some(axis),
        "锁着的轴不该跟着变",
    );
    assert!(
        app.queue()
            .breakdown()
            .expect("还展开着")
            .rows
            .iter()
            .any(|row| row.done.is_some()),
        "换轴之后裁完的那一项不见了——那一排换过去了",
    );

    // 撤掉那一批就解开。
    {
        let (screen, site) = app.queue_and_site();
        screen.undo(site, id);
    }
    跑(&ctx, &mut app, 1);
    assert_eq!(app.queue().breakdown().expect("还展开着").locked, None);
    {
        let (screen, _) = app.queue_and_site();
        screen.set_axis(换成);
    }
    跑(&ctx, &mut app, 1);
    assert!(app.queue().scope().is_some(), "撤完之后那一批该还展开着",);
    assert_eq!(app.queue().axis(), 换成, "撤完之后该换得动轴了");
    assert_eq!(
        app.queue().error(),
        None,
        "换成了，上一次那句拒绝该跟着作废",
    );
    assert!(
        app.queue()
            .breakdown()
            .expect("还展开着")
            .axis_refusal()
            .is_none(),
        "撤完了还挡着",
    );
}

#[test]
fn 下钻处理之后屏头批头与左栏徽标那几个数仍旧一致() {
    // 验收第 6 条。三处画的是同一个数（`Queue::pending`），批头那个大数是这一批眼下还剩多少
    // ——它们一起变，不然屏上会有两个互相打架的数。
    let ctx = headless::context();
    let (mut app, shape, axis) = 展开一批能过的(&ctx);
    let (_, 条数) = 下钻到头一项(&ctx, &mut app, axis);
    就地裁掉(&mut app, true);
    跑(&ctx, &mut app, 1);

    let 屏上 = 画一帧(&ctx, &mut app);
    let 待确认 = app.queue().queue().pending();
    assert!(
        屏上.contains(&format!("{} 个变体待确认", thousands(待确认))),
        "屏头那个数没跟着换：\n{屏上}",
    );
    assert_eq!(
        app.queue()
            .queue()
            .batches()
            .iter()
            .map(|batch| batch.count)
            .sum::<u64>(),
        待确认,
        "各批条数加起来与屏头那个数对不上",
    );
    let 这一批 = app
        .queue()
        .queue()
        .batches()
        .iter()
        .find(|batch| batch.shape == shape)
        .expect("裁掉一部分之后这一批该还在")
        .clone();
    assert!(
        屏上.contains(&format!(
            "已处理 {} 条，剩余 {} 条",
            thousands(条数),
            thousands(这一批.count)
        )),
        "批头没说清裁掉了多少、还剩多少：\n{屏上}",
    );
    // 左栏那枚徽标画的是同一个数（`App::rail` 的「待裁」）：屏上正好写着它的地方至少有那一处。
    assert!(
        !正好是它的每一处(
            &headless::frame(&ctx, headless::input(), |ui| app.ui(ui)),
            &thousands(待确认),
        )
        .is_empty(),
        "左栏徽标上那个数不见了",
    );
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

    let index = verdict::Index::load(&app.site().store, &app.site().library_identity)
        .expect("读得出沉淀库");
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
        assert!(
            !界面这批.is_empty(),
            "这一批一条都没盖住：{:?}",
            batch.shape
        );

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
        applied.matched as usize, 这一批,
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
fn 计划书开着时按退出键撤掉计划书_一个字都不写_关上之后逐条键盘照常() {
    // 计划书是一层弹层（票 `gui-looks-like-the-design/04`）：底下那一屏的单键快捷键一个都不接，
    // Esc 关最上面那一层，等于按页脚上的「取消」。**关上之后那道门得放开**——一直拦着的话，
    // 逐条流从此一个键都按不动，比拦不住更难查。
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
    app.queue_and_site().0.show_one_by_one();
    跑(&ctx, &mut app, 1);
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
    跑(&ctx, &mut app, 2);
    assert!(app.queue().pending().is_some(), "前提：计划书弹出来了");

    按(&ctx, &mut app, egui::Key::N);
    assert!(
        app.queue().applied().is_none(),
        "计划书开着时按 N 当场落下了一条",
    );

    按(&ctx, &mut app, egui::Key::Escape);
    assert!(app.queue().pending().is_none(), "按了 Esc，计划书还挂着");
    assert_eq!(
        app.site().store.counts().expect("读得出沉淀库").total,
        0,
        "按了 Esc 却写了沉淀库",
    );

    // 关上那一下补的那一帧（弹层要了一次重画）。
    跑(&ctx, &mut app, 1);
    按(&ctx, &mut app, egui::Key::N);
    assert!(
        app.queue().applied().is_some(),
        "计划书关上之后按 N 不生效——逐条流的键盘被一直拦着",
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
    assert!(
        账.cleared >= 6,
        "同一次匹配带来的六个字段该一条不剩：{账:?}"
    );
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
        屏上.contains("下一趟刮削才改写"),
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
        &site.library_identity,
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
    let library_identity = app.site().library_identity.clone();
    assert!(
        app.site()
            .store
            .batches(&library_identity, 10)
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

#[test]
fn 卡片悬停说得出这一批在命令行上是什么() {
    // ADR-0005：屏上点一张卡片、命令行敲一条 `--shape`，选中的必须是同一批。三个轴的
    // 输入框各自挂着「命令行上是 `--under`」，**卡片上一个字都没有**——而卡片才是人真正
    // 挑批的地方。少了这一句，从界面上挑好的那一批得回命令行 `romcat triage list`
    // 再列一遍才拿得到那串字（挂单 `Q177`）。
    //
    // 断的是**那串字逐字对得上**：折算那一对（`Shape::selector` / `Shape::parse`）是
    // 「屏上这一批与命令行那一条是同一批」在字面上的落点，悬停里印的必须是它折出来的
    // 那一串，不是另编一句像模像样的话。报告里那一行也是这么印的（`triage::report`）。
    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 2);

    let 头一批 = app
        .queue()
        .queue()
        .batches()
        .first()
        .expect("这个规模上该分得出批")
        .clone();
    let 那句 = format!("命令行上是 `--shape '{}'`", 头一批.shape.selector());

    let 屏上 = shared::悬停在(&ctx, &头一批.why(), |ui| app.ui(ui));
    assert!(屏上.contains(&那句), "卡片上悬不出「{那句}」：\n{屏上}",);
}

/// 一份**手搭的**中立库，形状照排查报告里那份现场来：3 个变体，2 个跑过识别
/// （1 个命中、自动通过，1 个未命中、带一条低置信候选进队列），**1 个连识别都还没跑过**。
///
/// 合成数据造不出第三种——`demo::queue` 给每个变体都写了一行结论。而这一屏最该说清的
/// 正是它：同一份库，命令行 `triage list` 印「另有 1 个变体连识别都还没跑过」。
///
/// 搭子在 `shared::小库`——浏览屏那边搭的是同一份形状（`三档并排的库`），
/// 两处只差各落哪一档、摆在哪几个平台上、工作目录是哪个。
fn 有一个连识别都没跑过的库() -> App {
    use shared::档;

    shared::小库(
        &[
            ("SFC", "命中.zip", 档::命中),
            ("FC", "未命中.zip", 档::待裁决),
            ("GB", "还没轮到它.zip", 档::还没识别),
        ],
        // 这一份从前递的是 `demo::workspace()`——本文件头上那条规矩明写着不许，
        // 而它是整套界面测试里唯一破例的一处（挂单 `Q350`）。
        工作目录(),
    )
}

#[test]
fn 待确认屏说得出库里还有几个变体连识别都没跑过() {
    // 词表「还没识别」：一个变体**连识别都还没跑过**。它一条候选都没有、裁不了，
    // 队列里根本没有它——不说出来的话，「队列 1 条」会被读成「库里只剩 1 条没定下来」，
    // 而那 1 个变体连命中率的分母都进不去。同一份库命令行印的就是这句话。
    let ctx = headless::context();
    let mut app = 有一个连识别都没跑过的库();
    assert_eq!(
        app.queue().queue().not_run(),
        1,
        "库里该有 1 个没跑过识别的"
    );
    assert_eq!(app.queue().queue().pending(), 1, "队列里该有 1 条待裁决");

    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| app.ui(ui)));
    assert!(
        屏上.contains("连识别都还没跑过"),
        "屏头一个字都没提库里那 1 个还没识别的：\n{屏上}",
    );
    // **两句话不许长得一样**：底下四档里「一条候选都没有」那一档曾经也叫「还没识别」，
    // 于是屏上会同时出现两个「还没识别」，一个说 1、一个说 0。词表把这两件事裁成了
    // 两个词（`CONTEXT.md` 的**还没识别**与**没有候选**）。
    assert!(
        屏上.contains("没有候选"),
        "四档里那一档该叫「没有候选」：\n{屏上}",
    );
    assert!(
        !屏上.contains("还没识别 0"),
        "「还没识别」这四个字被四档那一行拿去说了另一件事：\n{屏上}",
    );
}

#[test]
fn 逐条流按_u_无可撤时说清楚而不是一声不吭() {
    // 同一个函数里 `Y` 那一支写着「不许什么都不做还不吭声」；命令行 `undo --last`
    // 无批时报错退 1。这一支一声不吭地返回，人只会以为键盘坏了。
    let ctx = headless::context();
    let mut app = 界面(2_000);
    {
        let (screen, _) = app.queue_and_site();
        screen.show_one_by_one();
    }
    跑(&ctx, &mut app, 1);
    assert_eq!(app.queue().mode(), Mode::OneByOne);
    assert!(app.queue().applied().is_none(), "这一趟还没落下过任何一批");

    按(&ctx, &mut app, egui::Key::U);
    let 错 = app.queue().error().expect("`U` 无可撤时得说一句");
    assert!(错.contains("没什么可撤"), "得说清什么都没发生：{错}");
    // 更早那几批裁决如今在界面上撤得掉（裁决记录，挂单 `Q448`），那条路就该指到那儿，
    // 不该再叫人回终端。
    assert!(
        错.contains("裁决记录"),
        "更早那几批裁决得指到界面上撤得掉它们的地方：{错}",
    );
    assert!(
        !错.contains("romcat"),
        "界面上撤得掉更早那几批了，还在叫人回终端：{错}",
    );
    assert!(app.queue().undone().is_none(), "什么都没撤，账上不许多一笔");
}

// ——— 点表头换排序（票 `parking-3/08`，挂账 `D153`）———

/// 切到**逐条**那一屏（表在那儿），并把展开的那一批收起来——不收的话逐条只看得见
/// 那一批，而这几条要看的是整个队列排出来什么样。
fn 逐条看整个队列(ctx: &egui::Context, app: &mut App) {
    {
        let (screen, _) = app.queue_and_site();
        if let Some(scope) = screen.scope() {
            screen.open_batch(&scope.shape);
        }
        screen.show_one_by_one();
    }
    跑(ctx, app, 2);
}

/// 屏上那一栏待选列表**画出来的第一条**是哪个文件名。
///
/// 列表在左边、先画，一条头一行是文件名：按画出来的次序，头一段正好是队列里某一条文件名的，就是列表的第一条
/// （右边详情里那个文件名画在它后头）。
///
/// **看屏上而不是看 `selected()[0]`**：这一条要证的正是「画出来的换了」。
fn 列表上第一条(app: &App, 屏上: &str) -> String {
    let 名字: std::collections::BTreeSet<&str> = app
        .queue()
        .queue()
        .selected()
        .iter()
        .map(romcat_core::triage::Item::name)
        .collect();
    屏上
        .lines()
        .find(|line| 名字.contains(line))
        .unwrap_or_else(|| panic!("屏上一条待选的都没画：\n{屏上}"))
        .to_string()
}

/// 按一下屏上写着 `那一段` 的地方（移过去、按下、松开），返回**松开那一帧**画出来的字。
fn 点一下(ctx: &egui::Context, app: &mut App, 那一段: &str) -> String {
    let 头一帧 = headless::frame(ctx, headless::input(), |ui| app.ui(ui));
    let Some(位置) = shared::那一段画在哪儿(&头一帧, 那一段) else {
        panic!("屏上没有「{那一段}」，没处点：\n{}", 画出来的字(&头一帧));
    };
    按在(ctx, app, 位置)
}

/// 按一下屏上**正好**写着 `那一段` 的那一颗（整段一字不差），返回松开之后那一帧画出来的字。
///
/// 按钮上的字是别的句子里的一截时用它：裁决记录里那颗「撤销」也在那一块开头那句说明里。
fn 点正好那一颗(ctx: &egui::Context, app: &mut App, 那一段: &str) -> String {
    let 头一帧 = headless::frame(ctx, headless::input(), |ui| app.ui(ui));
    let Some(位置) = shared::正好那一段画在哪儿(&头一帧, 那一段) else {
        panic!(
            "屏上没有正好写着「{那一段}」的那一段，没处点：\n{}",
            画出来的字(&头一帧)
        );
    };
    按在(ctx, app, 位置)
}

/// 在这个位置上按下、松开，返回**松开之后下一帧**画出来的字。
fn 按在(ctx: &egui::Context, app: &mut App, 位置: egui::Pos2) -> String {
    let 按 = |pressed: bool| egui::Event::PointerButton {
        pos: 位置,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut input = headless::input();
    input.events.push(egui::Event::PointerMoved(位置));
    input.events.push(按(true));
    headless::frame(ctx, input, |ui| app.ui(ui));
    let mut input = headless::input();
    input.events.push(按(false));
    headless::frame(ctx, input, |ui| app.ui(ui));
    // 换排序是这一帧末尾写回去的，屏上那张表下一帧才照新次序画。
    画出来的字(&headless::frame(ctx, headless::input(), |ui| app.ui(ui)))
}

#[test]
fn 点一下列表头上的排序屏上画出来的第一条就换了() {
    // 挂账 `D153`：真机上 16,656 条待裁决，而次序**永远是变体的键**——想按容量或按
    // 结论找出该先动的那几条，从前只能靠选择器缩小范围。逐条那一屏照稿两栏之后
    // （票 `gui-looks-like-the-design/18`），排序收在待选列表栏头那一颗里。
    let ctx = headless::context();
    let mut app = 界面(demo::QUEUE_ROWS);
    逐条看整个队列(&ctx, &mut app);

    let 头一帧 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| app.ui(ui)));
    let 先 = 列表上第一条(&app, &头一帧);
    assert_eq!(
        app.queue().queue().order(),
        (ItemOrder::Key, false),
        "列出来那一下就是按变体的键正着排（`queue_rows` 的 `ORDER BY v.key`）",
    );

    // 点栏头那颗「变体 ▲」打开那几列，再点正在排的「变体」——正在排的那一列再点一次就**翻方向**。
    let _ = 点正好那一颗(&ctx, &mut app, &format!("{} ▲", ItemOrder::Key.label()));
    let 屏上 = 点正好那一颗(&ctx, &mut app, ItemOrder::Key.label());
    assert_eq!(
        app.queue().queue().order(),
        (ItemOrder::Key, true),
        "点的是正在排的那一列，该翻方向：\n{屏上}",
    );
    let 后 = 列表上第一条(&app, &屏上);
    assert_ne!(先, 后, "屏上画出来的第一条没换：\n{屏上}");
    assert!(
        屏上
            .lines()
            .any(|line| line == format!("{} ▼", ItemOrder::Key.label())),
        "栏头那颗上的箭头没跟着翻：\n{屏上}",
    );

    // **屏上那一条就是排出来的第一条**：两处对不上的话，人按 Y/N 裁的不是他看的那一条。
    let items = app.queue().queue().selected();
    assert_eq!(后, items[0].name(), "画出来的第一条不是排在第一位的那条");
    assert_eq!(
        先,
        items[items.len() - 1].name(),
        "倒过来之后，原来的第一条该落到最后一条",
    );
}

#[test]
fn 五列每一列都排得出一个全序而且一条都不少() {
    // **换一份同样合法的数据，它还成立吗**：这一条不断言「第一行是某个名字」——
    // 那种断言只在这一批合成数据上成立。它断言的是次序本身的性质：**相邻两条都不逆**、
    // 而且**一条都不多一条都不少**。
    //
    // 只比相邻的那 n−1 对，**不两两比**：反对称与传递性是 `ItemOrder::cmp_items` 自己
    // 保证的（每一档都拿唯一的键收尾），拿 16,656 条去穷举 1.38 亿对既证不出更多东西，
    // 又会把门禁拖垮。
    let mut app = 界面(demo::QUEUE_ROWS);
    let 原有: Vec<String> = {
        let items = app.queue().queue().selected();
        let mut keys: Vec<String> = items.iter().map(|item| item.variant.key.clone()).collect();
        keys.sort_unstable();
        keys
    };
    assert_eq!(原有.len() as u64, demo::QUEUE_ROWS);

    for order in ItemOrder::ALL {
        for descending in [false, true] {
            app.queue_and_site().0.sort_by(order, descending);
            let items = app.queue().queue().selected();
            assert_eq!(items.len() as u64, demo::QUEUE_ROWS, "排一次少了几条");
            // `is_lt` 而不是 `!is_gt`：**同值时按键收尾**，于是相邻两条永远分得出先后
            // ——出现一对 `Equal` 就说明那一档漏了收尾那一比，这一列上的次序就不唯一了。
            let 逆了 = items
                .windows(2)
                .position(|pair| !order.cmp_items(&pair[0], &pair[1], descending).is_lt());
            if let Some(at) = 逆了 {
                panic!(
                    "按「{}」{}排，第 {at} 对没排好：{} 在 {} 前面",
                    order.label(),
                    if descending { "倒着" } else { "正着" },
                    items[at].variant.key,
                    items[at + 1].variant.key,
                );
            }
            let mut 键: Vec<&str> = items.iter().map(|item| item.variant.key.as_str()).collect();
            键.sort_unstable();
            assert!(
                键.iter().copied().eq(原有.iter().map(String::as_str)),
                "排一次之后条目变了一批",
            );
        }
    }
}

#[test]
fn 按容量倒着排头一条就是最大的那一条() {
    // 挂账 `D153` 里那句「人可能想按容量或按结论排」——按容量倒着排，头一条就是
    // 真机上该先动的那一条。**拿全队列自己数出来的最大值比**，不写死一个数字；
    // 而且**换一份同样合法的数据**跑：这一条与上面那条走的是同一段代码、不同的规模。
    let mut app = 界面(2_000);
    let 最大 = app
        .queue()
        .queue()
        .selected()
        .iter()
        .map(|item| item.variant.bytes)
        .max()
        .expect("队列不空");
    let 最小 = app
        .queue()
        .queue()
        .selected()
        .iter()
        .map(|item| item.variant.bytes)
        .min()
        .expect("队列不空");
    assert!(最大 > 最小, "这批数据里容量全一样，这一条就没在测排序");

    app.queue_and_site().0.sort_by(ItemOrder::Bytes, true);
    assert_eq!(app.queue().queue().selected()[0].variant.bytes, 最大);
    app.queue_and_site().0.sort_by(ItemOrder::Bytes, false);
    assert_eq!(app.queue().queue().selected()[0].variant.bytes, 最小);
}

#[test]
fn 换排序之后换选择器不必重排而且光标还停在同一条上() {
    // 排的是**整份条目**，而换选择器那一次分区是稳定的——于是换个选择器，选中的那一段
    // 照旧是排好的。光标记的是键不是下标（`Screen::resolve_cursor`），排序一换它得
    // 自己找回新位置：不然人点表头之前停在哪一条，点完就换成了别人。
    let ctx = headless::context();
    let mut app = 界面(2_000);
    逐条看整个队列(&ctx, &mut app);
    let 停在的 = app.queue().queue().selected()[7].variant.key.clone();
    app.queue_and_site().0.pick_row(&停在的);
    跑(&ctx, &mut app, 1);
    assert_eq!(app.queue().at(), 7);

    app.queue_and_site().0.sort_by(ItemOrder::Bytes, true);
    跑(&ctx, &mut app, 1);
    let at = app.queue().at();
    assert_eq!(
        app.queue().queue().selected()[at].variant.key,
        停在的,
        "排完之后光标落到别人身上了",
    );

    // 换个选择器：那一段照旧按容量倒着排，**没有重排一遍**。
    let (axis, label) = {
        let queue = app.queue().queue();
        let row = queue.groups(Axis::Directory).first().expect("有分组");
        (Axis::Directory, row.label.clone())
    };
    app.queue_and_site().0.pick(axis, &label);
    跑(&ctx, &mut app, 1);
    let items = app.queue().queue().selected();
    assert!(!items.is_empty(), "这一组该选中一批");
    assert!(
        items
            .windows(2)
            .all(|pair| pair[0].variant.bytes >= pair[1].variant.bytes),
        "换完选择器那一段不是排好的",
    );
    assert_eq!(app.queue().queue().order(), (ItemOrder::Bytes, true));

    // ⭐ **筛着的时候再换一列排，选中的还得是同一批**。
    //
    // `selected()` 是内存里那份 `Vec` 前 N 条那一段。排完整份而不重新分一次区的话，
    // 那条线当场错位——那一段从「选择器选中的那些」变成「排完之后的前 N 条」，
    // 于是屏上写着「这一批 N 条」，按下整批通过落下的却是人从没选过的变体。
    let 这一批: std::collections::BTreeSet<String> =
        items.iter().map(|item| item.variant.key.clone()).collect();
    assert!(
        (这一批.len() as u64) < demo::QUEUE_ROWS,
        "这一组该是队列的一部分而不是全部，不然下面那条断言是句空话",
    );
    for (order, descending) in [(ItemOrder::State, false), (ItemOrder::Key, true)] {
        app.queue_and_site().0.sort_by(order, descending);
        跑(&ctx, &mut app, 1);
        let items = app.queue().queue().selected();
        assert_eq!(
            items
                .iter()
                .map(|item| item.variant.key.clone())
                .collect::<std::collections::BTreeSet<_>>(),
            这一批,
            "按「{}」排完，选中的不再是选择器选出来的那一批",
            order.label(),
        );
        assert!(
            items
                .windows(2)
                .all(|pair| order.cmp_items(&pair[0], &pair[1], descending).is_lt()),
            "按「{}」排完，选中的那一段不是排好的",
            order.label(),
        );
    }
}

#[test]
fn 停在中文离线源那一条上依据里没有星号也没有文档编号() {
    // 屏上的每一句都得是维护者用得上的话（票 `gui-looks-like-the-design/03`）。
    // 依据这句话是核心库写的、落进中立库、原样画在这一屏上——egui 不认 markdown。
    let ctx = headless::context();
    let (mut app, zh) = 带中文匹配的界面(中文匹配用的条数);
    停在(&ctx, &mut app, &zh.variant);
    let 屏上 = 详情滚一趟(&ctx, &mut app);
    assert!(
        屏上.contains(&format!("条目 {}", zh.entry)),
        "依据没摆上屏，这条什么都没验：\n{屏上}",
    );
    for 不该有 in ["**", "ADR-"] {
        assert!(!屏上.contains(不该有), "屏上画出了「{不该有}」：\n{屏上}");
    }
}

// ——— 裁决记录：列得出、撤得掉任意一批裁决（票 `gui-answers-all-six/06`）———

/// 两份**重复拷贝**（同一份内容，钉的是同一条**内容锚**）的名字，外加一份别的内容。
///
/// 重复拷贝是真机上的常态，也是**两批裁决在同一条锚上叠起来**唯一的来路：裁过的变体
/// 当场退出队列，同一个变体裁不了第二遍；而另一份拷贝还在队列里，裁它落下的那一批
/// 就盖住了前一批（核心库 `tests/triage.rs` 那条同形状的测试）。
const 甲名: &str = "甲 某汉化.gba";
const 乙名: &str = "乙 某汉化.gba";
const 丙名: &str = "丙 另一部.gba";

/// 变体的键（`shared::变体` 折出来的那个样子）。
fn 键(名字: &str) -> String {
    format!("{}/GBA/{名字}", shared::根)
}

/// 搭一份**手搭的**小现场：三个识别过、一条候选都没有的变体，甲与乙是同一份内容。
///
/// 合成数据（`demo::queue`）里每个变体的内容各不相同，搭不出「两批叠在同一条锚上」；
/// `shared::小库` 不写内容判据，裁决全退到路径锚上，也搭不出。所以这里自己摆
/// 条目、变体、哈希与结论——四样缺一样，内容锚就折不出来（`identify::content_prints`）。
fn 有两份重复拷贝的现场() -> romcat_core::site::Site {
    use romcat_core::catalog::identify::{ContentHash, Identification};
    use romcat_core::catalog::{Catalog, EntryRecord, Verdict as 这次的判断};
    use romcat_core::fs::{EntryKind, EntryMeta};
    use romcat_core::platform::Manifest;
    use romcat_core::site::Site;

    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    romcat_core::catalog::roots::add_root(
        &catalog,
        None,
        shared::根,
        std::path::Path::new(&format!("/{}", shared::根)),
    )
    .expect("建得出根");
    let variants: Vec<_> = [甲名, 乙名, 丙名]
        .iter()
        .map(|名字| shared::变体("GBA", 名字))
        .collect();
    let entries: Vec<EntryRecord> = variants
        .iter()
        .map(|variant| EntryRecord {
            key: variant.key.clone(),
            kind: EntryKind::File,
            meta: EntryMeta::Known {
                len: variant.bytes,
                modified: None,
            },
            non_utf8: false,
            verdict: 这次的判断::Added,
            sample: None,
            container: None,
        })
        .collect();
    // **条目写在前头**：那一趟会把这些键上算过的东西当成重扫来的一起清掉（`demo::queue` 同一句）。
    catalog.write(1, &entries).expect("写得进条目");
    catalog
        .replace_variants(&variants, 1, &Manifest::default())
        .expect("写得进变体");
    let hashes: Vec<ContentHash> = variants
        .iter()
        .map(|variant| ContentHash {
            key: variant.key.clone(),
            inner: String::new(),
            size: variant.bytes,
            crc32: if variant.key == 键(丙名) {
                0x0BAD_F00D
            } else {
                0x5EED_CAFE
            },
            looked: true,
            header: None,
            bare_size: None,
            bare_crc32: None,
            nkit: None,
            sha1: None,
            bare_sha1: None,
        })
        .collect();
    catalog.put_content_hashes(&hashes).expect("写得进哈希");
    let 结论: Vec<Identification> = variants
        .iter()
        .map(|variant| Identification {
            variant_key: variant.key.clone(),
            platform: None,
            standalone: None,
            edition: None,
            state: State::Unmatched,
            reason: None,
            units: 1,
            nkit: 0,
            read_bytes: 0,
            work_id: None,
            release_id: None,
            candidates: Vec::new(),
        })
        .collect();
    catalog.write_identifications(&结论).expect("写得进结论");
    Site::in_memory(
        catalog,
        verdict::Store::in_memory().expect("开得出沉淀库"),
        shared::根,
    )
}

/// **在这个窗口之外**裁一条：上一趟会话、或者命令行，走的是核心库那条同一条路。
/// 返回落下的那一批裁决的号。
fn 在别处裁(site: &mut romcat_core::site::Site, key: &str) -> i64 {
    let index = verdict::Index::load(&site.store, &site.library_identity).expect("读得出沉淀库");
    let mut queue = triage::Queue::load(&site.catalog, &index).expect("列得出队列");
    queue.set_filter(Filter {
        keys: vec![key.to_string()],
        ..Filter::default()
    });
    let decide = Draft {
        unknown: true,
        ..Draft::default()
    }
    .build(&site.library_identity)
    .expect("说得成立");
    let plan = queue
        .plan(&site.catalog, &site.store, &decide)
        .expect("排得出计划");
    queue
        .apply(&mut site.catalog, &mut site.store, &plan)
        .expect("落得下去")
        .batch
}

/// 光标停到这一条上，按 `N`（记成「我看过了，认不出」）：**逐条流里一下就是一批裁决**。
/// 返回落下的那一批的号。
fn 逐条拒(ctx: &egui::Context, app: &mut App, key: &str) -> i64 {
    停在(ctx, app, key);
    按(ctx, app, egui::Key::N);
    assert!(app.queue().error().is_none(), "{:?}", app.queue().error());
    app.queue().applied().expect("落下了就该有账").batch
}

/// 点一下顶栏上那颗「裁决记录」，再跑两帧让那一块摆稳。
fn 打开裁决记录(ctx: &egui::Context, app: &mut App) {
    let _ = 点一下(ctx, app, "裁决记录");
    跑(ctx, app, 2);
}

/// 这一帧画出来的字。
fn 画一帧(ctx: &egui::Context, app: &mut App) -> String {
    画出来的字(&headless::frame(ctx, headless::input(), |ui| app.ui(ui)))
}

/// 裁决记录里这一批的**主行**：第几批裁决、多少条（拿主意的人 2026-09-15 定，票 `gui-looks-like-the-design/18`）。
/// 撤没撤过不在这一行的字里——撤过的那一行划删除线、旁边一枚「已撤销」。
fn 在册那一行(batch: &verdict::Batch) -> String {
    format!("第 {} 批裁决 · {} 条", batch.id, thousands(batch.rows))
}

/// 裁决记录里这一批的**副行**：什么时候落的（本地时间的短格式，与任务屏历史、库屏「上次扫描」同一处画，
/// `clock::Clock`）、裁成什么。
fn 副行(batch: &verdict::Batch) -> String {
    format!(
        "{} · {}",
        Clock::System.short(batch.decided_at),
        batch.summary
    )
}

/// 这一帧里**正好**写着 `那几个字` 的每一段字画在哪儿（左上角），按画出来的次序。
fn 正好是它的每一处(output: &egui::FullOutput, 那几个字: &str) -> Vec<egui::Pos2> {
    fn 找(shape: &egui::epaint::Shape, 那几个字: &str, 每一处: &mut Vec<egui::Pos2>) {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == 那几个字 => {
                每一处.push(text.pos);
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

/// 这一帧里**含着** `那几个字` 的每一段字画在哪儿（左上角），按画出来的次序。
fn 含着它的每一处(output: &egui::FullOutput, 那几个字: &str) -> Vec<egui::Pos2> {
    fn 找(shape: &egui::epaint::Shape, 那几个字: &str, 每一处: &mut Vec<egui::Pos2>) {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text().contains(那几个字) => {
                每一处.push(text.pos);
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

/// 沉淀库里点名的这一批。
fn 册子上的(app: &App, id: i64) -> verdict::Batch {
    app.site()
        .store
        .batch(id)
        .expect("读得出沉淀库")
        .expect("沉淀库里有这一批")
}

#[test]
fn 裁决记录从沉淀库列出落过的每一批裁决_连这个窗口开之前落下的() {
    // 从前这一屏只记得「本次进程里刚落下的那一批」——一个可空的位置。窗口一关、
    // 或者那一批是命令行落的，屏上就一个字都没有，而沉淀库里每一批都还躺着。
    let mut site = 有两份重复拷贝的现场();
    let 早先那一批 = 在别处裁(&mut site, &键(丙名));
    let ctx = headless::context();
    let mut app = App::new(site, 工作目录());
    let 刚落下的 = 逐条拒(&ctx, &mut app, &键(甲名));

    打开裁决记录(&ctx, &mut app);
    let 屏上 = 画一帧(&ctx, &mut app);
    let 行 = |id: i64| {
        let 那一行 = 在册那一行(&册子上的(&app, id));
        屏上
            .lines()
            .position(|line| line == 那一行)
            .unwrap_or_else(|| panic!("裁决记录里没有「{那一行}」：\n{屏上}"))
    };
    assert!(
        行(刚落下的) < 行(早先那一批),
        "裁决记录该新的在前：\n{屏上}",
    );
}

#[test]
fn 撤得掉任意一批裁决不只是最后一批_撤完那些变体当场回到队列里() {
    // 从前界面上只撤得掉「刚落下的那一批」，更早的那些得回终端
    // `romcat triage undo --batch <号>`（挂单 `Q448`）。
    let ctx = headless::context();
    let mut app = App::new(有两份重复拷贝的现场(), 工作目录());
    let _甲那一批 = 逐条拒(&ctx, &mut app, &键(甲名));
    let 丙那一批 = 逐条拒(&ctx, &mut app, &键(丙名));
    let 乙那一批 = 逐条拒(&ctx, &mut app, &键(乙名));
    assert_eq!(
        app.queue().queue().pending(),
        0,
        "三条都裁完了，队列该是空的"
    );
    assert_eq!(
        app.queue().applied().map(|applied| applied.batch),
        Some(乙那一批),
        "前提：最后落下的是乙那一批，要撤的丙那一批不是它",
    );

    let (screen, site) = app.queue_and_site();
    screen.undo(site, 丙那一批);
    assert!(screen.error().is_none(), "{:?}", screen.error());

    // **当场回到队列里**：不重新列、不重跑识别，逐条那张表下一帧就画得出它。
    let 屏上 = 画一帧(&ctx, &mut app);
    assert!(
        屏上.lines().any(|line| line == 丙名),
        "撤完丙那一批，丙没回到队列里：\n{屏上}",
    );
    assert_eq!(app.queue().queue().pending(), 1, "只该回来丙那一条");
    // 别的批一条不受牵连：甲、乙那两批照旧在册，它们的变体照旧不在队列里。
    for 名字 in [甲名, 乙名] {
        assert!(
            !屏上.lines().any(|line| line == 名字),
            "撤的是丙那一批，{名字} 却也回到了队列里：\n{屏上}",
        );
    }
    assert!(
        !册子上的(&app, 乙那一批).undone(),
        "撤的是丙那一批，乙那一批却被标成了已撤",
    );
}

#[test]
fn 撤一批被后来还在册的一批盖住时当场拒并说清是哪一批盖的_撤过的仍在记录上标着已撤() {
    // **批与批在同一条锚上是叠着的**：甲、乙是同一份内容，乙那一批记着的「它盖掉了什么」
    // 正是甲那一批落下的那条。先撤甲那一批的话它回不到「落下之前」——核心库整份拒下，
    // 界面要把那句话原样画出来，说清是哪一批盖的，让人先撤那一批。
    let ctx = headless::context();
    let mut app = App::new(有两份重复拷贝的现场(), 工作目录());
    let 甲那一批 = 逐条拒(&ctx, &mut app, &键(甲名));
    let 乙那一批 = 逐条拒(&ctx, &mut app, &键(乙名));
    打开裁决记录(&ctx, &mut app);
    // 丙没裁过，还在队列里。
    let 裁完剩下的 = app.queue().queue().pending();

    let (screen, site) = app.queue_and_site();
    screen.undo(site, 甲那一批);
    let 那句话 = screen
        .error()
        .expect("被后来还在册的一批盖住了，得当场说一句")
        .to_string();
    assert!(
        那句话.contains(&format!("第 {乙那一批} 批")),
        "那句话没说清是哪一批盖的：{那句话}",
    );
    // **画在裁决记录那块抽屉里**：逐条那一屏的详情里也画着同一句（整屏那一处），只看「屏上有没有」证不出它在抽屉里。
    let out = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 屏上 = 画出来的字(&out);
    let 抽屉左沿 = headless::VIEWPORT[0] - Tokens::builtin().layout.drawer_width;
    assert!(
        含着它的每一处(&out, &那句话)
            .iter()
            .any(|at| at.x > 抽屉左沿),
        "拒下的那句话没画在裁决记录那块抽屉里：\n{屏上}",
    );
    // **拒下了就一个字都不动**：甲那一批照旧在册，甲照旧不在队列里。
    assert!(
        屏上
            .lines()
            .any(|line| line == 在册那一行(&册子上的(&app, 甲那一批))),
        "撤不动的那一批在裁决记录上不该变样：\n{屏上}",
    );
    assert!(
        !册子上的(&app, 甲那一批).undone() && !屏上.lines().any(|line| line == "已撤销"),
        "撤不动的那一批被标成了已撤销：\n{屏上}",
    );
    assert_eq!(
        app.queue().queue().pending(),
        裁完剩下的,
        "拒下了却有变体回到了队列里",
    );
    assert!(
        !屏上.lines().any(|line| line == 甲名),
        "拒下了，甲却回到了队列里：\n{屏上}",
    );

    // 照那句话说的，先撤盖住它的那一批，再撤它。
    for batch in [乙那一批, 甲那一批] {
        let (screen, site) = app.queue_and_site();
        screen.undo(site, batch);
        assert!(
            screen.error().is_none(),
            "第 {batch} 批：{:?}",
            screen.error()
        );
    }
    跑(&ctx, &mut app, 1);
    let out = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 屏上 = 画出来的字(&out);
    // **撤过的那两批仍在裁决记录上**，标着已撤销，旁边是「放回」而不是「撤销」。
    for batch in [甲那一批, 乙那一批] {
        let 册子 = 册子上的(&app, batch);
        assert!(册子.undone(), "第 {batch} 批该是撤过的");
        for 那一行 in [在册那一行(&册子), 副行(&册子)] {
            assert!(
                屏上.lines().any(|line| line == 那一行),
                "撤过的那一批从裁决记录上消失了：「{那一行}」\n{屏上}",
            );
        }
    }
    assert_eq!(
        屏上.lines().filter(|line| *line == "已撤销").count(),
        2,
        "撤过的两批旁边该各有一枚「已撤销」：\n{屏上}",
    );
    // 数抽屉里的：屏底那条提示条（刚撤完那一批）自己也带一颗「放回」。
    let 抽屉左沿 = headless::VIEWPORT[0] - Tokens::builtin().layout.drawer_width;
    let 抽屉里的 = |那几个字: &str| {
        正好是它的每一处(&out, 那几个字)
            .iter()
            .filter(|at| at.x > 抽屉左沿)
            .count()
    };
    assert_eq!(
        抽屉里的("放回"),
        2,
        "撤过的两批旁边该各有一颗「放回」：\n{屏上}"
    );
    assert_eq!(
        抽屉里的("撤销"),
        0,
        "两批都撤过了，还摆着「撤销」：\n{屏上}"
    );
    assert_eq!(
        app.queue().queue().pending(),
        裁完剩下的 + 2,
        "两份拷贝都该回到队列里",
    );
}

#[test]
fn 屏上一批变体与一批裁决两处措辞分得开() {
    // 词表**批**那一条：一级分批那一列卡片是**一批变体**（还没落任何库），裁决记录里的是
    // **一批裁决**（撤销的粒度）。两处从前都只说「批」——「分成 12 批」旁边摆着「撤回第 3 批」，
    // 读的人分不出第 3 批是哪一张卡片。
    let ctx = headless::context();
    let mut app = App::new(有两份重复拷贝的现场(), 工作目录());
    let 落下的 = 逐条拒(&ctx, &mut app, &键(甲名));
    app.queue_and_site().0.show_batches();
    打开裁决记录(&ctx, &mut app);
    let 屏上 = 画一帧(&ctx, &mut app);

    let 分批那一句 = 屏上
        .lines()
        .find(|line| line.starts_with("可整批处理的排在前面 · "))
        .unwrap_or_else(|| panic!("屏上没有一级分批那一句：\n{屏上}"));
    assert!(
        分批那一句.contains("批变体") && !分批那一句.contains("裁决"),
        "一级分批说的是一批变体：{分批那一句}",
    );
    assert!(
        屏上
            .lines()
            .any(|line| line == 在册那一行(&册子上的(&app, 落下的))),
        "裁决记录里没有刚落下的那一批：\n{屏上}",
    );
    // 落下之后那一句走屏底的提示条（拿主意的人 2026-09-15 定，照稿）：说几条，不说「第 N 批」。
    assert!(
        屏上.lines().any(|line| line == "已拒绝 1 条"),
        "落下之后屏底没有那条提示条：\n{屏上}",
    );
    for line in 屏上.lines() {
        if line.contains(&format!("第 {落下的} 批")) {
            assert!(
                line.contains("批裁决"),
                "这一句提到第 {落下的} 批，却没说是哪一种批：{line}",
            );
        }
    }
}

#[test]
fn 裁决记录里那颗撤销与放回按下去就是撤销与放回() {
    // 前几条直接调 `Screen::undo` / `Screen::redo`；这一条点的是裁决记录里那两颗按钮本身
    // ——票 `gui-looks-like-the-design/18` 重排的正是它们。接线接反了（「撤销」那颗去放回），
    // 这一条当场红。
    let ctx = headless::context();
    let mut app = App::new(有两份重复拷贝的现场(), 工作目录());
    let 那一批 = 逐条拒(&ctx, &mut app, &键(丙名));
    打开裁决记录(&ctx, &mut app);

    let 屏上 = 点正好那一颗(&ctx, &mut app, "撤销");
    assert!(app.queue().error().is_none(), "{:?}", app.queue().error());
    assert!(
        册子上的(&app, 那一批).undone(),
        "按了「撤销」，那一批却没撤：\n{屏上}",
    );
    assert!(
        屏上.lines().any(|line| line == 丙名),
        "撤完了，丙没回到队列里：\n{屏上}",
    );

    let 屏上 = 点正好那一颗(&ctx, &mut app, "放回");
    assert!(app.queue().error().is_none(), "{:?}", app.queue().error());
    let 册子 = 册子上的(&app, 那一批);
    assert!(!册子.undone(), "按了「放回」，那一批却还撤着：\n{屏上}");
    assert!(
        屏上.lines().any(|line| line == 在册那一行(&册子)),
        "放回之后那一行该回到在册：\n{屏上}",
    );
    assert!(
        !屏上.lines().any(|line| line == 丙名),
        "放回之后丙该再退出队列：\n{屏上}",
    );
}

// ——— 照稿重排（票 `gui-looks-like-the-design/18`）———

#[test]
fn 正文头上三格_前几批只数能整批通过的_与库屏工序段同一个数_另两格是有多个候选与没有候选() {
    // 设计稿待确认屏正文头上那三格（`.qsum`）。头一格与库屏工序段「前 N 批可一次处理 N 个」出自同一处
    // （`triage::head_coverage` 与 `Queue::coverage` 走的是同一副），不另造一份数。
    let ctx = headless::context();
    let mut app = 界面(demo::QUEUE_ROWS);
    跑(&ctx, &mut app, 2);
    let 账 = app.queue().queue().coverage(triage::HEADLINE_BATCHES);
    let index = verdict::Index::load(&app.site().store, &app.site().library_identity)
        .expect("读得出沉淀库");
    assert_eq!(
        triage::head_coverage(&app.site().catalog, &index, triage::HEADLINE_BATCHES)
            .expect("算得出前几批"),
        账,
        "与库屏工序段说的不是同一个数",
    );
    assert!(
        账.head > 0 && 账.multiple > 0 && 账.bare > 0,
        "合成数据里三样都该有：{账:?}",
    );

    let 屏上 = 画一帧(&ctx, &mut app);
    let 行: Vec<&str> = 屏上.lines().collect();
    for (数, 说明) in [
        (
            账.head,
            format!("前 {} 批可直接批量处理 · 每条只有一个候选", 账.head_batches),
        ),
        (账.multiple, "有多个候选 · 需要逐条选择".to_string()),
        (账.bare, "没有候选 · 无法批量处理".to_string()),
    ] {
        let at = 行
            .iter()
            .position(|line| *line == 说明)
            .unwrap_or_else(|| panic!("屏上没有「{说明}」：\n{屏上}"));
        assert_eq!(
            at.checked_sub(1).map(|up| 行[up]),
            Some(thousands(数).as_str()),
            "「{说明}」上面那个数不对：\n{屏上}",
        );
    }
    assert!(
        !屏上.contains("批变体盖住"),
        "旧的那句「前 N 批变体盖住…」还在：\n{屏上}",
    );
}

#[test]
fn 屏头右侧照稿_三枚置信度标签_按批逐条那一对_裁决记录() {
    // 设计稿 `.scrhead`：高 / 中 / 低三枚标签、「按批｜逐条」、「裁决记录 N」。原来那句「队列 N 条待裁决；选中 N 条 ·
    // N 条候选」与第四枚「没有候选 N」照稿删了（拿主意的人 2026-09-15）：待确认几个写在副标题上，没有候选几个写在
    // 正文第三格；选中几条在逐条那一屏上写着。
    let ctx = headless::context();
    let mut app = 界面(demo::QUEUE_ROWS);
    跑(&ctx, &mut app, 2);
    let 屏上 = 画一帧(&ctx, &mut app);
    for (tier, count) in app.queue().queue().all_tiers().to_vec() {
        let 那一枚 = format!("{} {}", tier.label(), thousands(count));
        assert_eq!(
            屏上.lines().any(|line| line == 那一枚),
            tier != Tier::Unidentified,
            "「{那一枚}」该不该在屏头上：\n{屏上}",
        );
    }
    assert!(
        !屏上.contains("条待裁决"),
        "屏头上还写着「队列 N 条待裁决」：\n{屏上}"
    );
    assert!(
        屏上.lines().any(|line| line == "裁决记录 0"),
        "屏头上没有「裁决记录 N」：\n{屏上}",
    );
    assert!(
        屏上.lines().any(|line| line == "重新列队列"),
        "屏头最右那颗「重新列队列」没了：\n{屏上}",
    );
    assert!(
        !屏上.lines().any(|line| line.starts_with("沉淀库 ")),
        "「沉淀库 在哪」挪进裁决记录抽屉的页脚了，屏头上不该还有：\n{屏上}",
    );

    let 多候选 = app
        .queue()
        .queue()
        .coverage(triage::HEADLINE_BATCHES)
        .multiple;
    let 屏上 = 点正好那一颗(&ctx, &mut app, "逐条");
    assert_eq!(app.queue().mode(), Mode::OneByOne, "按了「逐条」");
    assert!(
        app.queue().scope().is_none(),
        "屏头那颗「逐条」看的不是默认展开的头一批",
    );
    // 照稿（设计稿 `.obolist` 栏头）：屏头「逐条」看的是有多个候选的那些，栏头写「有多个候选 N 条」，
    // 详情抬头写「第 1 条 · 共 N 条有多个候选」。
    assert_eq!(app.queue().queue().selected().len() as u64, 多候选);
    assert!(
        屏上
            .lines()
            .any(|line| line == format!("第 1 条 · 共 {} 条有多个候选", thousands(多候选))),
        "详情抬头没照稿写全：\n{屏上}"
    );
    // 屏头那三枚数的是整个队列，切到逐条之后照旧（设计稿 `.scrhead`）。
    for (tier, count) in app.queue().queue().all_tiers().to_vec() {
        let 那一枚 = format!("{} {}", tier.label(), thousands(count));
        assert_eq!(
            屏上.lines().any(|line| line == 那一枚),
            tier != Tier::Unidentified,
            "切到逐条之后「{那一枚}」该不该在屏头上：\n{屏上}",
        );
    }
    for 该有 in [
        "有多个候选".to_string(),
        format!("{} 条", thousands(多候选)),
    ] {
        assert!(
            屏上.lines().any(|line| line == 该有),
            "待选列表栏头没有「{该有}」：\n{屏上}"
        );
    }
    let _ = 点正好那一颗(&ctx, &mut app, "按批");
    assert_eq!(app.queue().mode(), Mode::Batches, "按了「按批」");
}

#[test]
fn 裁决记录是贴右边的一块抽屉_开着照样裁得动_关闭收得起来() {
    // 设计稿 `aside.drawer#lots`：浮在正文上、贴右边、宽 380，不是模态（拿主意的人 2026-09-15 定，收挂单 `Q625`、
    // `Q661`）。开着的时候照样在队列上裁、撤了看队列变。
    let ctx = headless::context();
    let mut app = App::new(有两份重复拷贝的现场(), 工作目录());
    let 丙那一批 = 逐条拒(&ctx, &mut app, &键(丙名));
    打开裁决记录(&ctx, &mut app);

    let out = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 屏上 = 画出来的字(&out);
    let 左沿 = headless::VIEWPORT[0] - Tokens::builtin().layout.drawer_width;
    for 那一段 in [
        "裁决记录".to_string(),
        "撤销后，相关变体会回到待确认队列".to_string(),
        在册那一行(&册子上的(&app, 丙那一批)),
        副行(&册子上的(&app, 丙那一批)),
        "关闭".to_string(),
        format!("沉淀库 {}", app.site().store.location()),
    ] {
        let 在 = shared::正好那一段画在哪儿(&out, &那一段)
            .unwrap_or_else(|| panic!("抽屉里没有「{那一段}」：\n{屏上}"));
        assert!(
            在.x > 左沿,
            "「{那一段}」没画在右边那一块抽屉里（x = {}，抽屉左沿 {左沿}）",
            在.x
        );
    }

    // **不是模态**：开着的时候键盘照样归逐条流，按 `N` 就落下一批，抽屉里跟着多一行。
    let 乙那一批 = 逐条拒(&ctx, &mut app, &键(乙名));
    let 屏上 = 画一帧(&ctx, &mut app);
    assert!(
        屏上
            .lines()
            .any(|line| line == 在册那一行(&册子上的(&app, 乙那一批))),
        "抽屉开着时落下的那一批没列进去：\n{屏上}",
    );

    let 屏上 = 点正好那一颗(&ctx, &mut app, "关闭");
    assert!(
        !屏上
            .lines()
            .any(|line| line == "撤销后，相关变体会回到待确认队列"),
        "按了「关闭」，抽屉还开着：\n{屏上}",
    );
}

#[test]
fn 还没识别时的空态照稿_暂无待确认项_几个变体还没识别_一颗运行识别() {
    // 设计稿 `#q-empty`。词照词表写「还没识别」（拿主意的人 2026-09-14 定），中文常规体不加粗。
    use shared::档;

    let ctx = headless::context();
    let mut app = shared::小库(
        &[
            ("FC", "甲.zip", 档::还没识别),
            ("FC", "乙.zip", 档::还没识别),
        ],
        shared::干净工作目录("romcat-测试-队列-空态"),
    );
    app.show_view(View::Queue);
    跑(&ctx, &mut app, 2);
    assert!(!app.queue().queue().identified(), "前提：一趟识别都没跑过");
    let 屏上 = 画一帧(&ctx, &mut app);
    for 那一句 in [
        "暂无待确认项：2 个变体还没识别",
        "识别完成后，无法自动确定的结果会出现在这里，并按判定依据自动分批。",
        "运行识别",
        "与「库」页面工序中的「识别」是同一个操作。",
    ] {
        assert!(
            屏上.lines().any(|line| line == 那一句),
            "空态上没有「{那一句}」：\n{屏上}",
        );
    }
}

/// 按批那一屏的正文**真的滚一趟**（滚轮事件），把一路上画出来的字按**头一回画出来的先后**收起来，一段一行、不重复。
///
/// 正文比一屏长（展开那一批、虚线框、框下那几批），egui 不画视口之外的东西。先滚回顶上，再一步步往下滚到底。
fn 正文滚一趟(ctx: &egui::Context, app: &mut App) -> String {
    const STEPS: u32 = 30;
    let 指针 = egui::pos2(760.0, 500.0);
    let 滚 = |dy: f32| {
        let mut input = headless::input();
        input.events.push(egui::Event::PointerMoved(指针));
        input.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, dy),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        });
        input
    };
    for _ in 0..STEPS {
        headless::frame(ctx, 滚(10_000.0), |ui| app.ui(ui));
    }
    let mut 收到的: Vec<String> = Vec::new();
    for step in 0..=STEPS {
        let input = if step == 0 {
            headless::input()
        } else {
            滚(-120.0)
        };
        let 这一帧 = 画出来的字(&headless::frame(ctx, input, |ui| app.ui(ui)));
        for line in 这一帧.lines() {
            if !收到的.iter().any(|seen| seen == line) {
                收到的.push(line.to_owned());
            }
        }
    }
    收到的.join("\n")
}

#[test]
fn 批列表照稿_能整批通过的先摆前几批_虚线框写着没有候选几个_框下是不能整批通过的批() {
    // 设计稿 `#batches` 与 `.bare`（拿主意的人 2026-09-15 定）：能整批通过的照稿画成卡片，只先摆前几批，
    // 余下的收成一句「另有 N 批（M 条）同样只有一个候选。」；接着是「没有候选」那个虚线框；框下列出不能整批通过的批，
    // 样式同卡片，主按钮是「逐条处理」，留「全部拒绝」，不给「全部通过」。数全是核心库那一处交的（`Coverage`）。
    let ctx = headless::context();
    let mut app = 界面(demo::QUEUE_ROWS);
    跑(&ctx, &mut app, 2);
    let 账 = app.queue().queue().coverage(triage::HEADLINE_BATCHES);
    let batches = app.queue().queue().batches().to_vec();
    // 一屏摆不下：真的滚一趟，按头一回画出来的先后收字（egui 不画视口之外的东西）。
    let 屏上 = 正文滚一趟(&ctx, &mut app);
    let 行: Vec<&str> = 屏上.lines().collect();
    let 第几行 = |那一句: &str| {
        行.iter()
            .position(|line| *line == 那一句)
            .unwrap_or_else(|| panic!("屏上没有「{那一句}」：\n{屏上}"))
    };

    let 框头 = 第几行(&format!("没有候选 · {} 个", thousands(账.bare)));
    let 分布: Vec<String> = State::ALL
        .iter()
        .zip(账.bare_by_state)
        .filter(|(_, count)| *count > 0)
        .map(|(state, count)| format!("{} {} 个", state.label(), thousands(count)))
        .collect();
    let 说明 = format!(
        "其中{}。它们没有可供确认的候选，因此不能批量通过。",
        分布.join("、")
    );
    assert!(第几行(&说明) > 框头, "虚线框里那句说明该在框头底下");
    assert!(
        行.contains(&"逐条指定"),
        "虚线框里没有「逐条指定」：\n{屏上}"
    );
    if 账.rest_answerable_batches > 0 {
        let 另有 = 第几行(&format!(
            "另有 {} 批（{} 条）同样只有一个候选。",
            thousands(账.rest_answerable_batches as u64),
            thousands(账.rest_answerable),
        ));
        assert!(另有 < 框头, "「另有 N 批同样只有一个候选」该在虚线框上面");
    }

    // 框下是不能整批通过的批：一条候选都没有的那一批，卡头第二行（为什么没定下来）画在虚线框底下。
    let 不能过 = batches
        .iter()
        .find(|batch| {
            matches!(
                &batch.shape,
                Shape::Bare {
                    reason: Some(_),
                    ..
                }
            )
        })
        .cloned()
        .expect("合成数据里该有一批说得出为什么没定下来");
    let Shape::Bare {
        reason: Some(理由),
    ..
    } = &不能过.shape
    else {
        unreachable!()
    };
    assert!(第几行(理由) > 框头, "不能整批通过的那一批该摆在虚线框底下");

    展开(&mut app, &不能过.shape);
    跑(&ctx, &mut app, 1);
    let 屏上 = 正文滚一趟(&ctx, &mut app);
    assert!(
        屏上.lines().any(|line| line == "全部拒绝"),
        "不能整批通过的那一批照样拒得了：\n{屏上}"
    );
    assert!(
        !屏上.lines().any(|line| line.starts_with("全部通过")),
        "不能整批通过的那一批不给「全部通过」：\n{屏上}",
    );
    assert!(
        屏上.lines().any(|line| line == "逐条处理"),
        "不能整批通过的那一批主按钮是「逐条处理」：\n{屏上}"
    );
}

#[test]
fn 批卡照稿_卡头是形状各段与共同依据_展开后判定依据是那句原话_样本旁边换一组() {
    // 设计稿 `.batch`（拿主意的人 2026-09-15 定）：卡头第一行照依据形状各段排、第二行放那句共同依据；展开后判定依据那一框
    // 画「判定依据：」加 `Batch::why` 原话，不另编句子；样本旁边那颗叫「换一组」（词表**批**不拿来说样本）。
    let ctx = headless::context();
    let mut app = 界面(demo::QUEUE_ROWS);
    跑(&ctx, &mut app, 2);
    let batch = app.queue().queue().batches()[0].clone();
    assert!(batch.passable(), "能整批通过的排在前面，头一批该是它");
    let Shape::Candidates {
        source,
        dat,
        convention,
        fanout,
        ..
    } = &batch.shape
    else {
        panic!("头一批该是有候选的那一支：{:?}", batch.shape);
    };
    let 屏上 = 画一帧(&ctx, &mut app);
    let mut 该有 = vec![
        source.clone(),
        dat.clone(),
        convention.label().to_string(),
        fanout.label().to_string(),
        format!("判定依据：{}", batch.why()),
        format!("全部通过（{} 条）", thousands(batch.count)),
    ];
    该有.extend(batch.evidence.clone());
    for 那一段 in 该有.iter().map(String::as_str).chain([
        "逐条处理",
        "全部拒绝",
        "换一组",
        "细分",
        "按目录",
        "按候选作品",
        "按命名规律",
    ]) {
        assert!(
            屏上.lines().any(|line| line == 那一段),
            "屏上没有「{那一段}」：\n{屏上}"
        );
    }
    assert!(
        !屏上.contains("换一组样本"),
        "样本旁边那颗照稿叫「换一组」：\n{屏上}"
    );
    // **下钻着的那一组说在就地那一框里**（票 `gui-looks-like-the-design/19` 照稿换的）：
    // 票 18 那一趟拿屏底一句「作用范围：…」说它，而底下那一排从此始终作用于整批剩下的部分
    // ——那一句留着就会与按钮上写的数打架。没下钻时那一框照稿不画。
    assert!(
        !屏上.lines().any(|line| line.starts_with("只看：")),
        "没下钻就开了就地那一框：\n{屏上}"
    );
    let 那一组 = app
        .queue()
        .queue()
        .drill(&Scope::whole(batch.shape.clone()), Axis::Directory)
        .rows
        .first()
        .cloned()
        .expect("该有一组");
    app.queue_and_site().0.drill_into(&那一组.label);
    let 下钻之后 = 画一帧(&ctx, &mut app);
    for 那一段 in [
        format!("只看：{}", 那一组.label),
        format!("通过这 {} 条", thousands(那一组.count)),
        format!("拒绝这 {} 条", thousands(那一组.count)),
        "返回整批".to_owned(),
    ] {
        assert!(
            下钻之后.lines().any(|line| line == 那一段),
            "就地那一框里没有「{那一段}」：\n{下钻之后}"
        );
    }
    assert!(
        下钻之后.contains(&format!("全部通过（{} 条）", thousands(batch.count))),
        "下钻把底下那一排也收窄了——它该始终作用于整批：\n{下钻之后}"
    );
    app.queue_and_site().0.drill_out();
    跑(&ctx, &mut app, 1);

    let 上一组 = app.queue().samples();
    let _ = 点正好那一颗(&ctx, &mut app, "换一组");
    assert_ne!(app.queue().samples(), 上一组, "按了「换一组」，样本没换");
}

#[test]
fn 逐条照稿两栏_左边待选列表_右边详情有候选卡片四颗按钮与键位提示() {
    // 设计稿 `.obo`（拿主意的人 2026-09-15 定）：左边待选列表，栏头写着看的是哪一批、几条、「筛选…」与排序；右边这一条的
    // 详情——第几条、文件名、路径，候选一张一张摆成卡片，四颗带键帽的按钮与一颗「手工指定…」，一框键位提示。原来那三块
    // （分组表、五列的表、底下那张表单）不在了。
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
    app.queue_and_site().0.show_one_by_one();
    跑(&ctx, &mut app, 2);
    let item = app
        .queue()
        .queue()
        .selected()
        .get(app.queue().at())
        .cloned()
        .expect("光标底下该有一条");
    let 共 = app.queue().queue().selected().len() as u64;

    let out = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 屏上 = 画出来的字(&out);
    for 那一段 in [
        多候选.shape.label(),
        format!("{} 条", thousands(共)),
        "筛选…".to_string(),
        format!("{} ▲", ItemOrder::Key.label()),
        format!("第 1 条 · 共 {} 条", thousands(共)),
        format!(
            "候选 {} 个 · 选择一个，或选择「都不对」",
            item.candidates.len()
        ),
        "通过所选候选".to_string(),
        "都不对".to_string(),
        "先放着".to_string(),
        "撤销上一条".to_string(),
        "手工指定…".to_string(),
        "切换候选".to_string(),
        "先放着（不保存，稍后仍会出现）".to_string(),
    ] {
        assert!(
            屏上.lines().any(|line| line == 那一段),
            "屏上没有「{那一段}」：\n{屏上}"
        );
    }
    // 候选卡片的标题只写作品名，中文身份另起一枚标签（照稿）。
    for candidate in &item.candidates {
        assert!(
            屏上.lines().any(|line| line == candidate.game),
            "候选卡片的标题不是光作品名：\n{屏上}"
        );
        if let Some(mark) = candidate.chinese {
            assert!(
                屏上.lines().any(|line| line == mark.label()),
                "中文身份那一枚标签没画：\n{屏上}"
            );
        }
    }
    for 键 in ["←", "→", "Y", "N", "空格", "U"] {
        assert!(
            屏上.lines().any(|line| line == 键),
            "键位提示里没有「{键}」：\n{屏上}"
        );
    }
    for candidate in &item.candidates {
        assert!(
            屏上.contains(candidate.game.as_str()),
            "候选卡片上没写作品：{}",
            candidate.game
        );
    }
    // 置信度只靠行首那一道色，行里不写那一档的词（照稿，挂单 `Q872`）；词在候选卡片的标签上。
    for tier in [Tier::High, Tier::Medium, Tier::Low] {
        assert!(
            !屏上
                .lines()
                .any(|line| line.ends_with(&format!(" · {}", tier.label()))),
            "待选列表行里还写着「{}」：\n{屏上}",
            tier.label()
        );
    }
    // 有候选的这一条不画结论那一枚（照稿）：有候选的一律是命中。
    assert!(
        !屏上.lines().any(|line| line == State::Matched.label()),
        "有候选的这一条还画着结论标签：\n{屏上}"
    );
    for 旧的 in ["从哪一批下手", "预览这一批", "← 回到分批"] {
        assert!(!屏上.contains(旧的), "原来那几块还在：「{旧的}」\n{屏上}");
    }

    // 点第二张候选卡片就选它（与 `→` 同一件事）。
    let 位置 = shared::那一段画在哪儿(&out, &item.candidates[1].game)
        .unwrap_or_else(|| panic!("屏上没有第二张候选卡片：\n{屏上}"));
    let _ = 按在(&ctx, &mut app, 位置);
    assert_eq!(app.queue().nth(), 1, "点了第二张候选卡片");

    // 点「先放着」走的与 `空格` 同一条路：一个字都不写库，光标往下走一条。
    let _ = 点正好那一颗(&ctx, &mut app, "先放着");
    assert_eq!(app.queue().at(), 1, "「先放着」没往下走一条");
    assert_eq!(app.site().store.counts().expect("读得出").total, 0);
}

#[test]
fn 手工指定与筛选各是一层弹层_开着时键盘不接() {
    // 拿主意的人 2026-09-15 定：手工指定那张表单走 `crate::dialog`；选择器与识别结论那几个勾收进待选列表栏头的「筛选…」。
    // 两层都是模态弹层，开着的时候逐条流的键盘一个都不接（`dialog::screen_has_keys`）。
    let ctx = headless::context();
    let mut app = 界面(2_000);
    逐条看整个队列(&ctx, &mut app);

    let 屏上 = 点正好那一颗(&ctx, &mut app, "手工指定…");
    for 那一段 in [
        "裁成",
        "作品",
        "汉化组",
        "只裁选中的这一条",
        "预览这一批",
        "取消",
    ] {
        assert!(
            屏上.lines().any(|line| line == 那一段),
            "「手工指定」那一层里没有「{那一段}」：\n{屏上}"
        );
    }
    按(&ctx, &mut app, egui::Key::N);
    assert!(app.queue().applied().is_none(), "弹层开着时按 N 落下了一批");
    let _ = 点正好那一颗(&ctx, &mut app, "取消");

    let 屏上 = 点正好那一颗(&ctx, &mut app, "筛选…");
    for 那一段 in [
        "按识别结论",
        "按目录",
        "按候选作品",
        "按命名规律",
        "清除筛选",
        "完成",
    ] {
        assert!(
            屏上.lines().any(|line| line == 那一段),
            "「筛选」那一层里没有「{那一段}」：\n{屏上}"
        );
    }
    let 那一组 = app
        .queue()
        .queue()
        .groups(Axis::Directory)
        .first()
        .cloned()
        .expect("该有目录分组");
    let 名 = if 那一组.label.is_empty() {
        "（主库根）".to_string()
    } else {
        那一组.label.clone()
    };
    let _ = 点正好那一颗(
        &ctx,
        &mut app,
        &format!("{}  {名}", thousands(那一组.count)),
    );
    assert_eq!(
        app.queue().queue().selected().len() as u64,
        那一组.count,
        "点一组就是一条选择器"
    );
    let _ = 点正好那一颗(&ctx, &mut app, "清除筛选");
    assert_eq!(
        app.queue().queue().selected().len(),
        2_000,
        "清除筛选回到整个队列"
    );
    let 屏上 = 点正好那一颗(&ctx, &mut app, "完成");
    assert!(
        !屏上.lines().any(|line| line == "清除筛选"),
        "按了「完成」那一层还开着：\n{屏上}"
    );
}

#[test]
fn 识别跑过而队列裁空时空态不给运行识别_一批能整批通过的都没有时头一格不写前零批() {
    use shared::档;

    let ctx = headless::context();
    let 画这一份 = |ctx: &egui::Context, 手上的: &[(&str, &str, 档)], 目录: &str| {
        let mut app = shared::小库(手上的, shared::干净工作目录(目录));
        app.show_view(View::Queue);
        跑(ctx, &mut app, 2);
        let 屏上 = 画一帧(ctx, &mut app);
        (app, 屏上)
    };
    let 说明 = "识别完成后，无法自动确定的结果会出现在这里，并按判定依据自动分批。";

    // 一、识别跑过、队列裁空、库里一个还没识别的都没有：卡上只有标题与说明，按下去什么都不会多出来的那颗不画。
    let (app, 屏上) = 画这一份(&ctx, &[("FC", "甲.zip", 档::命中)], "romcat-测试-队列-裁空");
    assert!(
        app.queue().queue().identified() && app.queue().queue().pending() == 0,
        "前提：识别跑过、队列是空的"
    );
    assert!(
        屏上.lines().any(|line| line == 说明),
        "空态卡没画：\n{屏上}"
    );
    for 不该有 in ["运行识别", "与「库」页面工序中的「识别」是同一个操作。"]
    {
        assert!(
            !屏上.lines().any(|line| line == 不该有),
            "库里没有还没识别的，却画了「{不该有}」：\n{屏上}"
        );
    }

    // 二、队列裁空、库里还有一个还没识别的：标题带着数，给那颗按钮。
    let (_, 屏上) = 画这一份(
        &ctx,
        &[("FC", "甲.zip", 档::命中), ("FC", "乙.zip", 档::还没识别)],
        "romcat-测试-队列-裁空还有没识别的",
    );
    for 该有 in ["暂无待确认项：1 个变体还没识别", "运行识别"] {
        assert!(
            屏上.lines().any(|line| line == 该有),
            "空态卡上没有「{该有}」：\n{屏上}"
        );
    }

    // 三、队列里只有没有候选的：头一格照稿留着，数是 0，说明不写「前 0 批」（库屏工序段这时一句都不说）。
    let (app, 屏上) = 画这一份(
        &ctx,
        &[
            ("FC", "甲.zip", 档::没有候选),
            ("FC", "乙.zip", 档::没有候选),
        ],
        "romcat-测试-队列-只有没有候选的",
    );
    assert_eq!(
        app.queue()
            .queue()
            .coverage(triage::HEADLINE_BATCHES)
            .head_batches,
        0,
        "前提：一批能整批通过的都没有"
    );
    let 行: Vec<&str> = 屏上.lines().collect();
    let at = 行
        .iter()
        .position(|line| *line == "可直接批量处理 · 每条只有一个候选")
        .unwrap_or_else(|| panic!("头一格的说明不对：\n{屏上}"));
    assert_eq!(
        at.checked_sub(1).map(|up| 行[up]),
        Some("0"),
        "头一格的数不对：\n{屏上}"
    );
    assert!(
        !屏上.lines().any(|line| line.starts_with("前 0 批")),
        "一批都没有还写「前 0 批」：\n{屏上}"
    );
}

/// 一份**手搭的**小现场：七个识别过的变体，各带一条没采纳的低置信候选、各撞一份不同的 DAT——七批都能整批通过。
///
/// 合成数据里能整批通过的正好是五批（与「前几批」一样多），摆不出「另有 N 批同样只有一个候选」那一句。
fn 有七批能整批通过的库() -> App {
    use romcat_core::catalog::Confidence;
    use romcat_core::catalog::identify::Identification;
    use romcat_core::platform::Manifest;
    use romcat_core::site::Site;
    use romcat_core::verdict::Store;

    let mut catalog = romcat_core::catalog::Catalog::open_in_memory().expect("开得出中立库");
    romcat_core::catalog::roots::add_root(
        &catalog,
        None,
        shared::根,
        std::path::Path::new(&format!("/{}", shared::根)),
    )
    .expect("建得出根");
    let variants: Vec<_> = (0..7)
        .map(|at| shared::变体("GBA", &format!("第{at}部.gba")))
        .collect();
    catalog
        .replace_variants(&variants, 1, &Manifest::default())
        .expect("写得进变体");
    let 结论: Vec<Identification> = variants
        .iter()
        .enumerate()
        .map(|(at, variant)| {
            let mut 候选 = shared::候选(variant, false, Confidence::Low);
            候选.dat = format!("第{at}份.dat");
            Identification {
                variant_key: variant.key.clone(),
                platform: None,
                standalone: None,
                edition: None,
                state: State::Unmatched,
                reason: None,
                units: 1,
                nkit: 0,
                read_bytes: 0,
                work_id: None,
                release_id: None,
                candidates: vec![候选],
            }
        })
        .collect();
    catalog.write_identifications(&结论).expect("写得进结论");
    let store = Store::in_memory().expect("开得出沉淀库");
    App::new(
        Site::in_memory(catalog, store, shared::根),
        shared::干净工作目录("romcat-测试-队列-七批"),
    )
}

#[test]
fn 能整批通过的多过前几批时先摆前几批_另有那一句_按列出这几批就全摆出来() {
    let ctx = headless::context();
    let mut app = 有七批能整批通过的库();
    跑(&ctx, &mut app, 2);
    let 账 = app.queue().queue().coverage(triage::HEADLINE_BATCHES);
    assert_eq!(
        (
            账.head_batches,
            账.rest_answerable_batches,
            账.rest_answerable
        ),
        (5, 2, 2),
        "前提：七批里前五批之外还有两批能整批通过"
    );
    let 各批的 = |batch: &triage::Batch| match &batch.shape {
        Shape::Candidates { dat, .. } => dat.clone(),
        Shape::Bare { .. } => panic!("这七批都该有候选"),
    };
    let dats: Vec<String> = app.queue().queue().batches().iter().map(各批的).collect();

    let 屏上 = 正文滚一趟(&ctx, &mut app);
    for (at, dat) in dats.iter().enumerate() {
        assert_eq!(
            屏上.lines().any(|line| line == dat),
            at < 5,
            "第 {at} 批（{dat}）该不该先摆出来：\n{屏上}"
        );
    }
    assert!(
        屏上
            .lines()
            .any(|line| line == "另有 2 批（2 条）同样只有一个候选。"),
        "没写另有几批：\n{屏上}"
    );

    let _ = 点正好那一颗(&ctx, &mut app, "列出这 2 批");
    let 屏上 = 正文滚一趟(&ctx, &mut app);
    for dat in &dats {
        assert!(
            屏上.lines().any(|line| line == dat),
            "按了「列出这 2 批」，{dat} 那一批还没摆出来：\n{屏上}"
        );
    }
    assert!(
        !屏上.contains("同样只有一个候选。"),
        "全摆出来了还写着另有几批：\n{屏上}"
    );
}

#[test]
fn 落下一批之后屏底提示条说已通过几条_按撤销就撤掉那一批_撤完带放回() {
    // 设计稿 `passBatch` 与 `undoLot`：落下一批之后屏底一条「已通过 N 条」带「撤销」，撤完一条「已撤销，N 个变体回到
    // 待确认队列」（拿主意的人 2026-09-15 定，原来正文顶上那两行回执删了）。两颗按钮走的是裁决记录抽屉里那两颗同一条
    // 核心库入口（`Screen::undo` / `Screen::redo`）。
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
        .expect("该有一批能整批通过");
    {
        let (screen, site) = app.queue_and_site();
        screen.pass(site, &Scope::whole(batch.shape.clone()));
        screen.commit(site);
    }
    let 那一批 = app.queue().applied().expect("落下了就该有账").batch;
    // 提示条是一块 egui 浮层：头一帧只量大小不画（egui 的 `Area` 头一帧看不见），第二帧才画出来。
    跑(&ctx, &mut app, 1);
    let 屏上 = 画一帧(&ctx, &mut app);
    let 那一句 = format!("已通过 {} 条", thousands(batch.count));
    assert!(
        屏上.lines().any(|line| line == 那一句),
        "屏底没有「{那一句}」：\n{屏上}"
    );
    for 旧的 in ["裁决已沉淀", "撤回第"] {
        assert!(!屏上.contains(旧的), "正文顶上那两行回执还在：\n{屏上}");
    }

    let 屏上 = 点正好那一颗(&ctx, &mut app, "撤销");
    assert!(
        册子上的(&app, 那一批).undone(),
        "按了提示条上的「撤销」，那一批没撤：\n{屏上}"
    );
    assert_eq!(
        app.queue().queue().pending(),
        原有,
        "撤完那些变体该回到队列里"
    );
    let 那一句 = format!("已撤销，{} 个变体回到待确认队列", thousands(batch.count));
    assert!(
        屏上.lines().any(|line| line == 那一句),
        "撤完屏底没有「{那一句}」：\n{屏上}"
    );

    let 屏上 = 点正好那一颗(&ctx, &mut app, "放回");
    assert!(
        !册子上的(&app, 那一批).undone(),
        "按了提示条上的「放回」，那一批还撤着：\n{屏上}"
    );
    assert_eq!(app.queue().queue().pending(), 原有 - batch.count);
    assert!(
        屏上
            .lines()
            .any(|line| line == format!("已放回 {} 条", thousands(batch.count))),
        "放回之后屏底没有那一句：\n{屏上}"
    );
}
