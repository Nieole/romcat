# 08: 命令行输出改走核心库那一份

**What to build:** 命令行里自己折的、自己写的几处改调核心库那一处：报告里的「中断」「这一趟被中断了」改**部分完成**；裁决时刻用核心库那一处折法；
铺媒体回执由核心库立一句、界面与命令行都读；印给人看的主库名用显示名，不印带哈希的主库标识。
另：命令行 `sublibrary set` 调核心库的目标路径判断与名字判断，与界面同一个判据。

⚠️ 与 06 都动命令行入口那个大文件，并行派时错开。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q574`：见 `../grill.md` 里那一条。
收挂单 `Q626`：见 `../grill.md` 里那一条。
收挂单 `Q655`：见 `../grill.md` 里那一条。
收挂单 `Q454`：见 `../grill.md` 里那一条。
收挂单 `Q944`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 先写红测试：命令行 `sublibrary set` 目标落在工作目录里 → 当场被拦（今天放行）
  —— `crates/cli/tests/sublibrary.rs::目标落在主库里_工作目录里_被别的子库占着_当场拦下_与界面同一句话`：三种目标（主库的根里、
  工作目录里、套在掌机那台的目标里）各被拦下、印的是界面那张测试逐字照稿钉着的同一句，被拦的那台没写进库，掌机自己只改容量照旧放行。
  在 `d09e6f5` 上红（主库那一格当场就放行了：「已建子库「备用卡」：目标 …/FC」）。命令行改调 `sublibrary::target::vet_name` 与
  `target::vet`；拦下时那句话收进核心库（`TargetRefusal` / `NameRefusal` 的 `Display`），界面 `refusal_line` 删掉、改读它
  （`crates/gui/tests/sublibrary.rs` 118 条照绿）。没给 `--target` 时照判原来那条路径（挂单 `Q1367`）；空着那一句的变化记在 `Q1368`。
  命令行那几份测试的卡从前摆在工作目录里（`sublibrary.rs` 的 `建`、`capability.rs` 三条），挪到了工作目录之外。
- [x] 命令行与核心库报告输出里不再有「中断」（部分完成那几处）
  —— 核心库三条单元测试，在 `d09e6f5` 上都红：`report::tests::停下来的那一趟报告里叫部分完成_不叫中断`（体检报告状态行、增量那一节、
  末尾那句）、`report::duplicates::tests::部分完成与索引截断都写在明细抬头上`（重复明细，原「中断与索引截断…」那条改断言）、
  `sync::report::tests::停下来的那一趟回执里叫部分完成_不叫中断`（同步回执），三条都另断言整份输出里一个「中断」都没有。
  命令行自己写的几句：扫描「这一趟部分完成，没有重新成型」；中文索引重建停下那两句一个字没写，收场是「已取消」（词从 `Ending::Stopped` 取）；
  Ctrl-C 那两句改说「收到 Ctrl-C」「装不上 Ctrl-C 的处理」。**保留：** 命令行自己写的这几句没有跑二进制的测试——要在测试里稳稳按下一次
  Ctrl-C 落在扫描或重建中途造不出来；报告那几处是命令行原样印的核心库那一份，由上面三条钉着。
- [x] 命令行裁决记录的时刻与界面格式一致；命令行不再有自己的时刻折法
  —— 命令行的 `when` / `leap` 删掉，`triage batches` 与 `triage undo` 改调核心库 `report::human_time`（界面 `clock::Clock::short`
  也在它上面折）。`crates/cli/tests/triage.rs::裁决记录的时刻是核心库那一处折的`：两条命令印的时刻逐字等于 `human_time(那一批的 decided_at)`。
  **保留：** 这是纯重构，输出一字不变，那条测试在基点上也是绿的（钉的是往后不再分家）；命令行照旧 UTC、带年份，界面是本机时区、
  今年的不带年份——「格式一致」到同一处公历换算为止，时区那一半是挂单 `Q901` 已裁维持。命令行那条闰日、月长的单元测试搬到
  核心库 `report::render::tests::时刻写成年月日时分_闰日与月长不错`。
- [x] 铺媒体回执命令行与界面一字不差
  —— 核心库立 `MediaReport::receipt`（字照界面那一份，比命令行那份多说没铺成的几份）：`adapter::report::tests::铺媒体那句回执说得出铺了几份_怎么铺的_没铺出去的几份也说`
  钉着两种字面。命令行：`crates/cli/tests/pegasus.rs::铺媒体那句回执是核心库那一句_与界面一字不差`，stderr 里有一行逐字等于
  按 `--json` 那份报告的数折出来的 `receipt()`，在基点上红（「媒体铺出去 1 份（…），落点上本来就有的 0 份没重铺。」）。界面：
  `crates/gui/tests/roots.rs::打开铺媒体之后导出那一趟真的铺出去_任务台记完成_回执说得出铺了几份` 加一句断言，回执里那一行逐字等于 `receipt()`。
  界面的 `media_laid` 删掉。命令行在回执之后另补一句去处（有没铺出去的时「详见报告「媒体」那一节」），末尾那句重复数数的「有 N 份媒体没铺成」去掉，退出码不变。
- [x] 「还没有落过一批裁决」那两句印的是主库显示名
  —— 印 `site.display_name()`（词表**主库原名**）。`crates/cli/tests/triage.rs::印给人看的是主库原名_不是带哈希的主库标识`：`triage batches`、
  `triage undo --last` 两句与同一个毛病的第三句 `triage same-work`「主库「小库」上没有疑似同一作品的建议。」都印「小库」、不印带哈希的标识；
  在基点上红（「主库「小库-c2626158d9e8a0ef」上还没有落过一批裁决。」）。
- [x] 门禁全绿（本机不分大小写的盘上 `sync_run` 那五条已知红除外，`Q479`）
  —— `cargo xtask gate --keep-going -j 3 --test-threads 3`（`CARGO_INCREMENTAL=0`），日志 `/Users/nicoer/dev/game-wt/logs/q8-cao-08-gate1.log`
  （开头两行是 `/Users/nicoer/dev/game-wt/slot1`、`q8/cao-08`），末行 `EXIT=0`，汇总「7 条全绿」：fmt / glossary / check / clippy /
  test（1002 秒，`--no-fail-fast`，通过 2,800、红 0、挂起 2——那两条是量级测量）/ numbers / doc。括号里那条已知红已经过时：本机默认盘上
  门禁就绿。截图基线一张没变。README 的票数与测试条数在本分支上 `cargo xtask numbers --write` 写回过（179 → 180 张、2,795 → 2,802 条）。

收挂单 `Q574` `Q626` `Q655` `Q454` `Q944`：五条都在 `../grill.md` 各条底下标了 settled，做了什么写在那儿与上面各框里。
本票新记挂单 `Q1367`–`Q1370`（`.scratch/PARKING-LOT.md`）。
