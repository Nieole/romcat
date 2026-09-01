//! **字段级多源优先级**：哪个源的哪个字段说了算。
//!
//! ## 为什么优先级是按字段而不是按源
//!
//! 因为**不同字段的最佳来源不同**，而且这个库上有硬证据：
//!
//! | 字段 | 谁给得出 |
//! |---|---|
//! | 标题 | No-Intro / Redump 的条目名最整齐；TOSEC 的名字里塞满了年份与小组名 |
//! | **年份** | **只有 TOSEC**——No-Intro 与 Redump 的名字里根本没有 |
//! | **发行商** | **只有 TOSEC** |
//! | 汉化组 | 只有 TOSEC 的 `[tr zh …]` |
//!
//! 按源排序的话，「No-Intro 优先」等于把年份与发行商一起丢掉。Skyscraper 的
//! `priorities.xml` 早就是按字段排的（调研 13.1/13.3(5)），这不是新发明。
//!
//! ## 三条规则
//!
//! 1. **人工来源永远排最前。** `裁决`（票 08）排在每条链的第一位，于是人改过的东西
//!    永远不会被任何数据源覆盖。Skyscraper 的文档也是这么写的：手工添加的资源
//!    "will be prioritized above all others"。
//! 2. **按平台可覆写。** 街机那一栏 MAME 说了算（ADR-0010），别的平台上 MAME 是逐芯片的
//!    记录，标题反而不如 No-Intro。
//! 3. **没列到的源不报错，按时间戳兜底**（新的优先）。加一个源不必先改配置——
//!    这条是「以后还要加在线源」的前提。
//!
//! ## 它是**数据不是代码**
//!
//! 与平台清单、数据源清单一样：工具内置一份，`--priorities <文件>` 整份换掉，
//! `romcat scrape --dump-priorities` 导得出底稿。

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::catalog::scrape::ScrapedValue;

/// 内置的那一份优先级表。
const BUILTIN: &str = include_str!("priorities.toml");

/// 本程序认得的优先级表版本。
pub const PRIORITIES_VERSION: u32 = 1;

/// **人工来源**在优先级表里叫什么。它排在每条链的第一位。
pub const VERDICT: &str = "裁决";

/// 优先级表本身写坏了。
#[derive(Debug, thiserror::Error)]
pub enum PriorityError {
    /// 文件读不出来。
    #[error("优先级表 {path} 读不出来：{source}")]
    Io {
        /// 出问题的文件。
        path: String,
        /// 底层错误。
        source: std::io::Error,
    },
    /// TOML 语法或结构不对。
    #[error("优先级表 {path} 读不动：{source}")]
    Toml {
        /// 出问题的文件。
        path: String,
        /// 底层错误。
        source: toml::de::Error,
    },
    /// 版本对不上。
    #[error("优先级表 {path} 的版本是 {found}，本程序认得的是 {PRIORITIES_VERSION}")]
    Version {
        /// 出问题的文件。
        path: String,
        /// 文件里写的版本。
        found: u32,
    },
    /// 内容自相矛盾。
    #[error("优先级表 {path} 里 {detail}")]
    Invalid {
        /// 出问题的文件。
        path: String,
        /// 哪里不对。
        detail: String,
    },
}

/// 一份字段级优先级表。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Priorities {
    orders: BTreeMap<String, Vec<String>>,
    overrides: BTreeMap<(String, String), Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct RawPriorities {
    #[serde(rename = "版本")]
    version: u32,
    #[serde(rename = "字段", default)]
    fields: Vec<RawOrder>,
    #[serde(rename = "平台覆盖", default)]
    overrides: Vec<RawOverride>,
}

#[derive(Debug, Deserialize)]
struct RawOrder {
    #[serde(rename = "名")]
    field: String,
    #[serde(rename = "顺序")]
    order: Vec<String>,
    #[serde(rename = "说明", default)]
    #[allow(dead_code)]
    note: String,
}

#[derive(Debug, Deserialize)]
struct RawOverride {
    #[serde(rename = "平台")]
    platform: String,
    #[serde(rename = "字段")]
    field: String,
    #[serde(rename = "顺序")]
    order: Vec<String>,
    #[serde(rename = "说明", default)]
    #[allow(dead_code)]
    note: String,
}

impl Priorities {
    /// 内置的那一份。
    ///
    /// # Panics
    /// 内置的表写坏了就是编译期该发现的错，这里直接崩——它有测试盯着。
    #[must_use]
    pub fn builtin() -> Self {
        Self::parse(BUILTIN, "（内置）").expect("内置的优先级表必须读得动")
    }

    /// 内置那一份的原文，供 `--dump-priorities` 导出底稿。
    #[must_use]
    pub fn builtin_text() -> &'static str {
        BUILTIN
    }

    /// 从文件读一份。
    ///
    /// # Errors
    /// 读不出来、读不动、版本对不上、或者内容自相矛盾时返回错误。
    pub fn load(path: &Path) -> Result<Self, PriorityError> {
        let display = crate::path::display(path);
        let text = std::fs::read_to_string(path).map_err(|source| PriorityError::Io {
            path: display.clone(),
            source,
        })?;
        Self::parse(&text, &display)
    }

    fn parse(text: &str, path: &str) -> Result<Self, PriorityError> {
        let raw: RawPriorities = toml::from_str(text).map_err(|source| PriorityError::Toml {
            path: path.to_string(),
            source,
        })?;
        if raw.version != PRIORITIES_VERSION {
            return Err(PriorityError::Version {
                path: path.to_string(),
                found: raw.version,
            });
        }
        let mut orders = BTreeMap::new();
        for order in raw.fields {
            if order.order.is_empty() {
                return Err(PriorityError::Invalid {
                    path: path.to_string(),
                    detail: format!("字段「{}」的顺序是空的", order.field),
                });
            }
            if orders.insert(order.field.clone(), order.order).is_some() {
                return Err(PriorityError::Invalid {
                    path: path.to_string(),
                    detail: format!("字段「{}」写了两遍", order.field),
                });
            }
        }
        let mut overrides = BTreeMap::new();
        for over in raw.overrides {
            let key = (over.platform.clone(), over.field.clone());
            if over.order.is_empty() {
                return Err(PriorityError::Invalid {
                    path: path.to_string(),
                    detail: format!("{} 上「{}」的覆盖顺序是空的", over.platform, over.field),
                });
            }
            if overrides.insert(key, over.order).is_some() {
                return Err(PriorityError::Invalid {
                    path: path.to_string(),
                    detail: format!("{} 上「{}」的覆盖写了两遍", over.platform, over.field),
                });
            }
        }
        Ok(Self { orders, overrides })
    }

    /// 某个字段在某个平台上的顺序。**按平台的覆盖优先**，没有覆盖就用通用那条。
    #[must_use]
    pub fn order(&self, field: &str, platform: Option<&str>) -> &[String] {
        if let Some(platform) = platform
            && let Some(order) = self
                .overrides
                .get(&(platform.to_string(), field.to_string()))
        {
            return order;
        }
        self.orders.get(field).map_or(&[], Vec::as_slice)
    }

    /// 这个源在这个字段上排第几。**没列到的排在全部列到的之后**。
    #[must_use]
    pub fn rank(&self, field: &str, platform: Option<&str>, source: &str) -> usize {
        self.order(field, platform)
            .iter()
            .position(|name| name == source)
            .unwrap_or(usize::MAX)
    }

    /// 表里列到了哪些字段。
    #[must_use]
    pub fn fields(&self) -> Vec<&str> {
        self.orders.keys().map(String::as_str).collect()
    }

    /// 按平台的覆盖有哪几条。
    #[must_use]
    pub fn platform_overrides(&self) -> Vec<(&str, &str, &[String])> {
        self.overrides
            .iter()
            .map(|((platform, field), order)| (platform.as_str(), field.as_str(), order.as_slice()))
            .collect()
    }

    /// 从一堆候选值里选出胜出的那一个。
    ///
    /// 排序键三层，缺一不可：
    ///
    /// 1. **优先级表里的名次**——这是它存在的理由；
    /// 2. **采集时刻，新的优先**——没列到的源之间靠它分先后（调研 13.3(4)）；
    /// 3. **源名**——前两层平手时定死顺序，同一份库跑两次结果必须一样。
    #[must_use]
    pub fn pick<'a>(
        &self,
        field: &str,
        platform: Option<&str>,
        values: &'a [ScrapedValue],
    ) -> Option<&'a ScrapedValue> {
        values
            .iter()
            .filter(|value| value.field == field)
            .min_by_key(|value| {
                (
                    self.rank(field, platform, &value.source),
                    std::cmp::Reverse(value.at),
                    value.source.clone(),
                )
            })
    }

    /// 一个锚点上全部字段各自的胜出值：字段 → 那一条。
    #[must_use]
    pub fn merge<'a>(
        &self,
        platform: Option<&str>,
        values: &'a [ScrapedValue],
    ) -> BTreeMap<String, &'a ScrapedValue> {
        let mut out = BTreeMap::new();
        for value in values {
            if out.contains_key(&value.field) {
                continue;
            }
            if let Some(best) = self.pick(&value.field, platform, values) {
                out.insert(value.field.clone(), best);
            }
        }
        out
    }
}

impl Default for Priorities {
    fn default() -> Self {
        Self::builtin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 值(field: &str, source: &str, value: &str, at: i64) -> ScrapedValue {
        ScrapedValue {
            field: field.to_string(),
            source: source.to_string(),
            value: value.to_string(),
            evidence: String::new(),
            at,
        }
    }

    #[test]
    fn 内置那份读得动() {
        let priorities = Priorities::builtin();
        assert!(priorities.fields().contains(&"标题"));
        assert_eq!(
            priorities.order("标题", None).first().map(String::as_str),
            Some(VERDICT)
        );
    }

    #[test]
    fn 用一个源的标题配另一个源的年份() {
        let priorities = Priorities::builtin();
        let values = vec![
            值("标题", "TOSEC", "1942 TOSEC 版写法", 100),
            值("标题", "No-Intro", "1942", 100),
            值("年份", "TOSEC", "1985", 100),
        ];
        let merged = priorities.merge(Some("FC"), &values);
        assert_eq!(merged["标题"].source, "No-Intro");
        assert_eq!(merged["年份"].source, "TOSEC");
    }

    #[test]
    fn 按平台的覆盖压过通用那条() {
        let priorities = Priorities::builtin();
        let values = vec![
            值("标题", "No-Intro", "Contra (USA)", 100),
            值("标题", "MAME", "Contra", 100),
        ];
        assert_eq!(
            priorities.merge(Some("FC"), &values)["标题"].source,
            "No-Intro"
        );
        assert_eq!(
            priorities.merge(Some("街机"), &values)["标题"].source,
            "MAME"
        );
    }

    #[test]
    fn 没列到的源排在后面且新的优先() {
        let priorities = Priorities::builtin();
        let values = vec![
            值("标题", "某个没列到的源", "旧的", 100),
            值("标题", "另一个没列到的源", "新的", 200),
            值("标题", "文件名", "文件名给的", 100),
        ];
        // 文件名列在表里，排在两个没列到的之前。
        assert_eq!(priorities.merge(None, &values)["标题"].value, "文件名给的");

        let values = vec![
            值("标题", "某个没列到的源", "旧的", 100),
            值("标题", "另一个没列到的源", "新的", 200),
        ];
        assert_eq!(priorities.merge(None, &values)["标题"].value, "新的");
    }

    #[test]
    fn 人工来源排在最前() {
        let priorities = Priorities::builtin();
        for field in priorities.fields() {
            assert_eq!(
                priorities.order(field, None).first().map(String::as_str),
                Some(VERDICT),
                "字段「{field}」没有把人工来源排在最前"
            );
        }
    }

    #[test]
    fn 空顺序是错的() {
        let text = "\"版本\" = 1\n[[\"字段\"]]\n\"名\" = \"标题\"\n\"顺序\" = []\n";
        assert!(matches!(
            Priorities::parse(text, "（测试）"),
            Err(PriorityError::Invalid { .. })
        ));
    }

    #[test]
    fn 版本对不上就拒() {
        let text = "\"版本\" = 99\n";
        assert!(matches!(
            Priorities::parse(text, "（测试）"),
            Err(PriorityError::Version { found: 99, .. })
        ));
    }
}
