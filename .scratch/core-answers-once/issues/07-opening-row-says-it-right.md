# 07: 开场那一行说得准：四态、短话、根数、撞名的下一步

**What to build:** 开场上每一份中立库**四态**：开得了 ／ 结构版本对不上 ／ **读不动** ／ 文件坏了（ADR-0021 再修订）。读不动那一行的下一步是「去修权限，或关掉占用它的程序」。
版本对不上交出短话与细节：那一行只印短话，细节挂悬停。主库那一行加「N 个根」。
添加主库撞名时补下一步「换个名字，或者去开那一份」；名字空着那句不带路径。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q613`：见 `../grill.md` 里那一条。
收挂单 `Q841`：见 `../grill.md` 里那一条。
收挂单 `Q764`：见 `../grill.md` 里那一条。
收挂单 `Q615`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 先写红测试（照工作目录列举那份测试的写法）：收掉一份库文件的读权限 → 列举交出「读不动」（今天是「文件坏了」）
  证据：`crates/core/tests/catalog_list.rs::一份中立库的读权限被收掉时列举交出读不动`（收权限用
  `testing::revoke_file`，root 下造不出来时它印一行「跳过 …」并交 `None`）。先写的红是编译红
  （今天的树上没有那一支）：`error[E0599]: no variant named 'Unreadable' found for enum 'CatalogState'`
  ——`crates/core/tests/catalog_list.rs:473`。这一条同时钉住「那一行不叫删掉」：`!说的.contains("删掉")`。
- [x] 判不准的错误归「文件坏了」并照印原话
  证据：`crates/core/tests/catalog_list.rs::调用方分得开结构版本对不上与文件坏了` 里新加的那一段——
  一份「这不是一份 SQLite 数据库」的文件仍交 `CatalogState::Broken`，`said` 里带着系统原话
  （断言 `to_lowercase().contains("not a database")`）。分法只在一处：
  `CatalogError::unreadable()`（`crates/core/src/catalog.rs`），认的是 `SQLITE_PERM` /
  `SQLITE_CANTOPEN` / `SQLITE_BUSY` / `SQLITE_LOCKED` 四条，其余一律 `None`（判不准 → 文件坏了）。
  实测（本机 macOS，收掉读权限的那一份）：`SqliteFailure(Error { code: CannotOpen, extended_code: 14 })`；
  另一条连接 `BEGIN EXCLUSIVE` 占着时：`SqliteFailure(Error { code: DatabaseBusy, extended_code: 5 },
  Some("database is locked"))`。
- [x] 开场版本对不上那一行只有一行高，悬停看得到细节
  证据：`crates/gui/tests/snapshot.rs::开场_版本对不上那一行只有一行高_细节在悬停里`：量的是那一行底下
  那句小字有多高、与别的行底下那句比（< 1.5 倍）；再断言屏上只有短话、没有「会跟着丢的」；
  指针停在那一行上之后画出来的字里有「会跟着丢的有两样」与「手动加的例外」。
  红证据（把夹具那句退回整段长话）：`版本对不上的那一行比别的行高（136 点 对 17 点）`——六行对一行。
  两半的措辞在核心库一处：`CatalogError::Version` 的载荷是 `VersionMismatch`，
  `brief()` 给那一行（不带路径：左边已经印着这份库叫什么）、`details()` 挂悬停、`Display` 是
  「短话——细节」整句（终端里一个字不少，`crates/core/tests/catalog_list.rs` 那条版本测试新加的一段
  钉着路径、短话、细节三样都在整句里）。
- [x] 主库那一行写着根数
  证据：核心库 `CatalogFacts` 多交一个 `roots`（`crates/core/src/workspace.rs`，
  `crates/core/tests/catalog_list.rs::开得了的那一份说得出它有几个根`：两个根的那份库交回 `roots == 2`）；
  界面照稿的次序画成「2 个根 · 46,444 个变体 · 上次扫描 …」（`crates/gui/src/opening.rs::row_ui`，
  稿：`.scratch/gui-looks-like-the-design/prototype.html:845`）。屏上那一眼的证据在
  `crates/gui/tests/snapshot.rs::开场_读不动的那一份写着读不动_下一步是修权限` 里断言
  「2 个根 · 46,444 个变体」，基线 `opening/catalogs-{light,dark}.png` 也画着它。
  **保留：** 核心那一半是先改后写测试（红只到编译红「no field `roots`」这一层），界面那一半是先写红的
  （把那一行退回「N 个变体」之后 `开场_读不动的那一份写着读不动_下一步是修权限` 红在「开得了的那一行没写根数」）。
- [x] 撞名、空名字两句照新写法（向导测试里看得到）
  证据：核心 `crates/core/tests/library_name.rs::撞名与空名字那两句都说得出下一步`——
  `AlreadyExists` 那句话里有「换个名字」与「去开那一份」、且不含 `romcat `（不带命令）；
  `BlankLibraryName` 那句话里没有 `.sqlite3`、没有 `catalog`、也没有按空名字折出来的那条路径。
  界面（向导）：`crates/gui/tests/pick.rs::向导起名那一问撞名时说得出下一步` 与
  `::向导起名那一问空着时不带按空名字折出来的路径`（都是看**画出来的字**）。
  两条红证据（把核心那两句退回原话再跑）：`撞名那句没说出下一步：中立库 … 已经在了——建库不会去开现成的那一份`、
  `空名字那句里带着一条按空名字折出来的路径（.sqlite3）：中立库 …/catalog/library-cbf29ce484222325.sqlite3 的主库原名不能是空白…`。
- [x] 开场相关截图基线重批，拿主意的人点头；root 下跑不出读不动时如实跳过
  证据：变了 2 张、新增 2 张——`opening/catalogs-light.png`、`opening/catalogs-dark.png`（变了：
  根数 + 版本那一行从六行折成一行）、`opening/unreadable-library-{light,dark}.png`（新增：第四态第一次
  上屏）。对照材料：`/Users/nicoer/dev/game-wt/logs/cao-07-compare/CHANGED.md`（含「旧 ｜ 新」并排图
  `catalogs-{light,dark}-旧与新.png`，以及四处我拿不准的样式问题与先照哪个做的）。
  重批之前不带 `UPDATE_SNAPSHOTS` 跑过一趟：红的只有这四张，别的一张没动。**「拿主意的人点头」这一环
  挂在上面那份对照材料上，本票交回时还悬着。** root 下跑不出读不动的那一侧：两条收权限的测试都走
  `testing::revoke` / `testing::revoke_file`，造不出来就印一行「跳过 …」并如实返回（本机不是 root，
  两条都真跑了）。
- [x] 门禁全绿（本机不分大小写的盘上 `sync_run` 那五条已知红除外，`Q479`）
  证据：日志 `/Users/nicoer/dev/game-wt/logs/cao-07-gate2.log`，末行 `EXIT=0`，那七条：`fmt` 绿、`glossary` 绿、
  `check` 绿、`clippy` 绿、`test` 绿（1538s）、`numbers` 绿、`doc` 绿，底下「7 条全绿。」。全长零处
  `FAILED` / `panicked`；`sync_run` 那五条按 `Q479` 的说法在本机不分大小写的盘上**印「跳过：…」，是绿**。
  第一趟（同为这棵树这身改动）跑到 doc-test 收尾那一段时那个进程被外面掐掉了，没留下 `EXIT=` 那行——
  现场留在 `/Users/nicoer/dev/game-wt/logs/cao-07-gate1-truncated.log`（3000 多条测试零处红，就是没有回执那行）；
  为了让门禁有一份完整的回执，另起了一趟 `gate2`。

**记两条新挂单：** `Q1357`（「读不动」这个词不在词表里）、`Q1358`（读不动那一行的句子带着整串路径，
与版本那一行的取舍不一致，样式上待点头）。

## Comments

- 2026-10-28：门禁 `/Users/nicoer/dev/game-wt/logs/cao-07-gate2.log` 末行 `EXIT=0`，七条全绿（那七行的原话在
  末条验收里）。收尾前 `git merge q8/cao-int` 是 **Already up to date**（集成分支尖端仍是基点 `186fbfd`），
  于是没有「合完之后」的第二趟门禁。「集成分支上另有一趟门禁在跑」的那一趟与本票无关，没碰。
- 2026-10-28：两处**没做**的，写在这儿免得当成做了：一、`SiteError::Stranded` 那句里的 `said` 仍是
  `CatalogError::Version` 的**整句**（短话 + 细节）——那一句说的是「先别删，沉淀库还没弄好」，细节在那儿
  不多余，而它是开场上那一行的罕见一态，没有基线钉着；二、`Listing::Unreadable`（**整个目录**读不动）
  那一态没动，票面要的是「每一份中立库四态」，目录那一层 ADR-0021 早就分了。

**拿主意的人 2026-10-29 裁（样式四问）**：① 读不动那一行**留着**整串路径（照印核心库原话）；② 标签照旧叫「打不开」，不改 ADR 里的「文件坏了」；③ 主库那一行**根数在前**、变体数在后（照设计稿）；④ 那两张新基线（`opening/unreadable-library-{light,dark}`）**留着**。四处都照已做的来，基线不再重批。
