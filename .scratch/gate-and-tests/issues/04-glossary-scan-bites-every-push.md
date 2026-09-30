# 04: 词表扫描咬得住每一次 push

**What to build:** 词表扫描加一个「跟哪个提交比」的覆盖口（环境变量）：给了就扫它之后的改动，没给照旧比 `main` 的 merge base。CI 工作流在 push 事件上把 `github.event.before` 递进去；它是全零（新分支、强推）时照旧如实跳过。
本地合并再推的改动从此在 CI 上也被扫；本地合并前也能手动指一个提交自己验。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q543`：见 `../grill.md` 里那一条。——**已收**：覆盖口是环境变量 `ROMCAT_GLOSSARY_SINCE`（`xtask/src/glossary.rs` 的 `SINCE_VAR`，前缀照 `ROMCAT_HOME`），`cargo xtask glossary` 读它、原样递进 `scan(dir, since)`；给了一个提交就扫它与 `HEAD` 的 merge base 以来的改动（含未提交的），空值当没给，全零、求不到都如实跳过。`.github/workflows/gate.yml` 跑门禁那一步只在 push 上递 `github.event.before`，`xtask/src/gate.rs` 没动（子进程照旧继承）。`glossary.rs` 模块文档、`gate.yml` 两段注释、`long-jobs.md`「读回执」、词表 `CONTEXT.md` 那句括号（挂单 `Q1217`）都改成了实话。前一趟被 `concurrency` 掐掉时那一次推上去的提交仍没人扫，记挂单 `Q1216`。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 先写红测试（词表扫描已有的测试）：站在 `main` 上、给一个覆盖提交 → 扫的是那之后新写的代码（今天只看未提交的）
      —— `xtask/tests/gate.rs::词表那一条给了覆盖提交就扫它之后提交上去的改动`：丢弃仓库站在 `main` 上、撞词那一处已提交、工作区干净，不给覆盖口时绿（照旧），给上一版时必须红并点名 `crates/a/src/lib.rs:2`「近义一」、不报 `old.rs` 那处存量。
      在基点代码上先红（`gate.rs:642` panicked，输出「范围：merge base 就是 `HEAD`（59bb094）……扫了 0 份 0 行……没有新写的代码可扫」），加上覆盖口后绿。
      同一刀带出「求不到」那条路：`词表那一条的覆盖口指向求不到的提交时如实跳过而不报假红`（真造强推的形状：源仓库 reset 掉一个提交再提交一个撞词的，`file://` 全量克隆里没有被盖掉的那一版）一写就绿——那条 let-else 是第一刀的最小实现不得不写的；把它改成报错做变异实测，这条红。
- [x] 覆盖口为全零 → 如实跳过、退 0、说清为什么
      —— `xtask/tests/gate.rs::词表那一条的覆盖口是全零时如实跳过并说清为什么`：先红（印的是「跳过：……在这份仓库里求不到」，没说全零），加上全零判断后绿；断言退 0、印「跳过：」「全零」「一行代码都没扫」，工作区里那处撞词不报。
      本树上实跑：`ROMCAT_GLOSSARY_SINCE=0000000000000000000000000000000000000000 cargo xtask glossary` 印「跳过：`ROMCAT_GLOSSARY_SINCE` 给的是全零（`000…`）：没有上一版可比——GitHub 推一条新分支时递的 `github.event.before` 就是它。」与「这一趟一行代码都没扫——不是扫过了没撞上。」
- [x] 没给覆盖口 → 行为与今天一样
      —— 默认路径的判断顺序与印出来的每一句（浅克隆、找不到 `main`、求不出 merge base 三种跳过，两种「范围：」）一字未改；已有的五条词表测试只把 `scan(dir)` 改成 `scan(dir, None)`、命令行夹具不给时摘掉这个变量，照旧绿。
      新加 `词表那一条的覆盖口是空的就与没给一样`（CI 的 pull_request 那一趟递的就是空串）：空值与不给印出来一字不差；去掉空值过滤做变异实测，这条红（空串被当成全零跳过）。
      推到 `main` 那一趟整个门禁都带着这个变量、测试进程也继承——命令行夹具不摘掉它时有 2 条词表测试变红（变异实测），现在摘掉了；带着 `ROMCAT_GLOSSARY_SINCE=<真提交>` 跑那 11 条词表测试全绿。
- [x] CI 工作流在 push 事件上递了上一版（看工作流文件）
      —— `.github/workflows/gate.yml` 跑门禁那一步：`env: ROMCAT_GLOSSARY_SINCE: ${{ github.event_name == 'push' && github.event.before || '' }}`（push 时是上一版、pull_request 时是空串——`pull_request` 在 `synchronize` 时也带 `before`，递进去就只扫最后一次推的那几个提交）。
      钉子：`xtask/tests/gate.rs::ci_在推到_main_的那一趟把上一版递给词表那一条`，先红后绿，钉在「跑门禁」那一步里（把这一行挪到 checkout 那一步做变异实测，它红）。**保留**：没在 GitHub Actions 上真跑过（本票不 push），合进 `main` 后头一次推就是头一次真跑；它掐不掐前一趟见挂单 `Q1216`。
- [x] `long-jobs.md` 那句「推到 main 的那一趟一行都不扫」改成实话
      —— 「读回执」那段改成：推到 `main` 的那一趟把推之前那一版递进 `ROMCAT_GLOSSARY_SINCE`，扫这一次推上去的全部提交，**事后**报红；前一趟被掐掉时那一次推上去的没扫到（`Q1216`）；PR 那一趟照旧比 merge base。另补了手动用法（合完一张票、推之前给 `HEAD^1`，快进合并给 `ORIG_HEAD`——在本树 `9cd2b2f` 这个合并提交上实跑过 `HEAD^1`，范围是 `72d5139` 以来、1 份 `.rs` 4 行），「绿可能是跳过」那段补上全零与求不到两种。`gate.yml` 与 `glossary.rs` 模块文档里同一句也改了。

**门禁与审查：** 真门禁日志 `/Users/nicoer/dev/game-wt/logs/q6-gt04-gate1.log`（3,364 行，开头两行是 `/Users/nicoer/dev/game-wt/slot-3` 与 `q6/gt-04-glossary-scan-bites-every-push`），`EXIT=0`，汇总表：
`绿 fmt 4s` / `绿 glossary 0s` / `绿 check 45s` / `绿 clippy 50s` / `绿 test 1837s cargo test --workspace --all-features --no-fail-fast -j 3 -- --test-threads=3` / `绿 numbers 4s` / `绿 doc 17s`，「7 条全绿。」；86 个 `test result`，2,631 条通过、0 条失败、2 条 `#[ignore]` 的量级测量。
glossary 那一行的范围：「merge base 就是 `HEAD`（9cd2b2f）……扫了 `crates/` 下 0 份 `.rs` 里新写的 0 行」——本票改动全没提交、也都不在 `crates/` 里，这是对的。
门禁之前 `cargo xtask numbers --write`（日志 `/Users/nicoer/dev/game-wt/logs/q6-gt04-numbers.log`，`EXIT=0`）把 README 的测试条数 2,628 → 2,633（多出的 5 条正是本票加的）。门禁之后只动了本票（`Status` 改 `done` 让票数差一，合并时在 `main` 上重写），不进编译，没跑第二趟。
`/code-review` 两轴回执开头列的审查范围与自取的 7 份文件逐个对得上。按审查改了：空值的解释从入口收进 `scan`（原先 `scan(dir, Some(""))` 会被当成全零）、CI 那条测试钉到那一步上、`HEAD^1` 补快进合并、`gate.yml` 注释收短并把 `Q1216` 的说明挪到 `concurrency` 那段、`Q1216` 的另一条路改对（只关 `cancel-in-progress` 堵不严，GitHub 一个组只留一趟排队的）。
没改的：`SinceMergeBase` / `SinceGiven` 同形、`Against` match 两次、两处 `file://` 克隆夹具重复——判断题，合起来要么改动已有的公开枚举、要么抽一个只有两处用的夹具，不划算。
