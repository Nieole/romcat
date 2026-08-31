//! 真实磁盘上的 [`LibraryFs`] 实现。

use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::{DirEntry, EntryKind, LibraryFs};
use crate::path::long_path;

/// 真实文件系统。只打开文件读，从不创建、修改或删除任何东西。
#[derive(Debug, Default, Clone, Copy)]
pub struct RealFs;

impl RealFs {
    /// 新建一个真实文件系统视图。
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl LibraryFs for RealFs {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        // Windows 上 `canonicalize` 本身就返回 `\\?\` 形式；其他平台返回绝对路径。
        // 再过一道 `long_path` 是为了不依赖标准库的这个实现细节。
        let absolute = fs::canonicalize(long_path(path).as_ref())?;
        Ok(long_path(&absolute).into_owned())
    }

    fn read_dir(&self, dir: &Path) -> io::Result<Vec<DirEntry>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(long_path(dir).as_ref())? {
            let entry = entry?;
            // `DirEntry::metadata` 不跟随符号链接，正是这里要的语义。
            let meta = entry.metadata();
            let file_type = entry.file_type()?;
            let kind = if file_type.is_symlink() {
                EntryKind::Symlink
            } else if file_type.is_dir() {
                EntryKind::Dir
            } else if file_type.is_file() {
                EntryKind::File
            } else {
                EntryKind::Other
            };
            let (len, modified) = match &meta {
                Ok(meta) => (meta.len(), meta.modified().ok()),
                Err(_) => (0, None),
            };
            out.push(DirEntry {
                path: entry.path(),
                kind,
                len: if kind == EntryKind::File { len } else { 0 },
                modified,
            });
        }
        Ok(out)
    }

    fn read_head(&self, file: &Path, limit: usize) -> io::Result<Vec<u8>> {
        let handle = fs::File::open(long_path(file).as_ref())?;
        let mut buf = Vec::new();
        handle.take(limit as u64).read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn read_tail(&self, file: &Path, limit: usize) -> io::Result<Vec<u8>> {
        let mut handle = fs::File::open(long_path(file).as_ref())?;
        let len = handle.metadata()?.len();
        let want = u64::try_from(limit).unwrap_or(u64::MAX).min(len);
        handle.seek(SeekFrom::Start(len - want))?;
        let mut buf = Vec::new();
        handle.take(want).read_to_end(&mut buf)?;
        Ok(buf)
    }
}
