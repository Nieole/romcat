//! 测试支持：临时目录。
//!
//! 只为测试存在，但它是普通模块而不是 `#[cfg(test)]`——集成测试与后续票的测试都要用。

pub mod cart;
pub mod container;
pub mod disc;
pub mod sample;
pub mod switch;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// 一个用完即删的临时目录。
#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// 目录路径。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// 建一个临时目录，名字里带上 `tag` 便于出问题时辨认。
///
/// # Panics
/// 建不出目录时直接 panic——测试环境连临时目录都写不了，继续跑没有意义。
#[must_use]
pub fn temp_dir(tag: &str) -> TempDir {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = env::temp_dir().join(format!(
        "romcat-{tag}-{}-{nanos}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("能建临时目录");
    TempDir { path }
}
