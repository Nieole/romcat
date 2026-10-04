# 23: 浏览屏左栏条件组与「加入合集」那一层照稿

**What to build:** 编排者 2026-10-04 看票 07 的并排图时，发现两处与稿差得远，又没有任何开着的票管：

- **左栏条件组**（稿 `.gtree` / `.tg` / `.tgh`，`prototype.html` 约 335、677 行与 `renderTree` 约 2885–2900 行）。
  - 稿上：组左边一道强调色竖线（嵌套组改 `mid` 色）；头一行「另加条件：」后接连接方式下拉（「任一满足」）；每条子句是「字段 ▾ / 运算 ▾」一行、值框一行；删除的 `×` 在子句右边；「+ 子句」「+ 组」在组底下。
  - 今天：`×` 在左；「+ 子句」「+ 分组」在顶上；没有竖线、没有「另加条件：」。
- **「加入合集」那一层**（稿 `openColl` 约 2678–2693 行）：稿上每一档（已有的合集、「新建合集」）是一张带边框的卡片，名字一行、小字一行（「6 个作品 · 其中 1 个已经在这个合集中」），选中的那张描强调色边。今天是光秃秃的单选。

另收两条票 07 记的挂单，与上面同一片：
- 条件组框底「还有 N 条没生效」那段，照「筛不出东西」那句的做法拆到每条子句底下（挂单 `Q1477`）。
- 「管理合集」改名那一行，名字擦空时理由常驻弱色「请输入名称。」，与「加入合集」那一层同一句（挂单 `Q1478`）。

**先盘点差距再动手**：照 `../gaps-diff-preview-and-merge.md` 那份的做法，把这两块与稿逐项对一遍：结构、留白、颜色、字号、各态，以及稿没画的态。写成 `../gaps-condition-group-and-collections.md`。稿与词表 / 字体 / 核心能力冲突的、稿没画的，列成选择题，**停下回来交拿主意的人裁**，裁完再做。

来源：拿主意的人 2026-10-04 在票 07 的人的关上裁：开一张票，这一轮照稿做；`Q1477`、`Q1478` 并进来。

收挂单 `Q1477`：「没生效」那段拆到每条子句底下。（已收，挂单标 settled）
收挂单 `Q1478`：改名擦空时理由常驻。（已收，挂单标 settled）

规格：`../spec.md`；稿 `../../gui-looks-like-the-design/prototype.html`；ADR-0005（读到底：画灰时理由常驻）。

⚠️ 05–09、19、21、22 与本票都动浏览屏（`browse.rs`、`filter.rs`、`collections.rs`），串着做。

**Blocked by:** 07

**Status:** done

- [x] 差距清单写成、岔路口交拿主意的人裁过（裁定写进清单末尾）
  证据：`../gaps-condition-group-and-collections.md`（照稿 18 条 G-01…G-14、J-01…J-04、R-01；岔路口 F-1…F-9；核心缺 C-1、C-2），
  稿图与 11 张并排图在仓外 `/Users/nicoer/dev/game-wt/logs/q8-gd-23-gap-audit/`。拿主意的人 2026-10-04 逐条裁了，写在清单末尾「裁定」一节
  （F-9 选 C，其余照推荐）。
- [x] 先写在今天 `main` 上红的测试（界面）：条件组的 `×` 在子句右边、「+ 子句」「+ 组」在组底下、组左边有那道竖线（拿矩形与画出来的形状断，不比像素）；「加入合集」每一档是带边框的一块、选中的那块描强调色边
  证据：`crates/gui/tests/browse.rs` 的 `条件组照稿_叉在子句右边_两颗加号在组底下_组左边一道竖线`——规则 `平台=GBA 且 (类型~RPG 或 作品^口袋)`，
  拿值那一格的框（包着值的最小一块方块）断「×」在它右沿之外、竖着落在这一条两行里；「+ 子句」「+ 组」各两颗、都在最后一条子句底下；
  顶层左边一道强调色竖条从头一条伸到最后一条、套进来的组一道 `mid` 色竖条只管它那两条；组头「另加子句：」「组：」；运算符收起写 `Op::short`。
  在基点上红：`/Users/nicoer/dev/game-wt/logs/q8-gd-23-cold1.log`（「平台」那一条的「×」画在值那一格左边，x 231–244 对值框 219–339），`EXIT=101`。
  `加入合集那一层每一档是一张带框的卡_选中的那张描强调色边`——库里两个合集，三档名字外头各有一张填了底、描了边的卡；点「送朋友的」那张**右半边空白处**，
  只有它的描边换成强调色。在基点上红（那一档外头没有带框的卡）。
  另有几条照裁定立的：`加子句加组照稿起手_焦点落进新那一条的值框`（F-3）、`组最多套三层_第三层不给加组`（F-2）、
  `条件组空着时说它是干什么的_规则原文写没有条件全部作品`（G-04、G-06、F-1）、`管理合集那一层名单装在框里_每一行的按钮靠右对齐`（F-8），
  `管理合集删除确认的两颗按钮摆在警示框里头`（F-8，审查补的），
  核心库 `sublibrary::rule::tests::运算符的短词照设计稿_符号打头摆得进条件组那一格`（F-4、C-1，九个运算符逐个走）。
  **每条都在写它那一刻的树上先看过断言红**（一片一片做，前一片已经绿着）；拿到基点 `44c40b7` 上，整份 `tests/browse.rs` 因为用了新加的
  `Op::short` 编不过——那是编译红，断言本身照上面说的在基点上也都红（没有「+ 组」、没有卡、没有框、焦点不进值框）。
- [x] 「没生效」那段贴在那条子句底下，默认窗口不滚就看得见（与票 07 那条同一把尺子）
  证据：`写错的子句底下贴一句红字_框下只留一行计数_默认窗口不滚就看得见`——规则 `年份>=1990 且 简介~勇者冒险`，点进 1990 删成 199；
  默认 1280×800 那一帧里核心库那一句（`Clause::build` 交回的 `RuleError`，F-5 裁 A）整句落在左栏露出来的那一截里（`画着的每一处连裁剪`，同票 07 那条）；
  够高的一帧里它夹在「199」与下一条的值之间、`lo` 色；框下一行「1 个子句写错了，改正前不会生效」在最后一条底下、`lo` 色；屏上不再有「没生效：」。
  在基点上红：那一句只在框底汇总里（「　年份>=199 —— …」），单独一句画了 0 处。
  改法：`crates/gui/src/filter.rs` 的 `leaf_ui` 在写错那一条底下印 `RuleError`、三格描 `lo`，没填的值框描虚线；`pending` 那一串换成 `Unset` 两个数，
  框下那一行由 `Filter::unset_ui` 画（`browse.rs` 在 `rule_box` 之后调），只有没填的用 `mid`。
- [x] 「管理合集」改名擦空时屏上常驻「请输入名称。」，与「加入合集」那一层是同一句（同一个来源）
  证据：`管理合集改名擦空时屏上常驻请输入名称_与加入合集那一层同一句`——按「改名」、把框里的名字删光，屏上正好一处
  `collection::BadName::Empty.advice()`，在「保存」底下、帮助字的弱色 `ink-3`。在基点上红（那一句只在悬停里，画了 0 处）。
  **同一个来源**：两个弹层都走 `crates/gui/src/browse/collections.rs` 的 `名字用不得的理由`（取 `check_name` 交回的那一档，空名弱色、其余红），
  「加入合集」那一层原来那一段 `match` 收进了它；排活入口拒下时说的也是 `BadName::Empty` 那一句（票 07）。位置照「加入合集」那一层摆框底下
  （拿主意的人 2026-10-04 定，不照稿摆右边）。
- [x] 动了令牌跑 `python3 .scratch/gui-looks-like-the-design/check_tokens.py`，新令牌都有核对，结果写进证据
  证据：新立 9 个键（11 格）——`[space]` 的 `rule-group-padding`、`rule-group-gap`、`rule-gap`，`[layout]` 的 `choice-card-gap`、`rule-group-bar`、
  `rule-join-height`、`rule-join-padding`、`rule-control-padding`、`rule-clause-columns`；脚本里新加 `filter_literals` 一段逐格对稿（`.tg`、`.tg .tgh`、
  `.tg .tgh select`、`.tc`、`.tc .tci`、`.tc select,.tc input`、加号那一排、`DLG.coll` 的 `.col`）。跑出来退 1，只有基点上本来就有的两条
  （`size-title: 设计稿是 17，令牌是 15`、视频播放标 shade）——在基点 `44c40b7` 的脚本与令牌上跑一遍是同样两条；没有新的不一致，没有「没人核」的格。
- [x] 受影响的截图基线重批；每张新截图基线先由编排者对稿自审，过关的再交拿主意的人点头
  证据：改了 30 张、新立 8 张，逐张说明与「旧｜新｜稿」并排图在仓外 `/Users/nicoer/dev/game-wt/logs/q8-gd-23-compare/`（`CHANGED.md`）。
  改了的：`browse/{rows,cards,covers,empty,context-menu,keys-sheet,suspicion,scrape,filter-thin,join-collection}-{light,dark}`、`browse/scrape-blocked-light`、
  `browse/scrape-online-light`、`merge/{step1,step2,step3,suggest}-{light,dark}`——差距清单预计 22 张，多出那 8 张 `merge/*` 是向导盖着浏览屏、左栏露出的一条
  （弹层本身没动）；`merge/split-*` 一个像素没动（选择卡抽成 `look::mode_card` 之后画法逐像素一样）。新立的：`browse/filter-pending-*`（1280×1080，断言计数与规则原文整个露在左栏里）、
  `browse/join-collection-existing-*`、`browse/manage-collections-*`、`browse/manage-collections-rename-*`。
  日志：重批前 `q8-gd-23-snap1.log`（`117 passed; 38 failed`），重批 `snap2-update`、`snap5-update`、`snap8-update`（自审改了两回规则原文空态那一句），
  复跑 `q8-gd-23-snap9.log`（开头两行这棵树与这条分支，`155 passed; 0 failed`，末行 `EXIT=0`）。编排者对稿自审过，拿主意的人 2026-10-05 放行 38 张（见 Comments）。
- [x] 门禁全绿
  证据：`/Users/nicoer/dev/game-wt/logs/q8-gd-23-gate1.log`（开头两行 `/Users/nicoer/dev/game-wt/slot2`、`q8/gd-23`；`CARGO_INCREMENTAL=0 cargo xtask gate --keep-going -j 3 --test-threads 3`，
  跑在提交之前、改完票面 `Status`、`numbers --write` 与审查的修改之后），末行 `EXIT=0`，汇总表「7 条全绿」：fmt、glossary（扫了 9 份 `.rs` 新写的 1636 行，没撞上）、
  check、clippy、test（`--no-fail-fast`，959 秒，各测试二进制合计 2,940 passed、0 failed）、numbers、doc。
  `numbers --write` 写回 README 两处（`/Users/nicoer/dev/game-wt/logs/q8-gd-23-numbers.log`）：票数 192/221 → 193/221、测试条数 2,925 → 2,943
  （本票加了 9 条界面测试、1 条核心库单元测试、8 张截图）。

## 审查（`/code-review`，2026-10-05）

两轴各派一个只读 agent，回执开头列的审查范围与改动清单逐个对上（这棵树、基点 `44c40b7`、未提交的改动）。

- **Spec 轴**：G-01…G-14、J-01…J-04、R-01、F-1…F-9 逐条对过都做了。改了的一条：删除确认那两颗按钮从警示框外头挪进框里头（照稿 `.warnbox`），
  `look::warn_box` 加了一个带内容的 `warn_box_then`，补测试 `管理合集删除确认的两颗按钮摆在警示框里头`（先在旧摆法上看过红）。
  说成越界、留着的两样：规则原文空态那一句另起一行、用常规体（自审时改的，已随 38 张基线过了人的关，理由在 `browse.rs` 的 `rule_text` 文档里）；
  `merge/*` 多变的 8 张（向导盖着浏览屏、左栏露一条，写进了上面基线那一条证据）。
- **Standards 轴**：没有硬违规。照判断题改了的：`browse.rs` 那一句「四万多个变体」补「见台账」；`filter.rs` 的 `Unset` 改名 `NotInEffect`
  （与 `not_in_effect()` 对上），三分档收成 `这一条怎样::of`，`thin_clause_ui` 收现成的子句不再读第二遍，两处小控件尺寸收成 `小控件`，
  `Leaf::added` 去掉裸 `bool`、加 `.focused()`；`collections.rs` 的左字右钮挪进 `look::text_and_buttons`，按钮上的字只写一遍，
  幽灵按钮走 `look::small_ghost_button`，新建那一档的理由沿用这一帧问过的那一回 `check_name`；测试里的重复助手、建合集的夹具收成一处。
  没改的：别的几处测试里的形状遍历各断各的（方块、竖条、字色），没收进 `shared`。

## Comments

**拿主意的人 2026-10-04 裁**（差距清单九道岔路口，原文与裁定在 `../gaps-condition-group-and-collections.md` 末尾）：F-1 选 B（头一行「另加子句：」、
说明句「…等维度加子句」，规则原文空态照稿）；F-2 选 A（最多三层，只在界面上限）；F-3 选 A；F-4 选 A（短词补进核心库 `Op`）；F-5 选 A；F-6 选 A；
F-7 选 A；F-8 选 A（「管理合集」那一层其余差距并进本票）；F-9 选 C（全仓留「▼」）；改名擦空那一句摆输入框底下。

**拿主意的人 2026-10-05 裁**（人的关：38 张基线放行；并排图与选择题原文在 `/Users/nicoer/dev/game-wt/logs/q8-gd-23-compare/CHANGED.md`）：

1. 条件组「没生效」那一对的画面：**甲，1280×1080**，断言框下那一行计数与规则原文整个露在左栏里。
2. 没填的那一格指针停上去：**甲，换全仓输入框悬停那一档的实线**，不保持虚线。
3. 下拉展开后的选项：**甲，照全仓用正文 13 号**，只有收起那一格是 11.5。
4. 挂单 `Q1867`（「加入子库」那一层把写错的子句也算成「未填写」）：**并进票 `gui-draws-the-rest-of-the-design/08`**，编排者在那张票上追加「收挂单 `Q1867`」。
