# 03: 裸跑 cargo 与门禁不再互顶缓存

**What to build:** 本机保留 Homebrew 的 Rust（不装 rustup，钉版只管 CI，`long-jobs.md` 已写明）。追出裸跑 `cargo` 与门禁内层 `cargo` 编译指纹不同的那个环境变量（门禁经 `cargo run` 起，子进程继承了外层 cargo 设的变量），在门禁起子进程时清掉，并在注释里写明它是什么、为什么清。

⚠️ 与 02 都改门禁那一个文件，串着做。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q1168`：见 `../grill.md` 里那一条。

**Blocked by:** 02

**Status:** ready-for-agent

- [ ] 先演示今天的错：裸跑一次 `cargo test` → 跑门禁 → 再裸跑，记下两次都重编的回执
- [ ] 票里写明追出来的是哪个变量、怎么确认的
- [ ] 门禁自己那份测试断言子进程不带那个变量
- [ ] 改完后同样顺序跑，第二次裸跑不再整份重编（回执为证）
- [ ] 收尾时照 `docs/agents/long-jobs.md` 跑一趟真门禁，回执写进票里当证据
