# 15: ScreenScraper 账号落盘（0600）

**What to build:** 设置屏可以填 ScreenScraper 账号，存工作目录里一份**只本机本人可读**（0600）的文件，只一套。核心库一处读写，界面与命令行都走它；读取顺序：环境变量优先，其次那份文件。
「测试连接」不在这张票。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q1063`：见 `../grill.md` 里那一条。

收差距（`../../gui-draws-the-rest-of-the-design/gaps-scrape-dialog.md`，2026-10-02 盘点时撞见）：设置屏 `settings.rs` 约 657 行那句说服务端配额「写在刮削面板上」，可刮削面板从来没画过服务端配额——改成实话（配额在哪看得到，或者照实说眼下看不到）。另：刮削弹层没账号时要指向设置屏（gui-draws 票 17 的 `F-7`），那一问「有没有账号」由本票立的那一处答。

**Blocked by:** None (can start immediately)

**Status:** done

账号的读写只在一处：`crates/core/src/scrape/online/account.rs`（由 `romcat_core::scrape::online` 重新导出）。`find_account(工作目录)` 环境变量优先、其次工作目录里的 `screenscraper.toml`（`workspace::screenscraper_account_path`），两边不拼；`saved_account` 只读文件那一套（设置屏摆回四格用）；`save_account` 写，Unix 上先收 0600 再写字、只装得下一套、四格全空就删掉文件；**「有没有账号」是 `online::has_account(工作目录)`**——环境变量或文件哪一条都算，给 gd-17 的 `F-7` 直接调。原来的 `Credentials::from_env` 删了，命令行 `romcat scrape`、刮削面板、设置屏一律走这一处。Windows 上收不了权限，照实写在 `online::WHO_CAN_READ`（屏上、命令行、文件开头说的是同一句）。「测试连接」照裁定不做。主库一个字节都不碰：账号文件落在工作目录，测试全在临时目录。

- [x] 先写红测试（核心库）：写出账号 → 文件权限是 0600（Unix）→ 读得回
  - 证据：`crates/core/tests/screenscraper_account.rs`（新，12 条）的 `写出账号之后读得回_文件只有本人读得了`（权限 `& 0o777 == 0o600`，`find_account_with` 读回 `(那一套, AccountFrom::File)`，`saved_account` 也读回同一套）。同文件还钉着：`原先那份文件谁都读得了_重写之后收成只本人读得了`（先摆一份 0644 的，存一次变 0600）、`只存一套_再存一次就换掉上一套`、`文件里摆着第二套就读不懂_不挑一套将就着用`、`开发者那两样缺一样就存不进去`（也不留下文件）、`四样都清空再存就是不留账号`、`有没有账号_文件那一条也算`、`账号拼成的凭据带着那一套_空着的两样不发`、`账号印进日志时不露密码`、`文件读不懂时报错不带那一行原文_密码不跟着报错出去`。
  - 红在哪：基点 `c986470` 上这份测试编不过（`online::Account`、`save_account`、`workspace::screenscraper_account_path` 都不在）。最后那条是 /code-review 的 Spec 轴抓出来的真泄露——TOML 报错会把出错那一行原文引出来，人手写漏了引号的那一行就是密码本身；先写了这条、确认它红（报错里印着 `sspassword = 编的密码丁`），再改成只报第几行，转绿。
- [x] 设了环境变量时环境变量优先
  - 证据：核心库 `设了环境变量时环境变量优先`（文件里存着甲、环境变量给了丙 → 读到丙、来自 `Env`，一样都不从文件里补）与 `环境变量只给了半套时不算_退到文件那一套`（只给 `SCREENSCRAPER_SSID` → 整套用文件的，不拼）；两条走 `find_account_with` 递一份假环境变量——工作区禁 `unsafe`，测试里设不了真环境变量。端到端那一半在命令行：`crates/cli/tests/scrape_profile.rs` 的 `在线档设了环境变量时用环境变量那一套`（子进程真给了 `SCREENSCRAPER_DEVID` / `DEVPASSWORD`，工作目录里也存着一套，stderr 说「用的是开工具之前给的环境变量」）。
- [x] 界面测试：设置屏填账号 → 关窗再开还在；密码格不明文回显
  - 证据：`crates/gui/tests/settings.rs` 的 `设置屏填好账号_关窗再开还在_密码格不明文回显`：四个框里打字、按「保存」，屏上从「账号没给」变「账号已给」，`online::saved_account` 读回的就是那一套；丢掉这扇窗、在同一个工作目录上重开一扇，开发者标识与用户名两格照旧摆着，两格密码的原文在打字时、存后、重开后都不在屏上画出来的字里（egui `TextEdit::password` 画、读屏、复制出去的都是圆点）。另两条：`开发者那两样缺一样时存不进去_说清为什么`（只填用户名按保存 → 屏上说 403、文件不落）、`配额那句话说实话_不说写在刮削面板上`。几条都 `ignore_env`：这台机器上真给没给环境变量不许左右它们。
  - 基点上编不过（`settings::ACCOUNT_FIELDS`、`Screen::ignore_env` 都不在）。
- [x] 命令行刮削能用文件里的账号（命令行测试，不联网：断言它认到了账号）
  - 证据：`crates/cli/tests/scrape_profile.rs` 的 `在线档认得工作目录里存着的账号_一个请求都不发`：走核心库那一处存一套编出来的账号、摘掉四个环境变量，`romcat scrape --profile 在线 --online-budget 0` 起得来，stderr 说「ScreenScraper 账号：用的是工作目录里存着的那一套」，报告头是「刮削：在线档」，密码不在 stdout 也不在 stderr。**不联网**：`--online-budget 0` 让自设上限在发第一个请求之前就收手（`Net::charge` 在 `fetcher.get` 之前），而这份小库里本来就没有撞过 DAT 的条目；手跑一趟核过报告里「请求 0 个」。原来那条 `在线档缺凭据时不硬闯_也不悄悄退回离线` 照旧绿（stderr 仍说 `SCREENSCRAPER_DEVID`、论坛人工申请、不要拿别人的 devid 用）。
- [x] 设置屏截图基线重批
  - 证据：`UPDATE_SNAPSHOTS=1 CARGO_INCREMENTAL=0 cargo test -p romcat-gui --all-features --test snapshot -j 3 -- --test-threads 3`，日志 `/Users/nicoer/dev/game-wt/logs/q8-vs-15-snap-update1.log`，开头两行是本树与 `q8/vs-15`，末行 `EXIT=0`，137 条全过。137 张里**变了 2 张、新增 2 张，别的一张没动**：变了 `settings/sources-{light,dark}`（ScreenScraper 那一格多了四格与「保存」、两句说明换成实话），新增 `settings/sources-account-{light,dark}`（工作目录里存着一套时：两格摆着、两格密码画成圆点——圆点那个字形画不画得出来，读字的测试验不到）。并排图（旧｜新｜稿）与每张为什么变：`/Users/nicoer/dev/game-wt/logs/q8-vs-15-compare/`（`CHANGED.md`）。
  - 保留：四格怎么排（稿上只画了用户名与密码一列两格）是挂单 `Q1687`，待拿主意的人裁；样式上拿不准的几处列在 `CHANGED.md` 末尾。
  - 令牌：新立 `settings-account-label = 70`（稿上这一格 `.frm` 的名一列），`check_tokens.py` 加了一条核对（故意把令牌改成 71 跑过，它报「设计稿是 70，令牌是 71」）。收尾跑 `python3 .scratch/gui-looks-like-the-design/check_tokens.py`：只报基点上就有的两条（`size-title` 稿 17 令牌 15、视频播放标 shade），与改前逐字一样，没有新的不一致、没有没人核的令牌。
- [x] 门禁全绿（本机 `sync_run` 那五条已知红除外，`Q479`）
  - 证据（2026-10-03，本分支工作树，提交之前）：`CARGO_INCREMENTAL=0 cargo xtask gate --keep-going -j 3 --test-threads 3`，日志 `/Users/nicoer/dev/game-wt/logs/q8-vs-15-gate1.log`，开头两行是 `/Users/nicoer/dev/game-wt/slot3` 与 `q8/vs-15`，末行 `EXIT=0`。汇总表 fmt、glossary（扫了 `crates/` 下 12 份 `.rs` 新写的 1,097 行，没跳过、没撞上）、check、clippy、test（1,044 秒，带 `--no-fail-fast`，2,795 条通过、0 条失败、2 条 `#[ignore]` 的量级测量）、numbers、doc，**7 条全绿**。括号里「五条已知红除外」那句已经过时：本机默认盘上门禁就是绿的，`sync_run` 一条没红。
  - `numbers` 先在本分支 `cargo xtask numbers --write` 写回过两趟（`q8-vs-15-numbers1.log`、`numbers2.log`，都是 `EXIT=0`）：测试条数 2,778 → 2,797、测试目标数 89 → 90（多了 `screenscraper_account` 这一份），翻了本票的 `Status:` 之后票数 178/210 → 179/210。
  - 改过的文件里复述的真库数照「真库数只住台账」换成了量级词并指台账（`crates/core/src/scrape.rs` 七处、`crates/core/src/scrape/online.rs` 三处，/code-review 的 Standards 轴点出来的）；issue-tracker.md 那条粗筛 grep 对本票改过的文件跑出 0 行。
- [x] 收挂单 `Q1063`：账号那一半照 grill 的裁定做完，`../grill.md` 那一条已标 settled。
  - 保留：`Q1063` 标题里「两条配额留不住」那一半本票没做——只照票末那一行把设置屏那句话改成实话；留不留、怎么画记在挂单 `Q1688`。「测试连接」照裁定另议。
- [x] 收差距（票末追加那一行）：设置屏那句配额的话改成实话；「有没有账号」立好了。
  - 证据：设置屏那句现在是「服务端报的两条配额（今日请求、今日「未识别 ROM」）只在一趟联网刮削跑着的时候才有，界面上眼下哪儿都看不到，这儿也留不住；命令行刮削的报告里印着」（测试 `配额那句话说实话_不说写在刮削面板上`；命令行报告「在线那一侧的账」一节印着「服务端说的剩余」，`crates/core/src/scrape/report.rs`）。「有没有账号」是 `romcat_core::scrape::online::has_account(&工作目录)`（测试 `有没有账号_文件那一条也算`）。
  - 保留：刮削弹层上「没有账号」那一行本身是 gd-17 的 `F-7`，本票没画。刮削面板按「开始刮削」拒下的那句因为 `from_env` 删了不得不动，眼下写的是「在设置屏『数据源』那一节填一次，或者开工具之前给环境变量 …」，gd-17 照 `F-7` 接着改。

## Comments

- 挂单（2026-10-03）：`Q1687`（四格两行两列 vs 稿上一列两格）、`Q1688`（配额留不住那一半的去处）、`Q1689`（读文件不看权限）、`Q1690`（词表里没有「账号」）。
