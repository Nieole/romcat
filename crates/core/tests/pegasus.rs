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
use romcat_core::adapter::{Capability, assert_capability};
use romcat_core::catalog::Catalog;
use romcat_core::dat::Convention;
use romcat_core::dat::logiqx::{DatHeader, GameRecord, RomRecord};
use romcat_core::dat::repo::{DatMeta, DatRepo, Unit};
use romcat_core::fs::RealFs;
use romcat_core::identify;
use romcat_core::identify::fuzzy;
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::scrape::{self, Priorities};
use romcat_core::testing::container::{ZipEntrySpec, crc32, zip_container};
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

const 日版变体: &str = "FC/魂斗罗日版/Contra (Japan).zip";
const 台版变体: &str = "FC/魂斗罗台版/魂斗罗.zip";
const 汉化变体: &str = "FC/魂斗罗汉化/魂斗罗 中文版[dwt_so 汉化].zip";
const 塞尔达: &str = "FC/Zelda/Zelda (USA).zip";
const BIOS: &str = "街机/FBA-ROMS/BIOS/neogeo.zip";
const 补丁: &str = "FC/《流星洛克人3》汉化补丁.zip";
/// 两个**没有作品链接**、而且显示标题会撞在一起的变体。
///
/// 它们撞名是有意的：真机上一趟「导出 → 导入 → 再导出」里，光 FC 一个平台就有近千个
/// 条目的标题与别的条目撞名（同一部作品的不同 HACK 版、不同合集包里的同一个名字）。
/// 段对回基线时若让「标题一字不差」这条软判据抢在「变体键相同」前面，撞名的那些就会
/// 互相截胡。
const 重名甲: &str = "FC/重名/甲/超级玛丽.zip";
const 重名乙: &str = "FC/重名/乙/超级玛丽.zip";

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
        &root.join(日版变体),
        &zip_container(&[ZipEntrySpec::stored("Contra.nes", 日版())]),
    );
    写(
        &root.join(台版变体),
        &zip_container(&[ZipEntrySpec::stored("Contra.nes", 台版())]),
    );
    写(
        &root.join(汉化变体),
        &zip_container(&[ZipEntrySpec::stored("魂斗罗.nes", 汉化版())]),
    );
    写(
        &root.join(塞尔达),
        &zip_container(&[ZipEntrySpec::stored("Zelda.nes", 只有英文())]),
    );
    // **非游戏资产**：模拟器要它，它本身不是游戏（ADR-0010）。
    写(
        &root.join(BIOS),
        &zip_container(&[ZipEntrySpec::stored("neogeo.rom", 街机bios())]),
    );
    // **补丁**：不可运行，识别那一趟会判它跳过。
    写(
        &root.join(补丁),
        &zip_container(&[ZipEntrySpec::stored("rockman3.ips", 补丁字节())]),
    );
    // 两个 DAT 认不出、文件名却一模一样的变体：显示标题会撞在一起。
    for (key, 字节) in [(重名甲, 0x11u8), (重名乙, 0x22u8)] {
        写(
            &root.join(key),
            &zip_container(&[ZipEntrySpec::stored("mario.nes", vec![字节; 2_048])]),
        );
    }

    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    catalog
        .set_library_root(&romcat_core::path::display(root))
        .expect("写得下主库根");
    let mut options = ScanOptions::new(root);
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &CancelToken::new()).expect("扫得动");

    let mut 场 = 现场 {
        dir,
        _pool: temp_dir("pegasus-pool"),
        catalog,
    };
    跑一遍(&mut 场);
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
    let repo = 建_dat();
    identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &identify::Ammo {
            repo: &repo,
            verdicts: &verdict::Index::empty(),
            naming: &fuzzy::Naming::off(),
        },
        &identify::Options::new(现场.dir.path()),
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别不该失败");
    let mut options = scrape::Options::new(现场.dir.path(), 现场._pool.path());
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
        },
    )
    .expect("刮削不该失败");
    title::run(&mut 现场.catalog, &Priorities::builtin()).expect("折得出标题");
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
        },
    )
    .expect("导得出来")
}

fn 读出(现场: &现场, name: &str) -> String {
    fs::read_to_string(现场.out().join(name)).expect("读得出导出来的文件")
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

    let 段 = text
        .split("game: ")
        .find(|段| 段.contains(汉化变体))
        .expect("魂斗罗那一段在");
    // **首选变体排在最前**：汉化 > 官中 > 日版（ADR-0012）。
    let 汉化位置 = 段.find(汉化变体).expect("有汉化");
    let 台版位置 = 段.find(台版变体).expect("有台版");
    let 日版位置 = 段.find(日版变体).expect("有日版");
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
        .find(|段| 段.contains(汉化变体))
        .expect("魂斗罗那一段在");
    assert!(
        段.find(日版变体) < 段.find(汉化变体),
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
