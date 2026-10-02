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

    // **移的就是弹层上写着的那一个**：它把那个变体的键印在标题底下那张卡上。
    let 移的是 = 开了
        .lines()
        .find(|line| line.starts_with(&format!("{根}/")))
        .expect("弹层上该写着移的是哪一个变体")
        .to_string();

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
