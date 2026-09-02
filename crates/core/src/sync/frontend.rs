//! 给子库折一份**前端元数据**，路径全部相对子库根。
//!
//! ## 相对路径不是排版偏好，是这套东西能用的前提
//!
//! ADR-0015：**导出到子库的元数据里，ROM 与媒体路径一律相对于子库根目录。** 卡插到
//! 别的设备、挂载点变化、盘符变化都不受影响。绝不写主库的绝对路径——写了的话，那份
//! 元数据在掌机上一条都指不着。
//!
//! 这在这里是**构造上**成立的，不是靠记得：`file:` 写的是变体在主库里的**键**
//! （相对主库根，ADR-0020），而子库里的布局照搬那个键（挂账 D79），于是同一串字符
//! 两边都对；`assets.*` 写的是 [`media`](super::media) 铺出来的相对路径。两处都不
//! 经过任何绝对路径。
//!
//! ## 元数据是**生成物**，没有主库侧的源文件
//!
//! 于是清单里那两列换了含义，见 [`lay`] 里的注释：源写成
//! `<文件名>#<内容哈希前 16 位>`，内容一变键就变，「要不要重写」判得出来。
//!
//! ## 一个合集一个文件
//!
//! 与 `adapter::converge` 那一侧同一条约定，理由也一样：Pegasus 里**写文件的顺序
//! 本身有语义**，两个合集写进同一个文件，里面每个游戏就同时属于两个。

use std::collections::{BTreeMap, BTreeSet};

use crate::adapter::converge::{self, VARIANT_KEY};
use crate::adapter::{Adapter, AdapterError, Body};
use crate::catalog::{Catalog, CatalogError, frontend::hash_of};
use crate::scrape::priority::Priorities;
use crate::sublibrary::Selected;

use super::{DesiredFile, FileKind, Stamp};

/// 元数据在清单里挂在哪个「变体」名下。
///
/// 它不属于任何变体，但清单每一条都要有一格。填一个**固定的记号**而不是文件名：
/// 报告里「涉及几个变体」那个数因此至多多出 1，而不是每多一个平台就多一个。
pub const NOT_A_VARIANT: &str = "（前端元数据）";

/// 折出来的元数据。
#[derive(Debug, Clone, Default)]
pub struct Laid {
    /// 目标上该有的元数据文件，按路径排。
    pub files: Vec<DesiredFile>,
    /// 相对子库根的路径 → 那份文件的字节。**执行那一步直接写它**——元数据没有源文件，
    /// 字节是现折出来的。
    pub bytes: BTreeMap<String, Vec<u8>>,
    /// 收敛出了几个前端条目。
    pub entries: u64,
}

/// 折不出元数据的原因。
#[derive(Debug, thiserror::Error)]
pub enum FrontendError {
    /// 读中立库失败。
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    /// 适配器写不出来。
    #[error(transparent)]
    Adapter(#[from] AdapterError),
}

/// 给这个子库折一份前端元数据。
///
/// `assets` 是 [`media::lay`](super::media::lay) 铺出来的那份「变体 → 资源槽 → 相对
/// 路径」。**资源取首选变体那一个**：一个条目底下可能挂着好几个变体，而条目在前端里
/// 只有一张封面——取首选变体的，与「默认启动首选变体」是同一个选择（ADR-0012）。
///
/// # Errors
/// 读中立库失败、或者适配器写不出来时返回错误。
pub fn lay(
    catalog: &Catalog,
    adapter: &dyn Adapter,
    priorities: &Priorities,
    selected: &Selected,
    assets: &BTreeMap<String, BTreeMap<&'static str, Vec<String>>>,
) -> Result<Laid, FrontendError> {
    let picked: BTreeSet<String> = selected
        .picked
        .iter()
        .map(|variant| variant.key.clone())
        .collect();
    let converged = converge::run_within(catalog, priorities, adapter.file_name(), Some(&picked))?;

    let mut out = Laid {
        entries: converged.entries,
        ..Laid::default()
    };
    for file in &converged.files {
        let mut doc = file.doc.clone();
        for entry in &mut doc.entries {
            let Body::Game(game) = &mut entry.body else {
                continue;
            };
            // 首选变体写在 `x-romcat-variant` 里（`converge::build_game`）。
            let Some(head) = game.extra.get(VARIANT_KEY).and_then(|keys| keys.first()) else {
                continue;
            };
            let Some(slots) = assets.get(head) else {
                continue;
            };
            for (slot, paths) in slots {
                game.assets.insert((*slot).to_string(), paths.clone());
            }
        }
        // **不给基线**：子库里那份元数据是工具自己生成的，没有维护者手写的原文要保。
        // 目标上已经有一份而且不是我们放的那一份时，计划器那一侧会把它报成
        // 「落点被占」或者「被改过」，一个字节都不会覆盖过去（ADR-0015）。
        let bytes = adapter.write(&doc, None)?;
        let fingerprint = hash_of(&bytes);
        out.files.push(DesiredFile {
            path: file.file_name.clone(),
            kind: FileKind::Metadata,
            bytes: bytes.len() as u64,
            unreadable: false,
            // **生成物没有源文件**，于是「源」这一格写它自己的身份加**内容指纹**：
            // 内容一变键就变，`source_unchanged` 当场判出要重写。时间那一格填 0
            // ——生成物没有修改时间这回事，而判据已经在键里了。
            source: format!("{}#{}", file.file_name, &fingerprint[..16]),
            source_stamp: Stamp {
                bytes: bytes.len() as u64,
                mtime_ns: Some(0),
            },
            variant: NOT_A_VARIANT.to_string(),
            // 元数据不转格式：适配器写出来的就是目标前端要的那一份。
            convert: None,
        });
        out.bytes.insert(file.file_name.clone(), bytes);
    }
    out.files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}
