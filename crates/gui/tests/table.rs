//! 十万行表格：**内存里永远只有当前视口的内容**，滚起来的代价与总行数无关。
//!
//! 这三条断言都不看时钟里的绝对数字（那随机器变），看的是**结构**：
//!
//! - 内存里装了几行——上界是一扇窗，与库里有多少变体无关。
//! - 读了几次库——常态滚动下远少于帧数，不是每帧都查。
//! - 每帧代价的**比值**——一千行与十万行之间不该差出一个量级。
//!
//! 绝对数字（多少毫秒、多少 fps）由 `romcat-gui --bench` 在 release 下量，写在票据里。

use romcat_gui::app::{App, View};
use romcat_gui::bench::{self, Sweep};
use romcat_gui::demo;
use romcat_gui::table::SPAN;

/// 这几条测试自己的**工作目录**。
///
/// **不共用 `demo::workspace()`**：那是 `--demo` 那个演示窗口用的目录，而窗口会往里写
/// **版式偏好**（面板拖到哪儿）。共用的话，维护者开一次演示窗口把某块面板拖高一截，
/// 下一次跑这几条测试屏上就少了几行——而断言数的正好是行数。
fn 工作目录() -> std::path::PathBuf {
    std::env::temp_dir().join("romcat-测试-表格")
}

/// **变体表那条线的量级**（票 22）：一行一个变体，与收敛成多少行无关。
///
/// 浏览屏那几条量的是**作品级主列表**，它们照真库的形状造（`demo::BROWSE_VARIANTS`
/// 收敛成 `demo::BROWSE_LINES` 行）；这一份不是——`demo::synthetic` 一个作品都不造，
/// 十万个变体在主列表上就是十万行，那正是这三条要撑住的最坏情况。
const 十万行: u64 = 100_000;

/// 拿来做对照的小库：**同一趟滚动，库大一百倍，内存里的行数该一个不差**。
const 小库: u64 = 1_000;

fn 界面(rows: u64) -> App {
    let site = demo::site(demo::synthetic(rows).expect("造得出合成数据")).expect("开得出现场");
    let mut app = App::new(site, 工作目录());
    // 这几条量的是**变体表**；打开工具看见的那一屏是待确认队列（ADR-0002）。
    app.show_view(View::Browse);
    app
}

#[test]
fn 滚完十万行内存里也只有一扇窗() {
    let mut app = 界面(十万行);
    let cost = bench::scroll(&mut app, 240, Sweep::Whole);
    assert_eq!(app.window().total(), 十万行);
    assert!(
        app.window().retained() as u64 <= SPAN,
        "内存里 {} 行，超过一扇窗（{SPAN} 行）",
        app.window().retained(),
    );
    // 一趟滚到底是最坏情况：每帧都跨出窗口，于是每帧都读一次库。
    assert!(cost.reads > 0, "一行都没读过，这一趟没滚动");
}

#[test]
fn 内存里的行数与总行数无关() {
    let mut 小 = 界面(小库);
    let mut 大 = 界面(十万行);
    for app in [&mut 小, &mut 大] {
        let _ = bench::scroll(app, 60, Sweep::Rows(3.0));
    }
    assert_eq!(小.window().total(), 小库);
    assert_eq!(大.window().total(), 十万行);
    assert_eq!(
        小.window().retained(),
        大.window().retained(),
        "库大了一百倍，内存里的行数就该一模一样",
    );
}

#[test]
fn 常态滚动不是每帧都读库() {
    let mut app = 界面(十万行);
    let cost = bench::scroll(&mut app, 240, Sweep::Rows(3.0));
    // 240 帧每帧 3 行 = 720 行，一扇窗 512 行，最多跨两次。
    assert!(
        cost.reads <= 3,
        "滚了 720 行读了 {} 次库，窗口预取没起作用",
        cost.reads,
    );
}

#[test]
fn 每帧代价与总行数无关() {
    let mut 小 = 界面(小库);
    let mut 大 = 界面(十万行);
    let 小的 = bench::scroll(&mut 小, 120, Sweep::Rows(3.0));
    let 大的 = bench::scroll(&mut 大, 120, Sweep::Rows(3.0));
    assert!(小的.median_ms > 0.0 && 大的.median_ms > 0.0, "没量到时间");
    // 阈值给得很松：要证的是「不随总行数线性涨」，而不是两个数字一样。
    // 库大了一百倍，真按行数算代价的话这个比值会是两位数。
    assert!(
        大的.median_ms < 小的.median_ms * 4.0,
        "十万行每帧 {:.3} ms，一千行 {:.3} ms——开销跟着总行数涨了",
        大的.median_ms,
        小的.median_ms,
    );
}
