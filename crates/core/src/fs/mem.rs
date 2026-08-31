//! 内存里的 [`LibraryFs`] 实现，供测试使用。
//!
//! 有了它，遍历、归类、断点续跑这些逻辑不需要真实磁盘就能测——也不需要那块
//! 现在没挂载的 10T 外置盘。

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::{DirEntry, EntryKind, LibraryFs};

#[derive(Debug, Clone)]
enum Node {
    Dir,
    File { data: Vec<u8>, len: u64 },
    Symlink,
    Unreadable,
}

/// 内存文件系统。路径一律用绝对路径。
#[derive(Debug, Default, Clone)]
pub struct MemFs {
    nodes: BTreeMap<PathBuf, Node>,
}

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
        self.nodes.insert(path, Node::File { data, len });
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

    /// 建一个读不动的文件，用来测「读失败要被计入报告而不是中断扫描」。
    pub fn unreadable_file(&mut self, path: impl AsRef<Path>) -> &mut Self {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            self.dir(parent);
        }
        self.nodes.insert(path, Node::Unreadable);
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
            let (kind, len) = match node {
                Node::Dir => (EntryKind::Dir, 0),
                Node::File { len, .. } => (EntryKind::File, *len),
                Node::Symlink => (EntryKind::Symlink, 0),
                Node::Unreadable => (EntryKind::File, 0),
            };
            out.push(DirEntry {
                path: path.clone(),
                kind,
                len,
                modified: Some(SystemTime::UNIX_EPOCH),
            });
        }
        Ok(out)
    }

    fn read_head(&self, file: &Path, limit: usize) -> io::Result<Vec<u8>> {
        match self.nodes.get(file) {
            Some(Node::File { data, .. }) => Ok(data[..data.len().min(limit)].to_vec()),
            Some(Node::Unreadable) => Err(denied(file)),
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
            Some(Node::Unreadable) => Err(denied(file)),
            Some(_) => Err(denied(file)),
            None => Err(not_found(file)),
        }
    }
}
