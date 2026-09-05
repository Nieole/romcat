//! **Switch 那一层**：磁盘上摆一份 Switch 主库，跑一遍识别。
//!
//! 这里要证的是票 27 的那句话：**容器层的文件名表全是明文，免密钥就能识别到「哪个
//! 游戏的哪个版本」。** 密集逻辑挂在纯函数上（`identify::switch` 自己有一整套
//! 单元测试），这一份证的是它真的接在了扫描、成型、TitleID 索引与中立库之间。
//!
//! **样本里没有一个字节的真 NCA，也没有任何密钥**（`testing::switch`）——那正是这一层
//! 存在的理由：要读的东西全在容器头里。

use std::fs;
use std::path::Path;

use romcat_core::capability::{self, Roster};
use romcat_core::task::Handle;
use romcat_core::catalog::identify::State;
use romcat_core::catalog::{Catalog, Confidence, Roots};
use romcat_core::dat::repo::DatRepo;
use romcat_core::fs::RealFs;
use romcat_core::identify::fuzzy;
use romcat_core::identify::{self, Options};
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::testing::switch as sample;
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::titledb::store::Store as TitleDb;
use romcat_core::titledb::{Content, Title};
use romcat_core::verdict;

/// 本体的 TitleID。真机上那 82 个 `.nsp` 里相当一部分不是本体，所以三种都要摆。
const BASE: &str = "0100A0C01BED8000";
/// 同一部作品的更新包：尾 `800`，是一份**补丁**。
const PATCH: &str = "0100A0C01BED8800";
/// 同一部作品的 DLC：号段在本体之上一档，是一份**附属内容**。
const DLC: &str = "0100A0C01BED9001";

/// 本体那份 NSP 里的 ContentId。反查靠它。
const BASE_NCA: &str = "dfdb0f5bc5c5056a2f35d0379ee23020";
/// 更新那份 NSZ 里的 ContentId（压缩过的只改扩展名，词干原样保留）。
const PATCH_NCA: &str = "150cf9022bfb2e72527669f2701ee31b";

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

/// `<RightsId>.tik` 的文件名：TitleID ‖ 7 字节零 ‖ KeyGeneration。
fn 票据(title_id: &str) -> String {
    format!("{}00000000000000{}.tik", title_id.to_lowercase(), "10")
}

struct 现场 {
    dir: TempDir,
    catalog: Catalog,
    repo: DatRepo,
}

fn 建现场() -> 现场 {
    let dir = temp_dir("switch");
    let root = dir.path();

    // ── 本体：CDN 风格的 NSP（带 `.tik` 与 `.cert`），文件名里也写着 TitleID。
    写(
        &root.join(format!("switch/伊蘇X 北境歷險 [{BASE}][v0].nsp")),
        &sample::pfs0(&[
            &票据(BASE),
            &票据(BASE).replace(".tik", ".cert"),
            "8eed26260dbdb1ea545119cc0368fa06.cnmt.nca",
            &format!("{BASE_NCA}.nca"),
        ]),
    );
    // ── 更新：**压缩过的** NSZ。识别零解压——只改最后一个字符，词干原样保留。
    写(
        &root.join(format!("switch/伊蘇X 更新 [{PATCH}][v196608].nsz")),
        &sample::pfs0(&[
            "ba39a7f62eeb23476c08600ed405a1cf.cnmt.ncz",
            &format!("{PATCH_NCA}.ncz"),
            &票据(PATCH),
            &票据(PATCH).replace(".tik", ".cert"),
        ]),
    );
    // ── DLC：文件名里**一个 TitleID 都没写**（真库里 33.6% 的文件名含汉字，
    //    这一类多半已经把 `[TitleID]` 丢了）。容器照样说得出来。
    写(
        &root.join("switch/伊蘇X 追加曲包.nsp"),
        &sample::pfs0(&[
            &票据(DLC),
            &票据(DLC).replace(".tik", ".cert"),
            "00c3cb5d146efa3cc9f2cb83682cf483.cnmt.nca",
        ]),
    );
    // ── 卡带：scene 风格的 XCI，`secure` 分区摆在几百 KB 之后——**前缀读够不着，
    //    只能 seek**。它的 ContentId 查不到 titledb（卡带上的 NCA DistributionType
    //    不同、ContentId 也就不同，实测 isGameCard 只有 121 条）。
    写(
        &root.join(format!("switch/伊蘇X 卡带 [{BASE}][HK][v0].xci")),
        &sample::xci(
            0,
            &["update", "logo", "normal", "secure"],
            &[
                &票据(BASE),
                "2458773bdce28491e9e0e5043f86015a.nca",
                "f3345bfc3fae7e5302dde1e132c222a5.cnmt.nca",
            ],
        ),
    );
    // ── 名字与容器**对不上**：文件名写着别的 TitleID。极强的「被改过」信号。
    写(
        &root.join("switch/改过名的 [0100FFFFFFFFF000][v0].nsp"),
        &sample::pfs0(&[&票据(BASE), "8eed26260dbdb1ea545119cc0368fa06.cnmt.nca"]),
    );
    // ── 整合包：一个容器里装着不止一档内容（本体 + 一批 titledb 查不到的 NCA）。
    //    真机上那份 `(1G+1U+9D)(MOD14).xci` 就是这个形状。
    写(
        &root.join(format!("switch/伊蘇X 整合版 [{BASE}][v0].nsp")),
        &sample::pfs0(&[
            &票据(BASE),
            "8eed26260dbdb1ea545119cc0368fa06.cnmt.nca",
            &format!("{BASE_NCA}.nca"),
            "aaaa0000000000000000000000000001.nca",
            "aaaa0000000000000000000000000002.nca",
            "aaaa0000000000000000000000000003.nca",
            "aaaa0000000000000000000000000004.nca",
        ]),
    );
    // ── 不是 Switch 容器的东西躺在 switch 目录下（ADR-0011：目录只是强先验）。
    写(&root.join("switch/其实不是.nsp"), &[0_u8; 0x2000]);

    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::named(root, "库");
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");

    现场 {
        dir,
        catalog,
        repo: DatRepo::in_memory().expect("开得出来"),
    }
}

/// 一份摆好的 TitleID 索引：反查表加两个区的 eShop 元数据。
///
/// **港服与美服共用同一个 TitleID**（真机实测 69.4% 如此）——这份索引照这个样子摆，
/// 于是地区该是「多区共用」而不是港版，中文只是**语言属性**（ADR-0019）。
fn 建_titledb() -> TitleDb {
    let mut store = TitleDb::in_memory().expect("开得出来");
    store
        .replace_ncas(&[
            (
                BASE_NCA.to_string(),
                Content {
                    title_id: BASE.to_string(),
                    version: 0,
                },
            ),
            (
                PATCH_NCA.to_string(),
                Content {
                    title_id: PATCH.to_string(),
                    version: 196_608,
                },
            ),
        ])
        .expect("写得进");
    let 一条 = |title_id: &str, name: &str, languages: &str| Title {
        title_id: title_id.to_string(),
        name: name.to_string(),
        publisher: Some("Falcom".to_string()),
        languages: Some(languages.to_string()),
        regions: Vec::new(),
    };
    store
        .replace_region("USA", &[一条(BASE, "Ys X - Nordics", "En,Ja,Zh")])
        .expect("写得进");
    store
        .replace_region("Hong Kong", &[一条(BASE, "伊蘇X －北境歷險－", "Zh,Ja")])
        .expect("写得进");
    store
}

fn 跑(现场: &mut 现场, titledb: Option<&TitleDb>) -> identify::Outcome {
    let options = Options::new(Roots::single("库", 现场.dir.path()));
    identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &identify::Ammo {
            repo: &现场.repo,
            verdicts: &verdict::Index::empty(),
            naming: &fuzzy::Naming::off(),
            guessing: &identify::model::Guessing::off(),
            titledb,
        },
        &options,
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("跑得动")
}

/// 找出一个变体的候选，按报告里那个顺序。
fn 候选(现场: &现场, 名字里带: &str) -> Vec<romcat_core::catalog::identify::Candidate> {
    let key = 现场
        .catalog
        .variants()
        .expect("读得出变体")
        .into_iter()
        .find(|it| it.key.contains(名字里带))
        .unwrap_or_else(|| panic!("找得到 {名字里带}"))
        .key;
    现场.catalog.candidates_of(&key).expect("读得出候选")
}

fn 结论(现场: &现场, 名字里带: &str) -> State {
    let key = 现场
        .catalog
        .variants()
        .expect("读得出变体")
        .into_iter()
        .find(|it| it.key.contains(名字里带))
        .unwrap_or_else(|| panic!("找得到 {名字里带}"))
        .key;
    现场
        .catalog
        .identification_of(&key)
        .expect("读得出结论")
        .expect("有这一条")
        .0
}

#[test]
fn 免密钥就认得出哪个游戏的哪个版本() {
    // ⭐ 票 27 的核心：容器层不加密，读它一个密钥都不要。
    let mut 现场 = 建现场();
    let titledb = 建_titledb();
    let outcome = 跑(&mut 现场, Some(&titledb));

    let 本体 = 候选(&现场, "伊蘇X 北境歷險");
    let best = 本体.first().expect("有候选");
    assert_eq!(best.confidence, Confidence::High);
    assert!(best.accepted, "ContentId 反查等同于一次精确哈希命中");
    assert_eq!(best.source, "titledb");
    assert!(
        best.evidence.contains("100.00% 唯一映射"),
        "依据要说清凭什么敢自动通过：{}",
        best.evidence
    );
    assert!(
        best.evidence.contains("免密钥"),
        "这条纪律要写在依据里：{}",
        best.evidence
    );
    // **地区是多区共用不是港版**：港服与美服共用同一个 TitleID，中文是语言属性
    // 而不是一条独立的发行版（ADR-0019）。
    assert_eq!(best.game, "Ys X - Nordics (World) (En,Ja,Zh)");
    assert_eq!(outcome.switch.resolved, 3, "本体、更新、整合版各反查出一条");
}

#[test]
fn 压缩过的照样认_一个字节都不解压() {
    let mut 现场 = 建现场();
    let titledb = 建_titledb();
    let outcome = 跑(&mut 现场, Some(&titledb));

    let 更新 = 候选(&现场, "伊蘇X 更新");
    let best = 更新.first().expect("有候选");
    assert!(best.accepted, "`.ncz` 的词干就是 ContentId，照样反查得到");
    assert!(
        best.game.contains("Update v196608"),
        "补丁要说得出是哪一版：{}",
        best.game
    );
    assert!(
        best.evidence.contains("补丁"),
        "本体 / 补丁 / 附属内容要分得开：{}",
        best.evidence
    );
    // ES-DE 的扩展名表里没有 .nsz / .xcz——这件事要传得到能力档案那一侧（ADR-0017）。
    assert!(
        best.evidence.contains("ES-DE"),
        "压缩过的在那个前端里默认看不见：{}",
        best.evidence
    );
    assert_eq!(outcome.switch.compressed, 1);
}

#[test]
fn 没取过_titledb_也认得出是哪个游戏_只是说不出版本() {
    // 容器的明文文件名表免密钥就说得出 TitleID；查表只是把结论从「哪个游戏」
    // 抬到「哪个游戏的哪个版本」。
    let mut 现场 = 建现场();
    let outcome = 跑(&mut 现场, None);

    let 本体 = 候选(&现场, "伊蘇X 北境歷險");
    let best = 本体.first().expect("有候选");
    assert_eq!(
        best.confidence,
        Confidence::Medium,
        "说不出版本就到不了高置信"
    );
    assert!(!best.accepted);
    assert_eq!(best.source, "Switch 容器");
    assert_eq!(best.serial.as_deref(), Some(BASE));
    assert!(
        best.evidence.contains("还没取过 titledb"),
        "「没查过」与「查了没有」是两件事：{}",
        best.evidence
    );
    assert_eq!(outcome.switch.resolved, 0);
    assert!(outcome.switch.ticketed >= 4, "四份带票据的都读出了 TitleID");
}

#[test]
fn 文件名丢了_title_id_的照样认得出() {
    // ⚠ 真库里 33.6% 的文件名含汉字，多半已经把 `[TitleID]` 丢了——
    // 所以容器那几层必须做，不能只靠文件名。
    let mut 现场 = 建现场();
    let titledb = 建_titledb();
    跑(&mut 现场, Some(&titledb));

    let dlc = 候选(&现场, "追加曲包");
    let best = dlc.first().expect("有候选");
    assert_eq!(best.serial.as_deref(), Some(DLC));
    assert!(
        best.evidence.contains("附属内容"),
        "DLC 要认得出来：{}",
        best.evidence
    );
    // 附属内容挂在**本体那部作品**下，不该长成一部叫 `0100A0C01BED9001` 的独立作品。
    assert!(best.game.starts_with("Ys X - Nordics"), "{}", best.game);
}

#[test]
fn 卡带走_seek_而不是前缀_并且反查不到是常态() {
    // ⚠ `secure` 分区的 HFS0 头真机实测落在文件的 368 MB 处：前缀读一辈子也够不着。
    let mut 现场 = 建现场();
    let titledb = 建_titledb();
    跑(&mut 现场, Some(&titledb));

    let 卡带 = 候选(&现场, "卡带");
    let best = 卡带.first().expect("有候选");
    assert_eq!(best.serial.as_deref(), Some(BASE), "票据在 secure 分区里");
    assert_eq!(best.confidence, Confidence::Medium);
    assert!(
        best.evidence.contains("isGameCard"),
        "XCI 反查不到是构造上的必然，要说清楚：{}",
        best.evidence
    );
    assert!(best.evidence.contains("scene XCI"), "{}", best.evidence);
}

#[test]
fn 文件名与容器对不上就点名_而同一个本体号段不算对不上() {
    let mut 现场 = 建现场();
    let titledb = 建_titledb();
    let outcome = 跑(&mut 现场, Some(&titledb));

    let 改过 = 候选(&现场, "改过名的");
    let best = 改过.first().expect("有候选");
    assert!(
        best.evidence.contains("连本体那一串都对不上"),
        "极强的「被改过」信号要逐条点名：{}",
        best.evidence
    );
    // ⚠ 真机上五条「对不上」全是同一种形状：文件名写的是**本体**的 TitleID，
    // 而卡带里那张票据是它的**补丁**——那是一张「本体加更新」的卡该有的样子。
    // 按原样比会把五条正常的卡全报成警告，而真出事时那句警告就没人信了。
    let 更新 = 候选(&现场, "伊蘇X 更新");
    let 那条 = 更新.first().expect("有候选");
    assert!(
        !那条.evidence.contains("⚠"),
        "文件名写本体、容器里是它的补丁，不是冲突：{}",
        那条.evidence
    );
    assert_eq!(outcome.switch.name_conflicts, 1, "只有真对不上的那一条算");
}

#[test]
fn 不是_switch_容器的如实说而不是硬认() {
    // 目录只是强先验（ADR-0011）：`switch/` 下躺着的不一定是 Switch 的东西。
    let mut 现场 = 建现场();
    let titledb = 建_titledb();
    跑(&mut 现场, Some(&titledb));

    assert_eq!(结论(&现场, "其实不是"), State::NoEvidence);
    assert!(候选(&现场, "其实不是").is_empty());
}

#[test]
fn 第二趟一个字节都不读() {
    // **算过的不再算**：探出来的事实落在 `content_switch` 里，按文件的三元组作废
    // （与光盘、卡带两层同一条路，挂账 D14）。
    let mut 现场 = 建现场();
    let titledb = 建_titledb();
    let first = 跑(&mut 现场, Some(&titledb));
    assert!(first.read_bytes > 0, "第一趟总要读一遍");
    let second = 跑(&mut 现场, Some(&titledb));
    assert_eq!(second.read_bytes, 0, "第二趟该是零");
    assert_eq!(second.switch.resolved, first.switch.resolved);
}

#[test]
fn 报告分得出本体补丁与附属内容() {
    // 不分开数的话，一堆更新包会被报成游戏（调研的实现陷阱第 7 条）。
    let mut 现场 = 建现场();
    let titledb = 建_titledb();
    let outcome = 跑(&mut 现场, Some(&titledb));
    let kinds = outcome.report.switch_kinds.clone();
    let 取 = |code: &str| {
        kinds
            .iter()
            .find(|(kind, _)| kind == code)
            .map_or(0, |(_, count)| *count)
    };
    assert_eq!(取("base"), 4, "本体那份 NSP、卡带、改过名的、整合版");
    assert_eq!(取("patch"), 1);
    assert_eq!(取("addon"), 1);
    let text = outcome.report.render_text();
    assert!(text.contains("Switch 的内容分布"), "{text}");
}

#[test]
fn 压缩过的那些走到能力档案时_es_de_说它吃不下() {
    // ⭐ 票 7 那条验收：`.nsz` / `.xcz` 的可见性问题**要传得到能力档案那里**
    // （ADR-0017，票 21 已经把 ES-DE 那一档写好了）。此前传不到的原因不在能力档案，
    // 而在**那些文件压根不是变体**——扫描器归不了类，成型也就不聚它们。
    // 这条测试走的正是那条链路：扫描 → 成型 → 变体 → 能力档案。
    let mut 现场 = 建现场();
    跑(&mut 现场, None);
    let profile = Roster::builtin()
        .find("es-de-exfat")
        .expect("内置档案里有 ES-DE")
        .clone();
    let 压缩过的 = 现场
        .catalog
        .variants()
        .expect("读得出变体")
        .into_iter()
        .find(|it| it.key.ends_with(".nsz"))
        .expect("`.nsz` 现在是一个变体");
    let romcat_core::capability::Decision::Unsupported { want, .. } = capability::decide(
        &profile,
        压缩过的.platform.as_deref(),
        &压缩过的.key,
        压缩过的.bytes,
        None,
    ) else {
        panic!("ES-DE 的 switch 扩展名表里没有 nsz");
    };
    assert!(want.contains("nsp"), "目标要的是 nsp / xci：{want}");
    // 而没压缩过的那一份照样吃得下——对照组，不然上面那条测不出是扩展名的事。
    let 本体 = 现场
        .catalog
        .variants()
        .expect("读得出变体")
        .into_iter()
        .find(|it| it.key.contains("北境歷險"))
        .expect("找得到本体");
    assert_eq!(
        capability::decide(
            &profile,
            本体.platform.as_deref(),
            &本体.key,
            本体.bytes,
            None
        ),
        romcat_core::capability::Decision::AsIs,
    );
}

#[test]
fn 容器里另有一批查不到的_content_id_就不自动通过() {
    // ⚠ 真机 67 条反查的分布：**60 条恰好漏一个**（Meta NCA 按构造不在自己的清单里），
    // 而漏 5 / 6 / 13 / 24 个的那 7 条全是「一个容器里装着不止一档内容」——
    // `(1G+1U+9D)(MOD14)` 的整合卡、双游戏合集、本体加更新打包的三部曲。
    // 只看「指向唯一一个 (TitleID, 版本)」就自动通过，等于把那 7 份当成了好转储。
    let mut 现场 = 建现场();
    let titledb = 建_titledb();
    跑(&mut 现场, Some(&titledb));

    // 本体那份：4 条内容里 1 条查得到、Meta 那条查不到 → 漏 2 个，不自动通过。
    // （fixture 里本体有 cnmt + 一条 nca，只有 nca 在 titledb 里 → 漏 1 个，通过。）
    let 本体 = 候选(&现场, "北境歷險");
    assert!(本体[0].accepted, "只漏 Meta 那一个是常态");

    // 造一份「装着不止一档内容」的：同一个 TitleID 的一条 nca，外加四条查不到的。
    let key = 现场
        .catalog
        .variants()
        .expect("读得出变体")
        .into_iter()
        .find(|it| it.key.contains("整合"))
        .map(|it| it.key);
    assert!(key.is_some(), "现场里摆了一份整合包");
    let 整合 = 候选(&现场, "整合");
    assert!(!整合[0].accepted, "多出来一批查不到的，不敢自动通过");
    assert_eq!(整合[0].confidence, Confidence::Medium);
    assert!(
        整合[0].evidence.contains("查不到"),
        "要说清为什么不敢：{}",
        整合[0].evidence
    );
}
