//! 一份开好的**中立库**加**沉淀库**：裁决这件事要的全部原料。
//!
//! 两份库都在**工作目录**里，都在本机（ADR-0009）——**主库一个字节都不读**。
//! 队列、候选、依据、裁决全部由这两份库折出来（ADR-0001），所以外置盘没挂上的时候
//! 照样列得出队列、裁得下去。
//!
//! ## 为什么这三样必须一起开
//!
//! 一条**路径锚**记的是「主库『某某』里的某某变体」，而那个「某某」就是**主库标识**——中立库的文件名
//! （[`Slug::text`]）。谁自己另起一个标识，谁裁出来的那批锚就与别处对不上——命令行裁的
//! 界面看不见，反过来也一样。把「开哪份中立库」「开哪份沉淀库」「这份主库的主库标识」捏成
//! 一个类型，就是不让第二份算法长出来。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::catalog::scrape::VerdictValue;
use crate::catalog::{Catalog, CatalogError, TitleRow};
use crate::path;
use crate::scrape::priority::VERDICT;
use crate::scrape::{AnchorKind, Field};
use crate::verdict::{Store, TakeBack, VerdictError};
use crate::workspace::{self, Slug};

/// 开不出这份现场的原因。
#[derive(Debug, thiserror::Error)]
pub enum SiteError {
    /// 中立库还不在。
    #[error("还没有 {0} 这份中立库。先跑一次 `romcat scan`。")]
    NoCatalog(String),
    /// 按这个名字折出来的中立库不在，而这个工作目录里另一份库的**主库原名**正是它——那份
    /// 多半改过名：改名只换主库原名，找库认的仍是建库时那个名字折出来的主库标识（挂单 `Q472`）。
    #[error(
        "还没有 {located_by} 这份中立库——这个工作目录里主库原名叫「{name}」的是 {other}。\
         找库认的是建库时的那个名字，改名不动它：要开的是那一份，就按它建库时的名字找"
    )]
    Renamed {
        /// 靠什么没找到（`--library <名字>` 那一串）。
        located_by: String,
        /// 人给的那个名字。
        name: String,
        /// 主库原名正是这个名字的那份中立库。
        other: String,
    },
    /// 中立库要落进主库里去了。**主库只读**（ADR-0004）。
    #[error("中立库 {0} 落在主库内。主库只读，请把工作目录放到别处。")]
    InsideLibrary(String),
    /// 那不像是一份中立库文件。
    #[error("{0} 不像是一份中立库文件。")]
    NotACatalog(String),
    /// 中立库打不开。
    #[error("中立库打不开：{0}")]
    Catalog(#[from] CatalogError),
    /// 沉淀库打不开。
    #[error("沉淀库打不开：{0}")]
    Verdict(#[from] VerdictError),
    /// 中立库结构版本对不上，而它里面还没搬进沉淀库的人定的东西（**人工纠正**、**首选变体**、
    /// **亲手加的叫法**、**详情页上改过的字段**）这一回没救出来（[`rescue`]）。那句「删掉它重扫」
    /// **这时不能照做**。
    #[error(
        "{said}——但这份库里还记着没搬进沉淀库的人工纠正、首选变体、亲手加的叫法或详情页上\
         改过的字段，这一回没搬成（{why}）。\
         删掉它之前先把沉淀库弄好，不然它们就没了"
    )]
    Stranded {
        /// 那句叫人删库重扫的原话（[`CatalogError::Version`]）。
        said: String,
        /// 没搬成的原因。
        why: String,
    },
}

/// 人在现场上定下一件事（首选变体、亲手加的叫法、字段修改），两份库有一份没写进去。
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// 沉淀库写不动。**这时中立库一个字都没动**：先落的是沉淀库。
    #[error("沉淀库写不动：{0}")]
    Store(#[from] VerdictError),
    /// 中立库读写不动。走到这一支时沉淀库那一笔**写了没有看是哪个入口**：首选变体、亲手加的
    /// 叫法、字段修改那几支先落沉淀库，这时沉淀库已经记下，投影没跟上——下次开现场照沉淀库重建
    /// （[`reconcile`]）；`merge::adopt` 读中立库挑标题那一步只碰中立库，走到这里就是什么都没记下。
    #[error("中立库读写不动：{0}")]
    Catalog(#[from] CatalogError),
}

impl From<crate::title::RefoldError> for WriteError {
    fn from(error: crate::title::RefoldError) -> Self {
        match error {
            crate::title::RefoldError::Catalog(error) => Self::Catalog(error),
            crate::title::RefoldError::Verdict(error) => Self::Store(error),
        }
    }
}

impl SiteError {
    /// 按 `slug` 折出来的中立库不在时该说哪一句——命令行各命令与 [`Site::open`] 说的是同一句。
    ///
    /// 给的是名字、而这个工作目录里另一份库的主库原名正是它时，说清是哪一份
    /// （[`SiteError::Renamed`]，判「是不是它」只在 [`workspace::namesake`]）；否则就是
    /// 「还没有，先跑一次 `romcat scan`」（[`SiteError::NoCatalog`]）。
    ///
    /// 中立库住的那个目录列不开时查不了这个名字是哪一份库的原名，就只说「还没有」：那一句
    /// 只是替人多指一步路，指不出来不改变「按这个名字折出来的那份不在」（挂单 `Q614`）。
    #[must_use]
    pub fn not_found(workspace: &Path, slug: Slug<'_>, located_by: &str) -> Self {
        if let Slug::Named(name) = slug
            && let Ok(Some(other)) = workspace::namesake(workspace, name)
        {
            return Self::Renamed {
                located_by: located_by.to_string(),
                name: name.to_string(),
                other: path::display(&other),
            };
        }
        Self::NoCatalog(located_by.to_string())
    }
}

/// 一份开好的中立库加沉淀库，连这份主库的**主库标识**。
#[derive(Debug)]
pub struct Site {
    /// **中立库**：变体、候选、依据、这一轮的识别结论。
    pub catalog: Catalog,
    /// **沉淀库**：裁决落在这里，**不跟中立库走**，删掉中立库重扫也不丢。
    pub store: Store,
    /// 这份主库的**主库标识**：**路径锚**与**断点**文件名认的都是它。
    ///
    /// **它不是给人看的名字。** 这一串是
    /// [`Slug::text`](crate::workspace::Slug::text) 折出来的「可读的一半 + 哈希」，
    /// 也就是中立库的主文件名；人起的那个**主库原名**走
    /// [`Catalog::library_name`](crate::catalog::Catalog::library_name)。
    pub library_identity: String,
}

impl Site {
    /// 按工作目录里的名字开一份。
    ///
    /// `root` 给了就顺手挡一次：中立库落进主库里是 ADR-0004 的红线，那该在开工之前
    /// 就被拦下，而不是扫到一半才发现。
    ///
    /// # Errors
    /// 中立库不在、落在主库里、或者两份库有一份打不开时返回错误。
    pub fn open(
        workspace: &Path,
        slug: Slug<'_>,
        root: Option<&Path>,
        located_by: &str,
    ) -> Result<Self, SiteError> {
        let catalog = workspace::catalog_path(workspace, slug);
        if !catalog.exists() {
            return Err(SiteError::not_found(workspace, slug, located_by));
        }
        Self::at(workspace, &catalog, slug.text(), root)
    }

    /// 直接开这一份中立库文件。
    ///
    /// **主库标识就是它的主文件名**：中立库落在 `工作目录/catalog/{主库标识}.sqlite3`
    /// （[`workspace::catalog_path`]），所以这不是猜，是同一条算法反过来走。文件被人改过
    /// 名字的话标识就跟着变——那时路径锚会记在另一个标识下，`--library` 才是稳的那条路。
    ///
    /// # Errors
    /// 同 [`Site::open`]。
    pub fn open_file(
        workspace: &Path,
        catalog: &Path,
        root: Option<&Path>,
    ) -> Result<Self, SiteError> {
        if !catalog.exists() {
            return Err(SiteError::NoCatalog(path::display(catalog)));
        }
        let library_identity = workspace::library_identity_of(catalog)
            .ok_or_else(|| SiteError::NotACatalog(path::display(catalog)))?;
        Self::at(workspace, catalog, library_identity, root)
    }

    /// 这份现场里**某一个根**的**断点**文件在哪。
    ///
    /// 与 [`workspace::checkpoint_path`] 折出来的**是同一条路径**，只是不再要一个
    /// [`Slug`]：开完现场的人手上只剩 [`Site::library_identity`]，而它**就是** `Slug::text()`
    /// 交出来的那一串（[`Site::open`] 与 [`Site::open_file`] 两条路都保证，底下那条
    /// 单元测试钉着）。再拿它包一次 `Slug::Named` 会哈希两遍，折出第二个文件名——
    /// 于是界面停下来的那一趟，命令行 `romcat scan --resume` 就接不上了，
    /// 而「命令行裁的界面看得见，反过来也一样」是这个仓库的判据。
    #[must_use]
    pub fn checkpoint_path(&self, workspace: &Path, root_name: &str) -> PathBuf {
        workspace::checkpoint_path_of(workspace, &self.library_identity, root_name)
    }

    /// 这份主库的**主库原名**——窗口标题、报告抬头写的就是它。
    ///
    /// 先问中立库自己记着的原名（[`Catalog::library_name`]，票 01 落进元数据表的那一行；
    /// 读不到那一行时它自己会从文件名截，剥掉哈希后缀）。**只活在内存里的那一份没有
    /// 文件名可截**，那时退回 [`Self::library_identity`]——`library_name` 在那种库上交出来的是
    /// 「（内存）」这个占位路径，对人没有任何意义，而合成数据走的正是这条。
    ///
    /// ## 它**不是** [`Self::library_identity`]
    ///
    /// 那一个是**主库标识**：[`Slug::text`] 折出来的「可读的一半 + 十六位哈希」，也就是中立库
    /// 的主文件名，**路径锚**与**断点**文件名认的都是它——换一个字，命令行裁的界面就
    /// 看不见了。这一个进不了任何键，也没人拿它去找文件，它只回答「人管这份库叫什么」。
    ///
    /// 摆在这儿而不摆在界面层：「一份现场该报哪个名字」是一条领域判断，两处各挑一次
    /// 迟早挑出两个答案（ADR-0005）。
    #[must_use]
    pub fn display_name(&self) -> String {
        if self.catalog.file().is_none() {
            return self.library_identity.clone();
        }
        self.catalog.library_name()
    }

    /// 这份主库的**人工纠正**：沉淀库里按 [`Self::library_identity`] 取出来的那一份——
    /// 成型要照着的就是它（[`ScanOptions::shaping_overrides`](crate::scan::ScanOptions::shaping_overrides)）。
    ///
    /// # Errors
    /// 读沉淀库失败时返回错误。
    pub fn shaping_overrides(&self) -> Result<BTreeMap<String, String>, VerdictError> {
        self.store.shaping_overrides(&self.library_identity)
    }

    /// 定**首选变体**：这个作品在这个平台上默认启动 `variant_key` 那一个。界面上点那一行、
    /// 合并作品时人亲手挑的那几个平台、命令行 `export --prefer` 走的都是它。
    ///
    /// **先落沉淀库，再改中立库那份投影**（与合集 `collection::commit` 同一个次序）：
    /// 投影那一笔写不动时，沉淀库里已经记下了，下次开现场照它重建（[`reconcile`]）；
    /// 反过来的话，写不动沉淀库时屏上亮着、删库重扫就没了。
    ///
    /// # Errors
    /// 两份库有一份写不动时返回错误。
    pub fn set_preferred_variant(
        &mut self,
        work: &str,
        platform: &str,
        variant_key: &str,
    ) -> Result<(), WriteError> {
        self.store
            .set_preferred_variant(&self.library_identity, work, platform, variant_key)?;
        self.catalog
            .set_preferred_variant(work, platform, variant_key)?;
        Ok(())
    }

    /// 撤掉**首选变体**裁决，回到规则算的那一个。返回原来有没有这一条（两份库哪一份里有都算）。
    ///
    /// 次序同 [`Self::set_preferred_variant`]：先沉淀库，再投影。
    ///
    /// # Errors
    /// 两份库有一份写不动时返回错误。
    pub fn clear_preferred_variant(
        &mut self,
        work: &str,
        platform: &str,
    ) -> Result<bool, WriteError> {
        let stored = self
            .store
            .clear_preferred_variant(&self.library_identity, work, platform)?;
        let projected = self.catalog.clear_preferred_variant(work, platform)?;
        Ok(stored || projected)
    }

    /// 往标题集合里加几条**亲手写的叫法**，源一律记成**裁决**（交进来的那一格不看）：
    /// 界面上「加进集合」、作品详情页改显示标题、合并作品时留下的别名走的都是它。
    ///
    /// 次序同 [`Self::set_preferred_variant`]：先沉淀库，再投影。
    ///
    /// # Errors
    /// 两份库有一份写不动时返回错误。
    pub fn add_own_titles(&mut self, rows: &[TitleRow]) -> Result<(), WriteError> {
        let rows: Vec<TitleRow> = rows
            .iter()
            .map(|row| TitleRow {
                source: VERDICT.to_string(),
                ..row.clone()
            })
            .collect();
        self.store.put_own_titles(&self.library_identity, &rows)?;
        self.catalog.put_titles(&rows)?;
        Ok(())
    }

    /// 撤掉一条**亲手写的叫法**（`row` 的源得是裁决，不是就什么都不做、答 `false`）。
    /// 返回中立库里那一行删掉了没有。
    ///
    /// 走的是详情面板上那个「删」同一处（[`title::suppress`](crate::title::suppress)）：
    /// 裁决来源的叫法在那里先删沉淀库那一条、再删投影，不记压制。**只一处实现**，两条路各写
    /// 一遍的话迟早一处忘了沉淀库。刮削来的叫法不走这里：删它要记一条**压制**，那是
    /// `title::suppress` 另一支的事。
    ///
    /// # Errors
    /// 两份库有一份写不动时返回错误。
    pub fn remove_own_title(&mut self, row: &TitleRow) -> Result<bool, WriteError> {
        if !row.is_verdict() {
            return Ok(false);
        }
        let done = crate::title::suppress(
            &mut self.catalog,
            &mut self.store,
            &self.library_identity,
            row,
        )?;
        Ok(done.removed)
    }

    /// 记一格**字段修改**：`subject` 这个锚点上的 `field` 写成 `value`，源记**裁决**、优先于所有
    /// 数据源。作品详情页「编辑 → 保存」、「使用这个值」、合并作品时人挑的那一格走的都是它。
    ///
    /// 次序同 [`Self::set_preferred_variant`]：先沉淀库，再投影——两边记的是同一个时刻，
    /// 下次开现场照沉淀库重建时比得出「一样」，一个字都不写。
    ///
    /// # Errors
    /// 两份库有一份写不动时返回错误。
    pub fn put_verdict_value(
        &mut self,
        anchor: AnchorKind,
        subject: &str,
        field: Field,
        value: &str,
        evidence: &str,
    ) -> Result<(), WriteError> {
        let row = VerdictValue::now(anchor, subject, field, value, evidence);
        self.store.put_verdict_value(&self.library_identity, &row)?;
        self.catalog.put_verdict_row(&row)?;
        Ok(())
    }

    /// 撤掉一格**字段修改**，让数据源重新说了算。返回原来有没有这一格（两份库哪一份里有都算）。
    ///
    /// 次序同 [`Self::set_preferred_variant`]：先沉淀库，再投影。
    ///
    /// # Errors
    /// 两份库有一份写不动时返回错误。
    pub fn clear_verdict_value(
        &mut self,
        anchor: AnchorKind,
        subject: &str,
        field: Field,
    ) -> Result<bool, WriteError> {
        let stored =
            self.store
                .clear_verdict_value(&self.library_identity, anchor, subject, field)?;
        let projected = self.catalog.clear_verdict_value(anchor, subject, field)?;
        Ok(stored || projected)
    }

    /// **收回清单**（`CONTEXT.md`）：把 `sublibrary` 这一台差量预览里**被修改过**的那几份记回它的清单，
    /// 此后工具有权更新或删除它们；连同一笔审计（谁、何时、哪几份）记进沉淀库。交回那一笔；一份都收不了
    /// （没有被修改过的，或者清单在预览之后又变过，[`Manifest::take_back`](crate::sync::Manifest::take_back)）
    /// 时是 `None`，**两份库一个字都不写**。界面上「被修改过」那一栏点头之后走的就是它。
    ///
    /// `surprises` 是人按下去时看着的那份差量里的异常（[`Plan::surprises`](crate::sync::Plan::surprises)）；
    /// `who` 是审计里「谁」那一格，界面填的是 [`verdict::account`](crate::verdict::account)。
    ///
    /// **只改清单与审计**：设备上与主库里的文件一个字节都不碰（ADR-0004、ADR-0015）。
    ///
    /// ## 次序：先记审计，再改清单
    ///
    /// 两份库不在一个事务里。反过来的话，清单改了、审计没记下，就是一次**没有审计的扩权**——恰恰是这件事
    /// 要留一笔的那个理由。所以先落沉淀库（与 [`Self::set_preferred_variant`] 同一个次序）；清单写不进去时
    /// 那一笔收回没生效，审计里那一笔当场删回去，免得它说一句假话。
    ///
    /// # Errors
    /// 读写两份库失败时返回错误；清单没写进去时审计里那一笔已经删回去了。
    pub fn take_back_into_manifest(
        &mut self,
        sublibrary: &str,
        surprises: &[crate::sync::Surprise],
        who: &str,
    ) -> Result<Option<TakeBack>, WriteError> {
        let manifest = self.catalog.manifest(sublibrary)?;
        let (taken_back, files) = manifest.take_back(surprises);
        if files.is_empty() {
            return Ok(None);
        }
        let record = TakeBack {
            sublibrary: sublibrary.to_string(),
            who: who.to_string(),
            at: crate::catalog::now_secs(),
            files,
        };
        let id = self.store.put_take_back(&self.library_identity, &record)?;
        if let Err(error) = self.catalog.put_manifest(sublibrary, &taken_back) {
            // 删不回去的话审计里多一笔没生效的：报的仍是清单那一句——人要先修的是中立库。
            let _ = self.store.drop_take_back(id);
            return Err(error.into());
        }
        Ok(Some(record))
    }

    /// 一份**全在内存里**的现场。演示与实测走这条，连磁盘都不碰。
    #[must_use]
    pub fn in_memory(catalog: Catalog, store: Store, library_identity: &str) -> Self {
        Self {
            catalog,
            store,
            library_identity: library_identity.to_string(),
        }
    }

    fn at(
        workspace: &Path,
        catalog: &Path,
        library_identity: String,
        root: Option<&Path>,
    ) -> Result<Self, SiteError> {
        if let Some(root) = root {
            let root = path::normalize_existing(root);
            let target = path::normalize_existing(catalog);
            if path::is_inside(&root, &target) {
                return Err(SiteError::InsideLibrary(path::display(&target)));
            }
        }
        let catalog = match Catalog::open(catalog) {
            Ok(opened) => opened,
            // **结构版本对不上**：那句话会叫人删掉它重扫。删之前，它里面没搬走的人定的东西
            // 得先救进沉淀库——那句「一条不丢」才是真的。
            Err(error @ CatalogError::Version { .. }) => {
                rescue(workspace, catalog, &error)?;
                return Err(error.into());
            }
            Err(error) => return Err(error.into()),
        };
        let mut catalog = catalog;
        let mut store = Store::open(&workspace::verdict_store_path(workspace))?;
        reconcile(&mut catalog, &mut store, &library_identity)?;
        Ok(Self {
            catalog,
            store,
            library_identity,
        })
    }
}

/// 开一份中立库干活之前，**把它与沉淀库对齐**：旧中立库里还没搬进沉淀库的人定的东西先搬
/// 一次，再把中立库里那几份**投影**照沉淀库重建。
///
/// 开现场（[`Site::open`] / [`Site::open_file`]）走它；命令行上不经现场、自己开两份库的
/// 那两条路（`romcat scan`、`romcat shape`）也走它——**删掉中立库之后头一趟就是扫描**，
/// 投影得在那一趟就回来，不能等人再开一次现场。
///
/// ## 一样人定的东西怎么从中立库搬进沉淀库（做法）
///
/// ADR-0001 的修订：不可再生的东西不住在中立库里。**成型的人工纠正**是头一样
/// （票 `one-criterion-per-thing/07`），**首选变体**与**亲手加的叫法**是第二批
/// （票 `verdict-store-and-sync/01`），**详情页上改过的字段**是第三批（票 `02`）。往后再搬一样，照这几步：
///
/// 1. **沉淀库追加一条迁移**（`verdict::MIGRATIONS`，只追加、不改已有的）：一张表，列是中立库
///    那张表的列，**键前面加主库标识**（`library`），再加一列 `decided_at`。它与**路径锚**同一个
///    处境——只在本机这一份主库里成立——所以**导出不带它**（`Store::export` 只折裁决与匹配
///    裁决两张表，什么都不用做）。中立库那几行的源一律是裁决的，源那一列不另存；那张表自己
///    带着时刻一列的（`scrape_value.at`），`decided_at` 就是它，投影时原样写回——两边一字不差，
///    重建时才比得出「一样」。
/// 2. **中立库那张表降为投影**：建表语句不动（不升结构版本），这里照沉淀库**整份换掉**；
///    与眼下一样就一个字都不写。写它的每一处人的动作改走 [`Site`] 上那个写入口：先落沉淀库，
///    再改投影（删一条叫法另有 [`title::suppress`](crate::title::suppress) 那一处，它不收现场、
///    收的是两份库加主库标识；[`Site::remove_own_title`] 走的就是它）。**一处都不许漏**：
///    投影整份替换，漏下的那一处照旧直写中立库，写下的东西屏上当场亮着，下次开现场就被抹掉——
///    界面上按得下去的每一处，拿一条「关掉再打开」的界面测试钉住。
/// 3. **旧中立库里已有的救进沉淀库一次**，在中立库的元数据表上记下「搬过了」（每一样一个键，
///    `catalog::meta::MetaKey`），同一个键上沉淀库已有的不盖；结构版本对不上、开不进去的
///    那一份在 [`rescue`] 里只读地救。**旧行一行不删**：搬走不是删掉。
/// 4. 删库那句话（[`CatalogError::Version`]）把它从「会丢的」挪到「一条不丢」那边。
///
/// **次序：先搬、再投影。** 反过来的话，头一次开一份旧库时投影照一份还空着的沉淀库重建，
/// 旧行当场就被抹掉了。
///
/// **不是从中立库搬过来、而是新长出来的一样**只走第 1 步：**收回清单**的审计（票 `verdict-store-and-sync/08`）
/// 与成型纠正的**待生效**记录（票 `12`）键前面是主库标识、时刻叫 `decided_at`、导出不带；中立库里没有它们的
/// 旧行可救，也没有哪张表是它们的投影，所以这里一行都不用加。
///
/// # Errors
/// 读写两份库失败时返回错误。
pub fn reconcile(catalog: &mut Catalog, store: &mut Store, library: &str) -> Result<(), SiteError> {
    carry_over_shaping_overrides(catalog, store, library)?;
    carry_over_preferred_and_titles(catalog, store, library)?;
    carry_over_verdict_values(catalog, store, library)?;
    catalog.replace_preferred_variants(&store.preferred_variants(library)?)?;
    catalog.replace_verdict_titles(&store.own_titles(library)?)?;
    catalog.replace_verdict_values(&store.verdict_values(library)?)?;
    Ok(())
}

/// 把票 `one-criterion-per-thing/07` 之前记在这份中立库里的**人工纠正**搬进沉淀库，
/// **只搬一次**。返回新收下几条。
///
/// 人工纠正从那张票起住沉淀库（ADR-0001 的修订，挂账 D97）。旧程序建的中立库里那张表
/// 可能攒着人一条条纠正出来的东西；新程序不再读它，**不搬一次那些就悄悄没了**。
/// 开一份现场（[`Site::open`] / [`Site::open_file`]）先做这件事；命令行上不经现场、
/// 自己开两份库再成型的那两条路（`romcat scan`、`romcat shape`）也调它。
///
/// 顺序是**先写沉淀库、再在中立库里记「搬过了」**：中途断掉，最坏是下次再搬一遍
/// （沉淀库里已有的不盖），不会丢。记下之后旧表不再交出东西——人在沉淀库里撤掉的，
/// 不许被旧表带回来。**旧表一行不删**：搬走不是删掉。
///
/// `library` 是这份中立库的**主库标识**：沉淀库几份主库共用，纠正按它分开。
/// 结构版本对不上、开不进去的那一份走 [`rescue`]。
///
/// # Errors
/// 读中立库、写沉淀库或记下「搬过了」失败时返回错误。
pub fn carry_over_shaping_overrides(
    catalog: &Catalog,
    store: &mut Store,
    library: &str,
) -> Result<usize, SiteError> {
    // 只活在内存里的库是这一版建的，没有旧表。
    let Some(file) = catalog.file() else {
        return Ok(0);
    };
    let Some(stranded) = Catalog::stranded_shaping_overrides(file)? else {
        return Ok(0);
    };
    let added = store.add_missing_shaping_overrides(library, &stranded)?;
    catalog.mark_shaping_overrides_carried()?;
    Ok(added)
}

/// 把票 `verdict-store-and-sync/01` 之前记在这份中立库里的**首选变体**与**亲手加的叫法**
/// 搬进沉淀库，**只搬一次**。返回新收下几条（两样合起来）。
///
/// 做法与 [`carry_over_shaping_overrides`] 一个字不差（[`reconcile`] 那一段「做法」）：
/// 先写沉淀库、再在中立库里记「搬过了」，中途断掉最坏是下次再搬一遍（沉淀库里已有的不盖）；
/// 记下之后旧行不再交出东西——人在沉淀库里撤掉的，不许被旧行带回来。**旧行一行不删**，
/// 紧接着的投影照沉淀库把它们重建一遍，内容一样就一个字都不写。
///
/// # Errors
/// 读中立库、写沉淀库或记下「搬过了」失败时返回错误。
pub fn carry_over_preferred_and_titles(
    catalog: &Catalog,
    store: &mut Store,
    library: &str,
) -> Result<usize, SiteError> {
    // 只活在内存里的库是这一版建的，没有旧行。
    let Some(file) = catalog.file() else {
        return Ok(0);
    };
    let Some(stranded) = Catalog::stranded_preferred_and_titles(file)? else {
        return Ok(0);
    };
    let added = store.add_missing_preferred_variants(library, &stranded.preferred)?
        + store.add_missing_own_titles(library, &stranded.titles)?;
    catalog.mark_preferred_and_titles_carried()?;
    Ok(added)
}

/// 把票 `verdict-store-and-sync/02` 之前记在这份中立库里的**详情页上改过的字段**搬进沉淀库，
/// **只搬一次**。返回新收下几格。
///
/// 做法与 [`carry_over_preferred_and_titles`] 一个字不差（[`reconcile`] 那一段「做法」）。
/// 搬进去的每一格**原样带着它的依据与时刻**：紧接着的投影照沉淀库重建，与旧行一模一样，
/// 一个字都不写。
///
/// # Errors
/// 读中立库、写沉淀库或记下「搬过了」失败时返回错误。
pub fn carry_over_verdict_values(
    catalog: &Catalog,
    store: &mut Store,
    library: &str,
) -> Result<usize, SiteError> {
    // 只活在内存里的库是这一版建的，没有旧行。
    let Some(file) = catalog.file() else {
        return Ok(0);
    };
    let Some(stranded) = Catalog::stranded_verdict_values(file)? else {
        return Ok(0);
    };
    let added = store.add_missing_verdict_values(library, &stranded)?;
    catalog.mark_verdict_values_carried()?;
    Ok(added)
}

/// **结构版本对不上、开不进去**的那一份中立库：把它里面还没搬走的人定的东西——**人工纠正**、
/// **首选变体**、**亲手加的叫法**、**详情页上改过的字段**——救进沉淀库。返回新收下几条（几样合起来）。
///
/// 那份库打不开，它交出来的那句话（`unopened`，[`CatalogError::Version`]）叫人删掉它
/// 重扫、并说这几样「一条不丢」——**这句话要成立，删之前就得先救**，而现场开不起来，
/// [`reconcile`] 那条路走不到。所以撞上这句话的每一处都先调它：
/// 开现场（`Site::at`）、开场列举（`workspace::catalogs`）、命令行自己开中立库的那两处。
///
/// **那份旧库一个字不改**（不记「搬过了」）：它开不进去，也就没人能在沉淀库里撤掉它的
/// 那几样，再救一遍只是把已经有的再核一遍（`Store::add_missing_*` 那几支都是已有的不盖）。
/// 主库标识从文件名认（[`workspace::library_identity_of`]），与开现场同一处。
///
/// 往后再搬一样人定的东西（[`reconcile`] 那一段「做法」），在这里再添一支。
///
/// # Errors
/// 没救成时交回 [`SiteError::Stranded`]：原话连同没救成的原因，说清删之前先弄好沉淀库。
pub fn rescue(
    workspace: &Path,
    catalog_file: &Path,
    unopened: &CatalogError,
) -> Result<usize, SiteError> {
    let rescue = || -> Result<usize, SiteError> {
        let shaping = Catalog::stranded_shaping_overrides(catalog_file)?.unwrap_or_default();
        let own = Catalog::stranded_preferred_and_titles(catalog_file)?.unwrap_or_default();
        let values = Catalog::stranded_verdict_values(catalog_file)?.unwrap_or_default();
        if shaping.is_empty()
            && own.preferred.is_empty()
            && own.titles.is_empty()
            && values.is_empty()
        {
            return Ok(0);
        }
        let library = workspace::library_identity_of(catalog_file)
            .ok_or_else(|| SiteError::NotACatalog(path::display(catalog_file)))?;
        let mut store = Store::open(&workspace::verdict_store_path(workspace))?;
        Ok(store.add_missing_shaping_overrides(&library, &shaping)?
            + store.add_missing_preferred_variants(&library, &own.preferred)?
            + store.add_missing_own_titles(&library, &own.titles)?
            + store.add_missing_verdict_values(&library, &values)?)
    };
    rescue().map_err(|why| SiteError::Stranded {
        said: unopened.to_string(),
        why: why.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn 中立库的主文件名就是这份主库的主库标识() {
        // `Site::open_file` 靠这条把 `--catalog <文件>` 逆推回主库标识。它不是猜——
        // `catalog_path` 就是这么拼的。这条一旦漂开，直接开文件裁出来的**路径锚**
        // 会记在另一个标识下，`--library` 那条路就看不见它们了。
        let workspace = PathBuf::from("/work");
        for slug in [
            Slug::Named("主库"),
            Slug::AtPath(Path::new("/Volumes/甲/Game")),
        ] {
            let path = workspace::catalog_path(&workspace, slug);
            assert_eq!(
                path.file_stem().expect("有主文件名").to_string_lossy(),
                slug.text(),
            );
        }
    }

    #[test]
    fn 界面折出来的断点路径与命令行找的是同一个文件() {
        // 界面手上只有 `Site::library_identity`，命令行手上是 `Slug`。两条路折出两个文件名的话，
        // 界面停下来的那一趟，`romcat scan --resume` 就接不上——而「命令行裁的界面
        // 看得见，反过来也一样」是这个仓库的判据。
        let workspace = PathBuf::from("/work");
        for slug in [
            Slug::Named("主库"),
            Slug::AtPath(Path::new("/Volumes/甲/Game")),
        ] {
            let site = Site {
                catalog: Catalog::open_in_memory().expect("开得出"),
                store: crate::verdict::Store::in_memory().expect("开得出"),
                library_identity: slug.text(),
            };
            for root_name in ["主库", "元数据库", "带 / 斜杠的根名"] {
                assert_eq!(
                    site.checkpoint_path(&workspace, root_name),
                    workspace::checkpoint_path(&workspace, slug, root_name),
                );
            }
        }
    }
}
