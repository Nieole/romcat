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
    /// 转换之后两份东西撞到同一条路径上。
    Collision,
}

impl RejectReason {
    /// 报告里用的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::TooBig => "超过单文件上限",
            Self::BadName => "文件名有不收的字符",
            Self::NameTooLong => "文件名太长",
            Self::PathTooLong => "路径太长",
            Self::Collision => "落点撞车",
        }
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

    /// 这一份放得进去吗；放不进去就说清是哪一条拦下的。
    ///
    /// `path` 是相对子库根的路径（键，`/` 分隔、NFC）；`prefix_chars` 是子库根本身
    /// 那串路径有多长——**路径上限比的是完整路径**，同一份内容挂在 `E:\Games` 与挂在
    /// 一条很深的路径底下结论会不同，那是实情不是缺陷。
    #[must_use]
    pub fn screen(
        &self,
        path: &str,
        bytes: u64,
        prefix_chars: usize,
    ) -> Option<(RejectReason, String)> {
        if let Some(limit) = self.max_file_bytes
            && bytes > limit
        {
            return Some((
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
            if let Some(bad) = segment.chars().find(|ch| {
                self.forbidden.contains(ch) || (!self.forbidden.is_empty() && ch.is_control())
            }) {
                return Some((
                    RejectReason::BadName,
                    format!("「{segment}」里的 {bad:?} 是 {} 不收的字符", self.name),
                ));
            }
            if let Some(limit) = self.max_name_chars {
                let width = segment.encode_utf16().count();
                if width > limit {
                    return Some((
                        RejectReason::NameTooLong,
                        format!("「{segment}」有 {width} 个字符，{} 至多 {limit}", self.name),
                    ));
                }
            }
            if !self.reserved_stems.is_empty() {
                let stem = segment.split('.').next().unwrap_or(segment).to_uppercase();
                if self.reserved_stems.contains(&stem) {
                    return Some((
                        RejectReason::BadName,
                        format!("「{segment}」撞上保留名 {stem}"),
                    ));
                }
            }
        }
        if let Some(limit) = self.max_path_chars {
            let width = prefix_chars + 1 + path.encode_utf16().count();
            if width > limit {
                return Some((
                    RejectReason::PathTooLong,
                    format!("完整路径 {width} 个字符，{} 至多 {limit}", self.name),
                ));
            }
        }
        None
    }
}
