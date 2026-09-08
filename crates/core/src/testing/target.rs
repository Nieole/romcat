//! 目标设备那一侧的**假视图**：把开发机上造不出来的那一格造出来。
//!
//! 同步执行层那道**落点闸**（`sync::execute` 模块文档七、八）的正确性，在**两种折叠
//! 语义**上不是同一件事：不分大小写的目标上，卡里那份 `GB/Tetris.zip` 与我们要写的
//! `GB/tetris.zip` **就是同一个文件**，挡不住就是把维护者的东西顶掉；分大小写的盘上
//! 它们是两个文件，挡不住只是在旁边多写一份。可**一台机器上只有一种**——开发机是
//! ext4（分），ADR-0015 定的目标设备（exFAT / FAT32）与 ADR-0018 那台主力机（默认
//! APFS）都是不分。造不出另一种挂载点，于是「不敏感那边只会更容易挡住」这句话挂了
//! 两轮都只是**推理**（挂单 `Q135`）。
//!
//! 办法是把卡上真实那棵树**照一张相**，放进一个指定折叠语义的 [`MemFs`]，从
//! [`Sources::target`](crate::sync::Sources::target) 塞进执行层。
//! 字节照旧落在真实的临时目录里——那道接缝**只管读**，于是「真机行为一个字节不变」
//! 是构造上成立的，不是断言出来的。
//!
//! # 这不是什么
//!
//! - **不是一层通用的文件系统抽象。** 它只喂那道闸问目标的那几句话（占没占、盘上真名
//!   是什么）。写那一侧——建目录、落 `.romcat-part`、`sync_all`、`rename`、读回戳
//!   ——照旧是 `std::fs`，不许拿它去替换。
//! - **照的是一张相，不是一块活盘。** 相是在这一趟写字节**之前**照的，而那道闸问的
//!   每一句也都在写之前。唯一落在后面的是
//!   [`settled`](crate::sync::execute) 那一问（`create_dir_all` 刚回来那一刻的目录
//!   真名）：这一趟里工具自己新建的目录相片上没有，那一问会落回原样拼出来的那一条
//!   ——而那个目录**正是刚按我们要的写法建出来的**，两条一模一样。要验「一趟当中卡
//!   变了」得另想办法。
//! - **文件内容一律零填充。** 长度照实（`observe` 数得对），内容不是真的：那道闸只
//!   问名字。
//! - **列不开的目录底下照不到。** Unix 上一个 `0300` 的目录 `read_dir` 失败、按名字
//!   `open` 却照样成功，相片只留得下「这一格列不开」（[`MemFs::unlistable_dir`]），
//!   底下那些文件留不下来。要那一格连着底下的东西一起造，直接手捏一个 `MemFs`：
//!   先 `unlistable_dir` 再往底下 `file`，那正是真盘的语义。
//! - **`read_head` 对**目录**答得与真盘不一样。** Unix 上 `File::open` 打得开目录、
//!   `take(0)` 一个字节都不读于是照样成功，`MemFs` 却报错（`crate::fs` 那条
//!   `目录上先原样试一次是个假阳性_所以目录段另走一条` 钉的就是这个差别）。落点闸问的
//!   键全是文件，够不着；要验「该建目录的位置上躺着一个文件」这类分支，得先想清楚
//!   这一格。
//! - **删除那一步会被它骗。** `erase` 从这道接缝上认盘上真名，却用 `std::fs` 真删。
//!   相片说「在」而真盘上没有时，`remove_file` 会报一条本不存在的失败。往**带删除步骤**
//!   的计划里塞视图之前，先确认相片与真卡对得上。
//!
//! [`MemFs::unlistable_dir`]: crate::fs::MemFs::unlistable_dir

use std::path::Path;

use crate::fs::{EntryKind, LibraryFs, MemFs, RealFs};

/// 目标文件系统**认不认大小写**——落点闸的正确性在这两档上不是同一件事。
///
/// 与 [`fs::case_insensitive`](crate::fs::case_insensitive) 答的是同一个问题，
/// 只是这里由测试**指定**而不是去问盘：要的正是那台机器上问不出来的那一档。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Folding {
    /// **分大小写**：ext4、大小写敏感的 APFS。`GB/` 与 `gb/` 是两个目录。
    Sensitive,
    /// **不分大小写**：ADR-0015 定的目标设备（exFAT / FAT32）、ADR-0018 那台主力机
    /// （默认 APFS）、Windows。`GB/` 与 `gb/` 落在同一个目录上。
    Insensitive,
}

impl Folding {
    /// 断言消息里用的那个词。同一条测试跑两遍，红的时候要说得出是哪一遍。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Sensitive => "分大小写",
            Self::Insensitive => "不分大小写",
        }
    }
}

/// 给 `root` 底下那棵树照一张相，按 `folding` 的折叠语义放进一个 [`MemFs`]。
///
/// 不分大小写那一档会把 `GB/` 与 `gb/` **并成一个**——真盘上那两个本来就装不下
/// （[`MemFs::insensitive`](crate::fs::MemFs::insensitive)）。并进谁是**定的**：
/// 每一层按路径排过再照，排在前面那个留下。
#[must_use]
pub fn snapshot(root: &Path, folding: Folding) -> MemFs {
    let mut fs = match folding {
        Folding::Sensitive => MemFs::new(),
        Folding::Insensitive => MemFs::insensitive(),
    };
    fs.dir(root);
    copy_into(&mut fs, root);
    fs
}

/// 照一层，然后往下走。
fn copy_into(fs: &mut MemFs, dir: &Path) {
    let Ok(mut entries) = RealFs.read_dir(dir) else {
        // 列不开：如实记成列不开，别记成一个空目录——那两件事在 ADR-0021 里是两态。
        fs.unlistable_dir(dir);
        return;
    };
    // 排一遍：不分大小写那一档要并层，并进谁得是定的，不能随 `read_dir` 的次序变。
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    for entry in entries {
        match entry.kind {
            EntryKind::Dir => {
                fs.dir(&entry.path);
                copy_into(fs, &entry.path);
            }
            EntryKind::File if entry.meta.is_unreadable() => {
                fs.unreadable_meta(&entry.path);
            }
            EntryKind::File => {
                let len = entry.meta.byte_len().unwrap_or(0);
                fs.file(&entry.path, vec![0_u8; usize::try_from(len).unwrap_or(0)]);
            }
            EntryKind::Symlink => {
                fs.symlink(&entry.path);
            }
            EntryKind::Other => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::temp_dir;
    use std::fs;

    fn 写(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
        fs::write(path, bytes).expect("能写文件");
    }

    #[test]
    fn 相片照得下名字与长度() {
        let 卡 = temp_dir("snap-basic");
        写(&卡.path().join("GB/Tetris.zip"), &[7_u8; 32]);
        let 相 = snapshot(卡.path(), Folding::Sensitive);
        let 那份 = 卡.path().join("GB/Tetris.zip");
        assert_eq!(相.read_head(&那份, 64).expect("开得了").len(), 32);
        assert!(
            相.read_head(&卡.path().join("GB/tetris.zip"), 0).is_err(),
            "分大小写那一档上，只差大小写的那条打不开",
        );
    }

    #[test]
    fn 不分大小写那一档上_只差大小写的落在同一个东西上() {
        let 卡 = temp_dir("snap-fold");
        写(&卡.path().join("GB/Tetris.zip"), &[7_u8; 32]);
        let 相 = snapshot(卡.path(), Folding::Insensitive);
        assert!(
            相.read_head(&卡.path().join("gb/tetris.zip"), 0).is_ok(),
            "不分大小写的卡上，这两条就是同一个文件",
        );
    }

    #[test]
    fn 不分大小写那一档上_两个只差大小写的目录并成一个() {
        // 真盘上那两个装不下。并进谁是定的：排在前面那个（`GB` < `gb`）。
        let 卡 = temp_dir("snap-merge");
        写(&卡.path().join("GB/别的.txt"), b"x");
        写(&卡.path().join("gb/Tetris.zip"), b"y");
        let 相 = snapshot(卡.path(), Folding::Insensitive);
        let mut 名字: Vec<String> = 相
            .read_dir(卡.path())
            .expect("列得开")
            .into_iter()
            .filter_map(|entry| {
                entry
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
            })
            .collect();
        名字.sort();
        assert_eq!(名字, vec!["GB".to_string()], "两层并成了一层");
        assert!(相.read_head(&卡.path().join("GB/Tetris.zip"), 0).is_ok());
        assert!(相.read_head(&卡.path().join("GB/别的.txt"), 0).is_ok());
    }

    #[test]
    fn 相片存的是盘上原始那个形态_不做_nfc_归一() {
        // ADR-0020 的红线：**读盘用系统给的原始形式，入库与比较才用 NFC**。
        // 相片是拿去读的那一头，归一了就等于凭空造出一块「查找不分解敏感」的盘，
        // 而 SD 卡的 exFAT / FAT32 是不是那样没人查过（挂账 D82）。
        const 分解形: &str = "\u{30b1}\u{3099}ーム";
        let 卡 = temp_dir("snap-nfd");
        写(&卡.path().join(分解形).join("一.zip"), b"x");
        let 相 = snapshot(卡.path(), Folding::Sensitive);
        assert!(
            相.read_head(&卡.path().join(分解形).join("一.zip"), 0)
                .is_ok(),
            "盘上是哪个形态，相片里就得是哪个",
        );
        assert!(
            相.read_head(&卡.path().join("ゲーム/一.zip"), 0).is_err(),
            "预组合那一条打不开——真盘上分解敏感时正是这样，归一了这条就假绿了",
        );
    }

    #[test]
    fn 分大小写那一档上_两个只差大小写的目录照旧是两个() {
        let 卡 = temp_dir("snap-two");
        写(&卡.path().join("GB/别的.txt"), b"x");
        写(&卡.path().join("gb/Tetris.zip"), b"y");
        let 相 = snapshot(卡.path(), Folding::Sensitive);
        assert_eq!(相.read_dir(卡.path()).expect("列得开").len(), 2);
    }
}
