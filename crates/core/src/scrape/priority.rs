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

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::adapter::converge::{self, Anchor};
use crate::catalog::scrape::ScrapedValue;
use crate::catalog::{Catalog, CatalogError};
use crate::scrape::{AnchorKind, Field};
use crate::title::{Chosen, TitleSet};

/// 内置的那一份优先级表。
const BUILTIN: &str = include_str!("priorities.toml");

/// 本程序认得的优先级表版本。
pub const PRIORITIES_VERSION: u32 = 1;

/// **人工来源**在优先级表里叫什么。它排在每条链的第一位。
pub const VERDICT: &str = "裁决";

/// **固定在每条链最前、挪不动的那几家**，照这个次序：[`VERDICT`]，然后是手工维护的
/// 元数据——每个前端格式各一家（[`crate::adapter::names`]：`Pegasus`、`ES-Gamelist`）。
///
/// 人改过的东西不许被任何数据源覆盖（ADR-0001）；手工维护的那两份是维护者多年养出来的，
/// 一次刮削盖掉它们正是那条 ADR 说不可接受的那件事（`priorities.toml` 的规则一）。
/// 界面上那一层据此画「固定」、挪的时候据此拒（[`Priorities::raise`]）。
#[must_use]
pub fn pinned() -> Vec<&'static str> {
    let mut out = vec![VERDICT];
    out.extend(crate::adapter::names());
    out
}

/// 这个源是不是**固定**的那几家（[`pinned`]）。
#[must_use]
pub fn is_pinned(source: &str) -> bool {
    pinned().contains(&source)
}

/// 几份表里提到的字段：**[`Field::all`] 那七个总在、照那个次序**，表里写着的表外字段排在
/// 后面（码位序）。
///
/// 写成文本（[`Priorities::to_text`]）、界面上左边那一列、「保存后的变化」列哪几个字段
/// （[`shifts`]），都照这一个次序。
#[must_use]
pub fn listed_fields(tables: &[&Priorities]) -> Vec<String> {
    let mut out: Vec<String> = Field::all()
        .into_iter()
        .map(|field| field.label().to_string())
        .collect();
    let extra: BTreeSet<&String> = tables
        .iter()
        .copied()
        .flat_map(|table| {
            table
                .orders
                .keys()
                .chain(table.overrides.keys().map(|(_, field)| field))
        })
        .filter(|field| Field::from_label(field).is_none())
        .collect();
    out.extend(extra.into_iter().cloned());
    out
}

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
    /// 写不进去（[`Priorities::save`]）。
    #[error("优先级表 {path} 写不进去：{source}")]
    Write {
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

/// 写回的那一份开头那几行注释。
///
/// **每条的「说明」不带出去**（[`Priorities::to_text`]），于是指回内置底稿：理由都在那儿。
/// 手工维护的那几家叫什么从 [`pinned`] 取，不写死。
fn written_header() -> String {
    format!(
        "# 字段级多源优先级：哪个源的哪个字段说了算。这一份是界面上改完写回来的。\n\
         #\n\
         # 每条为什么这么排、三条规则怎么说，看内置那一份底稿：\n\
         #   romcat scrape --dump-priorities <文件>\n\
         # `{VERDICT}` 永远排第一，手工维护的元数据（{}）紧随其后；\n\
         # 没列到的源排在全部列到的之后，彼此按采集时刻排（新的优先）。\n\n",
        pinned()
            .into_iter()
            .filter(|source| *source != VERDICT)
            .collect::<Vec<_>>()
            .join("、"),
    )
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

    /// **写成文本**：读回来与这一份一模一样（往返测试钉着），版本号照写
    /// [`PRIORITIES_VERSION`]，读的那一侧照旧拿它校验。
    ///
    /// 字段照 [`Field::all`] 的次序，表外的字段排在后面；按平台的覆盖照平台名排。
    ///
    /// **每条的「说明」不带出去**：这张表只记排序，而说明是内置那一份写给改表的人看的
    /// 理由——排序一改，那几句话就可能不再成立。开头几行注释指回内置底稿。
    ///
    /// ## 结构手写，值交给 `toml` 转义
    ///
    /// 整份交给 `toml::to_string` 是不行的（往返测试抓出来的）：它把表数组的头写成不带
    /// 引号的 `[[字段]]`，而它自己的解析器不认中文的裸键，读回来当场报错。于是键照读的
    /// 那一侧（`RawPriorities` 的 `rename`）一个个写死、带着引号；字符串与列表里的引号、
    /// 反斜杠、换行交给 `toml::Value` 自己转义。
    #[must_use]
    pub fn to_text(&self) -> String {
        use std::fmt::Write as _;

        let text = |value: &str| toml::Value::String(value.to_string()).to_string();
        let list = |order: &[String]| {
            toml::Value::Array(order.iter().cloned().map(toml::Value::String).collect()).to_string()
        };
        let mut out = written_header();
        let _ = writeln!(out, "\"版本\" = {PRIORITIES_VERSION}");
        for field in listed_fields(&[self]) {
            let Some(order) = self.orders.get(&field) else {
                continue;
            };
            let _ = write!(
                out,
                "\n[[\"字段\"]]\n\"名\" = {}\n\"顺序\" = {}\n",
                text(&field),
                list(order),
            );
        }
        for ((platform, field), order) in &self.overrides {
            let _ = write!(
                out,
                "\n[[\"平台覆盖\"]]\n\"平台\" = {}\n\"字段\" = {}\n\"顺序\" = {}\n",
                text(platform),
                text(field),
                list(order),
            );
        }
        out
    }

    /// **写到 `path`**：先写进旁边一份临时文件，再改名换上去。
    ///
    /// 读这份表的那几条路（刮削、整理标题、导出、同步——命令行与界面都是）每一趟开头读
    /// 一次。换上去是一下子的事，于是它们读到的要么是旧的一份、要么是新的一份，不会是
    /// 写到一半的：半份表读不动，而读不动就停下，不退回内置那份。
    ///
    /// **只写这一份文件**，不排任何刮削任务：刮削结果按「锚点 × 字段 × 源」并存，换表
    /// 只是换一次排序。写回工作目录那一份时，路径取
    /// [`workspace::priorities_path`](crate::workspace::priorities_path)——读的那一侧用的是同一个。
    ///
    /// # Errors
    /// 建目录、写临时文件或改名失败时返回 [`PriorityError::Write`]。
    pub fn save(&self, path: &Path) -> Result<(), PriorityError> {
        let failed = |at: &Path, source: std::io::Error| PriorityError::Write {
            path: crate::path::display(at),
            source,
        };
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|error| failed(parent, error))?;
        }
        let mut temp = path.as_os_str().to_os_string();
        temp.push(".tmp");
        let temp = PathBuf::from(temp);
        std::fs::write(&temp, self.to_text()).map_err(|error| failed(&temp, error))?;
        std::fs::rename(&temp, path).map_err(|error| failed(path, error))
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

    /// 表里点名了、而 `known` 里没有的源，去重后按字典序。
    ///
    /// **不是错误**：规则三说得很清楚，没列到的源按时间戳兜底，反过来列了不存在的源
    /// 也只是排序时永远轮不到它。但那是**静默**的——用户以为自己调了优先级，实际什么
    /// 也没发生。所以报告要点名。人工来源（[`VERDICT`]）不算：它到票 08 才产出值，
    /// 位置先占着是有意的。
    #[must_use]
    pub fn sources_not_in(&self, known: &[&str]) -> Vec<String> {
        let mut out: BTreeSet<&str> = BTreeSet::new();
        for order in self.orders.values().chain(self.overrides.values()) {
            for source in order {
                if source != VERDICT && !known.contains(&source.as_str()) {
                    out.insert(source);
                }
            }
        }
        out.into_iter().map(ToString::to_string).collect()
    }

    /// 按平台的覆盖有哪几条。
    #[must_use]
    pub fn platform_overrides(&self) -> Vec<(&str, &str, &[String])> {
        self.overrides
            .iter()
            .map(|((platform, field), order)| (platform.as_str(), field.as_str(), order.as_slice()))
            .collect()
    }

    /// 这个字段在两份表里排得一样不一样：通用那条，连同每个平台的覆盖。
    #[must_use]
    pub fn differs_in(&self, other: &Self, field: &str) -> bool {
        let mine = self.overrides.iter().filter(|((_, of), _)| of == field);
        let theirs = other.overrides.iter().filter(|((_, of), _)| of == field);
        self.orders.get(field) != other.orders.get(field) || !mine.eq(theirs)
    }

    /// 这条链上第 `at` 家**往前挪得动吗**：它自己与它前面那一家都不是固定的那几家
    /// （[`is_pinned`]）。
    ///
    /// **固定的那几家在哪儿都挪不动，别的源也换不到它们那一侧去。** 手写的表没把它们排在
    /// 最前时，这一层不替人重排——只是谁也跨不过它们。
    ///
    /// `platform` 给了就问那个平台的覆盖；那条覆盖不在就是挪不动，**不会退到通用那条上去**。
    #[must_use]
    pub fn can_raise(&self, field: &str, platform: Option<&str>, at: usize) -> bool {
        at > 0 && self.can_swap_with_next(field, platform, at - 1)
    }

    /// 第 `at` 家**往后挪得动吗**。规矩同 [`Priorities::can_raise`]。
    #[must_use]
    pub fn can_lower(&self, field: &str, platform: Option<&str>, at: usize) -> bool {
        self.can_swap_with_next(field, platform, at)
    }

    /// 把第 `at` 家往前挪一位；挪不动（[`Priorities::can_raise`]）就什么都不做。挪成了返回 `true`。
    pub fn raise(&mut self, field: &str, platform: Option<&str>, at: usize) -> bool {
        self.can_raise(field, platform, at) && self.swap_with_next(field, platform, at - 1)
    }

    /// 把第 `at` 家往后挪一位；挪不动（[`Priorities::can_lower`]）就什么都不做。挪成了返回 `true`。
    pub fn lower(&mut self, field: &str, platform: Option<&str>, at: usize) -> bool {
        self.can_lower(field, platform, at) && self.swap_with_next(field, platform, at)
    }

    /// 挪的时候动的是哪一条：给了平台就是那个平台的覆盖，没给就是通用那条。
    fn own_order(&self, field: &str, platform: Option<&str>) -> Option<&Vec<String>> {
        match platform {
            Some(platform) => self
                .overrides
                .get(&(platform.to_string(), field.to_string())),
            None => self.orders.get(field),
        }
    }

    /// 第 `at` 家与它后面那一家换得了位吗：两家都在，都不是固定的那几家。
    fn can_swap_with_next(&self, field: &str, platform: Option<&str>, at: usize) -> bool {
        self.own_order(field, platform).is_some_and(|order| {
            at + 1 < order.len() && !is_pinned(&order[at]) && !is_pinned(&order[at + 1])
        })
    }

    /// 第 `at` 家与它后面那一家换位。**先问过 [`Self::can_swap_with_next`] 再调它。**
    fn swap_with_next(&mut self, field: &str, platform: Option<&str>, at: usize) -> bool {
        let order = match platform {
            Some(platform) => self
                .overrides
                .get_mut(&(platform.to_string(), field.to_string())),
            None => self.orders.get_mut(field),
        };
        order.is_some_and(|order| {
            order.swap(at, at + 1);
            true
        })
    }

    /// 一条值在这个字段上排第几的**排序键**：[`Priorities::pick`] 那四层（表里的名次、采集时刻新的优先、源名、
    /// 值本身）。挑出胜出的那一条与把几条排出先后（[`entry_fields`]）用的是同一把键——两处各写一遍，
    /// 「排第一的」与「挑出来的」迟早对不上。
    fn order_key(
        &self,
        field: &str,
        platform: Option<&str>,
        value: &ScrapedValue,
    ) -> (usize, std::cmp::Reverse<i64>, String, String) {
        (
            self.rank(field, platform, &value.source),
            std::cmp::Reverse(value.at),
            value.source.clone(),
            value.value.clone(),
        )
    }

    /// 从一堆候选值里选出胜出的那一个。
    ///
    /// 排序键四层，缺一不可：
    ///
    /// 1. **优先级表里的名次**——这是它存在的理由；
    /// 2. **采集时刻，新的优先**——没列到的源之间靠它分先后（调研 13.3(4)）；
    /// 3. **源名**——前三层平手时定死顺序；
    /// 4. **值本身**——同一个源在同一个字段上说得出好几句话（标题集合那一种形状），
    ///    前三层对它们完全相同。少了这一层，挑出来的是库里恰好先返回的那一条，
    ///    **同一份库跑两次结果可能不一样**。
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
            .min_by_key(|value| self.order_key(field, platform, value))
    }

    /// 胜出那个**源**在这个字段上说的**全部**值。
    ///
    /// [`Priorities::pick`] 挑的是一条，那对单值字段（标题、年份、简介）正合适。可开发商、
    /// 发行商与类型是**集合字段**——数据源一个键写了几家（`|开发= 科乐美、KCE东京`）、
    /// 维护者用续行写了两行，中立库里就是几行。这时挑一条等于**换掉一家公司**：前三层
    /// 排序键（表里的名次、采集时刻、源名）在同一个源的几条值上完全平手，第四层比的是
    /// 值本身，也就是**码位序**——挑出来的既不是第一家也不是主要那家（挂单 Q27）。
    ///
    /// 所以这里先用同一套优先级选出**哪个源说了算**，再把那个源在这个字段上的话全都交出来。
    /// **不跨源合并**：两个源各说各的「Konami」与「KONAMI」并成两条，那不是集合是重复。
    ///
    /// 值的先后**照传进来的次序**，而导出那条链传进来的是码位序
    /// （[`Catalog::for_each_scraped_value`](crate::catalog::Catalog::for_each_scraped_value)
    /// 的排序键；`ScrapedValue` 上没有数据源原次序那一列）。**少一家比排错序坏得多**，
    /// 所以这一步先把「全都在」做到；原次序要不要一路带到这里，是 Q27 留给拿主意的人的那半。
    /// 拿这个次序去覆盖用户手写的排版是不行的——那一侧比的是集合而不是列表
    /// （`adapter::pegasus` 的 `same_values`）。
    #[must_use]
    pub fn pick_all<'a>(
        &self,
        field: &str,
        platform: Option<&str>,
        values: &'a [ScrapedValue],
    ) -> Vec<&'a ScrapedValue> {
        let Some(winner) = self.pick(field, platform, values) else {
            return Vec::new();
        };
        values
            .iter()
            .filter(|value| value.field == field && value.source == winner.source)
            .collect()
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

/// 一处显示值**是谁说的、说的什么**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    /// 哪个源。作品的标题集合是空的、退回作品名时是 `None`。
    pub source: Option<String>,
    /// 说的什么。集合字段（开发商、发行商、类型）是那个源在这个字段上的全部值
    /// （[`Priorities::pick_all`]），显示标题是挑出来的那一个。
    pub values: Vec<String>,
}

/// 换一份表之后，**一个条目上一个字段写出去的值**从什么变成什么。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shift {
    /// 条目挂在哪一层：认出了作品的挂作品，没认出的挂那个变体。
    pub anchor: AnchorKind,
    /// 作品名，或变体的键。
    pub subject: String,
    /// 换之前。
    pub before: Said,
    /// 换之后。
    pub after: Said,
}

/// 一个字段上会变的那几处：作品几个、变体几个，外加头一处当例子。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldShifts {
    /// 字段。
    pub field: String,
    /// 显示值会变的作品有几个。一部作品横跨几个平台、在几个平台上都变了，也只算一个。
    pub works: u64,
    /// 显示值会变的、没认出作品的变体有几个。
    pub variants: u64,
    /// 头一处，照走到的次序（平台照码位序；同一平台里作品在前、没认出作品的变体在后）。
    /// 一处都不变时是 `None`。
    pub example: Option<Shift>,
}

impl FieldShifts {
    /// 记下一处。
    fn count(&mut self, shift: Shift) {
        match shift.anchor {
            AnchorKind::Work => self.works += 1,
            AnchorKind::Variant => self.variants += 1,
        }
        if self.example.is_none() {
            self.example = Some(shift);
        }
    }
}

/// **换一份优先级表，导出去的显示值会变几处**：按字段数一数，每个字段举一个例子。
///
/// 只读中立库、一个字节都不写，**也不重采**：刮削结果按「锚点 × 字段 × 源」并存，两份表
/// 各折一遍、比一比就是答案。
///
/// ## 照导出真会写出去的算（ADR-0024）
///
/// 条目怎么摆（哪些变体不导出、条目挂作品还是挂变体、首选变体是谁、条目读哪几条值）问的是
/// 导出那一侧的 `converge::layout`，每个字段写出去的是什么问的是 `converge::shown`——
/// 导出折条目照的也是这两处。于是：补丁、附属内容、非游戏资产不算；认出了作品的条目只看
/// 作品上的值（汉化组看首选变体身上的）；作品的显示标题是标题集合挑出来的那一个
/// （[`crate::title::choose`]，平台不参与，按平台的覆盖改了它不跟着变）；年份、简介这些
/// 只写一条的字段比的是挑出来的那一条，开发商、发行商、类型比的是那个源的全部值。
///
/// **写出去的字变了才算一处**：两个源说的都是 `1985` 时，换谁说了算，前端里看不出来。
///
/// ## 列哪几个字段
///
/// 两份表**排得不一样**的那几个（[`Priorities::differs_in`]），照 [`listed_fields`] 的次序。
/// 排得不一样、这份库上却一处都不变的字段照样列出来（例子是 `None`）：人要知道「改了，但在
/// 这份库上不起作用」。表外的字段导出不写，永远一处都不变。两份一样时交空的，一行库都不读。
///
/// # Errors
/// 读中立库失败时返回错误。
pub fn shifts(
    catalog: &Catalog,
    before: &Priorities,
    after: &Priorities,
) -> Result<Vec<FieldShifts>, CatalogError> {
    let mut found: Vec<FieldShifts> = listed_fields(&[before, after])
        .into_iter()
        .filter(|field| before.differs_in(after, field))
        .map(|field| FieldShifts {
            field,
            ..FieldShifts::default()
        })
        .collect();
    if found.is_empty() {
        return Ok(found);
    }
    let layout = converge::layout(catalog, None)?;
    // 标题集合挑显示标题要照着表挑一遍；标题那一栏没改就不必挑。
    let titles_changed = found
        .iter()
        .any(|shifts| shifts.field == Field::Title.label());
    let (chosen_before, chosen_after): (BTreeMap<String, Chosen>, BTreeMap<String, Chosen>) =
        if titles_changed {
            (
                converge::chosen_titles(catalog, before)?,
                converge::chosen_titles(catalog, after)?,
            )
        } else {
            (BTreeMap::new(), BTreeMap::new())
        };

    // 一部作品横跨几个平台是几个条目，但「哪几部作品会变」只算一次。
    let mut counted: BTreeSet<(usize, AnchorKind, &str)> = BTreeSet::new();
    for (platform, planned) in &layout.platforms {
        for one in planned {
            let (kind, subject) = match &one.anchor {
                Anchor::Work(work) => (AnchorKind::Work, work.as_str()),
                Anchor::Loose(key) => (AnchorKind::Variant, key.as_str()),
            };
            for (at, shifts) in found.iter_mut().enumerate() {
                let Some(field) = Field::from_label(&shifts.field) else {
                    continue;
                };
                if counted.contains(&(at, kind, subject)) {
                    continue;
                }
                let written = |table: &Priorities, chosen: &BTreeMap<String, Chosen>| {
                    converge::shown(
                        field,
                        platform,
                        layout.values(one),
                        layout.head_values(one),
                        chosen.get(one.anchor.name()),
                        table,
                    )
                };
                let (Some(was), Some(now)) = (
                    written(before, &chosen_before),
                    written(after, &chosen_after),
                ) else {
                    continue;
                };
                if was.values == now.values {
                    continue;
                }
                counted.insert((at, kind, subject));
                shifts.count(Shift {
                    anchor: kind,
                    subject: subject.to_string(),
                    before: was,
                    after: now,
                });
            }
        }
    }
    Ok(found)
}

/// 一个条目上**一个字段眼下写出去的是什么**，连这个字段上各个源各说了什么。
///
/// 作品详情页「元数据」那一面照它画：每一格写着眼下用的是哪个源的值（`shown`），「其他来源」列的是 `offered`
/// 里别的那几条，「使用这个值」拿其中一条写成裁决。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldShown {
    /// 哪个字段。
    pub field: Field,
    /// 写出去的是谁说的什么；这个字段写出去是空的时是 `None`。
    pub shown: Option<Said>,
    /// 这个字段上**每个源说的每一条**（裁决也在里头），照这份表的名次排（名次一样时照 [`Priorities::pick`] 那几层）。
    ///
    /// 条目挂在作品上时显示标题由标题集合挑，那一格这里是空的——叫法都在标题集合里。
    pub offered: Vec<ScrapedValue>,
}

/// 作品详情页上**哪几个变体摆汉化组那一行**、各自那一格写出去的是什么（票 `gui-looks-like-the-design/15`）。
///
/// 汉化组挂在变体上（ADR-0012）：认出作品的，**汉化版**（`Catalog::variant_kind` 照首选变体那条规则答，不带裁决）或者
/// 身上已经有汉化组值的变体各一行，次序照作品底下变体的次序；那一格照 [`entry_fields`] 算，与导出同一处。
///
/// # Errors
/// 读库失败时返回错误。
pub fn translation_groups(
    catalog: &Catalog,
    work: &crate::catalog::browse::WorkDetail,
    priorities: &Priorities,
) -> Result<Vec<(String, FieldShown)>, CatalogError> {
    let mut out = Vec::new();
    for variant in &work.variants {
        let key = variant.row.key.as_str();
        let fan = catalog.variant_kind(variant)? == Some(converge::Preference::FanTranslated);
        let platform = variant.row.platform.as_deref().unwrap_or("");
        if let Some(group) = entry_fields(
            catalog,
            AnchorKind::Variant,
            key,
            platform,
            Some(key),
            priorities,
        )?
        .into_iter()
        .find(|one| one.field == Field::TranslationGroup)
        .filter(|group| fan || !group.offered.is_empty())
        {
            out.push((key.to_owned(), group));
        }
    }
    Ok(out)
}

/// **一个条目上每个字段眼下写出去的是什么**，照 [`Field::all`] 的次序一格一格列。
///
/// 与导出折条目、换表之前数「哪几处显示值会变」（[`shifts`]）问的是**同一处**（`converge::shown`，ADR-0024）：
/// 认出了作品的条目读作品上的值，汉化组读首选变体身上的（`head`，给 `None` 就是没有），显示标题是标题集合挑出来的
/// 那一个（[`crate::title::choose`]）；没认出作品的条目读那个变体自己的值，显示标题照这份表挑。
///
/// `anchor` 与 `subject` 是条目挂在哪儿：作品与作品名，或变体与变体的键。`platform` 是这个条目在哪个平台上
/// ——按平台的覆盖照它取。
///
/// # Errors
/// 读中立库失败时返回错误。
pub fn entry_fields(
    catalog: &Catalog,
    anchor: AnchorKind,
    subject: &str,
    platform: &str,
    head: Option<&str>,
    priorities: &Priorities,
) -> Result<Vec<FieldShown>, CatalogError> {
    let values = catalog.scraped_values(anchor.label(), subject)?;
    let head_values = match head {
        Some(key) => catalog.scraped_values(AnchorKind::Variant.label(), key)?,
        None => Vec::new(),
    };
    let chosen = match anchor {
        AnchorKind::Work => Some(crate::title::choose(
            &TitleSet {
                work: subject.to_string(),
                entries: catalog.titles_of(subject)?,
            },
            priorities,
        )),
        AnchorKind::Variant => None,
    };
    Ok(Field::all()
        .into_iter()
        .map(|field| {
            let label = field.label();
            // 汉化组挂在变体上（ADR-0012），读首选变体身上的；别的字段读条目自己挂的那一层。
            let from: &[ScrapedValue] = if field == Field::TranslationGroup {
                &head_values
            } else {
                &values
            };
            let mut offered: Vec<ScrapedValue> = if field == Field::Title && chosen.is_some() {
                Vec::new()
            } else {
                from.iter()
                    .filter(|value| value.field == label)
                    .cloned()
                    .collect()
            };
            offered.sort_by_key(|value| priorities.order_key(label, Some(platform), value));
            FieldShown {
                field,
                shown: converge::shown(
                    field,
                    platform,
                    &values,
                    &head_values,
                    chosen.as_ref(),
                    priorities,
                ),
                offered,
            }
        })
        .collect())
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
    fn 集合字段交出胜出那个源说的全部值而不是挑一条() {
        // `|开发= 科乐美、KCE东京` 在中立库里是两行。挑一条的话，前三层排序键在同一个源
        // 的两条值上完全平手，第四层比码位——挑出来的是 `KCE东京`，**换的是一家公司**
        // 不是次序（挂单 Q27）。
        let priorities = Priorities::builtin();
        let values = vec![
            值("开发商", "中文离线源", "科乐美", 100),
            值("开发商", "中文离线源", "KCE东京", 100),
        ];
        let mut got: Vec<&str> = priorities
            .pick_all("开发商", Some("FC"), &values)
            .into_iter()
            .map(|value| value.value.as_str())
            .collect();
        got.sort_unstable();
        assert_eq!(got, ["KCE东京", "科乐美"], "两家都得在");
    }

    #[test]
    fn 集合字段不跨源合并_输的那个源一条都不带出来() {
        // 两个源各说各的不是集合，是重复。胜出的那个源说了算，另一个一条都不出。
        let priorities = Priorities::builtin();
        let values = vec![
            值("开发商", "TOSEC", "Konami", 100),
            值("开发商", "中文离线源", "科乐美", 100),
            值("开发商", "中文离线源", "KCE东京", 100),
        ];
        let 胜出源 = priorities
            .pick("开发商", Some("FC"), &values)
            .expect("挑得出")
            .source
            .clone();
        let got = priorities.pick_all("开发商", Some("FC"), &values);
        assert!(
            got.iter().all(|value| value.source == 胜出源),
            "只该有胜出那个源的话：{got:?}"
        );
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
    fn 表里点名了却不存在的源报得出来() {
        let priorities = Priorities::builtin();
        let 全都在 = crate::scrape::all_source_names();
        assert!(priorities.sources_not_in(&全都在).is_empty());
        // 少了一个 TOSEC，就该点它的名——而不是让它静默地排到链尾。
        let 缺一个: Vec<&str> = 全都在.into_iter().filter(|name| *name != "TOSEC").collect();
        assert_eq!(
            priorities.sources_not_in(&缺一个),
            vec!["TOSEC".to_string()]
        );
    }

    #[test]
    fn 空顺序是错的() {
        let text = "\"版本\" = 1\n[[\"字段\"]]\n\"名\" = \"标题\"\n\"顺序\" = []\n";
        assert!(matches!(
            Priorities::parse(text, "（测试）"),
            Err(PriorityError::Invalid { .. })
        ));
    }

    /// 往返那条测试用的几份表：内置那份，外加几份各带一样怪处的。
    ///
    /// 怪处挑的是「写成文本时最容易写坏」的那几样：源名、字段名、平台名里带引号、反斜杠、
    /// 井号（TOML 的注释起头）与换行；表外的字段名；不止一个平台的覆盖；一条覆盖都没有、
    /// 一个字段都没有。
    fn 各带一样怪处的表() -> Vec<Priorities> {
        let 读 = |text: &str| Priorities::parse(text, "（测试）").expect("手写的表读得动");
        vec![
            Priorities::builtin(),
            读(concat!(
                "\"版本\" = 1\n",
                "[[\"字段\"]]\n\"名\" = \"简介\"\n",
                "\"顺序\" = [\"裁决\", \"Pegasus\", \"ES-Gamelist\", \"中文离线源\", \"ScreenScraper\"]\n",
                "[[\"字段\"]]\n\"名\" = \"表外的\\\"字段\\\" # 一\\n二\"\n",
                "\"顺序\" = [\"带\\\"引号\\\"的源\", \"反斜杠\\\\源\", \"带 # 井号\", \"两行\\n的源\"]\n",
                "[[\"平台覆盖\"]]\n\"平台\" = \"街机\"\n\"字段\" = \"标题\"\n",
                "\"顺序\" = [\"裁决\", \"MAME\", \"文件名\"]\n",
                "[[\"平台覆盖\"]]\n\"平台\" = \"FC\"\n\"字段\" = \"年份\"\n",
                "\"顺序\" = [\"TOSEC\"]\n",
                "[[\"平台覆盖\"]]\n\"平台\" = \"反斜杠\\\\平台 # \\\"甲\\\"\"\n",
                "\"字段\" = \"表外的\\\"字段\\\" # 一\\n二\"\n",
                "\"顺序\" = [\"两行\\n的源\"]\n",
            )),
            读("\"版本\" = 1\n[[\"字段\"]]\n\"名\" = \"标题\"\n\"顺序\" = [\"文件名\"]\n"),
            读("\"版本\" = 1\n"),
        ]
    }

    #[test]
    fn 导出成文本再读回来一模一样() {
        for 原来的 in 各带一样怪处的表() {
            let text = 原来的.to_text();
            let 读回来的 = Priorities::parse(&text, "（导出）")
                .unwrap_or_else(|error| panic!("导出的文本读不动：{error}\n{text}"));
            assert_eq!(读回来的, 原来的, "读回来与原来那份不一样：\n{text}");
        }

        // 街机的标题那条覆盖**整条**回来，不是插进通用那条里。
        let 改过的 = &各带一样怪处的表()[1];
        let 读回来的 = Priorities::parse(&改过的.to_text(), "（导出）").expect("导出的文本读得动");
        assert_eq!(
            读回来的.order("标题", Some("街机")),
            ["裁决", "MAME", "文件名"]
        );
        assert_eq!(读回来的.order("年份", Some("FC")), ["TOSEC"]);

        // **版本号照写**：读的那一侧照旧拿 `PRIORITIES_VERSION` 校验得住。
        let raw: RawPriorities = toml::from_str(&改过的.to_text()).expect("导出的是 TOML");
        assert_eq!(raw.version, PRIORITIES_VERSION);
    }

    #[test]
    fn 内置那份导出之后与内置原文说的是同一件事() {
        let 原文 = Priorities::parse(Priorities::builtin_text(), "（内置原文）").expect("读得动");
        let 导出的 = Priorities::parse(&Priorities::builtin().to_text(), "（导出）")
            .expect("导出的文本读得动");
        assert_eq!(导出的, 原文);
        for field in 原文.fields() {
            assert_eq!(
                导出的.order(field, None),
                原文.order(field, None),
                "{field}"
            );
        }
        assert_eq!(导出的.platform_overrides(), 原文.platform_overrides());
    }

    #[test]
    fn 固定的那几家不在开头时照样挪不动_别家也跨不过它们() {
        // 手写的表把 `Pegasus` 排在了一个数据源后面、`裁决` 夹在中间：这一层不替人重排，
        // 但**固定的那几家在哪儿都挪不动，别的源也换不到它们那一侧去**。
        let mut table = Priorities::parse(
            "\"版本\" = 1\n[[\"字段\"]]\n\"名\" = \"简介\"\n\
             \"顺序\" = [\"ScreenScraper\", \"Pegasus\", \"中文离线源\", \"裁决\", \"TOSEC\", \"文件名\"]\n",
            "（测试）",
        )
        .expect("手写的表读得动");
        let 原来的 = table.clone();
        // 固定的那两家本身：上移、下移都不动。
        for at in [1, 3] {
            assert!(
                !table.can_raise("简介", None, at),
                "第 {at} 家不该往前挪得动"
            );
            assert!(
                !table.can_lower("简介", None, at),
                "第 {at} 家不该往后挪得动"
            );
            assert!(!table.raise("简介", None, at));
            assert!(!table.lower("简介", None, at));
        }
        // 紧挨着固定那几家的数据源：跨不过去。
        assert!(
            !table.lower("简介", None, 0),
            "ScreenScraper 跨过了 Pegasus"
        );
        assert!(!table.raise("简介", None, 2), "中文离线源跨过了 Pegasus");
        assert!(!table.lower("简介", None, 2), "中文离线源跨过了裁决");
        assert!(!table.raise("简介", None, 4), "TOSEC 跨过了裁决");
        assert_eq!(table, 原来的);
        // 两边都不是固定的那几家：照常挪。
        assert!(table.can_lower("简介", None, 4));
        assert!(table.lower("简介", None, 4));
        assert_eq!(
            table.order("简介", None),
            [
                "ScreenScraper",
                "Pegasus",
                "中文离线源",
                "裁决",
                "文件名",
                "TOSEC"
            ]
        );
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
