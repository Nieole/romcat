//! **工序进度**：一次问出全部工序「还差多少」。
//!
//! 这个文件钉的是三件事，都是「不这么做会出事」而不是「这样比较好看」：
//!
//! 1. **识别那一支不另造一份数**——它与**待确认队列**、与命令行的队列报告印的是同一个。
//!    造第二份的话，两处迟早对不上，而库屏上那一行说的正是「下一步该干什么」。
//! 2. **加了一块盘重扫之后那个数自己就涨上去**（规格 28）：工序段答的是「还差多少」，
//!    不是「上次几点跑的」——时间戳答不了「要不要重跑识别」这个问题。
//! 3. **某一支交不出度量时退回显示上次跑的时刻**，而且**退路只对单支生效**：
//!    一支退了不许把别的支也降级成时刻。
//!
//! 主库**一律拿本地 fixture 目录模拟**：绝不去动任何真实设备（ADR-0004）。

use std::fs;
use std::path::Path;

use romcat_core::catalog::{Catalog, Roots};
use romcat_core::dat::repo::DatRepo;
use romcat_core::fs::RealFs;
use romcat_core::identify::{self, Options, fuzzy};
use romcat_core::report::thousands;
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::stage::{Behind, Stage, Stages};
use romcat_core::task::Handle;
use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::triage::Queue;
use romcat_core::triage::report::QueueReport;
use romcat_core::verdict::{self, Store};

const 库名: &str = "小库";

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

/// 一份小 fixture 主库。**只读**——这几行只往临时目录里写，一个字节都不碰真库。
fn 建库(tag: &str, 份数: usize) -> TempDir {
    let dir = temp_dir(tag);
    for i in 0..份数 {
        写(
            &dir.path().join(format!("FC/游戏{i:02}.zip")),
            &zip(1024 + i),
        );
    }
    dir
}

struct 现场 {
    _工作区: TempDir,
    catalog: Catalog,
    repo: DatRepo,
    store: Store,
}

impl 现场 {
    fn 摆好() -> Self {
        let 工作区 = temp_dir("stage-ws");
        let catalog = Catalog::open(&工作区.path().join("catalog").join("小库.sqlite3"))
            .expect("开得出中立库");
        let repo = DatRepo::open(&工作区.path().join("dat.sqlite3")).expect("开得出 DAT 库");
        let store = Store::in_memory().expect("开得出沉淀库");
        Self {
            _工作区: 工作区,
            catalog,
            repo,
            store,
        }
    }

    fn 扫(&mut self, 根名: &str, 目录: &Path) {
        let mut options = ScanOptions::named(目录, 根名);
        options.jobs = Jobs::Fixed(2);
        scan::scan(&RealFs::new(), &mut self.catalog, &options, &Handle::new()).expect("扫得动");
    }

    fn 跑识别(&mut self, 根名: &str, 目录: &Path) {
        let index = verdict::Index::load(&self.store, 库名).expect("读得出沉淀库");
        identify::run(
            &RealFs::new(),
            &mut self.catalog,
            &identify::Ammo {
                repo: &self.repo,
                verdicts: &index,
                naming: &fuzzy::Naming::off(),
                guessing: &identify::model::Guessing::off(),
                titledb: None,
            },
            &Options::new(Roots::single(根名, 目录)),
            &CancelToken::new(),
            &mut |_| {},
        )
        .expect("识别不该失败");
    }

    /// **待确认队列**眼下说库里还有多少个变体连识别都没跑过。
    fn 队列说的(&self) -> u64 {
        let index = verdict::Index::load(&self.store, 库名).expect("读得出沉淀库");
        Queue::load(&self.catalog, &index)
            .expect("列得出队列")
            .not_run()
    }

    /// 识别那一行。
    fn 识别那一行(&self) -> romcat_core::stage::StageRow {
        Stages::survey(&self.catalog)
            .of(Stage::Identify)
            .expect("工序段有识别那一行")
            .clone()
    }
}

#[test]
fn 识别那一行的数与待确认队列和命令行队列报告印的是同一个() {
    // **不另造一份**：这个数只有 `Catalog::not_run_count` 一处算得出来，工序段、
    // 待确认队列屏与命令行 `triage list` 都从那一处取。造第二份的话两处迟早对不上，
    // 而库屏那一行说的正是「下一步该干什么」。
    let 甲 = 建库("stage-甲", 3);
    let 乙 = 建库("stage-乙", 2);
    let mut 现场 = 现场::摆好();
    现场.扫("甲", 甲.path());
    现场.跑识别("甲", 甲.path());

    // 跑完识别，甲那三个都问过了。
    assert_eq!(现场.识别那一行().behind, Behind::Left(0));
    assert_eq!(现场.队列说的(), 0);

    // **加了一块盘重扫**：乙那两个连识别都还没跑过，那个数自己就涨上去（规格 28）。
    现场.扫("乙", 乙.path());
    let 行 = 现场.识别那一行();
    let Behind::Left(还差) = 行.behind else {
        panic!("识别这一支说得出还差多少：{:?}", 行.behind);
    };
    assert_eq!(还差, 2, "乙那两个变体连识别都还没跑过");
    assert_eq!(现场.队列说的(), 还差, "队列屏与工序段说的不是同一个数");
    assert_eq!(
        现场.catalog.not_run_count().expect("数得出"),
        还差,
        "工序段另造了一份数",
    );

    // **命令行那份报告印的也是它**：`triage list` 拿的是同一个 `not_run_count`。
    let 报告 = QueueReport::build(
        "小库.sqlite3",
        "沉淀库",
        0,
        现场.catalog.not_run_count().expect("数得出"),
        &[],
        verdict::Counts::default(),
        0,
    );
    let 印出来 = 报告.render_text();
    assert!(
        印出来.contains(&thousands(还差)),
        "命令行报告印的不是这个数：\n{印出来}",
    );
    assert!(
        行.render().contains(&thousands(还差)),
        "工序段那一行印的不是这个数：{}",
        行.render(),
    );
    assert!(
        行.render().contains("连识别都还没跑过"),
        "那一行说的不是「还差多少」：{}",
        行.render(),
    );
}

#[test]
fn 一支交不出度量时退回上次跑的时刻_别的支照旧报数() {
    // **退路只对单支生效**（规格 31）。07 与 08 那两支的度量可能算不出来而退回时刻，
    // 那时**不许**把现成的识别那一支也一起降级成时刻——它明明数得出来。
    let 甲 = 建库("stage-退路", 4);
    let mut 现场 = 现场::摆好();
    现场.扫("甲", 甲.path());

    let 识别 = 现场.识别那一行();
    assert_eq!(识别.behind, Behind::Left(4), "识别这一支数得出来");

    // 交不出度量的那一支长这样：**画的是时刻，不是零**——「还差 0」与「算不出还差多少」
    // 是两件事，混起来那一行就在撒谎。
    let 退了的 = romcat_core::stage::StageRow {
        stage: Stage::Identify,
        behind: Behind::Unmeasured {
            at: Some(1_700_000_000),
            why: "要全表比对，太贵".to_string(),
        },
    };
    let 那一句 = 退了的.render();
    assert!(
        那一句.contains("上次跑"),
        "退路那一行没说上次跑是什么时候：{那一句}"
    );
    assert!(
        那一句.contains("算不出还差多少"),
        "退路那一行没说清它为什么不报数：{那一句}",
    );
    assert!(
        !那一句.contains(" 0 "),
        "算不出还差多少却画了个零出来：{那一句}"
    );

    // **一支退了，别的支照旧报数**：这一条是「退路只对单支生效」的兑现处。
    assert_eq!(现场.识别那一行().behind, Behind::Left(4));
}

#[test]
fn 从没跑过的那一支说的是还没跑过_不是某个时刻() {
    // 退回时刻的那一支也可能压根没跑过。那时画一个时刻出来是凭空捏造。
    let 没跑过 = romcat_core::stage::StageRow {
        stage: Stage::Identify,
        behind: Behind::Unmeasured {
            at: None,
            why: "还没有上次".to_string(),
        },
    };
    assert!(没跑过.render().contains("还没跑过"), "{}", 没跑过.render());
}

#[test]
fn 一个变体都没有的库里识别这一支说它不差什么() {
    // 空库不该说「0 个变体连识别都还没跑过」——那句话读起来像出了什么事。
    let 现场 = 现场::摆好();
    let 行 = 现场.识别那一行();
    assert_eq!(行.behind, Behind::Left(0));
    assert!(!行.render().contains("还没跑过"), "{}", 行.render());
}
