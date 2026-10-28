# 14: 排计划收一个「今天」，截图测试钉死它

**What to build:** 排差量预览时「能力档案超过半年没核实」那一句，照的是真钟（`sync/prepare.rs` 的 `prepare_selected`：`profile.stale_claims(&today())`）。内置档案最早一条核实日期是 2026-08-31，到 2027-02-27 前后，截图测试里挑了 `retroarch-exfat` / `retroarch-fat32` 又排过差量预览的那几张基线（`crates/gui/tests/snapshot.rs` 约 3544、3810、4021 行）会多出一行，门禁会红在一张与它无关的票上（挂单 `Q1354`）。

让排计划收一个「今天」：`Request` 里一格，不给就是真钟；截图测试钉死它，与界面平台表那一格同一个办法（`crates/gui/src/sublibrary.rs` 约 594 行已经钉了）。

来源：拿主意的人 2026-10-04 在票 06 回报之后裁（挂单 `Q1354`）：开小票，这一轮做。

收挂单 `Q1354`：排计划收一个「今天」，截图测试钉死它。

规格：`../spec.md`。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 先写在今天 `main` 上红的测试（核心）：给排计划一个「今天」= 某份档案核实日期加 200 天 → 计划里有「超过半年没核实」那一条；给核实日期加 10 天 → 没有
    - `crates/core/tests/sync.rs::排计划收一个今天_给了就照它判档案陈旧`。先红得看得见：`Request` 还没这一格时先是 `E0560: struct Request has no field named today`；把格子加上、`prepare_selected` 还没照它判时是断言红 —— `panicked at crates/core/tests/sync.rs:795: 给了「今天」= 2027-03-19，计划里该有陈旧声明那一条：[]`（那一趟 `concerns()` 是空的）。改完：同一支测试绿，断言 `Concern::StaleClaims` 在、那句说着「超过半年没核实」；`2026-09-10`（核实日期 2026-08-31 + 10 天）时一条都没有。
- [x] 不给「今天」时照旧用真钟（一条测试钉住默认）
    - `crates/core/tests/sync.rs::排计划不给今天时照真钟判档案陈旧`：不给「今天」时 `Request::today: None`，计划报出的 `stale_claims` 与 `Roster::builtin().find("retroarch-exfat").stale_claims(&capability::today())` 逐字相等。实现是 `prepare_selected` 末尾一处：`let today = request.today.map_or_else(today, ToString::to_string);`。
- [x] 截图测试里排差量预览的那几张都钉了同一个「今天」；把钉的日期挪到半年之后、那几张会红（变异检查一次，写进证据），再挪回来
    - 钉在 `crates/gui/tests/snapshot.rs` 的 `const 钉死的今天: &str = "2026-09-15"`（原来 `钉死今天` 那个助手里的字面量提出来成了常量）。界面那一侧：`Screen::set_today` 那一格同时喂给平台表（`Claim::is_stale`）与排计划（`sync::Request::today`，`Screen::preview` 里两处 `Request` 都带 `today: today.as_deref()`，`today` 是 `self.today` 的一份拷贝）——钉一处，两处都钉住。排过差量预览的现场各补一行 `set_today(钉死的今天)`：`摆着异常的一台()`（差量异常 ×8、收回清单弹层 ×2 的现场）与 `拍刚同步完`。
    - **变异检查（做了一次）**：把常量改成 `2027-03-19`（核实日期 + 200 天）跑 `cargo test -p romcat-gui --all-features --test snapshot 子库_` → `test result: FAILED. 16 passed; 14 failed`，红的是 `子库_差量异常_{设备上缺失,放不进目标,被修改过,目标位置被占用}×{浅色,暗色}`（8 张）与 `子库_收回清单弹层_{浅色,暗色}`（2 张），外加 `子库_目标设置弹层` / `子库_目标设置弹层滚到底` ×2（那 4 张是平台表那一格本来就钉着的同一个日期，一起变陈旧，属同一处钉子）。单跑一支的红：`'sublibrary/anomalies-changed-light' Image did not match snapshot. Diff: 75396`；看 `.new.png`，多出来的正是那两行「⚠️ 这份能力档案里有 9 条声明超过半年没核实。模拟器一年发好几版，而矩阵错了比不转换更糟。」。`子库_刚同步完_{浅色,暗色}` 没红：它拍的时候那一趟同步已经让预览失效，屏上根本没有计划。
    - 挪回来（`2026-09-15`）再跑同一组：`test result: ok. 30 passed; 0 failed`；失败时留下的 `.new.png` / `.diff.png` / `.old.png` 全删了，树上不留。
- [x] 截图基线一张不变
    - 带钉死的那一版跑 `cargo test -p romcat-gui --all-features --test snapshot 子库_` → 30 张全绿（比对的是仓库里那几张 PNG）；门禁那一趟全量 `snapshot` 也是绿的（日志见下一条）。`git status` 里 `crates/gui/tests/snapshots/` 一张都没动。
- [x] 门禁全绿
    - 日志 `/Users/nicoer/dev/game-wt/logs/cao-14-gate2.log`（开头两行 `/Users/nicoer/dev/game-wt/slot3` / `q8/cao-14`），末行原文 `EXIT=0`，`── 门禁 ──` 底下七条全绿：`fmt` 4s、`glossary` 3s、`check` 0s、`clippy` 1s、`test` 1779s、`numbers` 5s、`doc` 5s，随后一行「7 条全绿。」。那一趟 `数：测试条数=2,990 测试目标数=90 票数=201/223 ADR份数=24`。
    - 头一趟 `cao-14-gate1.log` 只红了 `fmt`（我在测试里写的一个 `assert_eq!` 换行），其余六条绿（`test` 2733s 绿 = 全量测试含全量截图基线绿）；`cargo fmt --all` 改完跑的第二趟就是上面这条。`numbers` 已照 `cargo xtask numbers --write` 写回 README：测试条数 2,988 → 2,990、票数 200/223 → 201/223。
    - 收尾时 `git merge q8/cao-int` 是 no-op（`Already up to date.`，我们的基点是它的尖端 `f37723f`），因此没有第二趟合并后的门禁。

**收尾：** 挂单 `Q1354` 已在 `.scratch/PARKING-LOT.md` 上写清这张票做了什么（条目原本已是 `settled`，补上「这一票实际做了什么」与落点）。另记一条新挂单 `Q1807`：核心与界面那几份**不是截图**的测试照旧用真钟排计划，到 2027-02-27 前后它们会不会红，这一票没管。
