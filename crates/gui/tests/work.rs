//! **作品详情页**（票 `gui-looks-like-the-design/15`）：双击主列表一行，或者在侧边详情里点「查看详情」，
//! 一部作品的全部情况在这一层里看得完、改得动——概览、变体与文件、元数据、标题、媒体、识别依据六个面。
//!
//! 断言看的是**这一帧真画出来的字**（`shared::画出来的字`）与真按下去的结果，不看界面里头的结构。

use romcat_core::catalog::browse::{WorkAnchor, WorkQuery};
use romcat_core::report::thousands;
use romcat_core::scrape::Priorities;
use romcat_gui::app::{App, View};
use romcat_gui::browse::work::Tab;
use romcat_gui::{demo, headless};

mod shared;
use shared::{正好那一段画在哪儿, 点一下, 画出来的字, 等任务台空了};

/// 六个面，照稿上的次序。
const 六个面: [&str; 6] = ["概览", "变体与文件", "元数据", "标题", "媒体", "识别依据"];

/// 这几条测试自己的**工作目录**：不与别的测试、也不与演示窗口共用（窗口往里写版式偏好）。
fn 工作目录() -> std::path::PathBuf {
    std::env::temp_dir().join("romcat-测试-作品详情")
}

fn 界面(rows: u64) -> App {
    let site = demo::site(demo::browse(rows).expect("造得出合成数据")).expect("开得出现场");
    let mut app = App::new(site, 工作目录());
    app.show_view(View::Browse);
    app
}

fn 跑(ctx: &egui::Context, app: &mut App, frames: u32) {
    for _ in 0..frames {
        headless::frame(ctx, headless::input(), |ui| app.ui(ui));
    }
}

/// 表上头几行里**头一个认出了作品的**：它在表上第几行、表上那一行主栏写着什么（显示标题，取不到时作品名）。
///
/// 照表格自己取行的那一条问（同一份默认筛选、同一份内置优先级表），不从界面里头掏。
fn 表上一个作品(app: &mut App) -> (u64, String) {
    let (_, site) = app.browse_and_site();
    site.catalog
        .work_page_with_titles(&WorkQuery::default(), 0, 8, &Priorities::builtin())
        .expect("取得出一页")
        .into_iter()
        .zip(0_u64..)
        .find_map(|(row, at)| {
            matches!(row.anchor, WorkAnchor::Work(_)).then(|| (at, row.display.unwrap_or(row.name)))
        })
        .expect("合成数据头几行里该有认出了作品的")
}

/// 在屏上**正好**写着 `那几个字` 的地方连点两下（两帧各一次按下松开），再跑一帧，交出那一帧画出来的字。
fn 双击(ctx: &egui::Context, app: &mut App, 那几个字: &str) -> String {
    let 头一帧 = headless::frame(ctx, headless::input(), |ui| app.ui(ui));
    let Some(位置) = 正好那一段画在哪儿(&头一帧, 那几个字) else {
        panic!(
            "屏上没有正好写着「{那几个字}」的地方，没处双击：\n{}",
            画出来的字(&头一帧)
        );
    };
    let 按 = |pressed: bool| egui::Event::PointerButton {
        pos: 位置,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    for _ in 0..2 {
        let mut input = headless::input();
        input.events.push(egui::Event::PointerMoved(位置));
        input.events.push(按(true));
        input.events.push(按(false));
        headless::frame(ctx, input, |ui| app.ui(ui));
    }
    画出来的字(&headless::frame(ctx, headless::input(), |ui| app.ui(ui)))
}

/// 这一帧里有没有**正好**是这几个字的一段。
fn 有这一段(屏上: &str, 那几个字: &str) -> bool {
    屏上.lines().any(|line| line == 那几个字)
}

#[test]
fn 双击主列表一行打开作品详情页_六个面都在_返回浏览回到表格() {
    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 3);
    let (_, 名字) = 表上一个作品(&mut app);

    let 屏上 = 双击(&ctx, &mut app, &名字);
    assert!(
        有这一段(&屏上, "← 返回浏览"),
        "双击之后没有「← 返回浏览」：\n{屏上}"
    );
    assert!(
        有这一段(&屏上, &format!("浏览 / {名字}")),
        "顶上那一条没写这是哪个作品：\n{屏上}"
    );
    for 面 in 六个面 {
        assert!(有这一段(&屏上, 面), "六个面里没画出「{面}」：\n{屏上}");
    }
    // **详情页盖住浏览屏的屏头与正文**（设计稿 `.wd` 铺满 `.scr`）：表格上方那一条、屏头那句副标题都不在了。
    assert!(
        !屏上.contains("在每行开头显示封面"),
        "详情页开着，表格上方那一条还画着：\n{屏上}"
    );
    assert!(
        !屏上.contains("查找作品，并对选中的内容进行操作"),
        "详情页开着，浏览屏的屏头还画着：\n{屏上}"
    );

    let 回去 = 点一下(&ctx, "← 返回浏览", |ui| app.ui(ui));
    assert!(
        回去.contains("在每行开头显示封面"),
        "点「← 返回浏览」没回到表格：\n{回去}"
    );
    assert!(
        !有这一段(&回去, "识别依据"),
        "回到浏览屏了，详情页的面还画着：\n{回去}"
    );
}

/// 按一下屏上**正好**写着 `那几个字` 的地方（移过去、按下、松开），再跑一帧，交出那一帧画出来的字。
fn 按正好(ctx: &egui::Context, app: &mut App, 那几个字: &str) -> String {
    let 头一帧 = headless::frame(ctx, headless::input(), |ui| app.ui(ui));
    let Some(位置) = 正好那一段画在哪儿(&头一帧, 那几个字) else {
        panic!(
            "屏上没有正好写着「{那几个字}」的地方，没处按：\n{}",
            画出来的字(&头一帧)
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
    headless::frame(ctx, input, |ui| app.ui(ui));
    let mut input = headless::input();
    input.events.push(按(false));
    headless::frame(ctx, input, |ui| app.ui(ui));
    画出来的字(&headless::frame(ctx, headless::input(), |ui| app.ui(ui)))
}

#[test]
fn 侧边详情里点查看详情打开概览_点编辑元数据打开元数据那一面() {
    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 3);
    let (_, 名字) = 表上一个作品(&mut app);

    // 单击一行只点开侧边详情，不打开详情页。
    let 屏上 = 按正好(&ctx, &mut app, &名字);
    assert!(app.browse().page().is_none(), "单击一行不该打开作品详情页");
    assert!(
        有这一段(&屏上, "查看详情") && 有这一段(&屏上, "编辑元数据"),
        "侧边详情里没摆「查看详情」「编辑元数据」：\n{屏上}"
    );

    let 屏上 = 按正好(&ctx, &mut app, "查看详情");
    assert_eq!(
        app.browse().page().map(|page| page.tab()),
        Some(Tab::Overview)
    );
    assert!(
        有这一段(&屏上, &format!("浏览 / {名字}")),
        "点「查看详情」没打开这个作品的详情页：\n{屏上}"
    );

    按正好(&ctx, &mut app, "← 返回浏览");
    assert!(app.browse().page().is_none(), "「← 返回浏览」没关掉详情页");
    按正好(&ctx, &mut app, "编辑元数据");
    assert_eq!(
        app.browse().page().map(|page| page.tab()),
        Some(Tab::Metadata),
        "「编辑元数据」该打开元数据那一面"
    );
}

#[test]
fn 上一个下一个照表的次序走_首尾相接_顶上写着第几个() {
    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 3);
    let (at, 名字) = 表上一个作品(&mut app);
    let 共 = thousands(app.window().total());

    let 屏上 = 双击(&ctx, &mut app, &名字);
    assert!(
        有这一段(&屏上, &format!("{} / {共}", at + 1)),
        "顶上没写这是表上第几个：\n{屏上}"
    );

    let 屏上 = 按正好(&ctx, &mut app, "下一个");
    assert!(
        有这一段(&屏上, &format!("{} / {共}", at + 2)),
        "点「下一个」没换到表上的下一行：\n{屏上}"
    );
    assert!(
        !有这一段(&屏上, &format!("浏览 / {名字}")),
        "换到下一个了，顶上还写着原来那个作品：\n{屏上}"
    );

    let 屏上 = 按正好(&ctx, &mut app, "上一个");
    assert!(
        有这一段(&屏上, &format!("{} / {共}", at + 1))
            && 有这一段(&屏上, &format!("浏览 / {名字}")),
        "点「上一个」没回到原来那个作品：\n{屏上}"
    );

    // **首尾相接**（设计稿 `stepWD`）：从表上头一个往前，是最后一个。
    for _ in 0..at {
        按正好(&ctx, &mut app, "上一个");
    }
    let 屏上 = 按正好(&ctx, &mut app, "上一个");
    assert!(
        有这一段(&屏上, &format!("{共} / {共}")),
        "从头一个往前没绕到最后一个：\n{屏上}"
    );
}

/// 找一个**同一平台上不止一个变体**的作品：首选变体得有得挑才测得出来。交回作品在 `work` 表里的行号。
///
/// 照核心库的公开查询找（`variant_detail` 的 `siblings` 是同作品同平台、按首选规则排好的那一串），不从界面里头掏。
fn 一个有得挑首选的作品(app: &mut App) -> i64 {
    let (_, site) = app.browse_and_site();
    site.catalog
        .variant_page(&romcat_core::catalog::VariantQuery::default(), 0, 512)
        .expect("取得出一页")
        .into_iter()
        .find_map(|row| {
            let detail = site
                .catalog
                .variant_detail(&row.key, &Priorities::builtin(), None)
                .expect("读得出详情")?;
            (detail.siblings.len() > 1).then_some(row.work_id).flatten()
        })
        .expect("合成数据里该有同作品同平台的两个变体")
}

/// 打开这个作品的详情页，停在这一面，跑两帧（头一帧滚动区还在量尺寸），交出第二帧画出来的字。
fn 打开详情页(ctx: &egui::Context, app: &mut App, work_id: i64, tab: Tab) -> String {
    {
        let (browse, site) = app.browse_and_site();
        browse.open_work(&site.catalog, &WorkAnchor::Work(work_id));
        browse.open_page(tab);
    }
    headless::frame(ctx, headless::input(), |ui| app.ui(ui));
    画出来的字(&headless::frame(ctx, headless::input(), |ui| app.ui(ui)))
}

#[test]
fn 变体与文件那一面每个变体一张卡_位置发行版与文件都在_设为首选变体记为裁决() {
    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 3);
    let work_id = 一个有得挑首选的作品(&mut app);
    let 屏上 = 打开详情页(&ctx, &mut app, work_id, Tab::Variants);

    // 期望从核心库的公开查询里取：这个作品有哪几个变体、各叫什么（变体简称）、哪一个眼下不是首选。
    let (头一个, 头一个的详情, 头一个简称, 不是首选的) = {
        let (_, site) = app.browse_and_site();
        let work = site
            .catalog
            .work_detail(&WorkQuery::default(), &WorkAnchor::Work(work_id))
            .expect("读得出")
            .expect("有这个作品");
        let 简称 = site
            .catalog
            .variant_short_names(&work, &Priorities::builtin())
            .expect("拼得出变体简称");
        let 详情 = |key: &str| {
            site.catalog
                .variant_detail(key, &Priorities::builtin(), None)
                .expect("读得出")
                .expect("有这个变体")
        };
        let 不是首选的 = work
            .variants
            .iter()
            .map(|variant| 详情(&variant.row.key))
            .find(|detail| !detail.is_preferred())
            .expect("有得挑首选的作品里总有一个不是首选");
        let 头一个 = work.variants[0].row.key.clone();
        (头一个.clone(), 详情(&头一个), 简称[0].clone(), 不是首选的)
    };

    assert!(
        有这一段(&屏上, &头一个简称),
        "头一张卡上没写变体简称「{头一个简称}」：\n{屏上}"
    );
    assert!(
        有这一段(&屏上, "首选变体"),
        "哪一个是首选变体没标出来：\n{屏上}"
    );
    // **位置**照稿写「根名 · 相对路径」，拆键由核心库做。
    let (根名, 相对) = romcat_core::path::split_root(&头一个);
    assert!(
        有这一段(&屏上, "位置") && 屏上.contains(&format!("{根名} · {相对}")),
        "头一张卡上没写它在哪儿：\n{屏上}"
    );
    for 格 in ["发行版", "地区", "语言", "序列号", "大小", "CRC-32"] {
        assert!(有这一段(&屏上, 格), "发行版那几格里没有「{格}」：\n{屏上}");
    }
    if let Some(地区) = 头一个的详情
        .release
        .as_ref()
        .and_then(|release| release.region.as_deref())
    {
        assert!(有这一段(&屏上, 地区), "地区那一格没写「{地区}」：\n{屏上}");
    }
    for (成员, 身份) in &头一个的详情.members {
        let 文件名 = romcat_core::path::file_name_of_key(成员);
        assert!(
            屏上.contains(文件名) && 有这一段(&屏上, 身份.code()),
            "文件表里没列「{文件名}」（{}）：\n{屏上}",
            身份.code()
        );
    }

    // **设为首选变体**：按第一颗，落的是按变体的键排头一个不是首选的那一个——记为裁决。
    按正好(&ctx, &mut app, "设为首选变体");
    let (_, site) = app.browse_and_site();
    let work = 不是首选的.work.clone().expect("认出了作品");
    let 平台 = 不是首选的.row.platform.clone().expect("认出了平台");
    assert_eq!(
        site.catalog
            .preferred_variant(&work, &平台)
            .expect("读得出"),
        Some(不是首选的.row.key.clone()),
        "「设为首选变体」没落成那个平台上的首选变体裁决"
    );

    // **恢复规则选择**（拿主意的人 2026-09-15 定）：有了首选裁决才摆这一颗；按下去撤掉那条裁决，按规则重新选，这一颗跟着收起。
    let 屏上 = 带着事件跑一帧(&ctx, &mut app, Vec::new());
    assert!(
        有这一段(&屏上, "恢复规则选择"),
        "有首选变体裁决时卡上没摆「恢复规则选择」：\n{屏上}"
    );
    按正好(&ctx, &mut app, "恢复规则选择");
    let (_, site) = app.browse_and_site();
    assert_eq!(
        site.catalog
            .preferred_variant(&work, &平台)
            .expect("读得出"),
        None,
        "「恢复规则选择」没撤掉那条首选变体裁决"
    );
    let 屏上 = 带着事件跑一帧(&ctx, &mut app, Vec::new());
    assert!(
        !有这一段(&屏上, "恢复规则选择"),
        "撤掉裁决之后「恢复规则选择」还摆着：\n{屏上}"
    );
}

/// 找一个作品，它**头一个变体**满足 `要`。交回作品在 `work` 表里的行号与那个变体（核心库交回来的样子）。
fn 头一个变体满足(
    app: &mut App,
    要: impl Fn(&romcat_core::catalog::browse::WorkVariant) -> bool,
) -> (i64, romcat_core::catalog::browse::WorkVariant) {
    let (_, site) = app.browse_and_site();
    site.catalog
        .work_page(&WorkQuery::default(), 0, 512)
        .expect("取得出一页")
        .into_iter()
        .filter_map(|row| match row.anchor {
            WorkAnchor::Work(id) => Some(id),
            WorkAnchor::Loose(_) => None,
        })
        .find_map(|id| {
            let work = site
                .catalog
                .work_detail(&WorkQuery::default(), &WorkAnchor::Work(id))
                .expect("读得出")?;
            let 头一个 = work.variants.into_iter().next()?;
            要(&头一个).then_some((id, 头一个))
        })
        .expect("合成数据里该有这样的作品")
}

#[test]
fn 识别依据那一面逐变体列出候选来源与置信度_一条候选都没有时照核心库那句说() {
    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 3);

    // ── 有候选的：每条候选撞的是哪条条目、哪个源、哪一档置信度，连「判定依据」那一句。
    let (有候选的作品, 变体) =
        头一个变体满足(&mut app, |variant| !variant.candidates.is_empty());
    let 屏上 = 打开详情页(&ctx, &mut app, 有候选的作品, Tab::Evidence);
    assert!(
        有这一段(&屏上, variant_state(&变体)),
        "头一张卡上没写识别结论：\n{屏上}"
    );
    assert!(
        有这一段(&屏上, 变体.confidence_label()),
        "头一张卡上没写置信度那一档：\n{屏上}"
    );
    let 头一条 = &变体.candidates[0];
    assert!(
        屏上.contains(&头一条.game),
        "候选那一列没写撞上的条目「{}」：\n{屏上}",
        头一条.game
    );
    assert!(
        有这一段(&屏上, &头一条.source) && 有这一段(&屏上, 头一条.confidence.label()),
        "候选那一行没写来源「{}」与置信度「{}」：\n{屏上}",
        头一条.source,
        头一条.confidence.label()
    );
    assert!(
        屏上.contains("高置信自动通过；中、低置信进入待确认队列。"),
        "识别依据那一面的帮助没照稿说全：\n{屏上}"
    );
    // 「判定依据：」后头照依据形状各段排：来源 / DAT / 哈希口径 · 依据 · 候选数（核心库 `WorkVariant::basis_line`）。
    let 那一句 = format!(
        "判定依据：{}",
        变体.basis_line().expect("有候选的变体说得出依据")
    );
    assert!(有这一段(&屏上, &那一句), "卡上没有「{那一句}」：\n{屏上}");
    for 表头 in ["候选", "来源", "置信度"] {
        assert!(
            有这一段(&屏上, 表头),
            "候选表没有「{表头}」那一列：\n{屏上}"
        );
    }

    // ── 一条候选都没有的：「没有候选」与「还没识别」说的话不一样，由核心库挑那一句（`no_candidate_hint`）。
    按正好(&ctx, &mut app, "← 返回浏览");
    let (没候选的作品, 变体) =
        头一个变体满足(&mut app, |variant| variant.candidates.is_empty());
    let 屏上 = 打开详情页(&ctx, &mut app, 没候选的作品, Tab::Evidence);
    let 那一句 = 变体
        .no_candidate_hint()
        .expect("一条候选都没有时核心库有话说");
    assert!(
        屏上.contains(那一句),
        "一条候选都没有，卡上没照核心库那句说（{那一句}）：\n{屏上}"
    );
}

/// 这个变体的识别结论那个词：跑过识别的是结论那一档，没跑过的是「还没识别」（核心库的两个常量）。
fn variant_state(variant: &romcat_core::catalog::browse::WorkVariant) -> &'static str {
    variant.state.map_or(
        romcat_core::catalog::identify::NOT_RUN_LABEL,
        romcat_core::catalog::State::label,
    )
}

/// 这个作品在详情页上是哪一个**条目**：作品名、头一个平台（「当前值」照它算，协调人 2026-09-15 定）、
/// 那个平台上的首选变体（汉化组从它身上取）。照核心库的公开查询问。
fn 条目(app: &mut App, work_id: i64) -> (String, String, Option<String>) {
    let (_, site) = app.browse_and_site();
    let work = site
        .catalog
        .work_detail(&WorkQuery::default(), &WorkAnchor::Work(work_id))
        .expect("读得出")
        .expect("有这个作品");
    let 平台 = work.platforms.first().cloned().expect("认出了平台");
    let 头 = work
        .variants
        .iter()
        .filter(|variant| variant.row.platform.as_deref() == Some(平台.as_str()))
        .find_map(|variant| {
            site.catalog
                .variant_detail(&variant.row.key, &Priorities::builtin(), None)
                .expect("读得出")
                .and_then(|detail| detail.preferred_now().map(str::to_owned))
        });
    (work.name, 平台, 头)
}

/// 核心库说这个条目上这个字段眼下写出去的是什么（与导出同一处）。
fn 核心库说的(
    app: &mut App,
    (作品名, 平台, 头): &(String, String, Option<String>),
    field: romcat_core::scrape::Field,
) -> romcat_core::scrape::priority::FieldShown {
    let (_, site) = app.browse_and_site();
    romcat_core::scrape::priority::entry_fields(
        &site.catalog,
        romcat_core::scrape::AnchorKind::Work,
        作品名,
        平台,
        头.as_deref(),
        &Priorities::builtin(),
    )
    .expect("读得出")
    .into_iter()
    .find(|one| one.field == field)
    .expect("这一格在")
}

#[test]
fn 元数据那一面每个字段写着用的是哪个源的值_一键改用另一个来源记为裁决_能撤销() {
    use romcat_core::scrape::Field;
    use romcat_core::scrape::priority::{Said, VERDICT};

    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 3);
    let work_id = 一个有得挑首选的作品(&mut app);
    let 这个条目 = 条目(&mut app, work_id);
    给简介摆两家说法(&mut app, &这个条目);
    let 眼下 = 核心库说的(&mut app, &这个条目, Field::Description);
    let 用的 = 眼下.shown.clone().expect("简介写出去不是空的");
    let 源 = 用的.source.clone().expect("是哪个源说的");
    let 另一条 = 眼下
        .offered
        .iter()
        .find(|value| value.source != 源)
        .expect("另一家说法也列着")
        .clone();

    let 屏上 = 打开详情页(&ctx, &mut app, work_id, Tab::Metadata);
    assert!(
        有这一段(&屏上, "标题集合"),
        "显示标题那一格的徽标该照稿写「标题集合」：\n{屏上}"
    );
    assert!(
        有这一段(&屏上, &用的.values[0]) && 有这一段(&屏上, &源),
        "简介那一格没写眼下用的是「{源}」说的「{}」：\n{屏上}",
        用的.values[0]
    );
    let 其他 = format!("其他 {} 个来源 ▾", 眼下.offered.len() - 1);
    assert!(有这一段(&屏上, &其他), "简介那一格没有「{其他}」：\n{屏上}");

    let 屏上 = 按正好(&ctx, &mut app, &其他);
    assert!(
        有这一段(&屏上, &另一条.value) && 有这一段(&屏上, "使用这个值"),
        "展开之后没列出「{}」说的「{}」：\n{屏上}",
        另一条.source,
        另一条.value
    );

    // **一键改用另一个来源**：记为裁决，写出去的换成那一句。
    let 屏上 = 按正好(&ctx, &mut app, "使用这个值");
    assert_eq!(
        核心库说的(&mut app, &这个条目, Field::Description).shown,
        Some(Said {
            source: Some(VERDICT.to_string()),
            values: vec![另一条.value.clone()],
        }),
        "「使用这个值」没记成裁决"
    );
    assert!(
        有这一段(&屏上, "手动"),
        "简介那一格没标出眼下用的是手动修改（来源是裁决的照稿印「手动」）：\n{屏上}"
    );

    // **能撤销**：回到数据源说了算。
    按正好(&ctx, &mut app, "撤销手动修改");
    assert_eq!(
        核心库说的(&mut app, &这个条目, Field::Description).shown,
        Some(用的),
        "撤销之后没回到原来那一家"
    );

    // **显示标题也是字段**：不能只在编辑态把名称填进框里；平时应能从标题集合中直接选另一来源，
    // 写成裁决后又撤掉，回到标题集合原来的选择。
    let 标题原来 = 核心库说的(&mut app, &这个条目, Field::Title)
        .shown
        .expect("显示标题在");
    let 标题集 = {
        let (_, site) = app.browse_and_site();
        site.catalog.titles_of(&这个条目.0).expect("读得出标题集合")
    };
    let 另一名称 = 标题集
        .iter()
        .find(|row| !标题原来.values.contains(&row.value))
        .expect("合成数据里有另一种名称")
        .clone();
    let 标题其他 = format!(
        "其他 {} 个来源 ▾",
        标题集
            .iter()
            .filter(|row| !标题原来.values.contains(&row.value))
            .count()
    );
    let 屏上 = 按正好(&ctx, &mut app, &标题其他);
    assert!(
        有这一段(&屏上, &另一名称.value) && 有这一段(&屏上, "使用这个值"),
        "显示标题没列出可直接采用的另一来源：\n{屏上}"
    );
    按正好(&ctx, &mut app, "使用这个值");
    assert_eq!(
        核心库说的(&mut app, &这个条目, Field::Title).shown,
        Some(Said {
            source: Some(VERDICT.to_string()),
            values: vec![另一名称.value.clone()],
        }),
        "显示标题改用另一来源没有记成裁决"
    );
    按正好(&ctx, &mut app, "撤销手动修改");
    assert_eq!(
        核心库说的(&mut app, &这个条目, Field::Title).shown,
        Some(标题原来),
        "撤销显示标题的裁决后没有回到标题集合原来的选择"
    );
}

/// 一个源在这个作品上说了一句简介：写进中立库的样子与刮削写的一样（`put_scraped` 按「锚点 × 源」整份换掉）。
fn 采到一句简介(
    app: &mut App,
    (作品名, _, _): &(String, String, Option<String>),
    source: &str,
    value: &str,
) {
    use romcat_core::catalog::scrape::{Harvested, HarvestedValue};
    use romcat_core::scrape::{AnchorKind, Field};

    let (_, site) = app.browse_and_site();
    site.catalog
        .put_scraped(&[Harvested {
            anchor: AnchorKind::Work.label().to_string(),
            subject: 作品名.clone(),
            source: source.to_string(),
            input: "测试".to_string(),
            values: vec![HarvestedValue {
                field: Field::Description.label().to_string(),
                value: value.to_string(),
                evidence: "测试摆的".to_string(),
            }],
            media: Vec::new(),
        }])
        .expect("写得进刮削结果");
}

/// 给这个作品的简介摆两家说法：ScreenScraper 一句英文、中文离线源一句中文。
fn 给简介摆两家说法(app: &mut App, 这个条目: &(String, String, Option<String>)) {
    采到一句简介(app, 这个条目, "ScreenScraper", "An English description.");
    采到一句简介(app, 这个条目, "中文离线源", "一段中文简介。");
}

/// 点一下简介那一框（照它眼下写着的字找），再全选（Cmd+A）——之后打的字换掉整框。
fn 选中简介那一框(ctx: &egui::Context, app: &mut App, 框里写着: &str) {
    按正好(ctx, app, 框里写着);
    let mut input = headless::input();
    input.events.push(egui::Event::Key {
        key: egui::Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    });
    headless::frame(ctx, input, |ui| app.ui(ui));
}

/// 跑一帧，带着这几件事。交出这一帧画出来的字。
fn 带着事件跑一帧(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) -> String {
    let mut input = headless::input();
    input.events = events;
    画出来的字(&headless::frame(ctx, input, |ui| app.ui(ui)))
}

#[test]
fn 编辑元数据保存后记为裁决_重新刮削不覆盖_放弃不落库() {
    use romcat_core::scrape::Field;
    use romcat_core::scrape::priority::{Said, VERDICT};

    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 3);
    let work_id = 一个有得挑首选的作品(&mut app);
    let 这个条目 = 条目(&mut app, work_id);
    给简介摆两家说法(&mut app, &这个条目);
    let 用的 = 核心库说的(&mut app, &这个条目, Field::Description)
        .shown
        .expect("简介写出去不是空的");
    let 源 = 用的.source.clone().expect("是哪个源说的");
    let 裁决 = |value: &str| {
        Some(Said {
            source: Some(VERDICT.to_string()),
            values: vec![value.to_string()],
        })
    };

    打开详情页(&ctx, &mut app, work_id, Tab::Metadata);
    let 屏上 = 按正好(&ctx, &mut app, "编辑");
    assert!(
        有这一段(&屏上, "还没有修改。") && 有这一段(&屏上, "保存") && 有这一段(&屏上, "放弃"),
        "点「编辑」之后底下没摆出保存那一条：\n{屏上}"
    );

    选中简介那一框(&ctx, &mut app, &用的.values[0]);
    带着事件跑一帧(
        &ctx,
        &mut app,
        vec![egui::Event::Text("手写的简介".to_string())],
    );
    let 屏上 = 带着事件跑一帧(&ctx, &mut app, Vec::new());
    assert!(
        有这一段(&屏上, "已修改 1 个字段。") && 有这一段(&屏上, "已修改"),
        "改了简介那一框，屏上没说改了几个字段、改的是哪一格：\n{屏上}"
    );
    // 没按保存之前一个字都没落库。
    assert_eq!(
        核心库说的(&mut app, &这个条目, Field::Description).shown,
        Some(用的.clone()),
        "还没按保存就落了库"
    );

    按正好(&ctx, &mut app, "保存");
    assert_eq!(
        核心库说的(&mut app, &这个条目, Field::Description).shown,
        裁决("手写的简介"),
        "保存之后简介没记成裁决"
    );

    // **重新刮削不覆盖**：原来那一家又采回来一句新的，写出去的照旧是裁决。
    采到一句简介(&mut app, &这个条目, &源, "刮削又采回来一句。");
    assert_eq!(
        核心库说的(&mut app, &这个条目, Field::Description).shown,
        裁决("手写的简介"),
        "重新刮削把裁决盖掉了"
    );

    // **放弃**：改了不存，库里一个字不动，保存那一条收起来。
    跑(&ctx, &mut app, 2);
    按正好(&ctx, &mut app, "编辑");
    选中简介那一框(&ctx, &mut app, "手写的简介");
    带着事件跑一帧(
        &ctx,
        &mut app,
        vec![egui::Event::Text("放弃掉的一句".to_string())],
    );
    let 屏上 = 按正好(&ctx, &mut app, "放弃");
    assert_eq!(
        核心库说的(&mut app, &这个条目, Field::Description).shown,
        裁决("手写的简介"),
        "放弃之后库里变了"
    );
    assert!(
        !有这一段(&屏上, "保存"),
        "放弃之后保存那一条还摆着：\n{屏上}"
    );
}

#[test]
fn 编辑时清空一格数据源给的值_保存不谎报已保存_库里一个字不动() {
    use romcat_core::scrape::Field;

    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 3);
    let work_id = 一个有得挑首选的作品(&mut app);
    let 这个条目 = 条目(&mut app, work_id);
    给简介摆两家说法(&mut app, &这个条目);
    let 用的 = 核心库说的(&mut app, &这个条目, Field::Description)
        .shown
        .expect("简介写出去不是空的");

    打开详情页(&ctx, &mut app, work_id, Tab::Metadata);
    按正好(&ctx, &mut app, "编辑");
    // 全选之后按退格：那一框清空了，可简介那一格没有手动修改可撤——它本来就是数据源给的值。
    选中简介那一框(&ctx, &mut app, &用的.values[0]);
    带着事件跑一帧(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::Backspace,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    带着事件跑一帧(&ctx, &mut app, Vec::new());
    按正好(&ctx, &mut app, "保存");

    assert_eq!(
        核心库说的(&mut app, &这个条目, Field::Description).shown,
        Some(用的),
        "清空一格数据源给的值，库里却变了"
    );
    let 回执 = app.browse().notice().unwrap_or_default().to_owned();
    assert!(
        !回执.contains("已保存 1 个字段") && 回执.starts_with("清空的 1 个字段本来就是数据源的值"),
        "清空一格数据源给的值，回执照旧说已保存：{回执}"
    );
}

#[test]
fn 编辑中连着用输入法打字不丢字() {
    use romcat_core::scrape::Field;
    use romcat_core::scrape::priority::{Said, VERDICT};

    // ADR-0005：正在输入时那一框不许被重建——组字到一半那一框换了身份，上屏的字就没人接。
    // 这里每个字都照输入法真发的那样分三帧：先组字「zh」，再组字那个字，再上屏；头一个字组下去那一帧，
    // 那一行冒出「已修改」、底下保存那一条的字也变了——框子要是跟着重建，后面几个字就丢了。
    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 3);
    let work_id = 一个有得挑首选的作品(&mut app);
    let 这个条目 = 条目(&mut app, work_id);
    给简介摆两家说法(&mut app, &这个条目);
    let 用的 = 核心库说的(&mut app, &这个条目, Field::Description)
        .shown
        .expect("简介写出去不是空的");

    打开详情页(&ctx, &mut app, work_id, Tab::Metadata);
    按正好(&ctx, &mut app, "编辑");
    选中简介那一框(&ctx, &mut app, &用的.values[0]);
    let 组字 = |text: &str| {
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: text.to_string(),
            active_range_chars: None,
        })
    };
    for 字 in ["中", "文", "输", "入", "法"] {
        带着事件跑一帧(&ctx, &mut app, vec![组字("zh")]);
        带着事件跑一帧(&ctx, &mut app, vec![组字(字)]);
        带着事件跑一帧(
            &ctx,
            &mut app,
            vec![egui::Event::Ime(egui::ImeEvent::Commit(字.to_string()))],
        );
    }
    按正好(&ctx, &mut app, "保存");
    assert_eq!(
        核心库说的(&mut app, &这个条目, Field::Description).shown,
        Some(Said {
            source: Some(VERDICT.to_string()),
            values: vec!["中文输入法".to_string()],
        }),
        "连着用输入法打的五个字没有一字不差地落进去"
    );
}

#[test]
fn 标题那一面列出标题集合_写明显示标题与排序标题怎么选出来_隐藏恢复添加() {
    use romcat_core::scrape::priority::VERDICT;

    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 3);
    let work_id = 一个有得挑首选的作品(&mut app);
    // 期望从核心库里取：这个作品叫什么、标题集合挑出来的显示标题与排序标题、集合里头一条。
    let (作品名, 挑的, 头一条) = {
        let (_, site) = app.browse_and_site();
        let work = site
            .catalog
            .work_detail(&WorkQuery::default(), &WorkAnchor::Work(work_id))
            .expect("读得出")
            .expect("有这个作品");
        let detail = site
            .catalog
            .variant_detail(&work.variants[0].row.key, &Priorities::builtin(), None)
            .expect("读得出")
            .expect("有这个变体");
        let 挑的 = detail.display.clone().expect("认出作品的挑得出显示标题");
        let 头一条 = detail
            .titles
            .first()
            .cloned()
            .expect("合成数据里每部作品都有叫法");
        (work.name, 挑的, 头一条)
    };
    assert!(!头一条.is_verdict(), "合成数据里一开始一条裁决叫法都没有");

    let 屏上 = 打开详情页(&ctx, &mut app, work_id, Tab::Titles);
    assert!(
        有这一段(&屏上, "显示标题") && 屏上.contains(&挑的.display),
        "没写显示标题是哪一个（{}）：\n{屏上}",
        挑的.display
    );
    assert!(
        屏上.lines().any(|line| line.starts_with("选取顺序：")),
        "没写显示标题是怎么选出来的：\n{屏上}"
    );
    assert!(
        有这一段(&屏上, "排序标题")
            && 屏上.contains(&挑的.sort_shown)
            && 屏上.contains(挑的.sort_from.label()),
        "没写排序标题（{}）与它从哪儿来（{}）：\n{屏上}",
        挑的.sort,
        挑的.sort_from.label()
    );
    // 列宽照稿按比例分：「名称」约占三成，其余几列均摊，「隐藏」贴右（协调人 2026-09-15 定）。
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 头一处 = |text: &str| {
        每一处画在哪儿(&这一帧, text)
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("标题那一面没画「{text}」"))
    };
    let (名称, 语言, 隐藏) = (头一处("名称"), 头一处("语言"), 头一处("隐藏"));
    let 名称占 = (语言.min.x - 名称.min.x) / (隐藏.max.x - 名称.min.x);
    assert!(
        (0.25..=0.36).contains(&名称占),
        "「名称」一列该约占三成，眼下占 {名称占:.2}：名称 {名称:?}，语言 {语言:?}，隐藏 {隐藏:?}"
    );
    for 表头 in ["名称", "语言", "类型", "来源", "使用的变体"] {
        assert!(
            有这一段(&屏上, 表头),
            "标题集合那张表没有「{表头}」那一列：\n{屏上}"
        );
    }
    assert!(
        有这一段(&屏上, &头一条.value)
            && 有这一段(&屏上, 头一条.language.label())
            && 有这一段(&屏上, 头一条.kind.label())
            && 有这一段(&屏上, &头一条.source),
        "标题集合头一条没列全（{} / {} / {} / {}）：\n{屏上}",
        头一条.value,
        头一条.language.label(),
        头一条.kind.label(),
        头一条.source
    );

    // ── 隐藏：压掉的那一条记进沉淀库（重新整理标题也不会把它加回来），底下列着、恢复得了。
    let 屏上 = 按正好(&ctx, &mut app, "隐藏");
    let 压了的 = {
        let (_, site) = app.browse_and_site();
        site.store
            .title_suppressions_of(&作品名)
            .expect("读得出沉淀库")
    };
    assert_eq!(
        压了的
            .iter()
            .map(|one| one.value.as_str())
            .collect::<Vec<_>>(),
        [头一条.value.as_str()],
        "按「隐藏」压掉的不是表上头一条"
    );
    let 屏上 = if 有这一段(&屏上, "已隐藏的名称 · 1") {
        屏上
    } else {
        带着事件跑一帧(&ctx, &mut app, Vec::new())
    };
    assert!(
        有这一段(&屏上, "已隐藏的名称 · 1") && 有这一段(&屏上, "恢复"),
        "隐藏之后底下没列出压掉的那一条：\n{屏上}"
    );
    按正好(&ctx, &mut app, "恢复");
    {
        let (_, site) = app.browse_and_site();
        assert!(
            site.store
                .title_suppressions_of(&作品名)
                .expect("读得出沉淀库")
                .is_empty(),
            "按「恢复」没撤掉那条压制"
        );
    }
    // 恢复之后就地摆着下一步：那条叫法要等下一趟整理标题才回到集合里，旁边那颗「整理标题」排的是与库屏工序段
    // 同一趟（排的那一下钉在 `tests/browse.rs` 的「撤掉一条标题压制之后…」那一条），不再叫人去开终端。
    let 屏上 = 滚到看得见(&ctx, &mut app, "整理标题");
    assert!(
        屏上.contains("撤掉了对") && !屏上.contains("romcat titles"),
        "恢复之后那句回执没摆出来，或者还在叫人去开终端：\n{屏上}"
    );

    // ── 添加一个名称：记为裁决。
    按正好(&ctx, &mut app, "添加一个名称");
    带着事件跑一帧(
        &ctx,
        &mut app,
        vec![egui::Event::Text("界面上加的名称".to_string())],
    );
    按正好(&ctx, &mut app, "添加");
    let (_, site) = app.browse_and_site();
    let 加的 = site
        .catalog
        .titles_of(&作品名)
        .expect("读得出")
        .into_iter()
        .find(|row| row.value == "界面上加的名称")
        .expect("加的那个名称进了标题集合");
    assert_eq!(加的.source, VERDICT, "加的名称没记为裁决");
}

/// 指针停进详情页正文，真发滚轮往下滚，直到画出**正好**写着 `那几个字` 的那一段，再等它停稳（连着两帧画在同一处）。
/// 交回停稳那一帧画出来的字。滚了四十下还没见着就当场炸，并印出最后那一帧。
fn 滚到看得见(ctx: &egui::Context, app: &mut App, 那几个字: &str) -> String {
    let 正文里 = egui::pos2(800.0, 500.0);
    let mut 上一处 = None;
    for _ in 0..40 {
        let 这一帧 = headless::frame(ctx, headless::input(), |ui| app.ui(ui));
        let 在 = 正好那一段画在哪儿(&这一帧, 那几个字);
        if 在.is_some() && 在 == 上一处 {
            return 画出来的字(&这一帧);
        }
        if 在.is_none() {
            let mut input = headless::input();
            input.events.push(egui::Event::PointerMoved(正文里));
            input.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -160.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
            headless::frame(ctx, input, |ui| app.ui(ui));
        }
        上一处 = 在;
    }
    let 最后 = headless::frame(ctx, headless::input(), |ui| app.ui(ui));
    panic!(
        "滚了四十下还没看见正好写着「{那几个字}」的地方：\n{}",
        画出来的字(&最后)
    );
}

/// 一张纯色 PNG 的字节。
fn png(width: u32, height: u32) -> Vec<u8> {
    let buf = image::RgbImage::from_pixel(width, height, image::Rgb([30, 90, 160]));
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(buf)
        .write_to(&mut out, image::ImageFormat::Png)
        .expect("编得出 PNG");
    out.into_inner()
}

/// 往池里落一份、中立库里记一条，交回内容哈希。
///
/// `measured` 是**入池那一刻量下来的**那三样（`scrape::measure`）：详情页媒体那一格
/// 底下那行照它写「来源 · 尺寸 · 时长 · 大小」。交一份空的就是「没量过」。
fn 入池(
    pool: &romcat_core::scrape::pool::MediaPool,
    catalog: &mut romcat_core::catalog::Catalog,
    bytes: &[u8],
    ext: &str,
    measured: romcat_core::scrape::measure::Measured,
) -> String {
    let (hash, _) = pool.take_bytes(bytes, ext).expect("落得进池");
    catalog
        .put_media(
            &hash,
            ext,
            u64::try_from(bytes.len()).unwrap_or(0),
            measured,
        )
        .expect("记得进库");
    hash
}

/// 一份**带真媒体池**的现场（照 `tests/media.rs` 那一份）：头一个认出的作品上挂着池里真有的一张封面、一份「视频」。
/// 交回界面、工作目录（得活到测试结束——媒体池在它里头）、那个作品的行号、那份视频在池里的落点。
fn 带媒体池的现场(
    tag: &str,
) -> (App, romcat_core::testing::TempDir, i64, std::path::PathBuf) {
    use romcat_core::catalog::scrape::{Harvested, HarvestedMedia};
    use romcat_core::scrape::pool::MediaPool;
    use romcat_core::scrape::{AnchorKind, MediaKind};

    let dir = romcat_core::testing::temp_dir(tag);
    let pool =
        MediaPool::open(&romcat_core::workspace::media_pool_dir(dir.path())).expect("开得出媒体池");
    let mut catalog = demo::browse(2_000).expect("造得出合成数据");
    let 一行 = catalog
        .work_page(&WorkQuery::default(), 0, 64)
        .expect("取得出一页")
        .into_iter()
        .find(|row| matches!(row.anchor, WorkAnchor::Work(_)))
        .expect("合成数据里该有认出了作品的行");
    let WorkAnchor::Work(work_id) = 一行.anchor else {
        unreachable!("上面挑的就是认出了作品的行");
    };
    let 封面 = 入池(
        &pool,
        &mut catalog,
        &png(600, 800),
        "png",
        romcat_core::scrape::measure::Measured {
            width: Some(600),
            height: Some(800),
            duration_ms: None,
        },
    );
    // 一份「视频」：字节是什么不要紧——抽帧那条路在拉进程那一步就该退化。
    //
    // **它的三样是量过的**：量尺在**入池**那一刻跑，而这台机器眼下没有 ffmpeg 只挡得住
    // **抽首帧**——库里记着的那三格照样读得回来。那正是换过机器之后真会看到的样子。
    let 片子 = 入池(
        &pool,
        &mut catalog,
        &[0x00; 256],
        "mp4",
        romcat_core::scrape::measure::Measured {
            width: Some(640),
            height: Some(480),
            duration_ms: Some(30_000),
        },
    );
    catalog
        .put_scraped(&[Harvested {
            anchor: AnchorKind::Work.label().to_string(),
            subject: 一行.name.clone(),
            source: "测试".to_string(),
            input: "测试指纹".to_string(),
            values: Vec::new(),
            media: vec![
                HarvestedMedia {
                    kind: MediaKind::Cover.label().to_string(),
                    hash: 封面,
                    evidence: "测试摆进去的".to_string(),
                },
                HarvestedMedia {
                    kind: MediaKind::Video.label().to_string(),
                    hash: 片子.clone(),
                    evidence: "测试摆进去的".to_string(),
                },
            ],
        }])
        .expect("写得进");
    let site = demo::site(catalog).expect("开得出现场");
    let mut app = App::new(site, dir.path().to_path_buf());
    app.show_view(View::Browse);
    let 片子在哪 = pool.path_of(&片子, "mp4");
    (app, dir, work_id, 片子在哪)
}

#[test]
fn 媒体那一面_没有ffmpeg时视频占位并说明原因_播放交给系统播放器() {
    let ctx = headless::context();
    let (mut app, _目录, work_id, 片子在哪) = 带媒体池的现场("gui-作品详情-媒体");
    // **不拉起真的播放器**：打开外部程序那一下换成记下交出去的是哪个文件。
    let 打开过 = std::sync::Arc::new(std::sync::Mutex::new(Vec::<std::path::PathBuf>::new()));
    {
        let (browse, _) = app.browse_and_site();
        // ffmpeg 那一条**不靠这台机器装没装**：抽帧那个程序名换成一个不存在的。
        browse
            .gallery_mut()
            .set_program(romcat_core::scrape::preview::NO_SUCH_PROGRAM);
        let 记下 = std::sync::Arc::clone(&打开过);
        browse.set_opener(move |path| {
            记下.lock().expect("锁得上").push(path.to_path_buf());
            Ok(())
        });
    }

    打开详情页(&ctx, &mut app, work_id, Tab::Media);
    // 后台那几件做完：抽首帧那一下在拉进程时就退化成占位。只跑帧、不等挂钟。
    for _ in 0..100_000 {
        跑(&ctx, &mut app, 1);
        let gallery = app.browse().gallery();
        if gallery.busy() == 0 && gallery.lacks_ffmpeg() {
            break;
        }
        std::thread::yield_now();
    }
    let 屏上 = 带着事件跑一帧(&ctx, &mut app, Vec::new());
    assert!(
        有这一段(&屏上, "没有找到 ffmpeg，无法生成预览帧")
            && 有这一段(&屏上, "视频文件完好，可以直接播放"),
        "没有 ffmpeg 时视频那一格没占位、没说原因：\n{屏上}"
    );
    assert!(
        有这一段(&屏上, "封面"),
        "媒体那一面没把封面一格一格列出来：\n{屏上}"
    );
    // 每一格底下写「来源 · 尺寸 · 时长 · 大小」（设计稿 `.mi .help`，票 34）。尺寸与时长是
    // **入池那一刻量下来的**（`scrape::measure`），大小是入池那份字节有多大。
    let 封面那一句 = format!(
        "测试 · {} · {}",
        romcat_core::report::pixel_size(600, 800),
        romcat_core::report::human_bytes(u64::try_from(png(600, 800).len()).unwrap_or(0))
    );
    // 媒体那几格在头上那一块底下，滚下去才看得见（egui 不画视口外的字）。
    let 屏上 = 滚到看得见(&ctx, &mut app, &封面那一句);
    assert!(
        有这一段(&屏上, &封面那一句),
        "封面那一格底下没写「{封面那一句}」：\n{屏上}"
    );
    // 合成数据给这个作品另挂着几份池里没有的封面：视频那一格排在后头，滚下去才看得见（egui 不画视口外的字）。
    //
    // 视频那一句**四段都在**：时长排在尺寸后面（设计稿 `mediaOf` 的 `640 × 480 · 0:30`）。
    // **这台机器没有 ffmpeg 只挡得住抽首帧**——那一格是占位，而库里记着的三格照样读得回来。
    let 视频那一句 = format!(
        "测试 · {} · {} · 256 B",
        romcat_core::report::pixel_size(640, 480),
        romcat_core::report::media_duration(30_000)
    );
    let 屏上 = 滚到看得见(&ctx, &mut app, "播放");
    assert!(
        有这一段(&屏上, "视频") && 有这一段(&屏上, &视频那一句),
        "媒体那一面没把视频一格一格列出来，或者底下没写「{视频那一句}」：\n{屏上}"
    );

    按正好(&ctx, &mut app, "播放");
    assert_eq!(
        *打开过.lock().expect("锁得上"),
        [片子在哪],
        "「播放」没把池里那份视频交给系统播放器"
    );
    assert!(
        app.browse().error().is_none(),
        "没有 ffmpeg 不该报错：{:?}",
        app.browse().error()
    );
}

#[test]
fn 头上那一块与概览照稿_简介基本信息媒体状态四块_编辑元数据进编辑态() {
    use romcat_core::report::capacity;
    use romcat_core::scrape::Field;

    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 3);
    let work_id = 一个有得挑首选的作品(&mut app);
    let 这个条目 = 条目(&mut app, work_id);
    给简介摆两家说法(&mut app, &这个条目);
    let 简介 = 核心库说的(&mut app, &这个条目, Field::Description)
        .shown
        .expect("简介写出去不是空的");
    // 期望从核心库的公开查询里取：显示标题与排序标题、逐个变体的识别结论、首选变体的简称、表上那一行的几个词。
    let (挑的, 识别那一句, 首选简称, 元数据, 置信度, 变体那一格) = {
        let (_, site) = app.browse_and_site();
        let work = site
            .catalog
            .work_detail(&WorkQuery::default(), &WorkAnchor::Work(work_id))
            .expect("读得出")
            .expect("有这个作品");
        let 详情 = site
            .catalog
            .variant_detail(&work.variants[0].row.key, &Priorities::builtin(), None)
            .expect("读得出")
            .expect("有这个变体");
        let mut 数: Vec<(&str, usize)> = Vec::new();
        for variant in &work.variants {
            let 词 = variant_state(variant);
            match 数.iter_mut().find(|(one, _)| *one == 词) {
                Some(one) => one.1 += 1,
                None => 数.push((词, 1)),
            }
        }
        let 识别那一句 = 数
            .iter()
            .map(|(词, n)| format!("{词} {n} 个"))
            .collect::<Vec<_>>()
            .join(" · ");
        let 简称们 = site
            .catalog
            .variant_short_names(&work, &Priorities::builtin())
            .expect("拼得出变体简称");
        let 首选 = 这个条目.2.clone().expect("头一个平台上有首选变体");
        let 首选简称 = work
            .variants
            .iter()
            .position(|variant| variant.row.key == 首选)
            .map(|at| 简称们[at].clone())
            .expect("首选变体在这个作品底下");
        let 那一行 = site
            .catalog
            .work_page_with_titles(&WorkQuery::default(), 0, 4_000, &Priorities::builtin())
            .expect("取得出一页")
            .into_iter()
            .find(|row| row.anchor == WorkAnchor::Work(work_id))
            .expect("表上有这一行");
        (
            详情.display.clone().expect("挑得出显示标题"),
            识别那一句,
            首选简称,
            那一行.meta_label(),
            那一行.confidence_label(),
            format!(
                "{} 个 · {}",
                那一行.variants,
                capacity(那一行.bytes, 那一行.unreadable_files)
            ),
        )
    };

    // 中文版本由核心库按作品答（与首选变体那条规则同一处判）；一个中文的都没有时照稿写「无」。
    let 中文版本 = {
        let (_, site) = app.browse_and_site();
        let work = site
            .catalog
            .work_detail(&WorkQuery::default(), &WorkAnchor::Work(work_id))
            .expect("读得出")
            .expect("有这个作品");
        site.catalog
            .work_chinese_mark(&work)
            .expect("答得出中文版本")
    };

    // 收藏：头一个变体收进收藏，状态块里「收藏」那一行照核心库那一问写（只读，拿主意的人 2026-09-15 定）。
    let 收藏那一句 = {
        use romcat_core::collection::{self, FAVORITE};

        let (_, site) = app.browse_and_site();
        let keys: Vec<String> = site
            .catalog
            .work_detail(&WorkQuery::default(), &WorkAnchor::Work(work_id))
            .expect("读得出")
            .expect("有这个作品")
            .variants
            .iter()
            .map(|variant| variant.row.key.clone())
            .collect();
        collection::add(site, FAVORITE, &keys[..1]).expect("加得进收藏");
        match collection::favorite_of(site, &keys).expect("问得出") {
            None => panic!("头一个变体刚收进收藏，核心库却说没收藏"),
            Some(romcat_core::verdict::ANCHOR_CONTENT) => "已收藏 · 按文件内容记录",
            Some(_) => "已收藏 · 只按路径记录：变体没有内容判据，文件改名或移动后会丢失",
        }
    };

    let 屏上 = 打开详情页(&ctx, &mut app, work_id, Tab::Overview);
    // ── 头上那一块：「平台」那一格照稿写全名，平台表里没写全名的写代号（全名由核心库的平台表给）。
    let 平台全名 = {
        let (_, site) = app.browse_and_site();
        let manifest = romcat_core::platform::Manifest::builtin();
        site.catalog
            .work_detail(&WorkQuery::default(), &WorkAnchor::Work(work_id))
            .expect("读得出")
            .expect("有这个作品")
            .platforms
            .iter()
            .map(|code| manifest.full_name(code).unwrap_or(code).to_owned())
            .collect::<Vec<_>>()
            .join(" / ")
    };
    assert!(
        有这一段(&屏上, &平台全名),
        "头上「平台」那一格没照平台表写「{平台全名}」：\n{屏上}"
    );
    // ── 头上那一块：它是什么、叫什么、哪个平台，几格事实，几枚标签（有中文版本时跟着一枚写它）。
    if let Some(mark) = 中文版本 {
        assert!(
            有这一段(&屏上, mark.label()),
            "头上那几枚标签里没写中文版本「{}」：\n{屏上}",
            mark.label()
        );
    }
    assert!(
        有这一段(&屏上, "作品")
            && 有这一段(&屏上, &挑的.display)
            && 有这一段(&屏上, &这个条目.1),
        "头上那一块没写它是作品、叫「{}」、在「{}」上：\n{屏上}",
        挑的.display,
        这个条目.1
    );
    for 格 in ["平台", "年份", "类型", "开发商", "发行商", "变体"] {
        assert!(
            有这一段(&屏上, 格),
            "头上那几格事实里没有「{格}」：\n{屏上}"
        );
    }
    assert!(
        有这一段(&屏上, &变体那一格),
        "变体那一格没写「{变体那一格}」：\n{屏上}"
    );
    assert!(
        有这一段(&屏上, 置信度) && 有这一段(&屏上, &format!("元数据：{元数据}")),
        "头上那几枚标签没写置信度「{置信度}」与「元数据：{元数据}」：\n{屏上}"
    );
    assert!(
        有这一段(&屏上, "编辑元数据") && 有这一段(&屏上, "刮削此作品"),
        "头上那一排按钮不全：\n{屏上}"
    );

    // ── 概览：简介、基本信息、媒体、状态四块。
    for 块 in ["简介", "基本信息", "媒体", "状态"] {
        assert!(有这一段(&屏上, 块), "概览里没有「{块}」那一块：\n{屏上}");
    }
    assert!(
        有这一段(&屏上, &简介.values[0]),
        "简介那一块没写眼下的简介：\n{屏上}"
    );
    assert!(
        有这一段(&屏上, "识别") && 有这一段(&屏上, &识别那一句),
        "状态那一块没写逐个变体的识别结论「{识别那一句}」：\n{屏上}"
    );
    let 屏上 = 滚到看得见(&ctx, &mut app, 收藏那一句);
    assert!(
        有这一段(&屏上, "收藏") && 有这一段(&屏上, 收藏那一句),
        "状态块里没写收藏那一行「{收藏那一句}」：\n{屏上}"
    );
    let 屏上 = 滚到看得见(&ctx, &mut app, "首选变体");
    // 基本信息里带来源徽标的那几行：名与值按头一行对齐（岔路口 3 选 A）。挑一格合成数据里有值的。
    let (那一格, 那一格的值) = [
        Field::Year,
        Field::Genre,
        Field::Developer,
        Field::Publisher,
    ]
    .into_iter()
    .find_map(|field| {
        核心库说的(&mut app, &这个条目, field)
            .shown
            .map(|said| (field, said.values.join("、")))
    })
    .expect("合成数据里这几格总有一格有值");
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 名们 = 每一处画在哪儿(&这一帧, 那一格.label());
    let 值们 = 每一处画在哪儿(&这一帧, &那一格的值);
    assert!(
        名们.iter().any(|名| 值们
            .iter()
            .any(|值| 值.min.x > 名.max.x && (值.min.y - 名.min.y).abs() <= 1.5)),
        "基本信息里「{}」那一行名与值没按头一行对齐：名 {名们:?}，值 {值们:?}",
        那一格.label()
    );
    let 中文那一格 = 中文版本.map_or("无", |mark| mark.label());
    assert!(
        有这一段(&屏上, "中文版本") && 有这一段(&屏上, 中文那一格),
        "基本信息里没写中文版本「{中文那一格}」：\n{屏上}"
    );
    assert!(
        有这一段(&屏上, &首选简称) && 屏上.contains(&挑的.sort_shown),
        "基本信息里没写首选变体「{首选简称}」或排序标题「{}」（照原样大小写）：\n{屏上}",
        挑的.sort_shown
    );

    // ── 「编辑元数据」：换到元数据那一面，直接进编辑态。
    按正好(&ctx, &mut app, "← 返回浏览");
    let 屏上 = 打开详情页(&ctx, &mut app, work_id, Tab::Overview);
    assert!(有这一段(&屏上, "编辑元数据"));
    let 屏上 = 按正好(&ctx, &mut app, "编辑元数据");
    assert_eq!(
        app.browse().page().map(|page| page.tab()),
        Some(Tab::Metadata)
    );
    assert!(
        有这一段(&屏上, "还没有修改。"),
        "「编辑元数据」没直接进编辑态：\n{屏上}"
    );
}

#[test]
fn 作品详情页开着时窗口标题带上作品名_关掉回到浏览() {
    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 3);
    let (_, 名字) = 表上一个作品(&mut app);
    let anchor = {
        let (_, site) = app.browse_and_site();
        site.catalog
            .work_page_with_titles(&WorkQuery::default(), 0, 8, &Priorities::builtin())
            .expect("取得出一页")
            .into_iter()
            .find(|row| row.display.as_deref().unwrap_or(row.name.as_str()) == 名字)
            .map(|row| row.anchor)
            .expect("表上有这一行")
    };
    let 发出去的标题 = |output: &egui::FullOutput| -> Vec<String> {
        output
            .viewport_output
            .values()
            .flat_map(|viewport| viewport.commands.iter())
            .filter_map(|command| match command {
                egui::ViewportCommand::Title(title) => Some(title.clone()),
                _ => None,
            })
            .collect()
    };

    {
        let (browse, site) = app.browse_and_site();
        browse.open_work(&site.catalog, &anchor);
        browse.open_page(Tab::Overview);
    }
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 要的 = format!("浏览 — {名字}");
    assert!(
        发出去的标题(&这一帧)
            .iter()
            .any(|title| title.ends_with(&要的)),
        "打开详情页那一帧没把窗口标题换成带作品名的：{:?}",
        发出去的标题(&这一帧)
    );
    assert!(
        app.window_title().ends_with(&要的),
        "{}",
        app.window_title()
    );

    {
        let (browse, _) = app.browse_and_site();
        browse.close_page();
    }
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    assert!(
        发出去的标题(&这一帧)
            .iter()
            .any(|title| title.ends_with("— 浏览")),
        "关掉详情页那一帧窗口标题没换回浏览：{:?}",
        发出去的标题(&这一帧)
    );
}

#[test]
fn 在文件系统中打开交给系统的是那个变体在盘上所在的目录() {
    use romcat_core::catalog::identify::{Identification, Provenance};
    use romcat_core::catalog::{Catalog, State};
    use romcat_core::platform::Manifest;
    use romcat_core::shape::{Role, SINGLE_FILE_RULE, Variant};
    use romcat_core::site::Site;
    use romcat_core::verdict::Store;

    // 一份**盘上真有文件**的小库：根指到临时目录里的「盘」，底下 `SFC/幻想传说.zip` 真的在。
    let 目录 = romcat_core::testing::temp_dir("gui-作品详情-文件系统");
    let 盘 = 目录.path().join("盘");
    std::fs::create_dir_all(盘.join("SFC")).expect("建得出目录");
    std::fs::write(盘.join("SFC").join("幻想传说.zip"), b"PK").expect("写得下文件");
    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    romcat_core::catalog::roots::add_root(&catalog, None, "主库", &盘).expect("建得出根");
    let key = "主库/SFC/幻想传说.zip".to_string();
    catalog
        .replace_variants(
            &[Variant {
                main_key: key.clone(),
                platform: Some("SFC".to_string()),
                rule: SINGLE_FILE_RULE.to_string(),
                manual: false,
                files: 1,
                bytes: 2,
                unreadable_files: 0,
                members: vec![(key.clone(), Role::Main)],
                key: key.clone(),
            }],
            1,
            &Manifest::default(),
        )
        .expect("写得进变体");
    let work_id = catalog
        .add_work("幻想传说", Provenance::Identified)
        .expect("建得出作品");
    catalog
        .write_identifications(&[Identification {
            variant_key: key,
            platform: None,
            edition: None,
            standalone: None,
            state: State::Matched,
            reason: None,
            units: 1,
            nkit: 0,
            read_bytes: 0,
            work_id: Some(work_id),
            release_id: None,
            candidates: Vec::new(),
        }])
        .expect("写得进结论");
    let site = Site::in_memory(catalog, Store::in_memory().expect("开得出沉淀库"), "主库");
    let mut app = App::new(site, 目录.path().to_path_buf());
    app.show_view(View::Browse);
    let 打开过 = std::sync::Arc::new(std::sync::Mutex::new(Vec::<std::path::PathBuf>::new()));
    {
        let (browse, _) = app.browse_and_site();
        let 记下 = std::sync::Arc::clone(&打开过);
        browse.set_opener(move |path| {
            记下.lock().expect("锁得上").push(path.to_path_buf());
            Ok(())
        });
    }
    let ctx = headless::context();
    跑(&ctx, &mut app, 2);

    打开详情页(&ctx, &mut app, work_id, Tab::Variants);
    按正好(&ctx, &mut app, "在文件系统中打开");
    assert_eq!(
        *打开过.lock().expect("锁得上"),
        [盘.join("SFC")],
        "「在文件系统中打开」交出去的不是那个变体在盘上所在的目录"
    );
}

/// 这一帧里**正好**写着这几个字的每一段画在哪儿（外框），按画出来的次序。
fn 每一处画在哪儿(output: &egui::FullOutput, 那几个字: &str) -> Vec<egui::Rect> {
    fn 找(shape: &egui::epaint::Shape, 那几个字: &str, out: &mut Vec<egui::Rect>) {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == 那几个字 => {
                out.push(egui::Rect::from_min_size(text.pos, text.galley.size()));
            }
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    找(one, 那几个字, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        找(&clipped.shape, 那几个字, &mut out);
    }
    out
}

#[test]
fn 变体卡上第几版那一格照核心库写_说不出时写破折号() {
    // 票 `gui-looks-like-the-design/34`，词表**第几版**：屏上那一格照设计稿写「版本」。
    // **判断不在界面**——两层怎么挑由 `VariantDetail::edition` 一处判，这里只核对
    // 「核心库说什么、屏上就写什么」。
    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 3);
    let work_id = 一个有得挑首选的作品(&mut app);

    // 合成数据里一条修订标记都没有，也没人裁过——那一格该是「—」。
    let 屏上 = 打开详情页(&ctx, &mut app, work_id, Tab::Variants);
    assert!(
        有这一段(&屏上, "版本"),
        "变体卡上该有「版本」那一行（设计稿 `.vbody` 那张 dl.infol 的最后一格）：\n{屏上}"
    );

    // 给头一个变体记一条**裁决说的那一版**，屏上那一格跟着写它。
    let 头一个 = {
        let (browse, site) = app.browse_and_site();
        browse.open_work(&site.catalog, &WorkAnchor::Work(work_id));
        let 键 = browse
            .work()
            .expect("点开了")
            .variants
            .first()
            .expect("底下有变体")
            .row
            .key
            .clone();
        记一条裁决说的第几版(site, &键, "v1.2");
        键
    };
    assert!(!头一个.is_empty());
    let 屏上 = 打开详情页(&ctx, &mut app, work_id, Tab::Variants);
    assert!(
        有这一段(&屏上, "v1.2"),
        "裁决说了第几版，那一格就该写它（词表**第几版**：裁决 > 发行版的修订 > 说不出）：\n{屏上}"
    );
}

/// 往识别结论上记一条**裁决说的第几版**（`identification.edition`）。
///
/// 走 `write_identifications` 那条正门：那一列的写者只有它，测试另开一条路就等于
/// 绕过被测的那段代码。
fn 记一条裁决说的第几版(site: &mut romcat_core::site::Site, key: &str, edition: &str) {
    use romcat_core::catalog::State;
    use romcat_core::catalog::identify::Identification;

    let 原来的 = site
        .catalog
        .variant(key)
        .expect("读得出")
        .expect("有这个变体");
    site.catalog
        .write_identifications(&[Identification {
            variant_key: key.to_owned(),
            platform: None,
            standalone: None,
            edition: Some(edition.to_owned()),
            state: State::Matched,
            reason: None,
            units: 1,
            nkit: 0,
            read_bytes: 0,
            work_id: 原来的.work_id,
            release_id: 原来的.release_id,
            candidates: Vec::new(),
        }])
        .expect("写得进");
}

#[test]
fn 状态块上子库与导出两行照核心库写_都答不出时说没有() {
    // 票 `gui-looks-like-the-design/34`：状态块「子库」「导出」两行。
    // **两样的判断都在核心库**——落在哪几个子库里走求值那一处（`sublibrary::holding`，
    // 与同步那一趟同一个函数），上次几点写出去的读导出那一趟逐条记下的账。
    use romcat_core::scrape::AnchorKind;
    use romcat_core::sublibrary::{Rule, Sublibrary};

    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 3);
    let work_id = 一个有得挑首选的作品(&mut app);

    // 一、一个子库都没有、一趟也没导过：两行照实说。
    let 屏上 = 打开详情页(&ctx, &mut app, work_id, Tab::Overview);
    assert!(
        有这一段(&屏上, "子库"),
        "状态块该有「子库」那一行：\n{屏上}"
    );
    assert!(
        有这一段(&屏上, "还没导出"),
        "一趟都没导过时照实说「还没导出」，不拿整库那个时刻顶上去：\n{屏上}"
    );

    // 二、建一个收得住它的子库、再记一趟导出。
    let (作品名, 平台) = {
        let (browse, site) = app.browse_and_site();
        browse.open_work(&site.catalog, &WorkAnchor::Work(work_id));
        let work = browse.work().expect("点开了");
        let 平台 = work
            .variants
            .iter()
            .find_map(|variant| variant.row.platform.clone())
            .expect("底下总有一个说得出平台的变体");
        (work.name.clone(), 平台)
    };
    {
        let (_, site) = app.browse_and_site();
        site.catalog
            .put_sublibrary(&Sublibrary {
                name: "掌机".to_owned(),
                target: "/Volumes/SDCARD/掌机".to_owned(),
                target_raw: Some("/Volumes/SDCARD/掌机".to_owned()),
                format: "Pegasus".to_owned(),
                capacity: None,
                capability: None,
                capacity_by_device: false,
            })
            .expect("子库写得进");
        site.catalog
            .add_rule(
                "掌机",
                &Rule::parse(&format!("平台={平台}")).expect("规则读得懂"),
            )
            .expect("规则写得进");
        site.catalog
            .mark_exported(
                1,
                false,
                "Pegasus",
                &[romcat_core::catalog::export::ExportedEntry {
                    anchor: AnchorKind::Work,
                    subject: 作品名.clone(),
                    platform: 平台.clone(),
                }],
            )
            .expect("记得下");
    }
    // 重开一次详情页：`open_page` 造的是一份新的 `Page`，于是这一帧照库里现在的样子重读。
    let 屏上 = 打开详情页(&ctx, &mut app, work_id, Tab::Overview);
    assert!(
        有这一段(&屏上, "掌机"),
        "规则收得住它，「子库」那一行就该列出这个子库：\n{屏上}"
    );
    let 那一趟 = {
        let (_, site) = app.browse_and_site();
        site.catalog.exported_at().expect("读得出").expect("打上了")
    };
    let 导出那一句 = format!(
        "已导出到 Pegasus · {}",
        romcat_core::report::human_time(那一趟)
    );
    assert!(
        有这一段(&屏上, &导出那一句),
        "导出那一趟逐条记下的账里有它，这一行就该写「{导出那一句}」：\n{屏上}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 成型纠正（票 `gui-looks-like-the-design/29`）：从**变体**那一面进得去，也撤得掉
// ═══════════════════════════════════════════════════════════════════════════

/// 一份真落在临时目录里的小库：`FDS/某游戏/` 底下两面磁碟，FDS 没声明多碟同族，两面各成一个变体
/// ——**多碟没合在一起**。**一个字节都不碰真盘。**
fn 有多碟存疑的库() -> (
    App,
    romcat_core::testing::TempDir,
    romcat_core::testing::TempDir,
) {
    use romcat_core::catalog::Catalog;
    use romcat_core::catalog::roots;
    use romcat_core::fs::RealFs;
    use romcat_core::scan::{self, Jobs, ScanOptions};
    use romcat_core::site::Site;
    use romcat_core::task::Handle;
    use romcat_core::testing::temp_dir;

    let 工作区 = temp_dir("gui-work-shaping-ws");
    let 盘 = temp_dir("gui-work-shaping-disk");
    for (相对, 字节) in [
        ("FDS/某游戏/某游戏 (Disk 1).fds", vec![1_u8; 64]),
        ("FDS/某游戏/某游戏 (Disk 2).fds", vec![2_u8; 64]),
    ] {
        let 落点 = 盘.path().join(相对);
        std::fs::create_dir_all(落点.parent().expect("有上级目录")).expect("建得出目录");
        std::fs::write(&落点, 字节).expect("写得进");
    }
    let 库文件 = 工作区.path().join("catalog").join("fixture.sqlite3");
    drop(Catalog::create(&库文件, "fixture").expect("建得出中立库"));
    let mut site = Site::open_file(工作区.path(), &库文件, None).expect("开得出现场");
    let 目录 = romcat_core::path::normalize_existing(盘.path());
    roots::add_root(&site.catalog, Some(工作区.path()), "主库", &目录).expect("加得上根");
    let mut options = ScanOptions::named(&目录, "主库");
    options.workspace = Some(工作区.path().to_path_buf());
    options.jobs = Jobs::Fixed(1);
    scan::scan(&RealFs::new(), &mut site.catalog, &options, &Handle::new()).expect("扫得完");
    let mut app = App::new(site, 工作区.path().to_path_buf());
    app.show_view(View::Browse);
    (app, 工作区, 盘)
}

/// 打开第一面写着这个键的那一行的**作品详情页**，停在「变体与文件」那一面。
fn 开变体那一面(app: &mut App, key: &str) {
    let (browse, site) = app.browse_and_site();
    browse.open_work(&site.catalog, &WorkAnchor::Loose(key.to_string()));
    browse.open_page(Tab::Variants);
}

/// 库里眼下有哪几个变体：（键, 是不是人工纠正出来的），按键排。
fn 库里的变体(app: &mut App) -> Vec<(String, bool)> {
    let (_, site) = app.browse_and_site();
    let mut out: Vec<(String, bool)> = site
        .catalog
        .variants()
        .expect("读得出变体")
        .into_iter()
        .map(|row| (row.key, row.manual))
        .collect();
    out.sort();
    out
}

#[test]
fn 作品详情变体那一面也进得去成型纠正_合成之后那张卡撤得掉_撤完回到规则原本的结果() {
    // 票 29 验收第 1 条后半句（**从作品详情的变体那一面也进得去**）与第 5 条
    // （**能撤销，撤销后回到规则原本的结果**）。
    let ctx = headless::context();
    let (mut app, _工作区, _盘) = 有多碟存疑的库();
    开变体那一面(&mut app, "主库/FDS/某游戏/某游戏 (Disk 1).fds");
    跑(&ctx, &mut app, 2);

    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| app.ui(ui)));
    assert!(
        屏上.contains("多碟没合在一起") && 屏上.contains("2 个变体只差碟片标记"),
        "变体那一面没说这一处成型存疑（核心库那一句）：\n{屏上}"
    );
    assert!(
        有这一段(&屏上, "调整成型…"),
        "变体那一面进不去成型纠正：\n{屏上}"
    );

    let 屏上 = 点一下(&ctx, "调整成型…", |ui| app.ui(ui));
    assert!(
        有这一段(&屏上, "调整成型 · 某游戏"),
        "「调整成型…」开的不是那一层：\n{屏上}"
    );
    assert!(
        屏上.contains("记为人工纠正") && !屏上.contains("记为裁决"),
        "屏上该说「记为人工纠正」：\n{屏上}"
    );

    点一下(&ctx, "合成一个变体", |ui| app.ui(ui));
    等任务台空了(&mut app);
    跑(&ctx, &mut app, 3);
    assert_eq!(
        库里的变体(&mut app),
        vec![("主库/FDS/某游戏/某游戏 (Disk 1).fds".to_string(), true)],
        "两面磁碟没合成一个人工纠正出来的变体"
    );

    // ——— 撤销：那张卡上一颗，撤完回到成型规则原本的结果 ———
    开变体那一面(&mut app, "主库/FDS/某游戏/某游戏 (Disk 1).fds");
    跑(&ctx, &mut app, 2);
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| app.ui(ui)));
    assert!(
        有这一段(&屏上, "撤销成型纠正"),
        "人工纠正出来的那张卡上没有撤销：\n{屏上}"
    );

    点一下(&ctx, "撤销成型纠正", |ui| app.ui(ui));
    等任务台空了(&mut app);
    跑(&ctx, &mut app, 3);
    assert_eq!(
        库里的变体(&mut app),
        vec![
            ("主库/FDS/某游戏/某游戏 (Disk 1).fds".to_string(), false),
            ("主库/FDS/某游戏/某游戏 (Disk 2).fds".to_string(), false),
        ],
        "撤销之后没回到成型规则原本的结果（两面磁碟各自成一个变体）"
    );
}
