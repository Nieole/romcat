# 门禁说到做到，测试不赌挂钟

**Status:** 待 `/to-spec`
**Opened:** 2026-09-23（`/settle` 第五轮收口 → `/grill-with-docs`）
**来源：** `.scratch/PARKING-LOT.md` 第五轮收口里无家可归的那摊活；条目正文在各原票末尾「挂单裁决（第五轮收口）」一节

## 它建什么

又是「靠人念一遍」那条防线漏掉的地方：门禁有几格没盖住却报绿（第一组）；测试靠挂钟与大盘赛跑（第二组）；令牌与真库数这两样事实没有机器替人核（第三组）。

## 15 件（形状已裁定的直接照写）

## 第一组：门禁的开关与本机环境

### `Q510` — 门禁的 test 那一步不带 `--no-fail-fast`：第一个红的测试二进制后面的都不跑

**已裁定（2026-09-23 grill）：** 门禁 test 那一步只在 `--keep-going` 时加 `--no-fail-fast`；默认仍是第一处红就停。
核查（2026-09-23）：xtask/src/gate.rs:259-272（fn test）只拼 ["test","--workspace","--all-features"]，无 --no-fail-fast；.github/workflows/gate.yml:90 跑 `cargo xtask gate --keep-going`，即 CI 上步间 keep-going、test 步内仍停在第一个红二进制；docs/agents/long-jobs.md:125-127 目前让人手动另跑 `--no-fail-fast` 补。
依赖／可并：与 Q1082 同改 xtask/src/gate.rs + xtask/tests/gate.rs，放同一张票或串行
（大小 S，来自票 `第五轮队列编排`）

### `Q1082` — 门禁的盲区：「编测试但不开 demo」这个组合一步都不跑

**已裁定（2026-09-23 grill）：** `check` 那一步加 `--all-targets`（翻 `Q208` 第三轮的「不补」）。
核查（2026-09-23）：xtask/src/gate.rs:229-237 check 仍是 cargo check --workspace（无 --all-targets）；:223-228 注释自认「默认特性下的测试目标门禁里哪一条都没编」。上游 Q208（parking-3/02 末尾，第三轮裁 A）当时的理由是「漏的那一格今天没有触发者」——Q1081 已经触发过一次（tests/snapshot.rs 的 use 带 demo 门，e002756 修），前提不再成立。
依赖／可并：与 Q510 同改 gate.rs，串行或同票
（大小 S，来自票 `gui-looks-like-the-design/31`）

### `Q543` — CI 上推到 `main` 的那一趟一行都不扫：在本地合进 `main` 再推上去的改动，CI 上没有这道机器

**已裁定（2026-09-23 grill）：** 词表扫描加一个「跟哪个提交比」的开关，`gate.yml` 在 push 事件上递 `github.event.before`（全零时照旧跳过）。
核查（2026-09-23）：xtask/src/glossary.rs:13、:24、:255-293 只比「相对 main 的 merge base」，on_base 时只看未提交的，无任何覆盖口（grep 无 env::var）；.github/workflows/gate.yml 的 push 只在 main 上、未递 github.event.before；long-jobs.md:133 明写「推到 main 的那一趟一行都不扫」。第五轮实际流程是本地 merge 进 main 再推、不开 PR，所以这道机器在 CI 上从未响过；编排者靠记忆里的「merge --no-ff --no-commit 再跑 glossary」手工补。
依赖／可并：可并
（大小 S，来自票 `machine-checks-premises/02`）

### `Q479` — 六条真盘测试默认临时目录分大小写，默认 APFS 的本机上门禁在 main 上就红

**已裁定（2026-09-23 grill）：** 不分大小写的盘上如实跳过分大小写那一遍并印「跳过：…」（照 `target.rs` 已有的做法）；CI 的 ext4 上照跑全量。`Q947` 随之作废。
核查（2026-09-23）：crates/core/tests/sync_run.rs:844/899/934/1235/1263 那五条仍未按盘的大小写跳过（1235 只差大小写那条照旧无条件断言）；crates/core/src/testing/target.rs:243-245 已是「先问 fs::case_insensitive、摆不出就印『跳过：…』」（06712c0）。docs/agents/long-jobs.md:138-139 明写「怎么处置还没裁，见 Q479」。本机实况：主力机仍靠稀疏盘 TMPDIR=/Users/nicoer/dev/game-wt/cs/tmp 才全绿。
依赖／可并：与 Q947 连动（选 ① 后 Q947 自然 gone）、可与 Q510 并
（大小 S，来自票 `第五轮队列编排`）

### `Q947` — 分大小写那块盘没挂上时，编译在 `ring` / `zstd-sys` 处退出 1，报的却是 clang 的错

**作废：** `Q479` 定了在不分大小写的盘上如实跳过，不再需要门禁起手核 TMPDIR。
核查（2026-09-23）：xtask/src 与 docs/agents/*.md 里都没有 TMPDIR / 挂载点 / clang 报错形状的任何检查或说明；约定只活在用户记忆 worktree-build-isolation-lessons（TMPDIR=/Users/nicoer/dev/game-wt/cs/tmp，重启后要重新 hdiutil attach）。
依赖／可并：Q479：选 ① 则不再需要稀疏盘 TMPDIR，本条随之 gone；选 ②/③ 则在 long-jobs.md 记一句「TMPDIR 指的盘没挂上时报成 clang: unable to make temporary file」，或 xtask gate 起手核 TMPDIR 存在
（大小 S，来自票 `gui-looks-like-the-design/21`）

### `Q1168` — 裸跑 `cargo test` 与 `cargo xtask` 里那趟**互相把缓存顶掉**：每次约 100 秒、7 个 crate 重编

**已裁定（2026-09-23 grill）：** **保留 Homebrew 的 Rust**，只追裸跑 cargo 与门禁互顶缓存的那个变量，在 xtask 里清掉；`rust-toolchain.toml` 的钉版留着管 CI，`long-jobs.md` 写明「本机是 Homebrew、不走钉版」。
核查（2026-09-23）：根因未追。新证据（只读查得）：本机 `which -a cargo rustc` 只有 /opt/homebrew/bin，`cargo 1.98.0 (Homebrew)`、`rustc 1.98.0 (Homebrew)`，rustup 不存在、~/.rustup/toolchains 不存在——而 rust-toolchain.toml 钉的是 1.98.1、CI 装的也是 1.98.1。即本机从来没按钉的工具链跑过门禁，fmt/clippy 与 CI 不是同一版。缓存互顶：.cargo/config.toml 的 xtask 别名走 cargo run，子进程继承 cargo run 注入的环境（CARGO/CARGO_PKG_*/DYLD_FALLBACK_LIBRARY_PATH 等），嫌疑变量仍待逐个 diff。
依赖／可并：影响所有 worktree 的 target 热度，最好在没有派活跑门禁的窗口做、与 Q947/Q479 同属「本机环境」
（大小 M，来自票 `machine-checks-premises/03`）

### `Q948` — ego-browser 窗口停在 maximized 时截不出图，CDP 一直超时

**纯活，不动仓库：** 把 ego-browser 对稿的约定（窗口 normal、钉视口 1280×800）写进编排者的记忆。
核查（2026-09-23）：仓库内无 ego-browser 截图约定（docs/ 下无 maximized/setWindowBounds/1280 命中）；「看网页一律 ego-browser」只在用户记忆 browser-use-ego-browser.md，里头没写 windowState 与视口两步。
依赖／可并：不进代码，改记忆（或对稿派单模板）一段：先 Browser.setWindowBounds 成 normal，再 Emulation.setDeviceMetricsOverride 1280×800，然后截；可并
（大小 S，来自票 `gui-looks-like-the-design/21`）

## 第二组：测试等信号不等挂钟

### `Q833` — 「走完向导之后扫描……按得下停下」靠扫描比进主窗口头两帧慢才验得到：盘从三千个文件加到三万个

**已裁定（2026-09-23 grill）：** 给扫描加一道测试用的闸（走到第 N 个条目停住等信号，与「占位活」同一规矩），这条测试彻底不等时间；`Q1146` 跟着收。
核查（2026-09-23）：crates/gui/tests/program.rs:1143-1157 摆一块大盘 仍是 30 组×1000 个 zip（三万个文件）；:1172 走完向导之后扫描…按得下停下 仍在赌「扫描比进主窗口头几帧慢」。Q864（同一条测试，票 32）已并进；Q1146（机器忙时飘红）也是这一条。新情况：crates/gui/src/program.rs:222 Program::app_mut 已存在（票面说的「Program 对外没有摸得到任务台的入口」不再成立），但扫描的 RealFs 仍在 roots.rs:563 Screen::scan 里写死，没有等信号的闸。
依赖／可并：吸收 Q864、Q1146、与 Q741 同束
（大小 M，来自票 `gui-looks-like-the-design/25`）

### `Q1146` — `tests/program.rs` 那条走完向导的测试在机器忙的时候会飘红

**上游已答，只差动手：** 随 Q833 裁：是同一条测试同一个病（等挂钟），第五轮收口已把 Q864 并进 Q833，这条同理并入。
核查（2026-09-23）：同 Q833：crates/gui/tests/program.rs:1172 那一条；票 14 全量并排跑时红一次，单跑与整份 --test program 都绿。
依赖／可并：Q833
（大小 S，来自票 `gui-looks-like-the-design/14`）

### `Q741` — 两处等按停的占位活（`占住任务台`、`排一趟停在原地的`）没并进共享的 `占位活`

**纯活：** roots.rs 占住任务台与 task.rs 排一趟停在原地的并进共享 占位活。
核查（2026-09-23）：crates/gui/tests/roots.rs:233-240 现场::占住任务台 仍是 loop{check; sleep(1ms)}；crates/gui/tests/task.rs:86-95 排一趟停在原地的 仍是 loop{check; sleep(2ms)}；共享的 占位活 在 crates/gui/tests/shared/mod.rs:475-538，已被 layout/health/sublibrary/media/snapshot 等用上。两函数引用合计 13 处。
依赖／可并：只动 crates/gui/tests/roots.rs、task.rs，可并；与 Q833 同属「测试等信号不等挂钟」一束
（大小 S，来自票 `machine-checks-premises/07`）

### `Q556` — 四档收场里「部分完成」那一档，没有从工序段那个入口走的测试

**已裁定（2026-09-23 grill）：** 不补——工序段那一支没另写一行，入口到收场那一跳已被另三档盖住。
核查（2026-09-23）：crates/gui/tests/roots.rs:2294/2324/2377/2424 只有完成、排队中按钮、已取消、失败四条，「部分完成」无从 Section::start 入口的测试；crates/gui/src/scrape.rs:824-880 run 里 RealFs 与 HttpFetcher 写死，唯一插得进去的是 progress 闭包（:870 task.tick）；核心库的同类钉子在 crates/core/tests/stage.rs:1345 写出第一份就按停（适配器接缝）。
依赖／可并：无
（大小 S，来自票 `gui-answers-all-six/04`）

## 第三组：令牌与真库数

### `Q486` — 「界面里没有第二处写死的颜色」没有测试守着

**已裁定（2026-09-23 grill）：** 加一条测试扫界面源码（剔除测试模块与 `tokens.rs`），见颜色字面量就红，只放行透明与占位色；先把现存 3 处写死的白色换成令牌。
核查（2026-09-23）：无任何测试扫 crates/gui/src 的 Color32 字面量。且已经回退：crates/gui/src/health.rs:1309、:1321 平台标用 egui::Color32::WHITE（票 28，7580126）；crates/gui/src/look.rs:1071 开关圆点 Color32::WHITE（64541b9）——tokens.toml:36/:65 已有 on-accent（亮 #FFFFFF / 暗 #0D1030），暗色主题下写死白与令牌口径不一。另有合法的 Color32::TRANSPARENT（look.rs:458 等）与 Color32::PLACEHOLDER（queue.rs 多处，layout 用）必须放行。
依赖／可并：换 WHITE 为令牌会动基线截图（health、settings 开关），与 GUI 基线票串行、可与 Q963/Q1083 同束（令牌单一来源）
（大小 S，来自票 `gui-looks-like-the-design/01`）

### `Q963` — 待确认屏「细分」那一栏的令牌只有本票新加的三个进了 `check_tokens.py`，票 18 那一批没进

**已裁定（2026-09-23 grill）：** 补齐漏核的存量；脚本加完整性检查：每个令牌要么被核、要么列进显式豁免表，否则退 1。脚本仍手动跑，设计稿不进门禁。
核查（2026-09-23）：.scratch/gui-looks-like-the-design/check_tokens.py 里 dist-row-gap、batch-gap、batch-head-padding、match-row-gap、kbd-padding 等票 18 的令牌 0 处命中（crates/gui/src/tokens.toml 里各 1 处）；票 19 的 dist-bar/drill-tint 在。脚本不在门禁里（xtask/.github/docs 均无 check_tokens 引用），也没有「tokens.toml 每个键都得被核或显式豁免」的完整性检查——颜色走 :root 循环自动全核，布局/字号类令牌靠逐条 literal_pairs。
依赖／可并：吸收 Q1083、与 Q486 同束（令牌单一来源）
（大小 M，来自票 `gui-looks-like-the-design/19`）

### `Q1083` — 票 24 那三个令牌没进设计稿核对脚本

**纯活：** 给 size-diff-value/diff-tile-padding/diff-gap 补核对，并把「新立令牌须进脚本」写进收尾清单。
核查（2026-09-23）：crates/gui/src/tokens.toml 有 size-diff-value / diff-tile-padding / diff-gap，.scratch/gui-looks-like-the-design/check_tokens.py 0 处命中。
依赖／可并：Q963（同一件事的另三个令牌，随它的裁决补）
（大小 S，来自票 `gui-looks-like-the-design/31`）

### `Q500` — 代码、根 `Cargo.toml` 与编进二进制的 toml 里还复述着真库数，没改

**已裁定（2026-09-23 grill）：** 先改引错文件的 `filename.rs:147` 与三份非 `.rs` 文件；其余由下一张碰到那个文件的票顺手换成量级词，并把「写量级词、指向台账」写进约定。
核查（2026-09-23）：按票面那条 git grep 复查：现在 60 个文件、135 行（票面记 55/128，还在长）。根 Cargo.toml:58-59（2,685 个文件、2.50 TiB）、:73（21,901）、:87/:93（342）仍在；crates/core/src/filename.rs:147 附近仍引 docs/scale-reference.md（那份不描述用户的库）。
依赖／可并：避开在飞 GUI 票的文件；与 Q963 无关，可并
（大小 M，来自票 `machine-checks-premises/04`）
