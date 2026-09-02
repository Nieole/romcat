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
use super::{Entry, dump};

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
    if !options.full && store.fingerprint()? == Some(out.fingerprint.clone()) {
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
    let (entries, records) = read_dump(library, &file, manifest, rules)?;
    out.records = records;
    out.games = u64::try_from(entries.len()).unwrap_or(u64::MAX);
    store.replace(&entries, &release.name, &out.fingerprint)?;
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
fn read_dump(
    library: &dyn LibraryFs,
    file: &Path,
    manifest: &Manifest,
    rules: &Rules,
) -> Result<(Vec<Entry>, u64), SyncError> {
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
            entries.push(entry_of(&row, manifest, rules));
        }
        Ok(())
    })
    .map_err(|error| SyncError::Malformed(format!("{SUBJECTS} 读不动：{error}")))?;
    Ok((entries, records))
}

/// 把一条记录折成索引里的一条。
fn entry_of(row: &dump::Row, manifest: &Manifest, rules: &Rules) -> Entry {
    let raw = row.platforms();
    let mut platforms: Vec<String> = Vec::new();
    for text in &raw {
        if let Some(platform) = platform_of(manifest, rules, text)
            && !platforms.iter().any(|it| it == platform)
        {
            platforms.push(platform.to_string());
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
    }
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
}
