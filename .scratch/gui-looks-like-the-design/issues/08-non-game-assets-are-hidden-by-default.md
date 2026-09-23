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

## 挂单裁决（第五轮收口，2026-09-23）

从 `.scratch/PARKING-LOT.md` 迁来（`/settle`，范围 `Q449`–`Q1185`）。每条顶上多一行**裁决**：落哪一档、去了哪儿。
编号 `Qn` 留着占号、不复用。

### Q771 — 非游戏资产那一处判断怎么进得了 SQL 的 `WHERE`：挂成 SQLite 函数，不物化成一列

- **裁决（第五轮收口，2026-09-23）：** **岔** —— 实现走的就是条目建议的那条；追认：维持现状（2026-09-23 拿主意的人整批追认）。

- **来自：** 票 `gui-looks-like-the-design/08`
- **类别：** 规格没说
- **在哪：** `crates/core/src/catalog/browse.rs` `register_functions` 与 `NON_GAME_ASSET_FN`（`VariantQuery::where_clause`、`Catalog::non_game_asset_rows`、`work_page_totals` 问它）；`crates/core/src/catalog.rs` `prepare` 与 `read_only_at` 各挂一次；根 `Cargo.toml` 的 rusqlite 开 `functions`（不带新依赖，`Cargo.lock` 没变）
- **为什么没停线：** 两条路都不升中立库结构版本；推翻它换的是 `WHERE` 那一句与两处注册，出不了这张票。
- **这张票实际做了什么：** 把 `classify::non_game_asset` 原样注册成 `romcat_non_game_asset(键)`（确定性函数），收起那一档在 `WHERE` 里问它；数总数、翻页、详情、批量操作的作用范围共用那一份筛选，SQL 里一个字的判据都不写。钉它的是 `crates/core/tests/works.rs` 三条：`非游戏资产默认不列出_收起了几行与翻出来的表对得上`、`列出来的每一行说得出它是非游戏资产_收起时一行都不标`、`几种开法开出来的库都问得动那一处判断`（建库、打开、分出只读、只读打开、内存库各问一遍；实测拿掉只读入口那一句注册，它当场红在「no such function」上）。
- **另一条路：** 物化成一列（ADR-0024 推论 3 允许）：成型写变体时判一次落进 `variant` 表，`add_columns` 纯加列、开库时给老库补上（先例是 `identification.standalone`），浏览读列。
- **建议留哪条：** 留函数。判断只看键、现问不贵；物化要动变体那张表的写入口、给老库整表补一遍，判据哪天改了还要等下一趟扫描才跟上。代价有两样：收起那一档（默认）数总数时每个变体都要回表取键再问一次，原先不筛时多半只扫索引就够——真库量级上慢多少**没量**（`--bench-paging` 要 release 构建）；拿 `sqlite3` 直接开库的人问不了这个函数。
- **谁来裁：** 拿主意的人
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q772 — 「收起了几个」按行数算，不按变体数

- **裁决（第五轮收口，2026-09-23）：** **岔** —— 实现走的就是条目建议的那条；追认：维持现状（2026-09-23 拿主意的人整批追认）。

- **来自：** 票 `gui-looks-like-the-design/08`
- **类别：** 规格没说
- **在哪：** `crates/core/src/catalog/browse.rs` `Catalog::non_game_asset_rows`（`HAVING MIN(…)`）与 `WorkRow::non_game_asset`；`crates/gui/src/browse.rs` `non_game_asset_label`
- **为什么没停线：** 两种数法只差一句 SQL 与屏上那句话；真库上两者相等（识别挑作品时跳过非游戏资产，它们各自成一行），只在人手工把 BIOS 挂进某部作品时分开。
- **这张票实际做了什么：** 数「这批筛选下底下变体全是非游戏资产的行」，于是「收起时的行数 + 收起了几个 = 列出时的行数」严格成立，翻页也对得上。一部作品底下夹着一个手工挂进来的 BIOS：那一行两档都列着，收起时变体数少一个；那个 BIOS 不在「收起了几个」里、行上不标，列出时在详情面板里标。钉它的是 `crates/core/tests/works.rs::非游戏资产默认不列出_收起了几行与翻出来的表对得上`（「魂斗罗」那一段）。
- **另一条路：** 按变体数：数这批筛选下的非游戏资产变体，手工挂进作品的也数进去；代价是那种情况下「行数差」与「收起了几个」不相等。
- **建议留哪条：** 留行数。票的验收是「计数与实际列出的行对得上」，屏上收起的单位是行；分开的那种情况只有人亲手造得出来，那时人知道它在哪。
- **谁来裁：** 拿主意的人
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q773 — 「列出非游戏资产」那颗开关不进子库的规则

- **裁决（第五轮收口，2026-09-23）：** **岔** —— 实现走的就是条目建议的那条；追认：维持现状（2026-09-23 拿主意的人整批追认）。

- **来自：** 票 `gui-looks-like-the-design/08`
- **类别：** 规格没说
- **在哪：** `crates/core/src/catalog/browse.rs` `WorkQuery::to_rule`（不读 `non_game_assets`）与 `WorkQuery::non_game_assets` 的文档
- **为什么没停线：** 本票只改列不列出；不进规则就是规则那一侧一个字没改，推翻它是往上加东西（一条 `Unruly` 或规则语言的一维），不拆本票落地的活。
- **这张票实际做了什么：** `to_rule` 不读开关，与排序同一待遇。于是默认收起时按「存成子库」，子库照规则选变体，**会带上屏上收起的那几个 BIOS**（同步照拷，导出照旧不成条目）。钉它的是 `crates/core/tests/works.rs::列出来的每一行说得出它是非游戏资产_收起时一行都不标` 末尾那句 `to_rule` 相等。
- **另一条路：** 规则语言加一维「非游戏资产=是/否」，收起时折进规则，子库选出来的与屏上一个不差——「筛出来的这一批」与「导过去的那一批」对得上那条约定（挂单 `Q70` 的道理）。
- **建议留哪条：** 留不进。掌机上的模拟器照样要 BIOS，子库把它们拷过去是对的；加一维要动规则语言、子库事实与解析。代价：存子库那一刻，屏上列着的比子库会选的少那几个，屏上没有一句话提醒。
- **谁来裁：** 拿主意的人
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q774 — 左栏「全清」把非游戏资产那颗开关也拨回收起

- **裁决（第五轮收口，2026-09-23）：** **岔** —— 实现走的就是条目建议的那条；追认：维持现状（2026-09-23 拿主意的人整批追认）。

- **来自：** 票 `gui-looks-like-the-design/08`
- **类别：** 规格没说
- **在哪：** `crates/gui/src/browse.rs` `Screen::clear_filter`（`..WorkQuery::default()`）；`Screen::begin_editing` 同理
- **为什么没停线：** 推翻它是 `clear_filter` 里多留一个字段，一行。
- **这张票实际做了什么：** 没改：「全清」照旧拿默认查询兜底，开关跟着回到收起；从子库屏「改选择」跳回来也是收起。没有测试单独钉。
- **另一条路：** 开关与排序、搜索框同一待遇，全清时留着。
- **建议留哪条：** 留现在这样。开关摆在筛选栏里、`WorkQuery::same_filter` 算它是筛选，「全清」的承诺是回到默认；日后这一屏重排若把开关挪出筛选栏，再改成留着。
- **谁来裁：** 拿主意的人
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q775 — 左栏那几档的条数跟着「列出非游戏资产」那颗开关走

- **裁决（第五轮收口，2026-09-23）：** **岔** —— 实现走的就是条目建议的那条；追认：维持现状（2026-09-23 拿主意的人整批追认）。

- **来自：** 票 `gui-looks-like-the-design/08`（规格审查抓出来的）
- **类别：** 规格没说
- **在哪：** `crates/core/src/catalog/browse.rs` `Catalog::facets`（收一个 `NonGameAssets`）；`crates/gui/src/browse.rs` `Screen::reload_facets` 与 `sync_window` 开头那一句
- **为什么没停线：** 两条路都只动左栏那几条 `GROUP BY` 与界面重问的时机，出不了这张票。
- **这张票实际做了什么：** `facets` 收开关；收起那一档五条查询都只数不是非游戏资产的，「还没识别」拿来减的总数也是同一档（第一版漏了这一句：总数不数 BIOS、结论数却数，这一档会少）。界面拨开关时重问一次。钉它的是 `crates/core/tests/works.rs::非游戏资产默认不列出_收起了几行与翻出来的表对得上` 末尾（FC 收起 7 / 列出 9，「还没识别」32 / 37）与 `crates/gui/tests/browse.rs::非游戏资产默认不列出_屏上说收起了几个_开关打开后列出来并标着`（PS 1 / 2，街机没有 / 1）。
- **另一条路：** 左栏条数照旧数全库——它本来就不跟着别的几维收窄，是「摸清库里有什么」用的；收起的那几个由开关底下那句话交代。
- **建议留哪条：** 跟着走。人头一眼看的是默认那一屏（一档都没选），左栏写「PS 7」、点进去筛出 6 个，是这一屏自己说两个数；别的几维不互相收窄是另一回事，选了一档再看别的档本来不承诺对得上。代价：拨一下开关多问一趟那五条查询。
- **谁来裁：** 拿主意的人
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q776 — 变体那一层的查询默认全列，只有主列表默认收起

- **裁决（第五轮收口，2026-09-23）：** **岔** —— 实现走的就是条目建议的那条；追认：维持现状（2026-09-23 拿主意的人整批追认）。

- **来自：** 票 `gui-looks-like-the-design/08`（规格审查抓出来的）
- **类别：** 规格没说
- **在哪：** `crates/core/src/catalog/browse.rs` `impl Default for VariantQuery`（`non_game_assets: Listed`）与 `NonGameAssets` 的文档；拿 `VariantQuery::default()` 的几处：`crates/core/src/workspace.rs` `facts_of`、`crates/core/src/scrape/estimate.rs` `estimate`、`crates/gui/src/main.rs` 字体自检、`crates/gui/src/bench.rs` `paging`
- **为什么没停线：** 推翻它是改一个 `Default`、那几处各写一行，出不了这张票。
- **这张票实际做了什么：** `NonGameAssets` 与 `WorkQuery` 默认收起；`VariantQuery` 手写 `Default`、默认全列，于是那几处「库里一共几个变体」一个都没变，那几个文件一行没动。钉它的是 `crates/core/tests/works.rs::非游戏资产默认不列出_收起了几行与翻出来的表对得上` 里那两句 32 / 37。
- **另一条路：** 两层都默认收起，那几处调用方各自亲手写「列出」。第一版就是这么做的，只改了开场那一处、漏了刮削估算、字体自检、测速三处——规格审查抓出来的。
- **建议留哪条：** 现在这样。那几处问的都是库有多大，漏写一处就是一个悄悄少掉几个的数，没有一处会说话；变体那一层眼下没有一屏在浏览，日后真有一屏拿它浏览，默认多列几个 BIOS 是看得见的。代价：同一个开关两层默认不同，要读 `Default` 那一段才知道。
- **谁来裁：** 拿主意的人
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）