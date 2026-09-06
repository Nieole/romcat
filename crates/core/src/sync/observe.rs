//! 把目标设备折成**实际状态**：三方对比里的第三方。
//!
//! ## 为什么这一步做得起来
//!
//! ADR-0015 定死了目标设备**走读卡器**：SD 卡挂成普通盘。那是唯一能让工具**验证目标
//! 真实状态**的连接方式——MTP 上 `stat` 极贵、无随机访问，只能盲信清单，
//! 「报告目标上的意外变化」根本实现不了。于是这里就是一次普通的只读遍历。
//!
//! ## 走的是主库那道只读接缝
//!
//! 遍历经 [`LibraryFs`]，而这个 trait **根本没有写的办法**（ADR-0004 的做法）。
//! 排计划这一路上一个字节都写不出去，是类型保证的，不是纪律。测试因此也能拿
//! [`MemFs`](crate::fs::MemFs) 在内存里造出整张卡，包括「元数据读不到」那一态。
//!
//! ## 卡不在位是错误，不是「空目标」
//!
//! 把「盘没插」读成「目标上什么都没有」，会让计划变成「清单里的每一条都意外消失了、
//! 期望里的每一条都要重传」——一份灾难性的预览。所以根目录不在时**直接失败**。

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use crate::catalog::mtime_ns;
use crate::fs::{EntryKind, EntryMeta, LibraryFs};
use crate::path;

use super::{Stamp, TargetFile, TargetState};

/// 目标设备看不成的原因。
#[derive(Debug, thiserror::Error)]
pub enum ObserveError {
    /// 子库根不在位。**卡没插上就该在这里停住**，而不是排出一份「全删全传」的计划。
    #[error(
        "目标不在位：{path}（{source}）\n插上读卡器，或者 `romcat sublibrary set` 改一下目标路径。"
    )]
    Absent {
        /// 子库根。
        path: String,
        /// 底层错误。
        source: io::Error,
    },
    /// 子库根列不开。
    #[error("目标根目录列不开：{path}（{source}）")]
    Unreadable {
        /// 子库根。
        path: String,
        /// 底层错误。
        source: io::Error,
    },
}

/// 只读地看一遍目标设备，折出**实际状态**。
///
/// 三条处置：
///
/// - **目录递归下去**，本身不入账：计划的单位是文件。
/// - **元数据读不到的照样入账**，戳是 `None`（ADR-0021 的第三态）。既不算在、
///   也不算不在，于是计划器一律不动它。
/// - **符号链接与其它类型也入账、戳也是 `None`**：「有东西挡在这儿、但说不清是什么」
///   与「说不清它是不是我们放的那份」在处置上是同一件事——都不动。
///
/// 列不开的目录数出来即可：那一枝底下的东西全部说不清，而说不清的一律不碰。
///
/// ## 顺手把两件**目录**的事也带回来
///
/// - **走过的每一个目录**（[`TargetState::dirs`]），键是 `read_dir` 给的**真名**折成
///   NFC 的那一条。落点的目录段要拿它去折齐（[`align`](crate::sync::align)）——
///   卡上那个 `gb/` 与我们键里的 `GB/`，在不分大小写的目标上是同一个目录。
/// - **这个文件系统认不认大小写**（[`TargetState::case_insensitive`]）。走一遍本来就
///   把每一层都列了，同一层里两个名字折起来一样就当场证完；证不出来才多问一次
///   （[`fs::case_insensitive`](crate::fs::case_insensitive)）。仍然只读。
///
/// # Errors
/// 子库根不在位或列不开时返回错误。
pub fn observe(fs: &dyn LibraryFs, root: &Path) -> Result<TargetState, ObserveError> {
    let root = fs
        .canonicalize(root)
        .map_err(|source| ObserveError::Absent {
            path: path::display(root),
            source,
        })?;
    let top = fs
        .read_dir(&root)
        .map_err(|source| ObserveError::Unreadable {
            path: path::display(&root),
            source,
        })?;

    let mut out = TargetState::default();
    let mut stack: Vec<PathBuf> = Vec::new();
    let mut pending = top;
    // 同一层里两个名字折起来一样：这一层装得下它们，于是这个文件系统**分大小写**。
    // 走一遍本来就要列每一层，这个证据不花任何额外的系统调用。
    let mut case_sensitive = false;
    loop {
        let mut folded: BTreeSet<String> = BTreeSet::new();
        for entry in pending {
            if let Some(name) = entry.path.file_name().and_then(|name| name.to_str())
                && !folded.insert(path::fold(name))
            {
                case_sensitive = true;
            }
            if entry.kind == EntryKind::Dir && !entry.meta.is_unreadable() {
                out.dirs.insert(path::catalog_key(&root, &entry.path));
                stack.push(entry.path);
                continue;
            }
            let stamp = match (entry.kind, entry.meta) {
                (EntryKind::File, EntryMeta::Known { len, modified }) => Some(Stamp {
                    bytes: len,
                    mtime_ns: modified.and_then(mtime_ns),
                }),
                // 读不到元数据的目录、符号链接、设备文件——说不清是什么，一律不动。
                _ => None,
            };
            out.files.push(TargetFile {
                path: path::catalog_key(&root, &entry.path),
                stamp,
            });
        }
        let Some(dir) = stack.pop() else { break };
        match fs.read_dir(&dir) {
            Ok(entries) => pending = entries,
            Err(_) => {
                out.unlistable_dirs += 1;
                pending = Vec::new();
            }
        }
    }
    out.files.sort_by(|a, b| a.path.cmp(&b.path));
    out.case_insensitive = if case_sensitive {
        Some(false)
    } else {
        crate::fs::case_insensitive(fs, &root)
    };
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemFs;

    #[test]
    fn 走一遍目标折出实际状态() {
        let mut fs = MemFs::new();
        fs.file("/卡/GB/一.zip", vec![0; 1024]);
        fs.file("/卡/saves/存档.sav", vec![0; 16]);
        fs.touch("/卡/saves/存档.sav", 42);
        let state = observe(&fs, Path::new("/卡")).expect("看得见");
        assert_eq!(state.files.len(), 2);
        assert_eq!(state.files[0].path, "GB/一.zip");
        assert_eq!(state.files[0].stamp.expect("有戳").bytes, 1024);
        assert_eq!(state.files[1].path, "saves/存档.sav");
        assert_eq!(
            state.files[1].stamp.expect("有戳").mtime_ns,
            Some(42_000_000_000)
        );
    }

    #[test]
    fn 卡不在位是错误而不是空目标() {
        let fs = MemFs::new();
        let error = observe(&fs, Path::new("/卡")).expect_err("停住");
        assert!(matches!(error, ObserveError::Absent { .. }));
    }

    #[test]
    fn 元数据读不到的文件照样入账_戳是空的() {
        let mut fs = MemFs::new();
        fs.unreadable_meta("/卡/GB/读不到.zip");
        let state = observe(&fs, Path::new("/卡")).expect("看得见");
        assert_eq!(state.files.len(), 1);
        assert!(state.files[0].stamp.is_none(), "第三态：不是 0 字节");
    }

    #[test]
    fn 列不开的目录数出来() {
        let mut fs = MemFs::new();
        fs.file("/卡/GB/一.zip", vec![0; 8]);
        fs.unlistable_dir("/卡/System Volume Information");
        let state = observe(&fs, Path::new("/卡")).expect("看得见");
        assert_eq!(state.unlistable_dirs, 1);
        assert_eq!(state.files.len(), 1);
    }

    #[test]
    fn 走过的目录也记下来() {
        // 落点的目录段要拿它去折齐：卡上那个目录到底怎么拼，只有 `read_dir` 说得清。
        let mut fs = MemFs::new();
        fs.file("/卡/gb/一.zip", vec![0; 8]);
        fs.dir("/卡/Media/box");
        let state = observe(&fs, Path::new("/卡")).expect("看得见");
        assert_eq!(
            state.dirs.iter().map(String::as_str).collect::<Vec<_>>(),
            ["Media", "Media/box", "gb"],
        );
    }

    #[test]
    fn 认不认大小写_看一眼目标就带回来() {
        let mut 不认 = MemFs::insensitive();
        不认.file("/卡/gb/一.zip", vec![0; 8]);
        let state = observe(&不认, Path::new("/卡")).expect("看得见");
        assert_eq!(state.case_insensitive, Some(true));

        let mut 认 = MemFs::new();
        认.file("/卡/gb/一.zip", vec![0; 8]);
        let state = observe(&认, Path::new("/卡")).expect("看得见");
        assert_eq!(state.case_insensitive, Some(false));
    }

    #[test]
    fn 同一层里两个只差大小写的目录_不必再问就知道它分大小写() {
        // 装得下 `GB/` 与 `gb/` 这件事本身就是证据，一次额外的系统调用都不用花。
        let mut fs = MemFs::new();
        fs.file("/卡/GB/一.zip", vec![0; 8]);
        fs.file("/卡/gb/二.zip", vec![0; 8]);
        let state = observe(&fs, Path::new("/卡")).expect("看得见");
        assert_eq!(state.case_insensitive, Some(false));
    }

    #[test]
    fn 符号链接说不清是什么_戳是空的() {
        let mut fs = MemFs::new();
        fs.symlink("/卡/GB/链接.zip");
        let state = observe(&fs, Path::new("/卡")).expect("看得见");
        assert_eq!(state.files.len(), 1);
        assert!(state.files[0].stamp.is_none());
    }
}
