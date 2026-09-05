//! 验收**长活的把手**这一层：报得出进度、被要求停下时停在干净的地方。
//!
//! 把手本身那几件事（排队、按停了不算失败、失败说得出哪一步）由
//! `romcat_core::task` 自己的单元测试钉住。这个文件钉的是别处验不了的三件：
//!
//! 1. **排差量预览真的接上了把手**——从头走到尾，进度走完全部步数。
//! 2. **停下来的地方是干净的**：它整条只读，被叫停时目标设备与中立库一个字节都没动，
//!    **再排一次照样排得出完整的一份**——那正是「不重做已完成的部分」在一条只读的活上
//!    唯一说得通的含义（它没有已完成的部分要保住）。
//! 3. **后台那条线程读的那份只读连接**读得到同一份库，而且写不动——
//!    「两份库不一致」这条路在构造上就不存在（挂账 D156 的裁决靠它成立）。
//!
//! 目标设备**一律拿本地临时目录模拟**：绝不去动任何真实设备或 SD 卡。

use std::fs;
use std::path::Path;
use std::time::SystemTime;

use romcat_core::catalog::{Catalog, CatalogError};
use romcat_core::fs::RealFs;
use romcat_core::scan::{self, Jobs, ScanOptions};
use romcat_core::sublibrary::{Rule, Sublibrary};
use romcat_core::sync;
use romcat_core::task::Handle;
use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

/// 一份小 fixture 主库。**只读**——这几行只往临时目录里写，一个字节都不碰真库。
fn 建库() -> TempDir {
    let dir = temp_dir("task-lib");
    写(&dir.path().join("SFC/幻想传说 汉化版.zip"), &zip(4096));
    写(&dir.path().join("SFC/圣剑传说 3 汉化版.zip"), &zip(8192));
    写(&dir.path().join("GBA/口袋妖怪 绿宝石.zip"), &zip(2048));
    dir
}

/// 一整套现场：fixture 主库、工作目录、当目标用的那个 fixture 目录、一份**落在磁盘上**
/// 的中立库（后台那条线程要的第二份连接只有落盘的库分得出来）。
struct 现场 {
    _库: TempDir,
    工作区: TempDir,
    卡: TempDir,
    库文件: std::path::PathBuf,
    catalog: Catalog,
}

impl 现场 {
    fn 摆好() -> Self {
        let 库 = 建库();
        let 工作区 = temp_dir("task-ws");
        let 卡 = temp_dir("task-card");
        let 库文件 = 工作区.path().join("catalog").join("fixture.sqlite3");
        let mut catalog = Catalog::open(&库文件).expect("能开中立库");
        let mut options = ScanOptions::named(库.path(), "库");
        options.jobs = Jobs::Fixed(2);
        scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");
        catalog
            .put_sublibrary(&Sublibrary {
                name: "掌机".to_string(),
                target: romcat_core::path::display(卡.path()),
                target_raw: None,
                format: "pegasus".to_string(),
                capacity: None,
                capability: None,
            })
            .expect("建得出子库");
        catalog
            .add_rule("掌机", &Rule::parse("平台=SFC").expect("规则读得懂"))
            .expect("写得进规则");
        Self {
            _库: 库,
            工作区,
            卡,
            库文件,
            catalog,
        }
    }

    fn 排一次(&self, task: &Handle) -> Result<sync::Prepared, String> {
        sync::prepare(
            &self.catalog,
            self.工作区.path(),
            "掌机",
            &sync::Request::default(),
            task,
        )
    }

    /// 目标设备与中立库眼下的样子：每个文件的路径、长度、修改时间。
    fn 快照(&self) -> Vec<(String, u64, Option<SystemTime>)> {
        let mut out = Vec::new();
        for dir in [self.卡.path(), self.工作区.path()] {
            收(dir, &mut out);
        }
        out.sort();
        out
    }
}

/// 把一个目录底下每个文件的路径、长度、修改时间收起来。
fn 收(dir: &Path, out: &mut Vec<(String, u64, Option<SystemTime>)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            收(&path, out);
            continue;
        }
        let meta = entry.metadata().expect("读得到元数据");
        out.push((
            romcat_core::path::display(&path),
            meta.len(),
            meta.modified().ok(),
        ));
    }
}

#[test]
fn 排差量预览一路报得出走到第几步() {
    let 场 = 现场::摆好();
    let task = Handle::new();
    assert_eq!(task.progress().at, 0, "还没开始就报了步数");
    let prepared = 场.排一次(&task).expect("排得出来");
    assert!(prepared.plan.adds.files > 0, "一个新增都没有");

    let progress = task.progress();
    assert!(progress.steps > 0, "没说这一趟一共几步，进度条画不出来");
    assert_eq!(
        progress.at,
        progress.steps,
        "走到底了却没报满：{}",
        progress.render(),
    );
    // 最后一步是排计划——那正是**差量预览**本身。
    assert_eq!(progress.step, "排计划");
    // 进度条按「走完的那几步」算，所以站在最后一步上时它是 11/12 而不是满格：
    // **满格是跑完之后的事**，而那时候它已经从「进行中」挪进历史了。
    let fraction = progress.fraction().expect("说得出走了几成");
    assert!((0.9..1.0).contains(&fraction), "{fraction}");
}

#[test]
fn 按停下之后一个字节都没动而且再排一次照样排得出() {
    // 「能停」比「能取消」严格：**要停在干净的地方**。排差量预览整条只读，
    // 所以它的干净是可以照字面核对的——目标设备与工作目录连修改时间都一样。
    let 场 = 现场::摆好();
    let 停之前 = 场.快照();

    let task = Handle::new();
    task.stop();
    let message = 场.排一次(&task).expect_err("已经按了停下，不该排出一份来");
    assert!(
        message.contains("停"),
        "停下来的理由该说清是被按停了：{message}",
    );

    assert_eq!(
        场.快照(),
        停之前,
        "按停了却动了目标设备或者工作目录里的文件"
    );

    // **再排一次照样排得出完整的一份。** 它没有「已完成的部分」要保住——停下等于
    // 什么都没发生，重排就是从头排一次（几百毫秒的活）。
    let 再来 = Handle::new();
    let prepared = 场.排一次(&再来).expect("停过一次不该影响下一次");
    assert!(prepared.plan.touched() > 0, "重排出来的是一份空计划");
    assert_eq!(prepared.selected.picked.len(), 2, "重排选出来的东西不一样");
}

#[test]
fn 后台那份只读连接读得到同一份库而且写不动() {
    // 挂账 D156 当时对「为界面再开一份连接」的顾虑是「会长出两份库不一致」。
    // 这条测试把那句顾虑钉死：第二份连接**写不动**，所以全程只有一个写者。
    let 场 = 现场::摆好();
    let mut 只读 = 场.catalog.read_only().expect("分得出第二份连接");
    assert_eq!(
        只读.location(),
        场.catalog.location(),
        "第二份连接指着另一个文件",
    );
    assert_eq!(
        只读.sublibraries().expect("读得出子库").len(),
        场.catalog.sublibraries().expect("读得出子库").len(),
    );

    let 写不动 = 只读.put_sublibrary(&Sublibrary {
        name: "偷偷写一条".to_string(),
        target: romcat_core::path::display(场.卡.path()),
        target_raw: None,
        format: "pegasus".to_string(),
        capacity: None,
        capability: None,
    });
    assert!(写不动.is_err(), "只读连接居然写进去了");
    assert_eq!(
        场.catalog.sublibraries().expect("读得出子库").len(),
        1,
        "库里多出了一条",
    );
    drop(只读);
    assert!(场.库文件.is_file());
}

#[test]
fn 只活在内存里的库分不出第二份连接() {
    // 合成数据那份库只活在内存里。**这时候要直说**，而不是悄悄开出一份空库来——
    // 那会让界面拿着一份没有任何变体的库排出一份空差量。
    let catalog = Catalog::open_in_memory().expect("能开中立库");
    let error = catalog.read_only().expect_err("内存库不该分得出第二份连接");
    assert!(
        matches!(error, CatalogError::NotOnDisk { .. }),
        "报的不是「只活在内存里」：{error}",
    );
}
