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
//! `best`），分成两个名字纯粹是为了让[优先级表](super::priority)排得动它们——别名垫在
//! 标题那条链的最后，**只进集合、只管搜得到，永远轮不到它当显示标题**。
//!
//! 合成一个源名的话，两者在库里是同一个源的几条值，排序上完全平手，最后按字典序定
//! 胜负：`合金彈頭7` 与 `合金弹头7` 谁当显示标题全看码位。那不是一条判据。
//!
//! **别名那一路只在变体这一层说话**：一部作品的几个叫法该跟着那个撞上它的文件走，
//! 顺手搬到作品锚点上只会让同一串字在库里多躺一份、还多一条说不清是谁撞出来的依据。

use std::collections::BTreeMap;

use crate::identify::fuzzy;
use crate::identify::naming;
use crate::zh;

use super::{AnchorKind, DatEntry, Failure, Field, Harvest, Locality, Source, Subject};

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
}

impl Hit {
    /// 这一条的**依据**，写成给人看的一句。
    fn evidence(&self, dump: &str) -> String {
        self.one.evidence(dump, self.label, &self.text)
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
}

impl<'a> ChineseSource<'a> {
    /// 造一个。**调用方保证索引在场**（`sources()` 只在取过数之后造它）；
    /// 万一不在场，这个源一句话都不说，而不是给一个没有出处的中文名。
    #[must_use]
    pub fn new(naming: fuzzy::Naming<'a>) -> Self {
        Self { naming }
    }

    /// 这个**变体**锚点上撞得出哪一条。
    fn best(&self, subject: &Subject<'_>) -> Option<Hit> {
        self.hit(subject.main_key?, subject.platform, subject.entries)
    }

    /// 拿一个变体的那几样撞一次。
    ///
    /// 参数表摊开成三样而不是收一个 [`Subject`]：作品那一层撞的是[名下的变体](
    /// super::WorkVariant)，手里根本没有那些变体的 `Subject`。
    fn hit(&self, main_key: &str, platform: Option<&str>, entries: &[DatEntry]) -> Option<Hit> {
        let index = self.naming.index?;
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
                if best.as_ref().is_none_or(|seen| one.score > seen.one.score) {
                    best = Some(Hit {
                        one,
                        text: text.to_string(),
                        label,
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
        let mut tally: BTreeMap<u32, usize> = BTreeMap::new();
        for (_, hit) in &hits {
            *tally.entry(hit.one.entry.id).or_default() += 1;
        }
        // 键取 `(票数, 条目号取反)`：**这个最大值是唯一的**，所以「最大值有好几个时
        // 取哪一个」这种依赖遍历顺序的事根本不会发生。
        let (&winner, &votes) = tally
            .iter()
            .max_by_key(|(id, count)| (**count, std::cmp::Reverse(**id)))?;
        // 胜出那条条目名下可能有好几个变体撞上，挑一个当**依据**里的代表：相似度最高的
        // 那个；相似度也平手时取**变体键最小**的那个。后半句不能省、也不能改成「取先
        // 遍历到的那个」——那样这条结论就挂在调用方摆进来的次序上，而这个函数自己保证
        // 不了那件事。按键定序，它与次序无关。
        let mut chosen: Option<&(&str, Hit)> = None;
        for got in hits.iter().filter(|(_, hit)| hit.one.entry.id == winner) {
            let better = match chosen {
                None => true,
                Some(&(key, ref seen)) => match got.1.one.score.partial_cmp(&seen.one.score) {
                    Some(std::cmp::Ordering::Greater) => true,
                    Some(std::cmp::Ordering::Equal) => got.0 < key,
                    _ => false,
                },
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
        // 用的是哪一版 dump、**这一版索引从数据源里取了哪几样**、以及**匹配参数**——
        // 门槛从 0.85 调到 0.80 该重采一遍，取的字段从五样变成九样也该重采一遍，
        // 不盖它们的话缓存会一口咬定「输入没变」而整条跳过。
        let platform = subject.platform.unwrap_or("");
        let tuning = self.naming.tuning.fingerprint();
        // 已经撞上的 DAT 条目名也进指纹：年份从它们里读。
        let entries: Vec<&str> = subject
            .entries
            .iter()
            .map(|entry| entry.game.as_str())
            .collect();
        let mut parts = vec![
            main,
            platform,
            self.naming.index.map_or("", zh::Index::dump),
            self.naming.index.map_or("", zh::Index::fields),
            tuning.as_str(),
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
            self.naming.tuning.fingerprint(),
        ];
        for variant in subject.variants {
            parts.push(variant.main_key.clone());
            parts.push(variant.platform.clone().unwrap_or_default());
            // **条目数也进去**：少了它，「一个变体带一条 DAT 条目」与「两个变体、
            // 后一个身上一条条目都没有」在这串里长得一模一样。
            parts.push(variant.entries.len().to_string());
            parts.extend(variant.entries.iter().map(|entry| entry.game.clone()));
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
    /// 眼下只有**类型**（票 02 用它把「撞在变体层、挂在作品层」这条路跑通）。
    /// 简介、开发商、发行商分别是票 03 与票 04 的活，接在同一条路上。
    fn collect_work(&self, subject: &Subject<'_>, out: &mut Harvest) {
        let Some(won) = self.work_hit(subject) else {
            return;
        };
        // **类型是单值字段**：`Priorities::merge` 一个字段只回一个值，而集合那条路
        // （`Harvest::each` 加 `title::fold`）眼下只有标题走得通。条目的 infobox 里
        // 写了好几个类型时取头一个——数据源里的原次序，同一份库跑两次取的是同一个。
        let Some(genre) = won.hit.one.entry.genres.first() else {
            return;
        };
        let dump = self.naming.index.map_or("", zh::Index::dump);
        out.value(
            Field::Genre,
            genre.clone(),
            format!(
                "{}；**这条结论挂在作品这一层**：撞是名下的变体「{}」撞的，\
                 而类型跨平台跨地区都成立（名下 {} 个变体撞上了中文条目，\
                 其中 {} 个撞的是这一条）",
                won.hit.evidence(dump),
                won.from,
                won.matched,
                won.votes,
            ),
        );
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
            AnchorKind::Variant => self.collect_variant(subject, out),
            AnchorKind::Work => self.collect_work(subject, out),
        }
        // 本地源没有会失败的动作：索引整份在内存里。
        Ok(())
    }
}

/// **中文离线源的别名那一路**：撞上的那条条目**还叫什么**。
///
/// ## 为什么它是一个单独的源，而不是上面那个源多说几句
///
/// 两路给的东西在标题这一栏上的分量差得远：中文名是「这个文件的正题撞上的那个名字」，
/// 别名是「那条条目还叫什么」。分成两个源名，[优先级表](super::priority)就排得动它们
/// ——别名排在整条链的最后，**只进标题集合、只管搜得到，永远轮不到它当显示标题**。
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
    use crate::scrape::{DatEntry, LocalMedia, WorkVariant};

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

    /// 在**作品**锚点上采一趟。
    fn 采作品(variants: &[WorkVariant]) -> Harvest {
        let rules = Rules::builtin();
        let index = 索引();
        let mut out = Harvest::default();
        源(&rules, &index)
            .collect(&作品(variants), &mut out)
            .expect("本地源不会失败");
        out
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
        assert_eq!(out.values.len(), 1, "两个变体撞到同一条，只该有一份类型");
        assert_eq!(out.values[0].field, Field::Genre);
        assert_eq!(out.values[0].value, "ACT");
        let 依据 = &out.values[0].evidence;
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
        assert!(源(&rules, &index).probe(&作品(&[名下("nds/x.7z")])).is_some());
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
        assert_eq!(out.values.len(), 1);
        assert_eq!(out.values[0].value, "AVG", "两票的条目 6 该胜过一票的条目 4");
        assert!(
            out.values[0]
                .evidence
                .contains("名下 3 个变体撞上了中文条目，其中 2 个撞的是这一条"),
            "{}",
            out.values[0].evidence
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
        assert_eq!(正.values.len(), 1);
        assert_eq!(正.values[0].value, "ACT", "平票该取条目号小的那条（4 < 6）");
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
        assert_ne!(源(&rules, &现在).probe(&work), 源(&rules, &上一版).probe(&work));
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
}
