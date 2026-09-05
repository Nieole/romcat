//! 跨平台路径处理。
//!
//! 跨平台是第一天的约束而非后期适配（ADR-0018）：开发在 macOS，主力机是 Windows，
//! 外接硬盘为 NTFS。因此路径处理集中在这里，扫描器不允许出现任何平台专属捷径。
//!
//! Windows 的 `MAX_PATH` 是 260 字符，而 10T 主库里深层目录加中文文件名极易超限。
//! 对策是 `\\?\` 扩展长度前缀：[`long_path`] 在 Windows 上给绝对路径加前缀，
//! 其余平台原样返回。前缀只在真正调用系统 API 时加，报告里展示的仍是原路径。
//!
//! 另一半是**中立库的键**：[`library_key`] 把系统给的路径折成「**根名** + 相对那个根、
//! 分隔符统一成 `/`、再规范化成 NFC」的形式。读盘用系统给的原始路径，入库与比较用键，
//! **两者不能混用**（ADR-0020）。
//!
//! 键的第一段是**根名**，是因为主库是**一组根**（`CONTEXT.md`）：几块盘扫进同一份
//! 中立库，只按相对路径当键的话两块盘上同名的东西会静默覆盖。拆键走 [`split_root`]。

use std::borrow::Cow;
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};

use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfc_quick};

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
pub fn char_len(text: &str) -> usize {
    let bare = strip_windows_verbatim(text);
    let text = bare.as_deref().unwrap_or(text);
    text.chars().map(char::len_utf16).sum()
}

/// 路径是否超过 Windows 的 `MAX_PATH` 限制。
///
/// 超限本身不是错误——加了 `\\?\` 前缀就能正常访问——但它是体检报告要报出来的数字：
/// 库里有多少路径在没有长路径支持的工具下会失败。
#[must_use]
pub fn exceeds_max_path(text: &str) -> bool {
    char_len(text) > MAX_PATH
}

/// 把文本规范化成 NFC（ADR-0020）。
///
/// **macOS 的 NTFS 驱动把文件名规范化成 NFD 之后才交给 `readdir`**：实测 1.99% 的路径
/// 受影响（日文浊音假名、拉丁重音符），其中 5 个是**目录名**——一个目录中招，整棵子树
/// 的键都跟着变。不做这一步，盘在 macOS 与 Windows 之间接一次，约 5,100 个文件会被
/// 判成新文件重扫一遍，而两台机器轮流碰同一块盘正是既定的工作方式（ADR-0018）。
///
/// 已经是 NFC 的原样借用不额外分配——库里 98% 的路径走这条。
#[must_use]
pub fn nfc(text: &str) -> Cow<'_, str> {
    match is_nfc_quick(text.chars()) {
        IsNormalized::Yes => Cow::Borrowed(text),
        _ => Cow::Owned(text.nfc().collect()),
    }
}

/// **相对某个根**的路径键：分隔符统一成 `/`，再规范化成 NFC。
///
/// 三件事各有理由：
///
/// - **相对**：挂载点会变。macOS 上重挂一次就可能从 `/Volumes/新加卷` 变成
///   `/Volumes/新加卷 1`，Windows 上盘符也会变。存绝对路径的话，换一次挂载点
///   **全库**都会被判成新文件——那比 ADR-0020 要防的 5,100 个还糟。
/// - **分隔符统一**：同一块盘在 Windows 上是 `\`、在 macOS 上是 `/`（ADR-0018）。
///   按 [`Component`] 拆再用 `/` 接，于是 Unix 文件名里合法的字面 `\` 不会被误当分隔符。
/// - **NFC**：见 [`nfc`] 与 ADR-0020。
///
/// **中立库的键不是它**——主库是**一组根**，中立库的键还要在前面带上**根名**，
/// 走 [`library_key`]。这一支单独留着，是因为**子库**那一侧也要算「相对目标根的路径」，
/// 而那边没有根名可言（`sync::observe`）。
///
/// **读盘要用系统给的原始路径，只有入库与比较才用这个键**——两者不能混用。
#[must_use]
pub fn catalog_key(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let mut key = String::with_capacity(relative.as_os_str().len());
    for component in relative.components() {
        let Component::Normal(part) = component else {
            continue;
        };
        if !key.is_empty() {
            key.push('/');
        }
        match part.to_str() {
            Some(text) => key.push_str(text),
            // 非 UTF-8 的名字：有损转换会把不同的字节折成同一串 U+FFFD，而键是中立库的
            // 主键——撞上就是一条记录被静默覆盖，事实来源少一个文件（ADR-0001）。
            // 缀一段原始字节的指纹，让不同的名字仍然是不同的键。键只用来认身份，
            // 读盘走的始终是系统给的原始路径。
            None => {
                key.push_str(&part.to_string_lossy());
                let _ = write!(key, "#{:016x}", os_str_fingerprint(part));
            }
        }
    }
    nfc(&key).into_owned()
}

/// 一个**根**的名字不能用的理由。
///
/// 根名是**中立库的键**的第一段（见 [`library_key`]），因此它不是一个纯粹的标签：
/// 它得能与后面的相对路径拼成一条不会撞车、也不会被拆错的键。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootNameError {
    /// 空的，或者去掉两头空白之后是空的。
    Empty,
    /// 带了路径分隔符。键就是按 `/` 拆的，名字里再有一个，「根名」与「相对路径」
    /// 的界线就没了。`\` 一并挡掉：Windows 上它是分隔符。
    Separator,
    /// 带了控制字符。它进得了键，却在报告与界面上看不见——两条键长得一模一样却不相等，
    /// 是最难查的那种撞车。
    Control,
    /// 带了 `=`。命令行上「换某个根的位置」写成 `根名=路径`（`--library-root`），
    /// 从**左边第一个** `=` 切。名字里也有一个的话，那个根就再也点不到名——
    /// `a=b=/新位置` 切出来的左半是 `a`，而库里那个根叫 `a=b`。
    /// **挡在起名这一步**，语法才是全的：库里任何一个根都写得出来。
    Equals,
}

impl std::fmt::Display for RootNameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("根名不能是空的"),
            Self::Separator => f.write_str("根名里不能有 `/` 或 `\\`——它们是键的分隔符"),
            Self::Control => f.write_str("根名里不能有控制字符"),
            Self::Equals => f.write_str(
                "根名里不能有 `=`——`根名=路径` 靠它切开，名字里再有一个就点不到这个根了",
            ),
        }
    }
}

impl std::error::Error for RootNameError {}

/// 把一个**根**的名字折成键里那一段：去掉两头空白，再规范化成 NFC。
///
/// NFC 这一下与 [`catalog_key`] 同源（ADR-0020）：名字是用户敲进来的，而同一个名字在
/// macOS 与 Windows 上敲出来可能一个 NFD 一个 NFC。不折一下，「元数据库」这个根在两台
/// 机器上就是两个根，全库重扫一遍。
///
/// # Errors
/// 名字空、带分隔符、带控制字符或带 `=` 时返回 [`RootNameError`]。
pub fn root_name(name: &str) -> Result<String, RootNameError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(RootNameError::Empty);
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(RootNameError::Separator);
    }
    if trimmed.chars().any(char::is_control) {
        return Err(RootNameError::Control);
    }
    if trimmed.contains('=') {
        return Err(RootNameError::Equals);
    }
    Ok(nfc(trimmed).into_owned())
}

/// **中立库里一条记录的键**：`根名` + `/` + 相对根的路径。
///
/// 主库是**一组根**（`CONTEXT.md`）：几块盘、几个目录扫进同一份中立库。只按相对路径
/// 当键的话，两块盘上同名的 `FC/魂斗罗.zip` 会是同一条记录——**一条静默覆盖另一条**，
/// 而中立库是事实来源（ADR-0001）。带上根名，两个根的变体从此互不覆盖，
/// 而且每条键自己说得出它来自哪块盘。
///
/// **根自己的键就是根名**（相对路径是空串）。
///
/// 拼接不会破坏 NFC：中间那个 `/` 不参与任何组合，两侧各自已经是 NFC。
#[must_use]
pub fn library_key(root_name: &str, root: &Path, path: &Path) -> String {
    join_root(root_name, &catalog_key(root, path))
}

/// 把根名与**相对根的路径**接成中立库的键。
#[must_use]
pub fn join_root(root_name: &str, relative: &str) -> String {
    if relative.is_empty() {
        return root_name.to_string();
    }
    let mut key = String::with_capacity(root_name.len() + 1 + relative.len());
    key.push_str(root_name);
    key.push('/');
    key.push_str(relative);
    key
}

/// 把中立库的键拆成 `(根名, 相对根的路径)`。根自己那条键拆出来的相对路径是空串。
#[must_use]
pub fn split_root(key: &str) -> (&str, &str) {
    match key.split_once('/') {
        Some((name, rest)) => (name, rest),
        None => (key, ""),
    }
}

/// 中立库的键属于哪个**根**。
#[must_use]
pub fn root_of_key(key: &str) -> &str {
    split_root(key).0
}

/// 中立库的键去掉根名之后剩下的那一截，**相对根**。
#[must_use]
pub fn relative_of_key(key: &str) -> &str {
    split_root(key).1
}

/// 一段 `OsStr` 原始码元的 FNV-1a 指纹。
///
/// 只要「不同的字节大概率给出不同的值」，不需要密码学强度。两个平台的码元宽度不同，
/// 因此各走各的分支——这不是平台捷径，是两边真实不同的东西（ADR-0018）。
#[cfg(unix)]
fn os_str_fingerprint(part: &OsStr) -> u64 {
    use std::os::unix::ffi::OsStrExt;
    fnv1a(part.as_bytes().iter().map(|byte| u32::from(*byte)))
}

/// 一段 `OsStr` 原始码元的 FNV-1a 指纹。
#[cfg(windows)]
fn os_str_fingerprint(part: &OsStr) -> u64 {
    use std::os::windows::ffi::OsStrExt;
    fnv1a(part.encode_wide().map(u32::from))
}

/// 一段 `OsStr` 原始码元的 FNV-1a 指纹。
#[cfg(not(any(unix, windows)))]
fn os_str_fingerprint(part: &OsStr) -> u64 {
    fnv1a(part.to_string_lossy().chars().map(u32::from))
}

fn fnv1a(units: impl Iterator<Item = u32>) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for unit in units {
        for byte in unit.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
    }
    hash
}

/// 把中立库的键还原成给人看的完整路径。
///
/// `root` 是这条键**那个根**在盘上的位置：键的第一段是根名，由它顶替掉。
/// 拿哪个根，由调用方按 [`root_of_key`] 查（`catalog::roots::Roots::display_key`）。
///
/// 分隔符跟着 `root` 走而不是跟着当前平台走：在 macOS 上看一份上次在 Windows 上扫出来
/// 的中立库时，`D:\Game\FC\…` 比 `D:\Game/FC/…` 更像那台机器上的真实路径。
#[must_use]
pub fn display_key(root: &str, key: &str) -> String {
    let key = relative_of_key(key);
    if key.is_empty() {
        return root.to_string();
    }
    let separator = if root.contains('\\') { '\\' } else { '/' };
    let root = root.trim_end_matches(['/', '\\']);
    let mut out = String::with_capacity(root.len() + key.len() + 1);
    out.push_str(root);
    out.push(separator);
    for (index, part) in key.split('/').enumerate() {
        if index > 0 {
            out.push(separator);
        }
        out.push_str(part);
    }
    out
}

/// 把一段名字折成可比较的形式：先小写，再规范化成 NFC。
///
/// 两件事都不能少。小写是因为同一个 TitleID 目录在不同转储工具下写成 `PCSG00042` 或
/// `pcsg00042`；NFC 的理由与键同源（ADR-0020）——macOS 的 NTFS 驱动交出来的名字是 NFD，
/// 而平台清单里写的多半是 NFC，不折一下的话带浊音假名的目录名会对不上。
#[must_use]
pub fn fold(text: &str) -> String {
    nfc(&text.to_lowercase()).into_owned()
}

/// 取出一个键的**平台目录**名。
///
/// 平台由目录给出（ADR-0011：目录是强先验而非权威）。键的第一段是**根名**，
/// 所以平台在第二段：`元数据库/FC/魂斗罗.zip` 的平台目录是 `FC`。直接躺在某个根下面
/// 的文件没有平台目录，返回 `None`——它们照常计入报告，平台未知不构成跳过的理由。
#[must_use]
pub fn platform_of_key(key: &str) -> Option<&str> {
    let (head, rest) = split_root(key).1.split_once('/')?;
    (!head.is_empty() && !rest.is_empty()).then_some(head)
}

/// 取出一个键的**平台目录**那一段键，`根名/平台`。
///
/// 与 [`platform_of_key`] 的区别只在带不带根名：成型要拿它与 `parent_of` 出来的
/// 那一截比（`shape::climb`），而那一截是**完整的键**，比名字对不上。
#[must_use]
pub fn platform_dir_of_key(key: &str) -> Option<&str> {
    let platform = platform_of_key(key)?;
    let root = root_of_key(key);
    Some(&key[..root.len() + 1 + platform.len()])
}

/// 键的文件名部分。
///
/// 只按 `/` 拆：键里的分隔符只有 `/`，而 `\` 在 Windows 上是分隔符、在 Unix 上是合法
/// 的文件名字符——交给 `Path` 去拆会在两个平台上给出不同答案，而中立库的键必须两边一致。
#[must_use]
pub fn file_name_of_key(key: &str) -> &str {
    key.rsplit('/').next().unwrap_or(key)
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
        assert_eq!(char_len(&verbatim), char_len(raw));

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
        let short = format!("D:\\{}", "游".repeat(90));
        assert!(!exceeds_max_path(&short));
        assert_eq!(char_len(&short), 93);

        let long = format!("D:\\{}", "游".repeat(300));
        assert!(exceeds_max_path(&long));
    }

    #[test]
    fn 平台目录取根名之后那一级() {
        // 第一段是**根名**，平台在它后面（`library_key`）。
        assert_eq!(platform_of_key("库/FC/超级马里奥.zip"), Some("FC"));
        assert_eq!(platform_of_key("库/PS1/某游戏/disc.cue"), Some("PS1"));
        assert_eq!(platform_dir_of_key("库/PS1/某游戏/disc.cue"), Some("库/PS1"));
        // 直接躺在某个根下面的文件没有平台目录；根自己那条键也没有。
        assert_eq!(platform_of_key("库/散落的游戏.gba"), None);
        assert_eq!(platform_of_key("库"), None);
        assert_eq!(platform_dir_of_key("库/散落的游戏.gba"), None);
    }

    #[test]
    fn 根名不许带分隔符也不许带控制字符() {
        assert_eq!(root_name("  主库  ").as_deref(), Ok("主库"));
        assert_eq!(root_name(""), Err(RootNameError::Empty));
        assert_eq!(root_name("   "), Err(RootNameError::Empty));
        assert_eq!(root_name("甲/乙"), Err(RootNameError::Separator));
        assert_eq!(root_name(r"甲\乙"), Err(RootNameError::Separator));
        assert_eq!(root_name("甲\u{7}乙"), Err(RootNameError::Control));
        // 名字也要折成 NFC（ADR-0020）：两台机器敲同一个名字才是同一个根。
        assert_eq!(root_name(分解).as_deref(), root_name(预组合).as_deref());
    }

    #[test]
    fn 根名不许带等号否则那个根点不到名() {
        // `--library-root 根名=路径` 从**左边第一个** `=` 切。名字里也有一个的话，
        // `a=b=/新位置` 切出来的左半是 `a`，而库里那个根叫 `a=b`——再也点不到它。
        assert_eq!(root_name("a=b"), Err(RootNameError::Equals));
        assert!(root_name("a=b").unwrap_err().to_string().contains('='));
        // Windows 那两种写法里一个 `=` 都没有，照旧当路径走，不受这条影响。
        assert_eq!(root_name("主库").as_deref(), Ok("主库"));
        assert_eq!(root_name(r"C:").as_deref(), Ok("C:"));
    }

    #[test]
    fn 中立库的键带着根名而且拆得回来() {
        let key = library_key("元数据库", Path::new("/盘乙"), Path::new("/盘乙/FC/魂斗罗.zip"));
        assert_eq!(key, "元数据库/FC/魂斗罗.zip");
        assert_eq!(split_root(&key), ("元数据库", "FC/魂斗罗.zip"));
        // 根自己那条键就是它的名字。
        assert_eq!(
            library_key("元数据库", Path::new("/盘乙"), Path::new("/盘乙")),
            "元数据库"
        );
        assert_eq!(split_root("元数据库"), ("元数据库", ""));
        // 两块盘上同名的东西不再是同一条键——那正是这一层要挡住的静默覆盖。
        assert_ne!(
            library_key("甲", Path::new("/盘甲"), Path::new("/盘甲/FC/魂斗罗.zip")),
            library_key("乙", Path::new("/盘乙"), Path::new("/盘乙/FC/魂斗罗.zip")),
        );
    }

    #[test]
    fn 根名与相对路径接起来仍然是_nfc() {
        // 中间那个 `/` 不参与任何组合，两侧各自已经是 NFC（ADR-0020）。
        let key = library_key(
            &root_name(分解).expect("这个名字能用"),
            Path::new("/盘"),
            Path::new(&format!("/盘/{分解}/游戏.zip")),
        );
        assert_eq!(nfc(&key), key);
        assert_eq!(key, format!("{预组合}/{预组合}/游戏.zip"));
    }

    #[test]
    fn 折出来的名字小写且是_nfc() {
        assert_eq!(fold("PCSG00042"), "pcsg00042");
        // 「が」的分解形折完之后要与预组合形相等
        assert_eq!(fold(分解), fold(预组合));
    }

    #[test]
    fn 根下的散文件没有平台目录() {
        assert_eq!(platform_of_key("库/readme.txt"), None);
        assert_eq!(platform_of_key(""), None);
    }

    /// 「が」有两种写法：预组合的 U+304C，与「か」加组合浊音符 U+3099。
    /// macOS 的 NTFS 驱动交出来的是后者，Windows 上存的是前者。
    const 预组合: &str = "\u{304C}";
    const 分解: &str = "\u{304B}\u{3099}";

    #[test]
    fn 分解形的假名被规范化成预组合形() {
        assert_ne!(预组合, 分解, "两种写法的字节本来就不同");
        assert_eq!(nfc(分解), 预组合);
        assert_eq!(nfc(预组合), 预组合);
    }

    #[test]
    fn 已是_nfc_的文本不额外分配() {
        assert!(matches!(nfc("FC/超级马里奥.zip"), Cow::Borrowed(_)));
        assert!(matches!(nfc(分解), Cow::Owned(_)));
    }

    #[test]
    fn 键相对扫描根且规范化成_nfc() {
        let root = Path::new("/Volumes/新加卷/Game");
        let path = PathBuf::from(format!("/Volumes/新加卷/Game/PSP/{分解}me.iso"));
        assert_eq!(catalog_key(root, &path), format!("PSP/{预组合}me.iso"));
    }

    #[test]
    fn 换个挂载点键不变() {
        // 这正是不存绝对路径的理由：macOS 重挂一次盘名就可能变。
        let a = catalog_key(
            Path::new("/Volumes/新加卷"),
            Path::new("/Volumes/新加卷/FC/a.zip"),
        );
        let b = catalog_key(
            Path::new("/Volumes/新加卷 1"),
            Path::new("/Volumes/新加卷 1/FC/a.zip"),
        );
        assert_eq!(a, "FC/a.zip");
        assert_eq!(a, b);
    }

    #[cfg(unix)]
    #[test]
    fn 两个非_utf8_的名字不会折成同一个键() {
        use std::os::unix::ffi::OsStrExt;
        // 有损转换会把这两个都变成同一串 U+FFFD。键是中立库的主键，撞上就是
        // 一条记录被静默覆盖。
        let root = Path::new("/lib");
        let a = Path::new("/lib/FC").join(OsStr::from_bytes(b"\xff\xfe.zip"));
        let b = Path::new("/lib/FC").join(OsStr::from_bytes(b"\xfe\xff.zip"));
        assert_eq!(
            a.to_string_lossy(),
            b.to_string_lossy(),
            "有损转换分不开它们"
        );

        let key_a = catalog_key(root, &a);
        let key_b = catalog_key(root, &b);
        assert_ne!(key_a, key_b);
        assert_eq!(key_a, catalog_key(root, &a), "同一个名字每次给出同一个键");
    }

    #[test]
    fn 键能还原成给人看的路径() {
        // 键的第一段是根名，由 `root` 那条路径顶替掉。
        assert_eq!(display_key("/lib", "库/FC/a.zip"), "/lib/FC/a.zip");
        assert_eq!(display_key("/lib/", "库/FC/a.zip"), "/lib/FC/a.zip");
        // 在 macOS 上看 Windows 扫出来的中立库，分隔符跟着根走
        assert_eq!(display_key(r"D:\Game", "库/FC/a.zip"), r"D:\Game\FC\a.zip");
        // 根自己那条键还原出来就是那个根。
        assert_eq!(display_key("/lib", "库"), "/lib");
    }

    #[test]
    fn 键的文件名只按斜杠拆() {
        assert_eq!(file_name_of_key("FC/子目录/游戏.zip"), "游戏.zip");
        assert_eq!(file_name_of_key("游戏.zip"), "游戏.zip");
        // Unix 上 `\` 是合法的文件名字符，不能当分隔符
        assert_eq!(file_name_of_key(r"FC/a\b.zip"), r"a\b.zip");
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
