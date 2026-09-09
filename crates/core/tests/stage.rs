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
