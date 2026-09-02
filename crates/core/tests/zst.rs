//! `.zst` 与 `.tar.zst`：**第一个进不了「零解压读 CRC-32」快路的透明容器**。
//!
//! 这些测试守的是票 26 的四条实测结论（`docs/research/zstandard-containers.md`）：
//!
//! 1. 三层都拿不到 CRC-32——内部条目的校验和一栏必须是空的，不能编一个出来。
//! 2. `Frame_Content_Size` 只有约 53% 的文件带它，**不能假设它存在**。
//! 3. 「读第一条」有精确上界（约 131 KB），与文件多大无关；「列全清单」没有捷径。
//! 4. **pax 陷阱**：第一个 512 字节块是伪条目，真条目在偏移 1024。

use std::io::Read;
use std::path::{Path, PathBuf};

use romcat_core::container::{
    self, ContainerError, ContainerKind, Demand, FailureReason, InnerEntry, ReadPlan, zst,
};
use romcat_core::fs::{DirEntry, LibraryFs, MemFs, ReadSeek};
use romcat_core::platform::Manifest;
use romcat_core::testing::container::{
    TarEntrySpec, tar_archive, tar_zst, tar_zst_without_size, zst_needing_dictionary, zstd_frame,
    zstd_frame_without_size,
};

/// 一段**压不动**的字节。
///
/// 用 xorshift 而不是等差数列：等差数列一压就没了，那样整份内容会缩进一个块里，
/// 「读第一条只读 131 KB」与「早停省下了多少」两条就都测不出来。
fn 噪声(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

fn 建库(name: &str, bytes: Vec<u8>) -> MemFs {
    let mut library = MemFs::new();
    library.file(name, bytes);
    library
}

fn 穿(library: &dyn LibraryFs, name: &str) -> container::Listing {
    container::list(library, Path::new(name)).expect("读得出内部构成")
}

/// 一层记账的只读主库：记下每次 `read_head` 要了多少、以及有没有整份打开过。
///
/// 「读够即中止」不能靠读代码相信，得让读盘量本身变成可断言的事实。
struct 记账主库 {
    inner: MemFs,
    head_limits: std::sync::Mutex<Vec<usize>>,
    opens: std::sync::atomic::AtomicUsize,
}

impl 记账主库 {
    fn new(inner: MemFs) -> Self {
        Self {
            inner,
            head_limits: std::sync::Mutex::new(Vec::new()),
            opens: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn 要过的头部长度(&self) -> Vec<usize> {
        self.head_limits.lock().unwrap().clone()
    }

    fn 整份打开过几次(&self) -> usize {
        self.opens.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl LibraryFs for 记账主库 {
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        self.inner.canonicalize(path)
    }

    fn read_dir(&self, dir: &Path) -> std::io::Result<Vec<DirEntry>> {
        self.inner.read_dir(dir)
    }

    fn read_head(&self, file: &Path, limit: usize) -> std::io::Result<Vec<u8>> {
        self.head_limits.lock().unwrap().push(limit);
        self.inner.read_head(file, limit)
    }

    fn read_tail(&self, file: &Path, limit: usize) -> std::io::Result<Vec<u8>> {
        self.inner.read_tail(file, limit)
    }

    fn open(&self, file: &Path) -> std::io::Result<Box<dyn ReadSeek + '_>> {
        self.opens
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.inner.open(file)
    }
}

// ─────────────────────── 帧头：零解压读得出的那些 ───────────────────────

#[test]
fn 帧头零解压读得出大小与校验和标志与字典_id() {
    let plain = 噪声(7, 4096);
    let 带大小 = zstd_frame(&plain);
    let frame = zst::parse_frame(&带大小).expect("解得出帧头");
    assert_eq!(frame.content_size, Some(plain.len() as u64));
    assert!(frame.has_checksum);
    assert_eq!(frame.dictionary_id, None);

    // `tar --zstd` 那一种：**大小与校验和双双缺席**。真盘上约 47% 的文件如此，
    // 「零解压拿未压缩大小」在它们身上不成立。
    let 没大小 = zstd_frame_without_size(&plain);
    let frame = zst::parse_frame(&没大小).expect("解得出帧头");
    assert_eq!(frame.content_size, None);
    assert!(!frame.has_checksum);
}

#[test]
fn 帧头带外部字典_id_的被标记为无法独立解压() {
    let bytes = zst_needing_dictionary(0xDEAD_BEEF, &噪声(3, 512));
    let frame = zst::parse_frame(&bytes).expect("帧头本身是好的");
    assert_eq!(frame.dictionary_id, Some(0xDEAD_BEEF));

    let library = 建库("/lib/PSV/带字典.tar.zst", bytes);
    let error = container::list(&library, Path::new("/lib/PSV/带字典.tar.zst")).unwrap_err();
    assert!(
        matches!(error, ContainerError::NeedsDictionary(0xDEAD_BEEF)),
        "得到的是 {error}"
    );
    // **它要能被数出来**：报告要答得出「有多少个是缺字典的」，那与「结构坏了」
    // 的后续处置完全不同。
    assert_eq!(error.reason(), FailureReason::NeedsDictionary);
}

#[test]
fn 不是_zstd_帧与旧版帧格式是两句不同的话() {
    let library = 建库("/lib/假的.zst", b"PK\x03\x04....".to_vec());
    let error = container::list(&library, Path::new("/lib/假的.zst")).unwrap_err();
    assert!(matches!(error, ContainerError::NotAContainer { .. }));

    // v0.8 之前的帧魔数落在 0xFD2FB522..=0xFD2FB527。认得出来，但解不了。
    let library = 建库("/lib/旧的.zst", vec![0x22, 0xB5, 0x2F, 0xFD, 0, 0, 0, 0]);
    let error = container::list(&library, Path::new("/lib/旧的.zst")).unwrap_err();
    assert!(matches!(error, ContainerError::UnsupportedMethod(_)));
}

// ─────────────────────── 读第一条：便宜的那条路 ───────────────────────

#[test]
fn 读第一条的读盘量有硬上界而且不必整份打开() {
    // 第一条就有 400 KB 压不动的字节：整个文件远远超过上界，
    // 「只读一个块」与「把文件读一遍」在这上面区分得开。
    let bytes = tar_zst(&[
        TarEntrySpec::file("第一个.vpk", 噪声(11, 400 * 1024)),
        TarEntrySpec::file("第二个.vpk", 噪声(12, 400 * 1024)),
    ]);
    assert!(
        bytes.len() > zst::PEEK_LIMIT * 3,
        "样本要比上界大得多才测得出东西，实际 {} 字节",
        bytes.len()
    );

    let library = 记账主库::new(建库("/lib/PSV/大.tar.zst", bytes));
    let peek = zst::peek(&library, Path::new("/lib/PSV/大.tar.zst")).expect("读得出第一条");
    match &peek.inner {
        zst::Inner::Tar(first) => {
            assert_eq!(first.path, "第一个.vpk");
            assert_eq!(first.size, 400 * 1024);
        }
        other => panic!("应该认出里面是 tar，得到的是 {other:?}"),
    }

    let limits = library.要过的头部长度();
    assert_eq!(limits, vec![zst::PEEK_LIMIT], "只读一次头，且不超过上界");
    assert_eq!(
        zst::PEEK_LIMIT,
        14 + 3 + 131_072,
        "上界是规范算出来的：帧头 + 块头 + 一个 Block_Maximum_Size"
    );
    assert_eq!(
        library.整份打开过几次(),
        0,
        "便宜那条路一次都不该整份打开文件——那正是它便宜的原因"
    );
}

#[test]
fn pax_伪条目不会被当成真条目() {
    // bsdtar 默认写 pax。先证明这份样本真的带着那个坑，再证明我们没踩进去。
    let tar = tar_archive(&[TarEntrySpec::file("真条目.vpk", 噪声(21, 300)).with_pax_header()]);
    assert!(
        String::from_utf8_lossy(&tar[..100]).starts_with("PaxHeader/真条目.vpk"),
        "样本的第一个 512 字节块必须是 PaxHeader 伪条目，否则这条测试什么都没验"
    );
    assert!(
        String::from_utf8_lossy(&tar[1024..1100]).starts_with("真条目.vpk"),
        "真条目在偏移 1024"
    );
    assert_eq!(tar[156], b'x', "伪条目的 typeflag 是 'x'");

    let library = 建库("/lib/PSV/pax.tar.zst", zstd_frame(&tar));
    // 便宜那条路要跳过伪条目。
    let peek = zst::peek(&library, Path::new("/lib/PSV/pax.tar.zst")).expect("读得出第一条");
    match &peek.inner {
        zst::Inner::Tar(first) => assert_eq!(first.path, "真条目.vpk"),
        other => panic!("得到的是 {other:?}"),
    }
    // 完整那条路同样不能把伪条目当成一个内部文件。
    let listing = 穿(&library, "/lib/PSV/pax.tar.zst");
    let 名字: Vec<&str> = listing
        .contents
        .entries
        .iter()
        .map(|e| e.path.as_str())
        .collect();
    assert_eq!(名字, vec!["真条目.vpk"]);
    assert_eq!(listing.contents.entries[0].size, 300);
}

#[test]
fn gnu_longname_伪条目不会被当成真条目而且真名字不被截断() {
    // 真库里的形态：`././@LongLink` 块是**老式 v7 头**（magic 处全是零），
    // 真条目的 `name` 只装得下前 100 字节。NDS 那批 559 个里有 414 个如此。
    let 长名 = "牧场物语 欢迎来到风之集市 七夕特别版 修正版(简)日语片头版(JP)(SOMA&YOME汉化组)(1024Mb)/牧场物语 欢迎来到风之集市 七夕特别版 修正版(简)日语片头版(JP)(SOMA&YOME汉化组)(1024Mb).nds";
    assert!(长名.len() > 100, "名字要真的超过 100 字节才测得出截断");
    let tar = tar_archive(&[TarEntrySpec::file(长名, 噪声(23, 700)).with_gnu_long_name()]);
    assert!(
        String::from_utf8_lossy(&tar[..13]) == "././@LongLink",
        "样本的第一个块必须是 GNU longname 伪条目，否则这条测试什么都没验"
    );
    assert_eq!(tar[156], b'L', "伪条目的 typeflag 是 'L'");
    assert_eq!(&tar[257..263], &[0u8; 6], "伪条目是 v7 头：magic 处全是零");

    let library = 建库("/lib/nds/长名.tar.zst", zstd_frame(&tar));
    let peek = zst::peek(&library, Path::new("/lib/nds/长名.tar.zst")).expect("读得出第一条");
    match &peek.inner {
        zst::Inner::Tar(first) => assert_eq!(first.path, 长名),
        other => panic!("得到的是 {other:?}"),
    }

    let listing = 穿(&library, "/lib/nds/长名.tar.zst");
    let 名字: Vec<&str> = listing
        .contents
        .entries
        .iter()
        .map(|e| e.path.as_str())
        .collect();
    assert_eq!(名字, vec![长名], "伪条目不进清单，真名字也不该被截断");
    assert_eq!(listing.contents.entries[0].size, 700);

    // 序号错位是最坏的一种错：它会把甲的字节当成乙的交出去。
    let (_, got) = 读(
        &library,
        "/lib/nds/长名.tar.zst",
        &listing,
        &ReadPlan::all(&listing),
    );
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, 长名);
    assert_eq!(got[0].1, 噪声(23, 700));
}

#[test]
fn 只裹一份内容的_tar_零成本就判得出来() {
    let 单个 = 建库(
        "/lib/PSV/一个.tar.zst",
        tar_zst(&[TarEntrySpec::file("TWEWY.vpk", 噪声(31, 1000))]),
    );
    let peek = zst::peek(&单个, Path::new("/lib/PSV/一个.tar.zst")).expect("读得出第一条");
    assert_eq!(peek.single_member(), Some(true));

    let 两个 = 建库(
        "/lib/PSV/两个.tar.zst",
        tar_zst(&[
            TarEntrySpec::file("甲.vpk", 噪声(32, 1000)),
            TarEntrySpec::file("乙.vpk", 噪声(33, 1000)),
        ]),
    );
    let peek = zst::peek(&两个, Path::new("/lib/PSV/两个.tar.zst")).expect("读得出第一条");
    assert_eq!(peek.single_member(), Some(false));

    // 帧头没写大小时这一问答不了——**答不了要说答不了**，不能猜一个。
    let 没大小 = 建库(
        "/lib/PSV/没大小.tar.zst",
        tar_zst_without_size(&[TarEntrySpec::file("甲.vpk", 噪声(34, 1000))]),
    );
    let peek = zst::peek(&没大小, Path::new("/lib/PSV/没大小.tar.zst")).expect("读得出第一条");
    assert_eq!(peek.single_member(), None);
}

// ─────────────────────── 列全清单：贵的那条路 ───────────────────────

#[test]
fn 列全清单读得出每条的名字与大小但一条_crc_都没有() {
    let 甲 = 噪声(41, 5000);
    let 乙 = 噪声(42, 300);
    let library = 建库(
        "/lib/PSV/ATSP11823.tar.zst",
        tar_zst(&[
            TarEntrySpec::dir("sce_sys"),
            TarEntrySpec::file("sce_sys/param.sfo", 甲.clone()),
            TarEntrySpec::file("eboot.bin", 乙.clone()),
        ]),
    );
    let listing = 穿(&library, "/lib/PSV/ATSP11823.tar.zst");
    assert_eq!(listing.kind, ContainerKind::Zstd);
    assert!(!ContainerKind::Zstd.is_penetrable());

    let entries = &listing.contents.entries;
    assert_eq!(entries.len(), 3);
    assert!(entries[0].is_dir);
    assert_eq!(entries[1].path, "sce_sys/param.sfo");
    assert_eq!(entries[1].size, 甲.len() as u64);
    assert_eq!(entries[2].path, "eboot.bin");
    assert_eq!(entries[2].size, 乙.len() as u64);

    // **一条 CRC 都不能有。** 两层格式里都没有「成员 → 内容哈希」的映射表，
    // 编一个出来等于让第一命中层拿着假指纹去撞 DAT。
    assert!(entries.iter().all(|e| e.crc32.is_none()));
    assert_eq!(
        listing.contents.without_crc(),
        listing.contents.file_count()
    );

    // 整条流是一个顺序单元：它天生就是 solid，读中间那个躲不开前面的字节。
    assert_eq!(listing.contents.blocks, 1);
    assert!(listing.contents.is_solid());
}

#[test]
fn 帧头没写大小时照样列得出全清单() {
    let library = 建库(
        "/lib/PSV/流式.tar.zst",
        tar_zst_without_size(&[
            TarEntrySpec::file("甲.vpk", 噪声(51, 700)),
            TarEntrySpec::file("乙.vpk", 噪声(52, 900)),
        ]),
    );
    let listing = 穿(&library, "/lib/PSV/流式.tar.zst");
    assert_eq!(listing.contents.file_count(), 2);
    assert_eq!(listing.contents.total_size(), 1600);
}

#[test]
fn 老式_v7_的_tar_靠头部校验和也认得出来() {
    let library = 建库(
        "/lib/PSV/老的.tar.zst",
        tar_zst(&[TarEntrySpec::file("甲.vpk", 噪声(61, 400)).as_v7()]),
    );
    let listing = 穿(&library, "/lib/PSV/老的.tar.zst");
    assert_eq!(listing.contents.entries[0].path, "甲.vpk");
}

#[test]
fn 裸_zst_的内部名是外层名去掉后缀() {
    let 内容 = 噪声(71, 2048);
    // 名字里带 `.tar` 也不作数：判据是字节，不是扩展名。这一份的内容不是 tar。
    let library = 建库("/lib/FC/超级马里奥.nes.zst", zstd_frame(&内容));
    let listing = 穿(&library, "/lib/FC/超级马里奥.nes.zst");
    assert_eq!(listing.contents.entries.len(), 1);
    assert_eq!(listing.contents.entries[0].path, "超级马里奥.nes");
    assert_eq!(listing.contents.entries[0].size, 内容.len() as u64);
    assert_eq!(listing.contents.entries[0].crc32, None);
    assert!(!listing.contents.is_solid(), "只有一条，谈不上 solid");

    // 帧头没写大小时，大小只能量一遍——量出来的必须与真值一致。
    let library = 建库("/lib/FC/魂斗罗.nes.zst", zstd_frame_without_size(&内容));
    let listing = 穿(&library, "/lib/FC/魂斗罗.nes.zst");
    assert_eq!(listing.contents.entries[0].size, 内容.len() as u64);
}

#[test]
fn 名字像_tar_zst_而里面不是_tar_时按单文件处理() {
    // 反过来的一半：后缀说是 tar，字节说不是。跟着字节走。
    let 内容 = 噪声(75, 4096);
    let library = 建库("/lib/PSV/骗人的.tar.zst", zstd_frame(&内容));
    let listing = 穿(&library, "/lib/PSV/骗人的.tar.zst");
    assert_eq!(listing.contents.entries.len(), 1);
    assert_eq!(listing.contents.entries[0].path, "骗人的.tar");
}

// ─────────────────────── 取内容：一趟走完，读够就停 ───────────────────────

fn 读(
    library: &dyn LibraryFs,
    path: &str,
    listing: &container::Listing,
    plan: &ReadPlan,
) -> (container::ReadStats, Vec<(String, Vec<u8>)>) {
    let mut got: Vec<(String, Vec<u8>)> = Vec::new();
    let stats = container::read_entries(
        library,
        Path::new(path),
        listing,
        plan,
        &mut |entry: &InnerEntry, reader: &mut dyn Read| {
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf)?;
            got.push((entry.path.clone(), buf));
            Ok(())
        },
    )
    .expect("读得出来");
    (stats, got)
}

#[test]
fn 取内容按顺序一趟走完并且早停() {
    let 甲 = 噪声(81, 200 * 1024);
    let 乙 = 噪声(82, 200 * 1024);
    let 丙 = 噪声(83, 200 * 1024);
    let library = 建库(
        "/lib/PSV/三个.tar.zst",
        tar_zst(&[
            TarEntrySpec::file("甲.vpk", 甲.clone()),
            TarEntrySpec::file("乙.vpk", 乙.clone()),
            TarEntrySpec::file("丙.vpk", 丙.clone()),
        ]),
    );
    let listing = 穿(&library, "/lib/PSV/三个.tar.zst");

    // 只要第一条：后面两条的字节一个都不该解。
    let (stats, got) = 读(
        &library,
        "/lib/PSV/三个.tar.zst",
        &listing,
        &ReadPlan::only(&listing, 0),
    );
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].1, 甲);
    assert_eq!(stats.blocks_decoded, 1, "整条流是一个顺序单元，只走一趟");
    assert_eq!(stats.bytes_decompressed, 甲.len() as u64);

    // 只要最后一条：前面两条**必须**顺着走完——顺序流跳不过去，这是格式使然。
    let (stats, got) = 读(
        &library,
        "/lib/PSV/三个.tar.zst",
        &listing,
        &ReadPlan::only(&listing, 2),
    );
    assert_eq!(got[0].1, 丙);
    assert_eq!(
        stats.bytes_decompressed,
        (甲.len() + 乙.len() + 丙.len()) as u64,
        "跳过 = 真的解出来再丢掉"
    );
}

#[test]
fn 只要前若干字节时读够就停() {
    let 甲 = 噪声(91, 300 * 1024);
    let library = 建库(
        "/lib/PSV/头部.tar.zst",
        tar_zst(&[TarEntrySpec::file("sce_sys/param.sfo", 甲.clone())]),
    );
    let listing = 穿(&library, "/lib/PSV/头部.tar.zst");
    let plan = ReadPlan::new(&listing, |_| Demand::Prefix(0x200));
    let (stats, got) = 读(&library, "/lib/PSV/头部.tar.zst", &listing, &plan);
    assert_eq!(got[0].1, 甲[..0x200]);
    assert_eq!(
        stats.bytes_decompressed, 0x200,
        "只要 0x200 字节就该只解 0x200 字节——票 09 要的 param.sfo 正是这么取"
    );
}

#[test]
fn 裸_zst_的内容取得回来() {
    let 内容 = 噪声(95, 8192);
    let library = 建库("/lib/FC/游戏.nes.zst", zstd_frame(&内容));
    let listing = 穿(&library, "/lib/FC/游戏.nes.zst");
    let (_, got) = 读(
        &library,
        "/lib/FC/游戏.nes.zst",
        &listing,
        &ReadPlan::all(&listing),
    );
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, "游戏.nes");
    assert_eq!(got[0].1, 内容);
}

// ─────────────────── 扫描：默认不读，读过一次就落进中立库 ───────────────────

fn 建一个_zst_库() -> MemFs {
    let mut library = MemFs::new();
    library.dir("/lib").dir("/lib/PSV").file(
        "/lib/PSV/ATSP11823.tar.zst",
        tar_zst(&[
            TarEntrySpec::file("sce_sys/param.sfo", 噪声(101, 1200)),
            TarEntrySpec::file("eboot.bin", 噪声(102, 800)),
        ]),
    );
    library
}

fn 扫(library: &MemFs, options: &romcat_core::scan::ScanOptions) -> romcat_core::catalog::Catalog {
    let mut catalog = romcat_core::catalog::Catalog::open_in_memory().expect("能开中立库");
    let cancel = romcat_core::scan::CancelToken::new();
    romcat_core::scan::scan(library, &mut catalog, options, &cancel).expect("扫得动");
    catalog
}

#[test]
fn 扫描默认不为_zst_付全量解压的代价() {
    let library = 建一个_zst_库();
    let catalog = 扫(&library, &romcat_core::scan::ScanOptions::new("/lib"));
    let totals = catalog
        .aggregate(&Default::default(), &Manifest::builtin())
        .expect("能折出统计")
        .containers
        .totals();
    // **还没读过不是穿不透，也不是「库里没有容器」。** 三者的下一步完全不同。
    assert_eq!(totals.containers, 1);
    assert_eq!(totals.unread, 1);
    assert_eq!(totals.penetrated, 0);
    assert_eq!(totals.failed, 0);
    assert_eq!(totals.inner_files, 0);

    // 报告要说清**为什么**没读：zst 穿不透，读它要把整条流解一遍。
    let report = romcat_core::report::HealthReport::build(
        &catalog
            .aggregate(&Default::default(), &Manifest::builtin())
            .unwrap(),
        &catalog.report_meta().unwrap(),
    );
    assert!(report.containers.unread_includes_impenetrable);
    let text = report.render_text();
    assert!(text.contains("还没读过"), "{text}");
    assert!(text.contains("romcat scan --zst"), "{text}");
}

#[test]
fn 加了开关才读并且第二趟直接沿用中立库里的结论() {
    let library = 建一个_zst_库();
    let mut options = romcat_core::scan::ScanOptions::new("/lib");
    options.decompress_zst = true;

    let mut catalog = romcat_core::catalog::Catalog::open_in_memory().expect("能开中立库");
    let cancel = romcat_core::scan::CancelToken::new();
    romcat_core::scan::scan(&library, &mut catalog, &options, &cancel).expect("首扫");
    let totals = catalog
        .aggregate(&Default::default(), &Manifest::builtin())
        .unwrap()
        .containers
        .totals();
    assert_eq!(totals.penetrated, 1);
    assert_eq!(totals.unread, 0);
    assert_eq!(totals.inner_files, 2);
    assert_eq!(totals.inner_bytes, 2000);
    assert_eq!(totals.inner_without_crc, 2, "zst 一条 CRC 都给不出来");

    // 第二趟**不带**开关：三元组没变，中立库里已经有结论，不该被抹掉，
    // 也不该重新解一遍（重解一遍就是每次扫描重付 3.8–7.6 小时）。
    let 不带开关 = romcat_core::scan::ScanOptions::new("/lib");
    romcat_core::scan::scan(&library, &mut catalog, &不带开关, &cancel).expect("二扫");
    let totals = catalog
        .aggregate(&Default::default(), &Manifest::builtin())
        .unwrap()
        .containers
        .totals();
    assert_eq!(totals.penetrated, 1, "上一趟落库的结论要留着");
    assert_eq!(totals.unread, 0);
    assert_eq!(totals.inner_files, 2);
}

#[test]
fn 内容变了就重解一遍而不是沿用旧清单() {
    let mut library = 建一个_zst_库();
    let mut options = romcat_core::scan::ScanOptions::new("/lib");
    options.decompress_zst = true;
    let mut catalog = romcat_core::catalog::Catalog::open_in_memory().expect("能开中立库");
    let cancel = romcat_core::scan::CancelToken::new();
    romcat_core::scan::scan(&library, &mut catalog, &options, &cancel).expect("首扫");

    // 沿用与否的判据就是增量扫描那一套三元组：换了内容、拨了修改时间，就该重解。
    library.file(
        "/lib/PSV/ATSP11823.tar.zst",
        tar_zst(&[TarEntrySpec::file("只剩这一个.vpk", 噪声(103, 4321))]),
    );
    library.touch("/lib/PSV/ATSP11823.tar.zst", 60);
    romcat_core::scan::scan(&library, &mut catalog, &options, &cancel).expect("二扫");

    let totals = catalog
        .aggregate(&Default::default(), &Manifest::builtin())
        .unwrap()
        .containers
        .totals();
    assert_eq!(totals.inner_files, 1);
    assert_eq!(totals.inner_bytes, 4321);
}

#[test]
fn 头部抽样也说得出_zst_里装着什么() {
    let library = 建一个_zst_库();
    let catalog = 扫(&library, &romcat_core::scan::ScanOptions::new("/lib"));
    let aggregate = catalog
        .aggregate(&Default::default(), &Manifest::builtin())
        .expect("能折出统计");
    let 抽样 = aggregate
        .samples
        .get(&romcat_core::header::ProbeClass::Zstd)
        .expect("zst 这一类被抽到了");
    assert_eq!(抽样.parsed, 1, "帧头加第一个块都读出来了");
    assert_eq!(抽样.sampled, 1);
}
