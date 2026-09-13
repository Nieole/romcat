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
