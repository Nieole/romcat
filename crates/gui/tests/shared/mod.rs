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
    fn 找(shape: &egui::epaint::Shape, 那一段: &str) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(text) => text
                .galley
                .text()
                .contains(那一段)
                .then(|| egui::Rect::from_min_size(text.pos, text.galley.size()).center()),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|one| 找(one, 那一段)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|clipped| 找(&clipped.shape, 那一段))
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
