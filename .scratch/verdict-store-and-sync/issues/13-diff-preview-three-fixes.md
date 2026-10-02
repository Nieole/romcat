# 13: 差量预览三处：补回、撞车容量、--json

**What to build:** - 勾「补回」时先停掉台上那一趟再重排，不白跑。
- 「放不进目标」那一行撞车的几份只算一次容量，屏上写明口径。
- 撞车明细进 `--json`。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q1029`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 先写红测试（核心库）：三份撞到同一路径 → 放不进目标的容量只算一份（今天算三份）
  - `crates/core/tests/sync.rs` 的 `三份撞到同一条落点_放不进目标的容量只算一份`：三份（1000 / 3000 / 2000）撞一处、两份（500 / 500）撞另一处、一份不撞。修之前红在 `left: 7000, right: 3500`（日志 `/Users/nicoer/dev/game-wt/logs/q8-vs-13-red1.log`），修之后绿；份数照旧 5、变体 5。
  - 一处取**最大的那一份**（`Collision::bytes`），票没说取哪一份，记挂单 `Q1667`。命令行报告原先自己把每份字节加一遍（`report.rs`），一并改走 `Plan::rejected_tally`。
- [x] `romcat sublibrary preview --json` 里有撞车明细（命令行测试）
  - ⚠️ 票写错了一处：这条命令叫 `romcat sublibrary plan`，没有 `preview` 子命令；测试照真名写。`sync --json` 打的是同一份计划，也带上了。
  - `crates/cli/tests/sync.rs` 的 `撞车明细在json里_撞在一起的几份归成一处`：第二块盘上同一条相对路径扫进同一份库，`plan --json` 里有 `collisions`，一处两份、按主库侧的键排（`另一块盘/…`、`库/…`），平铺的 `rejected` 照旧在；文本报告印「2 个、2.00 KiB。」与口径句。修之前红在「--json 里没有撞车明细」（日志 `…/q8-vs-13-red2.log`）。
  - 做法：归堆挪成 `Collision::among`，排计划时存进字段 `Plan::collisions`（ADR-0024 推论 3）。`--json` 还没有放不进目标那一笔账，记挂单 `Q1670`。
- [x] 界面测试：差量预览跑着时勾「补回」→ 台上那一趟被停、新一趟起来
  - `crates/gui/tests/sublibrary.rs` 的 `差量预览排着时勾补回_台上那一趟先停掉_按新的勾排的那一趟起来`：旧那一趟不在队里、任务历史记「已取消」，排完的只有新那一趟，屏上不说「已取消」。修之前红在「按旧的勾排的那一趟还在队里，白跑一趟：[2, 3]」（日志 `…/q8-vs-13-red3.log`）。
  - **保留**：测试用占位活把旧那一趟钉在**队里**（不带竞态），钉的是「撤掉排着的那一趟」这一支；真在跑的那一支走同一个 `Board::stop`（递停下信号，收场时号对不上被 `settle` 放过），读代码成立，没有单独一条测试钉住。
  - 同一个病根（只弃认不停）在换卡、新建、删子库与「算一遍容量」那几处还在，记挂单 `Q1669`。
  - 口径句上屏另有一条：`放不进目标那一格撞车的几份只算一次容量_屏上写明口径`（第五格画只算一次的容量，小字行有口径句；没撞车时不写）。摆在五格底下那行小字里，记挂单 `Q1668`。
- [x] 门禁全绿（本机 `sync_run` 那五条已知红除外，`Q479`）
  - 「那五条已知红除外」已过时（本机默认盘上门禁就绿），这一趟没有例外：门禁 `/Users/nicoer/dev/game-wt/logs/q8-vs-13-gate1.log`（开头两行 `/Users/nicoer/dev/game-wt/slot1`、`q8/vs-13`）末行 `EXIT=0`，汇总表七条全绿（fmt 2s / glossary 0s / check 26s / clippy 20s / test 1887s，带 `--no-fail-fast` / numbers 8s / doc 24s）。numbers 照新数写回 README：票数 173/209、测试条数 2,738。
  - 截图基线重批八张：`sublibrary/anomalies-{missing,nofit,changed}-{light,dark}`、`sublibrary/take-back-{light,dark}`——第五格 12.79 KiB → 8.79 KiB、小字行多一段口径句、下面整块下移一行；并排图与说明 `/Users/nicoer/dev/game-wt/logs/q8-vs-13-compare/CHANGED.md`。
