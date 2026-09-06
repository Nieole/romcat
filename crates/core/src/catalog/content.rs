//! 中立库里的**三层内容层级**：作品 / 发行版 / 变体，外加与平台正交的**合集**。
//!
//! ## 为什么现在就把三层立起来
//!
//! 票 07 之前识别还没开工，**作品**与**发行版**两张表基本是空的。照样现在就立，是因为
//! 这一层的形状决定后面每一张票：识别往发行版上挂候选、刮削往作品上挂简介、导出按作品
//! 收敛、子库按变体挑选。等到那时再改，改的是已经写满数据的表。
//!
//! ## 三层各自的空与不空
//!
//! - **变体**是磁盘上一份实际可玩的东西，由**成型**产出（[`crate::shape`]），现在就是满的。
//! - **发行版**指向一个作品；`platform` 与 `languages` 分开存，是 ADR-0019 那道**世代裂缝**
//!   的落地形状：卡带与光盘世代的官中是**独立一条发行版**（有独立序列号、DAT 里独立一条），
//!   数字世代的中文只是**同一条发行版的语言属性**（Switch 港服与美服 69.4% 共用 TitleID）。
//!   一张表同时装得下两侧，不必为数字世代造现实中不存在的发行版。
//! - **变体到发行版的链接可空**，而且**空本身是信息**：同人移植与 homebrew 没有任何官方
//!   发行版，直接挂在作品下——没有发行版链接这件事就告诉识别管线「别拿它去撞 DAT」。
//!
//! ## 平台与合集正交
//!
//! **平台是 `variant` 上的一列，合集是自己的一张表加一张关系表——两者不共用任何表、
//! 任何字段**（ADR-0011）。用户当前的库里「目录 = 平台 = 前端的 collection」三者恰好
//! 重合，但捏成一个之后「我通关过的」「适合双人玩的」这类跨平台合集就永远表达不了了。
//!
//! ## 重新成型不许抹掉已有的结论
//!
//! [`Catalog::replace_variants`] 会整张换掉变体，但**保住 `work_id` / `release_id`**：
//! 那是裁决攒出来的，改一条成型规则不该把它们冲掉。合集成员同理——那张关系表根本不动，
//! 变体暂时消失（盘没插、目录改了名）之后再回来，它还在原来的合集里。

use std::collections::BTreeMap;

use rusqlite::{OptionalExtension, params};

use super::identify::Provenance;
use super::{Catalog, CatalogError};
use crate::platform::Manifest;
use crate::scan::aggregate::ShapingAcc;
use crate::shape::{Role, Variant};

/// 三层内容层级与合集的表。
pub(super) const CONTENT_SCHEMA: &str = "\
-- **作品**：跨平台、跨地区的同一个游戏概念。刮削来的简介、封面默认挂在这一层。
-- 票 07 之前这张表是空的——识别还没开工，谁也不知道哪个变体属于哪个作品。
CREATE TABLE IF NOT EXISTS work(
    id     INTEGER PRIMARY KEY,
    name   TEXT NOT NULL,
    -- 这一行是**识别**自己造的，还是**裁决**定下来的（票 07）。重跑识别要清掉
    -- 自己上一轮造的，而裁决攒出来的一行都不能碰。
    origin TEXT NOT NULL
) STRICT;

-- **发行版**：某个平台、某个地区的一次官方发行，通常对应 No-Intro / Redump 里的一条记录。
--
-- `languages` 与 `platform` 分开存，装的是 ADR-0019 那道世代裂缝：老世代的官中是
-- 独立一条发行版（独立序列号、独立哈希），数字世代的中文只是同一条发行版的语言属性。
CREATE TABLE IF NOT EXISTS release(
    id        INTEGER PRIMARY KEY,
    work_id   INTEGER NOT NULL REFERENCES work(id),
    platform  TEXT,
    region    TEXT,
    serial    TEXT,
    languages TEXT,
    origin    TEXT NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS release_work ON release(work_id);

-- **变体**：磁盘上一份实际可玩的东西。键是主文件的键，或者整棵目录树的根。
--
-- `platform` **可空**：认不出平台的内容照常入库，平台未知不构成拒绝入库的理由。
-- `work_id` / `release_id` 也可空，而且空是有含义的（见模块文档）。
CREATE TABLE IF NOT EXISTS variant(
    key        TEXT PRIMARY KEY,
    platform   TEXT,
    rule       TEXT NOT NULL,
    main_key   TEXT NOT NULL,
    files      INTEGER NOT NULL,
    -- **容量是下界**：元数据读不到的成员按 0 计入（ADR-0021），`unreadable` 记着少算了几个。
    bytes      INTEGER NOT NULL,
    unreadable INTEGER NOT NULL,
    manual     INTEGER NOT NULL,
    work_id    INTEGER REFERENCES work(id),
    release_id INTEGER REFERENCES release(id)
) STRICT;

CREATE INDEX IF NOT EXISTS variant_platform ON variant(platform);

-- 这两条索引不是为了查得快，是为了**删得动**：`rusqlite` 的 bundled SQLite
-- 编译时开了 `SQLITE_DEFAULT_FOREIGN_KEYS=1`，外键检查默认是**开着**的。
-- 于是重跑识别删掉自己上一轮建的一万多条发行版时，每删一行都要在 variant 上找
-- 「还有没有人指着我」——没有索引就是一次全表扫描，实测 46,444 个变体上要 3 分半。
CREATE INDEX IF NOT EXISTS variant_work ON variant(work_id);
CREATE INDEX IF NOT EXISTS variant_release ON variant(release_id);

-- 界面那张表按这几列排序时走的索引（`catalog::browse`）。每条都缀上 `key`，
-- 因为排序键本身有大量并列，并列行的次序不定死翻页就会漏行与重行——而缀了 `key`
-- 之后，`ORDER BY bytes DESC, key DESC` 正好是这条索引倒着扫，不必落临时表排序。
--
-- 值不值得多这四条索引：不加的话，十万行按容量翻到第九万行实测要 **96 毫秒**一次，
-- 那是六帧的预算，滚动条一拖就是肉眼可见的卡顿；加了之后是 **1 毫秒**级。
-- 代价是索引本身的体积，真机四万多个变体上是几兆。
CREATE INDEX IF NOT EXISTS variant_platform_key ON variant(platform, key);
CREATE INDEX IF NOT EXISTS variant_rule_key ON variant(rule, key);
CREATE INDEX IF NOT EXISTS variant_files_key ON variant(files, key);
CREATE INDEX IF NOT EXISTS variant_bytes_key ON variant(bytes, key);

-- **作品级主列表那一条 `GROUP BY` 走的索引**（票 `gui-redesign/13`）。
--
-- 上面四条接的是变体表的 `ORDER BY`；这一条接的是**分组**。主列表按「一个作品一行」
-- 出行，分组键是 `(work_id, 没认出作品时那个变体自己的键)`——`work_id` 上虽然已经有
-- `variant_work`，但第二项是个**表达式**，索引里没有，于是 SQLite 只能把 46,428 行
-- 全塞进一口临时 b 树里排一遍才分得出组。把那个表达式原样写进索引，那口 b 树就没了
-- （查询计划从 `USE TEMP B-TREE FOR GROUP BY` 变成 `SCAN variant USING INDEX`）。
--
-- 单把这一条加上、别的都不动，实测（release，46,428 个变体收敛成 10,978 行）：
-- 数一次总行数与取一页那条查询**各快一倍以上**。整条路的改前改后见
-- `docs/library-facts.md`「作品级主列表翻一页要多久」，量的命令是 `--bench-paging`。
--
-- ⚠️ 表达式要与查询里那一条**是同一个表达式**（`catalog::browse` 的 `WORK_GROUP_BY`）。
-- 「同一个」比的是名字解析之后那棵树，不是字面：这里写 `work_id` / `key`，查询里写
-- `variant.work_id` / `variant.key`，解析到同一列，认得上（索引定义里反倒不许带表名）。
-- 但**换一个写法就认不上了**——比如把 `CASE` 改成 `iif`、或者调换两支——
-- 而且不会报错，只是悄悄慢回去。改动那一句时用 `EXPLAIN QUERY PLAN` 看一眼：
-- 认上了是 `SCAN variant USING INDEX variant_group`，认不上是 `USE TEMP B-TREE FOR GROUP BY`。
CREATE INDEX IF NOT EXISTS variant_group
    ON variant(work_id, CASE WHEN work_id IS NULL THEN key END);

-- 变体的成员：一个条目只属于一个变体，因此键就是主键。
-- `role` 是**主文件 / 附属文件 / 内部资源 / 附属内容**之一。
-- 目录树成型出来的变体，它的主文件成员是**那个目录本身**。
CREATE TABLE IF NOT EXISTS variant_member(
    key         TEXT PRIMARY KEY,
    variant_key TEXT NOT NULL REFERENCES variant(key),
    role        TEXT NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS variant_member_variant ON variant_member(variant_key);

-- **合集**：用户自定义的一组游戏，与平台正交。
-- 它自成一张表，成员关系走 collection_variant——**平台是 variant 上的一列，
-- 两者不共用任何表、任何字段**（ADR-0011）。
CREATE TABLE IF NOT EXISTS collection(
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
) STRICT;

CREATE TABLE IF NOT EXISTS collection_variant(
    collection_id INTEGER NOT NULL REFERENCES collection(id),
    variant_key   TEXT NOT NULL,
    PRIMARY KEY (collection_id, variant_key)
) STRICT;

-- **人工纠正**：这个条目其实属于那个变体。
-- 规则会出错，所以人工纠正成型的结果是一等公民功能（CONTEXT 的「成型规则」词条）。
-- 它**不随重新成型消失**，也不随重扫消失。
CREATE TABLE IF NOT EXISTS shaping_override(
    key         TEXT PRIMARY KEY,
    variant_key TEXT NOT NULL
) STRICT;
";

/// 一行**变体**要读哪几列，以及它们的次序。
///
/// **只有这一处写列名。** 按主键取一条（[`Catalog::variant`]）、翻页取一段
/// （`catalog::browse`）、取同作品同平台的那几个（`catalog::detail`）三处走同一份，
/// 于是往 `variant` 表上加一列时改一处就够——分开写的话，漏改的那一处会静默地
/// 读出一个字段全是默认值的 [`VariantRow`]。
pub(super) const VARIANT_COLUMNS: &str = "key, platform, rule, main_key, files, bytes, unreadable,
                                          manual, work_id, release_id";

/// 按 [`VARIANT_COLUMNS`] 的次序读一行变体。
pub(super) fn read_variant_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<VariantRow> {
    Ok(VariantRow {
        key: row.get(0)?,
        platform: row.get(1)?,
        rule: row.get(2)?,
        main_key: row.get(3)?,
        files: u64::try_from(row.get::<_, i64>(4)?).unwrap_or(0),
        bytes: u64::try_from(row.get::<_, i64>(5)?).unwrap_or(0),
        unreadable_files: u64::try_from(row.get::<_, i64>(6)?).unwrap_or(0),
        manual: row.get::<_, i64>(7)? != 0,
        work_id: row.get(8)?,
        release_id: row.get(9)?,
    })
}

/// `meta` 里记「成型跑到哪一次遍历为止」的那把键。
const META_SHAPED_SCAN: &str = "shaped_scan";

/// `meta` 里记「成型用的是哪一份平台清单」的那把键。
const META_SHAPED_MANIFEST: &str = "shaped_manifest";

/// 一条**发行版**记录读回来的样子。
///
/// `region` 与 `languages` 分开读出来不是冗余：**标题集合**靠它们分辨三件事——
/// 名字是官方英文名还是日文原名的罗马字转写（地区是不是 Japan）、这条发行版是不是
/// 官中那一条（地区是不是中国 / 台湾 / 香港）、以及它是不是只是**顺带带着中文**
/// （语言里有 `Zh` 而地区不是中文地区）。最后两件正是 ADR-0019 那道世代裂缝的两侧。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseRow {
    /// 行号。**只在一趟之内有效**——重跑识别会把它换掉。
    pub id: i64,
    /// 属于哪个作品。
    pub work_id: i64,
    /// 平台；可空。
    pub platform: Option<String>,
    /// 地区；DAT 的名字里认不出来时是 `None`。
    pub region: Option<String>,
    /// 序列号；可空。
    pub serial: Option<String>,
    /// 语言标记组（`En,Zh-Hans`）；可空。
    pub languages: Option<String>,
}

impl ReleaseRow {
    /// 这条发行版标着的语言，拆成一个个语言码。
    ///
    /// **这一处拆，别处不拆。** 库里存的是逗号分隔的一串（`En,Fr,De`），而读它的地方
    /// 有三个：选择集求事实、筛选面板列可选值、详情面板摆给人看。三处各写一遍
    /// `split(',').trim().filter(非空)` 的话，哪天格式变了（分号？空格？）就得记得改三处。
    #[must_use]
    pub fn language_codes(languages: &str) -> Vec<String> {
        languages
            .split(',')
            .map(str::trim)
            .filter(|code| !code.is_empty())
            .map(str::to_string)
            .collect()
    }
}

/// 一条变体记录读回来的样子。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantRow {
    /// 变体的键。
    pub key: String,
    /// 平台；可空。
    pub platform: Option<String>,
    /// 哪条成型规则成的型。
    pub rule: String,
    /// 主文件。
    pub main_key: String,
    /// 文件成员数。
    pub files: u64,
    /// 文件成员的字节合计。**这是个下界**（见 `unreadable_files`）。
    pub bytes: u64,
    /// 成员里有几个元数据读不到，因此 `bytes` 少算了它们（ADR-0021）。
    pub unreadable_files: u64,
    /// 是不是人工纠正出来的。
    pub manual: bool,
    /// 属于哪个作品；票 07 之前一律是 `None`。
    pub work_id: Option<i64>,
    /// 基于哪个发行版；**`None` 本身是信息**（同人移植与 homebrew 没有官方发行版）。
    pub release_id: Option<i64>,
}

/// 一个变体的一个成员，连它在主库里的形态与大小。
///
/// 它是 [`Catalog::variant_files`] 的返回行，也是**同步计划器**折期望状态的原料
/// （[`sync::desired`](crate::sync::desired)）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberFile {
    /// 属于哪个变体。
    pub variant_key: String,
    /// 成员自己的键。
    pub key: String,
    /// 是不是普通文件。目录、符号链接不是——它们不是要搬的东西。
    pub is_file: bool,
    /// 字节数；**`None` 是元数据读不到**而不是 0 字节（ADR-0021）。
    pub len: Option<u64>,
    /// 修改时间（UNIX 纪元起的纳秒）；取不到时是 `None`。
    ///
    /// 同步靠它判「主库里这份变了没有」：只比大小的话，**原地改过、大小没变**的
    /// 文件会被静默判成不用重传——与增量扫描比的是同一个三元组。
    pub mtime_ns: Option<i64>,
    /// 是这个变体的**主文件**吗。
    ///
    /// 票 21 起要它：**能力档案判的是主文件**，因为主文件才是「用来交给模拟器启动
    /// 的那一个」（`CONTEXT.md`）。拿它去判每一个成员的话，一个 PSV 目录树变体底下
    /// 几千个**内部资源**会各自领一条「吃不下」，报告当场变成噪音。
    pub is_main: bool,
}

impl Catalog {
    /// 供**成型**读的条目表：键、是不是目录、大小。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn shape_entries(&self) -> Result<Vec<crate::shape::Entry>, CatalogError> {
        let mut statement = self
            .conn
            .prepare("SELECT key, kind, len FROM entry WHERE key <> '' ORDER BY key")
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let kind: i64 = row.get(1).map_err(|source| self.err(source))?;
            // 符号链接与「其他项」不参与成型：跟随链接会让同一份内容被算两次，
            // 而设备、管道之类根本不是内容。
            if kind != super::KIND_FILE && kind != super::KIND_DIR {
                continue;
            }
            // `len` 是 `NULL` 就是**元数据读不到**（ADR-0021），不是 0 字节——
            // 库里另有 4,317 个真正的空文件，塌成一个数两边都会说谎。
            let len: Option<i64> = row.get(2).map_err(|source| self.err(source))?;
            out.push(crate::shape::Entry {
                key: row.get(0).map_err(|source| self.err(source))?,
                is_dir: kind == super::KIND_DIR,
                len: len.and_then(|len| u64::try_from(len).ok()),
            });
        }
        Ok(out)
    }

    /// 全部人工纠正：条目的键 → 它该归到哪个变体。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn shaping_overrides(&self) -> Result<BTreeMap<String, String>, CatalogError> {
        let mut statement = self
            .conn
            .prepare("SELECT key, variant_key FROM shaping_override")
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        let mut out = BTreeMap::new();
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            out.insert(
                row.get(0).map_err(|source| self.err(source))?,
                row.get(1).map_err(|source| self.err(source))?,
            );
        }
        Ok(out)
    }

    /// 记一条人工纠正：`key` 其实属于 `variant_key` 这个变体。
    ///
    /// `key == variant_key` 就是「这一条自己当主文件」。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn set_shaping_override(
        &mut self,
        key: &str,
        variant_key: &str,
    ) -> Result<(), CatalogError> {
        self.conn
            .execute(
                "INSERT INTO shaping_override(key, variant_key) VALUES(?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET variant_key = excluded.variant_key",
                params![key, variant_key],
            )
            .map(|_| ())
            .map_err(|source| self.err(source))
    }

    /// 撤掉一条人工纠正，返回原来有没有这一条。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn clear_shaping_override(&mut self, key: &str) -> Result<bool, CatalogError> {
        self.conn
            .execute("DELETE FROM shaping_override WHERE key = ?1", params![key])
            .map(|removed| removed > 0)
            .map_err(|source| self.err(source))
    }

    /// 整批换掉变体，**但保住 `work_id` / `release_id`**。
    ///
    /// 那两列是裁决攒出来的（票 08 之后是**沉淀库**的一部分），改一条成型规则不该把它们
    /// 冲掉。合集的关系表根本不动——变体暂时消失再回来，它还在原来的合集里。
    ///
    /// **换完顺手收一遍孤儿**：识别的结论与候选挂在**变体**上，而变体只在这里换——
    /// 不在这一步收，删掉一个文件重扫之后，那个变体的结论、候选与它独家撑着的作品、
    /// 发行版会一直留在库里交给报告与导出，直到下一趟识别才被清掉
    /// （[`drop_variant_orphans`](super::identify::drop_variant_orphans) 上写着判据与
    /// 那份清单为什么是这几张表）。
    ///
    /// `scan` 是这次成型对着的遍历代号，记进 `meta`，报告据此说得出「成型是不是比库旧」。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn replace_variants(
        &mut self,
        variants: &[Variant],
        scan: i64,
        manifest: &Manifest,
    ) -> Result<(), CatalogError> {
        let path = self.path.clone();
        let to_err = |source| CatalogError::Sqlite {
            path: path.clone(),
            source,
        };
        // 先把已有的链接读出来，换完再贴回去。
        let mut links: BTreeMap<String, (Option<i64>, Option<i64>)> = BTreeMap::new();
        {
            let mut statement = self
                .conn
                .prepare(
                    "SELECT key, work_id, release_id FROM variant
                     WHERE work_id IS NOT NULL OR release_id IS NOT NULL",
                )
                .map_err(to_err)?;
            let mut rows = statement.query([]).map_err(to_err)?;
            while let Some(row) = rows.next().map_err(to_err)? {
                links.insert(
                    row.get(0).map_err(to_err)?,
                    (row.get(1).map_err(to_err)?, row.get(2).map_err(to_err)?),
                );
            }
        }

        let tx = self.conn.transaction().map_err(to_err)?;
        tx.execute("DELETE FROM variant_member", [])
            .and_then(|_| tx.execute("DELETE FROM variant", []))
            .map_err(to_err)?;
        {
            let mut insert_variant = tx
                .prepare(
                    "INSERT INTO variant(key, platform, rule, main_key, files, bytes,
                         unreadable, manual, work_id, release_id)
                     VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                )
                .map_err(to_err)?;
            let mut insert_member = tx
                .prepare("INSERT INTO variant_member(key, variant_key, role) VALUES(?1, ?2, ?3)")
                .map_err(to_err)?;
            for variant in variants {
                let (work, release) = links.get(&variant.key).copied().unwrap_or((None, None));
                insert_variant
                    .execute(params![
                        variant.key,
                        variant.platform,
                        variant.rule,
                        variant.main_key,
                        i64::try_from(variant.files).unwrap_or(i64::MAX),
                        i64::try_from(variant.bytes).unwrap_or(i64::MAX),
                        i64::try_from(variant.unreadable_files).unwrap_or(i64::MAX),
                        i64::from(variant.manual),
                        work,
                        release,
                    ])
                    .map_err(to_err)?;
                for (key, role) in &variant.members {
                    insert_member
                        .execute(params![key, variant.key, role.code()])
                        .map_err(to_err)?;
                }
            }
        }
        // 换完了才收孤儿：这时 `variant` 里正好是新的那一批，「指不着任何变体」才问得准。
        // 在**同一个事务里**跑，是因为「变体没了」与「它的结论也没了」必须一起落盘——
        // 分两次提交的话，中间被打断就留下一份变体已经换掉、结论还指着旧键的库，
        // 而那正是这一趟要治的病。
        super::identify::drop_variant_orphans(&tx).map_err(to_err)?;
        tx.commit().map_err(to_err)?;
        self.meta_set(META_SHAPED_SCAN, &scan.to_string())?;
        self.meta_set(META_SHAPED_MANIFEST, &manifest.fingerprint().to_string())
    }

    /// 成型跑到哪一次遍历为止；从没成型过时是 `None`。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn shaped_scan(&self) -> Result<Option<i64>, CatalogError> {
        Ok(self
            .meta_get(META_SHAPED_SCAN)?
            .and_then(|text| text.parse().ok()))
    }

    /// 成型用的是哪一份平台清单（指纹）；从没成型过时是 `None`。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn shaped_manifest(&self) -> Result<Option<u64>, CatalogError> {
        Ok(self
            .meta_get(META_SHAPED_MANIFEST)?
            .and_then(|text| text.parse().ok()))
    }

    /// 一条变体记录。
    ///
    /// **走 `prepare_cached`**：这一条会被逐个变体地问上几万遍（`collection::add` 把
    /// 全选那一批展开之后一个一个问，`scrape::zh::Rulings::resolve` 也是），
    /// 每次重新解析一遍 SQL 就是白花几万次。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn variant(&self, key: &str) -> Result<Option<VariantRow>, CatalogError> {
        self.conn
            .prepare_cached(&format!("SELECT {VARIANT_COLUMNS} FROM variant WHERE key = ?1"))
            .and_then(|mut statement| {
                statement
                    .query_row(params![key], read_variant_row)
                    .optional()
            })
            .map_err(|source| self.err(source))
    }

    /// 一个变体有哪些成员，按键排序。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn variant_members(&self, key: &str) -> Result<Vec<(String, Role)>, CatalogError> {
        let mut statement = self
            .conn
            .prepare_cached(
                "SELECT key, role FROM variant_member WHERE variant_key = ?1 ORDER BY key",
            )
            .map_err(|source| self.err(source))?;
        let mut rows = statement
            .query(params![key])
            .map_err(|source| self.err(source))?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let code: String = row.get(1).map_err(|source| self.err(source))?;
            out.push((
                row.get(0).map_err(|source| self.err(source))?,
                Role::from_code(&code).unwrap_or(Role::Internal),
            ));
        }
        Ok(out)
    }

    /// 一批变体的**文件成员**，连它们在主库里的大小。**同步计划器的原料**。
    ///
    /// 一趟顺读整张成员表、在内存里筛，而不是逐个变体查一次，也不是把几千个键拼成
    /// 一条 `IN`：真库上 216,203 条成员对 46,444 个变体，一条规则选中上千个变体是
    /// 常态，逐个查就是上千次往返，而拼 `IN` 会撞上 SQLite 的绑定参数上限。
    ///
    /// **非文件成员照样返回**（`is_file` 为假），由调用方决定怎么处置——目录树变体的
    /// 那个目录本身是它的主文件，在这里悄悄扔掉的话，「一个成员都不该凭空消失」
    /// 这件事就没人数得出来了。
    ///
    /// **搬的时候四种身份一视同仁**（挂账 D80）：同步要搬的是变体的全部文件成员。
    /// 但 [`MemberFile::is_main`] 仍然读出来——**格式转换判的是主文件**（票 21），
    /// 那是「交给模拟器启动的那一个」，与「要不要搬」是两个问题。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn variant_files(
        &self,
        variants: &std::collections::BTreeSet<String>,
    ) -> Result<Vec<MemberFile>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT m.variant_key, m.key, e.kind, e.readable, e.len, e.mtime_ns, m.role
                 FROM variant_member m JOIN entry e ON e.key = m.key
                 ORDER BY m.variant_key, m.key",
            )
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let variant_key: String = row.get(0).map_err(|source| self.err(source))?;
            if !variants.contains(&variant_key) {
                continue;
            }
            let kind: i64 = row.get(2).map_err(|source| self.err(source))?;
            let readable: i64 = row.get(3).map_err(|source| self.err(source))?;
            let len: Option<i64> = row.get(4).map_err(|source| self.err(source))?;
            out.push(MemberFile {
                variant_key,
                key: row.get(1).map_err(|source| self.err(source))?,
                is_file: kind == super::KIND_FILE,
                // **读不到就是 `None`，不是 0**（ADR-0021）：库里另有 4,317 个
                // 真正的空文件，混在一起两个数都会说谎。
                len: if readable == 0 {
                    None
                } else {
                    len.and_then(|len| u64::try_from(len).ok())
                },
                mtime_ns: if readable == 0 {
                    None
                } else {
                    row.get(5).map_err(|source| self.err(source))?
                },
                is_main: row.get::<_, String>(6).map_err(|source| self.err(source))?
                    == Role::Main.code(),
            });
        }
        Ok(out)
    }

    /// 一个条目属于哪个变体；不属于任何变体时是 `None`。
    ///
    /// **内部资源问的就是这个**：`at9` / `psarc` 各自不成变体，但它们属于哪个变体要答得上来。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn variant_of(&self, key: &str) -> Result<Option<(String, Role)>, CatalogError> {
        self.conn
            .query_row(
                "SELECT variant_key, role FROM variant_member WHERE key = ?1",
                params![key],
                |row| {
                    let code: String = row.get(1)?;
                    Ok((
                        row.get::<_, String>(0)?,
                        Role::from_code(&code).unwrap_or(Role::Internal),
                    ))
                },
            )
            .optional()
            .map_err(|source| self.err(source))
    }

    /// 从变体表折出成型的统计。**不碰磁盘**。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn shaping(&self, manifest: &Manifest) -> Result<ShapingAcc, CatalogError> {
        let mut acc = ShapingAcc {
            shaped_scan: self.shaped_scan()?,
            // 用清单 A 成型、用清单 B 出报告，平台那张表按 B 分组而变体数按 A 算——
            // 两边对不上，而报告不说的话谁也看不出来。
            manifest_changed: self
                .shaped_manifest()?
                .is_some_and(|shaped| shaped != manifest.fingerprint()),
            ..ShapingAcc::default()
        };
        let mut statement = self
            .conn
            .prepare("SELECT platform, rule, files, bytes, manual, unreadable FROM variant")
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let platform: Option<String> = row.get(0).map_err(|source| self.err(source))?;
            let rule: String = row.get(1).map_err(|source| self.err(source))?;
            let files = u64::try_from(row.get::<_, i64>(2).map_err(|source| self.err(source))?)
                .unwrap_or(0);
            let bytes = u64::try_from(row.get::<_, i64>(3).map_err(|source| self.err(source))?)
                .unwrap_or(0);
            let manual: i64 = row.get(4).map_err(|source| self.err(source))?;
            let unreadable =
                u64::try_from(row.get::<_, i64>(5).map_err(|source| self.err(source))?)
                    .unwrap_or(0);
            acc.variants += 1;
            acc.files += files;
            acc.bytes += bytes;
            acc.unreadable_files += unreadable;
            acc.manual += u64::from(manual != 0);
            *acc.by_rule.entry(rule).or_default() += 1;
            if let Some(platform) = platform {
                let counts = acc.by_platform.entry(platform).or_default();
                counts.variants += 1;
                counts.files += files;
                counts.bytes += bytes;
            }
        }

        let mut statement = self
            .conn
            .prepare("SELECT role, COUNT(*) FROM variant_member GROUP BY role")
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let code: String = row.get(0).map_err(|source| self.err(source))?;
            let count = u64::try_from(row.get::<_, i64>(1).map_err(|source| self.err(source))?)
                .unwrap_or(0);
            if let Some(role) = Role::from_code(&code) {
                *acc.by_role.entry(role).or_default() += count;
            }
        }
        Ok(acc)
    }

    // ── 三层：作品与发行版 ──────────────────────────────────────────────────

    /// 记一个**作品**，返回它的 id。
    ///
    /// `origin` 说这一行是谁造的。识别重跑时只清自己造的那些（见
    /// [`Catalog::clear_identifications`](super::Catalog::clear_identifications)）。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn add_work(&mut self, name: &str, origin: Provenance) -> Result<i64, CatalogError> {
        self.conn
            .execute(
                "INSERT INTO work(name, origin) VALUES(?1, ?2)",
                params![name, origin.label()],
            )
            .map_err(|source| self.err(source))?;
        Ok(self.conn.last_insert_rowid())
    }

    /// 改一行**作品**的来路。
    ///
    /// **识别撞出来的与裁决定下来的共用同一张作品表**（一部作品两行会让导出时的收敛
    /// 把它拆成两个条目）。于是同一个名字先被识别建出来、后被裁决点名时，这一行要改口
    /// 说自己是裁决的——报告里「识别建出来的作品数」才数得对。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn set_work_origin(&mut self, id: i64, origin: Provenance) -> Result<(), CatalogError> {
        self.conn
            .execute(
                "UPDATE work SET origin = ?2 WHERE id = ?1",
                params![id, origin.label()],
            )
            .map(|_| ())
            .map_err(|source| self.err(source))
    }

    /// 记一个**发行版**，返回它的 id。
    ///
    /// `languages` 装的是 ADR-0019 那道世代裂缝的数字世代一侧：中文在那里是同一条
    /// 发行版的语言属性，不另成发行版。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn add_release(
        &mut self,
        work_id: i64,
        platform: Option<&str>,
        region: Option<&str>,
        serial: Option<&str>,
        languages: Option<&str>,
        origin: Provenance,
    ) -> Result<i64, CatalogError> {
        self.conn
            .execute(
                "INSERT INTO release(work_id, platform, region, serial, languages, origin)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                params![work_id, platform, region, serial, languages, origin.label()],
            )
            .map_err(|source| self.err(source))?;
        Ok(self.conn.last_insert_rowid())
    }

    /// 找一条形状一模一样的**发行版**；没有就是 `None`。
    ///
    /// **裁决**拿它跨调用去重：`romcat triage decide` 一次一批，两批之间内存里那张
    /// 去重表是空的，同一次发行被第二批裁决点到时不查库就会多建一行。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn release_like(
        &self,
        work_id: i64,
        platform: Option<&str>,
        region: Option<&str>,
        serial: Option<&str>,
        languages: Option<&str>,
    ) -> Result<Option<i64>, CatalogError> {
        self.conn
            .query_row(
                "SELECT id FROM release
                 WHERE work_id = ?1
                   AND COALESCE(platform, '')  = COALESCE(?2, '')
                   AND COALESCE(region, '')    = COALESCE(?3, '')
                   AND COALESCE(serial, '')    = COALESCE(?4, '')
                   AND COALESCE(languages, '') = COALESCE(?5, '')
                 ORDER BY id LIMIT 1",
                params![work_id, platform, region, serial, languages],
                |row| row.get(0),
            )
            .optional()
            .map_err(|source| self.err(source))
    }

    /// 把一个变体挂到作品与发行版上。
    ///
    /// 两个都可以是 `None`，而且**空是有含义的**：同人移植与 homebrew 没有官方发行版，
    /// 没有发行版链接这件事本身就告诉识别管线「别拿它去撞 DAT」。
    ///
    /// # Errors
    /// 写库失败、或那个变体不存在时返回错误。
    pub fn link_variant(
        &mut self,
        variant_key: &str,
        work_id: Option<i64>,
        release_id: Option<i64>,
    ) -> Result<(), CatalogError> {
        self.conn
            .execute(
                "UPDATE variant SET work_id = ?2, release_id = ?3 WHERE key = ?1",
                params![variant_key, work_id, release_id],
            )
            .map(|_| ())
            .map_err(|source| self.err(source))
    }

    /// 全部**作品**：id → 名字。
    ///
    /// 刮削拿它做**锚点**：作品锚点是**作品名**而不是行号，因为重跑识别会把行整批
    /// 换掉（`catalog::scrape` 的模块文档说的就是这件事）。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn work_names(&self) -> Result<BTreeMap<i64, String>, CatalogError> {
        let mut statement = self
            .conn
            .prepare("SELECT id, name FROM work")
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|source| self.err(source))?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (id, name) = row.map_err(|source| self.err(source))?;
            out.insert(id, name);
        }
        Ok(out)
    }

    /// **这个变体属于哪个作品**：作品锚点上那个名字；识别还没认出来时是 `None`。
    ///
    /// 单开一条而不是让调用方读一遍 [`work_names`](Self::work_names)：只想问一个变体的
    /// 时候，那是把 9,226 行整份读进内存去取其中一行。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn work_of_variant(&self, key: &str) -> Result<Option<String>, CatalogError> {
        self.conn
            .prepare_cached(
                "SELECT work.name FROM variant JOIN work ON work.id = variant.work_id
                 WHERE variant.key = ?1",
            )
            .and_then(|mut statement| {
                statement
                    .query_row(params![key], |row| row.get::<_, String>(0))
                    .optional()
            })
            .map_err(|source| self.err(source))
    }

    /// 全部**发行版**：id → 那一行。
    ///
    /// 一次读完而不是逐条查：**标题集合**要为每个变体问一次「它基于的那条发行版是什么
    /// 地区、什么语言」，真库上那是 46,444 次查询，而发行版本身只有 18,571 行。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn releases(&self) -> Result<BTreeMap<i64, ReleaseRow>, CatalogError> {
        let mut statement = self
            .conn
            .prepare("SELECT id, work_id, platform, region, serial, languages FROM release")
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                Ok(ReleaseRow {
                    id: row.get(0)?,
                    work_id: row.get(1)?,
                    platform: row.get(2)?,
                    region: row.get(3)?,
                    serial: row.get(4)?,
                    languages: row.get(5)?,
                })
            })
            .map_err(|source| self.err(source))?;
        let mut out = BTreeMap::new();
        for row in rows {
            let row = row.map_err(|source| self.err(source))?;
            out.insert(row.id, row);
        }
        Ok(out)
    }

    /// 一个作品下面有哪几条发行版。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn releases_of(&self, work_id: i64) -> Result<Vec<i64>, CatalogError> {
        let mut statement = self
            .conn
            .prepare("SELECT id FROM release WHERE work_id = ?1 ORDER BY id")
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![work_id], |row| row.get(0))
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    // ── 合集：与平台正交 ────────────────────────────────────────────────────

    /// 建一个**合集**（已存在就返回原来那个的 id）。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn add_collection(&mut self, name: &str) -> Result<i64, CatalogError> {
        self.conn
            .execute(
                "INSERT INTO collection(name) VALUES(?1) ON CONFLICT(name) DO NOTHING",
                params![name],
            )
            .map_err(|source| self.err(source))?;
        self.conn
            .query_row(
                "SELECT id FROM collection WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .map_err(|source| self.err(source))
    }

    /// 把一个变体放进一个合集。
    ///
    /// **一批一起放走 [`Self::add_all_to_collection`]**：那一条是一个事务，这一条是一个。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn add_to_collection(
        &mut self,
        collection_id: i64,
        variant_key: &str,
    ) -> Result<(), CatalogError> {
        self.conn
            .execute(
                "INSERT INTO collection_variant(collection_id, variant_key) VALUES(?1, ?2)
                 ON CONFLICT DO NOTHING",
                params![collection_id, variant_key],
            )
            .map(|_| ())
            .map_err(|source| self.err(source))
    }

    /// 把一批变体放进一个合集，**一个事务**。
    ///
    /// 界面上「全选 → 收藏」一下就是四万多条（`collection::add`），一条一个事务等于
    /// 四万多次提交。逐条那一版留着给只放一个的场合。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn add_all_to_collection(
        &mut self,
        collection_id: i64,
        variant_keys: &[String],
    ) -> Result<(), CatalogError> {
        let to_err = |source| CatalogError::Sqlite {
            path: self.path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        {
            let mut insert = tx
                .prepare(
                    "INSERT INTO collection_variant(collection_id, variant_key) VALUES(?1, ?2)
                     ON CONFLICT DO NOTHING",
                )
                .map_err(to_err)?;
            for key in variant_keys {
                insert.execute(params![collection_id, key]).map_err(to_err)?;
            }
        }
        tx.commit().map_err(to_err)
    }

    /// 把一个变体从一个合集里拿出来（按**合集名**找）。返回真的拿出来了没有。
    ///
    /// 按名字而不是按 id：调用方手里是**沉淀库**那条成员关系，它记的是名字
    /// （`collection::apply`）。让调用方先查一次 id 只是把同一次查询挪个地方。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn remove_from_collection(
        &mut self,
        name: &str,
        variant_key: &str,
    ) -> Result<bool, CatalogError> {
        self.conn
            .execute(
                "DELETE FROM collection_variant
                 WHERE variant_key = ?2
                   AND collection_id IN (SELECT id FROM collection WHERE name = ?1)",
                params![name, variant_key],
            )
            .map(|changed| changed > 0)
            .map_err(|source| self.err(source))
    }

    /// 把一批变体从一个合集里拿出来，**一个事务**。理由同 [`Self::add_all_to_collection`]。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn remove_all_from_collection(
        &mut self,
        name: &str,
        variant_keys: &[String],
    ) -> Result<(), CatalogError> {
        let to_err = |source| CatalogError::Sqlite {
            path: self.path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        {
            let mut delete = tx
                .prepare(
                    "DELETE FROM collection_variant
                     WHERE variant_key = ?2
                       AND collection_id IN (SELECT id FROM collection WHERE name = ?1)",
                )
                .map_err(to_err)?;
            for key in variant_keys {
                delete.execute(params![name, key]).map_err(to_err)?;
            }
        }
        tx.commit().map_err(to_err)
    }

    /// 把一条成员都不剩的合集从这份投影里去掉。返回去掉了几个。
    ///
    /// **屏上「合集 N 个」里不该有一个一件东西都选不出来的**：那是这一票要消灭的东西
    /// （挂账 D74 说的「合集 0 个」是它的极端情形）。合集本身住在沉淀库里，
    /// 这里去掉的只是投影上那一行。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn drop_empty_collections(&mut self) -> Result<usize, CatalogError> {
        self.conn
            .execute(
                "DELETE FROM collection
                 WHERE id NOT IN (SELECT collection_id FROM collection_variant)",
                [],
            )
            .map_err(|source| self.err(source))
    }

    /// 把这份投影**整份换成**给的这几组，**一个事务**（`collection::project`）。
    ///
    /// 换的是投影不是合集本身——合集住在沉淀库里，那份不可再生，一条都不许删。
    ///
    /// **先清后写必须在一个事务里。** 中途出错或者进程被杀的话，投影会停在空的或者
    /// 半份的样子：屏上、筛选栏那一维、子库的选择集上所有的星一起消失，而人看不出
    /// 那是「沉淀库里没了」还是「重建跑到一半」——那正是这一票要消灭的东西。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn replace_collections(
        &mut self,
        groups: &[(&str, Vec<String>)],
    ) -> Result<(), CatalogError> {
        let to_err = |source| CatalogError::Sqlite {
            path: self.path.clone(),
            source,
        };
        let tx = self.conn.transaction().map_err(to_err)?;
        tx.execute_batch(
            "DELETE FROM collection_variant;
             DELETE FROM collection;",
        )
        .map_err(to_err)?;
        {
            let mut insert_name = tx
                .prepare("INSERT INTO collection(name) VALUES(?1)")
                .map_err(to_err)?;
            let mut insert_member = tx
                .prepare(
                    "INSERT INTO collection_variant(collection_id, variant_key) VALUES(?1, ?2)
                     ON CONFLICT DO NOTHING",
                )
                .map_err(to_err)?;
            for (name, keys) in groups {
                if keys.is_empty() {
                    continue;
                }
                insert_name.execute(params![name]).map_err(to_err)?;
                let id = tx.last_insert_rowid();
                for key in keys {
                    insert_member.execute(params![id, key]).map_err(to_err)?;
                }
            }
        }
        tx.commit().map_err(to_err)
    }

    /// 一个合集里现在有哪些变体。
    ///
    /// 只列**还在库里**的那些：关系表本身不清理，变体暂时消失（盘没插、目录改了名）
    /// 之后回来还在原来的合集里。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn collection_members(&self, collection_id: i64) -> Result<Vec<String>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT cv.variant_key FROM collection_variant cv
                 JOIN variant v ON v.key = cv.variant_key
                 WHERE cv.collection_id = ?1 ORDER BY cv.variant_key",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![collection_id], |row| row.get(0))
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 一个变体属于哪些合集，按名字排序。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn collections_of(&self, variant_key: &str) -> Result<Vec<String>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT c.name FROM collection c
                 JOIN collection_variant cv ON cv.collection_id = c.id
                 WHERE cv.variant_key = ?1 ORDER BY c.name",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map(params![variant_key], |row| row.get(0))
            .map_err(|source| self.err(source))?;
        rows.collect::<Result<_, _>>()
            .map_err(|source| self.err(source))
    }

    /// 全库的合集成员关系：变体的键 → 它在哪几个合集里。
    ///
    /// 与逐个变体问一遍 [`Self::collections_of`] 的差别不是风格问题：**子库的选择集**
    /// 要在 46,444 个变体上求值，逐个查等于把这条连接跑四万多遍。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn collection_memberships(&self) -> Result<BTreeMap<String, Vec<String>>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT cv.variant_key, c.name FROM collection c
                 JOIN collection_variant cv ON cv.collection_id = c.id
                 ORDER BY cv.variant_key, c.name",
            )
            .map_err(|source| self.err(source))?;
        let mut rows = statement.query([]).map_err(|source| self.err(source))?;
        let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
        while let Some(row) = rows.next().map_err(|source| self.err(source))? {
            let key: String = row.get(0).map_err(|source| self.err(source))?;
            let name: String = row.get(1).map_err(|source| self.err(source))?;
            out.entry(key).or_default().push(name);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::{MANUAL_RULE, SINGLE_FILE_RULE};

    fn 库() -> Catalog {
        Catalog::open_in_memory().expect("能开中立库")
    }

    fn 变体(key: &str, platform: Option<&str>) -> Variant {
        Variant {
            key: key.to_string(),
            platform: platform.map(ToString::to_string),
            rule: SINGLE_FILE_RULE.to_string(),
            main_key: key.to_string(),
            manual: false,
            files: 1,
            bytes: 100,
            unreadable_files: 0,
            members: vec![(key.to_string(), Role::Main)],
        }
    }

    #[test]
    fn 三层挂得起来且没有发行版本身是信息() {
        let mut catalog = 库();
        catalog
            .replace_variants(
                &[
                    变体("FC/甲.zip", Some("FC")),
                    变体("PS1/乙.chd", Some("PS1")),
                ],
                1,
                &Manifest::builtin(),
            )
            .expect("写得进");

        let work = catalog
            .add_work("幻想传说", Provenance::Verdict)
            .expect("建得了作品");
        let release = catalog
            .add_release(
                work,
                Some("SFC"),
                Some("日本"),
                Some("SHVC-TO"),
                Some("ja"),
                Provenance::Verdict,
            )
            .expect("建得了发行版");
        catalog
            .link_variant("FC/甲.zip", Some(work), Some(release))
            .expect("挂得上");
        // 同人移植：挂在作品下，**没有发行版**——空本身就在告诉识别管线别去撞 DAT。
        catalog
            .link_variant("PS1/乙.chd", Some(work), None)
            .expect("挂得上");

        let 甲 = catalog.variant("FC/甲.zip").expect("读得出").expect("在");
        assert_eq!(甲.work_id, Some(work));
        assert_eq!(甲.release_id, Some(release));
        let 乙 = catalog.variant("PS1/乙.chd").expect("读得出").expect("在");
        assert_eq!(乙.work_id, Some(work));
        assert_eq!(乙.release_id, None);
        assert_eq!(catalog.releases_of(work).expect("读得出"), vec![release]);
    }

    #[test]
    fn 平台与合集是两回事互不干涉() {
        // 用户当前的库里「目录 = 平台 = 前端的 collection」三者重合，但捏成一个之后
        // 「我通关过的」这类跨平台合集就永远表达不了了（ADR-0011）。
        let mut catalog = 库();
        catalog
            .replace_variants(
                &[
                    变体("FC/甲.zip", Some("FC")),
                    变体("PS1/乙.chd", Some("PS1")),
                    变体("FC/丙.zip", Some("FC")),
                ],
                1,
                &Manifest::builtin(),
            )
            .expect("写得进");
        let 通关过 = catalog.add_collection("我通关过的").expect("建得了合集");
        let 双人 = catalog.add_collection("适合双人玩的").expect("建得了合集");
        catalog
            .add_to_collection(通关过, "FC/甲.zip")
            .expect("加得进");
        catalog
            .add_to_collection(通关过, "PS1/乙.chd")
            .expect("加得进");
        catalog
            .add_to_collection(双人, "FC/甲.zip")
            .expect("加得进");

        assert_eq!(
            catalog.collection_members(通关过).expect("读得出"),
            vec!["FC/甲.zip".to_string(), "PS1/乙.chd".to_string()],
            "一个合集横跨两个平台"
        );
        assert_eq!(
            catalog.collections_of("FC/甲.zip").expect("读得出"),
            vec!["我通关过的".to_string(), "适合双人玩的".to_string()],
            "一个变体同时在两个合集里"
        );
        assert!(
            catalog
                .collections_of("FC/丙.zip")
                .expect("读得出")
                .is_empty(),
            "同一个平台下的另一个变体不因平台相同就进合集"
        );
    }

    #[test]
    fn 重新成型不抹掉已有的作品与发行版链接() {
        let mut catalog = 库();
        catalog
            .replace_variants(&[变体("FC/甲.zip", Some("FC"))], 1, &Manifest::builtin())
            .expect("写得进");
        let work = catalog
            .add_work("超级马里奥", Provenance::Verdict)
            .expect("建得了作品");
        catalog
            .link_variant("FC/甲.zip", Some(work), None)
            .expect("挂得上");

        // 改了一条成型规则，重新成型一遍
        catalog
            .replace_variants(
                &[变体("FC/甲.zip", Some("FC")), 变体("FC/乙.zip", Some("FC"))],
                2,
                &Manifest::builtin(),
            )
            .expect("写得进");
        assert_eq!(
            catalog
                .variant("FC/甲.zip")
                .expect("读得出")
                .expect("在")
                .work_id,
            Some(work),
            "裁决攒出来的链接不该被一次重新成型冲掉"
        );
        assert_eq!(catalog.shaped_scan().expect("读得出"), Some(2));
    }

    #[test]
    fn 合集成员在变体暂时消失之后还认得回来() {
        let mut catalog = 库();
        catalog
            .replace_variants(&[变体("FC/甲.zip", Some("FC"))], 1, &Manifest::builtin())
            .expect("写得进");
        let 合集 = catalog.add_collection("我通关过的").expect("建得了合集");
        catalog
            .add_to_collection(合集, "FC/甲.zip")
            .expect("加得进");

        catalog
            .replace_variants(&[], 2, &Manifest::builtin())
            .expect("写得进");
        assert!(
            catalog.collection_members(合集).expect("读得出").is_empty(),
            "变体不在了就不列出来"
        );

        catalog
            .replace_variants(&[变体("FC/甲.zip", Some("FC"))], 3, &Manifest::builtin())
            .expect("写得进");
        assert_eq!(
            catalog.collection_members(合集).expect("读得出"),
            vec!["FC/甲.zip".to_string()],
            "变体回来了，它还在原来的合集里"
        );
    }

    #[test]
    fn 人工纠正存得住也撤得掉() {
        let mut catalog = 库();
        catalog
            .set_shaping_override("FC/乙.zip", "FC/甲.zip")
            .expect("记得下");
        assert_eq!(
            catalog
                .shaping_overrides()
                .expect("读得出")
                .get("FC/乙.zip"),
            Some(&"FC/甲.zip".to_string())
        );
        assert!(catalog.clear_shaping_override("FC/乙.zip").expect("撤得掉"));
        assert!(catalog.shaping_overrides().expect("读得出").is_empty());
        assert!(!catalog.clear_shaping_override("FC/乙.zip").expect("撤得掉"));
    }

    #[test]
    fn 成型统计从变体表折出来() {
        let mut catalog = 库();
        let mut 手工 = 变体("FC/甲.zip", Some("FC"));
        手工.manual = true;
        手工.rule = MANUAL_RULE.to_string();
        手工
            .members
            .push(("FC/乙.zip".to_string(), Role::Companion));
        手工.files = 2;
        catalog
            .replace_variants(
                &[手工, 变体("PS1/丙.chd", Some("PS1"))],
                7,
                &Manifest::builtin(),
            )
            .expect("写得进");

        let acc = catalog.shaping(&Manifest::builtin()).expect("折得出来");
        assert_eq!(acc.variants, 2);
        assert_eq!(acc.manual, 1);
        assert_eq!(acc.shaped_scan, Some(7));
        assert_eq!(acc.by_platform["FC"].variants, 1);
        assert_eq!(acc.by_platform["FC"].files, 2);
        assert_eq!(acc.by_role[&Role::Main], 2);
        assert_eq!(acc.by_role[&Role::Companion], 1);
    }

    #[test]
    fn 内部资源答得出自己属于哪个变体() {
        let mut catalog = 库();
        let variant = Variant {
            key: "PSV/某游戏".to_string(),
            platform: Some("PSV".to_string()),
            rule: "PSV 目录树".to_string(),
            main_key: "PSV/某游戏".to_string(),
            manual: false,
            files: 1,
            bytes: 4000,
            unreadable_files: 0,
            members: vec![
                ("PSV/某游戏".to_string(), Role::Main),
                ("PSV/某游戏/bgm/a.at9".to_string(), Role::Internal),
            ],
        };
        catalog
            .replace_variants(&[variant], 1, &Manifest::builtin())
            .expect("写得进");
        assert_eq!(
            catalog.variant_of("PSV/某游戏/bgm/a.at9").expect("读得出"),
            Some(("PSV/某游戏".to_string(), Role::Internal)),
            "at9 不成变体，但它属于哪个变体要答得上来"
        );
    }
}
