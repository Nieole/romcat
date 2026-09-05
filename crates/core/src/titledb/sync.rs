//! **取一次 titledb**：下载那几个 JSON，就地建成本机索引。
//!
//! ## 走的是票 06 那道取数闸门，不另开一条路
//!
//! 每一个 URL 都过 [`dat::guard`](crate::dat::guard)——那是这个库里**唯一一处**「可以连
//! 哪儿」的声明。`raw.githubusercontent.com` 本来就在白名单上，所以这一层**一个字都不必
//! 往白名单里加**（同[中文离线源](crate::zh::sync)）。
//!
//! **绝不 `git clone`**：那个仓库约 44 GB，而要的只有几个文件（调研的实现陷阱第 5 条）。
//!
//! ## 增量：`ETag` 没变就整件跳过
//!
//! titledb 每日推送，而这几个文件加起来两三百 MB。`raw.githubusercontent.com` 给
//! `ETag`，于是一次 `HEAD` 就问得出「变了没有」——**按文件各记一个**，四个区里只有一个
//! 变了就只重下那一个。
//!
//! ## 原件留着
//!
//! 与 DAT 和中文 dump 同一条道理（`dat_cache_dir` 的文档）：改一版折算规则之后重建索引，
//! 不必把那两三百 MB 再下一遍。`--full` 是**重建索引**不是**重下**。

use std::io::BufReader;
use std::path::{Path, PathBuf};

use crate::dat::fetch::{FetchError, Fetcher};

use super::store::{FETCHED_AT, Stats, Store, StoreError};
use super::{Content, Title};

/// titledb 的单文件落点。**只走 raw，不走 API、更不 clone。**
pub const RAW: &str = "https://raw.githubusercontent.com/blawar/titledb/master/";

/// 反查那张表从哪个文件来。
pub const CNMTS: &str = "cnmts.json";

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
    /// 取回来的原件放哪。
    pub cache: PathBuf,
    /// 无视 `ETag`，整份**重建索引**。原件在手边就不重下。
    pub full: bool,
    /// 只说这一趟会干什么，不取也不写。
    pub dry_run: bool,
    /// 连 eShop 元数据（名字、发行商、语言）一起取吗。
    ///
    /// 分开一个开关，是因为**两者的价钱差一个数量级**：`cnmts.json` 一份 50 MB 就够
    /// 反查出 (TitleID, 版本)，而四个区的元数据加起来两三百 MB。只想让识别跑起来的人
    /// 不必付后面那笔。
    pub regions: bool,
}

/// 一个文件这一趟怎么了。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Fetched {
    /// 文件名。
    pub file: String,
    /// 它的 `ETag`。
    pub etag: String,
    /// **指纹没变，整件跳过了**。
    pub skipped: bool,
    /// 这一趟真的下了一遍（原件不在手边或者变了）。
    pub downloaded: bool,
    /// 从里面读出多少条。
    pub rows: u64,
}

/// 一趟取数干了什么。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Synced {
    /// 逐个文件的账。
    pub files: Vec<Fetched>,
    /// 只排了计划没真干。
    pub dry_run: bool,
    /// 索引建完之后长什么样。
    pub stats: Stats,
}

/// 取一次。
///
/// # Errors
/// 取数、解析或写索引失败时返回错误。
pub fn sync(
    fetcher: &dyn Fetcher,
    store: &mut Store,
    options: &Options,
) -> Result<Synced, SyncError> {
    let mut out = Synced {
        dry_run: options.dry_run,
        ..Synced::default()
    };
    let mut wanted: Vec<(&str, Option<&str>)> = vec![(CNMTS, None)];
    if options.regions {
        wanted.extend(
            super::REGIONS
                .iter()
                .map(|(file, region)| (*file, Some(*region))),
        );
    }
    for (file, region) in wanted {
        let url = format!("{RAW}{file}");
        let head = fetcher.head(&url)?;
        let etag = head
            .get("etag")
            .map(ToString::to_string)
            .unwrap_or_default();
        let key = format!("etag:{file}");
        let seen = store.meta(&key)?;
        let path = options.cache.join(file);
        let unchanged = !etag.is_empty() && seen.as_deref() == Some(etag.as_str());
        let mut record = Fetched {
            file: file.to_string(),
            etag: etag.clone(),
            skipped: unchanged && !options.full,
            downloaded: false,
            rows: 0,
        };
        if record.skipped || options.dry_run {
            out.files.push(record);
            continue;
        }
        std::fs::create_dir_all(&options.cache).map_err(|source| SyncError::Io {
            path: crate::path::display(&options.cache),
            source,
        })?;
        // **原件在手边而且没变就不再下一遍**，`--full` 也不例外：`--full` 是重建索引，
        // 不是重下（同 `zh::sync`）。
        if !path.exists() || !unchanged {
            fetcher.download(&url, &path)?;
            record.downloaded = true;
        }
        record.rows = match region {
            None => {
                let rows = read_cnmts(&path)?;
                let count = u64::try_from(rows.len()).unwrap_or(u64::MAX);
                store.replace_ncas(&rows)?;
                count
            }
            Some(region) => {
                let rows = read_region(&path)?;
                let count = u64::try_from(rows.len()).unwrap_or(u64::MAX);
                store.replace_region(region, &rows)?;
                count
            }
        };
        if !etag.is_empty() {
            store.put_meta(&key, &etag)?;
        }
        out.files.push(record);
    }
    // **取回的时刻**：库里此前只留 ETag，那答得出「变没变」却答不出「多久没取了」，
    // 而界面上那一行问的正是后者。
    if !options.dry_run {
        store.put_meta(FETCHED_AT, &crate::catalog::now_secs().to_string())?;
    }
    out.stats = store.stats()?;
    Ok(out)
}

/// `cnmts.json` 里一个版本下挂着的东西。**只留要的那一样。**
#[derive(serde::Deserialize)]
struct CnmtRow {
    #[serde(rename = "contentEntries", default)]
    content_entries: Option<Vec<ContentEntry>>,
}

#[derive(serde::Deserialize)]
struct ContentEntry {
    #[serde(rename = "ncaId")]
    nca_id: Option<String>,
}

/// 读 `cnmts.json`，折成 ContentId → (TitleID, 版本)。
///
/// ⚠ **两套 ContentType 枚举**（调研的实现陷阱第 2 条）：`cnmts.json` 用的是 NCM 的
/// （0=Meta,1=Program…），`ncas.json` 用的是 NCA 头的（0=Program,1=Meta…），顺序不同。
/// 这一层**一个都不用**——反查只要 `ncaId`，哪一种内容都照收：容器里读到的任意一个
/// ContentId 都该查得出同一个 (titleId, version)。
fn read_cnmts(path: &Path) -> Result<Vec<(String, Content)>, SyncError> {
    let file = std::fs::File::open(path).map_err(|source| SyncError::Io {
        path: crate::path::display(path),
        source,
    })?;
    let parsed: std::collections::BTreeMap<String, std::collections::BTreeMap<String, CnmtRow>> =
        serde_json::from_reader(BufReader::with_capacity(1 << 20, file)).map_err(|error| {
            SyncError::Malformed(format!(
                "{} 不是 titledb 的 cnmts.json：{error}",
                crate::path::display(path)
            ))
        })?;
    let mut out = Vec::new();
    for (title_id, versions) in parsed {
        for (version, row) in versions {
            let Ok(version) = version.parse::<u32>() else {
                continue;
            };
            for entry in row.content_entries.into_iter().flatten() {
                let Some(nca_id) = entry.nca_id else { continue };
                out.push((
                    nca_id.to_ascii_lowercase(),
                    Content {
                        title_id: title_id.to_ascii_uppercase(),
                        version,
                    },
                ));
            }
        }
    }
    Ok(out)
}

/// `{区}.{语}.json` 里的一条。**只留要的那几样。**
#[derive(serde::Deserialize)]
struct RegionRow {
    id: Option<String>,
    name: Option<String>,
    publisher: Option<String>,
    #[serde(default)]
    languages: Option<Vec<String>>,
}

/// 读一个区的 eShop 元数据。
///
/// 键是 `nsuId`（一个区一个），而**同一个 TitleID 在一个区里可能有好几条 `nsuId`**
/// （捆绑包、重新上架）。这里按 TitleID 收口，**先来的赢**：同一个区里两条同 TitleID
/// 的记录，名字与语言几乎总是一样的，而让后来的覆盖前面的只会让两次取数得到不同的库。
fn read_region(path: &Path) -> Result<Vec<Title>, SyncError> {
    let file = std::fs::File::open(path).map_err(|source| SyncError::Io {
        path: crate::path::display(path),
        source,
    })?;
    let parsed: std::collections::BTreeMap<String, RegionRow> =
        serde_json::from_reader(BufReader::with_capacity(1 << 20, file)).map_err(|error| {
            SyncError::Malformed(format!(
                "{} 不是 titledb 的区域文件：{error}",
                crate::path::display(path)
            ))
        })?;
    let mut seen: std::collections::BTreeMap<String, Title> = std::collections::BTreeMap::new();
    for row in parsed.into_values() {
        let (Some(id), Some(name)) = (row.id, row.name) else {
            continue;
        };
        let title_id = id.to_ascii_uppercase();
        seen.entry(title_id.clone()).or_insert_with(|| Title {
            title_id,
            name,
            publisher: row.publisher,
            languages: row.languages.as_deref().and_then(super::languages_of),
            regions: Vec::new(),
        });
    }
    Ok(seen.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dat::fetch::CannedFetcher;

    fn 写下(dir: &Path, name: &str, body: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn cnmts_折成_content_id_到游戏与版本() {
        let dir = std::env::temp_dir().join("romcat-titledb-cnmts");
        let path = 写下(
            &dir,
            CNMTS,
            r#"{"0100a0c01bed8800":{"196608":{"contentEntries":[
                 {"ncaId":"150CF9022BFB2E72527669F2701EE31B","type":1},
                 {"ncaId":"dfdb0f5bc5c5056a2f35d0379ee23020"}],"titleType":129}}}"#,
        );
        let rows = read_cnmts(&path).unwrap();
        assert_eq!(rows.len(), 2);
        // ContentId 一律折成小写（容器里的文件名就是小写），TitleID 一律大写。
        assert_eq!(rows[0].0, "150cf9022bfb2e72527669f2701ee31b");
        assert_eq!(rows[0].1.title_id, "0100A0C01BED8800");
        assert_eq!(rows[0].1.version, 196_608);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn 区域文件按_title_id_收口先来的赢() {
        let dir = std::env::temp_dir().join("romcat-titledb-region");
        let path = 写下(
            &dir,
            "HK.zh.json",
            r#"{"70010000000001":{"id":"0100A0C01BED8000","name":"伊蘇X",
                 "publisher":"Falcom","languages":["zh","zh","ja"]},
                "70010000000002":{"id":"0100a0c01bed8000","name":"伊蘇X 重新上架",
                 "languages":["zh"]},
                "70010000000003":{"name":"没有 id 的一条"}}"#,
        );
        let rows = read_region(&path).unwrap();
        assert_eq!(rows.len(), 1, "同 TitleID 收成一条，没有 id 的丢掉");
        assert_eq!(rows[0].title_id, "0100A0C01BED8000");
        assert_eq!(
            rows[0].languages.as_deref(),
            Some("Zh,Ja"),
            "繁简都写成 zh，去重"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn etag_没变就整件跳过() {
        let dir = std::env::temp_dir().join("romcat-titledb-etag");
        std::fs::remove_dir_all(&dir).ok();
        let body = r#"{"0100a0c01bed8800":{"0":{"contentEntries":[{"ncaId":"aa"}]}}}"#;
        let canned = CannedFetcher::new().with_headers(
            &format!("{RAW}{CNMTS}"),
            &[("etag", "\"abc\"")],
            body.as_bytes(),
        );
        let mut store = Store::in_memory().unwrap();
        let options = Options {
            cache: dir.clone(),
            full: false,
            dry_run: false,
            regions: false,
        };
        let first = sync(&canned, &mut store, &options).unwrap();
        assert!(first.files[0].downloaded);
        assert_eq!(first.files[0].rows, 1);
        let again = sync(&canned, &mut store, &options).unwrap();
        assert!(again.files[0].skipped, "ETag 没变，整件跳过");
        assert!(!again.files[0].downloaded);
        std::fs::remove_dir_all(&dir).ok();
    }
}
