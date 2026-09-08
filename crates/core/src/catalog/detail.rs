//! 一个变体的**详情**：库浏览那一屏点开一行之后要摆出来的全部原料。
//!
//! ## 为什么是一次查询而不是界面上十次
//!
//! 详情面板要的东西散在八张表里（变体、成员、作品、发行版、合集、识别结论、标题、
//! 媒体引用）。让界面自己去凑，等于把「一个变体由哪些东西构成」这条领域知识抄进
//! `crates/gui`——而那一层随时可能被整个换掉（ADR-0005 的降级预案）。所以凑的动作
//! 在这里，界面只拿一个 [`VariantDetail`] 往外画。
//!
//! ## 全部按键查，一条都不整表读
//!
//! `catalog::content` 里已经有 [`work_names`](Catalog::work_names)、
//! [`releases`](Catalog::releases)、[`preferred_variants`](Catalog::preferred_variants)
//! 这类**整份读回来**的入口，那是给导出与报告用的——它们本来就要走遍全库。
//! 详情面板不是：人点一行就要出一次，真库上作品有 9,226 个、自动通过的候选有 38,963
//! 条，每点一行整读一遍就是几十毫秒的卡顿。所以这里另开几条按主键查的路。
//!
//! ## ADR-0012：**首选变体与标题来源解耦**
//!
//! 面板上这两样挨着摆，最容易被读成一件事，而它们恰恰不是：首选变体决定**默认启动
//! 哪一个**，中文标题永远取**官中版的官方译名**——即使首选启动的是汉化版。
//! [`VariantDetail::chinese_title`] 把那条叫法单独交出来，就是为了让界面能把这句话
//! 摆在人眼前，而不是让人自己去推。

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use rusqlite::{OptionalExtension, params};

use super::content::{ReleaseRow, VARIANT_COLUMNS, VariantRow, read_variant_row};
use super::identify::State;
use super::scrape::ScrapedValue;
use super::title::TitleRow;
use super::{Catalog, CatalogError};
use crate::adapter::converge::{Preference, preference_for};
use crate::dat::chinese::ChineseMark;
use crate::scrape::pool::MediaPool;
use crate::scrape::{AnchorKind, MediaKind};
use crate::shape::Role;
use crate::title::{Chosen, Language, TitleKind, TitleSet};

/// 一个变体在**首选变体**那条规则里排第几，连它凭什么排在那儿。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sibling {
    /// 那个变体。
    pub row: VariantRow,
    /// 凭什么：裁决 / 汉化 / 官中 / 日版 / 其他。
    pub preference: Preference,
}

/// 一类媒体这个变体有多少。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaHave {
    /// 封面 / 截图 / 视频 / 其他。
    pub kind: MediaKind,
    /// 库里记着几条这一类的**引用**。
    pub refs: u64,
    /// 其中有几条真的在**媒体池**里躺着那个文件；**没查池子时是 `None`**。
    ///
    /// **与引用数分开数**：库里记着一条引用、池里却没有那份字节，是导出时一张都铺不出去
    /// 的那一档（`sync::media::lay` 的 `not_in_pool`）。合成一个数的话，面板会说
    /// 「有封面」而同步时封面是空的。
    ///
    /// **「没查」与「查了、没有」也得分开**：媒体池整个不在位（还没刮削过、换了台机器）
    /// 时报一句「一张都没有」，与如实说「没查池子」不是一回事——前者会让人去重跑刮削，
    /// 而问题其实出在工作目录上（同 ADR-0021 那条「不可读是第三种状态」的道理）。
    pub in_pool: Option<u64>,
}

/// 一条**媒体引用**：库里记着一份什么、来自哪个源、在不在**媒体池**里。
///
/// 与 [`MediaHave`] 那份计数分开：计数回答「缺不缺」，这一条回答「是哪一份、去哪儿找」。
/// 「媒体资源在界面中可见」要的是后者——只给一个数字，人连它落在池里哪个文件都说不出。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaItem {
    /// 挂在**作品**上还是**变体**上（ADR-0009）。
    pub anchor: AnchorKind,
    /// 封面 / 截图 / 视频 / 其他。
    pub kind: MediaKind,
    /// 哪个源给的。
    pub source: String,
    /// 内容哈希，也就是它在**媒体池**里的主键（**文件名不是媒体的主键**，ADR-0009）。
    pub hash: String,
    /// 扩展名。池里那个文件靠 `(哈希, 扩展名)` 找。
    pub ext: String,
    /// 它在**媒体池**里的落点；没查池子时是 `None`。
    pub at: Option<PathBuf>,
    /// 池里真有那个文件吗；没查池子时是 `None`。
    pub in_pool: Option<bool>,
    /// **依据**：这条引用是怎么来的。
    pub evidence: String,
}

/// 一条**刮削来的字段值**，连它挂在哪一层。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueItem {
    /// 挂在**作品**上还是**变体**上。年份挂作品、汉化组挂变体（ADR-0012）。
    pub anchor: AnchorKind,
    /// 那条值。
    pub value: ScrapedValue,
}

impl ValueItem {
    /// 这一条是不是人工**裁决**写下的。
    #[must_use]
    pub fn is_verdict(&self) -> bool {
        self.value.source == crate::scrape::priority::VERDICT
    }
}

/// 要在面板上点名「缺不缺」的那几类媒体。
///
/// [`MediaKind::Other`] **不在里面**：它是「认不出是什么的图」，缺它不是个缺口
/// ——导出时它一张都不铺（猜错了就是把说明书当封面）。
pub const EXPECTED_MEDIA: [MediaKind; 3] =
    [MediaKind::Cover, MediaKind::Screenshot, MediaKind::Video];

/// 一个变体的详情。
#[derive(Debug, Clone, PartialEq)]
pub struct VariantDetail {
    /// 变体本身。
    pub row: VariantRow,
    /// 属于哪个**作品**；识别还没认出来时是 `None`。
    pub work: Option<String>,
    /// 基于哪条**发行版**；同人移植与 homebrew 没有，那本身就是给识别管线的信号。
    pub release: Option<ReleaseRow>,
    /// 在哪几个**合集**里（与平台正交，ADR-0011）。
    pub collections: Vec<String>,
    /// 发行版标着的语言，拆开的。
    pub languages: Vec<String>,
    /// 这一轮的识别结论；没识别过时是 `None`。
    pub state: Option<State>,
    /// 没定下来的话，**为什么**。
    pub reason: Option<String>,
    /// 文件成员，连各自的身份。
    pub members: Vec<(String, Role)>,
    /// 这个作品的**标题集合**：全部叫法，每条带语言、类型、来源与依据。
    pub titles: Vec<TitleRow>,
    /// 从标题集合里挑出来的**显示标题**与**排序标题**；作品未知时是 `None`。
    pub display: Option<Chosen>,
    /// 中文标题取的是哪一条叫法。读它走 [`Self::chinese_title`]。
    chinese_title: Option<TitleRow>,
    /// **首选变体**的人工裁决；没人裁过是 `None`（那时按规则算，见 [`Self::preferred_now`]）。
    pub preferred: Option<String>,
    /// 同一个作品、同一个平台下的全部变体，**按首选规则排好**，第一个就是眼下的首选。
    pub siblings: Vec<Sibling>,
    /// 媒体各类各有多少。
    pub media: Vec<MediaHave>,
    /// 媒体**一条条列出来**：是哪一份、来自哪个源、在池里哪儿。
    pub media_items: Vec<MediaItem>,
    /// **刮削来的字段值**：年份、发行商、开发商、类型、简介、汉化组、标题。
    ///
    /// 同一个字段可以有好几条——**三元组并存，不互相覆盖**（`catalog::scrape`）。
    /// 哪一条最终写进导出，由优先级表说了算，而**裁决**排在每个字段的最前。
    pub values: Vec<ValueItem>,
}

impl VariantDetail {
    /// 眼下**实际生效**的首选变体是哪一个。
    ///
    /// 人裁过就是人裁的那个；没裁过就是规则算出来的第一名（汉化 > 官中 > 日版 > 其他）。
    /// 作品未知时没有这回事——那种变体在导出时自成一个条目。
    #[must_use]
    pub fn preferred_now(&self) -> Option<&str> {
        self.siblings.first().map(|first| first.row.key.as_str())
    }

    /// 这个变体自己就是首选吗。
    #[must_use]
    pub fn is_preferred(&self) -> bool {
        self.preferred_now() == Some(self.row.key.as_str())
    }

    /// **中文标题**取的是哪一条叫法（ADR-0012）。
    ///
    /// 官中版的**官方译名**优先，其次才是别的中文叫法。**它与首选变体无关**——
    /// 首选启动的是汉化版，中文标题照旧取官中版的官方译名。
    ///
    /// 挑法与导出**同一条**（[`title::best_chinese`](crate::title::best_chinese) 用的
    /// 是 `title::choose` 那把排序键）：面板上写着「中文标题取的是这一条」，而导出时
    /// 写进去的是另一条，那句话就是在骗人。
    #[must_use]
    pub fn chinese_title(&self) -> Option<&TitleRow> {
        self.chinese_title.as_ref()
    }

    /// 缺哪几类媒体：[`EXPECTED_MEDIA`] 里一张都用不上的那些。
    ///
    /// 「用得上」＝库里记着引用，**而且**（查过池子的话）池里真有那个文件。
    #[must_use]
    pub fn missing_media(&self) -> Vec<MediaKind> {
        EXPECTED_MEDIA
            .into_iter()
            .filter(|kind| {
                !self.media.iter().any(|have| {
                    have.kind == *kind && have.refs > 0 && have.in_pool.is_none_or(|got| got > 0)
                })
            })
            .collect()
    }

    /// 库里记着引用、**池里却没有那个文件**的媒体有几条。导出时它们一张都铺不出去。
    ///
    /// 没查过池子时是 0——那时说不出这个数，而**编一个出来比不说更糟**。
    #[must_use]
    pub fn dangling_media(&self) -> u64 {
        self.media
            .iter()
            .filter_map(|have| Some(have.refs.saturating_sub(have.in_pool?)))
            .sum()
    }
}

impl Catalog {
    /// 一个变体的详情；库里没有这个键时是 `None`。
    ///
    /// `priorities` 用来挑**显示标题**——与导出走的是同一个函数
    /// （[`title::choose`](crate::title::choose)），于是面板上写着的那个标题，
    /// 就是同步到掌机上会看见的那个。
    ///
    /// `pool` 给了就顺便查一遍**媒体池**里那些文件在不在。不给就只按库里记着的引用算，
    /// [`MediaHave::in_pool`] 那一栏留空——**「没查」不等于「没有」**。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn variant_detail(
        &self,
        key: &str,
        priorities: &crate::scrape::Priorities,
        pool: Option<&MediaPool>,
    ) -> Result<Option<VariantDetail>, CatalogError> {
        let Some(row) = self.variant(key)? else {
            return Ok(None);
        };
        let work = match row.work_id {
            Some(id) => self.work_name(id)?,
            None => None,
        };
        let release = match row.release_id {
            Some(id) => self.release(id)?,
            None => None,
        };
        let languages = release
            .as_ref()
            .and_then(|release| release.languages.as_deref())
            .map(ReleaseRow::language_codes)
            .unwrap_or_default();
        let (state, reason) = match self.identification_of(key)? {
            Some((state, reason)) => (Some(state), reason),
            None => (None, None),
        };
        let titles = match &work {
            Some(work) => self.titles_of(work)?,
            None => Vec::new(),
        };
        let set = work.as_ref().map(|work| TitleSet {
            work: work.clone(),
            entries: titles.clone(),
        });
        let display = set
            .as_ref()
            .map(|set| crate::title::choose(set, priorities));
        let chinese_title = set
            .as_ref()
            .and_then(|set| crate::title::best_chinese(set, priorities))
            .cloned();
        let preferred = match (&work, &row.platform) {
            (Some(work), Some(platform)) => self.preferred_variant(work, platform)?,
            _ => None,
        };
        let siblings = match (&work, row.work_id, &row.platform) {
            (Some(_), Some(work_id), Some(platform)) => {
                self.rank_siblings(work_id, platform, preferred.as_deref())?
            }
            _ => Vec::new(),
        };
        let media = self.media_have(key, work.as_deref(), pool)?;
        let media_items = self.media_items(key, work.as_deref(), pool)?;
        let mut values = Vec::new();
        for (anchor, subject) in anchors_of(key, work.as_deref()) {
            for value in self.scraped_values(anchor.label(), subject)? {
                values.push(ValueItem { anchor, value });
            }
        }

        Ok(Some(VariantDetail {
            collections: self.collections_of(key)?,
            members: self.variant_members(key)?,
            row,
            work,
            release,
            languages,
            state,
            reason,
            titles,
            display,
            chinese_title,
            preferred,
            siblings,
            media,
            media_items,
            values,
        }))
    }

    /// 一个作品的名字；没这一行时是 `None`。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn work_name(&self, id: i64) -> Result<Option<String>, CatalogError> {
        self.conn
            .prepare_cached("SELECT name FROM work WHERE id = ?1")
            .and_then(|mut statement| {
                statement
                    .query_row(params![id], |row| row.get(0))
                    .optional()
            })
            .map_err(|source| self.err(source))
    }

    /// 一条发行版；没这一行时是 `None`。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn release(&self, id: i64) -> Result<Option<ReleaseRow>, CatalogError> {
        self.conn
            .prepare_cached(
                "SELECT id, work_id, platform, region, serial, languages FROM release WHERE id = ?1",
            )
            .and_then(|mut statement| {
                statement
                    .query_row(params![id], |row| {
                        Ok(ReleaseRow {
                            id: row.get(0)?,
                            work_id: row.get(1)?,
                            platform: row.get(2)?,
                            region: row.get(3)?,
                            serial: row.get(4)?,
                            languages: row.get(5)?,
                        })
                    })
                    .optional()
            })
            .map_err(|source| self.err(source))
    }

    /// 这个作品在这个平台上的**首选变体**裁决；没人裁过是 `None`。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn preferred_variant(
        &self,
        work: &str,
        platform: &str,
    ) -> Result<Option<String>, CatalogError> {
        self.conn
            .prepare_cached(
                "SELECT variant_key FROM preferred_variant WHERE work = ?1 AND platform = ?2",
            )
            .and_then(|mut statement| {
                statement
                    .query_row(params![work, platform], |row| row.get(0))
                    .optional()
            })
            .map_err(|source| self.err(source))
    }

    /// 撤掉一条**首选变体**裁决，回到规则算出来的那一个。返回撤掉了没有。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn clear_preferred_variant(
        &mut self,
        work: &str,
        platform: &str,
    ) -> Result<bool, CatalogError> {
        self.conn
            .execute(
                "DELETE FROM preferred_variant WHERE work = ?1 AND platform = ?2",
                params![work, platform],
            )
            .map(|removed| removed > 0)
            .map_err(|source| self.err(source))
    }

    /// 删掉**标题集合**里的一条叫法。返回删掉了没有。
    ///
    /// 主键那五样一起给，因为它们**就是**去重键——少给一样就会连坐删掉别的源给的同名
    /// 叫法，而那些是重跑刮削才能再造出来的东西。
    ///
    /// **它只删这一行。** 标题集合是[折出来的一份投影](crate::title::fold)，非**裁决**
    /// 来源的那些下一趟重折会原样重建——所以人按下界面上那个「删」时走的不是这里，
    /// 是 [`title::suppress`](crate::title::suppress)：它删完还往**沉淀库**里记一条
    /// **压制**，重折照它筛一遍（票 `parking-3/13`、挂账 D157）。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn remove_title(
        &mut self,
        work: &str,
        language: Language,
        kind: TitleKind,
        source: &str,
        value: &str,
    ) -> Result<bool, CatalogError> {
        self.conn
            .execute(
                "DELETE FROM title
                 WHERE work = ?1 AND language = ?2 AND kind = ?3 AND source = ?4 AND value = ?5",
                params![work, language.code(), kind.label(), source, value],
            )
            .map(|removed| removed > 0)
            .map_err(|source| self.err(source))
    }

    /// 同一个作品、同一个平台下的全部变体，**按首选规则排好**。
    ///
    /// 排法与导出那一步一个字不差（[`preference_for`]），于是面板上排第一的那个，
    /// 就是同步到掌机上会默认启动的那个。
    fn rank_siblings(
        &self,
        work_id: i64,
        platform: &str,
        picked: Option<&str>,
    ) -> Result<Vec<Sibling>, CatalogError> {
        let mut statement = self
            .conn
            .prepare_cached(&format!(
                "SELECT {VARIANT_COLUMNS} FROM variant
                 WHERE work_id = ?1 AND platform = ?2 ORDER BY key"
            ))
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![work_id, platform], read_variant_row)
            .map_err(|source| self.err(source))?;
        let rows: Vec<VariantRow> = rows
            .collect::<Result<_, _>>()
            .map_err(|source| self.err(source))?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            // 中文记号从**自动通过**的候选上读回来（与 `adapter::converge` 同一条路），
            // 只查这几个变体自己的——整表走一遍是 38,963 条。
            let marks: BTreeSet<ChineseMark> = self
                .candidates_of(&row.key)?
                .into_iter()
                .filter(|candidate| candidate.accepted)
                .filter_map(|candidate| candidate.chinese)
                .collect();
            let release = match row.release_id {
                Some(id) => self.release(id)?,
                None => None,
            };
            let preference = preference_for(&row.key, &marks, release.as_ref(), picked);
            out.push(Sibling { row, preference });
        }
        // **首选排最前**，同一档之内按键排——同一份库看两次必须是同一个次序。
        out.sort_by(|a, b| {
            a.preference
                .cmp(&b.preference)
                .then_with(|| a.row.key.cmp(&b.row.key))
        });
        Ok(out)
    }

    /// 这个变体各类媒体各有多少：库里记着几条引用、其中几条池里真有那个文件。
    ///
    /// **作品锚点与变体锚点合起来看**：封面挂在作品上、这个变体目录里那几张图挂在变体上
    /// （`CONTEXT.md` 的「媒体池」与 ADR-0009），而人问的是「这个东西导出去有没有封面」。
    fn media_have(
        &self,
        key: &str,
        work: Option<&str>,
        pool: Option<&MediaPool>,
    ) -> Result<Vec<MediaHave>, CatalogError> {
        let items = self.media_items(key, work, pool)?;
        let mut refs: BTreeMap<MediaKind, (u64, u64)> = BTreeMap::new();
        for item in &items {
            let slot = refs.entry(item.kind).or_default();
            slot.0 += 1;
            slot.1 += u64::from(item.in_pool.unwrap_or(false));
        }
        Ok(MediaKind::all()
            .into_iter()
            .map(|kind| {
                let (refs, in_pool) = refs.get(&kind).copied().unwrap_or_default();
                MediaHave {
                    kind,
                    refs,
                    in_pool: pool.map(|_| in_pool),
                }
            })
            .collect())
    }

    /// 这个变体的媒体**一条条列出来**：是哪一份、来自哪个源、在池里哪儿。
    ///
    /// **作品锚点与变体锚点合起来看**：封面挂在作品上、这个变体目录里那几张图挂在变体上
    /// （ADR-0009），而人问的是「这个东西导出去有没有封面」。
    ///
    /// 一条一条读而不是 `GROUP BY` 数出来：**在不在池里**这件事库里没有，只有拿哈希与
    /// 扩展名去问池子才知道（[`MediaPool::contains`]）。
    fn media_items(
        &self,
        key: &str,
        work: Option<&str>,
        pool: Option<&MediaPool>,
    ) -> Result<Vec<MediaItem>, CatalogError> {
        let mut out = Vec::new();
        for (anchor, subject) in anchors_of(key, work) {
            let mut statement = self
                .conn
                .prepare_cached(
                    "SELECT r.kind, r.source, r.hash, m.ext, r.evidence
                     FROM media_ref r JOIN media m ON m.hash = r.hash
                     WHERE r.anchor = ?1 AND r.subject = ?2
                     ORDER BY r.kind, r.source, r.hash",
                )
                .map_err(|source| self.err(source))?;
            let rows = statement
                .query_map(params![anchor.label(), subject], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                })
                .map_err(|source| self.err(source))?;
            for row in rows {
                let (label, source, hash, ext, evidence) = row.map_err(|e| self.err(e))?;
                // **认不出的类别不静默归进「其他」**：那会把「这是张说明书」与
                // 「这一版不认得这个类别」说成同一件事。认不出就整条不算。
                let Some(kind) = MediaKind::from_label(&label) else {
                    continue;
                };
                out.push(MediaItem {
                    anchor,
                    kind,
                    source,
                    at: pool.map(|pool| pool.path_of(&hash, &ext)),
                    in_pool: pool.map(|pool| pool.contains(&hash, &ext)),
                    hash,
                    ext,
                    evidence,
                });
            }
        }
        Ok(out)
    }
}

/// 一个变体的两个**锚点**：它自己，以及它属于的那个作品。
///
/// **两处合起来看**：封面与年份挂在作品上，汉化组与这个变体目录里那几张图挂在变体上
/// （ADR-0009、ADR-0012）。人问的是「这个东西导出去带什么」，而不该被要求先弄清
/// 某个字段挂在哪一层。
fn anchors_of<'a>(key: &'a str, work: Option<&'a str>) -> Vec<(AnchorKind, &'a str)> {
    let mut out = vec![(AnchorKind::Variant, key)];
    if let Some(work) = work {
        out.push((AnchorKind::Work, work));
    }
    out
}
