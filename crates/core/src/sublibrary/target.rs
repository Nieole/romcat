//! **目标路径挑得对不对**：新建子库、改目标设置时，路径一填进来就判一遍（票 `gui-looks-like-the-design/21`）。
//!
//! ## 挡三种，再说清在不在
//!
//! 1. **落在主库的根里，或者把根包在里面**（[`TargetRefusal::InLibrary`]）。主库只读（ADR-0004），而同步是真的往
//!    目标上建目录、写文件、删文件。把根包在里面也不行：同步往 `<平台目录>/…` 写，平台目录与根同名时就写进了主库。
//! 2. **属于工作目录，或者把工作目录包在里面**（[`TargetRefusal::InWorkspace`]）。中立库、断点、媒体池住在那儿，
//!    同步删的是清单里的路径，路径一撞就删到工具自己的东西上。
//! 3. **已被别的子库占用**（[`TargetRefusal::Taken`]）：同一条路径，或者套在一起。两份清单管着同一棵目录，
//!    一台同步时会把另一台放上去的文件当成「清单之外」——而清单之外的东西工具连看都不该看（ADR-0015）。
//!
//! 「套在一起」**两个方向都算**，与加根那道 [`check_placement`](crate::catalog::roots::check_placement) 同一个口径。
//!
//! 过了这三道再看**在不在**（[`Presence`]）：在的话报出那个卷的文件系统与可用空间；**不在也照样建得出**——子库是
//! 持久实体，不是「插上卡才存在的东西」（ADR-0015）。
//!
//! ## 为什么在核心里
//!
//! 界面上的弹层、命令行、点同步时那道闸问的是同一件事（ADR-0024）。「落在主库里」那一条与
//! [`refuse_target_in_library`](crate::sync::prepare::refuse_target_in_library) 共用 [`library_overlap`]，
//! 两边不各判一遍。
//!
//! ## 每调一次都查盘
//!
//! 化开路径、看那个卷，都是真去问文件系统。**什么时候调由壳定**：界面只在路径那一格的字改了、或者目录选择器交回来时
//! 调一次，不在每一帧里调——挂载点卡住时整个窗口会跟着卡。

use std::path::{Path, PathBuf};

use crate::catalog::roots::Roots;
use crate::catalog::{Catalog, CatalogError};
use crate::path;

/// 一条目标路径**不能用**的理由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetRefusal {
    /// 没填。
    Empty,
    /// 落在主库的根里，或者把根包在里面（ADR-0004）。
    InLibrary {
        /// 撞上的是哪个根。
        root: String,
        /// 那个根在哪。
        place: String,
        /// 是目标把根包在里面（`true`），还是目标落在根里（`false`，含相等）。
        around: bool,
    },
    /// 属于工作目录，或者把工作目录包在里面。
    InWorkspace {
        /// 工作目录在哪。
        workspace: String,
    },
    /// 已被别的子库占用：同一条路径，或者套在一起。
    Taken {
        /// 被哪个子库占着。
        by: String,
    },
    /// 那条路径上是一份文件，不是目录。
    NotADirectory,
}

/// 目标此刻**在不在**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Presence {
    /// 那个目录不在（卡没插、盘没挂）。**照样建得出。**
    Absent,
    /// 在：它所在的那个卷。
    Present(Volume),
}

/// 目标所在的那个卷。每一格都**读得到才有**，读不到是 `None`，不编一个数。
///
/// 由 `sysinfo` 的磁盘接口读（Windows / macOS / Linux 都给，工作区清单里写着为什么挑它）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Volume {
    /// 文件系统，写成人认得的样子（`exFAT`、`FAT32`、`APFS`……）。
    pub filesystem: Option<String>,
    /// 卷的总量，字节。**按设备容量**那一档的上限就是它。
    pub total: Option<u64>,
    /// 卷上还能写多少，字节。
    pub available: Option<u64>,
    /// 是不是**可移动存储**（SD 卡、U 盘、读卡器挂上来的盘；macOS 上挂着的磁盘映像也算）。读不到时是 `false`。
    pub removable: bool,
}

/// 一个子库名字**不能用**的理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameRefusal {
    /// 没填。
    Empty,
    /// 已经有别的子库叫这个名字。子库按名字存，放行就是悄悄把那一台的目标设置盖掉。
    Taken,
}

/// 目标路径与主库的根撞在一起的那一处。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryOverlap {
    /// 撞上的是哪个根。
    pub root: String,
    /// 那个根在哪。
    pub place: PathBuf,
    /// 是目标把根包在里面（`true`），还是目标落在根里（`false`，含相等）。
    pub around: bool,
}

/// 目标与这一组根里哪一个撞在一起：落在根里（含相等）、或者把根包在里面。都不撞是 `None`。
///
/// `target` 要先化成可比较的绝对形态（[`path::normalize_existing`]）；两边比之前各自折成可比形态
/// （[`path::is_inside_place`]），Windows 上 `\\?\D:\…` 与库里存的 `D:\…` 才比得上。
#[must_use]
pub fn library_overlap(roots: &Roots, target: &Path) -> Option<LibraryOverlap> {
    roots.iter().find_map(|(name, root)| {
        let around = if path::is_inside_place(root, target) {
            false
        } else if path::is_inside_place(target, root) {
            true
        } else {
            return None;
        };
        Some(LibraryOverlap {
            root: name.to_string(),
            place: root.to_path_buf(),
            around,
        })
    })
}

/// 两条（化开过的）路径套不套在一起：相等、或者任一方落在另一方里。
fn entangled(a: &Path, b: &Path) -> bool {
    path::is_inside_place(a, b) || path::is_inside_place(b, a)
}

/// 判一条目标路径：三种拦下的理由（见模块文档），都过了再说它在不在。
///
/// `editing` 是正在改的那一台叫什么——**它自己原来那条路径不算被占**；新建时给 `None`。`target` 是系统给的原始形式
/// （界面上框里那一串、目录选择器交回来的那一个），读盘走它（ADR-0020）。
///
/// # Errors
/// 中立库读不动（根、已有的子库）时返回 [`CatalogError`]。
pub fn vet(
    catalog: &Catalog,
    workspace: &Path,
    editing: Option<&str>,
    target: &Path,
) -> Result<Result<Presence, TargetRefusal>, CatalogError> {
    if path::display(target).trim().is_empty() {
        return Ok(Err(TargetRefusal::Empty));
    }
    let place = path::normalize_existing(target);
    if let Some(overlap) = library_overlap(&Roots::load(catalog)?, &place) {
        return Ok(Err(TargetRefusal::InLibrary {
            root: overlap.root,
            place: path::display(&overlap.place),
            around: overlap.around,
        }));
    }
    let workspace = path::normalize_existing(workspace);
    if entangled(&workspace, &place) {
        return Ok(Err(TargetRefusal::InWorkspace {
            workspace: path::display(&workspace),
        }));
    }
    for other in catalog.sublibraries()? {
        if editing == Some(other.name.as_str()) {
            continue;
        }
        if entangled(&path::normalize_existing(&other.read_path()), &place) {
            return Ok(Err(TargetRefusal::Taken { by: other.name }));
        }
    }
    Ok(if target.is_dir() {
        Ok(Presence::Present(volume(target)))
    } else if target.exists() {
        Err(TargetRefusal::NotADirectory)
    } else {
        Ok(Presence::Absent)
    })
}

/// 判一个子库名字：空着、或者已被别的子库用了就拦下。
///
/// `editing` 是正在改的那一台叫什么——名字没改就不算撞；新建时给 `None`。名字两头的空白不算数（存的时候也去掉）。
///
/// # Errors
/// 中立库读不动时返回 [`CatalogError`]。
pub fn vet_name(
    catalog: &Catalog,
    editing: Option<&str>,
    name: &str,
) -> Result<Result<(), NameRefusal>, CatalogError> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(Err(NameRefusal::Empty));
    }
    if editing == Some(name) {
        return Ok(Ok(()));
    }
    Ok(match catalog.sublibrary(name)? {
        Some(_) => Err(NameRefusal::Taken),
        None => Ok(()),
    })
}

/// 看一眼 `place` 所在的卷：**挂载点是 `place` 的上级里最深的那一个**（`/Volumes/SDCARD/Game` 落在
/// `/Volumes/SDCARD` 上，不落在 `/` 上）。一个都对不上（列不出卷）时每一格都空着。
///
/// 查盘，什么时候调由调用方定（模块文档「每调一次都查盘」）。
#[must_use]
pub fn volume(place: &Path) -> Volume {
    use sysinfo::{DiskRefreshKind, Disks};

    let place = path::normalize_existing(place);
    let disks =
        Disks::new_with_refreshed_list_specifics(DiskRefreshKind::everything().without_io_usage());
    let Some(disk) = disks
        .list()
        .iter()
        .filter(|disk| path::is_inside_place(disk.mount_point(), &place))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
    else {
        return Volume::default();
    };
    // 总量是 0 说明这一格没读到（真卷不会是 0 字节）：那时可用空间也不可信，两格一起空着。
    let total = Some(disk.total_space()).filter(|bytes| *bytes > 0);
    Volume {
        filesystem: disk
            .file_system()
            .to_str()
            .filter(|name| !name.is_empty())
            .map(filesystem_label),
        total,
        available: total.map(|_| disk.available_space()),
        removable: disk.is_removable(),
    }
}

/// 系统报的文件系统类型名换成人认得的写法：macOS 报 `exfat` / `msdos`，Linux 报 `exfat` / `vfat`，Windows 报 `exFAT` / `FAT32`。
///
/// `msdos` 与 `vfat` 是 FAT 这一族（FAT12 / 16 / 32 都报它）；写成 `FAT32` 是因为 4 GB 以上的 SD 卡出厂不是 FAT32 就是
/// exFAT，FAT16 只剩 2 GB 以下的老卡。认不出的原样交回。
fn filesystem_label(raw: &str) -> String {
    match raw.to_ascii_lowercase().as_str() {
        "exfat" => "exFAT".to_string(),
        "msdos" | "vfat" | "fat32" => "FAT32".to_string(),
        "apfs" => "APFS".to_string(),
        "hfs" => "HFS+".to_string(),
        "ntfs" | "ntfs3" => "NTFS".to_string(),
        _ => raw.to_string(),
    }
}
