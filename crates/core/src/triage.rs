//! **待确认队列**与**批量裁决**。
//!
//! 置信度不足以自动通过的变体聚在这里，由人**批量**裁定；每条裁决沉淀进
//! [`verdict::Store`]，把文件哈希直接钉到发行版上——裁决一次，重装、换机、日后再次
//! 拷进同一个文件时直接精确命中（ADR-0002、ADR-0008）。
//!
//! ## 批量不是锦上添花，是这件事成不成立的分界
//!
//! ADR-0002 的原话：**只能逐条点的队列在几千条规模下等于没有，整个方案会退化成
//! 「全自动瞎填」。** 真机上队列是**一万六千多条**，逐条点不可能。所以选择器与裁决
//! 是**分开的两半**，任何一个选择器都能配任何一种裁决：
//!
//! | 轴 | 选择器 | 一条命令覆盖 |
//! |---|---|---|
//! | 按目录 | [`Filter::under`] | 一个平台目录下的全部待裁决变体 |
//! | 按候选作品 | [`Filter::candidate_work`] | 候选指着同一部作品的全部变体 |
//! | 按命名规律 | [`Filter::name_contains`] | 名字里带同一个汉化组记号的全部变体 |
//!
//! ## 候选集不是天花板
//!
//! [`DecisionSpec::Pick`] 从候选里挑一条，但队列里**大多数变体一条候选都没有**
//! （真机上未命中 11,823 条、无判据 4,537 条，候选数都是 0）。所以
//! [`DecisionSpec::Manual`] 是一等公民：人直接说出作品、平台、地区、**汉化组**与**版本**，
//! 不必先有一条候选。判定「都不对」有两种说法，**分开记**：
//! [`DecisionSpec::NoRelease`] 是「它没有发行版」（同人移植、homebrew），
//! [`DecisionSpec::Unknown`] 是「我看过了，认不出」。
//!
//! ## 队列成员的判据
//!
//! **没有一条自动通过的候选，而且沉淀库还没说过话。** 就这两条：
//!
//! - 自动通过的（精确哈希命中）不占人的时间，那是 ADR-0002 分三档的全部意义。
//! - **跳过**的（补丁、homebrew）默认不在队列里——它们不是「拿不定主意」，是「不该撞
//!   DAT」。要复核它们，把 `跳过` 写进 [`Filter::states`]。
//! - 裁决过的一律退出队列，包括 `认不出` 那一档：**「我看过了，认不出」与「还没人看过」
//!   是两件事**，混在一起的话人会被反复问同一个问题。
//!
//! ## 撤销：粒度是**批**，两边一起回去
//!
//! **批量的胆量来自撤销可信。** 一次「整批通过三千条」按错了却撤不干净，批量这件事本身
//! 就不成立——所以一次 [`apply`] 落下的那些记成一**批**（[`verdict::Batch`]），
//! [`undo_batch`] 把**沉淀库与中立库两边**一起退回这一批落下之前的样子，
//! 撤完当场列队列就看得见它们回来了，**不必重跑识别、也不要 DAT 库在手边**。
//!
//! 撤销放回中立库的那一份不是现编的，是这一批落下之前
//! [`Catalog::stash_conclusions`](crate::catalog::Catalog::stash_conclusions) 收起来的
//! ——**上一趟识别自己算出来的东西**。所以「重算一遍是什么样」与「眼下是什么样」
//! 照旧只有一个答案，[`undo_batch`] 的文档把这笔账算全了（原挂账 D102）。
//!
//! 撤销本身也撤得回来：[`redo_batch`] 把那一批原样放回去，一个字都不必用户重打。
//!
//! **批与批在同一条锚上是叠着的**，于是撤销按落下的**倒序**走：重复拷贝是真机上的常态，
//! 同一条内容锚上的两份分在两批里是常事，而后一批记着的「它盖掉了什么」正是前一批落下的
//! 那条。被后来还在册的那一批盖住的先撤不动——[`undo_batch`] 说清是哪一批盖的，
//! 让人先撤那一批，而不是把它标成已撤、过一会儿裁决又活过来。
//!
//! [`plan_forget`] / [`forget`] 那一对是**另一件事**——按选择器忘掉散落的裁决
//! （典型：别人分享来、`triage import` 收下的那些，它们不属于本机任何一批）。
//! 它只动沉淀库，理由与出口都写在 [`forget`] 上。

pub mod batch;
pub mod queue;
pub mod report;

pub use batch::{Batch, Drill, Fanout, Sample, Scope, Shape};
pub use queue::Queue;

use std::collections::{BTreeMap, BTreeSet};

use crate::catalog::identify::{QueueRow, Tier};
use crate::catalog::{Candidate, Catalog, CatalogError, State, VariantRow};
use crate::dat::chinese::ChineseMark;
use crate::identify::{self, ContentPrint, Projector};
use crate::path::{file_name_of_key, fold};
use crate::verdict::{self, Anchor, Decision, Facts, Store, Verdict, VerdictError};

/// 队列这一层跑不下去的原因。
#[derive(Debug, thiserror::Error)]
pub enum TriageError {
    /// 中立库读写失败。
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    /// 沉淀库读写失败。
    #[error(transparent)]
    Verdict(#[from] VerdictError),
    /// 点名要撤的那一**批**根本不在。
    #[error("沉淀库里没有第 {0} 批。`romcat triage batches` 列得出有哪几批")]
    NoBatch(i64),
    /// 那一批已经撤过了。
    #[error("第 {0} 批已经撤过了。要放回去用 `romcat triage redo --batch {0}`")]
    AlreadyUndone(i64),
    /// 那一批还没撤过，没什么可放回去的。
    #[error("第 {0} 批还没撤过，没什么可放回去的")]
    NotUndone(i64),
    /// 手上这份计划是对着**另一批条目**排的——排完之后队列变过样。
    ///
    /// 措辞的分寸：这不是「数据坏了」，是「**这份计划过期了**」。两份库一个字都没动，
    /// 人要做的只是**重排一份计划**再落下。
    #[error(
        "这份计划是对着另一批条目排的：其中 {missing} 条已经不在手上这一批里了（{names}）。\
         排完计划之后队列变过样——裁掉过其中一条、换过选择器、重列过一次，都会这样。\
         两份库一个字都没动，重排一份计划再落下"
    )]
    StalePlan {
        /// 有几条对不上。
        missing: usize,
        /// 头几条的键，够人认出是哪些。
        names: String,
    },
    /// 那一批被后来的、眼下还在册的一批盖住了，撤不动也放不回去。
    #[error(
        "第 {batch} 批里有 {rows} 条被第 {by} 批盖住了，那一批还在册——\
         这一批在那几条锚上回不去。先撤第 {by} 批（`romcat triage undo --batch {by}`）"
    )]
    CoveredBy {
        /// 点名的那一批。
        batch: i64,
        /// 盖住它的那一批。
        by: i64,
        /// 被盖住了几条。
        rows: u64,
    },
}

/// **待确认队列**里的一条。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// 变体本身。
    pub variant: VariantRow,
    /// 这一轮识别的结论。
    pub state: State,
    /// 「无判据」与「跳过」的具体理由。**这是队列里最该先读的一行**——它说的是
    /// 「为什么没定下来」，而那往往就决定了该怎么裁。
    pub reason: Option<String>,
    /// 全部**候选**，各自带**依据**。可能一条都没有。
    pub candidates: Vec<Candidate>,
    /// 拿得到的**内容判据**；`None` 表示这一条只钉得住本机的路径。
    ///
    /// 要 [`fill_prints`] 跑过才有值——那一步要为每个变体查两次中立库，
    /// 一万多条的队列不该在只是列一眼的时候整份算出来。
    pub print: Option<ContentPrint>,
}

impl Item {
    /// 从库里那一行折出一条队列条目。**判据那一栏留空**，要它得再跑一趟 [`fill_prints`]。
    ///
    /// 收在一处而不是两处各写一遍：队列与「忘掉裁决」走的是同一张表、同一个形状，
    /// 各写一遍的话「候选什么时候读」这类差别会悄悄漂开。
    fn of(row: QueueRow, candidates: Vec<Candidate>) -> Self {
        Self {
            variant: row.variant,
            state: row.state,
            reason: row.reason,
            candidates,
            print: None,
        }
    }

    /// 变体的文件名，**按命名规律**筛的就是它。
    #[must_use]
    pub fn name(&self) -> &str {
        file_name_of_key(&self.variant.key)
    }

    /// 变体所在的目录（键里最后一个 `/` 之前的部分）；顶层是空串。
    #[must_use]
    pub fn directory(&self) -> &str {
        match self.variant.key.rfind('/') {
            Some(at) => &self.variant.key[..at],
            None => "",
        }
    }

    /// 这一条的裁决会钉在什么上。
    #[must_use]
    pub fn anchor(&self, library: &str) -> Anchor {
        match &self.print {
            Some(print) => Anchor::Content {
                crc32: print.crc32,
                size: print.size,
                sha1: None,
            },
            None => Anchor::Path {
                library: library.to_string(),
                variant_key: self.variant.key.clone(),
            },
        }
    }

    /// 这一条的裁决钉在哪一份内容上：`(成员的键, 容器内部路径)`。
    ///
    /// 拿不到内容判据时退回**主文件**——那时锚是路径锚，这两样只用来在投影出来的那条
    /// 候选上说清「是包里的哪一个」。**落下与重做共用它**：两处各写一遍的话，重做出来的
    /// 那条候选会指向另一份内容，而那正是「重放一遍结果一模一样」这句话的反面。
    #[must_use]
    pub fn representative(&self) -> (String, String) {
        match &self.print {
            Some(print) => (print.member.clone(), print.inner.clone()),
            None => (self.variant.main_key.clone(), String::new()),
        }
    }

    /// 这一条标成**置信度四档**里的哪一档（ADR-0002）。
    ///
    /// 看的是**第一条候选**——与 [`Shape::of`](batch::Shape::of) 同一条规则，理由也同一个：
    /// 「整批通过」就是采用第一条（`triage::batch` 的模块文档）。两处各写一遍的话，
    /// 屏上那条色条与按下去做的事迟早会指着不同的候选。
    #[must_use]
    pub fn tier(&self) -> Tier {
        Tier::of(self.candidates.first().map(|lead| lead.confidence))
    }

    /// 这一条的名字里带着哪些**记号**——`[…]` 与 `(…)` 括起来的那几段。
    ///
    /// **按命名规律**这个轴要它才用得起来：汉化组、版本、语言几乎总是写在方括号里
    /// （`[ACG汉化组]`、`[T+Chi]`、`(简)`），而人在下 `--name` 之前得先看得见有哪些。
    /// 报告把它们连条数一起印出来，照着抄一个就是一条覆盖几百条的命令。
    #[must_use]
    pub fn name_marks(&self) -> Vec<String> {
        let mut marks = Vec::new();
        for (open, close) in [('[', ']'), ('(', ')'), ('【', '】'), ('（', '）')] {
            let mut rest = self.name();
            while let Some(start) = rest.find(open) {
                let after = &rest[start + open.len_utf8()..];
                let Some(end) = after.find(close) else { break };
                let mark = after[..end].trim();
                // 一个字的记号（`(J)`、`(简)`）与空记号都不成其为「规律」——
                // 它们选出来的是半个库，不是一批。
                if mark.chars().count() >= 2 {
                    marks.push(mark.to_string());
                }
                rest = &after[end + close.len_utf8()..];
            }
        }
        marks.sort();
        marks.dedup();
        marks
    }

    /// 这一条的**候选作品**都有哪些——候选的条目名剥掉标记组之后的正题。
    #[must_use]
    pub fn candidate_works(&self) -> Vec<String> {
        let mut works: Vec<String> = self
            .candidates
            .iter()
            .map(|candidate| identify::naming::work_title(&candidate.game))
            .collect();
        works.sort();
        works.dedup();
        works
    }
}

/// 从队列里挑哪些。**几个条件之间是交集**，同一个条件里给了几个值是并集。
///
/// 全空表示「整个队列」。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// **按目录**：变体的键落在这个前缀下。`库/FC` 与 `库/FC/` 一个意思。
    ///
    /// 键的第一段是**根名**（`path::library_key`），所以前缀也带着它——
    /// 顺带白拿一条：`--under <根名>` 就是「整个这个根」。
    pub under: Vec<String>,
    /// 按平台。
    pub platform: Vec<String>,
    /// 按识别结论。空表示默认那三档（命中但没通过 / 未命中 / 无判据），**不含跳过**。
    pub states: Vec<State>,
    /// **按命名规律**：变体的文件名里含有这段文字（不分大小写）。
    pub name_contains: Vec<String>,
    /// **按候选作品**：它的候选里有一条指着这部作品。
    pub candidate_work: Vec<String>,
    /// **按依据形状**：它落在这一批里。**一级分批点一下就是它**（[`batch::batches`]）。
    ///
    /// 与另外三个轴同一个身份：屏上那张卡片写着「3,053 条」，照着折出来的选择器就该
    /// 选中同样 3,053 条。差一条，人按下去的那一下就不是他看过的那一批。
    pub shape: Vec<Shape>,
    /// 点名这几个变体。
    pub keys: Vec<String>,
}

impl Filter {
    /// 一个条件都没给吗。**整批裁决之前要看这个**：空过滤器选中的是整个队列。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.under.is_empty()
            && self.platform.is_empty()
            && self.states.is_empty()
            && self.name_contains.is_empty()
            && self.candidate_work.is_empty()
            && self.shape.is_empty()
            && self.keys.is_empty()
    }

    /// 这一档**识别结论**要不要。空表示默认那三档，**不含跳过**。
    ///
    /// 与 [`Self::keeps`] 分开，是因为它在 [`Item`] 折出来**之前**就用得上：折一条队列
    /// 条目要读它的候选，而跳过的那些本来就不进队列，先筛掉能省下几千次查询。
    #[must_use]
    pub fn keeps_state(&self, state: State) -> bool {
        if self.states.is_empty() {
            state != State::Skipped
        } else {
            self.states.contains(&state)
        }
    }

    /// 这一条留不留——**结论那一档也算在内**。
    ///
    /// [`Queue`] 就地重筛靠它：队列列一次之后，换选择器不该再读一遍中立库。
    #[must_use]
    pub fn keeps_item(&self, item: &Item) -> bool {
        self.keeps_state(item.state) && self.keeps(item)
    }

    /// 这一条留不留，**不看识别结论**。
    ///
    /// 「忘掉裁决」走的是这一条：裁决过的变体已经退出队列，它们的结论是什么样都得选得到
    /// （确认没有发行版的那些眼下正投影成**跳过**，拿默认三档去选一条也选不着）。
    #[must_use]
    pub fn keeps(&self, item: &Item) -> bool {
        if !self.keys.is_empty() && !self.keys.contains(&item.variant.key) {
            return false;
        }
        if !self.under.is_empty()
            && !self
                .under
                .iter()
                .any(|prefix| under(&item.variant.key, prefix))
        {
            return false;
        }
        if !self.platform.is_empty() {
            let platform = item.variant.platform.as_deref().unwrap_or("");
            if !self
                .platform
                .iter()
                .any(|wanted| wanted.eq_ignore_ascii_case(platform))
            {
                return false;
            }
        }
        if !self.name_contains.is_empty() {
            let name = fold(item.name());
            if !self
                .name_contains
                .iter()
                .any(|needle| name.contains(&fold(needle)))
            {
                return false;
            }
        }
        if !self.candidate_work.is_empty() {
            let works: Vec<String> = item.candidate_works().iter().map(|w| fold(w)).collect();
            if !self
                .candidate_work
                .iter()
                .any(|wanted| works.contains(&fold(wanted)))
            {
                return false;
            }
        }
        // **形状那一条不分配**（[`Shape::holds`]）：一万八千条每帧都要过一遍。
        if !self.shape.is_empty() && !self.shape.iter().any(|shape| shape.holds(item)) {
            return false;
        }
        true
    }
}

/// 这个键落在那个目录前缀下吗。
///
/// `FC` 选中 `FC/游戏.zip`，但**不选中** `FCX/游戏.zip`——差一个字符就是另一个平台，
/// 而这条命令后面跟着的是「照这个改几百条」。
///
/// 空前缀是**最顶上那一层**，不是「全部」：[`Item::directory`] 给顶层的条目交出的正是
/// 空串，而「按目录」那张表上的每一行都得能原样折回一个选择器（[`Axis::filter`]）。
/// 主库变成一组根之后每条键都至少有一段根名，于是这一支实际上走不到了，留着是为了让
/// 「每一行都折得回一个选择器」这条不变量不依赖键的形状。
/// 要整个队列不写 `--under`，那是 `under` 这个字段整个为空的意思。
fn under(key: &str, prefix: &str) -> bool {
    let prefix = prefix.trim_end_matches('/');
    if prefix.is_empty() {
        return !key.contains('/');
    }
    key.strip_prefix(prefix)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// **批量裁决的一个轴**：队列按什么分组，以及点中一组之后选择器长什么样。
///
/// 两件事收在同一个类型里而不是各写一处，因为报告与界面上那句话是
/// 「一条 `--under gba/【全部汉化】` 覆盖 **852** 条」——它只在选择器真的选出同样
/// 852 条时才算数。分组按 [`Item::directory`]、选中按 [`Filter::under`]，两边各写一遍
/// 的话，报告说的数与命令跑出来的数迟早对不上，而用户是照着那个数按下去的。
///
/// ADR-0002 点名的正是这三个轴：**按目录**、**按候选作品**、**按汉化组命名规律**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Axis {
    /// **按目录**：一个平台目录（或它下面某一层）里的全部待裁决变体。
    Directory,
    /// **按候选作品**：候选指着同一部作品的那些。
    CandidateWork,
    /// **按命名规律**：名字里带同一个记号的那些——汉化组几乎总是写在方括号里。
    NameMark,
}

impl Axis {
    /// 三个轴，报告与界面照这个次序摆。
    pub const ALL: [Self; 3] = [Self::Directory, Self::CandidateWork, Self::NameMark];

    /// 它在 [`Axis::ALL`] 里排第几。**穷尽匹配**而不是去表里找一遍：找得到与找不到
    /// 两条路里，后一条根本不存在，写出来只会多一个悄悄退回第一个轴的分支。
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Self::Directory => 0,
            Self::CandidateWork => 1,
            Self::NameMark => 2,
        }
    }

    /// 这个轴在界面上叫什么。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Directory => "按目录",
            Self::CandidateWork => "按候选作品",
            Self::NameMark => "按命名规律",
        }
    }

    /// 这个轴筛的是什么，一句人话。界面上是输入框的提示。
    #[must_use]
    pub fn hint(self) -> &'static str {
        match self {
            Self::Directory => "这个目录下的",
            Self::CandidateWork => "候选指着这部作品的",
            Self::NameMark => "名字里含这段文字的",
        }
    }

    /// 命令行上对应的那个开关。报告把它印在表头上——照着抄一行就是一条覆盖几百条的命令。
    #[must_use]
    pub fn selector(self) -> &'static str {
        match self {
            Self::Directory => "--under",
            Self::CandidateWork => "--candidate-work",
            Self::NameMark => "--name",
        }
    }

    /// 这一条落在这个轴的哪几组里。
    ///
    /// 可以落进不止一组：一个名字里能有好几个记号，一个变体也能有好几条候选。
    /// 也可以一组都不落——队列里绝大多数条目一条候选都没有。
    #[must_use]
    pub fn keys_of(self, item: &Item) -> Vec<String> {
        match self {
            Self::Directory => vec![item.directory().to_string()],
            Self::CandidateWork => item.candidate_works(),
            Self::NameMark => item.name_marks(),
        }
    }

    /// 点中这一组之后，选择器长什么样。
    #[must_use]
    pub fn filter(self, label: &str) -> Filter {
        let value = vec![label.to_string()];
        match self {
            Self::Directory => Filter {
                under: value,
                ..Filter::default()
            },
            Self::CandidateWork => Filter {
                candidate_work: value,
                ..Filter::default()
            },
            Self::NameMark => Filter {
                name_contains: value,
                ..Filter::default()
            },
        }
    }
}

/// 一行分组计数：这一组叫什么、带几条。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct GroupRow {
    /// 这一组叫什么。**它同时是选择器的值**——[`Axis::filter`] 拿它折出选择器。
    pub label: String,
    /// 多少条。
    pub count: u64,
}

/// 把这些条目按某个轴数一遍：**每一行就是一次批量裁决能覆盖多少**。
///
/// 报告与界面用的是同一个函数。多的排前面（人要先看见最值钱的那一批），
/// 同数按名字定死顺序（同一份库跑两次，表得长得一模一样）。
#[must_use]
pub fn tally(items: &[Item], axis: Axis) -> Vec<GroupRow> {
    tally_by(items, |item| axis.keys_of(item))
}

/// 按任意一把钥匙数一遍。报告拿它数「按平台」「为什么没定下来」这类不成其为轴的分组。
pub(crate) fn tally_by(items: &[Item], keys: impl Fn(&Item) -> Vec<String>) -> Vec<GroupRow> {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for item in items {
        for key in keys(item) {
            *counts.entry(key).or_default() += 1;
        }
    }
    let mut rows: Vec<GroupRow> = counts
        .into_iter()
        .map(|(label, count)| GroupRow { label, count })
        .collect();
    rows.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.cmp(&b.label)));
    rows
}

/// 走一趟**待确认队列**的结果。
///
/// 三样一起给，是因为**它们出自同一趟扫描**：中立库里那 46,444 行只该读一遍。
/// 分成三个入口的话，命令行为了印一行「队列 N 条、选中 M 条」就要读三遍。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Survey {
    /// 跑过识别没有。没跑过时队列是空的，**但那不是「没什么可裁的」**——
    /// 该说的是「先跑一次 `romcat identify`」。
    pub identified: bool,
    /// 整个队列有多少条（不过任何选择器）。
    pub queue: u64,
    /// 选择器选中的那些。
    pub items: Vec<Item>,
}

/// 折出**待确认队列**。
///
/// **一个字节都不读主库**：全部原料在中立库与沉淀库里（ADR-0001）。
/// [`Item::print`] 那一栏留空，要它得再跑一趟 [`fill_prints`]。
///
/// # Errors
/// 读中立库失败时返回错误。
pub fn survey(
    catalog: &Catalog,
    verdicts: &verdict::Index,
    filter: &Filter,
) -> Result<Survey, TriageError> {
    let rows = catalog.queue_rows()?;
    let mut out = Survey {
        identified: !rows.is_empty(),
        ..Survey::default()
    };
    let whole = Filter::default();
    for row in rows {
        // 整个队列有多少条，与选择器无关——报告要拿这两个数并排放，
        // 不然用户看不出自己的选择器是选窄了还是库里本来就只有这些。
        if in_queue(&row, &whole) && !decided(verdicts, &row) {
            out.queue += 1;
        }
        if !in_queue(&row, filter) || decided(verdicts, &row) {
            continue;
        }
        // 候选按需读——真库里那是 150,959 行，而队列里绝大多数条目一条都没有。
        let candidates = if row.candidates > 0 {
            catalog.candidates_of(&row.variant.key)?
        } else {
            Vec::new()
        };
        let item = Item::of(row, candidates);
        if filter.keeps(&item) {
            out.items.push(item);
        }
    }
    Ok(out)
}

/// 这个变体在队列里吗（还没过过滤器）。
fn in_queue(row: &QueueRow, filter: &Filter) -> bool {
    row.accepted == 0 && filter.keeps_state(row.state)
}

/// 沉淀库对这个变体说过话没有。
///
/// **不在这里算内容判据**：那要为每个变体查两次中立库，一万多条的队列上就是几万次
/// 查询，而队列每列一次都要付这笔钱。判据从**识别留下的痕迹**上读：
///
/// - 定成发行版的，被投影成一条**自动通过**的候选——[`in_queue`] 已经用 `accepted`
///   把它们挡在外面了。
/// - 确认没有发行版的，被投影成**跳过**——默认那三档里没有它。
/// - 「都不对而且认不出」的，结论没变，但理由那一列上盖着
///   [`identify::VERDICT_UNKNOWN_REASON`] 那一句。
///
/// 只剩路径锚那一种要真的查一下沉淀库，而那是一次内存里的查表。
fn decided(verdicts: &verdict::Index, row: &QueueRow) -> bool {
    row.reason.as_deref() == Some(identify::VERDICT_UNKNOWN_REASON)
        || verdicts.by_path(&row.variant.key).is_some()
}

/// 把每条的**内容判据**算出来。**一个字节都不读主库。**
///
/// # Errors
/// 读中立库失败时返回错误。
pub fn fill_prints(catalog: &Catalog, items: &mut [Item]) -> Result<(), TriageError> {
    for item in items.iter_mut() {
        item.print = identify::content_print(catalog, &item.variant)?;
    }
    Ok(())
}

/// 这一批要下什么裁决。
///
/// 前两支只定**从哪儿起头**——采用一条候选，还是从零说起；其余那几样事实由
/// [`Overrides`] 一并盖上去。分成两半是因为它们本来就是两件事：`--pick 1 --team 外星科技`
/// 说的是「就是这条候选，另外汉化组是外星科技」，而把汉化组塞进「哪一条候选」里说不通。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionSpec {
    /// 采用第几条**候选**（从 1 数起）。候选不够多的那些会被挡下并说清。
    Pick(usize),
    /// **手工指定**这部作品。候选集不构成天花板——一条候选都没有时走的就是这条。
    Manual(String),
    /// 确认它**没有发行版**：同人移植与 homebrew。
    NoRelease {
        /// 挂在哪个作品下；认不出就留空。
        work: Option<String>,
    },
    /// 都不对，而且认不出是什么。记下来别再问第二遍。
    Unknown,
}

impl DecisionSpec {
    /// 「裁成什么」这句话。计划书、批的摘要、界面上那一行**共用它**。
    ///
    /// 收在核心库里而不是各处各写一句：**批的摘要要与当初计划书上那句话对得上**，
    /// 不然半年后按编号撤销的人看着摘要，认不出它就是自己当初看过并点头的那一批。
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Pick(nth) => format!("采用各自的第 {nth} 条候选"),
            Self::Manual(work) => format!("作品《{work}》"),
            Self::NoRelease { work } => format!(
                "确认没有发行版{}",
                work.as_deref()
                    .map(|work| format!("，挂在作品《{work}》下"))
                    .unwrap_or_default()
            ),
            Self::Unknown => "都不对，而且认不出是什么".to_string(),
        }
    }
}

/// 人补上去的那几样事实。给了就**盖过**从候选里读出来的那一份。
///
/// **汉化组**与**版本**永远只能从这里来：自动识别只保证做到发行版级，
/// 「这是谁汉化的第几版」本来就只有人说得出（ADR-0008）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overrides {
    /// 平台。不给就用变体自己的那个（目录是强先验，ADR-0011）。
    pub platform: Option<String>,
    /// 地区。
    pub region: Option<String>,
    /// 序列号。
    pub serial: Option<String>,
    /// 语言标记组。
    pub languages: Option<String>,
    /// 中文身份：**汉化版**还是**官中版**（ADR-0012）。
    pub chinese: Option<ChineseMark>,
    /// **汉化组**。
    pub team: Option<String>,
    /// 版本。
    pub version: Option<String>,
}

impl Overrides {
    /// 一样都没给吗。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// 盖到一份事实上。
    fn apply_to(&self, facts: &mut Facts) {
        for (slot, value) in [
            (&mut facts.platform, &self.platform),
            (&mut facts.region, &self.region),
            (&mut facts.serial, &self.serial),
            (&mut facts.languages, &self.languages),
            (&mut facts.team, &self.team),
            (&mut facts.version, &self.version),
        ] {
            if value.is_some() {
                slot.clone_from(value);
            }
        }
        if self.chinese.is_some() {
            facts.chinese = self.chinese;
        }
    }
}

/// 一次批量裁决说的全部话。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decide {
    /// 裁成什么。
    pub spec: DecisionSpec,
    /// 人补上去的那几样事实。
    pub overrides: Overrides,
    /// 记一句为什么。
    pub note: Option<String>,
    /// 路径锚要记是哪一份主库。
    pub library: String,
}

impl Decide {
    /// 这一批**裁成什么**，写成一句给人看的话。落进 [`verdict::Batch::summary`]。
    #[must_use]
    pub fn summary(&self) -> String {
        let mut text = self.spec.describe();
        for (label, value) in [
            ("平台", &self.overrides.platform),
            ("地区", &self.overrides.region),
            ("序列号", &self.overrides.serial),
            ("语言", &self.overrides.languages),
            ("汉化组", &self.overrides.team),
            ("版本", &self.overrides.version),
        ] {
            if let Some(value) = value {
                text.push_str(&format!("；{label} {value}"));
            }
        }
        if let Some(mark) = self.overrides.chinese {
            text.push_str(&format!("；中文 {}", mark.label()));
        }
        text
    }
}

/// 一次裁决**还在起草**的样子：四种说法挑一种，外加人补的那几样事实。
///
/// 命令行上是四个开关，界面上是四个单选钮——**说的是同一件事，判据只该有一份**。
/// 「一次只说一种裁决」与「说它不成其为一次发行就没有汉化组可记」这两句写两遍，
/// 两处迟早会漂开，而漂开的样子是：界面上收下了汉化组，库里一个字都没记。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Draft {
    /// 采用第几条**候选**（从 1 数起）。
    pub pick: Option<usize>,
    /// **手工指定**这部作品；配 `no_release` 时是「挂在哪个作品下」。
    pub work: Option<String>,
    /// 判它**没有发行版**。
    pub no_release: bool,
    /// 判「都不对，而且认不出」。
    pub unknown: bool,
    /// 人补上去的那几样事实。
    pub overrides: Overrides,
    /// 记一句为什么。
    pub note: Option<String>,
}

impl Draft {
    /// 折成一次批量裁决；说不成立的话**当场说不成立**。
    ///
    /// # Errors
    /// 一次说了不止一种裁决、一种都没说、序号从 0 数起，或者给「不成其为一次发行」
    /// 的那两档配了事实，都返回一句给人看的话。
    pub fn build(&self, library: &str) -> Result<Decide, String> {
        Ok(Decide {
            spec: self.spec()?,
            overrides: self.overrides.clone(),
            note: self.note.clone(),
            library: library.to_string(),
        })
    }

    /// 这份草稿说得成立吗。
    ///
    /// **界面拿它决定「落下」那个按钮亮不亮**，并把话原样显示出来；命令行拿它在开库
    /// 之前就把不成立的话说清。
    ///
    /// # Errors
    /// 与 [`Self::build`] 同。
    pub fn check(&self) -> Result<(), String> {
        self.spec().map(|_| ())
    }

    /// 这一次要下的是哪一种裁决。**四选一**，给多了就说清而不是挑一个。
    fn spec(&self) -> Result<DecisionSpec, String> {
        let given = [
            self.unknown,
            self.no_release,
            self.pick.is_some(),
            self.work.is_some() && !self.no_release,
        ];
        if given.iter().filter(|on| **on).count() > 1 {
            return Err(
                "一次只说一种裁决：采用候选、手工指定作品、没有发行版、认不出，挑一个。"
                    .to_string(),
            );
        }
        // **说不成立的话就当场说不成立，绝不静默丢掉。** 「没有发行版」与「认不出」说的是
        // 「它不成其为一次发行」，而汉化组、版本、地区那几样说的是「这次发行是什么样」
        // ——两句话不能同时说。收下再默默扔掉的话，计划书上印着「汉化组 外星科技」，
        // 库里却一个字都没记。
        if (self.no_release || self.unknown) && !self.overrides.is_empty() {
            // 这句话命令行与界面共用，所以**不提开关名**——界面上没有开关。
            return Err(format!(
                "「{}」说的是「它不成其为一次发行」，那就没有平台、地区、汉化组、版本可记。\n\
                 那几样留空，或者改成手工指定它是哪次发行。",
                if self.unknown {
                    "认不出"
                } else {
                    "没有发行版"
                }
            ));
        }
        if self.unknown {
            return Ok(DecisionSpec::Unknown);
        }
        if self.no_release {
            return Ok(DecisionSpec::NoRelease {
                work: self.work.clone(),
            });
        }
        if let Some(nth) = self.pick {
            if nth == 0 {
                return Err("候选的序号从 1 数起。".to_string());
            }
            return Ok(DecisionSpec::Pick(nth));
        }
        let Some(work) = self.work.clone() else {
            return Err(
                "没说要裁成什么：采用一条候选、手工指定作品、判它没有发行版、\n\
                 判「都不对而且认不出」，四选一。"
                    .to_string(),
            );
        };
        Ok(DecisionSpec::Manual(work))
    }
}

/// 一条要落下的裁决。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decided {
    /// 哪个变体。
    pub key: String,
    /// 要写进沉淀库的那一条。
    pub verdict: Verdict,
    /// 它盖掉了沉淀库里已有的一条吗。
    pub replaces: bool,
}

/// 一条落不下去的裁决，连原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blocked {
    /// 哪个变体。
    pub key: String,
    /// 为什么落不下去。
    pub why: String,
}

/// 一次批量裁决**将要**做什么。
///
/// **先出计划再动手**，与同步那一侧的差量预览同源（ADR-0016）：一条命令改几百条记录，
/// 看不见它要改什么就按下去，错了没处找。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// 要落下的。
    pub decided: Vec<Decided>,
    /// 落不下去的，连原因。
    pub blocked: Vec<Blocked>,
    /// **裁成什么**，写成一句给人看的话（[`Decide::summary`]）。
    ///
    /// 它跟着计划走到 [`apply`]，落成那一**批**的摘要。计划书上印的与半年后按编号撤销时
    /// 看见的因此是同一句话。
    pub summary: String,
    /// 记的那一句为什么，跟着计划落进批里。
    pub note: Option<String>,
    /// 这一批落在哪份主库上（路径锚里记的那个名字）。
    pub library: String,
    /// 这份计划是对着**哪一批条目**排的：它们的变体键，落得下的与落不下的都在里面。
    ///
    /// **一份计划的身份就是那一批条目。** 计划书上印的每一行说的都是排它那一刻队列里
    /// 的那一条，[`apply`] 因此拿它对一次账：手上这一批凑不齐就整份拒掉，理由与出口
    /// 写在 [`apply`] 上。
    pub against: BTreeSet<String>,
}

impl Plan {
    /// 钉在内容上的有几条——**可分享的就是这些**。
    #[must_use]
    pub fn content_anchored(&self) -> usize {
        self.decided
            .iter()
            .filter(|row| row.verdict.anchor.is_shareable())
            .count()
    }

    /// 只钉得住本机路径的有几条。
    #[must_use]
    pub fn path_anchored(&self) -> usize {
        self.decided.len() - self.content_anchored()
    }

    /// 会盖掉已有裁决的有几条。
    #[must_use]
    pub fn replacing(&self) -> usize {
        self.decided.iter().filter(|row| row.replaces).count()
    }
}

/// 排一次批量裁决的计划。**不写任何库。**
///
/// # Errors
/// 读沉淀库失败时返回错误。
pub fn plan(store: &Store, items: &[Item], decide: &Decide) -> Result<Plan, TriageError> {
    plan_each(store, items, decide)
}

/// 与 [`plan`] 同一件事，只是这些条目不必躺在一段连续的切片里。
///
/// **整批操作走的是它**：一级分批的一批是队列里**散落**的那些条，把它们拷成一段切片
/// 只为了排一次计划，在一万八千条的队列上就是白拷一遍。
///
/// # Errors
/// 读沉淀库失败时返回错误。
pub fn plan_each<'a>(
    store: &Store,
    items: impl IntoIterator<Item = &'a Item>,
    decide: &Decide,
) -> Result<Plan, TriageError> {
    let mut plan = Plan {
        summary: decide.summary(),
        note: decide.note.clone(),
        library: decide.library.clone(),
        ..Plan::default()
    };
    // **一批是一次落下，也就是一个时刻**，所以时刻只取这一次。逐条各取一次的话，
    // 一批三千条会跨过秒界，而同一条**内容锚**上的几份**重复拷贝**本该落成同一条裁决
    // ——差一秒就成了两条，[`undo_batch`] 再也认不出哪一条是这一批自己落下的。
    let decided_at = crate::catalog::now_secs();
    for item in items {
        // **落得下的与落不下的都算数**：计划书上那两段说的是同一批条目，
        // 少了哪一段它描述的都不再是排它时的那一批。
        plan.against.insert(item.variant.key.clone());
        let decision = match resolve(item, decide) {
            Ok(decision) => decision,
            Err(why) => {
                plan.blocked.push(Blocked {
                    key: item.variant.key.clone(),
                    why,
                });
                continue;
            }
        };
        let anchor = item.anchor(&decide.library);
        let replaces = store.find(&anchor)?.is_some();
        plan.decided.push(Decided {
            key: item.variant.key.clone(),
            verdict: Verdict::at(anchor, decision, decided_at).with_note(decide.note.clone()),
            replaces,
        });
    }
    Ok(plan)
}

/// 这一条具体裁成什么。
fn resolve(item: &Item, decide: &Decide) -> Result<Decision, String> {
    let mut facts = match &decide.spec {
        DecisionSpec::NoRelease { work } => {
            return Ok(Decision::NoRelease { work: work.clone() });
        }
        DecisionSpec::Unknown => return Ok(Decision::Unknown),
        DecisionSpec::Manual(work) => Facts {
            work: work.clone(),
            ..Facts::default()
        },
        DecisionSpec::Pick(nth) => {
            let candidate = item.candidates.get(nth.saturating_sub(1)).ok_or_else(|| {
                format!(
                    "只有 {} 条候选，挑不出第 {nth} 条——先看清这一条有哪些候选",
                    item.candidates.len()
                )
            })?;
            let parsed = identify::naming::parse(&candidate.game, None);
            Facts {
                work: parsed.work,
                platform: Some(candidate.platform.clone()),
                region: parsed.region,
                serial: candidate.serial.clone(),
                languages: parsed.languages,
                chinese: candidate.chinese,
                team: None,
                version: None,
            }
        }
    };
    if facts.work.trim().is_empty() {
        return Err("裁决要说得出是哪部**作品**，不然它什么也没定下来".to_string());
    }
    decide.overrides.apply_to(&mut facts);
    if facts.platform.is_none() {
        // 平台由目录给出，是强先验（ADR-0011）。人没另说时它就是对的那一个。
        facts.platform = item.variant.platform.clone();
    }
    Ok(Decision::Release(facts))
}

/// 一次批量裁决落下之后的账。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct Applied {
    /// 一共落了几条。
    pub verdicts: u64,
    /// 其中是新增的。
    pub added: u64,
    /// 其中盖掉了已有的。
    pub replaced: u64,
    /// 钉在内容上的（可分享）。
    pub content_anchored: u64,
    /// 只钉得住本机路径的。
    pub path_anchored: u64,
    /// 立刻变成**命中**的变体数。
    pub matched: u64,
    /// 立刻变成**跳过**的变体数（确认没有发行版的那些）。
    pub skipped: u64,
    /// 这一趟落成了第几**批**。**撤销点名的就是它**（[`undo_batch`]）。
    pub batch: i64,
}

/// 把计划真正落下：写沉淀库，并**立刻**在中立库里兑现。
///
/// 两处一起写，是因为它们回答的是两个问题：沉淀库回答「世上这份内容是什么」（换机、
/// 重装都带着走），中立库回答「这个库现在长什么样」（导出、子库、报告马上就要读它）。
/// 只写前者的话，用户裁完一批得先跑一趟 `romcat identify` 才看得见结果。
///
/// 下一趟识别会把中立库这一半整批清掉再照沉淀库重放一遍，结果与这里写下的一模一样
/// （[`Projector`] 两处共用）。
///
/// ## 落下的同时记成一**批**
///
/// 一次 `apply` 就是一批（[`verdict::Batch`]），**撤销以它为粒度**（[`undo_batch`]）。
/// 记批要在动手之前，而且分两处记，各按各的身份：
///
/// - **沉淀库**记这一批落下的那些、以及每条**盖掉了什么**——被盖掉的那条不可再生，
///   除了那儿没有第二份。
/// - **中立库**记这几个变体眼下的结论与候选（[`Catalog::stash_conclusions`]）——
///   那是上一趟识别自己算出来的东西，可再生，跟着中立库活。
///
/// 顺序是**先记批、再落裁决**。反过来的话，中途出错会留下一批已经落库、却没有一处
/// 记着它们是哪一批的裁决——那时撤销从一开始就无从谈起。
///
/// ## 计划过期了整份拒掉，不落一半
///
/// 计划排完到落下之间队列可能已经变过样：逐条流里刚裁掉过其中一条、换过一套选择器、
/// 重列过一次。那时计划里那几行说的条目**手上这一批里根本没有**，而从前的做法是把
/// 它们略过去——沉淀库那一条已经落进去了，中立库这一半却不投影，同一条变体两边各说
/// 各的，要等下一趟识别才收得回来。
///
/// 三条路里取的是**整份拒掉**（[`TriageError::StalePlan`]）：
///
/// - **只落对得上的那部分**——就是上面那半吊子：人在计划书上点头的是 14 条，落下去
///   的是 13 条，批里却照旧记着 14 行。批量的胆量来自撤销可信，一份账目对不上的批
///   第一时间就把它废掉了。
/// - **自己去中立库把缺的补读出来再投影**——落下去的就不再是人看过的那一份
///   （ADR-0016：先出计划再动手）。更要命的是那几条往往**正因为刚被裁过**才不在手上
///   这一批里，补读回来等于拿一份过期的答案盖掉刚落下的新答案。
/// - **取的这条**：说清哪几条过期了，让人**重排一份计划**。两份库一个字都不动，
///   重排一次的代价是一次点击。
///
/// 这道门住在核心库而不是界面上（ADR-0005）：「一份计划还作不作数」是领域判断，
/// 命令行与日后别的壳照样要它。界面另有自己的一道门，那道门管的是**别让它发生**，
/// 这一道管的是**发生了也不会两边各说各的**。
///
/// # Errors
/// 计划过期、或者写中立库、沉淀库失败时返回错误。
pub fn apply(
    catalog: &mut Catalog,
    store: &mut Store,
    items: &[Item],
    plan: &Plan,
) -> Result<Applied, TriageError> {
    let by_key: BTreeMap<&str, &Item> = items
        .iter()
        .map(|item| (item.variant.key.as_str(), item))
        .collect();
    // **先对账、再动手。** 计划的身份是它对着哪一批条目排的（[`Plan::against`]），
    // 手上这一批凑不齐就一个字都不写——理由与另外两条路写在这个函数的文档里。
    let mut missing: BTreeSet<&str> = BTreeSet::new();
    let mut landing_rows: Vec<(&Decided, &Item)> = Vec::with_capacity(plan.decided.len());
    for key in plan.against.iter().map(String::as_str) {
        if !by_key.contains_key(key) {
            missing.insert(key);
        }
    }
    for row in &plan.decided {
        match by_key.get(row.key.as_str()) {
            Some(item) => landing_rows.push((row, item)),
            None => {
                missing.insert(row.key.as_str());
            }
        }
    }
    if !missing.is_empty() {
        return Err(stale_plan(&missing));
    }
    // **一条锚在一批里只有一条裁决。** 裁决的身份是**锚**、不是变体：同一条内容锚上的
    // 几份**重复拷贝**在计划里各占一行，落进沉淀库却只有一条——最后落下的那条。
    // 批里若按变体各记各的，撤销就认不出「锚上眼下这条是不是这一批自己落下的」；
    // 放回去时落下的也可能与当初那条不是同一个（批里那几行按变体的键排，与计划的顺序
    // 不是一回事）。所以先按锚归到最后落下的那一条上，两处一起用它。
    let mut landing: BTreeMap<&Anchor, &Verdict> = BTreeMap::new();
    for row in &plan.decided {
        landing.insert(&row.verdict.anchor, &row.verdict);
    }
    // **先把批记下来。** `before` 要在落下之前读——落完再读，读到的就是刚写进去的那条，
    // 而那正是撤销时要拿来还原的东西。
    let mut rows = Vec::with_capacity(landing_rows.len());
    for (row, item) in &landing_rows {
        let (member, inner) = item.representative();
        let verdict = landing
            .get(&row.verdict.anchor)
            .copied()
            .unwrap_or(&row.verdict);
        rows.push(verdict::BatchRow {
            variant_key: row.key.clone(),
            member,
            inner,
            after: verdict.clone(),
            before: store.find(&verdict.anchor)?,
        });
    }
    let batch = store.put_batch(&plan.library, &plan.summary, plan.note.as_deref(), &rows)?;
    let keys: Vec<&str> = rows.iter().map(|row| row.variant_key.as_str()).collect();
    catalog.stash_conclusions(batch, &keys)?;

    let mut account = Applied {
        batch,
        ..Applied::default()
    };
    let mut projector = Projector::new();
    // 结论攒一批写一次：`write_identifications` 一次一个事务，几百条各开一次
    // 是把一件批量的事做成几百件零碎的事。
    let mut records = Vec::new();
    for (row, item) in &landing_rows {
        let verdict = landing
            .get(&row.verdict.anchor)
            .copied()
            .unwrap_or(&row.verdict);
        if store.put(verdict)? {
            account.added += 1;
        } else {
            account.replaced += 1;
        }
        account.verdicts += 1;
        if verdict.anchor.is_shareable() {
            account.content_anchored += 1;
        } else {
            account.path_anchored += 1;
        }
        // 「认不出」不产生结论——它只是在理由那一列上盖一句，别的一个字不动。
        // 结论本身没变（照旧是未命中或无判据），变的只是「为什么还停在这儿」。
        if matches!(verdict.decision, Decision::Unknown) {
            catalog.set_identification_reason(
                &item.variant.key,
                Some(identify::VERDICT_UNKNOWN_REASON),
            )?;
            continue;
        }
        let (member, inner) = item.representative();
        let Some(record) = projector.project(catalog, &item.variant, verdict, &member, &inner)?
        else {
            continue;
        };
        match record.state {
            State::Matched => account.matched += 1,
            State::Skipped => account.skipped += 1,
            _ => {}
        }
        records.push(record);
    }
    catalog.write_identifications(&records)?;
    Ok(account)
}

/// 计划过期那句话。**说得出是哪几条**，人才知道队列在哪儿变过、该重排哪一批。
fn stale_plan(missing: &BTreeSet<&str>) -> TriageError {
    // 只点三条名：过期的可能是整整一批，把一万八千个键印在一句话里没人读得下去。
    let head = 3;
    let mut names: Vec<String> = missing.iter().take(head).map(|key| (*key).to_string()).collect();
    if missing.len() > head {
        names.push(format!("……还有 {} 条", missing.len() - head));
    }
    TriageError::StalePlan {
        missing: missing.len(),
        names: names.join("、"),
    }
}

/// 撤掉一**批**之后的账。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Undone {
    /// 撤的是第几批。
    pub batch: i64,
    /// 从沉淀库里删掉了几条。
    pub removed: u64,
    /// 其中把**它盖掉的那条旧裁决**放回去了几条。
    pub restored: u64,
    /// **没动**几条：同一条锚上后来有人在**批以外**重新裁过（`triage import` 收下的、
    /// 别人分享来的），那是别人的账。被后来那一**批**盖住的撤不动，压根走不到这儿
    /// （[`TriageError::CoveredBy`]）。
    pub kept: u64,
    /// 中立库里放回了几个变体的结论。
    pub variants: u64,
    /// 中立库那一半回滚得了吗。
    ///
    /// 为假就是这一批的快照已经随重跑识别清掉了（[`Catalog::clear_identifications`]），
    /// 那时只回滚得了沉淀库那一半——**该如实说出来**，而不是让人以为队列已经回来了。
    pub catalog_rolled_back: bool,
}

/// 这一批被后来、眼下还在册的一批盖住了吗——盖住了就撤不动、也放不回去。
///
/// **撤销与放回共用这一问**：两边认「这一批在这条锚上说了算吗」用的必须是同一条判据，
/// 不然一边拦下的另一边照旧做得成，而做成的那一下正是把账做乱的那一下。
///
/// 它问的是**册子**（[`Store::batch_covering`]）而不是「锚上眼下那条长什么样」：
/// 两批落下的裁决**值可以一模一样**（同一秒、同一部作品），按值比对认不出这件事。
fn covered_by(store: &Store, batch: i64, rows: &[verdict::BatchRow]) -> Result<(), TriageError> {
    let anchors: BTreeSet<Anchor> = rows.iter().map(|row| row.anchor().clone()).collect();
    if let Some((by, covered)) = store.batch_covering(batch, &anchors)? {
        return Err(TriageError::CoveredBy {
            batch,
            by,
            rows: covered,
        });
    }
    Ok(())
}

/// 撤掉一**批**：**沉淀库与中立库两边都回到这一批落下之前的样子**。
///
/// ## 它凭什么不算「当场手改投影」
///
/// 原挂账 D102 当初维持原样，理由是：中立库那一半是投影，重算是它唯一的权威来路，
/// 当场手改会让「重算一遍是什么样」与「眼下是什么样」有两个答案。
///
/// **这一条顾虑是对的，而这里正是按它办的。** 撤销放回去的不是现编的一份结论，是
/// [`Catalog::stash_conclusions`] 在这一批落下之前收起来的那一份——**上一趟识别自己
/// 算出来的东西**，一个字节都不是这里编的。放回去之后两个答案照旧是同一个。
///
/// 反过来说，D102 维持原样的那个形状才是两个答案真的分了家：裁决从沉淀库里删掉了，
/// 而它投影出来的那条「命中」还留在中立库里——那时「重算一遍」说的是「回队列」，
/// 「眼下」说的是「命中《某作品》」。
///
/// 还有一层：选项 B（`undo` 顺手重算一遍）要 **DAT 库在手边**，而 `undo` 不要求。
/// 这条路**一个字节的 DAT 都不要**——要放回去的东西早就在中立库里躺着了。
///
/// ## 只动这一批，而且**撤不干净就不撤**
///
/// 动手之前先问一句「这一批被后来、眼下还在册的哪一批盖住了没有」
/// （[`Store::batch_covering`]）。**批与批在同一条锚上是叠着的**：后一批记着的
/// 「它盖掉了什么」正是前一批落下的那条，撤后一批就会把它放回来。所以被盖住的那一批
/// 根本回不到「它落下之前」——硬撤的话它被标成已撤，而它的裁决过一会儿又活了。
/// 那时不撤，并说清是**哪一批**盖的，让人先撤那一批（[`TriageError::CoveredBy`]）。
///
/// 剩下那种「别人的账」不属于任何一批（`triage import` 收下的、别人分享来的），
/// 谁也不会再把这一批的那条放回来：那一条一个字不动、记进 [`Undone::kept`]，别的照撤
/// ——这一批落下的裁决一条都不在生效了，标成已撤是实话。
///
/// 落到每一条上的判据是「**锚上眼下这条还是这一批当初落下的那条吗**」。不是就一个字
/// 都不动，那一条的中立库结论也不碰——它眼下的样子是那条**新**裁决的投影，拿一份更老
/// 的快照盖上去才是真的改坏了。
///
/// # Errors
/// 没这一批、这一批已经撤过了、被后来的一批盖住了、或者读写两份库失败时返回错误。
pub fn undo_batch(
    catalog: &mut Catalog,
    store: &mut Store,
    batch: i64,
) -> Result<Undone, TriageError> {
    let found = store.batch(batch)?.ok_or(TriageError::NoBatch(batch))?;
    if found.undone() {
        return Err(TriageError::AlreadyUndone(batch));
    }
    let rows = store.batch_rows(batch)?;
    covered_by(store, batch, &rows)?;
    let mut account = Undone {
        batch,
        ..Undone::default()
    };
    let mut rolled_back: Vec<String> = Vec::new();
    // **重复拷贝是真机上的常态**：同一份内容躺着好几份，它们钉的是同一条**内容锚**，
    // 而一批里可能同时裁了好几份。第二份走到这儿时，锚上那条已经被第一份处理过了——
    // 那不是「别人重新裁过」，那就是我们自己刚留下的样子。分不清的话，第二份的中立库
    // 结论会留着一条指向已经不存在的裁决的「命中」，也就是票 08 要消掉的那个形状。
    //
    // 判据是「**锚上眼下这个样子是不是这一批自己刚留下的**」，不是「锚上是不是空的」：
    // 这一批盖掉过一条旧裁决时，第一份撤完锚上留下的是那条**旧的**，不是空的。
    // 几份拷贝在批里记的是**同一条** `after`（[`apply`] 按锚归过），所以整条相等这个
    // 判据认得出它们；各记各的话，只差一秒都会让先走到的那一份被当成别人的账。
    let mut mine: BTreeSet<Anchor> = BTreeSet::new();
    for row in rows {
        let current = store.find(row.anchor())?;
        if current.as_ref() == Some(&row.after) {
            store.remove(row.anchor())?;
            mine.insert(row.after.anchor.clone());
            account.removed += 1;
            if let Some(before) = &row.before {
                store.put(before)?;
                account.restored += 1;
            }
        } else if !(mine.contains(row.anchor()) && current == row.before) {
            account.kept += 1;
            continue;
        }
        rolled_back.push(row.variant_key);
    }
    account.catalog_rolled_back = catalog.stashed(batch)? > 0;
    if account.catalog_rolled_back {
        let keys: Vec<&str> = rolled_back.iter().map(String::as_str).collect();
        account.variants = catalog.restore_conclusions(batch, &keys)?;
    }
    store.mark_batch_undone(batch, true)?;
    Ok(account)
}

/// 把撤掉的那一**批**放回去。**撤销本身撤得回来，走的就是这条。**
///
/// 它不是「再裁一遍」——一个字都不必用户重打：这一批当初落下的每一条原样记在
/// [`verdict::BatchRow::after`] 里，放回去就是把它们重新写进沉淀库，再走
/// [`Projector`] 那条**与识别共用的**路投影回中立库。
///
/// 与 [`undo_batch`] 对称，它也只动这一批，而且**放不回去就不放**：先问同一句
/// 「被后来还在册的哪一批盖住了没有」（[`covered_by`]）——放回去要写的正是那条锚，
/// 硬写下去会把那一批的裁决顶掉，而顶掉了什么一处也没记。剩下每条再核对
/// 「这条锚上眼下还是撤销之后留下的那个样子吗」，不是就不动。
///
/// **中立库那一半的快照不重新收一遍。** 快照说的是「这一批第一次落下之前是什么样」，
/// 那句话不因为撤了又放回去而改变；重新收一遍反而会把撤销刚放回去的那一份当成
/// 「之前」，于是再撤一次就撤了个寂寞。
///
/// # Errors
/// 没这一批、这一批没撤过、被后来的一批盖住了、或者读写两份库失败时返回错误。
pub fn redo_batch(
    catalog: &mut Catalog,
    store: &mut Store,
    batch: i64,
) -> Result<Applied, TriageError> {
    let found = store.batch(batch)?.ok_or(TriageError::NoBatch(batch))?;
    if !found.undone() {
        return Err(TriageError::NotUndone(batch));
    }
    let rows = store.batch_rows(batch)?;
    covered_by(store, batch, &rows)?;
    let mut account = Applied {
        batch,
        ..Applied::default()
    };
    let mut projector = Projector::new();
    let mut records = Vec::new();
    // 与撤销那一侧对称：**重复拷贝是真机上的常态**，一批里可能有好几份同内容的拷贝。
    // 第二份走到这儿时锚上那条已经是这一批自己刚放回去的了——那不是「别人裁过」，
    // 不写第二遍，但它的中立库那一半照样要补上。
    let mut mine: BTreeSet<Anchor> = BTreeSet::new();
    for row in rows {
        let current = store.find(row.anchor())?;
        let already = mine.contains(row.anchor()) && current.as_ref() == Some(&row.after);
        if current != row.before && !already {
            continue;
        }
        if !already {
            if store.put(&row.after)? {
                account.added += 1;
            } else {
                account.replaced += 1;
            }
            mine.insert(row.after.anchor.clone());
            account.verdicts += 1;
            if row.after.anchor.is_shareable() {
                account.content_anchored += 1;
            } else {
                account.path_anchored += 1;
            }
        }
        let Some(variant) = catalog.variant(&row.variant_key)? else {
            continue;
        };
        if matches!(row.after.decision, Decision::Unknown) {
            catalog
                .set_identification_reason(&variant.key, Some(identify::VERDICT_UNKNOWN_REASON))?;
            continue;
        }
        let Some(record) =
            projector.project(catalog, &variant, &row.after, &row.member, &row.inner)?
        else {
            continue;
        };
        match record.state {
            State::Matched => account.matched += 1,
            State::Skipped => account.skipped += 1,
            _ => {}
        }
        records.push(record);
    }
    catalog.write_identifications(&records)?;
    store.mark_batch_undone(batch, false)?;
    Ok(account)
}

/// 一次「忘掉裁决」的账。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Forget {
    /// 要忘掉的：变体的键连它钉在什么上。
    pub rows: Vec<(String, Anchor)>,
}

/// 排一次「忘掉裁决」的计划：这些变体上眼下有裁决的都列出来。
///
/// 它走的是**全部变体**而不是队列——裁决过的变体本来就已经退出队列了，
/// 拿队列去选它们一条也选不到。
///
/// # Errors
/// 读中立库或沉淀库失败时返回错误。
pub fn plan_forget(
    catalog: &Catalog,
    store: &Store,
    filter: &Filter,
    library: &str,
) -> Result<Forget, TriageError> {
    let mut out = Forget::default();
    // 去重按**锚本身**，不按它印出来的那句话：同一份内容在库里存在多份拷贝时
    // （真机上重复拷贝是常态），它们钉的是同一条裁决，只该忘一次。
    let mut seen: BTreeSet<Anchor> = BTreeSet::new();
    for row in catalog.queue_rows()? {
        // 候选只在选择器真的按它筛时才读——真库里那是 150,959 行。
        let candidates = if row.candidates > 0 && !filter.candidate_work.is_empty() {
            catalog.candidates_of(&row.variant.key)?
        } else {
            Vec::new()
        };
        let mut item = Item::of(row, candidates);
        if !filter.keeps(&item) {
            continue;
        }
        item.print = identify::content_print(catalog, &item.variant)?;
        let anchor = item.anchor(library);
        if store.find(&anchor)?.is_some() && seen.insert(anchor.clone()) {
            out.rows.push((item.variant.key.clone(), anchor));
        }
    }
    Ok(out)
}

/// 真的忘掉。返回忘掉了几条。
///
/// **这一条只动沉淀库**，中立库那一半要等下一趟 `romcat identify` 重算才回到队列里。
///
/// 那不是遗留的将就，是这条路能给的全部：它按**选择器**选中的可以是任何一条裁决——
/// 别人分享来、`triage import` 收下的那些不属于本机任何一批，也就没有一份「落下之前
/// 是什么样」的快照可放回去。
///
/// **要两边一起回去，用 [`undo_batch`]**：本机自己裁下去的每一批都记着那份快照。
///
/// # Errors
/// 写沉淀库失败时返回错误。
pub fn forget(store: &mut Store, plan: &Forget) -> Result<u64, TriageError> {
    let mut gone = 0;
    for (_, anchor) in &plan.rows {
        if store.remove(anchor)? {
            gone += 1;
        }
    }
    Ok(gone)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 按目录差一个字符就是另一个平台() {
        assert!(under("库/FC/游戏.zip", "库/FC"));
        assert!(under("库/FC/游戏.zip", "库/FC/"));
        // 这条命令后面跟着的是「照这个改几百条」，`FC` 绝不能捎上 `FCX`。
        assert!(!under("库/FCX/游戏.zip", "库/FC"));
    }

    #[test]
    fn 根那一层写成斜杠还是空串都是同一批() {
        // 「按目录」那张表上主库根那一组的标签是**空串**，而界面上空框的意思是「不筛」，
        // 于是界面把它写成 `/`。两种写法必须选出同一批，否则表上写多少与点下去选中多少
        // 就对不上了——那正是 `Axis` 存在的理由。
        for key in ["顶层.zip", "FC/游戏.zip", "a/b/c.zip"] {
            assert_eq!(
                under(key, ""),
                under(key, "/"),
                "{key} 在两种写法下不是同一个答案",
            );
        }
        assert!(under("顶层.zip", "/"), "主库根那一层该选得中");
        assert!(!under("FC/游戏.zip", "/"), "根那一层不该把子目录也捎上");
    }
}
