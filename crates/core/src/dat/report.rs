//! **DAT 仓库的报告**：每个平台手上有多少弹药。
//!
//! 这份报告要回答的是「票 07 开工前，每个平台能指望多少条记录」。它按**平台**与
//! **源**两个维度铺开，因为两者都会决定该先做哪个平台：
//!
//! - 平台那一列对着 `docs/library-facts.md` 里的变体数——FC 有 20,516 个变体，
//!   有没有两万条 DAT 记录接得住，是完全不同的两件事。
//! - 源那一列区分**汉化**与**官中**（ADR-0012）。把两者加成一个「中文条目数」
//!   会让「TOSEC 补的正是官方库覆盖不到的那部分」这个判断彻底失真。

use std::collections::BTreeMap;
use std::fmt::Write as _;

use rusqlite::params;
use serde::Serialize;

use super::repo::{DatRepo, RepoError};
use crate::report::{pad, thousands};

/// 一份 DAT 在报告里的一行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DatRow {
    /// 哪个源。
    pub source: String,
    /// DAT 名。
    pub name: String,
    /// 归哪个平台。
    pub platform: String,
    /// 哈希口径。
    pub convention: String,
    /// DAT 自称的版本。
    pub version: String,
    /// 条目数。
    pub games: u64,
    /// 文件记录数。
    pub roms: u64,
    /// 汉化条目数。
    pub fan: u64,
    /// 官中条目数。
    pub official: u64,
}

/// 一组（平台或源）的合计。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GroupRow {
    /// 组名：平台名，或者源名。
    pub name: String,
    /// 几份 DAT。
    pub dats: u64,
    /// 条目数。
    pub games: u64,
    /// 文件记录数。
    pub roms: u64,
    /// 汉化条目数。
    pub fan: u64,
    /// 官中条目数。
    pub official: u64,
    /// 这一组的记录来自哪几个源（按源分组时是这一组覆盖哪几个平台）。
    pub from: Vec<String>,
}

/// DAT 仓库的报告。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DatReport {
    /// DAT 库在哪。
    pub location: String,
    /// 按平台。
    pub platforms: Vec<GroupRow>,
    /// 按源。
    pub sources: Vec<GroupRow>,
    /// 每一份 DAT。
    pub dats: Vec<DatRow>,
    /// 合计。
    pub totals: GroupRow,
}

impl DatReport {
    /// 从 DAT 库折出一份报告。**不联网**。
    ///
    /// # Errors
    /// 库读不出来时返回错误。
    pub fn build(repo: &DatRepo) -> Result<Self, RepoError> {
        let mut dats = Vec::new();
        {
            let mut statement = repo
                .conn()
                .prepare(
                    "SELECT source, name, platform, convention, version, games, roms, fan, official
                     FROM dat ORDER BY platform, source, name",
                )
                .map_err(|source| RepoError::Sqlite {
                    path: repo.location(),
                    source,
                })?;
            let rows = statement
                .query_map(params![], |row| {
                    Ok(DatRow {
                        source: row.get(0)?,
                        name: row.get(1)?,
                        platform: row.get(2)?,
                        convention: row.get(3)?,
                        version: row.get(4)?,
                        games: row.get::<_, i64>(5)?.unsigned_abs(),
                        roms: row.get::<_, i64>(6)?.unsigned_abs(),
                        fan: row.get::<_, i64>(7)?.unsigned_abs(),
                        official: row.get::<_, i64>(8)?.unsigned_abs(),
                    })
                })
                .map_err(|source| RepoError::Sqlite {
                    path: repo.location(),
                    source,
                })?;
            for row in rows {
                dats.push(row.map_err(|source| RepoError::Sqlite {
                    path: repo.location(),
                    source,
                })?);
            }
        }

        let platforms = group(&dats, |row| (&row.platform, &row.source));
        let sources = group(&dats, |row| (&row.source, &row.platform));
        let mut totals = GroupRow {
            name: "合计".to_string(),
            ..GroupRow::default()
        };
        for row in &dats {
            totals.dats += 1;
            totals.games += row.games;
            totals.roms += row.roms;
            totals.fan += row.fan;
            totals.official += row.official;
        }

        Ok(Self {
            location: repo.location(),
            platforms,
            sources,
            dats,
            totals,
        })
    }

    /// 渲染成给人看的文本。
    #[must_use]
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "DAT 仓库");
        let _ = writeln!(out, "{}", "═".repeat(20));
        let _ = writeln!(out, "DAT 库          {}", self.location);
        let _ = writeln!(
            out,
            "合计            {} 份 DAT，{} 条条目，{} 条文件记录",
            thousands(self.totals.dats),
            thousands(self.totals.games),
            thousands(self.totals.roms)
        );
        let _ = writeln!(
            out,
            "中文            汉化 {} 条，官中 {} 条",
            thousands(self.totals.fan),
            thousands(self.totals.official)
        );

        if self.dats.is_empty() {
            let _ = writeln!(out, "\n库里还没有 DAT。先跑一次 `romcat dat sync`。");
            return out;
        }

        section(&mut out, "按平台", "平台", &self.platforms, "源");
        section(&mut out, "按源", "源", &self.sources, "平台");
        out
    }
}

fn group<'a>(
    dats: &'a [DatRow],
    key: impl Fn(&'a DatRow) -> (&'a String, &'a String),
) -> Vec<GroupRow> {
    let mut groups: BTreeMap<&str, GroupRow> = BTreeMap::new();
    for row in dats {
        let (name, other) = key(row);
        let entry = groups.entry(name.as_str()).or_insert_with(|| GroupRow {
            name: name.clone(),
            ..GroupRow::default()
        });
        entry.dats += 1;
        entry.games += row.games;
        entry.roms += row.roms;
        entry.fan += row.fan;
        entry.official += row.official;
        if !entry.from.iter().any(|seen| seen == other) {
            entry.from.push(other.clone());
        }
    }
    let mut rows: Vec<GroupRow> = groups.into_values().collect();
    // 条目多的排前面：那正是「先做哪个平台」要看的顺序。
    rows.sort_by(|a, b| b.games.cmp(&a.games).then_with(|| a.name.cmp(&b.name)));
    rows
}

fn section(out: &mut String, title: &str, head: &str, rows: &[GroupRow], other: &str) {
    let _ = write!(
        out,
        "\n{title}\n{}\n",
        "─".repeat(title.chars().count() + 8)
    );
    let _ = writeln!(
        out,
        "{}{}{}{}{}{}",
        pad(head, 10),
        pad("DAT", 6),
        pad("条目", 12),
        pad("文件", 12),
        pad("汉化", 8),
        pad("官中", 8),
    );
    for row in rows {
        let _ = writeln!(
            out,
            "{}{}{}{}{}{}{}",
            pad(&row.name, 10),
            pad(&thousands(row.dats), 6),
            pad(&thousands(row.games), 12),
            pad(&thousands(row.roms), 12),
            pad(&thousands(row.fan), 8),
            pad(&thousands(row.official), 8),
            row.from.join("、"),
        );
    }
    let _ = writeln!(out, "（末列是这一组涉及的{other}）");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dat::Convention;
    use crate::dat::logiqx::{DatHeader, GameRecord, RomRecord};
    use crate::dat::repo::{DatMeta, Unit};

    fn game(name: &str) -> GameRecord {
        GameRecord {
            name: name.to_string(),
            roms: vec![RomRecord {
                name: format!("{name}.bin"),
                crc32: Some(1),
                ..RomRecord::default()
            }],
            ..GameRecord::default()
        }
    }

    fn seeded() -> DatRepo {
        let mut repo = DatRepo::in_memory().expect("开得出来");
        let mut writer = repo
            .begin(&Unit {
                source: "TOSEC".to_string(),
                name: "TOSEC/SNK Neo-Geo Pocket Color - Games.dat".to_string(),
                url: "https://raw.githubusercontent.com/x/y/z".to_string(),
                fingerprint: "sha".to_string(),
            })
            .expect("事务");
        writer
            .write_dat(
                &DatMeta {
                    name: "SNK Neo-Geo Pocket Color - Games".to_string(),
                    platform: "NGPC".to_string(),
                    convention: Convention::AsIs,
                    header: DatHeader::default(),
                },
                &[
                    game("Metal Slug - 1st Mission (1999)(SNK)(en-ja)[tr zh]"),
                    game("Dark Arms - Beast Buster 1999 (1999)(SNK)(en-ja)[tr zh]"),
                    game("Something (1999)(SNK)"),
                ],
            )
            .expect("写");
        writer.commit().expect("提交");

        let mut writer = repo
            .begin(&Unit {
                source: "No-Intro".to_string(),
                name: "no-intro.zip".to_string(),
                url: "https://github.com/x/y".to_string(),
                fingerprint: "1".to_string(),
            })
            .expect("事务");
        writer
            .write_dat(
                &DatMeta {
                    name: "SNK - NeoGeo Pocket Color".to_string(),
                    platform: "NGPC".to_string(),
                    convention: Convention::AsIs,
                    header: DatHeader::default(),
                },
                &[game("Some Game (Japan) (Zh)"), game("Other (USA)")],
            )
            .expect("写");
        writer.commit().expect("提交");
        repo
    }

    #[test]
    fn 按平台与按源两个维度都数得出() {
        let repo = seeded();
        let report = DatReport::build(&repo).expect("折得出报告");
        assert_eq!(report.totals.dats, 2);
        assert_eq!(report.totals.games, 5);
        // 汉化与官中**分开数**——加成一个数就看不出 TOSEC 补的是哪一块了。
        assert_eq!(report.totals.fan, 2);
        assert_eq!(report.totals.official, 1);

        assert_eq!(report.platforms.len(), 1);
        let ngpc = &report.platforms[0];
        assert_eq!(ngpc.name, "NGPC");
        assert_eq!(ngpc.games, 5);
        assert_eq!(ngpc.fan, 2);
        assert_eq!(ngpc.from, ["No-Intro", "TOSEC"]);

        let tosec = report
            .sources
            .iter()
            .find(|row| row.name == "TOSEC")
            .expect("有 TOSEC");
        assert_eq!((tosec.games, tosec.fan, tosec.official), (3, 2, 0));
        let nointro = report
            .sources
            .iter()
            .find(|row| row.name == "No-Intro")
            .expect("有 No-Intro");
        assert_eq!((nointro.games, nointro.fan, nointro.official), (2, 0, 1));
    }

    #[test]
    fn 空库的报告说得清下一步() {
        let repo = DatRepo::in_memory().expect("开得出来");
        let text = DatReport::build(&repo).expect("折得出").render_text();
        assert!(text.contains("romcat dat sync"), "{text}");
    }

    #[test]
    fn 文本报告把汉化与官中分两列() {
        let text = DatReport::build(&seeded()).expect("折得出").render_text();
        assert!(text.contains("汉化"), "{text}");
        assert!(text.contains("官中"), "{text}");
        assert!(text.contains("NGPC"), "{text}");
    }
}
