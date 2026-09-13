//! **列出一个工作目录里有哪些中立库**。
//!
//! 核心库此前一个列举函数都没有：所有开库的路都是「给我名字或路径，我去折一个出来看
//! 在不在」（`workspace::catalog_path` + `Site::open`）。**开场**那一屏要的正相反——
//! 人还说不出名字，得先看见这个**工作目录**里有些什么（ADR-0023）。

use romcat_core::catalog::roots::{self, RootScan};
use romcat_core::catalog::{Catalog, SCHEMA_VERSION};
use romcat_core::platform::Manifest;
use romcat_core::shape::{Role, SINGLE_FILE_RULE, Variant};
use romcat_core::testing::{self, Revoke, temp_dir};
use romcat_core::workspace::{self, CatalogEntry, CatalogState, Listing, Slug};

/// 列这个工作目录，**得是有库那一态**，交出那几行。
fn 有库(工作目录: &std::path::Path) -> Vec<CatalogEntry> {
    match workspace::catalogs(工作目录) {
        Listing::Catalogs(那几行) => 那几行,
        别的 => panic!("该列得出库：{别的:?}"),
    }
}

/// 在这个工作目录里现建一份中立库，返回它的文件路径。
fn 建一份(工作目录: &std::path::Path, 名字: &str) -> std::path::PathBuf {
    let slug = Slug::Named(名字);
    let 库文件 = workspace::catalog_path(工作目录, slug);
    drop(Catalog::create(&库文件, &slug.display_name()).expect("能建中立库"));
    库文件
}

/// 给这份库加一个叫「主库」的根，**一趟都不扫**，交回开着的那份库。
///
/// 根指着一块**没挂上的盘**：列举一个字节都不许去读它，上次扫描的时刻住在中立库里（ADR-0009）。
fn 加一个根(工作目录: &std::path::Path, 库文件: &std::path::Path) -> Catalog {
    let catalog = Catalog::open(库文件).expect("能再打开");
    roots::add_root(
        &catalog,
        Some(工作目录),
        "主库",
        std::path::Path::new("/没挂上的那块盘/主库"),
    )
    .expect("加得上根");
    catalog
}

/// 给这份库加一个根（[`加一个根`]）并记下它在 `时刻` 扫过一趟。
fn 记一趟扫描(工作目录: &std::path::Path, 库文件: &std::path::Path, 时刻: i64) {
    加一个根(工作目录, 库文件)
        .record_root_scan(
            "主库",
            &RootScan {
                at: 时刻,
                elapsed_ms: 1_000,
                entries: 0,
                interrupted: false,
            },
        )
        .expect("记得下上次扫描");
}

/// 列这个工作目录，交出那几行上印的名字，**按列举交出来的次序**。
fn 名字次序(工作目录: &std::path::Path) -> Vec<String> {
    有库(工作目录).into_iter().map(|一份| 一份.name).collect()
}

/// 造一个变体：键是 `主库/<平台>/<名字>`，单文件规则，一个主成员。
fn 变体(平台: &str, 名字: &str) -> Variant {
    let key = format!("主库/{平台}/{名字}");
    Variant {
        main_key: key.clone(),
        platform: Some(平台.to_string()),
        rule: SINGLE_FILE_RULE.to_string(),
        manual: false,
        files: 1,
        bytes: 4_096,
        unreadable_files: 0,
        members: vec![(key.clone(), Role::Main)],
        key,
    }
}

#[test]
fn 列出这个工作目录里的中立库() {
    let 工作目录 = temp_dir("列举-两份");
    建一份(工作目录.path(), "甲库");
    建一份(工作目录.path(), "乙库");

    let 列出来的 = 有库(工作目录.path());

    // 这一条只管**列全了**，不管次序。从前这儿写的是「按主库原名的码位排，于是『乙库』
    // 排在『甲库』前头」——挂单 `Q389` 裁成了按上次扫描时刻倒排，次序另由紧跟在这条后面、
    // 按上次扫描排的那几条钉着。
    assert_eq!(列出来的.len(), 2, "该列出两份：{列出来的:?}");
    let 名字: std::collections::BTreeSet<&str> =
        列出来的.iter().map(|一份| 一份.name.as_str()).collect();
    assert_eq!(名字, std::collections::BTreeSet::from(["甲库", "乙库"]));
}

#[test]
fn 刚扫过的库排在很久没扫的前头() {
    // 挂单 `Q389`：开场那几行里「哪份是我刚在弄的」比「哪份名字码位小」有用得多——用户
    // 机器上是三份库（ADR-0023）。名字特意挑成码位与扫描先后**反着**的：「乙库」码位小，
    // 可它很久没扫了。
    let 工作目录 = temp_dir("列举-按上次扫描");
    let 甲 = 建一份(工作目录.path(), "甲库");
    let 乙 = 建一份(工作目录.path(), "乙库");
    记一趟扫描(工作目录.path(), &甲, 1_700_000_000);
    记一趟扫描(工作目录.path(), &乙, 1_600_000_000);

    assert_eq!(
        名字次序(工作目录.path()),
        ["甲库", "乙库"],
        "刚扫过的那份没排在最上面",
    );
}

#[test]
fn 从没扫过的库排在最前() {
    // 一份从没扫过的库是**建出来了却没开工**的那一份：未完成的东西该看得见，排最后会让它
    // 沉到底下，而那正是人最该点进去的一行（`Q389` 那一件在规格第五节里的裁定）。它**有根**，
    // 只是那个根一趟都没扫过——上次扫描那一格是空的，不是零。名字的码位比另两份都大，按名字排
    // 它垫底。
    let 工作目录 = temp_dir("列举-从没扫过");
    let 甲 = 建一份(工作目录.path(), "甲库");
    let 乙 = 建一份(工作目录.path(), "乙库");
    let 没开工的 = 建一份(工作目录.path(), "还没开工的库");
    记一趟扫描(工作目录.path(), &甲, 1_700_000_000);
    记一趟扫描(工作目录.path(), &乙, 1_600_000_000);
    drop(加一个根(工作目录.path(), &没开工的));

    assert_eq!(
        名字次序(工作目录.path()),
        ["还没开工的库", "甲库", "乙库"],
        "从没扫过的那份没排在最前",
    );
}

#[test]
fn 同一个工作目录列两次次序一样() {
    // 目录列出来的次序是文件系统给的，两次打开不保证一样，而开场那一屏的行不该自己跳来
    // 跳去。上次扫描时刻**撞在同一秒**、或者**都没扫过**时，得有一个定死的次级键：
    // **主库原名**（比码位，这个仓库不做拼音排序），同名的再按路径。
    //
    // 建库的次序特意与排出来的次序反着，免得碰巧照着建库的先后排也过得去。
    let 工作目录 = temp_dir("列举-两次一样");
    建一份(工作目录.path(), "甲库");
    建一份(工作目录.path(), "乙库");
    let 丙 = 建一份(工作目录.path(), "丙库");
    let 丁 = 建一份(工作目录.path(), "丁库");
    记一趟扫描(工作目录.path(), &丙, 1_700_000_000);
    记一趟扫描(工作目录.path(), &丁, 1_700_000_000);

    let 头一次 = 名字次序(工作目录.path());
    let 第二次 = 名字次序(工作目录.path());

    assert_eq!(头一次, 第二次, "同一个工作目录列两次，次序变了");
    // 甲、乙都没扫过：「乙」码位小。丙、丁同一秒扫的：「丁」码位小。
    assert_eq!(
        头一次,
        ["乙库", "甲库", "丁库", "丙库"],
        "扫描时刻相同或都没扫过时，没按主库原名定次序",
    );
}

#[test]
fn 说不上上次什么时候扫的库排在最后() {
    // 挂单 `Q731`：**读不出来所以不知道，不是从没扫过**——同 ADR-0021 那条修订「读不动不是
    // 空的」一个道理。结构版本对不上、文件坏了的那几份，上次扫描那一格根本读不出来；
    // 把它们当成「从没扫过」排到最前，用户机器上那三份结构版本对不上的库（ADR-0023）就会
    // 压在他刚在弄的那份上面，而它们的「打开」还按不下去。
    //
    // 它们**照列**（`一份读不出来的库不连累其余`），只是垫底，各自再按名字排。
    let 工作目录 = temp_dir("列举-说不上来的垫底");
    建一份(工作目录.path(), "还没开工的库");
    let 甲 = 建一份(工作目录.path(), "甲库");
    记一趟扫描(工作目录.path(), &甲, 1_700_000_000);
    testing::catalog_at_version(
        &workspace::catalog_path(工作目录.path(), Slug::Named("版本对不上的库")),
        4,
    );
    std::fs::write(
        workspace::catalog_path(工作目录.path(), Slug::Named("坏的库")),
        "这不是一份 SQLite 数据库",
    )
    .expect("写得出那个文件");

    assert_eq!(
        名字次序(工作目录.path()),
        ["还没开工的库", "甲库", "坏的库", "版本对不上的库"],
        "说不上上次什么时候扫的那几份没垫底",
    );
}

#[test]
fn 变体数与上次扫描时刻盘没挂上照样交得出() {
    // 这三个数（主库原名、变体数、上次扫描时刻）住在**中立库**里，不在主库上（ADR-0009）。
    // 于是外置盘不在位时开场那一屏照样画得出来——那正是这个工作方式的前提（ADR-0018）。
    let 工作目录 = temp_dir("列举-三个数");
    let 库文件 = 建一份(工作目录.path(), "外置盘上的库");
    {
        let mut catalog = Catalog::open(&库文件).expect("能再打开");
        // 根指着一块**没挂上的盘**：底下这几个数一个都不许去读它。
        roots::add_root(
            &catalog,
            Some(工作目录.path()),
            "主库",
            std::path::Path::new("/没挂上的那块盘/主库"),
        )
        .expect("加得上根");
        catalog
            .replace_variants(
                &[变体("FC", "魂斗罗.zip"), 变体("FC", "超级马里奥.zip")],
                1,
                &Manifest::default(),
            )
            .expect("写得进变体");
        catalog
            .record_root_scan(
                "主库",
                &RootScan {
                    at: 1_700_000_000,
                    elapsed_ms: 1_000,
                    entries: 2,
                    interrupted: false,
                },
            )
            .expect("记得下上次扫描");
    }

    let 列出来的 = 有库(工作目录.path());
    let [一份] = 列出来的.as_slice() else {
        panic!("该只有一份库：{列出来的:?}");
    };
    let CatalogState::Openable(Ok(那几个数)) = &一份.state else {
        panic!("这一份该开得进去、数也读得回来：{一份:?}");
    };
    assert_eq!(那几个数.variants, 2);
    assert_eq!(那几个数.scanned_at, Some(1_700_000_000));
}

#[test]
fn 结构版本对不上的库照列并说清是哪个版本对哪个版本() {
    // **从列表里静默消失才是最难查的那种错**（ADR-0023）：用户机器上真有三份结构版本
    // 对不上的库，开场那一屏上线第一眼看见的就是它们。照列，并把那句话原样带上——
    // 措辞由核心库一处出（`CatalogError::Version`），列举这一层不另造一句。
    let 工作目录 = temp_dir("列举-版本对不上");
    let 好的 = 建一份(工作目录.path(), "还开得了的库");
    let 旧的 = workspace::catalog_path(工作目录.path(), Slug::Named("版本对不上的库"));
    testing::catalog_at_version(&旧的, 4);

    let 列出来的 = 有库(工作目录.path());
    assert_eq!(
        列出来的.len(),
        2,
        "打不开的那一份不许从列表里消失：{列出来的:?}"
    );

    let 那一份 = 列出来的
        .iter()
        .find(|一份| 一份.path == 旧的)
        .expect("版本对不上的那一份照样列出来了");
    assert!(
        !那一份.state.openable(),
        "版本对不上的那一份不该是「开得进去」"
    );
    let CatalogState::SchemaMismatch { said: 说的, .. } = &那一份.state else {
        panic!("版本对不上的那一份没交出「结构版本对不上」：{那一份:?}");
    };
    assert!(
        说的.contains("结构版本是 4"),
        "没说清库里是哪个版本：{说的}"
    );
    assert!(
        说的.contains(&format!("本程序认得的是 {SCHEMA_VERSION}")),
        "没说清本程序认哪个版本：{说的}",
    );
    assert!(说的.contains("删掉它重扫一遍"), "没说该怎么办：{说的}");
    // **逐项说清会丢什么**（ADR-0001 的修订，挂账 D97）：人按下去之前得知道代价，
    // 不许只说一句「重扫一遍就好」。
    for 会丢的 in ["子库的定义与清单", "底本", "首选变体", "亲手加的叫法"] {
        assert!(说的.contains(会丢的), "没说「{会丢的}」会丢：{说的}");
    }

    // **开不了的那一份也得有名字**：从文件名截，剥掉哈希后缀。
    assert_eq!(那一份.name, "版本对不上的库");

    // 别的照列，一份打不开不连累其余。
    let 好那一份 = 列出来的
        .iter()
        .find(|一份| 一份.path == 好的)
        .expect("好的那一份照列");
    assert!(
        matches!(好那一份.state, CatalogState::Openable(Ok(_))),
        "好的那一份被连累了：{好那一份:?}"
    );
}

#[test]
fn 调用方分得开结构版本对不上与文件坏了() {
    // ADR-0021 那条修订、挂单 `Q393`：从前两种打不开交出来都是一句字符串，调用方要分只能去
    // 解析那句话。而人对这两种的下一步各不相同——结构版本对不上删掉重扫就好，文件坏了是
    // 另一回事。**分支靠类型，不靠字符串**：这条测试一个字都不看那句话。
    let 工作目录 = temp_dir("列举-分得开");
    let 好的 = 建一份(工作目录.path(), "好的库");
    let 旧的 = workspace::catalog_path(工作目录.path(), Slug::Named("版本对不上的库"));
    testing::catalog_at_version(&旧的, 4);
    let 坏的 = workspace::catalog_path(工作目录.path(), Slug::Named("坏的库"));
    std::fs::write(&坏的, "这不是一份 SQLite 数据库").expect("写得出那个文件");

    let 列出来的 = 有库(工作目录.path());
    let 那一份 = |文件: &std::path::PathBuf| {
        列出来的
            .iter()
            .find(|一份| &一份.path == 文件)
            .unwrap_or_else(|| panic!("{} 没列出来：{列出来的:?}", 文件.display()))
    };

    assert!(
        matches!(
            那一份(&旧的).state,
            CatalogState::SchemaMismatch { found: 4, expected, .. } if expected == SCHEMA_VERSION
        ),
        "版本对不上的那一份没交出「结构版本对不上」、或者版本号不对：{:?}",
        那一份(&旧的),
    );
    assert!(
        matches!(那一份(&坏的).state, CatalogState::Broken { .. }),
        "坏了的那一份没交出「文件坏了」：{:?}",
        那一份(&坏的),
    );
    assert!(
        matches!(那一份(&好的).state, CatalogState::Openable(Ok(_))),
        "好的那一份没交出「开得了」：{:?}",
        那一份(&好的),
    );
}

#[test]
fn 主库原名读不到时退回从文件名截既不空着也不是那串哈希() {
    // 票 01 之前建的库不知道自己叫什么。退路在
    // `Catalog::library_name` 里，这一条钉的是**列举这一层真的走了那条退路**——
    // 开场那一屏上一行空白或者一串十六进制，人都认不出那是自己的哪份库。
    let 工作目录 = temp_dir("列举-没名字");
    let slug = Slug::Named("没记过名字的库");
    let 库文件 = workspace::catalog_path(工作目录.path(), slug);
    testing::catalog_without_name(&库文件);

    let 列出来的 = 有库(工作目录.path());
    let [一份] = 列出来的.as_slice() else {
        panic!("该只有一份库：{列出来的:?}");
    };
    assert_eq!(一份.name, "没记过名字的库");
    assert!(!一份.name.contains(&slug.text()), "交出来的是那串哈希");
}

#[test]
fn 一份读不出来的库不连累其余() {
    // 验收里那条「不因为其中某一份库打不开而整屏失败」——版本对不上只是打不开的一种，
    // 文件被截断、被别的东西占了名字都算。**那一份单独标出来，其余照列。**
    let 工作目录 = temp_dir("列举-坏的一份");
    建一份(工作目录.path(), "好的库");
    let 坏的 = workspace::catalog_path(工作目录.path(), Slug::Named("坏的库"));
    std::fs::write(&坏的, "这不是一份 SQLite 数据库").expect("写得出那个文件");

    let 列出来的 = 有库(工作目录.path());
    assert_eq!(列出来的.len(), 2, "整批塌了：{列出来的:?}");
    let 那一份 = 列出来的
        .iter()
        .find(|一份| 一份.path == 坏的)
        .expect("坏的那一份照样列出来了");
    assert!(
        !那一份.state.openable(),
        "开不进去的那一份不该是「开得进去」：{那一份:?}"
    );
    assert_eq!(那一份.name, "坏的库", "开不了也得有个认得出的名字");
    assert!(
        列出来的
            .iter()
            .any(|一份| matches!(一份.state, CatalogState::Openable(Ok(_)))),
        "好的那一份被连累了：{列出来的:?}",
    );
}

#[test]
fn 一份中立库都没有时列出来是空的而且不留下任何东西() {
    // 开场那一屏那句「还没有库，添加一个主库开始」由它撑着。**列一遍不许留痕**：
    // 只是想看看有哪些库，不该顺手把目录建出来，更不该建出一份空库。
    let 工作目录 = temp_dir("列举-空的");
    let 列出来的 = workspace::catalogs(工作目录.path());
    assert!(
        matches!(列出来的, Listing::Empty),
        "还没建过库的工作目录该是「空的」：{列出来的:?}"
    );
    assert!(
        !工作目录.path().join("catalog").exists(),
        "列一遍把目录建出来了",
    );
}

#[test]
fn 工作目录读不动时交出来的是读不动而不是空的() {
    // ADR-0021 那条修订：**读不动与空的是两态**（挂单 `Q388`）。从前列不开那个目录就当空的
    // 交出去，开场屏上说「还没有库」——而正确的下一步完全不同：去修那个目录的权限，别去
    // 建第二份库。
    //
    // 造的是**真的**读不动的目录，不注入：里头明明有一份库，权限位一收就列不开。造不出来
    // （不是 Unix、或者跑测试的是 root）就如实跳过，`testing::revoke` 印出为什么。
    //
    // **两层都钉**：工作目录**本身**读不动（票面说的是「把工作目录指到一个读不动的位置」），
    // 与它底下中立库住的那个目录读不动。
    let 工作目录 = temp_dir("列举-收了读权限");
    let 库文件 = 建一份(工作目录.path(), "列不开也在的库");
    let 目录 = 库文件.parent().expect("有那个目录").to_path_buf();

    for 收哪一层 in [工作目录.path(), 目录.as_path()] {
        let Some(_还回去) = testing::revoke(收哪一层, Revoke::Read) else {
            return;
        };

        let 列出来的 = workspace::catalogs(工作目录.path());

        let Listing::Unreadable(为什么) = &列出来的 else {
            panic!(
                "{} 读不动时交出来的不是「读不动」：{列出来的:?}",
                收哪一层.display()
            );
        };
        // 列的是中立库住的那个目录；工作目录本身读不动时，列不开的也是它。
        assert_eq!(为什么.dir, 目录, "没说清是哪个目录读不动");
        let 说的 = 为什么.to_string();
        assert!(说的.contains("读不动"), "那句话没说是读不动：{说的}");
        assert!(
            说的.contains(&romcat_core::path::display(&目录)),
            "那句话没说清是哪个目录：{说的}"
        );
    }
}

#[test]
fn 不是中立库的那些文件不算数() {
    let 工作目录 = temp_dir("列举-旁边的文件");
    let 库文件 = 建一份(工作目录.path(), "唯一那份库");
    let 目录 = 库文件.parent().expect("有那个目录");
    std::fs::write(目录.join("笔记.txt"), "随手记的").expect("写得出");
    // SQLite 自己那两个附件：它们跟中立库同名、只多一个后缀，**不是第二份库**。
    std::fs::write(目录.join("唯一那份库.sqlite3-wal"), "").expect("写得出");
    std::fs::create_dir(目录.join("子目录")).expect("建得出");

    let 列出来的 = 有库(工作目录.path());
    let [一份] = 列出来的.as_slice() else {
        panic!("旁边的东西被当成库了：{列出来的:?}");
    };
    assert_eq!(一份.path, 库文件);
}
