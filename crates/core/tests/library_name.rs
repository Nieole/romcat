//! **这份主库叫什么**：原名落进中立库的元数据表，读不到那一行时从文件名截。
//!
//! 名字此前没有落点。中立库的文件名是「可读的那一段 + 哈希」
//! （[`workspace::Slug::text`]）：可读那一段滤掉非法字符、截到 24 个字符、全滤光时
//! 退成 `library`，哈希那一段不可逆。于是人起的那个名字只活在他敲过的那行命令里，
//! **开场那一屏没处去读它**（ADR-0023）。
//!
//! 这里钉的是三件事：建库时那一行真的落了盘、读得回来的是**原名**而不是折过的文件名、
//! 以及**读不到时退回从文件名截**——票 01 之前建的那些库一个字都不会改，
//! 中立库的结构版本也不许为这一行加 1（它是元数据表上的**纯加**）。
//!
//! 另钉一件与原名相对的事：**主库标识**正过一次名（`Site::library` → `Site::library_identity`），
//! **路径锚**里存的那串字节一个没动。
//!
//! **已经建好的库改得了名**（`Catalog::set_library_name`，挂单 `Q370`）：改完开场屏与现场上印的
//! 是新名字，**路径锚与中立库的文件名一个字节不动**——锚认的是主库标识，改的是主库原名。
//! 元数据表那一族的键名收进一处之后，旧库里按原先那几串键名记下的账也得照样读得回来。

use romcat_core::catalog::{Catalog, EntryRecord, SCHEMA_VERSION, Verdict};
use romcat_core::fs::{EntryKind, EntryMeta};
use romcat_core::site::Site;
use romcat_core::testing::temp_dir;
use romcat_core::verdict::{Anchor, Membership, Store};
use romcat_core::workspace::{self, Slug};

#[test]
fn 建库时原名落进元数据表再开一次读得回来() {
    let 工作目录 = temp_dir("原名-建库");
    let slug = Slug::Named("主库");
    let 库文件 = workspace::catalog_path(工作目录.path(), slug);

    // 建库这个动作本身就是开库：这一趟把名字一并写下。
    drop(Catalog::open_named(&库文件, &slug.display_name()).expect("能建中立库"));

    let catalog = Catalog::open(&库文件).expect("能再打开");
    assert_eq!(catalog.library_name(), "主库");
}

#[test]
fn 票01之前建的库读不到那一行时退回从文件名截() {
    // 那些库的元数据表里压根没有这一行，而它们**不会被改**——旧库拿新程序打开照样能用
    // （中立库的结构版本没有为这一行加 1）。于是「这份主库叫什么」只能从文件名截，
    // 截出来的是人认得出的那一半，不是那串哈希。
    let 工作目录 = temp_dir("原名-旧库");
    let slug = Slug::Named("我的库");
    let 库文件 = workspace::catalog_path(工作目录.path(), slug);

    {
        // 票 01 之前那条路：建库时没人告诉它这份主库叫什么。
        let mut catalog = Catalog::open(&库文件).expect("能建中立库");
        写一条(&mut catalog);
    }

    // 新程序打开同一份：版本对得上（对不上 `open_named` 当场就拒），库里的东西一条不少，
    // 名字退回从文件名截。**新程序也不会顺手把名字补进去**——那一行只在建库那一趟落。
    let catalog = Catalog::open_named(&库文件, &slug.display_name()).expect("旧库照样打得开");
    assert!(
        catalog.contains("库/FC/魂斗罗.zip").expect("读得出来"),
        "旧库里的东西一条不少"
    );
    assert_eq!(catalog.library_name(), "我的库");
    assert!(
        !catalog.library_name().contains(&slug.text()),
        "退路交出来的不许是那串哈希"
    );
}

#[test]
fn 名字折不进文件名时元数据表里仍是原名() {
    // 名字里带路径分隔符、控制字符、或长过 24 个字符：文件名照旧按既有规则折
    // （滤字符、截断、缀哈希），而落进元数据表的是**原名**。两条路各自正确、互不干扰。
    let 工作目录 = temp_dir("原名-怪名字");
    for 原名 in [
        "甲/乙\\丙:丁",
        "这个主库的名字长得超过二十四个字符所以文件名一定截得到它",
        "带\u{1}控制字符",
    ] {
        let slug = Slug::Named(原名);
        let 库文件 = workspace::catalog_path(工作目录.path(), slug);
        drop(Catalog::open_named(&库文件, &slug.display_name()).expect("能建中立库"));

        let 主文件名 = 库文件
            .file_stem()
            .expect("有主文件名")
            .to_string_lossy()
            .into_owned();
        assert_eq!(主文件名, slug.text(), "文件名照旧按既有规则折");
        assert!(!主文件名.contains('/') && !主文件名.contains('\\') && !主文件名.contains(':'));

        let catalog = Catalog::open(&库文件).expect("能再打开");
        assert_eq!(catalog.library_name(), 原名, "元数据表里是原名");
    }
}

#[test]
fn 名字被滤光时文件名退成那个固定词而元数据表里仍是原名() {
    let 工作目录 = temp_dir("原名-滤光");
    let slug = Slug::Named("。、？");
    let 库文件 = workspace::catalog_path(工作目录.path(), slug);
    drop(Catalog::open_named(&库文件, &slug.display_name()).expect("能建中立库"));

    let 主文件名 = 库文件
        .file_stem()
        .expect("有主文件名")
        .to_string_lossy()
        .into_owned();
    assert!(
        主文件名.starts_with("library-"),
        "字符全被滤光，文件名退成那个固定词：{主文件名}"
    );

    let catalog = Catalog::open(&库文件).expect("能再打开");
    assert_eq!(catalog.library_name(), "。、？");
}

#[test]
fn 界面与命令行开同一份库读到的是同一个名字() {
    // **验的是读这一侧**：开库这件事在核心里，命令行按 `--library` 的名字找，
    // 界面直接开开场上列出来的那一份文件（`site::Site` 的两个入口，界面走的就是它们，
    // 见 `crates/gui/src/site.rs`）。两条路读出两个名字的话，人在终端里与在界面上就认不出
    // 自己操作的是同一份库了。建库那一侧不在这一条里——那两个入口都先 `exists()` 才开，
    // 建不出新库，眼下唯一建得出库的是命令行的 `open_catalog`（挂单 `Q371`）。
    //
    // 名字挑一个**折进文件名会变形**的：`。、？` 全被滤光，文件名退成 `library-…`。
    // 于是「两边一样」不是因为两边都在读文件名。
    let 工作目录 = temp_dir("原名-两条路");
    for 原名 in ["主库", "。、？"] {
        let slug = Slug::Named(原名);
        let 库文件 = workspace::catalog_path(工作目录.path(), slug);
        drop(Catalog::open_named(&库文件, &slug.display_name()).expect("能建中立库"));

        let 按名字 = Site::open(工作目录.path(), slug, None, "--library").expect("开得出现场");
        let 按文件 = Site::open_file(工作目录.path(), &库文件, None).expect("开得出现场");

        assert_eq!(按名字.catalog.library_name(), 原名);
        assert_eq!(按文件.catalog.library_name(), 原名);
    }
}

#[test]
fn 路径锚里存的主库标识一个字节没动() {
    // **主库标识**在代码里正过一次名（票 `no-mute-spots-opening-a-catalog/01`），**只改叫法，
    // 不动锚**。沉淀库不可再生，里头那些**路径锚**记的就是这一串原样的字节——它要是变了
    // 一个字，那些锚一条都不删，却从此一条都撞不上。
    //
    // 这串字面量是正名之前从代码里读出来的，**不许照着现在的算法重算一遍**：重算出来的
    // 那一串永远与代码一致，钉不住任何东西。
    const 锚里存的: &str = "主库-f5c61109e92b6036";
    let 工作目录 = temp_dir("标识-路径锚");
    let slug = Slug::Named("主库");
    let 库文件 = workspace::catalog_path(工作目录.path(), slug);
    drop(Catalog::open_named(&库文件, &slug.display_name()).expect("能建中立库"));

    let 变体 = "主盘/FC/魂斗罗.zip";
    {
        // 正名之前落下的那条路径锚：沉淀库里原样躺着的就是这串字节。
        let mut store =
            Store::open(&workspace::verdict_store_path(工作目录.path())).expect("开得出沉淀库");
        store
            .join(&[Membership::now(
                "收藏",
                Anchor::Path {
                    library: 锚里存的.to_string(),
                    variant_key: 变体.to_string(),
                },
            )])
            .expect("写得进沉淀库");
    }

    // 两个入口都得认得它：命令行按 `--library` 的名字找，界面直接开列出来的那份文件。
    let 按名字 = Site::open(工作目录.path(), slug, None, "--library").expect("开得出现场");
    let 按文件 = Site::open_file(工作目录.path(), &库文件, None).expect("开得出现场");
    for (路, site) in [("按名字", &按名字), ("按文件", &按文件)] {
        assert_eq!(site.library_identity, 锚里存的, "{路}开出来的主库标识变了");
        let 锚 = Anchor::Path {
            library: site.library_identity.clone(),
            variant_key: 变体.to_string(),
        };
        assert_eq!(
            site.store.joined(&锚).expect("读得出沉淀库"),
            ["收藏"],
            "{路}开出来的现场认不出原先那条路径锚"
        );
    }
}

#[test]
fn 已经建好的库改得了名开场屏与报告上印的是新名字() {
    // 维护者起错了名字，不必删库重来（挂单 `Q370`）。改的是**主库原名**那一行：
    // 开场那一屏列的（`workspace::catalogs`）、窗口标题与报告抬头印的（`Site::display_name`、
    // `Catalog::library_name`）都是它。库里的东西一条不少，文件也不挪。
    let 工作目录 = temp_dir("原名-改名");
    let 起的名字 = Slug::Named("起错了的名字");
    let 库文件 = workspace::catalog_path(工作目录.path(), 起的名字);
    {
        let mut catalog =
            Catalog::open_named(&库文件, &起的名字.display_name()).expect("能建中立库");
        写一条(&mut catalog);
    }

    Catalog::open(&库文件)
        .expect("能再打开")
        .set_library_name("改过的名字")
        .expect("改得了名");

    let 列出来的 = workspace::catalogs(工作目录.path());
    let [一份] = 列出来的.as_slice() else {
        panic!("改名不该多出或少掉一份库：{列出来的:?}");
    };
    assert_eq!(一份.name, "改过的名字", "开场屏上印的还是旧名字");
    assert_eq!(一份.path, 库文件, "改名把中立库文件挪走了");

    let 现场 = Site::open_file(工作目录.path(), &库文件, None).expect("开得出现场");
    assert_eq!(
        现场.display_name(),
        "改过的名字",
        "窗口标题上印的还是旧名字"
    );
    assert!(
        现场.catalog.contains("库/FC/魂斗罗.zip").expect("读得出来"),
        "改名之后库里的东西少了"
    );
}

#[test]
fn 改名之后路径锚一个字节没动沉淀库里的裁决照旧对得上() {
    // **锚认的是主库标识，改的是主库原名。** 沉淀库不可再生，里头的**路径锚**记的是
    // 主库标识那一串原样的字节；改名要是顺手把它（连同中立库的文件名）跟着新名字折一遍，
    // 那些裁决一条都不删，却从此一条都撞不上。
    //
    // 字面量同 `路径锚里存的主库标识一个字节没动`：取自正名之前的代码，不照算法重算。
    const 锚里存的: &str = "主库-f5c61109e92b6036";
    let 工作目录 = temp_dir("改名-路径锚");
    let 起的名字 = Slug::Named("主库");
    let 库文件 = workspace::catalog_path(工作目录.path(), 起的名字);
    drop(Catalog::open_named(&库文件, &起的名字.display_name()).expect("能建中立库"));

    let 变体 = "主盘/FC/魂斗罗.zip";
    {
        let mut store =
            Store::open(&workspace::verdict_store_path(工作目录.path())).expect("开得出沉淀库");
        store
            .join(&[Membership::now(
                "收藏",
                Anchor::Path {
                    library: 锚里存的.to_string(),
                    variant_key: 变体.to_string(),
                },
            )])
            .expect("写得进沉淀库");
    }

    Catalog::open(&库文件)
        .expect("能再打开")
        .set_library_name("改过的名字")
        .expect("改得了名");

    // 找库仍按**原先那个名字**：`--library` 折出来的是主库标识，改名不动它。
    let 按名字 = Site::open(工作目录.path(), 起的名字, None, "--library").expect("开得出现场");
    let 按文件 = Site::open_file(工作目录.path(), &库文件, None).expect("开得出现场");
    for (路, site) in [("按名字", &按名字), ("按文件", &按文件)] {
        assert_eq!(site.display_name(), "改过的名字", "{路}开出来的不是新名字");
        assert_eq!(
            site.library_identity, 锚里存的,
            "{路}开出来的主库标识跟着改名变了"
        );
        let 锚 = Anchor::Path {
            library: site.library_identity.clone(),
            variant_key: 变体.to_string(),
        };
        assert_eq!(
            site.store.joined(&锚).expect("读得出沉淀库"),
            ["收藏"],
            "{路}开出来的现场认不出改名之前那条路径锚"
        );
    }
}

#[test]
fn 改名成空白时退回从文件名截而不是留一行空的() {
    // **空白不是名字**，改名也一样（建库那一趟的规矩见下一条）。清空了名字的库退回
    // 从文件名截——截出来的是建库时那个名字折进文件名的可读一半——而不是在开场那一屏
    // 留一行空白，也不是还印着清空之前那个名字。
    let 工作目录 = temp_dir("改名-空白");
    let 起的名字 = Slug::Named("原先的名字");
    let 库文件 = workspace::catalog_path(工作目录.path(), 起的名字);
    drop(Catalog::open_named(&库文件, &起的名字.display_name()).expect("能建中立库"));

    for 空的 in ["", "   "] {
        {
            let catalog = Catalog::open(&库文件).expect("能再打开");
            catalog.set_library_name("改过的名字").expect("改得了名");
            catalog.set_library_name(空的).expect("清得掉名字");
        }
        assert_eq!(
            Catalog::open(&库文件).expect("能再打开").library_name(),
            "原先的名字",
            "清空名字之后该退回从文件名截"
        );
        let 列出来的 = workspace::catalogs(工作目录.path());
        assert_eq!(
            列出来的
                .iter()
                .map(|一份| 一份.name.as_str())
                .collect::<Vec<_>>(),
            ["原先的名字"],
            "开场屏上该印从文件名截出来的那一半"
        );
    }
}

#[test]
fn 名字是空白时退回从文件名截而不是留一行空的() {
    // `--library ""` 命令行不拦（改它的行为不在这张票里）。**空白不是名字**：那一行不落，
    // 读的时候也退回从文件名截，于是报告里不会印出一句光秃秃的「主库：」，
    // 开场那一屏也不会多一行没有名字的库。
    let 工作目录 = temp_dir("原名-空名字");
    for 空的 in ["", "   "] {
        let slug = Slug::Named(空的);
        let 库文件 = workspace::catalog_path(工作目录.path(), slug);
        drop(Catalog::open_named(&库文件, &slug.display_name()).expect("能建中立库"));

        let catalog = Catalog::open(&库文件).expect("能再打开");
        assert_eq!(
            catalog.library_name(),
            "library",
            "退回从文件名截，而文件名的可读一半本来就退成了那个固定词"
        );
    }
}

#[test]
fn 元数据表那一族的键名一个字节没动旧库记下的账照样读得回来() {
    // 元数据表上那几笔账（主库原名、上次折标题、上次导出、记住的导出配置、成型记到哪一趟）
    // **键名是落在盘上的**。键名收进一处的时候改岔一个字，旧库里那一行不删，却从此读不回来
    // ——改过的名字变回从文件名截、记住的导出配置要人重选，谁都不报错。
    //
    // 造法是**按旧程序落盘的样子直接往那张表里写**（同 `testing::catalog_at_version`）。
    // 这几串键名取自键名收进一处之前的代码，**不许照着现在的代码抄一遍**。
    use romcat_core::catalog::ExportSetup;
    use std::path::PathBuf;

    let 工作目录 = temp_dir("元数据表-键名");
    let 库文件 = workspace::catalog_path(工作目录.path(), Slug::Named("键名"));
    drop(Catalog::open(&库文件).expect("能建中立库"));
    {
        let conn = rusqlite::Connection::open(&库文件).expect("能再打开那个文件");
        for (键, 值) in [
            ("library_name", "旧程序记下的名字"),
            ("titles_folded_at", "1700000001"),
            ("exported_at", "1700000002"),
            ("export_format", "Pegasus"),
            ("export_out_dir", "/导出/目录"),
            ("shaped_scan", "3"),
            ("shaped_manifest", "42"),
        ] {
            conn.execute(
                "INSERT INTO meta(key, value) VALUES(?1, ?2)",
                rusqlite::params![键, 值],
            )
            .expect("写得进那一行");
        }
    }

    let catalog = Catalog::open(&库文件).expect("旧库照样打得开");
    assert_eq!(catalog.library_name(), "旧程序记下的名字");
    assert_eq!(
        catalog.titles_folded_at().expect("读得出"),
        Some(1_700_000_001)
    );
    assert_eq!(catalog.exported_at().expect("读得出"), Some(1_700_000_002));
    assert_eq!(
        catalog.export_setup().expect("读得出"),
        Some(ExportSetup {
            format: "Pegasus".to_string(),
            out: PathBuf::from("/导出/目录"),
        })
    );
    assert_eq!(catalog.shaped_scan().expect("读得出"), Some(3));
    assert_eq!(catalog.shaped_manifest().expect("读得出"), Some(42));
}

/// 往库里放一条记录，好证明「旧库照样能用」不是空话。
fn 写一条(catalog: &mut Catalog) {
    catalog
        .write(
            1,
            &[EntryRecord {
                key: "库/FC/魂斗罗.zip".to_string(),
                kind: EntryKind::File,
                meta: EntryMeta::Known {
                    len: 2_048,
                    modified: None,
                },
                non_utf8: false,
                verdict: Verdict::Added,
                sample: None,
                container: None,
            }],
        )
        .expect("写得进去");
}

#[test]
fn 记这一行不动中立库的结构版本() {
    // 元数据表是键值表，**加一个键是纯加**：已有的表一列没动、一条语义没改，按仓库
    // 既定的判据（改了已有表的列或含义才加 1）这不构成升版。为它逼用户删掉几百 MB 的库、
    // 重扫大半个钟头，换不到任何东西。
    //
    // 这个数钉在这儿，是好让日后真要升版的人在这里绊一下：升版本身没有错，
    // 错的是**为这一行**升版。
    assert_eq!(SCHEMA_VERSION, 7);
}
