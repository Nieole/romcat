//! **附属文件落单**（词表同名条目，2026-09-15 拿主意的人定）：存档、补丁这类**只对某一个主文件有意义**的文件，
//! 在它自己那个目录里找不到同名的主文件。
//!
//! ## 判据
//!
//! 1. **哪几种文件**：存档（[`classify::is_save`]）与补丁（[`classify::patch_extension`]——与识别跳过补丁看的是同一张表，
//!    ADR-0024）。
//! 2. **在范围之内**：落在平台目录里（ADR-0011 修订段）。未纳入管理的目录不进识别管线，那里的存档不在这条判据里。
//! 3. **没进任何变体**：人工纠正把它并进了某个变体，它就有了主人。
//! 4. **同一目录里没有同名的主文件**：去掉扩展名之后名字相同（大小写不敏感、比较前过 NFC）的**内容**文件——透明容器、
//!    压缩镜像、裸文件（成型那一侧的 `is_content`，同一处判）。模拟器正是照这个约定找存档与软补丁的：`游戏.gba` 旁边的
//!    `游戏.sav`、`游戏.ips`。
//!
//! 落单时再看一眼**同一平台目录里**别处有没有同名的主文件：有就说出是哪个目录——人整理时最先要知道的是这个；没有就是
//! 找不到。
//!
//! ## 只发现并报告
//!
//! 它不改成型、不挪文件（ADR-0004）；成型那一侧也不因为它把存档挂进变体——那是另一件事，这里只回答「哪些落了单」。

use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::{Deserialize, Serialize};

use super::{Entry, base_stem, is_content, parent_of, scope_of};
use crate::classify;
use crate::path::{file_name_of_key, fold, platform_dir_of_key};
use crate::platform::Manifest;

/// 落单的是哪一种文件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CompanionKind {
    /// 模拟器的**存档**。
    Save,
    /// **补丁**（词表同名条目）。
    Patch,
}

impl CompanionKind {
    /// 报告与界面上用的名字。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Save => "存档",
            Self::Patch => "补丁",
        }
    }

    /// 这个文件是哪一种；既不是存档也不是补丁时是 `None`。
    fn of(name: &str) -> Option<Self> {
        if classify::is_save(name) {
            Some(Self::Save)
        } else if classify::patch_extension(name).is_some() {
            Some(Self::Patch)
        } else {
            None
        }
    }
}

/// 一个**落单的附属文件**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stranded {
    /// 它在中立库里的键。
    pub key: String,
    /// 哪一种。
    pub kind: CompanionKind,
    /// 同一平台目录里、别的目录下有同名的主文件时，那个目录的键（有好几处时取键最小的那一处）；找不到是 `None`。
    pub main_elsewhere: Option<String>,
}

/// 找出**落单的附属文件**，按键排（判据见模块文档）。
///
/// `entries` 是中立库的条目，目录条目不看；`in_variant` 是进了某个变体的那些键。**纯函数**：不碰磁盘、不读库。
#[must_use]
pub fn stranded_companions(
    entries: &[Entry],
    in_variant: &HashSet<String>,
    manifest: &Manifest,
) -> Vec<Stranded> {
    let files = || {
        entries
            .iter()
            .filter(|entry| !entry.is_dir && scope_of(manifest, &entry.key).in_scope())
    };
    // 范围之内的主文件候选：（所在目录, 折过的主干），以及（平台目录, 折过的主干）→ 它们落在哪几个目录。
    let mut here: HashSet<(&str, String)> = HashSet::new();
    let mut in_platform: BTreeMap<(&str, String), BTreeSet<&str>> = BTreeMap::new();
    for entry in files() {
        if !is_content(&entry.key) {
            continue;
        }
        let (Some(dir), Some(platform_dir)) =
            (parent_of(&entry.key), platform_dir_of_key(&entry.key))
        else {
            continue;
        };
        let stem = stem_of(&entry.key);
        here.insert((dir, stem.clone()));
        in_platform
            .entry((platform_dir, stem))
            .or_default()
            .insert(dir);
    }

    let mut out = Vec::new();
    for entry in files() {
        if in_variant.contains(&entry.key) {
            continue;
        }
        let Some(kind) = CompanionKind::of(&entry.key) else {
            continue;
        };
        let (Some(dir), Some(platform_dir)) =
            (parent_of(&entry.key), platform_dir_of_key(&entry.key))
        else {
            continue;
        };
        let stem = stem_of(&entry.key);
        if here.contains(&(dir, stem.clone())) {
            continue;
        }
        let main_elsewhere = in_platform
            .get(&(platform_dir, stem))
            .and_then(|dirs| dirs.iter().next())
            .map(|dir| (*dir).to_string());
        out.push(Stranded {
            key: entry.key.clone(),
            kind,
            main_elsewhere,
        });
    }
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out
}

/// 去掉扩展名之后的名字，折成可比较的形态（大小写不敏感、NFC）。
fn stem_of(key: &str) -> String {
    fold(base_stem(file_name_of_key(key)))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashSet};

    use super::*;
    use crate::platform::Manifest;
    use crate::shape::Entry;

    /// 一组文件条目：路径相对根「库」写，这里接上根名。
    fn 条目(相对: &[&str]) -> Vec<Entry> {
        相对
            .iter()
            .map(|key| Entry {
                key: format!("库/{key}"),
                is_dir: false,
                len: Some(1024),
            })
            .collect()
    }

    /// 落单的那几条：（键去掉根名, 哪一种, 主文件在另一个目录时是哪个目录）。
    fn 落单(
        相对: &[&str],
        进了变体的: &[&str],
    ) -> BTreeSet<(String, CompanionKind, Option<String>)> {
        let 进了: HashSet<String> = 进了变体的.iter().map(|key| format!("库/{key}")).collect();
        stranded_companions(&条目(相对), &进了, &Manifest::builtin())
            .into_iter()
            .map(|one| {
                (
                    one.key.trim_start_matches("库/").to_string(),
                    one.kind,
                    one.main_elsewhere
                        .map(|dir| dir.trim_start_matches("库/").to_string()),
                )
            })
            .collect()
    }

    #[test]
    fn 同目录里没有同名主文件的存档与补丁落单() {
        let 结论 = 落单(
            &[
                "GBA/汉化/火焰之纹章 圣魔之光石.sav",
                "FC/【中文游戏】/热血格斗.ips",
                "GBA/汉化/逆转裁判.gba",
            ],
            &[],
        );
        assert_eq!(
            结论,
            BTreeSet::from([
                (
                    "GBA/汉化/火焰之纹章 圣魔之光石.sav".to_string(),
                    CompanionKind::Save,
                    None
                ),
                (
                    "FC/【中文游戏】/热血格斗.ips".to_string(),
                    CompanionKind::Patch,
                    None
                ),
            ])
        );
    }

    #[test]
    fn 同名主文件在同一平台的另一个目录时_说出是哪个目录() {
        let 结论 = 落单(&["SFC/汉化/时空之轮.srm", "SFC/日版/时空之轮.sfc"], &[]);
        assert_eq!(
            结论,
            BTreeSet::from([(
                "SFC/汉化/时空之轮.srm".to_string(),
                CompanionKind::Save,
                Some("SFC/日版".to_string())
            )])
        );
    }

    #[test]
    fn 同目录里有同名主文件的不算落单_主文件是透明容器也算() {
        let 结论 = 落单(
            &[
                "GBA/汉化/黄金太阳.sav",
                "GBA/汉化/黄金太阳.gba",
                "SFC/汉化/时空之轮.IPS",
                "SFC/汉化/时空之轮.zip",
            ],
            &[],
        );
        assert!(结论.is_empty(), "{结论:?}");
    }

    #[test]
    fn 进了变体的_范围之外的_不是存档补丁的_都不算落单() {
        let 结论 = 落单(
            &[
                // 人工纠正把它并进了某个变体：它不落单。
                "GBA/汉化/人工并过去的.sav",
                // 未纳入管理的顶层目录不进识别管线（ADR-0011 修订段），那里的存档不在这条判据里。
                "存档/火焰之纹章.sav",
                // 不是存档、也不是补丁。
                "FC/说明.txt",
                "FC/封面.png",
            ],
            &["GBA/汉化/人工并过去的.sav"],
        );
        assert!(结论.is_empty(), "{结论:?}");
    }
}
