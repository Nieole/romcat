# 08 — 非游戏资产默认不列出

**What to build:** 维护者在浏览屏上默认看不到 BIOS 这类非游戏资产，屏上写明白有多少个
被收起；想看时一个开关就显示出来。

**Blocked by:** 无 —— 可立即开工

**Status:** done

⚠️ **行为只改「列不列出」**：它们照旧入库、永不导出。

⚠️ **界面不判「是不是非游戏资产」**（ADR-0024）：读核心库现成的那一处结论，不在浏览屏
里自己按扩展名或目录名判一遍。`one-criterion-per-thing/04` 正在把那个判断提到两侧都够得
着的一层——本票不必等它，但**必须读判断结果**，那张票做完这里自动跟着变。

- [x] 默认不列出，屏上说明白收起了多少个
  - 核心库：`crates/core/tests/works.rs::非游戏资产默认不列出_收起了几行与翻出来的表对得上`——`WorkQuery::default()` 收起，`Catalog::non_game_asset_rows` 交 4，翻出来的表里正好少那 4 行。
  - 界面：`crates/gui/tests/browse.rs::非游戏资产默认不列出_屏上说收起了几个_开关打开后列出来并标着`——默认表上 2 行，屏上画着「收起了 2 个非游戏资产」，收起的那一行没画出来。
- [x] 开关打开后列出来，并标出它们是非游戏资产
  - 同上那条界面测试：点「列出非游戏资产」之后 4 行都在，屏上「非游戏资产」标记的数目等于核心库 `WorkRow::non_game_asset` 为真的行数（2），再点一下收回 2 行。
  - `crates/core/tests/works.rs::列出来的每一行说得出它是非游戏资产_收起时一行都不标`——按每页 7 行翻完，标着的正好是那 4 行，收起时一行都不标；手工挂进作品的 BIOS 行上不标，详情里由 `WorkVariant::non_game_asset()` 标。
- [x] 计数与实际列出的行对得上（数与表一致）
  - `crates/core/tests/works.rs::非游戏资产默认不列出_收起了几行与翻出来的表对得上`：两档各按每页 7 行翻完、无重行；收起时 32 行 + 收起了 4 = 列出时 36 行；按 FC / PS / SFC 筛着也成立。左栏条数跟着开关（FC 7 / 9，「还没识别」32 / 37，挂单 `Q775`）。
  - ⚠️ 保留：按**行**数。人手工把 BIOS 挂进某部作品时，那一行两档都列着，那个 BIOS 不在「收起了几个」里（挂单 `Q772`）。
- [x] 导出行为一个字没变
  - `crates/core/src/adapter/` 相对基点 `7935488` 零改动（`git diff 7935488 -- crates/core/src/adapter` 为空）；导出那道闸 `converge::excluded` 照旧直接问 `classify::non_game_asset`、不读开关。
  - 全量门禁 test 一步里导出测试（`crates/core/tests/pegasus.rs`，含 BIOS 记成 `NotAnEntry::NonGameAsset` 的那条）照旧绿。
  - 顺带：子库规则不读开关（挂单 `Q773`），子库照规则选变体、会带上收起的 BIOS。
- [x] 界面里没有第二处「是不是非游戏资产」的判断（测试断言用的是核心库交回来的结论）
  - 判断只在 `crates/core/src/classify.rs` 的 `non_game_asset`：核心库经 SQLite 函数 `romcat_non_game_asset`（`catalog::browse::register_non_game_asset`，挂单 `Q771`）与 `WorkVariant::non_game_asset()` 问它；SQL 字符串里不写判据，函数名只在 `NON_GAME_ASSET_FN` 一处。
  - `grep -rni bios crates --include='*.rs'` 在非测试代码里除 `classify.rs` 外只剩开关悬停里一句说明文字；界面只读 `WorkRow::non_game_asset`、`WorkVariant::non_game_asset()`，屏上那个词是核心库的 `NON_GAME_ASSET_LABEL`。
  - 界面测试的断言读的是 `WorkRow::non_game_asset` 与 `NON_GAME_ASSET_LABEL`；判断挂上连接的几种开法由 `crates/core/tests/works.rs::几种开法开出来的库都问得动那一处判断` 钉着（拿掉只读入口那一句注册实测红）。

**门禁（2026-09-14）：** `/Users/nicoer/dev/game-wt/logs/slot-3-gl08-gate2.log`——fmt、glossary、check、clippy、test、doc 六条全绿，`EXIT=0`；test 一步 73 个测试目标、1992 条过、0 失败。第一趟 `slot-3-gl08-gate.log` 红在 clippy（`type_complexity`，第二趟聚合的六元组），换成 `GroupTotals` 结构体后重跑。实现提交 `d53dc6b`。挂单 `Q771`–`Q776`。
