//! 验收**能力档案与格式转换**：真的转一份出来，写到本地 fixture 目标上。
//!
//! 别处验不了的四件事在这里验：
//!
//! 1. **转换真的转得出来**，而且产物是好的——重打包出来的 zip 用本仓库自己的零解压层
//!    读得回去，CRC-32 与原容器里那一条逐条相等（ADR-0014：转换后仍要零解压识别得了）。
//! 2. **转换只产生新文件，主库一个字节不改**（ADR-0004）：转完之后主库那几份的
//!    大小、修改时间、inode 与链接数全部原样。
//! 3. **放不进目标存储的在差量预览阶段就被报出来**（ADR-0017 补充段）：它连一条
//!    步骤都长不出来，于是「传到一半失败」在构造上不会发生。
//! 4. **转换产物默认不缓存**；给了缓存目录才落第三份，第二台设备直接命中。
//!
//! 目标设备**一律拿本地临时目录模拟**：绝不去动任何真实设备或 SD 卡。

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::{fs, io};

use romcat_core::capability::{Filesystem, Profile, RejectReason, Roster};
use romcat_core::catalog::Roots;
use romcat_core::task::Handle;
use romcat_core::catalog::Catalog;
use romcat_core::container::{self, ReadPlan};
use romcat_core::fs::RealFs;
use romcat_core::scan::{self, Jobs, ScanOptions};
use romcat_core::sublibrary::{self, Rule, Selection, Sublibrary};
use romcat_core::sync::{self, Act, Manifest, Sources};
use romcat_core::testing::{TempDir, temp_dir};
use sevenz_rust2::{ArchiveEntry, ArchiveWriter};

/// 一份**压不动**的样本：线性同余伪随机。
///
/// 故意不用周期性的图案——那种东西 LZMA2 能压到几百字节，于是「转出来多大」「转多久」
/// 这两个数在测试里全塌成 0，而这张票要验的正是它们。
fn 样本(seed: u8, len: usize) -> Vec<u8> {
    let mut state = u64::from(seed).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..len)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) as u8
        })
        .collect()
}

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

/// 造一个 7z。
fn 七z(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut buffer = Cursor::new(Vec::new());
    {
        let mut writer = ArchiveWriter::new(&mut buffer).expect("能开始写 7z");
        for (name, data) in entries {
            writer
                .push_archive_entry(
                    ArchiveEntry::new_file(name),
                    Some(Cursor::new(data.clone())),
                )
                .expect("能写");
        }
        writer.finish().expect("能收尾");
    }
    buffer.into_inner()
}

/// 造一个 zip（stored，够用）。
fn 建zip(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    use romcat_core::testing::container::{ZipEntrySpec, zip_container};
    let specs: Vec<ZipEntrySpec> = entries
        .iter()
        .map(|(name, data)| ZipEntrySpec::deflated(name, data.clone()))
        .collect();
    zip_container(&specs)
}

/// 一趟同步要的全套。
struct 现场 {
    _库: TempDir,
    _工作区: TempDir,
    卡: TempDir,
    catalog: Catalog,
    库根: PathBuf,
    卡带原文: Vec<u8>,
    镜像原文: Vec<u8>,
}

impl 现场 {
    fn 摆好() -> Self {
        let 库 = temp_dir("conv-lib");
        let 卡带原文 = 样本(7, 96 * 1024);
        let 镜像原文 = 样本(9, 160 * 1024);
        // SFC 的卡带包是 7z：Snes9x 与 ares 只吃 zip，于是它要被重打包。
        写(
            &库.path().join("SFC/魂斗罗.7z"),
            &七z(&[("魂斗罗.sfc", 卡带原文.clone())]),
        );
        // PS1 的光盘镜像装在 zip 里：光盘类模拟器一个归档都不吃，于是它要被解出来。
        写(
            &库.path().join("PS1/最终幻想7.zip"),
            &建zip(&[("最终幻想7.chd", 镜像原文.clone())]),
        );
        let 工作区 = temp_dir("conv-ws");
        let 卡 = temp_dir("conv-card");
        let 库根 = 库.path().to_path_buf();
        let mut catalog = Catalog::open_in_memory().expect("能开中立库");
        let mut options = ScanOptions::named(&库根, "库");
        options.jobs = Jobs::Fixed(2);
        scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");
        Self {
            _库: 库,
            _工作区: 工作区,
            卡,
            catalog,
            库根,
            卡带原文,
            镜像原文,
        }
    }

    fn 选中(&self, 规则: &str) -> sublibrary::Selected {
        let selection = Selection {
            rules: vec![Rule::parse(规则).expect("规则读得懂")],
            exceptions: Vec::new(),
        };
        let facts = sublibrary::facts(&self.catalog).expect("折得出事实");
        sublibrary::select(&selection, &facts)
    }

    /// 排一趟计划。**只读**：中立库 + 看一眼目标。
    fn 排(
        &self,
        规则: &str,
        profile: &Profile,
        manifest: &Manifest,
    ) -> (sync::Desired, sync::Plan) {
        let selected = self.选中(规则);
        let mut desired = sync::desired(&self.catalog, &selected, profile).expect("折得出期望状态");
        desired.files.sort_by(|a, b| a.path.cmp(&b.path));
        desired.screen(&profile.filesystem, 0);
        let mut 子库 = Sublibrary::at("掌机", self.卡.path(), "Pegasus", None);
        子库.capability = Some(profile.name.clone());
        let actual = sync::observe(&RealFs, self.卡.path()).expect("看得见目标");
        let plan = sync::plan(&子库, &desired, manifest, &actual, sync::Options::default());
        (desired, plan)
    }

    /// 真的跑一趟。
    fn 跑(
        &self,
        desired: &sync::Desired,
        plan: &sync::Plan,
        缓存: Option<&Path>,
    ) -> sync::Outcome {
        let actual = sync::observe(&RealFs, self.卡.path()).expect("看得见目标");
        let from_pool = std::collections::BTreeMap::new();
        let generated = std::collections::BTreeMap::new();
        let sources = Sources {
            library: &RealFs,
            library_roots: Some(&Roots::single("库", &self.库根)),
            target_root: self.卡.path(),
            from_pool: &from_pool,
            generated: &generated,
            link_probe_dir: None,
            convert_cache: 缓存,
        };
        sync::execute::run(
            plan,
            desired,
            &actual,
            &Manifest::empty(),
            &sources,
            &Handle::new(),
        )
        .expect("执行得动")
    }
}

/// 主库里一份文件的「一个字节都没动」凭据。
#[derive(Debug, PartialEq, Eq)]
struct 原样 {
    len: u64,
    mtime: Option<std::time::SystemTime>,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    links: u64,
    bytes: Vec<u8>,
}

fn 取证(path: &Path) -> 原样 {
    let meta = fs::metadata(path).expect("读得到");
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    原样 {
        len: meta.len(),
        mtime: meta.modified().ok(),
        #[cfg(unix)]
        inode: meta.ino(),
        #[cfg(unix)]
        links: meta.nlink(),
        bytes: fs::read(path).expect("读得出"),
    }
}

/// 把一份 zip 里的每条内部条目整份读出来。
fn 读zip内容(path: &Path) -> Vec<(String, u32, Vec<u8>)> {
    let listing = container::list(&RealFs, path).expect("读得回去");
    let plan = ReadPlan::all(&listing);
    let mut out = Vec::new();
    container::read_entries(
        &RealFs,
        path,
        &listing,
        &plan,
        &mut |entry, reader: &mut dyn Read| -> io::Result<()> {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes)?;
            out.push((entry.path.clone(), entry.crc32.unwrap_or(0), bytes));
            Ok(())
        },
    )
    .expect("读得动");
    out
}

// ───────────────────────── 一、真的转得出来

#[test]
fn 卡带的_7z_重打包成_zip_而且产物零解压读得回去() {
    let 现场 = 现场::摆好();
    let profile = Roster::builtin()
        .find("独立模拟器-exfat")
        .expect("内置有这一份")
        .clone();
    let (desired, plan) = 现场.排("平台=SFC", &profile, &Manifest::empty());

    // 差量预览里就说得出「要转几个、要读多少、大概多久」。
    assert_eq!(plan.converts.files, 1, "一个 7z 要转成 zip");
    assert!(plan.convert_source_bytes > 0);
    assert!(plan.convert_ms > 0, "耗时预估不该是 0");
    assert_eq!(plan.convert_estimated, 1, "重打包的产物大小是估的");
    let 那一步 = plan
        .steps
        .iter()
        .find(|step| step.convert.is_some())
        .expect("有一条转换步骤");
    assert_eq!(那一步.act, Act::Add);
    assert_eq!(那一步.source, "库/SFC/魂斗罗.7z", "源仍然指着主库里的原始形态");
    assert_eq!(那一步.path, "SFC/魂斗罗.zip", "落点是产物——相对子库根，不带根名");

    let 转之前 = 取证(&现场.库根.join("SFC/魂斗罗.7z"));
    let outcome = 现场.跑(&desired, &plan, None);
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
    assert_eq!(outcome.converted.files, 1);
    assert_eq!(outcome.convert_cached, 0, "默认不缓存");

    // 产物在卡上，而且**本仓库自己的零解压层读得回去**——转换后仍要零解压识别得了
    // （ADR-0014：zip 的中央目录存 CRC-32）。
    let 产物 = 现场.卡.path().join("SFC/魂斗罗.zip");
    assert!(产物.is_file(), "产物该在卡上");
    let 内容 = 读zip内容(&产物);
    assert_eq!(内容.len(), 1);
    assert_eq!(内容[0].0, "魂斗罗.sfc");
    assert_eq!(内容[0].2, 现场.卡带原文, "解出来逐字节等于原内容");
    assert_eq!(
        内容[0].1,
        romcat_core::testing::container::crc32(&现场.卡带原文),
        "中央目录里的 CRC-32 对得上",
    );

    // ⭐ **主库一个字节不改**（ADR-0004）：大小、修改时间、inode、链接数全部原样。
    assert_eq!(取证(&现场.库根.join("SFC/魂斗罗.7z")), 转之前);
}

#[test]
fn 光盘类不吃归档_把镜像解出来成裸文件() {
    let 现场 = 现场::摆好();
    let profile = Roster::builtin()
        .find("独立模拟器-exfat")
        .expect("内置有这一份")
        .clone();
    let (desired, plan) = 现场.排("平台=PS1", &profile, &Manifest::empty());

    let 那一步 = plan
        .steps
        .iter()
        .find(|step| step.convert.is_some())
        .expect("有一条转换步骤");
    assert_eq!(那一步.path, "PS1/最终幻想7.chd");
    // 未压缩大小零解压就在容器头里，是**准数**：预览里那个容量不是估的。
    assert_eq!(那一步.bytes, 现场.镜像原文.len() as u64);
    assert_eq!(plan.convert_estimated, 0);

    let 转之前 = 取证(&现场.库根.join("PS1/最终幻想7.zip"));
    let outcome = 现场.跑(&desired, &plan, None);
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
    let 产物 = 现场.卡.path().join("PS1/最终幻想7.chd");
    assert_eq!(fs::read(&产物).expect("读得出"), 现场.镜像原文);
    // 卡上那份 zip **不该存在**：转换换掉的是落点，不是多放一份。
    assert!(!现场.卡.path().join("PS1/最终幻想7.zip").exists());
    assert_eq!(取证(&现场.库根.join("PS1/最终幻想7.zip")), 转之前);
}

#[test]
fn 不作声称的档案一个都不转_与票_20_的行为一个字不差() {
    let 现场 = 现场::摆好();
    let (desired, plan) = 现场.排("平台=SFC,PS1", &Profile::unclaimed(), &Manifest::empty());
    assert_eq!(plan.converts.files, 0);
    assert_eq!(plan.convert_ms, 0);
    assert!(plan.unsupported.is_empty());
    assert!(plan.rejected.is_empty());
    let outcome = 现场.跑(&desired, &plan, None);
    assert_eq!(outcome.converted.files, 0);
    assert!(现场.卡.path().join("SFC/魂斗罗.7z").is_file(), "原样搬过去");
    assert!(现场.卡.path().join("PS1/最终幻想7.zip").is_file());
}

// ───────────────────────── 二、放不进目标存储的在预览阶段就报出来

/// 一份单文件上限只有 1 KiB 的档案：不必真造一个 4 GiB 文件，就能走完整条路。
///
/// 真正那个 4 GiB 的数由 `capability` 的单元测试对着内置 FAT32 档案钉死，
/// 这里验的是**它怎么走到差量预览里**。
const 小卡: &str = r#"
"版本" = 1
[["文件系统"]]
"名" = "小卡"
"单文件上限" = 1024
"来源" = "测试用：把 FAT32 那 4 GiB 缩成 1 KiB，好在不造大文件的前提下走完整条路"
"核实日期" = "2026-09-02"
[["平台矩阵"]]
"名" = "全都吃"
[["平台矩阵"."条目"]]
"平台" = ["*"]
"吃" = ["7z", "zip", "sfc", "chd"]
"来源" = "测试用"
"核实日期" = "2026-09-02"
[["能力档案"]]
"名" = "不作声称"
"平台矩阵" = "全都吃"
"文件系统" = "小卡"
"#;

#[test]
fn 超出单文件上限的在差量预览阶段就报出来_而且一条步骤都长不出来() {
    let 现场 = 现场::摆好();
    let profile = Roster::parse(小卡, "（测试）")
        .expect("读得出")
        .find("不作声称")
        .expect("有")
        .clone();
    let (desired, plan) = 现场.排("平台=SFC,PS1", &profile, &Manifest::empty());

    assert_eq!(plan.rejected.len(), 2, "两份都超过 1 KiB");
    assert!(
        plan.rejected
            .iter()
            .all(|file| file.reason == RejectReason::TooBig)
    );
    assert!(
        plan.rejected
            .iter()
            .all(|file| file.detail.contains("小卡")),
        "报告要说清是哪个文件系统拦的",
    );
    // ⭐ **连一条步骤都长不出来**：于是「传到一半失败」在构造上不会发生。
    assert_eq!(plan.steps.len(), 0);
    assert_eq!(plan.adds.files, 0);
    let 预览 = plan.render_text();
    assert!(预览.contains("放不进目标存储"), "{预览}");
    assert!(预览.contains("这一趟一个都不传"), "{预览}");

    let outcome = 现场.跑(&desired, &plan, None);
    assert_eq!(outcome.touched(), 0);
    assert_eq!(fs::read_dir(现场.卡.path()).expect("列得开").count(), 0);
}

#[test]
fn 放不进去的不新增_也不删除() {
    // 目标上已经有一份、清单里也记着，而档案说它放不进去——**不删**。
    // 这条声明本身就可能是错的，而删掉别人的东西是这条链路上唯一不可逆的动作。
    let 现场 = 现场::摆好();
    let 宽 = Profile::unclaimed();
    let (desired, plan) = 现场.排("平台=SFC", &宽, &Manifest::empty());
    let outcome = 现场.跑(&desired, &plan, None);
    let 清单 = outcome.manifest;
    assert_eq!(清单.files.len(), 1);

    let 窄 = Roster::parse(小卡, "（测试）")
        .expect("读得出")
        .find("不作声称")
        .expect("有")
        .clone();
    let (_, plan) = 现场.排("平台=SFC", &窄, &清单);
    assert_eq!(plan.rejected.len(), 1);
    assert_eq!(plan.deletes.files, 0, "放不进去不是删掉它的理由");
    assert_eq!(plan.steps.len(), 0);
    assert!(现场.卡.path().join("SFC/魂斗罗.7z").is_file());
}

#[test]
fn 内置_fat32_档案对一份_4_5_gib_的镜像判放不下() {
    // 不造 4 GiB 的文件，直接拿真档案里那个真数去筛——ADR-0017 补充段点名的
    // 正是「PS2 / PSP 的大 ISO 在 FAT32 上直接放不进去」。
    let fat32: Filesystem = Roster::builtin()
        .find("retroarch-fat32")
        .expect("内置有")
        .filesystem
        .clone();
    let mut desired = sync::Desired::default();
    desired.files.push(sync::DesiredFile {
        path: "PS2/某作.iso".to_string(),
        kind: sync::FileKind::Rom,
        bytes: 4_831_838_208,
        unreadable: false,
        source: "库/PS2/某作.iso".to_string(),
        source_stamp: sync::Stamp {
            bytes: 4_831_838_208,
            mtime_ns: Some(1),
        },
        variant: "库/PS2/某作.iso".to_string(),
        convert: None,
    });
    desired.screen(&fat32, 0);
    assert!(desired.files.is_empty());
    assert_eq!(desired.rejected.len(), 1);
    assert_eq!(desired.rejected[0].reason, RejectReason::TooBig);
    assert!(desired.rejected[0].detail.contains("4.00 GiB"));
}

// ───────────────────────── 三、转不了的如实报出来

#[test]
fn 目标吃不下而且转不了的照搬_但点名说出口() {
    let 库 = temp_dir("conv-rar-lib");
    // `.rar` 几乎无人支持，而这一版读不了它——**照搬，但报出来**。
    写(&库.path().join("SFC/魂斗罗.rar"), &样本(3, 4096));
    let 工作区 = temp_dir("conv-rar-ws");
    let 卡 = temp_dir("conv-rar-card");
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::named(库.path(), "库");
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");

    let profile = Roster::builtin()
        .find("retroarch-exfat")
        .expect("内置有")
        .clone();
    let selection = Selection {
        rules: vec![Rule::parse("平台=SFC").expect("读得懂")],
        exceptions: Vec::new(),
    };
    let facts = sublibrary::facts(&catalog).expect("折得出事实");
    let selected = sublibrary::select(&selection, &facts);
    let desired = sync::desired(&catalog, &selected, &profile).expect("折得出期望状态");

    assert_eq!(desired.unsupported.len(), 1);
    assert_eq!(desired.unsupported[0].path, "SFC/魂斗罗.rar");
    assert!(desired.unsupported[0].want.contains("zip"));
    // **照样搬过去**：不搬是静默丢掉用户亲手挑中的东西。
    assert_eq!(desired.files.len(), 1);
    assert_eq!(desired.files[0].path, "SFC/魂斗罗.rar");

    let mut 子库 = Sublibrary::at("掌机", 卡.path(), "Pegasus", None);
    子库.capability = Some(profile.name.clone());
    let actual = sync::observe(&RealFs, 卡.path()).expect("看得见目标");
    let plan = sync::plan(
        &子库,
        &desired,
        &Manifest::empty(),
        &actual,
        sync::Options::default(),
    );
    let 预览 = plan.render_text();
    assert!(预览.contains("到了目标上打不开"), "{预览}");
    assert!(预览.contains("SFC/魂斗罗.rar"), "{预览}");
    drop(工作区);
}

// ───────────────────────── 四、转换缓存

#[test]
fn 默认不缓存_给了目录才落第三份而且第二趟直接命中() {
    let 现场 = 现场::摆好();
    let profile = Roster::builtin()
        .find("独立模拟器-exfat")
        .expect("内置有")
        .clone();

    // 默认：不给缓存目录，边转边流式写进目标。
    let (desired, plan) = 现场.排("平台=SFC", &profile, &Manifest::empty());
    let outcome = 现场.跑(&desired, &plan, None);
    assert_eq!(outcome.converted.files, 1);
    assert_eq!(outcome.convert_cached, 0);

    // 把卡清空，改成开缓存再来两趟。
    for entry in fs::read_dir(现场.卡.path()).expect("列得开") {
        let path = entry.expect("读得到").path();
        if path.is_dir() {
            fs::remove_dir_all(&path).expect("删得掉");
        } else {
            fs::remove_file(&path).expect("删得掉");
        }
    }
    let 缓存 = temp_dir("conv-cache");
    let (desired, plan) = 现场.排("平台=SFC", &profile, &Manifest::empty());
    let 第一趟 = 现场.跑(&desired, &plan, Some(缓存.path()));
    assert_eq!(第一趟.converted.files, 1);
    assert_eq!(第一趟.convert_cached, 0, "缓存是空的，这一趟真转了");
    let 缓存里 = count_files(缓存.path());
    assert_eq!(缓存里, 1, "产物落进了缓存");

    // 第二台设备：同一份源、同一条配方，直接命中。
    let 卡二 = temp_dir("conv-card2");
    let selected = 现场.选中("平台=SFC");
    let mut desired2 = sync::desired(&现场.catalog, &selected, &profile).expect("折得出期望状态");
    desired2.screen(&profile.filesystem, 0);
    let mut 子库二 = Sublibrary::at("备用卡", 卡二.path(), "Pegasus", None);
    子库二.capability = Some(profile.name.clone());
    let actual2 = sync::observe(&RealFs, 卡二.path()).expect("看得见目标");
    let plan2 = sync::plan(
        &子库二,
        &desired2,
        &Manifest::empty(),
        &actual2,
        sync::Options::default(),
    );
    let from_pool = std::collections::BTreeMap::new();
    let generated = std::collections::BTreeMap::new();
    let sources = Sources {
        library: &RealFs,
        library_roots: Some(&Roots::single("库", &现场.库根)),
        target_root: 卡二.path(),
        from_pool: &from_pool,
        generated: &generated,
        link_probe_dir: None,
        convert_cache: Some(缓存.path()),
    };
    let 第二趟 = sync::execute::run(
        &plan2,
        &desired2,
        &actual2,
        &Manifest::empty(),
        &sources,
        &Handle::new(),
    )
    .expect("执行得动");
    assert_eq!(第二趟.convert_cached, 1, "第二台设备直接从缓存取");
    assert_eq!(count_files(缓存.path()), 1, "缓存里还是那一份");
    assert_eq!(
        fs::read(卡二.path().join("SFC/魂斗罗.zip")).expect("读得出"),
        fs::read(现场.卡.path().join("SFC/魂斗罗.zip")).expect("读得出"),
        "两台设备上是同一份字节",
    );
}

fn count_files(root: &Path) -> usize {
    let mut count = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("列得开") {
            let path = entry.expect("读得到").path();
            if path.is_dir() {
                stack.push(path);
            } else {
                count += 1;
            }
        }
    }
    count
}
