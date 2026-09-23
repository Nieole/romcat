# 08: 注释里的真库数换量级词、指台账

**What to build:** 代码注释、根 `Cargo.toml` 与内嵌 toml 里复述的真库数会过期：先改引错文件的那一处（改指台账 `docs/library-facts.md`）与根 `Cargo.toml`、两份内嵌 toml，换成量级词加「见台账」。
其余 `.rs` 注释不在这张票里统一改；把「写量级词、指向台账」写进约定，由下一张碰到那个文件的票顺手换。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q500`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] 引错文件那一处已改指台账
- [ ] 根 `Cargo.toml` 与两份内嵌 toml 里不再有具体真库数
- [ ] 约定写进 agent 会读到的那份文档（`CLAUDE.md` 指向的 `docs/agents/` 之一）
- [ ] `cargo xtask numbers --check` 照旧绿
