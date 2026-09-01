//! **DAT 库**：镜像下来的哈希数据落在这里。
//!
//! 它**不是**中立库，是另一份 SQLite，住在工作目录的 `dat/` 下。分开的理由有三条，
//! 每条单独都够：
//!
//! - **DAT 与哪个主库无关。** 中立库一个主库一份（键是相对主库根的路径，ADR-0020），
//!   而「世上有哪些发行版」对两块盘是同一份。
//! - **删库重扫不该赔上一百多 MB 的 DAT。** 中立库的结构版本一变就让用户删库重扫
//!   （`catalog::SCHEMA_VERSION`），那是几分钟的事；重下 DAT 不是。
//! - **两边的更新节奏不同。** 扫描按盘走，同步按天走。
//!
//! ## 一件东西 = 一次取数的最小单位
//!
//! `unit` 记的是「取回来的一件东西」，它的粒度随源而变：No-Intro 是那个 106 MB 的
//! 整包（镜像只发整包），Redump 是一个系统，TOSEC / MAME / GoodNES 是仓库里的一个文件。
//! **增量就落在这一层**：指纹没变就整件跳过，一个字节都不取。
//!
//! 一件东西**整件换掉**——旧的 DAT 连同它的条目与文件记录一起删，新的整批写进去。
//! 逐条比对差异在这里毫无意义：DAT 本来就是整份重新生成的。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use super::Convention;
use super::chinese::{ChineseMark, mark_of};
use super::logiqx::{DatHeader, GameRecord};

/// DAT 库的结构版本。结构变了就加 1；读到对不上的版本直接重建。
///
/// 重建这条路在这里一直走得通——DAT 库里没有任何攒出来的东西，全部内容都能重取。
/// 沉淀库（票 08）绝不能放进这份库，放进来这条就不成立了。
pub const SCHEMA_VERSION: u32 = 1;

const SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS meta(
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

-- 一次取数的最小单位。`fingerprint` 是增量的全部依据：
--   No-Intro  → 发行资产的 updated_at + 大小（镜像只发整包）
--   Redump    → Content-Disposition 里的附件名（带条目数与生成时刻）
--   其余三家  → git blob 的 sha（仓库自己算好的，天然增量）
CREATE TABLE IF NOT EXISTS unit(
    id          INTEGER PRIMARY KEY,
    source      TEXT    NOT NULL,
    name        TEXT    NOT NULL,
    url         TEXT    NOT NULL,
    fingerprint TEXT    NOT NULL,
    fetched_at  INTEGER NOT NULL,
    UNIQUE(source, name)
) STRICT;

-- 一份 DAT。`convention` 是这张票的核心一列——No-Intro 的 headerless 集按去头，
-- TOSEC 与 GoodNES 按含头，票 07 据它决定拿哪一套哈希去撞。
CREATE TABLE IF NOT EXISTS dat(
    id         INTEGER PRIMARY KEY,
    unit       INTEGER NOT NULL,
    source     TEXT    NOT NULL,
    name       TEXT    NOT NULL,
    platform   TEXT    NOT NULL,
    convention TEXT    NOT NULL,
    version    TEXT    NOT NULL,
    games      INTEGER NOT NULL,
    roms       INTEGER NOT NULL,
    fan        INTEGER NOT NULL,
    official   INTEGER NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS dat_unit ON dat(unit);
CREATE INDEX IF NOT EXISTS dat_platform ON dat(platform);

-- 一条 DAT 条目。**通常**是一个发行版，但 TOSEC 的 `[tr zh]` 条目是汉化版，
-- 那是变体——所以这一层只叫条目，挂到哪一层由票 07 定。
CREATE TABLE IF NOT EXISTS game(
    id       INTEGER PRIMARY KEY,
    dat      INTEGER NOT NULL,
    name     TEXT    NOT NULL,
    ident    TEXT,
    cloneof  TEXT,
    serial   TEXT,
    chinese  TEXT
) STRICT;

CREATE INDEX IF NOT EXISTS game_dat ON game(dat);

-- 一条文件记录。`dat` 是冗余的一列：票 07 从哈希查过来之后要立刻知道
-- 「这是哪份 DAT 的、什么口径」，多一跳 join 在几十万行上不划算。
CREATE TABLE IF NOT EXISTS rom(
    id     INTEGER PRIMARY KEY,
    game   INTEGER NOT NULL,
    dat    INTEGER NOT NULL,
    name   TEXT    NOT NULL,
    size   INTEGER,
    crc32  INTEGER,
    md5    TEXT,
    sha1   TEXT,
    sha256 TEXT,
    status TEXT
) STRICT;

-- 票 07 的三条查询路径。CRC-32 是主力，SHA-1 是 GoodNES 唯一能给的东西。
CREATE INDEX IF NOT EXISTS rom_crc32 ON rom(crc32);
CREATE INDEX IF NOT EXISTS rom_sha1 ON rom(sha1);
CREATE INDEX IF NOT EXISTS rom_md5 ON rom(md5);
CREATE INDEX IF NOT EXISTS rom_dat ON rom(dat);
";

/// DAT 库读写出错。
#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    /// 目录建不出来。
    #[error("DAT 库的目录建不出来：{path}（{source}）")]
    Io {
        /// 出问题的路径。
        path: String,
        /// 底层错误。
        source: std::io::Error,
    },
    /// 底层读写失败。
    #[error("DAT 库读写失败：{path}（{source}）")]
    Sqlite {
        /// DAT 库文件。
        path: String,
        /// 底层错误。
        source: rusqlite::Error,
    },
    /// 结构版本对不上。
    #[error(
        "DAT 库 {path} 的结构版本是 {found}，本程序认得的是 {expected}。删掉它重新同步即可——这份库里没有攒出来的东西，全部内容都能重取"
    )]
    Version {
        /// DAT 库文件。
        path: String,
        /// 文件里的版本。
        found: u32,
        /// 本程序的版本。
        expected: u32,
    },
}

/// 一件取回来的东西。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    /// 属于哪个源。
    pub source: String,
    /// 源内唯一的名字：DAT 名、系统短码，或仓库内路径。
    pub name: String,
    /// 从哪儿取的。
    pub url: String,
    /// 变没变的依据。
    pub fingerprint: String,
}

/// 一份 DAT 的身份，入库时随内容一起交进来。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatMeta {
    /// DAT 在源里叫什么。
    pub name: String,
    /// 归哪个平台。
    pub platform: String,
    /// 哈希口径。
    pub convention: Convention,
    /// DAT 自己的头。
    pub header: DatHeader,
}

/// 一份 DAT 入库之后的计数。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DatCounts {
    /// 条目数。
    pub games: u64,
    /// 文件记录数。
    pub roms: u64,
    /// 汉化条目数。
    pub fan: u64,
    /// 官中条目数。
    pub official: u64,
}

impl DatCounts {
    /// 两份加起来。
    #[must_use]
    pub fn plus(self, other: Self) -> Self {
        Self {
            games: self.games + other.games,
            roms: self.roms + other.roms,
            fan: self.fan + other.fan,
            official: self.official + other.official,
        }
    }
}

/// 镜像下来的哈希数据库。
pub struct DatRepo {
    conn: Connection,
    path: PathBuf,
}

impl DatRepo {
    /// 打开（或建出）一份 DAT 库。
    ///
    /// # Errors
    /// 目录建不出来、打不开、或者结构版本对不上时返回错误。
    pub fn open(path: &Path) -> Result<Self, RepoError> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|source| RepoError::Io {
                path: crate::path::display(parent),
                source,
            })?;
        }
        let conn = Connection::open(path).map_err(|source| RepoError::Sqlite {
            path: crate::path::display(path),
            source,
        })?;
        let repo = Self {
            conn,
            path: path.to_path_buf(),
        };
        repo.prepare()?;
        Ok(repo)
    }

    /// 完全在内存里开一份。测试用。
    ///
    /// # Errors
    /// 建不出来时返回错误。
    pub fn in_memory() -> Result<Self, RepoError> {
        let conn = Connection::open_in_memory().map_err(|source| RepoError::Sqlite {
            path: ":memory:".to_string(),
            source,
        })?;
        let repo = Self {
            conn,
            path: PathBuf::from(":memory:"),
        };
        repo.prepare()?;
        Ok(repo)
    }

    fn prepare(&self) -> Result<(), RepoError> {
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
        match found.and_then(|text| text.parse::<u32>().ok()) {
            Some(version) if version != SCHEMA_VERSION => Err(RepoError::Version {
                path: self.location(),
                found: version,
                expected: SCHEMA_VERSION,
            }),
            Some(_) => Ok(()),
            None => {
                self.conn
                    .execute(
                        "INSERT INTO meta(key, value) VALUES('schema_version', ?1)",
                        params![SCHEMA_VERSION.to_string()],
                    )
                    .map_err(|source| self.error(source))?;
                Ok(())
            }
        }
    }

    /// 把底层错误包成这份库的错误。查询散在别的模块里（[`super::lookup`]、
    /// [`super::report`]），它们也得说得出「是哪份库读不出来」。
    pub(crate) fn error(&self, source: rusqlite::Error) -> RepoError {
        RepoError::Sqlite {
            path: self.location(),
            source,
        }
    }

    /// 这份库在哪。
    #[must_use]
    pub fn location(&self) -> String {
        crate::path::display(&self.path)
    }

    /// 某个源手上已有的东西：名字 → 指纹。**增量的依据就是它。**
    ///
    /// # Errors
    /// 读不出来时返回错误。
    pub fn fingerprints(&self, source: &str) -> Result<BTreeMap<String, String>, RepoError> {
        let mut statement = self
            .conn
            .prepare("SELECT name, fingerprint FROM unit WHERE source = ?1")
            .map_err(|error| self.error(error))?;
        let rows = statement
            .query_map(params![source], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| self.error(error))?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (name, fingerprint) = row.map_err(|error| self.error(error))?;
            out.insert(name, fingerprint);
        }
        Ok(out)
    }

    /// 库里一条记录都没有。
    ///
    /// # Errors
    /// 读不出来时返回错误。
    pub fn is_empty(&self) -> Result<bool, RepoError> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM dat", [], |row| row.get(0))
            .map_err(|error| self.error(error))?;
        Ok(count == 0)
    }

    /// 开始换掉一件东西。旧记录在这里就删掉，新的往回写。
    ///
    /// # Errors
    /// 写不进时返回错误。
    pub fn begin(&mut self, unit: &Unit) -> Result<UnitWriter<'_>, RepoError> {
        let path = crate::path::display(&self.path);
        let wrap = |source: rusqlite::Error| RepoError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(wrap)?;
        // 整件换掉：先把这件旧的全部记录连根拔掉。SQLite 没开外键级联，
        // 这里手写三条 DELETE——顺序从叶子往根走，中途出错事务整个回滚。
        tx.execute(
            "DELETE FROM rom WHERE dat IN (SELECT d.id FROM dat d JOIN unit u ON d.unit = u.id
             WHERE u.source = ?1 AND u.name = ?2)",
            params![unit.source, unit.name],
        )
        .map_err(wrap)?;
        tx.execute(
            "DELETE FROM game WHERE dat IN (SELECT d.id FROM dat d JOIN unit u ON d.unit = u.id
             WHERE u.source = ?1 AND u.name = ?2)",
            params![unit.source, unit.name],
        )
        .map_err(wrap)?;
        tx.execute(
            "DELETE FROM dat WHERE unit IN (SELECT id FROM unit WHERE source = ?1 AND name = ?2)",
            params![unit.source, unit.name],
        )
        .map_err(wrap)?;
        let now = i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|since| since.as_secs())
                .unwrap_or_default(),
        )
        .unwrap_or_default();
        tx.execute(
            "INSERT INTO unit(source, name, url, fingerprint, fetched_at) VALUES(?1,?2,?3,?4,?5)
             ON CONFLICT(source, name) DO UPDATE SET
                url = excluded.url,
                fingerprint = excluded.fingerprint,
                fetched_at = excluded.fetched_at",
            params![unit.source, unit.name, unit.url, unit.fingerprint, now],
        )
        .map_err(wrap)?;
        let unit_id: i64 = tx
            .query_row(
                "SELECT id FROM unit WHERE source = ?1 AND name = ?2",
                params![unit.source, unit.name],
                |row| row.get(0),
            )
            .map_err(wrap)?;
        Ok(UnitWriter {
            tx,
            path,
            source: unit.source.clone(),
            unit_id,
            counts: DatCounts::default(),
            dats: 0,
        })
    }

    /// 借出底层连接，给报告那边跑聚合查询。
    #[must_use]
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }
}

/// 正在换掉的那一件东西。**没 `commit` 就什么都没写进去。**
pub struct UnitWriter<'a> {
    tx: rusqlite::Transaction<'a>,
    path: String,
    source: String,
    unit_id: i64,
    counts: DatCounts,
    dats: u64,
}

impl UnitWriter<'_> {
    /// 写进一份 DAT。
    ///
    /// 一份一份地交而不是整件攒着，是因为一件东西可以很大：No-Intro 那个整包里
    /// 有 334 份 DAT。一份最多几万条，攒在内存里无所谓；334 份加起来就不好说了。
    ///
    /// # Errors
    /// 写不进时返回错误。
    pub fn write_dat(
        &mut self,
        meta: &DatMeta,
        games: &[GameRecord],
    ) -> Result<DatCounts, RepoError> {
        let path = self.path.clone();
        let wrap = |source: rusqlite::Error| RepoError::Sqlite {
            path: path.clone(),
            source,
        };
        let mut counts = DatCounts::default();
        self.tx
            .execute(
                "INSERT INTO dat(unit, source, name, platform, convention, version, games, roms, fan, official)
                 VALUES(?1,?2,?3,?4,?5,?6,0,0,0,0)",
                params![
                    self.unit_id,
                    self.source,
                    meta.name,
                    meta.platform,
                    meta.convention.label(),
                    meta.header.version,
                ],
            )
            .map_err(wrap)?;
        let dat_id = self.tx.last_insert_rowid();

        {
            let mut insert_game = self
                .tx
                .prepare(
                    "INSERT INTO game(dat, name, ident, cloneof, serial, chinese)
                     VALUES(?1,?2,?3,?4,?5,?6)",
                )
                .map_err(wrap)?;
            let mut insert_rom = self
                .tx
                .prepare(
                    "INSERT INTO rom(game, dat, name, size, crc32, md5, sha1, sha256, status)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                )
                .map_err(wrap)?;
            for game in games {
                let chinese = mark_of(&game.name);
                match chinese {
                    Some(ChineseMark::FanTranslated) => counts.fan += 1,
                    Some(ChineseMark::Official) => counts.official += 1,
                    None => {}
                }
                insert_game
                    .execute(params![
                        dat_id,
                        game.name,
                        game.key,
                        game.cloneof,
                        game.serial,
                        chinese.map(ChineseMark::label),
                    ])
                    .map_err(wrap)?;
                let game_id = self.tx.last_insert_rowid();
                counts.games += 1;
                for rom in &game.roms {
                    insert_rom
                        .execute(params![
                            game_id,
                            dat_id,
                            rom.name,
                            // SQLite 的整数是有符号 64 位。大小与 CRC-32 都塞得下，
                            // 但得显式折过去——rusqlite 不给 u64/u32 实现 ToSql，
                            // 正是为了逼这一步说清楚。
                            rom.size.and_then(|size| i64::try_from(size).ok()),
                            rom.crc32.map(i64::from),
                            rom.md5,
                            rom.sha1,
                            rom.sha256,
                            rom.status,
                        ])
                        .map_err(wrap)?;
                    counts.roms += 1;
                }
            }
        }

        self.tx
            .execute(
                "UPDATE dat SET games = ?1, roms = ?2, fan = ?3, official = ?4 WHERE id = ?5",
                params![
                    i64::try_from(counts.games).unwrap_or(i64::MAX),
                    i64::try_from(counts.roms).unwrap_or(i64::MAX),
                    i64::try_from(counts.fan).unwrap_or(i64::MAX),
                    i64::try_from(counts.official).unwrap_or(i64::MAX),
                    dat_id,
                ],
            )
            .map_err(wrap)?;
        self.counts = self.counts.plus(counts);
        self.dats += 1;
        Ok(counts)
    }

    /// 落定。返回这件东西一共写进了几份 DAT、多少条。
    ///
    /// # Errors
    /// 提交失败时返回错误。
    pub fn commit(self) -> Result<(u64, DatCounts), RepoError> {
        let path = self.path;
        let counts = self.counts;
        let dats = self.dats;
        self.tx
            .commit()
            .map_err(|source| RepoError::Sqlite { path, source })?;
        Ok((dats, counts))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dat::logiqx::RomRecord;

    fn unit(name: &str, fingerprint: &str) -> Unit {
        Unit {
            source: "TOSEC".to_string(),
            name: name.to_string(),
            url: format!("https://raw.githubusercontent.com/x/y/{name}"),
            fingerprint: fingerprint.to_string(),
        }
    }

    fn meta(name: &str) -> DatMeta {
        DatMeta {
            name: name.to_string(),
            platform: "NGPC".to_string(),
            convention: Convention::AsIs,
            header: DatHeader {
                version: "2023-11-07".to_string(),
                ..DatHeader::default()
            },
        }
    }

    fn game(name: &str, crc: u32) -> GameRecord {
        GameRecord {
            name: name.to_string(),
            roms: vec![RomRecord {
                name: format!("{name}.bin"),
                size: Some(2_097_152),
                crc32: Some(crc),
                ..RomRecord::default()
            }],
            ..GameRecord::default()
        }
    }

    #[test]
    fn 写进去再读回来() {
        let mut repo = DatRepo::in_memory().expect("开得出来");
        assert!(repo.is_empty().expect("读得出"));
        let mut writer = repo.begin(&unit("a.dat", "sha-1111")).expect("开得了事务");
        let counts = writer
            .write_dat(
                &meta("TOSEC/SNK Neo-Geo Pocket Color - Games.dat"),
                &[
                    game(
                        "Metal Slug - 1st Mission (1999)(SNK)(en-ja)[tr zh]",
                        0x8ade_d757,
                    ),
                    game("Normal Game (1999)(SNK)", 1),
                ],
            )
            .expect("写得进");
        assert_eq!(counts.games, 2);
        assert_eq!(counts.roms, 2);
        assert_eq!(counts.fan, 1, "汉化条目要单独数");
        assert_eq!(counts.official, 0);
        writer.commit().expect("提交");

        assert!(!repo.is_empty().expect("读得出"));
        assert_eq!(
            repo.fingerprints("TOSEC").expect("读得出").get("a.dat"),
            Some(&"sha-1111".to_string())
        );
    }

    #[test]
    fn 同一件东西再来一遍是整件换掉不是叠加() {
        // DAT 是整份重新生成的，逐条比对差异毫无意义——而漏删一次，
        // 库里就永远留着一批已经不存在的记录，票 07 会照着它给出候选。
        let mut repo = DatRepo::in_memory().expect("开得出来");
        let mut writer = repo.begin(&unit("a.dat", "sha-1111")).expect("事务");
        writer
            .write_dat(&meta("x"), &[game("旧的", 1), game("也旧", 2)])
            .expect("写");
        writer.commit().expect("提交");

        let mut writer = repo.begin(&unit("a.dat", "sha-2222")).expect("事务");
        writer
            .write_dat(&meta("x"), &[game("新的", 3)])
            .expect("写");
        writer.commit().expect("提交");

        let games: i64 = repo
            .conn()
            .query_row("SELECT COUNT(*) FROM game", [], |row| row.get(0))
            .expect("数得出");
        let roms: i64 = repo
            .conn()
            .query_row("SELECT COUNT(*) FROM rom", [], |row| row.get(0))
            .expect("数得出");
        let units: i64 = repo
            .conn()
            .query_row("SELECT COUNT(*) FROM unit", [], |row| row.get(0))
            .expect("数得出");
        assert_eq!((games, roms, units), (1, 1, 1));
        assert_eq!(
            repo.fingerprints("TOSEC").expect("读得出").get("a.dat"),
            Some(&"sha-2222".to_string())
        );
    }

    #[test]
    fn 没提交就什么都没写进去() {
        let mut repo = DatRepo::in_memory().expect("开得出来");
        {
            let mut writer = repo.begin(&unit("a.dat", "sha")).expect("事务");
            writer
                .write_dat(&meta("x"), &[game("半截", 1)])
                .expect("写");
            // 不 commit，直接丢掉
        }
        assert!(repo.is_empty().expect("读得出"));
        assert!(repo.fingerprints("TOSEC").expect("读得出").is_empty());
    }
}
