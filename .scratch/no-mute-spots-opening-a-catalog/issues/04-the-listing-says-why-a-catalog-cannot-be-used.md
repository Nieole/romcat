# 04 — 开场的列举说得出「为什么用不了」

**What to build:** 维护者把**工作目录**指到一个**读不动**的位置时，**开场**屏不再说
「还没有库，认领一个主库开始」——它如实说那个目录读不动。而认领新主库那条路在
**目录写不动**时当场说清（它正要往那个目录里写）。代码这一侧也**分得开**
「结构版本对不上」与「文件坏了」。

- [x] 收挂单 `Q388`（目录读不动与它压根不在，交出来是同一样东西）
  - 已收：`workspace::catalogs` 交三态 `Listing`：`Catalogs`（至少一份）／`Empty`（中立库住的目录还不在也是这一态）／`Unreadable(DirUnreadable)`（`read_dir` 出 `NotFound` 以外的错，列到一半出错也算）。证据见验收第 1 条。
- [x] 收挂单 `Q393`（`CatalogError` 被压成一句话，调用方分不出两种失败）
  - 已收：`CatalogEntry` 的 `openable` 与 `facts` 两格换成 `state: CatalogState`：`Openable(Result<CatalogFacts, String>)`／`SchemaMismatch { found, expected, said }`／`Broken { said }`，按 `CatalogError::Version` 这个**类型**分支，`said` 是核心库原话。证据见验收第 3 条。
- [x] 收挂单 `Q524`（2026-09-14 拿主意的人裁：**归本票**）：添加主库那条向导第一步照旧只查主库标识占没占、自己另判一次空白，撞**主库原名**要到「开始扫描」那一下才由建库入口（`Catalog::create` → `refuse_library_name`）拦下。本票正要改向导这一步（目录写不动时当场说清），就在同一处把空白与撞名都改成问核心库那一处判断（ADR-0024），第一步就说清，不再自己另判一份。
  - 已收：新加 `Catalog::refuse_create(path, name)`。名字收不收问的就是 `refuse_library_name`，另先问那份在不在（`AlreadyExists`）、目录写不写得进（`DirUnwritable`），一个字节都不写。`Wizard::named` 只问它，自己那份 `trim().is_empty()` 与 `catalog_path(..).exists()`（`Q411` 的形状）删掉；起名那一下与「开始扫描」那一下认的是同一个 `Wizard::slug`。证据：`crates/gui/tests/program.rs::向导起名留空时说的是核心库那句空白`、`向导起的名字撞上一份改过名的库时起名那一步就拦下`（由原先那条「开始扫描那一下拦下」改写）、`起名那一步就拦下这个工作目录里已经在用的名字说的是核心库那句原话`，三条先红；`crates/core/tests/library_name.rs::建库之前问得出会被拦下的那几样而且一个字节都不碰`（先红：`AlreadyExists` 那一格）。
  - **保留**：屏上从向导自己那句换成核心库原话，「换个名字，或者去开那一份」半句没了（`Q615`）；列不开的目录里查不了重名就不收，建库与改名一起改了（`Q612`）。

挂单 `Q611`–`Q616` 是本票走过的岔路口。

**Blocked by:** 无 —— 可立即开工

**Status:** done

**门禁（2026-09-14，分支 `q5/nm-04-listing-says-why`，`cargo xtask gate -j 3 --test-threads 3 --keep-going`，`TMPDIR` 指到大小写敏感盘）：** 六条里五条绿、doc 红，`EXIT=1`。绿的是 fmt 1s、glossary 0s、check 11s、clippy 14s（`--all-targets --all-features`）、test 414s（`--all-features`，71 个测试目标，1896 条通过、0 失败）；doc 4s 红在一处 `redundant explicit link target`——`crates/core/src/workspace.rs:280`，本票新写的 `CatalogState` 文档里的 `[`CatalogError`](crate::catalog::CatalogError)`。日志 `/Users/nicoer/dev/game-wt/logs/slot-1-nm04-gate.log`。那一处去掉显式目标之后单跑 doc：`RUSTDOCFLAGS="-D warnings" cargo doc -p romcat-core --no-deps --all-features --lib -j 3` 绿（`EXIT_CORE=0`），顺带单跑 `romcat-gui`（`--lib --bins`）与 `romcat-cli`（`--bins`）的 doc 也绿（`EXIT_GUI=0`、`EXIT=0`），日志 `/Users/nicoer/dev/game-wt/logs/slot-1-nm04-doc.log`。修的只是一行文档注释，全量门禁没在分支上重跑，合进 main 之后由编排再跑一次。

⚠️ **这两条是同一件事，不是两处小疏漏**——同一个判断被压平了两次：一次压在**目录**那一层，
一次压在**库**那一层。判据照抄 ADR-0021 那条修订：**一个如实记录的状态，胜过一个把两件事
说成一件的沉默。**

- **对目录：三态** —— 有库 ／ 空的 ／ **读不动**
- **对每一份中立库：说得出原因的状态** —— 开得了 ／ **结构版本对不上** ／ **文件坏了**

⚠️ **先读这一条，别把工作量估大了**：**屏上给人看的那句话已经是对的**——那个错误类型自己
说得出是哪个版本对哪个版本，而且有一条现成测试钉着。**分不开的是调用方**（它拿到的是一句
字符串）。所以改动集中在**列举交出来的类型**，**不在屏上已有的措辞**。

⚠️ **要连开场屏一起改完才算数**：类型分得开了而屏上仍旧说同一句话，等于没做。

⚠️ **那条「读不动」的测试要造一个真的读不动的目录**。造不出来的平台上要**如实跳过并说明**，
**不许改成「假装读不动」**——那样验的是替身，不是行为。

- [x] 读不动的目录交出来的**不是「空的」**，开场屏上说的是另一句话
  - 证据：`crates/core/tests/catalog_list.rs::工作目录读不动时交出来的是读不动而不是空的`——工作目录本身、与它底下中立库住的那个目录，两层各收一次读权限（`0000`），都交 `Listing::Unreadable`，那句话说「读不动」并带目录路径；先红：交出来的是 `Empty`。`crates/gui/tests/program.rs::工作目录读不动时开场说的是读不动而不是还没有库`——屏上没有「还没有库，添加一个主库开始」，有「读不动」与那个目录；先红：屏上说的正是「还没有库」。开场屏 `catalogs_ui` 按三态各画各的，读不动画核心库那句原话（出错色，取自令牌），版式没动。临时目录名里不带「读不动」三个字，免得屏上印着的路径替断言对上。
- [x] 认领新主库那条路在**目录写不动**时当场说清——**带保留**
  - 证据：`crates/gui/tests/program.rs::工作目录写不动时向导起名那一步就说清`——工作目录收成 `0555`，起名后按「下一步」，屏上说「写不动」、仍在第一步；先红：走到了第二步。`crates/core/tests/library_name.rs::工作目录写不动时建库之前就说清是写不动`——`CatalogError::DirUnwritable`，说清拦住的是工作目录本身（中立库目录还没建时问最近那一级已经在的上级）；先红。判的是 `access(2)`（写与进），一个字节都不写；写得动的工作目录上「问一遍不写东西」由 `向导走到第二步就放弃时工作目录里一个文件都不多` 与 `建库之前问得出会被拦下的那几样而且一个字节都不碰` 钉着。
  - **保留**：Windows 上这一问答不出来，交给「开始扫描」那一下建库自己报错（`Q611`）。新加依赖 `rustix`（只开 `std` 与 `fs`，只在 Unix 上），事先报过编排、编排同意。
- [x] 调用方**分得开**「结构版本对不上」与「文件坏了」（不靠解析字符串）——**带保留**
  - 证据：`crates/core/tests/catalog_list.rs::调用方分得开结构版本对不上与文件坏了`——一份版本 4、一份不是 SQLite 的文件、一份好的，只按 `CatalogState` 的分支断言，一个字都不看那句话；先红：版本对不上那一份交的是 `Broken`。命令行守卫与开场屏的「打开」按 `CatalogState::openable` 分。
  - **保留**：中立库文件自己读不动（权限、被锁）照 ADR-0021 修订段的三态落「文件坏了」，没分第四态（`Q613`）。
- [x] 反向钉住：**照旧说得出是哪个版本对哪个版本**
  - 证据：`crates/core/tests/catalog_list.rs::结构版本对不上的库照列并说清是哪个版本对哪个版本`——测试名与「结构版本是 4」「本程序认得的是 {SCHEMA_VERSION}」「删掉它重扫一遍」三句断言一字未改，取那句话的地方从 `facts` 的 `Err` 换成 `SchemaMismatch.said`。`crates/gui/tests/program.rs` 里同名那条一行没动，照绿。`CatalogError::Version` 的措辞没动。
- [x] 反向钉住：**一份坏掉的库照旧不连累其余**
  - 证据：`crates/core/tests/catalog_list.rs::一份读不出来的库不连累其余` 照绿，改的只是取状态的写法；`调用方分得开结构版本对不上与文件坏了` 里好的那一份照样是 `Openable(Ok(..))`；界面 `结构版本对不上的库照列并说清是哪个版本对哪个版本` 里好的那一份照样画出「2 个变体」。
- [x] 造不出真读不动目录的平台上，那条测试如实跳过并说明为什么——**带保留**
  - 证据：`romcat_core::testing::revoke(dir, Revoke::Read | Revoke::Write)` 改完权限位**当场试一下**（列得开吗、写得进吗）：不是 Unix、或者权限位拦不住（跑的是 root）就交 `None`，并印「跳过 <测试名>：这台机器上造不出…的目录——为什么」；守卫丢掉时把权限原样还回去。五条权限测试都走它，没有一条用替身。本机（macOS，uid 501）没有跳过：五条带 `--nocapture` 跑，没有一行「跳过」，而且都是在真目录上先红的。
  - **保留**：测试框架没有「跑到一半跳过」这一态，跳过的那条在汇总里记成通过，那一行要 `--nocapture` 才看得见（审查 Spec 轴第 1 条）。没挂 `#[ignore]`：`docs/agents/long-jobs.md` 规定它只挂要人主动跑、跑起来很贵的东西。
