//! **选择集报告**：这个子库眼下选中了多少条、共多少容量、装不装得下。
//!
//! 与体检、命中率、刮削、标题、导出那几份同一个形状——从中立库折出来（ADR-0001），
//! 不碰主库一个字节，也不需要目标设备在位。
//!
//! ## 它必须说出口的四件事
//!
//! 1. **选中了多少条、共多少容量**，按平台拆开。这是用户问这份报告的第一个问题。
//! 2. **每条规则各命中多少**。选出来的数不对时，得看得出是哪一条写错了。
//! 3. **例外起没起作用**。排除掉的里有几个是规则本来会选中的——那几条才是真正在
//!    干活的例外；收入的里有几个规则本来也会选中——那几条是多余的，删掉不影响结果。
//! 4. **超没超容量上限，超了多少**。ADR-0016 定死了**不自动截断**：报出超出量与
//!    按体积排序的裁剪建议，砍谁由用户决定。

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde::Serialize;

use crate::report::{heading, human_bytes, pad, thousands};

use super::{
    LoadedSelection, Selected, Sublibrary, Trim, VariantFacts, over_capacity, trim_suggestions,
};

/// 报告里例外那一栏最多列几个键。
const EXAMPLES: usize = 10;

/// 一条规则在报告里的样子。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RuleLine {
    /// 序号。**命令行拿它删规则**，所以它要与库里存的那个序号是同一个。
    pub ordinal: i64,
    /// 用户写的原文。
    pub text: String,
    /// 这条规则自己命中多少个变体（不扣例外、不扣与别条的重叠）。
    pub hits: u64,
}

/// 一条**读不懂**的规则在报告里的样子。
///
/// 它必须进报告而不是只写到 stderr：照 JSON 核对的人看不到 stderr，
/// 而「有一条规则被跳过了」正是他最需要知道的一件事——少选出来的东西全在那条里。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct BrokenRuleLine {
    /// 序号。
    pub ordinal: i64,
    /// 存进去的原文。
    pub text: String,
    /// 错在哪。
    pub error: String,
}

/// 一个平台在报告里的样子。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PlatformLine {
    /// 平台名。
    pub platform: String,
    /// 选中几个变体。
    pub variants: u64,
    /// 共多少字节（下界）。
    pub bytes: u64,
}

/// 一份选择集报告。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SelectionReport {
    /// 中立库在哪。
    pub catalog: String,
    /// 子库叫什么。
    pub name: String,
    /// 目标设备上的子库根。
    pub target: String,
    /// 前端格式。
    pub format: String,
    /// 容量上限；`None` 表示不设限。
    pub capacity: Option<u64>,
    /// 库里一共多少个变体——**选中数的分母**。
    pub variants: u64,
    /// 选中多少个变体。
    pub picked: u64,
    /// 选中的容量合计（下界，ADR-0021）。
    pub bytes: u64,
    /// 选中的变体分属多少个「作品 × 平台」。
    ///
    /// **不等于前端条目数**：收敛与「哪些不导出」（附属内容、非游戏资产、补丁）
    /// 落在导出那一步（ADR-0013、票 20）。这里只报分组数，不冒充那个数。
    pub anchors: u64,
    /// 逐条规则。
    pub rules: Vec<RuleLine>,
    /// 读不懂、这一趟没参与求值的那几条。
    pub broken_rules: Vec<BrokenRuleLine>,
    /// 按平台拆开。
    pub platforms: Vec<PlatformLine>,
    /// 例外收入了几个。
    pub forced_in: u64,
    /// 其中规则本来也会选中的（多余的例外）。
    pub forced_in_redundant: u64,
    /// 例外排除了几个。
    pub forced_out: u64,
    /// 其中规则本来会选中的（真的起作用的例外）。
    pub forced_out_effective: u64,
    /// 指向库里没有这个变体的例外。
    pub missing_exceptions: Vec<String>,
    /// 规则引到、但这份库里一条数据都没有的维度。
    pub thin_dimensions: Vec<String>,
    /// 超出容量上限多少字节；没超或没设上限时是 `None`。
    pub over_capacity: Option<u64>,
    /// 超了的话，按体积排序的裁剪建议。**不自动截断**（ADR-0016）。
    pub trim_suggestions: Vec<Trim>,
}

impl SelectionReport {
    /// 从一次求值折出报告。
    ///
    /// 收整份 [`LoadedSelection`] 而不是拆开的「规则 + 序号」：那两半本来就是一起
    /// 从库里读出来的，拆开传就得再补一个「对不齐怎么办」的分支，而那个分支
    /// 构造上根本走不到。
    #[must_use]
    pub fn build(
        catalog: &str,
        sublibrary: &Sublibrary,
        loaded: &LoadedSelection,
        facts: &[VariantFacts],
        selected: &Selected,
    ) -> Self {
        let mut platforms: BTreeMap<String, PlatformLine> = BTreeMap::new();
        for picked in &selected.picked {
            let name = picked
                .platform
                .clone()
                .unwrap_or_else(|| crate::report::UNKNOWN_PLATFORM_LABEL.to_string());
            let line = platforms.entry(name.clone()).or_insert(PlatformLine {
                platform: name,
                ..PlatformLine::default()
            });
            line.variants += 1;
            line.bytes += picked.bytes;
        }
        // 「作品 × 平台」的分组数，与 `adapter::converge` 的锚点是同一套算法：
        // 认出了作品就按作品，没认出就一个变体一组。**平台未知的写法要与按平台那一栏
        // 一致**，否则同一份报告里会出现两套「没有平台」的表示。
        //
        // 同名的两部作品会并成一组——那是**作品锚点是作品名**这件事的后果（挂账 D53），
        // 收敛那一侧一模一样，不是这里另开的口子。
        let by_key: BTreeMap<&str, &VariantFacts> = facts
            .iter()
            .map(|variant| (variant.key.as_str(), variant))
            .collect();
        let mut anchors: BTreeSet<(String, String)> = BTreeSet::new();
        for picked in &selected.picked {
            let platform = picked
                .platform
                .clone()
                .unwrap_or_else(|| crate::report::UNKNOWN_PLATFORM_LABEL.to_string());
            let anchor = by_key
                .get(picked.key.as_str())
                .and_then(|variant| variant.work.clone())
                .unwrap_or_else(|| picked.key.clone());
            anchors.insert((platform, anchor));
        }

        // 超没超与砍谁，与同步计划器共用一套算法（`sublibrary::over_capacity` /
        // `trim_suggestions`）：两处各写一遍的话，「这份报告说装得下、那份说砍这几个」
        // 这种对不上的账迟早会出现。
        let over = over_capacity(sublibrary.capacity, selected.bytes);
        let trims = if over.is_some() {
            trim_suggestions(
                selected
                    .picked
                    .iter()
                    .map(|picked| (picked.key.clone(), picked.bytes)),
            )
        } else {
            Vec::new()
        };

        Self {
            catalog: catalog.to_string(),
            name: sublibrary.name.clone(),
            target: sublibrary.target.clone(),
            format: sublibrary.format.clone(),
            capacity: sublibrary.capacity,
            variants: facts.len() as u64,
            picked: selected.picked.len() as u64,
            bytes: selected.bytes,
            anchors: anchors.len() as u64,
            rules: loaded
                .selection
                .rules
                .iter()
                .zip(&loaded.ordinals)
                .enumerate()
                .map(|(index, (rule, ordinal))| RuleLine {
                    ordinal: *ordinal,
                    text: rule.text.clone(),
                    hits: selected.rule_hits.get(index).copied().unwrap_or(0),
                })
                .collect(),
            broken_rules: loaded
                .broken
                .iter()
                .map(|broken| BrokenRuleLine {
                    ordinal: broken.ordinal,
                    text: broken.text.clone(),
                    error: broken.error.to_string(),
                })
                .collect(),
            platforms: platforms.into_values().collect(),
            forced_in: selected.forced_in,
            forced_in_redundant: selected.forced_in_redundant,
            forced_out: selected.forced_out,
            forced_out_effective: selected.forced_out_effective,
            missing_exceptions: selected.missing_exceptions.clone(),
            thin_dimensions: selected
                .thin_dimensions
                .iter()
                .map(|name| (*name).to_string())
                .collect(),
            over_capacity: over,
            trim_suggestions: trims,
        }
    }

    /// 打给人看的那一份。
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "子库 {}", self.name);
        let _ = writeln!(out, "{}", "═".repeat(24));
        let _ = writeln!(out, "目标            {}", self.target);
        let _ = writeln!(out, "前端格式        {}", self.format);
        let _ = writeln!(
            out,
            "容量上限        {}",
            self.capacity
                .map_or_else(|| "不设限".to_string(), human_bytes)
        );
        let _ = writeln!(out, "中立库          {}", self.catalog);

        heading(&mut out, "选中");
        let _ = writeln!(
            out,
            "变体            {} / {}（作品 × 平台 {} 组）",
            thousands(self.picked),
            thousands(self.variants),
            thousands(self.anchors),
        );
        let _ = writeln!(out, "容量            {}", human_bytes(self.bytes));
        let _ = writeln!(
            out,
            "容量是**下界**：元数据读不到的成员按 0 计入（ADR-0021）。"
        );

        heading(&mut out, "规则");
        if self.rules.is_empty() {
            let _ = writeln!(
                out,
                "一条规则都没有。`romcat sublibrary rule {} --add \"平台=GB 且 中文=汉化\"`",
                self.name
            );
        } else {
            let _ = writeln!(out, "{}{}规则", pad("序号", 6), pad("命中", 10));
            for rule in &self.rules {
                let _ = writeln!(
                    out,
                    "{}{}{}",
                    pad(&rule.ordinal.to_string(), 6),
                    pad(&thousands(rule.hits), 10),
                    rule.text,
                );
            }
            let _ = writeln!(
                out,
                "命中是**每条各自算**的：规则之间有重叠时这几个数加起来会多于选中数。"
            );
        }
        if !self.broken_rules.is_empty() {
            let _ = writeln!(
                out,
                "⚠️ 有 {} 条规则读不懂，这一趟没参与求值——少选出来的东西全在它们里面：",
                thousands(self.broken_rules.len() as u64),
            );
            for broken in &self.broken_rules {
                let _ = writeln!(
                    out,
                    "  {}{}——{}",
                    pad(&broken.ordinal.to_string(), 6),
                    broken.text,
                    broken.error,
                );
            }
        }
        if !self.thin_dimensions.is_empty() {
            let _ = writeln!(
                out,
                "⚠️ 规则引到的 {} 在这份中立库里一条数据都没有——用到它的规则一个都选不出来。",
                self.thin_dimensions.join("、")
            );
        }

        heading(&mut out, "例外");
        let _ = writeln!(
            out,
            "收入            {}（其中 {} 个规则本来也会选中，删掉不影响结果）",
            thousands(self.forced_in),
            thousands(self.forced_in_redundant),
        );
        let _ = writeln!(
            out,
            "排除            {}（其中 {} 个规则本来会选中，**这几条真的在起作用**）",
            thousands(self.forced_out),
            thousands(self.forced_out_effective),
        );
        if !self.missing_exceptions.is_empty() {
            let _ = writeln!(
                out,
                "另有 {} 条例外指向库里没有的变体——**照旧记着**（盘没插、目录改了名都会这样）：",
                thousands(self.missing_exceptions.len() as u64),
            );
            for key in self.missing_exceptions.iter().take(EXAMPLES) {
                let _ = writeln!(out, "  {key}");
            }
        }

        if !self.platforms.is_empty() {
            heading(&mut out, "按平台");
            let _ = writeln!(out, "{}{}平台", pad("变体", 10), pad("容量", 12));
            for line in &self.platforms {
                let _ = writeln!(
                    out,
                    "{}{}{}",
                    pad(&thousands(line.variants), 10),
                    pad(&human_bytes(line.bytes), 12),
                    line.platform,
                );
            }
        }

        heading(&mut out, "装得下吗");
        match (self.capacity, self.over_capacity) {
            (None, _) => {
                let _ = writeln!(
                    out,
                    "没设容量上限。`--capacity 512GB` 设一个就能在这里看到余量。"
                );
            }
            (Some(limit), None) => {
                let _ = writeln!(
                    out,
                    "装得下：{} / {}，还剩 {}。",
                    human_bytes(self.bytes),
                    human_bytes(limit),
                    human_bytes(limit.saturating_sub(self.bytes)),
                );
            }
            (Some(limit), Some(over)) => {
                let _ = writeln!(
                    out,
                    "**装不下**：{} / {}，超出 {}。",
                    human_bytes(self.bytes),
                    human_bytes(limit),
                    human_bytes(over),
                );
                let _ = writeln!(
                    out,
                    "**不会自动截断**（ADR-0016）——同一套规则在两张不同容量的卡上会选出\n\
                     完全不同的东西，而你无从得知它砍掉了什么。砍谁由你定，最大的几个是：",
                );
                let _ = writeln!(out, "  {}{}变体", pad("腾出", 12), pad("累计", 12));
                for trim in &self.trim_suggestions {
                    let _ = writeln!(
                        out,
                        "  {}{}{}",
                        pad(&human_bytes(trim.bytes), 12),
                        pad(&human_bytes(trim.cumulative), 12),
                        trim.variant,
                    );
                }
                if let Some(last) = self.trim_suggestions.last()
                    && last.cumulative < over
                {
                    let _ = writeln!(
                        out,
                        "这十个**全砍掉也只腾出 {}，还差 {}**。`--json` 出完整的一份。",
                        human_bytes(last.cumulative),
                        human_bytes(over - last.cumulative),
                    );
                }
                let _ = writeln!(
                    out,
                    "排除一个：`romcat sublibrary except {} --exclude <变体的键>`",
                    self.name
                );
            }
        }
        out
    }
}
