//! **取一次中文离线数据源**：下载 Bangumi Archive 的 dump，就地建成本机索引。
//!
//! ## 走的是票 06 那道取数闸门，不另开一条路
//!
//! 每一个 URL 都过 [`dat::guard`](crate::dat::guard)——那是这个库里**唯一一处**
//! 「可以连哪儿」的声明。dump 的两个落点本来就在白名单上（`raw.githubusercontent.com`
//! 与 `github.com`，后者会 302 到 `release-assets.githubusercontent.com`，而闸门
//! **看每一跳**），所以这一层一个字都不必往白名单里加。**这是有意的**：加一条白名单
//! 是一个需要想清楚的决定，不该由一张新数据源清单顺手带进来。
//!
//! ## 增量：指纹没变就整件跳过
//!
//! dump 每周三导出一次，而这份索引一建就是几个月不用动。指纹取的是
//! `aux/latest.json` 里那个 `digest`（官方算好的 sha256）加文件名——**用官方给的那个数
//! 而不是自己算**：自己算要先把 435 MB 下回来，而增量的全部意义正是不下那 435 MB。
//!
//! **这个指纹只盯着远端那份 dump**，盯不住本机这两张表（平台清单与剥离规则）。
//! 补一条平台别名之后再跑一趟不带 `--full` 的 `zh sync`，会如实报「本机这份就是最新的」
//! 而整件跳过——dump 确实没换。要让新补的那条别名生效**得走 `--full`**
//! （[`Options::full`]）。这不是遗漏：本机那两张表变没变，不下那 435 MB 是问不出来的，
//! 而拿「那两张表现在长什么样」当依据，会在「改了别名但还没重建」时反过来说谎。
//!
//! ## 折出来的那张表跟着索引一起落库
//!
//! 建索引时每条条目的平台名都折成本工具的平台名（`entry_of` → [`platform_of`]），
//! 折出来的那一串决定交叉校验。**折叠发生在建索引那一刻**，所以「这一趟折出来了什么」
//! 要跟着索引一起记进库（[`zh::PlatformFold`](crate::zh::PlatformFold)），
//! 由刮削那一侧当输入指纹使。少了它，补一条别名重建之后刮削会整片复用旧的采集记录，
//! 新折得动的条目永远不产出。
//!
//! ## 一次 435 MB 的下载，一次 960 MB 的流
//!
//! zip 里那份 `subject.jsonlines` 解开是 960 MB，**绝不整份读进内存**：
//! [`container::read_entries`](crate::container::read_entries) 交出来的是一条流，
//! 这里一行一行读过去，只把游戏条目留下来（8.7 万条，几十 MB）。
//! 本机只剩几个 GiB，而这条链路上任何一处「先读进内存再说」都会当场撑爆。

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::container::{self, Demand, ReadPlan};
use crate::dat::fetch::{FetchError, Fetcher};
use crate::filename::Rules;
use crate::fs::LibraryFs;
use crate::platform::Manifest;

use super::store::{Stats, Store, StoreError};
use super::{Entry, PlatformFold, dump};

/// 最新那一版 dump 的地址索引。官方仓库里的一个文件，几百字节。
pub const LATEST_URL: &str =
    "https://raw.githubusercontent.com/bangumi/Archive/master/aux/latest.json";

/// dump 里那份条目表叫什么。
pub const SUBJECTS: &str = "subject.jsonlines";

/// 取数跑不下去的原因。
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// 取数失败（含闸门拦下）。
    #[error(transparent)]
    Fetch(#[from] FetchError),
    /// 索引读写失败。
    #[error(transparent)]
    Store(#[from] StoreError),
    /// 下回来的东西不是预期的样子。
    #[error("{0}")]
    Malformed(String),
    /// 本地文件读写失败。
    #[error("读写 {path} 失败：{source}")]
    Io {
        /// 出问题的路径。
        path: String,
        /// 底层错误。
        source: std::io::Error,
    },
}

/// 取数的选项。
#[derive(Debug, Clone)]
pub struct Options {
    /// 取回来的原件放哪。**留着原件**：改一条平台别名之后重建索引，不必把那 435 MB
    /// 再下一遍（与 `dat::cache_dir` 同一条道理）。
    pub cache: PathBuf,
    /// 无视指纹，整份**重建索引**。
    ///
    /// **它不等于「重下」**：原件的文件名里带着这一版的日期（`dump-2026-09-01.…zip`），
    /// 同名就是同一版，手边有就直接用。改一条平台别名之后要重建索引，走的正是这条——
    /// 那时再下一遍 415 MB 纯属白花。
    ///
    /// **它也是那条别名唯一的生效途径**：不带 `--full` 的那一趟只比远端那份 dump 的
    /// 指纹，本机这两张表变没变它一个字都不知道（见模块文档「增量」那一节）。
    /// 重建之后刮削那一侧会跟着重采——折出来的那张表进了输入指纹
    /// （[`zh::PlatformFold`](crate::zh::PlatformFold)），而不是像从前那样整片跳过。
    pub full: bool,
    /// 只说这一趟会干什么，不取也不写。
    pub dry_run: bool,
}

/// 一趟取数干了什么。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Synced {
    /// 这一版 dump 叫什么。
    pub dump: String,
    /// 它的指纹。
    pub fingerprint: String,
    /// 这一版有多大。
    pub bytes: u64,
    /// **指纹没变，整件跳过了**。
    pub skipped: bool,
    /// 只排了计划没真干。
    pub dry_run: bool,
    /// 从 dump 里读了多少条记录。
    pub records: u64,
    /// 其中留下来的游戏条目。
    pub games: u64,
    /// 索引建完之后长什么样。
    pub stats: Stats,
}

/// 取一次。
///
/// `library` 只用来读**本机缓存目录里那个 zip**——它不是主库，但读法一模一样，
/// 所以复用同一个只读接缝（`LibraryFs`）而不是另开一套文件读取。
///
/// # Errors
/// 取数、解析或写索引失败时返回错误。
pub fn sync(
    fetcher: &dyn Fetcher,
    library: &dyn LibraryFs,
    store: &mut Store,
    manifest: &Manifest,
    rules: &Rules,
    options: &Options,
) -> Result<Synced, SyncError> {
    let latest = fetcher.get(LATEST_URL)?;
    let text = String::from_utf8_lossy(&latest.body).into_owned();
    let release = Release::parse(&text)?;
    let mut out = Synced {
        dump: release.name.clone(),
        fingerprint: release.fingerprint(),
        bytes: release.size,
        dry_run: options.dry_run,
        ..Synced::default()
    };
    // **等着重建的那一份不许走「指纹没变就整件跳过」这条路。** 它的指纹确实没变——
    // 变的是本程序，而库里那份数据这一版读不了。跳过就等于让它永远读不了，
    // 而这条路本来是留给「本机这份就是最新的」的。
    let pending = store.rebuilding().is_some();
    if !options.full && !pending && store.fingerprint()? == Some(out.fingerprint.clone()) {
        out.skipped = true;
        out.stats = store.stats()?;
        return Ok(out);
    }
    if options.dry_run {
        out.stats = store.stats()?;
        return Ok(out);
    }
    std::fs::create_dir_all(&options.cache).map_err(|source| SyncError::Io {
        path: crate::path::display(&options.cache),
        source,
    })?;
    let file = options.cache.join(&release.name);
    // **原件已经在手边就不再下一遍**，`--full` 也不例外：文件名里带着这一版的日期，
    // 同名就是同一版。改一条平台别名重建索引时省的正是这 415 MB。
    if !file.exists() {
        fetcher.download(&release.url, &file)?;
    }
    let (entries, records, fold) = read_dump(library, &file, manifest, rules)?;
    out.records = records;
    out.games = u64::try_from(entries.len()).unwrap_or(u64::MAX);
    store.replace(&entries, &release.name, &out.fingerprint, &fold)?;
    out.stats = store.stats()?;
    Ok(out)
}

/// `aux/latest.json` 里那几样。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Release {
    name: String,
    url: String,
    digest: String,
    size: u64,
}

impl Release {
    fn parse(text: &str) -> Result<Self, SyncError> {
        let value: serde_json::Value = serde_json::from_str(text)
            .map_err(|error| SyncError::Malformed(format!("{LATEST_URL} 不是 JSON：{error}")))?;
        let field = |key: &str| -> Result<String, SyncError> {
            value
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
                .ok_or_else(|| SyncError::Malformed(format!("{LATEST_URL} 里没有 {key}")))
        };
        Ok(Self {
            name: field("name")?,
            url: field("browser_download_url")?,
            digest: field("digest").unwrap_or_default(),
            size: value
                .get("size")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
        })
    }

    /// 变没变的依据。**用官方算好的那个 sha256**——自己算要先把 435 MB 下回来，
    /// 而增量的全部意义正是不下那 435 MB。
    fn fingerprint(&self) -> String {
        format!("{}|{}", self.name, self.digest)
    }
}

/// 把 dump 里那份条目表流一遍，留下游戏条目。
///
/// 交出来的第三样是**这一趟平台折叠实际折出来的那张表**（[`PlatformFold`]）：它得跟着
/// 索引一起落库，刮削那一侧拿它当输入指纹。攒在这儿而不是事后从条目上倒推——
/// 条目身上只剩折完的那一串与原文拼成的一行（`Entry::platform_text`），
/// 哪个原文折出了哪个平台名已经看不出来了。
fn read_dump(
    library: &dyn LibraryFs,
    file: &Path,
    manifest: &Manifest,
    rules: &Rules,
) -> Result<(Vec<Entry>, u64, PlatformFold), SyncError> {
    let listing = container::list(library, file).map_err(|error| {
        SyncError::Malformed(format!("{} 读不动：{error}", crate::path::display(file)))
    })?;
    if !listing
        .contents
        .entries
        .iter()
        .any(|entry| entry.path == SUBJECTS)
    {
        return Err(SyncError::Malformed(format!(
            "{} 里没有 {SUBJECTS}——这不是一份 Bangumi Archive 的 dump",
            crate::path::display(file)
        )));
    }
    let plan = ReadPlan::new(&listing, |entry| {
        if entry.path == SUBJECTS {
            Demand::All
        } else {
            Demand::Skip
        }
    });
    let mut entries: Vec<Entry> = Vec::new();
    let mut records = 0u64;
    let mut fold = PlatformFold::default();
    container::read_entries(library, file, &listing, &plan, &mut |_entry, reader| {
        // **一行一行读**：解开是 960 MB，整份读进内存会当场撑爆本机剩下的那几个 GiB。
        let mut lines = BufReader::with_capacity(1 << 20, reader);
        let mut line = String::new();
        loop {
            line.clear();
            if lines.read_line(&mut line)? == 0 {
                break;
            }
            records += 1;
            let Ok(row) = serde_json::from_str::<dump::Row>(line.trim_end()) else {
                // 一行读不动就跳过这一行。**整份 dump 不该为一行畸形的 JSON 作废**，
                // 而它确实可能出现——那是一份用户共同维护的 wiki 导出来的东西。
                continue;
            };
            if !row.is_game() {
                continue;
            }
            entries.push(entry_of(&row, manifest, rules, &mut fold));
        }
        Ok(())
    })
    .map_err(|error| SyncError::Malformed(format!("{SUBJECTS} 读不动：{error}")))?;
    Ok((entries, records, fold))
}

/// 把一条记录折成索引里的一条。
///
/// 折出来的每一对都记进 `fold`——**那是这份索引与「当时那两张表」之间唯一留得下来的
/// 凭据**，刮削那一侧靠它认出「补过别名了，这些条目该重采」。`platform_of` 折不动的
/// 一个都不记（[`PlatformFold`] 的文档说了为什么够用、为什么有界）。
fn entry_of(
    row: &dump::Row,
    manifest: &Manifest,
    rules: &Rules,
    fold: &mut PlatformFold,
) -> Entry {
    let raw = row.platforms();
    let mut platforms: Vec<String> = Vec::new();
    for text in &raw {
        if let Some(platform) = platform_of(manifest, rules, text) {
            // **去重之前就记**：同一条条目里写了两遍的那一次也是一次实实在在的折叠，
            // 而这张表本来就按对去重。
            fold.record(text, platform);
            if !platforms.iter().any(|it| it == platform) {
                platforms.push(platform.to_string());
            }
        }
    }
    Entry {
        id: row.id,
        name: row.name.trim().to_string(),
        name_cn: row.name_cn.trim().to_string(),
        aliases: row.aliases(),
        year: row.year(),
        platforms,
        platform_text: raw.join("、"),
        // **简介一个字都不改**：换行、开头那两个全角空格、以及数据源自带的排版都是内容的
        // 一部分（规格 18）。连两头都不掐——`　　以细腻的画风…` 那两个全角空格是排版，
        // `trim` 会把它们当空白扫掉。整段只有空白时才算没有。
        summary: if row.summary.trim().is_empty() {
            String::new()
        } else {
            row.summary.clone()
        },
        genres: row.genres(),
        developers: row.developers(),
        publishers: row.publishers(),
    }
}

/// **从本机那份原件重建索引**：一个网络请求都不发，也不要用户重下那 435 MB。
///
/// 结构版本一变就走这条（[`Store::rebuilding`]）。原件就是取数时留在缓存目录里的那个
/// zip——文件名里带着这一版的日期，**同名就是同一版**（`Options::full` 的文档说的是
/// 同一件事）。重建完把原来那个指纹记回去，下一趟 `zh sync` 才认得出「本机这份就是
/// 最新的」而整件跳过。
///
/// 三种结局都不是错误：不必重建、原件不在手边（那时才轮到用户跑一趟 `zh sync`）、
/// 重建了。
///
/// # Errors
/// 原件读不动或者写索引失败时返回错误。
pub fn rebuild(
    library: &dyn LibraryFs,
    store: &mut Store,
    manifest: &Manifest,
    rules: &Rules,
    cache: &Path,
) -> Result<Rebuilt, SyncError> {
    let Some(pending) = store.rebuilding().cloned() else {
        return Ok(Rebuilt::NotNeeded);
    };
    let file = cache.join(&pending.dump);
    if pending.dump.is_empty() || !file.exists() {
        return Ok(Rebuilt::NoOriginal {
            was: pending.was,
            dump: pending.dump,
        });
    }
    let (entries, _, fold) = read_dump(library, &file, manifest, rules)?;
    let games = u64::try_from(entries.len()).unwrap_or(u64::MAX);
    store.replace(&entries, &pending.dump, &pending.fingerprint, &fold)?;
    Ok(Rebuilt::Done {
        was: pending.was,
        dump: pending.dump,
        games,
    })
}

/// 一趟重建的结局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rebuilt {
    /// 结构版本对得上，什么都没做。
    NotNeeded,
    /// 该重建，但**本机没有那份原件**——只有这一种结局才轮到用户跑一趟 `zh sync`。
    NoOriginal {
        /// 旧索引的结构版本。
        was: u32,
        /// 缺的是哪一份原件；上一版索引连 dump 名都没记时是空串。
        dump: String,
    },
    /// 从本机那份原件重建好了。
    Done {
        /// 旧索引的结构版本。
        was: u32,
        /// 从哪一份原件重建的。
        dump: String,
        /// 重建出多少条游戏条目。
        games: u64,
    },
}

/// 中文数据源写的那个平台名，在本工具里叫什么。
///
/// **先问平台清单再问剥离规则**：`GBA` / `NDS` / `PSP` 两边写法一模一样，平台清单
/// 那张 `目录` 别名表就折得动；剥离规则里那张表只补它折不动的那些
/// （`Nintendo Switch`、`PlayStation Vita`）。两张表的分工写在
/// [`filename` 的规则文件](crate::filename::Rules::BUILTIN)里。
#[must_use]
pub fn platform_of<'a>(manifest: &'a Manifest, rules: &'a Rules, text: &str) -> Option<&'a str> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(platform) = manifest.platform_for_dir(trimmed) {
        return Some(platform.name.as_str());
    }
    // 去掉空格与连字符再问一遍：`Wii U` 与 `wiiu`、`Mega Drive` 与 `megadrive`。
    let squeezed = crate::filename::fold_platform(trimmed);
    if !squeezed.is_empty()
        && let Some(platform) = manifest.platform_for_dir(&squeezed)
    {
        return Some(platform.name.as_str());
    }
    rules.platform_alias(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_json_读得出下载地址与指纹() {
        // 这一段是 2026-09-02 实测取回来的那一份，原样。
        let text = r#"{
  "browser_download_url": "https://github.com/bangumi/Archive/releases/download/archive/dump-2026-09-01.210329Z.zip",
  "digest": "sha256:5d02c90e8317c47f17f912b614f25c7611cd08297bf428450ebf7a00a9821a39",
  "name": "dump-2026-09-01.210329Z.zip",
  "size": 435891841
}"#;
        let release = Release::parse(text).expect("读得出来");
        assert_eq!(release.name, "dump-2026-09-01.210329Z.zip");
        assert_eq!(release.size, 435_891_841);
        assert!(release.fingerprint().contains("5d02c90e"));
        // 下载地址在闸门的白名单上——这一层一个字都不必往白名单里加。
        assert!(crate::dat::guard::check(&release.url).is_ok());
        assert!(crate::dat::guard::check(LATEST_URL).is_ok());
    }

    #[test]
    fn 结构版本一变就从本机那份原件重建_不下载不报错() {
        // 规格 16：「存储结构升级时自己从本地那份原件重建，这样我不必重新下载 435 MB。」
        // 这条测试里**没有 fetcher**——重建这条路上根本没有发请求的地方。
        let dir = crate::testing::temp_dir("zh-rebuild");
        let cache = dir.path().join("cache");
        std::fs::create_dir_all(&cache).expect("建得出缓存目录");
        std::fs::write(cache.join(原件名), 一份原件()).expect("写得下原件");

        let path = dir.path().join("zh.sqlite3");
        let manifest = Manifest::builtin();
        let rules = Rules::builtin();
        let library = crate::fs::RealFs;
        {
            let mut store = Store::open(&path).expect("开得起来");
            let (entries, records, fold) =
                read_dump(&library, &cache.join(原件名), &manifest, &rules).expect("原件读得动");
            assert_eq!(records, 1, "读到几条记录");
            assert_eq!(entries.len(), 1, "留下几条游戏条目");
            store
                .replace(&entries, 原件名, 指纹, &fold)
                .expect("写得进去");
        }
        // 把版本改回上一格：这就是「拿一份旧结构版本的索引打开」。
        let conn = rusqlite::Connection::open(&path).expect("开得起来");
        conn.execute(
            "UPDATE meta SET value = '1' WHERE key = 'schema_version'",
            [],
        )
        .expect("改得动");
        drop(conn);

        let mut store = Store::open(&path).expect("旧版本照样打得开");
        assert!(store.load().expect("读得回来").is_empty(), "先扫干净");
        let outcome = rebuild(&library, &mut store, &manifest, &rules, &cache).expect("重建得了");
        assert_eq!(
            outcome,
            Rebuilt::Done {
                was: 1,
                dump: 原件名.to_string(),
                games: 1,
            }
        );
        // 重建完索引又满了，而且带上了新结构才有的那几样。
        let index = store.load().expect("读得回来");
        assert_eq!(index.len(), 1);
        assert_eq!(index.entries()[0].genres, vec!["ACT"]);
        // 简介不进内存（`Store::load` 的文档），单独一条路读得出来。
        assert_eq!(
            store.summary(4).expect("读得回来").as_deref(),
            Some("　　以细腻的画风…")
        );
        // **指纹记回去了**：下一趟 `zh sync` 才认得出「本机这份就是最新的」而整件跳过。
        assert_eq!(
            store.fingerprint().expect("读得到").as_deref(),
            Some(指纹)
        );
        assert_eq!(store.rebuilding(), None, "重建完就不再等着重建了");
    }

    #[test]
    fn 等着重建的那一份不许因为指纹没变而整件跳过() {
        // 指纹确实没变——变的是本程序，而库里那份数据这一版读不了。跳过就等于让它
        // 永远读不了，而那条路本来是留给「本机这份就是最新的」的。
        let dir = crate::testing::temp_dir("zh-sync-pending");
        let cache = dir.path().join("cache");
        std::fs::create_dir_all(&cache).expect("建得出缓存目录");
        std::fs::write(cache.join(原件名), 一份原件()).expect("写得下原件");

        let path = dir.path().join("zh.sqlite3");
        {
            let mut store = Store::open(&path).expect("开得起来");
            store
                .replace(&[], 原件名, 指纹, &PlatformFold::default())
                .expect("写得进去");
        }
        let conn = rusqlite::Connection::open(&path).expect("开得起来");
        conn.execute(
            "UPDATE meta SET value = '1' WHERE key = 'schema_version'",
            [],
        )
        .expect("改得动");
        drop(conn);

        let mut store = Store::open(&path).expect("旧版本照样打得开");
        assert!(store.rebuilding().is_some());
        assert_eq!(store.fingerprint().expect("读得到").as_deref(), Some(指纹));

        let fetcher = crate::dat::CannedFetcher::new().with(LATEST_URL, 一份_latest_json());
        let outcome = sync(
            &fetcher,
            &crate::fs::RealFs,
            &mut store,
            &Manifest::builtin(),
            &Rules::builtin(),
            &Options {
                cache,
                full: false,
                dry_run: false,
            },
        )
        .expect("取得动");
        assert!(!outcome.skipped, "指纹一样也不许跳过——那份索引这一版读不了");
        assert_eq!(outcome.games, 1);
        // **原件在手边就没下载**：`CannedFetcher` 根本没备那个下载地址，
        // 真去下会当场失败。
        assert_eq!(store.load().expect("读得回来").len(), 1);
        assert!(store.rebuilding().is_none());
    }

    #[test]
    fn 本机没有那份原件时如实说一句而不是硬报错() {
        let dir = crate::testing::temp_dir("zh-rebuild-missing");
        let path = dir.path().join("zh.sqlite3");
        {
            let mut store = Store::open(&path).expect("开得起来");
            store
                .replace(&[], "dump-2026-09-01.210329Z.zip", "sha256:abc", &PlatformFold::default())
                .expect("写得进去");
        }
        let conn = rusqlite::Connection::open(&path).expect("开得起来");
        conn.execute(
            "UPDATE meta SET value = '1' WHERE key = 'schema_version'",
            [],
        )
        .expect("改得动");
        drop(conn);
        let mut store = Store::open(&path).expect("旧版本照样打得开");
        let outcome = rebuild(
            &crate::fs::RealFs,
            &mut store,
            &Manifest::builtin(),
            &Rules::builtin(),
            dir.path(),
        )
        .expect("不是错误");
        assert_eq!(
            outcome,
            Rebuilt::NoOriginal {
                was: 1,
                dump: "dump-2026-09-01.210329Z.zip".to_string(),
            }
        );
    }

    /// 缓存目录里那份原件叫什么。文件名里带着这一版的日期，**同名就是同一版**。
    const 原件名: &str = "dump-2026-09-01.210329Z.zip";

    /// 那一版的指纹，形状与 [`Release::fingerprint`] 一致。
    const 指纹: &str = "dump-2026-09-01.210329Z.zip|sha256:abc";

    /// `aux/latest.json` 说的正是本机手上这一版。
    fn 一份_latest_json() -> Vec<u8> {
        format!(
            "{{\"browser_download_url\": \
             \"https://github.com/bangumi/Archive/releases/download/archive/{原件名}\",\
             \"digest\": \"sha256:abc\", \"name\": \"{原件名}\", \"size\": 1}}"
        )
        .into_bytes()
    }

    /// 一份最小的 dump：一个 zip，里头一份 `subject.jsonlines`，一条游戏记录。
    fn 一份原件() -> Vec<u8> {
        let line = r#"{"id":4,"type":4,"name":"メタルスラッグ7","name_cn":"合金弹头7","infobox":"{{Infobox Game\r\n|别名={\r\n[Metal Slug 7]\r\n}\r\n|平台= NDS\r\n|游戏类型= ACT\r\n|开发= SNK\r\n|发行= 世嘉\r\n}}","platform":4001,"summary":"　　以细腻的画风…","date":"2008-07-17","meta_tags":["ACT","NDS","游戏"]}"#;
        crate::testing::container::zip_container(&[
            crate::testing::container::ZipEntrySpec::stored(
                SUBJECTS,
                format!("{line}
").into_bytes(),
            ),
        ])
    }

    #[test]
    fn 平台名先问平台清单再问剥离规则() {
        let manifest = Manifest::builtin();
        let rules = Rules::builtin();
        // 平台清单那张目录别名表折得动的。
        assert_eq!(platform_of(&manifest, &rules, "GBA"), Some("GBA"));
        assert_eq!(platform_of(&manifest, &rules, "NDS"), Some("NDS"));
        // 去掉空格之后折得动的。
        assert_eq!(platform_of(&manifest, &rules, "Wii U"), Some("WIIU"));
        // 只有剥离规则那张表折得动的。
        assert_eq!(
            platform_of(&manifest, &rules, "Nintendo Switch"),
            Some("SWITCH")
        );
        assert_eq!(platform_of(&manifest, &rules, "PS Vita"), Some("PSV"));
        // 两张表都不认的一律留空——**认不出就留空**，硬折一个平台出来会让交叉校验说谎。
        assert_eq!(platform_of(&manifest, &rules, "iOS"), None);
        assert_eq!(platform_of(&manifest, &rules, ""), None);
    }

    /// 一份最小的 dump，平台那一格写什么由调用方定。
    fn 一份原件写着(平台: &str) -> Vec<u8> {
        let line = format!(
            r#"{{"id":4,"type":4,"name":"メタルスラッグ7","name_cn":"合金弹头7","infobox":"{{{{Infobox Game\r\n|平台= {平台}\r\n|游戏类型= ACT\r\n}}}}","platform":4001,"date":"2008-07-17","meta_tags":["ACT","游戏"]}}"#
        );
        crate::testing::container::zip_container(&[crate::testing::container::ZipEntrySpec::stored(
            SUBJECTS,
            format!("{line}\n").into_bytes(),
        )])
    }

    #[test]
    fn 建索引时折出来的那张表跟着索引一起落库() {
        // 折叠发生在**建索引那一刻**，用的是那一刻的平台清单与别名表。所以「这一趟
        // 折出来了什么」得记进库——本机那两张表事后再变，也改不了库里这一份是怎么折的。
        let dir = crate::testing::temp_dir("zh-fold-meta");
        let cache = dir.path().join("cache");
        std::fs::create_dir_all(&cache).expect("建得出缓存目录");
        std::fs::write(cache.join(原件名), 一份原件写着("NDS")).expect("写得下原件");

        let mut store = Store::open(&dir.path().join("zh.sqlite3")).expect("开得起来");
        let (entries, _, fold) = read_dump(
            &crate::fs::RealFs,
            &cache.join(原件名),
            &Manifest::builtin(),
            &Rules::builtin(),
        )
        .expect("原件读得动");
        assert_eq!(fold.line(), "共 1 对\nNDS=NDS", "折出来的那一对记下了");
        store
            .replace(&entries, 原件名, 指纹, &fold)
            .expect("写得进去");

        // 落进 `meta`，而且**索引读回来时带在身上**——刮削那一侧拿它当输入指纹。
        assert_eq!(
            store
                .meta(crate::zh::store::PLATFORM_FOLD)
                .expect("读得到"),
            Some("共 1 对\nNDS=NDS".to_string())
        );
        assert_eq!(
            store.load().expect("读得回来").platform_fold(),
            "共 1 对\nNDS=NDS"
        );
    }

    #[test]
    fn 补一条平台别名重建之后折出来的那张表跟着变() {
        // 用户的原话。`--full` 是那条别名唯一的生效途径（`Options::full` 的文档），
        // 而重建之后刮削那一侧认得出「这份索引不是原来那份」，靠的就是这张表变了。
        let dir = crate::testing::temp_dir("zh-fold-alias");
        let cache = dir.path().join("cache");
        std::fs::create_dir_all(&cache).expect("建得出缓存目录");
        // 两张表都不认的一个平台名。
        std::fs::write(cache.join(原件名), 一份原件写着("任天堂红白机")).expect("写得下原件");
        let manifest = Manifest::builtin();
        let 读一遍 = |rules: &Rules| {
            read_dump(&crate::fs::RealFs, &cache.join(原件名), &manifest, rules).expect("读得动")
        };

        let (折不动的条目, _, 折不动) = 读一遍(&Rules::builtin());
        assert!(折不动的条目[0].platforms.is_empty(), "折不动");
        assert!(折不动.is_empty());
        assert_eq!(折不动.line(), "共 0 对");

        let 规则文件 = dir.path().join("rules.toml");
        std::fs::write(
            &规则文件,
            "\"版本\" = 1\n[[\"中文源平台别名\"]]\n\"叫\" = \"任天堂红白机\"\n\"是\" = \"FC\"\n",
        )
        .expect("写得下规则");
        let (折得动的条目, _, 折得动) = 读一遍(&Rules::load(&规则文件).expect("读得进来"));
        assert_eq!(折得动的条目[0].platforms, vec!["FC".to_string()]);
        assert_eq!(折得动.line(), "共 1 对\n任天堂红白机=FC");
        assert_ne!(折不动.line(), 折得动.line(), "两趟折出来的表不一样");
    }

    #[test]
    fn 与这份_dump_无关的那些别名不进折出来的那张表() {
        // **多盖会误伤。** 平台清单里加一个这份 dump 里根本没人写的平台、或者补一条
        // 谁都没用上的别名，折出来的东西一个字都没变，就不该引发全片重采——
        // 盖的是**折叠真发生了什么**，不是那两张表现在长什么样。
        let dir = crate::testing::temp_dir("zh-fold-unrelated");
        let cache = dir.path().join("cache");
        std::fs::create_dir_all(&cache).expect("建得出缓存目录");
        std::fs::write(cache.join(原件名), 一份原件写着("NDS")).expect("写得下原件");
        let manifest = Manifest::builtin();
        let 读一遍 = |rules: &Rules| {
            read_dump(&crate::fs::RealFs, &cache.join(原件名), &manifest, rules)
                .expect("读得动")
                .2
        };

        let 规则文件 = dir.path().join("rules.toml");
        std::fs::write(
            &规则文件,
            "\"版本\" = 1\n[[\"中文源平台别名\"]]\n\"叫\" = \"世嘉土星\"\n\"是\" = \"SS\"\n",
        )
        .expect("写得下规则");
        assert_eq!(
            读一遍(&Rules::builtin()).line(),
            读一遍(&Rules::load(&规则文件).expect("读得进来")).line(),
            "这份 dump 里没人写「世嘉土星」，补它不改变这一趟折出来的东西"
        );
    }
}
