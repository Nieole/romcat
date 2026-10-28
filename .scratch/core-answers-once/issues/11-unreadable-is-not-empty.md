# 11: 读不动不压成没有：打开现场、守卫、详情页三行

**What to build:** - 打开现场时工作目录读不动，说读不动，不说「还没有这份库，先跑一次 scan」。
- 「不许写进任何一份主库」那道守卫查不出主库的根时**只放行新建**：输出落点已存在就拒绝，并说明是因为查不出主库的根（ADR-0004 修订）。
- 作品详情页「子库」「导出」三行读不到时画「读不动」，不画「—」「还没导出」。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q614`：见 `../grill.md` 里那一条。
收挂单 `Q616`：见 `../grill.md` 里那一条。
收挂单 `Q996`：见 `../grill.md` 里那一条。
收挂单 `Q1450`（2026-10-02 拿主意的人裁）：作品详情页头上「平台」那一格允许折成两行——平台全名一律带厂名之后「Nintendo Super Famicom」一行放不下、被截成「Nintendo Super F…」；别的几格照旧一行。改完重批受影响的 `work/*` 基线（编排者对稿、拿主意的人点头）。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 先写红测试（命令行）：中立库目录收掉权限后，导出沉淀库到**已存在**的文件被拒、原文件字节不变、话里说明原因（今天会覆盖）
      红：`crates/cli/tests/triage.rs::中立库目录读不动时导出只放行新建_已存在的拒掉且一个字节不动` 在这棵树上是红的，红的正是「今天会覆盖」——`目标已经在了，查不出主库的根时该拒：已导出 0 条裁决到 …/分享.json。`（这一趟照写覆盖了）。
      绿：同一条测试现在过：拒绝时 stderr 里有「查不出主库的根」与「读不动」，`fs::read(&原先的)` 与原来那些字节逐字相等。收权限造不出来（不是 Unix、或者跑测试的是 root）时 `romcat_core::testing::revoke` 印一行「跳过：…」并如实返回。
- [x] 同样条件下导出到新文件照样成功
      同一条测试的后半：`--out` 指到一个不存在的 `新的分享.json` 时退出码是 0，文件落了盘（`write_out` 走 `create_new`）。
- [x] 工作目录读不动时打开现场交出「读不动」
      红：`crates/core/tests/catalog_list.rs::工作目录读不动时打开现场说的也是读不动不是还没有这份库` 在那棵树上红的是 `读不动被说成了「还没有这份库」：还没有 --library 锁着的库 这份中立库。先跑一次 romcat scan。`
      绿：`Site::open`（与同形的 `Site::open_file`）的「在不在」改成 `try_exists`，说不出来时交 `SiteError::Unreadable`——「中立库住的那个目录读不动：…（系统怎么说的）。说不准里头那份中立库在不在——先去修那个目录的权限，或者关掉占着它的程序」。同一份测试还钉了「那句话里说得出是哪个目录」。
- [x] 详情页三行读不到时画「读不动」（界面测试）
      红：`crates/gui/tests/work.rs::详情页三行读不动时画读不动_不画破折号与还没导出` 在那棵树上是 0 处「读不动」（`left: 0 right: 3`）。
      绿：同一条测试现在过——「收藏」「子库」「导出」三行都画「读不动」，屏上不再有「还没导出」「未收藏」那两段。造的是真的读不动：从另一条连接把 `variant` 与 `export_entry` 两张表改名，逼这一页重读一趟（`Screen::forget_page`）。三行改交三态 `Readable`（`crates/gui/src/browse/work.rs`）。
- [x] root 下跑不出读不动时如实跳过并印一行
      两条收权限的测试（核心库那条、命令行那条）都用 `romcat_core::testing::revoke`，它造不出来时印「跳过：…」并交 `None`，测试于是如实返回（与 `Q479` 同一个规矩）。本机这两个测试真跑到了「读不动」那一支（不是跳过）。
- [x] 门禁全绿（本机不分大小写的盘上 `sync_run` 那五条已知红除外，`Q479`）
      日志 `/Users/nicoer/dev/game-wt/logs/cao-11-gate1.log`：`fmt`／`glossary`／`check`／`clippy`／`test`／`numbers`／`doc` 七条全绿（`test` 1505s），末行 `EXIT=0`。本机不分大小写的盘上 `sync_run` 那五条照旧如实跳过（`Q479`），不是这一票的回归。收尾前 `git merge q8/cao-int` 是 no-op（集成分支尖端仍是 `186fbfd`，与这一票的基点同一个），所以没有第二趟门禁。

另外收了票正文追加的 `Q1450`：`crates/gui/src/browse/work.rs` 的 `hero_facts` 里「平台」那一格按词折行、两行封顶（`LayoutJob` 的 `wrap.max_rows = 2`），别的五格照旧一行截尾巴。测试 `crates/gui/tests/work.rs::头上平台那一格折成两行看得全_别的几格照旧一行`（红时 `left: Some(1) right: Some(2)`）；受影响的 12 张 `work/*` 基线重批（`work/sticky-*` 两张没动，那时头上那一块已经滚出去了），对照材料在仓库外 `/Users/nicoer/dev/game-wt/logs/cao-11-compare/`（`CHANGED.md` 与十二张 `旧｜新`），拿主意的人点头那一环挂在编排者那边。
