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
    shared::干净工作目录("romcat-测试-浏览")
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

/// 按一下主列表**表头**上那一列。
///
/// **不走 `shared::点一下`**，两处过不去：
///
/// 1. 「平台」这类词**左栏也有一处**（条件组里维度那个下拉），而 `shared::点一下` 按
///    「含有」找**头一处**，点到的是左栏那一个。所以这里按**整段一字不差**找，
///    再取**最靠右**的那一处——主列表在左栏右边。
/// 2. **取 galley 的中心点会点到隔壁那一列**：实测点「容量」选中的是「年份」。
///    靠右那两列（变体、容量）的列名画在自己这一格的右头，而这一层量到的 galley 矩形
///    与屏上那几个字的位置**对不齐**（`Shape::Text` 的 `pos` 在这条路上不是最终屏幕
///    坐标——基线图 `snapshots/browse/rows-*.png` 上那几个字是正常右对齐的，没有出格）。
///    没有去追那个偏移是哪儿来的：这条测试要的是「点得中这一列」，不是「量得准这一格」。
///
/// 于是点的是那几个字的**左缘**而不是中心。**点没点中由断言兜着**：底下那条测试点了
/// 作品、容量、年份三列，每一下都断言选中的是哪一列，点错列当场红。
/// **要量表头的对齐，去看基线图，别拿这儿的矩形当真。**
fn 点表头(ctx: &egui::Context, 列名: &str, mut 画一帧: impl FnMut(&mut egui::Ui)) {
    fn 收(shape: &egui::epaint::Shape, 列名: &str, out: &mut Vec<egui::Rect>) {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == 列名 => {
                out.push(egui::Rect::from_min_size(text.pos, text.galley.size()));
            }
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    收(one, 列名, out);
                }
            }
            _ => {}
        }
    }
    let 头一帧 = headless::frame(ctx, headless::input(), &mut 画一帧);
    let mut 处处 = Vec::new();
    for clipped in &头一帧.shapes {
        收(&clipped.shape, 列名, &mut 处处);
    }
    // 主列表在屏子中间那一大块：左栏那一份（如果有）在它左边。取**最靠右**的那一处。
    let Some(那一格) = 处处.into_iter().max_by(|a, b| a.min.x.total_cmp(&b.min.x)) else {
        panic!(
            "屏上没有正好写着「{列名}」的表头：\n{}",
            shared::画出来的字(&头一帧)
        );
    };
    let 位置 = egui::pos2(那一格.min.x + 2.0, 那一格.center().y);
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
    shared::跑一帧(ctx, 画一帧);
}

/// **点表头：一下正着、两下倒着、三下回到默认那一种**
/// （票 `gui-looks-like-the-design/11` 验收第 1 条）。
///
/// 第三下要紧的是**搜索着的时候**：默认那一种排法就是按匹配质量排
/// （`WorkQuery::sorted_by_default`），没有这一下的话，人点过一次表头就再也回不到
/// 「匹配得好的排前面」，除非把搜索词删掉重打。
///
/// **默认那一列（作品）只有两态**：默认那一种就是它正着排，它的第三态与第一态是同一个。
#[test]
fn 点表头一下正着两下倒着三下回到默认() {
    let ctx = headless::context();
    let mut app = 界面(ROWS);
    跑(&ctx, &mut app, 2);
    let 排法 = |app: &mut App| {
        let (browse, _) = app.browse_and_site();
        let query = browse.query();
        (query.order, query.descending, query.sorted_by_default())
    };
    assert_eq!(
        排法(&mut app),
        (WorkOrder::Name, false, true),
        "一进屏本该是默认那一种排法",
    );

    // 一下：按这一列正着排。
    点表头(&ctx, "容量", |ui| app.ui(ui));
    assert_eq!(
        排法(&mut app),
        (WorkOrder::Bytes, false, false),
        "点头一下没换成按容量正着排",
    );

    // 两下：同一列翻方向。
    点表头(&ctx, "容量", |ui| app.ui(ui));
    assert_eq!(
        排法(&mut app),
        (WorkOrder::Bytes, true, false),
        "再点一下没翻成倒着排",
    );

    // 三下：回到默认那一种。
    点表头(&ctx, "容量", |ui| app.ui(ui));
    assert_eq!(
        排法(&mut app),
        (WorkOrder::Name, false, true),
        "点第三下没回到默认那一种排法",
    );

    // 换一列照样走这三下——不是只有容量那一列特殊。
    点表头(&ctx, "年份", |ui| app.ui(ui));
    assert_eq!(排法(&mut app), (WorkOrder::Year, false, false));
    点表头(&ctx, "年份", |ui| app.ui(ui));
    assert_eq!(排法(&mut app), (WorkOrder::Year, true, false));
    点表头(&ctx, "年份", |ui| app.ui(ui));
    assert_eq!(排法(&mut app), (WorkOrder::Name, false, true));

    // **作品那一列只有两态**：默认那一种就是它正着排。
    点表头(&ctx, "作品", |ui| app.ui(ui));
    assert_eq!(
        排法(&mut app),
        (WorkOrder::Name, true, false),
        "点默认那一列该直接翻成倒着排",
    );
    点表头(&ctx, "作品", |ui| app.ui(ui));
    assert_eq!(
        排法(&mut app),
        (WorkOrder::Name, false, true),
        "再点一下该回到默认那一种",
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

    // 挑第二个：详情跟着换到它。变体级的操作（改选择时右栏选中那张卡底下那几颗例外按钮）作用的正是这一个
    // ——按下去落在哪个变体上，钉在 `tests/sublibrary.rs` 的「改选择时例外按钮与备注框…」那一条。
    {
        let (browse, site) = app.browse_and_site();
        browse.pick(&site.catalog, &变体们[1]);
    }
    assert_eq!(app.browse().variant_key(), Some(变体们[1].as_str()));
    assert_eq!(
        app.browse().detail().map(|detail| detail.row.key.as_str()),
        Some(变体们[1].as_str()),
        "选中了第二个，详情里摆的还是别的变体",
    );
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

/// `one_line` 是公开的（待确认屏画刮削来的值走它）：「原样画得下的**一个字都不动**」只有从函数这一侧看得见——
/// 屏上画的是同一串字，中间换没换过一份字符串出去，看画出来的那一帧看不出来。
#[test]
fn 一行画得下的值原样不动_带换行的折平_顶到闸上的收住() {
    assert_eq!(browse::one_line("两个人一起打外星人。"), None);
    assert_eq!(
        browse::one_line("　　两个人一起打外星人。\n第二段：外星人赢了。").as_deref(),
        Some("　　两个人一起打外星人。 第二段：外星人赢了。"),
        "换行折平成一段，不掐两头",
    );
    let 收住的 = browse::one_line(&"外".repeat(romcat_core::scrape::zh::DESCRIPTION_LIMIT))
        .expect("顶到闸上的要收住");
    assert!(
        收住的.ends_with('…') && 收住的.chars().count() < 80,
        "收窄过要看得出来，而且一行画得下：{收住的}"
    );
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
fn 三种组合方式在筛选器上各自成立而且组嵌得动() {
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

/// 全选一批，再按「刮削…」。**走的是界面上那条一模一样的路**：
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

    assert!(panel.is_open(), "按「刮削…」该把面板摊开");
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

    // 人在详情面板上一个字一个字敲进去的那条（作品详情页上按「保存」落的就是这一种）。
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
    // 与「刮削…」同一条规矩：摆出一份「作用于 0 个变体」的回执，
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

    // ── 三、屏上分得出「压掉了」与「没采到」：作品详情页标题那一面底下列着「已隐藏的名称」，钉在
    //       `tests/work.rs` 的「标题那一面列出标题集合…」那一条。
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

    // 屏上看得见：这一趟叫什么、走到哪一步、以及那颗「停止」。
    // **这一帧不走 `App::ui`**：它头一件事就是问一遍任务台，而那一问会把跑完的那一趟
    // 收走——这里要看的正是它**跑着的时候**屏上长什么样。
    let 屏上 = 画出来的字(&headless::frame(&ctx, headless::input(), |ui| {
        romcat_gui::task::Screen::new().ui(ui, app.tasks_mut());
    }));
    assert!(屏上.contains("放进「收藏」"), "任务屏上没有这一趟：{屏上}");
    assert!(屏上.contains("停止"), "按不着「停止」：{屏上}");
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

    // ── 一、那句回执不再指向命令行，旁边摆着就地的入口：画在作品详情页标题那一面上，钉在 `tests/work.rs` 的
    //       「标题那一面列出标题集合…」那一条。
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
        shared::干净工作目录("romcat-测试-浏览-非游戏资产"),
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
    // 作品那一列窄得摆不下时副行**从尾部截断**（同一条挂单）：根名留着、相对路径左边删字补「…」；
    // 写上根名就把文件名挤没时省掉根名、只写「…尾巴」（Q809 的例外）——这几种都算画着。
    // 左边多了导航、标签挪到副行开头之后，1280 宽的窗口里这一格就只有那么宽。
    for name in &标着的 {
        let 副行 = name.replacen('/', romcat_gui::table::ROOT_SEPARATOR, 1);
        let (根名, 相对路径) = name.split_once('/').expect("键里带着根名");
        let 截过的开头 = format!("{根名}{}…", romcat_gui::table::ROOT_SEPARATOR);
        let 是尾巴 = |tail: &str| !tail.is_empty() && 相对路径.ends_with(tail);
        let 画着 = 屏上.lines().any(|line| {
            line == 副行
                || line.strip_prefix(截过的开头.as_str()).is_some_and(是尾巴)
                || line.strip_prefix('…').is_some_and(是尾巴)
        });
        assert!(
            画着,
            "列出来的「{副行}」没画在屏上（整段、从尾部截断、省掉根名的「…尾巴」都算）：\n{屏上}"
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
        shared::干净工作目录("romcat-测试-浏览-未关联作品"),
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
    // 四、副行在标签后头（挂单 `Q878`），照 `Q809` 写「根名 · 相对路径」、画不下从左边删字补「…」；写上根名就会
    //     把文件名挤没时省掉根名、只写「…尾巴」（Q809 的例外，拿主意的人 2026-09-14 定）。
    //     副行是正题底下、标签后头那一段；留下来的那一截得是相对路径的**尾巴**（截掉的是左边）。
    let 副行 = |行: &[&str], 正题: &str| -> String {
        let 在 = 行
            .iter()
            .position(|line| *line == 正题)
            .unwrap_or_else(|| panic!("没画正题「{正题}」"));
        assert_eq!(
            行.get(在 + 1).copied(),
            Some("未关联作品"),
            "「{正题}」底下头一样该是标签"
        );
        行.get(在 + 2)
            .copied()
            .unwrap_or_else(|| panic!("「{正题}」那一行没有副行"))
            .to_owned()
    };
    let 带根名 = format!("{}{}", shared::根, romcat_gui::table::ROOT_SEPARATOR);
    let 留下的尾巴 = |副行: &str, 键: &str| -> String {
        let 相对路径 = 键.split_once('/').expect("键里带着根名").1;
        let 去掉根名 = 副行.strip_prefix(带根名.as_str()).unwrap_or(副行);
        let 尾巴 = 去掉根名.strip_prefix('…').unwrap_or(去掉根名);
        assert!(
            !尾巴.is_empty() && 相对路径.ends_with(尾巴),
            "副行「{副行}」留下的不是相对路径的尾巴"
        );
        尾巴.to_owned()
    };
    //     1280 宽的窗口里扣掉标签，这一格只剩六十来点：两行都至少留得下文件名末尾几个字、以 `.zip` 结尾。
    let 长副行 = 副行(&行, "超级机器人大战R");
    let 短副行 = 副行(&行, "短");
    for (那一行, 键) in [(&长副行, 长键), (&短副行, 短键)] {
        let 尾巴 = 留下的尾巴(那一行, 键);
        assert!(
            尾巴.chars().count() >= 4 && 尾巴.ends_with(".zip"),
            "1280 宽下标签后头的副行该留得下文件名末尾几个字、以 .zip 结尾：「{那一行}」"
        );
    }
    assert!(
        长副行.contains('…'),
        "长路径在 1280 宽下该截过：「{长副行}」"
    );
    // 五、**画得下**：截过的那一段整个落在它那一格的裁剪矩形里，不是画出去再被格子裁掉。
    let (外框, 裁剪) = 正好这一段的外框(&这一帧, &长副行).expect("刚找到的那一段");
    assert!(
        外框.min.x >= 裁剪.min.x - 0.5 && 外框.max.x <= 裁剪.max.x + 0.5,
        "截过的路径 {外框:?} 伸出了它那一格 {裁剪:?}——那是被裁掉的，不是截到画得下",
    );
    // 六、**宽窗口里宽度够**：两行都照 `Q809` 带着根名；画得下的短路径一个字都不删，只是根名与相对路径之间
    //     换成「 · 」。窗口只是这一条里拉宽，别的几条照旧是 1280。
    let 宽窗口 = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(2560.0, headless::VIEWPORT[1]),
        )),
        ..Default::default()
    };
    for _ in 0..3 {
        headless::frame(&ctx, 宽窗口.clone(), |ui| app.ui(ui));
    }
    let 宽的一帧 = headless::frame(&ctx, 宽窗口, |ui| app.ui(ui));
    let 宽屏上 = 画出来的字(&宽的一帧);
    let 宽行: Vec<&str> = 宽屏上.lines().collect();
    let 宽长副行 = 副行(&宽行, "超级机器人大战R");
    assert!(
        宽长副行.starts_with(带根名.as_str()),
        "宽窗口里宽度够，副行该照 Q809 带根名：「{宽长副行}」\n{宽屏上}"
    );
    留下的尾巴(&宽长副行, 长键);
    assert_eq!(
        副行(&宽行, "短"),
        短键.replacen('/', romcat_gui::table::ROOT_SEPARATOR, 1),
        "宽窗口里画得下的短路径不该被截：\n{宽屏上}"
    );
    // 七、**开了行首封面、宽度更紧**（协调人 2026-09-15 审行首封面那两张打回「未关联作品 …s」）：标签后头连
    //     文件名末尾几个字都放不下时只画标签、不画路径碎片；画了路径就至少露出 `PATH_MIN_CHARS` 个字。
    //     标签后头紧跟着的那一段以根名或「…」开头才是路径，否则是平台那一格——那就是没画路径。
    for _ in 0..2 {
        headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    }
    shared::点一下(&ctx, "在每行开头显示封面", |ui| app.ui(ui));
    跑(&ctx, &mut app, 2);
    let 封面一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 封面屏上 = 画出来的字(&封面一帧);
    let 封面行: Vec<&str> = 封面屏上.lines().collect();
    for (正题, 键) in [("超级机器人大战R", 长键), ("短", 短键)] {
        let 标签后头 = 副行(&封面行, 正题);
        if 标签后头.starts_with(带根名.as_str()) || 标签后头.starts_with('…') {
            let 尾巴 = 留下的尾巴(&标签后头, 键);
            assert!(
                尾巴.chars().count() >= romcat_gui::table::PATH_MIN_CHARS,
                "开了行首封面，「{正题}」那一行的路径只露出「{标签后头}」：\n{封面屏上}"
            );
        }
    }
}

#[test]
fn 字体样张开关只在带演示启动的窗口里摆出来() {
    // 拿主意的人 2026-09-14：「字体样张」只在演示/开发构建里出现（挂单 `Q874`）。看的是**运行时**那个标记
    // （`App::mark_demo`，只有程序带 `--demo` 启动时 `main.rs` 才设），不看编译开关——这份测试本身就开着
    // `demo` 特性编，没设标记时屏头上照样不该有它。
    let ctx = headless::context();
    let mut app = shared::小库(
        &[("SFC", "短.zip", shared::档::命中)],
        shared::干净工作目录("romcat-测试-浏览-演示标记"),
    );
    app.show_view(View::Browse);
    跑(&ctx, &mut app, 2);
    // 认的是**正好画着「字体样张」那一段**：状态栏里印着工作目录，目录名带着这几个字就会被「含有」误认。
    let 摆着开关 = |屏上: &str| 屏上.lines().any(|line| line == "字体样张");
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    assert!(
        !摆着开关(&屏上),
        "没带演示启动，屏头上摆出了「字体样张」：\n{屏上}"
    );
    app.mark_demo();
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    assert!(
        摆着开关(&屏上),
        "带演示启动的窗口，屏头上没有「字体样张」：\n{屏上}"
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
    let 目录 = shared::干净工作目录("romcat-测试-浏览-收起");
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

#[test]
fn 管理合集那颗按钮顶在筛选栏右边沿_与同栏右对齐的说明字齐头() {
    // 稿上 `.fpane` 那一行是 `<span class="sec">收藏与合集</span><span class="sp"></span>
    // <button class="btn ghost sm">管理合集…</button>`，`.sp` 是 `flex:1`——一整格空当把按钮
    // 推到这一栏的右边沿。头一版没摆那格空当，按钮跟在标题后头、离右边沿差出七十来个像素。
    //
    // **拿什么当右边沿**：同一栏里「条件组」那一行的说明字是 `section_title` 用同一套办法
    // 右对齐的，它的右沿就是这一栏画字能到的最右处。按钮的字比它再往里缩一份按钮左右留白
    // （字在按钮框里居中，框的右沿才贴着边），所以两边差的正好是那一份留白。
    //
    // 不量图、只量矩形：这一栏里同名的字在表格与右栏里也画着，靠 `一栏::筛选栏` 圈出来。
    let ctx = headless::context();
    let mut app = 界面(200);
    跑(&ctx, &mut app, 2);
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 栏 = 一栏::筛选栏(&ctx, &这一帧);

    let 按钮字 = 栏.正好那一段在哪儿("管理合集…");
    let 右对齐的说明 = 栏.正好那一段在哪儿("存成子库时就是规则");
    let 留白 = romcat_gui::tokens::Tokens::builtin()
        .layout
        .button_small_padding;

    let 差 = 右对齐的说明.right() - 按钮字.right();
    assert!(
        (差 - 留白).abs() <= 1.5,
        "「管理合集…」没顶在筛选栏右边沿：它的右沿 {}，同栏右对齐的说明字右沿 {}，\n\
         差 {差}，按一份按钮留白应当是 {留白}",
        按钮字.right(),
        右对齐的说明.right(),
    );
    // 标题还在它左边，两段字没叠在一起。
    let 标题 = 栏.正好那一段在哪儿("收藏与合集");
    assert!(
        标题.right() < 按钮字.left(),
        "标题 {:?} 与按钮 {:?} 叠上了",
        标题,
        按钮字,
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

    /// **筛选栏**（浏览屏最左那一栏）。
    fn 筛选栏(ctx: &egui::Context, output: &egui::FullOutput) -> Self {
        let 栏 = egui::PanelState::load(ctx, egui::Id::new(romcat_gui::layout::FILTER.id))
            .expect("筛选栏那一栏画过")
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
            .put_media(
                &hash,
                "png",
                bytes.len() as u64,
                romcat_core::scrape::measure::Measured::default(),
            )
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
    // **一个字都不伸出那一栏**：变体那一行印的是这串长键，得截到画得下，
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
    // **这一栏最底下那一块**（收藏与合集），照稿那几段摆开之后一屏装不下：像人一样把指针放在这一栏上往下滚，
    // 滚一下跑一帧，等它画出来为止（不看挂钟）。一路上照旧一个字都不许伸出那一栏。文件表归作品详情页
    // 「变体与文件」那一面（票 `gui-looks-like-the-design/15`），不再垫在这一栏底下。看完滚回顶上，底下那一段
    // 还要看头上那一块。
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
    let mut 到了底 = false;
    for _ in 0..40 {
        if 滚一下(&ctx, &mut app, true)
            .字
            .iter()
            .any(|(text, _)| text.starts_with("收藏与合集"))
        {
            到了底 = true;
            break;
        }
    }
    assert!(到了底, "往下滚到底，最底下那一块（收藏与合集）该露出来");
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
fn 卡片视图可切换并画出作品信息() {
    let ctx = headless::context();
    let mut app = shared::小库(
        &[("SFC", "短.zip", shared::档::没有候选)],
        shared::干净工作目录("romcat-测试-浏览-卡片视图"),
    );
    app.show_view(View::Browse);
    跑(&ctx, &mut app, 2);

    app.browse_and_site().0.show_cards();
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    for 字 in ["短", "SFC", "年份未知", "1 个变体", "没有候选"] {
        assert!(屏上.contains(字), "卡片视图没有「{字}」：\n{屏上}");
    }
}

/// **卡片那一条上并排两组分段开关，各认各的**（票 `gui-looks-like-the-design/13`）。
///
/// 稿上 `.cbar` 里「分组」与「卡片大小」是**两组 `.seg`**（`aria-label="分组"` /
/// `aria-label="卡片大小"`）。照稿改过来那一下当场撞了 egui 的 id：
/// `look::segmented` 从前拿 `(“分段开关”, 下标)` 认每一颗，而**同一个 `Ui` 里摆两组时
/// `ui.id()` 是同一个、下标又都从 0 数起**，于是两组的第 0 颗、第 1 颗各自重号。
///
/// 撞上之后有两件事：屏上画出「First use of Widget ID」那行红字，
/// 而且**按一组的第 0 颗会连着动另一组的第 0 颗**。
///
/// ⚠️ **这条 bug 那一轮所有行为测试都是绿的**——是照着重出的基线图看出来的。
/// 所以这一条两头都断：红字不许出现（最通用的那一头），
/// 以及**按「小」不许把分组关掉**（撞号真会干的那件事：「小」与「不分组」都是第 0 颗）。
#[test]
fn 卡片那一条上两组分段开关互不串号() {
    let ctx = headless::context();
    let mut app = shared::小库(
        &[
            ("SFC", "短.zip", shared::档::命中),
            ("GBA", "另一个.zip", shared::档::命中),
        ],
        shared::干净工作目录("romcat-测试-浏览-两组分段"),
    );
    app.show_view(View::Browse);
    跑(&ctx, &mut app, 2);
    app.browse_and_site().0.show_cards();
    跑(&ctx, &mut app, 2);

    // 一、**屏上不许有 egui 的撞号红字**。它是 egui 自己画上去的，
    // 一行都不该出现在成品屏上。
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    for 红字 in ["Widget ID", "First use", "Second use"] {
        assert!(
            !屏上.contains(红字),
            "屏上出现了 egui 的撞号红字「{红字}」——同一个 `Ui` 里两组分段开关重号了：\n{屏上}"
        );
    }
    // 两组都在屏上（不在的话底下那两步测的是空气）。
    for 一颗 in ["不分组", "按平台", "小", "中", "大"] {
        assert!(屏上.contains(一颗), "卡片那一条上没有「{一颗}」：\n{屏上}");
    }

    // 二、按「按平台」——分组的次序由中立库排，所以它看得见地落在查询上。
    shared::点正好(&ctx, "按平台", |ui| app.ui(ui));
    跑(&ctx, &mut app, 2);
    assert_eq!(
        app.browse().query().order,
        WorkOrder::Platform,
        "按了「按平台」，分组没生效"
    );

    // 三、**按大小那一组的第 0 颗（「小」），分组不许被带着动**。
    // 撞号的时候正是这一下把「不分组」一起按了。
    shared::点正好(&ctx, "小", |ui| app.ui(ui));
    跑(&ctx, &mut app, 2);
    assert_eq!(
        app.browse().query().order,
        WorkOrder::Platform,
        "按了大小那一组的「小」，分组却被带着关掉了——两组分段开关串号了"
    );
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    for 红字 in ["Widget ID", "First use", "Second use"] {
        assert!(!屏上.contains(红字), "按过之后冒出撞号红字：\n{屏上}");
    }
}

/// 一份**认出一个作品、另有两行认不出**的小库：卡片墙上那枚「未关联作品」要的正是这副样子。
///
/// **不走 [`shared::小库`]**：它写的结论一律 `work_id: None`，三行会全落成「未关联作品」，
/// 分不出已关联那一档。这里照它那一套搭，只把 GBA 那一行接到一个真作品上。
fn 一张认出作品两张没有的小库() -> App {
    use romcat_core::catalog::identify::{Candidate, Identification, Provenance};
    use romcat_core::catalog::{Catalog, Confidence, State};
    use romcat_core::platform::Manifest;
    use romcat_core::shape::Variant;
    use romcat_core::site::Site;
    use romcat_core::verdict::Store;

    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    romcat_core::catalog::roots::add_root(
        &catalog,
        None,
        shared::根,
        std::path::Path::new("/主库"),
    )
    .expect("建得出根");
    let 认出的 = shared::变体("GBA", 长路径);
    let 没候选的 = shared::变体("SFC", "短.zip");
    let 还没识别的 = shared::变体("FC", "另一个.nes");
    catalog
        .replace_variants(
            &[认出的.clone(), 没候选的.clone(), 还没识别的],
            1,
            &Manifest::default(),
        )
        .expect("写得进变体");
    let work = catalog
        .add_work(shared::候选作品, Provenance::Identified)
        .expect("建得出作品");
    let 一条 =
        |variant: &Variant, state: State, work_id, candidates: Vec<Candidate>| Identification {
            variant_key: variant.key.clone(),
            platform: None,
            standalone: None,
            edition: None,
            state,
            reason: None,
            units: 1,
            nkit: 0,
            read_bytes: 0,
            work_id,
            release_id: None,
            candidates,
        };
    catalog
        .write_identifications(&[
            一条(
                &认出的,
                State::Matched,
                Some(work),
                vec![shared::候选(&认出的, true, Confidence::High)],
            ),
            一条(&没候选的, State::Unmatched, None, Vec::new()),
            // 「还没识别」那一行**结论表里一行都不写**（`shared::档::还没识别`）。
        ])
        .expect("写得进结论");
    let store = Store::in_memory().expect("开得出沉淀库");
    let mut app = App::new(
        Site::in_memory(catalog, store, shared::根),
        shared::干净工作目录("romcat-测试-浏览-卡片未关联标签"),
    );
    app.show_view(View::Browse);
    app
}

#[test]
fn 卡片墙上认不出作品的那几张挂着未关联作品标签_认出的不挂() {
    // **卡面也挂那枚标签**（稿上没画，拿主意的人 2026-09-20 定）。
    //
    // 这一条**认的是卡片**，不是拿表格那把尺子来量：截图门里 `带标签的行正题露得出字`
    // 查的是表格那两行怎么截断，而卡片墙上根本没有行。这里问的是「哪一张卡面上有那枚标签」——
    // 标签摆在卡面下半截那一行的最左边，于是与那张卡的标题**左沿对齐**、就落在它下头
    // `card-info-height` 那一截里。
    let ctx = headless::context();
    let mut app = 一张认出作品两张没有的小库();
    跑(&ctx, &mut app, 2);
    app.browse_and_site().0.show_cards();
    跑(&ctx, &mut app, 2);

    let 中 = 一栏::正中(
        &ctx,
        &headless::frame(&ctx, headless::input(), |ui| app.ui(ui)),
    );
    // 同一张卡上标题画**两遍**（封面里一遍、卡面下半截一遍）；标签跟的是下面那一遍。
    let 卡面标题 = |名字: &str| -> egui::Rect {
        中.字
            .iter()
            .filter(|(text, _)| text == 名字)
            .map(|(_, rect)| *rect)
            .max_by(|甲, 乙| 甲.min.y.total_cmp(&乙.min.y))
            .unwrap_or_else(|| panic!("卡片墙上没有「{名字}」这张卡：\n{}", 中.全文()))
    };
    let 令牌 = romcat_gui::tokens::Tokens::builtin();
    let (下半截, 留白) = (令牌.layout.card_info_height, 令牌.layout.tag_padding);
    let 挂着标签 = |名字: &str| -> bool {
        let 标题 = 卡面标题(名字);
        中.字.iter().any(|(text, rect)| {
            // 收上来的是**那几个字**画在哪儿，而标签的底色比字再往左一份 `tag-padding`
            // （`table::tag`）——底色的左沿才是与卡面标题对齐的那一条。
            text == romcat_gui::table::UNLINKED_LABEL
                && (rect.min.x - 留白 - 标题.min.x).abs() <= 1.0
                && rect.min.y >= 标题.min.y
                && rect.max.y <= 标题.min.y + 下半截
        })
    };

    for 名字 in ["短", "另一个"] {
        assert!(
            挂着标签(名字),
            "认不出作品的那张卡「{名字}」该挂着「{}」：\n{}",
            romcat_gui::table::UNLINKED_LABEL,
            中.全文(),
        );
    }
    assert!(
        !挂着标签(shared::候选作品),
        "认出了作品的那张卡「{}」不该挂「{}」：\n{}",
        shared::候选作品,
        romcat_gui::table::UNLINKED_LABEL,
        中.全文(),
    );
    // 一共就画两枚——多一枚说明判据跑到别的卡上去了。
    let 几枚 = 中
        .字
        .iter()
        .filter(|(text, _)| *text == romcat_gui::table::UNLINKED_LABEL)
        .count();
    assert_eq!(几枚, 2, "卡片墙上该正好两枚标签：\n{}", 中.全文());
}

#[test]
fn 卡片工具条显示覆盖率并能改排序() {
    let ctx = headless::context();
    let (mut app, _dir) = 一个有封面一个没有的小库();
    app.browse_and_site().0.show_cards();
    跑(&ctx, &mut app, 3);

    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    for 字 in ["排序", "默认", "有封面 1 / 2"] {
        assert!(屏上.contains(字), "卡片工具条没有「{字}」：\n{屏上}");
    }

    // 分组时「默认」仍然可点；选具体字段会取消分组，交给中立库按那个字段重排。
    //
    // **按整段一字不差点**（`点正好`）：左栏那句「搜索结果默认按匹配程度排序……」里也有
    // 「默认」两个字，按「含有」找头一处会点到那句话上，下拉压根打不开。
    shared::点正好(&ctx, "默认", |ui| app.ui(ui));
    shared::点正好(&ctx, "容量", |ui| app.ui(ui));
    assert_eq!(
        app.browse().query().order,
        romcat_core::catalog::browse::WorkOrder::Bytes,
        "卡片工具条的排序没有同步进查询"
    );
}

/// **卡片下拉上「默认」与「名称」是两档**（票 `gui-looks-like-the-design/12`，挂单
/// `Q1098`，拿主意的人 2026-09-21 定照稿拆回两个）。
///
/// 票 09 把两档合成了一个（`Name => "默认"`），于是从表头倒着排过来的 `(作品, 倒着)`
/// 在这个下拉上照样显示「默认」——**而那一刻库里排的并不是默认那一种**，
/// 搜索着的时候它也不再按匹配质量排（票 `11` 的 `WorkQuery::sorted_by_default`）。
#[test]
fn 卡片下拉上默认与名称是两档_倒着排时不再显示默认() {
    use romcat_core::catalog::browse::WorkOrder;

    let ctx = headless::context();
    let (mut app, _dir) = 一个有封面一个没有的小库();
    app.browse_and_site().0.show_cards();
    跑(&ctx, &mut app, 3);

    // 一进来是默认那一种排法：下拉上写「默认」。
    //
    // **按整行比，不用「含有」**：左栏那句「搜索结果默认按匹配程度排序……」里也有这两个
    // 字，按含有找的话这条测试在任何实现上都是绿的（第一版就这么假绿过一次）。
    let 下拉上写着 = |屏上: &str, 字: &str| 屏上.lines().any(|line| line == 字);
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    assert!(
        下拉上写着(&屏上, "默认"),
        "默认那一种排法下，卡片下拉该写「默认」：\n{屏上}"
    );

    // **把排法换成「作品、倒着」**——那正是从表头点过来的那个状态。
    {
        let query = app.browse_and_site().0.query_mut();
        query.order = WorkOrder::Name;
        query.descending = true;
    }
    跑(&ctx, &mut app, 2);
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    assert!(
        !app.browse().query().sorted_by_default(),
        "这个状态本来就不是默认那一种，不然这条测试验不到要害"
    );
    assert!(
        下拉上写着(&屏上, "名称"),
        "按作品倒着排时，卡片下拉该写「名称」：\n{屏上}"
    );
    assert!(
        !下拉上写着(&屏上, "默认"),
        "按作品倒着排，下拉上却还写着「默认」——那一刻库里排的并不是默认那一种：\n{屏上}"
    );

    // **选「默认」把方向也按回去**，否则它与「名称」是同一个状态、两档就白拆了。
    // 先点开下拉（那时它写着「名称」），再点那一档。
    shared::点正好(&ctx, "名称", |ui| app.ui(ui));
    shared::点正好(&ctx, "默认", |ui| app.ui(ui));
    assert!(
        app.browse().query().sorted_by_default(),
        "选了「默认」之后该回到默认那一种排法（列与方向都回）"
    );
}

#[test]
fn 卡片视图选择记在工作目录而不进中立库() {
    let ctx = headless::context();
    let workspace = shared::干净工作目录("romcat-测试-浏览-卡片偏好");
    let mut first = shared::小库(
        &[("SFC", "短.zip", shared::档::没有候选)],
        workspace.clone(),
    );
    first.show_view(View::Browse);
    first.browse_and_site().0.show_cards();
    shared::跑一帧(&ctx, |ui| first.ui(ui));
    let second = shared::小库(&[("SFC", "短.zip", shared::档::没有候选)], workspace);
    assert!(second.layout().render().contains("视图·浏览视图 = 卡片"));
}

#[test]
fn 筛不出东西时说清楚并给一颗清除筛选_按下去表就回来() {
    // **空态那句带着条件数**（照稿，挂单 `Q806`）：这一趟摊开了非游戏资产、又筛了一个
    // 平台，按定下来的口径就是 **2 个**（那颗开关算一个、分面算一个）。
    const 空态: &str = "没有符合当前 2 个筛选条件的作品。";

    let ctx = headless::context();
    let mut app = shared::小库(
        &[("SFC", "短.zip", shared::档::命中)],
        shared::干净工作目录("romcat-测试-浏览-空态"),
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

/// **拆掉底栏之后**（票 `gui-looks-like-the-design/15` 收挂单 `Q804`）：改元数据挪进了作品详情页，浏览屏底下那块编辑
/// 面板没了；选中了多少挪进表格上方那一条（设计稿 `.tbar` 的 `#w-picked` 与 `#clear-pick`），「清除选择」一按就清。
#[test]
fn 勾了几行时表格上方那一条写着已选几个作品_清除选择一按就清_底下不再有编辑面板() {
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
    跑(&ctx, &mut app, 2);
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    assert!(
        屏上.lines().any(|line| line.starts_with("已选 3 个作品")),
        "表格上方那一条没写选中了几个作品：\n{屏上}"
    );
    assert!(
        屏上.lines().any(|line| line == "清除选择"),
        "勾了几行却没有「清除选择」：\n{屏上}"
    );
    assert!(
        egui::PanelState::load(&ctx, egui::Id::new("浏览编辑")).is_none(),
        "浏览屏底下那块编辑面板还画着"
    );

    let 屏上 = shared::点一下(&ctx, "清除选择", |ui| app.ui(ui));
    assert!(
        app.browse().picked().is_empty(app.window().total()),
        "按了「清除选择」选中的那几行还在"
    );
    assert!(
        !屏上.lines().any(|line| line == "清除选择"),
        "一行都没选了，「清除选择」还摆着：\n{屏上}"
    );
}

/// 这一帧里**文字里含着**这几个字的每一段画在哪儿（外框）。
fn 含着这几个字的每一段(
    output: &egui::FullOutput,
    那几个字: &str,
) -> Vec<(String, egui::Rect)> {
    fn 找(shape: &egui::epaint::Shape, 那几个字: &str, out: &mut Vec<(String, egui::Rect)>) {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text().contains(那几个字) => {
                out.push((
                    text.galley.text().to_owned(),
                    egui::Rect::from_min_size(text.pos, text.galley.size()),
                ));
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

/// **「N 个作品（共 M）」固定在表格上方那一条的头一行，控件与帮助自己另起一行**
/// （协调人 2026-09-15 定，岔路口 1 选 C）。
///
/// 票 `gui-looks-like-the-design/10` 把这一条重排成两层：头一层只有视图切换与这个数，第二层才是当前视图的
/// 控件与帮助。要守的是同一件事——**这个数不许被挤到第二行**——所以这里认头一层的「表格」与第二层的帮助。
///
/// ⚠️ **「右端」那半句 2026-09-22 起不作数了**（挂单 `Q1102`）：那几颗批量操作照稿挪回
/// 这一条的右端之后，这个数照稿挪到了**视图切换紧后头**（稿上 `.seg` 之后就是 `.cnt`）。
/// 09-15 那条裁定护的是「不许掉到第二行」，那一半照旧守着；「在右端」那一半被稿推翻了。
/// 所以这里**连它在那一组左边一起断**——不然这个数哪天又飘回右端也没人拦。
#[test]
fn 表格上方那一条的作品数摆在头一行_紧跟视图切换_帮助自己折行() {
    let ctx = headless::context();
    let mut app = 界面(2_000);
    跑(&ctx, &mut app, 3);
    let 这一帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 表格 = 含着这几个字的每一段(&这一帧, "表格")
        .into_iter()
        .find(|(text, _)| text == "表格")
        .map(|(_, rect)| rect)
        .expect("表格上方那一条画着视图切换的「表格」");
    let (数那一句, 数) = 含着这几个字的每一段(&这一帧, "个作品（共")
        .into_iter()
        .next()
        .expect("表格上方那一条画着作品数那一句");
    let (_, 帮助) = 含着这几个字的每一段(&这一帧, "双击一行打开作品详情")
        .into_iter()
        .next()
        .expect("表格上方那一条画着帮助");
    assert!(
        (数.center().y - 表格.center().y).abs() <= 4.0,
        "「{数那一句}」没摆在头一行：视图切换 {表格:?}，那一句 {数:?}"
    );
    assert!(
        数.min.x > 表格.max.x,
        "「{数那一句}」没摆在视图切换右边：视图切换 {表格:?}，那一句 {数:?}"
    );
    assert!(
        帮助.min.y > 数.max.y,
        "帮助那句没有另起一行：那一句 {数:?}，帮助 {帮助:?}"
    );
    // **这个数在那一组批量操作之前**（稿上 `.cnt` 在 `.acts` 之前，挂单 `Q1102`）。
    //
    // **「之前」要分两种情形量**：那一组摆得下时与这个数同一行，比的是 x；摆不下时整组
    // 换到第二行（票 `13` 把那一组填到四颗之后，两栏摊开着就是这一档），那时比的是 y。
    // 只比 x 的话，换行之后这条断言会拿第二行的 x 去比第一行的 x——而那没有意义。
    let (_, 头一颗) = 含着这几个字的每一段(&这一帧, "刮削…")
        .into_iter()
        .next()
        .expect("那一条画着批量操作那一组");
    let 同一行 = (头一颗.center().y - 数.center().y).abs() <= 4.0;
    assert!(
        if 同一行 {
            数.max.x < 头一颗.min.x
        } else {
            数.max.y <= 头一颗.min.y
        },
        "作品数跑到那一组批量操作后头去了：那一句 {数:?}，头一颗 {头一颗:?}（同一行：{同一行}）"
    );
}

/// **收起后那根窄条与筛空时那句空态，印的是同一个条件数**
/// （票 `gui-looks-like-the-design/12`，挂单 `Q806`；口径是拿主意的人 2026-09-21 定的）。
///
/// 两处各数一遍的话，屏上会说出两个数——所以数在核心库数**一次**
/// （`WorkQuery::filter_count`），这两处都问它（ADR-0024）。
///
/// 夹具**三样各有一点**，正是口径里要加起来的那三样：一个分面（平台）、条件组里一条
/// **生效的**子句、以及非游戏资产那颗开关。外加一个**不该算**的搜索词——它管顺序不管
/// 集合，算进来就是在屏上说搜索缩小了这一批。
#[test]
fn 收起后的窄条与筛空时的空态印的是同一个条件数() {
    use romcat_core::catalog::browse::NonGameAssets;
    use romcat_core::sublibrary::Rule;

    let ctx = headless::context();
    let mut app = shared::小库(
        &[("SFC", "短.zip", shared::档::命中)],
        shared::干净工作目录("romcat-测试-浏览-条件数"),
    );
    app.show_view(View::Browse);
    跑(&ctx, &mut app, 3);

    {
        let query = app.browse_and_site().0.query_mut();
        // 一个分面：库里没有 FC，于是这一趟一行都不剩、空态画得出来。
        query.platform = Some(PlatformFilter::from_label("FC"));
        // 一条生效的子句。
        query.rule = Some(Rule::parse("年份>=1990").expect("读得懂"));
        // 那颗开关。
        query.non_game_assets = NonGameAssets::Listed;
        // **不该算的那一样。**
        query.search = "口袋".to_string();
        assert_eq!(query.filter_count(), 3, "口径：两样加一条子句，搜索词不算");
    }
    跑(&ctx, &mut app, 2);

    // 一、空态那句。
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    assert_eq!(app.window().total(), 0, "这个筛选下该一行都不剩");
    assert!(
        屏上.contains("没有符合当前 3 个筛选条件的作品。"),
        "空态那句没写条件数、或者数得不对：\n{屏上}"
    );

    // 二、把左栏收起来，窄条上竖着写的是同一个数（照稿 `.fstrip`「筛选 · 3 个条件」）。
    let 屏上 = 点这半边的(&ctx, "«", true, |ui| app.ui(ui));
    assert!(
        屏上.contains("筛\n选\n·\n3\n个\n条\n件"),
        "收起后那根窄条上没竖着写「筛选·3个条件」：\n{屏上}"
    );
}

/// 一帧**够高、读得到左栏底下那几句话**的输入。
///
/// **为什么不用 [`headless::input`] 那个 1280×800**：左栏是一个 `ScrollArea`，而 egui 的
/// `Label` 滚出视口就**不画**（不是画了被裁掉）——屏上那份字里于是一个字都不剩。左栏这半年
/// 一直在长（票 17 加了「整理建议」一簇，这一票在「收藏与合集」那一行加了颗按钮），
/// 量下来：加按钮之前那几句话正好卡在 800 上**一个像素都不剩**，加了之后要 810。
///
/// 于是这儿把窗口放高。**这条测试要钉的是屏上说了什么，不是默认窗口恰好差几个像素**
/// ——拿 800 去钉，等于让下一个往左栏加东西的人替这条 12 票的测试背账。
/// 「默认窗口下那句警告在不在折线以下」是另一件事，记在挂单 `Q1104` 里。
fn 够高的一帧() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(headless::VIEWPORT[0], 1200.0),
        )),
        ..Default::default()
    }
}

/// **筛不出东西的子句：屏上逐条点名，但一条都不拦**
/// （票 `gui-looks-like-the-design/12`）。
///
/// 三档各来一条：认不出的平台名、还没建的合集、以及**评分**那一维（立着但眼下没有源）。
/// 判在核心库一处（`sublibrary::thin`），这一层只把它印出来（ADR-0024）。
///
/// **与「还有 N 条没生效」是两件事**：那一段说的是**没填完或者填错了**的，它们不进规则；
/// 这一段说的是**读得成、也进了规则**、只是眼下一个变体都选不中的。把后者也拦下来是错的
/// ——合集可以是待会儿才建的，平台清单也会长。
#[test]
fn 筛不出东西的子句屏上逐条点名但不拦着() {
    let ctx = headless::context();
    // **用小库不用合成数据**：合成数据那份左栏长得多（平台与语言各一大簇），
    // 条件组那一段会被 `ScrollArea` 剔到视口外，屏上根本读不到那几句话。
    let mut app = shared::小库(
        &[("SFC", "短.zip", shared::档::命中)],
        shared::干净工作目录("romcat-测试-浏览-筛不出东西"),
    );
    app.show_view(View::Browse);
    跑(&ctx, &mut app, 3);

    let text = "平台=没这个平台 且 合集=还没建的 且 评分>=0.8";
    let rule = romcat_core::sublibrary::Rule::parse(text).expect("读得懂");
    app.browse_and_site().0.set_filter_rule(Some(rule.clone()));
    跑(&ctx, &mut app, 2);
    let 屏上 = shared::画出来的字(&headless::frame(&ctx, 够高的一帧(), |ui| app.ui(ui)));

    // 一、三条都点到名，而且数目写出来。
    assert!(
        屏上.contains("有 3 条筛不出东西："),
        "屏上没说有几条筛不出东西：\n{屏上}"
    );
    for 该说的 in ["没这个平台", "还没建的", "评分"] {
        assert!(
            屏上.contains(该说的),
            "「{该说的}」那一条没被点名：\n{屏上}"
        );
    }

    // 二、**一条都没被拦**：三条照样进了规则，屏上那行规则原文一字不少。
    assert_eq!(
        app.browse()
            .query()
            .rule
            .as_ref()
            .map(|one| one.text.clone()),
        Some(text.to_string()),
        "筛不出东西的子句被悄悄扔掉了——那会让存出去的子库比屏上说的宽",
    );

    // 三、**它不是「没生效」**：那一段说的是没填完或填错的，这一趟一条都没有。
    assert!(
        !屏上.contains("还有 1 条没生效") && !屏上.contains("还有 3 条没生效"),
        "把「筛不出东西」说成了「没生效」，两件事混了：\n{屏上}"
    );
}

/// **搜索框底下那句排序说明，只在搜索框里真有字时才画**
/// （挂单 `Q1099`，拿主意的人 2026-09-21 定）。
///
/// 两层道理：**没搜的时候它是废话**（「搜索结果默认按匹配程度排序」——可还没搜），
/// 而且它**要占两行**（这一句得连「默认」一起说，票 `11` 改了口径），常驻的话左栏最底下
/// 「语言」那一段会被一路顶出视口。
///
/// **两个方向各断一条**：只断「搜索着的时候有」的话，一份「永远显示」的实现照样全绿，
/// 而那正是这一条要防的。
#[test]
fn 排序那句说明只在搜索着的时候才出现() {
    const 那句话: &str = "搜索结果默认按匹配程度排序";

    let ctx = headless::context();
    let mut app = shared::小库(
        &[("SFC", "短.zip", shared::档::命中)],
        shared::干净工作目录("romcat-测试-浏览-排序说明"),
    );
    app.show_view(View::Browse);
    跑(&ctx, &mut app, 3);

    // 一、**没搜索：屏上没有这句话**。
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    assert!(
        !屏上.contains(那句话),
        "还没搜就把「{那句话}」挂在屏上了——那句话此刻不成立：\n{屏上}"
    );

    // 二、**打了字：屏上有**。
    app.browse_and_site().0.query_mut().search = "幻想".to_string();
    跑(&ctx, &mut app, 2);
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    assert!(
        屏上.contains(那句话),
        "搜索着的时候没说清结果是按什么排的：\n{屏上}"
    );

    // 三、**把字删干净，它跟着收回去**——只留空白也算没搜。
    app.browse_and_site().0.query_mut().search = "   ".to_string();
    跑(&ctx, &mut app, 2);
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    assert!(
        !屏上.contains(那句话),
        "搜索词清空之后那句话没收回去：\n{屏上}"
    );
}

/// **表格上方那一组批量操作：整组靠右、组内不拆散、摆不下整组换一行**
/// （设计稿 `.tbar{flex-wrap:wrap}` 加 `.acts{margin-left:auto;flex-wrap:nowrap}`；
/// 拿主意的人 2026-09-22 对着稿裁的，挂单 `Q1102`）。
///
/// **两种宽窄各验一遍**，这是要害：
///
/// - **没勾行**时左边只有视图切换与作品数，那一组摆得下，落在**同一行的右头**；
/// - **勾了行**之后作品数变长（「已选 N 个作品 · M 个变体」）、还多出一颗「清除选择」，
///   左边挤占了宽度，那一组**整组换到第二行**、在那一行里照旧靠右。
///
/// 头一版只画了一帧默认布局就断言，**换行那条路一次都没走到**——而那条路恰恰是写错的
/// （拿 `add_space` 想逼 `horizontal_wrapped` 折行，实际只是把光标推过右沿，三颗按钮
/// 连画都没画出来）。抓到它的是合并向导那条测试，不是这一条。
///
/// 票 `gui-looks-like-the-design/10` 栽的是「**字画着、点不动**」，所以这里除了位置还
/// **真按一下**。
#[test]
fn 批量那一组整组靠右_摆不下就整组换行_不拆散也点得中() {
    /// 照稿的次序，**五颗齐了**：「加入合集…」票 13 补的（挂单 `Q1102`）、
    /// 「加入子库…」票 23 补的（最右那颗，稿上唯一的主按钮）。
    ///
    /// ⚠️ **这份清单漏一颗，这条测试就在量错的那一颗**：它拿「最后一颗的右沿」
    /// 判整组靠没靠右，清单短一截就会拿倒数第二颗去量，于是**整组明明靠右也判红**。
    /// 票 23 那一趟正是这么红的（清单还停在四颗），而票 12 那一趟也栽过同一下
    /// （acts 从三颗填到四颗）。**往 `ACTIONS` 里添一颗，这儿要跟着添。**
    const 照稿次序: [&str; 5] = ["刮削…", "★ 收藏", "加入合集…", "合并作品…", "加入子库…"];

    let ctx = headless::context();
    let mut app = 界面(ROWS);
    跑(&ctx, &mut app, 3);

    /// 那几颗各画在哪儿，按屏上从左到右排好。
    fn 那一组(帧: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
        let mut 几颗: Vec<(String, egui::Rect)> = 照稿次序
            .iter()
            .map(|字| {
                含着这几个字的每一段(帧, 字)
                    .into_iter()
                    .find(|(text, _)| text == 字)
                    .unwrap_or_else(|| panic!("屏上没有「{字}」这颗按钮：\n{}", 画出来的字(帧)))
            })
            .collect();
        几颗.sort_by(|(_, a), (_, b)| a.min.x.total_cmp(&b.min.x));
        几颗
    }

    /// 这一组**整组不拆散**：三颗在同一行上、次序照稿。
    fn 断整组不拆散(几颗: &[(String, egui::Rect)]) {
        let 行 = 几颗[0].1.center().y;
        for (字, rect) in 几颗 {
            assert!(
                (rect.center().y - 行).abs() <= 2.0,
                "「{字}」没和这一组其余几颗在同一行上（{rect:?}，这一行 y≈{行}）",
            );
        }
        let 屏上次序: Vec<&str> = 几颗.iter().map(|(字, _)| 字.as_str()).collect();
        assert_eq!(屏上次序, 照稿次序, "这一组的次序与稿上不一样");
    }

    /// 中间那一栏的**右边界**：右栏摊开着就是它那块面板的左沿，收起来了就是视口右缘
    /// 减去那根窄条。按钮越过它就是盖到右栏上了。
    fn 中栏右边界(帧: &egui::FullOutput) -> f32 {
        含着这几个字的每一段(帧, "点主列表里的一行")
            .into_iter()
            .next()
            .map_or_else(
                || headless::VIEWPORT[0] - romcat_gui::tokens::Tokens::builtin().layout.strip_width,
                |(_, rect)| rect.min.x,
            )
    }

    // ── 一、**把两栏收起来**：中间那一栏宽了，四颗摆得下，落在同一行的右头 ──
    //
    // 两栏摊开时中间只有五百多点，而这一组是**五颗 386 点**——**摆不下才是常态**
    // （票 23 量过：留得出 321，差 65；`ACTIONS` 的文档里有整笔账）。
    // 所以「摆得下」那条路得先把地方腾出来，不然这一条永远只验到换行那一支。
    romcat_gui::layout::FILTER.set_collapsed(&ctx, true);
    romcat_gui::layout::DETAIL.set_collapsed(&ctx, true);
    跑(&ctx, &mut app, 2);
    let 帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 几颗 = 那一组(&帧);
    断整组不拆散(&几颗);
    let (_, 作品数) = 含着这几个字的每一段(&帧, "个作品（共")
        .into_iter()
        .next()
        .expect("那一条上画着作品数");
    assert!(
        (几颗[0].1.center().y - 作品数.center().y).abs() <= 4.0,
        "摆得下的时候这一组该和作品数同一行：那一组 {:?}，作品数 {作品数:?}",
        几颗[0].1,
    );
    // **靠右**：最后一颗的右沿**贴着中间那一栏的右边界**（稿上 `.acts` 的
    // `margin-left:auto`）。这么断比「与作品数之间空多少」稳——那个间隙随作品数那一句
    // 多长而变，库一换就不作数了；贴不贴右沿是「有没有靠右」本身。
    let 贴右沿 = |帧: &egui::FullOutput, 几颗: &[(String, egui::Rect)]| {
        中栏右边界(帧) - 几颗[几颗.len() - 1].1.max.x
    };
    assert!(
        贴右沿(&帧, &几颗) < 60.0,
        "这一组没靠右：最后一颗右沿离中间那一栏的右边界还有 {}",
        贴右沿(&帧, &几颗),
    );
    for (字, rect) in &几颗 {
        assert!(
            rect.max.x < 中栏右边界(&帧),
            "「{字}」越过了中间那一栏的右边界（{rect:?}，右边界 {}）",
            中栏右边界(&帧),
        );
    }

    // ── 二、**两栏摊开**：中间那一栏窄回来，这一组整组换到第二行 ────────────
    romcat_gui::layout::FILTER.set_collapsed(&ctx, false);
    romcat_gui::layout::DETAIL.set_collapsed(&ctx, false);
    // 顺带勾一行：作品数变成「已选 N 个作品 · M 个变体」、还多出一颗「清除选择」，
    // 左边更挤——换行那条路走得更实。
    {
        let (browse, site) = app.browse_and_site();
        let anchor = site
            .catalog
            .work_page(browse.query(), 0, 1)
            .expect("取得出一行")
            .remove(0)
            .anchor;
        browse.picked_mut().toggle(&anchor);
    }
    跑(&ctx, &mut app, 2);
    let 帧 = headless::frame(&ctx, headless::input(), |ui| app.ui(ui));
    let 几颗 = 那一组(&帧);
    断整组不拆散(&几颗);
    let (那一句, 已选) = 含着这几个字的每一段(&帧, "已选")
        .into_iter()
        .next()
        .expect("勾了行之后那一条上画着「已选 …」");
    assert!(
        几颗[0].1.min.y > 已选.max.y,
        "左边挤满之后这一组没换到第二行：「{那一句}」{已选:?}，头一颗 {:?}",
        几颗[0].1,
    );
    // 换了行照旧**靠右**：同一条尺子量第二行。
    assert!(
        贴右沿(&帧, &几颗) < 60.0,
        "换行之后这一组没靠右：最后一颗右沿离右边界还有 {}",
        贴右沿(&帧, &几颗),
    );
    for (字, rect) in &几颗 {
        assert!(
            rect.max.x < 中栏右边界(&帧),
            "换行之后「{字}」越过了中间那一栏的右边界（{rect:?}）",
        );
    }

    // ── 三、**点得中**：票 10 栽的正是「字画着、点不动」 ───────────────────
    let 屏上 = shared::点正好(&ctx, "刮削…", |ui| app.ui(ui));
    assert!(
        屏上.contains("刮削"),
        "按了「刮削…」什么都没发生——多半是它画在行外，点不中：\n{屏上}"
    );
}

/// 按一下**弹层里**正好写着 `这几个字` 的那一处，接着（`字` 非空时）打进去几个字。
///
/// ⚠️ **弹层的图形画在最后**（它盖在整屏上头），所以「屏上正好写着这几个字的头一处」
/// 找到的是**底下那一屏**的那一处，不是弹层里的。`shared::点正好` 与 `shared::打字`
/// 取的都是头一处——真栽过一次：「管理合集」里那个改名输入框预填着合集名，而左栏
/// 分面上也有同一个名字，于是那一下点到了左栏的分面标签，字打进了空处，
/// 改名于是「把『通关过的』改成『通关过的』」。
///
/// 这儿取**最后一处**：弹层在最上面那一层。
fn 点弹层里的(
    ctx: &egui::Context,
    这几个字: &str,
    字: &str,
    mut 画一帧: impl FnMut(&mut egui::Ui),
) -> String {
    fn 收(shape: &egui::epaint::Shape, 这几个字: &str, out: &mut Vec<egui::Pos2>) {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == 这几个字 => {
                out.push(egui::Rect::from_min_size(text.pos, text.galley.size()).center());
            }
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    收(one, 这几个字, out);
                }
            }
            _ => {}
        }
    }
    let 头一帧 = headless::frame(ctx, headless::input(), &mut 画一帧);
    let mut 处处 = Vec::new();
    for clipped in &头一帧.shapes {
        收(&clipped.shape, 这几个字, &mut 处处);
    }
    let 位置 = *处处.last().unwrap_or_else(|| {
        panic!(
            "弹层里没有正好写着「{这几个字}」的地方：\n{}",
            画出来的字(&头一帧)
        )
    });
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
    if !字.is_empty() {
        headless::frame(
            ctx,
            shared::输入(vec![egui::Event::Text(字.to_string())]),
            &mut 画一帧,
        );
    }
    shared::跑一帧(ctx, 画一帧)
}

/// **「管理合集」那个弹层：改名走核心库那一趟，屏上照实印回执**
/// （票 `gui-looks-like-the-design/13`，设计稿 `DLG.coll`）。
///
/// **收藏那一行照稿写明改不动也删不掉**——ADR-0005 那条「不禁按钮」在这儿的样子是
/// 那一行本来就没有那两颗，而屏上说得出为什么。
#[test]
fn 管理合集那一层改得动名字_收藏那一行写明改不动() {
    let ctx = headless::context();
    let mut app = shared::小库(
        &[("SFC", "短.zip", shared::档::命中)],
        shared::干净工作目录("romcat-测试-浏览-管理合集"),
    );
    app.show_view(View::Browse);
    跑(&ctx, &mut app, 3);

    // 先有一个自建合集：直接走核心库那条路铺好现场。
    {
        let (browse, site) = app.browse_and_site();
        let keys = site
            .catalog
            .variant_page(&romcat_core::catalog::browse::VariantQuery::default(), 0, 8)
            .expect("取得出变体")
            .into_iter()
            .map(|row| row.key)
            .collect::<Vec<_>>();
        romcat_core::collection::add(site, "通关过的", &keys).expect("加得进");
        browse.invalidate(site);
    }
    跑(&ctx, &mut app, 3);

    // 一、左栏抬头那颗「管理合集…」把弹层摊开。
    shared::点正好(&ctx, "管理合集…", |ui| app.ui(ui));
    跑(&ctx, &mut app, 2);
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    assert!(
        屏上.contains("合集是你自己起名的一组作品"),
        "「管理合集」那一层没摊开：\n{屏上}"
    );
    // **收藏那一行照稿写明它改不动也删不掉**。
    assert!(
        屏上.contains("默认的一组，不能改名或删除"),
        "收藏那一行没写明改不动：\n{屏上}"
    );
    assert!(屏上.contains("通关过的"), "那个自建合集没列出来：\n{屏上}");

    // 二、按「改名」、打上新名字、按「保存」。
    点弹层里的(&ctx, "改名", "", |ui| app.ui(ui));
    // 框里预填着原名；**把光标挪到末尾再打**，不然字插在点到的那一处中间。
    点弹层里的(&ctx, "通关过的", "", |ui| app.ui(ui));
    headless::frame(
        &ctx,
        shared::输入(vec![
            shared::按键事件(egui::Key::End),
            egui::Event::Text("·改".to_string()),
        ]),
        |ui| app.ui(ui),
    );
    let 屏上 = 点弹层里的(&ctx, "保存", "", |ui| app.ui(ui));

    // 三、**回执照实说**：改了几条成员关系、有没有子库规则跟着改。
    assert!(
        屏上.contains("改叫") && 屏上.contains("成员关系"),
        "改完没说清动了什么：\n{屏上}"
    );
    // 四、**库里真的改了**：新名字在分面上，旧名字不在了。
    let 分面: Vec<String> = app
        .browse()
        .facets()
        .collections
        .iter()
        .map(|一个| 一个.value.clone())
        .collect();
    assert!(
        分面.iter().any(|一个| 一个 == "通关过的·改"),
        "改完分面上没有新名字：{分面:?}\n屏上：\n{屏上}\n回执：{:?}／{:?}",
        app.browse().notice(),
        app.browse().error(),
    );
    assert!(
        !分面.iter().any(|一个| 一个 == "通关过的"),
        "旧名字还在分面上：{分面:?}"
    );
}

/// **筛选筛不着的合集，照样列得出、改得动、删得掉**
/// （挂单 `Q1109`，拿主意的人 2026-09-22 裁）。
///
/// 「管理合集」那一层从前列的是左栏那份**分面**，而分面走的是中立库里的投影
/// （`JOIN collection_variant`）、还跟着「列出非游戏资产」那颗开关走。于是两种情形下
/// 一个合集会从这一层里**整个消失**——成员全是路径锚而那些文件眼下不在库里
/// （换了根、或删了根还没重扫），或者成员全是非游戏资产而那颗开关收着。
/// 消失之后既改不了名也删不掉，**而核心库那两条路本来是按沉淀库办的，能力一直在**。
///
/// 这一条造的是头一种：往沉淀库里记一条**库里没有对应变体**的成员关系。
/// 它在分面上一定不出现（投影里没有它），而弹层里必须出现。
#[test]
fn 筛不着的合集在管理合集里照样列得出() {
    use romcat_core::verdict::{Anchor, Membership};

    let ctx = headless::context();
    let mut app = shared::小库(
        &[("SFC", "短.zip", shared::档::命中)],
        shared::干净工作目录("romcat-测试-浏览-筛不着的合集"),
    );
    app.show_view(View::Browse);
    跑(&ctx, &mut app, 3);

    // 往沉淀库里记一条锚——**故意挑一份库里没有的内容**，于是投影落不到任何变体。
    const 名字: &str = "换过根的那一批";
    {
        let (browse, site) = app.browse_and_site();
        site.store
            .join(&[Membership::now(
                名字,
                Anchor::Content {
                    crc32: 0xDEAD_BEEF,
                    size: 1_234_567,
                    sha1: None,
                },
            )])
            .expect("记得进沉淀库");
        browse.invalidate(site);
    }
    跑(&ctx, &mut app, 3);

    // 一、**分面上没有它**——这一条是前提，不成立的话底下测的就不是 `Q1109`。
    assert!(
        !app.browse()
            .facets()
            .collections
            .iter()
            .any(|一个| 一个.value == 名字),
        "这条成员关系居然投影出了变体，这一条的前提不成立了"
    );

    // 二、**弹层里有它，还写得出记了几个成员**。
    shared::点正好(&ctx, "管理合集…", |ui| app.ui(ui));
    跑(&ctx, &mut app, 2);
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    assert!(
        屏上.contains(名字),
        "筛选筛不着的合集没列在「管理合集」里——它就此改不动也删不掉：\n{屏上}"
    );
    assert!(
        屏上.contains("1 个成员"),
        "没写出它记了几个成员（数的是沉淀库那本账，一行一条锚）：\n{屏上}"
    );

    // 三、**改得动**：改完沉淀库里是新名字。
    点弹层里的(&ctx, "改名", "", |ui| app.ui(ui));
    点弹层里的(&ctx, 名字, "", |ui| app.ui(ui));
    headless::frame(
        &ctx,
        shared::输入(vec![
            shared::按键事件(egui::Key::End),
            egui::Event::Text("·改".to_string()),
        ]),
        |ui| app.ui(ui),
    );
    点弹层里的(&ctx, "保存", "", |ui| app.ui(ui));
    跑(&ctx, &mut app, 2);
    let 沉淀库里: Vec<String> = app
        .site()
        .store
        .collections()
        .expect("读得出")
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert!(
        沉淀库里.iter().any(|一个| 一个 == "换过根的那一批·改"),
        "改名没落到沉淀库：{沉淀库里:?}\n回执：{:?}／{:?}",
        app.browse().notice(),
        app.browse().error(),
    );
}

/// **「管理合集」那一层：删除＝把成员全部移出，写着它的子库规则一个字不改**
/// （票 `gui-looks-like-the-design/13`）。
///
/// 三件事一起钉：
///
/// 1. 删之前那句警告**说得出有几条子库规则提到它**（那个数按下「删除…」那一下才问）；
/// 2. 按下去之后**库里真的空了**——断的是分面与沉淀库，不是屏上那句回执；
/// 3. **那几条子库规则原样留着**。这是核心库那一处有意的决定（`collection::drop_all`
///    的文档写着理由：删完还可能再建一个同名的回来），屏上那句回执与它得是同一口径。
///
/// 另外钉一条容易忘的：**正被筛着的那个合集删掉时，筛选栏那一维要跟着清掉**，
/// 不然屏上筛着一个已经不在的合集，一行都不剩而看不出为什么。
#[test]
fn 管理合集那一层删得掉_成员全部移出而规则原样留着() {
    let ctx = headless::context();
    let mut app = shared::小库(
        &[("SFC", "短.zip", shared::档::命中)],
        shared::干净工作目录("romcat-测试-浏览-删合集"),
    );
    app.show_view(View::Browse);
    跑(&ctx, &mut app, 3);

    // 现场：一个自建合集，外加一条写着它的子库规则、一条与它无关的。
    {
        let (browse, site) = app.browse_and_site();
        let keys = site
            .catalog
            .variant_page(&romcat_core::catalog::browse::VariantQuery::default(), 0, 8)
            .expect("取得出变体")
            .into_iter()
            .map(|row| row.key)
            .collect::<Vec<_>>();
        romcat_core::collection::add(site, "送朋友的", &keys).expect("加得进");
        site.catalog
            .put_sublibrary(&romcat_core::sublibrary::Sublibrary::at(
                "掌机",
                std::path::Path::new("/Volumes/SDCARD/掌机"),
                "Pegasus",
                None,
            ))
            .expect("建得出子库");
        for 一条 in ["合集=送朋友的", "平台=SFC"] {
            site.catalog
                .add_rule(
                    "掌机",
                    &romcat_core::sublibrary::Rule::parse(一条).expect("读得懂"),
                    None,
                )
                .expect("加得进规则");
        }
        browse.invalidate(site);
    }
    跑(&ctx, &mut app, 3);

    // 一之一、**先按「按它筛选」筛着**（顺带钉住那一颗）：删完这一维该跟着清掉。
    shared::点正好(&ctx, "管理合集…", |ui| app.ui(ui));
    跑(&ctx, &mut app, 2);
    点弹层里的(&ctx, "按它筛选", "", |ui| app.ui(ui));
    跑(&ctx, &mut app, 3);
    assert_eq!(
        app.browse().query().collection.as_deref(),
        Some("送朋友的"),
        "按了「按它筛选」，筛选栏那一维没跟着设上"
    );

    // 一、再摊开一次、按「删除…」，那句警告说得出有几条规则提到它。
    shared::点正好(&ctx, "管理合集…", |ui| app.ui(ui));
    跑(&ctx, &mut app, 2);
    let 屏上 = 点弹层里的(&ctx, "删除…", "", |ui| app.ui(ui));
    assert!(
        屏上.contains("1 条规则提到这个合集"),
        "删之前那句警告没说清有几条子库规则提到它：\n{屏上}"
    );
    assert!(
        屏上.contains("作品本身不受影响"),
        "删之前那句警告没说清作品本身不动：\n{屏上}"
    );

    // 二、按「删除合集」。
    let 屏上 = 点弹层里的(&ctx, "删除合集", "", |ui| app.ui(ui));
    跑(&ctx, &mut app, 2);

    // 三、**库里真的空了**——断分面与沉淀库，不断屏上那句话。
    let 分面: Vec<String> = app
        .browse()
        .facets()
        .collections
        .iter()
        .map(|一个| 一个.value.clone())
        .collect();
    assert!(
        !分面.iter().any(|一个| 一个 == "送朋友的"),
        "删完分面上还有它：{分面:?}\n屏上：\n{屏上}\n回执：{:?}／{:?}",
        app.browse().notice(),
        app.browse().error(),
    );
    let 沉淀库里: Vec<String> = app
        .site()
        .store
        .collections()
        .expect("读得出")
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert!(
        !沉淀库里.iter().any(|一个| 一个 == "送朋友的"),
        "删完沉淀库里还有它：{沉淀库里:?}"
    );

    // 四、**那两条子库规则原样留着**（核心库那一处有意的决定）。
    let 规则们: Vec<String> = app
        .site()
        .catalog
        .sublibrary_rules("掌机")
        .expect("读得出")
        .into_iter()
        .map(|一条| 一条.text)
        .collect();
    assert_eq!(
        规则们,
        vec!["合集=送朋友的".to_string(), "平台=SFC".to_string()],
        "删合集不该动子库规则——再建一个同名的就又筛得出来了"
    );

    // 五、**正被筛着的那一维跟着清掉**。
    assert_eq!(
        app.browse().query().collection,
        None,
        "删掉正被筛着的那个合集之后，筛选栏那一维没跟着清掉"
    );
}

/// **「加入合集」那个弹层：新建一个合集，把勾中的那一批放进去**
/// （票 `gui-looks-like-the-design/13`）。
///
/// 那一颗在表格上方那一条的批量操作里，位置照稿排在「★ 收藏」与「合并作品…」之间
/// （挂单 `Q1102`）。
#[test]
fn 加入合集那一层建得出新合集_名字写不得时加不进() {
    let ctx = headless::context();
    let mut app = shared::小库(
        &[("SFC", "短.zip", shared::档::命中)],
        shared::干净工作目录("romcat-测试-浏览-加入合集"),
    );
    app.show_view(View::Browse);
    跑(&ctx, &mut app, 3);

    // 勾一行——那一组作用于勾中的那一批。
    {
        let (browse, site) = app.browse_and_site();
        let anchor = site
            .catalog
            .work_page(browse.query(), 0, 1)
            .expect("取得出一行")
            .remove(0)
            .anchor;
        browse.picked_mut().toggle(&anchor);
    }
    跑(&ctx, &mut app, 2);

    // 一、那一颗把弹层摊开；库里还没有合集，所以默认落在「新建合集」那一档。
    shared::点正好(&ctx, "加入合集…", |ui| app.ui(ui));
    跑(&ctx, &mut app, 2);
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    assert!(屏上.contains("新建合集"), "那一层没摊开：\n{屏上}");
    // **挂不住内容锚那件事照实说，但不现编那个数**（挂单 `Q1103`）。
    assert!(
        屏上.contains("只钉得住本机路径"),
        "没说清无判据那一档会怎样：\n{屏上}"
    );

    // 二、**名字写不得时「加入」按不动，而且屏上说得出为什么**（ADR-0005）。
    //
    // 两档各来一个：带逗号的（逗号是规则里的值分隔符），以及**写不进规则**的
    // （两侧带空白的连接词）。后者第一版漏掉了——`check_name` 当时只拦逗号，
    // 于是 `甲 或 乙` 建得出来、`合集=甲 或 乙` 筛不出东西，改名还会把子库悄悄改空。
    for (打什么, 该说的) in [("带,逗号", "名称里不能有逗号"), ("甲 或 乙", "写不进规则")]
    {
        let 屏上 = 点弹层里的(&ctx, "例如：通关过的", 打什么, |ui| app.ui(ui));
        assert!(
            屏上.contains(该说的),
            "名字「{打什么}」用不得，屏上却没说为什么：\n{屏上}"
        );
        // **「加不进」得真的加不进**：按一下「加入」，库里一个合集都不许多出来。
        // 只断屏上那句错的话，按钮真按得动时这一条照样绿。
        点弹层里的(&ctx, "加入", "", |ui| app.ui(ui));
        跑(&ctx, &mut app, 3);
        let 沉淀库里 = app.site().store.collections().expect("读得出");
        assert!(
            沉淀库里.is_empty(),
            "名字用不得却加进去了：{沉淀库里:?}\n回执：{:?}／{:?}",
            app.browse().notice(),
            app.browse().error(),
        );
        // 把打进去的擦掉，好让下一档从空框开始。
        点弹层里的(&ctx, 打什么, "", |ui| app.ui(ui));
        for _ in 0..打什么.chars().count() {
            headless::frame(
                &ctx,
                shared::输入(vec![
                    shared::按键事件(egui::Key::End),
                    shared::按键事件(egui::Key::Backspace),
                ]),
                |ui| app.ui(ui),
            );
        }
    }

    // 三、**换个用得上的名字**（上一步已经把框擦空了）。
    点弹层里的(&ctx, "例如：通关过的", "", |ui| app.ui(ui));
    headless::frame(
        &ctx,
        shared::输入(vec![egui::Event::Text("送朋友的".to_string())]),
        |ui| app.ui(ui),
    );
    let 屏上 = shared::跑一帧(&ctx, |ui| app.ui(ui));
    assert!(
        !屏上.contains("名称里不能有逗号"),
        "名字改好了那句错还挂着：\n{屏上}"
    );

    // 四、按「加入」——那一趟排上任务台，跑完认领才落库。
    点弹层里的(&ctx, "加入", "", |ui| app.ui(ui));
    shared::等任务台空了(&mut app);
    跑(&ctx, &mut app, 3);

    // 五、**库里真的有了这个合集**，分面上带着数。
    let 分面: Vec<(String, u64)> = app
        .browse()
        .facets()
        .collections
        .iter()
        .map(|一个| (一个.value.clone(), 一个.count))
        .collect();
    assert!(
        分面.iter().any(|(名, 几个)| 名 == "送朋友的" && *几个 > 0),
        "建出来的那个合集没进分面：{分面:?}"
    );
}
