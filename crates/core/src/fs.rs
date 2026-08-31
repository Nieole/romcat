//! 主库的只读接缝。
//!
//! **这个 trait 只有读操作。** 主库是 10T 不可再生的资源，工具对它一个字节都不改
//! （ADR-0004）。把只读做成类型层面的事实，好过在每个实现里记得别写——扫描器只能
//! 通过 [`LibraryFs`] 接触主库，而 [`LibraryFs`] 根本没有写的办法。
//!
//! 这也是把 IO 挤到边缘的那道接缝：遍历、归类、统计全是纯逻辑，测试挂在纯逻辑上，
//! 用 [`mem::MemFs`] 完全在内存里跑。

pub mod mem;
pub mod real;

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub use mem::MemFs;
pub use real::RealFs;

/// 目录项的类型。符号链接不跟随，因此它是独立的一类而不是 `File` 或 `Dir`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// 普通文件。
    File,
    /// 目录。
    Dir,
    /// 符号链接。不跟随——跟随会引入环，也会让同一份内容被统计两次。
    Symlink,
    /// 其余（设备、管道等）。
    Other,
}

/// 一条目录项的元数据。
///
/// **不可读是第三态**（ADR-0021）。主库那块 NTFS 盘上实测有 4,085 个文件（256,128 个
/// 里的 1.59%）在 macOS 的 fskit 只读驱动下连元数据都取不到——`stat` / `lstat` /
/// `open` / `read` 全部失败（`ENOTSUP`），**只有 `readdir` 有效**：名字拿得到，其余
/// 一无所知。
///
/// 用枚举而不是两个 `Option`，是因为「大小未知」和「大小为零」必须分得开：库里另有
/// **4,317 个真正的空文件**。混在一起会让体检报告的空文件数虚高，也会让增量扫描把
/// 「读不到」误当成「变成了 0 字节」。这里用类型逼调用方显式处理这一态。
///
/// 详见 `docs/research/ntfs-mtime-precision.md` 第 6.2 节与 ADR-0021。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryMeta {
    /// 元数据读到了。
    Known {
        /// 文件字节数；目录与链接为 0。
        len: u64,
        /// 最后修改时间；驱动给不出时为 `None`。
        ///
        /// 它是增量扫描三元组 `(路径, 大小, 修改时间)` 的第三项。NTFS 的 100 纳秒刻度
        /// 完整透出、APFS 是 1 纳秒、且不存在时区偏差，这一侧已实测可用
        /// （`docs/research/ntfs-mtime-precision.md`）。
        modified: Option<SystemTime>,
    },
    /// 元数据读不到。**不是**「大小为 0」，也**不是**「不存在」。
    Unreadable,
}

impl EntryMeta {
    /// 字节数；读不到时是 `None`（不是 `Some(0)`）。
    #[must_use]
    pub fn byte_len(&self) -> Option<u64> {
        match self {
            Self::Known { len, .. } => Some(*len),
            Self::Unreadable => None,
        }
    }

    /// 最后修改时间；读不到时是 `None`。
    #[must_use]
    pub fn modified(&self) -> Option<SystemTime> {
        match self {
            Self::Known { modified, .. } => *modified,
            Self::Unreadable => None,
        }
    }

    /// 元数据是否读不到。
    #[must_use]
    pub fn is_unreadable(&self) -> bool {
        matches!(self, Self::Unreadable)
    }
}

/// 一条目录项，带扫描需要的全部元数据。
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// 完整路径，**系统给出的原始形式**。读盘用它；入库与比较要先过
    /// [`path::catalog_key`](crate::path::catalog_key) 折成 NFC 键（ADR-0020）。
    pub path: PathBuf,
    /// 目录项类型。
    pub kind: EntryKind,
    /// 元数据，含**不可读**这一第三态。
    pub meta: EntryMeta,
}

/// 主库的只读视图。
pub trait LibraryFs: Sync {
    /// 规范化扫描根：转成绝对路径，并在 Windows 上加 `\\?\` 扩展长度前缀。
    ///
    /// 之后所有子路径都从这个结果拼出来，于是整条遍历天然是长路径安全的。
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;

    /// 列出一层目录，不递归、不跟随符号链接。
    fn read_dir(&self, dir: &Path) -> io::Result<Vec<DirEntry>>;

    /// 读取文件头部至多 `limit` 字节。文件比 `limit` 短时返回实际长度。
    fn read_head(&self, file: &Path, limit: usize) -> io::Result<Vec<u8>>;

    /// 读取文件尾部至多 `limit` 字节。
    ///
    /// WS / WSC 的内部头在文件末尾，没有这个方法就只能把它们排除在抽样之外。
    fn read_tail(&self, file: &Path, limit: usize) -> io::Result<Vec<u8>>;
}
