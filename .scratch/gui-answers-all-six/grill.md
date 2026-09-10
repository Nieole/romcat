# 六步都在界面上答完

**Status:** 待 `/to-spec`
**Opened:** 2026-09-09（`/settle` 第四轮收口 → `/grill-with-docs`）
**判据：** ADR-0023「修订：给「自足」一条判据」；词表词头**自足**（`CONTEXT.md`「工具自己的状态」）

## 它建什么

主干**六步**每一步在界面上都有落点。眼下九张票（`gui-self-sufficient`）之后仍有七处
「就在这台机器上、就现在、就这一份库」的动作**界面答不出、人得回终端**。这一批把它们补齐。

判据一句话：**「这件事界面做不到，人得回终端」本身就是一条缺陷，不是一次取舍。**
上一轮这七处每一处都被判成「不在这张票的范围内」，一次都没被判成「违背 ADR-0023」——
判据就是为治这个加的。

## 七件（形状已裁定的直接照写）

### 1. 识别用得上库里已经付过钱的答案 —— 挂单 `Q418`

界面上 `crates/gui/src/stages.rs:516` 一句 `Guessing::off()` 把整层关掉，**连中立库里
`model_answer` 表存着的答案也不用**。那张表是刻意不清的（`catalog/identify.rs:2830`：
「那是唯一花过钱的一张表」），重扫、重跑识别都不动它。人要捡回自己付过的钱，得回终端
跑一趟 `romcat identify`。

**形状已裁定（2026-09-09）：四行，不动核心库。**
`Guessing` 是 `pub struct` 且八个字段全 `pub`（`crates/core/src/identify/model.rs:1025`），
命令行本来就是字面量建的（`crates/cli/src/main.rs:1837`）。照它建：
`answers: &Answers::build(catalog.model_answers()?)`、`net: None`、
**`price: Price { 0, 0 }`、`announce: None`、`planning: false` 三样一起钉死**。

⚠️ **当初否掉这一条的理由不成立，别照抄。** 挂单里写的是「0 价算出来的计划会印
『花费 0.00 美元』」——命令行那条拒绝在 `main.rs:1795`，它拦的是**印一份计划**；
界面这一路 `announce: None`、`planning: false`，**从不印计划**，于是 0 价永远不会被看见。
价目表在这条路上是白背的（核心库自己从不读 `pricing.toml`，`Guessing.price` 是外面塞进来的）。
日后真要开推断，那时再装价目表，那时它才有意义。

缓存命中在主循环里、与网络无关（`crates/core/src/identify.rs:3071`–`3113`）；
`ask_model` 一见 `net: None` 就在 `identify.rs:576` 返回，一个请求都不发。

### 2. 导出撞上「外面有人动过」时给一条出路 —— 挂单 `Q441`

`crates/gui/src/stages.rs::export_run` 把 `ExportOptions { dry_run: false, force: false }`
钉死。`force: false` **是对的**（票面第 5 条逐字要求不静默覆盖），但界面上眼下**没有**
「我看过了，照写」这条出路——人得回命令行跑 `romcat export --force`。

**形状已裁定（2026-09-09）：裸出路 + 逐份点名，不做差量预览。**
撞上时列出「这几份文件外面被动过」，配一颗「我看过了，照写」。
`transfer::export_task` 本来就一份文件一份文件地写、每写完存一份底本（`put_snapshot`），
**点名是白拿的**；给人看清「差在哪儿」是差量预览那一套的活，另一张票的体量。

### 3. 撤更早的那一批裁决 —— 挂单 `Q448` ①

`crates/gui/src/queue.rs:1753` 的 `undo_last` 只认 `self.applied`（`queue.rs:152`，
**一个 `Option`，不是 `Vec`**）——也就是本次进程里刚落下的那一批。更早的要回终端。

**核心库那一层早就通了**：`triage::Queue::undo(catalog, store, library, batch: i64)`
（`crates/core/src/triage/queue.rs:449`）与 `triage::undo_batch` **本来就吃任意批号**，
沉淀库 `verdict_batch` + `verdict_batch_row` 存着每条的 `after`/`before`
（`crates/core/src/verdict.rs:245`–`279`），撤一批不删行、只打 `undone_at`；
还自带一条守卫——被后来还在册的批盖住就整份拒（`Store::batch_covering`，
`verdict.rs:1299`，`TriageError::CoveredBy`）。**缺的只有界面从不列批**：
`store.batches()` 全仓只有命令行调（`crates/cli/src/main.rs:3664`、`3790`）。

所以这件「新功能」实际是**一屏列表加一个批号**，不是新能力。

⚠️ **命名陷阱，动手前先读词表**：`CONTEXT.md` 词条**批**已经把两个意思分开了——
「一批变体」（待确认屏一级分批）与「一批裁决」（撤销的粒度）。而代码里
`Queue::batches()`（`crates/core/src/triage/queue.rs:254`）是**前者**、
`store.batches()` 是**后者**，两个同名方法在同一屏上。别再加第三个 `batches`。

### 4. 识别的断点 —— 挂单 `Q420`

`crates/core/src/identify.rs::run` 起手一句 `catalog.clear_identifications()`——
按停之后下一趟从头再算一遍。扫描有**断点**、同步有**清单**，识别两样都没有。
屏上那句话如实说了（不骗人），但能力缺口是真的。

**形状已裁定（2026-09-09）：只做「下一趟不重算已经算完的」，不做通用续跑。**
把那句全清改成只清这一趟要重算的那些即可；**不做断点文件**（那是扫描那一套的体量）。

### 5. 刮削收进工序段 —— 挂单 `Q447`

**刮削是主干六步之一**（词表**工序**），而 `Stage::ALL` 只有识别／折标题／导出三支
（`crates/core/src/stage.rs:121`）。票 `09` 只改了文案（三处指向浏览屏那颗「刮削选中…」），
按钮一颗没加——它的硬约束逐字禁掉了「加第四支」与「第二份实现」。

**口径已裁定（2026-09-09）：那一行数「一条刮削结论都没有的变体」，并在屏上明写这个口径。**
一句 SQL（`Catalog::scraped_subjects()` 反减，`crates/core/src/catalog/scrape.rs:908`），便宜。
屏上那句话要明写「这一行数的是一条刮削结论都没有的变体，**不是**按当前旋钮还差多少」。

⚠️ **为什么不按旋钮算**：真判据是逐（锚点 × 源）比**输入指纹**，而指纹把「这一趟要哪些
字段」折进去了（`crates/core/src/scrape.rs:880`、`894`）——**同一个变体在窄字段那一趟算
「刮过」、宽字段那一趟算「没刮过」**。按旋钮算的那个数是刮削面板 `estimate` 那本账的事
（`crates/core/src/scrape/estimate.rs`），**两本账别混**。

⚠️ 加一支 `Stage` 要改**五处、两处不在 core**，清单逐条写在 `crates/core/src/stage.rs:38`–`48`。

### 6. 扫描完屏头那个数别过期 —— 挂单 `Q419`

`crates/gui/src/app.rs::poll_tasks` 只在识别跑完之后重列队列。扫描确实不改变队列本身
（那些变体连结论都还没有，进不了队列），但**队列屏屏头那个「还没识别 N 个」会过期**
——屏上那个数会说错话，撞规格第 31 条「至少它不骗我」。

判据参考：识别那个数的便宜出处已经有了，而且三处共用一处——`Catalog::not_run_count()`
（`crates/core/src/catalog/identify.rs:2354`，一句 SQL 一次 `query_row`），队列屏、
命令行队列报告、工序段都取它。别另造一份数。

### 7. 目录选择器 —— 挂单 `Q387`

开场换工作目录（`crates/gui/src/opening.rs::workspace_ui`）与向导选第一个根，眼下都只能
把绝对路径手打进一个单行框。**引 `rfd`，两处都换成系统目录选择器**（2026-09-09 裁定）。
仓库眼下 `rfd` 零出现，这是引一个新依赖。

## 不在这一批里

- **`Q369`（`romcat scan` 不印主库名）出局**——判据说那是命令行自己的输出，不是界面欠的。
  **已于 2026-09-09 就地做掉**（`crates/cli/src/main.rs` 加一行 + `crates/cli/tests/library_name.rs::扫描那一趟也印得出主库名`）。
- **差量预览**：`Q441` 明确不做它。

## 开票时

按 `docs/agents/issue-tracker.md`「挂单托给一张票时」那条规矩，每张票正文里写清
`收挂单 Qn`，收尾时和验收条目一起勾。
