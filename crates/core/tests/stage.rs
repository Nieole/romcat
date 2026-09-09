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

use romcat_core::adapter::pegasus::Pegasus;
use romcat_core::adapter::transfer;
use romcat_core::catalog::{Catalog, Confidence, Roots, TitleRow};
use romcat_core::dat::repo::DatRepo;
use romcat_core::fs::RealFs;
use romcat_core::identify::{self, Options, fuzzy};
use romcat_core::report::thousands;
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::scrape::Priorities;
use romcat_core::stage::{Behind, Stage, Stages};
use romcat_core::task::Handle;
use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::title;
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

/// 一份**横跨两个平台**的 fixture 主库：导出会收敛成两份元数据文件。
///
/// 「写过一份之后按停」得有第二份可写才验得到——一份的库停在哪儿都是「写完了」。
fn 建库两个平台(tag: &str) -> TempDir {
    let dir = temp_dir(tag);
    for (平台, i) in [("FC", 0), ("SFC", 1)] {
        写(&dir.path().join(format!("{平台}/游戏.zip")), &zip(1024 + i));
    }
    dir
}

struct 现场 {
    工作区: TempDir,
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
            工作区,
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

    /// 折标题那一行。
    fn 折标题那一行(&self) -> romcat_core::stage::StageRow {
        Stages::survey(&self.catalog)
            .of(Stage::FoldTitles)
            .expect("工序段有折标题那一行")
            .clone()
    }

    /// 跑一趟**折标题**。界面上点那一行的按钮、命令行 `romcat titles` 走的都是它。
    fn 折标题(&mut self) {
        title::run(&mut self.catalog, &self.store, &Priorities::builtin()).expect("折得出标题");
    }

    /// 导出那一行。
    fn 导出那一行(&self) -> romcat_core::stage::StageRow {
        Stages::survey(&self.catalog)
            .of(Stage::Export)
            .expect("工序段有导出那一行")
            .clone()
    }

    /// 跑一趟**导出**：把中立库写成 Pegasus 能读的元数据，铺在一个临时目录里。
    /// 界面上点那一行的按钮、命令行 `romcat export` 走的都是它。
    ///
    /// **一个 ROM 都不搬**（ADR-0004）：落点是另开的一个临时目录，主库那份 fixture
    /// 一个字节都不动。
    fn 导出(&mut self) -> romcat_core::adapter::report::ExportReport {
        let out = self.工作区.path().join("导出去");
        transfer::export(
            &mut self.catalog,
            &Pegasus,
            &Priorities::builtin(),
            &transfer::ExportOptions {
                out,
                dry_run: false,
                force: false,
            },
        )
        .expect("导得出来")
    }

    /// 往标题集合里塞一条叫法，**当作上一趟折出来的那批**。
    ///
    /// 它的 `source` 不是**裁决**，所以下一趟重折的第一件事
    /// （`Catalog::clear_titles`）就该把它清掉——「按停时清空那一下有没有发生」
    /// 因此看得见。
    fn 摆一条上一趟折出来的叫法(&mut self) {
        self.catalog
            .put_titles(&[TitleRow {
                work: "某作品".to_string(),
                value: "上一趟折出来的那条".to_string(),
                language: romcat_core::title::Language::Chinese,
                kind: romcat_core::title::TitleKind::Translated,
                source: "文件名".to_string(),
                region: None,
                variant_key: None,
                confidence: Confidence::High,
                seam: None,
                evidence: "摆料".to_string(),
                seen: 1,
            }])
            .expect("写得进");
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

#[test]
fn 折标题那一支退回上次跑的时刻_而识别那一行照旧报数() {
    // **这一支的度量走了退路**（规格「退路已经认可」）：算「标题集合变过、还没重折的
    // 作品数」要把整份集合重折一遍比对，而那一趟正是这道工序自己——实测见
    // `Stage::FoldTitles` 上那段说明与挂单 `Q426`。
    //
    // 退路那一行说的是**上次跑的时刻**，而且**只降它自己这一支**：识别那一行照旧报数
    // （验收第 2 条逐字写着「并且识别那一行不受影响」）。
    let 甲 = 建库("stage-折标题", 4);
    let mut 现场 = 现场::摆好();
    现场.扫("甲", 甲.path());

    // 还没折过：画的是「还没跑过」，**不是零**——「还差 0」与「算不出还差多少」
    // 是两件事。
    let 行 = 现场.折标题那一行();
    let Behind::Unmeasured { at, why } = &行.behind else {
        panic!("折标题这一支眼下走的是退路：{:?}", 行.behind);
    };
    assert_eq!(*at, None, "一趟都没折过，却报得出一个时刻");
    assert!(why.contains("重折"), "没说清它为什么算不出还差多少：{why}",);
    assert!(行.render().contains("还没跑过"), "{}", 行.render(),);
    assert!(
        !行.render().contains(" 0 "),
        "算不出还差多少却画了个零出来：{}",
        行.render(),
    );

    // **识别那一支不受影响**：它明明数得出来。
    assert_eq!(现场.识别那一行().behind, Behind::Left(4));

    // 折一趟：那一行改口说「上次跑是 ⋯」——这条数据通路是这张票带进来的
    // （`Catalog::titles_folded_at`），票 `08` 白拿它。
    现场.折标题();
    let 行 = 现场.折标题那一行();
    let Behind::Unmeasured { at: Some(at), .. } = 行.behind else {
        panic!("折过一趟了，那一行还说不出上次是什么时候：{:?}", 行.behind);
    };
    assert!(at > 0, "记下来的时刻不像个时刻：{at}");
    assert!(
        行.render().contains("上次跑是"),
        "折过一趟了，那一行还在说「还没跑过」：{}",
        行.render(),
    );

    // 折完之后识别那一行**一个字都没变**。
    assert_eq!(现场.识别那一行().behind, Behind::Left(4));
}

#[test]
fn 工序段一道工序一行_次序就是那张全部工序的名单() {
    // `Stage::ALL` 是库屏上从上到下的次序，也是主干六步里的先后。漏一项的话
    // 那一支整个不出现在库屏上，而**不出现**是最难查的那种错。
    let 现场 = 现场::摆好();
    let stages = Stages::survey(&现场.catalog);
    let 次序: Vec<Stage> = stages.rows().iter().map(|row| row.stage).collect();
    assert_eq!(次序, Stage::ALL.to_vec(), "工序段少了一行，或者次序对不上");
    assert!(
        stages.of(Stage::FoldTitles).is_some(),
        "折标题那一行没排进来",
    );
}

#[test]
fn 折标题的度量真做出来时那一行说的是几个作品还没重折() {
    // **退路不是这一支永远的说法**：拿主意的人日后要是认了那张变更计数表
    // （挂单 `Q426`），`row_of` 折出来的就是 `Left`，而那一行该说的话在这儿钉着。
    let 差着 = romcat_core::stage::StageRow {
        stage: Stage::FoldTitles,
        behind: Behind::Left(1_234),
    };
    let 那一句 = 差着.render();
    assert!(那一句.contains(&thousands(1_234)), "{那一句}");
    assert!(那一句.contains("还没重折"), "{那一句}");

    // **不差什么了也得说话**：一行空白读起来像出了什么事。
    let 不差 = romcat_core::stage::StageRow {
        stage: Stage::FoldTitles,
        behind: Behind::Left(0),
    };
    assert!(!不差.render().is_empty());
    assert!(!不差.render().contains("还没重折"), "{}", 不差.render());
}

#[test]
fn 折标题按停时清空那一下压根没发生_标题集合一条都没少() {
    // **这一条钉的是这道工序最坏的那种收场。** 重折是「清掉再写回」
    // （`title::run_task` → `write_back`），停在那两句中间，`title` 那张表就是空的
    // ——而库屏、详情面板、导出与搜索读的都是它。所以最后一个能停的地方摆在写回
    // **之前**：那一趟要么写完、要么一个字节都没写。
    //
    // 拿一个**一开始就停着的把手**验，不靠「恰好停在某一步」那种挂钟彩票。
    let mut 现场 = 现场::摆好();
    现场.摆一条上一趟折出来的叫法();
    let 折之前 = 现场.catalog.title_count().expect("数得出");
    assert_eq!(折之前, 1, "前提：库里有一条上一趟折出来的叫法");

    let 把手 = Handle::new();
    把手.stop();
    let 结果 = title::run_task(
        &mut 现场.catalog,
        &现场.store,
        &Priorities::builtin(),
        &把手,
    );
    assert!(
        matches!(结果, Err(title::FoldTitlesError::Halted(_))),
        "按停了却没交出「被按停了」那一支：{结果:?}",
    );
    assert_eq!(
        现场.catalog.title_count().expect("数得出"),
        折之前,
        "停下来那一趟把标题集合清掉了——它该一个字节都不动",
    );
    // 也没打时刻戳：**没跑完就不算跑过**。
    assert_eq!(现场.catalog.titles_folded_at().expect("读得出"), None);
}

#[test]
fn 折标题这一趟报得出走到第几步() {
    // 验收第 3 条「报得出进度」的落点：任务屏画的那句「几/几 哪一步」取的就是把手上
    // 这几个数（`task::Progress::render`）。总步数由调用方声明——界面那一侧还多一步
    // 「读优先级表」，所以它报的是 `1 + TASK_STEPS`。
    let mut 现场 = 现场::摆好();
    let 把手 = Handle::new();
    把手.steps(title::TASK_STEPS);
    title::run_task(
        &mut 现场.catalog,
        &现场.store,
        &Priorities::builtin(),
        &把手,
    )
    .expect("折得出标题");

    let 进度 = 把手.progress();
    assert_eq!(进度.steps, title::TASK_STEPS, "总步数报错了");
    assert_eq!(进度.at, title::TASK_STEPS, "走过的步数与声明的对不上");
    assert_eq!(进度.step, "写回中立库", "最后停在的那一步说错了");
    // **说得出走了几成**：说不出的话界面画的是一条来回跑的条，而不是一条走着的条
    // （`Progress::fraction` 的文档）。最后一步「眼下在第 3 步」折出来是 2/3，
    // 不是十成——那是这个数自己的口径，不是这儿算错了。
    assert!(进度.fraction().is_some(), "说不出走了几成");
}

#[test]
fn 顺手把集合折回来不算跑过这道工序() {
    // **「上次跑是几点」说的是有人跑了这道工序**，不是「集合上次被折出来」。
    // 一条否定裁决就地重折（`scrape::zh::judge` 走的 `title::refold`）也把集合折了
    // 一遍，可人并没有点过那颗按钮——那一下要是也打时刻戳，库屏会说「上次跑是刚刚」，
    // 而那句话在骗人（规格 31：「那一行退回显示上次跑的时刻，这样它至少不骗我」）。
    let mut 现场 = 现场::摆好();
    title::refold(&mut 现场.catalog, &现场.store).expect("折得回来");
    assert!(
        matches!(
            现场.折标题那一行().behind,
            Behind::Unmeasured { at: None, .. }
        ),
        "顺手折了一遍就算跑过这道工序了：{:?}",
        现场.折标题那一行().behind,
    );

    // 真跑一趟才算。
    现场.折标题();
    assert!(
        matches!(
            现场.折标题那一行().behind,
            Behind::Unmeasured { at: Some(_), .. }
        ),
        "跑过了却说不出上次是什么时候：{:?}",
        现场.折标题那一行().behind,
    );
}

#[test]
fn 导出那一支退回上次跑的时刻_而识别那一行照旧报数() {
    // **这一支的度量也走了退路**（规格「退路已经认可」，挂单 `Q436`）：算「上次导出
    // 之后库里改了多少条」要把整库收敛一遍、再逐份与上次写出去的**底本**比对，
    // 而那一趟正是这道工序自己——实测见 `Stage::Export` 上那段说明。
    //
    // 退路那一行说的是**上次跑的时刻**，而且**只降它自己这一支**：识别那一行照旧报数
    // （验收第 1 条逐字写着「且不牵连另外两支」）。
    let 甲 = 建库("stage-导出", 4);
    let mut 现场 = 现场::摆好();
    现场.扫("甲", 甲.path());

    // 还没导过：画的是「还没跑过」，**不是零**——「还差 0」与「算不出还差多少」
    // 是两件事。
    let 行 = 现场.导出那一行();
    let Behind::Unmeasured { at, why } = &行.behind else {
        panic!("导出这一支眼下走的是退路：{:?}", 行.behind);
    };
    assert_eq!(*at, None, "一趟都没导过，却报得出一个时刻");
    assert!(why.contains("收敛"), "没说清它为什么算不出还差多少：{why}");
    assert!(行.render().contains("还没跑过"), "{}", 行.render());
    assert!(
        !行.render().contains(" 0 "),
        "算不出还差多少却画了个零出来：{}",
        行.render(),
    );

    // **另外两支不受影响**：识别明明数得出来，折标题照旧说它自己那一句。
    assert_eq!(现场.识别那一行().behind, Behind::Left(4));
    assert!(现场.折标题那一行().render().contains("还没跑过"));

    // 导一趟：那一行改口说「上次跑是 ⋯」。
    现场.导出();
    let 行 = 现场.导出那一行();
    let Behind::Unmeasured { at: Some(at), .. } = 行.behind else {
        panic!("导过一趟了，那一行还说不出上次是什么时候：{:?}", 行.behind);
    };
    assert!(at > 0, "记下来的时刻不像个时刻：{at}");
    assert!(
        行.render().contains("上次跑是"),
        "导过一趟了，那一行还在说「还没跑过」：{}",
        行.render(),
    );

    // 导完之后识别那一行**一个字都没变**。
    assert_eq!(现场.识别那一行().behind, Behind::Left(4));
}

#[test]
fn 导出的度量真做出来时那一行说的是几个条目在上次导出之后变过() {
    // **退路不是这一支永远的说法**：拿主意的人日后要是认了那张变更计数表
    // （挂单 `Q436`），`row_of` 折出来的就是 `Left`，而那一行该说的话在这儿钉着。
    let 差着 = romcat_core::stage::StageRow {
        stage: Stage::Export,
        behind: Behind::Left(2_345),
    };
    let 那一句 = 差着.render();
    assert!(那一句.contains(&thousands(2_345)), "{那一句}");
    assert!(那一句.contains("上次导出之后变过"), "{那一句}");

    // **不差什么了也得说话**：一行空白读起来像出了什么事。
    let 不差 = romcat_core::stage::StageRow {
        stage: Stage::Export,
        behind: Behind::Left(0),
    };
    assert!(!不差.render().is_empty());
    assert!(!不差.render().contains("变过"), "{}", 不差.render());
}

#[test]
fn 导出一份都没写就被按停_盘上与中立库都一个字节没动() {
    // **四档收场里的第二档**：停了，什么都没留下。收敛整个库是这一趟最长的一段
    // （见 `Stage::Export` 上的实测），按停多半就落在那儿——那时一份文件都还没写，
    // 说「停在半路」是骗人的（`Handle::halfway` 的文档：只读的活不该说这一句）。
    //
    // 拿一个**一开始就停着的把手**验，不靠「恰好停在某一步」那种挂钟彩票。
    let 甲 = 建库("stage-导出按停", 3);
    let mut 现场 = 现场::摆好();
    现场.扫("甲", 甲.path());
    let out = 现场.工作区.path().join("导出去");

    let 把手 = Handle::new();
    把手.stop();
    let 结果 = transfer::export_task(
        &mut 现场.catalog,
        &Pegasus,
        &Priorities::builtin(),
        &transfer::ExportOptions {
            out: out.clone(),
            dry_run: false,
            force: false,
        },
        &把手,
    );
    assert!(
        matches!(结果, Err(transfer::ExportError::Halted(_))),
        "按停了却没交出「被按停了」那一支：{结果:?}",
    );
    assert!(!out.exists(), "一份都没写的那一趟却建出了导出目录");
    // 也没打时刻戳：**没跑完就不算跑过**。
    assert_eq!(现场.catalog.exported_at().expect("读得出"), None);
    assert!(现场.导出那一行().render().contains("还没跑过"));
}

#[test]
fn 只排计划那一趟不打时刻戳() {
    // `dry_run` 一个字节都不写盘。说「上次跑是刚刚」，那一行就在骗人
    // （规格第 31 条「至少不骗我」）。
    let 甲 = 建库("stage-导出预演", 3);
    let mut 现场 = 现场::摆好();
    现场.扫("甲", 甲.path());
    let out = 现场.工作区.path().join("预演");

    transfer::export(
        &mut 现场.catalog,
        &Pegasus,
        &Priorities::builtin(),
        &transfer::ExportOptions {
            out,
            dry_run: true,
            force: false,
        },
    )
    .expect("排得出计划");
    assert_eq!(现场.catalog.exported_at().expect("读得出"), None);
    assert!(现场.导出那一行().render().contains("还没跑过"));
}

/// 一份**照着 Pegasus 写、但写出第一份就替人按下「停下」**的适配器。
///
/// 「停在半路那一趟说得出留下了什么」得**真的留下几份文件**才验得到，而靠时间去抢
/// 那一下抢不准——这份 fixture 小到几毫秒就导完了（挂单 `Q196` / `Q349` 说的正是那种
/// 挂钟彩票）。这一层把那一下钉死在「写出第几份」上。
///
/// **停下的信号落在第一份写出去之后**：`export_task` 在每一份之前看一眼把手，
/// 所以第一份整份落成、第二份一个字节都没写。它自己**一个判断都不做**，只转发给
/// [`Pegasus`]。
struct 写出第一份就按停<'a> {
    task: &'a Handle,
    写过几份: std::cell::Cell<usize>,
}

impl romcat_core::adapter::Adapter for 写出第一份就按停<'_> {
    fn name(&self) -> &'static str {
        Pegasus.name()
    }
    fn ceiling(&self) -> romcat_core::adapter::Capability {
        Pegasus.ceiling()
    }
    fn file_name(&self) -> &'static str {
        Pegasus.file_name()
    }
    fn read(
        &self,
        bytes: &[u8],
    ) -> Result<romcat_core::adapter::Parsed, romcat_core::adapter::AdapterError> {
        Pegasus.read(bytes)
    }
    fn write(
        &self,
        doc: &romcat_core::adapter::Document,
        baseline: Option<&romcat_core::adapter::Parsed>,
    ) -> Result<Vec<u8>, romcat_core::adapter::AdapterError> {
        self.写过几份.set(self.写过几份.get() + 1);
        self.task.stop();
        Pegasus.write(doc, baseline)
    }
    fn media_placement(
        &self,
        rom_key: &str,
        kind: romcat_core::scrape::MediaKind,
        hash: &str,
        ext: &str,
    ) -> Option<romcat_core::adapter::MediaPlacement> {
        Pegasus.media_placement(rom_key, kind, hash, ext)
    }
}

#[test]
fn 导出写过一份之后被按停_记的是停在半路而且写过的那几份留在盘上() {
    // **导出有「停在半路」这一档，而折标题没有**——差别是真的：重折是「清掉再写回」，
    // 停在中间等于把整份集合丢掉；导出是**一份文件一份文件地写**，写完一份就把
    // **底本**一起存进中立库。停在第三份上，前两份真的躺在盘上了，下一趟还拿它们
    // 当基线接着比——那正是「没走完却留下了东西」（`Handle::halfway`）。
    let 甲 = 建库两个平台("stage-导出半路");
    let mut 现场 = 现场::摆好();
    现场.扫("甲", 甲.path());
    let out = 现场.工作区.path().join("导出去");

    let ended = {
        let mut board: romcat_core::task::Board<romcat_core::adapter::report::ExportReport> =
            romcat_core::task::Board::new();
        board.run_here("导出", |task| {
            transfer::export_task(
                &mut 现场.catalog,
                &写出第一份就按停 {
                    task,
                    写过几份: std::cell::Cell::new(0),
                },
                &Priorities::builtin(),
                &transfer::ExportOptions {
                    out: out.clone(),
                    dry_run: false,
                    force: false,
                },
                task,
            )
            .map_err(romcat_core::task::Cutoff::from)
        });
        board.poll().expect("就地跑就是当场跑完").ended
    };

    let romcat_core::task::Ending::Halfway {
        product: report,
        left_behind,
    } = ended
    else {
        panic!("写过一份之后按停，台上却没记成停在半路");
    };
    assert!(report.interrupted, "被按停了却没记上");
    let 写出去的: Vec<&romcat_core::adapter::report::ExportedFile> =
        report.files.iter().filter(|file| file.written).collect();
    assert_eq!(写出去的.len(), 1, "该只写出去一份：{:#?}", report.files);
    // **第二份一个字节都没写**：这个库横跨两个平台，收敛出两份元数据文件。
    // 落点上只躺着一份，这条测试才真的验在「没走完」上。
    let 盘上几份 = fs::read_dir(&out)
        .expect("导出目录建出来了")
        .filter(|entry| entry.as_ref().is_ok_and(|entry| entry.path().is_file()))
        .count();
    assert_eq!(盘上几份, 1, "落点上不止一份，那这一趟其实跑完了");
    // **留下了什么由长入口自己说**（`Handle::halfway`）：只有它知道写出去了几份、
    // 一共几份。
    assert!(
        left_behind.contains(&format!("共 {} 份", thousands(2))),
        "那一句没说清一共几份：{left_behind}",
    );
    assert!(
        left_behind.contains("接着把剩下的写完"),
        "那一句没说清下一趟接得上：{left_behind}",
    );
    // 写过的那一份**真的躺在盘上**。
    let 落点 = std::path::PathBuf::from(&写出去的[0].path);
    assert!(
        落点.exists(),
        "报告说写了，盘上却没有：{}",
        写出去的[0].path
    );
    // **没打时刻戳**：只写了一半说不上「导过了」。
    assert_eq!(现场.catalog.exported_at().expect("读得出"), None);
    assert!(现场.导出那一行().render().contains("还没跑过"));
}

#[test]
fn 导出这一趟报得出走到第几步() {
    // 验收第 4 条「报得出进度」的落点：任务屏画的那句「几/几 哪一步」取的就是把手上
    // 这几个数（`task::Progress::render`）。总步数由调用方声明——界面那一侧还多一步
    // 「读优先级表」，所以它报的是 `1 + TASK_STEPS`。
    let 甲 = 建库("stage-导出步数", 3);
    let mut 现场 = 现场::摆好();
    现场.扫("甲", 甲.path());
    let out = 现场.工作区.path().join("导出去");

    let 把手 = Handle::new();
    把手.steps(transfer::TASK_STEPS);
    transfer::export_task(
        &mut 现场.catalog,
        &Pegasus,
        &Priorities::builtin(),
        &transfer::ExportOptions {
            out,
            dry_run: false,
            force: false,
        },
        &把手,
    )
    .expect("导得出来");

    let 进度 = 把手.progress();
    assert_eq!(进度.steps, transfer::TASK_STEPS, "总步数报错了");
    assert_eq!(进度.at, transfer::TASK_STEPS, "走过的步数与声明的对不上");
    assert_eq!(进度.step, "逐份写出去", "最后停在的那一步说错了");
    assert!(进度.fraction().is_some(), "说不出走了几成");
}

#[test]
fn 选过一次前端格式与目录_下一趟从库里读得回来() {
    // 验收第 2、3 条：第一次跑之前选一次，记进中立库的元数据表；下一趟不必再选。
    // **纯加键、不升结构版本**——`SCHEMA_VERSION` 一动不动。
    let 现场 = 现场::摆好();
    assert_eq!(
        现场.catalog.export_setup().expect("读得出"),
        None,
        "一次都没选过，却读回来一套配置",
    );

    let 选好的 = romcat_core::catalog::ExportSetup::check("pegasus", "/一个/目录")
        .expect("这是带得出来的格式");
    // **存的是适配器自己报的那个名字**：快照那张表按格式名分栏，大小写各存一份的话
    // 上一趟的**底本**再也认不出来。
    assert_eq!(选好的.format, "Pegasus");
    现场.catalog.set_export_setup(&选好的).expect("记得下");

    assert_eq!(
        现场.catalog.export_setup().expect("读得出"),
        Some(选好的),
        "记下了却读不回来——下一趟还得再选一遍",
    );
    // 记一套配置**不算导过一趟**。
    assert_eq!(现场.catalog.exported_at().expect("读得出"), None);
}

#[test]
fn 格式打错时当场说清眼下带的是哪几个() {
    // **判据在核心里**（ADR-0005）：界面那一层只把这句话转出来，不自己判「这个格式
    // 有没有」——散一份判断出去，命令行与界面迟早会对同一个字给出两种答复。
    let 错 = romcat_core::catalog::ExportSetup::check("Pegasüs", "/一个/目录")
        .expect_err("没有这个格式");
    let 那一句 = 错.to_string();
    assert!(那一句.contains("Pegasüs"), "没说清打错的是哪个：{那一句}");
    for 带得出来的 in romcat_core::adapter::names() {
        assert!(
            那一句.contains(带得出来的),
            "没列出眼下带的那几个：{那一句}",
        );
    }

    let 错 = romcat_core::catalog::ExportSetup::check("Pegasus", "   ").expect_err("没给目录");
    assert!(错.to_string().contains("目录"), "{错}");
}

#[test]
fn 每一份都撞上外面有人动过_那一趟不打时刻戳() {
    // **四档收场之外的第五种「什么都没写」**：这一趟走的是正常出口、报告也没有
    // `interrupted`，可每一份都被挡下，盘上一个字节都没多。那时说「上次跑是刚刚」，
    // 工序段那一行就在骗人（规格第 31 条「至少不骗我」）——判据统一成
    // **真往盘上写过东西**，与只排计划、按停那两档同一条。
    let 甲 = 建库("stage-导出全挡下", 3);
    let mut 现场 = 现场::摆好();
    现场.扫("甲", 甲.path());
    let out = 现场.工作区.path().join("导出去");

    // 落点上先躺着一份**我们从没见过**的文件——那可能就是维护者的原件，
    // 覆盖等于把它抹掉（`transfer` 的模块文档）。
    let 落点 = out.join("FC.metadata.pegasus.txt");
    写(&落点, b"# \xe6\x89\x8b\xe5\x86\x99\xe7\x9a\x84\n");
    let 手写的 = fs::read_to_string(&落点).expect("读得出");

    let report = 现场.导出();
    assert!(!report.conflicts.is_empty(), "该被挡下：{report:#?}");
    assert!(
        report.files.iter().all(|file| !file.written),
        "一份都不该写出去：{:#?}",
        report.files,
    );
    assert_eq!(
        fs::read_to_string(&落点).expect("读得出"),
        手写的,
        "**没有静默覆盖**：手写的那份还在",
    );
    // **一个字节都没写，就不算导过一趟。**
    assert_eq!(现场.catalog.exported_at().expect("读得出"), None);
    assert!(现场.导出那一行().render().contains("还没跑过"));
}
