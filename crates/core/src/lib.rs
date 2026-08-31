//! 主库扫描与库体检的核心库。
//!
//! 核心是一个**独立的库**，命令行与将来的界面都只是它的壳（ADR-0005）：10T 规模的扫描
//! 迟早要挂后台或换台机器跑，只能靠点按钮驱动的程序不可接受。
//!
//! 入口是 [`scan::scan`]：对主库只读地遍历一遍，把结论写进**中立库**
//! （[`catalog::Catalog`]），再由中立库折出 [`report::HealthReport`]。
//!
//! 事实来源是中立库而不是某次扫描的内存状态（ADR-0001）：报告因此不必扫盘就出得来，
//! 而第二次之后的扫描按 `(路径, 大小, 修改时间)` 跳过未变的文件。

pub mod catalog;
pub mod classify;
pub mod container;
pub mod fs;
pub mod header;
pub mod path;
pub mod platform;
pub mod report;
pub mod scan;
pub mod shape;
pub mod testing;
pub mod workspace;
