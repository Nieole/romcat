//! **子库屏**：一台设备一张卡、**选择集只读**、排差量预览、同步从这里触发。
//!
//! 这几条是这张票最要紧的纪律，而它们都是「不这么做会出事」而不是「这样比较好看」：
//!
//! - **这一屏不选内容**（票 `gui-redesign/11`）：规则与例外都在浏览屏上改，
//!   点「改选择」跳过去、规则预填进筛选器，按「更新到子库」原样带回。
//! - **同步前必须预览差量**（ADR-0016）：没排过预览，那个按钮就不该动得了。
//! - **删除前必须干跑预览**（ADR-0015）：计划里有删除时还要人再点一次头。
//! - **容量超限不自动截断**（ADR-0016）：报出超出量与按体积排序的裁剪建议，一个都不砍。
//! - **只碰清单里记录过的文件**（ADR-0015）：维护者自己拷进卡里的东西，同步前后
//!   连修改时间都一样。
//! - **目标不许落在主库里**（ADR-0004）：存下来那一步就拦，不等到点同步。
//! - **三条长活全走任务台**（票 `gui-redesign/15`）：排差量预览、算一遍容量、同步。
//!   而**台上排着的那一趟认的是排它时那份计划**——破了这一条，人改完规则、台上那趟旧活
//!   跑起来，往卡上写的就是他已经改掉的那一批。
//!
//! 目标设备**一律拿本地 fixture 目录模拟**：绝不去动任何真实设备或 SD 卡。

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::Duration;

use romcat_core::capability::RejectReason;
use romcat_core::catalog::Catalog;
use romcat_core::catalog::browse::Scope;
use romcat_core::fs::RealFs;
use romcat_core::scan::{self, Jobs, ScanOptions};
use romcat_core::site::Site;
use romcat_core::sublibrary::{Exception, Group, Join, Rule};
use romcat_core::task::Ending;
use romcat_core::task::Handle;
use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};
use romcat_gui::app::{App, View};
use romcat_gui::headless;

mod shared;
use shared::画出来的字;

/// 这一趟拿来当目标的那个 fixture 目录里，维护者自己拷进去的东西叫什么。
const 存档: &str = "我自己拷进来的存档.sav";

/// 那份存档里装着什么。同步前后必须一个字节不差。
const 存档内容: &str = "通关存档，别动";

/// 摆进库里那条**读不懂**的规则，原文长这样。屏上要原样印出来。
const 坏规则原文: &str = "这不是一条规则";

/// 连画两帧，交出**后一帧**画在屏上的字。
///
/// 头一帧 egui 还在量各块占多大，摊开与收起的状态要下一帧才落定
/// （`这一屏画得出来_摊开与收起都不炸` 也是连跑两帧）。
fn 画两帧(ctx: &egui::Context, 场: &mut 现场) -> String {
    let mut out = String::new();
    for _ in 0..2 {
        out = 画出来的字(&headless::frame(ctx, headless::input(), |ui| 场.app.ui(ui)));
    }
    out
}

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

/// 一份小 fixture 主库。**只读**——这几行只往临时目录里写，一个字节都不碰真库。
fn 建库() -> TempDir {
    let dir = temp_dir("gui-sub-lib");
    写(&dir.path().join("SFC/幻想传说 汉化版.zip"), &zip(4096));
    写(&dir.path().join("SFC/圣剑传说 3 汉化版.zip"), &zip(8192));
    写(&dir.path().join("GBA/口袋妖怪 绿宝石.zip"), &zip(2048));
    dir
}

/// 一整套现场：fixture 主库、工作目录、当目标用的那个 fixture 目录、界面。
struct 现场 {
    库: TempDir,
    工作区: TempDir,
    卡: TempDir,
    app: App,
}

impl 现场 {
    fn 摆好() -> Self {
        Self::摆好带(None)
    }

    /// 摆一套现场。`第二个根` 给了的话就一起扫进**同一份中立库**（主库是一组根）。
    fn 摆好带(第二个根: Option<&TempDir>) -> Self {
        let 库 = 建库();
        let 工作区 = temp_dir("gui-sub-ws");
        let 卡 = temp_dir("gui-sub-card");
        // 维护者自己拷进卡里的东西。工具连看都不该看它（ADR-0015）。
        写(&卡.path().join(存档), 存档内容.as_bytes());

        // **中立库落在磁盘上**，不是只活在内存里：排差量预览跑在**任务台**上，
        // 后台那条线程要的是同一个文件的第二份只读连接（`Catalog::read_only`）。
        // 真库本来就是这个样子，fixture 照着摆才验得到那条路。
        let 库文件 = 工作区.path().join("catalog").join("fixture.sqlite3");
        {
            let mut catalog = Catalog::open(&库文件).expect("能开中立库");
            let mut roots: Vec<(&str, &Path)> = vec![("库", 库.path())];
            if let Some(第二个) = 第二个根 {
                roots.push(("另一块盘", 第二个.path()));
            }
            for (name, root) in roots {
                let mut options = ScanOptions::named(root, name);
                options.jobs = Jobs::Fixed(2);
                scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");
            }
        }
        let site = Site::open_file(工作区.path(), &库文件, None).expect("开得出现场");
        let mut app = App::new(site, 工作区.path().to_path_buf());
        app.show_view(View::Sublibraries);
        Self {
            库,
            工作区,
            卡,
            app,
        }
    }

    /// 建一个子库。`capacity` 写成 `512GB` 那样；空串是不设限。
    fn 建子库(&mut self, name: &str, capacity: &str) {
        let target = romcat_core::path::display(self.卡.path());
        let (screen, site) = self.app.sublibrary_and_site();
        {
            let form = screen.form_mut();
            form.name = name.to_string();
            form.target = target;
            capacity.clone_into(&mut form.capacity);
        }
        screen.save(site);
        assert!(screen.error().is_none(), "{:?}", screen.error());
    }

    /// 往库里摆一条规则。
    ///
    /// **这一屏上写不了规则了**（票 `gui-redesign/11`：选择集在这儿只读），
    /// 所以这个前提直接摆进中立库——界面上那条路是「浏览屏筛好按存成子库」，
    /// 它自己在 `tests/browse.rs` 里验。
    fn 加规则(&mut self, name: &str, rule: &str) {
        let parsed = Rule::parse(rule).expect("读得懂");
        let (screen, site) = self.app.sublibrary_and_site();
        site.catalog.add_rule(name, &parsed).expect("写得进去");
        screen.reload(site);
        screen.open(site, name);
    }

    /// 往库里塞一条**读不懂**的规则，跟着把那张卡重读一遍。
    ///
    /// 库里怎么会有读不懂的规则？中立库是个 SQLite 文件，人打得开；换一版程序、
    /// 删掉一个维度之后旧规则也会读不懂（`LoadedSelection::from_stored` 的文档）。
    /// 这里照那种情形摆一条：原文存的是读不回来的字，`add_rule` 那道「先读懂再写」
    /// 的闸只好绕过去——真库上它正是这么长出来的。
    fn 摆一条读不懂的(&mut self, name: &str, text: &str) -> i64 {
        let 坏的 = Rule {
            text: text.to_string(),
            root: Group::new(Join::All, Vec::new()),
        };
        let (screen, site) = self.app.sublibrary_and_site();
        let ordinal = site.catalog.add_rule(name, &坏的).expect("写得进去");
        screen.open(site, name);
        ordinal
    }

    /// 点「改选择」，跟着让窗口把人送去浏览屏。
    fn 改选择(&mut self) {
        self.app.sublibrary_and_site().0.edit_selection();
        self.app.route();
    }

    /// 在浏览屏上点「更新到子库」，跟着让窗口把人送回子库屏。
    fn 更新到子库(&mut self) {
        {
            let (browse, site) = self.app.browse_and_site();
            browse.update_sublibrary(site);
            assert!(browse.error().is_none(), "{:?}", browse.error());
        }
        self.app.route();
    }

    /// 眼下浏览屏那份筛选展开出来是哪一批变体。
    fn 屏上筛出来的(&mut self) -> BTreeSet<String> {
        let (browse, site) = self.app.browse_and_site();
        let query = browse.query().clone();
        site.catalog
            .scoped_variants(&query, Scope::AllExcept(&[]))
            .expect("展得开")
            .into_iter()
            .collect()
    }

    /// 某个子库眼下的选择集选出来是哪一批变体。
    fn 子库选出来的(&self, name: &str) -> BTreeSet<String> {
        let catalog = &self.app.site().catalog;
        let loaded = catalog.selection(name).expect("选择集读得回来");
        assert!(loaded.broken.is_empty(), "{:?}", loaded.broken);
        let facts = romcat_core::sublibrary::facts(catalog).expect("事实折得出来");
        romcat_core::sublibrary::select(&loaded.selection, &facts)
            .picked
            .into_iter()
            .map(|picked| picked.key)
            .collect()
    }

    /// 排一次差量预览，然后等它跑完。**它进任务队列**，所以要一直问「跑完没有」。
    fn 排预览(&mut self) {
        {
            let (screen, site, tasks) = self.app.sublibrary_site_and_tasks();
            screen.preview(site, tasks);
        }
        self.等任务跑完();
    }

    /// 等任务台上那一趟跑完，并把产物收回该收它的那一屏。
    fn 等任务跑完(&mut self) {
        for _ in 0..600 {
            self.app.poll_tasks();
            if !self.app.tasks().busy() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("任务六秒都没跑完");
    }

    /// 按一下「算一遍容量」，然后等它跑完。**它进任务队列**，与排差量预览同一条路。
    /// 摊开某一张卡（读它的规则与例外）。**这一下会把摆着的那份差量作废**——
    /// 换了子库还留着上一个的差量，是这一屏最容易骗到人的一种写法。
    fn 摊开(&mut self, name: &str) {
        let (screen, site) = self.app.sublibrary_and_site();
        screen.open(site, name);
    }

    /// 排一趟**占着位子**的活上去。台上一次只跑一趟，于是这之后排上去的那些都在队里
    /// 等着——「排上去之后再改规则」这类事情因此不带竞态。
    ///
    /// **步数给得足够多**，多到它绝不可能在测试看完之前自己跑完：早先写死的
    /// 400 步 × 5 ms 正好两秒，机器一忙（全量测试并排跑）就自己先结束了、测试假失败
    /// （`tests/task.rs` 里那个占位任务栽过同一跤）。用它的每一条都自己按停下。
    fn 占住位子(&mut self) -> u64 {
        self.app.tasks_mut().queue("占着位子", |task| {
            for _ in 0..40_000 {
                task.check()?;
                std::thread::sleep(Duration::from_millis(5));
            }
            Err("这一趟本来就只是占着位子".to_string())
        })
    }

    fn 求值(&mut self) {
        {
            let (screen, site, tasks) = self.app.sublibrary_site_and_tasks();
            screen.evaluate(site, tasks);
        }
        self.等任务跑完();
    }

    /// 点同步，然后等它跑完。**它也进任务队列**（票 `gui-redesign/15`），
    /// 所以等法与排差量预览、算一遍容量三条一模一样。
    fn 同步到底(&mut self) {
        {
            let (screen, site, tasks) = self.app.sublibrary_site_and_tasks();
            screen.sync(site, tasks);
        }
        self.等任务跑完();
    }
}

#[test]
fn 一台设备一张卡目标格式容量与待同步步数一眼看得见() {
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "1GB");
    场.建子库("备份卡", "");
    场.加规则("掌机", "平台=SFC");
    场.加规则("备份卡", "平台=GBA");

    // 一台设备一张卡：卡上那几格直接来自子库自己。
    let 卡片: Vec<_> = 场.app.sublibrary().list().to_vec();
    assert_eq!(卡片.len(), 2, "两台设备该是两张卡");
    let 掌机 = 卡片.iter().find(|row| row.name == "掌机").expect("在");
    assert!(!掌机.target.is_empty(), "卡上没有目标路径");
    assert_eq!(掌机.format, "Pegasus", "卡上没有前端格式");
    assert_eq!(掌机.capacity, Some(1_000_000_000), "卡上没有容量上限");

    // **待同步步数**：排过差量的那一台才有；没排过就是「还不知道」，不是零。
    assert!(场.app.sublibrary().prepared().is_none());
    场.排预览();
    let screen = 场.app.sublibrary();
    let plan = &screen.prepared().expect("排得出来").plan;
    assert_eq!(plan.sublibrary, "备份卡", "排的该是眼下摊开的那一台");
    assert!(plan.touched() > 0, "待同步步数是零，这条断言等于没测");
}

#[test]
fn 容量条三段各自标得出数() {
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "1GB");
    场.加规则("掌机", "平台=SFC");

    // **卡不在手边也算得出选中那一段**（ADR-0009）：只问中立库。
    场.求值();
    let gauge = 场.app.sublibrary().gauge("掌机");
    assert_eq!(gauge.capacity, Some(1_000_000_000), "上限那一段没数");
    assert!(gauge.picked > 0, "选中那一段没数");
    assert_eq!(
        gauge.strangers, None,
        "还没排过差量就说得出「清单之外」多大——那是编的",
    );
    // 一成都不到：条子上选中那一段与上限的比例算得出来。
    assert!(gauge.picked_share() > 0.0 && gauge.picked_share() < 0.01);
    assert_eq!(gauge.stranger_share(), 0.0);
    assert_eq!(gauge.scale(), 1_000_000_000, "没超限时条子照上限画满");

    // **清单之外要目标设备在位才知道。** 排完差量它就有数了。
    场.排预览();
    let plan_strangers = 场
        .app
        .sublibrary()
        .prepared()
        .expect("排得出来")
        .plan
        .stranger_bytes;
    let gauge = 场.app.sublibrary().gauge("掌机");
    assert_eq!(
        gauge.strangers,
        Some(plan_strangers),
        "清单之外那一段与计划里那个数不是同一个",
    );
    assert!(plan_strangers > 0, "维护者那份存档该被数进清单之外");
    assert_eq!(
        gauge.taken(),
        gauge.picked + plan_strangers,
        "卡上一共占多少 = 选中的 + 清单之外的",
    );
}

#[test]
fn 点改选择跳到浏览屏而且筛选器里预填的是这个子库的规则() {
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    场.加规则("掌机", "中文=汉化");

    场.改选择();
    assert_eq!(场.app.view(), View::Browse, "没跳去浏览屏");
    let editing = 场.app.browse().editing().expect("正在改一个子库");
    assert_eq!(editing.sublibrary, "掌机");
    assert!(editing.broken.is_empty());

    // **多条规则之间是并集**，与求值同一条口径——预填的正是那一条。
    let 预填 = 场.app.browse().query().rule.clone().expect("预填了规则");
    let 并 = Rule::any_of(vec![
        Rule::parse("平台=SFC").expect("读得懂"),
        Rule::parse("中文=汉化").expect("读得懂"),
    ])
    .expect("并得起来");
    assert_eq!(预填, 并, "筛选器里预填的不是这个子库的规则");

    // **屏上筛出来的就是这个子库选出来的那一批。**
    assert_eq!(场.屏上筛出来的(), 场.子库选出来的("掌机"));
}

#[test]
fn 在浏览屏调完更新到子库规则原样带回而且选出来的与屏上一致() {
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");

    // 一趟什么都没改的往返：规则**原样**回去，一个字都不变。
    场.改选择();
    场.更新到子库();
    assert_eq!(场.app.view(), View::Sublibraries, "没跳回子库屏");
    let 规则: Vec<_> = 场
        .app
        .sublibrary()
        .rules()
        .iter()
        .map(|stored| stored.text.clone())
        .collect();
    assert_eq!(规则, vec!["平台=SFC".to_string()], "空跑一趟规则就走样了");

    // 再来一趟，这次在浏览屏上真的改：多要一个平台。
    场.改选择();
    {
        let (browse, _) = 场.app.browse_and_site();
        browse.set_filter_rule(Some(Rule::parse("平台=SFC 或 平台=GBA").expect("读得懂")));
    }
    let 屏上 = 场.屏上筛出来的();
    assert_eq!(屏上.len(), 3, "这份筛选该选中全部三个变体");
    场.更新到子库();

    // **换掉而不是加上去**：加的话旧那条还在，子库选出来的就比屏上多。
    let 规则: Vec<_> = 场
        .app
        .sublibrary()
        .rules()
        .iter()
        .map(|stored| stored.text.clone())
        .collect();
    assert_eq!(规则, vec!["平台=SFC 或 平台=GBA".to_string()]);
    assert_eq!(
        场.子库选出来的("掌机"),
        屏上,
        "带回去之后选出来的与屏上不是同一批"
    );
    // 回到子库屏时那份差量预览当场作废：选择集变了。
    assert!(场.app.sublibrary().prepared().is_none());
}

#[test]
fn 例外在浏览屏上加减子库屏如实显示有几条() {
    // ADR-0016：**例外优先于规则、永久记住**。加减落在浏览屏——「哪一份」只有在
    // 详情面板里才指得准；子库屏只数一数。
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    assert!(场.app.sublibrary().exceptions().is_empty());

    场.改选择();
    {
        let (browse, site) = 场.app.browse_and_site();
        browse.set_exception(site, "库/GBA/口袋妖怪 绿宝石.zip", Exception::Include);
        browse.set_exception(site, "库/SFC/幻想传说 汉化版.zip", Exception::Exclude);
        assert!(browse.error().is_none(), "{:?}", browse.error());
        let editing = browse.editing().expect("还在改");
        assert_eq!(editing.exceptions.len(), 2, "例外没落库");
    }
    场.更新到子库();

    // **子库屏如实显示有几条**，方向也分得出来。
    let 例外 = 场.app.sublibrary().exceptions();
    assert_eq!(例外.len(), 2);
    assert_eq!(
        例外
            .iter()
            .filter(|row| row.kind == Exception::Exclude)
            .count(),
        1,
    );
    // 例外真的起了作用：规则说要的那个被排除了，规则没说的那个被含了进来。
    let 选中 = 场.子库选出来的("掌机");
    assert!(
        !选中.contains("库/SFC/幻想传说 汉化版.zip"),
        "排除例外没起作用"
    );
    assert!(
        选中.contains("库/GBA/口袋妖怪 绿宝石.zip"),
        "收入例外没起作用"
    );

    // 撤掉之后重新由规则说了算。
    场.改选择();
    {
        let (browse, site) = 场.app.browse_and_site();
        browse.clear_exception(site, "库/SFC/幻想传说 汉化版.zip");
    }
    场.更新到子库();
    assert_eq!(场.app.sublibrary().exceptions().len(), 1);
    assert!(
        场.子库选出来的("掌机")
            .contains("库/SFC/幻想传说 汉化版.zip")
    );
}

#[test]
fn 目标落在主库里当场拦下() {
    // ADR-0004：**主库只读**。同步会往目标上写文件、删文件，绝不能指着那块盘——
    // 而拦在**存下来**这一步，不等到点同步：一个指着主库的子库定义放在库里，
    // 下一次点同步之前谁都不知道它错了。
    let mut 场 = 现场::摆好();
    let 主库里 = romcat_core::path::display(&场.库.path().join("SFC"));
    {
        let (screen, site) = 场.app.sublibrary_and_site();
        {
            let form = screen.form_mut();
            form.name = "指错盘的".to_string();
            form.target = 主库里;
        }
        screen.save(site);
        let message = screen.error().expect("该被拦下来");
        assert!(
            message.contains("主库只读"),
            "拦下来的理由该说清是主库只读：{message}",
        );
    }
    assert!(
        场.app
            .site()
            .catalog
            .sublibrary("指错盘的")
            .expect("读得动")
            .is_none(),
        "拦住了却还是把这条定义写进了中立库",
    );
}

#[test]
fn 选择集在这一屏上只读摆得出规则与例外() {
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    assert_eq!(场.app.sublibrary().list().len(), 1);
    assert_eq!(场.app.sublibrary().picked(), Some("掌机"));
    assert_eq!(场.app.sublibrary().rules().len(), 1);
    assert!(场.app.sublibrary().broken().is_empty());

    // **读不懂的那条照旧摆出来**，而且「改选择」不碰它：它没参与求值，
    // 顺手删掉等于拿一次改选择悄悄清掉用户还没来得及修的东西。
    场.摆一条读不懂的("掌机", 坏规则原文);
    assert_eq!(场.app.sublibrary().broken().len(), 1);

    场.改选择();
    assert_eq!(场.app.browse().editing().expect("在改").broken.len(), 1);
    场.更新到子库();
    assert_eq!(
        场.app.sublibrary().broken().len(),
        1,
        "「改选择」把读不懂的那条一起换掉了",
    );
    assert_eq!(场.app.sublibrary().rules().len(), 1, "读得懂的那条该被换掉");
}

#[test]
fn 读不懂的规则在改选择那条横幅里扔得掉而且别的一条都没动() {
    // 票 `gui-redesign/14`（挂单 `Q86`）：一条读不回来的规则原先在界面上**改不动也
    // 删不掉**，只能去命令行。这一票给它一条出路——**开在浏览屏筛选栏顶上那条横幅里**，
    // 不在子库卡上：子库屏一个写的动作都没有（票 `gui-redesign/11` 的「这一屏不选内容」，
    // 它把八个概念降到三个靠的就是这条）。
    //
    // 这几条断言看的是**这一帧真画出来的字**：查数据结构里那一条是在测别的东西。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    {
        // 一条**永久记住**的手挑例外（ADR-0016）。这一趟扔规则不许连它一起清掉。
        let (_, site) = 场.app.sublibrary_and_site();
        site.catalog
            .set_exception(
                "掌机",
                "库/GBA/口袋妖怪 绿宝石.zip",
                Exception::Include,
                Some("小时候玩过"),
            )
            .expect("例外写得进");
    }
    let 坏的 = 场.摆一条读不懂的("掌机", 坏规则原文);

    // **子库卡上照旧摆得出那一条**（红的，连它错在哪），而下一步说得出在哪儿。
    let 卡上 = 画两帧(&ctx, &mut 场);
    assert!(卡上.contains(坏规则原文), "卡上没摆出那条读不懂的：{卡上}");
    assert!(
        卡上.contains("按上面「改选择」跳去浏览屏"),
        "卡上没写清下一步在哪儿——「看不出下一步」正是挂单 Q86 里最贵的那一半：{卡上}",
    );
    assert!(
        !卡上.contains("扔掉这条"),
        "子库屏上长出了一颗删规则的按钮——票 11 立的「这一屏不选内容」破了：{卡上}",
    );

    // **出路在浏览屏筛选栏顶上那条横幅里**：原文原样摆着，跟着一颗「扔掉这条」。
    // 摆在这儿而不是底下那块面板里，是因为那儿要滚一屏才看得见。
    场.改选择();
    let 横幅 = 画两帧(&ctx, &mut 场);
    assert!(横幅.contains(坏规则原文), "横幅里没摆出那条原文：{横幅}");
    assert!(
        横幅.contains("扔掉这条"),
        "横幅里没有扔掉它的那颗按钮：{横幅}"
    );

    // 按下去那一下。
    {
        let (browse, site) = 场.app.browse_and_site();
        browse.discard_broken_rule(site, 坏的);
        assert!(browse.error().is_none(), "{:?}", browse.error());
        assert!(
            browse.editing().expect("还在改").broken.is_empty(),
            "扔完了，屏上那一栏没重读",
        );
    }
    // 窗口把「这个子库动过了」转告子库屏，那张卡重读一遍。
    场.app.route();
    场.app.show_view(View::Sublibraries);
    let 卡上 = 画两帧(&ctx, &mut 场);
    assert!(!卡上.contains(坏规则原文), "扔掉了，卡上还印着它：{卡上}");

    // **别的一条都没动。**
    let 规则: Vec<String> = 场
        .app
        .sublibrary()
        .rules()
        .iter()
        .map(|stored| stored.text.clone())
        .collect();
    assert_eq!(
        规则,
        vec!["平台=SFC".to_string()],
        "读得懂的那条被顺手带走了"
    );
    assert!(场.app.sublibrary().broken().is_empty());
    let 例外 = 场.app.sublibrary().exceptions();
    assert_eq!(
        例外.len(),
        1,
        "**例外是永久记住的**（ADR-0016），不许被顺手清掉"
    );
    assert_eq!(例外[0].note.as_deref(), Some("小时候玩过"));
    assert_eq!(例外[0].kind, Exception::Include);

    // 扔掉它**不改变这个子库选出什么**——它本来就没参与求值。
    let 选中 = 场.子库选出来的("掌机");
    assert_eq!(选中.len(), 3, "两个 SFC 变体加那条收入的例外");
    assert!(
        选中.contains("库/GBA/口袋妖怪 绿宝石.zip"),
        "收入例外没起作用"
    );
}

#[test]
fn 没排过差量预览就同步不了() {
    // ADR-0016：**同步前必须预览差量，这是硬要求不是优化项。**
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    {
        let (screen, site, tasks) = 场.app.sublibrary_site_and_tasks();
        screen.sync(site, tasks);
        assert_eq!(tasks.queued().len(), 0, "没预览却往任务台上排了一趟");
        assert!(tasks.running().is_none(), "没预览却往任务台上排了一趟");
        let message = screen.error().expect("该被拦下来");
        assert!(
            message.contains("预览"),
            "拦下来的理由该说是缺预览：{message}"
        );
        assert!(screen.outcome().is_none(), "没预览却真的传了");
    }
    // 目标目录上一个新文件都没出现。
    let 卡上 = fs::read_dir(场.卡.path()).expect("列得开").count();
    assert_eq!(卡上, 1, "卡上除了维护者自己那份存档不该多出东西");
}

#[test]
fn 差量预览摆得出新增与净变化而且步骤全部展开得了() {
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    场.排预览();

    let screen = 场.app.sublibrary();
    assert!(screen.error().is_none(), "{:?}", screen.error());
    let prepared = screen.prepared().expect("排得出来");
    // 选择集选中了两个 SFC 变体。
    assert_eq!(prepared.selected.picked.len(), 2);
    // 差量里全是新增：卡上本来什么都没有。
    assert!(prepared.plan.adds.files > 0, "一个新增都没有");
    assert_eq!(prepared.plan.deletes.files, 0);
    assert_eq!(prepared.plan.updates.files, 0);
    assert!(prepared.plan.net_bytes > 0, "净变化该是正的");
    assert_eq!(prepared.plan.touched(), prepared.plan.steps.len() as u64);
    // **清单之外的那个存档被数出来了，而工具一个字节都不碰它**（ADR-0015）。
    assert_eq!(prepared.plan.strangers, 1);
    assert!(
        prepared.plan.steps.iter().all(|step| step.path != 存档),
        "维护者自己拷进去的文件混进了计划",
    );

    // **超长时能全部展开**：默认只摆头几条，按一下摊开全部；重排一次又收回去。
    assert!(
        !场.app.sublibrary().expanded(),
        "一进来就摊开会把同步按钮挤没了"
    );
    场.app.sublibrary_and_site().0.expand(true);
    assert!(场.app.sublibrary().expanded());
    场.排预览();
    assert!(!场.app.sublibrary().expanded(), "重排一次没收回去");

    // **摊开之后那张表真的画了几行**，而且一步都没截：这一份计划只有两步，两步全画。
    let ctx = headless::context();
    场.app.sublibrary_and_site().0.expand(true);
    画两帧(&ctx, &mut 场);
    assert_eq!(
        场.app.sublibrary().steps_drawn(),
        场.app
            .sublibrary()
            .prepared()
            .expect("排得出来")
            .plan
            .steps
            .len(),
        "摊开之后画出来的行数与计划的步数对不上",
    );
}

#[test]
fn 别处改过选择集之后这一屏缓着的差量与容量账当场作废() {
    // 这一屏缓两样派生的东西：差量预览与算过的选择集。**改它们的地方在另一屏上**
    // （票 `gui-redesign/11` 把选择集整个搬去了浏览屏），所以缓着的账得有人来丢。
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "1GB");
    场.加规则("掌机", "平台=SFC");
    场.求值();
    场.排预览();
    let 原先选中 = 场.app.sublibrary().gauge("掌机").picked;
    assert!(原先选中 > 0);

    // **一按就落库的例外**：人按完可以点「不改了」，也可以直接从顶栏切回子库屏
    // ——那两条路上都没有「更新到子库」。
    场.改选择();
    {
        let (browse, site) = 场.app.browse_and_site();
        browse.set_exception(site, "库/SFC/幻想传说 汉化版.zip", Exception::Exclude);
        browse.cancel_editing();
    }
    场.app.route();
    场.app.show_view(View::Sublibraries);

    let screen = 场.app.sublibrary();
    assert!(
        screen.prepared().is_none(),
        "记完例外那份差量还在——「同步」认的正是它，按下去会把刚排除掉的那个传上卡",
    );
    assert!(
        screen.evaluated("掌机").is_none(),
        "记完例外容量账还是旧的那一份",
    );
    assert_eq!(screen.exceptions().len(), 1, "卡上还写着「一条例外都没有」");

    // **改容量上限也一样**：报告里的「超出多少、砍谁」说的还是上一个上限。
    场.求值();
    assert!(场.app.sublibrary().evaluated("掌机").is_some());
    {
        let (screen, site) = 场.app.sublibrary_and_site();
        "4KB".clone_into(&mut screen.form_mut().capacity);
        screen.save(site);
    }
    assert!(
        场.app.sublibrary().evaluated("掌机").is_none(),
        "改完上限还留着按旧上限算出来的那份报告",
    );
}

#[test]
fn 两个根撞在卡上同一条路径时差量预览当场报出来() {
    // 挂单 Q57 点名交给这一票：子库里的落点**剥掉根名**（不剥的话前端认不出平台，
    // ADR-0013），于是两个根里同一条相对路径会落在卡上同一个文件上。
    // **撞上的一个都不放行**（`RejectReason::Collision`），而这一屏得把它说出口
    // ——不说的话，人会对着「明明选中了却没传过去」发呆。
    let 另一块盘 = temp_dir("gui-sub-lib2");
    写(&另一块盘.path().join("SFC/幻想传说 汉化版.zip"), &zip(9999));
    let mut 场 = 现场::摆好带(Some(&另一块盘));
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    场.排预览();

    let plan = &场.app.sublibrary().prepared().expect("排得出来").plan;
    let 撞上的: Vec<_> = plan
        .rejected
        .iter()
        .filter(|row| row.reason == RejectReason::Collision)
        .collect();
    assert_eq!(撞上的.len(), 2, "两条都该被挡下：{:?}", plan.rejected);
    assert!(
        撞上的
            .iter()
            .all(|row| row.path == "SFC/幻想传说 汉化版.zip")
    );
    // 那句话里印的是**完整的键**：不带根名的话两行长得一模一样，
    // 人看不出撞的是哪两块盘。
    assert!(
        撞上的
            .iter()
            .any(|row| row.detail.contains("库/SFC/幻想传说 汉化版.zip"))
            && 撞上的
                .iter()
                .any(|row| row.detail.contains("另一块盘/SFC/幻想传说 汉化版.zip")),
        "{:?}",
        撞上的,
    );
    // 撞上的那两份一步都没进计划——**既不新增也不删除**。
    assert!(
        plan.steps
            .iter()
            .all(|step| step.path != "SFC/幻想传说 汉化版.zip"),
        "撞上的还是被传了一份上去",
    );
    // 没撞的那个照旧要传。
    assert!(
        plan.steps
            .iter()
            .any(|step| step.path == "SFC/圣剑传说 3 汉化版.zip"),
        "没撞的那个被连累了",
    );
}

#[test]
fn 容量超限时给裁剪建议但一个都不自动砍掉() {
    // ADR-0016：**容量超限不自动截断**——同一套规则在两张不同容量的卡上会选出完全
    // 不同的东西，而用户无从得知它砍掉了什么。
    let mut 场 = 现场::摆好();
    场.建子库("小卡", "4KB");
    场.加规则("小卡", "平台=SFC,GBA");
    场.排预览();

    let prepared = 场.app.sublibrary().prepared().expect("排得出来");
    let over = prepared.plan.over_capacity.expect("这份计划该是超限的");
    assert!(over > 0);
    assert!(
        !prepared.plan.trim_suggestions.is_empty(),
        "超限了却一条裁剪建议都没给",
    );
    // 建议**按体积从大到小**，而且累计值是往上加的——「砍到第几个才够」要一眼看得出来。
    let mut 上一个 = u64::MAX;
    let mut 累计 = 0;
    for trim in &prepared.plan.trim_suggestions {
        assert!(trim.bytes <= 上一个, "裁剪建议没按体积排");
        上一个 = trim.bytes;
        累计 += trim.bytes;
        assert_eq!(trim.cumulative, 累计);
    }
    // **一个都没被砍掉**：三个变体全在，而且每一个都真的进了计划。
    assert_eq!(prepared.selected.picked.len(), 3);
    for picked in &prepared.selected.picked {
        assert!(
            prepared
                .plan
                .steps
                .iter()
                .any(|step| step.variant == picked.key),
            "变体 {} 被悄悄截掉了——那正是这条 ADR 禁止的事",
            picked.key,
        );
    }
    // 条子超了就照实际占用画满，不然「正好装满」与「超了三倍」长得一模一样。
    let over = prepared.plan.over_capacity;
    let gauge = 场.app.sublibrary().gauge("小卡");
    assert!(gauge.scale() > 4_000, "超限了条子还照上限画");
    assert_eq!(gauge.picked_share() + gauge.stranger_share(), 1.0);
    // **条子与旁边那行「超出容量上限」必须是同一笔账**：两个总量各算各的话，
    // 会出现「条子画到九成、旁边说超了 200 MiB」——那正是 `Gauge` 的文档说不该
    // 发生的事。这是一条恒等式，不是「差不多」。
    assert_eq!(
        romcat_core::sublibrary::over_capacity(gauge.capacity, gauge.taken()),
        over,
        "条子的总量与计划算超出量用的不是同一个数",
    );
}

#[test]
fn 卡不在手边也算得出选中多少与超限多少() {
    // 子库是**持久实体**，不是「插上卡才存在的东西」（ADR-0015）；而排差量预览要
    // 目标在位（三方对比的第三方就是目标上实际有什么）。于是只求选择集这一步单开一条路：
    // 卡不在手边照样看得见容量账。
    let mut 场 = 现场::摆好();
    场.建子库("小卡", "4KB");
    场.加规则("小卡", "平台=SFC");
    // 把当目标用的那个 fixture 目录整个挪走，模拟「卡没插」。
    let 卡路径 = 场.卡.path().to_path_buf();
    fs::remove_dir_all(&卡路径).expect("删得掉");

    场.求值();
    let screen = 场.app.sublibrary();
    assert!(screen.error().is_none(), "{:?}", screen.error());
    // 报告本身由核心折（`SelectionReport::build`），与 `romcat sublibrary show`
    // 印出来的是同一个值——界面上另算一遍就会长出「这份说装得下、那份说砍这几个」。
    let report = screen.evaluated("小卡").expect("求得出来");
    assert_eq!(report.picked, 2);
    assert_eq!(report.rules.len(), 1);
    assert_eq!(report.rules[0].hits, 2);
    assert!(report.over_capacity.is_some(), "4KB 装不下 12KiB");
    assert!(!report.trim_suggestions.is_empty(), "超限了却没给裁剪建议");
    // 没排过差量那一侧同样对得上：`选中 = report.bytes`、清单之外是「还不知道」。
    let over = report.over_capacity;
    let gauge = screen.gauge("小卡");
    assert_eq!(gauge.taken(), report.bytes);
    assert_eq!(
        romcat_core::sublibrary::over_capacity(gauge.capacity, gauge.taken()),
        over,
    );

    // 而**差量预览**这时该直说目标不在位，不是编一份出来。
    场.排预览();
    let screen = 场.app.sublibrary();
    assert!(screen.prepared().is_none(), "卡不在位却排出了一份差量");
    assert!(screen.error().is_some(), "卡不在位该说出口");
}

#[test]
fn 折一趟事实全部设备共用() {
    // 折事实是走一遍全库（真机上 343 ms，挂账 D156）。一台一折的话，五张卡就是五趟。
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.建子库("备份卡", "");
    场.加规则("掌机", "平台=SFC");
    场.加规则("备份卡", "平台=GBA");

    场.求值();
    let screen = 场.app.sublibrary();
    assert_eq!(screen.evaluated("掌机").expect("求得出来").picked, 2);
    assert_eq!(screen.evaluated("备份卡").expect("求得出来").picked, 1);
    assert!(screen.gauge("掌机").picked > screen.gauge("备份卡").picked);
}

#[test]
fn 改过选择那份预览当场作废() {
    // 留着上一套规则排出来的差量，是这一屏最容易骗到人的一种写法。
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    场.排预览();
    assert!(场.app.sublibrary().prepared().is_some());

    场.改选择();
    {
        let (browse, _) = 场.app.browse_and_site();
        browse.set_filter_rule(Some(Rule::parse("平台=GBA").expect("读得懂")));
    }
    场.更新到子库();
    assert!(
        场.app.sublibrary().prepared().is_none(),
        "改过选择之后那份差量说的已经不是眼下这套选择集会做的事了",
    );
}

#[test]
fn 同步从这里触发而且只碰清单里记录过的文件() {
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    场.排预览();
    let 该动几个 = 场
        .app
        .sublibrary()
        .prepared()
        .expect("排得出来")
        .plan
        .touched();

    let 存档路径 = 场.卡.path().join(存档);
    let 存档时间 = fs::metadata(&存档路径).expect("在盘上").modified().ok();
    场.同步到底();

    let screen = 场.app.sublibrary();
    assert!(screen.error().is_none(), "{:?}", screen.error());
    let outcome = screen.outcome().expect("跑完了");
    assert_eq!(outcome.touched(), 该动几个, "落下来的与预览说的不是一回事");
    assert!(!outcome.interrupted);
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
    // **界面上要说得出这一趟干了什么**：被停的一趟、半数写失败的一趟与全成功的一趟
    // 长得一样，那句「同步用了 X 秒」就是在骗人。
    let notice = screen.notice().expect("有回执");
    assert!(
        notice.contains("新增") && notice.contains("删除"),
        "回执只报了个总数：{notice}",
    );

    // **维护者自己拷进去的那份，连修改时间都没动过。**
    assert_eq!(
        fs::read(&存档路径).expect("还在"),
        存档内容.as_bytes(),
        "清单之外的文件被改了",
    );
    assert_eq!(
        fs::metadata(&存档路径).expect("还在").modified().ok(),
        存档时间,
        "清单之外的文件时间戳被动了",
    );

    // **清单落回了中立库**：下一趟增量才接得上。
    let manifest = 场.app.site().catalog.manifest("掌机").expect("读得出清单");
    assert!(!manifest.files.is_empty(), "清单是空的，下一趟就接不上了");
    assert!(
        !manifest.files.iter().any(|file| file.path == 存档),
        "清单里混进了维护者自己的文件",
    );

    // 传完之后那份预览是过去时了，得重排。
    assert!(场.app.sublibrary().prepared().is_none());
}

#[test]
fn 排差量预览进任务队列跑完之后留一条带耗时的历史() {
    // 挂账 D156：排差量预览原先跑在画帧那条线程上，点一下窗口就僵住几秒。
    // 现在它是**任务台**上的一趟活——跑完了台上把那份差量交回这一屏。
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    场.排预览();

    let screen = 场.app.sublibrary();
    assert!(screen.error().is_none(), "{:?}", screen.error());
    assert!(screen.prepared().is_some(), "差量没交回来");
    assert!(screen.previewing().is_none(), "跑完了却还记着一趟在排");
    assert!(screen.prepare_ms() > 0.0, "耗时没记下来");

    let history = 场.app.tasks().history();
    assert_eq!(history.len(), 1, "任务台上没留下这一趟");
    assert!(
        history[0].name.contains("排差量预览") && history[0].name.contains("掌机"),
        "历史那条说不清是给哪个子库排的：{}",
        history[0].name,
    );
    assert_eq!(history[0].ending, Ending::Done(()));
}

#[test]
fn 排差量预览按停之后一个字节都没写而且再排一次照样排得出() {
    // 「能停」比「能取消」严格：**要停在干净的地方**。排差量预览整条只读，所以它的
    // 干净可以照字面核对——目标设备上一个文件都没多、没少、没被改过。
    //
    // 先拿一趟占位的活把台上那个位子占住，于是「排差量预览」是**排着队**的那一趟，
    // 停它这件事就不带竞态。停的路子与停正在跑的那一趟是同一个 `Board::stop`。
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    let 卡上原样 = 卡上有什么(场.卡.path());

    let 占位 = 场.占住位子();
    {
        let (screen, site, tasks) = 场.app.sublibrary_site_and_tasks();
        screen.preview(site, tasks);
    }
    let 预览 = 场.app.sublibrary().previewing().expect("排上队了");
    场.app.tasks_mut().stop(预览);
    场.app.poll_tasks();

    let screen = 场.app.sublibrary();
    assert!(screen.prepared().is_none(), "按停了却还是排出了一份差量");
    assert!(screen.previewing().is_none(), "按停了却还记着一趟在排");
    assert!(
        screen.error().is_none(),
        "按停下不是出错：{:?}",
        screen.error()
    );
    let notice = screen.notice().expect("该说一句它被停了");
    assert!(notice.contains("停"), "回执没说清是被停了：{notice}");
    // **什么都没排出来，就别记「排它用了多久」**：那是给一份不存在的差量记账。
    assert_eq!(screen.prepare_ms(), 0.0, "按停的那一趟也记了耗时");
    assert_eq!(
        场.app.tasks().history()[0].ending,
        Ending::Stopped,
        "按停了却记成了别的",
    );

    // **目标设备上一个字节都没动。**
    assert_eq!(卡上有什么(场.卡.path()), 卡上原样, "按停了却动了卡上的文件");

    // **再排一次照样排得出完整的一份**：它没有半截状态要收拾。
    场.app.tasks_mut().stop(占位);
    场.等任务跑完();
    场.排预览();
    let screen = 场.app.sublibrary();
    assert!(screen.error().is_none(), "{:?}", screen.error());
    let prepared = screen.prepared().expect("停过一次不该影响下一次");
    assert_eq!(prepared.selected.picked.len(), 2);
    assert!(prepared.plan.adds.files > 0, "重排出来的是一份空计划");
}

#[test]
fn 这一屏画得出来_摊开与收起都不炸() {
    // headless 的一帧钉的是**状态转换**不是像素（规格「接缝二」）：卡片、容量条、
    // 差量步骤表这三样各自都要真的走一遍画的那条路——它们里头有 id、有画笔、
    // 有一张虚拟化的表，编得过不等于画得出来。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "1GB");
    场.建子库("备份卡", "");
    场.加规则("掌机", "平台=SFC");
    场.加规则("备份卡", "平台=GBA");
    场.求值();
    场.排预览();
    for 摊开 in [false, true] {
        场.app.sublibrary_and_site().0.expand(摊开);
        for _ in 0..2 {
            headless::frame(&ctx, headless::input(), |ui| 场.app.ui(ui));
        }
    }
    // 「改选择」跳去浏览屏之后那一屏照样画得出来——例外那一栏是新长出来的。
    场.改选择();
    {
        let (browse, site) = 场.app.browse_and_site();
        let anchor = site
            .catalog
            .work_page(browse.query(), 0, 1)
            .expect("取得出一页")
            .into_iter()
            .next()
            .expect("有行")
            .anchor;
        browse.open_work(&site.catalog, &anchor);
        let key = browse.work().expect("开了").variants[0].row.key.clone();
        browse.pick(&site.catalog, &key);
    }
    for _ in 0..2 {
        headless::frame(&ctx, headless::input(), |ui| 场.app.ui(ui));
    }
    assert_eq!(场.app.view(), View::Browse);
}

/// 一棵目录树底下每个文件的**相对路径、内容、修改时间**，递归到底，按路径排好。
///
/// **「一处不差」要的是逐文件比对**：只列顶上那一层的话，`SFC/` 底下多出来的半份文件、
/// 卡上剩下的 `.romcat-part` 都看不见——而那正是「停下来的地方是干净的」要钉的东西。
///
/// `跳过` 是相对路径前缀。**中立库那个目录得跳过**：SQLite 读一遍就可能动 `-wal`、
/// `-shm` 两个旁支文件，那与「这一趟往盘上写没写东西」是两回事。
fn 目录树(dir: &Path, 跳过: &[&str]) -> Vec<(String, Vec<u8>, Option<std::time::SystemTime>)> {
    fn 收(
        根: &Path,
        at: &Path,
        跳过: &[&str],
        out: &mut Vec<(String, Vec<u8>, Option<std::time::SystemTime>)>,
    ) {
        let Ok(entries) = fs::read_dir(at) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let rel = path
                .strip_prefix(根)
                .expect("在这棵树里")
                .to_string_lossy()
                .replace('\\', "/");
            if 跳过.iter().any(|skip| rel.starts_with(skip)) {
                continue;
            }
            if path.is_dir() {
                收(根, &path, 跳过, out);
                continue;
            }
            out.push((
                rel,
                fs::read(&path).expect("读得到"),
                entry.metadata().expect("读得到元数据").modified().ok(),
            ));
        }
    }
    let mut out = Vec::new();
    收(dir, dir, 跳过, &mut out);
    out.sort();
    out
}

/// 目标设备上眼下有什么。
fn 卡上有什么(dir: &Path) -> Vec<(String, Vec<u8>, Option<std::time::SystemTime>)> {
    目录树(dir, &[])
}

#[test]
fn 同步进任务台跑完之后留一条带耗时的历史而且清单写在认领那一步() {
    // 挂单 Q89：票 11 验收第 6 条要的是「排差量与同步走任务台」，而当时只兑现了排差量
    // 那一半。同步照旧走它自己那条后台线程，于是「上次同步花了多久」这个问题任务屏
    // 答不了——那句回执一换屏就没了。
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    场.排预览();

    {
        let (screen, site, tasks) = 场.app.sublibrary_site_and_tasks();
        screen.sync(site, tasks);
        assert!(screen.syncing().is_some(), "同步没排上任务台");
        // **`sync()` 那一下自己不干活**：干了的话它跑的就是画帧那条线程。
        assert!(
            screen.outcome().is_none(),
            "点一下同步就把账收了——那趟活跑在画帧这条线程上",
        );
        assert!(tasks.busy(), "任务台上没有这一趟");
    }
    // **清单要等认领那一步才落库**：台上那条线拿的是只读连接，写不动。
    assert!(
        场.app
            .site()
            .catalog
            .manifest("掌机")
            .expect("读得出清单")
            .files
            .is_empty(),
        "台上那条线就把清单写了——它写不动才对",
    );

    场.等任务跑完();

    let screen = 场.app.sublibrary();
    assert!(screen.error().is_none(), "{:?}", screen.error());
    assert!(screen.syncing().is_none(), "跑完了却还记着一趟在同步");
    let outcome = screen.outcome().expect("跑完了");
    assert!(outcome.touched() > 0, "一步都没做，这条断言等于没测");
    let notice = screen.notice().expect("有回执");
    assert!(notice.contains("同步用了"), "回执没说耗时：{notice}");
    // **说得出是哪一台**：这句话可能是几十分钟前排上去的那一趟交回来的，
    // 而那会儿摊开的多半已经是另一张卡了。
    assert!(notice.contains("掌机"), "回执没说是哪一台：{notice}");

    // **任务屏上留得下这一趟**：名字说得出是哪个子库，历史那一行带着耗时。
    let history = 场.app.tasks().history();
    assert_eq!(history.len(), 2, "该有排差量与同步两条");
    assert!(
        history[0].name.contains("同步") && history[0].name.contains("掌机"),
        "历史那条说不清是给哪个子库同步的：{}",
        history[0].name,
    );
    assert_eq!(history[0].ending, Ending::Done(()));
    assert!(history[0].elapsed > Duration::ZERO, "历史那条没带耗时");

    // **清单落回了中立库**，而且是在认领那一步落的——下一趟增量才接得上。
    let manifest = 场.app.site().catalog.manifest("掌机").expect("读得出清单");
    assert!(!manifest.files.is_empty(), "清单是空的，下一趟就接不上了");
}

#[test]
fn 同步跑着的时候别的屏照常画得出来() {
    // 一趟真同步几十 GiB、可能几十分钟。跑在画帧那条线程上的话，窗口就是几分钟的白板
    // ——期间切不了屏、滚不动列表、连「停下」都点不着。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    场.排预览();
    {
        let (screen, site, tasks) = 场.app.sublibrary_site_and_tasks();
        screen.sync(site, tasks);
    }
    // **这一下没把活干完**：干完了就说明它跑在画帧这条线程上。
    assert!(场.app.sublibrary().outcome().is_none());

    // 换到浏览屏接着画。**画的这几帧与那趟同步是同时的**，而每一帧浏览屏都答得上话。
    场.app.show_view(View::Browse);
    let mut 画了 = 0;
    for _ in 0..600 {
        headless::frame(&ctx, headless::input(), |ui| 场.app.ui(ui));
        画了 += 1;
        assert!(场.app.window().retained() > 0, "同步跑着的时候浏览屏空了",);
        if 场.app.sublibrary().outcome().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(画了 > 0);
    let screen = 场.app.sublibrary();
    assert!(screen.error().is_none(), "{:?}", screen.error());
    assert!(screen.outcome().is_some(), "六秒了同步还没跑完");
    // 画帧那条路自己就把产物认领了（`App::ui` 每帧问一次任务台）。
    assert!(screen.syncing().is_none());
}

#[test]
fn 台上排着的那一趟同步认的是排它时那份计划() {
    // ADR-0016 那句「同步前必须预览差量」在这一屏上是**构造上的事实**：同步按钮只认屏上
    // 正摆着的那份计划，规则一改它当场作废。搬上任务台之后这条还得成立——**台上排着的
    // 那一趟要认的仍是排它时那份计划**，而不是「跑到的时候屏上摆着的那份」。
    // 破了这一条，人改完规则、台上那趟旧活跑起来，往卡上写的就是他已经改掉的那一批。
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    场.排预览();

    // 先把台上那个位子占住，于是同步是**排着队**的那一趟——「排上去之后再改规则」
    // 这件事就不带竞态。
    let 占位 = 场.占住位子();
    {
        let (screen, site, tasks) = 场.app.sublibrary_site_and_tasks();
        screen.sync(site, tasks);
    }
    assert!(场.app.sublibrary().syncing().is_some(), "同步没排上队");

    // **趁它还排着队，把规则整个换掉**：SFC 换成 GBA。
    场.改选择();
    {
        let (browse, _) = 场.app.browse_and_site();
        browse.set_filter_rule(Some(Rule::parse("平台=GBA").expect("读得懂")));
    }
    场.更新到子库();
    assert!(
        场.app.sublibrary().prepared().is_none(),
        "改过规则那份预览就该当场作废（ADR-0016）",
    );

    // 放行，让排着的那一趟跑完。
    场.app.tasks_mut().stop(占位);
    场.等任务跑完();

    let screen = 场.app.sublibrary();
    assert!(screen.error().is_none(), "{:?}", screen.error());
    assert!(screen.outcome().is_some(), "那一趟没跑");

    // **卡上是排它时那两个 SFC 变体，不是改完之后那个 GBA 的。**
    let 卡上: BTreeSet<String> = 卡上有什么(场.卡.path())
        .into_iter()
        .map(|(path, _, _)| path)
        .collect();
    assert!(
        卡上.iter().any(|path| path.starts_with("SFC/")),
        "排它时那份计划要传的 SFC 一个都没上卡：{卡上:?}",
    );
    assert!(
        !卡上.iter().any(|path| path.starts_with("GBA/")),
        "台上那一趟认了改完之后的规则——那是人已经改掉的那一批：{卡上:?}",
    );

    // 而屏上仍然没有差量预览：**要把改完的那一批传上去，得重排一次**。
    assert!(场.app.sublibrary().prepared().is_none());
}

#[test]
fn 排着队的那一趟同步撤得掉而且撤完目标与工作目录一处不差() {
    // 「能停」比「能取消」严格：**要停在干净的地方**。还没轮到就撤掉的那一趟一个字节
    // 都没写，所以它的干净可以照字面核对——目标设备与工作目录逐文件比对一处不差。
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    场.排预览();

    let 占位 = 场.占住位子();
    {
        let (screen, site, tasks) = 场.app.sublibrary_site_and_tasks();
        screen.sync(site, tasks);
    }
    let 号 = 场.app.sublibrary().syncing().expect("排上队了");
    // 中立库那个目录跳过：SQLite 读一遍就可能动 `-wal`、`-shm`，那与「往盘上写没写
    // 东西」是两回事。
    let 卡上原样 = 卡上有什么(场.卡.path());
    let 工作目录原样 = 目录树(场.工作区.path(), &["catalog"]);

    场.app.tasks_mut().stop(号);
    场.app.poll_tasks();

    let screen = 场.app.sublibrary();
    assert!(screen.syncing().is_none(), "撤掉了却还记着一趟在同步");
    assert!(screen.outcome().is_none(), "撤掉了却记了一趟同步的账");
    assert!(
        screen.error().is_none(),
        "撤掉不是出错：{:?}",
        screen.error()
    );
    let notice = screen.notice().expect("该说一句它被撤掉了");
    assert!(notice.contains("撤掉"), "回执没说清是被撤掉了：{notice}");
    assert_eq!(
        场.app.tasks().history()[0].ending,
        Ending::Stopped,
        "撤掉了却记成了别的",
    );

    // **目标设备与工作目录逐文件比对一处不差。**
    assert_eq!(卡上有什么(场.卡.path()), 卡上原样, "撤掉了却动了卡上的文件");
    assert_eq!(
        目录树(场.工作区.path(), &["catalog"]),
        工作目录原样,
        "撤掉了却在工作目录里留了东西",
    );

    // **那份差量还摆着，再按一次照样传得出去。**
    assert!(
        场.app.sublibrary().prepared().is_some(),
        "撤掉同步不该连差量一起丢"
    );
    场.app.tasks_mut().stop(占位);
    场.等任务跑完();
    场.同步到底();
    let screen = 场.app.sublibrary();
    assert!(screen.error().is_none(), "{:?}", screen.error());
    assert!(
        screen.outcome().expect("跑完了").touched() > 0,
        "撤过一次之后再同步一趟什么都没做",
    );
}

#[test]
fn 算一遍容量进任务台跑完之后留一条带耗时的历史() {
    // 挂单 Q87：它原先跑在画帧那条线程上，真机量级上按一下窗口僵 343 毫秒（挂账 D156），
    // 期间连「停下」都没有。
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.建子库("备份卡", "");
    场.加规则("掌机", "平台=SFC");
    场.加规则("备份卡", "平台=GBA");

    {
        let (screen, site, tasks) = 场.app.sublibrary_site_and_tasks();
        screen.evaluate(site, tasks);
        assert!(screen.evaluating().is_some(), "没排上任务台");
        // **这一下自己不算**：算了的话它跑的就是画帧那条线程。
        assert!(
            screen.evaluated("掌机").is_none(),
            "按一下就把数算出来了——那趟活跑在画帧这条线程上",
        );
    }
    场.等任务跑完();

    let screen = 场.app.sublibrary();
    assert!(screen.error().is_none(), "{:?}", screen.error());
    assert!(screen.evaluating().is_none(), "跑完了却还记着一趟在算");
    assert_eq!(screen.evaluated("掌机").expect("算得出来").picked, 2);
    assert_eq!(screen.evaluated("备份卡").expect("算得出来").picked, 1);

    let history = 场.app.tasks().history();
    assert_eq!(history.len(), 1, "任务台上没留下这一趟");
    assert!(
        history[0].name.contains("算一遍容量"),
        "历史那条说不清跑的是什么：{}",
        history[0].name,
    );
    assert_eq!(history[0].ending, Ending::Done(()));
    assert!(history[0].elapsed > Duration::ZERO, "历史那条没带耗时");
}

#[test]
fn 算一遍容量按停之后那几个数没长出来而且一个字节都没写() {
    // 它整条只读——只问中立库，连目标设备都不看（ADR-0009：卡不在手边也算得出来）。
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    let 卡上原样 = 卡上有什么(场.卡.path());

    let 占位 = 场.占住位子();
    {
        let (screen, site, tasks) = 场.app.sublibrary_site_and_tasks();
        screen.evaluate(site, tasks);
    }
    let 号 = 场.app.sublibrary().evaluating().expect("排上队了");
    场.app.tasks_mut().stop(号);
    场.app.poll_tasks();

    let screen = 场.app.sublibrary();
    assert!(screen.evaluating().is_none(), "按停了却还记着一趟在算");
    assert!(screen.evaluated("掌机").is_none(), "按停了却算出了一份");
    assert!(
        screen.error().is_none(),
        "按停下不是出错：{:?}",
        screen.error()
    );
    let notice = screen.notice().expect("该说一句它被停了");
    assert!(notice.contains("停"), "回执没说清是被停了：{notice}");
    assert_eq!(场.app.tasks().history()[0].ending, Ending::Stopped);
    assert_eq!(卡上有什么(场.卡.path()), 卡上原样, "按停了却动了卡上的文件");

    // 再算一次照样算得出来：它没有半截状态要收拾。
    场.app.tasks_mut().stop(占位);
    场.等任务跑完();
    场.求值();
    assert_eq!(
        场.app
            .sublibrary()
            .evaluated("掌机")
            .expect("算得出来")
            .picked,
        2,
    );
}

#[test]
fn 存过子库之后台上那趟还没认领的容量不认了() {
    // 那一趟折报告用的是**排它时**那份子库——上限、目标、能力档案全在里头。
    // 改完上限再把它收回来，卡上就会摆出「上限写着 4KB、旁边说没超」这种对不上的账。
    // 而**丢得不吭声也不行**：人按过那个按钮、等了一趟，屏上一个数都没长出来。
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "1TB");
    场.加规则("掌机", "平台=SFC");

    let 占位 = 场.占住位子();
    {
        let (screen, site, tasks) = 场.app.sublibrary_site_and_tasks();
        screen.evaluate(site, tasks);
    }
    assert!(场.app.sublibrary().evaluating().is_some(), "没排上队");

    // 趁它还排着队，把容量上限从 1TB 改成 4KB。
    场.建子库("掌机", "4KB");
    assert!(
        场.app.sublibrary().evaluating().is_none(),
        "存过子库之后那一趟还认着——它算的是改之前那个上限",
    );
    let notice = 场.app.sublibrary().notice().expect("该说一句");
    assert!(
        notice.contains("算一遍容量"),
        "把那一趟丢了却不吭声：{notice}",
    );

    场.app.tasks_mut().stop(占位);
    场.等任务跑完();
    assert!(
        场.app.sublibrary().evaluated("掌机").is_none(),
        "按旧上限算出来的那份被收回来了",
    );

    // **再算一次报的是新上限**：4KB 装不下那两个 SFC 变体。
    场.求值();
    assert!(
        场.app
            .sublibrary()
            .evaluated("掌机")
            .expect("算得出来")
            .over_capacity
            .is_some(),
        "改完上限重算一遍，该报超限",
    );
}

#[test]
fn 删掉一个子库之后台上那趟还没认领的容量不认了() {
    // 认领是**整份替换**：刚删掉那一台的报告会又长回来，卡片没了、账还在。
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.建子库("备份卡", "");
    场.加规则("掌机", "平台=SFC");
    场.摊开("掌机");

    let 占位 = 场.占住位子();
    {
        let (screen, site, tasks) = 场.app.sublibrary_site_and_tasks();
        screen.evaluate(site, tasks);
    }
    {
        let (screen, site) = 场.app.sublibrary_and_site();
        screen.remove(site);
    }
    assert!(
        场.app.sublibrary().evaluating().is_none(),
        "删掉之后那趟还认着"
    );

    场.app.tasks_mut().stop(占位);
    场.等任务跑完();
    assert_eq!(场.app.sublibrary().list().len(), 1, "该只剩一台");
    assert!(
        场.app.sublibrary().evaluated("掌机").is_none(),
        "删掉的那一台，它的账又长回来了",
    );
}

#[test]
fn 按停一趟同步之后子库屏与任务屏说的是同一件事() {
    // **挂单 Q153**：写过东西的活被按停时照旧交出**清单**（ADR-0015），于是它以前
    // 长着「跑完了」的样子进任务台——子库屏如实说「⚠️ 这一趟被你按停了」，任务屏历史
    // 那一行却写着「完成」。同一趟活在两屏上说两套话，维护者只能两屏比对才敢下结论。
    //
    // 界面上那一下就是这样：按「同步」把活排上台（`Board::queue` 当场开跑），
    // 人紧接着在任务屏上按「停下」。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    场.排预览();
    {
        let (screen, site, tasks) = 场.app.sublibrary_site_and_tasks();
        screen.sync(site, tasks);
    }
    let 号 = 场.app.sublibrary().syncing().expect("这一趟排上任务台了");
    // **按下停下与那一趟真的开跑之间隔着好几个数量级**：这两行是已经热了的几百纳秒，
    // 而对面那条线程要先被 `thread::spawn` 生出来、排上 CPU，然后才走得到第一步
    // ——它整趟传完是十几毫秒的事。所以这一下**不靠抢**：它落在第一步之前。
    //
    // **这儿用不了 `占住位子` 那一招**（撤单那条测试用的是它）：占住位子之后这一趟
    // 就排在队里，而撤掉一趟**还没开跑**的活是「停了，什么都没留下」那一档
    // ——正好不是这条测试要验的那一档。
    场.app.tasks_mut().stop(号);
    场.等任务跑完();

    // 核心那一侧：这一趟走的是**第三支**，不是「跑完了」。
    let ending = &场.app.tasks().history()[0].ending;
    assert!(
        matches!(ending, Ending::Halfway { .. }),
        "被按停却交出了清单的那一趟记成了「{}」\n\
         （记成「完成」的话：这一趟抢在按下停下之前就整趟传完了——\
         那是机器满载时的偶发，不是实现坏了）",
        ending.render(),
    );

    // 子库屏那句回执。
    场.app.show_view(View::Sublibraries);
    let 子库屏 = 画两帧(&ctx, &mut 场);
    assert!(
        子库屏.contains("按停"),
        "子库屏那句回执没说这一趟是被按停的：\n{子库屏}",
    );

    // 任务屏历史那一行——**同一件事，同一个词**。
    场.app.show_view(View::Tasks);
    let 任务屏 = 画两帧(&ctx, &mut 场);
    assert!(
        任务屏.contains("按停"),
        "任务屏历史那一行没说这一趟是被按停的：\n{任务屏}",
    );
    // 历史那一行是「名字、耗时、怎么收场」三格，所以这一趟的收场就在它名字下面两行。
    // **盯住这一趟自己那一行**：台上还有排差量预览那一趟，它是真跑完的，
    // 屏上本来就该有一个「完成」。
    let 这一趟的收场 = 任务屏
        .lines()
        .skip_while(|line| line.trim() != "同步「掌机」")
        .nth(2)
        .expect("任务屏历史里没有这一趟");
    assert!(
        这一趟的收场.contains("停在半路"),
        "任务屏历史那一行没说它留下了东西：{这一趟的收场}",
    );
    assert!(
        !这一趟的收场.contains("完成"),
        "任务屏历史把被按停的那一趟记成了「完成」：{这一趟的收场}",
    );
}

// ——— 步骤表列得全（票 `parking-3/08`，挂账 `D158`）———

/// 造一份 `steps` 步的差量计划。
///
/// **只填屏上那张表画得出来的那几样**：这几条要看的是那张表**画多少行**，
/// 计划本身排得对不对由 `crates/core/tests/sync.rs` 管。造而不是排，是因为真机量级的
/// 一份计划要一万多个变体才排得出来，而这几条要的只是「它有一万步」这一个性质。
fn 造计划(name: &str, steps: usize) -> romcat_core::sync::Plan {
    use romcat_core::sync::{Act, FileKind, Plan, Stamp, Step};
    Plan {
        // **各叫各的名字**：那张表的 id 是照子库名折出来的（`id_salt`），
        // 同一个上下文里画两份同名的计划会共用一份滚动状态。
        sublibrary: name.to_string(),
        steps: (0..steps)
            .map(|at| {
                let source = format!("库/GBA/第{at:06}个.gba");
                Step {
                    act: Act::Add,
                    path: format!("GBA/第{at:06}个.gba"),
                    kind: FileKind::Rom,
                    bytes: 4096,
                    was: 0,
                    source: source.clone(),
                    source_stamp: Stamp {
                        bytes: 4096,
                        mtime_ns: None,
                    },
                    variant: source,
                    restore: false,
                    convert: None,
                }
            })
            .collect(),
        ..Plan::default()
    }
}

/// 把滚动位置按到「第 `at` 步」那一行上，像素。
///
/// 一行的行距在 `egui_extras` 里是 `行高 + item_spacing.y`，**照样式算而不是写死一个
/// 数**——样式一动，写死的那个数会悄悄滚到别处去。这一份上下文没改过间距
/// （`headless::context` 只装字体），所以拿的就是默认那套，与 `bench::row_pitch` 同一条。
fn 滚到第几步(at: usize) -> f32 {
    at as f32 * (romcat_gui::table::ROW_HEIGHT + egui::Style::default().spacing.item_spacing.y)
}

/// 画三帧那张步骤表，返回（这一帧画了几行，这一帧画出来的字）。
///
/// 三帧：头一帧 egui 还在量滚动区有多大，列宽与滚动位置要下一帧才落定。
fn 画步骤表(
    ctx: &egui::Context,
    plan: &romcat_core::sync::Plan,
    scroll_to: Option<f32>,
) -> (usize, String) {
    let mut 画了 = 0;
    let mut 屏上 = String::new();
    for _ in 0..3 {
        let out = headless::frame(ctx, headless::input(), |ui| {
            画了 = romcat_gui::sublibrary::steps_table(ui, plan, scroll_to);
        });
        屏上 = 画出来的字(&out);
    }
    (画了, 屏上)
}

#[test]
fn 八千步的计划滚到第五千步那一行照样摆得出来() {
    // 挂账 `D158`：真机量级上一次同步动上万个文件（实测合成数据 8,206 步），而表列满
    // 2,000 条就打住——想在界面上确认第 5,000 步是什么就得转去命令行，
    // 而**同步前必须看一遍它要做什么**是 ADR-0016 的硬要求。
    let ctx = headless::context();
    let plan = 造计划("八千步", 8_206);
    let 第五千步 = plan.steps[5_000].path.clone();

    let (画了, 屏上) = 画步骤表(&ctx, &plan, Some(滚到第几步(5_000)));
    assert!(
        屏上.contains(&第五千步),
        "第 5,000 步（{第五千步}）没摆在屏上：\n{屏上}",
    );
    assert!(
        !屏上.contains("步没列"),
        "还留着那句「另有 N 步没列」：\n{屏上}",
    );
    // 摆得出第 5,000 步，靠的**不是**把八千行都画出来。
    assert!(
        (1..100).contains(&画了),
        "这一帧画了 {画了} 行——一屏摆得下的只有几十行",
    );

    // 最后一步也滚得到：截断是从末尾开始丢的，只验中间那一行验不出「一步都不截」。
    // 按到**最后一行**上而不是它后头——真界面上滚动条到底就停了，滚不过去；
    // 这里强按一个越界的偏移，`body.rows` 算出来的头一行会落在总数之外，一行都不画。
    let 末 = plan.steps.len() - 1;
    let 最后一步 = plan.steps[末].path.clone();
    let (_, 屏上) = 画步骤表(&ctx, &plan, Some(滚到第几步(末)));
    assert!(
        屏上.contains(&最后一步),
        "最后一步（{最后一步}）滚不到：\n{屏上}",
    );
}

#[test]
fn 翻行画几行只跟视口有多高有关与总步数无关() {
    // **判据不是秒表**（票 `parking-3/17` 给主列表立的也是这条）：挂钟在门禁上是一张
    // 彩票，而「这一帧真的画了几行」机器忙不忙一个字都不影响。
    //
    // 两份**同样合法、只是长短不同**的计划，滚到同一个位置上画出来的行数必须一样——
    // 不一样就说明代价跟着总步数走，那时「去掉上限」就是把卡顿放进来。
    let ctx = headless::context();
    let 短 = 造计划("两千步", 2_000);
    let 长 = 造计划("八千步", 8_206);
    for 第几步 in [0, 500, 1_500] {
        let offset = Some(滚到第几步(第几步));
        let (短画了, _) = 画步骤表(&ctx, &短, offset);
        let (长画了, _) = 画步骤表(&ctx, &长, offset);
        assert!(短画了 > 0, "滚到第 {第几步} 步一行都没画");
        assert_eq!(
            短画了, 长画了,
            "滚到第 {第几步} 步：2,000 步的计划画 {短画了} 行、8,206 步的画 {长画了} 行——\
             代价跟着总步数走了",
        );
    }
}
