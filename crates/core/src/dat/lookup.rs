//! **拿判据去 DAT 库里查**：CRC-32 加大小，第一命中层的全部查询都在这里。
//!
//! 查询走 `rom(crc32)` 上那条索引（票 06 建的），一次查询是一次索引命中——48 万条
//! 文件记录上这一层的代价可以忽略，真正贵的是拿到判据那一步。
//!
//! **判据是 CRC-32 加未压缩大小两件事，不是只有 CRC-32。** 32 位的校验和在几十万条
//! 记录上撞车是现实存在的（生日问题），大小几乎免费地把它挡掉。DAT 偶尔有不记大小
//! 的记录（实测极少），那种命中单独标出来，[`Hit::sized`] 为假——[`super::super::identify`]
//! 据此把置信度降一档，而不是假装它和精确命中一样可靠。
//!
//! ## 还有一条 SHA-1 的窄路（票 10）
//!
//! 库里有一批记录**连 `crc32` 与 `size` 两列都是空的**：GoodNES 那两份实测 30,244 条
//! （FC 22,095、MD 8,149，其中 748 条中文汉化）只记 SHA-1。它们撞不到第一命中层，
//! 不是因为撞不上，是因为根本没有可以对的那一列。[`DatRepo::lookup_sha1`] 专为它们而设，
//! 而且**只查那一批**（`crc32 IS NULL`）——不然每一份 FC 卡都会得到一条与 CRC 那一层
//! 一模一样的重复候选。

use rusqlite::params;

use super::chinese::ChineseMark;
use super::repo::{DatRepo, RepoError};
use super::{Convention, chinese};

/// 一条命中是靠哪一样撞上的。
///
/// **它决定 [`Hit::is_exact`] 怎么算**：CRC-32 那条要大小一起对上才算精确（32 位的
/// 校验和在几十万条记录上会撞车），SHA-1 那条不需要——160 位摆在那儿，而且那批记录
/// 本来就一列大小都没有。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Matched {
    /// CRC-32 加未压缩大小。
    CrcAndSize,
    /// SHA-1。
    Sha1,
}

impl Matched {
    /// 依据里写的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::CrcAndSize => "CRC-32 加大小",
            Self::Sha1 => "SHA-1",
        }
    }
}

/// DAT 库里被撞上的一条文件记录，连同它所在的条目与那份 DAT。
///
/// 它就是一条**候选**的**依据**：命中了哪个数据库的哪条记录、匹配了哪个字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// 哪个数据源。
    pub source: String,
    /// 哪一份 DAT。
    pub dat: String,
    /// 这份 DAT 归哪个平台。
    pub platform: String,
    /// 这份 DAT 的哈希口径。
    pub convention: Convention,
    /// 条目名。**通常是一个发行版**，但 TOSEC 的 `[tr zh]` 条目是**汉化版**，那是变体。
    pub game: String,
    /// 父条目名（No-Intro 的 parent/clone）。同一**作品**下的多个发行版靠它归堆。
    pub cloneof: Option<String>,
    /// 文件记录名。
    pub rom: String,
    /// 序列号；No-Intro 有，Redump 与 TOSEC 没有。
    pub serial: Option<String>,
    /// 条目名上的中文记号：**汉化版**还是**官中版**（ADR-0012）。
    pub chinese: Option<ChineseMark>,
    /// DAT 自己记的大小；`None` 表示这份 DAT 没记。
    pub size: Option<u64>,
    /// DAT 记的 `status`，实测取值 `verified` / `baddump` / `nodump`。
    pub status: Option<String>,
    /// 大小对上了没有。`false` 表示这条记录根本没记大小，只凭 CRC-32 撞上的。
    pub sized: bool,
    /// 靠哪一样撞上的。
    pub matched_by: Matched,
}

impl Hit {
    /// 这条命中是不是**精确**的。
    ///
    /// CRC-32 那条要**大小一起对上**；SHA-1 那条只要撞上就是精确的——那批记录一列大小
    /// 都没有，拿 `sized` 去要求它等于永远不认账。两条都还要 DAT 自己没说这是坏转储。
    #[must_use]
    pub fn is_exact(&self) -> bool {
        let matched = match self.matched_by {
            Matched::CrcAndSize => self.sized,
            Matched::Sha1 => true,
        };
        matched && !matches!(self.status.as_deref(), Some("baddump" | "nodump"))
    }
}

impl DatRepo {
    /// DAT 库覆盖到哪几个平台。
    ///
    /// 识别拿它回答一个花钱的问题：**这个平台值不值得为它读盘**。库里一条记录都没有的
    /// 平台（真机上是 Switch 与街机），读出来的哈希无处可撞。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn platforms(&self) -> Result<std::collections::BTreeSet<String>, RepoError> {
        let mut statement = self
            .conn()
            .prepare("SELECT DISTINCT platform FROM dat")
            .map_err(|source| self.error(source))?;
        let rows = statement
            .query_map(params![], |row| row.get::<_, String>(0))
            .map_err(|source| self.error(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.error(source))
    }

    /// 拿 `(CRC-32, 大小)` 去查，返回全部撞上的文件记录。
    ///
    /// 大小对不上的记录**不返回**——那不是候选，是撞车。DAT 没记大小的记录返回，
    /// 但 [`Hit::sized`] 为假。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn lookup(&self, crc32: u32, size: u64) -> Result<Vec<Hit>, RepoError> {
        let mut statement = self
            .conn()
            .prepare_cached(
                "SELECT d.source, d.name, d.platform, d.convention,
                        g.name, g.cloneof, r.name, g.serial, g.chinese, r.size, r.status
                 FROM rom r
                 JOIN game g ON g.id = r.game
                 JOIN dat  d ON d.id = r.dat
                 WHERE r.crc32 = ?1
                 ORDER BY r.id",
            )
            .map_err(|source| self.error(source))?;
        let rows = statement
            .query_map(params![i64::from(crc32)], |row| {
                let recorded: Option<i64> = row.get(9)?;
                let recorded = recorded.and_then(|size| u64::try_from(size).ok());
                let convention: String = row.get(3)?;
                let game: String = row.get(4)?;
                let chinese = chinese::mark_of(&game);
                Ok(Hit {
                    source: row.get(0)?,
                    dat: row.get(1)?,
                    platform: row.get(2)?,
                    convention: Convention::from_label(&convention).unwrap_or(Convention::AsIs),
                    game,
                    cloneof: row.get(5)?,
                    rom: row.get(6)?,
                    serial: row.get(7)?,
                    chinese,
                    size: recorded,
                    status: row.get(10)?,
                    sized: recorded == Some(size),
                    matched_by: Matched::CrcAndSize,
                })
            })
            .map_err(|source| self.error(source))?;
        let mut out = Vec::new();
        for row in rows {
            let hit = row.map_err(|source| self.error(source))?;
            // 大小对不上就是撞车，不是候选。没记大小的留着，但标出来。
            if hit.size.is_none() || hit.sized {
                out.push(hit);
            }
        }
        Ok(out)
    }

    /// 拿一串 **SHA-1**（四十位小写十六进制）去查，返回全部撞上的文件记录。
    ///
    /// **只查第一命中层够不着的那一批**（`crc32 IS NULL`）。查全部的话，每一份 FC 卡都会
    /// 得到一条与 CRC 那一层内容完全相同的候选——多一条候选不多一分信息，只多一次
    /// 人工裁决时要读的东西。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn lookup_sha1(&self, sha1: &str, size: u64) -> Result<Vec<Hit>, RepoError> {
        let mut statement = self
            .conn()
            .prepare_cached(
                "SELECT d.source, d.name, d.platform, d.convention,
                        g.name, g.cloneof, r.name, g.serial, g.chinese, r.size, r.status
                 FROM rom r
                 JOIN game g ON g.id = r.game
                 JOIN dat  d ON d.id = r.dat
                 WHERE r.sha1 = ?1 AND r.crc32 IS NULL
                 ORDER BY r.id",
            )
            .map_err(|source| self.error(source))?;
        let rows = statement
            .query_map(params![sha1], |row| {
                let recorded: Option<i64> = row.get(9)?;
                let recorded = recorded.and_then(|size| u64::try_from(size).ok());
                let convention: String = row.get(3)?;
                let game: String = row.get(4)?;
                let chinese = chinese::mark_of(&game);
                Ok(Hit {
                    source: row.get(0)?,
                    dat: row.get(1)?,
                    platform: row.get(2)?,
                    convention: Convention::from_label(&convention).unwrap_or(Convention::AsIs),
                    game,
                    cloneof: row.get(5)?,
                    rom: row.get(6)?,
                    serial: row.get(7)?,
                    chinese,
                    size: recorded,
                    status: row.get(10)?,
                    sized: recorded == Some(size),
                    matched_by: Matched::Sha1,
                })
            })
            .map_err(|source| self.error(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.error(source))
    }

    /// 哪几个平台**值得为它算 SHA-1**：库里有第一命中层够不着的记录的那几个。
    ///
    /// 识别拿它回答一个花钱的问题。SHA-1 要把内容整份读一遍，而绝大多数平台的每一条
    /// 记录都带 CRC-32——为它们多算一个摘要换不到任何一条候选。真机实测只有 FC 与 MD
    /// 两个卡带平台在这张名单上（还有 PS1 / SS / DC / Mega-CD 那几个 MAME 的
    /// `<disk>`，但那是光盘，体积上这笔钱不划算，ADR-0008 的修订段算过）。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn sha1_only_platforms(&self) -> Result<std::collections::BTreeSet<String>, RepoError> {
        let mut statement = self
            .conn()
            .prepare(
                "SELECT DISTINCT d.platform FROM rom r JOIN dat d ON d.id = r.dat
                 WHERE r.crc32 IS NULL AND r.sha1 IS NOT NULL",
            )
            .map_err(|source| self.error(source))?;
        let rows = statement
            .query_map(params![], |row| row.get::<_, String>(0))
            .map_err(|source| self.error(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.error(source))
    }
}

/// DAT 库里被**序列号**撞上的一条条目。
///
/// 它和 [`Hit`] 是两件事，所以是两个类型：`Hit` 说的是「这串字节就是那条文件记录」，
/// 而这一条说的是「这张盘里写着的编号就是那次发行的编号」。**它没有文件记录**
/// ——一条 `<game>` 可以有好几个 `<rom>`，而序列号是挂在条目上的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerialHit {
    /// 哪个数据源。
    pub source: String,
    /// 哪一份 DAT。
    pub dat: String,
    /// 这份 DAT 归哪个平台。
    pub platform: String,
    /// 条目名。
    pub game: String,
    /// 父条目名（No-Intro 的 parent/clone）。
    pub cloneof: Option<String>,
    /// DAT 里这个序列号的**原样**写法。依据里写它。
    pub shown: String,
    /// 条目名上的中文记号。
    pub chinese: Option<ChineseMark>,
}

impl DatRepo {
    /// 拿一个**折平后的序列号**去查，返回全部写着它的条目。
    ///
    /// 折平的规矩在 [`serial`](super::serial)：只留字母数字、大写。调用方必须先折过，
    /// 这里不再折一次——两边各折一次的话，哪天改了规矩就会有一边忘掉。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn lookup_serial(&self, serial: &str) -> Result<Vec<SerialHit>, RepoError> {
        let mut statement = self
            .conn()
            .prepare_cached(
                "SELECT d.source, d.name, d.platform, g.name, g.cloneof, s.shown
                 FROM game_serial s
                 JOIN game g ON g.id = s.game
                 JOIN dat  d ON d.id = s.dat
                 WHERE s.serial = ?1
                 ORDER BY g.id",
            )
            .map_err(|source| self.error(source))?;
        let rows = statement
            .query_map(params![serial], |row| {
                let game: String = row.get(3)?;
                let chinese = chinese::mark_of(&game);
                Ok(SerialHit {
                    source: row.get(0)?,
                    dat: row.get(1)?,
                    platform: row.get(2)?,
                    game,
                    cloneof: row.get(4)?,
                    shown: row.get(5)?,
                    chinese,
                })
            })
            .map_err(|source| self.error(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.error(source))
    }

    /// 每个平台索引了多少条**序列号**。
    ///
    /// 报告拿它回答与 `DAT 条目` 那一列同样的问题：**没有弹药的平台认不出来是另一回事**。
    /// 真库实测 Redump 的官方 DAT 一条序列号都不写，于是 PS2 / NGC / WII 这三个平台
    /// 序列号读得出来也无处可撞——那不是这一层不准，是那几份 DAT 里没有这样东西。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn serial_counts(&self) -> Result<std::collections::BTreeMap<String, u64>, RepoError> {
        let mut statement = self
            .conn()
            .prepare(
                "SELECT d.platform, COUNT(*) FROM game_serial s JOIN dat d ON d.id = s.dat
                 GROUP BY d.platform",
            )
            .map_err(|source| self.error(source))?;
        let rows = statement
            .query_map(params![], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                ))
            })
            .map_err(|source| self.error(source))?;
        let mut out = std::collections::BTreeMap::new();
        for row in rows {
            let (platform, count) = row.map_err(|source| self.error(source))?;
            out.insert(platform, count);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dat::logiqx::{DatHeader, GameRecord, RomRecord};
    use crate::dat::repo::{DatMeta, Unit};

    fn 装一份(repo: &mut DatRepo, source: &str, convention: Convention, games: &[GameRecord]) {
        let mut writer = repo
            .begin(&Unit {
                source: source.to_string(),
                name: format!("{source}.dat"),
                url: "https://example.invalid/x".to_string(),
                fingerprint: "sha".to_string(),
            })
            .expect("事务");
        writer
            .write_dat(
                &DatMeta {
                    name: format!("{source} - FC"),
                    platform: "FC".to_string(),
                    convention,
                    header: DatHeader::default(),
                },
                games,
            )
            .expect("写");
        writer.commit().expect("提交");
    }

    fn 条目(name: &str, size: Option<u64>, crc: u32) -> GameRecord {
        GameRecord {
            name: name.to_string(),
            roms: vec![RomRecord {
                name: format!("{name}.nes"),
                size,
                crc32: Some(crc),
                ..RomRecord::default()
            }],
            ..GameRecord::default()
        }
    }

    #[test]
    fn 说得出库里有哪几个平台() {
        // 识别拿它决定「这个平台值不值得为它读盘」。
        let mut repo = DatRepo::in_memory().expect("开得出来");
        assert!(repo.platforms().expect("查得出").is_empty());
        装一份(
            &mut repo,
            "No-Intro",
            Convention::Headerless,
            &[条目("甲 (Japan)", Some(1), 1)],
        );
        assert!(repo.platforms().expect("查得出").contains("FC"));
        assert!(!repo.platforms().expect("查得出").contains("SWITCH"));
    }

    #[test]
    fn 判据是_crc32_加大小两件事() {
        let mut repo = DatRepo::in_memory().expect("开得出来");
        装一份(
            &mut repo,
            "No-Intro",
            Convention::Headerless,
            &[条目("甲 (Japan)", Some(40_960), 0xAABB_CCDD)],
        );
        assert_eq!(repo.lookup(0xAABB_CCDD, 40_960).expect("查得出").len(), 1);
        // CRC 一样、大小不一样：撞车，不是候选。
        assert!(repo.lookup(0xAABB_CCDD, 40_976).expect("查得出").is_empty());
        assert!(repo.lookup(0x0000_0001, 40_960).expect("查得出").is_empty());
    }

    #[test]
    fn 一份内容可以撞上好几个源() {
        let mut repo = DatRepo::in_memory().expect("开得出来");
        装一份(
            &mut repo,
            "No-Intro",
            Convention::Headerless,
            &[条目("甲 (Japan)", Some(40_960), 1)],
        );
        装一份(
            &mut repo,
            "TOSEC",
            Convention::AsIs,
            &[条目("甲 (1990)(某社)[tr zh 某组]", Some(40_960), 1)],
        );
        let hits = repo.lookup(1, 40_960).expect("查得出");
        assert_eq!(hits.len(), 2, "一个变体可以有多条候选");
        assert_eq!(hits[0].convention, Convention::Headerless);
        assert_eq!(hits[1].convention, Convention::AsIs);
        assert_eq!(hits[1].chinese, Some(ChineseMark::FanTranslated));
        assert!(hits.iter().all(Hit::is_exact));
    }

    #[test]
    fn 没记大小的命中标出来而不是当精确命中() {
        let mut repo = DatRepo::in_memory().expect("开得出来");
        装一份(
            &mut repo,
            "TOSEC",
            Convention::AsIs,
            &[条目("没记大小的", None, 7)],
        );
        let hits = repo.lookup(7, 12_345).expect("查得出");
        assert_eq!(hits.len(), 1);
        assert!(!hits[0].sized);
        assert!(!hits[0].is_exact(), "只凭 CRC-32 撞上的不算精确命中");
    }

    #[test]
    fn 坏转储不算精确命中() {
        let mut repo = DatRepo::in_memory().expect("开得出来");
        let mut game = 条目("坏的 (Japan)", Some(16), 9);
        game.roms[0].status = Some("baddump".to_string());
        装一份(&mut repo, "No-Intro", Convention::AsIs, &[game]);
        let hits = repo.lookup(9, 16).expect("查得出");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].sized);
        assert!(!hits[0].is_exact(), "DAT 自己说这是坏转储");
    }
}
