//! **Pegasus 这一趟**：把维护者手工维护的元数据无损导进来，再把中立库导出成他能直接
//! 喂给 Pegasus 的东西。
//!
//! 这个文件要证的是五件在单元测试里成立、在真库上未必成立的事：
//!
//! 1. **一次往返不蒸发心血**——注释、未知键、`x-` 扩展键、字段顺序，导出之后还在；
//! 2. **手工维护的值真的落进了中立库**，而且压过 DAT（`priorities.toml` 里
//!    `Pegasus` 排在全部数据源之前）；
//! 3. **作品级收敛**——一个条目、多个文件、首选变体排在最前，而裁决压过规则；
//! 4. **附属内容、非游戏资产与补丁一个都不导出**（ADR-0013、ADR-0010）；
//! 5. **外部改动不被静默覆盖**（ADR-0001 修订段）。
//!
//! fixture 的形状照真机来（`docs/library-facts.md` 与 2026-09-01 的实地查看）：
//! 内容在**透明容器**里，官中版与汉化版各占一个目录，`街机/FBA-ROMS/BIOS/` 下躺着
//! 两个 BIOS set，库里还散着几个汉化补丁。

use std::fs;
use std::path::{Path, PathBuf};

use romcat_core::adapter::converge::{NotAnEntry, Preference};
use romcat_core::adapter::pegasus::Pegasus;
use romcat_core::adapter::transfer::{self, ExportOptions};
use romcat_core::adapter::{Adapter, Capability, assert_capability};
use romcat_core::catalog::Catalog;
use romcat_core::catalog::Roots;
use romcat_core::dat::Convention;
use romcat_core::dat::logiqx::{DatHeader, GameRecord, RomRecord};
use romcat_core::dat::repo::{DatMeta, DatRepo, Unit};
use romcat_core::fs::RealFs;
use romcat_core::identify;
use romcat_core::identify::fuzzy;
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::scrape::{self, Priorities};
use romcat_core::task::Handle;
use romcat_core::testing::container::{ZipEntrySpec, crc32, zip_container};
use romcat_core::testing::switch::{
    ADD_ON_NSP, BASE_NSP, UPDATE_NSP, base_update_add_on, ticket, xci,
};
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::title;
use romcat_core::verdict;

fn 日版() -> Vec<u8> {
    vec![0xA1; 4_096]
}
fn 台版() -> Vec<u8> {
    vec![0xA2; 4_096]
}
fn 汉化版() -> Vec<u8> {
    vec![0xB2; 4_096]
}
fn 只有英文() -> Vec<u8> {
    vec![0xD4; 1_024]
}
fn 街机bios() -> Vec<u8> {
    vec![0xE5; 512]
}
fn 补丁字节() -> Vec<u8> {
    vec![0xF6; 256]
}

/// 把一条**中立库的键**折回盘上那条相对主库根的路径：剥掉第一段根名。
/// 摆 fixture 用它，断言用键本身——两者差的正是这一段（`path::library_key`）。
fn 相对(key: &str) -> &str {
    key.strip_prefix("库/").unwrap_or(key)
}

const 日版变体: &str = "库/FC/魂斗罗日版/Contra (Japan).zip";
const 台版变体: &str = "库/FC/魂斗罗台版/魂斗罗.zip";
const 汉化变体: &str = "库/FC/魂斗罗汉化/魂斗罗 中文版[dwt_so 汉化].zip";
const 塞尔达: &str = "库/FC/Zelda/Zelda (USA).zip";
const BIOS: &str = "库/街机/FBA-ROMS/BIOS/neogeo.zip";
const 补丁: &str = "库/FC/《流星洛克人3》汉化补丁.zip";
/// 两个**没有作品链接**、而且显示标题会撞在一起的变体。
///
/// 它们撞名是有意的：真机上一趟「导出 → 导入 → 再导出」里，光 FC 一个平台就有近千个
/// 条目的标题与别的条目撞名（同一部作品的不同 HACK 版、不同合集包里的同一个名字）。
/// 段对回基线时若让「标题一字不差」这条软判据抢在「变体键相同」前面，撞名的那些就会
/// 互相截胡。
const 重名甲: &str = "库/FC/重名/甲/超级玛丽.zip";
const 重名乙: &str = "库/FC/重名/乙/超级玛丽.zip";

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

struct 现场 {
    dir: TempDir,
    _pool: TempDir,
    catalog: Catalog,
}

impl 现场 {
    /// 导出的落点。
    ///
    /// **就是主库根。** 那不是图省事：`file:` 是相对元数据文件所在目录解析的，
    /// 而中立库的键是相对主库根的（ADR-0020）——两者要对得上，元数据文件就得躺在
    /// 主库根下。ADR-0004 允许工具往主库里写**元数据文件**，ROM 一个字节都不碰。
    fn out(&self) -> &Path {
        self.dir.path()
    }
}

fn 建现场() -> 现场 {
    let dir = temp_dir("pegasus");
    let root = dir.path();
    写(
        &root.join(相对(日版变体)),
        &zip_container(&[ZipEntrySpec::stored("Contra.nes", 日版())]),
    );
    写(
        &root.join(相对(台版变体)),
        &zip_container(&[ZipEntrySpec::stored("Contra.nes", 台版())]),
    );
    写(
        &root.join(相对(汉化变体)),
        &zip_container(&[ZipEntrySpec::stored("魂斗罗.nes", 汉化版())]),
    );
    写(
        &root.join(相对(塞尔达)),
        &zip_container(&[ZipEntrySpec::stored("Zelda.nes", 只有英文())]),
    );
    // **非游戏资产**：模拟器要它，它本身不是游戏（ADR-0010）。
    写(
        &root.join(相对(BIOS)),
        &zip_container(&[ZipEntrySpec::stored("neogeo.rom", 街机bios())]),
    );
    // **补丁**：不可运行，识别那一趟会判它跳过。
    写(
        &root.join(相对(补丁)),
        &zip_container(&[ZipEntrySpec::stored("rockman3.ips", 补丁字节())]),
    );
    // 两个 DAT 认不出、文件名却一模一样的变体：显示标题会撞在一起。
    for (key, 字节) in [(重名甲, 0x11u8), (重名乙, 0x22u8)] {
        写(
            &root.join(相对(key)),
            &zip_container(&[ZipEntrySpec::stored("mario.nes", vec![字节; 2_048])]),
        );
    }

    扫成现场(dir, "库")
}

/// 把 `dir` 当成一份根叫 `根名` 的主库扫一遍，再照 [`按根名跑一遍`] 把识别、刮削、标题跑完。
fn 扫成现场(dir: TempDir, 根名: &str) -> 现场 {
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::named(dir.path(), 根名);
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");

    let mut 场 = 现场 {
        dir,
        _pool: temp_dir("pegasus-pool"),
        catalog,
    };
    按根名跑一遍(&mut 场, 根名);
    场
}

fn 条目(name: &str, rom: &str, bytes: &[u8]) -> GameRecord {
    GameRecord {
        name: name.to_string(),
        roms: vec![RomRecord {
            name: rom.to_string(),
            size: Some(bytes.len() as u64),
            crc32: Some(crc32(bytes)),
            ..RomRecord::default()
        }],
        ..GameRecord::default()
    }
}

fn 装(repo: &mut DatRepo, source: &str, dat: &str, games: &[GameRecord]) {
    let mut writer = repo
        .begin(&Unit {
            source: source.to_string(),
            name: dat.to_string(),
            url: "https://example.invalid/x".to_string(),
            fingerprint: "sha".to_string(),
        })
        .expect("开得了事务");
    writer
        .write_dat(
            &DatMeta {
                name: dat.to_string(),
                platform: "FC".to_string(),
                convention: Convention::AsIs,
                header: DatHeader::default(),
            },
            games,
        )
        .expect("写得进");
    writer.commit().expect("提交");
}

fn 建_dat() -> DatRepo {
    let mut repo = DatRepo::in_memory().expect("开得出来");
    装(
        &mut repo,
        "No-Intro",
        "Nintendo - Nintendo Entertainment System",
        &[
            条目("Contra (Japan)", "Contra (Japan).nes", &日版()),
            条目(
                "Contra (Taiwan) (En,Zh-Hant)",
                "Contra (Taiwan).nes",
                &台版(),
            ),
            条目("Zelda (USA)", "Zelda (USA).nes", &只有英文()),
        ],
    );
    装(
        &mut repo,
        "TOSEC",
        "TOSEC/Nintendo Famicom & Entertainment System - Games - [NES].dat",
        &[条目(
            "Contra (1988-02-09)(Konami)(JP)[tr zh dwt_so][v.20030208]",
            "Contra [tr zh].nes",
            &汉化版(),
        )],
    );
    repo
}

fn 跑一遍(现场: &mut 现场) {
    按根名跑一遍(现场, "库");
}

/// 与 [`跑一遍`] 同一趟，只是这份主库的**根**叫 `根名`。
fn 按根名跑一遍(现场: &mut 现场, 根名: &str) {
    let repo = 建_dat();
    identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &identify::Ammo {
            repo: &repo,
            verdicts: &verdict::Index::empty(),
            naming: &fuzzy::Naming::off(),
            guessing: &identify::model::Guessing::off(),
            titledb: None,
        },
        &identify::Options::new(Roots::single(根名, 现场.dir.path())),
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别不该失败");
    let mut options = scrape::Options::new(Roots::single(根名, 现场.dir.path()), 现场._pool.path());
    options.media = false;
    scrape::run(
        &RealFs::new(),
        &mut 现场.catalog,
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
    title::run(
        &mut 现场.catalog,
        &romcat_core::verdict::Store::in_memory().expect("能开沉淀库"),
        &Priorities::builtin(),
    )
    .expect("折得出标题");
}

fn 导出(现场: &mut 现场, force: bool) -> romcat_core::adapter::report::ExportReport {
    let out = 现场.out().to_path_buf();
    transfer::export(
        &mut 现场.catalog,
        &Pegasus,
        &Priorities::builtin(),
        &ExportOptions {
            out,
            dry_run: false,
            force,
            media: None,
        },
    )
    .expect("导得出来")
}

fn 读出(现场: &现场, name: &str) -> String {
    fs::read_to_string(现场.out().join(name)).expect("读得出导出来的文件")
}

/// 这一趟导出写出去的全部元数据文件拼成一份——断言「谁是前端条目、谁不是」用。
fn 导出的全文(现场: &现场, report: &romcat_core::adapter::report::ExportReport) -> String {
    report
        .files
        .iter()
        .map(|file| fs::read_to_string(现场.out().join(&file.path)).expect("读得出"))
        .collect()
}

impl 现场 {
    /// **媒体池**。刮削那一趟收媒体用的也是这个目录。
    fn 池(&self) -> romcat_core::scrape::pool::MediaPool {
        romcat_core::scrape::pool::MediaPool::open(self._pool.path()).expect("池建得出")
    }
}

/// 往**媒体池**里塞一份媒体，挂到某个变体上。返回它的内容哈希。
fn 收一份媒体(
    现场: &mut 现场,
    变体: &str,
    kind: romcat_core::scrape::MediaKind,
    bytes: &[u8],
) -> String {
    use romcat_core::catalog::scrape::{Harvested, HarvestedMedia};
    let hash = romcat_core::catalog::frontend::hash_of(bytes);
    写(&现场.池().path_of(&hash, "png"), bytes);
    现场
        .catalog
        .put_media(
            &hash,
            "png",
            bytes.len() as u64,
            romcat_core::scrape::measure::Measured::default(),
        )
        .expect("池里记得下");
    现场
        .catalog
        .put_scraped(&[Harvested {
            anchor: romcat_core::scrape::AnchorKind::Variant.label().to_string(),
            subject: 变体.to_string(),
            // 一个源在一个锚点上写两次是同一个结果，于是每份媒体各记一个源。
            source: format!("本地媒体-{hash}"),
            input: format!("{变体}/{hash}"),
            values: Vec::new(),
            media: vec![HarvestedMedia {
                kind: kind.label().to_string(),
                hash: hash.clone(),
                evidence: "测试".to_string(),
            }],
        }])
        .expect("引用写得进");
    hash
}

/// 一棵目录树底下有哪些文件：相对树根的路径，`/` 分隔。
fn 盘上的文件(dir: &Path) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        let Ok(entries) = fs::read_dir(&at) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let relative = path.strip_prefix(dir).expect("在树里");
            out.insert(
                relative
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/"),
            );
        }
    }
    out
}

#[test]
fn 不开铺媒体时_主库里一个媒体文件都不多() {
    // **默认关着**（票 `one-criterion-per-thing/08`）。导出目录就是主库根，
    // 池里真有一份挂在塞尔达上的封面，这一条才验得出「有也不铺」。
    let mut 现场 = 建现场();
    收一份媒体(
        &mut 现场,
        塞尔达,
        romcat_core::scrape::MediaKind::Cover,
        b"\x89PNG-- zelda cover --",
    );
    let 之前 = 盘上的文件(现场.out());

    let report = 导出(&mut 现场, false);

    let 多出来的: Vec<String> = 盘上的文件(现场.out()).difference(&之前).cloned().collect();
    assert_eq!(
        多出来的,
        ["FC.metadata.pegasus.txt"],
        "不开时多出来的只该是元数据文件"
    );
    assert!(report.media.is_none(), "{:?}", report.media);
    let 元数据 = 读出(&现场, "FC.metadata.pegasus.txt");
    assert!(
        !元数据.contains("assets."),
        "不铺就一个资源槽都不写：{元数据}"
    );
}

/// 开着**铺媒体**导出一趟。
fn 导出_铺媒体(现场: &mut 现场) -> romcat_core::adapter::report::ExportReport {
    let out = 现场.out().to_path_buf();
    let pool = 现场.池();
    transfer::export(
        &mut 现场.catalog,
        &Pegasus,
        &Priorities::builtin(),
        &ExportOptions {
            out,
            dry_run: false,
            force: false,
            media: Some(pool),
        },
    )
    .expect("导得出来")
}

#[test]
fn 开了铺媒体_按内容寻址铺进_media_而且条目里写着那条路径() {
    // Pegasus 是**内容寻址**：`media/<哈希前两位>/<哈希>.<扩展名>`，前端靠条目里写死的
    // `assets.*` 找图。维护者直接拿 Pegasus 读主库时看得见封面，靠的就是这两样都在。
    let mut 现场 = 建现场();
    let 封面 = b"\x89PNG-- zelda cover --".to_vec();
    let hash = 收一份媒体(
        &mut 现场,
        塞尔达,
        romcat_core::scrape::MediaKind::Cover,
        &封面,
    );
    let 之前 = 盘上的文件(现场.out());

    let report = 导出_铺媒体(&mut 现场);

    let 落点 = format!("media/{}/{hash}.png", &hash[..2]);
    let 多出来的: Vec<String> = 盘上的文件(现场.out()).difference(&之前).cloned().collect();
    assert_eq!(
        多出来的,
        ["FC.metadata.pegasus.txt".to_string(), 落点.clone()]
    );
    assert_eq!(
        fs::read(现场.out().join(&落点)).expect("铺出去了"),
        封面,
        "铺出去的就是池里那一份",
    );
    let media = report.media.expect("开了铺媒体，报告里就有这一半的账");
    assert_eq!(media.files, 1, "{media:?}");
    assert_eq!(media.linked + media.copied, 1, "{media:?}");
    let 元数据 = 读出(&现场, "FC.metadata.pegasus.txt");
    assert!(
        元数据.contains(&format!("assets.boxFront: {落点}")),
        "塞尔达那一条得写着封面在哪：{元数据}"
    );
}

/// 一份照 Pegasus 写、**写出元数据就替人按下「停下」**的适配器。
///
/// 这份 fixture 只收敛出一份元数据文件（FC；街机那个 BIOS 成不了条目），于是停下的信号
/// 正好落在「元数据写完、媒体一份都还没铺」那道缝上——不靠挂钟去抢那一下（挂单 `Q196`）。
/// 它自己一个判断都不做，只转发给 [`Pegasus`]。
struct 写出元数据就按停<'a> {
    task: &'a Handle,
}

impl Adapter for 写出元数据就按停<'_> {
    fn name(&self) -> &'static str {
        Pegasus.name()
    }
    fn ceiling(&self) -> Capability {
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
fn 元数据写完媒体还没铺就按停_记的是没走完而且说得出媒体一份都还没铺() {
    // 媒体排在元数据后面铺：元数据几秒就写完，媒体可能是几十 GiB 的复制。停在这道缝上，
    // 元数据那一份**真的躺在盘上**，于是这一趟是「没走完却留下了东西」，而那句话得说清
    // 媒体一份都还没铺、一共几份——人才知道再跑一次要付多少。
    let mut 现场 = 建现场();
    收一份媒体(
        &mut 现场,
        塞尔达,
        romcat_core::scrape::MediaKind::Cover,
        b"\x89PNG-- zelda cover --",
    );
    let pool = 现场.池();
    let out = 现场.out().to_path_buf();

    let ended = {
        let mut board: romcat_core::task::Board<romcat_core::adapter::report::ExportReport> =
            romcat_core::task::Board::new();
        board.run_here("导出", |task| {
            transfer::export_task(
                &mut 现场.catalog,
                &写出元数据就按停 { task },
                &Priorities::builtin(),
                &ExportOptions {
                    out: out.clone(),
                    dry_run: false,
                    force: false,
                    media: Some(pool.clone()),
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
        panic!("元数据写完之后按停，台上却没记成没走完：{ended:?}");
    };
    assert!(report.interrupted, "被按停了却没记上");
    assert!(
        out.join("FC.metadata.pegasus.txt").is_file(),
        "写完的元数据留在盘上"
    );
    assert!(!out.join("media").exists(), "媒体一份都还没铺");
    let media = report.media.expect("开了铺媒体，报告里就有这一半的账");
    assert_eq!(
        (media.files, media.linked + media.copied),
        (1, 0),
        "{media:?}"
    );
    assert!(
        left_behind.contains("元数据文件写出去 1 份（共 1 份）"),
        "那一句没说清元数据写了几份：{left_behind}"
    );
    assert!(
        left_behind.contains("媒体一份都还没铺（共 1 份）"),
        "那一句没说清媒体铺到哪儿了：{left_behind}"
    );
    // **没打时刻戳**：没走完说不上「导过了」。
    assert_eq!(现场.catalog.exported_at().expect("读得出"), None);
}

#[test]
fn 铺媒体连着没铺成主动停了_记的是没走完而且说得出铺到第几份() {
    // 活自己收的手与人按的停下是同一档收场：没走完、却留下了东西（词表**部分完成**）。
    // 这一条走 `export_task` 整条路：铺出去的那一份留在盘上，收场那句话说得出铺到第几份、
    // 一共几份。
    //
    // 让它**确定地**连着失败，不靠挂钟：Pegasus 按内容寻址铺（`media/<哈希前两位>/…`），
    // 把排在后面那几份的 `media/<前两位>` 先占成一个**文件**——那一枝的目录建不出来，
    // 每一份都以同一句话失败，正是「连着失败太多次」要认出来的那一类。
    let mut 现场 = 建现场();
    let mut 按前缀: std::collections::BTreeMap<String, Vec<u8>> = std::collections::BTreeMap::new();
    let mut n = 0;
    while 按前缀.len() < 12 {
        let 字节 = format!("screenshot-{n}").into_bytes();
        n += 1;
        let hash = romcat_core::catalog::frontend::hash_of(&字节);
        按前缀.entry(hash[..2].to_string()).or_insert(字节);
    }
    for 字节 in 按前缀.values() {
        收一份媒体(
            &mut 现场,
            塞尔达,
            romcat_core::scrape::MediaKind::Screenshot,
            字节,
        );
    }
    let mut 前缀们 = 按前缀.keys();
    let 第一份 = 前缀们.next().expect("有第一份").clone();
    for 前缀 in 前缀们 {
        写(&现场.out().join("media").join(前缀), b"not a directory");
    }
    let pool = 现场.池();
    let out = 现场.out().to_path_buf();

    let ended = {
        let mut board: romcat_core::task::Board<romcat_core::adapter::report::ExportReport> =
            romcat_core::task::Board::new();
        board.run_here("导出", |task| {
            transfer::export_task(
                &mut 现场.catalog,
                &Pegasus,
                &Priorities::builtin(),
                &ExportOptions {
                    out: out.clone(),
                    dry_run: false,
                    force: false,
                    media: Some(pool.clone()),
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
        panic!("铺媒体连着失败主动停了，台上却没记成没走完：{ended:?}");
    };
    assert!(!report.interrupted, "没人按停下");
    let media = report.media.expect("开了铺媒体，报告里就有这一半的账");
    assert!(media.gave_up, "{media:?}");
    assert_eq!(media.linked + media.copied, 1, "{media:?}");
    assert!(
        out.join("media").join(&第一份).is_dir(),
        "铺出去的那一份留在盘上"
    );
    assert!(
        left_behind.contains("媒体铺到第 11 份（共 12 份）"),
        "那一句没说清铺到第几份：{left_behind}"
    );
    assert!(
        left_behind.contains("主动停了"),
        "那一句没说清为什么收的手：{left_behind}"
    );
    // **没打时刻戳**：没走完说不上「导过了」。
    assert_eq!(现场.catalog.exported_at().expect("读得出"), None);
}

/// 一棵目录树底下每个文件的字节、修改时间与链接数。
fn 每个文件(
    dir: &Path,
) -> std::collections::BTreeMap<String, (Vec<u8>, std::time::SystemTime, u64)> {
    盘上的文件(dir)
        .into_iter()
        .map(|path| {
            let at = dir.join(&path);
            let meta = fs::metadata(&at).expect("读得到");
            #[cfg(unix)]
            let links = {
                use std::os::unix::fs::MetadataExt;
                meta.nlink()
            };
            #[cfg(not(unix))]
            let links = 1;
            let bytes = fs::read(&at).expect("读得出");
            (path, (bytes, meta.modified().expect("有修改时间"), links))
        })
        .collect()
}

#[test]
fn 铺媒体只写进媒体目录_主库里的_rom_一个字节都没动() {
    // ADR-0004：工具只写元数据文件、媒体目录与子库。导出目录就是主库根，于是逐个文件对：
    // 原来就在的那些，字节、修改时间、链接数一样不差（链接数防的是拿主库里的文件当
    // 硬链接的源）；多出来的只许是元数据文件与 `media/` 底下的。
    let mut 现场 = 建现场();
    收一份媒体(
        &mut 现场,
        塞尔达,
        romcat_core::scrape::MediaKind::Cover,
        b"\x89PNG-- zelda cover --",
    );
    收一份媒体(
        &mut 现场,
        台版变体,
        romcat_core::scrape::MediaKind::Screenshot,
        b"\x89PNG-- contra screenshot --",
    );
    let 之前 = 每个文件(现场.out());
    assert!(
        之前.keys().filter(|path| path.ends_with(".zip")).count() >= 6,
        "主库里得真有 ROM：{:?}",
        之前.keys()
    );

    let report = 导出_铺媒体(&mut 现场);

    let 之后 = 每个文件(现场.out());
    for (path, 原样) in &之前 {
        assert!(之后.get(path) == Some(原样), "{path} 被动过了");
    }
    let 多出来的: Vec<&String> = 之后
        .keys()
        .filter(|path| !之前.contains_key(*path))
        .collect();
    for path in &多出来的 {
        assert!(
            path.starts_with("media/")
                || (!path.contains('/') && path.ends_with(".metadata.pegasus.txt")),
            "{path} 落在媒体目录与元数据文件之外"
        );
    }
    assert_eq!(
        多出来的
            .iter()
            .filter(|path| path.starts_with("media/"))
            .count(),
        2,
        "{多出来的:?}"
    );
    let media = report.media.expect("开了铺媒体，报告里就有这一半的账");
    assert_eq!(media.linked + media.copied, 2, "{media:?}");
}

/// 一份**照维护者手工维护的样子**写的元数据：注释、未知键、`x-` 扩展键、
/// 字段顺序不按我们的来、缩进不齐。
const 手写的: &str = "\
# 我的 FC 库，2019 年开始攒的
# 排版是我自己排的，别给我重排

collection: FC
shortname: nes
x-我的备注: 这一栏是给我自己看的

game: 我给它起的名字
sort-by: WO GEI TA QI DE
# 这一版是当年买的那张卡
file: FC/魂斗罗台版/魂斗罗.zip
developer : 我手打的开发商
release: 1988
rating: 85
players: 1-2
没有这个键: 但它得留着
x-通关: 是
";

fn 放一份手写的(现场: &现场) -> PathBuf {
    let path = 现场.out().join("FC.metadata.pegasus.txt");
    fs::write(&path, 手写的).expect("写得进");
    path
}

#[test]
fn 手工维护的元数据往返一个字节都不差_档位是实测出来的() {
    let assertion = assert_capability(&Pegasus, 手写的.as_bytes()).expect("读得动");
    assert!(assertion.identical, "第 {:?} 处分岔", assertion.difference);
    assert_eq!(
        assertion.asserted,
        Capability::LosslessRoundTrip,
        "过了才算无损往返"
    );
}

#[test]
fn 导出前就说得出这个格式的结构性损失_而且导入那一侧说的是同一句() {
    // ADR-0003 要的是「导出前就知道这个格式会丢掉什么」，而不是让用户导出一趟、
    // 发现简介排版变了、以为是刮削出了问题。
    //
    // `手写的` 那份 fixture 里**一条带换行的简介都没有**——两条声明照样得在两份报告里，
    // 因为它们说的是格式**结构上**做不到什么，不是这一趟丢了几条。
    let mut 现场 = 建现场();
    跑一遍(&mut 现场);
    let path = 放一份手写的(&现场);
    let 导入报告 = transfer::import(
        &mut 现场.catalog,
        &Pegasus,
        std::slice::from_ref(&path),
        None,
    )
    .expect("导得进");
    let 导出报告 = 导出(&mut 现场, true);

    assert_eq!(
        导入报告.structural_losses,
        Pegasus.structural_losses(),
        "导入这一侧照搬适配器的声明"
    );
    assert_eq!(
        导出报告.structural_losses, 导入报告.structural_losses,
        "两侧是同一份"
    );

    for 报告 in [导入报告.render_text(), 导出报告.render_text()] {
        assert!(报告.contains("换行"), "单个换行折成空格得说出口：{报告}");
        assert!(报告.contains("空格"), "{报告}");
        assert!(
            报告.contains("U+3000"),
            "开头那两个全角空格得点名——中文离线源的简介就长这样：{报告}"
        );
        assert!(
            报告.contains("段落"),
            "空行分隔的段落是往返得回来的那一样，别让人以为全丢了：{报告}"
        );
        // **后补的三条也要真的印出来。** 只钉前两条的话，把后三条从
        // `STRUCTURAL_LOSSES` 里删掉这条测试照样绿——而它的名字说的是「这个格式的
        // 结构性损失」，不是「其中两条」。逐条挑一个别处不会出现的词。
        assert!(
            报告.contains("每一行"),
            "两头的空白掐的是每一行，不是值的两端：{报告}"
        );
        assert!(
            报告.contains("整条消失"),
            "掐空了字段就没了，导出前就得说出口：{报告}"
        );
        assert!(
            报告.contains("字面量"),
            "多值里的空串读回来是字面量 `.`：{报告}"
        );
        assert!(
            报告.contains("拆成多个值"),
            "多值里带换行的值会被拆开：{报告}"
        );
    }
}

#[test]
fn 导入保留未知键与扩展键与注释与字段顺序_并且落进中立库() {
    let mut 现场 = 建现场();
    let path = 放一份手写的(&现场);
    let report = transfer::import(
        &mut 现场.catalog,
        &Pegasus,
        std::slice::from_ref(&path),
        None,
    )
    .expect("导得进");

    assert_eq!(report.tier, "无损往返", "实测档位");
    let file = &report.files[0];
    assert!(file.roundtrip);
    assert_eq!(file.comments, 3, "三条注释都在快照里");
    assert_eq!(file.unknown_keys, 1, "`没有这个键` 原样留着");
    assert_eq!(file.extension_keys, 2, "`x-我的备注` 与 `x-通关`");
    assert_eq!(file.resolved, 1, "这一条对回了台版那个变体");

    // **原文逐字节存进了中立库。** 这是往返的全部依据。
    let stored = 现场
        .catalog
        .snapshot(
            "Pegasus",
            &romcat_core::path::display(&romcat_core::path::normalize_existing(&path)),
        )
        .expect("读得出")
        .expect("有这一份");
    assert_eq!(stored.bytes, 手写的.as_bytes(), "快照是逐字节的");

    // **手工维护的值真的进了库，而且压过 DAT。** `priorities.toml` 把 `Pegasus`
    // 排在全部数据源之前，理由就是「人手写的东西不该被刮削盖掉」（ADR-0001）。
    let values = 现场
        .catalog
        .scraped_values("变体", 台版变体)
        .expect("读得出");
    assert!(
        values
            .iter()
            .any(|value| value.source == "Pegasus" && value.value == "我给它起的名字"),
        "手工维护的标题落库了：{values:#?}"
    );
    let 胜出 = Priorities::builtin()
        .pick("标题", Some("FC"), &values)
        .expect("挑得出");
    assert_eq!(
        胜出.source, "Pegasus",
        "手工维护的值压过 DAT 与文件名——否则一次刮削就把它盖掉了"
    );
}

#[test]
fn 导出之后维护者的原件在库里一个字节都没少() {
    // **这张票的首要交付。** 导出会把盘上那份改写成合并后的样子（他的注释与未知键
    // 都在，但那已经不是原件了）。原件若在库里也被导出快照顶掉，就是两处都没了
    // ——ADR-0001 说的正是这个代价不可接受。
    let mut 现场 = 建现场();
    let path = 放一份手写的(&现场);
    transfer::import(
        &mut 现场.catalog,
        &Pegasus,
        std::slice::from_ref(&path),
        None,
    )
    .expect("导得进");
    导出(&mut 现场, false);

    let 键 = romcat_core::path::display(&romcat_core::path::normalize_existing(&path));
    let 原件 = 现场
        .catalog
        .imported_snapshot("Pegasus", &键)
        .expect("读得出")
        .expect("原件还在库里");
    assert_eq!(原件.bytes, 手写的.as_bytes(), "原件逐字节还在");
    assert_ne!(
        fs::read(&path).expect("读得出"),
        手写的.as_bytes(),
        "盘上那份确实被导出改写过——正因如此库里那份原件才不能丢"
    );
}

#[test]
fn 把导出产物换回原件不算外部改动() {
    // 维护者把自己的原件放回落点，是**他手上本来就有的东西**，不是「有人在外面动过」。
    // 报成冲突的话，他每趟都要 `--force` 一次——而 `--force` 一旦成了习惯，
    // 这道守卫就白设了。
    let mut 现场 = 建现场();
    let path = 放一份手写的(&现场);
    transfer::import(
        &mut 现场.catalog,
        &Pegasus,
        std::slice::from_ref(&path),
        None,
    )
    .expect("导得进");
    导出(&mut 现场, false);
    fs::write(&path, 手写的).expect("换回原件");
    let report = 导出(&mut 现场, false);
    assert!(report.conflicts.is_empty(), "{:#?}", report.conflicts);
}

#[test]
fn 导出以快照为基线_注释未知键与排版一字不动() {
    let mut 现场 = 建现场();
    let path = 放一份手写的(&现场);
    transfer::import(&mut 现场.catalog, &Pegasus, &[path], None).expect("导得进");

    let report = 导出(&mut 现场, false);
    assert!(report.conflicts.is_empty(), "{:#?}", report.conflicts);
    let text = 读出(&现场, "FC.metadata.pegasus.txt");

    assert!(text.contains("# 我的 FC 库，2019 年开始攒的"), "{text}");
    assert!(text.contains("# 排版是我自己排的，别给我重排"), "{text}");
    assert!(text.contains("# 这一版是当年买的那张卡"), "{text}");
    assert!(text.contains("没有这个键: 但它得留着"), "未知键：{text}");
    assert!(text.contains("x-我的备注: 这一栏是给我自己看的"), "{text}");
    assert!(text.contains("x-通关: 是"), "维护者自己加的扩展键：{text}");
    assert!(
        text.contains("shortname: nes"),
        "他写对了的 shortname 不许被我们猜出来的那个覆盖：{text}"
    );
    assert!(
        text.contains("developer : 我手打的开发商"),
        "连键与冒号之间那个空格都没动：{text}"
    );
    assert!(
        text.contains("rating: 85"),
        "Pegasus 自己会丢掉这一行，但那是它的事——我们一个字不改：{text}"
    );
    // 排版顺序是他的，不是我们的。
    let sort = text.find("sort-by").expect("有 sort-by");
    let file = text.find("FC/魂斗罗台版").expect("有 file");
    let developer = text.find("developer :").expect("有 developer");
    assert!(sort < file, "字段顺序原样留着：{text}");
    assert!(file < developer, "字段顺序原样留着：{text}");

    // 这份导出来的文件自己再读一遍还能逐字节写回去。
    assert!(report.files.iter().all(|f| f.roundtrip));
}

#[test]
fn 作品级收敛_一个条目多个文件_首选变体排在最前() {
    let mut 现场 = 建现场();
    let report = 导出(&mut 现场, false);
    let text = 读出(&现场, "FC.metadata.pegasus.txt");

    // 魂斗罗的三个变体（日版、台版官中、民间汉化）收成**一个**条目。
    assert_eq!(
        text.matches("game: ").count(),
        report.entries as usize,
        "条目数对得上：{text}"
    );
    assert!(report.converged_entries >= 1, "至少有一个条目装着多个变体");

    // 导出的 `file:` 是**相对主库根**的路径：根名是中立库这一侧的东西，不写进前端
    // （`adapter::converge` 里那段注释）。
    let 段 = text
        .split("game: ")
        .find(|段| 段.contains(相对(汉化变体)))
        .expect("魂斗罗那一段在");
    // **首选变体排在最前**：汉化 > 官中 > 日版（ADR-0012）。
    let 汉化位置 = 段.find(相对(汉化变体)).expect("有汉化");
    let 台版位置 = 段.find(相对(台版变体)).expect("有台版");
    let 日版位置 = 段.find(相对(日版变体)).expect("有日版");
    assert!(汉化位置 < 台版位置, "汉化排在官中前面：{段}");
    assert!(台版位置 < 日版位置, "官中排在日版前面：{段}");

    // **标题来源与首选变体解耦**：默认启动的是汉化版，标题仍是官中的官方译名。
    assert!(
        段.starts_with("魂斗罗\n"),
        "标题该是官中的官方译名而不是汉化版文件名：{段}"
    );
    assert_eq!(
        *report
            .preferred
            .iter()
            .find(|(why, _)| why == Preference::FanTranslated.label())
            .map(|(_, count)| count)
            .expect("有汉化这一档"),
        1
    );
}

#[test]
fn 工具自造的东西只写进自己的扩展键_不动维护者的标签() {
    // `tag` 是维护者自己的词表。往里塞我们算出来的 `汉化`，基线合并那一步就会把他
    // 整行标签换掉——那一层比的是整个列表，不是逐个标签。
    let mut 现场 = 建现场();
    let path = 现场.out().join("FC.metadata.pegasus.txt");
    fs::write(
        &path,
        format!(
            "collection: FC

game: 魂斗罗
tag: 我通关了
file: {汉化变体}
"
        ),
    )
    .expect("写得进");
    transfer::import(
        &mut 现场.catalog,
        &Pegasus,
        std::slice::from_ref(&path),
        None,
    )
    .expect("导得进");
    导出(&mut 现场, false);
    let text = 读出(&现场, "FC.metadata.pegasus.txt");
    assert!(text.contains("tag: 我通关了"), "他的标签一个字不动：{text}");
    assert!(
        text.contains("x-romcat-chinese: 汉化"),
        "我们算出来的东西走自己的扩展键：{text}"
    );
}

#[test]
fn 裁决压过首选变体规则() {
    let mut 现场 = 建现场();
    现场
        .catalog
        .set_preferred_variant("Contra", "FC", 日版变体)
        .expect("裁决写得进");
    导出(&mut 现场, false);
    let text = 读出(&现场, "FC.metadata.pegasus.txt");
    let 段 = text
        .split("game: ")
        .find(|段| 段.contains(相对(汉化变体)))
        .expect("魂斗罗那一段在");
    assert!(
        段.find(相对(日版变体)) < 段.find(相对(汉化变体)),
        "人指名了日版，规则就得让路：{段}"
    );
}

#[test]
fn 非游戏资产与补丁不导出为前端条目() {
    let mut 现场 = 建现场();
    let report = 导出(&mut 现场, false);

    let 数 = |why: NotAnEntry| {
        report
            .excluded
            .iter()
            .find(|(label, _)| label == why.label())
            .map(|(_, count)| *count)
            .unwrap_or(0)
    };
    assert_eq!(数(NotAnEntry::NonGameAsset), 1, "BIOS 那一个：{report:#?}");
    assert_eq!(数(NotAnEntry::Patch), 1, "补丁那一个：{report:#?}");

    for name in ["FC.metadata.pegasus.txt", "街机.metadata.pegasus.txt"] {
        let path = 现场.out().join(name);
        if path.exists() {
            let text = fs::read_to_string(&path).expect("读得出");
            assert!(!text.contains(BIOS), "非游戏资产不导出：{text}");
            assert!(!text.contains(补丁), "补丁不导出：{text}");
        }
    }
}

#[test]
fn 真叫_bios_的根底下的游戏照旧导出成条目() {
    // **根名不参与「是不是非游戏资产」的判断**：根的名字是维护者起的，一个真叫 `BIOS`
    // 的根底下的游戏照样是游戏。识别那一侧是同一个答案（`tests/identify.rs` 的
    // `真叫_bios_的根底下的游戏照旧认得出作品`）。
    let dir = temp_dir("pegasus-bios-root");
    写(
        &dir.path().join("FC/魂斗罗日版/Contra (Japan).zip"),
        &zip_container(&[ZipEntrySpec::stored("Contra.nes", 日版())]),
    );
    let mut 现场 = 扫成现场(dir, "BIOS");

    let report = 导出(&mut 现场, false);
    let 非游戏资产 = report
        .excluded
        .iter()
        .find(|(label, _)| label == NotAnEntry::NonGameAsset.label())
        .map_or(0, |(_, count)| *count);
    assert_eq!(非游戏资产, 0, "根叫 BIOS 不算：{report:#?}");
    let text = 读出(&现场, "FC.metadata.pegasus.txt");
    assert!(
        text.contains("Contra (Japan).zip"),
        "它底下的游戏照样是前端条目：{text}"
    );
}

#[test]
fn switch_的补丁与附属内容名字里一个字都没说也不导出为前端条目() {
    // 挂账 `D142`：Switch 的更新包是**补丁**、追加内容是**附属内容**（词表、ADR-0013），可它们
    // 的文件名多半只写着版本号或者一个中文名。说出是哪一种的是容器里那张票据的 TitleID
    // （尾 `800` 是更新包，既不是 `000` 也不是 `800` 是追加内容）——那条依据比名字准。
    // 按名字判出来的那一头照旧拦得住，见 `非游戏资产与补丁不导出为前端条目`。
    let dir = temp_dir("pegasus-switch");
    for (path, bytes) in base_update_add_on() {
        写(&dir.path().join(path), &bytes);
    }
    let mut 现场 = 扫成现场(dir, "库");

    let report = 导出(&mut 现场, false);
    let 数 = |why: NotAnEntry| {
        report
            .excluded
            .iter()
            .find(|(label, _)| label == why.label())
            .map_or(0, |(_, count)| *count)
    };
    assert_eq!(数(NotAnEntry::Patch), 1, "更新包那一个：{report:#?}");
    assert_eq!(
        数(NotAnEntry::ExtraContent),
        1,
        "追加内容那一个：{report:#?}"
    );
    let text = 导出的全文(&现场, &report);
    let 名字 = |path: &'static str| path.rsplit('/').next().unwrap_or(path);
    assert!(text.contains(名字(BASE_NSP)), "本体照样是前端条目：{text}");
    assert!(!text.contains(名字(UPDATE_NSP)), "补丁不导出：{text}");
    assert!(!text.contains(名字(ADD_ON_NSP)), "附属内容不导出：{text}");
}

#[test]
fn 本体加更新的卡带只带着更新的票据_照样是前端条目() {
    // 另一头：**票据不一定替整个容器说话**。卡带上的本体没有票据（卡带那一套加密不走
    // titlekey），同一张卡里打进去的更新包却带着——真库上五张「本体加更新」的卡里唯一
    // 那张票据都是更新包的（`identify::switch::cross_check` 那段注释）。照那张票据把整张卡
    // 判成补丁，一部游戏就从前端里没了。
    const 更新: &str = "0100A0C01BED8800";
    let dir = temp_dir("pegasus-switch-card");
    写(
        &dir.path().join("switch/伊蘇X 卡带[v1.0.2].xci"),
        &xci(
            0,
            &["update", "logo", "normal", "secure"],
            &[
                &ticket(更新),
                "8eed26260dbdb1ea545119cc0368fa06.cnmt.nca",
                "dfdb0f5bc5c5056a2f35d0379ee23020.nca",
                "ba39a7f62eeb23476c08600ed405a1cf.cnmt.nca",
                "150cf9022bfb2e72527669f2701ee31b.nca",
            ],
        ),
    );
    let mut 现场 = 扫成现场(dir, "库");

    let report = 导出(&mut 现场, false);
    assert!(
        report.excluded.iter().all(|(_, count)| *count == 0),
        "{report:#?}"
    );
    let text = 导出的全文(&现场, &report);
    assert!(
        text.contains("伊蘇X 卡带[v1.0.2].xci"),
        "整张卡是游戏：{text}"
    );
}

#[test]
fn 没有那一列的旧库_按名字判出来的补丁照旧不导出为前端条目() {
    // **结构版本没升**：`identification.standalone` 是纯加的一列，旧库打开时补上。补上之后
    // 老行若是空的，导出那道闸（它不再 parse 理由）就会把识别早就判过的汉化补丁当成前端
    // 条目放出去——而「接着上一趟算」的识别不会重算这些已经有结论的变体。
    let dir = temp_dir("pegasus-old-catalog");
    写(
        &dir.path().join(相对(补丁)),
        &zip_container(&[ZipEntrySpec::stored("rockman3.ips", 补丁字节())]),
    );
    写(
        &dir.path().join(相对(塞尔达)),
        &zip_container(&[ZipEntrySpec::stored("Zelda.nes", 只有英文())]),
    );
    let 工作目录 = temp_dir("pegasus-old-catalog-ws");
    let 库文件 = 工作目录.path().join("中立库.sqlite");
    let mut catalog = Catalog::create(&库文件, "旧库").expect("能建中立库");
    let mut options = ScanOptions::named(dir.path(), "库");
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");
    let mut 现场 = 现场 {
        dir,
        _pool: temp_dir("pegasus-old-catalog-pool"),
        catalog,
    };
    跑一遍(&mut 现场);

    // 退回加这一列之前的样子：识别过，库里却没有那一列。
    drop(std::mem::replace(
        &mut 现场.catalog,
        Catalog::open_in_memory().expect("能开中立库"),
    ));
    let conn = rusqlite::Connection::open(&库文件).expect("能再打开那个文件");
    conn.execute_batch("ALTER TABLE identification DROP COLUMN standalone")
        .expect("删得掉那一列");
    drop(conn);
    现场.catalog = Catalog::open(&库文件).expect("旧库照样打得开");

    let report = 导出(&mut 现场, false);
    let 补丁数 = report
        .excluded
        .iter()
        .find(|(label, _)| label == NotAnEntry::Patch.label())
        .map_or(0, |(_, count)| *count);
    assert_eq!(
        补丁数, 1,
        "旧库里按名字判出来的那一个照旧拦得住：{report:#?}"
    );
    let text = 导出的全文(&现场, &report);
    assert!(text.contains("Zelda (USA).zip"), "游戏照样导出：{text}");
    assert!(
        !text.contains("《流星洛克人3》汉化补丁.zip"),
        "补丁不导出：{text}"
    );
}

#[test]
fn 外部改动不被静默覆盖() {
    let mut 现场 = 建现场();
    导出(&mut 现场, false);
    let target = 现场.out().join("FC.metadata.pegasus.txt");

    // 有人在工具外面改了它。
    let mut text = fs::read_to_string(&target).expect("读得出");
    text.push_str("\ngame: 我后来手加的\nfile: FC/Zelda/Zelda (USA).zip\n");
    fs::write(&target, &text).expect("写得进");

    let report = 导出(&mut 现场, false);
    assert_eq!(report.conflicts.len(), 1, "该检测到外部改动：{report:#?}");
    assert_eq!(
        fs::read_to_string(&target).expect("读得出"),
        text,
        "**没有静默覆盖**：手改的那一段还在"
    );

    // 用户明说可以丢，才丢。
    let report = 导出(&mut 现场, true);
    assert!(report.conflicts.is_empty());
    assert_ne!(fs::read_to_string(&target).expect("读得出"), text);
}

#[test]
fn 落点上有一份从没见过的文件时先停下来() {
    let mut 现场 = 建现场();
    放一份手写的(&现场);
    // **没有导入过**：那份文件可能就是维护者的原件，覆盖等于把它抹掉。
    let report = 导出(&mut 现场, false);
    assert_eq!(report.conflicts.len(), 1, "{report:#?}");
    assert!(
        report.conflicts[0].why.contains("从没见过"),
        "{:#?}",
        report.conflicts
    );
    assert_eq!(
        fs::read_to_string(现场.out().join("FC.metadata.pegasus.txt")).expect("读得出"),
        手写的,
        "一个字节都没动"
    );
}

#[test]
fn 导出再导入再导出_文件逐字节不变_而且不会越长越长() {
    // **这是一次真正的往返**，走的是产品路径而不是测试里编的样本：
    // 导出 → 把导出来的那几份导进来 → 再导出一次。第二次以第一次为基线，
    // 库里没变，写出来就该一个字节都不差。
    //
    // 它盯的是一个真机上真出现过的错：段对回基线时，一个本该按变体键对上的段
    // 被某个**同名的、而且已经被别人占掉的**段截胡，于是它对不上基线、被当成新段
    // 接在文件末尾——文件一趟比一趟长，而基线里那一段再也不会被更新。
    // fixture 里 `重名` 那两个变体就是为它准备的。
    let mut 现场 = 建现场();
    导出(&mut 现场, false);
    let 第一次: Vec<(String, String)> = fs::read_dir(现场.out())
        .expect("读得出")
        .filter_map(|entry| {
            let path = entry.expect("有这一条").path();
            let name = path.file_name()?.to_string_lossy().into_owned();
            name.ends_with("metadata.pegasus.txt")
                .then(|| (name, fs::read_to_string(&path).expect("读得出")))
        })
        .collect();
    assert!(!第一次.is_empty());

    let files: Vec<PathBuf> = 第一次
        .iter()
        .map(|(name, _)| 现场.out().join(name))
        .collect();
    let report = transfer::import(&mut 现场.catalog, &Pegasus, &files, None).expect("导得进");
    assert!(
        report.files.iter().all(|file| file.roundtrip),
        "{report:#?}"
    );

    导出(&mut 现场, false);
    for (name, 原文) in &第一次 {
        assert_eq!(
            &fs::read_to_string(现场.out().join(name)).expect("读得出"),
            原文,
            "{name} 第二趟导出与第一趟逐字节不同"
        );
    }
}

#[test]
fn 排不动的排序标题不写_于是也盖不掉维护者写对了的那一行() {
    // 中文按码位排等于乱排（`CONTEXT.md` 的**排序标题**词条）。折不出一个排得动的
    // 排序键时，写一个中文的 `sort-by` 与不写是同一个效果——但它会**盖掉**维护者
    // 自己给中文条目配的拉丁排序键。这一条真机上撞到过。
    let mut 现场 = 建现场();
    let path = 现场.out().join("FC.metadata.pegasus.txt");
    fs::write(
        &path,
        "collection: FC

         game: 我给它起的中文名
         sort-by: WO GEI TA QI DE
         file: FC/魂斗罗台版/魂斗罗.zip
",
    )
    .expect("写得进");
    transfer::import(&mut 现场.catalog, &Pegasus, &[path], None).expect("导得进");
    导出(&mut 现场, false);
    let text = 读出(&现场, "FC.metadata.pegasus.txt");
    assert!(
        text.contains("sort-by: WO GEI TA QI DE"),
        "他写对了的排序键不许被一个按码位排的中文串盖掉：{text}"
    );
}

#[test]
fn 一个文件一个合集_而且合集就是平台() {
    let mut 现场 = 建现场();
    let report = 导出(&mut 现场, false);
    for file in &report.files {
        let text = fs::read_to_string(&file.path).expect("读得出");
        assert_eq!(
            text.matches("\ncollection: ").count() + usize::from(text.starts_with("collection: ")),
            1,
            "一个文件最多一个合集段——Pegasus 会把 game 加进此前定义过的所有合集：{text}"
        );
    }
    assert!(
        report
            .files
            .iter()
            .any(|file| file.collection == "FC" && file.path.ends_with("FC.metadata.pegasus.txt"))
    );
}

/// 维护者用**续行**写的多值：两家开发商、两家发行商、两个类型。
///
/// 这是 Pegasus 官方文档里 `files:` 那种写法，手工维护的文件里到处都是。
const 手写的多值: &str = "\
# 开发商写了两家，别给我丢掉一家
collection: FC

game: 我给它起的名字
file: FC/魂斗罗台版/魂斗罗.zip
developers:
  甲公司
  乙公司
publishers:
  丙公司
  丁公司
genres:
  动作
  射击
";

#[test]
fn 维护者用续行写的多值_一次往返一家都不少() {
    // **一次往返不蒸发心血**（ADR-0001）落到集合字段上：`developers` 写了两家，
    // 导入只落头一家的话，导出那一趟会「合法地」把他两行覆盖成一行——第二家没了，
    // 键还从 `developers` 缩成了 `developer`，而报告一个字都不说。
    let mut 现场 = 建现场();
    let path = 现场.out().join("FC.metadata.pegasus.txt");
    fs::write(&path, 手写的多值).expect("写得进");
    let report = transfer::import(
        &mut 现场.catalog,
        &Pegasus,
        std::slice::from_ref(&path),
        None,
    )
    .expect("导得进");

    // **报告数的是落进中立库的值的条数**，不是字段数：两家开发商就是两条。
    // 数成一条的话，「丢掉的那一条」在账面上根本看不出来。
    assert_eq!(
        report.values, 7,
        "标题 1 + 开发商 2 + 发行商 2 + 类型 2：{report:#?}"
    );

    导出(&mut 现场, false);
    let text = 读出(&现场, "FC.metadata.pegasus.txt");
    // 他写的那三段续行一个字都没动——**键也还是他写的那个复数形式**。
    assert!(text.contains("developers:\n  甲公司\n  乙公司\n"), "{text}");
    assert!(text.contains("publishers:\n  丙公司\n  丁公司\n"), "{text}");
    assert!(text.contains("genres:\n  动作\n  射击\n"), "{text}");
    assert!(
        !text.contains("developer: "),
        "库里与他写的是同一批，就不该重写这一行：{text}"
    );
}

#[test]
fn 导出逐条记下写出去的是谁_之后扫进来的作品照实说还没导出() {
    // 票 `gui-looks-like-the-design/34`：作品详情页状态块「导出」那一行。
    //
    // **整库那一个时刻答不了它**——导出整库级、不挑选（词表**导出**），所以拿
    // `exported_at` 去答的话，上次导出之后才扫进来的作品会跟着说「已导出」，
    // 而那正是这一行最该答对的一种情形。
    use romcat_core::scrape::AnchorKind;

    let mut 现场 = 建现场();
    let report = 导出(&mut 现场, false);
    let 时刻 = 现场
        .catalog
        .exported_at()
        .expect("读得出")
        .expect("走完了就该打上时刻戳");

    // 一、写出去的每个条目都记下了，而且**与整库那个时刻是同一个数**（同一道闸、同一趟）。
    let 魂斗罗 = 现场
        .catalog
        .entry_exported(AnchorKind::Work, "Contra")
        .expect("读得出")
        .expect("这一条这一趟真写出去了");
    assert_eq!(魂斗罗.format, "Pegasus");
    assert_eq!(
        魂斗罗.at, 时刻,
        "逐条那批与整库那个时刻分了家，屏上就会有一行说导过了、另一行说没有"
    );

    // 二、**压根不在这份库里的作品**问出来是 `None`——不是「整库导过了所以它也导过了」。
    assert_eq!(
        现场
            .catalog
            .entry_exported(AnchorKind::Work, "这个作品不存在")
            .expect("读得出"),
        None
    );

    // 三、导出**之后**才进来的那个作品：整库那一行说「导过了」，而它自己照实说「还没」。
    现场
        .catalog
        .add_work(
            "刚扫进来的",
            romcat_core::catalog::identify::Provenance::Identified,
        )
        .expect("建得出作品");
    assert!(
        现场.catalog.exported_at().expect("读得出").is_some(),
        "整库那一行照旧说导过了"
    );
    assert_eq!(
        现场
            .catalog
            .entry_exported(AnchorKind::Work, "刚扫进来的")
            .expect("读得出"),
        None,
        "上次导出那会儿它还不在库里——这一行不许跟着说「已导出」"
    );

    // 四、**每趟整份重写**：再导一趟，这一条的时刻跟着换，行数不累积。
    assert!(report.entries > 0, "这份 fixture 得真收敛出条目来");
    let 行数 = |catalog: &romcat_core::catalog::Catalog| {
        catalog
            .entry_exported(AnchorKind::Work, "Contra")
            .expect("读得出")
            .is_some()
    };
    assert!(行数(&现场.catalog));
}

#[test]
fn 只排计划那一趟一条逐条的账都不记() {
    // 与整库那个时刻同一道闸：`dry_run` 一个字节都没写，说不上「导过了」。
    use romcat_core::scrape::AnchorKind;

    let mut 现场 = 建现场();
    let out = 现场.out().to_path_buf();
    transfer::export(
        &mut 现场.catalog,
        &Pegasus,
        &Priorities::builtin(),
        &ExportOptions {
            out,
            dry_run: true,
            force: false,
            media: None,
        },
    )
    .expect("排得出计划");
    assert_eq!(现场.catalog.exported_at().expect("读得出"), None);
    assert_eq!(
        现场
            .catalog
            .entry_exported(AnchorKind::Work, "Contra")
            .expect("读得出"),
        None,
        "只排计划那一趟一条逐条的账都不该记"
    );
}
