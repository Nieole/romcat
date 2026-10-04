//! 验收**同步执行与媒体导出**这一层：真的往目标上写字节。
//!
//! 计划器那一侧由 `tests/sync.rs` 验，这个文件验的是别处验不了的四件事：
//!
//! 1. **只走计划里的那几步**——目标上手动拷进去的存档、金手指、截图，同步前后
//!    连修改时间都一样（ADR-0015）。
//! 2. **写完从目标读回来**：清单里记的戳与盘上真实的戳逐条相等。
//! 3. **中断后状态一致、可续跑**：落点上没有半份文件，清单描述的是到中断为止
//!    真实有什么，再跑一趟就接上。
//! 4. **媒体同卷就链接、不同卷或不支持就复制**（ADR-0009）——链接那一支要证明的是
//!    「不重复占用空间」，于是断言的是两条路径指着同一个 inode。
//!
//! 目标设备**一律拿本地临时目录模拟**：绝不去动任何真实设备或 SD 卡。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use romcat_core::adapter;
use romcat_core::capability::Profile;
use romcat_core::catalog::Catalog;
use romcat_core::catalog::Roots;
use romcat_core::catalog::scrape::{Harvested, HarvestedMedia};
use romcat_core::fs::{DirEntry, LibraryFs, MemFs, ReadSeek, RealFs};
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::scrape::measure::Measured;
use romcat_core::scrape::pool::MediaPool;
use romcat_core::scrape::priority::Priorities;
use romcat_core::scrape::{AnchorKind, MediaKind};
use romcat_core::sublibrary::{self, Rule, Selection, Sublibrary};
use romcat_core::sync::{self, Act, FileKind, Manifest, Placement, Sources};
use romcat_core::task::Handle;
use romcat_core::testing::sample::zip;
use romcat_core::testing::target::{self, Folding};
use romcat_core::testing::{TempDir, temp_dir};

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

/// 一份小 fixture 主库。
fn 建库() -> TempDir {
    let dir = temp_dir("run-lib");
    写(&dir.path().join("FC/魂斗罗.zip"), &zip(2048));
    写(&dir.path().join("FC/超级玛丽.zip"), &zip(4096));
    写(&dir.path().join("GB/口袋妖怪.zip"), &zip(8192));
    dir
}

/// 一份主库，里面有一个**大到一块读不完**的文件。
///
/// 逐块中断那条路要它：`copy_stream` 每读一块看一眼中断信号，文件比一块还小的话，
/// 那个分支一次都走不到。
fn 建个大库() -> TempDir {
    let dir = temp_dir("run-big-lib");
    写(&dir.path().join("FC/大部头.zip"), &zip(16 * 1024 * 1024));
    dir
}

fn 扫成库(root: &Path) -> Catalog {
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let mut options = ScanOptions::named(root, "库");
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &Handle::new()).expect("扫得动");
    catalog
}

fn 选中(catalog: &Catalog, 规则: &str) -> sublibrary::Selected {
    let selection = Selection {
        rules: vec![Rule::parse(规则).expect("规则读得懂")],
        exceptions: Vec::new(),
    };
    let facts = sublibrary::facts(catalog).expect("折得出事实");
    sublibrary::select(&selection, &facts)
}

/// 一趟同步要的全套：期望状态、计划、执行用的那几样。
struct 现场 {
    _库: TempDir,
    工作区: TempDir,
    卡: TempDir,
    catalog: Catalog,
    pool: MediaPool,
    库根: PathBuf,
    /// 卡所在的那块**真盘**不分大小写——默认 APFS、Windows 是，CI 的 ext4 不是。
    ///
    /// 真盘上有几格只在分大小写的盘上摆得出来。照测试夹具已有的做法（`testing::target`）
    /// 先问，答「不分」的那几格如实跳过并印「跳过：…」；问不出来（`None`）照跑，不当成
    /// 任何一边。**摆现场时就问**：卡上还什么都没有，答案不沾被测代码写下的东西。
    卡不分大小写: bool,
}

impl 现场 {
    fn 摆好() -> Self {
        Self::摆在(建库())
    }

    fn 摆在(库: TempDir) -> Self {
        let 工作区 = temp_dir("run-ws");
        let 卡 = temp_dir("run-card");
        let 卡不分大小写 = romcat_core::fs::case_insensitive(&RealFs, 卡.path()) == Some(true);
        let 库根 = 库.path().to_path_buf();
        let catalog = 扫成库(&库根);
        let pool = MediaPool::open(&工作区.path().join("media")).expect("池建得出");
        Self {
            _库: 库,
            工作区,
            卡,
            catalog,
            pool,
            库根,
            卡不分大小写,
        }
    }

    /// **挡下来就一个字节都不写**：维护者那份旁边没多出一份 `落点`。
    ///
    /// 只有分大小写的真盘上断得了：不分大小写的盘上 `落点` 就是维护者那份
    /// （`GB/tetris.zip` 就是 `GB/Tetris.zip`），问它在不在答的永远是「在」。那块盘上也
    /// 用不着它——闸真漏了，字节正落在维护者那份身上，「一个字节都不许动」那条已经咬住了。
    /// 于是不分就如实跳过，印一行「跳过：…」。`注` 接在断言与跳过那一行末尾，一条测试
    /// 跑两档视图时用来分清是哪一档；只跑一趟的给空串。
    fn 旁边一个字节都没写(&self, 落点: &Path, 注: &str) {
        let 相对 = 落点.strip_prefix(self.卡.path()).unwrap_or(落点);
        if self.卡不分大小写 {
            eprintln!(
                "跳过：临时目录所在的盘不分大小写，`{}` 就是维护者那份，\
                 「旁边一个字节都不写」这一格只有分大小写的盘上断得了{注}",
                相对.display()
            );
        } else {
            assert!(
                !落点.exists(),
                "挡下来就一个字节都不写，可卡上多了一份 `{}`{注}",
                相对.display()
            );
        }
    }

    /// 往**媒体池**里塞一份媒体，并把它挂到某个变体上。
    fn 收一份媒体(&mut self, 变体: &str, kind: MediaKind, bytes: &[u8]) -> String {
        let hash = romcat_core::catalog::frontend::hash_of(bytes);
        let at = self.pool.path_of(&hash, "png");
        写(&at, bytes);
        self.catalog
            .put_media(&hash, "png", bytes.len() as u64, Measured::default())
            .expect("池里记得下");
        self.catalog
            .put_scraped(&[Harvested {
                anchor: AnchorKind::Variant.label().to_string(),
                subject: 变体.to_string(),
                // 一个源在一个锚点上写两次是**同一个结果**（`put_scraped`），
                // 于是两份媒体得来自两个源，不然第二次会把第一次的删掉。
                source: format!("本地媒体-{}", kind.label()),
                input: format!("{变体}/{hash}"),
                values: Vec::new(),
                media: vec![HarvestedMedia {
                    kind: kind.label().to_string(),
                    hash: hash.clone(),
                    evidence: "测试".to_string(),
                }],
            }])
            .expect("引用写得进");
        hash
    }

    /// 折一趟：期望状态 + 计划 + 执行要的那几样。**不作声称**的档案，不转也不检查。
    fn 排一趟(&self, 规则: &str, manifest: &Manifest) -> 一趟 {
        self.排一趟_按档案(规则, manifest, &Profile::unclaimed())
    }

    /// 同上，但指定一份**能力档案**——转格式与文件系统检查都从这儿来（票 21）。
    fn 排一趟_按档案(&self, 规则: &str, manifest: &Manifest, profile: &Profile) -> 一趟 {
        let selected = 选中(&self.catalog, 规则);
        let adapter = adapter::find("Pegasus").expect("带着 Pegasus 适配器");
        let priorities = Priorities::builtin();
        let footprint = sync::Footprint::gather(&self.catalog, &selected).expect("读得出脚印");
        let mut desired = footprint.desired(profile);
        // 与 `sync::prepare` 同一个次序：先筛 ROM，多碟变体的播放列表排在筛过之后、再筛一遍；筛完还在的那几份
        // 交给媒体与前端元数据（条目改指它、封面照它的名字铺），最后几样一起再筛一遍。
        desired.screen(&profile.filesystem, 0);
        let playlists = sync::playlist::lay(
            &footprint,
            &desired,
            adapter.as_ref(),
            profile,
            &romcat_core::platform::Manifest::builtin(),
        );
        desired.add_and_screen(playlists.files.iter().cloned(), &profile.filesystem, 0);
        let launch = playlists.launching(&desired);
        let media = sync::media::lay(
            &self.catalog,
            adapter.as_ref(),
            &self.pool,
            &selected,
            &launch,
        )
        .expect("铺得出媒体");
        let frontend = sync::frontend::lay(
            &self.catalog,
            adapter.as_ref(),
            &priorities,
            &selected,
            &media.assets,
            &launch,
        )
        .expect("折得出元数据");
        desired.add_and_screen(
            media.files.iter().chain(&frontend.files).cloned(),
            &profile.filesystem,
            0,
        );
        let mut generated = playlists.bytes;
        generated.extend(frontend.bytes);

        let mut 子库 = Sublibrary::at("掌机", self.卡.path(), "Pegasus", None);
        子库.capability = Some(profile.name.clone());
        let actual = sync::observe(&RealFs, self.卡.path()).expect("看得见目标");
        // 与 `sync::prepare` 同一条线：落点的目录段先与目标折齐，再排计划。
        let mut from_pool = media.from_pool;
        let realign = sync::align(&mut desired, &actual);
        realign.apply(&mut from_pool);
        realign.apply(&mut generated);
        let plan = sync::plan(&子库, &desired, manifest, &actual, sync::Options::default());
        一趟 {
            desired,
            actual,
            plan,
            from_pool,
            generated,
        }
    }
}

struct 一趟 {
    desired: sync::Desired,
    actual: sync::TargetState,
    plan: sync::Plan,
    from_pool: BTreeMap<String, PathBuf>,
    generated: BTreeMap<String, Vec<u8>>,
}

impl 现场 {
    fn 执行(&self, 这趟: &一趟, 清单: &Manifest, cancel: &CancelToken) -> sync::Outcome {
        self.执行_带缓存(这趟, 清单, None, cancel)
    }

    fn 执行_带缓存(
        &self,
        这趟: &一趟,
        清单: &Manifest,
        缓存: Option<&Path>,
        cancel: &CancelToken,
    ) -> sync::Outcome {
        self.执行_全(这趟, 清单, 缓存, &RealFs, cancel)
    }

    /// 把**目标那一侧**换成一份指定[折叠语义](Folding)的只读视图再跑一趟。
    ///
    /// 卡上真实那棵树照一张相放进去（`testing::target`），字节照旧落在真实的临时
    /// 目录里——那道接缝只管读（`sync::execute` 模块文档八）。
    fn 执行_折(
        &self,
        这趟: &一趟,
        清单: &Manifest,
        折叠: Folding,
        cancel: &CancelToken,
    ) -> sync::Outcome {
        let 视图 = target::snapshot(self.卡.path(), 折叠);
        self.执行_折_视图(这趟, 清单, &视图, cancel)
    }

    /// 同上，但视图由调用方自己捏——用来验「塞得进去」这件事本身。
    fn 执行_折_视图(
        &self,
        这趟: &一趟,
        清单: &Manifest,
        目标: &dyn LibraryFs,
        cancel: &CancelToken,
    ) -> sync::Outcome {
        self.执行_全(这趟, 清单, None, 目标, cancel)
    }

    fn 执行_全(
        &self,
        这趟: &一趟,
        清单: &Manifest,
        缓存: Option<&Path>,
        目标: &dyn LibraryFs,
        cancel: &CancelToken,
    ) -> sync::Outcome {
        let sources = Sources {
            library: &RealFs,
            library_roots: Some(&Roots::single("库", &self.库根)),
            target: 目标,
            target_root: self.卡.path(),
            from_pool: &这趟.from_pool,
            generated: &这趟.generated,
            link_probe_dir: Some(&self.pool.scratch()),
            convert_cache: 缓存,
        };
        // 把手与这几条测试手里那个中断信号共用同一个信号：`execute::run` 从票
        // `gui-redesign/15` 起收的是**把手**（它得报得出正在传哪个文件），
        // 而「按停下」照旧是同一件事。
        sync::execute::run(
            &这趟.plan,
            &这趟.desired,
            &这趟.actual,
            清单,
            &sources,
            &Handle::with_cancel(cancel.clone()),
        )
        .expect("执行得动")
    }
}

/// 一棵目录树底下每个文件的 `(相对路径, 字节数, 修改时间)`。
fn 盘上有什么(dir: &Path) -> BTreeMap<String, (u64, std::time::SystemTime)> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        let Ok(entries) = fs::read_dir(&at) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let meta = entry.metadata().expect("读得到");
            out.insert(
                path.strip_prefix(dir)
                    .expect("在树里")
                    .display()
                    .to_string(),
                (meta.len(), meta.modified().expect("有时间")),
            );
        }
    }
    out
}

#[test]
fn 执行只走计划里的那几步_清单之外的东西连时间戳都没动() {
    let 现场 = 现场::摆好();
    // 维护者自己拷进卡里的东西：存档、金手指、截图。**工具连看都不看**（ADR-0015）。
    写(&现场.卡.path().join("saves/魂斗罗.sav"), &[9u8; 512]);
    写(&现场.卡.path().join("cheats/金手指.txt"), b"unlimited");
    写(&现场.卡.path().join("screenshots/一.png"), &[7u8; 64]);
    let 之前: BTreeMap<_, _> = 盘上有什么(现场.卡.path())
        .into_iter()
        .filter(|(path, _)| {
            path.starts_with("saves")
                || path.starts_with("cheats")
                || path.starts_with("screenshots")
        })
        .collect();

    let 这趟 = 现场.排一趟("平台=FC", &Manifest::empty());
    assert!(这趟.plan.steps.iter().all(|step| step.act == Act::Add));
    let outcome = 现场.执行(&这趟, &Manifest::empty(), &CancelToken::new());
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
    assert_eq!(outcome.added.files, 这趟.plan.adds.files);

    let 之后: BTreeMap<_, _> = 盘上有什么(现场.卡.path())
        .into_iter()
        .filter(|(path, _)| {
            path.starts_with("saves")
                || path.starts_with("cheats")
                || path.starts_with("screenshots")
        })
        .collect();
    assert_eq!(之前, 之后, "清单之外的文件连修改时间都不许变");
    // 它们也没混进清单——进了清单，下一趟就成了工具敢删的东西。
    for file in &outcome.manifest.files {
        assert!(
            !file.path.starts_with("saves/")
                && !file.path.starts_with("cheats/")
                && !file.path.starts_with("screenshots/"),
            "{} 混进了清单",
            file.path
        );
    }
}

#[test]
fn 清单里的戳是写完从目标上读回来的那一个() {
    let 现场 = 现场::摆好();
    let 这趟 = 现场.排一趟("平台=FC", &Manifest::empty());
    let outcome = 现场.执行(&这趟, &Manifest::empty(), &CancelToken::new());

    let 盘上 = 盘上有什么(现场.卡.path());
    assert!(!outcome.manifest.files.is_empty());
    for file in &outcome.manifest.files {
        let 名 = file.path.replace('/', std::path::MAIN_SEPARATOR_STR);
        let (bytes, modified) = 盘上.get(&名).unwrap_or_else(|| panic!("{名} 该在盘上"));
        assert_eq!(file.stamp.bytes, *bytes, "{名} 的大小");
        let 真的 = romcat_core::catalog::mtime_ns(*modified);
        assert_eq!(file.stamp.mtime_ns, 真的, "{名} 的修改时间");
        assert!(!file.absent);
    }
}

#[test]
fn 中断之后落点上没有半份文件_再跑一趟接着来() {
    let 现场 = 现场::摆好();
    let 这趟 = 现场.排一趟("平台=FC,GB", &Manifest::empty());
    assert!(这趟.plan.steps.len() > 2, "得有几步可断");

    // 一开始就按下停止：一步都不该做成。
    let cancel = CancelToken::new();
    cancel.cancel();
    let 断了 = 现场.执行(&这趟, &Manifest::empty(), &cancel);
    assert!(断了.interrupted);
    assert_eq!(断了.touched(), 0);
    assert!(
        断了.manifest.files.is_empty(),
        "一个都没放上去，清单就是空的"
    );
    assert!(
        盘上有什么(现场.卡.path())
            .keys()
            .all(|name| !name.ends_with(".romcat-part")),
        "落点上不许留半份文件"
    );

    // 接着跑：从头再排一次计划（清单是空的），这一趟全做完。
    let 再一趟 = 现场.排一趟("平台=FC,GB", &断了.manifest);
    let 好了 = 现场.执行(&再一趟, &断了.manifest, &CancelToken::new());
    assert!(!好了.interrupted);
    assert_eq!(好了.added.files, 再一趟.plan.adds.files);

    // 再排一次：什么都不用动——**中断没有留下任何要修的东西**。
    let 第三趟 = 现场.排一趟("平台=FC,GB", &好了.manifest);
    assert_eq!(第三趟.plan.touched(), 0, "{}", 第三趟.plan.render_text());
}

#[test]
fn 媒体同卷时走硬链接_不重复占用空间() {
    let mut 现场 = 现场::摆好();
    现场.收一份媒体("库/FC/魂斗罗.zip", MediaKind::Cover, &[1u8; 4096]);
    现场.收一份媒体("库/FC/魂斗罗.zip", MediaKind::Screenshot, &[2u8; 2048]);

    let 这趟 = 现场.排一趟("平台=FC", &Manifest::empty());
    let 媒体步 = 这趟
        .plan
        .steps
        .iter()
        .filter(|step| step.kind == FileKind::Media)
        .count();
    assert_eq!(媒体步, 2, "两份媒体都进了计划");

    let outcome = 现场.执行(&这趟, &Manifest::empty(), &CancelToken::new());
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
    // 临时目录与媒体池都在同一个卷上，于是**探测的结论必然是链接**。
    assert_eq!(outcome.placement, Some(Placement::Link));
    assert_eq!(outcome.linked, 2, "两份媒体都是链上去的");

    for (path, 池里) in &这趟.from_pool {
        let 卡上 = 现场.卡.path().join(path);
        assert!(卡上.is_file(), "{path} 该在卡上");
        assert_eq!(
            fs::read(&卡上).expect("读得出"),
            fs::read(池里).expect("读得出")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(
                fs::metadata(&卡上).expect("读得到").ino(),
                fs::metadata(池里).expect("读得到").ino(),
                "同一个 inode 才叫不重复占用空间",
            );
        }
    }
}

#[test]
fn 探不动硬链接就复制_降级路径在任何文件系统上都成立() {
    let mut 现场 = 现场::摆好();
    现场.收一份媒体("库/FC/魂斗罗.zip", MediaKind::Cover, &[3u8; 1024]);
    let 这趟 = 现场.排一趟("平台=FC", &Manifest::empty());

    // `link_probe_dir` 给 `None` 就是「没法探测」——SD 卡的 exFAT / FAT32 上探测
    // 一定报复制，这条降级不是优化项而是必需路径（ADR-0009）。
    let sources = Sources {
        library: &RealFs,
        library_roots: Some(&Roots::single("库", &现场.库根)),
        target: &RealFs,
        target_root: 现场.卡.path(),
        from_pool: &这趟.from_pool,
        generated: &这趟.generated,
        link_probe_dir: None,
        convert_cache: None,
    };
    let outcome = sync::execute::run(
        &这趟.plan,
        &这趟.desired,
        &这趟.actual,
        &Manifest::empty(),
        &sources,
        &Handle::new(),
    )
    .expect("执行得动");
    assert_eq!(outcome.placement, Some(Placement::Copy));
    assert_eq!(outcome.linked, 0);
    for path in 这趟.from_pool.keys() {
        assert!(现场.卡.path().join(path).is_file(), "{path} 照样铺到位了");
    }
}

#[test]
fn 探测本身建不出文件时报复制() {
    // 探不动不该让同步停下来：降级复制在任何文件系统上都成立。
    let 卡 = temp_dir("run-probe");
    let 不存在 = Path::new("/dev/null/建不出来");
    assert_eq!(sync::execute::probe(不存在, 卡.path()), Placement::Copy);
    // 探测过后目标上不许留下探测文件——留一个就是「清单之外」多一条。
    assert_eq!(fs::read_dir(卡.path()).expect("列得开").count(), 0);
}

#[test]
fn 子库内元数据里的路径全部相对子库根() {
    let mut 现场 = 现场::摆好();
    现场.收一份媒体("库/FC/魂斗罗.zip", MediaKind::Cover, &[4u8; 512]);
    let 这趟 = 现场.排一趟("平台=FC", &Manifest::empty());
    现场.执行(&这趟, &Manifest::empty(), &CancelToken::new());

    let 元数据 = 现场.卡.path().join("FC.metadata.pegasus.txt");
    let text = fs::read_to_string(&元数据).expect("元数据落到位了");
    let 主库根 = 现场.库根.display().to_string();
    assert!(!text.contains(&主库根), "绝不写主库的绝对路径：\n{text}");
    assert!(
        !text.contains(&现场.卡.path().display().to_string()),
        "也不写目标的绝对路径——盘符一变就全指不着了：\n{text}"
    );
    for line in text.lines() {
        for key in ["file:", "files:", "assets."] {
            if let Some(rest) = line.trim().strip_prefix(key) {
                let value = rest.trim_start_matches(':').trim();
                assert!(
                    !value.starts_with('/') && !value.contains(":\\"),
                    "「{line}」不是相对路径"
                );
            }
        }
    }
    // 元数据里指着的每一条路径，在卡上都真的有那个文件。
    assert!(text.contains("files: FC/魂斗罗.zip"), "{text}");
    assert!(text.contains("assets.boxFront: media/"), "{text}");
    for line in text.lines() {
        if let Some(value) = line.trim().strip_prefix("assets.boxFront:") {
            let at = 现场.卡.path().join(value.trim());
            assert!(at.is_file(), "元数据指着的 {} 该在卡上", value.trim());
        }
    }
}

#[test]
fn 不要了而且目标上也没有的那些从清单里整条丢掉() {
    let 现场 = 现场::摆好();
    let 第一趟 = 现场.排一趟("平台=FC,GB", &Manifest::empty());
    let 一 = 现场.执行(&第一趟, &Manifest::empty(), &CancelToken::new());
    assert!(一.manifest.files.iter().any(|f| f.path.starts_with("GB/")));

    // 规则改成只要 FC：GB 那几个该被删，清单里也该消失。
    let 第二趟 = 现场.排一趟("平台=FC", &一.manifest);
    assert!(第二趟.plan.deletes.files > 0);
    let 二 = 现场.执行(&第二趟, &一.manifest, &CancelToken::new());
    assert_eq!(二.deleted.files, 第二趟.plan.deletes.files);
    assert!(
        !二.manifest.files.iter().any(|f| f.path.starts_with("GB/")),
        "删掉的东西不该还留在清单里"
    );
    assert!(!现场.卡.path().join("GB/口袋妖怪.zip").exists());
    // 再排一趟：对齐了。
    assert_eq!(现场.排一趟("平台=FC", &二.manifest).plan.touched(), 0);
}

#[test]
fn 复制到一半被中断_落点上留的是完整的旧文件而不是半份() {
    // 前一条中断测试是**开跑前**就按下停止，于是 `copy_stream` 里那条「每读一块看一眼
    // 中断信号」的分支一次都没走到。这一条专走它：库里那份 16 MiB 一块读不完，
    // 另一个线程在第一个 `.romcat-part` 冒出来的那一刻按下停止。
    let 现场 = 现场::摆在(建个大库());
    let 这趟 = 现场.排一趟("平台=FC", &Manifest::empty());
    assert!(
        这趟.plan.adds.bytes > 1024 * 1024,
        "得有一份大到一块读不完的"
    );

    let cancel = CancelToken::new();
    let 卡 = 现场.卡.path().to_path_buf();
    let 按停止 = {
        let cancel = cancel.clone();
        std::thread::spawn(move || {
            // 半份文件一冒头就按下去。等不到也要停——测试不许挂住。
            let 起点 = std::time::Instant::now();
            let mut 看见了半份 = false;
            while 起点.elapsed() < std::time::Duration::from_secs(20) {
                if 有半份文件(&卡) {
                    看见了半份 = true;
                    break;
                }
                std::thread::yield_now();
            }
            cancel.cancel();
            看见了半份
        })
    };
    let 断了 = 现场.执行(&这趟, &Manifest::empty(), &cancel);
    let 看见了半份 = 按停止.join().expect("线程跑得完");

    // 不管停在哪一步，**这三条都得成立**。
    assert!(!有半份文件(现场.卡.path()), "落点上不许留半份文件");
    assert!(断了.failures.is_empty(), "{:?}", 断了.failures);
    for file in &断了.manifest.files {
        let at = 现场.卡.path().join(&file.path);
        let meta = fs::metadata(&at).unwrap_or_else(|_| panic!("{} 该在盘上", file.path));
        assert_eq!(
            meta.len(),
            file.stamp.bytes,
            "{} 记进清单的就得是完整的",
            file.path
        );
    }
    if 看见了半份 {
        // 真的在复制途中停住了：那一份没进清单，落点上也没有它。
        assert!(断了.interrupted, "半路停住就该报中断");
    }

    // 续跑：从剩下的接着来，一趟做完。
    let 再一趟 = 现场.排一趟("平台=FC", &断了.manifest);
    let 好了 = 现场.执行(&再一趟, &断了.manifest, &CancelToken::new());
    assert!(!好了.interrupted);
    assert_eq!(现场.排一趟("平台=FC", &好了.manifest).plan.touched(), 0);
}

#[test]
fn 上一趟遗留的半份文件先清掉再写() {
    // 进程被 `kill -9` 掉、机器断电——临时文件会留在落点旁边。下一趟必须当它不存在，
    // 而不是接着往里写（那会写出一份前半旧后半新的东西）。
    let 现场 = 现场::摆好();
    let 这趟 = 现场.排一趟("平台=FC", &Manifest::empty());
    let 遗留 = 现场.卡.path().join("FC/魂斗罗.zip.romcat-part");
    写(&遗留, "上一趟写了一半的垃圾".as_bytes());

    let outcome = 现场.执行(&这趟, &Manifest::empty(), &CancelToken::new());
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
    assert!(
        !遗留.exists(),
        "遗留的半份该被清掉，而不是留在卡上当「清单之外」"
    );
    let 落点 = 现场.卡.path().join("FC/魂斗罗.zip");
    assert_eq!(
        fs::read(&落点).expect("读得出"),
        fs::read(现场.库根.join("FC/魂斗罗.zip")).expect("读得出"),
        "写出来的是完整的那一份，不是接在垃圾后面的",
    );
}

/// 一份**替人按停**的目标视图：落点闸问到某一条落点时按下停下，别的一律转给真盘。
///
/// 「铺到一半按停」得真的铺下去几份才验得到，而靠时间去抢那一下抢不准（挂单 `Q196`）。
/// 闸在放每一份之前都要问目标一句「这条落点上有没有东西」（`sync::execute` 模块文档八），
/// 这一层把按停钉死在「问到哪一份」上。它自己一个判断都不做。
struct 问到这一份就按停<'a> {
    task: &'a Handle,
    落点: PathBuf,
}

impl LibraryFs for 问到这一份就按停<'_> {
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        RealFs.canonicalize(path)
    }

    fn read_dir(&self, dir: &Path) -> std::io::Result<Vec<DirEntry>> {
        RealFs.read_dir(dir)
    }

    fn read_head(&self, file: &Path, limit: usize) -> std::io::Result<Vec<u8>> {
        if file == self.落点 {
            self.task.stop();
        }
        RealFs.read_head(file, limit)
    }

    fn read_tail(&self, file: &Path, limit: usize) -> std::io::Result<Vec<u8>> {
        RealFs.read_tail(file, limit)
    }

    fn open(&self, file: &Path) -> std::io::Result<Box<dyn ReadSeek + '_>> {
        RealFs.open(file)
    }
}

#[test]
fn 导出铺媒体铺到一半按停_铺过的留在盘上_说得出铺了几份() {
    // 票 `one-criterion-per-thing/08`：导出开着铺媒体时走 `execute::place_media`，每一份
    // 落地的规矩与同步是同一份。按停在两份之间生效：铺过的那几份原样留在盘上，账上说得出
    // 铺了几份，落点上没有半份文件。
    let mut 现场 = 现场::摆好();
    for (字节, kind) in [
        (1u8, MediaKind::Cover),
        (2, MediaKind::Screenshot),
        (3, MediaKind::Video),
    ] {
        现场.收一份媒体("库/FC/魂斗罗.zip", kind, &[字节; 1024]);
    }
    let selected = 选中(&现场.catalog, "平台=FC");
    let adapter = adapter::find("Pegasus").expect("带着 Pegasus 适配器");
    let laid = sync::media::lay(
        &现场.catalog,
        adapter.as_ref(),
        &现场.pool,
        &selected,
        &BTreeMap::new(),
    )
    .expect("铺得出媒体");
    assert_eq!(laid.from_pool.len(), 3, "{:?}", laid.from_pool);
    let 第二份 = laid.from_pool.keys().nth(1).expect("有第二份").clone();

    let task = Handle::new();
    let 视图 = 问到这一份就按停 {
        task: &task,
        落点: 现场.卡.path().join(&第二份),
    };
    let scratch = 现场.pool.scratch();
    let generated = BTreeMap::new();
    let sources = Sources {
        library: &RealFs,
        library_roots: None,
        target: &视图,
        target_root: 现场.卡.path(),
        from_pool: &laid.from_pool,
        generated: &generated,
        link_probe_dir: Some(&scratch),
        convert_cache: None,
    };

    let placed = sync::execute::place_media(&sources, &task).expect("放得动");

    assert!(placed.interrupted, "按停了却没记上：{placed:?}");
    // 按停落在第二份的落点闸上，那一份已经在放了：链接一步到位，复制则逐块看信号当场收手。
    let 该有几份: usize = match placed.placement {
        Some(Placement::Link) => 2,
        _ => 1,
    };
    assert_eq!(placed.placed(), 该有几份 as u64, "{placed:?}");
    let 盘上 = 盘上有什么(现场.卡.path());
    assert_eq!(盘上.len(), 该有几份, "盘上只该有铺过的那几份：{盘上:?}");
    for path in laid.from_pool.keys().take(该有几份) {
        assert!(
            现场.卡.path().join(path).is_file(),
            "{path} 铺过了就得留在盘上"
        );
    }
    assert!(!有半份文件(现场.卡.path()), "落点上不许留半份文件");
}

/// 这棵树底下有没有半份文件。
fn 有半份文件(dir: &Path) -> bool {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        let Ok(entries) = fs::read_dir(&at) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".romcat-part"))
            {
                return true;
            }
        }
    }
    false
}

/// 一份主库，里面那个文件与卡上维护者自己那份**只差大小写**。
fn 建个只差大小写的库() -> TempDir {
    let dir = temp_dir("run-case-lib");
    写(&dir.path().join("GB/tetris.zip"), &zip(2048));
    dir
}

#[test]
fn 大小写不敏感的目标上_清单之外只差大小写的文件不被顶掉() {
    // 卡是 exFAT / FAT32，macOS 默认的 APFS 也一样：**大小写不敏感**。
    // 维护者自己往卡上放了 `GB/Tetris.zip`，而选择集里的变体落点是 `GB/tetris.zip`。
    // 清单之外的文件工具一律不碰（ADR-0015），于是这该报**落点被占**而不是新增。
    let 现场 = 现场::摆在(建个只差大小写的库());
    let 维护者那份 = 现场.卡.path().join("GB/Tetris.zip");
    写(&维护者那份, "这是我自己拷进去的".as_bytes());
    let 原样 = fs::read(&维护者那份).expect("读得出");

    let 这趟 = 现场.排一趟("平台=GB", &Manifest::empty());
    assert!(
        这趟
            .plan
            .surprises
            .iter()
            .any(|s| s.kind == sync::SurpriseKind::Occupied),
        "该报落点被占：{:?}",
        这趟.plan.surprises
    );
    assert!(
        !这趟
            .plan
            .steps
            .iter()
            .any(|step| step.path.eq_ignore_ascii_case("GB/tetris.zip")),
        "落点被占就一步都不该排：{:?}",
        这趟.plan.steps
    );

    let outcome = 现场.执行(&这趟, &Manifest::empty(), &CancelToken::new());
    assert_eq!(
        fs::read(&维护者那份).expect("还在"),
        原样,
        "维护者自己那份连一个字节都不许动",
    );
    assert!(
        !现场.卡.path().join("GB/tetris.zip").exists()
            || fs::read(现场.卡.path().join("GB/tetris.zip")).expect("读得出") == 原样,
        "卡上不该多出一份工具写的 tetris.zip",
    );
    for file in &outcome.manifest.files {
        assert!(
            !file.path.eq_ignore_ascii_case("GB/tetris.zip"),
            "{} 混进了清单",
            file.path
        );
    }
}

#[test]
fn 塞得进一个假目标_不塞就是真盘() {
    // 这道闸问目标的那几句话走 `Sources::target`（`sync::execute` 模块文档八）。
    // **它在不在**，判据是「同一份主库、同一趟计划、两张一样的空卡，塞与不塞答案相反」：
    //
    // - 塞一份说「那儿躺着维护者一份只差大小写的文件」的视图进去 → 挡得住；
    // - 不塞 → 那一问落在**真盘**上，盘上真的没有，于是字节照常落下去。
    //
    // 两边都不是「假装成功」：第一趟落点上一个字节都没写，第二趟写的是真文件。
    // **第二趟另起一张卡**而不是复用第一张：第一趟虽然被挡下的那一步没写东西，
    // 同一趟里的元数据那一步照样落到真卡上了，复用的话它自己会把第二趟挡下来。
    let 现场 = 现场::摆在(建个只差大小写的库());
    let 这趟 = 现场.排一趟("平台=GB", &Manifest::empty());
    let 落点 = 现场.卡.path().join("GB/tetris.zip");
    assert!(!落点.exists(), "真卡上这会儿什么都没有");

    let mut 假卡 = MemFs::insensitive();
    假卡.file(
        现场.卡.path().join("GB/Tetris.zip"),
        "这是我自己拷进去的".as_bytes().to_vec(),
    );
    let 假的 = 现场.执行_折_视图(&这趟, &Manifest::empty(), &假卡, &CancelToken::new());
    assert!(
        假的
            .failures
            .iter()
            .any(|failure| failure.path == "GB/tetris.zip" && failure.act == Act::Add),
        "视图说那儿有东西，闸就得挡下来：{:?}",
        假的.failures
    );
    assert!(!落点.exists(), "挡下来就一个字节都不写");

    // 换一张干净的真卡、同一份主库、同一趟计划，这回**不塞**：那一问落在盘上，
    // 盘上真的没有，于是字节照常落下去。
    let 另一处 = 现场::摆在(建个只差大小写的库());
    let 另一趟 = 另一处.排一趟("平台=GB", &Manifest::empty());
    let 真的 = 另一处.执行(&另一趟, &Manifest::empty(), &CancelToken::new());
    assert!(
        真的.failures.is_empty(),
        "不塞视图时那一问落在真盘上，盘上没有就该照常落下去：{:?}",
        真的.failures
    );
    assert_eq!(
        fs::read(另一处.卡.path().join("GB/tetris.zip")).expect("真的落在真盘上"),
        fs::read(另一处.库根.join("GB/tetris.zip")).expect("读得出"),
        "写那一侧照旧是真实文件系统，一个字节都没经过那道接缝",
    );
}

/// 落点闸那几条**在两种折叠语义下各跑一遍**。
///
/// 卡是 exFAT / FAT32、主力机是默认 APFS——**都不分大小写**；开发机是 ext4，**分**。
/// 这道闸的正确性在两边不是同一件事：不分大小写的目标上，卡里那份 `GB/Tetris.zip`
/// 与我们要写的 `GB/tetris.zip` **就是同一个文件**，挡不住就是把维护者的东西顶掉；
/// 分大小写的盘上它们是两个文件，挡不住只是在旁边多写一份。一台机器上造不出另一种
/// 挂载点，于是不敏感那半边挂了两轮都只是一句推理（挂单 `Q135`）——眼下它由
/// `Sources::target` 那道接缝喂进来（`sync::execute` 模块文档八）。
const 两种折叠语义: [Folding; 2] = [Folding::Sensitive, Folding::Insensitive];

#[test]
fn 计划算完之后才出现的落点占用_执行这一层也挡得住() {
    // 计划靠的是 `observe` 交出来的那份键的集合，而那份集合**可能是不全的**：
    // 列不开的目录底下一个键都拿不到，那一枝上的落点计划根本无从判断；计划算完到
    // 真的改名之间也隔着整趟同步的时间，卡还插在机器上。于是执行这一层还要兜一道。
    for 折叠 in 两种折叠语义 {
        let 说 = 折叠.label();
        let 现场 = 现场::摆在(建个只差大小写的库());
        let 这趟 = 现场.排一趟("平台=GB", &Manifest::empty());
        assert!(
            这趟
                .plan
                .steps
                .iter()
                .any(|step| step.act == Act::Add && step.path == "GB/tetris.zip"),
            "{说}：计划这一侧看不见它，本来就该排一条新增"
        );

        // 排完计划之后，维护者才把自己那份拷进卡里——只差大小写。
        let 维护者那份 = 现场.卡.path().join("GB/Tetris.zip");
        写(&维护者那份, "这是我自己拷进去的".as_bytes());
        let 原样 = fs::read(&维护者那份).expect("读得出");

        let outcome = 现场.执行_折(&这趟, &Manifest::empty(), 折叠, &CancelToken::new());
        assert_eq!(
            fs::read(&维护者那份).expect("还在"),
            原样,
            "{说}：维护者自己那份连一个字节都不许动",
        );
        assert!(
            outcome
                .failures
                .iter()
                .any(|failure| failure.path == "GB/tetris.zip" && failure.act == Act::Add),
            "{说}：挡下来要记成一条没做成，而不是整趟停住：{:?}",
            outcome.failures
        );
        assert!(
            !outcome
                .manifest
                .files
                .iter()
                .any(|file| file.path.eq_ignore_ascii_case("GB/tetris.zip")),
            "{说}：没写成的不许进清单"
        );
        // 上面那条「维护者那份一个字节没动」在**不分大小写**那一档上咬不动：闸真漏了，
        // 字节会落到真卡（ext4）上另一个 inode 的 `GB/tetris.zip` 里，维护者那份照样
        // 完好。所以还得断这一条——挡下来就是一个字节都没写。真卡本身不分大小写时
        // 这一条断不了也用不着，见 `旁边一个字节都没写`。
        现场.旁边一个字节都没写(
            &现场.卡.path().join("GB/tetris.zip"),
            &format!("（视图按「{说}」折的那一趟）"),
        );
    }
}

#[test]
fn 不注入时闸在真盘上照样挡得住() {
    // 上面那四条两档跑的都是**假视图**。真盘那一档不能只剩「盘上没有就放行」
    // （`塞得进一个假目标_不塞就是真盘` 的后半段）——**挡住**那条路也得有人在真实
    // 文件系统上钉着，不然接缝一接错，四条假视图测试照样全绿。
    let 现场 = 现场::摆在(建个只差大小写的库());
    let 这趟 = 现场.排一趟("平台=GB", &Manifest::empty());
    let 维护者那份 = 现场.卡.path().join("GB/Tetris.zip");
    写(&维护者那份, "这是我自己拷进去的".as_bytes());
    let 原样 = fs::read(&维护者那份).expect("读得出");

    let outcome = 现场.执行(&这趟, &Manifest::empty(), &CancelToken::new());
    assert!(
        outcome
            .failures
            .iter()
            .any(|failure| failure.path == "GB/tetris.zip" && failure.act == Act::Add),
        "真盘上也得挡下来：{:?}",
        outcome.failures
    );
    assert_eq!(
        fs::read(&维护者那份).expect("还在"),
        原样,
        "维护者自己那份连一个字节都不许动",
    );
    // 真卡不分大小写（默认 APFS）时，逐字那一问就挡得下来（按名字开得了维护者那份），
    // 折起来那一问坏了这条也照绿——它在真盘上挡不挡得住，要分大小写的盘（CI 的 ext4）
    // 才验得到。
    现场.旁边一个字节都没写(&现场.卡.path().join("GB/tetris.zip"), "");
}

/// `#[cfg(unix)]`：**列不开却写得进**的目录只有 Unix 的权限位造得出来——`0300` 是
/// 「进得去、写得进、就是列不开」。Windows 上那种目录摆不出来，那边这道闸的这一格
/// 由手捏的假视图钉着（`列不开的目录底下_假视图上闸照样挡得住`）。
#[cfg(unix)]
#[test]
fn 列不开的目录底下_落点被占照样挡得住() {
    // ADR-0021：**读不动是第三态**，既不是「有」也不是「没有」。这道闸折起来那一问
    // 从前把「列不开」折成了「这一层没有挡路的」，于是在大小写敏感的盘上，维护者那份
    // `GB/Tetris.zip` 旁边会多出一份工具写的 `GB/tetris.zip`——两份只差大小写。
    //
    // 这条走**真盘、不注入**：`0300` 的目录 `read_dir` 失败、按名字 `open` 照样成功，
    // 而相片照不下这一格（`testing::target` 模块文档），假视图替不了它。
    use std::os::unix::fs::PermissionsExt as _;

    let 现场 = 现场::摆在(建个只差大小写的库());
    let 这趟 = 现场.排一趟("平台=GB", &Manifest::empty());
    // 排完计划之后，维护者才把自己那份拷进卡里，随后那个目录变成列不开的。
    let 维护者那份 = 现场.卡.path().join("GB/Tetris.zip");
    写(&维护者那份, "这是我自己拷进去的".as_bytes());
    let 平台目录 = 现场.卡.path().join("GB");
    fs::set_permissions(&平台目录, fs::Permissions::from_mode(0o300)).expect("改得动权限");
    let 真的列不开 = fs::read_dir(&平台目录).is_err();

    let outcome = 现场.执行(&这趟, &Manifest::empty(), &CancelToken::new());

    // **权限先收回来**：底下那几条断言、以及临时目录自己的清理，都要列得开它。
    fs::set_permissions(&平台目录, fs::Permissions::from_mode(0o700)).expect("改得回权限");
    assert!(
        真的列不开,
        "这台机器上 0300 的目录照样列得开（跑测试的是 root？），这一格钉不住",
    );
    assert!(
        outcome
            .failures
            .iter()
            .any(|failure| failure.path == "GB/tetris.zip" && failure.act == Act::Add),
        "列不开就是答不出来，答不出来就不许写：{:?}",
        outcome.failures
    );
    // 真卡不分大小写（默认 APFS）时，逐字那一问就挡得下来（按名字开得了维护者那份），
    // 「列不开」又被折成「没有」这条也照绿——那一格由分大小写的盘（CI 的 ext4）与假视图
    // 那一条（`列不开的目录底下_假视图上闸照样挡得住`）钉着。
    现场.旁边一个字节都没写(&现场.卡.path().join("GB/tetris.zip"), "");
    assert_eq!(
        fs::read(&维护者那份).expect("还在"),
        "这是我自己拷进去的".as_bytes(),
        "维护者自己那份连一个字节都不许动",
    );
    assert!(
        !outcome
            .manifest
            .files
            .iter()
            .any(|file| file.path.eq_ignore_ascii_case("GB/tetris.zip")),
        "没写成的不许进清单",
    );
}

#[test]
fn 列不开的目录底下_假视图上闸照样挡得住() {
    // 同一格的另一头：上面那条要一个 `0300` 的真目录，只有 Unix 摆得出来。这一条走
    // 那道接缝，**哪台机器上都跑得了**。
    //
    // 相片照不下「列不开的目录**底下**还有东西」这一格（`testing::target` 模块文档），
    // 于是手捏一个 `MemFs`：先 `unlistable_dir` 再往底下 `file`——那正是真盘的语义
    // （`read_dir` 失败、按名字 `open` 照样成功）。
    let 现场 = 现场::摆在(建个只差大小写的库());
    let 这趟 = 现场.排一趟("平台=GB", &Manifest::empty());
    let mut 假卡 = MemFs::new();
    假卡.unlistable_dir(现场.卡.path().join("GB"));
    假卡.file(
        现场.卡.path().join("GB/Tetris.zip"),
        "这是我自己拷进去的".as_bytes().to_vec(),
    );

    let outcome = 现场.执行_折_视图(&这趟, &Manifest::empty(), &假卡, &CancelToken::new());
    let 挡下来的 = outcome
        .failures
        .iter()
        .find(|failure| failure.path == "GB/tetris.zip" && failure.act == Act::Add)
        .unwrap_or_else(|| {
            panic!(
                "列不开就是答不出来，答不出来就不许写：{:?}",
                outcome.failures
            )
        });
    assert!(
        挡下来的.why.contains("列不开"),
        "报告要说得出是被哪一格挡下来的：{}",
        挡下来的.why
    );
    assert!(
        !现场.卡.path().join("GB/tetris.zip").exists(),
        "挡下来就一个字节都不写",
    );
    assert!(!outcome.gave_up && !outcome.interrupted, "不该整趟停住");
}

#[test]
fn 只差大小写的是上一级目录_执行这一层照样挡得住() {
    // 折的是**整条键**，不是最后那一段：计划那一侧拿 `path::fold` 折 `gb/Tetris.zip`
    // 一整条，执行这一侧只折文件名的话，上一级目录换个大小写就从缝里漏过去了——
    // 而卡上那个 `gb` 与我们要建的 `GB` 在不敏感的卡上本来就是同一个目录。
    for 折叠 in 两种折叠语义 {
        let 说 = 折叠.label();
        let 现场 = 现场::摆在(建个只差大小写的库());
        let 这趟 = 现场.排一趟("平台=GB", &Manifest::empty());

        // 同样是排完计划之后才出现的：目录这一级也只差大小写。
        //
        // 卡上顺手再摆一个 `GB/`：大小写敏感的盘上它与 `gb/` 能并存，而 `GB` 排在 `gb`
        // 前面。一层只跟排在前面那个候选的话，`gb/` 底下挡路的那份就从缝里漏过去了。
        // **不分大小写那一档上这两层并成一层**——真卡上本来就装不下两个，于是挡下来
        // 的理由换了一个（同一个目录里那份逐字就撞上了），可结论得是同一个。
        写(
            &现场.卡.path().join("GB/别的.txt"),
            "维护者自己的东西".as_bytes(),
        );
        let 维护者那份 = 现场.卡.path().join("gb/Tetris.zip");
        写(&维护者那份, "这是我自己拷进去的".as_bytes());
        let 原样 = fs::read(&维护者那份).expect("读得出");

        let outcome = 现场.执行_折(&这趟, &Manifest::empty(), 折叠, &CancelToken::new());
        assert_eq!(
            fs::read(&维护者那份).expect("还在"),
            原样,
            "{说}：维护者自己那份连一个字节都不许动",
        );
        assert!(
            outcome
                .failures
                .iter()
                .any(|failure| failure.path == "GB/tetris.zip" && failure.act == Act::Add),
            "{说}：上一级目录只差大小写也要挡下来：{:?}",
            outcome.failures
        );
        assert!(
            outcome
                .failures
                .iter()
                .any(|failure| failure.why.contains("Tetris.zip")),
            "{说}：报告要说得出是哪个落点被占着：{:?}",
            outcome.failures
        );
    }
}

/// 一份主库，`GB` 底下除了那个只差大小写的，还有别的东西。
fn 建个只差大小写又不止一件的库() -> TempDir {
    let dir = temp_dir("run-case-lib-more");
    写(&dir.path().join("GB/tetris.zip"), &zip(2048));
    写(&dir.path().join("GB/口袋妖怪.zip"), &zip(4096));
    dir
}

#[test]
fn 落点被占只挡那一条_其余几步照常做完() {
    // 一条挡下来不许拖累整趟：与「单个文件写不进去不中断整趟」同一条纪律。
    for 折叠 in 两种折叠语义 {
        let 说 = 折叠.label();
        let 现场 = 现场::摆在(建个只差大小写又不止一件的库());
        let 这趟 = 现场.排一趟("平台=GB", &Manifest::empty());
        let 一共 = 这趟.plan.steps.len();
        assert!(
            一共 >= 2,
            "{说}：这一趟得有别的步可做：{:?}",
            这趟.plan.steps
        );

        写(
            &现场.卡.path().join("GB/Tetris.zip"),
            "这是我自己拷进去的".as_bytes(),
        );

        let outcome = 现场.执行_折(&这趟, &Manifest::empty(), 折叠, &CancelToken::new());
        assert!(
            !outcome.interrupted && !outcome.gave_up,
            "{说}：不该整趟停住"
        );
        assert_eq!(outcome.failures.len(), 1, "{说}：{:?}", outcome.failures);
        assert_eq!(
            outcome.manifest.files.len(),
            一共 - 1,
            "{说}：其余几步都该写成、都该进清单：{:?}",
            outcome.manifest.files
        );
        assert_eq!(
            fs::read(现场.卡.path().join("GB/口袋妖怪.zip")).expect("读得出"),
            fs::read(现场.库根.join("GB/口袋妖怪.zip")).expect("读得出"),
            "{说}：同一趟里别的那份照常落到卡上",
        );
    }
}

/// 一份主库，`GB` 底下摆着 12 份，够把「连着失败就停下来」那个计数顶过去。
fn 建个够多的库() -> TempDir {
    let dir = temp_dir("run-case-lib-many");
    for i in 1..=12 {
        写(&dir.path().join(format!("GB/g{i:02}.zip")), &zip(2048 + i));
    }
    dir
}

#[test]
fn 落点被占再多也不算系统性故障_不触发连着失败就停下来() {
    // `GIVE_UP_AFTER` 是 10，防的是「卡满了、卡被拔了、目标变成只读」那一类——
    // 接着往下试没有意义的那种。落点被占不是那一类：它是**这一个落点**的确定性条件。
    // 而计划里新增是**连在一起**的（`steps` 按 `Act` 排过），一算进那个计数，
    // 维护者往卡里拷十来个只差大小写的文件就能让其余几百步一步都不做。
    for 折叠 in 两种折叠语义 {
        let 说 = 折叠.label();
        let 现场 = 现场::摆在(建个够多的库());
        let 这趟 = 现场.排一趟("平台=GB", &Manifest::empty());

        // 排完计划之后，前 11 份的落点全被占上——比那个计数多一个。
        for i in 1..=11 {
            写(
                &现场.卡.path().join(format!("GB/G{i:02}.zip")),
                "这是我自己拷进去的".as_bytes(),
            );
        }

        let outcome = 现场.执行_折(&这趟, &Manifest::empty(), 折叠, &CancelToken::new());
        assert!(!outcome.gave_up, "{说}：落点被占不该被当成系统性故障");
        assert!(!outcome.interrupted, "{说}：不该整趟停住");
        assert_eq!(outcome.failures.len(), 11, "{说}：{:?}", outcome.failures);
        assert!(
            outcome
                .failures
                .iter()
                .all(|failure| failure.act == Act::Add),
            "{说}：{:?}",
            outcome.failures
        );
        assert_eq!(
            fs::read(现场.卡.path().join("GB/g12.zip")).expect("排在最后那一份照样落得下"),
            fs::read(现场.库根.join("GB/g12.zip")).expect("读得出"),
            "{说}：排在最后那一份照样落得下",
        );
    }
}

#[test]
fn 该建目录的位置上躺着个文件_闸挡下来而且放弃机制真的触发() {
    // 卡上有个**文件**叫 `GB`（前端写的一份索引、维护者手滑拷进去的东西都可能），
    // 而这一趟要往 `GB/` 底下写十几份。两层判定从前都答不出这一格：逐字那一问只问
    // 落点自己在不在，折起来那一问走到「不是目录」就当这一枝空的，于是一路走到
    // `create_dir_all` 才炸——而它报的是 `AlreadyExists`，正好撞上「落点被占不计数」
    // 那条豁免（那条认的是**错误种类**）。结果：整个平台目录下每一条新增都以同一句话
    // 失败，连着失败的计数一次都不涨，放弃机制永不触发。
    for 折叠 in 两种折叠语义 {
        let 说 = 折叠.label();
        let 现场 = 现场::摆在(建个够多的库());
        let 这趟 = 现场.排一趟("平台=GB", &Manifest::empty());
        assert!(
            这趟
                .plan
                .steps
                .iter()
                .filter(|step| step.act == Act::Add)
                .count()
                > 10,
            "{说}：这一趟得有超过阈值那么多条新增才验得了放弃机制"
        );
        // 排完计划之后才出现的：那个位置上躺着的是个文件。
        let 挡路的 = 现场.卡.path().join("GB");
        写(&挡路的, "我是个文件，不是目录".as_bytes());

        let outcome = 现场.执行_折(&这趟, &Manifest::empty(), 折叠, &CancelToken::new());
        assert!(
            outcome.gave_up,
            "{说}：同一句话印上几百遍正是放弃机制要防的那一幕：{:?}",
            outcome.failures
        );
        assert_eq!(
            outcome.failures.len(),
            10,
            "{说}：到阈值就该收手，不是把整份计划跑完：{:?}",
            outcome.failures
        );
        assert!(
            outcome
                .failures
                .iter()
                .all(|failure| failure.act == Act::Add),
            "{说}：{:?}",
            outcome.failures
        );
        assert!(
            outcome
                .failures
                .iter()
                .all(|failure| failure.why.contains("不是目录")),
            "{说}：报告要说得出是被这一格挡下来的：{:?}",
            outcome.failures
        );
        assert_eq!(
            fs::read(&挡路的).expect("还在"),
            "我是个文件，不是目录".as_bytes(),
            "{说}：清单之外的东西一个字节都不许动（ADR-0015）",
        );
    }
}

/// 一份主库，`GB` 底下只有一份——够验「大小写敏感的盘上 `gb` 这个文件挡不住 `GB/`」。
#[test]
fn 只差大小写的那个文件不是目录_分大小写的盘上照样建得出目录() {
    // 卡上躺着一个叫 `gb` 的**文件**，我们要写 `GB/tetris.zip`。分大小写的盘上
    // `GB/` 与 `gb` 本来就并存得了，`create_dir_all` 一次就成——闸不许在这儿误报。
    // 判据不猜：拿我们自己那个写法去问一句文件系统（与 `real_dir` 那一手同一条口径）。
    //
    // 这一条**整条都在验分大小写的盘**：不分大小写的盘（默认 APFS、Windows）上 `gb` 这个
    // 文件与 `GB/` 这个目录根本并存不了，`create_dir_all` 撞上的是盘不是闸，拆不出一半
    // 与大小写无关的来照跑。摆不出来就如实整条跳过——分大小写的盘（CI 的 ext4）上照跑。
    let 现场 = 现场::摆在(建个只差大小写的库());
    if 现场.卡不分大小写 {
        eprintln!("跳过：临时目录所在的盘不分大小写，摆不出并存的 `gb` 文件与 `GB/` 目录");
        return;
    }
    let 这趟 = 现场.排一趟("平台=GB", &Manifest::empty());
    写(&现场.卡.path().join("gb"), "我是个文件".as_bytes());

    let outcome = 现场.执行_折(
        &这趟,
        &Manifest::empty(),
        Folding::Sensitive,
        &CancelToken::new(),
    );
    assert!(
        outcome.failures.is_empty(),
        "分大小写的盘上这两个并存得了，不许误报：{:?}",
        outcome.failures
    );
    assert!(
        现场.卡.path().join("GB/tetris.zip").is_file(),
        "字节该照常落下去",
    );
}

/// `#[cfg(unix)]`：Windows 上这种目录软链要另一套权限，造不出来。
#[cfg(unix)]
#[test]
fn 平台目录是个符号链接_底下那份照样挡得住() {
    // `LibraryFs::read_dir` **不跟随符号链接**（`fs::real`），于是卡上一个指向别处的
    // 平台目录交出来的 `kind` 是 `Symlink` 而不是 `Dir`。折起来那一问要是按 `kind`
    // 挑「是不是目录」再往下走，这一整枝就漏了——而维护者那份正躺在它底下。
    // 挡不挡得住由**下一层列不列得开**说了算，不由这一层的 `kind` 说了算。
    let 现场 = 现场::摆在(建个只差大小写的库());
    let 这趟 = 现场.排一趟("平台=GB", &Manifest::empty());
    let 真身 = 现场.卡.path().join("别处");
    写(&真身.join("Tetris.zip"), "这是我自己拷进去的".as_bytes());
    std::os::unix::fs::symlink(&真身, 现场.卡.path().join("GB")).expect("建得出符号链接");

    let outcome = 现场.执行(&这趟, &Manifest::empty(), &CancelToken::new());
    assert!(
        outcome
            .failures
            .iter()
            .any(|failure| failure.path == "GB/tetris.zip" && failure.act == Act::Add),
        "软链底下那份也得挡得住：{:?}",
        outcome.failures
    );
    // 真卡不分大小写（默认 APFS）时，逐字那一问就挡得下来（顺着软链按名字开得了维护者
    // 那份），折起来那一问又按 `kind` 把软链那一枝漏掉这条也照绿——那一格要分大小写的盘
    // （CI 的 ext4）才验得到。
    现场.旁边一个字节都没写(&真身.join("tetris.zip"), "");
    assert_eq!(
        fs::read(真身.join("Tetris.zip")).expect("还在"),
        "这是我自己拷进去的".as_bytes(),
        "维护者自己那份连一个字节都不许动",
    );
}

/// 一层**记数**的目标视图：每个目录被 `read_dir` 了几遍。
///
/// 「同一个目录被列了两遍」这件事只有在这道接缝上看得见——数在实现里数就成了对着内部
/// 结构断言，而接缝上数的正是这道闸真的问了盘几句话。
struct 数着列 {
    底下: MemFs,
    次数: Mutex<BTreeMap<PathBuf, usize>>,
}

impl 数着列 {
    fn 摆上(底下: MemFs) -> Self {
        Self {
            底下,
            次数: Mutex::new(BTreeMap::new()),
        }
    }

    fn 列了几遍(&self, dir: &Path) -> usize {
        self.次数
            .lock()
            .expect("锁得住")
            .get(dir)
            .copied()
            .unwrap_or(0)
    }
}

impl LibraryFs for 数着列 {
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        self.底下.canonicalize(path)
    }

    fn read_dir(&self, dir: &Path) -> std::io::Result<Vec<DirEntry>> {
        *self
            .次数
            .lock()
            .expect("锁得住")
            .entry(dir.to_path_buf())
            .or_default() += 1;
        self.底下.read_dir(dir)
    }

    fn read_head(&self, file: &Path, limit: usize) -> std::io::Result<Vec<u8>> {
        self.底下.read_head(file, limit)
    }

    fn read_tail(&self, file: &Path, limit: usize) -> std::io::Result<Vec<u8>> {
        self.底下.read_tail(file, limit)
    }

    fn open(&self, file: &Path) -> std::io::Result<Box<dyn ReadSeek + '_>> {
        self.底下.open(file)
    }
}

#[test]
fn 一次落点判定之内_同一个目录只列一遍() {
    // 落点判定问目标两遍：逐字那一问的退路（`real_path` 逐段列目录）与折起来那一问，
    // 走的是**同一批目录**。各列各的等于把同一个目录整层列两遍——一个平台目录下
    // 几千份文件，那就是几千次多余的整层 listing。
    //
    // 记性建在**这一次判定的栈上**，不是整趟：整趟的那份跑到一半就过期，会把
    // 「计划算完到真的改名之间」那道缝重新打开（挂单 `Q134`）。
    let 现场 = 现场::摆在(建个只差大小写的库());
    // 卡上先摆一个逐字同名的平台目录：两问都得走进它，才数得出「列了两遍」。
    写(
        &现场.卡.path().join("GB/别的.txt"),
        "维护者自己的东西".as_bytes(),
    );
    let 这趟 = 现场.排一趟("平台=GB", &Manifest::empty());
    assert!(
        这趟
            .plan
            .steps
            .iter()
            .any(|step| step.act == Act::Add && step.path == "GB/tetris.zip"),
        "这一趟得有那一条新增：{:?}",
        这趟.plan.steps
    );

    let 视图 = 数着列::摆上(target::snapshot(现场.卡.path(), Folding::Sensitive));
    let outcome = 现场.执行_折_视图(&这趟, &Manifest::empty(), &视图, &CancelToken::new());
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
    assert_eq!(
        视图.列了几遍(&现场.卡.path().join("GB")),
        1,
        "一次落点判定之内，那个平台目录只该被列一遍",
    );
}

#[test]
fn 不分大小写的卡上_清单记的是盘上那个目录真名() {
    // 模块文档八那三句里的**第三句**（`settled`：目录段真名）。上面四条落点占用测试
    // 的目录段在两档上都是逐字命中，`real_dir` 那条「只差大小写就再问一次文件系统」
    // 的支一次都没走到——而模块文档**六**点名的那种损失正是它防的：清单记成我们要的
    // 那个写法，下一趟 `observe` 交出来的却是 `read_dir` 给的真名，工具从第二趟起就
    // 认不出自己放的那一份，报成「没了」、同时被数进「清单之外」。
    //
    // 卡上先有一个 `gb/`（前端或维护者建的，里面躺着存档），主库那边的键写作 `GB/`。
    // 不分大小写的目标上两者**就是同一个目录**：字节其实落在 `gb/` 里，清单就得记 `gb/`。
    let 现场 = 现场::摆好();
    写(&现场.卡.path().join("gb/存档.sav"), &[9u8; 64]);
    let 这趟 = 现场.排一趟("平台=GB", &Manifest::empty());

    let outcome = 现场.执行_折(
        &这趟,
        &Manifest::empty(),
        Folding::Insensitive,
        &CancelToken::new(),
    );
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
    assert!(
        outcome
            .manifest
            .files
            .iter()
            .any(|file| file.path == "gb/口袋妖怪.zip"),
        "清单记的得是盘上那个目录真名（`gb/`），不是我们要的那个写法：{:?}",
        outcome
            .manifest
            .files
            .iter()
            .map(|file| &file.path)
            .collect::<Vec<_>>()
    );
    assert!(
        现场.卡.path().join("gb/口袋妖怪.zip").is_file(),
        "字节真的落在 gb/ 里",
    );
}

#[test]
fn 卡上那个目录只差大小写_第二趟照样认得出自己放的那一份() {
    // 触发路 A：卡上已经有一个 `gb/`（前端或维护者建的，里面还躺着存档），而主库
    // 那边的键写作 `GB/`。目标大小写不敏感（exFAT / FAT32 / Windows / 默认 APFS）时
    // 两者**就是同一个目录**——第一趟的文件其实落在 `gb/` 里，清单却记成 `GB/`，
    // 于是从第二趟起它每一趟都被报成「没了」，同时又被数进「清单之外」。
    let 现场 = 现场::摆好();
    写(&现场.卡.path().join("gb/存档.sav"), &[9u8; 64]);

    let 第一趟 = 现场.排一趟("平台=GB", &Manifest::empty());
    assert!(第一趟.plan.adds.files > 0, "头一趟总得放点什么上去");
    let 一 = 现场.执行(&第一趟, &Manifest::empty(), &CancelToken::new());
    assert!(一.failures.is_empty(), "{:?}", 一.failures);

    // **清单记的必须是盘上真实的那条路径**：目标怎么拼那个目录名，由目标说了算。
    for file in &一.manifest.files {
        let 名 = file.path.replace('/', std::path::MAIN_SEPARATOR_STR);
        assert!(
            现场.卡.path().join(&名).is_file(),
            "清单记着 {}，盘上却没有这条路径",
            file.path
        );
    }

    // 第二趟：一步都不用做，一句意外都不该有，清单之外的只有维护者那份存档。
    let 第二趟 = 现场.排一趟("平台=GB", &一.manifest);
    assert_eq!(
        第二趟.plan.touched(),
        0,
        "第二趟不该有任何一步：\n{}",
        第二趟.plan.render_text()
    );
    assert!(
        第二趟.plan.surprises.is_empty(),
        "自己放的那一份不该被报成意外：{:?}",
        第二趟.plan.surprises
    );
    assert_eq!(
        第二趟.plan.strangers, 1,
        "清单之外只有维护者那份存档，不该把自己放的也数进去"
    );

    // 维护者那份存档一个字节都没动。
    assert_eq!(
        fs::read(现场.卡.path().join("gb/存档.sav")).expect("还在"),
        vec![9u8; 64]
    );
}

// ───────────────────────── 多碟变体同步到卡上时多生成一份 `.m3u`（票 `verdict-store-and-sync/11`）
//
// 前端里换碟不用手动：卡上那套多碟游戏旁边多一份播放列表，按碟序列出每张碟的主文件。
// 它是**生成物**——只在同步那一侧生成到卡上、清单照管；导出到主库那一侧不生成（ADR-0004）。

/// 一份主库：`ps/某游戏/` 里一套两碟的 PS1 游戏（每张碟一份 `.cue` 加一份 `.bin`），外加一个 FC 游戏。
fn 建个多碟的库() -> TempDir {
    let dir = temp_dir("run-discs-lib");
    for 碟 in 1..=2u8 {
        写(
            &dir.path().join(format!("ps/某游戏/游戏 (Disc {碟}).cue")),
            format!(
                "FILE \"游戏 (Disc {碟}).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n"
            )
            .as_bytes(),
        );
        写(
            &dir.path().join(format!("ps/某游戏/游戏 (Disc {碟}).bin")),
            &[碟; 4096],
        );
    }
    写(&dir.path().join("FC/魂斗罗.zip"), &zip(2048));
    dir
}

/// 两碟那套游戏的播放列表落在卡上的哪儿：主文件那个目录里，名字是剥掉碟片标记之后的那一截（设计稿
/// `Final Fantasy VII (Japan).m3u`）。
const 播放列表: &str = "ps/某游戏/游戏.m3u";

impl 现场 {
    /// 建一个叫「掌机」的子库，目标是这张卡，前端格式是 `格式`，带一条规则。交回那条规则的号。
    fn 建子库(&mut self, 格式: &str, 规则: &str) -> i64 {
        self.catalog
            .put_sublibrary(&Sublibrary::at("掌机", self.卡.path(), 格式, None))
            .expect("子库写得进");
        self.catalog
            .add_rule("掌机", &Rule::parse(规则).expect("规则读得懂"), None)
            .expect("规则写得进")
    }

    /// 「掌机」改用名册里叫 `档案` 的那份能力档案。
    fn 换档案(&mut self, 档案: &str) {
        let mut 子库 = self
            .catalog
            .sublibrary("掌机")
            .expect("读得出子库")
            .expect("子库在");
        子库.capability = Some(档案.to_string());
        self.catalog.put_sublibrary(&子库).expect("子库写得进");
    }

    /// **照真的那条线**同步一趟：`sync::prepare` 排计划、`execute::run` 落到卡上、清单记回中立库
    /// ——命令行与界面走的就是这几步。下一趟排计划读的就是这一趟记回去的那份清单。
    fn 照真线同步一趟(&mut self) -> sync::Outcome {
        let prepared = sync::prepare(
            &self.catalog,
            self.工作区.path(),
            "掌机",
            &sync::Request::default(),
            &Handle::new(),
        )
        .expect("排得出计划");
        let roots = Roots::single("库", &self.库根);
        let sources = Sources {
            library: &RealFs,
            library_roots: Some(&roots),
            target: &RealFs,
            target_root: &prepared.root,
            from_pool: &prepared.from_pool,
            generated: &prepared.generated,
            link_probe_dir: Some(&prepared.scratch),
            convert_cache: None,
        };
        let outcome = sync::execute::run(
            &prepared.plan,
            &prepared.desired,
            &prepared.actual,
            &prepared.manifest,
            &sources,
            &Handle::new(),
        )
        .expect("传得动");
        assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
        self.catalog
            .put_manifest("掌机", &outcome.manifest)
            .expect("清单记得下");
        outcome
    }
}

#[test]
fn 两碟的变体同步到卡上_旁边多一份按碟序列出两张碟的播放列表_而且进了清单() {
    let mut 现场 = 现场::摆在(建个多碟的库());
    现场.建子库("ES-Gamelist", "平台=PS1");
    let outcome = 现场.照真线同步一趟();

    let 卡上 = 现场.卡.path().join(播放列表);
    assert_eq!(
        fs::read_to_string(&卡上).unwrap_or_else(|_| panic!("卡上该有 {播放列表}")),
        "游戏 (Disc 1).cue\n游戏 (Disc 2).cue\n",
        "按碟序列出每张碟的主文件，路径相对播放列表自己所在的目录",
    );
    // 两张碟本身照旧原样落在卡上——播放列表指着的每一行都真有那个文件。
    for 碟 in ["游戏 (Disc 1).cue", "游戏 (Disc 2).cue"] {
        assert!(
            现场.卡.path().join("ps/某游戏").join(碟).is_file(),
            "{碟} 该在卡上"
        );
    }

    // **清单照管**：它进了清单，类别是播放列表，记的戳是写完从卡上读回来的那一个；记回中立库再读出来还是它。
    let 记着 = outcome
        .manifest
        .files
        .iter()
        .find(|file| file.path == 播放列表)
        .unwrap_or_else(|| panic!("{播放列表} 该进清单：{:?}", outcome.manifest.files));
    assert_eq!(记着.kind.label(), "播放列表");
    assert_eq!(记着.stamp.bytes, fs::metadata(&卡上).expect("读得到").len());
    assert_eq!(
        记着.variant, "库/ps/某游戏/游戏 (Disc 1).cue",
        "挂在那个多碟变体名下"
    );
    let 读回来 = 现场.catalog.manifest("掌机").expect("读得出清单");
    assert!(
        读回来
            .files
            .iter()
            .any(|file| file.path == 播放列表 && file.kind.label() == "播放列表"),
        "记回中立库再读出来，那一条还在：{:?}",
        读回来.files
    );
}

#[test]
fn 那套多碟游戏移出子库_播放列表跟着清单一起删掉() {
    let mut 现场 = 现场::摆在(建个多碟的库());
    let 规则号 = 现场.建子库("ES-Gamelist", "平台=PS1");
    现场.照真线同步一趟();
    assert!(
        现场.卡.path().join(播放列表).is_file(),
        "头一趟该把 {播放列表} 放上去"
    );

    // 规则改成只要 FC：那套 PS1 游戏移出子库。
    assert!(
        现场
            .catalog
            .remove_rule("掌机", 规则号)
            .expect("删得掉规则")
    );
    现场
        .catalog
        .add_rule("掌机", &Rule::parse("平台=FC").expect("规则读得懂"), None)
        .expect("规则写得进");
    let outcome = 现场.照真线同步一趟();

    assert!(
        !现场.卡.path().join(播放列表).exists(),
        "移出子库之后播放列表跟着删掉"
    );
    assert!(
        !outcome
            .manifest
            .files
            .iter()
            .any(|file| file.path == 播放列表),
        "删掉的播放列表不该还留在清单里：{:?}",
        outcome.manifest.files
    );
    assert!(
        !现场.卡.path().join("ps/某游戏/游戏 (Disc 2).cue").exists(),
        "碟本身也跟着走了"
    );
    assert!(现场.卡.path().join("FC/魂斗罗.zip").is_file());
}

// ───────────────────────── 多碟那一条前端条目改指播放列表（票 `verdict-store-and-sync/18`）
//
// 票 11 让卡上多了一份 `.m3u`，这几条钉的是**前端条目拿它启动**——前端里换碟不用手动（用户故事 27）：
// ES-DE 的 `<path>` 指它、封面照它的名字铺、各张碟各写一条藏起来的条目；Pegasus 的 `files:` 指它。

/// 两碟那套游戏那个变体的键：主文件是碟 1。
const 两碟变体: &str = "库/ps/某游戏/游戏 (Disc 1).cue";

/// 卡上一份前端元数据里的条目，照那个格式的适配器自己读回来。
fn 卡上的条目(卡: &Path, 格式: &str, 元数据: &str) -> Vec<romcat_core::adapter::Game> {
    let bytes = fs::read(卡.join(元数据)).unwrap_or_else(|_| panic!("卡上该有 {元数据}"));
    adapter::find(格式)
        .unwrap_or_else(|| panic!("带着 {格式} 适配器"))
        .read(&bytes)
        .expect("读得动")
        .doc
        .entries
        .iter()
        .filter_map(|entry| entry.game().cloned())
        .collect()
}

/// 这一条在 ES-DE 里是不是藏起来的（`<hidden>true</hidden>`）。
fn 藏着(game: &romcat_core::adapter::Game) -> bool {
    game.unknown
        .get("hidden")
        .is_some_and(|values| values.iter().any(|value| value.trim() == "true"))
}

#[test]
fn 两碟的变体同步到_es_de_的卡上_条目指播放列表_封面照播放列表的名字铺_各张碟藏起来() {
    let mut 现场 = 现场::摆在(建个多碟的库());
    现场.收一份媒体(两碟变体, MediaKind::Cover, b"cover-of-the-two-disc-game");
    现场.建子库("ES-Gamelist", "平台=PS1");
    现场.照真线同步一趟();

    let 条目 = 卡上的条目(现场.卡.path(), "ES-Gamelist", "gamelists/ps/gamelist.xml");
    // **条目指播放列表**：`<path>` 相对平台目录，带着元数据、不藏——前端里点它启动，模拟器才换得了碟。
    let 指播放列表: Vec<_> = 条目
        .iter()
        .filter(|game| game.files == ["某游戏/游戏.m3u"])
        .collect();
    assert_eq!(指播放列表.len(), 1, "该有一条指播放列表：{条目:#?}");
    assert!(!藏着(指播放列表[0]), "{:#?}", 指播放列表[0]);
    assert!(!指播放列表[0].title.is_empty(), "{:#?}", 指播放列表[0]);
    // **各张碟各一条、都藏起来**：ES-DE 自己走目录认游戏，不写的话两张碟会各成一条没有元数据的条目。
    for 碟 in ["某游戏/游戏 (Disc 1).cue", "某游戏/游戏 (Disc 2).cue"] {
        let 这张: Vec<_> = 条目.iter().filter(|game| game.files == [碟]).collect();
        assert_eq!(这张.len(), 1, "{碟} 该有一条：{条目:#?}");
        assert!(藏着(这张[0]), "{碟} 该藏起来：{:#?}", 这张[0]);
    }
    assert_eq!(条目.len(), 3, "一条指播放列表、两张碟各一条：{条目:#?}");

    // **封面照播放列表的名字铺**：ES-DE 照 `<path>` 那份文件的名字找媒体（`游戏.m3u` → `游戏.png`）。
    assert!(
        现场
            .卡
            .path()
            .join("downloaded_media/ps/covers/某游戏/游戏.png")
            .is_file(),
        "封面该照播放列表的名字铺：{:?}",
        盘上有什么(现场.卡.path()).keys().collect::<Vec<_>>()
    );
    assert!(
        !现场
            .卡
            .path()
            .join("downloaded_media/ps/covers/某游戏/游戏 (Disc 1).png")
            .exists(),
        "照头一张碟的名字铺的那一份 ES-DE 找不着它的主人"
    );
}

#[test]
fn 两碟的变体同步到_pegasus_的卡上_条目的_files_指播放列表_碟本身不列() {
    // Pegasus 只列元数据里写着的条目，`files:` 指什么就拿什么交给启动命令（`{file.path}`）；只有一个文件时不弹挑选框。
    let mut 现场 = 现场::摆在(建个多碟的库());
    现场.建子库("Pegasus", "平台=PS1");
    现场.照真线同步一趟();

    assert_eq!(
        fs::read_to_string(现场.卡.path().join(播放列表))
            .unwrap_or_else(|_| panic!("Pegasus 的卡上也该有 {播放列表}")),
        "游戏 (Disc 1).cue\n游戏 (Disc 2).cue\n",
    );
    let 条目 = 卡上的条目(现场.卡.path(), "Pegasus", "ps.metadata.pegasus.txt");
    assert_eq!(条目.len(), 1, "{条目:#?}");
    assert_eq!(
        条目[0].files,
        [播放列表],
        "条目启动播放列表，两张碟本身不列进 `files:`：{条目:#?}"
    );
}

#[test]
fn 单碟变体的条目一个字不变_两家都是() {
    // 同一份主库里那个单碟的 FC 游戏：条目照旧指它的主文件，没有藏起来的条目、没有播放列表——一个字都不因为
    // 隔壁那套多碟游戏改指播放列表而变（字面量就是票 18 之前两家写出来的样子）。
    for (格式, 元数据, 原样) in [
        (
            "ES-Gamelist",
            "gamelists/FC/gamelist.xml",
            "<?xml version=\"1.0\"?>\n\
             <gameList>\n\
             \x20   <game>\n\
             \x20       <path>./魂斗罗.zip</path>\n\
             \x20       <name>魂斗罗.zip</name>\n\
             \x20       <x-romcat-variant>库/FC/魂斗罗.zip</x-romcat-variant>\n\
             \x20   </game>\n\
             </gameList>\n",
        ),
        (
            "Pegasus",
            "FC.metadata.pegasus.txt",
            "collection: FC\n\
             \n\
             game: 魂斗罗.zip\n\
             files: FC/魂斗罗.zip\n\
             x-romcat-variant: 库/FC/魂斗罗.zip\n",
        ),
    ] {
        let mut 现场 = 现场::摆在(建个多碟的库());
        现场.建子库(格式, "平台=FC 或 平台=PS1");
        现场.照真线同步一趟();
        assert_eq!(
            fs::read_to_string(现场.卡.path().join(元数据))
                .unwrap_or_else(|_| panic!("卡上该有 {元数据}")),
            原样,
            "{格式}"
        );
    }
}

#[test]
fn 多碟那套移出子库_条目_播放列表_媒体一起清干净() {
    // 条目改指播放列表、封面照播放列表的名字铺之后，移出子库那一趟照样全归清单管：元数据里那一条没了（PS1 一个条目
    // 都不剩，那份元数据文件整个删掉）、播放列表与封面跟着删、碟也走了。
    for (格式, 元数据) in [
        ("ES-Gamelist", "gamelists/ps/gamelist.xml"),
        ("Pegasus", "ps.metadata.pegasus.txt"),
    ] {
        let mut 现场 = 现场::摆在(建个多碟的库());
        let hash = 现场.收一份媒体(两碟变体, MediaKind::Cover, b"cover-of-the-two-disc-game");
        let 封面 = match 格式 {
            "ES-Gamelist" => "downloaded_media/ps/covers/某游戏/游戏.png".to_string(),
            _ => format!("media/{}/{hash}.png", &hash[..2]),
        };
        let 规则号 = 现场.建子库(格式, "平台=PS1");
        现场.照真线同步一趟();
        for 该有 in [元数据, 播放列表, 封面.as_str()] {
            assert!(
                现场.卡.path().join(该有).is_file(),
                "{格式}：头一趟该把 {该有} 放上去：{:?}",
                盘上有什么(现场.卡.path()).keys().collect::<Vec<_>>()
            );
        }

        assert!(
            现场
                .catalog
                .remove_rule("掌机", 规则号)
                .expect("删得掉规则")
        );
        现场
            .catalog
            .add_rule("掌机", &Rule::parse("平台=FC").expect("规则读得懂"), None)
            .expect("规则写得进");
        let outcome = 现场.照真线同步一趟();
        for 该走 in [
            元数据,
            播放列表,
            封面.as_str(),
            "ps/某游戏/游戏 (Disc 1).cue",
            "ps/某游戏/游戏 (Disc 2).cue",
        ] {
            assert!(
                !现场.卡.path().join(该走).exists(),
                "{格式}：移出子库之后 {该走} 该删掉"
            );
            assert!(
                !outcome.manifest.files.iter().any(|file| file.path == 该走),
                "{格式}：{该走} 不该还留在清单里"
            );
        }
    }
}

/// 卡上所有 `.m3u` 的相对路径。
fn 卡上的播放列表(卡: &Path) -> Vec<String> {
    盘上有什么(卡)
        .into_keys()
        .filter(|path| path.ends_with(".m3u"))
        .map(|path| path.replace(std::path::MAIN_SEPARATOR, "/"))
        .collect()
}

#[test]
fn 两家前端都用得上播放列表_同一套多碟游戏两张卡上各一份_都进了清单() {
    // 票 11 那时 Pegasus 答用不上（条目照旧指头一张碟，没人指着的 `.m3u` 它见不着）；票 18 让条目改指播放列表之后，
    // 两家都用得上：同一份主库、同一条规则，ES-DE 的卡上与 Pegasus 的卡上各一份，清单照管。
    for 格式 in ["ES-Gamelist", "Pegasus"] {
        let mut 现场 = 现场::摆在(建个多碟的库());
        现场.建子库(格式, "平台=PS1");
        let outcome = 现场.照真线同步一趟();
        assert_eq!(卡上的播放列表(现场.卡.path()), [播放列表], "{格式}");
        assert!(
            outcome
                .manifest
                .files
                .iter()
                .any(|file| file.path == 播放列表 && file.kind.label() == "播放列表"),
            "{格式}：{:?}",
            outcome.manifest.files
        );
        // 两张碟照旧都搬上去了。
        assert!(
            现场.卡.path().join("ps/某游戏/游戏 (Disc 2).cue").is_file(),
            "{格式}"
        );
    }
}

#[test]
fn 导出到主库那一侧不生成播放列表_主库里只多出元数据文件() {
    // ADR-0004：导出铺在主库上，只许多出元数据文件与媒体目录。同一套多碟游戏同步到 ES-DE 的卡上会多一份
    // `.m3u`（上面那几条），导出到主库——连 ES-DE 那个格式在内——一份都不生成。
    for adapter in adapter::all() {
        let mut 现场 = 现场::摆在(建个多碟的库());
        let 之前 = 盘上有什么(&现场.库根);
        let report = romcat_core::adapter::transfer::export(
            &mut 现场.catalog,
            adapter.as_ref(),
            &Priorities::builtin(),
            &romcat_core::adapter::transfer::ExportOptions {
                out: 现场.库根.clone(),
                dry_run: false,
                force: false,
                media: None,
            },
        )
        .expect("导得出来");
        assert!(
            report.entries > 0,
            "{} 那一趟总得导出点什么",
            adapter.name()
        );

        let 之后 = 盘上有什么(&现场.库根);
        assert!(
            卡上的播放列表(&现场.库根).is_empty(),
            "{} 导出到主库，主库里多出了播放列表",
            adapter.name()
        );
        // 原来就在的每一份一个字节都没动（大小与修改时间都一样），多出来的只有这个格式的元数据文件。
        for (path, 原样) in &之前 {
            assert_eq!(之后.get(path), Some(原样), "{path} 被动过");
        }
        let 多出来的: Vec<&String> = 之后
            .keys()
            .filter(|path| !之前.contains_key(*path))
            .collect();
        assert!(
            多出来的
                .iter()
                .all(|path| path.ends_with(adapter.file_name())),
            "{} 导出到主库只该多出元数据文件：{多出来的:?}",
            adapter.name()
        );
        // **条目也不改**（票 `verdict-store-and-sync/18`）：主库里没有播放列表，那套多碟游戏的条目照旧指碟 1，
        // 也没有藏起来的条目——同步那一侧改指播放列表的事一个字都没漏到导出这边来。
        let (元数据, 碟1) = match adapter.name() {
            "ES-Gamelist" => ("gamelists/ps/gamelist.xml", "某游戏/游戏 (Disc 1).cue"),
            _ => ("ps.metadata.pegasus.txt", "ps/某游戏/游戏 (Disc 1).cue"),
        };
        let 条目 = 卡上的条目(&现场.库根, adapter.name(), 元数据);
        let 那一条: Vec<_> = 条目
            .iter()
            .filter(|game| {
                game.extra
                    .get("romcat-variant")
                    .is_some_and(|keys| keys.iter().any(|key| key == 两碟变体))
            })
            .collect();
        assert_eq!(那一条.len(), 1, "{}：{条目:#?}", adapter.name());
        assert_eq!(那一条[0].files, [碟1], "{}", adapter.name());
        assert!(
            !条目.iter().any(藏着),
            "{} 导出到主库不该写藏起来的条目：{条目:#?}",
            adapter.name()
        );
    }
}

#[test]
fn 能力档案说这个平台的模拟器不吃_m3u_就不生成_吃的照生成() {
    // 「这个平台的模拟器吃不吃 `.m3u`」由能力档案答，与判主文件吃不吃得下是同一处（ADR-0017）。
    // 内置的「独立模拟器」那一张矩阵里 PS1 吃 `m3u`（DuckStation 的扩展名分派里有它）。
    let 档案 = "独立模拟器-exfat";
    let 那一行 =
        r#""吃" = ["cue", "bin", "img", "iso", "ecm", "chd", "mds", "pbp", "ccd", "m3u", "sub"]"#;
    assert!(
        romcat_core::capability::Roster::builtin_text().contains(那一行),
        "内置名册里 PS1 那一行变了，这条测试得跟着改"
    );

    let mut 吃 = 现场::摆在(建个多碟的库());
    吃.建子库("ES-Gamelist", "平台=PS1");
    吃.换档案(档案);
    吃.照真线同步一趟();
    assert_eq!(卡上的播放列表(吃.卡.path()), [播放列表]);

    // 同一份档案，只把 PS1 那一行的 `m3u` 拿掉（放进工作目录的 `capability.toml` 就生效）。
    let mut 不吃 = 现场::摆在(建个多碟的库());
    写(
        &不吃.工作区.path().join("capability.toml"),
        romcat_core::capability::Roster::builtin_text()
            .replace(那一行, &那一行.replace(r#""m3u", "#, ""))
            .as_bytes(),
    );
    不吃.建子库("ES-Gamelist", "平台=PS1");
    不吃.换档案(档案);
    不吃.照真线同步一趟();
    assert!(
        卡上的播放列表(不吃.卡.path()).is_empty(),
        "档案说 PS1 的模拟器不吃 m3u，卡上就不该有播放列表"
    );
    assert!(不吃.卡.path().join("ps/某游戏/游戏 (Disc 2).cue").is_file());
}

#[test]
fn 两套多碟游戏的播放列表撞在同一条路径上_一个都不放行_报在放不进目标里() {
    // 同一个目录里两套多碟（一套 `.chd`、一套 `.iso`，各成一个变体），剥掉碟片标记之后都叫「游戏」：
    // 两份播放列表都想落在 `ps/某游戏/游戏.m3u`。播放列表照同一份文件系统声明筛过——撞车一个都不放行
    // （**落点撞车**），碟照常搬。
    let 库 = temp_dir("run-discs-twice-lib");
    for 碟 in 1..=2u8 {
        写(
            &库.path().join(format!("ps/某游戏/游戏 (Disc {碟}).chd")),
            &[碟; 1024],
        );
        写(
            &库.path().join(format!("ps/某游戏/游戏 (CD {碟}).iso")),
            &[碟 + 10; 1024],
        );
    }
    let mut 现场 = 现场::摆在(库);
    现场.建子库("ES-Gamelist", "平台=PS1");
    let prepared = sync::prepare(
        &现场.catalog,
        现场.工作区.path(),
        "掌机",
        &sync::Request::default(),
        &Handle::new(),
    )
    .expect("排得出计划");
    let 撞了: Vec<&sync::Rejected> = prepared
        .plan
        .rejected
        .iter()
        .filter(|row| row.path == "ps/某游戏/游戏.m3u")
        .collect();
    assert_eq!(
        撞了.len(),
        2,
        "两份播放列表都报出来：{:?}",
        prepared.plan.rejected
    );
    assert!(
        撞了
            .iter()
            .all(|row| row.reason == romcat_core::capability::RejectReason::Collision)
    );
    // 话要说对撞车的原因：不是两块盘上的同一条相对路径，是同一个目录里两套多碟同名；还得说出是哪一套。
    for row in &撞了 {
        assert!(row.detail.contains("多碟"), "{}", row.detail);
        assert!(!row.detail.contains("两个根"), "{}", row.detail);
        assert!(row.detail.contains(&row.variant), "{}", row.detail);
    }
    assert!(
        !prepared
            .plan
            .steps
            .iter()
            .any(|step| step.path == "ps/某游戏/游戏.m3u"),
        "撞上的一个都不放行"
    );

    现场.照真线同步一趟();
    assert!(卡上的播放列表(现场.卡.path()).is_empty());
    assert!(现场.卡.path().join("ps/某游戏/游戏 (CD 2).iso").is_file());
}

#[test]
fn 两家前端都要读平台清单_那份清单写坏了照实说读不动() {
    // 认各张碟要平台清单（与扫描、成型同一条查法）。票 11 那时 Pegasus 用不上播放列表、连清单都不读；票 18 之后两家
    // 都用得上，清单读不动时两家都照实说，而不是悄悄少生成几份播放列表（代码里「用不上的前端不读」那道闸还留着，
    // 给答「用不上」的适配器——`Adapter::uses_playlists` 的默认）。
    for 格式 in ["ES-Gamelist", "Pegasus"] {
        let mut 现场 = 现场::摆在(建个多碟的库());
        现场.建子库(格式, "平台=PS1");
        写(
            &现场.工作区.path().join("platforms.toml"),
            b"[[[ \xe5\x86\x99\xe5\x9d\x8f\xe4\xba\x86",
        );
        let Err(sync::PlanCutoff::Unplanned(why)) = sync::prepare(
            &现场.catalog,
            现场.工作区.path(),
            "掌机",
            &sync::Request::default(),
            &Handle::new(),
        ) else {
            panic!("{格式} 要读平台清单，读不动该排不出");
        };
        assert!(why.to_string().contains("平台清单读不动"), "{格式}：{why}");
    }
}
