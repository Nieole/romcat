//! **DAT 源**：从识别撞出来的条目名里读元数据。
//!
//! DAT 不给结构化字段，语义**全编码在条目名里**。识别那一层已经保守地读过一遍
//! （[`identify::naming`](crate::identify::naming) 只取作品名、地区与语言，认不出就留空），
//! 这里接着往下读——**刮削才是补齐元数据的地方**，那份文档自己就是这么说的。
//!
//! ## 一个源一个实例，不是一个「DAT 源」通吃
//!
//! `No-Intro` 与 `TOSEC` 的命名规范**完全不同**：
//!
//! | | No-Intro / Redump | TOSEC |
//! |---|---|---|
//! | 样子 | `1942 (Japan, USA) (En)` | `1942 (1985-12-11)(Capcom)(JP-US)` |
//! | 第一个 `(…)` | 地区 | **发行日期** |
//! | 第二个 `(…)` | 语言 | **发行商** |
//! | 年份 | 没有 | 有 |
//! | 发行商 | 没有 | 有 |
//!
//! 把两家塞进同一个解析器，第一件事就是把 TOSEC 的 `1985-12-11` 当成地区
//! （`identify::naming` 正是靠「不在已知地区表里就留空」躲开这一刀的）。所以这里
//! **一个数据源一个实例**：每个实例只看自己那个源的条目，按自己那套规范读。
//!
//! 这也让**字段级多源优先级**有了真东西可排：标题 No-Intro 的最整齐，而**年份与发行商
//! 只有 TOSEC 给得出**——「用 A 源的标题配 B 源的年份」不是假设，是这个库上的事实。
//!
//! ## 认不出就留空
//!
//! `(199x)` 与 `(198x)` 是 TOSEC 表示「年份不确定」的写法，`(-)` 是「发行商不详」。
//! 两者都**不产出值**：错的元数据比缺的元数据难查得多。

use crate::identify::naming;

use super::{Failure, Field, Harvest, Locality, Source, Subject};

/// 一个 DAT 数据源。
#[derive(Debug, Clone)]
pub struct DatSource {
    name: &'static str,
}

impl DatSource {
    /// 盯着某一个数据源（`No-Intro` / `Redump` / `TOSEC` / `MAME` / `GoodNES`）。
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self { name }
    }

    /// 这个锚点上属于本源的条目名，去重后按字典序。
    fn names<'a>(&self, subject: &Subject<'a>) -> Vec<&'a str> {
        let mut names: Vec<&str> = subject
            .entries
            .iter()
            .filter(|entry| entry.source == self.name)
            .map(|entry| entry.game.as_str())
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }
}

impl Source for DatSource {
    fn name(&self) -> &str {
        self.name
    }

    fn locality(&self) -> Locality {
        // DAT 早在票 06 就镜像到本地了。刮削这一趟连 DAT 库都不打开——要的东西
        // 识别已经抄进中立库的候选里了。
        Locality::Local
    }

    fn probe(&self, subject: &Subject<'_>) -> Option<String> {
        let names = self.names(subject);
        if names.is_empty() {
            return None;
        }
        Some(super::fingerprint(&names))
    }

    fn collect(&self, subject: &Subject<'_>, out: &mut Harvest) -> Result<(), Failure> {
        let tosec = self.name == "TOSEC";
        for name in self.names(subject) {
            let why = format!("{} 的条目名「{name}」", self.name);
            match subject.kind {
                super::AnchorKind::Work => {
                    out.value(Field::Title, naming::work_title(name), &why);
                    if tosec {
                        if let Some(year) = tosec_year(name) {
                            out.value(Field::Year, year, format!("{why}里第一个括号是发行日期"));
                        }
                        if let Some(publisher) = tosec_publisher(name) {
                            out.value(
                                Field::Publisher,
                                publisher,
                                format!("{why}里第二个括号是发行商"),
                            );
                        }
                    }
                }
                super::AnchorKind::Variant => {
                    // **汉化组挂在变体上**：汉化版是变体不是发行版（ADR-0012），
                    // 而「谁做的汉化」正是这个库里官方数据库补不上的那一块。
                    if let Some(group) = translation_group(name) {
                        out.value(
                            Field::TranslationGroup,
                            group,
                            format!("{why}里的 `[tr zh …]` 标记"),
                        );
                    }
                }
            }
        }
        // 本地源没有会失败的动作：条目名已经在中立库里躺着了。
        Ok(())
    }
}

/// 名字里由 `open` / `close` 括起来的每一组（不含括号本身），按出现顺序。
///
/// 不处理嵌套：DAT 的命名规范里这些标记本来就是平铺的，而按最近的 `close` 收口
/// 在畸形名字上也不会走飞（同 `dat::chinese::groups`）。
fn groups(name: &str, open: char, close: char) -> impl Iterator<Item = &str> {
    let mut rest = name;
    std::iter::from_fn(move || {
        let start = rest.find(open)? + open.len_utf8();
        let body = &rest[start..];
        let end = body.find(close)?;
        rest = &body[end + close.len_utf8()..];
        Some(&body[..end])
    })
}

/// TOSEC 名字里的发行年份。
///
/// 第一个 `(…)` 是发行日期，形如 `1985`、`1985-12-11`、`199x`、`19xx`。
/// **只认得出四位数字才产出**——`199x` 说的正是「不知道是哪一年」，把它当年份写进去
/// 等于把「不知道」伪装成「知道」。
#[must_use]
pub fn tosec_year(name: &str) -> Option<String> {
    let first = groups(name, '(', ')').next()?;
    let head = first.get(..4)?;
    if head.len() == 4 && head.chars().all(|c| c.is_ascii_digit()) {
        Some(head.to_string())
    } else {
        None
    }
}

/// TOSEC 名字里的发行商。第二个 `(…)`；`-` 是「不详」，不产出。
#[must_use]
pub fn tosec_publisher(name: &str) -> Option<String> {
    let second = groups(name, '(', ')').nth(1)?.trim();
    if second.is_empty() || second == "-" {
        return None;
    }
    Some(second.to_string())
}

/// TOSEC 名字里 `[tr zh …]` 后面那个**汉化组**。
///
/// 实测的几种样子：`[tr zh]`（没署名）、`[tr zh dwt_so]`、`[tr zh MS emumax]`。
/// 没署名就返回 `None`——空着比编一个名字强。
#[must_use]
pub fn translation_group(name: &str) -> Option<String> {
    for group in groups(name, '[', ']') {
        let mut parts = group.split_whitespace();
        if parts.next() != Some("tr") {
            continue;
        }
        // `tr` 后面第一段是语言码。这里只认中文——别的语言的汉化组不是这个库要的东西。
        let language = parts.next()?;
        if !language.starts_with("zh") && language != "chi" {
            continue;
        }
        let rest: Vec<&str> = parts.collect();
        if rest.is_empty() {
            return None;
        }
        return Some(rest.join(" "));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scrape::{AnchorKind, DatEntry};

    fn 条目(source: &str, game: &str) -> DatEntry {
        DatEntry {
            source: source.to_string(),
            game: game.to_string(),
        }
    }

    fn 作品<'a>(entries: &'a [DatEntry]) -> Subject<'a> {
        Subject {
            kind: AnchorKind::Work,
            id: "1942",
            platform: Some("FC"),
            entries,
            main_key: None,
            media: &[],
            media_limit: None,
            confirmed: true,
            basis: None,
        }
    }

    #[test]
    fn tosec的名字读得出年份与发行商() {
        assert_eq!(
            tosec_year("1942 (1985-12-11)(Capcom)(JP-US)").as_deref(),
            Some("1985")
        );
        assert_eq!(
            tosec_publisher("1942 (1985-12-11)(Capcom)(JP-US)").as_deref(),
            Some("Capcom")
        );
    }

    #[test]
    fn 年份不确定就不产出() {
        assert_eq!(tosec_year("1944 (199x)(-)(AS)[p]"), None);
        assert_eq!(tosec_year("20 in 1 (19xx)(-)(AS)[p]"), None);
        assert_eq!(tosec_publisher("1944 (199x)(-)(AS)[p]"), None);
    }

    #[test]
    fn 汉化组读得出也读得出没署名() {
        assert_eq!(
            translation_group("Saint Seiya (1988-05-31)(Bandai)(JP)[tr zh dwt_so][v0.01]")
                .as_deref(),
            Some("dwt_so")
        );
        assert_eq!(
            translation_group("Macross (1985-12-10)(Bandai)(JP)[tr zh MS emumax][v.20060513]")
                .as_deref(),
            Some("MS emumax")
        );
        assert_eq!(
            translation_group("Contra (1988-02-09)(Konami)(JP)[tr zh][v.20030208]"),
            None
        );
        assert_eq!(
            translation_group("Something (1990)(X)(JP)[tr de someone]"),
            None
        );
    }

    #[test]
    fn 一个源只看自己的条目() {
        let entries = vec![
            条目("No-Intro", "1942 (Japan, USA) (En)"),
            条目("TOSEC", "1942 (1985-12-11)(Capcom)(JP-US)"),
        ];
        let subject = 作品(&entries);

        let mut nointro = Harvest::default();
        DatSource::new("No-Intro")
            .collect(&subject, &mut nointro)
            .expect("本地源不会失败");
        assert_eq!(nointro.values.len(), 1, "No-Intro 只给得出标题");
        assert_eq!(nointro.values[0].value, "1942");

        let mut tosec = Harvest::default();
        DatSource::new("TOSEC")
            .collect(&subject, &mut tosec)
            .expect("本地源不会失败");
        let fields: Vec<Field> = tosec.values.iter().map(|found| found.field).collect();
        assert_eq!(fields, vec![Field::Title, Field::Year, Field::Publisher]);
    }

    #[test]
    fn 没有本源条目时整条无话可说() {
        let entries = vec![条目("TOSEC", "1942 (1985-12-11)(Capcom)(JP-US)")];
        assert!(DatSource::new("Redump").probe(&作品(&entries)).is_none());
    }

    #[test]
    fn 输入指纹只跟本源的条目名走() {
        let a = vec![
            条目("No-Intro", "1942 (Japan, USA) (En)"),
            条目("TOSEC", "1942 (1985-12-11)(Capcom)(JP-US)"),
        ];
        let b = vec![
            条目("No-Intro", "1942 (Japan, USA) (En)"),
            条目("TOSEC", "1942 (1985-12-11)(Capcom)(JP-US)"),
            条目("MAME", "1942"),
        ];
        let source = DatSource::new("No-Intro");
        assert_eq!(source.probe(&作品(&a)), source.probe(&作品(&b)));
    }
}
