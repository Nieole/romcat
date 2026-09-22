//! **收藏与合集**这条接缝（票 `gui-redesign/06`）：从「勾一批按一下星」到
//! 「删掉中立库重扫、改名、挪到另一个根之后它还在」。
//!
//! 要证的是票据那几条要害：
//!
//! 1. **同一套成员关系**——收藏是名字定死的那一组合集，所以 `收藏=是` 与 `合集=收藏`
//!    选出来的是同一批，自建合集走的是同一张表、同一套函数。
//! 2. **落沉淀库、锚在内容上**——中立库整份删掉重扫，收藏一条都不少；文件改了名、
//!    挪到另一个根上，照样认得出。
//! 3. **拿不到内容锚的如实标出来**——**无判据**那些只钉得住本机的位置，
//!    挪了位置就会飘。这一条**必须真的飘给它看**，不能只在文档里写一句。
//! 4. **中立库里的合集是投影**——识别跑完照沉淀库重建一遍，与直接写投影的结果一致。
//!
//! fixture 里三个变体各自代表一档：两个 zip（**透明容器**，判据零解压就有，
//! 拿得到内容锚），一个裸文件而且这一趟不许回盘读（**无判据**，只剩路径锚）。
//! 真库里那一档有 2,631 个（`docs/library-facts.md`）。

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use romcat_core::catalog::browse::{MAX_PAGE, VariantQuery};
use romcat_core::catalog::{Catalog, Roots};
use romcat_core::collection::{self, CollectionError, FAVORITE};
use romcat_core::dat::repo::DatRepo;
use romcat_core::fs::RealFs;
use romcat_core::identify::{self, Options, fuzzy};
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::site::Site;
use romcat_core::sublibrary::{self, Rule, Selection};
use romcat_core::task::{Cutoff, Halted, Handle};
use romcat_core::testing::container::{ZipEntrySpec, zip_container};
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::verdict::{self, ANCHOR_CONTENT, ANCHOR_PATH, Anchor, Store};

/// 这份主库的**主库标识**：**路径锚**里记的就是它。
const 主库标识: &str = "小库";
/// 主库那一组根里，头一个叫什么。
const 主根: &str = "主盘";
/// 第二个根：「挪到另一个根之后还认得出」要它。
const 副根: &str = "备份盘";

const 马里奥: &str = "主盘/FC/超级马里奥.zip";
const 勇者: &str = "主盘/FC/勇者斗恶龙 汉化.zip";
const 裸卡带: &str = "主盘/FC/没判据的那一份.nes";

fn 卡带(fill: u8) -> Vec<u8> {
    let mut data = vec![0u8; 16];
    data[..4].copy_from_slice(b"NES\x1A");
    data.extend(std::iter::repeat_n(fill, 40_960));
    data
}

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

/// 摆好主库的字节。**只有这一处知道文件长什么样**，重扫那几条测试照它重来一遍。
fn 摆好主库(root: &Path) {
    写(
        &root.join("FC/超级马里奥.zip"),
        &zip_container(&[ZipEntrySpec::stored("Super Mario (Japan).nes", 卡带(0xA1))]),
    );
    写(
        &root.join("FC/勇者斗恶龙 汉化.zip"),
        &zip_container(&[ZipEntrySpec::stored("rom.nes", 卡带(0xB1))]),
    );
    // **裸文件**：下面识别时不许回盘读，于是它一条判据都拿不到——那正是真库里
    // 「无判据」那一档的样子。
    写(&root.join("FC/没判据的那一份.nes"), &卡带(0xC1));
}

struct 现场 {
    dir: TempDir,
    副: TempDir,
    site: Site,
    repo: DatRepo,
}

fn 建现场() -> 现场 {
    let dir = temp_dir("collection");
    let 副 = temp_dir("collection-副");
    摆好主库(dir.path());
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    扫(&mut catalog, 主根, dir.path());
    现场 {
        dir,
        副,
        site: Site::in_memory(catalog, Store::in_memory().expect("开得出沉淀库"), 主库标识),
        repo: DatRepo::in_memory().expect("开得出 DAT 库"),
    }
}

fn 扫(catalog: &mut Catalog, name: &str, root: &Path) {
    let mut options = ScanOptions::named(root, name);
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), catalog, &options, &Handle::new()).expect("扫得动");
}

/// 跑一趟识别。**不许回盘读**：裸文件因此一条判据都拿不到，只剩路径锚。
///
/// 合集的投影就在这一趟的末尾（`identify::run`），所以「重扫之后收藏还在」
/// 验的正是这一句跑完之后的样子。
fn 跑识别(现场: &mut 现场) -> identify::Outcome {
    let roots = Roots::single(主根, 现场.dir.path());
    跑识别在(现场, roots)
}

/// 同上，但主库摆在这一组根上。「挪到另一个根」那条要它。
fn 跑识别在(现场: &mut 现场, roots: Roots) -> identify::Outcome {
    let index = verdict::Index::load(&现场.site.store, 主库标识).expect("读得出沉淀库");
    let mut options = Options::new(roots);
    options.read_library = false;
    identify::run(
        &RealFs::new(),
        &mut 现场.site.catalog,
        &identify::Ammo {
            repo: &现场.repo,
            verdicts: &index,
            naming: &fuzzy::Naming::off(),
            guessing: &identify::model::Guessing::off(),
            titledb: None,
        },
        &options,
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别不该失败")
}

/// 一条规则在**中立库**里筛出哪几个变体的键。
fn 库里筛(catalog: &Catalog, text: &str) -> BTreeSet<String> {
    let rule = Rule::parse(text).unwrap_or_else(|error| panic!("「{text}」读不懂：{error}"));
    let query = VariantQuery {
        rule: Some(rule),
        ..VariantQuery::default()
    };
    catalog
        .variant_page(&query, 0, MAX_PAGE)
        .expect("取得出一页")
        .into_iter()
        .map(|row| row.key)
        .collect()
}

/// 同一条规则在**内存**那一侧选出哪几个。两处必须同一批。
fn 选择集选(catalog: &Catalog, text: &str) -> BTreeSet<String> {
    let rule = Rule::parse(text).unwrap_or_else(|error| panic!("「{text}」读不懂：{error}"));
    let facts = sublibrary::facts(catalog).expect("事实折得出来");
    sublibrary::select(
        &Selection {
            rules: vec![rule],
            exceptions: Vec::new(),
        },
        &facts,
    )
    .picked
    .into_iter()
    .map(|picked| picked.key)
    .collect()
}

/// 两个求值器各跑一遍，比两批键。
fn 筛(catalog: &Catalog, text: &str) -> BTreeSet<String> {
    let 库 = 库里筛(catalog, text);
    assert_eq!(
        库,
        选择集选(catalog, text),
        "「{text}」在中立库里筛出来的与选择集选出来的不是同一批",
    );
    库
}

fn 键(keys: &[&str]) -> Vec<String> {
    keys.iter().map(|key| (*key).to_string()).collect()
}

#[test]
fn 勾一批按一下星_屏上当场筛得出来而且两种锚各数各的() {
    let mut 现场 = 建现场();
    跑识别(&mut 现场);

    let applied = collection::add(&mut 现场.site, FAVORITE, &键(&[马里奥, 勇者, 裸卡带]))
        .expect("加得进收藏");
    // **两种锚各数一个数**（验收第 7 条）：屏上要写得出「其中 N 个挪了位置会飘」。
    assert_eq!(
        (
            applied.content,
            applied.path,
            applied.changed,
            applied.missing
        ),
        (2, 1, 3, 0),
        "两个 zip 拿得到内容判据，裸文件这一趟拿不到——那一个只钉得住本机路径",
    );

    // **屏上当场生效**：不必等下一趟识别，投影已经跟着改了。
    assert_eq!(
        筛(&现场.site.catalog, "收藏=是"),
        BTreeSet::from_iter(键(&[马里奥, 勇者, 裸卡带])),
    );
    // **收藏就是那个名字定死的合集**——两条规则一个字都不差。
    assert_eq!(
        筛(&现场.site.catalog, "合集=收藏"),
        筛(&现场.site.catalog, "收藏=是"),
    );
    // `收藏=否` 是补集不是空集：这一维是个是非题，两档都是值。
    assert!(筛(&现场.site.catalog, "收藏=否").is_empty());

    // 再按一次不攒出第二条：成员关系是个是非题。
    let 再来 = collection::add(&mut 现场.site, FAVORITE, &键(&[马里奥])).expect("加得进");
    assert_eq!((再来.touched(), 再来.changed), (1, 0));

    // 取消：一批一起拿出来，`收藏=是` 当场空掉。
    let 取消 = collection::remove(&mut 现场.site, FAVORITE, &键(&[马里奥, 勇者, 裸卡带]))
        .expect("拿得出来");
    assert_eq!(取消.changed, 3);
    assert!(筛(&现场.site.catalog, "收藏=是").is_empty());
    assert!(现场.site.store.memberships().expect("读得到").is_empty());
}

#[test]
fn 收藏落沉淀库_锚是内容锚_无判据的那个如实退成路径锚() {
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    collection::add(&mut 现场.site, FAVORITE, &键(&[马里奥, 裸卡带])).expect("加得进");

    // **落的是沉淀库**，不是中立库——中立库那份是投影。
    let mut 成员 = 现场.site.store.memberships().expect("读得到");
    成员.sort_by(|a, b| a.anchor.cmp(&b.anchor));
    assert_eq!(成员.len(), 2);
    assert!(
        成员.iter().all(|one| one.name == FAVORITE),
        "收藏就是名字定死的那一组合集",
    );
    let 内容锚 = 成员
        .iter()
        .filter(|one| matches!(one.anchor, Anchor::Content { .. }))
        .count();
    assert_eq!(内容锚, 1, "拿得到判据的那一个钉在内容上");
    assert!(
        成员.iter().any(|one| matches!(
            &one.anchor,
            Anchor::Path { library, variant_key } if library == 主库标识 && variant_key == 裸卡带
        )),
        "无判据的那一个如实退成路径锚，而且记着这是哪份主库的哪个变体",
    );

    // **界面照它写那句「会飘」**：`standing` 把两种锚分开说，不含糊成「在收藏里」。
    assert_eq!(
        collection::standing(&现场.site, 马里奥).expect("问得出"),
        vec![(FAVORITE.to_string(), ANCHOR_CONTENT)],
    );
    assert_eq!(
        collection::standing(&现场.site, 裸卡带).expect("问得出"),
        vec![(FAVORITE.to_string(), ANCHOR_PATH)],
    );
    assert!(
        collection::standing(&现场.site, 勇者)
            .expect("问得出")
            .is_empty(),
    );
}

#[test]
fn 删掉中立库重扫一遍_收藏一条都不少() {
    // 票据那句「收藏住沉淀库不住中立库」真正要兑现的地方：中立库整份丢掉、
    // 从零扫一遍、再识别一趟，收藏照沉淀库重建回来。
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    collection::add(&mut 现场.site, FAVORITE, &键(&[马里奥, 勇者, 裸卡带])).expect("加得进");
    collection::add(&mut 现场.site, "通关过的", &键(&[马里奥])).expect("加得进");

    // **删掉中立库**：换一份全新的，一个字节都不从旧的那份带过来。
    现场.site.catalog = Catalog::open_in_memory().expect("能开中立库");
    assert!(
        筛(&现场.site.catalog, "收藏=是").is_empty(),
        "新库里本来什么都没有",
    );
    扫(&mut 现场.site.catalog, 主根, 现场.dir.path());
    let outcome = 跑识别(&mut 现场);

    assert_eq!(
        筛(&现场.site.catalog, "收藏=是"),
        BTreeSet::from_iter(键(&[马里奥, 勇者, 裸卡带])),
        "重扫之后收藏一条都不少（路径锚那一个的键没变，所以它也回来了）",
    );
    assert_eq!(
        筛(&现场.site.catalog, "合集=通关过的"),
        BTreeSet::from_iter(键(&[马里奥])),
    );
    // 识别那一趟自己说得出它重建了什么——**投影是识别的产出之一**。
    assert_eq!(
        (outcome.collections.collections, outcome.collections.members),
        (2, 4),
    );
    assert_eq!(outcome.collections.unresolved, 0);
}

#[test]
fn 改了名或挪到另一个根_内容锚认得出而路径锚如实地飘了() {
    // **这条测试的正题是那半句诚实话。** 内容锚换个名字、换块盘都认得出；
    // 路径锚不认得——而票据要求把这件事说出来，不是含糊过去。
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    collection::add(&mut 现场.site, FAVORITE, &键(&[马里奥, 勇者, 裸卡带])).expect("加得进");

    // 主盘上：马里奥**改个名**；勇者**挪到另一个根上**；裸卡带也改个名。
    let 主 = 现场.dir.path().to_path_buf();
    let 副 = 现场.副.path().to_path_buf();
    fs::rename(
        主.join("FC/超级马里奥.zip"),
        主.join("FC/马里奥 改过名.zip"),
    )
    .expect("改得了名");
    fs::create_dir_all(副.join("FC")).expect("建得出目录");
    fs::rename(
        主.join("FC/勇者斗恶龙 汉化.zip"),
        副.join("FC/勇者斗恶龙 汉化.zip"),
    )
    .expect("挪得动");
    fs::rename(
        主.join("FC/没判据的那一份.nes"),
        主.join("FC/没判据的那一份 改过名.nes"),
    )
    .expect("改得了名");

    现场.site.catalog = Catalog::open_in_memory().expect("能开中立库");
    扫(&mut 现场.site.catalog, 主根, &主);
    扫(&mut 现场.site.catalog, 副根, &副);
    let mut roots = Roots::single(主根, &主);
    roots.set(副根, &副);
    let outcome = 跑识别在(&mut 现场, roots);

    assert_eq!(
        筛(&现场.site.catalog, "收藏=是"),
        BTreeSet::from([
            "主盘/FC/马里奥 改过名.zip".to_string(),
            "备份盘/FC/勇者斗恶龙 汉化.zip".to_string(),
        ]),
        "改了名、挪到另一个根，内容锚照样认得出；路径锚那一条如实地飘了",
    );
    // **飘掉的那一条一个字都没删**：沉淀库不可再生，落不了地不等于不该留着。
    assert_eq!(现场.site.store.memberships().expect("读得到").len(), 3);
    assert_eq!(
        outcome.collections.unresolved, 1,
        "识别那一趟点名说有一条落不了地，而不是静静少掉一颗星",
    );
}

#[test]
fn 中立库里的合集是投影_照沉淀库重建一遍结果一致() {
    // 验收第 8 条。**投影可以整份丢掉重建**：手动往投影里塞进去的东西、以及沉淀库里
    // 删掉的那些，重建之后一律照沉淀库说了算。
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    collection::add(&mut 现场.site, "送朋友的", &键(&[马里奥, 勇者])).expect("加得进");
    let 重建前 = 筛(&现场.site.catalog, "合集=送朋友的");

    // 往投影里手塞一个沉淀库里没有的合集，再把一个真成员从投影里抠掉。
    let 冒牌 = 现场
        .site
        .catalog
        .add_collection("冒牌合集")
        .expect("建得出");
    现场
        .site
        .catalog
        .add_to_collection(冒牌, 裸卡带)
        .expect("写得进");
    现场
        .site
        .catalog
        .remove_from_collection("送朋友的", 勇者)
        .expect("抠得掉");
    assert_ne!(筛(&现场.site.catalog, "合集=送朋友的"), 重建前);

    let 成员 = 现场.site.store.memberships().expect("读得到");
    let projected = collection::project(&mut 现场.site.catalog, &成员).expect("投影得出来");

    assert_eq!(
        筛(&现场.site.catalog, "合集=送朋友的"),
        重建前,
        "照沉淀库回来了"
    );
    assert!(
        筛(&现场.site.catalog, "合集=冒牌合集").is_empty(),
        "沉淀库里没有的那个合集，重建之后一条都不剩",
    );
    assert_eq!((projected.collections, projected.members), (1, 2));
    // 一条成员都不剩的合集不该留在筛选栏那一维里——那正是挂账 D74 说的「合集 0 个」。
    assert!(
        现场
            .site
            .catalog
            .facets(romcat_core::catalog::NonGameAssets::Hidden)
            .expect("问得出")
            .collections
            .iter()
            .all(|facet| facet.count > 0),
        "投影里不该有一个一件东西都选不出来的合集",
    );
}

#[test]
fn 同一份内容存了两处_一处点星两处一起亮_而且重扫之后还是这个样子() {
    // **内容锚钉的是「世上这份内容」，不是「本机这一个变体」。** 同一份内容在本机存了
    // 两处（两块盘各一份，`report::duplicates` 专门数这个）时，一条成员关系落在两个
    // 变体上——这是内容锚的定义，不是 bug。
    //
    // 真正要钉的是**增量与整份重建给出同一个样子**：屏上按下去那一下只知道人点了哪一个，
    // 要是它只点亮那一个，下一趟识别照沉淀库重建时另一个会凭空也亮起来；
    // 取消那一下同理——只熄一个的话，屏上还亮着而沉淀库里已经没了。
    let mut 现场 = 建现场();
    let 备份 = "主盘/FC/超级马里奥 备份.zip";
    写(
        &现场.dir.path().join("FC/超级马里奥 备份.zip"),
        &zip_container(&[ZipEntrySpec::stored("Super Mario (Japan).nes", 卡带(0xA1))]),
    );
    扫(&mut 现场.site.catalog, 主根, 现场.dir.path());
    跑识别(&mut 现场);

    // 只点**一个**。
    let applied = collection::add(&mut 现场.site, FAVORITE, &键(&[马里奥])).expect("加得进");
    assert_eq!(applied.content, 1, "人点的是一个变体");
    assert_eq!(
        现场.site.store.memberships().expect("读得到").len(),
        1,
        "沉淀库里也只有一条——那一条说的是「这份内容」",
    );
    assert_eq!(
        筛(&现场.site.catalog, "收藏=是"),
        BTreeSet::from_iter(键(&[马里奥, 备份])),
        "同一份内容的两处一起亮",
    );

    // **重扫一遍，样子不变**：增量那条路与整份重建给的是同一个展开。
    let 重扫前 = 筛(&现场.site.catalog, "收藏=是");
    现场.site.catalog = Catalog::open_in_memory().expect("能开中立库");
    扫(&mut 现场.site.catalog, 主根, 现场.dir.path());
    跑识别(&mut 现场);
    assert_eq!(筛(&现场.site.catalog, "收藏=是"), 重扫前);

    // 取消那一下同理：只点一个，两处一起熄。
    collection::remove(&mut 现场.site, FAVORITE, &键(&[备份])).expect("拿得出");
    assert!(筛(&现场.site.catalog, "收藏=是").is_empty());
    assert!(现场.site.store.memberships().expect("读得到").is_empty());
}

#[test]
fn 取消时两种锚都拿_识别之前放进去的那一条也拿得掉() {
    // **只拿一种的话星星点不灭。** 一个变体可能是识别之前（拿不到判据时）按**路径锚**
    // 放进去的，之后判据算出来了、它眼下该钉的是**内容锚**——取消那一下要是只按眼下
    // 这一种去删，库里那条旧的原样留着，重扫一趟它又亮回来了。
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    // 手工摆一条**路径锚**的成员关系（模拟识别之前放进去的那一条）。
    现场
        .site
        .store
        .join(&[verdict::Membership::now(
            FAVORITE,
            Anchor::Path {
                library: 主库标识.to_string(),
                variant_key: 马里奥.to_string(),
            },
        )])
        .expect("放得进");
    // 再按一次星：这一下钉的是内容锚。于是同一个变体上两条成员关系都在。
    collection::add(&mut 现场.site, FAVORITE, &键(&[马里奥])).expect("加得进");
    assert_eq!(现场.site.store.memberships().expect("读得到").len(), 2);
    assert_eq!(
        collection::standing(&现场.site, 马里奥).expect("问得出"),
        vec![(FAVORITE.to_string(), ANCHOR_CONTENT)],
        "两种锚都有时报内容锚那一种——认得出改名的那条成立，整条就认得出改名",
    );

    let 取消 = collection::remove(&mut 现场.site, FAVORITE, &键(&[马里奥])).expect("拿得出");
    assert_eq!(取消.changed, 2, "两条成员关系都该拿掉");
    assert!(现场.site.store.memberships().expect("读得到").is_empty());
    assert!(筛(&现场.site.catalog, "收藏=是").is_empty());
}

#[test]
fn 合集没名字当场拒绝() {
    // 一组东西没有名字，日后既指不着它也筛不出它。**建出来再说**是最坏的一条路。
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let error = collection::add(&mut 现场.site, "  ", &键(&[马里奥])).expect_err("该拒绝");
    assert!(format!("{error}").contains("名字"), "{error}");
}

#[test]
fn 批量取判据与一条一条问_答的是同一份() {
    // 票 `parking-3/09` 那条硬要求：**单条那份是批量那份的特例**。
    // 「谁代表这个变体」是唯一的一句说法（`identify::representative`）——写两遍迟早漂开，
    // 而漂开的后果正是这份 fixture 摆着的东西：同一个变体上的收藏与裁决钉在不同的锚上。
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let 变体 = 现场.site.catalog.variants().expect("列得出变体");
    assert_eq!(变体.len(), 3, "三个变体各占一档");

    let 整批 = identify::content_prints(&现场.site.catalog, &变体).expect("整批取得出判据");
    for row in &变体 {
        let 一条 = identify::content_print(&现场.site.catalog, row).expect("一条一条也取得出");
        assert_eq!(
            整批.get(&row.key),
            一条.as_ref(),
            "{} 两条路答出来的代表成员不是同一份",
            row.key,
        );
    }
    // **拿不到判据的那一个整批里没有它**：`None` 不许被折成一条空判据——
    // 那会让「无判据」那一档凭空得到一个内容锚。
    assert!(!整批.contains_key(裸卡带));
    assert_eq!(整批.len(), 2, "两个 zip 拿得到判据，裸卡带这一趟拿不到");
}

#[test]
fn 整批收藏的读那一半停得下来_而且停下时一个字都没写() {
    // 票 `parking-3/09`：这一下搬上**任务台**之后要按得停。读那一半整条只读，
    // 所以停在哪儿都是干净的——那是[收场](romcat_core::task::Ending)四档里的
    // 「停了，什么都没留下」，不是**停在半路**。
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let 把手 = Handle::new();
    把手.stop();
    let 错 = collection::plan(
        &现场.site.catalog,
        主库标识,
        FAVORITE,
        &键(&[马里奥, 勇者, 裸卡带]),
        true,
        &把手,
    )
    .expect_err("按过停下就该停");
    // **它得落在「被按停」那一支上**：任务台按支分「停了」与「失败」
    // （`task::Board::settle`），不看这句话说了什么。折过去照旧是 `Cutoff::Halted`
    // ——那一支一个字都不带，于是这儿的措辞怎么改都不会让屏上说这一趟出了错。
    assert!(matches!(错, CollectionError::Halted(_)), "{错:?}");
    assert_ne!(
        错.to_string(),
        Halted.to_string(),
        "这一句与核心库那一句一样的话，「靠措辞分档」那条老路就还留着一半",
    );
    assert_eq!(Cutoff::from(错), Cutoff::Halted);
    // 两份库一个字都没动。
    assert!(现场.site.store.memberships().expect("读得到").is_empty());
    assert!(筛(&现场.site.catalog, "收藏=是").is_empty());
}

#[test]
fn 排一趟再落下去_与当场收藏那一下一个字不差() {
    // **`add` 是「排一趟再落下去」的特例**：两条路各算各的话，任务台上那一趟与
    // 命令行那一下会给出两批不一样的锚。所以这一条**真的把两条路并排跑一遍**
    // ——各自对着一串写死的数字断言的话，两串数字一起写错也看不出来。
    let 那一批 = 键(&[马里奥, 勇者, 裸卡带]);

    let mut 当场 = 建现场();
    跑识别(&mut 当场);
    let 当场的账 = collection::add(&mut 当场.site, FAVORITE, &那一批).expect("加得进");

    let mut 排一趟 = 建现场();
    跑识别(&mut 排一趟);
    let 把手 = Handle::new();
    let 计划 = collection::plan(
        &排一趟.site.catalog,
        主库标识,
        FAVORITE,
        &那一批,
        true,
        &把手,
    )
    .expect("排得出");
    // **排完还没写库**：写那一半在认领那一步（台上那条线拿的是只读连接）。
    assert!(排一趟.site.store.memberships().expect("读得到").is_empty());
    // 进度报得出来：这一趟在屏上不是一块白板。
    let 进度 = 把手.progress();
    assert!(进度.at > 0 && !进度.step.is_empty(), "{进度:?}");
    assert_eq!((进度.done, 进度.total), (3, 3));

    let 排出来的账 = collection::commit(&mut 排一趟.site, &计划).expect("落得下去");
    assert_eq!(排出来的账, 当场的账, "两条路数出来的账不是同一本");
    assert_eq!(
        筛(&排一趟.site.catalog, "收藏=是"),
        筛(&当场.site.catalog, "收藏=是"),
        "两条路收藏进去的不是同一批",
    );
    // **沉淀库里那些成员关系也要一模一样**：锚就是它们的身份，两条路钉在不同的东西上
    // 正是这张票要防的那件事。
    let 锚 = |现场: &现场| -> Vec<Anchor> {
        let mut out: Vec<Anchor> = 现场
            .site
            .store
            .memberships()
            .expect("读得到")
            .into_iter()
            .map(|one| one.anchor)
            .collect();
        out.sort();
        out
    };
    assert_eq!(锚(&排一趟), 锚(&当场), "两条路钉的锚不是同一批");
}

/// **作品详情页状态块那一行「收藏」照它写**（票 `gui-looks-like-the-design/15`，拿主意的人 2026-09-15 定只读地写一行）：
/// 一个作品底下几个变体里有没有在收藏里的、有的话有没有只钉得住本机路径的——判断在核心库，界面照着挑一句话。
#[test]
fn 一个作品收没收藏_收了的里头有只钉得住路径的就照路径说() {
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    collection::add(&mut 现场.site, FAVORITE, &键(&[马里奥, 裸卡带])).expect("加得进");
    let 问 = |site: &Site, keys: &[&str]| collection::favorite_of(site, &键(keys)).expect("问得出");

    assert_eq!(问(&现场.site, &[勇者]), None, "底下一个都没收藏：未收藏");
    assert_eq!(
        问(&现场.site, &[马里奥, 勇者]),
        Some(ANCHOR_CONTENT),
        "收了的都钉在内容上：按文件内容记录"
    );
    assert_eq!(
        问(&现场.site, &[马里奥, 裸卡带]),
        Some(ANCHOR_PATH),
        "收了的里头有一个只钉得住路径：挪了位置会丢，照路径说"
    );
    // 在别的合集里不算收藏。
    collection::add(&mut 现场.site, "通关过的", &键(&[勇者])).expect("加得进");
    assert_eq!(问(&现场.site, &[勇者]), None, "在自建合集里不等于收藏了");
}

/// **改名：四条校验判在一处**（票 `gui-looks-like-the-design/13`，稿上 `collNameErr`）。
///
/// 界面自己判字符串的话，「加入合集」那个弹层与「改名」那个弹层迟早对同一个名字给出
/// 两种答复（ADR-0024）。
#[test]
fn 合集名那四条校验判在一处() {
    use romcat_core::collection::{BadName, check_name};

    let 已有 = ["通关过的".to_string(), "送朋友的".to_string()];

    // 一、过得去的：**交回来的是收拾过空白的那一串**，调用方忘不掉 trim。
    assert_eq!(
        check_name("  新的一组 ", &已有).expect("用得上"),
        "新的一组"
    );

    // 二、四条各撞一次。
    assert_eq!(check_name("", &已有), Err(BadName::Empty));
    assert_eq!(
        check_name("   ", &已有),
        Err(BadName::Empty),
        "只有空白也算空"
    );
    assert_eq!(check_name(FAVORITE, &已有), Err(BadName::Reserved));
    assert_eq!(check_name("通关过的", &已有), Err(BadName::Taken));
    assert_eq!(check_name("甲,乙", &已有), Err(BadName::Comma));
    assert_eq!(
        check_name("甲，乙", &已有),
        Err(BadName::Comma),
        "全角逗号也得拦：中文输入法下打出来的是这一个"
    );

    // 三、**每一条都说得出怎么才行**（ADR-0005 那条「不禁按钮」）。
    for 哪一条 in [
        BadName::Empty,
        BadName::Reserved,
        BadName::Taken,
        BadName::Comma,
    ] {
        assert!(!哪一条.advice().is_empty(), "{哪一条:?} 说不出为什么不行");
    }

    // 四、**改名要把被改的那一个先剔掉**，不然「改成它自己」会被判成重名。
    let 剔掉之后: Vec<String> = 已有
        .iter()
        .filter(|一个| *一个 != "通关过的")
        .cloned()
        .collect();
    assert_eq!(
        check_name("通关过的", &剔掉之后).expect("改成它自己该过得去"),
        "通关过的",
    );
}

/// **改名：成员关系、投影、以及写着 `合集=旧名` 的子库规则，三样一起改**
/// （票 `gui-looks-like-the-design/13`）。
///
/// 规则不跟着改的话，那个子库**第二天就选不出东西了**——而且屏上不会红，
/// 只是那一批悄悄变空。
#[test]
fn 改名连引用它的子库规则一起改_收藏改不得() {
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    collection::add(&mut 现场.site, "送朋友的", &键(&[马里奥, 勇者])).expect("加得进");

    // 一个子库，两条规则：一条指着这个合集，一条与它无关。
    现场
        .site
        .catalog
        .put_sublibrary(&sublibrary::Sublibrary::at(
            "掌机",
            std::path::Path::new("/Volumes/SDCARD/掌机"),
            "Pegasus",
            None,
        ))
        .expect("建得出子库");
    for text in ["合集=送朋友的", "平台=FC"] {
        现场
            .site
            .catalog
            .add_rule("掌机", &Rule::parse(text).expect("读得懂"))
            .expect("加得进规则");
    }

    let 改前 = 筛(&现场.site.catalog, "合集=送朋友的");
    assert!(!改前.is_empty(), "改名之前这个合集本该筛得出东西");

    let 账 = collection::rename(&mut 现场.site, "送朋友的", "给小明的").expect("改得动");
    assert!(账.members > 0, "沉淀库里那批成员关系没换名字");
    assert_eq!(账.rules, 1, "该跟着改的只有指着这个合集那一条");

    // 一、**新名字筛得出同一批，旧名字筛不出东西了**。
    assert_eq!(
        筛(&现场.site.catalog, "合集=给小明的"),
        改前,
        "改名之后新名字该筛出改名之前那一批"
    );
    assert!(
        筛(&现场.site.catalog, "合集=送朋友的").is_empty(),
        "旧名字还筛得出东西——投影没跟着重建"
    );

    // 二、**子库那两条规则**：指着它的那条改了，无关那条一个字没动。
    let 规则们: Vec<String> = 现场
        .site
        .catalog
        .sublibrary_rules("掌机")
        .expect("读得出")
        .into_iter()
        .map(|一条| 一条.text)
        .collect();
    assert!(
        规则们.contains(&"合集=给小明的".to_string()),
        "指着这个合集那条规则没跟着改：{规则们:?}"
    );
    assert!(
        规则们.contains(&"平台=FC".to_string()),
        "与这个合集无关那条规则被动了：{规则们:?}"
    );

    // 三、**收藏改不得**。
    assert!(
        matches!(
            collection::rename(&mut 现场.site, FAVORITE, "我的最爱"),
            Err(CollectionError::Reserved)
        ),
        "收藏那一组的名字是定死的，该拦住"
    );

    // 四、**新名字过不了校验就一个字都不写**。
    assert!(matches!(
        collection::rename(&mut 现场.site, "给小明的", "带,逗号"),
        Err(CollectionError::Name(_))
    ));
    assert_eq!(
        筛(&现场.site.catalog, "合集=给小明的"),
        改前,
        "校验没过却把库改了"
    );
}
