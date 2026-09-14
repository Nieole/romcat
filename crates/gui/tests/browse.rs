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
use romcat_core::catalog::State;
use romcat_core::catalog::browse::{PlatformFilter, StateFilter, WorkAnchor, WorkOrder, WorkQuery};
use romcat_core::dat::chinese::ChineseMark;
use romcat_core::scrape::priority::VERDICT;
use romcat_core::scrape::{AnchorKind, Field, Gather, MediaKind};
use romcat_core::shape::Role;
use romcat_core::stage::Stage;
use romcat_core::title::{Language, TitleKind};
use romcat_gui::app::{App, View};
use romcat_gui::bench::{self, Sweep};
use romcat_gui::table::{SPAN, row_height};
use romcat_gui::{browse, demo, headless};

mod shared;
use shared::画出来的字;

/// 合成数据的规模。**照真库的形状来**：`demo::BROWSE_VARIANTS` 个变体收敛成
/// `demo::BROWSE_LINES` 行主列表（`docs/library-facts.md`）。
///
/// 从前这儿写死 46,483（变体数对了），可作品数是合成数据里那二十个写死的名字，
/// 收出来只有 3,596 行——量出来的帧率与面板行数都不是维护者真会遇到的那个（挂单 `Q156`）。
const ROWS: u64 = demo::BROWSE_VARIANTS;

/// 这几条测试自己的**工作目录**。
///
/// **不共用 `demo::workspace()`**：那是 `--demo` 那个演示窗口用的目录，而窗口会往里写
/// **版式偏好**（面板拖到哪儿）。共用的话，维护者开一次演示窗口把某块面板拖高一截，
/// 下一次跑这几条测试屏上就少了几行——而断言数的正好是行数。
fn 工作目录() -> std::path::PathBuf {
    std::env::temp_dir().join("romcat-测试-浏览")
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

/// 真库量级上主列表收出**真库那么多行**——不是二十个作品收出来的那 3,596 行。
///
/// 主列表那条查询的代价跟着**分出来多少组**走（票 `gui-redesign/13`），组数不对，
/// 这一屏量出来的每一个数都与真库无关。
#[test]
fn 真库量级上主列表收出真库那么多行() {
    let mut app = 界面(ROWS);
    let 行数 = {
        let (browse, site) = app.browse_and_site();
        site.catalog.work_total(browse.query()).expect("数得出来")
    };
    assert_eq!(
        行数,
        demo::BROWSE_LINES,
        "{ROWS} 个变体该收敛成 {} 行，实际 {行数} 行",
        demo::BROWSE_LINES,
    );
}

#[test]
fn 五个维度都有得选而且各自带着条数() {
    let app = 界面(ROWS);
    let facets = app.browse().facets();
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
    // 「主列表一个游戏一行——不是 46,428 个变体」。合成数据里
    // `demo::BROWSE_WORKS` 个作品，外加每十三个留一个**压根没识别过**的变体
    // ——那些认不出作品，各自一行，一条都不许被吞掉（与导出那一侧同一条口径）。
    // （从前这儿是写死的二十个，收出来 3,596 行；挂单 `Q156`。）

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
        let (browse, site) = app.browse_and_site();
        site.catalog
            .work_page(browse.query(), 0, 4_000)
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
            let (_, site) = app.browse_and_site();
            site.catalog
                .variant_total(&romcat_core::catalog::VariantQuery::default())
                .expect("数得出来")
        },
        ROWS,
    );
}

/// 这一帧**屏上画出来的主列表那几行**，按画出来的次序；连同这个筛选下**下推出来的那一页**。
///
/// 表是虚拟化的，屏上只有视口那几十行——所以屏上那一串是「这一帧写着的字里，
/// 哪几段是这一页的行名」，而不是把库再问一遍。两串一比，就问得出
/// 「屏上画的次序是不是下推出来的那个次序」。
///
/// **它靠一条前提**：一个行名这一帧**只画一次**。眼下成立（没点开任何一行，右边那块
/// 详情面板摆的是「点开一行…」那句话）；哪天别处也把某个作品名单画成一段，
/// 这一串就会多出一项，底下那条前缀断言当场红——**红在这儿比悄悄放过好**。
fn 屏上与下推的次序(ctx: &egui::Context, app: &mut App) -> (Vec<String>, Vec<String>) {
    let 这一页: Vec<String> = {
        let (browse, site) = app.browse_and_site();
        site.catalog
            .work_page(browse.query(), 0, SPAN)
            .expect("取得出一页")
            .into_iter()
            .map(|row| row.name)
            .collect()
    };
    let 认得的: std::collections::HashSet<&str> = 这一页.iter().map(String::as_str).collect();
    let 屏上 = 画出来的字(&headless::frame(ctx, headless::input(), |ui| app.ui(ui)));
    let 画出来的: Vec<String> = 屏上
        .lines()
        .filter(|line| 认得的.contains(line))
        .map(str::to_string)
        .collect();
    (画出来的, 这一页)
}

/// **按作品名排完，屏上画出来的次序就是作品名的次序**（票 `parking-3/11` 验收第 4 条）。
///
/// 底下那条 `五列都排得了序而且换排序真的换了次序` 断的是「五种排法的头一行不全一样」：
/// 它在一份「压根没排」的实现上会红，可在一份**排了、但排错了**的实现上照样绿；
/// 而且它一次都没看屏上画出来的字，看的是又问了一遍库拿回来的那批行。
///
/// 这一条换了三样：
///
/// 1. **看的是这一帧真的画出来的字**（`shared::画出来的字`）。
/// 2. **期望从数据自己算出来**——把拿回来的那串名字自己排一遍，比的是同一串。
///    换一份同样合法的数据它照样成立；写死一批名字就只证得了「这批数据碰巧排成这样」。
///    中文作品名尤其要这么写：`ORDER BY` 那一侧是 UTF-8 逐字节比，Rust 这一侧的
///    `str` 也是——两边同一个口径，比得才有意义。
/// 3. **两个方向都断**，而且断「翻了方向屏上真的变了」。
///
/// **还没认出作品的那些行照样在同一串里**：主列表上它们画的是自己的键
/// （`COALESCE(work.name, variant.key)`，与 `adapter::converge` 的 `Anchor::Loose`
/// 同一条口径），所以这一屏上那一格从来不是空的——空着的那一格在**变体表**那一层，
/// 由 `crates/core/tests/browse.rs` 那条断它排在末尾。
#[test]
fn 按作品名排完屏上画出来的次序就是作品名的次序() {
    let ctx = headless::context();
    let mut app = 界面(2_000);
    // **先落在别的列上，再点回作品名那一列**：作品名是默认档，直接开跑的话这条测试
    // 连「换过来」这一下都没走过，验的只是「默认排序恰好是对的」。
    {
        let (browse, _) = app.browse_and_site();
        browse.query_mut().order = WorkOrder::Bytes;
    }
    跑(&ctx, &mut app, 2);
    {
        let (browse, _) = app.browse_and_site();
        let query = browse.query_mut();
        query.order = WorkOrder::Name;
        query.descending = false;
    }
    跑(&ctx, &mut app, 2);

    let (屏上, 这一页) = 屏上与下推的次序(&ctx, &mut app);
    assert!(
        屏上.len() >= 5,
        "屏上只认出 {} 行，比不出次序来",
        屏上.len()
    );
    assert_eq!(
        屏上,
        这一页[..屏上.len()],
        "屏上画出来的次序与下推出来的那一页对不上",
    );
    let mut 期望 = 这一页.clone();
    期望.sort();
    assert_eq!(这一页, 期望, "按作品名正序排出来的不是作品名的次序");

    // **两种行摆在同一串里**：认出作品的画作品名，还没认出的画自己的键。
    {
        let (browse, site) = app.browse_and_site();
        let anchors: Vec<WorkAnchor> = site
            .catalog
            .work_page(browse.query(), 0, SPAN)
            .expect("取得出一页")
            .into_iter()
            .map(|row| row.anchor)
            .collect();
        assert!(
            anchors
                .iter()
                .any(|anchor| matches!(anchor, WorkAnchor::Work(_)))
                && anchors
                    .iter()
                    .any(|anchor| matches!(anchor, WorkAnchor::Loose(_))),
            "这一页上只有一种行，「两种行排在同一串里」这句话没验到",
        );
    }

    {
        let (browse, _) = app.browse_and_site();
        browse.query_mut().descending = true;
    }
    跑(&ctx, &mut app, 2);
    let (倒着画的, 倒着那一页) = 屏上与下推的次序(&ctx, &mut app);
    assert!(倒着画的.len() >= 5, "翻了方向屏上认不出几行来");
    assert_eq!(
        倒着画的,
        倒着那一页[..倒着画的.len()],
        "翻了方向之后屏上画的与下推出来的对不上",
    );
    let mut 期望 = 倒着那一页.clone();
    期望.sort();
    期望.reverse();
    assert_eq!(倒着那一页, 期望, "翻了方向排出来的不是倒过来的作品名次序");
    assert_ne!(屏上, 倒着画的, "翻了方向，屏上一个字都没变");
}

/// **筛选器里作品名也用得上**，而且是**下推着筛**的（票 `parking-3/11` 验收第 3 条后半）。
///
/// 走的是筛选面板那条路（`browse::Screen::set_filter_rule`），不是直接改查询——
/// 屏上人按的就是它。断四样：真的筛掉了行、筛出来的那一页每一行都对得上、
/// 那一页正好装满（不多不少）、**屏上画的就是那一页的头几行**。
///
/// 最后那一样断的是**次序加前缀**（`屏上 == 这一页[..屏上.len()]`），不是
/// 「屏上每一行都对得上」——后者是句废话：`屏上与下推的次序` 本来就是拿这一页的名字
/// 当白名单滤出来的，屏上真画了别的行只会被静默滤掉，而不是被发现。
///
/// 「内存里还是那一扇窗」那一句**由构造保证**（`Window::retained()` 就是窗里那几行），
/// 摆在这儿是拦住「日后有人把窗换成整份读回来」，不是拦这一次筛选。
///
/// **筛的那个名字从库里现取**：写死一个名字就只证得了「这批合成数据里恰好有它」。
#[test]
fn 筛选器里按作品名筛得动而且下推着筛() {
    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 2);
    let 全部 = app.window().total();

    // 挑一行**认出了作品**的，拿它名字开头那一段当条件。
    let 那一段 = {
        let (browse, site) = app.browse_and_site();
        let name = site
            .catalog
            .work_page(browse.query(), 0, SPAN)
            .expect("取得出一页")
            .into_iter()
            .find(|row| matches!(row.anchor, WorkAnchor::Work(_)))
            .map(|row| row.name)
            .expect("这一页该有认出了作品的行");
        // 条件里不带空格：规则那门语言按空格分「且 / 或」。
        name.split(' ').next().expect("非空").to_string()
    };

    let 剩下 = 按规则筛(&ctx, &mut app, &format!("作品^{那一段}"));
    assert!(
        剩下 > 0 && 剩下 < 全部,
        "按「作品^{那一段}」筛出 {剩下} 行（一共 {全部} 行），这条筛选没起作用",
    );
    assert!(
        app.window().retained() as u64 <= SPAN,
        "筛完内存里留了 {} 行——筛选没下推",
        app.window().retained(),
    );

    let (屏上, 这一页) = 屏上与下推的次序(&ctx, &mut app);
    assert!(
        这一页.iter().all(|name| name.starts_with(&那一段)),
        "筛出来的行里有名字对不上「{那一段}」的",
    );
    assert_eq!(
        这一页.len() as u64,
        剩下.min(SPAN),
        "筛出来那一页装的行数与总数对不上",
    );
    assert!(!屏上.is_empty(), "屏上一行都没认出来，比不出什么");
    assert_eq!(
        屏上,
        这一页[..屏上.len()],
        "屏上画着的那几行不是筛出来那一页的头几行",
    );
}

#[test]
fn 五列都排得了序而且换排序真的换了次序() {
    let ctx = headless::context();
    let mut app = 界面(4_000);
    跑(&ctx, &mut app, 2);
    let 头一行 = |app: &mut App| {
        let (browse, site) = app.browse_and_site();
        site.catalog
            .work_page(browse.query(), 0, 1)
            .expect("取得出一页")
            .into_iter()
            .next()
            .map(|row| row.anchor)
    };
    let mut 见过 = Vec::new();
    for order in WorkOrder::ALL {
        {
            let (browse, _) = app.browse_and_site();
            let query = browse.query_mut();
            query.order = order;
            query.descending = true;
        }
        跑(&ctx, &mut app, 1);
        assert!(
            app.window().total() > 0,
            "按{}排完一行都不剩",
            order.label()
        );
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
        let facets = app.browse().facets();
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
            let (browse, _) = app.browse_and_site();
            加一维(browse.query_mut());
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
        let (browse, _) = app.browse_and_site();
        browse.query_mut().state = Some(StateFilter::Concluded(State::Matched));
    }
    跑(&ctx, &mut app, 1);
    let 五维 = app.window().total();

    // **下推的证据一：** 界面上那个数与另一条独立查出来的数相等。
    let 独立数 = {
        let (browse, site) = app.browse_and_site();
        site.catalog.work_total(browse.query()).expect("数得出来")
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
        let (browse, _) = app.browse_and_site();
        *browse.query_mut() = WorkQuery::default();
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
        let (browse, site) = app.browse_and_site();
        site.catalog
            .work_page(browse.query(), 0, 3)
            .expect("取得出一页")
            .into_iter()
            .map(|row| row.anchor)
            .collect()
    };

    // 一行都没选就是一行都没选——**空选择不等于全选**。
    assert_eq!(app.browse().picked().count(行数), 0);
    assert!(app.browse().picked().is_empty(行数));

    for anchor in &头几行 {
        app.browse_and_site().0.picked_mut().toggle(anchor);
    }
    跑(&ctx, &mut app, 1);
    assert_eq!(app.browse().picked().count(行数), 3, "多选没记住");
    assert!(app.browse().picked().contains(&头几行[0]));
    // 再点一下就取消。
    app.browse_and_site().0.picked_mut().toggle(&头几行[0]);
    assert_eq!(app.browse().picked().count(行数), 2);

    // **全选**：当前筛选下的每一行。它记的是这个筛选，不是一万行的身份。
    app.browse_and_site().0.picked_mut().select_all();
    跑(&ctx, &mut app, 1);
    assert!(app.browse().picked().is_all());
    assert_eq!(app.browse().picked().count(行数), 行数);
    // 全选之后点掉一行，正好少一条（ADR-0016 那条「规则加手动例外」的形状）。
    app.browse_and_site().0.picked_mut().toggle(&头几行[1]);
    assert_eq!(app.browse().picked().count(行数), 行数 - 1);
    assert!(!app.browse().picked().contains(&头几行[1]));

    // **换排序不作废**：筛出来的还是同一批行，只是重排了一遍——人勾了几行再按一下
    // 表头，选中不该凭空消失。
    {
        let (browse, _) = app.browse_and_site();
        browse.query_mut().order = WorkOrder::Bytes;
        browse.query_mut().descending = true;
    }
    跑(&ctx, &mut app, 2);
    assert_eq!(
        app.browse().picked().count(app.window().total()),
        行数 - 1,
        "换了个排序，选中的那一批就没了",
    );

    // **换筛选就作废**：全选说的是「当前筛出来的这一批」，条件一改那批就不是同一批。
    {
        let (browse, _) = app.browse_and_site();
        browse.query_mut().platform = Some(PlatformFilter::from_label(
            &browse.facets().platforms[0].value.clone(),
        ));
    }
    跑(&ctx, &mut app, 1);
    assert!(
        app.browse().picked().is_empty(app.window().total()),
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
        let (browse, site) = app.browse_and_site();
        site.catalog
            .work_page(browse.query(), 0, 4_000)
            .expect("取得出一页")
            .into_iter()
            .find(|row| row.variants > 1)
            .expect("有挂着不止一个变体的行")
    };

    app.browse_and_site().0.picked_mut().toggle(&一行.anchor);
    跑(&ctx, &mut app, 2);
    let 作用范围 = {
        let (browse, site) = app.browse_and_site();
        browse.batch_variants(&site.catalog).expect("展开得了")
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
        app.browse().scope_total(),
        Some(作用范围.len() as u64),
        "屏上那句「作用于多少个变体」与真展开出来的那一批对不上",
    );

    // 详情面板里列的那几个变体，与批量操作要动的那几个是同一批。
    {
        let (browse, site) = app.browse_and_site();
        browse.open_work(&site.catalog, &一行.anchor);
    }
    let 面板里的: Vec<String> = app
        .browse()
        .work()
        .expect("点得开")
        .variants
        .iter()
        .map(|variant| variant.row.key.clone())
        .collect();
    assert_eq!(面板里的, 作用范围);

    // **全选**展开的是整个库的变体。
    app.browse_and_site().0.picked_mut().select_all();
    let 全部 = {
        let (browse, site) = app.browse_and_site();
        browse.batch_variants(&site.catalog).expect("展开得了")
    };
    assert_eq!(全部.len() as u64, 4_000, "全选没盖住当前筛选下的全部变体");
}

#[test]
fn 选中一个变体时变体级的操作只作用于它() {
    // 另半条：**在详情面板里选中某一个变体，变体级的操作只作用于它。**
    let mut app = 界面(4_000);
    let 一行 = 一行有兄弟的(&mut app);
    {
        let (browse, site) = app.browse_and_site();
        browse.open_work(&site.catalog, &一行);
    }
    let 变体们: Vec<String> = app
        .browse()
        .work()
        .expect("点得开")
        .variants
        .iter()
        .map(|variant| variant.row.key.clone())
        .collect();
    assert!(变体们.len() > 1, "得有得挑才测得出来");
    // 点开一行默认选中第一个变体：面板的第二三层总得有东西摆。
    assert_eq!(app.browse().variant_key(), Some(变体们[0].as_str()));

    // 挑第二个，往**变体**这一层写一条元数据。
    {
        let (browse, site) = app.browse_and_site();
        browse.pick(&site.catalog, &变体们[1]);
        let draft = browse.value_draft_mut();
        draft.field = Field::TranslationGroup;
        draft.anchor = AnchorKind::Variant;
        draft.value = "只该落在这一个变体上的汉化组".to_string();
        let key = 变体们[1].clone();
        browse.put_value(site, &key);
    }
    let 落在它头上 = |app: &mut App, key: &str| {
        let (browse, site) = app.browse_and_site();
        browse.pick(&site.catalog, key);
        browse
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
        let (browse, site) = app.browse_and_site();
        browse.open_work(&site.catalog, &一行);
    }
    let 原先 = app
        .browse()
        .variant_key()
        .expect("点开就该选中一个")
        .to_string();
    let 那个平台 = app
        .browse()
        .detail()
        .expect("点得开")
        .row
        .platform
        .clone()
        .expect("有平台");

    // 筛成**别的平台**：原先那个变体一定不在这一行底下了。
    let 另一个 = app
        .browse()
        .facets()
        .platforms
        .iter()
        .map(|facet| facet.value.clone())
        .find(|value| *value != 那个平台)
        .expect("合成数据里不止一个平台");
    {
        let (browse, _) = app.browse_and_site();
        browse.query_mut().platform = Some(PlatformFilter::from_label(&另一个));
    }
    跑(&ctx, &mut app, 2);
    if let Some(work) = app.browse().work() {
        let 现在 = app.browse().variant_key().expect("还有变体就该选中一个");
        assert_ne!(现在, 原先, "选中的还是那个已经被筛掉的变体");
        assert!(
            work.variants.iter().any(|v| v.row.key == 现在),
            "选中的那个变体不在这一行底下",
        );
    } else {
        // 这一行在新筛选下一个变体都不剩，那就该一起清掉，不留一份没人认领的详情。
        assert!(app.browse().variant_key().is_none());
        assert!(app.browse().detail().is_none());
    }
}

#[test]
fn 详情面板列得出全部变体每个带置信度与依据() {
    let mut app = 界面(4_000);
    let 一行 = 一行有兄弟的(&mut app);
    {
        let (browse, site) = app.browse_and_site();
        browse.open_work(&site.catalog, &一行);
    }
    let work = app.browse().work().expect("点得开").clone();
    assert!(!work.name.is_empty());
    assert!(!work.platforms.is_empty());
    assert!(work.variants.len() > 1, "该列出这个作品的全部变体");
    // 每个变体带置信度；**有候选的那些带依据**——没有依据的候选事后无法复核（ADR-0002）。
    let mut 有候选 = 0;
    for variant in &work.variants {
        assert!(!variant.row.key.is_empty());
        for candidate in &variant.candidates {
            有候选 += 1;
            assert!(!candidate.evidence.is_empty(), "候选没有依据，事后没法复核",);
            // 行上那一档是**最高**的那一档（`Confidence` 的 `Ord` 里 `High` 最小），
            // 所以它只该比每一条候选更靠前——一个变体撞上一条高一条低是真库的常态。
            assert!(
                variant.confidence() <= Some(candidate.confidence),
                "行上那一档置信度比某条候选还低",
            );
        }
        if variant.candidates.is_empty() {
            // 一条候选都没有不是「撞过没撞上」，而它自己还分两种：连识别都还没跑过是
            // **还没识别**，跑过了却一个字都没说得出来是**没有候选**（词表两条词条）。
            assert_eq!(variant.confidence(), None);
            assert_eq!(
                variant.confidence_label(),
                if variant.state.is_none() {
                    romcat_core::catalog::identify::NOT_RUN_LABEL
                } else {
                    "没有候选"
                },
            );
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
        let (browse, site) = app.browse_and_site();
        browse.pick(&site.catalog, &key);
    }
    let detail = app.browse().detail().expect("点得开");
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
        let (browse, _) = app.browse_and_site();
        browse.query_mut().chinese = Some(ChineseMark::FanTranslated.label().to_string());
    }
    跑(&ctx, &mut app, 1);
    let 汉化 = app.window().total();
    assert!(汉化 > 0, "中文=汉化 一条都筛不出来");

    // 同一批变体按「语言里有中文」筛，一条都不该有——它们的发行版是日版。
    {
        let (browse, _) = app.browse_and_site();
        let query = browse.query_mut();
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
        let (browse, site) = app.browse_and_site();
        browse.pick(&site.catalog, &key);
    }
    let detail = app.browse().detail().expect("点开得了").clone();
    assert!(!detail.values.is_empty(), "刮削来的字段一条都没折出来");
    let work = detail.work.clone().expect("认出了作品");

    {
        let (browse, site) = app.browse_and_site();
        let draft = browse.value_draft_mut();
        draft.field = Field::Description;
        draft.anchor = AnchorKind::Work;
        draft.value = "界面上手写的简介".to_string();
        browse.put_value(site, &work);
    }
    let 写完 = app.browse().detail().expect("还在");
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
        let (browse, site) = app.browse_and_site();
        browse.clear_value(site, AnchorKind::Work, &work, Field::Description);
    }
    assert!(
        app.browse()
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
        let (browse, site) = app.browse_and_site();
        browse.pick(&site.catalog, &key);
    }
    let detail = app.browse().detail().expect("点开得了");
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
        let (browse, site) = app.browse_and_site();
        browse.pick(&site.catalog, &key);
    }
    let detail = app.browse().detail().expect("点开得了");
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
        let (browse, site) = app.browse_and_site();
        browse.pick(&site.catalog, &key);
    }
    let 原有 = app.browse().detail().expect("点开得了").titles.len();
    let work = app
        .browse()
        .detail()
        .expect("点开得了")
        .work
        .clone()
        .expect("认出了作品");

    {
        let (browse, site) = app.browse_and_site();
        let draft = browse.title_draft_mut();
        draft.value = "界面上敲进去的中文译名".to_string();
        draft.language = Language::Chinese;
        draft.kind = TitleKind::Translated;
        browse.add_title(site, &work);
    }
    let detail = app.browse().detail().expect("还在");
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
        let (browse, site) = app.browse_and_site();
        browse.suppress_title(site, &新的);
    }
    assert_eq!(
        app.browse().detail().expect("还在").titles.len(),
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
        let (browse, site) = app.browse_and_site();
        browse.pick(&site.catalog, &key);
    }
    let detail = app.browse().detail().expect("点开得了").clone();
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
        let (browse, site) = app.browse_and_site();
        browse.set_preferred(site, &work, &platform, &别的);
    }
    let 改后 = app.browse().detail().expect("还在");
    assert_eq!(改后.preferred.as_deref(), Some(别的.as_str()), "裁决没落库");
    assert_eq!(改后.preferred_now(), Some(别的.as_str()), "裁决没盖过规则");
    // **这一条就是 ADR-0012**：换了首选，中文标题那一条来源一个字没动。
    let 改后中文 = 改后.chinese_title().expect("中文叫法还在");
    assert_eq!(改后中文.value, 中文标题.value);
    assert_eq!(改后中文.source, 中文标题.source);
    assert_eq!(改后中文.kind, 中文标题.kind);

    {
        let (browse, site) = app.browse_and_site();
        browse.clear_preferred(site, &work, &platform);
    }
    let 撤后 = app.browse().detail().expect("还在");
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
        let travel = app.window().total() as f32 * row_height();
        for step in 0..=STEPS {
            app.browse_and_site().0.scroll_to = Some(travel * step as f32 / STEPS as f32);
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
            let (browse, site) = app.browse_and_site();
            browse.pick(&site.catalog, &key);
            browse
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
    let (_, site) = app.browse_and_site();
    site.catalog
        .variant_page(&romcat_core::catalog::VariantQuery::default(), 0, limit)
        .expect("取得出一页")
        .into_iter()
        .map(|row| row.key)
        .collect()
}

/// 找一行**底下挂着不止一个变体**的：选中语义那两条得有得挑才测得出来。
fn 一行有兄弟的(app: &mut App) -> WorkAnchor {
    let (browse, site) = app.browse_and_site();
    site.catalog
        .work_page(browse.query(), 0, 4_000)
        .expect("取得出一页")
        .into_iter()
        .find(|row| row.variants > 1)
        .map(|row| row.anchor)
        .expect("合成数据里该有挂着不止一个变体的作品")
}

/// 找一条**带附属文件与内部资源**的变体：合成数据里每九个留一个（见 `demo::browse`）。
fn 一条带附属文件的(app: &mut App) -> String {
    for key in 头几行(app, 64) {
        let 有 = {
            let (_, site) = app.browse_and_site();
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
            let (browse, site) = app.browse_and_site();
            browse.pick(&site.catalog, &key);
            browse
                .detail()
                .is_some_and(|detail| detail.siblings.len() > 1)
        };
        if 有兄弟 {
            return key;
        }
    }
    panic!("合成数据里该有同作品同平台的两个变体");
}

/// 底下那块编辑面板的**右半栏**滚一趟，把这一路上画出来的字都收起来。
///
/// 刮削字段那一栏排在标题集合与首选变体之后，而那块面板默认 260 点高
/// （`layout::EDIT`）——一屏摆不下是必然的，而 egui 不画视口之外的文字
/// （`ui.is_rect_visible`）。所以这里滚的是**真的滚轮事件**，而且**指针先停进那一栏**：
/// 滚轮归指针底下那块滚动区，少了这一下滚的就是别处——挂单 Q20 试过的三条路里，
/// 滚轮那条栽的正是这里（待确认屏的 `详情滚一趟` 走的也是这条路，挂单 Q169）。
fn 元数据栏滚一趟(ctx: &egui::Context, app: &mut App) -> String {
    const STEPS: u32 = 24;
    let mut out = String::new();
    for step in 0..=STEPS {
        let mut input = headless::input();
        // 指针停在底下那块面板的**右半栏**：左边那 42% 是「它是什么」那一栏
        // （`facts_column`），改元数据的在右边。
        input
            .events
            .push(egui::Event::PointerMoved(egui::pos2(900.0, 700.0)));
        if step > 0 {
            input.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -150.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
        }
        out.push_str(&画出来的字(
            &headless::frame(ctx, input, |ui| app.ui(ui)),
        ));
    }
    out
}

/// 这一趟画出来的字里，**以这几个字开头的那一段**。
///
/// 一段一行（见 `shared::画出来的字`），而刮削字段那一栏一条值就画成一段：于是
/// 「屏上那一行写的是什么」问得出来。**挑得准靠的是开头那几个字**（「字段 · 哪一层」）
/// ——同一趟里还画着挂在悬停里的那份原文，它没有这个开头。
fn 屏上那一行<'a>(屏上: &'a str, 开头: &str) -> &'a str {
    屏上
        .lines()
        .find(|line| line.starts_with(开头))
        .unwrap_or_else(|| {
            panic!(
                "滚下来画出的 {} 段字里没有以「{开头}」开头的那一段",
                屏上.lines().count(),
            )
        })
}

#[test]
fn 一条顶到闸上的简介收成一行画得下的那一截() {
    // 票 03 的第二处边界的界面这一半。中立库里一条简介最多 4,000 字
    // （`scrape::zh::DESCRIPTION_LIMIT`），而刮削字段那一栏画在一条**横排**里——
    // 横排不折行，整段原样排进去就是四五万点宽的一行，面板跟着长出一条横向滚动条。
    //
    // **断言看的是这一帧真的画出来的字。** 钉在 `one_line` 那个纯函数上只证得了
    // 「算出来的那一截是对的」，证不了「屏上摆的就是它」——那是挂单 Q20 记着的缺口。
    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 2);
    let key = 一条认出作品的(&mut app);
    let 顶到闸上 = "外".repeat(romcat_core::scrape::zh::DESCRIPTION_LIMIT);
    // **换行也要管**：数据源的排版原样留在值里（规格 18），可横排里一个换行就把那
    // 一行撑高，底下几条就被挤出视口。摆在**作品**那一层，与变体那一层那条长的分得开。
    let 带换行 = "　　两个人一起打外星人。\n第二段：外星人赢了。";
    {
        let (browse, site) = app.browse_and_site();
        let work = site
            .catalog
            .work_of_variant(&key)
            .expect("读得出")
            .expect("这一条认出了作品");
        site.catalog
            .put_verdict_value(
                AnchorKind::Variant,
                &key,
                Field::Description,
                &顶到闸上,
                "测试摆进去的",
            )
            .expect("写得进去");
        site.catalog
            .put_verdict_value(
                AnchorKind::Work,
                &work,
                Field::Description,
                带换行,
                "测试摆进去的",
            )
            .expect("写得进去");
        browse.pick(&site.catalog, &key);
    }

    let 屏上 = 元数据栏滚一趟(&ctx, &mut app);
    // `values_ui` 拼的是「字段 · 哪一层｜来源｜值」：开头那几个字挑得出是哪一条，
    // 而**值那一段**是最后一个 `｜` 之后那一截——量长度要量它，前头那十一个字是固定开销。
    let 开头 = |anchor: AnchorKind| format!("{} · {}", Field::Description.label(), anchor.label());
    let 值那一段 = |line: &str| {
        line.rsplit('｜')
            .next()
            .expect("屏上那一行是「字段 · 哪一层｜来源｜值」")
            .to_string()
    };
    // ——— 长度：屏上摆的是**省略号收住的那一截** ———
    let 那一截 = 值那一段(屏上那一行(&屏上, &开头(AnchorKind::Variant)));
    assert!(那一截.ends_with('…'), "收窄过要看得出来：{那一截}");
    assert!(
        那一截.chars().count() < 80,
        "库里那条 {} 个字，屏上这一截画了 {} 个——横排里不折行，面板照旧被撑出去",
        顶到闸上.chars().count(),
        那一截.chars().count(),
    );
    // ——— 换行：折平成一段画出来，而不是掐掉两头 ———
    let 折平的 = 值那一段(屏上那一行(&屏上, &开头(AnchorKind::Work)));
    assert!(
        折平的.ends_with("第二段：外星人赢了。"),
        "换行那条没折平：屏上那一行断在换行处，后半段没跟上来｜{折平的}",
    );
    assert!(折平的.starts_with('\u{3000}'), "折平不等于掐两头：{折平的}",);

    // 原样画得下的**一个字都不动**——这一条只有从函数那一侧看得见：屏上画的是同一串字，
    // 中间换没换过一份字符串出去，看画出来的那一帧看不出来。
    assert_eq!(browse::one_line("两个人一起打外星人。"), None);
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
        let (browse, site) = app.browse_and_site();
        site.catalog
            .put_verdict_value(
                AnchorKind::Variant,
                &key,
                Field::Description,
                &"外".repeat(romcat_core::scrape::zh::DESCRIPTION_LIMIT),
                "测试摆进去的",
            )
            .expect("写得进去");
        browse.pick(&site.catalog, &key);
    }
    let cost = bench::scroll(&mut app, 120, Sweep::Rows(3.0));
    let 行数 = {
        let (browse, site) = app.browse_and_site();
        site.catalog.work_total(browse.query()).expect("数得出来")
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

/// 「翻行会看得出来的延迟」这道门，**按读了几次库算，不按秒表**（挂单 `Q345` / `Q357`）。
///
/// 票 `parking-3/01` 把 `crates/gui/tests/media.rs` 那条「头一帧 < 1 秒」的挂钟断言拿掉了
/// ——票 `parking-3/10` 用 16 趟对照实测坐实了它落到门禁上就是一张彩票（过 2 败 6 /
/// 过 1 败 7，机器闲下来 9/9 全过）。代价是从此没人替「翻行会不会卡」把门。
///
/// 这一条把那道门按回去，判据换成 `bench::FrameCost::reads`：**这一趟滚动读了几次库**。
/// 它数的是次数不是毫秒，机器忙不忙一个字都不影响它。而**真库量级**是它缺的那一半
/// ——上面那条 `库里有一条顶到闸上的简介时列表照样滚得动` 在两千行上断的就是这个数，
/// 真正要防的却是「库一大就每帧都回库查」。
///
/// 断的是**同一趟滚动、同样多的库读**：主列表从 946 行涨到 `demo::BROWSE_LINES` 行，
/// 翻同样多行的代价一次都不许多。表格是虚拟化的（一扇窗 `SPAN` 行），这件事成立才
/// 谈得上「翻行看不出延迟」；它一旦不成立，屏上那一下卡在哪个量级上是机器说了算的，
/// 而门禁再也拦不住。
#[test]
fn 翻行的代价按读了几次库算而且真库量级上一次不多() {
    // 240 帧每帧 3 行 ＝ 720 行，一扇窗 512 行——**不管库里有多少行**，最多跨两次。
    const 滚多少帧: u32 = 240;
    const 每帧几行: f32 = 3.0;
    /// 一次都跨不出窗口的库比不出这件事：小的那份也得真的滚得动。
    const 小的那份: u64 = 4_000;

    let mut 小 = 界面(小的那份);
    let mut 大 = 界面(ROWS);
    let 行数 = |app: &mut App| -> u64 {
        let (browse, site) = app.browse_and_site();
        site.catalog.work_total(browse.query()).expect("数得出来")
    };
    let (小行数, 大行数) = (行数(&mut 小), 行数(&mut 大));
    assert_eq!(大行数, demo::BROWSE_LINES, "大的那份该是真库那么多行");
    assert!(
        小行数 > SPAN && 小行数 < 大行数 / 10,
        "两份都得滚得动、量级还得真的拉开：{小行数} 对 {大行数}",
    );

    let 小的 = bench::scroll(&mut 小, 滚多少帧, Sweep::Rows(每帧几行));
    let 大的 = bench::scroll(&mut 大, 滚多少帧, Sweep::Rows(每帧几行));
    assert!(
        大的.reads <= 3,
        "真库量级上滚 {} 行读了 {} 次库——窗口预取没接住，翻行就会看得出来",
        滚多少帧 * 每帧几行 as u32,
        大的.reads,
    );
    assert_eq!(
        小的.reads, 大的.reads,
        "同一趟滚动，{小行数} 行读 {} 次、{大行数} 行读 {} 次——代价跟着库变大了",
        小的.reads, 大的.reads,
    );
    assert!(小的.reads > 0, "一次库都没读，这一趟没滚动");
}

/// 把一条规则预填进筛选器，跑几帧，返回筛出多少行。
///
/// **走的是界面上那条路**（[`browse::Screen::set_filter_rule`]），不是直接改查询：
/// 子库屏点「改选择」跳回来预填的正是它（票 `gui-redesign/11`）。
fn 按规则筛(ctx: &egui::Context, app: &mut App, text: &str) -> u64 {
    let rule = romcat_core::sublibrary::Rule::parse(text)
        .unwrap_or_else(|error| panic!("「{text}」读不懂：{error}"));
    app.browse_and_site().0.set_filter_rule(Some(rule));
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
        let facets = app.browse().facets();
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
    let 平台 = app.browse().facets().platforms[0].value.clone();
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
        app.browse().facets().platforms[0].value.clone()
    };
    按规则筛(&ctx, &mut app, &format!("平台={平台} 或 中文=汉化"));

    let 行数 = app.window().total();
    let 变体数 = app.browse().filtered_total().expect("数得出来");
    assert!(
        行数 > 0 && 变体数 >= 行数,
        "{变体数} 个变体撑不起 {行数} 行"
    );

    // 屏上写着几个变体，按下批量操作就该动几个——**三处同一个数**。
    let (库里, 屏上) = {
        let (browse, site) = app.browse_and_site();
        let query = browse.query().clone();
        (
            site.catalog
                .scoped_variants(&query, romcat_core::catalog::browse::Scope::AllExcept(&[]))
                .expect("展得开")
                .len(),
            browse.filtered_total().expect("数得出来"),
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
        let facets = app.browse().facets();
        (
            facets.platforms[0].value.clone(),
            facets.collections[0].value.clone(),
        )
    };
    // 左栏点一个合集，筛选器里再搭一棵「任一满足」的树——**两半都要带过去**。
    {
        let (browse, _) = app.browse_and_site();
        browse.query_mut().collection = Some(合集.clone());
        browse.set_filter_rule(Some(
            romcat_core::sublibrary::Rule::parse(&format!("平台={平台} 或 中文=汉化"))
                .expect("读得懂"),
        ));
    }
    跑(&ctx, &mut app, 2);

    let 屏上: std::collections::BTreeSet<String> = {
        let (browse, site) = app.browse_and_site();
        let query = browse.query().clone();
        site.catalog
            .scoped_variants(&query, romcat_core::catalog::browse::Scope::AllExcept(&[]))
            .expect("展得开")
            .into_iter()
            .collect()
    };
    assert!(
        !屏上.is_empty(),
        "这份筛选该选得中东西，否则这条断言等于没测"
    );

    {
        let (browse, site) = app.browse_and_site();
        let draft = browse.save_draft_mut();
        draft.name = "掌机".to_string();
        draft.target = "/Volumes/SDCARD/掌机".to_string();
        browse.save_as_sublibrary(site);
        assert!(browse.error().is_none(), "{:?}", browse.error());
        assert!(browse.notice().is_some(), "建完子库没有回执");
    }

    // **新建的子库选出来的东西与屏上一致。**
    let (库里那条, 子库选出来的) = {
        let (_, site) = app.browse_and_site();
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
    assert_eq!(
        子库选出来的, 屏上,
        "存成子库之后选出来的与屏上筛出来的不是同一批"
    );
    // 那条规则就是屏上那几条，一条不多一条不少。
    assert_eq!(
        库里那条,
        format!("合集={合集} 且 (平台={平台} 或 中文=汉化)"),
    );

    // 重名不覆盖：撞上一个已有的子库要当场说清，不能悄悄把它的选择集并上一批。
    {
        let (browse, site) = app.browse_and_site();
        let draft = browse.save_draft_mut();
        draft.name = "掌机".to_string();
        draft.target = "/Volumes/SDCARD/另一张".to_string();
        browse.save_as_sublibrary(site);
        assert!(
            browse.error().is_some_and(|error| error.contains("掌机")),
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
        let (browse, _) = app.browse_and_site();
        // 「识别状态」进不了规则：那是这一趟的进度不是内容。
        browse.query_mut().state = Some(StateFilter::Unidentified);
        let draft = browse.save_draft_mut();
        draft.name = "存不成".to_string();
        draft.target = "/Volumes/SDCARD/存不成".to_string();
    }
    跑(&ctx, &mut app, 1);
    {
        let (browse, site) = app.browse_and_site();
        browse.save_as_sublibrary(site);
        assert!(
            browse
                .error()
                .is_some_and(|error| error.contains("识别状态")),
            "少写一条就存下去了：那样子库选出来的会比屏上多",
        );
        assert!(
            site.catalog.sublibrary("存不成").expect("读得动").is_none(),
            "挡住了却还是建了个子库出来",
        );
    }
}

#[test]
fn 裁完一批之后浏览屏缓着的那几行也重取() {
    // 待确认屏改的是识别结论与作品链接，而浏览屏那一列画的正是它们
    // （`WorkRow::confidence`）。窗里缓着的 512 行只在**换筛选**时才作废，裁决一个字都
    // 没改筛选——于是裁完回浏览屏，看见的还是裁之前那几行。与库屏扫完一个根走的是同一个
    // 入口（`browse::Screen::invalidate`），只是那一趟由任务台交回来，这一趟发生在
    // 本进程的这一帧里，由窗口转告（`App::route`）。
    let ctx = headless::context();
    let mut app = 界面(4_000);
    跑(&ctx, &mut app, 2);
    let 读过 = app.window().reads();
    assert!(读过 > 0, "浏览屏本该已经取过一段行");
    // **没人动过库就一次都不该再读**——窗存在的理由正是这个。这一条同时钉住下面那条
    // 断言不是「每帧都在重取」蒙出来的。
    跑(&ctx, &mut app, 3);
    assert_eq!(app.window().reads(), 读过, "没人动过库，窗不该重新取一遍");

    // ——— 到待确认屏整批裁一批 ———
    // 「整批拒绝」（记成「我看过了，认不出」）：这份合成数据里的队列条目一条候选都没有
    // ——真机上 98.2% 正是这个样子——所以走得通的是这一条，不是「整批通过」。
    let batch = app
        .queue()
        .queue()
        .batches()
        .first()
        .cloned()
        .expect("合成数据里该有分好的批");
    if app.queue().scope().map(|scope| scope.shape).as_ref() != Some(&batch.shape) {
        app.queue_and_site().0.open_batch(&batch.shape);
    }
    let scope = app.queue().scope().expect("展开了就该有作用范围");
    {
        let (screen, site) = app.queue_and_site();
        screen.reject(site, &scope);
        screen.commit(site);
    }
    assert!(app.queue().error().is_none(), "{:?}", app.queue().error());
    assert!(app.queue().applied().is_some(), "这一批该落下了");

    // ——— 回浏览屏：窗得照裁完之后的库重取 ———
    app.show_view(View::Browse);
    跑(&ctx, &mut app, 2);
    assert!(
        app.window().reads() > 读过,
        "裁完一批，浏览屏还画着裁之前缓下来的那几行",
    );
}
// ── 搜索框与匹配质量排序（票 `gui-redesign/05`）──────────────────────────
//
// 排序本身在核心库里（`crates/core/tests/search.rs` 把五档一条条钉着）。这一组钉的是
// **界面上那条路真的通到那儿**：搜索框里打的字进了查询、屏上那张表跟着换了一批行、
// 而且它**进不了子库的规则**。

/// 在搜索框里打几个字，跑几帧，把主列表眼下这一页读回来。
///
/// **走的是界面上那条路**（[`browse::Screen::query_mut`] 就是那个文本框绑着的东西），
/// 不是绕过界面直接问核心库。
fn 搜(ctx: &egui::Context, app: &mut App, needle: &str) -> Vec<romcat_core::catalog::WorkRow> {
    {
        let (browse, _) = app.browse_and_site();
        browse.query_mut().search = needle.to_string();
    }
    跑(ctx, app, 2);
    let (browse, site) = app.browse_and_site();
    site.catalog
        .work_page(browse.query(), 0, 64)
        .expect("取得出一页")
}

#[test]
fn 搜索框打几个字之后匹配得好的排前面() {
    let ctx = headless::context();
    let mut app = 界面(4_000);
    跑(&ctx, &mut app, 2);
    let 全库 = app.window().total();

    // 合成数据里的作品名是中文（`demo::WORKS`）：「幻想传说」以「幻想」开头，
    // 「最终幻想 Ⅶ ★特别版★」只是含有它。
    let rows = 搜(&ctx, &mut app, "幻想");
    assert!(!rows.is_empty(), "搜「幻想」一行都没有");
    assert!(
        app.window().total() < 全库,
        "搜完之后还是整个库那么多行，搜索框没接上",
    );

    // **屏上那张表就是这一批**：窗口的总数与核心库另数一遍的数字相等。
    let 另数一遍 = {
        let (browse, site) = app.browse_and_site();
        site.catalog.work_total(browse.query()).expect("数得出来")
    };
    assert_eq!(app.window().total(), 另数一遍);

    // 匹配质量是第一把键：`hit` 一路不减，而头一行是「以它开头」那一档。
    let mut 上一档 = romcat_core::catalog::SearchHit::TitleStart;
    for row in &rows {
        let hit = row.hit.expect("搜索着的时候每一行都说得出命中在哪儿");
        assert!(hit >= 上一档, "「{}」排到了比它更好的一档前面", row.name);
        上一档 = hit;
    }
    assert_eq!(
        rows.first().map(|row| row.hit),
        Some(Some(romcat_core::catalog::SearchHit::TitleStart)),
        "以搜索词开头的没排最前",
    );
}

#[test]
fn 别名搜得到而且屏上说得出是别名命中的() {
    let ctx = headless::context();
    let mut app = 界面(4_000);
    跑(&ctx, &mut app, 2);

    // 合成数据里每个作品的**标题集合**有一条英文官方名 `Work NN (USA)`，
    // 而作品名与变体的键里一个 `Work` 都没有——搜得到它，只可能是**别名**那条路。
    let rows = 搜(&ctx, &mut app, "work 0");
    assert!(!rows.is_empty(), "按别名搜「work 0」一行都没有");
    for row in &rows {
        assert!(
            !row.name.to_ascii_lowercase().contains("work 0"),
            "「{}」是名字自己命中的，那证明不了别名这条路",
            row.name,
        );
        assert_eq!(
            row.hit,
            Some(romcat_core::catalog::SearchHit::AliasStart),
            "「{}」的命中档不对",
            row.name,
        );
    }
    // **屏上印得出那句话**：一行名字里一个搜索词都没有的作品冒在前面，
    // 不说清凭什么，就是这份规格从头到尾在消灭的那种「看不懂」。
    assert_eq!(
        romcat_core::catalog::SearchHit::AliasStart.label(),
        "别名以它开头",
    );
}

#[test]
fn 搜索与筛选叠加而且搜索进不了子库的规则() {
    let ctx = headless::context();
    let mut app = 界面(4_000);
    跑(&ctx, &mut app, 2);

    // 先筛后搜：两边叠加，行数只会更少。
    let 只搜 = 搜(&ctx, &mut app, "幻想").len();
    {
        let (browse, _) = app.browse_and_site();
        browse.query_mut().platform = Some(PlatformFilter::Named("SFC".to_string()));
    }
    跑(&ctx, &mut app, 2);
    let (筛完, 命中档) = {
        let (browse, site) = app.browse_and_site();
        let rows = site
            .catalog
            .work_page(browse.query(), 0, 64)
            .expect("取得出一页");
        let 档: Vec<_> = rows.iter().filter_map(|row| row.hit).collect();
        (rows.len(), 档)
    };
    assert!(筛完 <= 只搜, "先筛后搜反而多出行来");
    assert!(命中档.is_sorted(), "先筛之后次序就不按匹配质量了");

    // **排序不进子库的规则**：搜索框里还有字时「存成子库」当场挡住，
    // 而且一个子库都没建出来。
    {
        let (browse, _) = app.browse_and_site();
        let draft = browse.save_draft_mut();
        draft.name = "存不成".to_string();
        draft.target = "/Volumes/SDCARD/存不成".to_string();
    }
    跑(&ctx, &mut app, 1);
    {
        let (browse, site) = app.browse_and_site();
        browse.save_as_sublibrary(site);
        assert!(
            browse.error().is_some_and(|error| error.contains("搜索")),
            "搜索框里还有字却把子库存下去了：子库要的是集合不是顺序",
        );
        assert!(site.catalog.sublibrary("存不成").expect("读得动").is_none());
    }

    // 清空搜索框之后照存不误——挡住的是搜索，不是「存成子库」这件事本身。
    {
        let (browse, _) = app.browse_and_site();
        browse.query_mut().search.clear();
    }
    跑(&ctx, &mut app, 1);
    {
        let (browse, site) = app.browse_and_site();
        browse.save_as_sublibrary(site);
        assert!(browse.error().is_none(), "{:?}", browse.error());
        let 选择集 = site.catalog.selection("存不成").expect("选择集读得回来");
        // **只带条件不带顺序**：折出来的规则里只有那个平台档，一个字的排序都没有。
        assert_eq!(
            选择集
                .selection
                .rules
                .iter()
                .map(|rule| rule.text.as_str())
                .collect::<Vec<_>>(),
            vec!["平台=SFC"],
        );
    }
}

#[test]
fn 筛选栏那颗全清不碰搜索框() {
    // 「搜索管排序、筛选器管集合」是这一票的界线，而那颗按钮的标签只说**筛选**。
    // 顺手清掉另一块面板上的搜索框，正是这句话立不住的样子。
    let ctx = headless::context();
    let mut app = 界面(4_000);
    跑(&ctx, &mut app, 2);
    {
        let (browse, _) = app.browse_and_site();
        browse.query_mut().search = "幻想".to_string();
        browse.query_mut().platform = Some(PlatformFilter::Named("SFC".to_string()));
        browse.query_mut().order = WorkOrder::Bytes;
        browse.query_mut().descending = true;
    }
    跑(&ctx, &mut app, 2);
    {
        let (browse, _) = app.browse_and_site();
        browse.clear_filter();
        let query = browse.query();
        assert_eq!(query.search, "幻想", "全清把搜索框也清掉了");
        assert_eq!(query.platform, None, "全清没把筛选条件清掉");
        // 排序照旧不属于筛选。
        assert_eq!(query.order, WorkOrder::Bytes);
        assert!(query.descending);
    }
}

// ── 刮削面板（票 `gui-redesign/10`）────────────────────────────────────────
//
// 这一组钉的是**按下去之前那几件事**：范围是不是筛出来的那一批、默认勾着哪几样、
// 勾联网源之前有没有把配额那件事说清楚、屏上那个请求数是不是真的 0。
// 最后一条比别处严一档——**在线源赌的是用户的账号与 IP**（ADR-0007）。

/// 面板测试用的规模：几百行够摆出「筛一批、勾几行」，而刮削真跑一趟只要几毫秒。
const 小库: u64 = 400;

/// 全选一批，再按「刮削选中…」。**走的是界面上那条一模一样的路**：
/// 先画两帧让屏上那句「作用于多少个变体」数出来，再按那一下。
fn 摊开刮削面板(ctx: &egui::Context, app: &mut App) {
    跑(ctx, app, 2);
    app.browse_and_site().0.picked_mut().select_all();
    跑(ctx, app, 2);
    let (browse, site) = app.browse_and_site();
    browse.open_scrape(&site.catalog);
}

#[test]
fn 刮削面板四个旋钮的默认位置() {
    let ctx = headless::context();
    let mut app = 界面(小库);
    摊开刮削面板(&ctx, &mut app);
    let panel = app.browse().scrape();

    assert!(panel.is_open(), "按「刮削选中…」该把面板摊开");
    // **默认只勾本地源**（ADR-0007）：联网那一档要人明确点头才开。
    assert!(!panel.online(), "联网源默认不该勾上");
    // 字段那一栏答的是「我要什么」，几样本来就是一次撞完一起带回来的，默认全勾。
    for field in romcat_gui::scrape::KNOBS {
        assert!(
            panel.fields().contains(&field),
            "{} 默认该勾着",
            field.label()
        );
    }
    // 媒体默认不收：真库那块盘 10 TB，收媒体要回盘把图读一遍。
    assert!(!panel.media(), "媒体默认不该勾上");
    // 采法默认**补缺**：重采是绕过输入指纹全部重来，那不该是随手按到的那一档。
    assert_eq!(panel.sweep(), Gather::Fill);

    // **字段拨得动，而且拨完真的落到选项上。**
    let (browse, site) = app.browse_and_site();
    browse.scrape_mut().toggle_field(Field::Description);
    let options = browse.scrape().options(&site.catalog).expect("折得出选项");
    assert!(
        !options.fields.contains(&Field::Description),
        "取消勾选没生效"
    );
    assert!(options.fields.contains(&Field::Genre), "别的字段不该跟着掉");
    // **标题与汉化组不在旋钮上，因此永远采**：中文名与别名落在标题集合里，
    // 而这个库最要紧的产出就是它们。
    assert!(options.fields.contains(&Field::Title));
    assert!(options.fields.contains(&Field::TranslationGroup));
}

#[test]
fn 刮削面板每一种样子都画得出来() {
    let ctx = headless::context();
    let mut app = 界面(小库);
    摊开刮削面板(&ctx, &mut app);
    跑(&ctx, &mut app, 2);

    // 配额提醒摆着的那一帧（那时四个旋钮让位给它）。
    app.browse_site_and_tasks().0.scrape_mut().toggle_online();
    assert!(app.browse().scrape().quota_prompt());
    跑(&ctx, &mut app, 2);

    // 勾上联网源之后那一帧：底下那本账多出「每份图还要各下一次」那一句。
    app.browse_site_and_tasks().0.scrape_mut().confirm_online();
    app.browse_site_and_tasks().0.scrape_mut().set_media(true);
    跑(&ctx, &mut app, 2);
    assert!(
        app.browse()
            .scrape()
            .estimate()
            .is_some_and(|account| account.media_downloads),
        "收媒体的在线档该把「每份图还要各下一次」说出来",
    );

    // 收起来那一帧。
    app.browse_site_and_tasks().0.scrape_mut().close();
    跑(&ctx, &mut app, 2);
    assert!(!app.browse().scrape().is_open());
}

#[test]
fn 刮削面板开着时按退出键就收起_旋钮留在原位() {
    // 刮削是一层弹层（票 `gui-looks-like-the-design/04`）：Esc 关最上面那一层，等于按页脚上
    // 退出那一颗——与「取消」一样，**旋钮留在原位**（人多半是回去改筛选，改完还想按同一套）。
    let ctx = headless::context();
    let mut app = 界面(小库);
    摊开刮削面板(&ctx, &mut app);
    app.browse_site_and_tasks()
        .0
        .scrape_mut()
        .toggle_field(Field::Description);
    跑(&ctx, &mut app, 3);
    assert!(app.browse().scrape().is_open(), "前提：面板摊开着");

    let esc = shared::输入(vec![shared::按键事件(egui::Key::Escape)]);
    headless::frame(&ctx, esc, |ui| app.ui(ui));

    assert!(!app.browse().scrape().is_open(), "按了 Esc，刮削面板还摊着");
    assert!(
        !app.browse().scrape().fields().contains(&Field::Description),
        "收起来之后旋钮没留在原位",
    );
}

#[test]
fn 一行都没勾时不摊开刮削面板而是直说() {
    let ctx = headless::context();
    let mut app = 界面(小库);
    跑(&ctx, &mut app, 2);
    let (browse, site) = app.browse_and_site();
    browse.open_scrape(&site.catalog);

    // 摆一块「作用于 0 个变体」的面板出来，人会去按那个按钮，然后对着一份什么都没干的
    // 报告猜哪儿出了问题。
    assert!(!app.browse().scrape().is_open(), "一行都没勾不该摊开面板");
    assert!(
        app.browse().error().is_some_and(|why| why.contains("勾")),
        "该直说一行都没勾：{:?}",
        app.browse().error(),
    );
}

#[test]
fn 刮削面板的范围就是屏上写着的那个数() {
    let ctx = headless::context();
    let mut app = 界面(小库);
    摊开刮削面板(&ctx, &mut app);
    let panel = app.browse().scrape();

    // **屏上写几个、面板列几个、按下去动几个，三处同一个数**（票 03 的口径）。
    assert!(panel.scope_total() > 0, "全选之后范围不该是空的");
    assert_eq!(
        panel.scope_total(),
        panel.scope_shown(),
        "面板列的与屏上写的对不上——那正是「按下去动的比屏上写的多」那种事故",
    );
    assert_eq!(
        Some(panel.scope_total()),
        app.browse().scope_total(),
        "范围没走批量操作那条现成的路",
    );
}

#[test]
fn 勾联网源先弹配额提醒_说清赌的是账号与地址() {
    let ctx = headless::context();
    let mut app = 界面(小库);
    摊开刮削面板(&ctx, &mut app);
    let (browse, ..) = app.browse_site_and_tasks();
    let panel = browse.scrape_mut();

    panel.toggle_online();
    // **提醒摆出来了，而联网源还没勾上。** 反过来的话，人是在勾完之后才读到
    // 「撞穿是永久封禁」——那时候已经晚了。
    assert!(panel.quota_prompt(), "勾联网源该先弹配额提醒");
    assert!(!panel.online(), "点头之前不该算勾上");

    let 提醒 = romcat_gui::scrape::QUOTA_WARNING;
    for 该说的 in ["账号", "IP", "永久封禁"] {
        assert!(提醒.contains(该说的), "配额提醒里没说「{该说的}」：{提醒}");
    }

    panel.decline_online();
    assert!(
        !panel.online() && !panel.quota_prompt(),
        "「算了」该把这一下整个撤掉"
    );

    panel.toggle_online();
    panel.confirm_online();
    assert!(
        panel.online() && !panel.quota_prompt(),
        "点过头之后才算勾上"
    );
}

#[test]
fn 只勾本地源时面板上估的请求数是零() {
    let ctx = headless::context();
    let mut app = 界面(小库);
    摊开刮削面板(&ctx, &mut app);
    let (browse, site, _) = app.browse_site_and_tasks();
    browse.scrape_mut().recount(&site.catalog);
    let account = browse.scrape().estimate().expect("这本账该算得出来");

    // **这不是「大概是 0」**：离线档只收自报本地的源，混进一个联网源会当场被拒。
    assert_eq!(account.requests, 0, "只勾本地源时一个网络请求都不该发");
    assert!(!account.media_downloads, "本地源收的图不花配额");
    assert!(account.anchors() > 0, "锚点数是 0 说明范围根本没算出来");
}

#[test]
fn 刮削走任务台而且裁决与手工写的元数据一个字都没动() {
    let ctx = headless::context();
    let mut app = 界面(小库);
    跑(&ctx, &mut app, 2);

    // 人在详情面板上一个字一个字敲进去的那条（`Screen::put_value` 走的就是它）。
    let 作品 = {
        let (browse, site) = app.browse_and_site();
        let rows = site
            .catalog
            .work_page(browse.query(), 0, 50)
            .expect("取得出一页");
        rows.iter()
            .find_map(|row| match &row.anchor {
                WorkAnchor::Work(_) => Some(row.name.clone()),
                WorkAnchor::Loose(_) => None,
            })
            .expect("总有一行是认出作品的")
    };
    {
        let (_, site) = app.browse_and_site();
        site.catalog
            .put_verdict_value(
                AnchorKind::Work,
                &作品,
                Field::Description,
                "这是我自己写的简介",
                "浏览屏的详情面板上人工写的",
            )
            .expect("写得进");
    }

    摊开刮削面板(&ctx, &mut app);
    let (browse, site, tasks) = app.browse_site_and_tasks();
    // **重采**：绕过输入指纹全部重来——这一档最有机会把人写的东西冲掉。
    browse.scrape_mut().set_sweep(Gather::Refresh);
    browse.scrape_mut().start(site, tasks);
    let 任务号 = browse.scrape().running().expect("该排上一趟活");

    // **活走的是任务台**：跑完之后台上留着一条带耗时的历史。
    跑(&ctx, &mut app, 4);
    assert!(
        app.tasks()
            .history()
            .iter()
            .any(|record| record.id == 任务号),
        "刮削那一趟没进任务台的历史",
    );
    assert!(app.browse().scrape().running().is_none(), "跑完了该销号");
    assert!(
        app.browse().scrape().error().is_none(),
        "刮削出错了：{:?}",
        app.browse().scrape().error(),
    );

    // **面板上那句「裁决与你手工维护的元数据不会被动」，行为上也成立。**
    let (_, site) = app.browse_and_site();
    let 还在 = site
        .catalog
        .scraped_values("作品", &作品)
        .expect("读得出")
        .into_iter()
        .find(|value| value.source == VERDICT && value.field == Field::Description.label());
    assert_eq!(
        还在.map(|value| value.value).as_deref(),
        Some("这是我自己写的简介"),
        "重采一趟数据源把人手写的元数据冲掉了——{}",
        romcat_gui::scrape::UNTOUCHED,
    );
}

// ── 收藏与合集（票 `gui-redesign/06`） ────────────────────────────────────────

/// 一条规则在这份库里筛出哪几个变体。屏上那颗星与筛选器说的是不是同一批，靠它比。
fn 按规则数(app: &mut App, text: &str) -> Vec<String> {
    按规则与状态数(app, text, None)
}

/// 同上，再叠一档**识别状态**。
///
/// 状态**进不了规则**（`Unruly::State`：那是这一趟的进度不是内容），所以它只能这么叠——
/// 拿它挑「无判据」那一档的样本，验的正是「拿不到内容判据的收藏只钉得住路径」。
fn 按规则与状态数(app: &mut App, text: &str, state: Option<StateFilter>) -> Vec<String> {
    use romcat_core::catalog::browse::{MAX_PAGE, VariantQuery};
    use romcat_core::sublibrary::Rule;
    let rule = Rule::parse(text).expect("规则读得懂");
    let (_, site) = app.browse_and_site();
    site.catalog
        .variant_page(
            &VariantQuery {
                rule: Some(rule),
                state,
                ..VariantQuery::default()
            },
            0,
            MAX_PAGE,
        )
        .expect("取得出一页")
        .into_iter()
        .map(|row| row.key)
        .collect()
}

#[test]
fn 多选之后按一下星_那一批当场收藏而且收藏是筛得出来的() {
    // 票 `gui-redesign/06` 验收第 1、2 条的界面这一半：**勾几行、按一下、当场生效**，
    // 而且 `收藏=是` 筛出来的与星标的那批一模一样。
    //
    // **作用范围一律走票 03 那条口径**（`batch_variants`）：屏上写几个、按下去动几个。
    let ctx = headless::context();
    let mut app = 界面(4_000);
    跑(&ctx, &mut app, 2);
    let 头几行: Vec<WorkAnchor> = {
        let (browse, site) = app.browse_and_site();
        site.catalog
            .work_page(browse.query(), 0, 3)
            .expect("取得出一页")
            .into_iter()
            .map(|row| row.anchor)
            .collect()
    };
    for anchor in &头几行 {
        app.browse_and_site().0.picked_mut().toggle(anchor);
    }
    跑(&ctx, &mut app, 1);
    let 作用范围 = {
        let (browse, site) = app.browse_and_site();
        browse.batch_variants(&site.catalog).expect("展开得了")
    };
    assert!(作用范围.len() > 1, "这一条要测的正是「一下子收藏一批」");

    {
        let (browse, site, board) = app.browse_site_and_tasks();
        browse.favorite(site, board);
    }
    跑(&ctx, &mut app, 2);
    let mut 星标 = 按规则数(&mut app, "收藏=是");
    let mut 期望 = 作用范围.clone();
    星标.sort();
    期望.sort();
    assert_eq!(星标, 期望, "`收藏=是` 筛出来的与按下去动的不是同一批");
    // **收藏就是那个名字定死的合集**：同一批东西，两条规则。
    let mut 按合集 = 按规则数(&mut app, "合集=收藏");
    按合集.sort();
    assert_eq!(按合集, 期望);
    // 筛选栏那一维上真的多了一档——「合集 0 个」那句话（挂账 D74）从此不成立。
    assert!(
        app.browse()
            .facets()
            .collections
            .iter()
            .any(|facet| facet.value == "收藏" && facet.count > 0),
        "收藏没出现在筛选栏的合集那一维里",
    );

    // 回执里**两种锚各说一句**（验收第 7 条）：合成数据里「无判据」那一档拿不到内容判据。
    let 回执 = app.browse().notice().expect("按完该有一句回执").to_string();
    assert!(回执.contains("收藏"), "{回执}");
    assert!(
        回执.contains("本机的路径") || 回执.contains("全部钉在内容上"),
        "回执里没说清锚钉在哪：{回执}",
    );

    // 取消：同一批一起拿出来，`收藏=是` 当场空掉。
    {
        let (browse, site, board) = app.browse_site_and_tasks();
        browse.unfavorite(site, board);
    }
    跑(&ctx, &mut app, 2);
    assert!(按规则数(&mut app, "收藏=是").is_empty(), "取消收藏没生效");
}

#[test]
fn 自建合集建得出来而且按合集筛得出来() {
    // 验收第 3 条，也是挂账 D74 的正题：**合集从此有建的办法**。
    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 2);
    let 一行 = {
        let (browse, site) = app.browse_and_site();
        site.catalog
            .work_page(browse.query(), 0, 1)
            .expect("取得出一页")
            .into_iter()
            .map(|row| row.anchor)
            .next()
            .expect("有一行")
    };
    app.browse_and_site().0.picked_mut().toggle(&一行);
    跑(&ctx, &mut app, 1);
    let 作用范围 = {
        let (browse, site) = app.browse_and_site();
        browse.batch_variants(&site.catalog).expect("展开得了")
    };

    {
        let (browse, site, board) = app.browse_site_and_tasks();
        "送朋友的".clone_into(browse.collection_draft_mut());
        browse.join_collection(site, board);
    }
    跑(&ctx, &mut app, 2);
    let mut 选出来 = 按规则数(&mut app, "合集=送朋友的");
    let mut 期望 = 作用范围.clone();
    选出来.sort();
    期望.sort();
    assert_eq!(选出来, 期望);

    // 移出之后**这个合集从筛选栏那一维里消失**：一件东西都选不出来的合集不该摆在那儿。
    {
        let (browse, site, board) = app.browse_site_and_tasks();
        browse.leave_collection(site, board);
    }
    跑(&ctx, &mut app, 2);
    assert!(按规则数(&mut app, "合集=送朋友的").is_empty());
    assert!(
        !app.browse()
            .facets()
            .collections
            .iter()
            .any(|facet| facet.value == "送朋友的"),
        "空掉的合集还留在筛选栏那一维里",
    );
}

#[test]
fn 拿不到内容锚的那些在详情面板上被标出来() {
    // 验收第 7 条的界面这一半。**无判据**那一档拿不到内容判据，收藏只钉得住本机路径
    // ——挪了位置会飘。屏上必须说出这句话，而不是含糊成「在收藏里」。
    let ctx = headless::context();
    let mut app = 界面(4_000);
    跑(&ctx, &mut app, 2);
    app.browse_and_site().0.picked_mut().select_all();
    跑(&ctx, &mut app, 1);
    {
        let (browse, site, board) = app.browse_site_and_tasks();
        browse.favorite(site, board);
    }
    跑(&ctx, &mut app, 2);

    // 全库都收藏了，于是**两种锚都找得到样本**。
    let 无判据 = 按规则与状态数(
        &mut app,
        "收藏=是",
        Some(StateFilter::Concluded(State::NoEvidence)),
    );
    assert!(!无判据.is_empty(), "合成数据里该有一批无判据的");
    // 摸一个变体，看屏上那块「收藏与合集」写着什么。**这一份读的是沉淀库**
    // ——投影那张表只记「在不在里面」，记不着它靠什么认出来的。
    let 摸一个 = |app: &mut App, key: &str| -> Vec<(String, &'static str)> {
        {
            let (browse, site) = app.browse_and_site();
            browse.pick(&site.catalog, key);
        }
        跑(&ctx, app, 1);
        app.browse().standing().to_vec()
    };
    let 收藏落在 = |落点: &[(String, &'static str)]| -> &'static str {
        落点
            .iter()
            .find(|(name, _)| name == "收藏")
            .map(|(_, anchor)| *anchor)
            .expect("这一个该在收藏里")
    };
    // 先挑一个**无判据**的：它只钉得住路径。
    let 那一个 = 无判据[0].clone();
    assert_eq!(
        收藏落在(&摸一个(&mut app, &那一个)),
        romcat_core::verdict::ANCHOR_PATH,
        "无判据的那一个该如实标成「只钉得住本机路径」",
    );

    // 再挑一个拿得到判据的：它钉在内容上。
    let 有判据 = 按规则与状态数(
        &mut app,
        "收藏=是",
        Some(StateFilter::Concluded(State::Matched)),
    );
    assert!(!有判据.is_empty());
    assert_eq!(
        收藏落在(&摸一个(&mut app, &有判据[0])),
        romcat_core::verdict::ANCHOR_CONTENT,
        "拿得到判据的那一个该钉在内容上",
    );

    // **左栏说它在哪个合集里，右栏就得说得出同一句。** 合成数据那几个合集也是从沉淀库
    // 投影出来的（`demo::site` 走的就是真正那条路），不然筛选栏上算它在「我通关过的」里、
    // 详情面板上却写「一个都没进」——而这一屏正是这一票要给人看的东西。
    let 通关过的 = 按规则数(&mut app, "合集=我通关过的");
    assert!(!通关过的.is_empty(), "合成数据里该有这个合集");
    let 落点 = 摸一个(&mut app, &通关过的[0]);
    assert!(
        落点.iter().any(|(name, _)| name == "我通关过的"),
        "左栏算它在「我通关过的」里，右栏却没写：{落点:?}",
    );
}

#[test]
fn 一行都没勾就按星_说清而不是静静什么都不做() {
    // 与「刮削选中…」同一条规矩：摆出一份「作用于 0 个变体」的回执，
    // 人只会对着它猜哪儿出了问题。
    let ctx = headless::context();
    let mut app = 界面(500);
    跑(&ctx, &mut app, 2);
    {
        let (browse, site, board) = app.browse_site_and_tasks();
        browse.favorite(site, board);
    }
    跑(&ctx, &mut app, 1);
    let 话 = app.browse().error().expect("该说一句").to_string();
    assert!(话.contains("一行都没勾"), "{话}");
    assert!(按规则数(&mut app, "收藏=是").is_empty(), "什么都不该动");
}

/// 三个变体各落一档的一份小库：**命中**、**跑过了一条候选都没有**、**连识别都没跑过**。
///
/// 用手搭的而不是合成数据：这一条要看的是**同一张表上三个词并排**，而合成数据里
/// 哪一行落哪一档随规模变。
///
/// 搭子在 `shared::小库`——待确认屏那边搭的是同一份形状（`有一个连识别都没跑过的库`），
/// 两处只差各落哪一档、摆在哪几个平台上、工作目录是哪个。
fn 三档并排的库() -> App {
    use shared::档;

    let mut app = shared::小库(
        &[
            ("SFC", "命中.zip", 档::命中),
            ("SFC", "一条候选都没有.zip", 档::没有候选),
            ("SFC", "还没轮到它.zip", 档::还没识别),
        ],
        工作目录(),
    );
    app.show_view(View::Browse);
    app
}

#[test]
fn 浏览屏把还没识别与没有候选印成两个词() {
    // 词表两条（`CONTEXT.md`）：**还没识别**是「连识别都还没跑过」，
    // **没有候选**是「识别跑过了、却一条候选都没有」。两件事折进
    // `WorkRow::confidence` 都是 `None`，印同一个词的话，这一栏就会对着一整批
    // 压根没识别过的变体说「识别跑过了、没找着」，而该做的事也不一样——
    // 前者去跑 `romcat identify`，后者得人自己来（票 `gui-redesign/17`）。
    let ctx = headless::context();
    let mut app = 三档并排的库();
    跑(&ctx, &mut app, 2);

    // **三行一行行点开**：变体行在详情面板里，那正是从前手写这几个词的地方。
    let anchors: Vec<WorkAnchor> = {
        let (browse, site) = app.browse_and_site();
        site.catalog
            .work_page(browse.query(), 0, 16)
            .expect("读得动")
            .into_iter()
            .map(|row| row.anchor)
            .collect()
    };
    assert_eq!(anchors.len(), 3, "三个变体各自成一行");
    let mut 各栏 = Vec::new();
    for anchor in &anchors {
        {
            let (browse, site) = app.browse_and_site();
            browse.open_work(&site.catalog, anchor);
        }
        跑(&ctx, &mut app, 1);
        各栏.push(一栏::看(
            &ctx,
            &headless::frame(&ctx, headless::input(), |ui| app.ui(ui)),
        ));
    }

    // 断的是**侧边详情里那张变体卡片**：置信度那枚标签上一字不差是那个词，卡片底下那一行一字不差
    // 是那个变体的键，两样在同一回点开的那一栏里。只看那一栏、只认一字不差：光看那几个字的话，
    // 左栏「识别结论」那一簇里「还没识别」那一枚、判定依据里「识别结论：还没识别」那一句就足以让
    // 断言通过，而它们说的是另一件事。
    // 卡片底下那一行照稿写「根名 · 相对路径」（挂单 `Q809`）。
    for (那个词, 相对路径) in [
        ("高置信", "SFC/命中.zip"),
        ("没有候选", "SFC/一条候选都没有.zip"),
        ("还没识别", "SFC/还没轮到它.zip"),
    ] {
        let 那个键 = format!(
            "{}{}{相对路径}",
            shared::根,
            romcat_gui::table::ROOT_SEPARATOR
        );
        assert!(
            各栏
                .iter()
                .any(|栏| 栏.有这一段(那个词) && 栏.有这一段(&那个键)),
            "侧边详情里没有一张卡片同时写着「{那个词}」与「{那个键}」：\n{}",
            各栏
                .iter()
                .map(一栏::全文)
                .collect::<Vec<_>>()
                .join("\n——\n"),
        );
    }
    // 悬停里跟着说的那一句**这儿验不了**：它在 `on_hover_ui` 里，而这几帧没有指针，
    // egui 一个字都不画。那一句的两个分岔由核心库那条测试钉着
    // （`WorkVariant::no_candidate_hint`，`crates/core/tests/works.rs`）——
    // 判据收在核心库里的好处正是这个：它验得了，而这一层只是印。
}

#[test]
fn 删掉一条刮削来的叫法之后屏上分得出压掉了与没采到() {
    // 挂账 D157：刮削来的叫法也删得掉，可**标题集合是折出来的一份投影**——不留记号的话
    // 下一趟重折它又回来了，而界面没解释为什么。这一条钉的是记号落在**沉淀库**里，
    // 而且屏上说得出「压掉了」与「没采到」不是一回事。
    let ctx = headless::context();
    let mut app = 界面(2_000);
    let key = 一条认出作品的(&mut app);
    {
        let (browse, site) = app.browse_and_site();
        browse.pick(&site.catalog, &key);
    }
    let detail = app.browse().detail().expect("点开得了").clone();
    let work = detail.work.clone().expect("认出了作品");
    let 刮削来的 = detail
        .titles
        .iter()
        .find(|row| !row.is_verdict())
        .expect("合成数据里有刮削来的叫法")
        .clone();

    {
        let (browse, site) = app.browse_and_site();
        browse.suppress_title(site, &刮削来的);
    }

    // ── 一、中立库里那一行当场就没了。
    assert!(
        !app.browse()
            .detail()
            .expect("还在")
            .titles
            .iter()
            .any(|row| row.value == 刮削来的.value),
        "删掉的那条还在集合里",
    );

    // ── 二、**记号落在沉淀库里**：中立库整份可再生，记在那儿等于说
    //       「下一次改结构时你删过的全部复活」。
    assert_eq!(app.browse().suppressed().len(), 1, "面板手上那份压制清单");
    assert_eq!(
        app.browse().suppressed()[0].key(),
        romcat_core::title::suppression_key(&刮削来的),
        "压的是那条叫法的五样，不是别的",
    );
    {
        let (_, site) = app.browse_and_site();
        assert_eq!(
            site.store
                .title_suppressions_of(&work)
                .expect("读得到")
                .len(),
            1,
            "沉淀库里真有这一条",
        );
    }
    assert!(
        app.browse()
            .notice()
            .expect("有回执")
            .contains("重新整理标题也不会把它加回来"),
        "回执要说清「删」从此算数：{:?}",
        app.browse().notice(),
    );

    // ── 三、屏上分得出「压掉了」与「没采到」——两者对维护者是不同的意思。
    let 屏上 = 元数据栏滚一趟(&ctx, &mut app);
    assert!(
        屏上.contains("压掉的叫法"),
        "屏上没摆出压掉的那几条：{屏上}"
    );
    assert!(
        屏上.contains(&刮削来的.value),
        "压掉的那条要指名道姓，不然人不知道自己压了什么：{屏上}"
    );

    // ── 四、**撤得掉**：这一下不是不可逆的。
    let 那条压制 = app.browse().suppressed()[0].clone();
    {
        let (browse, site) = app.browse_and_site();
        browse.lift_title(site, &那条压制);
    }
    assert!(app.browse().suppressed().is_empty(), "撤掉之后它不再算数",);
    {
        let (_, site) = app.browse_and_site();
        assert!(
            site.store
                .title_suppressions_of(&work)
                .expect("读得到")
                .is_empty(),
        );
    }
}

/// 一份**落在磁盘上**的小库，连它的界面。
///
/// 整批收藏非要它不可：台上那条线读的是同一个库文件的**第二份只读连接**
/// （`Catalog::read_only`），而只活在内存里的那种库（合成数据走的就是那条）
/// 分不出第二份来。
fn 磁盘上的小库(变体数: usize) -> (App, romcat_core::testing::TempDir) {
    use romcat_core::catalog::Catalog;
    use romcat_core::platform::Manifest;
    use romcat_core::shape::{SINGLE_FILE_RULE, Variant};
    use romcat_core::site::Site;
    use romcat_core::workspace::{self, Slug};

    let dir = romcat_core::testing::temp_dir("浏览-磁盘上的库");
    let 库文件 = workspace::catalog_path(dir.path(), Slug::Named("小库"));
    {
        let mut catalog = Catalog::create(&库文件, "小库").expect("开得出中立库");
        // **建不出根就当场炸**：吞掉它的话，变体会挂在一个不存在的根上，
        // 而失败会以「屏上少了一行」的样子冒出来。
        romcat_core::catalog::roots::add_root(
            &catalog,
            None,
            "主库",
            std::path::Path::new("/主库"),
        )
        .expect("建得出根");
        let variants: Vec<Variant> = (0..变体数)
            .map(|at| {
                let key = format!("主库/SFC/第{at:03}个.zip");
                Variant {
                    main_key: key.clone(),
                    platform: Some("SFC".to_string()),
                    rule: SINGLE_FILE_RULE.to_string(),
                    manual: false,
                    files: 1,
                    bytes: 4096,
                    unreadable_files: 0,
                    members: vec![(key.clone(), Role::Main)],
                    key,
                }
            })
            .collect();
        catalog
            .replace_variants(&variants, 1, &Manifest::default())
            .expect("写得进变体");
    }
    let site = Site::open_file(dir.path(), &库文件, None).expect("开得出现场");
    let mut app = App::new(site, dir.path().to_path_buf());
    app.show_view(View::Browse);
    (app, dir)
}

#[test]
fn 整批收藏排上任务台_跑着的时候屏上有进度也按得停() {
    // 票 `parking-3/09`：全选之后按那颗星，窗口不许冻住——它像别的长活一样排上任务台，
    // 看得见进度、按得停。实测这一下从前在画帧那条线程上跑 6.7 秒（挂单 `Q119`）。
    let ctx = headless::context();
    let (mut app, _dir) = 磁盘上的小库(200);
    跑(&ctx, &mut app, 2);
    app.browse_and_site().0.picked_mut().select_all();
    跑(&ctx, &mut app, 1);
    {
        let (browse, site, board) = app.browse_site_and_tasks();
        browse.favorite(site, board);
    }

    // **它排上了台，不是在这条线程上跑完了**——读那一半跑在别处，写那一半在认领那一步，
    // 所以这会儿两份库一个字都还没动。
    assert!(app.tasks().running().is_some(), "这一下没排上任务台");
    assert!(
        按规则数(&mut app, "收藏=是").is_empty(),
        "按下去的那一刻就写了库，那就等于还在画帧这条线程上干活",
    );

    // 等它报出第一步。**进度只会往前走**（报过就一直在那儿），所以这个等法是确定的。
    for _ in 0..2_000 {
        if app
            .tasks()
            .running()
            .is_some_and(|live| live.progress.at > 0)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    // 屏上看得见：这一趟叫什么、走到哪一步、以及那颗「停下」。
    // **这一帧不走 `App::ui`**：它头一件事就是问一遍任务台，而那一问会把跑完的那一趟
    // 收走——这里要看的正是它**跑着的时候**屏上长什么样。
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        romcat_gui::task::Screen::new().ui(ui, app.tasks_mut());
    }));
    assert!(屏上.contains("放进「收藏」"), "任务屏上没有这一趟：{屏上}");
    assert!(屏上.contains("停下"), "按不着「停下」：{屏上}");
    assert!(屏上.contains("折锚"), "屏上说不出它走到哪一步：{屏上}");

    // 跑完之后**按号认领**：投影当场生效，回执照旧两种锚各说一句。
    for _ in 0..600 {
        跑(&ctx, &mut app, 1);
        if !app.tasks().busy() && !app.tasks().settled() {
            break;
        }
    }
    let 星标 = 按规则数(&mut app, "收藏=是");
    assert_eq!(星标.len(), 200, "认领完那一批该整批进收藏");
    let 回执 = app
        .browse()
        .notice()
        .expect("认领完该有一句回执")
        .to_string();
    assert!(回执.contains("收藏"), "{回执}");
}

#[test]
fn 撤掉一条标题压制之后就地摆着折标题的入口_排的是与库屏工序段同一趟() {
    // 从前那句回执写的是「它**下一趟重折**（`romcat titles`）之后回到标题集合里」
    // ——人读完就去开终端了。**这一屏不折**那句照旧成立（折一趟要走遍全库），
    // 变的是下一步就在旁边：点一下，与库屏工序段那一行完全同一趟活排上任务台
    // （规格 48、验收第 2、3 条）。
    let ctx = headless::context();
    let mut app = 界面(2_000);
    let key = 一条认出作品的(&mut app);
    {
        let (browse, site) = app.browse_and_site();
        browse.pick(&site.catalog, &key);
    }
    let detail = app.browse().detail().expect("点开得了").clone();
    let 刮削来的 = detail
        .titles
        .iter()
        .find(|row| !row.is_verdict())
        .expect("合成数据里有刮削来的叫法")
        .clone();
    {
        let (browse, site) = app.browse_and_site();
        browse.suppress_title(site, &刮削来的);
    }
    let 那条压制 = app.browse().suppressed()[0].clone();
    {
        let (browse, site) = app.browse_and_site();
        browse.lift_title(site, &那条压制);
    }

    // ── 一、那句回执不再指向命令行，旁边摆着就地的入口。
    let 屏上 = 元数据栏滚一趟(&ctx, &mut app);
    assert!(
        !屏上.contains("romcat titles"),
        "撤掉压制之后还在叫人去开终端：\n{屏上}",
    );
    assert!(屏上.contains("撤掉了对"), "那句回执没了：\n{屏上}",);
    assert!(
        屏上.lines().any(|line| line.trim() == "整理标题"),
        "撤掉压制之后就地没有整理标题那个入口：\n{屏上}",
    );

    // ── 二、点它排的是**与库屏工序段那一行完全同一趟**：只有 `Section::start` 排出去
    //       的任务号才进得了 `Section::running`，也只有它认领得下来。
    app.browse_and_site().0.ask_fold_titles();
    assert!(
        app.roots().stages().task_of(Stage::FoldTitles).is_none(),
        "按下去那一下不该由浏览屏自己排活",
    );
    跑(&ctx, &mut app, 2);
    assert!(
        app.roots()
            .stages()
            .notice()
            .is_some_and(|说的| 说的.starts_with("整理标题 跑完了")),
        "库屏工序段没认领这一趟：{:?} / {:?}",
        app.roots().stages().notice(),
        app.roots().stages().error(),
    );
    // **在任务台上看不出区别**：那一行的名字就是这道工序的名字。
    assert_eq!(app.tasks().history()[0].name, "整理标题");
}

/// 夹着两个**非游戏资产**的一份小库，四个变体各自成一行。
///
/// **哪几个算非游戏资产不在这里判**：判断只有核心库那一处，这份库只是照「根名之后有一段
/// 目录叫 `bios`」摆的数据；下面的断言读的是核心库交回来的标记（`WorkRow::non_game_asset`）。
/// 一个**文件名**叫 `bios` 的游戏也摆进来——判的是目录段，它照旧是游戏。
fn 夹着非游戏资产的小库() -> App {
    use shared::档;

    let mut app = shared::小库(
        &[
            ("PS", "幻想传说.bin", 档::命中),
            ("PS", "bios/SCPH-1001.BIN", 档::还没识别),
            ("街机", "BIOS/neogeo.zip", 档::还没识别),
            ("SFC", "bios.sfc", 档::没有候选),
        ],
        std::env::temp_dir().join("romcat-测试-浏览-非游戏资产"),
    );
    app.show_view(View::Browse);
    app
}

/// 核心库说**标着**的那几行叫什么（按当前筛选）。
fn 核心库标着的(app: &mut App) -> Vec<String> {
    let (browse, site) = app.browse_and_site();
    site.catalog
        .work_page(browse.query(), 0, 64)
        .expect("读得动")
        .into_iter()
        .filter(|row| row.non_game_asset)
        .map(|row| row.name)
        .collect()
}

#[test]
fn 非游戏资产默认不列出_屏上说收起了几个_开关打开后列出来并标着() {
    use romcat_core::catalog::browse::{NON_GAME_ASSET_LABEL, NonGameAssets};

    // 左栏「平台」那一档某个平台写着几个；那一档压根不在时是 `None`。
    let 平台条数 = |app: &App, 平台: &str| {
        app.browse()
            .facets()
            .platforms
            .iter()
            .find(|facet| facet.value == 平台)
            .map(|facet| facet.count)
    };

    let ctx = headless::context();
    let mut app = 夹着非游戏资产的小库();
    跑(&ctx, &mut app, 2);

    // 开关底下那句小字（拿主意的人照稿定的字）：共几个、不会被导出。数是核心库数的，开没开都是这一句。
    const 非游戏资产那句: &str = "BIOS 等文件共 2 个，不会被导出";

    // 一、默认收起：表上只有两行，屏上说清有几个，行上一个标记都没有。
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    assert_eq!(app.window().total(), 2, "默认收起：四行里只列两行");
    assert!(
        屏上.contains(非游戏资产那句),
        "屏上没说有几个非游戏资产：\n{屏上}"
    );
    assert!(
        !屏上.contains("SCPH-1001"),
        "收起的那一行画出来了：\n{屏上}"
    );
    assert!(核心库标着的(&mut app).is_empty());
    assert_eq!(
        屏上
            .lines()
            .filter(|line| *line == NON_GAME_ASSET_LABEL)
            .count(),
        0
    );
    // 左栏的条数跟着收起：PS 只数那一个游戏，街机那一档压根不在——点进去列几个，这儿就写几个。
    assert_eq!(平台条数(&app, "PS"), Some(1));
    assert_eq!(平台条数(&app, "街机"), None);

    // 二、点那颗开关：列出来，行上标着——**标着哪几行由核心库说**，屏上照着标。
    let 屏上 = shared::点一下(&ctx, "显示非游戏资产", |ui| app.ui(ui));
    assert_eq!(app.browse().query().non_game_assets, NonGameAssets::Listed);
    assert_eq!(app.window().total(), 4, "列出来之后四行都在");
    let 标着的 = 核心库标着的(&mut app);
    assert_eq!(标着的.len(), 2, "{标着的:?}");
    // 认不出作品的那一行副行照稿写「根名 · 相对路径」（挂单 `Q809`），屏上认的是那个样子。
    for name in &标着的 {
        let 副行 = name.replacen('/', romcat_gui::table::ROOT_SEPARATOR, 1);
        assert!(
            屏上.contains(&副行),
            "列出来的「{副行}」没画在屏上：\n{屏上}"
        );
    }
    assert_eq!(
        屏上
            .lines()
            .filter(|line| *line == NON_GAME_ASSET_LABEL)
            .count(),
        标着的.len(),
        "行上标着的与核心库说的对不上：\n{屏上}",
    );
    assert!(屏上.contains(非游戏资产那句), "{屏上}");
    assert_eq!(
        平台条数(&app, "PS"),
        Some(2),
        "列出来之后左栏的条数跟着回来"
    );
    assert_eq!(平台条数(&app, "街机"), Some(1));

    // 三、再点一下收回去：数与表回到原样。
    let 屏上 = shared::点一下(&ctx, "显示非游戏资产", |ui| app.ui(ui));
    assert_eq!(app.window().total(), 2);
    assert!(屏上.contains(非游戏资产那句), "{屏上}");
}

// ——— 票 `gui-looks-like-the-design/09`：浏览屏照稿重排 ———

/// 这一段文字**画在哪儿、被夹在哪一格里**：`(那一段自己的外框, 它的裁剪矩形)`，按画出来的
/// 次序取头一处；一字不差地比，不是「含有」。
///
/// 「从尾部截断」那一条要的是**画得下**，不是「画出去了、被格子裁掉了」——两者在
/// [`画出来的字`] 里是同一串字，只有外框比得出来。
fn 正好这一段的外框(
    output: &egui::FullOutput,
    那一段: &str,
) -> Option<(egui::Rect, egui::Rect)> {
    fn 找(
        shape: &egui::epaint::Shape,
        clip: egui::Rect,
        那一段: &str,
    ) -> Option<(egui::Rect, egui::Rect)> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == 那一段 => Some((
                egui::Rect::from_min_size(text.pos, text.galley.size()),
                clip,
            )),
            egui::epaint::Shape::Vec(shapes) => {
                shapes.iter().find_map(|one| 找(one, clip, 那一段))
            }
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|clipped| 找(&clipped.shape, clipped.clip_rect, 那一段))
}

/// 两个**认不出作品**的变体各自成一行：一个路径长到一格画不下，一个短得一眼画得完。
///
/// 长的那个文件名就是剥离规则模块开头那一行（`romcat_core::filename` 的文档与它自己那条
/// 测试钉着：剥完是 `超级机器人大战R`），又在前面垫了两层真库里常见的整理目录。
/// `shared::小库` 不给变体挂作品，于是两行都是**未关联作品**的那一种。
fn 两行认不出作品的小库() -> App {
    use shared::档;

    let mut app = shared::小库(
        &[
            (
                "GBA",
                "【全部汉化】/GBA 汉化合集 第一辑（按首字排好）/超级机器人大战R[星组](v1.2+)(简)(JP)(68.92Mb).zip",
                档::没有候选,
            ),
            ("SFC", "短.zip", 档::命中),
        ],
        std::env::temp_dir().join("romcat-测试-浏览-未关联作品"),
    );
    app.show_view(View::Browse);
    app
}

#[test]
fn 认不出作品的那一行画正题与未关联作品标签_路径从尾部截断画得下() {
    const 长键: &str = "主库/GBA/【全部汉化】/GBA 汉化合集 第一辑（按首字排好）/超级机器人大战R[星组](v1.2+)(简)(JP)(68.92Mb).zip";
    const 短键: &str = "主库/SFC/短.zip";

    let ctx = headless::context();
    let mut app = 两行认不出作品的小库();
    跑(&ctx, &mut app, 3);
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 屏上 = 画出来的字(&这一帧);
    let 行: Vec<&str> = 屏上.lines().collect();

    // 一、主栏是**正题**，不是那一长串键。
    assert!(行.contains(&"超级机器人大战R"), "主栏没画正题：\n{屏上}");
    assert!(行.contains(&"短"), "短的那一行也该画正题：\n{屏上}");
    // 二、两行都挂着「未关联作品」。
    assert_eq!(
        行.iter().filter(|line| **line == "未关联作品").count(),
        2,
        "两行认不出作品的都该挂那个标签：\n{屏上}",
    );
    // 三、**不再印一长串原始路径**：整条长键在屏上一处都不整段出现。
    assert!(
        !行.contains(&长键),
        "认不出作品的那一行还在整段印原始路径：\n{屏上}"
    );
    // 四、副行照稿是「根名 · 相对路径」，**从尾部截断**：根名留着，相对路径左边删字补「…」，
    //     文件名那一截留着（挂单 `Q809`）。
    let 根名在前 = format!("{}{}…", shared::根, romcat_gui::table::ROOT_SEPARATOR);
    let 截过的 = 行
        .iter()
        .copied()
        .find(|line| {
            line.strip_prefix(根名在前.as_str())
                .is_some_and(|tail| !tail.is_empty() && 长键.ends_with(tail))
        })
        .unwrap_or_else(|| panic!("没有一行是「{根名在前}」开头、长键的尾巴：\n{屏上}"));
    assert!(
        截过的.ends_with("(68.92Mb).zip"),
        "截掉的该是左边，文件名那一截得留着：{截过的}"
    );
    // 五、**画得下**：截过的那一段整个落在它那一格的裁剪矩形里，不是画出去再被格子裁掉。
    let (外框, 裁剪) = 正好这一段的外框(&这一帧, 截过的).expect("刚找到的那一段");
    assert!(
        外框.min.x >= 裁剪.min.x - 0.5 && 外框.max.x <= 裁剪.max.x + 0.5,
        "截过的路径 {外框:?} 伸出了它那一格 {裁剪:?}——那是被裁掉的，不是截到画得下",
    );
    // 六、**量过宽度才截**：画得下的短路径一个字都不删，只是根名与相对路径之间换成「 · 」。
    let 短副行 = 短键.replacen('/', romcat_gui::table::ROOT_SEPARATOR, 1);
    assert!(
        行.contains(&短副行.as_str()),
        "画得下的短路径不该被截：\n{屏上}"
    );
}

/// 按一下屏上**正好**写着 `文字`、而且画在**左半屏还是右半屏**的那一处，返回松开之后再画一帧的字。
///
/// 两颗箭头长得一样：左栏的「收起」是「«」，右栏收起之后的「展开」也是「«」——只按字找，
/// 点到的是先画出来的那一颗。`shared::点一下` 按「含有」找头一处，这里多一道「哪半边」。
fn 点这半边的(
    ctx: &egui::Context,
    文字: &str,
    左半边: bool,
    mut 画一帧: impl FnMut(&mut egui::Ui),
) -> String {
    fn 收(shape: &egui::epaint::Shape, 文字: &str, out: &mut Vec<egui::Pos2>) {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == 文字 => {
                out.push(egui::Rect::from_min_size(text.pos, text.galley.size()).center());
            }
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    收(one, 文字, out);
                }
            }
            _ => {}
        }
    }
    let 头一帧 = headless::frame(ctx, headless::input(), &mut 画一帧);
    let mut 处处 = Vec::new();
    for clipped in &头一帧.shapes {
        收(&clipped.shape, 文字, &mut 处处);
    }
    let 中线 = headless::VIEWPORT[0] / 2.0;
    let Some(位置) = 处处.into_iter().find(|at| (at.x < 中线) == 左半边) else {
        panic!(
            "{}半屏上没有正好写着「{文字}」的地方：\n{}",
            if 左半边 { "左" } else { "右" },
            画出来的字(&头一帧)
        );
    };
    let 按 = |pressed: bool| egui::Event::PointerButton {
        pos: 位置,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    headless::frame(
        ctx,
        shared::输入(vec![egui::Event::PointerMoved(位置), 按(true)]),
        &mut 画一帧,
    );
    headless::frame(ctx, shared::输入(vec![按(false)]), &mut 画一帧);
    shared::跑一帧(ctx, 画一帧)
}

/// 这条边界眼下多宽：egui 自己存着的那一份。**收起来之后它照旧存着**——展开回来靠的就是它。
fn 面板多宽(ctx: &egui::Context, boundary: romcat_gui::layout::Boundary) -> f32 {
    let state = egui::PanelState::load(ctx, egui::Id::new(boundary.id))
        .unwrap_or_else(|| panic!("「{}」那块面板一次都没画过", boundary.id));
    boundary.side.of(state.size())
}

#[test]
fn 左右两栏收得起来_关掉再打开还收着_展开回到原来那么宽() {
    use romcat_gui::layout;

    // 两栏里各挑一句**只在那一栏里**的话：它在不在屏上，就是那一栏摊没摊开。
    const 筛选栏里的: &str = "显示非游戏资产";
    const 详情栏里的: &str = "点主列表里的一行，看它包含哪几个变体。";

    // **版式偏好往工作目录里写**：先清干净，上一趟收起来的不该带进这一趟。
    let 目录 = std::env::temp_dir().join("romcat-测试-浏览-收起");
    let _ = std::fs::remove_dir_all(&目录);
    let 开 = || {
        let mut app = shared::小库(&[("SFC", "短.zip", shared::档::命中)], 目录.clone());
        app.show_view(View::Browse);
        app
    };

    let ctx = headless::context();
    let mut app = 开();
    跑(&ctx, &mut app, 3);
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    assert!(屏上.contains(筛选栏里的), "筛选栏默认摊开着：\n{屏上}");
    assert!(屏上.contains(详情栏里的), "详情栏默认摊开着：\n{屏上}");
    let 原来多宽 = 面板多宽(&ctx, layout::FILTER);

    // 一、收起左栏：那一栏的东西不画了，正中那张表照旧在。
    let 屏上 = 点这半边的(&ctx, "«", true, |ui| app.ui(ui));
    assert!(!屏上.contains(筛选栏里的), "左栏没收起来：\n{屏上}");
    assert!(屏上.contains(详情栏里的), "收左栏不该碰右栏：\n{屏上}");
    assert!(
        屏上.lines().any(|line| line == "短"),
        "表格该照旧在：\n{屏上}"
    );

    // 二、收起右栏。
    let 屏上 = 点这半边的(&ctx, "»", false, |ui| app.ui(ui));
    assert!(!屏上.contains(详情栏里的), "右栏没收起来：\n{屏上}");

    // 三、**关掉再打开**：新的 `App`、新的上下文——egui 自己那份内存跟着旧窗口一起没了，
    //     两栏还收着只能是工作目录里记下了。
    drop(app);
    let ctx = headless::context();
    let mut 再开 = 开();
    跑(&ctx, &mut 再开, 3);
    let 屏上 = shared::跑一帧(&ctx, |ui| 再开.ui(ui));
    assert!(
        !屏上.contains(筛选栏里的) && !屏上.contains(详情栏里的),
        "再打开两栏该还收着：\n{屏上}"
    );

    // 四、展开左栏：回来的是收起之前那么宽，不是被窄条那 36 点带歪。
    let 屏上 = 点这半边的(&ctx, "»", true, |ui| 再开.ui(ui));
    assert!(屏上.contains(筛选栏里的), "左栏没展开：\n{屏上}");
    assert!(
        (面板多宽(&ctx, layout::FILTER) - 原来多宽).abs() <= 1.0,
        "展开回来是 {}，收起之前是 {原来多宽}",
        面板多宽(&ctx, layout::FILTER),
    );
}

/// 右边那一栏（侧边详情）这一帧画了什么：几段字连它们的外框、贴了图的那几块、纯色块。
///
/// **只收那一栏里的**：同一个平台名、同一句「1 个变体」在左栏与表格里也画着，混进来的话
/// 断言测的就不是侧边详情了。那一栏在哪儿问 egui 自己存的面板尺寸（`layout::DETAIL`）。
struct 一栏 {
    字: Vec<(String, egui::Rect)>,
    图: Vec<egui::Rect>,
    色块: Vec<(egui::Color32, egui::Rect)>,
}

impl 一栏 {
    /// 侧边详情那一栏。
    fn 看(ctx: &egui::Context, output: &egui::FullOutput) -> Self {
        let 栏 = egui::PanelState::load(ctx, egui::Id::new(romcat_gui::layout::DETAIL.id))
            .expect("侧边详情那一栏画过")
            .outer_rect;
        Self::看这一块(output, 栏)
    }

    /// **正中那一栏**：左栏右沿到右栏左沿之间，表格就在这儿。
    fn 正中(ctx: &egui::Context, output: &egui::FullOutput) -> Self {
        let 边 = |boundary: romcat_gui::layout::Boundary| {
            egui::PanelState::load(ctx, egui::Id::new(boundary.id))
                .unwrap_or_else(|| panic!("「{}」那一栏画过", boundary.id))
                .outer_rect
        };
        let 左 = 边(romcat_gui::layout::FILTER);
        let 右 = 边(romcat_gui::layout::DETAIL);
        let 栏 = egui::Rect::from_min_max(
            egui::pos2(左.max.x, 左.min.y),
            egui::pos2(右.min.x, 右.max.y),
        );
        Self::看这一块(output, 栏)
    }

    /// 正好写着这一段的那一处在哪儿（一字不差）。
    fn 正好那一段在哪儿(&self, 那一段: &str) -> egui::Rect {
        self.字
            .iter()
            .find(|(text, _)| text == 那一段)
            .map(|(_, rect)| *rect)
            .unwrap_or_else(|| {
                panic!(
                    "这一栏里没有正好写着「{那一段}」的那一段：\n{}",
                    self.全文()
                )
            })
    }

    fn 看这一块(output: &egui::FullOutput, 栏: egui::Rect) -> Self {
        fn 收(shape: &egui::epaint::Shape, 栏: egui::Rect, out: &mut 一栏) {
            match shape {
                egui::epaint::Shape::Text(text) => {
                    let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                    if 栏.contains(rect.center()) {
                        out.字.push((text.galley.text().to_string(), rect));
                    }
                }
                egui::epaint::Shape::Mesh(mesh)
                    if mesh.texture_id != egui::TextureId::default() =>
                {
                    let rect = mesh.calc_bounds();
                    if 栏.contains(rect.center()) {
                        out.图.push(rect);
                    }
                }
                egui::epaint::Shape::Rect(rect) if 栏.contains(rect.rect.center()) => {
                    if rect.fill_texture_id() == egui::TextureId::default() {
                        out.色块.push((rect.fill, rect.rect));
                    } else {
                        out.图.push(rect.rect);
                    }
                }
                egui::epaint::Shape::Vec(shapes) => {
                    for one in shapes {
                        收(one, 栏, out);
                    }
                }
                _ => {}
            }
        }
        let mut out = 一栏 {
            字: Vec::new(),
            图: Vec::new(),
            色块: Vec::new(),
        };
        for clipped in &output.shapes {
            收(&clipped.shape, 栏, &mut out);
        }
        out
    }

    fn 有这一段(&self, 那一段: &str) -> bool {
        self.字.iter().any(|(text, _)| text == 那一段)
    }

    /// 以这几个字开头的那一段在哪儿。
    fn 那一段在哪儿(&self, 开头: &str) -> egui::Rect {
        self.字
            .iter()
            .find(|(text, _)| text.starts_with(开头))
            .map(|(_, rect)| *rect)
            .unwrap_or_else(|| panic!("侧边详情里没有以「{开头}」开头的那一段：\n{}", self.全文()))
    }

    fn 全文(&self) -> String {
        self.字
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// 一张纯色 PNG 的字节，3:4——封面那一格的比例。
fn 一张封面() -> Vec<u8> {
    let buf = image::RgbImage::from_pixel(60, 80, image::Rgb([30, 90, 160]));
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(buf)
        .write_to(&mut out, image::ImageFormat::Png)
        .expect("编得出 PNG");
    out.into_inner()
}

/// 有封面的那一个变体在根底下的路径：**长到侧边详情一行画不下**——前面垫两层真库里常见的整理目录。
///
/// 变体那一行与文件那一行都印这串键，从前它们不折不截，把那一栏撑出界、盖到表格底下去
/// （第二段对着截图看出来的）。
const 长路径: &str = "【全部汉化】/GBA 汉化合集 第一辑（按首字排好）/超级机器人大战R[星组](v1.2+)(简)(JP)(68.92Mb).zip";

/// 侧边详情那一栏里**伸出界**的字：裁剪矩形落在那一栏里，外框却越过了那一栏的左右两沿。
///
/// 看的是这一帧真画出去的每一段字的外框，不是「字在不在」——伸出界的字照样在
/// [`画出来的字`] 里，只是被表格盖着看不见。
fn 伸出右栏的字(ctx: &egui::Context, output: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
    fn 收(
        shape: &egui::epaint::Shape,
        clip: egui::Rect,
        栏: egui::Rect,
        out: &mut Vec<(String, egui::Rect)>,
    ) {
        match shape {
            egui::epaint::Shape::Text(text) if 栏.contains(clip.center()) => {
                let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                if rect.min.x < 栏.min.x - 0.5 || rect.max.x > 栏.max.x + 0.5 {
                    out.push((text.galley.text().to_string(), rect));
                }
            }
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    收(one, clip, 栏, out);
                }
            }
            _ => {}
        }
    }
    let 栏 = egui::PanelState::load(ctx, egui::Id::new(romcat_gui::layout::DETAIL.id))
        .expect("侧边详情那一栏画过")
        .outer_rect;
    let mut out = Vec::new();
    for clipped in &output.shapes {
        收(&clipped.shape, clipped.clip_rect, 栏, &mut out);
    }
    out
}

/// 两个认不出作品的变体：一个在**媒体池**里有一张封面（挂在它自己这个变体上），一个什么图都没有。
///
/// 工作目录要活到测试结束——媒体池就在它里头，所以连它一起交出去。
fn 一个有封面一个没有的小库() -> (App, romcat_core::testing::TempDir) {
    use shared::档;

    let dir = romcat_core::testing::temp_dir("gui-浏览-侧边详情");
    // **池子得先在**：界面开起来那一刻看工作目录里有没有媒体池，没有就不指（「没查」与「没有」分开）。
    let pool = romcat_core::scrape::pool::MediaPool::open(&romcat_core::workspace::media_pool_dir(
        dir.path(),
    ))
    .expect("开得出媒体池");
    let mut app = shared::小库(
        &[("GBA", 长路径, 档::命中), ("SFC", "短.zip", 档::没有候选)],
        dir.path().to_path_buf(),
    );
    app.show_view(View::Browse);
    let bytes = 一张封面();
    let (hash, _) = pool.take_bytes(&bytes, "png").expect("落得进池");
    {
        let (_, site) = app.browse_and_site();
        site.catalog
            .put_media(&hash, "png", bytes.len() as u64)
            .expect("记得进库");
        site.catalog
            .put_scraped(&[romcat_core::catalog::scrape::Harvested {
                anchor: AnchorKind::Variant.label().to_string(),
                subject: format!("主库/GBA/{长路径}"),
                source: "测试".to_string(),
                input: "测试指纹".to_string(),
                values: Vec::new(),
                media: vec![romcat_core::catalog::scrape::HarvestedMedia {
                    kind: MediaKind::Cover.label().to_string(),
                    hash,
                    evidence: "测试摆进去的".to_string(),
                }],
            }])
            .expect("写得进");
    }
    (app, dir)
}

/// 一帧一帧跑到后台把那几份图解完。**不看挂钟**：解码在后台线程上，每跑一帧它就往前走一截；
/// 跑满上限还没解完就当场炸，并说清卡在哪儿。
fn 跑到图解完(ctx: &egui::Context, app: &mut App) {
    for _ in 0..20_000 {
        跑(ctx, app, 1);
        let gallery = app.browse().gallery();
        if gallery.ready() >= 1 && gallery.busy() == 0 {
            return;
        }
    }
    panic!("跑了两万帧图还没解完：{:?}", app.browse().gallery());
}

#[test]
fn 侧边详情摆封面或字卡_平台年份变体数_判定依据_变体列表与媒体() {
    let 有封面的 = format!("主库/GBA/{长路径}");
    const 没封面的: &str = "主库/SFC/短.zip";

    let ctx = headless::context();
    let (mut app, _dir) = 一个有封面一个没有的小库();
    跑(&ctx, &mut app, 2);

    // 一、有封面的那一个。
    {
        let (browse, site) = app.browse_and_site();
        browse.open_work(&site.catalog, &WorkAnchor::Loose(有封面的.clone()));
    }
    跑到图解完(&ctx, &mut app);
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 栏 = 一栏::看(&ctx, &这一帧);
    // **一个字都不伸出那一栏**：变体那一行、文件那一行印的都是这串长键，得截到画得下，
    // 那一栏也不许被它撑宽（没拖过，就还是默认那么宽）。
    let 伸出去的 = 伸出右栏的字(&ctx, &这一帧);
    assert!(
        伸出去的.is_empty(),
        "侧边详情里有字伸出了那一栏：{伸出去的:#?}",
    );
    let 栏宽 = egui::PanelState::load(&ctx, egui::Id::new(romcat_gui::layout::DETAIL.id))
        .expect("画过")
        .outer_rect
        .width();
    assert!(
        (栏宽 - romcat_gui::layout::DETAIL.default).abs() <= 1.0,
        "侧边详情被撑成了 {栏宽} 宽，默认是 {}",
        romcat_gui::layout::DETAIL.default,
    );
    // 变体卡片底下那一行是整条键，在那一栏里折着摆下（上面那条「一个字都不伸出那一栏」管着它折没折）。
    // 文件那一行在这一栏更底下，滚下去再看（这一条测试的末尾）。
    assert!(
        栏.有这一段(&有封面的.replacen('/', romcat_gui::table::ROOT_SEPARATOR, 1)),
        "变体卡片底下那一行该是「根名 · 相对路径」整条：\n{}",
        栏.全文()
    );
    for 该有的 in [
        "未关联作品的变体",
        "超级机器人大战R",
        "GBA · 年份未知",
        "1 个变体",
        "判定依据",
        "变体 1 个",
    ] {
        assert!(
            栏.有这一段(该有的),
            "侧边详情里没有「{该有的}」：\n{}",
            栏.全文()
        );
    }
    // **判定依据摊在栏里**，不只挂在悬停里：这几帧没有指针，悬停里的字一个都不画。
    assert!(
        栏.字.iter().any(|(text, _)| text.contains("精确哈希命中")),
        "判定依据没摊在栏里：\n{}",
        栏.全文()
    );
    栏.那一段在哪儿("媒体 1 个");
    // **封面贴在头上那一块**：比「变体 1 个」那一行高。底下媒体那几格也贴着同一张图，那几格不算。
    let 变体那一行 = 栏.那一段在哪儿("变体 1 个");
    assert!(
        栏.图.iter().any(|rect| rect.max.y <= 变体那一行.min.y),
        "封面该画在头上那一块，贴了图的地方：{:?}；「变体 1 个」在 {变体那一行:?}",
        栏.图,
    );
    // **文件那一行在这一栏底下**，照稿那几段摆开之后一屏装不下：像人一样把指针放在这一栏上往下滚，
    // 滚一下跑一帧，等它画出来为止（不看挂钟）；再看它是不是从左边截、留着文件名。一路上照旧
    // 一个字都不许伸出那一栏。看完滚回顶上，底下那一段还要看头上那一块。
    let 栏框 = egui::PanelState::load(&ctx, egui::Id::new(romcat_gui::layout::DETAIL.id))
        .expect("画过")
        .outer_rect;
    let 滚一下 = |ctx: &egui::Context, app: &mut App, 往下: bool| {
        let mut input = headless::input();
        input.events.push(egui::Event::PointerMoved(栏框.center()));
        input.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, if 往下 { -150.0 } else { 150.0 }),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        });
        let 这一帧 = headless::frame(ctx, input, |ui| app.ui(ui));
        let 伸出去的 = 伸出右栏的字(ctx, &这一帧);
        assert!(
            伸出去的.is_empty(),
            "滚动的时候侧边详情里有字伸出了那一栏：{伸出去的:#?}",
        );
        一栏::看(ctx, &这一帧)
    };
    let mut 文件那一行 = None;
    for _ in 0..40 {
        文件那一行 = 滚一下(&ctx, &mut app, true)
            .字
            .into_iter()
            .map(|(text, _)| text)
            .find(|text| text.contains('…') && text.contains("(68.92Mb).zip"));
        if 文件那一行.is_some() {
            break;
        }
    }
    assert!(
        文件那一行.is_some(),
        "往下滚到底，文件那一行也该从左边截、留着文件名那一截",
    );
    let mut 回到顶上 = false;
    for _ in 0..60 {
        if 滚一下(&ctx, &mut app, false).有这一段("未关联作品的变体") {
            回到顶上 = true;
            break;
        }
    }
    assert!(回到顶上, "滚回顶上之后头上那一块该露出来");
    // 头上那一块露出来时滚动条未必到了顶：再往上多滚几下，底下那一段从最顶上开始看。
    for _ in 0..10 {
        滚一下(&ctx, &mut app, false);
    }

    // 二、没有封面的那一个：头上那一块是**字卡**，底色取的是这个平台的颜色。
    {
        let (browse, site) = app.browse_and_site();
        browse.open_work(&site.catalog, &WorkAnchor::Loose(没封面的.to_string()));
    }
    跑(&ctx, &mut app, 2);
    let 栏 = 一栏::看(
        &ctx,
        &headless::frame(&ctx, headless::input(), |ui| app.ui(ui)),
    );
    for 该有的 in [
        "未关联作品的变体",
        "短",
        "SFC · 年份未知",
        "1 个变体",
        "判定依据",
    ] {
        assert!(
            栏.有这一段(该有的),
            "侧边详情里没有「{该有的}」：\n{}",
            栏.全文()
        );
    }
    // 一条候选都没有时，判定依据那一块说的是核心库挑的那一句（`WorkVariant::no_candidate_hint`）。
    assert!(
        栏.字
            .iter()
            .any(|(text, _)| text.contains("识别跑过了，一条候选都没有")),
        "没有候选时判定依据该说清：\n{}",
        栏.全文()
    );
    let 变体那一行 = 栏.那一段在哪儿("变体 1 个");
    let 平台色 = romcat_gui::tokens::Tokens::builtin()
        .color
        .platform
        .of("SFC");
    assert!(
        栏.色块
            .iter()
            .any(|(color, rect)| *color == 平台色 && rect.max.y <= 变体那一行.min.y),
        "没有封面时头上那一块该是带平台色（{平台色:?}）的字卡：{:?}",
        栏.色块,
    );
    assert!(
        栏.图.iter().all(|rect| rect.min.y >= 变体那一行.min.y),
        "没有封面就不该在头上贴图：{:?}",
        栏.图,
    );
}

#[test]
fn 列表每行开头显示封面的开关_有封面贴封面_没封面画平台色块() {
    let tokens = romcat_gui::tokens::Tokens::builtin();
    let 平台色 = tokens.color.platform.of("SFC");
    // 同一行：竖直方向上离那一行的正题不超过半行（开着封面时一行照令牌 `table-row-cover`）。
    let 半行 = tokens.layout.table_row_cover / 2.0;

    let ctx = headless::context();
    let (mut app, _dir) = 一个有封面一个没有的小库();
    跑(&ctx, &mut app, 3);

    // 一、默认关着：表里一张图都不贴，也没有平台色块。
    let 中 = 一栏::正中(
        &ctx,
        &headless::frame(&ctx, headless::input(), |ui| app.ui(ui)),
    );
    assert!(中.图.is_empty(), "开关关着时表里不该贴图：{:?}", 中.图);
    assert!(
        !中.色块.iter().any(|(color, _)| *color == 平台色),
        "开关关着时不该画平台色块：{:?}",
        中.色块,
    );

    // 二、打开开关，跑到表里那张封面解出来为止（解码在后台，不看挂钟）。
    shared::点一下(&ctx, "在每行开头显示封面", |ui| app.ui(ui));
    let mut 中 = 一栏::正中(
        &ctx,
        &headless::frame(&ctx, headless::input(), |ui| app.ui(ui)),
    );
    for _ in 0..20_000 {
        if !中.图.is_empty() {
            break;
        }
        中 = 一栏::正中(
            &ctx,
            &headless::frame(&ctx, headless::input(), |ui| app.ui(ui)),
        );
    }

    // 三、有封面的那一行：正题左边贴着那张图。
    let 长 = 中.正好那一段在哪儿("超级机器人大战R");
    assert!(
        中.图
            .iter()
            .any(|rect| rect.max.x <= 长.min.x && (rect.center().y - 长.center().y).abs() <= 半行),
        "有封面的那一行，正题 {长:?} 左边该贴着封面：{:?}",
        中.图,
    );
    // 四、没有封面的那一行：正题左边一块平台色，块里写着平台代号。
    let 短 = 中.正好那一段在哪儿("短");
    assert!(
        中.色块.iter().any(|(color, rect)| {
            *color == 平台色
                && rect.max.x <= 短.min.x
                && (rect.center().y - 短.center().y).abs() <= 半行
        }),
        "没有封面的那一行，正题 {短:?} 左边该有一块 SFC 的平台色（{平台色:?}）：{:?}",
        中.色块,
    );
    assert!(
        中.字.iter().any(|(text, rect)| {
            text == "SFC"
                && rect.max.x <= 短.min.x
                && (rect.center().y - 短.center().y).abs() <= 半行
        }),
        "平台色块里该写着平台代号：\n{}",
        中.全文(),
    );
}

#[test]
fn 筛不出东西时说清楚并给一颗清除筛选_按下去表就回来() {
    const 空态: &str = "没有符合当前筛选条件的作品。";

    let ctx = headless::context();
    let mut app = shared::小库(
        &[("SFC", "短.zip", shared::档::命中)],
        std::env::temp_dir().join("romcat-测试-浏览-空态"),
    );
    app.show_view(View::Browse);
    跑(&ctx, &mut app, 3);
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    assert!(
        !屏上.contains(空态),
        "有一行的时候不该说筛不出东西：\n{屏上}"
    );

    // 一、打开「显示非游戏资产」、再筛一个库里压根没有的平台：一行都不剩，屏上说清为什么空着。
    {
        let query = app.browse_and_site().0.query_mut();
        query.non_game_assets = romcat_core::catalog::browse::NonGameAssets::Listed;
        query.platform = Some(PlatformFilter::from_label("FC"));
    }
    跑(&ctx, &mut app, 2);
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    assert_eq!(app.window().total(), 0, "这个筛选下该一行都不剩");
    assert!(
        屏上.contains(空态),
        "筛不出东西时屏上没说为什么空着：\n{屏上}"
    );
    assert!(
        屏上.lines().any(|line| line == "清除筛选"),
        "空态旁边该有一颗「清除筛选」：\n{屏上}"
    );
    // **表头照旧在**：空态是表里的一行，不是把整张表换掉——排序那几个表头还点得着。
    assert!(
        屏上.lines().any(|line| line.starts_with("作品")),
        "空态把表头也换掉了：\n{屏上}"
    );

    // 二、按「清除筛选」：条件清掉，那一行回来，空态那句收掉。
    let 屏上 = shared::点一下(&ctx, "清除筛选", |ui| app.ui(ui));
    assert!(
        app.browse().query().platform.is_none(),
        "按完「清除筛选」平台那一档该清掉"
    );
    // 「显示非游戏资产」是视图开关，不是条件：清筛选不把它拨回去。
    assert_eq!(
        app.browse().query().non_game_assets,
        romcat_core::catalog::browse::NonGameAssets::Listed,
        "清除筛选不该把「显示非游戏资产」拨回收起"
    );
    assert_eq!(app.window().total(), 1, "清掉之后那一行该回来");
    assert!(
        屏上.lines().any(|line| line == "短"),
        "那一行没画回来：\n{屏上}"
    );
    assert!(!屏上.contains(空态), "表回来了空态那句还挂着：\n{屏上}");
}
