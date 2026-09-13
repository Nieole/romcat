//! 几份界面测试**共用**的那点东西。
//!
//! 它摆在**子目录**里：`tests/` 底下每个 `.rs` 都被当成一个独立的测试二进制，
//! 共用的东西直接放那儿会变成一个一条测试都没有的二进制。

// `tests/` 底下每个 `.rs` 都是一个独立的测试二进制，而这个模块**整份**编进每一个
// `mod shared;` 它的二进制里。用得上哪几样各家不同——用不上的那几样在那个二进制里
// 就是死代码，`dead_code` 会为它报一条。这条告警说的不是「这段代码没人用」，
// 而是「这个二进制没用它」，所以整份压掉。
#![allow(dead_code)]

/// 这一帧**真的画在屏上**的那些字，**一段一行**。
///
/// 「屏上摆得出来」「屏上写的是同一个词」「屏上没了」这三类断言只有看这个才算数：
/// 查数据结构里有没有这条是在测别的东西（库里写没写、删没删干净，另有断言管），
/// 而那几屏要证的正是它**画出来了**。egui 每画一段文字就留下一个 `Galley`，它带着原文。
///
/// 每段后面跟一个换行，于是 `lines()` 问得出「屏上那一行写的是什么」——浏览屏那条
/// 「一条顶到闸上的简介收成一行画得下的那一截」要的正是这个粒度。**一段自己带的换行
/// 照样劈成两行**：值里若留着换行，屏上那一行就断在换行处——那条测试断「折平了」
/// 靠的正是这一点。
pub fn 画出来的字(output: &egui::FullOutput) -> String {
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

// ——— 手搭的一份小库 ———

/// 这份小库的**根名**。变体的键第一段就是它（`path::library_key`）。
///
/// 摆成常量而不是让调用方给：断言写的是整条键（`主库/SFC/命中.zip`），根名一改，
/// 两份测试里那一批字面量得跟着改——而这几条测试要看的从来不是根叫什么。
pub const 根: &str = "主库";

/// 候选里那部作品的名字。两份测试原来各写各的，写的是同一个。
pub const 候选作品: &str = "幻想传说 (Japan)";

/// 一个变体在这份小库里**落哪一档**。
///
/// 四档对应词表里四件不同的事，而它们在库里的差别只在**结论表**那一行上：
/// 写不写、写成什么结论、带不带候选。手搭这份库的理由正是这个——**四档要在同一张表上
/// 并排**，而合成数据里哪一行落哪一档随规模变：`demo::queue` 更是给**每个**变体都写了
/// 一行结论，[`档::还没识别`] 它一条都造不出（`demo::browse` 那一份倒是每十三个留一个
/// 不写结论的，可它同一屏上凑不齐另外三档挨在一起的样子）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum 档 {
    /// 识别跑过、**命中**：一条高置信、已采纳的候选。
    命中,
    /// 识别跑过、**未命中**，带一条低置信、没采纳的候选——它进得了**待确认队列**。
    待裁决,
    /// 识别**跑过了**，却一条候选都没有（词表：**没有候选**）。
    没有候选,
    /// 结论表里**一行都不写**——那就是词表里的**还没识别**。
    还没识别,
}

/// 造一个变体：键是 `<根>/<平台>/<名字>`，单文件规则，一个主成员。
#[must_use]
pub fn 变体(平台: &str, 名字: &str) -> romcat_core::shape::Variant {
    use romcat_core::shape::{Role, SINGLE_FILE_RULE, Variant};

    let key = format!("{根}/{平台}/{名字}");
    Variant {
        main_key: key.clone(),
        platform: Some(平台.to_string()),
        rule: SINGLE_FILE_RULE.to_string(),
        manual: false,
        files: 1,
        bytes: 4096,
        unreadable_files: 0,
        members: vec![(key.clone(), Role::Main)],
        key,
    }
}

/// 造一条候选，挂在这个变体自己的**主成员**上。
///
/// `member_key` 与 `platform` 都取那个变体自己的：单文件变体撞上的就是它那一个成员，
/// 而候选说的平台与变体所在的平台是同一个。
#[must_use]
pub fn 候选(
    变体: &romcat_core::shape::Variant,
    accepted: bool,
    confidence: romcat_core::catalog::Confidence,
) -> romcat_core::catalog::identify::Candidate {
    use romcat_core::catalog::identify::Candidate;
    use romcat_core::dat::Convention;

    Candidate {
        member_key: 变体.main_key.clone(),
        inner: String::new(),
        confidence,
        accepted,
        source: "合成".to_string(),
        dat: "合成.dat".to_string(),
        platform: 变体.platform.clone().unwrap_or_default(),
        game: 候选作品.to_string(),
        rom: "rom.bin".to_string(),
        hashed_as: Convention::AsIs,
        dat_convention: Convention::AsIs,
        evidence: "精确哈希命中".to_string(),
        chinese: None,
        serial: None,
        release_id: None,
    }
}

/// 搭一份**手搭的**小库：一行 `(平台, 名字, 档)` 一个变体，开出一个界面来。
///
/// 用手搭的而不是合成数据，是因为这几条要看的是**同一张表上几个档并排**，
/// 而合成数据里哪一行落哪一档随规模变；[`档::还没识别`] 它更是造不出来
/// （每个变体都被写了一行结论）。
///
/// `workspace` 由调用方给：**版式偏好是往工作目录里写文件的**，几份测试共用一个的话，
/// 一条测试拖出来的宽度会落到另一条测试打开的窗口上。
///
/// # Panics
/// 建库、建根、写变体、写结论任一步失败时当场炸。**吞掉它的话**，变体会挂在一个不存在
/// 的根上，而失败会以「屏上少了一行」的样子冒出来——一个夹具搭错报成一个界面缺陷。
#[must_use]
pub fn 小库(
    手上的: &[(&str, &str, 档)], workspace: std::path::PathBuf
) -> romcat_gui::app::App {
    use romcat_core::catalog::identify::Identification;
    use romcat_core::catalog::{Catalog, Confidence, State};
    use romcat_core::platform::Manifest;
    use romcat_core::site::Site;
    use romcat_core::verdict::Store;

    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    romcat_core::catalog::roots::add_root(
        &catalog,
        None,
        根,
        std::path::Path::new(&format!("/{根}")),
    )
    .expect("建得出根");

    let variants: Vec<_> = 手上的
        .iter()
        .map(|(平台, 名字, _)| 变体(平台, 名字))
        .collect();
    catalog
        .replace_variants(&variants, 1, &Manifest::default())
        .expect("写得进变体");

    let 结论: Vec<Identification> = 手上的
        .iter()
        .zip(&variants)
        .filter_map(|((_, _, 落在), variant)| {
            let (state, candidates) = match 落在 {
                档::命中 => (State::Matched, vec![候选(variant, true, Confidence::High)]),
                档::待裁决 => (
                    State::Unmatched,
                    vec![候选(variant, false, Confidence::Low)],
                ),
                档::没有候选 => (State::Unmatched, Vec::new()),
                // **一行都不写**——那就是「还没识别」。
                档::还没识别 => return None,
            };
            Some(Identification {
                variant_key: variant.key.clone(),
                platform: None,
                state,
                reason: None,
                units: 1,
                nkit: 0,
                read_bytes: 0,
                work_id: None,
                release_id: None,
                candidates,
            })
        })
        .collect();
    catalog.write_identifications(&结论).expect("写得进结论");

    let store = Store::in_memory().expect("开得出沉淀库");
    romcat_gui::app::App::new(Site::in_memory(catalog, store, 根), workspace)
}

// ——— 输入 ———

/// 按下一次 `key` 的键盘事件，不带修饰键。
///
/// 塞进 [`输入`] 跑一帧，走的就是真键盘那条路（`egui::Event::Key` 进 `RawInput`）。
#[must_use]
pub fn 按键事件(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

/// 一帧的输入：视口照 [`romcat_gui::headless::input`]，带着这几件事。
#[must_use]
pub fn 输入(events: Vec<egui::Event>) -> egui::RawInput {
    let mut input = romcat_gui::headless::input();
    input.events = events;
    input
}

// ——— 悬停 ———

/// 屏上写着这一段字的地方——**画出来的那一段**的中心点；找不着时是 `None`。
///
/// 悬停那一类断言要的正是它：`on_hover_text` 挂在某个控件上，而测试手上只有
/// 「屏上那一行写的是什么」。egui 每画一段文字就留下一个 `Galley`，它同时带着原文与
/// 画在哪儿——于是「把指针停到写着这句话的地方去」问得出来，**不用把控件的
/// `Rect` 从界面层漏出来**。
///
/// 按画出来的次序找**头一处**：同一句话在屏上出现不止一次时，取的是先画的那一处。
#[must_use]
pub fn 那一段画在哪儿(output: &egui::FullOutput, 那一段: &str) -> Option<egui::Pos2> {
    头一处画在哪儿(output, &|text| text.contains(那一段))
}

/// 屏上**正好**写着这一段字的地方：整段一字不差，不是「含有」。
///
/// 按钮上的字常常是别的句子里的一截——裁决记录里那颗「撤销」，也在那一块开头那句
/// 「撤销一批，那些变体……」里；按 [`那一段画在哪儿`] 找，点到的是先画出来的那句话。
#[must_use]
pub fn 正好那一段画在哪儿(
    output: &egui::FullOutput, 那一段: &str
) -> Option<egui::Pos2> {
    头一处画在哪儿(output, &|text| text == 那一段)
}

/// 按画出来的次序，头一段认得下的字画在哪儿（中心点）。
fn 头一处画在哪儿(
    output: &egui::FullOutput,
    认: &dyn Fn(&str) -> bool,
) -> Option<egui::Pos2> {
    fn 找(shape: &egui::epaint::Shape, 认: &dyn Fn(&str) -> bool) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(text) => 认(text.galley.text())
                .then(|| egui::Rect::from_min_size(text.pos, text.galley.size()).center()),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|one| 找(one, 认)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|clipped| 找(&clipped.shape, 认))
}

/// 指针不动地再跑这么多帧，够 egui 那道悬停延迟跨过去。
///
/// egui 0.36 的 `Style::interaction.tooltip_delay` 默认 **0.5 秒**，而这一层没有真的
/// 时钟：`RawInput::time` 留空时 egui 每帧自己往前推 `predicted_dt`（1/60 秒）。
/// 于是「指针停够半秒」在这里就是「不动地再跑三十帧」。取 90 是留了三倍的余量——
/// 同一道闸还看 `time_since_last_scroll`，那个数从这一帧往回算。
///
/// **贵不贵量过了**：九十一帧连布局带三角化，队列屏 2,000 行那份合成数据上整条测试
/// 1.2 秒跑完。另一条路是在测试的 `ctx` 上把 `tooltip_delay` 直接调成 0——那样两三帧
/// 就够，代价是屏上那句话到底等多久才出来从此没人看着（挂单 `Q347`）。
const 悬停要跑几帧: u32 = 90;

/// 把指针停在屏上写着 `那一段` 的地方，停够那道悬停延迟，返回**停住之后**那一帧
/// 画出来的字（含悬停自己那一句）。
///
/// 两条前提，调它之前得认下来：
///
/// - **先把界面跑稳。** 位置是在**头一帧**上量的，指针停在**下一帧**——首帧还在估
///   滚动区尺寸的时候量出来的位置会偏。失手的样子是当场炸或者断言红，不是伪绿。
/// - **它把指针留在 `ctx` 里**，而且悬停那一句还开着。同一个 `ctx` 后面再收一次
///   「画出来的字」会多出那一句；要干净的一帧就另起一个 `ctx`。
///
/// # Panics
/// 屏上找不着 `那一段` 时当场炸，并把这一帧画出来的字一并印出来——指针没处停的话，
/// 底下那条断言就是在测「没有悬停」，而它本来该测的是「悬停里写着什么」。
pub fn 悬停在(
    ctx: &egui::Context,
    那一段: &str,
    mut 画一帧: impl FnMut(&mut egui::Ui),
) -> String {
    use romcat_gui::headless;

    let 头一帧 = headless::frame(ctx, headless::input(), &mut 画一帧);
    let Some(停在) = 那一段画在哪儿(&头一帧, 那一段) else {
        panic!(
            "屏上没有「{那一段}」这一段，指针没处停：\n{}",
            画出来的字(&头一帧)
        );
    };

    let mut input = headless::input();
    input.events.push(egui::Event::PointerMoved(停在));
    let mut out = headless::frame(ctx, input, &mut 画一帧);
    for _ in 0..悬停要跑几帧 {
        out = headless::frame(ctx, headless::input(), &mut 画一帧);
    }
    画出来的字(&out)
}

// ——— 跑一帧、点一下、打字 ———
//
// 收的都是「怎么画一帧」那个闭包，与 [`悬停在`] 同一个形状：开场那一屏、添加主库向导、
// 主窗口，谁都递得进来。几份老测试各有一份绑死在自己那种屏上的（`program.rs`、`queue.rs`、
// `roots.rs`），新测试用这几样，不再各搭一份。

/// 跑一帧，交出这一帧画出来的字。
pub fn 跑一帧(ctx: &egui::Context, 画一帧: impl FnMut(&mut egui::Ui)) -> String {
    use romcat_gui::headless;

    画出来的字(&headless::frame(ctx, headless::input(), 画一帧))
}

/// 按一下屏上写着 `那一段` 的地方（移过去、按下、松开），返回**松开之后再画一帧**画出来的字。
///
/// 多跑一帧才收，是因为按下去的后果（换屏、重列、弹出下一步）落在松开那一帧的帧末，
/// 下一帧才画得出来。
///
/// # Panics
/// 屏上找不着 `那一段` 时当场炸，并把这一帧画出来的字一并印出来——没处点的话，底下那条
/// 断言测的就是「什么都没按」。
pub fn 点一下(
    ctx: &egui::Context,
    那一段: &str,
    mut 画一帧: impl FnMut(&mut egui::Ui),
) -> String {
    use romcat_gui::headless;

    let 头一帧 = headless::frame(ctx, headless::input(), &mut 画一帧);
    let Some(位置) = 那一段画在哪儿(&头一帧, 那一段) else {
        panic!("屏上没有「{那一段}」，没处点：\n{}", 画出来的字(&头一帧));
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

/// 往屏上那个写着 `框上写着` 的输入框里打一段字。
///
/// 先[点一下](点一下)把焦点放进去，再发一条文本事件——egui 把文本事件交给**拿着焦点**的那个
/// 控件，没有第二条路把字送进去。
pub fn 打字(
    ctx: &egui::Context,
    框上写着: &str,
    字: &str,
    mut 画一帧: impl FnMut(&mut egui::Ui),
) {
    use romcat_gui::headless;

    点一下(ctx, 框上写着, &mut 画一帧);
    let mut input = headless::input();
    input.events.push(egui::Event::Text(字.to_string()));
    headless::frame(ctx, input, &mut 画一帧);
}

// ——— 占位活：等信号，不等挂钟（票 `machine-checks-premises/07`）———

/// 一对**信号**里发的那一头：[`发`](信号::发) 一下，[`等信号`] 那一头就走。
///
/// **丢掉它也算发。** 测试半路炸了、忘了收拾，等着的那一头照样醒——一趟占位活不会
/// 因此把任务台占到进程结束。
///
/// 底下是一条 `std::sync::mpsc` 通道，不是 `Condvar`：「发的那一头丢了，等着的那一头就醒」
/// 是通道自带的；`Condvar` 得自己写 `Drop`、自己防假唤醒与锁中毒。
#[derive(Debug)]
pub struct 信号(std::sync::mpsc::Sender<()>);

/// 一对信号里等着的那一头。
#[derive(Debug)]
pub struct 等信号(std::sync::mpsc::Receiver<()>);

/// 开一对信号。
#[must_use]
pub fn 一对信号() -> (信号, 等信号) {
    let (发的, 收的) = std::sync::mpsc::channel();
    (信号(发的), 等信号(收的))
}

impl 信号 {
    /// 发出去。等着的那一头已经不在了（那一趟排着队时被撤掉，连它一起丢了）也不要紧。
    pub fn 发(self) {
        let _ = self.0.send(());
    }
}

impl 等信号 {
    /// 停在这儿，直到信号发过来、或者发的那一头被丢掉。**不看挂钟**：一毫秒都不睡，
    /// 等多久全由发信号的那一侧说了算。
    pub fn 等(self) {
        let _ = self.0.recv();
    }
}

/// 任务台上一趟**占着位子、等测试发信号才收场**的活。
///
/// 按停那类界面测试要它：任务台一次只跑一趟，台上摆着它，之后排上去的那一趟就稳稳停在
/// 队里——「排着队时按停」「跑着的时候那颗按钮按不下去」这类事情因此不带竞态。
///
/// ## 为什么等信号
///
/// 早先几份测试各写各的「几千步、每步睡几毫秒」（挂单 `Q427`）。那是一个**挂钟窗口**：
/// 机器一忙，它就在测试看完之前自己跑完了。**真的咬过一次**——写死「400 步 × 5 ms」正好
/// 两秒的那一份，全量测试并排跑时自己先结束，测试假失败。占位活只是假装在干活，
/// **干多久本来就该由测试说了算**，所以它什么都不干，就等一个信号。
///
/// ## 怎么用、怎么停
///
/// - **排上**：`let 占位 = 占位活::排上(app.tasks_mut(), "装作在扫一趟库");`
/// - **按停**：`占位.按停(app.tasks_mut());`——先走界面上那颗「停下」同一个入口
///   （`Board::stop`），再发信号叫醒它，于是它记成**已取消**。之后照常等台上空了。
/// - **只放行**：`占位.放行();`——没人按停，它收场记成失败。要的只是它占过那一段时用这个。
/// - **丢掉它等于放行**，所以别拿 `let _ =` 接它：那样它排上去当场就走了。
/// - 要它等信号之前先报点进度、或者收场时交别的，用 [`照这样排上`](Self::照这样排上)。
///
/// 只拿 `Board::stop(id)` 按停而不发信号，**它醒不过来**：停下的信号是一个标志位，
/// 叫不醒一条正在等的线程。那样「等台上空了」那一步会等满它自己的时限再炸，不会挂住。
#[must_use = "丢掉它就等于当场放行，那一趟占不住位子"]
#[derive(Debug)]
pub struct 占位活 {
    id: u64,
    发的: 信号,
}

impl 占位活 {
    /// 排一趟占位活上去：一步都不走，停在那儿等信号。醒过来时被按停过就记成**已取消**，
    /// 没按停过就记成**失败**。
    pub fn 排上<T: Send + 'static>(
        tasks: &mut romcat_core::task::Board<T>, 名字: &str
    ) -> Self {
        Self::照这样排上(tasks, 名字, |task, 等着| {
            等着.等();
            task.check()?;
            Err(romcat_core::task::Cutoff::failed(
                "占位活放行了：它本来就只是占着位子",
            ))
        })
    }

    /// 排一趟占位活，**活长什么样由调用方给**：它收到把手与等着的那一头，
    /// `等着.等()` 那一下就是停在那儿等信号。
    ///
    /// **醒过来之后先看一眼有没有被按停**（`task.check()?`，或者折进领域错误再抛），
    /// 不然被按停过它也记不成「已取消」。
    pub fn 照这样排上<T: Send + 'static>(
        tasks: &mut romcat_core::task::Board<T>,
        名字: &str,
        活: impl FnOnce(&romcat_core::task::Handle, 等信号) -> Result<T, romcat_core::task::Cutoff>
        + Send
        + 'static,
    ) -> Self {
        let (发的, 等着) = 一对信号();
        let 排它的线程 = std::thread::current().id();
        let id = tasks.queue(名字, move |task| {
            // **跑在排它的那条线程上就不等**：信号只有那条线程发得出，等下去就把它永远堵死，
            // 测试不红、门禁卡住。任务台哪天改成就地跑活，这里当场交一句失败，测试照常红。
            if std::thread::current().id() == 排它的线程 {
                return Err(romcat_core::task::Cutoff::failed(
                    "占位活跑在了排它的那条线程上：等信号会把那条线程堵死",
                ));
            }
            活(task, 等着)
        });
        Self { id, 发的 }
    }

    /// 它在任务台上的任务号。
    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }

    /// **按停**它：先走界面上那颗「停下」同一个入口，再发信号叫醒它。
    ///
    /// **次序不能反**：先发信号的话，它可能赶在停下的标志落下之前醒过来看那一眼，
    /// 于是记成失败而不是已取消——而且时灵时不灵。
    pub fn 按停<T: Send + 'static>(self, tasks: &mut romcat_core::task::Board<T>) {
        tasks.stop(self.id);
        self.发的.发();
    }

    /// 不按停，只放它走。
    pub fn 放行(self) {
        self.发的.发();
    }
}
