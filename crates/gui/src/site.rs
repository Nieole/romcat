//! 界面从哪儿找那两份库。
//!
//! **开库这件事本身在核心里**（[`romcat_core::site::Site`]）——中立库、沉淀库、
//! 「这份主库在**路径锚**里叫什么名字」三样一起开，界面与命令行走同一条。谁自己另起
//! 一个名字，谁裁出来的那批锚就与别处对不上。
//!
//! 这一层只剩一件事：把命令行上那三种给法（主库根 / `--library <名字>` /
//! `--catalog <文件>`）折成核心库认得的形状，外加一条 `--catalog` 独有的推断——
//! 它的**工作目录**从文件路径反推（`工作目录/catalog/某某.sqlite3`），因为沉淀库
//! 得跟它住在同一个工作目录里。

use std::path::{Path, PathBuf};

use romcat_core::site::Site;
use romcat_core::workspace::{self, Slug};

/// 界面从哪儿找中立库。三种给法任给一样。
///
/// **一样都不给的那条路不归它管**：由 [`crate::program::Program::start`] 接走，
/// 交给[**开场**](crate::opening)那一屏——列出这个工作目录里有哪些库，挑一份开进去
/// （ADR-0023：界面自足，而且不擅自造一份假的）。这行字从前写的是「都不给就是合成
/// 数据」，那是 `--demo` 还会兜底的年代留下的。
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

impl<'a> Locate<'a> {
    /// 只说「开这一份中立库文件」的那条路。
    ///
    /// 它单独立成一个构造子，是因为**这条路自带一段推断**——工作目录从那条文件路径反推
    /// （[`Self::workspace_dir`]）。[**上次开的那份**](crate::recent)记下来的正是这样一条
    /// 路径，于是「拿记着的那份开库」与「判它属不属于这个工作目录」两处走的是同一条路，
    /// 没有第二个地方再拼一次这个形状。
    #[must_use]
    pub fn at_catalog(catalog: &'a Path) -> Self {
        Self {
            catalog: Some(catalog),
            ..Self::default()
        }
    }

    /// 说了要开现成的库吗。
    #[must_use]
    pub fn given(&self) -> bool {
        self.catalog.is_some() || self.library.is_some() || self.root.is_some()
    }

    /// 这一趟说的工作目录**管得着这份中立库吗**。
    ///
    /// 没说 `--workspace` 时一律算数——那时工作目录本来就是跟着中立库走的
    /// （[`Self::workspace_dir`] 的第二支）。说了的话，那份库得**住在他说的那个工作目录
    /// 里**：换一个工作目录等于换一整套工具状态（`CONTEXT.md` 的**工作目录**），
    /// 把别处那一份开进来就等于把人当场说的话当没听见。
    ///
    /// **比的是反推出来的那个工作目录**，不另拆一遍路径：形状不对（那条路径压根不长成
    /// `工作目录/catalog/某某.sqlite3`）时它退回默认那个，于是与人给的目录对不上——
    /// 那是对的，说不出自己属于哪个工作目录的一份库，本来就不该算作「这个工作目录里的
    /// 那一份」。
    ///
    /// 眼下只有一个调用方：[`Program::start_with`](crate::program::Program::start_with)
    /// 拿它判「记着的那份这一趟还算不算数」。
    #[must_use]
    pub fn covers(&self, catalog: &Path) -> bool {
        self.workspace
            .is_none_or(|说的| Locate::at_catalog(catalog).workspace_dir() == 说的)
    }

    /// 这一趟的**工作目录**：中立库、沉淀库、**媒体池**、能力档案名册都在这儿。
    ///
    /// 说了 `--workspace` 就是它；只给了 `--catalog` 时从那条路径反推
    /// （从 `工作目录/catalog/某某.sqlite3` 那个形状反推）；再没有就是默认那个。
    ///
    /// 它单独交出来，是因为**子库那一屏要用它**：排差量预览要读媒体池与能力档案名册
    /// （`romcat_core::sync::prepare`）。
    #[must_use]
    pub fn workspace_dir(&self) -> PathBuf {
        self.workspace.map_or_else(
            || {
                self.catalog
                    .and_then(workspace_of)
                    .unwrap_or_else(workspace::default_dir)
            },
            Path::to_path_buf,
        )
    }

    /// 开这份现场。
    ///
    /// # Errors
    /// 说不出要开哪一份、库不在、或者打不开时，返回一句给人看的话。
    pub fn open(&self) -> Result<Site, String> {
        let workspace = self.workspace_dir();
        let opened = match self.catalog {
            Some(catalog) => Site::open_file(&workspace, catalog, self.root),
            None => {
                let (slug, located_by) = match (self.library, self.root) {
                    (Some(name), _) => (Slug::Named(name), format!("--library {name}")),
                    (None, Some(root)) => (Slug::AtPath(root), romcat_core::path::display(root)),
                    // **走不到这儿**：一样都不给的那条路由
                    // [`crate::program::Program::start`] 先接走（它先问 [`Self::given`]，
                    // 然后进开场）。留着这一支是因为这个函数是公开的，谁都能空手调它
                    // 一次；但它这句话从前写的是「不给就是合成数据」，与 ADR-0023 正相反。
                    (None, None) => {
                        return Err("说清要开哪份库：给主库根、`--library <名字>`，\
                                    或者 `--catalog <文件>`。"
                            .to_string());
                    }
                };
                Site::open(&workspace, slug, self.root, &located_by)
            }
        };
        opened.map_err(|error| format!("{error}"))
    }
}

/// `工作目录/catalog/某某.sqlite3` 反推回工作目录；形状不对就是 `None`。
///
/// 沉淀库不跟中立库同一个文件，但跟它同一个**工作目录**——只给了 `--catalog` 时，
/// 那个目录只能从这条路径上读出来。
fn workspace_of(catalog: &Path) -> Option<PathBuf> {
    let parent = catalog.parent()?;
    if parent.file_name()? != "catalog" {
        return None;
    }
    Some(parent.parent()?.to_path_buf())
}
