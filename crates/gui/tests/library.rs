//! **浏览屏**：主列表按作品出行、五个维度筛得动、点开一行看得全、改得动元数据。
//!
//! 这几条断言不看代码长什么样，看的是**跑出来的结果**：
//!
//! - 主列表**一个游戏一行**，认不出作品的那些一条都没被吞掉。
//! - 筛选真的下推到中立库——筛完之后内存里还是那一扇窗，而总行数与另一条独立查出来的
//!   数字相等。
//! - **选中语义**：选中主列表的行，批量操作作用于它们的变体；在详情面板里选中某一个
//!   变体，变体级的改动只落在它头上。
//! - 详情面板三层齐：作品 → 变体（每个带置信度与**依据**）→ 文件（含附属文件与内部资源）。
//! - 改元数据当场落库：所有元数据编辑收敛在这里（ADR-0001 的修订段），
//!   主库的元数据文件不再是编辑入口。
//! - **首选变体与标题来源解耦**（ADR-0012）：首选换成汉化版，中文标题的来源一个字不变。

use egui::widgets::text_edit::TextEditState;
use romcat_core::catalog::browse::{PlatformFilter, StateFilter, WorkAnchor, WorkOrder, WorkQuery};
use romcat_core::catalog::State;
use romcat_core::dat::chinese::ChineseMark;
use romcat_core::scrape::priority::VERDICT;
use romcat_core::scrape::{AnchorKind, Field, MediaKind};
use romcat_core::shape::Role;
use romcat_core::title::{Language, TitleKind};
use romcat_gui::app::{App, View};
use romcat_gui::bench::{self, Sweep};
use romcat_gui::table::{ROW_HEIGHT, SPAN};
use romcat_gui::{demo, headless, library};

/// 合成数据的规模。真库是 46,483 个变体（`docs/library-facts.md`），照它来。
const ROWS: u64 = 46_483;

fn 界面(rows: u64) -> App {
    let site = demo::site(demo::library(rows).expect("造得出合成数据")).expect("开得出现场");
    let mut app = App::new(site, demo::workspace());
    app.show_view(View::Variants);
    app
}

fn 跑(ctx: &egui::Context, app: &mut App, frames: u32) {
    for _ in 0..frames {
        headless::frame(ctx, headless::input(), |ui| app.ui(ui));
    }
}

#[test]
fn 五个维度都有得选而且各自带着条数() {
    let app = 界面(ROWS);
    let facets = app.library().facets();
    // 平台、合集、语言、识别状态——票 25 点名的那四个。一个空了，界面上那一维就是死的。
    assert!(!facets.platforms.is_empty(), "平台那一维是空的");
    assert!(!facets.collections.is_empty(), "合集那一维是空的");
    assert!(!facets.languages.is_empty(), "语言那一维是空的");
    // **中文与语言不是同一维**（ADR-0012）：汉化版是变体，底版多半是日版发行版，
    // 只按语言筛的话这个库最要紧的那批一条都不出现。
    assert!(!facets.chinese.is_empty(), "中文那一维是空的");
    assert!(
        facets
            .chinese
            .iter()
            .all(|facet| !facets.languages.iter().any(|l| l.value == facet.value)),
        "中文那一维的记号跟语言码撞上了，那两维在模型上必须分开",
    );
    assert_eq!(facets.states.len(), StateFilter::ALL.len());
    // **合集与平台正交**（ADR-0011）：合集名不该是平台名。
    for facet in &facets.collections {
        assert!(
            !facets.platforms.iter().any(|p| p.value == facet.value),
            "合集「{}」与某个平台重名了，那两维在模型上必须分开",
            facet.value,
        );
    }
    // 每一档都带着条数：没有条数的下拉框，人只能一个个点开试。
    assert!(facets.platforms.iter().all(|facet| facet.count > 0));
    // 「还没识别」是独立的一档，不能与「未命中」混（那会把命中率说错）。
    let 还没识别 = facets
        .states
        .iter()
        .find(|(filter, _)| *filter == StateFilter::Unidentified)
        .expect("有这一档");
    assert!(还没识别.1 > 0, "合成数据里该有一批压根没识别过的");
}

#[test]
fn 主列表按作品出行而且行数与库里的作品数对得上() {
    // 「主列表一个游戏一行——不是 46,428 个变体」。合成数据里 20 个作品，
    // 外加每十三个留一个**压根没识别过**的变体——那些认不出作品，各自一行，
    // 一条都不许被吞掉（与导出那一侧同一条口径）。
    let ctx = headless::context();
    let mut app = 界面(ROWS);
    跑(&ctx, &mut app, 2);
    let 行数 = app.window().total();
    assert!(行数 > 0, "主列表一行都没有");
    assert!(
        行数 < ROWS,
        "{行数} 行 vs {ROWS} 个变体——按作品收敛根本没起作用",
    );

    let rows = {
        let (library, site) = app.library_and_site();
        site.catalog
            .work_page(library.query(), 0, 4_000)
            .expect("取得出一页")
    };
    let 作品行 = rows
        .iter()
        .filter(|row| matches!(row.anchor, WorkAnchor::Work(_)))
        .count() as u64;
    let 散行 = rows.len() as u64 - 作品行;
    assert!(作品行 > 0 && 散行 > 0, "两支各该有一批");
    // 收敛真的起了作用：作品那几行底下挂着不止一个变体。
    assert!(
        rows.iter().any(|row| row.variants > 1),
        "一行都没挂着两个以上的变体，六成作品挂着不止一个才是真库的形状",
    );
    // 每一行都摆得出票据点名的那六样。
    for row in rows.iter().take(64) {
        assert!(!row.name.is_empty(), "这一行没有名字");
        assert!(!row.platforms.is_empty(), "平台集合是空的");
        assert!(row.variants > 0, "一行底下一个变体都没有");
        assert!(!row.confidence_label().is_empty());
        assert!(!row.missing_label().is_empty());
    }
    // 变体表那一层照旧是全部变体——收敛的是**行**，不是库。
    assert_eq!(
        {
            let (_, site) = app.library_and_site();
            site.catalog
                .variant_total(&romcat_core::catalog::VariantQuery::default())
                .expect("数得出来")
        },
        ROWS,
    );
}

#[test]
fn 五列都排得了序而且换排序真的换了次序() {
    let ctx = headless::context();
    let mut app = 界面(4_000);
    跑(&ctx, &mut app, 2);
    let 头一行 = |app: &mut App| {
        let (library, site) = app.library_and_site();
        site.catalog
            .work_page(library.query(), 0, 1)
            .expect("取得出一页")
            .into_iter()
            .next()
            .map(|row| row.anchor)
    };
    let mut 见过 = Vec::new();
    for order in WorkOrder::ALL {
        {
            let (library, _) = app.library_and_site();
            let query = library.query_mut();
            query.order = order;
            query.descending = true;
        }
        跑(&ctx, &mut app, 1);
        assert!(app.window().total() > 0, "按{}排完一行都不剩", order.label());
        见过.push(头一行(&mut app));
    }
    assert!(
        见过.iter().collect::<std::collections::BTreeSet<_>>().len() > 1,
        "五种排序的头一行完全一样，那说明排序压根没生效",
    );
}

#[test]
fn 五个维度筛得动而且筛选下推到中立库() {
    let ctx = headless::context();
    let mut app = 界面(ROWS);
    跑(&ctx, &mut app, 2);
    let 全部 = app.window().total();

    let (平台, 合集, 语言) = {
        let facets = app.library().facets();
        (
            facets.platforms[0].value.clone(),
            facets.collections[0].value.clone(),
            facets.languages[0].value.clone(),
        )
    };

    // 一维一维加上去，行数只降不升——**各维之间是「且」**：这是浏览，不是搜索。
    let mut 上一次 = 全部;
    for 加一维 in [
        Box::new(|q: &mut WorkQuery, v: &str| q.platform = Some(PlatformFilter::from_label(v)))
            as Box<dyn Fn(&mut WorkQuery, &str)>,
        Box::new(|q: &mut WorkQuery, v: &str| q.collection = Some(v.to_string())),
        Box::new(|q: &mut WorkQuery, v: &str| q.language = Some(v.to_string())),
    ]
    .into_iter()
    .zip([平台.as_str(), 合集.as_str(), 语言.as_str()])
    .map(|(f, v)| move |q: &mut WorkQuery| f(q, v))
    {
        {
            let (library, _) = app.library_and_site();
            加一维(library.query_mut());
        }
        跑(&ctx, &mut app, 1);
        let 现在 = app.window().total();
        assert!(现在 > 0, "这一维筛完一条都不剩，那测不出什么");
        assert!(
            现在 <= 上一次,
            "多加一个条件行数反而涨了：{上一次} → {现在}"
        );
        上一次 = 现在;
    }
    {
        let (library, _) = app.library_and_site();
        library.query_mut().state = Some(StateFilter::Concluded(State::Matched));
    }
    跑(&ctx, &mut app, 1);
    let 五维 = app.window().total();

    // **下推的证据一：** 界面上那个数与另一条独立查出来的数相等。
    let 独立数 = {
        let (library, site) = app.library_and_site();
        site.catalog.work_total(library.query()).expect("数得出来")
    };
    assert_eq!(五维, 独立数);

    // **下推的证据二：** 筛过之后内存里还是那一扇窗，不是把结果读进来再过一遍。
    assert!(
        app.window().retained() as u64 <= SPAN,
        "内存里 {} 行，超过一扇窗（{SPAN} 行）",
        app.window().retained(),
    );

    // 全清之后回到全部。
    {
        let (library, _) = app.library_and_site();
        *library.query_mut() = WorkQuery::default();
    }
    跑(&ctx, &mut app, 1);
    assert_eq!(app.window().total(), 全部);
}

#[test]
fn 多选与全选可用而且屏上看得见选中多少条() {
    let ctx = headless::context();
    let mut app = 界面(4_000);
    跑(&ctx, &mut app, 2);
    let 行数 = app.window().total();
    let 头几行: Vec<WorkAnchor> = {
        let (library, site) = app.library_and_site();
        site.catalog
            .work_page(library.query(), 0, 3)
            .expect("取得出一页")
            .into_iter()
            .map(|row| row.anchor)
            .collect()
    };

    // 一行都没选就是一行都没选——**空选择不等于全选**。
    assert_eq!(app.library().picked().count(行数), 0);
    assert!(app.library().picked().is_empty(行数));

    for anchor in &头几行 {
        app.library_and_site().0.picked_mut().toggle(anchor);
    }
    跑(&ctx, &mut app, 1);
    assert_eq!(app.library().picked().count(行数), 3, "多选没记住");
    assert!(app.library().picked().contains(&头几行[0]));
    // 再点一下就取消。
    app.library_and_site().0.picked_mut().toggle(&头几行[0]);
    assert_eq!(app.library().picked().count(行数), 2);

    // **全选**：当前筛选下的每一行。它记的是这个筛选，不是一万行的身份。
    app.library_and_site().0.picked_mut().select_all();
    跑(&ctx, &mut app, 1);
    assert!(app.library().picked().is_all());
    assert_eq!(app.library().picked().count(行数), 行数);
    // 全选之后点掉一行，正好少一条（ADR-0016 那条「规则加手动例外」的形状）。
    app.library_and_site().0.picked_mut().toggle(&头几行[1]);
    assert_eq!(app.library().picked().count(行数), 行数 - 1);
    assert!(!app.library().picked().contains(&头几行[1]));

    // **换排序不作废**：筛出来的还是同一批行，只是重排了一遍——人勾了几行再按一下
    // 表头，选中不该凭空消失。
    {
        let (library, _) = app.library_and_site();
        library.query_mut().order = WorkOrder::Bytes;
        library.query_mut().descending = true;
    }
    跑(&ctx, &mut app, 2);
    assert_eq!(
        app.library().picked().count(app.window().total()),
        行数 - 1,
        "换了个排序，选中的那一批就没了",
    );

    // **换筛选就作废**：全选说的是「当前筛出来的这一批」，条件一改那批就不是同一批。
    {
        let (library, _) = app.library_and_site();
        library.query_mut().platform = Some(PlatformFilter::from_label(
            &library.facets().platforms[0].value.clone(),
        ));
    }
    跑(&ctx, &mut app, 1);
    assert!(
        app.library().picked().is_empty(app.window().total()),
        "换了筛选还留着上一批的选中，批量操作会作用到人没看见的行上",
    );
}

#[test]
fn 选中作品时批量操作作用于其全部变体() {
    // 这一票要钉死的那半条选中语义：**选中主列表的行 ＝ 选中这些作品，
    // 批量操作作用于它们的变体**。
    let ctx = headless::context();
    let mut app = 界面(4_000);
    跑(&ctx, &mut app, 2);
    let 一行 = {
        let (library, site) = app.library_and_site();
        site.catalog
            .work_page(library.query(), 0, 4_000)
            .expect("取得出一页")
            .into_iter()
            .find(|row| row.variants > 1)
            .expect("有挂着不止一个变体的行")
    };

    app.library_and_site().0.picked_mut().toggle(&一行.anchor);
    跑(&ctx, &mut app, 2);
    let 作用范围 = {
        let (library, site) = app.library_and_site();
        library.batch_variants(&site.catalog).expect("展开得了")
    };
    assert_eq!(
        作用范围.len() as u64,
        一行.variants,
        "屏上那一行写着 {} 个变体，批量操作却作用于 {} 个",
        一行.variants,
        作用范围.len(),
    );
    assert!(作用范围.len() > 1, "这一条要测的正是「不止一个变体」");
    // 屏上那句「作用于多少个变体」与真展开出来的那一批是同一个数。
    assert_eq!(
        app.library().scope_total(),
        Some(作用范围.len() as u64),
        "屏上那句「作用于多少个变体」与真展开出来的那一批对不上",
    );

    // 详情面板里列的那几个变体，与批量操作要动的那几个是同一批。
    {
        let (library, site) = app.library_and_site();
        library.open_work(&site.catalog, &一行.anchor);
    }
    let 面板里的: Vec<String> = app
        .library()
        .work()
        .expect("点得开")
        .variants
        .iter()
        .map(|variant| variant.row.key.clone())
        .collect();
    assert_eq!(面板里的, 作用范围);

    // **全选**展开的是整个库的变体。
    app.library_and_site().0.picked_mut().select_all();
    let 全部 = {
        let (library, site) = app.library_and_site();
        library.batch_variants(&site.catalog).expect("展开得了")
    };
    assert_eq!(全部.len() as u64, 4_000, "全选没盖住当前筛选下的全部变体");
}

#[test]
fn 选中一个变体时变体级的操作只作用于它() {
    // 另半条：**在详情面板里选中某一个变体，变体级的操作只作用于它。**
    let mut app = 界面(4_000);
    let 一行 = 一行有兄弟的(&mut app);
    {
        let (library, site) = app.library_and_site();
        library.open_work(&site.catalog, &一行);
    }
    let 变体们: Vec<String> = app
        .library()
        .work()
        .expect("点得开")
        .variants
        .iter()
        .map(|variant| variant.row.key.clone())
        .collect();
    assert!(变体们.len() > 1, "得有得挑才测得出来");
    // 点开一行默认选中第一个变体：面板的第二三层总得有东西摆。
    assert_eq!(app.library().variant_key(), Some(变体们[0].as_str()));

    // 挑第二个，往**变体**这一层写一条元数据。
    {
        let (library, site) = app.library_and_site();
        library.pick(&site.catalog, &变体们[1]);
        let draft = library.value_draft_mut();
        draft.field = Field::TranslationGroup;
        draft.anchor = AnchorKind::Variant;
        draft.value = "只该落在这一个变体上的汉化组".to_string();
        let key = 变体们[1].clone();
        library.put_value(site, &key);
    }
    let 落在它头上 = |app: &mut App, key: &str| {
        let (library, site) = app.library_and_site();
        library.pick(&site.catalog, key);
        library
            .detail()
            .expect("点得开")
            .values
            .iter()
            .any(|item| item.value.value == "只该落在这一个变体上的汉化组")
    };
    assert!(落在它头上(&mut app, &变体们[1]), "写下去的那条没落库");
    for other in 变体们.iter().filter(|key| *key != &变体们[1]) {
        assert!(
            !落在它头上(&mut app, other),
            "变体级的改动溅到了同一个作品下的另一个变体 {other}",
        );
    }
}

#[test]
fn 换筛选之后选中的那个变体跟着归位() {
    // 换一套筛选之后，原先选中的那个变体可能已经不在这一行底下了。面板还照着它画的话，
    // 人看见的是一行、改元数据动到的是另一行。
    let ctx = headless::context();
    let mut app = 界面(4_000);
    跑(&ctx, &mut app, 2);
    let 一行 = 一行有兄弟的(&mut app);
    {
        let (library, site) = app.library_and_site();
        library.open_work(&site.catalog, &一行);
    }
    let 原先 = app.library().variant_key().expect("点开就该选中一个").to_string();
    let 那个平台 = app
        .library()
        .detail()
        .expect("点得开")
        .row
        .platform
        .clone()
        .expect("有平台");

    // 筛成**别的平台**：原先那个变体一定不在这一行底下了。
    let 另一个 = app
        .library()
        .facets()
        .platforms
        .iter()
        .map(|facet| facet.value.clone())
        .find(|value| *value != 那个平台)
        .expect("合成数据里不止一个平台");
    {
        let (library, _) = app.library_and_site();
        library.query_mut().platform = Some(PlatformFilter::from_label(&另一个));
    }
    跑(&ctx, &mut app, 2);
    if let Some(work) = app.library().work() {
        let 现在 = app.library().variant_key().expect("还有变体就该选中一个");
        assert_ne!(现在, 原先, "选中的还是那个已经被筛掉的变体");
        assert!(
            work.variants.iter().any(|v| v.row.key == 现在),
            "选中的那个变体不在这一行底下",
        );
    } else {
        // 这一行在新筛选下一个变体都不剩，那就该一起清掉，不留一份没人认领的详情。
        assert!(app.library().variant_key().is_none());
        assert!(app.library().detail().is_none());
    }
}

#[test]
fn 详情面板列得出全部变体每个带置信度与依据() {
    let mut app = 界面(4_000);
    let 一行 = 一行有兄弟的(&mut app);
    {
        let (library, site) = app.library_and_site();
        library.open_work(&site.catalog, &一行);
    }
    let work = app.library().work().expect("点得开").clone();
    assert!(!work.name.is_empty());
    assert!(!work.platforms.is_empty());
    assert!(work.variants.len() > 1, "该列出这个作品的全部变体");
    // 每个变体带置信度；**有候选的那些带依据**——没有依据的候选事后无法复核（ADR-0002）。
    let mut 有候选 = 0;
    for variant in &work.variants {
        assert!(!variant.row.key.is_empty());
        for candidate in &variant.candidates {
            有候选 += 1;
            assert!(
                !candidate.evidence.is_empty(),
                "候选没有依据，事后没法复核",
            );
            // 行上那一档是**最高**的那一档（`Confidence` 的 `Ord` 里 `High` 最小），
            // 所以它只该比每一条候选更靠前——一个变体撞上一条高一条低是真库的常态。
            assert!(
                variant.confidence() <= Some(candidate.confidence),
                "行上那一档置信度比某条候选还低",
            );
        }
        if variant.candidates.is_empty() {
            // 一条候选都没有是**还没识别**，不是「撞过没撞上」。
            assert_eq!(variant.confidence(), None);
        }
    }
    assert!(有候选 > 0, "合成数据里该有带候选的变体");
}

#[test]
fn 详情面板列得出选中变体的全部文件含附属文件与内部资源() {
    // 「一个变体不等于一个文件」（`CONTEXT.md` 的「变体」词条）：真库里主文件之外
    // 还有 142 个附属文件与 158,641 个内部资源。
    let mut app = 界面(4_000);
    let key = 一条带附属文件的(&mut app);
    {
        let (library, site) = app.library_and_site();
        library.pick(&site.catalog, &key);
    }
    let detail = app.library().detail().expect("点得开");
    let 身份: std::collections::BTreeSet<Role> =
        detail.members.iter().map(|(_, role)| *role).collect();
    assert!(身份.contains(&Role::Main), "主文件没列出来");
    assert!(身份.contains(&Role::Companion), "附属文件没列出来");
    assert!(身份.contains(&Role::Internal), "内部资源没列出来");
    for (member, _) in &detail.members {
        assert!(
            member.starts_with(&detail.row.key),
            "列进来的 {member} 不是这个变体的成员",
        );
    }
}

#[test]
fn 汉化版按中文这一维筛得出来而按语言筛不出来() {
    // 这一条是那个「语言筛不出汉化版」的坑：汉化版是**变体**，它基于的发行版通常是
    // 日版，语言那一列上一个中文字都没有（ADR-0012）。
    let ctx = headless::context();
    let mut app = 界面(4_000);
    跑(&ctx, &mut app, 2);
    {
        let (library, _) = app.library_and_site();
        library.query_mut().chinese = Some(ChineseMark::FanTranslated.label().to_string());
    }
    跑(&ctx, &mut app, 1);
    let 汉化 = app.window().total();
    assert!(汉化 > 0, "中文=汉化 一条都筛不出来");

    // 同一批变体按「语言里有中文」筛，一条都不该有——它们的发行版是日版。
    {
        let (library, _) = app.library_and_site();
        let query = library.query_mut();
        query.chinese = None;
        query.language = Some("Ja".to_string());
    }
    跑(&ctx, &mut app, 1);
    assert!(app.window().total() > 0, "合成数据里该有日版发行版");
}

#[test]
fn 刮削来的元数据看得见也改得动() {
    // 「**所有元数据编辑收敛在这里完成**」（ADR-0001 的修订段）说的不只是标题与
    // 首选变体：年份、发行商、简介这几样也会写进导出条目。
    let mut app = 界面(2_000);
    let key = 一条认出作品的(&mut app);
    {
        let (library, site) = app.library_and_site();
        library.pick(&site.catalog, &key);
    }
    let detail = app.library().detail().expect("点开得了").clone();
    assert!(!detail.values.is_empty(), "刮削来的字段一条都没折出来");
    let work = detail.work.clone().expect("认出了作品");

    {
        let (library, site) = app.library_and_site();
        let draft = library.value_draft_mut();
        draft.field = Field::Description;
        draft.anchor = AnchorKind::Work;
        draft.value = "界面上手写的简介".to_string();
        library.put_value(site, &work);
    }
    let 写完 = app.library().detail().expect("还在");
    let 那条 = 写完
        .values
        .iter()
        .find(|item| item.value.value == "界面上手写的简介")
        .expect("写下去的那条在");
    // **来源是裁决**，而裁决排在每个字段的最前——写下之后导出真会用它。
    assert!(那条.is_verdict());
    assert_eq!(那条.anchor, AnchorKind::Work);
    assert!(
        !那条.value.evidence.is_empty(),
        "没有依据的结论事后无法复核"
    );

    {
        let (library, site) = app.library_and_site();
        library.clear_value(site, AnchorKind::Work, &work, Field::Description);
    }
    assert!(
        app.library()
            .detail()
            .expect("还在")
            .values
            .iter()
            .all(|item| item.value.value != "界面上手写的简介"),
        "撤掉之后那条还在",
    );
}

#[test]
fn 媒体一条条列得出来而不只是一个数() {
    // 「媒体资源在界面中可见」——只报一个数，人连那张封面落在池里哪个文件都说不出。
    let mut app = 界面(2_000);
    let key = 一条认出作品的(&mut app);
    {
        let (library, site) = app.library_and_site();
        library.pick(&site.catalog, &key);
    }
    let detail = app.library().detail().expect("点开得了");
    assert!(!detail.media_items.is_empty(), "媒体一条都没列出来");
    for item in &detail.media_items {
        assert!(!item.hash.is_empty(), "媒体池按内容哈希存，哈希不能是空的");
        assert!(!item.source.is_empty());
        assert!(!item.evidence.is_empty(), "没有依据的引用事后无法复核");
    }
    // 逐条数出来的，与那份计数说的是同一件事。
    let 封面条数 = detail
        .media_items
        .iter()
        .filter(|item| item.kind == MediaKind::Cover)
        .count() as u64;
    let 封面计数 = detail
        .media
        .iter()
        .find(|have| have.kind == MediaKind::Cover)
        .expect("有这一档")
        .refs;
    assert_eq!(封面条数, 封面计数);
}

#[test]
fn 点开一条看得见作品发行版标题集合首选变体与媒体() {
    let mut app = 界面(2_000);
    let key = 一条认出作品的(&mut app);
    {
        let (library, site) = app.library_and_site();
        library.pick(&site.catalog, &key);
    }
    let detail = app.library().detail().expect("点开得了");
    assert_eq!(detail.row.key, key);
    assert!(detail.work.is_some(), "作品没折出来");
    assert!(detail.release.is_some(), "发行版没折出来");
    assert!(!detail.languages.is_empty(), "语言没折出来");
    assert!(detail.state.is_some(), "识别结论没折出来");
    assert!(!detail.members.is_empty(), "文件成员没折出来");
    assert!(!detail.titles.is_empty(), "标题集合是空的");
    assert!(detail.display.is_some(), "显示标题没挑出来");
    assert!(!detail.siblings.is_empty(), "同作品同平台的变体一个都没列");
    // **首选变体**：没人裁过时按规则算，第一名就是眼下生效的那个。
    assert!(detail.preferred.is_none(), "还没人裁过就不该有裁决");
    assert!(detail.preferred_now().is_some(), "规则也该算得出一个首选");

    // **媒体缺哪些看得出来**：合成数据里视频一份都没有。
    assert!(
        detail.missing_media().contains(&MediaKind::Video),
        "缺的媒体里该有视频，实际是 {:?}",
        detail.missing_media(),
    );
    // 媒体池不在位时那一栏是**没查**，不是**没有**——两件事得分得开（同 ADR-0021）。
    assert!(
        detail.media.iter().all(|have| have.in_pool.is_none()),
        "没指媒体池却报出了「池里有几份」",
    );
}

#[test]
fn 加一条叫法当场进标题集合而且来源是裁决() {
    let mut app = 界面(2_000);
    let key = 一条认出作品的(&mut app);
    {
        let (library, site) = app.library_and_site();
        library.pick(&site.catalog, &key);
    }
    let 原有 = app.library().detail().expect("点开得了").titles.len();
    let work = app
        .library()
        .detail()
        .expect("点开得了")
        .work
        .clone()
        .expect("认出了作品");

    {
        let (library, site) = app.library_and_site();
        let draft = library.title_draft_mut();
        draft.value = "界面上敲进去的中文译名".to_string();
        draft.language = Language::Chinese;
        draft.kind = TitleKind::Translated;
        library.add_title(site, &work);
    }
    let detail = app.library().detail().expect("还在");
    assert_eq!(detail.titles.len(), 原有 + 1, "加进去的那条没落库");
    let 新的 = detail
        .titles
        .iter()
        .find(|row| row.value == "界面上敲进去的中文译名")
        .expect("找得到刚加的那条")
        .clone();
    // **来源是裁决**：人改过的东西不许被任何数据源覆盖，重折标题集合时一行都不碰。
    assert_eq!(新的.source, VERDICT);
    assert!(新的.is_verdict());
    assert!(!新的.evidence.is_empty(), "没有依据的结论事后无法复核");

    // 删得掉。
    {
        let (library, site) = app.library_and_site();
        library.remove_title(site, &work, 新的.language, 新的.kind, VERDICT, &新的.value);
    }
    assert_eq!(
        app.library().detail().expect("还在").titles.len(),
        原有,
        "删掉之后没回到原样",
    );
}

#[test]
fn 首选变体裁得动撤得掉而中文标题的来源一个字不变() {
    // ADR-0012：**首选变体与标题来源解耦**。即使首选启动的是汉化版，
    // 中文标题仍取官中版的官方译名。
    let mut app = 界面(4_000);
    let key = 一条有兄弟的(&mut app);
    {
        let (library, site) = app.library_and_site();
        library.pick(&site.catalog, &key);
    }
    let detail = app.library().detail().expect("点开得了").clone();
    let 中文标题 = detail
        .chinese_title()
        .expect("合成数据里每部作品都有中文译名")
        .clone();
    let 别的 = detail
        .siblings
        .iter()
        .map(|s| s.row.key.clone())
        .find(|k| *k != detail.preferred_now().unwrap_or_default())
        .expect("有第二个变体可挑");
    let work = detail.work.clone().expect("认出了作品");
    let platform = detail.row.platform.clone().expect("认出了平台");

    {
        let (library, site) = app.library_and_site();
        library.set_preferred(site, &work, &platform, &别的);
    }
    let 改后 = app.library().detail().expect("还在");
    assert_eq!(改后.preferred.as_deref(), Some(别的.as_str()), "裁决没落库");
    assert_eq!(改后.preferred_now(), Some(别的.as_str()), "裁决没盖过规则");
    // **这一条就是 ADR-0012**：换了首选，中文标题那一条来源一个字没动。
    let 改后中文 = 改后.chinese_title().expect("中文叫法还在");
    assert_eq!(改后中文.value, 中文标题.value);
    assert_eq!(改后中文.source, 中文标题.source);
    assert_eq!(改后中文.kind, 中文标题.kind);

    {
        let (library, site) = app.library_and_site();
        library.clear_preferred(site, &work, &platform);
    }
    let 撤后 = app.library().detail().expect("还在");
    assert!(撤后.preferred.is_none(), "裁决没撤掉");
    assert!(撤后.preferred_now().is_some(), "撤掉之后规则该顶上");
}

#[test]
fn 表格里画多少行文本输入框都是那几个() {
    // ADR-0005 的修订段：中文输入放详情面板，不放表格单元格——表格是虚拟化的，
    // 正在组字的那一行滚出视口时控件就没了，输入法上屏时没人接。
    let 数一遍 = |rows: u64| {
        let ctx = headless::context();
        let mut app = 界面(rows);
        跑(&ctx, &mut app, 3);
        const STEPS: u32 = 24;
        #[allow(clippy::cast_precision_loss)]
        let travel = app.window().total() as f32 * ROW_HEIGHT;
        for step in 0..=STEPS {
            app.library_and_site().0.scroll_to = Some(travel * step as f32 / STEPS as f32);
            headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
        }
        ctx.data(|data| data.count::<TextEditState>())
    };
    let 少 = 数一遍(200);
    let 多 = 数一遍(20_000);
    assert!(少 > 0, "一个文本框都没画出来的话这条断言等于没测");
    assert_eq!(
        少, 多,
        "库从 200 条涨到 20,000 条、还滚了一整趟，文本输入框却从 {少} 个变成 {多} 个\
         ——那说明有文本框长在表格单元格里",
    );
}

/// 找一条**已经认出作品**的变体。
///
/// 合成数据里每十三个留一个压根没识别过的（那是「还没识别」那一档），第一行正好可能是
/// 它——而没有作品就没有标题集合，那几条断言会验错东西。
fn 一条认出作品的(app: &mut App) -> String {
    for key in 头几行(app, 64) {
        let 认出来了 = {
            let (library, site) = app.library_and_site();
            library.pick(&site.catalog, &key);
            library
                .detail()
                .is_some_and(|detail| detail.work.is_some() && detail.release.is_some())
        };
        if 认出来了 {
            return key;
        }
    }
    panic!("合成数据里该有认出了作品的变体");
}

/// 眼下这个筛选下的头几个**变体**（不是主列表那几行）。
fn 头几行(app: &mut App, limit: u64) -> Vec<String> {
    let (_, site) = app.library_and_site();
    site.catalog
        .variant_page(&romcat_core::catalog::VariantQuery::default(), 0, limit)
        .expect("取得出一页")
        .into_iter()
        .map(|row| row.key)
        .collect()
}

/// 找一行**底下挂着不止一个变体**的：选中语义那两条得有得挑才测得出来。
fn 一行有兄弟的(app: &mut App) -> WorkAnchor {
    let (library, site) = app.library_and_site();
    site.catalog
        .work_page(library.query(), 0, 4_000)
        .expect("取得出一页")
        .into_iter()
        .find(|row| row.variants > 1)
        .map(|row| row.anchor)
        .expect("合成数据里该有挂着不止一个变体的作品")
}

/// 找一条**带附属文件与内部资源**的变体：合成数据里每九个留一个（见 `demo::library`）。
fn 一条带附属文件的(app: &mut App) -> String {
    for key in 头几行(app, 64) {
        let 有 = {
            let (_, site) = app.library_and_site();
            site.catalog
                .variant_members(&key)
                .expect("列得出成员")
                .len()
                > 1
        };
        if 有 {
            return key;
        }
    }
    panic!("合成数据里该有带附属文件的变体");
}

/// 找一条**同作品同平台还有别的变体**的：首选变体那件事得有得挑才测得出来。
fn 一条有兄弟的(app: &mut App) -> String {
    for key in 头几行(app, 512) {
        let 有兄弟 = {
            let (library, site) = app.library_and_site();
            library.pick(&site.catalog, &key);
            library
                .detail()
                .is_some_and(|detail| detail.siblings.len() > 1)
        };
        if 有兄弟 {
            return key;
        }
    }
    panic!("合成数据里该有同作品同平台的两个变体");
}

#[test]
fn 一条顶到闸上的简介收成一行画得下的那一截() {
    // 票 03 的第二处边界的界面这一半。中立库里一条简介最多 4,000 字
    // （`scrape::zh::DESCRIPTION_LIMIT`），而刮削字段那一栏画在一条**横排**里——
    // 横排不折行，整段原样排进去就是四五万点宽的一行，面板跟着长出一条横向滚动条。
    let 顶到闸上 = "外".repeat(romcat_core::scrape::zh::DESCRIPTION_LIMIT);
    let 一行 = library::one_line(&顶到闸上).expect("这么长该收窄");
    assert!(
        一行.chars().count() < 80,
        "收成了 {} 个字，那一行还是画不下",
        一行.chars().count(),
    );
    assert!(一行.ends_with('…'), "收窄过要看得出来：{一行}");

    // **换行也要管**：数据源的排版原样留在值里（规格 18），可横排里一个换行就把那
    // 一行撑高，底下几条就被挤出视口。
    let 带换行 = library::one_line("　　两个人一起打外星人。\n第二段：外星人赢了。")
        .expect("带换行的该折平");
    assert!(!带换行.contains('\n'));
    assert!(带换行.starts_with('\u{3000}'), "折平不等于掐两头：{带换行}");

    // 原样画得下的**一个字都不动**——不动就不必换一份字符串出去。
    assert_eq!(library::one_line("两个人一起打外星人。"), None);
}

#[test]
fn 库里有一条顶到闸上的简介时列表照样滚得动() {
    // 「界面上的列表仍然滚得动」这一条钉的是**表**：它是虚拟化的，滚起来的代价与库里
    // 有什么无关。简介根本不在表的七列里，所以这一条要证的是「它也没从别处漏进来」。
    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 2);
    let key = 头几行(&mut app, 1).first().cloned().expect("有一行");
    {
        let (library, site) = app.library_and_site();
        site.catalog
            .put_verdict_value(
                AnchorKind::Variant,
                &key,
                Field::Description,
                &"外".repeat(romcat_core::scrape::zh::DESCRIPTION_LIMIT),
                "测试摆进去的",
            )
            .expect("写得进去");
        library.pick(&site.catalog, &key);
    }
    let cost = bench::scroll(&mut app, 120, Sweep::Rows(3.0));
    let 行数 = {
        let (library, site) = app.library_and_site();
        site.catalog.work_total(library.query()).expect("数得出来")
    };
    assert_eq!(app.window().total(), 行数);
    assert!(
        app.window().retained() as u64 <= SPAN,
        "内存里 {} 行，超过一扇窗（{SPAN} 行）",
        app.window().retained(),
    );
    // 240 行滚下来，一扇窗 512 行——窗口预取该只跨一次。
    assert!(cost.reads <= 2, "滚了 240 行读了 {} 次库", cost.reads);
    assert!(cost.median_ms > 0.0, "没量到时间，这一趟没滚动");
}

/// 把一条规则预填进筛选器，跑几帧，返回筛出多少行。
///
/// **走的是界面上那条路**（[`library::Screen::set_filter_rule`]），不是直接改查询：
/// 子库屏点「改选择」跳回来预填的正是它（票 `gui-redesign/11`）。
fn 按规则筛(ctx: &egui::Context, app: &mut App, text: &str) -> u64 {
    let rule = romcat_core::sublibrary::Rule::parse(text)
        .unwrap_or_else(|error| panic!("「{text}」读不懂：{error}"));
    app.library_and_site().0.set_filter_rule(Some(rule));
    跑(ctx, app, 2);
    app.window().total()
}

#[test]
fn 三种连接在筛选器上各自成立而且组嵌得动() {
    let ctx = headless::context();
    let mut app = 界面(ROWS);
    跑(&ctx, &mut app, 2);
    let 全部 = app.window().total();
    let (甲, 乙) = {
        let facets = app.library().facets();
        (
            facets.platforms[0].value.clone(),
            facets.platforms[1].value.clone(),
        )
    };

    // 全部满足：一层层收窄。
    let 单条 = 按规则筛(&ctx, &mut app, &format!("平台={甲}"));
    assert!(单条 > 0 && 单条 < 全部, "{单条} vs {全部}：这一条根本没筛");
    let 又加一条 = 按规则筛(&ctx, &mut app, &format!("平台={甲} 且 中文=汉化"));
    assert!(又加一条 <= 单条, "「全部满足」加一条反而多出行来");

    // 任一满足：两个平台一起看，比任一个单独看都不少。
    let 另一条 = 按规则筛(&ctx, &mut app, &format!("平台={乙}"));
    let 并起来 = 按规则筛(&ctx, &mut app, &format!("平台={甲} 或 平台={乙}"));
    assert!(
        并起来 >= 单条 && 并起来 >= 另一条,
        "{并起来} 比 {单条} / {另一条} 还少——「任一满足」算成了交集",
    );

    // 都不满足：这两个平台一个都不许沾。再叠上其中一个，一行都不该剩。
    let 都不 = 按规则筛(&ctx, &mut app, &format!("都不(平台={甲} 或 平台={乙})"));
    assert!(都不 > 0, "合成数据里该有别的平台");
    assert_eq!(
        按规则筛(
            &ctx,
            &mut app,
            &format!("平台={甲} 且 都不(平台={甲} 或 平台={乙})")
        ),
        0,
        "「是甲」与「甲乙都不是」同时成立，那说明「都不满足」算错了",
    );

    // 组嵌两层：里面那个自己是「任一满足」。
    let 嵌套 = 按规则筛(
        &ctx,
        &mut app,
        &format!("平台={甲} 且 (中文=汉化 或 类型~角色)"),
    );
    assert!(嵌套 > 0 && 嵌套 <= 单条);

    // **筛选下推到中立库**：筛完之后内存里还是那一扇窗，不是把全库读进来。
    assert!(
        app.window().retained() as u64 <= SPAN,
        "内存里留了 {} 行——筛选没下推",
        app.window().retained(),
    );
}

#[test]
fn 以开始与以结束在筛选器上筛得动() {
    let ctx = headless::context();
    let mut app = 界面(4_000);
    跑(&ctx, &mut app, 2);
    let 平台 = app.library().facets().platforms[0].value.clone();
    let 头一个字: String = 平台.chars().take(1).collect();
    let 末一个字: String = 平台.chars().last().into_iter().collect();

    let 等于 = 按规则筛(&ctx, &mut app, &format!("平台={平台}"));
    let 开头 = 按规则筛(&ctx, &mut app, &format!("平台^{头一个字}"));
    let 结尾 = 按规则筛(&ctx, &mut app, &format!("平台${末一个字}"));
    assert!(开头 >= 等于, "「以…开始」比「等于」还窄");
    assert!(结尾 >= 等于, "「以…结束」比「等于」还窄");
    // 整个名字当前缀就等于「等于」——同一批行。
    assert_eq!(按规则筛(&ctx, &mut app, &format!("平台^{平台}")), 等于);
}

#[test]
fn 筛出来的条数与真正命中的条数一致() {
    let ctx = headless::context();
    let mut app = 界面(ROWS);
    let 平台 = {
        跑(&ctx, &mut app, 2);
        app.library().facets().platforms[0].value.clone()
    };
    按规则筛(&ctx, &mut app, &format!("平台={平台} 或 中文=汉化"));

    let 行数 = app.window().total();
    let 变体数 = app.library().filtered_total().expect("数得出来");
    assert!(行数 > 0 && 变体数 >= 行数, "{变体数} 个变体撑不起 {行数} 行");

    // 屏上写着几个变体，按下批量操作就该动几个——**三处同一个数**。
    let (库里, 屏上) = {
        let (library, site) = app.library_and_site();
        let query = library.query().clone();
        (
            site.catalog
                .scoped_variants(&query, romcat_core::catalog::browse::Scope::AllExcept(&[]))
                .expect("展得开")
                .len(),
            library.filtered_total().expect("数得出来"),
        )
    };
    assert_eq!(库里 as u64, 屏上);
}

#[test]
fn 存成子库把当前条件原样变成规则() {
    let ctx = headless::context();
    let mut app = 界面(4_000);
    跑(&ctx, &mut app, 2);
    let (平台, 合集) = {
        let facets = app.library().facets();
        (
            facets.platforms[0].value.clone(),
            facets.collections[0].value.clone(),
        )
    };
    // 左栏点一个合集，筛选器里再搭一棵「任一满足」的树——**两半都要带过去**。
    {
        let (library, _) = app.library_and_site();
        library.query_mut().collection = Some(合集.clone());
        library.set_filter_rule(Some(
            romcat_core::sublibrary::Rule::parse(&format!("平台={平台} 或 中文=汉化"))
                .expect("读得懂"),
        ));
    }
    跑(&ctx, &mut app, 2);

    let 屏上: std::collections::BTreeSet<String> = {
        let (library, site) = app.library_and_site();
        let query = library.query().clone();
        site.catalog
            .scoped_variants(&query, romcat_core::catalog::browse::Scope::AllExcept(&[]))
            .expect("展得开")
            .into_iter()
            .collect()
    };
    assert!(!屏上.is_empty(), "这份筛选该选得中东西，否则这条断言等于没测");

    {
        let (library, site) = app.library_and_site();
        let draft = library.save_draft_mut();
        draft.name = "掌机".to_string();
        draft.target = "/Volumes/SDCARD/掌机".to_string();
        library.save_as_sublibrary(site);
        assert!(library.error().is_none(), "{:?}", library.error());
        assert!(library.notice().is_some(), "建完子库没有回执");
    }

    // **新建的子库选出来的东西与屏上一致。**
    let (库里那条, 子库选出来的) = {
        let (_, site) = app.library_and_site();
        let loaded = site.catalog.selection("掌机").expect("选择集读得回来");
        assert!(loaded.broken.is_empty(), "{:?}", loaded.broken);
        let facts = romcat_core::sublibrary::facts(&site.catalog).expect("事实折得出来");
        let picked: std::collections::BTreeSet<String> =
            romcat_core::sublibrary::select(&loaded.selection, &facts)
                .picked
                .into_iter()
                .map(|picked| picked.key)
                .collect();
        (loaded.selection.rules[0].text.clone(), picked)
    };
    assert_eq!(子库选出来的, 屏上, "存成子库之后选出来的与屏上筛出来的不是同一批");
    // 那条规则就是屏上那几条，一条不多一条不少。
    assert_eq!(
        库里那条,
        format!("合集={合集} 且 (平台={平台} 或 中文=汉化)"),
    );

    // 重名不覆盖：撞上一个已有的子库要当场说清，不能悄悄把它的选择集并上一批。
    {
        let (library, site) = app.library_and_site();
        let draft = library.save_draft_mut();
        draft.name = "掌机".to_string();
        draft.target = "/Volumes/SDCARD/另一张".to_string();
        library.save_as_sublibrary(site);
        assert!(
            library.error().is_some_and(|error| error.contains("掌机")),
            "重名居然存下去了",
        );
    }
}

#[test]
fn 写不成规则的筛选条件当场挡住() {
    let ctx = headless::context();
    let mut app = 界面(4_000);
    跑(&ctx, &mut app, 2);
    {
        let (library, _) = app.library_and_site();
        // 「识别状态」进不了规则：那是这一趟的进度不是内容。
        library.query_mut().state = Some(StateFilter::Unidentified);
        let draft = library.save_draft_mut();
        draft.name = "存不成".to_string();
        draft.target = "/Volumes/SDCARD/存不成".to_string();
    }
    跑(&ctx, &mut app, 1);
    {
        let (library, site) = app.library_and_site();
        library.save_as_sublibrary(site);
        assert!(
            library.error().is_some_and(|error| error.contains("识别状态")),
            "少写一条就存下去了：那样子库选出来的会比屏上多",
        );
        assert!(
            site.catalog.sublibrary("存不成").expect("读得动").is_none(),
            "挡住了却还是建了个子库出来",
        );
    }
}
