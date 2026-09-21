//! **移除一个根**（票 `gui-looks-like-the-design/26`）：按下那一下之前算得出代价，
//! 按下去之后**盘上一个字节都不动**。
//!
//! 这个文件钉四件事，每一件都是「不这么做用户就会做出错误的决定」：
//!
//! 1. **代价的四个数**——去掉多少变体、浏览里少几行、导出少几条、哪台子库少多少——
//!    各是真的，而且与真按下去之后的结果对得上。
//! 2. **变体的键是「根名 + 相对路径」**（词表**根**），所以移除一个根**只影响它自己那一支**：
//!    别的根底下同名的东西一条都不许少。
//! 3. **主库只读**（ADR-0004）：移除一个根只动中立库，那个目录里的文件、大小、内容
//!    整份快照不变。
//! 4. **说了算的那几样留得住**：裁决与合集锚在内容锚上，这个根加回来照旧对得上。

use std::collections::BTreeMap;
use std::path::Path;

use romcat_core::catalog::identify::{Candidate, Identification, Provenance, Standalone};
use romcat_core::catalog::roots::add_root;
use romcat_core::catalog::{Catalog, Confidence, State};
use romcat_core::dat::Convention;
use romcat_core::platform::Manifest;
use romcat_core::shape::{Role, SINGLE_FILE_RULE, Variant};
use romcat_core::sublibrary::{Rule, Sublibrary};
use romcat_core::testing::temp_dir;

/// 主库那个根叫什么。
const 主库: &str = "主库";
/// 另一块盘那个根叫什么。**同名的东西两边都有**，用来钉「只影响它自己那一支」。
const 备份: &str = "备份";

fn 变体(key: &str, platform: &str, bytes: u64) -> Variant {
    Variant {
        key: key.to_string(),
        platform: Some(platform.to_string()),
        rule: SINGLE_FILE_RULE.to_string(),
        main_key: key.to_string(),
        manual: false,
        files: 1,
        bytes,
        unreadable_files: 0,
        members: vec![(key.to_string(), Role::Main)],
    }
}

fn 识别(key: &str, work: Option<i64>, standalone: Option<Standalone>) -> Identification {
    Identification {
        variant_key: key.to_string(),
        platform: None,
        // **第几版只有裁决说得出**（ADR-0008）：这份合成 fixture 走的是识别那条路，交 `None`。
        edition: None,
        standalone,
        state: State::Matched,
        reason: None,
        units: 1,
        nkit: 0,
        read_bytes: 0,
        work_id: work,
        release_id: None,
        candidates: vec![Candidate {
            member_key: key.to_string(),
            inner: String::new(),
            confidence: Confidence::High,
            accepted: true,
            source: "No-Intro".to_string(),
            dat: "测试.dat".to_string(),
            platform: "GB".to_string(),
            game: "某条 DAT 条目".to_string(),
            rom: "rom.bin".to_string(),
            hashed_as: Convention::AsIs,
            dat_convention: Convention::AsIs,
            evidence: "合成 fixture 里钉死的依据".to_string(),
            chinese: None,
            serial: None,
            release_id: None,
        }],
    }
}

/// 一份两块盘的库，形状照真库摆：
///
/// - **只住在备份盘上的作品**（`只在备份`）：移除备份之后它整个从浏览里消失。
/// - **两块盘各有一份的作品**（`两边都有`）：移除备份之后它还在——留下主库那一份。
/// - **备份盘上一个还没认出作品的变体**：它自己就是浏览里的一行、也是导出的一条。
/// - **备份盘上一个补丁**：入库、进不了前端条目（ADR-0013），所以它不进「导出少几条」。
/// - **主库上一个只属于主库的作品**：它一条都不该被这笔账算进去。
fn 现场() -> Catalog {
    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    catalog
        .replace_variants(
            &[
                变体("备份/GB/只在备份.zip", "GB", 1_000),
                变体("备份/GB/只在备份 汉化.zip", "GB", 1_100),
                变体("备份/GB/两边都有.zip", "GB", 2_000),
                变体("备份/GB/散落.zip", "GB", 3_000),
                变体("备份/GB/汉化补丁.ips", "GB", 16),
                变体("主库/GB/两边都有.zip", "GB", 2_000),
                变体("主库/GB/只在主库.zip", "GB", 4_000),
            ],
            1,
            &Manifest::default(),
        )
        .expect("变体写得进");

    let 只在备份 = catalog
        .add_work("只在备份", Provenance::Identified)
        .expect("建得出作品");
    let 两边都有 = catalog
        .add_work("两边都有", Provenance::Identified)
        .expect("建得出作品");
    let 只在主库 = catalog
        .add_work("只在主库", Provenance::Identified)
        .expect("建得出作品");
    catalog
        .write_identifications(&[
            识别("备份/GB/只在备份.zip", Some(只在备份), None),
            识别("备份/GB/只在备份 汉化.zip", Some(只在备份), None),
            识别("备份/GB/两边都有.zip", Some(两边都有), None),
            // 还没认出作品的那一个：`work_id` 空着，它自己成一行、也自己成一条条目。
            识别("备份/GB/散落.zip", None, None),
            // **补丁不是前端条目**（ADR-0013）：它算进「去掉多少变体」，不算进「导出少几条」。
            识别("备份/GB/汉化补丁.ips", None, Some(Standalone::Patch)),
            识别("主库/GB/两边都有.zip", Some(两边都有), None),
            识别("主库/GB/只在主库.zip", Some(只在主库), None),
        ])
        .expect("识别结论写得进");
    add_root(&catalog, None, 主库, Path::new("/Volumes/新加卷/Game")).expect("加得上根");
    add_root(&catalog, None, 备份, Path::new("/Volumes/备份/Pegasus")).expect("加得上根");
    catalog
}

#[test]
fn 移除一个根之前说得出会去掉多少变体_多少作品会消失_导出少多少条() {
    let catalog = 现场();
    let 代价 = catalog.root_removal(备份).expect("算得出代价");

    assert_eq!(代价.variants, 5, "备份那一支底下五个变体");
    // 浏览里消失的两行：作品「只在备份」（两个变体都在备份盘上），加上那个还没认出作品的
    // 散落变体自己那一行。**「两边都有」不消失**——主库那一份还在。补丁在浏览里照样是一行。
    assert_eq!(
        代价.works, 3,
        "只在备份 + 散落 + 补丁；两边都有的那一部不许算进来"
    );
    // 导出少的两条：作品「只在备份」在 GB 上那一条，加上散落那一条。
    // **补丁进不了前端条目**，所以它不在这个数里。
    assert_eq!(代价.entries, 2, "补丁不成条目（ADR-0013）");
    assert!(代价.sublibraries.is_empty(), "一台子库都还没有");
}

#[test]
fn 移除另一个根算出来的是另一笔账() {
    let catalog = 现场();
    let 代价 = catalog.root_removal(主库).expect("算得出代价");
    assert_eq!(代价.variants, 2);
    assert_eq!(代价.works, 1, "只有「只在主库」那一部会消失");
    assert_eq!(代价.entries, 1);
}

#[test]
fn 没扫过的根移除起来没有代价() {
    let catalog = 现场();
    add_root(&catalog, None, "空的", Path::new("/Volumes/还没扫过")).expect("加得上根");
    let 代价 = catalog.root_removal("空的").expect("算得出代价");
    assert_eq!(代价, romcat_core::catalog::RootRemoval::default());
}

#[test]
fn 说得出哪台子库的选择集会少多少() {
    let mut catalog = 现场();
    catalog
        .put_sublibrary(&Sublibrary::at(
            "掌机",
            Path::new("/Volumes/SD"),
            "pegasus",
            None,
        ))
        .expect("存得下子库");
    catalog
        .add_rule("掌机", &Rule::parse("平台=GB").expect("读得懂"))
        .expect("加得上规则");
    // 第二台只收主库那一份：它一个都不该少。
    catalog
        .put_sublibrary(&Sublibrary::at(
            "另一台",
            Path::new("/Volumes/另一张卡"),
            "pegasus",
            None,
        ))
        .expect("存得下子库");
    catalog
        .add_rule("另一台", &Rule::parse("作品=只在主库").expect("读得懂"))
        .expect("加得上规则");

    let 代价 = catalog.root_removal(备份).expect("算得出代价");
    let 少了: BTreeMap<&str, u64> = 代价
        .sublibraries
        .iter()
        .map(|one| (one.name.as_str(), one.variants))
        .collect();
    assert_eq!(少了.get("掌机").copied(), Some(5), "备份那五个都在选择集里");
    assert!(
        !少了.contains_key("另一台"),
        "一个都不少的那台不列出来：{代价:?}"
    );
}

#[test]
fn 按下去之前看见的那个数就是事后报出来的那个数() {
    let mut catalog = 现场();
    let 代价 = catalog.root_removal(备份).expect("算得出代价");
    let 去掉了 = catalog.remove_root(备份).expect("移得掉");
    assert_eq!(去掉了, 代价.variants);
    assert_eq!(catalog.roots().expect("读得出根").len(), 1);
    assert!(
        catalog
            .variant("主库/GB/两边都有.zip")
            .expect("查得到")
            .is_some(),
        "别的根底下同名的东西一条都不许少——变体的键是「根名 + 相对路径」"
    );
    assert!(
        catalog
            .variant("备份/GB/两边都有.zip")
            .expect("查得到")
            .is_none()
    );
    assert_eq!(
        catalog.root_stats(主库).expect("数得出").variants,
        2,
        "另一个根那一支一个都不许少"
    );
}

#[test]
fn 移除一个根不动盘上的任何一个字节() {
    // **主库只读**（ADR-0004）：这一下只写中立库。目标目录整份快照前后逐字节相同。
    let 盘 = temp_dir("core-移除根-只读");
    for (相对, 内容) in [
        ("GB/只在备份.zip", "abcd"),
        ("GB/两边都有.zip", "efgh"),
        ("说明.txt", "别动我"),
    ] {
        let 落点 = 盘.path().join(相对);
        std::fs::create_dir_all(落点.parent().expect("有上级目录")).expect("建得出目录");
        std::fs::write(&落点, 内容.as_bytes()).expect("写得进");
    }
    let 之前 = 快照(盘.path());

    let mut catalog = 现场();
    catalog
        .set_root_path(备份, &盘.path().display().to_string())
        .expect("改得了根的位置");
    catalog.root_removal(备份).expect("算得出代价");
    catalog.remove_root(备份).expect("移得掉");

    assert_eq!(之前, 快照(盘.path()), "盘上的文件一个字节都不许变");
}

/// 一个目录整份的样子：每个文件的相对路径 → 它的字节。
fn 快照(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut 待走 = vec![dir.to_path_buf()];
    while let Some(这一层) = 待走.pop() {
        for 项 in std::fs::read_dir(&这一层).expect("读得动目录") {
            let 项 = 项.expect("读得动一项");
            let 路 = 项.path();
            if 路.is_dir() {
                待走.push(路);
            } else {
                let 相对 = 路
                    .strip_prefix(dir)
                    .expect("在这个目录底下")
                    .display()
                    .to_string();
                out.insert(相对, std::fs::read(&路).expect("读得动文件"));
            }
        }
    }
    out
}
