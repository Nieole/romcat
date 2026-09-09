//! 测试支持：临时目录、样本字节、目标设备的假视图。
//!
//! 只为测试存在，但它是普通模块而不是 `#[cfg(test)]`——集成测试与后续票的测试都要用。

pub mod cart;
pub mod container;
pub mod disc;
pub mod sample;
pub mod switch;
pub mod target;

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

/// 现建一份**结构版本对不上**的中立库。
///
/// 用户机器上真有三份这样的库（ADR-0023：三个工作目录各一份，版本 4，而本程序认 7），
/// 而**照列不误**正是**开场**那一屏的判据——从列表里静静消失才是最难查的那种错。
/// 造得出这样一份，那条路才验得了。
///
/// 造法是**先按当前结构建出来，再把版本那一行改掉**：真正的旧库表结构也旧，
/// 而认得出「这份库对不上」的那一步在读任何一张表之前就做完了
/// （[`Catalog::open_read_only`](crate::catalog::Catalog::open_read_only) 只核对
/// `meta` 里那一行），所以这份替身在这条路上与真旧库一模一样。
///
/// # Panics
/// 建不出、或者改不动那一行时当场 panic——夹具搭不起来，底下那条测试验的就不是它以为
/// 的那件事了。
pub fn catalog_at_version(file: &Path, version: u32) {
    use rusqlite::{Connection, params};

    drop(crate::catalog::Catalog::open(file).expect("能建中立库"));
    let conn = Connection::open(file).expect("能再打开那个文件");
    let 改了 = conn
        .execute(
            "UPDATE meta SET value = ?1 WHERE key = 'schema_version'",
            params![version.to_string()],
        )
        .expect("改得动版本那一行");
    assert_eq!(改了, 1, "版本那一行没改到");
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
