//! 增量扫描的判据：拿这次看到的三元组去比中立库里记的那份。

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::fs::EntryMeta;

/// 中立库里记着的那条 `(大小, 修改时间)`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recorded {
    /// 上次扫到时元数据是读得到的。
    Known {
        /// 字节数。
        len: u64,
        /// 修改时间（UNIX 纪元起的纳秒）；驱动给不出时是 `None`。
        mtime_ns: Option<i64>,
    },
    /// 上次扫到时元数据就读不到（ADR-0021）。
    Unreadable,
}

/// 一个条目这次扫描的结论。
///
/// 四态而不是三态：**不可读**是独立的一种（ADR-0021），它既不算已变——否则那 4,085 个
/// 文件每次扫描都要重来一遍，永远收敛不了——也不算已删，否则它们会从中立库里消失，
/// 而它们在 Windows 上是正常文件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// 路径、大小、修改时间都与中立库里记的一致。
    Unchanged,
    /// 大小或修改时间变了。
    Changed,
    /// 中立库里没有这条路径。
    Added,
    /// 元数据读不到。既不算已变，也不算已删。
    Unreadable,
}

/// 一次扫描相对中立库上一次状态的差异。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanDelta {
    /// 三元组未变，跳过了重看的。
    pub unchanged: u64,
    /// 大小或修改时间变了。
    pub changed: u64,
    /// 中立库里原本没有的。
    pub added: u64,
    /// 中立库里有、这次没见到的。只有完整扫完一遍才算得出来。
    pub removed: u64,
    /// 元数据读不到的（ADR-0021 的第三态）。
    pub unreadable: u64,
}

impl ScanDelta {
    /// 记下一条结论。
    pub fn record(&mut self, verdict: Verdict) {
        match verdict {
            Verdict::Unchanged => self.unchanged += 1,
            Verdict::Changed => self.changed += 1,
            Verdict::Added => self.added += 1,
            Verdict::Unreadable => self.unreadable += 1,
        }
    }
}

/// 中立库上一次扫描留下的全库三元组，整份读进内存供工作线程比对。
///
/// 整份读进来而不是逐条查库，是因为工作线程有好几个，而 SQLite 的连接不能跨线程共享；
/// 256,128 条记录约 30 MB，换来的是工作线程零查询。真到了内存吃紧那天再改按目录分批取。
#[derive(Debug, Default, Clone)]
pub struct Baseline {
    entries: HashMap<String, Recorded>,
    penetrated: HashSet<String>,
}

impl Baseline {
    /// 一份空基线：什么都没记过，于是一切都是新增。`--full` 走的就是这条。
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// 记下一条。
    pub fn insert(&mut self, key: String, recorded: Recorded) {
        self.entries.insert(key, recorded);
    }

    /// 记下「这个**透明容器**上次已经穿透过了」。
    pub fn insert_penetrated(&mut self, key: String) {
        self.penetrated.insert(key);
    }

    /// 中立库里有没有这个容器的穿透结论。
    ///
    /// 三元组没变的容器本来就该跳过重穿，但有一种情况必须补上：上一次是带
    /// `--no-containers` 扫的，中立库里根本没有这份结论。只看三元组的话，那些容器
    /// 会**永远**不被穿透，报告静悄悄地少报一批内部文件。
    #[must_use]
    pub fn penetrated(&self, key: &str) -> bool {
        self.penetrated.contains(key)
    }

    /// 这次看到的元数据对上中立库里记的那份，是什么结论。
    ///
    /// 「修改时间取不到」一律判为已变：证明不了没变，就不能说没变。这与**不可读**是两回事
    /// ——不可读连大小都没有，走的是自己那一态。
    #[must_use]
    pub fn verdict(&self, key: &str, meta: &EntryMeta) -> Verdict {
        let EntryMeta::Known { len, modified } = meta else {
            return Verdict::Unreadable;
        };
        let Some(recorded) = self.entries.get(key) else {
            return Verdict::Added;
        };
        let Recorded::Known {
            len: stored_len,
            mtime_ns: Some(stored_mtime),
        } = recorded
        else {
            // 上次读不到、这次读到了：现在有真东西可以入库，当作已变。
            return Verdict::Changed;
        };
        let same = *stored_len == *len
            && modified
                .and_then(super::mtime_ns)
                .is_some_and(|now| now == *stored_mtime);
        if same {
            Verdict::Unchanged
        } else {
            Verdict::Changed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn 已知(len: u64, secs: u64) -> EntryMeta {
        EntryMeta::Known {
            len,
            modified: Some(UNIX_EPOCH + Duration::from_secs(secs)),
        }
    }

    fn 基线() -> Baseline {
        let mut baseline = Baseline::empty();
        baseline.insert(
            "FC/马里奥.zip".to_string(),
            Recorded::Known {
                len: 1024,
                mtime_ns: Some(1_000_000_000),
            },
        );
        baseline
    }

    #[test]
    fn 三元组一致就是未变() {
        assert_eq!(
            基线().verdict("FC/马里奥.zip", &已知(1024, 1)),
            Verdict::Unchanged
        );
    }

    #[test]
    fn 大小变了是已变() {
        assert_eq!(
            基线().verdict("FC/马里奥.zip", &已知(2048, 1)),
            Verdict::Changed
        );
    }

    #[test]
    fn 大小一样但修改时间变了也是已变() {
        // 只比大小的话这种改动会被整个漏掉，而它恰恰是最常见的「内容变了」。
        assert_eq!(
            基线().verdict("FC/马里奥.zip", &已知(1024, 99)),
            Verdict::Changed
        );
    }

    #[test]
    fn 库里没有的是新增() {
        assert_eq!(基线().verdict("FC/新来的.zip", &已知(1, 1)), Verdict::Added);
    }

    #[test]
    fn 读不到元数据的既不算已变也不算新增() {
        let baseline = 基线();
        assert_eq!(
            baseline.verdict("FC/马里奥.zip", &EntryMeta::Unreadable),
            Verdict::Unreadable
        );
        assert_eq!(
            baseline.verdict("FC/从没见过.zip", &EntryMeta::Unreadable),
            Verdict::Unreadable
        );
    }

    #[test]
    fn 修改时间取不到时保守判为已变() {
        let meta = EntryMeta::Known {
            len: 1024,
            modified: None,
        };
        assert_eq!(基线().verdict("FC/马里奥.zip", &meta), Verdict::Changed);
    }

    #[test]
    fn 上次读不到这次读到了算已变() {
        let mut baseline = Baseline::empty();
        baseline.insert("FC/马里奥.zip".to_string(), Recorded::Unreadable);
        assert_eq!(
            baseline.verdict("FC/马里奥.zip", &已知(1024, 1)),
            Verdict::Changed
        );
    }

    #[test]
    fn 修改时间超出可存范围时保守判为已变() {
        let far_future = UNIX_EPOCH + Duration::from_secs(1 << 40);
        let meta = EntryMeta::Known {
            len: 1024,
            modified: Some(far_future),
        };
        assert_eq!(super::super::mtime_ns(far_future), None);
        assert_eq!(基线().verdict("FC/马里奥.zip", &meta), Verdict::Changed);
    }

    #[test]
    fn 纪元之前的修改时间是负数() {
        let before = UNIX_EPOCH - Duration::from_secs(1);
        assert_eq!(super::super::mtime_ns(before), Some(-1_000_000_000));
        assert_eq!(super::super::mtime_ns(SystemTime::UNIX_EPOCH), Some(0));
    }
}
