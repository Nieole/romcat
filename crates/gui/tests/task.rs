//! **任务**那一屏：长活跑在画帧那条线程之外，屏上看得见、按得停、留得下历史。
//!
//! 这几条都是「不这么做会出事」而不是「这样比较好看」：
//!
//! - **任务跑着的时候别的屏照常用**。跑在画帧那条线程上的话，窗口就是一块白板——
//!   期间切不了屏、滚不动列表、连「停下」都点不着（挂账 D156 说的正是这件事）。
//! - **按停下之后记成「按停了」，不是「失败」**。两者长得一样的话，用户会去找哪儿坏了。
//! - **失败不静默结束**：哪一步、为什么，两样都得说出来。
//! - **历史各自带耗时**——「下次大概要多久」是这一屏唯一回答得了的问题。
//!
//! 验的是**状态转换**，不是像素（那是 ADR-0005 给这一层定的验收面）。

use std::time::Duration;

use romcat_core::task::Ending;
use romcat_gui::app::{App, View};
use romcat_gui::{demo, headless};

/// 一个装着合成数据的界面。
fn 开一个() -> App {
    App::new(
        demo::site(demo::synthetic(200).expect("造得出合成数据")).expect("开得出现场"),
        demo::workspace(),
    )
}

fn 跑一帧(ctx: &egui::Context, app: &mut App) {
    headless::frame(ctx, headless::input(), |ui| app.ui(ui));
}

/// 跑一帧，返回它请求的**下一次重画**要等多久。`ZERO` 就是「立刻再画一帧」。
fn 重画间隔(ctx: &egui::Context, app: &mut App) -> Duration {
    headless::frame(ctx, headless::input(), |ui| app.ui(ui))
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .map_or(Duration::MAX, |viewport| viewport.repaint_delay)
}

/// 排一趟**会跑一会儿**的活上去：每 5 毫秒看一眼有没有被叫停，最多两秒。
///
/// 它跑完也不交出产物（`Err`）——这一趟存在的意义只是**占着那个位子**，
/// 好让别的屏在它跑着的时候照样画。真的活长什么样看 `tests/sublibrary.rs`。
/// 一趟**跑不完**的活：用它的三条测试都自己按「停下」，没有一条等它自然结束。
///
/// **步数要给得足够多，多到它绝不可能在测试看完之前自己跑完。** 早先是 400 步 × 5 ms
/// ＝ 正好两秒，而 `任务跑着的时候别的屏照常画得出来` 要在三十帧之内看见它还在跑——
/// 机器一忙（全量测试并排跑、内存吃紧）三十帧就超过两秒，那一趟活自己先结束了，
/// 测试于是在 `running().expect(…)` 上炸，而它想钉的那件事根本没出问题。
/// **单独跑绿、全量跑挂**的测试比没有测试更坏：它会把后面每一张票的门禁都染成红的。
const 占位步数: u32 = 40_000;

fn 排一趟占位的(app: &mut App) -> u64 {
    app.tasks_mut().queue("装作在扫一趟库", |task| {
        task.steps(占位步数);
        for at in 1..=占位步数 {
            task.step(&format!("走到第 {at} 块"))?;
            std::thread::sleep(Duration::from_millis(5));
        }
        Err("这一趟本来就只是占着位子".to_string())
    })
}

/// 一直画帧，直到台上空了。
fn 画到台上空了(ctx: &egui::Context, app: &mut App) {
    for _ in 0..600 {
        跑一帧(ctx, app);
        if !app.tasks().busy() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("六秒了台上还有活");
}

#[test]
fn 任务跑着的时候别的屏照常画得出来() {
    let ctx = headless::context();
    let mut app = 开一个();
    app.show_view(View::Variants);
    let id = 排一趟占位的(&mut app);

    // **这一趟活要跑两秒，而它在画帧那条线程上的话，`queue` 那一下就整整两秒地跑完了
    // ——一帧都轮不上。** 所以「三十帧画完之后它还在跑」本身就是那句「不在这条线程上」。
    跑一帧(&ctx, &mut app);
    let 头一帧那会儿 = app.tasks().running().expect("那一趟活不见了").progress.at;
    for _ in 0..30 {
        跑一帧(&ctx, &mut app);
    }
    let live = app.tasks().running().expect("三十帧之后那一趟活不见了");
    assert_eq!(live.id, id);
    // **画着帧的同时它也在往前走**：两件事真的在同时发生，不是排着队轮流来。
    assert!(
        live.progress.at > 头一帧那会儿,
        "画了三十帧，那趟活还停在第 {头一帧那会儿} 步——它没在跑",
    );

    // 库浏览那一屏照常答得上话：这一帧要来的行还在。
    assert!(app.window().retained() > 0, "任务跑着的时候库浏览空了");

    let 按下那一刻 = std::time::Instant::now();
    app.tasks_mut().stop(id);
    画到台上空了(&ctx, &mut app);
    let 按下之后过了 = 按下那一刻.elapsed();
    // **按停了就记成按停了，不是失败。** 这一条本身就说明它没跑到头——跑到头会记成
    // `Failed`（那个占位闭包最后返回的是 `Err`）。
    assert_eq!(app.tasks().history()[0].ending, Ending::Stopped);
    assert!(
        app.tasks().history()[0].elapsed > Duration::ZERO,
        "历史那条没带耗时",
    );
    // **「停得动」量的是按下之后多久真的停**，不是那一趟活总共跑了多久。
    // 早先拿「总耗时 < 2 秒」当判据，可那个数里绝大部分是**这条测试自己画三十一帧
    // 花掉的时间**——机器一忙就超过两秒，于是一条好好的实现被判成「停不动」。
    assert!(
        按下之后过了 < Duration::from_secs(3),
        "按了停下，过了 {按下之后过了:?} 台上还没空——那不叫停得动",
    );
}

#[test]
fn 任务屏三种样子都画得出来() {
    // 空台、跑着的、留着历史的——三种都得画得出来。这一屏是整份规格的基础设施，
    // 它自己画崩了的话，别的屏的长活就没有落点了。
    let ctx = headless::context();
    let mut app = 开一个();
    app.show_view(View::Tasks);
    跑一帧(&ctx, &mut app);
    assert!(app.tasks().history().is_empty());

    let id = 排一趟占位的(&mut app);
    跑一帧(&ctx, &mut app);
    assert!(app.tasks().running().is_some());

    app.tasks_mut().stop(id);
    画到台上空了(&ctx, &mut app);
    跑一帧(&ctx, &mut app);
    assert_eq!(app.tasks().history().len(), 1);

    app.tasks_mut().clear_history();
    跑一帧(&ctx, &mut app);
    assert!(app.tasks().history().is_empty());
}

#[test]
fn 台上有活的每一帧都请求下一帧() {
    // 不请求的话进度条不走、已用时间不涨、**「停下」按钮按不动**——窗口看着就是死的。
    // egui 只在有输入事件时才自己画下一帧，所以这一句得由我们主动说。
    let ctx = headless::context();
    let mut app = 开一个();
    app.show_view(View::Tasks);
    let id = 排一趟占位的(&mut app);
    assert_eq!(
        重画间隔(&ctx, &mut app),
        Duration::ZERO,
        "台上有活，这一帧却没请求下一帧",
    );

    app.tasks_mut().stop(id);
    画到台上空了(&ctx, &mut app);
}

#[test]
fn 失败的那一趟在历史里说得出哪一步为什么() {
    // **不静默结束**：一趟活没做成，界面上得说得出它停在哪一步、为什么。
    let ctx = headless::context();
    let mut app = 开一个();
    app.show_view(View::Tasks);
    app.tasks_mut().queue("排差量预览 · 掌机", |task| {
        task.steps(12);
        task.step("读子库")?;
        task.step("看一眼目标")?;
        Err("卡不在位：/Volumes/掌机".to_string())
    });
    画到台上空了(&ctx, &mut app);

    let record = &app.tasks().history()[0];
    let Ending::Failed { step, why } = &record.ending else {
        panic!("该记成失败：{:?}", record.ending);
    };
    assert_eq!(step, "看一眼目标");
    assert!(why.contains("卡不在位"), "{why}");
    assert!(
        record.ending.render().contains("看一眼目标"),
        "画出来的那句话没说是哪一步：{}",
        record.ending.render(),
    );
}
