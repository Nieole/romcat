# 10: 数据源许可与状态词由核心库给

**What to build:** 数据源名册带上**许可**字段（数据写在数据源清单里），设置屏「关于」逐个印出来，不再说「以各家自己的说明为准」。
数据源状态的「未下载 / 读不动」由核心库给一个词，设置屏与库屏都读它。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q1067`：见 `../grill.md` 里那一条。
收挂单 `Q1074`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 先写红测试：设置屏「关于」里每个数据源都有许可那一格
  —— `crates/gui/tests/settings.rs::关于那一节每个数据源都有许可那一格`：名单得是数据源清单里那几份 DAT 的源加中文离线源、
  Switch 数据库、ScreenScraper，每一家在「关于」那一节上单独一格写着名字，同一行右边一格写着核心库给的那一句许可；整节不许再有
  「以它们自己的说明为准」。头一版只用今天就有的接口（名册的名字、`Source::label`、`SCREEN_SCRAPER`）写，在 `1c34ce3` 上跑出红
  （日志 `/Users/nicoer/dev/game-wt/logs/q8-cao-10-red1.log`，`EXIT=101`，红在「还在说『以它们自己的说明为准』」）；之后改成逐行对
  `sources::licenses()`。核心库那一边：`sources::tests::关于那张名单上每个数据源都写着许可`（名单与次序、每家非空、ScreenScraper
  「CC BY-NC-SA 4.0」、中文离线源「CC BY-SA」、Switch 数据库「MIT」、MAME「CC0-1.0」照调研原文）。名单由核心库
  `sources::licenses()` 给一次（ADR-0024）；「关于」那一行改成一家一行，设置屏头一回翻到那一节时问一次、不每帧问。
  另三家的许可住在各自模块里（`zh::LICENSE`、`titledb::LICENSE`、`scrape::online::LICENSE`），挂单 `Q1388`。
  `romcat dat sources` 也逐家印「许可 …」（`crates/cli/tests/dat.rs::数据源清单看得见也导得出` 加断言，加之前红）。
- [x] 名册里每个数据源都填了许可（缺一个就红的测试）
  —— `registry::Source` 加 `license`，数据写在 `crates/core/src/dat/sources.toml` 各家的「许可」上（出处写在那一条上面的注释里：
  调研 `docs/research/scraper-sources.md` §8.1–8.4；GoodNES 那一句是 2026-10-04 核 BizHawk 仓库根上 LICENSE 原文定的）。
  `registry::tests::内置清单里每个源都写着许可` 缺一家就红（先写，在加字段之前编不过）。自己改的清单不写这一项照旧读得动，
  印出来是「清单里没写」（`registry::tests::自己改的清单不写许可也读得动`、`Source::license_or_unnoted`），挂单 `Q1387`。
- [x] 设置屏与库屏不再各自写状态词
  —— 核心库立 `SourceState::label`（没下载「未下载」、读不动「读不动」、下载了的那一档 `None`）与量列宽用的 `SourceState::LABELS`，
  `sources::tests::没下载与读不动那两档屏上叫什么由这里给一次` 钉着（先写，编不过）。设置屏 `条数` 与库屏数据源那张表改读它，
  两处的 `"未下载"` / `"读不动"` 字面量删掉；库屏那张表顺手收成一处画法（记录数那一格从词或条数里取，颜色照旧跟竖条）。
  两屏各一条：`crates/gui/tests/settings.rs::数据源读不动时设置屏那一格写核心库给的词`（工作目录、数据源两节）、
  `crates/gui/tests/roots.rs::读不动的数据源那一格写核心库给的词_说明照印原话`。**保留：** 这两条在基点上换成那两个字面量也是绿的
  ——屏上的字本来就一样，钉的是往后两屏读同一处；「一处」本身由核心库那条单元测试与两屏源码里不再有那两个字面量作证。
- [x] 设置屏截图基线重批
  —— 只变了 `settings/about-light`、`settings/about-dark` 两张：整个截图门先不带更新跑过一趟（`q8-cao-10-snapshot1.log`），
  139 张里只红这两张，库屏那张一张没变。并排图（旧｜新｜稿，稿用 ego-browser 截 `prototype.html`）与每张为什么变、样式上拿不准的
  四问（许可放哪一行、许可那一列的颜色、ScreenScraper 的「可选」标、分不分组，都先照推荐做了）在
  `/Users/nicoer/dev/game-wt/logs/q8-cao-10-compare/`（`CHANGED.md`）。树上不留 `.new.png` / `.diff.png` / `.old.png`。
- [x] 门禁全绿（本机不分大小写的盘上 `sync_run` 那五条已知红除外，`Q479`）
  —— `CARGO_INCREMENTAL=0 cargo xtask gate --keep-going -j 3 --test-threads 3`，日志 `/Users/nicoer/dev/game-wt/logs/q8-cao-10-gate2.log`
  （开头两行是 `/Users/nicoer/dev/game-wt/slot1`、`q8/cao-10`），末行 `EXIT=0`，汇总「7 条全绿」：fmt / glossary / check / clippy /
  test（1267 秒，`--no-fail-fast`，通过 2,832、红 0、挂起 2——那两条是量级测量）/ numbers / doc。括号里那条已知红已经过时：本机默认盘上
  门禁就绿。README 的票数与测试条数在本分支上 `cargo xtask numbers --write` 写回过（182 → 183 张、2,827 → 2,834 条）。
  头一趟（`q8-cao-10-gate1.log`）起跑时盘上只剩 4 GiB，跑到 clippy 被我自己停掉，没有 `EXIT=`，不算数。

收挂单 `Q1067` `Q1074`：两条都在 `../grill.md` 各条底下标了 settled，做了什么写在那儿与上面各框里。
本票新记挂单 `Q1387`–`Q1389`（`.scratch/PARKING-LOT.md`）。
收尾时照「真库数只住台账」清了改过的文件：`sources.toml` 头注那「22 个平台」、PSP / PSV / FC 三条映射说明里的变体数，
`scrape/online.rs` 模块文档里票 07 的按平台命中率，换成量级词并指台账 `docs/library-facts.md`（台账里都核得到）。

拿主意的人 2026-10-04 看 `settings/about-{light,dark}` 并排图裁样式四问，都照推荐：许可并进「数据源」那一行（名一列、许可一列）；
许可那一列弱色；ScreenScraper 不标「（可选）」；八家平铺，不按 DAT 分组。两张基线点头。
