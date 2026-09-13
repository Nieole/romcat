//! **量级测量**：整理标题与导出各跑一趟要多久，折算到真库是几秒——还是不是比一帧的
//! 预算差着两个数量级。
//!
//! ## 为什么留在树里
//!
//! 工序段上**整理标题**与**导出**两行退回显示时刻，理由之一是那两个「还差多少」只有
//! 跑一遍那道工序才算得出来，而那一行在画帧那条线程上现算（`romcat_core::stage::Stage`
//! 那两支的文档）。撑着这句话的是两趟一次性的量，量完就删了（挂单 `Q426`、`Q436`），
//! 要复核得照条目里那段重搭一遍。这份文件就是那两趟，留下来了。
//!
//! ## 这不是回归测试
//!
//! 两条基准都挂着 `#[ignore]`：门禁不跑，平常的 `cargo test` 也不跑。它们**不断言耗时**
//! ——挂钟数在一台忙着的机器上是彩票——只把量到的数与折算印出来给人读。
//! 要看的是**差几个数量级**，不是慢了 5%，所以不引 criterion、不建 `benches/`。
//! `#[ignore]` 在这个仓库只给这种东西，房规在 `docs/agents/long-jobs.md`。
//!
//! 跑法（报告直接写进 stderr，不必加 `--nocapture`）：
//!
//! ```sh
//! cargo test -p romcat-core --test magnitude -- --ignored
//! ```
//!
//! 默认是 debug 构建，与当初那两趟同一个口径；加 `--release` 量的是另一件事，
//! 报告头上会写明是哪一种。
//!
//! ## 夹具只有一份
//!
//! 两条基准量的是**同一种合成库**（`合成库::造`）：N 个变体摊在十个卡带平台上，扫描、
//! 识别（每个变体撞上一条合成的 DAT 条目，于是各自认出一部作品）、离线刮削、整理标题
//! 都跑过——也就是工序段那一行被现算时库所处的样子。
//!
//! 唯一一条不挂 `#[ignore]` 的测试钉的是夹具本身：**造 N 个变体就真有 N 个**，每个都
//! 认出了作品。夹具悄悄少造了，基准照样印得出一个数，只是量的不是报告上说的那个库。
//!
//! 主库拿临时目录模拟，一个字节都不碰真实设备（ADR-0004）。

use std::collections::BTreeSet;
use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use romcat_core::adapter::converge;
use romcat_core::adapter::pegasus::Pegasus;
use romcat_core::adapter::transfer;
use romcat_core::catalog::{Catalog, Roots};
use romcat_core::dat::Convention;
use romcat_core::dat::logiqx::{DatHeader, GameRecord, RomRecord};
use romcat_core::dat::repo::{DatMeta, DatRepo, Unit};
use romcat_core::fs::RealFs;
use romcat_core::identify::{self, fuzzy};
use romcat_core::report::thousands;
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::scrape::{self, Priorities};
use romcat_core::task::Handle;
use romcat_core::testing::container::{ZipEntrySpec, crc32, zip_container};
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::{title, verdict};

/// 折算到真库时乘的那个变体数：**真库变体数的量级，不是确切数**。
///
/// 确切数带着日期与出处只记在 `docs/library-facts.md`（台账的规矩：别处要用就指过去，
/// 不抄）。这两条基准回答的是「差几个数量级」，要的只是量级，所以取五万：真库那个数
/// 往上取到一位有效数字。台账哪天多出几百个变体，这个量级照样对；抄一个确切数过来，
/// 它就是台账之外会漂的第二处（挂单 `Q500`、`Q531`）。
const 真库变体数的量级: u32 = 50_000;

/// 一帧的预算。工序段那一行在画帧那条线程上现算，一秒 60 帧取整成 16 毫秒——
/// 与 `romcat_core::stage::Stage` 那两支文档里说的是同一个预算。
const 一帧: Duration = Duration::from_millis(16);

/// 两条基准造的合成库有多大。与当初导出那一趟同一个个头（整理标题那一趟是 2,000）：
/// 大到被量的那几项一趟上百毫秒、挂钟抖动淹不掉，又小到一份库几秒钟造得完。
const 量的变体数: usize = 4_000;

/// 十个卡带平台。成型都走「一文件一变体」（`platforms.toml`），一份 zip 就是一个变体。
const 平台: [&str; 10] = [
    "FC", "SFC", "GB", "GBC", "GBA", "NDS", "N64", "MD", "SMS", "GG",
];

const 根名: &str = "库";

/// **这份文件里的测试一次只跑一条。** libtest 默认并行跑测试，两条基准同时造库、
/// 同时掐表，量到的是它们互相抢机器的样子；`--include-ignored` 时夹具那条也掺进来。
/// 锁住之后不必再记得加 `--test-threads 1`。
static 一次只跑一条: Mutex<()> = Mutex::new(());

fn 独占() -> MutexGuard<'static, ()> {
    一次只跑一条.lock().unwrap_or_else(PoisonError::into_inner)
}

/// 一份**合成库**：扫描、识别、刮削、整理标题都跑过的中立库，连同它那份临时主库。
struct 合成库 {
    _主库: TempDir,
    _媒体池: TempDir,
    catalog: Catalog,
}

/// 第 `i` 个变体的字节：前 8 个字节是编号，于是每一份内容都不同、各撞各的那一条 DAT 条目。
fn 内容(i: usize) -> Vec<u8> {
    let mut bytes = vec![0x5A; 256];
    bytes[..8].copy_from_slice(&(i as u64).to_le_bytes());
    bytes
}

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

impl 合成库 {
    /// 造 `变体数` 个变体，轮流摊在前 `平台数` 个平台上；每个变体在 DAT 里有自己的一条
    /// No-Intro 条目（CRC-32 加大小），于是识别给它认出一条发行版、一部作品。
    fn 造(变体数: usize, 平台数: usize) -> Self {
        let 主库 = temp_dir("magnitude-library");
        let mut 各平台的条目: Vec<Vec<GameRecord>> = vec![Vec::new(); 平台数];
        for i in 0..变体数 {
            let 第几个平台 = i % 平台数;
            let bytes = 内容(i);
            let rom = format!("Game {i:05}.bin");
            写(
                &主库
                    .path()
                    .join(format!("{}/游戏{i:05}.zip", 平台[第几个平台])),
                &zip_container(&[ZipEntrySpec::stored(&rom, bytes.clone())]),
            );
            各平台的条目[第几个平台].push(GameRecord {
                name: format!("Game {i:05} (USA)"),
                roms: vec![RomRecord {
                    name: rom,
                    size: Some(bytes.len() as u64),
                    crc32: Some(crc32(&bytes)),
                    ..RomRecord::default()
                }],
                ..GameRecord::default()
            });
        }

        let mut repo = DatRepo::in_memory().expect("开得出 DAT 库");
        for (名, games) in 平台.iter().zip(&各平台的条目) {
            let dat = format!("{名} - 合成");
            let mut writer = repo
                .begin(&Unit {
                    source: "No-Intro".to_string(),
                    name: dat.clone(),
                    url: "https://example.invalid/x".to_string(),
                    fingerprint: "sha".to_string(),
                })
                .expect("开得了事务");
            writer
                .write_dat(
                    &DatMeta {
                        name: dat,
                        platform: (*名).to_string(),
                        convention: Convention::AsIs,
                        header: DatHeader::default(),
                    },
                    games,
                )
                .expect("写得进");
            writer.commit().expect("提交");
        }

        let mut catalog = Catalog::open_in_memory().expect("能开中立库");
        let mut options = ScanOptions::named(主库.path(), 根名);
        options.jobs = Jobs::Fixed(4);
        scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");

        identify::run(
            &RealFs::new(),
            &mut catalog,
            &identify::Ammo {
                repo: &repo,
                verdicts: &verdict::Index::empty(),
                naming: &fuzzy::Naming::off(),
                guessing: &identify::model::Guessing::off(),
                titledb: None,
            },
            &identify::Options::new(Roots::single(根名, 主库.path())),
            &CancelToken::new(),
            &mut |_| {},
        )
        .expect("识别不该失败");

        let 媒体池 = temp_dir("magnitude-pool");
        let mut options = scrape::Options::new(Roots::single(根名, 主库.path()), 媒体池.path());
        options.media = false;
        scrape::run(
            &RealFs::new(),
            &mut catalog,
            &Priorities::builtin(),
            &options,
            None,
            &mut scrape::RunContext {
                cancel: &CancelToken::new(),
                progress: &mut |_| {},
                naming: &fuzzy::Naming::off(),
                summaries: None,
                rulings: &scrape::zh::Rulings::none(),
            },
        )
        .expect("刮削不该失败");

        let store = verdict::Store::in_memory().expect("开得出沉淀库");
        title::run(&mut catalog, &store, &Priorities::builtin()).expect("整理标题不该失败");

        Self {
            _主库: 主库,
            _媒体池: 媒体池,
            catalog,
        }
    }

    /// 报告里说明这份库的那一行——三个数都是从库里读回来的，不是照 `造` 的参数抄的。
    fn 说明(&self) -> String {
        let variants = self.catalog.variants().expect("读得出变体");
        let 平台数 = variants
            .iter()
            .filter_map(|variant| variant.platform.as_deref())
            .collect::<BTreeSet<_>>()
            .len();
        format!(
            "  合成库：{} 个变体 / {} 个作品，摊在 {平台数} 个平台上",
            thousands(variants.len() as u64),
            thousands(self.catalog.work_names().expect("读得出作品").len() as u64),
        )
    }
}

/// 同一件事连跑 `趟数` 趟，交回每一趟的耗时。
fn 连跑(趟数: usize, mut 一趟: impl FnMut()) -> Vec<Duration> {
    (0..趟数)
        .map(|_| {
            let 起 = Instant::now();
            一趟();
            起.elapsed()
        })
        .collect()
}

fn 题头(工序: &str) -> String {
    let 构建 = if cfg!(debug_assertions) {
        "debug 构建"
    } else {
        "release 构建"
    };
    format!("\n══ 量级 · {工序} · {构建} ══")
}

/// 一项的耗时，连同**线性折算到真库量级**之后比一帧的预算差几个数量级。
fn 折算(名: &str, 耗时: &[Duration]) -> String {
    let 最快 = 耗时.iter().min().copied().unwrap_or_default();
    let 最慢 = 耗时.iter().max().copied().unwrap_or_default();
    let 放大 = f64::from(真库变体数的量级) / 量的变体数 as f64;
    let 真库最快 = 最快.as_secs_f64() * 放大;
    let 真库最慢 = 最慢.as_secs_f64() * 放大;
    let 倍数 = 真库最慢 / 一帧.as_secs_f64();
    format!(
        "  {名}（{} 趟）：{:.1}–{:.1} 毫秒\n    \
         折算到真库量级（{} 个变体）：{:.2}–{:.2} 秒，慢的那头是一帧 {} 毫秒的 {:.0} 倍，\
         差 {:.1} 个数量级",
        耗时.len(),
        最快.as_secs_f64() * 1_000.0,
        最慢.as_secs_f64() * 1_000.0,
        thousands(u64::from(真库变体数的量级)),
        真库最快,
        真库最慢,
        一帧.as_millis(),
        倍数,
        倍数.log10(),
    )
}

const 脚注: &str = "  （折算按变体数线性放大；5 万是真库变体数的量级，\
                    确切数只记在 docs/library-facts.md）";

/// 把报告印出来。**直接写 stderr**：libtest 截的是 `print!` / `eprint!` 那几个宏，
/// 不截直接写进去的字节——于是 `-- --ignored` 不带 `--nocapture` 也看得见这份报告。
fn 印(报告: &str) {
    std::io::stderr()
        .lock()
        .write_all(format!("{报告}\n").as_bytes())
        .expect("写得进 stderr");
}

#[test]
fn 合成库造几个变体就真有几个_每个都认出了作品() {
    let _独占 = 独占();
    let 库 = 合成库::造(12, 3);

    let variants = 库.catalog.variants().expect("读得出变体");
    assert_eq!(
        variants.len(),
        12,
        "造了 12 个变体，库里却是 {}",
        variants.len()
    );
    for 名 in &平台[..3] {
        let 这个平台 = variants
            .iter()
            .filter(|variant| variant.platform.as_deref() == Some(*名))
            .count();
        assert_eq!(这个平台, 4, "{名} 底下该摊到 4 个变体，实际 {这个平台}");
    }
    assert_eq!(
        库.catalog.work_names().expect("读得出作品").len(),
        12,
        "每个变体都该认出自己那一部作品",
    );
}

#[test]
#[ignore = "量级测量，要人主动跑、一趟要造一份几千个变体的合成库：cargo test -p romcat-core --test magnitude -- --ignored"]
fn 整理标题一趟的量级() {
    let _独占 = 独占();
    let 库 = 合成库::造(量的变体数, 平台.len());

    let mut 叫法 = 0;
    let fold = 连跑(3, || {
        叫法 = title::fold(&库.catalog).expect("算得出标题集合").len();
    });

    印(&[
        题头("整理标题"),
        库.说明(),
        format!("  title::fold 一趟交出 {} 条叫法", thousands(叫法 as u64)),
        折算("title::fold", &fold),
        脚注.to_string(),
    ]
    .join("\n"));
}

#[test]
#[ignore = "量级测量，要人主动跑、一趟要造一份几千个变体的合成库：cargo test -p romcat-core --test magnitude -- --ignored"]
fn 导出一趟的量级() {
    let _独占 = 独占();
    let mut 库 = 合成库::造(量的变体数, 平台.len());
    let priorities = Priorities::builtin();
    let 导出目录 = temp_dir("magnitude-export");

    let mut 条目 = 0;
    let mut 份数 = 0;
    let 收敛 = 连跑(3, || {
        let converged = converge::run(&库.catalog, &priorities, &Pegasus).expect("收敛得出条目");
        条目 = converged.entries;
        份数 = converged.files.len();
    });
    // 整趟导出只排计划、不写盘：量的是「算出那个数」要花多少，不是写文件要花多少。
    let 整趟 = 连跑(2, || {
        transfer::export(
            &mut 库.catalog,
            &Pegasus,
            &priorities,
            &transfer::ExportOptions {
                out: 导出目录.path().join("导出去"),
                dry_run: true,
                force: false,
            },
        )
        .expect("排得出导出计划");
    });

    印(&[
        题头("导出"),
        库.说明(),
        format!(
            "  收敛成 {} 个条目、{份数} 份元数据文件（Pegasus）",
            thousands(条目)
        ),
        折算("converge::run", &收敛),
        折算("transfer::export，只排计划不写盘", &整趟),
        脚注.to_string(),
    ]
    .join("\n"));
}
