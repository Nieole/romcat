//! 内存里的 [`LibraryFs`] 实现，供测试使用。
//!
//! 有了它，遍历、归类、增量判断、断点续跑这些逻辑不需要真实磁盘就能测——也不需要那块
//! 现在没挂载的 10T 外置盘。
//!
//! 它刻意能造出主库上真实存在的两种「读不到」，因为两者在扫描里的结论完全不同：
//!
//! - [`MemFs::unreadable_meta`]：`readdir` 列得出名字，元数据读不到。这是 ADR-0021 的
//!   **第三态**，既不算已变也不算已删。
//! - [`MemFs::unreadable_content`]：元数据正常，但打开读头部会失败。它只影响头部抽样。

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::{DirEntry, EntryKind, EntryMeta, LibraryFs, ReadSeek};

#[derive(Debug, Clone)]
enum Node {
    Dir,
    File {
        data: Vec<u8>,
        len: u64,
        modified: SystemTime,
    },
    Symlink,
    /// 元数据正常，内容读不动。
    UnreadableContent {
        len: u64,
        modified: SystemTime,
    },
    /// 名字列得出，元数据读不到（ADR-0021 的第三态）。
    UnreadableMeta,
    /// 在上级目录里列得出来、自己却列不开的目录。
    UnlistableDir,
}

/// 内存文件系统。路径一律用绝对路径。
#[derive(Debug, Default, Clone)]
pub struct MemFs {
    nodes: BTreeMap<PathBuf, Node>,
}

/// 文件默认的修改时间。测试要造「内容变了」时用 [`MemFs::touch`] 往后拨。
const DEFAULT_MTIME: SystemTime = SystemTime::UNIX_EPOCH;

impl MemFs {
    /// 新建一个空的内存文件系统。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 建一个目录（连同它的所有上级目录）。
    pub fn dir(&mut self, path: impl AsRef<Path>) -> &mut Self {
        let path = path.as_ref().to_path_buf();
        let mut current = Some(path.as_path());
        while let Some(dir) = current {
            self.nodes.entry(dir.to_path_buf()).or_insert(Node::Dir);
            current = dir.parent();
        }
        self
    }

    /// 建一个文件，内容为 `data`。
    pub fn file(&mut self, path: impl AsRef<Path>, data: impl Into<Vec<u8>>) -> &mut Self {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            self.dir(parent);
        }
        let data = data.into();
        let len = data.len() as u64;
        self.nodes.insert(
            path,
            Node::File {
                data,
                len,
                modified: DEFAULT_MTIME,
            },
        );
        self
    }

    /// 把一个文件的修改时间往后拨 `seconds` 秒，大小不变。
    ///
    /// 这是增量扫描最该防的一种变化：**大小一样但内容变了**。只比大小的话它会被漏掉。
    ///
    /// # Panics
    /// 路径不是一个普通文件时 panic——测试写错了该立刻知道。
    pub fn touch(&mut self, path: impl AsRef<Path>, seconds: u64) -> &mut Self {
        let path = path.as_ref().to_path_buf();
        match self.nodes.get_mut(&path) {
            Some(Node::File { modified, .. } | Node::UnreadableContent { modified, .. }) => {
                *modified += Duration::from_secs(seconds);
            }
            _ => panic!("touch 的目标不是文件：{}", path.display()),
        }
        self
    }

    /// 删掉一个条目。
    pub fn remove(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.nodes.remove(path.as_ref());
        self
    }

    /// 建一个符号链接。扫描器不跟随它，因此不需要记目标。
    pub fn symlink(&mut self, path: impl AsRef<Path>) -> &mut Self {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            self.dir(parent);
        }
        self.nodes.insert(path, Node::Symlink);
        self
    }

    /// 建一个元数据正常、但内容读不动的文件，用来测「读失败要被计入报告而不是中断扫描」。
    pub fn unreadable_content(&mut self, path: impl AsRef<Path>, len: u64) -> &mut Self {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            self.dir(parent);
        }
        self.nodes.insert(
            path,
            Node::UnreadableContent {
                len,
                modified: DEFAULT_MTIME,
            },
        );
        self
    }

    /// 建一个列不开的目录：上级列得出它，`read_dir` 它自己会失败。
    ///
    /// 用来测「列不开的目录下面那些记录不许被当成已删除」——那正是 ADR-0021
    /// 点名的陷阱：扫描把「看不见」解释成「不存在」。
    pub fn unlistable_dir(&mut self, path: impl AsRef<Path>) -> &mut Self {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            self.dir(parent);
        }
        self.nodes.insert(path, Node::UnlistableDir);
        self
    }

    /// 建一个**元数据读不到**的文件：名字列得出，`stat` 失败（ADR-0021 的第三态）。
    ///
    /// 主库那块 NTFS 盘在 macOS 上实测有 4,085 个这样的文件。
    pub fn unreadable_meta(&mut self, path: impl AsRef<Path>) -> &mut Self {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            self.dir(parent);
        }
        self.nodes.insert(path, Node::UnreadableMeta);
        self
    }
}

fn denied(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("拒绝访问：{}", path.display()),
    )
}

fn not_found(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("路径不存在：{}", path.display()),
    )
}

impl LibraryFs for MemFs {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        if self.nodes.contains_key(path) {
            Ok(path.to_path_buf())
        } else {
            Err(not_found(path))
        }
    }

    fn read_dir(&self, dir: &Path) -> io::Result<Vec<DirEntry>> {
        match self.nodes.get(dir) {
            Some(Node::Dir) => {}
            Some(Node::UnlistableDir) => return Err(denied(dir)),
            Some(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::NotADirectory,
                    format!("不是目录：{}", dir.display()),
                ));
            }
            None => return Err(not_found(dir)),
        }
        let mut out = Vec::new();
        for (path, node) in &self.nodes {
            if path.parent() != Some(dir) {
                continue;
            }
            let known = |len: u64, modified: SystemTime| EntryMeta::Known {
                len,
                modified: Some(modified),
            };
            let (kind, meta) = match node {
                Node::Dir => (EntryKind::Dir, known(0, DEFAULT_MTIME)),
                Node::File { len, modified, .. } => (EntryKind::File, known(*len, *modified)),
                Node::Symlink => (EntryKind::Symlink, known(0, DEFAULT_MTIME)),
                Node::UnreadableContent { len, modified } => {
                    (EntryKind::File, known(*len, *modified))
                }
                Node::UnreadableMeta => (EntryKind::File, EntryMeta::Unreadable),
                Node::UnlistableDir => (EntryKind::Dir, known(0, DEFAULT_MTIME)),
            };
            out.push(DirEntry {
                path: path.clone(),
                kind,
                meta,
            });
        }
        Ok(out)
    }

    fn read_head(&self, file: &Path, limit: usize) -> io::Result<Vec<u8>> {
        match self.nodes.get(file) {
            Some(Node::File { data, .. }) => Ok(data[..data.len().min(limit)].to_vec()),
            Some(_) => Err(denied(file)),
            None => Err(not_found(file)),
        }
    }

    fn read_tail(&self, file: &Path, limit: usize) -> io::Result<Vec<u8>> {
        match self.nodes.get(file) {
            Some(Node::File { data, .. }) => {
                let start = data.len().saturating_sub(limit);
                Ok(data[start..].to_vec())
            }
            Some(_) => Err(denied(file)),
            None => Err(not_found(file)),
        }
    }

    fn open(&self, file: &Path) -> io::Result<Box<dyn ReadSeek + '_>> {
        match self.nodes.get(file) {
            // 拷一份而不是借出去：内存主库在测试里会被继续改，借出去的句柄会把
            // `&mut MemFs` 锁死，而真实实现返回的本来就是一个独立的文件句柄。
            Some(Node::File { data, .. }) => Ok(Box::new(io::Cursor::new(data.clone()))),
            Some(_) => Err(denied(file)),
            None => Err(not_found(file)),
        }
    }
}
