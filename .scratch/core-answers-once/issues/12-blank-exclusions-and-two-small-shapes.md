# 12: 筛选 `!=` 白写要说，外加两处小形状

**What to build:** 筛选写了 `!=` 后面跟库里没有的值（这一条什么都没排除），屏上说「这一条没排除掉任何东西」。
顺手收两处小形状：删掉没人调用、多段扩展名还会判错的那个判扩展名函数；「影响」那几行并成一支画法、参数换成具名类型，条目集合答一个具名键。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q1101`：见 `../grill.md` 里那一条。
收挂单 `Q949`：见 `../grill.md` 里那一条。
收挂单 `Q984`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 先写红测试：条件组里写 `平台 != 库里没有的平台` → 屏上有那句提醒（今天没有）
  - 红：`crates/gui/tests/browse.rs:5032` panic「这一条没被点名：「这一条没排除掉任何东西」」（改核心库之前单跑）。
  - 绿：`crates/gui/tests/browse.rs` 的 `不等号后面写了库里没有的值屏上说没排除掉任何东西`——屏上印的是核心库 `ThinClause::NothingExcluded` 那一句。
- [x] `==` 那几支原有的提醒照旧
  - `crates/core/tests/filter.rs` 的 `筛不出东西的子句点得出名而且不当错拦住`（13 条全过：`平台=GBX`／`合集=还没建的`／`评分>=0.8` 照旧点名，`平台!=GB`、`平台!=GB,没这个平台` 不误报）。
  - `crates/gui/tests/browse.rs` 的 `筛不出东西的子句屏上逐条点名但不拦着`、`筛不出东西的提示贴在那一条子句底下_默认窗口不滚就整句看得见` 两条照旧绿。
- [x] 那个判扩展名函数全仓无引用、已删
  - `grep -rn '\.takes(\|fn takes\b' crates/ xtask/ --include=*.rs` 无结果；`crates/core/src/capability.rs` 的 `Accepts::takes` 删掉，唯一调用点（`crates/core/src/sync/playlist.rs` 判吃不吃 `.m3u`）改问 `takes_key`。`cargo test -p romcat-core --test sync_run 播放列表`（14 条）与 `… m3u`（1 条）全过。
  - 核查时「全仓零调用方」的结论在票落地前已被 `verdict-store-and-sync/11` 添了一个调用方，删的时候一并改过来。
- [x] 「影响」只剩一支画法；受影响的截图基线（若有）重批
  - `crates/gui/src/look.rs` 只剩 `impact(ui, tone, parts)`：`impact_warn` / `impact_row` 并掉，`(字, 强调)` 换成具名 `ImpactPart`；`crates/core/src/adapter/converge.rs` 的 `Entries::of` 答具名 `EntryKey`，`catalog/roots.rs` 与 `triage/merge.rs` 那几处键跟着换。
  - 截图套件不带 `UPDATE_SNAPSHOTS` 单跑一次：159 张全过 → 一张基线都没受影响，无需重批。
- [x] 门禁全绿（本机不分大小写的盘上 `sync_run` 那五条已知红除外，`Q479`）
  - `/Users/nicoer/dev/game-wt/logs/cao-12-gate2.log` 末行 `EXIT=0`，七条全绿（fmt／glossary／check／clippy／test／numbers／doc）。
  - 第一趟 `cao-12-gate1.log` 红在 clippy：`crates/gui/tests/snapshot.rs:936` 的 `.map(&号)`（既有 lint，与本票改动无关），照 clippy 改成 `.map(号)` 后第二趟全绿。
