//! **合并作品**与**移出此作品**（票 `gui-looks-like-the-design/16`）。
//!
//! 把本来是同一个游戏、却被识别成两个**作品**的东西并到一起：选一个保留，其余作品的
//! **变体**改挂到它名下；反向操作是把一个变体**移出此作品**，放到新建的或另一个已有作品。
//!
//! ## 它**不是新机制**，所以它住在这儿
//!
//! 落成的是一**批裁决**，每个归入的变体一条，与待确认队列里**手工指定作品**
//! （[`DecisionSpec::Manual`](super::DecisionSpec::Manual)）是同一种（`CONTEXT.md` 的
//! **合并作品**条）。于是 [`apply`](super::apply)、[`undo_batch`](super::undo_batch)、
//! [`redo_batch`](super::redo_batch) 一个字都不改，**撤销照旧以批为粒度**——
//! 这一层只管把「要落哪几条」折出来，落下与撤销走的是那条走了一整轮的路。
//!
//! ## 三件这一层非做不可的事
//!
//! ### 一、**发行版信息一个字不动**
//!
//! 现成的 [`DecisionSpec::Manual`](super::DecisionSpec::Manual) 走
//! [`resolve`](super::resolve) 时造的是「只有作品名与变体自己的平台」那一份事实——
//! 地区、序列号、语言、中文身份全落空。合并要动的变体**大多已经撞上过 DAT**，照那条路
//! 落一遍等于把撞出来的事实整片抹掉，与词表**合并作品**条里「发行版信息一个字不动」
//! 逐字相反。
//!
//! 所以这一层**自己排计划**（[`plan`]），每条裁决的事实起头取「这个变体眼下那一份」：
//! 沉淀库已经对这份内容说过话的用它那一份（[`Verdict`] 里的 [`Facts`]），没裁过的从
//! 中立库里识别落下的那一行读回来（`release` 那几格加 `identification.edition`），
//! **只把 [`Facts::work`] 换掉**，别的原样抄回去。
//!
//! ⚠️ **DAT 条目名尾巴上的修订标记（`release.revision`）过不来**：裁决落成的发行版行
//! 由 [`Projector`](crate::identify::Projector) 建，那一处刻意不写 `revision`（几个变体
//! 共用一行，写进去会让头一个落库的盖住其余几个），而 [`Facts`] 里没有装它的格子。
//! 挂单 `Q1010`，与 `Q993`／`Q994` 同一族。
//!
//! ### 二、**条目不从待确认队列里取**
//!
//! [`survey`](super::survey) 只交回队列里的条目（没有自动通过的候选、沉淀库还没说过话），
//! 而合并要动的变体**全都已经自动通过**——它们一条都不在队列里。所以这一层照
//! [`Item`](super::Item) 的形状自己折（变体行、内容判据整批取回来），交给
//! [`apply`](super::apply) 的仍是同一种东西。
//!
//! ### 三、**按下去之前先算清会发生什么**
//!
//! [`Impact`] 四样各答一个问题，与[移除一个根](crate::catalog::roots::RootRemoval)
//! 那一处同一个形状、同一条纪律：**判断都在别处那一份**——前端条目怎么分堆照导出那一处
//! （[`converge::Entries`]，ADR-0013 与 ADR-0024），哪台子库装着这些变体照子库求值那一处
//! （[`sublibrary::holding`](crate::sublibrary::holding)，ADR-0016）。
//!
//! **它贵**：要走一遍全库变体表加一趟子库求值。界面在按「下一步」进第三步那一下算一次、
//! 把结果攥在手里（挂单 `Q1011`，与票 26 的 `Q983` 同源），**不许进画帧那条线程**。

use std::collections::{BTreeMap, BTreeSet};

use crate::adapter::converge;
use crate::catalog::identify::Candidate;
use crate::catalog::{Catalog, CatalogError, Confidence, State, TitleRow, VariantRow};
use crate::identify;
use crate::scrape::priority::{Priorities, Said, VERDICT, entry_fields};
use crate::scrape::{AnchorKind, Field};
use crate::title::{TitleKind, TitleSet, language_of};
use crate::verdict::{Decision, Facts, Store, Verdict, VerdictError};

use super::{Applied, Blocked, Decided, Item, Plan, TriageError};

/// 一次归属改写是**哪一种**。
///
/// 屏上两处入口、库里一条路：两支落下的东西逐字同形（一条
/// [`Decision::Release`] 裁决，只把作品换掉），差的只是**说给人听的那句话**
/// ——裁决记录半年后要认得出当初按的是哪一颗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// **合并作品**：其余作品的变体改挂到保留的那个名下。
    Merge,
    /// **移出此作品**：一个变体移到新建的或另一个已有的作品名下。
    Split,
}

impl Kind {
    /// 屏上与裁决记录里这一种叫什么。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Merge => "合并作品",
            Self::Split => "移出此作品",
        }
    }

    /// 这一**批**在裁决记录里那句摘要。
    ///
    /// 收在这儿而不是界面上：[`Plan::summary`] 会原样落进 [`verdict::Batch::summary`]
    /// （[`apply`](super::apply)），**计划书上印的与半年后按编号撤销时看见的必须是同一句**。
    fn summary(self, from: &[String], into: &str) -> String {
        let 谁 = if from.is_empty() {
            // 归入的是**还没认出作品**的变体：它没有作品名可写，但这一批照样得说得出
            // 「从哪儿来的」。
            NO_WORK.to_string()
        } else {
            from.iter()
                .map(|name| format!("《{name}》"))
                .collect::<Vec<_>>()
                .join("、")
        };
        format!("{} · {谁} → 《{into}》", self.label())
    }
}

/// 归入的变体**还没认出作品**时，那一句里替它说的话。
const NO_WORK: &str = "还没认出作品的变体";

/// 合并 / 移出跑不下去的原因。
///
/// 每一条都说得出**人该怎么办**：这一层的入口全是人刚点过的东西，
/// 「做不成」而不说为什么，人只能挨个试。
#[derive(Debug, thiserror::Error)]
pub enum MergeError {
    /// 没说要归到哪个作品名下。
    #[error("要归到哪个作品名下得说得出来：保留的那个作品，或者新建作品的名字")]
    NoWork,
    /// 一个要归入的变体都没有。
    #[error("一个要归入的变体都没有：取消勾选的那些仍留在原来的作品里，至少得留下一个")]
    Nothing,
    /// 中立库里没有这个变体。
    #[error("中立库里没有这个变体：{0}。库可能重扫过了，回浏览屏重新选一遍")]
    NoVariant(String),
    /// 中立库读写失败。
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    /// 沉淀库读写失败。
    #[error(transparent)]
    Verdict(#[from] VerdictError),
}

/// 一次合并 / 移出**将要**做的事：交给 [`apply`] 的那一份。
///
/// **先出计划再动手**（ADR-0016）：里头装着要落的每一条裁决、落不下去的连原因，
/// 以及这一批的摘要。[`Impact`] 另算——那一趟贵。
#[derive(Debug, Clone)]
pub struct Regrouping {
    /// 哪一种。
    kind: Kind,
    /// 归到哪个作品名下。
    into: String,
    /// 从哪几个作品来的（去重、排过序）；还没认出作品的那些不在里面。
    from: Vec<String>,
    /// 交给 [`apply`](super::apply) 的那几条：核心库照 [`Item`] 的形状自己折的，不经队列。
    items: Vec<Item>,
    /// 计划本身。
    plan: Plan,
}

impl Regrouping {
    /// 哪一种。
    #[must_use]
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// 归到哪个作品名下。
    #[must_use]
    pub fn into_work(&self) -> &str {
        &self.into
    }

    /// 从哪几个作品来的。**别名要留的就是这几个名字**（[`keep_aliases`]）。
    #[must_use]
    pub fn from(&self) -> &[String] {
        &self.from
    }

    /// 计划本身。
    #[must_use]
    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    /// **写几条裁决**：每个归入的变体一条。屏上「合并后会发生什么」头一行写它。
    #[must_use]
    pub fn verdicts(&self) -> u64 {
        u64::try_from(self.plan.decided.len()).unwrap_or(u64::MAX)
    }

    /// 落不下去的那几条，连原因。
    #[must_use]
    pub fn blocked(&self) -> &[Blocked] {
        &self.plan.blocked
    }

    /// 这一批要动的那几个变体的键。
    #[must_use]
    pub fn moving(&self) -> Vec<&str> {
        self.items
            .iter()
            .map(|item| item.variant.key.as_str())
            .collect()
    }
}

/// 排一次合并 / 移出的计划。**两份库一个字都不写。**
///
/// `moving` 是要归入的那几个变体的键——**作用范围在界面那一侧展开**
/// （`Catalog::scoped_variants`，与屏上那一行写着的变体数同一个数），这一层只收键。
/// 已经挂在 `into` 名下的会被悄悄略过：它们本来就在那儿，再裁一遍只是多一条一模一样的
/// 裁决。
///
/// # Errors
/// 没说要归到哪个作品、一个变体都不剩、点名的变体不在库里、或者读两份库失败时返回错误。
pub fn plan(
    catalog: &Catalog,
    store: &Store,
    library: &str,
    kind: Kind,
    into: &str,
    moving: &[String],
) -> Result<Regrouping, MergeError> {
    let into = into.trim();
    if into.is_empty() {
        return Err(MergeError::NoWork);
    }
    let works = catalog.work_names()?;
    let mut rows: Vec<VariantRow> = Vec::new();
    let mut from: BTreeSet<String> = BTreeSet::new();
    for key in moving {
        let Some(row) = catalog.variant(key)? else {
            return Err(MergeError::NoVariant(key.clone()));
        };
        match row.work_id.and_then(|id| works.get(&id)) {
            // 已经在它名下——不必再裁一遍。
            Some(name) if name == into => continue,
            Some(name) => {
                from.insert(name.clone());
            }
            None => {}
        }
        rows.push(row);
    }
    if rows.is_empty() {
        return Err(MergeError::Nothing);
    }
    // **内容判据整批一趟取回来**（同队列那一步 [`fill_prints`](super::fill_prints)）：
    // 一条一条地问是一个变体四次库。
    let prints = identify::content_prints(catalog, rows.iter())?;
    let from: Vec<String> = from.into_iter().collect();
    let note = kind.summary(&from, into);
    let mut plan = Plan {
        summary: note.clone(),
        note: Some(note.clone()),
        library: library.to_string(),
        ..Plan::default()
    };
    // **一批是一次落下，也就是一个时刻**——理由与 [`plan_each`](super::plan_each) 逐字同一条：
    // 同一条内容锚上的几份**重复拷贝**差一秒就成了两条，撤销那一侧再也认不出
    // 「锚上眼下这条是不是这一批自己落下的」。
    let decided_at = crate::catalog::now_secs();
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let key = row.key.clone();
        let (state, reason) = match catalog.identification_of(&key)? {
            Some((state, reason)) => (state, reason),
            // 连识别都还没跑过的变体照样归得进去（**还没识别**那一档）。这一格
            // [`apply`](super::apply) 一眼都不看，摆在这儿只为让这条 [`Item`] 长得完整。
            None => (State::NoEvidence, None),
        };
        let item = Item {
            variant: row,
            state,
            reason,
            // **候选不读**：合并不挑候选，而真库上那是 15 万行。
            candidates: Vec::new(),
            print: prints.get(&key).cloned(),
        };
        let anchor = item.anchor(library);
        let facts = facts_of(catalog, store, &item.variant, &anchor, into)?;
        plan.against.insert(key.clone());
        plan.decided.push(Decided {
            key,
            verdict: Verdict::at(anchor.clone(), Decision::Release(facts), decided_at)
                .with_note(Some(note.clone())),
            replaces: store.find(&anchor)?.is_some(),
        });
        items.push(item);
    }
    Ok(Regrouping {
        kind,
        into: into.to_string(),
        from,
        items,
        plan,
    })
}

/// 这个变体**眼下那一份事实**，作品换成 `into`，别的原样抄回去。
///
/// 回退链只有两档，与 `VariantDetail::edition` 那条同一种写法：
///
/// 1. **沉淀库已经对这份内容说过话**——用它那一份。那是人亲手定的，比库里算出来的权威。
///    说的是「没有发行版」或者「认不出」的，那两档没有事实可抄，落回第二档。
/// 2. **没裁过**——从中立库里识别落下的那一行读：发行版那几格（平台、地区、序列号、语言）、
///    已接受候选上的中文身份、`identification.edition` 记着的**第几版**。
fn facts_of(
    catalog: &Catalog,
    store: &Store,
    row: &VariantRow,
    anchor: &crate::verdict::Anchor,
    into: &str,
) -> Result<Facts, MergeError> {
    if let Some(found) = store.find(anchor)?
        && let Decision::Release(facts) = found.decision
    {
        return Ok(Facts {
            work: into.to_string(),
            ..facts
        });
    }
    let release = match row.release_id {
        Some(id) => catalog.release(id)?,
        None => None,
    };
    let chinese = catalog
        .candidates_of(&row.key)?
        .into_iter()
        .find(|candidate: &Candidate| candidate.accepted)
        .and_then(|candidate| candidate.chinese);
    Ok(Facts {
        work: into.to_string(),
        // 发行版那一行没说平台时退回变体自己的那个——目录是强先验（ADR-0011），
        // 与 [`resolve`](super::resolve) 收尾那一句同一条。
        platform: release
            .as_ref()
            .and_then(|release| release.platform.clone())
            .or_else(|| row.platform.clone()),
        region: release.as_ref().and_then(|release| release.region.clone()),
        serial: release.as_ref().and_then(|release| release.serial.clone()),
        languages: release
            .as_ref()
            .and_then(|release| release.languages.clone()),
        chinese,
        // **汉化组**只活在沉淀库里（[`Projector`](crate::identify::Projector) 不投影它），
        // 没裁过的变体身上本来就没有这一格。
        team: None,
        version: catalog.decided_edition(&row.key)?,
    })
}

/// 把计划真正落下。**走的是批量裁决那条路**（[`apply`](super::apply)）：沉淀库记这一批、
/// 中立库收起落下之前的结论，于是[撤销](super::undo_batch)把两边一起退回去。
///
/// # Errors
/// 与 [`apply`](super::apply) 同。
pub fn apply(
    catalog: &mut Catalog,
    store: &mut Store,
    regrouping: &Regrouping,
) -> Result<Applied, TriageError> {
    super::apply(catalog, store, &regrouping.items, &regrouping.plan)
}

/// **合并 / 移出之后会发生什么**：按下那一下之前该看见的那笔账。
///
/// 四样各答一个问题，判断**一条都不在这儿**：前端条目怎么分堆照导出那一处
/// （[`converge::Entries`]），哪台子库装着这些变体照子库求值那一处
/// （[`sublibrary::holding`](crate::sublibrary::holding)）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Impact {
    /// 写几条裁决：每个归入的变体一条。
    pub verdicts: u64,
    /// **前端条目**眼下多少条。
    pub entries_before: u64,
    /// 合并之后多少条。收敛在导出那一步（词表**收敛**），所以这一格说的是「下次导出」。
    pub entries_after: u64,
    /// 哪几个作品会**一个变体都不剩**——它们的名字该留作**别名**。
    pub emptied: Vec<String>,
    /// 哪几台**子库**的选择集里装着这些变体：它们的**选择集不变**（选的是变体），
    /// 但前端条目变了，同步之前要重新生成差量预览。
    pub sublibraries: Vec<String>,
}

/// 算一遍 [`Impact`]。**两份库一个字都不写，主库一个字节都不读**（ADR-0001、ADR-0004）。
///
/// ## 这一趟不便宜
///
/// 前端条目那两个数要走一遍全库变体表加一趟 [`converge::Entries`]，子库那一栏是一趟
/// [`facts_of`](crate::sublibrary::facts_of) 加逐台求值。**不许进画帧那条线程**：
/// 界面按「下一步」进第三步时算一次、把结果攥在手里（挂单 `Q1011`）。
///
/// # Errors
/// 读中立库失败时返回错误。
pub fn impact(catalog: &Catalog, regrouping: &Regrouping) -> Result<Impact, CatalogError> {
    let moving: BTreeSet<&str> = regrouping.moving().into_iter().collect();
    let entries = converge::Entries::load(catalog)?;
    let works = catalog.work_names()?;
    let mut before: BTreeSet<(String, converge::Anchor)> = BTreeSet::new();
    let mut after: BTreeSet<(String, converge::Anchor)> = BTreeSet::new();
    // 作品名 → 这一趟之后它底下还剩几个变体。
    let mut left: BTreeMap<&str, u64> = BTreeMap::new();
    for name in &regrouping.from {
        left.insert(name.as_str(), 0);
    }
    for variant in catalog.variants()? {
        let mine = moving.contains(variant.key.as_str());
        if !mine
            && let Some(name) = variant.work_id.and_then(|id| works.get(&id))
            && let Some(rest) = left.get_mut(name.as_str())
        {
            *rest += 1;
        }
        // 做不成条目的那几个（**补丁**、**附属内容**、**非游戏资产**）一开始就不算数。
        let Ok((platform, anchor)) = entries.of(&variant) else {
            continue;
        };
        before.insert((platform.clone(), anchor.clone()));
        let anchor = if mine {
            converge::Anchor::Work(regrouping.into.clone())
        } else {
            anchor
        };
        after.insert((platform, anchor));
    }
    let keys = regrouping.moving();
    Ok(Impact {
        verdicts: regrouping.verdicts(),
        entries_before: u64::try_from(before.len()).unwrap_or(u64::MAX),
        entries_after: u64::try_from(after.len()).unwrap_or(u64::MAX),
        emptied: left
            .into_iter()
            .filter(|(_, rest)| *rest == 0)
            .map(|(name, _)| name.to_string())
            .collect(),
        sublibraries: crate::sublibrary::holding(catalog, &keys)?,
    })
}

/// 别的作品在这个字段上说的那一句。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    /// 哪个作品说的。
    pub work: String,
    /// 它眼下写出去的是谁说的什么（[`Said`]，与导出、元数据那一面同一处算）。
    pub said: Said,
}

/// 一个字段上的**冲突**：保留作品与别的作品各说各的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// 哪个字段。
    pub field: Field,
    /// 保留作品眼下写出去的；这个字段它空着时是 `None`。
    pub keep: Option<Said>,
    /// 别的作品各说的什么——**只列与保留作品不一样、而且不是空的**。
    pub others: Vec<Offer>,
}

/// **只列出有冲突的字段**：两边一样的、对方空着的一条都不列。
///
/// 每个作品写出去的是什么由 [`entry_fields`] 答（与导出、作品详情页元数据那一面**同一处**，
/// ADR-0024）；这一层只做减法。次序照 [`Field::all`] 那份固定的排列。
///
/// **汉化组不在里面**：它挂在**变体**上（ADR-0012），合并一个字都不动它。
///
/// # Errors
/// 读中立库失败时返回错误。
pub fn conflicts(
    catalog: &Catalog,
    priorities: &Priorities,
    keep: &str,
    sources: &[String],
) -> Result<Vec<Conflict>, CatalogError> {
    let shown = |work: &str| -> Result<BTreeMap<Field, Option<Said>>, CatalogError> {
        Ok(
            // 按平台的覆盖与**汉化组**那一格这里都用不上：作品级的字段与平台无关，
            // 而汉化组挂在变体上、合并不动它。
            entry_fields(catalog, AnchorKind::Work, work, "", None, priorities)?
                .into_iter()
                .map(|one| (one.field, one.shown))
                .collect(),
        )
    };
    let mine = shown(keep)?;
    let theirs: Vec<(String, BTreeMap<Field, Option<Said>>)> = sources
        .iter()
        .filter(|work| work.as_str() != keep)
        .map(|work| Ok((work.clone(), shown(work)?)))
        .collect::<Result<_, CatalogError>>()?;
    let mut out = Vec::new();
    for field in Field::all() {
        if field == Field::TranslationGroup {
            continue;
        }
        let keep_said = mine.get(&field).cloned().flatten();
        let mut others = Vec::new();
        for (work, fields) in &theirs {
            let Some(said) = fields.get(&field).cloned().flatten() else {
                continue;
            };
            if keep_said
                .as_ref()
                .is_some_and(|mine| mine.values == said.values)
            {
                continue;
            }
            others.push(Offer {
                work: work.clone(),
                said,
            });
        }
        if !others.is_empty() {
            out.push(Conflict {
                field,
                keep: keep_said,
                others,
            });
        }
    }
    Ok(out)
}

/// 第三步选中了别的作品那一格：把它记到保留作品名下。
///
/// **走的是作品详情页「改用另一个来源的值」同一条路**（`put_verdict_value` /
/// `put_titles`，源记**裁决**）：手动改写优先于所有数据源，重新刮削不覆盖。
/// 显示标题另走标题集合——那一格不是刮削字段，它由 [`choose`](crate::title::choose) 挑。
///
/// ⚠️ **它不在这一批裁决里**，所以[撤销这一批](super::undo_batch)不会把它退回去；
/// 要改回来在作品详情页元数据那一面上原地撤（挂单 `Q1012`）。
///
/// # Errors
/// 写中立库失败时返回错误。
pub fn adopt(
    catalog: &mut Catalog,
    priorities: &Priorities,
    keep: &str,
    field: Field,
    offer: &Offer,
) -> Result<(), CatalogError> {
    let why = format!("{}：改用《{}》的值", Kind::Merge.label(), offer.work);
    if field == Field::Title {
        let chosen = crate::title::choose(
            &TitleSet {
                work: offer.work.clone(),
                entries: catalog.titles_of(&offer.work)?,
            },
            priorities,
        );
        return catalog.put_titles(&[TitleRow {
            work: keep.to_string(),
            value: chosen.display,
            language: chosen.language,
            // 挑出来的就是作品名本身时它不属于任何一档，记成**别名**——谁也没为它背书。
            kind: chosen.kind.unwrap_or(TitleKind::Alias),
            source: VERDICT.to_string(),
            region: None,
            variant_key: None,
            confidence: Confidence::High,
            seam: chosen.seam,
            evidence: why,
            seen: 1,
        }]);
    }
    catalog.put_verdict_value(
        AnchorKind::Work,
        keep,
        field,
        &offer.said.values.join("、"),
        &why,
    )
}

/// 把被合并作品的名字**留作别名**：搜这些名字仍能找到合并后的作品。
///
/// 记进保留作品的标题集合、源是**裁决**——重新整理标题时一行都不碰
/// （`Catalog::clear_titles`）。
///
/// ⚠️ 同 [`adopt`]：**它不在这一批裁决里**，撤销这一批不会把它去掉；
/// 要去掉在作品详情页标题那一面上删（挂单 `Q1012`）。
///
/// # Errors
/// 写中立库失败时返回错误。
pub fn keep_aliases(
    catalog: &mut Catalog,
    keep: &str,
    names: &[String],
) -> Result<(), CatalogError> {
    let rows: Vec<TitleRow> = names
        .iter()
        .filter(|name| name.as_str() != keep)
        .map(|name| TitleRow {
            work: keep.to_string(),
            value: name.clone(),
            language: language_of(name, None),
            // **别名**：既不是官方名称也不是译名，谁也没为它背书（词表**别名**那一档）。
            kind: TitleKind::Alias,
            source: VERDICT.to_string(),
            region: None,
            variant_key: None,
            confidence: Confidence::High,
            seam: None,
            evidence: format!(
                "{}：《{name}》并入《{keep}》，名字留作别名",
                Kind::Merge.label()
            ),
            seen: 1,
        })
        .collect();
    catalog.put_titles(&rows)
}

/// **「以后扫描到的也自动归入」还做不到**，屏上那个勾选框不可选，写的就是这一句。
///
/// 2026-09-13 裁定暂缓：合并记在**每个变体的内容锚**上，要做到自动归入，得在**沉淀库**
/// 新增一条**作品级**的「A 与 B 是同一作品」记录（`CONTEXT.md` 的**合并作品**条）。
/// 收在核心库里而不是界面上：这句话说的是库能做什么、做不到什么，命令行与日后别的壳
/// 照样要它。
#[must_use]
pub fn auto_absorb_reason(into: &str) -> String {
    format!(
        "还不能选：合并记在每个变体上，要做到自动归入，需要在沉淀库新增一条作品级的\
         「A 与 B 是同一作品」记录。眼下新扫描到的同类变体仍会单独成为一个作品，\
         可以再合并一次——再合一次就归进《{into}》了。"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 这一批的摘要说得出从哪儿来到哪儿去() {
        let 从 = ["口袋妖怪 红".to_string(), "精灵宝可梦 红".to_string()];
        assert_eq!(
            Kind::Merge.summary(&从, "宝可梦 红"),
            "合并作品 · 《口袋妖怪 红》、《精灵宝可梦 红》 → 《宝可梦 红》",
        );
        assert_eq!(
            Kind::Split.summary(&从[..1], "幻想传说（汉化版）"),
            "移出此作品 · 《口袋妖怪 红》 → 《幻想传说（汉化版）》",
        );
    }

    #[test]
    fn 归入的是还没认出作品的变体时那一句照样说得出从哪儿来() {
        let 话 = Kind::Merge.summary(&[], "幻想传说");
        assert!(
            话.contains(NO_WORK),
            "没有作品名可写时那一句得替它说清：{话}",
        );
        assert!(话.contains("《幻想传说》"), "到哪儿去也得写：{话}");
    }
}
