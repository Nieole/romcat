//! 中立库里的**刮削**结论：字段值、**媒体池**的映射，以及采集缓存。
//!
//! ## 五张表各自回答一个问题
//!
//! - `scrape_value`：**一个锚点、一个字段、一个源给出的值**。**并存而非覆盖**
//!   （调研 13.3(4)）：后写的源盖掉先写的，就永远做不了字段级 fallback，也无法在不重新
//!   采集的前提下调一次优先级——而调优先级正是这套机制存在的理由。
//!
//!   去重键里**带上值本身**，于是一个源在一个字段上说得出好几句话。**这不是给「一个源
//!   两个答案」开口子**：单值字段仍旧第一个胜出，那道闸在 `Harvest::value` 上。它是给
//!   **标题集合**留的位置——中立库里标题永远是集合不是单值（`CONTEXT.md`），而中文离线源
//!   撞上一条条目之后，那条条目的中文名与**别名**本来就是同一部作品的好几个叫法
//!   （`Harvest::each`）。
//! - `media`：**媒体池**里的一份媒体，主键是**内容哈希**。文件名不是媒体的主键
//!   （ADR-0009）：各前端的媒体匹配键在文件名、标题、label 之间横跳，净化函数多对一
//!   不可逆，只有内容哈希在所有格式之间都说得通。
//! - `media_ref`：谁引用了那份媒体。**同一份媒体被多个锚点引用时，`media` 里仍然只有
//!   一行、池里仍然只有一个文件**——「只存一份」是内容寻址天然给的，不是另加的去重步骤。
//! - `media_blob`：主库里那份媒体文件算过的内容哈希。**读过的盘不白读**：由扫描按文件的
//!   三元组作废，与 `content_hash` 是同一条路（挂账 D14）。
//! - `media_remote`：**在线源下过的那个 URL 算出来是哪一份内容**。它与 `media_blob`
//!   刻意分成两张表，理由是**作废的方式完全相反**：`media_blob` 的键是主库里的文件，
//!   扫描一发现文件没了就整行扫掉（`Catalog::sweep` 的 `key NOT IN entry`）；而一个
//!   在线 URL 在主库里永远不存在，混进同一张表会被那句 SQL 每次扫描都清一遍，
//!   于是**下过的图每次都要重下**——花的是配额与带宽。
//! - `scrape_probe`：一个源对一个锚点采集过了没有，以及当时的**输入指纹**。指纹一样就
//!   整条跳过，连「查过、没有」也记着——离线档省的是算力，在线档（票 14）省的是配额，
//!   而重复打一次空查询在 ScreenScraper 那边要额外扣一份「未识别 ROM」配额（ADR-0007）。
//!
//! ## 锚点为什么是字符串而不是行号
//!
//! **识别与刮削是两个可分别重跑的阶段。** 重跑识别会把它自己造的作品与发行版整批删掉
//! 再造一遍（[`Catalog::clear_identifications`](super::Catalog::clear_identifications)），
//! 新造出来的行拿的是新的 `INTEGER PRIMARY KEY`。刮削结论若挂在行号上，重跑一次识别
//! 就全成了孤儿——在线档那边等于把配额烧掉重来。
//!
//! 所以锚点是**自然键**：作品锚点是**作品名**（那正是识别给作品去重用的键），变体锚点是
//! **变体的键**（相对主库根的路径，ADR-0020）。两者都不随识别重跑而变。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{OptionalExtension, params};

use super::{Catalog, CatalogError};
use crate::scrape::priority::VERDICT;
use crate::scrape::{AnchorKind, Field};

/// 刮削相关的表。
pub(super) const SCRAPE_SCHEMA: &str = "\
-- 一个锚点、一个字段、一个源给出的值。**不同的源并存，不互相覆盖。**
CREATE TABLE IF NOT EXISTS scrape_value(
    anchor   TEXT NOT NULL,
    subject  TEXT NOT NULL,
    field    TEXT NOT NULL,
    source   TEXT NOT NULL,
    value    TEXT NOT NULL,
    -- **依据**：这个值是怎么来的。没有依据的值事后无法复核（ADR-0002 的道理，
    -- 换到刮削这一侧同样成立）。
    evidence TEXT NOT NULL,
    -- 采集时刻。**优先级表里没列到的源按它兜底**（新的优先），于是加一个源不必改配置。
    at       INTEGER NOT NULL,
    -- **值也在键里**：一个源在一个字段上说得出好几句话（标题集合那一种形状）。
    -- 少了它，中文离线源撞上一条条目之后那几个别名只有一个落得了库，其余的连同它们的
    -- **依据**一起蒸发，而采集记录还记着「采到五条」——数对不上，还查不出为什么。
    PRIMARY KEY (anchor, subject, field, source, value)
) STRICT;

CREATE INDEX IF NOT EXISTS scrape_value_subject ON scrape_value(anchor, subject);

-- **媒体池**里的一份媒体。主键是内容哈希——文件名不是媒体的主键（ADR-0009）。
CREATE TABLE IF NOT EXISTS media(
    hash  TEXT PRIMARY KEY,
    ext   TEXT    NOT NULL,
    bytes INTEGER NOT NULL,
    at    INTEGER NOT NULL
) STRICT;

-- 谁引用了那份媒体。一份媒体被多个锚点引用，这里多几行，池里仍然只有一个文件。
CREATE TABLE IF NOT EXISTS media_ref(
    anchor   TEXT NOT NULL,
    subject  TEXT NOT NULL,
    kind     TEXT NOT NULL,
    source   TEXT NOT NULL,
    hash     TEXT NOT NULL REFERENCES media(hash),
    evidence TEXT NOT NULL,
    at       INTEGER NOT NULL,
    PRIMARY KEY (anchor, subject, kind, source, hash)
) STRICT;

CREATE INDEX IF NOT EXISTS media_ref_hash ON media_ref(hash);

-- 主库里那份媒体文件算过的内容哈希。扫描按文件的三元组作废它（同 content_hash）。
CREATE TABLE IF NOT EXISTS media_blob(
    key   TEXT PRIMARY KEY,
    bytes INTEGER NOT NULL,
    hash  TEXT    NOT NULL
) STRICT;

-- 在线源下过的那个 URL 算出来是哪一份内容。**与 media_blob 分开**：扫描按
-- 「主库里还有没有这个键」清 media_blob，而在线 URL 在主库里永远不存在，
-- 混在一起等于每次扫描都把下过的图作废一遍——那要重花配额与带宽。
CREATE TABLE IF NOT EXISTS media_remote(
    url   TEXT PRIMARY KEY,
    bytes INTEGER NOT NULL,
    hash  TEXT    NOT NULL
) STRICT;

-- 一份**视频**抽出来的首帧是池里的哪一份内容（票 `gui-redesign/07`）。
-- **抽帧要外部 ffmpeg，一份视频抽一次就够了**：真库的媒体池里有 178 个 mp4，
-- 每开一次详情面板重抽一遍，等于每次翻库都拉起一百多个进程。
--
-- 键是**视频自己的内容哈希**而不是它的键：媒体池按内容哈希存（ADR-0009），
-- 同一段视频被几个锚点引用时池里只有一份，它的首帧当然也只该抽一次。
--
-- **与 media_blob、media_remote 各分一张表**，理由同它们那两条——作废的方式不一样：
-- 这一行只在池里那份视频没了才失效，而扫描扫的是主库、动不着它。
CREATE TABLE IF NOT EXISTS media_frame(
    video TEXT PRIMARY KEY REFERENCES media(hash),
    frame TEXT NOT NULL    REFERENCES media(hash),
    at    INTEGER NOT NULL
) STRICT;

-- 一个源对一个锚点采集过了没有。`input` 是当时的**输入指纹**，一样就整条跳过。
-- `vals` / `pics` 记的是那次采到几条，**零也照记**——「查过、没有」是一条结论，
-- 不是「还没查」。
CREATE TABLE IF NOT EXISTS scrape_probe(
    anchor  TEXT NOT NULL,
    subject TEXT NOT NULL,
    source  TEXT NOT NULL,
    input   TEXT NOT NULL,
    vals    INTEGER NOT NULL,
    pics    INTEGER NOT NULL,
    at      INTEGER NOT NULL,
    PRIMARY KEY (anchor, subject, source)
) STRICT;
";

/// 一条要写进去的字段值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarvestedValue {
    /// 字段。
    pub field: String,
    /// 值。
    pub value: String,
    /// **依据**。
    pub evidence: String,
}

/// 一条要写进去的媒体引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarvestedMedia {
    /// 媒体类型。
    pub kind: String,
    /// 内容哈希，也就是它在**媒体池**里的主键。
    pub hash: String,
    /// **依据**。
    pub evidence: String,
}

/// 一个源在一个锚点上采到的东西，写库前的样子。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Harvested {
    /// 锚点种类：`作品` 或 `变体`。
    pub anchor: String,
    /// 锚点：作品名，或变体的键。
    pub subject: String,
    /// 哪个源。
    pub source: String,
    /// 这一轮的**输入指纹**。下次一样就整条跳过。
    pub input: String,
    /// 字段值。
    pub values: Vec<HarvestedValue>,
    /// 媒体引用。
    pub media: Vec<HarvestedMedia>,
}

/// 一条读回来的字段值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrapedValue {
    /// 字段。
    pub field: String,
    /// 哪个源给的。
    pub source: String,
    /// 值。
    pub value: String,
    /// **依据**。
    pub evidence: String,
    /// 采集时刻（Unix 秒）。优先级表没列到的源按它兜底。
    pub at: i64,
}

/// 一条读回来的媒体引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrapedMedia {
    /// 媒体类型：封面 / 截图 / 视频 / 其他。
    pub kind: String,
    /// 哪个源给的。
    pub source: String,
    /// 内容哈希，也就是它在**媒体池**里的主键。
    pub hash: String,
    /// **依据**。
    pub evidence: String,
    /// 采集时刻（Unix 秒）。
    pub at: i64,
}

/// 媒体池的家底：几份、多少字节、被引用几次。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PoolCounts {
    /// 池里有几份媒体（**按内容哈希数**，不是按引用数）。
    pub blobs: u64,
    /// 池里一共多少字节。
    pub bytes: u64,
    /// 一共有几条引用。它减去 `blobs` 就是「只存一份」省下来的份数。
    pub refs: u64,
    /// 被不止一个锚点引用的媒体有几份。
    pub shared: u64,
}

/// 逐条走文件条目时收到的那三样：键、字节数、修改时间。
pub type FileVisitor<'a> = dyn FnMut(&str, Option<u64>, Option<i64>) + 'a;

/// 逐条走刮削结论时收到的那三样：锚点种类、锚点、那条值。
///
/// **`evidence` 在这条路上是空的**：报告只用得着值与源，而真库里那是 78,902 条依据、
/// 每条几十个字——为一次计数把它们全搬进内存不划算。要看依据走
/// [`scraped_values`](Catalog::scraped_values)。
pub type ScrapedVisitor<'a> = dyn FnMut(&str, &str, ScrapedValue) + 'a;

/// 一个源在一个平台上的**覆盖**：那个平台有几个变体、其中几个被它撞上了。
///
/// 报告要答得出「N64、DC 这些老平台为什么补不上」——**那是数据源本身浅，不是匹配
/// 算法的锅**（票 06）。不按平台报的话，用户只看得见一个全库的百分比，怪错的地方。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceCoverage {
    /// 平台。平台认不出来的那些归在调用方给的那个词底下。
    pub platform: String,
    /// 这个平台一共几个变体。**分母是库里的变体数**，不是这一趟采过的个数。
    pub variants: u64,
    /// 其中几个变体身上有这个源给的值。
    pub matched: u64,
}

/// 刮削结论按「字段 × 源」的条数。报告要答得出「某个源值不值得继续用」。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldCount {
    /// 字段。
    pub field: String,
    /// 哪个源。
    pub source: String,
    /// 这个源在这个字段上给了几个值。
    pub values: u64,
    /// 涉及几个锚点。
    pub subjects: u64,
}

/// 一句 `IN (…)` 里最多塞多少个键。
///
/// SQLite 的绑定变量有上限：新版 32,766，老版 999。往小里设，因为**超上限的后果是
/// 一句读不懂的错**（「too many SQL variables」），而分批的代价只是多几趟查询。
const KEYS_PER_QUERY: usize = 900;

impl Catalog {
    /// 一个锚点上、各个源上次采集的**输入指纹**：源 → 指纹。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn scrape_inputs(
        &self,
        anchor: &str,
        subject: &str,
    ) -> Result<BTreeMap<String, String>, CatalogError> {
        let mut statement = self
            .conn
            .prepare_cached(
                "SELECT source, input FROM scrape_probe WHERE anchor = ?1 AND subject = ?2",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![anchor, subject], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|source| self.err(source))?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (source, input) = row.map_err(|source| self.err(source))?;
            out.insert(source, input);
        }
        Ok(out)
    }

    /// 把一批采集结果写进去。
    ///
    /// **一个源在一个锚点上写两次是同一个结果**：先删掉它上一轮在这个锚点上留下的值与
    /// 媒体引用，再插新的。删的只是**这个源**的行——别的源的值一条都不碰，那正是
    /// 「并存而非覆盖」的落点。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put_scraped(&mut self, batch: &[Harvested]) -> Result<(), CatalogError> {
        let all: BTreeSet<Field> = Field::all().into_iter().collect();
        self.put_scraped_within(batch, &all, true)
    }

    /// 把一批采集结果写进去，**只在这几个字段之内替换**。
    ///
    /// 与 [`put_scraped`](Self::put_scraped) 的差别只有一条：那一条是「这个源在这个锚点上
    /// 说的全部话」，这一条是「这个源在这个锚点上、**这几个字段上**说的话」。
    ///
    /// 差别要命在哪：界面上那个「字段」旋钮（票 `gui-redesign/10`）能只勾简介跑一趟，
    /// 而删得太宽的话，这一趟就会把上一趟采到的类型、开发商、发行商一起抹掉——
    /// 那正是「三元组并存、没有覆盖」要防的事。**不收媒体时同理**：媒体引用一条都不删，
    /// 否则「这趟不收媒体」会把收过的媒体扔了。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put_scraped_within(
        &mut self,
        batch: &[Harvested],
        fields: &BTreeSet<Field>,
        media: bool,
    ) -> Result<(), CatalogError> {
        if batch.is_empty() {
            return Ok(());
        }
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let at = super::now_secs();
        let tx = self.conn.transaction().map_err(to_err)?;
        {
            // **删得多窄由这一趟要什么说了算。** 名单是全的就一句删完（那是命令行走的
            // 那条，也是既有行为）；名单收窄了就一个字段一句——多出来的那几句只在
            // 界面按窄名单跑的时候执行，而它换来的是「不在名单里的字段一个字都不动」。
            let mut drop_values = tx
                .prepare(
                    "DELETE FROM scrape_value WHERE anchor = ?1 AND subject = ?2 AND source = ?3",
                )
                .map_err(to_err)?;
            let mut drop_field = tx
                .prepare(
                    "DELETE FROM scrape_value
                     WHERE anchor = ?1 AND subject = ?2 AND source = ?3 AND field = ?4",
                )
                .map_err(to_err)?;
            let everything = Field::all().iter().all(|field| fields.contains(field));
            let mut drop_media = tx
                .prepare("DELETE FROM media_ref WHERE anchor = ?1 AND subject = ?2 AND source = ?3")
                .map_err(to_err)?;
            let mut insert_value = tx
                .prepare(
                    "INSERT INTO scrape_value(anchor, subject, field, source, value, evidence, at)
                     VALUES(?1,?2,?3,?4,?5,?6,?7)
                     ON CONFLICT(anchor, subject, field, source, value) DO UPDATE SET
                        evidence = excluded.evidence, at = excluded.at",
                )
                .map_err(to_err)?;
            let mut insert_media = tx
                .prepare(
                    "INSERT INTO media_ref(anchor, subject, kind, source, hash, evidence, at)
                     VALUES(?1,?2,?3,?4,?5,?6,?7)
                     ON CONFLICT(anchor, subject, kind, source, hash) DO UPDATE SET
                        evidence = excluded.evidence, at = excluded.at",
                )
                .map_err(to_err)?;
            let mut insert_probe = tx
                .prepare(
                    "INSERT INTO scrape_probe(anchor, subject, source, input, vals, pics, at)
                     VALUES(?1,?2,?3,?4,?5,?6,?7)
                     ON CONFLICT(anchor, subject, source) DO UPDATE SET
                        input = excluded.input, vals = excluded.vals,
                        pics = excluded.pics, at = excluded.at",
                )
                .map_err(to_err)?;
            for got in batch {
                if everything {
                    drop_values
                        .execute(params![got.anchor, got.subject, got.source])
                        .map_err(to_err)?;
                } else {
                    for field in fields {
                        drop_field
                            .execute(params![got.anchor, got.subject, got.source, field.label()])
                            .map_err(to_err)?;
                    }
                }
                if media {
                    drop_media
                        .execute(params![got.anchor, got.subject, got.source])
                        .map_err(to_err)?;
                }
                for found in &got.values {
                    insert_value
                        .execute(params![
                            got.anchor,
                            got.subject,
                            found.field,
                            got.source,
                            found.value,
                            found.evidence,
                            at
                        ])
                        .map_err(to_err)?;
                }
                for picture in &got.media {
                    insert_media
                        .execute(params![
                            got.anchor,
                            got.subject,
                            picture.kind,
                            got.source,
                            picture.hash,
                            picture.evidence,
                            at
                        ])
                        .map_err(to_err)?;
                }
                insert_probe
                    .execute(params![
                        got.anchor,
                        got.subject,
                        got.source,
                        got.input,
                        i64::try_from(got.values.len()).unwrap_or(i64::MAX),
                        i64::try_from(got.media.len()).unwrap_or(i64::MAX),
                        at
                    ])
                    .map_err(to_err)?;
            }
        }
        tx.commit().map_err(to_err)
    }

    /// 一个锚点上的全部字段值，按字段、源排序。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn scraped_values(
        &self,
        anchor: &str,
        subject: &str,
    ) -> Result<Vec<ScrapedValue>, CatalogError> {
        let mut statement = self
            .conn
            .prepare_cached(
                "SELECT field, source, value, evidence, at FROM scrape_value
                 WHERE anchor = ?1 AND subject = ?2 ORDER BY field, source",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![anchor, subject], |row| {
                Ok(ScrapedValue {
                    field: row.get(0)?,
                    source: row.get(1)?,
                    value: row.get(2)?,
                    evidence: row.get(3)?,
                    at: row.get(4)?,
                })
            })
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 一个锚点上的全部媒体引用。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn scraped_media(
        &self,
        anchor: &str,
        subject: &str,
    ) -> Result<Vec<ScrapedMedia>, CatalogError> {
        let mut statement = self
            .conn
            .prepare_cached(
                "SELECT kind, source, hash, evidence, at FROM media_ref
                 WHERE anchor = ?1 AND subject = ?2 ORDER BY kind, source, hash",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![anchor, subject], |row| {
                Ok(ScrapedMedia {
                    kind: row.get(0)?,
                    source: row.get(1)?,
                    hash: row.get(2)?,
                    evidence: row.get(3)?,
                    at: row.get(4)?,
                })
            })
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 主库里那份媒体文件算过的内容哈希：`(字节数, 哈希)`。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn media_blob(&self, key: &str) -> Result<Option<(u64, String)>, CatalogError> {
        self.conn
            .prepare_cached("SELECT bytes, hash FROM media_blob WHERE key = ?1")
            .and_then(|mut statement| {
                statement
                    .query_row(params![key], |row| {
                        Ok((
                            u64::try_from(row.get::<_, i64>(0)?).unwrap_or(0),
                            row.get::<_, String>(1)?,
                        ))
                    })
                    .optional()
            })
            .map_err(|source| self.err(source))
    }

    /// 池里这份媒体的扩展名；池里没有就是 `None`。
    ///
    /// **扩展名以库里记的那一个为准。** 同一串字节以 `.jpg` 与 `.jpeg` 两个名字出现时，
    /// 内容哈希是同一个——若各按各的扩展名落盘，池里就成了两个文件，「只存一份」当场
    /// 失效。先问库、再落盘，第一次记下的那个扩展名一直用下去。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn media_ext(&self, hash: &str) -> Result<Option<String>, CatalogError> {
        self.conn
            .prepare_cached("SELECT ext FROM media WHERE hash = ?1")
            .and_then(|mut statement| {
                statement
                    .query_row(params![hash], |row| row.get(0))
                    .optional()
            })
            .map_err(|source| self.err(source))
    }

    /// 这份**视频**抽过的首帧是池里的哪一份；没抽过就是 `None`。
    ///
    /// **第二次打开不重抽**靠的就是它（票 `gui-redesign/07` 的第四条验收）。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn media_frame(&self, video: &str) -> Result<Option<String>, CatalogError> {
        self.conn
            .prepare_cached("SELECT frame FROM media_frame WHERE video = ?1")
            .and_then(|mut statement| {
                statement
                    .query_row(params![video], |row| row.get(0))
                    .optional()
            })
            .map_err(|source| self.err(source))
    }

    /// 记下「这份视频的首帧抽出来是那一份内容」。
    ///
    /// **后写的盖掉先写的**：换了一版 ffmpeg 重抽出来的那一帧才是眼下池里那一份，
    /// 留着旧的等于指着一个可能已经不在的文件。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put_media_frame(&mut self, video: &str, frame: &str) -> Result<(), CatalogError> {
        self.conn
            .execute(
                "INSERT INTO media_frame(video, frame, at) VALUES(?1,?2,?3)
                 ON CONFLICT(video) DO UPDATE SET frame = excluded.frame, at = excluded.at",
                params![video, frame, super::now_secs()],
            )
            .map(|_| ())
            .map_err(|source| self.err(source))
    }

    /// 往池里记一份媒体。**同一个哈希只记一次**——已经有了就什么都不做。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put_media(&mut self, hash: &str, ext: &str, bytes: u64) -> Result<(), CatalogError> {
        self.conn
            .execute(
                "INSERT INTO media(hash, ext, bytes, at) VALUES(?1,?2,?3,?4)
                 ON CONFLICT(hash) DO NOTHING",
                params![
                    hash,
                    ext,
                    i64::try_from(bytes).unwrap_or(i64::MAX),
                    super::now_secs()
                ],
            )
            .map(|_| ())
            .map_err(|source| self.err(source))
    }

    /// 在线源下过的这个 URL 算出来是哪一份内容：`(字节数, 哈希)`。
    ///
    /// **下过的量不白下**：一次在线刮削花的是配额，比回盘读一遍贵得多。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn remote_media(&self, url: &str) -> Result<Option<(u64, String)>, CatalogError> {
        self.conn
            .prepare_cached("SELECT bytes, hash FROM media_remote WHERE url = ?1")
            .and_then(|mut statement| {
                statement
                    .query_row(params![url], |row| {
                        Ok((
                            u64::try_from(row.get::<_, i64>(0)?).unwrap_or(0),
                            row.get::<_, String>(1)?,
                        ))
                    })
                    .optional()
            })
            .map_err(|source| self.err(source))
    }

    /// 记下「这个 URL 下过了，内容哈希是这个」。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put_remote_media(
        &mut self,
        url: &str,
        hash: &str,
        bytes: u64,
    ) -> Result<(), CatalogError> {
        self.conn
            .execute(
                "INSERT INTO media_remote(url, bytes, hash) VALUES(?1,?2,?3)
                 ON CONFLICT(url) DO UPDATE SET bytes = excluded.bytes, hash = excluded.hash",
                params![url, i64::try_from(bytes).unwrap_or(i64::MAX), hash],
            )
            .map(|_| ())
            .map_err(|source| self.err(source))
    }

    /// 记下「主库里这个文件算过了，内容哈希是这个」。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put_media_blob(
        &mut self,
        key: &str,
        hash: &str,
        bytes: u64,
    ) -> Result<(), CatalogError> {
        self.conn
            .execute(
                "INSERT INTO media_blob(key, bytes, hash) VALUES(?1,?2,?3)
                 ON CONFLICT(key) DO UPDATE SET bytes = excluded.bytes, hash = excluded.hash",
                params![key, i64::try_from(bytes).unwrap_or(i64::MAX), hash],
            )
            .map(|_| ())
            .map_err(|source| self.err(source))
    }

    /// 一条条走过库里全部**文件**条目：键、字节数、修改时间。
    ///
    /// 刮削拿它找本地媒体。走回调而不是返回一整份 `Vec`：真库里这是 256,128 行。
    /// 大小与修改时间一并交出来，是因为它们要进本地媒体源的**输入指纹**——那正是
    /// 扫描判增量用的那个三元组（ADR-0021 的第三态在这里表现为两个 `None`）。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn for_each_file(&self, each: &mut FileVisitor) -> Result<(), CatalogError> {
        let mut statement = self
            .conn
            .prepare("SELECT key, len, mtime_ns FROM entry WHERE kind = 0 ORDER BY key")
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let key: String = row.get(0).map_err(|source| self.err(source))?;
            let len: Option<i64> = row.get(1).map_err(|source| self.err(source))?;
            let mtime: Option<i64> = row.get(2).map_err(|source| self.err(source))?;
            each(&key, len.and_then(|len| u64::try_from(len).ok()), mtime);
        }
        Ok(())
    }

    /// 媒体池的家底。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn pool_counts(&self) -> Result<PoolCounts, CatalogError> {
        let one = |sql: &str| -> Result<u64, CatalogError> {
            let value: i64 = self
                .conn
                .query_row(sql, [], |row| row.get(0))
                .map_err(|source| self.err(source))?;
            Ok(u64::try_from(value).unwrap_or(0))
        };
        Ok(PoolCounts {
            blobs: one("SELECT COUNT(*) FROM media")?,
            bytes: one("SELECT COALESCE(SUM(bytes), 0) FROM media")?,
            refs: one("SELECT COUNT(*) FROM media_ref")?,
            shared: one("SELECT COUNT(*) FROM (SELECT hash FROM media_ref
                 GROUP BY hash HAVING COUNT(DISTINCT anchor || '\u{1}' || subject) > 1)")?,
        })
    }

    /// 按「字段 × 源」数一遍刮削结论。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn field_counts(&self) -> Result<Vec<FieldCount>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT field, source, COUNT(*), COUNT(DISTINCT anchor || '\u{1}' || subject)
                 FROM scrape_value GROUP BY field, source ORDER BY field, COUNT(*) DESC",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok(FieldCount {
                    field: row.get(0)?,
                    source: row.get(1)?,
                    values: u64::try_from(row.get::<_, i64>(2)?).unwrap_or(0),
                    subjects: u64::try_from(row.get::<_, i64>(3)?).unwrap_or(0),
                })
            })
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 一个源在**变体**这一层的覆盖，按平台：那个平台几个变体、其中几个被它撞上。
    ///
    /// **只数变体锚点。** 中文离线源在两层都说话（票 02），可撞只发生在变体这一层
    /// ——作品那一层是读「名下的变体撞到了哪些条目号」推上去的。于是「撞上多少」这个
    /// 数唯一说得清的地方就是变体：作品锚点上按平台归本来就归不动（一部作品跨平台）。
    ///
    /// **分母是库里那个平台的变体数**，不是这一趟采过的个数。报告是从中立库折出来的
    /// （ADR-0001），而「这一趟看了几个」是 `PlanCounts` 那一侧的事；两个口径混在一份
    /// 表里，读的人无从判断 40% 说的是「四成撞上了」还是「采过的里头四成撞上了」。
    ///
    /// **平台为空的那一行也在里面**（同 [`chinese_by_platform`](Catalog::chinese_by_platform)）：
    /// 滤掉它，按平台加出来的总数就与全库的变体数对不上。归在 `unknown` 那个词底下。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn source_by_platform(
        &self,
        source: &str,
        unknown: &str,
    ) -> Result<Vec<SourceCoverage>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                // `LEFT JOIN` 而不是相关子查询：一个平台一行，撞不上的那些平台照样有行
                // ——那正是这张表要说的话。`COUNT(DISTINCT s.subject)` 不数 NULL，
                // 于是它就是「撞上了的变体数」，而同一个变体上有几条值不影响它。
                "SELECT COALESCE(v.platform, ?3), COUNT(DISTINCT v.key), COUNT(DISTINCT s.subject)
                 FROM variant v
                 LEFT JOIN scrape_value s
                   ON s.subject = v.key AND s.anchor = ?2 AND s.source = ?1
                 GROUP BY 1 ORDER BY 1",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(
                params![source, AnchorKind::Variant.label(), unknown],
                |row| {
                    Ok(SourceCoverage {
                        platform: row.get(0)?,
                        variants: u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                        matched: u64::try_from(row.get::<_, i64>(2)?).unwrap_or(0),
                    })
                },
            )
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 某一类媒体有几条引用、涉及几份**不同的内容**。
    ///
    /// 两个数分开报是有用的：引用 646 条而内容只有 600 份，差出来的 46 份就是
    /// 「只存一份」省下来的。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn media_kind_counts(&self, kind: &str) -> Result<(u64, u64), CatalogError> {
        self.conn
            .query_row(
                "SELECT COUNT(*), COUNT(DISTINCT hash) FROM media_ref WHERE kind = ?1",
                params![kind],
                |row| {
                    Ok((
                        u64::try_from(row.get::<_, i64>(0)?).unwrap_or(0),
                        u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                    ))
                },
            )
            .map_err(|source| self.err(source))
    }

    /// 每个字段各有几个锚点拿到了值：字段 → 锚点数。
    ///
    /// **单独查一次，不从 [`field_counts`](Self::field_counts) 加出来**：同一个锚点被
    /// 两个源填过，加起来就数了两遍；取各源的最大值又只是个下界。报告里印出去的数
    /// 得是真的。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn field_subjects(&self) -> Result<BTreeMap<String, u64>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT field, COUNT(DISTINCT anchor || '\u{1}' || subject)
                 FROM scrape_value GROUP BY field",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                ))
            })
            .map_err(|source| self.err(source))?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (field, count) = row.map_err(|source| self.err(source))?;
            out.insert(field, count);
        }
        Ok(out)
    }

    /// 某个字段上，值里带着某个**记号**的那些锚点：`(锚点种类, 锚点)`，按键排好。
    ///
    /// 眼下的用处只有一个：报告要把**被截断的简介**点得出名（票 03 的验收）——截断
    /// 这件事不许是悄悄发生的。
    ///
    /// **按记号找而不是按长度找。** 长度那条判据要求「截到多少字」这个常量与库里的
    /// 值永远一致，而闸是会调的：调完闸之后，上一趟按老闸截出来的那些行就点不出来了，
    /// 而它们恰恰是最需要被点出来的。记号写在值里，跟着那一行一起活。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn values_marked(
        &self,
        field: &str,
        mark: &str,
    ) -> Result<Vec<(String, String)>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                // `instr` 而不是 `LIKE`：记号里若有 `%` 或 `_`，`LIKE` 会把它们当通配符。
                "SELECT DISTINCT anchor, subject FROM scrape_value
                 WHERE field = ?1 AND instr(value, ?2) > 0
                 ORDER BY anchor, subject",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![field, mark], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 一条条走过全部刮削出来的字段值：锚点种类、锚点、字段、源、值、采集时刻。
    ///
    /// 报告拿它跑一遍**真正的合并**——不跑的话，「按字段级优先级合并」就只是一个
    /// 库函数，产品里没有任何地方证明它在工作。
    ///
    /// **次序按主键排到底**（锚点、主体、字段、源、值），不只排到主体。集合字段
    /// （开发商、发行商、类型）在同一个锚点上并存好几条，导出那一侧原样交给前端
    /// （[`Priorities::pick_all`](crate::scrape::priority::Priorities::pick_all)），
    /// 只排到主体的话，同一份库跑两次交出来的先后可能不一样——查询计划换一个索引就够了。
    /// 于是排到底：先后是**码位序**，稳定、说得出口。数据源里的**原次序**这一层拿不到
    /// （`scrape_value` 上没有那一列，挂单 Q27），所以拿次序说事的人别指望这里。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn for_each_scraped_value(&self, each: &mut ScrapedVisitor) -> Result<(), CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT anchor, subject, field, source, value, at FROM scrape_value
                 ORDER BY anchor, subject, field, source, value",
            )
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let anchor: String = row.get(0).map_err(|source| self.err(source))?;
            let subject: String = row.get(1).map_err(|source| self.err(source))?;
            each(
                &anchor,
                &subject,
                ScrapedValue {
                    field: row.get(2).map_err(|source| self.err(source))?,
                    source: row.get(3).map_err(|source| self.err(source))?,
                    value: row.get(4).map_err(|source| self.err(source))?,
                    evidence: String::new(),
                    at: row.get(5).map_err(|source| self.err(source))?,
                },
            );
        }
        Ok(())
    }

    /// 有刮削结论的锚点各有几个：`(锚点种类, 个数)`。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn scraped_subjects(&self) -> Result<Vec<(String, u64)>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT anchor, COUNT(DISTINCT subject) FROM scrape_value
                 GROUP BY anchor ORDER BY anchor",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                ))
            })
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 人在界面上直接写下的一个字段值，**来源记作裁决**。
    ///
    /// 与 [`put_scraped`](Self::put_scraped) 分开的理由是它**只动这一行**：那条路的语义
    /// 是「一个源在一个锚点上写两次是同一个结果」，于是它会先把这个源在这个锚点上的值
    /// 整批删掉再插——人一次只改一个字段，走那条路等于把他上次写的另外五个字段一起抹了。
    ///
    /// 优先级表把**裁决**排在每个字段的最前（`priorities.toml` 的规则一），所以写下之后
    /// 导出真会用它——而不是「记下了但不生效」。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn put_verdict_value(
        &mut self,
        anchor: AnchorKind,
        subject: &str,
        field: Field,
        value: &str,
        evidence: &str,
    ) -> Result<(), CatalogError> {
        // **裁决一个字段上只有一条**：人改了主意就是改了主意，不是又添了一句。
        // 去重键里带上值之后，光靠 `ON CONFLICT` 做不到这件事——改一个字的新裁决会
        // 落成第二行，旧的那条还在。所以先把这个字段上的裁决删干净再写。
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        tx.execute(
            "DELETE FROM scrape_value
             WHERE anchor = ?1 AND subject = ?2 AND field = ?3 AND source = ?4",
            params![anchor.label(), subject, field.label(), VERDICT],
        )
        .map_err(to_err)?;
        tx.execute(
            "INSERT INTO scrape_value(anchor, subject, field, source, value, evidence, at)
             VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                anchor.label(),
                subject,
                field.label(),
                VERDICT,
                value,
                evidence,
                super::now_secs(),
            ],
        )
        .map_err(to_err)?;
        tx.commit().map_err(to_err)
    }

    /// 撤掉一条**裁决**来源的字段值，让别的源重新说了算。返回撤掉了没有。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn clear_verdict_value(
        &mut self,
        anchor: AnchorKind,
        subject: &str,
        field: Field,
    ) -> Result<bool, CatalogError> {
        self.conn
            .execute(
                "DELETE FROM scrape_value
                 WHERE anchor = ?1 AND subject = ?2 AND field = ?3 AND source = ?4",
                params![anchor.label(), subject, field.label(), VERDICT],
            )
            .map(|removed| removed > 0)
            .map_err(|source| self.err(source))
    }

    /// 把**一个源在一个锚点上**留下的一切清掉：值、媒体引用、以及那条采集记录。
    ///
    /// 用在「这个源这次无话可说、而上次说过话」那一态上——重跑识别把某个源的候选
    /// 清空时就是这样。**留着比缺着更糟**：那些值带着一条指向已经不存在的条目的**依据**，
    /// 事后复核会对不上。
    ///
    /// 与 [`put_scraped`](Self::put_scraped) 一样，**只碰这个源的行**。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn forget_scraped(
        &mut self,
        anchor: &str,
        subject: &str,
        source: &str,
    ) -> Result<(), CatalogError> {
        for sql in [
            "DELETE FROM scrape_value WHERE anchor = ?1 AND subject = ?2 AND source = ?3",
            "DELETE FROM media_ref    WHERE anchor = ?1 AND subject = ?2 AND source = ?3",
            "DELETE FROM scrape_probe WHERE anchor = ?1 AND subject = ?2 AND source = ?3",
        ] {
            self.conn
                .execute(sql, params![anchor, subject, source])
                .map_err(|source| self.err(source))?;
        }
        Ok(())
    }

    /// 把刮削结论整批清掉。**换一套源之后整份重来**走它。
    ///
    /// **`--refresh` 不走这一条**（它只是不看那道输入指纹，见 `scrape::run`）：清空跑在
    /// 采集**之前**，中途按停下或者撞上配额就会只剩一个空壳；而它无条件删掉的
    /// `media_ref` 在「这趟不收媒体」那一档根本写不回来。
    ///
    /// **裁决那一行不碰**（`source = ` [`VERDICT`]）：刮削结论整份可再生，人在详情面板上
    /// 亲手写下的那句不是——中立库之外没有第二份（沉淀库导出的是裁决与匹配两张表，
    /// 不含它），冲掉就永远没了。同一条纪律
    /// [`clear_titles`](Self::clear_titles) 上已经写着。
    ///
    /// **另外两张表照旧整批清**：裁决只落在 `scrape_value` 上，
    /// [`put_verdict_value`](Self::put_verdict_value) 既不写媒体引用也不写采集记录。
    /// 采集记录尤其**必须**清干净——留下一条，下一趟就被输入指纹咬定「这一对采全了」
    /// 而整条跳过，`--refresh` 于是名存实亡。
    ///
    /// **媒体池里的文件一个都不删**——池是内容寻址的，删文件要先确认没人再引用它，
    /// 那是 `vacuum` 那一档的活（调研 13.3(10)），不该混在这里顺手做掉。
    ///
    /// **[`裁决`](VERDICT)那一源的值一条都不删。** 它与刮削结论住在同一张表里，
    /// 但它不是采来的——那是人在界面上一个字一个字敲进去的，重采一趟数据源不该把它
    /// 冲掉（`priorities.toml` 的第一条规则、ADR-0001）。`media_ref` 与 `scrape_probe`
    /// 上没有裁决那一源的行（人写的是值，不是采集记录），所以只有这一张表要设这道闸。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn clear_scraped(&mut self) -> Result<(), CatalogError> {
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        tx.execute("DELETE FROM media_ref", []).map_err(to_err)?;
        tx.execute(
            "DELETE FROM scrape_value WHERE source <> ?1",
            params![VERDICT],
        )
        .map_err(to_err)?;
        tx.execute("DELETE FROM scrape_probe", []).map_err(to_err)?;
        tx.commit().map_err(to_err)
    }

    /// 这一批变体牵动的**作品锚点**：作品名，连它的**代表变体**。
    ///
    /// 代表变体是「一部作品发一次在线查询」时拿去取**判据**的那一个，取的是键最小的
    /// 那一个（`Plan::build` 按键遍历，挑中的就是它）。**它在全部变体里选，不在这一批
    /// 里选**：范围收窄不该让同一部作品换一份判据去查——那会把配额花在两条不同的
    /// 查询上，也会让**输入指纹**跟着范围抖动。
    ///
    /// `only` 是变体的键；`None` 是全库。这一条是[刮削估算](crate::scrape::estimate)
    /// 的入口：它要在按下去之前答出「会发多少个请求」，而把整份计划立起来
    /// （四万多个变体、二十万条候选、全库的文件表）在画帧那条线程上是走不通的。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn work_representatives(
        &self,
        only: Option<&BTreeSet<String>>,
    ) -> Result<Vec<(String, String)>, CatalogError> {
        let Some(keys) = only else {
            return self.representatives_where("", &[]);
        };
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        // **一次绑不下四万多个键。** SQLite 的绑定变量有上限（新版 32,766，老版 999），
        // 而真库全选就是 46,444 个——超过上限时它报的是「too many SQL variables」，
        // 在屏上会变成一句莫名其妙的「中立库读不动」。所以分批问，答案在 Rust 这边并起来。
        //
        // **分批不改结果**：`MIN(key)` 取的是作品名归组之后的最小键，而那一组里装的是
        // **这部作品的全部变体**（`only` 只出现在子查询里，管的是「哪几部作品要采」）
        // ——同一部作品落在哪一批里，代表变体都是同一个。
        let mut out: BTreeMap<String, String> = BTreeMap::new();
        for chunk in keys.iter().collect::<Vec<_>>().chunks(KEYS_PER_QUERY) {
            let holes = std::iter::repeat_n("?", chunk.len())
                .collect::<Vec<_>>()
                .join(",");
            let where_sql = format!(
                "WHERE work.name IN (
                     SELECT 名下.name FROM variant AS 那批
                     JOIN work AS 名下 ON 名下.id = 那批.work_id
                     WHERE 那批.key IN ({holes}))"
            );
            let args: Vec<&str> = chunk.iter().map(|key| key.as_str()).collect();
            for (name, representative) in self.representatives_where(&where_sql, &args)? {
                out.insert(name, representative);
            }
        }
        Ok(out.into_iter().collect())
    }

    /// [`work_representatives`](Self::work_representatives) 的那一句 SQL，
    /// `WHERE` 与它的参数由调用方给。**两条路共用一句**，免得全库那一条与分批那一条
    /// 在 `MIN(key)` 的归组上悄悄分家。
    fn representatives_where(
        &self,
        where_sql: &str,
        args: &[&str],
    ) -> Result<Vec<(String, String)>, CatalogError> {
        // **按作品名归组，不按 `work_id`。** `work.name` 上没有 UNIQUE，重跑识别造出
        // 两行同名的作品是可能的；而 `Plan::build` 的作品锚点是**按名字**攒的
        // （`work_entries` 那张 `BTreeMap<String, WorkSlot>`）。两边归组的键不一样时，
        // 同名那两行会被这边数成两个作品、代表变体也可能挑到 `Plan` 没挑的那一个
        // ——判据不同、指纹不同、请求数与真跑漂开，而那正是这块面板最怕的方向。
        let sql = format!(
            "SELECT work.name, MIN(variant.key)
             FROM variant JOIN work ON work.id = variant.work_id
             {where_sql}
             GROUP BY work.name
             ORDER BY work.name"
        );
        let mut statement = self.conn.prepare(&sql).map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(rusqlite::params_from_iter(args.iter()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// **一个源在某一层上的全部采集记录**：锚点 → 上次的输入指纹。
    ///
    /// 与 [`scrape_inputs`](Self::scrape_inputs) 的差别是问法：那一条问「这个锚点上各源
    /// 说过什么」，这一条问「这个源在这一层上对哪些锚点说过话」。估算要的是后者——
    /// 一次问回来，而不是几千个锚点各问一次。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn scrape_inputs_of(
        &self,
        anchor: &str,
        source: &str,
    ) -> Result<BTreeMap<String, String>, CatalogError> {
        let mut statement = self
            .conn
            .prepare_cached(
                "SELECT subject, input FROM scrape_probe WHERE anchor = ?1 AND source = ?2",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![anchor, source], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|source| self.err(source))?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (subject, input) = row.map_err(|source| self.err(source))?;
            out.insert(subject, input);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 裁决一个字段上永远只有一条() {
        // 去重键里带上值之后，`ON CONFLICT` 做不到这件事了：改一个字的新裁决会落成
        // 第二行，旧的那条还在。人改了主意就是改了主意，不是又添了一句。
        let mut catalog = Catalog::open_in_memory().expect("能开中立库");
        catalog
            .put_verdict_value(
                AnchorKind::Variant,
                "FC/甲.zip",
                Field::Title,
                "魂斗罗",
                "人定的",
            )
            .expect("写得进去");
        catalog
            .put_verdict_value(
                AnchorKind::Variant,
                "FC/甲.zip",
                Field::Title,
                "魂斗罗改",
                "改主意了",
            )
            .expect("写得进去");
        let values = catalog.scraped_values("变体", "FC/甲.zip").expect("读得出");
        let 裁决: Vec<&str> = values
            .iter()
            .filter(|value| value.source == VERDICT)
            .map(|value| value.value.as_str())
            .collect();
        assert_eq!(裁决, vec!["魂斗罗改"]);
    }

    #[test]
    fn 按平台数覆盖时同一个变体上几条值只算一次而平台空着的那一行照样在() {
        // 报告拿这张表回答「N64、DC 这些老平台为什么补不上」（票 06）。两条性质：
        // 撞上的是**变体数**（一个变体上几条值不影响它），以及**平台空着的照样一行**
        // ——滤掉它，按平台加出来的总数就与全库的变体数对不上。
        let mut catalog = Catalog::open_in_memory().expect("能开中立库");
        catalog
            .replace_variants(
                &[
                    变体("FC/撞上的.zip", Some("FC")),
                    变体("FC/没撞上的.zip", Some("FC")),
                    变体("散着的.bin", None),
                ],
                1,
                &crate::platform::Manifest::builtin(),
            )
            .expect("写得进");
        let 采到 = |key: &str, value: &str| Harvested {
            anchor: AnchorKind::Variant.label().to_string(),
            subject: key.to_string(),
            source: "中文离线源".to_string(),
            input: "指纹".to_string(),
            values: vec![HarvestedValue {
                field: Field::Title.label().to_string(),
                value: value.to_string(),
                evidence: "依据".to_string(),
            }],
            media: Vec::new(),
        };
        catalog
            .put_scraped(&[
                采到("FC/撞上的.zip", "魂斗罗"),
                // 同一个变体上的第二条值（别名那一路就是这个形状）。
                采到("FC/撞上的.zip", "魂斗羅"),
                采到("散着的.bin", "魔界村"),
            ])
            .expect("写得进");

        let 覆盖 = catalog
            .source_by_platform("中文离线源", "（平台未知）")
            .expect("读得出");
        assert_eq!(
            覆盖,
            vec![
                SourceCoverage {
                    platform: "FC".to_string(),
                    variants: 2,
                    matched: 1,
                },
                SourceCoverage {
                    platform: "（平台未知）".to_string(),
                    variants: 1,
                    matched: 1,
                },
            ],
        );
        // 别的源在这张表上一个数都不动——它问的是「这个源撞上了多少」。
        let 别人 = catalog
            .source_by_platform("TOSEC", "（平台未知）")
            .expect("读得出");
        assert!(别人.iter().all(|row| row.matched == 0));
    }

    /// 一个最小的变体（同 `catalog::content` 那一侧的写法）。
    fn 变体(key: &str, platform: Option<&str>) -> crate::shape::Variant {
        crate::shape::Variant {
            key: key.to_string(),
            platform: platform.map(ToString::to_string),
            rule: crate::shape::SINGLE_FILE_RULE.to_string(),
            main_key: key.to_string(),
            manual: false,
            files: 1,
            bytes: 100,
            unreadable_files: 0,
            members: vec![(key.to_string(), crate::shape::Role::Main)],
        }
    }

    #[test]
    fn 整份清掉刮削结论时裁决一条都不删() {
        // **裁决与刮削结论住在同一张表里**（源名是「裁决」），而它不是采来的
        // ——那是人在界面上一个字一个字敲进去的，重建不出来。
        // 一句光秃秃的 `DELETE FROM scrape_value` 会把它一起带走。
        let mut catalog = Catalog::open_in_memory().expect("能开中立库");
        catalog
            .put_verdict_value(AnchorKind::Work, "魂斗罗", Field::Title, "魂斗罗", "人定的")
            .expect("写得进");
        catalog
            .put_scraped(&[Harvested {
                anchor: AnchorKind::Work.label().to_string(),
                subject: "魂斗罗".to_string(),
                source: "TOSEC".to_string(),
                input: "指纹".to_string(),
                values: vec![HarvestedValue {
                    field: Field::Year.label().to_string(),
                    value: "1988".to_string(),
                    evidence: "条目名".to_string(),
                }],
                media: Vec::new(),
            }])
            .expect("写得进");

        catalog.clear_scraped().expect("清得掉");

        let 剩下的 = catalog.scraped_values("作品", "魂斗罗").expect("读得出");
        assert_eq!(剩下的.len(), 1, "该只剩裁决那一条");
        assert_eq!(剩下的[0].source, VERDICT);
        assert_eq!(剩下的[0].value, "魂斗罗");
    }

    #[test]
    fn 作品代表变体这一问吃得下四万多个键() {
        // **真库全选就是 46,444 个键**，而 SQLite 的绑定变量有上限（新版 32,766）。
        // 一句塞完的写法在这个量级上报的是「too many SQL variables」，
        // 而那句错在刮削面板上会变成一句莫名其妙的「中立库读不动」——
        // 屏上于是既没有估算也没有原因。分批之后它只是多几趟查询。
        let mut catalog = Catalog::open_in_memory().expect("能开中立库");
        let variants: Vec<crate::shape::Variant> = (0..40_000)
            .map(|i| 变体(&format!("库/FC/第{i:06}个.zip"), Some("FC")))
            .collect();
        catalog
            .replace_variants(&variants, 1, &crate::platform::Manifest::builtin())
            .expect("写得进");
        let keys: BTreeSet<String> = variants.iter().map(|v| v.key.clone()).collect();

        // 一个都没认出作品，所以答案是空的——**这一条钉的是「问得出去」不是「答什么」**。
        let works = catalog
            .work_representatives(Some(&keys))
            .expect("四万多个键也该问得出去");
        assert!(works.is_empty(), "这些变体一个作品都没挂上");
    }

    #[test]
    fn 引用一份池里没有的媒体会被外键挡下() {
        // `media_ref.hash REFERENCES media(hash)` **不是装饰**：`rusqlite` 的 bundled
        // SQLite 编译时开了 `SQLITE_DEFAULT_FOREIGN_KEYS=1`，外键检查默认就是开着的
        // （票 07 为此在 `variant` 上补过两条索引，见 `catalog::content`）。
        // 这条测试把那个前提钉住——哪天换了 SQLite 的编译选项，这里会先响。
        let mut catalog = Catalog::open_in_memory().expect("能开中立库");
        let 悬空 = Harvested {
            anchor: "变体".to_string(),
            subject: "FC/某个.zip".to_string(),
            source: "本地媒体".to_string(),
            input: "指纹".to_string(),
            values: Vec::new(),
            media: vec![HarvestedMedia {
                kind: "封面".to_string(),
                hash: "池里根本没有这一份".to_string(),
                evidence: "编的".to_string(),
            }],
        };
        assert!(catalog.put_scraped(&[悬空]).is_err());
    }

    #[test]
    fn 一个源的值删得干净而别的源一条不碰() {
        let mut catalog = Catalog::open_in_memory().expect("能开中立库");
        let 采到 = |source: &str, value: &str| Harvested {
            anchor: "作品".to_string(),
            subject: "魔界村".to_string(),
            source: source.to_string(),
            input: format!("{source} 的指纹"),
            values: vec![HarvestedValue {
                field: "标题".to_string(),
                value: value.to_string(),
                evidence: "依据".to_string(),
            }],
            media: Vec::new(),
        };
        catalog
            .put_scraped(&[
                采到("No-Intro", "Ghosts 'n Goblins"),
                采到("TOSEC", "魔界村"),
            ])
            .expect("写得进");
        catalog
            .forget_scraped("作品", "魔界村", "TOSEC")
            .expect("删得掉");

        let 剩下 = catalog.scraped_values("作品", "魔界村").expect("读得出");
        assert_eq!(剩下.len(), 1);
        assert_eq!(剩下[0].source, "No-Intro");
        // 采集记录也跟着走，否则下一趟会以为 TOSEC 采过了。
        let 记录 = catalog.scrape_inputs("作品", "魔界村").expect("读得出");
        assert_eq!(记录.keys().collect::<Vec<_>>(), vec!["No-Intro"]);
    }

    #[test]
    fn 整批清结论时数据源的值与采集记录都清掉而裁决留着() {
        // `--refresh` 走的就是这一条。刮削结论整份可再生，人亲手写下的那句不是——
        // 中立库之外没有第二份。**采集记录必须一起清掉**：留下一条，下一趟就被输入
        // 指纹咬定「这一对采全了」而整条跳过，重采一遍于是名存实亡。
        let mut catalog = Catalog::open_in_memory().expect("能开中立库");
        catalog
            .put_scraped(&[Harvested {
                anchor: AnchorKind::Work.label().to_string(),
                subject: "魔界村".to_string(),
                source: "TOSEC".to_string(),
                input: "指纹".to_string(),
                values: vec![HarvestedValue {
                    field: Field::Year.label().to_string(),
                    value: "1985".to_string(),
                    evidence: "依据".to_string(),
                }],
                media: Vec::new(),
            }])
            .expect("写得进");
        catalog
            .put_verdict_value(AnchorKind::Work, "魔界村", Field::Year, "1986", "人定的")
            .expect("写得下");

        catalog.clear_scraped().expect("清得掉");

        let 剩下 = catalog.scraped_values("作品", "魔界村").expect("读得出");
        assert_eq!(剩下.len(), 1, "只该剩人写下的那一条");
        assert_eq!(剩下[0].source, VERDICT);
        assert_eq!(剩下[0].value, "1986");
        assert!(
            catalog
                .scrape_inputs("作品", "魔界村")
                .expect("读得出")
                .is_empty(),
            "采集记录一条不留，下一趟才真的重采"
        );
    }
}
