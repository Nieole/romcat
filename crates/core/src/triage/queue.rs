//! 一次**待确认队列**的会话：列一次，之后就地重筛、分组、裁决。
//!
//! ## 为什么是「列一次」而不是每次都查库
//!
//! 命令行一条命令干一件事，折一次队列印一张表就结束了。界面不是——人在这里的动作是
//! **换个选择器再看一眼**，一分钟里能换十几次。每换一次都重跑一遍 [`survey`]，
//! 真机上是每次 1.3 秒的卡顿（46,444 行连表带候选走一遍），那样的队列没人用得下去。
//!
//! 所以队列在这里**整份留在内存里**，换选择器只是把 [`Filter::keeps_item`] 重跑一遍。
//! 这与「绝不把全库载入内存」（ADR-0005、`catalog::browse`）不矛盾，理由有两条：
//!
//! - **队列不是库。** 库是四万六千个变体、还要连上候选与标题；队列是其中拿不定主意的
//!   那一万六千条，一条 [`Item`] 就几百字节，整份不到十兆。
//! - **分组本来就要走完全部条目。** 「一条 `--name [ACG汉化组]` 覆盖 129 条」这句话
//!   ——ADR-0002 说它是队列成不成立的分界——只能由走一遍全部条目数出来。既然必须走完，
//!   留着就是白拿的。
//!
//! 真正下推给中立库的是**别的东西**：[`Item::print`] 那一栏（要为每个变体查两次库）
//! 只为**选中的**那些算，见 [`Queue::plan`]。
//!
//! ## 选中的那些排在前面
//!
//! 换选择器时做的是一次**稳定分区**：选中的挪到前面，键的次序不变。于是
//! [`Queue::selected`] 是一段真的切片，[`plan`] 与 [`apply`] 直接拿它，不必为了
//! 「凑出一段连续的条目」把几百条 [`Item`] 复制一遍。

use std::collections::BTreeSet;

use super::{
    Applied, Axis, Decide, Filter, GroupRow, Item, Plan, State, TriageError, apply, fill_prints,
    plan, survey, tally,
};
use crate::catalog::Catalog;
use crate::verdict::{self, Store};

/// 一次**待确认队列**的会话。
///
/// 它是**界面与命令行之间那条线的核心侧**：列队列、换选择器、按轴分组、排计划、落下、
/// 把落下的那些从队列里去掉——这一串没有一步是界面的事（ADR-0005：核心是独立的库）。
#[derive(Debug)]
pub struct Queue {
    /// 整个队列，**四档都在**。前 `taken` 条是选择器选中的。
    items: Vec<Item>,
    /// 选中了几条。
    taken: usize,
    /// 整个队列有多少条待裁决，按报告的口径（默认那三档，**不含跳过**）。
    pending: u64,
    /// 内存里那些**跳过**的有多少条。
    ///
    /// 它们不算在 `pending` 里（跳过不是「拿不定主意」），但**选中的条数里可能有它们**
    /// ——界面上勾一下就连跳过一起复核。两个数分开报，状态栏才说得出
    /// 「队列 16,656 条待裁决，另有 N 条跳过，选中 M 条」这种自洽的话。
    skipped: u64,
    /// 跑过识别没有。没跑过时队列是空的，**但那不是「没什么可裁的」**。
    identified: bool,
    /// 眼下的选择器。
    filter: Filter,
    /// 选中的那些按三个轴分出来的组，与 [`Axis::ALL`] 同序。
    groups: [Vec<GroupRow>; Axis::ALL.len()],
    /// **内容判据**已经算过的那些变体的键。
    ///
    /// 算不出来（无判据那一档）也记进来：那时 [`Item::print`] 照旧是 `None`，光看它
    /// 分不出「还没算」与「算过了、真没有」，而详情面板每帧都要问一次这条钉在什么上
    /// ——分不出的后果是每帧两次查库。
    printed: BTreeSet<String>,
}

impl Queue {
    /// 列一次队列。**一个字节都不读主库**（ADR-0001）。
    ///
    /// 四档一次全列进来，于是界面上勾一下「连跳过一起看」不必再读一遍库——**跳过**
    /// 默认不在队列里（它不是「拿不定主意」），但复核它是一等公民的动作。
    ///
    /// # Errors
    /// 读中立库失败时返回错误。
    pub fn load(catalog: &Catalog, verdicts: &verdict::Index) -> Result<Self, TriageError> {
        let everything = Filter {
            states: State::ALL.to_vec(),
            ..Filter::default()
        };
        let survey = survey(catalog, verdicts, &everything)?;
        let skipped = survey
            .items
            .iter()
            .filter(|item| item.state == State::Skipped)
            .count();
        let mut queue = Self {
            items: survey.items,
            taken: 0,
            pending: survey.queue,
            skipped: skipped as u64,
            identified: survey.identified,
            filter: Filter::default(),
            groups: Default::default(),
            printed: BTreeSet::new(),
        };
        queue.refresh();
        Ok(queue)
    }

    /// 一份空队列：还没开库时界面画的就是它。
    #[must_use]
    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            taken: 0,
            pending: 0,
            skipped: 0,
            identified: false,
            filter: Filter::default(),
            groups: Default::default(),
            printed: BTreeSet::new(),
        }
    }

    /// 跑过识别没有。
    #[must_use]
    pub fn identified(&self) -> bool {
        self.identified
    }

    /// 整个队列有多少条**待裁决**（报告口径：默认那三档，不含**跳过**）。
    #[must_use]
    pub fn pending(&self) -> u64 {
        self.pending
    }

    /// 内存里那些**跳过**的有多少条。它们默认不在队列里，勾一下才连它们一起复核。
    #[must_use]
    pub fn skipped(&self) -> u64 {
        self.skipped
    }

    /// 眼下的选择器。
    #[must_use]
    pub fn filter(&self) -> &Filter {
        &self.filter
    }

    /// 选择器选中的那些。**它是一段真的切片**，[`Queue::plan`] 直接拿它去排计划。
    #[must_use]
    pub fn selected(&self) -> &[Item] {
        &self.items[..self.taken]
    }

    /// 选中的那些在这个轴上分成哪些组。**每一行就是一次批量裁决能覆盖多少。**
    #[must_use]
    pub fn groups(&self, axis: Axis) -> &[GroupRow] {
        &self.groups[axis.index()]
    }

    /// 换一套选择器。和现在这套一样就什么都不做——界面每帧都会调它。
    pub fn set_filter(&mut self, filter: Filter) {
        if self.filter != filter {
            self.filter = filter;
            self.refresh();
        }
    }

    /// 排一次批量裁决的计划。**不写任何库。**
    ///
    /// 顺手把选中那些的**内容判据**补齐：那一步要为每个变体查两次中立库，只为选中的
    /// 那些付这笔钱——一万多条的队列不该在只是看一眼分组的时候整份算出来。
    ///
    /// # Errors
    /// 读中立库或沉淀库失败时返回错误。
    pub fn plan(
        &mut self,
        catalog: &Catalog,
        store: &Store,
        decide: &Decide,
    ) -> Result<Plan, TriageError> {
        self.ensure_prints(catalog, 0..self.taken)?;
        plan(store, self.selected(), decide)
    }

    /// 第 `at` 条（选中的那些里数），**内容判据已经补齐**。
    ///
    /// 详情面板要拿它说清「这一条的裁决钉在**内容**上还是只钉得住**路径**」——
    /// 前者换台机器还认得出、可以分享，后者不行，而那是人在按下去之前该知道的事。
    ///
    /// # Errors
    /// 读中立库失败时返回错误。
    pub fn detail(&mut self, catalog: &Catalog, at: usize) -> Result<Option<&Item>, TriageError> {
        if at >= self.taken {
            return Ok(None);
        }
        self.ensure_prints(catalog, at..at + 1)?;
        Ok(self.items.get(at))
    }

    /// 把这一段的**内容判据**补齐，算过的不再算。
    fn ensure_prints(
        &mut self,
        catalog: &Catalog,
        range: std::ops::Range<usize>,
    ) -> Result<(), TriageError> {
        for at in range {
            let Some(item) = self.items.get(at) else {
                break;
            };
            if self.printed.contains(&item.variant.key) {
                continue;
            }
            fill_prints(catalog, &mut self.items[at..=at])?;
            self.printed.insert(self.items[at].variant.key.clone());
        }
        Ok(())
    }

    /// 把计划真正落下，并把落下的那些**从队列里去掉**。
    ///
    /// 去掉这件事不能等下一趟识别：ADR-0002 说队列是主界面，而一个裁完了还留在原地的
    /// 条目会被再问一遍。下一趟 `romcat identify` 重放沉淀库之后，结果与这里一模一样
    /// （[`apply`] 与识别共用同一个投影器）。
    ///
    /// # Errors
    /// 写中立库或沉淀库失败时返回错误。
    pub fn apply(
        &mut self,
        catalog: &mut Catalog,
        store: &mut Store,
        plan: &Plan,
    ) -> Result<Applied, TriageError> {
        let account = apply(catalog, store, self.selected(), plan)?;
        let gone: BTreeSet<&str> = plan.decided.iter().map(|row| row.key.as_str()).collect();
        // 两个数分开减：跳过的那些本来就没算在待裁决里。
        let (mut pending, mut skipped) = (0, 0);
        for item in &self.items {
            if gone.contains(item.variant.key.as_str()) {
                if item.state == State::Skipped {
                    skipped += 1;
                } else {
                    pending += 1;
                }
            }
        }
        self.pending = self.pending.saturating_sub(pending);
        self.skipped = self.skipped.saturating_sub(skipped);
        self.items
            .retain(|item| !gone.contains(item.variant.key.as_str()));
        self.printed.retain(|key| !gone.contains(key.as_str()));
        self.refresh();
        Ok(account)
    }

    /// 重新过一遍选择器：选中的挪到前面，再按三个轴数一遍。
    fn refresh(&mut self) {
        let items = std::mem::take(&mut self.items);
        let (mut taken, rest): (Vec<Item>, Vec<Item>) = items
            .into_iter()
            .partition(|item| self.filter.keeps_item(item));
        self.taken = taken.len();
        taken.extend(rest);
        self.items = taken;
        self.groups = Axis::ALL.map(|axis| tally(self.selected(), axis));
    }
}
