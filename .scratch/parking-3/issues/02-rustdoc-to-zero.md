# 02 — rustdoc 清到零并敲成硬红

**What to build:** 维护者写文档时断一条链接，门禁**当场**红。眼下 `cargo doc` 那条门禁
从来没绿过——68 条 rustdoc 告警全是既有的（`romcat-core` 独占 56 条），退出码是 0，
所以「跑绿」这个说法只在字面上成立。四条门禁里，只有它从来没写过标准。（挂单 `Q189`）

**Blocked by:** 01（硬红那个开关要落进票 01 建的门禁子命令里）

**Status:** done

- [x] `cargo doc` 那条补上 `--all-features`——它现在盖的范围比另外三条窄，漏掉 `demo` 后面那一整块。
      —— `cargo xtask gate --list` 现在第五行是
      `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --lib --bins`。
      ⚠️ 它让出去的那一格**接住了**：另加一条 `cargo check --workspace`（默认特性），
      门禁从四条变**五条**。取舍与代价见下面「本票走的那条路」与挂单 `Q204`。
- [x] 68 条清到 **0**。四类：多余的显式链接目标、解析不了的 intra-doc 链接、「既是函数又是模块」的歧义（要用消歧前缀）、公开文档链到私有条目。
      —— **实测是 67 条不是 68**（见下一条）。四类逐条清完：多余的显式链接目标 **7**、
      解析不了的 intra-doc 链接 **4**、「既是函数又是模块」的歧义 **3**（`sync.rs` 的
      `observe`、`sync/execute.rs` 的 `super::observe`、`cli/src/main.rs` 的 `sync::prepare`
      ——三处都是**函数**，加 `()` 消歧）、公开文档链到私有条目 **53**。
      清完实测：`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
      --lib --bins` 一条 rustdoc 告警都没有，退出码 0（另有两行 cargo 自己的
      `output filename collision`，见 `Q209`）。
      —— **`crates/` 底下 34 个文件**（cli 1 / core 29 / gui 4）。
- [x] 补 `--all-features` 之后**重新数一遍**——盖的范围变了，条数多半不是 68。把新数写进报告。
      —— 开工当天在本票分支（切自 `main` 的 `cd1e9b8`）实测：**67 条**，
      `romcat-core` **58** / `romcat-gui` **8** / `romcat-cli` **1**。
      ⚠️ **「盖的范围变了、条数会变」这个前提不成立**：默认特性与 `--all-features`
      两趟的告警清单逐行 diff **一条不差**（只差那行「generated 58 warnings」出现的位置，
      并发次序而已）。`demo` 后面那一整块的文档本来就一条告警都没有。
      补 `--all-features` 仍然是对的（那一块从此有人读了），但它今天一条也没多出来。
      记在挂单 `Q206`。
- [x] 硬红开关（rustdoc 的 `-Dwarnings`）落进门禁子命令，从此新断一条链接当场红。
      —— `xtask/src/gate.rs` 的 `DOC_ENV` 从 `&[]` 换成
      `&[("RUSTDOCFLAGS", "-D warnings")]`，接缝上票 `01` 留的那条告示当场兑现掉了。
- [x] 断链的红是可验的：故意断一条，门禁红；改回来，门禁绿。
      —— **两处都验了，一处是一次性的、一处是留在仓库里的：**
      ① **真仓库上**：把 `crates/core/src/title.rs:300` 的 `七层（\`rank\`）` 改成
      `[七层](NoSuchRankThing)`，`RUSTDOCFLAGS="-D warnings" cargo doc --workspace
      --no-deps --all-features` 报 `error: unresolved link to \`NoSuchRankThing\``、
      `error: could not document \`romcat-core\``，**退出码 101**；原样改回来，同一条命令
      **退出码 0**。
      ② **留在仓库里的钉子**：`xtask/tests/gate.rs` 新添
      `文档里断一条链接就红改成不带链接的写法就绿`——照票 `01` 那条 fmt 红绿测试的写法，
      拿一份丢弃 crate **真起子进程**跑门禁的 `doc` 那一条，两面都验：文档里链到一个
      不存在的条目就退非零，同一份输入把链接改成不带链接的代码体就退 0。
      **只验一面是恒真的**：只验红，一条恒红的命令也能过；只验绿，本票之前那条软红也能过。
- [x] 只改文档注释文本与链接写法，**一行行为都不改**。判据：这张票的 diff 里非文档注释行为 0（`git diff` 过滤 `///` / `//!` 之外的改动）。
      —— **34 个文件**，判据当场跑过：
      `git diff -U0 -- crates/ | grep -E '^[+-]' | grep -vE '^(\+\+\+|---)' | grep -vE '^[+-][[:space:]]*(///|//!)'`
      **输出为空**——`crates/` 底下 34 个文件的改动**全部**是 `///` / `//!` 行。
      门禁那一侧（`xtask/`、`README.md`、`.github/workflows/gate.yml`）是本票要改的
      门禁配置，不在这条判据的范围里。
- [x] 挂单 `Q189` 标 `settled` 并迁回票 `queue-followups/11`。
      —— `.scratch/PARKING-LOT.md` 的「已收索引」里 `Q189` 那一行标了 **settled** 并指向去处；
      `.scratch/queue-followups/issues/11-format-repo-and-gate.md` 末尾「挂单裁决（第三轮）」
      一节下**追加**了「追加：`Q189`（票 `parking-3/02` 落地）」，逐条写了裁决，
      并顺带更正该票记录里的两处数（68 → 67、四条 → 五条）。**原验收正文一个字没动。**

## 硬约束

- 「公开文档链到私有条目」那几处，**不许**靠把私有条目改成公开来消警——那是改 API 面。改文档措辞或用非链接写法。

## 本票走的那条路：门禁从四条变五条

票 `01` 把一条告示留在 `DOC_ENV` 的接缝上（它 `/code-review` 抓的第 1 条）：
`clippy` 与 `test` 都带 `--all-features`，于是**只有 `doc` 这一条跑在默认特性上**
——也就是 `demo` 关掉、`cargo build --release` 真正交付出去的那份配置。给 `doc` 补上
`--all-features` 之后，四条就**没有任何一条**再编译那份配置了。

**这是一条真的取舍，不是顺手改。** 两条路都站得住，票 `01` 也把两条都写在那儿了：

| 路 | 做法 | 为什么没走 / 走了 |
|---|---|---|
| A | 另加一条默认特性的 `cargo check --workspace` | **走了。** 它盯的东西说得出名字：「`cargo build --release` 交付出去的那份配置还编不编得过」。 |
| B | `doc` 留在默认特性，另开一条 `--all-features` 的 `doc` | 没走。它盯的是「默认特性下**文档**编不编得出来」——那件事没人在乎，它只是**碰巧**顺带保住了编译。用一件没人在乎的事去替一件要紧的事把门，正是这一格当初变空的原因。 |

于是 `steps()` 现在是 **五条**：`fmt` → `check` → `clippy` → `test` → `doc`，仍按从便宜到贵排。
`check` 是五条里**唯一**跑在默认特性上的，`xtask/tests/gate.rs` 的
`交付出去的那份配置还有一条在看着` 钉着这件事（它数的是「不带 `--all-features` 且带
`--workspace` 的门禁有且只有一条，名字叫 `check`」）。

**代价，说清楚**：

1. **「四条」这族字面量全改了口。** `README.md`、`.github/workflows/gate.yml`、
   `xtask/src/main.rs`、`xtask/Cargo.toml` 里写死的「四条」都改成了「五条」。
   往后每加一条门禁都得再改一遍——与 `Q198` 记的测试条数是同一类东西（`Q204`）。
2. **多一份默认特性的编译产物**，冷跑多一趟 `cargo check` 的时间。
3. **`check` 不带 `--all-targets`**，漏的那一格写在 `Q208` 里。

## 挂单

本票号段 `Q204`–`Q213`，用了 6 个（`Q204`–`Q209`），`Q210`–`Q213` 留空号。

| 挂单 | 一句话 |
|---|---|
| `Q204` | 给 `doc` 补 `--all-features` 让出的那一格，我另加了一条 `cargo check --workspace`：门禁从四条变五条 |
| `Q205` | `Op::ALL` 的注释指着一个从来不存在的 `Self::for_number`，而 `Op::ALL` 全仓库零使用者（只改注释，一行代码没动） |
| `Q206` | 票正文那个「68 条、`romcat-core` 独占 56」是旧数；且「补 `--all-features` 之后条数会变」这个前提不成立 |
| `Q207` | 53 处「公开文档链到私有条目」一律去掉链接，那 53 个引用从此点不过去；没走 `--document-private-items` 那条路 |
| `Q208` | `check` 那一条不带 `--all-targets`：默认特性下的测试与实测目标没人编 |
| `Q209` | 给 `doc` 补 `--lib --bins` 让同名 bin 也被读，代价是每趟两行 cargo 的 `output filename collision` |

## 票哪里写错了

- **「68 条 rustdoc 告警全是既有的（`romcat-core` 独占 56 条）」**（票正文第 4 行、
  验收第 2 条、`spec.md` Problem Statement 第 1 条）——实测 **67 条**，
  `romcat-core` **58** / `romcat-gui` **8** / `romcat-cli` **1**。这一轮十七张票各自
  添过又清过，数一直在动；票第 3 条验收（「重新数一遍」）写的正是这件事。（`Q206`）
- **「补 `--all-features` 之后……盖的范围变了，条数多半不是 68」**（验收第 3 条）——
  在**只读 lib** 的口径下（也就是票与门禁原本的口径），范围确实变了而**条数一条都没变**：
  两趟的告警清单逐行相同。收尾审查挖出了成因（见下），把同名 bin 也读进来之后再量，
  这句话反过来成立、但方向与票猜的相反：**默认特性 68 条、`--all-features` 67 条**
  ——`--all-features` 让告警**少**一条，不是多。（`Q206`）

## 审查回执

`/code-review`（`--effort high`）只读审了 `git diff HEAD`（工作目录钉死在本 worktree，
**一条 cargo 都没跑、一条破坏性 git 都没跑、一个字节没改**）。它独立复核了本票的核心判据：
`git diff HEAD -- crates/ | grep -vE '^[+-]\s*//'` **输出为空**；去掉显式目标的那七处
在各自文件的 `use` 里都在作用域内；补上显式目标的那两处作用域里确实没有 `sync` /
`workspace`（不会反过来触发「多余的显式链接目标」）；三处 `()` 消歧确实都是歧义；
`rule.rs` 那段重写过的注释与 `Op::fits` 的实现对得上。

报了 **4 条，全部落地**：

| # | 它报的 | 处置 |
|---|---|---|
| 1 | **medium。「从此断一条文档链接门禁当场红」不是全仓面的。** `cargo doc` 跳过与 lib 同名的 bin，而 `romcat-gui` 与 `xtask` 都是 lib + 同名 bin——`crates/gui/src/main.rs`（四百多行、七处 intra-doc 链接）与 `xtask/src/main.rs` 一个字都没被 rustdoc 读过。证据是现成的 `target/doc/src/romcat_gui/` 有 `demo.rs.html` 却没有 `main.rs.html`，而没有 lib 的 `romcat-cli` 反倒有 | **改了，这条抓得最准——它抓的正是本票自己写下的那句话。** 我独立复现过它给的证据，两处都对。`doc` 那一条补上 `--lib --bins`；**并且补了一条钉子**：`xtask/tests/gate.rs` 的 `与_lib_同名的那个_bin_里断一条链接照样红`（丢弃 crate 的 `lib.rs` 干净、断链只在同名 `main.rs` 里）。把 `--lib --bins` 拿掉它当场红、装回去就绿，**实测过**；而原先那条只验 lib 的红绿测试在两种情况下都绿——也就是说旧钉子确实钉不住这一格。代价（每趟两行 cargo 的 `output filename collision`）记在 `Q209`。它顺带解释了「两趟条数一条不差」的成因，`Q206` 已经补上。 |
| 2 | **low。`check()` 的注释说「默认特性下的测试目标由 `clippy` 那条在 `--all-features` 下盖着」不成立，而且与本票自己开的 `Q208` 打架** | **改了。** 这条对：`clippy` 编的那批测试目标是 `demo` **开着**的那一份，盖不住默认特性那一格；`Q208` 里写的才是对的，而代码注释把洞说成盖住了——「将来照注释办事的人不会去看挂单」。注释改成如实说「五条里一条都没编」，并写清漏的是哪种文件、要盖住得付什么代价。 |
| 3 | **low。「四条」这族字面量漏扫了一处**：`Step::name` 的文档还写着「`fmt` / `clippy` / `test` / `doc`」，缺 `check` | **改了。** 正是 `Q204` 记的那族维护代价，偏偏漏在改动最集中的这个文件里。 |
| 4 | **low。「`crates/` 底下 33 个文件」是 34**（cli 1 + core 29 + gui 4），票与 `Q207` 同一个数 | **改了**两处。这份记录的用处就是给后来人当账本。 |

它另外查过、判为没问题的：新加那条红绿测试两面都有效（`DOC_ENV` 退回 `&[]` 时红那一半会
失败，所以钉得住）；`交付出去的那份配置还有一条在看着` 的过滤条件在 `fmt` 上不会误命中；
`--all-features` 新纳入 rustdoc 的 `crates/gui/src/demo.rs` 里 19 条 intra-doc 链接
（含跨 crate 的几条）逐条对过定义，都解析得了。

⚠️ 它提到丢弃 crate 的包名 `丢弃` 是 Unicode 但过得去——**那只对 lib-only 成立**。
落第 1 条时造 lib + 同名 bin 的夹具当场炸了
`crate name 丢弃 passed to --extern is not a valid ASCII identifier`：
一旦包里的 bin 要链自己的 lib，cargo 递的 `--extern` 就走 ASCII 校验。
包名已改成 `throwaway`，理由写在那份 `Cargo.toml` 的注释里。

## 门禁实测（收尾，一趟五条）

`cargo xtask gate --keep-going -j 6`，退出码 **0**，**5 条全绿**，合计 468 秒：

```
绿  fmt        1s  cargo fmt --all --check
绿  check      7s  cargo check --workspace -j 6
绿  clippy    15s  cargo clippy --workspace --all-targets --all-features -j 6
绿  test     434s  cargo test --workspace --all-features -j 6
绿  doc       11s  RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --lib --bins -j 6
```

`test` 那一条：66 个 `test result:` 行、**1,722 条通过、0 失败**（本票添的是 `xtask` 那 3 条，
4 → 7）。**这个数随票涨，以 `cargo xtask gate` 自己的输出为准。**
`doc` 那一条另印两行 cargo 的 `output filename collision`（`romcat_gui` 与 `xtask`），
是 `--lib --bins` 的既定代价，不是 rustdoc 告警——退出码是 0，见 `Q209`。
`Q349` 那条挂钟彩票（`任务六秒都没跑完`）这一趟没撞上。

## 挂单裁决（第三轮收口，2026-09-08）

本票在实现路上记下的 **6 条**（`Q204`–`Q209`）由票 `parking-3/18` 收口裁完，条目全文迁自 `.scratch/PARKING-LOT.md`（那边的条目区已清空）。**本票的原验收正文一个字没动。**

分三档：**A** 已经做完只是没销 ／ **B** 不成立了 ／ **C** 仍然成立要开票。判据是**一条挂单有了主人就算收了，不是等它做完**。本票：**A 4 ／ B 0 ／ C 2**。

| 挂单 | 档 | 一句话 |
|---|---|---|
| `Q204` | **A** | 给 `doc` 补 `--all-features` 让出的那一格，我另加了一条 `cargo check --workspace`：门禁从四条变五条 |
| `Q205` | **C**<br>第四轮「零散（各自独立，不成组）」 | `Op::ALL` 的注释指着一个从来不存在的 `Self::for_number`，而 `Op::ALL` 全仓库零使用者 |
| `Q206` | **A** | 票正文那个「68 条、`romcat-core` 独占 56」是旧数；而且「补 `--all-features` 之后条数会变」这个前提不成立 |
| `Q207` | **A** | 53 处「公开文档链到私有条目」一律改成不带链接的代码体，那 53 个引用从此点不过去 |
| `Q208` | **A** | `check` 那一条不带 `--all-targets`：默认特性下的测试与实测目标没人编 |
| `Q209` | **C**<br>第四轮「门禁与工具链」 | 给 `doc` 补 `--lib --bins` 是为了让同名 bin 也被读，代价是每趟印两行 cargo 的 `output filename collision` |

### Q204 — 给 `doc` 补 `--all-features` 让出的那一格，我另加了一条 `cargo check --workspace`：门禁从四条变五条

- **来自：** 票 `parking-3/02`
- **类别：** 两份东西矛盾（票 `parking-3/01` 在接缝上留的告示，本票必须当场裁）
- **在哪：** `xtask/src/gate.rs` 的 `steps()` / `check()` / `DOC_ENV`；
  `xtask/tests/gate.rs` 的 `交付出去的那份配置还有一条在看着`；
  `README.md`「⚠️ 门禁必须带 `--all-features`」一节
- **为什么没停线：** 票 `01` 已经把两条路都写在 `DOC_ENV` 的注释里了（另加一条默认特性的
  `cargo check --workspace`，或把 `doc` 留在默认特性、另开一条 `--all-features` 的），
  两条都站得住，代价也都在本票之内——翻案只要动 `steps()` 一行。

**这是一次接力，不是一次顺手改：票 `01` 预告，票 `02` 兑现。**

票 `01` 落门禁时它自己的 `/code-review` 报的第 1 条就是这个，而修它不是那张票的活，
于是票 `01` **把告示留在了接缝上**（`DOC_ENV` 的文档里整整一节，点名给票 `02`）。
告示说的是：`clippy` 与 `test` 都带 `--all-features`，于是当时**只有 `doc` 这一条跑在
默认特性上**——也就是 `demo` 关掉、`cargo build --release` 真正交付出去的那份配置。
换句话说，**那份配置编不编得过，当时是靠 rustdoc 顺带保住的**。票 `02` 一给 `doc` 补上
`--all-features`（票要求的，也是对的），四条就没有任何一条再编译它。

- **这张票实际做了什么：** 走了**第一条**：`doc` 补上 `--all-features`，另加
  `cargo check --workspace`（默认特性，不带 `--all-targets`），排在 `fmt` 之后、
  `clippy` 之前（仍是从便宜到贵）。

  **为什么是这一条**：它盯的东西**说得出名字**——「`cargo build --release` 交付出去的
  那份配置还编不编得过」。
  **另一条路（让 `doc` 保持默认特性、另开一条 `--all-features` 的 `doc`）也站得住**，
  它同样能把那一格填上，而且不必新起一个名字。**没走它的理由**：那样一来「谁在守这一格」
  就是「默认特性下**文档**编不编得出来」——那件事本身没人在乎，它只是碰巧顺带保住了编译，
  正是这一格当初变空的原因（换一个人把 `doc` 的参数一改，格子又空了，而且没人看得出来）。
  ⚠️ 而且**那条路会漏掉 `demo` 后面那一整块的文档**：`doc` 留在默认特性时，
  `crates/gui/src/demo.rs` 里那 19 条 intra-doc 链接没有一条被读过，
  而票 `02` 的题目正是「文档链接断了要当场红」。要两头都不漏就得**两条 `doc`**，
  那是门禁六条而不是五条。

  **代价，说清楚：**
  1. **门禁多一格时间。** 默认特性是一份**独立的**编译产物，与 `clippy` / `test` /
     `doc` 那份 `--all-features` 的不共用 fingerprint。本机热跑 1.9 秒，**冷跑没量**
     ——冷机上它是一整趟 `cargo check --workspace` 的钱。
  2. **「四条」这族字面量全改了口。** `README.md` / `.github/workflows/gate.yml` /
     `xtask/src/main.rs` / `xtask/Cargo.toml`，外加 `xtask/src/gate.rs` 里
     `Step::name` 的文档（这一处是收尾审查抓出来的，我自己漏扫了）。往后每加一条门禁
     都得再扫一遍——与 `Q198` 记的测试条数是同一类东西。
  3. **`check` 只是 `check` 不是 `build`。** 链接期的问题它不管（本仓库眼下没有
     C 链接以外的东西）。
  4. **它不带 `--all-targets`**，漏的那一格另记在 `Q208`。
- **谁来裁：** 拿主意的人（要不要把「四条 / 五条」这族字面量也交给 `--list` 去答）
- **状态：** settled —— 第三轮收口 2026-09-08，走 **A（已经做完，收口只是销号）**
- **收尾裁决（第三轮收口，2026-09-08）：** **A** —— 门禁四条变五条已落地，`cargo check --workspace` 盯的东西说得出名字（「`cargo build --release` 交付出去的那份配置还编不编得过」），四条代价逐条写清。剩下的那个问题（字面量交给 `--list`）并进 `Q198`。

### Q205 — `Op::ALL` 的注释指着一个从来不存在的 `Self::for_number`，而 `Op::ALL` 全仓库零使用者

- **来自：** 票 `parking-3/02`
- **类别：** 两份东西矛盾（文档说的和代码做的不一致）
- **在哪：** `crates/core/src/sublibrary/rule.rs:368` 一带（`Op::ALL` 那个常量）；
  真正判「哪个符号配得上哪个维度」的是同文件 `Op::fits`
- **为什么没停线：** 这是本票四类 rustdoc 告警里的「解析不了的 intra-doc 链接」那一条，
  清掉它只要改注释；而**票明写**「真发现文档说的和代码做的不一致，那是一条挂单，
  不是就地改代码」。
- **这张票实际做了什么：** 只改注释，一行代码没动。原句是
  「界面照这个次序摆运算符；文字维度取前五个，数值维度取 `[`Self::for_number`]`」——
  `for_number` 在全仓库**一次都没出现过**（只在这句注释里），而 `Op::ALL` 本身也
  **一个使用者都没有**（`grep -rn '\bALL\b' crates/` 出来的全是别的类型的 `ALL`）。
  改成照 `fits` 的实情写：文字维度正好是前五个，数值维度是除掉那三个字串符号
  （`~` `^` `$`）之外的**六个**——顺带把原句那半句「数值维度取 `for_number`」暗示的
  「数值那一档也是一段连续前缀」纠正掉，它不是。
  **没做的**：没有去掉 `Op::ALL`，也没有补一个 `for_number`。界面眼下摆运算符走的是
  别的路（`fits`），`Op::ALL` 是不是该删、还是该有个使用者，得看子库规则编辑器那一族。
- **谁来裁：** 拿主意的人
- **状态：** settled —— 第三轮收口 2026-09-08，走 **C（仍然成立，归第四轮）**
- **收尾裁决（第三轮收口，2026-09-08）：** **C** —— 注释已改准（照 `Op::fits` 的实情写，顺带纠正了「数值那一档也是一段连续前缀」的暗示）。**没裁的是**：`Op::ALL` 全仓库零使用者，该删还是该有个使用者。条目自己指了归宿——子库规则编辑器那一族。
  **去处：** 第四轮规格材料，「零散（各自独立，不成组）」那一组。

### Q206 — 票正文那个「68 条、`romcat-core` 独占 56」是旧数；而且「补 `--all-features` 之后条数会变」这个前提不成立

- **来自：** 票 `parking-3/02`
- **类别：** 票写错了
- **在哪：** `.scratch/parking-3/issues/02-rustdoc-to-zero.md` 第 4 行与第 11–13 行；
  `.scratch/parking-3/spec.md` Problem Statement 第 1 条；`xtask/src/gate.rs` 原 `DOC_ENV` 注释
- **为什么没停线：** 票第 13 条验收写的正是「补 `--all-features` 之后**重新数一遍**」，
  照做即可。
- **这张票实际做了什么：** 开工当天在本票分支（从 `main` 的 `cd1e9b8` 切出）实测：
  **67 条**（`romcat-core` 58 / `romcat-gui` 8 / `romcat-cli` 1），不是 68 / 56。
  更要紧的是——**默认特性与 `--all-features` 数出来一条不差**：两趟的告警清单
  逐行 diff 只差一行「`romcat-core` (lib doc) generated 58 warnings」出现的**位置**
  （并发编译的次序），内容完全相同。也就是说票（与 `DOC_ENV` 原注释）预设的
  「盖的范围变了，条数多半不是 68」这个前提**不成立**：`demo` 后面那一整块文档本来就
  一条告警都没有。补 `--all-features` 仍然是对的（那一块从此有人读了），
  但它今天**一条也没多出来**。
  ⚠️ **收尾审查把这个「一条不差」的成因挖出来了，它不只是巧合**（见 `Q209`）：
  `cargo doc` 默认跳过与 lib 同名的 bin，于是 `crates/gui/src/main.rs` 两趟都没被读过
  ——而那份文件的第 13 行 `见 [\`demo\`]` 在**默认特性**下必然解析不了。
  给 `doc` 补上 `--lib --bins` 之后重量：**默认特性 68 条、`--all-features` 67 条**，
  差的正是那一条。也就是说「补 `--all-features` 之后条数会变」这个前提**是成立的**，
  只是方向与票猜的相反——`--all-features` 让告警**少**一条，而不是多。
  ⚠️ 编排者递给我的那张对照表（默认 67 / `--all-features` 70）也是同一个误会：
  70 是 `grep -c '^warning'` 数出来的，里头含着三行「generated N warnings」的**汇总行**。
  真数是两边都 67。
- **谁来裁：** 收尾
- **状态：** settled —— 第三轮收口 2026-09-08，走 **A（已经做完，收口只是销号）**
- **收尾裁决（第三轮收口，2026-09-08）：** **A** —— 票写错了（「68 条、core 独占 56」→ 实测 **67**，core 58 / gui 8 / cli 1），已实测纠正并**追加**进票 `queue-followups/11` 末尾，那张票的原验收正文一个字没动。收尾审查还把「默认特性与 `--all-features` 一条不差」的成因挖了出来（`Q209`）——不是巧合，是 `cargo doc` 默认跳过同名 bin。

### Q207 — 53 处「公开文档链到私有条目」一律改成不带链接的代码体，那 53 个引用从此点不过去

- **来自：** 票 `parking-3/02`
- **类别：** 路过发现，不在范围内
- **在哪：** 本票 diff 里 `crates/` 底下那 34 个文件（`git log -p` 找
  「`[`X`]` → `` `X` ``」那一族改动）；判据写在 `xtask/src/gate.rs` 的 `DOC_ENV` 注释里
- **为什么没停线：** 票的硬约束把另外两条路都堵死了——「**不许**靠把私有条目改成公开来
  消警，那是改 API 面」；剩下的就是改措辞或去链接。
- **这张票实际做了什么：** 67 条里有 **53 条**是这一类，一律
  `[`X`]` → `` `X` ``（措辞一个字不动，只把链接语法拆掉）。
  **代价，说清楚**：这 53 处从此在生成的文档里**点不过去**了——它们指的本来就是
  `cargo doc` 不生成页面的私有条目，所以链接**原先**也是坏的（rustdoc 报的正是这个），
  但读源码的人不再能靠编辑器的「跳转」直接过去。
  **另一条路**是给门禁那一条加 `--document-private-items`，那样 53 条全部当场变合法、
  一个字都不用改。**没走**：那会把整个私有实现面都摊进公开文档站，
  「公开文档」与「内部注释」的界线就没了——而这条界线正是这一类告警在守的东西。
  真要恢复导航，正路是**另开一条只给维护者看的私有文档**
  （`cargo doc --document-private-items` 单独出一份），与门禁那一条互不相干。
- **谁来裁：** 拿主意的人
- **状态：** settled —— 第三轮收口 2026-09-08，走 **A（已经做完，收口只是销号）**
- **收尾裁决（第三轮收口，2026-09-08）：** **A** —— 53 处一律 `[`X`]` → `` `X` ``，措辞一个字没动。代价（这 53 处从此在生成的文档里点不过去）与正路（真要恢复导航就另开一份 `--document-private-items` 的私有文档，与门禁那一条互不相干）都写明了，没人欠一个动作。

### Q208 — `check` 那一条不带 `--all-targets`：默认特性下的测试与实测目标没人编

- **来自：** 票 `parking-3/02`
- **类别：** 路过发现，不在范围内
- **在哪：** `xtask/src/gate.rs` 的 `check()`
- **为什么没停线：** 它要盯的东西（`cargo build --release` 交付的那份配置）**盖住了**，
  多盖的那一块由 `clippy --all-targets --all-features` 接着。
- **这张票实际做了什么：** 只写 `cargo check --workspace`，**不加** `--all-targets`。
  理由：这一条存在的唯一意义是「交付出去的那份还编不编得过」，那份就是库与二进制；
  加上 `--all-targets` 会把默认特性下的测试目标也编一遍，而那批目标在
  `--all-features` 下已经被 `clippy` 编过一遍了——多花的时间买不到新东西。
  **漏的那一格**：一个**不带** `#[cfg(feature = "demo")]`、却引用了 `demo` 后面东西的
  测试文件，五条全绿而 `cargo test --workspace`（不带 `--all-features`）编不过。
  眼下不存在这种文件（`demo` 那七个测试目标走的是 `required-features`）。
- **谁来裁：** 拿主意的人
- **状态：** settled —— 第三轮收口 2026-09-08，走 **A（已经做完，收口只是销号）**
- **收尾裁决（第三轮收口，2026-09-08）：** **A** —— `check` 不带 `--all-targets` 是有理由的取舍——多盖的那一块由 `clippy --all-targets --all-features` 接着。漏的那一格今天**没有触发者**：`demo` 那七个测试目标走的是 `required-features`。

### Q209 — 给 `doc` 补 `--lib --bins` 是为了让同名 bin 也被读，代价是每趟印两行 cargo 的 `output filename collision`

- **来自：** 票 `parking-3/02`（收尾 `/code-review` 抓的第 1 条）
- **类别：** 工具缺陷（[cargo#6313](https://github.com/rust-lang/cargo/issues/6313)）
- **在哪：** `xtask/src/gate.rs` 的 `doc()`；`crates/gui/Cargo.toml` 与 `xtask/Cargo.toml`
  的 `[[bin]]`（两个包都是 lib 加一个**同名** bin）
- **为什么没停线：** 两条路都在本票之内，翻案只要改 `doc()` 的参数串。
- **这张票实际做了什么：** 给 `doc` 补上 `--lib --bins`。
  **不补会怎样**：`cargo doc` 默认跳过与 lib 同名的 bin（两者都往
  `target/doc/<crate>/index.html` 里写），于是 `crates/gui/src/main.rs`（四百多行、
  里头七处 intra-doc 链接）与 `xtask/src/main.rs` **一个字都没被 rustdoc 读过**
  ——本票敲的那句「从此断一条文档链接，门禁当场红」在补之前**只对 lib 成立**。
  实测佐证：`target/doc/src/romcat_gui/` 有 `demo.rs.html` 却**没有** `main.rs.html`，
  而没有 lib 的 `romcat-cli` 反倒有 `main.rs.html`。
  **代价，说清楚**：cargo 为那两个包各印一行 `warning: output filename collision`，
  每趟门禁都印。那是 **cargo** 的告警不是 rustdoc 的，`-D warnings` 不把它当错
  （实测退出码仍是 0）；而 `target/doc/<crate>/index.html` 从此是 lib 与 bin 里
  **后跑完的那一个**——门禁不消费 `target/doc/`，人自己跑 `cargo doc --open` 不带
  `--bins`，拿到的仍是正常的 lib 文档。
  **另一条路**是另开一条 `cargo doc -p romcat-gui --bins`（外加一条给 `xtask` 的），
  门禁就变七条、还是撞同一个文件名。**没走**：多两条命令买不到别的东西。
  真要根治得给两个 bin 改名，那是改交付物的名字，不该由一张文档票定。
  **钉子**：`xtask/tests/gate.rs` 的 `与_lib_同名的那个_bin_里断一条链接照样红`
  ——丢弃 crate 的 `lib.rs` 干净、断链只在同名 `main.rs` 里；把 `--lib --bins`
  拿掉它当场红（实测过），而原先那条只验 lib 的红绿测试照旧绿。
- **谁来裁：** 拿主意的人（两个 bin 要不要改名）
- **状态：** settled —— 第三轮收口 2026-09-08，走 **C（仍然成立，归第四轮）**
- **收尾裁决（第三轮收口，2026-09-08）：** **C** —— `--lib --bins` 补上是对的（补之前 `crates/gui/src/main.rs` 四百多行、七处 intra-doc 链接**从来没被 rustdoc 读过**），代价是每趟门禁印两行 cargo 的 `output filename collision`。根治要给两个 bin 改名——**那是改交付物的名字，不该由一张文档票定**，条目自己就是这么写的。
  **去处：** 第四轮规格材料，「门禁与工具链」那一组。
