# 03 — 开场：列出这个工作目录里的库，开一份进去

**What to build:** 双击图标、一个参数都不给，看见的是**开场**——这个**工作目录**里有哪些**中立库**，一份一行：主库名、多少个**变体**、上次扫描是什么时候。这些数住在中立库里，**盘没挂上照样看得见**。选一行进主窗口，开场交出**现场**之后自己退场，窗口标题写着开的是哪一份。

**结构版本对不上的旧库照列不误**，当场说清"版本 X，本程序认 Y，删掉重扫"——从列表里静静消失才是最难查的那种错。工作目录里一份库都没有时，直说"还没有库，认领一个主库开始"。

开场上能换一个工作目录，换完立刻看见那个目录里的库——不换的话，那些不在默认位置的库一份都开不出来。

核心库这一侧要新增"列出一个工作目录里有哪些中立库"：现在一个列举函数都没有，所有开库的路都是"给我名字或路径，我去折一个出来看在不在"。

**Blocked by:** 01（主库名要从元数据表读）、02（开场是顶层类型的第一态）

**Status:** done

- [x] 不带参数启动看见开场，列出当前工作目录里的中立库
      —— `gui/tests/program.rs::一个参数都不给看见的是开场列着这个工作目录里的库`
      （`Program::start` 在 `!locate.given()` 时交出 `Stage::Opening`，不再报错退出）；
      核心那一侧 `core/tests/catalog_list.rs::列出这个工作目录里的中立库`。
- [x] 每行显示主库名、变体数、上次扫描时刻
      —— 同上那条 GUI 测试三样各断一次（「我的主库」／「3 个变体」／`human_time(1_700_000_000)`）；
      数由 `workspace::CatalogFacts { variants, scanned_at }` 交出，
      `core/tests/catalog_list.rs::变体数与上次扫描时刻盘没挂上照样交得出`。
- [x] 外置盘不在位时这三个数照样显示
      —— `core/tests/catalog_list.rs::变体数与上次扫描时刻盘没挂上照样交得出`：根指着
      `/没挂上的那块盘/主库`，三个数全从中立库里折出来（ADR-0009）。GUI 那几条的夹具
      `建一份像样的库` 用的是同一个不存在的根，所以开场那一屏也是在盘不在位的前提下画出来的。
- [x] 主库名读不到时退回从文件名截取（承 01 的退路），不空着也不显示哈希串
      —— `core/tests/catalog_list.rs::主库名读不到时退回从文件名截既不空着也不是那串哈希`
      （退路本身在票 01 的 `Catalog::library_name` 里，这一条钉的是列举真的走了它）；
      屏上那一半由 `gui/tests/program.rs` 那条反向断言 `!屏上.contains(&主库名(&库文件))` 钉着。
- [x] 结构版本对不上的库照列，并说清是哪个版本对哪个版本、该怎么办
      —— 核心与界面各一条同名测试 `结构版本对不上的库照列并说清是哪个版本对哪个版本`，
      断的是「结构版本是 4」「本程序认得的是 7」「删掉它重扫一遍」。**措辞没另造**：
      原话出自 `CatalogError::Version` 一处，`Catalog::open_read_only` 只核对不建表不改版本，
      那份库的内容一个字节都没被动过。夹具 `testing::catalog_at_version`。
- [x] 选中一行进主窗口，窗口标题写着那一份库
      —— `gui/tests/program.rs::在开场上选中一行就进主窗口标题写着那一份库`：点「打开」→
      顶栏五屏画出来 → 标题含「我的主库」**且不含**那串带哈希的主文件名。
      标题原先写的是标识符（`Site::library`），本轮就地改掉（原 `Q386`，已 closed）：
      核心新增 `Site::display_name()`，`App::new` 改用它，**`App` 的公开签名一个字没动**。
- [x] 工作目录里一份中立库都没有时，说"还没有库，认领一个主库开始"
      —— `gui/tests/program.rs::工作目录里一份库都没有时开场说还没有库认领一个主库开始`
      （断的是字面量，不引 `opening::NO_CATALOG`）；核心那一半
      `core/tests/catalog_list.rs::一份中立库都没有时列出来是空的而且不留下任何东西`
      顺带钉住「列一遍不把目录建出来」。
- [x] 在开场上换一个工作目录，立刻列出那个目录里的库
      —— `gui/tests/program.rs::在开场上换一个工作目录立刻列出那个目录里的库`：
      往那个框里真打字、真点「换过去」，换完甲那边的库当场不在屏上、乙那边的在。
      **保留**：眼下只能贴路径，没有目录选择器（挂单 `Q387`——这个仓库一个文件对话框
      依赖都没有，引不引是拿主意的人的事）。
- [x] 列举一个目录时不因为其中某一份库打不开而整屏失败——那一份单独标出来，其余照列
      —— `core/tests/catalog_list.rs::一份读不出来的库不连累其余`（一份不是 SQLite 的文件
      与一份好库并排，两份都列得出，坏的那份 `openable` 为假）；界面那一半由版本对不上
      那条同时断「好的那一份照列、它那几个数也画出来了」。
      另外 **`openable` 与 `facts` 是分开的两件事**（code review 的 Spec 轴指出，当场改的）：
      一份开得动、只是某次查询没读回数来的库不会被画成按不下去的坏库——把「这一行的数缺了」
      误判成「这份库开不了」，正是这一条要防的那种病。

## 收尾

**门禁**：`cargo xtask gate --keep-going` 在 `ticket/gui-self-sufficient-03` 上跑了一趟，
**五条全绿**——fmt 1s ／ check 16s ／ clippy 14s ／ test 658s ／ doc 8s。
`--all-features` **1,750 条**（69 个测试目标，0 失败），基点 `f5a98f9` 是 1,738，
差 12 条正是本票新增的 7（`core/tests/catalog_list.rs`）＋ 5（`gui/tests/program.rs` 净增）。
墙钟比 `main` 上那 9m00s 长，是因为同时另有一棵树在跑门禁（队列宽度 2 的设计点）。

**交给下游的两样：**

- **开场那一屏怎么再次进入**（票 `04`／`05` 要）：公开构造子
  `romcat_gui::program::Program::opening(workspace: PathBuf) -> Program`。
  开场本体是 `romcat_gui::opening::Screen`，`Screen::ui(&mut self, ui) -> Option<Chosen>`，
  交出 `Chosen { site, workspace }`；换目录走 `Screen::look_at(PathBuf)`。
  「认领新主库」的位子在 `workspace_ui` 那一行（代码里点名留给票 `05`）。
- **核心库那个列举函数**（票 `05` 拿它查重名）：
  `romcat_core::workspace::catalogs(&Path) -> Vec<CatalogEntry>`，
  `CatalogEntry { path: PathBuf, name: String, openable: bool, facts: Result<CatalogFacts, String> }`，
  `CatalogFacts { variants: u64, scanned_at: Option<i64> }`。按主库名的码位排、同名再按路径排。

**挂单**：`Q386`–`Q393` 八条，其中 `Q386`（标题写的是标识符）本轮**就地做掉、状态 closed**，
其余七条 open：`Q387` 没有目录选择器、`Q388` 目录读不动当空目录、`Q389` 按码位排、
`Q390` 回 `Q382`（删 `impl eframe::App for App` 与本票硬约束顶着）、`Q391` 进屏要开每一份库、
`Q392` 「还没扫过」两处措辞、`Q393` 错误压成 `String`。
