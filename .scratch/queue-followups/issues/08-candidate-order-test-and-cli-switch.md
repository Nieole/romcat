# 08 — 候选按可信程度排：补测试，并给命令行按依据形状选的开关

**What to build:** 待确认队列那一层的「候选按可信程度排」**一条测试都没有**，眼下靠人肉复核。这一票给它补上。

同时把**按依据形状选一批**的开关补进命令行——待确认屏已经按依据形状分批，命令行却选不出同一批，两边拿到的东西不一致。

**Blocked by:** 无 —— 可立即开工

**Status:** done

- [x] 「候选按可信程度排」有测试钉着，次序是确定的
      —— **三条**，各钉一段（为什么要分三条见挂单 Q173）：
      `crates/core/src/identify.rs::tests::候选先按可信程度排再论平台与源的先后` 直接拿
      私有的 `rank` 比三层：中文离线源**中置信**那条排在 No-Intro **低置信**那条前面
      （可信程度压过源）、平台对得上的压过源的先后、都一样时才按
      `No-Intro → Redump → TOSEC → MAME → GoodNES` 论。
      `crates/core/tests/identify.rs::中立库交回候选的次序就是按可信程度排的那一个`
      走完整一趟识别再从中立库读回来：`库/FC/谁也不认得.zip` 上摆两条，**高置信那条出自
      GoodNES（源排第五）、中置信那条出自 No-Intro（源排头一个），而且中置信那份 DAT
      先写进去**——两种排法给出相反的答案，交回来的必须是 `[GoodNES 高置信, No-Intro
      中置信]`。`crates/core/tests/triage.rs::一级分批的键取的是最可信的那条候选`
      在队列里钉分批的键与 `candidates[0]` 是同一个。
      **变异实测两次**：把 `rank` 里置信度那一层换成常量 0，第 1、2 条当场红；
      把整条 `sort_by` 拿掉（Q82 说的旧写法「只剩写入顺序」），第 2、3 条当场红
      （`left: [("GoodNES", Medium), ("No-Intro", Medium)]`）。
- [x] 命令行选得出与界面同一批（按**依据形状**）
      —— `romcat triage list / decide / undo` 都多了一个可重复的 `--shape`
      （`crates/cli/src/main.rs::TriageFilterArgs`）。认那串字的活**在核心库里**：
      `triage::batch::Shape::selector` / `Shape::parse` 那一对，命令行只负责把它塞进
      `Filter::shape`（ADR-0005）。报告多一张「按依据形状」表把每一批连它那串字一起印出来
      （`QueueReport::by_shape`），`triage list` 末尾再给一行整条可粘贴的 decide 命令，
      **这一趟的选择器一样不少地带上**——库选择器走 `TriageCommonArgs::选择器()`，
      队列选择器走新加的 `TriageFilterArgs::选择器()`（挂单 Q175、Q178：只带 `--shape`
      的话，那张表上的条数是筛过之后数出来的，而命令跑的是全库那一批）。
      端到端：`crates/cli/tests/triage.rs::报告印出来的那串字照抄一条就选中同一批`
      ——从报告 JSON 里逐条抄 `selector` 跑回去，`selected` 与那一批的 `count` 一个数都不差
      （2 条那批与 1 条那批，加起来正是队列的 3 条）；
      `依据形状写岔了当场说清该怎么写` 钉住写岔了不是静悄悄选中零条；同一条测试里另跑一趟
      `--name 甲`，断言印出来的命令里有 `--name '甲'`。
      段界那条认法**只有头一段没有闭合词表兜底**，所以另有一条
      `crates/core/src/triage/batch.rs::tests::源那一段里不许出现分隔符` 把前提钉在真的
      源名上（内置数据源清单 ＋ 代码里写死的那四个），哪天有人加一个带 ` / ` 的源名当场红
      （挂单 Q180）。
- [x] 界面与命令行选出来的是同一批，有测试对照
      —— `crates/gui/tests/queue.rs::界面点开的那一批与命令行按同一串字选出来的一条不差`：
      屏上**逐批**点开（合成数据分出 4 批以上，两支都有），拿 `Queue::members(&scope)`
      收下界面这一侧盖住的键；再把同一个形状折成那串字、`Shape::parse` 认回来、走
      `triage::survey` 从中立库重折一遍队列，两个键列表必须一模一样。
      **不是空对空**：每一批都断言 `!界面这批.is_empty()`。
- [x] 门禁全绿（`--all-features`）
      —— `cargo clippy --workspace --all-targets --all-features` 干净；
      `cargo test --workspace --all-features -- --test-threads=2`：
      **58 个 `test result: ok`、1,581 条通过、0 失败**（基线 1,571 条 ＋ 这一票新增 10 条）。


## 挂单裁决

从 `.scratch/PARKING-LOT.md` 迁来。那份挂单是这一轮队列的**待办队列**，收尾时清空；
裁决之后条目迁回它所属的票，编号 `Qn` 留着占号、不复用。

### Q82 — 一级分批的键取「第一条候选」，不取「置信度最高的那条」

- **来自：** 票 `gui-redesign/09`
- **类别：** 票没想到的第三种情况
- **在哪：** `crates/core/src/triage/batch.rs::Shape::of`
- **为什么没停线：** 两条路里只有一条自洽，而它就是这张票要的那条。
- **这张票实际走了哪条路：** 取 **`candidates[0]`**。理由是「整批通过」在领域里就是
  `DecisionSpec::Pick(1)`——它采用的正是第一条。取「置信度最高的那条」的话，卡片上那句
  共同依据说的是 A、按下去做的是 B，而**整批通过时人验证的正是那句话**，那句话就成了假的。
- **它挂着的那条线：** 候选从中立库出来是**按写入顺序**（`candidates_of` 的
  `ORDER BY id`），也就是识别当初产出它们的顺序。眼下第一条实际上就是最可信的那条
  （`identify::rank` 排过），但**没有一条测试钉住这件事**。哪天识别改了写入顺序，
  分批的键会跟着变，而没人会发现。
- **顺带：** 「**按依据形状**」这个选择器只有界面上点得到——`Filter::shape` 在命令行上
  没有对应开关（`crates/cli/src/main.rs::TriageFilterArgs::build` 那里留了注释）。
  它要一串「源 / DAT / 置信度 / 哈希口径 / 候选数」的写法，那是另一张票的事。
- **谁来裁：** 收尾——要么给「候选按可信程度排」补一条测试，要么开一张票让命令行也能按形状选。
- **状态：** 本票做完了 —— **两件都做了**，不是二选一。测试三条（见验收第 1 条，
  变异实测两次都红过）；`--shape` 落在 `TriageFilterArgs` 上，那串字的折算在核心库里
  （`Shape::selector` / `Shape::parse`），报告与 `triage list` 末尾把它印出来给人抄。
  这条挂单**从这里起不再 open**。
- **收尾裁决：** **由本票接着做** —— 它就是从这条挂单立起来的。
