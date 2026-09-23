# 03 — 建库变成一个显式的动作

**What to build:** 「打开」一份不存在的中立库时**当场报错**，不再顺手在盘上建一份出来。
要建库另有一个**明说在建**的入口，而且**它必然收一个名字**——于是不会再有一份建出来却没记住
名字的库。

- [x] 收挂单 `Q371`：建库的入口不止一个，`Catalog::open` 仍是「打开即创建」。
  - 已收：`Catalog::open` 只开不建，建库只剩 `Catalog::create`（见验收第 1、2、4 条）。

- [x] 收挂单 `Q469`（2026-09-13 拿主意的人裁：**空白名字当场报错**）：建库入口收到空白（含全是空格）的名字就报错，不再退回从文件名截——「不会再有一份建出来却没记住名字的库」由构造保证。命令行 `--library ""` 今天建得出库、退回从文件名截，钉它的那条测试（`名字是空白时退回从文件名截而不是留一行空的`）在本票跟着改；验收第 3 条「行为不变」对这一处不成立，按裁定改。票 `02` 已让改名函数 `Catalog::set_library_name` 收到空白就报错。
  - 已收：`Catalog::create` 碰盘之前问 `refuse_library_name`，空白报 `BlankLibraryName`，工作目录里一个文件都不多。那条测试**按裁定翻成** `crates/core/tests/library_name.rs::建库时名字是空白就当场报错盘上一个文件都不多`（`""`、`"   "`、`"\t\n"` 三种，先红：改之前建出了库）；命令行另钉 `crates/cli/tests/library_name.rs::扫描时给空白名字当场报错一份库都不建`（先红）。`BlankLibraryName` 那句话从「不能改成空白」改成「不能是空白」，建库改名共用。

- [x] 收挂单 `Q472`（2026-09-13 拿主意的人裁：**归本票**）：改名只换主库原名、找库仍认主库标识，于是改名之后 `scan --library <新名字>` 走「打开即创建」会**另建一份**，开场屏上两行同名。本票让「打开」不再顺手建之后，那一路改成报错说清；另在建库那个唯一入口查「同一个工作目录里主库原名不许重」，核心库的改名函数也照同一处判断查重（ADR-0024：一个判断一处实现）。
  - 已收：`refuse_library_name` 拿要落的名字与同一个目录里其余每一份库在开场屏上印的名字比（`workspace::namesake_beside`；「撞没撞」只在 `workspace::namesake_in` 一处比，名字取自与列举同一处 `listed_name`），撞了报 `LibraryNameTaken`，说清撞的是哪个文件；建库与改名都问它。`crates/core/tests/library_name.rs::同一个工作目录里主库原名重了建库当场报错不另建一份`（换一个工作目录同名照样建得出）、`改名成同一个工作目录里另一份库的主库原名时当场报错`（改成自己眼下的名字不算撞），命令行与界面各一条见验收第 4 条，全部先红。**改名之后按新名字找库也说清**：命令行各命令（`open_catalog` 与六处预问）与 `Site::open`（界面按名字启动、`triage` 那一组）共用 `SiteError::not_found`，给的名字恰是另一份库的主库原名时说「还没有 … 这份中立库——这个工作目录里主库原名叫「…」的是 <文件>」：`crates/core/tests/library_name.rs::改过名之后按新名字找库说清那个名字是哪一份库的主库原名`、`crates/cli/tests/library_name.rs::改过名之后拿新名字扫描或出报告都说清是哪一份而不另建一份`（两条先红）。命令行 `scan` 撞名时另补一句「`--library` 给它建库时的那个名字；要另建一份，起个别的名字」。比法的岔路记在 `Q521`。

**Blocked by:** **02 — 元数据表那一族键，每个一对具名函数**

**Status:** done

**门禁（2026-09-14，分支 `q5/nm-03-explicit-create`，`cargo xtask gate -j 3 --test-threads 3 --keep-going`）：** 5 条全绿，`EXIT=0`——fmt 1s、check 19s、clippy 22s（`--all-targets --all-features`）、test 664s（`--all-features`，70 个测试目标，1841 条通过、0 失败）、doc 6s（`-D warnings`）。日志 `/Users/nicoer/dev/game-wt/logs/slot-1-nm03-gate.log`。

⚠️ **让「建库时不记名字」在类型上就写不出来**（ADR-0024 那条推论）。新入口收名字是**必填**，
不是可选参数。

⚠️ **两条理由，写在票面上免得被当成洁癖**：① 眼下建得出中立库的地方**已经有两个**，
只有一个会记下名字；②「打开即创建」是那类「顺手」的接口，代价是**每个调用方都要自己
记得先问一句「在不在」**——现在正是这样，两处各问了一次，其中一处就是挂单 `Q411`。

⚠️ **波及面小**：调「打开」的只有三处，逐处分流即可。名字从票 `02` 那组具名函数里写进去，
别另起一条写入路径。

- [x] 「打开」一个不存在的中立库**报错**，且**盘上一个文件都不多**
  - 证据：`crates/core/tests/library_name.rs::打开一份不存在的中立库当场报错盘上一个文件都不多`——断言 `CatalogError::Missing`，且临时工作目录底下一项都没有（连 `catalog/` 目录都没建）。先红：改之前 `open` 顺手建出来了（「打开一份不存在的中立库该报错，却开出来了」）。`Catalog::open` 不带 `SQLITE_OPEN_CREATE`、不 `create_dir_all`；盘上有文件却连结构版本那一行都没有时报 `Version { found: 0 }`，不再顺手写上版本变成一份没名字的库；那一行在不在是切 WAL、建表之前先核的，核出来不在就一个字节都不写：`打开一份不是建好的中立库的文件报错一个字节都不写`（空文件，先红：改之前 `open` 往里写了表）。
- [x] 建库走那个明说在建的入口，**不给名字就编不过**
  - 证据：`Catalog::create(path: &Path, name: &str)`，`name` 不是 `Option`；`Catalog::open_named` 与私有的 `open_with` 删掉，盘上的库再没有第二条建出来的路（`open_in_memory` 只活在内存里，不在任何工作目录）。`建库时原名落进元数据表再开一次读得回来`、`建库时那份库已经在了就报错原先那一份一个字不改`（`create_new` 撞上已有文件报 `AlreadyExists`，原先那份名字与内容不动）。
- [x] 原先那三处调用方各自分流完，行为不变——**带保留**
  - 实测调用点比票面「三处」多（票面是写票时的数）：生产代码里开库的是 `crates/core/src/site.rs` `Site::at`、`crates/gui/src/roots.rs` 后台扫描那条线程、`crates/cli/src/main.rs` `refuse_writing_into_any_library`——这三处本来开的就是已有的库，照旧 `Catalog::open`，一行没改；建库的是命令行 `open_catalog` 与界面 `claim.rs` `开出那份现场` 两处。命令行分成 `open_catalog`（只开不建，不在就说 `SiteError::not_found` 那一句——没有撞上改过名的库时就是原来那句「还没有 … 这份中立库。先跑一次 `romcat scan`」）与 `open_or_create_catalog`（只有 `scan` 用），两条共用 `guarded_catalog_path` 折路径、守主库只读；向导改调 `Catalog::create`，建得成就说明本来不在，`本来不在 = path.exists()` 那一问随之去掉。`run_report` 与 `SubCommonArgs::open` 两处紧挨着开库的 `exists()` 预问去掉，另六处留着、说的那句话改成同一个 `SiteError::not_found`（挂单 `Q523`）。测试夹具里拿 `open` 建库的十来处改走 `create`（`testing::catalog_without_name` 造票 01 之前的无名旧库）。
  - 行为不变的证据：命令行整个 crate 15 个测试二进制全绿（`cargo test -p romcat-cli`），界面 lib 与 `program` / `roots` / `sublibrary` / `font_check` / `browse` 全绿，核心库 lib（970 条）与 `library_name` / `catalog_list` / `task` / `fixture_library` / `stage` 全绿。
  - **保留**：① 按裁定变了的两处——`--library ""` 当场报错（`Q469`）、同一个工作目录里主库原名撞了报错（`Q472`），见下面两条收挂单；② `romcat names`（`run_names_recheck`）从前不先问在不在，给一个不存在的 `--library` 会顺手建出一份空库，现在说「还没有 … 这份中立库」——那是本票要治的毛病的又一处实例；③ 查重名把没记过名字、打不开的库从文件名截的那一半也算上（`Q521`），于是不给 `--library` 扫两块末级目录同名的盘，第二块会报错；④ 按名字找不到、而那个名字恰是另一份库的主库原名时，报错那句话多说一句是哪一份（`Q472` 的「报错说清」），命令行各命令与 `Site::open` 同一句。
- [x] 命令行与界面两条建库的路都走同一个入口（没有第二份实现）
  - 证据：命令行 `open_or_create_catalog` 与界面 `Wizard::开出那份现场` 都调 `Catalog::create`；名字收不收只在 `crates/core/src/catalog.rs` `refuse_library_name` 一处判，改名 `set_library_name` 问的也是它。两边撞同一件事时屏上是核心库同一句话：`crates/cli/tests/library_name.rs::改过名之后拿新名字扫描或出报告都说清是哪一份而不另建一份`、`crates/gui/tests/program.rs::向导起的名字撞上一份改过名的库时开始扫描那一下拦下而不另建一份`（两条先红：改之前 `open_named` 先把文件建出来才报错，工作目录里多一份库）。
- [x] 建出来的库照旧记着 **主库原名**（现成行为，反向钉住）
  - 证据：`crates/core/tests/library_name.rs::建库时原名落进元数据表再开一次读得回来`、`名字折不进文件名时元数据表里仍是原名`、`名字被滤光时文件名退成那个固定词而元数据表里仍是原名`；`crates/cli/tests/library_name.rs::扫描那一趟也印得出主库原名`；`crates/gui/tests/program.rs::在开场上认领一个新主库走完向导就进主窗口`（标题是人起的名字）与 `向导建出来的库与命令行扫出来的库一样命令行接得上`。

## 挂单裁决（第五轮收口，2026-09-23）

从 `.scratch/PARKING-LOT.md` 迁来（`/settle`，范围 `Q449`–`Q1185`）。每条顶上多一行**裁决**：落哪一档、去了哪儿。
编号 `Qn` 留着占号、不复用。

### Q521 — 查主库原名撞没撞，比的是开场那一行印的名字：没记过名字的库从文件名截的那一半、打不开的库也算

- **裁决（第五轮收口，2026-09-23）：** **岔** —— 实现走的就是条目建议的那条；追认：维持现状（2026-09-23 拿主意的人整批追认）。

- **来自：** 票 `no-mute-spots-opening-a-catalog/03`
- **类别：** 规格没说
- **在哪：** `crates/core/src/catalog.rs` `refuse_library_name`；`crates/core/src/workspace.rs` `namesake_in` / `listed_name`
- **为什么没停线：** `Q472` 的裁决只说「同一个工作目录里主库原名不许重」，没说退回来的那个名字算不算；换一种比法是改 `refuse_library_name` 里那一处过滤。
- **这张票实际做了什么：** 要落的名字（NFC）与同一个目录里其余每一份库在开场屏上印的名字逐字比：记着原名的比原名，没记过的（票 01 之前建的）比从文件名截出来的那一半，打不开的（结构版本对不上）也比文件名那一半——列举与查重问的是同一处 `listed_name`。于是不给 `--library` 扫两块末级目录同名的盘（`/Volumes/甲/Game` 与 `/Volumes/乙/Game`），第二块当场报错，要人用 `--library` 另起一个名字。
- **另一条路：** 只比元数据表里真记着的那一行，退回来的名字不算——没起过名字的库本来就「没起过名字」，撞上它不该挡人。
- **建议留哪条：** 留这条。`Q472` 要治的是「开场屏上两行同名，人分不出哪份是哪份」，而开场屏印的正是退回来的那个名字；只比记着的那一行，两块盘照样印成两行 `Game`。报错那句话说清了撞上的是哪个文件，命令行还补了一句用 `--library` 另起名字。另记一笔：词表**主库原名**条写的退路是「主库根的末级目录名」，而 `Catalog::library_name` 从票 01 起退回的是中立库文件名可读的那一半（不给 `--library` 时那一半是末级目录名滤剩的 ASCII 字母数字）——两者的出入是票 01 留下的，本票比的是屏上真印出来的那个。
- **谁来裁：** 拿主意的人
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q522 — 建库落名字没调 `set_library_name`：名字在 `create` 碰盘之前判一次，建表那一步直接落 `MetaKey::LibraryName`

- **裁决（第五轮收口，2026-09-23）：** **记** —— 另一条要么开两遍库要么留残文件，稻草人。记下，无事可做

- **来自：** 票 `no-mute-spots-opening-a-catalog/03`
- **类别：** 票写错了
- **在哪：** `crates/core/src/catalog.rs` `Catalog::create` 与 `prepare` 里 `(None, Birth::Created(name))` 那一支；票面「名字从票 `02` 那组具名函数里写进去，别另起一条写入路径」
- **为什么没停线：** 写的是同一个键、过的是同一处判断，翻成调具名函数是改一行。
- **这张票实际做了什么：** `create` 先问 `refuse_library_name`（空白、撞名），过了才建目录与文件；建表那一步 `meta_set(MetaKey::LibraryName, name)`。`set_library_name` 是同一处判断加同一个 `meta_set`。
- **另一条路：** 照票面字面，建表那一步调 `set_library_name`。它会再判一次，建一份库就要把同目录里其余每一份库只读地开两遍；要是只让它判、`create` 不先判，拒收的那一次盘上已经多了一个文件，得回头删。
- **建议留哪条：** 留这条。票面那句写在票 `02` 落地之前，防的是「按字符串键另写一条」，而 `MetaKey` 已经把键名收成一处；ADR-0024 要的是**判断**只有一处，它在。
- **谁来裁：** 收尾
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q523 — 命令行只去掉了两处紧挨着开库的「在不在」预问，另外六处留着

- **裁决（第五轮收口，2026-09-23）：** **岔** —— 实现走的就是条目建议的那条；追认：维持现状（2026-09-23 拿主意的人整批追认）。

- **来自：** 票 `no-mute-spots-opening-a-catalog/03`
- **类别：** 规格没说
- **在哪：** `crates/cli/src/main.rs`：去掉的是 `run_report` 与 `SubCommonArgs::open`；留着的是 `run_shape`、`run_identify`、`run_scrape`、`run_titles`、`run_import`、`run_export` 里的 `if !catalog_path.exists()`
- **为什么没停线：** `open_catalog` 如今只开不建，不在就说 `SiteError::not_found` 那一句，六处预问说的也是它，已经不承重；删不删只影响两样都缺时先报哪一句。
- **这张票实际做了什么：** 去掉的两处，预问与 `open_catalog` 之间什么都没有，删了行为一个字不变；留着的六处，预问排在读平台清单、查 DAT 库、读优先级表、查报告落点这些判据前面，删了之后「库也没有、DAT 库也没有」时先报的会换成后者。六处预问里那句话原先各抄一份字面量，现在都改成 `SiteError::not_found`——与 `open_catalog`、`Site::open` 同一句，改名之后按新名字找库时说清是哪一份。
- **另一条路：** 六处一起删，全靠 `open_catalog` 那一句——「每个调用方都得记得先问一句」这件事在命令行上一处不剩。
- **建议留哪条：** 留这条。票的验收第 3 条要「各自分流完，行为不变」，六处的报错次序有人可能照着写过脚本；它们不再承重，日后哪张票碰到那几条命令顺手删掉就是。
- **谁来裁：** 收尾
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q524 — 添加主库那条向导第一步照旧只查主库标识占没占、自己判空白，撞主库原名要到「开始扫描」那一下才由建库入口拦下

- **裁决（第五轮收口，2026-09-23）：** **记** —— 已裁归票 04 并落地。已落地：票 no-mute-spots-opening-a-catalog/04

- **来自：** 票 `no-mute-spots-opening-a-catalog/03`
- **类别：** 规格没说
- **在哪：** `crates/gui/src/claim.rs` `Wizard::named`（`catalog_path(..).exists()`，挂单 `Q411` 的形状；另有一处自己的 `trim().is_empty()`，是 `is_blank_name` 之外的第二个空白判断）与 `Wizard::开出那份现场`；ADR-0023「重名在第一步就拦」
- **为什么没停线：** 拦是拦得住的（`crates/gui/tests/program.rs::向导起的名字撞上一份改过名的库时开始扫描那一下拦下而不另建一份`），差的只是拦在第一步还是第二步；挪到第一步是界面那一层多问一句。
- **这张票实际做了什么：** 向导只做分流：建库那一下改调 `Catalog::create`，撞了原名时屏上是核心库那句原话，一份库都不多；第一步一行没动。
- **另一条路：** 向导第一步起完名就问核心库：撞没撞主库原名问 `workspace::namesake`（本票已公开，找库报错在用），空不空问建库入口那一处空白判断（眼下是私有的 `is_blank_name`，得公开出来），向导里那份 `trim().is_empty()` 删掉。
- **建议留哪条：** 留另一条，但不在这张票做：票 `no-mute-spots-opening-a-catalog/04` 与 `gui-looks-like-the-design/05` 紧跟着要改同一片向导代码，派活说明要本票向导那边只做分流；`Q411`（「这个主库名被占了」长在向导里）也在同一处，一起收。
- **谁来裁：** 拿主意的人
- **裁决（2026-09-14，拿主意的人）：** 归票 `no-mute-spots-opening-a-catalog/04`，正文已追加「收挂单 `Q524`」。
- **状态：** settled
- **票 04 已落地：** 向导第一步 `Wizard::named` 只问核心库新加的 `Catalog::refuse_create`——名字收不收问的就是 `refuse_library_name`（空白、撞主库原名），另先问那份在不在、目录写不写得进，一个字节都不写；向导自己那份 `trim().is_empty()` 与 `catalog_path(..).exists()` 删掉。空白与撞主库原名在第一步就拦下，屏上是核心库原话（`crates/gui/tests/program.rs::向导起名留空时说的是核心库那句空白`、`向导起的名字撞上一份改过名的库时起名那一步就拦下`，都先红）。措辞少了「换个名字」半句，记在 `Q615`。
