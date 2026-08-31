//! 断点：还没扫的目录，以及这次扫描的代号。
//!
//! 断点里**不再装统计**——统计的家是中立库（ADR-0001），扫到的每一条记录都已经落进
//! SQLite 了。断点只回答一个问题：接着从哪儿扫。于是它从「10T 库可能几 MB」
//! 缩回到一份目录清单。
//!
//! 中断可续跑的代价必须落在**工作目录**而不是主库上——主库只读（ADR-0004），
//! 而且外接盘不常挂载，中立库与断点都存在本机（ADR-0009）。
//!
//! 一致性靠两条规则守住：
//!
//! - 断点里的 `pending` 同时包含队列里的和**正在扫的**目录。正在扫的那些结果被整份
//!   丢掉，续跑时重扫一遍即可——重做一点点，好过少算一个目录。
//! - 续跑沿用同一个 `scan`。删除是「这次扫描没见到的」，跨几次续跑累计起来才算数；
//!   中断的扫描绝不 sweep。

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 断点文件的格式版本。结构变了就加 1，读到对不上的版本直接从头扫。
///
/// 3：统计搬进中立库，断点只剩待扫目录、扫描代号与累计耗时。
pub const FORMAT_VERSION: u32 = 3;

/// 断点读写过程中的错误。
#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    /// 读写失败。
    #[error("断点文件读写失败：{path}（{source}）")]
    Io {
        /// 出问题的路径。
        path: String,
        /// 底层错误。
        source: io::Error,
    },
    /// 内容解析失败。
    #[error("断点文件解析失败：{path}（{source}）")]
    Parse {
        /// 出问题的路径。
        path: String,
        /// 底层错误。
        source: serde_json::Error,
    },
    /// 版本对不上。
    #[error("断点文件版本是 {found}，本程序认得的是 {expected}")]
    Version {
        /// 文件里的版本。
        found: u32,
        /// 本程序的版本。
        expected: u32,
    },
    /// 扫描根对不上。
    #[error("断点记录的扫描根是 {stored}，与这次要扫的 {current} 不是同一个")]
    RootMismatch {
        /// 断点里的根。
        stored: String,
        /// 这次的根。
        current: String,
    },
}

/// 路径的可存盘表示。
///
/// 路径不保证是 UTF-8：Unix 上是任意字节，Windows 上是任意 UTF-16 码元。
/// 存成有损字符串会让续跑找不到那个目录，因此非 UTF-8 的路径原样存码元。
/// 两个平台共用同一个 JSON 结构，只有转换函数分平台。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum PathRepr {
    /// 能无损转成 UTF-8 的路径。
    Text(String),
    /// 非 UTF-8 路径，按平台原生码元存。
    Raw(Vec<u32>),
}

#[cfg(unix)]
fn encode_path(path: &Path) -> PathRepr {
    use std::os::unix::ffi::OsStrExt;
    match path.to_str() {
        Some(text) => PathRepr::Text(text.to_string()),
        None => PathRepr::Raw(
            path.as_os_str()
                .as_bytes()
                .iter()
                .map(|b| u32::from(*b))
                .collect(),
        ),
    }
}

#[cfg(unix)]
fn decode_path(repr: &PathRepr) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    match repr {
        PathRepr::Text(text) => PathBuf::from(text),
        PathRepr::Raw(units) => {
            let bytes: Vec<u8> = units
                .iter()
                .map(|u| u8::try_from(*u).unwrap_or(b'?'))
                .collect();
            PathBuf::from(OsString::from_vec(bytes))
        }
    }
}

#[cfg(windows)]
fn encode_path(path: &Path) -> PathRepr {
    use std::os::windows::ffi::OsStrExt;
    match path.to_str() {
        Some(text) => PathRepr::Text(text.to_string()),
        None => PathRepr::Raw(path.as_os_str().encode_wide().map(u32::from).collect()),
    }
}

#[cfg(windows)]
fn decode_path(repr: &PathRepr) -> PathBuf {
    use std::os::windows::ffi::OsStringExt;
    match repr {
        PathRepr::Text(text) => PathBuf::from(text),
        PathRepr::Raw(units) => {
            let wide: Vec<u16> = units
                .iter()
                .map(|u| u16::try_from(*u).unwrap_or(u16::from(b'?')))
                .collect();
            PathBuf::from(OsString::from_wide(&wide))
        }
    }
}

#[cfg(not(any(unix, windows)))]
fn encode_path(path: &Path) -> PathRepr {
    PathRepr::Text(path.to_string_lossy().into_owned())
}

#[cfg(not(any(unix, windows)))]
fn decode_path(repr: &PathRepr) -> PathBuf {
    match repr {
        PathRepr::Text(text) => PathBuf::from(text),
        PathRepr::Raw(units) => {
            let text: String = units.iter().filter_map(|u| char::from_u32(*u)).collect();
            PathBuf::from(text)
        }
    }
}

/// 一次扫描的断点。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// 格式版本。
    pub format_version: u32,
    /// 扫描根。
    pub root: PathRepr,
    /// 每类文件的抽样配额，续跑时沿用。
    pub samples_per_class: usize,
    /// 还没扫完的目录：队列里的加上中断时正在扫的。
    pub pending: Vec<PathRepr>,
    /// 这次扫描在中立库里的代号。续跑必须沿用它，否则续跑之前扫到的记录
    /// 会被当成「这次没见到」而在收尾时删掉。
    pub scan: i64,
    /// 累计耗时，含此前几次续跑。
    pub elapsed_ms: u64,
}

impl Checkpoint {
    /// 从内存状态建一个断点。
    #[must_use]
    pub fn new(
        root: &Path,
        samples_per_class: usize,
        pending: &[PathBuf],
        scan: i64,
        elapsed_ms: u64,
    ) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            root: encode_path(root),
            samples_per_class,
            pending: pending.iter().map(|p| encode_path(p)).collect(),
            scan,
            elapsed_ms,
        }
    }

    /// 断点记录的扫描根。
    #[must_use]
    pub fn root(&self) -> PathBuf {
        decode_path(&self.root)
    }

    /// 还没扫的目录。
    #[must_use]
    pub fn pending(&self) -> Vec<PathBuf> {
        self.pending.iter().map(decode_path).collect()
    }

    /// 原子地写盘：先写临时文件再改名，中途断电不会留下半个断点。
    ///
    /// # Errors
    /// 建目录、写文件或改名失败时返回错误。
    pub fn save(&self, path: &Path) -> Result<(), CheckpointError> {
        let io_err = |path: &Path, source: io::Error| CheckpointError::Io {
            path: crate::path::display(path),
            source,
        };
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
        }
        let text = serde_json::to_vec(self).map_err(|source| CheckpointError::Parse {
            path: crate::path::display(path),
            source,
        })?;
        let mut temp = path.as_os_str().to_os_string();
        temp.push(".tmp");
        let temp = PathBuf::from(temp);
        fs::write(&temp, &text).map_err(|e| io_err(&temp, e))?;
        fs::rename(&temp, path).map_err(|e| io_err(path, e))?;
        Ok(())
    }

    /// 从盘上读一个断点，并核对版本与扫描根。
    ///
    /// # Errors
    /// 文件不存在、版本对不上、扫描根对不上或内容坏掉时返回错误。
    pub fn load(path: &Path, expected_root: &Path) -> Result<Self, CheckpointError> {
        let text = fs::read(path).map_err(|source| CheckpointError::Io {
            path: crate::path::display(path),
            source,
        })?;
        let checkpoint: Self =
            serde_json::from_slice(&text).map_err(|source| CheckpointError::Parse {
                path: crate::path::display(path),
                source,
            })?;
        if checkpoint.format_version != FORMAT_VERSION {
            return Err(CheckpointError::Version {
                found: checkpoint.format_version,
                expected: FORMAT_VERSION,
            });
        }
        let stored = checkpoint.root();
        if stored != expected_root {
            return Err(CheckpointError::RootMismatch {
                stored: crate::path::display(&stored),
                current: crate::path::display(expected_root),
            });
        }
        Ok(checkpoint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 断点能原样存回来() {
        let dir = crate::testing::temp_dir("checkpoint");
        let file = dir.path().join("scan").join("checkpoint.json");
        let pending = vec![PathBuf::from("/lib/FC"), PathBuf::from("/lib/PS1/游戏")];
        let checkpoint = Checkpoint::new(Path::new("/lib"), 32, &pending, 7, 1234);
        checkpoint.save(&file).expect("能存盘");

        let back = Checkpoint::load(&file, Path::new("/lib")).expect("能读回");
        assert_eq!(back, checkpoint);
        assert_eq!(back.pending(), pending);
        assert_eq!(back.scan, 7, "续跑要沿用同一个扫描代号");
    }

    #[test]
    fn 扫描根对不上时拒绝续跑() {
        let dir = crate::testing::temp_dir("checkpoint");
        let file = dir.path().join("checkpoint.json");
        Checkpoint::new(Path::new("/lib"), 32, &[], 1, 0)
            .save(&file)
            .expect("能存盘");
        let err = Checkpoint::load(&file, Path::new("/other")).expect_err("必须报错");
        assert!(matches!(err, CheckpointError::RootMismatch { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn 非_utf8_路径也能原样存回来() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let weird = PathBuf::from(OsStr::from_bytes(b"/lib/\xff\xfe/game"));
        let repr = encode_path(&weird);
        assert!(matches!(repr, PathRepr::Raw(_)));
        assert_eq!(decode_path(&repr), weird);
    }
}
