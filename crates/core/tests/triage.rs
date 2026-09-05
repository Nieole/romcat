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

use romcat_core::catalog::{Catalog, State, Roots};
use romcat_core::task::Handle;
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
    let mut options = ScanOptions::named(root, "库");
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");
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
        &Options::new(Roots::single("库", 现场.dir.path())),
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
            under: vec!["库/FC".to_string()],
            ..Filter::default()
        },
        &手工("某部作品"),
    );
    assert_eq!(applied.verdicts, 3, "FC 目录下三条一次裁完");
    // `FC` 不该顺手把 GBA 那条也裁了。
    let 剩下 = 队列(&现场, &Filter::default());
    assert_eq!(
        keys(&剩下),
        vec!["库/GBA/某掌机游戏[某汉化组]汉化.zip".to_string()]
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
        .identification_of("库/FC/某游戏 别家汉化.zip")
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
            under: vec!["库/GBA".to_string()],
            ..Filter::default()
        },
        &decide,
    );
    assert_eq!(applied.skipped, 1);
    let (state, reason) = 现场
        .catalog
        .identification_of("库/GBA/某掌机游戏[某汉化组]汉化.zip")
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
        .identification_of("库/GBA/某掌机游戏[某汉化组]汉化.zip")
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
            under: vec!["库/GBA".to_string()],
            ..Filter::default()
        },
        &decide,
    );
    assert_eq!(applied.verdicts, 1);
    // 结论一个字没改（它本来就是未命中），但**退出队列**。
    let (state, _) = 现场
        .catalog
        .identification_of("库/GBA/某掌机游戏[某汉化组]汉化.zip")
        .expect("读得出")
        .expect("有结论");
    assert_eq!(state, State::Unmatched);
    跑识别(&mut 现场);
    assert!(
        !keys(&队列(&现场, &Filter::default()))
            .contains(&"库/GBA/某掌机游戏[某汉化组]汉化.zip".to_string()),
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
    let mut options = ScanOptions::named(乙目录.path(), "库");
    options.jobs = Jobs::Fixed(1);
    scan::scan(&RealFs::new(), &mut 乙, &options, &Handle::new()).expect("扫得动");
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
        &Options::new(Roots::single("库", 乙目录.path())),
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别不该失败");
    assert_eq!(outcome.from_verdicts, 1, "路径变了、名字变了，字节没变");
    let (state, _) = 乙
        .identification_of("库/FC/另一个目录/改了个名.zip")
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
    let mut options = ScanOptions::named(dir.path(), "库");
    options.jobs = Jobs::Fixed(1);
    scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");
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

/// 一个变体眼下在中立库里长什么样：结论、理由、候选、挂在哪个作品与发行版上。
/// **撤销要证的就是这一整份原样回来了**，只比状态是骗自己。
fn 中立库快照(现场: &现场, key: &str) -> (State, Option<String>, Vec<String>, Option<i64>, Option<i64>) {
    let (state, reason) = 现场
        .catalog
        .identification_of(key)
        .expect("读得出")
        .expect("有结论");
    let candidates = 现场
        .catalog
        .candidates_of(key)
        .expect("读得出候选")
        .into_iter()
        .map(|candidate| format!("{}|{}|{}", candidate.source, candidate.game, candidate.evidence))
        .collect();
    let variant = 现场.catalog.variant(key).expect("读得出").expect("变体在");
    (state, reason, candidates, variant.work_id, variant.release_id)
}

/// 撤销的**对拍**：一批落下**之前**与撤完**之后**必须一模一样。
///
/// 「撤销按批走、中立库与沉淀库两边一起回到那一批落下之前」这句承诺落到断言上就是它：
/// 中立库那一半按变体逐个比（结论、理由、候选、身上那两条链接）、连作品表一起比，
/// 沉淀库那一半整份比。只比状态是骗自己。
fn 对拍快照(现场: &现场, keys: &[String]) -> String {
    let mut out = String::new();
    for key in keys {
        out.push_str(&format!("{key} → {:?}\n", 中立库快照(现场, key)));
    }
    out.push_str(&format!(
        "作品 → {:?}\n沉淀库 → {:?}\n",
        现场.catalog.work_names().expect("读得出"),
        现场.store.all().expect("读得出"),
    ));
    out
}

/// 两份**重复拷贝**的现场：同一份内容躺在甲、乙两个文件里，钉的是同一条**内容锚**。
///
/// **真机上重复拷贝是常态**，撤销那一侧最难的几条都出在这个形状上，所以它收在一处。
fn 建重复拷贝现场(tag: &str, fill: u8) -> 现场 {
    let dir = temp_dir(tag);
    let 同一份 = 汉化版(fill);
    for name in ["FC/甲 某汉化.zip", "FC/乙 某汉化.zip"] {
        写(
            &dir.path().join(name),
            &zip_container(&[ZipEntrySpec::stored("rom.nes", 同一份.clone())]),
        );
    }
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::named(dir.path(), "库");
    options.jobs = Jobs::Fixed(1);
    scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");
    现场 {
        dir,
        catalog,
        repo: 建_dat(),
        store: Store::in_memory().expect("开得出沉淀库"),
    }
}

/// 点名 FC 目录下的这几个变体。
fn 点名(names: &[&str]) -> Filter {
    Filter {
        keys: names.iter().map(|name| format!("库/FC/{name}")).collect(),
        ..Filter::default()
    }
}

#[test]
fn 撤掉一批之后中立库与沉淀库两边都回到原样() {
    // 票 gui-redesign/08 的要害，也是原挂账 D102 被推翻的那一条：撤销**不必重跑识别**。
    // 一次「整批通过三千条」按错了却撤不干净，批量这件事本身就不成立。
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let filter = Filter {
        name_contains: vec!["外星科技".to_string()],
        ..Filter::default()
    };
    let 那两条 = keys(&队列(&现场, &filter));
    assert_eq!(那两条.len(), 2);
    let 原样: Vec<_> = 那两条.iter().map(|key| 中立库快照(&现场, key)).collect();
    let 原有作品 = 现场.catalog.work_names().expect("读得出").len();

    let applied = 裁(&mut 现场, &filter, &手工("外星科技的某作"));
    assert_eq!(applied.verdicts, 2);
    assert!(applied.batch > 0, "落下的那一趟该记成一批");
    assert!(队列(&现场, &filter).is_empty(), "裁完了不该还在队列里");

    let account =
        triage::undo_batch(&mut 现场.catalog, &mut 现场.store, applied.batch).expect("撤得掉");
    assert_eq!((account.removed, account.kept), (2, 0));
    assert!(account.catalog_rolled_back, "中立库那一半也该回去");
    assert_eq!(account.variants, 2);

    // **沉淀库那一半**：一条不剩。
    assert_eq!(现场.store.counts().expect("数得出").total, 0);
    // **中立库那一半**：结论、理由、候选、两条链接，一样不差地回来了——
    // 而且这中间**一趟识别都没跑**。
    for (key, before) in 那两条.iter().zip(&原样) {
        assert_eq!(&中立库快照(&现场, key), before, "{key} 没回到原样");
    }
    assert_eq!(
        现场.catalog.work_names().expect("读得出").len(),
        原有作品,
        "裁决建出来的那行作品该跟着收掉——重跑一趟识别时它本来就不在",
    );
    // 而它们**当场**回到了队列里。
    assert_eq!(队列(&现场, &filter).len(), 2);
}

#[test]
fn 撤销只动这一批别的裁决一条都不受影响() {
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let 甲 = Filter {
        name_contains: vec!["外星科技".to_string()],
        ..Filter::default()
    };
    let 乙 = Filter {
        name_contains: vec!["别家汉化".to_string()],
        ..Filter::default()
    };
    let 第一批 = 裁(&mut 现场, &甲, &手工("外星科技的某作"));
    let 第二批 = 裁(&mut 现场, &乙, &手工("别家的某作"));
    assert_ne!(第一批.batch, 第二批.batch, "两次裁决该是两批");
    assert_eq!(现场.store.counts().expect("数得出").total, 3);

    triage::undo_batch(&mut 现场.catalog, &mut 现场.store, 第一批.batch).expect("撤得掉");
    assert_eq!(
        现场.store.counts().expect("数得出").total,
        1,
        "撤第一批不该碰第二批那一条",
    );
    assert!(队列(&现场, &乙).is_empty(), "第二批裁过的不该回到队列里");
    assert_eq!(队列(&现场, &甲).len(), 2);
}

#[test]
fn 一批盖掉了先前的裁决时撤销把旧的那条放回去() {
    // 被盖掉的那条**不可再生**：沉淀库里那一行没了就是没了。所以批里存着它，
    // 撤销把它原样放回去——不然「撤销只动这一批」就是一句空话。
    //
    // 怎么会盖上：**重复拷贝是真机上的常态**（同一份内容躺着好几份）。裁过其中一份之后，
    // 另一份在下一趟识别之前照旧留在队列里，而它俩钉的是同一条**内容锚**。
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let filter = Filter {
        name_contains: vec!["别家汉化".to_string()],
        ..Filter::default()
    };
    let 那一条 = 队列(&现场, &filter).remove(0);
    let 锚 = 那一条.anchor(库名);
    let 旧的 = verdict::Verdict::now(
        锚.clone(),
        Decision::Release(verdict::Facts {
            work: "先定成这个".to_string(),
            ..verdict::Facts::default()
        }),
    );
    现场.store.put(&旧的).expect("写得进");

    let 这一批 = 裁(&mut 现场, &filter, &手工("改成那个"));
    assert_eq!(这一批.replaced, 1, "该盖掉先前那条");
    assert_eq!(现场.store.counts().expect("数得出").total, 1);

    let account =
        triage::undo_batch(&mut 现场.catalog, &mut 现场.store, 这一批.batch).expect("撤得掉");
    assert_eq!((account.removed, account.restored), (1, 1));
    let 现在 = 现场.store.all().expect("读得出");
    assert_eq!(现在.len(), 1);
    assert_eq!(现在[0], 旧的, "盖掉的那条旧裁决要原样回来");
}

#[test]
fn 一批里有几份同内容的拷贝时撤销把它们全都放回队列() {
    // **重复拷贝是真机上的常态**：同一份内容躺着好几份，它们钉的是同一条**内容锚**。
    // 一批里同时裁了两份时，撤销走到第二份，锚上那条已经被第一份删掉了——分不清
    // 「我们自己刚删的」与「别人重新裁过」的话，第二份会留着一条指向已经不存在的裁决的
    // 「命中」，而那正是这一票要消掉的那个形状。
    let mut 现场 = 建重复拷贝现场("triage-重复拷贝", 0xD0);
    跑识别(&mut 现场);
    let filter = Filter {
        name_contains: vec!["某汉化".to_string()],
        ..Filter::default()
    };
    assert_eq!(队列(&现场, &filter).len(), 2);

    let applied = 裁(&mut 现场, &filter, &手工("同一部作品"));
    assert_eq!(applied.verdicts, 2, "两个变体各记一条，锚是同一条");
    assert_eq!(现场.store.counts().expect("数得出").total, 1, "同一条锚只有一条裁决");

    let account =
        triage::undo_batch(&mut 现场.catalog, &mut 现场.store, applied.batch).expect("撤得掉");
    assert_eq!(account.kept, 0, "第二份不是「别人的账」，是我们自己刚删的那条");
    assert_eq!(account.variants, 2, "两个变体的结论都该放回去");
    assert_eq!(现场.store.counts().expect("数得出").total, 0);
    assert_eq!(队列(&现场, &filter).len(), 2, "两份拷贝都该回到队列里");

    // 放回去那一侧同样：两个变体都得重新兑现成命中。
    let 放回 =
        triage::redo_batch(&mut 现场.catalog, &mut 现场.store, applied.batch).expect("放得回去");
    assert_eq!(放回.matched, 2);
    assert!(队列(&现场, &filter).is_empty());
}

#[test]
fn 几份同内容的拷贝盖掉过旧裁决时撤销照样把它们全都放回队列() {
    // 上一条的**盖掉了旧裁决**那一版。撤第一份时锚上留下的是那条**旧的**，不是空的——
    // 判据要是写成「锚上是不是空的」，第二份就会被当成「别人的账」，它的中立库结论
    // 留着一条指向已经不存在的裁决的「命中」，再也回不到队列里。
    let mut 现场 = 建重复拷贝现场("triage-重复拷贝-盖掉", 0xD4);
    跑识别(&mut 现场);
    let filter = Filter {
        name_contains: vec!["某汉化".to_string()],
        ..Filter::default()
    };
    // 那条锚上先有一条裁决（换台机器导进来的、或者别处那份拷贝早先裁过的）。
    let 锚 = 队列(&现场, &filter)[0].anchor(库名);
    let 旧的 = verdict::Verdict::now(
        锚,
        Decision::Release(verdict::Facts {
            work: "先定成这个".to_string(),
            ..verdict::Facts::default()
        }),
    );
    现场.store.put(&旧的).expect("写得进");

    let applied = 裁(&mut 现场, &filter, &手工("改成那个"));
    assert_eq!((applied.verdicts, applied.replaced), (2, 2), "两条都盖在同一条锚上");

    let account =
        triage::undo_batch(&mut 现场.catalog, &mut 现场.store, applied.batch).expect("撤得掉");
    assert_eq!(account.kept, 0, "第二份不是「别人的账」");
    assert_eq!(account.variants, 2, "两个变体的结论都该放回去");
    let 现在 = 现场.store.all().expect("读得出");
    assert_eq!(现在.len(), 1);
    assert_eq!(现在[0], 旧的, "盖掉的那条旧裁决要原样回来");
    assert_eq!(队列(&现场, &filter).len(), 2, "两份拷贝都该回到队列里");
}

#[test]
fn 一批里两份重复拷贝的裁决跨了秒界时撤销照样把它们全都放回队列() {
    // **一批是一次落下，也就是一个时刻**——可原先是逐条取的。真机上一批三千条，
    // 两次取值落在不同秒的窗口不小；同一条**内容锚**上的两份**重复拷贝**因此在批里
    // 记下两条只差一秒的裁决，而沉淀库里那条锚上只有一条。撤销按整条相等去认
    // 「锚上眼下这条是不是这一批自己落的」，先走到的那一份就对不上，被当成
    // 「别人的账」留下，它那条指向已经不存在的裁决的「命中」永久留在中立库里。
    //
    // 票 08 那两条「重复拷贝」回归测试恰好都在同一秒内跑完，所以一直是绿的。
    let mut 现场 = 建重复拷贝现场("triage-重复拷贝-跨秒", 0xD8);
    跑识别(&mut 现场);
    let filter = 点名(&["甲 某汉化.zip", "乙 某汉化.zip"]);
    let 那两条 = keys(&队列(&现场, &filter));
    assert_eq!(那两条.len(), 2);
    let 原样 = 对拍快照(&现场, &那两条);

    let items = 队列(&现场, &filter);
    let mut plan = triage::plan(&现场.store, &items, &手工("同一部作品")).expect("排得出计划");
    assert_eq!(
        plan.decided[0].verdict.anchor, plan.decided[1].verdict.anchor,
        "两份重复拷贝钉的是同一条内容锚",
    );
    // **人为错开一秒**：逐条取时刻时跨过秒界就是这个样子。
    plan.decided[1].verdict.decided_at += 1;
    let applied = triage::apply(&mut 现场.catalog, &mut 现场.store, &items, &plan).expect("落得下");

    let account =
        triage::undo_batch(&mut 现场.catalog, &mut 现场.store, applied.batch).expect("撤得掉");
    assert_eq!(
        account.kept, 0,
        "第二份不是「别人的账」，是这一批自己在同一条锚上落下的那一条",
    );
    assert_eq!(account.variants, 2, "两个变体的结论都该放回去");
    assert_eq!(现场.store.counts().expect("数得出").total, 0);
    assert_eq!(对拍快照(&现场, &那两条), 原样, "撤完两边都该回到这一批落下之前");
    assert_eq!(队列(&现场, &filter).len(), 2, "两份拷贝都该回到队列里");

    // 放回去那一侧同样：两份拷贝一起回到命中，队列一条不剩。
    let 放回 =
        triage::redo_batch(&mut 现场.catalog, &mut 现场.store, applied.batch).expect("放得回去");
    assert_eq!(放回.matched, 2);
    assert!(队列(&现场, &filter).is_empty());
}

#[test]
fn 后一批盖住了前一批时先撤前一批被拒绝并说清是哪一批盖的() {
    // **批与批在同一条锚上是叠着的**：后一批的「它盖掉了什么」里存着前一批落下的那条，
    // 撤后一批就会把它放回来。所以前一批被后一批盖住时根本回不到「它落下之前」——
    // 硬撤的话前一批被标成已撤，而它的裁决过一会儿又活了，`undo` 说「已经撤过了」、
    // 要 `redo` 一趟空转再 `undo` 才回得来。
    let mut 现场 = 建重复拷贝现场("triage-两批交叠", 0xDA);
    跑识别(&mut 现场);
    let 两份 = 点名(&["甲 某汉化.zip", "乙 某汉化.zip"]);
    let 那两条 = keys(&队列(&现场, &两份));
    assert_eq!(那两条.len(), 2);
    let 原样 = 对拍快照(&现场, &那两条);

    let 批一 = 裁(&mut 现场, &点名(&["甲 某汉化.zip"]), &手工("作品一"));
    let 批二 = 裁(&mut 现场, &点名(&["乙 某汉化.zip"]), &手工("作品二"));
    assert_eq!(批二.replaced, 1, "两份重复拷贝钉的是同一条锚，后一批盖住了前一批");

    let 话 = triage::undo_batch(&mut 现场.catalog, &mut 现场.store, 批一.batch)
        .expect_err("被后来的批盖住了就不该撤得动")
        .to_string();
    assert!(
        话.contains(&format!("第 {} 批", 批二.batch)),
        "错误要说清是哪一批盖的，好让人先撤那一批：{话}",
    );
    // 拒绝了就一个字都不动：批一照旧在册，沉淀库照旧是批二那条。
    assert!(
        !现场.store.batch(批一.batch).expect("读得出").expect("在册").undone(),
        "撤不动的一批不该被标成已撤",
    );
    assert_eq!(现场.store.counts().expect("数得出").total, 1);

    // 按落下的倒序撤（先二后一），两边一起回到两批都没落之前。
    triage::undo_batch(&mut 现场.catalog, &mut 现场.store, 批二.batch).expect("撤得掉");
    triage::undo_batch(&mut 现场.catalog, &mut 现场.store, 批一.batch).expect("撤得掉");
    assert_eq!(对拍快照(&现场, &那两条), 原样, "两批都撤完就该回到原样");
    assert_eq!(队列(&现场, &两份).len(), 2);
}

#[test]
fn 锚上是别处写下的裁决时撤销不动它并如实报出没动几条() {
    // 「别人的账」不都是后来的**批**：`triage import` 收下的、别人分享来的那些不属于
    // 本机任何一批，谁也不会再把这一批的那条放回来。那时这一批的裁决**眼下确实不在生效**，
    // 撤销照旧撤得动，只是那一条一个字不动，并报出「没动几条」。
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let filter = Filter {
        name_contains: vec!["外星科技".to_string()],
        ..Filter::default()
    };
    let applied = 裁(&mut 现场, &filter, &手工("外星科技的某作"));
    assert_eq!(applied.verdicts, 2);

    let 那条锚 = 现场.store.all().expect("读得出")[0].anchor.clone();
    let 别处的 = verdict::Verdict::now(
        那条锚,
        Decision::Release(verdict::Facts {
            work: "别处定的".to_string(),
            ..verdict::Facts::default()
        }),
    );
    现场.store.put(&别处的).expect("写得进");

    let account =
        triage::undo_batch(&mut 现场.catalog, &mut 现场.store, applied.batch).expect("撤得掉");
    assert_eq!((account.removed, account.kept), (1, 1));
    assert!(
        现场.store.batch(applied.batch).expect("读得出").expect("在册").undone(),
        "这一批落下的裁决一条都不在生效了，就该记成已撤",
    );
    let 现在 = 现场.store.all().expect("读得出");
    assert_eq!(现在.len(), 1);
    assert_eq!(现在[0], 别处的, "别处写下的那条一个字都不该动");
}

#[test]
fn 撤销本身撤得回来() {
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let filter = Filter {
        name_contains: vec!["外星科技".to_string()],
        ..Filter::default()
    };
    let applied = 裁(&mut 现场, &filter, &手工("外星科技的某作"));
    triage::undo_batch(&mut 现场.catalog, &mut 现场.store, applied.batch).expect("撤得掉");
    assert_eq!(队列(&现场, &filter).len(), 2);

    // 放回去：一个字都不必用户重打，那一批当初落下的每一条原样记在批里。
    let 放回 =
        triage::redo_batch(&mut 现场.catalog, &mut 现场.store, applied.batch).expect("放得回去");
    assert_eq!(放回.verdicts, 2);
    assert_eq!(放回.matched, 2, "中立库那一半也该当场兑现");
    assert_eq!(现场.store.counts().expect("数得出").total, 2);
    assert!(队列(&现场, &filter).is_empty());

    // 撤过的批不许再撤一次，放回去的批不许再放一次——两句都得说得出口。
    assert!(
        triage::redo_batch(&mut 现场.catalog, &mut 现场.store, applied.batch).is_err(),
        "已经在册的一批不该再放一次",
    );
    // 再撤一次照样干净。
    triage::undo_batch(&mut 现场.catalog, &mut 现场.store, applied.batch).expect("撤得掉");
    assert_eq!(现场.store.counts().expect("数得出").total, 0);
    assert_eq!(队列(&现场, &filter).len(), 2);
    assert!(
        triage::undo_batch(&mut 现场.catalog, &mut 现场.store, applied.batch).is_err(),
        "撤过的一批不该再撤一次",
    );
}

#[test]
fn 跑过识别之后那一批只撤得回沉淀库那一半并如实说出来() {
    // 中立库那一半的快照**可再生**，所以它跟着中立库活：重跑一趟识别就清掉了
    // （`Catalog::clear_identifications`）。那时撤销只回滚得了沉淀库那一半——
    // **说出来**比让人以为队列已经回来了强（ADR-0021 那条纪律）。
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let filter = Filter {
        name_contains: vec!["外星科技".to_string()],
        ..Filter::default()
    };
    let applied = 裁(&mut 现场, &filter, &手工("外星科技的某作"));
    跑识别(&mut 现场);

    let account =
        triage::undo_batch(&mut 现场.catalog, &mut 现场.store, applied.batch).expect("撤得掉");
    assert_eq!(account.removed, 2, "沉淀库那一半照样撤得干净");
    assert!(!account.catalog_rolled_back, "快照没了就该如实说没回滚");
    // 再跑一趟识别，它们照样回到队列——那是这条路一直都在的出口。
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
        .find(|row| row.label == "库/FC")
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

#[test]
fn 分组表上那一行照着抄成选择器选中的就是那么多条() {
    // 报告与界面上那句话是「一条 `--under gba/【全部汉化】` 覆盖 **852** 条」。
    // 它只在选择器真的选出同样 852 条时才算数——分组按 `Item::directory` 数、
    // 选中按 `Filter::under` 筛，两边各写一遍的话迟早各说各的（`triage::Axis`）。
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let 整个队列 = 队列(&现场, &Filter::default());
    assert!(!整个队列.is_empty(), "队列该有东西");

    for axis in triage::Axis::ALL {
        for row in triage::tally(&整个队列, axis) {
            let 选中 = 队列(&现场, &axis.filter(&row.label));
            match axis {
                // **按命名规律**是子串匹配（`--name 汉化` 在真机上覆盖 1,986 条，
                // 而「汉化」压根不是一个括号记号），所以它选中的是这一组**或更多**。
                triage::Axis::NameMark => assert!(
                    选中.len() as u64 >= row.count,
                    "`{} {}` 的表上写着 {} 条，真选出来只有 {} 条",
                    axis.selector(),
                    row.label,
                    row.count,
                    选中.len(),
                ),
                _ => assert_eq!(
                    选中.len() as u64,
                    row.count,
                    "`{} {}` 的表上写着 {} 条，真选出来 {} 条",
                    axis.selector(),
                    row.label,
                    row.count,
                    选中.len(),
                ),
            }
        }
    }
}

#[test]
fn 队列列一次之后换选择器不再读库() {
    // 界面上人一分钟能换十几次选择器。每换一次重跑一遍 `survey`，真机上就是每次
    // 1.3 秒的卡顿——那样的队列没人用得下去（`triage::queue` 的模块文档）。
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let index = verdict::Index::load(&现场.store, 库名).expect("读得出沉淀库");
    let mut queue = triage::Queue::load(&现场.catalog, &index).expect("列得出队列");
    let 整个队列 = queue.selected().len();
    assert!(整个队列 > 0);

    // 换成「只要 FC 目录下的」——一次都不碰中立库。
    queue.set_filter(triage::Axis::Directory.filter("库/FC"));
    let fc = queue.selected().len();
    assert!(fc > 0 && fc < 整个队列, "FC 该是队列的一部分而不是全部");
    assert!(
        queue.selected().iter().all(|item| item.directory() == "库/FC"),
        "选中的里面混进了别的目录",
    );

    // 换回去，条数与刚列出来时一模一样：就地重筛不该丢东西。
    queue.set_filter(Filter::default());
    assert_eq!(queue.selected().len(), 整个队列);
}

#[test]
fn 裁完的当场从队列里消失() {
    // ADR-0002 说队列是主界面，而一个裁完了还留在原地的条目会被人再问一遍。
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let index = verdict::Index::load(&现场.store, 库名).expect("读得出沉淀库");
    let mut queue = triage::Queue::load(&现场.catalog, &index).expect("列得出队列");
    let 原有 = queue.pending();

    queue.set_filter(triage::Axis::Directory.filter("库/GBA"));
    let 这一批 = queue.selected().len();
    assert!(这一批 > 0);
    let 键: Vec<String> = queue
        .selected()
        .iter()
        .map(|item| item.variant.key.clone())
        .collect();

    let decide = triage::Draft {
        work: Some("某掌机游戏".to_string()),
        overrides: Overrides {
            team: Some("某汉化组".to_string()),
            ..Overrides::default()
        },
        ..triage::Draft::default()
    }
    .build(库名)
    .expect("说得成立");
    let plan = queue
        .plan(&现场.catalog, &现场.store, &decide)
        .expect("排得出计划");
    assert_eq!(plan.decided.len(), 这一批);
    let applied = queue
        .apply(&mut 现场.catalog, &mut 现场.store, &plan)
        .expect("落得下");
    assert_eq!(applied.verdicts, 这一批 as u64);

    assert_eq!(queue.pending(), 原有 - 这一批 as u64);
    queue.set_filter(Filter::default());
    assert!(
        queue
            .selected()
            .iter()
            .all(|item| !键.contains(&item.variant.key)),
        "刚裁完的还留在队列里",
    );
}

/// 往现场里再摆一份**名字撞得上中文离线源**的汉化版，再增量扫一遍。
fn 装_中文名撞得上的一份(现场: &mut 现场) {
    // **不摆 iNES 头**：卡带内部头说了算（`identify::platform_of`），摆一份 FC 的头
    // 会让这份 GBA 卡的平台被判成 FC，中文离线源那条平台交叉校验当场判冲突。
    写(
        &现场.dir.path().join("GBA/超级机器人大战R[星组](v1.2+)(简)(JP)(68.92Mb).zip"),
        &zip_container(&[ZipEntrySpec::stored("srwr.gba", vec![0xD0; 40_960])]),
    );
    let mut options = ScanOptions::named(现场.dir.path(), "库");
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut 现场.catalog, &options, &Handle::new()).expect("扫得动");
}

/// 一份迷你中文离线索引：**刮削那一侧的数据源**，撞出来的候选与 DAT 的候选同表。
fn 中文离线索引() -> romcat_core::zh::Index {
    romcat_core::zh::Index::build(
        vec![romcat_core::zh::Entry {
            id: 4_723,
            name: "スーパーロボット大戦R".to_string(),
            name_cn: "超级机器人大战R".to_string(),
            aliases: Vec::new(),
            year: Some(2002),
            platforms: vec!["GBA".to_string()],
            platform_text: "GBA".to_string(),
            ..romcat_core::zh::Entry::default()
        }],
        "dump-2026-09-01".to_string(),
    )
}

/// 跑一趟识别，**中文离线源那一层开着**。
fn 跑识别带中文源(现场: &mut 现场, index: &romcat_core::zh::Index) {
    let rules = romcat_core::filename::Rules::builtin();
    let verdicts = verdict::Index::load(&现场.store, 库名).expect("读得出沉淀库");
    identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &identify::Ammo {
            repo: &现场.repo,
            verdicts: &verdicts,
            naming: &fuzzy::Naming {
                rules: &rules,
                index: Some(index),
                tuning: romcat_core::zh::Tuning::default(),
            },
            guessing: &identify::model::Guessing::off(),
            titledb: None,
        },
        &Options::new(Roots::single("库", 现场.dir.path())),
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别不该失败");
}

fn 列队列(现场: &现场) -> triage::Queue {
    let index = verdict::Index::load(&现场.store, 库名).expect("读得出沉淀库");
    triage::Queue::load(&现场.catalog, &index).expect("列得出队列")
}

#[test]
fn 一级分批按依据形状分而且各批加起来就是整个队列() {
    // 票 `gui-redesign/09`：打开待确认屏看见的是**工具已经分好的几十批**，不是一万八千
    // 行的表。这一条钉的是那几十批算得对——各批条数加起来必须就是队列的条数，
    // 不然屏上「前 5 批盖住多少」是编的。
    let mut 现场 = 建现场();
    装_goodnes(&mut 现场);
    跑识别(&mut 现场);
    let queue = 列队列(&现场);

    let batches = queue.batches();
    assert!(batches.len() >= 2, "至少该分出「撞上了的」与「一条候选都没有的」两批");
    assert_eq!(
        batches.iter().map(|batch| batch.count).sum::<u64>(),
        queue.selected().len() as u64,
        "各批条数加起来不等于队列的条数，屏上那个百分比就是编的",
    );
    // 每一批照着它的形状折回一个选择器，**选中的与卡片上写的是同一批**。
    for batch in batches {
        let mut queue = 列队列(&现场);
        queue.set_filter(Filter {
            shape: vec![batch.shape.clone()],
            ..Filter::default()
        });
        assert_eq!(
            queue.selected().len() as u64,
            batch.count,
            "卡片上写着 {} 条，照着折出来的选择器却选中 {} 条：{}",
            batch.count,
            queue.selected().len(),
            batch.why(),
        );
    }

    // 撞上 GoodNES 那一批说得出**源 / DAT / 哈希口径**，还带着那句共同依据。
    let 撞上的 = batches
        .iter()
        .find(|batch| matches!(&batch.shape, triage::Shape::Candidates { source, .. } if source == "GoodNES"))
        .expect("该有一批是 GoodNES 撞出来的");
    assert_eq!(撞上的.count, 1);
    assert_eq!(撞上的.shape.fanout(), triage::Fanout::One);
    assert!(撞上的.passable(), "单候选那一批问的是「对不对」，按批答得了");
    assert!(撞上的.why().contains("GoodNES / GoodNES / 含头"), "{}", 撞上的.why());
    assert!(撞上的.why().contains("这份 DAT 没记大小"), "{}", 撞上的.why());

    // 一条候选都没有那一批说的是**为什么没定下来**，而且整批通过说不出口。
    let 光秃的 = batches
        .iter()
        .find(|batch| matches!(batch.shape, triage::Shape::Bare { .. }))
        .expect("该有一批一条候选都没有");
    assert!(
        !光秃的.passable(),
        "一条候选都没有却给「整批通过」，那是替人挑了个没人写过的答案",
    );
}

#[test]
fn 二级下钻的条数加起来等于它所属的一级() {
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let queue = 列队列(&现场);
    let batch = queue
        .batches()
        .first()
        .cloned()
        .expect("该分得出至少一批");
    let scope = triage::Scope::whole(batch.shape.clone());
    assert_eq!(queue.count(&scope), batch.count);

    // **按目录那个轴分得干净**：一条只落一个目录，所以各组之和就是这一批。
    let drilled = queue.drill(&scope, triage::Axis::Directory);
    assert!(drilled.adds_up(), "{drilled:?}");
    assert_eq!(
        drilled.rows.iter().map(|row| row.count).sum::<u64>(),
        batch.count,
        "二级各组加起来不等于它所属的一级",
    );

    // 下钻到某一组之后，那一组自己的条数与二级表上写的一模一样。
    let row = drilled.rows.first().expect("该有一组");
    let 那一组 = triage::Scope::under(batch.shape.clone(), triage::Axis::Directory, &row.label);
    assert_eq!(queue.count(&那一组), row.count, "下钻到 {}", row.label);
}

#[test]
fn 每批的随机样本换一组就真的换一组而且都在批内() {
    let mut 现场 = 建现场();
    跑识别(&mut 现场);
    let queue = 列队列(&现场);
    let batch = queue
        .batches()
        .iter()
        .max_by_key(|batch| batch.count)
        .cloned()
        .expect("该分得出至少一批");
    let scope = triage::Scope::whole(batch.shape.clone());
    let 批内: Vec<String> = queue
        .members(&scope)
        .iter()
        .map(|item| item.variant.key.clone())
        .collect();
    assert!(批内.len() > 2, "这一批太小，换样本这件事测不出来");

    let 头一组 = queue.sample(&scope, 0, 2);
    let 第二组 = queue.sample(&scope, 1, 2);
    assert_ne!(头一组, 第二组, "「换一组样本」按下去还是同一组");
    for one in 头一组.iter().chain(第二组.iter()) {
        assert!(批内.contains(&one.key), "样本跑到批外面去了：{}", one.key);
    }
}

#[test]
fn 整批通过之后按批整个撤回() {
    // 票 `gui-redesign/09` 验收第 5 条：整批操作之后可以**按批整个撤回**，
    // 走的是票 08 那条路——中立库与沉淀库两边一起回到这一批落下之前，
    // **一个字节的 DAT 都不读**。
    let mut 现场 = 建现场();
    装_goodnes(&mut 现场);
    跑识别(&mut 现场);
    let mut queue = 列队列(&现场);
    let 原有 = queue.pending();

    let batch = queue
        .batches()
        .iter()
        .find(|batch| batch.passable())
        .cloned()
        .expect("该有一批是单候选的");
    let scope = triage::Scope::whole(batch.shape.clone());
    let 这一批 = queue.count(&scope);
    assert!(这一批 > 0);

    // **整批通过 ＝ 采用第一条候选**。分批时说的那句共同依据说的正是它。
    let decide = triage::Draft {
        pick: Some(1),
        ..triage::Draft::default()
    }
    .build(库名)
    .expect("说得成立");
    let plan = queue
        .plan_scope(&现场.catalog, &现场.store, &decide, &scope)
        .expect("排得出计划");
    assert_eq!(plan.decided.len() as u64, 这一批, "{:?}", plan.blocked);
    let applied = queue
        .apply(&mut 现场.catalog, &mut 现场.store, &plan)
        .expect("落得下");
    assert_eq!(applied.verdicts, 这一批);
    assert_eq!(queue.pending(), 原有 - 这一批);

    // **按批整个撤回**：两边一起回去，当场列队列就看得见它们回来了。
    let undone = queue
        .undo(&mut 现场.catalog, &mut 现场.store, 库名, applied.batch)
        .expect("撤得掉");
    assert_eq!((undone.batch, undone.removed, undone.kept), (applied.batch, 这一批, 0));
    assert!(undone.catalog_rolled_back, "中立库那一半没回去");
    assert_eq!(queue.pending(), 原有, "撤回之后队列该回到整批通过之前那么多条");
    assert_eq!(现场.store.counts().expect("读得出").total, 0);
    // 那一批照旧数得出来——形状没变，卡片回到屏上。
    assert_eq!(queue.count(&scope), 这一批);
}

#[test]
fn 整批拒绝走的是认不出那一档() {
    // 「都不对」在这一屏上就是**整批拒绝**：它记的是「我看过了，认不出」，
    // 于是这些条退出队列、不再被问第二遍（`triage` 的模块文档）。
    let mut 现场 = 建现场();
    装_goodnes(&mut 现场);
    跑识别(&mut 现场);
    let mut queue = 列队列(&现场);
    let batch = queue
        .batches()
        .iter()
        .find(|batch| batch.passable())
        .cloned()
        .expect("该有一批是单候选的");
    let scope = triage::Scope::whole(batch.shape.clone());
    let 这一批 = queue.count(&scope);

    let decide = triage::Draft {
        unknown: true,
        ..triage::Draft::default()
    }
    .build(库名)
    .expect("说得成立");
    let plan = queue
        .plan_scope(&现场.catalog, &现场.store, &decide, &scope)
        .expect("排得出计划");
    assert_eq!(plan.decided.len() as u64, 这一批);
    let applied = queue
        .apply(&mut 现场.catalog, &mut 现场.store, &plan)
        .expect("落得下");
    assert_eq!(applied.verdicts, 这一批);
    assert_eq!(现场.store.counts().expect("读得出").unknown, 这一批);
    assert!(
        queue.count(&scope) == 0,
        "整批拒绝之后那一批还在队列里，人会被同一批问第二遍",
    );
}

#[test]
fn 识别与刮削的待确认在同一条队列里() {
    // 票 `gui-redesign/09` 验收第 9 条。**中文离线源是刮削那一侧的数据源**，
    // 而它撞出来的候选与 DAT 的候选同表、同一条队列——于是它自己占一批，
    // 卡片上写着的源就是「中文离线源」。
    let mut 现场 = 建现场();
    装_中文名撞得上的一份(&mut 现场);
    let index = 中文离线索引();
    跑识别带中文源(&mut 现场, &index);
    let queue = 列队列(&现场);

    let 中文源那批 = queue
        .batches()
        .iter()
        .find(|batch| {
            matches!(&batch.shape, triage::Shape::Candidates { source, .. } if source == "中文离线源")
        })
        .cloned()
        .expect("中文离线源撞出来的候选该在同一条队列里，自己占一批");
    assert!(中文源那批.count > 0);
    assert!(
        中文源那批.why().contains("中文离线源 / dump-2026-09-01"),
        "{}",
        中文源那批.why(),
    );
    // 它与 DAT 那几批**在同一份 batches 里**——不必去第二个地方。
    assert_eq!(
        queue.batches().iter().map(|batch| batch.count).sum::<u64>(),
        queue.selected().len() as u64,
    );
}

