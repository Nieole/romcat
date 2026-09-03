//! 一份开好的**中立库**加**沉淀库**：裁决这件事要的全部原料。
//!
//! 两份库都在**工作目录**里，都在本机（ADR-0009）——**主库一个字节都不读**。
//! 队列、候选、依据、裁决全部由这两份库折出来（ADR-0001），所以外置盘没挂上的时候
//! 照样列得出队列、裁得下去。
//!
//! ## 为什么这三样必须一起开
//!
//! 一条**路径锚**记的是「主库『某某』里的某某变体」，而那个「某某」就是中立库的文件名
//! （[`Slug::text`]）。谁自己另起一个名字，谁裁出来的那批锚就与别处对不上——命令行裁的
//! 界面看不见，反过来也一样。把「开哪份中立库」「开哪份沉淀库」「这份主库叫什么」捏成
//! 一个类型，就是不让第二份算法长出来。

use std::path::Path;

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
}

/// 一份开好的中立库加沉淀库，连这份主库在**裁决**里叫什么名字。
#[derive(Debug)]
pub struct Site {
    /// **中立库**：变体、候选、依据、这一轮的识别结论。
    pub catalog: Catalog,
    /// **沉淀库**：裁决落在这里，**不跟中立库走**，删掉中立库重扫也不丢。
    pub store: Store,
    /// 这份主库在**路径锚**里叫什么名字。
    pub library: String,
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
            return Err(SiteError::NoCatalog(located_by.to_string()));
        }
        Self::at(workspace, &catalog, slug.text(), root)
    }

    /// 直接开这一份中立库文件。
    ///
    /// **主库的名字就是它的主文件名**：中立库落在 `工作目录/catalog/{名字}.sqlite3`
    /// （[`workspace::catalog_path`]），所以这不是猜，是同一条算法反过来走。文件被人改过
    /// 名字的话名字就跟着变——那时路径锚会记在另一个名字下，`--library` 才是稳的那条路。
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
        let library = catalog
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .ok_or_else(|| SiteError::NotACatalog(path::display(catalog)))?;
        Self::at(workspace, catalog, library, root)
    }

    /// 一份**全在内存里**的现场。演示与实测走这条，连磁盘都不碰。
    #[must_use]
    pub fn in_memory(catalog: Catalog, store: Store, library: &str) -> Self {
        Self {
            catalog,
            store,
            library: library.to_string(),
        }
    }

    fn at(
        workspace: &Path,
        catalog: &Path,
        library: String,
        root: Option<&Path>,
    ) -> Result<Self, SiteError> {
        if let Some(root) = root {
            let root = path::normalize_existing(root);
            let target = path::normalize_existing(catalog);
            if path::is_inside(&root, &target) {
                return Err(SiteError::InsideLibrary(path::display(&target)));
            }
        }
        Ok(Self {
            catalog: Catalog::open(catalog)?,
            store: Store::open(&workspace::verdict_store_path(workspace))?,
            library,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn 中立库的主文件名就是这份主库的名字() {
        // `Site::open_file` 靠这条把 `--catalog <文件>` 逆推回主库的名字。它不是猜——
        // `catalog_path` 就是这么拼的。这条一旦漂开，直接开文件裁出来的**路径锚**
        // 会记在另一个名字下，`--library` 那条路就看不见它们了。
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
}
