//! **成型存疑**（词表同名条目，2026-09-15 拿主意的人定）：成型规则把文件聚成变体时拿不准的地方。
//!
//! 成型规则会出错（词表**成型规则**），人工纠正是正门；这一处只负责把**多半出了错**的地方指出来，交给人看。两种：
//!
//! ## 多碟没合在一起
//!
//! **同一个目录**里、**不是人工纠正出来的**两个以上变体，名字末尾的碟片标记剥掉之后完全相同，而其中至少一个真的带着
//! 碟片标记——它们多半是同一套多碟游戏，却各自成了变体。
//!
//! - 碟片标记取平台清单里**全部多碟同族规则**的那一套（`platforms.toml`，数据不是代码），剥法与多碟同族同一个
//!   （`strip_markers`，只认结尾、扩展名留着）。声明了多碟同族的平台，规则已经合上的不会再出现在这里；没声明的平台
//!   （FDS 的两面磁碟）、或者标记没对上规则的，才落到这里。
//! - **只认同一个目录**：分在两个兄弟目录里的几张碟由多碟同族那条规则去合，这里不猜。
//!
//! ## 一个目录被当成了一个变体
//!
//! 一个**目录树**规则成出来、**不是人工纠正出来的**变体，它的目录**直接**躺着两个以上**各自独立的内容**，而且名字
//! （去掉扩展名）各不相同——那个目录多半是装着好几个游戏的合集，被当成了一份转储。
//!
//! 「各自独立的内容」**宁可窄不宜宽**：透明容器、压缩镜像、光盘镜像 `.iso`，或者清单里「只可能属于某一个平台」的扩展名
//! （`.vpk`、`.xci` 之流）。`.bin`、`.sfo` 这类同时也是转储自己的数据文件（PSV 的 `eboot.bin`），不算；分卷的非入口段
//! 跟着入口段算一份。**直接躺着**只看目录这一层：转储的分区目录（`app/`、`PS3_GAME/`）底下是它自己的内部资源。
//!
//! ## 只发现并报告
//!
//! 两种都不改成型、不挪文件（ADR-0004），也不自己纠正——人工纠正落沉淀库，是人点头的事（票 29）。

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{Entry, base_stem, last_component, parent_of, scope_of, strip_markers};
use crate::classify::{self, Category};
use crate::path::{extension_lower, file_name_of_key, fold};
use crate::platform::pattern::Pattern;
use crate::platform::{Manifest, ShapeKind};

/// 成型存疑是哪一种。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DoubtKind {
    /// 同一个目录里只差碟片标记的几个变体，各自成了变体。
    UnmergedDiscs,
    /// 目录树成出来的一个变体里直接躺着好几个各自独立的内容。
    CrowdedTree,
}

impl DoubtKind {
    /// 报告与界面上用的名字。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::UnmergedDiscs => "多碟没合在一起",
            Self::CrowdedTree => "一个目录被当成一个变体",
        }
    }
}

/// 成型存疑那条判据看一个变体的哪几样：键、哪条规则成的型、是不是人工纠正出来的。
///
/// 摆成借用的一小份而不是整个变体：成型当场出的 [`super::Variant`] 与中立库读回来的变体记录都折得出这三样。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shaped<'a> {
    /// 变体的键。
    pub key: &'a str,
    /// 哪条成型规则成的型（名字）。
    pub rule: &'a str,
    /// 是不是人工纠正出来的。
    pub manual: bool,
}

/// 一处**成型存疑**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Doubt {
    /// 哪一种。
    pub kind: DoubtKind,
    /// 那一处：多碟没合在一起时是那几个变体所在的目录的键；一个目录被当成一个变体时是那个变体（目录）的键。
    pub at: String,
    /// 牵涉的那几条，按键排：几个变体的键；或者目录里那几个独立内容的键。
    pub items: Vec<String>,
}

/// 找出**成型存疑**的地方，按那一处的键排（判据见模块文档）。
///
/// `variants` 是成型出来的变体，`entries` 是中立库的条目。**纯函数**：不碰磁盘、不读库。
#[must_use]
pub fn shaping_doubts(
    variants: &[Shaped<'_>],
    entries: &[Entry],
    manifest: &Manifest,
) -> Vec<Doubt> {
    let mut out = unmerged_discs(variants, manifest);
    out.extend(crowded_trees(variants, entries, manifest));
    out.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.kind.cmp(&b.kind)));
    out
}

/// 多碟没合在一起的那几处。
fn unmerged_discs(variants: &[Shaped<'_>], manifest: &Manifest) -> Vec<Doubt> {
    let markers: Vec<Pattern> = manifest
        .rules()
        .iter()
        .filter(|rule| rule.kind == ShapeKind::DiscFamily)
        .flat_map(|rule| rule.disc_markers.iter().cloned())
        .collect();
    if markers.is_empty() {
        return Vec::new();
    }
    // （所在目录, 剥掉碟片标记之后的名字）→（那几个变体, 其中有没有真带标记的）。
    let mut families: BTreeMap<(&str, String), (Vec<&str>, bool)> = BTreeMap::new();
    for variant in variants {
        if variant.manual || !scope_of(manifest, variant.key).in_scope() {
            continue;
        }
        let Some(dir) = parent_of(variant.key) else {
            continue;
        };
        let name = last_component(variant.key);
        let family = without_disc_marker(name, &markers);
        let marked = family != name;
        let bucket = families.entry((dir, family)).or_default();
        bucket.0.push(variant.key);
        bucket.1 |= marked;
    }
    families
        .into_iter()
        .filter(|(_, (keys, marked))| keys.len() >= 2 && *marked)
        .map(|((dir, _), (mut keys, _))| {
            keys.sort_unstable();
            Doubt {
                kind: DoubtKind::UnmergedDiscs,
                at: dir.to_string(),
                items: keys.into_iter().map(ToString::to_string).collect(),
            }
        })
        .collect()
}

/// 名字末尾的碟片标记剥掉之后的样子；**扩展名留着**，标记在扩展名前面（与多碟同族那条规则同一个剥法）。
fn without_disc_marker(name: &str, markers: &[Pattern]) -> String {
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => {
            format!("{}.{extension}", strip_markers(stem, markers))
        }
        _ => strip_markers(name, markers),
    }
}

/// 一个目录被当成了一个变体的那几处。
fn crowded_trees(variants: &[Shaped<'_>], entries: &[Entry], manifest: &Manifest) -> Vec<Doubt> {
    let roots: BTreeSet<&str> = variants
        .iter()
        .filter(|variant| {
            !variant.manual
                && manifest
                    .rule(variant.rule)
                    .is_some_and(|rule| rule.kind == ShapeKind::DirectoryTree)
        })
        .map(|variant| variant.key)
        .collect();
    if roots.is_empty() {
        return Vec::new();
    }
    // 变体的目录 →（去掉扩展名、折过的名字 → 叫这个名字的那几份独立内容）。
    let mut inside: BTreeMap<&str, BTreeMap<String, Vec<&str>>> = BTreeMap::new();
    for entry in entries {
        if entry.is_dir {
            continue;
        }
        let Some(dir) = parent_of(&entry.key) else {
            continue;
        };
        if !roots.contains(dir) || !independent_content(&entry.key, manifest) {
            continue;
        }
        inside
            .entry(dir)
            .or_default()
            .entry(fold(base_stem(file_name_of_key(&entry.key))))
            .or_default()
            .push(&entry.key);
    }
    inside
        .into_iter()
        .filter(|(_, names)| names.len() >= 2)
        .map(|(root, names)| {
            let mut items: Vec<String> = names
                .into_values()
                .flatten()
                .map(ToString::to_string)
                .collect();
            items.sort_unstable();
            Doubt {
                kind: DoubtKind::CrowdedTree,
                at: root.to_string(),
                items,
            }
        })
        .collect()
}

/// 这个文件能不能**自己单独算一份内容**（模块文档「一个目录被当成了一个变体」那一节）。
fn independent_content(key: &str, manifest: &Manifest) -> bool {
    let name = Path::new(file_name_of_key(key));
    let classification = classify::classify(name);
    if classification.suspect.is_some() || classification.split_volume {
        return false;
    }
    if matches!(
        classification.category,
        Category::TransparentContainer | Category::CompressedImage
    ) {
        return true;
    }
    extension_lower(name)
        .is_some_and(|ext| ext == "iso" || manifest.platform_for_extension(&ext).is_some())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;
    use crate::platform::Manifest;
    use crate::shape::{Entry, plan};

    /// 一组文件（路径相对根「库」写），连一路上的每一级目录一起摆成条目——目录树的锚认的是**目录**条目。
    fn 条目(相对: &[&str]) -> Vec<Entry> {
        let mut dirs = BTreeSet::new();
        let mut entries = Vec::new();
        for key in 相对 {
            let key = format!("库/{key}");
            let mut cursor = key.as_str();
            while let Some((parent, _)) = cursor.rsplit_once('/') {
                dirs.insert(parent.to_string());
                cursor = parent;
            }
            entries.push(Entry {
                key,
                is_dir: false,
                len: Some(1024),
            });
        }
        entries.extend(dirs.into_iter().map(|key| Entry {
            key,
            is_dir: true,
            len: None,
        }));
        entries.sort_by(|a, b| a.key.cmp(&b.key));
        entries
    }

    /// 照内置清单成型一遍（`overrides` 是人工纠正：键 → 归到哪个变体，都相对根写），再问成型存疑。
    /// 交回（哪一种, 那一处去掉根名, 牵涉的几条去掉根名）。
    fn 存疑(相对: &[&str], 纠正: &[(&str, &str)]) -> Vec<(DoubtKind, String, Vec<String>)> {
        let manifest = Manifest::builtin();
        let entries = 条目(相对);
        let overrides: BTreeMap<String, String> = 纠正
            .iter()
            .map(|(key, to)| (format!("库/{key}"), format!("库/{to}")))
            .collect();
        let plan = plan(&entries, &manifest, &overrides);
        let 变体: Vec<Shaped<'_>> = plan
            .variants
            .iter()
            .map(|variant| Shaped {
                key: &variant.key,
                rule: &variant.rule,
                manual: variant.manual,
            })
            .collect();
        let 剥 = |key: &str| key.trim_start_matches("库/").to_string();
        shaping_doubts(&变体, &entries, &manifest)
            .into_iter()
            .map(|doubt| {
                (
                    doubt.kind,
                    剥(&doubt.at),
                    doubt.items.iter().map(|key| 剥(key)).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn 同一目录里只差碟片标记却各自成了变体的_是多碟没合在一起() {
        // FDS 没有声明多碟同族（`platforms.toml`），两面磁碟各成一个变体；碟片标记取清单里多碟同族规则的那一套。
        let 结论 = 存疑(
            &[
                "FDS/某游戏/某游戏 (Disk 1).fds",
                "FDS/某游戏/某游戏 (Disk 2).fds",
            ],
            &[],
        );
        assert_eq!(
            结论,
            vec![(
                DoubtKind::UnmergedDiscs,
                "FDS/某游戏".to_string(),
                vec![
                    "FDS/某游戏/某游戏 (Disk 1).fds".to_string(),
                    "FDS/某游戏/某游戏 (Disk 2).fds".to_string(),
                ],
            )]
        );
    }

    #[test]
    fn 规则已经合上的_名字碰巧像的_分在两个目录的_人工纠正过的_都不算多碟存疑() {
        // PS1 声明了多碟同族：两张碟已经是一个变体。
        assert!(
            存疑(
                &["ps/某游戏/游戏 (Disc 1).chd", "ps/某游戏/游戏 (Disc 2).chd"],
                &[]
            )
            .is_empty()
        );
        // 名字里碰巧带「碟」字，剥不出碟片标记。
        assert!(存疑(&["FDS/光碟大战.fds", "FDS/光碟传奇.fds"], &[]).is_empty());
        // 只认同一个目录。
        assert!(
            存疑(
                &["FDS/甲/某游戏 (Disk 1).fds", "FDS/乙/某游戏 (Disk 2).fds"],
                &[]
            )
            .is_empty()
        );
        // 人工纠正过的变体不再存疑：人已经看过这一处了。
        assert!(
            存疑(
                &["FDS/某游戏 (Disk 1).fds", "FDS/某游戏 (Disk 2).fds"],
                &[("FDS/某游戏 (Disk 1).fds", "FDS/某游戏 (Disk 1).fds")],
            )
            .is_empty()
        );
    }

    #[test]
    fn 目录树变体里直接躺着几个各自独立的镜像或容器的_是一个目录被当成了一个变体() {
        let 结论 = 存疑(
            &[
                "ps3/动作合集/PS3_GAME/USRDIR/EBOOT.BIN",
                "ps3/动作合集/甲.iso",
                "ps3/动作合集/乙.7z",
            ],
            &[],
        );
        assert_eq!(
            结论,
            vec![(
                DoubtKind::CrowdedTree,
                "ps3/动作合集".to_string(),
                vec![
                    "ps3/动作合集/乙.7z".to_string(),
                    "ps3/动作合集/甲.iso".to_string()
                ],
            )]
        );
    }

    #[test]
    fn 正常的目录树转储不算存疑_根下只有一个包或者只有转储自己的数据文件() {
        // PSV 的 NoNpDrm 转储：根下只有分区目录。
        assert!(
            存疑(
                &[
                    "PSV/零之轨迹[PCSG00042]/app/PCSG00042/eboot.bin",
                    "PSV/零之轨迹[PCSG00042]/app/PCSG00042/sce_sys/param.sfo",
                    "PSV/零之轨迹[PCSG00042]/patch/PCSG00042/eboot.bin",
                ],
                &[]
            )
            .is_empty()
        );
        // TitleID 写在目录名里、根下躺着一个透明容器：一个包是它自己，不是好几个游戏。
        assert!(存疑(&["PSV/AIME00001(wan华镜 v3.1)/AIME00001.tar.zst"], &[]).is_empty());
        // PS3 转储根下的 `PARAM.SFO` 与 `.BIN`：`.bin` 这类是转储自己的数据，不算独立的内容。
        assert!(
            存疑(
                &[
                    "ps3/某游戏/PS3_GAME/USRDIR/EBOOT.BIN",
                    "ps3/某游戏/PARAM.SFO",
                    "ps3/某游戏/DATA.BIN",
                    "ps3/某游戏/EXTRA.BIN",
                ],
                &[]
            )
            .is_empty()
        );
    }
}
