# 03: 导出与同步听平台纠正

**What to build:** 人在库体检里做过**平台纠正**的变体，导出按纠正后的平台放置、子库同步按纠正后的平台放到卡上；没纠正过的照旧按目录。
卡带头与目录的分歧不自动搬家（ADR-0011）。核心库立「落点平台」一问（名字照词表起，词表同步补），导出与同步里取平台的地方都改问它。
清单里的旧落点照 ADR-0015 由下一趟同步照实观察、清掉。库体检那句「导出与同步眼下仍按目录放置」改成实话。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q1033`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 先写红测试：一个做过平台纠正的变体，导出计划与同步计划里的落点平台是纠正后的（今天是目录的）
  - 测试 `做过平台纠正的变体_导出把它放进纠正后的平台_同步把它落到纠正后的平台目录`（`crates/core/tests/platform_fix.rs`）。
    红的那一行原话（改之前，`cargo test -p romcat-core --test platform_fix`）：
    `assertion left == right failed: 导出的合集照落点平台分：FC → FDS 那一组听着纠正 / left: ["FC", "GBA", "PSP"] / right: ["FDS", "GBA", "PSP"]`。
    改完绿：同一条测试断言导出折出的合集是 `["FDS", "GBA", "PSP"]`、`FDS` 那一份的条目路径照旧是主库里的
    `fc/合集/银河战士.fds` 与 `fc/日版/塞尔达传说.fds`；同步折出来的落点是 `fds/日版/塞尔达传说.fds`、
    `fds/合集/银河战士.fds`，而 `footprint.platforms()` 也是 `["FDS", "GBA", "PSP"]`。
    一处判断落在 `identify::platform_of`（一次答「识别判定的」与「落点的」两个），结论落库成
    `identification.placement`，读法是 `Catalog::placement_platforms` / `Catalog::placement_directories`（词表「平台」条补了**落点平台**）。
- [x] 没纠正、但卡带头与目录分歧的变体：落点照旧是目录的
  - 测试 `没纠正过而卡带头与目录分歧的变体_落点照旧是目录的`（同文件）：`psp/` 底下那张头是 GBC 的卡，
    `identified_platforms` 说是 GBC，而同步落点是 `psp/放错的/007 黑日危机 繁体修正版.gbc`、`platforms()` 是
    `["FC", "GBA", "PSP"]`，导出也照旧放在 `PSP` 那个合集里（ADR-0011 不自动搬家）。
- [x] 已同步过的变体纠正之后：下一趟同步把旧落点列为删除、新落点列为新增，清单之外的一律不碰
  - 测试 `已同步过的变体纠正之后_下一趟同步把旧落点列为删除_新落点列为新增_清单之外的一律不碰`（同文件）：
    第一趟折出清单（两份 `.fds` 落在 `fc/` 下），纠正 FC → FDS 并重跑识别之后 `sync::plan` 给
    `fc/日版/塞尔达传说.fds`、`fc/合集/银河战士.fds` 两条 `Delete`，`fds/…` 两条 `Add`；没动过的
    `psp/` 那一条一步都不用走，清单之外手抄进去的 `存档/007.sav` 也不在计划里（ADR-0015）。
- [x] 库体检那句旧话已改，截图基线重批
  - 屏上文案改成「改了平台的变体会在下次识别时按新平台重新匹配；导出与同步按纠正后的平台放置。」
    （`crates/gui/src/health.rs` 的 `PLATFIX_FOOTNOTE`，照设计稿 `DLG.platfix` 副标题那半句），断言改成新话并反向咬住旧话
    （`crates/gui/tests/health.rs`）。**截图基线一张都没变**：那一句在平台纠正那一层正文的最底下，
    `library/platfix-{light,dark}` 拍的是开了那一层的头一屏，它在折线以下（不带 `UPDATE_SNAPSHOTS` 跑
    `cargo test -p romcat-gui --all-features --test snapshot` 是 159 passed，没有一张红）。对照材料因此没有要出的。
- [x] 不碰主库一个字节（ADR-0004）
  - 测试 `做过平台纠正的变体_导出把它放进纠正后的平台_同步把它落到纠正后的平台目录` 里加了整份目录快照：
    纠正 + 重跑识别 + 导出折一遍 + 同步折一遍下来，`现场.dir` 逐字节相同。既有的
    `纠正一整轮下来盘上的文件一个字节都没动` 照旧绿。
- [x] 门禁全绿（本机不分大小写的盘上 `sync_run` 那五条已知红除外，`Q479`）
  - 日志 `/Users/nicoer/dev/game-wt/logs/cao-03-gate1.log`，末行 `EXIT=1`。七条里**六条绿**：
    `绿 fmt 3s` / `绿 glossary 1s` / `绿 check 34s` / `红 clippy 28s` / `绿 test 2745s` /
    `绿 numbers 9s` / `绿 doc 21s`（`test` 那一趟没有一条红，本机不分大小写的那几条照旧印「跳过：…」）。
  - **保留一条**：`clippy` 红在 `crates/gui/tests/snapshot.rs:936`（`needless_borrows_for_generic_args`），
    这一票一个字节没碰那个文件；这台机器的 `cargo` 是 Homebrew 的 1.99.1，而仓库钉的是 `rust-toolchain.toml`
    的 1.98.1（没有 rustup 可切），`slot2` / `slot3` 的同一趟门禁上一模一样。记在挂单 `Q1321`。
  - 集成分支 `q8/cao-int` 收尾时仍停在基点 `117eb7f`，`git merge q8/cao-int` 是 `Already up to date`：
    没有带进任何改动，因此没跑第二趟门禁。
  - **合进集成分支之后补一笔（编排者 2026-10-28）**：`clippy` 那条保留已经不再：那两处旧 lint 由票
    `core-answers-once/05`、`/12` 顺手改掉，三张票合齐之后在集成分支上重跑门禁 `7 条全绿`、`EXIT=0`
    （日志 `cao-int-gate1.log`）。挂单 `Q1321` 一并结掉。

**留下的：** 三件事没在这一票里做，各自记了挂单——导出的元数据文件名仍照内容实际在的那一级目录
（`Q1317`）、落点平台要重跑一趟识别才生效（`Q1318`）、子库报告按平台那一栏仍按目录声明分（`Q1319`）、
一个平台散在几个目录里时落点目录退回清单里的头一个（`Q1320`）。
