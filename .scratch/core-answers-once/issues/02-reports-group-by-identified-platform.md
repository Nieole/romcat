# 02: 报告与识别打分按识别判定的平台

**What to build:** 识别给候选打分时比的平台、刮削报告按平台分组、识别报告逐条取的平台，一律改读**识别判定的平台**。
两份报告头上各加一行口径：「按识别判定的平台分组」。放错目录的卡不再因为目录丢分，报告说的平台就是刮削实际取的平台。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q605`：见 `../grill.md` 里那一条。

**Blocked by:** 01

**Status:** done

- [x] 先写红测试（平台纠正那份夹具）：一张卡带头与目录分歧的变体，刮削报告把它记在识别判定的平台下（今天记在目录声明下）
  —— `crates/core/tests/platform_fix.rs::刮削报告按识别判定的平台分组_放错目录的卡记在卡带头说的平台下`。在 `9b3851c` 上红：
  按平台那一节分出 `{FC: 2, GBA: 2, PSP: 1}`；改后 `{FC: 2, GBA: 1, GBC: 1, NDS: 1}`（`psp/` 那张 GBC 卡与 `gba/` 那份头是 NDS 的各归内容说的平台）。
  改的是 `Catalog::source_by_platform`，与 `Catalog::identified_platforms`（刮削 `Plan::build` 读的那个）同一句读法
  `catalog::identify::identified_platform_sql!`。另有 `做过平台纠正的变体两份报告都记在纠正之后的平台下_按内容改与保持目录两个方向`
  （规格「纠正过的变体」那一格：FC → FDS 按内容改、GBA → NDS 保持目录，两份报告都分成 `{FDS: 2, GBA: 2, GBC: 1}`；
  它是实现之后补的，审查 Spec 轴点出来的，基点上照样红——基点按目录声明分成 `{FC: 2, GBA: 2, PSP: 1}`）。
- [x] 识别报告同一条变体同样按识别判定的平台出现
  —— `platform_fix.rs::识别报告按识别判定的平台分组_同一张放错目录的卡记在卡带头说的平台下_那一行的中文数跟着它走`，在基点上红
  （`{FC: 2, GBA: 2, PSP: 1}`）。同一张卡人裁成汉化版之后，GBC 那一行 `命中 1、汉化 1`，各行加起来的汉化数等于全库那个数。
  识别报告按平台的每一列都改走同一句读法：结论那一趟（`for_each_identification`）、还没识别那一趟（`for_each_not_run`）、
  中文两列（`chinese_by_platform`）、只靠名字两列（`NAME_ONLY_SQL`）。只靠名字那两列没有单独一条测试（这份夹具造不出只靠名字的候选），
  靠的是与别的列同一句读法。
- [x] 候选打分：同一份 DAT 记录，放错目录的卡与放对目录的卡打出同样的分
  —— `platform_fix.rs::候选打分比的是识别判定的平台_放错目录的卡与放对目录的卡对同几条_dat_记录排出同一个次序`：同一张只能在 GBC
  上跑的卡分别躺在 `gb/` 与 `gbc/`，两份 DAT（No-Intro 的 GBC、TOSEC 的 GB）都写着它的游戏码。基点上 `gb/` 那张排成
  `[TOSEC GB, No-Intro GBC]`、`gbc/` 那张 `[No-Intro GBC, TOSEC GB]`，红；改后两张一样，都是中置信。`identify::rank` 收的是
  `platform_of` 那一次的答案（`assemble` 里问一次，文件名那一层、排序、落库三处同用）。`has_ammo` / `has_sha1_ammo` 照 `Q605` 原条目的建议留着。
- [x] 两份报告头都有那一行口径（命令行输出里看得到）
  —— 一句话住一处 `report::PLATFORM_BASIS`，两份 `render_text` 头上印「平台口径        按识别判定的平台分组——……」。命令行：
  `crates/cli/tests/identify.rs::识别跑通并把命中率打出来` 与 `crates/cli/tests/scrape_profile.rs::不给档案就是离线档_一个网络请求都不发`
  断言 `romcat identify` / `romcat scrape` 的 stdout 里有这一行（基点上红）；核心库那两条报告测试断言它在报告头（第一个空行之前）。
- [x] 门禁全绿（本机不分大小写的盘上 `sync_run` 那五条已知红除外，`Q479`）
  —— `cargo xtask gate --keep-going -j 3 --test-threads 3`，日志 `/Users/nicoer/dev/game-wt/logs/q8-cao-02-gate1.log`
  （开头两行是 `/Users/nicoer/dev/game-wt/slot3`、`q8/cao-02`），末行 `EXIT=0`，汇总「7 条全绿」：fmt / glossary / check /
  clippy / test（1925 秒，`--no-fail-fast`，通过 2,688、红 0、挂起 2）/ numbers / doc。括号里那条已知红已经过时：本机默认盘上
  门禁就绿，那五条如实跳过（`docs/agents/long-jobs.md`）。截图基线一张没变。README 的票数与测试条数在本分支上
  `cargo xtask numbers --write` 写回过（167 → 168 张、2,686 → 2,690 条）。
