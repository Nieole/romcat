//! 界面上**画时刻只走这一处**：本地时间、短格式——今年的写「09-03 14:58」，不是今年的才带年份
//! （拿主意的人 2026-09-14 定，照设计稿）。
//!
//! **本地时区从系统时区库读**（`jiff`，只加在界面这一层；为什么是它写在根 `Cargo.toml` 那一条上）。库屏「上次扫描」、
//! 任务屏历史的「时间」、待确认屏裁决记录的时刻都走这里——任务屏从前另有一份（票 `gui-looks-like-the-design/25`），
//! 库屏这一份的偏移却交 0，同一个窗口里两处一个本地、一个 UTC（票 `gui-looks-like-the-design/18` 并成这一处）。
//!
//! **截图里不许有当前时间**：截图测试钉死一个「此刻」与一个固定偏移（[`Clock::fixed`]），「今年」
//! 才不跟着跑测试的那一天变。
//!
//! 公历换算照旧走核心库那一处（`romcat_core::report::human_time`）：这里只加偏移、去掉今年的年份。
//! **核心库与命令行照旧 UTC**（挂单 `Q901`）。

use std::time::{SystemTime, UNIX_EPOCH};

use romcat_core::report::human_time;

/// 画时刻时拿哪一刻当「此刻」、本地比 UTC 快多少秒。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Clock {
    /// 读系统时钟与系统时区。
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

    /// 此刻，UNIX 纪元起的秒：读系统时钟，或者钉死的那一刻。「上次体检」记的就是它（`crate::health`）。
    #[must_use]
    pub fn now(self) -> i64 {
        match self {
            Self::System => system_now(),
            Self::Fixed { now, .. } => now,
        }
    }

    /// `at`（UNIX 纪元起的秒）画成本地时间的短格式：与此刻同一年写 `MM-DD HH:MM`，否则 `YYYY-MM-DD HH:MM`。
    #[must_use]
    pub fn short(self, at: i64) -> String {
        // 系统时区**按那一刻各读一次**：夏令时前后，同一个时区快的秒数不一样。
        let (now, 那一刻的偏移, 此刻的偏移) = match self {
            Self::System => {
                let now = system_now();
                (now, local_offset(at), local_offset(now))
            }
            Self::Fixed { now, offset } => (now, offset, offset),
        };
        let 那一刻 = human_time(at.saturating_add(i64::from(那一刻的偏移)));
        let 此刻 = human_time(now.saturating_add(i64::from(此刻的偏移)));
        match (那一刻.split_once('-'), 此刻.split_once('-')) {
            (Some((那年, 月日时分)), Some((今年, _))) if 那年 == 今年 => {
                月日时分.to_string()
            }
            _ => 那一刻,
        }
    }
}

/// 那一刻本地时区比 UTC 快多少秒：读系统时区库。时刻出了 `jiff` 认的范围就当 UTC。
fn local_offset(secs: i64) -> i32 {
    jiff::Timestamp::from_second(secs)
        .map_or(0, |at| jiff::tz::TimeZone::system().to_offset(at).seconds())
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
    fn 系统钟按本机时区画_不是按_utc() {
        // 真窗口那一路从前偏移交 0：库屏「上次扫描」画的是 UTC，任务屏历史画的却是本地时间。
        // 取一个不是今年的时刻，结果就不跟着「此刻」变；本机时区照系统时区库读（与画的那一处同一个来源）。
        // 本机若恰好就在 UTC，这一条验不出偏移接没接上。
        let 那一刻 = 1_700_000_000;
        let 本机偏移 = jiff::tz::TimeZone::system()
            .to_offset(jiff::Timestamp::from_second(那一刻).expect("在范围里"))
            .seconds();
        let 十年后 = 那一刻 + 86_400 * 3_650;
        assert_eq!(
            Clock::System.short(那一刻),
            Clock::fixed(十年后, 本机偏移).short(那一刻),
        );
    }

    #[test]
    fn 偏移跨过零点与年份时_日子与年份都照本地算() {
        let 此刻 = 九月三号;
        assert_eq!(Clock::fixed(此刻, 28_800).short(除夕晚上), "01-01 04:00");
        assert_eq!(Clock::fixed(此刻, 0).short(除夕晚上), "2025-12-31 20:00");
    }
}
