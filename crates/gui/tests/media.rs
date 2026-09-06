//! **详情面板的媒体预览**（票 `gui-redesign/07`）：封面与截图内嵌画出来，视频是一张
//! 抽出来的首帧加一个播放标。
//!
//! 这几条断言不看像素，看的是**状态转换**（规格的「接缝二」）：
//!
//! - 池里那张 png / jpg **画得出来**，而且传上显卡那张的纵横比与原图一致。
//! - **ffmpeg 不在的机器上照常可用**：视频那一格是占位，一帧照常画完，不报错、不崩。
//! - **切换选中那一帧不解码**：第一帧问出去、后台去解，图是后面某一帧的事。
//! - **媒体池不在位**时如实说「没查」，而不是摆一屏「找不到文件」。
//!
//! ffmpeg 那一条**不靠这台机器装没装**：抽帧那个程序名是收进来的
//! （`preview::NO_SUCH_PROGRAM`），于是那条退化路在装了 ffmpeg 的机器上照样测得到。

use std::time::{Duration, Instant};

use romcat_core::catalog::Catalog;
use romcat_core::catalog::browse::{WorkAnchor, WorkQuery};
use romcat_core::catalog::scrape::{Harvested, HarvestedMedia};
use romcat_core::scrape::pool::MediaPool;
use romcat_core::scrape::preview;
use romcat_core::scrape::{AnchorKind, MediaKind};
use romcat_core::testing::{TempDir, temp_dir};
use romcat_gui::app::{App, View};
use romcat_gui::media::Look;
use romcat_gui::{demo, headless};

/// 合成数据的规模。这几条测的是媒体那一块，不必照真库那四万多行来。
const ROWS: u64 = 2_000;

/// 这几条测试自己的**工作目录**。
///
/// **不共用 `demo::workspace()`**：那是 `--demo` 那个演示窗口用的目录，而窗口会往里写
/// **版式偏好**（面板拖到哪儿）。共用的话，维护者开一次演示窗口把某块面板拖高一截，
/// 下一次跑这几条测试屏上就少了几行——而断言数的正好是行数。
fn 工作目录() -> std::path::PathBuf {
    std::env::temp_dir().join("romcat-测试-媒体")
}

/// 一张纯色 PNG 的字节。3:4 是详情面板里封面那一格的比例。
fn png(width: u32, height: u32) -> Vec<u8> {
    let buf = image::RgbImage::from_pixel(width, height, image::Rgb([30, 90, 160]));
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(buf)
        .write_to(&mut out, image::ImageFormat::Png)
        .expect("编得出 PNG");
    out.into_inner()
}

/// 这一趟现场里那几份媒体各是谁。
struct 现场 {
    /// 工作目录得活到测试结束——媒体池就在它里头。
    目录: std::path::PathBuf,
    _dir: TempDir,
    app: App,
    /// 挂着媒体的那个作品。
    anchor: WorkAnchor,
    /// 封面那张图的内容哈希。
    封面: String,
    /// 那份「视频」的内容哈希。
    片子: String,
    /// 这一趟的**媒体池**。测试拿它往里摆一份首帧。
    pool: MediaPool,
}

/// 一份**带真媒体池**的现场：池里一张真 png、一份假 mp4，中立库上挂好引用。
///
/// 合成数据自己那几条媒体引用**池里没有对应文件**（`demo::browse` 只记库不落盘），
/// 那是有意的——它撑的是「缺哪些媒体」那条验收。这里另加两条真的。
fn 现场(tag: &str) -> 现场 {
    let dir = temp_dir(tag);
    let pool =
        MediaPool::open(&romcat_core::workspace::media_pool_dir(dir.path())).expect("开得出媒体池");
    let mut catalog = demo::browse(ROWS).expect("造得出合成数据");

    // 挂在**作品锚点**上：封面本来就锚在作品这一层（ADR-0009），而详情面板把作品锚点
    // 与变体锚点合起来看——于是点开这一行的任何一个变体，这张封面都在。
    let 一行 = catalog
        .work_page(&WorkQuery::default(), 0, 64)
        .expect("取得出一页")
        .into_iter()
        .find(|row| matches!(row.anchor, WorkAnchor::Work(_)))
        .expect("合成数据里该有认出了作品的行");

    let 封面 = 入池(&pool, &mut catalog, &png(600, 800), "png");
    // 一份「视频」：字节是什么不要紧——抽帧那条路在拉进程那一步就该退化。
    let 片子 = 入池(&pool, &mut catalog, &[0x00; 256], "mp4");
    catalog
        .put_scraped(&[Harvested {
            anchor: AnchorKind::Work.label().to_string(),
            subject: 一行.name.clone(),
            source: "测试".to_string(),
            input: "测试指纹".to_string(),
            values: Vec::new(),
            media: vec![
                HarvestedMedia {
                    kind: MediaKind::Cover.label().to_string(),
                    hash: 封面.clone(),
                    evidence: "测试摆进去的".to_string(),
                },
                HarvestedMedia {
                    kind: MediaKind::Video.label().to_string(),
                    hash: 片子.clone(),
                    evidence: "测试摆进去的".to_string(),
                },
            ],
        }])
        .expect("写得进");

    let site = demo::site(catalog).expect("开得出现场");
    let mut app = App::new(site, dir.path().to_path_buf());
    app.show_view(View::Browse);
    // 点开那一行——**这一下顺带选中它底下第一个变体**，于是详情面板三层都有东西摆，
    // 媒体那几格才真的画得到（`Gallery::cell`）。
    {
        let (browse, site) = app.browse_and_site();
        browse.open_work(&site.catalog, &一行.anchor);
    }
    现场 {
        目录: dir.path().to_path_buf(),
        _dir: dir,
        app,
        anchor: 一行.anchor,
        封面,
        片子,
        pool,
    }
}

fn 入池(pool: &MediaPool, catalog: &mut Catalog, bytes: &[u8], ext: &str) -> String {
    let (hash, _) = pool.take_bytes(bytes, ext).expect("落得进池");
    catalog
        .put_media(&hash, ext, u64::try_from(bytes.len()).unwrap_or(0))
        .expect("记得进库");
    hash
}

fn 跑(ctx: &egui::Context, app: &mut App, frames: u32) {
    for _ in 0..frames {
        headless::frame(ctx, headless::input(), |ui| app.ui(ui));
    }
}

/// 一帧一帧跑到后台那几件都做完（或者等不下去了）。
fn 等图(ctx: &egui::Context, app: &mut App, 最多: Duration) {
    let 截止 = Instant::now() + 最多;
    loop {
        跑(ctx, app, 1);
        if app.browse().gallery().busy() == 0 || Instant::now() >= 截止 {
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// 详情里那条封面。
fn 封面条(app: &App, hash: &str) -> romcat_core::catalog::detail::MediaItem {
    app.browse()
        .detail()
        .expect("点开得了")
        .media_items
        .iter()
        .find(|item| item.hash == hash)
        .expect("那条封面该列出来")
        .clone()
}

#[test]
fn 封面在详情面板里画得出来而且纵横比一个像素都不歪() {
    // 用户故事 16：「我想在详情里直接看见封面与截图，不必照着路径去文件管理器里开」。
    let ctx = headless::context();
    let mut 场 = 现场("gui-媒体-画得出来");
    等图(&ctx, &mut 场.app, Duration::from_secs(20));

    let item = 封面条(&场.app, &场.封面);
    let look = 场.app.browse().gallery().look(&item);
    let Look::Ready(texture) = look else {
        panic!("封面该画得出来，实际是 {look:?}");
    };
    let size = texture.size_vec2();
    assert!(
        (size.x / size.y - 0.75).abs() < 0.01,
        "原图是 3:4，传上去的这张成了 {size:?}",
    );
    assert!(场.app.browse().gallery().ready() >= 1);
    assert!(
        场.app.browse().gallery().error().is_none(),
        "不该出错：{:?}",
        场.app.browse().gallery().error(),
    );
}

#[test]
fn ffmpeg不在时视频那一格是占位而不是报错() {
    // 用户故事 18，也是规格里点名要钉的那条：「ffmpeg 不在时，视频位显示占位图标
    // 而不是报错」。**这台机器装没装都测得到**——抽帧那个程序名是给进去的。
    let ctx = headless::context();
    let mut 场 = 现场("gui-媒体-没有ffmpeg");
    场.app
        .browse_and_site()
        .0
        .gallery_mut()
        .set_program(preview::NO_SUCH_PROGRAM);
    等图(&ctx, &mut 场.app, Duration::from_secs(20));

    let detail = 场.app.browse().detail().expect("点开得了");
    let 视频 = detail
        .media_items
        .iter()
        .find(|item| item.kind == MediaKind::Video)
        .expect("那条视频该列出来");
    let look = 场.app.browse().gallery().look(视频);
    let Look::Missing(why) = look else {
        panic!("没有 ffmpeg 就该是占位，实际是 {look:?}");
    };
    assert!(why.is_no_ffmpeg(), "该说的是「没这个程序」，实际是 {why:?}");
    assert!(
        场.app.browse().gallery().lacks_ffmpeg(),
        "面板该说得出这台机器上没有 ffmpeg",
    );
    // **不是错误**：一台没装 ffmpeg 的机器上这个窗口照常可用。
    assert!(场.app.browse().gallery().error().is_none());
    // 而封面照旧画得出来——一格没有图，不该把整块拖下水。
    assert!(matches!(
        场.app.browse().gallery().look(&封面条(&场.app, &场.封面)),
        Look::Ready(_)
    ));
    // 再跑几帧，确认它不会因为「没有 ffmpeg」每帧重试一次。
    跑(&ctx, &mut 场.app, 3);
    assert_eq!(场.app.browse().gallery().busy(), 0, "不该反复重排");
}

#[test]
fn 切换选中那一帧不解码() {
    // 「大图不拖慢翻行——切换选中时不阻塞画帧」。**头一帧只把活排出去**，
    // 图是后面某一帧的事：解码若摊在画帧那条线程上，第一帧结束时它就已经在了。
    let ctx = headless::context();
    let mut 场 = 现场("gui-媒体-不阻塞");
    let 起 = Instant::now();
    跑(&ctx, &mut 场.app, 1);
    let 头一帧 = 起.elapsed();

    assert_eq!(
        场.app.browse().gallery().ready(),
        0,
        "头一帧就把图解出来了，那说明解码在画帧这条线程上",
    );
    assert!(场.app.browse().gallery().busy() > 0, "该有活排出去了");
    assert!(
        头一帧 < Duration::from_secs(1),
        "头一帧花了 {头一帧:?}，翻行会看得出来",
    );

    等图(&ctx, &mut 场.app, Duration::from_secs(20));
    assert!(场.app.browse().gallery().ready() >= 1, "后来该解出来");
}

#[test]
fn 媒体池不在位时如实说没查而不是说没有() {
    // 与 ADR-0021 同一条规矩：「没查」与「查了、没有」是两件事。
    // 这一趟的工作目录**故意不存在**（里头没有 `media/`），走的正是这条路。
    let ctx = headless::context();
    let site = demo::site(demo::browse(ROWS).expect("造得出合成数据")).expect("开得出现场");
    let mut app = App::new(site, 工作目录());
    app.show_view(View::Browse);
    {
        let (browse, site) = app.browse_and_site();
        let 一行 = site
            .catalog
            .work_page(&WorkQuery::default(), 0, 1)
            .expect("取得出一页")
            .into_iter()
            .next()
            .expect("总该有一行");
        browse.open_work(&site.catalog, &一行.anchor);
    }
    跑(&ctx, &mut app, 2);

    assert!(!app.browse().gallery().has_pool(), "这一趟本来就没指池子");
    for item in &app.browse().detail().expect("点开得了").media_items {
        assert!(
            matches!(app.browse().gallery().look(item), Look::NoPool),
            "没指池子时该说「没查」，实际是 {:?}",
            app.browse().gallery().look(item),
        );
    }
}

#[test]
fn 一屏媒体都画完之后别的屏照常切得动() {
    // 「任务跑着的时候照常用别的屏」那条规矩，在媒体这一侧的样子：后台那条解码线程
    // 不该把窗口按住。**排着活的时候切屏、切回来，图还在。**
    let ctx = headless::context();
    let mut 场 = 现场("gui-媒体-切屏");
    跑(&ctx, &mut 场.app, 1);
    场.app.show_view(View::Queue);
    跑(&ctx, &mut 场.app, 2);
    场.app.show_view(View::Browse);
    等图(&ctx, &mut 场.app, Duration::from_secs(20));

    assert_eq!(
        场.app.browse().work().expect("还开着").anchor,
        场.anchor,
        "切出去再切回来，点开的还是那一行",
    );
    assert!(matches!(
        场.app.browse().gallery().look(&封面条(&场.app, &场.封面)),
        Look::Ready(_)
    ));
}

#[test]
fn 抽过的首帧第二次打开一个进程都不拉() {
    // 验收第四条在界面这一侧的样子：**抽出来的首帧缓存进媒体池，第二次打开不重抽**。
    //
    // 摆法是先把「上一趟抽出来的那一帧」按内容哈希放进池、映射记进中立库，再把抽帧
    // 那个程序名换成一个**不存在的**——这一趟只要拉了进程就必然抽不出来。
    // 于是「视频那一格画得出来」这条断言本身，就证明了它一个进程都没拉。
    let ctx = headless::context();
    let mut 场 = 现场("gui-媒体-不重抽");
    let 首帧 = {
        let (_, site) = 场.app.browse_and_site();
        入池(&场.pool, &mut site.catalog, &png(640, 480), "png")
    };
    {
        let (_, site) = 场.app.browse_and_site();
        site.catalog
            .put_media_frame(&场.片子, &首帧)
            .expect("记得进库");
    }
    场.app
        .browse_and_site()
        .0
        .gallery_mut()
        .set_program(preview::NO_SUCH_PROGRAM);
    等图(&ctx, &mut 场.app, Duration::from_secs(20));

    let detail = 场.app.browse().detail().expect("点开得了");
    let 视频 = detail
        .media_items
        .iter()
        .find(|item| item.hash == 场.片子)
        .expect("那条视频该列出来");
    let look = 场.app.browse().gallery().look(视频);
    let Look::Ready(texture) = look else {
        panic!("记过首帧就该直接画得出来，实际是 {look:?}");
    };
    let size = texture.size_vec2();
    assert!(
        (size.x / size.y - 640.0 / 480.0).abs() < 0.01,
        "画出来的该是那一帧，实际 {size:?}",
    );
    // 一格都没拉进程，因此「这台机器没 ffmpeg」那句提示也不该冒出来。
    assert!(!场.app.browse().gallery().lacks_ffmpeg());
}

#[test]
fn 没解出来的那几件面板上说得清是哪一件为什么() {
    // 验收第五条：「媒体读不动时说清是哪一件、为什么，不静默留空」。
    // 合成数据自己那几条媒体引用**池里没有对应文件**，正好是这条要说的情形。
    let ctx = headless::context();
    let mut 场 = 现场("gui-媒体-说得清");
    场.app
        .browse_and_site()
        .0
        .gallery_mut()
        .set_program(preview::NO_SUCH_PROGRAM);
    等图(&ctx, &mut 场.app, Duration::from_secs(20));

    let items = 场
        .app
        .browse()
        .detail()
        .expect("点开得了")
        .media_items
        .clone();
    let 说的 = 场.app.browse().gallery().troubles(&items);
    assert!(!说的.is_empty(), "合成数据那几条池里都没有文件，该说得出来");
    for (是哪一件, 为什么) in &说的 {
        assert!(!是哪一件.is_empty(), "得说清是哪一件");
        assert!(!为什么.is_empty(), "得说清为什么");
    }
    // **「这台机器没装 ffmpeg」不在这张单子里**：面板另说一句就够了，
    // 真库里 178 个视频各摆一行是噪音。
    assert!(
        !说的
            .iter()
            .any(|(_, 为什么)| 为什么.contains(preview::NO_SUCH_PROGRAM)),
        "没装 ffmpeg 那一档不该逐件重复：{说的:?}",
    );
    assert!(场.app.browse().gallery().lacks_ffmpeg(), "它该由那一句单说");
    // 而真摆进池里的那张封面不该出现在这张单子上——它好好的。
    assert!(
        !说的
            .iter()
            .any(|(是哪一件, _)| 是哪一件.starts_with("封面 · 测试")),
        "解出来了的不该报成没解出来：{说的:?}",
    );
}

#[test]
fn 没选中变体时后台跑完的那几件照样收得回来() {
    // **不收的话，人点开一段视频、抽到一半切走，那份已经落进池里的首帧就成了孤儿**：
    // 池里多一个文件，两张表里一行都没有，下次打开照样重抽。
    // 这里用得着的观察点是 `busy()`：收账那一步（`Loader::drain`）不跑，它永远归不了零。
    let ctx = headless::context();
    let mut 场 = 现场("gui-媒体-没选中也收账");
    跑(&ctx, &mut 场.app, 1);
    assert!(场.app.browse().gallery().busy() > 0, "该有活排出去了");

    // 把选中的那个变体撤掉：详情面板整块没得画了。
    {
        let (browse, site) = 场.app.browse_and_site();
        browse.pick(&site.catalog, "这个键谁也不是");
    }
    assert!(场.app.browse().detail().is_none(), "该没得画了");

    等图(&ctx, &mut 场.app, Duration::from_secs(20));
    assert_eq!(
        场.app.browse().gallery().busy(),
        0,
        "没选中变体时收账那一步没跑，后台那几件永远挂着",
    );
    assert_eq!(场.app.browse().gallery().pending(), 0, "没有首帧欠着记库");
}

/// 一个**冒充 ffmpeg 的脚本**：不管收到什么参数，都把这份 PNG 原样吐到标准输出。
///
/// `#[cfg(unix)]`：Windows 上写不出这么一个可执行脚本。那边这条路由核心库那两条
/// （「程序不在」与「记过就不重抽」）各盖住一半。
#[cfg(unix)]
fn 假ffmpeg(dir: &std::path::Path, 吐出来的: &std::path::Path) -> std::path::PathBuf {
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
fn 任务在跑时首帧先攒着不去跟别人抢写锁() {
    // 「大图不拖慢翻行——切换选中时不阻塞画帧」在**写库**那一侧的样子。
    //
    // 记首帧走的是画帧线程手里那份写连接，而扫描在后台另拿着一份**写得动**的连接
    // （`Catalog::prepare` 那段注释写着）。两个写者撞上时，SQLite 会在
    // `busy_timeout` 上等——**最长十秒的画帧线程阻塞**。于是任务台上有活在跑时先攒着，
    // 跑完了再记。**攒着比丢掉要紧**：池里那个文件已经落下去了，不记库它就是个孤儿。
    let ctx = headless::context();
    let mut 场 = 现场("gui-媒体-攒着");
    let 样张 = 场.目录.join("样张.png");
    std::fs::write(&样张, png(320, 240)).expect("写得出样张");
    let 程序 = 假ffmpeg(&场.目录, &样张);
    场.app
        .browse_and_site()
        .0
        .gallery_mut()
        .set_program(程序.to_string_lossy().as_ref());

    // 占住任务台。**由测试放行，不靠睡够多久**——睡一个固定的时长，机器慢一点
    // 这条测试就时灵时不灵。收场是失败还是完成不要紧，要的只是这段时间里 `busy()` 为真。
    let 放行 = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let 它 = std::sync::Arc::clone(&放行);
        let (_, _, tasks) = 场.app.browse_site_and_tasks();
        tasks.queue("占着台子", move |_| {
            while !它.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err("测试用的占位活".to_string())
        });
    }

    // 跑到首帧抽完（后台那条解码线程不受任务台影响）。
    等图(&ctx, &mut 场.app, Duration::from_secs(20));
    assert!(
        场.app.browse().gallery().pending() > 0,
        "任务在跑时该先攒着，而不是去跟它抢写锁",
    );
    {
        let (_, site) = 场.app.browse_and_site();
        assert_eq!(
            site.catalog.media_frame(&场.片子).expect("读得出"),
            None,
            "还没轮到记库",
        );
    }

    // 放它走，再跑几帧——这时候才记进去。
    放行.store(true, std::sync::atomic::Ordering::Relaxed);
    let 截止 = Instant::now() + Duration::from_secs(20);
    while 场.app.browse().gallery().pending() > 0 && Instant::now() < 截止 {
        跑(&ctx, &mut 场.app, 1);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(场.app.browse().gallery().pending(), 0, "该记进去了");
    assert!(场.app.browse().gallery().error().is_none());
    let (_, site) = 场.app.browse_and_site();
    let 记着的 = site.catalog.media_frame(&场.片子).expect("读得出");
    assert!(记着的.is_some(), "首帧该记进中立库了");
    // 而池里那一份也在——落盘那一半后台早做完了（ADR-0009）。
    assert!(场.pool.contains(&记着的.expect("有"), "png"));
}
