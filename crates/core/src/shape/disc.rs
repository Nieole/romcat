//! **一个多碟变体里的各张碟**：每张碟是哪一份主文件、按什么次序（票 `verdict-store-and-sync/11`）。
//!
//! 同步把一个多碟变体送上卡时多生成一份 `.m3u`，按碟序列出每张碟的主文件（`sync::playlist`）。「这个变体是不是
//! 多碟、各张碟的主文件是哪几份」是**成型**的判断：多碟同族把几个变体合成一个时，每张碟原本是一个变体、有它自己的
//! 主文件；合完之后别的碟的主文件降成了附属文件（一个变体只有一个主文件），中立库里于是不再记着谁是哪张碟。
//!
//! 这里**拿同一份成型把那几条成员再走一遍**（多碟同族之前那几步，[`super::shape_singly`]）——各自成型出来的那几个
//! 主文件，就是各张碟。不另立一份「哪个扩展名算碟」的判据（ADR-0024）：`.cue` 加 `.bin` 那一组的主文件是 `.cue`、
//! `.chd` 自己就是一张碟、分卷的主文件是入口卷，全由成型那一份规则说了算。
//!
//! 人工纠正合成的（[`super::fix::merge`]）与多碟同族合成的走同一条路：成员表里的身份都只剩主文件与附属文件。

use std::cmp::Ordering;
use std::collections::BTreeMap;

use super::{Entry, Role, shape_singly};
use crate::platform::Manifest;
use crate::platform::pattern::Pattern;

/// 一个变体里的**各张碟**：每张碟一个主文件，按碟序；不到两张（不是多碟）时是空的。
///
/// `members` 是这个变体的**文件**成员连它们的身份（目录成员别交进来）。只有主文件与附属文件算数——**内部资源**
/// 与**附属内容**是一份转储自己的数据，不是一张张碟（目录树变体于是一张碟都认不出来）。
///
/// **纯函数**：不碰磁盘、不读库。
#[must_use]
pub fn discs(members: &[(String, Role)], manifest: &Manifest) -> Vec<String> {
    let entries: Vec<Entry> = members
        .iter()
        .filter(|(_, role)| matches!(role, Role::Main | Role::Companion))
        .map(|(key, _)| Entry {
            key: key.clone(),
            is_dir: false,
            len: None,
        })
        .collect();
    let mut keys: Vec<String> = shape_singly(&entries, manifest, &BTreeMap::new())
        .variants
        .into_iter()
        .map(|variant| variant.main_key)
        .collect();
    if keys.len() < 2 {
        return Vec::new();
    }
    // **碟序看碟片标记，不看字面**：先比文件名末尾剥下来的那一截标记（没有标记的排最前——真实的转储常常是
    // `游戏.chd` 加 `游戏 (Disc 2).chd`，碟 1 不写标记），数字按数值比（`(Disc 10)` 排在 `(Disc 2)` 后面）；
    // 标记一样（比如标记只写在目录名上）再按整条键比，数字照样按数值。
    let markers = super::disc_markers(manifest);
    keys.sort_by(|a, b| {
        numeric_aware(marker_of(a, &markers), marker_of(b, &markers))
            .then_with(|| numeric_aware(a, b))
    });
    keys
}

/// 一条键的文件名（去掉扩展名）末尾那一截**碟片标记**；没有标记时是空串。
fn marker_of<'k>(key: &'k str, markers: &[Pattern]) -> &'k str {
    let stem = super::base_stem(super::last_component(key));
    // 剥出来的永远是 `stem` 的一个前缀（[`super::strip_markers`] 只从尾巴上剥），剩下的那一截就是标记。
    let kept = super::strip_markers(stem, markers).len();
    stem.get(kept..).unwrap_or("")
}

/// 两串字比先后，**连着的一段数字按数值比**：`Disc 2` 排在 `Disc 10` 前面。其余逐字比。
fn numeric_aware(a: &str, b: &str) -> Ordering {
    let mut left = a.chars().peekable();
    let mut right = b.chars().peekable();
    loop {
        match (left.peek().copied(), right.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |chars: &mut std::iter::Peekable<std::str::Chars<'_>>| {
                    let mut digits = String::new();
                    while let Some(digit) = chars.next_if(char::is_ascii_digit) {
                        digits.push(digit);
                    }
                    digits
                };
                let (x, y) = (take(&mut left), take(&mut right));
                let (x, y) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                let order = x.len().cmp(&y.len()).then_with(|| x.cmp(y));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                left.next();
                right.next();
            }
        }
    }
}

/// 一个文件名去掉扩展名、再剥掉末尾的**碟片标记**之后的那一截：`游戏 (Disc 1).cue` → `游戏`。
///
/// 碟片标记取平台清单里**全部多碟同族规则**的那一套（与成型存疑「多碟没合在一起」同一套），剥法与多碟同族那条规则
/// 同一个（只认结尾）。没有标记时就是去掉扩展名的名字本身。
#[must_use]
pub fn disc_family(name: &str, manifest: &Manifest) -> String {
    let markers = super::disc_markers(manifest);
    super::strip_markers(super::base_stem(name), &markers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 清单() -> Manifest {
        Manifest::builtin()
    }

    fn 成员(members: &[(&str, Role)]) -> Vec<(String, Role)> {
        members
            .iter()
            .map(|(key, role)| ((*key).to_string(), *role))
            .collect()
    }

    #[test]
    fn 同一个目录里两张碟各一份_cue_加_bin_每张碟的主文件是_cue() {
        let 变体 = 成员(&[
            ("库/ps/某游戏/游戏 (Disc 1).bin", Role::Companion),
            ("库/ps/某游戏/游戏 (Disc 1).cue", Role::Main),
            ("库/ps/某游戏/游戏 (Disc 2).bin", Role::Companion),
            ("库/ps/某游戏/游戏 (Disc 2).cue", Role::Companion),
        ]);
        assert_eq!(
            discs(&变体, &清单()),
            [
                "库/ps/某游戏/游戏 (Disc 1).cue",
                "库/ps/某游戏/游戏 (Disc 2).cue"
            ]
        );
    }

    #[test]
    fn 碟序按碟号数_第十张排在第二张后面() {
        let 变体: Vec<(String, Role)> = (1..=10)
            .map(|碟| {
                (
                    format!("库/ss/某游戏/游戏 (Disc {碟}).chd"),
                    if 碟 == 1 {
                        Role::Main
                    } else {
                        Role::Companion
                    },
                )
            })
            .collect();
        let 认出来 = discs(&变体, &清单());
        assert_eq!(认出来.len(), 10);
        assert_eq!(认出来[1], "库/ss/某游戏/游戏 (Disc 2).chd");
        assert_eq!(
            认出来[9], "库/ss/某游戏/游戏 (Disc 10).chd",
            "按字面排 (Disc 10) 会挤到 (Disc 2) 前面"
        );
    }

    #[test]
    fn 头一张碟不写标记的那一种_它排在最前() {
        // 真实的转储常常是 `游戏.chd` 加 `游戏 (Disc 2).chd`（多碟同族那条规则的注释）。按字面排，空格比点小，
        // `(Disc 2)` 会排到前面去。
        let 变体 = 成员(&[
            ("库/ps/游戏 (Disc 2).chd", Role::Main),
            ("库/ps/游戏.chd", Role::Companion),
        ]);
        assert_eq!(
            discs(&变体, &清单()),
            ["库/ps/游戏.chd", "库/ps/游戏 (Disc 2).chd"]
        );
    }

    #[test]
    fn 几张碟分在几个目录里_照样一张一份() {
        // 真库里就是这样：`…]Disc A/` 与 `…]Disc B/` 是两个兄弟目录（多碟同族那条规则的测试）。
        let 变体 = 成员(&[
            (
                "库/ps/龙骑士传说[简]Disc B/Legend (Disc 2).chd",
                Role::Companion,
            ),
            ("库/ps/龙骑士传说[简]Disc A/Legend (Disc 1).chd", Role::Main),
        ]);
        assert_eq!(
            discs(&变体, &清单()),
            [
                "库/ps/龙骑士传说[简]Disc A/Legend (Disc 1).chd",
                "库/ps/龙骑士传说[简]Disc B/Legend (Disc 2).chd",
            ]
        );
    }

    #[test]
    fn 一张碟的多轨镜像不是多碟() {
        let 变体 = 成员(&[
            ("库/ps/游戏.cue", Role::Main),
            ("库/ps/游戏 (Track 01).bin", Role::Companion),
            ("库/ps/游戏 (Track 02).bin", Role::Companion),
        ]);
        assert!(discs(&变体, &清单()).is_empty());
    }

    #[test]
    fn 目录树转储里的内部资源不是一张张碟() {
        // PS3 的转储：主文件是那个目录，里面的文件都是内部资源。
        let 变体 = 成员(&[
            ("库/ps3/某游戏", Role::Main),
            ("库/ps3/某游戏/PS3_GAME/USRDIR/EBOOT.BIN", Role::Internal),
            ("库/ps3/某游戏/PS3_GAME/USRDIR/data.iso", Role::Internal),
            ("库/ps3/某游戏/PS3_UPDATE/game.iso", Role::Internal),
        ]);
        assert!(discs(&变体, &清单()).is_empty());
    }

    #[test]
    fn 剥掉碟片标记的名字() {
        assert_eq!(disc_family("游戏 (Disc 1).cue", &清单()), "游戏");
        assert_eq!(disc_family("Legend (Disc 2).chd", &清单()), "Legend");
        assert_eq!(
            disc_family("游戏.chd", &清单()),
            "游戏",
            "没有标记就是名字本身"
        );
    }
}
