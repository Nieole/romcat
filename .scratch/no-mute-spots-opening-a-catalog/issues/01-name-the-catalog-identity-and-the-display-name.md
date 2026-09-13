# 01 —（prefactor）`主库标识` 与 `主库原名` 正名

**What to build:** 读代码的人不必再猜「主库名」指的是哪一个。公开面上那个**名不副实**的
"主库名"——它其实是中立库的主文件名、也是**路径锚**里那个键——改成与词表 **主库标识** 同名；
给人看的那个照旧是 **主库原名**。

收挂单 `Q372`：公开面上并存两个「主库名」，只有一个是标识符。
—— **已收（2026-09-13，提交 `68c76d6`）**：`Site::library` 改叫 `Site::library_identity`，与 `Catalog::library_name`（主库原名）两处文档互指；正文在 `.scratch/gui-self-sufficient/issues/01-library-name-lands-in-the-catalog.md` 的 `### Q372`，那份历史记录不改。字段名没照 grill 那句 `Site::slug`，见挂单 `Q452`（等拿主意的人裁）。

**Blocked by:** 无 —— 可立即开工

**Status:** done

⚠️ **只改叫法，不动锚。** **路径锚里存的键一个字节不动**——它存的本来就是这个标识，
语义没变、只是名字对上了。真要动锚是另一件事，而且那件事至今没有理由。

⚠️ **这是 prefactor。** 票 `02` 与 `03` 要往这片地上加东西（元数据键的具名函数、建库入口），
**先正名，它们才不会用着混淆的词造出新的**——否则那些新函数要被改两次名。

⚠️ **59 处调用点，分布在三个 crate**（约各二十处）。**够不上「宽重构」**——一次机械改名就完，
不必 expand–contract。**编译器就是这一件的测试，不新增测试。**

⚠️ **词表那两条词头已经落笔**（**主库标识** / **主库原名**，两条互指）。代码里之所以并存两个
"主库名"，根子是词表从没给这两样各起一个名字——改完之后词表、类型名、文档是同一套说法。

- [x] 公开面上那个标识改成与词表 **主库标识** 同名，59 处调用点全部跟上
      —— `crates/core/src/site.rs` `Site::library` → `Site::library_identity`（连 `in_memory` / `at` / `open_file` 里的同名参数与局部量）。**实测调用点 25 处，不是 59**：命令行 8、核心库 2、界面源码 10、界面测试 5，外加 `site.rs` 自己 3 处；「59」是 `grep '\.library\b'` 的原始行数，里面混着 `args.library`（`--library` 参数）、`LibraryFs` 字段、verdict 行字段这些不相干的名字。编译器就是测试：门禁 check / clippy 两步绿。**保留**：英文名取 `library_identity` 而不是 grill 写的 `slug`（词表**主库标识**条 `_Avoid_` 列着 `slug`），挂单 `Q452` 等拿主意的人裁，要翻得在合进 `main` 之前翻。
- [x] 给人看的那个照旧叫 **主库原名**，两处文档互指
      —— `Catalog::library_name` 名字不动，文档开头写明它是**主库原名**，并有一节「它**不是** [`Site::library_identity`]」；`Site::library_identity` 的文档指回 `Catalog::library_name`；`Site::display_name` 那一节「它**不是** [`Self::library_identity`]」同样跟上。三处都是 rustdoc 链接，门禁 doc 步（`-D warnings`）绿，说明链接都解析得到。词表那两条词头本来就互指。
- [x] **路径锚里存的键一个字节没动**（有测试反向钉住）
      —— `crates/core/tests/library_name.rs::路径锚里存的主库标识一个字节没动`：字面量 `"主库-f5c61109e92b6036"` 取自改名前的代码（不是照算法重算的）；按名字（`Site::open`）和按文件（`Site::open_file`）两个入口开现场，断言标识逐字节等于它，而且沉淀库里预先按这串字节落的路径锚能被 `Store::joined` 认出。改名前后都绿；把 `Site::at` 变异成只存可读的一半时变红（left `"主库"`，right 那串字面量），还原后 `cmp` 逐字节一致。沉淀库那几列 `library` 没动。规格测试表写着「不新增测试」，票面这一条要求「有测试反向钉住」，按编排者的指示补了这一条。
- [x] 全仓门禁绿，行为一处没变
      —— 门禁 `cargo xtask gate -j 3 --test-threads 3 --keep-going`，日志 `/Users/nicoer/dev/game-wt/logs/slot-1-gate.log`，`EXIT=1`：fmt / check / clippy / doc 绿；test 里 romcat-core lib 969 过 1 挂，**唯一红的是既有的、与盘大小写有关的** `testing::target::tests::分大小写那一档上_两个只差大小写的目录照旧是两个`（在临时目录里真建 `GB/` 与 `gb/`，这台 Mac 的 APFS 不分大小写，两个并成一个；本票没碰 `crates/core/src/testing/`；Linux CI 上是绿的），编排者在 `main` 上另修。**保留**：cargo test 在那个 lib 二进制上就停了，排在它后面的核心库集成测试、界面测试、xtask 测试没在这趟门禁里跑到。改动涉及的几份在最后一次改动后单独跑过，全绿（`/Users/nicoer/dev/game-wt/logs/slot-1-targeted-tests-2.log`，`EXIT=0`）：核心库 `library_name` / `collection` / `stage` / `triage` / `catalog_list`、核心库 `site::` 单元测试、命令行 `library_name`、界面 `program` / `queue`（`--all-features`）。行为：diff 只有改名和注释；屏上的字与命令行输出一个字没改（`Q451`、`Q454` 刻意留着）。
- [x] 全仓 `grep` 不到第三种叫法
      —— **带保留勾。** `.scratch/` 以外，代码注释、测试名、`docs/adr/0020`、`docs/adr/0023` 里的「主库名」「在路径锚里叫什么名字」「库名」都按所指换成了**主库标识**或**主库原名**（改过名的五个测试函数见挂单 `Q453`）。grep「主库名」还剩：`crates/gui/src/claim.rs:37` 屏上提示字，以及 `crates/gui/tests/program.rs` 里五处按这串字找框的 `打字`（改它就是改行为，与上一条相悖——挂单 `Q451`，已托给票 `gui-looks-like-the-design/03` 的正文）；`CONTEXT.md` 的 `_Avoid_` 那一行自己；`crates/core/src/catalog/export.rs:14-15` 说明引文里换过词的那句注释。Rust 名字里仍装着主库标识的：`Anchor::Path::library`、`Index::load` 等的 `library` 参数、`Decide::library`、`crates/core/tests/rescan.rs` 的 `const 库名`（挂单 `Q449`）；主库原名一侧的 `display_name` / `library_label`（挂单 `Q450`）。命令行把标识印给人看的两句见 `Q454`。
