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

**Status:** ready-for-agent

- [ ] 连识别都没跑过的变体印**还没识别**，跑过了却一条候选都没有的印**没有候选**，
      两者在同一张表上并存也分得开（一条测试同时造出这两种行）
- [ ] `StateFilter` 的筛选器往返照旧：`from_label(label(x)) == Some(x)` 五档全过
- [ ] 五屏（**库 / 浏览 / 待确认 / 子库 / 任务**）里印这一档的地方一处不漏
- [ ] 报告与队列里的 `not_run` 口径一个字没变，那张加得起来的表照旧加得起来
- [ ] `spec.md` 那句「高 / 中 / 低 / 还没识别」跟着改
- [ ] `crates/` 里指向挂单 `Q77`/`Q80`/`Q84` 的四处文档改成指向词表（挂单 `Q148`）
- [ ] 挂单 `Q146`、`Q148` 标为 resolved
- [ ] 门禁命令全绿（带 `--all-features`）
