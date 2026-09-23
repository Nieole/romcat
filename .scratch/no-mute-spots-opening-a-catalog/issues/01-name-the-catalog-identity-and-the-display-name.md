# 01 —（prefactor）`主库标识` 与 `主库原名` 正名

**What to build:** 读代码的人不必再猜「主库名」指的是哪一个。公开面上那个**名不副实**的
"主库名"——它其实是中立库的主文件名、也是**路径锚**里那个键——改成与词表 **主库标识** 同名；
给人看的那个照旧是 **主库原名**。

收挂单 `Q372`：公开面上并存两个「主库名」，只有一个是标识符。
—— **已收（2026-09-13，提交 `68c76d6`）**：`Site::library` 改叫 `Site::library_identity`，与 `Catalog::library_name`（主库原名）两处文档互指；正文在 `.scratch/gui-self-sufficient/issues/01-library-name-lands-in-the-catalog.md` 的 `### Q372`，那份历史记录不改。字段名没照 grill 那句 `Site::slug`，见挂单 `Q452`（等拿主意的人裁）。

**Blocked by:** 无 —— 可立即开工

**Status:** done

⚠️ **只改叫法，不动锚。** **路径锚里存的键一个字节不动**——它存的本来就是这个标识，
语义没变、只是名字对上了。真要动锚是另一件事，而且那件事至今没有理由。

⚠️ **这是 prefactor。** 票 `02` 与 `03` 要往这片地上加东西（元数据键的具名函数、建库入口），
**先正名，它们才不会用着混淆的词造出新的**——否则那些新函数要被改两次名。

⚠️ **59 处调用点，分布在三个 crate**（约各二十处）。**够不上「宽重构」**——一次机械改名就完，
不必 expand–contract。**编译器就是这一件的测试，不新增测试。**

⚠️ **词表那两条词头已经落笔**（**主库标识** / **主库原名**，两条互指）。代码里之所以并存两个
"主库名"，根子是词表从没给这两样各起一个名字——改完之后词表、类型名、文档是同一套说法。

- [x] 公开面上那个标识改成与词表 **主库标识** 同名，59 处调用点全部跟上
      —— `crates/core/src/site.rs` `Site::library` → `Site::library_identity`（连 `in_memory` / `at` / `open_file` 里的同名参数与局部量）。**实测调用点 25 处，不是 59**：命令行 8、核心库 2、界面源码 10、界面测试 5，外加 `site.rs` 自己 3 处；「59」是 `grep '\.library\b'` 的原始行数，里面混着 `args.library`（`--library` 参数）、`LibraryFs` 字段、verdict 行字段这些不相干的名字。编译器就是测试：门禁 check / clippy 两步绿。**保留**：英文名取 `library_identity` 而不是 grill 写的 `slug`（词表**主库标识**条 `_Avoid_` 列着 `slug`），挂单 `Q452` 等拿主意的人裁，要翻得在合进 `main` 之前翻。
- [x] 给人看的那个照旧叫 **主库原名**，两处文档互指
      —— `Catalog::library_name` 名字不动，文档开头写明它是**主库原名**，并有一节「它**不是** [`Site::library_identity`]」；`Site::library_identity` 的文档指回 `Catalog::library_name`；`Site::display_name` 那一节「它**不是** [`Self::library_identity`]」同样跟上。三处都是 rustdoc 链接，门禁 doc 步（`-D warnings`）绿，说明链接都解析得到。词表那两条词头本来就互指。
- [x] **路径锚里存的键一个字节没动**（有测试反向钉住）
      —— `crates/core/tests/library_name.rs::路径锚里存的主库标识一个字节没动`：字面量 `"主库-f5c61109e92b6036"` 取自改名前的代码（不是照算法重算的）；按名字（`Site::open`）和按文件（`Site::open_file`）两个入口开现场，断言标识逐字节等于它，而且沉淀库里预先按这串字节落的路径锚能被 `Store::joined` 认出。改名前后都绿；把 `Site::at` 变异成只存可读的一半时变红（left `"主库"`，right 那串字面量），还原后 `cmp` 逐字节一致。沉淀库那几列 `library` 没动。规格测试表写着「不新增测试」，票面这一条要求「有测试反向钉住」，按编排者的指示补了这一条。
- [x] 全仓门禁绿，行为一处没变
      —— 门禁 `cargo xtask gate -j 3 --test-threads 3 --keep-going`，日志 `/Users/nicoer/dev/game-wt/logs/slot-1-gate.log`，`EXIT=1`：fmt / check / clippy / doc 绿；test 里 romcat-core lib 969 过 1 挂，**唯一红的是既有的、与盘大小写有关的** `testing::target::tests::分大小写那一档上_两个只差大小写的目录照旧是两个`（在临时目录里真建 `GB/` 与 `gb/`，这台 Mac 的 APFS 不分大小写，两个并成一个；本票没碰 `crates/core/src/testing/`；Linux CI 上是绿的），编排者在 `main` 上另修。**保留**：cargo test 在那个 lib 二进制上就停了，排在它后面的核心库集成测试、界面测试、xtask 测试没在这趟门禁里跑到。改动涉及的几份在最后一次改动后单独跑过，全绿（`/Users/nicoer/dev/game-wt/logs/slot-1-targeted-tests-2.log`，`EXIT=0`）：核心库 `library_name` / `collection` / `stage` / `triage` / `catalog_list`、核心库 `site::` 单元测试、命令行 `library_name`、界面 `program` / `queue`（`--all-features`）。行为：diff 只有改名和注释；屏上的字与命令行输出一个字没改（`Q451`、`Q454` 刻意留着）。
- [x] 全仓 `grep` 不到第三种叫法
      —— **带保留勾。** `.scratch/` 以外，代码注释、测试名、`docs/adr/0020`、`docs/adr/0023` 里的「主库名」「在路径锚里叫什么名字」「库名」都按所指换成了**主库标识**或**主库原名**（改过名的五个测试函数见挂单 `Q453`）。grep「主库名」还剩：`crates/gui/src/claim.rs:37` 屏上提示字，以及 `crates/gui/tests/program.rs` 里五处按这串字找框的 `打字`（改它就是改行为，与上一条相悖——挂单 `Q451`，已托给票 `gui-looks-like-the-design/03` 的正文）；`CONTEXT.md` 的 `_Avoid_` 那一行自己；`crates/core/src/catalog/export.rs:14-15` 说明引文里换过词的那句注释。Rust 名字里仍装着主库标识的：`Anchor::Path::library`、`Index::load` 等的 `library` 参数、`Decide::library`、`crates/core/tests/rescan.rs` 的 `const 库名`（挂单 `Q449`）；主库原名一侧的 `display_name` / `library_label`（挂单 `Q450`）。命令行把标识印给人看的两句见 `Q454`。

## 挂单裁决（第五轮收口，2026-09-23）

从 `.scratch/PARKING-LOT.md` 迁来（`/settle`，范围 `Q449`–`Q1185`）。每条顶上多一行**裁决**：落哪一档、去了哪儿。
编号 `Qn` 留着占号、不复用。

### Q449 — 正名只改到 `Site` 那一个字段，别处装着主库标识的 `library` 名字没跟

- **裁决（第五轮收口，2026-09-23）：** **岔** —— 实现走的就是条目建议的那条；追认：维持现状（2026-09-23 拿主意的人整批追认）。

- **来自：** 票 `no-mute-spots-opening-a-catalog/01`
- **类别：** 规格没说
- **在哪：** `crates/core/src/verdict.rs:405` `Anchor::Path::library`、`:1969` `Index::load` 与 `:2059` `MatchIndex::load` 的 `library` 参数；`crates/core/src/triage.rs:829` `Decide::library` 与 `:996` 计划上的 `library`；`crates/core/src/collection.rs` `anchor_of` / `plan` 的 `library` 参数；`crates/core/src/workspace.rs` `checkpoint_path_of` 的 `library` 参数；`crates/gui/src/demo.rs` `LIBRARY`；`crates/core/tests/rescan.rs:38` `const 库名`（同一个值兼作根名）
- **为什么没停线：** 撞的是名字不是行为，两条路都不动锚。
- **这张票实际做了什么：** 只改 `Site::library` → `Site::library_identity`，连带 `Site` 自己几条构造路径上的同名参数与局部量、直接从它取值的三个局部量（`collection.rs` `apply`、`gui/src/browse.rs`、`gui/tests/queue.rs`）、三份测试里只装主库标识的 `const 库名`（`collection` / `stage` / `triage`）。上面列的名字原样留着，只把它们文档里的「主库名」「那个名字」改成**主库标识**。
- **另一条路：** 追到底，凡装主库标识的 Rust 名字都叫 `library_identity`；沉淀库那几列 `library` 照旧（库列名改名是破坏性变更，`CONTEXT.md`「三类不算撞」第 2 条）。
- **建议留哪条：** 留这条。`Anchor::Path::library` 那一族与沉淀库的列同名，读得通；追到底要动 `verdict.rs` 近百处加 `triage.rs` 与界面层，而这张 prefactor 是给票 `02` / `03` 让路的，diff 越宽越撞车。`Q372` 点名的也只有 `Site::library`。`rescan.rs` 那个常量兼作根名，拆开是重构不是改名。
- **谁来裁：** 拿主意的人
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q450 — 主库原名那一侧还有 `display_name` / `library_label` 两个名字，而词表 `_Avoid_` 列着「显示名」

- **裁决（第五轮收口，2026-09-23）：** **岔** —— 实现走的就是条目建议的那条；追认：维持现状（2026-09-23 拿主意的人整批追认）。

- **来自：** 票 `no-mute-spots-opening-a-catalog/01`
- **类别：** 两份东西矛盾
- **在哪：** `crates/core/src/site.rs` `Site::display_name`；`crates/core/src/workspace.rs` `Slug::display_name`；`crates/gui/src/app.rs` `App::library_label` / `App::set_library_label`
- **为什么没停线：** 票面说给人看的那个「照旧」；它们交出的就是主库原名，撞的是名字不是行为。
- **这张票实际做了什么：** 名字不动；`Site::display_name` 与 `App` 那几处的文档改成说它交出的是**主库原名**（合成数据那一路另给一段名字）。
- **另一条路：** 一并改成 `library_name`，于是 grep `library_name` 就是主库原名在代码里的全部。
- **建议留哪条：** 留这条。`Site::display_name` 比 `Catalog::library_name` 多一层内存库的退路，同名会让人以为是同一个函数；`App::library_label` 在合成数据那一路装的是「合成数据（演示）」，根本不是原名，叫 `library_name` 反而名不副实。
- **谁来裁：** 拿主意的人
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q451 — 认领向导起名框的提示字仍是「主库名，例如「主库」」

- **裁决（第五轮收口，2026-09-23）：** **记** —— 已由票 03 改掉提示字并落地。已落地：票 gui-looks-like-the-design/03（claim.rs NAME_HINT 已是「主库原名，例如「主库」」）

- **来自：** 票 `no-mute-spots-opening-a-catalog/01`
- **类别：** 路过发现，不在范围内
- **在哪：** `crates/gui/src/claim.rs:37` `NAME_HINT`；`crates/gui/tests/program.rs` 五处 `打字(.., "主库名，例如", ..)` 靠这串字找框；「主库名」在词表**主库标识**条的 `_Avoid_` 里
- **为什么没停线：** 那是屏上的字，改它就是改行为，与本票「行为一处没变」相悖。
- **这张票实际做了什么：** 没动；往票 `gui-looks-like-the-design/03`（界面上的说法与词表对齐，它本来就要把「认领新主库」改成「添加主库」）正文里追加了「收挂单 `Q451`」。本票验收第 5 条因此带保留勾。
- **另一条路：** 单开一张票只改这一句。
- **建议留哪条：** 托给 `03`：同一个向导、同一类改动，分两张票会让同一屏的文案被改两趟。
- **谁来裁：** 拿主意的人
- **收掉（票 `gui-looks-like-the-design/03`）：** 提示字改成「主库原名，例如「主库」」，`crates/gui/tests/program.rs` 五处按它找框的 `打字` 跟着改。
- **状态：** settled

### Q452 — 字段叫 `library_identity`，没照 grill 那句 `Site::slug`

- **裁决（第五轮收口，2026-09-23）：** **记** —— 拿主意的人已裁留 library_identity 并落地。已落地：票 no-mute-spots-opening-a-catalog/01

- **来自：** 票 `no-mute-spots-opening-a-catalog/01`
- **类别：** 两份东西矛盾
- **在哪：** `.scratch/no-mute-spots-opening-a-catalog/grill.md` 逐件表 `Q372` 那一行与第 5 节（`Site::library` → `Site::slug`），对上 `CONTEXT.md` **主库标识**条 `_Avoid_: slug、库 ID、主库名` 与词表开头「是在给这个概念起名吗？是，就躲开」；落点 `crates/core/src/site.rs` `Site::library_identity`
- **为什么没停线：** 规格与票面写的都是「改成与词表**主库标识**同名」，不点 `slug`；岔只在这一个英文词上。
- **这张票实际做了什么：** 起名 `library_identity`：主库＝`library`（仓库现成的译法，`Catalog::library_name` 就是主库原名），标识＝`identity`（票文件名 `catalog-identity` 用的同一个词），与 `library_name` 两两对仗。`workspace::Slug` 这个类型照旧，算旧账——同 `gui/src/stages.rs` 里 `job_of` 那条注释：旧的 `Job` 不往新代码里扩。
- **另一条路：** 照 grill 叫 `Site::slug`：与 `Slug::text()` 同名、短，改名第一遍就是这么落的，改回去是一次机械替换。`library_id` 也想过，撞 `_Avoid_` 里的「库 ID」。
- **建议留哪条：** 留 `library_identity`。词表是决定、grill 是材料（`docs/agents/domain.md`）；`slug` 正是词表点名要躲的词，而这张票的全部意义就是让代码与词表同一套说法。**要翻就在合进 `main` 之前翻**：票 `02` / `03` 会在这个字段上加代码。
- **谁来裁：** 拿主意的人
- **裁决（2026-09-13，拿主意的人）：** 留 `library_identity`。grill 里那句 `Site::slug` 作废。
- **状态：** settled

### Q453 — 五个测试函数名跟着正名改了，已完成的票里当证据引的旧名从此 grep 不到

- **裁决（第五轮收口，2026-09-23）：** **记** —— 改名已做；留旧名违 domain.md，是稻草人。记下，无事可做

- **来自：** 票 `no-mute-spots-opening-a-catalog/01`
- **类别：** 规格没说
- **在哪：** `crates/cli/tests/library_name.rs` `报告印得出主库原名而不是那串带哈希的文件名` / `扫描那一趟也印得出主库原名`；`crates/core/tests/catalog_list.rs` `主库原名读不到时退回从文件名截既不空着也不是那串哈希`；`crates/gui/tests/program.rs` `起名那一步就拦下这个工作目录里已被占用的主库标识`；`crates/core/src/site.rs` `中立库的主文件名就是这份主库的主库标识`。引旧名的有 `.scratch/gui-self-sufficient/issues/01-*.md:32`、`03-*.md:28`、`05-*.md:30`
- **为什么没停线：** 票据正文是历史记录，按规矩不改；旧名在 git 历史里照样查得到。
- **这张票实际做了什么：** 改了测试名（`docs/agents/domain.md`：测试名用词表的词；本票验收第 5 条），旧票一个字没动。
- **另一条路：** 测试名留旧的「主库名」，旧票里的证据照旧 grep 得到，代价是验收第 5 条多几处例外。
- **建议留哪条：** 留改名。证据链靠的是提交，不是今天的文件名；留旧名等于把要消掉的那个叫法钉在测试名上。
- **谁来裁：** 收尾
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q454 — 命令行有两句把主库标识（带哈希）当成给人看的名字印出来

- **裁决（第五轮收口，2026-09-23）：** **活** —— 无家可归（全仓没有一张开着的票占这片地），进 `/grill-with-docs`，束「核心库该多交一样东西、或把一个判据收到一处」。

- **来自：** 票 `no-mute-spots-opening-a-catalog/01`
- **类别：** 路过发现，不在范围内
- **在哪：** `crates/cli/src/main.rs` 列落过的批（约 3673 行）与撤销／放回找批（约 3804 行）那两句「主库「{}」上还没有落过一批裁决。」，填的是 `site.library_identity`
- **为什么没停线：** 正名之前就这样（填的是 `site.library`）；改成印主库原名是改行为，本票不许。
- **这张票实际做了什么：** 没动。正名之后这两行一眼看得出印的是标识，而词表说它「不是给人看的名字」。
- **另一条路：** 当场改成 `site.display_name()`。
- **建议留哪条：** 该改成 `display_name()`，但不在这张 prefactor 里：命令行输出变了要有测试钉住，是一张行为票的活。
- **谁来裁：** 拿主意的人
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）