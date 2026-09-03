//! 一份开好的**中立库**加**沉淀库**：界面开工前要的全部原料。
//!
//! 两份库都在**工作目录**里，都在本机（ADR-0009）——**主库一个字节都不读**，
//! 队列、候选、依据、裁决全部由这两份库折出来（ADR-0001）。所以外置盘没挂上的时候
//! 这个界面照样打得开、照样裁得动。
//!
//! ## 为什么路径的算法要跟命令行一模一样
//!
//! 一条**路径锚**记的是「主库『某某』里的某某变体」，那个「某某」就是中立库的文件名
//! （`workspace::Slug::text`）。界面若自己另起一个名字，同一台机器上界面裁的与命令行裁的
//! 就落在两批锚上——`romcat triage list` 看不见界面刚裁完的那几条。于是这里只做一件事：
//! 把 `--library` / 主库根 / `--catalog` 三种给法**折回同一个名字**。
//!
//! `--catalog` 那条尤其要说清：中立库的文件名**就是**那个名字加 `.sqlite3`，
//! 所以直接开一个文件时，主库的名字就是它的主文件名——不是猜的，是同一条算法的逆向。

use std::path::{Path, PathBuf};

use romcat_core::catalog::Catalog;
use romcat_core::verdict::Store;
use romcat_core::workspace::{self, Slug};

/// 一份开好的中立库加沉淀库，连这份主库在裁决里叫什么名字。
pub struct Site {
    /// **中立库**：变体、候选、依据、这一轮的识别结论。
    pub catalog: Catalog,
    /// **沉淀库**：裁决落在这里，**不跟中立库走**，删掉中立库重扫也不丢。
    pub store: Store,
    /// 这份主库在**路径锚**里叫什么名字。
    pub library: String,
}

/// 界面从哪儿找中立库。
#[derive(Debug, Clone, Default)]
pub struct Locate<'a> {
    /// 主库根目录。**只用来找到对应的中立库，一个字节都不读它。**
    pub root: Option<&'a Path>,
    /// 按名字找（扫描时 `--library` 起的那个名字）。
    pub library: Option<&'a str>,
    /// 工作目录：中立库与沉淀库存这里。
    pub workspace: Option<&'a Path>,
    /// 直接开这一份中立库文件。给了它就不再按名字找。
    pub catalog: Option<&'a Path>,
}

impl Site {
    /// 开一份现成的库。
    ///
    /// # Errors
    /// 说不出要开哪一份、库不在、或者打不开时，返回一句给人看的话。
    pub fn open(locate: &Locate<'_>) -> Result<Self, String> {
        let workspace = locate.workspace.map_or_else(
            || {
                locate
                    .catalog
                    .and_then(workspace_of)
                    .unwrap_or_else(workspace::default_dir)
            },
            Path::to_path_buf,
        );
        let (path, library) = match locate.catalog {
            // 中立库的文件名就是主库在**路径锚**里的名字（见模块文档）。
            Some(path) => (path.to_path_buf(), stem_of(path)?),
            None => {
                let slug = match (locate.library, locate.root) {
                    (Some(name), _) => Slug::Named(name),
                    (None, Some(root)) => Slug::AtPath(root),
                    (None, None) => {
                        return Err("说清要开哪份库：给主库根、`--library <名字>`，\
                                    或者 `--catalog <文件>`。不给就是合成数据。"
                            .to_string());
                    }
                };
                (workspace::catalog_path(&workspace, slug), slug.text())
            }
        };
        if !path.exists() {
            return Err(format!(
                "还没有 {} 这份中立库。先跑一次 `romcat scan`。",
                path.display()
            ));
        }
        let catalog = Catalog::open(&path).map_err(|error| format!("中立库打不开：{error}"))?;
        let store = Store::open(&workspace::verdict_store_path(&workspace))
            .map_err(|error| format!("沉淀库打不开：{error}"))?;
        Ok(Self {
            catalog,
            store,
            library,
        })
    }

    /// 一份**全在内存里**的现场：合成数据配一份空沉淀库。
    ///
    /// 演示与实测走这条。**主库只读**（ADR-0004），而这条路连磁盘都不碰。
    #[must_use]
    pub fn in_memory(catalog: Catalog, store: Store) -> Self {
        Self {
            catalog,
            store,
            library: "合成数据".to_string(),
        }
    }
}

/// `工作目录/catalog/某某.sqlite3` 反推回工作目录；形状不对就是 `None`。
fn workspace_of(catalog: &Path) -> Option<PathBuf> {
    let parent = catalog.parent()?;
    if parent.file_name()? != "catalog" {
        return None;
    }
    Some(parent.parent()?.to_path_buf())
}

/// 中立库文件的主文件名——也就是这份主库在路径锚里的名字。
fn stem_of(catalog: &Path) -> Result<String, String> {
    catalog
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .ok_or_else(|| format!("{} 不像是一份中立库文件。", catalog.display()))
}
