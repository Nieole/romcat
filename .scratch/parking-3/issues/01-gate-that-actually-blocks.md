# 01 — 门禁真的会拦人

**What to build:** 维护者推一次代码，门禁四条自动跑起来；本地跑的是同一套定义，不会漂。
换一台机器提交，`cargo fmt` 的产出不会突然多出一大片与手上这件事无关的重排。

眼下「门禁」只是 `README.md` 「开发」一节里的四行命令，仓库里没有任何机器执行它的地方，
也没有任何东西钉住是哪一版 `rustfmt`。（挂单 `Q187` `Q188`）

**Blocked by:** None — can start immediately

**Status:** done

- [x] 门禁四条落成仓库里**一个真实可跑的子命令**，定义只有一份。承载它的东西不进交付出去的二进制。
      —— `cargo xtask gate`（别名在 `.cargo/config.toml`）。四条只在 `xtask/src/gate.rs` 的
      `steps()` 里定义一遍；`README.md` 与 `.github/workflows/gate.yml` 都只指向它，
      「它跑哪四条」由 `cargo xtask gate --list` 当场答。承载它的是独立 crate `xtask`，
      `romcat` 与 `romcat-gui` 都不依赖它，一个字节都不进那两个二进制（挂单 `Q194` 记了
      为什么不做成 `romcat gate`）。**钉着的测试**：`xtask/tests/gate.rs` 的
      `四条打印出来就是原先写在_readme_里的那四条`——四行字面量抄自门禁落地**之前**的
      README 开发块，是独立基准，不是从 `steps` 反推的。
- [x] 那个子命令能退回**限流模式**（并发与测试线程可调），默认按机器给的资源跑；限流那一档要能在这台 15 GB 的开发机上跑完不被 OOM 杀掉。
      —— `--throttle`（＝ `-j 1 --test-threads=2`），`-j N` 与 `--test-threads N` 各自也覆盖得动；
      不给就一个开关都不递。测试 `限流那一档把两个开关都递下去` 把两边都钉了（限流那一档递、
      默认那一档一个都不递）。**实测**：`cargo xtask gate --keep-going --throttle` 在这台
      15 GB 机器上**四条全绿**、挂钟 9:40，`/usr/bin/time -v` 报
      **Maximum resident set size = 490,408 KB ≈ 490 MB**，没有被 OOM 杀掉。
      ⚠️ **口径**：那是整棵进程树里**单个进程**的峰值，不是整趟的总和。作对照，
      默认那一档那趟（`-j 6 --test-threads 6`）同一指标是 **2,408,576 KB ≈ 2.4 GB**。
- [x] 给它一份故意不 fmt-clean 的输入，它**红**；改回来，它绿。这条要有测试钉着。
      —— `xtask/tests/gate.rs` 的 `排版不干净就红改回来就绿`：在系统临时目录里造一份
      **独立于本仓库**的丢弃 crate（`pub fn 加(a:i32,b:i32)->i32{a+b}`），拿门禁第一条
      真的起子进程 → 退非零；跑一趟 `cargo fmt --all` 之后同一条 → 退 0。整份 4 条 0.16s 跑完。
      另外端到端验过一次：往 `xtask/tests/gate.rs` 尾巴上塞一行不 fmt-clean 的代码，
      `cargo xtask gate` 打出 `红  fmt  3s`、在第一处红上停、`exit=1`；撤掉就零输出、`exit=0`。
- [x] `rust-toolchain.toml` 落在仓库根，钉住具体版本（当下实测 `rustfmt 1.9.0-stable`）。文件里写清副作用：rustup 从此为每个人拉这一版，第一次进目录可能触发一次下载。
      —— 落在仓库根：`channel = "1.98.1"`、`components = ["rustfmt", "clippy"]`、
      `profile = "minimal"`（挂单 `Q197`：这一项票没要求，是我加的，理由与保留写在那儿）。
      落它时实测 `rustc 1.98.1 (48a229cea 2026-09-01)` / `rustfmt 1.9.0-stable` /
      `clippy 0.1.98` / `cargo 1.98.1`，逐条写在文件里。副作用整节写明，README
      「工具链钉住了」一节收了短版。
      **带保留**：这台机器上 1.98.1 早已装着，落这份文件**没有触发下载**——「第一次进目录
      会下载」那句是按 rustup 的行为写的，本机没验到（同挂单 `Q197`）。
- [x] 说明 `Cargo.toml` 的 `rust-version = "1.95"` 与它**并存、互不替代**——前者是给 cargo 解析器的 MSRV 声明，不钉工具链。
      —— 写在两处：`rust-toolchain.toml` 里一整节（MSRV 是**下界**、管依赖解析与「多老的
      rustc 也编得动」，钉不住任何一版；`rust-toolchain.toml` 是**定值**、管「产出逐字节一致」），
      README「工具链钉住了」一节收了同一段的短版。两处都写了那句话：
      **门禁跑 1.98.1，而这套代码声明自己 1.95 就编得动，两个数不同不是矛盾**，
      并叮嘱别因为这里改了就顺手去动那边。
- [x] GitHub Actions 的 workflow **只调那个子命令**，不重复列命令；缓存 `~/.cargo` 与 `target/`。
      —— `.github/workflows/gate.yml` 里唯一那条门禁 `run` 是 `cargo xtask gate --keep-going`，
      文件里**一条门禁命令都没抄**。缓存走 `actions/cache@v4`，路径是 cargo 官方 CI 配方
      那四条（`~/.cargo/bin`、`registry/index`、`registry/cache`、`git/db`）加 `target/`，
      **不含** `registry/src`（那是解包出来的源码，存它等于同一份东西存两遍）；key 里带上
      `Cargo.lock` 与 `rust-toolchain.toml`。工具链走 `rustup toolchain install`（不给参数
      就读 `rust-toolchain.toml`），**不用 runner 预装的 stable**。
      **带保留**：这个仓库没有 remote，这份 YAML **从未被任何 CI 实跑过**（挂单 `Q195`）。
      其中唯一真有风险的那一格已经量掉：`ldd target/debug/romcat-gui` 只链
      `libc` / `libm` / `libgcc_s` / `ld-linux`，界面测试又全是 headless，所以
      **一个 apt 步骤都不写**，理由与实测写在文件里。
- [x] CI 上不带本机那套限流开关——runner 没有 15 GB 的约束。
      —— workflow 里没有 `--throttle`、没有 `-j`、没有 `--test-threads`；给的只有
      `--keep-going`（本地默认在第一处红上就停以省时间，CI 上反过来，一趟把四条的结论都给出来，
      省的是**人**来回推一次的时间）。那一步旁边写明了为什么不限流，README 同一节也写了。
- [x] `README.md` 的门禁块改成指向那个子命令，测试条数改成**当下实测值**（写这张票时是 1,631 条 / 62 个目标，README 上的 1,581 已过时），并写明这个数会随票涨、以门禁输出为准。
      —— 「开发」一节那四行命令块换成 `cargo xtask gate` 一句，另加 `--list`、限流两档、
      以及新的「工具链钉住了」一节；`xtask/` 也进了「项目怎么组织的」那张树。
      **测试条数按实测改准，三处一起改**（挂单 `Q198`：这个数住在三处，没有任何东西盯着它们同步）：
      `README.md` 的「1,581 条测试全绿」→ **1,635**；`README.md` 与
      `crates/gui/Cargo.toml` 里那对「少跑 101 条（1,480 而不是 1,581）」→
      **少跑 105 条（1,530 而不是 1,635）**，两边同一口径实测
      （`cargo test --workspace [--all-features] -- --list`）。两处都写上
      **「这个数会随票涨，以 `cargo xtask gate` 自己的输出为准」**。
      ⚠️ **票里那个数不是错的**：票写「1,631 条 / 62 个目标」，我实测 **1,635 条 / 63 个测试目标**
      （外加 3 趟 doc-test），差的正好是**这张票自己新加的** `xtask/tests/gate.rs` 那 1 个目标 4 条测试。
- [x] 挂单 `Q187` `Q188` 标 `settled` 并把裁决迁回票 `queue-followups/11` 末尾「挂单裁决（第三轮）」一节。
      —— `.scratch/PARKING-LOT.md` 的「已收索引」里两行都标了 **settled** 并指向去处；
      `.scratch/queue-followups/issues/11-format-repo-and-gate.md` 末尾新开
      「挂单裁决（第三轮）」一节，逐条写了裁决（`Q187` → `cargo xtask gate` + workflow +
      README；`Q188` → `rust-toolchain.toml`，并把该票当年转挂单时提的顾虑逐条认下）。
      顺带在那儿注了一句该票记录里的「1,581 条」到今天已是 1,635 条，**原句不改**——
      那是当时的实测，改掉就没有对照了。

## 别顺手做的

- **不改 `cargo doc` 那一条**，它归票 `02`。这张票只负责让门禁**有地方执行**，不负责让第四条变绿。
- 不动任何 `.rs` 文件的行为。

### ⚠️ 本票破了上面第二条一次，破在哪、为什么

**动了 `crates/gui/tests/media.rs` 一条断言**（挂单 `Q196`）：测试 `切换选中那一帧不解码`
里那条「头一帧的**挂钟**耗时 < 1 秒」被拿掉，同一个测试里另外两条**结构性**判据
（`ready() == 0`、`busy() > 0`）留着不动。

**为什么破**：这一条恰好是本票的题目本身。落门禁那天实测——同一棵树、同一份代码，
`--test-threads=6` 且机器上另有三个工作区在冷编译时它**红**（头一帧 1.82s），
`--test-threads=2` 与安静下来单跑三趟**全绿**。它量的是这条测试抢不抢得到 CPU，
不是这段代码有没有在画帧线程上解码；而本票落的 `.github/workflows/gate.yml` 跑在
比这台又慢又忙的 runner 上，第一次真跑就会撞上。一条**在机器忙时红、闲时绿**的门禁，
拦的不是人是运气——这与本票另一条硬约束（`cargo doc` 那 68 条不能在本票变成硬红，
免得「那一片红与谁改了什么毫无关系」）是同一条道理。

**代价，说清楚**：「翻行会看得出来」那条延迟从此在门禁四条里**一个把门的都没有**
（README 指的 `--bench` / `--bench-browse` 是手动跑的，不在四条里）。那一格是空的，
记在 `Q196` 里，路由给票 `16` `17`。

## 审查回执

`/code-review`（`--effort high`）只读审了工作区未提交的全部改动（它独立跑过
`cargo xtask gate --list` / `--throttle --list` / `-j 0 --list`、`cargo test -p xtask`、
`cargo clippy -p xtask --all-targets --all-features`、`cargo fmt --all --check`，
一个字节没改、一条 git 写命令没跑）。报了 **8 条，落地 7 条、转挂单 1 条**：

| # | 它报的 | 处置 |
|---|---|---|
| 1 | 眼下**只有 `doc` 这一条跑在默认特性上**（`demo` 关掉、也就是真正交付的那份配置）；票 `02` 一给 `doc` 补上 `--all-features`，门禁四条就没有任何一条再编译它——那时 `cargo build --release` 编不过而门禁全绿 | **在接缝上留了告示。** 这条抓得最准，而修它是票 `02` 的活（本票不该加第五条命令）。`DOC_ENV` 的文档里加了一整节点名给 `02`：补 `--all-features` 的同时把这一格接住（另加一条默认特性的 `cargo check --workspace`，或把 `doc` 留在默认特性、另开一条）。 |
| 2 | workflow 每个 PR 分支跑**两趟**：`push` 与 `pull_request` 都不加分支过滤会双触发，而 `concurrency` 的组用 `github.ref`，两个事件的 ref 不同、谁也掐不掉谁 | **改了。** `push` 限在 `branches: [main]`，分支上的改动交给 `pull_request` 盖。理由写在文件里。 |
| 3 | 缓存那一格实际是「只写一次、再不更新」（`actions/cache` 只在主 key 未命中时写回），而 `restore-keys` 的前缀又会把**旧工具链**建的 `target/` 拉回来，整棵重建、零复用 | **改了。** key 尾巴加 `github.run_id`（每趟必写回），`restore-keys` 的前缀补上 `Cargo.lock` + `rust-toolchain.toml` 的哈希（作废是 key 的行为，让 restore 也照着来）。两条理由都写在文件里。 |
| 4 | 去掉那条挂钟断言技术上站得住（它核了 `busy()` 就是 `inflight.len()`、只在每帧开头的 `drain()` 里减，剩下两条断言确定不看时序），但它踩了本票「不动任何 `.rs` 文件的行为」，且延迟从此没有把门的 | **在票里显式记了一笔**（上面「别顺手做的」底下那一节：破在哪、为什么破、代价是什么），连同挂单 `Q196`。不让它看起来像顺手带的。 |
| 5 | `xtask` 进 `members` 后 `cargo build --release` 会一并建它，在 `target/release/` 落一个非交付二进制，与 `xtask/Cargo.toml` 的注释矛盾 | **改了。** 加 `default-members = ["crates/core", "crates/cli", "crates/gui"]`——门禁四条都显式带 `--workspace`，**照样盖着 `xtask`**。 |
| 6 | `-j 0` / `--test-threads 0` 原样递下去，cargo 报「jobs may not be 0」，门禁表格印成 `红 clippy 0s`，把参数错误说成 clippy 红了 | **改了。** 两个开关都加 `value_parser` 下界 1。这条正中要害：门禁最不该撒的就是这种谎。 |
| 7 | 丢弃 crate 的 `Cargo.toml` 缺 `[workspace]`，`TMPDIR` 若落在某个 workspace 内，`cargo fmt` 会因「believes it's in a workspace」而红——红的原因与门禁无关 | **改了。** 加上 `[workspace]` 封口，理由写在测试里。 |
| 8 | 缺 `permissions:` 块，job 拿的是仓库默认 `GITHUB_TOKEN` 权限，而这趟一个 API 都不调 | **改了。** `permissions: {}`。 |

它另提两条判为「不必改」的：测试条数三处手抄（已是挂单 `Q198`）；
`cargo +stable xtask gate` 绕得过工具链的钉、而门禁本地不打印版本（**转挂单 `Q199`**，
连同「另一条路是加一行工具链横幅」与不加的理由）。

它逐条看过认为对、不构成问题的：四条命令的参数串与顺序、`--throttle` / `-j` /
`--test-threads` 的覆盖逻辑、`gate_job` 的退出码与「还有 N 条没跑」的点名、
`Step::display` 的环境变量引号、`run()` 的「起不来」分支、以及 `media.rs` 的
`Instant` / `Duration` 导入不会变成未用导入。
