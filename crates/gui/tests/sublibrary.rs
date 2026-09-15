//! **子库屏**：一台设备一张卡、**选择集只读**（只有超限时删减建议上的「排除」记得下一条例外）、
//! 排差量预览、同步从这里触发。
//!
//! 这几条是这张票最要紧的纪律，而它们都是「不这么做会出事」而不是「这样比较好看」：
//!
//! - **这一屏不增删规则**（票 `gui-redesign/11`）：规则与例外的增减在浏览屏上做，
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
use romcat_core::report::{decimal_bytes, human_bytes};
use romcat_core::scan::{self, Jobs, ScanOptions};
use romcat_core::site::Site;
use romcat_core::sublibrary::{Exception, Group, Join, Rule};
use romcat_core::task::{Ending, Handle};
use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};
use romcat_gui::app::{App, View};
use romcat_gui::headless;

mod shared;
use shared::{占位活, 悬停在, 正好那一段画在哪儿, 点一下, 画出来的字};

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

/// **没开跑的那一下不许在任务历史里留一条**（票 `gui-looks-like-the-design/07`）：任务历史眼下
/// 几条，与按下去之前数的一样。多出来的话，把多出来的那一条怎么收的场一并印出来。
fn 历史没多一条(场: &现场, 之前: usize) {
    assert_eq!(
        场.app.tasks().history().len(),
        之前,
        "没开跑的那一下在任务历史里多了一条：{:?}",
        场.app
            .tasks()
            .history()
            .first()
            .map(|record| record.ending.render()),
    );
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
            let mut catalog = Catalog::create(&库文件, "fixture").expect("能开中立库");
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
fn 还没有子库时是空态加一颗新建子库_不是示例设备() {
    // 票 `gui-looks-like-the-design/20`：设计稿的脚本里摆着两台示例设备，那是给稿子看的数据，
    // 不是空库上该画的东西。一台都没有时屏上说「还没有子库」、说清子库是什么，给一颗「新建子库」。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    assert!(
        场.app.sublibrary().list().is_empty(),
        "前提：一个子库都没有"
    );

    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(屏上.contains("还没有子库"), "空态没画出来：\n{屏上}");
    assert!(
        屏上.lines().any(|line| line == "新建子库"),
        "空态上没有「新建子库」那一颗：\n{屏上}"
    );
    // **不是示例设备**：一张卡都没画——卡上才有的那几段一段都不在。
    // （「选择集」不在单子上：屏头那句说的是它在哪儿编辑。）
    for 卡上才有的 in ["条规则", "清单外文件", "生成差量预览"] {
        assert!(
            !屏上.contains(卡上才有的),
            "没有子库却画出了卡上的「{卡上才有的}」：\n{屏上}"
        );
    }

    // 空态卡底下照稿也是那一行帮助字。
    assert!(
        屏上.contains("空间不足时只给出删减建议"),
        "空态卡底下没有那一行帮助字：\n{屏上}"
    );

    // 按下去就是开始新建：打开「新建子库」那层弹层，草稿清空，没有哪一张卡算摊开着。
    场.app.sublibrary_and_site().0.form_mut().name = "上回没存的草稿".to_string();
    let 屏上 = 点一下(&ctx, "新建子库", |ui| 场.app.ui(ui));
    assert!(
        场.app.sublibrary().target_settings_open(),
        "按了「新建子库」没打开那层弹层"
    );
    assert!(
        屏上.lines().any(|line| line == "创建子库"),
        "弹层上没有「创建子库」那一颗：\n{屏上}"
    );
    assert!(
        场.app.sublibrary_and_site().0.form_mut().name.is_empty(),
        "按了「新建子库」，草稿还是上回那份"
    );
    assert_eq!(场.app.sublibrary().picked(), None);
}

#[test]
fn 目标设置在弹层里_卡上按目标设置打开这一台_保存之后弹层关上卡上跟着变() {
    // 票 `gui-looks-like-the-design/20` 第二段：拿主意的人看稿，底下那块「配目标」面板不清楚是干什么的——
    // 去掉，表单搬进弹层（`crate::dialog`）。字段与存法原样搬来。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "1GB");
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(!屏上.contains("配目标"), "底下那块面板还在：\n{屏上}");
    assert!(!场.app.sublibrary().target_settings_open());

    let 屏上 = 点一下(&ctx, "目标设置…", |ui| 场.app.ui(ui));
    assert!(
        屏上.contains("目标设置 · 掌机"),
        "没打开这一台的目标设置：\n{屏上}"
    );
    assert_eq!(
        场.app.sublibrary_and_site().0.form_mut().name,
        "掌机",
        "弹层里的草稿不是这一台的"
    );

    场.app.sublibrary_and_site().0.form_mut().capacity = "2GB".to_string();
    点正好那一段(&ctx, "保存", |ui| 场.app.ui(ui));
    assert!(
        !场.app.sublibrary().target_settings_open(),
        "存下来之后弹层还开着：{:?}",
        场.app.sublibrary().error(),
    );
    assert_eq!(
        场.app.sublibrary().list()[0].capacity,
        Some(2_000_000_000),
        "改的容量上限没存进去"
    );
}

#[test]
fn 一台设备一张卡_卡头写清路径前端格式文件系统与能力档案() {
    // 票 `gui-looks-like-the-design/20`：卡头那一行是「这台设备是什么样的」。**文件系统跟着能力档案走**
    // （能力档案 = 平台矩阵 × 文件系统，ADR-0017）——所以两台挑两份不同的档案，那一格也得跟着不同。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "1GB");
    let 目标 = romcat_core::path::display(场.卡.path());
    {
        let (screen, site) = 场.app.sublibrary_and_site();
        let form = screen.form_mut();
        form.name = "备份卡".to_string();
        form.target.clone_from(&目标);
        form.capability = "retroarch-fat32".to_string();
        screen.save(site);
        assert!(screen.error().is_none(), "{:?}", screen.error());
    }

    let 屏上 = 画两帧(&ctx, &mut 场);
    for 名字 in ["掌机", "备份卡"] {
        assert!(
            屏上.lines().any(|line| line == 名字),
            "「{名字}」那张卡没画出来：\n{屏上}"
        );
    }
    let 卡头: Vec<&str> = 屏上
        .lines()
        .filter(|line| line.contains("能力档案："))
        .collect();
    assert_eq!(卡头.len(), 2, "两台设备该是两行卡头：\n{屏上}");
    // 没挑过档案的那台走「不作声称」，它的文件系统是「无限制」（内置名册 `profiles.toml`）。
    for (档案, 文件系统) in [("不作声称", "无限制"), ("retroarch-fat32", "FAT32")] {
        assert!(
            卡头.iter().any(|line| line.contains(&目标)
                && line.contains("Pegasus")
                && line.contains(文件系统)
                && line.contains(档案)),
            "没有哪一行卡头同时写着路径、前端格式、{文件系统}、{档案}：\n{}",
            卡头.join("\n"),
        );
    }
}

#[test]
fn 规则列表逐条写名称条件命中数与大小_合计写明去掉了几个规则之间重叠的() {
    // 票 `gui-looks-like-the-design/20`：每张卡上摆着这台设备的选择集——每条规则的名称、条件、
    // 命中多少、多大；合计是去重之后的，并写明去掉了几个重复。**命中数与大小每条各自算**
    // （不扣例外、不扣与别条的重叠），加起来多于合计，多出来的正是那几个重叠的。屏上的话照设计稿
    // （拿主意的人定）：「规则之间没有重复」里的「重复」说的是几条规则选中了同一个变体，不是**重复拷贝**。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.建子库("备份卡", "");
    场.加规则("备份卡", "平台=GBA");
    场.加规则("掌机", "平台=SFC");
    场.加规则("掌机", "平台=SFC,GBA");
    场.求值();

    // 各变体多大从盘上量：fixture 上每个成员都读得到，选中容量这个下界（ADR-0021）正好就是它。
    let 多大 = |相对: &str| fs::metadata(场.库.path().join(相对)).expect("在").len();
    let 幻想 = 多大("SFC/幻想传说 汉化版.zip");
    let 圣剑 = 多大("SFC/圣剑传说 3 汉化版.zip");
    let 口袋 = 多大("GBA/口袋妖怪 绿宝石.zip");

    let 屏上 = 画两帧(&ctx, &mut 场);
    for 那一行 in [
        "选择集 · 2 条规则".to_string(),
        // 标题是从条件拼出来的短名（`Rule::label`，设计稿 `autoName`），第二行是条件原文。只有平台一个子句
        // 的规则只写平台，不补「全部」（拿主意的人看 `snap-5` 候选图时点名）。
        "SFC".to_string(),
        "平台=SFC".to_string(),
        format!("2 个 · {}", human_bytes(幻想 + 圣剑)),
        "SFC、GBA".to_string(),
        "平台=SFC,GBA".to_string(),
        format!("3 个 · {}", human_bytes(幻想 + 圣剑 + 口袋)),
        format!("合计 3 个变体 · {}", human_bytes(幻想 + 圣剑 + 口袋)),
        "已去除 2 个被多条规则同时选中的变体".to_string(),
        // 没摊开的那一张也摆得出它自己的选择集。
        "选择集 · 1 条规则".to_string(),
        "GBA".to_string(),
        "平台=GBA".to_string(),
        format!("1 个 · {}", human_bytes(口袋)),
        format!("合计 1 个变体 · {}", human_bytes(口袋)),
        "规则之间没有重复".to_string(),
    ] {
        assert!(
            屏上.lines().any(|line| line == 那一行),
            "屏上没有「{那一行}」这一行：\n{屏上}"
        );
    }
}

#[test]
fn 容量条三段照词表画_没看过目标时清单之外画成未知而不是零() {
    // 词表**容量条**：选中（只问中立库，卡不在手边也算得出）、清单之外（要目标在位才知道）、上限。
    // 没看过目标时清单之外是「还不知道」——画成 0 的话，人会以为卡上是空的。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "1GB");
    场.加规则("掌机", "平台=SFC");
    let 选中: u64 = ["SFC/幻想传说 汉化版.zip", "SFC/圣剑传说 3 汉化版.zip"]
        .iter()
        .map(|相对| fs::metadata(场.库.path().join(相对)).expect("在").len())
        .sum();
    // 图例的字照稿（拿主意的人定）：「已选 / 清单外文件 / 容量上限」；容量上限写十进制（挂单 `Q856`）。
    let 上限 = format!("容量上限 {}", decimal_bytes(1_000_000_000));

    // 一、卡不在手边：算一遍容量，选中照样算得出，清单之外画成未知。
    let 卡路径 = 场.卡.path().to_path_buf();
    let 拔下来放在 = 卡路径.with_extension("拔了");
    fs::rename(&卡路径, &拔下来放在).expect("拔得下来");
    场.求值();
    let 屏上 = 画两帧(&ctx, &mut 场);
    for 那一行 in [
        format!("已选 {}（2 个变体）", human_bytes(选中)),
        "清单外文件：未知".to_string(),
        上限.clone(),
    ] {
        assert!(
            屏上.lines().any(|line| line == 那一行),
            "容量条图例里没有「{那一行}」：\n{屏上}"
        );
    }
    assert!(
        屏上.contains("未知不代表为零"),
        "没说清未知不是零：\n{屏上}"
    );
    assert!(
        !屏上.lines().any(|line| line.starts_with("清单外文件 0")),
        "没看过目标，清单之外却画成了零：\n{屏上}"
    );

    // 二、插回来再算一遍：清单之外就是维护者那份存档多大。
    fs::rename(&拔下来放在, &卡路径).expect("插得回去");
    场.求值();
    let 存档多大 = fs::metadata(卡路径.join(存档)).expect("在").len();
    let 屏上 = 画两帧(&ctx, &mut 场);
    let 看过之后 = format!("清单外文件 {}", human_bytes(存档多大));
    assert!(
        屏上.lines().any(|line| line == 看过之后),
        "看过目标之后清单之外没写那个数「{看过之后}」：\n{屏上}"
    );
    assert!(
        !屏上.contains("清单外文件：未知"),
        "看过目标了还说未知：\n{屏上}"
    );
    assert!(
        屏上.lines().any(|line| line == 上限),
        "上限那一段没了：\n{屏上}"
    );
}

/// 滚一下卡片那一列：指针移过去、发一次滚轮，**等滚动停下**，再把指针挪走。
///
/// `dy` 正数把内容往下推（看上面的），负数往上推（看下面的），与 `egui::Event::MouseWheel` 同号。
/// egui 的滚轮带平滑，一下要分几帧走完：跑帧跑到 `InputState::is_scrolling` 说停了为止，不看挂钟。
/// 指针要挪走：停在按钮上会冒悬停说明，混进屏上的字。
fn 滚一下(ctx: &egui::Context, 场: &mut 现场, dy: f32) {
    // 卡片那一列里的一点：左边那一列、屏头之下。
    let 卡片那一列 = egui::pos2(headless::VIEWPORT[0] / 4.0, headless::VIEWPORT[1] / 3.0);
    let mut input = headless::input();
    input.events.push(egui::Event::PointerMoved(卡片那一列));
    input.events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, dy),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    headless::frame(ctx, input, |ui| 场.app.ui(ui));
    for _ in 0..240 {
        if !ctx.input(|input| input.is_scrolling()) {
            break;
        }
        headless::frame(ctx, headless::input(), |ui| 场.app.ui(ui));
    }
    let mut input = headless::input();
    input.events.push(egui::Event::PointerGone);
    headless::frame(ctx, input, |ui| 场.app.ui(ui));
}

/// 把卡片那一列**先滚回顶上，再一截一截往下滚**，直到这一帧画出来的字里有一行认得下；交出那一帧的字。
///
/// 删减建议表在卡片的下半截：票 05 把按钮照稿加高之后，1280×800 那个视口装不下整张卡，而视口外的
/// 行不画——测「表里写着什么」得先像人一样把它滚进来，不靠「一屏碰巧摆得下」。每一截比卡片那一列的
/// 视口矮，任何一行往上走的路上都会在视口里停过一帧。
///
/// # Panics
/// 滚到底都没有认得下的那一行时当场炸，并把最后一帧的字印出来。
fn 滚到看得见(ctx: &egui::Context, 场: &mut 现场, 认: impl Fn(&str) -> bool) -> String {
    const 一截: f32 = 200.0;
    滚一下(ctx, 场, 100_000.0);
    let mut 屏上 = 画两帧(ctx, 场);
    for _ in 0..60 {
        if 屏上.lines().any(&认) {
            return 屏上;
        }
        滚一下(ctx, 场, -一截);
        屏上 = 画两帧(ctx, 场);
    }
    panic!("滚到底都没有认得下的那一行：\n{屏上}");
}

/// 按一下屏上**正好**写着 `那一段` 的地方（整段一字不差），返回松开之后再画一帧画出来的字。
///
/// 「排除」这颗按钮上的两个字，也是屏上别的句子里的一截；`shared::点一下` 认「含有」，
/// 会点到先画出来的那句话上。走法与它一样：移过去、按下、松开、再画一帧。
fn 点正好那一段(
    ctx: &egui::Context,
    那一段: &str,
    画一帧: impl FnMut(&mut egui::Ui),
) -> String {
    按在(ctx, 那一段, 正好那一段画在哪儿, 画一帧)
}

/// 按一下屏上**正好**写着 `那一段` 的**最后一处**（按画出来的次序）。
///
/// 弹层盖在屏上面、画在最后：卡片底下与弹层页脚上都有一颗「删除子库」时，要按的是弹层上那一颗。
fn 点最后正好那一段(
    ctx: &egui::Context,
    那一段: &str,
    画一帧: impl FnMut(&mut egui::Ui),
) -> String {
    按在(ctx, 那一段, 最后一处正好画着, 画一帧)
}

/// 照 `找` 在头一帧上认出来的那一处按一下：移过去、按下、松开，返回再画一帧画出来的字。
fn 按在(
    ctx: &egui::Context,
    那一段: &str,
    找: fn(&egui::FullOutput, &str) -> Option<egui::Pos2>,
    mut 画一帧: impl FnMut(&mut egui::Ui),
) -> String {
    let 头一帧 = headless::frame(ctx, headless::input(), &mut 画一帧);
    let Some(位置) = 找(&头一帧, 那一段) else {
        panic!(
            "屏上没有正好写着「{那一段}」的地方，没处点：\n{}",
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
    画出来的字(&headless::frame(ctx, headless::input(), &mut 画一帧))
}

/// 屏上**正好**写着 `那一段`、按画出来的次序**最后**那一处的中心点。
fn 最后一处正好画着(output: &egui::FullOutput, 那一段: &str) -> Option<egui::Pos2> {
    fn 找(shape: &egui::epaint::Shape, 那一段: &str, 最后: &mut Option<egui::Pos2>) {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text() == 那一段 => {
                *最后 = Some(egui::Rect::from_min_size(text.pos, text.galley.size()).center());
            }
            egui::epaint::Shape::Vec(shapes) => {
                for one in shapes {
                    找(one, 那一段, 最后);
                }
            }
            _ => {}
        }
    }
    let mut 最后 = None;
    for clipped in &output.shapes {
        找(&clipped.shape, 那一段, &mut 最后);
    }
    最后
}

/// 这一台眼下「算一遍容量」算出来的那笔账；卡不在手边、或者还没算过时当场炸。
fn 那笔账(场: &现场, name: &str) -> romcat_core::sublibrary::Room {
    场.app
        .sublibrary()
        .evaluated(name)
        .and_then(|report| report.fit.known())
        .unwrap_or_else(|| panic!("「{name}」的装不装得下没算出来"))
        .clone()
}

#[test]
fn 超限时删减建议表四个数齐_排除记为这个子库的手动例外_一个文件都不删() {
    // 票 `gui-looks-like-the-design/20`、ADR-0016：**超限只给建议，绝不自动删减。**建议表说清每项释放多少、
    // 累计多少、排除到哪一项就放得下、全排除也还差多少；按「排除」记成这个子库的一条手动例外，
    // 主库与卡上一个字节都不动。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("小卡", "");
    场.加规则("小卡", "平台=SFC,GBA");
    let 多大 = |相对: &str| fs::metadata(场.库.path().join(相对)).expect("在").len();
    let 圣剑 = 多大("SFC/圣剑传说 3 汉化版.zip");
    let 幻想 = 多大("SFC/幻想传说 汉化版.zip");
    let 口袋 = 多大("GBA/口袋妖怪 绿宝石.zip");
    let 最大的 = "库/SFC/圣剑传说 3 汉化版.zip";
    let 第二大的 = "库/SFC/幻想传说 汉化版.zip";
    // 同步完之后卡上占多少由核心算（目标现占 ＋ 净变化）。先不设限算一遍，拿它来定上限。
    场.求值();
    let 同步之后 = 那笔账(&场, "小卡").after_bytes;
    let 行号 = |屏上: &str, 那一行: &str| 屏上.lines().position(|line| line == 那一行);

    // 一、上限正好比同步之后少「最大那一个」那么多：排除到头一项就放得下。
    场.建子库("小卡", &format!("{}B", 同步之后 - 圣剑));
    场.求值();
    assert_eq!(
        那笔账(&场, "小卡").over_capacity,
        Some(圣剑),
        "前提：超出量正好是最大那一个"
    );
    // 表在卡片下半截，按钮照稿加高之后常常落在视口底下：滚到表里最后一行（累计到三个）看得见为止，
    // 表头与前几行在同一帧里。
    let 累计到三个 = human_bytes(圣剑 + 幻想 + 口袋);
    let 屏上 = 滚到看得见(&ctx, &mut 场, |line| line == 累计到三个);
    for 那一行 in [
        format!("超出容量上限 {}", human_bytes(圣剑)),
        "释放".to_string(),
        "累计".to_string(),
        最大的.to_string(),
        第二大的.to_string(),
        human_bytes(圣剑 + 幻想),
        human_bytes(圣剑 + 幻想 + 口袋),
    ] {
        assert!(
            屏上.lines().any(|line| line == 那一行),
            "删减建议表里没有「{那一行}」：\n{屏上}"
        );
    }
    let 放得下 = 行号(&屏上, "排除到这一项就能放下").expect("没说排除到哪一项就放得下");
    assert!(
        行号(&屏上, 最大的).expect("在") < 放得下 && 放得下 < 行号(&屏上, 第二大的).expect("在"),
        "「排除到这一项就能放下」该紧跟在头一项后面：\n{屏上}"
    );
    assert!(!屏上.contains("全部排除也还差"), "放得下却说还差：\n{屏上}");

    // 二、上限只有 1 字节：建议里那几个全排除也放不下，说清还差多少。卡上那份存档与元数据
    // 不在建议里（它们不是变体），差的正是它们。
    场.建子库("小卡", "1B");
    场.求值();
    let 还差 = format!(
        "全部排除也还差 {}",
        human_bytes(同步之后 - 1 - 圣剑 - 幻想 - 口袋)
    );
    // 「还差」那一句在表底下：滚到它为止。
    let 屏上 = 滚到看得见(&ctx, &mut 场, |line| line.contains(&还差));
    assert!(
        屏上.contains(&还差),
        "没说全排除也还差多少「{还差}」：\n{屏上}"
    );
    assert!(
        !屏上.contains("排除到这一项就能放下"),
        "全排除都放不下，却说排除到某一项就放得下：\n{屏上}"
    );
    // 屏上那一行不在，也可能只是滚出了视口：说了算的那一处一并问一遍。
    assert_eq!(
        那笔账(&场, "小卡").fits_after(),
        None,
        "全排除都放不下，核心却说排除到某一项就放得下"
    );

    // 三、回到情形一，按头一行的「排除」。
    场.建子库("小卡", &format!("{}B", 同步之后 - 圣剑));
    场.求值();
    // 那颗「排除」在表里：先滚到头一行看得见，再按。
    滚到看得见(&ctx, &mut 场, |line| line == 最大的);
    let 主库之前 = 目录树(场.库.path(), &[]);
    let 卡上之前 = 卡上有什么(场.卡.path());
    点正好那一段(&ctx, "排除", |ui| 场.app.ui(ui));
    场.等任务跑完();

    let 例外 = 场
        .app
        .site()
        .catalog
        .selection("小卡")
        .expect("读得回来")
        .selection
        .exceptions;
    assert!(
        例外
            .iter()
            .any(|row| row.variant_key == 最大的 && row.kind == Exception::Exclude),
        "按了排除，却没记成这个子库的排除例外：{例外:?}"
    );
    assert_eq!(目录树(场.库.path(), &[]), 主库之前, "主库动了——ADR-0004");
    assert_eq!(
        卡上有什么(场.卡.path()),
        卡上之前,
        "卡上动了——超限只建议，不删"
    );
    // 排除之后那笔账由核心重算：少了最大那一个，放得下了，建议表跟着收掉。
    // 手动例外那一行在卡片上半截，而视口还停在表那儿：滚回来找。
    let 屏上 = 滚到看得见(&ctx, &mut 场, |line| line.contains("排除 1 条"));
    assert!(
        屏上.contains("排除 1 条"),
        "卡上的手动例外那一行没跟着变：\n{屏上}"
    );
    assert!(
        !屏上.contains("超出容量上限"),
        "排除之后没重算，建议表还挂着：\n{屏上}"
    );
    assert_eq!(那笔账(&场, "小卡").over_capacity, None);
}

#[test]
fn 没摊开的那一张也画得出自己的容量条() {
    // 设计稿一台设备一张卡，每张卡上都有容量条：不是先点开哪一张才看得见它装不装得下。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("备份卡", "2GB");
    场.建子库("掌机", "1GB");
    场.加规则("备份卡", "平台=GBA");
    场.加规则("掌机", "平台=SFC");
    场.求值();
    assert_eq!(
        场.app.sublibrary().picked(),
        Some("掌机"),
        "前提：摊开的是掌机"
    );

    let 屏上 = 画两帧(&ctx, &mut 场);
    for 上限 in [1_000_000_000_u64, 2_000_000_000] {
        let 那一行 = format!("容量上限 {}", decimal_bytes(上限));
        assert!(
            屏上.lines().any(|line| line == 那一行),
            "有一张卡没画出容量条（没有「{那一行}」）：\n{屏上}"
        );
    }
    assert!(
        !屏上.contains("摊开"),
        "卡上还要人先摊开才看得见容量：\n{屏上}"
    );
}

#[test]
fn 容量条三段各自标得出数() {
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "1GB");
    场.加规则("掌机", "平台=SFC");

    // **清单之外要目标设备在位才知道**——卡就在手边，算一遍容量时就看过了。
    // （卡不在手边时它是「还不知道」：见 `卡不在手边算得出选中多少_…`。）
    场.求值();
    let gauge = 场.app.sublibrary().gauge("掌机");
    assert_eq!(gauge.capacity, Some(1_000_000_000), "上限那一段没数");
    assert!(gauge.picked > 0, "选中那一段没数");
    let room = 场
        .app
        .sublibrary()
        .evaluated("掌机")
        .and_then(|report| report.fit.known())
        .expect("卡在手边，装不装得下该算得出")
        .clone();
    assert_eq!(
        gauge.strangers,
        Some(room.stranger_bytes),
        "清单之外那一段与报告里那个数不是同一个",
    );
    assert_eq!(
        gauge.taken(),
        room.after_bytes,
        "条子的总量与报告算超出量用的不是同一个数",
    );
    // 一成都不到：条子上选中那一段与上限的比例算得出来。
    assert!(gauge.picked_share() > 0.0 && gauge.picked_share() < 0.01);
    assert_eq!(gauge.scale(), 1_000_000_000, "没超限时条子照上限画满");

    // 排完差量：还是同一个数。
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
        plan_strangers, room.stranger_bytes,
        "算容量与排差量看见的不是同一张卡"
    );
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
    // 详情面板里才指得准；子库屏上只摆出来（删减建议上的「排除」另有一条测试钉着）。
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
        // 那句话与目标设置弹层里路径底下那一句是同一句（设计稿 `probePath`，票 `gui-looks-like-the-design/21`）。
        assert!(
            message.contains("只读的主库"),
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
    // 不在子库卡上：子库屏上没有增删规则的动作（票 `gui-redesign/11` 把规则增删整个搬去了浏览屏，
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
        卡上.contains("按上面「从浏览添加…」跳去浏览屏"),
        "卡上没写清下一步在哪儿——「看不出下一步」正是挂单 Q86 里最贵的那一半：{卡上}",
    );
    assert!(
        !卡上.contains("扔掉这条"),
        "子库屏上长出了一颗删规则的按钮——票 11 把规则增删搬去浏览屏那一条破了：{卡上}",
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
fn 卡不在位时按排差量预览_屏上说清插上读卡器或改目标路径_任务历史不多一条() {
    // 票 `gui-looks-like-the-design/07`：**卡没插是按下去之前就判得出的**——盘上缺一样东西，
    // 查一眼那个目录在不在（ADR-0005 修订段「原料还没备齐」）。排上去再在「看一眼目标」那一下
    // 报失败的话，任务历史里就多一条压根没开跑的「失败」，人会去找哪儿坏了。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    fs::remove_dir_all(场.卡.path()).expect("删得掉");
    let 历史几条 = 场.app.tasks().history().len();

    场.排预览();
    let 屏上 = 画两帧(&ctx, &mut 场);
    let screen = 场.app.sublibrary();
    let 说的 = screen.error().expect("卡不在位该说出口");
    assert!(说的.contains("未连接"), "没说清为什么不行：{说的}");
    assert!(说的.contains("插上读卡器"), "没说去哪儿办：{说的}");
    assert!(说的.contains("目标路径"), "没说去哪儿办：{说的}");
    assert!(
        屏上.contains(说的),
        "那句话没画在屏上：{说的}\n屏上：\n{屏上}"
    );
    assert!(screen.prepared().is_none(), "卡不在位却排出了一份差量");
    历史没多一条(&场, 历史几条);
}

#[test]
fn 没摊开的卡上按排差量预览_卡不在位时照旧只在屏上说_任务历史不多一条() {
    // 票 `gui-looks-like-the-design/07` 的拒绝重排之后不许丢：每张卡底下都摆着一颗「生成差量预览」，
    // 没摊开的那一张按下去先换成那一台，排之前照旧查一眼目标在不在位——不排、只在屏上说。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    let 没插上 = romcat_core::path::display(&场.卡.path().with_extension("没插上"));
    {
        let (screen, site) = 场.app.sublibrary_and_site();
        let form = screen.form_mut();
        form.name = "备份卡".to_string();
        form.target = 没插上;
        form.capacity = String::new();
        screen.save(site);
        assert!(screen.error().is_none(), "{:?}", screen.error());
    }
    // 摊开的是掌机，而且排过一份差量。两张卡上各有一颗「生成差量预览」；卡按名字排，先画出来的是
    // 左栏备份卡那一颗。
    场.摊开("掌机");
    场.排预览();
    assert!(
        场.app.sublibrary().prepared().is_some(),
        "前提：掌机的差量排出来了：{:?}",
        场.app.sublibrary().error(),
    );
    画两帧(&ctx, &mut 场);
    let 历史几条 = 场.app.tasks().history().len();

    let 屏上 = 点正好那一段(&ctx, "生成差量预览", |ui| 场.app.ui(ui));
    let screen = 场.app.sublibrary();
    assert_eq!(screen.picked(), Some("备份卡"), "按的是备份卡那一颗");
    let 说的 = screen.error().expect("卡不在位该说出口");
    assert!(说的.contains("未连接"), "没说清为什么不行：{说的}");
    assert!(说的.contains("插上读卡器"), "没说去哪儿办：{说的}");
    assert!(
        屏上.contains(说的),
        "那句话没画在屏上：{说的}\n屏上：\n{屏上}"
    );
    assert!(screen.prepared().is_none(), "卡不在位却摆着一份差量");
    assert!(!场.app.tasks().busy(), "卡不在位却往任务台上排了活");
    历史没多一条(&场, 历史几条);
}

#[test]
fn 目标路径上是一份文件时排差量预览照旧排上去_任务历史记失败() {
    // 票 `gui-looks-like-the-design/07` 的另一半：**跑了没成照旧进历史，收场是「失败」。**
    // 那条路径上**有东西**，按下去之前查一眼看不出缺什么；它不是目录，是「看一眼目标」那一下
    // 真去列它才撞上的——那一趟开跑过，照实记失败、说清为什么。与上面那条分开的正是这一下。
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    let 卡路径 = 场.卡.path().to_path_buf();
    fs::remove_dir_all(&卡路径).expect("删得掉");
    fs::write(&卡路径, "一份文件，不是读卡器挂上来的目录").expect("写得进");
    let 历史几条 = 场.app.tasks().history().len();

    场.排预览();

    assert_eq!(
        场.app.tasks().history().len(),
        历史几条 + 1,
        "真跑过的那一趟没进任务历史",
    );
    let record = &场.app.tasks().history()[0];
    let Ending::Failed { step, why } = &record.ending else {
        panic!("目标列不开却把这一趟记成了「{}」", record.ending.render());
    };
    assert_eq!(step, "看一眼目标", "说不清停在哪一步");
    assert!(why.contains("列不开"), "说不清为什么没成：{why}");
    let 说的 = 场.app.sublibrary().error().expect("失败要说出来");
    assert!(说的.contains(&record.ending.render()), "{说的}");
    assert!(
        场.app.sublibrary().prepared().is_none(),
        "目标列不开却排出了一份差量"
    );
    fs::remove_file(&卡路径).expect("删得掉");
}

#[test]
fn 排过差量预览之后卡拔了再按同步_屏上说清插上读卡器_任务历史不多一条_也不在原处建目录() {
    // 同上，按的是同步。差量预览排出来之后卡被拔了：再按同步，**不排**。排上去的话同步那一趟
    // 起手就把目标根建出来——卡拔了之后那个路径指着的是本机的盘，于是一份子库被悄悄写进了
    // 本机一个新建的空目录里。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    场.排预览();
    assert!(
        场.app.sublibrary().prepared().is_some(),
        "前提：差量预览排出来了：{:?}",
        场.app.sublibrary().error(),
    );
    fs::remove_dir_all(场.卡.path()).expect("删得掉");
    let 历史几条 = 场.app.tasks().history().len();

    场.同步到底();
    let 屏上 = 画两帧(&ctx, &mut 场);
    let 说的 = 场.app.sublibrary().error().expect("卡不在位该说出口");
    assert!(说的.contains("未连接"), "没说清为什么不行：{说的}");
    assert!(说的.contains("插上读卡器"), "没说去哪儿办：{说的}");
    assert!(
        屏上.contains(说的),
        "那句话没画在屏上：{说的}\n屏上：\n{屏上}"
    );
    历史没多一条(&场, 历史几条);
    assert!(!场.卡.path().exists(), "卡不在位，却在原处建出了目录");
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

    // **摊开之后那张表真的画了几行**，而且一步都没截：这一份计划几步就画几行。
    // 靠的不是「一屏碰巧摆得下」：摊开那一下把表滚到视口顶上，上面那几段多高都不影响。
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
fn 卡不在手边算得出选中多少_装不装得下如实说算不出() {
    // 子库是**持久实体**，不是「插上卡才存在的东西」（ADR-0015）：卡不在手边照样
    // 看得见选中多少。而**装不装得下**比的是目标现占 ＋ 净变化，要看一眼目标——
    // 看不见就不给数，不拿选中容量去冒充（挂账 D76）。
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
    // 4KB 比 12KiB 小——可那只是选中容量；卡不在手边，现占算不出。
    match &report.fit {
        romcat_core::sublibrary::Fit::Unknown { why } => {
            assert!(why.contains("目标不在位"), "{why}");
        }
        romcat_core::sublibrary::Fit::Known(room) => {
            panic!("卡不在手边却给了一个数：{room:?}");
        }
    }
    // 条子照 `选中 = report.bytes` 画，清单之外是「还不知道」，不是零。
    let gauge = screen.gauge("小卡");
    assert_eq!(gauge.taken(), report.bytes);
    assert_eq!(
        gauge.strangers, None,
        "卡不在手边就说得出清单之外多大——那是编的"
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

    let 占位 = 占位活::排上(场.app.tasks_mut(), "占着位子");
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
    assert!(notice.contains("已取消"), "回执没说清是被停了：{notice}");
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
    占位.按停(场.app.tasks_mut());
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
    let 占位 = 占位活::排上(场.app.tasks_mut(), "占着位子");
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

    // 按停占位活，让排着的那一趟轮上、跑完。
    占位.按停(场.app.tasks_mut());
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

    let 占位 = 占位活::排上(场.app.tasks_mut(), "占着位子");
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
    占位.按停(场.app.tasks_mut());
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

    let 占位 = 占位活::排上(场.app.tasks_mut(), "占着位子");
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
    assert!(notice.contains("已取消"), "回执没说清是被停了：{notice}");
    assert_eq!(场.app.tasks().history()[0].ending, Ending::Stopped);
    assert_eq!(卡上有什么(场.卡.path()), 卡上原样, "按停了却动了卡上的文件");

    // 再算一次照样算得出来：它没有半截状态要收拾。
    占位.按停(场.app.tasks_mut());
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

    let 占位 = 占位活::排上(场.app.tasks_mut(), "占着位子");
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

    占位.按停(场.app.tasks_mut());
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
            .fit
            .known()
            .and_then(|room| room.over_capacity)
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

    let 占位 = 占位活::排上(场.app.tasks_mut(), "占着位子");
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

    占位.按停(场.app.tasks_mut());
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
    // **这儿用不了 `占位活` 那一招**（撤单那条测试用的是它）：台上摆着占位活时这一趟
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
        这一趟的收场.trim().starts_with("部分完成"),
        "任务屏历史那一行没说它留下了东西：{这一趟的收场}",
    );
    assert!(
        这一趟的收场.trim() != "完成",
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

#[test]
fn 卡底按删除子库先摊开再问一层_写明名字与目标路径_确认之后记录没了_按撤销原样回来() {
    // 票 `gui-looks-like-the-design/20` 第二段（拿主意的人 2026-09-14 定）：每张卡底下照稿一颗「删除子库」，按下去
    // 先摊开这张卡、弹一层确认（`crate::dialog`）——删的是这一份定义，设备上的文件一个字节都不动，名字与目标路径
    // 写明。删掉之后底边提示条上一颗「撤销」，按下去原样放回来（逐列一样由核心
    // `删掉子库交回整份_原样放回去之后与删之前逐列一样` 钉着）。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.建子库("备份卡", "");
    场.加规则("备份卡", "平台=GBA");
    场.加规则("备份卡", "平台=SFC");
    {
        let (_, site) = 场.app.sublibrary_and_site();
        site.catalog
            .set_exception(
                "备份卡",
                "库/SFC/幻想传说 汉化版.zip",
                Exception::Exclude,
                None,
            )
            .expect("例外写得进");
    }
    场.加规则("掌机", "平台=SFC");
    assert_eq!(
        场.app.sublibrary().picked(),
        Some("掌机"),
        "前提：摊开的是掌机"
    );
    let 规则 = 场
        .app
        .site()
        .catalog
        .sublibrary_rules("备份卡")
        .expect("读得动");
    let 例外 = 场
        .app
        .site()
        .catalog
        .sublibrary_exceptions("备份卡")
        .expect("读得动");
    assert_eq!((规则.len(), 例外.len()), (2, 1), "前提：两条规则一条例外");
    let 目标 = 场
        .app
        .sublibrary()
        .list()
        .iter()
        .find(|row| row.name == "备份卡")
        .expect("在")
        .target
        .clone();
    画两帧(&ctx, &mut 场);

    // 两张卡底下各一颗「删除子库」；卡按名字排，先画出来的是左栏备份卡那一颗。
    let 屏上 = 点正好那一段(&ctx, "删除子库", |ui| 场.app.ui(ui));
    assert_eq!(
        场.app.sublibrary().picked(),
        Some("备份卡"),
        "按下去没先摊开这一张"
    );
    assert!(场.app.sublibrary().delete_dialog_open(), "没弹那一层确认");
    assert!(
        屏上.contains("删除子库「备份卡」"),
        "弹层上没写名字：\n{屏上}"
    );
    assert!(
        屏上.contains(&目标),
        "弹层上没写目标路径「{目标}」：\n{屏上}"
    );
    assert!(屏上.contains("不会删除"), "没说设备上的文件不删：\n{屏上}");
    assert!(
        场.app
            .site()
            .catalog
            .sublibrary("备份卡")
            .expect("读得动")
            .is_some(),
        "还没确认就删了"
    );

    let 屏上 = 点最后正好那一段(&ctx, "删除子库", |ui| 场.app.ui(ui));
    assert!(!场.app.sublibrary().delete_dialog_open(), "删完弹层还开着");
    assert!(
        场.app
            .site()
            .catalog
            .sublibrary("备份卡")
            .expect("读得动")
            .is_none(),
        "确认之后记录还在"
    );
    assert!(
        场.app
            .site()
            .catalog
            .sublibrary_rules("备份卡")
            .expect("读得动")
            .is_empty(),
        "规则没跟着删"
    );
    assert_eq!(场.app.sublibrary().list().len(), 1, "卡片没少一张");
    assert!(
        屏上.contains("已删除子库「备份卡」，设备上的文件没有改动"),
        "提示条没摆出来：\n{屏上}"
    );
    assert_eq!(场.app.sublibrary().undo_pending(), Some("备份卡"));

    let 屏上 = 点正好那一段(&ctx, "撤销", |ui| 场.app.ui(ui));
    assert!(
        场.app.sublibrary().undo_pending().is_none(),
        "撤销之后还摆着撤销"
    );
    assert!(!屏上.contains("已删除子库"), "撤销之后提示条还在：\n{屏上}");
    assert!(
        屏上.lines().any(|line| line == "备份卡"),
        "撤销之后卡片没回来：\n{屏上}"
    );
    assert_eq!(
        场.app
            .site()
            .catalog
            .sublibrary_rules("备份卡")
            .expect("读得动"),
        规则,
        "规则没原样回来"
    );
    assert_eq!(
        场.app
            .site()
            .catalog
            .sublibrary_exceptions("备份卡")
            .expect("读得动"),
        例外,
        "例外没原样回来"
    );
}

#[test]
fn 删除子库那一层按取消_记录还在_也没有撤销() {
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    画两帧(&ctx, &mut 场);

    点正好那一段(&ctx, "删除子库", |ui| 场.app.ui(ui));
    assert!(
        场.app.sublibrary().delete_dialog_open(),
        "前提：那一层弹出来了"
    );
    let 屏上 = 点正好那一段(&ctx, "取消", |ui| 场.app.ui(ui));
    assert!(
        !场.app.sublibrary().delete_dialog_open(),
        "按了取消弹层还开着"
    );
    assert!(
        !屏上.contains("删除子库「掌机」"),
        "按了取消弹层还画着：\n{屏上}"
    );
    assert!(
        场.app
            .site()
            .catalog
            .sublibrary("掌机")
            .expect("读得动")
            .is_some(),
        "按了取消记录没了"
    );
    assert_eq!(
        场.app
            .site()
            .catalog
            .sublibrary_rules("掌机")
            .expect("读得动")
            .len(),
        1,
        "按了取消规则没了"
    );
    assert!(
        场.app.sublibrary().undo_pending().is_none(),
        "没删却摆着撤销"
    );
}

#[test]
fn 提示条停够了收起或者换到别的屏_撤销就没了() {
    // 拿主意的人 2026-09-14 定：删之前留下的那一份，只在撤销提示条还摆着的时候留着；提示条收起、或者换到别的屏，
    // 就丢掉——之后删除就是真删了。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.建子库("备份卡", "");
    场.加规则("掌机", "平台=SFC");
    {
        let (screen, site) = 场.app.sublibrary_and_site();
        screen.remove(site);
    }
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        屏上.contains("已删除子库「掌机」"),
        "前提：提示条摆着：\n{屏上}"
    );

    // 一、停够了：递一个远在后面的时刻进去（提示条按 egui 那一帧的时刻算，不看挂钟）。
    let mut 之后 = headless::input();
    之后.time = Some(3600.0);
    headless::frame(&ctx, 之后, |ui| 场.app.ui(ui));
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(!屏上.contains("已删除子库"), "停够了提示条还在：\n{屏上}");
    assert!(
        场.app.sublibrary().undo_pending().is_none(),
        "提示条收起了还留着撤销"
    );
    {
        let (screen, site) = 场.app.sublibrary_and_site();
        screen.undo_remove(site);
    }
    assert!(
        场.app
            .site()
            .catalog
            .sublibrary("掌机")
            .expect("读得动")
            .is_none(),
        "提示条收起之后还恢复得了"
    );

    // 二、换到别的屏再回来。
    场.摊开("备份卡");
    {
        let (screen, site) = 场.app.sublibrary_and_site();
        screen.remove(site);
    }
    画两帧(&ctx, &mut 场);
    assert_eq!(
        场.app.sublibrary().undo_pending(),
        Some("备份卡"),
        "前提：撤销还摆着"
    );
    场.app.show_view(View::Tasks);
    画两帧(&ctx, &mut 场);
    场.app.show_view(View::Sublibraries);
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        !屏上.contains("已删除子库"),
        "换过屏回来提示条还在：\n{屏上}"
    );
    assert!(
        场.app.sublibrary().undo_pending().is_none(),
        "换过屏还留着撤销"
    );
    {
        let (screen, site) = 场.app.sublibrary_and_site();
        screen.undo_remove(site);
    }
    assert!(
        场.app
            .site()
            .catalog
            .sublibrary("备份卡")
            .expect("读得动")
            .is_none(),
        "换过屏之后还恢复得了"
    );
}

#[test]
fn 目标设置原样保存_容量上限的字节数一个都不变() {
    // 拿主意的人 2026-09-14 定：弹层里容量上限写一位小数的十进制（「511.1 GB」），字没改过就沿用原来的字节数——
    // 那串字读回来是 511,100,000,000，原样按保存不该悄悄改掉上限。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    // 带上单位 `B`：弹层里「自定义」那一格光写一个数时按 GB 读（旁边写着 GB），这里要的是精确的字节数。
    场.建子库("掌机", "511123456789B");
    画两帧(&ctx, &mut 场);

    点一下(&ctx, "目标设置…", |ui| 场.app.ui(ui));
    assert_eq!(
        场.app.sublibrary_and_site().0.form_mut().capacity,
        "511.1",
        "弹层里的容量上限不是一位小数的十进制 GB 数（GB 写在那一格后面）"
    );
    点正好那一段(&ctx, "保存", |ui| 场.app.ui(ui));
    assert!(
        !场.app.sublibrary().target_settings_open(),
        "存下来之后弹层还开着：{:?}",
        场.app.sublibrary().error(),
    );
    assert_eq!(
        场.app.sublibrary().list()[0].capacity,
        Some(511_123_456_789),
        "原样保存改掉了容量上限"
    );
}

#[test]
fn 同步完再排一趟一步都不用做_卡头写已同步和清单几条() {
    // 设计稿 `devState`（拿主意的人 2026-09-14 定）：排过差量、没什么要同步时，卡头那一枚写「已同步 · 清单 N 条」，
    // N 是这一台清单记着几条，从核心现成的读清单入口取。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    场.排预览();
    场.同步到底();
    assert!(
        场.app.sublibrary().error().is_none(),
        "{:?}",
        场.app.sublibrary().error()
    );
    场.排预览();
    let 还要动 = 场
        .app
        .sublibrary()
        .prepared()
        .expect("排得出来")
        .plan
        .touched();
    assert_eq!(还要动, 0, "前提：刚同步完，再排一趟一步都不用做");
    let 几条 = 场
        .app
        .site()
        .catalog
        .manifest("掌机")
        .expect("读得出清单")
        .files
        .len();
    assert!(几条 > 0, "前提：清单里记着东西");

    let 屏上 = 画两帧(&ctx, &mut 场);
    let 那一枚 = format!("已同步 · 清单 {几条} 条");
    assert!(
        屏上.lines().any(|line| line == 那一枚),
        "卡头没写「{那一枚}」：\n{屏上}"
    );
}

#[test]
fn 卡不在位时卡底只写请先连接设备_路径与怎么办在悬停里() {
    // 拿主意的人 2026-09-14 定（照稿）：卡底按钮旁边只写「请先连接设备」，目标路径与怎么办放进悬停；
    // 容量图例底下是稿上那一行普通小字。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    let 目标 = 场.app.sublibrary().list()[0].target.clone();
    fs::remove_dir_all(场.卡.path()).expect("删得掉");
    {
        let (screen, site) = 场.app.sublibrary_and_site();
        screen.reload(site);
    }

    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        屏上.lines().any(|line| line == "请先连接设备"),
        "卡底没写「请先连接设备」：\n{屏上}"
    );
    assert!(
        屏上.contains("设备未连接时仍可计算已选容量"),
        "图例底下没有稿上那一行：\n{屏上}"
    );
    assert!(!屏上.contains("插上读卡器"), "怎么办不该摆在卡上：\n{屏上}");
    let 悬停 = 悬停在(&ctx, "请先连接设备", |ui| 场.app.ui(ui));
    assert!(
        悬停.contains("插上读卡器") && 悬停.contains(&目标),
        "悬停里没写目标路径与怎么办：\n{悬停}"
    );
}

#[test]
fn 规则行按叉先问一层_确认之后这一条没了_别的规则与序号不动_容量账作废() {
    // 拿主意的人 2026-09-14 看子库屏候选图：「选择集中缺少编辑和删除按钮」。每条规则行尾照稿一颗「×」，按下去先弹一层
    // 确认（`crate::dialog`，确认按钮 `danger`），写清移除的是哪一条、选中的变体会变；确认之后走核心 `Catalog::remove_rule`。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    场.加规则("掌机", "平台=GBA");
    场.求值();
    画两帧(&ctx, &mut 场);

    // 规则按序号排，先画出来的是第 1 条（SFC）那一颗「×」。
    let 屏上 = 点正好那一段(&ctx, "×", |ui| 场.app.ui(ui));
    assert!(
        场.app.sublibrary().rule_dialog_open(),
        "按了「×」没弹那一层确认"
    );
    assert!(
        屏上.contains("移除规则「SFC」"),
        "弹层上没写移除的是哪一条：\n{屏上}"
    );
    assert!(
        屏上.contains("选中的变体会变"),
        "弹层上没说选中的变体会变：\n{屏上}"
    );
    assert_eq!(
        场.app
            .site()
            .catalog
            .sublibrary_rules("掌机")
            .expect("读得动")
            .len(),
        2,
        "还没确认就删了"
    );

    let 屏上 = 点最后正好那一段(&ctx, "移除这条规则", |ui| 场.app.ui(ui));
    assert!(!场.app.sublibrary().rule_dialog_open(), "删完弹层还开着");
    let 规则 = 场
        .app
        .site()
        .catalog
        .sublibrary_rules("掌机")
        .expect("读得动");
    assert_eq!(规则.len(), 1, "确认之后规则没少一条：{规则:?}");
    assert_eq!(
        (规则[0].ordinal, 规则[0].text.as_str()),
        (2, "平台=GBA"),
        "删掉的不是按的那一条，或者剩下那一条的序号变了"
    );
    assert!(
        场.app.sublibrary().evaluated("掌机").is_none(),
        "移除之后卡上还摆着按旧规则算的容量账"
    );
    assert!(
        !屏上.lines().any(|line| line == "SFC") && 屏上.lines().any(|line| line == "GBA"),
        "卡上的规则列表没跟着变：\n{屏上}"
    );
}

#[test]
fn 规则行按叉再按取消_规则一条都没少() {
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    场.加规则("掌机", "平台=GBA");
    let 之前 = 场
        .app
        .site()
        .catalog
        .sublibrary_rules("掌机")
        .expect("读得动");
    画两帧(&ctx, &mut 场);

    点正好那一段(&ctx, "×", |ui| 场.app.ui(ui));
    assert!(
        场.app.sublibrary().rule_dialog_open(),
        "前提：那一层弹出来了"
    );
    let 屏上 = 点正好那一段(&ctx, "取消", |ui| 场.app.ui(ui));
    assert!(
        !场.app.sublibrary().rule_dialog_open(),
        "按了取消弹层还开着"
    );
    assert!(!屏上.contains("移除规则「"), "按了取消弹层还画着：\n{屏上}");
    assert_eq!(
        场.app
            .site()
            .catalog
            .sublibrary_rules("掌机")
            .expect("读得动"),
        之前,
        "按了取消规则变了"
    );
}

#[test]
fn 规则行按铅笔只改这一条_更新到子库之后其余规则都在() {
    // 拿主意的人 2026-09-14 定：规则行尾「✎」跳去浏览屏只把这一条预填进筛选器，调完按「更新到子库」只换回这一条
    // （核心 `Catalog::replace_rule`），别的规则一条不碰、序号照旧。「✎」是线条画的，屏上没有字可认，这一下走界面上
    // 按那颗按钮走的那个函数。
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    场.加规则("掌机", "平台=GBA");
    场.app.sublibrary_and_site().0.edit_rule("掌机", 1);
    场.app.route();
    assert_eq!(场.app.view(), View::Browse, "按了「✎」没跳去浏览屏");
    let 屏上 = 场.屏上筛出来的();
    assert!(
        屏上.len() == 2 && 屏上.iter().all(|key| key.contains("/SFC/")),
        "筛选器里预填的不是只有第 1 条（平台=SFC）：{屏上:?}"
    );

    {
        let (browse, _) = 场.app.browse_and_site();
        browse.set_filter_rule(Some(Rule::parse("平台=SFC 或 平台=GBA").expect("读得懂")));
    }
    场.更新到子库();
    assert_eq!(场.app.view(), View::Sublibraries, "没跳回子库屏");
    let 规则: Vec<(i64, String)> = 场
        .app
        .site()
        .catalog
        .sublibrary_rules("掌机")
        .expect("读得动")
        .into_iter()
        .map(|stored| (stored.ordinal, stored.text))
        .collect();
    assert_eq!(
        规则,
        vec![
            (1, "平台=SFC 或 平台=GBA".to_string()),
            (2, "平台=GBA".to_string())
        ],
        "「✎」回来换掉的不只是那一条，或者序号变了"
    );
}

// ——— 新建子库与目标设置（票 `gui-looks-like-the-design/21`）———

#[test]
fn 新建子库时路径当场校验_主库里工作目录里被别的子库占着都说清原因_创建子库按不下() {
    // 设计稿 `probePath` 那三句，逐字照稿；判断在核心库（`sublibrary::target::vet`），这一层只画。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    let 库里 = romcat_core::path::display(&场.库.path().join("SFC"));
    let 工作目录里 = romcat_core::path::display(&场.工作区.path().join("子库"));
    let 掌机的 = romcat_core::path::display(场.卡.path());
    点一下(&ctx, "新建子库", |ui| 场.app.ui(ui));
    for (目标, 该说) in [
        (
            库里,
            "这个目录在主库的根之内。子库需要写入文件，不能放在只读的主库里。",
        ),
        (工作目录里, "这个目录属于工作目录，请选择其他目录。"),
        (掌机的, "已被子库「掌机」使用。"),
    ] {
        {
            let form = 场.app.sublibrary_and_site().0.form_mut();
            form.name = "新掌机".to_string();
            form.target.clone_from(&目标);
        }
        let 屏上 = 画两帧(&ctx, &mut 场);
        assert!(屏上.contains(该说), "路径 {目标} 该说「{该说}」：\n{屏上}");
        点最后正好那一段(&ctx, "创建子库", |ui| 场.app.ui(ui));
        assert!(
            场.app.sublibrary().target_settings_open(),
            "路径被拦下时按「创建子库」不该关上弹层"
        );
        assert_eq!(
            场.app.sublibrary().list().len(),
            1,
            "路径被拦下时不该建出第二台"
        );
    }
}

#[test]
fn 新建子库时名字已被别的子库用了_当场说出来_建不出同名的也不盖掉原来那一台() {
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "1GB");
    let 另一张 = temp_dir("gui-sub-card-2");
    点一下(&ctx, "新建子库", |ui| 场.app.ui(ui));
    {
        let form = 场.app.sublibrary_and_site().0.form_mut();
        form.name = "掌机".to_string();
        form.target = romcat_core::path::display(另一张.path());
    }
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        屏上.contains("已经有同名的子库。"),
        "重名要当场说出来：\n{屏上}"
    );
    点最后正好那一段(&ctx, "创建子库", |ui| 场.app.ui(ui));
    assert!(场.app.sublibrary().target_settings_open());
    let 掌机 = &场.app.sublibrary().list()[0];
    assert_eq!(
        (掌机.capacity, 掌机.target.as_str()),
        (
            Some(1_000_000_000),
            romcat_core::path::display(场.卡.path()).as_str()
        ),
        "原来那一台被盖掉了"
    );
}

#[test]
fn 选择目录交回来的路径与贴进框里走同一条路_取消什么都不动() {
    // 票 `gui-answers-all-six/01` 那条薄封装：对话框那一层不测，交回来之后的那一半在这儿测。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    let 库里 = 场.库.path().join("GBA");
    点一下(&ctx, "新建子库", |ui| 场.app.ui(ui));
    场.app
        .sublibrary_and_site()
        .0
        .picked_target(Some(库里.clone()));
    assert_eq!(
        场.app.sublibrary_and_site().0.form_mut().target,
        romcat_core::path::display(&库里),
        "选中的目录没填进框里"
    );
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        屏上.contains("这个目录在主库的根之内。"),
        "选中的目录没照贴路径那样当场校验：\n{屏上}"
    );
    场.app.sublibrary_and_site().0.picked_target(None);
    assert_eq!(
        场.app.sublibrary_and_site().0.form_mut().target,
        romcat_core::path::display(&库里),
        "取消选择器把框里的字动了"
    );
}

#[test]
fn 目标设置打开时去读这一台的选择集_平台表只列选择集里出现的平台() {
    // 设计稿「只列出这个子库选择集中出现的平台」。读选择集要折一遍全库事实，排上任务台，不在画帧线程上跑。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC,GBA");
    点一下(&ctx, "目标设置…", |ui| 场.app.ui(ui));
    画两帧(&ctx, &mut 场);
    场.等任务跑完();
    画两帧(&ctx, &mut 场);
    assert_eq!(
        场.app.sublibrary().target_platforms(),
        Some(vec!["GBA".to_string(), "SFC".to_string()]),
        "平台表该只列选择集里出现的 GBA 与 SFC"
    );
}

#[test]
fn 目标设置里按平台覆盖_保存之后存进中立库只影响这一台_重开读得回来() {
    use romcat_core::capability::Override;

    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    let 另一张 = temp_dir("gui-sub-card-2");
    {
        let (screen, site) = 场.app.sublibrary_and_site();
        let form = screen.form_mut();
        form.name = "备份卡".to_string();
        form.target = romcat_core::path::display(另一张.path());
        form.capacity = String::new();
        assert!(screen.save(site), "{:?}", screen.error());
    }
    画两帧(&ctx, &mut 场);
    场.app.sublibrary_and_site().0.edit_target("掌机");
    场.app
        .sublibrary_and_site()
        .0
        .form_mut()
        .overrides
        .insert("SFC".to_string(), Override::Unpack);
    // 弹层头一帧只量多大、不画出来（`crate::dialog`）：先画两帧再按页脚上那一颗。
    画两帧(&ctx, &mut 场);
    点最后正好那一段(&ctx, "保存", |ui| 场.app.ui(ui));
    assert!(
        !场.app.sublibrary().target_settings_open(),
        "存下来之后弹层还开着：{:?}",
        场.app.sublibrary().error()
    );
    let catalog = &场.app.site().catalog;
    assert_eq!(
        catalog.capability_overrides("掌机").expect("读得动"),
        std::collections::BTreeMap::from([("SFC".to_string(), Override::Unpack)])
    );
    assert!(
        catalog
            .capability_overrides("备份卡")
            .expect("读得动")
            .is_empty(),
        "覆盖只影响这个子库"
    );
    场.app.sublibrary_and_site().0.edit_target("掌机");
    assert_eq!(
        场.app
            .sublibrary_and_site()
            .0
            .form_mut()
            .overrides
            .get("SFC"),
        Some(&Override::Unpack),
        "重开目标设置时覆盖没读回来"
    );
}

#[test]
fn 前端格式照稿两格分段_界面写es_de_说明句照实际布局写真文件名() {
    // 拿主意的人 2026-09-15 定：界面上统一写「ES-DE」（适配器标识照旧是 `ES-Gamelist`）；说明句照实际布局写，
    // 文件名与目录取核心库那几个常量与 `Adapter::metadata_path`，不照稿上的示意。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    点一下(&ctx, "新建子库", |ui| 场.app.ui(ui));
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        屏上.lines().any(|line| line == "Pegasus") && 屏上.lines().any(|line| line == "ES-DE"),
        "分段那两格该写 Pegasus 与 ES-DE：\n{屏上}"
    );
    assert!(
        !屏上.contains("ES-Gamelist"),
        "界面上不该露出适配器标识：\n{屏上}"
    );
    assert!(
        屏上.contains(
            "每个平台一份，例如 GBA.metadata.pegasus.txt，摊在子库根上；媒体放在 media 目录。"
        ),
        "Pegasus 那一句该拿库里头一个平台举例、写真实的元数据文件名与媒体目录：\n{屏上}"
    );
    assert!(!屏上.contains("平台目录"), "说明句里不该裸写占位：\n{屏上}");

    点最后正好那一段(&ctx, "ES-DE", |ui| 场.app.ui(ui));
    assert_eq!(
        场.app.sublibrary_and_site().0.form_mut().format,
        "ES-Gamelist",
        "按「ES-DE」存的该是适配器标识"
    );
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        屏上.contains(
            "每个平台一份，例如 gamelists/GBA/gamelist.xml；媒体放在 downloaded_media 目录。"
        ),
        "ES-DE 那一句该拿库里头一个平台举例、写真实的 gamelist 位置与媒体目录：\n{屏上}"
    );
}

#[test]
fn 卡头的前端格式写es_de_不写适配器标识() {
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    {
        let (screen, site) = 场.app.sublibrary_and_site();
        let form = screen.form_mut();
        form.name = "掌机".to_string();
        form.target = romcat_core::path::display(场.卡.path());
        form.format = "ES-Gamelist".to_string();
        assert!(screen.save(site), "{:?}", screen.error());
    }
    let 屏上 = 画两帧(&ctx, &mut 场);
    let 卡头: Vec<&str> = 屏上
        .lines()
        .filter(|line| line.contains("能力档案："))
        .collect();
    assert!(
        卡头.iter().any(|line| line.contains(" · ES-DE · ")),
        "卡头该写 ES-DE：{卡头:?}"
    );
    assert!(
        !屏上.contains("ES-Gamelist"),
        "卡上露出了适配器标识：\n{屏上}"
    );
}

/// 往工作目录里放一份能力档案名册：照内置那一份原文，换掉 `从` 那一串（名册文件改一处就生效，`Roster::in_workspace`）。
fn 换一处名册(工作区: &Path, 从: &str, 换成: &str) {
    let 原文 = romcat_core::capability::Roster::builtin_text();
    assert!(原文.contains(从), "内置名册里没有「{从}」");
    fs::write(工作区.join("capability.toml"), 原文.replacen(从, 换成, 1)).expect("写得进");
}

/// 打开这一台的目标设置，等它的选择集读回来，钉死「今天」，交回画出来的字。
fn 开目标设置等选择集(ctx: &egui::Context, 场: &mut 现场, name: &str) -> String {
    场.app.sublibrary_and_site().0.set_today("2026-09-15");
    场.app.sublibrary_and_site().0.edit_target(name);
    画两帧(ctx, 场);
    场.等任务跑完();
    画两帧(ctx, 场)
}

#[test]
fn 能力档案表逐平台写吃什么转什么_每行核实日期_来源里说了的那句说明画出来() {
    // 拿主意的人 2026-09-15 定：每行一小行「核实日期 …」、陈旧时换成警示色「陈旧」；「说明」只画来源里真说了的。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC,GBA");
    {
        let (screen, site) = 场.app.sublibrary_and_site();
        screen.edit_target("掌机");
        screen.form_mut().capability = "retroarch-exfat".to_string();
        assert!(screen.save(site), "{:?}", screen.error());
    }
    let 屏上 = 开目标设置等选择集(&ctx, &mut 场, "掌机");
    for 该有 in [
        "GBA",
        "SFC",
        "卡带裸文件、zip、7z、zst、apk",
        "核实日期 2026-08-31",
        "按档案",
    ] {
        assert!(屏上.contains(该有), "平台表里没有「{该有}」：\n{屏上}");
    }
    assert!(!屏上.contains("陈旧"), "内置档案眼下不陈旧：\n{屏上}");

    // 独立模拟器那一份：SFC 那一条来源里说了 Snes9x 与 ares 不认 7z 与 rar。
    场.app.sublibrary_and_site().0.form_mut().capability = "独立模拟器-exfat".to_string();
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        屏上.contains("Snes9x 与 ares 不支持 7z 与 rar"),
        "来源里说了的那句说明没画出来：\n{屏上}"
    );
}

#[test]
fn 核实日期超过半年的声明在平台表上标陈旧() {
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    换一处名册(
        场.工作区.path(),
        "\"核实日期\" = \"2026-08-31\"",
        "\"核实日期\" = \"2020-01-01\"",
    );
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC,GBA");
    {
        let (screen, site) = 场.app.sublibrary_and_site();
        screen.edit_target("掌机");
        screen.form_mut().capability = "retroarch-exfat".to_string();
        assert!(screen.save(site), "{:?}", screen.error());
    }
    let 屏上 = 开目标设置等选择集(&ctx, &mut 场, "掌机");
    assert!(屏上.contains("陈旧"), "超过 180 天的声明没标出来：\n{屏上}");
}

#[test]
fn 新建时平台表按所选档案的条目列_不带覆盖列() {
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.app.sublibrary_and_site().0.set_today("2026-09-15");
    点一下(&ctx, "新建子库", |ui| 场.app.ui(ui));
    场.app.sublibrary_and_site().0.form_mut().capability = "retroarch-exfat".to_string();
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        屏上.contains("PS1"),
        "新建时该按档案的条目列出平台：\n{屏上}"
    );
    assert!(屏上.contains("核实日期 2026-08-31"), "\n{屏上}");
    assert!(!屏上.contains("按档案"), "新建时不该带覆盖那一列：\n{屏上}");
}

#[test]
fn fat32档案下选择集里有超过单文件上限的_当场提醒() {
    // ADR-0017 补充段：FAT32 那 4 GiB 放不进去的，挑档案时就说，不等差量预览。fixture 里的 zip 只有几 KiB，
    // 于是把名册里 FAT32 的单文件上限改小——判据照旧是核心那一处（`Footprint::too_big`）。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    换一处名册(
        场.工作区.path(),
        "\"单文件上限\" = 4294967295",
        "\"单文件上限\" = 3000",
    );
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    let 屏上 = 开目标设置等选择集(&ctx, &mut 场, "掌机");
    assert!(
        !屏上.contains("单文件上限"),
        "没挑 FAT32 的档案就不该提醒：\n{屏上}"
    );
    场.app.sublibrary_and_site().0.form_mut().capability = "retroarch-fat32".to_string();
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        屏上.contains("FAT32 单文件上限"),
        "选择集里有超过单文件上限的，挑 FAT32 的档案时该当场提醒：\n{屏上}"
    );
}

/// 当目标用的那张卡此刻卷的总量，十进制 GB 一位小数（与屏上「按设备容量（…）」同一个写法）。
fn 卡的总量(场: &mut 现场) -> u64 {
    use romcat_core::sublibrary::target::{self, Presence};
    let workspace = 场.工作区.path().to_path_buf();
    let catalog = &场.app.site().catalog;
    match target::vet(catalog, &workspace, Some("掌机"), 场.卡.path()).expect("读得动") {
        Ok(Presence::Present(volume)) => volume.total.expect("读得出总量"),
        other => panic!("卡插着：{other:?}"),
    }
}

#[test]
fn 容量上限照稿二选一_按设备容量写出卡的总量_存下来这一档与此刻的总量() {
    // 拿主意的人 2026-09-15 定：照稿「按设备容量 / 自定义」二选一，按设备容量那一档跟着设备总容量走。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "1GB");
    let 总量 = 卡的总量(&mut 场);
    场.app.sublibrary_and_site().0.edit_target("掌机");
    let 屏上 = 画两帧(&ctx, &mut 场);
    for 该有 in [
        "设备连接时自动读取",
        "自定义",
        "给存档、截图等留出空间",
        &format!("按设备容量（{}）", decimal_bytes(总量)),
    ] {
        assert!(
            屏上.contains(该有),
            "容量上限那一格没有「{该有}」：\n{屏上}"
        );
    }

    场.app.sublibrary_and_site().0.form_mut().capacity_by_device = true;
    点最后正好那一段(&ctx, "保存", |ui| 场.app.ui(ui));
    assert!(
        !场.app.sublibrary().target_settings_open(),
        "存下来之后弹层还开着：{:?}",
        场.app.sublibrary().error()
    );
    let 掌机 = &场.app.sublibrary().list()[0];
    assert!(掌机.capacity_by_device, "按设备容量那一档没存下来");
    assert_eq!(
        掌机.capacity,
        Some(总量),
        "卡插着时该把此刻的总量记成上次读到的"
    );

    场.app.sublibrary_and_site().0.edit_target("掌机");
    {
        let form = 场.app.sublibrary_and_site().0.form_mut();
        assert!(form.capacity_by_device, "重开时这一档没填回来");
        form.capacity_by_device = false;
        form.capacity = "58".to_string();
    }
    画两帧(&ctx, &mut 场);
    点最后正好那一段(&ctx, "保存", |ui| 场.app.ui(ui));
    let 掌机 = &场.app.sublibrary().list()[0];
    assert!(!掌机.capacity_by_device, "换回自定义没存下来");
    assert_eq!(
        掌机.capacity,
        Some(58_000_000_000),
        "自定义那一格照十进制 GB 读"
    );
}

#[test]
fn 按设备容量那一台卡没插_写上次读到的总量_没读过就说不设上限() {
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    {
        let (screen, site) = 场.app.sublibrary_and_site();
        let mut 掌机 = site
            .catalog
            .sublibrary("掌机")
            .expect("读得动")
            .expect("在");
        掌机.capacity_by_device = true;
        掌机.capacity = Some(64_000_000_000);
        掌机.target = "/Volumes/ROMCAT-NO-SUCH-CARD".to_string();
        掌机.target_raw = Some(掌机.target.clone());
        site.catalog.put_sublibrary(&掌机).expect("写得进");
        screen.reload(site);
        screen.edit_target("掌机");
    }
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        屏上.contains("按设备容量（64 GB）"),
        "卡没插时该写上次读到的总量：\n{屏上}"
    );
    场.app.sublibrary_and_site().0.leave_target_settings();
    {
        let (screen, site) = 场.app.sublibrary_and_site();
        let mut 掌机 = site
            .catalog
            .sublibrary("掌机")
            .expect("读得动")
            .expect("在");
        掌机.capacity = None;
        site.catalog.put_sublibrary(&掌机).expect("写得进");
        screen.reload(site);
        screen.edit_target("掌机");
    }
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        屏上.contains("按设备容量（没读过，不设上限）"),
        "没读过总量时该说不设上限：\n{屏上}"
    );
}

#[test]
fn 目标在位时照稿写已连接与容量可用_清单外文件数数完才画() {
    // 拿主意的人 2026-09-15 定：连接状态行照稿全写；清单外文件数是只读遍历目标、排上任务台，数出来之前那半句不画。
    // 卡上躺着维护者自己拷进去的那一份存档：清单里没有它，数出来是 1。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.app.sublibrary_and_site().0.edit_target("掌机");
    let 屏上 = 画两帧(&ctx, &mut 场);
    for 该有 in ["已连接", "容量 ", "可用 "] {
        assert!(屏上.contains(该有), "卡插着该写「{该有}」：\n{屏上}");
    }
    场.等任务跑完();
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        屏上.contains("目录里已有 1 个文件，它们不在清单里，工具不会改动。"),
        "数完了该写清单外有几个文件：\n{屏上}"
    );

    // 路径换到另一个目录：重数。
    let 另一张 = temp_dir("gui-sub-card-2");
    写(&另一张.path().join("甲.sav"), b"1");
    写(&另一张.path().join("乙/丙.png"), b"2");
    场.app.sublibrary_and_site().0.form_mut().target = romcat_core::path::display(另一张.path());
    画两帧(&ctx, &mut 场);
    场.等任务跑完();
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        屏上.contains("目录里已有 2 个文件，它们不在清单里，工具不会改动。"),
        "路径改了该重数：\n{屏上}"
    );
}

#[test]
fn 目标不在位时说未连接_照样建得出() {
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    let 没插 = 场.卡.path().join("没插上的卡");
    点一下(&ctx, "新建子库", |ui| 场.app.ui(ui));
    {
        let form = 场.app.sublibrary_and_site().0.form_mut();
        form.name = "掌机".to_string();
        form.target = romcat_core::path::display(&没插);
    }
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(屏上.contains("未连接"), "目标不在该说未连接：\n{屏上}");
    assert!(
        !屏上.contains("目录里已有"),
        "不在的目录没有文件可数：\n{屏上}"
    );
    点最后正好那一段(&ctx, "创建子库", |ui| 场.app.ui(ui));
    assert_eq!(
        场.app.sublibrary().list().len(),
        1,
        "目标不在位也该建得出：{:?}",
        场.app.sublibrary().error()
    );
}

#[test]
fn 设备上的位置照实际规则写_改一台时取头一个变体的真实落点_换格式元数据位置跟着变() {
    // 拿主意的人 2026-09-15 定：落点预览用真实落点——子库里的布局照搬主库的键、剥掉根名；元数据位置由适配器答。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=GBA");
    let 卡 = romcat_core::path::display(场.卡.path());
    开目标设置等选择集(&ctx, &mut 场, "掌机");
    // 「设备上的位置」在弹层最底下：视口外 egui 不画字，先在弹层内容区上滚到底（等滚动停下）。
    滚一下(&ctx, &mut 场, -100_000.0);
    let 屏上 = 画两帧(&ctx, &mut 场);
    for 该有 in [
        "设备上的位置",
        &format!("{卡}/GBA/口袋妖怪 绿宝石.zip"),
        &format!("{卡}/GBA.metadata.pegasus.txt"),
        "按平台分目录，不带根名：两个根里相同的相对路径会在差量预览中报为落点撞车。",
    ] {
        assert!(屏上.contains(该有), "设备上的位置没有「{该有}」：\n{屏上}");
    }
    场.app.sublibrary_and_site().0.form_mut().format = "ES-Gamelist".to_string();
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        屏上.contains(&format!("{卡}/gamelists/GBA/gamelist.xml")),
        "换成 ES-DE 之后元数据位置该跟着变：\n{屏上}"
    );
}

#[test]
fn 新建时设备上的位置用示例名_目录照实际规则() {
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    点一下(&ctx, "新建子库", |ui| 场.app.ui(ui));
    画两帧(&ctx, &mut 场);
    // 「设备上的位置」在弹层最底下：视口外 egui 不画字，先在弹层内容区上滚到底（等滚动停下）。
    滚一下(&ctx, &mut 场, -100_000.0);
    let 屏上 = 画两帧(&ctx, &mut 场);
    for 该有 in [
        "/Volumes/SDCARD/GBA/火焰之纹章 烈火之剑.gba",
        "/Volumes/SDCARD/GBA.metadata.pegasus.txt",
    ] {
        assert!(
            屏上.contains(该有),
            "新建时设备上的位置没有「{该有}」：\n{屏上}"
        );
    }
}

#[test]
fn 目标设置里改名_保存之后旧名不在_新名规则与覆盖都在_卡片跟着换() {
    // 拿主意的人 2026-09-15 定：照稿名字可改，核心库一个事务里改名（`Catalog::rename_sublibrary`）。
    use romcat_core::capability::Override;

    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    场.app.sublibrary_and_site().0.edit_target("掌机");
    {
        let form = 场.app.sublibrary_and_site().0.form_mut();
        form.name = "RG35XX".to_string();
        form.overrides.insert("SFC".to_string(), Override::Keep);
    }
    画两帧(&ctx, &mut 场);
    点最后正好那一段(&ctx, "保存", |ui| 场.app.ui(ui));
    assert!(
        !场.app.sublibrary().target_settings_open(),
        "改名存下来之后弹层还开着：{:?}",
        场.app.sublibrary().error()
    );
    let 名字: Vec<&str> = 场
        .app
        .sublibrary()
        .list()
        .iter()
        .map(|row| row.name.as_str())
        .collect();
    assert_eq!(名字, ["RG35XX"], "改名建出了第二台，或者旧名还在");
    let catalog = &场.app.site().catalog;
    assert!(catalog.sublibrary("掌机").expect("读得动").is_none());
    assert_eq!(
        catalog.sublibrary_rules("RG35XX").expect("读得动").len(),
        1,
        "规则没跟着新名过去"
    );
    assert_eq!(
        catalog
            .capability_overrides("RG35XX")
            .expect("读得动")
            .get("SFC"),
        Some(&Override::Keep),
        "按平台覆盖没存在新名下"
    );
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        屏上.lines().any(|line| line == "RG35XX"),
        "卡片没跟着换名字：\n{屏上}"
    );
}

#[test]
fn 改名撞上别的子库_名字那一格当场说_保存关不上() {
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    let 另一张 = temp_dir("gui-sub-card-2");
    {
        let (screen, site) = 场.app.sublibrary_and_site();
        screen.leave_target_settings();
        let form = screen.form_mut();
        form.name = "备份卡".to_string();
        form.target = romcat_core::path::display(另一张.path());
        form.capacity = String::new();
        assert!(screen.save(site), "{:?}", screen.error());
    }
    场.app.sublibrary_and_site().0.edit_target("掌机");
    场.app.sublibrary_and_site().0.form_mut().name = "备份卡".to_string();
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(屏上.contains("已经有同名的子库。"), "\n{屏上}");
    点最后正好那一段(&ctx, "保存", |ui| 场.app.ui(ui));
    assert!(场.app.sublibrary().target_settings_open());
    assert_eq!(场.app.sublibrary().list().len(), 2, "撞名时两台都该还在");
}

#[test]
fn 保存目标设置之后已有的差量预览作废_提示条写差量预览已失效() {
    // 拿主意的人 2026-09-15 定（F9）：照稿用底边提示条，走共用的 `toast.rs`。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "1GB");
    场.加规则("掌机", "平台=SFC");
    场.排预览();
    assert!(
        场.app.sublibrary().prepared().is_some(),
        "前提：排出了一份差量预览"
    );
    场.app.sublibrary_and_site().0.edit_target("掌机");
    场.app.sublibrary_and_site().0.form_mut().capacity = "2".to_string();
    画两帧(&ctx, &mut 场);
    点最后正好那一段(&ctx, "保存", |ui| 场.app.ui(ui));
    assert!(
        场.app.sublibrary().prepared().is_none(),
        "改过目标设置，那份差量预览该作废"
    );
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        屏上.contains("已保存「掌机」的目标设置。差量预览已失效，同步前需要重新生成。"),
        "提示条该说差量预览已失效：\n{屏上}"
    );
}

#[test]
fn 新建子库存下之后提示条写已创建() {
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    点一下(&ctx, "新建子库", |ui| 场.app.ui(ui));
    {
        let form = 场.app.sublibrary_and_site().0.form_mut();
        form.name = "掌机".to_string();
        form.target = romcat_core::path::display(场.卡.path());
    }
    画两帧(&ctx, &mut 场);
    点最后正好那一段(&ctx, "创建子库", |ui| 场.app.ui(ui));
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        屏上.contains("已创建子库「掌机」"),
        "新建之后提示条该说已创建：\n{屏上}"
    );
}

#[test]
fn 打开目标设置顺带读的选择集与清单外文件数不进任务历史() {
    // 拿主意的人 2026-09-15 定：打开弹层时顺带跑的小活不进任务历史，跑着时也不摆在任务台那一栏里；人点起来的照旧进。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    let 之前 = 场.app.tasks().history().len();
    场.app.sublibrary_and_site().0.edit_target("掌机");
    画两帧(&ctx, &mut 场);
    场.等任务跑完();
    画两帧(&ctx, &mut 场);
    场.等任务跑完();
    assert_eq!(
        场.app.sublibrary().target_platforms(),
        Some(vec!["SFC".to_string()]),
        "前提：选择集读回来了"
    );
    历史没多一条(&场, 之前);
    assert!(场.app.tasks().running().is_none());
}

#[test]
fn 前端格式那句照稿写游玩记录和收藏不会被覆盖_两种格式都写() {
    // 两边都核实过才照稿写（Pegasus 的收藏与游玩时长在它自己的配置目录里；ES-DE 在卡上改过的 gamelist 同步不写回去）。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    点一下(&ctx, "新建子库", |ui| 场.app.ui(ui));
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        屏上.contains("前端里的游玩记录和收藏不会被覆盖。"),
        "\n{屏上}"
    );
    场.app.sublibrary_and_site().0.form_mut().format = "ES-Gamelist".to_string();
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        屏上.contains("前端里的游玩记录和收藏不会被覆盖。"),
        "\n{屏上}"
    );
}

#[test]
fn 本机磁盘时连接状态行照稿说不设上限按剩余空间计算_容量条照计划里的上限画() {
    // 测试用的临时目录落在哪种卷上跟机器有关：先问核心它是不是可移动存储，两种情形各钉各的。
    use romcat_core::sublibrary::target;

    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    let 可移动 = target::volume(场.卡.path()).removable;
    场.app.sublibrary_and_site().0.edit_target("掌机");
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert_eq!(
        屏上.contains("本机磁盘不设容量上限时，按剩余空间计算。"),
        !可移动,
        "可移动存储 {可移动}：\n{屏上}"
    );
    场.app.sublibrary_and_site().0.leave_target_settings();
    场.求值();
    let gauge = 场.app.sublibrary().gauge("掌机");
    let 计划里的 = 场
        .app
        .sublibrary()
        .evaluated("掌机")
        .and_then(|report| report.fit.known())
        .map(|room| room.capacity)
        .expect("卡插着，装不装得下算得出");
    assert_eq!(
        gauge.capacity, 计划里的,
        "容量条的上限该照计划里真用上的那个数"
    );
    assert_eq!(gauge.capacity.is_some(), !可移动);
}

#[test]
fn 档案对卡不作声称时不写卡是那半句_有声称时照写() {
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    点一下(&ctx, "新建子库", |ui| 场.app.ui(ui));
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(
        屏上.contains("决定每个平台放到设备上时要不要转换格式。") && !屏上.contains("卡是"),
        "不作声称那一份不该拼出「卡是…」：\n{屏上}"
    );
    场.app.sublibrary_and_site().0.form_mut().capability = "retroarch-fat32".to_string();
    let 屏上 = 画两帧(&ctx, &mut 场);
    assert!(屏上.contains("卡是 FAT32，单文件上限"), "\n{屏上}");
}

#[test]
fn 容量上限按名字那一行就选中那一档() {
    // 共用的单选件（`look::radio_option`）：圆点与名字、底下那行小字都按得动。
    let ctx = headless::context();
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "1GB");
    场.app.sublibrary_and_site().0.edit_target("掌机");
    画两帧(&ctx, &mut 场);
    点一下(&ctx, "设备连接时自动读取", |ui| 场.app.ui(ui));
    assert!(
        场.app.sublibrary_and_site().0.form_mut().capacity_by_device,
        "按「设备连接时自动读取」那一行没选中按设备容量"
    );
    点一下(&ctx, "给存档、截图等留出空间", |ui| {
        场.app.ui(ui)
    });
    assert!(
        !场.app.sublibrary_and_site().0.form_mut().capacity_by_device,
        "按「给存档、截图等留出空间」那一行没选中自定义"
    );
}
