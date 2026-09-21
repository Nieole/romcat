//! **一级分批**：把待确认队列按**依据形状**分成几十批，每批一句共同依据、一组随机样本。
//!
//! ## 为什么一定要有这一层
//!
//! 真机上待裁决 **18,241 个变体**（`.scratch/gui-redesign/spec.md`）。按 5 秒一条算是
//! **25 小时**——逐条不是可行路径。但其中八成只有**一个候选**：那不是「选哪个」，
//! 是「**对不对**」，而「对不对」可以按批回答。
//!
//! 分批的键是**依据形状**，因为它就是「**工具凭什么这么认为**」——
//! 而整批通过时人验证的正是那句话。按目录分、按平台分都做不到这一点：同一个目录里
//! 可以既有精确哈希撞上的，又有文件名猜出来的，一句话盖不住。
//!
//! ## 「批」这个词在这个 crate 里有两个意思
//!
//! - **这里的批**：一组**依据形状相同的待裁决变体**。它是屏上的一张卡片，
//!   还没落任何库。
//! - [`verdict::Batch`](crate::verdict::Batch)：一次 [`apply`](super::apply) 落下的
//!   那些**裁决**，**撤销以它为粒度**。
//!
//! 两者在按下「整批通过」那一刻一一对应：这一批变体落成那一批裁决。词表**两个都收了**
//! （`CONTEXT.md` 的「批」那一条，连「一批变体」这个叫法一起）。
//!
//! ## 一条只落一个形状
//!
//! [`Shape::of`] 取的是**第一条候选**。取「置信度最高的那条」听着更聪明，但
//! 「整批通过」就是 `--pick 1`（[`DecisionSpec::Pick`](super::DecisionSpec::Pick)），
//! 它采用的正是第一条——分批时说 A、按下去做 B，那句共同依据当场变成假话。
//!
//! 于是各批条数加起来**恰好**是队列的条数，屏上那个「前 5 批盖住多少」才算得出来。

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::catalog::identify::{Confidence, Tier};
use crate::catalog::{Candidate, State};
use crate::dat::Convention;

use super::{Axis, GroupRow, Item};

/// **选择器**那串字里各段之间的分隔。
///
/// 两边留空格：源与 DAT 的名字里塞满了 `-`、`(`、`)`，一个光秃秃的 `/` 贴着字，
/// 人一眼分不出哪儿是段界。
const SEP: &str = " / ";

/// [`Shape::Bare`] 那一支的头一段。
///
/// **与 [`Fanout::None`] 那个词不是同一句话**：那个说的是「这一批的候选数落在零那一档」，
/// 这一个说的是「这一批根本不走候选那条路，它说的是为什么没定下来」。
const BARE: &str = "一条候选都没有";

/// 一批的**候选数**落在哪一档。
///
/// 分档而不是记确切条数：屏上要的是「这一批能不能按批回答」，而那只分得出几档。
/// 档的边界照真库实测的分布来（2–3 个 2,750、4–10 个 688、10 个以上 146）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Fanout {
    /// **一条候选都没有**。真机上队列里绝大多数条目在这一档。
    None,
    /// **只有一个候选**：那不是「选哪个」，是「对不对」——按批回答得了的正是这些。
    One,
    /// 2–3 个。
    Few,
    /// 4–10 个。
    Several,
    /// 10 个以上。
    Many,
}

impl Fanout {
    /// 五档全在这儿。
    pub const ALL: [Self; 5] = [Self::None, Self::One, Self::Few, Self::Several, Self::Many];

    /// 有这么多条候选，落在哪一档。
    #[must_use]
    pub fn of(candidates: usize) -> Self {
        match candidates {
            0 => Self::None,
            1 => Self::One,
            2..=3 => Self::Few,
            4..=10 => Self::Several,
            _ => Self::Many,
        }
    }

    /// 屏上写成什么：依据形状的末一段（设计稿 `MAME / gameboy.xml / 含头 / 1 个候选`，拿主意的人 2026-09-15 定），
    /// 命令行 `--shape` 那串字的末一段也是它。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "没有候选",
            Self::One => "1 个候选",
            Self::Few => "2–3 个候选",
            Self::Several => "4–10 个候选",
            Self::Many => "10 个以上候选",
        }
    }

    /// 共同依据那句话里的说法，前面接着一个「都」字：「都只有一个候选」（[`Batch::why`]）。
    ///
    /// 与 [`Fanout::label`] 分开：那一个是形状的一段（「1 个候选」），这一个是一句话里的谓语。
    #[must_use]
    pub fn sentence(self) -> &'static str {
        match self {
            Self::None => "没有候选",
            Self::One => "只有一个候选",
            Self::Few => "有 2–3 个候选",
            Self::Several => "有 4–10 个候选",
            Self::Many => "有 10 个以上候选",
        }
    }

    /// 从词认回来。[`Shape::parse`] 认末一段靠它。
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|one| one.label() == label)
    }

    /// 这一档按批回答得了吗。
    ///
    /// **只有单候选那一档算数**：多候选问的是「选哪个」，同一批里各人的候选不是同一部
    /// 游戏，一次按下去等于替几千条各挑了一个没看过的答案。它们走逐条键盘流。
    #[must_use]
    pub fn answerable(self) -> bool {
        self == Self::One
    }

    /// 这一档是**有多个候选**的吗（两个以上）：问的是「选哪个」，只能逐条选（[`Coverage::multiple`] 数的就是这几档）。
    #[must_use]
    pub fn multiple(self) -> bool {
        !matches!(self, Self::None | Self::One)
    }
}

/// 一批的**依据形状**：这一批的变体，工具凭什么这么认为。
///
/// 两支不是同一种话：有候选时说的是「凭哪个源的哪份 DAT、用哪套哈希、多确信」，
/// 一条候选都没有时说的是「**为什么没定下来**」。硬凑成一个形状的话，后一支的
/// 源与 DAT 全是空的，屏上就是几十行「— / — / —」。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Shape {
    /// **有候选**：第一条候选的源、DAT、置信度、哈希口径，加候选数那一档。
    Candidates {
        /// 哪个数据源（No-Intro / TOSEC / MAME / 中文离线源……）。
        source: String,
        /// 哪一份 DAT。
        dat: String,
        /// **置信度**（ADR-0002）。
        confidence: Confidence,
        /// 撞上时用的是**哪套哈希**（含头 / 去头 / 逐芯片）。
        ///
        /// 记的是**撞上时用的那套**而不是 DAT 自己声明的那套：前者才是
        /// 「工具凭什么这么认为」，后者是那份 DAT 的属性。
        convention: Convention,
        /// 候选数那一档。**这一支永远不是 [`Fanout::None`]**。
        fanout: Fanout,
    },
    /// **一条候选都没有**：识别结论，加那句「为什么没定下来」。
    Bare {
        /// 这一轮识别的结论。
        state: State,
        /// 「无判据」「跳过」的具体理由；未命中那一档往往没有。
        reason: Option<String>,
    },
}

impl Shape {
    /// 这一条落在哪个形状里。
    ///
    /// 取的是**第一条候选**——理由见模块文档：整批通过就是 `--pick 1`。
    #[must_use]
    pub fn of(item: &Item) -> Self {
        Self::of_candidates(&item.candidates).unwrap_or_else(|| Self::Bare {
            state: item.state,
            reason: item.reason.clone(),
        })
    }

    /// 一个变体的这几条候选落在哪个**有候选**的形状里；一条都没有是 `None`（那一支要结论与理由，候选说不出来）。
    ///
    /// 与 [`Self::of`] 同一处折：待确认屏上一批的形状、作品详情页上一个变体的判定依据，说的是同一个形状。
    #[must_use]
    pub fn of_candidates(candidates: &[Candidate]) -> Option<Self> {
        let lead = candidates.first()?;
        Some(Self::Candidates {
            source: lead.source.clone(),
            dat: lead.dat.clone(),
            confidence: lead.confidence,
            convention: lead.hashed_as,
            fanout: Fanout::of(candidates.len()),
        })
    }

    /// **判定依据那一句**（设计稿 `No-Intro / gb.dat / 含头 · CRC-32 与文件大小一致 · 1 个候选`）：形状的前半截
    /// （[`Self::label`]）、那条候选自己的依据、候选数，各段之间一个「 · 」。作品详情页识别依据那一面印它。
    #[must_use]
    pub fn basis(&self, evidence: &str, candidates: usize) -> String {
        format!("{} · {evidence} · {candidates} 个候选", self.label())
    }

    /// 这一条的依据形状正是这一个吗。
    ///
    /// **一个字符串都不分配**：选择器每帧都要拿它问一万八千遍
    /// （[`Filter::keeps`](super::Filter::keeps)）。
    #[must_use]
    pub fn holds(&self, item: &Item) -> bool {
        match (self, item.candidates.first()) {
            (
                Self::Candidates {
                    source,
                    dat,
                    confidence,
                    convention,
                    fanout,
                },
                Some(lead),
            ) => {
                lead.source == *source
                    && lead.dat == *dat
                    && lead.confidence == *confidence
                    && lead.hashed_as == *convention
                    && Fanout::of(item.candidates.len()) == *fanout
            }
            (Self::Bare { state, reason }, None) => item.state == *state && item.reason == *reason,
            _ => false,
        }
    }

    /// 这一批的候选数落在哪一档。
    #[must_use]
    pub fn fanout(&self) -> Fanout {
        match self {
            Self::Candidates { fanout, .. } => *fanout,
            Self::Bare { .. } => Fanout::None,
        }
    }

    /// 这一批标成**四档**里的哪一档（ADR-0002）。屏上那条色条画的就是它。
    #[must_use]
    pub fn tier(&self) -> Tier {
        match self {
            Self::Candidates { confidence, .. } => Tier::of(Some(*confidence)),
            Self::Bare { .. } => Tier::Unidentified,
        }
    }

    /// 屏上那一行的前半截：`MAME / gameboy.xml / 含头`。
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Candidates {
                source,
                dat,
                convention,
                ..
            } => format!("{source} / {dat} / {}", convention.label()),
            Self::Bare { state, .. } => format!("{BARE} · {}", state.label()),
        }
    }

    /// 折成**命令行上那串字**：`romcat triage --shape` 收的就是它。
    ///
    /// 屏上点一张卡片、命令行敲一条 `--shape`，选中的必须是同一批（ADR-0005）。
    /// 形状是个结构，命令行只收得下一串字——所以这一对折算（连同 [`Shape::parse`]）
    /// 就是那句话在字面上的落点，而**两边都只有这一处**。报告把每一批连它这串字一起
    /// 印出来（`triage::report`），人照着抄一行就是一条覆盖几千条的命令。
    ///
    /// 有候选的写成五段 `<源> / <DAT> / <置信度> / <哈希口径> / <候选数>`；
    /// 一条候选都没有的写成 `一条候选都没有 / <结论>`，有理由时理由跟在第三段。
    ///
    /// ## 这串字认得回来的前提：**源那一段里不许出现 `SEP`**
    ///
    /// [`Shape::parse`] 的段界靠两头：末三段是闭合词表，认不出就当场说不认得；
    /// **头一段没有这层兜底**。源名里一旦出现一个 ` / `，认回来的是另一个形状——源短了
    /// 一截、DAT 长了一截——它一条都选不中，而且**不报错**。所以这不是风格问题，是那条
    /// 认法成立的前提；`源那一段里不许出现分隔符` 那条测试把它钉在真的源名上（内置数据源
    /// 清单里那几个，加代码里写死的那四个）。DAT 与理由不受这条约束，它们自带 `/` 照样
    /// 认得回来——中间那一段是整取的。
    #[must_use]
    pub fn selector(&self) -> String {
        match self {
            Self::Candidates {
                source,
                dat,
                confidence,
                convention,
                fanout,
            } => [
                source.as_str(),
                dat.as_str(),
                confidence.label(),
                convention.label(),
                fanout.label(),
            ]
            .join(SEP),
            Self::Bare { state, reason } => match reason {
                Some(reason) => [BARE, state.label(), reason.as_str()].join(SEP),
                None => [BARE, state.label()].join(SEP),
            },
        }
    }

    /// 把 [`Shape::selector`] 折出来的那串字认回一个形状。
    ///
    /// ## 两头对着认，中间那段整个留给 DAT
    ///
    /// DAT 的名字与「为什么没定下来」那句理由**都可能自带 `/`**。认错一段的后果不是
    /// 报错——是**静悄悄选中另一批**，而人按下去的那一下就不是他看过的那一批。所以段界
    /// 不靠数分隔符：源取头一段，置信度 / 哈希口径 / 候选数取**末三段**（三个都是闭合的
    /// 词表，认不出就当场说不认得），剩下的中间原样拼回去当 DAT。理由那一支同理，
    /// 第三段往后整个是理由。
    ///
    /// 两头的空白先剪掉——那串字是从报告上抄来的，行尾多个空格是常事。代价是
    /// **理由恰好是空串**的那一批（`Some("")`）折出来的字末尾那个分隔符会被剪掉，
    /// 认回来时报「认不出结论」。眼下理由的几个产地一个都给不出空串，而这条路失手的
    /// 样子是**当场报错**、不是静悄悄选中别的一批，所以留着（挂单 Q179）。
    ///
    /// # Errors
    /// 段数不够，或者哪一段的词认不出来时，交回一句说得清哪儿不对、该怎么写的话。
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let parts: Vec<&str> = text.split(SEP).collect();
        if parts[0] == BARE {
            let label = parts.get(1).ok_or_else(|| {
                format!(
                    "「{text}」少了识别结论。一条候选都没有的那一批写成 \
                     `{BARE}{SEP}<结论>`，理由（有的话）跟在第三段。"
                )
            })?;
            let state = State::from_label(label).ok_or_else(|| {
                format!("认不出结论「{label}」。写 `命中` / `未命中` / `无判据` / `跳过` 之一。")
            })?;
            return Ok(Self::Bare {
                state,
                reason: (parts.len() > 2).then(|| parts[2..].join(SEP)),
            });
        }
        if parts.len() < 5 {
            return Err(format!(
                "「{text}」不是一个依据形状。有候选的那一批写成 \
                 `<源>{SEP}<DAT>{SEP}<置信度>{SEP}<哈希口径>{SEP}<候选数>`，\
                 一条候选都没有的写成 `{BARE}{SEP}<结论>`。\
                 `romcat triage list` 把每一批连它这串字一起印出来，照着抄。"
            ));
        }
        let tail = parts.len() - 3;
        let confidence = Confidence::from_label(parts[tail]).ok_or_else(|| {
            format!(
                "认不出置信度「{}」。写 `高置信` / `中置信` / `低置信` 之一。",
                parts[tail]
            )
        })?;
        let convention = Convention::from_label(parts[tail + 1]).ok_or_else(|| {
            format!(
                "认不出哈希口径「{}」。写 `含头` / `去头` / `逐芯片` 之一。",
                parts[tail + 1]
            )
        })?;
        let fanout = Fanout::from_label(parts[tail + 2]).ok_or_else(|| {
            format!(
                "认不出候选数那一档「{}」。写 `{}` 之一。",
                parts[tail + 2],
                Fanout::ALL
                    .iter()
                    .map(|one| one.label())
                    .collect::<Vec<_>>()
                    .join("` / `"),
            )
        })?;
        // **有候选的那一支永远不是「没有候选」那一档。** 放它过去的话选择器一条都选不中，
        // 而屏上根本折不出这串字——那只可能是手打错了，当场说清比静悄悄选中零条强。
        if fanout == Fanout::None {
            return Err(format!(
                "「{}」这一档只属于一条候选都没有的那一批，而这串字写着源与 DAT。\
                 那一批写成 `{BARE}{SEP}<结论>`。",
                Fanout::None.label(),
            ));
        }
        Ok(Self::Candidates {
            source: parts[0].to_string(),
            dat: parts[1..tail].join(SEP),
            confidence,
            convention,
            fanout,
        })
    }
}

/// **一批**：一组依据形状相同的待裁决变体。
///
/// 屏上常驻三样，这里给出前两样（[`Batch::count`] 与 [`Batch::why`]）；
/// 第三样是随机样本，走 [`sample`]——它换一组就变，不该钉在这个结构上。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    /// 这一批的**依据形状**。**它同时是选择器的值**：
    /// [`Filter::shape`](super::Filter::shape) 拿它选出同样这些条。
    pub shape: Shape,
    /// 多少条。
    pub count: u64,
    /// 这一批的**依据**里逐字一样的那一段；凑不出一句完整的话时是 `None`。
    ///
    /// **是原话不是概括**：整批通过时人验证的就是这句，编一句出来等于让人验证一句
    /// 没人写过的话。
    pub evidence: Option<String>,
}

impl Batch {
    /// **那句共同依据**：屏上常驻三样里的第二样。
    #[must_use]
    pub fn why(&self) -> String {
        let mut line = self.shape.label();
        match &self.shape {
            Shape::Candidates { fanout, .. } => {
                let _ = write!(line, " —— 都{}", fanout.sentence());
                if let Some(shared) = &self.evidence {
                    let _ = write!(line, "，依据都是「{shared}」");
                }
            }
            Shape::Bare { reason, .. } => {
                if let Some(reason) = reason {
                    let _ = write!(line, " —— {reason}");
                }
            }
        }
        line
    }

    /// 这一批**整批通过**说得成立吗。
    ///
    /// 「通过」就是采用第一条候选。一条候选都没有时它什么也没定下来（核心库排计划时
    /// 会当场挡下），多候选时它是替人瞎挑——两种都是假的「一键搞定」。
    /// **拒绝**倒是任何一批都说得出口。
    #[must_use]
    pub fn passable(&self) -> bool {
        self.shape.fanout().answerable()
    }

    /// 这一批标成四档里的哪一档。
    #[must_use]
    pub fn tier(&self) -> Tier {
        self.shape.tier()
    }
}

/// 一屏分批的**账**：分成几批、一共多少条、前几批可一次处理多少、按批答得了的有多少、
/// 有多个候选与没有候选的各多少。
///
/// 它**算在核心库里**：屏上那句「前 5 批可直接批量处理 12,223 条」是这一屏存在的理由本身
/// （18,241 条按 5 秒一条是 25 小时），而算它就是走一遍这几批。界面只负责把这几个数
/// 摆出来（ADR-0005）；库屏工序段裁决那一行「前 N 批可一次处理 N 个」说的也是它。
///
/// ## 「前几批」只数能整批通过的
///
/// 从前数的是次序上的头几批，而那几批在真库上全是「一条候选都没有」——于是屏上与工序段都说
/// 「可一次处理」一万多条，其实一条都通过不了。**能整批通过的不足那么多批时，前几批就只有那几批**，
/// 不拿通过不了的凑数。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Coverage {
    /// 一共分成几批。
    pub batches: usize,
    /// 一共多少条。
    pub total: u64,
    /// 「前几批」算了几批：只数**能整批通过**的（[`Batch::passable`]）。
    pub head_batches: usize,
    /// 前几批盖住多少条——一次按下去就处理得掉的那些。
    pub head: u64,
    /// 前几批之外还剩几批（能不能整批通过的都算）。
    pub rest_batches: usize,
    /// 前几批之外还剩多少条。
    pub rest: u64,
    /// 其中**按批答得了**的（只有一个候选的那些）有多少条。
    pub answerable: u64,
    /// 前几批之外，**同样只有一个候选**的还有几批。
    pub rest_answerable_batches: usize,
    /// 那几批一共多少条。
    pub rest_answerable: u64,
    /// **有多个候选**的有多少条：问的是「选哪个」，只能逐条选。
    pub multiple: u64,
    /// **没有候选**的有多少条：没有可供确认的候选，不能整批通过。
    pub bare: u64,
    /// 没有候选的那些按**识别结论**各多少条，与 [`State::ALL`] 同序（待确认屏虚线框里「其中未命中 N 个、无判据 N 个」）。
    pub bare_by_state: [u64; State::ALL.len()],
}

impl Coverage {
    /// 前几批盖住的占几成，写成百分数。
    #[must_use]
    pub fn share(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            self.head as f64 * 100.0 / self.total as f64
        }
    }
}

/// 数一遍这几批的账。`head` 是「前几批」最多算几批——只数能整批通过的，见 [`Coverage`]。
#[must_use]
pub fn coverage(batches: &[Batch], head: usize) -> Coverage {
    let passable: Vec<&Batch> = batches.iter().filter(|batch| batch.passable()).collect();
    let head_batches = head.min(passable.len());
    let head_count: u64 = passable
        .iter()
        .take(head_batches)
        .map(|batch| batch.count)
        .sum();
    let total: u64 = batches.iter().map(|batch| batch.count).sum();
    let answerable: u64 = passable.iter().map(|batch| batch.count).sum();
    // **没有候选只有一种认法**：`Shape::Bare` 那一支（`Shape::fanout` 交 `Fanout::None` 的也正是它）。
    let mut bare_by_state = [0u64; State::ALL.len()];
    for batch in batches {
        if let Shape::Bare { state, .. } = &batch.shape
            && let Some(at) = State::ALL.iter().position(|one| one == state)
        {
            bare_by_state[at] += batch.count;
        }
    }
    let bare: u64 = bare_by_state.iter().sum();
    Coverage {
        batches: batches.len(),
        total,
        head_batches,
        head: head_count,
        rest_batches: batches.len() - head_batches,
        rest: total - head_count,
        answerable,
        rest_answerable_batches: passable.len() - head_batches,
        rest_answerable: answerable - head_count,
        multiple: total - answerable - bare,
        bare,
        bare_by_state,
    }
}

/// **一级分批**：把这些条目按依据形状分成几十批，**能整批通过的排前面**，同一类里多的排前面。
///
/// 能整批通过的排前面：人打开这一屏先看见一次按得下去的那几批，「前几批」（[`coverage`]）也就是次序上的
/// 头几批。同数按形状定死顺序：同一份库跑两次，屏上那一列卡片得长得一模一样。
#[must_use]
pub fn batches(items: &[Item]) -> Vec<Batch> {
    let mut grouped: BTreeMap<Shape, Vec<&str>> = BTreeMap::new();
    for item in items {
        grouped
            .entry(Shape::of(item))
            .or_default()
            .push(item.candidates.first().map_or("", |lead| &lead.evidence));
    }
    let mut out: Vec<Batch> = grouped
        .into_iter()
        .map(|(shape, evidences)| Batch {
            count: evidences.len() as u64,
            evidence: common_evidence(evidences.into_iter()),
            shape,
        })
        .collect();
    out.sort_by(|a, b| {
        b.passable()
            .cmp(&a.passable())
            .then_with(|| b.count.cmp(&a.count))
            .then_with(|| a.shape.cmp(&b.shape))
    });
    out
}

/// 选中的这些按**四档**各有多少条。屏头上那几个数。
#[must_use]
pub fn by_tier(items: &[Item]) -> [(Tier, u64); Tier::ALL.len()] {
    let mut counts = [0u64; Tier::ALL.len()];
    for item in items {
        counts[tier_index(item.tier())] += 1;
    }
    let mut out = [(Tier::High, 0); Tier::ALL.len()];
    for (at, tier) in Tier::ALL.into_iter().enumerate() {
        out[at] = (tier, counts[at]);
    }
    out
}

/// 这一档在 [`Tier::ALL`] 里排第几。
fn tier_index(tier: Tier) -> usize {
    match tier {
        Tier::High => 0,
        Tier::Medium => 1,
        Tier::Low => 2,
        Tier::Unidentified => 3,
    }
}

/// **整批操作的作用范围**：一级的某一批，或者它下面按某个轴下钻出来的那一组。
///
/// 两层收在同一个类型里，因为「每一层都能整批过、整批拒」这句话要求两层走同一条路：
/// 各写一份的话，二级那一层迟早会漏掉某一条一级已经有的检查。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    /// 哪一批。
    pub shape: Shape,
    /// 再按哪个轴下钻到哪一组；`None` 就是整批。
    pub drill: Option<(Axis, String)>,
}

impl Scope {
    /// 整个这一批。
    #[must_use]
    pub fn whole(shape: Shape) -> Self {
        Self { shape, drill: None }
    }

    /// 这一批下面按这个轴下钻出来的那一组。
    #[must_use]
    pub fn under(shape: Shape, axis: Axis, label: &str) -> Self {
        Self {
            shape,
            drill: Some((axis, label.to_string())),
        }
    }

    /// 这一条在这个范围里吗。
    #[must_use]
    pub fn holds(&self, item: &Item) -> bool {
        if !self.shape.holds(item) {
            return false;
        }
        match &self.drill {
            None => true,
            Some((axis, label)) => axis.keys_of(item).iter().any(|key| key == label),
        }
    }

    /// 屏上写成什么。
    #[must_use]
    pub fn label(&self) -> String {
        match &self.drill {
            None => self.shape.label(),
            Some((axis, label)) => format!("{} · {} {label}", self.shape.label(), axis.label()),
        }
    }
}

/// **二级下钻**的结果：这一批按某个轴分成哪些组，以及数得对不对得上。
///
/// 三个数一起给，是因为**只有按目录那个轴分得干净**：一条只落一个目录，于是各组之和
/// 恰好是这一批的条数。另外两个轴一条能落进好几组（一个名字里能有好几个记号、一个变体
/// 能有好几条候选），也能一组都不落。不把这两件事说出口，屏上「1,204 ＋ 918 ＋ 576」
/// 加起来大过批的条数时，人只会以为工具算错了。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Drill {
    /// 分出来的组，多的排前面。
    pub rows: Vec<GroupRow>,
    /// 这个范围里一共多少条。
    pub total: u64,
    /// 一组都没落进的有几条。
    pub ungrouped: u64,
    /// 落进**不止一组**的有几条。
    pub overlapping: u64,
}

impl Drill {
    /// 各组的条数加起来正好是这一批吗。
    ///
    /// **按目录那个轴永远为真**，那正是它做默认二级分法的理由。
    #[must_use]
    pub fn adds_up(&self) -> bool {
        self.overlapping == 0
            && self.rows.iter().map(|row| row.count).sum::<u64>() + self.ungrouped == self.total
    }
}

/// 把这些条目按某个轴下钻一层。
#[must_use]
pub fn drill(members: &[&Item], axis: Axis) -> Drill {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut out = Drill {
        total: members.len() as u64,
        ..Drill::default()
    };
    for item in members {
        let keys = axis.keys_of(item);
        if keys.is_empty() {
            out.ungrouped += 1;
        } else if keys.len() > 1 {
            out.overlapping += 1;
        }
        for key in keys {
            *counts.entry(key).or_default() += 1;
        }
    }
    out.rows = counts
        .into_iter()
        .map(|(label, count)| GroupRow { label, count })
        .collect();
    out.rows
        .sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.cmp(&b.label)));
    out
}

/// 一部分裁成了什么。
///
/// 只有两档，因为**下钻那一层能落下的只有「这一部分整个通过」与「整个拒绝」**——
/// 「手工指定」天生是逐条的动作，它不作用于一整组。屏上那两颗按钮照稿写的是
/// 「通过这 N 条」「拒绝这 N 条」，说的就是这两档。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartKind {
    /// 整批通过：采用第一条候选。
    Passed,
    /// 整批拒绝：记成「我看过了，认不出」。
    Rejected,
}

impl PartKind {
    /// 屏上那一项旁边那枚标签上写什么。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Passed => "已通过",
            Self::Rejected => "已拒绝",
        }
    }

    /// 同一件事的**动词**：「这一部分通过了」那句话里的那两个字。
    ///
    /// 单给一支，不拿 [`Self::label`] 剥字头：剥出来的那一句从此跟着标签走
    /// ——把标签改成「通过了」，那句话当场变成「这一部分通过了了」。
    #[must_use]
    pub fn verb(self) -> &'static str {
        match self {
            Self::Passed => "通过",
            Self::Rejected => "拒绝",
        }
    }
}

/// **这一批下面已经就地裁完的一部分**：按某个轴下钻出来的那一组，整批落下过一次裁决。
///
/// 只记**下钻那一层**落下的。一级整批落下之后这一批整个从队列里消失，屏上没有「剩下的
/// 部分」可说；而下钻落下一部分之后这一批还在，屏上那一栏要同时说清「这一项处理过了」
/// 与「还剩这些」——那两句话都只有记着这一条才说得出。
///
/// 落下的仍旧是**一批裁决**（[`verdict::Batch`](crate::verdict::Batch)），不另造一套：
/// [`Part::batch`] 指的就是它，撤销照旧以它为粒度。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// 哪一批下面的哪一组。`drill` 必不为空——整批那一层不记（见上）。
    pub scope: Scope,
    /// 落下的时候这一组有多少条。
    ///
    /// **落下时记住，不事后再数**：这些条一落下就从队列里消失了，再数是零，
    /// 而屏上那一行要写的正是「这一项当时有多少条」。
    pub count: u64,
    /// 通过还是拒绝。
    pub kind: PartKind,
    /// 落成了第几**批裁决**（[`verdict::Batch::id`](crate::verdict::Batch::id)）。
    /// 那一批撤掉，这一条跟着作废（[`Parts::keep`]）。
    ///
    /// 叫 `batch` 不叫 `lot`：后者正是词表**批**那一条 `_Gate_` 里的「批号」，
    /// 而这个东西全仓早有名字——[`Applied::batch`](super::Applied::batch)、
    /// [`undo_batch`](super::undo_batch) 指的都是它。
    pub batch: i64,
}

/// 眼下这一屏上，各批已经就地裁完的那几部分。
///
/// **它不是第二份账**：条数、撤没撤都以沉淀库那一批裁决为准，这里存的是沉淀库答不出的
/// 那一半——**那一批裁决当初作用在哪一批的哪一组上**。沉淀库只记「落了哪些条」，
/// 折不回「按目录 · `GB/汉化/`」这一句。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parts {
    done: Vec<Part>,
    revision: u64,
}

impl Parts {
    /// 记下刚落下的这一部分。`scope` 不带下钻那一层时**一个字都不记**并交回 `false`
    /// ——整批那一层不归这里管。
    ///
    /// **同一组落下过两次就记两条，后一条不盖掉前一条**：头一趟里有几条落不下去
    /// （[`Plan::blocked`](super::Plan::blocked)）时那一组还剩着，人会再裁一次——
    /// 盖掉的话头一批的账就从这里消失了，而它在裁决记录里还在册，撤掉它屏上也不会有反应。
    /// 同一批裁决（`batch` 相同）重记才是覆盖：那是同一件事说了两遍。
    pub fn record(&mut self, scope: Scope, count: u64, kind: PartKind, batch: i64) -> bool {
        if scope.drill.is_none() {
            return false;
        }
        self.done.retain(|part| part.batch != batch);
        self.done.push(Part {
            scope,
            count,
            kind,
            batch,
        });
        self.revision += 1;
        true
    }

    /// 只留下 `live` 认的那几批裁决对应的部分。
    ///
    /// **撤销以一批裁决为粒度**：那一批撤掉，那些变体当场回到队列，这一项就不再是
    /// 「处理过的」——屏上那一行要重新变成点得动的，细分方式也跟着解开。
    pub fn keep(&mut self, live: impl Fn(i64) -> bool) {
        let 原先 = self.done.len();
        self.done.retain(|part| live(part.batch));
        if self.done.len() != 原先 {
            self.revision += 1;
        }
    }

    /// 换过几次样子。缓着细分那一栏的人拿它认「这一份还作数吗」。
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// 这一批的细分方式**锁在哪个轴上**；一部分都没处理过就是 `None`。
    ///
    /// 锁的理由是**数对不上**：已经按一个轴处理掉一部分之后，换个轴再切一刀，两套切法
    /// 会重叠——新那一栏里每一项含着多少条已经处理掉的，谁也说不出。
    #[must_use]
    pub fn locked_axis(&self, shape: &Shape) -> Option<Axis> {
        self.under(shape).next().map(|(axis, _, _)| axis)
    }

    /// 这一批已经就地处理掉多少条。
    #[must_use]
    pub fn done_in(&self, shape: &Shape) -> u64 {
        self.under(shape).map(|(_, _, part)| part.count).sum()
    }

    /// 这一批下面已经处理掉的那几部分，连它们各自落在哪个轴的哪一组上。
    fn under(&self, shape: &Shape) -> impl Iterator<Item = (Axis, &str, &Part)> {
        self.done.iter().filter_map(move |part| {
            if &part.scope.shape != shape {
                return None;
            }
            let (axis, label) = part.scope.drill.as_ref()?;
            Some((*axis, label.as_str(), part))
        })
    }
}

/// **细分那一栏该画什么**：这一批按某个轴切成哪几组、每一组多少条、占这一批几成、
/// 哪几组已经整批裁过了，以及这一批还剩多少条、细分方式锁没锁住。
///
/// 词表**批**那一条分开了两个词：切出来的那些叫**组**，其中**已经整批裁过**的那一组叫
/// **一部分**（[`Part`]）。这里的一行（[`Slice`]）两样都装得下——画的可能是一个还没裁的组，
/// 也可能是一个已经裁过的一部分，靠 [`Slice::done`] 分开。
///
/// 不直接拿 [`Drill`] 画，是因为**处理掉的那一项会从 [`Drill`] 里消失**：那些条一落下
/// 就退出了队列，再数一遍就少一项——而屏上那一项必须还在，标着「已通过」，不然人只会
/// 以为自己刚才什么也没做。「还剩多少」「换不换得了轴」出自同一处，各算各的迟早三个数
/// 对不上，而按钮上写的正是那个数。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Breakdown {
    /// 各项，多的排前面。
    pub rows: Vec<Slice>,
    /// 这一批**本来**有多少条：还剩的，加上**整组都已经退出队列**的那几部分。占比条的分母。
    ///
    /// **它不一定等于 `left + done`。** 一部分里有几条落不下去时
    /// （[`Plan::blocked`](super::Plan::blocked)，少见），那一组还在队列里
    /// ——它那几条已经算在 `left` 里，`done` 里也有它整份，两下相加就重复了。
    /// 这一格只加真的数不到的那一截，所以各项占比照旧正好凑成一整批。
    pub whole: u64,
    /// 还剩多少条没裁——**整批操作那两颗按钮上写的就是它**。
    pub left: u64,
    /// 人在这一批上已经就地裁掉多少条——**卡头那句「已处理 N 条」写的是它**。
    ///
    /// 数的是落下去的全部，不管那一组裁没裁干净；分母那一格（`whole`）另有算法，见上。
    pub done: u64,
    /// 细分方式锁在这个轴上（[`Parts::locked_axis`]）。
    pub locked: Option<Axis>,
    /// 这个轴上**一组都没落进**的有几条（[`Drill::ungrouped`] 原样带过来）。
    pub ungrouped: u64,
    /// 落进**不止一组**的有几条（[`Drill::overlapping`]）。
    pub overlapping: u64,
}

impl Breakdown {
    /// 各项加起来正好是这一批吗。**按目录那个轴永远为真**，那正是它做默认分法的理由。
    ///
    /// 与 [`Drill::adds_up`] 问的是同一件事，差别在**数的是哪一套账**：那一支数眼下还在
    /// 队列里的，这一支数屏上真摆着的那几项（含已经裁掉、`drill` 再也数不到的那几个）。
    /// 裁掉一部分之后拿那一支去说这句话，屏上的数与话里的数就对不上了。
    #[must_use]
    pub fn adds_up(&self) -> bool {
        self.overlapping == 0
            && self.rows.iter().map(|row| row.count).sum::<u64>() + self.ungrouped == self.whole
    }

    /// **加不加得起来要说出口**：各项加起来大过这一批时那句话；加得起来就是 `None`。
    ///
    /// 屏上「1,204 ＋ 918 ＋ 576」加起来大过这一批的条数时，人只会以为工具算错了
    /// ——只有按目录那个轴一条只落一个组，另两个轴一条能落进好几组、也能一组都不落。
    #[must_use]
    pub fn tally_note(&self) -> Option<String> {
        if self.adds_up() {
            return None;
        }
        Some(format!(
            "这个轴上一条能落进好几组，所以各项加起来 {} 大过这一批的 {} 条；\
             另有 {} 条一组都没落进。按目录那个轴是分得干净的。",
            crate::report::thousands(self.rows.iter().map(|row| row.count).sum::<u64>()),
            crate::report::thousands(self.whole),
            crate::report::thousands(self.ungrouped),
        ))
    }

    /// 换细分方式为什么被挡住，一句人话；没锁住就是 `None`。
    #[must_use]
    pub fn axis_refusal(&self) -> Option<String> {
        self.locked.map(axis_refusal)
    }

    /// 这一批已经处理掉一部分了吗——屏上那几处「剩余」的措辞由它定。
    #[must_use]
    pub fn partly_done(&self) -> bool {
        self.done > 0
    }
}

/// **换细分方式为什么被挡住**那一句：这一批已经按 `locked` 那个轴裁掉过一部分了。
///
/// 收成自由函数、而不是只长在 [`Breakdown`] 上，因为**两处都要说这一句**：屏上那一排
/// 底下常驻的那一行（走 [`Breakdown::axis_refusal`]），与真正挡住换轴的那道守卫拒下时
/// 交出来的那一句。ADR-0005 的「再修订」一节逐字要求这两句**只许有一处**——各写一份
/// 就回到了那条规矩本来要防的「两份迟早分叉」。
#[must_use]
pub fn axis_refusal(locked: Axis) -> String {
    format!(
        "已{}处理了一部分；换细分方式前，请先在裁决记录中撤销那几批",
        locked.label()
    )
}

/// 细分那一栏的一行。
///
/// 与 [`GroupRow`] 长得几乎一样（名字 ＋ 条数），**不复用它**：那一支数的是「眼下队列里
/// 按这个轴分出来的组」，一条裁掉的都不认识——它是 [`tally`](super::tally) 与报告那一路
/// 的类型，命令行印的表也是它。这一支要多说两件只有屏上才有的事（裁过没有、还剩多少），
/// 硬加进 `GroupRow` 的话，命令行那张表上会凭空多出两列永远是空的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slice {
    /// 这一项叫什么。**它同时是选择器的值**——[`Axis::filter`] 拿它折出选择器。
    pub label: String,
    /// 这一项一共多少条。整组都裁掉了的那几项记的是**落下时**的条数（[`Part::count`]）。
    pub count: u64,
    /// 这一项眼下队列里**还剩**多少条；整组裁干净了就是 0。
    ///
    /// **屏上那一项点不点得动看的是它，不是裁没裁过**：少见的情形下一部分里有几条落不下去
    /// （[`Plan::blocked`](super::Plan::blocked)），那一项裁过了却还剩着——它得还点得动，
    /// 点进去把剩下的几条再裁一次才算完。
    pub left: u64,
    /// 裁过没有、裁成什么；没裁过是 `None`。
    pub done: Option<PartKind>,
}

impl Slice {
    /// 这一项占整批的几成——屏上那条占比条的长短。单项永远在 0.0 到 1.0 之间。
    ///
    /// 分母是这一批**本来**有多少条（[`Breakdown::whole`]），不是眼下还剩多少：拿剩下的
    /// 当分母，裁掉一项之后余下那几项的条会一起变长，而它们一条都没变。
    ///
    /// ⚠️ **各项加起来只有按目录那个轴正好是一整批**：另两个轴上一条能落进好几组，
    /// 几条加起来会大过 1.0——那不是算错了，而是 [`Breakdown::tally_note`] 要说出口的那件事。
    #[must_use]
    pub fn share(&self, whole: u64) -> f64 {
        if whole == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            self.count as f64 / whole as f64
        }
    }
}

/// 把这一批眼下的下钻结果与**已经处理掉的那几部分**合成屏上那一栏。
///
/// `drilled` 数的是**整批**（[`Scope::whole`]）——下钻只收窄整批操作的作用范围，
/// 那一栏本身不该跟着只剩一行。
#[must_use]
pub fn breakdown(drilled: &Drill, parts: &Parts, shape: &Shape, axis: Axis) -> Breakdown {
    let mut rows: Vec<Slice> = drilled
        .rows
        .iter()
        .map(|row| Slice {
            label: row.label.clone(),
            count: row.count,
            left: row.count,
            done: None,
        })
        .collect();
    // 这一批这个轴上裁过的那几部分里，**整组都已经退出队列的**有多少条。
    //
    // 只数这些，是因为它们正是 `drilled` 再也数不到的那一截——占比的分母要的是
    // 「这一批本来多少条」，而 `drilled.total` 只剩眼下还在队列里的。少见的情形下一部分
    // 里会有几条落不下去（[`Plan::blocked`](super::Plan::blocked)），那一组于是还在
    // `drilled` 里：它那几条已经算进 `drilled.total` 了，整份再加一遍就是重复计数。
    let mut 退出队列的 = 0;
    for (处理时的轴, label, part) in parts.under(shape) {
        // 锁着轴的时候这一条永远成立；换轴被挡住之前落下的那几部分不混进别的轴。
        if 处理时的轴 != axis {
            continue;
        }
        if let Some(at) = rows.iter().position(|row| row.label == label) {
            // 这一组还在队列里，也就是它没裁干净。标上裁过，条数照旧是眼下还剩的那些
            // ——屏上那一项于是还点得动，剩下的几条再裁一次就完了。
            rows[at].done = Some(part.kind);
        } else {
            退出队列的 += part.count;
            rows.push(Slice {
                label: label.to_string(),
                count: part.count,
                left: 0,
                done: Some(part.kind),
            });
        }
    }
    rows.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.cmp(&b.label)));
    Breakdown {
        rows,
        whole: drilled.total + 退出队列的,
        left: drilled.total,
        done: parts.done_in(shape),
        locked: parts.locked_axis(shape),
        ungrouped: drilled.ungrouped,
        overlapping: drilled.overlapping,
    }
}

/// **一条随机样本**：屏上常驻三样里的第三样。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    /// 变体的键（**根名 + 相对路径**）。
    pub key: String,
    /// 文件名。
    pub name: String,
    /// 它落在哪个目录下。
    pub directory: String,
    /// **第一条候选**指着哪个条目——整批通过采用的正是它。一条都没有时是 `None`。
    pub candidate: Option<String>,
}

/// 从这一批里抽 `want` 条样本。**换一组样本就换个 `seed`**。
///
/// ## 「换一组」为什么保证真的换了一组
///
/// 抽的是等差数列 `seed + i·步长`（模条数），步长与条数**互质**。互质保证这个数列
/// 走遍全部条目而不重复，于是它等价于某个排列上的一段**连续窗口**——两段等长的连续
/// 窗口相等当且仅当起点相同。起点是 `seed % 条数`，于是：
///
/// - **相邻两次一定不同**。界面上「换一组样本」把 `seed` 加一，而条数大过 `want` 时
///   `seed` 与 `seed + 1` 的起点必不相同——这正是那个按钮该保证的事。
/// - **按满一圈会转回原处**，那是应该的：批里就那么多条，走遍了就该重来。
///   两个相差恰好是条数倍数的 `seed` 给出同一组，不是缺陷。
///
/// 拿排列而不是直接取连续几条：条目按键排序，连着取五条全在同一个目录里，那不是样本。
/// **批小到只有两三条时步长退成 1**（见 `stride_for`），那时抽的就是连着的几条——
/// 一共就那么几条，散不散得开没有区别。
#[must_use]
pub fn sample(members: &[&Item], seed: u64, want: usize) -> Vec<Sample> {
    let count = members.len();
    if count == 0 || want == 0 {
        return Vec::new();
    }
    let take = want.min(count);
    let stride = stride_for(count);
    let start = usize::try_from(seed % count as u64).unwrap_or(0);
    (0..take)
        .map(|nth| {
            let at = (start + nth.wrapping_mul(stride)) % count;
            let item = members[at];
            Sample {
                key: item.variant.key.clone(),
                name: item.name().to_string(),
                directory: item.directory().to_string(),
                candidate: item.candidates.first().map(|lead| lead.game.clone()),
            }
        })
        .collect()
}

/// 抽样的步长：与 `count` 互质，且尽量离 `count` 的黄金分割近——那样连着抽出来的几条
/// 在原次序里散得最开。
fn stride_for(count: usize) -> usize {
    if count <= 2 {
        return 1;
    }
    #[allow(clippy::cast_precision_loss, clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let mut stride = ((count as f64) * 0.618_033_988_749_9) as usize;
    stride = stride.max(1);
    while gcd(stride, count) != 1 {
        stride += 1;
        if stride >= count {
            return 1;
        }
    }
    stride
}

/// 最大公约数。
fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// 这一批的**依据**里逐字一样的那一段。
///
/// 取最长公共前缀，再退到最后一个句读上——半句话不如不说。退完短过四个字就一个字
/// 都不说：那时它已经是「CRC-32」这种残片，写在卡片上只会占地方。
fn common_evidence<'a>(mut texts: impl Iterator<Item = &'a str>) -> Option<String> {
    let first = texts.next()?;
    if first.is_empty() {
        return None;
    }
    let mut end = first.len();
    for text in texts {
        end = end.min(shared_prefix(first, text));
        if end == 0 {
            return None;
        }
    }
    while end > 0 && !first.is_char_boundary(end) {
        end -= 1;
    }
    let head = &first[..end];
    let cut = if head.len() == first.len() {
        head
    } else {
        match head.rfind(['；', '。', '，', ';', ',']) {
            Some(at) => &head[..at],
            None => head,
        }
    };
    let cut = cut.trim();
    (cut.chars().count() >= 4).then(|| cut.to_string())
}

/// 两串逐字节一样的那一段有多长。
fn shared_prefix(left: &str, right: &str) -> usize {
    left.as_bytes()
        .iter()
        .zip(right.as_bytes())
        .take_while(|(a, b)| a == b)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Candidate, VariantRow};

    fn 条目(key: &str, candidates: Vec<Candidate>) -> Item {
        Item {
            variant: VariantRow {
                key: key.to_string(),
                platform: Some("FC".to_string()),
                rule: String::new(),
                main_key: key.to_string(),
                files: 1,
                bytes: 1024,
                unreadable_files: 0,
                manual: false,
                work_id: None,
                release_id: None,
            },
            state: if candidates.is_empty() {
                State::Unmatched
            } else {
                State::Matched
            },
            reason: None,
            candidates,
            print: None,
        }
    }

    fn 候选(source: &str, dat: &str, evidence: &str) -> Candidate {
        Candidate {
            member_key: String::new(),
            inner: String::new(),
            confidence: Confidence::Medium,
            accepted: false,
            source: source.to_string(),
            dat: dat.to_string(),
            platform: "FC".to_string(),
            game: format!("{dat} 里的某条"),
            rom: "rom.nes".to_string(),
            hashed_as: Convention::AsIs,
            dat_convention: Convention::AsIs,
            evidence: evidence.to_string(),
            chinese: None,
            serial: None,
            release_id: None,
        }
    }

    fn 一批(n: usize) -> Vec<Item> {
        (0..n)
            .map(|at| {
                条目(
                    &format!("库/FC/目录{}/游戏{at:04}.zip", at % 3),
                    vec![候选(
                        "MAME",
                        "nes.xml",
                        &format!("名字一字不差 + 平台对得上；序号 {at}"),
                    )],
                )
            })
            .collect()
    }

    #[test]
    fn 各批条数加起来就是队列的条数() {
        // 屏上「前 5 批盖住多少」只在这一条成立时算得出来。
        let mut items = 一批(30);
        items.push(条目("库/FC/别的.zip", vec![]));
        items.push(条目(
            "库/GBA/另一个源.zip",
            vec![候选(
                "中文离线源",
                "dump-2026-09-01",
                "正题模糊匹配上的中文名",
            )],
        ));
        let batches = batches(&items);
        assert_eq!(
            batches.iter().map(|batch| batch.count).sum::<u64>(),
            items.len() as u64,
        );
        assert_eq!(batches.len(), 3, "三种依据形状该分成三批");
        assert_eq!(batches[0].count, 30, "多的排前面");
        // 每一条都恰好落进一批——形状是选择器的值，选中的必须与数出来的是同一批。
        for item in &items {
            let 命中 = batches
                .iter()
                .filter(|batch| batch.shape.holds(item))
                .count();
            assert_eq!(命中, 1, "{} 落进了 {命中} 批", item.variant.key);
        }
    }

    #[test]
    fn 前几批盖住多少是核心库算出来的() {
        // 屏上那句「前 N 批盖住多少」是这一屏存在的理由本身，不该由界面自己去 sum
        // （ADR-0005）。
        let mut items = 一批(30);
        items.extend((0..7).map(|at| 条目(&format!("库/FC/光秃{at}.zip"), vec![])));
        let batches = batches(&items);
        let 账 = coverage(&batches, 1);
        assert_eq!(账.batches, 2);
        assert_eq!(账.total, 37);
        assert_eq!((账.head_batches, 账.head), (1, 30));
        assert_eq!((账.rest_batches, 账.rest), (1, 7));
        assert_eq!(账.answerable, 30, "按批答得了的只有单候选那一批");
        assert_eq!((账.multiple, 账.bare), (0, 7));
        assert!((账.share() - 30.0 * 100.0 / 37.0).abs() < 1e-9);
    }

    /// 四种形状各一批：没有候选的最多（30 条），两批只有一个候选（8 条、5 条），一批有多个候选（4 条）。
    fn 四种形状() -> Vec<Item> {
        let mut items: Vec<Item> = (0..30)
            .map(|at| 条目(&format!("库/FC/光秃{at}.zip"), vec![]))
            .collect();
        items.extend((0..8).map(|at| {
            条目(
                &format!("库/FC/单候选{at}.zip"),
                vec![候选("MAME", "nes.xml", "名字一字不差")],
            )
        }));
        items.extend((0..5).map(|at| {
            条目(
                &format!("库/GBA/单候选{at}.zip"),
                vec![候选("中文离线源", "dump-2026-09-01", "正题模糊匹配")],
            )
        }));
        items.extend((0..4).map(|at| {
            条目(
                &format!("库/FC/多候选{at}.zip"),
                vec![
                    候选("No-Intro", "nes.dat", "CRC-32 加大小撞上"),
                    候选("TOSEC", "nes.dat", "CRC-32 加大小撞上"),
                ],
            )
        }));
        items
    }

    #[test]
    fn 能整批通过的批排在前面_同一类里多的在前() {
        // 从前只按条数排：真库上头几批全是「一条候选都没有」，于是屏上「前 5 批」、库屏工序段
        // 「前 5 批可一次处理 N 个」说的都是一条都通过不了的那几批。
        let batches = batches(&四种形状());
        let 次序: Vec<(bool, u64)> = batches
            .iter()
            .map(|batch| (batch.passable(), batch.count))
            .collect();
        assert_eq!(
            次序,
            vec![(true, 8), (true, 5), (false, 30), (false, 4)],
            "能整批通过的排前面，同一类里多的在前",
        );
    }

    #[test]
    fn 前几批只数能整批通过的_另有几批同样只有一个候选() {
        let batches = batches(&四种形状());
        let 账 = coverage(&batches, 1);
        assert_eq!((账.batches, 账.total), (4, 47));
        assert_eq!(
            (账.head_batches, 账.head),
            (1, 8),
            "前 1 批是能整批通过的那 8 条"
        );
        assert_eq!(
            (账.rest_answerable_batches, 账.rest_answerable),
            (1, 5),
            "「另有 1 批（5 条）同样只有一个候选」",
        );
        assert_eq!(账.answerable, 13);
        assert_eq!(账.multiple, 4, "有多个候选的条数");
        assert_eq!(账.bare, 30, "没有候选的条数");
        // 没有候选的那些按识别结论各多少（待确认屏虚线框里「其中未命中 N 个、无判据 N 个」）。
        let 按结论: Vec<(State, u64)> = State::ALL.into_iter().zip(账.bare_by_state).collect();
        assert_eq!(
            按结论,
            vec![
                (State::Matched, 0),
                (State::Unmatched, 30),
                (State::NoEvidence, 0),
                (State::Skipped, 0),
            ],
        );
        assert_eq!((账.rest_batches, 账.rest), (3, 39));

        // 能整批通过的不足 N 批：前 N 批只数得出那几批，**不拿通过不了的凑数**。
        let 账 = coverage(&batches, 5);
        assert_eq!((账.head_batches, 账.head), (2, 13));
        assert_eq!((账.rest_answerable_batches, 账.rest_answerable), (0, 0));
        assert_eq!((账.rest_batches, 账.rest), (2, 34));

        // 一批能整批通过的都没有：前几批是零批零条。
        let 光秃 = super::batches(&四种形状()[..30]);
        let 账 = coverage(&光秃, 5);
        assert_eq!((账.head_batches, 账.head), (0, 0));
        assert_eq!((账.bare, 账.total), (30, 30));
    }

    #[test]
    fn 共同依据是原话不是概括() {
        let batches = batches(&一批(30));
        let 依据 = batches[0]
            .evidence
            .as_deref()
            .expect("该数得出共同的那一段");
        assert_eq!(依据, "名字一字不差 + 平台对得上", "{依据}");
        assert!(
            batches[0].why().contains("MAME / nes.xml / 含头"),
            "{}",
            batches[0].why(),
        );
        assert!(
            batches[0].why().contains("都只有一个候选"),
            "{}",
            batches[0].why()
        );
    }

    #[test]
    fn 一条候选都没有的那一批说的是为什么没定下来() {
        let mut 无判据 = 条目("库/FC/穿不透.zip", vec![]);
        无判据.state = State::NoEvidence;
        无判据.reason = Some("容器穿不透：格式不认".to_string());
        let batches = batches(&[无判据]);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].tier(), Tier::Unidentified);
        assert!(
            !batches[0].passable(),
            "一条候选都没有却说得出「整批通过」，那是假的一键搞定",
        );
        assert!(
            batches[0].why().contains("容器穿不透"),
            "{}",
            batches[0].why()
        );
    }

    #[test]
    fn 多候选那几档不给整批通过() {
        let 多 = 条目(
            "库/FC/多候选.zip",
            vec![
                候选("No-Intro", "nes.dat", "CRC-32 加大小撞上"),
                候选("TOSEC", "nes.dat", "CRC-32 加大小撞上"),
            ],
        );
        let batches = batches(&[多]);
        assert_eq!(batches[0].shape.fanout(), Fanout::Few);
        assert!(
            !batches[0].passable(),
            "多候选问的是「选哪个」，一次按下去等于替人挑了个没看过的答案",
        );
    }

    #[test]
    fn 按目录下钻的条数加起来等于它所属的一级() {
        let items = 一批(30);
        let members: Vec<&Item> = items.iter().collect();
        let drilled = drill(&members, Axis::Directory);
        assert!(drilled.adds_up(), "{drilled:?}");
        assert_eq!(
            drilled.rows.iter().map(|row| row.count).sum::<u64>(),
            30,
            "按目录一条只落一个组，各组之和就该是这一批",
        );
        assert_eq!(drilled.rows.len(), 3);
    }

    /// 这一批眼下的下钻结果，连它的依据形状。
    fn 一批加它的细分(n: usize) -> (Vec<Item>, Shape) {
        let items = 一批(n);
        let shape = Shape::of(&items[0]);
        (items, shape)
    }

    /// 眼下队列里还剩 `members` 这些条时，这一批在这个轴上的下钻。
    fn 下钻(items: &[Item], axis: Axis) -> Drill {
        let members: Vec<&Item> = items.iter().collect();
        drill(&members, axis)
    }

    #[test]
    fn 细分每一项都说得出占整批的几成() {
        // 屏上那条占比条的长短就是它。一项都没处理掉时，各项加起来正好是整批。
        let (items, shape) = 一批加它的细分(30);
        let 细分 = breakdown(&下钻(&items, Axis::Directory), &Parts::default(), &shape, Axis::Directory);
        assert_eq!((细分.whole, 细分.left, 细分.done), (30, 30, 0));
        assert!(细分.locked.is_none());
        let 加起来: f64 = 细分.rows.iter().map(|row| row.share(细分.whole)).sum();
        assert!((加起来 - 1.0).abs() < 1e-9, "各项占比加起来不是一整批：{细分:?}");
        assert!(细分.rows.iter().all(|row| row.done.is_none()));
    }

    #[test]
    fn 处理掉一项之后那一项还在栏上标着已通过_占比一条都没变() {
        // 那些条一落下就退出队列，`drill` 再数就少一项——而屏上那一项必须还在，
        // 不然人只会以为自己刚才什么也没做。占比条的分母是**本来**多少条，所以
        // 余下那几项的条一点都不该变长。
        let (items, shape) = 一批加它的细分(30);
        let 原先 = breakdown(&下钻(&items, Axis::Directory), &Parts::default(), &shape, Axis::Directory);
        let 那一项 = 原先.rows[0].clone();

        // 那一组落下之后队列里只剩别的组。
        let 剩下: Vec<Item> = items
            .iter()
            .filter(|item| item.directory() != 那一项.label)
            .cloned()
            .collect();
        let mut parts = Parts::default();
        assert!(parts.record(
            Scope::under(shape.clone(), Axis::Directory, &那一项.label),
            那一项.count,
            PartKind::Passed,
            7,
        ));
        let 之后 = breakdown(&下钻(&剩下, Axis::Directory), &parts, &shape, Axis::Directory);

        assert_eq!(之后.rows.len(), 原先.rows.len(), "处理掉的那一项从栏上消失了");
        let 标着 = 之后
            .rows
            .iter()
            .find(|row| row.label == 那一项.label)
            .expect("处理掉的那一项该还在栏上");
        assert_eq!(标着.done, Some(PartKind::Passed));
        assert_eq!(标着.count, 那一项.count, "标着的那一项该记落下时的条数");
        assert_eq!(标着.left, 0, "整组裁干净了，那一项该一条都不剩");
        assert_eq!(标着.done.map(PartKind::label), Some("已通过"));
        assert!(
            之后.rows.iter().filter(|row| row.done.is_none()).all(|row| row.left == row.count),
            "没裁过的那几项，还剩的就该是它全部：{之后:?}",
        );
        assert_eq!((之后.whole, 之后.left, 之后.done), (30, 30 - 那一项.count, 那一项.count));
        for 这一行 in &之后.rows {
            let 原来 = 原先
                .rows
                .iter()
                .find(|row| row.label == 这一行.label)
                .expect("项没变");
            assert!(
                (这一行.share(之后.whole) - 原来.share(原先.whole)).abs() < 1e-9,
                "处理掉一项之后别的项占比跟着变了：{这一行:?}",
            );
        }
    }

    #[test]
    fn 处理过一部分之后细分方式锁住_撤掉那一批就解开() {
        // 两套切法会重叠，数就对不上了——挡住，并说得出为什么。
        let (items, shape) = 一批加它的细分(30);
        let 那一项 = 下钻(&items, Axis::Directory).rows[0].clone();
        let mut parts = Parts::default();
        parts.record(
            Scope::under(shape.clone(), Axis::Directory, &那一项.label),
            那一项.count,
            PartKind::Rejected,
            7,
        );
        assert_eq!(parts.locked_axis(&shape), Some(Axis::Directory));
        let 细分 = breakdown(&下钻(&items, Axis::Directory), &parts, &shape, Axis::Directory);
        assert!(细分.partly_done());
        assert_eq!(
            细分.axis_refusal().as_deref(),
            Some("已按目录处理了一部分；换细分方式前，请先在裁决记录中撤销那几批"),
        );

        // 别的批一个字都不锁。
        let 别的批 = Shape::Bare {
            state: State::Unmatched,
            reason: None,
        };
        assert_eq!(parts.locked_axis(&别的批), None);

        // 那一批裁决撤掉，这一项就不再是处理过的。
        parts.keep(|batch| batch != 7);
        assert_eq!(parts.locked_axis(&shape), None);
        let 撤完 = breakdown(&下钻(&items, Axis::Directory), &parts, &shape, Axis::Directory);
        assert!(!撤完.partly_done());
        assert_eq!(撤完.axis_refusal(), None);
        assert!(撤完.rows.iter().all(|row| row.done.is_none()));
    }

    #[test]
    fn 整批那一层不记成一部分() {
        // 一级整批落下之后这一批整个从队列里消失，屏上没有「剩下的部分」可说。
        let (items, shape) = 一批加它的细分(30);
        let mut parts = Parts::default();
        assert!(!parts.record(Scope::whole(shape.clone()), 30, PartKind::Passed, 7));
        assert_eq!(parts.done_in(&shape), 0);
        assert_eq!(parts.locked_axis(&shape), None);
        let _ = items;
    }

    #[test]
    fn 换一组样本每次不同但都在批内() {
        let items = 一批(30);
        let members: Vec<&Item> = items.iter().collect();
        let keys: Vec<&str> = items.iter().map(|item| item.variant.key.as_str()).collect();
        let mut 上一组 = sample(&members, 0, 5);
        assert_eq!(上一组.len(), 5);
        for seed in 1..30u64 {
            let 这一组 = sample(&members, seed, 5);
            assert_ne!(这一组, 上一组, "第 {seed} 次换样本换出了同一组");
            assert!(
                这一组.iter().all(|one| keys.contains(&one.key.as_str())),
                "样本跑到批外面去了",
            );
            上一组 = 这一组;
        }
        // 抽的不是连着的五条——条目按键排序，连着五条全在同一个目录里那不是样本。
        let 目录: std::collections::BTreeSet<&str> =
            上一组.iter().map(|one| one.directory.as_str()).collect();
        assert!(目录.len() > 1, "五条样本全落在同一个目录里");
    }

    #[test]
    fn 依据形状与命令行那串字来回折得动() {
        // 屏上点一张卡片、命令行敲一条 `--shape`，选中的必须是同一批（ADR-0005）——
        // 而两边之间只有这一串字。折过去再认回来不是同一个形状，那句话当场就是假的。
        let items = {
            let mut items = 一批(30);
            items.push(条目("库/FC/光秃.zip", vec![]));
            let mut 穿不透 = 条目("库/FC/穿不透.zip", vec![]);
            穿不透.state = State::NoEvidence;
            穿不透.reason = Some("容器穿不透：格式不认".to_string());
            items.push(穿不透);
            items.push(条目(
                "库/FC/多候选.zip",
                vec![
                    候选("No-Intro", "nes.dat", "CRC-32 加大小撞上"),
                    候选("TOSEC", "nes.dat", "CRC-32 加大小撞上"),
                ],
            ));
            items
        };
        let batches = batches(&items);
        assert_eq!(batches.len(), 4, "四种依据形状");
        for batch in &batches {
            let 那串字 = batch.shape.selector();
            let 认回来 = Shape::parse(&那串字).unwrap_or_else(|why| panic!("{那串字}：{why}"));
            assert_eq!(认回来, batch.shape, "{那串字}");
            // 真正要的不是结构相等，是**选中同样这些条**。
            let 原来选中: Vec<&str> = items
                .iter()
                .filter(|item| batch.shape.holds(item))
                .map(|item| item.variant.key.as_str())
                .collect();
            let 认回来选中: Vec<&str> = items
                .iter()
                .filter(|item| 认回来.holds(item))
                .map(|item| item.variant.key.as_str())
                .collect();
            assert_eq!(原来选中.len() as u64, batch.count, "{那串字}");
            assert_eq!(原来选中, 认回来选中, "{那串字}");
        }
    }

    #[test]
    fn 名字里自带斜杠的照样认得回来() {
        // 段界不靠数分隔符：DAT 的名字与那句理由都可能自带 `/`，而认错一段的后果
        // 是**静悄悄选中另一批**。
        let 有斜杠 = Shape::Candidates {
            source: "MAME".to_string(),
            dat: "Sony - PlayStation / PSX (Aftermarket)".to_string(),
            confidence: Confidence::Medium,
            convention: Convention::PerChip,
            fanout: Fanout::Several,
        };
        assert_eq!(Shape::parse(&有斜杠.selector()), Ok(有斜杠));
        let 理由带斜杠 = Shape::Bare {
            state: State::NoEvidence,
            reason: Some("容器穿不透：7z / rar 都解不开".to_string()),
        };
        assert_eq!(Shape::parse(&理由带斜杠.selector()), Ok(理由带斜杠));
    }

    #[test]
    fn 源那一段里不许出现分隔符() {
        // **段界只有头一段没有兜底。** 末三段是闭合词表，认不出就当场说不认得；源名里
        // 一旦出现一个 ` / `，`Shape::parse` 认回来的是另一个形状（源短一截、DAT 长一截），
        // 它一条都选不中而且**不报错**——正是这一票最该防的那种失手。所以把这条前提钉在
        // **真的源名**上：内置数据源清单里那几个，加代码里写死的那四个。
        let mut 全部: Vec<String> = crate::dat::registry::Registry::builtin()
            .sources()
            .iter()
            .map(|one| one.name.clone())
            .collect();
        assert!(全部.len() >= 4, "内置清单里该有好几个源：{全部:?}");
        全部.extend(
            [
                crate::identify::fuzzy::SOURCE,
                crate::identify::model::SOURCE,
                crate::identify::switch::SOURCE_TITLEDB,
                crate::identify::switch::SOURCE_CONTAINER,
            ]
            .map(ToString::to_string),
        );
        for name in 全部 {
            assert!(
                !name.contains(SEP),
                "源「{name}」里有分隔符，它那一批折出来的 `--shape` 认回去会选中零条",
            );
        }
    }

    #[test]
    fn 认不出的那串字当场说清哪儿不对() {
        // 静悄悄选中零条是最坏的一种：人会以为这一批真的空了。
        let 说了什么 = |text: &str| Shape::parse(text).expect_err("这串字不该认得下来");
        assert!(说了什么("MAME / nes.xml / 含头").contains("不是一个依据形状"));
        assert!(说了什么("MAME / nes.xml / 很确信 / 含头 / 1 个候选").contains("置信度"));
        assert!(说了什么("MAME / nes.xml / 中置信 / 原样 / 1 个候选").contains("哈希口径"));
        assert!(说了什么("MAME / nes.xml / 中置信 / 含头 / 三个候选").contains("候选数"));
        assert!(说了什么("一条候选都没有").contains("少了识别结论"));
        assert!(说了什么("一条候选都没有 / 说不清").contains("认不出结论"));
        // 「没有候选」那一档只属于另一支：放它过去等于选中零条。
        assert!(说了什么("MAME / nes.xml / 中置信 / 含头 / 没有候选").contains("一条候选都没有"),);
    }

    #[test]
    fn 只剩一条时抽得出来也不假装换得动() {
        let items = 一批(1);
        let members: Vec<&Item> = items.iter().collect();
        assert_eq!(sample(&members, 0, 5).len(), 1);
        assert_eq!(sample(&members, 7, 5), sample(&members, 0, 5));
        assert!(sample(&[], 0, 5).is_empty());
    }
}
