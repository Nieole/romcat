//! 把**媒体池**里的封面、截图、视频铺到目标上。
//!
//! ## 铺在哪由**适配器**说了算
//!
//! 布局是格式的一部分（[`Adapter::media_placement`](crate::adapter::Adapter::media_placement)），
//! 两家差得很远：
//!
//! - **Pegasus**：`media/<内容哈希前两位>/<内容哈希>.<扩展名>`——与 [`MediaPool`]
//!   自己的分层一模一样，路径写进条目的 `assets.*`，前端不需要认得任何约定。
//! - **ES-DE**：`downloaded_media/<平台目录>/<类型>/<ROM 主名>.<扩展名>`，条目里
//!   **一个媒体路径都不写**——官方原话是 gamelist.xml 里不再包含媒体信息，
//!   应用按 ROM 文件名去找。
//!
//! 内容寻址那一种的三条理由（也是为什么它是 Pegasus 的默认）：
//!
//! - **文件名不是媒体的主键**（ADR-0009）。按游戏名铺出去要先净化文件名，而净化函数
//!   多对一不可逆——两个条目净化成同一个名字之后，谁也说不出那张图原本是谁的。
//! - **同一份媒体被多个条目引用时目标上仍然只有一份**。内容寻址天然给的，不必另跑
//!   一趟清理。
//! - **名字稳定**。按序号铺（`screenshot1`、`screenshot2`）的话，中间插进来一张新截图
//!   会让后面每一张都改名，于是下一趟同步凭空多出一批「更新」。
//!
//! ⚠️ **按文件名铺的那一种付得起这笔账**：同一张封面被三个变体引用时，目标上就是三份
//! 字节相同的文件。那是格式的代价——ES 家族没有内容寻址这回事，媒体的**主键就是
//! 文件名**，去重就等于让其中两个条目找不到自己的图。
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

use crate::adapter::Adapter;
use crate::catalog::{Catalog, CatalogError};
use crate::scrape::pool::MediaPool;
use crate::scrape::{AnchorKind, MediaKind};
use crate::sublibrary::Selected;

use super::{DesiredFile, FileKind, Stamp};

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
    /// **被同类挤掉**的图有几张。
    ///
    /// 靠文件名找媒体的格式（ES-DE）里，一个变体的一个类型只**放得下一张**——落点
    /// 是 `<ROM 主名>.<扩展名>`，第二张截图与第一张是同一条路径。挤掉不是错，是这个
    /// 格式的容量；但**得说出来**，不然它就是一次静默的丢失。
    pub crowded_out: u64,
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
pub fn lay(
    catalog: &Catalog,
    adapter: &dyn Adapter,
    pool: &MediaPool,
    selected: &Selected,
) -> Result<Laid, CatalogError> {
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
                let Some(ext) = catalog.media_ext(&reference.hash)? else {
                    out.not_in_pool += 1;
                    continue;
                };
                let Some(placement) =
                    adapter.media_placement(&picked.key, kind, &reference.hash, &ext)
                else {
                    out.unknown_kind += 1;
                    continue;
                };
                let at = pool.path_of(&reference.hash, &ext);
                let Ok(meta) = std::fs::metadata(&at) else {
                    out.not_in_pool += 1;
                    continue;
                };
                let path = placement.path;
                // 槽是 `None` 的格式**靠文件名找媒体**，条目里一个路径都不写。
                if let Some(slot) = placement.slot {
                    let slots = out.assets.entry(picked.key.clone()).or_default();
                    let list = slots.entry(slot).or_default();
                    if !list.contains(&path) {
                        list.push(path.clone());
                    }
                }
                if !placed.insert(path.clone()) {
                    // 同一条路径已经铺过了。**两种情形，只有一种要报**：内容寻址那一种
                    // 是同一份内容被多个条目引用（本来就该只有一份，见模块文档），
                    // 按文件名那一种是同一个变体的第二张图被同类挤掉——那是丢东西。
                    if placement.slot.is_none() {
                        out.crowded_out += 1;
                    }
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
    use crate::adapter::gamelist::Gamelist;
    use crate::adapter::pegasus::Pegasus;
    use crate::adapter::{Adapter, MediaPlacement};
    use crate::scrape::MediaKind;

    #[test]
    fn 认不出是什么的图哪个格式都不铺() {
        // **不猜**——猜错了就是把说明书当封面铺到掌机上。
        for adapter in [&Pegasus as &dyn Adapter, &Gamelist] {
            assert_eq!(
                adapter.media_placement("FC/甲.zip", MediaKind::Other, "abc123", "png"),
                None,
                "{}",
                adapter.name()
            );
        }
    }

    #[test]
    fn 两家的媒体布局差得很远() {
        // Pegasus：内容寻址，路径写进条目的资源槽。
        assert_eq!(
            Pegasus.media_placement("FC/甲.zip", MediaKind::Cover, "abc123", "png"),
            Some(MediaPlacement {
                path: "media/ab/abc123.png".to_string(),
                slot: Some("boxFront"),
            })
        );
        // ES-DE：按文件名约定，条目里一个路径都不写。
        assert_eq!(
            Gamelist.media_placement("FC/甲.zip", MediaKind::Cover, "abc123", "png"),
            Some(MediaPlacement {
                path: "downloaded_media/FC/covers/甲.png".to_string(),
                slot: None,
            })
        );
    }
}
