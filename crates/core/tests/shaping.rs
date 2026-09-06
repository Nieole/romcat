//! 在真实磁盘上跑一遍**成型**：扫描 → 中立库 → 变体 → 报告。
//!
//! 单元测试挂在 `shape::plan` 那个纯函数上，覆盖每条规则的细节。这里要证的是另一件事：
//! 那条纯函数真的接在了扫描与报告之间——磁盘上摆着一份 PSV 转储，扫完之后中立库里
//! 是**一个变体**而不是几百条垃圾条目，报告也这么说。
//!
//! fixture 的形状照着真机上看到的来（`/Volumes/新加卷/Game/PSV`，2026-09-01）：
//! NoNpDrm 的 `app/<TitleID>`、MaiDump 的 `<TitleID>`、TitleID 写在目录名里的、
//! 以及 `.vpk` 与被归档裹着的。

use std::fs;
use std::path::Path;

use romcat_core::catalog::Catalog;
use romcat_core::fs::RealFs;
use romcat_core::platform::Manifest;
use romcat_core::report::HealthReport;
use romcat_core::scan::aggregate::{ConflictEvidence, Limits};
use romcat_core::scan::{self, Jobs, ScanOptions, ScanOutcome};
use romcat_core::shape::Role;
use romcat_core::task::Handle;
use romcat_core::testing::sample::{chd, gba, iso, nds, zip};
use romcat_core::testing::{TempDir, temp_dir};

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

/// 一份把这张票每条验收都摆上一份的 fixture 主库。
fn 建库() -> TempDir {
    let dir = temp_dir("shaping");
    let root = dir.path();

    // ── PSV：四种安装形态并存，共 1 个 NoNpDrm + 1 个 MaiDump + 1 个 TitleID 目录名
    //         + 1 个 vpk + 1 个归档 = 5 个变体
    let nonpdrm = root.join("PSV/PSVENJP/零之轨迹[PCSG00042][日版]");
    写(&nonpdrm.join("app/PCSG00042/eboot.bin"), &[1u8; 64]);
    写(&nonpdrm.join("app/PCSG00042/sce_sys/param.sfo"), &[2u8; 32]);
    写(&nonpdrm.join("app/PCSG00042/data.psarc"), &[3u8; 128]);
    for i in 0..40 {
        写(
            &nonpdrm.join(format!("app/PCSG00042/bgm/{i}.at9")),
            &[4u8; 16],
        );
    }
    写(&nonpdrm.join("patch/PCSG00042/eboot.bin"), &[5u8; 64]);
    写(
        &nonpdrm.join("addcont/PCSG00042/TWDS20000000DLC2/x.psarc"),
        &[6u8; 24],
    );

    let maidump = root.join("PSV/PSVENJP/某游戏[PCSG00162][Mai]");
    写(&maidump.join("PCSG00162/eboot.bin"), &[7u8; 64]);
    写(&maidump.join("PCSG00162/data.psarc"), &[8u8; 96]);
    写(&maidump.join("PCSG00162_patch/eboot.bin"), &[9u8; 64]);

    写(
        &root.join("PSV/AIME00001(wan华镜 v3.1)/AIME00001.tar.zst"),
        &[10u8; 200],
    );
    写(&root.join("PSV/去月球/ToTheMoon.vpk"), &[11u8; 300]);
    写(&root.join("PSV/虚之少女-PSV-V1.0版.zip"), &zip(512));

    // ── PS3：整个 PS3_GAME 所在目录是一个变体
    写(
        &root.join("ps3/某游戏/PS3_GAME/USRDIR/EBOOT.BIN"),
        &[1u8; 64],
    );
    写(&root.join("ps3/某游戏/PS3_GAME/ICON0.PNG"), &[2u8; 16]);
    写(&root.join("ps3/某游戏/PS3_UPDATE/PS3UPDAT.PUP"), &[3u8; 16]);
    写(&root.join("ps3/某游戏/PARAM.SFO"), &[4u8; 16]);

    // ── PS1：cue + bin 一个变体；多碟同族一个变体
    写(&root.join("ps/生化危机/生化危机.cue"), b"FILE \"x.bin\"");
    写(&root.join("ps/生化危机/生化危机.bin"), &[1u8; 512]);
    写(&root.join("ps/龙骑士传说/传说 (Disc 1).chd"), &chd());
    写(&root.join("ps/龙骑士传说/传说 (Disc 2).chd"), &chd());
    写(&root.join("ps/龙骑士传说/传说 (Disc 3).chd"), &chd());

    // ── FC：一文件一变体；旁边的媒体与文档不成变体
    写(&root.join("FC/超级马里奥.zip"), &zip(1024));
    写(&root.join("FC/封面.png"), &[0u8; 32]);
    写(&root.join("FC/说明.txt"), "随便写点什么".as_bytes());

    // ── 冲突：目录说 GBA，文件是 NDS（ADR-0011 举的正是这个例子）
    写(&root.join("gba/其实是掌机双屏的.nds"), &nds(b"ANDJ"));
    写(&root.join("gba/正经的.gba"), &gba(b"AGSJ"));

    // ── 范围之外：`杂志` 没映射，`pc` 是明确排除的
    写(&root.join("杂志/电软/第一期.cbz"), &zip(64));
    写(&root.join("pc/某游戏/game.iso"), &iso());

    // ── 库根下的散文件：连顶层目录都没有
    写(&root.join("散落的游戏.gba"), &gba(b"AGBJ"));

    dir
}

fn 扫(root: &Path, catalog: &mut Catalog) -> ScanOutcome {
    let mut options = ScanOptions::named(root, "库");
    options.jobs = Jobs::Fixed(4);
    options.samples_per_class = 64;
    scan::scan(&RealFs::new(), catalog, &options, &Handle::new()).expect("扫描不该失败")
}

fn 变体数(report: &HealthReport, platform: &str) -> u64 {
    report
        .shaping
        .by_platform
        .iter()
        .find(|p| p.name == platform)
        .map_or(0, |p| p.variants)
}

#[test]
fn psv_那一堆内部资源在真盘上收敛成几个变体() {
    let dir = 建库();
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let outcome = 扫(dir.path(), &mut catalog);
    let report = &outcome.report;

    assert!(outcome.shaped, "扫完了就该成型");
    assert_eq!(变体数(report, "PSV"), 5, "四种安装形态各成各的变体");

    let psv = report
        .shaping
        .by_platform
        .iter()
        .find(|p| p.name == "PSV")
        .expect("有 PSV");
    assert!(
        psv.files > 45,
        "PSV 那一侧的文件数是 {}，fixture 里光 at9 就 40 个",
        psv.files
    );
    assert!(
        psv.files_per_variant > 9.0,
        "每变体才 {:.2} 个文件，说明目录树规则没生效——真库里 PSV 是 171,073 个文件",
        psv.files_per_variant
    );

    // 那 40 个 at9 一个都不许自成条目，但它们属于哪个变体要答得上来。
    let 变体 = catalog
        .variant_of("库/PSV/PSVENJP/零之轨迹[PCSG00042][日版]/app/PCSG00042/bgm/7.at9")
        .expect("读得出")
        .expect("它属于某个变体");
    assert_eq!(变体.0, "库/PSV/PSVENJP/零之轨迹[PCSG00042][日版]");
    assert_eq!(变体.1, Role::Internal, "at9 是内部资源，不是变体");
    assert!(
        catalog
            .variant("库/PSV/PSVENJP/零之轨迹[PCSG00042][日版]/app/PCSG00042/bgm/7.at9")
            .expect("读得出")
            .is_none(),
        "内部资源不各自成条目"
    );

    // 追加内容入库、算进变体，但标成附属内容（ADR-0013）。
    assert_eq!(
        catalog
            .variant_of(
                "库/PSV/PSVENJP/零之轨迹[PCSG00042][日版]/addcont/PCSG00042/TWDS20000000DLC2/x.psarc"
            )
            .expect("读得出")
            .map(|(_, role)| role),
        Some(Role::ExtraContent)
    );
}

#[test]
fn 三种成型规则在真盘上各自聚对() {
    let dir = 建库();
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    扫(dir.path(), &mut catalog);

    // cue 加它的 bin
    let cue = catalog
        .variant("库/ps/生化危机/生化危机.cue")
        .expect("读得出")
        .expect("有这个变体");
    assert_eq!(cue.files, 2);
    assert_eq!(cue.main_key, "库/ps/生化危机/生化危机.cue");

    // PS3_GAME 所在目录
    let ps3 = catalog
        .variant("库/ps3/某游戏")
        .expect("读得出")
        .expect("有这个变体");
    assert_eq!(ps3.files, 4, "PS3_UPDATE 与 PARAM.SFO 也吞进来");
    assert_eq!(ps3.main_key, "库/ps3/某游戏");

    // 多碟同族
    let 多碟 = catalog
        .variant("库/ps/龙骑士传说/传说 (Disc 1).chd")
        .expect("读得出")
        .expect("有这个变体");
    assert_eq!(多碟.files, 3, "三张碟是一个变体");
    assert_eq!(
        多碟.files.min(
            catalog
                .variant_members("库/ps/龙骑士传说/传说 (Disc 1).chd")
                .expect("读得出")
                .len() as u64
        ),
        3
    );
    assert!(
        catalog
            .variant("库/ps/龙骑士传说/传说 (Disc 2).chd")
            .expect("读得出")
            .is_none(),
        "第二张碟不另成一个变体"
    );
}

#[test]
fn 报告从统计文件数改为统计变体数() {
    let dir = 建库();
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let report = 扫(dir.path(), &mut catalog).report;

    assert!(report.shaping.shaped);
    assert!(!report.shaping.stale);
    assert!(report.shaping.variants > 0);
    assert!(
        report.shaping.variants < report.totals.files,
        "变体数 {} 该明显少于文件数 {}",
        report.shaping.variants,
        report.totals.files
    );
    // 每变体文件数可解释：变体吃掉的文件数 ÷ 变体数
    let 期望 =
        (report.shaping.files as f64 / report.shaping.variants as f64 * 100.0).round() / 100.0;
    assert!(
        (report.shaping.files_per_variant - 期望).abs() < 1e-9,
        "每变体文件数要对得上：{} vs {}",
        report.shaping.files_per_variant,
        期望
    );
    // 每个平台那一行也带上变体数
    let fc = report
        .platforms
        .iter()
        .find(|p| p.name == "FC")
        .expect("有 FC");
    assert_eq!(fc.variants, 1, "FC 只有 超级马里奥.zip 一个变体");
    assert!(fc.in_scope);

    let text = report.render_text();
    assert!(text.contains("成型：从文件到变体"), "{text}");
    assert!(text.contains("每变体文件数"), "{text}");
    assert!(text.contains("范围边界"), "{text}");
}

#[test]
fn 没进变体的文件按归类拆开报而不是笼统一句() {
    // 「收敛得好」与「压根没成型」不能给出同一个数：一份认不出锚的目录树转储，
    // 它那一堆 `at9` / `psarc` 会整批落在「没进变体」里，而它们既不是媒体也不是垃圾。
    let dir = temp_dir("shaping-gap");
    let root = dir.path();
    // 一份 PSV 转储，故意不放 `param.sfo` / `eboot.bin`，目录名也不像 TitleID
    写(&root.join("PSV/认不出的转储/bgm/a.at9"), &[1u8; 16]);
    写(&root.join("PSV/认不出的转储/data.psarc"), &[2u8; 32]);
    写(&root.join("PSV/封面.png"), &[3u8; 8]);

    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let report = 扫(root, &mut catalog).report;
    let 取 = |category: romcat_core::classify::Category| {
        report
            .shaping
            .unshaped
            .iter()
            .find(|c| c.category == category)
            .map_or(0, |c| c.files)
    };
    assert_eq!(
        取(romcat_core::classify::Category::Unclassified),
        2,
        "at9 与 psarc 落在「未归类」——这一栏就是成型的缺口"
    );
    assert_eq!(
        取(romcat_core::classify::Category::MediaOrMetadata),
        1,
        "封面.png"
    );
    assert!(
        report.render_text().contains("成型的缺口"),
        "报告得把这句话说出来"
    );
}

#[test]
fn 范围之外的三格分得开() {
    let dir = 建库();
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let report = 扫(dir.path(), &mut catalog).report;
    let scope = &report.scope;

    assert!(scope.platforms >= 5, "FC / GBA / PS1 / PS3 / PSV");
    assert_eq!(scope.unmapped_dirs, 1, "只有 杂志 没映射");
    assert_eq!(scope.unmapped_files, 1);
    assert_eq!(scope.excluded.len(), 1, "pc 是明确排除的");
    assert_eq!(scope.excluded[0].name, "pc");
    assert!(scope.excluded[0].reason.contains("平台清单"));
    assert_eq!(scope.root_level_files, 1, "散落的游戏.gba");

    // 范围之外的东西一个变体都不成（ADR-0011 修订段）
    let 杂志 = report
        .platforms
        .iter()
        .find(|p| p.name == "杂志")
        .expect("库体检照样报它");
    assert!(!杂志.in_scope);
    assert_eq!(杂志.variants, 0);
    assert_eq!(杂志.files, 1, "报告的覆盖面大于识别的覆盖面");
}

#[test]
fn 目录说的与文件说的对不上时被报出来() {
    let dir = 建库();
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let report = 扫(dir.path(), &mut catalog).report;

    assert_eq!(report.conflicts.total, 1, "{:?}", report.conflicts.examples);
    let conflict = &report.conflicts.examples[0];
    assert!(conflict.path.ends_with("其实是掌机双屏的.nds"));
    assert_eq!(conflict.declared, "GBA");
    assert_eq!(conflict.implied, "NDS");
    assert_eq!(
        conflict.evidence,
        ConflictEvidence::Confirmed,
        "头部抽样确认过内容真是 NDS，不只是扩展名说了句话"
    );
    assert!(report.render_text().contains("目录说的与文件说的对不上"));
}

#[test]
fn 人工纠正把两个散文件并成一个变体且熬得过重扫() {
    let dir = 建库();
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    扫(dir.path(), &mut catalog);
    assert!(
        catalog
            .variant("库/FC/超级马里奥.zip")
            .expect("读得出")
            .is_some()
    );

    // 维护者说：这个 png 其实是那个变体的一部分。
    catalog
        .set_shaping_override("库/FC/封面.png", "库/FC/超级马里奥.zip")
        .expect("记得下");
    catalog
        .set_shaping_override("库/FC/超级马里奥.zip", "库/FC/超级马里奥.zip")
        .expect("记得下");

    // 重扫一遍：纠正要熬得过去。
    let report = 扫(dir.path(), &mut catalog).report;
    let 并起来的 = catalog
        .variant("库/FC/超级马里奥.zip")
        .expect("读得出")
        .expect("还在");
    assert_eq!(并起来的.files, 2);
    assert!(并起来的.manual);
    assert_eq!(report.shaping.manual, 1);
}

#[test]
fn 加一个平台只要给一份清单() {
    // 「新增一个平台不需要改代码」那条验收的端到端形态：换一份清单，
    // 原本没映射的目录就成了平台，还按新规则成了型。
    let dir = temp_dir("shaping-manifest");
    let root = dir.path();
    写(&root.join("假想机/某游戏/data/main.fic"), &[1u8; 32]);
    写(&root.join("假想机/某游戏/data/bgm.fic"), &[2u8; 32]);
    写(&root.join("假想机/某游戏/CART.ID"), &[3u8; 8]);

    let 清单 = Manifest::parse(
        r#"
"版本" = 1
[["成型规则"]]
"名" = "假想机目录树"
"方式" = "目录树"
"锚文件" = ["CART.ID"]
"变体根" = "锚目录"
[["平台"]]
"名" = "假想机"
"目录" = ["假想机"]
"扩展名" = ["fic"]
"成型" = ["假想机目录树"]
"#,
        "（测试）",
    )
    .expect("清单编得出来");

    let mut options = ScanOptions::named(root, "库");
    options.jobs = Jobs::Fixed(2);
    options.manifest = 清单.clone();
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let outcome =
        scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");

    assert_eq!(变体数(&outcome.report, "假想机"), 1, "整棵树是一个变体");
    let variant = catalog
        .variant("库/假想机/某游戏")
        .expect("读得出")
        .expect("有这个变体");
    assert_eq!(variant.files, 3);
    assert_eq!(variant.platform.as_deref(), Some("假想机"));

    // 换回内置清单：同一个中立库里，`假想机` 立刻变回「还没映射」，一个变体都不成，
    // **而且报告要说出「这份变体表是用另一份清单成的型」**——不然平台那张表按新清单
    // 分组、变体数按旧清单算，两边对不上却什么都不说。
    let 内置 = catalog
        .aggregate(&Limits::default(), &Manifest::builtin())
        .expect("折得出统计");
    assert!(
        !内置.platforms["假想机"].placement.in_scope(),
        "内置清单不认得这个目录，它就该落在范围之外"
    );
    assert!(内置.shaping.manifest_changed, "换了清单要说出来");
    let report = HealthReport::build(&内置, &catalog.report_meta().expect("元信息读得出来"));
    assert!(report.shaping.manifest_changed);
    assert!(report.render_text().contains("另一份平台清单"));
}
