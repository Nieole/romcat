# 06: 核心库拒绝话去命令，两个壳各补去处

**What to build:** 核心库错误与拒绝的话里**不出现命令行命令**，需要的去处用结构化字段交出。命令行在印之前补命令，界面照 ADR-0005 修订段补屏上去处。
一族一起收：被后来的一批盖住、撞上外面改过的（「先导入」那半句只由命令行说，ADR-0023）、目标不在位（原因改成结构化，界面不再自己查一眼在不在位）、差量预览那两句。
另：子库长一问「它的前端格式这一版有没有适配器」，排差量预览按下那一刻就问，没有就屏上说、不排；界面两处各自查找的地方改问它。

⚠️ 与 08 都动命令行入口那个大文件，并行派时错开。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q622`：见 `../grill.md` 里那一条。
收挂单 `Q643`：见 `../grill.md` 里那一条。
收挂单 `Q851`：见 `../grill.md` 里那一条。
收挂单 `Q797`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 先写红测试：核心库这一族每个拒绝的文字里不含 `romcat`（今天有）
  —— 核心库测试，在 `1c34ce3` 上都红（或编不过）：`crates/core/tests/triage.rs::后一批盖住了前一批时先撤前一批被拒绝并说清是哪一批盖的`
  （`CoveredBy { by }` 交得出、那句不带命令）与 `撤批放回那一族拒绝的话只说事实_不带命令行命令_是哪一批交成字段`（`AlreadyUndone`、`NoBatch`）；
  `crates/core/tests/pegasus.rs::落点上有一份从没见过的文件时先停下来`（`ConflictKind::Unseen`、那句不带 `romcat import`）与 `外部改动不被静默覆盖`
  （`ConflictKind::Edited`）；`crates/core/tests/sync.rs::卡不在位时停住_而不是排出一份全删全传的计划`（`ObserveError::Absent`，不带命令、
  不带系统错误原文，`sync::observe::reach` 与它同一个答案）、`排不出计划时交出结构化的原因_那句话只说事实不带命令`（`Unplanned::NoSublibrary`、
  `Unplanned::Target(Absent)`）、`差量预览那几件怪事只说事实与去处的名字_是哪一种交成结构化的`（`Prepared::concerns` → `sync::Concern`）、
  `主库的根未连接那句只说事实_命令行旗标由命令行补`、`子库记着这一版没带的前端格式时_子库自己答得出_排计划交出这一种原因`；
  `crates/core/tests/sublibrary.rs::目标不在位时装不装得下如实说算不出_不给一个数`（`Fit::Unknown { why: Unplanned::Target(Absent) }`）。
  断言认的是命令那一截（`romcat ` 带空格、或 `` `romcat ``）：临时目录名里也带着 `romcat-`。
- [x] 命令行测试（跑二进制）：撞上每种拒绝时照旧看得到那条命令
  —— `crates/cli/tests/triage.rs::撤批放回被拒时命令行在核心库那句话后面补上那条命令`（`romcat triage undo --batch 2`、`redo --batch 2`、
  `triage batches`，在 `1c34ce3` 改核心之后红、命令行补上之后绿）；`crates/cli/tests/pegasus.rs::落点上有一份工具从没见过的文件时_命令行照旧叫人先导入`
  （stderr 里有 `romcat import`，基点上就红：那半句从前只在报告的那一份底下）；`crates/cli/tests/sync.rs` 的 `目标不在位时停住并说清怎么办`、
  `子库不在时说得清怎么建`、`子库记着这一版没带的前端格式时_说清是哪个格式_补上换一个的命令`、`主库的根未连接时同步停住_命令行补上换位置的旗标`；
  `crates/cli/tests/sublibrary.rs::主库不在位照样看得了选择集_卡不在手边时装不装得下说算不出`（`sublibrary show` 算不出时补「插上读卡器，或者
  `romcat sublibrary set 掌机 --target …`」）；`crates/cli/tests/capability.rs::子库记着的档案在名册里没有时不静默当没事_明说退回了不作声称`
  （`romcat sublibrary set 掌机 --capability`）；`crates/cli/tests/sync.rs::差量预览底下那几件怪事_核心那句只说事实_命令行照种类补上那条命令`
  （读不懂的规则 → `romcat sublibrary show 掌机`、媒体池里找不到 → `romcat scrape`、陈旧声明 → `romcat capability retroarch-exfat`，
  基点上最后那一条红：从前印的是字面上的 `<档案名>`）；导出那一条另断言 `romcat import` 整份输出里只出现一次（报告页脚不再另说一遍）。命令行补命令的几处：`triage_refusal`、`unplanned_text` / `unplanned_hint`、`warn_about`、
  导出撞上 `Unseen` 那一支、同步根未连接那一支。
- [x] 界面测试：撞上同样的拒绝时屏上有去处、没有 `romcat`
  —— `crates/gui/tests/queue.rs::撤一批被后来还在册的一批盖住时当场拒并说清是哪一批盖的_撤过的仍在记录上标着已撤`（加断言：不含 `romcat`、
  指到「裁决记录」里「第 N 批裁决」那一行）；`crates/gui/tests/roots.rs::落点上是一份工具从没见过的文件时_屏上说清为什么没写_不把人支回终端`；
  `crates/gui/tests/sublibrary.rs` 的 `子库记着的能力档案名册里没有时_差量预览底下说清退回了不作声称_去处是目标设置不是终端命令`、
  `主库的根未连接时按同步_屏上说清插上外置盘_不出现命令行的旗标`、`差量预览底下读不懂的规则与陈旧的档案声明_去处是卡上那一处不是终端命令`。这几条在 `1c34ce3` 上都红（屏上那句带着 `romcat …` / `--library-root`）。
  屏上补的几句去处话设计稿没画，记在挂单 `Q1350`。
- [x] 目标不在位：界面不再自查，屏上的话由核心库交出的原因种类挑
  —— 「在不在位」只在核心库 `sync::observe::reach` 一处判（`observe` 起手那两下）；界面 `target_absent`（`!root.exists()`）删掉，卡头标签、
  排差量预览与同步按下时改问它（`absent_path` / `absent_refusal`），核心库「目标设置」判路径时那一句在不在（`target::vet` 的 `Presence`，
  原先是 `is_dir` / `exists`）也改问它；「装不装得下」那两处悬停照 `Fit::Unknown { why: Unplanned }` 的种类挑话（`unknown_line`）。
  `crates/gui/tests/sublibrary.rs::卡不在位时卡底只写请先连接设备_路径与怎么办在悬停里` 加了一段：算过一遍容量之后悬停里不出现终端命令与
  系统错误原文（基点上红：悬停里接着核心那句原话）；`卡不在手边算得出选中多少_装不装得下如实说算不出` 改断言原因的种类。浏览屏「加入子库」
  那一层也照种类说：`crates/gui/tests/browse.rs::加入子库那一层算不出加入后多大时_照核心交出的原因种类说_不一律说目标不在位`（基点上红：
  任何原因都写「目标不在位」）。在、却列不开（路径上是一份文件）照旧排上去、记失败（`目标路径上是一份文件时排差量预览照旧排上去_任务历史记失败` 照绿）。
- [x] 子库记着这一版没有的前端格式：按下差量预览当场说、任务台上没有新一趟、没记失败
  —— 核心库 `Sublibrary::adapter()` / `sublibrary::format_adapter`（没有时交 `NoAdapter`）；`crates/gui/tests/sublibrary.rs::子库记着这一版没带的前端格式时按排差量预览_当场在屏上说_不排也不记失败`：
  按下之后任务台不忙、历史条数不变、屏上那句说清是哪个格式并指到「目标设置…」、不含 `romcat`（基点上红：排上了一趟）。界面「目标设置」弹层那两处
  （前端格式说明、设备上的位置）改问 `format_adapter`，`crates/gui/src` 里不再有 `adapter::find`。
- [x] 门禁全绿（本机不分大小写的盘上 `sync_run` 那五条已知红除外，`Q479`）
  —— `cargo xtask gate --keep-going -j 3 --test-threads 3`（`CARGO_INCREMENTAL=0`），日志 `/Users/nicoer/dev/game-wt/logs/q8-cao-06-gate1.log`
  （开头两行是 `/Users/nicoer/dev/game-wt/slot3`、`q8/cao-06`），末行 `EXIT=0`，汇总「7 条全绿」：fmt / glossary / check / clippy /
  test（1081 秒，`--no-fail-fast`，通过 2,841、红 0、挂起 2——那两条是量级测量）/ numbers / doc。括号里那条已知红已经过时：本机默认盘上
  门禁就绿。截图门单跑一趟（`/Users/nicoer/dev/game-wt/logs/q8-cao-06-snapshot1.log`，`EXIT=0`，139 条全过），基线一张没变。
  README 的票数与测试条数在本分支上 `cargo xtask numbers --write` 写回过（182 → 183 张、2,827 → 2,843 条）。

收挂单 `Q622` `Q643` `Q851` `Q797`：四条都在 `../grill.md` 各条底下标了 settled，做了什么写在那儿与上面各框里。
本票新记挂单 `Q1347`–`Q1354`（`.scratch/PARKING-LOT.md`）。
