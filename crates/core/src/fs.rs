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

/// 一条目录项，带扫描需要的全部元数据。
///
/// `modified` 在库体检里用不上，但它是票 02 增量扫描的判据
/// （`(路径, 大小, mtime)`），而列目录时它本来就是顺带拿到的。
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// 完整路径。
    pub path: PathBuf,
    /// 目录项类型。
    pub kind: EntryKind,
    /// 文件字节数；目录与链接为 0。
    pub len: u64,
    /// 最后修改时间，取不到时为 `None`。
    pub modified: Option<SystemTime>,
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
