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

pub mod queue;
pub mod report;

pub use queue::Queue;

use std::collections::{BTreeMap, BTreeSet};

use crate::catalog::identify::QueueRow;
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
    /// **按目录**：变体的键落在这个前缀下。`FC` 与 `FC/` 一个意思。
    pub under: Vec<String>,
    /// 按平台。
    pub platform: Vec<String>,
    /// 按识别结论。空表示默认那三档（命中但没通过 / 未命中 / 无判据），**不含跳过**。
    pub states: Vec<State>,
    /// **按命名规律**：变体的文件名里含有这段文字（不分大小写）。
    pub name_contains: Vec<String>,
    /// **按候选作品**：它的候选里有一条指着这部作品。
    pub candidate_work: Vec<String>,
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
        true
    }
}

/// 这个键落在那个目录前缀下吗。
///
/// `FC` 选中 `FC/游戏.zip`，但**不选中** `FCX/游戏.zip`——差一个字符就是另一个平台，
/// 而这条命令后面跟着的是「照这个改几百条」。
///
/// 空前缀是**主库根那一层**，不是「全部」：[`Item::directory`] 给顶层的文件交出的正是
/// 空串，而「按目录」那张表上的每一行都得能原样折回一个选择器（[`Axis::filter`]）。
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
    let mut plan = Plan::default();
    for item in items {
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
            verdict: Verdict::now(anchor, decision).with_note(decide.note.clone()),
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
/// # Errors
/// 写中立库或沉淀库失败时返回错误。
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
    let mut projector = Projector::new();
    let mut account = Applied::default();
    // 结论攒一批写一次：`write_identifications` 一次一个事务，几百条各开一次
    // 是把一件批量的事做成几百件零碎的事。
    let mut records = Vec::new();
    for row in &plan.decided {
        if store.put(&row.verdict)? {
            account.added += 1;
        } else {
            account.replaced += 1;
        }
        account.verdicts += 1;
        if row.verdict.anchor.is_shareable() {
            account.content_anchored += 1;
        } else {
            account.path_anchored += 1;
        }
        let Some(item) = by_key.get(row.key.as_str()) else {
            continue;
        };
        // 「认不出」不产生结论——它只是在理由那一列上盖一句，别的一个字不动。
        // 结论本身没变（照旧是未命中或无判据），变的只是「为什么还停在这儿」。
        if matches!(row.verdict.decision, Decision::Unknown) {
            catalog.set_identification_reason(
                &item.variant.key,
                Some(identify::VERDICT_UNKNOWN_REASON),
            )?;
            continue;
        }
        let (member, inner) = match &item.print {
            Some(print) => (print.member.clone(), print.inner.clone()),
            None => (item.variant.main_key.clone(), String::new()),
        };
        let Some(record) =
            projector.project(catalog, &item.variant, &row.verdict, &member, &inner)?
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
/// **中立库那一半不在这里回滚**：下一趟 `romcat identify` 会把结论整批重算，
/// 那时这几条自然回到队列里。当场改中立库的话，「重算一遍是什么样」与「眼下是什么样」
/// 就有两个答案了。
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
        assert!(under("FC/游戏.zip", "FC"));
        assert!(under("FC/游戏.zip", "FC/"));
        // 这条命令后面跟着的是「照这个改几百条」，`FC` 绝不能捎上 `FCX`。
        assert!(!under("FCX/游戏.zip", "FC"));
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
