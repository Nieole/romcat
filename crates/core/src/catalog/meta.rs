//! **元数据表那一族键**：中立库里「整份库一份」的那几笔账，全住在 `meta` 这一张键值表上。
//!
//! ## 这一族有哪些键
//!
//! [`MetaKey`] 就是全部，一个键一个变体，**键名只在它身上写一遍**（ADR-0024）。每一笔账在
//! [`Catalog`] 上有自己的具名函数，各带各的文档；**读写它的是哪几个函数，写在那个变体的
//! 文档里**——对照表不另抄一份，抄出来的那份编译器管不到，迟早与代码走岔。
//!
//! 两处是两个键合用一对函数，都是因为**单独一个键立不住**：前端格式与导出目录缺一格
//! 就不是一套跑得起来的配置；成型那两笔是换掉整批变体那一趟的收据，跟着那一趟一起落。
//!
//! ## 为什么不做通用的按键读写面
//!
//! 通用面把「这一族有哪些键、各自什么语义」变成一个**字符串约定**：键名在调用方与这里
//! 各写一遍，写岔一个字，读的那一方交回「没记过」，谁都不报错。所以按键读写的那两个
//! 函数只在 [`catalog`](super) 这一层里可见，收的也不是字符串而是 [`MetaKey`]。
//!
//! ## 为什么是键值表上的键，不是哪张表上的一列
//!
//! 这些都是**整份库一份**的账，落在某张表的行上等于同一个值抄几万遍；更要紧的是
//! **加一张表或一列要升结构版本**，而升版的意思是让人删掉重扫一份 8.60 TiB 的库
//! （[`SCHEMA_VERSION`](super::SCHEMA_VERSION) 的文档）。键值表上**加一个键是纯加**：
//! 已有的表一列没动，旧库拿新程序打开照样能用，读不到那一行就是「还没记过」。

use rusqlite::{OptionalExtension, params};

use super::{Catalog, CatalogError};

/// 元数据表上的一个键。**这张表上有哪些键，看这里就全了。**
///
/// ⚠️ **键名落在盘上**（[`Self::as_str`]）。改一个字，旧库里那一行不删，却从此读不回来
/// ——改过的名字、记住的导出配置、上次跑的时刻都会悄悄变回「没记过」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MetaKey {
    /// 中立库的**结构版本**（[`SCHEMA_VERSION`](super::SCHEMA_VERSION)）。
    ///
    /// 它不是一笔账，是**这张表读不读得**的前提：开库那一步先核它，对不上就不往下读
    /// （[`Catalog::open`]、[`Catalog::open_read_only`]），所以没有公开的具名函数。
    SchemaVersion,
    /// **主库原名**：人给这份主库起的那个名字，原样落、一个字符都不折。
    ///
    /// 建库那一趟落（[`Catalog::open_named`]），改名时换（[`Catalog::set_library_name`]），
    /// 读不到时退回从文件名截（[`Catalog::library_name`]）。**它不是主库标识**：
    /// 中立库的文件名与路径锚认的都不是这一行。
    LibraryName,
    /// **上次重折标题集合是什么时候**（UNIX 纪元起的秒）。
    ///
    /// 读 [`Catalog::titles_folded_at`]，写 [`Catalog::mark_titles_folded`]。不是 `title` 表上
    /// 的一列：那张表一行一条叫法，这是整趟活的账。
    TitlesFoldedAt,
    /// **上次导出是什么时候**（UNIX 纪元起的秒）。
    ///
    /// 读 [`Catalog::exported_at`]，写 [`Catalog::mark_exported`]。
    ExportedAt,
    /// 记住的**前端格式**：适配器报的那个名字。
    ///
    /// 与 [`Self::ExportDir`] 合成一套配置：读 [`Catalog::export_setup`]，
    /// 写 [`Catalog::set_export_setup`]。
    ExportFormat,
    /// 记住的**导出目录**：它在语义上是**主库根的替身**（`CONTEXT.md` 的**导出**条）。
    ///
    /// 读写同 [`Self::ExportFormat`]。盘上那串键名是 `export_out_dir`——落了盘的名字
    /// 不再改（`CONTEXT.md` 开头「三类不算撞」第 2 条），代码里照词表叫导出目录。
    ExportDir,
    /// **成型**跑到哪一次遍历为止。
    ///
    /// 读 [`Catalog::shaped_scan`]；与 [`Self::ShapedManifest`] 一起在
    /// [`Catalog::replace_variants`] 那一趟的末尾落，没有单独的写函数。
    ShapedScan,
    /// **成型**用的是哪一份平台清单（指纹）。
    ///
    /// 读 [`Catalog::shaped_manifest`]；写同 [`Self::ShapedScan`]。
    ShapedManifest,
}

impl MetaKey {
    /// 这个键落在盘上的那串字。
    const fn as_str(self) -> &'static str {
        match self {
            Self::SchemaVersion => "schema_version",
            Self::LibraryName => "library_name",
            Self::TitlesFoldedAt => "titles_folded_at",
            Self::ExportedAt => "exported_at",
            Self::ExportFormat => "export_format",
            Self::ExportDir => "export_out_dir",
            Self::ShapedScan => "shaped_scan",
            Self::ShapedManifest => "shaped_manifest",
        }
    }
}

impl Catalog {
    /// 读一个键；那一行不在时是 `None`。
    pub(super) fn meta_get(&self, key: MetaKey) -> Result<Option<String>, CatalogError> {
        self.conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                params![key.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|source| self.err(source))
    }

    /// 写一个键，已有就覆盖。
    pub(super) fn meta_set(&self, key: MetaKey, value: &str) -> Result<(), CatalogError> {
        self.conn
            .execute(
                "INSERT INTO meta(key, value) VALUES(?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key.as_str(), value],
            )
            .map_err(|source| self.err(source))?;
        Ok(())
    }
}
