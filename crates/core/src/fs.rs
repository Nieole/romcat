//! 主库的只读接缝。
//!
//! **这个 trait 只有读操作。** 主库是 10T 不可再生的资源，工具对它一个字节都不改
//! （ADR-0004）。把只读做成类型层面的事实，好过在每个实现里记得别写——扫描器只能
//! 通过 [`LibraryFs`] 接触主库，而 [`LibraryFs`] 根本没有写的办法。
//!
//! 这也是把 IO 挤到边缘的那道接缝：遍历、归类、统计全是纯逻辑，测试挂在纯逻辑上，
//! 用 [`mem::MemFs`] 完全在内存里跑。

pub mod mem;
pub mod real;

use std::collections::HashMap;
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub use mem::MemFs;
pub use real::RealFs;

/// 一个只读的随机访问句柄：能顺序读、也能 seek，但**没有任何写的办法**。
///
/// 它是 [`LibraryFs::open`] 的返回类型。之所以要有它，是因为穿透**透明容器**读不了
/// 固定的两头：zip 的中央目录在文件末尾、长度得先量出来，7z 的头部由
/// `NextHeaderOffset` 指到任意位置，拿到头部之后还要从某个偏移开始流式解压。
/// 只读这一条仍然由类型保证（ADR-0004）。
pub trait ReadSeek: Read + Seek {}

impl<T: Read + Seek + ?Sized> ReadSeek for T {}

/// 目录项的类型。符号链接不跟随，因此它是独立的一类而不是 `File` 或 `Dir`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// 普通文件。
    File,
    /// 目录。
    Dir,
    /// 符号链接。不跟随——跟随会引入环，也会让同一份内容被统计两次。
    Symlink,
    /// 其余（设备、管道等）。
    Other,
}

/// 一条目录项的元数据。
///
/// **不可读是第三态**（ADR-0021）。主库那块 NTFS 盘上实测有 4,085 个文件（256,128 个
/// 里的 1.59%）在 macOS 的 fskit 只读驱动下连元数据都取不到——`stat` / `lstat` /
/// `open` / `read` 全部失败（`ENOTSUP`），**只有 `readdir` 有效**：名字拿得到，其余
/// 一无所知。
///
/// 用枚举而不是两个 `Option`，是因为「大小未知」和「大小为零」必须分得开：库里另有
/// **4,317 个真正的空文件**。混在一起会让体检报告的空文件数虚高，也会让增量扫描把
/// 「读不到」误当成「变成了 0 字节」。这里用类型逼调用方显式处理这一态。
///
/// 详见 `docs/research/ntfs-mtime-precision.md` 第 6.2 节与 ADR-0021。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryMeta {
    /// 元数据读到了。
    Known {
        /// 文件字节数；目录与链接为 0。
        len: u64,
        /// 最后修改时间；驱动给不出时为 `None`。
        ///
        /// 它是增量扫描三元组 `(路径, 大小, 修改时间)` 的第三项。NTFS 的 100 纳秒刻度
        /// 完整透出、APFS 是 1 纳秒、且不存在时区偏差，这一侧已实测可用
        /// （`docs/research/ntfs-mtime-precision.md`）。
        modified: Option<SystemTime>,
    },
    /// 元数据读不到。**不是**「大小为 0」，也**不是**「不存在」。
    Unreadable,
}

impl EntryMeta {
    /// 字节数；读不到时是 `None`（不是 `Some(0)`）。
    #[must_use]
    pub fn byte_len(&self) -> Option<u64> {
        match self {
            Self::Known { len, .. } => Some(*len),
            Self::Unreadable => None,
        }
    }

    /// 最后修改时间；读不到时是 `None`。
    #[must_use]
    pub fn modified(&self) -> Option<SystemTime> {
        match self {
            Self::Known { modified, .. } => *modified,
            Self::Unreadable => None,
        }
    }

    /// 元数据是否读不到。
    #[must_use]
    pub fn is_unreadable(&self) -> bool {
        matches!(self, Self::Unreadable)
    }
}

/// 一条目录项，带扫描需要的全部元数据。
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// 完整路径，**系统给出的原始形式**。读盘用它；入库与比较要先过
    /// [`path::catalog_key`](crate::path::catalog_key) 折成 NFC 键（ADR-0020）。
    pub path: PathBuf,
    /// 目录项类型。
    pub kind: EntryKind,
    /// 元数据，含**不可读**这一第三态。
    pub meta: EntryMeta,
}

/// 把一个**中立库的键**还原成盘上真实存在的那条路径。
///
/// ADR-0020 的红线：**读盘用系统给的原始形式，入库与比较用 NFC**。键是 NFC 的，
/// 而盘上那个名字可能是分解形式——macOS 的 NTFS 驱动交出来的名字实测 1.99% 如此。
/// 直接把键接在根后面去开文件，在**分解敏感**的文件系统上会打不开，然后这个失败
/// 会被解释成「文件不在了」，而在同步这一侧，「不在了」意味着删除或者重传。
///
/// 于是这里分两步：
///
/// 1. **先原样试一次**。绝大多数路径两种形式相同，而查找不分解敏感的文件系统
///    （实测 macOS 的 fskit NTFS 驱动就是，383 个两种形式不同的键 NFC/NFD 都开得了）
///    连剩下那点也一次就中。这一步不花任何额外的系统调用。
/// 2. **不中才逐段列目录去认**，按 NFC 折过再比。只有真正撞上分解敏感的文件系统时
///    才走到这里。
///
/// 找不到时返回 `None`：**这是「盘上没有这条路径」的意思**，不是「读不动」。
///
/// 一趟里要还原成千上万条路径时走 [`DirCache::real_path`]：同一个目录只列一次。
///
/// # 交回来的不保证是**字节级的真名**
///
/// 第一步「原样试一次」一旦中了，交回来的就是**我们要的那个写法**，而不是盘上那一条。
/// 在**大小写不敏感**的目标上（exFAT / FAT32 / Windows / 默认 APFS）这两者会真的不同：
/// 盘上叫 `GB`，拿 `gb` 也开得了，交回来的却是 `gb`。
///
/// 它的契约因此是「**一条解析得到同一个东西的路径**」——拿去 `open` / `remove` /
/// `rename` 都对，拿去**当键**就错了。要字节级的真名走 [`real_dir`]。
#[must_use]
pub fn real_path(fs: &dyn LibraryFs, root: &Path, key: &str) -> Option<PathBuf> {
    DirCache::default().real_path(fs, root, key)
}

/// 把一个**目录**的键还原成盘上**字节级真实**的那条路径。
///
/// 与 [`real_path`] 是两件事，别混用：那一条只保证「解析得到同一个东西」，这一条保证
/// 「盘上就这么拼」。差别在**大小写不敏感**的文件系统上会咬人——子库同步拿落点的目录段
/// 去拼**清单的键**，而下一趟 [`observe`](fn@crate::sync::observe) 交出来的是 `read_dir`
/// 给的真名：两边不是同一个写法的话，工具从第二趟起就认不出自己放的那一份。
///
/// 判据一律**按 `read_dir` 的真实结果走**，逐段认下去：
///
/// 1. NFC 形式**一模一样**的那个名字——两种文件系统上都是它，先认它。
/// 2. 没有，但**只差大小写**的恰好有一个：那到底是不是我们要的这个目录，**问文件系统**
///    ——`read_dir` 那条我们要的路径。开得了就说明它认不出大小写，那一个就是；开不了
///    就说明它分大小写，我们要的那个目录**真的不存在**（`GB/` 与 `gb/` 在 ext4 上是两个
///    目录，这一条把它们守住）。
/// 3. 都不是就 `None`。
///
/// **不做「先原样试一次」那一步**：那一步在 Unix 上对目录是**假阳性**——`File::open`
/// 打得开目录，`take(0)` 一个字节都不读于是照样成功，交回来的就成了「我们要的写法」。
#[must_use]
pub fn real_dir(fs: &dyn LibraryFs, root: &Path, key: &str) -> Option<PathBuf> {
    let mut at = root.to_path_buf();
    for segment in key.split('/') {
        if segment.is_empty() {
            continue;
        }
        at = real_segment(fs, &at, segment)?;
    }
    Some(at)
}

/// [`real_dir`] 的一段。
fn real_segment(fs: &dyn LibraryFs, dir: &Path, segment: &str) -> Option<PathBuf> {
    let wanted = crate::path::nfc(segment);
    let folded = crate::path::fold(segment);
    let mut only_case: Option<PathBuf> = None;
    let mut how_many = 0_usize;
    for entry in fs.read_dir(dir).ok()? {
        if entry.kind != EntryKind::Dir {
            continue;
        }
        let Some(name) = entry.path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if crate::path::nfc(name) == wanted {
            return Some(entry.path);
        }
        if crate::path::fold(name) == folded {
            how_many += 1;
            only_case = Some(entry.path);
        }
    }
    // 只差大小写的不止一个：那一层装得下它们，于是这个文件系统分大小写，而我们要的
    // 那一个既然没逐字出现过就是真的不在。
    let one = only_case.filter(|_| how_many == 1)?;
    fs.read_dir(&dir.join(segment)).is_ok().then_some(one)
}

/// 这个目录所在的文件系统**认不认大小写**。
///
/// `Some(true)` 是「不认」（exFAT / FAT32 / NTFS / 默认 APFS——ADR-0015 定的目标设备
/// 与 ADR-0018 定的主力机全在这一档），`Some(false)` 是「认」（ext4、大小写敏感的
/// APFS），`None` 是**问不出来**。三态分开是因为「问不出来」不许当成任何一边：判成
/// 「不认」会在 ext4 上把两个真能并存的目录折成一个，判成「认」则是眼下这个 bug。
///
/// 三步，都只读：
///
/// 1. 这一层里有两个名字**折起来一样**吗。有就证完了：装得下它们的文件系统分大小写。
/// 2. 没有就挑一个带 ASCII 字母的名字，**把大小写翻过来问一次**。原样那条先确认问得通
///    （读不动的条目答出来的是「没有」而不是「分大小写」），翻过来那条开得了就是不认。
/// 3. 这一层一个带字母的名字都没有（空目录也算）：拿**这个目录自己**那一段名字去翻。
#[must_use]
pub fn case_insensitive(fs: &dyn LibraryFs, dir: &Path) -> Option<bool> {
    let entries = fs.read_dir(dir).ok()?;
    let mut seen: HashMap<String, ()> = HashMap::new();
    for entry in &entries {
        let Some(name) = entry.path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if seen.insert(crate::path::fold(name), ()).is_some() {
            return Some(false);
        }
    }
    for entry in &entries {
        let Some(name) = entry.path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(flipped) = flip_case(name) else {
            continue;
        };
        let flipped = dir.join(flipped);
        match entry.kind {
            EntryKind::Dir if fs.read_dir(&entry.path).is_ok() => {
                return Some(fs.read_dir(&flipped).is_ok());
            }
            EntryKind::File if fs.read_head(&entry.path, 0).is_ok() => {
                return Some(fs.read_head(&flipped, 0).is_ok());
            }
            _ => continue,
        }
    }
    let name = dir.file_name().and_then(|name| name.to_str())?;
    let flipped = dir.parent()?.join(flip_case(name)?);
    Some(fs.read_dir(&flipped).is_ok())
}

/// 把一个名字里每个 ASCII 字母的大小写翻过来；一个字母都没有时是 `None`。
///
/// **只翻 ASCII**：非 ASCII 的大小写映射不保证一一对应（`ß` 变 `SS`、土耳其语的 `i`
/// 另有一套），拿它去问文件系统问的就不是同一个名字了。
fn flip_case(name: &str) -> Option<String> {
    if !name.chars().any(|one| one.is_ascii_alphabetic()) {
        return None;
    }
    Some(
        name.chars()
            .map(|one| {
                if one.is_ascii_uppercase() {
                    one.to_ascii_lowercase()
                } else if one.is_ascii_lowercase() {
                    one.to_ascii_uppercase()
                } else {
                    one
                }
            })
            .collect(),
    )
}

/// 逐段列目录那条退路上，**每个目录只列一次**的那份记性。
///
/// 退路要把一层目录整个列出来、按 NFC 折过去认名字。同一个目录下有几百个分解形式的
/// 名字时——主库实测有 **5 个目录名**本身就是分解形式，一个目录中招整棵子树都跟着走
/// 退路（ADR-0020）——不记的话同一份 listing 会被反复读上几百遍。
///
/// **一趟识别的生命周期内有效就够了**：主库只读（ADR-0004），一趟里名字不会变；出了
/// 那一趟就把它扔掉，免得手里攥着一份过期的盘。
#[derive(Debug, Default)]
pub struct DirCache {
    /// 目录 → 「那一层每个名字的 NFC 形式 → 盘上真实的那条路径」。
    /// 列不开的目录记一份空的：列不开这件事也不必再问第二遍。
    by_dir: HashMap<PathBuf, HashMap<String, PathBuf>>,
}

impl DirCache {
    /// 与 [`real_path`] 同一件事，只是逐段列目录那条退路上每个目录只列一次。
    #[must_use]
    pub fn real_path(&mut self, fs: &dyn LibraryFs, root: &Path, key: &str) -> Option<PathBuf> {
        let direct = root.join(key.replace('/', std::path::MAIN_SEPARATOR_STR));
        // `read_head` 读 0 字节：只要开得了就说明这条路径在。比 `metadata` 更贴近
        // 「等下真要读它」这件事。
        //
        // **对目录它在 Unix 上也成功**（`File::open` 打得开目录，`take(0)` 一个字节都
        // 不读），于是交回来的是「我们要的写法」而不是盘上那一条——见函数文档那一节。
        // 要目录的字节级真名走 [`real_dir`]，别指望这一步会掉到下面逐段那条路上。
        if fs.read_head(&direct, 0).is_ok() {
            return Some(direct);
        }
        let mut at = root.to_path_buf();
        for segment in key.split('/') {
            if segment.is_empty() {
                continue;
            }
            at = self.lookup(fs, &at, segment)?;
        }
        Some(at)
    }

    /// `dir` 这一层里，NFC 形式是 `segment` 的那个名字，在盘上真实的那条路径。
    ///
    /// 一律走 listing 而不再逐段「先原样试一次」：那个试探对目录是一次整层 `read_dir`，
    /// 与直接列出来一样贵，而列出来的这一份还留得住给同一层的下一条用。名字读不出
    /// UTF-8 的条目跳过——折不了 NFC 的东西也就无从比对。
    fn lookup(&mut self, fs: &dyn LibraryFs, dir: &Path, segment: &str) -> Option<PathBuf> {
        let listing = self.by_dir.entry(dir.to_path_buf()).or_insert_with(|| {
            let mut index: HashMap<String, PathBuf> = HashMap::new();
            for entry in fs.read_dir(dir).unwrap_or_default() {
                let folded = entry
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| crate::path::nfc(name).into_owned());
                let Some(folded) = folded else { continue };
                // 同一层里两个名字折成同一个 NFC 时按 `read_dir` 的次序取头一个。
                index.entry(folded).or_insert(entry.path);
            }
            index
        });
        listing.get(crate::path::nfc(segment).as_ref()).cloned()
    }
}

/// 主库的只读视图。
pub trait LibraryFs: Sync {
    /// 规范化扫描根：转成绝对路径，并在 Windows 上加 `\\?\` 扩展长度前缀。
    ///
    /// 之后所有子路径都从这个结果拼出来，于是整条遍历天然是长路径安全的。
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;

    /// 列出一层目录，不递归、不跟随符号链接。
    fn read_dir(&self, dir: &Path) -> io::Result<Vec<DirEntry>>;

    /// 读取文件头部至多 `limit` 字节。文件比 `limit` 短时返回实际长度。
    fn read_head(&self, file: &Path, limit: usize) -> io::Result<Vec<u8>>;

    /// 读取文件尾部至多 `limit` 字节。
    ///
    /// WS / WSC 的内部头在文件末尾，没有这个方法就只能把它们排除在抽样之外。
    fn read_tail(&self, file: &Path, limit: usize) -> io::Result<Vec<u8>>;

    /// 打开一个文件做只读随机访问。
    ///
    /// 穿透**透明容器**要的就是它（[`crate::container`]）：zip 要先量长度再回头读中央
    /// 目录，7z 要按头部里的偏移跳过去，解压时还要从某个位置开始一路流下去。
    /// 返回的句柄只有 [`Read`] 与 [`Seek`]——主库仍然一个字节都写不了。
    fn open(&self, file: &Path) -> io::Result<Box<dyn ReadSeek + '_>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::MemFs;

    /// 同一个名字的两种规范化形式。`ゲ` 预组合 vs `ケ` + 浊音符。
    const 预组合: &str = "ゲーム/一.zip";
    const 分解形: &str = "\u{30b1}\u{3099}ーム/一.zip";

    #[test]
    fn 键与盘上的名字同形时一次就中() {
        let mut fs = MemFs::new();
        fs.file("/库/FC/魂斗罗.zip", vec![0; 8]);
        assert_eq!(
            real_path(&fs, Path::new("/库"), "FC/魂斗罗.zip"),
            Some(PathBuf::from("/库/FC/魂斗罗.zip"))
        );
    }

    #[test]
    fn 盘上是分解形式时照样找得到() {
        // `MemFs` 按字节精确匹配，于是它就是一个**分解敏感**的文件系统——
        // SD 卡的 exFAT / FAT32 会不会这样没人查过，而这正是挂账 D82 的由来。
        let mut fs = MemFs::new();
        fs.file(format!("/库/{分解形}"), vec![0; 8]);
        assert_ne!(预组合, 分解形, "两个字符串本身不同");
        assert!(
            fs.read_head(Path::new(&format!("/库/{预组合}")), 0)
                .is_err(),
            "直接拼是打不开的——这条测试要防的就是把这个失败读成「文件不在了」",
        );
        assert_eq!(
            real_path(&fs, Path::new("/库"), 预组合),
            Some(PathBuf::from(format!("/库/{分解形}"))),
        );
    }

    #[test]
    fn 目录上先原样试一次是个假阳性_所以目录段另走一条() {
        // Unix 上 `File::open` 打得开目录、`take(0)` 一个字节都不读，于是
        // `real_path` 的第一步在**目录**上照样成功，交回来的是我们要的写法。
        // `MemFs` 照着这个语义来（`read_head` 对目录报错），可真盘不是——
        // 这条测试钉住的是**两个函数的分工**：要字节级真名只能走 `real_dir`。
        let mut fs = MemFs::insensitive();
        fs.dir("/卡/gb");
        fs.file("/卡/gb/存档.sav", vec![0; 8]);
        assert_eq!(
            real_dir(&fs, Path::new("/卡"), "GB"),
            Some(PathBuf::from("/卡/gb")),
            "盘上叫 gb，交回来的就得是 gb",
        );
    }

    #[test]
    fn 分大小写的盘上_只差大小写的目录不算认得出来() {
        // ext4 上 `GB/` 与 `gb/` 是**两个目录**。折过去认等于把两棵树并成一棵。
        let mut fs = MemFs::new();
        fs.dir("/卡/gb");
        assert_eq!(real_dir(&fs, Path::new("/卡"), "gb"), Some("/卡/gb".into()));
        assert_eq!(
            real_dir(&fs, Path::new("/卡"), "GB"),
            None,
            "那个目录真的不在"
        );
    }

    #[test]
    fn 多层目录逐段折回真名() {
        let mut fs = MemFs::insensitive();
        fs.dir("/卡/roms/gb");
        assert_eq!(
            real_dir(&fs, Path::new("/卡"), "ROMS/GB"),
            Some(PathBuf::from("/卡/roms/gb")),
        );
    }

    #[test]
    fn 认不认大小写_两种盘都问得出来() {
        let mut 不认 = MemFs::insensitive();
        不认.file("/卡/gb/存档.sav", vec![0; 8]);
        assert_eq!(case_insensitive(&不认, Path::new("/卡")), Some(true));

        let mut 认 = MemFs::new();
        认.file("/卡/gb/存档.sav", vec![0; 8]);
        assert_eq!(case_insensitive(&认, Path::new("/卡")), Some(false));
    }

    #[test]
    fn 同一层装得下两个折起来一样的名字_那就是分大小写() {
        // 不必再问文件系统：装得下它们这件事本身就是证据。
        let mut fs = MemFs::new();
        fs.dir("/卡/gb");
        fs.dir("/卡/GB");
        assert_eq!(case_insensitive(&fs, Path::new("/卡")), Some(false));
    }

    #[test]
    fn 这一层没有带字母的名字时_拿目录自己那一段去问() {
        // 头一趟同步到一张空卡上就是这样：底下什么都没有，只剩子库根自己那个名字。
        let mut 不认 = MemFs::insensitive();
        不认.dir("/Volumes/SDCARD");
        assert_eq!(
            case_insensitive(&不认, Path::new("/Volumes/SDCARD")),
            Some(true)
        );

        let mut 认 = MemFs::new();
        认.dir("/Volumes/SDCARD");
        assert_eq!(
            case_insensitive(&认, Path::new("/Volumes/SDCARD")),
            Some(false)
        );
    }

    #[test]
    fn 一个字母都没有就是问不出来_而问不出来不许当成任何一边() {
        // 子库根与它底下的东西全是中文：翻不动大小写，也就问不出这个文件系统认不认。
        // 这一态**不许**当成任何一边——`sync::align` 见到它就一条都不折。
        let mut fs = MemFs::insensitive();
        fs.file("/卡/中文/一.zip", vec![0; 8]);
        assert_eq!(case_insensitive(&fs, Path::new("/卡")), None);
    }

    #[test]
    fn 盘上真的没有时是空的() {
        let mut fs = MemFs::new();
        fs.file("/库/FC/魂斗罗.zip", vec![0; 8]);
        assert_eq!(real_path(&fs, Path::new("/库"), "FC/不存在.zip"), None);
        assert_eq!(real_path(&fs, Path::new("/库"), "没有这个目录/x.zip"), None);
    }
}
