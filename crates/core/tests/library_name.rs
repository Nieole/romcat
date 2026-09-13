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

use std::path::Path;

use romcat_core::catalog::{Catalog, CatalogError, EntryRecord, SCHEMA_VERSION, Verdict};
use romcat_core::fs::{EntryKind, EntryMeta};
use romcat_core::site::Site;
use romcat_core::testing::{self, Revoke, temp_dir};
use romcat_core::verdict::{Anchor, Membership, Store};
use romcat_core::workspace::{self, Slug};

#[test]
fn 建库之前问得出会被拦下的那几样而且一个字节都不碰() {
    // 挂单 `Q524`：添加主库那条向导第一步从前自己判空白、自己问一句那个文件在不在，撞上
    // 主库原名要到「开始扫描」那一下才由建库入口拦下。现在它问核心库这一处
    // （`Catalog::refuse_create`）：名字收不收与建库问的是同一个判断，而且**问一遍一个字节
    // 都不碰**——点「开始扫描」之前工作目录里一个文件都不多（词表**添加主库**）。
    let 工作目录 = temp_dir("原名-建库之前");
    let 新的 = |名字: &str| workspace::catalog_path(工作目录.path(), Slug::Named(名字));

    Catalog::refuse_create(&新的("我的主库"), "我的主库").expect("空的工作目录里起一个新名字该收");
    assert!(
        !工作目录.path().join("catalog").exists(),
        "问了一遍就把目录建出来了"
    );

    for 空的 in ["", "   "] {
        let 错 = Catalog::refuse_create(&新的(空的), 空的).expect_err("空白名字该拦下");
        assert!(
            matches!(错, CatalogError::BlankLibraryName { .. }),
            "报的不是「名字是空白」那一句：{错:?}"
        );
    }

    // 撞上一份**改过名**的库的主库原名：主库标识没撞，撞的是原名。
    let 原先那份 = 新的("起错了的名字");
    drop(Catalog::create(&原先那份, "起错了的名字").expect("能建中立库"));
    Catalog::open(&原先那份)
        .expect("能再打开")
        .set_library_name("我的主库")
        .expect("改得了名");
    let 错 = Catalog::refuse_create(&新的("我的主库"), "我的主库")
        .expect_err("撞了同一个工作目录里另一份库的主库原名，该拦下");
    assert!(
        matches!(错, CatalogError::LibraryNameTaken { .. }),
        "报的不是「这个名字已经有库在用」那一句：{错:?}"
    );

    // 反过来：原名没撞，**主库标识**撞了——拿那份库建库时的名字再建一份，折出来的是同一个文件。
    let 错 =
        Catalog::refuse_create(&原先那份, "起错了的名字").expect_err("那个文件已经在了，该拦下");
    assert!(
        matches!(错, CatalogError::AlreadyExists { .. }),
        "报的不是「那份库已经在了」那一句：{错:?}"
    );

    assert_eq!(
        workspace::catalogs(工作目录.path())
            .entries()
            .expect("列得开")
            .len(),
        1,
        "问了几遍，工作目录里却多出了库"
    );
}

#[test]
fn 工作目录写不动时建库之前就说清是写不动() {
    // 票 04：添加主库那条路正要往工作目录里写，目录写不动就该当场说清，而不是等到「开始扫描」
    // 那一下才撞上一句「建不出来」。**问的时候一个字节都不写**：问的是系统答不答应写，
    // 不是真去写一个试试。
    //
    // 造的是**真的**写不动的目录；造不出来（不是 Unix、或者跑测试的是 root）就如实跳过。
    // Windows 上这一问本来就答不出来，交给建库那一下自己报错（挂单 `Q611`）。
    let 工作目录 = temp_dir("原名-收了写权限");
    let Some(_还回去) = testing::revoke(工作目录.path(), Revoke::Write) else {
        return;
    };
    let 新的 = workspace::catalog_path(工作目录.path(), Slug::Named("我的主库"));

    let 错 =
        Catalog::refuse_create(&新的, "我的主库").expect_err("写不动的工作目录里建库，该当场拦下");

    assert!(
        matches!(错, CatalogError::DirUnwritable(_)),
        "报的不是「写不动」那一句：{错:?}"
    );
    let 说的 = format!("{错}");
    assert!(说的.contains("写不动"), "那句话没说是写不动：{说的}");
    // 中立库住的那个目录还没建出来，拦住它的是最近那一级已经在的上级——工作目录本身。
    assert!(
        说的.contains(&romcat_core::path::display(工作目录.path())),
        "那句话没说清是哪个目录写不动：{说的}"
    );
    // 「问一遍一个字节都不写」在写不动的目录上验不出来（本来就写不进去），由
    // `建库之前问得出会被拦下的那几样而且一个字节都不碰` 在写得动的工作目录上钉着。
}

#[test]
fn 中立库住的那个目录列不开时查不了重名就不收() {
    // ADR-0021 那条修订：**列不开不是空的**。「这个名字撞没撞」要把同一个目录里每一份库
    // 都看一眼，列不开就答不出来——从前当成「没撞上」放过去，那正是把读不动说成空的。
    // 建库入口、建库之前那一问与改名是同一处判断，三个都钉（挂单 `Q612`）。
    let 工作目录 = temp_dir("原名-收了读权限");
    let 已有的 = workspace::catalog_path(工作目录.path(), Slug::Named("我的主库"));
    drop(Catalog::create(&已有的, "我的主库").expect("能建中立库"));
    // 改名那一路要一份**开着的**库：收权限之前开好，收完就开不了了。它比收权限那个守卫先声明，
    // 于是守卫先丢、权限先还回去，这份库再关。
    let 开着的 = Catalog::open(&已有的).expect("能再打开");
    let 目录 = 已有的.parent().expect("有那个目录").to_path_buf();
    let Some(_还回去) = testing::revoke(&目录, Revoke::Read) else {
        return;
    };
    let 新的 = workspace::catalog_path(工作目录.path(), Slug::Named("另一个名字"));

    for (哪一下, 结果) in [
        (
            "建库之前那一问",
            Catalog::refuse_create(&新的, "另一个名字"),
        ),
        ("建库", Catalog::create(&新的, "另一个名字").map(drop)),
        ("改名", 开着的.set_library_name("另一个名字")),
    ] {
        let 错 = 结果.expect_err("列不开的目录里查不了重名，该拦下");
        assert!(
            matches!(错, CatalogError::DirUnreadable(_)),
            "{哪一下}报的不是「读不动」那一句：{错:?}"
        );
        let 说的 = format!("{错}");
        assert!(
            说的.contains("读不动"),
            "{哪一下}那句话没说是读不动：{说的}"
        );
    }
}

#[test]
fn 建库时原名落进元数据表再开一次读得回来() {
    let 工作目录 = temp_dir("原名-建库");
    let slug = Slug::Named("主库");
    let 库文件 = workspace::catalog_path(工作目录.path(), slug);

    // **建库是一个明说的动作，名字是必填的**（挂单 `Q371`）：不给名字就编不过，于是不会有
    // 一份建出来却没记住名字的库。这一趟把名字一并写下。
    drop(Catalog::create(&库文件, &slug.display_name()).expect("能建中立库"));

    let catalog = Catalog::open(&库文件).expect("能再打开");
    assert_eq!(catalog.library_name(), "主库");
}

#[test]
fn 建库时那份库已经在了就报错原先那一份一个字不改() {
    // **建库不是打开**：那个文件已经在了，说明这份主库早就建过——顺手开它的话，给的这个
    // 名字要么被悄悄丢掉，要么把人家的名字改掉，两样都不是「建库」。
    let 工作目录 = temp_dir("建库-已经在了");
    let slug = Slug::Named("主库");
    let 库文件 = workspace::catalog_path(工作目录.path(), slug);
    {
        let mut catalog = Catalog::create(&库文件, "主库").expect("能建中立库");
        写一条(&mut catalog);
    }

    let 结果 = Catalog::create(&库文件, "另起的名字");

    let Err(错) = 结果 else {
        panic!("那份库已经在了，建库却成了");
    };
    assert!(
        matches!(错, CatalogError::AlreadyExists { .. }),
        "报的不是「这份库已经在了」那一句：{错:?}"
    );
    let catalog = Catalog::open(&库文件).expect("原先那一份照样打得开");
    assert_eq!(catalog.library_name(), "主库", "报了错却动了原先那份的名字");
    assert!(
        catalog.contains("库/FC/魂斗罗.zip").expect("读得出来"),
        "报了错却动了原先那份库里的东西"
    );
}

#[test]
fn 票01之前建的库读不到那一行时退回从文件名截() {
    // 那些库的元数据表里压根没有这一行，而它们**不会被改**——旧库拿新程序打开照样能用
    // （中立库的结构版本没有为这一行加 1）。于是「这份主库叫什么」只能从文件名截，
    // 截出来的是人认得出的那一半，不是那串哈希。
    let 工作目录 = temp_dir("原名-旧库");
    let slug = Slug::Named("我的库");
    let 库文件 = workspace::catalog_path(工作目录.path(), slug);

    // 票 01 之前那条路：建库时没人告诉它这份主库叫什么，元数据表里压根没有那一行。
    testing::catalog_without_name(&库文件);
    写一条(&mut Catalog::open(&库文件).expect("能再打开"));

    // 新程序打开同一份：版本对得上（对不上 `open` 当场就拒），库里的东西一条不少，
    // 名字退回从文件名截。**打开不会顺手把名字补进去**——那一行只在建库那一趟落。
    let catalog = Catalog::open(&库文件).expect("旧库照样打得开");
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
        drop(Catalog::create(&库文件, &slug.display_name()).expect("能建中立库"));

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
    drop(Catalog::create(&库文件, &slug.display_name()).expect("能建中立库"));

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
    // 自己操作的是同一份库了。建库那一侧不在这一条里——建库只有 `Catalog::create` 一个
    // 入口，命令行 `romcat scan` 与界面添加主库那条向导调的都是它（挂单 `Q371`）。
    //
    // 名字挑一个**折进文件名会变形**的：`。、？` 全被滤光，文件名退成 `library-…`。
    // 于是「两边一样」不是因为两边都在读文件名。
    let 工作目录 = temp_dir("原名-两条路");
    for 原名 in ["主库", "。、？"] {
        let slug = Slug::Named(原名);
        let 库文件 = workspace::catalog_path(工作目录.path(), slug);
        drop(Catalog::create(&库文件, &slug.display_name()).expect("能建中立库"));

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
    drop(Catalog::create(&库文件, &slug.display_name()).expect("能建中立库"));

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
        let mut catalog = Catalog::create(&库文件, &起的名字.display_name()).expect("能建中立库");
        写一条(&mut catalog);
    }

    Catalog::open(&库文件)
        .expect("能再打开")
        .set_library_name("改过的名字")
        .expect("改得了名");

    let 列出来的 = workspace::catalogs(工作目录.path());
    let Ok([一份]) = 列出来的.entries() else {
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
    drop(Catalog::create(&库文件, &起的名字.display_name()).expect("能建中立库"));

    let 变体 = "主盘/FC/魂斗罗.zip";
    落一条路径锚(工作目录.path(), 锚里存的, 变体);

    Catalog::open(&库文件)
        .expect("能再打开")
        .set_library_name("改过的名字")
        .expect("改得了名");

    // 找库仍按**原先那个名字**：`--library` 折出来的是主库标识，改名不动它。
    for (路, site) in
        两个入口都认得那条路径锚(工作目录.path(), 起的名字, &库文件, 锚里存的, 变体)
    {
        assert_eq!(site.display_name(), "改过的名字", "{路}开出来的不是新名字");
    }
}

#[test]
fn 改名成空白时当场报错那一行与路径锚都没动() {
    // **空白不是名字**（挂单 `Q469` 的裁决）：改名框里清空了名字按确定，该听到一句说得清的
    // 「不能是空白」，而不是悄悄把名字抹掉、退回从文件名截。**报了错就什么都没改**：
    // 元数据表里还是报错之前那个名字，路径锚一个字节没动。
    //
    // 建库那一趟给空白名字眼下照旧不报错、退回从文件名截（下一条钉着），改它归票 `03`。
    const 锚里存的: &str = "主库-f5c61109e92b6036";
    let 工作目录 = temp_dir("改名-空白");
    let 起的名字 = Slug::Named("主库");
    let 库文件 = workspace::catalog_path(工作目录.path(), 起的名字);
    drop(Catalog::create(&库文件, &起的名字.display_name()).expect("能建中立库"));
    let 变体 = "主盘/FC/魂斗罗.zip";
    落一条路径锚(工作目录.path(), 锚里存的, 变体);
    Catalog::open(&库文件)
        .expect("能再打开")
        .set_library_name("改过的名字")
        .expect("改得了名");

    for 空的 in ["", "   ", "\t\n"] {
        let 结果 = Catalog::open(&库文件)
            .expect("能再打开")
            .set_library_name(空的);
        let Err(错) = 结果 else {
            panic!("空白名字 {空的:?} 该当场报错，却改成了");
        };
        assert!(
            matches!(错, CatalogError::BlankLibraryName { .. }),
            "报的不是「名字是空白」那一句：{错:?}"
        );
        assert!(format!("{错}").contains("空白"), "那句话没说清是空白：{错}");
        assert_eq!(
            Catalog::open(&库文件).expect("能再打开").library_name(),
            "改过的名字",
            "报了错却动了元数据表那一行"
        );
    }

    let 列出来的 = workspace::catalogs(工作目录.path());
    assert_eq!(
        列出来的
            .entries()
            .expect("列得开")
            .iter()
            .map(|一份| 一份.name.as_str())
            .collect::<Vec<_>>(),
        ["改过的名字"],
        "开场屏上该还是报错之前那个名字"
    );
    for (路, site) in
        两个入口都认得那条路径锚(工作目录.path(), 起的名字, &库文件, 锚里存的, 变体)
    {
        assert_eq!(site.display_name(), "改过的名字", "{路}开出来的名字被动过");
    }
}

#[test]
fn 同一个工作目录里主库原名重了建库当场报错不另建一份() {
    // **改名只换主库原名，找库仍认主库标识**（挂单 `Q472`）。于是改名之后拿新名字去建库，
    // 折出来的是另一个文件——从前那一下会顺手**另建一份**，开场屏上两行同名，人分不出
    // 哪份是哪份。现在建库那一步查同一个工作目录里的主库原名，撞上就报错，说清撞的是哪一份。
    let 工作目录 = temp_dir("原名-建库撞名");
    let 起的名字 = Slug::Named("主库");
    let 原先那份 = workspace::catalog_path(工作目录.path(), 起的名字);
    drop(Catalog::create(&原先那份, &起的名字.display_name()).expect("能建中立库"));
    Catalog::open(&原先那份)
        .expect("能再打开")
        .set_library_name("改过的名字")
        .expect("改得了名");

    let 新名字 = Slug::Named("改过的名字");
    let 另一个文件 = workspace::catalog_path(工作目录.path(), 新名字);
    assert_ne!(另一个文件, 原先那份, "前提：新名字折出来的是另一个文件");
    let 结果 = Catalog::create(&另一个文件, &新名字.display_name());

    let Err(错) = 结果 else {
        panic!("主库原名撞上了同一个工作目录里的另一份库，建库却成了");
    };
    assert!(
        matches!(错, CatalogError::LibraryNameTaken { .. }),
        "报的不是「这个名字已经有库在用」那一句：{错:?}"
    );
    let 说的 = format!("{错}");
    assert!(说的.contains("改过的名字"), "没说清撞的是哪个名字：{说的}");
    assert!(
        说的.contains(&romcat_core::path::display(&原先那份)),
        "没说清撞上的是哪一份库：{说的}"
    );
    assert!(!另一个文件.exists(), "报了错却把另一份库建出来了");
    assert_eq!(
        workspace::catalogs(工作目录.path())
            .entries()
            .expect("列得开")
            .len(),
        1,
        "报了错，开场屏上却多出一行"
    );

    // **管的是同一个工作目录**：换一个工作目录等于换一整套工具状态，那边同名不算撞。
    let 另一个工作目录 = temp_dir("原名-建库撞名-另一个工作目录");
    drop(
        Catalog::create(
            &workspace::catalog_path(另一个工作目录.path(), 新名字),
            &新名字.display_name(),
        )
        .expect("另一个工作目录里同名照样建得出"),
    );
}

#[test]
fn 改名成同一个工作目录里另一份库的主库原名时当场报错() {
    // 建库与改名问的是**同一处判断**（ADR-0024）：只在建库那一步查，改名就能把两份库改成
    // 同一个名字，开场屏上照样两行同名。改成自己眼下这个名字不算撞。
    let 工作目录 = temp_dir("原名-改名撞名");
    let 甲 = workspace::catalog_path(工作目录.path(), Slug::Named("甲库"));
    let 乙 = workspace::catalog_path(工作目录.path(), Slug::Named("乙库"));
    drop(Catalog::create(&甲, "甲库").expect("能建甲库"));
    drop(Catalog::create(&乙, "乙库").expect("能建乙库"));

    let 结果 = Catalog::open(&乙)
        .expect("能再打开")
        .set_library_name("甲库");

    let Err(错) = 结果 else {
        panic!("改成了另一份库的名字，却改成了");
    };
    assert!(
        matches!(错, CatalogError::LibraryNameTaken { .. }),
        "报的不是「这个名字已经有库在用」那一句：{错:?}"
    );
    assert_eq!(
        Catalog::open(&乙).expect("能再打开").library_name(),
        "乙库",
        "报了错却动了元数据表那一行"
    );
    Catalog::open(&乙)
        .expect("能再打开")
        .set_library_name("乙库")
        .expect("改成自己眼下这个名字不算撞");
}

#[test]
fn 改过名之后按新名字找库说清那个名字是哪一份库的主库原名() {
    // **找库认的是主库标识，改名不动它**（挂单 `Q472`）。按新名字折出来的是另一个文件，找不到
    // 是对的；可只说一句「还没有这份中立库，先跑一次 `romcat scan`」，人照做就撞上建库那一步
    // 的撞名报错，绕一步才知道原因。命令行 `--library` 与界面按名字启动都走这一句。
    let 工作目录 = temp_dir("原名-改名后找库");
    let 起的名字 = Slug::Named("起错了的名字");
    let 原先那份 = workspace::catalog_path(工作目录.path(), 起的名字);
    drop(Catalog::create(&原先那份, &起的名字.display_name()).expect("能建中立库"));
    Catalog::open(&原先那份)
        .expect("能再打开")
        .set_library_name("改过的名字")
        .expect("改得了名");

    let 结果 = Site::open(
        工作目录.path(),
        Slug::Named("改过的名字"),
        None,
        "--library 改过的名字",
    );

    let Err(错) = 结果 else {
        panic!("按新名字折出来的是另一个文件，却开出了现场");
    };
    let 说的 = format!("{错}");
    assert!(
        说的.contains("主库原名叫「改过的名字」"),
        "没说清这个名字是哪一份库的主库原名：{说的}"
    );
    assert!(
        说的.contains(&romcat_core::path::display(&原先那份)),
        "没说清是哪一份库：{说的}"
    );
    assert!(
        !workspace::catalog_path(工作目录.path(), Slug::Named("改过的名字")).exists(),
        "找库顺手建出了一份"
    );
}

#[test]
fn 建库时名字是空白就当场报错盘上一个文件都不多() {
    // **空白不是名字**（挂单 `Q469` 的裁决）：从前 `--library ""` 建得出一份库、名字退回从
    // 文件名截，于是「不会有一份建出来却没记住名字的库」只是一句愿望。现在建库与改名是
    // 同一处判断，空白（含全是空白字符）当场报错——**报了错就什么都没建**，连中立库住的
    // 那个目录都不多。
    let 工作目录 = temp_dir("原名-空名字");
    for 空的 in ["", "   ", "\t\n"] {
        let slug = Slug::Named(空的);
        let 库文件 = workspace::catalog_path(工作目录.path(), slug);

        let 结果 = Catalog::create(&库文件, &slug.display_name());

        let Err(错) = 结果 else {
            panic!("空白名字 {空的:?} 该当场报错，却建出了库");
        };
        assert!(
            matches!(错, CatalogError::BlankLibraryName { .. }),
            "报的不是「名字是空白」那一句：{错:?}"
        );
        assert!(format!("{错}").contains("空白"), "那句话没说清是空白：{错}");
        assert!(
            std::fs::read_dir(工作目录.path())
                .expect("读得了工作目录")
                .next()
                .is_none(),
            "空白名字报了错，工作目录里却多出了东西"
        );
    }
}

#[test]
fn 打开一份不存在的中立库当场报错盘上一个文件都不多() {
    // **打开不是建库**（挂单 `Q371`）。「打开即创建」的代价是每个调用方都得自己先问一句
    // 「在不在」——漏问一处，一条打错的名字就在工作目录里留下一份没记过名字的空库，
    // 从此占着开场屏的一行。连中立库住的那个目录都不许顺手建出来。
    let 工作目录 = temp_dir("打开-不在");
    let 库文件 = workspace::catalog_path(工作目录.path(), Slug::Named("压根没建过"));

    let 结果 = Catalog::open(&库文件);

    let Err(错) = 结果 else {
        panic!("打开一份不存在的中立库该报错，却开出来了");
    };
    assert!(
        matches!(错, CatalogError::Missing { .. }),
        "报的不是「这份库不在」那一句：{错:?}"
    );
    assert!(
        std::fs::read_dir(工作目录.path())
            .expect("读得了工作目录")
            .next()
            .is_none(),
        "打开失败了，工作目录里却多出了东西"
    );
}

#[test]
fn 打开一份不是建好的中立库的文件报错一个字节都不写() {
    // **打开不建库**，对一个已经在盘上、却不是建好的中立库的文件也一样：一个空文件、或者建到
    // 一半断了的那一份，从前 `open` 会给它建表、写上结构版本，把它变成一份没记过名字的库。
    // 现在当场报错，那个文件一个字节不动，SQLite 的附件（`-wal` / `-shm`）也不多。
    let 工作目录 = temp_dir("打开-空文件");
    let 库文件 = workspace::catalog_path(工作目录.path(), Slug::Named("空文件"));
    let 目录 = 库文件.parent().expect("有上级目录").to_path_buf();
    std::fs::create_dir_all(&目录).expect("建得出目录");
    std::fs::write(&库文件, b"").expect("写得出空文件");

    let 结果 = Catalog::open(&库文件);

    assert!(结果.is_err(), "一个空文件却开成了中立库");
    drop(结果);
    assert_eq!(
        std::fs::metadata(&库文件).expect("文件还在").len(),
        0,
        "打开失败了，却往那个文件里写了东西"
    );
    assert_eq!(
        std::fs::read_dir(&目录).expect("读得了目录").count(),
        1,
        "打开失败了，目录里却多出了 SQLite 的附件"
    );
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
    testing::catalog_without_name(&库文件);
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

/// 往沉淀库里落一条收藏，锚在「主库标识 `标识` 这份主库里的 `变体`」这条**路径锚**上。
fn 落一条路径锚(工作目录: &Path, 标识: &str, 变体: &str) {
    let mut store = Store::open(&workspace::verdict_store_path(工作目录)).expect("开得出沉淀库");
    store
        .join(&[Membership::now(
            "收藏",
            Anchor::Path {
                library: 标识.to_string(),
                variant_key: 变体.to_string(),
            },
        )])
        .expect("写得进沉淀库");
}

/// 按名字（命令行 `--library`）与按文件（界面开场屏）两个入口各开一份现场：断言两份的
/// **主库标识**逐字节是 `标识`、都认得出 `落一条路径锚` 落下的那一条，再把两份现场交回去。
fn 两个入口都认得那条路径锚(
    工作目录: &Path,
    起的名字: Slug<'_>,
    库文件: &Path,
    标识: &str,
    变体: &str,
) -> Vec<(&'static str, Site)> {
    let 现场 = vec![
        (
            "按名字",
            Site::open(工作目录, 起的名字, None, "--library").expect("开得出现场"),
        ),
        (
            "按文件",
            Site::open_file(工作目录, 库文件, None).expect("开得出现场"),
        ),
    ];
    for (路, site) in &现场 {
        assert_eq!(site.library_identity, 标识, "{路}开出来的主库标识变了");
        let 锚 = Anchor::Path {
            library: site.library_identity.clone(),
            variant_key: 变体.to_string(),
        };
        assert_eq!(
            site.store.joined(&锚).expect("读得出沉淀库"),
            ["收藏"],
            "{路}开出来的现场认不出那条路径锚"
        );
    }
    现场
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
