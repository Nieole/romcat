//! **平台纠正**：把库体检那一格算出来的**组**，和沉淀库里人做过的**决定**合到一起
//! （票 `gui-looks-like-the-design/28`）。
//!
//! ## 这一层不判任何事
//!
//! 「哪些算不符」「这一组是从哪个平台到哪个平台」由核心库那一处判据说了算
//! （[`conflicting_platform`](crate::scan::aggregate::conflicting_platform)，分组落在
//! [`ConflictSummary::groups`](super::ConflictSummary::groups)）；「这一组人定过没有」
//! 读的是沉淀库（[`Store::platform_corrections`](crate::verdict::Store::platform_corrections)）。
//! 这里只把两边对起来，再把屏上要说的那几句话说出来——**界面一个数都不许自己算**
//! （ADR-0005、ADR-0024）。
//!
//! ## 处理过的组为什么还在报告里
//!
//! **主库只读**（ADR-0004）：纠正一个字节都不动盘上的文件，所以目录名与扩展名照旧
//! 对不上，下一趟体检照旧数得出这一组。**报告说的是盘上的事实，纠正说的是人的决定**
//! ——两样分开，撤销才回得去。概要那一格要的是**还没处理的那些**，由
//! [`PlatformCorrections::remaining`] 交出来。

use crate::platform::Manifest;
use crate::verdict::{PlatformCorrection, PlatformDecision};

use super::{ConflictGroup, HealthReport, render::thousands};

/// 屏上一组的现状：这一组是什么、人定过没有、改不改都行不行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorrectionGroup {
    /// 这一组本身（核心库的判据算出来的）。
    pub group: ConflictGroup,
    /// 人定过没有；还没处理是 `None`。
    pub decision: Option<PlatformDecision>,
    /// **改不改都行的那一类**：目录说的那台机器跑得了内容说的那个平台的游戏，有意放在
    /// 这个目录也说得通（[`Manifest::runs_games_of`]）。
    pub interchangeable: bool,
}

impl CorrectionGroup {
    /// 这一组叫什么，照设计稿：「GB 目录里的 GBC 游戏」。
    #[must_use]
    pub fn headline(&self) -> String {
        format!(
            "{} 目录里的 {} 游戏",
            self.group.declared, self.group.implied
        )
    }

    /// 凭什么这么判，一句话。判据只有一处（[`ConflictGroup::reason`]），这里原样交出去。
    #[must_use]
    pub fn reason(&self) -> String {
        self.group.reason()
    }

    /// **改不改都行**那一句；这一组不是那一类时是 `None`。
    ///
    /// 与 [`Self::reason`] 分开交，是因为屏上摆在两处：判据那一句摆在组里，这一句贴着
    /// 「保持」那颗按钮（设计稿 `DLG.platfix` 里 `g.keep` 那一支）。
    #[must_use]
    pub fn interchangeable_note(&self) -> Option<String> {
        self.interchangeable.then(|| {
            format!(
                "{} 能运行 {} 的游戏，保持也不影响游玩",
                self.group.declared, self.group.implied
            )
        })
    }

    /// 定过之后那枚标上写什么：「已改为 GBC」/「已保持 GB」；还没定是 `None`。
    #[must_use]
    pub fn settled(&self) -> Option<String> {
        Some(match self.decision? {
            PlatformDecision::ByContent => format!("已改为 {}", self.group.implied),
            PlatformDecision::KeepDeclared => format!("已保持 {}", self.group.declared),
        })
    }

    /// 这一组还算在库体检那一格里吗：人没定过才算。
    #[must_use]
    pub fn pending(&self) -> bool {
        self.decision.is_none()
    }
}

/// 屏上「平台纠正」那一层的全部内容。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlatformCorrections {
    groups: Vec<CorrectionGroup>,
}

impl PlatformCorrections {
    /// 把报告里的组与沉淀库里的决定对起来。
    ///
    /// **只列报告里有的组**：人定过、可盘上已经不再有那一组的（文件挪走了、根移除了），
    /// 那条决定照旧留在沉淀库里（撤销要它），但屏上没有它可画。
    #[must_use]
    pub fn build(
        report: &HealthReport,
        manifest: &Manifest,
        decided: &[PlatformCorrection],
    ) -> Self {
        let groups = report
            .conflicts
            .groups
            .iter()
            .map(|group| CorrectionGroup {
                decision: decided
                    .iter()
                    .find(|one| one.declared == group.declared && one.implied == group.implied)
                    .map(|one| one.decision),
                interchangeable: manifest.runs_games_of(&group.declared, &group.implied),
                group: group.clone(),
            })
            .collect();
        Self { groups }
    }

    /// 每一组，条数多的在前（报告排好的次序）。
    #[must_use]
    pub fn groups(&self) -> &[CorrectionGroup] {
        &self.groups
    }

    /// **还没处理的那几组一共几条**：库体检概要那一格画的就是它。
    #[must_use]
    pub fn remaining(&self) -> u64 {
        self.groups
            .iter()
            .filter(|one| one.pending())
            .map(|one| one.group.count)
            .sum()
    }

    /// 处理过几组。
    #[must_use]
    pub fn handled(&self) -> usize {
        self.groups.iter().filter(|one| !one.pending()).count()
    }

    /// 一共几组。
    #[must_use]
    pub fn len(&self) -> usize {
        self.groups.len()
    }

    /// 一组都没有。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// 概要那一格底下那句小字：处理过组就说处理了几组，一组都没处理就交回 `None`
    /// （那时小字照旧是判据的短写法，[`Finding::hint`](super::Finding::hint)）。
    #[must_use]
    pub fn handled_note(&self) -> Option<String> {
        let handled = self.handled();
        (handled > 0).then(|| {
            format!(
                "已处理 {} 组",
                thousands(u64::try_from(handled).unwrap_or(u64::MAX))
            )
        })
    }
}
