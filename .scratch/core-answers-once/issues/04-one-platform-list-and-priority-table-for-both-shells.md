# 04: 平台清单与优先级表两个壳读同一份，读不动说出来

**What to build:** 平台清单「先 `--manifest`、再工作目录那份、再内置」那条链提进核心库一处（照数据源名册在工作目录里找的先例），命令行与界面都走它。
界面里原来一律用内置清单的那几处改调它。工作目录那份平台清单或优先级表**读不动**时，开窗屏上说出来、并说明眼下用的是哪一份，不静默退回内置（ADR-0021）。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q1031`：见 `../grill.md` 里那一条。
收挂单 `Q976`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 先写红测试：工作目录放一份与内置不同的平台清单，界面库体检用的是它（今天是内置那份）
  - 红：`cargo test -p romcat-gui --all-features --test manifest 工作目录里那份平台清单说了算` 在改之前红在
    `panicked at crates/gui/tests/manifest.rs:118: 「假想机」被当成了未纳入管理的目录——用的还是内置那份清单`
    （同一份改动把 `health::check_run` 临时改回内置那一份时又红一次，红在同一句：改完之后 `check_run` 那一路真的被钉住了）。
  - 绿：同一条今天过。断言看的是**外部行为**：`HealthReport` 里 `假想机` 那一行 `in_scope == true`（认得的平台进识别管线）、
    `report.scope.unmapped_dirs == 0`（自己的清单认得它，不该落进「未纳入管理」）。改之前那两份数反过来。
    这一条走**两条路**：扫描交回来的那份报告，以及再按一下「重新体检」之后的那一份（`health::check_run` 才是界面自己折报告的那条路）。
- [x] 命令行 `--manifest` 与工作目录那份的先后照旧（现有命令行测试照旧绿）
  - 先后的两条既有测试照旧绿：`crates/cli/tests/shape.rs::工作目录里的清单不给参数也生效`、`::清单写坏了当场报错而不是悄悄退回内置的`
    （门禁 `test` 那条，见日志 `cao-04-gate1.log`）。
  - 先后本身另有两条新测试钉着（核心库那一处）：`crates/core/tests/platform_lookup.rs::点名的那一份压过工作目录里那一份` 与
    `::工作目录里那份平台清单说了算`。命令行那一层只剩 `ManifestArgs::load` 把 `--manifest` 交进去（`Manifest::locate(...).strict()`），
    排序不再有第二份。
- [x] 界面里不再有直接取内置平台清单的地方
  - `grep -rn "Manifest::builtin\|Manifest::default" crates/gui/src` 只剩 `crates/gui/src/demo.rs` 三处：那是合成数据
    **校验变体用的清单**（不是上屏那张表，演示库压根不读工作目录里那份），注释与挂单 `Q1329` 写着。
  - 其余七处改读 `App::new` 挑好的那一份：库体检折统计与平台纠正那一层（`health`）、重新成型（`shaping`）、扫描
    （`roots`，`ScanOptions.manifest`）、筛选器「认不认得这个平台」（`browse::rule_box`）、成型存疑与平台全名
    （`browse/work`）、合并向导第二步组头（`browse/merge`，向导开的时候把那一份拿过来）。`tokens.rs` 那条单元测试也改走
    `Manifest::locate`。
- [x] 平台清单读不动 → 开窗屏上有那句话；优先级表读不动 → 同样有一句
  - `crates/gui/tests/manifest.rs::工作目录里那份平台清单读不动时开窗屏上说出来`：屏上有「平台清单读不动」「眼下用的是内置那一份」
    与那一份的完整路径。`::开窗时优先级表读不动也说出来`：屏上有「优先级表读不动」「眼下用的是内置那一份」与 `priorities.toml` 的路径。
  - 两处的话都摆在主区最上方单独一行（与「版式存不下来」同一个画法、同一处），六屏上都看得见；开窗那一屏（待确认）就有。
    两句话都带下一步：「去修那一份的权限，或者关掉占着它的程序」（ADR-0021 那条判据）。
  - 「在不在」的问法从 `Path::exists()` 换成 `try_exists()`（核心库 `Manifest::locate` 与 `sync::prepare::priorities` 两处）：
    目录读不动从此报读不动，不再报「不在」——`crates/core/tests/platform_lookup.rs::工作目录里那份读不动时不静默退回内置的`
    收掉工作目录的读权限之后，`Manifest::locate` 交得出原因（改之前这里静静换成内置那份）。
- [x] 收权限在 root 下跑不出读不动时如实跳过并印一行
  - `crates/core/src/testing.rs::revoke_file`（新加，与 `revoke` 同一条规矩）：造不出来就印 `跳过 <测试名>：…` 并交 `None`，
    三条测试都写成 `let Some(_装着) = … else { return };`，不许改成「假装读不动」。
  - 这三条在本机（非 root）是真跑过的：`cargo test … -- --nocapture` 那一趟**没有**「跳过」那一行。
- [x] 门禁全绿（本机不分大小写的盘上 `sync_run` 那五条已知红除外，`Q479`）
  - 日志：`/Users/nicoer/dev/game-wt/logs/cao-04-gate4.log`（开头两行是本 worktree 与本分支），末行 `EXIT=0`，七条全绿：
    `绿 fmt 2s` / `绿 glossary 2s` / `绿 check 0s` / `绿 clippy 0s` / `绿 test 1079s` / `绿 numbers 3s` / `绿 doc 4s`，末行上一句是「7 条全绿。」。
  - `sync_run` 那一组 57 条全过：本机那块不分大小写的盘上，只在分大小写的盘上摆得出来的那五条走的是
    `crates/core/tests/sync_run.rs` 里 `跳过：…`（eprintln，`--nocapture` 才看得见）那一档，门禁里记成通过（`Q479`）。
  - 更早那两趟：`cao-04-gate1.log` 红了两条（fmt、doc：长行没折、`demo.rs` 里我插的辅助函数把 `synthetic` 的文档注释顶走了、
    命令行那条文档链接指不到核心库里的 `Choice`），都改掉了；`cao-04-gate3.log` 跑到 test 那一段被截断（没有 `EXIT=` 那一行，
    不当绿也不当红），于是重跑了 gate4。

## 做了什么（留给复核）

- 核心库：`platform::Manifest::locate(given, workspace)` 一处挑清单，交回 `platform::Choice`（眼下用的那一份、`Origin` 说它从哪儿来、
  想用的那份读不动时的 `Unreadable`）。`Choice::unreadable_note()` 是屏上那句，`Choice::strict()` 是**没处摆这句话**的调用方要的那一份
  （命令行停下、`sources::manifest` 照旧报错）。`Manifest::load` / `builtin` / `builtin_text` 原样不动。
- 界面：`App::new` 挑一次（`Manifest::locate(None, &workspace)`），把那一份交给库屏与浏览屏；挑的过程中读不动就记一句到
  `App::notices`，开窗屏最上方画出来。优先级表同一处、同一个画法。
- 优先级表：`sync::prepare::priorities` 的问法换成 `try_exists`，说不出来在不在就按读不动办。返回类型不动（仍是 `Result<_, String>`），
  所以命令行与别处几条路一行没改。

## Comments

- 与规格第一组「平台清单的查找链提进核心库一处……读不动交第三态」、第三组「开窗时优先级表读不动：屏上说出来，不静默用内置那份」
  逐条对上；ADR-0024（一处实现）、ADR-0021（读不动是第三态）、ADR-0023 修订段（屏上的话只说事实与去处）都照做了。
- 新挂单：`Q1327`（工作目录里另外那几份配置的「在不在」还是 `Path::exists()`：`sources::rules` / `load_registry` /
  `Roster::in_workspace`）、`Q1328`（那两份配置只在开窗那一刻读一次，外面改了要重开窗）、`Q1329`（`demo.rs` 那三处
  `Manifest::default()` 是有意留的）。`Q1327` 与 `Q1328` 都是没为它们停线的东西。
- 截图基线：**一张没变**（`tests/snapshot.rs` 159 条照旧绿，一条基线都没动）——两句话只在读不动时才有，夹具里没有那种工作目录。
- 门禁里 `numbers` 那条把 README 的两个数写回了：测试条数 2,995、测试目标数 92（这张票新加 4 + 3 条）。
