//! **文件名那一层的接缝**：磁盘上摆一份主库、手边一份中文离线索引，跑一遍识别，
//! 看那些数据库认不出来的变体有没有拿到**带依据的候选**，以及它们进没进**待确认队列**。
//!
//! 密集逻辑挂在纯函数上（`filename` 的剥离、`zh` 的相似度与两道交叉校验、
//! `identify::fuzzy` 的候选成型各有自己的单元测试），这里要证的是另一件事：
//! 那几条纯函数真的接在了扫描、识别与队列之间——**而且只在前面几层落空时才接上**。
//!
//! fixture 的形状照真机来：文件名带汉化组署名、语言记号与容量记号
//! （`超级机器人大战R[星组](v1.2+)(简)(JP)(68.92Mb).zip` 是真库里的名字）。

use std::fs;
use std::path::Path;

use romcat_core::catalog::identify::State;
use romcat_core::catalog::{Catalog, Confidence, EntryRecord, Verdict};
use romcat_core::container::{ContainerKind, Contents, InnerEntry, Penetration};
use romcat_core::dat::Convention;
use romcat_core::dat::logiqx::{DatHeader, GameRecord, RomRecord};
use romcat_core::dat::repo::{DatMeta, DatRepo, Unit};
use romcat_core::filename::Rules;
use romcat_core::fs::{EntryKind, EntryMeta, RealFs};
use romcat_core::identify::fuzzy;
use romcat_core::identify::{self, Options};
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::testing::container::{ZipEntrySpec, crc32, zip_container};
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::triage::{self, Filter};
use romcat_core::verdict;
use romcat_core::zh;

/// 一份 GBA 卡带的字节。内容是什么无所谓——这一层看的是名字。
fn 卡带(fill: u8) -> Vec<u8> {
    vec![fill; 4_096]
}

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

struct 现场 {
    dir: TempDir,
    catalog: Catalog,
    repo: DatRepo,
}

fn 建现场() -> 现场 {
    let dir = temp_dir("fuzzy");
    let root = dir.path();

    // 一、DAT 认得出来的那一份：精确哈希命中，**文件名那一层不该碰它**。
    写(
        &root.join("gba/认得出来的[星组](简)(JP)(64Mb).zip"),
        &zip_container(&[ZipEntrySpec::stored("known.gba", 卡带(0xA1))]),
    );
    // 二、DAT 认不出来、名字里写着中文的：这一层要为它产出候选。
    写(
        &root.join("gba/超级机器人大战R[星组](v1.2+)(简)(JP)(68.92Mb).zip"),
        &zip_container(&[ZipEntrySpec::stored("srwr.gba", 卡带(0xB2))]),
    );
    // 三、名字撞不上任何条目的：一条候选都不该有。
    写(
        &root.join("gba/谁也没听说过的东西.zip"),
        &zip_container(&[ZipEntrySpec::stored("x.gba", 卡带(0xC3))]),
    );
    // 四、**独占目录**：中文名写在上一级目录上，变体自己是一串拉丁字母。
    写(
        &root.join("gba/合金弹头7[某汉化组]/MSLUG7_CN.zip"),
        &zip_container(&[ZipEntrySpec::stored("m7.gba", 卡带(0xD4))]),
    );

    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::new(root);
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &CancelToken::new()).expect("扫得动");

    现场 {
        dir,
        catalog,
        repo: 建_dat(),
    }
}

/// 一份迷你 DAT：只收得下那一份「认得出来的」。
fn 建_dat() -> DatRepo {
    let mut repo = DatRepo::in_memory().expect("能开 DAT 库");
    let bytes = 卡带(0xA1);
    let mut writer = repo
        .begin(&Unit {
            source: "No-Intro".to_string(),
            name: "Nintendo - Game Boy Advance".to_string(),
            url: "https://example.invalid/x".to_string(),
            fingerprint: "sha".to_string(),
        })
        .expect("开得了事务");
    writer
        .write_dat(
            &DatMeta {
                name: "Nintendo - Game Boy Advance".to_string(),
                platform: "GBA".to_string(),
                convention: Convention::AsIs,
                header: DatHeader::default(),
            },
            &[GameRecord {
                name: "Known Game (Japan)".to_string(),
                roms: vec![RomRecord {
                    name: "known.gba".to_string(),
                    size: Some(bytes.len() as u64),
                    crc32: Some(crc32(&bytes)),
                    ..RomRecord::default()
                }],
                ..GameRecord::default()
            }],
        )
        .expect("写得进");
    writer.commit().expect("提交");
    repo
}

/// 一份迷你中文索引。两条条目，一条 GBA 一条 SFC——**平台交叉校验**要有东西可撞。
fn 建索引() -> zh::Index {
    zh::Index::build(
        vec![
            zh::Entry {
                id: 9677,
                name: "スーパーロボット大戦R".to_string(),
                name_cn: "超级机器人大战R".to_string(),
                aliases: vec!["Super Robot Taisen R".to_string()],
                year: Some(2002),
                platforms: vec!["GBA".to_string()],
                platform_text: "GBA".to_string(),
            },
            zh::Entry {
                id: 4,
                name: "メタルスラッグ7".to_string(),
                name_cn: "合金弹头7".to_string(),
                aliases: Vec::new(),
                year: Some(2008),
                platforms: vec!["GBA".to_string()],
                platform_text: "GBA".to_string(),
            },
            // 同名不同平台：**平台对不上的一条都不该产出**。
            zh::Entry {
                id: 111,
                name: "Known Game".to_string(),
                name_cn: "认得出来的".to_string(),
                aliases: Vec::new(),
                year: Some(1995),
                platforms: vec!["SFC".to_string()],
                platform_text: "SFC".to_string(),
            },
        ],
        "dump-2026-09-01".to_string(),
    )
}

fn 跑一趟(现场: &mut 现场, index: Option<&zh::Index>) {
    let rules = Rules::builtin();
    let naming = fuzzy::Naming {
        rules: &rules,
        index,
        tuning: zh::Tuning::default(),
    };
    let options = Options::new(现场.dir.path());
    identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &identify::Ammo {
            repo: &现场.repo,
            verdicts: &verdict::Index::empty(),
            naming: &naming,
        },
        &options,
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别不该失败");
}

fn 候选(现场: &现场, key: &str) -> Vec<romcat_core::catalog::Candidate> {
    现场.catalog.candidates_of(key).expect("读得到候选")
}

const 认得出来的: &str = "gba/认得出来的[星组](简)(JP)(64Mb).zip";
const 机器人: &str = "gba/超级机器人大战R[星组](v1.2+)(简)(JP)(68.92Mb).zip";
const 没听说过: &str = "gba/谁也没听说过的东西.zip";
const 独占目录里的: &str = "gba/合金弹头7[某汉化组]/MSLUG7_CN.zip";

#[test]
fn 剥完文件名撞上中文离线源产出带依据的候选() {
    let mut 现场 = 建现场();
    跑一趟(&mut 现场, Some(&建索引()));

    let 候选 = 候选(&现场, 机器人);
    assert_eq!(候选.len(), 1, "{候选:?}");
    let one = &候选[0];
    assert_eq!(one.source, fuzzy::SOURCE);
    assert_eq!(one.game, "超级机器人大战R");
    // **永不自动通过**：这一层一个字节都没看（ADR-0002）。
    assert!(!one.accepted);
    // 名字一字不差加平台对得上 → 中置信。
    assert_eq!(one.confidence, Confidence::Medium);
    // **依据**要说得出：从哪个名字剥的、剥掉了什么、撞上了哪条条目、两道校验各是什么。
    assert!(
        one.evidence.contains("按剥离规则剥成「超级机器人大战R」"),
        "{}",
        one.evidence
    );
    assert!(one.evidence.contains("汉化组「星组」"), "{}", one.evidence);
    assert!(one.evidence.contains("条目 9677"), "{}", one.evidence);
    assert!(
        one.evidence.contains("平台交叉校验对得上"),
        "{}",
        one.evidence
    );
    assert!(one.evidence.contains("永不自动通过"), "{}", one.evidence);
}

#[test]
fn 前面几层认出来的那一份这一层碰都不碰() {
    // **前面办成了的事不重办**——与光盘、卡带两层的 `worth_probing` 同一条道理。
    // 这一份的中文名在索引里躺着（`认得出来的`），但它已经精确命中，所以一条都不该多。
    let mut 现场 = 建现场();
    跑一趟(&mut 现场, Some(&建索引()));
    let 候选 = 候选(&现场, 认得出来的);
    assert!(候选.iter().all(|it| it.source != fuzzy::SOURCE), "{候选:?}");
    assert!(候选.iter().any(|it| it.accepted));
}

#[test]
fn 名字撞不上的一条候选都没有() {
    let mut 现场 = 建现场();
    跑一趟(&mut 现场, Some(&建索引()));
    assert!(候选(&现场, 没听说过).is_empty());
}

#[test]
fn 独占目录的名字也拿去撞() {
    // 真库里那个游戏的中文名常常写在上一级目录上，变体自己是一串拉丁字母
    // （`我的暑假[ACG汉化组]/ACG_Summer_Holiday.7z`）。
    let mut 现场 = 建现场();
    跑一趟(&mut 现场, Some(&建索引()));
    let 候选 = 候选(&现场, 独占目录里的);
    assert_eq!(候选.len(), 1, "{候选:?}");
    assert_eq!(候选[0].game, "合金弹头7");
    assert!(
        候选[0].evidence.contains("独占目录的名字"),
        "{}",
        候选[0].evidence
    );
}

#[test]
fn 这一层的候选一律进待确认队列() {
    // 票据的验收原话。判据是队列自己的判据：没有一条自动通过的候选。
    let mut 现场 = 建现场();
    跑一趟(&mut 现场, Some(&建索引()));
    let survey = triage::survey(&现场.catalog, &verdict::Index::empty(), &Filter::default())
        .expect("队列折得出来");
    let keys: Vec<&str> = survey
        .items
        .iter()
        .map(|item| item.variant.key.as_str())
        .collect();
    assert!(keys.contains(&机器人), "{keys:?}");
    assert!(keys.contains(&独占目录里的), "{keys:?}");
    // 认出来的那一份不在队列里——它不占人的时间（ADR-0002 分三档的全部意义）。
    assert!(!keys.contains(&认得出来的), "{keys:?}");
    // 队列里那一条看得见候选与依据。
    let item = survey
        .items
        .iter()
        .find(|item| item.variant.key == 机器人)
        .expect("在队列里");
    assert_eq!(item.candidates.len(), 1);
    assert_eq!(item.state, State::Matched);
}

#[test]
fn 没取过中文数据源时识别照跑只是少一层() {
    // 取数是另一趟（`romcat zh sync`）。没取过不该让识别跑不起来。
    let mut 现场 = 建现场();
    跑一趟(&mut 现场, None);
    assert!(候选(&现场, 机器人).is_empty());
    // 前面几层照常工作。
    assert!(候选(&现场, 认得出来的).iter().any(|it| it.accepted));
}

#[test]
fn 名字还是乱码的容器重读一遍就解对了() {
    // 票 03 的有损转换不可逆，而编码探测只对**之后**扫的容器生效。已经落库的那批
    // （真机 21,901 条）走这条补救路径：只读那几个容器的中央目录，只改名字那两列。
    let dir = temp_dir("recheck");
    let root = dir.path();
    // `上海大亨.nes` 的 GBK 字节——真库 `FC/【HACK版游戏】/…` 里那条名字。
    let gbk: &[u8] = &[
        0xc9, 0xcf, 0xba, 0xa3, 0xb4, 0xf3, 0xba, 0xe0, 0x2e, 0x6e, 0x65, 0x73,
    ];
    let 容器 = "FC/上海大亨.zip";
    写(
        &root.join(容器),
        &zip_container(&[ZipEntrySpec::stored_raw(gbk, 卡带(0xE5))]),
    );
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::new(root);
    options.jobs = Jobs::Fixed(1);
    scan::scan(&RealFs::new(), &mut catalog, &options, &CancelToken::new()).expect("扫得动");

    // 新扫的本来就解对了——这正是这一改的正面效果。
    let 名字 = |catalog: &Catalog| {
        catalog
            .container_files(容器)
            .expect("读得到")
            .first()
            .map(|it| it.0.clone())
            .expect("有一条")
    };
    assert_eq!(名字(&catalog), "上海大亨.nes");

    // 把它按票 03 那种样子写回去：有损转换、`lossy` 为真。走的是扫描那条真写入路径。
    let 有损 = String::from_utf8_lossy(gbk).into_owned();
    assert!(有损.contains('\u{fffd}'));
    catalog
        .write(
            1,
            &[EntryRecord {
                key: 容器.to_string(),
                kind: EntryKind::File,
                meta: EntryMeta::Known {
                    len: fs::metadata(root.join(容器)).expect("在盘上").len(),
                    modified: None,
                },
                non_utf8: false,
                verdict: Verdict::Changed,
                sample: None,
                container: Some(Penetration {
                    kind: ContainerKind::Zip,
                    contents: Contents {
                        entries: vec![InnerEntry {
                            path: 有损.clone(),
                            size: 4_096,
                            crc32: Some(0),
                            is_dir: false,
                            block: Some(0),
                            name_lossy: true,
                        }],
                        blocks: 1,
                    },
                    failure: None,
                }),
            }],
        )
        .expect("写得进去");
    assert_eq!(名字(&catalog), 有损);

    let outcome = scan::names::recheck(
        &RealFs::new(),
        &mut catalog,
        root,
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("重读跑得动");
    assert_eq!(outcome.containers, 1);
    assert_eq!(outcome.reread, 1);
    assert_eq!(outcome.renamed, 1);
    assert_eq!(outcome.still_lossy, 0);
    assert_eq!(名字(&catalog), "上海大亨.nes");
    // 样例是这一趟唯一**可核对**的产出：猜错了一眼看得出来。
    assert_eq!(outcome.samples.len(), 1);
    assert_eq!(outcome.samples[0].1, "上海大亨.nes");
}

#[test]
fn 换一套匹配参数重跑一遍不必重新扫描() {
    // 票据的验收：**匹配参数可调整并重跑**。同一份中立库、同一份索引，只换门槛。
    let mut 现场 = 建现场();
    let index = 建索引();
    let rules = Rules::builtin();
    let options = Options::new(现场.dir.path());
    for (threshold, 该有几条) in [(0.85_f64, 1_usize), (1.01_f64, 0_usize)] {
        let naming = fuzzy::Naming {
            rules: &rules,
            index: Some(&index),
            tuning: zh::Tuning {
                threshold,
                ..zh::Tuning::default()
            },
        };
        identify::run(
            &RealFs::new(),
            &mut 现场.catalog,
            &identify::Ammo {
                repo: &现场.repo,
                verdicts: &verdict::Index::empty(),
                naming: &naming,
            },
            &options,
            &CancelToken::new(),
            &mut |_| {},
        )
        .expect("识别不该失败");
        assert_eq!(候选(&现场, 机器人).len(), 该有几条, "门槛 {threshold}");
    }
}
