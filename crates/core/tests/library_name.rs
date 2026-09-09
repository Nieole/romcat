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

use romcat_core::catalog::{Catalog, EntryRecord, SCHEMA_VERSION, Verdict};
use romcat_core::fs::{EntryKind, EntryMeta};
use romcat_core::site::Site;
use romcat_core::testing::temp_dir;
use romcat_core::workspace::{self, Slug};

#[test]
fn 建库时原名落进元数据表再开一次读得回来() {
    let 工作目录 = temp_dir("库名-建库");
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
    let 工作目录 = temp_dir("库名-旧库");
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
    let 工作目录 = temp_dir("库名-怪名字");
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
    let 工作目录 = temp_dir("库名-滤光");
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
    let 工作目录 = temp_dir("库名-两条路");
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
fn 名字是空白时退回从文件名截而不是留一行空的() {
    // `--library ""` 命令行不拦（改它的行为不在这张票里）。**空白不是名字**：那一行不落，
    // 读的时候也退回从文件名截，于是报告里不会印出一句光秃秃的「主库：」，
    // 开场那一屏也不会多一行没有名字的库。
    let 工作目录 = temp_dir("库名-空名字");
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
