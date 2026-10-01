//! **沉淀库收下人手写的东西**（`verdict-store-and-sync` 那一组）：做一件人手写的事 →
//! 删掉中立库重扫 → 那件事还在。
//!
//! 照「改名之后路径锚一个字节没动、沉淀库里的裁决照旧对得上」那条（`library_name.rs`）的
//! 写法：中立库落在真实的工作目录里，**删掉它**是连 SQLite 的附件一起删文件，重开走的是
//! 开现场那一条正门（`Site::open_file`），扫描与识别都是真跑。
//!
//! 主库摆的是同一部作品在 FC 上的两个变体：日版与汉化版，两份都**穿得透**的 zip，
//! 裁决钉在内容锚上——于是删库重扫、挪了目录之后识别照样认得出它们是《魂斗罗》，
//! 这里要看的只是**人定的那几样**还在不在、对不对得上。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use romcat_core::catalog::{Catalog, Confidence, Roots, TitleRow, VariantDetail};
use romcat_core::dat::chinese::ChineseMark;
use romcat_core::dat::repo::DatRepo;
use romcat_core::fs::RealFs;
use romcat_core::identify::{self, Options, fuzzy};
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::scrape::priority::{Said, VERDICT, entry_fields};
use romcat_core::scrape::{AnchorKind, Field, Priorities};
use romcat_core::site::Site;
use romcat_core::task::Handle;
use romcat_core::testing::container::{ZipEntrySpec, zip_container};
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::title::{Language, TitleKind};
use romcat_core::verdict::{self, Anchor, Decision, Facts, Verdict};
use romcat_core::workspace::{self, CatalogState, Slug};

/// 主库那一组根里唯一的一个。中立库的键就是「根名 + 相对那个根的路径」。
const 根名: &str = "库";
const 作品: &str = "魂斗罗";
const 平台: &str = "FC";
const 日版: &str = "库/FC/魂斗罗 (Japan).zip";
const 汉化: &str = "库/FC/魂斗罗 汉化.zip";

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

fn 卡带(fill: u8) -> Vec<u8> {
    let mut data = vec![0u8; 16];
    data[..4].copy_from_slice(b"NES\x1A");
    data.extend(std::iter::repeat_n(fill, 40_960));
    data
}

/// 把一条**中立库的键**折回盘上那条相对主库根的路径：剥掉第一段根名。
fn 盘上(root: &Path, key: &str) -> PathBuf {
    root.join(key.strip_prefix("库/").expect("键以根名打头"))
}

/// 摆好主库的字节：日版与汉化版各一份穿得透的 zip。
fn 摆好主库() -> TempDir {
    let dir = temp_dir("verdict-store");
    写(
        &盘上(dir.path(), 日版),
        &zip_container(&[ZipEntrySpec::stored("Contra (Japan).nes", 卡带(0xA1))]),
    );
    写(
        &盘上(dir.path(), 汉化),
        &zip_container(&[ZipEntrySpec::stored("rom.nes", 卡带(0xB1))]),
    );
    dir
}

/// 在工作目录里建一份落在磁盘上的中立库（已经在就只开），连沉淀库一起开成现场。
fn 开现场(工作目录: &Path) -> (PathBuf, Site) {
    let 库文件 = workspace::catalog_path(工作目录, Slug::Named("主库"));
    if !库文件.exists() {
        drop(Catalog::create(&库文件, "主库").expect("建得出中立库"));
    }
    let site = Site::open_file(工作目录, &库文件, None).expect("开得出现场");
    (库文件, site)
}

/// **删掉中立库**：连 SQLite 的附件（`-wal` / `-shm`）一起，一个字节都不留给下一份。
fn 删库(库文件: &Path) {
    for 后缀 in ["", "-wal", "-shm"] {
        let mut 名 = 库文件.as_os_str().to_owned();
        名.push(后缀);
        match fs::remove_file(PathBuf::from(名)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("删不掉中立库：{error}"),
        }
    }
}

fn 扫(site: &mut Site, root: &Path) {
    let mut options = ScanOptions::named(root, 根名);
    options.jobs = Jobs::Fixed(2);
    options.shaping_overrides = site.shaping_overrides().expect("读得出沉淀库");
    scan::scan(&RealFs::new(), &mut site.catalog, &options, &Handle::new()).expect("扫得动");
}

/// 跑一趟识别：沉淀库里的裁决照内容锚重放，作品与发行版就是这么回来的。
fn 跑识别(site: &mut Site, root: &Path) {
    let index = verdict::Index::load(&site.store, &site.library_identity).expect("读得出沉淀库");
    let repo = DatRepo::in_memory().expect("开得出 DAT 库");
    let mut options = Options::new(Roots::single(根名, root));
    options.read_library = false;
    identify::run(
        &RealFs::new(),
        &mut site.catalog,
        &identify::Ammo {
            repo: &repo,
            verdicts: &index,
            naming: &fuzzy::Naming::off(),
            guessing: &identify::model::Guessing::off(),
            titledb: None,
        },
        &options,
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别不该失败");
}

/// 把两个变体都裁成 FC 上的《魂斗罗》，钉在**内容锚**上；汉化版带着汉化记号，
/// 于是首选变体那条规则（汉化 > 官中 > 日版 > 其他）挑的是它。
fn 裁成魂斗罗(site: &mut Site) {
    for (key, chinese) in [(日版, None), (汉化, Some(ChineseMark::FanTranslated))] {
        let variant = site
            .catalog
            .variant(key)
            .expect("读得出")
            .unwrap_or_else(|| panic!("{key} 该在库里"));
        let print = identify::content_print(&site.catalog, &variant)
            .expect("算得出")
            .expect("zip 的 CRC 零解压就有");
        site.store
            .put(&Verdict::now(
                Anchor::Content {
                    crc32: print.crc32,
                    size: print.size,
                    sha1: None,
                },
                Decision::Release(Facts {
                    work: 作品.to_string(),
                    platform: Some(平台.to_string()),
                    chinese,
                    ..Facts::default()
                }),
            ))
            .expect("落得进沉淀库");
    }
}

/// 扫一遍、裁一遍、识别一遍：一份认得出《魂斗罗》两个变体的现场。
fn 认好的现场(root: &Path, 工作目录: &Path) -> (PathBuf, Site) {
    let (库文件, mut site) = 开现场(工作目录);
    扫(&mut site, root);
    裁成魂斗罗(&mut site);
    跑识别(&mut site, root);
    (库文件, site)
}

/// 删掉中立库、从零扫一遍再识别一遍——结构版本一变，维护者按提示做的正是这件事。
fn 删库重扫(库文件: &Path, root: &Path, 工作目录: &Path) -> Site {
    删库(库文件);
    let (_, mut site) = 开现场(工作目录);
    扫(&mut site, root);
    跑识别(&mut site, root);
    site
}

/// 主库里每个文件的相对路径与全部字节。**主库只读**（ADR-0004）：前后两份得一个字节不差。
fn 主库的字节(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut todo = vec![root.to_path_buf()];
    while let Some(dir) = todo.pop() {
        for entry in fs::read_dir(&dir).expect("列得开") {
            let path = entry.expect("读得出").path();
            if path.is_dir() {
                todo.push(path);
            } else {
                let rel = path.strip_prefix(root).expect("在主库里").to_path_buf();
                out.insert(rel, fs::read(&path).expect("读得出"));
            }
        }
    }
    out
}

fn 详情(site: &Site, key: &str) -> VariantDetail {
    site.catalog
        .variant_detail(key, &Priorities::builtin(), None)
        .expect("读得动")
        .unwrap_or_else(|| panic!("{key} 该在库里"))
}

#[test]
fn 定过的首选变体_删掉中立库重扫之后还是它() {
    let 主库 = 摆好主库();
    let 原样 = 主库的字节(主库.path());
    let 工作目录 = temp_dir("verdict-store-ws");
    let (库文件, mut site) = 认好的现场(主库.path(), 工作目录.path());
    assert_eq!(
        详情(&site, 日版).preferred_now(),
        Some(汉化),
        "前提：规则挑的是汉化版",
    );

    // 人推翻规则：这部作品在 FC 上默认启动日版。
    site.set_preferred_variant(作品, 平台, 日版)
        .expect("定得下首选变体");
    assert_eq!(
        详情(&site, 汉化).preferred_now(),
        Some(日版),
        "当场就生效了"
    );

    drop(site);
    let site = 删库重扫(&库文件, 主库.path(), 工作目录.path());
    let 那一个 = 详情(&site, 汉化);
    assert_eq!(
        那一个.preferred_now(),
        Some(日版),
        "删库重扫之后，前端默认启动的还该是人定的那一个",
    );
    assert_eq!(
        那一个.preferred.as_deref(),
        Some(日版),
        "而且说得出它是裁决指定的，不是规则碰巧挑中",
    );
    assert!(
        主库的字节(主库.path()) == 原样,
        "定首选变体、删库重扫一整趟下来，主库该一个字节都没动（ADR-0004）",
    );
}

/// 人在详情面板上亲手给《魂斗罗》加的那一条叫法（「加进集合」那个按钮交进来的样子）。
fn 亲手加的叫法() -> TitleRow {
    TitleRow {
        work: 作品.to_string(),
        value: "魂斗罗 我起的名".to_string(),
        language: Language::Chinese,
        kind: TitleKind::Alias,
        source: VERDICT.to_string(),
        region: None,
        variant_key: Some(汉化.to_string()),
        confidence: Confidence::High,
        seam: None,
        evidence: "亲手写的".to_string(),
        seen: 1,
    }
}

/// 这部作品的标题集合里眼下有没有那一串字，而且是**裁决**来源的。
fn 有这条叫法(site: &Site, value: &str) -> bool {
    site.catalog
        .titles_of(作品)
        .expect("读得出")
        .iter()
        .any(|row| row.value == value && row.is_verdict())
}

#[test]
fn 亲手加的叫法_删掉中立库重扫之后还在() {
    let 主库 = 摆好主库();
    let 工作目录 = temp_dir("verdict-store-ws");
    let (库文件, mut site) = 认好的现场(主库.path(), 工作目录.path());

    site.add_own_titles(&[亲手加的叫法()])
        .expect("加得进标题集合");
    assert!(有这条叫法(&site, "魂斗罗 我起的名"), "当场就在集合里");

    drop(site);
    let site = 删库重扫(&库文件, 主库.path(), 工作目录.path());
    assert!(
        有这条叫法(&site, "魂斗罗 我起的名"),
        "删库重扫之后，亲手加的叫法该还在：{:?}",
        site.catalog.titles_of(作品).expect("读得出"),
    );
}

#[test]
fn 旧中立库里的首选变体与叫法_开现场时救进沉淀库一次_撤掉的不再回来() {
    // 这张票之前，两样都只住在中立库里。新程序开现场时把中立库那两份当成沉淀库的**投影**
    // 照沉淀库重建——**不先救一次，旧库里人定过的就在头一次开现场时被抹掉了**。
    let 工作目录 = temp_dir("verdict-store-carry");
    let 库文件 = workspace::catalog_path(工作目录.path(), Slug::Named("主库"));
    {
        // 旧版程序在这份中立库里记过的样子：一条首选变体、一条亲手加的叫法。
        let mut catalog = Catalog::create(&库文件, "主库").expect("建得出中立库");
        catalog
            .set_preferred_variant(作品, 平台, 日版)
            .expect("写得进旧库");
        catalog.put_titles(&[亲手加的叫法()]).expect("写得进旧库");
    }

    let (_, mut site) = 开现场(工作目录.path());
    let 标识 = site.library_identity.clone();
    assert_eq!(
        site.store
            .preferred_variants(&标识)
            .expect("读得出")
            .get(&(作品.to_string(), 平台.to_string()))
            .map(String::as_str),
        Some(日版),
        "开现场那一下，首选变体就搬进沉淀库了",
    );
    assert_eq!(
        site.store
            .own_titles(&标识)
            .expect("读得出")
            .iter()
            .map(|row| row.value.as_str())
            .collect::<Vec<_>>(),
        ["魂斗罗 我起的名"],
        "开现场那一下，亲手加的叫法就搬进沉淀库了",
    );
    assert!(有这条叫法(&site, "魂斗罗 我起的名"), "中立库里那一条也还在");
    assert_eq!(
        site.catalog
            .preferred_variant(作品, 平台)
            .expect("读得出")
            .as_deref(),
        Some(日版),
    );

    // 人撤掉这两样：下次开现场，旧库里原来那两行**不许**又被搬回来。
    assert!(site.clear_preferred_variant(作品, 平台).expect("撤得掉"));
    assert!(site.remove_own_title(&亲手加的叫法()).expect("撤得掉"));
    drop(site);
    let (_, site) = 开现场(工作目录.path());
    assert!(
        site.store
            .preferred_variants(&标识)
            .expect("读得出")
            .is_empty(),
        "撤掉的首选变体被旧库带回来了",
    );
    assert!(
        site.store.own_titles(&标识).expect("读得出").is_empty(),
        "撤掉的叫法被旧库带回来了",
    );
    assert_eq!(
        site.catalog.preferred_variant(作品, 平台).expect("读得出"),
        None
    );
    assert!(!有这条叫法(&site, "魂斗罗 我起的名"));
}

#[test]
fn 结构版本对不上的旧库_列出来那一下就救进沉淀库_照提示删库重扫之后还在() {
    // 删库那句提示说首选变体与亲手加的叫法「一条不丢」，而人读到它的时候，那份库**开不进去**
    // ——开现场时搬一次那条路走不到。列出来那一下就得先救出来，照提示删掉才真的不丢
    // （照人工纠正那一次的先例，挂单 `Q726`）。
    let 主库 = 摆好主库();
    let 工作目录 = temp_dir("verdict-store-stranded");
    let 库文件 = workspace::catalog_path(工作目录.path(), Slug::Named("主库"));
    {
        let mut catalog = Catalog::create(&库文件, "主库").expect("建得出中立库");
        catalog
            .set_preferred_variant(作品, 平台, 日版)
            .expect("写得进旧库");
        catalog.put_titles(&[亲手加的叫法()]).expect("写得进旧库");
    }
    // 一份旧版程序留下的库：两样还只住在它里面，结构版本也是旧的。
    rusqlite::Connection::open(&库文件)
        .expect("开得出旧库")
        .execute_batch("UPDATE meta SET value = '6' WHERE key = 'schema_version';")
        .expect("改得成一份旧版本的库");

    // 开场那一屏：它列出来了，说的是「删掉它重扫」。
    let 列出来的 = workspace::catalogs(工作目录.path());
    let 那一份 = 列出来的
        .entries()
        .expect("列得开")
        .iter()
        .find(|一份| 一份.path == 库文件)
        .expect("版本对不上的那一份照样列出来了");
    let CatalogState::SchemaMismatch { said: 说的, .. } = &那一份.state else {
        panic!("该是结构版本对不上：{那一份:?}");
    };
    assert!(说的.contains("删掉它重扫一遍"), "{说的}");

    // 照提示删库重扫。
    删库(&库文件);
    let (_, mut site) = 开现场(工作目录.path());
    扫(&mut site, 主库.path());
    裁成魂斗罗(&mut site);
    跑识别(&mut site, 主库.path());
    assert_eq!(
        详情(&site, 汉化).preferred_now(),
        Some(日版),
        "照提示删库重扫之后，首选变体还是旧库里定的那一个",
    );
    assert!(
        有这条叫法(&site, "魂斗罗 我起的名"),
        "照提示删库重扫之后，旧库里亲手加的叫法还在",
    );
}

#[test]
fn 挪了目录之后首选变体如实对不上_不挂到挪过去的那一个上() {
    // 首选变体钉的是一个**位置**（路径锚那一族）：熬得过删库重扫，熬不过改名与挪目录。
    // 挪了之后要**如实**：不说「裁决指定的」，也不猜着把那条裁决挪到挪过去的那一份上；
    // 沉淀库里那一条原样留着，挪回来就又对上了。
    let 主库 = 摆好主库();
    let 工作目录 = temp_dir("verdict-store-moved");
    let (库文件, mut site) = 认好的现场(主库.path(), 工作目录.path());
    site.set_preferred_variant(作品, 平台, 日版)
        .expect("定得下首选变体");
    site.add_own_titles(&[亲手加的叫法()])
        .expect("加得进标题集合");

    // 日版挪进一个子目录。内容一个字节没动，所以内容锚那条裁决照样认得出它是《魂斗罗》。
    const 挪过去的: &str = "库/FC/日版/魂斗罗 (Japan).zip";
    fs::create_dir_all(盘上(主库.path(), "库/FC/日版")).expect("建得出目录");
    fs::rename(盘上(主库.path(), 日版), 盘上(主库.path(), 挪过去的)).expect("挪得动");

    drop(site);
    let site = 删库重扫(&库文件, 主库.path(), 工作目录.path());
    let 那一个 = 详情(&site, 挪过去的);
    assert_eq!(
        那一个.work.as_deref(),
        Some(作品),
        "前提：挪过去的那一份照样认得出"
    );
    assert_eq!(
        那一个.preferred_now(),
        Some(汉化),
        "对不上的裁决不算数，首选照规则算——不挂到挪过去的那一份上",
    );
    assert_eq!(那一个.preferred, None, "对不上时不许说它是裁决指定的");
    assert_eq!(
        那一个.preferred_unmatched(),
        Some(日版),
        "而且说得出：人定过的那一个（在这个位置上）不在了",
    );
    assert_eq!(
        site.store
            .preferred_variants(&site.library_identity)
            .expect("读得出")
            .get(&(作品.to_string(), 平台.to_string()))
            .map(String::as_str),
        Some(日版),
        "沉淀库里那一条原样留着，不改、不删、不猜着往前挪",
    );
    // 亲手加的叫法挂在**作品名**上（中立库那张表的键），挪目录不改作品名：它照旧在《魂斗罗》那里。
    assert!(有这条叫法(&site, "魂斗罗 我起的名"));
}

#[test]
fn 导出沉淀库不带首选变体与亲手加的叫法() {
    // 两样与人工纠正同族：键是本机这一份主库里的键，给别人一条也用不上，还顺带把自己的目录
    // 结构交出去了（ADR-0001 修订、`--include-path` 也不带：挂单 `Q722` 的裁决）。
    // 库里那两条内容锚的裁决作对照：它们照旧在导出里。
    let 主库 = 摆好主库();
    let 工作目录 = temp_dir("verdict-store-export");
    let (_, mut site) = 认好的现场(主库.path(), 工作目录.path());
    site.set_preferred_variant(作品, 平台, 日版)
        .expect("定得下首选变体");
    site.add_own_titles(&[亲手加的叫法()])
        .expect("加得进标题集合");

    for 带路径锚 in [false, true] {
        let 导出 = site.store.export(带路径锚).expect("导得出");
        assert_eq!(导出.verdicts.len(), 2, "对照：两条内容锚的裁决在导出里");
        let 原文 = serde_json::to_string(&导出).expect("折得成 JSON");
        for 不该有的 in [日版, "魂斗罗 我起的名"] {
            assert!(
                !原文.contains(不该有的),
                "带路径锚={带路径锚}：导出里出现了「{不该有的}」：{原文}",
            );
        }

        // 别人导入之后，他那份沉淀库里一样都没有。
        let mut 别人的 = romcat_core::verdict::Store::in_memory().expect("开得出沉淀库");
        别人的.import(&原文).expect("导得进");
        assert!(
            别人的
                .preferred_variants(&site.library_identity)
                .expect("读得出")
                .is_empty()
        );
        assert!(
            别人的
                .own_titles(&site.library_identity)
                .expect("读得出")
                .is_empty()
        );
    }
}

/// 详情页上改的那一格的回执写的是什么（界面「保存」那一下记的**依据**）。
const 手写的: &str = "作品详情页上手动修改";

/// 核心库说《魂斗罗》这个条目（FC，汉化版当头）上这个字段眼下写出去的是什么——
/// 作品详情页元数据那一面与导出读的是同一处（`scrape::priority::entry_fields`）。
fn 写出去的(site: &Site, field: Field) -> Option<Said> {
    entry_fields(
        &site.catalog,
        AnchorKind::Work,
        作品,
        平台,
        Some(汉化),
        &Priorities::builtin(),
    )
    .expect("读得出")
    .into_iter()
    .find(|one| one.field == field)
    .expect("这一格在")
    .shown
}

/// 裁决说的那一句。
fn 裁决说(value: &str) -> Option<Said> {
    Some(Said {
        source: Some(VERDICT.to_string()),
        values: vec![value.to_string()],
    })
}

#[test]
fn 详情页改过的字段_删掉中立库重扫之后还在() {
    // 作品详情页「编辑 → 保存」、「使用这个值」落下的是一格**字段修改**：来源记裁决，
    // 优先于所有数据源。改在作品上的（年份）与改在变体上的（汉化组）各一格。
    let 主库 = 摆好主库();
    let 原样 = 主库的字节(主库.path());
    let 工作目录 = temp_dir("verdict-store-fields");
    let (库文件, mut site) = 认好的现场(主库.path(), 工作目录.path());
    assert_eq!(
        写出去的(&site, Field::Year),
        None,
        "前提：年份这一格没人说过"
    );

    site.put_verdict_value(AnchorKind::Work, 作品, Field::Year, "1987", 手写的)
        .expect("改得动年份");
    site.put_verdict_value(
        AnchorKind::Variant,
        汉化,
        Field::TranslationGroup,
        "我的汉化组",
        手写的,
    )
    .expect("改得动汉化组");
    assert_eq!(写出去的(&site, Field::Year), 裁决说("1987"), "当场就生效了");

    drop(site);
    let site = 删库重扫(&库文件, 主库.path(), 工作目录.path());
    assert_eq!(
        写出去的(&site, Field::Year),
        裁决说("1987"),
        "删库重扫之后，改在作品上的那一格该还在",
    );
    assert_eq!(
        写出去的(&site, Field::TranslationGroup),
        裁决说("我的汉化组"),
        "删库重扫之后，改在变体上的那一格该还在",
    );
    assert!(
        详情(&site, 汉化)
            .values
            .iter()
            .any(|one| one.is_verdict() && one.value.evidence == 手写的),
        "依据也原样回来了：{:?}",
        详情(&site, 汉化).values,
    );
    assert!(
        主库的字节(主库.path()) == 原样,
        "改字段、删库重扫一整趟下来，主库该一个字节都没动（ADR-0004）",
    );
}

#[test]
fn 删掉亲手加的叫法_删库重扫之后它也不回来() {
    // 详情面板上那个「删」对亲手加的叫法也管用（`title::suppress`）。它住沉淀库之后，
    // 删就得删沉淀库里那一条——只删中立库那份投影的话，下次开现场它就被重建回来了。
    let 主库 = 摆好主库();
    let 工作目录 = temp_dir("verdict-store-remove");
    let (库文件, mut site) = 认好的现场(主库.path(), 工作目录.path());
    site.add_own_titles(&[亲手加的叫法()])
        .expect("加得进标题集合");
    let 那一条 = site
        .catalog
        .titles_of(作品)
        .expect("读得出")
        .into_iter()
        .find(|row| row.value == "魂斗罗 我起的名")
        .expect("在集合里");

    let 删了 = romcat_core::title::suppress(
        &mut site.catalog,
        &mut site.store,
        &site.library_identity,
        &那一条,
    )
    .expect("删得掉");
    assert!(
        删了.removed && !删了.recorded,
        "裁决来源的删掉就是删掉，不记压制"
    );
    assert!(!有这条叫法(&site, "魂斗罗 我起的名"), "当场就不在了");

    drop(site);
    let site = 删库重扫(&库文件, 主库.path(), 工作目录.path());
    assert!(
        !有这条叫法(&site, "魂斗罗 我起的名"),
        "删掉的亲手加的叫法被沉淀库带回来了",
    );
}

#[test]
fn 撤掉的字段修改_删库重扫之后它也不回来() {
    // 元数据那一面「撤销手动修改」、编辑时把框清空保存，撤的都是那一格字段修改。它住沉淀库之后，
    // 撤就得撤沉淀库里那一条——只撤中立库那份投影的话，下次开现场它就被重建回来了。
    let 主库 = 摆好主库();
    let 工作目录 = temp_dir("verdict-store-fields-clear");
    let (库文件, mut site) = 认好的现场(主库.path(), 工作目录.path());
    site.put_verdict_value(AnchorKind::Work, 作品, Field::Year, "1987", 手写的)
        .expect("改得动年份");

    assert!(
        site.clear_verdict_value(AnchorKind::Work, 作品, Field::Year)
            .expect("撤得动"),
        "撤的时候说得出原来有这一格",
    );
    assert_eq!(写出去的(&site, Field::Year), None, "当场就撤掉了");
    assert!(
        !site
            .clear_verdict_value(AnchorKind::Work, 作品, Field::Year)
            .expect("撤得动"),
        "再撤一次：两份库里都没有了",
    );

    drop(site);
    let site = 删库重扫(&库文件, 主库.path(), 工作目录.path());
    assert_eq!(
        写出去的(&site, Field::Year),
        None,
        "撤掉的字段修改被沉淀库带回来了",
    );
}

#[test]
fn 旧中立库里的字段修改_开现场时救进沉淀库一次_撤掉的不再回来() {
    // 这张票之前，字段修改只住在中立库里。新程序开现场时把中立库那几行当成沉淀库的**投影**
    // 照沉淀库重建——**不先救一次，旧库里人改过的就在头一次开现场时被抹掉了**。
    let 工作目录 = temp_dir("verdict-store-fields-carry");
    let 库文件 = workspace::catalog_path(工作目录.path(), Slug::Named("主库"));
    {
        // 旧版程序在这份中立库里记过的样子：作品上一格、变体上一格。
        let mut catalog = Catalog::create(&库文件, "主库").expect("建得出中立库");
        catalog
            .put_verdict_value(AnchorKind::Work, 作品, Field::Year, "1987", 手写的)
            .expect("写得进旧库");
        catalog
            .put_verdict_value(
                AnchorKind::Variant,
                汉化,
                Field::TranslationGroup,
                "我的汉化组",
                手写的,
            )
            .expect("写得进旧库");
    }
    let 旧库里的 = Catalog::open(&库文件)
        .expect("开得出")
        .verdict_values()
        .expect("读得出");
    assert_eq!(旧库里的.len(), 2, "前提：旧库里两格");

    let (_, mut site) = 开现场(工作目录.path());
    let 标识 = site.library_identity.clone();
    assert_eq!(
        site.store.verdict_values(&标识).expect("读得出"),
        旧库里的,
        "开现场那一下，两格字段修改就原样搬进沉淀库了（依据与时刻一个字不差）",
    );
    assert_eq!(
        site.catalog.verdict_values().expect("读得出"),
        旧库里的,
        "中立库里那两格也还在",
    );

    // 人撤掉一格：下次开现场，旧库里原来那一行**不许**又被搬回来。
    assert!(
        site.clear_verdict_value(AnchorKind::Work, 作品, Field::Year)
            .expect("撤得掉")
    );
    drop(site);
    let (_, site) = 开现场(工作目录.path());
    let 剩下的: Vec<String> = site
        .store
        .verdict_values(&标识)
        .expect("读得出")
        .into_iter()
        .map(|one| one.value)
        .collect();
    assert_eq!(剩下的, ["我的汉化组"], "撤掉的那一格被旧库带回来了");
    assert_eq!(
        site.catalog
            .verdict_values()
            .expect("读得出")
            .into_iter()
            .map(|one| one.value)
            .collect::<Vec<_>>(),
        ["我的汉化组"],
    );
}

#[test]
fn 结构版本对不上的旧库里的字段修改_列出来那一下就救进沉淀库_照提示删库重扫之后还在() {
    // 删库那句提示说详情页上改过的字段「一条不丢」，而人读到它的时候，那份库**开不进去**
    // ——开现场时搬一次那条路走不到。列出来那一下就得先救出来（同首选变体与叫法那一条）。
    let 主库 = 摆好主库();
    let 工作目录 = temp_dir("verdict-store-fields-stranded");
    let 库文件 = workspace::catalog_path(工作目录.path(), Slug::Named("主库"));
    {
        let mut catalog = Catalog::create(&库文件, "主库").expect("建得出中立库");
        catalog
            .put_verdict_value(AnchorKind::Work, 作品, Field::Year, "1987", 手写的)
            .expect("写得进旧库");
    }
    rusqlite::Connection::open(&库文件)
        .expect("开得出旧库")
        .execute_batch("UPDATE meta SET value = '6' WHERE key = 'schema_version';")
        .expect("改得成一份旧版本的库");

    let 列出来的 = workspace::catalogs(工作目录.path());
    let 那一份 = 列出来的
        .entries()
        .expect("列得开")
        .iter()
        .find(|一份| 一份.path == 库文件)
        .expect("版本对不上的那一份照样列出来了");
    let CatalogState::SchemaMismatch { said: 说的, .. } = &那一份.state else {
        panic!("该是结构版本对不上：{那一份:?}");
    };
    assert!(说的.contains("删掉它重扫一遍"), "{说的}");

    删库(&库文件);
    let (_, mut site) = 开现场(工作目录.path());
    扫(&mut site, 主库.path());
    裁成魂斗罗(&mut site);
    跑识别(&mut site, 主库.path());
    assert_eq!(
        写出去的(&site, Field::Year),
        裁决说("1987"),
        "照提示删库重扫之后，旧库里改过的年份还在",
    );
}

#[test]
fn 导出沉淀库不带详情页改过的字段() {
    // 字段修改与首选变体、亲手加的叫法同族：键是本机这一份主库里的作品名与变体的键，
    // 给别人一格也用不上（`--include-path` 也不带，同那两样，挂单 `Q722` 的裁决）。
    let 主库 = 摆好主库();
    let 工作目录 = temp_dir("verdict-store-fields-export");
    let (_, mut site) = 认好的现场(主库.path(), 工作目录.path());
    site.put_verdict_value(AnchorKind::Work, 作品, Field::Year, "1987", 手写的)
        .expect("改得动年份");
    site.put_verdict_value(
        AnchorKind::Variant,
        汉化,
        Field::TranslationGroup,
        "我的汉化组",
        手写的,
    )
    .expect("改得动汉化组");

    for 带路径锚 in [false, true] {
        let 导出 = site.store.export(带路径锚).expect("导得出");
        assert_eq!(导出.verdicts.len(), 2, "对照：两条内容锚的裁决在导出里");
        let 原文 = serde_json::to_string(&导出).expect("折得成 JSON");
        for 不该有的 in ["1987", "我的汉化组", 手写的] {
            assert!(
                !原文.contains(不该有的),
                "带路径锚={带路径锚}：导出里出现了「{不该有的}」：{原文}",
            );
        }
        let mut 别人的 = romcat_core::verdict::Store::in_memory().expect("开得出沉淀库");
        别人的.import(&原文).expect("导得进");
        assert!(
            别人的
                .verdict_values(&site.library_identity)
                .expect("读得出")
                .is_empty()
        );
    }
}
