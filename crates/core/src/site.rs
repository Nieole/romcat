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

use crate::catalog::{Catalog, CatalogError};
use crate::path;
use crate::verdict::{Store, VerdictError};
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
    /// 中立库结构版本对不上，而它里面还没搬进沉淀库的**人工纠正**这一回没救出来
    /// （[`rescue_shaping_overrides`]）。那句「删掉它重扫」**这时不能照做**。
    #[error(
        "{said}——但这份库里还记着没搬进沉淀库的人工纠正，这一回没搬成（{why}）。\
         删掉它之前先把沉淀库弄好，不然它们就没了"
    )]
    Stranded {
        /// 那句叫人删库重扫的原话（[`CatalogError::Version`]）。
        said: String,
        /// 没搬成的原因。
        why: String,
    },
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
            // **结构版本对不上**：那句话会叫人删掉它重扫。删之前，它里面没搬走的人工纠正
            // 得先救进沉淀库——那句「人工纠正一条不丢」才是真的。
            Err(error @ CatalogError::Version { .. }) => {
                rescue_shaping_overrides(workspace, catalog, &error)?;
                return Err(error.into());
            }
            Err(error) => return Err(error.into()),
        };
        let mut store = Store::open(&workspace::verdict_store_path(workspace))?;
        carry_over_shaping_overrides(&catalog, &mut store, &library_identity)?;
        Ok(Self {
            catalog,
            store,
            library_identity,
        })
    }
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
/// 结构版本对不上、开不进去的那一份走 [`rescue_shaping_overrides`]。
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

/// **结构版本对不上、开不进去**的那一份中立库：把它里面还没搬走的人工纠正救进沉淀库。
/// 返回新收下几条。
///
/// 那份库打不开，它交出来的那句话（`unopened`，[`CatalogError::Version`]）叫人删掉它
/// 重扫、并说「人工纠正一条不丢」——**这句话要成立，删之前就得先救**，而现场开不起来，
/// [`carry_over_shaping_overrides`] 那条路走不到。所以撞上这句话的每一处都先调它：
/// 开现场（`Site::at`）、开场列举（`workspace::catalogs`）、命令行自己开中立库的那两处。
///
/// **那份旧库一个字不改**（不记「搬过了」）：它开不进去，也就没人能在沉淀库里撤掉它的
/// 纠正，再救一遍只是把已经有的再核一遍（[`Store::add_missing_shaping_overrides`]）。
/// 主库标识从文件名认（[`workspace::library_identity_of`]），与开现场同一处。
///
/// # Errors
/// 没救成时交回 [`SiteError::Stranded`]：原话连同没救成的原因，说清删之前先弄好沉淀库。
pub fn rescue_shaping_overrides(
    workspace: &Path,
    catalog_file: &Path,
    unopened: &CatalogError,
) -> Result<usize, SiteError> {
    let rescue = || -> Result<usize, SiteError> {
        let Some(stranded) = Catalog::stranded_shaping_overrides(catalog_file)? else {
            return Ok(0);
        };
        if stranded.is_empty() {
            return Ok(0);
        }
        let library = workspace::library_identity_of(catalog_file)
            .ok_or_else(|| SiteError::NotACatalog(path::display(catalog_file)))?;
        let mut store = Store::open(&workspace::verdict_store_path(workspace))?;
        Ok(store.add_missing_shaping_overrides(&library, &stranded)?)
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
