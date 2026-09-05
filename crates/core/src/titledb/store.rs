//! **TitleID 索引**落在本机的那份 SQLite。
//!
//! 与 DAT 库、中文索引各自一份文件，理由同源（`dat::repo` 与 `zh::store` 的模块文档）：
//!
//! - **它与哪个主库无关。** 「世上有哪些 Switch 游戏、哪个 ContentId 属于哪个版本」
//!   对两块盘是同一份。
//! - **删库重扫不该赔上一次几百 MB 的下载。**
//! - **更新节奏不同**：titledb 每日推送，扫描按盘走。
//!
//! 这份库里**没有一行是攒出来的**——全部内容都能从 titledb 重建，所以结构版本对不上时
//! 直接重建（同 `dat::repo::SCHEMA_VERSION`）。
//!
//! ## 查得起，所以不装进内存
//!
//! 与[中文索引](crate::zh::store::Store::load)反过来：那一边一次匹配要看上千条叫法，
//! 几万个变体就是几千万次查询，只能整份装进内存；**这一边是等值查主键**，真机上
//! Switch 只有 91 个变体、几百次查询，SQLite 一次几微秒。为它把 173,502 行装进内存
//! 是白花的常驻内存。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use super::{Content, Title};

/// 索引的结构版本。结构变了就加 1；读到对不上的版本直接重建。
pub const SCHEMA_VERSION: u32 = 1;

/// `meta` 里记「上次取回是什么时候」的那把键。
pub const FETCHED_AT: &str = "fetched_at";

/// 撞上写锁时**等多久**（毫秒）。
///
/// **不是调优，是那一屏的前提**（口径同 `Catalog::open` 那条注释）：这份库的 `open`
/// 有两个调用方——取回那条后台线程（它写），与 `sources::survey`（它只读，但眼下
/// 就跑在**画帧线程**上）。撞上就报错的话，那一屏会把「忙」记成**不可读**
/// ——而 ADR-0021 说的第三态是「元数据读不到」，不是「等一下就好」，两件事混一起，
/// 这一格会一直挂到取回结束才刷新。
///
/// **明写出来，是因为不写也有一个数，而那个数不是谁挑的**：`rusqlite` 的
/// `Connection::open` 自己塞了 5 秒（`inner_connection.rs` 里那句
/// `sqlite3_busy_timeout(db, 5000)`）。界面那一屏靠一个第三方库的默认值撑着，
/// 它改版就没了，而且没有一处说得出为什么是 5 秒。
///
/// **取 3 秒而不是中立库那 10 秒。** 中立库那 10 秒等在后台线程上，这一份可能等在
/// 画帧线程上，等 10 秒等于把界面挂死。真正的争用窗口是「第一次取回时两边同时建这份
/// 空库」那一下，亚毫秒级；3 秒已经比它大三个数量级。（把 `survey` 挪出画帧线程是
/// 另一条活，不在这儿解，但也别把它弄得更糟。）
const BUSY_TIMEOUT_MS: u32 = 3_000;

const SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS meta(
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

-- **ContentId → (TitleID, 版本)**。主键就是 ContentId：调研实测 173,502 个 ncaId
-- 100.00% 只属于唯一一个 (titleId, version)，零冲突——所以这张表天然是一对一的。
-- ContentId 一律**小写 hex**（容器里的文件名就是小写），TitleID 一律**大写**。
CREATE TABLE IF NOT EXISTS nca(
    content_id TEXT PRIMARY KEY,
    title_id   TEXT    NOT NULL,
    version    INTEGER NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS nca_title ON nca(title_id);

-- 一个 TitleID 在**某一个区**的 eShop 上的样子。
--
-- 主键带着 `region` 而不是只有 `title_id`，装的正是 ADR-0019 那道世代裂缝：同一个
-- TitleID 在几个区各有一行，**行数本身就是「这是不是一次多区共用的发行」的答案**
-- （港服与美服实测 69.4% 共用）。压成一行就再也数不出来了。
CREATE TABLE IF NOT EXISTS title(
    title_id  TEXT NOT NULL,
    region    TEXT NOT NULL,
    name      TEXT NOT NULL,
    publisher TEXT,
    -- 折成 No-Intro 那种写法的语言串（`En,Ja,Zh`）。
    languages TEXT,
    PRIMARY KEY (title_id, region)
) STRICT;
";

/// 索引读写出错。
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// 目录建不出来。
    #[error("TitleID 索引的目录建不出来：{path}（{source}）")]
    Io {
        /// 出问题的路径。
        path: String,
        /// 底层错误。
        source: std::io::Error,
    },
    /// 底层读写失败。
    #[error("TitleID 索引读写失败：{path}（{source}）")]
    Sqlite {
        /// 索引文件。
        path: String,
        /// 底层错误。
        source: rusqlite::Error,
    },
    /// 结构版本对不上。
    #[error(
        "TitleID 索引 {path} 的结构版本是 {found}，本程序认得的是 {expected}。\
         删掉它重新跑一次 `romcat switch sync` 即可——这份库里没有攒出来的东西，\
         全部内容都能重建"
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

/// 本机那份 TitleID 索引。
#[derive(Debug)]
pub struct Store {
    conn: Connection,
    path: PathBuf,
}

/// 索引里现在有什么。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Stats {
    /// 一共几条 ContentId → (TitleID, 版本)。
    pub ncas: u64,
    /// 涉及几个不同的 TitleID。
    pub titles: u64,
    /// eShop 元数据一共几行（同一个 TitleID 在几个区就是几行）。
    pub entries: u64,
    /// 有 eShop 元数据的 TitleID 数。
    pub named: u64,
    /// **官中**：语言里带 `Zh` 的 TitleID 数。这一列才是这份数据源对本项目的价值。
    pub chinese: u64,
    /// 其中**多区共用**同一个 TitleID 的（中文只是语言属性，ADR-0019）。
    pub chinese_shared: u64,
    /// 按区：区名与条目数。
    pub by_region: Vec<(String, u64)>,
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

    /// 完全在内存里开一份。测试用，也是「用户还没取过 titledb」时的那一份空索引。
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
        // **这条余量要排在最前面**，后面每一句才等得起。
        self.conn
            .execute_batch(&format!("PRAGMA busy_timeout = {BUSY_TIMEOUT_MS};"))
            .map_err(|source| self.error(source))?;
        // **偏偏转日志模式这一句不认忙等待**：它要的是独占，走的不是忙等待那条路
        // ——实测把余量设成 300 毫秒、另一份连接占着写锁，这一句 337 微秒就当场
        // `SQLITE_BUSY`，而紧跟着的 `CREATE TABLE` 老老实实等满了 300 毫秒。
        // **它才是那句「database is locked」真正的来路**：`survey` 与取回线程同时开一份
        // 刚建出来的空库，两边都想把它转成 WAL，输的那一边当场报错。
        // **撞上就放过**——能撞上只有这一种情形，而 WAL 记在库文件头里，谁转成了所有
        // 连接都按 WAL 走，这一份不必去争。已经是 WAL 的库上它本来就是空操作
        // （实测 2.4 微秒，写锁占着也不报忙）。
        if let Err(source) = self.conn.execute_batch("PRAGMA journal_mode=WAL;")
            && source.sqlite_error_code() != Some(rusqlite::ErrorCode::DatabaseBusy)
        {
            return Err(self.error(source));
        }
        self.conn
            .execute_batch(SCHEMA)
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

    /// 索引文件在哪。报告里说得出来。
    #[must_use]
    pub fn location(&self) -> String {
        crate::path::display(&self.path)
    }

    /// 写一条元信息。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put_meta(&self, key: &str, value: &str) -> Result<(), StoreError> {
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

    /// **整份换掉**那张 ContentId 表。
    ///
    /// 逐条比对差异在这里毫无意义——titledb 本来就是整份重新生成的（同 `dat::repo`）。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn replace_ncas(&mut self, rows: &[(String, Content)]) -> Result<(), StoreError> {
        let path = crate::path::display(&self.path);
        let failed = |source: rusqlite::Error| StoreError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(failed)?;
        tx.execute_batch("DELETE FROM nca;").map_err(failed)?;
        {
            let mut insert = tx
                .prepare(
                    "INSERT OR REPLACE INTO nca(content_id, title_id, version) VALUES (?1, ?2, ?3)",
                )
                .map_err(failed)?;
            for (content_id, content) in rows {
                insert
                    .execute(params![content_id, content.title_id, content.version])
                    .map_err(failed)?;
            }
        }
        tx.commit().map_err(failed)
    }

    /// **整份换掉**某一个区的 eShop 元数据。
    ///
    /// 按区换而不是整张表换：四个区各下一次，一个区取不回来不该把另外三个也清空。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn replace_region(&mut self, region: &str, rows: &[Title]) -> Result<(), StoreError> {
        let path = crate::path::display(&self.path);
        let failed = |source: rusqlite::Error| StoreError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(failed)?;
        tx.execute("DELETE FROM title WHERE region = ?1", params![region])
            .map_err(failed)?;
        {
            let mut insert = tx
                .prepare(
                    "INSERT OR REPLACE INTO title(title_id, region, name, publisher, languages)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .map_err(failed)?;
            for row in rows {
                insert
                    .execute(params![
                        row.title_id,
                        region,
                        row.name,
                        row.publisher,
                        row.languages,
                    ])
                    .map_err(failed)?;
            }
        }
        tx.commit().map_err(failed)
    }

    /// 上次取回是什么时候（UNIX 纪元起的秒）；从没取过时是 `None`。
    ///
    /// # Errors
    /// 读不出来时返回错误。
    pub fn fetched_at(&self) -> Result<Option<i64>, StoreError> {
        Ok(self.meta(FETCHED_AT)?.and_then(|text| text.parse().ok()))
    }

    /// 这份索引里有东西吗。**空的时候 Switch 那一层照样跑**——只是走不到「查得出版本」
    /// 那一档（`identify::switch` 的模块文档）。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn ready(&self) -> Result<bool, StoreError> {
        let count: i64 = self
            .conn
            .query_row("SELECT count(*) FROM nca", [], |row| row.get(0))
            .map_err(|source| self.error(source))?;
        Ok(count > 0)
    }

    /// ⭐ **反查**：一个 ContentId 属于哪个游戏的哪个版本。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn content(&self, content_id: &str) -> Result<Option<Content>, StoreError> {
        self.conn
            .query_row(
                "SELECT title_id, version FROM nca WHERE content_id = ?1",
                params![content_id.to_ascii_lowercase()],
                |row| {
                    Ok(Content {
                        title_id: row.get(0)?,
                        version: row.get(1)?,
                    })
                },
            )
            .optional()
            .map_err(|source| self.error(source))
    }

    /// 一个 TitleID 的 eShop 元数据。**几个区的合成一条**，地区那一栏由
    /// [`Title::region`] 按出现的区数定（ADR-0019）。
    ///
    /// 名字取**第一个说得出来的区**，顺序照 [`REGIONS`](super::REGIONS)——英文名排在
    /// 最前，因为它是下游折**作品**名时最稳的一个（中文译名归**标题集合**那一层去挑，
    /// 不在这儿抢）。
    ///
    /// ⭐ **语言取几个区的并集，不能只取一个区的。** 调研统计的「官中覆盖 8,447 个
    /// TitleID」正是 **US ∪ HK 去重**之后的数——同一个 TitleID 在美服的 `languages`
    /// 写 `En,Ja`、在港服写 `Zh,Ja` 是常态，只取排最前那个区，中文就没了，而
    /// **中文正是这份数据源对本项目的全部价值**（ADR-0019）。真机实测：并集之后
    /// 官中 9,141 个 TitleID，其中 5,971 个是多区共用的。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn title(&self, title_id: &str) -> Result<Option<Title>, StoreError> {
        let wanted = title_id.to_ascii_uppercase();
        let mut statement = self
            .conn
            .prepare(
                "SELECT region, name, publisher, languages FROM title
                 WHERE title_id = ?1 ORDER BY region",
            )
            .map_err(|source| self.error(source))?;
        let rows = statement
            .query_map(params![wanted], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(|source| self.error(source))?;
        let mut regions: BTreeSet<String> = BTreeSet::new();
        let mut found: Vec<(usize, String, Option<String>, Option<String>)> = Vec::new();
        for row in rows {
            let (region, name, publisher, languages) = row.map_err(|source| self.error(source))?;
            let rank = super::REGIONS
                .iter()
                .position(|(_, known)| *known == region)
                .unwrap_or(usize::MAX);
            found.push((rank, name, publisher, languages));
            regions.insert(region);
        }
        // **按 `REGIONS` 那个顺序排**，不按区名的字典序：名字取排最前那个区的，
        // 语言按同一个顺序并起来。两处共用一个次序，同一份索引查两次结果才一样。
        found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let mut codes: Vec<String> = Vec::new();
        for (_, _, _, languages) in &found {
            // 语言取并集：同一个 TitleID 在美服写 `En,Ja`、在港服写 `Zh,Ja` 是常态。
            for code in languages.iter().flat_map(|it| it.split(',')) {
                let code = code.trim();
                if !code.is_empty() && !codes.iter().any(|seen| seen == code) {
                    codes.push(code.to_string());
                }
            }
        }
        let Some((_, name, publisher, _)) = found.into_iter().next() else {
            return Ok(None);
        };
        Ok(Some(Title {
            title_id: wanted,
            name,
            publisher,
            languages: (!codes.is_empty()).then(|| codes.join(",")),
            regions: regions.into_iter().collect(),
        }))
    }

    /// 索引里现在有什么。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn stats(&self) -> Result<Stats, StoreError> {
        let one = |sql: &str| -> Result<u64, StoreError> {
            self.conn
                .query_row(sql, [], |row| row.get::<_, i64>(0))
                .map(|it| u64::try_from(it).unwrap_or(0))
                .map_err(|source| self.error(source))
        };
        let mut stats = Stats {
            ncas: one("SELECT count(*) FROM nca")?,
            titles: one("SELECT count(DISTINCT title_id) FROM nca")?,
            entries: one("SELECT count(*) FROM title")?,
            named: one("SELECT count(DISTINCT title_id) FROM title")?,
            chinese: one(
                "SELECT count(DISTINCT title_id) FROM title WHERE instr(languages, 'Zh') > 0",
            )?,
            chinese_shared: one("SELECT count(*) FROM (
                     SELECT title_id FROM title WHERE instr(languages, 'Zh') > 0
                     GROUP BY title_id HAVING count(*) > 1)")?,
            by_region: Vec::new(),
        };
        let mut statement = self
            .conn
            .prepare("SELECT region, count(*) FROM title GROUP BY region ORDER BY 2 DESC, 1")
            .map_err(|source| self.error(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|source| self.error(source))?;
        for row in rows {
            let (region, count) = row.map_err(|source| self.error(source))?;
            stats
                .by_region
                .push((region, u64::try_from(count).unwrap_or(0)));
        }
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 反查一个_content_id_得到游戏与版本() {
        let mut store = Store::in_memory().unwrap();
        assert!(!store.ready().unwrap(), "还没取过就是空的");
        store
            .replace_ncas(&[(
                "150cf9022bfb2e72527669f2701ee31b".to_string(),
                Content {
                    title_id: "0100A0C01BED8800".to_string(),
                    version: 196_608,
                },
            )])
            .unwrap();
        assert!(store.ready().unwrap());
        // 容器里的文件名是小写，查表大小写不敏感。
        let found = store
            .content("150CF9022BFB2E72527669F2701EE31B")
            .unwrap()
            .unwrap();
        assert_eq!(found.title_id, "0100A0C01BED8800");
        assert_eq!(found.version, 196_608);
        assert_eq!(store.content("0000").unwrap(), None);
    }

    #[test]
    fn 同一个_title_id_出现在几个区就合成一条多区共用的发行() {
        // ⭐ ADR-0019：港服与美服共用同一个 TitleID，中文是语言属性不是独立发行版。
        let mut store = Store::in_memory().unwrap();
        let 一条 = |name: &str, languages: &str| Title {
            title_id: "0100A0C01BED8000".to_string(),
            name: name.to_string(),
            publisher: Some("Falcom".to_string()),
            languages: Some(languages.to_string()),
            regions: Vec::new(),
        };
        store
            .replace_region("Hong Kong", &[一条("伊蘇X", "Zh,Ja")])
            .unwrap();
        store
            .replace_region("USA", &[一条("Ys X - Nordics", "En,Ja")])
            .unwrap();
        let found = store.title("0100a0c01bed8000").unwrap().unwrap();
        assert_eq!(found.regions, vec!["Hong Kong", "USA"]);
        assert_eq!(found.region(), Some("World"), "多区共用");
        assert_eq!(found.name, "Ys X - Nordics", "名字取排在最前的那个区");
        // ⭐ **语言取并集**：只取排最前那个区（美服 `En,Ja`），中文就没了——
        // 而中文正是这份数据源对本项目的全部价值（ADR-0019）。
        assert_eq!(found.languages.as_deref(), Some("En,Ja,Zh"));
        assert_eq!(found.entry_name(), "Ys X - Nordics (World) (En,Ja,Zh)");
    }

    #[test]
    fn 按区整份换掉_别的区不受影响() {
        let mut store = Store::in_memory().unwrap();
        let 一条 = |id: &str, region: &str| Title {
            title_id: id.to_string(),
            name: format!("{region} 的名字"),
            publisher: None,
            languages: Some("Zh".to_string()),
            regions: Vec::new(),
        };
        store
            .replace_region("China", &[一条("0100000000000001", "China")])
            .unwrap();
        store
            .replace_region("USA", &[一条("0100000000000002", "USA")])
            .unwrap();
        store
            .replace_region("USA", &[一条("0100000000000003", "USA")])
            .unwrap();
        let stats = store.stats().unwrap();
        assert_eq!(stats.entries, 2, "USA 换掉了，China 留着");
        assert_eq!(stats.chinese, 2);
        assert_eq!(stats.chinese_shared, 0, "各只在一个区出现");
        assert!(store.title("0100000000000002").unwrap().is_none());
    }

    #[test]
    fn switch_库的连接设了等锁的余量() {
        let dir = crate::testing::temp_dir("titledb-store-timeout");
        let store = Store::open(&dir.path().join("titledb.sqlite3")).expect("开得起来");
        let 余量: i64 = store
            .conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .expect("读得回来");
        assert_eq!(余量, i64::from(BUSY_TIMEOUT_MS), "设进去的读得回来");
    }

    /// 拿一份**刚建出来的空库**当现场：第二份连接 `BEGIN IMMEDIATE` 占住写锁。
    ///
    /// 这正是界面上「第一次取某个数据源」那一瞬间的形状——取回线程刚把文件建出来，
    /// 画帧线程那一侧的 `sources::survey` 同时开同一份空库。两边都要把它转成 WAL，
    /// 而**转日志模式那一句不认忙等待**，输的那一边当场 `SQLITE_BUSY`，于是那一屏把它
    /// 记成**不可读**（ADR-0021 的第三态）——而它其实只是忙。
    ///
    /// **钉不成 flaky**：断言只说「等得到、开得出来」，锁放得早放得晚它都成立。
    /// 中间那一小段停顿不参与判定，只是让**没设余量**的旧代码必定撞上那一下。
    #[test]
    fn 写锁占着时_switch_库等得到而不是当场报忙() {
        let dir = crate::testing::temp_dir("titledb-store-busy");
        let path = dir.path().join("titledb.sqlite3");
        let blocker = Connection::open(&path).expect("开得起来");
        blocker
            .execute_batch("BEGIN IMMEDIATE")
            .expect("拿得到写锁");

        let (报开工, 等开工) = std::sync::mpsc::channel();
        let (交结果, 等结果) = std::sync::mpsc::channel();
        let 那份路径 = path.clone();
        let 那条线程 = std::thread::spawn(move || {
            报开工.send(()).expect("说得出去");
            let 结果 = Store::open(&那份路径)
                .and_then(|store| store.stats())
                .map(|stats| stats.ncas)
                .map_err(|error| error.to_string());
            交结果.send(结果).expect("交得回去");
        });
        等开工.recv().expect("那条线程起来了");
        std::thread::sleep(std::time::Duration::from_millis(100));
        blocker.execute_batch("ROLLBACK").expect("放得开");

        let 拿到 = 等结果
            .recv_timeout(std::time::Duration::from_secs(60))
            .expect("等得到那条线程交回来的结果");
        那条线程.join().expect("收得回来");
        assert_eq!(拿到, Ok(0), "撞上写锁该等着，不该当场报忙");
    }
}
