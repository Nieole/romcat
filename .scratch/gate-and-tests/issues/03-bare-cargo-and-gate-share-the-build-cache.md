# 03: 裸跑 cargo 与门禁不再互顶缓存

**What to build:** 本机保留 Homebrew 的 Rust（不装 rustup，钉版只管 CI，`long-jobs.md` 已写明）。追出裸跑 `cargo` 与门禁内层 `cargo` 编译指纹不同的那个环境变量（门禁经 `cargo run` 起，子进程继承了外层 cargo 设的变量），在门禁起子进程时清掉，并在注释里写明它是什么、为什么清。

⚠️ 与 02 都改门禁那一个文件，串着做。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q1168`：见 `../grill.md` 里那一条。——**已收**：按已裁的形状，Homebrew 的 Rust 留着，`rust-toolchain.toml` 与 CI 都没动。追出来的是六个变量，不止一个（见下）。`xtask/src/gate.rs` 的 `Step::command_in` 起每条子进程前把它们 `env_remove` 掉，别的环境照旧继承。清单与理由写在常量 `CARGO_RUN_VARS_THAT_FLIP_FINGERPRINTS` 的文档上：它们是什么、为什么让指纹不同、怎么追出来的、往后怎么再追。`numbers` 数测试条数那一趟也走 `command_in`，一并清掉。

**Blocked by:** 02

**Status:** done

## 追出来的是哪几个变量、怎么确认的

- **根因**：`cargo xtask` 是一趟 `cargo run`，它往 xtask 进程里塞了一份描述 xtask 这个包的变量，门禁起的内层 cargo 原样继承。`ring` 的构建脚本对其中六个声明了 `cargo:rerun-if-env-changed`，分别是 `CARGO_MANIFEST_DIR`、`CARGO_PKG_NAME`、`CARGO_PKG_VERSION_MAJOR`、`_MINOR`、`_PATCH`、`_PRE`。cargo 记这种指纹时，读的是**它自己进程里**的值。裸跑时进程里没有这几个，门禁里有，值是 xtask 的。两边一换手，`ring` 的构建脚本就判脏，重跑 C 与汇编，连带 `ring`、`rustls`、`rustls-webpki`、`ureq`、`romcat-core`、`romcat-gui`、`romcat-cli` 七个 crate 重编。
- **怎么确认的**：
  1. **指纹日志**：裸跑时带 `CARGO_LOG=cargo::core::compiler::fingerprint=info`。除了「依赖变了」与源码 mtime，唯一的脏因是 `ring` 构建脚本上的 `EnvVarChanged { name: "CARGO_MANIFEST_DIR", old_value: Some("/Users/nicoer/dev/game-wt/slot-1/xtask"), new_value: None }`。反方向，也就是门禁在裸跑之后跑时，是 `old_value: None, new_value: Some(".../xtask")`。`old_value` 是 xtask 的目录而不是 `ring` 自己的，说明 cargo 比对的是它自己进程里的值。
  2. **交集**：cargo 每个单元只报第一处不同，所以另外几个名字靠两张表对出来。一张是 `cargo run` 实际塞进来的变量：在 `env -i` 下拿丢弃 crate 比对，共 17 个，包括 `CARGO`、`CARGO_MANIFEST_DIR`、`CARGO_MANIFEST_PATH`、13 个 `CARGO_PKG_*` 和 `DYLD_FALLBACK_LIBRARY_PATH`。另一张是本机 `target/debug/build/*/output` 里全部的 `rerun-if-env-changed`。整个依赖图里只有 `ring` 盯着前一张表里的变量，交集正好是上面六个。
  3. **改后为证**：清掉这六个之后，同样顺序再跑一遍，一个 `EnvVarChanged` 都不再出现（见第 4 条回执）。

- [x] 先演示今天的错：裸跑一次 `cargo test` → 跑门禁 → 再裸跑，记下两次都重编的回执
      —— 「跑门禁」这一环用 `cargo xtask numbers --check` 代替整趟门禁。它与门禁同路：`cargo run` 起 xtask，xtask 再经 `Step::command_in` 起内层 `cargo test --workspace --all-features -- --list`。这正是门禁 `test` 那一条的参数，继承的也是同一份环境。裸跑那一环敲的是同一行命令。三趟都带同样的 `TMPDIR=/Users/nicoer/dev/game-wt/cs/tmp CARGO_BUILD_JOBS=3 CARGO_LOG=cargo::core::compiler::fingerprint=info`，两边环境一致。第一趟之后 `xtask/src/gate.rs` 临时换回 `72d5139` 那份，跑完再放回新版，`shasum` 前后都是 `f17388b1…`，逐字一致。日志都在 `/Users/nicoer/dev/game-wt/logs/`：
      ① 裸跑，接在票 02 那趟门禁之后（`q6-gt03-demo1-bare.log`，`EXIT=0`）：重编 `ring`、`rustls`、`rustls-webpki`、`ureq`、三个 `romcat`，外加 `xtask`（它的源码随切分支变过），`-j 3` 用了 2m49s。`ring` 构建脚本报 `EnvVarChanged CARGO_MANIFEST_DIR Some(.../xtask) → None`。
      ② 门禁同路（`q6-gt03-demo2-gate-old.log`）：重编同样七个，外加 `xtask`（源码换回了旧版），1m03s。脏因是 `EnvVarChanged CARGO_MANIFEST_DIR None → Some(.../xtask)`。这一趟 `EXIT=1`，因为新加的测试让测试条数 2,628 → 2,629，与编译无关。
      ③ 再裸跑（`q6-gt03-demo3-bare-old.log`，`EXIT=0`）：重编同样七个，1m14s。脏因是 `EnvVarChanged CARGO_MANIFEST_DIR Some(.../xtask) → None`。
- [x] 票里写明追出来的是哪个变量、怎么确认的
      —— 见上面「追出来的是哪几个变量、怎么确认的」一节。是六个，不是一个。实测只拿到 `CARGO_MANIFEST_DIR` 这一个名字，另外五个是对交集推出来的，由第 4 条回执证实。
- [x] 门禁自己那份测试断言子进程不带那个变量
      —— `xtask/tests/gate.rs::门禁起的每一条都不带_cargo_run_塞给_xtask_的那几个包变量`。它对两档 `keep_going` 下的七条逐条断言：`command_in` 折出的命令里，显式拿掉的环境变量恰好是那六个，名字是手写的独立基准。另外断言 `doc` 那一条的 `RUSTDOCFLAGS="-D warnings"` 照旧递下去。先红：`fmt` 那一条 left `[]`、right 六个名字。在 `command_in` 里加上 `env_remove` 之后转绿。只断言命令，不在测试里真跑门禁。`cargo test -p xtask --test gate` 19 条全绿。
- [x] 改完后同样顺序跑，第二次裸跑不再整份重编（回执为证）
      —— 新版 `gate.rs`，同样的环境与命令：
      ④ 裸跑（`q6-gt03-demo4-bare-new.log`，`EXIT=0`）：只重编 `xtask`，因为源码刚放回新版，1.77s。
      ⑤ 门禁同路（`q6-gt03-demo5-gate-new.log`）：内层 `Finished … in 0.80s`，0 个 `Compiling`，0 个 `EnvVarChanged`。唯一的脏因是外层 `cargo run` 重编 xtask 的 bin（源码变了）。这一趟 `EXIT=1`，原因同②，是测试条数。
      ⑥ 再裸跑（`q6-gt03-demo6-bare-new.log`，`EXIT=0`）：`Finished … in 0.45s`，指纹日志里 0 行 dirty。
      改前每换一次手要白付七个 crate、一分多钟；改后是零。
- [x] 收尾时照 `docs/agents/long-jobs.md` 跑一趟真门禁，回执写进票里当证据
      —— 日志 `/Users/nicoer/dev/game-wt/logs/q6-gt03-gate1.log`，开头两行是 `/Users/nicoer/dev/game-wt/slot-1` 与 `q6/gt-03-bare-cargo-and-gate-share-the-build-cache`，最后一行 `EXIT=0`。汇总表：
      `绿 fmt 4s` / `绿 glossary 0s` / `绿 check 49s cargo check --workspace --all-targets -j 3` / `绿 clippy 38s` / `绿 test 1788s cargo test --workspace --all-features --no-fail-fast -j 3 -- --test-threads=3` / `绿 numbers 5s` / `绿 doc 15s`，「7 条全绿」。共 86 个 `test result`，0 个 `FAILED`。跑这一趟时 slot-3（票 04）的收尾门禁也在并行跑。门禁的 `test` 那一步 `Finished … in 0.54s`，没有重编，因为它接在⑥之后。
      `check` 那一步重查了 `ring` 一串，这是演示的账，不是互顶。演示里①–③让 `ring` 的构建脚本来回重跑了三次，而那份构建脚本产出由 dev 与 test 两档共用（`target/debug/build/` 下只有一个 `ring-*`）。上一次 dev 档的 check 还是票 02 的门禁，自然要补查一遍。门禁跑完之后，我在终端里裸跑同参数的四条（`q6-gt03-after-gate-bare.log`，`EXIT=0`）：`cargo check --workspace --all-targets`、`cargo clippy --workspace --all-targets --all-features`、`cargo test --workspace --all-features -- --list`、`RUSTDOCFLAGS="-D warnings" cargo doc …`，各 0.2–0.4s，指纹日志 0 行 dirty。

门禁之后只动了本票（勾选与 `done`）。改成 `done` 之后 README 的票数会差一，合并时由编排者在 `main` 上重写。测试条数 2,628 → 2,629 已用 `cargo xtask numbers --write` 写回 README，门禁的 `numbers` 那一步是在写回之后跑的。
