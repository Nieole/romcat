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
//!
//! 目标设备**一律拿本地 fixture 目录模拟**：绝不去动任何真实设备或 SD 卡。

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::Duration;

use romcat_core::catalog::Catalog;
use romcat_core::capability::RejectReason;
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

/// 这一趟拿来当目标的那个 fixture 目录里，维护者自己拷进去的东西叫什么。
const 存档: &str = "我自己拷进来的存档.sav";

/// 那份存档里装着什么。同步前后必须一个字节不差。
const 存档内容: &str = "通关存档，别动";

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
    _工作区: TempDir,
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
                scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new())
                    .expect("扫得动");
            }
        }
        let site = Site::open_file(工作区.path(), &库文件, None).expect("开得出现场");
        let mut app = App::new(site, 工作区.path().to_path_buf());
        app.show_view(View::Sublibraries);
        Self {
            库,
            _工作区: 工作区,
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

    fn 求值(&mut self) {
        let (screen, site) = self.app.sublibrary_and_site();
        screen.evaluate(site);
    }

    /// 点同步，然后等它跑完。**后台线程**跑的，所以要一直问。
    fn 同步到底(&mut self) {
        {
            let (screen, site) = self.app.sublibrary_and_site();
            screen.sync(site);
        }
        for _ in 0..600 {
            {
                let (screen, site) = self.app.sublibrary_and_site();
                screen.poll(site);
                if screen.outcome().is_some() || screen.error().is_some() {
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("同步十几秒都没跑完");
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
    assert_eq!(editing.broken, 0);

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
        browse.set_filter_rule(Some(
            Rule::parse("平台=SFC 或 平台=GBA").expect("读得懂"),
        ));
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
    assert_eq!(场.子库选出来的("掌机"), 屏上, "带回去之后选出来的与屏上不是同一批");
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
    assert!(!选中.contains("库/SFC/幻想传说 汉化版.zip"), "排除例外没起作用");
    assert!(选中.contains("库/GBA/口袋妖怪 绿宝石.zip"), "收入例外没起作用");

    // 撤掉之后重新由规则说了算。
    场.改选择();
    {
        let (browse, site) = 场.app.browse_and_site();
        browse.clear_exception(site, "库/SFC/幻想传说 汉化版.zip");
    }
    场.更新到子库();
    assert_eq!(场.app.sublibrary().exceptions().len(), 1);
    assert!(场.子库选出来的("掌机").contains("库/SFC/幻想传说 汉化版.zip"));
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
    {
        // 库里怎么会有读不懂的规则？中立库是个 SQLite 文件，人打得开；换一版程序、
        // 删掉一个维度之后旧规则也会读不懂（`LoadedSelection::from_stored` 的文档）。
        // 这里照那种情形摆一条：原文存的是读不回来的字。
        let 坏的 = Rule {
            text: "这不是一条规则".to_string(),
            root: Group::new(Join::All, Vec::new()),
        };
        let (screen, site) = 场.app.sublibrary_and_site();
        site.catalog.add_rule("掌机", &坏的).expect("写得进去");
        screen.open(site, "掌机");
    }
    assert_eq!(场.app.sublibrary().broken().len(), 1);

    场.改选择();
    assert_eq!(场.app.browse().editing().expect("在改").broken, 1);
    场.更新到子库();
    assert_eq!(
        场.app.sublibrary().broken().len(),
        1,
        "「改选择」把读不懂的那条一起换掉了",
    );
    assert_eq!(场.app.sublibrary().rules().len(), 1, "读得懂的那条该被换掉");
}

#[test]
fn 没排过差量预览就同步不了() {
    // ADR-0016：**同步前必须预览差量，这是硬要求不是优化项。**
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    {
        let (screen, site) = 场.app.sublibrary_and_site();
        screen.sync(site);
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
    assert!(!场.app.sublibrary().expanded(), "一进来就摊开会把同步按钮挤没了");
    场.app.sublibrary_and_site().0.expand(true);
    assert!(场.app.sublibrary().expanded());
    场.排预览();
    assert!(!场.app.sublibrary().expanded(), "重排一次没收回去");
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
    写(
        &另一块盘.path().join("SFC/幻想传说 汉化版.zip"),
        &zip(9999),
    );
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
    assert!(撞上的.iter().all(|row| row.path == "SFC/幻想传说 汉化版.zip"));
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
    let report = &screen.evaluated("小卡").expect("求得出来").report;
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
    assert_eq!(screen.evaluated("掌机").expect("求得出来").report.picked, 2);
    assert_eq!(
        screen.evaluated("备份卡").expect("求得出来").report.picked,
        1,
    );
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
    assert_eq!(history[0].ending, Ending::Done);
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

    let 占位 = 场.app.tasks_mut().queue("占着位子", |task| {
        for _ in 0..400 {
            task.check()?;
            std::thread::sleep(Duration::from_millis(5));
        }
        Err("这一趟只是占着位子".to_string())
    });
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

/// 目标设备上眼下有什么：每个文件的名字、内容、修改时间。
fn 卡上有什么(dir: &Path) -> Vec<(String, Vec<u8>, Option<std::time::SystemTime>)> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).expect("列得开").flatten() {
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        out.push((
            path.file_name()
                .expect("有名字")
                .to_string_lossy()
                .into_owned(),
            fs::read(&path).expect("读得到"),
            entry.metadata().expect("读得到元数据").modified().ok(),
        ));
    }
    out.sort();
    out
}
