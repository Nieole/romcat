//! **库浏览**：四个维度筛得动、点开一条看得全、改得动元数据。
//!
//! 这几条断言不看代码长什么样，看的是**跑出来的结果**：
//!
//! - 筛选真的下推到中立库——筛完之后内存里还是那一扇窗，而总行数与另一条独立查出来的
//!   数字相等。
//! - 详情面板要的八样（作品、发行版、合集、语言、识别结论、**标题集合**、
//!   **首选变体**、**媒体**）一次折得齐。
//! - 改元数据当场落库：所有元数据编辑收敛在这里（ADR-0001 的修订段），
//!   主库的元数据文件不再是编辑入口。
//! - **首选变体与标题来源解耦**（ADR-0012）：首选换成汉化版，中文标题的来源一个字不变。

use egui::widgets::text_edit::TextEditState;
use romcat_core::catalog::browse::StateFilter;
use romcat_core::catalog::{State, VariantQuery};
use romcat_core::scrape::MediaKind;
use romcat_core::scrape::priority::VERDICT;
use romcat_core::title::{Language, TitleKind};
use romcat_gui::app::{App, View};
use romcat_gui::table::{ROW_HEIGHT, SPAN};
use romcat_gui::{demo, headless};

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
fn 四个维度都有得选而且各自带着条数() {
    let app = 界面(ROWS);
    let facets = app.library().facets();
    // 平台、合集、语言、识别状态——票 25 点名的那四个。一个空了，界面上那一维就是死的。
    assert!(!facets.platforms.is_empty(), "平台那一维是空的");
    assert!(!facets.collections.is_empty(), "合集那一维是空的");
    assert!(!facets.languages.is_empty(), "语言那一维是空的");
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
fn 四个维度筛得动而且筛选下推到中立库() {
    let ctx = headless::context();
    let mut app = 界面(ROWS);
    跑(&ctx, &mut app, 2);
    let 全部 = app.window().total();
    assert_eq!(全部, ROWS);

    let (平台, 合集, 语言) = {
        let facets = app.library().facets();
        (
            facets.platforms[0].value.clone(),
            facets.collections[0].value.clone(),
            facets.languages[0].value.clone(),
        )
    };

    // 一维一维加上去，行数只降不升——**四条之间是「且」**：这是浏览，不是搜索。
    let mut 上一次 = 全部;
    for 加一维 in [
        Box::new(|q: &mut VariantQuery, v: &str| q.platform = Some(v.to_string()))
            as Box<dyn Fn(&mut VariantQuery, &str)>,
        Box::new(|q: &mut VariantQuery, v: &str| q.collection = Some(v.to_string())),
        Box::new(|q: &mut VariantQuery, v: &str| q.language = Some(v.to_string())),
    ]
    .into_iter()
    .zip([平台.as_str(), 合集.as_str(), 语言.as_str()])
    .map(|(f, v)| move |q: &mut VariantQuery| f(q, v))
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
    let 四维 = app.window().total();

    // **下推的证据一：** 界面上那个数与另一条独立查出来的数相等。
    let 独立数 = {
        let (library, site) = app.library_and_site();
        site.catalog
            .variant_total(library.query())
            .expect("数得出来")
    };
    assert_eq!(四维, 独立数);

    // **下推的证据二：** 筛过之后内存里还是那一扇窗，不是把结果读进来再过一遍。
    assert!(
        app.window().retained() as u64 <= SPAN,
        "内存里 {} 行，超过一扇窗（{SPAN} 行）",
        app.window().retained(),
    );

    // 全清之后回到全部。
    {
        let (library, _) = app.library_and_site();
        *library.query_mut() = VariantQuery::default();
    }
    跑(&ctx, &mut app, 1);
    assert_eq!(app.window().total(), 全部);
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
            app.set_scroll_to(Some(travel * step as f32 / STEPS as f32));
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

/// 眼下这个筛选下的头几行。
fn 头几行(app: &mut App, limit: u64) -> Vec<String> {
    let (library, site) = app.library_and_site();
    site.catalog
        .variant_page(library.query(), 0, limit)
        .expect("取得出一页")
        .into_iter()
        .map(|row| row.key)
        .collect()
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
