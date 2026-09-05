//! 主库的**一组根**。
//!
//! 主库不是一个目录，是一组根（`CONTEXT.md`）：几块盘、几个目录都可以加进同一个主库，
//! 扫完收进**同一份中立库**。这个模块管两件事：
//!
//! - **哪些根**——名字、路径、上次扫描的时刻与结果，落在 `library_root` 表里。
//! - **从键回到盘**——[`Roots`] 拿**中立库的键**的第一段（根名）查出那个根在盘上的位置，
//!   再接上相对路径。识别、刮削、同步都要走它才读得到那个文件。
//!
//! ## 为什么根名进键里而不是另起一列
//!
//! 中立库里有二十来张表以「条目的键」或「变体的键」当主键或外键，**沉淀库**的**路径锚**
//! 也是 `(主库名, 变体的键)` 那一对。键要是拆成两列，这些全得改一遍，连不可再生的
//! 沉淀库都要动结构。而键是一串不透明的文本这件事，除了「取平台目录」与「取文件名」
//! 之外没人依赖——把根名放进第一段，改的只有 `crate::path` 里拆键的那几个函数。
//!
//! ## 上次扫描的结果为什么存在这儿
//!
//! 「盘没挂上时这个根的上次结果仍然看得见」是这一层的硬要求。结果存在中立库里而不是
//! 现折——现折要读 `entry` 表数条目，那件事本身不碰盘、也做得到，但**「上次扫了多久」
//! 与「那一趟中断没有」现折不出来**。于是时刻、耗时、条数、中断与否落在根这一行上，
//! 而**变体数与容量**照旧从库里现折（[`Catalog::root_stats`]）——那两个数在成型之后才准，
//! 扫描当时的数字反而是过时的。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};

use super::{Catalog, CatalogError, now_secs};
use crate::fs::{DirCache, LibraryFs};
use crate::path;

/// 主库那一组根的表。
pub(super) const ROOTS_SCHEMA: &str = "\
-- 主库的一个**根**：一个被扫描的目录。
--
-- 主键是**名字**而不是路径，理由与子库、与 `--library` 同源：挂载点会变，名字不变。
-- 名字同时是**中立库的键**的第一段（`path::library_key`），所以它不许带 `/`。
CREATE TABLE IF NOT EXISTS library_root(
    name       TEXT PRIMARY KEY,
    -- 上次见到这个根的位置，展示形态。扫描时更新；盘没挂上时它仍然是最后已知的那个。
    path       TEXT    NOT NULL,
    added_at   INTEGER NOT NULL,
    -- 上次扫描的**结果**。没扫过就一列都没有——那与「扫过但一个条目都没有」不是一件事。
    scanned_at INTEGER,
    elapsed_ms INTEGER,
    entries    INTEGER,
    interrupted INTEGER
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS library_root_path ON library_root(path);
";

/// 主库里的一个**根**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryRoot {
    /// 根名。它是**中立库的键**的第一段。
    pub name: String,
    /// 上次见到这个根的位置。
    pub path: String,
    /// 加进这个主库的时刻（UNIX 纪元起的秒）。
    pub added_at: i64,
    /// 上次扫描的结果；从没扫过时是 `None`。
    pub scan: Option<RootScan>,
}

/// 一个根**上次扫描**留下的结果。
///
/// 它住在中立库里而不是跟着盘走，因此**盘没挂上时照样看得见**（ADR-0009 的道理）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootScan {
    /// 扫完的时刻（UNIX 纪元起的秒）。
    pub at: i64,
    /// 那一趟花了多久。
    pub elapsed_ms: u64,
    /// 那一趟这个根下面记了多少条目。
    pub entries: u64,
    /// 那一趟被中断了吗。**中断的那一趟数字是个下界**，报告要说得出来。
    pub interrupted: bool,
}

/// 一个根现在装着多少东西。从库里现折，**不碰磁盘**。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RootStats {
    /// 这个根下面有多少**变体**。
    pub variants: u64,
    /// 这个根下面记了多少个文件。
    pub files: u64,
    /// 这些文件加起来多少字节。**读不到大小的那些不计**，因此这是个下界（ADR-0021）。
    pub bytes: u64,
}

/// 加一个根为什么被拒。
///
/// 每一条都拦着一种**会静默出错**的情况，所以它们各自说得出自己那句话——
/// 「加不了」而不说为什么，用户只能挨个试。
#[derive(Debug, thiserror::Error)]
pub enum AddRootError {
    /// 名字不能用。
    #[error("这个根名不能用：{0}")]
    Name(#[from] path::RootNameError),
    /// 已经有一个同名的根。
    #[error("已经有一个叫「{name}」的根了（{path}）。换个名字，或者先移除它")]
    Duplicate {
        /// 撞上的那个根名。
        name: String,
        /// 那个根在哪。
        path: String,
    },
    /// 这个目录已经是另一个根。
    #[error("{path} 已经是根「{name}」了")]
    SamePath {
        /// 已有的那个根名。
        name: String,
        /// 那个路径。
        path: String,
    },
    /// 落在已有的根里面。
    #[error(
        "{path} 落在根「{name}」（{outer}）里面。\
         同一批文件会被数两遍——加根之前先想清楚要的是哪一层"
    )]
    Inside {
        /// 要加的那个路径。
        path: String,
        /// 外层那个根名。
        name: String,
        /// 外层那个根在哪。
        outer: String,
    },
    /// 把已有的根圈进去了。
    #[error(
        "{path} 把根「{name}」（{inner}）圈在里面了。\
         同一批文件会被数两遍——要换成这一层，先移除「{name}」"
    )]
    Contains {
        /// 要加的那个路径。
        path: String,
        /// 被圈进去的那个根名。
        name: String,
        /// 那个根在哪。
        inner: String,
    },
    /// 与工作目录纠缠在一起。
    #[error(
        "{path} 与工作目录 {workspace} 套在一起。\
         **主库只读**（ADR-0004）：中立库、断点、媒体池都写在工作目录里，\
         而它们一个字节都不许落进主库"
    )]
    Workspace {
        /// 要加的那个路径。
        path: String,
        /// 工作目录。
        workspace: String,
    },
    /// 中立库写不动。
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

impl Catalog {
    /// 主库的**一组根**，按名字排序。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn roots(&self) -> Result<Vec<LibraryRoot>, CatalogError> {
        let mut statement = self
            .conn
            .prepare(
                "SELECT name, path, added_at, scanned_at, elapsed_ms, entries, interrupted
                 FROM library_root ORDER BY name",
            )
            .map_err(|source| self.err(source))?;
        let rows = statement
            .query_map([], |row| {
                let scanned_at: Option<i64> = row.get(3)?;
                Ok(LibraryRoot {
                    name: row.get(0)?,
                    path: row.get(1)?,
                    added_at: row.get(2)?,
                    scan: scanned_at.map(|at| RootScan {
                        at,
                        elapsed_ms: u64::try_from(row.get::<_, i64>(4).unwrap_or(0)).unwrap_or(0),
                        entries: u64::try_from(row.get::<_, i64>(5).unwrap_or(0)).unwrap_or(0),
                        interrupted: row.get::<_, i64>(6).unwrap_or(0) != 0,
                    }),
                })
            })
            .map_err(|source| self.err(source))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|source| self.err(source))?);
        }
        Ok(out)
    }

    /// 按名字取一个根；没有这个根时是 `None`。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn root(&self, name: &str) -> Result<Option<LibraryRoot>, CatalogError> {
        Ok(self.roots()?.into_iter().find(|root| root.name == name))
    }

    /// 记一个根进来。**不做任何校验**——校验在 [`add_root`] 那条路上。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub(crate) fn insert_root(&self, name: &str, path: &str) -> Result<(), CatalogError> {
        self.conn
            .execute(
                "INSERT INTO library_root(name, path, added_at) VALUES(?1, ?2, ?3)
                 ON CONFLICT(name) DO UPDATE SET path = excluded.path",
                params![name, path, now_secs()],
            )
            .map(|_| ())
            .map_err(|source| self.err(source))
    }

    /// 改一个根现在挂在哪。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn set_root_path(&self, name: &str, path: &str) -> Result<(), CatalogError> {
        self.conn
            .execute(
                "UPDATE library_root SET path = ?2 WHERE name = ?1",
                params![name, path],
            )
            .map(|_| ())
            .map_err(|source| self.err(source))
    }

    /// 记下一个根**上次扫描**的结果。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn record_root_scan(&self, name: &str, scan: &RootScan) -> Result<(), CatalogError> {
        self.conn
            .execute(
                "UPDATE library_root
                 SET scanned_at = ?2, elapsed_ms = ?3, entries = ?4, interrupted = ?5
                 WHERE name = ?1",
                params![
                    name,
                    scan.at,
                    i64::try_from(scan.elapsed_ms).unwrap_or(i64::MAX),
                    i64::try_from(scan.entries).unwrap_or(i64::MAX),
                    i64::from(scan.interrupted),
                ],
            )
            .map(|_| ())
            .map_err(|source| self.err(source))
    }

    /// 一个根现在装着多少东西。**不碰磁盘**，盘没挂上时照样数得出来。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn root_stats(&self, name: &str) -> Result<RootStats, CatalogError> {
        let prefix = format!("{name}/");
        let (files, bytes) = self
            .conn
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(len), 0) FROM entry
                 WHERE kind = 0 AND readable = 1 AND substr(key, 1, length(?1)) = ?1",
                params![prefix],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .map_err(|source| self.err(source))?;
        let variants: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM variant WHERE substr(key, 1, length(?1)) = ?1",
                params![prefix],
                |row| row.get(0),
            )
            .map_err(|source| self.err(source))?;
        Ok(RootStats {
            variants: u64::try_from(variants).unwrap_or(0),
            files: u64::try_from(files).unwrap_or(0),
            bytes: u64::try_from(bytes).unwrap_or(0),
        })
    }

    /// 移除一个根：这个根下面的记录整批删掉，返回**去掉了多少变体**。
    ///
    /// 用户按下这一下之前该看见那个数——这是唯一一处工具会主动丢掉扫描结果的地方。
    /// 丢掉的全是可再生的（中立库整份可再生，`CONTEXT.md`）；**沉淀库一个字都不动**，
    /// 那里面是用户亲手定的东西。
    ///
    /// **这个根那一趟遍历也一起删掉。** 移除说的是「这个根在中立库里的一切都不算数了」，
    /// 遍历与它的批注是那个「一切」的一部分：留着遍历行，报告会继续替一个已经不在的根
    /// 报抬头与耗时；更要紧的是**断点的身份靠它**——同名同路径加回来时，工作目录里那份
    /// 按根名取的旧断点会重新对上，于是只扫 `pending` 那一半，收尾还什么都删不到，
    /// 扫完报「完整」却少文件。删掉这一行，那份旧断点就再也对不上谁
    /// （[`scan`](crate::scan) 那道守卫），加回来的根从头扫一遍。
    ///
    /// **断点文件本身不在这里删**：核心不知道工作目录在哪，而删不删在行为上没有区别
    /// ——留着它，`--resume` 也是读到、对不上、从头扫。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn remove_root(&mut self, name: &str) -> Result<u64, CatalogError> {
        let removed = self.root_stats(name)?.variants;
        let prefix = format!("{name}/");
        // **根自己那条键就是它的名字**，所以条目要连名字带前缀一起删。
        self.conn
            .execute(
                "DELETE FROM entry WHERE key = ?1 OR substr(key, 1, length(?2)) = ?2",
                params![name, prefix],
            )
            .map_err(|source| self.err(source))?;
        // 挂在**这个根的变体**身上的那几张表按前缀整批删。挂在别处的（作品级的刮削值、
        // 合集本身、别的根的东西）一条都不动——留着幽灵行比留着孤儿更坏：
        // 浏览屏与待确认队列上会长出指不着任何文件的行。
        for sql in [
            "DELETE FROM variant_member WHERE substr(variant_key, 1, length(?1)) = ?1",
            "DELETE FROM variant WHERE substr(key, 1, length(?1)) = ?1",
            "DELETE FROM shaping_override
             WHERE substr(key, 1, length(?1)) = ?1
                OR substr(variant_key, 1, length(?1)) = ?1",
            "DELETE FROM candidate WHERE substr(variant_key, 1, length(?1)) = ?1",
            "DELETE FROM identification WHERE substr(variant_key, 1, length(?1)) = ?1",
            "DELETE FROM model_answer WHERE substr(variant_key, 1, length(?1)) = ?1",
            "DELETE FROM collection_variant WHERE substr(variant_key, 1, length(?1)) = ?1",
            "DELETE FROM preferred_variant WHERE substr(variant_key, 1, length(?1)) = ?1",
            "DELETE FROM title
             WHERE variant_key IS NOT NULL AND substr(variant_key, 1, length(?1)) = ?1",
            // 刮削那三张表按**锚点**存，锚点既可能是变体的键也可能是作品名——
            // 只删 `subject = '变体'` 那一半，作品那一层是别的根也在用的。
            "DELETE FROM scrape_value
             WHERE subject = '变体' AND substr(anchor, 1, length(?1)) = ?1",
            "DELETE FROM media_ref
             WHERE subject = '变体' AND substr(anchor, 1, length(?1)) = ?1",
            "DELETE FROM scrape_probe
             WHERE subject = '变体' AND substr(anchor, 1, length(?1)) = ?1",
            // **子库的例外与清单**也认变体的键。例外跟着这个根走；清单只清「源指着这个
            // 根」的那几行——目标上真有什么，由下一趟同步照实观察出来（ADR-0015）。
            "DELETE FROM sublibrary_exception WHERE substr(variant_key, 1, length(?1)) = ?1",
            "DELETE FROM sublibrary_manifest WHERE substr(variant, 1, length(?1)) = ?1",
        ] {
            self.conn
                .execute(sql, params![prefix])
                .map_err(|source| self.err(source))?;
        }
        // 遍历与它的批注按**根名**记（`catalog::SCHEMA_VERSION` 的 7），跟着这个根一起走。
        for sql in [
            "DELETE FROM traversal WHERE root_name = ?1",
            "DELETE FROM traversal_note WHERE root_name = ?1",
            "DELETE FROM library_root WHERE name = ?1",
        ] {
            self.conn
                .execute(sql, params![name])
                .map_err(|source| self.err(source))?;
        }
        self.drop_orphans()?;
        Ok(removed)
    }

    /// 这份中立库上次扫的那个根叫什么名字（[`Catalog::last_traversal`] 那一趟）。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn last_scanned_root(&self) -> Result<Option<String>, CatalogError> {
        self.conn
            .query_row(
                "SELECT root_name FROM traversal ORDER BY scan DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|source| self.err(source))
    }
}

/// 加一个根：先把会静默出错的几种情况一一拦下，再落库。
///
/// 拦的四件事各有各的道理：
///
/// - **名字**要能当键的第一段（[`path::root_name`]）。
/// - **重名**会让两块盘的记录挤进同一串前缀——那正是这一票要治的病。
/// - **套在一起**（新根落在老根里，或者把老根圈进去）会让同一批文件被数两遍。
/// - **与工作目录纠缠**会让中立库、断点、媒体池落进主库，而**主库只读**（ADR-0004）。
///
/// `workspace` 是工作目录；`None` 表示这一趟没有工作目录可守（只活在内存里的库）。
///
/// **`root` 要先化成可比较的绝对形态**（[`path::normalize_existing`]，或者交给
/// [`LibraryFs::canonicalize`](crate::fs::LibraryFs::canonicalize) 化）。这里不代劳：
/// 扫描那一侧的根是**主库的只读视图**化出来的，而这里化只会用本机文件系统再化一遍
/// ——两者在符号链接上给出的答案不一定一样，一份中立库里于是会出现同一个根的两种写法。
///
/// # Errors
/// 上面四条任一不过、或者中立库写不动时返回 [`AddRootError`]。
pub fn add_root(
    catalog: &Catalog,
    workspace: Option<&Path>,
    name: &str,
    root: &Path,
) -> Result<LibraryRoot, AddRootError> {
    let name = path::root_name(name)?;
    let display = path::display(root);
    if let Some(existing) = catalog.root(&name)? {
        return Err(AddRootError::Duplicate {
            name: existing.name,
            path: existing.path,
        });
    }
    check_placement(catalog, workspace, &name, root)?;
    catalog.insert_root(&name, &display)?;
    Ok(LibraryRoot {
        name,
        path: display,
        added_at: now_secs(),
        scan: None,
    })
}

/// 一个根**摆在这个位置**行不行：不许与工作目录纠缠、不许与别的根套在一起。
///
/// 加一个新根走它，把一个已有的根**改指到别处**也走它（`scan::resolve_root`）——
/// 后者绕过去的话，把「主库」从 `/盘/Game` 重指到 `/盘`（而 `/盘/Game/FC` 已经是另一个
/// 根）就没人拦，同一批文件从此在两个根下各数一遍。
///
/// `name` 是这个根自己的名字：**与自己比不算套在一起**。
///
/// # Errors
/// 与工作目录纠缠、与别的根套在一起、或者中立库读不动时返回 [`AddRootError`]。
pub fn check_placement(
    catalog: &Catalog,
    workspace: Option<&Path>,
    name: &str,
    root: &Path,
) -> Result<(), AddRootError> {
    let display = path::display(root);
    if let Some(workspace) = workspace {
        let workspace = path::normalize_existing(workspace);
        if path::is_inside(root, &workspace) || path::is_inside(&workspace, root) {
            return Err(AddRootError::Workspace {
                path: display,
                workspace: path::display(&workspace),
            });
        }
    }
    for existing in catalog.roots()? {
        if existing.name == name {
            continue;
        }
        let other = PathBuf::from(&existing.path);
        if other == root {
            return Err(AddRootError::SamePath {
                name: existing.name,
                path: existing.path,
            });
        }
        if path::is_inside(&other, root) {
            return Err(AddRootError::Inside {
                path: display,
                name: existing.name,
                outer: existing.path,
            });
        }
        if path::is_inside(root, &other) {
            return Err(AddRootError::Contains {
                path: display,
                name: existing.name,
                inner: existing.path,
            });
        }
    }
    Ok(())
}

/// **从键回到盘**：一份「根名 → 那个根在哪」的对照表。
///
/// 识别要读那个文件的头、刮削要读那张图、同步要把那个文件搬过去——三件事手上都只有
/// **中立库的键**，而键的第一段是根名不是路径。这份表就是那一步翻译。
///
/// 它是**一份快照**，取的时候读一次库。一趟活跑到一半用户在界面上加了个根，
/// 这一趟不认它——那是对的：这一趟的计划本来就是按开跑时那份库算出来的。
#[derive(Debug, Clone, Default)]
pub struct Roots {
    by_name: BTreeMap<String, PathBuf>,
}

impl Roots {
    /// 从中立库读一份。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn load(catalog: &Catalog) -> Result<Self, CatalogError> {
        Ok(Self {
            by_name: catalog
                .roots()?
                .into_iter()
                .map(|root| (root.name, PathBuf::from(root.path)))
                .collect(),
        })
    }

    /// 只有一个根的一份，测试与命令行的覆盖走这条。
    #[must_use]
    pub fn single(name: &str, path: impl Into<PathBuf>) -> Self {
        let mut roots = Self::default();
        roots.set(name, path);
        roots
    }

    /// 换掉（或加上）一个根的位置。命令行的 `--library-root 名字=路径` 走这条。
    pub fn set(&mut self, name: &str, path: impl Into<PathBuf>) {
        self.by_name.insert(name.to_string(), path.into());
    }

    /// 一个根都没有。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// 有几个根。
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    /// 一条条走过 `(根名, 位置)`。
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Path)> {
        self.by_name
            .iter()
            .map(|(name, path)| (name.as_str(), path.as_path()))
    }

    /// 只有一个根时它叫什么。多于一个、或者一个都没有时是 `None`。
    #[must_use]
    pub fn only(&self) -> Option<&str> {
        match self.by_name.len() {
            1 => self.by_name.keys().next().map(String::as_str),
            _ => None,
        }
    }

    /// 这个根在哪。
    #[must_use]
    pub fn path_of(&self, name: &str) -> Option<&Path> {
        self.by_name.get(name).map(PathBuf::as_path)
    }

    /// 把一条**中立库的键**直接拼成盘上的路径。**不查盘**。
    ///
    /// 拼出来的路径是 NFC 的，而盘上那个名字可能是分解形式——真要开文件走
    /// [`Roots::real_path`]（ADR-0020）。
    #[must_use]
    pub fn join(&self, key: &str) -> Option<PathBuf> {
        let (name, relative) = path::split_root(key);
        let root = self.path_of(name)?;
        let mut path = root.to_path_buf();
        for part in relative.split('/') {
            if !part.is_empty() {
                path.push(part);
            }
        }
        Some(path)
    }

    /// 把一条**中立库的键**还原成盘上**真实存在**的那条路径。
    ///
    /// 认不出根名、或者盘上没有这条路径时返回 `None`——两者都是「读不到」，
    /// 与「读得到但是空的」不是一件事（ADR-0021）。
    #[must_use]
    pub fn real_path(&self, fs: &dyn LibraryFs, key: &str) -> Option<PathBuf> {
        self.real_path_in(fs, &mut DirCache::default(), key)
    }

    /// 与 [`Roots::real_path`] 同一件事，只是一趟里的几万条键共用一份 [`DirCache`]——
    /// 逐段列目录那条退路上，同一个目录只列一次。
    #[must_use]
    pub fn real_path_in(
        &self,
        fs: &dyn LibraryFs,
        dirs: &mut DirCache,
        key: &str,
    ) -> Option<PathBuf> {
        let (name, relative) = path::split_root(key);
        let root = self.path_of(name)?;
        dirs.real_path(fs, root, relative)
    }

    /// 把一条**中立库的键**还原成给人看的完整路径。认不出根名时原样返回那条键。
    #[must_use]
    pub fn display_key(&self, key: &str) -> String {
        match self.path_of(path::root_of_key(key)) {
            Some(root) => path::display_key(&path::display(root), key),
            None => key.to_string(),
        }
    }

    /// 反过来：盘上这条路径落在哪个根里，键是什么。
    ///
    /// 导入前端元数据时要走它——那些文件里写的是绝对路径，得先折回键才对得上变体
    /// （`adapter::transfer`）。**最长的根赢**：根之间本来不许套在一起，但命令行
    /// 给的覆盖路径管不住，取最长的那条至少不会把 `/盘/Game/FC` 判给 `/盘`。
    #[must_use]
    pub fn key_of(&self, path: &Path) -> Option<String> {
        let path = path::normalize_existing(path);
        let mut best: Option<(&str, &Path)> = None;
        for (name, root) in self.iter() {
            if !path::is_inside(root, &path) {
                continue;
            }
            if best.is_none_or(|(_, chosen)| root.as_os_str().len() > chosen.as_os_str().len()) {
                best = Some((name, root));
            }
        }
        let (name, root) = best?;
        Some(path::library_key(name, root, &path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 一份库() -> Catalog {
        Catalog::open_in_memory().expect("开得了内存库")
    }

    #[test]
    fn 根名带斜杠时拒绝加根() {
        let catalog = 一份库();
        let error = add_root(&catalog, None, "甲/乙", Path::new("/盘/Game")).expect_err("该被拒");
        assert!(matches!(
            error,
            AddRootError::Name(path::RootNameError::Separator)
        ));
    }

    #[test]
    fn 新根落在已有根内部时被拒绝并说清是哪一个() {
        let catalog = 一份库();
        catalog.insert_root("主库", "/盘/Game").expect("记得下");
        let error =
            add_root(&catalog, None, "子集", Path::new("/盘/Game/FC")).expect_err("该被拒");
        let AddRootError::Inside { name, .. } = &error else {
            panic!("该报「落在里面」，实际是 {error}");
        };
        assert_eq!(name, "主库");
        assert!(error.to_string().contains("数两遍"));
    }

    #[test]
    fn 新根把已有根圈进去时也被拒绝() {
        let catalog = 一份库();
        catalog.insert_root("主库", "/盘/Game/FC").expect("记得下");
        let error = add_root(&catalog, None, "整块盘", Path::new("/盘/Game")).expect_err("该被拒");
        assert!(matches!(error, AddRootError::Contains { .. }));
    }

    #[test]
    fn 新根与工作目录套在一起时被拒绝() {
        let catalog = 一份库();
        let error = add_root(
            &catalog,
            Some(Path::new("/家/.romcat")),
            "手滑",
            Path::new("/家/.romcat/roms"),
        )
        .expect_err("该被拒");
        assert!(matches!(error, AddRootError::Workspace { .. }));
        assert!(error.to_string().contains("ADR-0004"));
    }

    #[test]
    fn 重名的根被拒绝() {
        let catalog = 一份库();
        catalog.insert_root("主库", "/盘甲/Game").expect("记得下");
        let error = add_root(&catalog, None, "主库", Path::new("/盘乙/Game")).expect_err("该被拒");
        assert!(matches!(error, AddRootError::Duplicate { .. }));
    }

    #[test]
    fn 根名两头的空白被去掉且折成_nfc() {
        let catalog = 一份库();
        // `が` 的分解形：か + 浊音符。
        let 分解 = "  \u{304B}\u{3099}库  ";
        let root = add_root(&catalog, None, 分解, Path::new("/盘/Game")).expect("加得上");
        assert_eq!(root.name, "\u{304C}库");
    }

    #[test]
    fn 把一个已有的根改指到别处也要过摆位校验() {
        // 只有 `add_root` 拦、`set_root_path` 不拦的话，把「主库」从 `/盘/Game`
        // 重指到 `/盘`（而 `/盘/Game/FC` 已经是另一个根）就没人管，
        // 同一批文件从此在两个根下各数一遍。
        // 两个互不相干的根：加进来的时候它们是合法的。
        let catalog = 一份库();
        add_root(&catalog, None, "主库", Path::new("/盘/Game")).expect("加得上");
        add_root(&catalog, None, "元数据", Path::new("/盘/Pegasus")).expect("加得上");
        // 把「主库」重指到 `/盘`——那会把「元数据」整个圈进去。
        let error = check_placement(&catalog, None, "主库", Path::new("/盘")).expect_err("该被拒");
        let AddRootError::Contains { name, .. } = &error else {
            panic!("该报「圈进去了」，实际是 {error}");
        };
        assert_eq!(name, "元数据");
        // **与自己比不算套在一起**：换个挂载点是正常操作。
        check_placement(&catalog, None, "主库", Path::new("/盘/Game")).expect("原地不动");
        check_placement(&catalog, None, "主库", Path::new("/别的盘/Game")).expect("换挂载点");
    }

    #[test]
    fn 从键回到盘要认根名() {
        let roots = Roots::single("元数据库", "/盘乙/Pegasus");
        assert_eq!(
            roots.join("元数据库/FC/魂斗罗.zip"),
            Some(PathBuf::from("/盘乙/Pegasus/FC/魂斗罗.zip"))
        );
        assert_eq!(roots.join("没这个根/FC/魂斗罗.zip"), None);
    }
}
