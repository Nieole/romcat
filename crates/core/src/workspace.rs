//! 工作目录：中立库与断点住在这里，将来的媒体池也是。
//!
//! **它必须在本机而不是外置盘上**（ADR-0009）。外置盘不常挂载，中立库若跟着盘走，
//! 盘不在时连浏览元数据都做不到——而扫描是唯一真正需要盘在位的操作。
//!
//! 一个主库一份文件：中立库的键是**相对**主库根的路径（ADR-0020），两个主库的记录
//! 混进同一张表会直接撞车。文件名里既留原目录名（人能认出是哪块盘）又带路径的哈希
//! （两块盘的最后一级恰好同名时不会互相覆盖）。

use std::env;
use std::path::{Path, PathBuf};

/// 默认工作目录。
///
/// 一条链跨平台通用、没有 `cfg` 分支：`$ROMCAT_HOME` 优先，其次各平台的数据目录。
/// Windows 走 `%APPDATA%`，那是主力机（ADR-0018）。
#[must_use]
pub fn default_dir() -> PathBuf {
    for key in ["ROMCAT_HOME", "XDG_DATA_HOME", "APPDATA"] {
        if let Some(value) = env::var_os(key) {
            let path = PathBuf::from(value);
            if !path.as_os_str().is_empty() {
                return if key == "ROMCAT_HOME" {
                    path
                } else {
                    path.join("romcat")
                };
            }
        }
    }
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("romcat");
    }
    env::temp_dir().join("romcat")
}

/// 一个主库在工作目录里的短名：末级目录名加路径哈希。
#[must_use]
pub fn slug(root: &Path) -> String {
    let text = root.to_string_lossy();
    // FNV-1a。这里只要「不同路径大概率不同名」，不需要密码学强度。
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    let name: String = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "library".to_string())
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(24)
        .collect();
    let name = if name.is_empty() {
        "library".to_string()
    } else {
        name
    };
    format!("{name}-{hash:016x}")
}

/// 某个主库的**中立库**文件。
#[must_use]
pub fn catalog_path(workspace: &Path, root: &Path) -> PathBuf {
    workspace
        .join("catalog")
        .join(format!("{}.sqlite3", slug(root)))
}

/// 某个主库的断点文件。
#[must_use]
pub fn checkpoint_path(workspace: &Path, root: &Path) -> PathBuf {
    workspace
        .join("scans")
        .join(format!("{}.checkpoint.json", slug(root)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 不同主库的中立库与断点互不覆盖() {
        let workspace = PathBuf::from("/work");
        let a = Path::new("/Volumes/ROMs");
        let b = Path::new("/Volumes/ROMs2");
        assert_ne!(catalog_path(&workspace, a), catalog_path(&workspace, b));
        assert_ne!(
            checkpoint_path(&workspace, a),
            checkpoint_path(&workspace, b)
        );
    }

    #[test]
    fn 末级同名的两块盘也分得开() {
        let workspace = PathBuf::from("/work");
        let a = catalog_path(&workspace, Path::new("/Volumes/甲/Game"));
        let b = catalog_path(&workspace, Path::new("/Volumes/乙/Game"));
        assert_ne!(a, b);
        assert!(
            a.to_string_lossy().contains("Game"),
            "名字里要认得出是哪个目录"
        );
    }

    #[test]
    fn 中立库与断点都不落在主库里() {
        // 外置盘不常挂载，中立库跟着盘走的话盘不在时连浏览都做不到（ADR-0009）；
        // 何况主库只读（ADR-0004）。
        let root = Path::new("/Volumes/ROMs");
        let workspace = PathBuf::from("/work");
        assert!(!catalog_path(&workspace, root).starts_with(root));
        assert!(!checkpoint_path(&workspace, root).starts_with(root));
    }
}
