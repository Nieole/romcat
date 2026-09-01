//! 中立库里的**标题集合**：一个作品的全部叫法，每条带语言、地区、来源与类型。
//!
//! ## 标题为什么是一张表而不是 `work` 上的一列
//!
//! 一个作品在这个库里同时有好几个叫法：No-Intro 的 `Chrono Trigger`、日版条目的
//! `Chrono Trigger`（**同一串字，语言不同**）、官中版的中文译名、汉化组自取的名字。
//! 塞成一个单值字段，第一件事就是要在写的时候挑一个——而挑哪个**取决于导出到哪个前端**
//! （`CONTEXT.md` 的**显示标题**词条说的正是这件事）。挑早了，跨格式转换与按中文搜索
//! 这两件事就都做不了了。
//!
//! 所以这里存的是**全集**，[`crate::title::choose`] 才是挑的那一步，而且它**不落库**。
//!
//! ## 锚点是作品名而不是行号
//!
//! 与 `scrape_value` 同一条理由（见 [`catalog::scrape`](super::scrape) 的模块文档）：
//! 重跑识别会把它自己造的作品与发行版整批删掉再造一遍，新行拿的是新的
//! `INTEGER PRIMARY KEY`。标题集合若挂在行号上，重跑一次识别就全成了孤儿，而外键
//! 是开着的——那还不是「留下孤儿」，是 `DELETE FROM work` 当场报错。
//!
//! 同理，`region` 是**抄下来**的而不是指向发行版的一根指针：显示标题要靠它分辨
//! 「官方英文名」与「日文原名」（No-Intro 的日版条目名是罗马字，字形上与英文名一样），
//! 而那一行发行版随时会被下一趟识别换掉。
//!
//! ## 重折不许冲掉裁决
//!
//! [`Catalog::clear_titles`] 只删 `source <> '裁决'` 的行。人工定下来的叫法是**沉淀**，
//! 与作品、发行版上那一列 `origin` 是同一条纪律；这里把它落在 `source` 上，是因为
//! `裁决` 本来就是优先级表里的一个**源**（`scrape::priority::VERDICT`），
//! 而且它在那张表里已经排在每条链的第一位。

use rusqlite::params;

use super::identify::Confidence;
use super::{Catalog, CatalogError};
use crate::scrape::priority::VERDICT;
use crate::title::{Language, Seam, TitleKind};

/// 标题集合那张表。
pub(super) const TITLE_SCHEMA: &str = "\
-- **标题集合**：一个作品的全部叫法。中立库里标题永远是集合，不是单值（`CONTEXT.md`）。
--
-- 主键就是去重键：**同一个作品、同一种语言、同一个类型、同一个源、同一串字**只留一行，
-- `seen` 记着有几个变体这么叫——真库里同一个中文名会在几十个文件上重复出现，
-- 而「几个人这么叫」正是同一部作品有多个中文译名时的选定依据之一。
CREATE TABLE IF NOT EXISTS title(
    -- 锚点是**作品名**而不是行号：重跑识别会把作品整批换掉（同 scrape_value）。
    work        TEXT    NOT NULL,
    -- 语言码：zh / ja / en / und。**由字形加发行版地区判**，见 `title::language_of`。
    language    TEXT    NOT NULL,
    -- 官方名 / 译名 / 别名 / 汉化组自取的名。
    kind        TEXT    NOT NULL,
    -- 哪个源给的。`裁决` 是人工来源，重折时一行都不碰。
    source      TEXT    NOT NULL,
    value       TEXT    NOT NULL,
    -- 那一条发行版的地区，**抄下来的**。显示标题靠它分辨官方英文名与日文原名。
    region      TEXT,
    -- 变体级的叫法（文件名、汉化组自取的名）来自哪个变体；发行版级的是 NULL。
    variant_key TEXT,
    confidence  TEXT    NOT NULL,
    -- 中文译名走的是 ADR-0019 那道世代裂缝的哪一侧：独立发行版 / 语言属性。
    seam        TEXT,
    -- **依据**：这条叫法是怎么来的。没有依据的结论事后无法复核（ADR-0002）。
    evidence    TEXT    NOT NULL,
    seen        INTEGER NOT NULL,
    PRIMARY KEY (work, language, kind, source, value)
) STRICT;
";

/// 标题集合里的一条**叫法**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleRow {
    /// 哪个作品（锚点是**作品名**）。
    pub work: String,
    /// 这一串字。
    pub value: String,
    /// 语言。
    pub language: Language,
    /// 类型：官方名 / 译名 / 别名 / 汉化组自取的名。
    pub kind: TitleKind,
    /// 哪个源给的。
    pub source: String,
    /// 那条发行版的地区；变体级的叫法上是它所基于的发行版的地区，没有就是 `None`。
    pub region: Option<String>,
    /// 变体级的叫法来自哪个变体。
    pub variant_key: Option<String>,
    /// 置信度。**中文名的结论带置信度**，低的那些进待确认队列（ADR-0002）。
    pub confidence: Confidence,
    /// 中文译名走的是世代裂缝的哪一侧（ADR-0019）。
    pub seam: Option<Seam>,
    /// **依据**。
    pub evidence: String,
    /// 有几个变体这么叫。
    pub seen: u64,
}

impl TitleRow {
    /// 这一条是不是人工**裁决**定下来的。
    #[must_use]
    pub fn is_verdict(&self) -> bool {
        self.source == VERDICT
    }
}

/// 逐条走标题集合时收到的那一条。
pub type TitleVisitor<'a> = dyn FnMut(&TitleRow) + 'a;

impl Catalog {
    /// 把一批叫法写进去。**同一条写两次是同一个结果**。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put_titles(&mut self, rows: &[TitleRow]) -> Result<(), CatalogError> {
        if rows.is_empty() {
            return Ok(());
        }
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        {
            let mut insert = tx
                .prepare(
                    "INSERT INTO title(work, language, kind, source, value, region,
                         variant_key, confidence, seam, evidence, seen)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
                     ON CONFLICT(work, language, kind, source, value) DO UPDATE SET
                        region = excluded.region, variant_key = excluded.variant_key,
                        confidence = excluded.confidence, seam = excluded.seam,
                        evidence = excluded.evidence, seen = excluded.seen",
                )
                .map_err(to_err)?;
            for row in rows {
                insert
                    .execute(params![
                        row.work,
                        row.language.code(),
                        row.kind.label(),
                        row.source,
                        row.value,
                        row.region,
                        row.variant_key,
                        row.confidence.label(),
                        row.seam.map(Seam::label),
                        row.evidence,
                        i64::try_from(row.seen).unwrap_or(i64::MAX),
                    ])
                    .map_err(to_err)?;
            }
        }
        tx.commit().map_err(to_err)
    }

    /// 把折出来的叫法整批清掉，**裁决定下来的一行都不碰**。
    ///
    /// 返回清掉了几条。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn clear_titles(&mut self) -> Result<u64, CatalogError> {
        self.conn
            .execute("DELETE FROM title WHERE source <> ?1", params![VERDICT])
            .map(|removed| removed as u64)
            .map_err(|source| self.err(source))
    }

    /// 一个作品的全部叫法。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn titles_of(&self, work: &str) -> Result<Vec<TitleRow>, CatalogError> {
        let mut statement = self
            .conn
            .prepare_cached(&format!("{SELECT_TITLE} WHERE work = ?1 {ORDER_TITLE}"))
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![work], read_title)
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 一条条走过全部叫法，**按作品聚在一起**。
    ///
    /// 走回调而不是返回一整份 `Vec`：真库里作品有 9,226 个，而选**显示标题**是逐个作品
    /// 做的——攒齐一个作品的叫法就够挑一次，不必把全库的标题一起端进内存。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn for_each_title(&self, each: &mut TitleVisitor) -> Result<(), CatalogError> {
        let mut statement = self
            .conn
            .prepare(&format!("{SELECT_TITLE} {ORDER_TITLE}"))
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let row = read_title(row).map_err(|source| self.err(source))?;
            each(&row);
        }
        Ok(())
    }

    /// 标题集合里一共几条。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn title_count(&self) -> Result<u64, CatalogError> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM title", [], |row| row.get(0))
            .map_err(|source| self.err(source))?;
        Ok(u64::try_from(count).unwrap_or(0))
    }
}

const SELECT_TITLE: &str = "SELECT work, language, kind, source, value, region, variant_key,
     confidence, seam, evidence, seen FROM title";

/// 按作品聚在一起，作品内部再定死顺序——同一份中立库跑两次，挑出来的必须是同一条。
const ORDER_TITLE: &str = "ORDER BY work, language, kind, source, value";

fn read_title(row: &rusqlite::Row<'_>) -> rusqlite::Result<TitleRow> {
    let language: String = row.get(1)?;
    let kind: String = row.get(2)?;
    let confidence: String = row.get(7)?;
    let seam: Option<String> = row.get(8)?;
    Ok(TitleRow {
        work: row.get(0)?,
        language: Language::from_code(&language).unwrap_or(Language::Unknown),
        kind: TitleKind::from_label(&kind).unwrap_or(TitleKind::Alias),
        source: row.get(3)?,
        value: row.get(4)?,
        region: row.get(5)?,
        variant_key: row.get(6)?,
        confidence: Confidence::from_label(&confidence).unwrap_or(Confidence::Low),
        seam: seam.as_deref().and_then(Seam::from_label),
        evidence: row.get(9)?,
        seen: u64::try_from(row.get::<_, i64>(10)?).unwrap_or(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 叫法(work: &str, value: &str, source: &str) -> TitleRow {
        TitleRow {
            work: work.to_string(),
            value: value.to_string(),
            language: Language::Chinese,
            kind: TitleKind::Alias,
            source: source.to_string(),
            region: None,
            variant_key: None,
            confidence: Confidence::Low,
            seam: None,
            evidence: "编的".to_string(),
            seen: 1,
        }
    }

    #[test]
    fn 一个作品的多个叫法并存而不是互相覆盖() {
        let mut catalog = Catalog::open_in_memory().expect("能开中立库");
        let mut 官方 = 叫法("Chrono Trigger", "Chrono Trigger", "No-Intro");
        官方.language = Language::English;
        官方.kind = TitleKind::Official;
        catalog
            .put_titles(&[
                官方,
                叫法("Chrono Trigger", "时空之轮", "文件名"),
                叫法("Chrono Trigger", "超时空之轮", "文件名"),
            ])
            .expect("写得进");
        let 全部 = catalog.titles_of("Chrono Trigger").expect("读得出");
        assert_eq!(全部.len(), 3, "标题是集合，不是一个单值字段");
    }

    #[test]
    fn 重折不冲掉裁决() {
        let mut catalog = Catalog::open_in_memory().expect("能开中立库");
        let mut 人说的 = 叫法("Chrono Trigger", "超时空之轮", VERDICT);
        人说的.kind = TitleKind::Translated;
        人说的.confidence = Confidence::High;
        catalog
            .put_titles(&[人说的, 叫法("Chrono Trigger", "时空之轮", "文件名")])
            .expect("写得进");
        assert_eq!(catalog.clear_titles().expect("清得掉"), 1);
        let 剩下 = catalog.titles_of("Chrono Trigger").expect("读得出");
        assert_eq!(剩下.len(), 1);
        assert!(剩下[0].is_verdict(), "人工定下来的叫法是沉淀，重折不许冲掉");
        assert_eq!(剩下[0].value, "超时空之轮");
    }

    #[test]
    fn 一条叫法的四个维度都存得住也读得回来() {
        let mut catalog = Catalog::open_in_memory().expect("能开中立库");
        let row = TitleRow {
            work: "Pokemon 4-in-1".to_string(),
            value: "口袋妖怪四合一".to_string(),
            language: Language::Chinese,
            kind: TitleKind::Translated,
            source: "文件名".to_string(),
            region: Some("China".to_string()),
            variant_key: Some("FC/口袋妖怪四合一.zip".to_string()),
            confidence: Confidence::High,
            seam: Some(Seam::OwnRelease),
            evidence: "官中那一条发行版".to_string(),
            seen: 3,
        };
        catalog
            .put_titles(std::slice::from_ref(&row))
            .expect("写得进");
        assert_eq!(
            catalog.titles_of("Pokemon 4-in-1").expect("读得出"),
            vec![row]
        );
    }
}
