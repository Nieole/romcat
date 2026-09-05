//! **把名字重解一遍**：只读那些名字还是乱码的容器，别的一个字节都不碰。
//!
//! ## 它为什么单独存在
//!
//! 票 03 对非 UTF-8 的容器内部文件名按**有损转换**处理，理由是「识别靠 CRC-32、
//! 名字不参与命中」。票 11 起名字参与了（[`identify::fuzzy`](crate::identify::fuzzy)
//! 拿它去撞中文离线数据源），而有损转换**不可逆**——每个坏字节已经变成一个 `U+FFFD`，
//! 原来那个字再也回不来。
//!
//! [编码探测](crate::container::charset)挂在**读容器的时刻**，所以它只对之后扫的容器
//! 生效。已经落库的那批（真机 21,901 条）要重读一遍才解得对。
//!
//! **但不值得为它重扫整个主库**：全库扫一遍 27 分钟，还要连带丢掉算过的哈希与探过的
//! 光盘 / 卡带事实（那是识别首趟的十二分钟）。而真正要重读的只有中央目录——zip 与 7z
//! 的那张表就在文件尾部几 KB 处，一个容器几毫秒。于是这一趟：
//!
//! 1. 问中立库**哪些容器的名字还是乱码**（`SELECT DISTINCT key … WHERE lossy = 1`）；
//! 2. 只把那几个容器的内部构成重读一遍；
//! 3. **只改名字那两列**，大小、CRC-32、块号一个都不动——那些是识别的判据。
//!
//! ## 条数对不上就一条都不改
//!
//! 重读的应当是同一个文件（大小与修改时间都没变，不然扫描早把它整条换掉了），
//! 所以条目顺序与当初落库时一模一样，按序号对位是安全的。条数对不上说明这个文件
//! **确实变了**——那时该走的是扫描那条路，这一趟一个字都不改并如实报出来。

use crate::catalog::{Catalog, CatalogError, Roots};
use crate::container;
use crate::fs::LibraryFs;

use super::CancelToken;

/// 这一趟重解了什么。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Recheck {
    /// 名字还是乱码的容器有几个。
    pub containers: u64,
    /// 真的重读成功了几个。
    pub reread: u64,
    /// 改掉了几条名字。
    pub renamed: u64,
    /// 改完之后**仍然解不出来**的还有几条（三种编码都不是）。
    pub still_lossy: u64,
    /// 这一趟读不动的容器（盘不在位、穿不透）。**读不到不是结论**（ADR-0021）。
    pub unreadable: u64,
    /// 文件变了、条目对不上位，因此一个字都没改的容器。
    pub moved_on: u64,
    /// 几条**改之前 → 改之后**的样例。
    ///
    /// 它是这一趟唯一**可核对**的产出：三种编码的字节分布相近，猜错会把一种乱码换成
    /// 另一种，而一列数字看不出猜没猜对——一眼扫过 `??????.nes → 上海大亨.nes` 看得出。
    pub samples: Vec<(String, String)>,
    /// 这一趟被中断了吗。
    pub interrupted: bool,
}

/// 样例留几条。够看出「猜得对不对」，多了淹掉报告（同报告里别处的 `EXAMPLES`）。
const SAMPLES: usize = 12;

/// 把名字还是乱码的那些容器重读一遍。
///
/// **一个字节都不写主库**（ADR-0004）：只读容器的中央目录，只写中立库里名字那两列。
///
/// # Errors
/// 中立库读写失败时返回错误。读不动某个容器不算错误——那是一条如实记下来的计数。
pub fn recheck(
    library: &dyn LibraryFs,
    catalog: &mut Catalog,
    roots: &Roots,
    cancel: &CancelToken,
    progress: &mut dyn FnMut(&Recheck),
) -> Result<Recheck, CatalogError> {
    let keys = catalog.containers_with_lossy_names()?;
    let mut out = Recheck {
        containers: u64::try_from(keys.len()).unwrap_or(u64::MAX),
        ..Recheck::default()
    };
    for key in keys {
        if cancel.is_cancelled() {
            out.interrupted = true;
            break;
        }
        // **改之前长什么样**要从同一张表、同一个顺序上取（含目录条目），
        // 不然样例会与重读出来的那一列对不上位。
        let before = catalog.container_entries(&key)?;
        let Some(path) = roots.join(&key) else {
            // 键说的那个根不在这份库里：与「读不动」同一种处置，如实计数不猜。
            out.unreadable += 1;
            continue;
        };
        let listing = match container::list(library, &path) {
            Ok(listing) => listing,
            Err(_) => {
                out.unreadable += 1;
                continue;
            }
        };
        match catalog.rename_container_entries(&key, &listing.contents.entries)? {
            None => out.moved_on += 1,
            Some(changed) => {
                out.reread += 1;
                out.renamed += changed;
                for (was, now) in before.iter().zip(&listing.contents.entries) {
                    if was.path != now.path && out.samples.len() < SAMPLES {
                        out.samples.push((was.path.clone(), now.path.clone()));
                    }
                }
            }
        }
        out.still_lossy += u64::try_from(
            listing
                .contents
                .entries
                .iter()
                .filter(|entry| entry.name_lossy)
                .count(),
        )
        .unwrap_or(0);
        progress(&out);
    }
    Ok(out)
}


