//! 把**媒体池**里的封面、截图、视频铺到目标上。
//!
//! ## 目标上的布局照抄池的布局
//!
//! `媒体/<内容哈希前两位>/<内容哈希>.<扩展名>`——与 [`MediaPool`] 自己的分层一模一样。
//! 三条理由：
//!
//! - **文件名不是媒体的主键**（ADR-0009）。按游戏名铺出去要先净化文件名，而净化函数
//!   多对一不可逆——两个条目净化成同一个名字之后，谁也说不出那张图原本是谁的。
//! - **同一份媒体被多个条目引用时目标上仍然只有一份**。内容寻址天然给的，不必另跑
//!   一趟清理。
//! - **名字稳定**。按序号铺（`screenshot1`、`screenshot2`）的话，中间插进来一张新截图
//!   会让后面每一张都改名，于是下一趟同步凭空多出一批「更新」。
//!
//! 前端靠元数据里写死的 `assets.*` 路径找到它们（[`frontend`](super::frontend)），
//! 因此这套布局不需要前端认得任何约定。
//!
//! ## 认不出是什么的图一张都不铺
//!
//! `CONTEXT.md` 的媒体类型里有**其他**这一档：认不出是什么的图。**不猜**——猜错了
//! 就是把说明书扫描件当封面铺到掌机上。真库里 593 条引用中有 128 条是这一档。
//!
//! ## 池里没有的引用只数不报错
//!
//! 中立库记着「这部作品的封面是哈希 X」，而池里那个文件不在（池被清过、库从别处拷来）。
//! 这既不是错误也不是可修复的东西，如实数出来即可——与**不可读**是第三态同一条纪律。

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::catalog::{Catalog, CatalogError};
use crate::scrape::pool::MediaPool;
use crate::scrape::{AnchorKind, MediaKind};
use crate::sublibrary::Selected;

use super::{DesiredFile, FileKind, Stamp};

/// 媒体在子库里的落脚目录。
///
/// ASCII 且短：目标多半是 exFAT / FAT32 的 SD 卡，路径长度是稀缺资源（票 21 要查的
/// 正是这个）。**它有可能与主库里一个真叫 `media` 的平台目录撞上**——撞上时那条路径
/// 会被报成「落点被占」而不是被覆盖，因为工具在清单之外没有写的权利（ADR-0015）。
pub const MEDIA_DIR: &str = "media";

/// 一份媒体在 Pegasus 的哪个资源槽上。
///
/// **认不出是什么的图没有槽**：见模块文档。
#[must_use]
pub fn slot_of(kind: MediaKind) -> Option<&'static str> {
    Some(match kind {
        MediaKind::Cover => "boxFront",
        MediaKind::Screenshot => "screenshot",
        MediaKind::Video => "video",
        MediaKind::Other => return None,
    })
}

/// 铺出来的东西。
#[derive(Debug, Clone, Default)]
pub struct Laid {
    /// 目标上该有的媒体文件，按路径排。
    pub files: Vec<DesiredFile>,
    /// 相对子库根的路径 → 它在**媒体池**里的落点。执行那一步照它去取字节。
    pub from_pool: BTreeMap<String, PathBuf>,
    /// 变体的键 → 资源槽 → 相对子库根的路径。前端元数据照它写 `assets.*`。
    pub assets: BTreeMap<String, BTreeMap<&'static str, Vec<String>>>,
    /// 库里记着、池里却没有那个文件的引用有几条。
    pub not_in_pool: u64,
    /// 认不出是什么的图有几张——**一张都不铺**。
    pub unknown_kind: u64,
}

impl Laid {
    /// 铺出去的媒体共占多少。
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.files.iter().map(|file| file.bytes).sum()
    }
}

/// 把选中变体的媒体折成**期望状态**的一部分。
///
/// 两个锚点都看：躺在变体目录里的那几张图挂在**变体**上，刮削来的封面与简介挂在
/// **作品**上（[`AnchorKind`]）。只看一个锚点会让另一半媒体静默消失。
///
/// # Errors
/// 读中立库失败时返回错误。
pub fn lay(catalog: &Catalog, pool: &MediaPool, selected: &Selected) -> Result<Laid, CatalogError> {
    let works = work_of_variant(catalog)?;
    let mut out = Laid::default();
    // 同一份媒体被多个变体引用时目标上只有一个文件，于是同一条路径只铺一次；
    // 归属哪个变体记第一个遇到的那个（报告按变体折账，重复计一次就够）。
    let mut placed: BTreeSet<String> = BTreeSet::new();

    for picked in &selected.picked {
        let mut anchors = vec![(AnchorKind::Variant.label(), picked.key.clone())];
        if let Some(work) = works.get(&picked.key) {
            anchors.push((AnchorKind::Work.label(), work.clone()));
        }
        for (anchor, subject) in anchors {
            for reference in catalog.scraped_media(anchor, &subject)? {
                let Some(kind) = MediaKind::all()
                    .into_iter()
                    .find(|kind| kind.label() == reference.kind)
                else {
                    out.unknown_kind += 1;
                    continue;
                };
                let Some(slot) = slot_of(kind) else {
                    out.unknown_kind += 1;
                    continue;
                };
                let Some(ext) = catalog.media_ext(&reference.hash)? else {
                    out.not_in_pool += 1;
                    continue;
                };
                let at = pool.path_of(&reference.hash, &ext);
                let Ok(meta) = std::fs::metadata(&at) else {
                    out.not_in_pool += 1;
                    continue;
                };
                let path = format!(
                    "{MEDIA_DIR}/{}/{}.{ext}",
                    reference.hash.get(..2).unwrap_or("00"),
                    reference.hash,
                );
                let slots = out.assets.entry(picked.key.clone()).or_default();
                let list = slots.entry(slot).or_default();
                if !list.contains(&path) {
                    list.push(path.clone());
                }
                if !placed.insert(path.clone()) {
                    continue;
                }
                out.from_pool.insert(path.clone(), at);
                out.files.push(DesiredFile {
                    path,
                    kind: FileKind::Media,
                    bytes: meta.len(),
                    unreadable: false,
                    // **源是内容寻址的**：键就是内容，于是「主库那份变了没有」这个问题
                    // 在这里的答案永远是「没变」——池里同一个哈希下的字节不可能是别的。
                    // 时间那一格因此填 0 而不是真去 stat 池里那个文件：池文件的
                    // 修改时间是「什么时候收进池的」，与「它是不是同一份内容」无关，
                    // 拿它当判据只会让换过一次机器就重传全部媒体。
                    source: format!("{}.{ext}", reference.hash),
                    source_stamp: Stamp {
                        bytes: meta.len(),
                        mtime_ns: Some(0),
                    },
                    variant: picked.key.clone(),
                    // 媒体不转格式：**能力档案说的是模拟器吃什么**，而封面截图是给
                    // 前端看的，前端吃什么由适配器那一侧决定（票 13 的媒体池已经把
                    // 扩展名规范过了）。
                    convert: None,
                });
            }
        }
    }
    out.files.sort_by(|a, b| a.path.cmp(&b.path));
    for slots in out.assets.values_mut() {
        for list in slots.values_mut() {
            list.sort();
        }
    }
    Ok(out)
}

/// 变体的键 → 它属于哪部作品。
fn work_of_variant(catalog: &Catalog) -> Result<BTreeMap<String, String>, CatalogError> {
    let works = catalog.work_names()?;
    Ok(catalog
        .variants()?
        .into_iter()
        .filter_map(|variant| {
            let work = variant.work_id.and_then(|id| works.get(&id))?;
            Some((variant.key, work.clone()))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 认不出是什么的图没有资源槽() {
        // **不猜**——猜错了就是把说明书当封面铺到掌机上。
        assert_eq!(slot_of(MediaKind::Other), None);
        assert_eq!(slot_of(MediaKind::Cover), Some("boxFront"));
        assert_eq!(slot_of(MediaKind::Screenshot), Some("screenshot"));
        assert_eq!(slot_of(MediaKind::Video), Some("video"));
    }
}
