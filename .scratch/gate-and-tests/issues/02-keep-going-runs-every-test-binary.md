# 02: `--keep-going` 时 test 步看全；check 编全部目标

**What to build:** - 门禁带 `--keep-going` 时，test 那一步加 `--no-fail-fast`，一趟看到所有红的测试二进制；默认仍是第一处红就停。`long-jobs.md` 里「手动另跑一遍补」那段删掉。
- check 那一步加 `--all-targets`，「编测试但不开 demo 特性」那一格从此有人编（翻 `Q208` 的「不补」）。check 仍排在 clippy 之前。

⚠️ 与 03 都改门禁那一个文件，串着做。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q510`：见 `../grill.md` 里那一条。——**已收**：`xtask/src/gate.rs` 的 `test` 只在 `keep_going` 时带 `--no-fail-fast`（排在 `--` 前面），`steps` / `step` 收这个开关，`--keep-going --list` 当场印出；`long-jobs.md` 那段手动补跑的说明改成按给没给 `--keep-going` 分两支。编译不过的测试目标不在其列（实测），另记挂单 `Q1196`。
收挂单 `Q1082`：见 `../grill.md` 里那一条。——**已收**：`check` 带 `--all-targets`（翻 `Q208` 的「不补」），仍排在 `clippy` 前；`fn check` 上那段「哪一条都没编」「要盖住就补」的注释、README「门禁必须带 `--all-features`」一节两句、`crates/gui/tests/snapshot.rs:98` 的「门禁一步都不跑这个组合」都改成了实话。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 先写红测试（门禁自己那份测试）：`--keep-going` 时 test 步的参数里有 `--no-fail-fast`（今天没有）
      —— `xtask/tests/gate.rs::给_keep_going_时_test_那一条不在第一个红的测试二进制上停`。先红两次：签名还不收开关时编译不过（E0061），把开关穿进 `steps` 之后断言红（`gate.rs:239` panicked）；加上开关后绿。同一份还加了 `看全那个开关递给_cargo_而不是递给测试二进制`：限流加看全时 `--no-fail-fast` 得排在 `--` 前面，先红（实参 `[..., "--", "--test-threads=2", "--no-fail-fast"]`）后绿；排到 `--` 后面时测试二进制报 `Unrecognized option: 'no-fail-fast'`，在丢弃 crate 上实测过。
- [x] 不带 `--keep-going` 时没有 `--no-fail-fast`
      —— `xtask/tests/gate.rs::不给_keep_going_时_test_那一条仍在第一个红的测试二进制上停`：最小实现先无条件加开关，这条红（`gate.rs:256`），改成只跟着 `keep_going` 走后绿。`每一条打印出来对得上_readme_那一段` 的默认那一行 `cargo test --workspace --all-features` 一字未改，同样钉着。
- [x] check 步的参数里有 `--all-targets`
      —— `xtask/tests/gate.rs::编测试而不开_demo_那一格也有一条在编`：带 `--all-targets` 又不带 `--all-features` 的那几条，先红（left `[]`，right `["check"]`）后绿；字面量基准那一行改成 `cargo check --workspace --all-targets`。次序由 `门禁就是这几条而且顺序是从便宜到贵` 钉着，未改。
- [x] `long-jobs.md` 那段手动补跑的说明已删
      —— 「另跑一趟 `cargo test --workspace --all-features --no-fail-fast`」那句删了；那段改成：给了 `--keep-going` 就看全、没给停在第一个红的；2026-09-13 那次实测挪到「本票之前给了也照样停」（那趟确实带着 `--keep-going`，审查 Standards 轴抓的），并写明编译红不在其列。起跑那节「`--keep-going` 让一趟看全」一句也补上 `test` 里的测试二进制。
- [x] 收尾时照 `docs/agents/long-jobs.md` 跑一趟真门禁，回执写进票里当证据；记下 check 步前后耗时
      —— 日志 `/Users/nicoer/dev/game-wt/logs/q6-gt02-gate1.log`（开头两行是 `/Users/nicoer/dev/game-wt/slot-1` 与 `q6/gt-02-keep-going-runs-every-test-binary`），`EXIT=0`，汇总表：
      `绿 fmt 2s` / `绿 glossary 0s` / `绿 check 11s cargo check --workspace --all-targets -j 3` / `绿 clippy 33s` / `绿 test 1838s cargo test --workspace --all-features --no-fail-fast -j 3 -- --test-threads=3` / `绿 numbers 8s` / `绿 doc 17s`，「7 条全绿」；86 个 `test result`，0 个 `FAILED`。test 那 1838 秒时别的 worktree 也在跑门禁（本趟跑完时 `ps` 里仍有 4 个匹配 `cargo xtask gate` 的进程，日志目录里 `q6-gt05-gate1.log` 同时在长），历史日志里 test 步 p50 536 秒、最大 1880 秒，不是本票的开关造成的（全绿时 `--no-fail-fast` 什么都不多跑）。
      check 前后耗时（**保留**：门禁内没有同条件的「前」，数是这么来的）：① 同一棵树、基点代码、依赖全热、四个工作区 crate 刚 touch 过，单跑 `-j 3`：`cargo check --workspace` 2.46 秒，`cargo check --workspace --all-targets` 4.10 秒；② 本趟门禁里带 `--all-targets` 的 check 11 秒（其中重查了 `ring` / `rustls` / `ureq`——我裸跑时它们是热的、门禁里的 cargo 又查了一遍，看着是票 03 要追的互顶缓存，没追实；不是 `--all-targets` 的账），历史日志 137 趟不带 `--all-targets` 的 check：p50 11 秒、p90 22 秒；同趟 clippy 33 秒。规格预测的「涨到接近 clippy 的量级」在本机半热缓存下没出现，冷缓存下没量。
      门禁之后只动了本票、挂单与 README 的票数：本票改成 `done` 让票数 150/205 → 151/205，`cargo xtask numbers --write` 写回后 `--check` 绿；这几样都不进编译，没有再跑第二趟。
