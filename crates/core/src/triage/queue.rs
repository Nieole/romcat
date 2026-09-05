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

use super::batch::{self, Batch, Coverage, Drill, Sample, Scope};
use super::{
    Applied, Axis, Decide, Filter, GroupRow, Item, Plan, State, TriageError, Undone, apply,
    fill_prints, plan, plan_each, redo_batch, survey, tally, undo_batch,
};
use crate::catalog::identify::Tier;
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
    /// 库里有多少个变体**还没识别**（`catalog::identify::NOT_RUN_LABEL`）。
    ///
    /// 它们**一条都不在这份队列里**，而且是对的：队列装的是「识别拿不定主意的那些」，
    /// 而它们连一行结论都没有——没有候选、裁不了，混进 `pending` 会让「还剩多少要人裁」
    /// 变成一个虚数（那正是这一票要消掉的东西）。
    ///
    /// **但它们得有人报数。** 只说「队列 2 条待裁决」，用户会读成「库里只剩 2 条没定
    /// 下来」，而实情是另有 N 个连问都还没问过。屏头与报告在旁边单说一句，
    /// 该做的事也不一样：那一句是「先跑一趟 `romcat identify`」，不是「去裁决」。
    ///
    /// **选择器不筛它**（`Filter::under` 那些也不筛）：它们压根没有
    /// [`Item`] 可筛。这个数说的始终是**整个库**，与 [`Queue::pending`] 同一个口径。
    not_run: u64,
    /// 眼下的选择器。
    filter: Filter,
    /// 选中的那些按三个轴分出来的组，与 [`Axis::ALL`] 同序。
    groups: [Vec<GroupRow>; Axis::ALL.len()],
    /// 选中的那些按**依据形状**分出来的一级批，多的排前面。
    ///
    /// 与 `groups` 一起在换选择器时算一遍：分批本来就要走完全部条目，而走完了不留着
    /// 等于每帧再走一遍。真机上这是一万八千条一趟——与三个轴那三趟同一个量级。
    batches: Vec<Batch>,
    /// 选中的那些按**四档**各有多少条。屏头上那几个数。
    tiers: [(Tier, u64); Tier::ALL.len()],
    /// 队列**换过几次样子**：换选择器、裁完一批、撤回一批，各算一次。
    ///
    /// 界面拿它当**缓存的钥匙**：展开那一批的二级分组与随机样本只在这个数变了之后
    /// 才要重算，而它们各要走一遍这一批的全部条目——一批一万两千条上，每帧重算一次
    /// 就是每帧一万两千次分配。
    revision: u64,
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
            // **另问一次库**：`survey` 走的是 `queue_rows`，那是
            // `variant JOIN identification`——还没识别的变体一行都进不去，
            // 从它身上无论如何数不出这个数。
            not_run: catalog.not_run_count()?,
            identified: survey.identified,
            filter: Filter::default(),
            groups: Default::default(),
            batches: Vec::new(),
            tiers: batch::by_tier(&[]),
            revision: 0,
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
            not_run: 0,
            identified: false,
            filter: Filter::default(),
            groups: Default::default(),
            batches: Vec::new(),
            tiers: batch::by_tier(&[]),
            revision: 0,
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

    /// 库里有多少个变体**还没识别**。见这个字段的文档：它们不在队列里、也不算待裁决，
    /// 但屏头要说得出「另有 N 个还没识别」，否则那个 N 就被整个抹掉了。
    #[must_use]
    pub fn not_run(&self) -> u64 {
        self.not_run
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

    /// 队列换过几次样子。**界面拿它当缓存的钥匙**，见这个字段的文档。
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// **一级分批**：选中的那些按依据形状分成的几十批，多的排前面。
    ///
    /// 待确认屏打开看见的就是它——不是一万八千行的表（票 `gui-redesign/09`）。
    #[must_use]
    pub fn batches(&self) -> &[Batch] {
        &self.batches
    }

    /// 选中的那些按**四档**各有多少条。
    #[must_use]
    pub fn tiers(&self) -> &[(Tier, u64)] {
        &self.tiers
    }

    /// 选中的那些一共挂着多少条**候选**。屏头上「N 变体 · M 条候选」的后一个数。
    #[must_use]
    pub fn candidates(&self) -> u64 {
        self.selected()
            .iter()
            .map(|item| item.candidates.len() as u64)
            .sum()
    }

    /// 这个范围里的那些条目。**整批操作、下钻、抽样都从它出发。**
    #[must_use]
    pub fn members(&self, scope: &Scope) -> Vec<&Item> {
        self.at_in(scope).map(|at| &self.items[at]).collect()
    }

    /// 这个范围里有多少条。屏上「整批通过 3,053 条」写的就是它。
    #[must_use]
    pub fn count(&self, scope: &Scope) -> u64 {
        self.at_in(scope).count() as u64
    }

    /// 这个范围里的那些条目**排在选中的第几位**。
    ///
    /// 三处（[`Queue::members`]、[`Queue::count`]、[`Queue::plan_scope`]）走同一条：
    /// 「屏上写着 3,053 条」「抽样抽的那一批」「按下去真的改的那一批」必须是同一批，
    /// 各写一遍的话它们迟早各说各的。
    fn at_in<'a>(&'a self, scope: &'a Scope) -> impl Iterator<Item = usize> + 'a {
        self.selected()
            .iter()
            .enumerate()
            .filter(move |(_, item)| scope.holds(item))
            .map(|(at, _)| at)
    }

    /// 一屏分批的**账**：分成几批、前 `head` 批盖住多少、按批答得了的多少。
    #[must_use]
    pub fn coverage(&self, head: usize) -> Coverage {
        batch::coverage(&self.batches, head)
    }

    /// **二级下钻**：这个范围按某个轴再分一层。
    ///
    /// 三个轴是[既有的那三个](Axis)——报告与命令行数的是同一批，界面不另造一套。
    #[must_use]
    pub fn drill(&self, scope: &Scope, axis: Axis) -> Drill {
        batch::drill(&self.members(scope), axis)
    }

    /// 这个范围里的**随机样本**。**换一组样本就换个 `seed`**。
    #[must_use]
    pub fn sample(&self, scope: &Scope, seed: u64, want: usize) -> Vec<Sample> {
        batch::sample(&self.members(scope), seed, want)
    }

    /// 排一次**整批**裁决的计划。**不写任何库。**
    ///
    /// 与 [`Queue::plan`] 是同一件事，只是范围从「选择器选中的全部」收到「这一批」
    /// ——屏上按下「整批通过」时选择器一个字都没动，人看的还是那一列卡片。
    ///
    /// # Errors
    /// 读中立库或沉淀库失败时返回错误。
    pub fn plan_scope(
        &mut self,
        catalog: &Catalog,
        store: &Store,
        decide: &Decide,
        scope: &Scope,
    ) -> Result<Plan, TriageError> {
        let at: Vec<usize> = self.at_in(scope).collect();
        for index in &at {
            self.ensure_prints(catalog, *index..index + 1)?;
        }
        plan_each(store, at.iter().map(|index| &self.items[*index]), decide)
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

    /// 撤掉一**批**，并把队列整份重列。
    ///
    /// **重列而不是就地补回去**：撤销把中立库那一半退回了这一批落下之前的样子
    /// （结论、候选、变体身上那两条链接都在动），而队列是从中立库折出来的。就地拼一份
    /// 的话，界面上看见的与库里躺着的迟早各说各的——那正是这一票要消掉的东西。
    ///
    /// 界面与命令行走的是同一条 [`undo_batch`]（ADR-0005：核心是独立的库）。
    ///
    /// # Errors
    /// 没这一批、这一批已经撤过了、或者读写两份库失败时返回错误。
    pub fn undo(
        &mut self,
        catalog: &mut Catalog,
        store: &mut Store,
        library: &str,
        batch: i64,
    ) -> Result<Undone, TriageError> {
        let account = undo_batch(catalog, store, batch)?;
        self.reload(catalog, store, library)?;
        Ok(account)
    }

    /// 把撤掉的那一**批**放回去，并把队列整份重列。理由同 [`Queue::undo`]。
    ///
    /// # Errors
    /// 没这一批、这一批没撤过、或者读写两份库失败时返回错误。
    pub fn redo(
        &mut self,
        catalog: &mut Catalog,
        store: &mut Store,
        library: &str,
        batch: i64,
    ) -> Result<Applied, TriageError> {
        let account = redo_batch(catalog, store, batch)?;
        self.reload(catalog, store, library)?;
        Ok(account)
    }

    /// 整份重列一次，**选择器原样留着**。
    fn reload(
        &mut self,
        catalog: &Catalog,
        store: &Store,
        library: &str,
    ) -> Result<(), TriageError> {
        let filter = std::mem::take(&mut self.filter);
        let index = verdict::Index::load(store, library)?;
        *self = Self::load(catalog, &index)?;
        self.set_filter(filter);
        Ok(())
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
        self.batches = batch::batches(self.selected());
        self.tiers = batch::by_tier(self.selected());
        self.revision = self.revision.wrapping_add(1);
    }
}
