//! 库体检概要里**每一格的明细**：体检明细弹层画的那几行、「导出清单…」写出去的那份纯文本，读的是同一处。
//!
//! 数、路径、原因都从 [`HealthReport`] 里取，措辞在这里——界面与命令行不各写一份（ADR-0024）。
//! **重复拷贝**的完整明细（每一组、每一份）是 [`super::DuplicateDetails`]，要照着它动手处理的走那一份；这里那一格
//! 只列报告里留下的前几组。
//!
//! **只发现并报告**：库体检不改动主库里的任何文件（ADR-0004）。

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use super::HealthReport;
use super::render::{human_bytes, pad, thousands};

/// 库体检概要里说得出明细的那几格。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Finding {
    /// **重复拷贝**：同名且同大小的多份。
    Duplicates,
    /// 目录声明的平台与文件内容对不上。
    PlatformConflicts,
    /// **成型存疑**（词表同名条目）。
    ShapingDoubts,
    /// **不可读**：文件名拿得到、元数据读不到。
    Unreadable,
    /// 未纳入管理的目录：不在任何平台目录下。
    UnmappedDirs,
    /// **附属文件落单**（词表同名条目）。
    StrandedCompanions,
    /// **非游戏资产**。
    NonGameAssets,
}

impl Finding {
    /// 全部几格，照概要上的次序。
    pub const ALL: [Self; 7] = [
        Self::Duplicates,
        Self::PlatformConflicts,
        Self::ShapingDoubts,
        Self::Unreadable,
        Self::UnmappedDirs,
        Self::StrandedCompanions,
        Self::NonGameAssets,
    ];

    /// 这一格叫什么。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Duplicates => "重复拷贝",
            Self::PlatformConflicts => "目录与内容平台不符",
            Self::ShapingDoubts => "成型存疑",
            Self::Unreadable => "不可读",
            Self::UnmappedDirs => "未纳入管理的目录",
            Self::StrandedCompanions => "附属文件落单",
            Self::NonGameAssets => "非游戏资产",
        }
    }

    /// 这一格的数论什么：组、条、处、个。
    #[must_use]
    pub fn unit(self) -> &'static str {
        match self {
            Self::Duplicates => "组",
            Self::PlatformConflicts => "条",
            Self::ShapingDoubts => "处",
            Self::Unreadable
            | Self::UnmappedDirs
            | Self::StrandedCompanions
            | Self::NonGameAssets => "个",
        }
    }

    /// 判据，一句话（与词表对应条目同一个意思）。
    #[must_use]
    pub fn criterion(self) -> &'static str {
        match self {
            Self::Duplicates => "同名且同大小，不读内容，也不算哈希——动手前请自己核一眼",
            Self::PlatformConflicts => {
                "目录说的平台与文件内容说的不一致；目录只是强先验，内容可以推翻它"
            }
            Self::ShapingDoubts => {
                "同一目录里只差碟片标记却各自成了变体；或者一个目录树变体里直接躺着几份各自独立的内容"
            }
            Self::Unreadable => "文件名拿得到、元数据读不到；不是空文件，也不是穿不透的容器",
            Self::UnmappedDirs => "不在任何平台目录下：照扫照报，只是不进识别与刮削",
            Self::StrandedCompanions => "存档、补丁在自己那个目录里找不到同名的主文件",
            Self::NonGameAssets => {
                "路径里有一段目录是 bios：模拟器要它，它本身不是游戏，入库但永不导出"
            }
        }
    }
}

/// 明细里的一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindingRow {
    /// 这一行说的是哪儿：一份文件或一个目录的完整路径；未纳入管理的目录是那个目录的名字，连着文件数与容量。
    pub path: String,
    /// 为什么列在这儿；光看路径就说得清的是 `None`。
    pub reason: Option<String>,
    /// 这一行牵涉的那几条：成型存疑的那几个变体或几份独立内容、一组重复拷贝的那几份；别的格空着。
    pub items: Vec<String>,
    /// 「在文件系统中打开」打开哪个目录：文件是它所在的目录，目录是它自己；只有名字、没有路径的是 `None`。
    pub folder: Option<PathBuf>,
}

impl FindingRow {
    /// 一份文件：打开它所在的目录。
    fn file(path: &str, reason: Option<String>) -> Self {
        Self {
            path: path.to_string(),
            reason,
            items: Vec::new(),
            folder: Path::new(path).parent().map(Path::to_path_buf),
        }
    }
}

impl HealthReport {
    /// 这一格的数。
    #[must_use]
    pub fn finding_count(&self, finding: Finding) -> u64 {
        match finding {
            Finding::Duplicates => self.suspects.duplicate_groups,
            Finding::PlatformConflicts => self.conflicts.total,
            Finding::ShapingDoubts => self.shaping_doubts.total,
            Finding::Unreadable => self.anomalies.unreadable,
            Finding::UnmappedDirs => self.scope.unmapped_dirs,
            Finding::StrandedCompanions => self.stranded_companions.total,
            Finding::NonGameAssets => self.non_game_assets.files,
        }
    }

    /// 这一格的明细，一样一行，**照报告里留下的样例**（每一类有上限）；比 [`Self::finding_count`] 少时，
    /// 少的那几个没列出。
    #[must_use]
    pub fn finding_rows(&self, finding: Finding) -> Vec<FindingRow> {
        match finding {
            Finding::Duplicates => self
                .suspects
                .top_duplicates
                .iter()
                .map(|group| FindingRow {
                    path: group.name.clone(),
                    reason: Some(format!(
                        "{} · 可腾出 {} · 每份 {} · 共 {} 份",
                        group.platforms.join("、"),
                        human_bytes(group.reclaimable_bytes()),
                        human_bytes(group.size),
                        thousands(group.count),
                    )),
                    items: group.paths.clone(),
                    folder: None,
                })
                .collect(),
            Finding::PlatformConflicts => self
                .conflicts
                .examples
                .iter()
                .map(|conflict| {
                    FindingRow::file(
                        &conflict.path,
                        Some(format!(
                            "目录说 {}，内容是 {}（{}）",
                            conflict.declared,
                            conflict.implied,
                            conflict.evidence.label()
                        )),
                    )
                })
                .collect(),
            Finding::ShapingDoubts => self
                .shaping_doubts
                .examples
                .iter()
                .map(|doubt| FindingRow {
                    path: doubt.at.clone(),
                    reason: Some(match &doubt.platform {
                        Some(platform) => format!("{platform} · {}", doubt.reason()),
                        None => doubt.reason(),
                    }),
                    items: doubt.items.clone(),
                    folder: Some(PathBuf::from(&doubt.at)),
                })
                .collect(),
            Finding::Unreadable => self
                .anomalies
                .unreadable_examples
                .iter()
                .map(|path| FindingRow::file(path, None))
                .collect(),
            Finding::UnmappedDirs => self
                .scope
                .unmapped_examples
                .iter()
                .map(|example| FindingRow {
                    path: example.clone(),
                    reason: None,
                    items: Vec::new(),
                    folder: None,
                })
                .collect(),
            Finding::StrandedCompanions => self
                .stranded_companions
                .examples
                .iter()
                .map(|stranded| {
                    FindingRow::file(
                        &stranded.path,
                        Some(format!("{} · {}", stranded.kind.label(), stranded.reason())),
                    )
                })
                .collect(),
            Finding::NonGameAssets => self
                .non_game_assets
                .examples
                .iter()
                .map(|path| FindingRow::file(path, None))
                .collect(),
        }
    }

    /// 这一格的明细写成**纯文本**：「导出清单…」写出去的就是它。判据、数、每一行的路径与原因；样例截断时说清另有几个。
    #[must_use]
    pub fn render_finding(&self, finding: Finding) -> String {
        const LABEL: usize = 10;
        let count = self.finding_count(finding);
        let rows = self.finding_rows(finding);
        let mut out = String::new();
        let title = format!("{}明细", finding.label());
        let _ = writeln!(out, "{title}");
        let _ = writeln!(out, "{}", "═".repeat(20));
        let _ = writeln!(out, "{}{}", pad("判据", LABEL), finding.criterion());
        let _ = writeln!(
            out,
            "{}{} {}",
            pad("数量", LABEL),
            thousands(count),
            finding.unit()
        );
        let listed = u64::try_from(rows.len()).unwrap_or(u64::MAX);
        if listed < count {
            let _ = writeln!(
                out,
                "{}列出 {} {}，另有 {} {}没列出（报告里每一类只留前几个样例）",
                pad("样例", LABEL),
                thousands(listed),
                finding.unit(),
                thousands(count - listed),
                finding.unit(),
            );
        }
        let _ = writeln!(
            out,
            "\n只发现并报告：库体检不改动主库里的任何文件（ADR-0004）。"
        );
        if rows.is_empty() {
            let _ = writeln!(out, "\n没有这一类。");
            return out;
        }
        let _ = writeln!(out);
        for row in &rows {
            let _ = writeln!(out, "{}", row.path);
            if let Some(reason) = &row.reason {
                let _ = writeln!(out, "    {reason}");
            }
            for item in &row.items {
                let _ = writeln!(out, "    {item}");
            }
        }
        out
    }
}
