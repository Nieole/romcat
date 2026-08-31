//! 跨平台路径处理。
//!
//! 跨平台是第一天的约束而非后期适配（ADR-0018）：开发在 macOS，主力机是 Windows，
//! 外接硬盘为 NTFS。因此路径处理集中在这里，扫描器不允许出现任何平台专属捷径。
//!
//! Windows 的 `MAX_PATH` 是 260 字符，而 10T 主库里深层目录加中文文件名极易超限。
//! 对策是 `\\?\` 扩展长度前缀：[`long_path`] 在 Windows 上给绝对路径加前缀，
//! 其余平台原样返回。前缀只在真正调用系统 API 时加，报告里展示的仍是原路径。

use std::borrow::Cow;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

/// Windows 的 `MAX_PATH` 限制，单位是字符（UTF-16 码元）。
pub const MAX_PATH: usize = 260;

/// 把一个绝对的 Windows 路径转成 `\\?\` 扩展长度形式。
///
/// 输入必须是已规范化的绝对路径（不含 `.` 与 `..`）——扩展长度路径不做任何规范化，
/// 系统按字面量使用它。已经是扩展形式、或者不是绝对 Windows 路径时返回 `None`。
///
/// 这个函数在所有平台上都会编译，因此 Windows 的路径规则能在 macOS 上被测试覆盖。
#[must_use]
pub fn windows_verbatim(path: &str) -> Option<String> {
    if path.starts_with(r"\\?\") || path.starts_with(r"\\.\") {
        return None;
    }
    if let Some(rest) = path.strip_prefix(r"\\").or_else(|| path.strip_prefix("//")) {
        // UNC：`\\server\share\…` → `\\?\UNC\server\share\…`
        if rest.is_empty() {
            return None;
        }
        return Some(format!(r"\\?\UNC\{}", rest.replace('/', r"\")));
    }
    let bytes = path.as_bytes();
    let is_drive_absolute = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/');
    if is_drive_absolute {
        return Some(format!(r"\\?\{}", path.replace('/', r"\")));
    }
    None
}

/// 需要时给路径加上 Windows 扩展长度前缀；非 Windows 平台原样返回。
///
/// 只有能无损转成 UTF-8 的路径才会被加前缀——Windows 上的非 UTF-8 路径极少，
/// 原样返回至少不会把路径改坏。真正的规则在 [`windows_verbatim`] 里，它在所有平台上
/// 都编译、都被测试覆盖，这里两个分支各自小到不可能出错。
#[cfg(windows)]
#[must_use]
pub fn long_path(path: &Path) -> Cow<'_, Path> {
    match path.to_str().and_then(windows_verbatim) {
        Some(verbatim) => Cow::Owned(PathBuf::from(verbatim)),
        None => Cow::Borrowed(path),
    }
}

/// 需要时给路径加上 Windows 扩展长度前缀；非 Windows 平台原样返回。
#[cfg(not(windows))]
#[must_use]
pub fn long_path(path: &Path) -> Cow<'_, Path> {
    Cow::Borrowed(path)
}

/// 去掉 `\\?\` 扩展长度前缀，还原成人写得出来的那个路径。
///
/// 遍历时用的是加了前缀的路径，但报告里展示、以及数「有没有超过 260 字符」时，
/// 都该按原路径算——前缀是工具加的，不是库里那条路径的一部分。
/// 不带前缀时返回 `None`。
#[must_use]
pub fn strip_windows_verbatim(path: &str) -> Option<String> {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        return Some(format!(r"\\{rest}"));
    }
    path.strip_prefix(r"\\?\").map(ToString::to_string)
}

/// 路径的字符长度，按 Windows 的口径（UTF-16 码元）计算。
///
/// 一个汉字在 UTF-8 里是 3 字节但在 Windows 上只算 1 个字符，
/// 按字节数判断会把大量正常路径误报成超限。
#[must_use]
pub fn char_len(path: &Path) -> usize {
    display(path).chars().map(char::len_utf16).sum()
}

/// 路径是否超过 Windows 的 `MAX_PATH` 限制。
///
/// 超限本身不是错误——加了 `\\?\` 前缀就能正常访问——但它是体检报告要报出来的数字：
/// 库里有多少路径在没有长路径支持的工具下会失败。
#[must_use]
pub fn exceeds_max_path(path: &Path) -> bool {
    char_len(path) > MAX_PATH
}

/// 取出文件相对于扫描根的**平台目录**名。
///
/// 平台由目录给出（ADR-0011：目录是强先验而非权威）。直接躺在库根下的文件没有平台目录，
/// 返回 `None`——它们照常计入报告，平台未知不构成跳过的理由。
#[must_use]
pub fn platform_dir<'a>(root: &Path, path: &'a Path) -> Option<&'a OsStr> {
    let rel = path.strip_prefix(root).ok()?;
    let mut comps = rel.components();
    let first = match comps.next()? {
        Component::Normal(name) => name,
        _ => return None,
    };
    // 只有还有下一级时，第一级才是「目录」而不是文件本身。
    comps.next()?;
    Some(first)
}

/// 文件名的扩展名，转成小写。没有扩展名时返回 `None`。
#[must_use]
pub fn extension_lower(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    if ext.is_empty() { None } else { Some(ext) }
}

/// 文件名转成小写，用于与文件名规则表比对。
#[must_use]
pub fn file_name_lower(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// 尽量把路径化成可比较的绝对形态：从最深的**已存在**的祖先开始规范化，
/// 再把余下的部分接回去。
///
/// 断点文件通常还不存在，`canonicalize` 直接对它会失败；而在 macOS 上
/// `/var` 是指向 `/private/var` 的链接——不化开就会得出「断点不在主库里」这种错判，
/// 而那正是只读边界的守卫要拦的东西。
#[must_use]
pub fn normalize_existing(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut trailing: Vec<std::ffi::OsString> = Vec::new();
    let mut cursor = absolute.as_path();
    loop {
        if let Ok(canonical) = std::fs::canonicalize(long_path(cursor).as_ref()) {
            let mut result = canonical;
            for part in trailing.iter().rev() {
                result.push(part);
            }
            return result;
        }
        let (Some(parent), Some(name)) = (cursor.parent(), cursor.file_name()) else {
            return absolute;
        };
        trailing.push(name.to_os_string());
        cursor = parent;
    }
}

/// 路径的展示形态：去掉工具自己加的扩展长度前缀，非 UTF-8 部分按有损方式转换。
///
/// 展示用，不用于再次访问磁盘。
#[must_use]
pub fn display(path: &Path) -> String {
    let text = path.to_string_lossy();
    strip_windows_verbatim(&text).unwrap_or_else(|| text.into_owned())
}

/// 路径是否可以无损地表示成 UTF-8。
#[must_use]
pub fn is_utf8(path: &Path) -> bool {
    path.to_str().is_some()
}

/// `inner` 是否落在 `outer` 之内（含相等）。
///
/// 用于守住只读边界：断点文件绝不允许写进主库。
#[must_use]
pub fn is_inside(outer: &Path, inner: &Path) -> bool {
    inner.starts_with(outer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 盘符绝对路径加扩展长度前缀() {
        assert_eq!(
            windows_verbatim(r"D:\ROMs\FC\游戏.zip").as_deref(),
            Some(r"\\?\D:\ROMs\FC\游戏.zip")
        );
    }

    #[test]
    fn 正斜杠被换成反斜杠() {
        assert_eq!(
            windows_verbatim("D:/ROMs/FC").as_deref(),
            Some(r"\\?\D:\ROMs\FC")
        );
    }

    #[test]
    fn 网络路径走_unc_形式() {
        assert_eq!(
            windows_verbatim(r"\\nas\share\ROMs").as_deref(),
            Some(r"\\?\UNC\nas\share\ROMs")
        );
    }

    #[test]
    fn 已是扩展形式的路径不再重复加前缀() {
        assert_eq!(windows_verbatim(r"\\?\D:\ROMs"), None);
        assert_eq!(windows_verbatim(r"\\.\PhysicalDrive0"), None);
    }

    #[test]
    fn 相对路径与非_windows_路径不加前缀() {
        assert_eq!(windows_verbatim(r"ROMs\FC"), None);
        assert_eq!(windows_verbatim("/Volumes/ROMs/FC"), None);
        assert_eq!(windows_verbatim("D:"), None);
    }

    #[test]
    fn 扩展长度前缀不出现在报告里也不计入长度() {
        let raw = r"D:\ROMs\FC\游戏.zip";
        let verbatim = windows_verbatim(raw).expect("能加前缀");
        assert_eq!(strip_windows_verbatim(&verbatim).as_deref(), Some(raw));
        assert_eq!(display(Path::new(&verbatim)), raw);
        // 前缀是工具加的，不该让路径凭空多出 4 个字符
        assert_eq!(char_len(Path::new(&verbatim)), char_len(Path::new(raw)));

        let unc = windows_verbatim(r"\\nas\share\ROMs").expect("能加前缀");
        assert_eq!(
            strip_windows_verbatim(&unc).as_deref(),
            Some(r"\\nas\share\ROMs")
        );
        assert_eq!(strip_windows_verbatim("/Volumes/ROMs"), None);
    }

    #[test]
    fn 超长路径按字符而非字节判断() {
        // 90 个汉字 = 270 字节的 UTF-8，但只有 90 个 Windows 字符，不算超限。
        let short = PathBuf::from(format!("D:\\{}", "游".repeat(90)));
        assert!(!exceeds_max_path(&short));
        assert_eq!(char_len(&short), 93);

        let long = PathBuf::from(format!("D:\\{}", "游".repeat(300)));
        assert!(exceeds_max_path(&long));
    }

    #[test]
    fn 平台目录取相对根的第一级() {
        let root = Path::new("/lib");
        assert_eq!(
            platform_dir(root, Path::new("/lib/FC/超级马里奥.zip")),
            Some(OsStr::new("FC"))
        );
        assert_eq!(
            platform_dir(root, Path::new("/lib/PS1/某游戏/disc.cue")),
            Some(OsStr::new("PS1"))
        );
    }

    #[test]
    fn 库根下的散文件没有平台目录() {
        assert_eq!(
            platform_dir(Path::new("/lib"), Path::new("/lib/readme.txt")),
            None
        );
    }

    #[test]
    fn 扩展名统一小写() {
        assert_eq!(
            extension_lower(Path::new("/a/B.ZIP")).as_deref(),
            Some("zip")
        );
        assert_eq!(extension_lower(Path::new("/a/无扩展名")), None);
    }

    #[test]
    fn 未创建的路径也能化成绝对形态() {
        let dir = crate::testing::temp_dir("normalize");
        let target = dir.path().join("scans").join("checkpoint.json");
        let normalized = normalize_existing(&target);
        assert!(normalized.is_absolute());
        assert!(normalized.ends_with("scans/checkpoint.json"));
        // 已存在的那一段被化开了：macOS 上 /var 会变成 /private/var
        assert_eq!(
            normalized,
            normalize_existing(
                &dir.path()
                    .canonicalize()
                    .expect("能化开")
                    .join("scans/checkpoint.json")
            )
        );
    }

    #[test]
    fn 断点路径落在库内能被认出来() {
        assert!(is_inside(
            Path::new("/lib"),
            Path::new("/lib/.romcat/checkpoint.json")
        ));
        assert!(!is_inside(
            Path::new("/lib"),
            Path::new("/home/me/.romcat/checkpoint.json")
        ));
    }
}
