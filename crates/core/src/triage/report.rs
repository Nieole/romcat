//! **待确认队列**的报告：队列里有多少条、按哪个轴一次能覆盖多少、每条的候选与依据。
//!
//! 它和体检报告、命中率报告是同一个形状——**从中立库与沉淀库折出来**，一个字节都不读
//! 主库（ADR-0001）。
//!
//! ## 为什么按三个轴分组是报告的正文而不是附录
//!
//! 队列有一万六千多条，人要做的第一个决定是「**从哪一批下手**」。按目录、按候选作品、
//! 按命名规律分出来的那三张表，每一行就是一条 `romcat triage decide` 能一次覆盖的批
//! ——**报告直接告诉你这一条命令值多少**。少了它，批量裁决这件事在命令行上就只能靠猜。

use std::fmt::Write as _;

use serde::Serialize;

use super::batch;
use super::{Axis, Item, tally, tally_by};
use crate::catalog::identify::NOT_RUN_LABEL;
use crate::report::{heading, human_bytes, pad, thousands};
use crate::verdict;

pub use super::GroupRow;

/// 报告里印出来的一条**候选**。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CandidateRow {
    /// 第几条（`--pick` 要的就是这个数）。
    pub nth: usize,
    /// **置信度**。
    pub confidence: String,
    /// 哪个数据源。
    pub source: String,
    /// 条目名。
    pub game: String,
    /// 中文记号。
    pub chinese: Option<String>,
    /// **依据**：这条候选是怎么来的。
    pub evidence: String,
}

/// 报告里印出来的**一批变体**：待确认屏上那张卡片，连它照着抄得走的那串字。
///
/// 三个轴那几张表印的是 [`GroupRow`]（一个标签一个数），这一张多一样东西：**选择器
/// 本身**。按目录那个轴的标签就是 `--under` 的值，抄下来即可；依据形状不是——它是个
/// 五段的结构，屏上写的那半截（`源 / DAT / 哈希口径`）不够折回一个选择器。所以这里
/// 把 [`Shape::selector`](super::Shape::selector) 折出来的整串字单摆一列。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ShapeRow {
    /// `--shape` 收的那串字。**它就是屏上那张卡片的身份**。
    pub selector: String,
    /// 那句**共同依据**：整批通过时人验证的正是它。
    pub why: String,
    /// 多少条。
    pub count: u64,
    /// 这一批**整批通过**说得成立吗（只有单候选那一档算数）。
    pub passable: bool,
}

/// 报告里印出来的一条队列条目。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ItemRow {
    /// 变体的键。
    pub key: String,
    /// 平台。
    pub platform: Option<String>,
    /// 识别结论。
    pub state: String,
    /// 「无判据」「跳过」的理由。
    pub reason: Option<String>,
    /// 容量。
    pub bytes: u64,
    /// 裁决会钉在什么上：**内容**（可分享）还是**路径**（只在本机成立）。
    ///
    /// 只有跑过 [`fill_prints`](super::fill_prints) 的那几条才说得准，
    /// 而 [`QueueReport::build`] 印出来的正是那几条。
    pub anchor: String,
    /// 全部候选。
    pub candidates: Vec<CandidateRow>,
}

/// **待确认队列**的报告。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct QueueReport {
    /// 中立库在哪。
    pub catalog: String,
    /// 沉淀库在哪。
    pub store: String,
    /// 识别跑过没有。没跑过时队列是空的，而那不是「没什么可裁的」。
    pub identified: bool,
    /// 整个队列有多少条。
    pub queue: u64,
    /// 库里有多少个变体**还没识别**（[`NOT_RUN_LABEL`]）。
    ///
    /// **它们不在队列里，也不该在**：一条候选都没有、连结论都没有，裁不了。但只说
    /// 「队列 N 条待裁决」，用户会读成「库里只剩 N 条没定下来」——而实情是另有这么多个
    /// 连问都还没问过（识别中断，或者识别跑完之后又扫进了新文件、加了新的**根**）。
    ///
    /// **选择器不筛它**：与 [`queue`](Self::queue) 同一个口径，说的始终是整个库。
    /// [`not_identified`](Self::not_identified) 那一份里它是 0——那时整库都还没识别，
    /// 这个数没有信息量，那句话本身已经说清了。
    pub not_run: u64,
    /// 这次的选择器选中多少条。
    pub selected: u64,
    /// 选中的按识别结论分。
    pub by_state: Vec<GroupRow>,
    /// 选中的按平台分。
    pub by_platform: Vec<GroupRow>,
    /// **按目录**分——批量裁决的第一个轴。
    pub by_directory: Vec<GroupRow>,
    /// **按候选作品**分——第二个轴。一条候选都没有的不进这张表。
    pub by_candidate_work: Vec<GroupRow>,
    /// **按命名规律**分——第三个轴：名字里 `[…]` `(…)` 括起来的那几段带几条。
    pub by_name_mark: Vec<GroupRow>,
    /// **按依据形状**分——待确认屏一级分批的那几批（[`batch::batches`]）。
    ///
    /// 它与上面三个轴不是同一件事：那三个是**批量裁决的轴**（ADR-0002 点名的），
    /// 这一张是**队列自己的分法**——「工具凭什么这么认为」相同的聚成一批，
    /// 而整批通过时人验证的正是那句共同依据。屏上那一列卡片与这张表出自同一个函数，
    /// 所以照着抄一条 `--shape` 选中的就是屏上点那张卡片的同一批（ADR-0005）。
    pub by_shape: Vec<ShapeRow>,
    /// 「无判据 / 跳过」的理由分布：**为什么没定下来**。
    pub by_reason: Vec<GroupRow>,
    /// 印出来的那几条。
    pub shown: Vec<ItemRow>,
    /// 印了几条之外还剩多少条没印。
    pub more: u64,
    /// 沉淀库眼下的账。
    pub verdicts: verdict::Counts,
}

/// 分组的表最多印几行。再多就不是给人看的了。
const TOP: usize = 12;

impl QueueReport {
    /// 折一份报告出来。
    ///
    /// `items` 是**选择器选中的**那些，`queue` 是整个队列有多少条——两个数都要说，
    /// 不然用户看不出自己的选择器是选窄了还是库里本来就只有这些。
    ///
    /// `not_run` 是库里**还没识别**的变体数
    /// （[`Catalog::not_run_count`](crate::catalog::Catalog::not_run_count)）。它是
    /// **第三个数**，与前两个都不是一回事：那两个说的是「要人裁的有多少」，这个说的是
    /// 「连问都还没问过的有多少」。不给它，队列就把那批变体整个咽了下去。
    #[must_use]
    pub fn build(
        catalog: &str,
        store: &str,
        queue: u64,
        not_run: u64,
        items: &[Item],
        counts: verdict::Counts,
        show: usize,
    ) -> Self {
        let mut report = Self {
            catalog: catalog.to_string(),
            store: store.to_string(),
            identified: true,
            queue,
            not_run,
            selected: u64::try_from(items.len()).unwrap_or(u64::MAX),
            verdicts: counts,
            ..Self::default()
        };
        report.by_state = tally_by(items, |item| vec![item.state.label().to_string()]);
        report.by_platform = tally_by(items, |item| {
            vec![
                item.variant
                    .platform
                    .clone()
                    .unwrap_or_else(|| crate::report::UNKNOWN_PLATFORM_LABEL.to_string()),
            ]
        });
        // 三个轴走 `triage::tally`：报告印出来的「一条命令覆盖多少」与选择器真的选中
        // 多少，必须出自同一个 `Axis`（见 `Axis` 的文档）。
        report.by_directory = tally(items, Axis::Directory);
        report.by_candidate_work = tally(items, Axis::CandidateWork);
        report.by_name_mark = tally(items, Axis::NameMark);
        // **一级分批走的是与屏上同一个函数**（`batch::batches`）：报告印出来的那串字
        // 与屏上那张卡片必须是同一批，各算一份迟早会漂开（ADR-0005）。
        report.by_shape = batch::batches(items)
            .into_iter()
            .map(|one| ShapeRow {
                selector: one.shape.selector(),
                why: one.why(),
                count: one.count,
                passable: one.passable(),
            })
            .collect();
        report.by_reason = tally_by(items, |item| {
            item.reason.clone().map(|r| vec![r]).unwrap_or_default()
        });
        report.shown = items.iter().take(show).map(row_of).collect();
        report.more = u64::try_from(items.len().saturating_sub(report.shown.len())).unwrap_or(0);
        report
    }

    /// 还没跑过识别时的那一份。**队列是空的，但不是「没什么可裁的」**。
    #[must_use]
    pub fn not_identified(catalog: &str, store: &str, counts: verdict::Counts) -> Self {
        Self {
            catalog: catalog.to_string(),
            store: store.to_string(),
            identified: false,
            verdicts: counts,
            ..Self::default()
        }
    }

    /// 排成给人看的文本。
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "待确认队列");
        let _ = writeln!(out, "{}", "═".repeat(20));
        let _ = writeln!(out, "中立库          {}", self.catalog);
        let _ = writeln!(out, "沉淀库          {}", self.store);
        if !self.identified {
            let _ = writeln!(
                out,
                "\n还没跑过识别，队列无从谈起。先跑一次 `romcat identify`。"
            );
            return out;
        }
        let _ = writeln!(out, "队列            {} 条待裁决", thousands(self.queue));
        // **还没识别的那些单说一句。** 它们不在上面那个数里（没有候选，裁不了），
        // 而咽下去的话「队列 N 条」会被读成「库里只剩 N 条没定下来」。
        // 该做的事也不一样——这一句指向 `identify`，不是指向裁决。
        if self.not_run > 0 {
            let _ = writeln!(
                out,
                "{NOT_RUN_LABEL}        另有 {} 个变体连识别都还没跑过：\
                 它们**不在队列里**（一条候选都没有，裁不了），选择器也筛不到。\n\
                 {:16}先跑一趟 `romcat identify` 把它们补上。",
                thousands(self.not_run),
                "",
            );
        }
        if self.selected != self.queue {
            let _ = writeln!(
                out,
                "选中            {} 条（选择器筛过）",
                thousands(self.selected)
            );
        }
        if self.selected == 0 {
            let _ = writeln!(
                out,
                "\n一条都没选中。选择器写宽一点，或者去掉它看整个队列。"
            );
            return out;
        }

        group(&mut out, "按识别结论", &self.by_state, self.selected);
        group(&mut out, "按平台", &self.by_platform, self.selected);
        heading(&mut out, &axis_heading(Axis::Directory));
        table(&mut out, &self.by_directory, self.selected);
        heading(&mut out, &axis_heading(Axis::CandidateWork));
        if self.by_candidate_work.is_empty() {
            let _ = writeln!(
                out,
                "（选中的这些一条候选都没有——那正是队列的常态：真机上未命中与无判据\n\
                 加起来一万六千多条，候选数都是 0。这一批要走 `--work` 手工指定。）"
            );
        } else {
            table(&mut out, &self.by_candidate_work, self.selected);
        }
        heading(&mut out, &axis_heading(Axis::NameMark));
        if self.by_name_mark.is_empty() {
            let _ = writeln!(out, "（选中的这些名字里一个记号都没有）");
        } else {
            table(&mut out, &self.by_name_mark, self.selected);
        }
        heading(&mut out, "按依据形状——一条 `--shape` 覆盖多少（屏上那一列卡片）");
        shape_table(&mut out, &self.by_shape, self.selected);
        if !self.by_reason.is_empty() {
            heading(&mut out, "为什么没定下来");
            table(&mut out, &self.by_reason, self.selected);
        }

        heading(&mut out, "队列里的条目");
        for item in &self.shown {
            let _ = writeln!(
                out,
                "\n{}  [{}]{}",
                item.key,
                item.state,
                item.platform
                    .as_deref()
                    .map(|p| format!("  平台 {p}"))
                    .unwrap_or_default()
            );
            let _ = writeln!(
                out,
                "  容量 {}；裁决钉在**{}**上",
                human_bytes(item.bytes),
                item.anchor,
            );
            if let Some(reason) = &item.reason {
                let _ = writeln!(out, "  理由：{reason}");
            }
            if item.candidates.is_empty() {
                let _ = writeln!(out, "  候选：一条都没有——要裁决就得手工指定作品");
            }
            for candidate in &item.candidates {
                let _ = writeln!(
                    out,
                    "  {}. [{}] {} 《{}》{}",
                    candidate.nth,
                    candidate.confidence,
                    candidate.source,
                    candidate.game,
                    candidate
                        .chinese
                        .as_deref()
                        .map(|mark| format!("  {mark}"))
                        .unwrap_or_default()
                );
                let _ = writeln!(out, "     依据：{}", candidate.evidence);
            }
        }
        if self.more > 0 {
            let _ = writeln!(
                out,
                "\n……还有 {} 条没印（`--limit` 调多少条）。",
                thousands(self.more)
            );
        }

        heading(&mut out, "沉淀库");
        let counts = &self.verdicts;
        let _ = writeln!(
            out,
            "已裁决 {} 条：定成发行版 {}、确认没有发行版 {}、认不出 {}",
            thousands(counts.total),
            thousands(counts.releases),
            thousands(counts.no_release),
            thousands(counts.unknown),
        );
        let _ = writeln!(
            out,
            "其中钉在内容上的 {} 条（**可导出分享**），只钉得住本机路径的 {} 条；\
             补了汉化组的 {} 条",
            thousands(counts.content),
            thousands(counts.path),
            thousands(counts.with_team),
        );
        // **匹配裁决单独一行**（票 05）：它与上面那几个数**不是同一张表**——上面说的是
        // 「这份内容是什么」，这一行说的是「某个源撞出来的那一次匹配对不对」。
        // 加起来会让两个问题看着像一个。一条都没有时整行不印，免得多一行永远是 0 的噪音。
        if counts.matches > 0 {
            let _ = writeln!(
                out,
                "另有**匹配裁决** {} 条（说「就是这条」的 {}）：一条管住那一次匹配带来的\
                 全部字段，`romcat zh matches <变体键>` 看得出是哪几个",
                thousands(counts.matches),
                thousands(counts.matches_accepted),
            );
        }
        out
    }
}

fn row_of(item: &Item) -> ItemRow {
    ItemRow {
        key: item.variant.key.clone(),
        platform: item.variant.platform.clone(),
        state: item.state.label().to_string(),
        reason: item.reason.clone(),
        bytes: item.variant.bytes,
        anchor: if item.print.is_some() {
            verdict::ANCHOR_CONTENT.to_string()
        } else {
            verdict::ANCHOR_PATH.to_string()
        },
        candidates: item
            .candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| CandidateRow {
                nth: index + 1,
                confidence: candidate.confidence.label().to_string(),
                source: candidate.source.clone(),
                game: candidate.game.clone(),
                chinese: candidate.chinese.map(|mark| mark.label().to_string()),
                evidence: candidate.evidence.clone(),
            })
            .collect(),
    }
}

/// 一个轴那张表的表头：**这一条命令值多少**。
fn axis_heading(axis: Axis) -> String {
    format!("{}——一条 `{}` 覆盖多少", axis.label(), axis.selector())
}

fn group(out: &mut String, title: &str, rows: &[GroupRow], total: u64) {
    heading(out, title);
    table(out, rows, total);
}

fn table(out: &mut String, rows: &[GroupRow], total: u64) {
    for row in rows.iter().take(TOP) {
        let label = if row.label.is_empty() {
            "（主库根）"
        } else {
            row.label.as_str()
        };
        #[allow(clippy::cast_precision_loss)]
        let share = if total == 0 {
            0.0
        } else {
            row.count as f64 * 100.0 / total as f64
        };
        let _ = writeln!(
            out,
            "{}{:>8}  {share:5.1}%",
            pad(label, 44),
            thousands(row.count)
        );
    }
    if rows.len() > TOP {
        let _ = writeln!(out, "……另有 {} 组没印", thousands_len(rows.len() - TOP));
    }
}

/// 一级分批那张表：**一行说这一批有多大，下一行给出照着抄的那串字**。
///
/// 不塞进 [`table`]：那张表一行一个标签，而这里一行要摆两样长东西——那句共同依据
/// （一句完整的话）与整串选择器（五段）。挤成一行的话两样都会被截断，而选择器截断了
/// 就是不能抄。**加了单引号**：那串字里有空格，不引起来粘过去就散了。
fn shape_table(out: &mut String, rows: &[ShapeRow], total: u64) {
    for row in rows.iter().take(TOP) {
        #[allow(clippy::cast_precision_loss)]
        let share = if total == 0 {
            0.0
        } else {
            row.count as f64 * 100.0 / total as f64
        };
        let _ = writeln!(
            out,
            "{:>8}  {share:5.1}%  {}{}",
            thousands(row.count),
            row.why,
            if row.passable {
                ""
            } else {
                "（**整批通过说不成立**：候选不是恰好一条）"
            },
        );
        let _ = writeln!(out, "                  --shape '{}'", row.selector);
    }
    if rows.len() > TOP {
        let _ = writeln!(out, "……另有 {} 批没印", thousands_len(rows.len() - TOP));
    }
}

fn thousands_len(value: usize) -> String {
    thousands(u64::try_from(value).unwrap_or(u64::MAX))
}
