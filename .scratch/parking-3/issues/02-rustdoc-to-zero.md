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
