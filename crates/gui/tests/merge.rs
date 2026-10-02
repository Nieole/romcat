//! **合并作品**与**移出此作品**（票 `gui-looks-like-the-design/16`，设计稿 `renderMW` 与 `DLG.split`）。
//!
//! 断言看的是**这一帧真画出来的字**（`shared::画出来的字`）与**库里真的变了没有**，
//! 不看界面里头的结构、也不看「第几行是谁」。

use romcat_core::catalog::browse::{WorkAnchor, WorkQuery};
use romcat_core::catalog::identify::{Candidate, Identification, Provenance};
use romcat_core::catalog::{Catalog, Confidence, NewRelease, State};
use romcat_core::dat::Convention;
use romcat_core::platform::Manifest;
use romcat_core::scrape::{AnchorKind, Field};
use romcat_core::shape::{Role, SINGLE_FILE_RULE, Variant};
use romcat_core::site::Site;
use romcat_core::verdict::Store;
use romcat_gui::app::{App, View};
use romcat_gui::browse::work::Tab;
use romcat_gui::headless;

mod shared;
use shared::{
    干净工作目录, 正好那一段画在哪儿, 滚到底, 点一下, 画出来的字, 画着的每一处, 跑一帧
};

const 根: &str = "主库";
/// 保留的那个作品：底下两个变体，所以它是默认推荐保留的那一个。
const 甲: &str = "口袋妖怪 红";
/// 被合并的那个作品：底下一个变体。
const 乙: &str = "精灵宝可梦 红";
/// 完全不参与的那个作品，用来证明「至少两个」与「别的作品没被动到」。
const 丙: &str = "幻想传说";

fn 键(平台: &str, 名字: &str) -> String {
    format!("{根}/{平台}/{名字}")
}

fn 变体(平台: &str, 名字: &str) -> Variant {
    let key = 键(平台, 名字);
    Variant {
        main_key: key.clone(),
        platform: Some(平台.to_string()),
        rule: SINGLE_FILE_RULE.to_string(),
        manual: false,
        files: 1,
        bytes: 4_096,
        unreadable_files: 0,
        members: vec![(key.clone(), Role::Main)],
        key,
    }
}

fn 候选(variant: &Variant, 条目: &str, 汉化: bool) -> Candidate {
    Candidate {
        member_key: variant.main_key.clone(),
        inner: String::new(),
        confidence: Confidence::High,
        accepted: true,
        source: "合成".to_string(),
        dat: "合成.dat".to_string(),
        platform: variant.platform.clone().unwrap_or_default(),
        game: 条目.to_string(),
        rom: "rom.bin".to_string(),
        hashed_as: Convention::AsIs,
        dat_convention: Convention::AsIs,
        evidence: "精确哈希命中".to_string(),
        chinese: 汉化.then_some(romcat_core::dat::chinese::ChineseMark::FanTranslated),
        serial: None,
        release_id: None,
    }
}

/// 三个作品：甲两个变体（GB）、乙一个（GB）、丙一个（SFC）。
/// 甲有简介没年份，乙两样都有——**第三步只该列出有冲突的那几个字段**。
fn 建库() -> Catalog {
    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    romcat_core::catalog::roots::add_root(
        &catalog,
        None,
        根,
        std::path::Path::new(&format!("/{根}")),
    )
    .expect("建得出根");
    let 手上的 = [
        (甲, "GB", "口袋妖怪 红(汉化).zip"),
        (甲, "GB", "口袋妖怪 红(汉化 修正).zip"),
        (乙, "GB", "精灵宝可梦 红.zip"),
        (丙, "SFC", "幻想传说 汉化版.zip"),
    ];
    let variants: Vec<Variant> = 手上的
        .iter()
        .map(|(_, 平台, 名字)| 变体(平台, 名字))
        .collect();
    catalog
        .replace_variants(&variants, 1, &Manifest::default())
        .expect("写得进变体");

    let mut 结论 = Vec::new();
    for ((作品, 平台, _), variant) in 手上的.iter().zip(&variants) {
        let work = match catalog.work_named(作品).expect("读得动") {
            Some(id) => id,
            None => catalog
                .add_work(作品, Provenance::Identified)
                .expect("建得出作品"),
        };
        let release = catalog
            .add_release(
                work,
                &NewRelease {
                    platform: Some(平台),
                    region: Some("Japan"),
                    serial: Some("DMG-APAJ"),
                    languages: Some("Ja"),
                    revision: None,
                },
                Provenance::Identified,
            )
            .expect("建得出发行版");
        结论.push(Identification {
            variant_key: variant.key.clone(),
            platform: Some((*平台).to_string()),
            standalone: None,
            edition: None,
            state: State::Matched,
            reason: None,
            units: 1,
            nkit: 0,
            read_bytes: 0,
            work_id: Some(work),
            release_id: Some(release),
            candidates: vec![候选(variant, 作品, *作品 == 乙)],
        });
    }
    catalog.write_identifications(&结论).expect("写得进结论");

    for (作品, 字段, 值) in [
        (甲, Field::Description, "口袋妖怪 红的简介"),
        (乙, Field::Description, "精灵宝可梦 红的简介"),
        (乙, Field::Year, "1996"),
        (甲, Field::Developer, "Game Freak"),
        (乙, Field::Developer, "Game Freak"),
    ] {
        catalog
            .put_verdict_value(AnchorKind::Work, 作品, 字段, 值, "夹具")
            .expect("写得进刮削值");
    }
    catalog
}

fn 界面(名字: &str) -> App {
    let site = Site::in_memory(建库(), Store::in_memory().expect("开得出沉淀库"), 根);
    let mut app = App::new(site, 干净工作目录(名字));
    app.show_view(View::Browse);
    app
}

/// 这个作品在表上那一行的身份。
fn 那一行(app: &mut App, 作品: &str) -> WorkAnchor {
    let (_, site) = app.browse_and_site();
    let id = site
        .catalog
        .work_named(作品)
        .expect("读得动")
        .expect("库里有这个作品");
    WorkAnchor::Work(id)
}

/// 勾上这几个作品。走的是选中那一份状态（`Screen::picked_mut`），与在表上点勾选框同一处。
fn 勾上(app: &mut App, 作品们: &[&str]) {
    let 行: Vec<WorkAnchor> = 作品们.iter().map(|名字| 那一行(app, 名字)).collect();
    let (browse, _) = app.browse_and_site();
    for anchor in 行 {
        browse.picked_mut().toggle(&anchor);
    }
}

/// 这个变体眼下挂在哪个作品名下；没挂上是 `None`。
fn 挂在(app: &mut App, key: &str) -> Option<String> {
    let (_, site) = app.browse_and_site();
    let row = site
        .catalog
        .variant(key)
        .expect("读得动")
        .expect("有这一行");
    row.work_id.map(|id| {
        site.catalog
            .work_name(id)
            .expect("读得动")
            .expect("作品还在")
    })
}

/// 这一帧里有没有**正好**是这几个字的一段。
fn 有这一段(屏上: &str, 那几个字: &str) -> bool {
    屏上.lines().any(|line| line == 那几个字)
}

/// 点屏上**正好**写着这几个字的地方。
///
/// 共用那个 [`点一下`] 认的是「含有」，而这一票里「移出」正好是「移出此作品…」的一截——
/// 按含有找，点到的是弹层后头那颗按钮。
fn 正好点一下(
    ctx: &egui::Context,
    那几个字: &str,
    mut 画一帧: impl FnMut(&mut egui::Ui),
) -> String {
    let 头一帧 = headless::frame(ctx, headless::input(), &mut 画一帧);
    let Some(位置) = 正好那一段画在哪儿(&头一帧, 那几个字) else {
        panic!(
            "屏上没有正好写着「{那几个字}」的地方，没处点：\n{}",
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
    headless::frame(ctx, input, &mut 画一帧);
    let mut input = headless::input();
    input.events.push(按(false));
    headless::frame(ctx, input, &mut 画一帧);
    跑一帧(ctx, 画一帧)
}

fn 跑(ctx: &egui::Context, app: &mut App, frames: u32) {
    for _ in 0..frames {
        headless::frame(ctx, headless::input(), |ui| app.ui(ui));
    }
}

/// 跑到画稳为止，交出稳住那一帧画出来的字。
///
/// **弹层多高要两帧才定得下来**：页脚多高是上一帧量出来的（`crate::dialog` 那一层），
/// 头一帧页脚还在用默认值，内容区因此占满、页脚落到窗口外——那一帧里页脚一颗按钮都没画。
/// 这不是缺陷，是那一层自己说的「照上一帧的尺寸居中」；测试等它稳住再断言。
fn 稳一稳(ctx: &egui::Context, app: &mut App) -> String {
    跑(ctx, app, 5);
    跑一帧(ctx, |ui| app.ui(ui))
}

/// 勾上甲乙、按「合并作品…」、走到第几步。交回停在那一步、画稳之后那一帧画出来的字。
fn 走到第几步(ctx: &egui::Context, app: &mut App, 第几步: usize) -> String {
    勾上(app, &[甲, 乙]);
    跑(ctx, app, 2);
    点一下(ctx, "合并作品…", |ui| app.ui(ui));
    for _ in 1..第几步 {
        稳一稳(ctx, app);
        点一下(ctx, "下一步", |ui| app.ui(ui));
    }
    稳一稳(ctx, app)
}

#[test]
fn 勾两个作品按合并作品开出三步向导_默认保留变体多的那一个() {
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-开向导");
    let 屏上 = 走到第几步(&ctx, &mut app, 1);

    assert!(有这一段(&屏上, "合并作品"), "向导没开出来：\n{屏上}");
    for 步 in ["选择作品", "核对变体", "确认合并"] {
        assert!(有这一段(&屏上, 步), "三步那一排里没有「{步}」：\n{屏上}");
    }
    assert!(
        屏上.contains("选择要保留的作品"),
        "第一步那句说明没画出来：\n{屏上}",
    );
    assert!(有这一段(&屏上, 甲), "参与的两个作品都该列出来：\n{屏上}");
    assert!(有这一段(&屏上, 乙), "参与的两个作品都该列出来：\n{屏上}");
    assert!(
        有这一段(&屏上, "保留"),
        "没标出默认保留的是哪一个：\n{屏上}"
    );
    assert!(
        屏上.contains("GB · 年份未知 · 2 个变体") && 屏上.contains("GB · 1996 · 1 个变体"),
        "每一行该照稿写清平台、年份与变体数：\n{屏上}",
    );

    // **默认保留变体多的那一个**（甲底下两个、乙底下一个）。断在**它真会做什么**上，
    // 不断在「第几行画着『保留』」——屏上两处都写着作品名，按行序断会时红时绿。
    点一下(&ctx, "下一步", |ui| app.ui(ui));
    稳一稳(&ctx, &mut app);
    点一下(&ctx, "下一步", |ui| app.ui(ui));
    稳一稳(&ctx, &mut app);
    let 第三步 = 滚到底(&ctx, |ui| app.ui(ui));
    assert!(
        第三步.contains(&format!("只把作品改为「{甲}」")),
        "默认保留的不是变体多的那一个：\n{第三步}",
    );
}

#[test]
fn 第一步保留那枚标签紧跟在作品名后头_底下才是平台年份那一行() {
    // 设计稿 `.mwit`：`<b>${w.t}</b>${k?' <span class="keepb">保留</span>':''}`，底下 `.help` 才是
    // 「平台 · 年份 · 几个变体 · 置信度」——标签接在作品名后头、与名字同一行（挂单 `Q1015`）。
    // 从前它摆在「圆点 + 名字 + 说明」那一整块后头，离名字隔着整句说明那么远、落在两行之间。
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-保留紧跟作品名");
    走到第几步(&ctx, &mut app, 1);
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 标签们 = 画着的每一处(&这一帧, &|字| 字 == "保留");
    let [标签] = 标签们.as_slice() else {
        panic!(
            "第一步该正好有一枚「保留」：{标签们:?}\n{}",
            画出来的字(&这一帧)
        );
    };
    // 名字与标签之间隔 `.opt` 那一格（`option-gap`）；标签的字左边还有它自己那一份留白（`tag-padding`）。
    let 版式 = &romcat_gui::tokens::Tokens::builtin().layout;
    let 缝 = 版式.option_gap + 版式.tag_padding;
    let 名字们 = 画着的每一处(&这一帧, &|字| 字 == 甲);
    assert!(
        名字们.iter().any(|名字| {
            (名字.center().y - 标签.center().y).abs() < 1.0
                && 标签.left() >= 名字.right()
                && 标签.left() - 名字.right() <= 缝 + 1.0
        }),
        "「保留」该紧跟在「{甲}」后头、同一行：标签 {标签:?}，作品名画在 {名字们:?}",
    );
    // 置信度那一个词由核心库答，这里只认它前头那几段。
    let 说明们 = 画着的每一处(&这一帧, &|字| {
        字.starts_with("GB · 年份未知 · 2 个变体 · ")
    });
    let [说明] = 说明们.as_slice() else {
        panic!("甲那一行底下的说明：{说明们:?}\n{}", 画出来的字(&这一帧));
    };
    assert!(
        标签.bottom() <= 说明.top() + 1.0,
        "标签在名字那一行，说明在它底下：标签 {标签:?}，说明 {说明:?}",
    );
}

#[test]
fn 只勾一个作品时不开向导_屏上说清要勾两个以上() {
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-勾得不够");
    勾上(&mut app, &[甲]);
    跑(&ctx, &mut app, 2);
    let 屏上 = 点一下(&ctx, "合并作品…", |ui| app.ui(ui));
    assert!(
        !有这一段(&屏上, "选择作品"),
        "只勾了一个，不该开出向导：\n{屏上}",
    );

    // **那句回执落在下一帧**（票 `gui-looks-like-the-design/12`，挂单 `Q1102`）：
    // 这颗按钮从屏头挪到了表格上方那一条，而屏头是 `app.rs` 在 `Screen::ui` **之前**画的
    // ——从前按下去那一下赶得上同一帧的 `notice_toast`，现在赶不上了。
    // 对着屏幕的人分不出这一帧（十六毫秒），但测试分得出。
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    assert!(
        屏上.contains("请勾选两个或更多作品"),
        "没说清为什么没开：\n{屏上}",
    );
}

#[test]
fn 第二步按平台列变体_写明哪一个来自被合并的作品_每个平台各挑一个首选() {
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-第二步");
    let 屏上 = 走到第几步(&ctx, &mut app, 2);

    assert!(
        屏上.contains("取消勾选的变体不会合并"),
        "第二步那句说明没画出来：\n{屏上}",
    );
    assert!(有这一段(&屏上, "GB"), "该按平台分组：\n{屏上}");
    // **那一句只有一处**（ADR-0005 再修订那一节）：屏上常驻写着它，那几行的勾选框画成灰的、
    // 停上去说的也是它，而挡住的那一处（`merge::plan` 略过已经在它名下的）指的还是它。
    // 这里照常量断，不照抄字面——改了常量而屏上没跟着改时这一条才红得出来。
    assert!(
        有这一段(&屏上, romcat_core::triage::merge::ALREADY_HELD),
        "保留作品自己的那几个该标出「{}」：\n{屏上}",
        romcat_core::triage::merge::ALREADY_HELD,
    );
    assert!(
        屏上.contains(&format!("来自「{乙}」")),
        "归入的那一个该写明从哪儿来：\n{屏上}",
    );
    assert!(
        有这一段(&屏上, "首选"),
        "每个平台各该有一个首选可挑：\n{屏上}"
    );
    // **屏上说得出「是哪一种」**：那一句说明写着默认选中的是按「汉化 > 官中 > 日版 > 其他」
    // 选出来的，而那句话只有在屏上分得出种类时才对人有意义。种类由核心库拼
    // （`variant_short_names`），这里断的是它真画出来了。
    assert!(
        屏上.contains("汉化 > 官中 > 日版 > 其他"),
        "没说清默认选中的那一个是怎么来的：\n{屏上}",
    );
    assert!(
        有这一段(&屏上, "汉化版") && 有这一段(&屏上, "原版"),
        "这一组里该分得出「汉化版」与「原版」两种，不然那句规则示范不出来：\n{屏上}",
    );
}

#[test]
fn 第三步只列出有冲突的字段_两边一样的不列() {
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-冲突");
    let 屏上 = 走到第几步(&ctx, &mut app, 3);

    assert!(
        有这一段(&屏上, "字段冲突"),
        "第三步没画冲突那一块：\n{屏上}"
    );
    assert!(
        有这一段(&屏上, "简介"),
        "两句不一样的简介该列出来：\n{屏上}"
    );
    assert!(
        有这一段(&屏上, "年份"),
        "保留作品空着、对方有值的该列出来：\n{屏上}"
    );
    // **两边说的一模一样的不列**：开发商两边都是 Game Freak。
    assert!(
        !有这一段(&屏上, "开发商"),
        "两边一样的字段不该列成冲突：\n{屏上}",
    );
}

#[test]
fn 第三步写明会发生什么_写几条裁决_前端条目从多少变多少_子库要重排差量预览那一句() {
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-会发生什么");
    走到第几步(&ctx, &mut app, 3);
    // 「会发生什么」那一块在 1280×800 的视口里落在折叠线以下——egui 不画看不见的字。
    let 屏上 = 滚到底(&ctx, |ui| app.ui(ui));

    assert!(
        有这一段(&屏上, "合并后会发生什么"),
        "第三步没画那一块账：\n{屏上}",
    );
    assert!(屏上.contains("1 条裁决"), "没说写几条裁决：\n{屏上}");
    assert!(
        屏上.contains("发行版信息不变"),
        "没说清发行版信息一个字不动：\n{屏上}",
    );
    // **前端条目从多少变多少**：眼下甲（GB）、乙（GB）、丙（SFC）三条，合并之后两条。
    assert!(屏上.contains("3 → 2"), "前端条目那两个数没画出来：\n{屏上}");
    // **撤得回哪几样要说全**：批只装沉淀库的裁决，别名、字段值与首选变体不跟着撤。
    assert!(屏上.contains("整批撤销"), "没说撤得回来：\n{屏上}");
    assert!(
        屏上.contains("别名、上面选的字段值与首选变体不跟着撤"),
        "没说清撤不回哪几样：\n{屏上}",
    );
}

#[test]
fn 自动归入那个勾选框不可选_并写明原因与眼下的走法() {
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-自动归入");
    走到第几步(&ctx, &mut app, 3);
    let 屏上 = 滚到底(&ctx, |ui| app.ui(ui));

    assert!(
        屏上.contains("也自动归入"),
        "稿上那个勾选框没画出来：\n{屏上}",
    );
    assert!(
        屏上.contains("还不能选") && 屏上.contains("沉淀库"),
        "没写明为什么还不能选：\n{屏上}",
    );
    assert!(
        屏上.contains("可以再合并一次"),
        "没写明眼下的走法：\n{屏上}",
    );
    // **真的按不动**：点它一下，屏上仍旧写着「还不能选」，第三步也没走掉。
    let 再来 = 点一下(&ctx, "也自动归入", |ui| app.ui(ui));
    assert!(再来.contains("还不能选"), "那个勾选框点得动了：\n{再来}");
    assert!(
        有这一段(&再来, "合并后会发生什么"),
        "点那一格把这一层点没了：\n{再来}",
    );
}

#[test]
fn 按下合并之后变体归入保留的作品_被合并作品的名字留作别名() {
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-落下");
    let 屏上 = 走到第几步(&ctx, &mut app, 3);
    assert!(
        有这一段(&屏上, "合并 2 个作品"),
        "页脚那颗没画出来：\n{屏上}"
    );

    let 之后 = 点一下(&ctx, "合并 2 个作品", |ui| app.ui(ui));
    assert!(!有这一段(&之后, "字段冲突"), "落下之后向导该关掉：\n{之后}",);
    assert!(之后.contains("已合并"), "没给回执：\n{之后}");
    assert!(之后.contains("裁决记录"), "回执得说清去哪儿撤销：\n{之后}",);

    assert_eq!(
        挂在(&mut app, &键("GB", "精灵宝可梦 红.zip")).as_deref(),
        Some(甲),
        "变体没改挂到保留的作品名下",
    );
    // **发行版信息一个字不动**：序列号还在。
    let (_, site) = app.browse_and_site();
    let row = site
        .catalog
        .variant(&键("GB", "精灵宝可梦 红.zip"))
        .expect("读得动")
        .expect("有这一行");
    let release = row
        .release_id
        .and_then(|id| site.catalog.release(id).expect("读得动"))
        .expect("还基于一条发行版");
    assert_eq!(release.serial.as_deref(), Some("DMG-APAJ"));
    // **被合并作品的名字留作别名**：搜这个名字仍找得到合并后的作品。
    let 叫法 = site.catalog.titles_of(甲).expect("读得动");
    assert!(
        叫法.iter().any(|row| row.value == 乙),
        "被合并作品的名字没留作别名：{叫法:?}",
    );
}

#[test]
fn 合并落下之后在裁决记录里整批撤销_变体回到原来的作品() {
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-撤销");
    走到第几步(&ctx, &mut app, 3);
    点一下(&ctx, "合并 2 个作品", |ui| app.ui(ui));
    assert_eq!(
        挂在(&mut app, &键("GB", "精灵宝可梦 红.zip")).as_deref(),
        Some(甲),
    );

    // 撤销走的是**既有那条按批撤销的路**：核心库那一处，界面与命令行共用。
    let (_, site) = app.browse_and_site();
    let 这一批 = site
        .store
        .batches(根, 8)
        .expect("读得出批")
        .into_iter()
        .next()
        .expect("刚落下的那一批在册");
    assert!(
        这一批.summary.starts_with("合并作品 · "),
        "裁决记录里那句摘要认不出是哪一颗按的：{}",
        这一批.summary,
    );
    romcat_core::triage::undo_batch(&mut site.catalog, &mut site.store, 这一批.id).expect("撤得掉");
    assert_eq!(
        挂在(&mut app, &键("GB", "精灵宝可梦 红.zip")).as_deref(),
        Some(乙),
        "撤销没把变体还回原来的作品",
    );
}

/// 打开作品详情页、换到「变体与文件」那一面。
fn 打开详情页(ctx: &egui::Context, app: &mut App, 作品: &str) -> String {
    let anchor = 那一行(app, 作品);
    {
        let (browse, site) = app.browse_and_site();
        browse.open_work(&site.catalog, &anchor);
        browse.open_page(Tab::Variants);
    }
    跑一帧(ctx, |ui| app.ui(ui))
}

#[test]
fn 作品详情页头上有合并那一颗_按下去开得出向导() {
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-详情页入口");
    let 屏上 = 打开详情页(&ctx, &mut app, 甲);
    assert!(
        有这一段(&屏上, "合并…"),
        "详情页头上没有「合并…」：\n{屏上}"
    );

    点一下(&ctx, "合并…", |ui| app.ui(ui));
    let 开了 = 稳一稳(&ctx, &mut app);
    assert!(有这一段(&开了, "选择作品"), "按下去没开出向导：\n{开了}");
    assert!(
        开了.contains("至少需要两个作品"),
        "只带着一个作品进来时该说清还差什么：\n{开了}",
    );
    assert!(
        有这一段(&开了, "添加其他作品"),
        "第一步得摆得出「添加其他作品」：\n{开了}",
    );
}

#[test]
fn 变体卡上移出此作品_移到新建的作品_一条裁决撤得回来() {
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-移出此作品");
    let 屏上 = 打开详情页(&ctx, &mut app, 甲);
    assert!(
        有这一段(&屏上, "移出此作品…"),
        "变体卡上没有「移出此作品…」：\n{屏上}",
    );

    点一下(&ctx, "移出此作品…", |ui| app.ui(ui));
    let 开了 = 稳一稳(&ctx, &mut app);
    assert!(有这一段(&开了, "移到"), "弹层没开出来：\n{开了}");
    assert!(
        有这一段(&开了, "新建一个作品"),
        "两档去处该都摆出来：\n{开了}",
    );
    assert!(
        有这一段(&开了, "移入另一个作品"),
        "两档去处该都摆出来：\n{开了}",
    );
    assert!(
        有这一段(&开了, "移出后会发生什么"),
        "没写明会发生什么：\n{开了}",
    );
    assert!(开了.contains("1 条裁决"), "没说写几条裁决：\n{开了}");
    assert!(
        开了.contains("发行版信息不变"),
        "没说清发行版信息一个字不动：\n{开了}",
    );

    // **移的就是弹层上写着的那一个**：它把那个变体写成「根名 · 相对路径」印在标题底下那张卡上
    // （拿主意的人 2026-10-01 裁 `F-8`），拼回中立库的键就是它。
    let 移的是 = 开了
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{根} · ")))
        .map(|相对| format!("{根}/{相对}"))
        .expect("弹层上该写着移的是哪一个变体");

    let 落了 = 正好点一下(&ctx, "移出", |ui| app.ui(ui));
    assert!(落了.contains("已移到"), "没给回执：\n{落了}");
    // 默认名字照稿：原作品名加上变体简称头一段。
    let 新名字 = 挂在(&mut app, &移的是).expect("还挂着一个作品");
    assert_ne!(新名字, 甲, "没移出去");
    assert!(
        新名字.starts_with(&format!("{甲}（")),
        "新作品的默认名字不照稿：{新名字}",
    );
    // **另一个没被动到**：移出只动点名的那一个。
    let 另一个 = [
        键("GB", "口袋妖怪 红(汉化).zip"),
        键("GB", "口袋妖怪 红(汉化 修正).zip"),
    ]
    .into_iter()
    .find(|key| *key != 移的是)
    .expect("甲底下两个变体");
    assert_eq!(挂在(&mut app, &另一个).as_deref(), Some(甲));

    let (_, site) = app.browse_and_site();
    let 这一批 = site
        .store
        .batches(根, 8)
        .expect("读得出批")
        .into_iter()
        .next()
        .expect("刚落下的那一批在册");
    assert!(
        这一批.summary.starts_with("移出此作品 · "),
        "裁决记录里认不出是哪一颗按的：{}",
        这一批.summary,
    );
    romcat_core::triage::undo_batch(&mut site.catalog, &mut site.store, 这一批.id).expect("撤得掉");
    assert_eq!(
        挂在(&mut app, &移的是).as_deref(),
        Some(甲),
        "撤销没把变体还回原来的作品",
    );
}

#[test]
fn 取消勾选的变体不归入_第三步那一笔账跟着变() {
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-取消勾选");
    let 第二步 = 走到第几步(&ctx, &mut app, 2);
    assert!(
        第二步.contains(&format!("来自「{乙}」")),
        "第二步没列出它：\n{第二步}"
    );

    // 把乙那一个取消勾选（它是这一组里唯一的「汉化版」）：一个要归入的变体都不剩。
    点一下(&ctx, "汉化版", |ui| app.ui(ui));
    let 取消了 = 稳一稳(&ctx, &mut app);
    assert!(
        取消了.contains("一个要归入的变体都没有"),
        "取消完没说清为什么走不下去：\n{取消了}",
    );
    let 之后 = 跑一帧(&ctx, |ui| app.ui(ui));
    assert!(
        !有这一段(&之后, "合并后会发生什么"),
        "一个都不归入还走得到第三步：\n{之后}",
    );
}

#[test]
fn 合并只动参与的那几个作品_没参与的一个字没动() {
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-别的作品不动");
    走到第几步(&ctx, &mut app, 3);
    点一下(&ctx, "合并 2 个作品", |ui| app.ui(ui));

    assert_eq!(
        挂在(&mut app, &键("SFC", "幻想传说 汉化版.zip")).as_deref(),
        Some(丙),
        "没参与的作品被动了",
    );
    let (_, site) = app.browse_and_site();
    let 还剩 = site
        .catalog
        .work_page(&WorkQuery::default(), 0, 64)
        .expect("取得出一页");
    assert!(
        还剩.iter().any(|row| row.name == 丙),
        "没参与的作品该还在表上：{:?}",
        还剩.iter().map(|row| &row.name).collect::<Vec<_>>(),
    );
}

/// 夹具自己摆得对不对：三个作品、四个变体。
#[test]
fn 夹具摆出来的是三个作品四个变体() {
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-夹具");
    跑(&ctx, &mut app, 2);
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| app.ui(ui)));
    for 作品 in [甲, 乙, 丙] {
        assert!(屏上.contains(作品), "表上没有「{作品}」：\n{屏上}");
    }
}

#[test]
fn 人没点过首选变体时一条首选裁决都不落() {
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-首选不乱落");
    走到第几步(&ctx, &mut app, 3);
    点一下(&ctx, "合并 2 个作品", |ui| app.ui(ui));

    // **一条都不该有**：屏上默认选中的那一个是核心库照「汉化 > 官中 > 日版 > 其他」算的，
    // 把它也写下去等于拿一条裁决把规则冻住（词表**首选变体**：规则可被裁决覆盖，不是反过来）。
    let (_, site) = app.browse_and_site();
    let 裁过的 = site.catalog.preferred_variants().expect("读得动");
    assert!(
        裁过的.is_empty(),
        "人一下都没点，却落下了首选变体裁决：{裁过的:?}",
    );
}

#[test]
fn 别名只留并空了的那几个_屏上说清是哪几个() {
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-别名说清");
    走到第几步(&ctx, &mut app, 3);
    let 屏上 = 滚到底(&ctx, |ui| app.ui(ui));
    assert!(
        屏上.contains(&format!("留的是并空了的那几个：《{乙}》")),
        "没说清留的是哪几个名字：\n{屏上}",
    );

    点一下(&ctx, "合并 2 个作品", |ui| app.ui(ui));
    let (_, site) = app.browse_and_site();
    let 叫法 = site.catalog.titles_of(甲).expect("读得动");
    assert!(
        叫法.iter().any(|row| row.value == 乙),
        "并空了的那个名字该留作别名：{叫法:?}",
    );
    // 没参与的那个作品的名字**不该**被记成别名。
    assert!(
        !叫法.iter().any(|row| row.value == 丙),
        "没参与的作品名被记成别名了：{叫法:?}",
    );
}

#[test]
fn 刚落下那一批时提示条上有撤销_按一下变体就回到原来的作品() {
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-提示条撤销");
    走到第几步(&ctx, &mut app, 3);
    let 之后 = 点一下(&ctx, "合并 2 个作品", |ui| app.ui(ui));
    assert!(
        有这一段(&之后, "撤销"),
        "提示条上没有那颗「撤销」：\n{之后}"
    );
    assert_eq!(
        挂在(&mut app, &键("GB", "精灵宝可梦 红.zip")).as_deref(),
        Some(甲),
    );

    // 按下去走的是**既有那条按批撤销的路**（`triage::undo_batch`）。
    let 撤了 = 正好点一下(&ctx, "撤销", |ui| app.ui(ui));
    assert!(撤了.contains("已撤销"), "没给回执：\n{撤了}");
    assert_eq!(
        挂在(&mut app, &键("GB", "精灵宝可梦 红.zip")).as_deref(),
        Some(乙),
        "按了「撤销」变体没回到原来的作品",
    );
}

#[test]
fn 移出那一层的默认名字里那几个字由核心库答_不是把变体简称剖回去() {
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-移出-默认名字");
    打开详情页(&ctx, &mut app, 甲);
    点一下(&ctx, "移出此作品…", |ui| app.ui(ui));
    let 开了 = 稳一稳(&ctx, &mut app);

    // 甲底下那两个变体身上没有汉化记号，核心库答的是「原版」（`variant_short_name`）。
    assert!(
        开了.contains(&format!("{甲}（原版）")),
        "默认名字里那几个字不对：\n{开了}",
    );
}

/// 甲那一份**标题集合**：显示标题（中文译名，与作品名同字）、一条官方名称、两条别的叫法。
const 甲的官方名称: &str = "Pocket Monsters - Aka";
/// 甲头上那一句该写什么：官方名称打头，其余叫法照核心库那把排序键跟着（中文别名在日文别名前头）。
const 甲那一句: &str = "Pocket Monsters - Aka · 宝可梦 红版 · ポケットモンスター 赤";

fn 摆上甲的叫法(app: &mut App) {
    use romcat_core::title::{Language, TitleKind};

    let 一条 = |value: &str, language: Language, kind: TitleKind, source: &str| {
        romcat_core::catalog::TitleRow {
            work: 甲.to_string(),
            value: value.to_string(),
            language,
            kind,
            source: source.to_string(),
            region: None,
            variant_key: None,
            confidence: Confidence::High,
            seam: None,
            evidence: "夹具".to_string(),
            seen: 1,
        }
    };
    let (_, site) = app.browse_and_site();
    site.catalog
        .put_titles(&[
            一条(甲, Language::Chinese, TitleKind::Translated, "中文离线源"),
            一条(
                甲的官方名称,
                Language::English,
                TitleKind::Official,
                "No-Intro",
            ),
            一条("宝可梦 红版", Language::Chinese, TitleKind::Alias, "文件名"),
            一条(
                "ポケットモンスター 赤",
                Language::Japanese,
                TitleKind::Alias,
                "文件名",
            ),
        ])
        .expect("写得进标题集合");
}

#[test]
fn 详情页头上与合并向导那一行印的是同一句_官方名称打头其余叫法跟着() {
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-同一句副标题");
    摆上甲的叫法(&mut app);
    打开详情页(&ctx, &mut app, 甲);
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    assert_eq!(
        画着的每一处(&这一帧, &|字| 字 == 甲那一句).len(),
        1,
        "详情页头上那一句（设计稿 `.hsub`）该是「{甲那一句}」：\n{}",
        画出来的字(&这一帧)
    );

    // 从详情页头上那颗「合并…」开向导：头上那一句还画在弹层后头，向导里甲那一行底下再印一遍——**同一句**。
    点一下(&ctx, "合并…", |ui| app.ui(ui));
    稳一稳(&ctx, &mut app);
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 每一处 = 画着的每一处(&这一帧, &|字| 字 == 甲那一句);
    assert_eq!(
        每一处.len(),
        2,
        "详情页头上与合并向导甲那一行该各印一遍「{甲那一句}」：{每一处:?}\n{}",
        画出来的字(&这一帧)
    );
}

/// 稿上 `.mwit .mc` 那一格：宽 44，3:4。
const 封面那一格: egui::Vec2 = egui::vec2(44.0, 44.0 / 0.75);

/// 一张纯色 PNG 的字节，3:4。
fn 一张封面() -> Vec<u8> {
    let buf = image::RgbImage::from_pixel(60, 80, image::Rgb([30, 90, 160]));
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(buf)
        .write_to(&mut out, image::ImageFormat::Png)
        .expect("编得出 PNG");
    out.into_inner()
}

/// 同 [`界面`]，工作目录里先开一份**媒体池**、甲那个作品上挂一张封面；乙一张图都没有。
fn 甲有封面的界面(名字: &str) -> App {
    let dir = 干净工作目录(名字);
    // **池子得先在**：界面开起来那一刻看工作目录里有没有媒体池，没有就不指。
    let pool =
        romcat_core::scrape::pool::MediaPool::open(&romcat_core::workspace::media_pool_dir(&dir))
            .expect("开得出媒体池");
    let site = Site::in_memory(建库(), Store::in_memory().expect("开得出沉淀库"), 根);
    let mut app = App::new(site, dir);
    app.show_view(View::Browse);
    let bytes = 一张封面();
    let (hash, _) = pool.take_bytes(&bytes, "png").expect("落得进池");
    let (_, site) = app.browse_and_site();
    site.catalog
        .put_media(
            &hash,
            "png",
            bytes.len() as u64,
            romcat_core::scrape::measure::Measured::default(),
        )
        .expect("记得进库");
    site.catalog
        .put_scraped(&[romcat_core::catalog::scrape::Harvested {
            anchor: AnchorKind::Work.label().to_string(),
            subject: 甲.to_string(),
            source: "测试".to_string(),
            input: "测试指纹".to_string(),
            values: Vec::new(),
            media: vec![romcat_core::catalog::scrape::HarvestedMedia {
                kind: romcat_core::scrape::MediaKind::Cover.label().to_string(),
                hash,
                evidence: "测试摆进去的".to_string(),
            }],
        }])
        .expect("写得进");
    app
}

/// 这一帧里画着的每一块方的：外框、贴没贴图、底色。贴图的那几块不论画成方块还是网格都收。
fn 方块们(output: &egui::FullOutput) -> Vec<(egui::Rect, bool, egui::Color32)> {
    fn 收(shape: &egui::epaint::Shape, out: &mut Vec<(egui::Rect, bool, egui::Color32)>) {
        match shape {
            egui::epaint::Shape::Rect(rect) => out.push((
                rect.rect,
                rect.fill_texture_id() != egui::TextureId::default(),
                rect.fill,
            )),
            egui::epaint::Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::default() => {
                out.push((mesh.calc_bounds(), true, egui::Color32::TRANSPARENT));
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|one| 收(one, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        收(&clipped.shape, &mut out);
    }
    out
}

/// 向导里那一行左边那一格（`封面那一格` 那么大、在那一行说明的左边、与它同在一行里）上画着的方块。
fn 那一行左边那一格(
    output: &egui::FullOutput,
    说明: egui::Rect,
) -> Vec<(egui::Rect, bool, egui::Color32)> {
    方块们(output)
        .into_iter()
        .filter(|(rect, _, _)| {
            (rect.size() - 封面那一格).length() < 1.0
                && rect.right() < 说明.left()
                && rect.top() <= 说明.bottom()
                && rect.bottom() >= 说明.top()
        })
        .collect()
}

/// 这一帧里**画在 `那一块` 那个方块之后**、中心落在它里头的每一段字：写的什么、画在哪儿。
///
/// 只数画在它之后的：弹层盖在浏览屏上头，背后那张表在同一个位置上的字也在这一帧里，可那是被盖住的。
fn 那一块里的字(
    output: &egui::FullOutput, 那一块: egui::Rect
) -> Vec<(String, egui::Rect)> {
    fn 摊平<'a>(shape: &'a egui::epaint::Shape, out: &mut Vec<&'a egui::epaint::Shape>) {
        match shape {
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|one| 摊平(one, out)),
            one => out.push(one),
        }
    }
    let mut 全部 = Vec::new();
    for clipped in &output.shapes {
        摊平(&clipped.shape, &mut 全部);
    }
    let 从 = 全部
        .iter()
        .position(|shape| matches!(shape, egui::epaint::Shape::Rect(rect) if rect.rect == 那一块))
        .unwrap_or_else(|| panic!("这一帧里没画 {那一块:?} 那一块"));
    全部[从..]
        .iter()
        .filter_map(|shape| match shape {
            egui::epaint::Shape::Text(text) => {
                let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                那一块
                    .contains(rect.center())
                    .then(|| (text.galley.text().to_string(), rect))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn 合并向导每一行左边有封面缩略图_有封面贴封面_没有画平台代号() {
    let ctx = headless::context();
    let mut app = 甲有封面的界面("romcat-测试-合并-封面缩略图");
    走到第几步(&ctx, &mut app, 1);
    // 解码在后台：一帧一帧跑到甲那一行贴上图，**不看挂钟**；跑满上限还没贴上就当场炸。
    let mut 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 甲那一行 = |output: &egui::FullOutput| {
        画着的每一处(output, &|字| 字.starts_with("GB · 年份未知 · 2 个变体 · "))
            .first()
            .copied()
            .unwrap_or_else(|| panic!("向导里没有甲那一行：\n{}", 画出来的字(output)))
    };
    for _ in 0..5_000 {
        if 那一行左边那一格(&这一帧, 甲那一行(&这一帧))
            .iter()
            .any(|(_, 贴图, _)| *贴图)
        {
            break;
        }
        std::thread::yield_now();
        这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    }
    assert!(
        那一行左边那一格(&这一帧, 甲那一行(&这一帧))
            .iter()
            .any(|(_, 贴图, _)| *贴图),
        "甲在媒体池里有封面，向导那一行左边该贴上它：{:?}",
        那一行左边那一格(&这一帧, 甲那一行(&这一帧))
    );

    // 乙一张图都没有：那一格画占位——平台色调进去的底，正中只写平台代号（拿主意的人 2026-10-01 裁：作品名不画，
    // 右边粗体已经写着；从前逐字折行塞进 44 点宽、还压在水印上），不留白、不贴图。
    let 乙那一行 = 画着的每一处(&这一帧, &|字| 字.starts_with("GB · 1996 · 1 个变体 · "))
        .first()
        .copied()
        .unwrap_or_else(|| panic!("向导里没有乙那一行：\n{}", 画出来的字(&这一帧)));
    let 那一格 = 那一行左边那一格(&这一帧, 乙那一行);
    assert!(
        !那一格.is_empty() && 那一格.iter().all(|(_, 贴图, _)| !*贴图),
        "乙没有封面，那一格该画占位、不贴图：{那一格:?}",
    );
    assert!(
        那一格.iter().any(|(_, _, 底)| 底.a() > 0),
        "乙那一格占位该有底色：{那一格:?}",
    );
    let 卡 = 那一格[0].0;
    let 卡上的字 = 那一块里的字(&这一帧, 卡);
    let [(代号, 在哪)] = 卡上的字.as_slice() else {
        panic!("乙那一格占位上该只写平台代号一段字，写着的是 {卡上的字:?}");
    };
    assert_eq!(代号, "GB", "乙那一格占位上写的该是它的平台代号");
    assert!(
        (在哪.center() - 卡.center()).length() < 1.0,
        "平台代号该摆在那一格正中：字在 {在哪:?}，格子 {卡:?}",
    );
}

/// 乙补一条中文译名：显示标题换成它，作品名照旧是 [`乙`]。
const 乙的译名: &str = "精灵宝可梦 红版";

#[test]
fn 合并向导那一行粗体写显示标题_不是作品名() {
    // 拿主意的人 2026-10-01 裁（票 `gui-draws-the-rest-of-the-design/03` 人的关岔路口 1）：照稿 `.mwit` 的 `<b>${w.t}</b>`，
    // 与作品详情页大标题同一个名字——底下那一句去掉的正是显示标题，粗体再写作品名，向导里就哪儿都看不到它了。
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-粗体写显示标题");
    {
        let (_, site) = app.browse_and_site();
        site.catalog
            .put_titles(&[romcat_core::catalog::TitleRow {
                work: 乙.to_string(),
                value: 乙的译名.to_string(),
                language: romcat_core::title::Language::Chinese,
                kind: romcat_core::title::TitleKind::Translated,
                source: "中文离线源".to_string(),
                region: None,
                variant_key: None,
                confidence: Confidence::High,
                seam: None,
                evidence: "夹具".to_string(),
                seen: 1,
            }])
            .expect("写得进标题集合");
    }
    走到第几步(&ctx, &mut app, 1);
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 说明 = 画着的每一处(&这一帧, &|字| 字.starts_with("GB · 1996 · 1 个变体 · "))
        .first()
        .copied()
        .unwrap_or_else(|| panic!("向导里没有乙那一行：\n{}", 画出来的字(&这一帧)));
    // 紧挨在那一行说明头上、与它左沿对齐的那一段，就是那一行的粗体名。
    let 头上那一段 = |字: &egui::Rect| {
        (字.left() - 说明.left()).abs() < 1.0
            && 字.bottom() <= 说明.top() + 1.0
            && 说明.top() - 字.bottom() < 字.height()
    };
    assert!(
        画着的每一处(&这一帧, &|字| 字 == 乙的译名)
            .iter()
            .any(头上那一段),
        "乙那一行粗体该写显示标题「{乙的译名}」：\n{}",
        画出来的字(&这一帧)
    );
    assert!(
        !画着的每一处(&这一帧, &|字| 字 == 乙).iter().any(头上那一段),
        "乙那一行粗体不该再写作品名「{乙}」",
    );
}

/// 甲那一份标题集合里**别的叫法有五条**：那一句只列前三条，后头接「等 5 个」。
fn 摆上甲的一堆叫法(app: &mut App) {
    use romcat_core::title::{Language, TitleKind};

    let 一条 = |value: &str, language: Language, kind: TitleKind| romcat_core::catalog::TitleRow {
        work: 甲.to_string(),
        value: value.to_string(),
        language,
        kind,
        source: "文件名".to_string(),
        region: None,
        variant_key: None,
        confidence: Confidence::High,
        seam: None,
        evidence: "夹具".to_string(),
        seen: 1,
    };
    let (_, site) = app.browse_and_site();
    site.catalog
        .put_titles(&[
            一条(甲, Language::Chinese, TitleKind::Translated),
            一条(甲的官方名称, Language::English, TitleKind::Official),
            一条("宝可梦 红版", Language::Chinese, TitleKind::Alias),
            一条("口袋怪兽 红", Language::Chinese, TitleKind::Alias),
            一条(
                "ポケットモンスター 赤",
                Language::Japanese,
                TitleKind::Alias,
            ),
            一条("pm_red_cn", Language::Unknown, TitleKind::Alias),
            一条("pokered", Language::Unknown, TitleKind::Alias),
        ])
        .expect("写得进标题集合");
}

#[test]
fn 别的叫法多于三条时那一句只列前三条_后头接等几个() {
    // 拿主意的人 2026-10-01 裁（票 `gui-draws-the-rest-of-the-design/03` 人的关岔路口 2）：别名最多 3 条，后面接「等 M 个」，
    // 详情页头上与合并向导同一句。次序照核心库那把排序键：中文别名在前（同档按字排），认不出语言的与日文的在后。
    const 那一句: &str = "Pocket Monsters - Aka · 口袋怪兽 红 · 宝可梦 红版 · pm_red_cn 等 5 个";
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-别名截到三条");
    摆上甲的一堆叫法(&mut app);
    打开详情页(&ctx, &mut app, 甲);
    点一下(&ctx, "合并…", |ui| app.ui(ui));
    稳一稳(&ctx, &mut app);
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 每一处 = 画着的每一处(&这一帧, &|字| 字 == 那一句);
    assert_eq!(
        每一处.len(),
        2,
        "详情页头上与合并向导甲那一行该各印一遍「{那一句}」：{每一处:?}\n{}",
        画出来的字(&这一帧)
    );
}

/// 这一帧里正好写着这几个字的每一段：画在哪儿、什么颜色（排字时给的那个颜色，没给时是画的时候兜底那个）。
fn 那几个字画成什么色(
    output: &egui::FullOutput,
    那几个字: &str,
) -> Vec<(egui::Rect, egui::Color32)> {
    fn 收(
        shape: &egui::epaint::Shape,
        那几个字: &str,
        out: &mut Vec<(egui::Rect, egui::Color32)>,
    ) {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == 那几个字 => {
                let color = text
                    .galley
                    .job
                    .sections
                    .first()
                    .map(|section| section.format.color)
                    .filter(|color| *color != egui::Color32::PLACEHOLDER)
                    .unwrap_or(text.fallback_color);
                out.push((
                    egui::Rect::from_min_size(text.pos, text.galley.size()),
                    color,
                ));
            }
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().for_each(|one| 收(one, 那几个字, out))
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        收(&clipped.shape, 那几个字, &mut out);
    }
    out
}

#[test]
fn 第一步保留那枚标签照稿是强调色实底_字是强调色上的字色() {
    // 设计稿 `.keepb`：`background:var(--accent);color:var(--on-accent)`、高 20、左右 7（与 `.tag` 同），
    // 不是灰底的 `.tag`（拿主意的人 2026-10-01 在票 `gui-draws-the-rest-of-the-design/01` 人的关上裁：照稿改）。
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-保留标签照稿");
    走到第几步(&ctx, &mut app, 1);
    // 弹层是淡入的：等它整个显出来再量颜色，不然量到的是淡入那一半。
    跑(&ctx, &mut app, 60);
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 色 = romcat_gui::tokens::Tokens::builtin()
        .color
        .theme(ctx.theme());
    let 标签们 = 那几个字画成什么色(&这一帧, "保留");
    let [(字, 字色)] = 标签们.as_slice() else {
        panic!("第一步该正好有一枚「保留」：{标签们:?}");
    };
    assert_eq!(*字色, 色.on_accent, "「保留」那几个字该是强调色上的字色");
    let 版式 = &romcat_gui::tokens::Tokens::builtin().layout;
    let 底们: Vec<egui::Rect> = 方块们(&这一帧)
        .into_iter()
        .filter(|(rect, 贴图, _)| !*贴图 && rect.contains(字.center()))
        .filter(|(_, _, 底)| *底 == 色.accent)
        .map(|(rect, _, _)| rect)
        .collect();
    let [底] = 底们.as_slice() else {
        let 垫着的: Vec<_> = 方块们(&这一帧)
            .into_iter()
            .filter(|(rect, _, _)| rect.contains(字.center()))
            .collect();
        panic!(
            "「保留」底下该正好垫一块强调色（{:?}）的底：{底们:?}；垫着的是 {垫着的:?}",
            色.accent
        );
    };
    assert!(
        (底.height() - 版式.tag_height).abs() < 0.5
            && (字.left() - 底.left() - 版式.tag_padding).abs() < 1.0,
        "那块底高 {}（该 {}），字离左沿 {}（该 {}）",
        底.height(),
        版式.tag_height,
        字.left() - 底.left(),
        版式.tag_padding,
    );
}

// ——— 票 `gui-draws-the-rest-of-the-design/16`：合并向导与「移出此作品」照稿 ———

/// 在这个点上按一下（移过去、按下、松开），交回松开之后再画一帧画出来的字。
fn 点在(
    ctx: &egui::Context, 位置: egui::Pos2, mut 画一帧: impl FnMut(&mut egui::Ui)
) -> String {
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

/// 第一步里某一行的粗体名：紧挨在那一行说明（以 `说明开头` 打头）头上、与它左沿对齐的那一段。
fn 那一行的名字(output: &egui::FullOutput, 名字: &str, 说明开头: &str) -> egui::Rect {
    let 说明 = 画着的每一处(output, &|字| 字.starts_with(说明开头))
        .first()
        .copied()
        .unwrap_or_else(|| panic!("第一步没有「{说明开头}…」那一行：\n{}", 画出来的字(output)));
    画着的每一处(output, &|字| 字 == 名字)
        .into_iter()
        .find(|字| {
            (字.left() - 说明.left()).abs() < 1.0
                && 字.bottom() <= 说明.top() + 1.0
                && 说明.top() - 字.bottom() < 字.height()
        })
        .unwrap_or_else(|| {
            panic!(
                "「{说明开头}…」那一行头上没有粗体「{名字}」：\n{}",
                画出来的字(output)
            )
        })
}

#[test]
fn 第一步按在作品名右边的空白处也换得了保留的作品() {
    // 设计稿 `.mwit`：**整张卡是一颗按钮**（`<button class="mwit" data-mw="keep:…">`），点哪儿都换保留的那一个
    // （差距清单 `M-01`）。从前只有圆点与那几行字按得动，名字右边那一大片空白按下去什么都不做。
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-整卡换保留");
    走到第几步(&ctx, &mut app, 1);
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 乙的名字 = 那一行的名字(&这一帧, 乙, "GB · 1996 · 1 个变体 · ");
    // 名字右边两百点、与名字同一高度：那一行卡里，字已经写完了的空白处。
    let 空白处 = egui::pos2(乙的名字.right() + 200.0, 乙的名字.center().y);
    点在(&ctx, 空白处, |ui| app.ui(ui));
    稳一稳(&ctx, &mut app);

    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 乙的名字 = 那一行的名字(&这一帧, 乙, "GB · 1996 · 1 个变体 · ");
    let 标签们 = 画着的每一处(&这一帧, &|字| 字 == "保留");
    let [标签] = 标签们.as_slice() else {
        panic!(
            "第一步该正好有一枚「保留」：{标签们:?}\n{}",
            画出来的字(&这一帧)
        );
    };
    assert!(
        (标签.center().y - 乙的名字.center().y).abs() < 1.0 && 标签.left() >= 乙的名字.right(),
        "按在乙那一行名字右边的空白处，保留的该换成乙：「保留」画在 {标签:?}，乙的名字在 {乙的名字:?}",
    );
}

#[test]
fn 卡上那颗移除只移除那一行_不换保留的作品() {
    // 整张卡是一颗按钮之后，卡里那颗「移除」得照旧接它自己的点击（它摆在卡的上头）：按它不该顺手把保留的换成这一张。
    // **先把保留的换成丙**：默认保留的是甲（变体最多），而保留的那一行被移除时会退回「变体最多的那一个」——
    // 留着甲当保留的，就算那一下也被整张卡接走了，结果照样是甲，这条测试就红不出来。
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-卡上移除");
    勾上(&mut app, &[甲, 乙, 丙]);
    跑(&ctx, &mut app, 2);
    点一下(&ctx, "合并作品…", |ui| app.ui(ui));
    稳一稳(&ctx, &mut app);
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 丙的名字 = 那一行的名字(&这一帧, 丙, "SFC · 年份未知 · 1 个变体 · ");
    点在(
        &ctx,
        egui::pos2(丙的名字.right() + 200.0, 丙的名字.center().y),
        |ui| app.ui(ui),
    );
    稳一稳(&ctx, &mut app);
    let 保留的是 = |app: &mut App, 名字: &str, 说明开头: &str| {
        let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
        let 那一行 = 那一行的名字(&这一帧, 名字, 说明开头);
        画着的每一处(&这一帧, &|字| 字 == "保留")
            .iter()
            .any(|标签| (标签.center().y - 那一行.center().y).abs() < 1.0)
    };
    assert!(
        保留的是(&mut app, 丙, "SFC · 年份未知 · 1 个变体 · "),
        "按在丙那张卡的空白处，保留的该先换成丙",
    );

    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 乙的名字 = 那一行的名字(&这一帧, 乙, "GB · 1996 · 1 个变体 · ");
    let 移除 = 画着的每一处(&这一帧, &|字| 字 == "移除")
        .into_iter()
        .min_by(|a, b| {
            (a.center().y - 乙的名字.center().y)
                .abs()
                .total_cmp(&(b.center().y - 乙的名字.center().y).abs())
        })
        .expect("三张卡上各有一颗「移除」");
    点在(&ctx, 移除.center(), |ui| app.ui(ui));
    let 之后 = 稳一稳(&ctx, &mut app);
    assert!(
        !之后
            .lines()
            .any(|line| line.starts_with("GB · 1996 · 1 个变体 · ")),
        "按了乙那张卡上的「移除」，乙该从向导里去掉：\n{之后}",
    );
    assert!(
        保留的是(&mut app, 丙, "SFC · 年份未知 · 1 个变体 · "),
        "按的是乙那张卡上的「移除」，保留的该照旧是丙——变成了甲就是那一下也被整张卡接走了",
    );
}

/// 乙那一份标题集合里补一条与**甲的作品名**一字不差的别名：核心库据此认出甲乙**疑似同一作品**（命名撞上）。
fn 乙也叫甲的名字(catalog: &mut Catalog) {
    catalog
        .put_titles(&[romcat_core::catalog::TitleRow {
            work: 乙.to_string(),
            value: 甲.to_string(),
            language: romcat_core::title::Language::Chinese,
            kind: romcat_core::title::TitleKind::Alias,
            source: "文件名".to_string(),
            region: None,
            variant_key: None,
            confidence: Confidence::High,
            seam: None,
            evidence: "夹具".to_string(),
            seen: 1,
        }])
        .expect("写得进标题集合");
}

#[test]
fn 参与的作品与向导外的作品疑似同一作品时_第一步建议一并合并_按添加就进向导() {
    // 设计稿 `renderMW` 第一步那一段 `extra`：参与的作品里有一个与库里别的作品是**疑似同一作品**、另一边还没在向导里时，
    // 小标题「建议一并合并」底下一行「作品名 · 第一条理由 · 添加」（差距清单 `M-03`）。理由**逐字**取核心库（`Suspicion::reasons`）。
    let mut catalog = 建库();
    乙也叫甲的名字(&mut catalog);
    let store = Store::in_memory().expect("开得出沉淀库");
    let 那一对 = romcat_core::triage::same_work::survey(&catalog, &store, 根).expect("扫得动");
    let 理由 = 那一对
        .iter()
        .find(|one| one.touches(甲) && one.other_than(甲) == Some(乙))
        .map(|one| one.reasons()[0].clone())
        .unwrap_or_else(|| panic!("夹具没摆出甲乙这一对：{那一对:?}"));
    let site = Site::in_memory(catalog, store, 根);
    let mut app = App::new(site, 干净工作目录("romcat-测试-合并-建议一并合并"));
    app.show_view(View::Browse);

    let ctx = headless::context();
    勾上(&mut app, &[甲, 丙]);
    跑(&ctx, &mut app, 2);
    点一下(&ctx, "合并作品…", |ui| app.ui(ui));
    let 屏上 = 稳一稳(&ctx, &mut app);
    assert!(
        有这一段(&屏上, "建议一并合并"),
        "第一步该有「建议一并合并」那一段：\n{屏上}"
    );
    assert!(
        有这一段(&屏上, &理由),
        "那一行该逐字写核心库给的头一条理由「{理由}」：\n{屏上}"
    );
    assert!(
        !屏上
            .lines()
            .any(|line| line.starts_with("GB · 1996 · 1 个变体 · ")),
        "乙还没进向导：\n{屏上}",
    );

    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 理由在 = 画着的每一处(&这一帧, &|字| 字 == 理由)[0];
    let 添加 = 画着的每一处(&这一帧, &|字| 字 == "添加")
        .into_iter()
        .find(|字| (字.center().y - 理由在.center().y).abs() < 4.0)
        .unwrap_or_else(|| panic!("理由那一行上没有「添加」：\n{}", 画出来的字(&这一帧)));
    点在(&ctx, 添加.center(), |ui| app.ui(ui));
    let 之后 = 稳一稳(&ctx, &mut app);
    assert!(
        之后
            .lines()
            .any(|line| line.starts_with("GB · 1996 · 1 个变体 · ")),
        "按了「添加」，乙该进向导、摆成一张卡：\n{之后}",
    );
    assert!(
        !有这一段(&之后, "建议一并合并"),
        "乙进了向导，那一对两边都在里头了，这一段该收起来：\n{之后}",
    );
}

#[test]
fn 添加其他作品的搜索结果整行按下去就添加() {
    // 设计稿 `mwResults`：一行就是一颗按钮（`<button data-mw="add:…">`），按名字、按年份那一截都添加（差距清单 `M-04`）。
    // 从前那一行只有行尾一颗「添加」按得动。
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-整行添加");
    let 屏上 = 走到第几步(&ctx, &mut app, 1);
    assert!(
        !屏上
            .lines()
            .any(|line| line.starts_with("SFC · 年份未知 · 1 个变体 · ")),
        "丙还没进向导：\n{屏上}",
    );
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    // 「添加其他作品」底下那一框里丙那一行的名字：画在「添加其他作品」那个小标题底下的那一处。
    let 小标题 = 画着的每一处(&这一帧, &|字| 字 == "添加其他作品")[0];
    let 丙的名字 = 画着的每一处(&这一帧, &|字| 字 == 丙)
        .into_iter()
        .find(|字| 字.top() > 小标题.bottom())
        .unwrap_or_else(|| panic!("搜索结果里没有丙：\n{}", 画出来的字(&这一帧)));
    // 按在那一行字都写完了的右半边空白处：整行都是那一颗按钮。
    点在(
        &ctx,
        egui::pos2(丙的名字.right() + 300.0, 丙的名字.center().y),
        |ui| app.ui(ui),
    );
    let 之后 = 稳一稳(&ctx, &mut app);
    assert!(
        之后
            .lines()
            .any(|line| line.starts_with("SFC · 年份未知 · 1 个变体 · ")),
        "按在搜索结果里丙那一行右半边的空白处，丙该进向导：\n{之后}",
    );
}

#[test]
fn 第二步组头写平台全名_一行变体竖直居中_路径写根名与相对路径_置信度是标签() {
    // 设计稿 `.vgrp` / `.vrow`（差距清单 `M-07`、`M-08`、`M-10`，岔路口 `F-8` 拿主意的人 2026-10-01 裁 A）。
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-合并-第二步照稿");
    let 屏上 = 走到第几步(&ctx, &mut app, 2);
    // 组头：平台色标之后是**全名**（核心库平台表 `Manifest::full_name`；GB 写着「Game Boy」）。
    assert!(
        有这一段(&屏上, "Game Boy"),
        "GB 那一组的组头该写平台全名「Game Boy」：\n{屏上}",
    );
    // 路径写「根名 · 相对路径」（`table::root_and_path`），不写中立库的键。
    let 路径 = format!("{根} · GB/精灵宝可梦 红.zip");
    assert!(
        有这一段(&屏上, &路径),
        "乙那一行的路径该写「{路径}」：\n{屏上}"
    );
    assert!(
        !有这一段(&屏上, &键("GB", "精灵宝可梦 红.zip")),
        "路径不该再写中立库的键：\n{屏上}",
    );

    跑(&ctx, &mut app, 60);
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    // 乙那一行：简称在上、路径在下，那一摞的竖直正中就是这一行的中线；「来自「乙」」与体积都对着它。
    let 路径在 = 画着的每一处(&这一帧, &|字| 字 == 路径)[0];
    let 名在 = 画着的每一处(&这一帧, &|字| 字 == "汉化版")
        .into_iter()
        .find(|字| (字.left() - 路径在.left()).abs() < 1.0 && 字.bottom() <= 路径在.top() + 1.0)
        .unwrap_or_else(|| panic!("乙那一行路径头上没有简称：\n{}", 画出来的字(&这一帧)));
    let 中线 = (名在.top() + 路径在.bottom()) / 2.0;
    let 来自 = 画着的每一处(&这一帧, &|字| 字 == format!("来自「{乙}」"))[0];
    assert!(
        (来自.center().y - 中线).abs() < 1.5,
        "「来自「{乙}」」该对着那一行的中线（{中线}）竖直居中，画在 {来自:?}",
    );
    assert!(
        (路径在.height() - 名在.height()).abs() < 6.0 && 来自.height() < 名在.height() * 1.5,
        "那一行各格都该单行：简称 {名在:?}，路径 {路径在:?}，来自 {来自:?}",
    );
    // 置信度是标签（设计稿 `.chip`）：那个词底下垫着这一档的浅底（`hi-soft`），不是一道色条。
    let 色 = romcat_gui::tokens::Tokens::builtin()
        .color
        .theme(ctx.theme());
    let 这一行的词 = 画着的每一处(&这一帧, &|字| 字 == "高置信")
        .into_iter()
        .find(|字| (字.center().y - 中线).abs() < 4.0)
        .unwrap_or_else(|| panic!("乙那一行没有置信度那个词：\n{}", 画出来的字(&这一帧)));
    assert!(
        方块们(&这一帧)
            .iter()
            .any(|(rect, _, 底)| *底 == 色.hi_soft && rect.contains(这一行的词.center())),
        "乙那一行的「高置信」底下该垫一块 hi-soft 的标签底",
    );
}

/// 这一帧里画着的每一道**横线**（两端同高的线段，长过三百点）在哪个高度。弹层里页脚上沿那一道分隔线就在其中。
fn 横线们(output: &egui::FullOutput) -> Vec<f32> {
    fn 收(shape: &egui::epaint::Shape, out: &mut Vec<f32>) {
        match shape {
            egui::epaint::Shape::LineSegment {
                points: [头, 尾], ..
            } if (头.y - 尾.y).abs() < 0.5 && (头.x - 尾.x).abs() > 300.0 => {
                out.push(头.y);
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|one| 收(one, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        收(&clipped.shape, &mut out);
    }
    out
}

/// 同 [`界面`]，甲乙两边再各说一个**类型**与**发行商**、说的不一样：第三步于是有五个冲突的字段
/// （显示标题、简介、类型、发行商、年份；开发商两边一样，不列）。
fn 五个冲突的界面(名字: &str) -> App {
    let mut catalog = 建库();
    for (作品, 字段, 值) in [
        (甲, Field::Genre, "角色扮演"),
        (乙, Field::Genre, "策略角色扮演"),
        (甲, Field::Publisher, "Nintendo"),
        (乙, Field::Publisher, "Pokémon Company"),
    ] {
        catalog
            .put_verdict_value(AnchorKind::Work, 作品, 字段, 值, "夹具")
            .expect("写得进刮削值");
    }
    let site = Site::in_memory(catalog, Store::in_memory().expect("开得出沉淀库"), 根);
    let mut app = App::new(site, 干净工作目录(名字));
    app.show_view(View::Browse);
    app
}

#[test]
fn 第三步五个冲突字段的表整个摆得下_合并后会发生什么头一条不出折叠线_字段照稿叫显示标题照稿排() {
    // 设计稿 `.ctbl`（差距清单 `M-11`…`M-14`）：一格里几个选项挨着，一行约 36 点高；从前每个选项是一行带空说明的
    // `radio_option`，一行六十来点，五个字段就把「合并后会发生什么」整块挤到折叠线外（gl-16 打回过的同一件事）。
    // **拿矩形断，不比像素**：1280×800 的视口里不滚，五行都画在页脚上头，「写入 N 条裁决」那一条也画在页脚上头。
    let ctx = headless::context();
    let mut app = 五个冲突的界面("romcat-测试-合并-第三步摆得下");
    走到第几步(&ctx, &mut app, 3);
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    // **折叠线是页脚上沿那一道横线**（内容区的裁剪底边），不是页脚里那几个字：「取消」的字比那道线还低一截按钮留白。
    let 取消 = 画着的每一处(&这一帧, &|字| 字 == "取消")
        .into_iter()
        .map(|字| 字.top())
        .fold(f32::INFINITY, f32::min);
    assert!(
        取消.is_finite(),
        "页脚那颗「取消」没画出来：\n{}",
        画出来的字(&这一帧)
    );
    let 页脚 = 横线们(&这一帧)
        .into_iter()
        .filter(|y| *y < 取消)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(页脚.is_finite(), "页脚上沿那一道横线没画出来");
    // 弹层后头那张表的表头也有「年份」：只数画在「字段冲突」那个小标题底下的。
    let 小标题 = 画着的每一处(&这一帧, &|字| 字 == "字段冲突")[0];
    let 字段们 = ["显示标题", "简介", "类型", "发行商", "年份"];
    let 每一行: Vec<egui::Rect> = 字段们
        .iter()
        .map(|字段| {
            let 每一处: Vec<egui::Rect> = 画着的每一处(&这一帧, &|字| 字 == *字段)
                .into_iter()
                .filter(|字| 字.top() > 小标题.bottom())
                .collect();
            assert_eq!(
                每一处.len(),
                1,
                "「{字段}」那一行该正好画一处：\n{}",
                画出来的字(&这一帧)
            );
            每一处[0]
        })
        .collect();
    for (字段, 那一行) in 字段们.iter().zip(&每一行) {
        assert!(
            那一行.bottom() <= 页脚,
            "「{字段}」那一行被页脚挡住了：画在 {那一行:?}，页脚从 {页脚} 起",
        );
    }
    assert!(
        每一行.windows(2).all(|两行| 两行[0].top() < 两行[1].top()),
        "字段次序该照稿：{字段们:?}，画在 {每一行:?}",
    );
    assert!(
        画着的每一处(&这一帧, &|字| 字 == "标题")
            .iter()
            .all(|字| 字.top() < 小标题.bottom()),
        "标题那一行该叫「显示标题」：\n{}",
        画出来的字(&这一帧)
    );
    let 头一条 = 画着的每一处(&这一帧, &|字| 字.starts_with("写入 "))
        .first()
        .copied()
        .unwrap_or_else(|| {
            panic!(
                "不滚的时候「合并后会发生什么」头一条没画出来：\n{}",
                画出来的字(&这一帧)
            )
        });
    assert!(
        头一条.bottom() <= 页脚,
        "「合并后会发生什么」头一条出了折叠线：画在 {头一条:?}，页脚上沿在 {页脚}",
    );
    // 自动归入那一句只删尾巴「——再合一次就归进《X》了」（拿主意的人 2026-10-01 裁 `F-9`）。
    let 屏上 = 滚到底(&ctx, |ui| app.ui(ui));
    assert!(
        屏上.contains("可以再合并一次。"),
        "眼下的走法那半句该留着：\n{屏上}"
    );
    assert!(!屏上.contains("再合一次就归进"), "那句尾巴该删掉：\n{屏上}");
}

/// 打开甲的详情页，直接从「变体与文件」那一面上 `key` 那一张卡开「移出此作品」（与卡上那颗按钮同一个入口），跑稳。
fn 移出甲的(ctx: &egui::Context, app: &mut App, key: &str) -> String {
    打开详情页(ctx, app, 甲);
    {
        let (browse, site) = app.browse_and_site();
        browse.open_split(site, key);
    }
    稳一稳(ctx, app)
}

#[test]
fn 移走的正是首选变体时_那半句写顶上那一个的变体简称_不写文件名() {
    // 设计稿 `DLG.split`：「它的首选变体改为「${pf.l}」」——`l` 是变体简称（差距清单 `M-19`）；括号那截照旧留着（`F-9` 裁 B）。
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-移出-首选写简称");
    let 移走的 = 键("GB", "口袋妖怪 红(汉化).zip");
    {
        let (_, site) = app.browse_and_site();
        site.set_preferred_variant(甲, "GB", &移走的)
            .expect("记得下首选变体");
    }
    let 屏上 = 移出甲的(&ctx, &mut app, &移走的);
    // 甲剩下那一个身上没有汉化记号，核心库答的简称是「原版」（`variant_short_names`）。
    assert!(
        屏上.contains("改由「原版」顶上（照「汉化 > 官中 > 日版 > 其他」重选）"),
        "首选那半句该写顶上那一个的变体简称：\n{屏上}",
    );
    assert!(
        !屏上.contains("改由「口袋妖怪"),
        "首选那半句不该写文件名：\n{屏上}",
    );
}

#[test]
fn 移入另一个作品那一框每行写平台年份与变体数_同平台的排前() {
    // 设计稿 `DLG.split` 的 `.srch`：每行圆点、平台色标、粗体名、「年份 · N 个变体」，没搜时同平台的排前（差距清单 `M-20`）。
    // 甲在 GB 上；乙也在 GB、丙在 SFC——不论库里怎么排，乙都该在丙前头。
    let ctx = headless::context();
    let mut app = 界面("romcat-测试-移出-候选那一框");
    // 夹具自己摆得对不对：库里照默认次序排，丙在乙前头——不然「同平台的排前」示范不出来。
    {
        let (_, site) = app.browse_and_site();
        let 默认次序: Vec<String> = site
            .catalog
            .work_page(&WorkQuery::default(), 0, 8)
            .expect("取得出一页")
            .into_iter()
            .map(|row| row.name)
            .collect();
        let 第几 = |名: &str| 默认次序.iter().position(|one| one == 名);
        assert!(
            第几(丙) < 第几(乙),
            "夹具里默认次序该是丙在乙前头：{默认次序:?}"
        );
    }
    移出甲的(&ctx, &mut app, &键("GB", "口袋妖怪 红(汉化).zip"));
    点一下(&ctx, "移入另一个作品", |ui| app.ui(ui));
    稳一稳(&ctx, &mut app);
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 小标题 = 画着的每一处(&这一帧, &|字| 字 == "移到")[0];
    let 弹层里的 = |那几个字: &str| -> egui::Rect {
        画着的每一处(&这一帧, &|字| 字 == 那几个字)
            .into_iter()
            .find(|字| 字.top() > 小标题.bottom())
            .unwrap_or_else(|| {
                panic!(
                    "「移入另一个作品」那一框里没有「{那几个字}」：\n{}",
                    画出来的字(&这一帧)
                )
            })
    };
    let (乙在, 乙那句) = (弹层里的(乙), 弹层里的("1996 · 1 个变体"));
    let (丙在, 丙那句) = (弹层里的(丙), 弹层里的("年份未知 · 1 个变体"));
    for (名, 句, 平台) in [(乙在, 乙那句, "GB"), (丙在, 丙那句, "SFC")] {
        assert!(
            (句.center().y - 名.center().y).abs() < 4.0 && 句.left() > 名.right(),
            "「年份 · N 个变体」该跟在名字后头、同一行：名 {名:?}，句 {句:?}",
        );
        let 色标 = 画着的每一处(&这一帧, &|字| 字 == 平台)
            .into_iter()
            .find(|字| (字.center().y - 名.center().y).abs() < 4.0 && 字.right() < 名.left());
        assert!(
            色标.is_some(),
            "那一行名字前头该有平台「{平台}」：名 {名:?}"
        );
    }
    assert!(
        乙在.top() < 丙在.top(),
        "与甲同平台（GB）的乙该排在丙前头：乙 {乙在:?}，丙 {丙在:?}",
    );
}
