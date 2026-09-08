//! **门禁跑手。** `README.md`「开发」一节那四条命令的唯一定义（[`gate`]），
//! 本地 `cargo xtask gate` 与 GitHub Actions 都只调它。
//!
//! 这个 crate **不是交付出去的东西**：`romcat` 与 `romcat-gui` 都不依赖它，
//! 它一个字节都不进那两个二进制。

pub mod gate;
