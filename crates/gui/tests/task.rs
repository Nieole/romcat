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

use romcat_core::collection::CollectionError;
use romcat_core::task::{Cutoff, Ending, Halted, Handle, Live};
use romcat_gui::app::{App, View};
use romcat_gui::task::Product;
use romcat_gui::{demo, headless};

mod shared;
use shared::{一对信号, 占位活, 画出来的字};

/// 这几条测试自己的**工作目录**。
///
/// **不共用 `demo::workspace()`**：那是 `--demo` 那个演示窗口用的目录，而窗口会往里写
/// **版式偏好**（面板拖到哪儿）。共用的话，维护者开一次演示窗口把某块面板拖高一截，
/// 下一次跑这几条测试屏上就少了几行——而断言数的正好是行数。
fn 工作目录() -> std::path::PathBuf {
    std::env::temp_dir().join("romcat-测试-任务")
}

/// 这一屏用多少条合成数据。**故意不推到真库量级**（票 `parking-3/17`，挂单 `Q356`）。
///
/// 这几条钉的是任务本身：跑着的时候别的屏画不画得出来、按停记成什么、失败说不说得清、
/// 历史带不带耗时——没有一条跟着库有多大变。而 `任务跑着的时候别的屏照常画得出来`
/// 要在**三十帧之内**看见那一趟活还在跑：库一大，那三十帧就画得更久，同一条测试离
/// 那道时限更近一步。推到四万行不多验一件事，只把这一条推向挂钟彩票那一侧
/// （挂单 `Q196` / `Q349` 说的正是那种测试）。
const ROWS: u64 = 200;

/// 一个装着合成数据的界面。
fn 开一个() -> App {
    App::new(
        demo::site(demo::synthetic(ROWS).expect("造得出合成数据")).expect("开得出现场"),
        工作目录(),
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

/// 一档要验的进度：一个说得出口的名字，加一句往把手上报进度的话。
///
/// 摆成一个别名而不是就地写全，是因为「算不出还剩多久」有**两条不同的路**通向同一句
/// 「没有」，两档得并排验（见那条测试）——而并排就得把它们装进同一个 `Vec`。
type 一档进度 = (&'static str, Box<dyn FnOnce(&Handle) + Send>);

/// 排一趟活上去，让它**报完这点进度就停在那儿等着被叫停**。
///
/// 「屏上这一帧写着什么」那一类断言要的正是它：进度报到哪儿由 `报进度` 说了算，
/// 报完就不动了——于是测试看到的每一帧都是同一个数，不靠「睡够多久它大概走到第几步」。
/// 与共享夹具里的 `占位活` 的分工：那一趟是**占着位子**用的（一步都不走，等信号收场），
/// 这一趟是**摆一个确定的进度**用的。
fn 排一趟停在原地的(
    app: &mut App, 报进度: impl FnOnce(&Handle) + Send + 'static
) -> u64 {
    app.tasks_mut().queue("装作在扫一趟库", move |task| {
        报进度(task);
        loop {
            // 收到「停下」就带着 `Halted` 退出去——于是它记成「按停了」，不是「失败」。
            task.check()?;
            std::thread::sleep(Duration::from_millis(2));
        }
    })
}

/// 一直画帧，直到台上那一趟报出了要等的那个进度，交出那一刻的样子。
///
/// **不靠「睡三十毫秒它总该走到了」**：机器一忙那条线程可能一步都还没排上，
/// 那样这条测试就成了挂钟彩票（挂单 `Q196` / `Q349` 说的正是那种）。
/// 六百轮 × 10 毫秒 ＝ **最多等六秒**，与 [`画到台上空了`] 同一个数。
fn 等到那一趟报出(
    ctx: &egui::Context, app: &mut App, 报出: impl Fn(&Live) -> bool
) -> Live {
    for _ in 0..600 {
        跑一帧(ctx, app);
        if let Some(live) = app.tasks().running()
            && 报出(&live)
        {
            return live;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("六秒了台上那一趟还没报出要等的那个进度");
}

#[test]
fn 任务跑着的时候别的屏照常画得出来() {
    let ctx = headless::context();
    let mut app = 开一个();
    app.show_view(View::Browse);
    // 这一趟活**走一步就停下来等信号**：往下走、收场，都由这条测试发信号说了算，不靠睡够多久。
    let (走下一步, 等走下一步) = 一对信号();
    let 占位 = 占位活::照这样排上(
        app.tasks_mut(),
        "装作在扫一趟库",
        move |task, 等收场| {
            task.steps(2);
            task.step("走到第 1 块")?;
            等走下一步.等();
            task.step("走到第 2 块")?;
            等收场.等();
            task.check()?;
            Err(Cutoff::failed("这一趟本来就只是占着位子"))
        },
    );

    // **它在画帧那条线程上的话，台上就留不住它**：占位活发现自己跑在排它的那条线程上，
    // 当场交一句失败（等下去会把这条线程堵死）。所以「三十帧画完之后它还在台上」本身
    // 就是那句「不在这条线程上」。
    等到那一趟报出(&ctx, &mut app, |live| live.progress.at == 1);
    for _ in 0..30 {
        跑一帧(&ctx, &mut app);
    }
    let live = app.tasks().running().expect("三十帧之后那一趟活不见了");
    assert_eq!(live.id, 占位.id());
    assert_eq!(live.progress.at, 1, "没发信号，那趟活自己往前走了");
    // **画着帧的同时它也在往前走**：信号一发，它在自己那条线程上走下一步，这边一帧一帧
    // 画着就看得见——两件事真的在同时发生，不是排着队轮流来。
    走下一步.发();
    等到那一趟报出(&ctx, &mut app, |live| live.progress.at == 2);

    // 库浏览那一屏照常答得上话：这一帧要来的行还在。
    assert!(app.window().retained() > 0, "任务跑着的时候库浏览空了");

    let 按下那一刻 = std::time::Instant::now();
    占位.按停(app.tasks_mut());
    画到台上空了(&ctx, &mut app);
    let 按下之后过了 = 按下那一刻.elapsed();
    // **按停过的就记成已取消，不是失败。** 这一条本身就说明它是被按停收的场——只放行不按停
    // 会记成 `Failed`（那个占位闭包最后返回的是 `Err`）。
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

    let 占位 = 占位活::排上(app.tasks_mut(), "装作在扫一趟库");
    跑一帧(&ctx, &mut app);
    assert!(app.tasks().running().is_some());

    占位.按停(app.tasks_mut());
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
    let 占位 = 占位活::排上(app.tasks_mut(), "装作在扫一趟库");
    assert_eq!(
        重画间隔(&ctx, &mut app),
        Duration::ZERO,
        "台上有活，这一帧却没请求下一帧",
    );

    占位.按停(app.tasks_mut());
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
        Err(Cutoff::failed("卡不在位：/Volumes/掌机"))
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

#[test]
fn 按下停下之后任务屏历史写的是停了那一类_不是失败() {
    // 维护者在界面上按一下「停下」，那一趟就得显示成**停了**——不然他会以为自己按坏了
    // 什么，去找哪儿出了错。
    //
    // 从前这一档靠**那句话正好是核心库 `Halted` 交出来的那一句**分。于是界面上任何一处
    // 措辞与它差着字，按停就悄悄变成了失败——刮削那几处手写的「按停了」三个字正是
    // 这么掉进去的（挂单 `Q151`）。**这一条要能在判据改回字符串比对时当场红**，
    // 所以它交上来的那句话故意与核心库那一句不一样：整批收藏那条路交的正是这一句。
    let ctx = headless::context();
    let mut app = 开一个();
    app.show_view(View::Tasks);

    let 差着字的那一句 = CollectionError::from(Halted).to_string();
    assert_ne!(
        差着字的那一句,
        Halted.to_string(),
        "两句话一样的话，这一条就什么都没验",
    );

    let 占位 = 占位活::照这样排上(
        app.tasks_mut(),
        "放进「收藏」· 200 个变体",
        |task, 等着| {
            task.steps(1);
            task.step("为 200 个变体折锚")?;
            等着.等();
            // 整批收藏那条路原样：核心库把「被按停」折进自己那个错误枚举再交上来
            // （`collection::plan` 里那一段一样是 `check`）。
            task.check().map_err(CollectionError::from)?;
            Ok(一份产物())
        },
    );
    let id = 占位.id();
    // **等它走过那一步再按**：按早了，它在 `step` 上就直接交出 `Halted`，
    // 折进领域错误的那条路一次都没走到，这一条就绿得什么都没验。
    等到那一趟报出(&ctx, &mut app, |live| live.progress.at == 1);
    // 界面上那颗「停下」按下去走的就是它（`按停` 先走它，再叫醒那一趟）。
    占位.按停(app.tasks_mut());
    画到台上空了(&ctx, &mut app);

    let record = &app.tasks().history()[0];
    assert_eq!(record.id, id, "历史头一条不是刚按停的那一趟");
    assert_eq!(
        record.ending,
        Ending::Stopped,
        "按了停下，历史那一行却写成了「{}」——判据又回到那句话上了",
        record.ending.render(),
    );

    // 屏上那一行也得是「停了」那一类。
    let 屏上 = 一帧的字(&ctx, &mut app);
    assert!(
        屏上.lines().any(|line| line.trim() == "已取消"),
        "任务屏历史那一行不是「停了」那一类：\n{屏上}",
    );
    assert!(
        !屏上.lines().any(|line| line.contains("失败")),
        "按停的那一趟在屏上说成了失败：\n{屏上}",
    );
}

/// 一份**空**的容量账，当那种「有产物」的活的产物用。
///
/// 这几条钉的是**收场怎么画**，不是产物里装了什么——拿最不占地方的那一支来占位。
fn 一份产物() -> Product {
    Product::Evaluated(Box::default())
}

/// 跑一帧，交出这一帧真画在屏上的字。
fn 一帧的字(ctx: &egui::Context, app: &mut App) -> String {
    画出来的字(&headless::frame(ctx, headless::input(), |ui| app.ui(ui)))
}

#[test]
fn 四种收场在任务屏历史里各画各的话() {
    // 一条轴四档：**完成 / 已取消 / 部分完成 / 失败**。谁都不许长得跟谁一样——
    // 撞脸的那两档里，那句「跑了 X 秒」就成了骗人的话。
    let ctx = headless::context();
    let mut app = 开一个();
    app.show_view(View::Tasks);

    app.tasks_mut().queue("算一遍容量", |_| Ok(一份产物()));
    // 整条只读的活被按停：什么都没留下，可以当没跑过。
    app.tasks_mut().queue("排差量预览 · 掌机", |task| {
        task.stop();
        task.step("读选择集")?;
        Ok(一份产物())
    });
    // 写过东西的活被按停：产物照旧交出来，且明说它是半截的。
    app.tasks_mut().queue("同步 · 掌机", |task| {
        task.steps(3);
        task.step("新增 SFC/幻想传说 汉化版.zip")?;
        task.stop();
        task.halfway("按停时落了 1 件，清单记着到这儿为止目标上真实有什么");
        Ok(一份产物())
    });
    app.tasks_mut().queue("扫描 · 主库", |task| {
        task.steps(3);
        task.step("认根")?;
        Err(Cutoff::failed("卡不在位"))
    });
    画到台上空了(&ctx, &mut app);

    let 屏上 = 一帧的字(&ctx, &mut app);
    for 那一句 in [
        "完成",
        "已取消",
        "部分完成：按停时落了 1 件，清单记着到这儿为止目标上真实有什么",
        "失败：在「认根」这一步，卡不在位",
    ] {
        assert!(
            屏上.lines().any(|line| line.trim() == 那一句),
            "屏上没有这一行「{那一句}」：\n{屏上}",
        );
    }
    // **「部分完成」不是「完成」的一种写法**：那一行说得出留下了什么。
    assert_eq!(
        屏上.lines().filter(|line| line.trim() == "完成").count(),
        1,
        "四趟活里只有一趟是真跑完的：\n{屏上}",
    );
}

#[test]
fn 算不出还剩多久的那一帧屏上一个剩余时间的字都没有() {
    // 这张票的全部价值在这一条：**工具算不出来的时候一个字都不画**。
    // 一个会跳的「约剩」比没有「约剩」更坏——维护者会照它安排接下来一小时干什么。
    //
    // **两档都得验，因为它们是两条不同的路。** 只验一档的话，另一档哪天开始画出个
    // 兜底值来，这一屏一条都不响：
    //
    // 1. **总步数还没报**——`fraction` 本身是「没有」；
    // 2. **报了总步数，可走了零成**——`fraction` 说得出，答的是 `Some(0.0)`，而已用时间
    //    除以零成折不出一个数。这一档才是真机上最常撞的那一个：`sources::refetch`
    //    取 DAT 与取 Switch 那两趟一共就一步、一次 `tick` 都不叫，**整趟下载**都在这儿。
    let 两档: Vec<一档进度> = vec![
        (
            "总步数还没报",
            Box::new(|task: &Handle| {
                task.step("取 DAT").expect("没人叫停");
            }),
        ),
        (
            "报了总步数，可走了零成",
            Box::new(|task: &Handle| {
                task.steps(1);
                task.step("取 DAT").expect("没人叫停");
            }),
        ),
    ];

    for (哪一档, 报进度) in 两档 {
        let ctx = headless::context();
        let mut app = 开一个();
        app.show_view(View::Tasks);
        let id = 排一趟停在原地的(&mut app, 报进度);
        let live = 等到那一趟报出(&ctx, &mut app, |live| live.progress.at == 1);
        // **这一句在这儿是防恒绿的那道守卫**：夹具哪天不再造出「算不出还剩多久」
        // 那一档，先炸的是它，而不是让底下那条负面断言静静地变成恒真。
        assert!(
            live.remaining().is_none(),
            "「{哪一档}」这个夹具算得出还剩多久，底下那条断言就是恒真的",
        );

        let 屏上 = 一帧的字(&ctx, &mut app);
        // 「已用」照旧在——**少画的只有算不出来的那一个数**，不是整行都不画了。
        // 少了这一句的话，整行没画出来也能让底下那条负面断言过。
        assert!(
            屏上.lines().any(|line| line.trim().starts_with("已用")),
            "「{哪一档}」：算不出还剩多久，连「已用」都不画了：\n{屏上}",
        );
        assert!(
            !屏上.contains("剩"),
            "「{哪一档}」：算不出来却还是画了个剩余时间出来：\n{屏上}",
        );

        app.tasks_mut().stop(id);
        画到台上空了(&ctx, &mut app);
    }
}

#[test]
fn 走了一半时屏上那一句写着约剩多少() {
    // 「已用 X」与「约剩 X」两个数并排。口径：已用时间 ÷ 已完成比例 − 已用时间。
    let ctx = headless::context();
    let mut app = 开一个();
    app.show_view(View::Tasks);
    let id = 排一趟停在原地的(&mut app, |task| {
        task.steps(2);
        task.step("认根").expect("没人叫停");
        task.step("挨个文件过一遍").expect("没人叫停");
    });
    let live = 等到那一趟报出(&ctx, &mut app, |live| live.progress.at == 2);
    // 两步走完了一步：**这一档真的说得出走了几成**，与上一条那一档不是同一件事。
    assert_eq!(live.progress.fraction(), Some(0.5));
    assert!(live.remaining().is_some());

    let 屏上 = 一帧的字(&ctx, &mut app);
    let 那一段 = |前缀: &'static str| -> String {
        屏上
            .lines()
            .find_map(|line| line.trim().strip_prefix(前缀))
            .unwrap_or_else(|| panic!("屏上没有「{前缀}」那一段：\n{屏上}"))
            .to_string()
    };
    // **两个数并排**：「已用」没被挤掉，「约剩」也写出来了。
    let 已用 = 那一段("已用 ");
    let 约剩 = 那一段("约剩 ");
    // **数也得对得上，不能只是「有这么一句」。** 走了一半时口径自己说了算：
    // 已用 ÷ 0.5 − 已用 = 已用——于是屏上这两个数必然印成同一串字。这一条不看挂钟
    // （两句话取的是同一帧那一份 `Live`），却钉住了界面画的确实是核心折出来的那个数：
    // 换成一个常数、或者除错了倍数，它当场红。
    assert_eq!(
        约剩, 已用,
        "走了一半时「约剩」该与「已用」是同一个数（已用 ÷ 0.5 − 已用 = 已用）：\n{屏上}",
    );

    app.tasks_mut().stop(id);
    画到台上空了(&ctx, &mut app);
}

#[test]
fn 占位活没收到信号就一直占着台子_按停之后记成已取消() {
    // 共享夹具里的 `占位活` 是按停那类界面测试占台子用的。它得钉住：**没收到信号就一直
    // 占着**（排在它后面的那一趟一直排着）、**一步都不自己走**（不数步数、不睡够多久就收场），
    // **按停之后记成「已取消」**。
    let ctx = headless::context();
    let mut app = 开一个();
    app.show_view(View::Tasks);
    let 占位 = 占位活::排上(app.tasks_mut(), "装作在扫一趟库");
    let 排在后面的 = app.tasks_mut().queue("排在它后面", |_| Ok(一份产物()));

    for _ in 0..30 {
        跑一帧(&ctx, &mut app);
    }
    let live = app
        .tasks()
        .running()
        .expect("没收到信号，占位活却不在台上了");
    assert_eq!(live.id, 占位.id());
    assert_eq!(live.progress.at, 0, "没人让它走，它自己走了——它还在数步数");
    let 排着的: Vec<u64> = app.tasks().queued().iter().map(|(id, _)| *id).collect();
    assert_eq!(排着的, vec![排在后面的], "占位活没占住台子");

    let id = 占位.id();
    占位.按停(app.tasks_mut());
    画到台上空了(&ctx, &mut app);
    let 历史 = app.tasks().history();
    let 它 = 历史
        .iter()
        .find(|one| one.id == id)
        .expect("占位活进了历史");
    assert_eq!(它.ending, Ending::Stopped, "按停过的那一趟没记成已取消");
    let 后面那趟 = 历史
        .iter()
        .find(|one| one.id == 排在后面的)
        .expect("排在后面的那一趟轮上了");
    assert_eq!(后面那趟.ending, Ending::Done(()));
}

#[test]
fn 占位活放行了就收场_丢掉它也一样() {
    // 不按停、只发信号（媒体那条测试就是这么放它走的）：它照样收场，而且**不记成已取消**
    // ——没人按过停下。发信号的那一头**丢了也算发**：测试半路炸了、忘了收拾，
    // 那一趟也不会把台子占到进程结束。
    let ctx = headless::context();
    let mut app = 开一个();
    app.show_view(View::Tasks);

    let 放行的 = 占位活::排上(app.tasks_mut(), "装作在扫一趟库");
    let 放行的号 = 放行的.id();
    放行的.放行();
    画到台上空了(&ctx, &mut app);
    let 记的 = &app.tasks().history()[0];
    assert_eq!(记的.id, 放行的号);
    assert!(
        matches!(记的.ending, Ending::Failed { .. }),
        "没人按停，却记成了「{}」",
        记的.ending.render(),
    );

    let 丢掉的 = 占位活::排上(app.tasks_mut(), "装作在扫一趟库");
    let 丢掉的号 = 丢掉的.id();
    drop(丢掉的);
    画到台上空了(&ctx, &mut app);
    assert_eq!(app.tasks().history()[0].id, 丢掉的号);
}
