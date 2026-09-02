//! **中文条目索引**落在本机的那份 SQLite。
//!
//! 与 DAT 库分开一份文件，理由和当初把 DAT 库与中立库分开时一模一样
//! （`dat::repo` 的模块文档）：
//!
//! - **它与哪个主库无关。** 「世上有哪些游戏、中文叫什么」对两块盘是同一份。
//! - **删库重扫不该赔上一次 435 MB 的下载。** 中立库的结构版本一变就让用户删库重扫，
//!   那是几分钟的事；重下一份 dump 不是。
//! - **更新节奏不同**：dump 每周三导出一次，扫描按盘走。
//!
//! 这份库里**没有一行是攒出来的**——全部内容都能从 dump 重建，所以结构版本对不上时
//! 直接重建（同 `dat::repo::SCHEMA_VERSION`）。

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use super::{Entry, Index, NameKind};

/// 索引的结构版本。结构变了就加 1；读到对不上的版本直接重建。
pub const SCHEMA_VERSION: u32 = 1;

const SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS meta(
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

-- 一条中文条目。`platforms` 是**折成本工具平台名**之后的那一串，两头带逗号
-- （`,GBA,NDS,`）——与 `content_cart.family` 同一个写法，为的是 SQL 里能用 `instr`
-- 做整词匹配，不至于 `GB` 匹配上 `GBA`。
-- `platform_text` 是数据源原样写的那一串，**依据里要写它**：人去核对时看的是原文。
CREATE TABLE IF NOT EXISTS subject(
    id            INTEGER PRIMARY KEY,
    name          TEXT NOT NULL,
    name_cn       TEXT NOT NULL,
    year          INTEGER,
    platforms     TEXT NOT NULL,
    platform_text TEXT NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS subject_year ON subject(year);

-- 一条**叫法**。原名、中文名与别名各占一行——匹配撞的是叫法不是条目。
CREATE TABLE IF NOT EXISTS subject_name(
    subject INTEGER NOT NULL,
    kind    TEXT    NOT NULL,
    value   TEXT    NOT NULL,
    PRIMARY KEY (subject, kind, value)
) STRICT;
";

/// 索引读写出错。
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// 目录建不出来。
    #[error("中文索引的目录建不出来：{path}（{source}）")]
    Io {
        /// 出问题的路径。
        path: String,
        /// 底层错误。
        source: std::io::Error,
    },
    /// 底层读写失败。
    #[error("中文索引读写失败：{path}（{source}）")]
    Sqlite {
        /// 索引文件。
        path: String,
        /// 底层错误。
        source: rusqlite::Error,
    },
    /// 结构版本对不上。
    #[error(
        "中文索引 {path} 的结构版本是 {found}，本程序认得的是 {expected}。\
         删掉它重新跑一次 `romcat zh sync` 即可——这份库里没有攒出来的东西，全部内容都能重建"
    )]
    Version {
        /// 索引文件。
        path: String,
        /// 文件里的版本。
        found: u32,
        /// 本程序的版本。
        expected: u32,
    },
}

/// 本机那份中文条目索引。
#[derive(Debug)]
pub struct Store {
    conn: Connection,
    path: PathBuf,
}

/// 索引里现在有什么。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Stats {
    /// 这份索引是从哪一版 dump 建的。
    pub dump: String,
    /// 什么时候建的（Unix 秒）。
    pub built_at: i64,
    /// 一共几条条目。
    pub subjects: u64,
    /// 其中有中文名的。**这一列才是这份数据源的价值**。
    pub with_chinese: u64,
    /// 其中折得出本工具平台名的。
    pub with_platform: u64,
    /// 其中说得出年份的。
    pub with_year: u64,
    /// 按平台：平台名与条目数。**老平台深度浅是事实不是缺陷**，报告要摆出来。
    pub by_platform: Vec<(String, u64)>,
}

impl Store {
    /// 打开（必要时新建）本机那份索引。
    ///
    /// # Errors
    /// 目录建不出来、库打不开、或者结构版本对不上时返回错误。
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|source| StoreError::Io {
                path: crate::path::display(parent),
                source,
            })?;
        }
        let conn = Connection::open(path).map_err(|source| StoreError::Sqlite {
            path: crate::path::display(path),
            source,
        })?;
        let store = Self {
            conn,
            path: path.to_path_buf(),
        };
        store.prepare()?;
        Ok(store)
    }

    /// 完全在内存里开一份。测试用。
    ///
    /// # Errors
    /// 建不出来时返回错误。
    pub fn in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory().map_err(|source| StoreError::Sqlite {
            path: ":memory:".to_string(),
            source,
        })?;
        let store = Self {
            conn,
            path: PathBuf::from(":memory:"),
        };
        store.prepare()?;
        Ok(store)
    }

    fn prepare(&self) -> Result<(), StoreError> {
        self.conn
            .execute_batch(&format!("PRAGMA journal_mode=WAL;\n{SCHEMA}"))
            .map_err(|source| self.error(source))?;
        let found: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|source| self.error(source))?;
        match found.as_deref().and_then(|text| text.parse::<u32>().ok()) {
            Some(version) if version == SCHEMA_VERSION => Ok(()),
            Some(version) => Err(StoreError::Version {
                path: crate::path::display(&self.path),
                found: version,
                expected: SCHEMA_VERSION,
            }),
            None => {
                self.put_meta("schema_version", &SCHEMA_VERSION.to_string())?;
                Ok(())
            }
        }
    }

    fn error(&self, source: rusqlite::Error) -> StoreError {
        StoreError::Sqlite {
            path: crate::path::display(&self.path),
            source,
        }
    }

    fn put_meta(&self, key: &str, value: &str) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT INTO meta(key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map(|_| ())
            .map_err(|source| self.error(source))
    }

    /// 读一条元信息。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn meta(&self, key: &str) -> Result<Option<String>, StoreError> {
        self.conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|source| self.error(source))
    }

    /// 上一次取回来的那个 dump 的指纹（文件名加 sha256）。增量靠它。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn fingerprint(&self) -> Result<Option<String>, StoreError> {
        self.meta("fingerprint")
    }

    /// **整份换掉**：旧的条目连同叫法一起删，新的整批写进去。
    ///
    /// 逐条比对差异在这里毫无意义——dump 本来就是整份重新导出的（同 `dat::repo`）。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn replace(
        &mut self,
        entries: &[Entry],
        dump: &str,
        fingerprint: &str,
    ) -> Result<(), StoreError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|it| i64::try_from(it.as_secs()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        let tx = self
            .conn
            .transaction()
            .map_err(|source| StoreError::Sqlite {
                path: crate::path::display(&self.path),
                source,
            })?;
        tx.execute_batch("DELETE FROM subject_name; DELETE FROM subject;")
            .map_err(|source| StoreError::Sqlite {
                path: crate::path::display(&self.path),
                source,
            })?;
        {
            let mut subject = tx
                .prepare(
                    "INSERT OR REPLACE INTO subject(id, name, name_cn, year, platforms, platform_text)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .map_err(|source| StoreError::Sqlite {
                    path: crate::path::display(&self.path),
                    source,
                })?;
            let mut name = tx
                .prepare(
                    "INSERT OR IGNORE INTO subject_name(subject, kind, value) VALUES (?1, ?2, ?3)",
                )
                .map_err(|source| StoreError::Sqlite {
                    path: crate::path::display(&self.path),
                    source,
                })?;
            for entry in entries {
                subject
                    .execute(params![
                        entry.id,
                        entry.name,
                        entry.name_cn,
                        entry.year,
                        fold_platforms(&entry.platforms),
                        entry.platform_text,
                    ])
                    .map_err(|source| StoreError::Sqlite {
                        path: crate::path::display(&self.path),
                        source,
                    })?;
                for alias in &entry.aliases {
                    name.execute(params![entry.id, NameKind::Alias.code(), alias])
                        .map_err(|source| StoreError::Sqlite {
                            path: crate::path::display(&self.path),
                            source,
                        })?;
                }
            }
        }
        tx.commit().map_err(|source| StoreError::Sqlite {
            path: crate::path::display(&self.path),
            source,
        })?;
        self.put_meta("dump", dump)?;
        self.put_meta("fingerprint", fingerprint)?;
        self.put_meta("built_at", &now.to_string())?;
        Ok(())
    }

    /// 把整份索引装进内存。**匹配走内存不走 SQL**：一次匹配要看上千条叫法，
    /// 几万个变体就是几千万次查询，那不是 SQLite 该干的活。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn load(&self) -> Result<Index, StoreError> {
        let mut aliases: std::collections::BTreeMap<u32, Vec<String>> =
            std::collections::BTreeMap::new();
        let mut statement = self
            .conn
            .prepare("SELECT subject, value FROM subject_name ORDER BY subject, value")
            .map_err(|source| self.error(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|source| self.error(source))?;
        for row in rows {
            let (subject, value) = row.map_err(|source| self.error(source))?;
            aliases.entry(subject).or_default().push(value);
        }
        let mut statement = self
            .conn
            .prepare(
                "SELECT id, name, name_cn, year, platforms, platform_text FROM subject ORDER BY id",
            )
            .map_err(|source| self.error(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok(Entry {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    name_cn: row.get(2)?,
                    aliases: Vec::new(),
                    year: row.get(3)?,
                    platforms: split_platforms(&row.get::<_, String>(4)?),
                    platform_text: row.get(5)?,
                })
            })
            .map_err(|source| self.error(source))?;
        let mut entries = Vec::new();
        for row in rows {
            let mut entry = row.map_err(|source| self.error(source))?;
            entry.aliases = aliases.remove(&entry.id).unwrap_or_default();
            entries.push(entry);
        }
        let dump = self.meta("dump")?.unwrap_or_default();
        Ok(Index::build(entries, dump))
    }

    /// 索引里现在有什么。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn stats(&self) -> Result<Stats, StoreError> {
        let mut stats = Stats {
            dump: self.meta("dump")?.unwrap_or_default(),
            built_at: self
                .meta("built_at")?
                .and_then(|it| it.parse().ok())
                .unwrap_or(0),
            ..Stats::default()
        };
        let counts = self
            .conn
            .query_row(
                "SELECT COUNT(*),
                        SUM(CASE WHEN name_cn <> '' THEN 1 ELSE 0 END),
                        SUM(CASE WHEN platforms <> '' THEN 1 ELSE 0 END),
                        SUM(CASE WHEN year IS NOT NULL THEN 1 ELSE 0 END)
                 FROM subject",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                        row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                        row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                    ))
                },
            )
            .map_err(|source| self.error(source))?;
        stats.subjects = u64::try_from(counts.0).unwrap_or(0);
        stats.with_chinese = u64::try_from(counts.1).unwrap_or(0);
        stats.with_platform = u64::try_from(counts.2).unwrap_or(0);
        stats.with_year = u64::try_from(counts.3).unwrap_or(0);
        let mut by_platform: std::collections::BTreeMap<String, u64> =
            std::collections::BTreeMap::new();
        let mut statement = self
            .conn
            .prepare("SELECT platforms FROM subject WHERE platforms <> ''")
            .map_err(|source| self.error(source))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|source| self.error(source))?;
        for row in rows {
            for platform in split_platforms(&row.map_err(|source| self.error(source))?) {
                *by_platform.entry(platform).or_insert(0) += 1;
            }
        }
        stats.by_platform = by_platform.into_iter().collect();
        stats
            .by_platform
            .sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        Ok(stats)
    }
}

/// 平台列表折成 `,GBA,NDS,`；一个都没有时是空串。
fn fold_platforms(platforms: &[String]) -> String {
    if platforms.is_empty() {
        return String::new();
    }
    format!(",{},", platforms.join(","))
}

/// 把 `,GBA,NDS,` 拆回来。
fn split_platforms(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|it| !it.is_empty())
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 一条() -> Entry {
        Entry {
            id: 4,
            name: "メタルスラッグ7".to_string(),
            name_cn: "合金弹头7".to_string(),
            aliases: vec!["Metal Slug 7".to_string()],
            year: Some(2008),
            platforms: vec!["NDS".to_string()],
            platform_text: "NDS".to_string(),
        }
    }

    #[test]
    fn 写进去再读回来是同一条() {
        let mut store = Store::in_memory().expect("开得起来");
        store
            .replace(&[一条()], "dump-2026-09-01", "sha256:abc")
            .expect("写得进去");
        let index = store.load().expect("读得回来");
        assert_eq!(index.len(), 1);
        assert_eq!(index.entries()[0], 一条());
        assert_eq!(index.dump(), "dump-2026-09-01");
        assert_eq!(
            store.fingerprint().expect("读得到").as_deref(),
            Some("sha256:abc")
        );
    }

    #[test]
    fn 整份换掉不留旧条目() {
        // dump 是整份重新导出的，逐条比对差异毫无意义。
        let mut store = Store::in_memory().expect("开得起来");
        store.replace(&[一条()], "旧", "旧指纹").expect("写得进去");
        let mut another = 一条();
        another.id = 99;
        store.replace(&[another], "新", "新指纹").expect("写得进去");
        let index = store.load().expect("读得回来");
        assert_eq!(index.len(), 1);
        assert_eq!(index.entries()[0].id, 99);
    }

    #[test]
    fn 统计数得出中文名与平台的覆盖() {
        let mut store = Store::in_memory().expect("开得起来");
        let mut 没中文名 = 一条();
        没中文名.id = 5;
        没中文名.name_cn = String::new();
        没中文名.platforms = Vec::new();
        没中文名.year = None;
        store
            .replace(&[一条(), 没中文名], "dump", "指纹")
            .expect("写得进去");
        let stats = store.stats().expect("数得出来");
        assert_eq!(stats.subjects, 2);
        assert_eq!(stats.with_chinese, 1);
        assert_eq!(stats.with_platform, 1);
        assert_eq!(stats.with_year, 1);
        assert_eq!(stats.by_platform, vec![("NDS".to_string(), 1)]);
    }
}
