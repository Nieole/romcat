//! **子库**那一屏：建得出、写得进规则与例外、**差量预览摆得出来**、同步从这里触发。
//!
//! 这几条是这张票最要紧的纪律，而它们都是「不这么做会出事」而不是「这样比较好看」：
//!
//! - **同步前必须预览差量**（ADR-0016）：没排过预览，那个按钮就不该动得了。
//! - **删除前必须干跑预览**（ADR-0015）：计划里有删除时还要人再点一次头。
//! - **容量超限不自动截断**（ADR-0016）：报出超出量与按体积排序的裁剪建议，一个都不砍。
//! - **只碰清单里记录过的文件**（ADR-0015）：维护者自己拷进卡里的东西，同步前后
//!   连修改时间都一样。
//!
//! 目标设备**一律拿本地 fixture 目录模拟**：绝不去动任何真实设备或 SD 卡。

use std::fs;
use std::path::Path;
use std::time::Duration;

use romcat_core::catalog::Catalog;
use romcat_core::fs::RealFs;
use romcat_core::scan::{self, Jobs, ScanOptions};
use romcat_core::task::Handle;
use romcat_core::site::Site;
use romcat_core::sublibrary::Exception;
use romcat_core::task::Ending;
use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};
use romcat_gui::app::{App, View};

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
    _库: TempDir,
    _工作区: TempDir,
    卡: TempDir,
    app: App,
}

impl 现场 {
    fn 摆好() -> Self {
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
            let mut options = ScanOptions::named(库.path(), "库");
            options.jobs = Jobs::Fixed(2);
            scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new())
                .expect("扫得动");
        }
        let site = Site::open_file(工作区.path(), &库文件, None).expect("开得出现场");
        let mut app = App::new(site, 工作区.path().to_path_buf());
        app.show_view(View::Sublibraries);
        Self {
            _库: 库,
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

    fn 加规则(&mut self, name: &str, rule: &str) {
        let (screen, site) = self.app.sublibrary_and_site();
        rule.clone_into(screen.rule_draft_mut());
        screen.add_rule(site, name);
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
fn 建得出子库也写得进规则与例外() {
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    assert_eq!(场.app.sublibrary().list().len(), 1);
    assert_eq!(场.app.sublibrary().picked(), Some("掌机"));

    场.加规则("掌机", "平台=SFC");
    assert_eq!(场.app.sublibrary().rules().len(), 1);
    assert!(场.app.sublibrary().broken().is_empty());

    // **读不懂的规则当场说清楚，不入库**——一条写坏的不该混进选择集悄悄少选一批。
    {
        let (screen, site) = 场.app.sublibrary_and_site();
        "这不是一条规则".clone_into(screen.rule_draft_mut());
        screen.add_rule(site, "掌机");
        assert!(screen.error().is_some(), "写坏的规则该被当场拦下");
    }
    assert_eq!(场.app.sublibrary().rules().len(), 1, "写坏的那条不该入库");

    // **例外优先于规则、永久记住**（ADR-0016）。
    {
        let (screen, site) = 场.app.sublibrary_and_site();
        screen.add_exception(
            site,
            Exception::Exclude,
            "SFC/幻想传说 汉化版.zip",
            "太占地方",
        );
    }
    let 例外 = 场.app.sublibrary().exceptions();
    assert_eq!(例外.len(), 1);
    assert_eq!(例外[0].kind, Exception::Exclude);
    assert_eq!(例外[0].note.as_deref(), Some("太占地方"));
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
fn 差量预览摆得出新增与净变化() {
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
}

#[test]
fn 卡不在手边也算得出选中多少与超限多少() {
    // 子库是**持久实体**，不是「插上卡才存在的东西」（ADR-0015）；而排差量预览要
    // 目标在位（三方对比的第三方就是目标上实际有什么）。于是只求选择集这一步单开一条路：
    // 卡不在手边照样调得动规则、看得见容量账。
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
    let report = &screen.evaluated().expect("求得出来").report;
    assert_eq!(report.picked, 2);
    assert_eq!(report.rules.len(), 1);
    assert_eq!(report.rules[0].hits, 2);
    assert!(report.over_capacity.is_some(), "4KB 装不下 12KiB");
    assert!(!report.trim_suggestions.is_empty(), "超限了却没给裁剪建议");

    // 而**差量预览**这时该直说目标不在位，不是编一份出来。
    场.排预览();
    let screen = 场.app.sublibrary();
    assert!(screen.prepared().is_none(), "卡不在位却排出了一份差量");
    assert!(screen.error().is_some(), "卡不在位该说出口");
}

#[test]
fn 改过规则那份预览当场作废() {
    // 留着上一套规则排出来的差量，是这一屏最容易骗到人的一种写法。
    let mut 场 = 现场::摆好();
    场.建子库("掌机", "");
    场.加规则("掌机", "平台=SFC");
    场.排预览();
    assert!(场.app.sublibrary().prepared().is_some());

    场.加规则("掌机", "平台=GBA");
    assert!(
        场.app.sublibrary().prepared().is_none(),
        "改过规则之后那份差量说的已经不是眼下这套选择集会做的事了",
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
