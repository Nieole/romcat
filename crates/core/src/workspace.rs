//! 工作目录：中立库与断点住在这里，将来的媒体池也是。
//!
//! **它必须在本机而不是外置盘上**（ADR-0009）。外置盘不常挂载，中立库若跟着盘走，
//! 盘不在时连浏览元数据都做不到——而扫描是唯一真正需要盘在位的操作。
//!
//! 一个主库一份文件：中立库的键是**相对**主库根的路径（ADR-0020），两个主库的记录
//! 混进同一张表会直接撞车。文件名里既留原目录名（人能认出是哪块盘）又带路径的哈希
//! （两块盘的最后一级恰好同名时不会互相覆盖）。
//!
//! 那个哈希取的是**化开之后的绝对路径**，不是用户敲进来的那一串字：尾斜杠、`.`、
//! `..`、相对路径、符号链接一律先化开（[`path::normalize_existing`]），再剥掉 Windows
//! 的 `\\?\` 前缀、折成 NFC。同一个目录换个写法就另开一份中立库、而 `.` 又在任何目录下
//! 都指向同一份，是这一步没做时的两个症状。
//!
//! ## 名字优先于路径
//!
//! 但跟着路径走本身还有一个致命处，化开也治不了：macOS 重挂一次盘就可能从
//! `/Volumes/新加卷` 变成 `/Volumes/新加卷 1`，Windows 上盘符也会变——换了挂载点就找不到
//! 原来那份中立库，全库白扫一遍。而 ADR-0018 定的工作方式正是盘在两台机器之间来回接，
//! 所以这事会反复发生。
//!
//! 于是加了 [`Slug::Named`]：`--library <名字>` 一给，中立库就跟名字走而不跟路径走。
//! 键本来就是相对的（ADR-0020），换挂载点对键没有任何影响，只有「找得到那份库」这一步
//! 之前还挂在绝对路径上。不给名字时维持老行为，已有的库照样打得开。

use std::env;
use std::path::{Path, PathBuf};

use crate::path;

/// 默认工作目录。
///
/// 一条链跨平台通用、没有 `cfg` 分支：`$ROMCAT_HOME` 优先，其次各平台的数据目录。
/// `%APPDATA%` 排在 `$XDG_DATA_HOME` 前面，因为主力机是 Windows（ADR-0018），
/// 而 Windows 上偶尔也会有别的工具设上 `XDG_DATA_HOME`。
#[must_use]
pub fn default_dir() -> PathBuf {
    for key in ["ROMCAT_HOME", "APPDATA", "XDG_DATA_HOME"] {
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

/// 一份中立库在工作目录里叫什么。
///
/// 两种取法，差别只在「跟什么走」：[`Slug::Named`] 跟用户起的名字走，
/// [`Slug::AtPath`] 跟主库那个目录走。名字那条是为了换挂载点还能找回同一份库。
///
/// **两个 [`Slug`] 相等与它们开的是不是同一份中立库是两回事**：`AtPath` 存的是用户
/// 敲进来的那一串字，`x`、`x/`、`.` 各不相等却指着同一个目录，[`Slug::text`] 会把它们
/// 折成同一个名字。要判「是不是同一份库」，比 [`Slug::text`]，别比 [`Slug`] 本身。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slug<'a> {
    /// 用户用 `--library` 起的名字。换挂载点、换盘符都不影响。
    Named(&'a str),
    /// 没起名字时的老办法：跟主库那个目录走。存的是用户敲进来的原串，
    /// 化成唯一的绝对路径是 [`Slug::text`] 的事。
    AtPath(&'a Path),
}

impl<'a> Slug<'a> {
    /// 从可选的名字与主库根挑一种。
    #[must_use]
    pub fn pick(name: Option<&'a str>, root: &'a Path) -> Self {
        match name {
            Some(name) => Self::Named(name),
            None => Self::AtPath(root),
        }
    }

    /// 落到文件名上的那一串：`{认得出的那一半}-{哈希}`。
    ///
    /// 哈希取自完整的键（名字，或主库那个目录**化开之后**的绝对路径），保证「不同的键
    /// 几乎必然不同名、同一个键无论怎么敲都同名」；前半截只为人能一眼认出是哪份库。
    /// **哈希后缀还顺带挡掉 Windows 的保留设备名**——`CON` 会变成 `CON-xxxxxxxx`，
    /// 不再是保留名。
    ///
    /// **[`Self::AtPath`] 这一支会碰磁盘**（`canonicalize` 与**工作目录**），所以它既不是
    /// 纯函数，同一个 [`Slug`] 在盘挂上前后也可能给出不同答案。规范化落在这里而不落在
    /// 各个调用方，是因为调用方有三处（命令行两处、界面一处），漏一处就又是一份对不上的
    /// 中立库；而这里一处改完，连**沉淀库**里那些**路径锚**记的主库名也跟着一起对上了
    /// （`site::Site::open`）。代价是每次调用一趟系统调用，而它只在开一份现场、拼一个
    /// **断点**文件名时走到，不在任何热路径上。
    ///
    /// 两条路的**可读那一半用的过滤不一样**，这不是疏忽：
    ///
    /// - [`Self::AtPath`] 必须一个字符都不变，否则已有的中立库全都对不上名字——
    ///   而那正是这次要治的病。所以它照旧只留 ASCII 字母数字：
    ///   `/Volumes/新加卷/漫画` 折出来是 `library-…`，难看，但**是老库的名字**。
    /// - [`Self::Named`] 是这次新加的，没有向后兼容的包袱，于是放汉字过去：
    ///   `--library 主库` 折出来是 `主库-…`，人一眼认得出。
    #[must_use]
    pub fn text(self) -> String {
        match self {
            // 名字先规范化成 NFC 再取哈希（ADR-0020）。`--library` 存在的理由正是
            // 「盘在 macOS 与 Windows 之间来回接还能找回同一份库」，而两台机器交出来的
            // 同一个名字可能一个 NFD 一个 NFC——名字里带假名浊音或拉丁重音符的话，
            // 不规范化就会折出两个文件名、两份中立库，静默全库重扫。
            //
            // 名字那条不带 `-at-` 之类的前缀：两条路取的哈希输入不同（名字 vs 绝对路径），
            // 撞上同一个 16 位十六进制串的概率与随机撞名同量级，不值得为它牺牲可读性。
            Self::Named(name) => {
                let name = path::nfc(name);
                slug_from(&name, &readable(&name, char::is_alphanumeric))
            }
            // 路径先化成**一条唯一的绝对路径**再取哈希。同一个根用户敲得出好几种写法
            // ——`$S/lib`、补全补上尾斜杠的 `$S/lib/`、`cd` 进去之后的 `.`、以及夹了
            // `..` 的绕法——直接哈希那一串字的话，每一种写法都是一份新的中立库，
            // 扫完再开一次是空的。更糟的是 `.`：它在**任何**目录下都是同一个字符，
            // 于是几个毫不相干的目录被收成同一个主库底下的几个根，而中立库是事实来源
            // （ADR-0001）。
            //
            // 化开这一步顺带解掉符号链接（macOS 的 `/var` → `/private/var`），
            // 与只读边界那道守卫看的是同一条形态（`path::normalize_existing`）。
            // 根还不存在时只化得开已存在的那一段，余下原样接回去——打错字、盘没挂上
            // 都走这条，不能炸。
            Self::AtPath(root) => {
                let root = path::normalize_existing(root);
                // Windows 上 `canonicalize` 交出来的是 `\\?\D:\…`，那个前缀是工具与
                // 系统之间的事，不该进文件名：剥掉之后 `D:\ROMs` 无论用户敲哪种形式
                // 递进来都折出同一份库。NFC 与 `Self::Named` 那支同源（ADR-0020）
                // ——盘在 macOS 与 Windows 之间来回接，同一个目录名一台交出 NFD、
                // 一台交出 NFC，不折一下就是两份中立库。
                let text = path::nfc(&path::display(&root)).into_owned();
                // 人认得出的那一半取**化开之后**的末级目录名：`.` 与尾斜杠自己没有
                // 名字，化开之后才拿得到真正那个目录叫什么。
                let last = root
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let last = path::nfc(&last);
                slug_from(&text, &readable(&last, |c| c.is_ascii_alphanumeric()))
            }
        }
    }
}

/// 认得出是哪份库的那一半。
///
/// 过滤到只剩 `keep` 放行的字符加 `-` `_`，于是名字里写什么都造不出非法文件名——
/// `/ \ : * ? " < > |` 与控制字符一个都过不去。一个都不剩时退成 `library`。
fn readable(text: &str, keep: impl Fn(char) -> bool) -> String {
    let name: String = text
        .chars()
        .filter(|c| keep(*c) || *c == '-' || *c == '_')
        .take(24)
        .collect();
    if name.is_empty() {
        "library".to_string()
    } else {
        name
    }
}

fn slug_from(key: &str, readable: &str) -> String {
    // FNV-1a 的变体：乘数比正牌 FNV 的 `0x100000001b3` 多了一位十六进制，是票 01 留下的
    // 笔误。**不改**——改了已有的中立库全都对不上名字，而那正是这次要治的病。这里要的
    // 只是「不同的键大概率不同名」，任何奇数乘数都给得出，不需要密码学强度。
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{readable}-{hash:016x}")
}

/// 某个主库的**中立库**文件。
#[must_use]
pub fn catalog_path(workspace: &Path, slug: Slug<'_>) -> PathBuf {
    workspace
        .join("catalog")
        .join(format!("{}.sqlite3", slug.text()))
}

/// **DAT 仓库**在哪。
///
/// 注意它**不带 [`Slug`]**——这不是疏忽。中立库一个主库一份（键是相对主库根的路径，
/// ADR-0020），而「世上有哪些发行版」对两块盘是同一份。跟着主库分开存，等于把一百多 MB
/// 的 DAT 存两遍，而且第二块盘接上来还要重新同步一遍。
#[must_use]
pub fn dat_repo_path(workspace: &Path) -> PathBuf {
    workspace.join("dat").join("dat.sqlite3")
}

/// 取回来的原件放哪。
///
/// 留着原件是有用的：改一条 DAT→平台的映射之后重新入库，不必把那个 106 MB 的整包
/// 再下一遍——与 `romcat shape` 改一条成型规则不必重扫 8.6 TiB 是同一个道理（ADR-0022）。
#[must_use]
pub fn dat_cache_dir(workspace: &Path) -> PathBuf {
    workspace.join("dat").join("cache")
}

/// **中文离线数据源**的本机索引在哪。
///
/// 与 DAT 库一样**不带 [`Slug`]**，理由也一样：「世上有哪些游戏、中文叫什么」对两块盘
/// 是同一份。跟着主库分开存，等于把同一份索引存两遍，第二块盘接上来还要再取一次
/// 435 MB 的 dump。
#[must_use]
pub fn zh_store_path(workspace: &Path) -> PathBuf {
    workspace.join("zh").join("zh.sqlite3")
}

/// 中文数据源取回来的原件放哪。
///
/// 留着原件的理由与 [`dat_cache_dir`] 一模一样：改一条平台别名之后重建索引，
/// 不必把那 435 MB 再下一遍。
#[must_use]
pub fn zh_cache_dir(workspace: &Path) -> PathBuf {
    workspace.join("zh").join("cache")
}

/// **第三方 TitleID 数据库**的本机索引在哪（票 27）。
///
/// 与 DAT 库、中文索引一样**不带 [`Slug`]**，理由也一样：「哪个 ContentId 属于哪个
/// 游戏的哪个版本」对两块盘是同一份。跟着主库分开存，等于把同一份索引存两遍，
/// 第二块盘接上来还要再取一次几百 MB。
#[must_use]
pub fn titledb_store_path(workspace: &Path) -> PathBuf {
    workspace.join("titledb").join("titledb.sqlite3")
}

/// titledb 取回来的原件放哪。
///
/// 留着原件的理由与 [`dat_cache_dir`] 一模一样：改一版折算规则之后重建索引，
/// 不必把那两三百 MB 再下一遍。
#[must_use]
pub fn titledb_cache_dir(workspace: &Path) -> PathBuf {
    workspace.join("titledb").join("cache")
}

/// **沉淀库**在哪。
///
/// 与 DAT 库、媒体池一样**不带 [`Slug`]**，理由也同源：一条**裁决**说的是「世上这份
/// 内容是什么」，与它躺在哪块盘上无关——键是内容哈希不是路径。两块盘接同一台机器，
/// 裁决一次两边都受益。
///
/// 它**单独一份文件、不进中立库**：中立库里每一条都可再生（重扫、重成型、重识别），
/// 所以那边的结构版本一变就让用户删库重扫；而**裁决不可再生**，两者住在一起，那条便宜
/// 的路就再也走不通了（原挂账 D26，`verdict` 模块文档）。
#[must_use]
pub fn verdict_store_path(workspace: &Path) -> PathBuf {
    workspace.join("verdict").join("verdict.sqlite3")
}

/// **媒体池**在哪。
///
/// 与 DAT 库一样**不带 [`Slug`]**，理由也一样：池是**内容寻址**的，同一张封面在两块盘
/// 上算出来是同一个哈希，跟着主库分开存等于把同一份内容存两遍。而映射（谁引用了哪一份）
/// 落在各自的中立库里，两块盘互不干扰。
///
/// 它**必须在本机**（ADR-0009）：外置盘不常挂载，池若跟着盘走，盘不在时连看一眼封面
/// 都做不到。
#[must_use]
pub fn media_pool_dir(workspace: &Path) -> PathBuf {
    workspace.join("media")
}

/// **界面的版式偏好**：面板边界各自拖到哪儿了。
///
/// 它落在工作目录里而**不落进中立库**，这不是随手挑的位置：中立库整份可再生
/// （重扫、重成型、重识别都能把它造回来），所以那边的结构版本一变就让人删库重扫；
/// 界面偏好放进去，会被某一次重扫连人拖了半天的版式一起抹掉。
///
/// 与 DAT 库、媒体池一样**不带 [`Slug`]**：面板拖到哪儿是**这个人这块屏**的事，
/// 与他今天开的是哪一份库无关。跟着库分开存，等于换一份库就把版式忘一次。
///
/// **它随时可以删**：删了就回到默认版式，库里一个字节都不动。
#[must_use]
pub fn gui_layout_path(workspace: &Path) -> PathBuf {
    workspace.join("gui").join("layout.txt")
}

/// 某个主库里**某一个根**的断点文件。
///
/// 带根名，不是疏忽也不是洁癖：主库是一组根，一趟扫描只走其中一个
/// （`CONTEXT.md` 的**根**）。一份 `--library` 底下的几个根共用一个断点文件的话，
/// 扫乙盘会覆盖掉甲盘扫到一半的进度，`--resume` 还会拿甲盘的断点去对乙盘的根，
/// 撞出一句 `RootMismatch` 让整趟直接失败。
///
/// 根名先过 [`Slug::Named`] 那套过滤：它落到文件名上，`/ \ : * ? " < > |` 一个都过不去。
#[must_use]
pub fn checkpoint_path(workspace: &Path, slug: Slug<'_>, root_name: &str) -> PathBuf {
    checkpoint_path_of(workspace, &slug.text(), root_name)
}

/// 同上，只是主库那一半**已经折成了名字**（[`Slug::text`] 的产物）。
///
/// 开完一份现场之后，手上剩的就是那串名字而不是 [`Slug`]
/// （`site::Site::library`）。再包一次 [`Slug::Named`] 会哈希两遍、折出第二个文件名，
/// 于是界面写下的断点与命令行 `--resume` 要找的那一个对不上。拼法只此一处，两个入口
/// 走的是同一行代码。
#[must_use]
pub fn checkpoint_path_of(workspace: &Path, library: &str, root_name: &str) -> PathBuf {
    workspace.join("scans").join(format!(
        "{library}.{}.checkpoint.json",
        Slug::Named(root_name).text()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 不同主库的中立库与断点互不覆盖() {
        let workspace = PathBuf::from("/work");
        let a = Slug::AtPath(Path::new("/Volumes/ROMs"));
        let b = Slug::AtPath(Path::new("/Volumes/ROMs2"));
        assert_ne!(catalog_path(&workspace, a), catalog_path(&workspace, b));
        assert_ne!(
            checkpoint_path(&workspace, a, "根"),
            checkpoint_path(&workspace, b, "根")
        );
    }

    #[test]
    fn 同一份库里两个根的断点互不覆盖() {
        // 一趟扫描只走一个根。共用一个断点文件的话，扫乙盘会盖掉甲盘扫到一半的进度，
        // 而 `--resume` 会拿甲盘的断点去对乙盘的根，撞出一句「扫描根对不上」。
        let workspace = PathBuf::from("/work");
        let slug = Slug::Named("主库");
        assert_ne!(
            checkpoint_path(&workspace, slug, "甲盘"),
            checkpoint_path(&workspace, slug, "乙盘")
        );
        // 根名里的分隔符照样造不出非法文件名（同 `Slug::Named`）。
        let 怪名字 = checkpoint_path(&workspace, slug, "../../etc/passwd");
        assert_eq!(怪名字.parent(), Some(workspace.join("scans").as_path()));
    }

    #[test]
    fn 末级同名的两块盘也分得开() {
        let workspace = PathBuf::from("/work");
        let a = catalog_path(&workspace, Slug::AtPath(Path::new("/Volumes/甲/Game")));
        let b = catalog_path(&workspace, Slug::AtPath(Path::new("/Volumes/乙/Game")));
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
        assert!(!catalog_path(&workspace, Slug::AtPath(root)).starts_with(root));
        assert!(!checkpoint_path(&workspace, Slug::AtPath(root), "根").starts_with(root));
    }

    #[test]
    fn 起了名字之后换挂载点还是同一份中立库() {
        // macOS 重挂一次盘就可能从 `/Volumes/新加卷` 变成 `/Volumes/新加卷 1`，
        // Windows 上盘符也会变。名字一给，这些都不影响找得到哪份库（挂账 D16）。
        let workspace = PathBuf::from("/work");
        let 挂在甲 = catalog_path(
            &workspace,
            Slug::pick(Some("主库"), Path::new("/Volumes/新加卷/Game")),
        );
        let 挂在乙 = catalog_path(
            &workspace,
            Slug::pick(Some("主库"), Path::new("/Volumes/新加卷 1/Game")),
        );
        let 盘符 = catalog_path(&workspace, Slug::pick(Some("主库"), Path::new("E:\\Game")));
        assert_eq!(挂在甲, 挂在乙);
        assert_eq!(挂在甲, 盘符);
        assert_eq!(
            checkpoint_path(&workspace, Slug::Named("主库"), "根"),
            checkpoint_path(
                &workspace,
                Slug::pick(Some("主库"), Path::new("E:\\Game")),
                "根"
            )
        );
    }

    #[test]
    fn 不给名字时维持老行为() {
        let root = Path::new("/Volumes/新加卷/Game");
        assert_eq!(Slug::pick(None, root), Slug::AtPath(root));
        // 已有的库不能因为这次改动就打不开了：**这几串是票 01 那版算法的输出，钉死**。
        // 化开路径这一步没把它们动掉，因为它们本来就已经是**绝对、规范、无尾斜杠、
        // 不经符号链接**的写法——化开之后还是它自己。老库对得上的正好就是这一类；
        // 当初拿尾斜杠、`.` 或相对路径开出来的那些库，名字会变，得手动改文件名。
        //
        // 只在 Unix 上钉：`/Volumes/…` 在 Windows 上压根不是绝对路径（没有盘符前缀），
        // 会被接到当前**工作目录**后面，钉一个跟着机器变的串没有意义。
        #[cfg(unix)]
        {
            let workspace = PathBuf::from("/work");
            assert_eq!(
                catalog_path(&workspace, Slug::AtPath(root)),
                PathBuf::from("/work/catalog/Game-3a0183855885f3a5.sqlite3")
            );
            // 末级目录名不含 ASCII 字母数字的老库：可读那一半整个被滤光，退成 `library`。
            // 名字那条放汉字过去，路径这条**不许跟着放**——放了就是另一个文件名。
            assert_eq!(
                Slug::AtPath(Path::new("/Volumes/新加卷/漫画")).text(),
                "library-39a9fc87af2531b6"
            );
            assert_eq!(
                Slug::AtPath(Path::new("/Volumes/新加卷")).text(),
                "library-f88dd3e91cc9873a"
            );
        }
    }

    #[test]
    fn 同一个根的几种写法开的是同一份中立库() {
        // 用户敲同一个根有很多种敲法：zsh 补全给目录补一个尾斜杠、路径里夹一段 `.`
        // 或 `..`。哈希取的是**那一串字**的话，每一种写法都折出一份新的中立库——
        // 扫完再开一次发现是空的，8.6 TiB 白扫一遍。所以哈希取的必须是化开之后的
        // 绝对路径。
        let 临时 = crate::testing::temp_dir("slug-写法");
        let 根 = 临时.path().join("x");
        std::fs::create_dir_all(&根).expect("能建根目录");

        let 原样 = Slug::AtPath(&根).text();
        let 带尾斜杠 = 接一个分隔符(&根);
        let 夹一个点 = 临时.path().join(".").join("x");
        let 绕一圈 = 根.join("..").join("x");

        assert_eq!(原样, Slug::AtPath(&带尾斜杠).text(), "尾斜杠不该另开一份");
        assert_eq!(原样, Slug::AtPath(&夹一个点).text(), "夹一个点不该另开一份");
        assert_eq!(原样, Slug::AtPath(&绕一圈).text(), "绕一圈不该另开一份");
        assert!(
            原样.starts_with("x-"),
            "人认得出的那一半要取真实的目录名，实得 {原样}"
        );
    }

    #[test]
    fn 点号跟着工作目录走而不是把无关目录合成一个主库() {
        // `cd 甲 && romcat scan .` 与 `cd 乙 && romcat scan .` 曾经开的是同一份中立库
        // ——哈希取的是 `.` 这一个字符，与在哪儿敲的无关。于是两个毫不相干的目录成了
        // 同一个**主库**底下的两个**根**，而中立库是事实来源（ADR-0001）。
        //
        // 测试里不真的去换**工作目录**：那是进程全局的，并行跑的别的用例会跟着遭殃。
        // 换个说法证同一件事——`.` 折出来的必须等于**当前**工作目录折出来的，
        // 于是换个地方敲 `.` 必然是另一份中立库。
        let 工作目录 = std::env::current_dir().expect("拿得到工作目录");
        let 点号 = Slug::AtPath(Path::new(".")).text();
        assert_eq!(点号, Slug::AtPath(&工作目录).text(), "`.` 就是当前工作目录");

        let 上一级再拐回来 = Path::new("..").join(工作目录.file_name().expect("工作目录有名字"));
        assert_eq!(
            点号,
            Slug::AtPath(&上一级再拐回来).text(),
            "相对路径开的也是同一份"
        );

        let 另一个目录 = crate::testing::temp_dir("slug-另一个");
        assert_ne!(
            点号,
            Slug::AtPath(另一个目录.path()).text(),
            "两个无关目录不该合成一个主库"
        );
    }

    #[test]
    fn 根还不存在时照样折得出名字() {
        // 化开只化得开**已存在**的那一段（`path::normalize_existing`）。根打错字、盘还
        // 没挂上都会走到这里：原样接回去，绝不能炸。尾斜杠这一下不靠 `canonicalize`
        // 也得抹掉，否则补全给的那个斜杠照样开出第二份。
        let 临时 = crate::testing::temp_dir("slug-不存在");
        let 没有的根 = 临时.path().join("还没有这个目录");
        let 名字 = Slug::AtPath(&没有的根).text();
        assert!(!名字.is_empty());
        assert_eq!(
            名字,
            Slug::AtPath(&接一个分隔符(&没有的根)).text(),
            "尾斜杠不该另开一份"
        );
    }

    fn 接一个分隔符(path: &Path) -> PathBuf {
        let mut 串 = path.to_path_buf().into_os_string();
        串.push(std::path::MAIN_SEPARATOR_STR);
        PathBuf::from(串)
    }

    #[test]
    fn 中文索引不跟着主库分开存() {
        // 「世上有哪些游戏、中文叫什么」对两块盘是同一份，跟着主库分开存就要取两次。
        let workspace = PathBuf::from("/work");
        assert_eq!(
            zh_store_path(&workspace),
            PathBuf::from("/work/zh/zh.sqlite3")
        );
        assert!(zh_cache_dir(&workspace).starts_with("/work/zh"));
    }

    #[test]
    fn dat_仓库不跟着主库分开存() {
        // DAT 与哪个主库无关：两块盘接同一台机器，同步一次就够。
        let workspace = PathBuf::from("/work");
        assert_eq!(
            dat_repo_path(&workspace),
            PathBuf::from("/work/dat/dat.sqlite3")
        );
        assert!(dat_cache_dir(&workspace).starts_with("/work/dat"));
    }

    #[test]
    fn 不同名字互不覆盖() {
        let workspace = PathBuf::from("/work");
        let a = catalog_path(&workspace, Slug::Named("主库"));
        let b = catalog_path(&workspace, Slug::Named("备份库"));
        assert_ne!(a, b);
    }

    #[test]
    fn 名字里的非法字符造不出非法文件名() {
        // 名字是用户随手打的，里面什么都可能有。可读那一半只留字母数字与 `-` `_`，
        // 哈希那一半保证不同名字仍然分得开。
        for 名字 in [
            "../../etc/passwd",
            "C:\\Windows",
            "带 空格 和 * ? 的",
            "CON",
            "",
            "。、？",
        ] {
            let name = Slug::Named(名字).text();
            assert!(
                !name.contains('/') && !name.contains('\\') && !name.contains(':'),
                "{名字} 折出来的 {name} 里不该有路径分隔符"
            );
            assert!(!name.is_empty());
            // 保留设备名后面永远跟着哈希，于是 `CON` 不再是 `CON`。
            assert_ne!(name, "CON");
        }
        assert_ne!(
            Slug::Named("../../etc/passwd").text(),
            Slug::Named("etcpasswd").text()
        );
    }

    #[test]
    fn 名字的两种规范化形式折出同一份中立库() {
        // macOS 的 NTFS 驱动把名字交出来时是 NFD，Windows 上存的是 NFC（ADR-0020）。
        // `--library` 存在的理由就是「盘在两台机器之间来回接还能找回同一份库」，
        // 这一条要是漏了，带浊音假名的名字会在两台机器上折出两份库。
        let 预组合 = "ゲーム";
        let 分解形 = "\u{30b1}\u{3099}ーム";
        assert_ne!(预组合, 分解形, "两个字符串本身不同");
        assert_eq!(
            Slug::Named(预组合).text(),
            Slug::Named(分解形).text(),
            "规范化之后是同一份库"
        );
    }

    #[test]
    fn 汉字名字留得住() {
        let name = Slug::Named("主库").text();
        assert!(name.starts_with("主库-"), "{name}");
    }
}
