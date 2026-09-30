# 08: 注释里的真库数换量级词、指台账

**What to build:** 代码注释、根 `Cargo.toml` 与内嵌 toml 里复述的真库数会过期：先改引错文件的那一处（改指台账 `docs/library-facts.md`）与根 `Cargo.toml`、两份内嵌 toml，换成量级词加「见台账」。
其余 `.rs` 注释不在这张票里统一改；把「写量级词、指向台账」写进约定，由下一张碰到那个文件的票顺手换。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q500`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 引错文件那一处已改指台账
  - 证据：`crates/core/src/filename.rs:146`–`:147`，原来为「十万次量级」引的是 `docs/scale-reference.md`（那份不描述用户的库），
    改成「变体数 × 两个中文源 + 作品数；真库的变体四万多、作品九千多，见台账 `docs/library-facts.md`」——台账「锚点：作品 9,226 个、变体 46,444 个」那一行对得上，
    四万多 × 2 + 九千多 ≈ 十万，论证照旧成立。`git grep -n 'scale-reference' -- crates/core/src/filename.rs` 改前 1 行，改后 0 行。
- [x] 根 `Cargo.toml` 与两份内嵌 toml 里不再有具体真库数
  - 证据（「红」命令）：票面那条 `git grep -nE '46,444|…|16,420'` 收窄到 `Cargo.toml crates/core/src/platform/platforms.toml crates/core/src/scrape/priorities.toml`，
    改前 12 行、改后 0 行（rc=1）；再加上票面列表漏掉、但同在这三份文件里的 `2,685|98 png|2\.72 TiB|792 个|78\.5%|67%`，改前 14 行、改后 0 行。
    全仓同一条 grep（`-- crates xtask Cargo.toml`）从 60 个文件、135 行降到 57 个文件、123 行，其余照裁定不改。
  - 换法：量级词保住原句论证（「两千多个文件、两 TiB 多」「两万多条」「三分之二、十几万个」「四万多个变体里有三成多 DAT 认不出来」「近三 TiB」），各指一句台账。
    `platforms.toml` 里 `pc` 的「理由」是会进体检报告的字符串（`report.rs` 的 `excluded_reason`），不进清单指纹，也没有测试断言它的原文。
  - 三份 toml 都还解析得动：Python `tomllib` 逐份读通；`cargo test -p romcat-core --lib -j 2 -- --test-threads 2 platform:: scrape::priority`
    35 passed、0 failed，`EXIT=0`（日志 `/Users/nicoer/dev/game-wt/logs/q6-gt08-core-lib-toml.log`）；`cargo check -p romcat-core -j 2` `EXIT=0`。
  - **保留**：有两组数台账没收，没硬指台账，写的是它们真正的出处——`Cargo.toml` 的 jpg/png 份数指票 `gui-redesign/07`（挂单 `Q1256`），
    `priorities.toml` 里中文离线源的四个覆盖率是外部数据的数、指 `.scratch/offline-chinese-fields/spec.md`（挂单 `Q1257`）。`Cargo.toml:115` 的「10 TB 盘」是整数量级（`CLAUDE.md` 同一说法），没动。
- [x] 约定写进 agent 会读到的那份文档（`CLAUDE.md` 指向的 `docs/agents/` 之一）
  - 证据：`docs/agents/issue-tracker.md`「收尾一张票」一节，接在票 07 那段（令牌核对）之后另起一段「**真库数只住台账**」：写量级词再指台账、量级要保住论证、
    台账没收的数写真出处并记挂单、ADR / 调研 / `.scratch/` 不换、其余文件由下一张碰到它的票收尾时顺手换，附一条只看本票改动的粗筛命令（本树上试跑过：
    对本票改动 0 命中，对 `43ef95d~5..43ef95d` 那一段改动命中 `crates/cli/src/main.rs` 等处，空输入不跑）。
    选这里：每张票收尾都照这一节勾票（`CLAUDE.md`「工单」直指它），而「顺手换」的义务恰好落在收尾那一步；票 07 的令牌核对是同一类收尾检查。
  - **保留**：粗筛命令里那串数原样写进了 `docs/agents/`，与「当前文档里真库数只在台账」的口径相抵，挂单 `Q1258`。
- [x] `cargo xtask numbers --check` 照旧绿
  - 证据：`CARGO_BUILD_JOBS=2 cargo xtask numbers --check`（翻 `Status` 之前跑），`EXIT=0`：
    「数：测试条数=2,624 测试目标数=86 票数=151/205 ADR份数=24；1 份文件里 4 处标记。」（日志 `/Users/nicoer/dev/game-wt/logs/q6-gt08-numbers.log`）。
    翻 `Status` 之后票数会差一，照编排约定由合并时在 `main` 上重写。
- [x] 收挂单 `Q500`
  - 怎么收的：照 `../grill.md` 的裁定做了前一半（`filename.rs:147` 与三份非 `.rs` 文件，见上面两条）与约定（见第三条）；其余五十多个文件照裁定不在这张票里改。
    `Q500` 在原票 `machine-checks-premises/04` 里第五轮收口时已标 `settled`，状态不用再动。本票新记挂单 `Q1256`–`Q1258`。
