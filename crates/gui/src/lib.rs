//! romcat 的界面壳。**打开它看见的是待确认队列**（ADR-0002）。
//!
//! 核心是一个**独立的库**，界面只是它的壳（ADR-0005）：**领域判断一条都不在这里**——排序、
//! 筛选、分页、队列怎么分批、容量怎么写成人话、置信度那一档叫什么词，全由 [`romcat_core`]
//! 答。这一层只问它要哪一段、把要来的画出来；换掉这个 crate 的代价因此被限制在这一层。
//!
//! **这一层的边界、七屏各自的职责、ADR-0005 那三个风险（字体、十万行表格、输入法）怎么清的，
//! 记在 `docs/spec/gui.md`。** 改这一层之前先读那一份：这里的模块文档讲各自怎么画，
//! 那一份讲为什么这样分、以及哪些东西不许长到这里来。
//!
//! 合成数据在 `demo` feature 之后，**默认不编进来**——交付出去的客户端里不该有一个字节的
//! 假数据。它走的是核心库的公开 API，不是旁路。

pub mod app;
pub mod bench;
pub mod browse;
pub mod claim;
pub mod clock;
/// 合成数据。**只在 `demo` feature 下编进来**——交付的客户端里不该有假数据。
#[cfg(feature = "demo")]
pub mod demo;
pub mod dialog;
pub mod filter;
pub mod font;
pub mod headless;
pub mod health;
pub mod keys;
pub mod layout;
pub mod look;
pub mod media;
pub mod opening;
pub mod pick;
pub mod priority;
pub mod program;
pub mod queue;
pub mod rail;
pub mod recent;
pub mod roots;
pub mod scrape;
pub mod settings;
pub mod shaping;
pub mod site;
pub mod stages;
pub mod sublibrary;
pub mod table;
pub mod task;
pub mod toast;
pub mod tokens;
