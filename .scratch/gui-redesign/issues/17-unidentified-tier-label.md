# 17 — 「还没识别」与「没有候选」在界面上分开印

**What to build:** 词表把「还没识别」这四个字裁成了两个词（票 `16`、挂单 `Q146`）：

- **还没识别** ＝ 一个变体**连识别都还没跑过**，库里连它的识别结论都没有；
- **没有候选** ＝ 识别**跑过了**，却一条候选都没有，那一格没有置信度可标。

界面上这两件事眼下印的是同一个词。把它们分开印。

**为什么这值一张票：** 这条界线正是**命中率的分母**那条界线（ADR-0002）。报告与队列已经
为它分出了 `not_run` 计数，核心里两件事的 Rust 名字也已经刻意分开
（`NOT_RUN_LABEL` vs `Tier::Unidentified`）；眼下只剩打给用户的那个词还并着，
而用户看见的正是那个词。

## 它不是换个字符串

`Tier::of` 收的是 `Option<Confidence>`，**`None` 同时装着这两件事**：一个变体连识别都没跑过，
和一个跑过了却一条候选都没有的变体，折出来都是 `Tier::Unidentified`。
`crates/core/tests/works.rs` 里那批散落的行就是拿 `confidence == None` 断言「还没识别」的。
所以**直接把 `Tier::Unidentified::label()` 换成「没有候选」是错的**——连识别都没跑过的变体
会被印成「没有候选」，当场违背词表新收的那条。

先让浏览与队列那一侧**分得出**这两件事（库里已经有那份账：
`Catalog::not_run_count`、`triage::report` 的 `not_run`、`triage::queue` 的「另有 N 个还没识别」），
再各印各的词。分法有两条路，实现者挑一条并在挂单里写清为什么：
给那一档加一支（`Tier` 变五档），或者让行里多带一个「跑过没跑过」的标记，`Tier` 不动。

## 动哪儿

- `crates/core/src/catalog/identify.rs`：`Tier::Unidentified` 那一档打给用户的词
  与它那段「与词表撞词、记在挂单 `Q84`」的文档。**`NOT_RUN_LABEL` 不动**——
  它就是词表那条**还没识别**，是对的。
- `crates/core/src/catalog/browse.rs`：`WorkRow::confidence_label`；
  以及 `StateFilter::{label, from_label}` —— ⚠️ `from_label` 拿的是一个**裸字符串字面量**
  「还没识别」，不是 `NOT_RUN_LABEL`，只改 `label()` 会让筛选器的往返**静默断掉**，
  而断了没有任何测试会红。
- `crates/gui/src/`：`table.rs`（主列表那一栏）、`queue.rs`、`look.rs`（`tier_label`）、
  `browse.rs:1870,1876,2133`（手写的三处）。连断言这几个字的测试一起改。
- `.scratch/gui-redesign/spec.md` 那句「置信度色条与置信度标签在五屏里含义一致：
  高 / 中 / 低 / 还没识别」——不改它，spec 与产品就各说一套。
- 顺带（挂单 `Q148`）：`crates/core/src/triage/batch.rs`、`crates/gui/src/queue.rs`、
  `crates/core/src/identify/report.rs`、`crates/core/src/catalog/identify.rs` 里四处
  「词表眼下没收，挂单 `Q77`/`Q80`/`Q84`」的文档，改成指向 `CONTEXT.md` 的对应词条。

**不许动的：** `NOT_RUN_LABEL` 与 `not_run` 那套计数、以及报告里
`变体 = 命中 + 未命中 + 无判据 + 跳过 + 还没识别` 那个加得起来的口径。主库只读（ADR-0004）。

**Blocked by:** 无 —— 可立即开工

**Status:** done

- [x] 连识别都没跑过的变体印**还没识别**，跑过了却一条候选都没有的印**没有候选**，
      两者在同一张表上并存也分得开（一条测试同时造出这两种行）
      —— 核心那一侧两条：
      `crates/core/tests/works.rs::连识别都没跑过的与跑过了没候选的在同一张表上印两个词`
      （同一页上两行，`confidence` 都是 `None`，一行 `identified == true` 印「没有候选」、
      另一行 `false` 印「还没识别」；同一条还顺着点开详情面板，断了
      `WorkVariant::confidence_label` 与 `no_candidate_hint` 两种分岔各说各的话）；
      `crates/core/tests/works.rs::一行底下只要还剩一个变体没跑过识别这一行就说还没识别`
      （一行是一批变体时折的方向——实测把 SQL 里的 `MIN` 换回 `MAX`，这一条当场红）。
      界面那一侧 `crates/gui/tests/browse.rs::浏览屏把还没识别与没有候选印成两个词`：
      三个变体的库，屏上断的是**整句**——「高置信｜…命中.zip」「没有候选｜…一条候选都没有.zip」
      「还没识别｜…还没轮到它.zip」（只断那几个字的话，左边筛选面板里那一档的名字就够让它通过）。
- [x] `StateFilter` 的筛选器往返照旧：`from_label(label(x)) == Some(x)` 五档全过
      —— `crates/core/tests/browse.rs::识别状态那五档印出去的词认得回来`：五档循环全过，
      另断 `StateFilter::Unidentified.label() == NOT_RUN_LABEL`、它 `!=` `Tier::Unidentified.label()`、
      认不出的字折成 `None`。`from_label` 里那个裸字面量已换成 `NOT_RUN_LABEL` 常量引用。
      ⚠️ **票正文那句「只改 `label()` 会让筛选器的往返静默断掉」是错的**：
      `StateFilter::from_label` **眼下一个生产调用方都没有**——筛选面板拿的是
      `StateFilter` 值本身（`romcat_gui::browse` 那一段直接比、直接赋回），一个字符串
      都不经手；真走字符串往返的是 `PlatformFilter::from_label`。换成常量仍然该做
      （那一对是公开 API，断了没有一条编译错误会说话），但**断了不会在屏上冒出来**，
      只有这条新测试守着。文档已按实情改写。
- [x] 五屏（**库 / 浏览 / 待确认 / 子库 / 任务**）里印这一档的地方一处不漏
      —— 真会印一个变体级结论的只有**浏览**与**待确认**两屏（`Tier` 的文档原话：库屏摆根与
      数据源、子库屏摆设备与差量、任务屏摆队列与历史，那三屏上没有变体级结论可标）。
      清点：浏览屏两处（`table.rs` 主列表那一栏走 `WorkRow::confidence_label`、
      `browse.rs::variant_row` 走 `WorkVariant::confidence_label`），待确认屏四处
      （屏头四档、卡片 `tier_label`、逐条那张表、候选那一栏）——后者全都是**跑过识别**的条目，
      不必分辨。判据：`grep -rn "Tier" crates/gui/src` 之后逐处核过，`crates/gui/src` 里
      再没有第三个印这一档的地方。另外详情面板里两句「识别结论：还没识别」的裸字面量
      也换成了 `NOT_RUN_LABEL`；变体行悬停里那句「接下来该干什么」也收进了核心库
      （`WorkVariant::no_candidate_hint`）——它与印哪个词是同一条判据的两面，
      分家写两处就会改一处、指错一处（ADR-0005）。
- [x] 报告与队列里的 `not_run` 口径一个字没变，那张加得起来的表照旧加得起来
      —— `not_run` 那一族代码**一行都没动**（只改了 `report.rs` 里 `not_run` 字段的一段文档，
      把它指向词表）。钉着它的两条测试照旧绿：
      `crates/core/tests/identify.rs::还没识别的变体照样占着变体总数与全部变体里那个分母`
      （含「变体 = 命中 + 未命中 + 无判据 + 跳过 + 还没识别」那一行加得起来）与
      `crates/core/tests/triage.rs::还没识别的变体不算待裁决但队列报得出有几个`。
- [x] `spec.md` 那句「高 / 中 / 低 / 还没识别」跟着改
      —— `.scratch/gui-redesign/spec.md:200` 改成「高置信 / 中置信 / 低置信 / **没有候选**」，
      **还没识别**另起一条（那一节每条一句话，塞成一长句会跟周围的密度对不上）。
- [x] `crates/` 里指向挂单 `Q77`/`Q80`/`Q84` 的四处文档改成指向词表（挂单 `Q148`）
      —— **实际是五处 src 加两处测试注释，而票与 `Q148` 都点错了一处**：
      `crates/core/src/triage/batch.rs` 本来就已经写着「词表**两个都收了**」，核过之后没动。
      详见挂单 `Q148` 的「收的时候实际改了几处」。
- [x] 挂单 `Q146`、`Q148` 标为 resolved —— `Q146` 见本文件末尾、`Q148` 见 `.scratch/PARKING-LOT.md`。
- [x] 门禁命令全绿（带 `--all-features`）—— 四条最后一行：
      `cargo fmt --all --check` exit 0；clippy `Finished dev profile`（一条 `duplicated attribute`
      告警，来自 `main` 上本来就有的重复 `#[test]`，见挂单 `Q191`）；
      `cargo test --no-run` 编译通过；一趟全量 **62 个 `test result: ok`、1,631 条通过、0 失败**
      （基线 1,628，本票新增 3 条）。
      ⚠️ **保留一条**：`cargo fmt --all` 顺手改了两个不属于本票的文件
      （`crates/core/src/scrape/zh.rs`、`crates/core/tests/sync_run.rs`）——它们在 `main` 上
      **本来就不 fmt-clean**（判据：`git show HEAD:<路径> | rustfmt --check` 两处都报 diff）。
      按协调者收尾指示里那句「上面那 17 个都是你的」一起提交了，只为让**提交出来的树**上
      第一条门禁也是 0；本票没有主动改过这两个文件一个字。`sync_run.rs` 里那条重复的
      `#[test]`（clippy 报 `duplicated attribute`）**没修**，它不是排版问题。记在挂单 `Q191`。


## 挂单裁决（第二轮）

从 `.scratch/PARKING-LOT.md` 迁来；编号 `Qn` 留着占号、不复用。

### Q146 — 「还没识别」裁成两个词：第二种用法叫「没有候选」，而五屏上仍印着旧词

- **来自：** 票 `gui-redesign/16`（接 `Q84`）
- **类别：** 两份东西矛盾
- **在哪：** `CONTEXT.md` 的**还没识别**与新收的**没有候选**两条；
  `crates/core/src/catalog/identify.rs` 的 `Tier::Unidentified::label()`（返回「还没识别」）
  与 `NOT_RUN_LABEL`（同样四个字）；`crates/core/src/catalog/browse.rs` 的
  `WorkRow::confidence_label` 与 `StateFilter::{label, from_label}`；印这一档的界面在
  `crates/gui/src/{table.rs,queue.rs,look.rs}` 与 `crates/gui/src/browse.rs:1870,1876,2133`。
  **票 16 的 `Q84` 给的是 `crates/gui/src/browse.rs:976,996`，那两处是 `load_detail` 与
  `status`，一个字都不印这一档**——本条按实际印的地方重记了一遍。
- **为什么没停线：** 票 16 把这一裁明写着交给实现者（`Q84`），而它的验收最后一条又明写
  「代码里一个名字都没改」——裁得动词表，动不了标签。
- **这张票实际做了什么：** **裁成两个词，不并成一个。** 词表里**还没识别**仍是「一个变体
  连识别都还没跑过」，另收一条**没有候选**＝「识别跑过了、却一条候选都没有」。
  并成一个的话，「跑过没跑过」这条界线就没词可说了，而它正是**命中率的分母**那条界线
  （ADR-0002 那套置信度与待确认队列的账，加上核心里 `not_run` 与 `Unidentified` 已经被
  刻意分开的 Rust 名字）；ADR-0021 立的正是「不可读是第三态、不许并进已变或已删」这条纪律
  ——识别这一侧不该反过来把两件事并回一个词。**代码一个字没动**：那一档打给用户的仍是
  「还没识别」，于是词表与界面眼下对不上。
- **它不是换个字符串那么简单：** `Tier::of` 收的是 `Option<Confidence>`，
  **`None` 同时装着「连识别都没跑过」与「跑过了、一条候选都没有」**——
  `crates/core/tests/works.rs` 里那批散落的行就是拿 `confidence == None` 断言
  「还没识别」的。直接把 `Tier::Unidentified::label()` 换成「没有候选」，
  连识别都没跑过的变体也会被印成「没有候选」，当场违背词表新收的那条。
  同一个坑还有一处：`StateFilter::from_label` 拿的是一个**裸字符串字面量**
  「还没识别」，不是 `NOT_RUN_LABEL`，换掉 `label()` 那一半而漏了它，筛选器的
  往返就静默断掉。
- **要收的话怎么收：** 票 `gui-redesign/17` —— 先让浏览与队列那一侧分得出这两件事
  （库里已经有 `not_run` 那份账），再各印各的词，五屏与 `spec.md` 一起改。
- **谁来裁：** 票 `gui-redesign/17`
- **状态：** resolved（票 `gui-redesign/17`）
- **收尾裁决（第二轮）：** resolved —— **由票 `gui-redesign/17` 承接**。票 17 开头就写着「词表把这四个字裁成了两个词（票 16、挂单 Q146）」，连坑都一样（`Tier::of` 收 `Option<Confidence>`、`None` 装着两件事）。
- **票 17 实际怎么收的：** **给行加一个「跑过没跑过」的标记，`Tier` 四档一动不动**
  （`WorkRow::identified`、`WorkVariant::state`；挑这条路的三条理由记在挂单 `Q190`）。
  五屏上两个词分开印：待确认屏那一半（屏头两句并排）另一条线已经做对，本票照它的形状
  办了浏览屏那一半——主列表那一栏与详情面板的变体行都先问一句「跑过没跑过」再挑词，
  而挑词这件事落在核心库里（ADR-0005）。`StateFilter::from_label` 那个裸字面量也换成了
  `NOT_RUN_LABEL`，往返有测试钉着。

## 挂单裁决（第三轮）

从 `.scratch/PARKING-LOT.md` 第三轮收口迁来。

### Q191 — `main` 上本来就有两处不 fmt-clean，其中一处是重复的 `#[test]`

- **来自：** 本票（路过发现，不在范围内）
- **在哪：** `crates/core/tests/sync_run.rs:694` —— `#[test]` 连着写了两遍，
  `cargo clippy` 为它报一条 `duplicated attribute`（warn 级，退出码仍是 0）；
  `crates/core/src/scrape/zh.rs:2363` —— 一句 `assert!` 没折行。两处都在合并提交
  `79b6bbe` 之后的 `main` 上，本票一个字都没主动改过。
- **本票当时走的路：** 跑了门禁要求的 `cargo fmt --all`，那两个文件的排版改动一起提交了
  ——不提交的话，绿的只有工作区、不是提交出来的树。**重复的 `#[test]` 没修**：
  那不是排版问题（`cargo fmt` 只删了中间那个空行，两个属性都还在），是个真缺陷。
- **收尾裁决（第三轮）：settled** —— 那条重复的 `#[test]` 已由提交 `bd4273d`
  「删掉合并时留下的重复 `#[test]`」做掉。独立探针：`crates/core/tests/sync_run.rs`
  现有 16 个 `#[test]`，扫一遍没有相邻重复。排版那两处随 `cargo fmt --all` 一并落定。

### Q192 — 两份界面测试各搭了一份几乎逐字相同的小库夹具，而 `shared/mod.rs` 就是为这个建的

- **来自：** 本票的收尾审查（路过发现，不在范围内）
- **在哪：** `crates/gui/tests/queue.rs::有一个连识别都没跑过的库` 与
  `crates/gui/tests/browse.rs::三档并排的库` —— 约 70 行里一字不差的部分有：同一句
  `roots::add_root`、同一个 `变体` 闭包、同一批 `Identification` 字段，连注释都一样。
- **本票当时走的路：** **没抽。** 抽的话得同时改 `queue.rs` 那一份，而那份是另一条线
  正在并发推的验收证据；本票的硬约束是「待确认屏那一半不许推翻」。只把自己那一份写干净了。
- **收尾裁决（第三轮）：settled** —— 由票 `parking-3/16` 做掉，走的正是本条留下的那条路
  （「把 `变体`/`候选`/`一份小库` 三个搭子搬进 `shared/mod.rs`，两处一起换过去」）。
  实物：`crates/gui/tests/shared/mod.rs` 里的 `档`（四档枚举）、`变体`、`候选`、`小库`；
  两处夹具各自缩到十来行，只剩「各落哪一档」「摆在哪几个平台上」「工作目录是哪个」三样不同。
  **断言一条没改**：`queue` 32 条、`browse` 41 条原样跑绿（`queue` 后来因本票之外的
  卡片悬停多了 1 条，共 33 条）。并的时候统一了两格原本不一致的夹具细节（候选的
  `member_key` 与 `platform`），记在挂单 `Q344`；共用模块整份压掉 `dead_code` 记在 `Q346`。
