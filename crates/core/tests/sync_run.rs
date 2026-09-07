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

use romcat_core::adapter;
use romcat_core::capability::Profile;
use romcat_core::catalog::Catalog;
use romcat_core::catalog::Roots;
use romcat_core::catalog::scrape::{Harvested, HarvestedMedia};
use romcat_core::fs::RealFs;
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::scrape::pool::MediaPool;
use romcat_core::scrape::priority::Priorities;
use romcat_core::scrape::{AnchorKind, MediaKind};
use romcat_core::sublibrary::{self, Rule, Selection, Sublibrary};
use romcat_core::sync::{self, Act, FileKind, Manifest, Placement, Sources};
use romcat_core::task::Handle;
use romcat_core::testing::sample::zip;
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
    _工作区: TempDir,
    卡: TempDir,
    catalog: Catalog,
    pool: MediaPool,
    库根: PathBuf,
}

impl 现场 {
    fn 摆好() -> Self {
        Self::摆在(建库())
    }

    fn 摆在(库: TempDir) -> Self {
        let 工作区 = temp_dir("run-ws");
        let 卡 = temp_dir("run-card");
        let 库根 = 库.path().to_path_buf();
        let catalog = 扫成库(&库根);
        let pool = MediaPool::open(&工作区.path().join("media")).expect("池建得出");
        Self {
            _库: 库,
            _工作区: 工作区,
            卡,
            catalog,
            pool,
            库根,
        }
    }

    /// 往**媒体池**里塞一份媒体，并把它挂到某个变体上。
    fn 收一份媒体(&mut self, 变体: &str, kind: MediaKind, bytes: &[u8]) -> String {
        let hash = romcat_core::catalog::frontend::hash_of(bytes);
        let at = self.pool.path_of(&hash, "png");
        写(&at, bytes);
        self.catalog
            .put_media(&hash, "png", bytes.len() as u64)
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
        let mut desired = sync::desired(&self.catalog, &selected, profile).expect("折得出期望状态");
        let media = sync::media::lay(&self.catalog, adapter.as_ref(), &self.pool, &selected)
            .expect("铺得出媒体");
        let frontend = sync::frontend::lay(
            &self.catalog,
            adapter.as_ref(),
            &priorities,
            &selected,
            &media.assets,
        )
        .expect("折得出元数据");
        desired.files.extend(media.files.iter().cloned());
        desired.files.extend(frontend.files.iter().cloned());
        desired.files.sort_by(|a, b| a.path.cmp(&b.path));
        desired.screen(&profile.filesystem, 0);

        let mut 子库 = Sublibrary::at("掌机", self.卡.path(), "Pegasus", None);
        子库.capability = Some(profile.name.clone());
        let actual = sync::observe(&RealFs, self.卡.path()).expect("看得见目标");
        // 与 `sync::prepare` 同一条线：落点的目录段先与目标折齐，再排计划。
        let mut from_pool = media.from_pool;
        let mut generated = frontend.bytes;
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
        let sources = Sources {
            library: &RealFs,
            library_roots: Some(&Roots::single("库", &self.库根)),
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
fn 计划算完之后才出现的落点占用_执行这一层也挡得住() {
    // 计划靠的是 `observe` 交出来的那份键的集合，而那份集合**可能是不全的**：
    // 列不开的目录底下一个键都拿不到，那一枝上的落点计划根本无从判断；计划算完到
    // 真的改名之间也隔着整趟同步的时间，卡还插在机器上。于是执行这一层还要兜一道。
    let 现场 = 现场::摆在(建个只差大小写的库());
    let 这趟 = 现场.排一趟("平台=GB", &Manifest::empty());
    assert!(
        这趟
            .plan
            .steps
            .iter()
            .any(|step| step.act == Act::Add && step.path == "GB/tetris.zip"),
        "计划这一侧看不见它，本来就该排一条新增"
    );

    // 排完计划之后，维护者才把自己那份拷进卡里——只差大小写。
    let 维护者那份 = 现场.卡.path().join("GB/Tetris.zip");
    写(&维护者那份, "这是我自己拷进去的".as_bytes());
    let 原样 = fs::read(&维护者那份).expect("读得出");

    let outcome = 现场.执行(&这趟, &Manifest::empty(), &CancelToken::new());
    assert_eq!(
        fs::read(&维护者那份).expect("还在"),
        原样,
        "维护者自己那份连一个字节都不许动",
    );
    assert!(
        outcome
            .failures
            .iter()
            .any(|failure| failure.path == "GB/tetris.zip" && failure.act == Act::Add),
        "挡下来要记成一条没做成，而不是整趟停住：{:?}",
        outcome.failures
    );
    assert!(
        !outcome
            .manifest
            .files
            .iter()
            .any(|file| file.path.eq_ignore_ascii_case("GB/tetris.zip")),
        "没写成的不许进清单"
    );
}

#[test]

#[test]
fn 只差大小写的是上一级目录_执行这一层照样挡得住() {
    // 折的是**整条键**，不是最后那一段：计划那一侧拿 `path::fold` 折 `gb/Tetris.zip`
    // 一整条，执行这一侧只折文件名的话，上一级目录换个大小写就从缝里漏过去了——
    // 而卡上那个 `gb` 与我们要建的 `GB` 在不敏感的卡上本来就是同一个目录。
    let 现场 = 现场::摆在(建个只差大小写的库());
    let 这趟 = 现场.排一趟("平台=GB", &Manifest::empty());

    // 同样是排完计划之后才出现的：目录这一级也只差大小写。
    //
    // 卡上顺手再摆一个 `GB/`：大小写敏感的盘上它与 `gb/` 能并存，而 `GB` 排在 `gb`
    // 前面。一层只跟排在前面那个候选的话，`gb/` 底下挡路的那份就从缝里漏过去了。
    写(
        &现场.卡.path().join("GB/别的.txt"),
        "维护者自己的东西".as_bytes(),
    );
    let 维护者那份 = 现场.卡.path().join("gb/Tetris.zip");
    写(&维护者那份, "这是我自己拷进去的".as_bytes());
    let 原样 = fs::read(&维护者那份).expect("读得出");

    let outcome = 现场.执行(&这趟, &Manifest::empty(), &CancelToken::new());
    assert_eq!(
        fs::read(&维护者那份).expect("还在"),
        原样,
        "维护者自己那份连一个字节都不许动",
    );
    assert!(
        outcome
            .failures
            .iter()
            .any(|failure| failure.path == "GB/tetris.zip" && failure.act == Act::Add),
        "上一级目录只差大小写也要挡下来：{:?}",
        outcome.failures
    );
    assert!(
        outcome
            .failures
            .iter()
            .any(|failure| failure.why.contains("Tetris.zip")),
        "报告要说得出是哪个落点被占着：{:?}",
        outcome.failures
    );
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
    let 现场 = 现场::摆在(建个只差大小写又不止一件的库());
    let 这趟 = 现场.排一趟("平台=GB", &Manifest::empty());
    let 一共 = 这趟.plan.steps.len();
    assert!(一共 >= 2, "这一趟得有别的步可做：{:?}", 这趟.plan.steps);

    写(
        &现场.卡.path().join("GB/Tetris.zip"),
        "这是我自己拷进去的".as_bytes(),
    );

    let outcome = 现场.执行(&这趟, &Manifest::empty(), &CancelToken::new());
    assert!(!outcome.interrupted && !outcome.gave_up, "不该整趟停住");
    assert_eq!(outcome.failures.len(), 1, "{:?}", outcome.failures);
    assert_eq!(
        outcome.manifest.files.len(),
        一共 - 1,
        "其余几步都该写成、都该进清单：{:?}",
        outcome.manifest.files
    );
    assert_eq!(
        fs::read(现场.卡.path().join("GB/口袋妖怪.zip")).expect("读得出"),
        fs::read(现场.库根.join("GB/口袋妖怪.zip")).expect("读得出"),
        "同一趟里别的那份照常落到卡上",
    );
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
    let 现场 = 现场::摆在(建个够多的库());
    let 这趟 = 现场.排一趟("平台=GB", &Manifest::empty());

    // 排完计划之后，前 11 份的落点全被占上——比那个计数多一个。
    for i in 1..=11 {
        写(
            &现场.卡.path().join(format!("GB/G{i:02}.zip")),
            "这是我自己拷进去的".as_bytes(),
        );
    }

    let outcome = 现场.执行(&这趟, &Manifest::empty(), &CancelToken::new());
    assert!(!outcome.gave_up, "落点被占不该被当成系统性故障");
    assert!(!outcome.interrupted);
    assert_eq!(outcome.failures.len(), 11, "{:?}", outcome.failures);
    assert!(
        outcome
            .failures
            .iter()
            .all(|failure| failure.act == Act::Add),
        "{:?}",
        outcome.failures
    );
    assert_eq!(
        fs::read(现场.卡.path().join("GB/g12.zip")).expect("排在最后那一份照样落得下"),
        fs::read(现场.库根.join("GB/g12.zip")).expect("读得出"),
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
