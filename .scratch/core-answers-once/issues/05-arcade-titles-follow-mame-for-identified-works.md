# 05: 认出作品的街机也听 MAME 标题

**What to build:** 按平台的标题覆盖对**认出作品**的变体也生效：有平台语境的地方（导出、作品详情里按变体的平台）按那个平台重挑显示标题，
ADR-0010 定的 MAME 权威落到认出作品的街机上。没有平台语境的地方（浏览屏列表）照旧一个显示标题。屏上那句「覆盖只管没认出作品的」改成实话。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q783`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 先写红测试：一个认出作品的街机变体，导出到 MAME 平台时显示标题取 MAME 那一个（今天取作品的）
  - 证据：`crates/core/tests/priority.rs::认出作品的街机导出时显示标题照街机那一栏挑`。在今天的树上先红：
    `left: "Contra (World)" right: "Contra"`（夹具里「街机作品」的标题集合有 No-Intro 与 MAME 两条同档的
    英文官方名，街机那一栏的覆盖把 MAME 排前）。同一条红趟里 `换一份表之前说得出哪几处显示值会变_照导出
    真会写出去的算` 报 `works: 2`（对的是 `1`）——旧的挑法让「换表前后的变化」也以为那部街机作品的
    显示标题会变，而导出其实不变。修完三条都绿。
- [x] 作品详情按变体平台看到同样的标题
  - 证据：`crates/core/tests/priority.rs::作品详情按变体所在的平台看到同一个标题`——`variant_detail` 的
    `display.display == "Contra"`（MAME 那一条），大标题底下「还叫什么」照同一个平台算：`official ==
    Some("Contra (World)")`、`others` 空。红趟里这条报 `left: "Contra (World)"`。
- [x] 浏览屏列表上那部作品的显示标题不变
  - 证据：`crates/core/tests/priority.rs::浏览屏列表上那部作品的显示标题照旧一个`——`WorkQuery::default()`
    取一页，`WorkRow.display == Some("Contra (World)")`（通用那条 No-Intro 在前）。红趟里这条就是绿的
    （它本来不该变），修完照旧绿。
- [x] 导出结果与详情页相关的截图基线重批
  - 证据：`cargo test -p romcat-gui --all-features --test snapshot`（**不带** `UPDATE_SNAPSHOTS`）159 张
    全绿、一份 png 都没动——界面夹具（`crates/gui/tests/snapshot.rs` 的 `浏览的变体` / `浏览的作品`）里
    一部街机作品都没有，也没有哪部作品的标题集合收着 MAME 与 No-Intro 两条，于是「按平台重挑」在这份夹具
    上挑出来的与通用那条逐字相同；**没有要重批的图**。改了的只有屏上的字（`crates/gui/src/priority.rs::
    override_note`：旧句「它只管没认出作品的变体……眼下不看平台」换成实话），按平台单独那一栏本来就没有
    截图基线。对照材料与留给拿主意的人点头的两个问题（那句怎么落地、要不要为详情页补一张街机基线）在
    `/Users/nicoer/dev/game-wt/logs/cao-05-compare/CHANGED.md`。
- [x] 门禁全绿（本机不分大小写的盘上 `sync_run` 那五条已知红除外，`Q479`）
  - 证据：日志 `/Users/nicoer/dev/game-wt/logs/cao-05-gate3.log`，头两行是 `/Users/nicoer/dev/game-wt/slot2`
    与 `q8/cao-05`，末行原文 `EXIT=0`，底下那句 `7 条全绿。`。七条照原话：`绿 fmt 5s`、`绿 glossary 1s`、
    `绿 check 56s`、`绿 clippy 1s`、`绿 test 2063s`（`cargo test --workspace --all-features --no-fail-fast -j 2
    -- --test-threads=2`，约 2,983 条）、`绿 numbers 4s`、`绿 doc 10s`。这台机上没出现 `Q479` 那五条
    「跳过：…」的行，`sync_run` 也照实跑绿了。
  - 合并集成分支：`git merge q8/cao-int` 是 **no-op**（`Already up to date.`，cao-int 尖端 `117eb7f` 与本票
    基点同一个 sha），所以没跑第二趟门禁。
  - 中途 `cao-05-gate2.log` 红过两条，都不是这一票的语义：本票 `priority.rs` 的 `clippy::type_complexity`
    （收成 `converge::Titles` 修掉），以及 `q8/cao-int` 尖端就有的 `crates/gui/tests/snapshot.rs:936`
    `map(&号)` 一处 clippy 红（挂单 `Q1341`，当场按建议改成 `map(号)`，截图一张没变）。
