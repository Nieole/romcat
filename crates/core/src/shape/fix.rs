//! **成型纠正**：人对一处[成型存疑](super::Doubt)下的那个决定，折成一批**人工纠正**
//! （票 `gui-looks-like-the-design/29`）。
//!
//! 成型规则会出错，所以**人工纠正聚合结果是一等公民功能**（词表**成型规则**）——这一处是那扇正门
//! 里「决定怎么落」的那一半。落下来的东西与命令行 `romcat shape --merge` 写的是同一样：沉淀库里
//! 几行「这条键归哪个变体」（`Store::set_shaping_override`），重新成型时优先于一切规则
//! （[`super::plan`]）。
//!
//! ## 两种，与两种存疑一一对应
//!
//! - **合成**（[`merge`]）：勾中的那几个变体并成一个。[`DoubtKind::UnmergedDiscs`](super::DoubtKind::UnmergedDiscs)
//!   那一处走它。
//! - **拆开**（[`split`]）：一个目录树变体里那几份各自独立的内容，各自成一个变体。
//!   [`DoubtKind::CrowdedTree`](super::DoubtKind::CrowdedTree) 那一处走它。
//!
//! 撤销是同一批键上的删除（[`undo`]）：那几行没了，下一趟成型照规则重算，回到规则原本的结果。
//!
//! ## 判据只在这一处（ADR-0024）
//!
//! 「主文件挑哪一个」「合成之后附属文件是哪几条」「撤销要清掉哪几条」都在这里答。界面只画
//! [`Merged`] 这份预览、再把人按下的那一下转发过来——**屏上不许自己算一遍**，算两遍就会有
//! 「预览说主文件是甲、成型出来是乙」那一天。同一件事由 [`merge`] 交出的纠正喂给 [`super::plan`]
//! 之后必须长成同一个样子，本模块的测试钉着这一条。
//!
//! ## 盘上一个字节都不动（ADR-0004）
//!
//! 合成不生成播放列表、拆开不移动文件、撤销不恢复任何东西——改的只是「中立库里这些条目归哪个
//! 变体」。主文件因此**只能是已经在盘上的那几条成员里的一条**。

use std::collections::BTreeMap;

/// 一个变体连它的成员：纠正落的那几行记的是**成员**的键，不是变体的键。
///
/// 摆成自带的一小份而不是借 [`super::Variant`]，是因为屏上那一层拿到的是中立库读回来的变体
/// （`Catalog::variant_members`），而成型当场出的是 [`super::Variant`]——两边都折得出这三样。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Members {
    /// 变体的键。
    pub key: String,
    /// 主文件的键。
    pub main_key: String,
    /// 全部成员的键（目录成员也在内：目录树变体的主文件就是那个目录）。
    pub members: Vec<String>,
}

/// 合成之后那一个变体长什么样：屏上的**预览**画的就是它。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merged {
    /// 主文件：合进来的那几个变体里，**键最小的那一个的主文件**。
    pub main: String,
    /// 附属文件，按键排：其余每一条成员。
    pub companions: Vec<String>,
}

impl Merged {
    /// 一共几个文件成员（主文件加附属文件）。
    #[must_use]
    pub fn files(&self) -> usize {
        1 + self.companions.len()
    }
}

/// **合成**：把这几个变体并成一个，交回（要落的人工纠正, 合成之后长什么样）。
///
/// 主文件取**键最小的那个变体的主文件**——多碟的几张碟键上只差碟片标记，最小的就是第一张
/// （`(Disc 1)` 排在 `(Disc 2)` 前面），与命令行 `--merge` 把头一个键当主文件是同一条规矩。
///
/// **少于两个交回 `None`**：一个键说不清「这几条是一个变体」（命令行那一处逐字相同的话）。
#[must_use]
pub fn merge(selected: &[Members]) -> Option<(BTreeMap<String, String>, Merged)> {
    if selected.len() < 2 {
        return None;
    }
    let head = selected.iter().min_by(|a, b| a.key.cmp(&b.key))?;
    let main = head.main_key.clone();
    let mut overrides = BTreeMap::new();
    for variant in selected {
        for member in &variant.members {
            overrides.insert(member.clone(), main.clone());
        }
    }
    // 主文件那一条也要在，而且指向自己：少了它，`plan` 认不出这个变体是人工纠正出来的。
    overrides.insert(main.clone(), main.clone());
    let mut companions: Vec<String> = overrides
        .keys()
        .filter(|key| **key != main)
        .cloned()
        .collect();
    companions.sort();
    Some((overrides, Merged { main, companions }))
}

/// **拆开**：这几份各自独立的内容，各自成一个变体。
///
/// 每一条指向自己就是「你自己是一个变体」（[`super::plan`] 里 `target == key` 那一支）。目录本身
/// **不落任何一行**：它剩下的那些内部资源照旧归它，一份内部资源都不剩时它就不再是变体
/// （[`super::plan`] 里「一个文件成员都没有的不成变体」那一条）。
///
/// **少于两份交回 `None`**：一份内容的目录本来就该是一个变体，没什么可拆的。
#[must_use]
pub fn split(contents: &[String]) -> Option<BTreeMap<String, String>> {
    if contents.len() < 2 {
        return None;
    }
    Some(
        contents
            .iter()
            .map(|key| (key.clone(), key.clone()))
            .collect(),
    )
}

/// **撤销**：这一处的人工纠正要清掉哪几条键。
///
/// `spot` 是**同一处一起纠正出来的那几个变体**（[`Catalog::shaping_fix_group`](crate::catalog::Catalog::shaping_fix_group)
/// 折得出来）：
///
/// - **合成**落的是每个成员一行、都指向同一个主文件，于是这一处只有**一个**变体——清它全部成员就完了。
/// - **拆开**落的是那几份内容**各一行**，这一处于是有**好几个**变体。只清其中一个的话，剩下几份还各自
///   成变体——**回不到规则原本的结果**（票 29 验收第 5 条逐字要的就是这个），而那个目录本身不落行、
///   压根没有「撤这一份」那颗按钮可按。所以这一处得整处一起撤。
///
/// 清完重新成型，这几条回到规则算出来的地方。
#[must_use]
pub fn undo(spot: &[Members]) -> Vec<String> {
    let mut keys: Vec<String> = spot
        .iter()
        .flat_map(|variant| {
            variant
                .members
                .iter()
                .cloned()
                .chain(std::iter::once(variant.main_key.clone()))
        })
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::platform::Manifest;
    use crate::shape::{Entry, MANUAL_RULE, plan};

    fn 成员(key: &str, main: &str, members: &[&str]) -> Members {
        Members {
            key: key.to_string(),
            main_key: main.to_string(),
            members: members.iter().map(ToString::to_string).collect(),
        }
    }

    /// 一组文件连一路上的每一级目录，摆成中立库的条目（同 `doubt.rs` 里那一份）。
    fn 条目(相对: &[&str]) -> Vec<Entry> {
        let mut dirs = BTreeSet::new();
        let mut entries = Vec::new();
        for key in 相对 {
            let key = (*key).to_string();
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

    #[test]
    fn 合成取键最小那个变体的主文件_其余成员都成了附属文件_少于两个不成() {
        let (纠正, 预览) = merge(&[
            成员(
                "库/FDS/某游戏 (Disk 2).fds",
                "库/FDS/某游戏 (Disk 2).fds",
                &["库/FDS/某游戏 (Disk 2).fds"],
            ),
            成员(
                "库/FDS/某游戏 (Disk 1).fds",
                "库/FDS/某游戏 (Disk 1).fds",
                &["库/FDS/某游戏 (Disk 1).fds"],
            ),
        ])
        .expect("两个变体合得成");
        assert_eq!(预览.main, "库/FDS/某游戏 (Disk 1).fds");
        assert_eq!(预览.companions, vec!["库/FDS/某游戏 (Disk 2).fds"]);
        assert_eq!(预览.files(), 2);
        assert_eq!(纠正.len(), 2);
        assert!(纠正.values().all(|to| to == "库/FDS/某游戏 (Disk 1).fds"));
        // 一个键说不清「这几条是一个变体」。
        assert!(merge(&[成员("a", "a", &["a"])]).is_none());
    }

    #[test]
    fn 合成之后真成型出来的那一个变体_与预览逐字相同() {
        // **预览与成型不许各算一遍**（模块文档）：同一批纠正喂给 `plan`，主文件与附属文件得对得上。
        let entries = 条目(&[
            "库/FDS/某游戏/某游戏 (Disk 1).fds",
            "库/FDS/某游戏/某游戏 (Disk 2).fds",
        ]);
        let (纠正, 预览) = merge(&[
            成员(
                "库/FDS/某游戏/某游戏 (Disk 1).fds",
                "库/FDS/某游戏/某游戏 (Disk 1).fds",
                &["库/FDS/某游戏/某游戏 (Disk 1).fds"],
            ),
            成员(
                "库/FDS/某游戏/某游戏 (Disk 2).fds",
                "库/FDS/某游戏/某游戏 (Disk 2).fds",
                &["库/FDS/某游戏/某游戏 (Disk 2).fds"],
            ),
        ])
        .expect("合得成");
        let plan = plan(&entries, &Manifest::builtin(), &纠正);
        assert_eq!(plan.variants.len(), 1, "两个变体没合成一个：{plan:?}");
        let 合出来的 = &plan.variants[0];
        assert_eq!(合出来的.main_key, 预览.main);
        assert_eq!(合出来的.rule, MANUAL_RULE);
        assert!(合出来的.manual);
        let 附属: Vec<String> = 合出来的
            .members
            .iter()
            .filter(|(key, _)| *key != 预览.main)
            .map(|(key, _)| key.clone())
            .collect();
        assert_eq!(附属, 预览.companions);
    }

    #[test]
    fn 拆开之后每份独立内容各成一个变体_目录不再是变体_少于两份不拆() {
        let entries = 条目(&[
            "库/ps3/动作合集/PS3_GAME/USRDIR/EBOOT.BIN",
            "库/ps3/动作合集/甲.iso",
            "库/ps3/动作合集/乙.7z",
        ]);
        let manifest = Manifest::builtin();
        // 拆之前：整个目录是一个变体。
        let 拆之前 = plan(&entries, &manifest, &BTreeMap::new());
        assert_eq!(
            拆之前
                .variants
                .iter()
                .map(|v| v.key.clone())
                .collect::<Vec<_>>(),
            vec!["库/ps3/动作合集".to_string()],
        );
        let 纠正 = split(&[
            "库/ps3/动作合集/甲.iso".to_string(),
            "库/ps3/动作合集/乙.7z".to_string(),
        ])
        .expect("两份内容拆得开");
        let 拆之后 = plan(&entries, &manifest, &纠正);
        let 键: Vec<String> = 拆之后.variants.iter().map(|v| v.key.clone()).collect();
        assert_eq!(
            键,
            vec![
                "库/ps3/动作合集".to_string(),
                "库/ps3/动作合集/乙.7z".to_string(),
                "库/ps3/动作合集/甲.iso".to_string(),
            ],
            "拆出来的那两份没各自成变体，或者那份转储没留下：{拆之后:?}"
        );
        assert!(split(&["只有一份".to_string()]).is_none());
    }

    #[test]
    fn 拆光了的目录不再是变体() {
        // 目录里只有那两份独立内容：拆完之后它一个文件成员都不剩，不该再留一条谁也代表不了的记录。
        let entries = 条目(&[
            "库/PSV/AIME00001(合集)/甲.vpk",
            "库/PSV/AIME00001(合集)/乙.vpk",
        ]);
        let manifest = Manifest::builtin();
        assert_eq!(
            plan(&entries, &manifest, &BTreeMap::new())
                .variants
                .iter()
                .map(|v| v.key.clone())
                .collect::<Vec<_>>(),
            vec!["库/PSV/AIME00001(合集)".to_string()],
            "拆之前整个目录该是一个变体"
        );
        let 纠正 = split(&[
            "库/PSV/AIME00001(合集)/甲.vpk".to_string(),
            "库/PSV/AIME00001(合集)/乙.vpk".to_string(),
        ])
        .expect("拆得开");
        assert_eq!(
            plan(&entries, &manifest, &纠正)
                .variants
                .iter()
                .map(|v| v.key.clone())
                .collect::<Vec<_>>(),
            vec![
                "库/PSV/AIME00001(合集)/乙.vpk".to_string(),
                "库/PSV/AIME00001(合集)/甲.vpk".to_string(),
            ],
            "拆光了的目录还留着一条空变体"
        );
    }

    #[test]
    fn 拆开那一处要整处一起撤_只撤一份回不到规则原本的结果() {
        // 票 29 验收第 5 条「撤销后**回到规则原本的结果**」：拆开落的是那几份内容各一行，
        // 只清其中一份的话，剩下那几份还各自成变体——那不是规则原本的结果。
        let entries = 条目(&[
            "库/ps3/动作合集/PS3_GAME/USRDIR/EBOOT.BIN",
            "库/ps3/动作合集/甲.iso",
            "库/ps3/动作合集/乙.7z",
        ]);
        let manifest = Manifest::builtin();
        let 甲 = "库/ps3/动作合集/甲.iso".to_string();
        let 乙 = "库/ps3/动作合集/乙.7z".to_string();
        let 纠正 = split(&[甲.clone(), 乙.clone()]).expect("拆得开");
        let 一份 = |key: &String| Members {
            key: key.clone(),
            main_key: key.clone(),
            members: vec![key.clone()],
        };
        let 成什么样 = |纠正: &BTreeMap<String, String>| {
            plan(&entries, &manifest, 纠正)
                .variants
                .iter()
                .map(|v| v.key.clone())
                .collect::<Vec<_>>()
        };

        // 只撤一份：剩下那一份还各自成变体。
        let mut 少撤了 = 纠正.clone();
        for key in undo(&[一份(&甲)]) {
            少撤了.remove(&key);
        }
        assert_eq!(
            成什么样(&少撤了),
            vec![
                "库/ps3/动作合集".to_string(),
                "库/ps3/动作合集/乙.7z".to_string(),
            ],
            "只撤一份就该还剩一份挂着——这正是不能只撤一份的理由"
        );

        // 整处一起撤：回到规则原本的结果（整个目录一个变体）。
        let mut 整处撤 = 纠正;
        for key in undo(&[一份(&甲), 一份(&乙)]) {
            整处撤.remove(&key);
        }
        assert_eq!(
            成什么样(&整处撤),
            vec!["库/ps3/动作合集".to_string()],
            "整处撤完没回到规则原本的结果"
        );
    }

    #[test]
    fn 撤销清的是这个变体全部成员那几行_清完回到规则原本的结果() {
        let entries = 条目(&[
            "库/FDS/某游戏/某游戏 (Disk 1).fds",
            "库/FDS/某游戏/某游戏 (Disk 2).fds",
        ]);
        let manifest = Manifest::builtin();
        let (mut 纠正, _) = merge(&[
            成员(
                "库/FDS/某游戏/某游戏 (Disk 1).fds",
                "库/FDS/某游戏/某游戏 (Disk 1).fds",
                &["库/FDS/某游戏/某游戏 (Disk 1).fds"],
            ),
            成员(
                "库/FDS/某游戏/某游戏 (Disk 2).fds",
                "库/FDS/某游戏/某游戏 (Disk 2).fds",
                &["库/FDS/某游戏/某游戏 (Disk 2).fds"],
            ),
        ])
        .expect("合得成");
        let 合出来的 = plan(&entries, &manifest, &纠正).variants.remove(0);
        let 要清的 = undo(&[Members {
            key: 合出来的.key.clone(),
            main_key: 合出来的.main_key.clone(),
            members: 合出来的
                .members
                .iter()
                .map(|(key, _)| key.clone())
                .collect(),
        }]);
        assert_eq!(
            要清的,
            vec![
                "库/FDS/某游戏/某游戏 (Disk 1).fds".to_string(),
                "库/FDS/某游戏/某游戏 (Disk 2).fds".to_string(),
            ]
        );
        for key in &要清的 {
            纠正.remove(key);
        }
        assert_eq!(
            plan(&entries, &manifest, &纠正).variants.len(),
            2,
            "撤销之后没回到规则原本的结果（两张碟各自成变体）"
        );
    }
}
