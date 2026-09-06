//! **待确认队列**是主界面：列得出、盖得住一批、裁得下去，而中文输入不在表格单元格里。
//!
//! 合成数据的**形状照真机来**（票 08 实测：队列 16,656 条，
//! `--under 合成库/gba/【全部汉化】` 852、`--under 合成库/GoodNES3.1` 1,543、`--name 汉化` 1,986）。
//! 前缀带着**根名**：键的第一段就是它（`path::library_key`）。
//! 这几个数在这里是**断言**而不是注释——ADR-0002 说批量裁决的覆盖面是这件事成不成立的
//! 分界，界面上点一行选中多少，就该是报告上印的那个数。

use egui::widgets::text_edit::TextEditState;
use romcat_core::catalog::State;
use romcat_core::triage::{Axis, Draft, Overrides, Scope};
use romcat_gui::app::{App, View};
use romcat_gui::queue::Mode;
use romcat_gui::table::ROW_HEIGHT;
use romcat_gui::{demo, headless};

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

/// 这一帧**真的画在屏上**的那些字。
///
/// 断言「屏上看得见文件名」只有看这个才算数：查队列里有没有这条数据是恒真的废话，
/// 而这一屏要证的正是那几样摆出来了没有。egui 每画一段文字就留下一个 `Galley`，
/// 它带着原文。
fn 画出来的字(output: &egui::FullOutput) -> String {
    fn 收(shape: &egui::epaint::Shape, out: &mut String) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                out.push_str(text.galley.text());
                out.push('\n');
            }
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    收(one, out);
                }
            }
            _ => {}
        }
    }
    let mut out = String::new();
    for clipped in &output.shapes {
        收(&clipped.shape, &mut out);
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

/// 一份**手搭的**中立库，形状照排查报告里那份现场来：3 个变体，2 个跑过识别
/// （1 个命中、自动通过，1 个未命中、带一条低置信候选进队列），**1 个连识别都还没跑过**。
///
/// 合成数据造不出第三种——`demo::queue` 给每个变体都写了一行结论。而这一屏最该说清的
/// 正是它：同一份库，命令行 `triage list` 印「另有 1 个变体连识别都还没跑过」。
fn 有一个连识别都没跑过的库() -> App {
    use romcat_core::catalog::identify::Identification;
    use romcat_core::catalog::{Candidate, Catalog, Confidence, State};
    use romcat_core::dat::Convention;
    use romcat_core::platform::Manifest;
    use romcat_core::shape::{Role, SINGLE_FILE_RULE, Variant};
    use romcat_core::site::Site;
    use romcat_core::verdict::Store;

    let catalog = Catalog::open_in_memory().expect("开得出中立库");
    let _ = romcat_core::catalog::roots::add_root(
        &catalog,
        None,
        "主库",
        std::path::Path::new("/主库"),
    );
    let 变体 = |name: &str, platform: &str| {
        let key = format!("主库/{platform}/{name}");
        Variant {
            main_key: key.clone(),
            platform: Some(platform.to_string()),
            rule: SINGLE_FILE_RULE.to_string(),
            manual: false,
            files: 1,
            bytes: 4096,
            unreadable_files: 0,
            members: vec![(key.clone(), Role::Main)],
            key,
        }
    };
    let mut catalog = catalog;
    let variants = vec![
        变体("命中.zip", "SFC"),
        变体("未命中.zip", "FC"),
        变体("还没轮到它.zip", "GB"),
    ];
    catalog
        .replace_variants(&variants, 1, &Manifest::default())
        .expect("写得进变体");
    let 候选 = |accepted: bool, confidence| Candidate {
        member_key: String::new(),
        inner: String::new(),
        confidence,
        accepted,
        source: "合成".to_string(),
        dat: "合成.dat".to_string(),
        platform: "SFC".to_string(),
        game: "幻想传说 (Japan)".to_string(),
        rom: "rom.bin".to_string(),
        hashed_as: Convention::AsIs,
        dat_convention: Convention::AsIs,
        evidence: "精确哈希命中".to_string(),
        chinese: None,
        serial: None,
        release_id: None,
    };
    catalog
        .write_identifications(&[
            Identification {
                variant_key: variants[0].key.clone(),
                state: State::Matched,
                reason: None,
                units: 1,
                nkit: 0,
                read_bytes: 0,
                work_id: None,
                release_id: None,
                candidates: vec![候选(true, Confidence::High)],
            },
            Identification {
                variant_key: variants[1].key.clone(),
                state: State::Unmatched,
                reason: None,
                units: 1,
                nkit: 0,
                read_bytes: 0,
                work_id: None,
                release_id: None,
                candidates: vec![候选(false, Confidence::Low)],
            },
            // 第三个变体**一行都不写**——那就是「还没识别」。
        ])
        .expect("写得进结论");
    let store = Store::in_memory().expect("开得出沉淀库");
    App::new(Site::in_memory(catalog, store, "主库"), demo::workspace())
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
    // **两句话不许长得一样**：底下四档里「一条候选都没有」那一档曾经也叫「还没识别」
    // （挂单 Q84），于是屏上会同时出现两个「还没识别」，一个说 1、一个说 0。
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
    assert!(
        错.contains("romcat triage undo"),
        "更早那些批得指条路出去：{错}",
    );
    assert!(app.queue().undone().is_none(), "什么都没撤，账上不许多一笔");
}
