# 13: 屏上剩下的终端命令收干净——核心那几句去命令，界面自己写死的三句先答「界面上有没有这件事」

**What to build:** 票 06 收了它点名的那一族之后，「核心库的话里带着终端命令、界面原样画出来」还活在几处（挂单 `Q1349`）。照 06 的形状收：

- **核心库三句**：话只说事实与去处的名字，原因交成种类，命令行在它那一层补命令，界面补屏上的去处：
  - `catalog/browse.rs` 的 `WorkVariant::no_candidate_hint`（「先跑一趟 `romcat identify`」），浏览屏与作品详情画它。
  - `capability.rs` 的 `decide` 那句理由（「`romcat scan --zst`」「`romcat report`」），画在子库屏差量预览「目标吃不下」那几行的悬停里，命令行报告逐条印它。要交种类，就得改 `Decision::Unsupported` 与计划里那一列。
  - `identify.rs` 的 `collect` 那句穿不透的理由（「`romcat scan --zst`」）。
- **界面自己写死的三句**：库屏 `roots.rs` 的 `PICK_ROOT_NONE`（`romcat scan`）、子库屏 `sublibrary.rs` 约 2848 行（`romcat sublibrary rule <子库> --remove <序号>`）、浏览屏 `browse/suspicion.rs` 约 147 行（`romcat triage same-work … --undo`）。
- **每一句先答「界面上有没有这件事」**（ADR-0023 修订段：「这件事界面做不到，人得回终端」本身就是缺陷）：
  - 有落点的，指到屏上那一处。
  - 没有落点的，例如库屏扫描没有「读 zst」那一项，记挂单交拿主意的人裁：是界面补上这件事，还是这句话改说事实、不指路。不在界面上编造一个去处。

来源：拿主意的人 2026-10-04 在票 06 回报之后裁（挂单 `Q1349`）：另开一张票，这一轮做。

收挂单 `Q1349`：核心那三句去命令、交种类，界面写死的三句逐句答落点。

规格：`../spec.md`；ADR-0005、ADR-0023（读到底）；票 06 的做法见 `06-core-refusals-name-facts-not-commands.md` 与它的提交。

⚠️ 浏览屏 `browse.rs` 一串票（gd 05–09、19、20）在改，本票只碰 `browse/suspicion.rs` 与核心那句；要动 `browse.rs` 本体，与那一串串着做。

**Blocked by:** 06

**Status:** done

- [x] 先写在今天 `main` 上红的测试：核心那三句的 `Display` 里没有 `romcat `；命令行在对应处照旧印得出那条命令（各一条）
  —— 三句各有核心一条、命令行一条，都在基点上红（红的样子与改法逐句写在下面三条里）：
  `crates/core/tests/works.rs::连识别都没跑过的与跑过了没候选的在同一张表上印两个词`（`crates/core/src/catalog/browse.rs` 的 `no_candidate_hint` 那一句：
  加了「不含 `romcat `、点出「先跑一趟识别」」两条断言，基点上红——那一句是「…先跑一趟 `romcat identify`。」）；
  `crates/core/src/capability.rs::tests::转不了的理由只说事实_不带命令行命令_是哪一种交成种类`
  （`Decision::Unsupported.why` 改成 `UnsupportedReason`，这一条断言那种是 `ContentsUnknown`、`Display` 不含 `romcat `；基点上**编不过**——
  `why` 是 `String`，那句里带着「（zst 要 `romcat scan --zst`）」与「`romcat report` 说得出它卡在哪一类」）；
  `crates/core/tests/model_inference.rs::没读_zst_那句理由只说事实_不带命令行命令`
  （`crates/core/src/identify.rs` 的 `collect` 那句：断言理由含 `identify::NOT_READ_NOTE`、不含 `romcat `；基点上红——那一句缀着「（`romcat scan --zst`）」）。
  同族顺带改了断言的三条：`crates/core/src/capability.rs` 的 `::rar_几乎无人支持_而这一版转不了它_于是如实报出来`、`::穿不透的容器不会被说成不是容器`、
  `crates/core/tests/sync_run.rs::多碟变体…`（`那一行.why` 不含命令）。
  命令行那三条（跑二进制）：`crates/cli/tests/sync.rs::差量预览里转不了的容器_核心那句只说事实_命令行补上读一趟与看卡在哪一类的命令`
  （`romcat scan --zst` 与 `romcat report` 都在输出里，而核心那句所在那一行不含 `romcat `）；
  `crates/cli/tests/identify.rs::穿不透的容器那句理由只说事实_命令行补上读一趟那条命令`（报告里那句事实不含命令，stderr 里补上 `romcat scan --zst`）。
  补命令的两处：`crates/cli/src/main.rs` 的 `warn_about`（`plan` 与 `sync` 共用的那一处）与 `run_identify`。
- [x] 界面上画这几句的地方（浏览屏 / 详情的没候选提示、差量预览吃不下那几行的悬停、库屏选根、子库规则移除、同作品撤销）屏上没有 `romcat `，有落点的指到屏上那一处（各一条测试）
  —— 六处各一条：`crates/gui/tests/browse.rs::浏览屏把还没识别与没有候选印成两个词`（侧边详情那一栏里那句照核心库印、整栏不含 `romcat `）；
  `crates/gui/tests/work.rs::识别依据那一面照稿摆候选表与判定依据`（作品详情页那张卡上那句同上）；
  `crates/gui/tests/sublibrary.rs::目标吃不下而转不了那几行的悬停只说事实_不带终端命令也不编一个去处`（悬停里只剩事实）；
  `crates/gui/tests/roots.rs::屏头添加根_取消或弹不出选择窗口时屏上说去哪儿办`（`PICK_ROOT_NONE` 换了字，屏上不含 `romcat scan`）；
  `crates/gui/tests/sublibrary.rs::读不懂的规则在改子库那条横幅里扔得掉而且别的一条都没动`（**有落点**：那一行自己就指着浏览屏筛选栏顶上那条横幅，
  悬停里那句「命令行那条路也还在：`romcat sublibrary rule …`」删掉）；
  `crates/gui/tests/suspicion.rs::不是同一个的悬停里不带终端命令_也不编一个撤回的去处`。
  **保留一处**：浏览屏侧边详情与作品详情页那句「还没识别」有落点（库屏工序段的「识别」那一行、队列屏的「运行识别」），
  但**没指过去**——要改 `browse.rs` 与 `browse/work.rs` 本体，与本票不碰的那一串票重叠，挂单 `Q1799`。
- [x] 没有落点的每一句都记了挂单（谁来裁：拿主意的人），写清界面上缺的是哪件事
  —— 四条，句句写清界面上缺的那件事：`Q1797`（库屏的扫描没有「读 zst」这一项——`capability::decide` 与 `identify::collect` 里那两个 `romcat scan --zst`）、
  `Q1798`（库体检屏没有「透明容器：内部构成」那一节——`romcat report` 那句的去处）、
  `Q1800`（选择窗口弹不出来时，界面上没有第二个加根的入口——`PICK_ROOT_NONE`）、
  `Q1801`（记过「不是同一个」之后，界面上没有撤回它的地方——`suspicion.rs` 那句）。
  另记两条路过发现的：`Q1799`（上一条那处指路）、`Q1802`（识别候选的「依据」里还剩一句 `romcat switch sync`）。
- [x] 全仓 `crates/gui/src` 与核心交给界面的话里再搜不到 `romcat `（命令行与文档除外），结果写进证据
  —— `grep -rn "romcat " crates/gui/src` 还有 44 处，**逐处看过**：去掉注释与字面量之后（`crates/gui/tests/shared/source.rs` 那把刀同一个口径）
  只剩 6 处、没有一处是终端命令——`crates/gui/src/layout.rs` 的 `HEADER`（写进版式偏好文件开头的那段**说明**）、
  `crates/gui/src/recent.rs` 的 `HEADER`（同上，记着上次开的是哪一份库）、`crates/gui/src/main.rs` 的 `about`（程序名「romcat 界面：…」）、
  `crates/gui/src/app.rs` 与 `crates/gui/src/program.rs` 的窗口标题（「romcat — 开场」）。**界面源码里的字符串字面量里再也没有一条命令**
  （改之前那三条正是 `roots.rs` 的 `PICK_ROOT_NONE`、`sublibrary.rs` 的规则移除悬停、`browse/suspicion.rs` 的撤销悬停）。
  核心那一侧同样按这个口径扫了 `crates/core/src`：剩下的 `romcat ` 全在命令行的文本报告里（`identify/report.rs`、`sublibrary/report.rs`、
  `dat/report.rs`、`capability/report.rs`、`triage/report.rs`、`scrape/report.rs`、`report/render.rs`、`sync/report.rs`）、
  命令行的错误与 `--help` 里（`site.rs`、`catalog/roots.rs`——界面两处都不读它们），以及文件开头的说明（`scrape/online/account.rs`、
  `scrape/priority.rs`）。**还画在屏上的一句**：`crates/core/src/identify/switch.rs` 的候选依据里「`romcat switch sync` 取一次」——
  它不是拒绝话，是候选的依据（`Candidate.evidence` 是一段自由文本，全仓十几处拼它），不在本票点名的三句里，挂单 `Q1802`。
- [x] 受影响的截图基线（若有）重批；每张新截图基线先由编排者对稿自审，过关的再交拿主意的人点头
  —— **一张都没变**：`CARGO_INCREMENTAL=0 cargo test -p romcat-gui --all-features --test snapshot`（不带 `UPDATE_SNAPSHOTS`）
  159 条全过、0 条红，于是没有重批、没有对照材料（`/Users/nicoer/dev/game-wt/logs/cao-13-compare/` 不必建）。
  原因：变的四处有三处只在悬停里（差量预览吃不下那几行、规则移除、不是同一个），截图拍不到；
  `PICK_ROOT_NONE` 只在选目录窗口什么都没交回来时出现，截图里没有那一态；`no_candidate_hint` 那一句要「一条候选都没有」才画，
  截图夹具里没有那一态（159 条基线一张没动就是这一条的实测）。
- [x] 门禁全绿
  —— `CARGO_INCREMENTAL=0 cargo xtask gate --keep-going -j 2 --test-threads 2`，日志
  `/Users/nicoer/dev/game-wt/logs/cao-13-gate1.log`（开头两行是 `/Users/nicoer/dev/game-wt/slot2`、`q8/cao-13`），末行 `EXIT=0`，
  「── 门禁 ──」底下七条逐条绿（原文见文末「门禁」一节）；`test` 那一条 92 个测试目标全 `test result: ok`、红 0。
  收尾时 `git merge q8/cao-int` 是 **no-op**（`Already up to date.`，集成分支还在 `186fbfd`），于是没有再跑第二趟。

收挂单 `Q1349`：票 06 那一族之外的三句核心话去命令、原因交成种类，界面三句逐句答了落点——`../PARKING-LOT.md` 里 `Q1349` 底下补了
「票 `core-answers-once/13` 做掉了什么」一段。本票新记挂单 `Q1797`–`Q1802`。

## 做了什么

**核心库三句（事实留下，命令与去处交出去）：**

- `crates/core/src/catalog/browse.rs` 的 `WorkVariant::no_candidate_hint`：那句「…先跑一趟 `romcat identify`。」改成「…先跑一趟识别。」——
  只说事实、点出下一步那件**事**的名字；去哪一处按由界面补（`Q1799`）。
- `crates/core/src/capability.rs`：`Decision::Unsupported.why` 从 `String` 换成新立的 `UnsupportedReason`（十种，`Display` 只说事实），
  `Decision` 与计划里那一列（`crates/core/src/sync.rs` 的 `Unsupported.why`）都跟着交种类；`--json` 里那格照旧是一句话
  （手写的 `Serialize` 写的是 `Display` 那一份，挂单 `Q1347` 的口径不变）。`ContentsUnknown` 这一种是唯一要两个壳各补去处的。
- `crates/core/src/identify.rs` 的 `collect`：那句穿不透的理由去掉「（`romcat scan --zst`）」，那一截收成公开常量 `identify::NOT_READ_NOTE`
  ——命令行认的是**同一个常量**，不是一句话的字（ADR-0024）。

**命令行补命令：**

- `crates/cli/src/main.rs` 的 `warn_about`（`sublibrary plan` 与 `sync` 共用）：计划里有一条 `ContentsUnknown` 时补一句
  「`romcat scan --zst` 读一趟……；卡在哪一类，`romcat report` 说得出」。
- `run_identify`：识别报告的「无判据」里有 `NOT_READ_NOTE` 那一类时补一句「`romcat scan --zst` 读一趟……」。

**界面自己写死的三句（逐句答落点）：**

| 句子 | 界面上有没有这件事 | 这一票怎么改 |
|---|---|---|
| `crates/gui/src/sublibrary.rs` 的规则移除悬停 | **有**：浏览屏筛选栏顶上那条横幅（同一行自己就指着） | 悬停里那句命令行指路删掉，去处仍由上面那一行说 |
| `crates/gui/src/roots.rs` 的 `PICK_ROOT_NONE` | **没有**：加根只有「添加根…」这一颗，它正是弹不出来的那一下 | 只说事实（「这一下就加不了这个根」），不编去处——挂单 `Q1800` |
| `crates/gui/src/browse/suspicion.rs` 的「不是同一个」悬停 | **没有**：记过的那些只在体检那一趟被读回来，谁也不列出来 | 只说事实（「撤销也不在裁决记录里」），不编去处——挂单 `Q1801` |

**顺带改了口径的两处界面话**：子库屏差量预览「目标吃不下」那几行的悬停改印 `row.why.to_string()`（`UnsupportedReason` 的事实）；
`crates/gui/src/sublibrary.rs` 里读不懂的规则那一行不再挂悬停。

## 门禁

日志 `/Users/nicoer/dev/game-wt/logs/cao-13-gate1.log`，开头两行：

```
/Users/nicoer/dev/game-wt/slot2
q8/cao-13
```

末行与「── 门禁 ──」底下那七条（原文照抄）：

```
── 门禁 ──
绿  fmt        2s  cargo fmt --all --check
绿  glossary    3s  cargo xtask glossary
绿  check     21s  cargo check --workspace --all-targets -j 2
绿  clippy    25s  cargo clippy --workspace --all-targets --all-features -j 2
绿  test    1046s  cargo test --workspace --all-features --no-fail-fast -j 2 -- --test-threads=2
绿  numbers    3s  cargo xtask numbers --check
绿  doc        9s  RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --lib --bins -j 2
7 条全绿。
EXIT=0
```

`numbers` 那一条印的是 `测试条数=3,005 测试目标数=92 票数=204/223 ADR份数=24`；README 里那两个数由
`cargo xtask numbers --write` 写回（2,999 → 3,005、203 → 204）。

收尾时把集成分支合进来：`git merge q8/cao-int` → `Already up to date.`（`q8/cao-int` 还在 `186fbfd`，与本分支的基点同一个），
于是**没有再跑第二趟**——合并没有带进任何改动。
