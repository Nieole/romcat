//! **待确认队列**是主界面：列得出、盖得住一批、裁得下去，而中文输入不在表格单元格里。
//!
//! 合成数据的**形状照真机来**（票 08 实测：队列 16,656 条，
//! `--under 合成库/gba/【全部汉化】` 852、`--under 合成库/GoodNES3.1` 1,543、`--name 汉化` 1,986）。
//! 前缀带着**根名**：键的第一段就是它（`path::library_key`）。
//! 这几个数在这里是**断言**而不是注释——ADR-0002 说批量裁决的覆盖面是这件事成不成立的
//! 分界，界面上点一行选中多少，就该是报告上印的那个数。

use egui::widgets::text_edit::TextEditState;
use romcat_core::catalog::State;
use romcat_core::triage::{Axis, Draft, Overrides};
use romcat_gui::app::{App, View};
use romcat_gui::table::ROW_HEIGHT;
use romcat_gui::{demo, headless};

fn 界面(rows: u64) -> App {
    let site = demo::site(demo::queue(rows).expect("造得出合成数据")).expect("开得出现场");
    App::new(site, demo::workspace())
}

/// 跑几帧，返回这个上下文。
fn 跑(ctx: &egui::Context, app: &mut App, frames: u32) {
    for _ in 0..frames {
        headless::frame(ctx, headless::input(), |ui| app.ui(ui));
    }
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
