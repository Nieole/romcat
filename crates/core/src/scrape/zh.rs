//! **中文离线源**：把变体的文件名撞进中文离线数据源，取一个**有出处的中文名**。
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

use crate::filename::Rules;
use crate::identify::fuzzy;
use crate::identify::naming;
use crate::zh;

use super::{AnchorKind, Failure, Field, Harvest, Locality, Source, Subject};

/// 中文离线源。
///
/// 它借着索引与规则活着（两样都是整份装在内存里的，一次跑几万个锚点），
/// 所以带生命周期而不是自己拥有一份。
#[derive(Debug, Clone, Copy)]
pub struct ChineseSource<'a> {
    rules: &'a Rules,
    index: &'a zh::Index,
    tuning: zh::Tuning,
}

impl<'a> ChineseSource<'a> {
    /// 造一个。
    #[must_use]
    pub fn new(rules: &'a Rules, index: &'a zh::Index, tuning: zh::Tuning) -> Self {
        Self {
            rules,
            index,
            tuning,
        }
    }

    /// 这个锚点上撞得出哪一条。
    fn best(&self, subject: &Subject<'_>) -> Option<(zh::Match, String, &'static str)> {
        let name = crate::path::file_name_of_key(subject.main_key?);
        // **乱码不撞**（同 `identify::fuzzy`）：有损转换留下的替换字符一进来就把相似度
        // 算成一团糟，而撞出来的东西没人分辨得了对错。
        if name.contains('\u{FFFD}') {
            return None;
        }
        let parsed = self.rules.parse(name);
        // 年份这一侧：文件名里明写的，或者**已经撞上的那条 DAT 条目名**里读出来的。
        // 后者是这一侧比识别那一层多出来的弹药——已确认的变体身上往往有 TOSEC 的条目名，
        // 而 TOSEC 的第一个括号就是发行日期。
        let year = parsed.year.or_else(|| {
            subject
                .entries
                .iter()
                .filter_map(|entry| naming::tosec_year(&entry.game))
                .find_map(|year| year.parse::<u16>().ok())
        });
        let mut best: Option<(zh::Match, String, &'static str)> = None;
        for (label, text) in parsed.queries() {
            for one in self.index.lookup(
                &zh::Query {
                    text,
                    platform: subject.platform,
                    year,
                },
                &self.tuning,
            ) {
                // **只收够得着中置信的那一档**，而且它得真有一个中文名——
                // 条目自己都没写中文名时，这个源无话可说。
                if !one.strong(&self.tuning) || one.entry.name_cn.trim().is_empty() {
                    continue;
                }
                if best
                    .as_ref()
                    .is_none_or(|(seen, _, _)| one.score > seen.score)
                {
                    best = Some((one, text.to_string(), label));
                }
            }
        }
        best
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
        if subject.kind != AnchorKind::Variant {
            return None;
        }
        let main = subject.main_key?;
        // 指纹要盖住**一切会改变结果的东西**（`Source::probe` 的文档）：名字、平台、
        // 用的是哪一版 dump、以及**匹配参数**——门槛从 0.85 调到 0.80 该重采一遍，
        // 不盖它的话缓存会一口咬定「输入没变」而整条跳过。
        let platform = subject.platform.unwrap_or("");
        let tuning = format!(
            "{}|{}|{}|{}",
            self.tuning.threshold, self.tuning.strong, self.tuning.limit, self.tuning.year_slack
        );
        // 已经撞上的 DAT 条目名也进指纹：年份从它们里读。
        let entries: Vec<&str> = subject
            .entries
            .iter()
            .map(|entry| entry.game.as_str())
            .collect();
        let mut parts = vec![main, platform, self.index.dump(), tuning.as_str()];
        parts.extend(entries);
        Some(super::fingerprint(&parts))
    }

    fn collect(&self, subject: &Subject<'_>, out: &mut Harvest) -> Result<(), Failure> {
        let Some((one, text, label)) = self.best(subject) else {
            return Ok(());
        };
        out.value(
            Field::Title,
            one.entry.name_cn.clone(),
            one.evidence(self.index.dump(), label, &text),
        );
        // 本地源没有会失败的动作：索引整份在内存里。
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scrape::{DatEntry, LocalMedia};

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
                },
                zh::Entry {
                    id: 5,
                    name: "Only English".to_string(),
                    name_cn: String::new(),
                    aliases: Vec::new(),
                    year: Some(2000),
                    platforms: vec!["NDS".to_string()],
                    platform_text: "NDS".to_string(),
                },
            ],
            "dump-2026-09-01".to_string(),
        )
    }

    fn 变体<'a>(key: &'a str, entries: &'a [DatEntry], media: &'a [LocalMedia]) -> Subject<'a> {
        Subject {
            kind: AnchorKind::Variant,
            id: key,
            platform: Some("NDS"),
            entries,
            main_key: Some(key),
            media,
            media_limit: None,
            confirmed: true,
            basis: None,
        }
    }

    fn 采(key: &str, entries: &[DatEntry]) -> Harvest {
        let rules = Rules::builtin();
        let index = 索引();
        let source = ChineseSource::new(&rules, &index, zh::Tuning::default());
        let mut out = Harvest::default();
        source
            .collect(&变体(key, entries, &[]), &mut out)
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
        let source = ChineseSource::new(&rules, &index, zh::Tuning::default());
        let entries = vec![DatEntry {
            source: "TOSEC".to_string(),
            game: "Metal Slug 7 (2008)(SNK)".to_string(),
        }];
        let subject = 变体("nds/合金弹头7.7z", &entries, &[]);
        let (one, _, _) = source.best(&subject).expect("撞得上");
        assert_eq!(one.year, zh::Check::Agrees);
    }

    #[test]
    fn 只在变体这一层说话() {
        let rules = Rules::builtin();
        let index = 索引();
        let source = ChineseSource::new(&rules, &index, zh::Tuning::default());
        let mut work = 变体("合金弹头7", &[], &[]);
        work.kind = AnchorKind::Work;
        assert!(source.probe(&work).is_none());
    }

    #[test]
    fn 匹配参数进输入指纹() {
        // 门槛调了就该重采一遍。不盖它的话缓存会一口咬定「输入没变」而整条跳过。
        let rules = Rules::builtin();
        let index = 索引();
        let subject = 变体("nds/合金弹头7.7z", &[], &[]);
        let a = ChineseSource::new(&rules, &index, zh::Tuning::default()).probe(&subject);
        let tuning = zh::Tuning {
            threshold: 0.5,
            ..zh::Tuning::default()
        };
        let b = ChineseSource::new(&rules, &index, tuning).probe(&subject);
        assert_ne!(a, b);
    }
}
