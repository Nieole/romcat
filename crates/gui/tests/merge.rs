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
    干净工作目录, 正好那一段画在哪儿, 滚到底, 点一下, 画出来的字, 跑一帧
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
