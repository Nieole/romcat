//! **标题报告**：标题集合里有什么，以及挑出来的显示标题与排序标题长什么样。
//!
//! 与体检、命中率、刮削那几份报告同一个形状——**从中立库折出来**（ADR-0001），
//! 不重跑一遍刮削，也不碰主库一个字节。
//!
//! ## 三件必须说出口的事
//!
//! 1. **多少个作品的标题集合里有中文叫法。** 这个数回答的是**刮削到底采到了多少
//!    中文**，所以它数的是**标题集合**，不是**显示标题**——显示标题是后一步的选择
//!    （裁决可以把它定成英文名），采到的中文不该被那一步抹掉（挂账 `D163`）。
//!    它与详情面板上「中文标题取的是这一条」摆的是同一件事：
//!    [`title::best_chinese`](super::best_chinese) 那一条在不在。
//! 2. **那些中文名各是哪一档来的。** 官中的官方译名、库里的中文文件名、汉化组自取的
//!    名字，三者的可信程度差得很远（ADR-0012），混成一个数就看不出哪些该复核。
//! 3. **哪些作品排不动。** 一个拉丁标题都没有的作品，排序标题只能退回中文显示标题，
//!    而那正是「按码位排等于乱排」的那一档。**点名**比悄悄排掉强。

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Serialize;

use crate::catalog::{Catalog, CatalogError, Confidence};
use crate::report::{heading, pad, thousands};
use crate::scrape::priority::Priorities;

use super::{
    Chosen, Language, SortFrom, TitleKind, TitleRow, TitleSet, best_chinese, choose, work_titles,
};

/// 报告里最多列几个例子。
const EXAMPLES: usize = 10;

/// 一个中文叫法的例子。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TitleExample {
    /// 哪个作品。
    pub work: String,
    /// 摆出来的那一条叫法。中文那几节里它是**中文那一档的第一名**，不一定是显示标题。
    pub display: String,
    /// 这个作品的排序标题。
    pub sort: String,
    /// 这个名字来自哪个变体；发行版级的叫法没有变体，是 `None`。
    ///
    /// **它不是依据那句话的重复**：依据是给人读的一句散文，这一列是**机器读得动的
    /// 那一半**——裁决要从一条中文名跳到它来自的那个文件，靠正则去散文里抠路径，
    /// 正是「没有依据的结论无法复核」要避开的做法。
    pub variant: Option<String>,
    /// **依据**。
    pub evidence: String,
}

/// 同一部作品有好几个中文叫法的例子。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RivalExample {
    /// 哪个作品。
    pub work: String,
    /// 选中的那个。
    pub chosen: String,
    /// 落选的那些。
    pub others: Vec<String>,
    /// **依据**：凭什么选中的那个。
    pub evidence: String,
}

/// 一份标题报告。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TitleReport {
    /// 中立库在哪。
    pub catalog: String,
    /// 一共几个作品。
    pub works: u64,
    /// 其中几个作品的标题集合非空。
    pub works_with_titles: u64,
    /// 标题集合一共几条叫法。
    pub entries: u64,
    /// 集合里按语言：`(语言, 条数)`。
    pub by_language: Vec<(String, u64)>,
    /// 集合里按类型：`(类型, 条数)`。
    pub by_kind: Vec<(String, u64)>,
    /// 集合里按来源：`(源, 条数)`，多的排前面。
    pub by_source: Vec<(String, u64)>,
    /// 显示标题按语言：`(语言, 作品数)`。
    pub display_by_language: Vec<(String, u64)>,
    /// 标题集合是空的、显示标题退回作品名的作品有几个。
    pub display_from_work_name: u64,
    /// **标题集合里有中文叫法的作品数——「中文覆盖」就是这个数。**
    ///
    /// 数的是**标题集合**，不是**显示标题**：刮削采到了中文，它就算，哪怕人裁决把
    /// 显示标题定成了英文名（挂账 `D163`）。判据与详情面板摆的那一条是同一个
    /// （[`title::best_chinese`](super::best_chinese)）。
    pub chinese_works: u64,
    /// 其中**显示标题不是中文**的有几个。
    ///
    /// 报告里同时有「中文覆盖」与[显示标题按语言](Self::display_by_language)两个数，
    /// 两者本来就不相等，这一个是差出来的一半。不说出来，看报告的人会以为其中一处是 bug。
    ///
    /// **两个数之间不是一条减法。** 另一半来自反方向：标题集合是空的时候显示标题退回
    /// **作品名**（[`choose`]），作品名带汉字就按中文计——那种作品进得了
    /// 「显示标题按语言」的中文那一栏，却一条中文**叫法**都没采到，进不了这里。
    /// 那一族的上界是 [`display_from_work_name`](Self::display_from_work_name)。
    pub chinese_not_displayed: u64,
    /// 那些中文叫法按类型：`(类型, 作品数)`。
    pub chinese_by_kind: Vec<(String, u64)>,
    /// 那些中文叫法按置信度：`(置信度, 作品数)`。
    pub chinese_by_confidence: Vec<(String, u64)>,
    /// 官方译名走的是**世代裂缝**的哪一侧：`(那一侧, 作品数)`（ADR-0019）。
    pub chinese_by_seam: Vec<(String, u64)>,
    /// 官中译名的例子。
    pub official_examples: Vec<TitleExample>,
    /// 有不止一个中文叫法、因而**选定规则真的起了作用**的作品有几个。
    pub rival_works: u64,
    /// 那种作品的例子。
    pub rival_examples: Vec<RivalExample>,
    /// 排序标题各是从哪儿来的：`(来路, 作品数)`。
    pub sort_from: Vec<(String, u64)>,
    /// **一个拉丁标题都没有**、排序标题只能退回显示标题的作品有几个。
    pub unsortable_works: u64,
    /// 那种作品的例子。
    pub unsortable_examples: Vec<String>,
    /// **低置信的中文叫法**有几个——它们是待确认队列的输入（ADR-0002）。
    pub queue_works: u64,
    /// 队列里的例子。
    pub queue_examples: Vec<TitleExample>,
}

impl TitleReport {
    /// 从中立库折出报告。**不碰主库、不联网。**
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn build(catalog: &Catalog, priorities: &Priorities) -> Result<Self, CatalogError> {
        let mut report = Self {
            catalog: catalog.location().to_string(),
            ..Self::default()
        };

        // **只走一趟标题集合。** 集合那几个计数（按语言 / 类型 / 来源）与逐个作品挑
        // 显示标题要的是同一批行，分两趟读只是把 28,872 行读两遍。
        let sets = work_titles(catalog)?;
        report.works = u64::try_from(sets.len()).unwrap_or(u64::MAX);
        let mut by_language: BTreeMap<&str, u64> = BTreeMap::new();
        let mut by_kind: BTreeMap<&str, u64> = BTreeMap::new();
        let mut by_source: BTreeMap<&str, u64> = BTreeMap::new();
        let mut display_language: BTreeMap<&str, u64> = BTreeMap::new();
        let mut chinese_kind: BTreeMap<&str, u64> = BTreeMap::new();
        let mut chinese_confidence: BTreeMap<&str, u64> = BTreeMap::new();
        let mut chinese_seam: BTreeMap<&str, u64> = BTreeMap::new();
        let mut sort_from: BTreeMap<&str, u64> = BTreeMap::new();
        for set in &sets {
            if !set.entries.is_empty() {
                report.works_with_titles += 1;
            }
            report.entries += u64::try_from(set.entries.len()).unwrap_or(0);
            for row in &set.entries {
                *by_language.entry(row.language.label()).or_default() += 1;
                *by_kind.entry(row.kind.label()).or_default() += 1;
                *by_source.entry(row.source.as_str()).or_default() += 1;
            }
            let chosen = choose(set, priorities);
            *display_language.entry(chosen.language.label()).or_default() += 1;
            *sort_from.entry(chosen.sort_from.label()).or_default() += 1;
            if chosen.kind.is_none() {
                report.display_from_work_name += 1;
            }
            if chosen.sort_from == SortFrom::None {
                report.unsortable_works += 1;
                if report.unsortable_examples.len() < EXAMPLES {
                    report.unsortable_examples.push(chosen.display.clone());
                }
            }
            // **中文覆盖数的是标题集合，不是显示标题**（挂账 `D163`）。判据用
            // `best_chinese` 而不是 `chosen.language`：显示标题被裁成英文名之后，
            // 刮削采到的那条中文叫法照旧在集合里、详情面板也照旧摆着它，报告再把它
            // 抹掉，两处就各说一套。下面这几栏跟着一起改口径——它们要与总数对得上。
            if let Some(best) = best_chinese(set, priorities) {
                report.chinese_works += 1;
                if chosen.language != Language::Chinese {
                    report.chinese_not_displayed += 1;
                }
                *chinese_kind.entry(best.kind.label()).or_default() += 1;
                *chinese_confidence
                    .entry(best.confidence.label())
                    .or_default() += 1;
                if best.confidence == Confidence::Low {
                    report.queue_works += 1;
                    push_example(&mut report.queue_examples, set, &chosen, best);
                }
                if let Some(seam) = best.seam {
                    *chinese_seam.entry(seam.label()).or_default() += 1;
                    push_example(&mut report.official_examples, set, &chosen, best);
                }
                if chosen.chinese_names > 1 {
                    report.rival_works += 1;
                    push_rival(&mut report.rival_examples, set, best);
                }
            }
        }
        report.by_language = pick(&by_language, &Language::all().map(Language::label));
        report.by_kind = pick(&by_kind, &TitleKind::all().map(TitleKind::label));
        let mut sources: Vec<(String, u64)> = by_name(&by_source);
        // 来源按贡献多少排，多的在前——「哪个源在给这个库起名字」一眼看得出来。
        sources.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        report.by_source = sources;
        report.display_by_language = pick(&display_language, &Language::all().map(Language::label));
        report.chinese_by_kind = by_name(&chinese_kind);
        report.chinese_by_confidence = by_name(&chinese_confidence);
        report.chinese_by_seam = by_name(&chinese_seam);
        report.sort_from = by_name(&sort_from);
        Ok(report)
    }

    /// 渲染成给人看的文本。
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "标题集合与显示、排序标题");
        let _ = writeln!(out, "{}", "═".repeat(28));
        let _ = writeln!(out, "中立库          {}", self.catalog);
        let _ = writeln!(
            out,
            "作品            {} 个，其中 {} 个的标题集合非空",
            thousands(self.works),
            thousands(self.works_with_titles),
        );
        let _ = writeln!(
            out,
            "叫法            {} 条——**标题在中立库里永远是集合，不是一个单值字段**",
            thousands(self.entries),
        );

        heading(&mut out, "中文覆盖");
        let _ = writeln!(
            out,
            "**{} 个作品的标题集合里有中文叫法**（一共 {} 个作品）。\n\
             数的是**标题集合**里有没有中文，不是**显示标题**是不是中文——\
             显示标题是后一步的选择（裁决可以把它定成英文名），\n\
             而这个数要回答的是**刮削到底采到了多少中文**，不该被后一步抹掉。",
            thousands(self.chinese_works),
            thousands(self.works),
        );
        if self.chinese_not_displayed > 0 {
            let _ = writeln!(
                out,
                "其中 **{} 个作品的显示标题不是中文**——那条中文叫法照旧在集合里，\
                 详情面板摆的也照旧是它。\n所以下面「显示标题的回退链」那一栏的中文数\
                 **与这里对不上，而那是对的**：那一栏问的是「最后挑了哪一条」，\
                 这里问的是「采到了没有」。",
                thousands(self.chinese_not_displayed),
            );
        }
        rows(&mut out, "按类型", &self.chinese_by_kind);
        rows(&mut out, "按置信度", &self.chinese_by_confidence);
        if self.chinese_by_seam.is_empty() {
            let _ = writeln!(
                out,
                "没有一条中文名是从官中发行版上取到的——库里还没有认出官中版，\
                 眼下的中文名都来自文件名。"
            );
        } else {
            rows(&mut out, "官方译名的来路", &self.chinese_by_seam);
            let _ = writeln!(
                out,
                "\n上面那两侧是 ADR-0019 那道**世代裂缝**：卡带与光盘世代的官中是**独立一条\n\
                 发行版**（有自己的地区与序列号），数字世代的中文只是**同一条发行版的语言\n\
                 属性**（港服与美服共用同一个 TitleID）。**这道裂缝是现实的裂缝，不是模型的\n\
                 缺陷**——抹平它就要在某一侧撒谎。"
            );
        }
        examples(&mut out, "官方译名的例子", &self.official_examples);

        heading(&mut out, "显示标题的回退链");
        let _ = writeln!(
            out,
            "中文 > 官方英文名 > 日文原名 > 文件名；中文那一档内部再按\
             **官中的官方译名 > 官方名 > 别名 > 汉化组自取的名**（ADR-0012）。"
        );
        for (language, count) in &self.display_by_language {
            let _ = writeln!(out, "{}{}", pad(language, 10), thousands(*count));
        }
        if self.display_from_work_name > 0 {
            let _ = writeln!(
                out,
                "其中 {} 个作品一条叫法都没有，显示标题退回作品名。",
                thousands(self.display_from_work_name)
            );
        }

        heading(&mut out, "同一部作品有好几个中文名时挑哪一个");
        let _ = writeln!(
            out,
            "{} 个作品有不止一个中文叫法。选定规则**七层**，确定且可复现：\n\
             **人工裁决** > **「那条条目还叫什么」垫底**（中文离线源同一次撞上的那条\
             条目的别的叫法，进集合只为搜得到）> **语言与类型的档位**（中文的译名 > \
             官方名 > 别名 > 汉化组自取的名，然后才是官方英文名、日文原名）> \n\
             **置信度** > **优先级表里的源名次** > **有几个变体这么叫** > **字典序**。",
            thousands(self.rival_works),
        );
        for example in &self.rival_examples {
            let _ = writeln!(
                out,
                "  {} → 「{}」（落选：{}）\n    {}",
                example.work,
                example.chosen,
                example.others.join("、"),
                example.evidence,
            );
        }

        heading(&mut out, "排序标题");
        let _ = writeln!(
            out,
            "**与显示标题分开生成**：中文显示标题按 Unicode 码位排等于乱排，\
             所以**排序标题另取一个拉丁标题**，绝不拿中文名去排。"
        );
        for (from, count) in &self.sort_from {
            let _ = writeln!(out, "{}{}", pad(from, 20), thousands(*count));
        }
        if self.unsortable_works > 0 {
            let _ = writeln!(
                out,
                "\n**有 {} 个作品一个拉丁标题都没有**，排序标题只能退回显示标题——\
                 那一档就是按码位排的，\n要人工补一个英文名或拼音。例如：{}",
                thousands(self.unsortable_works),
                self.unsortable_examples.join("、"),
            );
        }

        heading(&mut out, "标题集合");
        rows(&mut out, "按语言", &self.by_language);
        rows(&mut out, "按类型", &self.by_kind);
        rows(&mut out, "按来源", &self.by_source);

        heading(&mut out, "低置信的中文名——待确认队列的输入");
        let _ = writeln!(
            out,
            "{} 个作品的中文叫法是**低置信**的：没有任何官中发行版为它背书，\
             它只是盘上那个文件叫这个。\n\
             合集包、精简版、带广告后缀的文件名全落在这一档，**照用但标记**\
             （ADR-0002 的低置信那一档）。\n\
             上面「按置信度」那一栏里的**中置信**是另一回事：那些名字的**来源**确凿\
             （官中发行版背书、或者确是某个汉化版），\n只是**字面**还没人看过一眼——\
             官方译名不在 DAT 里，中文名只存在于盘上那个文件的名字上。\n\
             **高置信这一档留给裁决**：人看过才叫确凿。",
            thousands(self.queue_works),
        );
        examples(&mut out, "队列里的例子", &self.queue_examples);
        out
    }
}

/// 一本按名字排的账摊成报告要的 `(名字, 个数)`。名字自己就定了顺序。
fn by_name(counts: &BTreeMap<&str, u64>) -> Vec<(String, u64)> {
    counts
        .iter()
        .map(|(name, count)| ((*name).to_string(), *count))
        .collect()
}

/// 同上，但**按 `order` 给的顺序**摊开，一个都没数到的那些不列。
///
/// 语言与类型有各自固定的排列顺序（`Language::all()` / `TitleKind::all()`），
/// 按名字的字典序排会让「中文、日文、英文」变成「中文、日文、英文」之外的样子——
/// 同一份库出的报告每次长得一样，人才对得着上一次看差异。
fn pick(counts: &BTreeMap<&str, u64>, order: &[&str]) -> Vec<(String, u64)> {
    order
        .iter()
        .filter_map(|name| counts.get(name).map(|count| ((*name).to_string(), *count)))
        .collect()
}

/// 摆一条叫法当例子：`row` 是要摆的那一条，`chosen` 只用来取这个作品的排序标题。
///
/// 依据与「来自哪个变体」都从 `row` 上直接取，**不再拿字面去集合里回找**——
/// 中文那一档的第一名与显示标题分开之后，那条回找会摸到另一条叫法上去。
fn push_example(into: &mut Vec<TitleExample>, set: &TitleSet, chosen: &Chosen, row: &TitleRow) {
    if into.len() >= EXAMPLES {
        return;
    }
    into.push(TitleExample {
        work: set.work.clone(),
        display: row.value.clone(),
        sort: chosen.sort.clone(),
        variant: row.variant_key.clone(),
        evidence: row.evidence.clone(),
    });
}

/// 同一部作品的几个中文叫法里，`best` 是胜出的那一条，其余的列成落选。
fn push_rival(into: &mut Vec<RivalExample>, set: &TitleSet, best: &TitleRow) {
    if into.len() >= EXAMPLES {
        return;
    }
    let mut others: Vec<String> = set
        .entries
        .iter()
        .filter(|entry| entry.language == Language::Chinese && entry.value != best.value)
        .map(|entry| format!("{}（{}）", entry.value, entry.kind.label()))
        .collect();
    others.sort();
    others.dedup();
    into.push(RivalExample {
        work: set.work.clone(),
        chosen: best.value.clone(),
        others,
        evidence: best.evidence.clone(),
    });
}

fn rows(out: &mut String, title: &str, counts: &[(String, u64)]) {
    if counts.is_empty() {
        return;
    }
    let line: Vec<String> = counts
        .iter()
        .map(|(name, count)| format!("{name} {}", thousands(*count)))
        .collect();
    let _ = writeln!(out, "{}{}", pad(title, 16), line.join("、"));
}

fn examples(out: &mut String, title: &str, list: &[TitleExample]) {
    if list.is_empty() {
        return;
    }
    let _ = writeln!(out, "\n{title}：");
    for example in list {
        let _ = writeln!(
            out,
            "  {} → 「{}」，排序标题 {}\n    {}\n    这个名字来自：{}",
            example.work,
            example.display,
            example.sort,
            example.evidence,
            example.variant.as_deref().unwrap_or("那一条发行版本身"),
        );
    }
}
