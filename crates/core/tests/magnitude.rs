//! **量级测量**：整理标题与导出各跑一趟要多久，折算到真库是几秒——还是不是比一帧的
//! 预算差着两个数量级；外加刮削弹层「读取硬盘」那一格要的那一问在真库个头上要多久。
//!
//! ## 为什么留在树里
//!
//! 工序段上**整理标题**与**导出**两行退回显示时刻，理由之一是那两个「还差多少」只有
//! 跑一遍那道工序才算得出来，而那一行在画帧那条线程上现算（`romcat_core::stage::Stage`
//! 那两支的文档）。撑着这句话的是两趟一次性的量，量完就删了（挂单 `Q426`、`Q436`），
//! 要复核得照条目里那段重搭一遍。这份文件就是那两趟，留下来了。
//!
//! 刮削弹层「读取硬盘」那一格写不写数，撑着的是第三趟（票 `gui-draws-the-rest-of-the-design/17`
//! 的 `F-11`）：那一问在摊开弹层那一刻、画帧那条线程上现算，超过一两百毫秒就不现算。
//!
//! ## 这不是回归测试
//!
//! 三条基准都挂着 `#[ignore]`：门禁不跑，平常的 `cargo test` 也不跑。它们**不断言耗时**
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
//! 报告头上会写明是哪一种。读盘那一问要拿去比「一两百毫秒」那道门槛，得用 `--release` 量
//! （交付出去的界面是 release 构建）。
//!
//! ## 夹具两份
//!
//! 整理标题与导出量的是**同一种合成库**（`合成库::造`）：N 个变体摊在十个卡带平台上，扫描、
//! 识别（每个变体撞上一条合成的 DAT 条目，于是各自认出一部作品）、离线刮削、整理标题
//! 都跑过——也就是工序段那一行被现算时库所处的样子。
//!
//! 读盘那一问量的是另一份（`读盘库::造`）：只有文件表与变体表，照真库的形状与个头摆——那一问只读这两样，
//! 而它的代价跟着全库的文件表走，造小了再放大量的不是同一件事。
//!
//! 两份夹具各有一条不挂 `#[ignore]` 的测试钉着夹具本身：**造 N 个变体就真有 N 个**、每个都
//! 认出了作品；读盘那份归得上的只有同名兄弟、转储里的图真写进了文件表。夹具悄悄少造了，
//! 基准照样印得出一个数，只是量的不是报告上说的那个库。
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
use romcat_core::catalog::{Catalog, EntryRecord, Roots, Verdict};
use romcat_core::dat::Convention;
use romcat_core::dat::logiqx::{DatHeader, GameRecord, RomRecord};
use romcat_core::dat::repo::{DatMeta, DatRepo, Unit};
use romcat_core::fs::{EntryKind, EntryMeta, RealFs};
use romcat_core::identify::{self, fuzzy};
use romcat_core::platform::Manifest;
use romcat_core::report::{human_bytes, thousands};
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::scrape::pool::MediaPool;
use romcat_core::scrape::{self, Priorities, local};
use romcat_core::shape::{Role, SINGLE_FILE_RULE, Variant};
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
                media: None,
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

// ——— 刮削弹层「读取硬盘」那一格要的那一问（票 `gui-draws-the-rest-of-the-design/17` 的 `F-11` / 差距 `C-1`） ———
//
// 那一格要的数是「这一批收媒体首趟要从主库读多少字节」：拿本地媒体源那份归属表（`local::index`）按范围求一次和，
// 已经入池的不算。票上定的是**先量一次那一问多久**：超过一两百毫秒就不在摊开弹层那一刻现算，退成「读多少要读过才知道」。
// 弹层摊开那一下在画帧那条线程上，一百多毫秒就是一下看得见的卡顿。

/// 读盘那一问量的合成库有多少个变体：**真库的个头**，不线性折算。
///
/// 那一问的代价跟着**全库的文件表**走，不跟着这一批有多大走：它要把每一条文件记录读出来挑媒体扩展名，再把全部变体
/// 读一遍定「独占目录」——全选整库与只勾一行花的是同一笔。所以直接造一份真库那么大的，不造小的再放大。
const 读盘_变体数: usize = 真库变体数的量级 as usize;

/// 每多少个变体里有一份**目录树转储**（一个变体吞掉一整棵目录）。
///
/// 真库的文件表大半是这种转储里的内部资源：几百份转储吞掉十几万个文件（台账 `docs/library-facts.md`「成型：从文件到变体」）。
const 读盘_转储间隔: usize = 200;

/// 一份转储里有多少个文件，其中多少个带媒体扩展名。
///
/// 照台账的比例摆：每份转储平均七百来个文件；全库带媒体或元数据扩展名的文件四五万个（「容器构成」那一节），
/// 几乎全在转储里——它们够不着归属规则，一张都不归，可那一问照样要把它们一条条读出来。
const 读盘_转储文件数: usize = 700;
const 读盘_转储里的图: usize = 180;

/// 每多少个变体带三份**同名兄弟**（`游戏.zip` 旁边的 `游戏.png` / `.jpg` / `.mp4`）——那一问真正归得上的那几百份
/// （台账「成型：从文件到变体」底下「媒体」那一小节：归得上的几百份）。
const 读盘_兄弟间隔: usize = 200;

/// 一份同名兄弟多大。真库归得上的那几百份合起来十来 GiB（同一节），大头是视频；这里一律取 16 MiB，求出来的和只看量级。
const 读盘_兄弟字节: u64 = 16 * 1024 * 1024;

/// 一份**照真库形状摆的合成库**：只有文件表与变体表——读盘那一问只读这两样。写在盘上：真库是盘上的一份 SQLite。
struct 读盘库 {
    _目录: TempDir,
    catalog: Catalog,
}

impl 读盘库 {
    fn 造(变体数: usize) -> Self {
        let 目录 = temp_dir("magnitude-first-read");
        let mut catalog =
            Catalog::create(&目录.path().join("catalog.sqlite"), "读盘量级").expect("建得出中立库");
        let 文件 = |key: String, len: u64| EntryRecord {
            key,
            kind: EntryKind::File,
            meta: EntryMeta::Known {
                len,
                modified: None,
            },
            non_utf8: false,
            verdict: Verdict::Added,
            sample: None,
            container: None,
        };
        let mut entries = Vec::new();
        let mut variants = Vec::with_capacity(变体数);
        for i in 0..变体数 {
            let 平台名 = 平台[i % 平台.len()];
            if i % 读盘_转储间隔 == 读盘_转储间隔 - 1 {
                // 一份转储：主文件是那个目录，内部资源躺在更深的目录里（两条归属规则都够不着）。
                let key = format!("{根名}/PSV/转储{i:06}");
                let mut members = Vec::with_capacity(读盘_转储文件数);
                for at in 0..读盘_转储文件数 {
                    let 扩展名 = if at < 读盘_转储里的图 {
                        "png"
                    } else {
                        "at9"
                    };
                    let member = format!("{key}/资源/{:02}/{at:04}.{扩展名}", at % 16);
                    entries.push(文件(member.clone(), 64 * 1024));
                    members.push((member, Role::Internal));
                }
                variants.push(Variant {
                    main_key: key.clone(),
                    platform: Some("PSV".to_string()),
                    rule: "PSV 目录树".to_string(),
                    manual: false,
                    files: 读盘_转储文件数 as u64,
                    bytes: 读盘_转储文件数 as u64 * 64 * 1024,
                    unreadable_files: 0,
                    members,
                    key,
                });
                continue;
            }
            // 其余一文件一变体，几千个挤在同一个平台目录里（真库 `FC/` 底下就是这样）。
            let key = format!("{根名}/{平台名}/游戏{i:06}.zip");
            entries.push(文件(key.clone(), 4 * 1024 * 1024));
            if i % 读盘_兄弟间隔 == 0 {
                for 扩展名 in ["png", "jpg", "mp4"] {
                    entries.push(文件(
                        format!("{根名}/{平台名}/游戏{i:06}.{扩展名}"),
                        读盘_兄弟字节,
                    ));
                }
            }
            variants.push(Variant {
                main_key: key.clone(),
                platform: Some(平台名.to_string()),
                rule: SINGLE_FILE_RULE.to_string(),
                manual: false,
                files: 1,
                bytes: 4 * 1024 * 1024,
                unreadable_files: 0,
                members: vec![(key.clone(), Role::Main)],
                key,
            });
        }
        catalog.write(1, &entries).expect("写得进文件表");
        catalog
            .replace_variants(&variants, 1, &Manifest::default())
            .expect("写得进变体");
        Self {
            _目录: 目录,
            catalog,
        }
    }

    /// **那一问本身**：全部变体读一遍、归属表立一遍，按范围（这里是全选整库）求和，池里已经有的不算。
    /// 交回（归得上几份、合起来多少字节）。
    fn 那一问(&self, 池: &MediaPool) -> (u64, u64) {
        let variants = self.catalog.variants().expect("读得出变体");
        let index = local::index(&self.catalog, &variants).expect("立得出归属表");
        let (mut 份数, mut 字节) = (0_u64, 0_u64);
        for media in index.values().flatten() {
            let 入过池 = self
                .catalog
                .media_blob(&media.key)
                .expect("读得出")
                .and_then(|(_, hash)| {
                    let ext = self.catalog.media_ext(&hash).expect("读得出")?;
                    Some(池.contains(&hash, &ext))
                })
                .unwrap_or(false);
            if !入过池 {
                份数 += 1;
                字节 += media.bytes.unwrap_or(0);
            }
        }
        (份数, 字节)
    }

    /// 报告里说明这份库的那一行——数都是从库里读回来的。
    fn 说明(&self) -> String {
        let mut 文件数 = 0_u64;
        self.catalog
            .for_each_file(&mut |_, _, _| 文件数 += 1)
            .expect("数得出文件");
        format!(
            "  合成库（盘上一份 SQLite）：{} 个变体、{} 条文件记录",
            thousands(self.catalog.variants().expect("读得出变体").len() as u64),
            thousands(文件数),
        )
    }
}

#[test]
fn 读盘那一问的合成库照真库的形状摆_归得上的只有同名兄弟() {
    let _独占 = 独占();
    let 库 = 读盘库::造(400);
    let 池目录 = temp_dir("magnitude-first-read-pool");
    let (份数, 字节) = 库.那一问(&MediaPool::at(池目录.path()));
    // 400 个变体里每 200 个带三份同名兄弟（第 0 个、第 200 个），转储里那几百张图一张都归不上。
    assert_eq!(份数, 6, "归得上的该只有那两组同名兄弟");
    assert_eq!(字节, 6 * 读盘_兄弟字节);
    let mut 带媒体扩展名的 = 0;
    库.catalog
        .for_each_file(&mut |key, _, _| {
            if key.ends_with(".png") || key.ends_with(".jpg") || key.ends_with(".mp4") {
                带媒体扩展名的 += 1;
            }
        })
        .expect("数得出文件");
    assert_eq!(
        带媒体扩展名的,
        6 + 2 * 读盘_转储里的图,
        "两份转储里的图也得真写进文件表——那一问的代价正在它们身上",
    );
}

#[test]
#[ignore = "量级测量，要人主动跑、一趟要造一份真库那么大的合成库：cargo test --release -p romcat-core --test magnitude -- --ignored 读盘"]
fn 读盘那一问的量级() {
    let _独占 = 独占();
    let 起 = Instant::now();
    let 库 = 读盘库::造(读盘_变体数);
    let 造库 = 起.elapsed();
    let 池目录 = temp_dir("magnitude-first-read-pool");
    let 池 = MediaPool::at(池目录.path());

    let mut 答 = (0, 0);
    let 耗时 = 连跑(5, || 答 = 库.那一问(&池));
    let 最快 = 耗时.iter().min().copied().unwrap_or_default();
    let 最慢 = 耗时.iter().max().copied().unwrap_or_default();

    印(&[
        题头("刮削弹层读盘那一问"),
        库.说明(),
        format!("  造库 {:.1} 秒（不算在那一问里）", 造库.as_secs_f64()),
        format!(
            "  那一问（全选整库）：归得上 {} 份、{}；{} 趟 {:.1}–{:.1} 毫秒",
            thousands(答.0),
            human_bytes(答.1),
            耗时.len(),
            最快.as_secs_f64() * 1_000.0,
            最慢.as_secs_f64() * 1_000.0,
        ),
        "  （不折算：这份库就是真库的个头。票上的门槛是一两百毫秒——摊开弹层那一下在画帧线程上现算）".to_string(),
    ]
    .join("\n"));
}
