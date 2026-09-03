//! 按扩展名与文件名把磁盘上的东西归类。
//!
//! 三类主线来自 ADR-0014：**透明容器**（zip / 7z / rar 这类只是包装的东西）识别要穿透它；**压缩镜像**不是包装，
//! 它就是变体本身的形态；其余可直接读的 ROM 与镜像是**裸文件**。体检报告要回答的
//! 「识别管线会走哪条路径为主」，答案就是这三类各占多少。
//!
//! 这里的表是**数据不是代码**的雏形：全部规则是几张常量表，票 05 把平台清单与成型
//! 规则外置成声明式配置时，它们会一起搬过去。

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::path::{extension_lower, file_name_lower};

/// 文件在识别管线里的归属。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Category {
    /// **透明容器**：zip / 7z / rar 这类只是包装、不改变内容身份的东西。
    TransparentContainer,
    /// **压缩镜像**：chd / cso / pbp / rvz 这类本身就是变体形态的格式。
    CompressedImage,
    /// **裸文件**：可直接读的 ROM 与未压缩镜像。
    BareFile,
    /// 媒体与元数据：封面、截图、视频、前端的元数据文件。
    MediaOrMetadata,
    /// 未归类：扩展名不在任何表里。报告要把它们列出来，那是表的缺口。
    #[default]
    Unclassified,
}

impl Category {
    /// 报告里用的中文名。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::TransparentContainer => "透明容器",
            Self::CompressedImage => "压缩镜像",
            Self::BareFile => "裸文件",
            Self::MediaOrMetadata => "媒体与元数据",
            Self::Unclassified => "未归类",
        }
    }

    /// 报告里固定的排列顺序。
    #[must_use]
    pub fn all() -> [Self; 5] {
        [
            Self::TransparentContainer,
            Self::CompressedImage,
            Self::BareFile,
            Self::MediaOrMetadata,
            Self::Unclassified,
        ]
    }
}

/// 一个文件疑似不该进库的理由。
///
/// 这里只**发现并报告**，绝不自动删除——去重永远由维护者拍板（ADR-0004）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SuspectReason {
    /// 重复拷贝：同名同大小的多份。
    DuplicateCopy,
    /// 文档：说明、攻略、网页、清单。
    Document,
    /// 模拟器本体：可执行文件与动态库。
    EmulatorBinary,
    /// 半截下载：下载工具的临时文件与备份。
    PartialDownload,
    /// 系统垃圾：`.DS_Store`、`Thumbs.db`、AppleDouble 之类。
    SystemJunk,
}

impl SuspectReason {
    /// 报告里用的中文名。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::DuplicateCopy => "重复拷贝",
            Self::Document => "文档",
            Self::EmulatorBinary => "模拟器本体",
            Self::PartialDownload => "半截下载",
            Self::SystemJunk => "系统垃圾",
        }
    }

    /// 报告里固定的排列顺序。
    #[must_use]
    pub fn all() -> [Self; 5] {
        [
            Self::DuplicateCopy,
            Self::Document,
            Self::EmulatorBinary,
            Self::PartialDownload,
            Self::SystemJunk,
        ]
    }
}

/// 一个文件的归类结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Classification {
    /// 三类主线里的归属。
    pub category: Category,
    /// 疑似不该入库的理由；`None` 表示看起来正常。
    pub suspect: Option<SuspectReason>,
    /// 是否像分卷的一段，且**不是入口段**。多个分卷合起来才是一个透明容器
    /// ——聚成一个变体发生在**成型**那一步（[`crate::shape`]），不在遍历途中
    /// （ADR-0022）。
    pub split_volume: bool,
}

const TRANSPARENT_CONTAINERS: &[&str] = &["zip", "7z", "rar", "cbz", "zst"];

// NKit 不在这张表里：NKit 文件叫 `game.nkit.iso`，扩展名是 iso，
// 认它要看光盘逻辑偏移 0x200 处的 `NKIT`（ADR-0014），那是探针的活。
const COMPRESSED_IMAGES: &[&str] = &[
    "chd", "cso", "ciso", "zso", "dax", "jso", "pbp", "rvz", "wia", "wbfs", "gcz", "wux",
];

const BARE_FILES: &[&str] = &[
    // 卡带
    "nes", "unf", "unif", "fds", "sfc", "smc", "swc", "fig", "gb", "gbc", "sgb", "gba", "agb",
    "nds", "dsi", "srl", "ids", "n64", "z64", "v64", "ndd", "md", "gen", "smd", "32x", "sms", "gg",
    "sg", "pce", "sgx", "ws", "wsc", "ngp", "ngc", "lnx", "lyx", "a26", "a78", "col", "int", "vec",
    "vb", "min", "jag", "j64", "rom", "prg", "d64", "adf", "3ds", "cci", "cxi", "cia", "3dsx",
    "xci", "nsp", "vpk", "pkg", "wad", "dol", "elf", "self", "sfo", "edat", "xex", "xbe", "wud",
    "wup", "tik", "tmd",
    // ⭐ Switch 的压缩形态（票 27）。**它们是裸文件不是压缩镜像**，两个理由：
    //
    // 1. **识别读它们的路子与 `.nsp` / `.xci` 一模一样**——`.nsz` 与 `.nsp`、`.xcz` 与
    //    `.xci` 的容器层完全相同（nsz 的 `docs/formats.md` 原话「functionally
    //    identical」），压缩只改内部条目的扩展名（`.nca` → `.ncz`）。头是明文，零解压。
    // 2. **压缩镜像在这个词表里的定义是「模拟器直接读它」**（`CONTEXT.md`），而**没有
    //    任何 Switch 模拟器读得了 `.nsz`**——Eden 的 `FileType` 枚举里没有它，
    //    ES-DE 的扩展名表里也没有。归进那一档等于说反了票 21 记下来的事实。
    //
    // 漏掉它们的代价真机上量得出来：这两个扩展名不在表里时它们**连变体都成不了**，
    // 55 个文件、74.67 GiB（Switch 容量的 33.4%）压根没进过识别管线。
    "nsz", "xcz", // 光盘镜像与轨道
    "iso", "img", "gcm", "cue", "bin", "gdi", "cdi", "mdf", "mds", "nrg", "ccd", "sub", "toc",
    "raw", "ape", "wv",
];

const MEDIA_OR_METADATA: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "bmp", "tga", "ico", "mp4", "mkv", "avi", "webm", "mov",
    "mpg", "mp3", "ogg", "wav", "flac", "xml", "json", "lpl", "dat", "yml", "yaml", "toml",
];

const DOCUMENTS: &[&str] = &[
    "txt", "md", "doc", "docx", "pdf", "nfo", "diz", "html", "htm", "mht", "mhtml", "rtf", "chm",
    "url", "xls", "xlsx", "ppt", "pptx", "epub", "log", "csv",
];

const EMULATOR_BINARIES: &[&str] = &[
    "exe", "dll", "msi", "dmg", "apk", "deb", "appimage", "so", "dylib", "bat", "cmd", "sh", "ps1",
    "jar", "lnk", "com", "scr", "ocx", "sys",
];

const PARTIAL_DOWNLOADS: &[&str] = &[
    "part",
    "crdownload",
    "download",
    "opdownload",
    "partial",
    "aria2",
    "ut",
    "td",
    "xltd",
    "dtapart",
    "unconfirmed",
    "filepart",
    "tmp",
    "temp",
    "bak",
    "old",
    "!qb",
];

const JUNK_FILE_NAMES: &[&str] = &[
    ".ds_store",
    "thumbs.db",
    "ehthumbs.db",
    "desktop.ini",
    ".directory",
    ".localized",
    "icon\r",
];

/// 遍历时整棵跳过的系统目录。它们里面没有库的内容，只会让报告里全是「拒绝访问」。
const SKIPPED_SYSTEM_DIRS: &[&str] = &[
    "$recycle.bin",
    "system volume information",
    ".trashes",
    ".trash",
    ".spotlight-v100",
    ".fseventsd",
    ".temporaryitems",
    "__macosx",
    "found.000",
];

/// 前端的元数据文件，按文件名而不是扩展名认。
const METADATA_FILE_NAMES: &[&str] = &[
    "metadata.pegasus.txt",
    "metadata.txt",
    "gamelist.xml",
    "miyoogamelist.xml",
    "collections.txt",
];

/// 这个目录是否整棵跳过。
#[must_use]
pub fn is_skipped_system_dir(name_lower: &str) -> bool {
    SKIPPED_SYSTEM_DIRS.contains(&name_lower)
}

/// 这个扩展名是否出现在任何一张归类表里。
///
/// [`container::volume`](crate::container::volume) 也要它：`.z64` 长得像 ZIP split
/// 的第 64 段，而它是 N64 的**裸文件**。认错会把整个 N64 平台记成透明容器。
pub(crate) fn is_known_extension(ext: &str) -> bool {
    TRANSPARENT_CONTAINERS.contains(&ext)
        || COMPRESSED_IMAGES.contains(&ext)
        || BARE_FILES.contains(&ext)
        || MEDIA_OR_METADATA.contains(&ext)
        || DOCUMENTS.contains(&ext)
        || EMULATOR_BINARIES.contains(&ext)
        || PARTIAL_DOWNLOADS.contains(&ext)
}

/// 给一个文件定归类。
#[must_use]
pub fn classify(path: &Path) -> Classification {
    let name = file_name_lower(path);
    let ext = extension_lower(path);
    // **判据只写一处**（[`container::volume`](crate::container::volume)）：归类、成型与
    // 穿透三处看的是同一套规则，抄成三份的话「哪些名字算一段」迟早漂成三个答案。
    // 数的是**非入口段**：`X.part1.rar` 与 `X.zip` 是整组的入口，它们自己就是那个容器，
    // 不算「多出来的一段」。
    let split_volume = crate::container::volume::is_non_entry_part(&name);

    let mut result = Classification {
        category: Category::Unclassified,
        suspect: None,
        split_volume,
    };

    if JUNK_FILE_NAMES.contains(&name.as_str()) || name.starts_with("._") {
        result.suspect = Some(SuspectReason::SystemJunk);
        return result;
    }
    if METADATA_FILE_NAMES.contains(&name.as_str()) {
        result.category = Category::MediaOrMetadata;
        return result;
    }
    let Some(ext) = ext else {
        return result;
    };
    let ext = ext.as_str();

    if TRANSPARENT_CONTAINERS.contains(&ext) {
        result.category = Category::TransparentContainer;
    } else if COMPRESSED_IMAGES.contains(&ext) {
        result.category = Category::CompressedImage;
    } else if BARE_FILES.contains(&ext) {
        result.category = Category::BareFile;
    } else if MEDIA_OR_METADATA.contains(&ext) {
        result.category = Category::MediaOrMetadata;
    } else if DOCUMENTS.contains(&ext) {
        result.suspect = Some(SuspectReason::Document);
    } else if EMULATOR_BINARIES.contains(&ext) {
        result.suspect = Some(SuspectReason::EmulatorBinary);
    } else if PARTIAL_DOWNLOADS.contains(&ext) {
        result.suspect = Some(SuspectReason::PartialDownload);
    } else if crate::container::volume::is_volume_segment(&name) {
        // 表里没有的扩展名才轮到分卷规则，例如 `.7z.001`、`.z01`、`.r00`。
        // **入口段也算**：`.7z.001` 是那一组的入口，它当然是个透明容器。
        result.category = Category::TransparentContainer;
    }

    result
}

/// 文件名里是否含中日韩汉字。
///
/// 这是「库里中文资源占多少」的**文件名层面的粗略代理**，不是识别结论：
/// 真正判定官中版与汉化版要看字节（票 10、票 12）。
#[must_use]
pub fn has_cjk(path: &Path) -> bool {
    path.file_name()
        .map(|name| name.to_string_lossy().chars().any(is_han))
        .unwrap_or(false)
}

/// 这个字符是汉字吗。
///
/// **假名不算**（[`is_kana`] 管那一档）。两条判据住在一起而不是各模块各写一份：
/// 「哪些码位算汉字」是一条会被引用很多次的事实——标题排不排得动（`title::sortable`）、
/// 文件名里哪一段是中文（`filename`）、一串字值不值得拿去撞（`zh`）看的都是它，
/// 而抄成四份之后，补一个码位区间就得记得改四处。
#[must_use]
pub fn is_han(c: char) -> bool {
    matches!(u32::from(c),
        0x3400..=0x4DBF     // 扩展 A
        | 0x4E00..=0x9FFF   // 基本区
        | 0xF900..=0xFAFF   // 兼容汉字
        | 0x20000..=0x2FA1F // 扩展 B 及以后
    )
}

/// 这个字符是假名吗（含半角片假名）。
#[must_use]
pub fn is_kana(c: char) -> bool {
    matches!(u32::from(c),
        0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9D)
}

/// 这个字符是汉字或假名。
#[must_use]
pub fn is_cjk(c: char) -> bool {
    is_han(c) || is_kana(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 归类(name: &str) -> Classification {
        classify(Path::new(name))
    }

    #[test]
    fn zip_与_7z_与_rar_是透明容器() {
        for name in ["a.zip", "a.7z", "a.RAR"] {
            assert_eq!(
                归类(name).category,
                Category::TransparentContainer,
                "{name}"
            );
        }
    }

    #[test]
    fn 压缩镜像不被当成容器() {
        for name in ["a.chd", "a.cso", "a.pbp", "a.rvz", "a.wbfs"] {
            assert_eq!(归类(name).category, Category::CompressedImage, "{name}");
        }
    }

    #[test]
    fn 卡带与光盘镜像是裸文件() {
        for name in ["a.nes", "a.sfc", "a.gba", "a.iso", "a.cue", "a.bin"] {
            assert_eq!(归类(name).category, Category::BareFile, "{name}");
        }
    }

    #[test]
    fn switch_的四种扩展名都是裸文件() {
        // ⭐ `.nsz` / `.xcz` 与 `.nsp` / `.xci` 的容器层完全相同，识别读它们零解压。
        // 它们不在这张表里时**连变体都成不了**——真机上 55 个文件、74.67 GiB
        // （Switch 容量的 33.4%）压根没进过识别管线（票 27）。
        for name in ["a.nsp", "a.xci", "a.nsz", "a.xcz"] {
            assert_eq!(归类(name).category, Category::BareFile, "{name}");
        }
    }

    #[test]
    fn 疑似不该入库的四类各自认得出来() {
        assert_eq!(归类("说明.txt").suspect, Some(SuspectReason::Document));
        assert_eq!(
            归类("retroarch.exe").suspect,
            Some(SuspectReason::EmulatorBinary)
        );
        assert_eq!(
            归类("game.iso.part").suspect,
            Some(SuspectReason::PartialDownload)
        );
        assert_eq!(归类(".DS_Store").suspect, Some(SuspectReason::SystemJunk));
        assert_eq!(归类("._游戏.zip").suspect, Some(SuspectReason::SystemJunk));
    }

    #[test]
    fn 前端元数据文件按文件名认() {
        assert_eq!(
            归类("metadata.pegasus.txt").category,
            Category::MediaOrMetadata
        );
        assert_eq!(归类("gamelist.xml").category, Category::MediaOrMetadata);
        // 同样是 txt，不是元数据文件名的就仍是文档
        assert_eq!(归类("金手指.txt").suspect, Some(SuspectReason::Document));
    }

    #[test]
    fn 认不出扩展名的进未归类() {
        let c = 归类("某文件.qqq");
        assert_eq!(c.category, Category::Unclassified);
        assert_eq!(c.suspect, None);
    }

    #[test]
    fn 分卷压缩的分卷被认出来() {
        for name in ["a.zip.002", "a.z01", "a.r00", "a.part12.rar"] {
            assert!(归类(name).split_volume, "{name}");
            assert_eq!(
                归类(name).category,
                Category::TransparentContainer,
                "{name}"
            );
        }
        // **入口段不算「多出来的一段」**：`.part1.rar` 与 `.7z.001` 自己就是那个容器，
        // 整组由它代表（票 04）。数它们会让「疑似分卷」把一组数两次。
        for name in ["a.part1.rar", "a.7z.001"] {
            assert!(!归类(name).split_volume, "{name} 是入口段");
            assert_eq!(
                归类(name).category,
                Category::TransparentContainer,
                "{name}"
            );
        }
        for name in ["a.zip", "a.rar", "a.z1", "a.001x", "part1.rar"] {
            assert!(
                !归类(name).split_volume || name == "part1.rar",
                "{name} 不该被当成分卷"
            );
        }
    }

    #[test]
    fn n64_的_z64_不是分卷() {
        // `.z64` 长得像「z + 两位数字」，但它是 N64 的裸文件；
        // 认错会把整个 N64 平台记成透明容器
        let c = 归类("超级马里奥64.z64");
        assert_eq!(c.category, Category::BareFile);
        assert!(!c.split_volume);
        // 同理，`.r00` 之外的已知扩展名也不该被吃掉
        assert!(!归类("game.z64").split_volume);
    }

    #[test]
    fn 透明容器与压缩镜像都要有对应的探针() {
        // 加了新的容器或镜像扩展名却忘了写探针，头部抽样就会静悄悄漏掉一整类
        for ext in TRANSPARENT_CONTAINERS.iter().chain(COMPRESSED_IMAGES) {
            let name = format!("样本.{ext}");
            assert!(
                crate::header::probe_class_for(Path::new(&name)).is_some(),
                "{ext} 有归类却没有探针"
            );
        }
    }

    #[test]
    fn 系统目录整棵跳过() {
        assert!(is_skipped_system_dir("$recycle.bin"));
        assert!(is_skipped_system_dir("__macosx"));
        assert!(!is_skipped_system_dir("fc"));
    }

    #[test]
    fn 中文文件名能被认出来() {
        assert!(has_cjk(Path::new("/lib/FC/超级马里奥.zip")));
        assert!(!has_cjk(Path::new("/lib/FC/Super Mario Bros.zip")));
        assert!(!has_cjk(Path::new("/lib/FC/ドラクエ.zip")), "假名不算汉字");
    }
}
