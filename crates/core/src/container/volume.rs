//! 分卷：**三种完全不同的机制**，只是名字都长得像「一个文件被切成了好几段」。
//!
//! CONTEXT 的 **透明容器** 词条只说了一句「分卷压缩的多个分卷合起来才是一个容器」，
//! 而落到磁盘上，那句话有三种互不相干的实现，混为一谈会让每一种都处理错
//! （调研 `containers-and-compressed-images.md` 1.7.1）：
//!
//! | 名字 | 机制 | 入口是哪一段 |
//! |---|---|---|
//! | `X.part1.rar`、`X.rar` + `X.r00` | **RAR 官方分卷**：数据跨卷续接，靠 `SPLIT_BEFORE/AFTER` 串接 | 第一卷 |
//! | `X.7z.001`、`X.iso.001` | **7-Zip 的通用字节切分**：`cat` 即可还原，各段没有独立头部 | `.001` |
//! | `X.z01` … `X.zip` | **ZIP 官方 split**（APPNOTE §8.3.3） | **最后**那个 `.zip` |
//!
//! 第三行是最反直觉的那条：ZIP 的中央目录写在**末段**，所以入口是 `.zip` 而不是 `.z01`。
//! 真库里那 4 组 WIIU 的 `XenobladeX-…-WUP.z01…z04 + .zip` 正是这一种——每组
//! 5 个文件、约 20 GiB，而 `.zip` 那一段自己就把 47 条内部条目的 CRC-32 交了出来。
//!
//! ## 这个模块只看名字
//!
//! 它不打开任何文件。判据全在文件名上，因此它是纯函数、测得动，也能被
//! [`crate::classify`]（归类）、[`crate::shape`]（成型）与 [`super::rar`]（穿透）
//! 三处共用同一套判据——三处各写一份的话，「哪些名字算一段」迟早会漂成三个答案。
//!
//! ## 入口段与非入口段
//!
//! **非入口段不是一个独立的容器**：它没有自己的中央目录、没有自己的头，整组由入口段
//! 代表。因此 [`ContainerKind::for_path`](super::ContainerKind::for_path) 对它返回
//! `None`，扫描不会单独去穿它，报告也不会把一组数成好几个容器。

use std::collections::BTreeMap;
use std::path::Path;

use crate::path::fold;

/// 三种分卷机制。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VolumeScheme {
    /// **RAR 官方分卷**。数据跨卷续接：一个内部文件可以从这一卷续到下一卷，
    /// 而**非末段记的是打包后数据的校验和**（ADR-0014），拿去撞 DAT 必然落空。
    Rar,
    /// **7-Zip 的通用字节切分**（`SplitHandler.cpp` 里注册名就叫 `Split`）。
    /// 它不是压缩格式：各段拼起来就是原文件，`cat` 即可还原。
    ByteSplit,
    /// **ZIP 官方 split**（APPNOTE §8.3.3）。入口是**最后**那个 `.zip`——
    /// 规范原文说末段用 `.zip` 扩展名正是为了「快速读到中央目录」。
    ZipSplit,
}

impl VolumeScheme {
    /// 这一组的入口段长什么样，说给人听。
    #[must_use]
    pub fn entry_hint(self) -> &'static str {
        match self {
            Self::Rar => "第一卷（`.part1.rar` 或 `.rar`）",
            Self::ByteSplit => "`.001`",
            Self::ZipSplit => "最后那个 `.zip`",
        }
    }
}

/// 一个文件名在分卷这件事上的身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    /// 哪一种机制。
    pub scheme: VolumeScheme,
    /// 同一组的聚合键：剥掉分卷后缀之后的基名，**折过大小写**。
    ///
    /// 只在同一个目录里比对——分卷不会散落在两个目录，而两个目录里同名的两组
    /// 是两组东西。
    pub group: String,
    /// 段号，排序用。
    pub index: u32,
    /// 它是不是这一组的**入口段**。
    ///
    /// **入口是可能而不是断定**：`X.zip` 绝大多数时候只是一个普通 zip，只有同目录下
    /// 真的坐着 `X.z01` 才说明它是某一组的末段。断定要看整组
    /// （[`group_volumes`]），单看一个名字断不了。
    pub entry: bool,
}

/// 这个文件名是不是**非入口段**——也就是「它自己不是一个独立的容器」。
#[must_use]
pub fn is_non_entry_part(name: &str) -> bool {
    matches!(volume_of(name), Some(volume) if !volume.entry)
}

/// 这个文件名是不是分卷的一段——**入口段也算**。
///
/// 归类要它：`X.7z.001` 与 `X.z01` 的扩展名不在任何一张归类表里，可它们
/// 明明是**透明容器**的一部分，不该掉进「未归类」。
#[must_use]
pub fn is_volume_segment(name: &str) -> bool {
    volume_of(name).is_some()
}

/// 按名字判断它在分卷里的身份；与分卷无关的名字是 `None`。
///
/// 注意 `X.rar` 与 `X.zip` 一律返回**入口候选**：它们绝大多数时候只是普通的容器，
/// 是不是某一组的入口要看同目录里有没有别的段（[`group_volumes`]）。
#[must_use]
pub fn volume_of(name: &str) -> Option<Volume> {
    // **只折 ASCII**：这些后缀全是 ASCII，而 `to_lowercase` 会改变字节长度
    // （希腊文的 `Σ`、土耳其文的 `İ` 都是），下面还要拿下标回原名上取子串。
    // 聚合键那一栏才用 `fold`——那一栏只当键用，不回原名取子串。
    let lower = name.to_ascii_lowercase();
    // RAR 新式分卷 `X.partN.rar`，也认 unrar `GetVolNumPos()` 注释里那种
    // `X.part3of9.rar`。第一卷是入口。
    if let Some(stem) = lower.strip_suffix(".rar") {
        if let Some((head, tail)) = stem.rsplit_once(".part") {
            let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
            let rest = &tail[digits.len()..];
            // `.part3of9.rar` 的尾巴是 `of9`；除此之外只允许什么都不剩。
            let tail_ok = rest.is_empty()
                || (rest.starts_with("of") && rest[2..].chars().all(|c| c.is_ascii_digit()));
            if !head.is_empty() && !digits.is_empty() && tail_ok {
                let index = digits.parse::<u32>().ok()?;
                return Some(Volume {
                    scheme: VolumeScheme::Rar,
                    group: fold(head),
                    index,
                    entry: index <= 1,
                });
            }
        }
        // 旧式分卷的入口就叫 `X.rar`，与「一个普通的 rar」同名——只能当候选。
        return Some(Volume {
            scheme: VolumeScheme::Rar,
            group: fold(stem),
            index: 1,
            entry: true,
        });
    }
    if let Some(stem) = lower.strip_suffix(".zip")
        && !stem.is_empty()
    {
        // ZIP split 的入口是**末段**，段号给个最大值，排序时它就落在 `.zNN` 后面。
        return Some(Volume {
            scheme: VolumeScheme::ZipSplit,
            group: fold(stem),
            index: u32::MAX,
            entry: true,
        });
    }

    let (head, ext) = lower.rsplit_once('.')?;
    if head.is_empty() {
        return None;
    }
    // 表里认得的扩展名一律不当分卷看。`.z64` 是 N64 的**裸文件**，长得却像 ZIP 的
    // 第 64 段——认错会把整个 N64 平台记成透明容器。
    if crate::classify::is_known_extension(ext) {
        return None;
    }
    // ⭐ **先确认这三个字节是 ASCII**。下面按字节下标切子串，而
    // `len() == 3` 数的是**字节**——一个汉字正好三字节，`游戏.命` 会一路过关，
    // 然后 `ext[1..]` 切在字符中间当场 panic。真机上是 `dtapart.命名规则` 这类
    // 名字把它引爆的：库里 39,856 个文件名含汉字，这不是边角情形。
    if ext.len() != 3 || !ext.is_ascii() {
        return None;
    }
    // 7-Zip 的通用字节切分 `.001`；基名连着它自己的扩展名一起当聚合键
    // （`X.7z.001` → `X.7z`）。
    if ext.chars().all(|c| c.is_ascii_digit()) {
        let index = ext.parse::<u32>().ok()?;
        return Some(Volume {
            scheme: VolumeScheme::ByteSplit,
            group: fold(head),
            index,
            entry: index == 1,
        });
    }
    if !ext[1..].chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let index = ext[1..].parse::<u32>().ok()?;
    match ext.as_bytes()[0] {
        // ZIP 官方 split 的中间段。入口是末段那个 `.zip`，所以这里一律不是入口。
        b'z' => Some(Volume {
            scheme: VolumeScheme::ZipSplit,
            group: fold(head),
            index,
            entry: false,
        }),
        // RAR 旧式分卷的第二卷起。`.r00` 是第二卷，入口是 `X.rar`。
        //
        // **只认 `.rNN`**：旧式编号跑完 `.r99` 之后接的是 `.s00`…`.z99`
        // （unrar `NextVolumeName()`），而那一段与 ZIP split 的 `.z01` 撞名。
        // 真库里 `.rNN` 一个都没有，为一个撞名的边角情形去猜是负收益——
        // 认不出来只是少聚一组，认错了会把 ZIP split 当成 RAR 分卷。
        b'r' => Some(Volume {
            scheme: VolumeScheme::Rar,
            group: fold(head),
            index: index + 2,
            entry: false,
        }),
        _ => None,
    }
}

/// 一组分卷。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeGroup {
    /// 哪一种机制。
    pub scheme: VolumeScheme,
    /// 聚合键。
    pub group: String,
    /// 组里的成员，按段号排好。存的是调用方给进来的**原名**。
    pub members: Vec<String>,
    /// 入口段的原名；整组一段入口都没有时是 `None`——那是**缺入口卷**，
    /// 剩下的段谁也代表不了这一组。
    pub entry: Option<String>,
}

/// 把同一个目录里的一批文件名分成组。
///
/// **只有真的成组的才算**：一组里至少要有一个非入口段。否则库里每一个 `.zip`
/// 都会自称是一组分卷的末段。
#[must_use]
pub fn group_volumes<'n>(names: impl IntoIterator<Item = &'n str>) -> Vec<VolumeGroup> {
    /// 一个桶里的一段：段号、是不是入口、原名。
    type Member = (u32, bool, String);
    let mut buckets: BTreeMap<(VolumeScheme, String), Vec<Member>> = BTreeMap::new();
    for name in names {
        let Some(volume) = volume_of(name) else {
            continue;
        };
        buckets
            .entry((volume.scheme, volume.group))
            .or_default()
            .push((volume.index, volume.entry, name.to_string()));
    }
    buckets
        .into_iter()
        .filter_map(|((scheme, group), mut members)| {
            if !members.iter().any(|(_, entry, _)| !*entry) {
                // 一段非入口段都没有：这不是一组分卷，只是一个普通的容器。
                return None;
            }
            members.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.2.cmp(&b.2)));
            let entry = members
                .iter()
                .find(|(_, entry, _)| *entry)
                .map(|(_, _, name)| name.clone());
            Some(VolumeGroup {
                scheme,
                group,
                members: members.into_iter().map(|(_, _, name)| name).collect(),
                entry,
            })
        })
        .collect()
}

/// 下一卷的文件名。**只管算名字，不管盘上有没有**。
///
/// 出处是 unrar `pathfn.cpp` 的 `NextVolumeName()`：新式在 `.partNN.` 里的数字上
/// 加一，位数不够就往前补一位（`.part9.rar` → `.part10.rar`）；旧式从 `.rar` 走到
/// `.r00`，再 `.r00` → `.r01`。
///
/// `new_numbering` 由**主头部的旗标**给出（RAR4 的 `MHD_NEWNUMBERING`，RAR5 恒为真），
/// 不是猜的：同一个基名两种编号都可能存在，只有容器自己说了算。
#[must_use]
pub fn next_volume_path(path: &Path, new_numbering: bool) -> Option<std::path::PathBuf> {
    let name = path.file_name()?.to_str()?;
    let next = next_volume_name(name, new_numbering)?;
    Some(path.with_file_name(next))
}

/// [`next_volume_path`] 的纯字符串版本。
#[must_use]
pub fn next_volume_name(name: &str, new_numbering: bool) -> Option<String> {
    // 同 `volume_of`：只折 ASCII，因为下面拿这里的下标回原名上取子串。
    let lower = name.to_ascii_lowercase();
    if new_numbering {
        // 在 `.partNN` 那一段数字上加一，**保住原来的位数**：`.part01` → `.part02`，
        // 而 `.part9` → `.part10`（unrar 那段注释里写的正是这个进位）。
        let at = lower.rfind(".part")?;
        let digits_at = at + ".part".len();
        let digits: String = name[digits_at..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if digits.is_empty() {
            return None;
        }
        let number = digits.parse::<u64>().ok()? + 1;
        let width = digits.len().max(number.to_string().len());
        return Some(format!(
            "{}{number:0width$}{}",
            &name[..digits_at],
            &name[digits_at + digits.len()..],
        ));
    }
    if let Some(stem) = lower.strip_suffix(".rar") {
        return Some(format!("{}.r00", &name[..stem.len()]));
    }
    // `.rNN` → `.r(NN+1)`；`.r99` 之后旧式编号进到 `.s00`，那一段不认（见 `volume_of`）。
    let (head, ext) = name.rsplit_once('.')?;
    let ext_lower = ext.to_ascii_lowercase();
    let rest = ext_lower.strip_prefix('r')?;
    if rest.len() != 2 || !rest.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let number = rest.parse::<u32>().ok()? + 1;
    if number > 99 {
        return None;
    }
    Some(format!("{head}.r{number:02}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 身份(name: &str) -> Option<Volume> {
        volume_of(name)
    }

    #[test]
    fn 三种分卷形态各归各的() {
        // RAR 官方分卷：第一卷是入口
        let rar = 身份("游戏.part1.rar").expect("认得出");
        assert_eq!(rar.scheme, VolumeScheme::Rar);
        assert_eq!(rar.group, "游戏");
        assert!(rar.entry);
        assert!(!身份("游戏.part2.rar").unwrap().entry);

        // 7-Zip 的通用字节切分：`.001` 是入口，聚合键连着原来的扩展名
        let split = 身份("合集.7z.001").expect("认得出");
        assert_eq!(split.scheme, VolumeScheme::ByteSplit);
        assert_eq!(split.group, "合集.7z");
        assert!(split.entry);
        assert!(!身份("合集.7z.002").unwrap().entry);

        // ZIP 官方 split：入口是**最后**那个 `.zip`，`.z01` 不是
        let zip = 身份("X.zip").expect("认得出");
        assert_eq!(zip.scheme, VolumeScheme::ZipSplit);
        assert!(zip.entry);
        assert!(!身份("X.z01").unwrap().entry);
    }

    #[test]
    fn 三种形态的入口不是同一段() {
        // 这条测的正是最容易搞反的那一件：ZIP 的入口在末尾，另外两种在开头。
        let 组 = group_volumes(["X.z01", "X.z02", "X.zip"]);
        assert_eq!(组.len(), 1);
        assert_eq!(组[0].scheme, VolumeScheme::ZipSplit);
        assert_eq!(组[0].entry.as_deref(), Some("X.zip"));

        let 组 = group_volumes(["X.part2.rar", "X.part1.rar"]);
        assert_eq!(组[0].entry.as_deref(), Some("X.part1.rar"));
        assert_eq!(组[0].members, ["X.part1.rar", "X.part2.rar"]);

        let 组 = group_volumes(["X.7z.002", "X.7z.001"]);
        assert_eq!(组[0].entry.as_deref(), Some("X.7z.001"));
    }

    #[test]
    fn 旧式分卷的入口是那个普通的_rar() {
        let 组 = group_volumes(["游戏.rar", "游戏.r00", "游戏.r01"]);
        assert_eq!(组.len(), 1);
        assert_eq!(组[0].scheme, VolumeScheme::Rar);
        assert_eq!(组[0].entry.as_deref(), Some("游戏.rar"));
        assert_eq!(组[0].members.len(), 3);
    }

    #[test]
    fn 一个普通的容器不是一组分卷() {
        // 库里 34,808 个 zip 与 1,333 个 rar 全都长着「入口候选」的样子，
        // 只有同目录里真有别的段才算一组。
        assert!(group_volumes(["普通.zip"]).is_empty());
        assert!(group_volumes(["普通.rar"]).is_empty());
        assert!(group_volumes(["甲.zip", "乙.zip"]).is_empty());
    }

    #[test]
    fn 缺入口卷的一组认得出来() {
        // 真库里那个 `废都物语_资料合辑_220928.7z.006`：`.001` 到 `.005` 都不在库里。
        let 组 = group_volumes(["资料.7z.006"]);
        assert_eq!(组.len(), 1);
        assert_eq!(组[0].entry, None, "缺入口卷");
    }

    #[test]
    fn 汉字扩展名不会切在字符中间() {
        // `len() == 3` 数的是**字节**，一个汉字正好三字节。真机上这条路
        // 当场 panic 过——库里 39,856 个文件名含汉字。
        assert_eq!(身份("游戏.命"), None);
        assert_eq!(身份("dtapart.命名规"), None);
        assert!(!is_non_entry_part("某文件.魂"));
        assert!(!is_volume_segment("某文件.魂"));
    }

    #[test]
    fn 已知扩展名不会被当成分卷() {
        // `.z64` 是 N64 的裸文件
        assert_eq!(身份("超级马里奥64.z64"), None);
        assert!(!is_non_entry_part("超级马里奥64.z64"));
        // 三位数字但扩展名认得的也一样
        assert_eq!(身份("game.nes"), None);
    }

    #[test]
    fn 下一卷的名字按_unrar_那两套算法算() {
        assert_eq!(
            next_volume_name("游戏.part1.rar", true).as_deref(),
            Some("游戏.part2.rar")
        );
        // 位数保住
        assert_eq!(
            next_volume_name("游戏.part01.rar", true).as_deref(),
            Some("游戏.part02.rar")
        );
        // 进位时补一位：unrar 那段注释写的就是 `.part9.rar` → `.part10.rar`
        assert_eq!(
            next_volume_name("游戏.part9.rar", true).as_deref(),
            Some("游戏.part10.rar")
        );
        assert_eq!(
            next_volume_name("游戏.part09.rar", true).as_deref(),
            Some("游戏.part10.rar")
        );
        // 旧式：`.rar` → `.r00` → `.r01`
        assert_eq!(
            next_volume_name("游戏.rar", false).as_deref(),
            Some("游戏.r00")
        );
        assert_eq!(
            next_volume_name("游戏.r00", false).as_deref(),
            Some("游戏.r01")
        );
        // `.r99` 之后是 `.s00`，这个模块不认——宁可少认一组，不能认错
        assert_eq!(next_volume_name("游戏.r99", false), None);
    }

    #[test]
    fn 非入口段不是一个独立的容器() {
        assert!(is_non_entry_part("X.part2.rar"));
        assert!(is_non_entry_part("X.z01"));
        assert!(is_non_entry_part("X.7z.002"));
        assert!(!is_non_entry_part("X.part1.rar"));
        assert!(!is_non_entry_part("X.zip"));
        assert!(!is_non_entry_part("X.rar"));
    }
}
