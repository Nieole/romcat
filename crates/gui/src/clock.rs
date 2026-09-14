//! 界面上**画时刻只走这一处**：本地时间、短格式——今年的写「09-03 14:58」，不是今年的才带年份
//! （拿主意的人 2026-09-14 定，照设计稿）。
//!
//! **本地时区从哪儿读**由 `gui-looks-like-the-design/25` 先落地（`jiff`，只加在界面这一层）；两边合并时
//! 只留一份。在那之前 [`Clock::System`] 的偏移交 0（UTC）。
//!
//! **截图里不许有当前时间**：截图测试钉死一个「此刻」与一个固定偏移（[`Clock::fixed`]），「今年」
//! 才不跟着跑测试的那一天变。
//!
//! 公历换算照旧走核心库那一处（`romcat_core::report::human_time`）：这里只加偏移、去掉今年的年份。

use std::time::{SystemTime, UNIX_EPOCH};

use romcat_core::report::human_time;

/// 画时刻时拿哪一刻当「此刻」、本地比 UTC 快多少秒。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Clock {
    /// 读系统时钟；偏移眼下交 0（见模块文档）。
    #[default]
    System,
    /// 钉死的此刻与偏移：截图测试用。
    Fixed {
        /// 此刻，UNIX 纪元起的秒。
        now: i64,
        /// 本地比 UTC 快多少秒（东八区是 `28_800`）。
        offset: i32,
    },
}

impl Clock {
    /// 钉死此刻与偏移。
    #[must_use]
    pub const fn fixed(now: i64, offset: i32) -> Self {
        Self::Fixed { now, offset }
    }

    /// `at`（UNIX 纪元起的秒）画成本地时间的短格式：与此刻同一年写 `MM-DD HH:MM`，否则 `YYYY-MM-DD HH:MM`。
    #[must_use]
    pub fn short(self, at: i64) -> String {
        let (now, offset) = match self {
            Self::System => (system_now(), 0),
            Self::Fixed { now, offset } => (now, offset),
        };
        let 那一刻 = human_time(at.saturating_add(i64::from(offset)));
        let 此刻 = human_time(now.saturating_add(i64::from(offset)));
        match (那一刻.split_once('-'), 此刻.split_once('-')) {
            (Some((那年, 月日时分)), Some((今年, _))) if 那年 == 今年 => {
                月日时分.to_string()
            }
            _ => 那一刻,
        }
    }
}

/// 系统时钟的此刻；读不到（钟拨到纪元以前）交 0。
fn system_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::Clock;

    /// 2026-09-03 14:58（UTC）。
    const 九月三号: i64 = 1_788_447_480;
    /// 2025-12-31 20:00（UTC）：东八区已经是 2026 年元旦。
    const 除夕晚上: i64 = 1_767_211_200;

    #[test]
    fn 今年的不带年份_不是今年的才带() {
        let 钟 = Clock::fixed(九月三号 + 86_400 * 11, 0);
        assert_eq!(钟.short(九月三号), "09-03 14:58");
        assert_eq!(钟.short(1_700_000_000), "2023-11-14 22:13");
    }

    #[test]
    fn 偏移跨过零点与年份时_日子与年份都照本地算() {
        let 此刻 = 九月三号;
        assert_eq!(Clock::fixed(此刻, 28_800).short(除夕晚上), "01-01 04:00");
        assert_eq!(Clock::fixed(此刻, 0).short(除夕晚上), "2025-12-31 20:00");
    }
}
