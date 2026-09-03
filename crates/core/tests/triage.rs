//! **待确认队列**与**批量裁决**这条接缝：从「识别拿不定主意」到「裁决沉淀下来、
//! 下一趟直接命中」。
//!
//! 要证的是票 08 的两条要害：
//!
//! 1. **批量**——按目录、按候选作品、按命名规律，一条命令套用一批。逐条点的队列在
//!    几千条规模下等于没有（ADR-0002）。
//! 2. **沉淀**——裁决钉在**文件哈希**上，换一份中立库、换一个路径，同一份内容直接
//!    精确命中，不再进队列（ADR-0008）。
//!
//! fixture 的形状照真机来：队列里绝大多数条目**一条候选都没有**（真机上未命中 11,823、
//! 无判据 4,537，候选数都是 0），所以「手工指定」才是主路径，不是备用路径。

use std::fs;
use std::path::Path;

use romcat_core::catalog::{Catalog, State};
use romcat_core::dat::Convention;
use romcat_core::dat::chinese::ChineseMark;
use romcat_core::dat::logiqx::{DatHeader, GameRecord, RomRecord};
use romcat_core::dat::repo::{DatMeta, DatRepo, Unit};
use romcat_core::fs::RealFs;
use romcat_core::identify::fuzzy;
use romcat_core::identify::{self, Options};
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::testing::container::{ZipEntrySpec, crc32, zip_container};
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::triage::{self, Decide, DecisionSpec, Filter, Overrides};
use romcat_core::verdict::{self, Anchor, Decision, Store};

const 库名: &str = "小库";

fn 卡带(fill: u8, payload: usize) -> Vec<u8> {
    let mut data = vec![0u8; 16];
    data[..4].copy_from_slice(b"NES\x1A");
    data.extend(std::iter::repeat_n(fill, payload));
    data
}

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

/// 一份原版卡带：DAT 里有它，识别自动通过，**不进队列**。
fn 原版() -> Vec<u8> {
    卡带(0xA1, 40_960)
}

/// 三份汉化版：DAT 里一条都没有，全是**未命中**，候选数 0——真机上的主力形态。
fn 汉化版(fill: u8) -> Vec<u8> {
    卡带(fill, 40_960)
}

struct 现场 {
    dir: TempDir,
    catalog: Catalog,
    repo: DatRepo,
    store: Store,
}

fn 建现场() -> 现场 {
    let dir = temp_dir("triage");
    let root = dir.path();
    写(
        &root.join("FC/超级马里奥.zip"),
        &zip_container(&[ZipEntrySpec::stored("Super Mario (Japan).nes", 原版())]),
    );
    for (index, name) in [
        "FC/勇者斗恶龙 外星科技汉化.zip",
        "FC/最终幻想 外星科技汉化.zip",
        "FC/某游戏 别家汉化.zip",
    ]
    .into_iter()
    .enumerate()
    {
        写(
            &root.join(name),
            &zip_container(&[ZipEntrySpec::stored(
                "rom.nes",
                汉化版(0xB0 + u8::try_from(index).expect("装得下")),
            )]),
        );
    }
    // 另一个平台的一份，用来证明 `--under` 真的按目录切
    写(
        &root.join("GBA/某掌机游戏[某汉化组]汉化.zip"),
        &zip_container(&[ZipEntrySpec::stored("rom.gba", 汉化版(0xC0))]),
    );

    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::new(root);
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &CancelToken::new()).expect("扫得动");
    现场 {
        dir,
        catalog,
        repo: 建_dat(),
        store: Store::in_memory().expect("开得出沉淀库"),
    }
}

fn 建_dat() -> DatRepo {
    let mut repo = DatRepo::in_memory().expect("开得出来");
    let 原 = 原版();
    let mut writer = repo
        .begin(&Unit {
            source: "TOSEC".to_string(),
            name: "fc.dat".to_string(),
            url: "https://example.invalid/x".to_string(),
            fingerprint: "sha".to_string(),
        })
        .expect("开得了事务");
    writer
        .write_dat(
            &DatMeta {
                name: "Nintendo Famicom - Games".to_string(),
                platform: "FC".to_string(),
                convention: Convention::AsIs,
                header: DatHeader::default(),
            },
            &[GameRecord {
                name: "Super Mario Bros. (1985)(Nintendo)".to_string(),
                roms: vec![RomRecord {
                    name: "smb.nes".to_string(),
                    size: Some(40_976),
                    crc32: Some(crc32(&原)),
                    ..RomRecord::default()
                }],
                ..GameRecord::default()
            }],
        )
        .expect("写得进");
    writer.commit().expect("提交");
    // GBA 平台也得有弹药，不然识别一个字节都不读它（`has_ammo`），
    // 那样它会落进「无判据」而不是「未命中」。
    let mut writer = repo
        .begin(&Unit {
            source: "No-Intro".to_string(),
            name: "gba.dat".to_string(),
            url: "https://example.invalid/y".to_string(),
            fingerprint: "sha2".to_string(),
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
                name: "Nothing (USA)".to_string(),
                roms: vec![RomRecord {
                    name: "nothing.gba".to_string(),
                    size: Some(7),
                    crc32: Some(7),
                    ..RomRecord::default()
                }],
                ..GameRecord::default()
            }],
        )
        .expect("写得进");
    writer.commit().expect("提交");
    repo
}

fn 跑识别(现场: &mut 现场) -> identify::Outcome {
    let index = verdict::Index::load(&现场.store, 库名).expect("读得出沉淀库");
    identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &identify::Ammo {
            repo: &现场.repo,
            verdicts: &index,
            naming: &fuzzy::Naming::off(),
            guessing: &identify::model::Guessing::off(),
            titledb: None,
        },
        &Options::new(现场.dir.path()),
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别不该失败")
}

fn 队列(现场: &现场, filter: &Filter) -> Vec<triage::Item> {
    let index = verdict::Index::load(&现场.store, 库名).expect("读得出沉淀库");
    let mut items = triage::survey(&现场.catalog, &index, filter)
        .expect("折得出队列")
        .items;
    triage::fill_prints(&现场.catalog, &mut items).expect("算得出判据");
    items
}

fn 裁(现场: &mut 现场, filter: &Filter, decide: &Decide) -> triage::Applied {
    let items = 队列(现场, filter);
    let plan = triage::plan(&现场.store, &items, decide).expect("排得出计划");
    triage::apply(&mut 现场.catalog, &mut 现场.store, &items, &plan).expect("落得下")
}

fn 手工(work: &str) -> Decide {
    Decide {
        spec: DecisionSpec::Manual(work.to_string()),
        overrides: Overrides {
            chinese: Some(ChineseMark::FanTranslated),
            ..Overrides::default()
        },
        note: None,
        library: 库名.to_string(),
    }
}

#[test]
fn 队列列出待裁决的变体连它的全部候选与依据() {
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let items = 队列(&现场, &Filter::default());

    // 自动通过的那份原版**不在队列里**——它不占人的时间（ADR-0002）。
    assert!(
        !items
            .iter()
            .any(|item| item.variant.key.contains("超级马里奥")),
        "精确命中的不该进队列"
    );
    // 四份汉化版全在，而且**一条候选都没有**：真机上这正是主力形态。
    assert_eq!(items.len(), 4, "{:?}", keys(&items));
    assert!(items.iter().all(|item| item.state == State::Unmatched));
    assert!(items.iter().all(|item| item.candidates.is_empty()));
    // 每条都拿得到内容判据，于是裁决钉得住哈希而不是路径。
    assert!(items.iter().all(|item| item.print.is_some()));
}

/// 装一份 GoodNES：一条 DAT 没记大小的记录——撞得上，但只凭 CRC-32，不许自动通过。
fn 装_goodnes(现场: &mut 现场) {
    let mut writer = 现场
        .repo
        .begin(&Unit {
            source: "GoodNES".to_string(),
            name: "good.dat".to_string(),
            url: "https://example.invalid/z".to_string(),
            fingerprint: "sha3".to_string(),
        })
        .expect("开得了事务");
    writer
        .write_dat(
            &DatMeta {
                name: "GoodNES".to_string(),
                platform: "FC".to_string(),
                convention: Convention::AsIs,
                header: DatHeader::default(),
            },
            &[GameRecord {
                name: "Dragon Quest [T+Chi]".to_string(),
                roms: vec![RomRecord {
                    name: "dq.nes".to_string(),
                    size: None,
                    crc32: Some(crc32(&汉化版(0xB0))),
                    ..RomRecord::default()
                }],
                ..GameRecord::default()
            }],
        )
        .expect("写得进");
    writer.commit().expect("提交");
}

#[test]
fn 命中但一条都没自动通过的照样进队列() {
    // 「通过但标记，等人裁决」那一档（ADR-0002 的中置信）。
    let mut 现场 = 建现场();
    装_goodnes(&mut 现场);
    跑识别(&mut 现场);

    let items = 队列(&现场, &Filter::default());
    let 那条 = items
        .iter()
        .find(|item| item.variant.key.contains("勇者斗恶龙"))
        .expect("该在队列里");
    assert_eq!(那条.state, State::Matched, "撞上了，但没自动通过");
    assert_eq!(那条.candidates.len(), 1);
    // **依据**说得出它是怎么来的，事后复核得了（ADR-0002）。
    assert!(
        那条.candidates[0].evidence.contains("这份 DAT 没记大小"),
        "{}",
        那条.candidates[0].evidence
    );
    assert!(那条.candidate_works().contains(&"Dragon Quest".to_string()));
}

#[test]
fn 按目录一次裁一批() {
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let applied = 裁(
        &mut 现场,
        &Filter {
            under: vec!["FC".to_string()],
            ..Filter::default()
        },
        &手工("某部作品"),
    );
    assert_eq!(applied.verdicts, 3, "FC 目录下三条一次裁完");
    // `FC` 不该顺手把 GBA 那条也裁了。
    let 剩下 = 队列(&现场, &Filter::default());
    assert_eq!(
        keys(&剩下),
        vec!["GBA/某掌机游戏[某汉化组]汉化.zip".to_string()]
    );
}

#[test]
fn 按命名规律一次裁一批() {
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let mut decide = 手工("外星科技的某作");
    decide.overrides.team = Some("外星科技".to_string());
    decide.overrides.version = Some("v1.2".to_string());
    let applied = 裁(
        &mut 现场,
        &Filter {
            name_contains: vec!["外星科技".to_string()],
            ..Filter::default()
        },
        &decide,
    );
    assert_eq!(applied.verdicts, 2, "名字里带同一个汉化组记号的那两条");

    // **汉化组与版本**跟着沉进去了（ADR-0008 点名要的两样）。
    let 沉 = 现场.store.all().expect("读得出");
    assert_eq!(沉.len(), 2);
    for verdict in &沉 {
        let Decision::Release(facts) = &verdict.decision else {
            panic!("该是发行版裁决");
        };
        assert_eq!(facts.team.as_deref(), Some("外星科技"));
        assert_eq!(facts.version.as_deref(), Some("v1.2"));
    }
}

#[test]
fn 按候选作品一次裁一批() {
    let mut 现场 = 建现场();
    // 让两条汉化版都撞上同一条 GoodNES 记录（同一个 CRC 撞不出两条，
    // 所以两条各写一条记录、同一个条目名）。
    let mut writer = 现场
        .repo
        .begin(&Unit {
            source: "GoodNES".to_string(),
            name: "good.dat".to_string(),
            url: "https://example.invalid/z".to_string(),
            fingerprint: "sha3".to_string(),
        })
        .expect("开得了事务");
    writer
        .write_dat(
            &DatMeta {
                name: "GoodNES".to_string(),
                platform: "FC".to_string(),
                convention: Convention::AsIs,
                header: DatHeader::default(),
            },
            &[
                GameRecord {
                    name: "Dragon Quest (Japan) [T+Chi]".to_string(),
                    roms: vec![RomRecord {
                        name: "dq1.nes".to_string(),
                        crc32: Some(crc32(&汉化版(0xB0))),
                        ..RomRecord::default()
                    }],
                    ..GameRecord::default()
                },
                GameRecord {
                    name: "Dragon Quest (USA) [T+Chi]".to_string(),
                    roms: vec![RomRecord {
                        name: "dq2.nes".to_string(),
                        crc32: Some(crc32(&汉化版(0xB1))),
                        ..RomRecord::default()
                    }],
                    ..GameRecord::default()
                },
            ],
        )
        .expect("写得进");
    writer.commit().expect("提交");
    跑识别(&mut 现场);

    let filter = Filter {
        candidate_work: vec!["Dragon Quest".to_string()],
        ..Filter::default()
    };
    // **采用各自的第 1 条候选**：批量裁决不必对每条重复说一遍它是什么。
    let mut decide = 手工("不该用到");
    decide.spec = DecisionSpec::Pick(1);
    decide.overrides.team = Some("某汉化组".to_string());
    let applied = 裁(&mut 现场, &filter, &decide);
    assert_eq!(applied.verdicts, 2, "候选指着同一部作品的那两条");
    assert_eq!(applied.matched, 2, "当场转成命中");

    let 沉 = 现场.store.all().expect("读得出");
    for verdict in &沉 {
        let Decision::Release(facts) = &verdict.decision else {
            panic!("该是发行版裁决");
        };
        // 作品名从候选的条目名里剥出来：两条发行版归到**同一部作品**下。
        assert_eq!(facts.work, "Dragon Quest");
        assert_eq!(facts.team.as_deref(), Some("某汉化组"));
    }
    // 两条发行版、一个作品——收敛不会把它拆成两个前端条目。
    assert_eq!(
        现场.catalog.work_names().expect("读得出").len(),
        2,
        "原版那部 + 裁决那部"
    );
}

#[test]
fn 采用候选时人补的那几样盖过候选自己带的() {
    // 「就是这条候选，另外汉化组是某某」是最常见的一句话。收下再忽略是最坏的
    // 一种「实现了」——用户以为记下了，库里一个字都没有。
    let mut 现场 = 建现场();
    装_goodnes(&mut 现场);
    跑识别(&mut 现场);
    let mut decide = 手工("不该用到");
    decide.spec = DecisionSpec::Pick(1);
    decide.overrides = Overrides {
        team: Some("某汉化组".to_string()),
        version: Some("v2".to_string()),
        region: Some("China".to_string()),
        chinese: Some(ChineseMark::FanTranslated),
        ..Overrides::default()
    };
    裁(
        &mut 现场,
        &Filter {
            name_contains: vec!["勇者斗恶龙".to_string()],
            ..Filter::default()
        },
        &decide,
    );
    let 沉 = 现场.store.all().expect("读得出");
    let Decision::Release(facts) = &沉[0].decision else {
        panic!("该是发行版裁决");
    };
    // 作品名照旧从候选里读出来，人没说的那几样才用候选的。
    assert_eq!(facts.work, "Dragon Quest");
    assert_eq!(facts.team.as_deref(), Some("某汉化组"));
    assert_eq!(facts.version.as_deref(), Some("v2"));
    assert_eq!(facts.region.as_deref(), Some("China"));
}

#[test]
fn 候选不够多的那些被挡下并说清() {
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let items = 队列(&现场, &Filter::default());
    let mut decide = 手工("不该用到");
    decide.spec = DecisionSpec::Pick(2);
    let plan = triage::plan(&现场.store, &items, &decide).expect("排得出计划");
    assert!(plan.decided.is_empty());
    assert_eq!(plan.blocked.len(), 4);
    assert!(
        plan.blocked[0].why.contains("只有 0 条候选"),
        "{:?}",
        plan.blocked[0]
    );
}

#[test]
fn 判定都不对之后手工指定候选集不构成天花板() {
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    // 一条候选都没有，照样裁得下去——这正是队列里的常态。
    let applied = 裁(
        &mut 现场,
        &Filter {
            name_contains: vec!["别家汉化".to_string()],
            ..Filter::default()
        },
        &手工("某部没人收录的作品"),
    );
    assert_eq!(applied.verdicts, 1);
    assert_eq!(applied.matched, 1);
    let (state, _) = 现场
        .catalog
        .identification_of("FC/某游戏 别家汉化.zip")
        .expect("读得出")
        .expect("有结论");
    assert_eq!(state, State::Matched, "裁决当场兑现，不必等下一趟识别");
}

#[test]
fn 确认没有发行版是一条明说的记录() {
    // 原挂账 D48：这件事曾经靠「作品有、发行版空」推断，而那个形状同时也是
    // 「裁决定了作品、识别认出了发行版」的样子。
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let mut decide = 手工("某同人移植");
    decide.spec = DecisionSpec::NoRelease {
        work: Some("某同人移植".to_string()),
    };
    let applied = 裁(
        &mut 现场,
        &Filter {
            under: vec!["GBA".to_string()],
            ..Filter::default()
        },
        &decide,
    );
    assert_eq!(applied.skipped, 1);
    let (state, reason) = 现场
        .catalog
        .identification_of("GBA/某掌机游戏[某汉化组]汉化.zip")
        .expect("读得出")
        .expect("有结论");
    assert_eq!(state, State::Skipped);
    assert!(
        reason
            .as_deref()
            .unwrap_or("")
            .contains("裁决记着它没有发行版"),
        "{reason:?}"
    );
    // 重跑识别照样跳过——判据是沉淀库里那条明说的记录，不是链接的形状。
    跑识别(&mut 现场);
    let (state, _) = 现场
        .catalog
        .identification_of("GBA/某掌机游戏[某汉化组]汉化.zip")
        .expect("读得出")
        .expect("有结论");
    assert_eq!(state, State::Skipped);
}

#[test]
fn 认不出那一档记下来就不再问第二遍() {
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let mut decide = 手工("不该用到");
    decide.spec = DecisionSpec::Unknown;
    let applied = 裁(
        &mut 现场,
        &Filter {
            under: vec!["GBA".to_string()],
            ..Filter::default()
        },
        &decide,
    );
    assert_eq!(applied.verdicts, 1);
    // 结论一个字没改（它本来就是未命中），但**退出队列**。
    let (state, _) = 现场
        .catalog
        .identification_of("GBA/某掌机游戏[某汉化组]汉化.zip")
        .expect("读得出")
        .expect("有结论");
    assert_eq!(state, State::Unmatched);
    跑识别(&mut 现场);
    assert!(
        !keys(&队列(&现场, &Filter::default()))
            .contains(&"GBA/某掌机游戏[某汉化组]汉化.zip".to_string()),
        "看过了、认不出，与还没人看过是两件事"
    );
}

#[test]
fn 裁决钉在文件哈希上下一趟直接命中不再进队列() {
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    裁(
        &mut 现场,
        &Filter {
            name_contains: vec!["外星科技".to_string()],
            ..Filter::default()
        },
        &手工("外星科技的某作"),
    );
    // 沉淀库里那两条钉的是**内容**，不是路径。
    for verdict in 现场.store.all().expect("读得出") {
        assert!(
            matches!(verdict.anchor, Anchor::Content { .. }),
            "{:?}",
            verdict.anchor
        );
    }
    let outcome = 跑识别(&mut 现场);
    assert_eq!(outcome.from_verdicts, 2, "两个变体的结论直接来自沉淀库");
    let 剩下 = keys(&队列(&现场, &Filter::default()));
    assert!(!剩下.iter().any(|key| key.contains("外星科技")), "{剩下:?}");
    // 候选表里那两条的**数据源**是沉淀库，与 DAT 平级地看得见。
    let counts = 现场.catalog.candidate_counts().expect("数得出");
    assert!(
        counts
            .sources
            .iter()
            .any(|row| row.source == identify::VERDICT_SOURCE && row.candidates == 2),
        "{:?}",
        counts.sources
    );
}

#[test]
fn 换一份中立库换一个路径同一个文件照样直接命中() {
    // 「裁决一次永久受益：重装、换机、日后拷进来的同一文件直接精确命中」（ADR-0008）。
    let mut 甲 = 建现场();
    跑识别(&mut 甲);
    裁(
        &mut 甲,
        &Filter {
            name_contains: vec!["勇者斗恶龙".to_string()],
            ..Filter::default()
        },
        &手工("勇者斗恶龙"),
    );
    let 导出 = serde_json::to_string(&甲.store.export(false).expect("导得出")).expect("序列化");

    // 换一台机器：另一个目录、另一个文件名、另一份中立库。只有字节是同一串。
    let 乙目录 = temp_dir("triage-换机");
    写(
        &乙目录.path().join("FC/另一个目录/改了个名.zip"),
        &zip_container(&[ZipEntrySpec::stored("rom.nes", 汉化版(0xB0))]),
    );
    let mut 乙 = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::new(乙目录.path());
    options.jobs = Jobs::Fixed(1);
    scan::scan(&RealFs::new(), &mut 乙, &options, &CancelToken::new()).expect("扫得动");
    let mut 乙库 = Store::in_memory().expect("开得出沉淀库");
    let account = 乙库.import(&导出).expect("收得下");
    assert_eq!((account.read, account.added), (1, 1));

    let index = verdict::Index::load(&乙库, "另一份库").expect("读得出");
    let outcome = identify::run(
        &RealFs::new(),
        &mut 乙,
        &identify::Ammo {
            repo: &建_dat(),
            verdicts: &index,
            naming: &fuzzy::Naming::off(),
            guessing: &identify::model::Guessing::off(),
            titledb: None,
        },
        &Options::new(乙目录.path()),
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别不该失败");
    assert_eq!(outcome.from_verdicts, 1, "路径变了、名字变了，字节没变");
    let (state, _) = 乙
        .identification_of("FC/另一个目录/改了个名.zip")
        .expect("读得出")
        .expect("有结论");
    assert_eq!(state, State::Matched);
}

#[test]
fn 拿不到内容判据时退到路径锚并如实说出来() {
    // 真机上无判据那一档有 4,537 条（容器穿不透、压缩镜像、目录树转储）。
    // 它们照样裁得下去，但那条裁决**只在本机成立**——说清比含糊过去强。
    let dir = temp_dir("triage-无判据");
    写(&dir.path().join("PS1/某游戏.chd"), &vec![7u8; 4_096]);
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::new(dir.path());
    options.jobs = Jobs::Fixed(1);
    scan::scan(&RealFs::new(), &mut catalog, &options, &CancelToken::new()).expect("扫得动");
    let mut 现场 = 现场 {
        dir,
        catalog,
        repo: 建_dat(),
        store: Store::in_memory().expect("开得出沉淀库"),
    };
    跑识别(&mut 现场);

    let items = 队列(&现场, &Filter::default());
    let 那条 = items.first().expect("队列里该有一条");
    assert_eq!(那条.state, State::NoEvidence);
    assert!(那条.print.is_none(), "压缩镜像这一层拿不到判据");
    assert!(matches!(那条.anchor(库名), Anchor::Path { .. }));

    let applied = 裁(&mut 现场, &Filter::default(), &手工("某游戏"));
    assert_eq!((applied.content_anchored, applied.path_anchored), (0, 1));
    // 导出**默认不带**路径锚：它对别人毫无用处，还顺带交出自己的目录结构。
    assert!(
        现场
            .store
            .export(false)
            .expect("导得出")
            .verdicts
            .is_empty()
    );
    assert_eq!(现场.store.export(true).expect("导得出").verdicts.len(), 1);
}

#[test]
fn 忘掉裁决之后重跑识别就回到队列里() {
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let filter = Filter {
        name_contains: vec!["外星科技".to_string()],
        ..Filter::default()
    };
    裁(&mut 现场, &filter, &手工("外星科技的某作"));
    跑识别(&mut 现场);
    assert!(队列(&现场, &filter).is_empty());

    let plan = triage::plan_forget(&现场.catalog, &现场.store, &filter, 库名).expect("排得出");
    assert_eq!(plan.rows.len(), 2);
    assert_eq!(triage::forget(&mut 现场.store, &plan).expect("忘得掉"), 2);
    跑识别(&mut 现场);
    assert_eq!(队列(&现场, &filter).len(), 2);
}

#[test]
fn 报告说得出按各个轴一次能覆盖多少() {
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let items = 队列(&现场, &Filter::default());
    let counts = 现场.store.counts().expect("数得出");
    let report = triage::report::QueueReport::build("（内存）", "（内存）", 4, &items, counts, 10);
    assert_eq!(report.queue, 4);
    // **按目录**那张表就是「一条 `--under` 值多少」。
    let fc = report
        .by_directory
        .iter()
        .find(|row| row.label == "FC")
        .expect("该有 FC 这一组");
    assert_eq!(fc.count, 3);
    // **按命名规律**那一轴：名字里 `[…]`（…）括起来的记号连条数一起印出来，
    // 人照着抄一个就是一条覆盖一批的命令。
    let 记号 = report
        .by_name_mark
        .iter()
        .find(|row| row.label == "某汉化组")
        .expect("该有这个记号");
    assert_eq!(记号.count, 1);
    let text = report.render_text();
    assert!(text.contains("按目录——一条 `--under` 覆盖多少"), "{text}");
    assert!(
        text.contains("按命名规律——一条 `--name` 覆盖多少"),
        "{text}"
    );
    assert!(text.contains("待确认队列"), "{text}");
}

fn keys(items: &[triage::Item]) -> Vec<String> {
    items
        .iter()
        .map(|item| item.variant.key.clone())
        .collect::<Vec<_>>()
}
