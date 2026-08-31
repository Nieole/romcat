//! 主库扫描与库体检的核心库。
//!
//! 核心是一个**独立的库**，命令行与将来的界面都只是它的壳（ADR-0005）：10T 规模的扫描
//! 迟早要挂后台或换台机器跑，只能靠点按钮驱动的程序不可接受。
//!
//! 入口是 [`scan::scan`]，它对主库只读地遍历一遍，产出 [`report::HealthReport`]。

pub mod classify;
pub mod fs;
pub mod header;
pub mod path;
pub mod report;
pub mod scan;
pub mod testing;
