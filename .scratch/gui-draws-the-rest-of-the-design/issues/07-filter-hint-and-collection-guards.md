# 07: 左栏与合集：子句旁提示、警告中性色、空名拦

**What to build:** - 「这一条筛不出东西」的提示贴在条件组框里出问题的那一条子句旁（不再在左栏折线以下）。
- 「加入合集」那块「只钉得住本机路径」的说明换中性色，认下常驻。
- 往合集里加／移出时，排活那个入口起手校验合集名：空名带理由拒绝（ADR-0005 修订段），弹层上常驻理由、可同时画灰。

⚠️ 05–09 都动浏览屏那一个大文件，并行派时错开。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q1104`：见 `../grill.md` 里那一条。（已收，挂单标 settled）
收挂单 `Q1110`：见 `../grill.md` 里那一条。（已收，挂单标 settled）
收挂单 `Q798`：见 `../grill.md` 里那一条。（已收，挂单标 settled）

**Blocked by:** 01

**Status:** done

- [x] 先写红测试（界面）：条件组里写一条筛不出东西的子句 → 提示就在那一条旁、默认窗口首屏内看得见
  证据：`crates/gui/tests/browse.rs` 的 `筛不出东西的提示贴在那一条子句底下_默认窗口不滚就整句看得见`——规则 `合集=还没建的 且 简介~勇者冒险`，
  默认 1280×800 那一帧上断言核心库那句（`ThinClause::advice`）整句落在左栏滚动区露出来的那一截里（`shared::画着的每一处连裁剪`，外框在裁剪矩形内）；
  够高的一帧上断言它夹在头一条的值框与下一条的值框之间。在 `bebd654` 上红：那句话画在 y 722–779，左栏露出来的只到 758。
  改法：`crates/gui/src/filter.rs` 的 `leaf_ui` 在每一条子句底下调 `thin_clause_ui`（说明字号、`mid` 色，照稿 `.tnote`），框底「有 N 条筛不出东西：」那段汇总拿掉；
  读子句收成 `Leaf::clause` 一处，折规则与这一问共用。原来那条 `筛不出东西的子句屏上逐条点名但不拦着` 改断核心库那三句话（从前断的「没这个平台」几个字在值框里本来就有，提示不画也绿）。
- [x] 合集名空着直接调排活入口 → 不排、屏上有理由（今天会排出去）
  证据：`crates/gui/tests/browse.rs` 的 `合集名空着直接调排活入口_不排上任务台_屏上说为什么`——勾一行，名字空串与全角空格各一档，`join_collection` / `leave_collection` 交替调四趟，
  每趟断言任务台历史一条没多、屏上写着「加入合集没开跑：请输入名称。」或「移出合集没开跑：请输入名称。」（相邻两趟说的话不同，才断得出这一趟真说了话），最后沉淀库里一个合集都没有。
  在 `bebd654` 上红：排上了一趟「放进「」· 1 个变体」，记失败「合集得有个名字…」。改法：`crates/gui/src/browse.rs` 的 `queue_collection` / `queue_collection_keys` 起手各问一遍 `合集名空着`
  （取 `collection::check_name` 交回的 `BadName::Empty`，与「上一趟还在跑」同一个摆法）。弹层上常驻同一句：`加入合集那一层建得出新合集_名字写不得时加不进` 补断名字空着时屏上就写着「请输入名称。」（弱色帮助字），改好名字后消失；在 `bebd654` 上红（那句只在悬停里）。
- [x] 「加入合集」说明是中性色（令牌断言）
  证据：`crates/gui/tests/browse.rs` 的 `加入合集那块路径锚说明是中性底_不是警示色`——按「加入合集…」、跑完淡入，断言「只钉得住本机路径」那句底下垫着的框填的是令牌 `panel-2`（设计稿 `.note`），
  不是 `lo-soft` 也不是 `mid-soft`；在 `bebd654` 上红（`warn_box` 的 `lo-soft`）。改法：`crates/gui/src/browse/collections.rs` 的 `路径锚的说明` 走 `look::note_box`，认下常驻。
  `tokens.toml` 没动，没有新令牌，`check_tokens.py` 不用跑。
- [x] 浏览屏左栏与合集弹层截图基线重批；每张新截图基线先由编排者对稿自审，过关的再交拿主意的人点头
  证据：既有基线一张没变——全量截图门 `/Users/nicoer/dev/game-wt/logs/q8-gd-07-snap1.log`（开头两行这棵树与这条分支，`139 passed; 4 failed`，红的四条正是新加的、还没有基线的四张，末行 `EXIT=101`）；
  随后只对那四条 `UPDATE_SNAPSHOTS=1`，新增 `browse/filter-thin-{light,dark}`、`browse/join-collection-{light,dark}`（`tests/snapshot.rs` 的 `拍条件组筛不出`、`拍加入合集`）。
  并排图、稿图与逐张说明在 `/Users/nicoer/dev/game-wt/logs/q8-gd-07-compare/`（`CHANGED.md`）。编排者对稿自审过；拿主意的人 2026-10-04 放行，三道岔路口见 Comments。
  **保留**：并排图「旧」那一格是占位——这两态在基点上从来没有基线，旧样子写成文字（把源码临时换回基点渲旧图被本机权限守卫拦下）。
- [x] 门禁全绿（本机 `sync_run` 那五条已知红除外，`Q479`）
  证据：`/Users/nicoer/dev/game-wt/logs/q8-gd-07-gate1.log`（开头两行 `/Users/nicoer/dev/game-wt/slot2`、`q8/gd-07`；`cargo xtask gate --keep-going -j 3 --test-threads 3`，`CARGO_INCREMENTAL=0`；
  跑在提交之前、改完票面 `Status` 与 `numbers --write` 之后），末行 `EXIT=0`，汇总表「7 条全绿」：fmt、glossary（扫了 6 份 `.rs` 新写的 469 行，没撞上）、check、clippy、
  test（`--no-fail-fast`，724 秒，各测试二进制合计 2,894 passed、0 failed）、numbers、doc。`Q479` 那五条已过时：本机默认盘上门禁就绿，这一趟没有例外。
  `numbers --write` 写回 README 两处：票数 188/219 → 189/219、测试条数 2,889 → 2,896（本票加了三条界面测试、四张截图）。

## Comments

**拿主意的人 2026-10-04 裁**（人的关：四张新基线 `browse/filter-thin-{light,dark}`、`browse/join-collection-{light,dark}` 放行；
并排图与选择题原文在 `/Users/nicoer/dev/game-wt/logs/q8-gd-07-compare/CHANGED.md`）：

1. 「加入合集」那块路径锚说明的底色：**甲，中性灰底**（设计稿 `.note`、令牌 `panel-2`，`look::note_box`），常驻。
   稿上那块是偏黄的 `.midbox`，而且只在这一批里真有无判据的时候才画；我们每次都画，所以不跟稿上的颜色。
2. 「新建合集」那一档名字空着时：**甲，常驻一句弱色的「请输入名称。」**，「加入」照旧画灰（ADR-0005 再修订）。
   打了字还用不得的那几档照旧红字。
3. 「筛不出东西」那句话的措辞：**甲，用核心库那句**（`sublibrary::ThinClause::advice`，票 `gui-looks-like-the-design/12` 定的），不照稿改短。
4. 挂单 `Q1477`（框底「还有 N 条没生效」照旧汇总）与 `Q1478`（改名那一行空名理由只挂悬停）：**照建议做，但不在本票**，
   并进编排者新开的票 `gui-draws-the-rest-of-the-design/23`（条件组与合集弹层照稿）。
