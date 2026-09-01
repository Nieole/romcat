//! **DAT 仓库**：本地维护几个哈希数据库的镜像与索引，识别的弹药库。
//!
//! 中立库记的是「磁盘上有什么」，这里记的是「世上有什么」。票 07 拿两边对撞，
//! 才第一次说得出真实的命中率。
//!
//! ## 两条取数纪律
//!
//! 这两条不是风格偏好，违反任何一条都会造成实际损害，因此它们不只是注释——
//! [`guard`] 是一道**运行时闸门**，把每个 URL 在发出去之前拦一遍。用户可以整份换掉
//! [`registry`] 那份数据源清单，闸门要挡的正是那种情况。
//!
//! 1. **绝不直连 No-Intro 的 DAT 生成站点 `datomatic.no-intro.org`。** 调研中一次
//!    参数畸形的请求就触发了**永久 IP 封禁**，解封要发邮件求人。它的 `robots.txt`
//!    声称允许抓取，**不可作为安全依据**（ADR-0007）。No-Intro 的 DAT 一律走
//!    GitHub 每日镜像。
//! 2. **Redump 不能走那个 GitHub 镜像。** 镜像的 `redump.py` 硬编码了 `redump.org`，
//!    而那是 2026-06-20 起的**冻结镜像**——旧站 60 个系统、现站 106 个，相差 46 个，
//!    且旧站会以「系统存在但 No discs found」的形式**误导**（ADR-0007 修订段）。
//!    Redump 必须走现行域名 `redump.info`。
//!
//! 还有一条实现纪律同样进了闸门：**GitHub 的 `/contents` API 在 1000 条处硬截断且
//! 不报错**，调研中因此一度误判 TOSEC 缺少多个平台集。列举仓库文件一律走
//! `git/trees?recursive=1`，并且**必须检查 `truncated` 标志**。
//!
//! ## 哈希口径：两套规则同时存在
//!
//! **No-Intro 的 headerless 集按去头哈希，TOSEC 与 GoodNES 按含头原样哈希**
//! （ADR-0002 修订段）。同一份 `'89 Dennou Kyuusei Uranai` 在两边的 SHA-1 完全不同，
//! 只算一套就会丢掉 TOSEC 里全部汉化条目。因此每一份 DAT 都带一个
//! [`Convention`]，票 07 据此知道该拿哪一套哈希去撞。
//!
//! ## 一条 DAT 记录是什么
//!
//! No-Intro 与 Redump 的一条 `<game>` 通常是一个**发行版**（`CONTEXT.md` 的定义就是
//! 这么写的）。但 TOSEC 的 `[tr zh]` 条目是**汉化版**，那是**变体**不是发行版——
//! 而这恰恰是 TOSEC 对这个库的全部价值所在。所以这一层不硬套内容层级，只如实记
//! 「条目」与「文件」，由票 07 在产出**候选**时才决定挂到哪一层。

pub mod chinese;
pub mod fetch;
pub mod gamedb;
pub mod guard;
pub mod logiqx;
pub mod lookup;
pub mod registry;
pub mod repo;
pub mod report;
pub mod softlist;
pub mod sync;
pub mod xml;

pub use chinese::{ChineseMark, mark_of};
pub use fetch::{CannedFetcher, FetchError, Fetched, Fetcher, Head, HttpFetcher};
pub use guard::Refusal;
pub use lookup::Hit;
pub use registry::{Lookup, Mapped, Origin, Registry, RegistryError, Shape, Source};
pub use repo::Unit;
pub use repo::{DatRepo, RepoError};
pub use report::DatReport;
pub use sync::{Action, Plan, PlanItem, SyncError, SyncOptions, SyncOutcome};

/// 一份 DAT 的哈希算在什么上——**同时存在两套规则，混用必然全落空**。
///
/// 证据是同一张卡在四个库里的四行（`docs/research/rom-identification.md` A.17.4）：
/// No-Intro headerless 的 SHA-1 与 MAME 的 PRG dataarea **完全一致**，而 TOSEC 的
/// 「带头」文件与 No-Intro 的「带头」文件**大小相同、哈希全不同**——带头 CRC 不可
/// 跨库移植。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Convention {
    /// **含头**：按磁盘上原样的整个文件算。TOSEC、Redump、GoodNES、以及 No-Intro
    /// 的 `(Headered)` 集都是这一档。
    AsIs,
    /// **去头**：先剥掉转储头（iNES 的 16 字节、copier 的 512 字节）再算。
    /// No-Intro 的 `(Headerless)` 集与绝大多数卡带集是这一档。
    Headerless,
    /// **逐芯片**：MAME software list 把一张卡拆成 `prg` / `chr` 若干 dataarea，
    /// 一条 `rom` 是一颗芯片的内容，不是一个能直接比对的文件。
    ///
    /// 单芯片卡上它恰好等于去头哈希（上面那条证据链就是这么对上的），多芯片卡上
    /// 则要拼起来才对得上——所以它是独立的一档，不能并进 [`Self::Headerless`]。
    PerChip,
}

impl Convention {
    /// 配置里与报告里写的那个词。两处用同一个字符串，省得各写一遍再对不上。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::AsIs => "含头",
            Self::Headerless => "去头",
            Self::PerChip => "逐芯片",
        }
    }

    /// 从配置里的词认回来。
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "含头" => Some(Self::AsIs),
            "去头" => Some(Self::Headerless),
            "逐芯片" => Some(Self::PerChip),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 哈希口径的词能来回折() {
        for convention in [
            Convention::AsIs,
            Convention::Headerless,
            Convention::PerChip,
        ] {
            assert_eq!(Convention::from_label(convention.label()), Some(convention));
        }
        assert_eq!(Convention::from_label("原样"), None);
    }
}
