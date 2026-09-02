//! **ES gamelist 这一趟**：把别人分享的元数据包无损吃进来，再把中立库写成
//! Cocoon / iiSU / Daijishō 三家都直接吃得下的 ES-DE 布局。
//!
//! 这个文件要证的是五件在单元测试里成立、在一趟真的导入导出里未必成立的事：
//!
//! 1. **两个根元素的文件读得动**——`<alternativeEmulator>` 写在 `<gameList>` 外面，
//!    标准 XML 解析器怼上去直接失败（调研 §B.1 源码补充第 8 条）；
//! 2. **用户状态逐条原样搬过去**——收藏、游玩次数、游玩时长、上次游玩长在同一个
//!    文件里，导出时省略等于把维护者多年的记录**清零**（ADR-0006）；
//! 3. **导入是合并不是覆盖**——包里那些我们没认出来的条目一条都不少；
//! 4. **落点是 `gamelists/<系统>/gamelist.xml`**，`<path>` 相对系统 ROM 目录；
//! 5. **媒体按 `downloaded_media/<系统>/<类型>/` 铺**，而且条目里一个媒体路径都不写。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use romcat_core::adapter::gamelist::Gamelist;
use romcat_core::adapter::transfer::{self, ExportOptions};
use romcat_core::adapter::{Capability, assert_capability};
use romcat_core::catalog::Catalog;
use romcat_core::catalog::scrape::{Harvested, HarvestedMedia};
use romcat_core::fs::RealFs;
use romcat_core::scan::{self, CancelToken, Jobs, ScanOptions};
use romcat_core::scrape::pool::MediaPool;
use romcat_core::scrape::priority::Priorities;
use romcat_core::scrape::{AnchorKind, MediaKind};
use romcat_core::sublibrary::{self, Rule, Selection};
use romcat_core::sync;
use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};

const 台版: &str = "FC/魂斗罗台版/魂斗罗.zip";
const 塞尔达: &str = "FC/Zelda.zip";
const 口袋妖怪: &str = "GB/口袋妖怪.zip";

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

struct 现场 {
    dir: TempDir,
    _工作区: TempDir,
    catalog: Catalog,
    pool: MediaPool,
}

impl 现场 {
    /// 导出的落点。**就是主库根**：ES-DE 的 `<path>` 相对系统 ROM 目录解析，
    /// 而系统 ROM 目录就是主库根下那一层平台目录。ADR-0004 允许工具往主库里写
    /// **元数据文件**，ROM 一个字节都不碰。
    fn out(&self) -> &Path {
        self.dir.path()
    }

    fn 包的落点(&self) -> PathBuf {
        self.dir.path().join("gamelists/FC/gamelist.xml")
    }
}

fn 建现场() -> 现场 {
    let dir = temp_dir("gamelist");
    let 工作区 = temp_dir("gamelist-ws");
    写(&dir.path().join(台版), &zip(2_048));
    写(&dir.path().join(塞尔达), &zip(4_096));
    写(&dir.path().join(口袋妖怪), &zip(8_192));

    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    catalog
        .set_library_root(&romcat_core::path::display(dir.path()))
        .expect("写得下主库根");
    let mut options = ScanOptions::new(dir.path());
    options.jobs = Jobs::Fixed(2);
    scan::scan(&RealFs::new(), &mut catalog, &options, &CancelToken::new()).expect("扫得动");
    let pool = MediaPool::open(&工作区.path().join("media")).expect("池建得出");
    现场 {
        dir,
        _工作区: 工作区,
        catalog,
        pool,
    }
}

/// 一份**照真的分享包的样子**写的 gamelist：
///
/// - `<alternativeEmulator>` 在 `<gameList>` **外面**（两个根元素，不是合法 XML）；
/// - 用户状态就长在 `<game>` 里；
/// - 有我们库里根本没有的条目（别人机器上的游戏）；
/// - 有别的 ES 变体写的媒体路径与我们不认得的元素；
/// - `<path>` 相对系统 ROM 目录，带前导 `./`。
const 分享包: &str = "<?xml version=\"1.0\"?>\n\
    <alternativeEmulator>\n\
    \x20   <label>Nestopia UE</label>\n\
    </alternativeEmulator>\n\
    <gameList>\n\
    \x20   <!-- 从 ES-DE 那台机器上导出来的 -->\n\
    \x20   <game>\n\
    \x20       <path>./魂斗罗台版/魂斗罗.zip</path>\n\
    \x20       <name>魂斗罗（台版）</name>\n\
    \x20       <desc>横版卷轴射击的开山之作</desc>\n\
    \x20       <developer>Konami</developer>\n\
    \x20       <genre>射击</genre>\n\
    \x20       <players>1-2</players>\n\
    \x20       <releasedate>19880209T000000</releasedate>\n\
    \x20       <rating>0.9</rating>\n\
    \x20       <image>./media/魂斗罗.png</image>\n\
    \x20       <favorite>true</favorite>\n\
    \x20       <playcount>137</playcount>\n\
    \x20       <playtime>28800</playtime>\n\
    \x20       <lastplayed>20240115T203000</lastplayed>\n\
    \x20       <completed>true</completed>\n\
    \x20       <kidgame>false</kidgame>\n\
    \x20       <altemulator>Nestopia UE</altemulator>\n\
    \x20   </game>\n\
    \x20   <game>\n\
    \x20       <path>./我这儿没有的游戏.zip</path>\n\
    \x20       <name>别人机器上的游戏</name>\n\
    \x20       <favorite>true</favorite>\n\
    \x20       <playcount>9</playcount>\n\
    \x20   </game>\n\
    </gameList>\n";

fn 放一份分享包(现场: &现场) -> PathBuf {
    let path = 现场.包的落点();
    写(&path, 分享包.as_bytes());
    path
}

fn 导入(现场: &mut 现场, path: &Path) -> romcat_core::adapter::report::ImportReport {
    let root = 现场.dir.path().to_path_buf();
    transfer::import(
        &mut 现场.catalog,
        &Gamelist,
        std::slice::from_ref(&path.to_path_buf()),
        Some(&root),
    )
    .expect("导得进")
}

fn 导出(现场: &mut 现场) -> romcat_core::adapter::report::ExportReport {
    let out = 现场.out().to_path_buf();
    transfer::export(
        &mut 现场.catalog,
        &Gamelist,
        &Priorities::builtin(),
        &ExportOptions {
            out,
            dry_run: false,
            force: false,
        },
    )
    .expect("导得出来")
}

#[test]
fn 两个根元素的文件读得动_档位是实测出来的() {
    // ⚠️ 这份文件**技术上不是合法 XML**：`<alternativeEmulator>` 写在 `<gameList>`
    // 外面，于是有两个根元素。标准解析器直接失败——ES-DE 源码注释自己承认了这点，
    // Skyscraper 为此留了一段 workaround。
    let assertion = assert_capability(&Gamelist, 分享包.as_bytes()).expect("读得动");
    assert!(assertion.identical, "第 {:?} 处分岔", assertion.difference);
    assert_eq!(assertion.asserted, Capability::LosslessRoundTrip);
}

#[test]
fn 导入把包里的值落进中立库_而用户状态一处都不落() {
    let mut 现场 = 建现场();
    let path = 放一份分享包(&现场);
    let report = 导入(&mut 现场, &path);

    assert_eq!(report.tier, "无损往返", "实测档位");
    let file = &report.files[0];
    assert!(file.roundtrip, "{:?}", file.difference);
    assert_eq!(file.games, 2, "两个 `<game>`");
    assert_eq!(file.resolved, 1, "台版那一条对回了库里的变体");
    assert_eq!(file.unresolved, 1, "别人机器上那个我们没有");
    // **五处用户状态**：收藏、游玩次数、游玩时长、上次游玩、通关状态；
    // 加上第二段里的收藏与游玩次数，一共七处。
    assert_eq!(file.user_state, 7, "数出来了，这是「搬运」的账");
    assert_eq!(file.comments, 1, "注释在快照里");
    // `kidgame` 与 `altemulator` 中立模型没有对应概念，原样留着。
    assert!(file.unknown_keys >= 2, "{}", file.unknown_keys);

    // **原文逐字节存进了中立库。** 这是往返的全部依据。
    let 键 = romcat_core::path::display(&romcat_core::path::normalize_existing(&path));
    let stored = 现场
        .catalog
        .snapshot("ES-Gamelist", &键)
        .expect("读得出")
        .expect("有这一份");
    assert_eq!(stored.bytes, 分享包.as_bytes(), "快照是逐字节的");

    // **别人做好的元数据真的进了库。**「吸收现成元数据」就是这一步。
    let values = 现场.catalog.scraped_values("变体", 台版).expect("读得出");
    assert!(
        values
            .iter()
            .any(|value| value.source == "ES-Gamelist" && value.value == "魂斗罗（台版）"),
        "{values:#?}"
    );
    // **而用户状态一个字都没进库**（ADR-0006：工具既不读也不写）。
    let 全部值: Vec<String> = 现场
        .catalog
        .scraped_values("变体", 台版)
        .expect("读得出")
        .into_iter()
        .map(|value| format!("{}={}", value.field, value.value))
        .collect();
    for 不该有 in ["137", "28800", "20240115T203000", "true"] {
        assert!(
            !全部值.iter().any(|value| value.ends_with(不该有)),
            "用户状态不该落进中立库：{全部值:?}"
        );
    }
}

#[test]
fn 导出把用户状态逐条原样搬过去_省略就等于清零() {
    let mut 现场 = 建现场();
    let path = 放一份分享包(&现场);
    导入(&mut 现场, &path);
    let report = 导出(&mut 现场);
    assert!(report.conflicts.is_empty(), "{:#?}", report.conflicts);

    let text = fs::read_to_string(现场.包的落点()).expect("读得出");
    // ⚠️ **这几行就是这张票的全部赌注。** 少一行，维护者多年的收藏与游玩记录就没了。
    for 原样 in [
        "<favorite>true</favorite>",
        "<playcount>137</playcount>",
        "<playtime>28800</playtime>",
        "<lastplayed>20240115T203000</lastplayed>",
        "<completed>true</completed>",
    ] {
        assert!(text.contains(原样), "用户状态被清零了：{原样}\n{text}");
    }
    // 中立模型不认的元素、别的 ES 变体写的媒体路径、注释也一样不少。
    assert!(text.contains("<kidgame>false</kidgame>"), "{text}");
    assert!(
        text.contains("<altemulator>Nestopia UE</altemulator>"),
        "{text}"
    );
    assert!(text.contains("<image>./media/魂斗罗.png</image>"), "{text}");
    assert!(text.contains("从 ES-DE 那台机器上导出来的"), "{text}");
    // 两个根元素都还在。
    assert!(text.contains("<alternativeEmulator>"), "{text}");
    assert!(text.contains("<label>Nestopia UE</label>"), "{text}");
}

#[test]
fn 导出是合并不是覆盖_包里没认出来的条目一条不少() {
    let mut 现场 = 建现场();
    let path = 放一份分享包(&现场);
    导入(&mut 现场, &path);
    let report = 导出(&mut 现场);

    let text = fs::read_to_string(现场.包的落点()).expect("读得出");
    // 别人机器上那个游戏我们库里根本没有——**它不因为工具不认得就消失**，
    // 连它的收藏与游玩次数一起留着。
    assert!(text.contains("<name>别人机器上的游戏</name>"), "{text}");
    assert!(text.contains("<playcount>9</playcount>"), "{text}");
    // 而台版那一条是**合并**进原来那一段的，不是在后面又写了一条。
    assert_eq!(
        text.matches("./魂斗罗台版/魂斗罗.zip").count(),
        1,
        "同一个文件写了两遍就是没对回基线：{text}"
    );
    let file = report
        .files
        .iter()
        .find(|file| file.collection == "FC")
        .expect("有 FC 这一份");
    assert_eq!(
        file.kept_verbatim, 1,
        "原样留下来的正是别人那一条——数的是**段**不是条目"
    );
    assert!(file.roundtrip, "写出来那份自己再读一遍还能逐字节写回去");
}

#[test]
fn 落点是_gamelists_下每系统一份_路径相对系统_rom_目录() {
    let mut 现场 = 建现场();
    let report = 导出(&mut 现场);

    // 一个平台一份，全在 `gamelists/<系统>/gamelist.xml` 下。
    let mut 落点: Vec<String> = report
        .files
        .iter()
        .map(|file| {
            file.path
                .rsplit_once("gamelist")
                .map(|(head, _)| head.to_string())
                .unwrap_or_default()
        })
        .collect();
    落点.sort();
    assert!(
        落点.iter().all(|path| path.contains("gamelists/")),
        "{落点:?}"
    );
    assert!(现场.dir.path().join("gamelists/FC/gamelist.xml").is_file());
    assert!(现场.dir.path().join("gamelists/GB/gamelist.xml").is_file());

    // `<path>` 相对**系统 ROM 目录**解析（源码 `createRelativePath()`），
    // 于是平台那一段被剥掉、前导 `./` 加上。
    let fc = fs::read_to_string(现场.dir.path().join("gamelists/FC/gamelist.xml")).expect("读得出");
    assert!(fc.contains("<path>./魂斗罗台版/魂斗罗.zip</path>"), "{fc}");
    assert!(
        !fc.contains("<path>./FC/"),
        "`<path>` 里不该带平台那一段：{fc}"
    );
    assert!(!fc.contains("口袋妖怪"), "GB 的东西不该串进 FC：{fc}");
}

#[test]
fn 导出的文件再导入回来_条目对得回库里的变体() {
    // 一趟闭合：导出 → 导入。`<path>` 是相对系统 ROM 目录的，
    // [`Adapter::rom_bases`] 那一侧要把它折回中立库的键。
    let mut 现场 = 建现场();
    导出(&mut 现场);
    let path = 现场.包的落点();
    let report = 导入(&mut 现场, &path);
    let file = &report.files[0];
    assert_eq!(file.unresolved, 0, "{:?}", report.unresolved_examples);
    assert_eq!(file.resolved, 2, "FC 下两个变体都对得回去");
    assert!(file.roundtrip, "{:?}", file.difference);
}

#[test]
fn 子库的媒体按_downloaded_media_铺_条目里一个路径都不写() {
    let mut 现场 = 建现场();
    // 往**媒体池**里塞一张封面，挂到台版那个变体上。
    let bytes = b"\x89PNG\r\n\x1a\n-- fake cover --".to_vec();
    let hash = romcat_core::catalog::frontend::hash_of(&bytes);
    写(&现场.pool.path_of(&hash, "png"), &bytes);
    现场
        .catalog
        .put_media(&hash, "png", bytes.len() as u64)
        .expect("池里记得下");
    现场
        .catalog
        .put_scraped(&[Harvested {
            anchor: AnchorKind::Variant.label().to_string(),
            subject: 台版.to_string(),
            source: "本地媒体".to_string(),
            input: format!("{台版}/{hash}"),
            values: Vec::new(),
            media: vec![HarvestedMedia {
                kind: MediaKind::Cover.label().to_string(),
                hash: hash.clone(),
                evidence: "测试".to_string(),
            }],
        }])
        .expect("引用写得进");

    let selection = Selection {
        rules: vec![Rule::parse("平台=FC").expect("规则读得懂")],
        exceptions: Vec::new(),
    };
    let facts = sublibrary::facts(&现场.catalog).expect("折得出事实");
    let selected = sublibrary::select(&selection, &facts);
    let media = sync::media::lay(&现场.catalog, &Gamelist, &现场.pool, &selected).expect("铺得出");

    // **路径镜像 ROM 相对系统目录的路径，文件名是去掉扩展名的 ROM 文件名。**
    let 路径: Vec<&str> = media.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(
        路径,
        vec!["downloaded_media/FC/covers/魂斗罗台版/魂斗罗.png"],
        "ES-DE 的媒体布局与 Pegasus 的 `media/` 完全不同"
    );
    // **条目里一个媒体路径都不写**：ES-DE 靠文件名找媒体（官方原话）。
    assert!(media.assets.is_empty(), "{:#?}", media.assets);

    let frontend = sync::frontend::lay(
        &现场.catalog,
        &Gamelist,
        &Priorities::builtin(),
        &selected,
        &BTreeMap::new(),
    )
    .expect("折得出元数据");
    let 元数据: Vec<&str> = frontend
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(元数据, vec!["gamelists/FC/gamelist.xml"]);
    let text =
        String::from_utf8(frontend.bytes["gamelists/FC/gamelist.xml"].clone()).expect("UTF-8");
    assert!(!text.contains("<image>"), "媒体路径不该写进条目：{text}");
    assert!(!text.contains("downloaded_media"), "{text}");
    // 生成的这一份自己也往返得了。
    assert!(
        assert_capability(&Gamelist, text.as_bytes())
            .expect("读得动")
            .identical
    );
}

#[test]
fn 落点上有一份工具没见过的文件时不静默覆盖() {
    // 那可能就是维护者从别的机器拷过来的原件（ADR-0001 修订段）。
    let mut 现场 = 建现场();
    放一份分享包(&现场);
    let report = 导出(&mut 现场);
    assert_eq!(report.conflicts.len(), 1, "{:#?}", report.files);
    assert_eq!(
        fs::read_to_string(现场.包的落点()).expect("读得出"),
        分享包,
        "一个字节都没覆盖过去"
    );
}
