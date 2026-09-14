//! **导出**记在中立库里的那两样：这一趟往哪个前端格式、哪个目录写，以及上次是什么时候。
//!
//! ## 为什么是元数据表上的键，不是一张新表
//!
//! 这三样都是**整份库一份**的账：前端格式一个、目录一个、上次导出的时刻一个。
//! 一张表存三行等于给每一行配一把主键去锁一个单例，而**加一张表要升结构版本**
//! ——升版的意思是让人删掉重扫一份 8.60 TiB 的库（`SCHEMA_VERSION` 的文档）。
//! 元数据表是键值表，**加几个键是纯加**：已有的表一列没动、一条语义没改，
//! 旧库拿新程序打开照样能用，读不到就是「还没选过 / 还没导过」。
//!
//! ## 它与 [`ExportOptions`](crate::adapter::transfer::ExportOptions) 分工不同
//!
//! 这一份是**记住的那套配置**：人第一次导出之前选一次，之后一键重导（规格
//! 「导出的配置（前端格式、导出目录）与主库原名同一处」——规格那句写的是「输出目录」「主库名」，
//! 而**输出**在词表**导出**条的 `_Avoid_` 里（挂单 `Q439`），「主库名」在**主库标识**条的 `_Avoid_` 里）。那一份是**这一趟怎么跑**
//! ——只排计划不写盘、外面有人动过也照写，那两个旋钮一趟一变，记下来反而危险。

use std::path::PathBuf;

use super::meta::MetaKey;
use super::{Catalog, CatalogError, now_secs};
use crate::adapter;

/// 记住的那套**导出**配置：往哪个前端格式写、写到哪个目录。
///
/// **第一次导出之前选一次，之后一键重导**（规格「导出：整库级，不做成子库」）。
/// 它落在元数据表上，与**主库原名**同一处。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportSetup {
    /// 哪个**前端格式**——[`adapter::all`] 里那些适配器的名字之一。
    pub format: String,
    /// 写到哪个目录。**那个目录在语义上是主库根的替身**（`CONTEXT.md` 的**导出**条）：
    /// 元数据里的 `file:` 相对它自己所在的目录解析，而中立库的键相对主库根，
    /// 所以把导出来的这几份文件放进主库根，路径直接就对。
    pub out: PathBuf,
}

/// 这套配置立不住的原因。
///
/// **判据在核心里**（ADR-0005）：界面那一层只把话转出来，不自己判「这个格式有没有」
/// ——散一份判断出去，命令行与界面迟早会对同一个字给出两种答复。
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ExportSetupError {
    /// 没有这个名字的适配器。
    #[error("没有叫「{given}」的前端格式。眼下带的是：{}。", known.join("、"))]
    NoSuchFormat {
        /// 人给的那个名字。
        given: String,
        /// 眼下带的那几个。
        known: Vec<String>,
    },
    /// 目录那一格是空的。
    #[error("还没说导到哪个目录。那个目录是主库根的替身——写进主库根，前端直接就读得到。")]
    NoOutDir,
}

impl ExportSetup {
    /// 校验人填的那两格，折出一套立得住的配置。
    ///
    /// **格式名大小写不敏感，但存的是适配器自己报的那个名字**：快照那张表按格式名分栏
    /// （`frontend_snapshot.format`），人这一趟写 `pegasus`、下一趟写 `Pegasus` 的话，
    /// 存两个名字进去等于上一趟的**底本**再也认不出来，而底本正是「外面有人动过没有」
    /// 的唯一判据。
    ///
    /// # Errors
    /// 没有这个格式、或者目录那一格是空的时返回 [`ExportSetupError`]。
    pub fn check(format: &str, out: &str) -> Result<Self, ExportSetupError> {
        let Some(adapter) = adapter::find(format.trim()) else {
            return Err(ExportSetupError::NoSuchFormat {
                given: format.trim().to_string(),
                known: adapter::names()
                    .into_iter()
                    .map(ToString::to_string)
                    .collect(),
            });
        };
        let out = out.trim();
        if out.is_empty() {
            return Err(ExportSetupError::NoOutDir);
        }
        Ok(Self {
            format: adapter.name().to_string(),
            out: PathBuf::from(out),
        })
    }

    /// 这套配置指的那个**适配器**。
    ///
    /// **界面那一层不自己去 [`adapter::find`]**（ADR-0005）：找不到时该说哪句话
    /// 是领域的事，散一份出去，命令行与界面迟早会对同一个字给出两种答复。
    /// 库里记着的那个格式眼下还在不在，只有真要跑那一趟时才问得着
    /// （[`Catalog::export_setup`] 读回来时**不判**，理由见那一条）。
    ///
    /// # Errors
    /// 眼下没有这个格式的适配器时返回 [`ExportSetupError::NoSuchFormat`]。
    pub fn adapter(&self) -> Result<Box<dyn adapter::Adapter>, ExportSetupError> {
        adapter::find(&self.format).ok_or_else(|| ExportSetupError::NoSuchFormat {
            given: self.format.clone(),
            known: adapter::names()
                .into_iter()
                .map(ToString::to_string)
                .collect(),
        })
    }
}

impl Catalog {
    /// 记住的那套**导出**配置；一次都没选过就是 `None`。
    ///
    /// **读回来的那两格不再校验**：适配器的名单是随程序走的，而这份配置是人选的。
    /// 读的时候判一遍、判不过就交回 `None` 的话，换一版程序就等于把人选过的东西
    /// 悄悄抹掉，而他会以为自己从没选过。立不立得住由真要跑那一趟时说
    /// （[`ExportSetup::check`]）。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn export_setup(&self) -> Result<Option<ExportSetup>, CatalogError> {
        let (Some(format), Some(out)) = (
            self.meta_get(MetaKey::ExportFormat)?,
            self.meta_get(MetaKey::ExportDir)?,
        ) else {
            return Ok(None);
        };
        if format.trim().is_empty() || out.trim().is_empty() {
            return Ok(None);
        }
        Ok(Some(ExportSetup {
            format,
            out: PathBuf::from(out),
        }))
    }

    /// 记下这套**导出**配置。**下一趟不必再选。**
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn set_export_setup(&self, setup: &ExportSetup) -> Result<(), CatalogError> {
        self.meta_set(MetaKey::ExportFormat, &setup.format)?;
        self.meta_set(MetaKey::ExportDir, &setup.out.to_string_lossy())
    }

    /// **上次导出是什么时候**（UNIX 纪元起的秒）；一趟都没导过就是 `None`。
    ///
    /// 库屏**工序段**上导出那一行靠它说话：那一支的「还差多少」算不出来，退回显示
    /// 上次跑的时刻（`stage::Stage::Export`）。**没导过就是 `None`**——那时画一个
    /// 时刻出来是凭空捏造。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn exported_at(&self) -> Result<Option<i64>, CatalogError> {
        Ok(self
            .meta_get(MetaKey::ExportedAt)?
            .and_then(|value| value.trim().parse::<i64>().ok()))
    }

    /// 记下**这一趟导出**的时刻、这一趟收敛出几个**条目**（`entries`），与这一趟有没有铺出**媒体**（`media`）——库屏工序段导出
    /// 那一行底下那句小字说的正是后两样（`stage::Stages::detail`）。
    ///
    /// **只有真把一趟导出走完了才调它**（`adapter::transfer::export_task` 的末尾）：
    /// 只排计划那一趟一个字节都没写，按停那一趟只写了一部分，撞上外面有人动过的那一趟
    /// 有几份没写——三者都说不上「导过了」。
    ///
    /// # Errors
    /// 写库失败时返回错误。
    pub fn mark_exported(&self, entries: u64, media: bool) -> Result<(), CatalogError> {
        self.meta_set(MetaKey::ExportedAt, &now_secs().to_string())?;
        self.meta_set(MetaKey::ExportedEntries, &entries.to_string())?;
        self.meta_set(MetaKey::ExportedWithMedia, if media { "1" } else { "0" })
    }

    /// **上次导出有没有铺出媒体**；一趟都没导过、或者这份库是记这一样之前导出的，就是 `None`。
    ///
    /// 与 [`Self::exported_at`] 同一下落（[`Self::mark_exported`]），说的是同一趟。铺没铺由导出那一趟自己说
    /// （`adapter::transfer::export_task` 的末尾），这里只读回来。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn exported_with_media(&self) -> Result<Option<bool>, CatalogError> {
        Ok(self
            .meta_get(MetaKey::ExportedWithMedia)?
            .and_then(|value| match value.trim() {
                "1" => Some(true),
                "0" => Some(false),
                _ => None,
            }))
    }

    /// **上次导出收敛出几个条目**；一趟都没导过、或者这份库是记这个数之前导出的，就是 `None`。
    ///
    /// 与 [`Self::exported_at`] 同一下落（[`Self::mark_exported`]）：打了时刻戳的那一趟才记这个数，所以两者说的是同一趟。
    ///
    /// # Errors
    /// 读库失败时返回错误。
    pub fn exported_entries(&self) -> Result<Option<u64>, CatalogError> {
        Ok(self
            .meta_get(MetaKey::ExportedEntries)?
            .and_then(|value| value.trim().parse::<u64>().ok()))
    }
}
