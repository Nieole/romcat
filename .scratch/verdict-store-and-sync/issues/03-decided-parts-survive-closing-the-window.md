# 03: 就地裁一组的作用范围关窗不丢

**What to build:** 待确认屏上就地落下一部分（按某个轴裁掉其中一组）之后，关掉窗口再开：那几项仍标「已通过」，这一批的细分方式仍锁在当初那个轴上。
沉淀库的批表记下结构化的**作用范围**（形状、轴、组名），开窗时照它折回；命令行列裁决记录时也看得到。

⚠️ 动沉淀库的顺序迁移清单：与 01→02→03→04→08→12 这条链上的票串着做，不并行。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q964`：见 `../grill.md` 里那一条。

**Blocked by:** 02

**Status:** done

- [x] 先写红测试：就地落下一组 → 重开现场（新进程等价）→ 那一批的作用范围读得回来（今天读不回）
  - 证据：`crates/core/tests/verdict_store.rs::就地拒绝的那一组_重开现场之后作用范围读得回来_删库重扫之后也还在`——主库摆三份（`FC/汉化/` 两份、`FC/日版/` 一份，全未命中、同一批），工作目录落盘，
    走 `Queue::plan_scope` + `Queue::apply` 就地拒绝 `库/FC/汉化` 那一组（界面「拒绝这 N 条」走的同一条路），`drop` 掉现场再 `Site::open_file` 重开：
    `triage::Part::of(&那一批)` 读回作用范围（形状、按目录、组名、2 条、已拒绝），`Part::label()` 是「形状 · 按目录 库/FC/汉化」；照册子 `Parts::sync` 之后细分方式锁在按目录、那一项标着已拒绝、条数 2、分母 3。再删掉中立库重扫一遍，照旧在。
    先红：编不过（`Parts::sync`、`Part::of` 不在）；接上之后临时把 `triage::apply` 记作用范围那一格换成 `None` 验过红（「重开之后那一批的作用范围读不回来」）。
  - 做法：沉淀库**第 12 条迁移**（只追加，第 3 条一字没动）给 `verdict_batch` 加 `scope_shape` / `scope_axis` / `scope_group` / `scope_count` / `scope_kind` 五格（`verdict::BatchScope`）——形状存 `Shape::selector` 那串字（Q964 选项 A 原话「依据形状的选择器串」），轴与裁成什么存词，**分格结构化存，不塞进摘要或备注再拆**（ADR-0024）。
    多存条数与裁成什么两格：折回**一部分**、标「已通过／已拒绝」、占比的分母要它们，而那些条一落下就退出了队列、事后数不出来。
    写：`Queue::plan_scope` 在计划上填 `Plan::part`（`Part::within`：范围带下钻那一层、裁决是整组通过或拒绝——`PartKind::of(&DecisionSpec)`——才有），`triage::apply` 交给 `Store::put_batch`。读与折回只在 `triage::Part` 一处（`of` / `record` / `label`）；`Parts` 不再自己记账，改成照册子折（`Parts::sync`，撤掉的不算）。
- [x] 界面测试：就地裁一组 → 重建待确认屏 → 那几项标「已通过」、细分方式锁着
  - 证据：`crates/gui/tests/queue.rs::就地裁掉一组之后关掉窗口再开_那一项照旧标着已通过_细分方式照旧锁着`——就地通过一组之后把待确认屏整个换成 `queue::Screen::new()` 再 `reload`（开窗走的同一条路，`App::new`），展开那一批：停在锁着的那个轴上、`breakdown.locked` 是它、那一项 `done = 已通过`、条数照旧，屏上画着「已通过」与挡住换轴那句话，`set_axis` 换别的轴被挡。
    先红：临时让新窗口 `reload` 不重列裁决记录（等于这个窗口一样都没记着、也不读沉淀库）验过红（「重开之后那一批该停在锁着的那个轴上」）。
  - 界面那一侧：`Screen::refresh_records` 改成照册子 `Parts::sync`；`reload` 先折回再看新展开那一批锁没锁，锁着就停在那个轴上（没锁照旧用人选的轴）；`Drilled` 与 `scope_note` 收掉，`apply_plan` 只看 `Plan::part` 决定收不收就地那一框；提示条那一档改从核心库 `PartKind::of` 取（审查指出原先界面层另判一份）。
    裁决记录那一行的悬停多一行「作用范围：…」（`Part::label`）。`就地落下的那一批裁决在记录上认得出是哪一组` 改断言结构化那一份、备注是空的。
  - **保留**：就地落下的那一批**备注不再带作用范围原话**（原先界面把 `Scope::label()` 塞进备注）——规格没说，记挂单 `Q1567` 待拿主意的人裁；票 `gui-draws-the-rest-of-the-design/13` 正文追加了一行说清副行从哪儿取。
- [x] 命令行裁决记录里那一批带着作用范围
  - 证据：`crates/cli/tests/triage.rs::就地落下的那一批列出来带着作用范围_别的批不带`——先用命令行按选择器落一批，再走核心库就地拒绝一组，子进程跑 `romcat triage batches`：就地那一批底下印「作用范围：一条候选都没有 · 未命中 · 按目录 库/FC」，按选择器落的那一批不印。先红（列表里没有那一行）。
  - 做法：`run_triage_batches` 拿 `triage::Part::of(batch)` 印 `label()`，与界面同一份、同一句。
- [x] 旧沉淀库（没有这一格）顺序迁移后照常打开，旧批次的作用范围为空
  - 证据：`crates/core/src/verdict.rs` 单元测试 `第十一版的老库升上来_旧批的作用范围为空_新落下的批记得住作用范围`——照 `MIGRATIONS[..11]` 建第十一版、用旧程序的样子记一批（备注里带着旧界面写的作用范围原话），`migrate` 之后那一批照常读回、`scope` 为空、备注一字不动（**不拆回结构**），新落下的一批五格原样读回（`batches` 与 `batch` 两条查询都验）。
    `第三版的老库带着裁决和批升上来_一条都不丢` 原先拿新程序的 `put_batch` 往第三版的库里写批，新程序要写第 12 条加的那几格、在旧库上写不进去——改成照旧程序的样子直写（`旧程序记一批`）。
  - 单元测试 `triage::batch` 里 `一部分记进批表再读回来一个字不差`（DAT 与理由自带 `/` 也认得回来；认不出的那几格不当成一部分，`Q1568`）、`整批那一层不记成一部分`（整批、手工指定、没有发行版都折不出），以及原有两条改成照册子折。
- [x] 门禁全绿（本机 `sync_run` 那五条已知红除外，`Q479`）
  - 证据：`cargo xtask gate --keep-going -j 3 --test-threads 3`，日志 `/Users/nicoer/dev/game-wt/logs/q8-vs-03-gate1.log`（开头两行 `/Users/nicoer/dev/game-wt/slot1`、`q8/vs-03`），末行 `EXIT=0`、「7 条全绿」——
    fmt 3s、glossary 1s（范围：分支还没有自己的提交，看的是全部未提交改动）、check 28s、clippy 1s、test 799s、numbers 26s、doc 11s；test 那一步 88 个目标合计 2667 passed / 0 failed / 2 ignored（量级测量那两条）。截图门在 test 里跑过，基线一张没变。
    票上「`sync_run` 那五条已知红除外」已过时：本机默认盘上门禁就绿（`Q479` 的裁决）。
    README 的票数与测试条数先照 `cargo xtask numbers --write` 写回（165/205、2,669，日志 `/Users/nicoer/dev/game-wt/logs/q8-vs-03-numbers1.log`）。

**收挂单 `Q964`**（`../grill.md` 那一条标了 settled；原票 `gui-looks-like-the-design/19` 那一条标了做掉）：走 A——沉淀库批表加结构化的作用范围，开窗时照它折回、锁轴、标「已通过」，命令行列批看得到；「塞进 summary 再 parse」那条路没走。

**顺手**：改过的文件里复述的真库数换成量级词、指台账；台账没收的写了出处（`Q1569`）。

挂单：`Q1567`–`Q1569`。
