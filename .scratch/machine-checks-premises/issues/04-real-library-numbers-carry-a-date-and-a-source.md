# 04 — 真库那些数带日期与出处，消掉第二处复述

**What to build:** 机器碰不到的那些数（真库、工具链、耗时）**一律带日期与出处**，
而且**不许在第二处复述**。眼下 `docs/scale-reference.md` 与 `docs/library-facts.md`
**并存着一批打架的数**——后者把前者标成了「判断作废」，而前者原样躺着。

收挂单 `Q380`（不可核那一半）。
→ **收了**：不可核那一半就是下面五条。`Q380` 正文在 `.scratch/gui-self-sufficient/issues/02-*.md`，
第四轮收口时已标 settled；可核那一半（测试条数、目标数、票数、ADR 份数）归票 `03`。

**Blocked by:** 无 —— 可立即开工

**Status:** done

⚠️ **这一票不写代码，写的是判断**：哪些数留在哪儿、哪一处复述该删。判据两条：
① **不可核的数一律带日期与出处**（事实台账已经是这个形状，照它办）；
② **同一个数只许有一处出处**，别处要引用就指过去，不复述。

⚠️ **两份文件打架那一处要真裁掉一边**，不是两边各加一句注解——注解是把一条矛盾换成两条
（`docs/agents/domain.md` 那条通则说的正是这个）。

⚠️ 范围只到「机器碰不到的数」。会随票变的那四样归票 `03`，别重叠。

- [x] `docs/scale-reference.md` 与 `docs/library-facts.md` 那批打架的数**只剩一处**
  —— 裁掉的是 `scale-reference.md`「对设计的影响」前四条（条目量级 7 万、中文约 9%、街机 12522
  是第二大、PS1 10384 是最大的单一集合），换成一段指台账的话（台账开头那句「这份数据取代它们」
  盖着四条，街机与中文两条在「被推翻的判断」里逐条记着）；台账那边不再抄站点的 `12,522` 与 `9%`，
  改成指 `scale-reference.md`。复核：`grep -nE '12522|10384|7 万|约 9%|第二大|最大的单一' docs/scale-reference.md`
  只剩表里两格（站点数自己的出处）；`grep -cE '12,522|资源站的 9%' docs/library-facts.md` 为 0。
  为什么裁这一边、四条全删、没删整份：`Q502`。
- [x] 真库那些数处处带日期与出处 —— 台账每个 `##` 节标题都带（日期，出处）：原来缺的六节
  （规模、被推翻的判断、容器构成、疑似不该入库、其他、macOS 环境限制）按 `git blame` 补上
  「2026-08-31，首次库体检」（`c5fe695`；macOS 那节另写明全量计数出自 `docs/research/ntfs-mtime-precision.md`），
  「并发」补「2026-09-01，票 29」（`0b47cc7`），「按平台的变体数」补上票 10 与 `c2b8c72`。从 `platforms.md`
  搬进来的体检数写明最早记在 `02714b2`。复核：`grep -nE '^## ' docs/library-facts.md | grep -vE '（20[0-9]{2}-[0-9]{2}-[0-9]{2}'`
  为 0 行。台账开头加了「记账规矩」两条（标题带日期出处；别处指过来不抄），票 `queue-followups/09` 往下追加照它办。
  **保留**：代码注释里的真库数不带日期，本票不改（`Q500`）。
- [x] 全仓再找不到同一个真库数被复述在第二个文件里 —— 把两份文件里每个千分位数与带单位的数抽出来，
  逐个在 `README.md`、`CLAUDE.md`、`CONTEXT.md`、`docs/platforms.md`、`docs/agents/`、`rust-toolchain.toml`、
  `rustfmt.toml`、`gate.yml` 里 `grep`：改前 21 个数有复述（`README.md` 开头与「真库实测」那张表、
  `docs/platforms.md`「库体检之后的范围调整」一节），改后只剩 `69.4%`（`CONTEXT.md:77`，titledb 的调研数，
  不是真库数，`Q505`）。`platforms.md` 里台账没有的几个体检数搬进了台账（`Q503`）；台账
  「macOS 环境限制」的「约 1%」与 `docs/research/ntfs-mtime-precision.md` §6.2 的全量计数打架，裁掉「约 1%」；
  台账里同一趟票 07 写了两遍、两遍说的抽样平台不一样（GBA / GBC），删掉对不上表的那一份（`Q506`）。
  顺着同一条「不留两句互相顶的话」：`README.md` 开头「绝大多数是汉化版」改成「大量」（`Q504`）；
  `platforms.md` 里出自翻倍计数的三个 TOSEC 分平台条数删掉（`Q505`）；`README.md` 那段中文离线源覆盖率
  （94.2% / 99.7% / 83.8%）整段删掉——那句「眼下一条都没被取出来过」已不成立，三个数是规格里的（`Q505`）。
  **保留**：「全仓」按当前文档算，`.scratch/`、`docs/adr/`、`docs/research/` 当历史不改（`Q501`）；
  `.rs`、根 `Cargo.toml` 与编进二进制的 toml 里还有 55 个文件、128 行复述（`Q500`）；量级词不算数（`Q507`）。
- [x] 工具链版本、耗时那几处同样处理 —— `README.md` 不再抄 `1.98.1`、`rustfmt 1.9.0`、`1.95`、
  `edition 2024`，指 `rust-toolchain.toml` 与根 `Cargo.toml`；`rust-toolchain.toml` 那句「落这一版时实测」
  补上日期与提交（2026-09-08，`fc27c10`），它里面原先三处复述根 `Cargo.toml` 的 `1.95` 改成指 `rust-version`。
  耗时：`README.md`「两秒出结果」改成量级词（出处 `xtask/src/gate.rs:9`）；`CONTEXT.md`「折算真库 1.7 与 2.0 秒」
  改成指 `crates/core/src/stage.rs` 那两支的文档。同一族的历史计数「67 条 rustdoc 告警（58 / 8 / 1）」
  也改成指 `xtask/src/gate.rs` 的 `DOC_ENV`。复核：
  `grep -nE '1\.98|1\.95|rustfmt 1\.|两秒出|\*\*67\*\*|1\.7 与|秒级' README.md CONTEXT.md docs/*.md` 为 0 行，
  `grep -c '1\.95' rust-toolchain.toml` 为 0。**保留**：`gate.rs` 里那两个数自己没有日期（`Q500`）。
- [x] 票 `03` 管的那四样一处没碰 —— `git diff a14db7a -U0 -- README.md crates | grep -E '^[-+]' | grep -cE '88/90|1,815|1,692|123 条|24 份|70 个|59 张'`
  为 0；`crates/gui/Cargo.toml` 没碰。

## Comments

**范围与口径**（先定判据再动笔）：一个数若是机器碰不到的（真库、外部盘上跑出来的、工具链版本、
耗时、历史计数），它只许在一个文件里有值、那个文件里带日期与出处，别处用词指过去。
量级词不算一个数（`Q507`）。唯一出处定在：真库数 → `docs/library-facts.md`；
资源站的数 → `docs/scale-reference.md`；工具链 → `rust-toolchain.toml` 与根 `Cargo.toml`；
代码里量出来的耗时与计数 → 量它的那段文档注释。

**审查（`/code-review`，两轴）之后改的**：「MAME 排那边第二」（站点表里它其实最大）改成量级词；
`scale-reference.md` 那句指针原先说四条都记在「被推翻的判断」，改准；`rust-toolchain.toml` 里的 `1.95`；
`CONTEXT.md` 那句不再同时写「秒级」与「一两秒」；台账两节标题的出处写准；`Q504`、`Q505` 从「只记挂单」
改成真裁掉一边；`Q501` 拆出 `Q507`。

**挂单**：`Q500`–`Q507`。本票只改 Markdown 与 `rust-toolchain.toml` 的注释，没跑 cargo。
