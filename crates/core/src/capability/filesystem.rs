//! **目标存储放得下什么**：能力档案的另一半。
//!
//! ADR-0017 的补充段：能力档案不能只描述模拟器支持哪些格式。**FAT32 有 4 GiB 单文件
//! 上限**，PS2 / PSP 的大 ISO 直接放不进去——这类情况必须在**差量预览**阶段就报出来，
//! 而不是传到一半失败。传到一半的失败会在卡上留下半份文件、在**清单**里留下一条谎。
//!
//! ## 它与模拟器那一半真的各变各的
//!
//! 同一个 RetroArch，卡是 exFAT 还是 FAT32 完全是另一件事。分成两个坐标，于是
//! `retroarch-exfat` 与 `retroarch-fat32` 共用同一张矩阵，只差这一份声明。
//!
//! ## 有几条其实是 Windows 的限制，不是文件系统的
//!
//! `MAX_PATH`、保留设备名、那几个不收的字符——都是 Windows API 的规矩，对 exFAT 与
//! FAT32 一样成立。而主力机就是 Windows（ADR-0018），卡在那台机器上写。只有
//! [`Filesystem::max_file_bytes`] 那一条是真正属于文件系统的。
//!
//! ## 硬链接支不支持**不在这里**
//!
//! 那由 [`sync::execute::probe`](crate::sync::execute::probe) 真建一个链接试出来，
//! 那是事实。写一条静态声明，一旦与探测结果矛盾，就正好是 ADR-0017 说的
//! 「矩阵错误比不转换更糟」（挂账 D93）。

use std::collections::BTreeSet;

use serde::Serialize;

use super::Claim;

/// 目标存储的约束。
///
/// ADR-0017 的补充段：能力档案不能只描述模拟器支持哪些格式。**FAT32 有 4 GiB 单文件
/// 上限**，PS2 / PSP 的大 ISO 直接放不进去——这类情况必须在**差量预览**阶段就报出来。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Filesystem {
    /// 文件系统名。
    pub name: String,
    /// 一句话说这是什么。
    pub note: String,
    /// 单文件字节上限；`None` 是不设限。
    pub max_file_bytes: Option<u64>,
    /// 一段文件名至多几个字符（UTF-16 码元）；`None` 是不设限。
    pub max_name_chars: Option<usize>,
    /// 完整路径至多几个字符（UTF-16 码元）；`None` 是不设限。
    pub max_path_chars: Option<usize>,
    /// 文件名里不许出现的字符。
    pub forbidden: BTreeSet<char>,
    /// 保留名（去掉扩展名之后撞上就不行），已折成大写。
    pub reserved_stems: BTreeSet<String>,
    /// 出处。
    pub claim: Claim,
}

/// 一份内容放不进目标存储的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum RejectReason {
    /// 超过单文件容量上限。**FAT32 那 4 GiB** 走的就是这条。
    TooBig,
    /// 文件名里有目标存储不收的字符。
    BadName,
    /// 单段文件名太长。
    NameTooLong,
    /// 完整路径太长。
    PathTooLong,
    /// 两份东西要落到目标上**同一条路径**。
    ///
    /// 两条路走到这里：转换把两个不同的容器解成了同名内容；或者两个**根**里同一条相对路径
    /// ——子库里的落点一律剥掉根名（ADR-0013），于是它们落在卡上同一个文件上。
    /// **撞上的一个都不放行**（`CONTEXT.md` 的**落点撞车**）。
    Collision,
}

impl RejectReason {
    /// 报告里用的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::TooBig => "超过单文件上限",
            // 照稿（差距 D-19）：与「放不进目标」那一栏头一句「文件名里有目标不收的字符」同一个说法。
            Self::BadName => "文件名里有目标不收的字符",
            Self::NameTooLong => "文件名太长",
            Self::PathTooLong => "路径太长",
            Self::Collision => "落点撞车",
        }
    }

    /// 这一类**人该去哪儿办**。
    ///
    /// **主库只读**：这几类工具一份都改不了，路只有「在主库里自己改名」或者「换一张卡 / 把它从选择集里排除」。
    /// [撞车](Self::Collision)那一类是 `None`——它有得办（排除其中一份），那句话跟着那一段自己走。
    ///
    /// **文件名不收的字符那句把具体字符列出来**（拿主意的人 2026-10-01 裁 `F-5` A，照稿）：字取这份文件系统的声明
    /// （[`Filesystem::forbidden`]），所以要把拦下它的那一份递进来。屏上不写 ADR 编号（票 gl-03 立的规矩）。
    ///
    /// 放在这儿而不是各印各的：命令行那一份与界面那一份说的必须是同一件事，两处各写一遍，
    /// 改了一处就会有一处在骗人（ADR-0024）。
    #[must_use]
    pub fn advice(self, filesystem: &Filesystem) -> Option<String> {
        const 改名: &str = "在主库里改名后重新扫描即可。";
        Some(match self {
            // 上限是多少那句事实另摆在这一段头上（[`Filesystem::max_file_fact`]），这里只说怎么办。
            Self::TooBig => "换一张不限单文件大小的卡，或者把这几份从选择集里排除。".to_string(),
            Self::BadName if filesystem.forbidden.is_empty() => 改名.to_string(),
            Self::BadName => format!(
                "Windows 与 {} 都不收 {}。{改名}",
                filesystem.name,
                filesystem.forbidden_shown(),
            ),
            Self::NameTooLong => "在主库里把名字改短后重新扫描即可。".to_string(),
            Self::PathTooLong => "把目标路径挪浅一层，或者在主库里把那几层目录名改短。".to_string(),
            Self::Collision => return None,
        })
    }

    /// 这一类那一段**零条也说的事实**：只有[超过单文件上限](Self::TooBig)那一段有——这张卡单文件上限是多少
    /// （[`Filesystem::max_file_fact`]，照稿零条也说，差距 D-20）。别的几类是 `None`。
    ///
    /// 与 [`Self::advice`] 分开：劝告有条才说（目标本来就不限单文件大小时劝人换卡是句蠢话），事实一直说。
    /// 「哪一段说事实」只在这一处判，命令行与界面各问一次。
    #[must_use]
    pub fn fact(self, filesystem: &Filesystem) -> Option<String> {
        (self == Self::TooBig).then(|| filesystem.max_file_fact())
    }

    /// 报告里固定的排列顺序。
    #[must_use]
    pub fn all() -> [Self; 5] {
        [
            Self::TooBig,
            Self::BadName,
            Self::NameTooLong,
            Self::PathTooLong,
            Self::Collision,
        ]
    }
}

/// 没说是哪一份的时候就是**不作声称**那一份（[`Filesystem::unlimited`]）：与子库没挑过档案时走的是同一份。
impl Default for Filesystem {
    fn default() -> Self {
        Self::unlimited()
    }
}

impl Filesystem {
    /// 什么都不检查的那一份。
    #[must_use]
    pub fn unlimited() -> Self {
        Self {
            name: "无限制".to_string(),
            note: "不作任何声称".to_string(),
            max_file_bytes: None,
            max_name_chars: None,
            max_path_chars: None,
            forbidden: BTreeSet::new(),
            reserved_stems: BTreeSet::new(),
            claim: Claim {
                cite: "这不是一份文件系统声明，是「不做检查」这个选择本身".to_string(),
                verified: "2026-09-02".to_string(),
            },
        }
    }

    /// 这份声明**一条约束都不说**吗（不设单文件上限、不设名字与路径长度、不禁字符、没有保留名）——「不作声称」那一份就是。
    /// 目标设置里「卡是 …」那半句只在它说了点什么时才写（票 `gui-looks-like-the-design/21`）。
    #[must_use]
    pub fn claims_nothing(&self) -> bool {
        self.max_file_bytes.is_none()
            && self.max_name_chars.is_none()
            && self.max_path_chars.is_none()
            && self.forbidden.is_empty()
            && self.reserved_stems.is_empty()
    }

    /// **单文件上限那句事实**：「exFAT 不限制单文件大小。」「FAT32 单文件不能超过 4 GiB。」
    ///
    /// 差量预览「放不进目标」里「超过单文件上限」那一段**零条也说**（照稿，差距 D-20）：它说的是这张卡的事实，
    /// 不是劝告——劝告（[`RejectReason::advice`]）照旧有条才说。「不作声称」那一份什么都不查，不能说成「不限制」
    /// （那是一句关于卡的声称），只说这份档案不查。
    #[must_use]
    pub fn max_file_fact(&self) -> String {
        match self.max_file_bytes {
            Some(limit) => format!("{} 单文件不能超过 {}。", self.name, 上限的说法(limit)),
            None if self.claims_nothing() => "这份能力档案不检查单文件大小。".to_string(),
            None => format!("{} 不限制单文件大小。", self.name),
        }
    }

    /// 不收的那几个字排成一串给人读：`\ / : * ? " < > |`。
    ///
    /// 次序照 Windows 改名时那句提示（设计稿也是这个次序）；声明里多出来的字按码位排在后头。
    #[must_use]
    pub fn forbidden_shown(&self) -> String {
        const WINDOWS: &str = "\\/:*?\"<>|";
        let mut chars: Vec<char> = self.forbidden.iter().copied().collect();
        chars.sort_by_key(|ch| (WINDOWS.find(*ch).unwrap_or(usize::MAX), *ch));
        chars.into_iter().map(字形).collect::<Vec<_>>().join(" ")
    }

    /// 这个字目标收不收：声明里点名的那几个，加上（有声明时）一切控制字符。
    fn refuses(&self, ch: char) -> bool {
        self.forbidden.contains(&ch) || (!self.forbidden.is_empty() && ch.is_control())
    }

    /// 这一份放得进去吗；放不进去就说清是哪一条拦下的（[`Barred`]）。
    ///
    /// `path` 是相对子库根的路径（键，`/` 分隔、NFC）；`prefix_chars` 是子库根本身
    /// 那串路径有多长——**路径上限比的是完整路径**，同一份内容挂在 `E:\Games` 与挂在
    /// 一条很深的路径底下结论会不同，那是实情不是缺陷。
    ///
    /// 文件名不收时**连不收的是什么一起交出来**（[`BadName`]）：屏上那一行右头照它写「含有「:」」，
    /// 不从 [`Barred::detail`] 那句话里抠——一个判断只在这一处做（ADR-0024）。
    #[must_use]
    pub fn screen(&self, path: &str, bytes: u64, prefix_chars: usize) -> Option<Barred> {
        if let Some(limit) = self.max_file_bytes
            && bytes > limit
        {
            return Some(Barred::because(
                RejectReason::TooBig,
                format!(
                    "{} 超过 {} 的单文件上限 {}",
                    crate::report::human_bytes(bytes),
                    self.name,
                    crate::report::human_bytes(limit),
                ),
            ));
        }
        for segment in path.split('/') {
            if segment.is_empty() {
                continue;
            }
            if let Some(bad) = segment.chars().find(|ch| self.refuses(*ch)) {
                // **整条路径里不收的字各记一次**，按出现的次序：只记头一个的话，人改掉它、重扫一遍，
                // 才撞上下一个。前面几段已经查过是干净的，所以这一段起往后数就是全部。
                let mut chars: Vec<char> = Vec::new();
                for ch in path.split('/').flat_map(str::chars) {
                    if self.refuses(ch) && !chars.contains(&ch) {
                        chars.push(ch);
                    }
                }
                return Some(Barred {
                    reason: RejectReason::BadName,
                    detail: format!("「{segment}」里的 {bad:?} 是 {} 不收的字符", self.name),
                    bad_name: Some(BadName::Chars(chars)),
                });
            }
            if let Some(limit) = self.max_name_chars {
                let width = segment.encode_utf16().count();
                if width > limit {
                    return Some(Barred::because(
                        RejectReason::NameTooLong,
                        format!("「{segment}」有 {width} 个字符，{} 至多 {limit}", self.name),
                    ));
                }
            }
            if !self.reserved_stems.is_empty() {
                let stem = segment.split('.').next().unwrap_or(segment).to_uppercase();
                if self.reserved_stems.contains(&stem) {
                    return Some(Barred {
                        reason: RejectReason::BadName,
                        detail: format!("「{segment}」撞上保留名 {stem}"),
                        bad_name: Some(BadName::Reserved(stem)),
                    });
                }
            }
        }
        if let Some(limit) = self.max_path_chars {
            let width = prefix_chars + 1 + path.encode_utf16().count();
            if width > limit {
                return Some(Barred::because(
                    RejectReason::PathTooLong,
                    format!("完整路径 {width} 个字符，{} 至多 {limit}", self.name),
                ));
            }
        }
        None
    }
}

/// [`Filesystem::screen`] 拦下一份时交回来的：哪一条约束、一句话、文件名不收时不收的是什么。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Barred {
    /// 哪一条约束拦下的。
    pub reason: RejectReason,
    /// 说清楚是怎么回事，给人读的一句话。
    pub detail: String,
    /// 文件名不收的是什么；只有 [`RejectReason::BadName`] 那一类有。
    pub bad_name: Option<BadName>,
}

impl Barred {
    /// 不是文件名那一类拦下的：没有 [`Self::bad_name`]。
    fn because(reason: RejectReason, detail: String) -> Self {
        Self {
            reason,
            detail,
            bad_name: None,
        }
    }
}

/// 文件名**不收的是什么**：[`RejectReason::BadName`] 那一类的结构化答案。
///
/// 差量预览「放不进目标」那一行右头照它写（设计稿「含有「:」」，差距 C-2），命令行同印。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum BadName {
    /// 含有目标不收的字：整条路径里按出现的次序，各记一次。
    Chars(Vec<char>),
    /// 去掉扩展名之后撞上保留名（已折成大写）。
    Reserved(String),
}

impl BadName {
    /// 屏上与命令行那半句：「含有「:」」「撞上保留名「CON」」。
    #[must_use]
    pub fn shown(&self) -> String {
        match self {
            Self::Chars(chars) => format!(
                "含有{}",
                chars
                    .iter()
                    .map(|ch| format!("「{}」", 字形(*ch)))
                    .collect::<String>()
            ),
            Self::Reserved(stem) => format!("撞上保留名「{stem}」"),
        }
    }
}

/// 一个字写给人看是什么样：控制字符画不出来，写成 `\u{…}`；别的照原样。
fn 字形(ch: char) -> String {
    if ch.is_control() {
        ch.escape_unicode().to_string()
    } else {
        ch.to_string()
    }
}

/// 一个单文件上限写给人看：正好是整 GiB（或者差一个字节就整——FAT32 那条记的是 4 GiB 减 1 字节，文件长度字段
/// 是 32 位）写成「4 GiB」，别的照 [`human_bytes`](crate::report::human_bytes)。
fn 上限的说法(limit: u64) -> String {
    const GIB: u64 = 1 << 30;
    for 整 in [limit, limit.saturating_add(1)] {
        if 整 >= GIB && 整 % GIB == 0 {
            return format!("{} GiB", 整 / GIB);
        }
    }
    crate::report::human_bytes(limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::Roster;

    #[test]
    fn 不作声称的文件系统一条约束都没有_真的文件系统有() {
        // 目标设置里「卡是 …」那半句只在文件系统真有声称时才写（票 `gui-looks-like-the-design/21`）。
        assert!(Filesystem::unlimited().claims_nothing());
        let roster = Roster::builtin();
        for (name, 不作声称) in [("无限制", true), ("exFAT", false), ("FAT32", false)] {
            let filesystem = roster
                .filesystems()
                .iter()
                .find(|filesystem| filesystem.name == name)
                .unwrap_or_else(|| panic!("内置名册里该有「{name}」"));
            assert_eq!(filesystem.claims_nothing(), 不作声称, "{name}");
        }
    }
}
