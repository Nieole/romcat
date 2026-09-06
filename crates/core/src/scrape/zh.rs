//! **中文离线源**：把变体的文件名撞进中文离线数据源，取一个**有出处的中文名**；
//! 撞上的那条条目里属于**作品**的那几样，挂到作品锚点上。
//!
//! ## 它补的是标题集合里最大的那个窟窿
//!
//! 票 15 实测：6,501 个作品有中文显示标题，其中 **6,374 个是「无人背书的中文文件名」**
//! ——盘上那个文件恰好叫这个名字，谁也没为它背书，所以是**别名**、**低置信**
//! （`title::classify` 那一档）。这个源做的就是把其中一部分变成**有出处**的：
//! 同一串中文名，但它同时是中文数据源里那条条目的名字，而且**平台与年份两道交叉校验
//! 都对上**。
//!
//! ## 为什么它在刮削这一侧，而不是识别那一侧
//!
//! 识别那一层（[`identify::fuzzy`](crate::identify::fuzzy)）只对**一条自动通过的候选
//! 都没有**的变体跑——那是它的定位：数据库覆盖不到时的倒数第二层。
//!
//! 而这个源要管的恰恰是**另一半**：哈希已经精确命中、作品已经立起来、只差一个中文名的
//! 那些。两件事用的是同一个匹配器（[`zh::Index::lookup`]）、同一份剥离规则，
//! 但落点完全不同——一个产出**候选**进队列，一个产出**标题**进集合。
//!
//! ## 只收够得着中置信的那一档
//!
//! **宁可留空也不要写错的中文名**（调研 §13.1）。标题与候选不同：一条错的候选摆在队列里
//! 等人裁决，代价是人多看一眼；一个错的标题会**直接铺进前端**，而且再也没人会去核对。
//! 所以这一侧的闸比识别那一侧紧一档：只有 [`zh::Match::strong`]（平台对得上，且名字
//! 一字不差或者年份也对得上）才产出。
//!
//! **多出来的那几个字段不放松这一条**：它们跟着同一次匹配走，同生共死。撞不上就一个
//! 字段都不产出，撞上了也仍旧是模糊匹配来的**中置信**结论，照旧进**待确认队列**
//! （ADR-0002）。
//!
//! ## 撞在变体这一层，字段挂在它该挂的那一层（票 02）
//!
//! 这一条是整份规格的骨架，两半各有各的道理：
//!
//! - **撞只能在变体层做。** 高信号只有一处——文件名剥出来的**正题**是中文的，拿它撞
//!   中文条目的中文名与别名。作品层手里只有 DAT 给的名字（多为英文或罗马字），
//!   而实测中文条目里带纯拉丁别名的不到六分之一，作品层自己撞连六分之一都撞不上。
//! - **类型、简介、开发商、发行商属于作品。** 那是 [`AnchorKind::Work`] 的定义
//!   （跨平台跨地区都成立的东西挂这一层）。挂到变体上就是同一部作品的每个变体各存
//!   一份重复内容。
//! - **中文名与别名留在变体层。** 依据说的是「这个文件的正题撞上的」——那是变体级的话。
//!
//! 于是 [`ChineseSource`] **在两层都参加**：变体层照旧撞、出中文名；作品层不自己撞名字，
//! 而是把[名下的变体](super::WorkVariant)各撞一遍，**取撞得最多的那条条目**，
//! 产出作品级字段。平票时取条目号最小的那一条——同一份输入跑两次，选出来的是同一条。
//!
//! **作品那一层不依赖采集顺序，也不依赖缓存状态**：它拿的是名下变体的输入，自己现撞
//! 一遍。理由写在 [`WorkVariant`](super::WorkVariant) 上——拿撞完的结果传过来的话，
//! 第二趟变体那一层整片命中缓存，作品锚点手里就是空的，上一趟好好的类型会被当成
//! 「这个源改主意了」清掉。代价是已确认的变体一趟里撞两遍（自己一遍、它所属的作品
//! 一遍），而那是一次内存里的倒排查表。
//!
//! ## 一次匹配，两个源名
//!
//! 撞上一条条目之后拿得到的不只一个名字：那条条目**还叫什么**（它的别名）同样是这部
//! 作品的叫法，该进**标题集合**——一部作品的几个叫法，用户搜哪个都该找得到。
//!
//! 于是这个模块出两个源：[`ChineseSource`] 给中文名与作品级字段，[`ChineseAliasSource`]
//! 给别名。**它们撞的是同一次**（同一份索引、同一套剥离规则、同一组匹配参数、同一个
//! `best`），分成两个名字纯粹是为了让排序认得出它们——别名垫在标题那条链的最后，
//! **只进集合、只管搜得到**，集合里还有别的叫法时轮不到它当显示标题。
//!
//! **垫底不是优先级表办到的**：那张表只在同一档之内分先后，跨语言与类型的档位时轮不到
//! 它说话（`name_cn` 是一串拉丁字时正名落到英文那几档，中文别名落进第 3 档，别名就赢
//! 了）。真管用的是 [`title::rank`](crate::title) 里按这个源名认出来的那一层，它排在
//! 档位**之前**。
//!
//! 合成一个源名的话，两者在库里是同一个源的几条值，排序上完全平手，最后按字典序定
//! 胜负：`合金彈頭7` 与 `合金弹头7` 谁当显示标题全看码位。那不是一条判据。
//!
//! **别名那一路只在变体这一层说话**：一部作品的几个叫法该跟着那个撞上它的文件走，
//! 顺手搬到作品锚点上只会让同一串字在库里多躺一份、还多一条说不清是谁撞出来的依据。
//!
//! ## 简介走的是另一条路（票 03）
//!
//! 撞上那条条目的**中文简介**是这份数据源里最大的一块（实测 94.2% 的游戏条目有它），
//! 可它**不跟着索引进内存**：8.7 万条乘中位 338 字是九十来 MB 常驻，而撞名字不看简介
//! （[`zh::store::Store::load`] 的文档）。撞上之后手里已经有条目号了，那时再按号去库里
//! 点一次名——一趟刮削也就几千次点名。这条路就是 [`Summaries`]。
//!
//! 因此**简介这一栏与别的作品级字段不同**：这个源手里没有 [`Summaries`] 时它一句话都
//! 不说，而类型照旧产出。这件事进[输入指纹](ChineseSource::work_probe)，不然把这条路
//! 接上之后重跑，缓存会一口咬定「输入没变」，那些简介永远补不上来。
//!
//! ## 一条**裁决**管住同一次匹配的全部字段（票 05）
//!
//! 撞上一条条目之后一口气产出六样东西：中文名、别名、类型、简介、开发商、发行商。
//! 它们**同生共死**，都来自同一个条目号——所以裁决的粒度是「**这次匹配对不对**」，
//! 不是「这个字段对不对」。按字段裁，用户得为同一次误撞裁决五遍。
//!
//! 那条裁决住在**沉淀库**里（[`verdict::MatchVerdict`]），钉在**内容锚**上：换台机器、
//! 改过名字之后仍然认得出，两块盘接同一台机器裁决一次两边都受益。刮削这一侧拿到的是
//! 它按变体键摊平之后的那一份（[`Rulings`]），两层各自这么用：
//!
//! - **否定**：那条条目从这个变体的候选里**划掉**（[`ChineseSource::hit`] 里那道闸）。
//!   于是中文名、别名一个都不产出，作品那一层数票时这个变体也不再投它的票——
//!   同一次匹配带来的其余字段跟着一起没了。
//! - **肯定**：那条条目在这个变体的候选里**排到最前**，依据的末尾从「一律进待确认队列」
//!   换成「由人工裁决确认过」（[`zh::Match::evidence_confirmed`]）。
//!
//! **裁决进输入指纹**（两层的 `probe` 都进）：不进的话，人裁完重跑一趟，缓存会一口咬定
//! 「输入没变」而整条跳过——那条错的中文名就永远撞回来。
//!
//! **置信度的口径一个字都没松**：没人裁过的模糊匹配仍旧是**中置信**、仍旧不自动通过。
//! 变的只是**一条裁决管多大范围**。
//!
//! ## 开发商与发行商拆成多条（票 04）
//!
//! 数据源里这两个键有两种写法：顿号分隔的一行（`|开发= 甲、乙`）与多值块
//! （`|发行={ [甲] [乙] }`）。取数那一侧（[`zh::dump::values`]）把两种都拆开了，
//! 这一层照着**一个值一条**产出（[`Harvest::each`]），而不是塞成一串带顿号的长字符串
//! ——用户在前端里按开发商筛的时候，「甲、乙」与「甲」是两个不同的东西，
//! 而一串长字符串两个都筛不出来。
//!
//! **这与类型那一栏的处置正好相反**（挂单 Q11：一条条目写了好几个类型时只取头一个），
//! 两者并不打架：那是票 02 拿类型跑通「撞在变体层、挂在作品层」时的最小做法，而这一票
//! 的验收明写着「拆成多条」。**代价说清楚**：同一个字段上并存好几条时，导出那一侧眼下
//! 只读得出一条（`adapter::converge::build_game` 里那个 `pick`），而胜出的是
//! `Priorities::pick` 第四层排序键挑的那条——那一层比的是**值本身**，也就是**码位序**。
//! 于是 `|开发= 科乐美、KCE东京` 导到前端里写的是 `KCE东京`：**换的不是次序，是换了
//! 一家公司**。数据源的原次序在中立库的上一层（`zh::store` 那张 `subject_fact` 的 `ord`）
//! 好好留着，只是导出那条链上还没有人读它。让前端拿到整组、或者把 `ord` 折进那层排序键，
//! 都是导出那一侧的活——「导出侧一行不改」是这一票的硬约束，所以记在挂单 Q27，不在这儿改。

use std::collections::BTreeMap;

use crate::catalog::{Catalog, CatalogError};
use crate::identify::fuzzy;
use crate::identify::naming;
use crate::verdict::{self, Anchor, MatchVerdict, VerdictError};
use crate::zh;

use super::{AnchorKind, DatEntry, Failure, Field, Harvest, Locality, Source, Subject};

/// **简介的闸**：一条简介最多留这么多**字**（`char`），超出的截掉。
///
/// ## 这个数是怎么定的
///
/// 数据源实测（`dump-2026-09-01`）：中位 338 字，最长 **9,962 字**。用户手上那份维护
/// 多年的前端元数据，简介的中位是 1,354 字——工具产出的东西不该比手写那份还短，所以
/// 闸必须高出这个数一大截。4,000 字同时满足两头：
///
/// - **绝大多数条目一个字都不动**：中位数的 11 倍，连用户手写那份的中位数也只到它的
///   三分之一。被这道闸碰到的是那条 9,962 字的极端条目那一类，而不是常态。
/// - **一条的上界是死的**：4,000 个汉字 = 12 KB。一万条作品全顶到闸上也就 120 MB，
///   而实际按中位数算是几 MB。存储与导出都不会因为某一条超长条目变得不可用。
///
/// **闸也进输入指纹**（[`ChineseSource::work_probe`]）：把它调小了重跑，同一条简介
/// 该重新截一遍，而不是被缓存跳过。
pub const DESCRIPTION_LIMIT: usize = 4_000;

/// 简介被截断时缀在末尾那句话的**开头**。
///
/// 报告靠它把被截断的条目**点得出名**（`scrape::report`）：截断这件事不许是悄悄发生的。
/// 按记号找而不是按长度找——闸是会调的，按长度找的话调完闸老的那些行就点不出来了。
pub const TRUNCATED_MARK: &str = "〔简介太长：原文 ";

/// **作品那一层产出哪几个字段。**
///
/// 它进[作品锚点的输入指纹](ChineseSource::work_probe)。索引那一行
/// （[`zh::store::FIELDS`]）盖不住这件事：那一行说的是**取数**那一侧往库里存了哪几样，
/// 而票 01 早就把开发商与发行商存进去了；改的是**这一侧取不取**。不盖住的话，票 03
/// 那一版采过的作品会被缓存一口咬定「输入没变」而整条跳过，这两栏永远补不上来。
///
/// **这一层将来多产出一个字段，这里要跟着加一行**——`collect_work` 真的产出的那几样
/// 与这一行对不对得上，有一条测试钉着。
const WORK_FIELDS: [Field; 4] = [
    Field::Genre,
    Field::Description,
    Field::Developer,
    Field::Publisher,
];

/// **这一层产出得了的全部字段**：变体那一层的中文名，加上作品那一层的那四样。
///
/// 报告拿它回答缺口那一节里最要紧的一句话（票 06）：空着的这一栏，**中文离线源到底
/// 给不给得出**。给得出的那些空着说的是「没撞上」或者「索引还没取」，那才轮得到
/// 「先跑一次 `romcat zh sync`」；而**年份与汉化组这一层根本不产出**——年份走 DAT
/// 那几家与在线源，汉化组只有 TOSEC 的 `[tr zh <组>]` 说得出。对着这两栏叫用户去取
/// 中文索引，是把人支去做一件永远不会有结果的事，与这张票要修的那句假话是同一类。
///
/// **这一层将来多产出一个字段，这里要跟着加一行**——有一条测试钉着它与
/// [`WORK_FIELDS`] 加中文名对得上。
pub const FIELDS: [Field; 5] = [
    Field::Title,
    Field::Genre,
    Field::Description,
    Field::Developer,
    Field::Publisher,
];

/// 把一条简介收进闸内。
///
/// 返回 `(收好的那一份, 原文有多少字)`；**没超闸时返回的就是原文，一个字都不改**——
/// 换行、开头那两个全角空格与数据源自带的排版都是内容的一部分（规格 18、挂单 Q3）。
fn clamp(text: &str) -> (String, Option<usize>) {
    let total = text.chars().count();
    if total <= DESCRIPTION_LIMIT {
        return (text.to_string(), None);
    }
    let head: String = text.chars().take(DESCRIPTION_LIMIT).collect();
    // 截断这件事**写在值里**，不只写在依据里：值是导出到前端、用户真会读到的那一份，
    // 而依据只有回到工具里才看得见。前端里一段话戛然而止，用户没有任何办法分辨那是
    // 数据源本来就写到这儿，还是工具砍的。
    //
    // **那句说明接在同一行上，不另起段。** Pegasus 那一侧的写出（`adapter::pegasus`
    // 的 `write_attribute`）把单值写成一行，值里的换行会变成一行顶格的续行，而空行
    // 更是直接把那一段截断在半路（挂单 Q22）。数据源自带的换行是规格 18 要求原样留着
    // 的，这一个是**我们自己加的**——加了它只会让那个洞多一处出口。
    (
        format!("{head}……{TRUNCATED_MARK}{total} 字，这里留了前 {DESCRIPTION_LIMIT} 字。〕"),
        Some(total),
    )
}

/// **按条目号取中文简介**的那条路。
///
/// ## 为什么它是单独一条路，而不是索引上的一格
///
/// 简介不跟着 [`zh::store::Store::load`] 进内存：8.7 万条条目、94.2% 有简介、中位
/// 338 字，装进来是九十来 MB 常驻，而那一层的活是**撞名字**，撞名字不看简介。
/// 撞完之后手里已经有条目号了，那时再按号点一次名——一趟刮削的作品锚点也就几千个。
///
/// ## 为什么是个 trait 而不是直接收一个 [`zh::store::Store`]
///
/// 这一侧要测的是「撞上之后简介落在哪个锚点上、超长的怎么处置」，不是「SQLite 读得出
/// 来没有」。收一个 trait，那些行为在内存里就测得完（同 `Source` 自己「采集不碰字节」
/// 那条纪律）。
pub trait Summaries: std::fmt::Debug {
    /// 某一条条目的**中文简介**；数据源没写、或者这条条目不在库里就是 `None`。
    ///
    /// # Errors
    /// 读不出来时返回一句给人看的话。**不要把它吞成 `None`**——那会让整趟悄悄少一栏，
    /// 而报告还说得像模像样。
    fn summary(&self, id: u32) -> Result<Option<String>, String>;
}

impl Summaries for zh::store::Store {
    fn summary(&self, id: u32) -> Result<Option<String>, String> {
        // **走的是库里那一列**，不是内存里那份索引——那一格永远是空串。
        zh::store::Store::summary(self, id).map_err(|error| format!("中文索引读不出简介：{error}"))
    }
}

/// **一个变体身上的匹配裁决**：人对这个源撞出来的哪几条条目说过话（票 05）。
///
/// 一个变体上可以有好几条——「条目 4 不对」与「条目 9 就是它」是两句不同的话，
/// 两句都要留着。把它们挤成一条（「这个变体认哪一条」），改一次匹配参数就分不清人
/// 到底否定过哪一个了。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ruling {
    /// 条目号 →（人说的是「就是这条」吗，那条裁决钉在什么上）。
    ///
    /// 锚跟着一起存，是因为**依据里要写它**：说得出「这条裁决换台机器还认不认得出」
    /// 比让人以为每条都认得出强（ADR-0021 在这里的样子）。
    by_entry: BTreeMap<u32, (bool, String)>,
}

impl Ruling {
    /// 人对这条条目说过话吗；说过就交出`（是不是「就是这条」, 锚是什么）`。
    #[must_use]
    pub fn stance(&self, entry: u32) -> Option<(bool, &str)> {
        self.by_entry
            .get(&entry)
            .map(|(accepted, anchor)| (*accepted, anchor.as_str()))
    }

    /// 进**输入指纹**的那一行。
    ///
    /// **锚也进去**：一条裁决从路径锚换成内容锚时，说的还是同一句话，但依据里那半句
    /// 变了——而依据是要落库的东西，不重采就永远是旧的那句。
    fn fingerprint(&self) -> String {
        self.by_entry
            .iter()
            .map(|(entry, (accepted, anchor))| {
                format!("{entry}{}{anchor}", if *accepted { "准" } else { "否" })
            })
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// **匹配裁决按变体键摊平之后的那一份**，一趟刮削整份拿在手里（票 05）。
///
/// ## 为什么摊平是单独一步
///
/// 沉淀库里那一份（[`verdict::MatchIndex`]）按**锚**存：内容锚说的是「世上这份内容」，
/// 那正是「换台机器仍然认得出」的来处。而刮削这一侧手里只有变体的键——「本机哪个变体
/// 装着这份内容」只有中立库答得出。
///
/// 摊平的方向是**从裁决问变体**，不是从变体问裁决：裁决是人一条条裁出来的，量级是几百
/// 到几千；变体是 46,444 个，而刮削那一侧本来就特意不为每个变体取内容判据
/// （见 `scrape::Plan::build` 里那句「变体这一层不带判据」）。
#[derive(Debug, Clone, Default)]
pub struct Rulings {
    by_variant: BTreeMap<String, Ruling>,
}

impl Rulings {
    /// 一条都没有的那一份。**没有沉淀库时用它，刮削照跑不误。**
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// 手上直接摆一条。**真跑一趟不走这条**——那走 [`Rulings::resolve`]，从沉淀库摊平。
    /// 它在这儿是为了让「刮削怎么用裁决」在内存里测得完，不必先开两份库。
    pub fn put(&mut self, variant_key: &str, entry: u32, accepted: bool, anchor: &str) {
        self.by_variant
            .entry(variant_key.to_string())
            .or_default()
            .by_entry
            .insert(entry, (accepted, anchor.to_string()));
    }

    /// 把沉淀库里那一份摊平到变体键上。
    ///
    /// **内容锚那一批要复核一遍**：[`Catalog::variants_with_content`] 交出来的是
    /// 「某个成员正好是这份内容」的变体，而裁决钉的是「代表这个变体的那份内容」。
    /// 两者只在一种情况下分岔——一个变体里某个附属成员（同一份说明文件是最可能的那种）
    /// 与另一个变体的锚撞了同一个 CRC-32 加大小。复核走 [`crate::identify::content_print`]，
    /// **那一层才是「谁代表这个变体」的唯一说法**，在这儿另写一遍迟早会漂开。
    ///
    /// # Errors
    /// 读中立库失败时返回错误。
    pub fn resolve(
        catalog: &Catalog,
        index: &verdict::MatchIndex,
        source: &str,
    ) -> Result<Self, CatalogError> {
        let mut out = Self::default();
        for (&(crc32, size), list) in index.by_content() {
            let wanted: Vec<&MatchVerdict> = list
                .iter()
                .filter(|verdict| verdict.source == source)
                .collect();
            if wanted.is_empty() {
                continue;
            }
            for key in catalog.variants_with_content(crc32, size)? {
                let Some(row) = catalog.variant(&key)? else {
                    continue;
                };
                let Some(print) = crate::identify::content_print(catalog, &row)? else {
                    continue;
                };
                if print.crc32 != crc32 || print.size != size {
                    continue;
                }
                for verdict in &wanted {
                    out.take(&key, verdict);
                }
            }
        }
        for (key, list) in index.by_path() {
            for verdict in list.iter().filter(|verdict| verdict.source == source) {
                out.take(key, verdict);
            }
        }
        Ok(out)
    }

    /// 收下一条。条目号读不成数的那些**如实扔掉**——那不是这个源写的号。
    fn take(&mut self, variant_key: &str, verdict: &MatchVerdict) {
        if let Ok(entry) = verdict.entry.parse::<u32>() {
            self.put(
                variant_key,
                entry,
                verdict.accepted,
                &verdict.anchor.describe(),
            );
        }
    }

    /// 这个变体身上的那一份；没人裁过就是 `None`。
    #[must_use]
    pub fn for_variant(&self, variant_key: &str) -> Option<&Ruling> {
        self.by_variant.get(variant_key)
    }

    /// 摊到了几个变体身上。
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_variant.len()
    }

    /// 一个变体都没摊到吗。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_variant.is_empty()
    }

    /// 一个变体那一份进输入指纹的字符串；没人裁过就是空串。
    fn fingerprint_of(&self, variant_key: &str) -> String {
        self.for_variant(variant_key)
            .map(Ruling::fingerprint)
            .unwrap_or_default()
    }
}

/// 一个变体撞出来的那一条，连着**拿什么去撞的**。
///
/// 三样绑在一起而不是分开传：**依据**里要写「与文件名剥出来的正题相似度多少」，
/// 而作品那一层手里同时有名下好几个变体撞出来的好几条——分开传三个参数，早晚会把
/// 甲变体的结论配上乙变体的查询串。
#[derive(Debug, Clone, PartialEq)]
struct Hit {
    /// 撞出来的结论。
    one: zh::Match,
    /// 拿去撞的那串字。
    text: String,
    /// 那串字是怎么来的（`正题` / `正题里的中文`）。
    label: &'static str,
    /// **人裁决过这一次匹配吗**（票 05）。裁过就是那条裁决钉在什么上的那一句。
    ///
    /// 它跟着 `Hit` 走而不是另开一路：依据的最后一句由它决定，而依据与结论必须是
    /// 同一次匹配上的两半——分开传，早晚会把甲的结论配上乙的那句话。
    confirmed: Option<String>,
}

impl Hit {
    /// 这一条的**依据**，写成给人看的一句。
    fn evidence(&self, dump: &str) -> String {
        match &self.confirmed {
            Some(anchor) => self
                .one
                .evidence_confirmed(dump, self.label, &self.text, anchor),
            None => self.one.evidence(dump, self.label, &self.text),
        }
    }
}

/// 作品那一层数完票之后手里的那一条。
#[derive(Debug, Clone, PartialEq)]
struct WorkHit {
    /// 胜出的那条匹配。
    hit: Hit,
    /// 它是名下哪个变体撞出来的。**依据里要写它。**
    from: String,
    /// 名下有几个变体撞上了**这一条**条目。
    votes: usize,
    /// 名下一共有几个变体撞上了中文条目（含撞到别条去的）。
    matched: usize,
}

impl WorkHit {
    /// 一个**作品级字段**的依据。
    ///
    /// 收在一处而不是每个字段各拼一遍：这段话说的是「这条结论为什么挂在作品这一层」，
    /// 对简介、类型、开发商、发行商是同一句，只有字段名那一个词不同。各拼一遍的话，
    /// 加第四个字段时最容易漏掉的正是后半句。
    fn evidence(&self, dump: &str, field: Field) -> String {
        format!(
            "{}；**这条结论挂在作品这一层**：撞是名下的变体「{}」撞的，\
             而{}跨平台跨地区都成立（名下 {} 个变体撞上了中文条目，\
             其中 {} 个撞的是这一条）",
            self.hit.evidence(dump),
            self.from,
            field.label(),
            self.matched,
            self.votes,
        )
    }
}

/// 中文离线源。
///
/// 它借着[识别那一层认得的东西](fuzzy::Naming)活着——**剥离规则、索引、匹配参数三样
/// 与那一层是同一份**，各带一份的话，调完参数只有一半生效。索引与规则都是整份装在内存
/// 里的（一次跑几万个锚点），所以带生命周期而不是自己拥有一份。
///
/// **它在两层都说话**：变体层给中文名，作品层给作品级字段（票 02，见模块文档）。
#[derive(Debug, Clone, Copy)]
pub struct ChineseSource<'a> {
    naming: fuzzy::Naming<'a>,
    summaries: Option<&'a dyn Summaries>,
    rulings: Option<&'a Rulings>,
}

impl<'a> ChineseSource<'a> {
    /// 造一个。**调用方保证索引在场**（`sources()` 只在取过数之后造它）；
    /// 万一不在场，这个源一句话都不说，而不是给一个没有出处的中文名。
    ///
    /// 造出来的这一份**不产出简介**——简介不在内存里那份索引上，它走
    /// [`Summaries`]，由 [`with_summaries`](Self::with_summaries) 接上。
    #[must_use]
    pub fn new(naming: fuzzy::Naming<'a>) -> Self {
        Self {
            naming,
            summaries: None,
            rulings: None,
        }
    }

    /// 接上**简介**那条路（票 03）。
    ///
    /// 单独一步而不是并进 [`new`](Self::new)：中文名、别名与类型都在内存里那份索引上，
    /// 简介在库里那一列上，两者的寿命与代价都不同（见 [`Summaries`]）。
    #[must_use]
    pub fn with_summaries(mut self, summaries: &'a dyn Summaries) -> Self {
        self.summaries = Some(summaries);
        self
    }

    /// 接上**匹配裁决**那一份（票 05）。
    ///
    /// 单独一步而不是并进 [`new`](Self::new)：没有沉淀库的时候这个源照样跑，只是这一趟
    /// 没有人裁过任何一次匹配——那与「裁过、全是否定」是两件事。
    #[must_use]
    pub fn with_rulings(mut self, rulings: &'a Rulings) -> Self {
        self.rulings = Some(rulings);
        self
    }

    /// 这个变体身上人裁过什么。
    fn ruling_of(&self, variant_key: &str) -> Option<&Ruling> {
        self.rulings
            .and_then(|rulings| rulings.for_variant(variant_key))
    }

    /// 这个**变体**锚点上撞得出哪一条。
    fn best(&self, subject: &Subject<'_>) -> Option<Hit> {
        // 变体锚点的 `id` 就是变体的键——**匹配裁决**按它摊平（[`Rulings`]）。
        self.hit(
            subject.id,
            subject.main_key?,
            subject.platform,
            subject.entries,
        )
    }

    /// 拿一个变体的那几样撞一次。
    ///
    /// 参数表摊开成四样而不是收一个 [`Subject`]：作品那一层撞的是[名下的变体](
    /// super::WorkVariant)，手里根本没有那些变体的 `Subject`。
    ///
    /// **匹配裁决在这一处生效**（票 05），一处而不是两处：变体层与作品层撞的是同一个
    /// 函数，闸设在这儿，两层就不可能一个认裁决一个不认。
    fn hit(
        &self,
        variant_key: &str,
        main_key: &str,
        platform: Option<&str>,
        entries: &[DatEntry],
    ) -> Option<Hit> {
        let index = self.naming.index?;
        let ruling = self.ruling_of(variant_key);
        let name = crate::path::file_name_of_key(main_key);
        // **乱码不撞**（同 `identify::fuzzy`）：有损转换留下的替换字符一进来就把相似度
        // 算成一团糟，而撞出来的东西没人分辨得了对错。
        if name.contains('\u{FFFD}') {
            return None;
        }
        let parsed = self.naming.rules.parse(name);
        // 年份这一侧：文件名里明写的，或者**已经撞上的那条 DAT 条目名**里读出来的。
        // 后者是这一侧比识别那一层多出来的弹药——已确认的变体身上往往有 TOSEC 的条目名，
        // 而 TOSEC 的第一个括号就是发行日期。
        let year = parsed
            .year
            .or_else(|| naming::year_in(entries.iter().map(|entry| entry.game.as_str())));
        let mut best: Option<Hit> = None;
        for (label, text) in parsed.queries() {
            for one in index.lookup(
                &zh::Query {
                    text,
                    platform,
                    year,
                },
                &self.naming.tuning,
            ) {
                // **只收够得着中置信的那一档**，而且它得真有一个中文名——
                // 条目自己都没写中文名时，这个源无话可说。
                if !one.strong(&self.naming.tuning) || one.entry.name_cn.trim().is_empty() {
                    continue;
                }
                let stance = ruling.and_then(|ruling| ruling.stance(one.entry.id));
                // **人说了「不是这条」，这条条目就从这个变体的候选里划掉**（票 05）。
                // 不是「这个变体从此没有中文条目」——那是两句不同的话，而人只说了前一句。
                if stance.is_some_and(|(accepted, _)| !accepted) {
                    continue;
                }
                let confirmed = stance
                    .filter(|(accepted, _)| *accepted)
                    .map(|(_, anchor)| anchor.to_string());
                // 排序键是`（人说过就是它, 相似度）`：**人说过的排在机器挑的前面**。
                // 这不是改匹配算法（候选还是它算出来的那一批），是在它交出来的那一批上
                // 认人说过的话——否则「肯定」这一档在下一趟就被一个分数更高的候选顶掉了。
                let better = match &best {
                    None => true,
                    Some(seen) => match (confirmed.is_some(), seen.confirmed.is_some()) {
                        (true, false) => true,
                        (false, true) => false,
                        _ => one.score > seen.one.score,
                    },
                };
                if better {
                    best = Some(Hit {
                        one,
                        text: text.to_string(),
                        label,
                        confirmed,
                    });
                }
            }
        }
        best
    }

    /// **名下的变体各撞一遍，取撞得最多的那条条目。**
    ///
    /// 一部作品的几个变体（原版、汉化版、UnDUB 版）本来就该撞到同一条条目上；
    /// 撞岔了是模糊匹配的常态，那时**多数说了算**。平票取**条目号最小**的那一条，
    /// 依据里那个代表变体平手时取**变体键最小**的那个：两处都与「谁摆在前面」无关，
    /// 于是同一批变体不论按什么次序传进来，选出来的结论与依据都一模一样。
    fn work_hit(&self, subject: &Subject<'_>) -> Option<WorkHit> {
        let mut hits: Vec<(&str, Hit)> = Vec::new();
        for variant in subject.variants {
            if let Some(hit) = self.hit(
                &variant.key,
                &variant.main_key,
                variant.platform.as_deref(),
                &variant.entries,
            ) {
                hits.push((variant.key.as_str(), hit));
            }
        }
        if hits.is_empty() {
            return None;
        }
        // **人裁过的压过数票。** `romcat zh judge --yes` 的原话是「这一次匹配带来的全部
        // 字段一并定下」，而作品那四栏也是这一层产出的——让人裁的那一条与模糊匹配来的
        // 平起平坐去数票，就会出现「人在甲上盖了章，可乙撞上的变体多，作品那四栏照旧
        // 写着乙」，命令自己的承诺当场落空、还不报错。裁决压过一切是这个仓库既定的那条
        // （`title::tests::裁决压过一切`、`converge::tests::裁决压过全部规则`）。
        //
        // 有人裁过就**只在人裁过的那些里数票**：一部作品下有人对两条不同条目各盖过章
        // 是真会发生的（裁的是不同变体），那时仍旧多数说了算，平票仍旧取条目号最小的
        // ——只是候选缩到盖过章的那几条里。一条都没裁过时与从前一模一样。
        let 有人裁过 = hits.iter().any(|(_, hit)| hit.confirmed.is_some());
        let mut tally: BTreeMap<u32, usize> = BTreeMap::new();
        for (_, hit) in hits
            .iter()
            .filter(|(_, hit)| !有人裁过 || hit.confirmed.is_some())
        {
            *tally.entry(hit.one.entry.id).or_default() += 1;
        }
        // 键取 `(票数, 条目号取反)`：**这个最大值是唯一的**，所以「最大值有好几个时
        // 取哪一个」这种依赖遍历顺序的事根本不会发生。
        let (&winner, &votes) = tally
            .iter()
            .max_by_key(|(id, count)| (**count, std::cmp::Reverse(**id)))?;
        // 胜出那条条目名下可能有好几个变体撞上，挑一个当**依据**里的代表。三层键：
        //
        // 1. **人裁决过这一次匹配的排最前**（票 05）。这一层不能省：作品级那四栏的依据
        //    是从代表这一条上抄下来的，而「这一次匹配盖没盖过章」正写在那句话的末尾。
        //    只按相似度挑的话，人裁的是甲、而乙分数更高，那四栏就照旧写着「一律进待确认
        //    队列」——一条裁决管住全部字段这件事当场落空，而且不报错。
        // 2. 相似度最高的那个。
        // 3. 相似度也平手时取**变体键最小**的那个。这一层不能省、也不能改成「取先遍历
        //    到的那个」——那样这条结论就挂在调用方摆进来的次序上，而这个函数自己保证
        //    不了那件事。按键定序，它与次序无关。
        let mut chosen: Option<&(&str, Hit)> = None;
        for got in hits.iter().filter(|(_, hit)| hit.one.entry.id == winner) {
            let better = match chosen {
                None => true,
                Some(&(key, ref seen)) => {
                    match (got.1.confirmed.is_some(), seen.confirmed.is_some()) {
                        (true, false) => true,
                        (false, true) => false,
                        _ => match got.1.one.score.partial_cmp(&seen.one.score) {
                            Some(std::cmp::Ordering::Greater) => true,
                            Some(std::cmp::Ordering::Equal) => got.0 < key,
                            _ => false,
                        },
                    }
                }
            };
            if better {
                chosen = Some(got);
            }
        }
        let (from, hit) = chosen?;
        Some(WorkHit {
            hit: hit.clone(),
            from: (*from).to_string(),
            votes,
            matched: hits.len(),
        })
    }

    /// **变体**锚点的输入指纹。
    fn variant_probe(&self, subject: &Subject<'_>) -> Option<String> {
        let main = subject.main_key?;
        // 指纹要盖住**一切会改变结果的东西**（`Source::probe` 的文档）：名字、平台、
        // 用的是哪一版 dump、**这一版索引从数据源里取了哪几样**、**它建的时候把平台
        // 折成了什么样**、**剥离规则**、以及**匹配参数**——门槛从 0.85 调到 0.80 该重采
        // 一遍，取的字段从五样变成九样也该重采一遍，补一条平台别名重建索引之后同样该重采
        // 一遍，补一条剥离规则之后还是该重采一遍，不盖它们的话缓存会一口咬定「输入没变」
        // 而整条跳过。
        let platform = subject.platform.unwrap_or("");
        let tuning = self.naming.tuning.fingerprint();
        // **剥离规则**（`filename::Rules::fingerprint`）：这一层撞的是从文件名里剥出来的
        // **正题**（上面 `hit` 里那句 `rules.parse(name)`），而剥离规则是配置不是代码，
        // 用户补一条自己遇到的模式，正题就变了、撞出来的条目也可能跟着变。它算的是规则的
        // **语义**——给规则文件加一行注释不该让全库重采一遍。
        let rules = self.naming.rules.fingerprint();
        // 已经撞上的 DAT 条目名也进指纹：年份从它们里读。
        let entries: Vec<&str> = subject
            .entries
            .iter()
            .map(|entry| entry.game.as_str())
            .collect();
        // **人裁过什么也是一样输入**（票 05）：不进指纹的话，人裁完重跑一趟，
        // 缓存会一口咬定「输入没变」而整条跳过——那条被否定掉的中文名就永远撞回来。
        let judged = self
            .rulings
            .map(|rulings| rulings.fingerprint_of(subject.id));
        let mut parts = vec![
            main,
            platform,
            self.naming.index.map_or("", zh::Index::dump),
            self.naming.index.map_or("", zh::Index::fields),
            // **建索引那一刻平台折出来的那张表**：dump 与字段那两行都盖不住它——
            // 补一条平台别名重建，dump 是同一份、字段一个没变，变的只有折出来的那一串，
            // 而交叉校验看的正是那一串（`zh::PlatformFold`）。盖的是**折叠真发生了
            // 什么**，不是本机那两张表现在长什么样：后者会在「改了别名但还没重建」时
            // 反过来说谎，说这份索引变了——而它一个字都没变。
            self.naming.index.map_or("", zh::Index::platform_fold),
            rules,
            tuning.as_str(),
            judged.as_deref().unwrap_or(""),
        ];
        parts.extend(entries);
        Some(super::fingerprint(&parts))
    }

    /// **作品**锚点的输入指纹：名下每个变体那几样输入，一样不少。
    ///
    /// 盖的是**输入**而不是撞完的结果——名下多一个变体、某个变体换了平台、或者它身上
    /// 多撞出一条 DAT 条目，作品这一层的答案都可能跟着变。名下一个变体都没有时
    /// **无话可说**，那与「撞过、一个都没撞上」是两件事。
    fn work_probe(&self, subject: &Subject<'_>) -> Option<String> {
        if subject.variants.is_empty() {
            return None;
        }
        let mut parts: Vec<String> = vec![
            self.naming.index.map_or("", zh::Index::dump).to_string(),
            self.naming.index.map_or("", zh::Index::fields).to_string(),
            // **建索引那一刻平台折出来的那张表**（`zh::PlatformFold`）：与变体那一层
            // 盖的是同一样东西，理由也一样——名下的变体撞不撞得上要过交叉校验那一关，
            // 而交叉校验看的正是折出来的那一串。少了它，补一条平台别名重建索引之后，
            // 这一层会整片复用旧的采集记录，新折得动的作品那四栏永远补不上来。
            self.naming
                .index
                .map_or("", zh::Index::platform_fold)
                .to_string(),
            // **剥离规则**（`filename::Rules::fingerprint`）：与变体那一层盖的是同一样
            // 东西，理由也一样——名下每个变体撞哪一条，撞的都是从它文件名里剥出来的正题，
            // 规则一换正题就换。少了它，用户补一条剥离规则重跑，这一层会整片复用旧的采集
            // 记录，新剥得对的那几个作品那四栏永远补不上来。
            self.naming.rules.fingerprint().to_string(),
            self.naming.tuning.fingerprint(),
            // **这一层产出哪几个字段**（[`WORK_FIELDS`]）：多接一样上来就该重采一遍。
            WORK_FIELDS
                .iter()
                .map(|field| field.label())
                .collect::<Vec<_>>()
                .join("、"),
            // **简介那道闸**：调小了重跑，同一条简介该重新截一遍。
            DESCRIPTION_LIMIT.to_string(),
            // **简介那条路在不在场**：不在场时这一层一条简介都产不出，接上之后不重采
            // 的话它们永远补不上来——而「接上」正是这张票让用户做的那件事。
            if self.summaries.is_some() {
                "简介：在"
            } else {
                "简介：不在"
            }
            .to_string(),
        ];
        for variant in subject.variants {
            parts.push(variant.main_key.clone());
            parts.push(variant.platform.clone().unwrap_or_default());
            // **条目数也进去**：少了它，「一个变体带一条 DAT 条目」与「两个变体、
            // 后一个身上一条条目都没有」在这串里长得一模一样。
            parts.push(variant.entries.len().to_string());
            parts.extend(variant.entries.iter().map(|entry| entry.game.clone()));
            // **名下每个变体身上人裁过什么**（票 05）：作品这一层的答案是名下变体数票
            // 数出来的，某一个变体的匹配被否定掉，票数就变了，这一层该重采。
            parts.push(
                self.rulings
                    .map(|rulings| rulings.fingerprint_of(&variant.key))
                    .unwrap_or_default(),
            );
        }
        let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
        Some(super::fingerprint(&parts))
    }

    /// 变体锚点上采到的：一个**有出处的中文名**。
    fn collect_variant(&self, subject: &Subject<'_>, out: &mut Harvest) {
        let Some(hit) = self.best(subject) else {
            return;
        };
        out.value(
            Field::Title,
            hit.one.entry.name_cn.clone(),
            hit.evidence(self.naming.index.map_or("", zh::Index::dump)),
        );
    }

    /// 作品锚点上采到的：撞上那条条目里**属于作品**的那几样。
    ///
    /// 四样：**类型**（票 02 用它把「撞在变体层、挂在作品层」这条路跑通）、**简介**
    /// （票 03）、**开发商**与**发行商**（票 04）。哪几样在这儿，[`WORK_FIELDS`]
    /// 那一行就写着哪几样——它进作品锚点的输入指纹。
    ///
    /// # Errors
    /// 简介那条路读不出来时返回 [`Failure::Skip`]：**这一对不写库，下一趟再来**。
    /// 不吞成「这条没有简介」——那会让整趟悄悄少一栏，而报告还说得像模像样。
    fn collect_work(&self, subject: &Subject<'_>, out: &mut Harvest) -> Result<(), Failure> {
        let Some(won) = self.work_hit(subject) else {
            return Ok(());
        };
        let dump = self.naming.index.map_or("", zh::Index::dump);
        // **类型是单值字段**：`Priorities::merge` 一个字段只回一个值，而集合那条路
        // （`Harvest::each` 加 `title::fold`）眼下只有标题走得通。条目的 infobox 里
        // 写了好几个类型时取头一个——数据源里的原次序，同一份库跑两次取的是同一个。
        if let Some(genre) = won.hit.one.entry.genres.first() {
            out.value(
                Field::Genre,
                genre.clone(),
                won.evidence(dump, Field::Genre),
            );
        }
        self.collect_facts(&won, dump, out);
        self.collect_summary(&won, dump, out)
    }

    /// 作品锚点上那几条**开发商**与**发行商**（票 04）。
    ///
    /// **一个值一条**（[`Harvest::each`]），不是一串带顿号的长字符串：数据源里这两个键
    /// 写成顿号分隔的一行或者多值块，取数那一侧（[`zh::dump::values`]）已经拆开了，
    /// 这一层照着一条一条产出。合起来的话，用户在前端里按开发商筛就只剩一条路——
    /// 拿「甲、乙」这一整串去筛，而那与「甲」是两个不同的东西。
    ///
    /// **两栏同一条路**：它们在数据源里是一对孪生的键（`|开发=` 与 `|发行=`），
    /// 形状、拆法与依据一模一样。各写一遍的话，将来改依据最容易漏掉的正是后一栏。
    ///
    /// ⚠️ **产出的次序不是导出的次序。** 这里按数据源的原次序一条条产出，可导出那一侧
    /// 只挑得出一条、而且按码位挑（见模块文档那一节与挂单 Q27）。要靠次序的人别指望
    /// 这个函数。
    fn collect_facts(&self, won: &WorkHit, dump: &str, out: &mut Harvest) {
        for (field, values) in [
            (Field::Developer, &won.hit.one.entry.developers),
            (Field::Publisher, &won.hit.one.entry.publishers),
        ] {
            for value in values {
                // **首尾空白掐在这儿**：取数那一侧掐过一遍，但手里这份索引也可能是
                // 上一版程序建的库读回来的。掐在产出这一处，这条纪律就与索引是谁建的
                // 无关。掐完是空的就一条都不产出——空值会让优先级链在它身上停下来。
                out.each(field, value.trim(), won.evidence(dump, field));
            }
        }
    }

    /// 作品锚点上那条**中文简介**（票 03）。
    ///
    /// 与类型分开一个函数，是因为它取数的路完全不同：类型在内存里那份索引上，简介在
    /// 库里那一列上、而且**读得出读不出是会失败的**（见 [`Summaries`]）。
    fn collect_summary(&self, won: &WorkHit, dump: &str, out: &mut Harvest) -> Result<(), Failure> {
        // 这条路没接上时**一句话都不说**，而不是给一条空简介：空值会让优先级链在它
        // 身上停下来（`Harvest::value` 的文档）。这件事进指纹，见 `work_probe`。
        let Some(summaries) = self.summaries else {
            return Ok(());
        };
        let Some(text) = summaries
            .summary(won.hit.one.entry.id)
            .map_err(|why| Failure::Skip { why })?
        else {
            return Ok(());
        };
        let (value, cut) = clamp(&text);
        let why = match cut {
            // **没超闸就一个字都没动**：换行、开头那两个全角空格与数据源自带的排版
            // 都是内容的一部分（规格 18）。
            None => won.evidence(dump, Field::Description),
            Some(total) => format!(
                "{}；⚠️ **这条简介被截断了**：原文 {total} 字，超过 {DESCRIPTION_LIMIT} \
                 字这道闸，落库的是前 {DESCRIPTION_LIMIT} 字",
                won.evidence(dump, Field::Description),
            ),
        };
        out.value(Field::Description, value, why);
        Ok(())
    }
}

impl Source for ChineseSource<'_> {
    fn name(&self) -> &str {
        fuzzy::SOURCE
    }

    fn locality(&self) -> Locality {
        // 索引早就取到本机了（`romcat zh sync`）。这一趟一个网络请求都不发。
        Locality::Local
    }

    fn probe(&self, subject: &Subject<'_>) -> Option<String> {
        match subject.kind {
            AnchorKind::Variant => self.variant_probe(subject),
            AnchorKind::Work => self.work_probe(subject),
        }
    }

    fn collect(&self, subject: &Subject<'_>, out: &mut Harvest) -> Result<(), Failure> {
        match subject.kind {
            // 变体那一层整个在内存里，没有会失败的动作。
            AnchorKind::Variant => {
                self.collect_variant(subject, out);
                Ok(())
            }
            // 作品那一层要去库里点一次简介，那一下**读得出读不出是会失败的**。
            AnchorKind::Work => self.collect_work(subject, out),
        }
    }
}

/// 裁一次匹配裁不下去的原因。
#[derive(Debug, thiserror::Error)]
pub enum JudgeError {
    /// 中立库读写失败。
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    /// 沉淀库读写失败。
    #[error(transparent)]
    Verdict(#[from] VerdictError),
    /// 中立库里没有这个变体。
    #[error("中立库里没有 {0} 这个变体。")]
    NoVariant(String),
}

/// 一条**匹配裁决**落下之后的账。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judged {
    /// 这条裁决钉在什么上。**内容锚才是换台机器还认得出的那一种。**
    pub anchor: Anchor,
    /// 新立的一条（真），还是改掉了本来就有的那一条（假）。
    pub fresh: bool,
    /// **这个变体自己撞的就是这条条目吗。**
    ///
    /// 假的时候这条裁决多半管不到任何东西：裁决钉在**这个变体**的内容上，而这条条目
    /// 是名下**别的变体**撞出来、在作品那一层数票胜出的。下一趟刮削时那几栏照旧由那些
    /// 变体投票投回来。调用方要把这件事说给人听，别让他以为裁完就完了。
    pub from_variant: bool,
    /// 就地清掉了几条字段值，一共。**只有否定那一档会清**，见 [`judge`]。
    pub cleared: u64,
    /// 其中**作品**那一层几条。为 0 就是那一层一个字都没动。
    pub cleared_work: u64,
    /// 这个变体属于哪个作品；识别还没认出来时是 `None`。
    ///
    /// **它不表示那一层被动过**——动没动看 [`cleared_work`](Self::cleared_work)。
    pub work: Option<String>,
}

/// 对**中文离线源那一次匹配**下一条裁决：说它对，或者说它不对（票 05）。
///
/// ## 粒度是「这次匹配」，不是「这个字段」
///
/// 撞上一条条目之后一口气产出六样：中文名、别名（变体锚点上）、类型、简介、开发商、
/// 发行商（作品锚点上）。它们**同生共死**——都来自同一个条目号，所以人只需要说一次
/// 「这次撞错了」，不必对同一次误撞裁决五遍。
///
/// ## 两档做的事不一样
///
/// - **否定**：那一次匹配的产出**就地清掉**——变体锚点上中文名与别名两个源的行、
///   作品锚点上中文离线源的行，一条不留。错的东西不许在库里多躺一秒，而它带着的
///   **依据**指着一条人已经说了不对的条目。作品那一层下一趟重跑刮削时按**剩下的变体**
///   重新数票：名下还有别的变体撞着同一条条目的话，那几栏会照样回来，而且是对的。
/// - **肯定**：**一个字都不清**。那些值是对的，留着；变的只是它们的**依据**——
///   下一趟重跑时末尾那句从「一律进待确认队列」换成「由人工裁决确认过」。
///   靠的是输入指纹（[`Ruling::fingerprint`] 进了两层的 `probe`），不是靠清库。
///
/// ## 作品那一层只在「这个变体自己撞的就是这条」时才动
///
/// 裁决钉在**这个变体**的内容上，而作品那一层的答案是**名下变体数票**数出来的——胜出的
/// 可能是别的变体撞出来的另一条条目。这时清掉作品那一层是**白清**：下一趟那些变体照旧
/// 投它们的票，那几栏原样回来，而用户以为自己刚刚把它裁掉了。所以先看变体锚点上有没有
/// 这条条目的产出（[`Judged::from_variant`]），没有就一个字都不动那一层，并且把这件事
/// 报回去。
///
/// **别的源产出的同名字段一个字都不碰**：这条裁决只管中文离线源那一次匹配。
///
/// 参数表摊开成三样而不是收一个 [`Site`](crate::site::Site)，与 `triage::apply` 一致：
/// 那个类型管的是「这三样必须一起**开**」，不是「每个函数都得收着它」。
///
/// # Errors
/// 变体不在库里、或者两份库有一份读写失败时返回错误。
pub fn judge(
    catalog: &mut Catalog,
    store: &mut verdict::Store,
    library: &str,
    variant_key: &str,
    entry: u32,
    accepted: bool,
    note: Option<String>,
) -> Result<Judged, JudgeError> {
    let Some(row) = catalog.variant(variant_key)? else {
        return Err(JudgeError::NoVariant(variant_key.to_string()));
    };
    // **内容锚优先**：换台机器、改过名字之后仍然认得出，而且两块盘接同一台机器时
    // 裁决一次两边都受益。拿不到内容判据（容器穿不透、压缩镜像、目录树转储）才退到
    // 路径锚——那一种**只在本机成立**，`Anchor::is_shareable` 说得出这件事。
    let anchor = match crate::identify::content_print(catalog, &row)? {
        Some(print) => Anchor::Content {
            crc32: print.crc32,
            size: print.size,
            sha1: None,
        },
        None => Anchor::Path {
            library: library.to_string(),
            variant_key: variant_key.to_string(),
        },
    };
    let fresh = store.put_match(
        &MatchVerdict::now(anchor.clone(), fuzzy::SOURCE, &entry.to_string(), accepted)
            .with_note(note),
    )?;
    let work = catalog.work_of_variant(variant_key)?;
    // **先问「这个变体自己撞的是不是这一条」**，再决定动不动作品那一层。问在清库之前，
    // 因为清完就问不出来了。
    let from_variant = catalog
        .scraped_values(AnchorKind::Variant.label(), variant_key)?
        .iter()
        .any(|value| value.source == fuzzy::SOURCE && zh::entry_in(&value.evidence) == Some(entry));
    let mut cleared = 0;
    let mut cleared_work = 0;
    if !accepted {
        for source in [fuzzy::SOURCE, fuzzy::ALIAS_SOURCE] {
            cleared += clear_source(catalog, AnchorKind::Variant, variant_key, source, entry)?;
        }
        if from_variant && let Some(work) = &work {
            cleared_work = clear_source(catalog, AnchorKind::Work, work, fuzzy::SOURCE, entry)?;
            cleared += cleared_work;
        }
    }
    Ok(Judged {
        anchor,
        fresh,
        from_variant,
        cleared,
        cleared_work,
        work,
    })
}

/// 把一个锚点上某个源**来自这一次匹配**的字段值清掉，返回清掉了几条。
///
/// **先按条目号确认这个锚点上的话真是这一次匹配说的**：作品那一层的答案是名下变体
/// 数票数出来的，胜出的可能是**另一条**条目——那几栏与人刚否定的这一次匹配无关，
/// 一个字都不该动。确认过之后整批扫掉是对的：一个源在一个锚点上只认一条条目
/// （变体层就撞一次，作品层数完票只留胜出那一条），所以「这个源在这个锚点上说的话」
/// 与「这一次匹配说的话」是同一批。
fn clear_source(
    catalog: &mut Catalog,
    kind: AnchorKind,
    subject: &str,
    source: &str,
    entry: u32,
) -> Result<u64, CatalogError> {
    let values = catalog.scraped_values(kind.label(), subject)?;
    let mine: Vec<_> = values
        .iter()
        .filter(|value| value.source == source)
        .collect();
    // 这个锚点上这个源的话，说的是这一次匹配吗。
    if !mine
        .iter()
        .any(|value| zh::entry_in(&value.evidence) == Some(entry))
    {
        return Ok(0);
    }
    // **连采集记录一起扫掉**（`forget_scraped`）：留着那条记录，下一趟会被输入指纹
    // 一口咬定「这一对采全了」而整条跳过——而这一票要的正是「重跑一趟结论稳定」。
    catalog.forget_scraped(kind.label(), subject, source)?;
    // **报的数就是真删掉的那批**（这个源在这个锚点上的全部行），不是「依据里认得出这个
    // 条目号」的那批。两者眼下是同一批，但前者是 `forget_scraped` 的定义，后者依赖依据
    // 那句话的写法——依据改一个字，后者就会少报。
    Ok(u64::try_from(mine.len()).unwrap_or(u64::MAX))
}

/// 一条**来自同一次匹配**的字段值，连它落在哪个锚点上。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedValue {
    /// 落在哪一层锚点上。
    pub kind: AnchorKind,
    /// 锚点：变体的键，或者作品名。
    pub subject: String,
    /// 哪个字段。
    pub field: Field,
    /// 值。
    pub value: String,
    /// 哪个源产出的（中文名那一路，还是别名那一路）。
    pub source: String,
    /// **依据**：这条值是怎么来的，那句话原样。
    ///
    /// 带着它而不是让读的那一侧自己再去库里捞一遍：队列要在**堆上写出依据**
    /// （票 `queue-followups/06`），而捞回来之后还得再按条目号认一遍谁属于这一堆
    /// ——那正是[归堆](matched_groups)这件事本身，第二处认法迟早与这一处漂开。
    pub evidence: String,
}

/// **同一次匹配带来的那一堆字段**：一个条目号，一堆值（票 05）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchGroup {
    /// 条目号。**它就是「同一次匹配」的判据**。
    pub entry: u32,
    /// 人已经裁决过这一次匹配了吗。
    pub confirmed: bool,
    /// **这一堆里有落在变体锚点上的值吗**——也就是「这个变体自己撞的就是这一条」。
    ///
    /// 假的时候这一堆全在作品锚点上：它是名下**别的变体**撞出来、在作品那一层数票胜出的
    /// 条目。**裁它要去裁那个变体**——裁决钉在内容上，钉在这个变体身上管不到那一层
    /// （[`judge`] 的文档说的就是这件事）。
    pub from_variant: bool,
    /// 这一次匹配带来的全部字段值，两层锚点都在里面。
    pub values: Vec<MatchedValue>,
}

/// 一个变体身上，**中文离线源那几次匹配**各带来了哪些字段，按**条目号**归堆（票 05）。
///
/// ## 为什么判据只能是条目号
///
/// 同一次匹配的产出散在两层锚点上——中文名与别名挂在**变体**上，类型、简介、开发商、
/// 发行商挂在**作品**上；字段名不同、值不同、锚点也不同。它们身上唯一共通的东西是各自
/// **依据**里那个条目号（[`zh::entry_in`]）。队列要**看得出哪几个字段来自同一次匹配**，
/// 就只能按它归。
///
/// 作品锚点上那一堆是**整部作品共有的**：名下别的变体也撞着同一条条目时，它们看见的是
/// 同一堆值。这不是重复计数，是这几个字段本来就挂在那一层（票 02）。
///
/// # Errors
/// 读中立库失败时返回错误。
pub fn matched_groups(
    catalog: &Catalog,
    variant_key: &str,
) -> Result<Vec<MatchGroup>, CatalogError> {
    let mut by_entry: BTreeMap<u32, MatchGroup> = BTreeMap::new();
    let work = catalog.work_of_variant(variant_key)?;
    let mut places = vec![(AnchorKind::Variant, variant_key.to_string())];
    if let Some(work) = work {
        places.push((AnchorKind::Work, work));
    }
    for (kind, subject) in places {
        for value in catalog.scraped_values(kind.label(), &subject)? {
            if value.source != fuzzy::SOURCE && value.source != fuzzy::ALIAS_SOURCE {
                continue;
            }
            let (Some(entry), Some(field)) = (
                zh::entry_in(&value.evidence),
                Field::from_label(&value.field),
            ) else {
                continue;
            };
            let group = by_entry.entry(entry).or_insert(MatchGroup {
                entry,
                confirmed: false,
                from_variant: false,
                values: Vec::new(),
            });
            group.confirmed |= zh::is_confirmed(&value.evidence);
            group.from_variant |= kind == AnchorKind::Variant;
            group.values.push(MatchedValue {
                kind,
                subject: subject.clone(),
                field,
                value: value.value,
                source: value.source,
                evidence: value.evidence,
            });
        }
    }
    Ok(by_entry.into_values().collect())
}

/// **中文离线源的别名那一路**：撞上的那条条目**还叫什么**。
///
/// ## 为什么它是一个单独的源，而不是上面那个源多说几句
///
/// 两路给的东西在标题这一栏上的分量差得远：中文名是「这个文件的正题撞上的那个名字」，
/// 别名是「那条条目还叫什么」。分成两个源名，排序才认得出它们——别名排在整条链的最后，
/// **只进标题集合、只管搜得到**，集合里还有别的叫法时轮不到它当显示标题。垫底那件事由
/// [`title::rank`](crate::title) 按这个源名办到（它排在语言与类型的档位**之前**）；
/// [优先级表](super::priority)只管同一档之内的定序。
///
/// 合成一路的话，两者在库里是同一个源的几条值，排序上完全平手，最后按字典序定胜负：
/// `合金彈頭7` 与 `合金弹头7` 谁当显示标题全看码位。那不是一条判据。
///
/// ## 它与上面那个源撞的是同一次
///
/// 同一份索引、同一套剥离规则、同一组匹配参数、同一个 `ChineseSource::best`。
/// 于是**两路同生共死**：中文名撞不上，别名也一个都不产出。
///
/// ## 它**只在变体这一层说话**
///
/// 上面那个源在作品层也参加（票 02），这一路不跟：一部作品的几个叫法该跟着那个撞上它的
/// 文件走，搬到作品锚点上只会让同一串字在库里多躺一份，还多一条说不清是谁撞出来的依据。
#[derive(Debug, Clone, Copy)]
pub struct ChineseAliasSource<'a> {
    inner: ChineseSource<'a>,
}

impl<'a> ChineseAliasSource<'a> {
    /// 造一个。参数与 [`ChineseSource::new`] 一模一样——**它们撞的是同一次**。
    #[must_use]
    pub fn new(naming: fuzzy::Naming<'a>) -> Self {
        Self {
            inner: ChineseSource::new(naming),
        }
    }

    /// 接上**匹配裁决**那一份（票 05）。
    ///
    /// **别名不单独裁**：它跟着中文名那一路的同一次匹配走，所以读的是同一个源名下的
    /// 那批裁决（[`fuzzy::SOURCE`]），而不是自己那个源名下的。人否定了那一次匹配，
    /// 中文名与别名一起没有——它们本来就同生共死。
    #[must_use]
    pub fn with_rulings(mut self, rulings: &'a Rulings) -> Self {
        self.inner = self.inner.with_rulings(rulings);
        self
    }
}

impl Source for ChineseAliasSource<'_> {
    fn name(&self) -> &str {
        fuzzy::ALIAS_SOURCE
    }

    fn locality(&self) -> Locality {
        Locality::Local
    }

    fn probe(&self, subject: &Subject<'_>) -> Option<String> {
        // **这一路不跟到作品那一层去。** 这道闸不能省：上面那个源的 `probe` 现在
        // 在作品锚点上也说话，直接转过去的话，别名就跟着被搬到作品锚点上了。
        if subject.kind != AnchorKind::Variant {
            return None;
        }
        // 与中文名那一路盖的是同一批输入——它们撞的是同一次。
        self.inner.probe(subject)
    }

    fn collect(&self, subject: &Subject<'_>, out: &mut Harvest) -> Result<(), Failure> {
        if subject.kind != AnchorKind::Variant {
            return Ok(());
        }
        let Some(hit) = self.inner.best(subject) else {
            return Ok(());
        };
        let dump = self.inner.naming.index.map_or("", zh::Index::dump);
        let evidence = hit.evidence(dump);
        for alias in &hit.one.entry.aliases {
            // **中文名不在这儿再来一遍**：那是另一路的值，重复一条只会让同一串字在
            // 标题集合里占两行、各带一条说法不同的**依据**。
            if alias.trim().is_empty() || alias.trim() == hit.one.entry.name_cn.trim() {
                continue;
            }
            out.each(
                Field::Title,
                alias.clone(),
                format!("{evidence}；而这条条目还叫「{alias}」"),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filename::Rules;
    use crate::scrape::{DatEntry, Finding, LocalMedia, WorkVariant};

    fn 索引() -> zh::Index {
        zh::Index::build(
            vec![
                zh::Entry {
                    id: 4,
                    name: "メタルスラッグ7".to_string(),
                    name_cn: "合金弹头7".to_string(),
                    aliases: vec!["Metal Slug 7".to_string()],
                    year: Some(2008),
                    platforms: vec!["NDS".to_string()],
                    platform_text: "NDS".to_string(),
                    genres: vec!["ACT".to_string()],
                    // 照真库那条记录写的（`zh::dump` 的单元测试钉的是同一条）：
                    // 开发写成**顿号分隔的一行**（`|开发= SNK、北斗`），发行写成
                    // **多值块**。取数那一侧已经把两种都拆开了，索引上就是几条。
                    developers: vec!["SNK".to_string(), "北斗".to_string()],
                    publishers: vec!["世嘉".to_string()],
                    ..zh::Entry::default()
                },
                zh::Entry {
                    id: 5,
                    name: "Only English".to_string(),
                    name_cn: String::new(),
                    aliases: Vec::new(),
                    year: Some(2000),
                    platforms: vec!["NDS".to_string()],
                    platform_text: "NDS".to_string(),
                    ..zh::Entry::default()
                },
                // 条目号**比上面那条大**，类型也不一样：作品那一层数票数对了没有，
                // 光看一条条目是看不出来的。
                zh::Entry {
                    id: 6,
                    name: "悪魔城ドラキュラ".to_string(),
                    name_cn: "恶魔城".to_string(),
                    aliases: Vec::new(),
                    year: Some(2008),
                    platforms: vec!["NDS".to_string()],
                    platform_text: "NDS".to_string(),
                    genres: vec!["AVG".to_string()],
                    // 开发只写了一家，而**发行这个键这条条目根本没写**——
                    // 缺键就是没有，不是错误（规格 21）。
                    developers: vec!["科乐美".to_string()],
                    ..zh::Entry::default()
                },
            ],
            "dump-2026-09-01".to_string(),
        )
    }

    fn 源<'a>(rules: &'a Rules, index: &'a zh::Index) -> ChineseSource<'a> {
        ChineseSource::new(fuzzy::Naming {
            rules,
            index: Some(index),
            tuning: zh::Tuning::default(),
        })
    }

    fn 变体<'a>(key: &'a str, entries: &'a [DatEntry], media: &'a [LocalMedia]) -> Subject<'a> {
        Subject {
            kind: AnchorKind::Variant,
            id: key,
            platform: Some("NDS"),
            entries,
            main_key: Some(key),
            media,
            variants: &[],
            media_limit: None,
            confirmed: true,
            basis: None,
        }
    }

    /// 一个作品锚点：自己没有主文件，手里只有**名下的变体**。
    fn 作品(variants: &[WorkVariant]) -> Subject<'_> {
        Subject {
            kind: AnchorKind::Work,
            id: "合金弹头7",
            platform: Some("NDS"),
            entries: &[],
            main_key: None,
            media: &[],
            variants,
            media_limit: None,
            confirmed: true,
            basis: None,
        }
    }

    /// 名下的一个变体。
    fn 名下(key: &str) -> WorkVariant {
        WorkVariant {
            key: key.to_string(),
            main_key: key.to_string(),
            platform: Some("NDS".to_string()),
            entries: Vec::new(),
        }
    }

    fn 采(key: &str, entries: &[DatEntry]) -> Harvest {
        let rules = Rules::builtin();
        let index = 索引();
        let mut out = Harvest::default();
        源(&rules, &index)
            .collect(&变体(key, entries, &[]), &mut out)
            .expect("本地源不会失败");
        out
    }

    /// 在**作品**锚点上采一趟。**简介那条路没接上**，所以这一趟不产出简介。
    fn 采作品(variants: &[WorkVariant]) -> Harvest {
        let rules = Rules::builtin();
        let index = 索引();
        let mut out = Harvest::default();
        源(&rules, &index)
            .collect(&作品(variants), &mut out)
            .expect("这一趟没有会失败的动作");
        out
    }

    /// 本机那份库里的简介，摆在内存里的一份（见 [`Summaries`] 为什么是个 trait）。
    #[derive(Debug)]
    struct 简介表(Option<String>);

    impl Summaries for 简介表 {
        fn summary(&self, _: u32) -> Result<Option<String>, String> {
            Ok(self.0.clone())
        }
    }

    /// 一条读不出来的：整条路都在，就是读不动。
    #[derive(Debug)]
    struct 读不动;

    impl Summaries for 读不动 {
        fn summary(&self, _: u32) -> Result<Option<String>, String> {
            Err("库文件被截断了".to_string())
        }
    }

    /// 摆一份**匹配裁决**：这个变体上，人对这条条目说了这句话。
    fn 裁过(key: &str, entry: u32, accepted: bool) -> Rulings {
        let mut rulings = Rulings::none();
        rulings.put(key, entry, accepted, "CRC-32 1234ABCD + 4096 字节");
        rulings
    }

    /// 带着一份匹配裁决，在**变体**锚点上采一趟。
    fn 采带裁决(key: &str, rulings: &Rulings) -> Harvest {
        let rules = Rules::builtin();
        let index = 索引();
        let mut out = Harvest::default();
        源(&rules, &index)
            .with_rulings(rulings)
            .collect(&变体(key, &[], &[]), &mut out)
            .expect("本地源不会失败");
        out
    }

    /// 带着一份匹配裁决，在**作品**锚点上采一趟。
    fn 采作品带裁决(variants: &[WorkVariant], rulings: &Rulings) -> Harvest {
        let rules = Rules::builtin();
        let index = 索引();
        let mut out = Harvest::default();
        源(&rules, &index)
            .with_rulings(rulings)
            .collect(&作品(variants), &mut out)
            .expect("这一趟没有会失败的动作");
        out
    }

    /// 接上**简介**那条路，在作品锚点上采一趟。
    fn 采作品带简介(
        variants: &[WorkVariant],
        summaries: &dyn Summaries,
    ) -> Result<Harvest, Failure> {
        let rules = Rules::builtin();
        let index = 索引();
        let mut out = Harvest::default();
        源(&rules, &index)
            .with_summaries(summaries)
            .collect(&作品(variants), &mut out)?;
        Ok(out)
    }

    /// 作品锚点上采到的某一个字段。
    fn 那一格(out: &Harvest, field: Field) -> Option<&Finding> {
        out.values.iter().find(|it| it.field == field)
    }

    /// 作品锚点上某个字段采到的**全部**值，按产出的次序。
    ///
    /// 与 [`那一格`] 分开：开发商与发行商在同一个锚点上是**好几条**（票 04），
    /// 拿 `find` 去看只看得见头一条，而这一票要钉的正是「一共有几条」。
    fn 那几条(out: &Harvest, field: Field) -> Vec<&str> {
        out.values
            .iter()
            .filter(|it| it.field == field)
            .map(|it| it.value.as_str())
            .collect()
    }

    #[test]
    fn 撞上了就给一个有出处的中文名() {
        let out = 采("nds/合金弹头7[某汉化组](简).7z", &[]);
        assert_eq!(out.values.len(), 1);
        assert_eq!(out.values[0].field, Field::Title);
        assert_eq!(out.values[0].value, "合金弹头7");
        // **依据**要说得出是哪条条目、两道校验各是什么——这正是「有出处」三个字的内容。
        assert!(
            out.values[0].evidence.contains("条目 4"),
            "{}",
            out.values[0].evidence
        );
        assert!(out.values[0].evidence.contains("平台交叉校验对得上"));
        // **类型不挂在变体上**：那是作品级的字段（票 02）。
        assert!(out.values.iter().all(|it| it.field != Field::Genre));
    }

    #[test]
    fn 够不着中置信的一律不给() {
        // 标题与候选不同：错的候选等人裁决，错的标题直接铺进前端。所以这一侧的闸更紧。
        let out = 采("nds/合金弹头7代.7z", &[]);
        assert!(out.values.is_empty());
    }

    #[test]
    fn 条目自己没有中文名时无话可说() {
        let out = 采("nds/Only English.7z", &[]);
        assert!(out.values.is_empty());
    }

    #[test]
    fn 年份从已经撞上的_tosec_条目名里读() {
        // 这一侧比识别那一层多出来的弹药：已确认的变体身上往往有 TOSEC 的条目名，
        // 而 TOSEC 的第一个括号就是发行日期。
        let rules = Rules::builtin();
        let index = 索引();
        let entries = vec![DatEntry {
            source: "TOSEC".to_string(),
            game: "Metal Slug 7 (2008)(SNK)".to_string(),
        }];
        let subject = 变体("nds/合金弹头7.7z", &entries, &[]);
        let hit = 源(&rules, &index).best(&subject).expect("撞得上");
        assert_eq!(hit.one.year, zh::Check::Agrees);
    }

    #[test]
    fn 作品那一层不自己撞名字读的是名下变体撞出来的条目() {
        // 票 02 的正题：**撞在变体层、挂在作品层**。同一部作品的两个变体撞到同一条
        // 条目时，作品锚点上**只有一份**类型，不是两份。
        let out = 采作品(&[
            名下("nds/合金弹头7[某汉化组](简).7z"),
            名下("nds/合金弹头7.7z"),
        ]);
        // 这一层现在产出四样（类型、简介、开发商、发行商），所以按**字段**数，
        // 不按整个 `Harvest` 的条数——「只有一份类型」才是这条测试要钉的话。
        assert_eq!(
            那几条(&out, Field::Genre),
            vec!["ACT"],
            "两个变体撞到同一条，只该有一份类型"
        );
        let 依据 = &那一格(&out, Field::Genre).expect("有这一条").evidence;
        // **依据**四样齐全：条目号、撞上的是哪个名字、两道校验各是什么。
        assert!(依据.contains("条目 4"), "{依据}");
        assert!(依据.contains("「合金弹头7」"), "{依据}");
        assert!(依据.contains("平台交叉校验对得上"), "{依据}");
        // 年份这一道要钉**结果**而不是栏目名——栏目名是无条件拼上去的，钉它恒真。
        // 这两个变体的文件名与 DAT 条目都写不出年份，所以这一道是「说不出」。
        assert!(依据.contains("年份交叉校验说不出"), "{依据}");
        // 还说得出这条结论是名下哪个变体撞出来的、名下几个变体撞上了。
        // **写全那个键**：只写前缀的话名下两个变体都对得上，换成哪一个断言都照绿。
        assert!(依据.contains("名下的变体「nds/合金弹头7.7z」"), "{依据}");
        assert!(
            依据.contains("名下 2 个变体撞上了中文条目，其中 2 个撞的是这一条"),
            "{依据}"
        );

        // **代表挑得与次序无关**：两个变体撞的是同一条条目、相似度也一模一样，
        // 那时按变体键定序。把它们反过来摆进去，结论与依据一个字都不变。
        let 反 = 采作品(&[
            名下("nds/合金弹头7.7z"),
            名下("nds/合金弹头7[某汉化组](简).7z"),
        ]);
        assert_eq!(out.values, 反.values);
        // **中置信、进待确认队列这一条一个字都没松**：多了几个字段不等于自动通过。
        assert!(依据.contains("一律进待确认队列"), "{依据}");
    }

    #[test]
    fn 名下一个变体都没撞上的作品什么都不产出() {
        // 撞不上就一个字段都不产出——**宁可留空也不要写错的**（调研 §13.1）。
        let out = 采作品(&[名下("nds/Only English.7z"), 名下("nds/谁也不认得的名字.7z")]);
        assert!(out.values.is_empty(), "{:?}", out.values);

        // 而「名下一个变体都没有」是另一件事：那时这个源**无话可说**，
        // 于是引擎会把上一轮挂上去的结论清掉，而不是当成「撞过、没撞上」。
        let rules = Rules::builtin();
        let index = 索引();
        assert!(源(&rules, &index).probe(&作品(&[])).is_none());
        assert!(
            源(&rules, &index)
                .probe(&作品(&[名下("nds/x.7z")]))
                .is_some()
        );
    }

    #[test]
    fn 两个变体撞到不同条目时取最多数() {
        // 一部作品的几个变体本该撞到同一条条目上；撞岔了是模糊匹配的常态，
        // 那时**多数说了算**——哪怕多数那一条的条目号更大。
        let out = 采作品(&[
            名下("nds/合金弹头7.7z"),
            名下("nds/恶魔城.7z"),
            名下("nds/恶魔城[某汉化组](简).7z"),
        ]);
        let 类型 = 那一格(&out, Field::Genre).expect("有这一条");
        assert_eq!(类型.value, "AVG", "两票的条目 6 该胜过一票的条目 4");
        // 别的字段也跟着那条胜出的条目走：条目 6 的开发是科乐美，不是条目 4 的 SNK。
        assert_eq!(那几条(&out, Field::Developer), vec!["科乐美"]);
        assert!(
            类型
                .evidence
                .contains("名下 3 个变体撞上了中文条目，其中 2 个撞的是这一条"),
            "{}",
            类型.evidence
        );
    }

    #[test]
    fn 平票时取条目号最小的那一条而且与名下的次序无关() {
        // 「数目相同时的取法是确定的」：同一份输入两次跑出同一个结果，
        // 而且换个次序摆进去，结果一个字都不变。
        let 甲 = 名下("nds/合金弹头7.7z");
        let 乙 = 名下("nds/恶魔城.7z");
        let 正 = 采作品(&[甲.clone(), 乙.clone()]);
        let 反 = 采作品(&[乙, 甲]);
        assert_eq!(
            那一格(&正, Field::Genre).expect("有这一条").value,
            "ACT",
            "平票该取条目号小的那条（4 < 6）"
        );
        assert_eq!(正.values, 反.values, "换个次序，结论与依据都该一模一样");
    }

    #[test]
    fn 作品那一层的指纹盖住名下变体的那几样() {
        // 名下多一个变体、某个变体换了平台、或者它身上多撞出一条 DAT 条目，
        // 作品这一层的答案都可能跟着变——不盖住的话缓存会一口咬定「输入没变」。
        let rules = Rules::builtin();
        let index = 索引();
        let source = 源(&rules, &index);
        let 一个 = [名下("nds/合金弹头7.7z")];
        let 两个 = [名下("nds/合金弹头7.7z"), 名下("nds/恶魔城.7z")];
        let mut 换平台 = 一个.clone();
        换平台[0].platform = Some("GBA".to_string());
        let mut 多一条 = 一个.clone();
        多一条[0].entries.push(DatEntry {
            source: "TOSEC".to_string(),
            game: "Metal Slug 7 (2008)(SNK)".to_string(),
        });
        let 指纹 = |v: &[WorkVariant]| source.probe(&作品(v)).expect("名下有变体就说得出话");
        assert_ne!(指纹(&一个), 指纹(&两个));
        assert_ne!(指纹(&一个), 指纹(&换平台));
        assert_ne!(指纹(&一个), 指纹(&多一条));
    }

    #[test]
    fn 别名那一路把条目的别名都交出来而且不重复中文名() {
        let rules = Rules::builtin();
        let index = 索引();
        let source = ChineseAliasSource::new(fuzzy::Naming {
            rules: &rules,
            index: Some(&index),
            tuning: zh::Tuning::default(),
        });
        let mut out = Harvest::default();
        source
            .collect(&变体("nds/合金弹头7.7z", &[], &[]), &mut out)
            .expect("本地源不会失败");
        let 值: Vec<&str> = out.values.iter().map(|it| it.value.as_str()).collect();
        assert_eq!(值, vec!["Metal Slug 7"]);
        assert!(out.values.iter().all(|it| it.field == Field::Title));
        // 依据里既说得出撞的是哪条条目，也说得出这一条是那条条目的另一个叫法。
        assert!(out.values[0].evidence.contains("条目 4"));
        assert!(out.values[0].evidence.contains("还叫「Metal Slug 7」"));
        // **两路同生共死**：中文名撞不上，别名也一个都不产出。
        let mut out = Harvest::default();
        source
            .collect(&变体("nds/合金弹头7代.7z", &[], &[]), &mut out)
            .expect("本地源不会失败");
        assert!(out.values.is_empty());
    }

    #[test]
    fn 别名那一路不跟到作品那一层去() {
        // 中文名那一路在作品层也参加了（票 02），这一路**不跟**：一部作品的几个叫法
        // 该跟着那个撞上它的文件走，搬到作品锚点上只会让同一串字在库里多躺一份。
        let rules = Rules::builtin();
        let index = 索引();
        let source = ChineseAliasSource::new(fuzzy::Naming {
            rules: &rules,
            index: Some(&index),
            tuning: zh::Tuning::default(),
        });
        let 名下变体 = [名下("nds/合金弹头7.7z")];
        let work = 作品(&名下变体);
        assert!(source.probe(&work).is_none());
        let mut out = Harvest::default();
        source.collect(&work, &mut out).expect("本地源不会失败");
        assert!(out.values.is_empty());
        // 对照：中文名那一路在同一个作品锚点上是说话的。
        assert!(源(&rules, &index).probe(&work).is_some());
    }

    #[test]
    fn 索引取了哪几样也进输入指纹() {
        // 改了取哪些字段之后重跑，撞上过的锚点该**重采**而不是被缓存跳过——
        // 那正是这一票把简介、类型、开发商、发行商取进索引之后要保住的事。
        let rules = Rules::builtin();
        let subject = 变体("nds/合金弹头7.7z", &[], &[]);
        let 造 = |index: &zh::Index| 源(&rules, index).probe(&subject);
        let 现在 = 索引();
        // 上一版索引：同一份 dump、同一批条目，只是当时只取了中文名与别名。
        let 上一版 = 索引().with_fields("中文名、别名、年份、平台".to_string());
        assert_ne!(造(&现在), 造(&上一版));
        // 作品那一层同样盖得住它。
        let 名下变体 = [名下("nds/合金弹头7.7z")];
        let work = 作品(&名下变体);
        assert_ne!(
            源(&rules, &现在).probe(&work),
            源(&rules, &上一版).probe(&work)
        );
    }

    /// 同一份 dump、同一批字段，只是**建索引时平台折得动与折不动**的两份索引。
    ///
    /// 折不动那一份就是用户遇到的那个场景：数据源写着一个两张表都不认的平台名，
    /// 条目身上那一串是空的，交叉校验说不出话，这个源整条产不出东西。
    fn 折不动那一版() -> zh::Index {
        let mut entries = 索引().entries().to_vec();
        for entry in &mut entries {
            entry.platforms.clear();
            entry.platform_text = "任天堂DS".to_string();
        }
        zh::Index::build(entries, "dump-2026-09-01".to_string())
            .with_platform_fold(zh::PlatformFold::default().line())
    }

    /// 补上那条别名重建之后的同一份索引：dump 没换、字段没换，只有折出来的那串变了。
    fn 折得动那一版() -> zh::Index {
        let mut entries = 索引().entries().to_vec();
        for entry in &mut entries {
            entry.platform_text = "任天堂DS".to_string();
        }
        let mut fold = zh::PlatformFold::default();
        fold.record("任天堂DS", "NDS");
        zh::Index::build(entries, "dump-2026-09-01".to_string()).with_platform_fold(fold.line())
    }

    #[test]
    fn 建索引时平台折成什么样进两层的输入指纹() {
        // **这一条是这次修复的正题。** 补一条平台别名、`zh sync --full` 重建索引之后，
        // 新折得动的那些条目该重采一遍。dump 是同一份、[`zh::store::FIELDS`] 一个字
        // 没变、匹配参数也没动——两个指纹从前一模一样，于是刮削整片复用旧的采集记录，
        // 那些条目永远不产出，而且不报错。
        let rules = Rules::builtin();
        let 折不动 = 折不动那一版();
        let 折得动 = 折得动那一版();
        // 前提先钉住：这两份索引除了折出来的那一串，别的一模一样。
        assert_eq!(折不动.dump(), 折得动.dump());
        assert_eq!(折不动.fields(), 折得动.fields());
        assert_ne!(折不动.platform_fold(), 折得动.platform_fold());

        let subject = 变体("nds/合金弹头7.7z", &[], &[]);
        assert_ne!(
            源(&rules, &折不动).probe(&subject),
            源(&rules, &折得动).probe(&subject),
            "变体那一层：折出来的那一串变了，指纹就得变"
        );
        let 名下变体 = [名下("nds/合金弹头7.7z")];
        let work = 作品(&名下变体);
        assert_ne!(
            源(&rules, &折不动).probe(&work),
            源(&rules, &折得动).probe(&work),
            "作品那一层同样盖得住它"
        );
    }

    #[test]
    fn 折不动的那一版一条中文名都产不出而折得动的产得出() {
        // 指纹必须跟着变的**理由**：这两份索引的产出真的不一样。
        // 不钉这一条，上面那条指纹测试只是在比两个字符串。
        let rules = Rules::builtin();
        let 折不动 = 折不动那一版();
        let 折得动 = 折得动那一版();
        let subject = 变体("nds/合金弹头7.7z", &[], &[]);

        let mut out = Harvest::default();
        源(&rules, &折不动)
            .collect(&subject, &mut out)
            .expect("本地源不该失败");
        assert!(
            out.values.is_empty(),
            "平台折不动时交叉校验说不出话，够不着中置信那一档，这个源无话可说"
        );

        let mut out = Harvest::default();
        源(&rules, &折得动)
            .collect(&subject, &mut out)
            .expect("本地源不该失败");
        assert_eq!(
            那一格(&out, Field::Title).map(|it| it.value.clone()),
            Some("合金弹头7".to_string()),
            "补上别名重建之后，同一个变体撞得上了"
        );
    }

    #[test]
    fn 老索引没记过折成什么样与一对都没折出来分得开() {
        // 升级到这一版之前建的索引，`meta` 里根本没有这一格，读回来是空串；
        // 而「这一趟一对都没折出来」写出来是 `共 0 对`。两者撞回同一个指纹的话，
        // 删掉最后一条别名重建之后会整片跳过。
        let rules = Rules::builtin();
        let 老索引 = 索引().with_platform_fold(String::new());
        let 一对都没折出来 = 索引().with_platform_fold(zh::PlatformFold::default().line());
        let subject = 变体("nds/合金弹头7.7z", &[], &[]);
        assert_ne!(
            源(&rules, &老索引).probe(&subject),
            源(&rules, &一对都没折出来).probe(&subject)
        );
    }

    #[test]
    fn 匹配参数进输入指纹() {
        // 门槛调了就该重采一遍。不盖它的话缓存会一口咬定「输入没变」而整条跳过。
        let rules = Rules::builtin();
        let index = 索引();
        let subject = 变体("nds/合金弹头7.7z", &[], &[]);
        let 名下变体 = [名下("nds/合金弹头7.7z")];
        let work = 作品(&名下变体);
        let 造 = |tuning| {
            ChineseSource::new(fuzzy::Naming {
                rules: &rules,
                index: Some(&index),
                tuning,
            })
        };
        let 松 = zh::Tuning {
            threshold: 0.5,
            ..zh::Tuning::default()
        };
        assert_ne!(
            造(zh::Tuning::default()).probe(&subject),
            造(松).probe(&subject)
        );
        assert_ne!(造(zh::Tuning::default()).probe(&work), 造(松).probe(&work));
    }

    /// 一份**补过一条正题噪音词**的剥离规则：`甲组特供版` 内置那份不认。
    ///
    /// 走文件那条路（`Rules::load`），因为用户就是这么补的——`--name-rules <文件>`。
    fn 补过的规则() -> (crate::testing::TempDir, Rules) {
        let dir = crate::testing::temp_dir("zh-name-rules");
        let path = dir.path().join("rules.toml");
        std::fs::write(&path, "\"版本\" = 1\n\"正题噪音词\" = [\"甲组特供版\"]\n").expect("写得下");
        let rules = Rules::load(&path).expect("读得进来");
        (dir, rules)
    }

    /// 剥离规则改了才撞得上的那个变体键。
    const 待剥的键: &str = "nds/合金弹头7甲组特供版.7z";

    #[test]
    fn 剥离规则进两层的输入指纹() {
        // **这一条是这张票的正题。** 剥离规则决定从文件名里剥出什么**正题**，
        // 而这一层撞的就是正题。用户照文档补一条自己遇到的模式再跑一趟 `scrape`
        // ——从前是整片复用旧的采集记录、一条新产出都没有，因为两层锚点的输入指纹
        // 盖的是 dump、取了哪几样字段、折出来的那张表与匹配参数，唯独没盖规则本身，
        // 而那几样一个字都没变。
        let 内置 = Rules::builtin();
        let (_dir, 补过) = 补过的规则();
        let index = 索引();
        let subject = 变体(待剥的键, &[], &[]);
        assert_ne!(
            源(&内置, &index).probe(&subject),
            源(&补过, &index).probe(&subject),
            "变体那一层：规则变了，指纹就得变"
        );
        let 名下变体 = [名下(待剥的键)];
        let work = 作品(&名下变体);
        assert_ne!(
            源(&内置, &index).probe(&work),
            源(&补过, &index).probe(&work),
            "作品那一层同样盖得住它"
        );
        // **规则没改时两层照旧是同一个指纹**：加这道判据不能让每一趟刮削都从头来。
        assert_eq!(
            源(&内置, &index).probe(&subject),
            源(&Rules::builtin(), &index).probe(&subject)
        );
        assert_eq!(
            源(&内置, &index).probe(&work),
            源(&Rules::builtin(), &index).probe(&work)
        );
    }

    #[test]
    fn 内置那份规则一条中文名都产不出而补过的产得出() {
        // 指纹必须跟着变的**理由**：这两份规则的产出真的不一样。
        // 不钉这一条，上面那条指纹测试只是在比两个字符串。
        let subject = 变体(待剥的键, &[], &[]);
        let index = 索引();

        let mut out = Harvest::default();
        源(&Rules::builtin(), &index)
            .collect(&subject, &mut out)
            .expect("本地源不该失败");
        assert!(
            out.values.is_empty(),
            "`甲组特供版` 剥不掉，正题撞不上任何条目，这个源无话可说"
        );

        let (_dir, 补过) = 补过的规则();
        let mut out = Harvest::default();
        源(&补过, &index)
            .collect(&subject, &mut out)
            .expect("本地源不该失败");
        assert_eq!(
            那一格(&out, Field::Title).map(|it| it.value.clone()),
            Some("合金弹头7".to_string()),
            "补上那条噪音词之后，同一个变体撞得上了"
        );
    }

    /// 数据源里那条简介的原样：开头两个**全角空格**、中间一个换行。
    const 简介原文: &str = "　　以细腻的画风讲了一个故事。\n第二段：故事讲完了。";

    #[test]
    fn 简介一个字都不改地落在作品锚点上() {
        // 规格 18 与挂单 Q3：换行、开头那两个全角空格、以及数据源自带的排版**原样保留**。
        // `str::trim` 把 U+3000 当空白扫掉，照仓库里别的字段那条惯例写就违反规格。
        let out = 采作品带简介(
            &[名下("nds/合金弹头7.7z")],
            &简介表(Some(简介原文.to_string())),
        )
        .expect("读得出来就不该失败");
        let got = 那一格(&out, Field::Description).expect("有这一条");
        assert_eq!(got.value, 简介原文, "简介被改动了");
        assert!(
            got.value.starts_with('\u{3000}'),
            "开头那两个全角空格被吃掉了"
        );
        // 依据说得出这条结论为什么挂在作品这一层。
        assert!(got.evidence.contains("条目 4"), "{}", got.evidence);
        assert!(
            got.evidence.contains("而简介跨平台跨地区都成立"),
            "{}",
            got.evidence
        );
        // 没超闸的那一条**不该**说自己被截断了。
        assert!(
            !got.evidence.contains("这条简介被截断了"),
            "{}",
            got.evidence
        );
        // 类型照旧在同一趟里产出——两个字段跟着同一次匹配走。
        assert_eq!(那一格(&out, Field::Genre).expect("有这一条").value, "ACT");
    }

    #[test]
    fn 同一条条目只留一份简介() {
        // 同一部作品的两个变体撞到同一条条目，作品锚点上**只有一份**简介，不是两份。
        let out = 采作品带简介(
            &[
                名下("nds/合金弹头7[某汉化组](简).7z"),
                名下("nds/合金弹头7.7z"),
            ],
            &简介表(Some(简介原文.to_string())),
        )
        .expect("读得出来就不该失败");
        let 几条 = out
            .values
            .iter()
            .filter(|it| it.field == Field::Description)
            .count();
        assert_eq!(几条, 1, "两个变体撞到同一条，只该有一份简介");
    }

    #[test]
    fn 恰好卡在闸上的那一条一个字都不截() {
        // 闸是「最多留这么多字」，不是「超过这么多就截」——边界那一条要留全。
        let 刚好 = "外".repeat(DESCRIPTION_LIMIT);
        let out = 采作品带简介(&[名下("nds/合金弹头7.7z")], &简介表(Some(刚好.clone())))
            .expect("读得出来就不该失败");
        let got = 那一格(&out, Field::Description).expect("有这一条");
        assert_eq!(got.value, 刚好);
        assert!(!got.value.contains(TRUNCATED_MARK));
    }

    #[test]
    fn 超过闸的那一条截到闸上而且留得下记号() {
        // 实测最长一条 9,962 字。截断这件事**写在值里**：值是导出到前端、用户真会读到
        // 的那一份，而依据只有回到工具里才看得见。
        let 超长 = "外".repeat(DESCRIPTION_LIMIT + 1);
        let out = 采作品带简介(&[名下("nds/合金弹头7.7z")], &简介表(Some(超长.clone())))
            .expect("读得出来就不该失败");
        let got = 那一格(&out, Field::Description).expect("有这一条");
        assert!(got.value.starts_with(&"外".repeat(DESCRIPTION_LIMIT)));
        assert!(got.value.contains(TRUNCATED_MARK), "记号丢了");
        // **按字取尾**：这一串是汉字，按字节切多半落在字符中间，一 panic 就顶掉了
        // 本该看见的那句说明。
        let 尾巴: String = got
            .value
            .chars()
            .rev()
            .take(40)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        assert!(
            got.value.contains(&format!("{} 字", DESCRIPTION_LIMIT + 1)),
            "说明里该写得出原文有多少字：{尾巴}",
        );
        // 正文那一段截到闸上，剩下的是那句说明——**总长有个死上界**。
        assert!(got.value.chars().count() < DESCRIPTION_LIMIT + 100);
        assert!(
            got.evidence.contains("这条简介被截断了"),
            "{}",
            got.evidence
        );
    }

    #[test]
    fn 数据源没写简介时无话可说而别的字段照旧产出() {
        // 缺一格不是错误——「照常产出它有的那些字段」（规格 21）。
        let out = 采作品带简介(&[名下("nds/合金弹头7.7z")], &简介表(None)).expect("不该失败");
        assert!(那一格(&out, Field::Description).is_none());
        assert_eq!(那一格(&out, Field::Genre).expect("有这一条").value, "ACT");
    }

    #[test]
    fn 简介读不动时整对不写库而不是当成没有简介() {
        // 吞成「这条没有简介」的话，一次读库失败会让整趟悄悄少一栏，而报告还说得
        // 像模像样；更糟的是缓存会把这一趟的空手当成结论，下一趟连重试都不会有。
        let got = 采作品带简介(&[名下("nds/合金弹头7.7z")], &读不动);
        assert!(
            matches!(got, Err(Failure::Skip { ref why }) if why.contains("库文件被截断了")),
            "{got:?}",
        );
    }

    #[test]
    fn 撞不上的作品一条简介都不产出() {
        // **宁可留空也不要写错的**：简介那条路每一条都答得出来，所以「没有简介」
        // 只可能是因为没撞上。
        let out = 采作品带简介(
            &[名下("nds/Only English.7z"), 名下("nds/谁也不认得的名字.7z")],
            &简介表(Some(简介原文.to_string())),
        )
        .expect("不该失败");
        assert!(out.values.is_empty(), "{:?}", out.values);
    }

    #[test]
    fn 开发商与发行商拆成多条落在作品锚点上() {
        // 票 04 的正题：数据源里一个键写了好几个值（顿号分隔的一行、或者多值块），
        // 到这一层就是**好几条**，不是一串带顿号的长字符串——用户按开发商筛的时候，
        // 「SNK、北斗」与「SNK」是两个不同的东西。
        let out = 采作品(&[名下("nds/合金弹头7.7z")]);
        assert_eq!(那几条(&out, Field::Developer), vec!["SNK", "北斗"]);
        // **只有一个值时就是一条**，没有多出空条目。
        assert_eq!(那几条(&out, Field::Publisher), vec!["世嘉"]);
        // 每一条都带**依据**，而且说得清它为什么挂在作品这一层。
        for found in out.values.iter().filter(|it| it.field == Field::Developer) {
            assert!(found.evidence.contains("条目 4"), "{}", found.evidence);
            assert!(
                found.evidence.contains("而开发商跨平台跨地区都成立"),
                "{}",
                found.evidence
            );
            // **中置信、照旧进待确认队列**：多两栏不等于自动通过（票 05 才管裁决）。
            assert!(
                found.evidence.contains("一律进待确认队列"),
                "{}",
                found.evidence
            );
        }
        // **不挂在变体上**：这两样跨平台跨地区都成立，挂到变体上就是每个变体各存一份。
        let 变体上的 = 采("nds/合金弹头7.7z", &[]);
        assert!(
            变体上的
                .values
                .iter()
                .all(|it| it.field != Field::Developer && it.field != Field::Publisher),
            "{:?}",
            变体上的.values
        );
    }

    #[test]
    fn 数据源缺发行这个键时照常产出它有的那些字段() {
        // 规格 21：**缺键就是没有，不是错误**。条目 6 写了开发没写发行。
        let out = 采作品(&[名下("nds/恶魔城.7z")]);
        assert_eq!(那几条(&out, Field::Developer), vec!["科乐美"]);
        assert!(
            那几条(&out, Field::Publisher).is_empty(),
            "缺的键就该是缺的"
        );
        assert_eq!(那一格(&out, Field::Genre).expect("有这一条").value, "AVG");
    }

    #[test]
    fn 拆出来的每一条都掐掉首尾空白而且不产出空串() {
        // 取数那一侧（`zh::dump::values`）已经掐过一遍，可手里这份索引也可能是**上一版
        // 程序**建的库读回来的。掐在产出这一处，这条纪律就与索引是谁建的无关。
        // 空串一条都不产出——空值会让优先级链在它身上停下来。
        let index = zh::Index::build(
            vec![zh::Entry {
                id: 4,
                name: "メタルスラッグ7".to_string(),
                name_cn: "合金弹头7".to_string(),
                year: Some(2008),
                platforms: vec!["NDS".to_string()],
                platform_text: "NDS".to_string(),
                developers: vec![
                    "  SNK  ".to_string(),
                    String::new(),
                    "　".to_string(),
                    "北斗\n".to_string(),
                ],
                ..zh::Entry::default()
            }],
            "dump-2026-09-01".to_string(),
        );
        let rules = Rules::builtin();
        let mut out = Harvest::default();
        源(&rules, &index)
            .collect(&作品(&[名下("nds/合金弹头7.7z")]), &mut out)
            .expect("这一趟没有会失败的动作");
        assert_eq!(那几条(&out, Field::Developer), vec!["SNK", "北斗"]);
    }

    #[test]
    fn 这一层产出的字段与进指纹的那一行对得上() {
        // `WORK_FIELDS` 进作品锚点的输入指纹。它与这一层**真的产出**的那几样一旦漂开，
        // 新接上来的字段就永远补不到已经采过的作品上——缓存会一口咬定「输入没变」。
        let out = 采作品带简介(
            &[名下("nds/合金弹头7.7z")],
            &简介表(Some(简介原文.to_string())),
        )
        .expect("读得出来就不该失败");
        let mut 产出: Vec<Field> = out.values.iter().map(|it| it.field).collect();
        产出.sort_unstable();
        产出.dedup();
        let mut 写着的 = WORK_FIELDS.to_vec();
        写着的.sort_unstable();
        assert_eq!(产出, 写着的);
    }

    #[test]
    fn 报告用的那份字段清单与两层真的产出的对得上() {
        // `FIELDS` 是报告的判据：「这一栏空着，跑一趟 `romcat zh sync` 有没有用」。
        // 它与真的产出漂开，报告就会把用户支去取一份补不了这一栏的索引（票 06）。
        let mut 写着的 = FIELDS.to_vec();
        写着的.sort_unstable();
        let mut 两层 = WORK_FIELDS.to_vec();
        两层.push(Field::Title);
        两层.sort_unstable();
        assert_eq!(写着的, 两层);
        // 变体那一层真的产出的就是中文名那一个（别名那一路也落在这个字段上）。
        let out = 采("nds/合金弹头7.7z", &[]);
        assert!(!out.values.is_empty(), "这个 fixture 本来就该撞得上");
        assert!(out.values.iter().all(|it| it.field == Field::Title));
    }

    #[test]
    fn 简介那条路在不在场进作品那一层的指纹() {
        // 不进的话，把这条路接上之后重跑，缓存会一口咬定「输入没变」而整条跳过——
        // 那些简介就永远补不上来了。
        let rules = Rules::builtin();
        let index = 索引();
        let 名下变体 = [名下("nds/合金弹头7.7z")];
        let work = 作品(&名下变体);
        let 没接上 = 源(&rules, &index);
        let 表 = 简介表(Some(简介原文.to_string()));
        let 接上了 = 没接上.with_summaries(&表);
        assert_ne!(没接上.probe(&work), 接上了.probe(&work));
        // **变体那一层不受影响**：简介是作品级的字段，那一层的输入一个字都没变。
        let subject = 变体("nds/合金弹头7.7z", &[], &[]);
        assert_eq!(没接上.probe(&subject), 接上了.probe(&subject));
    }

    #[test]
    fn 一条否定裁决把那条条目从这个变体的候选里划掉() {
        // 票 05：人说「不是这条」，中文名就不产出了——而中文名与别名、类型、简介、
        // 开发商、发行商是同一次匹配带来的，一条裁决全管。
        let key = "nds/合金弹头7[某汉化组](简).7z";
        assert!(!采(key, &[]).values.is_empty(), "没人裁过时本来是产出的");
        let out = 采带裁决(key, &裁过(key, 4, false));
        assert!(out.values.is_empty(), "{:?}", out.values);

        // **别名那一路跟着一起没有**：它们撞的是同一次。
        let rules = Rules::builtin();
        let index = 索引();
        let rulings = 裁过(key, 4, false);
        let mut 别名 = Harvest::default();
        ChineseAliasSource::new(fuzzy::Naming {
            rules: &rules,
            index: Some(&index),
            tuning: zh::Tuning::default(),
        })
        .with_rulings(&rulings)
        .collect(&变体(key, &[], &[]), &mut 别名)
        .expect("本地源不会失败");
        assert!(别名.values.is_empty(), "{:?}", 别名.values);
    }

    #[test]
    fn 否定的是这一条条目不是这个变体从此没有中文条目() {
        // 人只说了「4 不对」，没说「这个文件没有中文条目」。于是 4 从候选里划掉，
        // 剩下的照撞——撞出来的仍旧是**中置信**、仍旧进待确认队列。
        let key = "nds/恶魔城.7z";
        let out = 采带裁决(key, &裁过(key, 4, false));
        assert_eq!(out.values.len(), 1);
        assert_eq!(out.values[0].value, "恶魔城");
        assert!(
            out.values[0].evidence.contains("条目 6"),
            "{:?}",
            out.values[0]
        );
        assert!(out.values[0].evidence.contains("一律进待确认队列"));
    }

    #[test]
    fn 一条肯定裁决管住同一次匹配带来的全部字段() {
        // 六样字段散在两层锚点上，共通的只有那个条目号。一条裁决盖章，六样一起盖。
        let key = "nds/合金弹头7.7z";
        let rulings = 裁过(key, 4, true);
        let 变体产出 = 采带裁决(key, &rulings);
        assert_eq!(变体产出.values.len(), 1);
        assert!(zh::is_confirmed(&变体产出.values[0].evidence));
        assert!(!变体产出.values[0].evidence.contains("一律进待确认队列"));

        let 作品产出 = 采作品带裁决(&[名下(key)], &rulings);
        assert!(!作品产出.values.is_empty());
        for found in &作品产出.values {
            assert!(
                zh::is_confirmed(&found.evidence),
                "{:?} 该盖上同一个章",
                found.field
            );
        }
        // **值本身一个字都没变**：裁决改的是「这条结论算什么」，不是这条结论说什么。
        assert_eq!(那几条(&作品产出, Field::Genre), vec!["ACT"]);
        assert_eq!(那几条(&作品产出, Field::Developer), vec!["SNK", "北斗"]);
    }

    #[test]
    fn 人说过的那一条排在机器挑的前面() {
        // 不这么办的话，「肯定」这一档下一趟就被一个分数更高的候选顶掉了——
        // 而票 05 要的正是「裁决之后重跑刮削，结论稳定」。
        let key = "nds/合金弹头7.7z";
        // 没人裁过时撞的是条目 4；把条目 6 说成「就是它」，撞出来的就该是 6。
        assert!(采(key, &[]).values[0].evidence.contains("条目 4"));
        let out = 采带裁决(key, &裁过(key, 6, true));
        // 条目 6 在这个变体上够不着中置信，所以它进不了候选——人说了也白说，
        // 这一条钉的是**不许凭空造一条匹配出来**。
        assert!(
            out.values[0].evidence.contains("条目 4"),
            "{:?}",
            out.values[0]
        );
    }

    #[test]
    fn 作品那一层数票时不数被否定掉的那个变体() {
        // 名下两个变体，一个撞条目 4、一个撞条目 6。否定掉撞 4 的那个，
        // 作品这一层的答案就该翻成 6——它是名下变体数票数出来的。
        let 甲 = "nds/合金弹头7.7z";
        let 乙 = "nds/恶魔城.7z";
        let 没裁过 = 采作品(&[名下(甲), 名下(乙)]);
        // 平票时取条目号最小的那一条。
        assert_eq!(那几条(&没裁过, Field::Genre), vec!["ACT"]);

        let out = 采作品带裁决(&[名下(甲), 名下(乙)], &裁过(甲, 4, false));
        assert_eq!(那几条(&out, Field::Genre), vec!["AVG"]);
        assert_eq!(那几条(&out, Field::Developer), vec!["科乐美"]);
        let 依据 = &那一格(&out, Field::Genre).expect("有这一条").evidence;
        assert!(依据.contains("名下 1 个变体撞上了中文条目"), "{依据}");
    }

    #[test]
    fn 作品那一层人裁过的那一条压过数票() {
        // `romcat zh judge --yes` 的原话是「这一次匹配带来的全部字段一并定下」。
        // 让人裁的那一条与模糊匹配来的平起平坐去数票，就会出现「人在丙上盖了章，
        // 可甲乙撞的那条票多，作品那四栏照旧写着甲乙的答案」——命令自己的承诺当场
        // 落空，而且不报错。**裁决压过一切**是这个仓库既定的那条。
        let 甲 = "nds/合金弹头7.7z";
        let 乙 = "nds/合金弹头7[某汉化组](简).7z";
        let 丙 = "nds/恶魔城.7z";
        let 名下三个 = [名下(甲), 名下(乙), 名下(丙)];

        // 没人裁过：两票对一票，条目 4 赢。
        let 没裁过 = 采作品(&名下三个);
        assert_eq!(那几条(&没裁过, Field::Genre), vec!["ACT"]);

        // 人在丙上给条目 6 盖了章。票数仍是 2:1，可这一层的答案要翻成 6。
        let out = 采作品带裁决(&名下三个, &裁过(丙, 6, true));
        assert_eq!(
            那几条(&out, Field::Genre),
            vec!["AVG"],
            "人裁过的那一条压过数票"
        );
        assert_eq!(那几条(&out, Field::Developer), vec!["科乐美"]);
        // 盖过的章要跟到作品那四栏上——那正是「一条裁决管住全部字段」。
        let 依据 = &那一格(&out, Field::Genre).expect("有这一条").evidence;
        assert!(zh::is_confirmed(依据), "{依据}");
    }

    #[test]
    fn 作品那一层的章盖在人裁过的那个变体上而不是分数最高的那个() {
        // 作品级那四栏的依据是从**代表变体**那一条上抄下来的，而「盖没盖过章」正写在
        // 那句话的末尾。代表只按相似度挑的话，人裁的是甲、乙分数更高，那四栏就照旧写着
        // 「一律进待确认队列」——一条裁决管住全部字段这件事当场落空，而且不报错。
        let 高分 = "nds/合金弹头7.7z";
        let 低分 = "nds/合金弹头7[某汉化组](简).7z";
        let 名下 = [名下(高分), 名下(低分)];
        // 两个变体撞的是同一条条目、分数也平手，没人裁过时代表取变体键最小的那个。
        let 没裁过 = 采作品(&名下);
        assert!(!zh::is_confirmed(
            &那一格(&没裁过, Field::Genre).expect("有这一条").evidence
        ));
        // 裁**键更大**的那一个：代表该翻到它身上，那四栏才盖得上章。
        let out = 采作品带裁决(&名下, &裁过(低分, 4, true));
        for found in &out.values {
            assert!(
                zh::is_confirmed(&found.evidence),
                "{:?} 该盖上章：{}",
                found.field,
                found.evidence
            );
        }
        assert!(
            那一格(&out, Field::Genre)
                .expect("有这一条")
                .evidence
                .contains(&format!("名下的变体「{低分}」")),
            "代表该是人裁过的那一个"
        );
    }

    #[test]
    fn 匹配裁决进两层的输入指纹() {
        // 不进的话，人裁完重跑一趟，缓存会一口咬定「输入没变」而整条跳过——
        // 那条被否定掉的中文名就永远撞回来。
        let rules = Rules::builtin();
        let index = 索引();
        let key = "nds/合金弹头7.7z";
        let 名下变体 = [名下(key)];
        let work = 作品(&名下变体);
        let subject = 变体(key, &[], &[]);
        let 空 = Rulings::none();
        let 没裁过 = 源(&rules, &index).with_rulings(&空);
        let 否 = 裁过(key, 4, false);
        let 准 = 裁过(key, 4, true);
        let 裁否了 = 源(&rules, &index).with_rulings(&否);
        let 裁准了 = 源(&rules, &index).with_rulings(&准);
        for (甲, 乙) in [(没裁过, 裁否了), (没裁过, 裁准了), (裁否了, 裁准了)] {
            assert_ne!(甲.probe(&subject), 乙.probe(&subject), "变体那一层该重采");
            assert_ne!(甲.probe(&work), 乙.probe(&work), "作品那一层也该重采");
        }
        // **别人的裁决不算数**：裁的是另一个变体，这一个的指纹一个字都不该变。
        let 别人 = 裁过("nds/别的.7z", 4, false);
        assert_eq!(
            没裁过.probe(&subject),
            源(&rules, &index).with_rulings(&别人).probe(&subject)
        );
    }
}
