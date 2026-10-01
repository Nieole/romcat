//! **平台纠正**（票 `gui-looks-like-the-design/28`）：磁盘上摆一份目录与内容对不上的主库，
//! 按组下一个决定，看三件事有没有跟着变——
//!
//! 1. **库体检那一格**：处理过的组不再计数，撤销之后重新计数。
//! 2. **下一趟识别**：按纠正后的平台重新匹配，而「目录声明的平台」那一列一个字不动
//!    （票 `one-criterion-per-thing/03` 那条对照物）。
//! 3. **ADR-0004**：从头到尾**盘上一个字节都没动**——整份快照逐字节相同。
//!
//! 外加一件同一条回退链上的事（挂单 `Q602`）：**人裁决过的变体照样按卡带头判平台**。裁决说的是
//! 它是哪个发行版，不是它属于哪个平台；放错目录的一张卡被认真裁过之后，平台不许反而退回目录声明的那个。

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use romcat_core::catalog::identify::CartFactRow;
use romcat_core::catalog::{Catalog, Roots};
use romcat_core::dat::Convention;
use romcat_core::dat::chinese::ChineseMark;
use romcat_core::dat::logiqx::{DatHeader, GameRecord, RomRecord};
use romcat_core::dat::repo::{DatMeta, DatRepo, Unit};
use romcat_core::fs::RealFs;
use romcat_core::identify::cart::{self as cart_head, Cart};
use romcat_core::identify::{self, DecidedPlatforms, Options, VERDICT_SOURCE, fuzzy};
use romcat_core::platform::Manifest;
use romcat_core::report::{CorrectionGroup, CorrectionGroups, HealthReport};
use romcat_core::scan::aggregate::{ConflictEvidence, Limits};
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::task::Handle;
use romcat_core::testing::cart as real;
use romcat_core::testing::container::{ZipEntrySpec, zip_container};
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::verdict::{self, PlatformDecision, Store};

/// 这份主库的**主库标识**：平台纠正按它记，路径锚的裁决也按它钉。
const LIBRARY: &str = "这一份主库";

/// 放错目录的那张卡带：头是真的 GBC 卡（《007 黑日危机》繁体修正版），躺在 `psp/` 底下。
const 放错目录的卡带: &str = "psp/放错的/007 黑日危机 繁体修正版.gbc";

/// 那张卡带整份多大。比判平台要读的那段头大得多——读了整份还是只读头，一眼分得出来。
const 卡带多大: usize = 1 << 16;

struct 现场 {
    dir: TempDir,
    catalog: Catalog,
    store: Store,
}

/// 盘上摆一份目录与内容对不上的主库，**两组各挑一种凭据**：
///
/// - `fc/` 底下两份 `.fds`——扩展名只可能属于 FDS，而**卡带那一层对它一个字都说不出**
///   （磁碟不是卡带）。这一组用来看「按内容改」：改之前识别只好听目录的。
/// - `gba/` 底下一份 `.nds`，**头部字节是真的**（`testing::cart`），所以内容那一层自己就说得出
///   NDS。这一组用来看「保持目录的说法」——它要**压得过内容那一层**，否则人定完等于没定。
/// - `gba/` 底下还有一份本分的 `.gba`：同一个目录里不冲突的那一份一点不受影响。
/// - `杂物/` 底下也有一份 `.nds`——**未纳入管理的目录不进识别管线**，不算一条冲突（ADR-0011 修订段）。
/// - `psp/` 底下一张 **GBC 卡**（真头，`testing::cart`）——ADR-0011 说的「放错」，真库里 `psp/`
///   目录下就混着整包的 GB ROM（`identify::platform_of` 的文档）。`.gbc` 在平台清单里没有扩展名
///   那一列，**扫完、识别之前**库体检的那几组不数它；识别读过它的卡带头之后，它是 PSP → GBC 那一组
///   （票 `core-answers-once/01`）。它主要是给**裁决过的变体照样按卡带头判平台**那几条用的
///   （挂单 `Q602`）。
fn 建现场() -> 现场 {
    摆现场(Vec::new())
}

/// GB 目录里那张**只能在 GBC 上跑**的卡（CGB 标志 `C0h`，真头原样）。
const 只能在GBC上跑的: &str = "gb/汉化/007 黑日危机 汉化版.gbc";

/// GB 目录里那张**双模卡**（CGB 标志 `80h`：支持 GBC，也兼容单色 GB）。
const 双模卡: &str = "gb/合集/007 黑日危机 双模.gbc";

/// GBC 目录里那张 **GB 游戏**（CGB 标志 `00h`）。
const 单色卡: &str = "gbc/合集/007 黑日危机 单色.gb";

/// 同一份主库，再摆上 **GB / GBC 那一族的三张卡**（票 `core-answers-once/01`）：三张只差卡带头
/// `0x143` 那一个 CGB 标志（`testing::cart::gbc_twine_with_cgb_flag`）。
///
/// - `gb/` 底下一张**只能在 GBC 上跑**的——放在 GB 目录会被当成 GB 游戏导出，算不符，成 GB → GBC 那一组；
/// - `gb/` 底下一张**双模卡**——GB 跑得了它，**不算**不符；
/// - `gbc/` 底下一张 **GB 游戏**——GBC 向下兼容它、改不改都行，可内容说的就是 GB，算不符，成 GBC → GB 那一组。
///
/// `.gb` / `.gbc` 在平台清单里没有扩展名那一列，这三条全凭**识别读过的卡带头**：扫完、识别之前一条都数不出来。
/// 三份补成同一个长度、与 `psp/` 那张不同长：**内容锚**按 CRC-32 加大小钉，不许钉到这几张上。
fn 建现场_有_gb_与_gbc() -> 现场 {
    摆现场(vec![
        (只能在GBC上跑的, real::padded(&real::GBC_TWINE, 1 << 15)),
        (
            双模卡,
            real::padded(&real::gbc_twine_with_cgb_flag(0x80), 1 << 15),
        ),
        (
            单色卡,
            real::padded(&real::gbc_twine_with_cgb_flag(0x00), 1 << 15),
        ),
    ])
}

/// 盘上摆好 [`建现场`] 那几份，外加 `另摆` 的这几份，扫一遍。
fn 摆现场(另摆: Vec<(&str, Vec<u8>)>) -> 现场 {
    let dir = temp_dir("platform-fix");
    let root = dir.path();
    let 本来的 = vec![
        ("fc/日版/塞尔达传说.fds", b"fds-a".to_vec()),
        ("fc/合集/银河战士.fds", b"fds-b".to_vec()),
        (
            "gba/汉化/逆转裁判4.nds",
            real::padded(&real::NDS_GYAKUTEN_KENJI, 1 << 16),
        ),
        (
            "gba/汉化/火焰之纹章.gba",
            real::padded(&real::GBA_ROCKMAN_EXE6, 1 << 16),
        ),
        (
            "杂物/放错的.nds",
            real::padded(&real::NDS_GYAKUTEN_KENJI, 1 << 15),
        ),
        (放错目录的卡带, real::padded(&real::GBC_TWINE, 卡带多大)),
    ];
    for (相对, 内容) in 本来的.into_iter().chain(另摆) {
        let 落点 = root.join(相对);
        fs::create_dir_all(落点.parent().expect("有上级目录")).expect("建得出目录");
        fs::write(&落点, &内容).expect("写得进");
    }

    let mut catalog = Catalog::open_in_memory().expect("开得出中立库");
    let mut options = ScanOptions::named(root, "库");
    options.jobs = Jobs::Fixed(1);
    scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");

    现场 {
        dir,
        catalog,
        store: Store::in_memory().expect("开得出沉淀库"),
    }
}

fn 体检(现场: &现场) -> HealthReport {
    let aggregate = 现场
        .catalog
        .aggregate(&Limits::default(), &Manifest::builtin())
        .expect("折得出统计");
    HealthReport::build(
        &aggregate,
        &现场.catalog.report_meta().expect("元信息读得出来"),
    )
}

fn 那一层(现场: &现场) -> CorrectionGroups {
    CorrectionGroups::build(
        &体检(现场),
        &Manifest::builtin(),
        &现场
            .store
            .platform_corrections(LIBRARY)
            .expect("读得出人定过的那些"),
    )
}

/// 跑一趟识别，带上人定过的那些平台纠正与裁决；`回盘读` 关掉时一个字节都不读主库。
fn 跑一趟识别(现场: &mut 现场, 回盘读: bool) -> identify::Outcome {
    let corrections = 现场
        .store
        .platform_corrections(LIBRARY)
        .expect("读得出人定过的那些");
    let 裁决 = verdict::Index::load(&现场.store, LIBRARY).expect("读得出沉淀库");
    let mut options = Options::new(Roots::single("库", 现场.dir.path()));
    options.decided_platforms = Some(DecidedPlatforms::new(&corrections));
    options.read_library = 回盘读;
    let repo = 建_dat();
    identify::run(
        &RealFs::new(),
        &mut 现场.catalog,
        &identify::Ammo {
            repo: &repo,
            verdicts: &裁决,
            naming: &fuzzy::Naming::off(),
            guessing: &identify::model::Guessing::off(),
            titledb: None,
        },
        &options,
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("识别跑得动")
}

/// DAT 库里两份，各一条，哈希都对不上盘上任何一份：
///
/// - **GBC**：那张卡的原版，序列号是卡带头里那个 `BO7E`。放它进来是为了让「裁决短路那一步
///   **不产出候选**」这句话验得出来：没人裁过的时候，卡带那一层拿头里的游戏码撞得出这一条；
///   裁过之后同样读了头，却一条都不许撞出来。
/// - **PSP**：随便一条。有了它，`psp/` 底下的裸文件才会回盘算哈希（DAT 库里一条都没有的平台
///   不读，读出来也无处可撞）——按**内容锚**钉的裁决要到那之后才问得着，走的是第二处短路。
fn 建_dat() -> DatRepo {
    let mut repo = DatRepo::in_memory().expect("开得出 DAT 库");
    for (dat, platform, game, serial) in [
        (
            "Nintendo - Game Boy Color",
            "GBC",
            "007 - The World Is Not Enough (USA, Europe)",
            Some("BO7E"),
        ),
        (
            "Sony - PlayStation Portable",
            "PSP",
            "Some PSP Game (Japan)",
            None,
        ),
    ] {
        let mut writer = repo
            .begin(&Unit {
                source: "No-Intro".to_string(),
                name: dat.to_string(),
                url: "https://example.invalid/dat".to_string(),
                fingerprint: "sha".to_string(),
            })
            .expect("开得了事务");
        writer
            .write_dat(
                &DatMeta {
                    name: dat.to_string(),
                    platform: platform.to_string(),
                    convention: Convention::AsIs,
                    header: DatHeader::default(),
                },
                &[GameRecord {
                    name: game.to_string(),
                    roms: vec![RomRecord {
                        name: format!("{game}.rom"),
                        size: Some(1),
                        crc32: Some(0xDEAD_BEEF),
                        serial: serial.map(ToString::to_string),
                        ..RomRecord::default()
                    }],
                    ..GameRecord::default()
                }],
            )
            .expect("写得进");
        writer.commit().expect("提交");
    }
    repo
}

/// 往沉淀库里钉一条裁决：那张放错目录的卡带是《007 黑日危机》的一次发行，汉化组与第几版
/// 都说了——**平台一个字没说**，那不归人裁（`identify::Projector::project`）。
fn 钉一条裁决(现场: &mut 现场, 锚: verdict::Anchor) {
    现场
        .store
        .put(&verdict::Verdict::now(
            锚,
            verdict::Decision::Release(verdict::Facts {
                work: "007 黑日危机".to_string(),
                chinese: Some(ChineseMark::FanTranslated),
                team: Some("某汉化组".to_string()),
                version: Some("繁体修正版".to_string()),
                ..verdict::Facts::default()
            }),
        ))
        .expect("写得进");
}

/// 钉在那张卡带的路径上：拿不到内容判据也查得着，第一处短路（读盘之前）就问着。
fn 路径锚(现场: &现场) -> verdict::Anchor {
    verdict::Anchor::Path {
        library: LIBRARY.to_string(),
        variant_key: 变体键(现场, 放错目录的卡带),
    }
}

/// 钉在那张卡带的内容上（CRC-32 加大小）：裸文件的 CRC-32 要回盘算过才有，第二处短路才问着。
fn 内容锚() -> verdict::Anchor {
    let mut crc = flate2::Crc::new();
    crc.update(&real::padded(&real::GBC_TWINE, 卡带多大));
    verdict::Anchor::Content {
        crc32: crc.sum(),
        size: 卡带多大 as u64,
        sha1: None,
    }
}

/// 中立库里那个变体的键：键里带着盘上那条相对路径。
fn 变体键(现场: &现场, 相对: &str) -> String {
    现场
        .catalog
        .variants()
        .expect("读得出变体")
        .into_iter()
        .find(|variant| variant.key.ends_with(相对))
        .unwrap_or_else(|| panic!("{相对} 该是一个变体"))
        .key
}

/// 这一趟识别**为这个变体**读了多少字节：它那条结论上记着的数（`identification.read_bytes`）。
fn 为它读了(现场: &现场, 相对: &str) -> u64 {
    let 键 = 变体键(现场, 相对);
    let mut 读了 = None;
    现场
        .catalog
        .for_each_identification(&mut |_, _, _, key, read_bytes| {
            if key == 键 {
                读了 = Some(read_bytes);
            }
        })
        .expect("读得出结论");
    读了.unwrap_or_else(|| panic!("{相对} 这一趟该有一条结论"))
}

/// 这个变体**按哪个平台算**：读的是识别落下来的那一列（`identification.platform`）。
fn 按哪个平台算(现场: &现场, 键里带着: &str) -> Option<String> {
    现场
        .catalog
        .identified_platforms()
        .expect("读得出")
        .into_iter()
        .find(|(key, _)| key.contains(键里带着))
        .map(|(_, platform)| platform)
}

/// 中立库里**目录声明的那个平台**：平台冲突那张报表拿它当对照物，纠正过也不许回写。
fn 目录声明的(现场: &现场, 键里带着: &str) -> Option<String> {
    现场
        .catalog
        .variants()
        .expect("读得出变体")
        .into_iter()
        .find(|variant| variant.key.contains(键里带着))
        .and_then(|variant| variant.platform)
}

#[test]
fn 平台冲突按组列出来_每组说得出从哪到哪几条凭什么两条样例() {
    let 现场 = 建现场();
    let 这一层 = 那一层(&现场);
    let 各组: Vec<(String, String, u64)> = 这一层
        .groups()
        .iter()
        .map(|one| {
            (
                one.group.declared.clone(),
                one.group.implied.clone(),
                one.group.count,
            )
        })
        .collect();
    assert_eq!(
        各组,
        vec![
            ("FC".to_string(), "FDS".to_string(), 2),
            ("GBA".to_string(), "NDS".to_string(), 1),
        ],
        "未纳入管理的目录那一份不算；条数多的那一组在前"
    );
    let 头一组 = &这一层.groups()[0];
    assert_eq!(头一组.headline(), "FC 目录里的 FDS 游戏");
    assert!(
        头一组.reason().contains(".fds"),
        "理由说得出凭什么这么判：{}",
        头一组.reason()
    );
    assert_eq!(头一组.group.examples.len(), 2, "两条样例列得出来");
    assert!(头一组.pending(), "还没处理过");
    assert_eq!(这一层.remaining(), 3);
}

#[test]
fn 按内容改之后下一趟识别按新平台算_目录声明的那一列一个字不动_那一格不再数它() {
    let mut 现场 = 建现场();
    跑一趟识别(&mut 现场, true);
    assert_eq!(
        按哪个平台算(&现场, "塞尔达传说.fds").as_deref(),
        Some("FC"),
        "没人定过的时候，内容那一层说不出话就退回目录说的那个"
    );

    现场
        .store
        .set_platform_correction(LIBRARY, "FC", "FDS", PlatformDecision::ByContent)
        .expect("记得下");
    现场.catalog.clear_identifications().expect("清得掉上一趟");
    跑一趟识别(&mut 现场, true);

    assert_eq!(
        按哪个平台算(&现场, "塞尔达传说.fds").as_deref(),
        Some("FDS"),
        "人说按内容改，下一趟识别就按 FDS 去撞"
    );
    assert_eq!(
        按哪个平台算(&现场, "火焰之纹章.gba").as_deref(),
        Some("GBA"),
        "不冲突的那一份一点不受影响"
    );
    assert_eq!(
        目录声明的(&现场, "塞尔达传说.fds").as_deref(),
        Some("FC"),
        "「目录声明的平台」那一列是平台冲突那张报表的对照物，纠正也不许回写它"
    );

    // 识别读过卡带头之后，`psp/` 底下那张 GBC 卡也是一组（PSP → GBC，票 `core-answers-once/01`）：
    // 还没处理的剩 GBA → NDS 与它两组，各一条。
    let 这一层 = 那一层(&现场);
    assert_eq!(这一层.remaining(), 2, "处理过的那一组不再算进概要");
    assert_eq!(这一层.handled_note().as_deref(), Some("已处理 1 组"));
    assert_eq!(这一层.groups()[0].settled().as_deref(), Some("已改为 FDS"));
    assert_eq!(
        体检(&现场).conflicts.total,
        4,
        "报告说的是盘上的事实：处理过也一条不少"
    );
}

#[test]
fn 保持目录的说法压得过内容那一层_而且不再问第二遍() {
    // 「保持」不是「什么都不做」：这一份 `.nds` 的**头部字节是真的**，内容那一层自己就说得出
    // NDS（ADR-0011）。人说保持目录的说法，它就得压过去——否则定完下一趟识别照旧按 NDS 算，
    // 而那正是他说不要的。
    let mut 现场 = 建现场();
    跑一趟识别(&mut 现场, true);
    assert_eq!(
        按哪个平台算(&现场, "逆转裁判4.nds").as_deref(),
        Some("NDS"),
        "没人定过的时候内容说了算"
    );

    现场
        .store
        .set_platform_correction(LIBRARY, "GBA", "NDS", PlatformDecision::KeepDeclared)
        .expect("记得下");
    现场.catalog.clear_identifications().expect("清得掉上一趟");
    跑一趟识别(&mut 现场, true);

    assert_eq!(
        按哪个平台算(&现场, "逆转裁判4.nds").as_deref(),
        Some("GBA"),
        "人说保持目录的说法，内容那一层也压不过它"
    );
    // 还没处理的是 FC → FDS 两条与 PSP → GBC 一条（识别读过 `psp/` 那张卡的卡带头，票 `core-answers-once/01`）。
    let 这一层 = 那一层(&现场);
    assert_eq!(
        这一层.remaining(),
        3,
        "「保持」也算处理过——否则每体检一趟就要再问一遍"
    );
    assert_eq!(这一层.groups()[1].settled().as_deref(), Some("已保持 GBA"));
}

#[test]
fn 撤销之后这一组回到还没处理_那一格重新数它() {
    let mut 现场 = 建现场();
    现场
        .store
        .set_platform_correction(LIBRARY, "FC", "FDS", PlatformDecision::ByContent)
        .expect("记得下");
    assert_eq!(那一层(&现场).remaining(), 1);

    assert!(
        现场
            .store
            .undo_platform_correction(LIBRARY, "FC", "FDS")
            .expect("撤得掉")
    );
    let 这一层 = 那一层(&现场);
    assert_eq!(这一层.remaining(), 3, "撤销之后那一格重新数它");
    assert!(这一层.groups()[0].pending());
    assert_eq!(这一层.handled_note(), None);
}

#[test]
fn 纠正一整轮下来盘上的文件一个字节都没动() {
    // **主库只读**（ADR-0004）：纠正改的是「这一组按哪个平台算」，不是盘上的文件。
    // 目标目录整份快照逐字节相同——照票 26「移除一个根不动盘上的任何一个字节」那条的形状写。
    let mut 现场 = 建现场();
    let 之前 = 快照(现场.dir.path());

    跑一趟识别(&mut 现场, true);
    现场
        .store
        .set_platform_correction(LIBRARY, "FC", "FDS", PlatformDecision::ByContent)
        .expect("记得下");
    现场
        .store
        .set_platform_correction(LIBRARY, "GBA", "NDS", PlatformDecision::KeepDeclared)
        .expect("记得下");
    现场.catalog.clear_identifications().expect("清得掉上一趟");
    跑一趟识别(&mut 现场, true);
    let _ = 那一层(&现场);
    现场
        .store
        .undo_platform_correction(LIBRARY, "FC", "FDS")
        .expect("撤得掉");

    assert_eq!(
        之前,
        快照(现场.dir.path()),
        "纠正不移动、不改名、不改写任何一个文件"
    );
}

#[test]
fn 裁决过的放错目录的卡带照样按卡带头判平台_目录声明的与裁决说的一个字不动() {
    // 对照：**没人裁过**的同一张卡，识别读了卡带头，判的是头里那一族；卡带那一层还拿头里的
    // 游戏码撞得出一条候选。
    let mut 没裁过的 = 建现场();
    let 对照 = 跑一趟识别(&mut 没裁过的, true);
    assert_eq!(
        按哪个平台算(&没裁过的, 放错目录的卡带).as_deref(),
        Some("GBC"),
        "没人裁过的时候，平台按卡带头算"
    );
    assert!(对照.cart.candidates >= 1, "没人裁过时卡带那一层撞得出一条");

    // 同一张卡，**第一趟识别之前**就钉上一条裁决（删库重扫之后就是这样：中立库是新的，
    // 裁决钉在路径上，卡带头一次都没读过）。
    let mut 现场 = 建现场();
    let 锚 = 路径锚(&现场);
    钉一条裁决(&mut 现场, 锚);
    let 这一趟 = 跑一趟识别(&mut 现场, true);
    assert_eq!(这一趟.from_verdicts, 1, "这一趟真是裁决短路的");

    assert_eq!(
        按哪个平台算(&现场, 放错目录的卡带).as_deref(),
        Some("GBC"),
        "人裁过它是哪个发行版，平台照样按卡带头算——与没裁过的那张判出同一个"
    );
    assert_eq!(
        目录声明的(&现场, 放错目录的卡带).as_deref(),
        Some("PSP"),
        "「目录声明的平台」那一列一个字不动"
    );
    let 冲突 = 体检(&现场).conflicts;
    let 它 = 冲突
        .examples
        .iter()
        .find(|it| it.key.ends_with(放错目录的卡带))
        .expect("库体检「目录与内容平台不符」那一格报得出它：目录说 PSP，卡带头说 GBC");
    assert_eq!((它.declared.as_str(), 它.implied.as_str()), ("PSP", "GBC"));

    // 裁决说的那几样照旧：这一步只补平台那一格。
    let 键 = 变体键(&现场, 放错目录的卡带);
    let 候选 = 现场.catalog.candidates_of(&键).expect("读得出候选");
    assert_eq!(
        候选.len(),
        1,
        "这一趟没为它产出候选，只有裁决落成的那一条：{候选:?}"
    );
    assert_eq!(候选[0].source, VERDICT_SOURCE);
    assert_eq!(候选[0].game, "007 黑日危机", "裁决说的作品照旧");
    assert!(候选[0].release_id.is_some(), "裁决说的发行版照旧");
    assert_eq!(候选[0].chinese, Some(ChineseMark::FanTranslated));
    assert!(
        候选[0].evidence.contains("某汉化组"),
        "裁决说的汉化组照旧：{}",
        候选[0].evidence
    );
    assert_eq!(
        现场
            .catalog
            .decided_edition(&键)
            .expect("读得出")
            .as_deref(),
        Some("繁体修正版"),
        "裁决说的第几版照旧"
    );
    assert_eq!(这一趟.cart.candidates, 0, "卡带那一层这一趟一条候选都没撞");

    // 读盘：只读判平台所需的那段头，不读整份。
    let 读了 = 为它读了(&现场, 放错目录的卡带);
    let 头多长 = cart_head::probe_len(Cart::GameBoy, 卡带多大 as u64) as u64;
    assert!(
        读了 > 0 && 读了 <= 头多长,
        "只读判平台所需的头（至多 {头多长} 字节），这一趟为它读了 {读了}"
    );
}

#[test]
fn 裁决过的卡带头读过一次就落库_下一趟一个字节都不读() {
    let mut 现场 = 建现场();
    let 锚 = 路径锚(&现场);
    钉一条裁决(&mut 现场, 锚);
    跑一趟识别(&mut 现场, true);
    assert!(
        为它读了(&现场, 放错目录的卡带) > 0,
        "头一趟中立库里还没有算过的卡带头，要读"
    );

    // 卡带头已经在中立库里了（上一趟裁决短路那一步读完落的库）。
    let 下一趟 = 跑一趟识别(&mut 现场, true);
    assert_eq!(下一趟.from_verdicts, 1, "这一趟也是裁决短路的");
    assert_eq!(为它读了(&现场, 放错目录的卡带), 0, "为它一个字节都不读");
    assert_eq!(下一趟.read_bytes, 0, "整趟一个字节都不读");
    assert_eq!(
        按哪个平台算(&现场, 放错目录的卡带).as_deref(),
        Some("GBC"),
        "平台照样按取回来的卡带头算"
    );
}

#[test]
fn 关掉回盘读又没算过卡带头时_裁决过的卡带平台照旧按目录声明_一个字节都不读() {
    let mut 现场 = 建现场();
    let 锚 = 路径锚(&现场);
    钉一条裁决(&mut 现场, 锚);
    let 这一趟 = 跑一趟识别(&mut 现场, false);
    assert_eq!(这一趟.from_verdicts, 1, "这一趟真是裁决短路的");
    assert_eq!(为它读了(&现场, 放错目录的卡带), 0);
    assert_eq!(这一趟.read_bytes, 0, "关掉回盘读就一个字节都不读主库");
    assert_eq!(
        按哪个平台算(&现场, 放错目录的卡带).as_deref(),
        Some("PSP"),
        "卡带头中立库里没算过、又不许读，平台只好照旧按目录声明"
    );
}

#[test]
fn 按内容锚裁决的裸文件回盘算完哈希才问着裁决_平台照样按卡带头算() {
    // 第二处短路：裸文件的 CRC-32 要回盘算过才有，裁决要到那之后才问得着（`identify_variant`
    // 的第一之二步）。这一处与读盘之前那一处走同一步补依据，平台照样按卡带头算。
    let mut 现场 = 建现场();
    钉一条裁决(&mut 现场, 内容锚());
    let 这一趟 = 跑一趟识别(&mut 现场, true);
    assert_eq!(这一趟.from_verdicts, 1, "这一趟真是裁决短路的");
    assert!(
        为它读了(&现场, 放错目录的卡带) >= 卡带多大 as u64,
        "先整份读过一遍算哈希，裁决才问得着"
    );
    assert_eq!(
        按哪个平台算(&现场, 放错目录的卡带).as_deref(),
        Some("GBC"),
        "回盘之后才问着的裁决，平台照样按卡带头算"
    );
    assert_eq!(
        目录声明的(&现场, 放错目录的卡带).as_deref(),
        Some("PSP"),
        "「目录声明的平台」那一列一个字不动"
    );
    let 候选 = 现场
        .catalog
        .candidates_of(&变体键(&现场, 放错目录的卡带))
        .expect("读得出候选");
    assert_eq!(
        候选.iter().map(|it| it.source.as_str()).collect::<Vec<_>>(),
        vec![VERDICT_SOURCE],
        "这一趟没为它产出候选，只有裁决落成的那一条"
    );
}

/// 平台纠正那一层里**这一对平台**那一组；没有这一组是 `None`。
fn 那一组(这一层: &CorrectionGroups, 从: &str, 到: &str) -> Option<CorrectionGroup> {
    这一层
        .groups()
        .iter()
        .find(|one| one.group.declared == 从 && one.group.implied == 到)
        .cloned()
}

#[test]
fn 只能在_gbc_上跑的卡躺在_gb_目录_识别读过卡带头之后库体检那一格数到它_平台纠正里有_gb_到_gbc_那一组()
 {
    // 票 `core-answers-once/01`：设计稿平台纠正那一层打头就是这一组。`.gbc` 在平台清单里没有扩展名
    // 那一列，判据全在卡带头 `0x143` 那个 CGB 标志上——它说只能在 GBC 上跑，躺在 GB 目录就会被当成
    // GB 游戏导出，是真错。
    let mut 现场 = 建现场_有_gb_与_gbc();
    跑一趟识别(&mut 现场, true);

    let 报告 = 体检(&现场);
    let 库体检那一组 = 报告
        .conflicts
        .groups
        .iter()
        .find(|group| group.declared == "GB" && group.implied == "GBC")
        .expect("库体检那一格数到它：GB → GBC 那一组");
    assert_eq!(库体检那一组.count, 1);
    assert_eq!(
        库体检那一组.examples,
        vec![format!("库/{只能在GBC上跑的}")],
        "样例是那张卡在中立库里的键"
    );

    let 这一层 = 那一层(&现场);
    let 那一组 = 那一组(&这一层, "GB", "GBC").expect("平台纠正里有 GB → GBC 那一组");
    assert_eq!(那一组.headline(), "GB 目录里的 GBC 游戏");
    assert!(
        那一组.reason().contains("CGB 标志") && 那一组.reason().contains("0xC0"),
        "理由说得出凭的是卡带头的 CGB 标志：{}",
        那一组.reason()
    );
    assert!(那一组.pending(), "还没处理过");
    assert_eq!(
        那一组.interchangeable_note(),
        None,
        "GB 跑不了只能在 GBC 上跑的卡：只能改，没有「保持也不影响游玩」那一句"
    );
}

#[test]
fn 双模卡躺在_gb_目录不算不符_gb_游戏躺在_gbc_目录算_扩展名不对与卡带头族不对的照旧算() {
    let mut 现场 = 建现场_有_gb_与_gbc();
    跑一趟识别(&mut 现场, true);

    let 这一层 = 那一层(&现场);
    let 各组: Vec<(&str, &str, u64)> = 这一层
        .groups()
        .iter()
        .map(|one| {
            (
                one.group.declared.as_str(),
                one.group.implied.as_str(),
                one.group.count,
            )
        })
        .collect();
    assert_eq!(
        各组,
        vec![
            ("FC", "FDS", 2),
            ("GB", "GBC", 1),
            ("GBA", "NDS", 1),
            ("GBC", "GB", 1),
            ("PSP", "GBC", 1),
        ],
        "扩展名那一组（FC → FDS）、卡带头族不对的两组（GBA → NDS、PSP → GBC）照旧在；\
         CGB 标志说出 GB ↔ GBC 两组；条数多的在前，一样多按平台名"
    );
    let 样例: Vec<&String> = 这一层
        .groups()
        .iter()
        .flat_map(|one| &one.group.examples)
        .collect();
    assert!(
        !样例.iter().any(|key| key.ends_with(双模卡)),
        "双模卡兼容单色，GB 跑得了它，躺在 GB 目录里不算不符：{样例:?}"
    );

    let gb_游戏 = 那一组(&这一层, "GBC", "GB").expect("GBC → GB 那一组");
    assert_eq!(gb_游戏.group.examples, vec![format!("库/{单色卡}")]);
    assert!(
        gb_游戏.reason().contains("CGB 标志"),
        "理由说得出凭的是 CGB 标志：{}",
        gb_游戏.reason()
    );
    assert_eq!(
        gb_游戏.interchangeable_note().as_deref(),
        Some("GBC 能运行 GB 的游戏，保持也不影响游玩"),
        "GBC 向下兼容 GB：改不改都行"
    );
    assert!(
        那一组(&这一层, "GBA", "NDS")
            .expect("GBA → NDS 那一组")
            .reason()
            .contains("文件头"),
        "那份 `.nds` 的头是真的：识别读过之后，凭的是卡带头"
    );
    assert!(
        那一组(&这一层, "FC", "FDS")
            .expect("FC → FDS 那一组")
            .reason()
            .contains(".fds"),
        "磁碟不是卡带，那一组照旧凭扩展名"
    );
}

#[test]
fn 库体检那一格_平台纠正那几组_识别报告里的平台不符三处数得一样() {
    // ADR-0024：一件事一个判据。从前库体检按扩展名数、识别报告按卡带头的族数，同一份库两个数，
    // 读报告的人分不出那是两种毛病还是同一种数了两遍（挂单 `Q1030`）。
    let mut 现场 = 建现场_有_gb_与_gbc();
    let 这一趟 = 跑一趟识别(&mut 现场, true);

    let 库体检那一格 = 体检(&现场).conflicts.total;
    let 平台纠正那几组: u64 = 那一层(&现场)
        .groups()
        .iter()
        .map(|one| one.group.count)
        .sum();
    let 识别报告那一栏 = 这一趟.report.platform_conflicts;
    assert_eq!(库体检那一格, 6, "FC → FDS 两条，另外四组各一条");
    assert_eq!(平台纠正那几组, 库体检那一格);
    assert_eq!(识别报告那一栏, 库体检那一格);
    assert!(
        这一趟.report.render_text().contains("目录与内容平台不符"),
        "识别报告那一栏与库体检那一格叫同一个名字"
    );
}

#[test]
fn gb_到_gbc_那一组说保持_gb_下一趟识别按_gb_算_压得过卡带头() {
    // 识别问「这个变体撞上了哪一组」走的是同一处判据：卡带头说出来的那一组，人定了「保持」，
    // 下一趟识别就得听人的——否则那一组定完等于没定（`identify::platform_of` 那条回退链）。
    let mut 现场 = 建现场_有_gb_与_gbc();
    跑一趟识别(&mut 现场, true);
    assert_eq!(
        按哪个平台算(&现场, 只能在GBC上跑的).as_deref(),
        Some("GBC"),
        "没人定过的时候卡带头说了算"
    );
    assert_eq!(
        按哪个平台算(&现场, 双模卡).as_deref(),
        Some("GBC"),
        "双模卡不算不符，识别判定的照旧是卡带头说的那个：No-Intro 把它记在 GBC 集里"
    );

    现场
        .store
        .set_platform_correction(LIBRARY, "GB", "GBC", PlatformDecision::KeepDeclared)
        .expect("记得下");
    现场.catalog.clear_identifications().expect("清得掉上一趟");
    跑一趟识别(&mut 现场, true);

    assert_eq!(
        按哪个平台算(&现场, 只能在GBC上跑的).as_deref(),
        Some("GB"),
        "人说保持 GB，卡带头也压不过它"
    );
    assert_eq!(
        按哪个平台算(&现场, 双模卡).as_deref(),
        Some("GBC"),
        "那一组管不到双模卡：它没撞上任何一组"
    );
    let 那一组 = 那一组(&那一层(&现场), "GB", "GBC").expect("报告里那一组一条不少");
    assert_eq!(那一组.settled().as_deref(), Some("已保持 GB"));
}

#[test]
fn 透明容器里那张只能在_gbc_上跑的卡_样例写成容器的键接内部路径() {
    // 真库里卡带世代的大头在透明容器里（台账 `docs/library-facts.md`「容器构成」）：卡带头落库时
    // 记的是「容器的键 + 内部路径」，库体检那一格得按同一对去找它，找错一边就一条都数不出来。
    let 容器 = "gb/合集/007 合集.zip";
    let mut 现场 = 摆现场(vec![(
        容器,
        zip_container(&[ZipEntrySpec::stored(
            "007 黑日危机 汉化版.gbc",
            real::padded(&real::GBC_TWINE, 1 << 14),
        )]),
    )]);
    跑一趟识别(&mut 现场, true);

    let 那一组 = 那一组(&那一层(&现场), "GB", "GBC").expect("容器里那张也成 GB → GBC 那一组");
    assert_eq!(
        那一组.group.examples,
        vec![format!("库/{容器} › 007 黑日危机 汉化版.gbc")]
    );
    assert!(
        那一组.reason().contains("CGB 标志") && !那一组.reason().contains("容器内部"),
        "凭据是识别读过的卡带头，不是「容器内部」那一档（那一档只凭扩展名）：{}",
        那一组.reason()
    );
    assert_eq!(
        那一组
            .group
            .by_evidence
            .iter()
            .map(|(evidence, _, count)| (*evidence, *count))
            .collect::<Vec<_>>(),
        vec![(ConflictEvidence::CartHeader, 1)]
    );
}

#[test]
fn 扩展名说不符而卡带头的族认目录的_读过卡带头之后照旧算不符() {
    // 用户故事 4：合并判据不许漏掉从前报得出的。`md/` 底下一份 `.32x`，头是真的 MD 卡带头——
    // MD 那一族（MD、32X、Mega-CD）认 MD 目录，可扩展名只可能属于 32X。扫完就报得出 MD → 32X，
    // 识别读过卡带头之后照旧报得出（判据是并集：扩展名、卡带头，有一样说不符就算）。
    let 那一份 = "md/合集/梦幻模拟战 32X 版.32x";
    let mut 现场 = 摆现场(vec![(
        那一份,
        real::padded(&real::MD_SHINING_FORCE_2, 1 << 16),
    )]);
    let 扫完 = 那一层(&现场);
    assert!(
        那一组(&扫完, "MD", "32X").is_some(),
        "扫完、识别之前：扩展名说它是 32X"
    );

    跑一趟识别(&mut 现场, true);
    let 识别之后 = 那一层(&现场);
    let 那一组 = 那一组(&识别之后, "MD", "32X").expect("读过卡带头之后 MD → 32X 那一组照旧在");
    assert_eq!(那一组.group.examples, vec![format!("库/{那一份}")]);
    assert!(
        那一组.reason().contains(".32x"),
        "凭的还是扩展名：{}",
        那一组.reason()
    );
}

/// 把中立库里那几份 GB / GBC 卡带头改回**加 CGB 标志那一格之前**的样子：JSON 里没有 `cgb`
/// ——这一版之前识别过的库，盘上就是这个样子。
fn 退回没有_cgb_标志的旧头(现场: &mut 现场) {
    let mut 旧的 = Vec::new();
    for 相对 in [只能在GBC上跑的, 双模卡, 单色卡] {
        let 键 = 变体键(现场, 相对);
        for (inner, text) in 现场.catalog.cart_facts(&键).expect("读得出卡带头") {
            let mut facts: cart_head::Facts = serde_json::from_str(&text).expect("读得回来");
            facts.cgb = None;
            旧的.push(CartFactRow {
                key: 键.clone(),
                inner,
                platform: facts.platform.clone(),
                family: Some(",GB,GBC,".to_string()),
                facts: serde_json::to_string(&facts).expect("写得成 JSON"),
            });
        }
    }
    assert_eq!(旧的.len(), 3, "三张卡的头都读过、都落了库");
    现场.catalog.put_cart_facts(&旧的).expect("写得回去");
}

#[test]
fn 加_cgb_标志之前读的旧头_库体检先保守地不报_gb_到_gbc_下一趟识别重读那段头之后报出来() {
    let mut 现场 = 建现场_有_gb_与_gbc();
    跑一趟识别(&mut 现场, true);
    退回没有_cgb_标志的旧头(&mut 现场);

    let 旧头时 = 那一层(&现场);
    assert!(
        那一组(&旧头时, "GB", "GBC").is_none(),
        "旧头分不出兼容单色还是只能在 GBC 上跑：宁可少报一条，不把双模卡报成不符"
    );
    assert!(
        那一组(&旧头时, "GBC", "GB").is_some(),
        "旧头上平台那一格是 GB 的，CGB 标志必然不是 80h / C0h：照 GB 游戏答"
    );

    现场.catalog.clear_identifications().expect("清得掉上一趟");
    跑一趟识别(&mut 现场, true);
    assert!(
        为它读了(&现场, 只能在GBC上跑的) > 0,
        "旧头答不全平台不符那一问：重读那段头"
    );
    let 重读之后 = 那一层(&现场);
    assert_eq!(
        那一组(&重读之后, "GB", "GBC").map(|one| one.group.count),
        Some(1),
        "重读之后 GB → GBC 那一组报出来"
    );

    跑一趟识别(&mut 现场, true);
    assert_eq!(
        为它读了(&现场, 只能在GBC上跑的),
        0,
        "重读过的落了库，再下一趟一个字节都不读"
    );
}

/// 一个目录整份的样子：每个文件的相对路径 → 它的字节。
fn 快照(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut 待走 = vec![dir.to_path_buf()];
    while let Some(这一层) = 待走.pop() {
        for 项 in fs::read_dir(&这一层).expect("读得动目录") {
            let 项 = 项.expect("读得动一项");
            let 路 = 项.path();
            if 路.is_dir() {
                待走.push(路);
            } else {
                let 相对 = 路
                    .strip_prefix(dir)
                    .expect("在这个目录下面")
                    .to_string_lossy()
                    .into_owned();
                out.insert(相对, fs::read(&路).expect("读得动"));
            }
        }
    }
    out
}
