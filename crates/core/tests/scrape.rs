//! **刮削接缝**：识别跑完之后，元数据与媒体是怎么落到锚点上的。
//!
//! 密集逻辑挂在纯函数上（`scrape::dat` 读 TOSEC 的名字、`scrape::local` 的两条归属规则、
//! `scrape::priority` 的合并各有自己的单元测试），这里要证的是另外六件事——正是票 13
//! 的六条验收：
//!
//! 1. 离线档只用本地数据源，跑完不需要任何网络句柄；
//! 2. 多个源按**字段级**优先级合并；
//! 3. 结果持久化缓存，重跑不重新计算；
//! 4. 媒体按**内容哈希**进池，中立库只记映射；
//! 5. 同一份媒体被多个条目引用时**只存一份**；
//! 6. **识别与刮削是两个可分别重跑的阶段**——重跑识别不该把刮削结论冲掉。
//!
//! fixture 的形状照真机来（`docs/library-facts.md`）：内容在**透明容器**里，媒体是
//! 汉化目录里那几张 `封面.png` / `1.jpg`，而 `FC/一堆/` 底下几个 zip 挤在一起——
//! 那正是「独占目录」规则必须缩手的场景。

use std::fs;
use std::path::Path;

use romcat_core::catalog::Catalog;
use romcat_core::catalog::Roots;
use romcat_core::dat::Convention;
use romcat_core::dat::logiqx::{DatHeader, GameRecord, RomRecord};
use romcat_core::dat::repo::{DatMeta, DatRepo, Unit};
use romcat_core::fs::RealFs;
use romcat_core::identify;
use romcat_core::identify::fuzzy;
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::scrape::pool::MediaPool;
use romcat_core::scrape::priority::VERDICT;
use romcat_core::scrape::{self, AnchorKind, Field, Priorities};
use romcat_core::task::Handle;
use romcat_core::testing::container::{ZipEntrySpec, crc32, zip_container};
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::title;

/// 原版魂斗罗的字节。
fn 原版() -> Vec<u8> {
    vec![0xA1; 4_096]
}

/// 汉化版的字节：改过，No-Intro 与 Redump 政策上永不收录，只有 TOSEC 有。
fn 汉化版() -> Vec<u8> {
    vec![0xB2; 4_096]
}

/// **另一个组做的汉化版**：与上面那份不是同一串字节，因此是同一部作品的第二个**变体**。
/// 票 02 要的正是这个形状——同一部作品名下两个变体，各自撞一次中文条目。
fn 汉化版二() -> Vec<u8> {
    vec![0xE5; 4_096]
}

/// 一张封面。**两个变体旁边各放一份一模一样的**，用来验「只存一份」。
fn 封面() -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend(std::iter::repeat_n(0x5Au8, 512));
    bytes
}

/// 一张截图。与封面**不同**的内容。
fn 截图() -> Vec<u8> {
    let mut bytes = b"\xff\xd8\xff\xe0".to_vec();
    bytes.extend(std::iter::repeat_n(0x77u8, 256));
    bytes
}

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

struct 现场 {
    dir: TempDir,
    pool_dir: TempDir,
    catalog: Catalog,
    repo: DatRepo,
}

/// 把一条**中立库的键**折回盘上那条相对主库根的路径：剥掉第一段根名。
/// 摆 fixture 用它，断言用键本身——两者差的正是这一段（`path::library_key`）。
fn 相对(key: &str) -> &str {
    key.strip_prefix("库/").unwrap_or(key)
}

const 原版变体: &str = "库/FC/魂斗罗原版/Contra (Japan).zip";
const 汉化变体: &str = "库/FC/魂斗罗汉化/魂斗罗[dwt_so 汉化].zip";
const 汉化变体二: &str = "库/FC/魂斗罗汉化二/魂斗罗[另一组 汉化].zip";
const 作品: &str = "Contra";

fn 建现场() -> 现场 {
    let dir = temp_dir("scrape");
    let root = dir.path();

    // ── 两个变体各自独占一个目录，旁边躺着图。这是真库里汉化合集的标准形态。
    写(
        &root.join(相对(原版变体)),
        &zip_container(&[ZipEntrySpec::stored("Contra.nes", 原版())]),
    );
    写(&root.join("FC/魂斗罗原版/封面.png"), &封面());
    写(
        &root.join(相对(汉化变体)),
        &zip_container(&[ZipEntrySpec::stored("魂斗罗.nes", 汉化版())]),
    );
    // **同一张封面**：内容一模一样，名字与位置都不同。
    写(&root.join("FC/魂斗罗汉化/封面.png"), &封面());
    写(&root.join("FC/魂斗罗汉化/1.jpg"), &截图());

    // ── 同一部作品的**第二个汉化变体**，独占一个目录、旁边没有图。
    // 它与上面那个变体的文件名剥出来是同一个**正题**，于是两个变体撞到同一条中文条目
    // ——票 02 的「作品锚点上只有一份类型」要的就是这个形状。
    写(
        &root.join(相对(汉化变体二)),
        &zip_container(&[ZipEntrySpec::stored("魂斗罗2.nes", 汉化版二())]),
    );

    // ── 一个目录里两个变体：**同目录的图一张都不许认**。
    写(
        &root.join("FC/一堆/甲.zip"),
        &zip_container(&[ZipEntrySpec::stored("甲.nes", vec![0xC3; 1_024])]),
    );
    写(
        &root.join("FC/一堆/乙.zip"),
        &zip_container(&[ZipEntrySpec::stored("乙.nes", vec![0xD4; 1_024])]),
    );
    写(&root.join("FC/一堆/无关.jpg"), &截图());

    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::named(root, "库");
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");

    现场 {
        dir,
        pool_dir: temp_dir("scrape-pool"),
        catalog,
        repo: 建_dat(),
    }
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
    // No-Intro：条目名最整齐，但**没有年份也没有发行商**。
    装(
        &mut repo,
        "No-Intro",
        "Nintendo - Nintendo Entertainment System",
        &[条目("Contra (Japan)", "Contra (Japan).nes", &原版())],
    );
    // TOSEC：名字里带着发行日期与发行商，还收录了汉化版。
    装(
        &mut repo,
        "TOSEC",
        "TOSEC/Nintendo Famicom & Entertainment System - Games - [NES].dat",
        &[
            条目(
                "Contra (1988-02-09)(Konami)(JP)",
                "Contra (1988-02-09)(Konami)(JP).nes",
                &原版(),
            ),
            条目(
                "Contra (1988-02-09)(Konami)(JP)[tr zh dwt_so][v.20030208]",
                "Contra [tr zh].nes",
                &汉化版(),
            ),
            条目(
                "Contra (1988-02-09)(Konami)(JP)[tr zh 另一组][v.20240101]",
                "Contra [tr zh 2].nes",
                &汉化版二(),
            ),
        ],
    );
    repo
}

fn 识别(现场: &mut 现场) {
    identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &identify::Ammo {
            repo: &现场.repo,
            verdicts: &verdict::Index::empty(),
            naming: &fuzzy::Naming::off(),
            guessing: &identify::model::Guessing::off(),
            titledb: None,
        },
        &identify::Options::new(Roots::single("库", 现场.dir.path())),
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别不该失败");
}

fn 刮削(现场: &mut 现场) -> scrape::Outcome {
    刮削一趟(现场, false)
}

fn 刮削一趟(现场: &mut 现场, refresh: bool) -> scrape::Outcome {
    刮削带上限(现场, refresh, None)
}

fn 刮削带上限(现场: &mut 现场, refresh: bool, cap: Option<u64>) -> scrape::Outcome {
    let mut options =
        scrape::Options::new(Roots::single("库", 现场.dir.path()), 现场.pool_dir.path());
    options.refresh = refresh;
    options.max_media_bytes = cap;
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
    .expect("刮削不该失败")
}

/// 带上一份**中文离线索引**跑一趟。中文离线源与它的别名那一路只在取过数之后参加。
fn 刮削带中文索引(现场: &mut 现场, index: &romcat_core::zh::Index) -> scrape::Outcome {
    刮削带中文简介(现场, index, None)
}

/// 再带上**简介那条路**跑一趟（票 03）。
///
/// 简介不跟着索引进内存（`scrape::zh::Summaries` 的文档），所以它是单独一个句柄。
fn 刮削带中文简介(
    现场: &mut 现场,
    index: &romcat_core::zh::Index,
    summaries: Option<&dyn scrape::zh::Summaries>,
) -> scrape::Outcome {
    刮削带裁决(现场, index, summaries, &scrape::zh::Rulings::none())
}

/// 再带上一份**匹配裁决**跑一趟（票 05）。
///
/// 裁决住在沉淀库里、按内容锚钉；摊平到变体键上是 `scrape::zh::Rulings::resolve` 的活，
/// 而刮削这一侧收的就是摊平之后的那一份。这里直接摆一份，测的是**刮削怎么用它**。
fn 刮削带裁决(
    现场: &mut 现场,
    index: &romcat_core::zh::Index,
    summaries: Option<&dyn scrape::zh::Summaries>,
    rulings: &scrape::zh::Rulings,
) -> scrape::Outcome {
    刮削带这几样(
        现场,
        index,
        summaries,
        rulings,
        &romcat_core::filename::Rules::builtin(),
    )
}

/// 再带上一份**自己的剥离规则**跑一趟（票 04）。
///
/// `--name-rules <文件>` 换的就是它。剥离规则决定从文件名里剥出什么**正题**，
/// 而中文离线源撞的正是正题。
fn 刮削带剥离规则(
    现场: &mut 现场,
    index: &romcat_core::zh::Index,
    rules: &romcat_core::filename::Rules,
) -> scrape::Outcome {
    刮削带这几样(现场, index, None, &scrape::zh::Rulings::none(), rules)
}

/// 最底下那一趟：索引、简介、裁决、剥离规则四样都摆得动。
fn 刮削带这几样(
    现场: &mut 现场,
    index: &romcat_core::zh::Index,
    summaries: Option<&dyn scrape::zh::Summaries>,
    rulings: &scrape::zh::Rulings,
    rules: &romcat_core::filename::Rules,
) -> scrape::Outcome {
    let options = scrape::Options::new(Roots::single("库", 现场.dir.path()), 现场.pool_dir.path());
    scrape::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &Priorities::builtin(),
        &options,
        // **网络句柄压根不传**：这一趟的网络请求数是 0，不是「小于某个数」。
        None,
        &mut scrape::RunContext {
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
            naming: &fuzzy::Naming {
                rules,
                index: Some(index),
                tuning: romcat_core::zh::Tuning::default(),
            },
            summaries,
            rulings,
        },
    )
    .expect("刮削不该失败")
}

/// 本机那份中文索引里的简介，摆在内存里的一份。
///
/// 真跑的时候这一路是 `zh::store::Store`（简介留在库里那一列上，按条目号点着读）。
/// 这里要测的是「撞上之后简介落在哪个锚点、超长的怎么处置」，不是「SQLite 读得出来
/// 没有」——那正是 `scrape::zh::Summaries` 收成一个 trait 的理由。
#[derive(Debug, Default)]
struct 简介表(std::collections::BTreeMap<u32, String>);

impl 简介表 {
    fn 一条(id: u32, text: &str) -> Self {
        Self(std::iter::once((id, text.to_string())).collect())
    }
}

impl scrape::zh::Summaries for 简介表 {
    fn summary(&self, id: u32) -> Result<Option<String>, String> {
        Ok(self.0.get(&id).cloned())
    }
}

/// 一条**读不动**的简介那一路：整条路都在，就是读不出来。
#[derive(Debug)]
struct 读不动的简介表;

impl scrape::zh::Summaries for 读不动的简介表 {
    fn summary(&self, _: u32) -> Result<Option<String>, String> {
        Err("库文件被截断了".to_string())
    }
}

/// 一份最小的中文离线索引：魂斗罗那一条，带两个别名。
fn 中文索引() -> romcat_core::zh::Index {
    romcat_core::zh::Index::build(
        vec![romcat_core::zh::Entry {
            id: 12_345,
            name: "魂斗羅".to_string(),
            name_cn: "魂斗罗".to_string(),
            aliases: vec!["魂斗羅".to_string(), "Probotector".to_string()],
            year: Some(1988),
            platforms: vec!["FC".to_string()],
            platform_text: "FC".to_string(),
            // **简介这一格空着是对的**：`zh::store::Store::load` 读回来的条目上它永远是
            // 空串（简介不进内存），简介走的是另一条路——`scrape::zh::Summaries`。
            summary: String::new(),
            genres: vec!["ACT".to_string()],
            developers: vec!["Konami".to_string()],
            publishers: vec!["Konami".to_string()],
        }],
        "dump-2026-09-01".to_string(),
    )
}

/// 一个锚点上某个字段、某个源给的全部值。
fn 各值(现场: &现场, anchor: &str, subject: &str, field: &str, source: &str) -> Vec<String> {
    现场
        .catalog
        .scraped_values(anchor, subject)
        .expect("读得出")
        .into_iter()
        .filter(|value| value.field == field && value.source == source)
        .map(|value| value.value)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// 一个锚点上某个字段、某个源给的值。
fn 值(现场: &现场, anchor: &str, subject: &str, field: &str, source: &str) -> Option<String> {
    现场
        .catalog
        .scraped_values(anchor, subject)
        .expect("读得出")
        .into_iter()
        .find(|value| value.field == field && value.source == source)
        .map(|value| value.value)
}

#[test]
fn 撞上中文条目的变体在变体锚点上多出别名而且进了标题集合() {
    // 票 01 的正题：撞上一条条目之后，那条条目的**别名**该进**标题集合**——
    // 同一部作品的几个叫法，用户搜哪个都该找得到。
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let index = 中文索引();
    let outcome = 刮削带中文索引(&mut 现场, &index);

    // 一、**这一趟的网络请求数是 0**：两个中文源都自报本地，档案那道闸门只放本地源进来，
    // 而 `scrape::run` 拿到的网络句柄是 `None`。
    assert!(
        !outcome
            .report
            .sources
            .contains(&"ScreenScraper".to_string())
    );
    assert!(outcome.report.sources.contains(&"中文离线源".to_string()));
    assert!(
        outcome
            .report
            .sources
            .contains(&"中文离线源·别名".to_string())
    );

    // 二、中文名照旧落在**变体**锚点上，一条。
    assert_eq!(
        值(&现场, "变体", 汉化变体, "标题", "中文离线源").as_deref(),
        Some("魂斗罗")
    );
    // 三、**别名多出来了**，而且是几条不是一条——去重键里带着值，它们各占一行、
    // 各带自己的**依据**。中文名不在这一路里重复一遍。
    let 别名 = 各值(&现场, "变体", 汉化变体, "标题", "中文离线源·别名");
    assert_eq!(别名, vec!["Probotector", "魂斗羅"]);
    let 依据 = 现场
        .catalog
        .scraped_values("变体", 汉化变体)
        .expect("读得出")
        .into_iter()
        .find(|value| value.source == "中文离线源·别名" && value.value == "魂斗羅")
        .expect("有这一条")
        .evidence;
    assert!(依据.contains("条目 12345"), "{依据}");
    assert!(依据.contains("还叫「魂斗羅」"), "{依据}");

    // 四、**它们进了标题集合**。
    let rows = romcat_core::title::fold(&现场.catalog).expect("折得出标题集合");
    let 集合: Vec<&str> = rows
        .iter()
        .filter(|row| row.work == 作品)
        .map(|row| row.value.as_str())
        .collect();
    for 叫法 in ["魂斗罗", "魂斗羅", "Probotector"] {
        assert!(集合.contains(&叫法), "标题集合里少了「{叫法}」：{集合:?}");
    }
    // 别名进的是**别名**那一档，不是译名——那一档留给官中版的官方译名（ADR-0012）。
    let 一条 = rows
        .iter()
        .find(|row| row.value == "Probotector")
        .expect("有这一条");
    assert_eq!(一条.kind, romcat_core::title::TitleKind::Alias);
    assert_eq!(一条.source, "中文离线源·别名");

    // 五、**显示标题不受别名影响**。别名那一路整路垫在语言与类型的档位**之前**
    // （`title::rank` 的第 2 层），所以这里两种别名都轮不到：`Probotector` 是拉丁字母，
    // 光靠回退链最后一档也拦得住；`魂斗羅` 与中文名 `魂斗罗` 同语言同类型，光靠置信度
    // 也拦得住（别名是低置信，见 `title::classify`）。真正非要第 2 层不可的是第三种
    // ——**中文别名遇上拉丁的中文名**，那时档位排在置信度前面，前两道都不管用
    // （`title::tests::中文名其实是拉丁串时低置信别名不当显示标题`）。
    let set = romcat_core::title::TitleSet {
        work: 作品.to_string(),
        entries: rows
            .iter()
            .filter(|row| row.work == 作品)
            .cloned()
            .collect(),
    };
    let chosen = romcat_core::title::choose(&set, &Priorities::builtin());
    assert_eq!(chosen.display, "魂斗罗", "显示标题该是撞上的那个中文名");
    assert_eq!(
        rows.iter()
            .find(|row| row.value == "魂斗羅")
            .expect("集合里有这一条")
            .confidence,
        romcat_core::catalog::identify::Confidence::Low,
        "别名是低置信——它与文件名同档，再由优先级表分先后"
    );

    // 六、**撞不上中文条目的变体一个新字段都不产出。**
    assert!(各值(&现场, "变体", "库/FC/一堆/甲.zip", "标题", "中文离线源").is_empty());
    assert!(
        各值(
            &现场,
            "变体",
            "库/FC/一堆/甲.zip",
            "标题",
            "中文离线源·别名"
        )
        .is_empty()
    );
}

#[test]
fn 类型落在作品锚点上而且同一条条目只留一份() {
    // 票 02 的正题：**撞在变体层、挂在作品层**。同一部作品的两个汉化变体各撞一次，
    // 撞到的是同一条中文条目，于是作品锚点上**只有一份**类型。
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let index = 中文索引();
    let outcome = 刮削带中文索引(&mut 现场, &index);

    // 一、**这一趟的网络请求数是 0**：网络句柄压根没传，档案那道闸门也只放本地源进来。
    assert!(
        !outcome
            .report
            .sources
            .contains(&"ScreenScraper".to_string())
    );
    assert!(outcome.report.sources.contains(&"中文离线源".to_string()));

    // 二、类型落在**作品**锚点上，源是中文离线源，而且只有一份。
    assert_eq!(各值(&现场, "作品", 作品, "类型", "中文离线源"), vec!["ACT"]);

    // 三、每条新字段都带**依据**：条目号、撞上的是哪个名字、两道校验的结果，
    // 外加「这条结论是名下哪个变体撞出来的」。
    let 依据 = 现场
        .catalog
        .scraped_values("作品", 作品)
        .expect("读得出")
        .into_iter()
        .find(|value| value.field == "类型" && value.source == "中文离线源")
        .expect("有这一条")
        .evidence;
    assert!(依据.contains("条目 12345"), "{依据}");
    assert!(依据.contains("的中文名「魂斗罗」"), "{依据}");
    assert!(依据.contains("平台交叉校验对得上"), "{依据}");
    // 年份这一道钉**结果**：条目写着 1988，而 TOSEC 那条条目名的第一个括号也是 1988，
    // 所以两边都说得出、而且对得上。只钉「年份交叉校验」这五个字是恒真的——
    // 那句话无条件拼在每一条依据上。
    assert!(依据.contains("年份交叉校验对得上"), "{依据}");
    // **写全那个键**：名下两个汉化变体都以 `FC/魂斗罗汉化` 打头，只写前缀的话
    // 换成哪一个断言都照绿。代表取的是变体键最小的那个。
    assert!(
        依据.contains(&format!("名下的变体「{汉化变体}」")),
        "{依据}"
    );
    assert!(
        依据.contains("名下 2 个变体撞上了中文条目，其中 2 个撞的是这一条"),
        "{依据}"
    );
    // **产出仍是中置信、照旧进待确认队列**：多了几个字段不等于自动通过。
    assert!(依据.contains("一律进待确认队列"), "{依据}");

    // 四、**类型不挂在变体上**：那是作品级的字段，挂到变体上就是每个变体各存一份。
    for 变体 in [汉化变体, 汉化变体二] {
        assert!(各值(&现场, "变体", 变体, "类型", "中文离线源").is_empty());
    }

    // 五、**中文名与别名仍然在变体锚点上，没有被顺手搬走。**
    for 变体 in [汉化变体, 汉化变体二] {
        assert_eq!(
            值(&现场, "变体", 变体, "标题", "中文离线源").as_deref(),
            Some("魂斗罗")
        );
        assert_eq!(
            各值(&现场, "变体", 变体, "标题", "中文离线源·别名"),
            vec!["Probotector", "魂斗羅"]
        );
    }
    assert!(各值(&现场, "作品", 作品, "标题", "中文离线源").is_empty());
    assert!(
        各值(&现场, "作品", 作品, "标题", "中文离线源·别名").is_empty(),
        "别名那一路不跟到作品那一层去"
    );

    // 六、合并之后作品那一层的类型就是它——离线档里这一栏不再是空的。
    let values = 现场.catalog.scraped_values("作品", 作品).expect("读得出");
    let merged = Priorities::builtin().merge(Some("FC"), &values);
    assert_eq!(merged["类型"].source, "中文离线源");
    assert_eq!(merged["类型"].value, "ACT");

    // 七、**再跑一趟，作品那一层的类型还在。** 这一条是作品层「带输入不带结果」那个
    // 决定的验收：第二趟变体那一层整片命中缓存、`collect` 一次都不跑，若作品层读的是
    // 变体撞完的结果，这里就会空手而归，把上一趟好好的类型当成「这个源改主意了」清掉。
    let 再跑 = 刮削带中文索引(&mut 现场, &index);
    assert!(
        再跑.reused_probes > 0,
        "第二趟该有锚点因为输入指纹没变而跳过"
    );
    assert_eq!(再跑.forgotten, 0, "一条结论都不该被当成作废清掉");
    assert_eq!(各值(&现场, "作品", 作品, "类型", "中文离线源"), vec!["ACT"]);
}

#[test]
fn 名下一个变体都没撞上的作品不产出任何字段() {
    // **宁可留空也不要写错的**：撞不上就一个字段都不产出，而不是给一条像模像样的猜测。
    let mut 现场 = 建现场();
    识别(&mut 现场);
    // 索引里只有一条与这个库毫不相干的条目。
    let index = romcat_core::zh::Index::build(
        vec![romcat_core::zh::Entry {
            id: 999,
            name: "スーパーマリオ".to_string(),
            name_cn: "超级马里奥".to_string(),
            year: Some(1985),
            platforms: vec!["FC".to_string()],
            platform_text: "FC".to_string(),
            genres: vec!["ACT".to_string()],
            ..romcat_core::zh::Entry::default()
        }],
        "dump-2026-09-01".to_string(),
    );
    刮削带中文索引(&mut 现场, &index);

    let 中文的 = 现场
        .catalog
        .scraped_values("作品", 作品)
        .expect("读得出")
        .into_iter()
        .filter(|value| value.source.starts_with("中文离线源"))
        .count();
    assert_eq!(中文的, 0, "名下一个变体都没撞上，作品锚点上不该有任何一条");
}

/// 数据源里那条 infobox 的原样：**开发**写成顿号分隔的一行，**发行**写成多值块。
///
/// 两种写法真库里都有，说的是同一件事（`zh::dump` 的模块文档），而顿号那一种更常见。
/// 顺带还摆了 `|发行日期=`：它与 `|发行=` 是两个键，各读各的。
const 魂斗罗信息框: &str = "{{Infobox Game\n|中文名= 魂斗罗\n|平台= FC\n|游戏类型= ACT\n\
                           |开发= 科乐美、KCE东京\n|发行={\n[科乐美]\n[任天堂]\n}\n\
                           |发行日期= 1988-02-09\n}}";

/// 一份中文离线索引，条目上那几样**从 infobox 折出来**。
///
/// 走的是取数那一侧的同一条路：`zh::sync` 折条目时调的就是 `Row::developers` /
/// `Row::publishers`。直接把 `vec!["甲", "乙"]` 摆进去也能测出「多条落进库」，
/// 但那样一来「顿号那一行是怎么变成两条的」在这条接缝上就一个字都没证。
fn 中文索引带信息框(infobox: &str) -> romcat_core::zh::Index {
    let row = romcat_core::zh::dump::Row {
        id: 12_345,
        kind: 4,
        name: "魂斗羅".to_string(),
        name_cn: "魂斗罗".to_string(),
        platform: 4001,
        infobox: infobox.to_string(),
        summary: String::new(),
        date: Some("1988-02-09".to_string()),
        meta_tags: Vec::new(),
    };
    assert!(row.is_game(), "fixture 得是个游戏条目");
    romcat_core::zh::Index::build(
        vec![romcat_core::zh::Entry {
            id: row.id,
            name: row.name.clone(),
            name_cn: row.name_cn.clone(),
            aliases: row.aliases(),
            year: row.year(),
            platforms: vec!["FC".to_string()],
            platform_text: row.platforms().join("、"),
            // 简介走的是另一条路（`scrape::zh::Summaries`），索引上这一格永远空着。
            summary: String::new(),
            genres: row.genres(),
            developers: row.developers(),
            publishers: row.publishers(),
        }],
        "dump-2026-09-01".to_string(),
    )
}

#[test]
fn 开发商与发行商落在作品锚点上而且一个键写了几个值就拆成几条() {
    // 票 04 的正题：跑一趟**离线档**，开发商与发行商落在**作品**锚点上，源是中文离线源，
    // 而 infobox 里一个键写了几个值，库里就是**几条**——不是一串带顿号的长字符串。
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let index = 中文索引带信息框(魂斗罗信息框);
    let outcome = 刮削带中文索引(&mut 现场, &index);

    // 一、**这一趟的网络请求数是 0**：网络句柄压根没传，档案那道闸门也只放本地源进来。
    assert!(
        !outcome
            .report
            .sources
            .contains(&"ScreenScraper".to_string())
    );
    assert!(outcome.report.sources.contains(&"中文离线源".to_string()));
    assert!(
        outcome.report.online.is_none(),
        "离线档不该有在线那一侧的账"
    );

    // 二、**顿号分隔的一行拆成两条**（`|开发= 科乐美、KCE东京`）。
    assert_eq!(
        各值(&现场, "作品", 作品, "开发商", "中文离线源"),
        vec!["KCE东京", "科乐美"],
        "顿号那一行该是两条开发商，不是一条「科乐美、KCE东京」",
    );

    // 三、**多值块拆成两条**（`|发行={ [科乐美] [任天堂] }`）。
    assert_eq!(
        各值(&现场, "作品", 作品, "发行商", "中文离线源"),
        vec!["任天堂", "科乐美"],
    );

    // 四、这两样**不挂在变体上**：跨平台跨地区都成立的东西挂作品层，
    // 挂到变体上就是同一部作品的每个变体各存一份。
    for 变体 in [原版变体, 汉化变体, 汉化变体二] {
        assert!(各值(&现场, "变体", 变体, "开发商", "中文离线源").is_empty());
        assert!(各值(&现场, "变体", 变体, "发行商", "中文离线源").is_empty());
    }

    // 五、**每一条都带依据**：条目号、撞上的是哪个名字、两道校验的结果，
    // 外加「这条结论是名下哪个变体撞出来的」。
    let 全部 = 现场.catalog.scraped_values("作品", 作品).expect("读得出");
    let 开发商的依据: Vec<&str> = 全部
        .iter()
        .filter(|value| value.field == "开发商" && value.source == "中文离线源")
        .map(|value| value.evidence.as_str())
        .collect();
    assert_eq!(开发商的依据.len(), 2);
    for 依据 in 开发商的依据 {
        assert!(依据.contains("条目 12345"), "{依据}");
        assert!(依据.contains("的中文名「魂斗罗」"), "{依据}");
        assert!(依据.contains("平台交叉校验对得上"), "{依据}");
        assert!(依据.contains("年份交叉校验对得上"), "{依据}");
        assert!(依据.contains("而开发商跨平台跨地区都成立"), "{依据}");
        assert!(
            依据.contains(&format!("名下的变体「{汉化变体}」")),
            "{依据}"
        );
        // **中置信、照旧进待确认队列**：多两栏不等于自动通过（票 05 才管裁决那一侧）。
        assert!(依据.contains("一律进待确认队列"), "{依据}");
    }

    // 六、合并之后：**开发商这一栏由中文离线源填**——DAT 那几家一条都给不出，
    // 而 ScreenScraper 在离线档里根本不在场。
    let merged = Priorities::builtin().merge(Some("FC"), &全部);
    assert_eq!(merged["开发商"].source, "中文离线源");
    // 发行商那一栏**仍旧由 TOSEC 说了算**：它在离线档里是在场的，而且排在前面——
    // 它的发行商跟着那一条按哈希确认的 DAT 记录走。这是优先级表里明写的名次。
    assert_eq!(
        merged["发行商"].source, "TOSEC",
        "TOSEC 认得出这个文件时，发行商该听它的",
    );

    // 七、报告里「一个值都没采到的字段」不再点名开发商。
    assert!(
        !outcome.report.gaps.contains(&"开发商".to_string()),
        "离线档现在补得上开发商了：{:?}",
        outcome.report.gaps,
    );

    // 八、**再跑一趟，两栏都还在。** 与类型、简介两条同一个道理（票 02 的骨架）：
    // 第二趟变体那一层整片命中缓存，作品层照样自己现撞一遍。
    let 再跑 = 刮削带中文索引(&mut 现场, &index);
    assert_eq!(再跑.forgotten, 0, "一条结论都不该被当成作废清掉");
    assert_eq!(
        各值(&现场, "作品", 作品, "开发商", "中文离线源"),
        vec!["KCE东京", "科乐美"],
    );
    assert_eq!(
        各值(&现场, "作品", 作品, "发行商", "中文离线源"),
        vec!["任天堂", "科乐美"],
    );
}

#[test]
fn 数据源缺开发这个键的条目照常产出它有的那些字段() {
    // 规格 21：**缺键就是没有，不是错误**。这条 infobox 写了发行没写开发。
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let index = 中文索引带信息框(
        "{{Infobox Game\n|中文名= 魂斗罗\n|平台= FC\n|游戏类型= ACT\n\
         |发行= 科乐美\n|发行日期= 1988-02-09\n}}",
    );
    刮削带中文索引(&mut 现场, &index);

    // 缺的那个键就该是缺的——而不是一条空串（空值会让优先级链在它身上停下来）。
    assert!(
        各值(&现场, "作品", 作品, "开发商", "中文离线源").is_empty(),
        "没写的键不该凭空冒出一条",
    );
    // **只有一个值时就是一条**，没有多出空条目。
    assert_eq!(
        各值(&现场, "作品", 作品, "发行商", "中文离线源"),
        vec!["科乐美"]
    );
    // **整条不跳过**：它写了的那几样照样落库。
    assert_eq!(各值(&现场, "作品", 作品, "类型", "中文离线源"), vec!["ACT"]);
    assert_eq!(
        值(&现场, "变体", 汉化变体, "标题", "中文离线源").as_deref(),
        Some("魂斗罗"),
    );
}

/// 数据源里那条简介的原样：开头两个**全角空格**、中间一个换行。
///
/// Bangumi 的简介几乎都是这个形状。`str::trim` 会把 U+3000 当空白扫掉，所以这一路上
/// 一个字都不许改、两头也不许掐（挂单 Q3）。
const 简介原文: &str = "　　两个人一起打外星人。\n第二段：外星人赢了。";

#[test]
fn 简介落在作品锚点上而且换行与全角空格逐字保留() {
    // 票 03 的正题：跑一趟**离线档**，简介落在**作品**锚点上，源是中文离线源，
    // 而这一趟一个网络请求都没发。
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let index = 中文索引();
    let 简介 = 简介表::一条(12_345, 简介原文);
    let outcome = 刮削带中文简介(&mut 现场, &index, Some(&简介));

    // 一、**这一趟的网络请求数是 0**：网络句柄压根没传，档案那道闸门也只放本地源进来。
    assert!(
        !outcome
            .report
            .sources
            .contains(&"ScreenScraper".to_string())
    );
    assert!(outcome.report.sources.contains(&"中文离线源".to_string()));
    assert!(
        outcome.report.online.is_none(),
        "离线档不该有在线那一侧的账"
    );

    // 二、简介落在**作品**锚点上，源是中文离线源，而且**只有一份**——同一部作品名下
    // 两个汉化变体各撞了一次，撞到的是同一条条目。
    assert_eq!(
        各值(&现场, "作品", 作品, "简介", "中文离线源"),
        vec![简介原文],
        "作品锚点上该正好有一份简介，而且是原文",
    );

    // 三、**逐字保留**：开头那两个全角空格与中间那个换行都是内容的一部分，
    // 掐掉两头（`str::trim` 那一档）就违反规格 18。
    let 落库 = 值(&现场, "作品", 作品, "简介", "中文离线源").expect("有这一条");
    assert!(
        落库.starts_with('\u{3000}'),
        "开头那两个全角空格被吃掉了：{落库:?}"
    );
    assert!(落库.contains('\n'), "中间那个换行被压掉了：{落库:?}");
    assert_eq!(落库.chars().count(), 简介原文.chars().count(), "长度都变了");

    // 四、简介**不挂在变体上**：那是作品级的字段，挂到变体上就是每个变体各存一份。
    for 变体 in [原版变体, 汉化变体, 汉化变体二] {
        assert!(各值(&现场, "变体", 变体, "简介", "中文离线源").is_empty());
    }

    // 五、每一条都带**依据**：条目号、撞上的是哪个名字、两道校验的结果，
    // 外加「这条结论是名下哪个变体撞出来的」。
    let 依据 = 现场
        .catalog
        .scraped_values("作品", 作品)
        .expect("读得出")
        .into_iter()
        .find(|value| value.field == "简介" && value.source == "中文离线源")
        .expect("有这一条")
        .evidence;
    assert!(依据.contains("条目 12345"), "{依据}");
    assert!(依据.contains("的中文名「魂斗罗」"), "{依据}");
    assert!(依据.contains("平台交叉校验对得上"), "{依据}");
    assert!(依据.contains("而简介跨平台跨地区都成立"), "{依据}");
    assert!(
        依据.contains(&format!("名下的变体「{汉化变体}」")),
        "{依据}"
    );
    // **中置信、照旧进待确认队列**：多一个字段不等于自动通过（票 05 才管裁决那一侧）。
    assert!(依据.contains("一律进待确认队列"), "{依据}");
    // 没超闸的那一条**不该**说自己被截断了。
    assert!(!依据.contains("这条简介被截断了"), "{依据}");

    // 六、合并之后作品那一层的简介就是它——离线档这一栏不再是空的。
    let values = 现场.catalog.scraped_values("作品", 作品).expect("读得出");
    let merged = Priorities::builtin().merge(Some("FC"), &values);
    assert_eq!(merged["简介"].source, "中文离线源");
    assert_eq!(merged["简介"].value, 简介原文);

    // 七、报告里「一个值都没采到的字段」不再点名简介。
    assert!(
        !outcome.report.gaps.contains(&"简介".to_string()),
        "离线档现在补得上简介了：{:?}",
        outcome.report.gaps,
    );
    assert!(
        outcome.report.truncated_descriptions.is_empty(),
        "没超闸的简介不该被点名"
    );

    // 八、**再跑一趟，简介还在。** 与类型那一条同一个道理（票 02 的骨架）：
    // 第二趟变体那一层整片命中缓存，作品层照样自己现撞一遍。
    let 再跑 = 刮削带中文简介(&mut 现场, &index, Some(&简介));
    assert_eq!(再跑.forgotten, 0, "一条结论都不该被当成作废清掉");
    assert_eq!(
        各值(&现场, "作品", 作品, "简介", "中文离线源"),
        vec![简介原文]
    );
}

#[test]
fn 超长的简介有闸而且被截断的条目在报告里点得出名() {
    // 实测最长一条 9,962 字。**存储与导出都不能因为超长条目变得不可用**，
    // 而截断这件事**不许是悄悄发生的**（票 03 的两处边界之一）。
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let index = 中文索引();
    // 造一条比实测最长那条还长的：闸响不响与「数据源里最长是多少」无关。
    let 超长: String = "外".repeat(scrape::zh::DESCRIPTION_LIMIT + 500);
    let outcome = 刮削带中文简介(&mut 现场, &index, Some(&简介表::一条(12_345, &超长)));

    let 落库 = 值(&现场, "作品", 作品, "简介", "中文离线源").expect("有这一条");
    // 一、**闸真的响了**：正文截到闸上，后面缀的是那句说明，不是原文。
    assert!(
        落库.chars().count() < 超长.chars().count(),
        "一个字都没截：{} 字",
        落库.chars().count(),
    );
    assert!(
        落库.starts_with(&"外".repeat(scrape::zh::DESCRIPTION_LIMIT)),
        "截的不是前 {} 字",
        scrape::zh::DESCRIPTION_LIMIT,
    );
    // 二、**用户看得见**：落库那一份自己带着一句说明，前端里读到的那段不会无缘无故
    // 断在半路。
    assert!(落库.contains(scrape::zh::TRUNCATED_MARK), "{落库:.80}");
    assert!(
        落库.contains(&format!("{} 字", 超长.chars().count())),
        "说明里该写得出原文有多少字",
    );

    // 三、**依据**里也说得出这件事。
    let 依据 = 现场
        .catalog
        .scraped_values("作品", 作品)
        .expect("读得出")
        .into_iter()
        .find(|value| value.field == "简介" && value.source == "中文离线源")
        .expect("有这一条")
        .evidence;
    assert!(依据.contains("这条简介被截断了"), "{依据}");

    // 四、**报告里点得出名**：哪一个锚点被截了，照着这份名单查得回去。
    assert_eq!(
        outcome.report.truncated_descriptions,
        vec![("作品".to_string(), 作品.to_string())],
        "被截断的条目该在报告里逐条点名",
    );
    let 文本 = outcome.report.render_text();
    assert!(文本.contains("被截断的简介"), "文本报告里该有这一节");
    assert!(文本.contains(作品), "文本报告里该点得出锚点的名字");

    // 五、**报告是从中立库折出来的**（ADR-0001）：上一趟截掉的那些，下一趟照样点得出名。
    let 再跑 = 刮削带中文简介(&mut 现场, &index, Some(&简介表::一条(12_345, &超长)));
    assert_eq!(再跑.report.truncated_descriptions.len(), 1);
}

#[test]
fn 简介那条路没接上时一条简介都不产出而接上之后重跑真的补得上() {
    // 简介不跟着索引进内存，所以它是单独一条路。这条路**在不在场**进输入指纹：
    // 不进的话，把它接上之后重跑，缓存会一口咬定「输入没变」而整条跳过。
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let index = 中文索引();

    // 一、没接上时**一条简介都不产出**，而类型照旧产出——两者不是同生共死。
    刮削带中文索引(&mut 现场, &index);
    assert!(各值(&现场, "作品", 作品, "简介", "中文离线源").is_empty());
    assert_eq!(各值(&现场, "作品", 作品, "类型", "中文离线源"), vec!["ACT"]);

    // 二、接上之后重跑，简介**真的补上来了**，而且没有 `--refresh`。
    let 简介 = 简介表::一条(12_345, 简介原文);
    刮削带中文简介(&mut 现场, &index, Some(&简介));
    assert_eq!(
        各值(&现场, "作品", 作品, "简介", "中文离线源"),
        vec![简介原文]
    );
}

#[test]
fn 撞不上的作品不产出简介() {
    // **宁可留空也不要写错的**：撞不上就一个字段都不产出，而不是给一条像模像样的猜测。
    let mut 现场 = 建现场();
    识别(&mut 现场);
    // 索引里只有一条与这个库毫不相干的条目，而简介那一路**每一条都答得出来**——
    // 于是「没有简介」只可能是因为没撞上，不可能是因为简介取不到。
    let index = romcat_core::zh::Index::build(
        vec![romcat_core::zh::Entry {
            id: 999,
            name: "スーパーマリオ".to_string(),
            name_cn: "超级马里奥".to_string(),
            year: Some(1985),
            platforms: vec!["FC".to_string()],
            platform_text: "FC".to_string(),
            genres: vec!["ACT".to_string()],
            ..romcat_core::zh::Entry::default()
        }],
        "dump-2026-09-01".to_string(),
    );
    let 简介 = 简介表(
        [(999, 简介原文.to_string()), (12_345, 简介原文.to_string())]
            .into_iter()
            .collect(),
    );
    let outcome = 刮削带中文简介(&mut 现场, &index, Some(&简介));

    assert!(各值(&现场, "作品", 作品, "简介", "中文离线源").is_empty());
    for 变体 in [原版变体, 汉化变体, 汉化变体二] {
        assert!(各值(&现场, "变体", 变体, "简介", "中文离线源").is_empty());
    }
    // 报告那一栏照旧空着——**空着是如实的**，不是漏了。
    assert!(outcome.report.gaps.contains(&"简介".to_string()));
}

#[test]
fn 简介读不出来时这一对不写库而不是当成没有简介() {
    // 吞成「这条没有简介」的话，一次读库失败会让整趟悄悄少一栏，而报告还说得像模像样；
    // 更糟的是缓存会把这一趟的空手当成结论，下一趟连重试都不会有。
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let index = 中文索引();
    刮削带中文简介(&mut 现场, &index, Some(&读不动的简介表));

    // 作品锚点上**中文离线源一条值都没有**——连同一次采集里的类型一起，整对没写库。
    let 中文的 = 现场
        .catalog
        .scraped_values("作品", 作品)
        .expect("读得出")
        .into_iter()
        .filter(|value| value.source == "中文离线源")
        .count();
    assert_eq!(中文的, 0, "简介读不动时这一对不该写库");

    // 而变体那一层照跑——那一层整个在内存里，没有会失败的动作。
    assert_eq!(
        值(&现场, "变体", 汉化变体, "标题", "中文离线源").as_deref(),
        Some("魂斗罗"),
    );

    // 下一趟路通了就补得上：上一趟没写库，也就没有指纹把它挡在外面。
    let 简介 = 简介表::一条(12_345, 简介原文);
    刮削带中文简介(&mut 现场, &index, Some(&简介));
    assert_eq!(
        各值(&现场, "作品", 作品, "简介", "中文离线源"),
        vec![简介原文]
    );
}

#[test]
fn 离线档只用本地数据源() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let outcome = 刮削(&mut 现场);

    // `scrape::run` 的参数表里根本没有网络句柄——它拿到的是主库的**只读**视图、
    // 中立库、优先级表。这一条不是靠自觉，档案那道闸门会拒掉任何自报「要联网」的源。
    assert_eq!(
        outcome.report.sources,
        vec![
            "No-Intro",
            "Redump",
            "TOSEC",
            "MAME",
            "GoodNES",
            "文件名",
            "本地媒体"
        ],
        "离线档该正好是这七个本地源"
    );
}

#[test]
fn 元数据按层挂到锚点上() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);

    // **作品**这一层：标题两个源都给得出，年份与发行商只有 TOSEC 给得出。
    assert_eq!(
        值(&现场, "作品", 作品, "标题", "No-Intro").as_deref(),
        Some("Contra")
    );
    assert_eq!(
        值(&现场, "作品", 作品, "标题", "TOSEC").as_deref(),
        Some("Contra")
    );
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988")
    );
    assert_eq!(
        值(&现场, "作品", 作品, "发行商", "TOSEC").as_deref(),
        Some("Konami")
    );
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "No-Intro"),
        None,
        "No-Intro 的名字里根本没有年份，不该凭空冒出来"
    );

    // **变体**这一层：汉化组挂在这里（汉化版是变体不是发行版，ADR-0012）。
    assert_eq!(
        值(&现场, "变体", 汉化变体, "汉化组", "TOSEC").as_deref(),
        Some("dwt_so")
    );
    // 文件名兜底：DAT 认不出来的那些变体，标题只能从这儿来。
    assert_eq!(
        值(&现场, "变体", "库/FC/一堆/甲.zip", "标题", "文件名").as_deref(),
        Some("甲")
    );
}

#[test]
fn 字段级优先级用一个源的标题配另一个源的年份() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);

    let priorities = Priorities::builtin();
    let values = 现场.catalog.scraped_values("作品", 作品).expect("读得出");
    let merged = priorities.merge(Some("FC"), &values);

    assert_eq!(
        merged["标题"].source, "No-Intro",
        "标题该取条目名最整齐的那家"
    );
    assert_eq!(merged["年份"].source, "TOSEC", "而年份只有 TOSEC 给得出");
    assert_eq!(merged["发行商"].value, "Konami");

    // **改一次优先级不必重新采集**：三元组并存，换的只是排序。
    let 换一份 = Priorities::load(&写一份优先级表(&现场)).expect("读得动");
    assert_eq!(
        换一份.merge(Some("FC"), &values)["标题"].source,
        "TOSEC",
        "把 TOSEC 提到前面之后，标题该跟着换源——而库里一条值都没重采"
    );
}

/// 写一份把 TOSEC 提到 No-Intro 前面的优先级表。
fn 写一份优先级表(现场: &现场) -> std::path::PathBuf {
    let path = 现场.pool_dir.path().join("我的优先级.toml");
    fs::write(
        &path,
        "\"版本\" = 1\n\
         [[\"字段\"]]\n\
         \"名\" = \"标题\"\n\
         \"顺序\" = [\"裁决\", \"TOSEC\", \"No-Intro\", \"文件名\"]\n\
         [[\"字段\"]]\n\
         \"名\" = \"年份\"\n\
         \"顺序\" = [\"裁决\", \"TOSEC\"]\n",
    )
    .expect("写得出");
    path
}

#[test]
fn 媒体按内容哈希入池且同一份只存一份() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let outcome = 刮削(&mut 现场);

    // 两个变体旁边各有一份**一模一样**的封面，加上一张截图：三条引用、两份内容。
    let 原版媒体 = 现场
        .catalog
        .scraped_media("变体", 原版变体)
        .expect("读得出");
    let 汉化媒体 = 现场
        .catalog
        .scraped_media("变体", 汉化变体)
        .expect("读得出");
    assert_eq!(原版媒体.len(), 1);
    assert_eq!(汉化媒体.len(), 2);

    let 原版封面 = &原版媒体[0];
    let 汉化封面 = 汉化媒体
        .iter()
        .find(|m| m.kind == "封面")
        .expect("汉化那边也有封面");
    assert_eq!(
        原版封面.hash, 汉化封面.hash,
        "**文件名不是媒体的主键**：两份一模一样的字节必须算出同一个哈希"
    );

    // 池里只有一个文件。
    let pool = MediaPool::open(现场.pool_dir.path()).expect("开得出池");
    assert!(pool.contains(&原版封面.hash, "png"), "池里该有这一份");
    assert_eq!(outcome.new_blobs, 2, "两份不同的内容");
    assert_eq!(outcome.deduped, 1, "第三份算出来发现已经有了");

    let counts = 现场.catalog.pool_counts().expect("数得出");
    assert_eq!(counts.blobs, 2, "池里两份内容");
    assert_eq!(counts.refs, 3, "三条引用");
    assert_eq!(counts.shared, 1, "其中一份被两个锚点引用");

    // **中立库只记映射**：文件躺在池里，库里存的是哈希。
    assert!(
        !原版封面.hash.is_empty() && 原版封面.hash.len() == 64,
        "哈希该是 SHA-256 的 64 位十六进制"
    );
}

#[test]
fn 挤在一个目录里的变体一张图都不认() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);

    for key in ["库/FC/一堆/甲.zip", "库/FC/一堆/乙.zip"] {
        assert!(
            现场
                .catalog
                .scraped_media("变体", key)
                .expect("读得出")
                .is_empty(),
            "{key} 所在的目录里有两个变体，同目录的图属于谁说不清，宁可不给"
        );
    }
}

#[test]
fn 重跑不重新计算() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let 首趟 = 刮削(&mut 现场);
    assert!(首趟.read_bytes > 0, "首趟要把媒体读进池里");

    let 第二趟 = 刮削(&mut 现场);
    assert_eq!(第二趟.read_bytes, 0, "第二趟一个字节都不该再读盘");
    assert_eq!(第二趟.read_files, 0);
    assert!(
        第二趟.reused_probes > 0,
        "输入指纹没变的「锚点 × 源」该整条跳过"
    );
    assert_eq!(第二趟.new_blobs, 0, "池里不该多出任何东西");

    // 结论仍然在，且没有被重复插成两条。
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988")
    );
    assert_eq!(现场.catalog.pool_counts().expect("数得出").refs, 3);
}

#[test]
fn refresh_把结论重采一遍但池里的文件一个都不删() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);
    let 重来 = 刮削一趟(&mut 现场, true);

    assert_eq!(重来.reused_probes, 0, "--refresh 之后没有一条能跳过");
    assert_eq!(
        重来.read_bytes, 0,
        "但**算过的媒体哈希还在**——重采的是结论，不是重读一遍盘"
    );
    assert_eq!(现场.catalog.pool_counts().expect("数得出").blobs, 2);
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988")
    );
}

#[test]
fn refresh_把裁决写下的值原样留着() {
    // 人在浏览屏的详情面板上按下「写下」，来源记作**裁决**（`put_verdict_value`）。
    // 优先级表把裁决排在每个字段最前，导出真会用它；而**沉淀库里没有第二份**
    // ——`verdict.rs` 只导出裁决与匹配两张表，中立库里这条一没就永远没了。
    // 同一层的 `clear_titles` 早就写着「裁决定下来的一行都不碰」，这里要的是同一条纪律。
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);

    现场
        .catalog
        .put_verdict_value(
            AnchorKind::Work,
            作品,
            Field::Year,
            "1987",
            "浏览屏的详情面板上人工写的",
        )
        .expect("写得下");
    现场
        .catalog
        .put_verdict_value(
            AnchorKind::Variant,
            原版变体,
            Field::Title,
            "魂斗罗",
            "浏览屏的详情面板上人工写的",
        )
        .expect("写得下");

    let 重来 = 刮削一趟(&mut 现场, true);

    assert_eq!(
        值(&现场, "作品", 作品, "年份", VERDICT).as_deref(),
        Some("1987"),
        "作品锚点上人写下的年份该原样在着"
    );
    assert_eq!(
        值(&现场, "变体", 原版变体, "标题", VERDICT).as_deref(),
        Some("魂斗罗"),
        "变体锚点上人写下的标题同样"
    );

    // **只留裁决，不留别的**：采集记录照旧清空（一条都跳不过），数据源的值重采一遍。
    assert_eq!(重来.reused_probes, 0, "--refresh 之后没有一条能跳过");
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988")
    );
}

#[test]
fn 识别重跑之后刮削结论还在() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);
    let 之前 = 现场.catalog.scraped_values("作品", 作品).expect("读得出");
    assert!(!之前.is_empty());

    // 重跑识别：发行版**整批删掉再造一遍**，新造出来的行拿的是新的行号；作品那一行
    // 按名字复用（票 parking-3/10）。刮削结论挂在**作品名**上，两条路都活得下来。
    识别(&mut 现场);

    let 之后 = 现场.catalog.scraped_values("作品", 作品).expect("读得出");
    assert_eq!(之前, 之后, "重跑识别不该把刮削结论冲掉");
    assert_eq!(
        现场
            .catalog
            .scraped_media("变体", 原版变体)
            .expect("读得出")
            .len(),
        1,
        "媒体映射同样活得下来——它挂在变体的键上"
    );

    // 反过来也成立：刮削可以在不重跑识别的前提下再跑一遍。
    let 再刮 = 刮削(&mut 现场);
    assert_eq!(再刮.read_bytes, 0);
}

#[test]
fn 报告说离线档补不上的是图而不是简介类型开发商() {
    // 票 06 的正题。这一趟**没带中文索引**（`Naming::off`），所以那几栏确实空着——
    // 报告照旧点名，但**不许把空着说成「离线档补不上」**：那三样撞上一条中文条目就有。
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let outcome = 刮削(&mut 现场);

    // 空着的字段照旧点名——不点名，用户看到的就是「刮削跑完了」。
    for field in ["简介", "类型", "开发商"] {
        assert!(
            outcome.report.gaps.iter().any(|gap| gap == field),
            "报告该点名「{field}」这一趟一个值都没采到"
        );
    }
    let text = outcome.report.render_text();
    assert!(text.contains("一个值都没采到的字段"));
    // **那句假话不许再出现。** 它把「不含图片」误传成「补不上简介、类型与开发商」，
    // 代价是用户被推去烧在线配额换英文简介，而中文简介就躺在本地。
    assert!(
        !text.contains("本地数据源里没有这些东西"),
        "报告不许再说本地数据源里没有这几样：\n{text}"
    );
    assert!(
        !text.contains("换 `--profile 在线` 跑一趟才补得上"),
        "报告不许再把这几样推给在线档：\n{text}"
    );
    assert!(
        text.contains("**这不等于「离线档补不上」**"),
        "空着的字段要说清空的是什么原因：\n{text}"
    );
    // 如实说：**补不上的是图**，而那正是在线档存在的理由。
    assert!(
        text.contains("**离线档补不上的是图**"),
        "缺口那一节要说清离线档补不上的是图：\n{text}"
    );
    assert!(text.contains("这正是在线档存在的理由"), "{text}");
    // 规格的真机验收要报「网络请求数（应为 0）」——那个数报告自己印得出来。
    assert!(text.contains("**网络请求数是 0**"), "{text}");
    assert!(text.contains("媒体池"));
}

#[test]
fn 报告数得出中文离线源补上了哪几个字段各多少条() {
    // 票 06 的第三条验收：这一层到底值多少，报告要答得出——**贡献与胜出两个数**。
    let mut 现场 = 建现场();
    识别(&mut 现场);
    // 这份 infobox 六样齐全：别名、类型、开发（顿号分隔）、发行（多值块）。
    let index = 中文索引带信息框(
        "{{Infobox Game\n|中文名= 魂斗罗\n|平台= FC\n|游戏类型= ACT\n\
         |别名={\n[魂斗羅]\n[Probotector]\n}\n|开发= 科乐美、KCE东京\n\
         |发行={\n[科乐美]\n[任天堂]\n}\n|发行日期= 1988-02-09\n}}",
    );
    let 简介 = 简介表::一条(12_345, 简介原文);
    let outcome = 刮削带中文简介(&mut 现场, &index, Some(&简介));

    let 一行 = |source: &str, field: &str| {
        outcome
            .report
            .zh_fields
            .iter()
            .find(|row| row.source == source && row.field == field)
            .unwrap_or_else(|| panic!("报告里该有「{source} × {field}」这一行"))
    };
    // **两个变体撞上同一条条目**：中文名落在各自的变体锚点上。
    assert_eq!(一行("中文离线源", "标题").subjects, 2);
    assert_eq!(一行("中文离线源", "标题").values, 2);
    // 作品级那四栏落在**一个**作品锚点上；开发商与发行商一个键写了两个值就是两条。
    assert_eq!(一行("中文离线源", "简介").subjects, 1);
    assert_eq!(一行("中文离线源", "类型").values, 1);
    assert_eq!(一行("中文离线源", "开发商").values, 2, "科乐美、KCE东京");
    assert_eq!(一行("中文离线源", "发行商").values, 2, "科乐美、任天堂");
    // 别名另占一个源名，只在变体这一层说话。
    assert!(一行("中文离线源·别名", "标题").values >= 2);

    // **贡献与胜出是两回事**，报告两个都给（挂单 Q28）：开发商这一栏 DAT 各家一条都
    // 给不出，中文离线源胜出；发行商那一栏 TOSEC 在场而且排在前面，它一个都没胜出。
    assert_eq!(一行("中文离线源", "开发商").winners, 1);
    assert_eq!(
        一行("中文离线源", "发行商").winners,
        0,
        "TOSEC 认得出这个文件时，发行商那一栏轮不到中文离线源——只报贡献就是邀功",
    );

    let text = outcome.report.render_text();
    assert!(text.contains("中文离线源补上了什么"), "{text}");
    assert!(text.contains("合并之后胜出"), "{text}");
    // 这几栏补上了，缺口那一节就不该再点它们的名。
    for field in ["简介", "类型", "开发商", "发行商"] {
        assert!(
            !outcome.report.gaps.iter().any(|gap| gap == field),
            "「{field}」这一趟补上了：{:?}",
            outcome.report.gaps,
        );
    }
}

#[test]
fn 报告按平台报中文离线源的覆盖() {
    // 票 06 的第四条验收：按平台那一栏是给「老平台补不上」一个交代——**那是数据源
    // 本身浅，不是匹配算法的锅**。不按平台报，用户只看得见一个全库的百分比。
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let index = 中文索引();
    let outcome = 刮削带中文索引(&mut 现场, &index);

    let fc = outcome
        .report
        .zh_platforms
        .iter()
        .find(|row| row.platform == "FC")
        .expect("报告里该有 FC 这一行");
    // 分母是**库里这个平台的变体数**：五个变体（原版、两个汉化、挤在一个目录里的两个）。
    assert_eq!(fc.variants, 5);
    // 撞上的是两个汉化变体——它们的正题都是「魂斗罗」。原版那个是拉丁文件名，
    // 「一堆」底下那两个正题对不上，**撞不上就一个字段都不产出**。
    assert_eq!(fc.matched, 2);
    assert!((fc.rate() - 40.0).abs() < f64::EPSILON, "{}", fc.rate());
    // 合计行单独一格：`--json` 那一份里「撞上的变体数」是要被引用的一个数。
    assert_eq!(outcome.report.zh_total.matched, 2);
    assert_eq!(
        outcome.report.zh_total.variants,
        outcome
            .report
            .zh_platforms
            .iter()
            .map(|row| row.variants)
            .sum::<u64>(),
        "合计行与按平台那几行必须对得上",
    );

    let text = outcome.report.render_text();
    assert!(text.contains("中文离线源按平台的覆盖"), "{text}");
    assert!(text.contains("40.0%"), "{text}");
    assert!(
        text.contains("**老平台覆盖低多半是数据源本身浅**"),
        "{text}"
    );
}

#[test]
fn 没取过中文索引时那两节说的是索引还没取而不是摆一张全零的表() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let outcome = 刮削(&mut 现场);

    assert!(outcome.report.zh_fields.is_empty());
    assert_eq!(outcome.report.zh_total.matched, 0);
    let text = outcome.report.render_text();
    assert!(text.contains("romcat zh sync"), "{text}");
    // 几十行全零说不出任何事，只会把报告冲长。
    assert!(!text.contains("中文离线源按平台的覆盖"), "{text}");
}

#[test]
fn 一个字节都不读主库也采得出元数据() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let outcome = 不收媒体(&mut 现场);

    assert_eq!(outcome.read_bytes, 0);
    assert_eq!(outcome.new_blobs, 0);
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988")
    );
    assert!(
        现场
            .catalog
            .scraped_media("变体", 原版变体)
            .expect("读得出")
            .is_empty(),
        "关掉媒体就一份都不该进池"
    );
}

#[test]
fn 不收媒体不会把收过的媒体扔掉() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);
    let 收过的 = 现场.catalog.pool_counts().expect("数得出");
    assert_eq!(收过的.refs, 3);

    // `--no-media` 说的是「这趟不收媒体」，**不是「把收过的扔了」**。
    let outcome = 不收媒体(&mut 现场);
    assert_eq!(outcome.forgotten, 0, "本地媒体源整个没参加，谈不上作废");
    assert_eq!(现场.catalog.pool_counts().expect("数得出").refs, 3);
    assert_eq!(
        现场
            .catalog
            .scraped_media("变体", 原版变体)
            .expect("读得出")
            .len(),
        1
    );
}

fn 不收媒体(现场: &mut 现场) -> scrape::Outcome {
    let mut options =
        scrape::Options::new(Roots::single("库", 现场.dir.path()), 现场.pool_dir.path());
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
    .expect("刮削不该失败")
}

#[test]
fn 超上限与读不动是两件事() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    // 上限设成 100 字节：三份媒体全都超了。**它们文件好好的**，
    // 与「读不动」（ADR-0021 的第三态）不是同一件事，两个计数器必须分得开。
    let outcome = 刮削带上限(&mut 现场, false, Some(100));

    assert_eq!(outcome.oversized_media, 3, "三份都超上限");
    assert_eq!(outcome.unreadable_media, 0, "一份都不是读不动");
    assert_eq!(outcome.read_bytes, 0, "超上限的在打开文件之前就该拦下来");
    assert_eq!(outcome.new_blobs, 0);
    assert!(
        现场
            .catalog
            .scraped_media("变体", 原版变体)
            .expect("读得出")
            .is_empty()
    );
}

#[test]
fn 一个源不再说话时它上一轮的结论被清掉() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988")
    );

    // 把 TOSEC 从 DAT 库里撤掉再重跑识别：那个作品上不再有任何 TOSEC 条目。
    // 上一轮 TOSEC 挂上去的年份与发行商**必须跟着消失**——留着的话，它们带着一条
    // 指向已经不存在的条目的**依据**，事后复核会对不上。
    现场.repo = {
        let mut repo = DatRepo::in_memory().expect("开得出来");
        装(
            &mut repo,
            "No-Intro",
            "Nintendo - Nintendo Entertainment System",
            &[条目("Contra (Japan)", "Contra (Japan).nes", &原版())],
        );
        repo
    };
    识别(&mut 现场);
    let outcome = 刮削(&mut 现场);

    assert!(outcome.forgotten > 0, "该有「锚点 × 源」被清掉");
    assert_eq!(值(&现场, "作品", 作品, "年份", "TOSEC"), None);
    assert_eq!(值(&现场, "作品", 作品, "发行商", "TOSEC"), None);
    assert_eq!(值(&现场, "变体", 汉化变体, "汉化组", "TOSEC"), None);
    // 别的源一条都没被碰。
    assert_eq!(
        值(&现场, "作品", 作品, "标题", "No-Intro").as_deref(),
        Some("Contra")
    );
    assert_eq!(
        值(&现场, "变体", 汉化变体, "标题", "文件名").as_deref(),
        Some("魂斗罗")
    );
}

#[test]
fn 调高媒体上限之后那些媒体真的会被收进来() {
    let mut 现场 = 建现场();
    识别(&mut 现场);

    // 先用一个小得离谱的上限跑一趟：三份媒体全被挡在外面。
    let 收紧 = 刮削带上限(&mut 现场, false, Some(100));
    assert_eq!(收紧.oversized_media, 3);
    assert_eq!(现场.catalog.pool_counts().expect("数得出").refs, 0);

    // **上限放开再跑，它们必须进来。**
    // 上限改变采集的结果，所以它必须进**输入指纹**——不进的话，这一趟会一口咬定
    // 「输入没变」而整条跳过，那三份永远收不进来，只有整份 `--refresh` 才逃得掉。
    let 放开 = 刮削带上限(&mut 现场, false, None);
    assert_eq!(放开.oversized_media, 0);
    assert_eq!(放开.new_blobs, 2, "两份不同的内容");
    assert_eq!(现场.catalog.pool_counts().expect("数得出").refs, 3);

    // 反过来收紧也一样：上限降下来，此前收进来的那一份也该被挡在外面。
    let 再收紧 = 刮削带上限(&mut 现场, false, Some(100));
    assert_eq!(再收紧.oversized_media, 3);
    assert_eq!(现场.catalog.pool_counts().expect("数得出").refs, 0);
}

#[test]
fn 报告把合并真的跑了一遍() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let outcome = 刮削(&mut 现场);

    let 标题 = outcome
        .report
        .fields
        .iter()
        .find(|row| row.field == "标题")
        .expect("该有标题这一行");
    // 三个源都给了标题（No-Intro、TOSEC 各一个作品锚点，文件名给了每个变体），
    // 但**作品那一层上 No-Intro 胜出**——TOSEC 贡献了值却一条都没胜出。
    assert!(标题.sources.iter().any(|(source, _)| source == "TOSEC"));
    let 胜出: Vec<&str> = 标题
        .winners
        .iter()
        .map(|(source, _)| source.as_str())
        .collect();
    assert!(胜出.contains(&"No-Intro"), "作品那一层该是 No-Intro 胜出");
    assert!(
        !胜出.contains(&"TOSEC"),
        "TOSEC 排在 No-Intro 之后，一条都不该胜出"
    );

    // 年份只有 TOSEC 给得出，那它当然胜出——「用一个源的标题配另一个源的年份」
    // 这件事，报告自己就说得出来。
    let 年份 = outcome
        .report
        .fields
        .iter()
        .find(|row| row.field == "年份")
        .expect("该有年份这一行");
    assert_eq!(年份.winners, vec![("TOSEC".to_string(), 1)]);
    assert_eq!(年份.subjects, 1);

    assert!(
        outcome.report.unknown_sources.is_empty(),
        "内置优先级表里点名的源，这一档全都有"
    );
    assert!(outcome.report.render_text().contains("合并之后谁说了算"));
}

// ════════════════════════════════════════════════════════════════════════
// 票 05：一条**裁决**管住同一次匹配的全部字段
//
// 中文离线源撞上一条条目之后一口气产出六样：中文名、别名（变体锚点上），类型、简介、
// 开发商、发行商（作品锚点上）。它们**同生共死**——都来自同一个条目号。所以裁决的粒度
// 是「这次匹配对不对」，而不是「这个字段对不对」。
// ════════════════════════════════════════════════════════════════════════

/// 沉淀库开一份空的：这一批测试要往里落**匹配裁决**。
fn 沉淀库() -> verdict::Store {
    verdict::Store::in_memory().expect("开得出来")
}

/// 把沉淀库里那批匹配裁决摊平到变体键上——**真跑一趟走的就是这条路**。
fn 摊平(现场: &现场, store: &verdict::Store) -> scrape::zh::Rulings {
    let index = verdict::MatchIndex::load(store, "主库").expect("沉淀库读得出");
    scrape::zh::Rulings::resolve(&现场.catalog, &index, "中文离线源").expect("中立库读得出")
}

/// 对一个变体身上那一次匹配下裁决。
fn 裁(
    现场: &mut 现场,
    store: &mut verdict::Store,
    key: &str,
    entry: u32,
    accepted: bool,
) -> scrape::zh::Judged {
    scrape::zh::judge(&mut 现场.catalog, store, "主库", key, entry, accepted, None)
        .expect("裁得下去")
}

/// 一个锚点上某个源留下的全部字段值。
fn 某个源的全部值(
    现场: &现场, anchor: &str, subject: &str, source: &str
) -> Vec<String> {
    现场
        .catalog
        .scraped_values(anchor, subject)
        .expect("读得出")
        .into_iter()
        .filter(|value| value.source == source)
        .map(|value| format!("{}={}", value.field, value.value))
        .collect()
}

#[test]
fn 一条否定裁决管住同一次匹配带来的全部字段() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let index = 中文索引();
    刮削带中文索引(&mut 现场, &index);

    // 落库了才谈得上「一并失效」：变体锚点上中文名与别名，作品锚点上另外三样。
    assert_eq!(
        值(&现场, "变体", 汉化变体, "标题", "中文离线源").as_deref(),
        Some("魂斗罗")
    );
    assert!(!某个源的全部值(&现场, "变体", 汉化变体, "中文离线源·别名").is_empty());
    assert_eq!(各值(&现场, "作品", 作品, "类型", "中文离线源"), vec!["ACT"]);
    assert_eq!(
        各值(&现场, "作品", 作品, "开发商", "中文离线源"),
        vec!["Konami"]
    );

    // ── 一条裁决。**不是五条**：人只说了一句「这次撞错了」。
    let mut store = 沉淀库();
    let judged = 裁(&mut 现场, &mut store, 汉化变体, 12_345, false);

    // 一、**裁决记的是内容锚**——换台机器、改过名字之后仍然认得出（ADR-0008 的那条性质）。
    assert_eq!(judged.anchor.label(), "内容");
    assert!(judged.anchor.is_shareable());
    assert_eq!(judged.work.as_deref(), Some(作品));

    // 二、**同一次匹配带来的其余字段一并失效**：变体那两路、作品那一层，一条不留。
    assert!(某个源的全部值(&现场, "变体", 汉化变体, "中文离线源").is_empty());
    assert!(某个源的全部值(&现场, "变体", 汉化变体, "中文离线源·别名").is_empty());
    assert!(某个源的全部值(&现场, "作品", 作品, "中文离线源").is_empty());
    assert!(judged.cleared >= 4, "清掉的条数该报出来：{judged:?}");
    assert!(judged.from_variant, "这个变体自己撞的就是这一条");
    assert!(
        judged.cleared_work > 0,
        "作品那一层动过了才该报动过：{judged:?}"
    );

    // 三、**别的源产出的同名字段不受影响**——这条裁决只管这一次匹配。
    // 发行商这一栏两个源都说过话：中文离线源那条没了，TOSEC 那条一个字都没动。
    assert_eq!(
        值(&现场, "作品", 作品, "发行商", "TOSEC").as_deref(),
        Some("Konami")
    );
    assert_eq!(
        值(&现场, "作品", 作品, "标题", "TOSEC").as_deref(),
        Some("Contra")
    );
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988")
    );
    assert_eq!(
        值(&现场, "变体", 汉化变体, "标题", "文件名").as_deref(),
        Some("魂斗罗")
    );

    // 四、**这条裁决只管这一个变体**：名下另一个变体撞的是另一次匹配，一个字都没动。
    assert_eq!(
        值(&现场, "变体", 汉化变体二, "标题", "中文离线源").as_deref(),
        Some("魂斗罗")
    );

    // 五、**重跑刮削结论稳定**：不会被下一趟重新撞回错的那个。
    let rulings = 摊平(&现场, &store);
    assert_eq!(rulings.len(), 1, "内容锚该反查得回那个变体");
    刮削带裁决(&mut 现场, &index, None, &rulings);
    assert!(某个源的全部值(&现场, "变体", 汉化变体, "中文离线源").is_empty());
    assert!(!某个源的全部值(&现场, "变体", 汉化变体二, "中文离线源").is_empty());
    // 作品那一层**按剩下的变体重新数票**：另一个变体还撞着这条条目，所以那几栏回来了，
    // 而且是对的。这不是「裁决没生效」——生效的是「这个变体不再投它的票」。
    assert_eq!(各值(&现场, "作品", 作品, "类型", "中文离线源"), vec!["ACT"]);

    // 六、把名下另一个变体也否掉，作品那一层就真的一条都不剩了。
    裁(&mut 现场, &mut store, 汉化变体二, 12_345, false);
    let rulings = 摊平(&现场, &store);
    刮削带裁决(&mut 现场, &index, None, &rulings);
    assert!(某个源的全部值(&现场, "作品", 作品, "中文离线源").is_empty());
}

/// 一个作品的**标题集合**里现在有哪几条叫法，写成「源|叫法|类型」。
fn 叫法(现场: &现场) -> Vec<String> {
    现场
        .catalog
        .titles_of(作品)
        .expect("读得出")
        .into_iter()
        .map(|row| format!("{}|{}|{}", row.source, row.value, row.kind.label()))
        .collect()
}

/// 这个作品眼下的**显示标题**——详情面板与导出挑的是同一份（`title::choose`）。
fn 显示标题(现场: &现场) -> title::Chosen {
    let set = title::TitleSet {
        work: 作品.to_string(),
        entries: 现场.catalog.titles_of(作品).expect("读得出"),
    };
    title::choose(&set, &Priorities::builtin())
}

#[test]
fn 否定裁决把那条叫法从标题集合里也退出去() {
    // 就地清掉 `scrape_value` 只做了一半：中文名与别名同时是**标题集合**里的叫法，
    // 而详情面板与导出读的是 `title` 表、不是现折。集合不跟着折的话，人裁完看见的
    // **显示标题**照旧是刚被他否掉的那一条，来源依据还指着那条条目。
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let index = 中文索引();
    刮削带中文索引(&mut 现场, &index);
    // 真跑一趟的次序：`romcat scrape` 之后 `romcat titles`。
    title::run(&mut 现场.catalog, &Priorities::builtin()).expect("折得动");

    let 裁决前 = 叫法(&现场);
    assert!(
        裁决前.iter().any(|it| it.starts_with("中文离线源|魂斗罗|")),
        "{裁决前:?}"
    );
    assert!(
        裁决前.iter().any(|it| it.starts_with("中文离线源·别名|")),
        "{裁决前:?}"
    );
    assert_eq!(显示标题(&现场).display, "魂斗罗");

    // ── 一、否掉一个变体：那一行**还在**，名下另一个变体还这么叫。
    // 集合是那批值折出来的一份投影，按变体键去删会删掉别人还在背书的那一行。
    let mut store = 沉淀库();
    裁(&mut 现场, &mut store, 汉化变体, 12_345, false);
    let 只裁了一个 = 叫法(&现场);
    assert!(
        只裁了一个
            .iter()
            .any(|it| it.starts_with("中文离线源|魂斗罗|")),
        "另一个变体还这么叫，这一行该留着：{只裁了一个:?}"
    );
    let 几个变体这么叫 = 现场
        .catalog
        .titles_of(作品)
        .expect("读得出")
        .into_iter()
        .find(|row| row.source == "中文离线源")
        .expect("有这一条")
        .seen;
    assert_eq!(几个变体这么叫, 1, "背书的少了一个，`seen` 该跟着少");

    // ── 二、名下另一个变体也否掉：**没再跑 `romcat titles`**，集合这一刻就该干净。
    let judged = 裁(&mut 现场, &mut store, 汉化变体二, 12_345, false);
    let 裁决后 = 叫法(&现场);
    assert!(
        !裁决后.iter().any(|it| it.starts_with("中文离线源")),
        "被否掉的那条条目还在标题集合里：{裁决后:?}"
    );
    assert!(judged.untitled > 0, "退出去几条该报得出来：{judged:?}");

    // ── 三、**显示标题不再挑到被否掉的那一条**——这才是用户看得见的那一半。
    let 挑出来的 = 显示标题(&现场);
    assert_eq!(挑出来的.display, "魂斗罗", "文件名那条汉化组自取的名顶上来");
    assert_eq!(挑出来的.kind, Some(title::TitleKind::FanName));
    assert!(
        !挑出来的.evidence.contains("中文离线源"),
        "来源依据还指着被否掉的条目：{}",
        挑出来的.evidence
    );

    // ── 四、**裁决定下的叫法一条都不许冲掉**（`clear_titles` 的纪律）：重折是重折，
    // 不是把人说过的话一起扫了。
    assert!(
        !裁决后.iter().any(|it| it.starts_with(VERDICT)),
        "这个 fixture 本来就没有裁决来的叫法：{裁决后:?}"
    );
    assert!(
        裁决后.iter().any(|it| it.starts_with("文件名|")),
        "别的源折出来的叫法一条都不该少：{裁决后:?}"
    );
}

#[test]
fn 一条肯定裁决管住同一次匹配带来的全部字段() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let index = 中文索引();
    刮削带中文索引(&mut 现场, &index);

    let mut store = 沉淀库();
    let judged = 裁(&mut 现场, &mut store, 汉化变体, 12_345, true);
    // **肯定这一档一个字都不清**：那些值是对的，留着。
    assert_eq!(judged.cleared, 0);
    assert!(judged.fresh);

    let rulings = 摊平(&现场, &store);
    刮削带裁决(&mut 现场, &index, None, &rulings);

    // 同一次匹配带来的字段**一并定下**：两层锚点上每一条的依据都换了尾巴。
    let 盖过章 = |anchor: &str, subject: &str, source: &str| {
        let values = 现场
            .catalog
            .scraped_values(anchor, subject)
            .expect("读得出")
            .into_iter()
            .filter(|value| value.source == source)
            .collect::<Vec<_>>();
        assert!(!values.is_empty(), "{anchor} {subject} {source} 该有值");
        for value in values {
            assert!(
                romcat_core::zh::is_confirmed(&value.evidence),
                "{} 该盖上同一个章：{}",
                value.field,
                value.evidence
            );
            assert!(
                !value.evidence.contains("一律进待确认队列"),
                "{}",
                value.evidence
            );
        }
    };
    盖过章("变体", 汉化变体, "中文离线源");
    盖过章("变体", 汉化变体, "中文离线源·别名");
    盖过章("作品", 作品, "中文离线源");

    // **值本身一个字都没变**：裁决改的是「这条结论算什么」，不是它说什么。
    assert_eq!(
        值(&现场, "变体", 汉化变体, "标题", "中文离线源").as_deref(),
        Some("魂斗罗")
    );
    assert_eq!(各值(&现场, "作品", 作品, "类型", "中文离线源"), vec!["ACT"]);

    // **名下那个没裁过的变体照旧等着裁**：一条裁决只管它自己那一次匹配。
    let 另一个 = 值(&现场, "变体", 汉化变体二, "标题", "中文离线源");
    assert_eq!(另一个.as_deref(), Some("魂斗罗"));
    let 它的依据 = 现场
        .catalog
        .scraped_values("变体", 汉化变体二)
        .expect("读得出")
        .into_iter()
        .find(|value| value.source == "中文离线源")
        .expect("有这一条")
        .evidence;
    assert!(它的依据.contains("一律进待确认队列"), "{它的依据}");
}

#[test]
fn 队列看得出哪几个字段来自同一次匹配() {
    // 同一次匹配的产出散在两层锚点上，字段名不同、值不同、锚点也不同——共通的只有
    // 各自**依据**里那个条目号。看得出这件事，才裁得动「这一次匹配对不对」。
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let index = 中文索引();
    刮削带中文索引(&mut 现场, &index);

    let groups = scrape::zh::matched_groups(&现场.catalog, 汉化变体).expect("读得出");
    assert_eq!(groups.len(), 1, "只撞了一次，就只有一堆");
    let group = &groups[0];
    assert_eq!(group.entry, 12_345);
    assert!(!group.confirmed, "还没人裁过");

    // 两层锚点上的字段都在这一堆里，别名那一路也在。
    let 摘要: std::collections::BTreeSet<String> = group
        .values
        .iter()
        .map(|value| {
            format!(
                "{} {} {}",
                value.kind.label(),
                value.field.label(),
                value.source
            )
        })
        .collect();
    for 该有 in [
        "变体 标题 中文离线源",
        "变体 标题 中文离线源·别名",
        "作品 类型 中文离线源",
        "作品 开发商 中文离线源",
        "作品 发行商 中文离线源",
    ] {
        assert!(摘要.contains(该有), "{该有} 该在这一堆里：{摘要:?}");
    }
    // **别的源不混进来**：这一堆说的是「中文离线源那一次匹配」。
    assert!(
        group
            .values
            .iter()
            .all(|value| value.source.starts_with("中文离线源"))
    );

    // 裁过之后这一堆**看得出已经定下了**。
    let mut store = 沉淀库();
    裁(&mut 现场, &mut store, 汉化变体, 12_345, true);
    let rulings = 摊平(&现场, &store);
    刮削带裁决(&mut 现场, &index, None, &rulings);
    let groups = scrape::zh::matched_groups(&现场.catalog, 汉化变体).expect("读得出");
    assert!(groups[0].confirmed);
}

#[test]
fn 裁一条这个变体自己没撞上的条目不会白清作品那一层() {
    // 作品那一层的答案是**名下变体数票**数出来的，胜出的可能是别的变体撞出来的条目。
    // 拿这个变体去裁它是白裁——裁决钉在这个变体的内容上，下一趟那些变体照旧投回来。
    // 那时若还把作品那一层清了，用户看到的是「删了一次、然后什么都没变」。
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let index = 中文索引();
    刮削带中文索引(&mut 现场, &index);

    // 原版那个变体撞不上中文条目（它的正题是英文），但它与两个汉化变体同属一个作品，
    // 所以作品锚点上那几栏是**名下别人**撞出来的。
    assert!(某个源的全部值(&现场, "变体", 原版变体, "中文离线源").is_empty());
    assert_eq!(各值(&现场, "作品", 作品, "类型", "中文离线源"), vec!["ACT"]);

    let mut store = 沉淀库();
    let judged = 裁(&mut 现场, &mut store, 原版变体, 12_345, false);
    assert!(!judged.from_variant, "这个变体自己没有这条条目的产出");
    assert_eq!(judged.cleared_work, 0, "作品那一层一个字都不该动");
    assert_eq!(judged.cleared, 0);
    // 作品那一层原样还在——那几栏本来就不是这个变体撞出来的。
    assert_eq!(各值(&现场, "作品", 作品, "类型", "中文离线源"), vec!["ACT"]);

    // 队列那一侧也说得出这件事：这一堆全在作品那一层，裁它要去裁别的变体。
    let groups = scrape::zh::matched_groups(&现场.catalog, 原版变体).expect("读得出");
    assert_eq!(groups.len(), 1);
    assert!(!groups[0].from_variant);
    let 自己撞的 = scrape::zh::matched_groups(&现场.catalog, 汉化变体).expect("读得出");
    assert!(自己撞的[0].from_variant);
}

#[test]
fn 裁决记的是内容锚换台机器与改过名字之后仍然认得出() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let index = 中文索引();
    刮削带中文索引(&mut 现场, &index);

    let mut store = 沉淀库();
    裁(&mut 现场, &mut store, 汉化变体, 12_345, false);

    // 一、**换台机器**：那台机器的主库叫别的名字，路径锚一条都不算数；这一条照样认得出。
    let 别处 = verdict::MatchIndex::load(&store, "另一台机器上的主库").expect("读得出");
    assert_eq!(别处.len(), 1);
    // 导出默认不带只在本机成立的那些，而这一条带得出去。
    assert_eq!(store.export(false).expect("导得出").matches.len(), 1);

    // 二、**改过名字**：把那个变体所在的目录改名，重扫一趟——变体的键跟着变了，
    // 而裁决钉的是那份字节，摊平之后落在**新的键**上。
    let root = 现场.dir.path().to_path_buf();
    fs::rename(root.join("FC/魂斗罗汉化"), root.join("FC/魂斗罗汉化甲")).expect("改得动名字");
    let mut options = ScanOptions::named(&root, "库");
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut 现场.catalog, &options, &Handle::new()).expect("扫得动");
    识别(&mut 现场);

    let 新键 = "库/FC/魂斗罗汉化甲/魂斗罗[dwt_so 汉化].zip";
    assert!(
        现场.catalog.variant(新键).expect("读得出").is_some(),
        "改过名字之后该有这个变体"
    );
    let rulings = 摊平(&现场, &store);
    assert!(
        rulings.for_variant(新键).is_some(),
        "裁决该跟着字节走到新键上"
    );
    assert!(
        rulings.for_variant(汉化变体).is_none(),
        "老那个键已经不在库里了"
    );

    // 三、跑一趟刮削，那个变体照旧一个字段都不产出——**结论稳定**。
    刮削带裁决(&mut 现场, &index, None, &rulings);
    assert!(某个源的全部值(&现场, "变体", 新键, "中文离线源").is_empty());
}

// ════════════════════════════════════════════════════════════════════════
// 票 14：在线档与配额守护
//
// **真实凭据这一趟拿不到，也不该去申请**——ScreenScraper 的 devid 要在论坛人工审批，
// 而用户的账号与 IP 不是试验场。所以下面这些验的是**一个可复现的假服务器**：
// `CannedFetcher` 与真的走同一条路（同一个 `Fetcher` 接缝、同一道取数闸门、同一套
// 状态码处置），差别只在「字节从哪儿来」。它验得了限流、431 硬停止、闸门与续跑；
// 验不了的只有「真实凭据下的端到端」那一件，那件挂着账。
// ════════════════════════════════════════════════════════════════════════

use romcat_core::dat::CannedFetcher;
use romcat_core::scrape::online::{self, Credentials, Halt, Limits, Net};
use romcat_core::verdict;

/// `jeuInfos.php` 的落点。假服务器按**前缀**答，因为查询串里带着凭据与逐条参数。
const 查询端点: &str = "https://api.screenscraper.fr/api2/jeuInfos.php";
const 封面地址: &str = "https://www.screenscraper.fr/image.php?gameid=1&media=box-2D";

fn 凭据() -> Credentials {
    Credentials {
        dev_id: "测试".to_string(),
        dev_password: "口令".to_string(),
        soft_name: "romcat-test".to_string(),
        user: None,
        user_password: None,
    }
}

/// 不限流的一组参数：测试不该真的等在那儿。频率那一条另有专门的单元测试。
fn 宽松() -> Limits {
    Limits {
        interval: std::time::Duration::from_millis(0),
        budget: 100,
        backoff: std::time::Duration::from_millis(0),
    }
}

/// 一份 `jeuInfos` 的答复。形状照调研 §1.4 / §1.5。
fn 答复(带封面: bool) -> Vec<u8> {
    let medias = if 带封面 {
        serde_json::json!([{
            "type": "box-2D", "region": "wor", "url": 封面地址,
            "format": "png", "size": "520"
        }])
    } else {
        serde_json::json!([])
    };
    serde_json::to_vec(&serde_json::json!({
        "header": {"success": "true"},
        "response": {
            "ssuser": {
                "maxrequestsperday": "20000", "requeststoday": "7",
                "maxrequestskoperday": "500", "requestskotoday": "1",
                "maxthreads": "1"
            },
            "jeu": {
                "noms": [{"region": "wor", "text": "Contra"}],
                "synopsis": [{"langue": "zh", "text": "两个大兵闯外星人基地"}],
                "developpeur": {"id": 3, "text": "Konami 开发部"},
                "genres": [{"noms": [{"langue": "zh", "text": "动作"}]}],
                "medias": medias
            }
        }
    }))
    .expect("造得出 JSON")
}

fn 刮削在线(现场: &mut 现场, fetcher: &CannedFetcher, limits: Limits) -> scrape::Outcome {
    let options = 在线选项(现场);
    刮削在线带选项(现场, fetcher, limits, &options)
}

/// 在线档的一份默认选项。**估算与真跑收的是同一份**——两边各摆一份的话，
/// 「屏上说 0 个请求」与「按下去发了几个」就没有共同的前提了。
fn 在线选项(现场: &现场) -> scrape::Options {
    let mut options =
        scrape::Options::new(Roots::single("库", 现场.dir.path()), 现场.pool_dir.path());
    options.profile = scrape::Profile::Online;
    options
}

fn 刮削在线带选项(
    现场: &mut 现场,
    fetcher: &CannedFetcher,
    limits: Limits,
    options: &scrape::Options,
) -> scrape::Outcome {
    let cancel = CancelToken::new();
    let net = Net::new(fetcher, limits, 凭据(), &cancel);
    scrape::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &Priorities::builtin(),
        options,
        Some(&net),
        &mut scrape::RunContext {
            cancel: &cancel,
            progress: &mut |_| {},
            naming: &fuzzy::Naming::off(),
            summaries: None,
            rulings: &scrape::zh::Rulings::none(),
        },
    )
    .expect("刮削不该失败——配额超限是「停」不是「错」")
}

#[test]
fn 两档在任务级别切换而优先级表一份都不用换() {
    let mut 现场 = 建现场();
    识别(&mut 现场);

    // 第一趟离线：一个网络请求都不发。**这一趟没带中文索引**（`Naming::off`），
    // 所以简介、类型、开发商还空着——那是没撞上，不是离线档做不到（票 06）。
    let 离线 = 刮削(&mut 现场);
    assert_eq!(离线.report.profile, "离线档");
    assert!(离线.online.is_none(), "离线档不该有在线的账");
    for field in ["简介", "类型", "开发商"] {
        assert!(离线.report.gaps.iter().any(|gap| gap == field));
    }
    assert!(
        离线
            .report
            .idle_sources
            .contains(&"ScreenScraper".to_string()),
        "报告该说清「这一档没参加的源」，而不是把它报成不存在的源"
    );
    assert!(离线.report.unknown_sources.is_empty());

    // 第二趟在线：**同一份库、同一份优先级表**，只换了 `--profile`。
    let fetcher = CannedFetcher::new()
        .with_prefix(查询端点, 200, 答复(false))
        .with(封面地址, 封面());
    let 在线 = 刮削在线(&mut 现场, &fetcher, 宽松());
    assert_eq!(在线.report.profile, "在线档");
    assert!(在线.report.sources.contains(&"ScreenScraper".to_string()));

    // 这三样在线源也给得出——**它给的是英文的那一份**，而中文那一份离线档自己就有。
    assert_eq!(
        值(&现场, "作品", 作品, "简介", "ScreenScraper").as_deref(),
        Some("两个大兵闯外星人基地"),
        "**中文简介是这个库要的东西**，在线源取简介时中文排在最前"
    );
    assert_eq!(
        值(&现场, "作品", 作品, "开发商", "ScreenScraper").as_deref(),
        Some("Konami 开发部")
    );
    assert_eq!(
        值(&现场, "作品", 作品, "类型", "ScreenScraper").as_deref(),
        Some("动作")
    );

    // **离线那一趟的结论一条都没被冲掉**：三元组并存，两档共用同一份缓存。
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988")
    );
    assert_eq!(
        值(&现场, "作品", 作品, "标题", "No-Intro").as_deref(),
        Some("Contra")
    );

    // 而**字段级优先级照样可以单独覆盖**，且不必重采：标题这一栏在线源排在 DAT 之后。
    let values = 现场.catalog.scraped_values("作品", 作品).expect("读得出");
    let merged = Priorities::builtin().merge(Some("FC"), &values);
    assert_eq!(merged["标题"].source, "No-Intro");
    assert_eq!(merged["简介"].source, "ScreenScraper");
    assert_eq!(
        Priorities::load(&写一份优先级表(&现场))
            .expect("读得动")
            .merge(Some("FC"), &values)["标题"]
            .source,
        "TOSEC",
        "换一份表，标题跟着换源——库里一条值都没重采"
    );
}

#[test]
fn 在线档默认限流_并发与频率有明确上限() {
    // **默认值本身就是验收对象**：这一档的默认必须是收着的，而不是「用户不调就放开跑」。
    let 默认 = Limits::default();
    assert_eq!(online::MAX_CONCURRENCY, 1, "并发上限恒为 1，而且不给调");
    assert!(默认.interval >= std::time::Duration::from_millis(1_000));
    assert!(
        默认.budget > 0 && 默认.budget <= 1_000,
        "自设的请求上限要往小里设"
    );

    let mut 现场 = 建现场();
    识别(&mut 现场);
    let fetcher = CannedFetcher::new().with_prefix(查询端点, 200, 答复(false));
    let outcome = 刮削在线(&mut 现场, &fetcher, 宽松());

    // 报告必须把这几个数说出来——用户要在对着真账号跑之前看得见它们。
    let text = outcome.report.render_text();
    assert!(text.contains("在线那一侧的账"), "{text}");
    assert!(text.contains("并发            1"), "{text}");
    let 在线 = outcome.report.online.expect("在线档该有这一节");
    assert_eq!(在线.concurrency, 1);
    // 服务端允许几个线程也报出来——「我们比对面允许的还保守」该是看得见的事实。
    assert_eq!(在线.server_threads, Some(1));
    assert!(text.contains("服务端允许"), "{text}");
    // **服务端说的数字才作数**：三份官方文档给了三个不同的日配额。
    assert_eq!(在线.requests_left, Some(20_000 - 7));
    assert_eq!(在线.ko_left, Some(500 - 1));
}

#[test]
fn 配额超限是硬停止_不重试也不换账号() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    // 先离线跑一趟，攒下一批结论——它们必须活过下面那次硬停止。
    刮削(&mut 现场);

    // 431 是**专门为「未识别 ROM」设的**那一份配额。这个库里上万个变体在
    // ScreenScraper 眼里正是未识别 ROM，撞穿了连账号带 IP 一起永久封禁。
    let fetcher = CannedFetcher::new().with_prefix(
        查询端点,
        431,
        b"Faite du tri dans vos fichiers roms et repassez demain !".to_vec(),
    );
    let outcome = 刮削在线(&mut 现场, &fetcher, 宽松());

    let halt = outcome.halted.expect("431 该让整趟停下来");
    assert!(
        matches!(halt, Halt::Quota { .. }),
        "431 是配额超限，不是网络抖动"
    );
    assert!(halt.describe().contains("不重试也不换账号"));
    assert_eq!(
        fetcher.asked().len(),
        1,
        "**撞上 431 之后一个请求都不许再发**——重试与换账号都是永久封禁那条路"
    );

    // **已经采完的那部分留在库里。** 报告照出，退出的是这一趟不是这些结论。
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988")
    );
    assert_eq!(现场.catalog.pool_counts().expect("数得出").refs, 3);
    assert!(outcome.report.render_text().contains("这一趟停了"));
    assert!(
        值(&现场, "作品", 作品, "简介", "ScreenScraper").is_none(),
        "没问到就什么都不写——写了下一趟会整条跳过，那条结论就永远缺着"
    );
}

#[test]
fn 只对已确认的条目发请求_未识别的变体不消耗配额() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let fetcher = CannedFetcher::new().with_prefix(查询端点, 200, 答复(false));
    let outcome = 刮削在线(&mut 现场, &fetcher, 宽松());

    // 库里四个变体：两个撞上了 DAT，`FC/一堆/甲.zip` 与 `乙.zip` 一条候选都没有。
    assert_eq!(outcome.report.unconfirmed_variants, 2);
    assert_eq!(outcome.report.queryable_works, Some(1));
    // **一部作品一次查询**：两个已确认的变体属于同一部作品，合起来只发一个请求。
    // 未识别那两个一个字节都没发出去。
    assert_eq!(
        fetcher.asked().len(),
        1,
        "发过的请求：{:?}",
        fetcher.asked()
    );
    let asked = &fetcher.asked()[0];
    assert!(!asked.contains("甲"), "{asked}");
    assert!(!asked.contains("乙"), "{asked}");
    // 官方要求哈希与文件大小同发，少一样这次查询注定未命中、白扣一份配额。
    assert!(asked.contains("crc="), "{asked}");
    assert!(asked.contains("romtaille="), "{asked}");

    let text = outcome.report.render_text();
    assert!(text.contains("一个请求都没有为它们发出去"), "{text}");
}

#[test]
fn 在线结果进同一份持久化缓存_重跑不再问一遍() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let fetcher = CannedFetcher::new()
        .with_prefix(查询端点, 200, 答复(true))
        .with(封面地址, 封面());
    刮削在线(&mut 现场, &fetcher, 宽松());
    let 首趟 = fetcher.asked().len();
    assert!(首趟 > 0);

    // 第二趟：输入指纹没变，**整条跳过**。离线档省的是算力，在线档省的是配额。
    let 再来 = 刮削在线(&mut 现场, &fetcher, 宽松());
    assert_eq!(fetcher.asked().len(), 首趟, "第二趟一个请求都不该再发");
    assert!(再来.reused_probes > 0);
    assert_eq!(再来.online.expect("有在线的账").requests, 0);
    assert_eq!(
        值(&现场, "作品", 作品, "简介", "ScreenScraper").as_deref(),
        Some("两个大兵闯外星人基地")
    );
}

#[test]
fn 网络失败不影响已完成的部分_可续跑() {
    let mut 现场 = 建现场();
    识别(&mut 现场);

    // 一个什么都答不上来的服务器：在线那一对采不成，**而离线那七个源照常落库**。
    let 断网 = CannedFetcher::new();
    let 第一趟 = 刮削在线(&mut 现场, &断网, 宽松());
    assert!(第一趟.skipped > 0, "在线那一对该被记成「没采成」");
    assert!(第一趟.halted.is_none(), "一次网络错不该让整趟停下来");
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988")
    );
    assert_eq!(
        现场.catalog.pool_counts().expect("数得出").refs,
        3,
        "本地媒体照收"
    );
    assert!(值(&现场, "作品", 作品, "简介", "ScreenScraper").is_none());

    // 网回来了，**接着采**：没采成的那一对没写过采集记录，所以这一趟还会再来一次。
    let 通了 = CannedFetcher::new().with_prefix(查询端点, 200, 答复(false));
    let 第二趟 = 刮削在线(&mut 现场, &通了, 宽松());
    assert_eq!(通了.asked().len(), 1);
    assert_eq!(第二趟.skipped, 0);
    assert_eq!(
        值(&现场, "作品", 作品, "简介", "ScreenScraper").as_deref(),
        Some("两个大兵闯外星人基地")
    );
}

#[test]
fn 在线媒体进同一个媒体池且与本地媒体只存一份() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);
    let 收本地之后 = 现场.catalog.pool_counts().expect("数得出");

    // **在线源回来的封面与本地那张一模一样**——池是内容寻址的，它落在同一个文件上。
    let fetcher = CannedFetcher::new()
        .with_prefix(查询端点, 200, 答复(true))
        .with(封面地址, 封面());
    let outcome = 刮削在线(&mut 现场, &fetcher, 宽松());

    let 之后 = 现场.catalog.pool_counts().expect("数得出");
    assert_eq!(之后.blobs, 收本地之后.blobs, "同一串字节，池里仍然只有一份");
    assert_eq!(
        之后.refs,
        收本地之后.refs + 1,
        "多的是一条引用，不是一份内容"
    );
    assert_eq!(outcome.new_blobs, 0);
    assert_eq!(outcome.deduped, 1, "算出来发现池里已经有了");

    let 作品媒体 = 现场.catalog.scraped_media("作品", 作品).expect("读得出");
    assert_eq!(作品媒体.len(), 1);
    assert_eq!(作品媒体[0].source, "ScreenScraper");
    assert_eq!(作品媒体[0].kind, "封面");
    // **封面挂在作品这一层**（`CONTEXT.md` 的「作品」词条）。
    let pool = MediaPool::open(现场.pool_dir.path()).expect("开得出池");
    assert!(pool.contains(&作品媒体[0].hash, "png"));

    // 下过的不重下：URL 记在库里，第二趟连那张图都不再取。
    let 之前问过 = fetcher.asked().len();
    刮削在线(&mut 现场, &fetcher, 宽松());
    assert_eq!(fetcher.asked().len(), 之前问过);
}

#[test]
fn 响应里指向禁区的媒体地址下不来() {
    let mut 现场 = 建现场();
    识别(&mut 现场);

    // **媒体 URL 是服务器说了算的。** 一条被改过的响应把它指到 datomatic，
    // 一次请求就够触发永久 IP 封禁（ADR-0007）——闸门是唯一挡得住这件事的东西。
    let 禁区 = "https://datomatic.no-intro.org/box.png";
    let 答复 = serde_json::to_vec(&serde_json::json!({
        "response": {"jeu": {
            "noms": [{"region": "wor", "text": "Contra"}],
            "medias": [{"type": "box-2D", "region": "wor", "url": 禁区, "format": "png"}]
        }}
    }))
    .expect("造得出 JSON");
    let fetcher = CannedFetcher::new()
        .with_prefix(查询端点, 200, 答复)
        .with(禁区, 封面());
    let outcome = 刮削在线(&mut 现场, &fetcher, 宽松());

    assert_eq!(
        outcome.online.expect("在线档该有这一笔账").refused,
        1,
        "该被闸门拦下"
    );
    // **闸门拦下不是「读不动」**（ADR-0021 的第三态说的是盘），也不是「源说没有」。
    // 三件事的处置各不相同，混进同一个计数器，报告就会指着一块好好的盘说它读不动。
    assert_eq!(outcome.unreadable_media, 0, "盘好好的");
    assert_eq!(outcome.missing_media, 0, "源没说这张图不存在，是我们不许下");
    assert!(
        !fetcher.asked().iter().any(|url| url.contains("no-intro")),
        "一个字节都不该发到禁区去：{:?}",
        fetcher.asked()
    );
    assert!(outcome.halted.is_none(), "拦下一张图不该让整趟停下来");
    // 拦下是**永久**的事，所以这一对照样算采全了——标题落了库，下一趟不必重问。
    assert_eq!(
        值(&现场, "作品", 作品, "标题", "ScreenScraper").as_deref(),
        Some("Contra")
    );
    assert!(
        现场
            .catalog
            .scraped_media("作品", 作品)
            .expect("读得出")
            .is_empty()
    );
}

// ── 刮削面板的那本账（票 `gui-redesign/10`）────────────────────────────────
//
// 这一组钉的是**按下去之前那个数**。它比别处严一档，理由只有一条：**在线源赌的是
// 用户的账号与 IP**（ADR-0007）——配额同时按账号与 IP 计，撞穿了是永久封禁。
// 一个「预计 0 个请求」而按下去发了九千个的界面，比不给估算更坏。

/// 这一批变体的键，收成 [`scrape::Options::only`] 要的那个形状。
fn 范围(keys: &[&str]) -> std::collections::BTreeSet<String> {
    scrape::estimate::only(keys.iter().map(|key| (*key).to_string()))
}

#[test]
fn 只勾本地源时估算的请求数为零() {
    let mut 现场 = 建现场();
    识别(&mut 现场);

    let options = scrape::Options::new(Roots::single("库", 现场.dir.path()), 现场.pool_dir.path());
    let 账 = scrape::estimate::estimate(&现场.catalog, &options, 宽松()).expect("算得出");

    // **这不是「大概是 0」**：离线档只收自报本地的源，混进一个联网源会当场被拒
    // （`scrape::sources` 那道闸门）。所以这个 0 是构造上的，不是统计出来的。
    assert_eq!(账.requests, 0, "离线档一个网络请求都不该发");
    assert!(!账.media_downloads, "本地源收的图不花配额");
    assert!(账.anchors() > 0, "锚点数不该是 0——那说明范围根本没算出来");

    // 真跑一趟对上：离线档连那笔在线的账都没有。
    let outcome = 刮削(&mut 现场);
    assert!(outcome.online.is_none(), "离线档不该有在线那一笔账");
}

#[test]
fn 估算的请求数与在线档实际发出去的一致() {
    let mut 现场 = 建现场();
    识别(&mut 现场);

    // **不收媒体**：图有几份要查过才知道，源不说，谁也算不出来。收媒体那一档的口径是
    // 「`requests` 是下界」，由 `估算说得出媒体那一段是下界` 单独钉。
    let mut options = 在线选项(&现场);
    options.media = false;
    let 账 = scrape::estimate::estimate(&现场.catalog, &options, 宽松()).expect("算得出");
    assert!(账.requests > 0, "有已确认的作品锚点，不该一个请求都不发");
    assert!(账.over_budget.is_none(), "这点量撞不到自设上限");

    let fetcher = CannedFetcher::new().with_prefix(
        "https://api.screenscraper.fr/api2/jeuInfos.php",
        200,
        答复(false),
    );
    let outcome = 刮削在线带选项(&mut 现场, &fetcher, 宽松(), &options);
    let 实际 = outcome.online.expect("在线档该有这一笔账").requests;

    assert_eq!(
        账.requests, 实际,
        "屏上写的与真发出去的必须是同一个数——差一个都是拿用户的账号在赌",
    );

    // **第二趟补缺应当一个都不发**：判据没变，输入指纹就没变，整条跳过。
    let 二趟账 = scrape::estimate::estimate(&现场.catalog, &options, 宽松()).expect("算得出");
    assert_eq!(二趟账.requests, 0, "补缺不该为同一份判据再问一遍");
    let 二趟 = 刮削在线带选项(&mut 现场, &fetcher, 宽松(), &options);
    assert_eq!(二趟.online.expect("有账").requests, 0);

    // **重采绕过输入指纹**，于是估算与实际同时回到第一趟那个数。
    let mut 重采 = options.clone();
    重采.refresh = true;
    let 重采账 = scrape::estimate::estimate(&现场.catalog, &重采, 宽松()).expect("算得出");
    assert_eq!(重采账.requests, 账.requests, "重采该把这一批整个重问一遍");
    let 重采跑 = 刮削在线带选项(&mut 现场, &fetcher, 宽松(), &重采);
    assert_eq!(重采跑.online.expect("有账").requests, 重采账.requests);
}

#[test]
fn 范围里一个已确认的作品都没有时在线档也不发请求() {
    let mut 现场 = 建现场();
    识别(&mut 现场);

    // `FC/一堆/` 那两个 zip 一条 DAT 都没撞上——ScreenScraper 眼里它们正是
    // 「未识别 ROM」，而那份配额撞穿的处置是**连账号带 IP 永久封禁**（ADR-0007）。
    let mut options = 在线选项(&现场);
    options.only = Some(范围(&["库/FC/一堆/甲.zip", "库/FC/一堆/乙.zip"]));
    options.media = false;

    let 账 = scrape::estimate::estimate(&现场.catalog, &options, 宽松()).expect("算得出");
    assert_eq!(账.works, 0, "这两个变体一个作品都挂不上");
    assert_eq!(账.variants, 2, "范围就是这两个");
    assert_eq!(账.requests, 0);

    let fetcher = CannedFetcher::new();
    let outcome = 刮削在线带选项(&mut 现场, &fetcher, 宽松(), &options);
    assert_eq!(outcome.online.expect("有账").requests, 0);
    assert!(fetcher.asked().is_empty(), "一个请求都不该发出去");
}

#[test]
fn 估算说得出媒体那一段是下界() {
    let mut 现场 = 建现场();
    识别(&mut 现场);

    let options = 在线选项(&现场);
    let 账 = scrape::estimate::estimate(&现场.catalog, &options, 宽松()).expect("算得出");
    assert!(
        账.media_downloads,
        "收媒体的在线档要把「每份图还要各下一次」说出来",
    );

    let fetcher = CannedFetcher::new()
        .with_prefix(
            "https://api.screenscraper.fr/api2/jeuInfos.php",
            200,
            答复(true),
        )
        .with(封面地址, 封面());
    let outcome = 刮削在线带选项(&mut 现场, &fetcher, 宽松(), &options);
    let 实际 = outcome.online.expect("有账").requests;
    assert!(
        实际 > 账.requests,
        "收媒体时真发出去的会多出那几张图：预计 {} 个查询、实际 {实际} 个请求",
        账.requests,
    );
}

#[test]
fn 范围之外的变体这一趟一个字都不动() {
    let mut 现场 = 建现场();
    识别(&mut 现场);

    let mut options =
        scrape::Options::new(Roots::single("库", 现场.dir.path()), 现场.pool_dir.path());
    options.only = Some(范围(&[汉化变体]));
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

    // 范围里那个变体采到了。
    assert_eq!(
        值(&现场, "变体", 汉化变体, "汉化组", "TOSEC").as_deref(),
        Some("dwt_so"),
    );
    // **范围之外那个一条都没有**——屏上写「作用于 1 个变体」，按下去就只能动 1 个。
    assert!(
        现场
            .catalog
            .scraped_values("变体", 原版变体)
            .expect("读得出")
            .is_empty(),
        "范围之外的变体不该被采",
    );
    // **作品锚点照采**：简介与封面挂在作品这一层，漏掉它这一批一个字段都补不上。
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988"),
    );
}

#[test]
fn 重采不碰裁决与手工维护的元数据() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);

    // 人在界面上一个字一个字敲进去的那条（`library::Screen::put_value` 走的就是它）。
    现场
        .catalog
        .put_verdict_value(
            scrape::AnchorKind::Work,
            作品,
            scrape::Field::Description,
            "这是我自己写的简介",
            "浏览屏的详情面板上人工写的",
        )
        .expect("写得进");

    let 原版标题 = 值(&现场, "变体", 原版变体, "标题", "文件名").expect("文件名那一源该给得出");

    // **重采绕过输入指纹全部重来**——但重来的是采集，不是人的判断。
    刮削一趟(&mut 现场, true);
    assert_eq!(
        值(&现场, "作品", 作品, "简介", VERDICT).as_deref(),
        Some("这是我自己写的简介"),
        "重采一趟数据源不该把人手写的元数据冲掉",
    );

    // 范围收窄的那一趟同理。
    let mut options =
        scrape::Options::new(Roots::single("库", 现场.dir.path()), 现场.pool_dir.path());
    options.refresh = true;
    options.only = Some(范围(&[汉化变体]));
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
    assert_eq!(
        值(&现场, "作品", 作品, "简介", VERDICT).as_deref(),
        Some("这是我自己写的简介"),
    );
    // 范围之外那个变体上一趟采到的东西也还在：这一趟的重采只清这一批。
    assert_eq!(
        值(&现场, "变体", 原版变体, "标题", "文件名").as_deref(),
        Some(原版标题.as_str()),
    );
}

#[test]
fn 重采不收媒体时上一趟收进来的媒体引用还在() {
    // **面板默认就是「不收媒体 ＋ 补缺」，而人一按「重采」就走到这一档。**
    // 重采若先把这一批的采集记录整个清空，`media_ref` 会跟着没——而这一趟根本不收媒体，
    // 那些引用一个字节都写不回来，池里的图就成了没人引用的孤儿。
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);
    let 收过的 = 现场
        .catalog
        .scraped_media("变体", 汉化变体)
        .expect("读得出");
    assert!(!收过的.is_empty(), "第一趟该收到媒体");

    let mut options =
        scrape::Options::new(Roots::single("库", 现场.dir.path()), 现场.pool_dir.path());
    options.refresh = true;
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

    assert_eq!(
        现场
            .catalog
            .scraped_media("变体", 汉化变体)
            .expect("读得出")
            .len(),
        收过的.len(),
        "「这趟不收媒体」说的不是「把收过的扔了」",
    );
}

#[test]
fn 重采半路被按停也不会把没走到的锚点清空() {
    // **重采不是「先把库清空再重来」**，而是「不看那道输入指纹」。差别在这一条上：
    // 清空跑在采集之前，中途按停下就只剩一个空壳——而界面对停下来那一档说的是
    // 「已经采到的那些留在中立库里，再排一次接着采」。
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);

    let 停 = CancelToken::new();
    停.cancel();
    let mut options =
        scrape::Options::new(Roots::single("库", 现场.dir.path()), 现场.pool_dir.path());
    options.refresh = true;
    let outcome = scrape::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &Priorities::builtin(),
        &options,
        None,
        &mut scrape::RunContext {
            cancel: &停,
            progress: &mut |_| {},
            naming: &fuzzy::Naming::off(),
            summaries: None,
            rulings: &scrape::zh::Rulings::none(),
        },
    )
    .expect("按停了不是失败");
    assert!(outcome.interrupted, "这一趟该是被按停的");

    // 一个锚点都没走到，而上一趟采的东西一条都没少。
    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988"),
    );
    assert_eq!(
        值(&现场, "变体", 汉化变体, "汉化组", "TOSEC").as_deref(),
        Some("dwt_so"),
    );
}

#[test]
fn 字段选窄了不抹掉上一趟采到的别的字段() {
    let mut 现场 = 建现场();
    识别(&mut 现场);
    刮削(&mut 现场);
    assert_eq!(
        值(&现场, "作品", 作品, "发行商", "TOSEC").as_deref(),
        Some("Konami"),
    );

    // 只要年份跑一趟。**没有覆盖这回事**：三元组并存，这一趟只管它点名的那几个字段。
    let mut options =
        scrape::Options::new(Roots::single("库", 现场.dir.path()), 现场.pool_dir.path());
    options.fields = [scrape::Field::Year].into_iter().collect();
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

    assert_eq!(
        值(&现场, "作品", 作品, "年份", "TOSEC").as_deref(),
        Some("1988"),
        "点名要的字段该采到",
    );
    assert_eq!(
        值(&现场, "作品", 作品, "发行商", "TOSEC").as_deref(),
        Some("Konami"),
        "没点名的字段上一趟采到的值该原样留着",
    );
    // **收过的媒体也留着**：「这趟不收媒体」说的不是「把收过的扔了」。
    assert!(
        !现场
            .catalog
            .scraped_media("变体", 汉化变体)
            .expect("读得出")
            .is_empty(),
        "不收媒体不该把上一趟收进来的媒体引用删掉",
    );
}

// ══════════════════════════════════════════════════════════════════════════
// 补一条平台别名之后重建中文索引，刮削那一侧跟着重采
// ══════════════════════════════════════════════════════════════════════════

/// 缓存目录里那份 dump 原件叫什么。文件名里带着这一版的日期，**同名就是同一版**。
const 中文原件名: &str = "dump-2026-09-01.210329Z.zip";

/// 一份最小的中文离线源原件：魂斗罗那一条，**平台写的是 `任天堂红白机`**。
///
/// 这个写法平台清单那张目录别名表（`fc` / `nes` / `famicom`）与内置剥离规则里那张
/// 中文源平台别名表都不认——那正是用户遇到的场景：一个折不动的平台名。
fn 一份中文原件() -> Vec<u8> {
    let line = concat!(
        r#"{"id":12345,"type":4,"name":"魂斗羅","name_cn":"魂斗罗","#,
        r#""infobox":"{{Infobox Game\r\n|别名={\r\n[Probotector]\r\n}\r\n"#,
        r#"|平台= 任天堂红白机\r\n|游戏类型= ACT\r\n|开发= Konami\r\n|发行= Konami\r\n}}","#,
        r#""platform":4001,"summary":"　　丛林里的两个兵。","date":"1988-02-09","#,
        r#""meta_tags":["ACT","游戏"]}"#,
    );
    zip_container(&[ZipEntrySpec::stored(
        romcat_core::zh::sync::SUBJECTS,
        format!("{line}\n").into_bytes(),
    )])
}

/// `aux/latest.json` 说的正是缓存目录里手上这一版——于是取数那一趟一个字节都不下。
fn 一份中文_latest_json() -> Vec<u8> {
    format!(
        "{{\"browser_download_url\": \
         \"https://github.com/bangumi/Archive/releases/download/archive/{中文原件名}\",\
         \"digest\": \"sha256:abc\", \"name\": \"{中文原件名}\", \"size\": 1}}"
    )
    .into_bytes()
}

/// 跑一趟中文离线源取数，把索引整份读回来。
///
/// 走的是**真的那条路**：`zh sync` → 建索引时折平台 → 落库 → `Store::load`。
/// **折叠只在建索引那一刻发生**，所以这条测试必须从原件建起，手捏一份索引证不了这件事。
fn 取一趟中文数(
    store: &mut romcat_core::zh::store::Store,
    rules: &romcat_core::filename::Rules,
    cache: &Path,
    full: bool,
) -> romcat_core::zh::Index {
    let fetcher = romcat_core::dat::CannedFetcher::new()
        .with(romcat_core::zh::sync::LATEST_URL, 一份中文_latest_json());
    romcat_core::zh::sync::sync(
        &fetcher,
        &RealFs::new(),
        store,
        &romcat_core::platform::Manifest::builtin(),
        rules,
        &romcat_core::zh::sync::Options {
            cache: cache.to_path_buf(),
            full,
            dry_run: false,
        },
        &mut romcat_core::zh::sync::Context::unattended(),
    )
    .expect("取数不该失败");
    store.load().expect("索引读得回来")
}

#[test]
fn 补一条平台别名重建索引之后刮削重采而不是整片复用旧记录() {
    // **这一条是这次修复的正题，照用户那几步一步一步走。**
    //
    // 中文索引在**建索引时**把条目的平台名折成本工具的平台名，折出来的那一串决定
    // 交叉校验、进而决定这个源说不说得出话。用户遇到一个折不动的平台名，照文档补一条
    // 别名、`zh sync --full` 重建索引，再跑 `scrape`——从前是整片复用旧的采集记录、
    // 一条新产出都没有，因为两层锚点的输入指纹盖的是 dump 名与取了哪几样字段，
    // 而重建前后这两样一个字都没变。
    let mut 现场 = 建现场();
    识别(&mut 现场);

    let 工作区 = temp_dir("zh-fold");
    let cache = 工作区.path().join("cache");
    fs::create_dir_all(&cache).expect("建得出缓存目录");
    写(&cache.join(中文原件名), &一份中文原件());
    let mut store =
        romcat_core::zh::store::Store::open(&工作区.path().join("zh.sqlite3")).expect("开得起来");

    // ── 一、内置那两张表折不动 `任天堂红白机`：条目身上那一串是空的。
    let 折不动 = 取一趟中文数(
        &mut store,
        &romcat_core::filename::Rules::builtin(),
        &cache,
        false,
    );
    assert_eq!(折不动.len(), 1, "索引里就这一条");
    assert!(
        折不动.entries()[0].platforms.is_empty(),
        "两张表都不认 `任天堂红白机`——**认不出就留空**，硬折一个平台出来会让交叉校验说谎"
    );
    assert_eq!(
        折不动.entries()[0].platform_text,
        "任天堂红白机",
        "原文原样留着：人去核对时看的是它"
    );

    let 首趟 = 刮削带中文索引(&mut 现场, &折不动);
    assert!(
        值(&现场, "变体", 汉化变体, "标题", "中文离线源").is_none(),
        "平台折不动，交叉校验说不出话，够不着中置信那一档——这个源无话可说"
    );

    // ── 二、照文档补一条平台别名，`zh sync --full` 重建索引。
    let 规则文件 = 工作区.path().join("rules.toml");
    fs::write(
        &规则文件,
        "\"版本\" = 1\n[[\"中文源平台别名\"]]\n\"叫\" = \"任天堂红白机\"\n\"是\" = \"FC\"\n",
    )
    .expect("写得下规则");
    let 补过的规则 = romcat_core::filename::Rules::load(&规则文件).expect("读得进来");
    let 折得动 = 取一趟中文数(&mut store, &补过的规则, &cache, true);
    assert_eq!(
        折得动.entries()[0].platforms,
        vec!["FC".to_string()],
        "补上那条别名之后折得动了"
    );
    // **从前两层指纹就是靠这两样算的，而它们一个字都没变**——这正是那个洞。
    assert_eq!(折不动.dump(), 折得动.dump(), "同一版 dump");
    assert_eq!(折不动.fields(), 折得动.fields(), "取的字段一样");
    assert_ne!(
        折不动.platform_fold(),
        折得动.platform_fold(),
        "变的只有建索引那一刻折出来的那张表"
    );

    // ── 三、再跑一趟刮削：新折得动的那条真的产出来了。
    let 第二趟 = 刮削带中文索引(&mut 现场, &折得动);
    assert_eq!(
        值(&现场, "变体", 汉化变体, "标题", "中文离线源").as_deref(),
        Some("魂斗罗"),
        "重建之后这一条该重采出来，而不是被缓存一口咬定「输入没变」而整条跳过"
    );
    assert_eq!(
        值(&现场, "作品", 作品, "类型", "中文离线源").as_deref(),
        Some("ACT"),
        "作品那一层同样跟着重采"
    );

    // ── 四、**不是全片复用**：第二趟真的重算了几个锚点。
    //
    // 索引再没变的第三趟才把它们也跳过，于是复用数比第二趟多——这一条要拦的是
    // 「一条都没重采」。别的源（TOSEC、文件名那几路）输入确实没变，照旧跳过是对的。
    let 第三趟 = 刮削带中文索引(&mut 现场, &折得动);
    assert!(
        第三趟.reused_probes > 第二趟.reused_probes,
        "第二趟该有锚点因为折出来的那张表变了而重采：首趟 {}、第二趟 {}、第三趟 {}",
        首趟.reused_probes,
        第二趟.reused_probes,
        第三趟.reused_probes
    );
}

/// 两条条目的中文索引：`魂斗罗` 与 `斗罗大陆`。
///
/// 一个文件名剥成哪个**正题**，决定它撞上哪一条——这正是剥离规则说了算的那件事。
fn 两条条目的中文索引() -> romcat_core::zh::Index {
    let mut entries = 中文索引().entries().to_vec();
    entries.push(romcat_core::zh::Entry {
        id: 54_321,
        name: "斗罗".to_string(),
        name_cn: "斗罗大陆".to_string(),
        aliases: Vec::new(),
        year: Some(1988),
        platforms: vec!["FC".to_string()],
        platform_text: "FC".to_string(),
        summary: String::new(),
        genres: vec!["RPG".to_string()],
        developers: vec!["另一家".to_string()],
        publishers: vec!["另一家".to_string()],
    });
    romcat_core::zh::Index::build(entries, "dump-2026-09-01".to_string())
}

#[test]
fn 补一条剥离规则之后刮削重采而不是整片复用旧记录() {
    // **这一条是票 04 的正题，照用户那几步一步一步走。**
    //
    // 剥离规则决定从文件名里剥出什么**正题**，而中文离线源撞的正是正题。用户遇到一批
    // 剥不干净的名字，照文档往 `--name-rules` 那份文件里补一条，再跑一趟 `scrape`
    // ——从前是整片复用旧的采集记录、什么都不重采，因为两层锚点的输入指纹盖的是 dump、
    // 取了哪几样字段、建索引那一刻折出来的那张表与匹配参数，唯独没盖规则本身。
    // 而这一趟那几样一个字都没变：**同一份索引，只有规则换了**。
    let mut 现场 = 建现场();
    识别(&mut 现场);
    let index = 两条条目的中文索引();

    // ── 一、内置那份规则剥出来的正题是 `魂斗罗`，撞上 `魂斗罗` 那条。
    let 内置 = romcat_core::filename::Rules::builtin();
    let 首趟 = 刮削带剥离规则(&mut 现场, &index, &内置);
    assert_eq!(
        各值(&现场, "变体", 汉化变体, "标题", "中文离线源"),
        vec!["魂斗罗".to_string()]
    );
    assert_eq!(
        值(&现场, "作品", 作品, "类型", "中文离线源").as_deref(),
        Some("ACT"),
        "作品那四栏跟着同一次匹配走"
    );

    // ── 二、补一条正题噪音词：正题从 `魂斗罗` 变成 `斗罗`，撞上的是另一条条目。
    let 工作区 = temp_dir("zh-name-rules");
    let 规则文件 = 工作区.path().join("rules.toml");
    fs::write(&规则文件, "\"版本\" = 1\n\"正题噪音词\" = [\"魂\"]\n").expect("写得下规则");
    let 补过的规则 = romcat_core::filename::Rules::load(&规则文件).expect("读得进来");
    // **从前两层指纹就是靠这几样算的，而它们一个字都没变**——这正是那个洞。
    assert_eq!(
        内置.parse("魂斗罗[dwt_so 汉化].zip").title,
        "魂斗罗",
        "内置那份剥出来是这个"
    );
    assert_eq!(
        补过的规则.parse("魂斗罗[dwt_so 汉化].zip").title,
        "斗罗",
        "补一条之后剥出来是另一个正题"
    );

    // ── 三、再跑一趟刮削：撞上过的那两个锚点真的重采了，撞出来的是另一条条目。
    let 第二趟 = 刮削带剥离规则(&mut 现场, &index, &补过的规则);
    assert_eq!(
        各值(&现场, "变体", 汉化变体, "标题", "中文离线源"),
        vec!["斗罗大陆".to_string()],
        "换了规则该重采出另一条中文名，而不是被缓存一口咬定「输入没变」而整条跳过"
    );
    assert_eq!(
        值(&现场, "作品", 作品, "类型", "中文离线源").as_deref(),
        Some("RPG"),
        "作品那一层同样跟着重采"
    );

    // ── 四、**规则没改时照旧整片跳过**：加这道判据不让每一趟刮削都从头来。
    //
    // 第三趟规则一个字没改，于是复用数比第二趟多——这一条既拦「一条都没重采」，
    // 也拦「每一趟都从头来」。
    let 第三趟 = 刮削带剥离规则(&mut 现场, &index, &补过的规则);
    assert!(
        第三趟.reused_probes > 第二趟.reused_probes,
        "第二趟该有锚点因为规则变了而重采、第三趟该整片跳过：首趟 {}、第二趟 {}、第三趟 {}",
        首趟.reused_probes,
        第二趟.reused_probes,
        第三趟.reused_probes
    );
}
