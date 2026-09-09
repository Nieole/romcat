//! **列出一个工作目录里有哪些中立库**。
//!
//! 核心库此前一个列举函数都没有：所有开库的路都是「给我名字或路径，我去折一个出来看
//! 在不在」（`workspace::catalog_path` + `Site::open`）。**开场**那一屏要的正相反——
//! 人还说不出名字，得先看见这个**工作目录**里有些什么（ADR-0023）。

use romcat_core::catalog::roots::{self, RootScan};
use romcat_core::catalog::{Catalog, SCHEMA_VERSION};
use romcat_core::platform::Manifest;
use romcat_core::shape::{Role, SINGLE_FILE_RULE, Variant};
use romcat_core::testing::{self, temp_dir};
use romcat_core::workspace::{self, Slug};

/// 在这个工作目录里现建一份中立库，返回它的文件路径。
fn 建一份(工作目录: &std::path::Path, 名字: &str) -> std::path::PathBuf {
    let slug = Slug::Named(名字);
    let 库文件 = workspace::catalog_path(工作目录, slug);
    drop(Catalog::open_named(&库文件, &slug.display_name()).expect("能建中立库"));
    库文件
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

    let 名字: Vec<String> = workspace::catalogs(工作目录.path())
        .into_iter()
        .map(|一份| 一份.name)
        .collect();

    // 按**主库名**排：目录列出来的次序是文件系统给的，两次打开不保证一样，而开场那一屏
    // 的行不该自己跳来跳去。名字比的是**码位**（这个仓库不做拼音排序），于是「乙库」
    // 排在「甲库」前头——排得死板不要紧，要紧的是每次打开都是同一个次序。
    assert_eq!(名字, ["乙库", "甲库"]);
}

#[test]
fn 变体数与上次扫描时刻盘没挂上照样交得出() {
    // 这三个数（主库名、变体数、上次扫描时刻）住在**中立库**里，不在主库上（ADR-0009）。
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

    let 列出来的 = workspace::catalogs(工作目录.path());
    let [一份] = 列出来的.as_slice() else {
        panic!("该只有一份库：{列出来的:?}");
    };
    assert!(一份.openable, "这一份该开得进去");
    let 那几个数 = 一份.facts.as_ref().expect("这一份读得开");
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

    let 列出来的 = workspace::catalogs(工作目录.path());
    assert_eq!(
        列出来的.len(),
        2,
        "打不开的那一份不许从列表里消失：{列出来的:?}"
    );

    let 那一份 = 列出来的
        .iter()
        .find(|一份| 一份.path == 旧的)
        .expect("版本对不上的那一份照样列出来了");
    assert!(!那一份.openable, "版本对不上的那一份不该是「开得进去」");
    let 说的 = 那一份.facts.as_ref().expect_err("这一份开不出来");
    assert!(
        说的.contains("结构版本是 4"),
        "没说清库里是哪个版本：{说的}"
    );
    assert!(
        说的.contains(&format!("本程序认得的是 {SCHEMA_VERSION}")),
        "没说清本程序认哪个版本：{说的}",
    );
    assert!(说的.contains("删掉它重扫一遍"), "没说该怎么办：{说的}");

    // **开不了的那一份也得有名字**：从文件名截，剥掉哈希后缀。
    assert_eq!(那一份.name, "版本对不上的库");

    // 别的照列，一份打不开不连累其余。
    let 好那一份 = 列出来的
        .iter()
        .find(|一份| 一份.path == 好的)
        .expect("好的那一份照列");
    assert!(
        好那一份.openable && 好那一份.facts.is_ok(),
        "好的那一份被连累了：{好那一份:?}"
    );
}

#[test]
fn 主库名读不到时退回从文件名截既不空着也不是那串哈希() {
    // 票 01 之前建的库、以及拿 `Catalog::open` 建的库都不知道自己叫什么。退路在
    // `Catalog::library_name` 里，这一条钉的是**列举这一层真的走了那条退路**——
    // 开场那一屏上一行空白或者一串十六进制，人都认不出那是自己的哪份库。
    let 工作目录 = temp_dir("列举-没名字");
    let slug = Slug::Named("没记过名字的库");
    let 库文件 = workspace::catalog_path(工作目录.path(), slug);
    drop(Catalog::open(&库文件).expect("能建中立库"));

    let 列出来的 = workspace::catalogs(工作目录.path());
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

    let 列出来的 = workspace::catalogs(工作目录.path());
    assert_eq!(列出来的.len(), 2, "整批塌了：{列出来的:?}");
    let 那一份 = 列出来的
        .iter()
        .find(|一份| 一份.path == 坏的)
        .expect("坏的那一份照样列出来了");
    assert!(!那一份.openable, "开不进去的那一份不该是「开得进去」");
    assert!(那一份.facts.is_err(), "读不出来的却报了一份数：{那一份:?}");
    assert_eq!(那一份.name, "坏的库", "开不了也得有个认得出的名字");
    assert!(
        列出来的
            .iter()
            .any(|一份| 一份.openable && 一份.facts.is_ok()),
        "好的那一份被连累了：{列出来的:?}",
    );
}

#[test]
fn 一份中立库都没有时列出来是空的而且不留下任何东西() {
    // 开场那一屏那句「还没有库，认领一个主库开始」由它撑着。**列一遍不许留痕**：
    // 只是想看看有哪些库，不该顺手把目录建出来，更不该建出一份空库。
    let 工作目录 = temp_dir("列举-空的");
    assert!(workspace::catalogs(工作目录.path()).is_empty());
    assert!(
        !工作目录.path().join("catalog").exists(),
        "列一遍把目录建出来了",
    );
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

    let 列出来的 = workspace::catalogs(工作目录.path());
    let [一份] = 列出来的.as_slice() else {
        panic!("旁边的东西被当成库了：{列出来的:?}");
    };
    assert_eq!(一份.path, 库文件);
}
