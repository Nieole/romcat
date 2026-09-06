//! **媒体预览接缝**：池里那一份怎么变成一张画得出来的图（票 `gui-redesign/07`）。
//!
//! 这几条断言不看代码长什么样，看的是**跑出来的结果**：
//!
//! 1. jpg 与 png 都解得开，而且**纵横比一个像素都不歪**；
//! 2. **ffmpeg 不在的机器上照常可用**——视频那一格是占位，不报错、不崩；
//! 3. 抽出来的首帧按**内容哈希**进池、映射记进中立库，**第二次打开不重抽**；
//! 4. 读不动的那些**说得清是哪一件、为什么**，不静默留空；
//! 5. 解码**不在调用方那条线程上**——[`Loader`] 排出去，问一次立刻返回。
//!
//! 第 2、3 两条各有一个**不依赖这台机器装没装 ffmpeg** 的测法：抽帧那个程序名是收进来的
//! （[`preview::NO_SUCH_PROGRAM`]），于是「它不在」那条路在装了 ffmpeg 的机器上照样测得到。
//! 这不是绕开验收——**验收要的正是那条退化路走得通**，而不是「跑测试这台机器碰巧没装」。

use std::path::Path;
use std::time::Duration;

use romcat_core::catalog::Catalog;
use romcat_core::scrape::pool::MediaPool;
use romcat_core::scrape::preview::{self, EDGE, Loader, Missing, Preview};
use romcat_core::testing::{TempDir, temp_dir};

/// 一张纯色 PNG 的字节。
fn png(width: u32, height: u32) -> Vec<u8> {
    编码(width, height, image::ImageFormat::Png)
}

/// 一张纯色 JPEG 的字节。**真库里图片的大头是 jpg**（342 张，`docs/library-facts.md`）。
fn jpg(width: u32, height: u32) -> Vec<u8> {
    编码(width, height, image::ImageFormat::Jpeg)
}

fn 编码(width: u32, height: u32, format: image::ImageFormat) -> Vec<u8> {
    let buf = image::RgbImage::from_pixel(width, height, image::Rgb([200, 40, 40]));
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(buf)
        .write_to(&mut out, format)
        .expect("编得出来");
    out.into_inner()
}

/// 一个空池，外加一份**只活在内存里**的中立库。
fn 现场(tag: &str) -> (TempDir, MediaPool, Catalog) {
    let dir = temp_dir(tag);
    let pool = MediaPool::open(&dir.path().join("media")).expect("开得出池");
    let catalog = Catalog::open_in_memory().expect("开得出中立库");
    (dir, pool, catalog)
}

/// 把一串字节收进池里，返回它的内容哈希。
fn 入池(pool: &MediaPool, catalog: &mut Catalog, bytes: &[u8], ext: &str) -> String {
    let (hash, _) = pool.take_bytes(bytes, ext).expect("落得进池");
    catalog
        .put_media(&hash, ext, bytes.len() as u64)
        .expect("记得进库");
    hash
}

#[test]
fn png与jpg都解得开而且纵横比一个像素都不歪() {
    let (_dir, pool, mut catalog) = 现场("预览-两种格式");
    // 3:4 是详情面板里封面那一格的比例（原型 `prototype.html` 的 `.thumb`）。
    let 封面 = 入池(&pool, &mut catalog, &png(1200, 1600), "png");
    let 截图 = 入池(&pool, &mut catalog, &jpg(1600, 1200), "jpg");

    let 出来的封面 = preview::from_pool(&pool, &封面, "png", EDGE);
    let thumb = 出来的封面.ready().expect("png 该解得开");
    assert_eq!(thumb.width(), EDGE * 3 / 4);
    assert_eq!(thumb.height(), EDGE);
    assert_eq!(
        thumb.rgba().len() as u32,
        thumb.width() * thumb.height() * 4,
        "RGBA8 的长度得是 宽 × 高 × 4",
    );

    let 出来的截图 = preview::from_pool(&pool, &截图, "jpg", EDGE);
    let thumb = 出来的截图.ready().expect("jpg 该解得开");
    assert_eq!((thumb.width(), thumb.height()), (EDGE, EDGE * 3 / 4));
}

#[test]
fn 读不动的那些说得清是哪一件为什么() {
    let (_dir, pool, mut catalog) = 现场("预览-说得清");

    // 一、**库里记着、池里没有**。这与「一条引用都没有」是两件事：前者导出时铺不出图。
    let 幽灵 = "0".repeat(64);
    let 出来的 = preview::from_pool(&pool, &幽灵, "png", EDGE);
    let why = 出来的.missing().expect("池里没有这个文件");
    assert!(matches!(why, Missing::NotInPool { .. }), "{why:?}");
    assert!(
        why.render().contains(&幽灵),
        "那句话得指名道姓：{}",
        why.render()
    );

    // 二、**字节在，解不开**：半截文件。
    let mut 半截 = png(64, 64);
    半截.truncate(24);
    let 坏的 = 入池(&pool, &mut catalog, &半截, "png");
    let why = preview::from_pool(&pool, &坏的, "png", EDGE)
        .missing()
        .cloned()
        .expect("这不该解得开");
    assert!(matches!(why, Missing::Undecodable { .. }), "{why:?}");

    // 三、**这一版不解这个格式**：与「这份文件坏了」分得开，说的是工具的能力边界。
    let 别的 = 入池(&pool, &mut catalog, &png(8, 8), "gif");
    let why = preview::from_pool(&pool, &别的, "gif", EDGE)
        .missing()
        .cloned()
        .expect("gif 这一版不解");
    assert!(matches!(why, Missing::UnknownFormat { .. }), "{why:?}");
}

#[test]
fn ffmpeg不在的机器上视频是占位而不是报错() {
    let (_dir, pool, mut catalog) = 现场("预览-没有ffmpeg");
    // 一份「视频」：字节是什么不要紧，这条路在拉进程那一步就该退化。
    let 片子 = 入池(&pool, &mut catalog, &[0x00; 512], "mp4");

    let mut loader = Loader::start(pool.clone(), EDGE);
    loader.set_program(preview::NO_SUCH_PROGRAM);
    assert!(loader.want(&片子, "mp4", None).is_none(), "第一问该还没好");
    let 首帧 = loader.settle(Duration::from_secs(10));

    assert!(首帧.is_empty(), "没抽出来就不该有首帧要记库");
    let why = loader
        .want(&片子, "mp4", None)
        .and_then(Preview::missing)
        .expect("该是一档占位");
    assert!(why.is_no_ffmpeg(), "该说的是「没这个程序」，实际是 {why:?}");
    // 那句话要说得出**该去干什么**：装 ffmpeg，或者直接点开用系统播放器。
    assert!(
        why.render().contains(preview::NO_SUCH_PROGRAM),
        "{}",
        why.render()
    );
}

#[test]
fn 抽首帧这条路在装没装ffmpeg的机器上都不崩() {
    // 上一条测的是「程序不在」；这一条走的是**默认那个程序名**，于是装了 ffmpeg 的机器
    // 真去抽一帧、没装的机器退化成占位。两种结局都算通过——**不许 panic、不许返回错误**。
    let (_dir, pool, mut catalog) = 现场("预览-真跑一趟");
    let 片子 = 入池(&pool, &mut catalog, &[0x00; 512], "mp4");
    let at = pool.path_of(&片子, "mp4");

    match preview::extract_frame(preview::FFMPEG, &at) {
        Ok(png) => assert!(!png.is_empty(), "抽出来了就得有字节"),
        Err(why) => assert!(
            matches!(why, Missing::NoFfmpeg { .. } | Missing::FrameFailed { .. }),
            "抽不出来只该是这两档，实际是 {why:?}",
        ),
    }
}

#[test]
fn 首帧记进中立库之后第二次打开一个进程都不拉() {
    let (_dir, pool, mut catalog) = 现场("预览-不重抽");
    let 片子 = 入池(&pool, &mut catalog, &[0x00; 512], "mp4");
    // 假装上一趟已经抽出来了：首帧按**内容哈希**进池（ADR-0009），映射记在中立库上。
    let 首帧 = 入池(&pool, &mut catalog, &png(640, 480), "png");
    catalog.put_media_frame(&片子, &首帧).expect("记得进库");
    assert_eq!(
        catalog.media_frame(&片子).expect("读得出"),
        Some(首帧.clone()),
        "记进去的首帧该读得回来",
    );

    // **抽帧那个程序名故意给一个不存在的**：这一趟只要拉了进程就必然抽不出来。
    // 于是「解出来了」这一条断言，本身就证明了它一个进程都没拉。
    let mut loader = Loader::start(pool.clone(), EDGE);
    loader.set_program(preview::NO_SUCH_PROGRAM);
    let 记库的 = {
        loader.want(&片子, "mp4", Some(&首帧));
        loader.settle(Duration::from_secs(10))
    };
    assert!(记库的.is_empty(), "没重抽就没有新首帧要记库");
    let thumb = loader
        .want(&片子, "mp4", Some(&首帧))
        .and_then(Preview::ready)
        .expect("该直接把上一趟那张首帧解出来");
    assert_eq!((thumb.width(), thumb.height()), (EDGE, EDGE * 3 / 4));
}

#[test]
fn 记着抽过但池里那个文件没了就重来一遍() {
    // 人手动清过一次媒体池之后，中立库里那行 `media_frame` 还指着一个已经不在的文件。
    // **不当没抽过的话，那几段视频永远显示占位**，而提示指着一个他从没见过的首帧哈希
    // ——那是个自己修不好的状态。
    let (_dir, pool, mut catalog) = 现场("预览-首帧没了");
    let 片子 = 入池(&pool, &mut catalog, &[0x00; 512], "mp4");
    let 首帧 = 入池(&pool, &mut catalog, &png(640, 480), "png");
    catalog.put_media_frame(&片子, &首帧).expect("记得进库");
    // 把池里那份首帧删掉——库里那行还在。
    std::fs::remove_file(pool.path_of(&首帧, "png")).expect("删得掉");

    let mut loader = Loader::start(pool.clone(), EDGE);
    loader.set_program(preview::NO_SUCH_PROGRAM);
    loader.want(&片子, "mp4", Some(&首帧));
    loader.settle(Duration::from_secs(10));

    // 该走回抽帧那条路，于是在这台没有那个程序的机器上是「没装 ffmpeg」，
    // **不是**「池里没有 <某个哈希>.png」——后者会让人去找一个他从没见过的文件。
    let why = loader
        .want(&片子, "mp4", Some(&首帧))
        .and_then(Preview::missing)
        .expect("该是一档占位");
    assert!(
        why.is_no_ffmpeg(),
        "首帧文件没了就该当没抽过、走回抽帧那条路，实际是 {why:?}",
    );
}

#[test]
fn 问一份图当场就返回解码不在这条线程上() {
    // 「大图不拖慢翻行」那条验收在核心这一侧的样子：**问一次立刻回来**，
    // 图好了没是下一次问的事。真界面上「下一次问」就是下一帧。
    let (_dir, pool, mut catalog) = 现场("预览-不阻塞");
    // 一张 3000×3000 的图，解一次几十毫秒——那正是不该摊在画帧那条线程上的量级。
    let 大图 = 入池(&pool, &mut catalog, &png(3000, 3000), "png");

    let mut loader = Loader::start(pool.clone(), EDGE);
    let 起 = std::time::Instant::now();
    assert!(loader.want(&大图, "png", None).is_none(), "第一问该还没好");
    let 花了 = 起.elapsed();
    assert!(
        花了 < Duration::from_millis(50),
        "问一份图花了 {花了:?}，那说明解码摊在了问的这条线程上",
    );
    assert_eq!(loader.busy(), 1, "该有一件在后台跑着");

    loader.settle(Duration::from_secs(20));
    let thumb = loader
        .want(&大图, "png", None)
        .and_then(Preview::ready)
        .expect("后台该把它解出来");
    assert_eq!((thumb.width(), thumb.height()), (EDGE, EDGE));
    assert_eq!(loader.busy(), 0, "跑完了就不该还挂着");

    // 同一份问第二次不该再排一趟活——翻库时同一格每帧都要问一次。
    let 之前 = loader.cached();
    loader.want(&大图, "png", None);
    assert_eq!(loader.busy(), 0, "缓存里有了还去排队");
    assert_eq!(loader.cached(), 之前);
}

/// 一个**冒充 ffmpeg 的脚本**：不管收到什么参数，都把这份 PNG 原样吐到标准输出。
///
/// `#[cfg(unix)]`：Windows 上写不出这么一个可执行脚本，而这条路在那边由
/// 「程序不在」与「首帧记过就不重抽」两条测试各盖住一半。
#[cfg(unix)]
fn 假ffmpeg(dir: &Path, 吐出来的: &Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let at = dir.join("假ffmpeg");
    std::fs::write(&at, format!("#!/bin/sh\ncat '{}'\n", 吐出来的.display())).expect("写得出脚本");
    let mut perm = std::fs::metadata(&at).expect("读得到").permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&at, perm).expect("改得动权限");
    at
}

#[cfg(unix)]
#[test]
fn 抽出来的首帧落进池里再交回来等着记库() {
    // 验收第四条的前半截：**抽出来的首帧缓存进媒体池**。后半截（记进中立库之后
    // 第二次不重抽）在下面那条与界面那一侧各钉了一遍。
    let (dir, pool, mut catalog) = 现场("预览-抽得出来");
    let 片子 = 入池(&pool, &mut catalog, &[0x00; 512], "mp4");
    let 样张 = dir.path().join("样张.png");
    std::fs::write(&样张, png(320, 240)).expect("写得出样张");
    let 程序 = 假ffmpeg(dir.path(), &样张);

    let mut loader = Loader::start(pool.clone(), EDGE);
    loader.set_program(程序.to_string_lossy().as_ref());
    loader.want(&片子, "mp4", None);
    let 首帧 = loader.settle(Duration::from_secs(20));

    assert_eq!(首帧.len(), 1, "该交回来一份首帧等着记库");
    let frame = &首帧[0];
    assert_eq!(frame.video, 片子, "得说清它是从哪份视频抽出来的");
    assert!(frame.bytes > 0);
    // **落盘那一半后台已经做完了**：池里现在真有这个文件。
    assert!(
        pool.contains(&frame.hash, "png"),
        "首帧该已经按内容哈希落进池里（ADR-0009）",
    );
    // 而记库那一半是调用方的事——这里补上，下一次打开就走「不重抽」那条。
    catalog
        .put_media(&frame.hash, "png", frame.bytes)
        .expect("记得进");
    catalog.put_media_frame(&片子, &frame.hash).expect("记得进");
    assert_eq!(
        catalog.media_frame(&片子).expect("读得出"),
        Some(frame.hash.clone())
    );

    let thumb = loader
        .want(&片子, "mp4", None)
        .and_then(Preview::ready)
        .expect("抽出来的那一帧该当场解成缩略图");
    assert_eq!((thumb.width(), thumb.height()), (320, 240));
}

#[test]
fn 缓存攒过头就只留还看得见的那几份() {
    // 不淘汰的话，按真库翻一遍是 440 张图加 178 段视频的首帧全解过一遍——
    // 缓存里躺着六百来份 RGBA，单份最大 1 MiB，而且永不回落。
    let (_dir, pool, mut catalog) = 现场("预览-缓存淘汰");
    let 几张: Vec<String> = (0..5)
        .map(|i| 入池(&pool, &mut catalog, &png(16 + i, 16), "png"))
        .collect();

    let mut loader = Loader::start(pool.clone(), EDGE);
    for hash in &几张 {
        loader.want(hash, "png", None);
    }
    loader.settle(Duration::from_secs(20));
    assert_eq!(loader.cached(), 5);

    // **上限之内一份不动**：来回切两个变体不该每次重解。
    let 留着: std::collections::HashSet<_> = 几张[..2]
        .iter()
        .map(|hash| (hash.clone(), "png".to_string()))
        .collect();
    loader.trim(&留着, 64);
    assert_eq!(loader.cached(), 5, "没到上限就不该动");

    // 过了上限，只留还看得见的那几份。
    loader.trim(&留着, 3);
    assert_eq!(loader.cached(), 2);
    for hash in &几张[..2] {
        assert!(
            loader
                .want(hash, "png", None)
                .is_some_and(|p| p.ready().is_some()),
            "留着的那两份该还在缓存里（问一次就给，不必再排一趟）",
        );
    }
}

#[test]
fn 抽出来的首帧按内容哈希进池不重复存() {
    // ADR-0009 在这条路上的样子：首帧也是一份内容，**同一串字节只存一个文件**。
    let (_dir, pool, _catalog) = 现场("预览-首帧只存一份");
    let 一帧 = png(320, 240);
    let (头一次, 新的) = pool.take_bytes(&一帧, "png").expect("落得进");
    let (第二次, 又新的) = pool.take_bytes(&一帧, "png").expect("落得进");
    assert_eq!(头一次, 第二次, "同一串字节该算出同一个哈希");
    assert!(新的, "头一次该是新的");
    assert!(!又新的, "第二次是**去重命中**，池里仍然只有一个文件");
    assert!(pool.contains(&头一次, "png"));
}

#[test]
fn 打不开的东西如实报一句话而不是崩() {
    // 点视频那一下调的是系统默认程序。**一台没配默认播放器的机器上，
    // 该看见的是一行提示**，不是一个崩掉的窗口。
    let 结果 = preview::open_externally(Path::new("/不存在的目录/不存在的片子.mp4"));
    // 这台机器上有没有 `xdg-open` 说不准：有就 spawn 成功（那个程序自己去报错），
    // 没有就返回一句话。**两种都不许崩**，而返回错误时那句话得指名道姓。
    if let Err(说的) = 结果 {
        assert!(说的.contains("不存在的片子.mp4"), "{说的}");
    }
}
