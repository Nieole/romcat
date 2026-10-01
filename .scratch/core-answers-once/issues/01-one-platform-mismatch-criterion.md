# 01: 平台不符只判一处，GB↔GBC 照稿出来

**What to build:** 「平台不符」在核心库只判一处：扩展名、卡带头家族、CGB 标志三样一起看。库体检那一格、平台纠正那一层、识别报告都读它。
卡带头说「只能在 GBC 上跑」却躺在 `gb/` 目录的卡算不符；同时支持 GB 的 GBC 卡躺在 `gb/` 不算。
平台纠正那一层照稿多出 GB→GBC、GBC→GB 两组。库体检那一格在真库上会多几千条，是预期。

同一张票里给词表**平台**条补三种说法：**目录声明的平台**、**识别判定的平台**、**平台纠正**，写明各处按哪个算
（识别打分、刮削与识别报告按识别判定；导出与同步的落点按「有纠正听纠正，否则目录声明」；平台不符比的是目录声明与内容）。照 `docs/agents/domain.md` 写。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q1030`：见 `../grill.md` 里那一条。
收挂单 `Q606`：见 `../grill.md` 里那一条。

**Blocked by:** `no-wrong-deletes-or-platforms/03`（裁决过的变体按卡带头判平台）

**Status:** done

- [x] 先写在今天 `main` 上红的测试，接在核心库平台纠正那份测试的夹具上：CGB-only 卡在 GB 目录 → 库体检那一格数到它、平台纠正里有 GB→GBC 那一组
  - 证据：`crates/core/tests/platform_fix.rs` 的 `只能在_gbc_上跑的卡躺在_gb_目录_识别读过卡带头之后库体检那一格数到它_平台纠正里有_gb_到_gbc_那一组`。夹具拆成 `摆现场`，`建现场_有_gb_与_gbc` 在 `gb/` 底下摆一张只能在 GBC 上跑的卡（真头 `testing::cart::GBC_TWINE`，CGB 标志 `C0h`）。只加了夹具与测试、判据还没动时跑过一次，是红的：`库体检那一格数到它：GB → GBC 那一组`（`platform_fix.rs:665`）。断言库体检那一组 1 条、样例是那张卡的键，平台纠正里有「GB 目录里的 GBC 游戏」，理由带「CGB 标志」「0xC0」，没有「保持也不影响游玩」那一句（GB 跑不了它）。
  - 另做了一次变异核对：让判据不看卡带头，这条连同下面几条新测试全红（8 条红）。
- [x] 同一夹具：双模卡在 GB 目录不算不符；扩展名不对、卡带头家族不对的照旧算
  - 证据：`双模卡躺在_gb_目录不算不符_gb_游戏躺在_gbc_目录算_扩展名不对与卡带头族不对的照旧算`。识别之后五组是 `FC→FDS 2`（扩展名）、`GB→GBC 1`、`GBA→NDS 1`（卡带头族）、`GBC→GB 1`、`PSP→GBC 1`（卡带头族），双模卡（`80h`）不在任何一组的样例里；GBC→GB 那一组摊着「GBC 能运行 GB 的游戏，保持也不影响游玩」。
  - 「扩展名不对的照旧算」另钉了一条：`扩展名说不符而卡带头的族认目录的_读过卡带头之后照旧算不符`（`md/` 底下一份 `.32x`，真 MD 头）。这一条是审查 Spec 轴抓出来的：最初那一版让「读过卡带头就不再看扩展名」，这份 `.32x` 识别之后就从不符里消失了。先写这条，它红（`platform_fix.rs:867`）；判据改成并集之后绿。单测 `scan::aggregate::tests::平台不符_扩展名与卡带头取并集_两样说的平台不一样时记卡带头的` 钉住并集和「两样说的平台不一样时记卡带头说的」。
  - 透明容器里的卡也算：`透明容器里那张只能在_gbc_上跑的卡_样例写成容器的键接内部路径`。
- [x] 库体检那一格、平台纠正分组、识别报告里的平台冲突三处数得一样
  - 证据：`库体检那一格_平台纠正那几组_识别报告里的平台不符三处数得一样`：`HealthReport.conflicts.total`、平台纠正各组条数之和、`IdentifyReport.platform_conflicts` 三个都是 6。识别报告不再另写查询，读的是 `Catalog::aggregate` 那一份折统计（`IdentifyReport::build` 多收一个平台清单，`identify::Options::manifest`）。识别报告那一栏改叫「目录与内容平台不符」，按条数，和库体检同一个单位。
  - 保留：界面上库体检那一格画的是 `CorrectionGroups::remaining()`，也就是扣掉做过平台纠正的组之后的数；识别报告数的是全部。没定过任何一组时三处相等；定过之后格子上的数会小一截。识别报告那句话写明了这一点（「库体检那一格只画其中还没做平台纠正的组」）。
- [x] 扫描那边与识别那边原来各自的判据只剩一处调用（两处「GB/GBC 不算冲突」的豁免已删）
  - 证据：`catalog/identify.rs` 的 `CONFLICT_FROM`、`Catalog::platform_conflicts`、`Catalog::platform_conflict_count`、`catalog::identify::PlatformConflict` 都删了；卡带那一层自数的 `CartCount.conflicts`（第三份按族的判断）也删了，命令行那句「内部头与目录声明的平台对不上的有 N 份」跟着拿掉。判据只剩 `scan::aggregate::conflicting_platform(manifest, declared, extension, cart)` 一处，调用点三个：裸文件 `FileObservation::derive`、容器内部 `Aggregate::record_inner_entry`、识别问组 `DecidedPlatforms::decide`。`DecidedPlatforms` 不再自带一份清单，用的是这一趟的 `Options::manifest`。两处豁免的说法（`content_cart.family` 那段表注释和 `probe_carts` 里那段注释）都改掉了。CGB 标志落在 `cart::Facts::cgb`，「这份头认哪几个平台」是 `cart::Facts::platforms`。
  - 旧库里加这一格之前读的 GB / GBC 头：库体检先保守地答（平台是 GB 的照 GB 游戏算，是 GBC 的两个都认）；识别下一趟重读那段头（`Facts::predates_cgb`）。测试 `加_cgb_标志之前读的旧头_库体检先保守地不报_gb_到_gbc_下一趟识别重读那段头之后报出来`。
  - 保留：精确命中过 DAT 的卡带，识别不探卡带头，它们的 CGB 标志无从知道，原版、只能在 GBC 上跑的卡躺在 `gb/` 照旧报不出来（挂单 `Q1297`）。
- [x] 词表**平台**条补齐三种说法与各处口径
  - 证据：`CONTEXT.md`「平台」条补了**目录声明的平台**、**识别判定的平台**、**平台纠正**三种说法，各处按哪个算（识别打分、刮削与两份报告按识别判定的，前者落在票 02；导出与同步的落点有纠正听纠正、否则照目录声明，落在票 03；「目录与内容平台不符」比的是目录声明的与内容），以及「目录与内容平台不符」的判据：三样取并集，GB / GBC 那一族看 CGB 标志。「平台纠正」条里判据那一句补了三样输入。
- [x] 库体检与平台纠正那几张截图基线重批，拿主意的人点头
  - 证据：变了 2 张，`crates/gui/tests/snapshots/library/platfix-light.png` 与 `platfix-dark.png`。GB→GBC（4 条）、GBC→GB（3 条）两组照稿排在最前；夹具 `有平台不符` 在 `gb/`、`gbc/` 底下摆了卡，并在界面外跑一趟识别，让卡带头落库。库体检那两张（`library/health-*`）逐字节没变：那份夹具只扫不识别，那一格照旧是 0。重批日志 `/Users/nicoer/dev/game-wt/logs/q8-cao-01-snapshot2.log`，末行 `EXIT=0`，121 条全过；并排图与清单在 `/Users/nicoer/dev/game-wt/logs/q8-cao-01-compare/`。
  - 编排者对稿看过两张并排图，没有打回。四条样式岔路口由**拿主意的人 2026-10-01 裁**：
    1. GBC→GB 那句**照实写**「CGB 标志不是 0x80 也不是 0xC0」，不照稿写「为 0x00」（老卡上那个字节是标题的最后一个字）；
    2. 理由句里给人看的字**照稿叫「文件头」**（沿用规格 `gui-draws-the-rest-of-the-design` 里「照稿写的词沿用已有『屏上照稿写』的先例」）。只改了 `report::ConflictGroup::reason` 那几句与凭据名 `ConflictEvidence::CartHeader.label()`；代码里的名字、测试名、词表照旧叫卡带头。屏上、识别报告、命令行印的都是这一处的字；
    3. 族不对的那几组**不写**偏移与「与 NDS 数据库一致」，只说判据真看了的；
    4. 库体检那一格在截图里**不动**，行为由核心库的测试钉着。
- [x] 门禁全绿（本机不分大小写的盘上 `sync_run` 那五条已知红除外，`Q479`）
  - 证据：本机已没有那五条的红，要的就是全绿。日志 `/Users/nicoer/dev/game-wt/logs/q8-cao-01-gate1.log`，开头两行是 `/Users/nicoer/dev/game-wt/slot3` 与 `q8/cao-01`，末行 `EXIT=0`。汇总：fmt 绿 3s／glossary 绿 2s（看的是未提交的改动，17 份 `.rs` 里新写的 1080 行，没撞上）／check 绿 20s／clippy 绿 15s／test 绿 1344s（`--no-fail-fast`，88 个测试目标全 ok）／numbers 绿 20s（`numbers --write` 先写回了 README 两处：票数 165/205、测试条数 2,674）／doc 绿 23s，「7 条全绿」。

**收挂单 `Q1030`：** 照 grill 那一条落地：「平台不符」只在核心库判一处（`scan::aggregate::conflicting_platform`）——扩展名、卡带头家族、CGB 标志三样取并集，库体检那一格、平台纠正那一层、识别报告都读它；稿上打头的 GB↔GBC 两组出得来了。并集这一条照的是规格用户故事 4（「合并判据不会漏掉从前报得出的」）。两样都说不符、说的平台不一样时记卡带头说的，只有这一条路站得住：字节是内容自己说的，识别判定的平台那条回退链也是先听卡带头。GB 游戏躺在 GBC 目录也算不符，照的是票面与设计稿，grill 原话只说了 GB 目录那一半（挂单 `Q1298`）。

**收挂单 `Q606`：** `CONTEXT.md`「平台」条下立了**目录声明的平台**、**识别判定的平台**、**平台纠正**三种说法，写明各处按哪个算（见上面第五条）。

本票走过的岔路口记在挂单 `Q1297`–`Q1299`。
