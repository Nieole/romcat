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
//! 两者在按下「整批通过」那一刻一一对应：这一批变体落成那一批裁决。词表眼下两个都
//! 没收（挂单 Q77、Q80）。
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

use crate::catalog::State;
use crate::catalog::identify::{Confidence, Tier};
use crate::dat::Convention;

use super::{Axis, GroupRow, Item};

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

    /// 屏上写成什么。前面接着一个「都」字：「都只有一个候选」。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "没有候选",
            Self::One => "只有一个候选",
            Self::Few => "有 2–3 个候选",
            Self::Several => "有 4–10 个候选",
            Self::Many => "有 10 个以上候选",
        }
    }

    /// 这一档按批回答得了吗。
    ///
    /// **只有单候选那一档算数**：多候选问的是「选哪个」，同一批里各人的候选不是同一部
    /// 游戏，一次按下去等于替几千条各挑了一个没看过的答案。它们走逐条键盘流。
    #[must_use]
    pub fn answerable(self) -> bool {
        self == Self::One
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
        match item.candidates.first() {
            Some(lead) => Self::Candidates {
                source: lead.source.clone(),
                dat: lead.dat.clone(),
                confidence: lead.confidence,
                convention: lead.hashed_as,
                fanout: Fanout::of(item.candidates.len()),
            },
            None => Self::Bare {
                state: item.state,
                reason: item.reason.clone(),
            },
        }
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
            (Self::Bare { state, reason }, None) => {
                item.state == *state && item.reason == *reason
            }
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
            Self::Bare { state, .. } => format!("一条候选都没有 · {}", state.label()),
        }
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
                let _ = write!(line, " —— 都{}", fanout.label());
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

/// 一屏分批的**账**：分成几批、一共多少条、前几批盖住多少、按批答得了的有多少。
///
/// 它**算在核心库里**：屏上那句「前 5 批盖住 12,223 条」是这一屏存在的理由本身
/// （18,241 条按 5 秒一条是 25 小时），而算它就是走一遍这几批。界面只负责把这几个数
/// 摆成一句话（ADR-0005）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Coverage {
    /// 一共分成几批。
    pub batches: usize,
    /// 一共多少条。
    pub total: u64,
    /// 前几批算「前几批」。
    pub head_batches: usize,
    /// 前几批盖住多少条。
    pub head: u64,
    /// 前几批之外还剩几批。
    pub rest_batches: usize,
    /// 前几批之外还剩多少条。
    pub rest: u64,
    /// 其中**按批答得了**的（只有一个候选的那些）有多少条。
    pub answerable: u64,
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

/// 数一遍这几批的账。`head` 是「前几批」算几批。
#[must_use]
pub fn coverage(batches: &[Batch], head: usize) -> Coverage {
    let head_batches = head.min(batches.len());
    Coverage {
        batches: batches.len(),
        total: batches.iter().map(|batch| batch.count).sum(),
        head_batches,
        head: batches
            .iter()
            .take(head_batches)
            .map(|batch| batch.count)
            .sum(),
        rest_batches: batches.len() - head_batches,
        rest: batches
            .iter()
            .skip(head_batches)
            .map(|batch| batch.count)
            .sum(),
        answerable: batches
            .iter()
            .filter(|batch| batch.passable())
            .map(|batch| batch.count)
            .sum(),
    }
}

/// **一级分批**：把这些条目按依据形状分成几十批，多的排前面。
///
/// 同数按形状定死顺序：同一份库跑两次，屏上那一列卡片得长得一模一样。
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
    out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.shape.cmp(&b.shape)));
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
            vec![候选("中文离线源", "dump-2026-09-01", "正题模糊匹配上的中文名")],
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
        assert!((账.share() - 30.0 * 100.0 / 37.0).abs() < 1e-9);
    }

    #[test]
    fn 共同依据是原话不是概括() {
        let batches = batches(&一批(30));
        let 依据 = batches[0].evidence.as_deref().expect("该数得出共同的那一段");
        assert_eq!(依据, "名字一字不差 + 平台对得上", "{依据}");
        assert!(
            batches[0].why().contains("MAME / nes.xml / 含头"),
            "{}",
            batches[0].why(),
        );
        assert!(batches[0].why().contains("都只有一个候选"), "{}", batches[0].why());
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
        assert!(batches[0].why().contains("容器穿不透"), "{}", batches[0].why());
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
        let 目录: std::collections::BTreeSet<&str> = 上一组
            .iter()
            .map(|one| one.directory.as_str())
            .collect();
        assert!(目录.len() > 1, "五条样本全落在同一个目录里");
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
