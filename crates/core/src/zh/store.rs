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
//!
//! ## 版本对不上就地重建，**不要用户重下那 435 MB**
//!
//! 「重建」指的是从**本机那份原件**（取数时留在缓存目录里的那个 zip）重读一遍，
//! 不走顺序迁移，也**不发一个网络请求**。[`Store::open`] 撞上旧版本时把这一版的 `dump`
//! 与指纹留在 [`Store::rebuilding`] 里交给 [`sync::rebuild`](super::sync::rebuild)——
//! 那两样正是「从哪个原件重建、重建完该记什么指纹」的全部所需。
//!
//! 反过来做（报个错让用户自己删）在这份库上尤其糟：删掉它就等于删掉那份 435 MB 的
//! 下载凭据，而结构版本每加一格都要用户重下一遍，是这份库当初与中立库分家的理由的反面。
//!
//! ### 旧数据一直留到新数据真的写进来那一刻
//!
//! **打开一份旧索引什么都不毁**：旧表原样躺着，`schema_version` 也还写着旧的那个数。
//! 换结构与写新数据是[同一个事务](Store::replace)里的事。
//!
//! 这一条不是洁癖。重建要读一份 435 MB 的 zip（解开 960 MB），中途 Ctrl-C、进程被杀、
//! 或者那份缓存 zip 本身就是截断的，都是真会发生的事。先扫后建的话，那一下之后库里
//! 是一份空索引、盖着新版本号，而「该从哪份原件重建」的线索已经跟着 `meta` 一起没了
//! ——唯一的出路正是重下那 435 MB，恰恰是这一整段要避免的事。
//!
//! 代价是这中间 [`Store::load`] 与 [`Store::stats`] 交出来的是空的：旧表的列与本程序
//! 认得的对不上，**半懂不懂地读比读不出来更糟**。

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use super::{Entry, FactKind, Index, NameKind};

/// 索引的结构版本。结构变了就加 1；读到对不上的版本**就地重建**（见模块文档）。
///
/// 2：条目多存了简介、类型、开发商、发行商四样。
pub const SCHEMA_VERSION: u32 = 2;

/// 这一版索引**从数据源里取了哪几样**。
///
/// 它有两个去处，缺一不可：
///
/// - 写进 `meta`，于是 `romcat zh sync` 之后看得出这一版索引带了哪些字段；
/// - 进**输入指纹**（`scrape::zh` 的 `probe`）。改了取哪些字段就该重采一遍——
///   不盖它的话缓存会一口咬定「输入没变」而整条跳过，新取到的字段永远出不来。
pub const FIELDS: &str = "中文名、别名、简介、类型、开发商、发行商、年份、平台";

/// `meta` 单开一份**先建**：结构版本就写在它里面，而要读它得先有这张表。
const META_SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS meta(
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;
";

const SCHEMA: &str = "\
-- 一条中文条目。`platforms` 是**折成本工具平台名**之后的那一串，两头带逗号
-- （`,GBA,NDS,`）——与 `content_cart.family` 同一个写法，为的是 SQL 里能用 `instr`
-- 做整词匹配，不至于 `GB` 匹配上 `GBA`。
-- `platform_text` 是数据源原样写的那一串，**依据里要写它**：人去核对时看的是原文。
-- `summary` 是**中文简介**，原样存着：换行、全角空格与数据源自带的排版都是内容的
-- 一部分，压掉它们导出到前端里就是一坨。实测最长 9,962 字——SQLite 的 TEXT 装得下，
-- 撑不撑得动是界面那一侧的事（列表要滚得动）。
CREATE TABLE IF NOT EXISTS subject(
    id            INTEGER PRIMARY KEY,
    name          TEXT NOT NULL,
    name_cn       TEXT NOT NULL,
    year          INTEGER,
    platforms     TEXT NOT NULL,
    platform_text TEXT NOT NULL,
    summary       TEXT NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS subject_year ON subject(year);

-- 一条**别名**。
--
-- **原名与中文名不在这儿**：它们是条目自己的一级字段，就摆在 `subject` 那一行上。
-- 这张表只收 `infobox` 里那一组别名——于是 `kind` 这一列眼下恒为 `alias`。留着它是
-- 为了让读库的人一眼知道这些行是什么，而不是等着将来往里塞别的东西。
CREATE TABLE IF NOT EXISTS subject_name(
    subject INTEGER NOT NULL,
    kind    TEXT    NOT NULL,
    value   TEXT    NOT NULL,
    PRIMARY KEY (subject, kind, value)
) STRICT;

-- 条目上一条**不是叫法**的事实：类型、开发商、发行商（`zh::FactKind`）。
--
-- **与叫法分开一张表**：那一张会被拿去撞名字，这一张不会。混在一起，`开发= 任天堂`
-- 会变成一条能撞上《任天堂》的「叫法」。
--
-- `ord` 是数据源里的**原次序**，不是排序用的装饰。⚠️ **票 04 之后，导出那条链上还没有
-- 人读它**：开发商与发行商拆成多条进中立库之后，`converge::build_game` 只 `pick` 得出
-- 一条，而那一条由 `Priorities::pick` 第四层排序键（**值本身**，即码位序）定——
-- `|开发= 科乐美、KCE东京` 导出去写的是 `KCE东京`，**换了一家公司，不只是换了次序**。
-- 这一格留着不是白留：把它折进那层排序键、或者让导出侧按集合读这两栏，两条路都要它。
-- 记在挂单 Q27。
CREATE TABLE IF NOT EXISTS subject_fact(
    subject INTEGER NOT NULL,
    kind    TEXT    NOT NULL,
    ord     INTEGER NOT NULL,
    value   TEXT    NOT NULL,
    PRIMARY KEY (subject, kind, ord)
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
    /// **这份索引是更新的程序建的。**
    ///
    /// 与「旧版本」处置相反，而这个方向**必须拒绝**：旧程序不认得新结构里的列，
    /// 照「重建」走就是拿旧程序的理解把新索引整份覆盖掉。两个版本的程序交替在同一个
    /// 工作目录上跑（发布版加本地版、bisect、两份 checkout）时，那是真会发生的事。
    #[error(
        "中文索引 {path} 的结构版本是 {found}，比本程序认得的 {expected} 还新——\
         它是更新的那一版程序建的。升级程序即可；\
         真要用这一版程序，删掉它重跑一次 `romcat zh sync`"
    )]
    TooNew {
        /// 索引文件。
        path: String,
        /// 文件里的版本。
        found: u32,
        /// 本程序的版本。
        expected: u32,
    },
}

/// **这份索引等着从本机那份原件重建**：结构版本对不上。**旧数据还在**——换结构与写
/// 新数据是 [`Store::replace`] 那一个事务里的事，旧的一直留到新的真写进来那一刻。
///
/// 两样都是重建要用的：从缓存目录里哪个文件重读，以及重建完该把哪个指纹记回去
/// ——记回去了，下一趟 `romcat zh sync` 才认得出「本机这份就是最新的」而整件跳过。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rebuilding {
    /// 上一版是从哪个 dump 建的（缓存目录里那个文件就叫这个名字）。
    pub dump: String,
    /// 那一版的指纹。
    pub fingerprint: String,
    /// 扫掉之前那份索引里的结构版本。报告里要如实说一句。
    pub was: u32,
}

/// 本机那份中文条目索引。
#[derive(Debug)]
pub struct Store {
    conn: Connection,
    path: PathBuf,
    rebuilding: Option<Rebuilding>,
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
    /// 其中有中文简介的。
    pub with_summary: u64,
    /// 其中写得出类型的。
    pub with_genre: u64,
    /// 其中写得出开发商的。
    pub with_developer: u64,
    /// 其中写得出发行商的。
    pub with_publisher: u64,
    /// 这一版索引从数据源里取了哪几样（[`FIELDS`]）。**换了这一行就该重刮**。
    pub fields: String,
    /// 按平台：平台名与条目数。**老平台深度浅是事实不是缺陷**，报告要摆出来。
    pub by_platform: Vec<(String, u64)>,
}

impl Store {
    /// 打开（必要时新建）本机那份索引。
    ///
    /// 结构版本比本程序**旧**时**不报错，也一个字都不动**：只把重建要用的那两样记下来
    /// （[`Store::rebuilding`]），等着 [`sync::rebuild`](super::sync::rebuild) 从本机那份
    /// 原件重读一遍；换结构与写新数据是 [`Store::replace`] 那一个事务里的事，中途被打断
    /// 就一起回滚。这份库里没有攒出来的东西，重建不丢任何判断。
    ///
    /// 结构版本比本程序**新**时才报错（[`StoreError::TooNew`]）——旧程序不认得新结构里的
    /// 列，照「重建」走就是拿旧程序的理解把新索引整份覆盖掉。
    ///
    /// # Errors
    /// 目录建不出来或者库打不开时返回错误。
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
        let mut store = Self {
            conn,
            path: path.to_path_buf(),
            rebuilding: None,
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
        let mut store = Self {
            conn,
            path: PathBuf::from(":memory:"),
            rebuilding: None,
        };
        store.prepare()?;
        Ok(store)
    }

    /// 这份索引在等着重建吗——是的话，从哪个原件重建、重建完记哪个指纹。
    #[must_use]
    pub fn rebuilding(&self) -> Option<&Rebuilding> {
        self.rebuilding.as_ref()
    }

    fn prepare(&mut self) -> Result<(), StoreError> {
        // **先读版本再建那几张表**：`CREATE TABLE IF NOT EXISTS` 对已经存在的旧表一个字
        // 都不改，先建完再读，读到的会是新旧混着的一份结构（旧表缺着新列），而 `meta`
        // 里那个数还写着旧版本。`meta` 自己例外——版本就写在它里面，得先有它。
        self.conn
            .execute_batch(&format!("PRAGMA journal_mode=WAL;\n{META_SCHEMA}"))
            .map_err(|source| self.error(source))?;
        let found: Option<u32> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|source| self.error(source))?
            .and_then(|text| text.parse().ok());
        match found {
            // **比本程序新的一律拒绝。** 旧程序不认得新结构里的列，照「重建」走就是拿
            // 旧程序的理解把新索引整份覆盖掉。
            Some(version) if version > SCHEMA_VERSION => {
                return Err(StoreError::TooNew {
                    path: crate::path::display(&self.path),
                    found: version,
                    expected: SCHEMA_VERSION,
                });
            }
            // **旧版本：一个字都不动**，只把重建要用的两样记下来。换结构与写新数据是
            // `replace` 那一个事务里的事（见模块文档「旧数据一直留到新数据真的写进来」）。
            Some(version) if version < SCHEMA_VERSION => {
                self.rebuilding = Some(Rebuilding {
                    dump: self.meta("dump")?.unwrap_or_default(),
                    fingerprint: self.meta("fingerprint")?.unwrap_or_default(),
                    was: version,
                });
                return Ok(());
            }
            _ => {}
        }
        // 到这儿只剩两种：版本号正好，或者**根本没有版本号那一行**。后者又分两种，
        // 处置完全相反，**分不开就会写坏库**（挂单 Q14 的第二半）：
        //
        // - **空文件**——第一次开，什么表都还没有。建表、当场落版本号。
        // - **已经有表、却没有版本号**——上一版程序在 `execute_batch(SCHEMA)` 与写版本号
        //   之间被杀，或者干脆是加版本号之前那几版留下的。这一份的**形状不知道**，
        //   多半比本程序旧。这时若照「新建」走，`CREATE TABLE IF NOT EXISTS` 对已存在的
        //   旧表一个字不改，而版本号被盖成当前版——**旧形状的库从此顶着新版本号**，
        //   自动重建那条路再也不会触发，`replace` 每一趟都在
        //   `INSERT INTO subject(… 新列 …)` 上硬报 `no column named`。**失败还从「下一趟
        //   自己补得回来」变成了「永远补不回来」**。所以它要走重建那条路。
        let 已经有表 = self
            .conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'subject'",
                [],
                |_| Ok(()),
            )
            .optional()
            .map_err(|source| self.error(source))?
            .is_some();
        if found.is_none() && 已经有表 {
            // `was: 0` —— 「不知道是哪一版」。重建从本机那份原件读，读的是原件不是这张表，
            // 所以不知道旧版本号也照样重建得起来。
            self.rebuilding = Some(Rebuilding {
                dump: self.meta("dump")?.unwrap_or_default(),
                fingerprint: self.meta("fingerprint")?.unwrap_or_default(),
                was: 0,
            });
            return Ok(());
        }
        self.conn
            .execute_batch(SCHEMA)
            .map_err(|source| self.error(source))?;
        // **新建的库当场落版本号**，别让它变成上面那种「有表没版本号」的库。
        if found.is_none() {
            self.put_meta("schema_version", &SCHEMA_VERSION.to_string())?;
        }
        Ok(())
    }

    /// 换上新结构：旧的那几张表整个扔掉重建。
    ///
    /// **只有 [`Store::replace`] 调它，而且是在它那个事务里面调**。SQLite 的 DDL 是
    /// 事务性的，所以 `DROP` 与紧接着那几万条 `INSERT` 要么一起落盘、要么一起不落——
    /// 模块文档那句「换结构与写新数据是同一个事务里的事」说的正是这个。
    ///
    /// 早先它走的是 `Store::execute_batch`（**自动提交**），于是 `DROP` 落盘与新数据
    /// 提交之间有一个真实的窗口：重建写到一半被 Ctrl-C 或杀掉，库里就只剩一份空的新
    /// 结构表。缓存里那份 dump 还在时下一趟自己补得回来，被清掉了就只剩重下 435 MB
    /// ——恰是这套设计要避免的结局（挂单 Q23）。
    fn reshape(tx: &rusqlite::Transaction<'_>) -> rusqlite::Result<()> {
        tx.execute_batch(&format!(
            "DROP TABLE IF EXISTS subject_fact;
             DROP TABLE IF EXISTS subject_name;
             DROP TABLE IF EXISTS subject;
             {SCHEMA}"
        ))
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
        // **换结构与写数据在同一个事务里**：旧数据一直留到这一刻，而且中途被打断时
        // 一起回滚（`Store::reshape` 的文档）。
        let rebuilding = self.rebuilding.is_some();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|it| i64::try_from(it.as_secs()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        // 事务借走了 `self`，`self.error` 这一路就用不上了；把同一句话捏成一个闭包，
        // 免得下面每一步各写一遍 `StoreError::Sqlite { path, source }`。
        let path = crate::path::display(&self.path);
        let failed = |source: rusqlite::Error| StoreError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(failed)?;
        if rebuilding {
            Self::reshape(&tx).map_err(failed)?;
        }
        tx.execute_batch("DELETE FROM subject_fact; DELETE FROM subject_name; DELETE FROM subject;")
            .map_err(failed)?;
        {
            let mut subject = tx
                .prepare(
                    "INSERT OR REPLACE INTO subject(id, name, name_cn, year, platforms, platform_text, summary)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )
                .map_err(failed)?;
            let mut name = tx
                .prepare(
                    "INSERT OR IGNORE INTO subject_name(subject, kind, value) VALUES (?1, ?2, ?3)",
                )
                .map_err(failed)?;
            let mut fact = tx
                .prepare(
                    "INSERT OR IGNORE INTO subject_fact(subject, kind, ord, value)
                     VALUES (?1, ?2, ?3, ?4)",
                )
                .map_err(failed)?;
            for entry in entries {
                subject
                    .execute(params![
                        entry.id,
                        entry.name,
                        entry.name_cn,
                        entry.year,
                        fold_platforms(&entry.platforms),
                        entry.platform_text,
                        entry.summary,
                    ])
                    .map_err(failed)?;
                for alias in &entry.aliases {
                    name.execute(params![entry.id, NameKind::Alias.code(), alias])
                        .map_err(failed)?;
                }
                for kind in FactKind::all() {
                    for (at, value) in entry.facts(kind).iter().enumerate() {
                        fact.execute(params![
                            entry.id,
                            kind.code(),
                            i64::try_from(at).unwrap_or(i64::MAX),
                            value
                        ])
                        .map_err(failed)?;
                    }
                }
            }
        }
        tx.commit().map_err(failed)?;
        self.put_meta("dump", dump)?;
        self.put_meta("fingerprint", fingerprint)?;
        self.put_meta("built_at", &now.to_string())?;
        self.put_meta("fields", FIELDS)?;
        // **结构版本最后才落盘**：写在这之前的话，一次半途而废的重建会留下一份空索引
        // 盖着新版本号，而「该从哪份原件重建」的线索已经没了。
        self.put_meta("schema_version", &SCHEMA_VERSION.to_string())?;
        self.rebuilding = None;
        Ok(())
    }

    /// 把整份索引装进内存。**匹配走内存不走 SQL**：一次匹配要看上千条叫法，
    /// 几万个变体就是几千万次查询，那不是 SQLite 该干的活。
    ///
    /// **简介不在里面**（`Entry::summary` 一律是空串）。装进来要多背九十来 MB 常驻
    /// （8.7 万条条目、94.2% 有简介、中位 338 字），而这一层的活是撞名字，撞名字不看
    /// 简介。要某一条的简介走 [`Store::summary`]——那时手上已经有条目号了。
    ///
    /// 类型、开发商、发行商留在里面：它们是几个短串，量级差着两个数量级。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn load(&self) -> Result<Index, StoreError> {
        // 等着重建时交出一份空的：旧表的列与本程序认得的对不上，
        // **半懂不懂地读比读不出来更糟**。
        if self.rebuilding.is_some() {
            return Ok(Index::default());
        }
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
        // 事实（类型 / 开发商 / 发行商）按 `ord` 读回来：那是数据源里的**原次序**，
        // 前端只写得下一个开发商时取的就是第一个。
        let mut facts: std::collections::BTreeMap<(u32, String), Vec<String>> =
            std::collections::BTreeMap::new();
        let mut statement = self
            .conn
            .prepare("SELECT subject, kind, value FROM subject_fact ORDER BY subject, kind, ord")
            .map_err(|source| self.error(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, u32>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|source| self.error(source))?;
        for row in rows {
            let (subject, kind, value) = row.map_err(|source| self.error(source))?;
            facts.entry((subject, kind)).or_default().push(value);
        }
        // **简介不装进内存**，见 `load` 的文档：8.7 万条乘中位 338 字是九十来 MB 常驻，
        // 而这一层的活是撞名字，撞名字不看简介。要哪一条的简介走 [`Store::summary`]。
        let mut statement = self
            .conn
            .prepare(
                "SELECT id, name, name_cn, year, platforms, platform_text
                 FROM subject ORDER BY id",
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
                    ..Entry::default()
                })
            })
            .map_err(|source| self.error(source))?;
        let mut entries = Vec::new();
        for row in rows {
            let mut entry = row.map_err(|source| self.error(source))?;
            entry.aliases = aliases.remove(&entry.id).unwrap_or_default();
            for kind in FactKind::all() {
                let got = facts
                    .remove(&(entry.id, kind.code().to_string()))
                    .unwrap_or_default();
                *entry.facts_mut(kind) = got;
            }
            entries.push(entry);
        }
        let dump = self.meta("dump")?.unwrap_or_default();
        let fields = self.meta("fields")?.unwrap_or_default();
        Ok(Index::build(entries, dump).with_fields(fields))
    }

    /// 某一条条目的**中文简介**；这条条目不在库里、或者数据源没写就是 `None`。
    ///
    /// 单独一条路而不是跟着 [`Store::load`] 一起进内存：见那个函数的文档。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn summary(&self, id: u32) -> Result<Option<String>, StoreError> {
        self.conn
            .query_row(
                "SELECT summary FROM subject WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|source| self.error(source))
            .map(|found| found.filter(|text| !text.is_empty()))
    }

    /// 索引里现在有什么。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn stats(&self) -> Result<Stats, StoreError> {
        if self.rebuilding.is_some() {
            // 同 `load`：数不出来的时候不要编一个数。dump 那一行还说得出来，
            // 因为它就写在 `meta` 里，而 `meta` 一直没动过。
            return Ok(Stats {
                dump: self.meta("dump")?.unwrap_or_default(),
                ..Stats::default()
            });
        }
        let mut stats = Stats {
            dump: self.meta("dump")?.unwrap_or_default(),
            built_at: self
                .meta("built_at")?
                .and_then(|it| it.parse().ok())
                .unwrap_or(0),
            fields: self.meta("fields")?.unwrap_or_default(),
            ..Stats::default()
        };
        let counts = self
            .conn
            .query_row(
                "SELECT COUNT(*),
                        SUM(CASE WHEN name_cn <> '' THEN 1 ELSE 0 END),
                        SUM(CASE WHEN platforms <> '' THEN 1 ELSE 0 END),
                        SUM(CASE WHEN year IS NOT NULL THEN 1 ELSE 0 END),
                        SUM(CASE WHEN summary <> '' THEN 1 ELSE 0 END)
                 FROM subject",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                        row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                        row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                        row.get::<_, Option<i64>>(4)?.unwrap_or(0),
                    ))
                },
            )
            .map_err(|source| self.error(source))?;
        stats.subjects = u64::try_from(counts.0).unwrap_or(0);
        stats.with_chinese = u64::try_from(counts.1).unwrap_or(0);
        stats.with_platform = u64::try_from(counts.2).unwrap_or(0);
        stats.with_year = u64::try_from(counts.3).unwrap_or(0);
        stats.with_summary = u64::try_from(counts.4).unwrap_or(0);
        // **数的是有几条条目写得出这一样，不是一共有几条值**：一条条目写了三个开发商
        // 仍然只算一条有开发商，不然「覆盖率」会大于 100%。
        for kind in FactKind::all() {
            let count: i64 = self
                .conn
                .query_row(
                    "SELECT COUNT(DISTINCT subject) FROM subject_fact WHERE kind = ?1",
                    params![kind.code()],
                    |row| row.get(0),
                )
                .map_err(|source| self.error(source))?;
            let count = u64::try_from(count).unwrap_or(0);
            match kind {
                FactKind::Genre => stats.with_genre = count,
                FactKind::Developer => stats.with_developer = count,
                FactKind::Publisher => stats.with_publisher = count,
            }
        }
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
            summary: "　　以细腻的画风…\n第二段。".to_string(),
            genres: vec!["ACT".to_string()],
            developers: vec!["SNK".to_string(), "北斗".to_string()],
            publishers: vec!["世嘉".to_string()],
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
        // **简介不在装进内存那一份里**（见 `load` 的文档），别的一格不差。
        assert_eq!(
            index.entries()[0],
            Entry {
                summary: String::new(),
                ..一条()
            }
        );
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
    fn 简介类型开发商发行商都留得住() {
        // 这四样以前一列都没有——那份 435 MB 的数据被当成「撞名字的索引」在用。
        let mut store = Store::in_memory().expect("开得起来");
        store.replace(&[一条()], "dump", "指纹").expect("写得进去");
        // **简介单独一条路读**：装进内存那份索引里要多背九十来 MB，而撞名字不看简介。
        assert_eq!(
            store.summary(4).expect("读得回来").as_deref(),
            // **简介原样**：中间那个换行是内容的一部分，压掉它导出到前端里就是一坨。
            Some("　　以细腻的画风…\n第二段。")
        );
        assert_eq!(store.summary(999).expect("读得回来"), None, "没有这条条目");
        let index = store.load().expect("读得回来");
        let entry = &index.entries()[0];
        assert_eq!(entry.summary, "", "简介不进内存");
        assert_eq!(entry.genres, vec!["ACT"]);
        // **原次序留着**：前端只写得下一个开发商时取的就是第一个。
        assert_eq!(entry.developers, vec!["SNK", "北斗"]);
        assert_eq!(entry.publishers, vec!["世嘉"]);
        // 这一版索引取了哪几样也记着——它同时是刮削那一侧的输入指纹的一部分。
        assert_eq!(index.fields(), FIELDS);
    }

    /// 造一份「结构版本是 `version`」的索引，返回它的路径。
    fn 一份旧索引(dir: &crate::testing::TempDir, version: u32) -> PathBuf {
        let path = dir.path().join("zh.sqlite3");
        {
            let mut store = Store::open(&path).expect("开得起来");
            assert!(store.rebuilding().is_none(), "新建的一份不必重建");
            store
                .replace(&[一条()], "dump-2026-09-01.zip", "sha256:abc")
                .expect("写得进去");
        }
        let conn = rusqlite::Connection::open(&path).expect("开得起来");
        conn.execute(
            "UPDATE meta SET value = ?1 WHERE key = 'schema_version'",
            params![version.to_string()],
        )
        .expect("改得动");
        path
    }

    #[test]
    fn 结构版本旧了先不动旧数据_只交出重建线索() {
        // **不报错让用户自己删**：删掉它就等于删掉那份 435 MB 的下载凭据。
        // 也**不先扫后建**：重建要读一份 435 MB 的 zip，中途 Ctrl-C 是真会发生的事，
        // 那一下之后不该只剩一份空索引加一个新版本号。
        let dir = crate::testing::temp_dir("zh-store-version");
        let path = 一份旧索引(&dir, 1);

        let store = Store::open(&path).expect("旧版本照样打得开，不报错");
        let pending = store.rebuilding().expect("等着重建").clone();
        assert_eq!(pending.was, 1);
        // 重建要用的两样都在：从哪个原件重建、重建完记哪个指纹。
        assert_eq!(pending.dump, "dump-2026-09-01.zip");
        assert_eq!(pending.fingerprint, "sha256:abc");
        // 这中间读出来的是空的——旧表的列与本程序认得的对不上，半懂不懂地读更糟。
        assert!(store.load().expect("读得回来").is_empty());
        assert_eq!(store.stats().expect("数得出来").subjects, 0);
        // **而版本号还写着旧的那个**：这一趟半途而废也不丢线索，下次打开照样重建得了。
        assert_eq!(
            store.meta("schema_version").expect("读得到"),
            Some("1".to_string())
        );
        drop(store);
        assert!(
            Store::open(&path)
                .expect("再打开一次")
                .rebuilding()
                .is_some(),
            "没重建成就该一直等着"
        );

        // 真写进新数据那一刻，结构与版本号才一起换掉。
        let mut store = Store::open(&path).expect("开得起来");
        store
            .replace(&[一条()], "dump-2026-09-01.zip", "sha256:abc")
            .expect("写得进去");
        assert!(store.rebuilding().is_none());
        assert_eq!(
            store.meta("schema_version").expect("读得到"),
            Some(SCHEMA_VERSION.to_string())
        );
        assert_eq!(store.load().expect("读得回来").len(), 1);
    }

    #[test]
    fn 有表却没有版本号的库走重建_而不是被盖上当前版本号() {
        // 这一份是上一版程序在「建完表」与「写版本号」之间被杀留下的，也可能是加版本号
        // 之前那几版留下的。**形状不知道，多半比本程序旧。**
        //
        // 照「新建」走的话：`CREATE TABLE IF NOT EXISTS` 对已存在的旧表一个字不改，而
        // 版本号被盖成当前版——旧形状的库从此顶着新版本号，重建再也不会触发，`replace`
        // 每一趟都在 `INSERT INTO subject(… 新列 …)` 上硬报 `no column named`。
        // **失败于是从「下一趟自己补得回来」变成「永远补不回来」**，而唯一的出路是
        // 重下那 435 MB。
        let dir = crate::testing::temp_dir("zh-store-no-version");
        let path = 一份旧索引(&dir, 1);
        {
            let conn = rusqlite::Connection::open(&path).expect("开得起来");
            conn.execute("DELETE FROM meta WHERE key = 'schema_version'", [])
                .expect("删得掉");
        }

        let store = Store::open(&path).expect("打得开，不报错");

        let pending = store.rebuilding().expect("该等着重建").clone();
        assert_eq!(pending.was, 0, "不知道是哪一版");
        // 重建要用的两样照样交得出来——它们不在被删掉的那一行上。
        assert_eq!(pending.dump, "dump-2026-09-01.zip");
        assert_eq!(pending.fingerprint, "sha256:abc");
        // **绝不能顺手把当前版本号盖上去**：盖上了就再也回不来。
        assert_eq!(
            store.meta("schema_version").expect("读得到"),
            None,
            "没重建成之前不许盖版本号"
        );
        drop(store);
        assert!(
            Store::open(&path)
                .expect("再打开一次")
                .rebuilding()
                .is_some(),
            "没重建成就该一直等着"
        );

        // 真写进新数据那一刻，结构与版本号才一起换上。
        let mut store = Store::open(&path).expect("开得起来");
        store
            .replace(&[一条()], "dump-2026-09-01.zip", "sha256:abc")
            .expect("写得进去");
        assert!(store.rebuilding().is_none());
        assert_eq!(
            store.meta("schema_version").expect("读得到"),
            Some(SCHEMA_VERSION.to_string())
        );
        assert_eq!(store.load().expect("读得回来").len(), 1);
    }

    #[test]
    fn 结构版本比本程序新的一律拒绝() {
        // 与「旧版本」处置相反，而这个方向必须拒绝：照「重建」走就是拿旧程序的理解
        // 把新索引整份覆盖掉。两个版本的程序交替在同一个工作目录上跑是真会发生的事。
        let dir = crate::testing::temp_dir("zh-store-too-new");
        let path = 一份旧索引(&dir, SCHEMA_VERSION + 1);
        let error = Store::open(&path).expect_err("该拒绝");
        assert!(matches!(error, StoreError::TooNew { found, .. } if found == SCHEMA_VERSION + 1));
        // 库一个字都没动。
        let conn = rusqlite::Connection::open(&path).expect("开得起来");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM subject", [], |row| row.get(0))
            .expect("数得出来");
        assert_eq!(count, 1);
    }

    #[test]
    fn 统计数得出中文名与平台的覆盖() {
        let mut store = Store::in_memory().expect("开得起来");
        let mut 没中文名 = 一条();
        没中文名.id = 5;
        没中文名.name_cn = String::new();
        没中文名.platforms = Vec::new();
        没中文名.year = None;
        没中文名.summary = String::new();
        没中文名.genres = Vec::new();
        没中文名.developers = Vec::new();
        没中文名.publishers = Vec::new();
        store
            .replace(&[一条(), 没中文名], "dump", "指纹")
            .expect("写得进去");
        let stats = store.stats().expect("数得出来");
        assert_eq!(stats.subjects, 2);
        assert_eq!(stats.with_chinese, 1);
        assert_eq!(stats.with_platform, 1);
        assert_eq!(stats.with_year, 1);
        assert_eq!(stats.by_platform, vec![("NDS".to_string(), 1)]);
        // 新的四样也数得出来。**数的是有几条条目写得出这一样**：那一条写了两个开发商
        // 仍然只算一条，不然「覆盖率」会大于 100%。
        assert_eq!(stats.with_summary, 1);
        assert_eq!(stats.with_genre, 1);
        assert_eq!(stats.with_developer, 1);
        assert_eq!(stats.with_publisher, 1);
        assert_eq!(stats.fields, FIELDS);
    }
}
