# 02 — 元数据表那一族键，每个一对具名函数

**What to build:** 维护者**给已经建好的库改得了名**——他起错了名字不必删库重来。
而读代码的人看得出元数据表那一族有哪些键、各自什么意思。

收挂单 `Q370`：元数据表那一行没有公开的写入口，已有的库改不了名。
—— **已收（2026-09-13，提交 `3fd7bb8`）**：`Catalog::set_library_name` 与 `Catalog::library_name` 成对；元数据表那一族的键名收进私有枚举 `MetaKey`（`crates/core/src/catalog/meta.rs`）。正文在 `.scratch/gui-self-sufficient/issues/01-library-name-lands-in-the-catalog.md` 的 `### Q370`，那份历史记录不改。本票自己的岔路口见挂单 `Q469`–`Q472`。

**Blocked by:** **01 —（prefactor）`主库标识` 与 `主库原名` 正名**

**Status:** done

全量门禁（2026-09-13，在提交 `486326e` 上跑，Q469 裁决的改动已在里面）：`TMPDIR=/Users/nicoer/dev/game-wt/cs/tmp cargo xtask gate -j 3 --test-threads 3 --keep-going`，日志 `/Users/nicoer/dev/game-wt/logs/slot-1-nm02-gate2.log`。fmt、check、clippy、test、doc 五步全绿，`EXIT=0`；test 那一步 70 个测试目标全部 ok，1823 条通过、0 条失败。

⚠️ **每个键一对具名函数，不做通用的按键读写面。** 通用面把「这一族有哪些键、各自什么语义」
变成一个**字符串约定**，而键名会被两处各写一遍——ADR-0024 反对的正是这个。
那一族现在四个键（**主库原名**、上次折标题、上次导出、导出目录），四个键八个函数，不多。

⚠️ **「改名」是它的第一个用例**，而且 **改名不动任何一条路径锚**——锚认的是**主库标识**，
不是**主库原名**。这一条要有测试反向钉住。

⚠️ 挂单当初把这一条托给了票 `08`，而 `08` 来了、往同一张表写了两个键、**没收它**。
这一批就是那次失灵的清账。

- [x] 已经建好的库**改得了名**，改完开场屏与报告上印的是新名字
      —— `crates/core/src/catalog.rs` `Catalog::set_library_name`。核心库 `crates/core/tests/library_name.rs::已经建好的库改得了名开场屏与报告上印的是新名字`：先红（`E0599 no method named set_library_name`），后绿。改名之后开场屏的列举（`workspace::catalogs`）那一行名字是新的、文件路径没变，`Site::display_name` 是新名字，库里的记录一条不少。报告那一侧：命令行 `crates/cli/tests/library_name.rs::改过名的库报告印的是新名字`（`romcat report` 的 stderr 里有一行 `主库：改过的名字`）。**保留**：找库仍按起先那个名字，`--library <新名字>` 找不到这份库，`scan --library <新名字>` 会另建一份（挂单 `Q472`，已裁：归票 `03`）；界面入口不在本票（`gui-looks-like-the-design/31`）；界面窗口标题在开现场时取一次（`crates/gui/src/app.rs` `library_label`），改名之后要重开才换，那也是 `31` 的事。
- [x] 改名之后**路径锚一个字节没动**，沉淀库里的裁决照旧对得上
      —— `crates/core/tests/library_name.rs::改名之后路径锚一个字节没动沉淀库里的裁决照旧对得上`：字面量 `主库-f5c61109e92b6036` 取自正名之前的代码，不是照算法重算的。先在沉淀库里按这串字节落一条路径锚，再改名。之后按名字（`Site::open`）与按文件（`Site::open_file`）两个入口开现场：`library_identity` 逐字节等于它，`Store::joined` 认得出那条锚（`["收藏"]`），`display_name` 是新名字。上一条测试另断言中立库文件路径没变。
- [x] 四个键各有一对具名函数，每个带自己的文档
      —— **带保留勾**：实测是七个键加结构版本，不是四个（挂单 `Q470`，等收尾裁）。
      - 主库原名：`library_name` / `set_library_name`
      - 上次折标题：`titles_folded_at` / `mark_titles_folded`
      - 上次导出：`exported_at` / `mark_exported`
      - 前端格式 + 导出目录：两个键合用 `export_setup` / `set_export_setup`
      - 成型那两个键：各一个读函数 `shaped_scan` / `shaped_manifest`，写在 `replace_variants` 那一趟里一起落
      
      每个键在 `crates/core/src/catalog/meta.rs` `MetaKey` 的变体上有自己的文档，写明读写它的函数。键名的字节有测试反向钉住：`元数据表那一族的键名一个字节没动旧库记下的账照样读得回来`，把 `exported_at` 变异成别的字串就变红，还原后变绿。
- [x] 没有通用的按键读写面暴露出去
      —— `mod meta;` 是私有模块；`meta_get` / `meta_set` 是 `pub(super)`，只收 `MetaKey` 不收字符串（起初还有一个 `meta_clear`，`Q469` 裁成报错之后没了调用方，已删）。按字符串键读写的旧路已删：`grep -rn -E "META_|key = 'schema_version'" crates/core/src/catalog.rs crates/core/src/catalog/` 零命中。别的库（DAT 库、`titledb`、`zh` 各自的 `meta` 表）不是中立库，没动。
- [x] 读不到那个键时照旧退回从文件名截（现成行为，反向钉住）
      —— 三条现成测试动手前在基点 `04dc34c` 上跑绿（日志 `/Users/nicoer/dev/game-wt/logs/slot-1-nm02-slice1.log`，`EXIT=0`），改完照旧绿：`crates/core/tests/library_name.rs` 的 `票01之前建的库读不到那一行时退回从文件名截` 与 `名字是空白时退回从文件名截而不是留一行空的`，`crates/core/tests/catalog_list.rs::主库原名读不到时退回从文件名截既不空着也不是那串哈希`。**改名**成空白不走这条退路：挂单 `Q469` 裁成当场报错，`改名成空白时当场报错那一行与路径锚都没动` 钉着——先红（`E0599 no variant named BlankLibraryName`）后绿，断言报的是 `CatalogError::BlankLibraryName`、那句话里说了「空白」、元数据表那一行与路径锚都没动。建库那一路给空白名字照旧退回从文件名截（上面第二条现成测试一个字没改），改它归票 `03`。

## 挂单裁决（第五轮收口，2026-09-23）

从 `.scratch/PARKING-LOT.md` 迁来（`/settle`，范围 `Q449`–`Q1185`）。每条顶上多一行**裁决**：落哪一档、去了哪儿。
编号 `Qn` 留着占号、不复用。

### Q469 — 改名成空白：抹掉主库原名那一行、退回从文件名截，而不是当场拒收

- **裁决（第五轮收口，2026-09-23）：** **记** —— 已裁空白报错，票 02/03 都已落地。已落地：票 no-mute-spots-opening-a-catalog/02 与 /03（BlankLibraryName）

- **来自：** 票 `no-mute-spots-opening-a-catalog/02`
- **类别：** 规格没说
- **在哪：** `crates/core/src/catalog.rs` `Catalog::set_library_name`；测试 `crates/core/tests/library_name.rs::改名成空白时退回从文件名截而不是留一行空的`
- **为什么没停线：** 票与规格只说「改得了名」，没说空白怎么算；两条路都不碰锚，翻过来是改一处。
- **这张票实际做了什么：** 空白（含全是空格）→ 抹掉那一行，读的时候退回从文件名截；建库那一趟记名字也改走这个函数（`Catalog::prepare`）；「算不算空白」收成 `catalog.rs` 里私有的 `is_blank_name` 一处，读写两侧都问它。
- **另一条路：** 空白当场报错（`CatalogError` 加一个变体），改名框里清空了名字按确定，屏上说一句「名字不能是空的」；票 `03` 的建库入口调它，空白名字就建不出库。
- **建议留哪条：** 留这条，**但要在票 `03` 开工之前裁**——它就是那个入口要调的函数。理由：词表**主库原名**条「没起过名字时退回主库根的末级目录名」，空白就是「没起过名字」，有定义好的样子；命令行 `--library ""` 今天建得出库、退回从文件名截（`名字是空白时退回从文件名截而不是留一行空的` 钉着），而票 `03` 验收要「原先那三处调用方各自分流完，行为不变」。**代价说清**：在这条路上，票 `03` 那句「不会再有一份建出来却没记住名字的库」**不是类型保证的**——空白名字照样建得出一份退回从文件名截的库。要它由构造保证，就得选另一条路，并且改掉上面那条测试钉着的命令行行为。界面想在框里就拦空白，是屏上多一句提示，两条路都不必为它改。
- **谁来裁：** 拿主意的人
- **裁决（2026-09-13，拿主意的人）：** 选另一条路，**空白名字当场报错**。`Catalog::set_library_name` 收到空白（含全是空白字符）返回 `CatalogError::BlankLibraryName`，元数据表那一行一个字不动；「算不算空白」照旧只在 `is_blank_name` 一处。**建库那一路先不改**：`Catalog::prepare` 收到空白名字照旧不写那一行、读时退回从文件名截，命令行 `--library ""` 与钉它的 `名字是空白时退回从文件名截而不是留一行空的` 一个字不动，改它归票 `03`（`main` 上 `ff96e14` 已在票 `03` 正文追加「收挂单 `Q469`」）。本票照裁定改了代码：抹掉那一行的 `meta_clear` 没了调用方，删掉；测试翻成 `改名成空白时当场报错那一行与路径锚都没动`。
- **状态：** settled
- **票 03 已落地：** 建库入口 `Catalog::create` 收到空白（含全是空白字符）当场报 `CatalogError::BlankLibraryName`，判在碰盘之前，工作目录里一个文件都不多；建库与改名问的是同一处 `refuse_library_name`（「算不算空白」照旧只在 `is_blank_name`）。钉命令行 `--library ""` 的那条按裁定翻成 `crates/core/tests/library_name.rs::建库时名字是空白就当场报错盘上一个文件都不多`，命令行另钉 `crates/cli/tests/library_name.rs::扫描时给空白名字当场报错一份库都不建`。

### Q470 — 那一族实测七个键（外加结构版本那一行）不是四个；两处是两个键合用一对函数，成型那一对没有公开的写函数

- **裁决（第五轮收口，2026-09-23）：** **岔** —— 实现走的就是条目建议的那条；追认：维持现状（2026-09-23 拿主意的人整批追认）。

- **来自：** 票 `no-mute-spots-opening-a-catalog/02`
- **类别：** 票写错了
- **在哪：** 票面与规格第三节「四个键八个函数」，对上 `crates/core/src/catalog/meta.rs` `MetaKey`。`04dc34c` 上散在 `catalog.rs`、`catalog/export.rs`、`catalog/title.rs`、`catalog/content.rs` 四处：`library_name`、`titles_folded_at`、`exported_at`、`export_format`、`export_out_dir`、`shaped_scan`、`shaped_manifest`，外加 `schema_version`
- **为什么没停线：** 多出来的三个键在 `04dc34c` 上本来就有具名函数；缺写入口的只有主库原名这一个，正是票要收的 `Q370`。
- **这张票实际做了什么：** 只给主库原名补了写函数 `Catalog::set_library_name`；`export_format` + `export_out_dir` 照旧合成 `export_setup` / `set_export_setup` 一对；`shaped_scan` / `shaped_manifest` 照旧各一个读函数，写在 `Catalog::replace_variants` 那一趟里一起落；`schema_version` 没有公开函数。每个键由哪几个函数读写，写在 `meta.rs` 那个变体的文档里。
- **另一条路：** 照票面字面逐键一对：导出配置拆成 `export_format` / `set_export_format` 与 `export_dir` / `set_export_dir`（`ExportSetup` 留作两格拼起来的便利函数）；成型那两个键各配一个具名写函数，`replace_variants` 末尾调它们。
- **建议留哪条：** 留这条。导出配置缺一格就不是一套跑得起来的配置（`export_setup` 读回一格在一格不在时交 `None`，这条判断眼下只在那一处），拆成两对之后每个写的调用方都能只写一格，「缺格算没选过」得在读的那边兜着；成型那两笔只有 `replace_variants` 一个写者、而且必须跟着那一趟落，另配写函数只是把两行挪个地方，多出一个别处也调得着的入口。票要的是「看得出这一族有什么、没有按字符串键读写」，两处都满足；验收第 3 条因此带保留勾。
- **谁来裁：** 收尾
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q471 — 键名收进一个私有枚举 `MetaKey`（新模块 `catalog/meta.rs`），没留在各自的具名函数旁边

- **裁决（第五轮收口，2026-09-23）：** **岔** —— 实现走的就是条目建议的那条；追认：维持现状（2026-09-23 拿主意的人整批追认）。

- **来自：** 票 `no-mute-spots-opening-a-catalog/02`
- **类别：** 规格没说
- **在哪：** `crates/core/src/catalog/meta.rs`；原先的 `META_LIBRARY_NAME`（`catalog.rs`）、`META_EXPORT_FORMAT` / `META_EXPORT_OUT_DIR` / `META_EXPORTED_AT`（`catalog/export.rs`）、`META_TITLES_FOLDED_AT`（`catalog/title.rs`）、`META_SHAPED_SCAN` / `META_SHAPED_MANIFEST`（`catalog/content.rs`），以及 `catalog.rs` 里三处 `'schema_version'` 字面量
- **为什么没停线：** 公开面一处没变，变的只是私有 helper 收什么参数；翻回去是一次机械替换。
- **这张票实际做了什么：** 七个常量与三处 `schema_version` 字面量收成 `MetaKey` 的八个变体，常量上那几段「为什么是元数据表上的键」搬到变体与模块文档上，每个变体的文档写明读写它的具名函数；`meta_get` / `meta_set` 只收 `MetaKey`，可见性 `pub(super)`（`catalog` 这一层以内；起初还有一个 `meta_clear`，`Q469` 裁成报错之后没了调用方，已删）。导出目录那个变体叫 `ExportDir`（词表**导出**条 `_Avoid_` 列着「输出」），盘上键名 `export_out_dir` 不动。键名的字节有测试反向钉住：`crates/core/tests/library_name.rs::元数据表那一族的键名一个字节没动旧库记下的账照样读得回来`（把 `exported_at` 变异成别的字串时变红）。
- **另一条路：** 常量留在各自那个具名函数旁边（`04dc34c` 的样子），helper 照旧收 `&str`——每个键名本来也只写了一遍，读导出那几个函数的人不必跳到另一个文件。
- **建议留哪条：** 留枚举。票的第一句是「读代码的人看得出元数据表那一族有哪些键」，散在四个文件里要 grep `META_` 才凑得齐；而且 helper 收 `&str` 时，`catalog` 底下任何一个子模块都能随手写一个字面量进去，那正是票要堵的「字符串约定」。代价是加一个键要动三处——枚举变体、`as_str` 那一行（`match` 不全编不过）、具名函数——外加变体文档里那一句；模块文档起初抄了一张对照表，评审指出编译器管不到它，已删。
- **谁来裁：** 收尾
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q472 — 改名只换给人看的那个名字，找库仍认起先那个名字：`--library <新名字>` 找不到这份库

- **裁决（第五轮收口，2026-09-23）：** **记** —— 已裁归票 03 并落地查重。已落地：票 no-mute-spots-opening-a-catalog/03（LibraryNameTaken）

- **来自：** 票 `no-mute-spots-opening-a-catalog/02`
- **类别：** 规格没说
- **在哪：** `crates/cli/src/main.rs` `open_catalog`（约 1306 行：`catalog_path(ws, slug)` + `Catalog::open_named`，文件不在就建）与 `crates/core/src/site.rs` `Site::open`；认领向导起名那一步查的是主库标识占没占用（`crates/gui/tests/program.rs` `起名那一步就拦下这个工作目录里已被占用的主库标识`）；`CONTEXT.md` **主库标识**条「由人起的那个名字折出来」
- **为什么没停线：** 词表**主库标识**条「由人起的那个名字折出来」、**主库原名**条「改原名不动任何一条路径锚」——找库认标识是定下来的；日后选另一条路是在找库那一步**加**一层兜底，这张票落下的东西一行不用撤。
- **这张票实际做了什么：** `Catalog::set_library_name` 只换元数据表那一行；`Site::open` 与命令行 `open_catalog` 照旧按名字折主库标识去找。文档写明「找库仍按起先那个名字」，核心库 `改名之后路径锚一个字节没动沉淀库里的裁决照旧对得上` 与命令行 `改过名的库报告印的是新名字` 两条测试都按原先的名字找库。
- **另一条路：** 本票顺手让改过的名字也找得到：`Site::open` 按名字折不出一份库时，在工作目录里按主库原名再找一遍（`workspace::catalogs`），恰好一份就开它。
- **建议留哪条：** 留这条。按原名兜底先得有「一个工作目录里主库原名不重」这条保证，而眼下谁都不保证它（认领向导查的是**标识**占没占用）；没有它，兜底撞上两份只能报错，命令行与界面对「这个名字指哪份库」也会各有一套答案。**但这一条暴露的风险要有人接**：改名之后 `scan --library <新名字>` 走 `open_catalog` 会**另建一份**，开场屏上两行同名、人分不出哪份是哪份。票 `03`（建库只剩一个入口，正好在那儿查重）与 `gui-looks-like-the-design/31`（界面改名入口）都挨着它，挂哪张由拿主意的人定。
- **谁来裁：** 拿主意的人
- **裁决（2026-09-13，拿主意的人）：** 归票 `03`。`main` 上票 `03` 正文已追加「收挂单 `Q472`」：建库入口与改名函数在同一处查主库原名重不重。本票不改代码。
- **状态：** settled
- **票 03 已落地：** `Catalog::open` 不再建库；建库只剩 `Catalog::create`，它与 `Catalog::set_library_name` 问同一处 `refuse_library_name`——同一个工作目录里另一份库在开场屏上印的名字与它撞了（NFC 之后逐字比），就报 `CatalogError::LibraryNameTaken`，说清撞的是哪个文件。于是改名之后 `scan --library <新名字>` 报错、不另建，界面添加主库那条向导同（`crates/gui/tests/program.rs::向导起的名字撞上一份改过名的库时开始扫描那一下拦下而不另建一份`）；改名撞名钉在 `crates/core/tests/library_name.rs::改名成同一个工作目录里另一份库的主库原名时当场报错`。找库照旧认主库标识（本条原先那条路），但按新名字找不到时说清那个名字是哪一份库的主库原名（`SiteError::not_found`，命令行各命令与 `Site::open` 共用这一句）：`crates/core/tests/library_name.rs::改过名之后按新名字找库说清那个名字是哪一份库的主库原名`、`crates/cli/tests/library_name.rs::改过名之后拿新名字扫描或出报告都说清是哪一份而不另建一份`。比法的岔路见 `Q521`。
