//! 真实磁盘上的 [`LibraryFs`] 实现。

use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::{DirEntry, EntryKind, EntryMeta, LibraryFs, ReadSeek};
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
        //
        // **交出来的这一条不许直接拿去与库里存的路径比。** `library_root.path` 存的是
        // `path::display` 剥掉前缀之后的形态，而 `Path` 的 `==` 与 `starts_with` 按分量
        // 比，`Prefix::VerbatimDisk` 与 `Prefix::Disk` 不是同一个分量——比较一律走
        // `path::is_same_place` / `path::is_inside_place`，它们先把两边折齐。
        let absolute = fs::canonicalize(long_path(path).as_ref())?;
        Ok(long_path(&absolute).into_owned())
    }

    /// 列一层目录。
    ///
    /// # 禁止改用 `getattrlistbulk`（ADR-0021）
    ///
    /// `getattrlistbulk` 是 macOS 上批量取目录项元数据的接口，比这里「逐个 `metadata()`」
    /// 快得多，是将来给扫描提速时最自然的第一选择。**不要换。**
    ///
    /// 实测：主库里有 4,085 个文件在 fskit 的只读 NTFS 驱动下 `stat` 失败，而
    /// `getattrlistbulk` **根本不列出它们**——23 个受影响文件里列出 0 个。于是这批文件
    /// 会从「读不到」变成「不存在」，而扫描把不存在解释为**已删除**：一次提速改动会
    /// 静默地让 4,085 个文件从中立库里消失。
    ///
    /// 这个陷阱在代码里完全看不出来——`getattrlistbulk` 会成功返回，只是少了东西。
    /// 要提速，先证明新接口列得出 `readdir` 列得出的每一条。
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
            // 读不到就如实说读不到，绝不退化成 `len = 0`——那会和库里 4,317 个
            // 真正的空文件混在一起（ADR-0021）。
            let meta = match meta {
                Ok(meta) => EntryMeta::Known {
                    len: if kind == EntryKind::File {
                        meta.len()
                    } else {
                        0
                    },
                    modified: meta.modified().ok(),
                },
                Err(_) => EntryMeta::Unreadable,
            };
            out.push(DirEntry {
                path: entry.path(),
                kind,
                meta,
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

    fn open(&self, file: &Path) -> io::Result<Box<dyn ReadSeek + '_>> {
        Ok(Box::new(fs::File::open(long_path(file).as_ref())?))
    }
}
