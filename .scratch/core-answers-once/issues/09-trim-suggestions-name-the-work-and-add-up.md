# 09: 删减建议带作品名、算准数

**What to build:** 子库删减建议的每一行带作品名（正题，走全仓那一处取法）与平台。
计划器把作品级媒体记在**作品**名下，该作品的变体全被排除才算腾出；建议的「累计腾出」与「排除到这一项就能放下」跟着变成准数。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q852`：见 `../grill.md` 里那一条。
收挂单 `Q819`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 先写红测试：一个作品两个变体共用媒体，只排除其中一个时累计腾出不含那份媒体（今天含）
  - 证据：`crates/core/tests/sync_run.rs::作品共用的媒体_变体的没全排除就不算腾出_全排了才跟着算`。
    现场：两个变体（`库/FC/超级玛丽.zip` 4096、`库/FC/魂斗罗.zip` 2048）挂到同一个作品「魂斗罗」底下，
    一部作品级封面 2000 字节挂在**作品**锚点上，子库上限比「同步之后」少 5000 字节。
    在改之前那一趟红得看得见（把记账临时退回「按变体」重跑一次取证）：
    `assertion left == right failed: 只排除超级玛丽时魂斗罗还留着，封面照旧要铺——它不该算进腾出：
    Trim { variant: "库/FC/超级玛丽.zip", title: "魂斗罗", platform: Some("FC"), bytes: 6096,
    cumulative: 6096 }  left: 6096  right: 4096`。改完绿。
- [x] 两个都排除时才含
  - 证据：同一条测试的后半截：第二行 `bytes == 2048 + 2000 = 4048`、`cumulative == 8144`；
    `Room::of(&plan).fits_after() == Some(1)`（排除头一项只腾出 4096 < 超出量 5000，排到第二项才够）。
- [x] 删减建议行上有作品名与平台（界面测试）
  - 证据：`crates/gui/tests/sublibrary.rs::删减建议那几行写着作品名与平台`——每条建议的 `title` 与
    `「平台 · 变体的键」`都在画出来的字里，认得出作品的那一行写的是作品名「幻想传说」。
    红趟（把左栏退回只印键）报 `删减建议那一行没把平台「SFC」与变体的键写在一起`。
    另外两处照着同一份数据改：`crates/core/src/sublibrary/report.rs` 与 `crates/core/src/sync/report.rs`
    的文本报告改印「作品名（平台） · 键」（`sublibrary::trim_line`），`--json` 里 `title`／`platform` 跟着出来；
    既有那条 `超限时删减建议表四个数齐_…` 跟着改成新写法。
- [x] 子库屏删减建议那张截图基线重批
  - 证据：**不带** `UPDATE_SNAPSHOTS` 跑 `cargo test -p romcat-gui --all-features --test snapshot`，红的只有
    `子库_超限给删减建议_浅色` 与 `子库_超限给删减建议_暗色`（157 绿、2 红），别的 155 张一张都没动；
    重批之后 `git status` 里只有两张基线 png，`.new/.diff/.old` 全删掉了。
    对照材料（两张「旧｜新」并排图 + 为什么变 + 四个拿不准的样式问题）在
    `/Users/nicoer/dev/game-wt/logs/cao-09-compare/CHANGED.md`。
    表里那四个变体都认不出作品，于是四行印的都是**正题**（`库/SFC/圣剑传说 3 汉化版.zip` → `圣剑传说 3`）；
    认得出作品那一档由上面那条界面测试钉着。**样式待点头**：平台与键怎么摆、表头还叫不叫「变体」、
    名字用作品名还是显示标题——四个问题在 CHANGED.md 里，先照括号里那一档做了。
- [x] 门禁全绿（本机不分大小写的盘上 `sync_run` 那五条已知红除外，`Q479`）
  - 证据：日志 `/Users/nicoer/dev/game-wt/logs/cao-09-gate3.log`，头两行是 `/Users/nicoer/dev/game-wt/slot2`
    与 `q8/cao-09`，末行原文 `EXIT=0`，末行上面那句 `7 条全绿。`。七条原话：
    `绿 fmt 2s`、`绿 glossary 2s`、`绿 check 20s`、`绿 clippy 0s`、
    `绿 test 1033s`（`cargo test --workspace --all-features --no-fail-fast -j 2 -- --test-threads=2`，
    2,990 条、90 个测试目标）、`绿 numbers 3s`（`201/223 票`、`2,990 条测试`）、`绿 doc 8s`。
    这台机上没出现 `Q479` 那几条「跳过：…」的行（`grep -c '^跳过：'` 是 0），`sync_run` 照实跑绿了。
  - `gate1`（`/Users/nicoer/dev/game-wt/logs/cao-09-gate1.log`，`EXIT=1`）红过两条，都是这一票自己的：
    `doc`（`sync.rs` 两处文档链接带着多余的显式目标——`TrimPool` 与 `TrimName` 本地就能解析，去掉那个括号即好）
    与 `test` 里那条既有的 `超限时删减建议表四个数齐_…`（它按整行找变体的键，而那一行现在画
    「作品名 + 平台 · 键」，按新写法改）。`gate2` 跑到一半我自己掐掉了——那一趟开始之后我又改了
    一行悬停文字，那棵树已经不是要交的树了，于是整套重跑成 `gate3`。
  - 合并集成分支：`git merge q8/cao-int` 是 **no-op**（`Already up to date.`，cao-int 尖端仍是本票基点
    `f37723f`），所以没跑第二趟门禁。
